//! SlopStretch: bounded, pitch-preserving random-phase spectral stretching.
//! Audio callbacks only use preallocated memory and atomic control messages.
mod dsp;
mod wave;

use rvst_core::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
};

pub const PARAM_COUNT: usize = 11;
pub const SLOP: u32 = 1;
pub const FREEZE: u32 = 2;
pub const VIBE: u32 = 3;
pub const HARMONICS: u32 = 4;
pub const SPREAD: u32 = 5;
pub const REVERB: u32 = 6;
pub const OUTPUT: u32 = 7;
pub const REGION_START: u32 = 8;
pub const REGION_END: u32 = 9;
pub const MIX: u32 = 10;
pub const BYPASS: u32 = 11;
pub const SOURCE_SAMPLE_RATE: u32 = 48_000;
pub const MAX_SOURCE_SECONDS: usize = 30;
pub const MAX_SOURCE_FRAMES: usize = SOURCE_SAMPLE_RATE as usize * MAX_SOURCE_SECONDS;
/// The in-memory helper is intentionally bounded. File exports stream up to three hours.
pub const MAX_RENDER_SECONDS: f32 = 120.0;
pub const MAX_FILE_RENDER_SECONDS: f32 = 10_800.0;
pub const DEFAULT_VALUES: [f64; PARAM_COUNT] =
    [16.0, 0.0, 2.0, 0.2, 0.65, 0.3, -9.0, 0.0, 1.0, 1.0, 0.0];
pub const FFT_SIZES: [usize; 4] = [1024, 2048, 4096, 8192];

const fn spec(
    id: u32,
    name: &'static str,
    unit: &'static str,
    min: f64,
    max: f64,
    default: f64,
    step: Option<f64>,
) -> ParameterSpec {
    ParameterSpec {
        id,
        name,
        unit,
        min,
        max,
        default,
        step,
        bypass: id == BYPASS,
    }
}
pub const PARAMETERS: &[ParameterSpec] = &[
    spec(SLOP, "SLOP", "×", 1.0, 3600.0, DEFAULT_VALUES[0], None),
    spec(
        FREEZE,
        "Emotional Support",
        "",
        0.0,
        1.0,
        DEFAULT_VALUES[1],
        Some(1.0),
    ),
    spec(
        VIBE,
        "Vibe Resolution",
        "",
        0.0,
        3.0,
        DEFAULT_VALUES[2],
        Some(1.0),
    ),
    spec(
        HARMONICS,
        "Harmonics",
        "",
        0.0,
        1.0,
        DEFAULT_VALUES[3],
        None,
    ),
    spec(SPREAD, "Spread", "", 0.0, 1.0, DEFAULT_VALUES[4], None),
    spec(REVERB, "Reverb", "", 0.0, 1.0, DEFAULT_VALUES[5], None),
    spec(OUTPUT, "Output", "dB", -60.0, 6.0, DEFAULT_VALUES[6], None),
    spec(
        REGION_START,
        "Region start",
        "",
        0.0,
        1.0,
        DEFAULT_VALUES[7],
        None,
    ),
    spec(
        REGION_END,
        "Region end",
        "",
        0.0,
        1.0,
        DEFAULT_VALUES[8],
        None,
    ),
    spec(MIX, "Wet / dry", "", 0.0, 1.0, DEFAULT_VALUES[9], None),
    spec(
        BYPASS,
        "Bypass",
        "",
        0.0,
        1.0,
        DEFAULT_VALUES[10],
        Some(1.0),
    ),
];
const STEREO: &[AudioBus] = &[AudioBus {
    name: "Main",
    channels: 2,
    auxiliary: false,
}];
static DESCRIPTOR: PluginDescriptor = PluginDescriptor {
    id: "audio.slop.slopstretch",
    name: "SlopStretch",
    vendor: "Slop Audio",
    version: "0.1.0",
    url: "",
    email: "",
    vst3_id: [
        217, 144, 234, 213, 80, 72, 178, 249, 170, 9, 22, 95, 59, 50, 29, 90,
    ],
    kind: PluginKind::Effect,
    parameters: PARAMETERS,
    inputs: STEREO,
    outputs: STEREO,
    midi_input: false,
    midi_output: false,
    supports_f64: true,
};
const IDLE: usize = 0;
const LOADING: usize = 1;
const CAPTURING: usize = 2;
const SNAPSHOT: usize = 3;

/// All sample storage is bounded and atomic: editor/background jobs cannot create
/// a Rust data race with the audio thread. The state machine additionally enforces
/// one writer, and a revision check discards analysis that overlaps publication.
pub struct SlopControls {
    samples: Box<[AtomicU32]>,
    frames: AtomicUsize,
    mode: AtomicUsize,
    revision: AtomicU32,
    playing: AtomicBool,
    capture_requested: AtomicBool,
    rewind_requested: AtomicBool,
    playhead: AtomicU32,
    peak: AtomicU32,
    spectrum: [AtomicU32; 64],
    render_state: AtomicUsize,
    render_progress: AtomicU32,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct UiSnapshot {
    pub playing: bool,
    pub capturing: bool,
    pub source_frames: usize,
    pub sample_rate: u32,
    pub playhead: f32,
    pub peak: f32,
    pub revision: u32,
    pub rendering: bool,
    pub render_progress: f32,
}
#[derive(Debug, Clone, Copy)]
pub struct RenderParams {
    pub values: [f64; PARAM_COUNT],
    pub sample_rate: u32,
}
impl Default for RenderParams {
    fn default() -> Self {
        Self {
            values: DEFAULT_VALUES,
            sample_rate: SOURCE_SAMPLE_RATE,
        }
    }
}
impl SlopControls {
    fn new() -> Self {
        Self {
            samples: (0..MAX_SOURCE_FRAMES * 2)
                .map(|_| AtomicU32::new(0))
                .collect(),
            frames: AtomicUsize::new(0),
            mode: AtomicUsize::new(IDLE),
            revision: AtomicU32::new(0),
            playing: AtomicBool::new(false),
            capture_requested: AtomicBool::new(false),
            rewind_requested: AtomicBool::new(false),
            playhead: AtomicU32::new(0),
            peak: AtomicU32::new(0),
            spectrum: std::array::from_fn(|_| AtomicU32::new(0)),
            render_state: AtomicUsize::new(0),
            render_progress: AtomicU32::new(0),
        }
    }
    pub fn set_capture(&self, enabled: bool) {
        if enabled {
            self.playing.store(false, Ordering::Release);
        }
        self.capture_requested.store(enabled, Ordering::Release);
    }
    pub fn set_playing(&self, enabled: bool) {
        if enabled {
            self.capture_requested.store(false, Ordering::Release);
        }
        self.playing.store(enabled, Ordering::Release);
    }
    pub fn rewind(&self) {
        self.rewind_requested.store(true, Ordering::Release);
    }
    pub fn cancel_render(&self) {
        let _ = self
            .render_state
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire);
    }
    pub fn snapshot(&self) -> UiSnapshot {
        UiSnapshot {
            playing: self.playing.load(Ordering::Acquire),
            capturing: self.mode.load(Ordering::Acquire) == CAPTURING
                || self.capture_requested.load(Ordering::Acquire),
            source_frames: self.frames.load(Ordering::Acquire),
            sample_rate: SOURCE_SAMPLE_RATE,
            playhead: f32::from_bits(self.playhead.load(Ordering::Relaxed)),
            peak: f32::from_bits(self.peak.load(Ordering::Relaxed)),
            revision: self.revision.load(Ordering::Acquire),
            rendering: self.render_state.load(Ordering::Acquire) != 0,
            render_progress: f32::from_bits(self.render_progress.load(Ordering::Relaxed)),
        }
    }
    pub fn waveform(&self) -> [f32; 256] {
        let len = self.frames.load(Ordering::Acquire).min(MAX_SOURCE_FRAMES);
        let mut result = [0.0_f32; 256];
        if len == 0 {
            return result;
        }
        for (bin, value) in result.iter_mut().enumerate() {
            let start = bin * len / 256;
            let end = ((bin + 1) * len / 256).max(start + 1).min(len);
            let stride = ((end - start) / 48).max(1);
            for i in (start..end).step_by(stride) {
                *value = value.max(self.read(i, 0).abs()).max(self.read(i, 1).abs());
            }
        }
        result
    }
    pub fn spectrum(&self) -> [f32; 64] {
        std::array::from_fn(|i| f32::from_bits(self.spectrum[i].load(Ordering::Relaxed)))
    }
    #[inline]
    fn read(&self, index: usize, channel: usize) -> f32 {
        f32::from_bits(self.samples[index * 2 + channel].load(Ordering::Relaxed))
    }
    #[inline]
    fn write(&self, index: usize, sample: [f32; 2]) {
        self.samples[index * 2].store(sample[0].to_bits(), Ordering::Relaxed);
        self.samples[index * 2 + 1].store(sample[1].to_bits(), Ordering::Relaxed);
    }
    fn publish(&self, samples: &[[f32; 2]]) -> Result<(), String> {
        if samples.is_empty()
            || samples.len() > MAX_SOURCE_FRAMES
            || samples.iter().flatten().any(|x| !x.is_finite())
        {
            return Err("Source must contain finite stereo audio, up to 30 seconds.".into());
        }
        let _guard = self.claim(LOADING)?;
        self.playing.store(false, Ordering::Release);
        for (i, sample) in samples.iter().enumerate() {
            self.write(i, *sample);
        }
        self.frames.store(samples.len(), Ordering::Release);
        self.playhead.store(0, Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::Release);
        Ok(())
    }
    fn claim(&self, mode: usize) -> Result<SourceGuard<'_>, String> {
        self.mode
            .compare_exchange(IDLE, mode, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                "Finish capturing or loading before changing/exporting the source.".to_string()
            })?;
        Ok(SourceGuard(self))
    }
    fn copy_source(&self) -> Result<Vec<[f32; 2]>, String> {
        let _guard = self.claim(SNAPSHOT)?;
        let len = self.frames.load(Ordering::Acquire).min(MAX_SOURCE_FRAMES);
        if len == 0 {
            return Err("Consume some audio first. Even slop needs ingredients.".into());
        }
        Ok((0..len)
            .map(|i| [self.read(i, 0), self.read(i, 1)])
            .collect())
    }
    /// Decode PCM/float mono/stereo WAV and linearly resample to the bounded 48 kHz source store.
    pub fn load_wav_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        self.publish(&wave::decode(bytes)?)
    }
    /// A self-contained, three-second chord and bell recording. Loading never starts playback.
    pub fn load_demo(&self) -> Result<(), String> {
        let source: Vec<[f32; 2]> = (0..SOURCE_SAMPLE_RATE as usize * 3)
            .map(|i| {
                let t = i as f64 / SOURCE_SAMPLE_RATE as f64;
                let attack = (t * 50.0).min(1.0);
                let release = ((3.0 - t) * 10.0).min(1.0);
                let envelope = attack * release * (-t * 0.55).exp();
                let mut left = 0.0;
                let mut right = 0.0;
                for (j, hz) in [164.8138, 220.0, 246.9417, 329.6276].iter().enumerate() {
                    left += (std::f64::consts::TAU * hz * t + j as f64 * 0.17).sin() * 0.105;
                    right += (std::f64::consts::TAU * hz * t + j as f64 * 0.31).sin() * 0.105;
                }
                let bell = (std::f64::consts::TAU * 659.255 * t).sin() * (-t * 2.8).exp() * 0.16;
                [
                    ((left + bell) * envelope) as f32,
                    ((right + bell) * envelope) as f32,
                ]
            })
            .collect();
        self.publish(&source)
    }
}
struct SourceGuard<'a>(&'a SlopControls);
impl Drop for SourceGuard<'_> {
    fn drop(&mut self) {
        self.0.mode.store(IDLE, Ordering::Release);
    }
}

pub struct DemoPlugin {
    values: [f64; PARAM_COUNT],
    controls: Arc<SlopControls>,
    engine: dsp::Engine,
    sample_rate: f64,
    last_revision: u32,
    capture_frames: usize,
    capture_clock: u64,
    capture_next: f64,
    capture_previous: [f32; 2],
    output_gain: f64,
    playback_envelope: f64,
}
trait Sample: Copy {
    fn read(self) -> f64;
    fn write(value: f64) -> Self;
}
impl Sample for f32 {
    fn read(self) -> f64 {
        self as f64
    }
    fn write(value: f64) -> Self {
        value as f32
    }
}
impl Sample for f64 {
    fn read(self) -> f64 {
        self
    }
    fn write(value: f64) -> Self {
        value
    }
}
impl DemoPlugin {
    pub fn controls(&self) -> Arc<SlopControls> {
        Arc::clone(&self.controls)
    }
    fn accept_parameter(&mut self, id: u32, value: f64) {
        if (1..=PARAM_COUNT as u32).contains(&id) && value.is_finite() {
            let i = (id - 1) as usize;
            self.values[i] = PARAMETERS[i].clamp(value);
        }
    }
    fn capture_command(&mut self) {
        let requested = self.controls.capture_requested.load(Ordering::Acquire);
        let mode = self.controls.mode.load(Ordering::Acquire);
        if requested
            && mode == IDLE
            && self
                .controls
                .mode
                .compare_exchange(IDLE, CAPTURING, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            self.capture_frames = 0;
            self.capture_clock = 0;
            self.capture_next = 0.0;
            self.capture_previous = [0.0; 2];
            self.controls.frames.store(0, Ordering::Release);
            self.controls.revision.fetch_add(1, Ordering::Release);
            self.controls.playing.store(false, Ordering::Release);
            self.engine.reset();
            self.playback_envelope = 0.0;
        } else if !requested && mode == CAPTURING {
            // Close the final input-sample interval with a held sample. A causal
            // interpolator has no next input at Stop; this keeps capture duration
            // correct even when one host frame spans 48 source frames at 1 kHz.
            let final_frames = ((self.capture_clock as f64 * SOURCE_SAMPLE_RATE as f64
                / self.sample_rate)
                .ceil() as usize)
                .min(MAX_SOURCE_FRAMES);
            while self.capture_frames < final_frames {
                self.controls
                    .write(self.capture_frames, self.capture_previous);
                self.capture_frames += 1;
            }
            self.controls
                .frames
                .store(self.capture_frames, Ordering::Release);
            self.controls.revision.fetch_add(1, Ordering::Release);
            self.controls.mode.store(IDLE, Ordering::Release);
        }
    }
    fn capture_sample(&mut self, input: [f32; 2]) {
        let clock = self.capture_clock as f64;
        while self.capture_next <= clock && self.capture_frames < MAX_SOURCE_FRAMES {
            let fraction = if self.capture_clock == 0 {
                1.0
            } else {
                (self.capture_next - (clock - 1.0)).clamp(0.0, 1.0)
            } as f32;
            let sample = std::array::from_fn(|c| {
                self.capture_previous[c] + (input[c] - self.capture_previous[c]) * fraction
            });
            self.controls.write(self.capture_frames, sample);
            self.capture_frames += 1;
            self.capture_next += self.sample_rate / SOURCE_SAMPLE_RATE as f64;
        }
        self.capture_clock += 1;
        self.capture_previous = input;
        if self.capture_frames == MAX_SOURCE_FRAMES {
            self.controls
                .frames
                .store(self.capture_frames, Ordering::Release);
            self.controls
                .capture_requested
                .store(false, Ordering::Release);
            self.controls.revision.fetch_add(1, Ordering::Release);
            self.controls.mode.store(IDLE, Ordering::Release);
        }
    }
    fn render<T: Sample>(&mut self, b: &mut AudioBlock<'_, T>, events: &[Event]) -> ProcessStatus {
        self.capture_command();
        let revision = self.controls.revision.load(Ordering::Acquire);
        if revision != self.last_revision
            || self.controls.rewind_requested.swap(false, Ordering::AcqRel)
        {
            self.last_revision = revision;
            self.engine.reset();
            self.playback_envelope = 0.0;
        }
        let mut peak = 0.0_f64;
        for frame in 0..b.frames {
            // Host events are bounded by the host. Invalid IDs/nonfinite values are
            // ignored without constructing an allocating String error.
            for event in events {
                match event.kind {
                    EventKind::Parameter { id, value } if event.frame as usize == frame => {
                        self.accept_parameter(id, value)
                    }
                    EventKind::ParameterRamp(ramp)
                        if frame as u32 >= ramp.start_frame
                            && frame as u32 <= ramp.end_frame
                            && (1..=PARAM_COUNT as u32).contains(&ramp.id) =>
                    {
                        let value = PARAMETERS[(ramp.id - 1) as usize]
                            .denormalize(ramp.normalized_at(frame as u32));
                        self.accept_parameter(ramp.id, value);
                    }
                    _ => {}
                }
            }
            let mut input = [0.0; 2];
            for (ch, value) in input.iter_mut().enumerate() {
                let source_ch = ch.min(b.input_channels.saturating_sub(1));
                if b.input_channels > 0 {
                    let x = b.inputs[source_ch * b.frames + frame].read();
                    *value = if x.is_finite() {
                        x.clamp(-16.0, 16.0)
                    } else {
                        0.0
                    };
                }
            }
            let mode = self.controls.mode.load(Ordering::Acquire);
            let capturing = mode == CAPTURING;
            if capturing {
                self.capture_sample([input[0] as f32, input[1] as f32]);
            }
            let playing =
                self.controls.playing.load(Ordering::Acquire) && !capturing && mode != LOADING;
            let envelope_target = if playing { 1.0 } else { 0.0 };
            let step = 1.0 / (self.sample_rate * 0.01);
            self.playback_envelope += (envelope_target - self.playback_envelope).clamp(-step, step);
            let source_frames = self
                .controls
                .frames
                .load(Ordering::Acquire)
                .min(MAX_SOURCE_FRAMES);
            let mut wet = [0.0; 2];
            if source_frames > 0
                && mode != LOADING
                && !capturing
                && (playing || self.playback_envelope > 0.0)
            {
                wet = self.engine.next(
                    &*self.controls,
                    source_frames,
                    &self.values,
                    self.sample_rate,
                    Some(&self.controls),
                );
                // Publication may race an FFT calculation. Atomic storage remains
                // safe; discard any mixed-generation output before it reaches a host.
                if self.controls.revision.load(Ordering::Acquire) != revision
                    || self.controls.mode.load(Ordering::Acquire) == LOADING
                {
                    wet = [0.0; 2];
                }
            }
            let gain_target = 10.0_f64.powf(self.values[(OUTPUT - 1) as usize] / 20.0);
            self.output_gain += (gain_target - self.output_gain) * 0.002;
            let mix = self.values[(MIX - 1) as usize];
            for ch in 0..b.output_channels {
                let c = ch.min(1);
                let result = if self.values[(BYPASS - 1) as usize] >= 0.5 || capturing {
                    input[c]
                } else {
                    (wet[c] as f64 * self.playback_envelope * self.output_gain * mix
                        + input[c] * (1.0 - mix))
                        .clamp(-1.0, 1.0)
                };
                peak = peak.max(result.abs());
                b.outputs[ch * b.frames + frame] = T::write(result);
            }
        }
        if self.controls.mode.load(Ordering::Acquire) == CAPTURING {
            self.controls
                .frames
                .store(self.capture_frames, Ordering::Release);
        }
        self.controls
            .peak
            .store((peak as f32).to_bits(), Ordering::Relaxed);
        let len = self.controls.frames.load(Ordering::Acquire);
        self.controls.playhead.store(
            (if len == 0 {
                0.0
            } else {
                (self.engine.position() / len as f64).clamp(0.0, 1.0) as f32
            })
            .to_bits(),
            Ordering::Relaxed,
        );
        ProcessStatus::Continue
    }
}
impl Plugin for DemoPlugin {
    fn editor_size() -> (u32, u32) {
        (1180, 820)
    }
    fn editor_min_size() -> (u32, u32) {
        (900, 700)
    }
    fn descriptor() -> &'static PluginDescriptor {
        &DESCRIPTOR
    }
    fn new() -> Self {
        let controls = Arc::new(SlopControls::new());
        controls.load_demo().expect("bounded built-in demo");
        let revision = controls.revision.load(Ordering::Acquire);
        Self {
            values: DEFAULT_VALUES,
            controls,
            engine: dsp::Engine::new(),
            sample_rate: 48_000.0,
            last_revision: revision,
            capture_frames: 0,
            capture_clock: 0,
            capture_next: 0.0,
            capture_previous: [0.0; 2],
            output_gain: 10.0_f64.powf(DEFAULT_VALUES[6] / 20.0),
            playback_envelope: 0.0,
        }
    }
    fn editor_context(&self) -> Option<EditorContext> {
        Some(self.controls.clone())
    }
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), Error> {
        config.validate()?;
        if !(1000.0..=768000.0).contains(&config.sample_rate) {
            return Err("SlopStretch supports host sample rates from 1 to 768 kHz.".into());
        }
        self.sample_rate = config.sample_rate;
        self.reset();
        Ok(())
    }
    fn process(
        &mut self,
        b: &mut AudioBlock<'_, f32>,
        events: &[Event],
        _: &Transport,
        _: &mut dyn EventSink,
    ) -> ProcessStatus {
        self.render(b, events)
    }
    fn process_f64(
        &mut self,
        b: &mut AudioBlock<'_, f64>,
        events: &[Event],
        _: &Transport,
        _: &mut dyn EventSink,
    ) -> ProcessStatus {
        self.render(b, events)
    }
    fn reset(&mut self) {
        self.engine.reset();
        self.playback_envelope = 0.0;
        self.controls
            .capture_requested
            .store(false, Ordering::Release);
        if self.controls.mode.load(Ordering::Acquire) == CAPTURING {
            self.controls.mode.store(IDLE, Ordering::Release);
        }
        self.controls.peak.store(0, Ordering::Relaxed);
        self.controls.playhead.store(0, Ordering::Relaxed);
        self.output_gain = 10.0_f64.powf(self.values[6] / 20.0);
    }
    fn parameter(&self, id: u32) -> Option<f64> {
        id.checked_sub(1)
            .and_then(|i| self.values.get(i as usize).copied())
    }
    fn set_parameter(&mut self, id: u32, value: f64) -> Result<(), Error> {
        if !(1..=PARAM_COUNT as u32).contains(&id) || !value.is_finite() {
            return Err(Error(String::new()));
        }
        self.accept_parameter(id, value);
        Ok(())
    }
    fn save_state(&self) -> Result<Vec<u8>, Error> {
        let source = self.controls.copy_source().map_err(Error)?;
        let mut bytes = Vec::with_capacity(104 + source.len() * 8);
        bytes.extend_from_slice(b"SLOPSTR1");
        for value in self.values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&(source.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&SOURCE_SAMPLE_RATE.to_le_bytes());
        for frame in source {
            for sample in frame {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        Ok(bytes)
    }
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), Error> {
        const HEADER: usize = 8 + PARAM_COUNT * 8 + 8;
        if bytes.len() < HEADER || &bytes[..8] != b"SLOPSTR1" {
            return Err("Unknown or truncated SlopStretch state.".into());
        }
        let mut values = [0.0; PARAM_COUNT];
        for (i, value) in values.iter_mut().enumerate() {
            *value = f64::from_le_bytes(bytes[8 + i * 8..16 + i * 8].try_into().unwrap());
            if !value.is_finite() || PARAMETERS[i].clamp(*value) != *value {
                return Err("Invalid SlopStretch parameter state.".into());
            }
        }
        let count = u32::from_le_bytes(bytes[96..100].try_into().unwrap()) as usize;
        let rate = u32::from_le_bytes(bytes[100..104].try_into().unwrap());
        if count == 0
            || count > MAX_SOURCE_FRAMES
            || rate != SOURCE_SAMPLE_RATE
            || bytes.len() != HEADER + count * 8
        {
            return Err("Invalid SlopStretch source state.".into());
        }
        let mut source = Vec::with_capacity(count);
        for chunk in bytes[HEADER..].as_chunks::<8>().0 {
            let frame = [
                f32::from_le_bytes(chunk[..4].try_into().unwrap()),
                f32::from_le_bytes(chunk[4..].try_into().unwrap()),
            ];
            if frame.iter().any(|x| !x.is_finite() || x.abs() > 16.0) {
                return Err("Invalid SlopStretch sample state.".into());
            }
            source.push(frame);
        }
        // Validation is complete before either source or parameter mutation.
        self.controls.publish(&source).map_err(Error)?;
        self.values = values;
        self.reset();
        Ok(())
    }
    fn tail(&self) -> Tail {
        Tail::Infinite
    }
}

#[cfg(test)]
mod tests;
