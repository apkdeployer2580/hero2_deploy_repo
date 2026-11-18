//! 세이브 옮기기 (2026-10-02) — the one file that carries a phone's hero2 game state to another.
//! ★ 2026-10-28: the file format, the zip reader and the apply are `crate::savebundle`'s (the
//! ChronosSword engine with its four reviews' fixes); this module is the IMPORT POLICY — what a
//! file from outside may carry — and the lines the dialogs show.
//!
//! ── WHY ─────────────────────────────────────────────────────────────────────
//! User: *"A핸드폰에서 B핸드폰으로 세이브 데이터를 추출해서 보내고싶어 … 사용자에게도 설명해야해"*.
//! Nothing else can: the saves live in app-private storage, the release build is not
//! debuggable (`run-as` refuses), and `allowBackup=false` keeps them out of adb, cloud and
//! Smart Switch backups.
//!
//! ── WHAT TRAVELS ────────────────────────────────────────────────────────────
//! The files themselves, byte for byte (`savebundle::names`):
//!   `fs/n1..n5 · g1..g5`   the game's save pairs (`/n` bags · `/g` character, quickbar, materials)
//!   `account/bag · materials`  the ACCOUNT warehouse the five slots share
//!   `avatar/<slot>` · `avatar/<slot>.pending`  the costume records. ⛔ They are bound to the
//!                          hash of the `/g` they were committed with (`avatar_state`), so a
//!                          `/g` that travels without its record loads its character with no
//!                          costumes — they go together or the costumes are lost
//!   `host/wie_quickpages.txt` and the play rules (`savebundle::HOST_FILES`)
//! Not the vault, not what belongs to the device (layout, controls, sound, heat saver), and — since
//! 2026-10-28 — not `fs/cf`: an old file's copy is read and ignored (`savebundle::owned`).
//!
//! ── THE POLICY (2026-10-28) ─────────────────────────────────────────────────
//! [`parse_import`] = `savebundle::parse` (integrity) + [`import_policy`]: every slot a whole pair
//! whose halves the GAME can read — `/n` three blobs and `/g` five, each with the checksum the
//! game's own `bg.a` appends (chronos review 1 P2: a file that only LOOKED like a save passed); a
//! `/g` whose class and level index the game's own tables; at least one slot; every costume record
//! one `avatar_state::decode_record` reads; the host files text. The export runs the same policy
//! on what it is about to send (chronos review 3 P1: never make a file the other phone refuses).

use std::io::{Cursor, Read};

pub use crate::savebundle::{ACCOUNT_FILES, Area, Bundle, FILE_MAX, GAME_SETTINGS, HOST_FILES, MEMBER_MAX, aid_ok, names};
use crate::savebundle::{self, Reject};
use crate::savevault;

/// The AID of the archive this app runs — the `AID:` line of its `__adf__`, read the way
/// `wie_ktf::adf::KtfAdf::parse` reads it, i.e. the directory the guest filesystem keeps
/// everything under. Read from the bundled game itself so a phone that has never saved (and
/// so has no such directory yet) knows it too. `None` when it cannot be read or is not
/// [`aid_ok`].
pub fn archive_aid(game_zip: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(game_zip)).ok()?;
    let mut adf = Vec::new();
    zip.by_name("__adf__").ok()?.take(64 * 1024).read_to_end(&mut adf).ok()?;
    let line = adf.split(|b| *b == b'\n').find(|l| l.starts_with(b"AID:"))?;
    let aid = String::from_utf8_lossy(&line[4..]).into_owned();
    aid_ok(&aid).then_some(aid)
}

/// `bg.a([B[B)[B`'s three keys (`ag.n` · `ag.o` · `ag.p`; `avatar_state` holds the `/g` order).
const KEY_N: [u8; 6] = [0x05, 0x47, 0x18, 0x3B, 0x2B, 0x10];
const KEY_O: [u8; 6] = [0x0E, 0x1A, 0x48, 0x0A, 0x1C, 0x62];
const KEY_P: [u8; 6] = [0x43, 0x0C, 0x23, 0x44, 0x5B, 0x2B];

/// Is `n` a whole `/n<slot>` — the three blobs `ag.f(B)V` @0x1147d8 writes back to back
/// (`h.a(S[BI)V` length + `bg.a` body), keyed `ag.p` · `ag.n` · `ag.o`, each ending in the checksum
/// `bg.a` appends, and nothing after them? Measured on the three test saves (1,352 · 1,338 ·
/// 1,592 B → 18 + bags + cards). The game reads it with `ag.h(B)V` on 이어하기; one that is not
/// whole is a slot the receiving phone cannot load.
pub fn n_complete(n: &[u8]) -> bool {
    let mut p = 0usize;
    for key in [KEY_P, KEY_N, KEY_O] {
        if p + 2 > n.len() {
            return false;
        }
        let len = usize::from(u16::from_be_bytes([n[p], n[p + 1]]));
        p += 2;
        if len == 0 || p + len > n.len() {
            return false;
        }
        let body = len - 1;
        let sum: u32 = (0..body).map(|i| u32::from(key[(i + 1) % key.len()])).sum();
        if u32::from(n[p + body]) != sum & 0xff {
            return false;
        }
        p += len;
    }
    p == n.len()
}

/// Is `g` a whole `/g` whose class and level the game's own tables can index — the title's slot
/// screen draws both for every slot (`be.a(Graphics,IIB)V`), so a `/g` that frames well but says
/// class 7 would throw on the screen the player has to pass to reach anything.
pub fn g_sane(g: &[u8]) -> bool {
    crate::avatar_state::g_complete(g) && crate::avatar_state::g_class(g).is_some_and(|c| c <= 2) && g_level(g).is_some_and(|l| (1..=99).contains(&l))
}

/// The level a `/g<slot>` records — byte 0 of its second blob (`a.b()[B`, key `ag.o`;
/// `tools_re/h2save.py`), the number the game's own slot screen shows. `None` for a file that
/// is not a whole save. Measured on the three test saves: 48 · 26 · 47.
pub fn g_level(g: &[u8]) -> Option<u8> {
    if !crate::avatar_state::g_complete(g) {
        return None;
    }
    // `g_complete` walked the five blobs: both length prefixes are inside the file.
    let second = 2 + usize::from(u16::from_be_bytes([g[0], g[1]]));
    let n = usize::from(u16::from_be_bytes([g[second], g[second + 1]]));
    // The level and the checksum byte at least.
    if n < 2 {
        return None;
    }
    Some(g[second + 2] ^ KEY_O[1 % KEY_O.len()])
}

/// The class the way the game's own screens name it.
pub fn class_ko(class: u8) -> &'static str {
    match class {
        0 => "버서커",
        1 => "나이트",
        _ => "로그",
    }
}

/// One slot as the Java dialogs show it: `슬롯 1 · 나이트 Lv 48 · HELL` (the round only past the
/// first — every new game plays NORMAL, so saying it would be noise).
pub fn slot_line(slot: u8, g: Option<&[u8]>) -> String {
    let mut s = format!("슬롯 {slot}");
    let Some(g) = g else { return s };
    if let (Some(class), Some(level)) = (crate::avatar_state::g_class(g), g_level(g)) {
        s.push_str(&format!(" · {} Lv {level}", class_ko(class)));
    }
    match crate::avatar_state::g_progress(g).map(|p| p.mode) {
        Some(1) => s.push_str(" · HARDCORE"),
        Some(2) => s.push_str(" · HELL"),
        Some(3) => s.push_str(" · GUARDIAN"),
        _ => {}
    }
    s
}

/// One slot as the GAME's box shows it — every character one the game's font has (no `·`, no
/// lower case): `슬롯1 나이트 LV48 HELL`.
pub fn slot_box_line(slot: u8, g: Option<&[u8]>) -> String {
    let mut s = format!("슬롯{slot}");
    let Some(g) = g else { return s };
    if let (Some(class), Some(level)) = (crate::avatar_state::g_class(g), g_level(g)) {
        s.push_str(&format!(" {} LV{level}", class_ko(class)));
    }
    match crate::avatar_state::g_progress(g).map(|p| p.mode) {
        Some(1) => s.push_str(" HARD"),
        Some(2) => s.push_str(" HELL"),
        Some(3) => s.push_str(" GUARD"),
        _ => {}
    }
    s
}

/// What a bundle carries beyond its slots, for the dialogs: `계정창고 · 아바타 2벌 · 퀵슬롯`.
pub fn extras_line(b: &Bundle) -> String {
    let mut parts: Vec<String> = Vec::new();
    if ACCOUNT_FILES.iter().any(|f| b.get(Area::Account, f).is_some()) {
        parts.push("계정창고".to_string());
    }
    let records = b.files.iter().filter(|(a, n, _)| *a == Area::Avatar && !n.ends_with(".pending")).count();
    if records > 0 {
        parts.push(format!("아바타 기록 {records}개"));
    }
    if b.get(Area::Host, HOST_FILES[0]).is_some() {
        parts.push("퀵슬롯".to_string());
    }
    if HOST_FILES[1..].iter().any(|f| b.get(Area::Host, f).is_some()) {
        parts.push("게임 설정".to_string());
    }
    parts.join(" · ")
}

/// The player's sentences. Plain words with spaces, no `;` — the Java dialogs show them as they
/// are, and the game's box wraps them at the spaces (`hero2java::h2_box_wrap`).
pub const NOT_OURS: &str = "영웅서기2 세이브 파일이 아닙니다. 내보내기로 만든 파일을 압축을 풀지 말고 그대로 골라 주세요.";
pub const DAMAGED: &str = "세이브 파일이 손상됐습니다. 보낸 폰에서 다시 내보내 주세요.";
pub const OTHER_FORMAT: &str = "다른 버전의 세이브 파일입니다. 두 폰의 앱을 같은 최신 버전으로 맞춘 뒤 다시 내보내 주세요.";
pub const NO_SLOT: &str = "이 파일에는 세이브가 없습니다. 보낸 폰에서 먼저 저장한 뒤 다시 내보내 주세요.";

fn refuse(msg: &str, why: String) -> String {
    log::warn!("save transfer: refused: {why}");
    msg.to_string()
}

/// What a file from outside may carry (module header). `Err` = the player's sentence (the cause is
/// logged). Runs on an export too, before the file leaves the phone.
pub fn import_policy(b: &Bundle) -> Result<(), String> {
    for slot in savevault::SLOT_MIN..=savevault::SLOT_MAX {
        let n = b.get(Area::Fs, &savevault::guest_path(savevault::Half::Char, slot));
        let g = b.get(Area::Fs, &savevault::guest_path(savevault::Half::Prog, slot));
        match (n, g) {
            (None, None) => {}
            (Some(n), Some(g)) if savevault::usable_len(n.len()) && savevault::usable_len(g.len()) && n_complete(n) && g_sane(g) => {}
            (n, g) => {
                return Err(refuse(
                    DAMAGED,
                    format!(
                        "slot {slot}: n {:?} B (whole {:?}), g {:?} B (sane {:?})",
                        n.map(<[u8]>::len),
                        n.map(n_complete),
                        g.map(<[u8]>::len),
                        g.map(g_sane)
                    ),
                ));
            }
        }
    }
    if b.slots().is_empty() {
        return Err(refuse(NO_SLOT, "no slot".to_string()));
    }
    for (area, name, bytes) in &b.files {
        let ok = match area {
            Area::Avatar => std::str::from_utf8(bytes).ok().is_some_and(|t| crate::avatar_state::decode_record(t).is_ok()),
            // An empty warehouse still serialises to one byte (`bq.a()[B`).
            Area::Account => !bytes.is_empty(),
            Area::Host => std::str::from_utf8(bytes).is_ok(),
            Area::Fs => true,
        };
        if !ok {
            return Err(refuse(DAMAGED, format!("{}/{name}: not a file this build reads ({} B)", area.dir(), bytes.len())));
        }
    }
    Ok(())
}

/// A file from outside, for 세이브 불러오기: [`savebundle::parse`] then [`import_policy`]. `Err` is
/// a sentence for the player.
pub fn parse_import(bytes: &[u8]) -> Result<Bundle, String> {
    let b = savebundle::parse(bytes).map_err(|r| match r {
        Reject::NotOurs(why) => refuse(NOT_OURS, why),
        Reject::Damaged(why) => refuse(DAMAGED, why),
        Reject::Format(why) => refuse(OTHER_FORMAT, why),
    })?;
    import_policy(&b)?;
    Ok(b)
}

// Run on the phone like the rest of this crate's tests (CLAUDE.md 「셀프테스트」). `pub(crate)` so
// the vault's transfer tests can build the same whole `/g`.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// `bg.a([B[B)[B` as the game runs it (`tools_re/h2save.py encrypt`).
    fn encrypt(plain: &[u8], key: &[u8; 6]) -> Vec<u8> {
        let mut out: Vec<u8> = plain.iter().enumerate().map(|(i, b)| b ^ key[(i + 1) % 6]).collect();
        out.push((0..plain.len()).map(|i| u32::from(key[(i + 1) % 6])).sum::<u32>() as u8);
        out
    }

    fn framed(blobs: &[(Vec<u8>, [u8; 6])], tail: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for (p, k) in blobs {
            let e = encrypt(p, k);
            out.extend_from_slice(&(e.len() as u16).to_be_bytes());
            out.extend_from_slice(&e);
        }
        out.extend(std::iter::repeat(0u8).take(tail));
        out
    }

    /// A whole `/g` holding `class`, `level` and `mode`.
    pub(crate) fn fake_g(class: u8, level: u8, mode: u8) -> Vec<u8> {
        let mut third = vec![0u8; 80];
        third[32 + 32 + 3 + 4] = mode;
        let g = framed(
            &[
                (vec![40, 20, 28, 2, class], KEY_N),
                ((0..60).map(|i| if i == 0 { level } else { 1 }).collect(), KEY_O),
                (third, KEY_P),
                (vec![0u8; 12], KEY_N),
                (vec![0u8; 20], KEY_O),
            ],
            4,
        );
        assert!(crate::avatar_state::g_complete(&g));
        g
    }

    /// A whole `/n`.
    pub(crate) fn fake_n() -> Vec<u8> {
        framed(&[(vec![3u8; 17], KEY_P), (vec![5u8; 40], KEY_N), (vec![7u8; 20], KEY_O)], 0)
    }

    fn sample() -> Bundle {
        Bundle {
            aid: "01035D0B".into(),
            created_ms: 1_790_000_000_000,
            app: 159,
            backup: false,
            files: vec![
                (Area::Fs, "n1".into(), fake_n()),
                (Area::Fs, "g1".into(), fake_g(1, 48, 2)),
                (Area::Account, "bag".into(), vec![9u8; 83]),
                (Area::Account, "materials".into(), vec![0u8; 1]),
                (Area::Host, "wie_quickpages.txt".into(), b"own 1 1\n".to_vec()),
                (Area::Host, "wie_difficulty.txt".into(), b"1".to_vec()),
            ],
        }
    }

    #[test]
    fn a_bundle_passes_the_policy_and_reads_back() {
        let b = sample();
        let bytes = savebundle::encode(&b);
        assert_eq!(parse_import(&bytes).unwrap(), b);
        assert_eq!(extras_line(&b), "계정창고 · 퀵슬롯 · 게임 설정");
    }

    #[test]
    fn the_saves_must_be_ones_the_game_reads() {
        assert!(n_complete(&fake_n()));
        let mut torn = fake_n();
        torn.pop();
        assert!(!n_complete(&torn), "a /n a byte short");
        let mut sum = fake_n();
        sum[5] ^= 0x40;
        // the body changed, the checksum is over the KEY — still whole (the checksum is the game's)…
        assert!(n_complete(&sum));
        let mut bad_sum = fake_n();
        let first = 2 + usize::from(u16::from_be_bytes([bad_sum[0], bad_sum[1]]));
        bad_sum[first - 1] ^= 1;
        assert!(!n_complete(&bad_sum), "…but a wrong checksum byte is not");
        assert!(!n_complete(&[0u8; 64]), "a header-only file (chronos review 1 P2)");
        assert!(g_sane(&fake_g(2, 99, 0)));
        assert!(!g_sane(&fake_g(3, 10, 0)), "a class the slot screen cannot name");
        assert!(!g_sane(&fake_g(0, 0, 0)), "level 0");
        assert!(!g_sane(&fake_g(0, 100, 0)), "level 100");
        for (what, n, g) in [
            ("a /n that is not one", vec![1u8; 100], fake_g(1, 10, 0)),
            ("a /g of class 7", fake_n(), fake_g(7, 10, 0)),
            ("a torn /g", fake_n(), fake_g(1, 10, 0)[..40].to_vec()),
        ] {
            let mut b = sample();
            b.files.push((Area::Fs, "n2".into(), n));
            b.files.push((Area::Fs, "g2".into(), g));
            assert_eq!(import_policy(&b), Err(DAMAGED.to_string()), "{what}");
        }
    }

    #[test]
    fn a_slot_travels_only_as_a_whole_pair_and_there_is_one() {
        let mut lone = sample();
        lone.files.push((Area::Fs, "n2".into(), fake_n()));
        assert!(import_policy(&lone).is_err(), "a lone half");
        let mut none = sample();
        none.files.retain(|(a, _, _)| *a != Area::Fs);
        assert_eq!(import_policy(&none), Err(NO_SLOT.to_string()));
    }

    #[test]
    fn records_and_host_files_must_read() {
        let mut b = sample();
        b.files.push((Area::Avatar, "1".into(), b"not a record".to_vec()));
        assert!(import_policy(&b).is_err());
        let mut b = sample();
        b.files.push((Area::Host, "wie_exp_rate.txt".into(), vec![0xff, 0xfe]));
        assert!(import_policy(&b).is_err(), "not text");
        let mut b = sample();
        b.files.retain(|(a, n, _)| !(*a == Area::Account && n == "bag"));
        b.files.push((Area::Account, "bag".into(), Vec::new()));
        assert!(import_policy(&b).is_err(), "an empty warehouse blob is no bag");
    }

    #[test]
    fn the_reasons_are_the_players_sentences() {
        assert_eq!(parse_import(b"PK not really"), Err(NOT_OURS.to_string()));
        let mut z = savebundle::encode(&sample());
        let at = z.windows(4).position(|w| w == b"own ").unwrap();
        z[at] ^= 1;
        assert_eq!(parse_import(&z), Err(DAMAGED.to_string()));
        let newer = savebundle::zip(&[(savebundle::MANIFEST, b"format hero2-save 2\naid 01035D0B\n")]);
        assert_eq!(parse_import(&newer), Err(OTHER_FORMAT.to_string()));
    }

    /// The files the 2026-10-02 builds wrote, with the `zip` crate (stored), still read — and their
    /// `fs/cf` is read and left alone.
    #[test]
    fn a_file_the_zip_crate_wrote_still_reads() {
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let (n, g) = (fake_n(), fake_g(1, 48, 2));
        for (name, data) in [
            (savebundle::MANIFEST, &b"format hero2-save 1\naid 01035D0B\ncreated 5\napp 67\n"[..]),
            ("fs/n1", &n[..]),
            ("fs/g1", &g[..]),
            ("fs/cf", &b"\x3f\x32\0\0\0\0\0\0\0\0"[..]),
        ] {
            w.start_file(name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        let bytes = w.finish().unwrap().into_inner();
        let b = parse_import(&bytes).unwrap();
        assert_eq!((b.slots(), b.app, b.backup), (vec![1], 67, false));
        assert!(b.get(Area::Fs, GAME_SETTINGS).is_some() && !savebundle::owned(Area::Fs, GAME_SETTINGS));
    }

    #[test]
    fn the_slot_lines_read_the_save() {
        assert_eq!(slot_line(1, Some(&fake_g(1, 48, 2))), "슬롯 1 · 나이트 Lv 48 · HELL");
        assert_eq!(slot_line(2, Some(&fake_g(2, 26, 0))), "슬롯 2 · 로그 Lv 26");
        assert_eq!(slot_line(4, Some(&[7u8; 64])), "슬롯 4", "not a whole save: the slot alone");
        assert_eq!(slot_box_line(1, Some(&fake_g(1, 48, 2))), "슬롯1 나이트 LV48 HELL");
        assert_eq!(slot_box_line(3, Some(&fake_g(0, 3, 1))), "슬롯3 버서커 LV3 HARD");
        assert!(crate::guest_build::h2_euc_kr(&slot_box_line(1, Some(&fake_g(1, 48, 3)))).is_some(), "the game's font has every character");
    }

    #[test]
    fn the_archive_aid_comes_from_the_descriptor() {
        let game = savebundle::zip(&[("01035D0B.jar", b"jar"), ("__adf__", b"PID:PD004517\nAID:01035D0B\nName:x\n")]);
        assert_eq!(archive_aid(&game).as_deref(), Some("01035D0B"));
        let crlf = savebundle::zip(&[("__adf__", b"PID:PD004517\r\nAID:01035D0B\r\n")]);
        assert_eq!(archive_aid(&crlf), None, "not the directory name the filesystem would use — refused, not guessed");
    }
}
