// Standalone self-test of the audio master-bus math.
//
//   rustc --edition 2021 --test audio_mix_selftest.rs -o tt_audio.exe && ./tt_audio.exe
//
// ★ This `include!`s the REAL source, not a copy — so the gain constants and the
// limiter curve under test are exactly the ones the app ships.
// `wie_android/src/audio_mix.rs` must therefore stay dependency-free (same contract as
// endpoint.rs / buffs.rs / tls_buf.rs; see CLAUDE.md).
//
// Why standalone at all: `cargo test -p wie_android` dies at startup on this Windows
// host because of the linked winit/android deps.

include!("wie_android/src/audio_mix.rs");

// ── the volume law ───────────────────────────────────────────────────────────
// A mirror of `wie_backend::volume_gain`, which cannot be `include!`d here (it lives in
// the vendored no_std crate alongside the trait it serves). `law_matches_wie_backend`
// below pins it to that source text, so the two cannot drift apart silently — this is
// the mistake `items_selftest.rs` made and 44차 paid for.
fn volume_gain(volume: u8) -> f32 {
    let v = volume.min(100) as f32 / 100.0;
    v * v
}

#[cfg(test)]
mod law {
    use super::*;

    #[test]
    fn law_matches_wie_backend() {
        // Read the real definition and check the body still says what the mirror says.
        let src = std::fs::read_to_string("vendor/wie/wie_backend/src/audio_sink.rs")
            .expect("run this from the repo root: rustc --test audio_mix_selftest.rs");
        let body = src
            .split("pub fn volume_gain(volume: u8) -> f32 {")
            .nth(1)
            .expect("wie_backend::volume_gain has been renamed or removed — update this mirror");
        let body: String = body.split("\n}").next().unwrap().split_whitespace().collect();
        assert_eq!(
            body, "letv=volume.min(100)asf32/100.0;v*v",
            "wie_backend::volume_gain changed; update the mirror in this file"
        );
    }

    #[test]
    fn the_five_in_game_steps() {
        // Hero3 sends level*20 (probe-confirmed: `MC_mdaClipSetVolume(clip, 60)` for
        // both buses at the default settings).
        let want = [0.0f32, 0.04, 0.16, 0.36, 0.64, 1.0];
        for (level, want) in want.iter().enumerate() {
            let got = volume_gain((level * 20) as u8);
            assert!((got - want).abs() < 1e-6, "level {level}: got {got}, want {want}");
        }
    }

    #[test]
    fn both_buses_answer_to_the_same_law() {
        // The whole point of the fix: at any given level the two buses are scaled by
        // the SAME number, so "배경음 3 / 효과음 3" means one thing and not two.
        //
        // What it used to be: PCM got a LINEAR vol/100, MIDI got velocity*vol/100 which
        // the SoundFont's concave velocity->attenuation curve turns into roughly the
        // square of that. Level 3 therefore meant -4.4 dB on one bus and about -8.9 dB
        // *plus* a 14 dB synth-gain deficit on the other.
        let level3 = volume_gain(60);
        let old_pcm_linear = 0.6f32;
        assert!(level3 < old_pcm_linear, "the effect bus must get quieter at mid levels");
        let db = |v: f32| 20.0 * v.log10();
        assert!((db(level3) - db(old_pcm_linear) + 4.44).abs() < 0.05);
    }

    #[test]
    fn full_scale_effect_plus_music_cannot_clip_the_device() {
        // The end-to-end guarantee for "효과음이 나면 배경음이 끊긴다": four of the
        // game's fourteen effects decode to a literal 0 dBFS peak, and the loudest
        // measured BGM peak at unity synth gain is 1.182 (bgm3_mf).
        for level in [0u8, 20, 40, 60, 80, 100] {
            let g = volume_gain(level);
            let sfx = 1.0 * g * SFX_TRIM;
            let bgm = 1.182 * SYNTH_GAIN * g;
            let out = soft_limit(sfx + bgm);
            assert!(out.abs() < 1.0, "level {level}: {} -> {out} must stay inside full scale", sfx + bgm);
        }
    }

    #[test]
    fn music_is_far_louder_than_it_was_at_every_level() {
        // Old chain: synth gain 0.2, and the level applied to velocity (~squared).
        // New chain: synth gain 1.0, and the level applied to the output (squared).
        // The velocity path and the output path land on the same exponent, so what is
        // left is the 5x of synth gain that was being thrown away.
        for level in [20u8, 40, 60, 80, 100] {
            let old = 0.2 * volume_gain(level);
            let new = SYNTH_GAIN * volume_gain(level);
            let gain_db = 20.0 * (new / old).log10();
            assert!((gain_db - 13.98).abs() < 0.01, "level {level}: {gain_db:+.2} dB");
        }
    }
}
