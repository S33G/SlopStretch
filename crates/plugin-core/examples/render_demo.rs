//! Produce real audio examples without opening an audio device or plugin host.
use plugin_core::{DemoPlugin, RenderParams, SlopControls};
use rvst_core::Plugin;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist/audio"));
    std::fs::create_dir_all(&directory)?;
    let plugin = DemoPlugin::new();
    let context = plugin.editor_context().ok_or("no editor context")?;
    let controls = context
        .downcast::<SlopControls>()
        .map_err(|_| "wrong editor context")?;
    let mut values = [0.0; 11];
    for (index, spec) in DemoPlugin::descriptor().parameters.iter().enumerate() {
        values[index] = spec.default;
    }
    for (name, duration, stretch, freeze, reverb) in [
        ("01-before.wav", 3.0, 1.0, 0.0, 0.0),
        ("02-cathedral-but-sad.wav", 24.0, 32.0, 0.0, 0.62),
        ("03-emotional-support.wav", 12.0, 32.0, 1.0, 0.55),
    ] {
        values[0] = stretch;
        values[1] = freeze;
        values[5] = reverb;
        values[4] = 0.75;
        values[6] = -12.0;
        let path = directory.join(name);
        let started = std::time::Instant::now();
        controls.render_wav_to_path(
            &path,
            duration,
            RenderParams {
                values,
                sample_rate: 48000,
            },
        )?;
        let mut reader = hound::WavReader::open(&path)?;
        let mut peak = 0.0_f32;
        let mut squares = 0.0_f64;
        let mut count = 0_u64;
        for sample in reader.samples::<f32>() {
            let sample = sample?;
            if !sample.is_finite() {
                return Err("non-finite audio in rendered example".into());
            }
            peak = peak.max(sample.abs());
            squares += f64::from(sample).powi(2);
            count += 1;
        }
        let rms = (squares / count.max(1) as f64).sqrt();
        if peak == 0.0 || peak > 1.0 {
            return Err(format!("invalid rendered peak: {peak}").into());
        }
        println!(
            "{}: {duration:.1}s, {count} samples, peak={peak:.6}, rms={rms:.6}, rendered in {:.3}s",
            path.display(),
            started.elapsed().as_secs_f64()
        );
    }
    Ok(())
}
