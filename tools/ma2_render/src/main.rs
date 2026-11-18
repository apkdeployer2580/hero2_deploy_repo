//! hero2's MA-2 songs offline, through the code the app ships (`HERO2_BGM_FM_PLAN.md` §3 B1/B2).
//!
//! ```text
//! python tools/ma2_render/extract_songs.py dist/영웅서기2-1.zip dist/bgm_audit/songs
//! cargo run --release --manifest-path tools/ma2_render/Cargo.toml -- dump <song>... [--out <dir>]
//! ```
//!
//! `dump` parses each song with the shipping parser (`wie_backend::system::ma2`, included here
//! as source) and prints one summary line; with `--out` it also writes `<name>.ma2.txt` — every
//! key-on / key-off as `on|off ms channel note`, every level change and every voice — for
//! `tools/ma2_render/compare.py` to hold against the independent Python reading.
//!
//! `render` plays one song through the shipping FM synth (`wie_android/src/ma2_fm.rs`) the way
//! the app's player does — voices first, events on their millisecond, looping at the end — and
//! writes a WAV (after the app's soft limiter), printing RMS and the pre-limiter peak.
//! `--solo <program>` / `--solo drums` keeps one instrument, for balance work. It is the bench
//! for tuning the synth's constants by ear: edit `ma2_fm.rs`, rebuild (under a second), render.
//!
//! ```text
//! cargo run --release --manifest-path tools/ma2_render/Cargo.toml -- render dist/bgm_audit/songs/02 02.wav --secs 60
//! ```
//!
//! `sfx` does the same for an effect (an `ATR` file: `extract_songs.py … --effects`), through
//! the shipping decoder (`wie_backend::system::atr`) and the shipping upsampler
//! (`wie_android/src/sfx_upsample.rs`). `--mode` picks how much of that path is used, so the
//! steps can be told apart by ear and by number:
//!
//! ```text
//! old     what the app played before 2026-10-02: the other chip's nibble order + a straight line
//! decode  the right samples, still joined by a straight line
//! sinc    the right samples, band-limited, faded at the end — no regenerated octave
//! ship    what the app plays (the default)
//! ```

extern crate alloc;

#[allow(dead_code)]
mod ma2 {
    include!("../../../vendor/wie/wie_backend/src/system/ma2.rs");
}

#[allow(dead_code)]
mod fm {
    include!("../../../wie_android/src/ma2_fm.rs");
}

#[allow(dead_code)]
mod atr {
    include!("../../../vendor/wie/wie_backend/src/system/atr.rs");
}

#[allow(dead_code)]
mod upsample {
    include!("../../../wie_android/src/sfx_upsample.rs");
}

use std::{fmt::Write as _, fs, io::Write as _, path::PathBuf};

use fm::{FmVoice, Ma2Fm};
use ma2::{Ma2Event, Ma2Song, parse_ma2};

fn usage() -> ! {
    eprintln!("usage: ma2_render dump <song>... [--out <dir>]");
    eprintln!("       ma2_render render <song> <out.wav> [--secs 60] [--rate 44100] [--solo <program>|drums]");
    eprintln!("       ma2_render sfx <effect> <out.wav> [--mode old|decode|sinc|ship] [--rate 48000] [--gain 0.4]");
    std::process::exit(2);
}

/// The app's master soft limiter (`wie_android/src/audio_mix.rs`), so a render peaks the way
/// the phone does.
fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.7;
    let a = x.abs();
    if a <= KNEE {
        return x;
    }
    let head = 1.0 - KNEE;
    let over = a - KNEE;
    let y = KNEE + head * (over / (over + head));
    if x < 0.0 {
        -y
    } else {
        y
    }
}

fn write_wav(path: &PathBuf, rate: u32, pcm: &[f32]) {
    let mut f = std::io::BufWriter::new(fs::File::create(path).expect("create wav"));
    let nb = (pcm.len() * 2) as u32;
    let mut h = Vec::new();
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + nb).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&2u16.to_le_bytes());
    h.extend_from_slice(&rate.to_le_bytes());
    h.extend_from_slice(&(rate * 4).to_le_bytes());
    h.extend_from_slice(&4u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&nb.to_le_bytes());
    f.write_all(&h).expect("write wav");
    for s in pcm {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).expect("write wav");
    }
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Play `song` through the FM engine the way the app's player does — voices at the start,
/// events at their millisecond, the loop at `End` — for `secs` seconds.
fn render(args: &[String]) {
    let (Some(song_path), Some(out)) = (args.first(), args.get(1)) else { usage() };
    let (mut secs, mut rate, mut solo): (f32, u32, Option<String>) = (60.0, 44_100, None);
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        let v = it.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--secs" => secs = v.parse().unwrap_or_else(|_| usage()),
            "--rate" => rate = v.parse().unwrap_or_else(|_| usage()),
            "--solo" => solo = Some(v.clone()),
            _ => usage(),
        }
    }
    let bytes = fs::read(song_path).expect("read song");
    let song = parse_ma2(&bytes).unwrap_or_else(|| {
        eprintln!("{song_path}: not an HPS song");
        std::process::exit(1);
    });
    let mut fm = Ma2Fm::new(rate);
    fm.set_voices(0, song.voices.iter().map(|v| (v.bank, v.program, FmVoice::decode(&v.data))).collect());
    let mut program = [(0u8, 0u8); ma2::MA2_CHANNELS];
    let heard = |(bank, pc): (u8, u8)| match solo.as_deref() {
        None => true,
        Some("drums") => bank & 0x80 != 0,
        Some(p) => bank & 0x80 == 0 && p.parse::<u8>().ok() == Some(pc),
    };

    let total = (secs * rate as f32) as usize;
    let mut pcm = vec![0.0f32; total * 2];
    let mut done = 0usize;
    let mut base_ms = 0u64;
    'song: loop {
        for (t, e) in &song.events {
            let at = ((base_ms + *t as u64) * rate as u64 / 1000) as usize;
            if at > done {
                let upto = at.min(total);
                fm.render(&mut pcm[done * 2..upto * 2]);
                done = upto;
            }
            if done >= total {
                break 'song;
            }
            match *e {
                Ma2Event::Program { channel, bank, program: p } => {
                    program[channel as usize] = (bank, p);
                    fm.program(0, channel, bank, p);
                }
                Ma2Event::Volume { channel, value } => fm.volume(0, channel, value),
                Ma2Event::Expression { channel, value } => fm.expression(0, channel, value),
                Ma2Event::Pan { channel, value } => fm.pan(0, channel, value),
                Ma2Event::PitchBend { channel, value } => fm.pitch_bend(0, channel, value),
                Ma2Event::Modulation { channel, value } => fm.modulation(0, channel, value),
                Ma2Event::NoteOn { channel, note } => {
                    if heard(program[channel as usize]) {
                        fm.note_on(0, channel, note);
                    }
                }
                Ma2Event::NoteOff { channel, note } => fm.note_off(0, channel, note),
                Ma2Event::End => {}
            }
        }
        base_ms += song.len_ms as u64;
    }
    if done < total {
        fm.render(&mut pcm[done * 2..]);
    }
    let raw_peak = pcm.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    for s in pcm.iter_mut() {
        *s = soft_limit(*s);
    }
    let rms = (pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len().max(1) as f32).sqrt();
    write_wav(&PathBuf::from(out), rate, &pcm);
    println!("{out}: {secs} s @ {rate} Hz · RMS {:.1} dBFS · peak before limiter {:.1} dBFS", db(rms), db(raw_peak));
}

/// What `smaf_player` made of an effect's bytes before 2026-10-02 — Yamaha ADPCM-B, the HIGH
/// nibble first. Kept here, and only here, as the "before" of an A/B.
fn decode_adpcm_b(data: &[u8]) -> Vec<i16> {
    const STEP: [u32; 8] = [57, 57, 57, 57, 77, 102, 128, 153];
    let (mut value, mut step) = (0i32, 127u32);
    let mut out = Vec::with_capacity(data.len() * 2);
    for byte in data {
        for nibble in [byte >> 4, byte & 0x0f] {
            let diff = (((1 + ((nibble & 7) as u32 * 2)) * step) >> 3) as i32;
            value = (if nibble & 8 != 0 { value - diff } else { value + diff }).clamp(-32768, 32767);
            step = ((STEP[(nibble & 7) as usize] * step) >> 6).clamp(127, 24576);
            out.push(value as i16);
        }
    }
    out
}

/// The first `Awa` chunk of an effect file's `ATR` track, raw.
fn wave_bytes(file: &[u8]) -> Option<&[u8]> {
    let be32 = |at: usize| Some(u32::from_be_bytes(file.get(at..at + 4)?.try_into().ok()?) as usize);
    let end = 8 + be32(4)? - 2;
    let mut at = 8;
    while at + 8 <= end {
        let len = be32(at + 4)?;
        if &file[at..at + 3] == b"ATR" {
            let (mut sub, sub_end) = (at + 8 + 6, at + 8 + len);
            while sub + 8 <= sub_end {
                let sub_len = be32(sub + 4)?;
                if &file[sub..sub + 3] == b"Awa" {
                    return file.get(sub + 8..sub + 8 + sub_len);
                }
                sub += 8 + sub_len;
            }
        }
        at += 8 + len;
    }
    None
}

/// The straight line rodio's resampler drew between samples, to stereo.
fn linear_stereo(samples: &[i16], in_rate: u32, out_rate: u32, gain: f32) -> Vec<f32> {
    let n = samples.len() as u64 * out_rate as u64 / in_rate as u64;
    let mut out = Vec::with_capacity(n as usize * 2);
    for i in 0..n {
        let pos = i * in_rate as u64;
        let k = (pos / out_rate as u64) as usize;
        let frac = (pos % out_rate as u64) as f32 / out_rate as f32;
        let a = samples[k] as f32 / 32768.0;
        let b = samples.get(k + 1).map_or(a, |s| *s as f32 / 32768.0);
        let v = (a + (b - a) * frac) * gain;
        out.extend_from_slice(&[v, v]);
    }
    out
}

/// One effect to a WAV, through as much of the shipping path as `--mode` asks for.
fn sfx(args: &[String]) {
    let (Some(path), Some(out)) = (args.first(), args.get(1)) else { usage() };
    let (mut mode, mut rate, mut gain) = (String::from("ship"), 48_000u32, 0.4f32);
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        let v = it.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--mode" => mode = v.clone(),
            "--rate" => rate = v.parse().unwrap_or_else(|_| usage()),
            "--gain" => gain = v.parse().unwrap_or_else(|_| usage()),
            _ => usage(),
        }
    }
    let bytes = fs::read(path).expect("read effect");
    let waves = atr::parse_atr_waves(&bytes).unwrap_or_else(|| {
        eprintln!("{path}: not an ATR effect the app decodes itself (it would keep smaf_player's samples)");
        std::process::exit(1);
    });
    let Some(wave) = waves.first() else {
        eprintln!("{path}: plays no wave");
        std::process::exit(1);
    };
    let in_rate = wave.sampling_rate;
    let pcm = match mode.as_str() {
        "old" => linear_stereo(&decode_adpcm_b(wave_bytes(&bytes).expect("Awa chunk")), in_rate, rate, gain),
        "decode" => linear_stereo(&wave.data, in_rate, rate, gain),
        "sinc" => {
            // `sfx_clip` without its last step.
            let frames = wave.data.len();
            let fade = (in_rate as usize * 4 / 1000).min(frames);
            let mut x: Vec<f32> = wave.data.iter().map(|s| *s as f32 / 32768.0).collect();
            for (i, s) in x[frames - fade..].iter_mut().enumerate() {
                *s *= 0.5 * (1.0 + (std::f32::consts::PI * (i + 1) as f32 / fade as f32).cos());
            }
            upsample::sfx_resample(&x, in_rate, rate).iter().flat_map(|s| [s * gain, s * gain]).collect()
        }
        "ship" => upsample::sfx_clip(&wave.data, 1, in_rate, rate, gain),
        _ => usage(),
    };
    let n = pcm.len().max(1) as f32;
    let mean = pcm.iter().sum::<f32>() / n;
    let rms = (pcm.iter().map(|s| s * s).sum::<f32>() / n).sqrt();
    let ac = (pcm.iter().map(|s| (s - mean) * (s - mean)).sum::<f32>() / n).sqrt();
    let peak = pcm.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let last = pcm.chunks(2).rev().find(|f| f[0] != 0.0).map_or(0.0, |f| f[0]);
    write_wav(&PathBuf::from(out), rate, &pcm);
    println!(
        "{out}: {mode} · {in_rate} Hz -> {rate} Hz · {:.0} ms · RMS {:.1} dBFS (without its offset {:.1}) · offset {:+.4} · peak {:.1} dBFS · ends at {:+.4}",
        pcm.len() as f32 / 2.0 / rate as f32 * 1000.0,
        db(rms),
        db(ac),
        mean,
        db(peak),
        last
    );
}

/// Notes as `(on, off, channel, note)`, paired from the event list the way a synth sees it.
fn notes(song: &Ma2Song) -> Vec<(u32, u32, u8, u8)> {
    let mut held: [Option<(u32, u8)>; ma2::MA2_CHANNELS] = [None; ma2::MA2_CHANNELS];
    let mut out = Vec::new();
    for (t, e) in &song.events {
        match *e {
            Ma2Event::NoteOn { channel, note } => {
                if let Some((on, n)) = held[channel as usize].take() {
                    // Re-struck before its key-off: the songs never do this.
                    out.push((on, *t, channel, n));
                    eprintln!("  ! channel {channel}: note {n} re-struck at {t} ms");
                }
                held[channel as usize] = Some((*t, note));
            }
            Ma2Event::NoteOff { channel, note } => {
                if let Some((on, n)) = held[channel as usize] {
                    if n == note {
                        out.push((on, *t, channel, n));
                        held[channel as usize] = None;
                    }
                }
            }
            _ => {}
        }
    }
    out.sort();
    out
}

fn dump(args: &[String]) {
    let mut out_dir: Option<PathBuf> = None;
    let mut songs = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--out" {
            out_dir = Some(PathBuf::from(it.next().unwrap_or_else(|| usage())));
        } else {
            songs.push(PathBuf::from(a));
        }
    }
    if songs.is_empty() {
        usage();
    }
    let (mut total_notes, mut total_voices) = (0usize, 0usize);
    for path in &songs {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{name}: {e}");
                std::process::exit(1);
            }
        };
        let Some(song) = parse_ma2(&bytes) else {
            println!("{name}: NOT PARSED (the app would play it through General MIDI)");
            continue;
        };
        let notes = notes(&song);
        total_notes += notes.len();
        total_voices += song.voices.len();
        let past_end = notes.iter().filter(|n| n.1 > song.len_ms).count();
        println!(
            "{name}: len {} ms · {} voices · {} notes · rhythm {:#06x} · exclusive skipped {} · notes past the end {}",
            song.len_ms,
            song.voices.len(),
            notes.len(),
            song.rhythm,
            song.exclusive_ignored,
            past_end
        );
        if let Some(dir) = &out_dir {
            let mut s = String::new();
            let _ = writeln!(s, "len {}", song.len_ms);
            for v in &song.voices {
                let hex: String = v.data.iter().map(|b| format!("{b:02x}")).collect();
                let _ = writeln!(s, "voice {} {} {hex}", v.bank, v.program);
            }
            for (t, e) in &song.events {
                let line = match *e {
                    Ma2Event::NoteOn { channel, note } => format!("on {t} {channel} {note}"),
                    Ma2Event::NoteOff { channel, note } => format!("off {t} {channel} {note}"),
                    Ma2Event::Program { channel, bank, program } => format!("prog {t} {channel} {bank} {program}"),
                    Ma2Event::Volume { channel, value } => format!("vol {t} {channel} {value}"),
                    Ma2Event::Expression { channel, value } => format!("expr {t} {channel} {value}"),
                    Ma2Event::Pan { channel, value } => format!("pan {t} {channel} {value}"),
                    Ma2Event::PitchBend { channel, value } => format!("bend {t} {channel} {value}"),
                    Ma2Event::Modulation { channel, value } => format!("mod {t} {channel} {value}"),
                    _ => continue,
                };
                let _ = writeln!(s, "{line}");
            }
            fs::create_dir_all(dir).expect("out dir");
            fs::write(dir.join(format!("{name}.ma2.txt")), s).expect("write dump");
        }
    }
    println!("total: {} songs · {total_voices} voices · {total_notes} notes", songs.len());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("dump") => dump(&args[2..]),
        Some("render") => render(&args[2..]),
        Some("sfx") => sfx(&args[2..]),
        _ => usage(),
    }
}
