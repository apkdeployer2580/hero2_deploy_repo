//! Filesystem half of the save vault — see [`crate::savevault`] for the policy and
//! for WHY this exists.
//!
//! ── WHERE IT SITS ────────────────────────────────────────────────────────────
//! The guest's files live at `<data>/<aid>/fs/<path>` ([`crate::filesystem`]). The
//! vault is their sibling:
//!
//! ```text
//!   <data>/<aid>/fs/n1                 the live character
//!   <data>/<aid>/fs/g1                 the live progress
//!   <data>/<aid>/vault/1/<unix_ms>/n   one kept generation, both halves together
//!   <data>/<aid>/vault/1/<unix_ms>/g
//!   <data>/<aid>/vault/restore.txt     a restore scheduled for the next launch
//!   <data>/<aid>/account/bag           the ACCOUNT warehouse (#13, shared by all slots)
//!   <data>/<aid>/account/materials     … and the materials behind its 재료가방
//!   <data>/<aid>/vault/1/<unix_ms>/a   … and their copies inside each generation (optional)
//!   <data>/<aid>/vault/1/<unix_ms>/m
//!   <data>/<aid>/avatar/1              the costume record of slot 1's save (의상 아바타 A2)
//!   <data>/<aid>/avatar/1.pending      … its journal, from a save's first edge to its commit
//!   <data>/<aid>/vault/1/<unix_ms>/c   … and the record inside each generation (optional)
//! ```
//!
//! ★ #13 step 6 (2026-09-21, user decision ①a): a generation carries the ACCOUNT
//! warehouse too. The warehouse is the host's file, written when the game saves (the
//! same edge this vault watches), so restoring a slot's generation without it would
//! put back an inventory from one moment beside a warehouse from another — an item
//! moved to the warehouse after that save would then exist twice. `a` rides along:
//! kept with the pair, put back with the pair. A generation without one (older, or
//! never had a warehouse) leaves the live file alone.
//!
//! Deriving it from the aid the filesystem was handed, rather than from a guessed
//! app id, is deliberate: the vault cannot end up beside a different archive's
//! saves than the ones it is copying.
//!
//! ── WHEN IT RUNS ─────────────────────────────────────────────────────────────
//! [`before_overwrite`] is called from `AndroidFilesystem::write` and
//! `::truncate` — the only two ways a guest file can stop being what it was
//! (`wie_backend::Filesystem` has no delete). That is the emulator thread, inside
//! a WIPI call, so everything here is bounded: one cheap path test that rejects
//! every file but six, and then at most once per [`crate::savevault::MIN_GAP_MS`]
//! two ~1 KB reads, two writes and a small directory listing. The save it
//! piggybacks on already writes 2.2 KB synchronously.
//!
//! ⛔ Every failure is logged and swallowed. A vault that cannot write must never
//! be the reason a save fails — the live save is always the player's, and this is
//! only ever a second copy.

use std::{
    collections::HashMap,
    fs,
    io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::savevault::{self, Half};
use crate::save_transfer::{self, Area, Bundle};

/// Directory name of the vault, under the app's own root.
const VAULT_DIR: &str = "vault";

/// Directory name of the guest's filesystem, under the same root. Must match
/// `filesystem::AndroidFilesystem::path_for`.
const FS_DIR: &str = "fs";

/// A restore the player scheduled, waiting for the next launch.
const REQUEST_FILE: &str = "restore.txt";

/// The ACCOUNT warehouse's directory under the app root, its two files (the main bag and
/// the materials behind its 재료가방), and their names inside a generation.
const ACCOUNT_DIR: &str = "account";
pub const ACCOUNT_MAIN: &str = "bag";
pub const ACCOUNT_MATERIALS: &str = "materials";
const ACCOUNT_FILES: [(&str, &str); 2] = [(ACCOUNT_MAIN, "a"), (ACCOUNT_MATERIALS, "m")];

/// ★ 의상 아바타 A2 (2026-09-30): the per-slot costume records (`avatar/<slot>` committed,
/// `avatar/<slot>.pending` the journal — `crate::avatar_state`), and the committed record's
/// name inside a generation. A record is part of its save (it is keyed by the `/g` it was
/// committed with), so a generation carries it and a restore puts it back — or, for a
/// generation from before costumes, REMOVES the live one, so the restored save loads empty
/// rather than wearing costumes gained after it.
const AVATAR_DIR: &str = "avatar";
const AVATAR_GEN_NAME: &str = "c";

/// `/g<slot>` edges per file slot (index 1..=5): the worker commits a slot's costume record
/// when that slot's `/g` has been rewritten and reads whole again. `/n` alone is not a save
/// the record binds to (`b.a(BB)V` / `y.*` write the bags by themselves).
static G_EDGES: [std::sync::atomic::AtomicU64; 6] = [const { std::sync::atomic::AtomicU64::new(0) }; 6];

pub fn g_edges(slot: u8) -> u64 {
    G_EDGES.get(usize::from(slot)).map_or(0, |a| a.load(std::sync::atomic::Ordering::Relaxed))
}

/// The costume state of the character in the world, for the journal (`None` = nobody).
/// Published by the worker; read here, on the same thread, inside the save's first `/g` edge.
static AVATAR_WORKING: Mutex<Option<crate::avatar_state::AvatarState>> = Mutex::new(None);

/// Per file slot: may the next `/g` edge write the journal? One journal per save — the first
/// edge (the truncate, while `/g` is still the old save) writes it and disarms; the worker's
/// commit re-arms. A save that never completes leaves it disarmed, and the next save's commit
/// falls back to the worker's own copy.
static AVATAR_ARMED: Mutex<[bool; 6]> = Mutex::new([true; 6]);

/// The worker's costume state changed (a world entry, a grant, a wear) — or the world was
/// left (`None`). Re-arms every slot's journal.
pub fn publish_avatar_working(state: Option<&crate::avatar_state::AvatarState>) {
    if let Ok(mut g) = AVATAR_WORKING.lock() {
        *g = state.cloned();
    }
    if let Ok(mut a) = AVATAR_ARMED.lock() {
        *a = [true; 6];
    }
}

/// After a commit: the next save of `slot` journals again.
pub fn rearm_avatar_journal(slot: u8) {
    if let Ok(mut a) = AVATAR_ARMED.lock()
        && let Some(v) = a.get_mut(usize::from(slot))
    {
        *v = true;
    }
}

/// The costume records' directory, once an app root exists.
pub fn avatar_files(data_dir: &Path) -> Option<crate::avatar_state::AvatarFiles> {
    app_roots(data_dir).into_iter().next().map(|r| crate::avatar_state::AvatarFiles::new(r.join(AVATAR_DIR)))
}

/// The live `/g<slot>` as the game last wrote it.
pub fn read_live_g(data_dir: &Path, slot: u8) -> Option<Vec<u8>> {
    let root = app_roots(data_dir).into_iter().next()?;
    fs::read(root.join(FS_DIR).join(savevault::guest_path(Half::Prog, slot))).ok()
}

/// The journal: at the first `/g` edge of a save, the state being saved, keyed by the hash of
/// the `/g` it replaces (`crate::avatar_state` header). Emulator thread, inside the save —
/// one ~1 KB read and one small atomic write, once per save. Failures are logged and
/// swallowed: without a journal the commit still happens, only the crash window is open.
fn journal_avatar(root: &Path, slot: u8) {
    {
        let Ok(mut armed) = AVATAR_ARMED.lock() else { return };
        match armed.get_mut(usize::from(slot)) {
            Some(a) if *a => *a = false,
            _ => return,
        }
    }
    let working = match AVATAR_WORKING.lock() {
        Ok(g) => g.clone(),
        Err(_) => return,
    };
    let Some(state) = working else { return };
    let current = fs::read(root.join(FS_DIR).join(savevault::guest_path(Half::Prog, slot))).ok();
    let files = crate::avatar_state::AvatarFiles::new(root.join(AVATAR_DIR));
    match crate::avatar_state::write_journal(&files, slot, current.as_deref(), &state) {
        Ok(base) => log::info!("avatar: slot {slot} journal written (base {base:016x}, wearing {:?})", state.worn_avatars()),
        Err(e) => log::warn!("avatar: slot {slot} journal NOT written: {e}"),
    }
}

/// How many save-file EDGES the guest has produced (every `n<slot>` / `g<slot>` write or
/// truncate, gap or not). The worker polls it: a change means the game just committed,
/// and the warehouse blob is written at that moment and no other — so leaving without
/// saving forgets a move the way it forgets everything else.
static SAVE_EDGES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn save_edges() -> u64 {
    SAVE_EDGES.load(std::sync::atomic::Ordering::Relaxed)
}

/// ★ 2026-10-28 (code review, second pass P1): the host's own files that ride on a save — the warehouse
/// blobs, the costume records, the plate store — as the worker last found them. The 진단 정보 → 세이브
/// 관리 export runs on the UI thread and cannot ask the worker, so the worker publishes this every loop
/// and [`export_bundle`], which both export paths come through, reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCommits {
    /// Every save so far is on the disk with its host files.
    Settled,
    /// A save's host files are still being written (or its edge is not seen yet).
    Pending,
    /// A save's warehouse blob did not land: the disk holds the warehouse of before.
    AccountFailed,
    /// A save's costume record did not land: the disk holds the record of before.
    AvatarFailed,
}

/// What the worker published, and the save edge it was true at. `None` = no worker in this process
/// has published (no save was made in it, so none can be owed).
static HOST_COMMITS: Mutex<Option<(HostCommits, u64)>> = Mutex::new(None);

/// The worker, every loop (on its own thread — the only one the guest saves on).
pub fn publish_host_commits(state: HostCommits) {
    if let Ok(mut g) = HOST_COMMITS.lock() {
        *g = Some((state, save_edges()));
    }
}

fn host_commits() -> Option<(HostCommits, u64)> {
    HOST_COMMITS.lock().ok().and_then(|g| *g)
}

/// The export's refusals while a save's host files are not all on the disk beside it.
pub const ACCOUNT_UNSAVED: &str = "계정창고를 저장하지 못해 내보내지 않았습니다. 다시 저장한 뒤 눌러 주세요.";
pub const AVATAR_UNSAVED: &str = "아바타 정보를 저장하지 못해 내보내지 않았습니다. 다시 저장한 뒤 눌러 주세요.";
pub const SAVE_SETTLING: &str = "저장을 마무리하는 중입니다. 잠시 후 다시 눌러 주세요.";

/// Why an export must not take the disk: `published` = what the worker last said and at which save
/// edge, `edges` = the save edges now. A save since the worker looked owes host files it has not seen.
fn host_commits_refusal(published: Option<(HostCommits, u64)>, edges: u64) -> Option<&'static str> {
    match published {
        None => None,
        Some((_, at)) if at != edges => Some(SAVE_SETTLING),
        Some((HostCommits::Settled, _)) => None,
        Some((HostCommits::Pending, _)) => Some(SAVE_SETTLING),
        Some((HostCommits::AccountFailed, _)) => Some(ACCOUNT_UNSAVED),
        Some((HostCommits::AvatarFailed, _)) => Some(AVATAR_UNSAVED),
    }
}

/// The app root (`<data>/<aid>`) the filesystem is actually using, remembered the
/// first time the guest touches a save. Lets the JNI list find the vault without
/// having to guess an app id.
static APP_ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Last snapshot per slot, for [`savevault::should_snapshot`]. In memory on
/// purpose: a fresh process snapshotting on its first save is the behaviour we
/// want, not a bug to suppress.
static LAST_MS: Mutex<Option<HashMap<u8, u64>>> = Mutex::new(None);

/// Something the player has to be told on the first frame. Drained by `app.rs`.
static BOOT_NOTICE: Mutex<Option<&'static str>> = Mutex::new(None);

/// Wall clock in milliseconds. Zero if the clock is before the epoch, which only
/// makes [`savevault::should_snapshot`] more willing.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Remember which app root the guest's filesystem is using.
fn note_root(root: &Path) {
    if let Ok(mut g) = APP_ROOT.lock()
        && g.is_none()
    {
        log::info!("vault: app root = {root:?}");
        *g = Some(root.to_path_buf());
    }
}

/// The app root the filesystem is actually using, as soon as ANYTHING touches a guest
/// file — a read is enough, and the game reads its save long before it writes one.
///
/// ⛔ Publishing this was `before_overwrite`'s alone, i.e. it happened on the first SAVE
/// (2026-09-21, review). The ACCOUNT warehouse is read on the first in-world frame, far
/// earlier, so [`account_path`] fell through to [`app_roots`]' directory scan and took
/// whichever `<aid>` directory the filesystem happened to list first. With a second
/// archive root under `<data>` — a regenerated asset with a different AID leaves the old
/// tree behind, and this repo regenerates its asset — the host loaded ANOTHER archive's
/// warehouse, wrote it back under the right root on the first save, and from then on
/// archived it into every generation. Every other path in this module derives its
/// location from the aid the filesystem was handed, precisely so it cannot land beside a
/// different archive's saves; this makes the warehouse reader one of them.
pub fn note_app_root(base_path: &Path, aid: &str) {
    note_root(&base_path.join(aid));
}

/// A guest file is about to be overwritten or truncated. Keep what it says now.
///
/// `base_path` and `aid` are the filesystem's own, so this lands beside the very
/// files it is copying. `guest_path` is rejected immediately unless it is one of
/// the six that are a character.
pub fn before_overwrite(base_path: &Path, aid: &str, guest_path: &str) {
    // Cheapest possible rejection: this runs on every guest write, and all but
    // six paths in the whole game are not saves.
    let Some((half, slot)) = savevault::classify(guest_path) else {
        return;
    };
    let root = base_path.join(aid);
    note_root(&root);
    SAVE_EDGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if half == Half::Prog {
        // ★ 의상 아바타 A2: the journal BEFORE the game touches `/g` (this edge runs ahead of
        //   the truncate / write it announces), then the edge the worker's commit watches.
        journal_avatar(&root, slot);
        if let Some(e) = G_EDGES.get(usize::from(slot)) {
            e.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    let now = now_ms();
    // ★ Read the gap BEFORE doing any I/O, and take the lock only for that. The
    // second half of a save pair arrives milliseconds after the first and must
    // fall out here, not after two more file reads.
    //
    // ★★ AND STAMP IT HERE, WHATEVER HAPPENS NEXT (2026-09-20, review). The stamp used
    // to be written only after a successful `keep`, so that a transient disk error could
    // be retried — but the only retry that ever arrives is the OTHER HALF of the same
    // save, milliseconds later, and by then the guest has already rewritten the half we
    // are standing on. That retry therefore read a NEW `n<slot>` next to the OLD
    // `g<slot>` and vaulted the pair: a character from one moment wearing another
    // moment's progress, offered in the list and restorable — precisely what the pair
    // discipline exists to prevent, arrived at by being careful. (Same hole, two ways in:
    // a torn half on disk makes the first edge bail before `keep` is even reached.)
    //
    // A failed snapshot now costs that one generation, and the next SAVE the player makes
    // is a fresh burst that retries by itself. A missing generation is recoverable; a
    // mismatched one that looks intact is not.
    {
        let Ok(mut g) = LAST_MS.lock() else {
            return;
        };
        let map = g.get_or_insert_with(HashMap::new);
        if !savevault::should_snapshot(map.get(&slot).copied(), now) {
            return;
        }
        map.insert(slot, now);
    }

    // ★ BOTH halves, read now, while NEITHER has been touched. The file whose
    // edge we are on is still its old self (the truncate/write has not happened
    // yet) and so is its partner (the guest writes them back to back). That is
    // what makes the pair a consistent instant whichever order the game uses.
    let live = root.join(FS_DIR);
    let mut pair: Vec<(Half, Vec<u8>)> = Vec::with_capacity(2);
    for half in Half::BOTH {
        let path = live.join(savevault::guest_path(half, slot));
        match fs::read(&path) {
            Ok(bytes) if savevault::usable_len(bytes.len()) => pair.push((half, bytes)),
            Ok(bytes) => {
                // A torn or absent half. Not an error — the very first save of a
                // fresh slot looks exactly like this.
                //
                // ★ And so does every save to a slot the player has not used yet,
                // for a reason worth knowing: the archive's own `P/n2`, `P/g2`…
                // are VIRTUAL files held in memory by `FilesystemOverlay`, not
                // files on disk. Reads prefer the disk and fall back to them, so
                // the first save to such a slot creates the disk file and shadows
                // the archived one forever. There is nothing to keep at that
                // moment — the bytes being shadowed are the original dump's
                // previous owner, not this player's character — and this layer
                // could not reach them anyway.
                log::info!(
                    "vault: slot {slot} half {} is {} bytes — nothing worth keeping yet",
                    half.letter(),
                    bytes.len()
                );
                return;
            }
            Err(_) => return,
        }
    }

    if let Err(e) = keep(&root, slot, now, &pair, None) {
        log::warn!("vault: could not keep slot {slot} @ {now}: {e}");
        return;
    }
    log::info!("vault: kept slot {slot} @ {now} ({} + {} bytes)", pair[0].1.len(), pair[1].1.len());
}

/// Write one generation and trim the ring.
///
/// `pin` is a generation rotation must not touch — the one a restore is putting
/// back. Without it, capturing the pre-restore state could evict the very
/// generation being restored, and a player who picked a deep one could not pick
/// it again.
fn keep(root: &Path, slot: u8, unix_ms: u64, pair: &[(Half, Vec<u8>)], pin: Option<u64>) -> io::Result<()> {
    let dir = slot_dir(root, slot).join(unix_ms.to_string());
    fs::create_dir_all(&dir)?;
    for (half, bytes) in pair {
        write_atomic(&dir.join(half.letter().to_string()), bytes)?;
    }
    // The warehouse as it stands — the state that goes with THIS pair (its next write
    // comes after the save these edges belong to). Best effort: no warehouse, no copy.
    for (file, gen_name) in ACCOUNT_FILES {
        if let Ok(bytes) = fs::read(account_file(root, file)) {
            if let Err(e) = write_atomic(&dir.join(gen_name), &bytes) {
                log::warn!("vault: slot {slot} @ {unix_ms}: could not keep the warehouse {file}: {e}");
            }
        }
    }
    // ★ 의상 아바타 A2: the slot's committed costume record — the one keyed by the `/g` just
    //   kept (it was committed with that save; the save now being written commits its own
    //   after this edge). Best effort, like the warehouse: no record, no copy.
    if let Ok(bytes) = fs::read(root.join(AVATAR_DIR).join(slot.to_string())) {
        if let Err(e) = write_atomic(&dir.join(AVATAR_GEN_NAME), &bytes) {
            log::warn!("vault: slot {slot} @ {unix_ms}: could not keep the costume record: {e}");
        }
    }
    rotate(root, slot, pin);
    Ok(())
}

fn account_file(root: &Path, file: &str) -> PathBuf {
    root.join(ACCOUNT_DIR).join(file)
}

/// Where a warehouse blob (`ACCOUNT_MAIN` / `ACCOUNT_MATERIALS`) lives for this app, or
/// `None` before any root exists (a fresh install that has not saved yet — the first save
/// creates the root).
pub fn account_path(data_dir: &Path, file: &str) -> Option<PathBuf> {
    app_roots(data_dir).into_iter().next().map(|r| account_file(&r, file))
}

/// A warehouse blob, if there is one.
pub fn read_account(data_dir: &Path, file: &str) -> Option<Vec<u8>> {
    fs::read(account_path(data_dir, file)?).ok()
}

/// Write a warehouse blob — atomically, like the live save it travels with.
pub fn write_account(data_dir: &Path, file: &str, bytes: &[u8]) -> io::Result<()> {
    let path = account_path(data_dir, file).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no app root yet"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    write_atomic(&path, bytes)
}

/// Drop the oldest generations of one slot past the ceiling.
///
/// ★ A generation a restore is already SCHEDULED for is pinned too (2026-09-20, review).
/// The dialog's "나중에" button lets the player keep playing after choosing one, and
/// every save they make while it is pending rotates this ring — so the generation they
/// picked was an ordinary oldest-first candidate and, past the ceiling, would be deleted
/// before the relaunch that was supposed to put it back. They would get "복원 실패" with
/// no way to tell why.
fn rotate(root: &Path, slot: u8, pin: Option<u64>) {
    let dir = slot_dir(root, slot);
    let held = generations(&dir);
    let pending = pending_request(root).filter(|(s, _)| *s == slot).map(|(_, ms)| ms);
    let pinned = |ms: u64| Some(ms) == pin || Some(ms) == pending;
    let candidates: Vec<u64> = held.into_iter().filter(|ms| !pinned(*ms)).collect();
    // A pinned generation occupies a seat without being a candidate, so the
    // ceiling for the rest is one lower — otherwise a pin would silently raise
    // the ceiling by one every time a restore happened.
    let pins = usize::from(pin.is_some()) + usize::from(pending.is_some() && pending != pin);
    let keep_n = savevault::KEEP_MAX.saturating_sub(pins);
    for ms in savevault::drop_list(&candidates, keep_n) {
        let victim = dir.join(ms.to_string());
        match fs::remove_dir_all(&victim) {
            Ok(()) => log::info!("vault: slot {slot} dropped generation {ms}"),
            Err(e) => log::warn!("vault: slot {slot} cannot drop {ms}: {e}"),
        }
    }
}

fn slot_dir(root: &Path, slot: u8) -> PathBuf {
    root.join(VAULT_DIR).join(slot.to_string())
}

/// Every generation timestamp a slot directory holds, unsorted.
fn generations(dir: &Path) -> Vec<u64> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|s| s.parse::<u64>().ok()))
        .collect()
}

/// Replace a file without ever leaving a torn one in its place.
///
/// ⛔ Not `fs::write` for the destinations that matter: a process killed partway
/// through leaves the file half-written, and for the live save that is precisely
/// the loss this module exists to undo. The temp name starts with a dot so
/// [`savevault::classify`] can never mistake it for a save and vault it.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("tmp");
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

// ── reading the vault back ───────────────────────────────────────────────────

/// One kept generation, for the restore UI.
#[derive(Debug, Clone)]
pub struct Entry {
    pub slot: u8,
    pub unix_ms: u64,
    /// Length of the character half — what the dialog shows.
    pub char_len: usize,
    /// The app root this generation lives under.
    root: PathBuf,
}

impl Entry {
    fn dir(&self) -> PathBuf {
        slot_dir(&self.root, self.slot).join(self.unix_ms.to_string())
    }
}

/// The app roots to look in.
///
/// The remembered one first (set the moment the guest touched a save), then a
/// scan of `data_dir` for anything that looks like one. The scan is what answers
/// at BOOT, before the emulator has run and before anything could have been
/// remembered — which is exactly when [`apply_pending_restore`] asks.
///
/// ★ hero3 needs an app id here because a device that once ran a different build
/// of that game has two of these directories. hero2 bundles exactly one archive
/// and has only ever had one aid, so the scan is unambiguous; it still returns
/// every hit rather than picking one, so a second archive could never silently
/// shadow the player's real vault.
fn app_roots(data_dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Ok(g) = APP_ROOT.lock()
        && let Some(root) = g.clone()
        && root.is_dir()
    {
        out.push(root);
    }
    if let Ok(rd) = fs::read_dir(data_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if !p.join(VAULT_DIR).is_dir() && !p.join(FS_DIR).is_dir() {
                continue;
            }
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Every generation that could actually be restored, newest first.
///
/// A generation missing a half — or holding a torn one — is NOT listed. Offering
/// it would put a character from one moment next to another moment's progress,
/// which is the mismatch this vault keeps pairs to avoid.
pub fn list(data_dir: &Path) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for root in app_roots(data_dir) {
        for slot in savevault::SLOT_MIN..=savevault::SLOT_MAX {
            let dir = slot_dir(&root, slot);
            for ms in generations(&dir) {
                let gen_dir = dir.join(ms.to_string());
                let mut lens: Vec<usize> = Vec::with_capacity(2);
                for half in Half::BOTH {
                    match fs::metadata(gen_dir.join(half.letter().to_string())) {
                        Ok(md) if savevault::usable_len(md.len() as usize) => lens.push(md.len() as usize),
                        _ => break,
                    }
                }
                if lens.len() != 2 {
                    continue;
                }
                out.push(Entry { slot, unix_ms: ms, char_len: lens[0], root: root.clone() });
            }
        }
    }
    out.sort_by(|a, b| b.unix_ms.cmp(&a.unix_ms).then(a.slot.cmp(&b.slot)));
    out
}

/// The list as the Java dialog wants it: `slot|unix_ms|char_len` lines, newest
/// first, empty when there is nothing restorable.
pub fn offer_lines(data_dir: &Path) -> String {
    list(data_dir)
        .into_iter()
        .map(|e| savevault::offer_line(e.slot, e.unix_ms, e.char_len))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── restoring ────────────────────────────────────────────────────────────────

fn request_path(root: &Path) -> PathBuf {
    root.join(VAULT_DIR).join(REQUEST_FILE)
}

/// The generation a restore is already scheduled for in this root, if any.
///
/// Read from the file every time rather than cached: it is written by the JNI call on
/// the UI thread, read here on the emulator worker, and it outlives the process — a
/// cached copy would be wrong in both directions. One read per SAVE, which is a
/// hand-driven event.
fn pending_request(root: &Path) -> Option<(u8, u64)> {
    let spec = fs::read_to_string(request_path(root)).ok()?;
    savevault::parse_key(spec.lines().next()?.trim())
}

/// Schedule a restore for the next launch. `key` comes from [`offer_lines`].
///
/// ⛔ Scheduled, not applied. The guest holds the character in its own heap and
/// writes the whole pair back on its next save, so swapping the files underneath
/// a running game would be undone seconds later — by the very state the player
/// is trying to get rid of. [`apply_pending_restore`] runs before the emulator
/// exists, which is the only moment nothing can overwrite it.
pub fn request_restore(data_dir: &Path, key: &str) -> bool {
    let Some((slot, ms)) = savevault::parse_key(key) else {
        log::warn!("vault: restore request {key:?} is not slot|ms");
        return false;
    };
    let Some(entry) = list(data_dir).into_iter().find(|e| e.slot == slot && e.unix_ms == ms) else {
        log::warn!("vault: restore request for slot {slot} @ {ms} names no generation we hold");
        return false;
    };
    // ★ 세이브 옮기기: one thing is put back at a launch, and it is the last one asked for — a file
    //   staged earlier would be applied first and this restore then dropped (`drop_restore_request`).
    //   ★ 2026-10-28: so the stage goes FIRST and durably, and a stage that cannot surely go refuses
    //   this request (`savebundle` rule 5 — a promise left on the disk is kept at the next start).
    match crate::savebundle::unstage(data_dir) {
        Ok(true) => log::warn!("vault: the file waiting for the next launch is cancelled — a restore was scheduled after it"),
        Ok(false) => {}
        Err(e) => {
            log::error!("vault: the file waiting for the next launch cannot be cancelled ({e}) — the restore is not scheduled");
            return false;
        }
    }
    let path = request_path(&entry.root);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        log::error!("vault: cannot make room for the restore request: {e}");
        return false;
    }
    match fs::write(&path, savevault::key(slot, ms)) {
        Ok(()) => {
            log::warn!("vault: restore of slot {slot} @ {ms} scheduled for the next launch");
            true
        }
        Err(e) => {
            log::error!("vault: cannot write the restore request: {e}");
            false
        }
    }
}

/// Apply and consume a pending restore. Call ONCE at startup, before the emulator
/// boots. Returns a line for the log when something happened.
///
/// The request is deleted FIRST. A restore that fails must not re-run on every
/// launch: a loop that keeps writing the same generation over the save is
/// indistinguishable from the bug this feature exists to undo.
pub fn apply_pending_restore(data_dir: &Path) -> Option<String> {
    // ★ The FIRST root that has one, and only that one is consumed (2026-09-20, review).
    // The loop used to read and unlink a request in every root while keeping the last —
    // so with two roots the earlier request was destroyed without ever being honoured,
    // and `app_roots` puts the REMEMBERED root first precisely because it is the one that
    // should win.
    let mut found: Option<(PathBuf, String)> = None;
    for root in app_roots(data_dir) {
        let path = request_path(&root);
        if let Ok(spec) = fs::read_to_string(&path) {
            let _ = fs::remove_file(&path);
            found = Some((root, spec));
            break;
        }
    }
    let (_, spec) = found?;
    let key = spec.lines().next()?.trim().to_string();
    let (slot, ms) = savevault::parse_key(&key)?;

    let entry = list(data_dir).into_iter().find(|e| e.slot == slot && e.unix_ms == ms);
    let Some(entry) = entry else {
        set_notice("복원 실패");
        return Some(format!("restore of slot {slot} @ {ms} FAILED: the generation is gone"));
    };
    match restore(&entry) {
        Ok(()) => {
            set_notice("세이브 복원됨");
            Some(format!("RESTORED slot {slot} from {ms}"))
        }
        Err(e) => {
            // ⛔ The request is already consumed (deliberately), so silence here
            // would leave the player with a relaunch that did nothing and said
            // nothing. It has to be audible.
            set_notice("복원 실패");
            Some(format!("restore of slot {slot} @ {ms} FAILED: {e}"))
        }
    }
}

/// Put one generation back over the live pair.
///
/// ★ The CURRENT pair is kept first, unconditionally and with the restored
/// generation pinned. A restore is destructive — it overwrites a save — so the
/// state the player restored over has to stay recoverable, and the generation
/// they just picked has to survive being picked.
fn restore(entry: &Entry) -> io::Result<()> {
    let dir = entry.dir();

    // ★ Read BOTH halves fully before anything is written. A generation that
    // cannot be read completely must cost the live save nothing.
    let mut pair: Vec<(Half, Vec<u8>)> = Vec::with_capacity(2);
    for half in Half::BOTH {
        let bytes = fs::read(dir.join(half.letter().to_string()))?;
        if !savevault::usable_len(bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("half {} is {} bytes", half.letter(), bytes.len()),
            ));
        }
        pair.push((half, bytes));
    }

    // The warehouse of that moment, when the generation has one. Read before anything is
    // written, like the halves.
    //
    // ⛔ BOTH blobs or NEITHER (2026-09-21, review). `filter_map(..ok())` stood here, and
    // it made the warehouse the one thing this module restores as a torn pair: a
    // generation holding only `a` would put the main bag back while leaving the live
    // materials bag at some other moment — a 재료가방 record could then exist in both, or
    // in neither, which is exactly the mismatch the n/g pairing rule exists to prevent.
    // Every generation written since the warehouse landed has both (an empty materials
    // bag still serialises to one byte), so "only one" means a damaged or half-written
    // generation, and the honest answer there is to leave the live warehouse alone.
    let account: Vec<(&str, Vec<u8>)> = {
        let read: Vec<Option<(&str, Vec<u8>)>> =
            ACCOUNT_FILES.iter().map(|(file, gen_name)| fs::read(dir.join(gen_name)).ok().map(|b| (*file, b))).collect();
        if read.iter().all(Option::is_some) {
            read.into_iter().flatten().collect()
        } else {
            if read.iter().any(Option::is_some) {
                log::warn!(
                    "vault: generation {} of slot {} holds only one warehouse blob — leaving the live warehouse alone",
                    entry.unix_ms,
                    entry.slot
                );
            }
            Vec::new()
        }
    };

    // ★ 의상 아바타 A2: the costume record of that moment, when the generation has one — read
    //   before anything is written, like the halves.
    let costume = fs::read(dir.join(AVATAR_GEN_NAME)).ok();

    keep_pre_restore(entry);

    let live = entry.root.join(FS_DIR);
    fs::create_dir_all(&live)?;
    for (half, bytes) in &pair {
        write_atomic(&live.join(savevault::guest_path(*half, entry.slot)), bytes)?;
    }
    // The record goes back with its save — or, for a generation from before costumes, the live
    // record goes AWAY: left in place it would belong to no save on disk and load as nothing
    // anyway (it is keyed by another `/g`), but removing it says so on disk too. The journal
    // always goes: it belonged to a save that is no longer the live one.
    {
        let files = crate::avatar_state::AvatarFiles::new(entry.root.join(AVATAR_DIR));
        match &costume {
            Some(bytes) => {
                fs::create_dir_all(&files.dir)?;
                write_atomic(&files.committed_path(entry.slot), bytes)?;
                files.remove_pending(entry.slot);
                log::warn!("vault: RESTORED the costume record of slot {} ({} bytes)", entry.slot, bytes.len());
            }
            None => {
                files.remove_all(entry.slot);
                log::warn!("vault: generation {} of slot {} predates costumes — its character loads with none", entry.unix_ms, entry.slot);
            }
        }
    }
    for (file, bytes) in &account {
        let path = account_file(&entry.root, file);
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        write_atomic(&path, bytes)?;
        log::warn!("vault: RESTORED the warehouse {file} with slot {} ({} bytes)", entry.slot, bytes.len());
    }
    log::warn!(
        "vault: RESTORED slot {} from {} ({} + {} bytes)",
        entry.slot,
        entry.unix_ms,
        pair[0].1.len(),
        pair[1].1.len()
    );
    Ok(())
}

/// Keep whatever is live right now, before a restore writes over it.
///
/// Best effort: a restore must not be refused because the state it is replacing
/// could not be copied. That state is, by the player's own account, the broken
/// one.
///
/// ⛔ Best effort means "archive what CAN be read", not "archive nothing unless
/// everything can be read" (2026-09-21, review). An unreadable half used to
/// `return` from here — and an unreadable half is precisely the state a player
/// opens the restore dialog in, so in the one case this function exists for it
/// did nothing. That was survivable while a generation was only the slot's own
/// n/g pair, because `restore` then overwrote only what it had failed to copy.
/// It stopped being survivable when the ACCOUNT warehouse joined the generation:
/// the warehouse is shared by ALL the slots (five since 2026-09-27) and lives
/// outside the slot being repaired, so returning early let a restore rewind — with
/// no copy anywhere — every item the player had stored since that generation,
/// including items belonging to characters in the other slots.
///
/// A partial pair cannot be restored FROM: [`list`] skips any generation that
/// does not hold both halves, so what this writes is a warehouse copy plus
/// whatever survived, never a mismatched offer.
fn keep_pre_restore(entry: &Entry) {
    let live = entry.root.join(FS_DIR);
    let mut pair: Vec<(Half, Vec<u8>)> = Vec::with_capacity(2);
    for half in Half::BOTH {
        match fs::read(live.join(savevault::guest_path(half, entry.slot))) {
            Ok(bytes) if savevault::usable_len(bytes.len()) => pair.push((half, bytes)),
            _ => log::warn!(
                "vault: the live {} half of slot {} is missing or torn — keeping the rest before the restore",
                half.letter(),
                entry.slot
            ),
        }
    }
    // Nothing live and no warehouse to shelter: an empty generation directory would
    // only occupy a seat in the ring that `rotate` then has to drop again.
    if pair.is_empty() && !ACCOUNT_FILES.iter().any(|(file, _)| account_file(&entry.root, file).exists()) {
        log::warn!("vault: nothing live to keep before restoring slot {}", entry.slot);
        return;
    }
    let now = now_ms();
    if let Err(e) = keep(&entry.root, entry.slot, now, &pair, Some(entry.unix_ms)) {
        log::warn!("vault: could not keep the pre-restore state of slot {}: {e}", entry.slot);
    } else {
        log::warn!("vault: kept the pre-restore state of slot {} as {now}", entry.slot);
    }
}

// ── the boot notice ──────────────────────────────────────────────────────────

fn set_notice(msg: &'static str) {
    if let Ok(mut g) = BOOT_NOTICE.lock() {
        *g = Some(msg);
    }
}

/// Take the message a boot-time restore left, if any. Drained by `app.rs` on the
/// first frame — the player asked for this and has to see that it happened.
///
/// ⛔ Every syllable must exist in `hangul.rs`, or the toast draws blank.
pub fn take_boot_notice() -> Option<&'static str> {
    BOOT_NOTICE.lock().ok().and_then(|mut g| g.take())
}

// ── getting the backups off the device ───────────────────────────────────────

/// Copy the whole vault somewhere a PC can reach, returning how many files went.
///
/// Opening the restore dialog is also the moment support gets involved, and the
/// release build is not debuggable, so `/sdcard/Android/data/<pkg>/files` is the
/// ONLY channel a player's backups can travel on — `adb pull` reaches it without
/// root.
pub fn export(data_dir: &Path, dest: &Path) -> io::Result<usize> {
    let mut n = 0usize;
    for e in list(data_dir) {
        let out_dir = dest.join(e.slot.to_string()).join(e.unix_ms.to_string());
        fs::create_dir_all(&out_dir)?;
        let names = Half::BOTH.iter().map(|h| h.letter().to_string()).chain(std::iter::once(AVATAR_GEN_NAME.to_string()));
        for name in names {
            // ⛔ NOT `fs::copy`: it truncates the destination and only then
            // streams, so a source that dies mid-read — the very case this dump
            // exists for — would destroy an earlier, intact export in its place.
            let Ok(bytes) = fs::read(e.dir().join(&name)) else {
                continue;
            };
            if write_atomic(&out_dir.join(&name), &bytes).is_ok() {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// The marker that asks the next launch for a copy of the LIVE saves.
pub const EXPORT_LIVE_MARKER: &str = "wie_export_saves.txt";

/// Copy the LIVE saves (`fs/` and the ACCOUNT warehouse) next to `marker_dir` when a
/// marker there asks for it, and consume the marker. Called at boot, before the
/// emulator exists, so nothing is mid-write.
///
/// ★ 2026-09-26: [`export`] carries the vault — the pairs from BEFORE each overwrite —
/// and never the save the player is standing on, which is exactly what a bug report
/// ("play my slot and you will see it") needs. The release build is not debuggable, so
/// `run-as` cannot reach `fs/`; this is the same channel as `export`, but only on
/// request: `adb shell touch /sdcard/Android/data/com.hero2remake.app/files/wie_export_saves.txt`,
/// launch, `adb pull …/files/save_export`. Nothing a player can tap triggers it. The
/// saves are only READ; the marker goes first, so a failing copy cannot repeat on
/// every launch.
pub fn export_live_on_request(data_dir: &Path, marker_dir: &Path) -> Option<String> {
    let marker = marker_dir.join(EXPORT_LIVE_MARKER);
    if !marker.is_file() {
        return None;
    }
    if let Err(e) = fs::remove_file(&marker) {
        return Some(format!("live export NOT done: the marker could not be consumed ({e})"));
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let dest = marker_dir.join("save_export").join(stamp.to_string());
    let mut n = 0usize;
    for root in app_roots(data_dir) {
        let aid = root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        for sub in [FS_DIR, ACCOUNT_DIR, AVATAR_DIR] {
            let Ok(rd) = fs::read_dir(root.join(sub)) else { continue };
            let out_dir = dest.join(&aid).join(sub);
            if fs::create_dir_all(&out_dir).is_err() {
                continue;
            }
            for e in rd.flatten() {
                let p = e.path();
                if !p.is_file() {
                    continue;
                }
                if let (Ok(bytes), Some(name)) = (fs::read(&p), p.file_name())
                    && write_atomic(&out_dir.join(name), &bytes).is_ok()
                {
                    n += 1;
                }
            }
        }
    }
    Some(format!("live export: {n} file(s) to {dest:?}"))
}

/// ★ 2026-10-03: the directory an adb user stages REPLACEMENT save files in — the mirror of
/// [`export_live_on_request`]'s `save_export/<stamp>/<aid>/…`:
/// `<marker_dir>/save_put/<aid>/{fs,account,avatar}/<name>`.
pub const PUT_LIVE_DIR: &str = "save_put";

/// Put the staged files of [`PUT_LIVE_DIR`] over the live ones, and consume the staging tree.
/// Called at boot, before the emulator exists (nothing holds a character in a heap), after a
/// scheduled import / restore and before the export marker — so `export` in the same launch
/// shows the result.
///
/// User, 2026-10-03: *"신규 장신구 6종 s9, s25의 첫번쨰 세이브슬롯에 모두 넣어줘"* — a release build
/// is not debuggable, so adb cannot write `fs/`; the 세이브 옮기기 import cannot serve either: it
/// makes the phone EQUAL to the file, deleting what the file does not carry (the host's plate
/// rows, 난이도, 경험치배율 — which the export marker never copies out). This one only REPLACES:
/// a file that is staged is written, nothing else is touched or removed. The round trip is
/// `touch wie_export_saves.txt` → launch → pull `save_export/<stamp>/<aid>/` → edit (e.g.
/// `wie_probe --h2-give` on that tree) → push the changed files to `save_put/<aid>/…` → launch.
///
/// ⛔ All or nothing, checked before anything is written: every staged file must be a name the
/// transfer whitelist lists for its area (`save_transfer::names` — `fs` saves and `cf`,
/// `account`, `avatar`; no host file, no path taken from outside), at most `MEMBER_MAX` bytes,
/// and a save half must pass `savevault::usable_len`. The staging tree is removed FIRST (a
/// request that fails must not repeat on every launch). Every slot a staged file belongs to —
/// every live slot when the shared warehouse is staged — has its live pair kept in the vault
/// before the first write (「백업에서 복원」 undoes it); a slot that cannot be kept stops the put
/// with nothing written. Only the archive this app runs (`archive_aid`) is read.
/// ⚠ A `/g` put without its `avatar/<slot>` record loads its character without costumes (the
/// record is bound to the `/g` it was committed with) — replacing `/n` alone, the bags, keeps both.
pub fn put_live_on_request(data_dir: &Path, marker_dir: &Path, archive_aid: Option<&str>) -> Option<String> {
    put_live_at(data_dir, marker_dir, archive_aid, now_ms())
}

fn put_live_at(data_dir: &Path, marker_dir: &Path, archive_aid: Option<&str>, now: u64) -> Option<String> {
    let staged = marker_dir.join(PUT_LIVE_DIR);
    if !staged.is_dir() {
        return None;
    }
    // Read everything staged for this archive, then consume the tree — whatever happens next.
    let mut files: Vec<(Area, String, Vec<u8>)> = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    let aid_dir = archive_aid.map(|aid| staged.join(aid));
    if let Some(dir) = aid_dir.as_ref().filter(|d| d.is_dir()) {
        for area in [Area::Fs, Area::Account, Area::Avatar] {
            let Ok(rd) = fs::read_dir(dir.join(area.dir())) else { continue };
            let allowed = save_transfer::names(area);
            for e in rd.flatten() {
                let p = e.path();
                let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if !p.is_file() || !allowed.contains(&name) {
                    bad.push(format!("{}/{name}: not a save file name", area.dir()));
                    continue;
                }
                match fs::read(&p) {
                    Ok(b) if b.len() <= save_transfer::MEMBER_MAX => {
                        let half = area == Area::Fs && name != save_transfer::GAME_SETTINGS;
                        if half && !savevault::usable_len(b.len()) {
                            bad.push(format!("{}/{name}: {} bytes is not a whole save half", area.dir(), b.len()));
                        } else {
                            files.push((area, name, b));
                        }
                    }
                    Ok(b) => bad.push(format!("{}/{name}: {} bytes", area.dir(), b.len())),
                    Err(e) => bad.push(format!("{}/{name}: {e}", area.dir())),
                }
            }
        }
    }
    if let Err(e) = fs::remove_dir_all(&staged) {
        return Some(format!("save put NOT done: the staged files could not be consumed ({e})"));
    }
    let Some(aid) = archive_aid else {
        return Some("save put NOT done: the archive's AID is unknown".to_string());
    };
    if !bad.is_empty() {
        return Some(format!("save put NOT done (nothing written): {bad:?}"));
    }
    if files.is_empty() {
        return Some(format!("save put: nothing staged for archive {aid}"));
    }
    let root = data_dir.join(aid);
    // The slots whose state changes: a save half or a costume record names its slot; the
    // warehouse belongs to every slot (a generation carries the warehouse as it stood).
    let slot_of = |area: Area, name: &str| -> Option<u8> {
        let digits: String = name.chars().filter(|c| c.is_ascii_digit()).collect();
        (area != Area::Account && name != save_transfer::GAME_SETTINGS).then(|| digits.parse::<u8>().ok()).flatten()
    };
    let all = files.iter().any(|(a, ..)| *a == Area::Account);
    let live = root.join(FS_DIR);
    let mut kept: Vec<u8> = Vec::new();
    for slot in savevault::SLOT_MIN..=savevault::SLOT_MAX {
        if !all && !files.iter().any(|(a, n, _)| slot_of(*a, n) == Some(slot)) {
            continue;
        }
        let pair: Vec<(Half, Vec<u8>)> = Half::BOTH
            .iter()
            .filter_map(|&h| fs::read(live.join(savevault::guest_path(h, slot))).ok().filter(|b| savevault::usable_len(b.len())).map(|b| (h, b)))
            .collect();
        if pair.is_empty() {
            continue;
        }
        if let Err(e) = keep(&root, slot, now, &pair, None) {
            return Some(format!("save put NOT done (nothing written): slot {slot} could not be kept in the vault ({e})"));
        }
        kept.push(slot);
    }
    let mut done: Vec<String> = Vec::new();
    for (area, name, bytes) in &files {
        let dir = area_dir(data_dir, &root, *area);
        let written = fs::create_dir_all(&dir).and_then(|()| write_atomic(&dir.join(name), bytes));
        match written {
            Ok(()) => done.push(format!("{}/{name} ({} B)", area.dir(), bytes.len())),
            Err(e) => return Some(format!("save put STOPPED at {}/{name}: {e} — written before it: {done:?}; the vault holds slots {kept:?} as they were", area.dir())),
        }
    }
    Some(format!("save put: {done:?} written; slots {kept:?} kept in the vault first"))
}

/// ★ 의상 아바타 A3c: the marker that asks the next launch to put every avatar item the asset
/// makes into the wardrobe of the save slots it names (`1`, or `1 3` …).
pub const COSTUME_GRANT_MARKER: &str = "wie_costume_grant.txt";

/// ★ A3c (user, 2026-10-01: 「완료되면 s25 첫번째 세이브 슬롯에 만들어진 모든 아바타를 넣어줘」):
/// when the marker next to `marker_dir` is there, every item of `items_for(class)` (the catalog's
/// avatar items whose sheets the jar carries for the class the slot's save holds — ★ A4: a
/// berserker no off-hand, a knight the shield, a rogue the second dagger) that the character of
/// each named save slot does not own yet goes into that slot's costume record —
/// `avatar_state::grant_into_record`, bound to the
/// save on disk, so the next 이어하기 finds it with no save needed. OWNERSHIP ONLY: nothing is
/// worn — a character still looks only like what the wardrobe put on it (the 2026-10-01 rule
/// that removed the device-wide `wie_avatar.txt`). Called by the worker right after the jar is
/// read and before the game runs, so no guest holds the slot's character to save over it. The
/// marker is consumed FIRST, so a grant that fails cannot repeat on every launch; running it
/// again adds nothing the character already owns. Nothing a player can tap triggers it — the
/// release build is not debuggable, so this is the adb channel:
/// `adb shell "echo 1 > /sdcard/Android/data/com.hero2remake.app/files/wie_costume_grant.txt"`.
/// Returns the lines for the log (empty = no marker).
pub fn grant_costumes_on_request(data_dir: &Path, marker_dir: &Path, items_for: &dyn Fn(Option<u8>) -> Vec<(u16, u8)>) -> Vec<String> {
    let marker = marker_dir.join(COSTUME_GRANT_MARKER);
    // Gated on the file being THERE (like the export marker), never on its text reading as
    // UTF-8: a marker written by PowerShell 5.1 (`>` = UTF-16LE) must still be consumed and logged.
    if !marker.is_file() {
        return Vec::new();
    }
    let bytes = fs::read(&marker);
    if let Err(e) = fs::remove_file(&marker) {
        return vec![format!("costume grant NOT done: the marker could not be consumed ({e})")];
    }
    let bytes = match bytes {
        Ok(b) => b,
        Err(e) => return vec![format!("costume grant NOT done: the marker could not be read ({e}) — consumed")],
    };
    let Some(files) = avatar_files(data_dir) else {
        return vec!["costume grant: the app has no data root yet (the game never saved) — nothing done".to_string()];
    };
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    crate::avatar_state::grant_marker_run(&files, &bytes, savevault::SLOT_MAX, items_for, &|slot| read_live_g(data_dir, slot), seed)
}

// ── 세이브 옮기기 (2026-10-02) · 내보내기 · 불러오기 · 되돌리기 (2026-10-28) ────────────────
//
// [`crate::save_transfer`] is what a file may carry, [`crate::savebundle`] the file and the apply;
// this is where the members come from and go to on THIS phone. Export reads the live state. An
// import or a 되돌리기 is STAGED (the in-game 환경설정 rows, or 진단 정보 → 세이브 관리) and APPLIED at
// the next start, before the emulator exists — the restore's reason ([`request_restore`]): the
// running game holds its character in its heap and would write it back over the files.
//
// ★ 2026-10-28: the apply is `savebundle::apply_pending` — journaled (`.applying` + `.before`), every
// write fsynced, the phone before it kept whole (host files included) as a backup that 세이브
// 되돌리기 puts back, and a start that cannot make the disk one whole phone does not start the game
// (ChronosSword's four review fixes; `savebundle.rs` header). It replaces the 2026-10-02 importer,
// which consumed its stage before writing, rolled back from memory only, and started the game on
// whatever a failed rollback left.

/// The staged file — `savebundle::pending_path` (the 2026-10-02 path, kept).
#[cfg(test)]
fn import_path(data_dir: &Path) -> PathBuf {
    crate::savebundle::pending_path(data_dir)
}

/// Where an area's files live on this phone (`savebundle::Layout::dir`).
fn area_dir(data_dir: &Path, root: &Path, area: Area) -> PathBuf {
    crate::savebundle::Layout { data_dir: data_dir.to_path_buf(), root: root.to_path_buf() }.dir(area)
}

/// The archive root a transfer uses: `<data>/<aid>` of the archive this app runs when that is
/// known, else the one root on disk — the JNI can run before `android_main` has read the
/// archive (a process the system recreated under the file picker).
fn transfer_root(data_dir: &Path, aid: Option<&str>) -> Option<PathBuf> {
    match aid {
        Some(aid) => Some(data_dir.join(aid)),
        None => app_roots(data_dir).into_iter().find(|r| r.parent() == Some(data_dir)),
    }
}

/// This phone's save slots — the usable pairs — as the dialogs name them.
fn live_slot_lines(root: &Path) -> Vec<(u8, String)> {
    let live = root.join(FS_DIR);
    let read = |h: Half, slot: u8| fs::read(live.join(savevault::guest_path(h, slot))).ok().filter(|b| savevault::usable_len(b.len()));
    (savevault::SLOT_MIN..=savevault::SLOT_MAX)
        .filter_map(|slot| match (read(Half::Char, slot), read(Half::Prog, slot)) {
            (Some(_), Some(g)) => Some((slot, save_transfer::slot_line(slot, Some(&g)))),
            _ => None,
        })
        .collect()
}

/// A transfer file's slot as the Java dialogs name it.
fn bundle_slot_line(b: &Bundle, slot: u8) -> String {
    save_transfer::slot_line(slot, b.get(Area::Fs, &savevault::guest_path(Half::Prog, slot)))
}

/// A transfer file's slot as the GAME's box names it (`save_transfer::slot_box_line`).
pub fn bundle_slot_box_line(b: &Bundle, slot: u8) -> String {
    save_transfer::slot_box_line(slot, b.get(Area::Fs, &savevault::guest_path(Half::Prog, slot)))
}

/// The transfer file of this phone's live state, and the slots it carries. `Err` is a sentence
/// for the player.
///
/// Every owned file that exists goes, byte for byte — except a slot whose pair the game could not
/// read (torn, alone, not three / five whole blobs): that slot stays behind (it could only destroy
/// a character on the other phone), and the slots that went are returned so the dialog can say so.
/// A file that is there and cannot be read stops the export (chronos review 1 P1 — a gap would read
/// as 「this phone has no warehouse」). And the file is checked with the importer's own policy before
/// it is handed out: a phone never makes a file the other phone refuses (chronos review 3 P1).
pub fn export_bundle(data_dir: &Path, aid: Option<&str>, app: u32, created_ms: u64) -> Result<(Vec<u8>, Vec<u8>), String> {
    // ★ 2026-10-28 (code review P2): the plate store on the disk must be the one the saves beside it
    //   were made with — while its last write failed it is an older one (`config::write_quickpages`
    //   keeps the previous store whole and the worker retries). Both export paths come through here.
    if !crate::config::quickpages_write_ok() {
        log::warn!("save transfer: export refused — the last write of the plate store did not land");
        return Err(PAGES_UNSAVED.to_string());
    }
    // ★ 2026-10-28 (code review, second pass P1): the warehouse and the costume records too — the 진단 정보
    //   export does not come through the worker's wait, so the check is HERE, where both paths meet.
    let (published, edges) = (host_commits(), save_edges());
    if let Some(msg) = host_commits_refusal(published, edges) {
        log::warn!("save transfer: export refused — {msg} (published {published:?}, edges now {edges})");
        return Err(msg.to_string());
    }
    let root = transfer_root(data_dir, aid)
        .filter(|r| r.join(FS_DIR).is_dir())
        .ok_or_else(|| "이 폰에는 아직 저장한 세이브가 없습니다.".to_string())?;
    let Some(aid) = root.file_name().and_then(|n| n.to_str()).filter(|a| save_transfer::aid_ok(a)).map(str::to_string) else {
        log::warn!("save transfer: the archive root {root:?} is not a plain AID");
        return Err("앱의 게임 정보를 읽지 못했습니다.".to_string());
    };
    let layout = crate::savebundle::Layout::new(data_dir, &aid);
    let mut files = crate::savebundle::present(&layout).map_err(|e| {
        log::warn!("save transfer: cannot read the phone: {e}");
        "세이브 파일을 읽지 못했습니다.".to_string()
    })?;
    // The 진단 정보 export reads on the UI thread while the game runs: a save while it read is a mix.
    // (Only where a worker publishes — no game runs where none does.)
    if published.is_some() && save_edges() != edges {
        log::warn!("save transfer: export refused — the game saved while the files were read");
        return Err(SAVE_SETTLING.to_string());
    }
    let whole = |slot: u8| {
        let half = |h: Half| files.iter().find(|(a, n, _)| *a == Area::Fs && *n == savevault::guest_path(h, slot)).map(|(_, _, b)| b.as_slice());
        match (half(Half::Char), half(Half::Prog)) {
            (Some(n), Some(g)) => savevault::usable_len(n.len()) && savevault::usable_len(g.len()) && save_transfer::n_complete(n) && save_transfer::g_sane(g),
            _ => false,
        }
    };
    let slots: Vec<u8> = (savevault::SLOT_MIN..=savevault::SLOT_MAX).filter(|&s| whole(s)).collect();
    for slot in savevault::SLOT_MIN..=savevault::SLOT_MAX {
        let had = files.iter().any(|(a, n, _)| *a == Area::Fs && savevault::classify(n).is_some_and(|(_, s)| s == slot));
        if had && !slots.contains(&slot) {
            log::warn!("save transfer: slot {slot} is not a whole save the game can read — it stays behind");
        }
    }
    files.retain(|(a, n, _)| *a != Area::Fs || savevault::classify(n).is_none_or(|(_, s)| slots.contains(&s)));
    if slots.is_empty() {
        return Err("내보낼 세이브가 없습니다. 게임에서 먼저 저장(SAVE)해 주세요.".to_string());
    }
    let bundle = Bundle { aid, created_ms, app, backup: false, files };
    save_transfer::import_policy(&bundle)?;
    let bytes = crate::savebundle::encode(&bundle);
    // Read back with the importer before anyone sends it anywhere: a file this build writes but
    // cannot read is worse than no file.
    save_transfer::parse_import(&bytes)?;
    Ok((bytes, slots))
}

/// The export's refusal while the plate store's last write has not landed.
pub const PAGES_UNSAVED: &str = "퀵슬롯 정보를 저장하지 못해 내보내지 않았습니다. 저장 공간을 확인한 뒤 다시 눌러 주세요.";

/// [`export_bundle`] into `out`, durably. Returns the slots that went.
pub fn export_to(data_dir: &Path, aid: Option<&str>, app: u32, out: &Path) -> Result<Vec<u8>, String> {
    let (bytes, slots) = export_bundle(data_dir, aid, app, now_ms())?;
    if let Err(e) = crate::savebundle::write_atomically(out, &bytes) {
        log::warn!("save transfer: cannot write {out:?}: {e}");
        return Err("세이브 파일을 쓰지 못했습니다. 저장 공간을 확인해 주세요.".to_string());
    }
    log::warn!("save transfer: exported slots {slots:?} ({} bytes) to {out:?}", bytes.len());
    Ok(slots)
}

/// A transfer file the player picked, read with the same ceiling the parser holds.
fn read_transfer_file(path: &Path) -> Result<Vec<u8>, String> {
    let too_big = fs::metadata(path).map(|m| m.len() > save_transfer::FILE_MAX as u64).unwrap_or(false);
    if too_big {
        return Err("영웅서기2 세이브 파일이 아닙니다(파일이 너무 큽니다).".to_string());
    }
    fs::read(path).map_err(|e| {
        log::warn!("save transfer: cannot read {path:?}: {e}");
        "파일을 읽지 못했습니다.".to_string()
    })
}

/// Read and check a transfer file for THIS phone: the policy, the archive it belongs to, and the
/// app that made it — `app` is this build's versionCode (0 = unknown, which skips that check). A
/// NEWER app's file is refused: its host files may carry schemas this build would read as nothing
/// and, at the next save, write back without.
pub fn check_import(data_dir: &Path, aid: Option<&str>, app: u32, bytes: &[u8]) -> Result<Bundle, String> {
    let b = save_transfer::parse_import(bytes)?;
    if app != 0 && b.app > app {
        return Err(format!(
            "더 새 버전의 앱에서 만든 파일입니다(파일 {}, 이 폰 {}). 이 폰의 앱을 먼저 최신 버전으로 업데이트해 주세요.",
            b.app, app
        ));
    }
    let here = aid.map(str::to_string).or_else(|| transfer_root(data_dir, None).and_then(|r| r.file_name().and_then(|n| n.to_str()).map(str::to_string)));
    if let Some(here) = here
        && here != b.aid
    {
        log::warn!("save transfer: the file is archive {} and this phone is {here}", b.aid);
        return Err("다른 게임의 세이브 파일입니다.".to_string());
    }
    Ok(b)
}

/// [`check_import`] on a file on disk.
pub fn check_import_file(data_dir: &Path, aid: Option<&str>, app: u32, path: &Path) -> Result<(Vec<u8>, Bundle), String> {
    let bytes = read_transfer_file(path)?;
    let b = check_import(data_dir, aid, app, &bytes)?;
    Ok((bytes, b))
}

/// The import dialog's account of the file at `path` (`MainActivity.importPicked`): `ok`, then
/// `created|<unix ms>`, `app|<versionCode>`, one `file|<slot>` per slot it carries,
/// `extra|<what else>`, and one `here|<slot>` per save this phone holds now — what the import
/// will replace (kept as a backup that 세이브 되돌리기 puts back).
pub fn import_summary(data_dir: &Path, aid: Option<&str>, app: u32, path: &Path) -> Result<String, String> {
    let (_, b) = check_import_file(data_dir, aid, app, path)?;
    let mut out = vec!["ok".to_string(), format!("created|{}", b.created_ms), format!("app|{}", b.app)];
    for slot in b.slots() {
        out.push(format!("file|{}", bundle_slot_line(&b, slot)));
    }
    let extras = save_transfer::extras_line(&b);
    if !extras.is_empty() {
        out.push(format!("extra|{extras}"));
    }
    if let Some(root) = transfer_root(data_dir, aid.or(Some(b.aid.as_str()))) {
        out.extend(live_slot_lines(&root).into_iter().map(|(_, line)| format!("here|{line}")));
    }
    Ok(out.join("\n"))
}

/// What staging did. Either way the app closes now (the caller's next step).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Staged {
    /// Staged: the next start applies it.
    Done,
    /// The write failed and the stage could not be surely taken back — it may be applied at the
    /// next start. Play stops all the same (`savebundle::StageFailed::Unsure`): going on would let
    /// the next start apply it over what is played.
    Unsure,
}

/// Stage `bytes` (already checked) for the next start. `Err` = nothing is staged, on the disk — a
/// sentence for the player, and the game may go on. Cancels a 백업에서 복원 that was waiting: one
/// thing is put back at a start, and it is the last one asked for.
pub fn stage_bytes(data_dir: &Path, bytes: &[u8], what: &str) -> Result<Staged, String> {
    let staged = match crate::savebundle::stage(data_dir, bytes) {
        Ok(()) => Staged::Done,
        Err(crate::savebundle::StageFailed::Cancelled(why)) => {
            log::warn!("save transfer: {what} NOT staged (nothing on the disk): {why}");
            return Err("준비하지 못했습니다. 저장 공간을 확인해 주세요.".to_string());
        }
        Err(crate::savebundle::StageFailed::Unsure(why)) => {
            log::error!("save transfer: {what} may or may not be staged — the app closes: {why}");
            Staged::Unsure
        }
    };
    for root in app_roots(data_dir).into_iter().filter(|r| r.parent() == Some(data_dir)) {
        match crate::savebundle::remove_durably(&request_path(&root)) {
            Ok(()) => {}
            Err(e) => log::warn!("vault: the restore waiting in {root:?} could not be cancelled ({e}) — the start applies the {what} and drops it"),
        }
    }
    log::warn!("save transfer: {what} ({} bytes) scheduled for the next launch ({staged:?})", bytes.len());
    Ok(staged)
}

/// Schedule the import of the file at `path` for the next launch — checked again first (the file
/// the dialog showed may have been replaced since).
pub fn stage_import(data_dir: &Path, aid: Option<&str>, app: u32, path: &Path) -> Result<Staged, String> {
    let (bytes, b) = check_import_file(data_dir, aid, app, path)?;
    let staged = stage_bytes(data_dir, &bytes, "import")?;
    log::warn!("save transfer: import of slots {:?} (app {}) staged", b.slots(), b.app);
    Ok(staged)
}

/// 세이브 되돌리기: the newest backup — the phone just before the last 불러오기 / 되돌리기 — and when it
/// was kept. `Ok(None)` = there is none. `Err` = there is one and it cannot be read.
pub fn newest_backup(data_dir: &Path) -> Result<Option<(u64, Vec<u8>, Bundle)>, String> {
    let Some((ms, path)) = crate::savebundle::newest_backup(data_dir) else { return Ok(None) };
    let bytes = fs::read(&path).map_err(|e| {
        log::warn!("save transfer: cannot read the backup {path:?}: {e}");
        "되돌릴 세이브를 읽지 못했습니다.".to_string()
    })?;
    let b = crate::savebundle::parse(&bytes).map_err(|e| {
        log::warn!("save transfer: the backup {path:?} is unusable: {e:?}");
        "되돌릴 세이브가 손상됐습니다.".to_string()
    })?;
    Ok(Some((ms, bytes, b)))
}

/// Stage the newest backup, if it still is the one kept at `ms` (the one the question named).
pub fn stage_restore_newest(data_dir: &Path, ms: u64) -> Result<Staged, String> {
    match newest_backup(data_dir)? {
        Some((now, bytes, _)) if now == ms => stage_bytes(data_dir, &bytes, "restore of the newest backup"),
        _ => Err("되돌릴 세이브가 바뀌었습니다. 다시 눌러 주세요.".to_string()),
    }
}

/// What the start did with a staged file, for the first frame's dialog
/// (`MainActivity.showSaveMessage`: the first line is its title). Drained by `app.rs`.
static IMPORT_NOTICE: Mutex<Option<String>> = Mutex::new(None);

pub fn take_import_notice() -> Option<String> {
    IMPORT_NOTICE.lock().ok().and_then(|mut g| g.take())
}

/// A start that must not run the game (`savebundle::Outcome::Blocked`): the text of the dialog
/// that says so. Set by [`apply_pending_bundle`]; `app.rs` does not boot while it is set.
static BOOT_BLOCKED: Mutex<Option<String>> = Mutex::new(None);

pub fn boot_blocked() -> Option<String> {
    BOOT_BLOCKED.lock().ok().and_then(|g| g.clone())
}

/// The text of the start that does not run the game.
pub const BLOCKED_TEXT: &str = "게임을 시작하지 않습니다\n세이브를 이 폰의 저장 공간에 끝까지 쓰지 못했습니다.\n\n이대로 시작하면 두 세이브가 섞이거나, 나중에 덮어써질 수 있습니다.\n\n저장 공간을 확인한 뒤 앱을 다시 켜면 이어서 처리합니다.";

pub fn set_boot_blocked() {
    if let Ok(mut g) = BOOT_BLOCKED.lock() {
        *g = Some(BLOCKED_TEXT.to_string());
    }
}

/// The device's UTC offset in minutes, for the dates the dialogs show — Java hands it over before
/// anything else runs (`MainActivity.onCreate` → `nativeSetTimeZone`); KST until then.
static TZ_OFFSET_MIN: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(540);

pub fn set_tz_offset_min(m: i32) {
    TZ_OFFSET_MIN.store(m.clamp(-14 * 60, 14 * 60), std::sync::atomic::Ordering::Relaxed);
}

pub fn tz_offset_min() -> i32 {
    TZ_OFFSET_MIN.load(std::sync::atomic::Ordering::Relaxed)
}

/// This build's versionCode, from the same Java call (0 = unknown, which skips the newer-app check).
static APP_VERSION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_app_version(v: u32) {
    APP_VERSION.store(v, std::sync::atomic::Ordering::Relaxed);
}

pub fn app_version() -> u32 {
    APP_VERSION.load(std::sync::atomic::Ordering::Relaxed)
}

/// Apply and consume a staged file, or finish one an earlier start left unfinished. Call ONCE at
/// startup, before the emulator boots and before anything reads a save — it comes before
/// [`apply_pending_restore`] and the adb markers. Logs every step; the player's account goes to
/// [`take_import_notice`], and a start that must not run the game to [`boot_blocked`].
pub fn apply_pending_bundle(data_dir: &Path, archive_aid: Option<&str>) -> crate::savebundle::Outcome {
    let report = crate::savebundle::apply_pending(data_dir, archive_aid, 0, now_ms());
    for line in &report.log {
        log::warn!("{line}");
    }
    let (outcome, notice) = bundle_notice(&report);
    if outcome == crate::savebundle::Outcome::Blocked {
        set_boot_blocked();
    }
    if let (Some(text), Ok(mut g)) = (notice, IMPORT_NOTICE.lock()) {
        *g = Some(text);
    }
    outcome
}

/// The dialog a start's report earns (`None` = nothing to say).
fn bundle_notice(report: &crate::savebundle::Report) -> (crate::savebundle::Outcome, Option<String>) {
    use crate::savebundle::Outcome;
    let restore = report.target.as_ref().is_some_and(|t| t.backup);
    let text = match report.outcome {
        Outcome::Idle | Outcome::Blocked => None,
        Outcome::Applied => {
            let t = report.target.as_ref();
            let mut text = if restore {
                format!(
                    "세이브 되돌리기 완료\n{} 상태로 되돌렸습니다.\n",
                    t.map(|b| crate::savebundle::short_stamp(b.created_ms, tz_offset_min())).unwrap_or_default()
                )
            } else {
                String::from("세이브 불러오기 완료\n다른 폰에서 내보낸 세이브를 적용했습니다.\n")
            };
            for slot in t.map(Bundle::slots).unwrap_or_default() {
                text.push('\n');
                text.push_str(&bundle_slot_line(t.expect("slots come from the target"), slot));
            }
            text.push_str("\n\n이 폰에 있던 세이브는 백업했습니다.\n되돌리려면: 환경설정 → 세이브되돌리기");
            Some(text)
        }
        Outcome::NotApplied => Some(format!(
            "{}\n적용하지 못했습니다. 이 폰의 세이브는 그대로입니다.",
            if restore { "세이브 되돌리기 실패" } else { "세이브 불러오기 실패" }
        )),
    };
    (report.outcome, text)
}

/// The start applied (or gave up on) a staged file: a 백업에서 복원 still waiting was asked for
/// BEFORE it (staging cancels the restore), so it is dropped, not run over what was just applied.
pub fn drop_restore_request(data_dir: &Path) {
    for root in app_roots(data_dir) {
        if fs::remove_file(request_path(&root)).is_ok() {
            log::warn!("vault: a restore request older than the applied file was dropped ({root:?})");
        }
    }
}

// ★ 의상 아바타 A2 (2026-09-30): the vault's half of the costume record — a generation carries
// it, a restore puts it back (or removes the live one), the journal is written once per save.
// These touch the process-wide statics (`AVATAR_WORKING`, the edge counters), so they run one
// at a time. Run on the phone like the rest of this crate's tests (CLAUDE.md 「셀프테스트」).
#[cfg(test)]
mod avatar_tests {
    use super::*;
    use crate::avatar_state::{AvatarFiles, AvatarState, decode_record, fnv1a64};

    static SERIAL: Mutex<()> = Mutex::new(());

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("h2_vault_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_generation_carries_the_costume_record_and_a_restore_puts_it_back() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tmp("gen");
        fs::create_dir_all(root.join(FS_DIR)).unwrap();
        fs::write(root.join(FS_DIR).join("n2"), vec![1u8; 1352]).unwrap();
        fs::write(root.join(FS_DIR).join("g2"), vec![2u8; 889]).unwrap();
        let files = AvatarFiles::new(root.join(AVATAR_DIR));
        let mut saved = AvatarState::fresh(9);
        saved.grant(200, crate::avatar_state::SLOT_ARMOUR, 0, 0);
        files.write_committed(2, 0xabc, &saved).unwrap();
        let pair = vec![(Half::Char, vec![1u8; 1352]), (Half::Prog, vec![2u8; 889])];
        keep(&root, 2, 1000, &pair, None).unwrap();
        let generation = slot_dir(&root, 2).join("1000");
        assert_eq!(fs::read(generation.join(AVATAR_GEN_NAME)).unwrap(), fs::read(files.committed_path(2)).unwrap(), "the record rides with its pair");
        // Play on: a later record and a journal — then restore that generation.
        files.write_committed(2, 0xdef, &AvatarState::fresh(10)).unwrap();
        files.write_pending(2, 0xdef, &AvatarState::fresh(10)).unwrap();
        let entry = Entry { slot: 2, unix_ms: 1000, char_len: 1352, root: root.clone() };
        restore(&entry).unwrap();
        let back = decode_record(&fs::read_to_string(files.committed_path(2)).unwrap()).unwrap();
        assert_eq!((back.key, back.state), (0xabc, saved), "the generation's own record is back");
        assert!(!files.pending_path(2).exists(), "the journal of a save that is no longer live is gone");
        // A generation from before costumes: the live record goes, so the restored save loads empty.
        fs::remove_file(generation.join(AVATAR_GEN_NAME)).unwrap();
        restore(&entry).unwrap();
        assert!(!files.committed_path(2).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_journal_is_written_once_per_save_at_the_first_g_edge() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let base = tmp("journal");
        let aid = "AIDTEST";
        let root = base.join(aid);
        fs::create_dir_all(root.join(FS_DIR)).unwrap();
        let old = vec![7u8; 100];
        fs::write(root.join(FS_DIR).join("g3"), &old).unwrap();
        let files = AvatarFiles::new(root.join(AVATAR_DIR));
        let mut s = AvatarState::fresh(1);
        s.grant(200, crate::avatar_state::SLOT_HEAD, 0, 0);
        publish_avatar_working(Some(&s));
        let e0 = g_edges(3);
        before_overwrite(&base, aid, "n3");
        assert_eq!(g_edges(3), e0, "the bags saved alone are not a save the record binds to");
        assert!(!files.pending_path(3).exists());
        before_overwrite(&base, aid, "g3");
        assert_eq!(g_edges(3), e0 + 1);
        let j = decode_record(&fs::read_to_string(files.pending_path(3)).unwrap()).unwrap();
        assert_eq!((j.key, j.state), (fnv1a64(&old), s.clone()), "keyed by the /g being replaced");
        // The save's next /g edges do not rewrite it…
        fs::write(files.pending_path(3), "marker").unwrap();
        before_overwrite(&base, aid, "g3");
        assert_eq!(fs::read_to_string(files.pending_path(3)).unwrap(), "marker");
        // …the commit re-arms it for the next save…
        rearm_avatar_journal(3);
        before_overwrite(&base, aid, "g3");
        assert_ne!(fs::read_to_string(files.pending_path(3)).unwrap(), "marker");
        // …and nobody in the world means nothing to journal.
        publish_avatar_working(None);
        files.remove_pending(3);
        before_overwrite(&base, aid, "g3");
        assert!(!files.pending_path(3).exists());
        let _ = fs::remove_dir_all(&base);
    }

    /// ★ A3c: the adb marker puts the asset's items into the named slots' records — once, and
    /// only what the character lacks; the marker is gone whatever happens.
    #[test]
    fn the_grant_marker_fills_the_named_slots_wardrobes_once() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let base = tmp("grant");
        let root = base.join("AIDGRANT");
        fs::create_dir_all(root.join(FS_DIR)).unwrap();
        let g1 = vec![5u8; 64];
        fs::write(root.join(FS_DIR).join("g1"), &g1).unwrap();
        let marker_dir = base.join("sdcard");
        fs::create_dir_all(&marker_dir).unwrap();
        // ★ A4: the items for the slot's class — this `/g` is not a real save, so "any class".
        let items = |_: Option<u8>| vec![(200u16, 0u8), (200, 1), (200, 2), (200, 3)];
        assert!(grant_costumes_on_request(&base, &marker_dir, &items).is_empty(), "no marker, nothing");
        fs::write(marker_dir.join(COSTUME_GRANT_MARKER), "1, 3 9\n").unwrap();
        let lines = grant_costumes_on_request(&base, &marker_dir, &items);
        assert!(!marker_dir.join(COSTUME_GRANT_MARKER).exists(), "consumed");
        assert_eq!(lines.len(), 2, "slot 1, slot 3 (9 is not a slot): {lines:?}");
        assert!(lines[0].contains("slot 1") && lines[0].contains("gained 4"), "{lines:?}");
        assert!(lines[1].contains("no save"), "slot 3 has no /g3: {lines:?}");
        let files = AvatarFiles::new(root.join(AVATAR_DIR));
        let rec = decode_record(&fs::read_to_string(files.committed_path(1)).unwrap()).unwrap();
        assert_eq!(rec.key, fnv1a64(&g1));
        assert_eq!((rec.state.items.len(), rec.state.worn_avatars()), (4, [None; crate::avatar_state::SLOTS]), "owned, not worn");
        fs::write(marker_dir.join(COSTUME_GRANT_MARKER), "\u{feff}1\r\n").unwrap();
        let again = grant_costumes_on_request(&base, &marker_dir, &items);
        assert!(again[0].contains("slot 1") && again[0].contains("gained 0"), "a BOM and CRLF from a Windows editor still name slot 1 — and nothing is given twice: {again:?}");
        // PowerShell 5.1's `"1" > file` = UTF-16LE with a BOM: read, consumed, slot 1.
        fs::write(marker_dir.join(COSTUME_GRANT_MARKER), [0xff, 0xfe, b'1', 0, b'\r', 0, b'\n', 0]).unwrap();
        let utf16 = grant_costumes_on_request(&base, &marker_dir, &items);
        assert!(!marker_dir.join(COSTUME_GRANT_MARKER).exists() && utf16[0].contains("slot 1"), "{utf16:?}");
        fs::write(marker_dir.join(COSTUME_GRANT_MARKER), "one").unwrap();
        assert!(grant_costumes_on_request(&base, &marker_dir, &items)[0].contains("names no save slot"));
        assert!(!marker_dir.join(COSTUME_GRANT_MARKER).exists());
        let _ = fs::remove_dir_all(&base);
    }
}

// ★ 세이브 옮기기 (2026-10-02): a phone's state out to a file and into another phone. Each test
// gets its own data dirs and none touches the process-wide statics (the notice is returned, not
// published), so they run in parallel with the rest. Run on the phone (CLAUDE.md 「셀프테스트」).
#[cfg(test)]
mod host_commits_tests {
    use super::*;

    /// Code review 2026-10-28, second pass P1: the export that does not come through the worker
    /// (진단 정보 → 세이브 관리) refuses a disk whose host files did not land — and one saved since the
    /// worker last looked.
    #[test]
    fn the_export_refuses_what_the_worker_has_not_settled() {
        assert_eq!(host_commits_refusal(None, 7), None, "no worker published: no save can be owed");
        assert_eq!(host_commits_refusal(Some((HostCommits::Settled, 7)), 7), None);
        assert_eq!(host_commits_refusal(Some((HostCommits::Pending, 7)), 7), Some(SAVE_SETTLING));
        assert_eq!(host_commits_refusal(Some((HostCommits::AccountFailed, 7)), 7), Some(ACCOUNT_UNSAVED));
        assert_eq!(host_commits_refusal(Some((HostCommits::AvatarFailed, 7)), 7), Some(AVATAR_UNSAVED));
        // A save since the worker looked: its warehouse and record are owed, whatever was settled.
        assert_eq!(host_commits_refusal(Some((HostCommits::Settled, 7)), 9), Some(SAVE_SETTLING));
        assert_eq!(host_commits_refusal(Some((HostCommits::AccountFailed, 7)), 9), Some(SAVE_SETTLING));
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::*;
    use crate::save_transfer::tests::{fake_g, fake_n};

    const AID: &str = "01035D0B";

    /// A data dir of its own — one "phone".
    /// ★ 2026-10-03 the adb 「save put」: only the staged files are written — the slot's other half,
    /// the warehouse, the costume record and the host files stay as they were — after the slot's
    /// live pair went into the vault; the staging tree is consumed; a second launch does nothing;
    /// one name outside the whitelist or a torn half refuses the whole request, untouched.
    #[test]
    fn a_save_put_replaces_only_the_staged_files_and_keeps_the_slot_first() {
        let b = phone("put");
        let ext = b.join("ext");
        let root = b.join(AID);
        put_file(&root.join("fs/n1"), &[1u8; 1352]);
        put_file(&root.join("fs/g1"), &fake_g(1, 48, 2));
        put_file(&root.join("fs/n2"), &[2u8; 1352]);
        put_file(&root.join("fs/g2"), &fake_g(2, 26, 0));
        put_file(&root.join("fs/cf"), b"2cfcfcfcfc");
        put_file(&root.join("account/bag"), &[9u8; 83]);
        put_file(&root.join("avatar/1"), b"slot 1's costume record");
        put_file(&b.join("wie_quickpages.txt"), b"own 1 1\npage 0 1 2 - - - - -\n");
        put_file(&b.join("wie_difficulty.txt"), b"1");
        assert!(put_live_at(&b, &ext, Some(AID), 1_000).is_none(), "nothing staged, nothing done");

        let staged = ext.join(PUT_LIVE_DIR).join(AID);
        put_file(&staged.join("fs/n1"), &[7u8; 1500]);
        let line = put_live_at(&b, &ext, Some(AID), 2_000).unwrap();
        assert!(line.starts_with("save put: [\"fs/n1 (1500 B)\"] written; slots [1] kept"), "{line}");
        assert_eq!(fs::read(root.join("fs/n1")).unwrap(), vec![7u8; 1500], "the staged bags");
        assert_eq!(fs::read(root.join("fs/g1")).unwrap(), fake_g(1, 48, 2), "the character untouched");
        assert_eq!(fs::read(root.join("fs/n2")).unwrap(), vec![2u8; 1352], "another slot untouched");
        assert_eq!(fs::read(root.join("account/bag")).unwrap(), vec![9u8; 83]);
        assert_eq!(fs::read(root.join("avatar/1")).unwrap(), b"slot 1's costume record");
        assert_eq!(fs::read(b.join("wie_quickpages.txt")).unwrap(), b"own 1 1\npage 0 1 2 - - - - -\n", "host files stay");
        assert_eq!(fs::read(b.join("wie_difficulty.txt")).unwrap(), b"1");
        assert!(!ext.join(PUT_LIVE_DIR).exists(), "consumed");
        assert!(put_live_at(&b, &ext, Some(AID), 3_000).is_none(), "applied once");
        // The pair as it was is in the vault (slot 1 only).
        let kept = slot_dir(&root, 1).join("2000");
        assert_eq!(fs::read(kept.join("n")).unwrap(), vec![1u8; 1352]);
        assert_eq!(fs::read(kept.join("g")).unwrap(), fake_g(1, 48, 2));
        assert!(!slot_dir(&root, 2).exists(), "slot 2 was not touched, so not kept");

        // A name outside the whitelist, or a torn half: nothing is written, the tree is consumed.
        for (rel, bytes) in [("fs/evil.sh", vec![1u8; 10]), ("fs/n2", vec![1u8; 3]), ("host/wie_quickpages.txt", vec![1u8; 10])] {
            put_file(&staged.join("fs/n1"), &[8u8; 1500]);
            put_file(&staged.join(rel), &bytes);
            let line = put_live_at(&b, &ext, Some(AID), 4_000);
            if rel.starts_with("host/") {
                // An area this never reads: its file is ignored, the valid one still applies.
                assert!(line.unwrap().starts_with("save put: [\"fs/n1 (1500 B)\"]"), "{rel}");
                assert_eq!(fs::read(b.join("wie_quickpages.txt")).unwrap(), b"own 1 1\npage 0 1 2 - - - - -\n", "no host file is ever put");
                put_file(&root.join("fs/n1"), &[7u8; 1500]);
            } else {
                assert!(line.unwrap().starts_with("save put NOT done (nothing written)"), "{rel}");
                assert_eq!(fs::read(root.join("fs/n1")).unwrap(), vec![7u8; 1500], "{rel}: untouched");
            }
            assert!(!ext.join(PUT_LIVE_DIR).exists(), "{rel}: consumed");
        }
        // Another archive's tree, or no known archive: nothing written.
        put_file(&ext.join(PUT_LIVE_DIR).join("OTHER").join("fs/n1"), &[5u8; 1500]);
        assert!(put_live_at(&b, &ext, Some(AID), 5_000).unwrap().starts_with("save put: nothing staged"));
        put_file(&staged.join("fs/n1"), &[5u8; 1500]);
        assert!(put_live_at(&b, &ext, None, 6_000).unwrap().starts_with("save put NOT done: the archive's AID is unknown"));
        assert_eq!(fs::read(root.join("fs/n1")).unwrap(), vec![7u8; 1500]);
        let _ = fs::remove_dir_all(&b);
    }

    fn phone(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("h2_xfer_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn put_file(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn the_areas_are_the_vaults_own_directories() {
        assert_eq!((Area::Fs.dir(), Area::Account.dir(), Area::Avatar.dir()), (FS_DIR, ACCOUNT_DIR, AVATAR_DIR));
        assert_eq!(save_transfer::ACCOUNT_FILES, [ACCOUNT_MAIN, ACCOUNT_MATERIALS]);
        assert_eq!((crate::savebundle::SLOT_MIN, crate::savebundle::SLOT_MAX), (savevault::SLOT_MIN, savevault::SLOT_MAX));
        for slot in savevault::SLOT_MIN..=savevault::SLOT_MAX {
            for h in Half::BOTH {
                assert_eq!(crate::savebundle::save_name(h == Half::Prog, slot), savevault::guest_path(h, slot));
            }
        }
        let files = crate::avatar_state::AvatarFiles::new(PathBuf::from("x"));
        let avatar = save_transfer::names(Area::Avatar);
        for slot in savevault::SLOT_MIN..=savevault::SLOT_MAX {
            for p in [files.committed_path(slot), files.pending_path(slot)] {
                assert!(avatar.iter().any(|n| p.file_name().unwrap().to_str() == Some(n.as_str())), "{p:?} travels");
            }
        }
    }

    /// One phone's whole state, as the tests compare them: every owned path → its bytes.
    fn state(data: &Path) -> Vec<(Area, String, Vec<u8>)> {
        crate::savebundle::present(&crate::savebundle::Layout::new(data, AID)).unwrap()
    }

    /// A start, as `android_main` runs it — the report and the dialog it earns.
    fn start(data: &Path) -> (crate::savebundle::Outcome, Option<String>) {
        let report = crate::savebundle::apply_pending(data, Some(AID), 0, now_ms());
        bundle_notice(&report)
    }

    fn phone_a(tag: &str) -> PathBuf {
        let a = phone(tag);
        let ra = a.join(AID);
        put_file(&ra.join("fs/n1"), &fake_n());
        put_file(&ra.join("fs/g1"), &fake_g(1, 48, 2));
        put_file(&ra.join("fs/cf"), b"\x3f\x32cfcfcfcf");
        put_file(&ra.join("account/bag"), &[9u8; 83]);
        put_file(&ra.join("account/materials"), &[0u8]);
        put_file(&a.join("wie_quickpages.txt"), b"own 1 1\npage 0 1 2 - - - - -\n");
        put_file(&a.join("wie_difficulty.txt"), b"1");
        put_file(&a.join("wie_layout.txt"), b"this phone's screen");
        a
    }

    #[test]
    fn a_phone_moves_onto_a_fresh_phone_byte_for_byte() {
        let (a, b) = (phone_a("fresh_a"), phone("fresh_b"));
        let (bytes, slots) = export_bundle(&a, Some(AID), 67, 1_000).unwrap();
        assert_eq!(slots, vec![1]);
        let file = a.join("sent.zip");
        fs::write(&file, &bytes).unwrap();
        let summary = import_summary(&b, None, 67, &file).unwrap();
        assert!(summary.starts_with("ok\ncreated|1000\napp|67\nfile|슬롯 1 · 나이트 Lv 48 · HELL\n"), "{summary}");
        assert!(summary.contains("extra|계정창고 · 퀵슬롯 · 게임 설정") && !summary.contains("here|"), "{summary}");

        assert_eq!(stage_import(&b, None, 67, &file), Ok(Staged::Done));
        let (outcome, notice) = start(&b);
        assert_eq!(outcome, crate::savebundle::Outcome::Applied);
        let notice = notice.unwrap();
        assert!(notice.starts_with("세이브 불러오기 완료\n") && notice.contains("\n슬롯 1 · 나이트 Lv 48 · HELL"), "{notice}");
        assert_eq!(state(&b), state(&a), "the phone IS the other phone's state");
        assert!(!b.join(AID).join("fs/cf").exists(), "cf never travels — the receiving phone keeps (or ships) its own");
        assert!(!b.join("wie_layout.txt").exists(), "the device's own settings stay behind");
        assert!(!import_path(&b).exists(), "consumed");
        assert_eq!(start(&b).0, crate::savebundle::Outcome::Idle, "applied once");
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn an_import_keeps_the_phone_before_and_undo_puts_back_all_of_it() {
        let (a, b) = (phone_a("undo_a"), phone("undo_b"));
        let rb = b.join(AID);
        // Phone B played on its own: slots 1 and 3, a warehouse, a record, plates, rules, its settings file.
        put_file(&rb.join("fs/n1"), &fake_n());
        put_file(&rb.join("fs/g1"), &fake_g(0, 3, 0));
        put_file(&rb.join("fs/n3"), &fake_n());
        put_file(&rb.join("fs/g3"), &fake_g(0, 9, 0));
        put_file(&rb.join("fs/cf"), b"B settings");
        put_file(&rb.join("account/bag"), b"B's warehouse");
        put_file(&rb.join("account/materials"), b"B");
        put_file(&rb.join("avatar/1.pending"), b"B slot 1 journal");
        put_file(&b.join("wie_quickpages.txt"), b"own 3 0\n");
        put_file(&b.join("wie_difficulty.txt"), b"2");
        let mine = state(&b);

        let file = a.join("sent.zip");
        fs::write(&file, export_bundle(&a, Some(AID), 67, 1_000).unwrap().0).unwrap();
        let summary = import_summary(&b, Some(AID), 67, &file).unwrap();
        assert!(summary.contains("\nhere|슬롯 1 · 버서커 Lv 3\nhere|슬롯 3 · 버서커 Lv 9"), "the dialog names what is replaced: {summary}");
        stage_import(&b, Some(AID), 67, &file).unwrap();
        assert_eq!(start(&b).0, crate::savebundle::Outcome::Applied);
        assert_eq!(state(&b), state(&a));
        assert_eq!(fs::read(rb.join("fs/cf")).unwrap(), b"B settings", "B's own cf stays");

        // 세이브 되돌리기: the newest backup is B as it was — host files and journal included.
        let (ms, _, kept) = newest_backup(&b).unwrap().unwrap();
        assert!(kept.backup);
        assert_eq!(stage_restore_newest(&b, ms + 1), Err("되돌릴 세이브가 바뀌었습니다. 다시 눌러 주세요.".to_string()));
        assert_eq!(stage_restore_newest(&b, ms), Ok(Staged::Done));
        let (outcome, notice) = start(&b);
        assert_eq!(outcome, crate::savebundle::Outcome::Applied);
        assert!(notice.unwrap().starts_with("세이브 되돌리기 완료\n"));
        assert_eq!(state(&b), mine, "every file of B's, back");
        // …and once more: back to A's (the round trip).
        let (ms, _, _) = newest_backup(&b).unwrap().unwrap();
        stage_restore_newest(&b, ms).unwrap();
        start(&b);
        assert_eq!(state(&b), state(&a));
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn another_games_file_changes_nothing() {
        let (a, b) = (phone_a("other_a"), phone("other_b"));
        put_file(&b.join("OTHER/fs/n1"), &fake_n());
        put_file(&b.join("OTHER/fs/g1"), &fake_g(0, 1, 0));
        let file = a.join("sent.zip");
        fs::write(&file, export_bundle(&a, Some(AID), 67, 1_000).unwrap().0).unwrap();
        assert_eq!(import_summary(&b, Some("OTHER"), 67, &file).unwrap_err(), "다른 게임의 세이브 파일입니다.");
        assert!(stage_import(&b, Some("OTHER"), 67, &file).is_err());
        // Staged anyway (as if by an older dialog): the start refuses it, consumes it, touches nothing.
        put_file(&import_path(&b), &fs::read(&file).unwrap());
        let report = crate::savebundle::apply_pending(&b, Some("OTHER"), 0, 2_000);
        let (outcome, notice) = bundle_notice(&report);
        assert_eq!(outcome, crate::savebundle::Outcome::NotApplied);
        assert!(notice.unwrap().contains("이 폰의 세이브는 그대로입니다"));
        assert!(!import_path(&b).exists() && !b.join(AID).exists());
        assert_eq!(fs::read(b.join("OTHER/fs/n1")).unwrap(), fake_n());
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn a_newer_apps_file_waits_for_an_update() {
        let (a, b) = (phone_a("newer_a"), phone("newer_b"));
        let file = a.join("sent.zip");
        fs::write(&file, export_bundle(&a, Some(AID), 70, 1_000).unwrap().0).unwrap();
        assert!(import_summary(&b, None, 67, &file).unwrap_err().contains("업데이트"));
        assert!(import_summary(&b, None, 70, &file).is_ok());
        assert!(import_summary(&b, None, 0, &file).is_ok(), "an unreadable versionCode does not block the move");
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn a_slot_the_game_could_not_read_stays_behind_and_an_unreadable_file_stops_the_export() {
        let a = phone("torn_a");
        assert_eq!(export_bundle(&a, Some(AID), 67, 1).unwrap_err(), "이 폰에는 아직 저장한 세이브가 없습니다.");
        put_file(&a.join(AID).join("fs/n2"), &[2u8; 1352]); // long enough, but not three blobs
        put_file(&a.join(AID).join("fs/g2"), &fake_g(1, 2, 0));
        put_file(&a.join(AID).join("fs/n4"), &fake_n());
        assert!(export_bundle(&a, Some(AID), 67, 1).unwrap_err().contains("내보낼 세이브가 없습니다"));
        put_file(&a.join(AID).join("fs/n1"), &fake_n());
        put_file(&a.join(AID).join("fs/g1"), &fake_g(1, 48, 0));
        let (bytes, slots) = export_bundle(&a, None, 67, 1).unwrap();
        assert_eq!(slots, vec![1], "slot 2 and the lone half of slot 4 stay behind");
        let b = save_transfer::parse_import(&bytes).unwrap();
        assert!(b.get(Area::Fs, "n2").is_none() && b.get(Area::Fs, "g2").is_none() && b.get(Area::Fs, "n4").is_none());
        // A member that is there and cannot be read is not 「absent」 (chronos review 1 P1).
        fs::create_dir_all(a.join(AID).join("account/bag")).unwrap();
        assert_eq!(export_bundle(&a, None, 67, 1).unwrap_err(), "세이브 파일을 읽지 못했습니다.");
        let _ = fs::remove_dir_all(&a);
    }

    #[test]
    fn a_restore_and_an_import_cancel_each_other() {
        let (a, b) = (phone_a("cancel_a"), phone("cancel_b"));
        let rb = b.join(AID);
        put_file(&rb.join("fs/n2"), &fake_n());
        put_file(&rb.join("fs/g2"), &fake_g(0, 5, 0));
        keep(&rb, 2, 777, &[(Half::Char, fake_n()), (Half::Prog, fake_g(0, 5, 0))], None).unwrap();
        let file = a.join("sent.zip");
        fs::write(&file, export_bundle(&a, Some(AID), 67, 1_000).unwrap().0).unwrap();

        stage_import(&b, Some(AID), 67, &file).unwrap();
        assert!(request_restore(&b, &savevault::key(2, 777)));
        assert!(!import_path(&b).exists(), "the restore asked for last wins");
        stage_import(&b, Some(AID), 67, &file).unwrap();
        assert!(!request_path(&rb).exists(), "the import asked for last wins");
        // A restore request left over beside an applied file is dropped, not run over it.
        fs::write(request_path(&rb), savevault::key(2, 777)).unwrap();
        assert_eq!(start(&b).0, crate::savebundle::Outcome::Applied);
        drop_restore_request(&b);
        assert!(!request_path(&rb).exists());
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }
}
