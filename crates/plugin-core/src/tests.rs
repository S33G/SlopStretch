use super::*;
use rvst_test::{AllocationGuard, GuardedAllocator, OfflineHost};
#[global_allocator]
static ALLOCATOR: GuardedAllocator<std::alloc::System> = GuardedAllocator(std::alloc::System);

fn config(rate: f64) -> ProcessConfig {
    ProcessConfig {
        sample_rate: rate,
        max_frames: 512,
        input_channels: 2,
        output_channels: 2,
    }
}
fn neutral() -> [f64; PARAM_COUNT] {
    [1.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0]
}
fn tone(frames: usize, hz: f64) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|i| {
            let x = (std::f64::consts::TAU * hz * i as f64 / 48000.0).sin() as f32 * 0.4;
            [x, x]
        })
        .collect()
}
fn wav(samples: &[[f32; 2]], rate: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut writer = hound::WavWriter::new(
            std::io::Cursor::new(&mut bytes),
            hound::WavSpec {
                channels: 2,
                sample_rate: rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for frame in samples {
            for sample in frame {
                writer.write_sample(*sample).unwrap();
            }
        }
        writer.finalize().unwrap();
    }
    bytes
}
fn decoded(bytes: &[u8]) -> Vec<f32> {
    hound::WavReader::new(std::io::Cursor::new(bytes))
        .unwrap()
        .into_samples::<f32>()
        .map(Result::unwrap)
        .collect()
}
fn rms(data: &[f32]) -> f64 {
    (data.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / data.len().max(1) as f64).sqrt()
}
fn assert_rt(counts: rvst_test::AllocationCounts) {
    assert_eq!(
        counts.allocations + counts.reallocations + counts.deallocations,
        0,
        "{counts:?}"
    );
}

#[test]
fn descriptor_demo_and_silence_until_play() {
    DemoPlugin::descriptor().validate().unwrap();
    let mut host = OfflineHost::<DemoPlugin>::new(config(48000.0), 0).unwrap();
    let controls = host.plugin.controls();
    assert_eq!(controls.snapshot().source_frames, 144000);
    assert!(!controls.snapshot().playing);
    assert!(controls.waveform().iter().all(|x| x.is_finite()));
    assert!(controls.waveform().iter().any(|x| *x > 0.1));
    let input = [0.0; 1024];
    let block = host
        .process(&input, 512, &[], Transport::default())
        .unwrap();
    assert!(block.audio.iter().all(|x| *x == 0.0));
    assert_rt(block.allocations);
}

#[test]
fn fft_stretch_preserves_pitch_and_slows_source_time() {
    let source = tone(48000, 440.0);
    for stretch in [8.0, 64.0] {
        let mut engine = dsp::Engine::new();
        let mut params = neutral();
        params[0] = stretch;
        let output: Vec<f32> = (0..48000)
            .map(|_| engine.next(source.as_slice(), source.len(), &params, 48000.0, None)[0])
            .collect();
        assert!(output.iter().all(|x| x.is_finite()));
        assert!(rms(&output[8192..]) > 0.12);
        let mut peak_hz = 0.0;
        let mut peak_power = 0.0;
        let segment = &output[8192..32768];
        for hz in (300..600).step_by(2) {
            let coefficient = 2.0 * (std::f64::consts::TAU * hz as f64 / 48000.0).cos();
            let mut a = 0.0;
            let mut b = 0.0;
            for sample in segment {
                let next = *sample as f64 + coefficient * a - b;
                b = a;
                a = next;
            }
            let power = a * a + b * b - coefficient * a * b;
            if power > peak_power {
                peak_power = power;
                peak_hz = hz as f64;
            }
        }
        assert!(
            (peak_hz - 440.0).abs() < 20.0,
            "pitch moved to {peak_hz} at {stretch}x"
        );
        let expected = 48000.0 / stretch;
        assert!(
            (engine.position() - expected).abs() <= 1024.0 / stretch + 1.0,
            "source position {} vs {expected}",
            engine.position()
        );
    }
}

#[test]
fn one_x_is_exact_selected_source_and_freeze_holds_position() {
    let source: Vec<[f32; 2]> = (0..1000)
        .map(|i| [i as f32 / 1000.0, -(i as f32) / 1000.0])
        .collect();
    let mut engine = dsp::Engine::new();
    let mut params = neutral();
    params[7] = 0.25;
    params[8] = 0.75;
    for i in 0..1000 {
        let out = engine.next(source.as_slice(), source.len(), &params, 48000.0, None);
        assert_eq!(out, source[250 + i % 500]);
    }
    params[0] = 20.0;
    params[1] = 1.0;
    let held = engine.position();
    for _ in 0..8192 {
        let out = engine.next(source.as_slice(), source.len(), &params, 48000.0, None);
        assert!(out.iter().all(|x| x.is_finite()));
    }
    assert_eq!(engine.position(), held);
    params[7] = 1.0;
    params[8] = 1.0;
    for _ in 0..1024 {
        assert!(
            engine
                .next(source.as_slice(), source.len(), &params, 48000.0, None)
                .iter()
                .all(|x| x.is_finite())
        );
    }
}

#[test]
fn capture_resamples_input_and_f64_matches_f32() {
    let mut plugin = DemoPlugin::new();
    plugin.prepare(config(44100.0)).unwrap();
    let controls = plugin.controls();
    controls.set_capture(true);
    let input = [0.25_f64; 882];
    let mut output = [0.0; 882];
    let mut block = AudioBlock::new(&input, &mut output, 441, 2, 2).unwrap();
    let guard = AllocationGuard::start();
    plugin.process_f64(&mut block, &[], &Transport::default(), &mut NullEventSink);
    assert_rt(guard.finish());
    assert!(controls.snapshot().capturing);
    assert!(controls.snapshot().source_frames.abs_diff(480) <= 1);
    assert!(output.iter().all(|x| *x == 0.25));
    assert!(controls.copy_source().is_err());
    controls.set_capture(false);
    plugin.process_f64(
        &mut AudioBlock::new(&input, &mut output, 441, 2, 2).unwrap(),
        &[],
        &Transport::default(),
        &mut NullEventSink,
    );
    assert!(!controls.snapshot().capturing);
    assert!(
        controls
            .copy_source()
            .unwrap()
            .iter()
            .flatten()
            .all(|x| (*x - 0.25).abs() < 1e-6)
    );
    let mut a = DemoPlugin::new();
    let mut b = DemoPlugin::new();
    a.prepare(config(48000.0)).unwrap();
    b.prepare(config(48000.0)).unwrap();
    for (i, value) in neutral().iter().enumerate() {
        a.set_parameter(i as u32 + 1, *value).unwrap();
        b.set_parameter(i as u32 + 1, *value).unwrap();
    }
    a.controls.set_playing(true);
    b.controls.set_playing(true);
    let input32 = [0.0_f32; 1024];
    let input64 = [0.0_f64; 1024];
    let mut out32 = [0.0; 1024];
    let mut out64 = [0.0; 1024];
    a.process(
        &mut AudioBlock::new(&input32, &mut out32, 512, 2, 2).unwrap(),
        &[],
        &Transport::default(),
        &mut NullEventSink,
    );
    b.process_f64(
        &mut AudioBlock::new(&input64, &mut out64, 512, 2, 2).unwrap(),
        &[],
        &Transport::default(),
        &mut NullEventSink,
    );
    for (a, b) in out32.iter().zip(out64) {
        assert!((*a as f64 - b).abs() < 1e-7);
    }
}

#[test]
fn full_host_rate_range_capture_and_stretch_preserve_time_without_allocating() {
    // The CLAP validator exercises rates from 1 kHz through 768 kHz, including
    // non-integer rates. Check both extremes and a fractional normal-rate host.
    for rate in [1000.0_f64, 44100.5, 384000.0, 768000.0] {
        let mut host = OfflineHost::<DemoPlugin>::new(config(rate), 0).unwrap();
        let controls = host.plugin.controls();
        controls.set_capture(true);
        let mut input = [0.0_f32; 1024];
        let capture_frames = (rate * 0.05).round() as usize;
        let mut consumed = 0;
        while consumed < capture_frames {
            let frames = (capture_frames - consumed).min(512);
            for i in 0..frames {
                let sample = (std::f64::consts::TAU * 100.0 * (consumed + i) as f64 / rate).sin()
                    as f32
                    * 0.4;
                input[i] = sample;
                input[frames + i] = sample;
            }
            let result = host
                .process(&input[..frames * 2], frames, &[], Transport::default())
                .unwrap();
            assert_rt(result.allocations);
            assert!(result.audio.iter().all(|sample| sample.is_finite()));
            consumed += frames;
        }
        controls.set_capture(false);
        input.fill(0.0);
        let result = host
            .process(&input, 512, &[], Transport::default())
            .unwrap();
        assert_rt(result.allocations);
        let source_frames = (capture_frames as f64 * 48000.0 / rate).ceil() as usize;
        assert_eq!(
            controls.snapshot().source_frames,
            source_frames,
            "capture duration at {rate} Hz"
        );
        assert_eq!(controls.snapshot().sample_rate, SOURCE_SAMPLE_RATE);
        assert!(rms(&controls.waveform()) > 0.1);

        let mut params = neutral();
        params[0] = 8.0;
        params[2] = 0.0;
        params[5] = 1.0; // Includes the longest delay at the highest host rate.
        for (i, value) in params.iter().enumerate() {
            host.plugin.set_parameter(i as u32 + 1, *value).unwrap();
        }
        controls.set_playing(true);
        let mut peak = 0.0_f32;
        for _ in 0..16 {
            let result = host
                .process(&input, 512, &[], Transport::default())
                .unwrap();
            assert_rt(result.allocations);
            for sample in result.audio {
                assert!(sample.is_finite());
                peak = peak.max(sample.abs());
            }
        }
        assert!(peak > 0.01, "silent stretched audio at {rate} Hz");
        let source_position = controls.snapshot().playhead as f64 * source_frames as f64;
        let expected = (8192.0 * 48000.0 / rate / 8.0).rem_euclid(source_frames as f64);
        assert!(
            (source_position - expected).abs() < 0.01,
            "source clock at {rate} Hz: {source_position} vs {expected}"
        );
    }
}

#[test]
fn callback_is_allocation_free_through_commands_and_automation() {
    let mut host = OfflineHost::<DemoPlugin>::new(config(48000.0), 0).unwrap();
    let controls = host.plugin.controls();
    let input = [0.1; 1024];
    controls.set_playing(true);
    let events = [
        Event {
            frame: 0,
            kind: EventKind::Parameter { id: 0, value: 1.0 },
        },
        Event {
            frame: 0,
            kind: EventKind::Parameter {
                id: 1,
                value: f64::NAN,
            },
        },
        Event {
            frame: 2,
            kind: EventKind::Parameter {
                id: SLOP,
                value: 3600.0,
            },
        },
        Event {
            frame: 8,
            kind: EventKind::ParameterRamp(ParameterRamp {
                id: HARMONICS,
                start_frame: 8,
                end_frame: 500,
                start_normalized: 0.0,
                end_normalized: 1.0,
            }),
        },
        Event {
            frame: 15,
            kind: EventKind::Parameter {
                id: VIBE,
                value: 0.0,
            },
        },
        Event {
            frame: 31,
            kind: EventKind::Parameter {
                id: VIBE,
                value: 3.0,
            },
        },
        Event {
            frame: 64,
            kind: EventKind::Parameter {
                id: FREEZE,
                value: 1.0,
            },
        },
    ];
    for i in 0..24 {
        if i == 3 {
            controls.rewind();
        }
        if i == 5 {
            controls.set_capture(true);
        }
        if i == 8 {
            controls.set_capture(false);
            controls.set_playing(true);
        }
        if i == 12 {
            controls.load_demo().unwrap();
            controls.set_playing(true);
        }
        let result = host
            .process(&input, 512, &events, Transport::default())
            .unwrap();
        assert_rt(result.allocations);
        assert!(result.audio.iter().all(|x| x.is_finite()));
    }
    let guard = AllocationGuard::start();
    let invalid = host.plugin.set_parameter(0, f64::NAN);
    drop(invalid);
    assert_rt(guard.finish());
}

#[test]
fn wav_import_export_duration_selection_dry_bypass_and_limits() {
    let plugin = DemoPlugin::new();
    let controls = plugin.controls();
    let source: Vec<[f32; 2]> = (0..4800)
        .map(|i| if i < 2400 { [0.2; 2] } else { [0.6; 2] })
        .collect();
    controls.load_wav_bytes(&wav(&source, 48000)).unwrap();
    assert_eq!(controls.snapshot().source_frames, 4800);
    let mut params = RenderParams {
        values: neutral(),
        sample_rate: 48000,
    };
    params.values[7] = 0.5;
    let bytes = controls.render_wav(0.1, params).unwrap();
    let audio = decoded(&bytes);
    assert_eq!(audio.len(), 9600);
    assert!((audio[2400] - 0.6).abs() < 1e-6);
    params.values[0] = 16.0;
    params.values[6] = -40.0;
    params.values[9] = 0.0;
    let dry = decoded(&controls.render_wav(0.1, params).unwrap());
    assert!((dry[2400] - 0.6).abs() < 1e-6);
    params.values[9] = 1.0;
    params.values[10] = 1.0;
    let bypass = decoded(&controls.render_wav(0.1, params).unwrap());
    assert_eq!(dry, bypass);
    assert!(
        controls
            .render_wav(MAX_RENDER_SECONDS + 1.0, params)
            .is_err()
    );
    assert!(controls.render_wav(f32::NAN, params).is_err());
    assert!(controls.load_wav_bytes(b"not a wave").is_err());
    assert_eq!(controls.snapshot().source_frames, 4800);
    assert_eq!(controls.snapshot().render_progress, 1.0);
    assert!(!controls.snapshot().rendering);
}

#[test]
fn state_restores_embedded_source_and_rejects_corruption_atomically() {
    let mut plugin = DemoPlugin::new();
    plugin.set_parameter(SLOP, 99.0).unwrap();
    plugin.controls.publish(&tone(2400, 330.0)).unwrap();
    let saved = plugin.save_state().unwrap();
    let mut other = DemoPlugin::new();
    other.load_state(&saved).unwrap();
    assert_eq!(other.parameter(SLOP), Some(99.0));
    assert_eq!(other.controls.snapshot().source_frames, 2400);
    assert_eq!(saved, other.save_state().unwrap());
    for offset in [8, 104] {
        let mut bad = saved.clone();
        if offset == 8 {
            bad[offset..offset + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        } else {
            bad[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        }
        assert!(other.load_state(&bad).is_err());
        assert_eq!(saved, other.save_state().unwrap());
    }
    assert!(other.load_state(&saved[..saved.len() - 1]).is_err());
    assert_eq!(saved, other.save_state().unwrap());
}

#[test]
fn concurrent_source_publication_keeps_audio_finite() {
    let mut host = OfflineHost::<DemoPlugin>::new(config(48000.0), 0).unwrap();
    let controls = host.plugin.controls();
    controls.set_playing(true);
    let other = controls.clone();
    let writer = std::thread::spawn(move || {
        for i in 0..24 {
            let source = tone(4096, 200.0 + i as f64 * 3.0);
            other.publish(&source).unwrap();
            other.set_playing(true);
        }
    });
    let input = [0.0; 1024];
    for _ in 0..100 {
        let result = host
            .process(&input, 512, &[], Transport::default())
            .unwrap();
        assert_rt(result.allocations);
        assert!(result.audio.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }
    writer.join().unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streamed_render_uses_new_file_and_cancellation_removes_partial() {
    let plugin = DemoPlugin::new();
    let controls = plugin.controls();
    let dir = std::env::temp_dir().join(format!("slopstretch-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("short.wav");
    let _ = std::fs::remove_file(&path);
    controls
        .render_wav_to_path(&path, 0.05, RenderParams::default())
        .unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() > 1000);
    assert!(
        controls
            .render_wav_to_path(&path, 0.05, RenderParams::default())
            .is_err()
    );
    std::fs::remove_file(&path).unwrap();
    let cancelled = dir.join("cancelled.wav");
    let _ = std::fs::remove_file(&cancelled);
    let worker_controls = controls.clone();
    let worker_path = cancelled.clone();
    let worker = std::thread::spawn(move || {
        worker_controls.render_wav_to_path(&worker_path, 100.0, RenderParams::default())
    });
    // Cancel after the worker has entered the actual rendering loop.
    while controls.snapshot().render_progress <= 0.0 || !controls.snapshot().rendering {
        std::thread::yield_now();
    }
    controls.cancel_render();
    assert!(worker.join().unwrap().is_err());
    assert!(!cancelled.exists());
    let _ = std::fs::remove_dir(dir);
}
