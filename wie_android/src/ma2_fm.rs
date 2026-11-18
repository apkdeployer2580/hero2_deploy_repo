// hero2's songs through their own FM voices — a 2-operator FM synth in the manner of the
// Yamaha MA-2 (YMU759) the game was written for.
//
// ★ 2026-10-01 (`HERO2_BGM_FM_PLAN.md` §3 B2). Every hero2 song carries its voices in its
// score (`Mtsu`, parsed by `wie_backend::system::ma2`); until now the app threw them away and
// played each program number as a General MIDI instrument, which turns a plucked chord into a
// slow pad and a pad into an oboe (§8.37's balance table). This renders the voices themselves.
//
// ⛔ There is no MA-2 datasheet. The model is the OPL family's (YM3812/YMF262) — the VMA voice
// has OPL's operator: MULT·VIB·EGT·KSR · AR·DR·SL·RR · TL·KSL · WS, feedback on operator 1,
// two algorithms — with MA-3-family choices where the formats part (linear MULT, YMF825 LFO
// rates and depths, KSL order). Every such choice is a named constant below: they are
// starting points to be matched by ear, not measurements of the chip.
//
// Shape: [`FM_PARTS`] playback groups (0 = the song, 1 = a MIDI effect) × [`FM_CHANNELS`]
// channels, ONE voice per channel (the songs never overlap two notes on a channel, except one
// 4 ms tail in song 01 that a re-strike absorbs). A note latches its channel's voice when it
// starts; volume, expression and pan move the note that is sounding.
//
// No dependencies, edition 2021, no let-chains: `ma2_fm_selftest.rs` at the repo root and
// `tools/ma2_render` `include!` this very file.

/// Channels per playback group — the SMAF HPS logical channels (four tracks × four).
pub const FM_CHANNELS: usize = 16;
/// Playback groups: 0 = the music, 1 = a MIDI effect played over it (song 42).
pub const FM_PARTS: usize = 2;

/// One operator, exactly as the voice bytes say.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FmOp {
    pub mult: u8,
    pub vib: bool,
    /// 1 = a sustained tone: the envelope holds at SL while the key is down. 0 = percussive:
    /// on reaching SL it keeps falling at RR. (go-smaf `ToVM35`: `SR = EGT ? 0 : RR`; every
    /// drum voice in the songs is 0, every 10-second pad 1.)
    pub egt: bool,
    pub sus: bool,
    pub ksr: bool,
    pub rr: u8,
    pub dr: u8,
    pub ar: u8,
    pub sl: u8,
    pub tl: u8,
    pub ksl: u8,
    pub dvb: u8,
    pub dam: u8,
    pub am: bool,
    pub ws: u8,
}

/// A voice: `g0` (LFO · FB · ALG) and two operators. `g1` is 0x01 in every song and unused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FmVoice {
    pub lfo: u8,
    pub fb: u8,
    /// 0 = operator 1 modulates operator 2 · 1 = both are heard.
    pub alg: u8,
    pub ops: [FmOp; 2],
}

impl FmVoice {
    /// The 12 voice bytes after `43 03 <n> <bank> <program>`: `g0 g1 op1[5] op2[5]`.
    pub fn decode(d: &[u8; 12]) -> Self {
        let op = |o: &[u8]| FmOp {
            mult: o[0] >> 4,
            vib: o[0] & 0x08 != 0,
            egt: o[0] & 0x04 != 0,
            sus: o[0] & 0x02 != 0,
            ksr: o[0] & 0x01 != 0,
            rr: o[1] >> 4,
            dr: o[1] & 0x0f,
            ar: o[2] >> 4,
            sl: o[2] & 0x0f,
            tl: o[3] >> 2,
            ksl: o[3] & 0x03,
            dvb: o[4] >> 6,
            dam: (o[4] >> 4) & 0x03,
            am: o[4] & 0x08 != 0,
            ws: o[4] & 0x07,
        };
        FmVoice {
            lfo: d[0] >> 6,
            fb: (d[0] >> 3) & 0x07,
            alg: d[0] & 0x07,
            ops: [op(&d[2..7]), op(&d[7..12])],
        }
    }

    /// What a note plays when its program is not in the song: a plain sine that holds while
    /// the key is down. None of the 18 songs asks for a voice it does not carry.
    pub fn fallback() -> Self {
        let carrier = FmOp {
            mult: 1,
            egt: true,
            ar: 15,
            rr: 7,
            ..FmOp::default()
        };
        FmVoice {
            lfo: 0,
            fb: 0,
            alg: 0,
            // The modulator never attacks (AR 0), so the carrier is a pure sine.
            ops: [
                FmOp {
                    ar: 0,
                    tl: 63,
                    ..carrier
                },
                carrier,
            ],
        }
    }
}

// ── the model's numbers ─────────────────────────────────────────────────────────────────────

/// Frequency multiple per MULT (0 = ×½). MA-3's linear table; OPL's differs at 11, 13, 14.
const MULT: [f32; 16] = [0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0];

/// OPL decay/release time at rate 4 (R = 1, no key scaling), 0 → −96 dB. Each rate step of 4
/// halves it; the two low bits of the rate speed it by (4 + lo) / 4. R = 15 is 2.4 ms.
const DECAY_MS_R1: f32 = 39_281.0;
/// OPL attack time at rate 4, silence → full. Rate 60 and up is instant.
const ATTACK_MS_R1: f32 = 2_826.0;
/// The envelope's floor: at this attenuation an operator is off.
const ATT_MAX: f32 = 96.0;
/// OPL's attack is exponential in dB: each step takes `(att + 1 unit) / k` off, one unit being
/// 0.1875 dB — so it runs from −96 dB to 0 in `ln(96 / 0.1875 + 1)` time constants.
const ATTACK_FLOOR_DB: f32 = 0.1875;

/// Key-scale level, dB per octave, by KSL (the YMF825 order; OPL's is 0 / 3 / 1.5 / 6).
const KSL_DB_PER_OCT: [f32; 4] = [0.0, 1.5, 3.0, 6.0];
/// Where OPL's key-scale level starts: block 1, F-number 512 (≈ 48.6 Hz). Above it the
/// attenuation grows by the KSL rate per octave of the NOTE (not the operator) frequency.
const KSL_ZERO_HZ: f32 = 48.55;

/// LFO rate by the voice's LFO field, Hz (YMF825).
const LFO_HZ: [f32; 4] = [1.8, 4.0, 5.9, 7.0];
/// Tremolo depth by DAM, dB (YMF825).
const DAM_DB: [f32; 4] = [1.3, 2.8, 5.8, 11.8];
/// Vibrato depth by DVB, cents (YMF825).
const DVB_CENTS: [f32; 4] = [3.4, 6.7, 13.5, 26.8];

/// Modulator output → carrier phase, in wave-table units per unit of output. OPL adds the
/// modulator's ±4095 straight onto a 10-bit phase: a full-scale modulator swings the carrier
/// ±4 cycles.
const MOD_UNITS: f32 = 4096.0;

/// Pitch-bend range, semitones each way (the MIDI default; no song bends).
const BEND_SEMITONES: f32 = 2.0;

/// Master level of the whole synth, before the game's 배경음 level. Set so the FM songs
/// land where the General MIDI rendering of the same songs did (`HERO2_BGM_FM_PLAN.md` §3.2
/// 믹스) — the balance against the effect bus must not move.
pub const FM_GAIN: f32 = 0.25;

const WAVE_BITS: u32 = 10;
const WAVE_LEN: usize = 1 << WAVE_BITS;
/// Attenuation → gain, in 1/16 dB steps up to [`ATT_MAX`] + TL + KSL + AM headroom.
const GAIN_STEPS_PER_DB: f32 = 16.0;
const GAIN_LEN: usize = 256 * 16;

/// The eight waveforms by WS, as OPL3 has them: sine · half sine · absolute sine · quarter
/// pulses · alternating (double-rate sine, then silence) · camel (double-rate absolute
/// sine, then silence) · square · derived square (a log-linear ramp each half).
fn wave(ws: u8, i: usize) -> f32 {
    let p = i as f32 / WAVE_LEN as f32;
    let tau = core::f32::consts::TAU;
    let s = (tau * p).sin();
    match ws & 7 {
        0 => s,
        1 => s.max(0.0),
        2 => s.abs(),
        3 => {
            if (p * 4.0) as u32 % 2 == 0 {
                s.abs()
            } else {
                0.0
            }
        }
        4 => {
            if p < 0.5 {
                (2.0 * tau * p).sin()
            } else {
                0.0
            }
        }
        5 => {
            if p < 0.5 {
                (2.0 * tau * p).sin().abs()
            } else {
                0.0
            }
        }
        6 => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        _ => {
            if p < 0.5 {
                (-32.0 * p).exp2()
            } else {
                -(-32.0 * (1.0 - p)).exp2()
            }
        }
    }
}

/// Envelope time for an effective rate (0..=63), ms — `None` = never moves (rate 0).
fn rate_ms(base: f32, rate: u8, top: Option<f32>) -> Option<f32> {
    if rate < 4 {
        return None;
    }
    if rate >= 60 {
        return Some(top.unwrap_or(base / (1u32 << 14) as f32));
    }
    let hi = (rate >> 2) as i32;
    let lo = (rate & 3) as f32;
    Some(base / (1u32 << (hi - 1)) as f32 * 4.0 / (4.0 + lo))
}

/// OPL key code of a note (block × 2 + the F-number's top bit), for rate key scaling.
fn key_code(note: u8) -> u8 {
    let block = (note / 12).saturating_sub(1).min(7);
    block * 2 + u8::from(note % 12 >= 6)
}

fn note_hz(note: u8) -> f32 {
    440.0 * ((note as f32 - 69.0) / 12.0).exp2()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Stage {
    #[default]
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// An operator of a sounding note.
#[derive(Clone, Copy, Debug, Default)]
struct OpState {
    phase: u32,
    /// Phase step per sample at the note's frequency, before vibrato.
    step: u32,
    stage: Stage,
    /// Envelope attenuation, dB (0 = full).
    att: f32,
    /// Per-sample: attack factor (`att -= (att + floor) × k`), decay and release dB.
    attack_k: f32,
    attack_instant: bool,
    decay_db: f32,
    release_db: f32,
    sustain_db: f32,
    /// TL + KSL, dB.
    level_db: f32,
}

impl OpState {
    fn advance_envelope(&mut self, op: &FmOp) {
        match self.stage {
            Stage::Off => {}
            Stage::Attack => {
                self.att -= (self.att + ATTACK_FLOOR_DB) * self.attack_k;
                if self.att <= 0.0 {
                    self.att = 0.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                if self.att >= self.sustain_db {
                    self.att = self.sustain_db;
                    self.stage = Stage::Sustain;
                } else {
                    self.att = (self.att + self.decay_db).min(self.sustain_db);
                }
            }
            Stage::Sustain => {
                if !op.egt {
                    self.att += self.release_db;
                }
            }
            Stage::Release => self.att += self.release_db,
        }
        if self.att >= ATT_MAX && self.stage != Stage::Attack {
            self.att = ATT_MAX;
            self.stage = Stage::Off;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Channel {
    bank: u8,
    program: u8,
    volume: u8,
    expression: u8,
    pan: u8,
    bend: u16,
    modulation: u8,
    note: u8,
    keyed: bool,
    voice: FmVoice,
    ops: [OpState; 2],
    /// Operator 1's last two outputs, for its self-feedback.
    fb: [f32; 2],
    lfo_phase: f32,
    lfo_step: f32,
    gain_l: f32,
    gain_r: f32,
}

impl Channel {
    fn new() -> Self {
        let mut c = Channel {
            bank: 0,
            program: 0,
            volume: 100,
            expression: 127,
            pan: 64,
            bend: 0x2000,
            modulation: 0,
            note: 0,
            keyed: false,
            voice: FmVoice::fallback(),
            ops: [OpState::default(); 2],
            fb: [0.0; 2],
            lfo_phase: 0.0,
            lfo_step: 0.0,
            gain_l: 0.0,
            gain_r: 0.0,
        };
        c.update_gain();
        c
    }

    fn sounding(&self) -> bool {
        match self.voice.alg {
            0 => self.ops[1].stage != Stage::Off,
            _ => self.ops[0].stage != Stage::Off || self.ops[1].stage != Stage::Off,
        }
    }

    /// Volume and expression with the MIDI law (40·log10 each), panned by balance.
    fn update_gain(&mut self) {
        let v = self.volume as f32 / 127.0;
        let e = self.expression as f32 / 127.0;
        let g = v * v * e * e;
        let (l, r) = if self.pan < 64 {
            (1.0, self.pan as f32 / 64.0)
        } else {
            ((127 - self.pan) as f32 / 63.0, 1.0)
        };
        self.gain_l = g * l.min(1.0);
        self.gain_r = g * r.min(1.0);
    }
}

struct Part {
    voices: Vec<(u8, u8, FmVoice)>,
    ch: [Channel; FM_CHANNELS],
}

impl Part {
    fn new() -> Self {
        Part {
            voices: Vec::new(),
            ch: [Channel::new(); FM_CHANNELS],
        }
    }
}

/// The synth. Owned by the audio thread; every method is cheap and allocation-free except
/// [`Ma2Fm::set_voices`], which takes the table it is handed.
pub struct Ma2Fm {
    rate: f32,
    parts: [Part; FM_PARTS],
    waves: Vec<[f32; WAVE_LEN]>,
    gain: Vec<f32>,
}

impl Ma2Fm {
    pub fn new(sample_rate: u32) -> Self {
        let waves = (0..8u8)
            .map(|ws| {
                let mut t = [0.0f32; WAVE_LEN];
                for (i, v) in t.iter_mut().enumerate() {
                    *v = wave(ws, i);
                }
                t
            })
            .collect();
        let gain = (0..GAIN_LEN)
            .map(|i| {
                let db = i as f32 / GAIN_STEPS_PER_DB;
                if db >= ATT_MAX {
                    0.0
                } else {
                    (10.0f32).powf(-db / 20.0)
                }
            })
            .collect();
        Ma2Fm {
            rate: sample_rate.max(1) as f32,
            parts: [Part::new(), Part::new()],
            waves,
            gain,
        }
    }

    /// A new song on `part`: its voice table, every channel back to its defaults, silence.
    pub fn set_voices(&mut self, part: usize, voices: Vec<(u8, u8, FmVoice)>) {
        if let Some(p) = self.parts.get_mut(part) {
            p.voices = voices;
            p.ch = [Channel::new(); FM_CHANNELS];
        }
    }

    /// Silence `part` at once — every note, release tails included.
    pub fn all_off(&mut self, part: usize) {
        if let Some(p) = self.parts.get_mut(part) {
            for c in p.ch.iter_mut() {
                c.keyed = false;
                for o in c.ops.iter_mut() {
                    o.stage = Stage::Off;
                    o.att = ATT_MAX;
                }
            }
        }
    }

    fn channel(&mut self, part: usize, ch: u8) -> Option<&mut Channel> {
        self.parts.get_mut(part).and_then(|p| p.ch.get_mut(ch as usize))
    }

    pub fn program(&mut self, part: usize, ch: u8, bank: u8, program: u8) {
        if let Some(c) = self.channel(part, ch) {
            c.bank = bank;
            c.program = program;
        }
    }

    pub fn volume(&mut self, part: usize, ch: u8, value: u8) {
        if let Some(c) = self.channel(part, ch) {
            c.volume = value.min(127);
            c.update_gain();
        }
    }

    pub fn expression(&mut self, part: usize, ch: u8, value: u8) {
        if let Some(c) = self.channel(part, ch) {
            c.expression = value.min(127);
            c.update_gain();
        }
    }

    pub fn pan(&mut self, part: usize, ch: u8, value: u8) {
        if let Some(c) = self.channel(part, ch) {
            c.pan = value.min(127);
            c.update_gain();
        }
    }

    pub fn pitch_bend(&mut self, part: usize, ch: u8, value: u16) {
        let rate = self.rate;
        if let Some(c) = self.channel(part, ch) {
            c.bend = value.min(0x3fff);
            if c.keyed || c.sounding() {
                Self::set_pitch(c, rate);
            }
        }
    }

    pub fn modulation(&mut self, part: usize, ch: u8, value: u8) {
        if let Some(c) = self.channel(part, ch) {
            c.modulation = value;
        }
    }

    fn set_pitch(c: &mut Channel, rate: f32) {
        let bend = ((c.bend as f32 - 8192.0) / 8192.0 * BEND_SEMITONES / 12.0).exp2();
        let hz = note_hz(c.note) * bend;
        for (o, op) in c.ops.iter_mut().zip(c.voice.ops.iter()) {
            let f = hz * MULT[(op.mult & 15) as usize];
            // Past Nyquist an operator only aliases; the chip could not play it either.
            let cycles = if f < rate * 0.5 { f / rate } else { 0.0 };
            o.step = (cycles * 4_294_967_296.0) as u32;
        }
    }

    /// Key on: the channel takes the voice its program names and strikes it — the envelope
    /// attacks from wherever it is, the phase restarts (OPL).
    pub fn note_on(&mut self, part: usize, ch: u8, note: u8) {
        let rate = self.rate;
        let Some(p) = self.parts.get_mut(part) else { return };
        let Some(c) = p.ch.get_mut(ch as usize) else { return };
        let voice = p
            .voices
            .iter()
            .rev()
            .find(|(b, pr, _)| *b == c.bank && *pr == c.program)
            .map(|(_, _, v)| *v)
            .unwrap_or_else(FmVoice::fallback);
        c.voice = voice;
        c.note = note.min(127);
        c.keyed = true;
        c.lfo_step = LFO_HZ[(voice.lfo & 3) as usize] / rate;
        Self::set_pitch(c, rate);
        let kc = key_code(c.note);
        let ksl_oct = (note_hz(c.note) / KSL_ZERO_HZ).log2().max(0.0);
        for (o, op) in c.ops.iter_mut().zip(voice.ops.iter()) {
            let rof = if op.ksr { kc } else { kc >> 2 };
            let eff = |r: u8| if r == 0 { 0 } else { (r * 4 + rof).min(63) };
            let samples = |ms: f32| (ms * rate / 1000.0).max(1.0);
            let attack = rate_ms(ATTACK_MS_R1, eff(op.ar), Some(0.0));
            o.attack_instant = attack == Some(0.0);
            o.attack_k = match attack {
                Some(ms) if ms > 0.0 => {
                    let tau = samples(ms) / (ATT_MAX / ATTACK_FLOOR_DB + 1.0).ln();
                    1.0 - (-1.0 / tau).exp()
                }
                _ => 0.0,
            };
            o.decay_db = rate_ms(DECAY_MS_R1, eff(op.dr), None).map_or(0.0, |ms| ATT_MAX / samples(ms));
            o.release_db = rate_ms(DECAY_MS_R1, eff(op.rr), None).map_or(0.0, |ms| ATT_MAX / samples(ms));
            o.sustain_db = if op.sl == 15 { 93.0 } else { op.sl as f32 * 3.0 };
            o.level_db = op.tl as f32 * 0.75 + KSL_DB_PER_OCT[(op.ksl & 3) as usize] * ksl_oct;
            o.phase = 0;
            if o.stage == Stage::Off {
                o.att = ATT_MAX;
            }
            o.stage = Stage::Attack;
            if o.attack_instant {
                o.att = 0.0;
                o.stage = Stage::Decay;
            }
        }
        c.fb = [0.0; 2];
    }

    /// Key off, for this note only (a channel that has moved on keeps its new note).
    pub fn note_off(&mut self, part: usize, ch: u8, note: u8) {
        if let Some(c) = self.channel(part, ch) {
            if c.keyed && c.note == note {
                c.keyed = false;
                for o in c.ops.iter_mut() {
                    if o.stage != Stage::Off {
                        o.stage = Stage::Release;
                    }
                }
            }
        }
    }

    /// Channels with a sound in them, both parts.
    pub fn active_voices(&self) -> u32 {
        self.parts.iter().flat_map(|p| p.ch.iter()).filter(|c| c.sounding()).count() as u32
    }

    #[inline]
    fn level(&self, db: f32) -> f32 {
        let i = (db * GAIN_STEPS_PER_DB) as usize;
        if i < GAIN_LEN { self.gain[i] } else { 0.0 }
    }

    /// Render interleaved stereo into `out` (overwritten), × [`FM_GAIN`].
    pub fn render(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        let frames = out.len() / 2;
        for part in 0..FM_PARTS {
            for ch in 0..FM_CHANNELS {
                if !self.parts[part].ch[ch].sounding() {
                    continue;
                }
                // Copy out, work, copy back: keeps the borrow of the tables simple.
                let mut c = self.parts[part].ch[ch];
                self.render_channel(&mut c, &mut out[..frames * 2]);
                self.parts[part].ch[ch] = c;
            }
        }
    }

    fn render_channel(&self, c: &mut Channel, out: &mut [f32]) {
        let v = c.voice;
        let [op1, op2] = v.ops;
        let (w1, w2) = (&self.waves[(op1.ws & 7) as usize], &self.waves[(op2.ws & 7) as usize]);
        let fb_scale = if v.fb == 0 { 0.0 } else { (1u32 << (v.fb + 3)) as f32 };
        let tremolo = [
            if op1.am { DAM_DB[(op1.dam & 3) as usize] } else { 0.0 },
            if op2.am { DAM_DB[(op2.dam & 3) as usize] } else { 0.0 },
        ];
        let vibrato = [
            if op1.vib { (DVB_CENTS[(op1.dvb & 3) as usize] / 1200.0).exp2() - 1.0 } else { 0.0 },
            if op2.vib { (DVB_CENTS[(op2.dvb & 3) as usize] / 1200.0).exp2() - 1.0 } else { 0.0 },
        ];
        let lfo_used = tremolo[0] != 0.0 || tremolo[1] != 0.0 || vibrato[0] != 0.0 || vibrato[1] != 0.0;
        let shift = 32 - WAVE_BITS;
        for frame in out.chunks_exact_mut(2) {
            let mut tri = 0.0f32;
            if lfo_used {
                c.lfo_phase += c.lfo_step;
                if c.lfo_phase >= 1.0 {
                    c.lfo_phase -= 1.0;
                }
                // −1 … 1 … −1 over one LFO period.
                tri = 1.0 - 4.0 * (c.lfo_phase - 0.5).abs();
            }
            c.ops[0].advance_envelope(&op1);
            c.ops[1].advance_envelope(&op2);

            // Operator 1, with its own feedback.
            let o = &mut c.ops[0];
            o.phase = o.phase.wrapping_add(if vibrato[0] != 0.0 {
                (o.step as f32 * (1.0 + vibrato[0] * tri)) as u32
            } else {
                o.step
            });
            let fb = ((c.fb[0] + c.fb[1]) * fb_scale) as i32;
            let idx = ((o.phase >> shift) as i32).wrapping_add(fb) as usize & (WAVE_LEN - 1);
            let db1 = o.att + o.level_db + tremolo[0] * (tri + 1.0) * 0.5;
            let y1 = w1[idx] * self.level(db1);
            c.fb[1] = c.fb[0];
            c.fb[0] = y1;

            // Operator 2: modulated by 1 (algorithm 0) or beside it (1).
            let o = &mut c.ops[1];
            o.phase = o.phase.wrapping_add(if vibrato[1] != 0.0 {
                (o.step as f32 * (1.0 + vibrato[1] * tri)) as u32
            } else {
                o.step
            });
            let pm = if v.alg == 0 { (y1 * MOD_UNITS) as i32 } else { 0 };
            let idx = ((o.phase >> shift) as i32).wrapping_add(pm) as usize & (WAVE_LEN - 1);
            let db2 = o.att + o.level_db + tremolo[1] * (tri + 1.0) * 0.5;
            let y2 = w2[idx] * self.level(db2);

            let y = (if v.alg == 0 { y2 } else { y1 + y2 }) * FM_GAIN;
            frame[0] += y * c.gain_l;
            frame[1] += y * c.gain_r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Song 02's first voice (pc 89): `60 01 | 14 52 f4 3a a0 | 14 62 f7 00 a0`.
    const PAD_89: [u8; 12] = [0x60, 0x01, 0x14, 0x52, 0xf4, 0x3a, 0xa0, 0x14, 0x62, 0xf7, 0x00, 0xa0];

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    fn run(fm: &mut Ma2Fm, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; frames * 2];
        fm.render(&mut out);
        out
    }

    #[test]
    fn a_voice_decodes_field_by_field() {
        let v = FmVoice::decode(&PAD_89);
        assert_eq!((v.lfo, v.fb, v.alg), (1, 4, 0));
        let m = v.ops[0];
        assert_eq!((m.mult, m.vib, m.egt, m.sus, m.ksr), (1, false, true, false, false));
        assert_eq!((m.rr, m.dr, m.ar, m.sl), (5, 2, 15, 4));
        assert_eq!((m.tl, m.ksl, m.dvb, m.dam, m.am, m.ws), (14, 2, 2, 2, false, 0));
        let c = v.ops[1];
        assert_eq!((c.rr, c.dr, c.ar, c.sl, c.tl), (6, 2, 15, 7, 0));
    }

    #[test]
    fn the_envelope_rates_are_the_opl_tables() {
        assert_eq!(rate_ms(DECAY_MS_R1, 4, None), Some(39_281.0));
        let r15 = rate_ms(DECAY_MS_R1, 60, None).unwrap();
        assert!((r15 - 2.4).abs() < 0.01, "{r15}");
        assert_eq!(rate_ms(ATTACK_MS_R1, 60, Some(0.0)), Some(0.0), "AR 15 is instant");
        let a14 = rate_ms(ATTACK_MS_R1, 56, Some(0.0)).unwrap();
        assert!((a14 - 0.345).abs() < 0.01, "{a14}");
        // The two low bits: rate 5 is 4/5 of rate 4's time.
        assert_eq!(rate_ms(DECAY_MS_R1, 5, None), Some(39_281.0 * 0.8));
        assert_eq!(rate_ms(DECAY_MS_R1, 0, None), None, "rate 0 never moves");
    }

    #[test]
    fn a_sustained_voice_holds_and_a_percussive_one_dies_under_the_key() {
        let mut fm = Ma2Fm::new(48_000);
        let mut sustained = FmVoice::decode(&PAD_89);
        let mut percussive = sustained;
        for op in percussive.ops.iter_mut() {
            op.egt = false;
            op.dr = 12;
            op.rr = 12;
        }
        sustained.ops[1].sl = 1;
        fm.set_voices(0, vec![(0, 1, sustained), (0, 2, percussive)]);
        fm.program(0, 0, 0, 1);
        fm.program(0, 1, 0, 2);
        fm.note_on(0, 0, 60);
        fm.note_on(0, 1, 64);
        // Two seconds with both keys down.
        run(&mut fm, 96_000);
        let c = &fm.parts[0].ch;
        assert!(c[0].sounding(), "EGT 1 holds at SL while the key is down");
        assert!(!c[1].sounding(), "EGT 0 keeps falling at RR under the key");
        // Key off: the sustained one releases.
        fm.note_off(0, 0, 60);
        run(&mut fm, 96_000 * 4);
        assert!(!fm.parts[0].ch[0].sounding());
    }

    #[test]
    fn a_stale_key_off_does_not_stop_the_next_note() {
        let mut fm = Ma2Fm::new(48_000);
        fm.set_voices(0, vec![(0, 89, FmVoice::decode(&PAD_89))]);
        fm.program(0, 2, 0, 89);
        // Song 01 channel 2 at 13 068 ms: note 84 re-strikes the channel 4 ms before note 76's
        // key-off arrives.
        fm.note_on(0, 2, 76);
        run(&mut fm, 480);
        fm.note_on(0, 2, 84);
        fm.note_off(0, 2, 76);
        assert!(fm.parts[0].ch[2].keyed, "the old note's key-off must not release the new note");
        fm.note_off(0, 2, 84);
        assert!(!fm.parts[0].ch[2].keyed);
    }

    #[test]
    fn it_sounds_at_the_right_pitch() {
        // A plain sine carrier, no modulation: count zero crossings of A4.
        let mut fm = Ma2Fm::new(48_000);
        fm.set_voices(0, vec![]);
        fm.note_on(0, 0, 69);
        let out = run(&mut fm, 48_000);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let ups = left.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((438..=442).contains(&ups), "{ups} rising zero crossings in one second");
    }

    #[test]
    fn levels_follow_volume_and_expression_and_all_off_is_immediate() {
        let mut fm = Ma2Fm::new(48_000);
        fm.set_voices(0, vec![]);
        fm.note_on(0, 0, 69);
        run(&mut fm, 4_800);
        let full = rms(&run(&mut fm, 4_800));
        fm.volume(0, 0, 64);
        let half = rms(&run(&mut fm, 4_800));
        let db = 20.0 * (half / full).log10();
        // Volume 100 → 64 with the 40·log10 law: 40·log10(64/100) = −7.75 dB.
        assert!((db + 7.75).abs() < 0.3, "{db}");
        fm.expression(0, 0, 0);
        assert_eq!(rms(&run(&mut fm, 480)), 0.0);
        fm.expression(0, 0, 127);
        assert!(rms(&run(&mut fm, 480)) > 0.0);
        fm.all_off(0);
        assert_eq!(rms(&run(&mut fm, 480)), 0.0, "all_off leaves no tail");
        assert_eq!(fm.active_voices(), 0);
    }

    #[test]
    fn the_parts_are_independent() {
        let mut fm = Ma2Fm::new(48_000);
        fm.set_voices(0, vec![]);
        fm.set_voices(1, vec![]);
        fm.note_on(0, 3, 60);
        fm.note_on(1, 3, 72);
        run(&mut fm, 480);
        fm.all_off(1);
        assert_eq!(fm.active_voices(), 1, "silencing the effect leaves the song");
        fm.set_voices(1, vec![]);
        assert!(fm.parts[0].ch[3].sounding());
    }

    #[test]
    fn every_waveform_stays_in_range_and_the_ones_that_should_are_silent_half_the_time() {
        for ws in 0..8u8 {
            let t: Vec<f32> = (0..WAVE_LEN).map(|i| wave(ws, i)).collect();
            assert!(t.iter().all(|v| v.abs() <= 1.0 + 1e-6), "ws {ws}");
            let zeros = t.iter().filter(|v| **v == 0.0).count();
            match ws {
                1 | 4 | 5 => assert!(zeros >= WAVE_LEN / 2 - 1, "ws {ws}: {zeros}"),
                3 => assert!(zeros >= WAVE_LEN / 2 - 2, "ws {ws}: {zeros}"),
                _ => {}
            }
        }
    }
}
