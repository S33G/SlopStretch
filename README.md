# SlopStretch

**Turn 3 seconds of audio into 3 hours of unresolved feelings.**

An independent Rust/RVST ambient stretching effect with a shared native editor,
VST3 and CLAP adapters, and a standalone app. Purple glass, gummy buttons, actual
spectral processing. The “AI-native” marketing in the loader is a joke: the audio
engine runs locally and does not use an AI service.

## Run

```sh
cargo run --locked -p plugin-standalone
```

The included three-second demo is ready to play. The app starts silent; click
**Release the Slop**. Use **Load WAV** or drag a WAV onto the editor to replace it.

1. **Feed it.** Load a WAV, or press **Consume Audio** to capture the plugin's
   incoming audio. Press again to finish capture.
2. **Make it weird.** Select a region on the waveform. Turn **SLOP** from Normal
   through Concerning to Eternal. The selected and stretched durations stay visible.
3. **Keep it.** Press **Bake Ambient Lore** to render a stereo WAV in the background.

**Emotional Support** holds the current spectral position. **Vibe Resolution**
selects the FFT window; Harmonics, Spread, Reverb, Mix, and Output shape the sound.
The nine factory presets change sound parameters, not your loaded sample.

Sources are limited to 30 seconds of mono/stereo WAV (8–192 kHz), converted to
a 48 kHz internal buffer. The native export dialog streams 1 second–3 hours of
48 kHz stereo float WAV, shows the estimated size, and supports cancellation.
Existing files are preserved; choose a new filename for each bake.

Standalone input is explicit: the RVST runner does not open a microphone. Live
capture takes the DAW input when hosted as a plugin. In standalone you can feed a
test tone or a file:

```sh
cargo run --locked -p plugin-standalone -- --tone 220
cargo run --locked -p plugin-standalone -- --file input.wav
```

## Build

```sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo xtask bundle --release
sh scripts/package-macos.sh
```

`package-macos.sh` creates `dist/SlopStretch.app`, `dist/SlopStretch.vst3`, and
`dist/SlopStretch.clap` for the current macOS architecture. It applies local ad-hoc
signatures. It does not install, publish, notarize, or qualify them in a DAW.

To generate the included audio comparison examples without an audio device:

```sh
cargo run --release --locked -p plugin-core --example render_demo -- dist/audio
```

Choose a fresh directory when regenerating examples; the renderer preserves existing files.

For the same Rust editor in a browser:

```sh
cargo xtask web --serve --port 8789
```

This is an editor preview. RVST's web runner does not process audio.

## Source map

- `crates/plugin-core`: spectral engine, source/capture transfer, parameters,
  versioned state, waveform and spectrum telemetry, offline WAV render.
- `crates/plugin-ui`: shared egui editor, native file dialogs and background jobs.
- `crates/plugin-native`: direct RVST VST3/CLAP exports.
- `crates/plugin-standalone`: native audio-device runner.
- `vendor/rvst`: portable SDK snapshot used by this independent repository.

The built-in fonts are Outfit, DM Serif Display, and Space Mono. Their SIL Open
Font Licenses are included beside the font files in `crates/plugin-ui/assets/fonts`.

## Development identity

Version `0.1.0`; vendor `Slop Audio`; plugin ID `audio.slop.slopstretch`.
These are initial development defaults. Confirm the distribution identity before
the first public release. AU/AAX support and Windows/Linux builds are separate work.

## Verification

Current evidence and limits are recorded in `docs/verification.md`. This build
is a local macOS Apple Silicon development build; DAW qualification remains
separate. The vendored standalone runner has one small local patch to respect
the plugin’s requested initial editor dimensions.
