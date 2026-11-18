// Master-bus math for the Android audio path. Pure arithmetic, NO dependencies —
// `audio_mix_selftest.rs` at the repo root `include!`s this very file, so the numbers
// below are the ones that ship (see CLAUDE.md: `cargo test -p wie_android` cannot run
// on the Windows host).
//
// ── WHY THESE NUMBERS EXIST ──────────────────────────────────────────────────
// Player report 2026-08-16, three symptoms that turned out to be one bug:
//   ① 배경음이 휴대폰 볼륨을 최대로 올려야 겨우 들린다
//   ② 배경음 3단계와 효과음 3단계의 음량 차이가 너무 크다
//   ③ 효과음이 나는 스킬을 쓰면 배경음이 끊긴다
//
// Hero3 ships its two buses as physically different SMAF chunks — measured from the
// game's own jar, `res/snd/bgm*_mf` are `MTR` score tracks (MIDI, rendered here by
// oxisynth) and `res/snd/sd*_mf` are `ATR` audio tracks (Yamaha ADPCM, played as PCM).
// They never met in the same code path, so nothing ever compared their levels.
//
// Rendered with the shipping `GeneralUser.sf3` (20 s per song) at the game's DEFAULT
// setting — the probe shows `MC_mdaClipSetVolume(clip, 60)` for BOTH buses, i.e. both
// sliders on 3:
//
//     bus / file           peak dBFS     RMS dBFS
//     BGM  bgm0_mf           -25.2         -42.5
//     BGM  bgm3_mf           -20.6         -38.7
//     BGM  bgm13_mf          -28.2         -47.8
//     BGM  bgm16_mf          -28.6         -43.0
//     SFX  sd008_mf           -4.4          -8.3
//     SFX  sd004_mf (크리티컬) -4.4         -11.2
//     SFX  sd009_mf           -4.4         -15.0
//     SFX  sd002_mf          -10.4         -24.2
//
// A **27-39 dB** gap from one and the same setting. That is ①, ② and ③ at once: the
// music is 30 dB down so the phone has to be at maximum (①); the effect is 30 dB above
// it at the same slider position (②); and an SFX that already peaks at 0 dBFS *before*
// the slider both masks the music and pushes the summed mix past full scale, where the
// device clamps and the music is not merely buried but arithmetically destroyed (③).
//
// Three causes, three constants below.

/// Synth output gain — `0.2` -> `1.0`.
///
/// `0.2` is oxisynth's inherited FluidSynth default, chosen for a *desktop synth*
/// playing arbitrary MIDI with 100+ voices, where the sum has to stay clear of full
/// scale. Hero3's scores are 16-channel game MIDI at ~12 note-ons a second: measured
/// peaks across the four songs above at unity gain are 0.45 / 0.49 / 0.66 / 1.18, so
/// `0.2` was throwing away ~14 dB the mix never needed to give up. Unity is the right
/// baseline for this content; `bgm3_mf`'s 1.18 peak is what [`soft_limit`] is for.
pub const SYNTH_GAIN: f32 = 1.0;

/// PCM (SFX) bus trim.
///
/// The game's effect samples are *brickwalled*: four of the fourteen decode to a
/// literal 0 dBFS peak and `sd008_mf` sits at -3.9 dBFS RMS. Nothing downstream can
/// mix on top of a full-scale source, so the effect bus gives up fixed headroom and the
/// music bus keeps unity, instead of the 27-39 dB spread it was.
///
/// ★ `0.5` (-6.0 dB) -> `0.4` (-8.0 dB) after the player listened to both buses on 3:
/// "효과음이 상대적으로 좀 크게 들려서 … 아주 조금만 낮추는게 좋을것같아". That is a
/// -2 dB nudge, which moves the measured RMS gap at matched sliders from +4.6 dB to
/// **+2.6 dB** — effects still cut through sustained music, but no longer sit on top
/// of it. Tune here and nowhere else; the measurement table above is the reference.
pub const SFX_TRIM: f32 = 0.4;

/// The effect bus trims the settings row offers, quietest first.
///
/// ★★ WHY THIS IS A SETTING AND NOT ANOTHER GUESS (hero3 2026-08-27, ported
/// 2026-09-18). [`SFX_TRIM`] was tuned by ear on ONE phone and then a player on another
/// reported the opposite symptom: effects too quiet. Both reports are right. The trim is
/// a fixed −8.0 dB at EVERY slider step — `volume_gain` is the same square law on both
/// buses, so the effect bus sits exactly 8 dB under the music bus whatever the player
/// does — and 8 dB of a loud phone's headroom is not 8 dB of a quiet one's. A third
/// global number would just move the complaint to a third handset.
///
/// So the number the ear picks is the number the ear can change. Five steps of 3 dB,
/// centred on the shipped value: **−6 / −3 / 0 / +3 / +6 dB** relative to it. The
/// default index is [`SFX_LEVEL_DEFAULT`] and its entry IS `SFX_TRIM`, so a player who
/// never opens the row hears exactly what shipped — that is the whole point of the
/// choice, and the reason the tuned phone cannot regress.
///
/// ⛔ The top step stops at 0.8 (−1.9 dB), not 1.0. The game's loudest effect samples
/// decode near 0 dBFS; at unity they leave the mix no headroom at all and [`soft_limit`]
/// starts eating the music underneath them.
pub const SFX_LEVELS: [f32; 5] = [0.2, 0.283, 0.4, 0.566, 0.8];

/// Index into [`SFX_LEVELS`] that reproduces [`SFX_TRIM`] exactly.
pub const SFX_LEVEL_DEFAULT: u8 = 2;

/// The trim for a stored level, clamped. Out-of-range (a hand-edited file, a future
/// build that shrank the table) falls back to the shipped default rather than to
/// silence or to full scale — both of which are worse than "unchanged".
pub fn sfx_trim(level: u8) -> f32 {
    match SFX_LEVELS.get(level as usize) {
        Some(&t) => t,
        None => SFX_LEVELS[SFX_LEVEL_DEFAULT as usize],
    }
}

/// The row's own label for a level: how far it is from what shipped, in dB.
pub fn sfx_level_label(level: u8) -> &'static str {
    match level {
        0 => "-6dB",
        1 => "-3dB",
        2 => "기본",
        3 => "+3dB",
        4 => "+6dB",
        _ => "기본",
    }
}

/// Next level in the row's cycle, wrapping — the same grammar the PAD HUD row uses.
pub fn sfx_level_next(level: u8) -> u8 {
    if usize::from(level) + 1 >= SFX_LEVELS.len() { 0 } else { level + 1 }
}

/// The shipped default must BE the shipped constant, or a player who never opens the
/// row would hear a different mix than every build before the row existed.
const _: () = assert!(SFX_LEVELS[SFX_LEVEL_DEFAULT as usize] == SFX_TRIM);

/// Where [`soft_limit`] stops being a straight wire, in linear amplitude (-3.1 dBFS).
pub const LIMIT_KNEE: f32 = 0.7;

/// Master soft limiter — the last thing the mixed BGM+SFX bus passes through.
///
/// Below [`LIMIT_KNEE`] this is the identity, so ordinary play is bit-for-bit
/// untouched. Above it the curve bends asymptotically to 1.0 and **can never exceed
/// it**, which is the point: AAudio hard-clamps float PCM to [-1, 1], and hard
/// clipping a loud effect summed with quiet music does not just distort the effect —
/// it deletes the music inside every clipped sample. That is symptom ③.
///
/// Stateless (no attack/release, so no pumping and nothing to get wrong in the
/// real-time callback) and C¹-continuous at the knee: the derivative of the upper
/// branch at `over = 0` is exactly 1, so there is no audible corner where it engages.
pub fn soft_limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= LIMIT_KNEE {
        return x;
    }
    let head = 1.0 - LIMIT_KNEE;
    let over = a - LIMIT_KNEE;
    let y = LIMIT_KNEE + head * (over / (over + head));
    if x < 0.0 { -y } else { y }
}

// ── SFX IN FLIGHT (2026-10-05) ───────────────────────────────────────────────
// Player report: "지옥불 효과음 소리가 조금 큰거같은데 이거 조금 줄여줘".
//
// The file is not loud — the bus plays it sixteen times at once. 지옥불 (and 빛의파동 /
// 대지분노 — the same ring of `ce` effects) asks for effect 43 (속성공격합, 967 ms) once
// per tile hit: probe `--h2-beast-skill … --audio-log` counts **35 starts in 2.4 s, one
// every 60 ms**. The phone had one player, so each start cut the last one off. This bus
// plays every clip concurrently (on purpose: the old queue made rapid hits drain long
// after the action), so up to 16 copies overlapped. Measured on that very timeline
// (short-term loudness above 150 Hz, app path, 효과음 크기 기본):
//
//     one hit                    -21.1 dB    peak  -9.7 dBFS
//     지옥불, every copy kept     -11.6 dB    peak  -1.1 dBFS   ← into the limiter, music squashed
//     at most 3 copies           -14.9 dB    peak  -1.7 dBFS
//     at most 2 copies           -16.4 dB    peak  -2.9 dBFS   ← this
//     at most 1 copy (phone)     -19.5 dB    peak  -7.4 dBFS
//
// Two copies takes ~5 dB off and keeps the burst under the limiter's knee, while every
// start still has its attack — the rapid "타다닥" stays, only the pile-up goes.

/// How many copies of ONE effect the bus plays at once. A newer copy takes the place of
/// the oldest, which fades out over [`SFX_STEAL_FADE_MS`]. Different effects never take
/// each other's place.
pub const SFX_SAME_MAX: usize = 2;

/// How long a copy that gives up its place takes to go silent. Cutting it in one sample
/// clicks; 10 ms is too short to hear as a fade.
pub const SFX_STEAL_FADE_MS: u32 = 10;

/// What makes two clips "the same effect": the samples the game handed over (FNV-1a 64),
/// with their format. The game's own effect slider is already in the samples, so the same
/// effect at two slider positions counts as two — harmless, the slider does not move mid-burst.
pub fn sfx_key(channels: u8, sampling_rate: u32, samples: &[i16]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: u8| {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    eat(channels);
    sampling_rate.to_le_bytes().into_iter().for_each(&mut eat);
    for s in samples {
        s.to_le_bytes().into_iter().for_each(&mut eat);
    }
    h
}

/// One effect in flight, already in the bus format (interleaved stereo at the device rate).
struct SfxClip {
    key: u64,
    data: Vec<f32>,
    pos: usize,
    /// Frames of fade left once a newer copy took this one's place.
    fading: Option<u32>,
}

/// The effects the bus is playing. Kept in arrival order, so the first match is the oldest.
pub struct SfxClips {
    clips: Vec<SfxClip>,
    fade_frames: u32,
}

impl SfxClips {
    pub fn new(sample_rate: u32) -> Self {
        Self { clips: Vec::new(), fade_frames: (sample_rate * SFX_STEAL_FADE_MS / 1000).max(1) }
    }

    pub fn push(&mut self, key: u64, data: Vec<f32>) {
        if data.is_empty() {
            return;
        }
        let mut same = self.clips.iter().filter(|c| c.key == key && c.fading.is_none()).count();
        for clip in self.clips.iter_mut() {
            if same < SFX_SAME_MAX {
                break;
            }
            if clip.key == key && clip.fading.is_none() {
                clip.fading = Some(self.fade_frames);
                same -= 1;
            }
        }
        self.clips.push(SfxClip { key, data, pos: 0, fading: None });
    }

    pub fn clear(&mut self) {
        self.clips.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.clips.len()
    }

    /// Add every clip's next `buf.len()` samples (interleaved stereo) into `buf`.
    pub fn mix_into(&mut self, buf: &mut [f32]) {
        let fade = self.fade_frames as f32;
        self.clips.retain_mut(|clip| {
            let rest = &clip.data[clip.pos..];
            let n = rest.len().min(buf.len());
            match clip.fading.as_mut() {
                None => {
                    for (o, s) in buf[..n].iter_mut().zip(rest) {
                        *o += s;
                    }
                }
                Some(left) => {
                    for (o, s) in buf[..n].chunks_exact_mut(2).zip(rest.chunks_exact(2)) {
                        if *left == 0 {
                            break;
                        }
                        let g = *left as f32 / fade;
                        o[0] += s[0] * g;
                        o[1] += s[1] * g;
                        *left -= 1;
                    }
                    if *left == 0 {
                        return false;
                    }
                }
            }
            clip.pos += n;
            clip.pos < clip.data.len()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(v: f32) -> f32 {
        20.0 * v.log10()
    }

    #[test]
    fn limiter_is_transparent_below_the_knee() {
        for &x in &[0.0f32, 0.01, 0.25, 0.5, 0.69, LIMIT_KNEE] {
            assert_eq!(soft_limit(x), x, "positive {x}");
            assert_eq!(soft_limit(-x), -x, "negative {x}");
        }
    }

    #[test]
    fn limiter_never_reaches_full_scale() {
        // The whole reason it exists: AAudio clamps, and a clamp destroys the quiet
        // signal riding on the loud one.
        for i in 0..2000 {
            let x = i as f32 * 0.01; // up to 20.0, far past anything reachable
            let y = soft_limit(x);
            assert!(y < 1.0, "soft_limit({x}) = {y} must stay below full scale");
            assert!(soft_limit(-x) > -1.0);
        }
    }

    #[test]
    fn limiter_is_monotonic_and_odd() {
        let mut prev = f32::NEG_INFINITY;
        for i in 0..500 {
            let x = i as f32 * 0.01;
            let y = soft_limit(x);
            assert!(y > prev, "must be monotonic at {x}");
            prev = y;
            assert!((soft_limit(-x) + y).abs() < 1e-6, "must be odd at {x}");
        }
    }

    #[test]
    fn limiter_is_smooth_at_the_knee() {
        // No corner: the slope approaching the knee from above is ~1, so the limiter
        // fades in rather than switching on.
        let e = 1e-4;
        let slope = (soft_limit(LIMIT_KNEE + e) - soft_limit(LIMIT_KNEE)) / e;
        assert!((slope - 1.0).abs() < 1e-2, "slope at knee = {slope}");
    }

    #[test]
    fn worst_case_mix_survives_the_limiter() {
        // Measured worst case: `bgm3_mf` peaks at 1.182 at unity synth gain, and a
        // full-scale effect adds SFX_TRIM on top of it.
        let sum = 1.182 + 1.0 * SFX_TRIM;
        let out = soft_limit(sum);
        assert!(out < 1.0 && out > 0.9, "worst-case mix {sum} -> {out}");
    }

    #[test]
    fn the_two_buses_end_up_within_a_sane_distance() {
        // Peaks measured from the shipping assets, per bus, at full volume.
        let bgm_peak_at_unity = 0.656_f32; // bgm0_mf, the title theme
        let sfx_peak_at_unity = 1.0_f32; // sd004_mf / sd008_mf, brickwalled
        let gap = db(sfx_peak_at_unity * SFX_TRIM) - db(bgm_peak_at_unity * SYNTH_GAIN);
        // Before this change the same comparison was ~+24 dB (0.2 synth gain, and the
        // music additionally lost ~9 dB to velocity scaling at level 3).
        assert!(gap.abs() < 6.0, "buses should sit within 6 dB at the peak, got {gap:+.1} dB");
    }

    /// A stereo clip of `frames` frames, every sample `v`.
    fn flat(frames: usize, v: f32) -> Vec<f32> {
        vec![v; frames * 2]
    }

    #[test]
    fn two_copies_of_one_effect_play_in_full() {
        let mut clips = SfxClips::new(48_000);
        clips.push(7, flat(100, 0.1));
        clips.push(7, flat(100, 0.1));
        let mut buf = [0.0f32; 20];
        clips.mix_into(&mut buf);
        assert!(buf.iter().all(|s| (s - 0.2).abs() < 1e-6), "{buf:?}");
    }

    #[test]
    fn a_third_copy_fades_the_oldest_out_without_a_jump() {
        let rate = 48_000;
        let fade = (rate * SFX_STEAL_FADE_MS / 1000) as usize;
        let mut clips = SfxClips::new(rate);
        clips.push(7, flat(10_000, 0.1));
        clips.push(7, flat(10_000, 0.1));
        let mut buf = vec![0.0f32; 64];
        clips.mix_into(&mut buf);
        clips.push(7, flat(10_000, 0.1));
        let mut out = vec![0.0f32; (fade + 64) * 2];
        clips.mix_into(&mut out);
        // First frame: the old copy is still whole, plus the two others.
        assert!((out[0] - 0.3).abs() < 1e-6, "{}", out[0]);
        // It goes down a step of at most one fade-th of a copy per frame — no click.
        for f in out.chunks_exact(2).collect::<Vec<_>>().windows(2) {
            assert!(f[0][0] - f[1][0] < 0.1 / fade as f32 + 1e-6);
        }
        // And after the fade, exactly two copies are left.
        assert!((out[out.len() - 2] - 0.2).abs() < 1e-6, "{}", out[out.len() - 2]);
        assert_eq!(clips.len(), SFX_SAME_MAX);
    }

    #[test]
    fn different_effects_do_not_take_each_others_place() {
        let mut clips = SfxClips::new(48_000);
        for key in 0..5 {
            clips.push(key, flat(2_000, 0.1));
        }
        let mut buf = vec![0.0f32; 2_000];
        clips.mix_into(&mut buf);
        assert!((buf[1_998] - 0.5).abs() < 1e-5, "{}", buf[1_998]);
        assert_eq!(clips.len(), 5);
    }

    #[test]
    fn the_hellfire_burst_never_piles_up() {
        // probe: effect 43 (967 ms) started 35 times, one every 60 ms.
        let rate = 48_000u32;
        let clip_frames = rate as usize * 967 / 1000;
        let step = rate as usize * 60 / 1000;
        let mut clips = SfxClips::new(rate);
        let mut buf = vec![0.0f32; step * 2];
        let mut loudest = 0.0f32;
        for _ in 0..35 {
            clips.push(43, flat(clip_frames, 0.1));
            buf.fill(0.0);
            clips.mix_into(&mut buf);
            loudest = buf.iter().fold(loudest, |m, s| m.max(*s));
        }
        // Unbounded, 16 copies would be playing here (1.6).
        assert!(loudest <= 0.1 * SFX_SAME_MAX as f32 + 0.1 + 1e-6, "{loudest}");
        assert!(clips.len() <= SFX_SAME_MAX);
    }

    #[test]
    fn the_key_is_the_samples_and_their_format() {
        let a = [1i16, -2, 3];
        assert_eq!(sfx_key(1, 8_000, &a), sfx_key(1, 8_000, &a));
        assert_ne!(sfx_key(1, 8_000, &a), sfx_key(1, 4_000, &a));
        assert_ne!(sfx_key(1, 8_000, &a), sfx_key(2, 8_000, &a));
        assert_ne!(sfx_key(1, 8_000, &a), sfx_key(1, 8_000, &[1, -2, 4]));
    }

    #[test]
    fn the_effect_bus_sits_below_unity_but_stays_audible() {
        // The player asked for "아주 조금만" quieter, not for the effects to disappear.
        // Anything past about -12 dB stops reading as a hit at all.
        let trim_db = db(SFX_TRIM);
        assert!(
            (-12.0..=-4.0).contains(&trim_db),
            "SFX_TRIM {trim_db:+.1} dB is outside the useful band"
        );
    }
}
