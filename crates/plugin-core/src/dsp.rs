//! Real-time spectral engine. All scratch, FFT roots, windows and delay memory
//! are allocated before process; changing resolution only selects prepared data.
use super::{FFT_SIZES, PARAM_COUNT, SOURCE_SAMPLE_RATE, SlopControls};
use std::sync::atomic::Ordering;
const MAX_FFT: usize = 8192;
// The longest tap is 89.9 ms, including hosts running at 768 kHz.
const DELAY_SIZE: usize = 131072;
#[derive(Clone, Copy, Default)]
struct Complex {
    re: f32,
    im: f32,
}
impl Complex {
    fn mul(self, rhs: Self) -> Self {
        Self {
            re: self.re * rhs.re - self.im * rhs.im,
            im: self.re * rhs.im + self.im * rhs.re,
        }
    }
}
pub(crate) trait Source {
    fn sample(&self, index: usize, channel: usize) -> f32;
}
impl Source for SlopControls {
    fn sample(&self, index: usize, channel: usize) -> f32 {
        self.read(index, channel)
    }
}
impl Source for [[f32; 2]] {
    fn sample(&self, index: usize, channel: usize) -> f32 {
        self[index][channel]
    }
}
pub(crate) struct Engine {
    work: Vec<Complex>,
    roots: Vec<Complex>,
    windows: [Vec<f32>; 4],
    magnitudes: [Vec<f32>; 2],
    phases: Vec<[f32; 2]>,
    overlap: [Vec<f32>; 2],
    delays: [Vec<f32>; 2],
    cursor: usize,
    delay_cursor: usize,
    until_frame: usize,
    resolution: usize,
    position: f64,
    initialized: bool,
    direct: bool,
    random: u64,
}
impl Engine {
    pub(crate) fn new() -> Self {
        Self {
            work: vec![Complex::default(); MAX_FFT],
            roots: (0..MAX_FFT / 2)
                .map(|i| {
                    let angle = -std::f64::consts::TAU * i as f64 / MAX_FFT as f64;
                    Complex {
                        re: angle.cos() as f32,
                        im: angle.sin() as f32,
                    }
                })
                .collect(),
            windows: FFT_SIZES.map(|n| {
                (0..n)
                    .map(|i| {
                        (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos()) as f32
                    })
                    .collect()
            }),
            magnitudes: std::array::from_fn(|_| vec![0.0; MAX_FFT / 2 + 1]),
            phases: vec![[0.0; 2]; MAX_FFT / 2 + 1],
            overlap: std::array::from_fn(|_| vec![0.0; MAX_FFT]),
            delays: std::array::from_fn(|_| vec![0.0; DELAY_SIZE]),
            cursor: 0,
            delay_cursor: 0,
            until_frame: 0,
            resolution: 2,
            position: 0.0,
            initialized: false,
            direct: false,
            random: 0x8f92_9c71_eaad_1235,
        }
    }
    pub(crate) fn reset(&mut self) {
        self.overlap[0].fill(0.0);
        self.overlap[1].fill(0.0);
        self.delays[0].fill(0.0);
        self.delays[1].fill(0.0);
        self.cursor = 0;
        self.delay_cursor = 0;
        self.until_frame = 0;
        self.position = 0.0;
        self.initialized = false;
        self.random = 0x8f92_9c71_eaad_1235;
    }
    pub(crate) fn position(&self) -> f64 {
        self.position
    }
    fn random_phase(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random >> 40) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
    }
    pub(crate) fn next<S: Source + ?Sized>(
        &mut self,
        source: &S,
        frames: usize,
        values: &[f64; PARAM_COUNT],
        rate: f64,
        meters: Option<&SlopControls>,
    ) -> [f32; 2] {
        if frames == 0 {
            return [0.0; 2];
        }
        let a = values[7].min(values[8]);
        let b = values[7].max(values[8]);
        let start = (a * frames as f64).floor().clamp(0.0, (frames - 1) as f64);
        let end = (b * frames as f64).ceil().clamp(start + 1.0, frames as f64);
        let length = end - start;
        if !self.initialized || self.position < start || self.position >= end {
            self.position = start;
            self.initialized = true;
            self.until_frame = 0;
            self.overlap[0].fill(0.0);
            self.overlap[1].fill(0.0);
        }
        let resolution = (values[2].round() as usize).min(3);
        let freeze = values[1] >= 0.5;
        let direct = values[0] <= 1.000001 && !freeze;
        if self.resolution != resolution || self.direct != direct {
            self.resolution = resolution;
            self.direct = direct;
            self.until_frame = 0;
            self.overlap[0].fill(0.0);
            self.overlap[1].fill(0.0);
        }
        let ratio = SOURCE_SAMPLE_RATE as f64 / rate;
        let mut dry = [0.0; 2];
        if direct {
            for (ch, value) in dry.iter_mut().enumerate() {
                *value = read_interpolated(source, self.position, ch, start, end);
            }
            self.position = start + (self.position - start + ratio).rem_euclid(length);
        } else {
            if self.until_frame == 0 {
                self.synthesize(source, start, end, ratio, values, meters);
                let hop = FFT_SIZES[resolution] / 4;
                self.until_frame = hop;
                if !freeze {
                    self.position = start
                        + (self.position - start + hop as f64 * ratio / values[0].max(1.0))
                            .rem_euclid(length);
                }
            }
            for (ch, value) in dry.iter_mut().enumerate() {
                *value = self.overlap[ch][self.cursor];
                self.overlap[ch][self.cursor] = 0.0;
            }
            self.cursor = (self.cursor + 1) % MAX_FFT;
            self.until_frame -= 1;
        }
        // Cross-fed, damped ambience. Fixed memory; delay lengths track host rate.
        let left_delay = (rate * 0.0713).round() as usize;
        let right_delay = (rate * 0.0899).round() as usize;
        let l = self.delays[0]
            [(self.delay_cursor + DELAY_SIZE - left_delay.min(DELAY_SIZE - 1)) % DELAY_SIZE];
        let r = self.delays[1]
            [(self.delay_cursor + DELAY_SIZE - right_delay.min(DELAY_SIZE - 1)) % DELAY_SIZE];
        self.delays[0][self.delay_cursor] = dry[0] + l * 0.62 + r * 0.14;
        self.delays[1][self.delay_cursor] = dry[1] + r * 0.62 + l * 0.14;
        self.delay_cursor = (self.delay_cursor + 1) % DELAY_SIZE;
        let amount = values[5] as f32;
        [
            dry[0] * (1.0 - amount * 0.25) + l * amount * 0.45,
            dry[1] * (1.0 - amount * 0.25) + r * amount * 0.45,
        ]
    }
    fn synthesize<S: Source + ?Sized>(
        &mut self,
        source: &S,
        start: f64,
        end: f64,
        ratio: f64,
        values: &[f64; PARAM_COUNT],
        meters: Option<&SlopControls>,
    ) {
        let n = FFT_SIZES[self.resolution];
        for ch in 0..2 {
            for i in 0..n {
                self.work[i] = Complex {
                    re: read_interpolated(source, self.position + i as f64 * ratio, ch, start, end)
                        * self.windows[self.resolution][i],
                    im: 0.0,
                };
            }
            fft(&mut self.work[..n], &self.roots, false);
            for k in 0..=n / 2 {
                self.magnitudes[ch][k] =
                    (self.work[k].re * self.work[k].re + self.work[k].im * self.work[k].im).sqrt();
            }
        }
        let spread = values[4] as f32;
        for k in 1..n / 2 {
            let base = self.random_phase();
            let difference = self.random_phase() - std::f32::consts::PI;
            self.phases[k] = [base - difference * spread, base + difference * spread];
        }
        if let Some(controls) = meters {
            for i in 0..64 {
                // Logarithmic frequency groups; values are actual analysis energy.
                let low = (1.0_f64 + (n as f64 / 2.0 - 1.0).powf(i as f64 / 64.0)) as usize;
                let high = ((1.0_f64 + (n as f64 / 2.0 - 1.0).powf((i + 1) as f64 / 64.0))
                    as usize)
                    .max(low + 1)
                    .min(n / 2 + 1);
                let mut peak = 0.0_f32;
                for k in low.min(n / 2)..high {
                    peak =
                        peak.max((self.magnitudes[0][k] + self.magnitudes[1][k]) * 2.0 / n as f32);
                }
                controls.spectrum[i].store(peak.min(1.0).to_bits(), Ordering::Relaxed);
            }
        }
        let harmonic = values[3] as f32;
        for ch in 0..2 {
            self.work[0] = Complex::default();
            self.work[n / 2] = Complex::default();
            for k in 1..n / 2 {
                let mag = self.magnitudes[ch][k] * (1.0 - harmonic * 0.2)
                    + harmonic
                        * (self.magnitudes[ch][k / 2] * 0.2 + self.magnitudes[ch][k / 3] * 0.12);
                let (sin, cos) = self.phases[k][ch].sin_cos();
                let value = Complex {
                    re: mag * cos,
                    im: mag * sin,
                };
                self.work[k] = value;
                self.work[n - k] = Complex {
                    re: value.re,
                    im: -value.im,
                };
            }
            fft(&mut self.work[..n], &self.roots, true);
            for i in 0..n {
                let value = self.work[i].re * self.windows[self.resolution][i] * (4.0 / 3.0);
                self.overlap[ch][(self.cursor + i) % MAX_FFT] += value;
            }
        }
    }
}
#[inline]
fn read_interpolated<S: Source + ?Sized>(
    source: &S,
    position: f64,
    ch: usize,
    start: f64,
    end: f64,
) -> f32 {
    let pos = start + (position - start).rem_euclid(end - start);
    let first = pos.floor() as usize;
    let next = if first + 1 >= end as usize {
        start as usize
    } else {
        first + 1
    };
    let fraction = (pos - first as f64) as f32;
    let a = source.sample(first, ch);
    a + (source.sample(next, ch) - a) * fraction
}
fn fft(data: &mut [Complex], roots: &[Complex], inverse: bool) {
    let n = data.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let mut width = 2;
    while width <= n {
        let half = width / 2;
        for base in (0..n).step_by(width) {
            for k in 0..half {
                let mut root = roots[k * MAX_FFT / width];
                if inverse {
                    root.im = -root.im;
                }
                let odd = data[base + k + half].mul(root);
                let even = data[base + k];
                data[base + k] = Complex {
                    re: even.re + odd.re,
                    im: even.im + odd.im,
                };
                data[base + k + half] = Complex {
                    re: even.re - odd.re,
                    im: even.im - odd.im,
                };
            }
        }
        width *= 2;
    }
    if inverse {
        let scale = 1.0 / n as f32;
        for value in data {
            value.re *= scale;
            value.im *= scale;
        }
    }
}
