// Standalone self-test of hero2's effect upsampler.
//
//   rustc --edition 2021 --test sfx_upsample_selftest.rs -o tt_sfx_upsample.exe && ./tt_sfx_upsample.exe
//
// ★ This `include!`s the REAL source, not a copy — the kernel, the fade and the regenerated
// octave under test are the ones the app ships. `wie_android/src/sfx_upsample.rs` must
// therefore stay dependency-free (edition 2021, no let-chains). The effects themselves are
// exercised by `tools/ma2_render sfx` (they are game audio and stay out of the repository).

include!("wie_android/src/sfx_upsample.rs");

#[cfg(test)]
mod tests {
    use super::*;

    const IN: u32 = 8_000;
    const OUT: u32 = 48_000;

    fn tone(hz: f32, rate: u32, frames: usize, amp: f32) -> Vec<f32> {
        (0..frames).map(|n| amp * (2.0 * std::f32::consts::PI * hz * n as f32 / rate as f32).sin()).collect()
    }

    fn tone_i16(hz: f32, rate: u32, frames: usize, amp: f32) -> Vec<i16> {
        tone(hz, rate, frames, amp).iter().map(|s| (s * 32767.0) as i16).collect()
    }

    /// Amplitude of the `hz` component of `x` (a Hann-windowed single-bin transform).
    fn level(x: &[f32], rate: u32, hz: f32) -> f32 {
        let n = x.len() as f32;
        let (mut re, mut im, mut win) = (0.0f64, 0.0f64, 0.0f64);
        for (i, s) in x.iter().enumerate() {
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n).cos();
            let ph = 2.0 * std::f64::consts::PI * hz as f64 * i as f64 / rate as f64;
            re += (*s * w) as f64 * ph.cos();
            im += (*s * w) as f64 * ph.sin();
            win += w as f64;
        }
        (2.0 * (re * re + im * im).sqrt() / win) as f32
    }

    fn db(x: f32) -> f32 {
        20.0 * x.max(1e-9).log10()
    }

    /// The steady middle of a resampled tone: clear of both ends' ramps.
    fn middle(y: &[f32]) -> &[f32] {
        &y[y.len() / 4..y.len() * 3 / 4]
    }

    #[test]
    fn every_kernel_row_sums_to_one_and_the_first_is_an_impulse() {
        let k = sfx_kernel();
        assert_eq!(k.len(), SFX_PHASES + 1);
        for (p, row) in k.iter().enumerate() {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5, "row {p} sums to {sum}");
        }
        // Phase 0 is ON an input sample: that sample alone.
        assert!((k[0][SFX_HALF - 1] - 1.0).abs() < 1e-5);
        assert!(k[0].iter().enumerate().all(|(j, c)| j == SFX_HALF - 1 || c.abs() < 1e-5));
    }

    #[test]
    fn output_samples_on_input_instants_are_the_input() {
        let x = tone(700.0, IN, 400, 0.5);
        let y = sfx_resample(&x, IN, OUT);
        assert_eq!(y.len(), (400 + SFX_HALF) * 6);
        for i in 0..400 {
            assert!((y[i * 6] - x[i]).abs() < 1e-5, "input sample {i}");
        }
    }

    #[test]
    fn a_tone_keeps_its_level_across_the_band() {
        // The straight line this replaced lost 1.8 dB at 2 kHz and 5 dB at 3.3 kHz.
        for hz in [200.0, 1000.0, 2000.0, 3000.0, 3300.0] {
            let y = sfx_resample(&tone(hz, IN, 1600, 0.5), IN, OUT);
            let got = db(level(middle(&y), OUT, hz) / 0.5);
            assert!(got.abs() < 0.5, "{hz} Hz came through at {got:+.2} dB");
        }
    }

    #[test]
    fn a_tones_mirror_images_are_gone() {
        // A 1 kHz tone sampled at 8 kHz has images at 7, 9, 15, 17 kHz; a 3 kHz tone at 5 kHz.
        for (hz, images) in [(1000.0, [7000.0, 9000.0, 15000.0]), (3000.0, [5000.0, 11000.0, 13000.0])] {
            let y = sfx_resample(&tone(hz, IN, 1600, 0.5), IN, OUT);
            for image in images {
                let got = db(level(middle(&y), OUT, image) / 0.5);
                assert!(got < -70.0, "{hz} Hz left an image at {image} Hz, {got:.1} dB");
            }
        }
    }

    #[test]
    fn four_kilohertz_clips_and_other_device_rates_work_the_same() {
        for (in_rate, out_rate) in [(4_000, 48_000), (8_000, 44_100), (4_000, 44_100)] {
            let hz = in_rate as f32 / 8.0;
            let y = sfx_resample(&tone(hz, in_rate, 1600, 0.5), in_rate, out_rate);
            assert_eq!(y.len(), ((1600 + SFX_HALF) as u64 * out_rate as u64 / in_rate as u64) as usize);
            let got = db(level(middle(&y), out_rate, hz) / 0.5);
            assert!(got.abs() < 0.5, "{in_rate} -> {out_rate}: {got:+.2} dB");
            let image = db(level(middle(&y), out_rate, in_rate as f32 - hz) / 0.5);
            assert!(image < -70.0, "{in_rate} -> {out_rate}: image at {image:.1} dB");
        }
        // At or above the device rate there is nothing to interpolate between.
        let x = tone(1000.0, 48_000, 480, 0.5);
        assert_eq!(sfx_resample(&x, 48_000, 48_000), x);
        assert_eq!(sfx_resample(&x, 48_000, 24_000).len(), 240);
        assert!(sfx_resample(&[], IN, OUT).is_empty());
    }

    #[test]
    fn air_doubles_the_top_octave_and_nothing_else() {
        // 3 kHz is in the top octave (2–4 kHz): its double, 6 kHz, appears.
        let mut y = sfx_resample(&tone(3000.0, IN, 3200, 0.5), IN, OUT);
        sfx_add_air(&mut y, IN, OUT, SFX_AIR);
        let added = db(level(middle(&y), OUT, 6000.0) / 0.5);
        assert!((-30.0..-8.0).contains(&added), "6 kHz at {added:.1} dB under the 3 kHz tone");
        // …and the tone itself is untouched.
        assert!(db(level(middle(&y), OUT, 3000.0) / 0.5).abs() < 0.5);

        // 300 Hz is far below it: nothing is added anywhere above the band.
        let mut y = sfx_resample(&tone(300.0, IN, 3200, 0.5), IN, OUT);
        let before = y.clone();
        sfx_add_air(&mut y, IN, OUT, SFX_AIR);
        for hz in [4500.0, 6000.0, 7500.0] {
            let got = db(level(middle(&y), OUT, hz) / 0.5);
            assert!(got < -60.0, "a 300 Hz tone grew {got:.1} dB at {hz} Hz");
        }
        let change = y.iter().zip(&before).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(change < 0.5 * 0.02, "a 300 Hz tone was changed by {change}");
    }

    #[test]
    fn air_is_off_where_it_has_no_room_and_at_zero() {
        let clean = sfx_resample(&tone(3000.0, IN, 800, 0.5), IN, OUT);
        let mut y = clean.clone();
        sfx_add_air(&mut y, IN, OUT, 0.0);
        assert_eq!(y, clean);
        // A 22.05 kHz clip on a 44.1 kHz device: its next octave would be the device's last.
        let mut y = clean.clone();
        sfx_add_air(&mut y, 22_050, 44_100, SFX_AIR);
        assert_eq!(y, clean);
        // An 8 kHz clip needs 8 kHz of room: a 12 kHz device has 6.
        let mut y = clean.clone();
        sfx_add_air(&mut y, 8_000, 12_000, SFX_AIR);
        assert_eq!(y, clean);
    }

    #[test]
    fn silence_stays_silence() {
        let out = sfx_clip(&[0i16; 800], 1, IN, OUT, 0.4);
        assert_eq!(out.len(), (800 + SFX_HALF) * 6 * 2);
        assert!(out.iter().all(|s| *s == 0.0));
        assert!(sfx_clip(&[], 1, IN, OUT, 0.4).is_empty());
    }

    #[test]
    fn a_clip_is_stereo_scaled_and_ends_on_zero() {
        // A clip that stops at full swing — the tick the fade is there for.
        let pcm = vec![16_000i16; 800];
        let out = sfx_clip(&pcm, 1, IN, OUT, 0.4);
        assert!(out.chunks(2).all(|f| f[0] == f[1]), "mono plays on both sides");
        // The body is the clip at the gain…
        let body = out[2 * 6 * 400];
        assert!((body - 0.4 * 16_000.0 / 32768.0).abs() < 0.01, "body {body}");
        // …the end is silence, and it gets there without a step.
        let tail = &out[out.len() - 2 * 6 * SFX_HALF..];
        assert!(tail.iter().all(|s| s.abs() < 0.01), "the clip does not end on silence");
        let biggest_step = out.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).skip(6 * 100).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(biggest_step < 0.02, "a step of {biggest_step} at the end");
        // Gain is linear.
        let louder = sfx_clip(&pcm, 1, IN, OUT, 0.8);
        assert!((louder[2 * 6 * 400] - 2.0 * body).abs() < 1e-4);
    }

    #[test]
    fn a_stereo_clip_keeps_its_sides_apart() {
        let (l, r) = (tone_i16(500.0, IN, 800, 0.5), tone_i16(1500.0, IN, 800, 0.25));
        let pcm: Vec<i16> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
        let out = sfx_clip(&pcm, 2, IN, OUT, 1.0);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        let right: Vec<f32> = out.chunks(2).map(|f| f[1]).collect();
        assert!(db(level(middle(&left), OUT, 500.0) / 0.5).abs() < 0.5);
        assert!(db(level(middle(&left), OUT, 1500.0)) < -60.0);
        assert!(db(level(middle(&right), OUT, 1500.0) / 0.25).abs() < 0.5);
    }

    #[test]
    fn a_full_scale_clip_cannot_overshoot_by_much() {
        // The worst an 8 kHz clip can be: full-scale alternation is exactly at the band edge
        // and is filtered away; a full-scale square at 1 kHz overshoots a band-limited
        // reconstruction by Gibbs' 9 % and no more. The bus's soft limiter has that headroom.
        let pcm: Vec<i16> = (0..800).map(|n| if (n / 4) % 2 == 0 { 32767 } else { -32768 }).collect();
        let out = sfx_clip(&pcm, 1, IN, OUT, 1.0);
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 1.6, "peak {peak}");
    }
}
