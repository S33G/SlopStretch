use super::*;
use std::io::{Cursor, Seek, Write};

pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<[f32; 2]>, String> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|e| format!("Cannot read that WAV: {e}"))?;
    let spec = reader.spec();
    if !(1..=2).contains(&spec.channels) {
        return Err("Please use a mono or stereo WAV.".into());
    }
    if !(8000..=192000).contains(&spec.sample_rate) {
        return Err("WAV sample rate must be between 8 and 192 kHz.".into());
    }
    if reader.duration() == 0
        || reader.duration() as u64 > spec.sample_rate as u64 * MAX_SOURCE_SECONDS as u64
    {
        return Err("WAV source must be between one frame and 30 seconds long.".into());
    }
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            if spec.bits_per_sample != 32 {
                return Err("Use 32-bit float or PCM WAV audio.".into());
            }
            reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        }
        hound::SampleFormat::Int => {
            if ![8, 16, 24, 32].contains(&spec.bits_per_sample) {
                return Err("Use 8, 16, 24 or 32-bit PCM WAV audio.".into());
            }
            let scale = 2.0_f64.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|value| value.map(|v| (v as f64 / scale) as f32))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        }
    };
    if raw.is_empty()
        || !raw.len().is_multiple_of(spec.channels as usize)
        || raw.iter().any(|x| !x.is_finite() || x.abs() > 16.0)
    {
        return Err("WAV contains invalid or incomplete samples.".into());
    }
    let input_frames = raw.len() / spec.channels as usize;
    let frames = ((input_frames as f64 * SOURCE_SAMPLE_RATE as f64 / spec.sample_rate as f64)
        .round() as usize)
        .clamp(1, MAX_SOURCE_FRAMES);
    let mut result = Vec::with_capacity(frames);
    for i in 0..frames {
        let pos = i as f64 * spec.sample_rate as f64 / SOURCE_SAMPLE_RATE as f64;
        let first = (pos.floor() as usize).min(input_frames - 1);
        let next = (first + 1).min(input_frames - 1);
        let fraction = (pos - first as f64) as f32;
        result.push(std::array::from_fn(|channel| {
            let ch = channel.min(spec.channels as usize - 1);
            let a = raw[first * spec.channels as usize + ch];
            a + (raw[next * spec.channels as usize + ch] - a) * fraction
        }));
    }
    Ok(result)
}
struct RenderGuard<'a>(&'a SlopControls);
impl Drop for RenderGuard<'_> {
    fn drop(&mut self) {
        self.0.render_state.store(0, Ordering::Release);
    }
}
fn validate(seconds: f32, params: &RenderParams, limit: f32) -> Result<u64, String> {
    if !seconds.is_finite() || seconds <= 0.0 || seconds > limit {
        return Err(format!(
            "Choose a duration greater than zero and at most {limit} seconds."
        ));
    }
    if !(8000..=192000).contains(&params.sample_rate) {
        return Err("Export sample rate must be between 8 and 192 kHz.".into());
    }
    for (i, value) in params.values.iter().enumerate() {
        if !value.is_finite() || PARAMETERS[i].clamp(*value) != *value {
            return Err("Export parameters are invalid.".into());
        }
    }
    let frames = (seconds as f64 * params.sample_rate as f64).round() as u64;
    if frames == 0 || frames * 8 + 64 > u32::MAX as u64 {
        return Err(
            "That export exceeds the 4 GiB WAV limit. Reduce its duration or sample rate.".into(),
        );
    }
    Ok(frames)
}
impl SlopControls {
    /// Offline helper for previews/tests. Output is stereo 32-bit float WAV and
    /// uses the same DSP as native processing. This allocates; never call in process.
    pub fn render_wav(&self, seconds: f32, params: RenderParams) -> Result<Vec<u8>, String> {
        let frames = validate(seconds, &params, MAX_RENDER_SECONDS)?;
        let mut bytes = Vec::with_capacity((frames * 8 + 64) as usize);
        self.render_to_writer(Cursor::new(&mut bytes), frames, params)?;
        Ok(bytes)
    }
    /// Stream a long render to a newly created file. Existing files are preserved.
    /// Cancellation or a write failure removes this operation's partial output.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_wav_to_path(
        &self,
        path: &std::path::Path,
        seconds: f32,
        params: RenderParams,
    ) -> Result<(), String> {
        let frames = validate(seconds, &params, MAX_FILE_RENDER_SECONDS)?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("Cannot create WAV (choose a new filename): {e}"))?;
        let result = self.render_to_writer(std::io::BufWriter::new(file), frames, params);
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }
    fn render_to_writer<W: Write + Seek>(
        &self,
        writer: W,
        frames: u64,
        params: RenderParams,
    ) -> Result<(), String> {
        self.render_state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "A bake is already running.".to_string())?;
        let _guard = RenderGuard(self);
        self.render_progress.store(0, Ordering::Relaxed);
        let source = self.copy_source()?;
        let mut writer = hound::WavWriter::new(
            writer,
            hound::WavSpec {
                channels: 2,
                sample_rate: params.sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .map_err(|e| e.to_string())?;
        let mut engine = dsp::Engine::new();
        let mut dry_engine = dsp::Engine::new();
        let gain = 10.0_f64.powf(params.values[6] / 20.0);
        let values = params.values;
        let mut dry_values = values;
        dry_values[0] = 1.0;
        dry_values[1] = 0.0;
        dry_values[5] = 0.0;
        for frame in 0..frames {
            if frame % 512 == 0 {
                if self.render_state.load(Ordering::Acquire) == 2 {
                    return Err("Bake cancelled. The slop lives to feel another day.".into());
                }
                self.render_progress
                    .store((frame as f32 / frames as f32).to_bits(), Ordering::Relaxed);
            }
            let audio = engine.next(
                source.as_slice(),
                source.len(),
                &values,
                params.sample_rate as f64,
                None,
            );
            // An offline export has no host input: its dry signal is the selected
            // source at its original speed, looping over the same region.
            let dry = if values[9] < 1.0 || values[10] >= 0.5 {
                dry_engine.next(
                    source.as_slice(),
                    source.len(),
                    &dry_values,
                    params.sample_rate as f64,
                    None,
                )
            } else {
                [0.0; 2]
            };
            // Ten-millisecond edge fades avoid exporting a hard cut through a wave.
            let edge = (params.sample_rate as f64 * 0.01)
                .min(frames as f64 / 2.0)
                .max(1.0);
            let fade =
                ((frame as f64 / edge).min((frames - 1 - frame) as f64 / edge)).clamp(0.0, 1.0);
            for ch in 0..2 {
                let mixed = if values[10] >= 0.5 {
                    dry[ch] as f64
                } else {
                    audio[ch] as f64 * gain * values[9] + dry[ch] as f64 * (1.0 - values[9])
                };
                let sample = (mixed * fade).clamp(-1.0, 1.0) as f32;
                writer.write_sample(sample).map_err(|e| e.to_string())?;
            }
        }
        writer.finalize().map_err(|e| e.to_string())?;
        self.render_progress
            .store(1.0_f32.to_bits(), Ordering::Relaxed);
        Ok(())
    }
}
