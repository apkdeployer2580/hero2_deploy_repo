// 세이브 내보내기 · 불러오기 · 되돌리기 (2026-10-28) — the transfer file, and the journaled apply
// that puts one on the phone at the next start.
//
// ── WHY THIS FILE, AND WHY IT IS SHAPED LIKE THIS ───────────────────────────
// User, 2026-10-28: *"hero5와 cronossword 처럼 인게임 내에 환경설정에서 맨밑에 행 3개 추가해서 세이브
// 내보내기, 세이브 불러오기, 세이브 되돌리기 … cronossword에서 작업했을때 결함이 몇가지 있어서
// 코드리뷰로 수정작업을 여러번했는데 이거 모두 파악해서 문제없도록"*. ChronosSword's
// `cs_core/src/savebundle.rs` went through four reviews (its CLAUDE.md 「코드리뷰에서 나온 것」);
// this is that engine carried over with every one of those fixes, and hero2's own 세이브 옮기기
// (2026-10-02, `save_transfer.rs` + `vault.rs`) re-based on it — that one had five of the same
// holes (a half-applied import started the game; the stage was consumed before anything was
// written, so a killed process had no journal; no fsync; the zip read entries while it walked
// the directory; the undo kept only the slot pairs, not the host files the import replaced).
//
// The rules, each one a review finding:
//  1. **A journal on the disk means the game does not start** until the disk is one whole save
//     again — forwards (the save the player confirmed) or backwards (`.before`), else
//     [`Outcome::Blocked`] and the next start tries again. Every write is a whole file, so
//     retrying is always safe. (chronos review 1, P1 「반쯤 적용된 세이브로 게임이 시작됐다」)
//  2. **A read that fails is not 「the file is absent」** — only NotFound is. Anything else stops
//     an export, and stops an apply before it writes. (review 1, P1)
//  3. **The directory is checked whole before any entry is copied** — count, names, sizes, the
//     entries laid end to end — so a crafted directory cannot turn a small file into a large
//     allocation. (review 2, P2)
//  4. **Names are made durable**: temp + fsync + rename + directory fsync; the journal's removal
//     is synced before the game may start, and every start syncs the directories first.
//     (review 3, P2)
//  5. **The stage is a promise to apply at the next start.** Wherever the game goes on without
//     keeping it — a stage that failed after its rename, a stage the start gives up on — it is
//     taken back DURABLY, and when that cannot be done the game does not go on (the stage then
//     IS what the player chose). (review 4)
//  6. **Integrity and policy are separate**: [`parse`] only checks the file is ours and whole —
//     a backup or a `.before` must go back to whatever was on the disk — and the import policy
//     (whole pairs, saves the game can read, at least one slot) is `save_transfer::parse_import`'s,
//     which the export runs too, so a phone never makes a file the other phone would refuse.
//     (review 1 P2, review 3 P1)
//
// Dependency-free (std only), edition 2021, no let-chains: `savebundle_selftest.rs` at the repo
// root `include!`s this file, so the rules under test are the shipping rules. No `log::` here —
// what happened comes back as lines for the caller to log.
//
// ── THE FILE ─────────────────────────────────────────────────────────────────
// A plain zip, stored (a PC opens it), written by [`zip`] and read by [`unzip`] — which also
// reads the files the 2026-10-02 builds wrote with the `zip` crate (stored, no descriptor).
// `hero2_save.txt`:
//   format hero2-save 1
//   aid 01035D0B        the archive the files belong to
//   created <unix ms>   when it was made
//   app <versionCode>   the app that made it (a newer app's file is refused — `save_transfer`)
//   kind backup         only in a backup / pre-image: the phone as it was, not a file to send
// and the members `fs/n1..g5`, `account/bag · materials`, `avatar/<slot>[.pending]`,
// `host/<HOST_FILES>`. ⛔ `fs/cf` (the game's settings, which are also its 「data is here」 flag)
// is still READ in an old file but never written by an apply: a wrong one can leave the game at
// its download screen on every start, where 세이브 되돌리기 is out of reach.
//
// ── THE DISK ─────────────────────────────────────────────────────────────────
//   <data>/save_import/pending.zip       the stage (the 2026-10-02 path, kept)
//   <data>/save_import/pending.applying  the journal: the stage, renamed = the commit
//   <data>/save_import/pending.before    the pre-image: the phone as it was
//   <data>/save_import/pending.backup    the name of the backup this apply made (settled with it)
//   <data>/save_backup/<14-digit ms>.zip the phone before each apply, newest [`BACKUP_KEEP`]
//   <data>/save_doc/{export,import}_<id>.zip  one file per file-screen request

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// The manifest member.
pub const MANIFEST: &str = "hero2_save.txt";
/// Its `format` field. Bump the number for a change an older build must refuse.
pub const FORMAT: &str = "hero2-save 1";

/// The save slots (`savevault::SLOT_MIN..=SLOT_MAX` — asserted in `vault.rs`).
pub const SLOT_MIN: u8 = 1;
pub const SLOT_MAX: u8 = 5;

/// Where a member lives on the phone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    /// `<data>/<aid>/fs/` — the guest's own files.
    Fs,
    /// `<data>/<aid>/account/` — the ACCOUNT warehouse.
    Account,
    /// `<data>/<aid>/avatar/` — the costume records.
    Avatar,
    /// `<data>/` — the host files that belong to the game state, not to the device.
    Host,
}

impl Area {
    pub const ALL: [Area; 4] = [Area::Fs, Area::Account, Area::Avatar, Area::Host];

    /// The member directory — the same name as the area's directory on the phone.
    pub fn dir(self) -> &'static str {
        match self {
            Area::Fs => "fs",
            Area::Account => "account",
            Area::Avatar => "avatar",
            Area::Host => "host",
        }
    }

    pub fn of_dir(dir: &str) -> Option<Area> {
        Area::ALL.into_iter().find(|a| a.dir() == dir)
    }
}

/// The game's settings file in `fs/` — read in a file, never applied (module header).
pub const GAME_SETTINGS: &str = "cf";
/// The ACCOUNT warehouse files (`vault::ACCOUNT_MAIN` / `ACCOUNT_MATERIALS`).
pub const ACCOUNT_FILES: [&str; 2] = ["bag", "materials"];
/// The host files in `<data>` that travel: the plates, and the play rules (the costume rewards
/// read the 난이도 a quest was completed at). The device's own — layout, controls, sound — stay.
pub const HOST_FILES: [&str; 6] =
    ["wie_quickpages.txt", "wie_difficulty.txt", "wie_exp_rate.txt", "wie_auto_loot.txt", "wie_drop_rate.txt", "wie_respawn_rate.txt"];

/// `n<slot>` / `g<slot>` — `savevault::guest_path`.
pub fn save_name(prog: bool, slot: u8) -> String {
    format!("{}{slot}", if prog { 'g' } else { 'n' })
}

/// Every name an area may hold — the whitelist.
pub fn names(area: Area) -> Vec<String> {
    match area {
        Area::Fs => {
            let mut v: Vec<String> = (SLOT_MIN..=SLOT_MAX).flat_map(|s| [save_name(false, s), save_name(true, s)]).collect();
            v.push(GAME_SETTINGS.to_string());
            v
        }
        Area::Account => ACCOUNT_FILES.iter().map(|s| s.to_string()).collect(),
        Area::Avatar => (SLOT_MIN..=SLOT_MAX).flat_map(|s| [s.to_string(), format!("{s}.pending")]).collect(),
        Area::Host => HOST_FILES.iter().map(|s| s.to_string()).collect(),
    }
}

/// Does an apply own this path — write it when the bundle has it, remove it when it does not?
/// Everything on the list except `fs/cf`.
pub fn owned(area: Area, name: &str) -> bool {
    !(area == Area::Fs && name == GAME_SETTINGS)
}

/// Largest member: far above any (a full 127-cell bag is ~4 KB) — a sanity limit.
pub const MEMBER_MAX: usize = 256 * 1024;
/// Largest file, and the most all members together may hold. A real one is ~10–30 KB.
pub const FILE_MAX: usize = 4 * 1024 * 1024;

/// The most entries a file has: the manifest and every name.
pub fn entries_max() -> usize {
    1 + Area::ALL.iter().map(|a| names(*a).len()).sum::<usize>()
}

/// Is this an archive id the phone may name a directory with (`01035D0B`)?
pub fn aid_ok(aid: &str) -> bool {
    !aid.is_empty() && aid.len() <= 32 && aid.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// One transfer file, read or about to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    pub aid: String,
    pub created_ms: u64,
    /// The versionCode of the app that made it (0 = unknown).
    pub app: u32,
    /// A backup / pre-image (`kind backup`): the phone as it was — nothing more is asked of it.
    pub backup: bool,
    pub files: Vec<(Area, String, Vec<u8>)>,
}

impl Bundle {
    pub fn get(&self, area: Area, name: &str) -> Option<&[u8]> {
        self.files.iter().find(|(a, n, _)| *a == area && n == name).map(|(_, _, b)| b.as_slice())
    }

    /// The slots whose BOTH halves it carries.
    pub fn slots(&self) -> Vec<u8> {
        (SLOT_MIN..=SLOT_MAX).filter(|&s| self.get(Area::Fs, &save_name(false, s)).is_some() && self.get(Area::Fs, &save_name(true, s)).is_some()).collect()
    }

    /// The files an apply writes, in the order it writes them.
    fn owned_files(&self) -> Vec<(Area, String, Vec<u8>)> {
        let mut out = Vec::new();
        for area in Area::ALL {
            for name in names(area) {
                if owned(area, &name) {
                    if let Some(b) = self.get(area, &name) {
                        out.push((area, name, b.to_vec()));
                    }
                }
            }
        }
        out
    }
}

// ── zip ──────────────────────────────────────────────────────────────────────

/// CRC-32 (IEEE), bitwise — a few KB per file.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// 1980-01-01 00:00 — the file's own time is in the manifest.
const DOS_DATE: u16 = (1 << 5) | 1;

/// A stored zip of `entries`, in order — the layout [`unzip`] demands.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, 20);
        put16(&mut out, 0x0800); // UTF-8 names
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, DOS_DATE);
        put32(&mut out, crc);
        put32(&mut out, data.len() as u32);
        put32(&mut out, data.len() as u32);
        put16(&mut out, name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        put32(&mut central, 0x0201_4b50);
        put16(&mut central, 20);
        put16(&mut central, 20);
        put16(&mut central, 0x0800);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, DOS_DATE);
        put32(&mut central, crc);
        put32(&mut central, data.len() as u32);
        put32(&mut central, data.len() as u32);
        put16(&mut central, name.len() as u16);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_off = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, entries.len() as u16);
    put16(&mut out, entries.len() as u16);
    put32(&mut out, cd_len);
    put32(&mut out, cd_off);
    put16(&mut out, 0);
    out
}

fn get16(b: &[u8], i: usize) -> Option<usize> {
    b.get(i..i.checked_add(2)?).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize)
}

fn get32(b: &[u8], i: usize) -> Option<usize> {
    b.get(i..i.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
}

/// Read a stored zip. Anything else is an error.
///
/// ★ The directory is checked WHOLE before any entry is checksummed or copied (chronos review 2,
/// P2): a directory can name one stretch of data any number of times, and a reader that copies as
/// it goes turns a small file into a large allocation. So first — at most `entries_max` entries
/// (from the end record's own count), no name twice, none over `member_max`, all of them together
/// under `total_max`, every entry stored plainly (no encryption, no trailing descriptor, method 0,
/// sizes equal), and the entries laid out one after another, none inside another's data. Only then
/// the CRCs and the copies, which are therefore bounded by the input.
pub fn unzip(b: &[u8], entries_max: usize, member_max: usize, total_max: usize) -> Result<Vec<(String, Vec<u8>)>, String> {
    let bad = |why: &str| Err(why.to_string());
    if b.len() < 22 {
        return bad("shorter than an end record");
    }
    let lo = b.len().saturating_sub(22 + 0xffff);
    let eocd = (lo..=b.len() - 22)
        .rev()
        .find(|&i| get32(b, i) == Some(0x0605_4b50) && get16(b, i + 20).map(|c| i + 22 + c) == Some(b.len()))
        .ok_or_else(|| "no end record".to_string())?;
    if get16(b, eocd + 4) != Some(0) || get16(b, eocd + 6) != Some(0) {
        return bad("a multi-disk archive");
    }
    let count = get16(b, eocd + 10).unwrap_or(usize::MAX);
    if get16(b, eocd + 8) != Some(count) {
        return bad("the end record's two counts differ");
    }
    if count > entries_max {
        return Err(format!("{count} entries; a file has at most {entries_max}"));
    }
    let cd_len = get32(b, eocd + 12).unwrap_or(usize::MAX);
    let cd_off = get32(b, eocd + 16).unwrap_or(usize::MAX);
    if cd_off.checked_add(cd_len).map_or(true, |end| end > eocd) {
        return bad("central directory out of bounds");
    }

    struct Entry {
        name: String,
        crc: u32,
        local: usize,
        start: usize,
        end: usize,
    }
    let mut entries: Vec<Entry> = Vec::with_capacity(count);
    let mut total = 0usize;
    let mut p = cd_off;
    for _ in 0..count {
        if get32(b, p) != Some(0x0201_4b50) {
            return bad("broken central directory");
        }
        let field = |o: usize| get16(b, p + o).unwrap_or(usize::MAX);
        let (flags, method) = (field(8), field(10));
        let crc = get32(b, p + 16).unwrap_or(0) as u32;
        let (csize, size) = (get32(b, p + 20).unwrap_or(usize::MAX), get32(b, p + 24).unwrap_or(usize::MAX));
        let (n, extra, comment) = (field(28), field(30), field(32));
        let local = get32(b, p + 42).unwrap_or(usize::MAX);
        // Bit 0 is encryption, bit 3 a trailing data descriptor; neither is ours.
        if flags & 0x0009 != 0 || method != 0 || csize != size {
            return bad("an entry that is not stored plainly");
        }
        if n == usize::MAX || extra == usize::MAX || comment == usize::MAX {
            return bad("central directory overruns");
        }
        let name = b.get(p + 46..p + 46 + n).ok_or_else(|| "name out of bounds".to_string())?;
        let name = std::str::from_utf8(name).map_err(|_| "a name that is not UTF-8".to_string())?.to_string();
        p += 46 + n + extra + comment;
        if p > cd_off + cd_len {
            return bad("central directory overruns");
        }
        if entries.iter().any(|e| e.name == name) {
            return Err(format!("{name} twice"));
        }
        if size > member_max {
            return Err(format!("{name} is {size} bytes"));
        }
        total += size;
        if total > total_max {
            return Err(format!("{total} bytes of entries, over the {total_max} B ceiling"));
        }
        if get32(b, local) != Some(0x0403_4b50) {
            return bad("broken local header");
        }
        // Encryption or a trailing descriptor in the LOCAL header too.
        if get16(b, local + 6).map_or(true, |f| f & 0x0009 != 0) || get16(b, local + 8) != Some(0) {
            return bad("a local header that is not stored plainly");
        }
        let (ln, lx) = (get16(b, local + 26).unwrap_or(usize::MAX), get16(b, local + 28).unwrap_or(usize::MAX));
        if ln == usize::MAX || lx == usize::MAX || b.get(local + 30..local + 30 + ln) != Some(name.as_bytes()) {
            return bad("local and central names differ");
        }
        let start = local + 30 + ln + lx;
        let end = start.checked_add(size).filter(|&end| end <= cd_off).ok_or_else(|| "entry data out of bounds".to_string())?;
        entries.push(Entry { name, crc, local, start, end });
    }
    if p != cd_off + cd_len {
        return bad("the central directory's length is not its entries'");
    }
    // One after another, in whatever order the directory lists them. An entry whose header sits
    // inside another's data is the same bytes counted twice.
    let mut order: Vec<&Entry> = entries.iter().collect();
    order.sort_by_key(|e| e.local);
    if order.windows(2).any(|w| w[1].local < w[0].end) {
        return bad("entries that overlap");
    }

    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let data = &b[e.start..e.end];
        if crc32(data) != e.crc {
            return Err(format!("{}: CRC mismatch", e.name));
        }
        out.push((e.name, data.to_vec()));
    }
    Ok(out)
}

// ── the bundle ───────────────────────────────────────────────────────────────

/// The file of a bundle. Infallible: the names come from the whitelist by construction (a file
/// with a name that is not on it is a programming error, and is left out).
pub fn encode(b: &Bundle) -> Vec<u8> {
    let mut manifest = format!("format {FORMAT}\naid {}\ncreated {}\napp {}\n", b.aid, b.created_ms, b.app);
    if b.backup {
        manifest.push_str("kind backup\n");
    }
    let members: Vec<(String, &[u8])> = b
        .files
        .iter()
        .filter(|(a, n, _)| names(*a).iter().any(|x| x == n))
        .map(|(a, n, d)| (format!("{}/{n}", a.dir()), d.as_slice()))
        .collect();
    let mut entries: Vec<(&str, &[u8])> = vec![(MANIFEST, manifest.as_bytes())];
    entries.extend(members.iter().map(|(n, d)| (n.as_str(), *d)));
    zip(&entries)
}

/// Why a file is not one this can use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    /// Not a hero2 save file at all.
    NotOurs(String),
    /// A hero2 save file whose bytes were changed in transit.
    Damaged(String),
    /// A hero2 save file of a format this build does not read.
    Format(String),
}

/// Read a file. INTEGRITY ONLY — the zip, the manifest, the names: a backup or a pre-image is
/// whatever was on the disk, and putting it back must not depend on it passing anything else.
/// What an import may carry is `save_transfer::parse_import`'s question.
pub fn parse(bytes: &[u8]) -> Result<Bundle, Reject> {
    let no = |why: String| Reject::NotOurs(why);
    if bytes.len() > FILE_MAX {
        return Err(no(format!("{} bytes, over the {FILE_MAX} B ceiling", bytes.len())));
    }
    let entries = unzip(bytes, entries_max(), MEMBER_MAX, FILE_MAX).map_err(|why| if why.ends_with("CRC mismatch") { Reject::Damaged(why) } else { no(why) })?;
    let manifest = entries.iter().find(|(n, _)| n == MANIFEST).ok_or_else(|| no("no manifest".to_string()))?;
    let manifest = String::from_utf8_lossy(&manifest.1).into_owned();
    let field = |key: &str| manifest.lines().find_map(|l| l.trim().strip_prefix(key).and_then(|v| v.strip_prefix(' ')).map(str::trim).map(str::to_string));
    if field("format").as_deref() != Some(FORMAT) {
        return Err(Reject::Format(format!("format {:?}", field("format"))));
    }
    let aid = field("aid").filter(|a| aid_ok(a)).ok_or_else(|| no(format!("aid {:?}", field("aid"))))?;
    let created_ms = field("created").and_then(|v| v.parse().ok()).unwrap_or(0);
    let app = field("app").and_then(|v| v.parse().ok()).unwrap_or(0);
    let backup = field("kind").as_deref() == Some("backup");
    let mut files = Vec::new();
    for (member, data) in entries {
        if member == MANIFEST {
            continue;
        }
        let place = member.split_once('/').and_then(|(dir, name)| Area::of_dir(dir).map(|a| (a, name.to_string())));
        let Some((area, name)) = place.filter(|(area, name)| names(*area).iter().any(|n| n == name)) else {
            return Err(no(format!("{member:?} is not a transfer file")));
        };
        files.push((area, name, data));
    }
    Ok(Bundle { aid, created_ms, app, backup, files })
}

// ── the disk ─────────────────────────────────────────────────────────────────

/// Where the areas live for one archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub data_dir: PathBuf,
    /// `<data>/<aid>`.
    pub root: PathBuf,
}

impl Layout {
    pub fn new(data_dir: &Path, aid: &str) -> Layout {
        Layout { data_dir: data_dir.to_path_buf(), root: data_dir.join(aid) }
    }

    pub fn dir(&self, area: Area) -> PathBuf {
        match area {
            Area::Host => self.data_dir.clone(),
            a => self.root.join(a.dir()),
        }
    }

    pub fn path(&self, area: Area, name: &str) -> PathBuf {
        self.dir(area).join(name)
    }
}

/// Read a file, telling 「not there」 (`Ok(None)`) from 「cannot be read」 (rule 2).
pub fn read_if_there(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn exists(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Remove a file; one that is already gone is removed.
fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Make the names in a directory durable — a rename into it, a removal from it. A file's own
/// `sync_all` does not: after a power cut a removed name can come back, and the journal is a name
/// (rule 4). A failure is an error; a filesystem that cannot sync a directory at all says EINVAL,
/// and that one is let through, as is a directory that is not there (nothing in it to keep).
/// Unix only — Windows (the probe, the selftest) has no handle to sync a directory with.
pub fn sync_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
        let err = |e: std::io::Error| format!("{} (syncing the directory): {e}", dir.display());
        let d = match std::fs::File::open(dir) {
            Ok(d) => d,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(err(e)),
        };
        match d.sync_all() {
            Err(e) if e.kind() != ErrorKind::InvalidInput => return Err(err(e)),
            _ => {}
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// `create_dir_all`, durably: every directory it MADE has its name synced into its parent. A new
/// directory's name lives in its parent, and syncing the new directory (what [`write_atomically`]
/// does after its rename) keeps the file in it, not the directory itself — code review 2026-10-28
/// P2: a first 불러오기 makes `account/` and `avatar/` (on a new phone `<aid>/` and `fs/` too), and
/// the journal that guards the apply is removed after it, so a power cut could take them back with
/// nothing left to finish the apply. Returns the parents it synced, outermost first.
///
/// ⛔ It knows only the directories IT made: when a parent's sync fails, a retry finds the directory
/// already there and syncs nothing. So the points that must be durable — the end of an apply
/// ([`apply_sync_dirs`]) and a stage — sync the whole chain themselves, every time.
pub fn create_dir_durably(dir: &Path) -> Result<Vec<PathBuf>, String> {
    // The directories that are not there yet, deepest first. An error asking is treated as 「there」:
    // `create_dir_all` then says what is wrong.
    let mut made = Vec::new();
    let mut d = dir;
    while !d.as_os_str().is_empty() && !d.try_exists().unwrap_or(true) {
        made.push(d.to_path_buf());
        match d.parent() {
            Some(p) => d = p,
            None => break,
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut synced = Vec::new();
    for m in made.iter().rev() {
        let parent = m.parent().unwrap_or(Path::new(""));
        sync_dir(parent)?;
        synced.push(parent.to_path_buf());
    }
    Ok(synced)
}

/// Temp + fsync + rename + directory sync: an interrupted write leaves the previous file, and a
/// finished one survives a power cut — with the directories it had to make ([`create_dir_durably`]).
/// The temp name starts with a dot so nothing that lists a directory mistakes it for the file
/// (`savevault::classify`).
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().unwrap_or(Path::new(""));
    create_dir_durably(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.xfer"));
    let err = |e: std::io::Error| format!("{}: {e}", tmp.display());
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(err(e));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", path.display()));
    }
    sync_dir(dir)
}

/// Remove a file and make the removal durable (rule 5).
pub fn remove_durably(path: &Path) -> Result<(), String> {
    remove(path)?;
    sync_dir(path.parent().unwrap_or(Path::new("")))
}

/// Every owned member on the disk now. A member that is not there is left out; one that is there
/// and cannot be read is an ERROR, never a gap (rule 2) — a gap would read as 「this phone has no
/// warehouse」, and both an export and a pre-image would then carry that. A member too big to ever
/// be read back ([`MEMBER_MAX`]) is an error too: a backup that [`parse`] refuses is no backup.
pub fn present(layout: &Layout) -> Result<Vec<(Area, String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    for area in Area::ALL {
        for name in names(area) {
            if !owned(area, &name) {
                continue;
            }
            let path = layout.path(area, &name);
            if let Some(b) = read_if_there(&path)? {
                if b.len() > MEMBER_MAX {
                    return Err(format!("{}: {} bytes, over the {MEMBER_MAX} B a file can carry", path.display(), b.len()));
                }
                out.push((area, name, b));
            }
        }
    }
    Ok(out)
}

/// Make the owned paths exactly `files`: each written, every other one removed. Stops at the first
/// failure. Ends with every directory it touched synced, so the removals are as durable as the
/// writes before the journal that guards them goes.
fn apply(layout: &Layout, files: &[(Area, String, Vec<u8>)]) -> Result<(usize, usize), String> {
    let (mut written, mut removed) = (0, 0);
    for area in Area::ALL {
        for name in names(area) {
            if !owned(area, &name) {
                continue;
            }
            let path = layout.path(area, &name);
            match files.iter().find(|(a, n, _)| *a == area && *n == name) {
                Some((_, _, data)) => {
                    write_atomically(&path, data)?;
                    written += 1;
                }
                None => {
                    if exists(&path)? {
                        remove(&path)?;
                        removed += 1;
                    }
                }
            }
        }
    }
    for dir in apply_sync_dirs(layout) {
        sync_dir(&dir)?;
    }
    Ok((written, removed))
}

/// The directories an apply makes durable at its end: every one between an owned file and the data
/// dir — the areas', the archive root, the data dir. ALL of them, every time, never only the ones
/// this attempt made: a directory an earlier attempt made, whose parent's sync then failed, is
/// 「already there」 to the retry ([`create_dir_durably`] knows only what IT made), and the journal
/// goes right after the apply (code review 2026-10-28, second pass P2).
fn apply_sync_dirs(layout: &Layout) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for d in Area::ALL.into_iter().map(|a| layout.dir(a)).chain([layout.root.clone(), layout.data_dir.clone()]) {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs
}

/// The stage, the journal and the pre-image live here — under the data dir itself, not an archive
/// root: a phone that has never saved has none, and that is the phone a transfer is usually for.
pub const IMPORT_DIR: &str = "save_import";
pub const PENDING: &str = "pending.zip";
pub const APPLYING: &str = "pending.applying";
pub const BEFORE: &str = "pending.before";
/// The journal's third member: the NAME of the backup this apply made (`save_backup/<ms>.zip`), so
/// whichever start settles the apply knows which backup is its own (code review 2026-10-28, second
/// pass: a start that finished an earlier apply by putting the phone back left that apply's backup
/// — a copy of the phone as it now is — as the newest, and 되돌리기 kept 「going back」 to it).
pub const OWN_BACKUP: &str = "pending.backup";
/// The phone before each apply.
pub const BACKUP_DIR: &str = "save_backup";
pub const BACKUP_KEEP: usize = 5;
/// One file per file-screen request.
pub const DOC_DIR: &str = "save_doc";

pub fn pending_path(data_dir: &Path) -> PathBuf {
    data_dir.join(IMPORT_DIR).join(PENDING)
}

fn applying_path(data_dir: &Path) -> PathBuf {
    data_dir.join(IMPORT_DIR).join(APPLYING)
}

fn own_backup_marker(data_dir: &Path) -> PathBuf {
    data_dir.join(IMPORT_DIR).join(OWN_BACKUP)
}

/// The backup the marker names — only a name [`parse_backup_name`] accepts, in [`BACKUP_DIR`], so
/// a damaged marker can never point a removal anywhere else.
fn own_backup(data_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(own_backup_marker(data_dir)).ok()?;
    let name = text.trim();
    parse_backup_name(name)?;
    Some(data_dir.join(BACKUP_DIR).join(name))
}

/// Settle the backup an apply made, once the apply's outcome is known — on EVERY path that
/// decides one, the start that made it or a later one that finished its journal:
///  * `NotApplied` — the disk is confirmed the phone from before, so the backup is a copy of the
///    phone as it is NOW: it goes (else 되돌리기 would 「go back」 to where it already is, and the
///    backup before it — the one that matters — would never be the newest);
///  * `Applied` — the backup is the phone it replaced: it stays;
///  * `Blocked` — undecided: the backup and its marker stay for the start that decides (code review
///    2026-10-28 P1: removing it here lost the only copy once a later start finished the apply).
/// The marker goes once the outcome is decided.
fn settle_own_backup(data_dir: &Path, outcome: Outcome, log: &mut Vec<String>) -> Outcome {
    match outcome {
        Outcome::NotApplied => {
            if let Some(p) = own_backup(data_dir) {
                match remove(&p) {
                    Ok(()) => log.push(format!("save bundle: nothing changed, so the backup {} goes again", p.display())),
                    Err(e) => log.push(format!("save bundle: cannot remove the backup {} ({e})", p.display())),
                }
            }
            let _ = remove(&own_backup_marker(data_dir));
        }
        Outcome::Applied => {
            let _ = remove(&own_backup_marker(data_dir));
        }
        Outcome::Blocked | Outcome::Idle => {}
    }
    outcome
}

fn before_path(data_dir: &Path) -> PathBuf {
    data_dir.join(IMPORT_DIR).join(BEFORE)
}

/// The file a file-screen request writes (an export) or reads (an import) — its own, per request
/// number (chronos review 2: a copy can outlive the screen; one shared file was overwritten by the
/// next request while the last one's copy was still going).
pub fn doc_file(data_dir: &Path, export: bool, id: u32) -> PathBuf {
    data_dir.join(DOC_DIR).join(format!("{}_{id}.zip", if export { "export" } else { "import" }))
}

/// Nothing of the last run's file screens is alive at a start: their copies died with the process.
pub fn sweep_docs(data_dir: &Path) -> Vec<String> {
    let mut log = Vec::new();
    for e in std::fs::read_dir(data_dir.join(DOC_DIR)).into_iter().flatten().flatten() {
        if std::fs::remove_file(e.path()).is_ok() {
            log.push(format!("save bundle: removed a stale {}", e.file_name().to_string_lossy()));
        }
    }
    log
}

/// Why [`stage`] did not stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageFailed {
    /// Nothing is staged, on the disk: the game may go on.
    Cancelled(String),
    /// A stage may be on the disk and could not be surely taken back. The next start would apply
    /// it over anything played from here, so play stops — which is what the player asked for when
    /// they said 예.
    Unsure(String),
}

/// Accept a file for the next start. A write that fails may still have renamed the stage into
/// place — the directory sync comes after the rename — so a failure takes it back, durably,
/// before it lets the game go on (rule 5, chronos review 4).
pub fn stage(data_dir: &Path, bytes: &[u8]) -> Result<(), StageFailed> {
    let path = pending_path(data_dir);
    // …and the data dir, every time: the stage's directory may be one an earlier, failed stage made
    // and could not sync into it (code review 2026-10-28, second pass P2).
    let Err(why) = write_atomically(&path, bytes).and_then(|()| sync_dir(data_dir)) else {
        return Ok(());
    };
    match remove_durably(&path) {
        Ok(()) => Err(StageFailed::Cancelled(why)),
        Err(e) => Err(StageFailed::Unsure(format!("{why}; and taking the stage back: {e}"))),
    }
}

/// Take a stage back (a later request replaces it) — durably. `Err` = it may still be there.
pub fn unstage(data_dir: &Path) -> Result<bool, String> {
    let path = pending_path(data_dir);
    let was = exists(&path)?;
    remove_durably(&path)?;
    Ok(was)
}

// ── the backups ──────────────────────────────────────────────────────────────

/// A backup's time, if `name` is one. Everything else in the directory is ignored, never deleted.
pub fn parse_backup_name(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(".zip")?;
    if stem.len() == 14 && stem.bytes().all(|c| c.is_ascii_digit()) {
        stem.parse().ok()
    } else {
        None
    }
}

/// The backups, oldest first.
pub fn backups(data_dir: &Path) -> Vec<(u64, PathBuf)> {
    let mut v: Vec<(u64, PathBuf)> = std::fs::read_dir(data_dir.join(BACKUP_DIR))
        .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| parse_backup_name(&e.file_name().to_string_lossy()).map(|ms| (ms, e.path()))).collect())
        .unwrap_or_default();
    v.sort();
    v
}

pub fn newest_backup(data_dir: &Path) -> Option<(u64, PathBuf)> {
    backups(data_dir).pop()
}

/// Keep `current` — the phone now, already read — as a backup. `Ok(None)` when there is nothing to
/// keep, or when it already is the newest backup.
fn backup_keep(data_dir: &Path, current: &Bundle, now_ms: u64) -> Result<Option<PathBuf>, String> {
    if current.files.is_empty() {
        return Ok(None);
    }
    let newest = newest_backup(data_dir);
    if let Some((_, path)) = &newest {
        let same = std::fs::read(path).ok().and_then(|b| parse(&b).ok()).is_some_and(|b| b.files == current.files);
        if same {
            return Ok(None);
        }
    }
    // Monotonic even if the clock went back, so the newest by name is the newest by fact.
    let ms = newest.map_or(now_ms, |(t, _)| now_ms.max(t + 1));
    let path = data_dir.join(BACKUP_DIR).join(format!("{ms:014}.zip"));
    write_atomically(&path, &encode(current))?;
    Ok(Some(path))
}

/// Remove all but the newest [`BACKUP_KEEP`].
fn backup_rotate(data_dir: &Path) -> Vec<String> {
    let list = backups(data_dir);
    let extra = list.len().saturating_sub(BACKUP_KEEP);
    list.into_iter()
        .take(extra)
        .map(|(_, p)| match std::fs::remove_file(&p) {
            Ok(()) => format!("save bundle: backup rotated out {}", p.display()),
            Err(e) => format!("save bundle: cannot rotate out {}: {e}", p.display()),
        })
        .collect()
}

// ── the start ────────────────────────────────────────────────────────────────

/// What a start did about a staged file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing was staged and nothing was left over.
    Idle,
    /// The staged file is what is on the disk now.
    Applied,
    /// The staged file was not applied, and what is on the disk is the phone from before — never
    /// touched, or written back. The player is told.
    NotApplied,
    /// The files on the disk may be a mix of two, and neither could be finished — or a stage could
    /// not be surely taken back. **The game must not start**: the journal (or the stage) is kept,
    /// and the next start tries again.
    Blocked,
}

/// [`apply_pending`]'s answer.
#[derive(Debug)]
pub struct Report {
    pub outcome: Outcome,
    /// One line per step, for the log.
    pub log: Vec<String>,
    /// What was applied (Applied) or offered (NotApplied), when it could be read.
    pub target: Option<Bundle>,
}

impl Report {
    /// May the game start on what is on the disk? (The app asks `outcome` directly; the selftest asks this.)
    #[allow(dead_code)]
    pub fn may_start(&self) -> bool {
        self.outcome != Outcome::Blocked
    }
}

fn read_bundle(path: &Path) -> Result<Option<Bundle>, String> {
    match read_if_there(path)? {
        None => Ok(None),
        Some(b) => parse(&b).map(Some).map_err(|e| format!("{}: {e:?}", path.display())),
    }
}

/// Take the journal down after the disk became one whole phone. `APPLYING` first: once it is gone
/// nothing will be applied again. If it cannot surely go, the start is blocked — a journal beside a
/// running game is applied again at the next start, over everything played in between.
fn close_journal(data_dir: &Path, done: Outcome, log: &mut Vec<String>) -> Outcome {
    if let Err(e) = remove_durably(&applying_path(data_dir)) {
        log.push(format!("save bundle: cannot clear the journal ({e}); not starting the game on it"));
        return Outcome::Blocked;
    }
    if let Err(e) = remove(&before_path(data_dir)) {
        // Harmless with `APPLYING` gone: the next start sweeps it.
        log.push(format!("save bundle: cannot remove the pre-image ({e}); the next start sweeps it"));
    }
    if done == Outcome::Applied {
        log.extend(backup_rotate(data_dir));
    }
    done
}

/// With the journal committed: make the disk one whole phone — `target` if it can be finished,
/// otherwise `before` — and close the journal; or report the start blocked with the journal kept.
fn settle(data_dir: &Path, layout: &Layout, target: Option<&Bundle>, before: Option<&Bundle>, log: &mut Vec<String>) -> Outcome {
    if let Some(t) = target {
        match apply(layout, &t.owned_files()) {
            Ok((w, d)) => {
                log.push(format!(
                    "save bundle: applied {} — slots {:?}, {w} file(s) written, {d} removed",
                    if t.backup { "a backup".to_string() } else { format!("the file made at {}", t.created_ms) },
                    t.slots()
                ));
                return close_journal(data_dir, Outcome::Applied, log);
            }
            Err(e) => log.push(format!("save bundle: the apply stopped part-way ({e}); putting the phone from before back")),
        }
    }
    let Some(b) = before else {
        log.push("save bundle: there is no pre-image to put back; not starting the game on a mixed save".to_string());
        return Outcome::Blocked;
    };
    match apply(layout, &b.owned_files()) {
        Ok(_) => {
            log.push("save bundle: the phone from before is back; nothing changed".to_string());
            close_journal(data_dir, Outcome::NotApplied, log)
        }
        Err(e) => {
            log.push(format!(
                "save bundle: the phone from before could not be put back either ({e}); not starting the game on a mixed save — the next start tries again"
            ));
            Outcome::Blocked
        }
    }
}

/// Apply a staged file, or finish one an earlier start left unfinished. Called before the emulator
/// exists and before anything reads a file the player owns (`android_main`, the probe). `archive_aid`
/// = the archive this app runs; `app` stamps the pre-image and the backup.
pub fn apply_pending(data_dir: &Path, archive_aid: Option<&str>, app: u32, now_ms: u64) -> Report {
    let mut log = sweep_docs(data_dir);
    let done = |outcome, log, target| Report { outcome, log, target };
    let import_dir = data_dir.join(IMPORT_DIR);
    // Whatever names the last run changed are made durable before anything here reads them: a run
    // that removed a stage or a journal and could not sync the removal blocked its start, and
    // without this the next start in the same boot would see the removal in the page cache, start
    // the game, and a power cut later could bring the file back (chronos review 4).
    for dir in [import_dir.as_path(), data_dir] {
        if let Err(e) = sync_dir(dir) {
            log.push(format!("save bundle: cannot make the save directories durable ({e}); not starting the game"));
            return done(Outcome::Blocked, log, None);
        }
    }
    let (pending, applying, before) = (pending_path(data_dir), applying_path(data_dir), before_path(data_dir));

    // An apply that did not finish: interrupted, or failed and not put back.
    match exists(&applying) {
        Err(e) => {
            log.push(format!("save bundle: cannot tell whether an apply is unfinished ({e}); not starting the game"));
            return done(Outcome::Blocked, log, None);
        }
        Ok(true) => {
            log.push("save bundle: an earlier apply did not finish; finishing it".to_string());
            let Some(aid) = archive_aid else {
                log.push("save bundle: the archive's AID is unknown, so there is nowhere to finish it; not starting the game".to_string());
                return done(Outcome::Blocked, log, None);
            };
            let layout = Layout::new(data_dir, aid);
            let target = match read_bundle(&applying) {
                Ok(Some(t)) if t.aid == aid => Some(t),
                Ok(Some(t)) => {
                    log.push(format!("save bundle: the journal is archive {}, this app runs {aid}", t.aid));
                    None
                }
                Ok(None) => None,
                Err(e) => {
                    log.push(format!("save bundle: the journal cannot be read ({e})"));
                    None
                }
            };
            let back = read_bundle(&before).unwrap_or_else(|e| {
                log.push(format!("save bundle: the pre-image cannot be read ({e})"));
                None
            });
            let outcome = settle(data_dir, &layout, target.as_ref(), back.as_ref(), &mut log);
            let outcome = settle_own_backup(data_dir, outcome, &mut log);
            return done(outcome, log, target);
        }
        Ok(false) => {}
    }

    let bytes = match read_if_there(&pending) {
        Ok(Some(b)) => Ok(b),
        Ok(None) => {
            // A pre-image with no journal is what an apply leaves when it stops between its two
            // removals, or before its commit: nothing refers to it.
            if exists(&before).unwrap_or(false) && remove(&before).is_ok() {
                log.push("save bundle: removed a pre-image no journal refers to".to_string());
            }
            // …and a backup marker: with no journal and no stage, no apply is undecided — the
            // backup it names is an ordinary backup now.
            if exists(&own_backup_marker(data_dir)).unwrap_or(false) && remove(&own_backup_marker(data_dir)).is_ok() {
                log.push("save bundle: removed a backup marker no apply refers to".to_string());
            }
            return done(Outcome::Idle, log, None);
        }
        Err(e) => Err(format!("save bundle: the staged file cannot be read ({e}); nothing changed")),
    };
    // Giving up starts the game on the phone from before, so the stage must be gone from the disk
    // first — a read that failed once may work at the next start, and would then apply the stage
    // over what is played in between. If it cannot surely go, the start is blocked.
    let abandon = |why: String, mut log: Vec<String>, target: Option<Bundle>| {
        log.push(why);
        let _ = remove(&before);
        let outcome = match remove_durably(&pending) {
            Ok(()) => Outcome::NotApplied,
            Err(e) => {
                log.push(format!("save bundle: cannot take the staged file back ({e}); not starting the game, since the next start would apply it over what is played"));
                Outcome::Blocked
            }
        };
        // An earlier start's attempt at this same stage may have made a backup: settled with it.
        let outcome = settle_own_backup(data_dir, outcome, &mut log);
        done(outcome, log, target)
    };
    let bytes = match bytes {
        Ok(b) => b,
        Err(why) => return abandon(why, log, None),
    };
    let target = match parse(&bytes) {
        Ok(b) => b,
        Err(e) => return abandon(format!("save bundle: the staged file is unusable ({e:?}); nothing changed"), log, None),
    };
    let Some(aid) = archive_aid else {
        return abandon("save bundle: the archive's AID is unknown; nothing changed".to_string(), log, Some(target));
    };
    if target.aid != aid {
        let why = format!("save bundle: the staged file is archive {}, this app runs {aid}; nothing changed", target.aid);
        return abandon(why, log, Some(target));
    }
    let layout = Layout::new(data_dir, aid);
    // No destructive write without its pre-image — and a file that cannot be read has none.
    let current = match present(&layout) {
        Ok(files) => Bundle { aid: aid.to_string(), created_ms: now_ms, app, backup: true, files },
        Err(e) => return abandon(format!("save bundle: cannot read the phone it would replace ({e}); nothing changed"), log, Some(target)),
    };
    match backup_keep(data_dir, &current, now_ms) {
        Ok(Some(p)) => {
            log.push(format!("save bundle: the phone it replaces is in {}", p.display()));
            // Named in the journal before anything else is written, so whichever start settles this
            // apply settles its backup too (`settle_own_backup`). A backup that cannot be named is
            // taken back at once: unnamed, a rolled-back apply would leave it as a copy of 「now」.
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Err(e) = write_atomically(&own_backup_marker(data_dir), name.as_bytes()) {
                let _ = remove(&p);
                return abandon(format!("save bundle: cannot name the backup it made ({e}); nothing changed"), log, Some(target));
            }
        }
        Ok(None) => log.push("save bundle: the phone it replaces is empty, or already the newest backup".to_string()),
        Err(e) => return abandon(format!("save bundle: cannot keep the phone it replaces ({e}); nothing changed"), log, Some(target)),
    }
    if let Err(e) = write_atomically(&before, &encode(&current)) {
        return abandon(format!("save bundle: cannot write the pre-image ({e}); nothing changed"), log, Some(target));
    }
    // The commit. ⛔ Not removed first (the 2026-10-02 importer consumed its stage before it wrote
    // a thing, so a process killed half way left a mix and nothing to finish it with).
    if let Err(e) = std::fs::rename(&pending, &applying).map_err(|e| e.to_string()).and_then(|()| sync_dir(&import_dir)) {
        // The rename may have happened and only its sync failed: the journal is then what decides.
        if exists(&applying).unwrap_or(true) {
            log.push(format!("save bundle: the commit's sync failed ({e}); finishing the apply"));
            let outcome = settle(data_dir, &layout, Some(&target), Some(&current), &mut log);
            let outcome = settle_own_backup(data_dir, outcome, &mut log);
            return done(outcome, log, Some(target));
        }
        return abandon(format!("save bundle: cannot commit the apply ({e}); nothing changed"), log, Some(target));
    }
    let outcome = settle(data_dir, &layout, Some(&target), Some(&current), &mut log);
    let outcome = settle_own_backup(data_dir, outcome, &mut log);
    done(outcome, log, Some(target))
}

// ── time ─────────────────────────────────────────────────────────────────────

/// `(y, m, d, hh, mm)` for unix milliseconds at a UTC offset in minutes (Howard Hinnant's
/// civil_from_days — no clock library on the phone).
pub fn civil(ms: u64, offset_min: i32) -> (i64, u32, u32, u32, u32) {
    let secs = (ms / 1000) as i64 + i64::from(offset_min) * 60;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m as u32, d as u32, (rem / 3600) as u32, (rem % 3600 / 60) as u32)
}

/// `MM-DD HH:MM` — what fits on one line of the game's box.
pub fn short_stamp(ms: u64, offset_min: i32) -> String {
    let (_, m, d, hh, mm) = civil(ms, offset_min);
    format!("{m:02}-{d:02} {hh:02}:{mm:02}")
}
