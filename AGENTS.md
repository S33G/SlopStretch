# SlopStretch

An independent RVST spectral time-stretching plugin. The SDK snapshot lives in
`vendor/rvst`; keep plugin changes in `crates/plugin-*`.

- Keep audio callbacks allocation-free, lock-free, and free of filesystem access.
- Keep host parameter IDs and saved-state versions compatible after release.
- All jokes need a plain-language tooltip. Artificial delays belong only in the UI.
- Use the same Rust editor for standalone, VST3, CLAP, and browser UI preview.
- Distinguish offline DSP tests, native builds, format validators, and DAW testing.

Run `cargo test --workspace --locked`, `cargo fmt --all --check`, and
`cargo clippy --workspace --all-targets --locked -- -D warnings` before delivery.
