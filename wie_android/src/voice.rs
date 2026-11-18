// The awakenings' voices — four short recordings the host plays over the game's own sound.
//
// ★ 2026-10-09 (user: *"격분 사용시 그아앗! 하는 소리지르는 효과음 … 격분 폭발 시 갈! … 천공의 검 사용시엔 하늘이 열린다! …
// 개화 사용시엔 꽃이 핀다... 하고 나즈막히 말하는"* → *"dist 폴더 내에 천공의검 효과음(night_sword)과 개화의 효과음(flower)
// 격분의 효과음 2개(시작 angry_start, 끝 angry_ending) ogg파일들 넣어놨어"*). The game has no such sounds and no slot to put
// them in (its effects are 4-bit ADPCM at 8 kHz in a fixed table), so the host decodes the Ogg Vorbis files once and hands
// the PCM to the same queue the game's effects take (`audio_sink::play_host_clip`): the 효과음 크기 trim, the band-limited
// resample and the same-effect limit apply to them as to any effect. WHEN is `hero2java::H2VoiceWatch`; how loud is the
// game's own sound level (`bs.b:B`, [`voice_gain`]).
//
// ★★ 2026-10-24 and one effect the game lacks: the thunder of 어둠심판 · 벽력일섬 (`tools_re/thunder_sfx.py` synthesises it;
// WHEN is `hero2java::H2ThunderWatch`), at its own loudness ([`clip_target_db`]).
//
// Edition 2021, one dependency (`lewton`, already in the tree through oxisynth's SF3 fonts) — `wie_probe` `include!`s
// this file.

use std::sync::OnceLock;

/// The clips, in `hero2java::H2Voice` order (`H2Voice::index`): 격분 start · 격분 explosion · 천공의검 · 개화. The
/// sources are the user's files as given (`wie_android/assets/voice/`, copied from `dist/`).
pub const VOICE_FILES: [(&str, &[u8]); 6] = [
    ("angry_start", include_bytes!("../assets/voice/angry_start.ogg")),
    ("angry_ending", include_bytes!("../assets/voice/angry_ending.ogg")),
    ("night_sword", include_bytes!("../assets/voice/night_sword.ogg")),
    ("flower", include_bytes!("../assets/voice/flower.ogg")),
    // ★ 2026-10-13 악의's roar (the user's `dist/악의 1.ogg`).
    ("malice", include_bytes!("../assets/voice/malice.ogg")),
    // ★★ 2026-10-24 the lightning (user: *"어둠심판도 소리안나오고있어 번개소리 나야해"* — and 벽력일섬's bolts): the game ships
    // no thunder among its eleven effects, so the host plays this one — synthesised by `tools_re/thunder_sfx.py`.
    ("thunder", include_bytes!("../assets/voice/thunder.ogg")),
];

/// One decoded clip: interleaved 16-bit PCM, `channels` wide, at `rate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceClip {
    pub channels: u8,
    pub rate: u32,
    pub samples: Vec<i16>,
}

// `seconds` / `level_db` are the probe's measuring tools (its voice tests); the app does not call them.
#[allow(dead_code)]
impl VoiceClip {
    pub fn seconds(&self) -> f32 {
        let frames = self.samples.len() / usize::from(self.channels.max(1));
        frames as f32 / self.rate.max(1) as f32
    }

    /// `(peak, rms)` in dBFS — what the level choice is made from.
    pub fn level_db(&self) -> (f32, f32) {
        let db = |x: f64| if x <= 0.0 { -200.0 } else { (20.0 * x.log10()) as f32 };
        let peak = self.samples.iter().map(|s| i32::from(*s).unsigned_abs()).max().unwrap_or(0);
        let sum: f64 = self.samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
        let rms = (sum / self.samples.len().max(1) as f64).sqrt();
        (db(f64::from(peak) / 32768.0), db(rms / 32768.0))
    }
}

/// The window the loudness is measured over: what an ear calls "how loud is the shout" is its loudest stretch, not the
/// clip's average — a clip with a quiet lead-in or a long tail would read far quieter than it sounds.
pub const LOUDNESS_WINDOW_MS: u32 = 300;

/// The loudest [`LOUDNESS_WINDOW_MS`] of a clip, RMS in dBFS (windows every 25 ms; a clip shorter than the window is
/// one window).
pub fn active_db(samples: &[i16], channels: u8, rate: u32) -> f32 {
    let ch = usize::from(channels.max(1));
    let frames = samples.len() / ch;
    let win = ((rate as usize * LOUDNESS_WINDOW_MS as usize) / 1000).clamp(1, frames.max(1));
    let hop = (rate as usize / 40).max(1);
    let energy = |f0: usize| -> f64 { samples[f0 * ch..(f0 + win).min(frames) * ch].iter().map(|s| f64::from(*s) * f64::from(*s)).sum() };
    let mut best = 0.0f64;
    let mut f0 = 0;
    loop {
        best = best.max(energy(f0) / (win * ch) as f64);
        if f0 + win >= frames {
            break;
        }
        f0 = (f0 + hop).min(frames - win);
    }
    let rms = best.sqrt() / 32768.0;
    if rms <= 0.0 {
        -200.0
    } else {
        (20.0 * rms.log10()) as f32
    }
}

/// ★ 2026-10-09 (user: *"음량만 밸런스 맞추면돼"*) — every voice is brought to this loudness ([`active_db`]): the level
/// the game's own 기합 (effect 45, the berserker's shout these replace on 격분) reaches over the same window, measured
/// in the probe (`--audio-wav` A/B with `--h2-voice 0`, the effect bus's trim taken out). One level for the four, so
/// none stands out of the others or of the game's effects.
pub const VOICE_TARGET_DB: f32 = -11.0;
/// The ceiling a raised clip's peaks are held under (a soft knee from [`VOICE_KNEE`] — no hard clip).
pub const VOICE_CEILING: f32 = 0.95;
pub const VOICE_KNEE: f32 = 0.7;

/// ★★ 2026-10-24 the thunder's loudness: 4 dB under the voices — it is an effect, not a shout, and 어둠심판 strikes it again
/// every [`crate::guest_build::H2ThunderWatch::EVERY`] ticks while monsters stand in the square.
pub const THUNDER_TARGET_DB: f32 = -15.0;

/// The loudness clip `i` of [`VOICE_FILES`] is brought to.
pub fn clip_target_db(i: usize) -> f32 {
    match VOICE_FILES.get(i) {
        Some(("thunder", _)) => THUNDER_TARGET_DB,
        _ => VOICE_TARGET_DB,
    }
}

/// The gain [`normalise`] gives a clip of loudness `active` dB.
pub fn voice_norm_gain(active: f32) -> f32 {
    norm_gain_to(active, VOICE_TARGET_DB)
}

fn norm_gain_to(active: f32, target: f32) -> f32 {
    10f32.powf((target - active) / 20.0)
}

/// A clip brought to [`VOICE_TARGET_DB`]: scaled, then any sample past [`VOICE_KNEE`] of full scale bent smoothly toward
/// [`VOICE_CEILING`] (only a raised clip ever gets there).
pub fn normalise(clip: &mut VoiceClip) {
    normalise_to(clip, VOICE_TARGET_DB);
}

/// [`normalise`] to `target` dB.
pub fn normalise_to(clip: &mut VoiceClip, target: f32) {
    let gain = norm_gain_to(active_db(&clip.samples, clip.channels, clip.rate), target);
    let (knee, room) = (VOICE_KNEE, VOICE_CEILING - VOICE_KNEE);
    for s in clip.samples.iter_mut() {
        let x = f32::from(*s) / 32768.0 * gain;
        let a = x.abs();
        let y = if a <= knee { a } else { knee + room * ((a - knee) / room).tanh() };
        *s = (y.copysign(x) * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
    }
}

/// An Ogg Vorbis stream → PCM. `None` for anything lewton does not read, a rate or a channel count the bus would not
/// take, or no audio at all.
pub fn decode_ogg(bytes: &[u8]) -> Option<VoiceClip> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes)).ok()?;
    let channels = reader.ident_hdr.audio_channels;
    let rate = reader.ident_hdr.audio_sample_rate;
    if !(1..=2).contains(&channels) || !(8_000..=96_000).contains(&rate) {
        return None;
    }
    let mut samples = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().ok()? {
        samples.extend_from_slice(&packet);
    }
    (!samples.is_empty()).then_some(VoiceClip { channels, rate, samples })
}

/// Clip `i` of [`VOICE_FILES`], decoded and [`normalise`]d on first use and kept (a few hundred KB for all four). `None`
/// if it does not decode — the cue is then silent, never an error.
pub fn voice_clip(i: usize) -> Option<&'static VoiceClip> {
    static CLIPS: OnceLock<Vec<Option<VoiceClip>>> = OnceLock::new();
    CLIPS
        .get_or_init(|| {
            VOICE_FILES
                .iter()
                .enumerate()
                .map(|(i, (_, bytes))| {
                    decode_ogg(bytes).map(|mut c| {
                        normalise_to(&mut c, clip_target_db(i));
                        c
                    })
                })
                .collect()
        })
        .get(i)?
        .as_ref()
}

/// The game's own sound level `bs.b:B` (the 환경설정 window's first row) as the game's EFFECTS actually play it here:
/// `br.a(Clip,Z)V` @0x138948 plays nothing at all at `≤ 0`, and otherwise asks `Clip.setVolume(level × 10)` through
/// `br.a(I)V` — which this emulator does not honour (`wie_wipi_java` `org.kwis.msp.media.Clip::setVolume` is a stub), so
/// every effect plays at full whatever the bar says. The voices do the same — silent at 0, else full — so they sit where
/// the game's 기합 sat (probe A/B, 2026-10-09: scaled by the bar they came out 20 dB under it). The level they share
/// with the effects is the app's 효과음 크기, applied on the queue.
pub fn voice_gain(level: i8) -> f32 {
    if level <= 0 {
        0.0
    } else {
        1.0
    }
}

/// What the queue gets for clip `i` at the game's sound `level`: the clip as decoded, `None` when the game's sound is
/// off or the clip is missing.
pub fn voice_pcm(i: usize, level: i8) -> Option<(u8, u32, Vec<i16>)> {
    if voice_gain(level) <= 0.0 {
        return None;
    }
    let clip = voice_clip(i)?;
    Some((clip.channels, clip.rate, clip.samples.clone()))
}
