//! Android audio sink.
//!
//! wie feeds audio as **SMAF** events: PCM one-shots via `play_wave`, and music
//! as **MIDI** (Hero3's audio is 100% MIDI — device trace showed thousands of
//! `midi_note_on` and zero `play_wave`). Desktop wie routes MIDI to the OS synth
//! via `midir`, which does not exist on Android, so we render MIDI ourselves with
//! a pure-Rust SoundFont synth (`oxisynth`, SF3/Vorbis) and mix it with the PCM
//! path through rodio (cpal -> AAudio).
//!
//! Threading: the rodio device is opened on a dedicated thread spawned from
//! `AndroidPlatform::new()` (worker thread — `ndk_context` is populated by then,
//! which cpal's AAudio backend requires). The output handle, the `Player`, and
//! the mixer must stay alive for the whole session or playback stops.
//!
//! MIDI is decoupled from the audio callback: `midi_*` push `MidiEvent`s down an
//! mpsc channel; the `Bus` (owned by rodio's device mixer, i.e. the audio
//! callback thread) exclusively owns the `Synth`, draining pending events at the
//! start of each 64-frame render chunk. No `Mutex` on the synth -> no lock
//! contention between the emulator thread and the real-time audio thread.
//!
//! ── SIGNAL CHAIN (2026-08-16 · 2026-09-19) ───────────────────────────────────
//!   Synth (BGM, x music level, rendered AT THE DEVICE RATE) ─┐
//!                                                            ├─> Bus (sum → soft_limit) ─> device
//!   SFX clips (x SFX_TRIM, converted to device rate/stereo  ─┘
//!             ONCE on the wie-audio thread — `sfx_upsample`, 2026-10-02)
//!
//! `Bus` sums everything the game makes so the SUM can be limited before it reaches
//! AAudio, which hard-clamps. See `audio_mix` for the measurements behind every constant.
//!
//! ★ 2026-09-19: ONE source on the device mixer, not a mixer-in-a-mixer. simpleperf on
//! S9 put the `AudioTrack` thread at 24 % of the process, and 23 % of THAT was rodio
//! plumbing that did no audio work: two `MixerSource`s each polling their mpsc channel
//! and `retain_mut`-ing a `Vec<Box<dyn Source>>` per sample, and five generic iterator
//! layers (`UniformSourceIterator` → `ChannelCountConverter` → `SampleRateConverter` →
//! `Take` → source, twice) between the synth and the callback. The synth now renders at
//! whatever rate the device opened with (S9: rodio picks 44.1 kHz, so nothing changed
//! there — but a device that opens at 48 kHz no longer goes through rodio's linear
//! resampler), SFX clips are converted once when they arrive, and the callback's
//! per-sample work is one add and one `soft_limit`. Measured (S9, field BGM): plumbing
//! 23.5 → 13.4 % of the thread; the remaining 13 % is rodio's own device mixer, which
//! is the price of not opening cpal ourselves. The other 80 % is the synth's DSP —
//! voices 25 + interpolation 25 + reverb 22 + chorus 6 — i.e. the sound itself.
//!
//! ⚠ And the thread is NOT the heat item it looked like: Android schedules it on a
//! LITTLE core (S9: cpu1 at ~715 MHz while `wie-emu`/`android_main` sit on big cores at
//! 1.5–2.7 GHz). Its 30 % `top` share is 30 % of a slow, efficient core. See
//! `HERO2_NEXT_SESSION.md` 「AudioTrack」.
//!
//! ── FM (2026-10-01, `HERO2_BGM_FM_PLAN.md` §3 B3) ──────────────────────────────────────
//! hero2's songs are MA-2 scores that carry their own FM voices. With the FM switch on
//! (`config::read_bgm_synth`, the default) the player sends them here as `FmMsg`s instead of
//! MIDI, and the `Bus` renders them with `ma2_fm::Ma2Fm` at the same music gain the synth
//! gets. The SoundFont synth stays loaded — the switch can send everything back to it — but
//! in FM mode it has nothing to play, so after [`SYNTH_IDLE_SECS`] of silence it is no longer
//! rendered at all: its reverb and chorus cost the same in silence.

use std::{
    io::Cursor,
    num::NonZero,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use std::sync::{Mutex, OnceLock};

use oxisynth::{InterpolationMethod, MidiEvent, SoundFont, Synth, SynthDescriptor};
use rodio::{ChannelCount, DeviceSinkBuilder, SampleRate, Source};

use wie_backend::{Ma2Event, Ma2Voice};

use crate::{
    audio_mix::{SYNTH_GAIN, SfxClips, sfx_key, sfx_trim, soft_limit},
    ma2_fm::{FmVoice, Ma2Fm},
    sfx_upsample::sfx_clip,
};

/// Live audio-init status line for the DIAG dialog (2026-08-07). A remote tester
/// cannot run adb, so the one line that says WHY sound is absent has to be
/// readable on the device itself — this is that line, written at every stage of
/// the pipeline bring-up and shown by `MainActivity.showDiagnostics`.
static AUDIO_STATUS: OnceLock<Mutex<String>> = OnceLock::new();

fn status_cell() -> &'static Mutex<String> {
    AUDIO_STATUS.get_or_init(|| Mutex::new("audio thread not started".into()))
}

fn set_status(s: String) {
    if let Ok(mut g) = status_cell().lock() {
        *g = s;
    }
}

/// The current audio pipeline status, human-readable.
pub fn audio_status() -> String {
    status_cell().lock().map(|g| g.clone()).unwrap_or_else(|_| "?".into())
}

/// Frames rendered per MIDI-drain cycle (~1.3 ms @ 48 kHz = negligible timing
/// jitter for game BGM, while keeping the per-sample path lock-free). Also the
/// granularity at which a new SFX clip joins the mix.
const CHUNK_FRAMES: usize = 64;
/// The synth is built for the device's rate; this is only the fallback if the device
/// reports something absurd (rodio's `SampleRate` is `NonZero`, so 0 cannot happen).
const FALLBACK_SAMPLE_RATE: u32 = 48_000;

/// GM SoundFont (SF3, Vorbis-compressed) bundled into the .so via `include_bytes!`.
static SF3_BYTES: &[u8] = include_bytes!("../assets/GeneralUser.sf3");

/// (channel_count, sampling_rate, interleaved i16 samples) — one-shot PCM clip.
pub type WaveMsg = (u8, u32, Vec<i16>);
/// Re-export so `platform.rs` can name the MIDI channel type without depending
/// on oxisynth directly.
pub type MidiMsg = MidiEvent;

/// One FM synth command (`AudioSink::fm_*`), in the order the player sent it. The voices are
/// decoded on the sending side, so the audio thread only swaps a table in.
pub enum FmMsg {
    Voices(u8, Vec<(u8, u8, FmVoice)>),
    Event(u8, Ma2Event),
    AllOff(u8),
}

/// In FM mode, how long the SoundFont synth must have been silent (no voices, no events)
/// before its render is skipped — long enough for any reverb tail to have died.
const SYNTH_IDLE_SECS: u32 = 3;

/// The music-bus level, `0..=100`, as `MC_mdaClipSetVolume` gave it.
///
/// Written by the emulator thread through `AudioSink::set_music_volume`, read by the
/// audio callback once per render chunk. A plain atomic rather than a channel message
/// because it is a LEVEL, not an event: only the newest value matters, and a level that
/// arrives one 1.5 ms chunk late is inaudible. Starts at 100 so a guest that never sets
/// a volume is not silent.
static MUSIC_VOLUME: AtomicU8 = AtomicU8::new(100);

/// Synth load gauge (2026-09-19). simpleperf on S9 put the `AudioTrack` thread at a
/// quarter of the process with 43 % of it inside per-voice DSP — a cost that is linear in
/// the number of voices the synth is rendering, which nothing reported. The audio callback
/// samples `Synth::playing_voices` every [`VOICE_GAUGE_EVERY`] chunks and the emulator's
/// periodic stats line prints it (`audio_stats_take`). Relaxed atomics: the callback
/// thread must never block, and a gauge that is one chunk stale is still a gauge.
static VOICES_LAST: AtomicU32 = AtomicU32::new(0);
static VOICES_MAX: AtomicU32 = AtomicU32::new(0);
static VOICES_SUM: AtomicU32 = AtomicU32::new(0);
static VOICES_SAMPLES: AtomicU32 = AtomicU32::new(0);
static QUIET60_SUM: AtomicU32 = AtomicU32::new(0);
static QUIET80_SUM: AtomicU32 = AtomicU32::new(0);
static NOTE_ONS: AtomicU32 = AtomicU32::new(0);
static NOTE_OFFS: AtomicU32 = AtomicU32::new(0);
/// 32 chunks × 64 frames = 2048 frames ≈ 43 ms at 48 kHz.
const VOICE_GAUGE_EVERY: u32 = 32;

/// `(last, max, avg, quiet60, quiet80, note_ons, note_offs)` since the previous call.
/// Read once per stats interval by the emulator thread.
///
/// ★ THE SUMS ARE TAKEN BEFORE THE DIVISOR (2026-09-20, review). The audio callback
/// keeps adding to these while this runs, so the three averages can only be consistent
/// if every sample in a sum is also in `n`. Taking `n` first put the samples added in
/// between into the sums but not the count, which makes the reported voice count read
/// HIGH — the direction that would send someone optimising a load that is not there.
/// This way a straddling tick lands in `n` but not in the sums, so the averages read a
/// touch low, which is the harmless direction. (Five relaxed swaps cannot be one atomic
/// read; what they can be is biased the right way.)
pub fn audio_stats_take() -> (u32, u32, u32, u32, u32, u32, u32) {
    let sum = VOICES_SUM.swap(0, Ordering::Relaxed);
    let quiet60 = QUIET60_SUM.swap(0, Ordering::Relaxed);
    let quiet80 = QUIET80_SUM.swap(0, Ordering::Relaxed);
    let n = VOICES_SAMPLES.swap(0, Ordering::Relaxed).max(1);
    (
        VOICES_LAST.load(Ordering::Relaxed),
        VOICES_MAX.swap(0, Ordering::Relaxed),
        sum / n,
        quiet60 / n,
        quiet80 / n,
        NOTE_ONS.swap(0, Ordering::Relaxed),
        NOTE_OFFS.swap(0, Ordering::Relaxed),
    )
}

/// ★ 2026-10-09 the host's own one-shots (the awakenings' voices, `voice.rs`): the queue the guest's effects take, so a
/// voice gets what an effect gets — the 효과음 크기 trim, the band-limited resample, the same-effect limit. Set once by
/// [`spawn_audio_thread`] — replaced, not kept, if a later platform spawns another: a sender held here would keep an old
/// audio thread (and its output) alive forever.
static HOST_WAVE: OnceLock<Mutex<Option<Sender<WaveMsg>>>> = OnceLock::new();

/// Queue one host clip (interleaved 16-bit PCM, `channels` wide, at `rate`, already at the level it should play).
/// `false` before the audio thread exists or after it is gone.
pub fn play_host_clip(channels: u8, rate: u32, samples: Vec<i16>) -> bool {
    HOST_WAVE
        .get()
        .and_then(|tx| tx.lock().ok())
        .is_some_and(|tx| tx.as_ref().is_some_and(|tx| tx.send((channels, rate, samples)).is_ok()))
}

/// Spawn the rodio output thread + MIDI synth + FM synth; returns the three senders the
/// `AudioSink` pushes to. Call once (from `AndroidPlatform::new`, worker thread). `fm` = the
/// songs go to the FM synth (`config::read_bgm_synth`).
pub fn spawn_audio_thread(paused: Arc<AtomicBool>, lite: bool, fm: bool) -> (Sender<WaveMsg>, Sender<MidiMsg>, Sender<FmMsg>) {
    let (wave_tx, wave_rx) = channel::<WaveMsg>();
    let (midi_tx, midi_rx) = channel::<MidiMsg>();
    let (fm_tx, fm_rx) = channel::<FmMsg>();
    if let Ok(mut host) = HOST_WAVE.get_or_init(|| Mutex::new(None)).lock() {
        *host = Some(wave_tx.clone());
    }
    thread::Builder::new()
        .name("wie-audio".into())
        .spawn(move || audio_thread(wave_rx, midi_rx, fm_rx, paused, lite, fm))
        .expect("failed to spawn audio thread");
    (wave_tx, midi_tx, fm_tx)
}

fn audio_thread(wave_rx: Receiver<WaveMsg>, midi_rx: Receiver<MidiMsg>, fm_rx: Receiver<FmMsg>, paused: Arc<AtomicBool>, lite: bool, fm: bool) {
    set_status("opening output device...".into());
    // ★ RETRY FOREVER instead of giving up (2026-08-07). A tester's device shipped
    // with no sound at all, and the old code turned ONE failed open — a cold-boot
    // race, another app briefly holding the device, a Bluetooth route mid-switch —
    // into a whole session of silence. AAudio exists on every device we install on
    // (minSdk 26 == the AAudio floor), so a persistent failure here is a condition
    // worth re-testing, not a permanent fact. Quick retries first (transient races),
    // then a slow patient loop. Queued events are drained each attempt so a long
    // retry cannot accumulate a session of SFX buffers.
    let output_sink = {
        let mut attempt = 0u32;
        loop {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(s) => break s,
                Err(e) => {
                    attempt += 1;
                    log::error!("audio: open_default_sink failed (attempt {attempt}): {e:?}");
                    set_status(format!("output open FAILED x{attempt}: {e:?}"));
                    while wave_rx.try_recv().is_ok() {}
                    while midi_rx.try_recv().is_ok() {}
                    while fm_rx.try_recv().is_ok() {}
                    thread::sleep(Duration::from_secs(if attempt < 5 { 1 } else { 10 }));
                }
            }
        }
    };
    // ★ The device decides the rate; the synth is built FOR it (see the module doc).
    let device_rate = output_sink.config().sample_rate().get();
    let device_rate = if (8_000..=192_000).contains(&device_rate) { device_rate } else { FALLBACK_SAMPLE_RATE };
    let (sfx_tx, sfx_rx) = channel::<(u64, Vec<f32>)>();

    // Build the MIDI synth. If the soundfont fails to load, PCM still works (the bus
    // simply has no synth); `Bus` is infinite on its own, so silence cannot detach it.
    let synth = match build_synth(lite, device_rate) {
        Ok(synth) => {
            log::info!("audio: MIDI synth (GeneralUser.sf3) loaded at {device_rate} Hz, gain {SYNTH_GAIN}");
            set_status(format!("OK (output open @ {device_rate} Hz, MIDI synth loaded)"));
            Some(synth)
        }
        Err(e) => {
            log::error!("audio: synth load failed ({e}); MIDI BGM disabled");
            set_status(format!("output open, but synth load FAILED: {e} (BGM off, SFX on)"));
            None
        }
    };
    output_sink.mixer().add(Bus::new(synth, midi_rx, sfx_rx, fm_rx, device_rate, paused, fm));
    // ★ 2026-10-09 the voices decoded here, once, long before the first cast — not on the worker's frame that cues one.
    let voices = (0..crate::voice::VOICE_FILES.len()).filter(|i| crate::voice::voice_clip(*i).is_some()).count();
    log::info!("audio: {voices}/{} voice clip(s) decoded", crate::voice::VOICE_FILES.len());

    log::info!(
        "audio: rodio output stream opened ({device_rate} Hz) · songs through {}",
        if fm { "their own FM voices (ma2_fm)" } else { "the General MIDI SoundFont" }
    );

    // PCM one-shots (SFX). Each clip is converted to the bus format HERE (this thread,
    // not the callback) and then joins the mix at the next chunk, so it plays
    // *immediately and concurrently*. The old sequential Player queue backlogged
    // rapid SFX (e.g. crit "cri!"), so they kept draining after the action ended.
    // ★ 2026-10-05: concurrently up to two copies of ONE effect (`audio_mix::SFX_SAME_MAX`)
    // — 지옥불 restarts its 967 ms effect every 60 ms and piled up 16 of them.
    while let Ok((channel, sampling_rate, wave_data)) = wave_rx.recv() {
        if channel == 0 || sampling_rate == 0 {
            continue;
        }
        // The trim is the headroom this bus gives up so the music has somewhere to sit;
        // the game's own effect slider was already applied to the i16 samples by
        // `SmafPlayer`.
        //
        // ★ Read PER CLIP, not once: the 효과음 크기 row applies at the next sound rather
        // than at the next app start, which is the only way a player can compare steps by
        // ear (hero3 2026-08-27).
        let trim = sfx_trim(crate::sfx_level());
        // ★ 2026-10-02: band-limited to the device rate, faded at its end and given the
        // octave above its own — see `sfx_upsample`. This was rodio's linear resampler.
        // Run once, here, and the callback only adds: 1 s of an 8 kHz clip is 48,000 output
        // frames × 32 taps, a few ms of this thread.
        let clip = sfx_clip(&wave_data, channel, sampling_rate, device_rate, trim);
        // ★ 2026-10-05: which effect it is, so the bus can stop one effect piling up on
        // itself (지옥불) — see `audio_mix::SfxClips`.
        let key = sfx_key(channel, sampling_rate, &wave_data);
        if sfx_tx.send((key, clip)).is_err() {
            break;
        }
    }
    // `output_sink` stays alive until the channel disconnects (app teardown).
}

/// The whole mix, as the single source rodio's device mixer pulls from.
///
/// Renders in [`CHUNK_FRAMES`]-frame chunks: MIDI drained, synth rendered × music gain,
/// clips added, `soft_limit` applied — all on the chunk buffer, so `next()` is an index.
///
/// Deliberately INFINITE — silence (no synth, no clips) yields zeros rather than `None`.
/// rodio drops a mixer input the moment it returns `None`, and a master bus that can be
/// dropped is a session of silence with no way back.
struct Bus {
    synth: Option<Synth>,
    midi_rx: Receiver<MidiMsg>,
    sfx_rx: Receiver<(u64, Vec<f32>)>,
    /// The songs' own FM voices (see the module doc), and their commands.
    fm: Ma2Fm,
    fm_rx: Receiver<FmMsg>,
    /// FM mode: the songs go to `fm`, and an idle `synth` is not rendered.
    fm_mode: bool,
    fm_buf: [f32; CHUNK_FRAMES * 2],
    /// Chunks the synth has been silent in a row, and how many make it idle
    /// ([`SYNTH_IDLE_SECS`]).
    synth_quiet: u32,
    synth_idle_after: u32,
    clips: SfxClips,
    sample_rate: u32,
    /// Shared background/lock gate. While set, render silence instead of the
    /// synth so no BGM continues when the app is backgrounded / the screen locks.
    paused: Arc<AtomicBool>,
    buf: [f32; CHUNK_FRAMES * 2], // interleaved L,R,L,R,... for the current chunk
    pos: usize,
    /// Chunks rendered, for the voice gauge cadence.
    chunks: u32,
}

impl Bus {
    fn new(
        synth: Option<Synth>,
        midi_rx: Receiver<MidiMsg>,
        sfx_rx: Receiver<(u64, Vec<f32>)>,
        fm_rx: Receiver<FmMsg>,
        sample_rate: u32,
        paused: Arc<AtomicBool>,
        fm_mode: bool,
    ) -> Self {
        Self {
            synth,
            midi_rx,
            sfx_rx,
            fm: Ma2Fm::new(sample_rate),
            fm_rx,
            fm_mode,
            fm_buf: [0.0; CHUNK_FRAMES * 2],
            synth_quiet: 0,
            synth_idle_after: SYNTH_IDLE_SECS * sample_rate / CHUNK_FRAMES as u32,
            clips: SfxClips::new(sample_rate),
            sample_rate,
            paused,
            buf: [0.0; CHUNK_FRAMES * 2],
            pos: 0,
            chunks: 0,
        }
    }

    /// One FM command onto the synth.
    fn apply_fm(fm: &mut Ma2Fm, msg: FmMsg) {
        match msg {
            FmMsg::Voices(part, table) => fm.set_voices(part as usize, table),
            FmMsg::AllOff(part) => fm.all_off(part as usize),
            FmMsg::Event(part, event) => {
                let p = part as usize;
                match event {
                    Ma2Event::Program { channel, bank, program } => fm.program(p, channel, bank, program),
                    Ma2Event::Volume { channel, value } => fm.volume(p, channel, value),
                    Ma2Event::Expression { channel, value } => fm.expression(p, channel, value),
                    Ma2Event::Pan { channel, value } => fm.pan(p, channel, value),
                    Ma2Event::PitchBend { channel, value } => fm.pitch_bend(p, channel, value),
                    Ma2Event::Modulation { channel, value } => fm.modulation(p, channel, value),
                    Ma2Event::NoteOn { channel, note } => {
                        NOTE_ONS.fetch_add(1, Ordering::Relaxed);
                        fm.note_on(p, channel, note);
                    }
                    Ma2Event::NoteOff { channel, note } => {
                        NOTE_OFFS.fetch_add(1, Ordering::Relaxed);
                        fm.note_off(p, channel, note);
                    }
                    Ma2Event::End => {}
                }
            }
        }
    }

    fn render_chunk(&mut self) {
        self.pos = 0;
        // Backgrounded / screen locked: emit a silent chunk WITHOUT advancing the
        // synth or draining MIDI events. The emulator worker is paused in
        // lock-step (no new events), so held notes freeze and BGM resumes
        // seamlessly on foreground. Effects in flight are dropped — a clip that
        // resumed seconds later, on foreground, would be a blip from nowhere.
        if self.paused.load(Ordering::Relaxed) {
            self.buf.fill(0.0);
            self.clips.clear();
            return;
        }
        // The game's 배경음 setting, as an OUTPUT gain. Read once per chunk (~1.3 ms),
        // so a mid-song slider change is heard immediately but costs one atomic load
        // per 64 frames rather than one per sample. See `AudioSink::set_music_volume`
        // for why this is not applied to note velocity any more.
        let gain = wie_backend::volume_gain(MUSIC_VOLUME.load(Ordering::Relaxed));
        match self.synth.as_mut() {
            Some(synth) => {
                // Apply all MIDI events that arrived since the last chunk.
                let mut arrived = false;
                while let Ok(ev) = self.midi_rx.try_recv() {
                    arrived = true;
                    match ev {
                        MidiEvent::NoteOn { .. } => NOTE_ONS.fetch_add(1, Ordering::Relaxed),
                        MidiEvent::NoteOff { .. } => NOTE_OFFS.fetch_add(1, Ordering::Relaxed),
                        _ => 0,
                    };
                    let _ = synth.send_event(ev);
                }
                self.chunks = self.chunks.wrapping_add(1);
                if self.chunks % VOICE_GAUGE_EVERY == 0 {
                    let fm_voices = if self.fm_mode { self.fm.active_voices() } else { 0 };
                    let v = synth.playing_voices() as u32 + fm_voices;
                    VOICES_LAST.store(v, Ordering::Relaxed);
                    VOICES_MAX.fetch_max(v, Ordering::Relaxed);
                    VOICES_SUM.fetch_add(v, Ordering::Relaxed);
                    QUIET60_SUM.fetch_add(synth.playing_voices_below(1e-3) as u32, Ordering::Relaxed);
                    QUIET80_SUM.fetch_add(synth.playing_voices_below(1e-4) as u32, Ordering::Relaxed);
                    VOICES_SAMPLES.fetch_add(1, Ordering::Relaxed);
                }
                // ★ FM mode: the songs never reach this synth, yet its reverb and chorus
                // cost the same in silence (`config::read_audio_lite`). Once it has been
                // silent for [`SYNTH_IDLE_SECS`] it is not rendered; a MIDI event wakes it.
                let quiet = self.fm_mode && !arrived && synth.playing_voices() == 0;
                self.synth_quiet = if quiet { self.synth_quiet.saturating_add(1) } else { 0 };
                if self.synth_quiet > self.synth_idle_after {
                    self.buf.fill(0.0);
                } else {
                    for frame in self.buf.chunks_exact_mut(2) {
                        let (l, r) = synth.read_next();
                        frame[0] = l * gain;
                        frame[1] = r * gain;
                    }
                }
            }
            None => self.buf.fill(0.0),
        }
        // The songs' own voices, at the same music gain.
        if self.fm_mode {
            while let Ok(msg) = self.fm_rx.try_recv() {
                Self::apply_fm(&mut self.fm, msg);
            }
            self.fm.render(&mut self.fm_buf);
            for (o, s) in self.buf.iter_mut().zip(self.fm_buf.iter()) {
                *o += s * gain;
            }
        }
        // New effects join at a chunk boundary (≤ 1.3 ms late — inaudible).
        // A copy of an effect already playing twice takes the oldest one's place.
        while let Ok((key, data)) = self.sfx_rx.try_recv() {
            self.clips.push(key, data);
        }
        self.clips.mix_into(&mut self.buf);
        // The last stage before the device: the SUM is soft-limited so nothing ever asks
        // AAudio to clamp. See [`crate::audio_mix::soft_limit`].
        for s in self.buf.iter_mut() {
            *s = soft_limit(*s);
        }
    }
}

impl Iterator for Bus {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.buf.len() {
            self.render_chunk();
        }
        let s = self.buf[self.pos];
        self.pos += 1;
        Some(s)
    }
}

impl Source for Bus {
    fn current_span_len(&self) -> Option<usize> {
        None // infinite, parameters never change
    }
    fn channels(&self) -> ChannelCount {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.sample_rate).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Voice ceiling.
///
/// ★ 256 (oxisynth's default) -> 64, and this is a CEILING, not a change of sound: the
/// pool grows lazily (`VoicePool::note_on` pushes a 4 KB `Voice` when it needs one), so
/// a song that never exceeds 64 simultaneous voices allocates and renders exactly what
/// it always did. Hero3's SMAF scores are 16-channel GM at ~12 note-ons a second and do
/// not come close. What the cap removes is the WORST case: a stuck or pathological score
/// driving 256 voices through fourth-order interpolation inside the AAudio callback,
/// which on a slower device is a guaranteed underrun. Measured at 200 notes/s: 8.9 % of
/// a core at 256 vs 2.5 % at 64, identical output at game density.
///
/// Also bounds the pool's memory at 256 KB instead of 1 MB.
const POLYPHONY: u16 = 64;

fn build_synth(lite: bool, sample_rate: u32) -> Result<Synth, String> {
    // ★ `lite` is OFF unless `wie_audio_lite.txt` says otherwise, so the default build
    // renders exactly what it rendered before: reverb on, chorus on, fourth-order
    // interpolation. See `config::read_audio_lite` for why this is a switch and not a
    // device-detected tier — the two settings below are AUDIBLE, and the rule for this
    // round is that the phone that already works must sound the same.
    let mut synth = Synth::new(SynthDescriptor {
        sample_rate: sample_rate as f32,
        polyphony: POLYPHONY,
        // ★ NOT oxisynth's default 0.2 — see `audio_mix::SYNTH_GAIN`. That default is a
        // desktop-synth safety margin for arbitrary 100-voice MIDI; this game's scores
        // peak at 0.45-1.18 at unity, and the 14 dB it was giving away is most of why
        // the music needed the phone at maximum volume.
        gain: SYNTH_GAIN,
        chorus_active: !lite,
        reverb_active: !lite,
        // Fourth-order interpolation is, by oxisynth's own docs, "50% of the whole DSP
        // processing time", and unlike the fx above it is per ACTIVE VOICE — so it
        // scales with how busy the music is, which is exactly when a slow device is in
        // trouble. Linear is the classic tracker-quality fallback.
        interpolation: if lite {
            InterpolationMethod::Linear
        } else {
            InterpolationMethod::default()
        },
        ..Default::default()
    })
    .map_err(|e| format!("Synth::new: {e:?}"))?;

    let mut cursor = Cursor::new(SF3_BYTES);
    let font = SoundFont::load(&mut cursor).map_err(|e| format!("SoundFont::load: {e:?}"))?;
    synth.add_font(font, true);
    Ok(synth)
}

pub struct AndroidAudioSink {
    wave_tx: Sender<WaveMsg>,
    midi_tx: Sender<MidiMsg>,
    fm_tx: Sender<FmMsg>,
    /// The FM switch (`config::read_bgm_synth`).
    fm: bool,
}

impl AndroidAudioSink {
    pub fn new(wave_tx: Sender<WaveMsg>, midi_tx: Sender<MidiMsg>, fm_tx: Sender<FmMsg>, fm: bool) -> Self {
        Self { wave_tx, midi_tx, fm_tx, fm }
    }
}

impl wie_backend::AudioSink for AndroidAudioSink {
    fn fm_supported(&self) -> bool {
        self.fm
    }

    fn fm_voices(&self, part: u8, voices: &[Ma2Voice]) {
        let table = voices.iter().map(|v| (v.bank, v.program, FmVoice::decode(&v.data))).collect();
        let _ = self.fm_tx.send(FmMsg::Voices(part, table));
    }

    fn fm_event(&self, part: u8, event: Ma2Event) {
        let _ = self.fm_tx.send(FmMsg::Event(part, event));
    }

    fn fm_all_off(&self, part: u8) {
        let _ = self.fm_tx.send(FmMsg::AllOff(part));
    }

    fn play_wave(&self, channel: u8, sampling_rate: u32, wave_data: &[i16]) {
        let _ = self.wave_tx.send((channel, sampling_rate, wave_data.to_vec()));
    }

    fn midi_note_on(&self, channel_id: u8, note: u8, velocity: u8) {
        // MIDI convention: note-on with velocity 0 == note-off.
        let ev = if velocity == 0 {
            MidiEvent::NoteOff { channel: channel_id, key: note }
        } else {
            MidiEvent::NoteOn {
                channel: channel_id,
                key: note,
                vel: velocity,
            }
        };
        let _ = self.midi_tx.send(ev);
    }

    fn midi_note_off(&self, channel_id: u8, note: u8, _velocity: u8) {
        let _ = self.midi_tx.send(MidiEvent::NoteOff { channel: channel_id, key: note });
    }

    fn midi_program_change(&self, channel_id: u8, program: u8) {
        let _ = self.midi_tx.send(MidiEvent::ProgramChange {
            channel: channel_id,
            program_id: program,
        });
    }

    fn midi_control_change(&self, channel_id: u8, control: u8, value: u8) {
        let _ = self.midi_tx.send(MidiEvent::ControlChange {
            channel: channel_id,
            ctrl: control,
            value,
        });
    }

    fn midi_pitch_bend(&self, channel_id: u8, value: u16) {
        let _ = self.midi_tx.send(MidiEvent::PitchBend { channel: channel_id, value });
    }

    /// oxisynth's dedicated all-sound-off: every voice on the channel stops at once. The
    /// control-change route (`ControlChange { ctrl: 120 }`) only RELEASES them — see
    /// `wie_backend::AudioSink::midi_all_sound_off` for why a replaced song needs this.
    fn midi_all_sound_off(&self, channel_id: u8) {
        let _ = self.midi_tx.send(MidiEvent::AllSoundOff { channel: channel_id });
    }

    fn midi_sysex(&self, _data: &[u8]) {
        // oxisynth has no raw-sysex event; the game's sysex is a GM reset, which
        // the synth already defaults to. Safe to ignore.
    }

    fn set_music_volume(&self, volume: u8) -> bool {
        let v = volume.min(100);
        // Logged only on an actual change, which is once per 배경음 setting change —
        // so it is the one line that says what the music bus is doing without being
        // chatty. (Effects never reach here: see `Score::is_music`.)
        if MUSIC_VOLUME.swap(v, Ordering::Relaxed) != v {
            log::info!("audio: music bus level {v} (gain {:.3})", wie_backend::volume_gain(v));
        }
        // We own a real music bus, so the player must leave note velocity alone.
        true
    }
}
