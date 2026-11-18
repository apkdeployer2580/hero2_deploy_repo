// hero2's sound effects, from the rate the game ships them at to the rate the device plays.
//
// ★ 2026-10-02 ("효과음도 업스케일링"). The effects are 4-bit ADPCM at 8 kHz (one at 4 kHz) —
// `wie_backend::system::atr` decodes them — and they used to reach the bus through rodio's
// LINEAR resampler. A straight line between samples is a weak low-pass with a weak stop band:
// it droops the top of the clip's own band (−6 dB at 3.5 kHz) and leaves the mirror images of
// that band above 4 kHz only 10–20 dB down, loudest where the clip is loudest. This file does
// the three things a clip needs instead, once per clip on the audio thread (never in the
// callback):
//
//   1. band-limited interpolation — a Kaiser-windowed sinc, 32 taps at the clip's rate, so
//      the clip's band arrives flat and its images do not arrive at all;
//   2. a 4 ms fade at the clip's end, so a clip the game authored to stop a little off zero
//      (effect 43 ends at −1,377) does not end on a tick;
//   3. "air" — the octave above the clip's band, regenerated from the clip's own top octave
//      by rectification (the classic high-frequency regeneration for narrow-band audio). It
//      follows the clip's envelope, is harmonic to what it comes from, and a clip with
//      nothing in its top octave gets nothing. [`SFX_AIR`] is how much; `0.0` is none.
//
// No dependencies, edition 2021 — `sfx_upsample_selftest.rs`, `tools/ma2_render` and
// `wie_probe` `include!` this file.

use std::sync::OnceLock;

/// Interpolation kernel half-width, in input samples.
const SFX_HALF: usize = 16;
const SFX_TAPS: usize = SFX_HALF * 2;
/// Sub-sample positions the kernel is tabulated at (rows are blended linearly between).
const SFX_PHASES: usize = 256;
/// Kaiser window shape: about −80 dB of stop band.
const SFX_KAISER_BETA: f64 = 8.0;
/// The fade at the end of a clip, in ms.
const SFX_TAIL_FADE_MS: u32 = 4;
/// How much of the regenerated octave is added. Measured over hero2's eleven effects at this
/// value the new octave sits 11–16 dB under the clip's own top octave — a continuing roll-off,
/// not a shelf (the straight line's images sat 8–10 dB under it, and were not harmonic). ★ The knob to turn by ear; `0.0` leaves the clip band-limited and nothing else.
pub const SFX_AIR: f32 = 0.6;
/// Clips above this rate have their top octave where there is nothing left to add.
const SFX_AIR_MAX_RATE: u32 = 16_000;

/// Modified Bessel function of the first kind, order 0 (the Kaiser window's).
fn sfx_bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..64 {
        term *= (half / k as f64) * (half / k as f64);
        sum += term;
        if term < sum * 1e-14 {
            break;
        }
    }
    sum
}

/// `SFX_PHASES + 1` rows of `SFX_TAPS` coefficients. Row `p`, tap `j` weighs the input sample
/// `SFX_HALF - 1 - j + p / SFX_PHASES` samples before the output position. Every row sums to 1.
fn sfx_kernel() -> &'static [[f32; SFX_TAPS]] {
    static KERNEL: OnceLock<Vec<[f32; SFX_TAPS]>> = OnceLock::new();
    KERNEL.get_or_init(|| {
        let i0_beta = sfx_bessel_i0(SFX_KAISER_BETA);
        (0..=SFX_PHASES)
            .map(|p| {
                let mut row = [0.0f64; SFX_TAPS];
                for (j, tap) in row.iter_mut().enumerate() {
                    let u = p as f64 / SFX_PHASES as f64 + (SFX_HALF - 1) as f64 - j as f64;
                    let x = u / SFX_HALF as f64;
                    if x.abs() >= 1.0 {
                        continue;
                    }
                    let sinc = if u.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * u).sin() / (std::f64::consts::PI * u) };
                    *tap = sinc * sfx_bessel_i0(SFX_KAISER_BETA * (1.0 - x * x).sqrt()) / i0_beta;
                }
                let sum: f64 = row.iter().sum();
                let mut out = [0.0f32; SFX_TAPS];
                for (o, tap) in out.iter_mut().zip(row) {
                    *o = (tap / sum) as f32;
                }
                out
            })
            .collect()
    })
}

/// One channel from `in_rate` to `out_rate`, band-limited to the input's own band. The output
/// runs [`SFX_HALF`] input samples past the input's end, so the last sample rings out.
///
/// A clip already at or above the device rate (none of hero2's) is interpolated linearly,
/// as before.
pub fn sfx_resample(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if input.is_empty() || in_rate == 0 || out_rate == 0 {
        return Vec::new();
    }
    let (in_hz, out_hz) = (in_rate as u64, out_rate as u64);
    if in_rate >= out_rate {
        let out_len = (input.len() as u64 * out_hz / in_hz) as usize;
        return (0..out_len as u64)
            .map(|n| {
                let pos = n * in_hz;
                let i = (pos / out_hz) as usize;
                let frac = (pos % out_hz) as f32 / out_rate as f32;
                let a = input[i];
                let b = input.get(i + 1).copied().unwrap_or(a);
                a + (b - a) * frac
            })
            .collect();
    }
    let kernel = sfx_kernel();
    let out_len = ((input.len() + SFX_HALF) as u64 * out_hz / in_hz) as usize;
    let mut out = Vec::with_capacity(out_len);
    for n in 0..out_len as u64 {
        let pos = n * in_hz;
        let centre = (pos / out_hz) as isize;
        let phase = (pos % out_hz) as f32 / out_rate as f32 * SFX_PHASES as f32;
        let p = (phase as usize).min(SFX_PHASES - 1);
        let blend = phase - p as f32;
        let (row, next) = (&kernel[p], &kernel[p + 1]);
        // Tap j reads input[first + j]; only the taps that land inside the clip.
        let first = centre - (SFX_HALF as isize - 1);
        let from = (-first).max(0) as usize;
        let to = (input.len() as isize - first).clamp(0, SFX_TAPS as isize) as usize;
        let mut acc = 0.0f32;
        for j in from..to {
            acc += input[(first + j as isize) as usize] * (row[j] + (next[j] - row[j]) * blend);
        }
        out.push(acc);
    }
    out
}

/// A second-order filter section (transposed direct form II).
struct SfxBiquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl SfxBiquad {
    /// `high`: a high-pass at `hz`, else a low-pass; `q` 0.7071 is Butterworth.
    fn new(high: bool, hz: f32, rate: f32, q: f32) -> Self {
        let w = 2.0 * std::f32::consts::PI * hz / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b = if high {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Self {
            b: [b[0] / a0, b[1] / a0, b[2] / a0],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// Add the octave above a resampled clip's band (`in_rate / 2` up to `in_rate`), regenerated
/// from the clip's top octave: that octave, rectified — which doubles every frequency in it —
/// and filtered down to the new octave alone. `amount` scales it; see [`SFX_AIR`].
pub fn sfx_add_air(signal: &mut [f32], in_rate: u32, out_rate: u32, amount: f32) {
    if amount <= 0.0 || in_rate == 0 || in_rate > SFX_AIR_MAX_RATE || in_rate as u64 * 2 > out_rate as u64 {
        return;
    }
    let (edge, rate) = (in_rate as f32 / 2.0, out_rate as f32);
    let mut top_octave = SfxBiquad::new(true, edge / 2.0, rate, std::f32::consts::FRAC_1_SQRT_2);
    // Fourth-order Butterworth high-pass at the clip's band edge: what rectifying puts back
    // INSIDE the band (the envelope, the difference tones) must not come through.
    let mut above = [SfxBiquad::new(true, edge, rate, 0.541_196_1), SfxBiquad::new(true, edge, rate, 1.306_563)];
    let mut ceiling = SfxBiquad::new(false, edge * 2.0, rate, std::f32::consts::FRAC_1_SQRT_2);
    for s in signal.iter_mut() {
        let mut new_octave = top_octave.run(*s).abs();
        for stage in above.iter_mut() {
            new_octave = stage.run(new_octave);
        }
        *s += amount * ceiling.run(new_octave);
    }
}

/// One decoded effect — interleaved 16-bit PCM, `channels` wide, at `in_rate` — as the bus
/// plays it: interleaved stereo `f32` at `out_rate`, scaled by `gain`.
pub fn sfx_clip(samples: &[i16], channels: u8, in_rate: u32, out_rate: u32, gain: f32) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    let frames = samples.len() / channels;
    let fade = ((in_rate as u64 * SFX_TAIL_FADE_MS as u64 / 1000) as usize).min(frames);
    let side = |c: usize| {
        let mut x: Vec<f32> = (0..frames).map(|i| samples[i * channels + c] as f32 / 32768.0).collect();
        for (i, s) in x[frames - fade..].iter_mut().enumerate() {
            *s *= 0.5 * (1.0 + (std::f32::consts::PI * (i + 1) as f32 / fade as f32).cos());
        }
        let mut y = sfx_resample(&x, in_rate, out_rate);
        sfx_add_air(&mut y, in_rate, out_rate, SFX_AIR);
        y
    };
    let left = side(0);
    let right = if channels > 1 { Some(side(1)) } else { None };
    let mut out = Vec::with_capacity(left.len() * 2);
    for (i, l) in left.iter().enumerate() {
        out.push(l * gain);
        out.push(right.as_ref().map_or(*l, |r| r[i]) * gain);
    }
    out
}
