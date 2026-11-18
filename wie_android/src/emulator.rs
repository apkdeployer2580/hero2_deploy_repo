//! Emulator worker thread. Owns the `Emulator` + `Platform` and drives the
//! tick / redraw / key-repeat loop off the UI/JNI thread on a large stack (the
//! core-compat audit's R2 requirement). Combines wie_cli's key-repeat loop with
//! wie_web's adaptive multi-tick budget.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    thread,
    time::{Duration, Instant},
};

use winit::event_loop::EventLoopProxy;

use wie_backend::{Emulator, Event, KeyCode, Options, Platform, extract_zip};
use wie_j2me::J2MEEmulator;
use wie_ktf::KtfEmulator;
use wie_lgt::LgtEmulator;
use wie_skt::SktEmulator;

use crate::{
    InputEvent, QuickSlot, Stats, UserEvent, config, items, save,
    platform::{AndroidPlatform, AndroidScreen},
    quickpage,
};

/// Emulator worker stack. `EmulatedMemory.pages` alone is a 512 KB by-value
/// temporary during construction, plus interpreter recursion depth.
///
/// ★ 8 MB -> 64 MB (2026-08-06). This is MITIGATION, not a fix, and it is worth being
/// precise about which: five device tombstones (2026-08-05, 01:58 / 02:32 / 03:01 /
/// 03:24 / 03:57) are byte-identical in shape — `SIGSEGV / SEGV_ACCERR`, `Cause: stack
/// pointer is not in a rw map; likely due to stack overflow`, thread `wie-emu`, and a
/// backtrace of TWO addresses alternating for all 511 frames the tombstone will unwind.
/// Something in the future chain grows with elapsed play time and nothing else; three
/// in-process gauges have now RULED OUT the obvious layers rather than found it
/// (`ArmCoreThreadWrapper` nesting is 1 by construction, `ArmCore::run_function`
/// nesting tops out at 21, the executor's task count stays under 8).
///
/// A thread stack is reserved address space, not memory: on 64-bit Android the pages
/// commit only as they are touched, so the cost of the extra 56 MB is zero until the
/// bug that needs it happens. It buys roughly 8x the time to the crash — the observed
/// interval was 21-35 minutes — while [`stack_rss_kb`] measures the growth on the
/// device so the next session can name it instead of ruling things out.
const EMU_STACK: usize = 64 * 1024 * 1024;

/// Resident (touched) bytes of the mapping this thread's stack lives in, in KB.
///
/// ★ The one gauge that cannot measure the wrong layer. Every counter we can put
/// *inside* the code assumes it knows which function repeats; this asks the kernel how
/// much of the stack has ever been written, which is exactly the quantity that hits the
/// guard page. Linux never decommits stack pages, so it is a high-water mark.
///
/// `/proc/self/smaps` is not cheap to parse (tens of mappings), hence the ~30 s cadence
/// at the single call site — never inside the frame budget's hot path.
#[cfg(target_os = "android")]
fn stack_rss_kb() -> Option<u64> {
    let probe = 0u8;
    let sp = &probe as *const u8 as usize;
    let maps = std::fs::read_to_string("/proc/self/smaps").ok()?;
    let mut in_ours = false;
    for line in maps.lines() {
        if let Some((range, _)) = line.split_once(' ')
            && let Some((lo, hi)) = range.split_once('-')
            && let (Ok(lo), Ok(hi)) = (usize::from_str_radix(lo, 16), usize::from_str_radix(hi, 16))
        {
            // A header line: it either starts our mapping or ends it.
            in_ours = (lo..hi).contains(&sp);
            continue;
        }
        if in_ours && let Some(rest) = line.strip_prefix("Rss:") {
            return rest.trim().trim_end_matches(" kB").trim().parse().ok();
        }
    }
    None
}

#[cfg(not(target_os = "android"))]
fn stack_rss_kb() -> Option<u64> {
    None
}

/// Adaptive per-frame emulation budget (mirrors wie_web/src/rust/lib.rs).
const FRAME_BUDGET_MS: u128 = 24;
const TICK_FULL_MS: u128 = 6;
/// Target UI cadence; light frames sleep the remainder.
const FRAME_PACE: Duration = Duration::from_millis(16);
/// Guest key auto-repeat interval (matches wie_cli / wie_web).
const KEYREPEAT_MS: u128 = 100;

/// How often the minimap panel is re-rendered from the guest's map arrays
/// (`items::render_minimap`), in frames — ~4x/s at the 16 ms pace. The render is a
/// couple of small guest reads plus a few thousand pixels, so the cadence is about
/// not being wasteful rather than about cost.
///
/// ★ 2026-08-03: this REPLACED a ~250-line state machine that drove the guest's own
/// map window open and shut behind a frozen screen, because that was the only way to
/// make the guest repaint the buffer we used to mirror. The host now draws the same
/// picture itself, so there is no window to hide, nothing to freeze, no keys sent on
/// the player's behalf, and the panel follows the player through a door with no
/// refresh step at all. See the block comment on `items::render_minimap` for the
/// decoded guest routine and the pixel-exact verification.
const MM_RENDER_EVERY: u64 = 15;

/// How often the live PLAYER MARKER on that panel is re-read, in frames.
///
/// ★★ Separate from [`MM_RENDER_EVERY`] (2026-09-19, player: the dot must follow me in
/// real time). The picture and the dot are two different signals: the picture changes
/// only when the map does, the dot changes whenever a foot moves. One cadence cannot
/// serve both — at the picture's rate the dot arrives ~250 ms late, and at the dot's
/// rate a 25x25 grid is walked 30 times a second for nothing.
///
/// The marker poll is a few pointer hops and words (`hero2java::h2_player_marker`, plus
/// the stage's grid handle that tells a map change — see 4c in the loop), and the event
/// is only SENT when the value actually changed, so a standing player costs the reads and
/// nothing else — no event, no redraw.
const MM_MARKER_EVERY: u64 = 2;

/// How often the minimap's MONSTER squares are re-read, in frames (2026-09-29). A walk of
/// the stage's entity list — a few dozen entities, six reads each — so slower than the
/// player's dot: monsters walk at the player's pace, and 150 ms of lag on a red square
/// is not something anyone reads off a minimap. Sent only when the list changed.
const MM_MOBS_EVERY: u64 = 3;

/// ── ★ DIAGNOSTIC GATE (2026-08-04, user decision) ───────────────────────────────
///
/// 17차's "퀵바등록" feature rewrites the game's own popup label buffers in guest RAM
/// and calls a guest function to commit — see PLAY_FEEDBACK_FIXES.md §17.
///
/// ★ ON since 2026-08-04 (20차). Both reasons it was off have expired:
///
///  1. It was held back so a brand-new set of host->guest writes would not land in the
///     same build as the stack-overflow diagnostics (18차) or the 19차 save work — a
///     changed crash signature would have been unattributable. 19차 is now
///     device-verified, so there is nothing left to confuse it with.
///  2. Its ONE hard prerequisite is now met. §17 claimed the commit path was "already
///     proven by useItemEffect"; that was wrong, and 19차 proved it wrong: the very
///     first host-originated `call_guest_function` on a device PANICKED, because the
///     JVM identifies threads by executor task id and host-originated calls were never
///     `attach_thread`ed. This feature's commit IS a `call_guest_function`, so before
///     `db2c704` turning it on could only ever have crashed. That fix is in, and a
///     save round-trip through the same path is device-verified.
///
/// Gating the publish is sufficient to gate the whole feature: with no `UserEvent::Popup`
/// ever sent, the UI's `popup` stays `None`, so OK is never diverted to
/// `InputEvent::PopupConfirm` and none of the bind path can be reached.
const POPUP_QUICKBIND: bool = true;

/// Wall-clock UNIX milliseconds. Cash-shop buffs are stored as absolute stamps so a
/// "1 week" buff keeps running down while the app is closed; a monotonic clock would
/// restart at every launch and make the week unbounded.
///
/// A clock set backwards can only ever make a buff look longer, never shorter, and a
/// clock before the epoch yields 0 (= expired) rather than panicking.
/// Leaf guest PC -> sample count, filled by the profiler callback (see `Options` in
/// [`run`]) and drained by the 30 s gauge. `std::sync::Mutex` rather than a lock-free
/// structure because it is touched once per FLUSH BATCH, not per sample.
static PROFILE_HIST: std::sync::Mutex<std::collections::BTreeMap<u32, u64>> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// The `n` hottest guest addresses since the last call, as `addr:count` text, and
/// clears the histogram. Empty when the profiler is off.
fn profile_top(n: usize) -> String {
    let mut hist = match PROFILE_HIST.lock() {
        Ok(h) => h,
        Err(e) => e.into_inner(),
    };
    if hist.is_empty() {
        return String::new();
    }
    let total: u64 = hist.values().sum();
    let mut v: Vec<(u32, u64)> = hist.iter().map(|(a, c)| (*a, *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    let out = v
        .iter()
        .take(n)
        .map(|(a, c)| format!("{a:#x}={:.1}%", *c as f64 * 100.0 / total as f64))
        .collect::<Vec<_>>()
        .join(" ");
    hist.clear();
    format!(" | guest hot ({total} samples): {out}")
}

/// Take the profile histogram, leaving it empty, so the next tick's samples stand alone.
/// Paired with [`profile_merge_back`] — every take MUST be restored or the 30 s
/// aggregate silently loses everything before the take.
fn profile_take() -> std::collections::BTreeMap<u32, u64> {
    std::mem::take(&mut *PROFILE_HIST.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Summarise the samples taken since [`profile_take`] (i.e. exactly one tick's worth),
/// then fold them into `carry` and put the running total back.
///
/// `top == 0` skips the formatting, which is what every ordinary tick does — the swap
/// itself is two lock/move pairs per tick and costs nothing per SAMPLE, which is the
/// point: profiling one hitch must not change what the hitch costs.
fn profile_merge_back(carry: std::collections::BTreeMap<u32, u64>, top: usize) -> String {
    let mut hist = PROFILE_HIST.lock().unwrap_or_else(|e| e.into_inner());
    let out = if top == 0 || hist.is_empty() {
        String::new()
    } else {
        let total: u64 = hist.values().sum();
        let mut v: Vec<(u32, u64)> = hist.iter().map(|(a, c)| (*a, *c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        let hot = v
            .iter()
            .take(top)
            .map(|(a, c)| format!("{a:#x}={:.0}%", *c as f64 * 100.0 / total as f64))
            .collect::<Vec<_>>()
            .join(" ");
        format!(", hot pc ({total} samples): {hot}")
    };
    let mut merged = carry;
    for (pc, c) in hist.iter() {
        *merged.entry(*pc).or_insert(0) += *c;
    }
    *hist = merged;
    out
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A save in flight, plus the slot the current guest call's return value lands in.
///
/// One at a time by construction: a second serialise while the first buffer is still
/// unwritten would leak that buffer and could interleave two records into one file.
struct SaveState {
    job: Option<save::Job>,
    result: Arc<wie_backend::GuestCallResult>,
    /// UNIX ms when the last save STARTED — seeded to the SESSION START so the first
    /// autosave lands one interval after launch rather than immediately. `save::
    /// autosave_due` treats 0 as "not armed", so this must never be left at 0.
    last_ms: u64,
    /// A save that could not start yet, kept trying until it lands or expires:
    /// `(reason, expires_at_unix_ms)`.
    ///
    /// ★ 2026-08-07, and it is the fix for the "가끔 SAVE를 누르면 CANNOT SAVE HERE"
    /// report. A refusal has three causes, and only ONE of them deserves that toast:
    ///  1. the AREA forbids saving (`canSave` byte — boss rooms): a real "no";
    ///  2. a save is already in flight (the 5-minute autosave): try again in a moment;
    ///  3. the moment is wrong but the place is fine (a window still closing, the
    ///     selector flag, the UI's field flag up to ~80 ms stale): also try again.
    /// The old code toasted for all three, so an autosave racing the button — or a tap
    /// straight after closing the menu — read as "this place cannot save", which is
    /// exactly what the player reported. Now cause 1 answers immediately, causes 2/3
    /// arm this retry and the toast only fires if the retry window runs dry.
    ///
    /// Also carries the [`save::Reason::Item`] retries (cash-item use), whose window is
    /// minutes: the use happens with the inventory open, and the save fires once the
    /// player is back on the field.
    retry: Option<(save::Reason, u64)>,
    /// hero2's save in flight — `(reason, result slot)`.
    ///
    /// ★ A separate field rather than another `save::Job` step (2026-09-19). `save::Job`
    /// is hero3's four-call state machine: serialise, read the length out of scratch,
    /// write the record, write the slot summary. hero2 has none of those steps — it is
    /// ONE call to `ag.l()V` and the game does the rest — so modelling it as a Job would
    /// mean inventing three no-op steps and a scratch word that does not exist. The two
    /// are mutually exclusive by construction: `save_begin` takes one branch or the
    /// other, and each refuses while either is in flight.
    java: Option<(save::Reason, Arc<wie_backend::GuestCallResult>)>,
    /// ★ 2026-10-28: how the last hero2 `ag.l()V` ended (`true` = returned), for the 세이브내보내기 row
    /// that saves first (`SaveXfer`) — taken by it, `None` until a save ends.
    java_outcome: Option<bool>,
}

impl SaveState {
    fn new() -> Self {
        Self { job: None, result: Arc::new(wie_backend::GuestCallResult::new()), last_ms: now_ms(), retry: None, java: None, java_outcome: None }
    }

    /// Is any save in flight, by either mechanism?
    fn busy(&self) -> bool {
        self.job.is_some() || self.java.is_some()
    }
}

/// How long a refused MANUAL save keeps retrying before the player is told no.
/// Covers an autosave in flight (four guest calls, well under a second) and every
/// transient window/flag state with slack to spare, while still answering fast.
const MANUAL_RETRY_MS: u64 = 3000;
/// How long a cash-item save keeps trying. Minutes, not seconds: the player has to
/// close the inventory first, and using one mid-boss-room (부활서) legitimately
/// cannot save until the room is cleared — if they leave within this window the
/// consumption still gets committed, otherwise the next autosave covers it.
const ITEM_RETRY_MS: u64 = 10 * 60 * 1000;

/// A skill-plate tap being verified against guest state (2026-08-07). The plate's
/// hotkey has already been sent; the question is whether the game's own gates (idle,
/// SP cost, range) let the cast START — only then does the UI get a
/// [`UserEvent::SkillCast`] and only then does a cooldown begin. Two independent
/// signals, either one confirms: SP dropped since the tap (every real skill spends
/// some), or the combo state machine is visibly running a skill action
/// (`items::skill_cast_active`).
struct SkillWatch {
    slot: u8,
    /// SP at the moment of the tap. `None` = stats were unreadable right then; the
    /// state-machine signal alone decides.
    sp0: Option<u16>,
    /// Whether a skill action was ALREADY running at the tap. The state signal only
    /// confirms on a RISING edge: tapping plate B while plate A's animation is still
    /// playing must not read A's ongoing action as "B cast" — for that tap only an
    /// SP drop can confirm.
    active0: bool,
    deadline: Instant,
}

/// How long after the tap the guest gets to actually start the cast. The dispatcher
/// arms the cast on the very tick that consumes the key, so at the ~16 ms frame pace
/// this is dozens of polls of slack — while still short enough that an SP drop from
/// something unrelated (a monster draining it seconds later) cannot be mistaken for
/// the cast.
const SKILL_WATCH_WINDOW: Duration = Duration::from_millis(700);

/// ★ The ITEM-USE DRIVE in flight (hero2, 2026-09-18 ⑧): a plate holding 치료제 or
/// 귀환서 is used through the game's own inventory popup — the `0` key cannot reach
/// either — by walking menu → bag → record → 사용하기 and back, one verified key per
/// poll. The sequence, the checks and the clock are `hero2java::h2_use_step` (pure,
/// probe-tested); this is the worker's half: the screen hold, the input gate, the
/// writes and the keys. See [`step_use_drive`].
struct UseDrive {
    d: crate::guest_build::H2UseDrive,
    /// The screen and the input are the drive's right now: paint hold on
    /// (`platform::set_paint_hold`), the player's key-downs dropped, the HUD poll and
    /// the popup relabel paused. `false` during a HANDOFF — the game is asking the
    /// player something and the player needs all three back.
    owns: bool,
    plate: u8,
    started: Instant,
}

impl UseDrive {
    fn take(&mut self, on: bool) {
        self.owns = on;
        crate::platform::set_paint_hold(on);
    }
}

/// A tick this long is a hitch the player can feel. The game re-arms its frame timer at
/// ~20 ms on the field, so a single tick past this has already eaten a whole frame.
///
/// ── WHY A PER-TICK DETECTOR AND NOT THE 30 s GAUGE (2026-08-15, 49차) ────────────
/// Player: "다른건 다 문제없는데 크리티컬이 터질때만 살짝 느려져." The throughput gauge
/// averages over 1800 frames, which is 30 SECONDS — a critical flash lasts a few hundred
/// milliseconds, so it is diluted roughly 100:1 and every window looks the same. The
/// first hypothesis off that gauge (the `blitlut565` transform 4~8 fallback, which
/// MIGRATION_LGT.md §2.20 left open) was refuted by it too: during combat the host draws
/// 98.8 % of blits and the transform refusals run at ~5/s against ~10,600 calls/s.
///
/// So measure the thing the player is actually reporting: catch the slow TICK and print
/// what moved inside it. The one number that splits the field in half is the guest
/// instruction delta — a 40 ms tick with millions of instructions is the interpreter
/// (i.e. a pixel loop we have not hosted), the same tick with a few thousand is host-side
/// work on this thread (sound parse, allocator, a blit refusal path).
const SLOW_TICK_MS: u128 = 24;

/// Minimum spacing between hitch reports. A hitch that repeats every frame would put ~80
/// lines/s into logcat, and logcat writes come out of THIS thread's frame budget — an
/// instrument that makes the stall it is measuring is worse than none.
const HITCH_REPORT_GAP: Duration = Duration::from_millis(400);

/// Counters sampled either side of one guest tick, so a hitch can be attributed instead
/// of guessed at. All are relaxed atomic loads (~20 of them), which is nothing against a
/// tick measured in milliseconds.
#[derive(Clone, Copy, Default)]
struct TickCounters {
    instr: u64,
    svc: u64,
    frames: u64,
    calls: u64,
    hle: u64,
    px: u64,
    px_hle: u64,
    /// Blits asking for a transform the hosted loops do not implement. Counted here (and
    /// not just in `refused`) because the two blitters SHARE the refusal array, so
    /// `refused[1]` alone cannot say it was the LUT blitter that gave up.
    transformed: u64,
    refused: [u64; 5],
    plays: u64,
    parses: u64,
    allocs: u64,
}

impl TickCounters {
    fn now() -> Self {
        let (instr, svc, _) = wie_util::stats::snapshot();
        let ([calls, hle, px, px_hle], _, transform, refused) = wie_util::stats::blit_snapshot();
        let audio = wie_util::stats::audio_snapshot();
        let (allocs, ..) = wie_util::stats::heap_snapshot();
        Self {
            instr,
            svc,
            frames: wie_util::stats::frames(),
            calls,
            hle,
            px,
            px_hle,
            // [0] is the identity, which every hosted blit uses; 1..=9 are the ones that
            // still run in the guest.
            transformed: transform[1..].iter().sum(),
            refused,
            plays: audio[1],
            parses: audio[2],
            allocs,
        }
    }
}

/// One line naming what a slow tick actually did. Read it like this: `inst` in the
/// millions = the guest interpreter (find the loop and host it); `inst` small with a big
/// `parses` = the sound path; `blit` short of `calls` = the host blitter refused and the
/// guest drew it the slow way, and `refused`/`transform!=0` say why.
fn report_hitch(took: Duration, a: &TickCounters, b: &TickCounters, hot: &str) {
    let d = |x: u64, y: u64| y.saturating_sub(x);
    let refused: Vec<u64> = (0..5).map(|i| d(a.refused[i], b.refused[i])).collect();
    log::warn!(
        "emu hitch: tick {:.1} ms, {} guest frame(s) — inst {}, svc {}, blit {}/{} calls ({} px, {} host px), \
         transform!=0 {}, refused [fmt {} tf {} geom {} mem {} null {}], sound plays {} parses {}, allocs {}{hot}",
        took.as_secs_f64() * 1000.0,
        d(a.frames, b.frames),
        d(a.instr, b.instr),
        d(a.svc, b.svc),
        d(a.hle, b.hle),
        d(a.calls, b.calls),
        d(a.px, b.px),
        d(a.px_hle, b.px_hle),
        d(a.transformed, b.transformed),
        refused[0],
        refused[1],
        refused[2],
        refused[3],
        refused[4],
        d(a.plays, b.plays),
        d(a.parses, b.parses),
        d(a.allocs, b.allocs),
    );
}

/// Issue one step of the chain, giving it a fresh result slot.
fn issue(emulator: &mut dyn Emulator, st: &mut SaveState, addr: u32, args: &[u32]) {
    st.result = Arc::new(wie_backend::GuestCallResult::new());
    emulator.call_guest_function_out(addr, args, st.result.clone());
}

/// Begin a save, if the guest is in a state where the game itself would allow one.
/// Returns false when it was refused (already saving, or the gate said no).
fn save_begin(emulator: &mut dyn Emulator, st: &mut SaveState, reason: save::Reason, now: u64) -> bool {
    if st.busy() {
        return false;
    }
    // ★★ hero2: one JVM call, not a chain (2026-09-19). See `guest_anchors::Hero2Save`
    // for why the shapes differ and for the headless proof that this call produces the
    // same two records the game's own SYSTEM -> 세이브 -> 예 produces.
    if let Some(sv) = crate::guest_build::hero2_save() {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        if !crate::guest_build::h2_can_save_here(&read, &sv.area) {
            // The game's own refusal, mirrored. `ag.l()V` would have written anyway.
            return false;
        }
        // ★ 2026-09-25 (부활의 구): never a character that is DOWN — dying, dead, or at 0
        //   HP. The game cannot save then (its SYSTEM menu does not open from a fall), and
        //   the host's button could: a save taken under the revive box, or by a retry that
        //   lands during the GAME OVER, would put a corpse in the slot the reload reads.
        if crate::guest_build::hero2_java().is_some_and(|h2| crate::guest_build::h2_char_down(&read, &h2)) {
            return false;
        }
        let this = JavaThis::Static { class: sv.class, field: sv.field, descriptor: sv.descriptor };
        let Some(out) = java_call(emulator, crate::guest_build::JavaCall::Save, this, &[]) else {
            return false;
        };
        st.java = Some((reason, out));
        st.last_ms = now;
        log::info!("save: started ({reason:?}) — hero2 {}.{}()", sv.class, sv.method);
        return true;
    }
    let names = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        items::save_gate(&read)
    };
    let Some((name_game, name_slot)) = names else {
        return false;
    };
    // `save_gate` returning Some implies the chain exists, but resolve it explicitly —
    // the job carries the addresses it will call.
    let Some(chain) = crate::guest_build::save_chain_fns() else {
        return false;
    };
    let Some(job) = save::Job::new(reason, chain, name_game, name_slot) else {
        return false;
    };
    // Clear the out-parameter word before the serialise step writes it, so a stale
    // length from a previous save can never be mistaken for this one's.
    if !emulator.write_memory(chain.scratch, &0u32.to_le_bytes()) {
        log::warn!("save: could not clear the scratch word at {:#x}", chain.scratch);
        return false;
    }
    let save::Next::Call { addr, args, argc, .. } = job.first_call() else {
        return false;
    };
    st.job = Some(job);
    st.last_ms = now;
    issue(emulator, st, addr, &args[..argc]);
    log::info!("save: started ({reason:?})");
    true
}

/// Advance a save in flight by one poll. Called once per frame.
/// A recovery backup in flight: one guest call, then a socket on another thread.
///
/// Separate from [`SaveState`] rather than another step in its chain, because it must
/// not be able to fail a save. It starts AFTER the save is already committed, and if
/// anything here goes wrong the character is still on disk.
struct BackupState {
    /// The guest-call result slot, while `loadSaveRecord` is running.
    pending: Option<(std::sync::Arc<wie_backend::GuestCallResult>, u8)>,
    /// Polls spent waiting, so a call that never returns cannot wedge this forever.
    waited: u32,
    data_dir: PathBuf,
}

impl BackupState {
    const TIMEOUT_POLLS: u32 = 240;

    fn new(data_dir: PathBuf) -> Self {
        Self { pending: None, waited: 0, data_dir }
    }

    /// Ask the guest to load + decrypt the current slot's record.
    ///
    /// `loadSaveRecord` (guest 0x194f18) is the same call the game's own recovery screen
    /// makes, so the 219 bytes we take from its handle are byte-identical to the ones the
    /// client would have uploaded — see `backup.rs`.
    fn begin(&mut self, emulator: &mut dyn Emulator) {
        if self.pending.is_some() {
            return;
        }
        let target = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            items::backup_target(&read)
        };
        let Some((name, slot)) = target else {
            log::info!("backup: no eligible slot to back up");
            return;
        };
        let result = std::sync::Arc::new(wie_backend::GuestCallResult::new());
        emulator.call_guest_function_out(crate::backup::FN_LOAD_SAVE_RECORD, &[name, u32::from(slot)], result.clone());
        self.pending = Some((result, slot));
        self.waited = 0;
    }

    /// One poll. Hands the body to the upload thread as soon as the call returns.
    fn poll(&mut self, emulator: &mut dyn Emulator) {
        let Some((result, slot)) = self.pending.as_ref().map(|(r, s)| (r.clone(), *s)) else {
            return;
        };
        let Some(ret) = result.poll() else {
            self.waited += 1;
            if self.waited > Self::TIMEOUT_POLLS {
                log::warn!("backup: loadSaveRecord never returned — giving up on slot {slot}");
                self.pending = None;
            }
            return;
        };
        self.pending = None;
        let Ok(handle) = ret else {
            log::warn!("backup: loadSaveRecord could not be called for slot {slot}");
            return;
        };
        let body = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::backup::body_from_handle(&read, handle)
        };
        match body {
            // 0 = the record is missing or its MD5 failed. An empty slot has nothing to
            // back up, which is a normal outcome and not a warning.
            None => log::info!("backup: slot {slot} has no usable record ({handle:#x}) — nothing uploaded"),
            Some(body) => crate::hero3_link::upload_backup(self.data_dir.clone(), slot, body),
        }
    }
}

fn save_poll(emulator: &mut dyn Emulator, st: &mut SaveState, bk: &mut BackupState, _proxy: &EventLoopProxy<UserEvent>) {
    if let Some((reason, out)) = &st.java {
        let (reason, done) = (*reason, out.poll());
        match done {
            None => return, // still running — one frame, typically
            Some(Ok(_)) => {
                st.java = None;
                st.java_outcome = Some(true);
                log::info!("save: done ({reason:?}) — hero2 ag.l()V wrote /n<slot> and /g<slot>");
                if reason == save::Reason::Manual {
                    host_notice("저장완료");
                }
                if matches!(st.retry, Some((save::Reason::Item, _))) {
                    st.retry = None;
                }
                config::clear_cash_baseline(&bk.data_dir);
                // ⛔ No `bk.begin(emulator)` here: the recovery backup reads the record
                // back through hero3's `loadSaveRecord`, which hero2 does not have. The
                // save itself is complete; only the off-device copy is unavailable, and
                // pretending otherwise would call a hero3 address on a hero2 build.
            }
            Some(Err(())) => {
                st.java = None;
                st.java_outcome = Some(false);
                log::warn!("save: hero2 ag.l()V failed");
                if reason == save::Reason::Manual {
                    host_notice("저장실패");
                }
            }
        }
        return;
    }
    let Some(mut job) = st.job else {
        return;
    };
    let result = st.result.poll();
    let scratch = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        items::save_scratch(&read).unwrap_or(0)
    };
    let next = job.advance(result, scratch);
    st.job = Some(job);
    match next {
        save::Next::Wait => {}
        save::Next::Call { addr, args, argc, .. } => issue(emulator, st, addr, &args[..argc]),
        save::Next::Done => {
            st.job = None;
            log::info!("save: done ({:?}, payload {} B, slot record {} B)", job.reason, job.len, save::SLOT_RECORD_LEN);
            if job.reason == save::Reason::Manual {
                host_notice("저장완료");
            }
            // ANY completed save serialised the current state, so a queued cash-item
            // save is now redundant. A queued MANUAL retry deliberately stays: its
            // serialise may predate the tap, and the player was promised a toast.
            if matches!(st.retry, Some((save::Reason::Item, _))) {
                st.retry = None;
            }
            // …and by the same argument the rollback note is spent: the record just
            // written to disk carries the guest's stack decrement, so host and guest
            // now agree and there is nothing left to undo. ANY reason counts — an
            // autosave commits a cash-item use just as well as the armed retry does.
            config::clear_cash_baseline(&bk.data_dir);
            // ★ A fresh save is exactly when a recovery backup is worth taking, and it
            // is the answer to "네트워크 창고에 접속하지 않아도 백업이 올라가야한다"
            // (player, 2026-08-06): the player never has to go online, and the backup on
            // the server is never older than their last save. Doing it HERE rather than
            // at 초기화 is deliberate — by the time the slot is being wiped the record is
            // about to stop existing, and a backup that races a delete is the one that
            // loses. This one is already safe before the wipe is even offered.
            bk.begin(emulator);
        }
        save::Next::Failed(why) => {
            st.job = None;
            log::warn!("save: aborted at {:?} — {why}", job.step);
            if job.reason == save::Reason::Manual {
                host_notice("저장실패");
            }
        }
    }
}

/// Raise the game's OWN "연결이 끊어졌습니다" box after the host has dropped a connection
/// Android killed while the app was in the background.
///
/// ── WHY A GUEST POKE AND NOT A MESSAGE (2026-08-15, 48차) ────────────────────────
/// The player asked for one thing: "백그라운드에서 올라오는 순간에 바로 뜨면서 네트워크
/// 접속끊기고 메인메뉴로 나가져야 해." The toast above is the first half. The second half —
/// getting OFF the network screen — is what 47차 could not do, and the device proved why
/// (PLAY_FEEDBACK_FIXES.md §47차-D): the guest tears the socket down on its own and keeps
/// its loading modal, a synthetic reply frame only opens a box on some screens, and
/// closing that box changes nothing.
///
/// The game has the whole feature already: modal 5 draws the box AND its OK key runs
/// `netTeardown` and puts the player back on the online module's main screen. It is
/// unreachable here because the shipped trigger is the WIPI *pause* callback, which
/// Android backgrounding never calls, and its `sessionActive` precondition is cleared by
/// the guest's own teardown seconds earlier. So the host writes what the game's
/// `showModal` writes — see `guest_anchors::NetDisconnectModal` for the disassembly.
///
/// ★ Gated on [`items::shell_state`], the app-state word this crate already reads: those
/// modal tables belong to the ONLINE module (state 3 — title, slot select, single and
/// network warehouse, cash shop), and it is the same word `state_off()` names. Poking
/// the modal while the player is out in the field would set a box nothing is drawing,
/// which would then appear at some unrelated later moment — and its OK would yank them
/// to the online main screen from wherever they had got to. In the field there is no
/// stuck screen to escape anyway; the toast is the whole story there.
fn show_disconnect_modal(emulator: &mut dyn Emulator) {
    let Some(modal) = crate::guest_build::net_disconnect_modal() else {
        log::warn!(
            "net: no disconnect modal located for the {} build — the toast is all the player gets",
            crate::guest_build::name()
        );
        return;
    };
    let on_shell = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        items::shell_state(&read)
    };
    if !on_shell {
        log::info!("net: connection dropped off the online screens — toast only, nothing to escape from");
        return;
    }
    let mut ok = true;
    for (addr, bytes) in modal.writes(crate::guest_build::NET_DISCONNECT_TEXT_EUCKR) {
        ok &= emulator.write_memory(addr, &bytes);
    }
    log::warn!(
        "net: raised the game's own disconnect modal (state {} at {:#x}, writes ok = {ok}) — OK now runs its teardown and returns to the main screen",
        modal.disconnect_state,
        modal.state_word
    );
}

/// Re-press the walk direction the player is still holding, after something else took
/// the guest's keydown edge.
///
/// ★★ 2026-09-20, user: *"조이스틱 이동 중(손 계속 안떼고 있음) 이상태에서 OK 및 스킬 사용
/// 하면 사용후에 안떼고 있는 방향으로 이동을 이어서 진행했으면 좋겠어"*.
///
/// hero2 latches its walk direction on the **keydown edge** — the same fact
/// `App::settle_joy` is built on ("release first, then press", because the guest latches
/// on keydown) and the same one the probe's odometer measured (a latch is worth ~44 px
/// whether the key is held 20 ms or 400). An action key therefore ENDS the walk: the
/// direction key is still down and the thumb is still on the stick, but the guest never
/// sees another edge. `Event::Keyrepeat` is not one, which is why the 100 ms auto-repeat
/// does not rescue it either.
///
/// This synthesises no input of its own — it re-presses the key the player's thumb is
/// already holding, which is exactly the edge the action consumed.
fn relatch_walk(emulator: &mut dyn Emulator, held: &mut HashMap<KeyCode, Instant>, drive: &Option<UseDrive>, guard: &mut WalkGuard) {
    // Same exclusion as the auto-repeat: while the item-use drive owns the screen a walk
    // key steers the bag's cursor, not the character.
    if drive.as_ref().is_some_and(|d| d.owns) {
        return;
    }
    // ⛔ And the same exclusion for the windows the PLAYER opens (2026-09-21, review).
    // The drive's reason — "a walk key steers the cursor, not the character" — is a fact
    // about the guest's state, not about who put it there, so it holds verbatim for the
    // inventory, the warehouse, the field menu, a shop and a dialogue. Without this, a
    // thumb still on DOWN turned every OK release into a synthetic DOWN: the popup that
    // had just opened moved off the row the player chose, and the record the host then
    // read and 버리기 acted on was a different one. It also walked straight past the run
    // loop's LEFT/RIGHT page interception, so on cell 0 the game wrapped to cell 27 of
    // the same page while the host's page context stayed put.
    let in_field = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        items::in_field_state(&read)
    };
    if !in_field {
        return;
    }
    // ⛔ The MOST RECENTLY pressed direction, not "whichever the hasher yields first"
    // (same review). With two held — two thumbs on touch type B, or a gamepad diagonal —
    // `keys().find(..)` picked by `HashMap` iteration order, which is randomly seeded per
    // process and per resize, so the re-latched direction could differ between two
    // otherwise identical presses, each worth ~44 px of movement the player did not ask
    // for. Newest-wins is also what the stick means: it is the direction the thumb
    // arrived at last.
    let Some(dir) = newest_dir(held) else { return };
    // ★★ 2026-09-24: NOT while the character is mid-action. `ag.b(B)V` @0x110980 takes a
    //   direction press only in `g.t` 1 (standing → walk now) or 2 (walking → re-aim);
    //   in the attack states 6/7 it returns with nothing queued. An action's keyup — and
    //   every `tap_key`, whose down and up are one statement apart — lands squarely inside
    //   that window, so the re-press was refused and the still-held stick went dead until
    //   the thumb turned (measured headless: `--keys 700:UP:60,720:OK:3` stops the walk at
    //   f=724 and a held UP never walks again). The press is worth sending only once the
    //   character stands, so it is ARMED here and delivered by `WalkGuard::poll`.
    match field_walk(emulator) {
        Some(w) if w.state != crate::guest_build::H2_WALK_STANDING => {
            guard.relatch = Some(Instant::now());
            log::info!("walk: re-press of {dir:?} deferred — the character is in state {} (not standing)", w.state);
        }
        _ => {
            emulator.handle_event(Event::Keydown(dir));
            held.insert(dir, Instant::now());
        }
    }
}

fn is_dir(code: KeyCode) -> bool {
    matches!(code, KeyCode::UP | KeyCode::DOWN | KeyCode::LEFT | KeyCode::RIGHT)
}

/// The MOST RECENTLY pressed direction among the held keys — see the note in
/// [`relatch_walk`] on why not "whichever the hasher yields first".
fn newest_dir(held: &HashMap<KeyCode, Instant>) -> Option<KeyCode> {
    held.iter().filter(|(k, _)| is_dir(**k)).max_by_key(|(_, t)| **t).map(|(k, _)| *k)
}

/// hero2: the walk machine while the FIELD owns the screen. `None` on another build, off
/// the field (a script, a window, a load), or when the character cannot be read.
fn field_walk(emulator: &dyn Emulator) -> Option<crate::guest_build::H2Walk> {
    let h2 = crate::guest_build::hero2_java()?;
    let crate::guest_build::AppState::Absolute { addr, field, .. } = crate::guest_build::app_state()? else {
        return None;
    };
    let read = |a: u32, l: usize| emulator.read_memory(a, l);
    crate::guest_build::h2_field_walk(&read, &h2, addr, field)
}

/// ★★ 2026-09-24 — the two walk edges hero2 loses, and how the host pays them back.
///
/// Player: *"맵이동 후에 이동 시 조이스틱에서 손을 땠는데도 그방향으로 쭉 이동할떄가 있어"*.
///
/// hero2's field walks on a ONE-SLOT request machine (`Hero2Java::walk_request_slots`,
/// `HERO2_GUEST_RE.md` §8.14): a direction PRESS starts or re-aims the walk, a RELEASE
/// queues the stop, and `n.keyNotify(II)Z` looks at a release ONLY while the state word
/// is 3, the field. Every other state — a script box (4), a window (5), a stage load
/// (1) — returns before the walk check, and that release is gone for good. A load ends
/// the walk regardless (`a.r()V` → `g.r()V` puts `g.t` back to 1; measured on the
/// Sindraf exit, a held DOWN does not carry into the new map), but a script does not:
/// an arrival notice or a dialogue that opens while the character is walking freezes it
/// with `g.t == 2`, and if the finger lifts while the box is up — the natural moment —
/// the character walks off on its own the instant the box closes, with no key held and
/// nothing left to release. The host cannot make the guest look at that release; it can
/// only send it AGAIN once the guest is looking. That is `owed`: every direction release
/// the worker forwards is a debt until the character is seen NOT walking on the field.
///
/// The second edge is the mirror image, in [`relatch_walk`]: a re-press the game refuses
/// because the character is still in an action. `relatch` holds it until `g.t == 1`.
///
/// Both are re-sends of a key the player really pressed or really released; nothing here
/// invents input. Free while neither is pending (no guest read at all).
#[derive(Default)]
struct WalkGuard {
    /// A direction RELEASE the worker forwarded, and when: the guest may not have looked.
    owed: Option<(KeyCode, Instant)>,
    /// A walk re-press waiting for the character to stand (see [`relatch_walk`]).
    relatch: Option<Instant>,
}

/// How long a walk may legitimately outlive its release. The game applies the queued
/// stop at the next STEP boundary and a step is one tile (16 px at ~10 px per 50 ms
/// frame, so under ~100 ms; FAST 게임속도 shortens it). Re-sending inside that window
/// is harmless — it queues the same stop again — but would report every ordinary release
/// as a dropped one, so the guard waits this long first.
const WALK_STOP_GRACE: Duration = Duration::from_millis(250);
/// A deferred re-press that has found no standing character in this long is dropped: the
/// player has long since done something else with the stick.
const RELATCH_MAX: Duration = Duration::from_millis(1500);

impl WalkGuard {
    /// A direction went DOWN: its own release will settle the walk; the old debt is void.
    fn note_press(&mut self, code: KeyCode) {
        if is_dir(code) {
            self.owed = None;
        }
    }

    /// A direction went UP: owed until the character is seen not walking on the field.
    fn note_release(&mut self, code: KeyCode) {
        if is_dir(code) {
            self.owed = Some((code, Instant::now()));
        }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, held: &mut HashMap<KeyCode, Instant>, drive: &Option<UseDrive>) {
        if self.owed.is_none() && self.relatch.is_none() {
            return;
        }
        // Same exclusion as the auto-repeat and `relatch_walk`: with the item-use drive on
        // the screen a direction key steers the bag, not the character.
        if drive.as_ref().is_some_and(|d| d.owns) {
            return;
        }
        // `None` = the field does not own the screen (or this is not hero2): the guest is
        // not looking at walk keys, so nothing is decided this round.
        let walk = field_walk(emulator);
        let held_dir = newest_dir(held);
        if let Some(armed) = self.relatch {
            match (held_dir, walk) {
                // The thumb lifted meanwhile: there is no walk to resume.
                (None, _) => self.relatch = None,
                _ if armed.elapsed() > RELATCH_MAX => {
                    self.relatch = None;
                    log::info!("walk: deferred re-press dropped — the character did not stand again within {} ms", RELATCH_MAX.as_millis());
                }
                (Some(dir), Some(w)) if w.state == crate::guest_build::H2_WALK_STANDING => {
                    emulator.handle_event(Event::Keydown(dir));
                    held.insert(dir, Instant::now());
                    self.relatch = None;
                    log::info!("walk: {dir:?} re-pressed once the character stood again ({} ms after the action)", armed.elapsed().as_millis());
                }
                _ => {}
            }
        }
        if let Some((dir, since)) = self.owed {
            // A direction is down again: its release is the one that matters now.
            if held_dir.is_some() {
                self.owed = None;
                return;
            }
            match walk {
                None => {}
                // Standing (or in an action, which ends standing): the release took, or
                // a load ended the walk. Paid.
                Some(w) if w.state != crate::guest_build::H2_WALK_WALKING => self.owed = None,
                Some(_) if since.elapsed() >= WALK_STOP_GRACE => {
                    // Still walking this long after the release, on the field, with
                    // nothing held: the guest never saw it. Say it again — the game takes a
                    // bare release as "queue the stop" (`ag.b()V`), and a duplicate is a
                    // no-op.
                    emulator.handle_event(Event::Keyup(dir));
                    log::warn!(
                        "walk guard: {dir:?} release re-sent — the character was still walking {} ms after it (the guest looks at a release only on the field)",
                        since.elapsed().as_millis()
                    );
                    self.owed = Some((dir, Instant::now()));
                }
                Some(_) => {}
            }
        }
    }
}

/// ★★ 2026-09-24 — a stick TURN is not a release plus a press (player: *"좌측으로 이동하다
/// 우측으로 바꾸면 0.1초뒤에 우측으로 이동을하고 이미 손을 뗐는데도 우측으로 이동"*).
///
/// hero2 has no held-key table: a release is "queue a stop" (`n.keyNotify` →
/// `ag.b()V` → `ag.k = 1`) whichever key it names, and a release that arrives in the same
/// guest loop iteration as a press is DEFERRED to the next one (`ax.f` / `ax.n`,
/// `HERO2_GUEST_RE.md` §8.14). So two turns inside one 50 ms game frame — X→A, then A→B
/// — delivered as pairs became press A, release A (deferred), press B, and then the
/// deferred release A arrived last and overwrote the turn to B with a stop: the thumb
/// held B and the character stood. `App::settle_joy` bought safety from that by holding
/// every turn for 50 ms, which the player felt as the turn lag.
///
/// On the field the old direction's release is therefore never sent: the new press alone
/// re-aims the walk (`ag.b(B)V` with `g.t == 2` queues `k = 2, l = new`, last request
/// wins), and the one release the guest does get is the lift. The old key leaves the
/// worker's own `held` map silently. Anywhere else (a menu the gamepad steers, a window
/// that opened under the thumb) the turn is the plain pair it always was — the new press
/// is queued in front of the next event, the release is returned for the caller.
/// While the item-use drive owns the screen only the release half survives, as for any
/// key-up.
fn apply_turn(
    emulator: &dyn Emulator,
    held: &mut HashMap<KeyCode, Instant>,
    pending: &mut std::collections::VecDeque<InputEvent>,
    drive: &Option<UseDrive>,
    ev: InputEvent,
) -> Option<InputEvent> {
    let InputEvent::Turn { from, to } = ev else { return Some(ev) };
    if drive.as_ref().is_some_and(|d| d.owns) {
        return Some(InputEvent::Key { code: from, down: false });
    }
    if field_walk(emulator).is_some() {
        held.remove(&from);
        return Some(InputEvent::Key { code: to, down: true });
    }
    pending.push_front(InputEvent::Key { code: to, down: true });
    Some(InputEvent::Key { code: from, down: false })
}

/// ★ The reversal half of the turn lag — see [`crate::guest_build::h2_reverse_write`]:
/// a press straight back along the axis of a walk that is between two tiles flips the
/// facing now instead of at the tile edge. Called just before the press is forwarded;
/// on the field only (the helper reads `g.t`, and [`field_walk`] gates the state word).
fn reverse_walk(emulator: &mut dyn Emulator, code: KeyCode) {
    let dir = match code {
        KeyCode::UP => 1,
        KeyCode::DOWN => 2,
        KeyCode::LEFT => 3,
        KeyCode::RIGHT => 4,
        _ => return,
    };
    if field_walk(emulator).is_none() {
        return;
    }
    let Some(h2) = crate::guest_build::hero2_java() else { return };
    let at = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_reverse_write(&read, &h2, dir)
    };
    if let Some(at) = at {
        java_write(emulator, crate::guest_build::JavaWrite::CharFacing, at, &[dir]);
    }
}

/// One guest key as a complete press — and then [`relatch_walk`], because a one-shot
/// action taken while the stick is held is exactly the case that loses the walk.
fn tap_key(emulator: &mut dyn Emulator, held: &mut HashMap<KeyCode, Instant>, drive: &Option<UseDrive>, guard: &mut WalkGuard, code: KeyCode) {
    emulator.handle_event(Event::Keydown(code));
    emulator.handle_event(Event::Keyup(code));
    relatch_walk(emulator, held, drive, guard);
}

/// ★★ 2026-10-16 (code review): the 예 of the beast list's 「버리시겠습니까?」, HELD until the skill-point refund has run.
/// [`java_call`] only schedules — the backend resolves the beast's handle and invokes `bn.d()V` on a later tick — so
/// forwarding the OK at once let the game's `removeElementAt` (and the collection of the beast after it) race the refund,
/// and a refund that failed went unseen while the beast was gone. Now the OK reaches the game only once the call reports
/// success, and only if the same confirm is still up on the same beast. A failure, or a call that does not land within
/// [`BEAST_DISCARD_WAIT`], drops the OK: the beast stays, nothing is lost, and 예 again retries.
struct BeastDiscard {
    out: Arc<wie_backend::GuestCallResult>,
    beast: u32,
    idx: u32,
    since: Instant,
    /// The confirm key that was held — OK or the keypad's 5 (the game takes both as 예); the same key goes on, and
    /// its release is the one that pairs.
    key: KeyCode,
    /// The key's release already came (or the press was a tap's press + release) — forward the pair.
    released: bool,
}

const BEAST_DISCARD_WAIT: Duration = Duration::from_secs(2);

/// ★★ (code review) a key the game's confirm boxes take as 예: OK and the keypad's 5 alike (`d.a(II)Z`). Both must go
/// through [`beast_discard_gate`] — a 5 that went around it removed the beast with no refund.
fn is_confirm_key(code: KeyCode) -> bool {
    matches!(code, KeyCode::OK | KeyCode::NUM5)
}

/// A confirm key (`code`, see [`is_confirm_key`]) about to reach the game: when it is the 예 of a beast discard, ask
/// the game's own refund `bn.d()V` on that beast and HOLD the key in `discard` (→ true = taken; [`step_beast_discard`]
/// forwards it). `released` = the caller sends no release of its own (a tap's synthetic pair). Another confirm while
/// one waits is dropped.
fn beast_discard_gate(emulator: &mut dyn Emulator, discard: &mut Option<BeastDiscard>, code: KeyCode, released: bool) -> bool {
    if discard.is_some() {
        log::debug!("beast: an OK while the discard waits for its refund — dropped");
        return true;
    }
    let Some(h2) = crate::guest_build::hero2_java() else {
        return false;
    };
    let pending = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_beast_discard_pending(&read, &h2)
    };
    let Some((beast, idx)) = pending else {
        return false;
    };
    match java_call(emulator, crate::guest_build::JavaCall::BeastRefund, JavaThis::Handle(beast), &[]) {
        Some(out) => {
            log::info!("beast: discard of #{idx} ({beast:#x}) — bn.d()V asked first; the 예 waits for it");
            *discard = Some(BeastDiscard { out, beast, idx, since: Instant::now(), key: code, released });
        }
        None => log::warn!("beast: discard of #{idx} ({beast:#x}) — the refund call was refused; the 예 is dropped, the beast stays"),
    }
    true
}

/// Every worker frame: the held discard OK goes on once its refund has run (see [`BeastDiscard`]).
fn step_beast_discard(emulator: &mut dyn Emulator, discard: &mut Option<BeastDiscard>, held: &mut HashMap<KeyCode, Instant>, drive: &Option<UseDrive>, guard: &mut WalkGuard) {
    let Some(d) = discard.as_ref() else {
        return;
    };
    let d = match d.out.poll() {
        None if d.since.elapsed() < BEAST_DISCARD_WAIT => return,
        None => {
            log::warn!("beast: discard of #{} ({:#x}) — the refund did not land in {BEAST_DISCARD_WAIT:?}; the 예 is dropped, the beast stays", d.idx, d.beast);
            *discard = None;
            return;
        }
        Some(Err(())) => {
            log::warn!("beast: discard of #{} ({:#x}) — the refund failed; the 예 is dropped, the beast stays", d.idx, d.beast);
            *discard = None;
            return;
        }
        Some(Ok(_)) => match discard.take() {
            Some(d) => d,
            None => return,
        },
    };
    let still = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::hero2_java().and_then(|h2| crate::guest_build::h2_beast_discard_pending(&read, &h2))
    };
    if still.map(|(b, _)| b) != Some(d.beast) {
        log::info!("beast: discard of #{} ({:#x}) — refunded, but the confirm closed meanwhile; no 예 sent, the beast stays with its skills at 0", d.idx, d.beast);
        return;
    }
    log::info!("beast: discard of #{} ({:#x}) — refund done, the 예 ({:?}) goes on", d.idx, d.beast, d.key);
    emulator.handle_event(Event::Keydown(d.key));
    if d.released {
        emulator.handle_event(Event::Keyup(d.key));
        relatch_walk(emulator, held, drive, guard);
    } else {
        held.insert(d.key, Instant::now());
        guard.note_press(d.key);
    }
}

/// Apply one UI-thread input event to the guest, maintaining the auto-repeat map.
///
/// `picking` = the UI is currently highlighting the quickbar plates, waiting for the
/// player to choose one. It lives here rather than on the UI thread because every
/// decision that reads it also has to RE-READ guest RAM in the same breath.
#[allow(clippy::too_many_arguments)]
fn apply_input(
    emulator: &mut dyn Emulator,
    held: &mut HashMap<KeyCode, Instant>,
    picking: &mut bool,
    pages: &mut quickpage::Pages,
    window: &mut quickpage::Window,
    buffs: &mut crate::buffs::Buffs,
    saving: &mut SaveState,
    skill_watch: &mut Option<SkillWatch>,
    levelups: &mut u32,
    drive: &mut Option<UseDrive>,
    account: &mut AccountState,
    data_dir: &std::path::Path,
    proxy: &EventLoopProxy<UserEvent>,
    // ★ 2026-09-24: the walk edges the guest can lose — see [`WalkGuard`].
    guard: &mut WalkGuard,
    // ★★ 2026-10-16: a beast discard's 예, held until its refund has run — see [`BeastDiscard`].
    discard: &mut Option<BeastDiscard>,
    // The inventory paging patch is in, so a record past the 7 x 4 grid is reachable.
    paged: bool,
    ev: InputEvent,
) {
    // ★ The hidden-plate set (hero3 82차) is pushed into `pages` on the way in, here
    //   and in the poll: `activate` / `reserve_for_bind` / `set_item` / `flip_to` /
    //   `capture` all consult it, and one setter is what keeps them from drifting
    //   apart. The UI has its own gates on the same rule — these are the ones nothing
    //   can go around.
    if matches!(account.pending, Some(AccountStep::FlightConsume { .. } | AccountStep::FlightWarp { .. })) {
        return;
    }
    pages.set_hidden(crate::quick_hidden());
    match ev {
        InputEvent::QuickHidden { mask } => {
            // The editor flipped HIDE / SHOW. The live copy is already set (the UI did
            // that before relaying out); this arm is the STORE's write, since the
            // worker is the one writer of `wie_quickpages.txt`. Nothing in the guest
            // changes: a hidden plate keeps its binding and, if it was resident, its
            // seat — the next `pick_seat` evicts it like any other LRU plate.
            pages.set_hidden(mask);
            crate::config::write_quickpages(data_dir, pages);
            log::info!("quickpage: hidden mask {:#04x} persisted ({} visible)", pages.hidden(), crate::quickpage::visible_count(pages.hidden()));
        }
        InputEvent::Key { code, down: true } => {
            if !held.contains_key(&code) {
                // ★ #13 step 5: an OK with the menu alone on the ACCOUNT row is the host's
                //   — it opens the warehouse window instead of the game's card window.
                if code == KeyCode::OK
                    && let Some(h2) = crate::guest_build::hero2_java()
                    && account.intercept_ok(emulator, &h2)
                {
                    account.swallow_ok_up = true;
                    return;
                }
                // ★★ 2026-10-16 (user: *"야수스킬 찍은 후에 야수를 버려버리면 스킬포인트가 증발되는 버그"*): the 예 of the
                //   beast list's 「버리시겠습니까?」 — the game's `bz.a(BB)V` kind 10 just `removeElementAt`s the beast (then
                //   `a.e()V`); `bn.d()V`, the skill-point return, is the evolution window's call only. The host asks the
                //   game's own `bn.d()V` on that beast first (Σ `bn.k` back onto `a.d`, the levels 0 — nothing invented).
                //   ★★ (code review) and the 예 waits for it: `java_call` only schedules, so the key goes on from
                //   `step_beast_discard` once the refund reports success — never before (see [`BeastDiscard`]).
                //   ★★ (code review) the keypad's 5 is the same 예 to the game — it goes through the same gate.
                if is_confirm_key(code) && beast_discard_gate(emulator, discard, code, false) {
                    return;
                }
                // ★ An OK on a cash item's 사용하기 row starts the host-side effect. The
                //   guest state is read HERE, one statement before the key is forwarded,
                //   so it cannot be stale — the same reason 17차's PopupConfirm re-reads.
                //   We do not change the key: the GAME still runs `useItem`, whose shared
                //   tail (guest 0x154542) decrements the stack and removes the record at
                //   zero. The host only watches and supplies the effect the original
                //   client never had.
                if code == KeyCode::OK {
                    let pending = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        items::cash_use_pending(&read)
                    };
                    if let Some((cat, idx)) = pending {
                        let now = now_ms();
                        // ★ 2026-08-14 — BEFORE any host-side effect touches disk, write
                        //   down the state the last save already committed. The guest's
                        //   stack decrement will not reach disk until the retry armed at
                        //   the bottom of this block fires, and an app death in between
                        //   used to leave the effect without the cost (the player's
                        //   "저장을 안 하면 다시 사용할 수 있다"). See `buffs::CashBaseline`.
                        //
                        //   Ordering is the whole trick: a crash between this line and the
                        //   effect leaves a note EQUAL to the live state, so restoring it
                        //   is a no-op. The other order would commit the effect for free.
                        if cat == crate::buffs::CASH_CAT {
                            config::arm_cash_baseline(data_dir, buffs, *levelups);
                        }
                        if let Some(g) = crate::buffs::grant_for(cat, idx) {
                            buffs.apply(g, now);
                            config::write_buffs(data_dir, buffs);
                            log::info!(
                                "cash buff used: {g:?} -> exp x{}, relief {} levels, {} min left",
                                buffs.exp_multiplier(now),
                                buffs.relief_levels(now),
                                buffs.minutes_left(now)
                            );
                        } else if let Some(r) = crate::buffs::box_reward(cat, idx, now) {
                            // ★ Random boxes (비밀의상자 / 오브상자). `now` doubles as the
                            //   roll — one box open per millisecond is plenty of spread,
                            //   and it keeps this file free of an RNG dependency.
                            match r {
                                crate::buffs::BoxReward::Exp { mult, ms } => {
                                    buffs.apply_timed(crate::buffs::Grant::Exp(mult), now, ms);
                                    config::write_buffs(data_dir, buffs);
                                }
                                crate::buffs::BoxReward::Relief { ms } => {
                                    buffs.apply_timed(crate::buffs::Grant::LevelRelief(crate::buffs::RELIEF_LEVELS), now, ms);
                                    config::write_buffs(data_dir, buffs);
                                }
                                crate::buffs::BoxReward::FullRestore => {
                                    let writes = {
                                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                                        items::full_restore_writes(&read)
                                    };
                                    match writes {
                                        Some(ws) => {
                                            for (a, bytes) in ws {
                                                let ok = emulator.write_memory(a, &bytes);
                                                log::info!("box full restore: {a:#x} <- {bytes:02x?} (write ok = {ok})");
                                            }
                                        }
                                        None => log::warn!("box full restore skipped — no live character"),
                                    }
                                }
                            }
                            host_notice(crate::buffs::box_reward_toast(r));
                            log::info!("cash box used: (cat {cat:#x}, idx {idx:#x}) -> {r:?}");
                        } else if crate::buffs::is_socket_expand(cat, idx) {
                            // ★ 소켓확장 — the HOST no longer grants this (44차). 38차 wrote
                            //   the socket byte onto "the first equipped item with room",
                            //   which answered neither half of what the player asked for:
                            //   they want to CHOOSE the target, and to choose it out of the
                            //   network warehouse. It now runs through the game's own
                            //   오브결합 picker instead, entirely server-side — see
                            //   `buffs::IDX_SOCKET_EXPAND` and the server's
                            //   `ORB_IDX_SOCKET_EXPAND`.
                            //
                            //   This arm only exists for a copy left in someone's bag by the
                            //   old shop row: the guest's consume tail eats it regardless of
                            //   what we do, so it says where the working one lives rather
                            //   than vanishing silently.
                            log::warn!("socket expand used from the bag — inert since 44차; the warehouse copy is the real one");
                            host_notice(crate::buffs::SOCKET_EXPAND_BAG_TOAST);
                        } else if crate::buffs::is_level_elixir(cat, idx) {
                            // ★ 천상의 영약 (2026-08-13): the grant is a guest call into
                            //   the game's own checkLevelUp, which must run in the same
                            //   context the kill path runs it in — the FIELD — and the
                            //   inventory window is open right now. So the use only ARMS
                            //   a pending grant; the worker loop fires it on the next
                            //   field poll (see the levelup block by the autoloot poll).
                            //   Persisted so an app death between the two cannot eat a
                            //   consumed item.
                            if crate::guest_build::level_up_fn().is_some() {
                                *levelups = levelups.saturating_add(1);
                                config::write_pending_levelups(data_dir, *levelups);
                                log::info!("level elixir used: {} level-up(s) pending (fires on field return)", *levelups);
                            } else {
                                // The item should not be purchasable on such a build (the
                                // server lists it for everyone), so say WHY nothing happened.
                                log::warn!("level elixir used but checkLevelUp is not located on this build — no effect");
                            }
                        }
                        // ★ 2026-08-07: a CASH item use arms a save (경험의 보주, 상자,
                        // 부활서, …). The game consumes the item but the player may not
                        // save for a long while — a crash then rolls the count back,
                        // and for the host-granted buffs that is a free duplicate. The
                        // save cannot run NOW (the inventory window is open, the gate
                        // refuses), so it is armed as a long retry that fires once the
                        // player is back on a saveable field. A 부활서 popped inside a
                        // boss room simply keeps waiting — the room forbids saving,
                        // which is the accepted behaviour.
                        if cat == crate::buffs::CASH_CAT {
                            // Never DOWNGRADE a pending Manual retry to a silent one.
                            if !matches!(saving.retry, Some((save::Reason::Manual, _))) {
                                saving.retry = Some((save::Reason::Item, now + ITEM_RETRY_MS));
                                log::info!("save: armed after cash-item use (cat {cat:#x}, idx {idx:#x})");
                            }
                        }
                    }
                }
                if is_dir(code) {
                    reverse_walk(emulator, code);
                }
                if code == KeyCode::OK {
                    sync_bag_full_row(emulator);
                    bag_full_notice_on_ok(emulator);
                }
                emulator.handle_event(Event::Keydown(code));
                held.insert(code, Instant::now());
                guard.note_press(code);
            }
        }
        // `apply_turn` has already expanded every turn before this is reached.
        InputEvent::Turn { .. } => {}
        InputEvent::Key { code, down: false } => {
            if code == KeyCode::OK && std::mem::take(&mut account.swallow_ok_up) {
                return;
            }
            // ★★ 2026-10-16: the release of a discard 예 still waiting for its refund — the pair goes on together.
            if let Some(d) = discard.as_mut()
                && code == d.key
                && !d.released
            {
                d.released = true;
                return;
            }
            if held.remove(&code).is_some() {
                emulator.handle_event(Event::Keyup(code));
                // ★ 2026-09-24: a direction release is a DEBT until the character is seen
                //   standing on the field — the guest looks at it only in state 3 (`WalkGuard`).
                guard.note_release(code);
                // An OK / CLR / pad button that interrupted a walk: give the direction its
                // keydown edge back. Direction keys themselves are excluded — releasing one
                // IS the walk ending, and `App::settle_joy` owns that transition.
                if !is_dir(code) {
                    relatch_walk(emulator, held, drive, guard);
                }
            }
        }

        // ── the "퀵바등록" flow ────────────────────────────────────────────────
        //
        // ★ Every arm re-reads the popup state FIRST. The UI's copy is up to ~80 ms
        //   old, and the row it names decides which guest function runs — the skill
        //   dispatcher has no bounds check of its own (see items::popup_view), so a
        //   stale row is not a cosmetic error.
        InputEvent::PopupConfirm => {
            let view = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                items::popup_view(&read)
            };
            let Some(view) = view else {
                // The popup went away between the tap and here: hand the confirm to
                // the game unchanged, which is what the player's finger asked for.
                *picking = false;
                let _ = proxy.send_event(UserEvent::PickingSlot(false));
                // ★ …unless what replaced it is the host's own 개수 box. The UI diverts OK
                //   from its own copy of the popup, which is up to a frame old, so the
                //   press that opens the box can be followed by a `PopupConfirm` for a
                //   popup that is already gone — and a synthetic OK there would reach
                //   `aq`, whose OK the game answers by closing it (`q.a(50, n)` = close
                //   the child) with nothing moved.
                if let Some(h2) = crate::guest_build::hero2_java()
                    && account.intercept_ok(emulator, &h2)
                {
                    return;
                }
                // ★★ 2026-10-16: …nor a beast discard's 예 before its refund has run.
                if beast_discard_gate(emulator, discard, KeyCode::OK, true) {
                    return;
                }
                emulator.handle_event(Event::Keydown(KeyCode::OK));
                emulator.handle_event(Event::Keyup(KeyCode::OK));
                return;
            };
            match items::popup_row(&view, view.sel) {
                // The game's own row (사용하기 / 포인트투자 / 스킬상세정보): pass it through.
                items::PopupRow::PassThrough => {
                    if !beast_discard_gate(emulator, discard, KeyCode::OK, true) {
                        emulator.handle_event(Event::Keydown(KeyCode::OK));
                        emulator.handle_event(Event::Keyup(KeyCode::OK));
                    }
                }
                // 퀵슬롯등록. Do NOT commit yet — the row means "ask which plate".
                items::PopupRow::Bind => {
                    *picking = true;
                    let _ = proxy.send_event(UserEvent::PickingSlot(true));
                    host_notice("등록할퀵슬롯선택");
                    log::info!("popup: 퀵슬롯등록 armed (kind={}, item={}, value={})", view.kind, view.item, view.value);
                }
                // An unlearned skill: the game's bind guard would silently UNBIND
                // instead of registering, so refuse rather than offer it.
                items::PopupRow::BindRefused => log::info!("popup: 퀵슬롯등록 refused — skill not learned"),
                // ★ 퀵슬롯해제 (hero2, 2026-09-17): a HOST row. Clear the guest slot that
                //   holds this skill — one byte, the same `-1` the game's own dedup writes —
                //   drop the plate that owned it, and close the popup the way the player
                //   would (CLR = the popup's own cancel path, `parent.a(kind, -1)`).
                items::PopupRow::Unbind if view.item => {
                    // S7: an item plate lives only in the store — nothing in the guest
                    // names it, so nothing in the guest is written.
                    if pages.forget_item(view.item_kind) {
                        log::info!("popup: 퀵슬롯해제 — item type {} left its plate", view.item_kind);
                        crate::config::write_quickpages(data_dir, pages);
                        host_notice("퀵슬롯해제완료");
                    } else {
                        log::info!("popup: 퀵슬롯해제 — item type {} is on no plate", view.item_kind);
                        host_notice("등록안됨");
                    }
                    popup_close_host_row(emulator, &view);
                }
                items::PopupRow::Unbind => {
                    let write = crate::guest_build::hero2_java().and_then(|h2| {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_unbind_write(&read, &h2, view.value)
                    });
                    match write {
                        Some((addr, cur)) => {
                            if java_write(emulator, crate::guest_build::JavaWrite::Quickbar, addr, &[0xff]) {
                                log::info!("popup: 퀵슬롯해제 — quickbar byte at {addr:#x} ({cur}) cleared");
                                pages.forget_value_in(window, view.value);
                                // The skill is on no plate now; registering it again is the
                                // game's own routine, which starts its counter itself.
                                window.forget_parked(view.value);
                                host_notice("퀵슬롯해제완료");
                            }
                        }
                        None => {
                            log::info!("popup: 퀵슬롯해제 — value {} is in no guest slot", view.value);
                            host_notice("등록안됨");
                        }
                    }
                    emulator.handle_event(Event::Keydown(KeyCode::CLEAR));
                    emulator.handle_event(Event::Keyup(KeyCode::CLEAR));
                }
                // 상세보기 / 등록취소 / 버리기 (hero3): remap the display row to the guest
                // row the dispatcher expects and let the game run it.
                items::PopupRow::Remap(row) => {
                    popup_commit(emulator, &view, row);
                }
                // ★ 창고에넣기 / 가방에넣기 (#13 step 7): the host moves the record.
                items::PopupRow::Move => {
                    if let Some(h2) = crate::guest_build::hero2_java() {
                        account.start_move(emulator, &h2, view.popup);
                    }
                }
                // ★ 스탯초기화 / 스킬초기화 (#22): the host refunds and consumes the orb.
                items::PopupRow::Reset(what) => {
                    if let Some(h2) = crate::guest_build::hero2_java() {
                        account.start_reset(emulator, &h2, view.popup, what);
                    }
                }
                items::PopupRow::Flight(town) => {
                    if let Some(h2) = crate::guest_build::hero2_java() {
                        account.start_flight(emulator, &h2, view.popup, town);
                    }
                }
                items::PopupRow::None => log::info!("popup: OK on row {} of a {}-row popup dropped", view.sel, view.rows),
            }
        }
        InputEvent::PopupBindSlot { slot } => {
            *picking = false;
            let _ = proxy.send_event(UserEvent::PickingSlot(false));
            let view = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                items::popup_view(&read)
            };
            // The whole gate again: the popup may have closed, the cursor may have
            // moved to another cell, the character may have changed.
            let Some(v) = view.as_ref() else {
                log::info!("popup: bind to plate {slot} dropped — popup no longer valid");
                return;
            };
            // ★ S7: an ITEM plate. The store takes the type; the game writes nothing and
            //   is told nothing but CLR. If the plate was a RESIDENT skill, its guest
            //   slot is cleared (the same `-1` as 퀵슬롯해제) so the skill does not come
            //   back onto an empty plate at the next capture.
            if v.item {
                match pages.set_item(window, slot, v.item_kind) {
                    Some(freed) => {
                        if let Some(g) = freed {
                            let writes = crate::guest_build::hero2_java()
                                .map(|h2| {
                                    // ★ 2026-10-02: the skill leaving the slot keeps its
                                    //   cooldown (it may sit on the other page too).
                                    park_evicted(&*emulator, &h2, window, g, None);
                                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                                    crate::guest_build::h2_bind_writes(&read, &h2, g, None, -1)
                                })
                                .unwrap_or_default();
                            for (a, bytes) in &writes {
                                java_write(emulator, crate::guest_build::JavaWrite::Quickbar, *a, bytes);
                            }
                            log::info!("popup: plate {slot} gave up guest slot {g} ({} write(s))", writes.len());
                        }
                        log::info!("popup: item type {} bound to plate {slot}", v.item_kind);
                        crate::config::write_quickpages(data_dir, pages);
                        host_notice("퀵슬롯등록완료");
                    }
                    None => {
                        log::info!("popup: item bind to plate {slot} dropped — hidden or out of range");
                        host_notice(crate::HOST_SLOT_TOAST);
                    }
                }
                popup_close_host_row(emulator, v);
                return;
            }
            // ★ `slot` is a PLATE. The game binds into one of ITS slots, so the plate is
            //   given a guest seat first (`quickpage::Pages::reserve` — the plate window,
            //   S3) and the popup is pointed at THAT row. Nothing is written to the bar by
            //   the host here: the game is about to write that slot itself.
            let Some(guest) = pages.reserve_for_bind(window, slot, v.value) else {
                // Two refusals, two very different reports: an out-of-range plate is a
                // bug, a hidden one is the player's own setting (hero3 82차). Say which.
                let why = if pages.plate_hidden(slot) { "the player switched that plate off" } else { "not a plate" };
                log::info!("popup: bind to plate {slot} dropped — {why}");
                host_notice(crate::HOST_SLOT_TOAST);
                return;
            };
            match items::popup_guest_row(v, v.bind_row, guest) {
                Some(row) => {
                    log::info!("popup: binding plate {slot} through guest slot {guest} (guest row {row}, kind {}, value {})", v.kind, v.value);
                    // ★ 2026-10-02: the game is about to overwrite that slot and its counter
                    //   (its register routine starts the new skill's own) — the skill sitting
                    //   there leaves with the cooldown it has. What is parked for the new
                    //   skill stays until the capture READS it in its seat: a registration
                    //   the game refuses must not cost it its cooldown.
                    if let Some(h2) = crate::guest_build::hero2_java() {
                        park_evicted(&*emulator, &h2, window, guest, Some(v.value));
                    }
                    if popup_commit(emulator, v, row) {
                        crate::config::write_quickpages(data_dir, pages);
                        host_notice("퀵슬롯등록완료");
                    }
                }
                None => log::warn!("popup: bind to plate {slot} dropped — guest slot {guest} has no popup row"),
            }
        }
        InputEvent::PopupBindGuest { guest } => {
            let plate = window.plate_at(guest).unwrap_or(guest);
            apply_input(emulator, held, picking, pages, window, buffs, saving, skill_watch, levelups, drive, account, data_dir, proxy, guard, discard, paged, InputEvent::PopupBindSlot { slot: plate });
        }
        InputEvent::PopupCancelPick => {
            *picking = false;
            let _ = proxy.send_event(UserEvent::PickingSlot(false));
        }

        // ── quickbar page flip ────────────────────────────────────────────────
        //
        // Everything here needs guest memory, so the whole operation lives on this
        // thread and the UI only reported a tap. Order matters and is enforced by
        // `Pages::flip_to`: capture the outgoing page from a FRESH read, switch, then
        // plan the incoming one. Doing it in two steps from the UI side would lose a
        // page if the second half were dropped.
        // The player tapped SAVE. The gate and the whole four-call chain live on this
        // side because every one of their inputs is guest memory; the UI only reports
        // the tap and waits for a toast.
        InputEvent::QuickSave => {
            // ★ The control is NOT hidden and the tap is NOT short-circuited on a build
            //   whose save chain is unported (2026-08-11): removing shipped behaviour to
            //   paper over an unfinished port is the wrong trade. What the log does say
            //   is WHICH of the two situations produced a refusal, so a device report is
            //   readable — see the `save_chain_supported` note.
            if !crate::guest_build::host_save_supported() {
                log::warn!(
                    "save: {} build — neither save route is ported (hero3's chain: MIGRATION_LGT.md §4-3; hero2's ag.l()V: guest_anchors::Hero2Save), so this tap cannot complete; the game's own menu -> SYSTEM save works",
                    crate::guest_build::name()
                );
            }
            let now = now_ms();
            if save_begin(emulator, saving, save::Reason::Manual, now) {
                saving.retry = None; // whatever was queued, this save covers it
            } else {
                // ★ Which "may I save here" to ask depends on the build: hero3's reads
                //   hero3 CHAR offsets, hero2's reads `az.a()Z`'s own two fields. Asking
                //   the wrong one on hero2 would decode its Java field block as a hero3
                //   record — a confident answer about nothing.
                let (area_ok, down) = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    let area_ok = match crate::guest_build::hero2_save() {
                        Some(sv) => crate::guest_build::h2_can_save_here(&read, &sv.area),
                        None => items::can_save_here(&read),
                    };
                    (area_ok, crate::guest_build::hero2_java().is_some_and(|h2| crate::guest_build::h2_char_down(&read, &h2)))
                };
                if !saving.busy() && down {
                    // ★ A fallen character: refused outright — never retried, because the
                    //   retry window is exactly as long as a GAME OVER.
                    log::info!("save: refused — the character is down");
                    host_notice("저장불가");
                } else if !saving.busy() && !area_ok {
                    // The AREA forbids it (boss room) — the game's own answer; say so.
                    log::info!("save: refused — the game's canSave byte says no here");
                    host_notice("저장불가");
                } else {
                    // A save already in flight, or a transient state (window closing,
                    // stale field flag): retry briefly instead of lying about the place.
                    log::info!("save: busy/transient refusal — retrying for {MANUAL_RETRY_MS} ms");
                    saving.retry = Some((save::Reason::Manual, now + MANUAL_RETRY_MS));
                }
            }
        }
        InputEvent::SkillCast { slot } => {
            // One-shot hotkey, exactly what the plate used to send from the UI thread —
            // then arm the cast watch (see `SkillWatch`): only a CONFIRMED cast answers
            // with `UserEvent::SkillCast`, which is what starts the UI cooldown.
            let Some((code, _)) = crate::quickslot_hotkey(slot) else { return };
            tap_key(emulator, held, drive, guard, code);
            let (sp0, active0) = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                (items::read_stats(&read).map(|s| s.sp), items::skill_cast_active(&read))
            };
            *skill_watch = Some(SkillWatch { slot, sp0, active0, deadline: Instant::now() + SKILL_WATCH_WINDOW });
        }
        // ── a plate pressed on the FIELD (hero2 window model, S3) ──────────────
        //
        // A plate is not a guest slot. Only this thread can say which slot the plate is
        // in, or put it in one (`quickpage::Pages::activate`); a key sent from the UI
        // would press whatever else happened to be in that slot.
        InputEvent::QuickActivate { plate } => {
            let Some(h2) = crate::guest_build::hero2_java() else {
                // hero3 builds: plate == slot, the pre-window behaviour.
                if let Some((code, _)) = crate::quickslot_hotkey(plate) {
                    tap_key(emulator, held, drive, guard, code);
                }
                return;
            };
            // ★ 2026-10-02: a skill that is OFF the bar and still cooling is not seated — the
            //   game would refuse the cast (`cd > 0`), and seating it would only evict a skill
            //   the player can use. The UI swallows this tap from the ring it draws; the pad
            //   and a stale ring arrive here.
            if window.guest_of(plate).is_none()
                && let Some(v) = pages.plate(plate).value()
                && let Some((left, _)) = window.parked(v, cd_clock_now(&*emulator))
            {
                log::info!("quickpage: plate {plate} (value {v}) is off the bar with {left} tick(s) of cooldown left — not seated");
                return;
            }
            match pages.activate(window, plate) {
                quickpage::Activation::Resident { guest } => {
                    // Logged like the Bind arm: on the device this line is the only
                    // evidence a plate tap reached the guest at all.
                    log::info!("quickpage: plate {plate} resident in guest slot {guest} -> hotkey");
                    if let Some((code, _)) = crate::quickslot_hotkey(guest) {
                        tap_key(emulator, held, drive, guard, code);
                    }
                }
                quickpage::Activation::Bind { guest, value } => {
                    // ★ 2026-10-02: the evicted skill's counter is parked, and the slot
                    //   starts with THIS skill's (ready, unless the bar still had it).
                    let left = seat_cooldown(&*emulator, &h2, window, guest, Some(value));
                    let writes = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_bind_writes(&read, &h2, guest, Some(value), left)
                    };
                    if writes.is_empty() {
                        log::info!("quickpage: plate {plate} -> guest slot {guest} bind produced no writes (gate shut or bar unreadable)");
                        window.vacate(guest);
                        return;
                    }
                    let mut ok = true;
                    for (a, bytes) in &writes {
                        let what = if bytes.len() == 4 { crate::guest_build::JavaWrite::Cooldown } else { crate::guest_build::JavaWrite::Quickbar };
                        ok &= java_write(emulator, what, *a, bytes);
                    }
                    log::info!("quickpage: plate {plate} (value {value}) seated in guest slot {guest}: {} write(s), ok={ok}", writes.len());
                    if ok {
                        crate::config::write_quickpages(data_dir, pages);
                        if let Some((code, _)) = crate::quickslot_hotkey(guest) {
                            tap_key(emulator, held, drive, guard, code);
                        }
                    } else {
                        window.vacate(guest);
                    }
                }
                // S7: select the type in the bag the way `#` would (`bq.c` + `bq.a`),
                // then press `0`. The bag is walked NOW; a type the bag no longer has
                // presses nothing and says so.
                quickpage::Activation::Item { kind } => {
                    // ★★ 2026-10-17: 엘릭서 — the host's own drink, as its popup's 사용하기 (`use_elixir_plate`).
                    if kind == crate::guest_build::H2_PLATE_ELIXIR {
                        match account.use_elixir_plate(emulator, &h2) {
                            Ok(()) => log::info!("quickpage: plate {plate} — 엘릭서 drunk"),
                            Err(why) => {
                                log::info!("quickpage: plate {plate} (엘릭서) — {why}");
                                if why == AccountState::ELIXIR_NONE {
                                    host_notice("회복아이템없음");
                                }
                            }
                        }
                        return;
                    }
                    // ★★ 2026-10-21: 상점주인의 혼 — the game's shop opened on the field, as its popup's 사용하기
                    //   (`use_shop_plate`); the live save's INVENTORY must hold one (a warehouse orb is not there).
                    if kind == crate::guest_build::H2_PLATE_SHOP_ORB {
                        match account.use_shop_plate(emulator, &h2) {
                            Ok(()) => log::info!("quickpage: plate {plate} — 상점주인의 혼: the shop opens"),
                            Err(why) => {
                                log::info!("quickpage: plate {plate} (상점주인의 혼) — {why}");
                                if why == AccountState::SHOP_ORB_NONE {
                                    host_notice("아이템없음");
                                }
                            }
                        }
                        return;
                    }
                    // ★ ⑧ (2026-09-18): 치료제 / 귀환서 are not in `bq.j`, so `0` cannot
                    //   use them — the drive presses the bag's own 사용하기 instead.
                    if crate::guest_build::h2_item_drive_kind(kind) {
                        start_use_drive(emulator, drive, plate, kind, proxy, paged);
                        return;
                    }
                    let sel = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_heal_select_writes(&read, &h2, kind)
                    };
                    let Some((writes, index, total)) = sel else {
                        log::info!("quickpage: plate {plate} (item type {kind}) — none in the bag or not in the cycle");
                        host_notice("회복아이템없음");
                        return;
                    };
                    let mut ok = true;
                    for (a, bytes) in &writes {
                        ok &= java_write(emulator, crate::guest_build::JavaWrite::HealSelect, *a, bytes);
                    }
                    log::info!("quickpage: plate {plate} item type {kind} -> bag index {index} (x{total}), {} write(s), ok={ok} -> `0`", writes.len());
                    if ok {
                        tap_key(emulator, held, drive, guard, crate::HEAL_KEY);
                    }
                }
                quickpage::Activation::Nothing => log::info!("quickpage: plate {plate} pressed — empty or hidden, nothing to do"),
            }
        }
        // ── a tap on the game's own window (hero2 touch router, S4) ───────────
        // ★ #13: a page tab never reaches here — the worker loop answers it with its own
        //   paging state before dispatching (`BagPaging::select`).
        InputEvent::WorldHold(_) => {}
        InputEvent::BagPage(page) => log::warn!("bag pages: tab {page} reached apply_input — the loop should have taken it"),
        InputEvent::HostNotice(text) => host_notice(text),
        InputEvent::MenuTap { fx, fy } => {
            let Some(h2) = crate::guest_build::hero2_java() else { return };
            // While the plates are lit the question on screen is "which plate", and a
            // tap on the popup underneath must not answer it (hero3 79차-C).
            if *picking {
                return;
            }
            let tap = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_tap(&read, &h2, fx, fy, crate::inv_grid_shift())
            };
            match tap {
                crate::guest_build::H2Tap::Focus { writes } => {
                    let mut ok = true;
                    for (a, bytes) in &writes {
                        ok &= java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, *a, bytes);
                    }
                    log::info!("menu tap: ({fx},{fy}) -> focus ({} write(s), ok={ok})", writes.len());
                }
                crate::guest_build::H2Tap::Activate => {
                    // The focused row tapped again. With the register popup up this is the
                    // popup's OK, which has meanings of its own (퀵슬롯등록 arms the plates)
                    // — so it takes the same path an OK key would.
                    let popup_up = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        items::popup_view(&read).is_some()
                    };
                    log::info!("menu tap: ({fx},{fy}) -> OK (popup={popup_up})");
                    if popup_up {
                        apply_input(emulator, held, picking, pages, window, buffs, saving, skill_watch, levelups, drive, account, data_dir, proxy, guard, discard, paged, InputEvent::PopupConfirm);
                    } else if account.intercept_ok(emulator, &h2) {
                        // The ACCOUNT row, tapped twice: the host's window, not the game's.
                    } else if craft_full_guard(emulator, account, &h2) {
                        // ★★ 2026-10-20: a crafting OK into a full bag — the game's own box instead.
                    } else if beast_discard_gate(emulator, discard, KeyCode::OK, true) {
                        // A beast discard's 예, tapped: it waits for the refund (`BeastDiscard`).
                    } else {
                        emulator.handle_event(Event::Keydown(KeyCode::OK));
                        emulator.handle_event(Event::Keyup(KeyCode::OK));
                    }
                }
                crate::guest_build::H2Tap::Tab { right, writes } => {
                    let mut ok = true;
                    for (a, bytes) in &writes {
                        ok &= java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, *a, bytes);
                    }
                    let key = if right { KeyCode::RIGHT } else { KeyCode::LEFT };
                    log::info!("menu tap: ({fx},{fy}) -> tab {key:?} (ok={ok})");
                    if ok {
                        emulator.handle_event(Event::Keydown(key));
                        emulator.handle_event(Event::Keyup(key));
                    }
                }
                crate::guest_build::H2Tap::Page { dir } => {
                    let key = match dir {
                        0 => KeyCode::UP,
                        1 => KeyCode::DOWN,
                        2 => KeyCode::LEFT,
                        _ => KeyCode::RIGHT,
                    };
                    log::info!("menu tap: ({fx},{fy}) -> page {key:?}");
                    emulator.handle_event(Event::Keydown(key));
                    emulator.handle_event(Event::Keyup(key));
                }
                crate::guest_build::H2Tap::Swallow => log::info!("menu tap: ({fx},{fy}) -> swallowed (a window owns the panel)"),
                crate::guest_build::H2Tap::None => log::info!("menu tap: ({fx},{fy}) -> nothing here"),
            }
        }
        InputEvent::QuickPageNext => {
            let Some(h2) = crate::guest_build::hero2_java() else {
                log::info!("quickpage: page flip is hero2-only on this build");
                return;
            };
            let to = pages.next_page();
            // No live bar (boot / teardown): a flip now would warm a page into nothing.
            let live = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                items::in_game_state(&read).then(|| crate::guest_build::h2_quickbar_bytes(&read, &h2)).flatten()
            };
            if live.is_none() {
                log::info!("quickpage: flip to {to} refused — no live character");
                return;
            }
            let plan = pages.flip_to(window, to);
            let mut writes_total = 0usize;
            for (g, v) in &plan {
                // ★ 2026-10-02 (user: "페이지전환을 했다가 다시 돌아오면 스킬 쿨타임이 원복"): the
                //   game's counters are per SLOT, so each slot's outgoing skill leaves with
                //   the cooldown it has (parked, aged by the game's own ticks) and the
                //   incoming one is seated with what ITS has left — not READY.
                let left = seat_cooldown(&*emulator, &h2, window, *g, *v);
                let writes = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    crate::guest_build::h2_bind_writes(&read, &h2, *g, *v, left)
                };
                for (a, bytes) in &writes {
                    let what = if bytes.len() == 4 { crate::guest_build::JavaWrite::Cooldown } else { crate::guest_build::JavaWrite::Quickbar };
                    if java_write(emulator, what, *a, bytes) {
                        writes_total += 1;
                    }
                }
            }
            crate::config::write_quickpages(data_dir, pages);
            let _ = proxy.send_event(UserEvent::QuickPage(pages.current() as u8));
            log::info!("quickpage: now on page {} ({} guest write(s))", pages.current(), writes_total);
        }
    }
}

/// Make the GAME take popup row `row`: write its highlighted-row byte, then call its
/// own popup key handler with the select key.
///
/// This is the only host->guest write in the feature, and it is one byte. Everything
/// that follows — the bind, the unbind sentinel, the "is the skill learned" guard,
/// the item's existing-binding cleanup, the popup teardown — is the game's own code
/// running on its own stack.
fn popup_commit(emulator: &mut dyn Emulator, view: &items::PopupView, row: u8) -> bool {
    let Some(how) = crate::guest_build::popup_commit() else {
        log::warn!("popup: no commit mechanism for the {} build — row {row} dropped", crate::guest_build::name());
        return false;
    };
    // ★ Restore the NATIVE shape first, and only on the synthetic-OK path. See
    //   `items::popup_count_restore` for the tick this closes — and on hero2 the shape
    //   is three fields (label-array length, row count, box height) that have to grow
    //   back BEFORE the cursor goes past our shrunken count, or the painter indexes the
    //   label array out of bounds on the frame in between (`items::popup_restore_writes`).
    if how == crate::guest_build::PopupCommit::SyntheticOk {
        let restore = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            items::popup_restore_writes(&read, view)
        };
        for (a, bytes) in &restore {
            if !java_write(emulator, crate::guest_build::JavaWrite::PopupRows, *a, bytes) {
                log::warn!("popup: could not restore the native shape at {a:#x}");
                return false;
            }
        }
    }
    let addr = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        items::popup_sel_addr(&read)
    };
    let Some(addr) = addr else {
        log::warn!("popup: CHAR vanished before commit");
        return false;
    };
    if !java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, addr, &[row]) {
        log::warn!("popup: could not write the highlighted row at {addr:#x}");
        return false;
    }
    match how {
        crate::guest_build::PopupCommit::GuestCall { fn_addr, select_key } => {
            emulator.call_guest_function(fn_addr, &[u32::from(view.kind), select_key]);
        }
        // The same two events the row-0 pass-through above sends, which is what a finger
        // on the handset produces. The game's own popup dispatcher then reads the row
        // byte we just wrote.
        crate::guest_build::PopupCommit::SyntheticOk => {
            emulator.handle_event(Event::Keydown(KeyCode::OK));
            emulator.handle_event(Event::Keyup(KeyCode::OK));
        }
    }
    true
}

/// Close a popup after one of ITS HOST rows was taken (S7 item 등록 / 해제): the native
/// shape back first — cursor included, since ours may be past the game's count — then
/// CLR, the popup's own cancel path (`d.a(kind, -1)`), which the game handles like the
/// player backing out. The game never sees the host row.
fn popup_close_host_row(emulator: &mut dyn Emulator, view: &items::PopupView) {
    let (restore, cursor) = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        (items::popup_restore_writes(&read, view), items::popup_sel_addr(&read))
    };
    for (a, bytes) in &restore {
        // The cursor byte (`j.h`) is its own gate kind; the rest are the shape.
        let what = if Some(*a) == cursor { crate::guest_build::JavaWrite::WidgetCursor } else { crate::guest_build::JavaWrite::PopupRows };
        if !java_write(emulator, what, *a, bytes) {
            log::warn!("popup: could not restore the native shape at {a:#x}");
        }
    }
    emulator.handle_event(Event::Keydown(KeyCode::CLEAR));
    emulator.handle_event(Event::Keyup(KeyCode::CLEAR));
}

/// The ONE door for hero2's Java DATA writes (HERO2_QUICKSLOT_TOUCH_PLAN.md §4.5): the
/// per-kind gate, then the write, then a line for the device log either way. hero3 builds
/// never reach this — their writes go through their own, separately gated paths — and a
/// hero3 build calling it is refused by the gate, not by luck.
///
/// ★ On hero2 the quickslot and touch features are the FIRST host writes into the guest
/// at all, so every one of them is meant to be greppable: `java write:`.
/// Begin the item-use drive for `kind` from plate `plate` (hero2 ⑧). Refused, with a
/// toast, when one is already running, when the character is not on the field with
/// the menu closed, or when the bag has none of the type — all read NOW, none assumed.
fn start_use_drive(emulator: &mut dyn Emulator, drive: &mut Option<UseDrive>, plate: u8, kind: u8, _proxy: &EventLoopProxy<UserEvent>, paged: bool) {
    let Some(h2) = crate::guest_build::hero2_java() else { return };
    if drive.is_some() {
        log::info!("item drive: plate {plate} (type {kind}) pressed while a drive is running — ignored");
        return;
    }
    let started = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_use_start(&read, &h2, kind)
    };
    match started {
        Ok(mut d) => {
            log::info!("item drive: plate {plate} item type {kind} -> the bag's own 사용하기 (menu → bag → record → OK → back)");
            d.paged = paged;
            let mut u = UseDrive { d, owns: false, plate, started: Instant::now() };
            u.take(true);
            *drive = Some(u);
        }
        Err(why) => {
            log::info!("item drive: plate {plate} item type {kind} refused — {why}");
            let toast = if why.starts_with("none") { "아이템없음" } else { crate::HOST_SLOT_TOAST };
            host_notice(toast);
        }
    }
}

/// One poll of the item-use drive: ask `h2_use_step` what the screen calls for and do
/// it — cursor writes through the `WidgetCursor` gate, then one key. The hold and the
/// input gate follow the drive's ownership (`UseDrive::take`).
fn step_use_drive(emulator: &mut dyn Emulator, drive: &mut Option<UseDrive>, proxy: &EventLoopProxy<UserEvent>, bag_paging: &mut BagPaging, account: &mut AccountState) {
    use crate::guest_build::{H2Key, H2UseAction};
    let (Some(u), Some(h2)) = (drive.as_mut(), crate::guest_build::hero2_java()) else { return };
    let action = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_use_step(&read, &h2, &mut u.d)
    };
    match action {
        H2UseAction::Act { writes, key, what } => {
            // The record may be on another page of the bag: put the window there first,
            // through the pager, so the cursor written below names the cell the drive
            // means. `take` — a page is a fact about the step that asked for it.
            if let Some(page) = u.d.page.take() {
                bag_paging.page_to(emulator, &h2, proxy, page, u.d.index.unwrap_or(0));
            }
            let mut ok = true;
            for (a, bytes) in &writes {
                ok &= java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, *a, bytes);
            }
            let code = match key {
                H2Key::Ok => KeyCode::OK,
                H2Key::Clr => KeyCode::CLEAR,
            };
            log::info!("item drive: {what} ({} write(s), ok={ok}) -> {:?}", writes.len(), u.d.step);
            if ok {
                emulator.handle_event(Event::Keydown(code));
                emulator.handle_event(Event::Keyup(code));
            } else {
                // A refused write means the gate is shut or the object moved: do not
                // press a key whose target is unknown. The step clock ends the drive.
                log::warn!("item drive: a cursor write failed — the key is withheld");
            }
        }
        H2UseAction::Wait => {}
        H2UseAction::Handoff { why } => {
            log::info!("item drive: handing the screen to the player — {why}");
            u.take(false);
        }
        // ★★ 2026-10-21 비행서 plate: the scroll's popup is up — the host's own first-stage OK (the town list over its
        //   labels, or the game's refusal box when nothing was visited / not allowed here), then the screen is the
        //   player's. The drive waits in its handoff for the warp or the CLR.
        H2UseAction::FlightPicker { popup } => {
            log::info!("item drive: 비행서 popup {popup:#x} — writing the town list, handing the screen to the player");
            account.start_flight(emulator, &h2, popup, None);
            u.take(false);
        }
        H2UseAction::Resume => {
            log::info!("item drive: the player answered — closing the rest");
            u.take(true);
        }
        H2UseAction::Done => {
            log::info!("item drive: plate {} item type {} done in {} ms ({} poll(s))", u.plate, u.d.kind, u.started.elapsed().as_millis(), u.d.polls);
            u.take(false);
            *drive = None;
        }
        H2UseAction::Abort { why } => {
            log::warn!("item drive: plate {} item type {} stopped at {:?} — {why} ({} poll(s))", u.plate, u.d.kind, u.d.step, u.d.polls);
            u.take(false);
            *drive = None;
            host_notice("사용실패");
        }
    }
}

/// ★★ #13 step 4 (2026-09-21): the inventory PAGING state this thread owns.
///
/// The game's inventory window shows cells 0..27 of whatever bag it was opened on; the
/// bag holds up to 127 (`Hero2Java::BAG_CAPACITY`). The patch (`InventoryPagePatch`)
/// makes the window's seven bag calls add `ctx.off` to their index when `this` is the
/// inventory, and THIS is where `ctx` comes from: `page * 28`, on the window's own
/// bag handle, page 0 whenever the window is gone. Everything is idempotent per poll —
/// install once, then read-compare-write the two context words.
/// A list of `(site, as shipped, patched)` halfwords, applied once and then left alone.
///
/// ⛔ `done` latches only on SUCCESS — the first patch pass is the only thing standing
/// between "the image is not mapped yet" and "the site moved", and this cannot tell those
/// apart, so a failure must be free to try again. `said` is what keeps the second case
/// quiet: the patch pass runs every 30 frames, and an ungated warning would repeat twice
/// a second for the rest of the process (the log-spam of 2026-09-18).
///
/// ⛔ A partial landing is the one bad outcome — for a multi-halfword rewrite it leaves
/// the guest running half an instruction sequence. `h2_halfword_writes` makes it nearly
/// unreachable (it plans only when EVERY site reads as shipped or as already patched), so
/// what is left is a `write_memory` that fails midway: logged at error, retried next pass
/// from what the memory now says, so the retry asks only for what did not land.
struct HalfwordPatch {
    done: bool,
    said: bool,
}

impl HalfwordPatch {
    fn new() -> Self {
        Self { done: false, said: false }
    }

    fn apply(
        &mut self,
        emulator: &mut dyn Emulator,
        what: &str,
        effect: &str,
        sites: &[(u32, u16, u16)],
    ) {
        if self.done {
            return;
        }
        let (todo, already, total) = {
            let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
            crate::guest_build::h2_halfword_writes(&read, sites)
        };
        if total == 0 {
            return;
        }
        if todo.is_empty() && already == 0 {
            if !self.said {
                self.said = true;
                log::warn!("{what}: no site reads as shipped yet — not patched, the game keeps its own rule");
            }
            return;
        }
        let mut landed = already;
        for (a, bytes) in &todo {
            if emulator.write_memory(*a, bytes) {
                landed += 1;
            } else {
                log::warn!("{what}: the write at {a:#x} failed");
            }
        }
        self.done = landed == total;
        if self.done {
            log::info!("{what}: patched ({total} halfword(s)) — {effect}");
        } else {
            log::error!("{what}: only {landed}/{total} halfword(s) landed — HALF-PATCHED, retrying next pass");
        }
    }

    /// ★ 2026-10-09: [`Self::apply`] for CELL words (`guest_build::h2_cell_redirect_writes` — either shipped state
    /// of a cell, unresolved or resolved, takes the patched definition). Same latch and the same one warning.
    fn apply_cells(&mut self, emulator: &mut dyn Emulator, what: &str, effect: &str, cells: &[(u32, u32, u32, u32)]) {
        if self.done {
            return;
        }
        let (todo, already, total) = {
            let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
            crate::guest_build::h2_cell_redirect_writes(&read, cells)
        };
        if total == 0 {
            return;
        }
        if todo.is_empty() && already == 0 {
            if !self.said {
                self.said = true;
                log::warn!("{what}: no cell reads as shipped yet — not patched, the game keeps its own rule");
            }
            return;
        }
        let mut landed = already;
        for (a, bytes) in &todo {
            if emulator.write_memory(*a, bytes) {
                landed += 1;
            } else {
                log::warn!("{what}: the write at {a:#x} failed");
            }
        }
        self.done = landed == total;
        if self.done {
            log::info!("{what}: patched ({total} cell(s)) — {effect}");
        } else {
            log::error!("{what}: only {landed}/{total} cell(s) landed — HALF-PATCHED, retrying next pass");
        }
    }
}

/// ★ #17 (2026-09-24): every class skill hit ×1.5 — `guest_anchors::SkillPowerPatch`.
/// ★ 2026-09-26: and the berserker's also +3 % per level of 근력단련 (its SP term is gone —
/// `guest_anchors::strength_training_sp_off`).
///
/// One 144-byte host block (a 48-byte stub per job) and three definition words, installed
/// once from the patch pass, all three or none. Unlike the EXP redirect this is not a
/// setting, so there is no multiplier word to keep in step afterwards: install, verify,
/// done. ⛔ A verify that fails puts the three def words back — the arming writes are
/// the only ones that change behaviour, so restoring them restores the shipped game even
/// with the block left behind (nothing jumps to it).
struct SkillPower {
    /// The host block once all three redirects are in.
    block: Option<u32>,
    /// The block reserved for it, kept across a plan that could not be made yet:
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    /// `true` once a write or the verify failed — the game keeps its own multipliers.
    refused: bool,
    /// The "defs do not read as shipped" warning, once (the patch pass is every 30 frames).
    said: bool,
}

impl SkillPower {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::skill_power_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::SkillPowerPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("skill power: no guest block — class skills keep the shipped damage");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_skill_power_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            // Not latched — the same "too early or moved" ambiguity as the halfword
            // patches; the next pass is 30 frames away. Said once.
            if !self.said {
                self.said = true;
                log::warn!("skill power: the three <job>.a(BILbd;)I definitions do not read as shipped — not redirected yet");
            }
            return;
        };
        let originals: Vec<(u32, Vec<u8>)> = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            patch.jobs.iter().map(|(def, ..)| (*def, read(*def, 4))).collect()
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("skill power: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_skill_power_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            log::info!("skill power: class skill hits x1.5 (+3 %/level of 버서커 근력단련) — 3 multipliers redirected through {block:#x} (verified=true)");
            return;
        }
        for (def, word) in &originals {
            if word.len() == 4 && !emulator.write_memory(*def, word) {
                log::error!("skill power: could not restore the def word at {def:#x}");
            }
        }
        self.refused = true;
        log::error!("skill power: the install did not verify ({done}/{} writes) — defs restored, shipped damage", writes.len());
    }
}

/// ★ 2026-09-27 전투 밸런스: the COMBAT host block — weapon attack by type, 둔기강타/방패치기's
/// front sweep, 신의가호's three-way 검기, 민첩태세's invulnerability guards. See
/// `guest_anchors::CombatPatch`. Installed once from the patch pass, everything or nothing,
/// the same way as [`SkillPower`]: a verify that fails puts back every word it armed.
struct Combat {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

/// ★★ 2026-10-17 the combat block's address, for [`park_evicted`] — 쌍월's window lives in it (`CombatPatch::TWIN_*`) and the
/// plate window's seating is not handed the struct. 0 = not installed.
static COMBAT_BLOCK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

impl Combat {
    fn new() -> Self {
        COMBAT_BLOCK.store(0, std::sync::atomic::Ordering::Relaxed);
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::combat_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::CombatPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("combat: no guest block — weapons, 둔기강타/방패치기, 신의가호 and 민첩태세 keep the shipped rules");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_combat_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("combat: the defs / pool words / guard site do not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_combat_arms(&patch, block);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("combat: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_combat_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            COMBAT_BLOCK.store(block, std::sync::atomic::Ordering::Relaxed);
            log::info!("combat: weapons 도검·둔기 +10 % 단검 +20 % · 둔기강타 8 / 24 tiles · 방패치기 3 / 6 · 검기 lobe 13 / 27 (the wave a ring picture) · 발도 36-tile disc (+1 radius a reach item, 신의가호's crescent band), SP 1/4, cooldown 7 s · 쌍월 0.5 s window · 민첩태세 invulnerable · 철괴 +1 % per 5 지력 (50 % at most) · 치유오라 × (1 + 지력/100) — through {block:#x} (verified=true)");
            return;
        }
        // Back to shipped, the guard's halfword first (the reverse of the arming order).
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("combat: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("combat: the install did not verify ({done}/{} writes) — restored, shipped rules", writes.len());
    }
}

/// ★ 2026-09-29 맹독감염 → a buff whose 기본공격 poison ticks for a share of the attack —
/// `guest_anchors::VenomPatch`. Installed once from the patch pass, everything or nothing, the
/// way [`Combat`] is: the cast, the cooldown, the strike wrapper and the tick go in together
/// (the cast alone would be a buff that does nothing, the tick alone a far stronger old skill),
/// and a verify that fails puts back every arm.
struct Venom {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl Venom {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::venom_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::VenomPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("venom: no guest block — 맹독감염 keeps the shipped strike");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_venom_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("venom: the cast / cooldown / strike def / tick site do not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_venom_arms(&patch, block);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("venom: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_venom_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            use crate::guest_build::VenomPatch as V;
            log::info!(
                "venom: 맹독감염 = buff {} for {}..{} s (독연마 folded in at {} — cooldown 30 s) · 기본공격 poison {} s · a tick = the attack × {}..{} % — through {block:#x} (verified=true)",
                V::BUFF_ID,
                V::buff_frames_mastered(1) / 20,
                V::buff_frames_mastered(V::VENOM_MAX) / 20,
                V::MASTERY_MAX,
                u32::from(V::POISON_LEN) / 20,
                V::tick_percent_mastered(1),
                V::tick_percent_mastered(V::VENOM_MAX),
            );
            return;
        }
        // Back to shipped, the tick's halfword first (the reverse of the arming order).
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("venom: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("venom: the install did not verify ({done}/{} writes) — restored, shipped 맹독감염", writes.len());
    }
}

/// ★ 2026-09-30 야수진화제 판매가 (user: *"판매가격은 5만원, 적은 7만원인데 이걸 팔떄는 만원, 적은
/// 12000원으로"*): the sell window's price per item — `guest_anchors::ShopSellPatch`. Installed
/// once from the patch pass, all or nothing, the way [`Venom`] is: the sale and the painter go in
/// together (a list that shows one price and pays another would be worse than either), and a
/// verify that fails puts back every arm. Nothing is written to a record or a save.
struct ShopSell {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl ShopSell {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::shop_sell_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::ShopSellPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("shop sell: no guest block — the shop keeps the shipped price / 4");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_shop_sell_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("shop sell: the sale's or the painter's ch.d read does not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_shop_sell_arms(&patch, block);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("shop sell: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_shop_sell_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            let prices: Vec<String> = crate::guest_build::ShopSellPatch::PRICES.iter().map(|(k, v)| format!("{}:{} = {v}", k >> 8, k & 0xff)).collect();
            log::info!("shop sell: sell price {} (everything else price / 4, as shipped) — through {block:#x} (verified=true)", prices.join(" · "));
            return;
        }
        // Back to shipped, the sale's halfword first (the reverse of the arming order).
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("shop sell: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("shop sell: the install did not verify ({done}/{} writes) — restored, shipped price / 4", writes.len());
    }
}

/// ★★ 2026-10-10 보스 치명 (user: *"버서커 무기에 투지의 구 발랐을떄 효과 발동시 보스몹한테 데미지 0뜨는 버그"* →
/// fix 「2」): a crit or a 투지 hit on a boss is `d · max(10 · ⌊crit % / 10⌋, the weapon's crit ×10) / 10` —
/// never 0, never below an ordinary monster's, the shipped `⌊crit % / 10⌋ · d` (the rogue's ×6..9) whenever that
/// is larger. `guest_anchors::BossCritPatch`, HERO2_GUEST_RE.md §8.63. Installed once from the patch pass, all or
/// nothing, the way [`ShopSell`] is: the block, the pool word, the `bx r0` last; a verify that fails puts both
/// arms back. Nothing is written to a record or a save.
struct BossCrit {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl BossCrit {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::boss_crit_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::BossCritPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("boss crit: no guest block — a boss crit stays ⌊crit % / 10⌋ × the hit, as shipped");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_boss_crit_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("boss crit: the boss hit routine's crit read does not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_boss_crit_arms(&patch, block);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("boss crit: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_boss_crit_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            log::info!("boss crit: a boss crit is never below the weapon's multiplier (the shipped ⌊crit % / 10⌋ above it) — through {block:#x} (verified=true)");
            return;
        }
        // Back to shipped, the `bx r0` first (the reverse of the arming order).
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("boss crit: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("boss crit: the install did not verify ({done}/{} writes) — restored, the shipped boss crit", writes.len());
    }
}

/// ★★ 2026-10-21 보스 방어 관통 (user: *"전체 보스들의 데미지중 일부를 방어무시 데미지로 만들어줘. 데미지중 20%를
/// 방어무시데미지로 전환하고싶어."*): a boss's hit on the character is `max(R − DEF, R × 20 %)` — the 20 % passes the defence
/// and the knight's shield block, everything else (the dodge, the critical, 얼음장막, ×½, 철괴, the gear's cuts, the minimum 3)
/// as shipped. `guest_anchors::BossPiercePatch`. Installed once from the patch pass, all or nothing, the way [`BossCrit`]
/// is: the words, the code, each site's pool word, its `bx r0` last; a verify that fails puts every arm back. Nothing is
/// written to a record or a save.
struct BossPierce {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl BossPierce {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::boss_pierce_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::BossPiercePatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("boss pierce: no guest block — a boss's hit meets the whole defence, as shipped");
                    self.refused = true;
                    return;
                }
            },
        };
        let pct = crate::guest_build::BossPiercePatch::PIERCE_PCT;
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_boss_pierce_install_writes(&read, &patch, block, pct)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("boss pierce: the hit routine's defence / shield reads do not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_boss_pierce_arms(&patch, block).unwrap_or_default();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("boss pierce: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_boss_pierce_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            log::info!("boss pierce: {pct} % of a boss's hit passes defence — through {block:#x} (verified=true)");
            return;
        }
        // Back to shipped, each `bx r0` before its pool word (the reverse of the arming order).
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("boss pierce: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("boss pierce: the install did not verify ({done}/{} writes) — restored, the shipped hit", writes.len());
    }
}

/// ★ 2026-10-13 등급 줄 (user: *"이름이 긴 아이템들은 등급과 이름이 겹쳐져서 보이는 경우가 있는데 겹쳐지면 등급을
/// 두번째줄 우측끝으로 정렬할꺼야"*): a name too wide for its line puts its grade on the empty line under it,
/// right-aligned — `guest_anchors::GradeLinePatch`. Installed once from the patch pass, all or nothing, the way
/// [`BossCrit`] is: the block, the pool word, the `bx r0` last; a verify that fails puts both arms back.
/// Nothing is written to a record or a save.
struct GradeLine {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl GradeLine {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::grade_line_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::GradeLinePatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("grade line: no guest block — a long name and its grade share the line, as shipped");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_grade_line_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("grade line: the info painter's grade draw does not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_grade_line_arms(&patch, block);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("grade line: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_grade_line_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            log::info!("grade line: a name wider than {} px puts its grade on the line under it — through {block:#x} (verified=true)", crate::guest_build::GradeLinePatch::NAME_MAX);
            return;
        }
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("grade line: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("grade line: the install did not verify ({done}/{} writes) — restored, as shipped", writes.len());
    }
}

/// ★ 2026-10-02 HELL 보너스 드랍 (user: *"헬 장비드랍이 하드코어보다 조금 더 좋아야해"* · *"hell모드에서
/// 하드코어보다 장신구도 잘 나와야해"*): in HELL a kill rolls the enemy's two HELL-only slots once more —
/// `guest_anchors::HellDropPatch`, the slots `make_hero2_asset.py` adds. Installed once from the patch pass,
/// all or nothing, the way [`ShopSell`] is: the block, then `bd.q()V`'s definition word; a verify that fails
/// puts the word back. An asset without the slots gives nothing (the block checks), NORMAL and HARDCORE never
/// reach it. Nothing is written to a record or a save.
struct HellDrop {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// ★ 2026-10-02 저녁 (드랍 티어): the block's gate word as last written — the new six accessories'
    /// slot rolls only while the 환경설정 난이도 is NORMAL or above (`HellDropPatch::gate_open`; HARD only
    /// until 2026-10-03, user: *"신규 장신구를 헬 노말난이도부터 나오게"*).
    gate: Option<bool>,
}

impl HellDrop {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false, gate: None }
    }

    /// Keeps the block's gate word at the 난이도 now — with the HUD poll, cheap (one word when it changes).
    fn poll(&mut self, emulator: &mut dyn Emulator) {
        let Some(block) = self.block else { return };
        let open = crate::guest_build::HellDropPatch::gate_open(crate::difficulty());
        if self.gate == Some(open) {
            return;
        }
        let Some(patch) = crate::guest_build::hell_drop_patch() else { return };
        let Some((at, word)) = crate::guest_build::h2_hell_drop_gate_write(&patch, block, open) else { return };
        if emulator.write_memory(at, &word) {
            self.gate = Some(open);
            log::info!("hell drop: the 난이도-gated slot (NORMAL+) is {} (gate word {at:#x} <- {})", if open { "open" } else { "closed" }, u8::from(open));
        } else {
            log::warn!("hell drop: could not write the gate word at {at:#x}");
        }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::hell_drop_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::HellDropPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("hell drop: no guest block — HELL drops as HARDCORE");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_hell_drop_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("hell drop: bd.q()V's definition word does not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_hell_drop_arms(&patch, block);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("hell drop: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_hell_drop_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            self.gate = None; // written closed — the next poll sets it to the 난이도 now
            log::info!("hell drop: HELL kills roll the HELL bonus slots once more — through {block:#x} (verified=true)");
            return;
        }
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("hell drop: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("hell drop: the install did not verify ({done}/{} writes) — restored, HELL drops as HARDCORE", writes.len());
    }
}

/// ★★★ 2026-10-08 「가디언」 모드 ① — the mode-indexed tables, polled EVERY frame from the first until done
/// (`guest_anchors::GuardianPatch`, HERO2_GUARDIAN_PLAN.md). The order is the point:
///
///   1. `guest_anchors::GuestBuild::guardian_mode_tables` — the three `movs r1,#3` in `ag.<clinit>` / `co.<clinit>`
///      read `#4` — BEFORE those initialisers run. They run during the logo (`SaveSlots` itself asks for
///      `ag`'s), so this poll goes right before `SaveSlots::poll` on the same frame.
///   2. once an array exists four long, its fourth cell: `ag.i:[B[3]` ← 10 · `co.b:[I[3]` ← the purple
///      (`JavaWrite::ModeTables`; `ag.m:[B[3]` is the 0 the allocation left). `co` initialises at the title.
///   3. only when all three read four long and filled: `guardian_mode_open` — the round clear's `cmp r0,#1` → `#2`,
///      the one halfword that lets `ag.i` reach 3. A table that reads THREE long (the initialiser ran first) or
///      foreign keeps that halfword out for the whole run: the game stays at HELL, and says so once.
///
/// The labels (`guardian_labels`: slot screen · restart notice · fee text) go in with step 1 regardless — the asset
/// moved those rows, and without them the shipped indices show the wrong text in every mode.
///
/// ★ 2026-10-17 (user: *"게임 맨처음에 구동한 뒤에 이어하기 누르면 … 가디언 난이도가 공백으로 뜨는 현상 … 맨 처음 구동시에만"*):
/// `co`'s first reader is the slot screen itself — left to the game, `co.<clinit>` runs inside that screen's first paint,
/// which then draws GUARDIAN in `co.b[3]` = the allocation's 0 (black on the black box), and the screen keeps that picture
/// (probe: `co` initialised at f=421 = the 이어하기 key, the cell written at f=422). So once the halfwords are in and `co`
/// is still untouched, the host initialises it (`Emulator::init_java_class`, as `SaveSlots` does `ag`) — the logo, long
/// before the title — and the cell is written on the next poll.
struct ModeTables {
    tables: HalfwordPatch,
    labels: HalfwordPatch,
    open: HalfwordPatch,
    /// `init_java_class(COLOUR_CLASS)` in flight.
    colour_init: Option<Arc<wie_backend::GuestCallResult>>,
    colour_tries: u8,
    /// No new initialisation before this frame (a failed one waits a moment).
    colour_retry_at: u64,
    done: bool,
    off: bool,
}

impl ModeTables {
    /// Host initialisations of `co` before leaving it to the game (the old first-visit black label, said once).
    const COLOUR_TRIES: u8 = 8;
    const COLOUR_RETRY_FRAMES: u64 = 10;

    fn new() -> Self {
        Self {
            tables: HalfwordPatch::new(),
            labels: HalfwordPatch::new(),
            open: HalfwordPatch::new(),
            colour_init: None,
            colour_tries: 0,
            colour_retry_at: 0,
            done: false,
            off: false,
        }
    }

    fn finished(&self) -> bool {
        self.done || self.off
    }

    /// `co` before the game reaches it — only once the `movs r1,#3 → #4` are in (an early `<clinit>` would otherwise
    /// build the colours three long and close mode 3 for the run).
    fn init_colours(&mut self, emulator: &mut dyn Emulator, colour: crate::guest_build::H2ModeTable, frame: u64) {
        let class = crate::guest_build::GuardianPatch::COLOUR_CLASS;
        if let Some(out) = self.colour_init.take() {
            match out.poll() {
                None => self.colour_init = Some(out),
                Some(Ok(_)) => log::info!("guardian tables: {class} initialized by the host, ahead of the slot screen"),
                Some(Err(())) => {
                    log::warn!("guardian tables: {class} could not be initialized yet — retrying");
                    self.colour_retry_at = frame + Self::COLOUR_RETRY_FRAMES;
                }
            }
            return;
        }
        if !matches!(colour, crate::guest_build::H2ModeTable::NotYet) || !self.tables.done || frame < self.colour_retry_at {
            return;
        }
        if self.colour_tries >= Self::COLOUR_TRIES {
            if self.colour_tries == Self::COLOUR_TRIES {
                self.colour_tries += 1;
                log::warn!("guardian tables: {class} would not initialize from the host — the game does it on the slot screen (GUARDIAN's label is black on that first visit)");
            }
            return;
        }
        self.colour_tries += 1;
        let out = Arc::new(wie_backend::GuestCallResult::new());
        emulator.init_java_class(class, out.clone());
        self.colour_init = Some(out);
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, frame: u64) {
        if self.finished() {
            return;
        }
        let Some(p) = crate::guest_build::guardian_patch() else {
            self.off = true;
            return;
        };
        self.tables.apply(emulator, "guardian tables", "ag.i:[B · ag.m:[B · co.b:[I are built four long", crate::guest_build::guardian_mode_tables());
        self.labels.apply(emulator, "guardian labels", "slot screen GUARDIAN · restart notice by mode · the fee text at com[56]", crate::guest_build::guardian_labels());
        let t = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_mode_tables(&read, &p)
        };
        self.init_colours(emulator, t.colour, frame);
        if t.refused() {
            self.off = true;
            log::warn!("guardian tables: {t:?} — a mode table is not four long (its initialiser ran before the halfword landed): the round clear stays at HELL this run");
            return;
        }
        for (at, bytes) in t.writes() {
            if !java_write(emulator, crate::guest_build::JavaWrite::ModeTables, at, &bytes) {
                log::warn!("guardian tables: the fourth-cell write at {at:#x} failed — retrying next frame");
                return;
            }
        }
        if !t.ready() {
            return;
        }
        if !self.tables.done {
            // The arrays read four long although the halfwords never landed through this host (another pass of this
            // process wrote them, or the image differs): never open the round clear on that.
            if self.tables.said {
                self.off = true;
                log::warn!("guardian tables: the arrays read four long but the halfwords did not land here — the round clear stays at HELL");
            }
            return;
        }
        self.open.apply(emulator, "guardian round", "a HELL clear goes on to GUARDIAN (ag.i 3)", crate::guest_build::guardian_mode_open());
        if self.open.done {
            self.done = true;
            log::info!("guardian: the mode tables are four long (quest ×{} · weight {} · colour {:#08x}) and the round clear opens mode 3", crate::guest_build::GuardianPatch::QUEST_MULT, crate::guest_build::GuardianPatch::WEIGHT, crate::guest_build::GuardianPatch::COLOUR);
        } else if self.open.said {
            self.off = true;
        }
    }
}

/// ★★★ 2026-10-08 「가디언」 모드 ② — the monster template wrapper and the level-correction hooks, one host block
/// (`guest_anchors::GuardianPatch`). Installed once from the patch pass, all or nothing, the way [`HellDrop`] is:
/// the block, then `cq.a()V`'s definition word and the four hooked `bd.n` reads; a verify that fails puts every
/// arm back. The block does nothing unless `ag.i` reads 3 — in NORMAL · HARDCORE · HELL the game is byte for byte
/// the shipped one. Nothing is written to a record or a save.
struct Guardian {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl Guardian {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::guardian_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::GuardianPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("guardian: no guest block — GUARDIAN monsters would be HELL's");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_guardian_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("guardian: cq.a()V's definition word or a bd.n read site does not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_guardian_arms(&patch, block).unwrap_or_default();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("guardian: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_guardian_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            let w = crate::guest_build::GuardianPatch::words();
            log::info!("guardian: templates HELL {}+ → {} + ×{}/256 (cap {}) · HP ×{}/256 (bosses ×{}/256) · ATK ×{}/256 · DEF ×{}/256 · AGI ×{}/256 · formulas see the HELL level (×{}/256, on={}) · kill EXP ×{}/256 — through {block:#x} (verified=true)", w[0], w[1], w[2], w[3], w[4], w[10], w[5], w[6], w[7], w[8], w[9], w[11]);
            return;
        }
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("guardian: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("guardian: the install did not verify ({done}/{} writes) — restored, GUARDIAN monsters are HELL's", writes.len());
    }
}

/// ★★★ 2026-10-09 「가디언」 장비의 개성 옵션 31 (HERO2_GUARDIAN_DECISIONS.md C1): one host block,
/// `guest_anchors::GearPatch`. Installed once from the patch pass AFTER every other block that wraps the same words
/// (SkillPower · combat · accessory · 일풍격 · berserker · buff-refresh — its chains are what those left in the
/// definitions and in 철괴's hook word) and after the guardian and venom blocks (it points their GEAR_PTR words at its
/// effect words), all or nothing; a verify that fails puts every arm back. Then, with the HUD poll, the seven equipment
/// slots and the class decide the effect words (nothing worn = the shipped game). Max HP / SP are the block's own
/// (its `a.m()V` piece reads the equipment itself).
struct Gear {
    block: Option<u32>,
    chain: crate::guest_build::H2GearChain,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// The effect words as last written.
    effects: Vec<u8>,
    /// ★ 2026-10-09 (3): the 현자의 인장 resets the block has counted, as last seen.
    resets: u32,
    /// The cooldown clock's COUNT word the block was installed with (용의 심장's minute runs on it).
    clock: Option<u32>,
    /// ★★ 2026-10-14 (2차 B1 시간도둑): the seconds the block has taken off, as last seen.
    thief_secs: u32,
    /// ★★ 2026-10-15 (2차 F1 중력의 갑주): the bag's equipment records as last counted, and the polls since — the 127-cell walk
    /// runs once every [`Self::COUNT_EVERY`] polls (about a second), and only while the armour is worn.
    counted: u32,
    count_polls: u32,
    /// ★★ 2026-10-16 악의 무기: the berserker block (격분's state — M1), the 격분 gauge % the claw has banked (M1), the demon
    /// block's claw HITS · this block's HIT_COUNT · the clock as last seen (the deltas), 격분 as last seen running, and the
    /// ticks owed off 악의's cooldown ([`Self::take_malice_ticks`]).
    berserker: Option<u32>,
    live: crate::guest_build::H2GearLiveState,
    malice_ticks: i32,
}

impl Gear {
    const COUNT_EVERY: u32 = 4;

    fn new() -> Self {
        Self {
            block: None,
            chain: Default::default(),
            reserved: None,
            refused: false,
            said: false,
            effects: crate::guest_build::GearPatch::effects_none().to_vec(),
            resets: 0,
            clock: None,
            thief_secs: 0,
            counted: 0,
            count_polls: 0,
            berserker: None,
            live: Default::default(),
            malice_ticks: 0,
        }
    }

    /// ★★ 2026-10-16 (악의 무기 M1): the ticks the berserker's 격분 hits took off 악의's cooldown since the last call.
    fn take_malice_ticks(&mut self) -> i32 {
        std::mem::take(&mut self.malice_ticks)
    }

    /// ★★ 2026-10-16 (악의 무기) what the options that follow the moment see this poll — the shared state machine
    /// (`hero2java::H2GearLiveState`, the probe runs the same); the ticks it owes off 악의's cooldown go to [`Self::take_malice_ticks`].
    fn live(&mut self, emulator: &dyn Emulator, worn: &[(u8, u8)], class: u8, demon: Option<u32>) -> crate::guest_build::GearLive {
        let Some(patch) = crate::guest_build::gear_patch() else { return Default::default() };
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let before = self.live.bank;
        let (live, owed) = self.live.step(&read, patch.char_slot, worn, class, self.block, self.clock, demon, self.berserker);
        if before > 0 && self.live.bank == 0 && live.fury_bank == 0 {
            log::info!("gear: 광마의 대검 — 격분 started with the bank {before} %");
        }
        self.malice_ticks += owed;
        live
    }

    /// ★★ 2026-10-14 (2차 B1 시간도둑): the game ticks to take off the host's parked cooldowns — what the block took off the
    /// guest's four since the last poll (0 when nothing moved).
    fn thief_ticks(&mut self, emulator: &dyn Emulator) -> i32 {
        let Some(block) = self.block else { return 0 };
        let now = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_gear_thief_secs(&read, block)
        };
        let Some(now) = now else { return 0 };
        let moved = now.wrapping_sub(self.thief_secs).min(1000);
        self.thief_secs = now;
        moved as i32 * i32::from(crate::guest_build::GearPatch::THIEF_TICKS)
    }

    /// `ready` = every block this one chains after has settled (in or refused). `combat` · `guardian` · `venom` = those
    /// blocks' addresses (their hook / pointer words), `None` when they are not in; `clock` = the cooldown clock's COUNT
    /// word (`None` = no clock: 용의 심장's buff never comes on).
    /// ★★ 2026-10-14 `berserker` = the berserker block (its GEAR_PTR ← this block, this block's BSK_EXT ← its FGEAR — 격분 장비).
    #[allow(clippy::too_many_arguments)]
    /// ★★ 2026-10-15 `ilpung`: the ilpung block — with the combat block, its range pointer word ← this block's W_RANGE (2차 E).
    #[allow(clippy::too_many_arguments)]
    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool, combat: Option<u32>, guardian: Option<u32>, venom: Option<u32>, clock: Option<u32>, berserker: Option<u32>, ilpung: Option<u32>) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let Some(patch) = crate::guest_build::gear_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::GearPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("gear: no guest block — the guardian gear options change nothing");
                    self.refused = true;
                    return;
                }
            },
        };
        let hook_word = combat.and_then(|c| crate::guest_build::combat_patch().and_then(|cp| crate::guest_build::h2_iron_hook_at(&cp).map(|h| c + h)));
        let guardian_ptr = guardian.and_then(|g| crate::guest_build::guardian_patch().and_then(|gp| crate::guest_build::h2_guardian_word_write(&gp, g, crate::guest_build::GuardianPatch::W_GEAR_PTR, 0).map(|(at, _)| at)));
        let venom_ptr = venom.map(|v| v + crate::guest_build::VenomPatch::GEAR_PTR);
        let range_ptrs: Vec<u32> = [combat.map(|c| c + crate::guest_build::CombatPatch::GEAR_RANGE_PTR), ilpung.map(|i| i + crate::guest_build::IlpungPatch::GEAR_RANGE_PTR)].into_iter().flatten().collect();
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_gear_chain(&read, &patch, block, hook_word, None)
                .and_then(|chain| crate::guest_build::h2_gear_install_writes(&read, &patch, block, &chain, hook_word, guardian_ptr, venom_ptr, clock, berserker, &range_ptrs).map(|w| (chain, w)))
        };
        let Some((chain, writes)) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("gear: a wrapped word is not a Thumb address or reads as another patch's — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_gear_arms(&patch, block, &chain, hook_word).unwrap_or_default();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("gear: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_gear_installed(&read, &patch, block, &chain, hook_word, guardian_ptr, venom_ptr, clock, berserker, &range_ptrs)
        };
        if verified {
            self.block = Some(block);
            crate::GEAR_BLOCK.store(block, std::sync::atomic::Ordering::Relaxed); // ★★ 2026-10-17 the elixir's 술꾼의 장화
            self.chain = chain;
            self.clock = clock;
            self.berserker = berserker;
            log::info!("gear: the 31 guardian gear options through {block:#x} ({} arms, hook {hook_word:?}, guardian ptr {guardian_ptr:?}, venom ptr {venom_ptr:?}, clock {clock:x?}, chain {:x?}, verified=true)", arms.len(), chain.words());
            return;
        }
        for (at, before, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, before) {
                log::error!("gear: could not restore {at:#x}");
            }
        }
        for at in [guardian_ptr, venom_ptr].into_iter().flatten() {
            emulator.write_memory(at, &0u32.to_le_bytes());
        }
        self.refused = true;
        log::error!("gear: the install did not verify ({done}/{} writes) — restored, no guardian gear option is on", writes.len());
    }

    /// The worn items → the effect words (outside the world, or with no class, nothing is on). `true` when the seal
    /// (현자의 인장) has reset the cooldowns since the last poll — the caller drops the host's parked ones too.
    fn poll(&mut self, emulator: &mut dyn Emulator, in_game: bool, class: Option<u8>, demon: Option<u32>) -> bool {
        type P = crate::guest_build::GearPatch;
        let Some(block) = self.block else { return false };
        let Some(patch) = crate::guest_build::gear_patch() else { return false };
        let resets = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_gear_resets(&read, block)
        };
        let reset = resets.is_some_and(|r| r != self.resets);
        if let Some(r) = resets {
            if reset {
                log::info!("gear: 현자의 인장 reset every skill cooldown ({} → {r})", self.resets);
            }
            self.resets = r;
        }
        let (effects, worn) = match (in_game, class) {
            (true, Some(c)) => {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                let worn = crate::guest_build::h2_worn_all(&read, patch.char_slot);
                if P::wants_count(&worn) {
                    if self.count_polls % Self::COUNT_EVERY == 0 {
                        let h2 = crate::guest_build::hero2_java();
                        if let Some(n) = h2.and_then(|h2| crate::guest_build::h2_bag_equipment_count(&read, &h2)) {
                            self.counted = n;
                        }
                    }
                    self.count_polls = self.count_polls.wrapping_add(1);
                } else {
                    (self.counted, self.count_polls) = (0, 0);
                }
                let live = self.live(&*emulator, &worn, c, demon);
                (P::effects_for_live(&worn, c, self.counted, live).to_vec(), worn)
            }
            _ => {
                self.live.reset();
                (P::effects_none().to_vec(), Vec::new())
            }
        };
        if effects != self.effects && emulator.write_memory(block, &effects) {
            let ours: Vec<(u8, u8)> = worn.into_iter().filter(|(t, i)| P::item(*t, *i).is_some()).collect();
            log::info!("gear: effect words rewritten — worn options {ours:?} (class {class:?})");
            // ★★ 2026-10-14 the cooldown cuts for the plate ring (it reads the static table)
            let mut cuts = crate::GEAR_CD_CUT.lock().unwrap_or_else(|e| e.into_inner());
            for (c, row) in cuts.iter_mut().enumerate() {
                let at = (P::KCD_AT + u32::from(P::KINDS) * c as u32) as usize;
                row.copy_from_slice(&effects[at..at + usize::from(P::KINDS)]);
            }
            self.effects = effects;
        }
        reset
    }

    /// 용의 심장's buff for the HUD: the cell and its whole seconds left, `None` while it is off.
    fn dragon_buff(&self, emulator: &dyn Emulator) -> Option<crate::guest_build::H2BuffShow> {
        type P = crate::guest_build::GearPatch;
        let block = self.block?;
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let left = crate::guest_build::h2_gear_dragon_left(&read, block)?;
        let secs = left.div_ceil(crate::guest_build::H2_BUFF_FPS).min(u32::from(u16::MAX - 1)) as u16;
        Some(crate::guest_build::H2BuffShow { id: P::DRAGON_BUFF_ID, sheet: crate::atlas::HOST_SHEET, cell: crate::atlas::HOST_DRAGON_HEART, secs })
    }
}

/// ★★ 2026-10-17 (user: *"엘릭서 사용시 술꾼의 장화 옵션 발동도 되어야 된다는 말이야"*): an elixir drunk — from its popup's
/// 사용하기 or from a plate — starts 술꾼의 장화's 10 s walk exactly as a potion does. A potion's start is the gear block's
/// `a.g(B)V` wrapper (unit F: its count went down → DRUNK_LEFT ← W_DRUNK); the elixir is the host's and never passes
/// `a.g(B)V`, so the host writes the same word (`h2_gear_drink_write`). Nothing with the boots off or the block not in.
fn gear_drink(emulator: &mut dyn Emulator) {
    let block = crate::GEAR_BLOCK.load(std::sync::atomic::Ordering::Relaxed);
    if block == 0 {
        return;
    }
    let write = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_gear_drink_write(&read, block)
    };
    if let Some((a, bytes)) = write {
        let ok = emulator.write_memory(a, &bytes);
        log::info!("gear: 술꾼의 장화 — the elixir starts the potion's walk ({} updates, ok={ok})", u32::from_le_bytes(bytes[..4].try_into().unwrap_or([0; 4])));
    }
}

/// ★★ 2026-10-02 스킬을 바꾸는 장신구 6종 (user: 광룡의 외침 · 바바리안의 의지 · 야수조련 목걸이 ·
/// 크루세이더 · 벽력일섬 · 극사의 목걸이 — HELL drops that change a skill while worn): one host block,
/// `guest_anchors::AccessoryPatch`. Installed once from the patch pass AFTER the SkillPower block has
/// settled — one of its pieces chains to whatever the rogue's damage definition holds then (that
/// block, or the shipped body) — all or nothing; a verify that fails puts every arm back. Then, with the HUD
/// poll, the record in equipment slot 2 and the class decide the block's ACTIVE word (0 = the shipped
/// game, byte for byte), and a changed stage ends a channel. Nothing is written to a record or a save.
struct Accessory {
    block: Option<u32>,
    /// What the rogue's damage definition held when the block went in (the piece's chain target).
    chain: u32,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// The ACTIVE word as last written, and the stage handle the last poll saw.
    active: u32,
    stage: u32,
    /// The skill-name swap in the menu's `bw.c` table: `(table, code, [(slot, the row's own
    /// handle)])` — the rows pinned while the host points their slots at the appended ones.
    names: Option<(u32, u32, Vec<(u32, u32)>)>,
    /// ★★ 2026-10-14 BOLT_SET as last written — 벽력일섬 + a 번개의 호흡 in the weapon or the off-hand slot.
    bolt: bool,
    /// ★★ 2026-10-15 GK_SP as last written — 극사의 목걸이 + 피의 연쇄 in the weapon / off-hand slot (`AccessoryPatch::chain_sp`).
    chain_sp: u32,
}

impl Accessory {
    fn new() -> Self {
        Self { block: None, chain: 0, reserved: None, refused: false, said: false, active: 0, stage: 0, names: None, bolt: false, chain_sp: 0 }
    }

    /// `ready` = the SkillPower block is in or will never be (its pieces are this one's chain targets:
    /// installed first, they would make SkillPower's own definition check refuse).
    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let Some(patch) = crate::guest_build::accessory_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::AccessoryPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("accessory: no guest block — the six accessories change nothing");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_accessory_chain(&read, &patch, block, None)
                .and_then(|chain| crate::guest_build::h2_accessory_install_writes(&read, &patch, block, chain).map(|w| (chain, w)))
        };
        let Some((chain, writes)) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("accessory: an arm does not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_accessory_arms(&patch, block, chain).unwrap_or_default();
        // 극사's target query (the cell, then the call site) — armed after the definition words
        let target = crate::guest_build::h2_accessory_target_arms(&patch, block).unwrap_or_default();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("accessory: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_accessory_installed(&read, &patch, block, chain)
        };
        if verified {
            self.block = Some(block);
            self.chain = chain;
            log::info!("accessory: the six skill accessories through {block:#x} ({} arms, chain {chain:#x}, verified=true)", arms.len());
            return;
        }
        // Back to shipped, the last arm first (the reverse of the arming order). The cell goes back resolved
        // (the definition's address) — what the game's helper writes there on the first call anyway.
        for (at, shipped, _) in arms.iter().chain(target.iter()).rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("accessory: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("accessory: the install did not verify ({done}/{} writes) — restored, no accessory changes a skill", writes.len());
    }

    /// The worn accessory → ACTIVE (outside the world, or with no class, nothing is on); a new stage
    /// ends a channel (the block also ends it itself when ACTIVE stops matching or the map has no
    /// fighting). Published for the plate ring (`crate::ACCESSORY`).
    fn poll(&mut self, emulator: &mut dyn Emulator, in_game: bool, class: Option<u8>) {
        type P = crate::guest_build::AccessoryPatch;
        let Some(block) = self.block else { return };
        let Some(patch) = crate::guest_build::accessory_patch() else { return };
        let (code, stage, bolt, chain_sp) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let code = match (in_game, class, crate::guest_build::h2_worn_accessory(&read, patch.char_slot)) {
                (true, Some(c), Some((t, id))) => P::code_for(t, id, c),
                _ => P::NONE,
            };
            // ★★ 2026-10-14 일섬 「번개의 호흡」: one in either weapon slot (the user: one is enough)
            let blades: Vec<(usize, u8, u8)> = P::BOLT_SLOTS
                .iter()
                .filter_map(|&s| crate::guest_build::h2_worn_slot(&read, patch.char_slot, s).map(|(t, id)| (s, t, id)))
                .collect();
            (code, crate::guest_build::h2_rd_u32(&read, patch.stage_slot).unwrap_or(0), P::bolt_set(code, &blades), P::chain_sp(code, &blades))
        };
        if code != self.active {
            if emulator.write_memory(block + P::ACTIVE, &code.to_le_bytes()) {
                log::info!("accessory: ACTIVE {} -> {code} (class {class:?})", self.active);
                self.active = code;
            }
        }
        if bolt != self.bolt && emulator.write_memory(block + P::BOLT_SET, &u32::from(bolt).to_le_bytes()) {
            log::info!("accessory: 번개의 호흡 lightning {}", if bolt { "on" } else { "off" });
            self.bolt = bolt;
        }
        if chain_sp != self.chain_sp && emulator.write_memory(block + P::GK_SP, &chain_sp.to_le_bytes()) {
            log::info!("accessory: 피의 연쇄 SP share {chain_sp}/256");
            self.chain_sp = chain_sp;
        }
        if stage != self.stage {
            if self.stage != 0 {
                emulator.write_memory(block + P::CHANNEL, &0u32.to_le_bytes());
            }
            self.stage = stage;
        }
        crate::ACCESSORY.store(self.active, std::sync::atomic::Ordering::Relaxed);
        self.poll_names(emulator, &patch);
    }

    /// 일격 → 일섬 (★★ 2026-10-17 → 「벽력일섬」 with a 번개의 호흡 too) · 급소가격 → 극사 while worn: the two row slots of the menu's class skill table
    /// (`bw.c`, rebuilt from `t/sk2` every time the menu opens) point at the asset's appended rows,
    /// and back when the accessory comes off while the menu is still up. The rows themselves are
    /// never written; the originals are pinned the way the ACCOUNT title swap pins its row (the slot
    /// was their only reference — unpinned, the collector frees them and the restore would plant a
    /// dead handle in a table the collector walks).
    fn poll_names(&mut self, emulator: &mut dyn Emulator, patch: &crate::guest_build::AccessoryPatch) {
        type P = crate::guest_build::AccessoryPatch;
        let table = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_skill_table(&read, patch).unwrap_or(0)
        };
        // ★★ 2026-10-17 벽력일섬 + 번개의 호흡: 「벽력일섬」's rows (`AccessoryPatch::name_key`)
        let key = P::name_key(self.active, self.bolt);
        let wanted = P::SKILL_NAME_SWAPS.iter().any(|s| s.0 == key);
        if let Some((t, code, slots)) = self.names.take() {
            if t == table && code == key {
                self.names = Some((t, code, slots));
                return;
            }
            // The accessory changed with the menu still up: the rows go back. The menu closed: the
            // table is garbage with it (`bw.a(Z)V` nulls `bw.c`) — only the pins end.
            for (addr, original) in &slots {
                if t == table {
                    java_write(emulator, crate::guest_build::JavaWrite::TableRowRef, *addr, &original.to_le_bytes());
                }
                emulator.release_java_object(*original);
            }
            if t == table {
                Self::repaint_chain(emulator);
                log::info!("accessory: skill names put back (code {code})");
            }
        }
        if table == 0 || !wanted {
            return;
        }
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_skill_name_swaps(&read, patch, table, key)
        };
        let Some(plan) = plan else { return };
        let mut done = Vec::with_capacity(plan.len());
        for (addr, original, replacement) in plan {
            if !emulator.pin_java_object(original) {
                break;
            }
            if java_write(emulator, crate::guest_build::JavaWrite::TableRowRef, addr, &replacement.to_le_bytes()) {
                done.push((addr, original));
            } else {
                emulator.release_java_object(original);
                break;
            }
        }
        if !done.is_empty() {
            Self::repaint_chain(emulator);
            log::info!("accessory: {} skill name row(s) swapped (key {key:#x})", done.len());
            self.names = Some((table, key, done));
        }
    }

    /// The window chain repaints (a window paints only while its `j.e` is set).
    fn repaint_chain(emulator: &mut dyn Emulator) {
        let Some(h2) = crate::guest_build::hero2_java() else { return };
        let writes = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_skill_name_repaint_writes(&read, &h2)
        };
        for (addr, bytes) in writes {
            java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, addr, &bytes);
        }
    }
}

// ★ 일섬's afterimages ask this emulator for translucency with a tagged `Graphics.setAlpha` — the
// block's tag (a dependency-free constant) is the canvas's.
const _: () = assert!(crate::guest_build::AccessoryPatch::ALPHA_TAG == wie_backend::canvas::DRAW_ALPHA_TAG);
const _: () = assert!(crate::guest_build::SkyDrawPatch::STROKE_TAG == wie_backend::canvas::STROKE_TAG);
const _: () = assert!(crate::guest_build::SkyDrawPatch::STROKE_SOFT_TAG == wie_backend::canvas::STROKE_SOFT_TAG);

// ★ A3b: the costume book (`avatar_state`, dependency-free) repeats the override's part list —
// the wish it builds is read slot by slot against `AvatarPatch::PARTS`.
const _: () = {
    let (a, b) = (crate::avatar_state::COSTUME_PARTS, crate::guest_build::AvatarPatch::PARTS);
    let mut i = 0;
    while i < a.len() {
        assert!(a[i] == b[i], "avatar_state::COSTUME_PARTS must equal AvatarPatch::PARTS");
        i += 1;
    }
    // ★ A3c: and the avatar slot that draws each of those parts — the book's (`SLOT_OF_PART`,
    // what the override is built from) and the equipment window's (`H2_AV_PART_SLOT`).
    let (s, t) = (crate::avatar_state::SLOT_OF_PART, crate::guest_build::H2_AV_PART_SLOT);
    let mut j = 0;
    while j < s.len() {
        assert!(s[j] == t[j], "avatar_state::SLOT_OF_PART must equal hero2java::H2_AV_PART_SLOT");
        j += 1;
    }
    assert!(crate::avatar_state::SLOTS == crate::guest_build::H2_AV_SLOTS);
    // ★ A5: the quest slots the book marks are the ones the reader reads.
    assert!(crate::avatar_state::QUESTS == crate::guest_build::H2_QUESTS);
};

/// ★★ 의상 아바타 (2026-09-30, HERO2_AVATAR_PLAN.md A0/A1): the parts-loader override — the
/// character's body / arms / legs sheets picked by the host, loaded and drawn by the game
/// itself. See `guest_anchors::AvatarPatch`. Installed once from the patch pass like
/// [`Venom`] (all or nothing, every arm put back on a failed verify); the override words
/// follow `wanted`, and a change while the character is in the world asks the game's own
/// `cf.a(I)V` to reload the parts that changed (`JavaCall::PartsReload`, one call in flight
/// at a time). Before the world is entered nothing is reloaded: `cf.a()V` reads the words
/// when it loads every part on the way in.
///
/// ⛔ 2026-10-01 (user: "아바타는 이제 아바타옷장에서 입은거로만 적용되어야해"): `wanted` is
/// only ever the costume the character in the world WEARS from the costume window
/// (`AvatarBook::worn`). The A0/A1 test file `wie_avatar.txt`, which overrode every character
/// on the phone, is no longer read.
struct Avatar {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// The parts the host wants (`AvatarPatch::PARTS` order) — made safe for the class and
    /// the jar only when written (`h2_avatar_resolve`).
    wanted: [crate::guest_build::AvatarPart; crate::guest_build::AvatarPatch::N],
    /// The slots the block holds, as the host last wrote them.
    applied: Option<[crate::guest_build::AvatarWord; crate::guest_build::AvatarPatch::N]>,
    /// Steps still to run (a part to reload, ★ 2026-10-01 or to clear, the sparkle, the swing
    /// sound), and the one in flight `(call, step, since)`.
    queue: Vec<crate::guest_build::H2PartOp>,
    call: Option<(Arc<wie_backend::GuestCallResult>, crate::guest_build::H2PartOp, Instant)>,
    /// ★ 2026-10-01: the equipment the EQUIPMENT window's character was last refreshed to —
    /// the window changes the records at once and reloads the parts only on 「적용」 예, so the
    /// host runs the game's own per-slot refresh for each change (`hero2java::H2EquipLook`).
    look: crate::guest_build::H2EquipLook,
    /// ★ A3b: parts whose words changed while the character was NOT in the world. The world
    /// entry's `cf.a()V` may have read them before the change (a save's costume and class are
    /// known on the same frames the load runs), so they are reloaded once the world is there
    /// — a reload of words it already used costs one call and changes nothing.
    pre_world: Vec<u8>,

}

impl Avatar {
    fn new() -> Self {
        let wanted = crate::guest_build::AVATAR_PASS;
        Self { block: None, reserved: None, refused: false, said: false, wanted, applied: None, queue: Vec::new(), call: None, look: Default::default(), pre_world: Vec::new() }
    }

    /// ★ A2: the parts the loader should use now — the worn costume of the character in the
    /// world. A change is written and reloaded by the next [`Self::poll`].
    fn set_wanted(&mut self, wanted: [crate::guest_build::AvatarPart; crate::guest_build::AvatarPatch::N]) {
        self.wanted = wanted;
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::avatar_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::AvatarPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("avatar: no guest block — the equipment's own look");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_avatar_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("avatar: a region or branch of cf.a(I)V does not read as shipped — not installed yet");
            }
            return;
        };
        let arms = crate::guest_build::h2_avatar_arms(&patch, block);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("avatar: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_avatar_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            self.applied = Some(crate::guest_build::AVATAR_PASS_WORDS);
            log::info!("avatar: the parts loader takes the host's body / arms / legs / head / weapon / off-hand — through {block:#x} (verified=true)");
            return;
        }
        // Back to shipped, the branches first (the reverse of the arming order).
        for (at, shipped, _) in arms.iter().rev() {
            if !emulator.write_memory(*at, shipped) {
                log::error!("avatar: could not restore {at:#x}");
            }
        }
        self.refused = true;
        log::error!("avatar: the install did not verify ({done}/{} writes) — restored, the equipment's own look", writes.len());
    }

    /// Every 5 frames: the step in flight, then the words (the wish resolved for the
    /// character's `class` and the jar's `sheets`), then — ★ 2026-10-01 — the equipment the
    /// EQUIPMENT window (`equip_window` = it is up) changed, then the next step.
    fn poll(&mut self, emulator: &mut dyn Emulator, in_game: bool, equip_window: bool, class: Option<u8>, sheets: &crate::guest_build::AvatarSheets) {
        use crate::guest_build::H2PartOp;
        let Some(block) = self.block else { return };
        if let Some((out, op, since)) = self.call.take() {
            match out.poll() {
                None => {
                    self.call = Some((out, op, since));
                    return;
                }
                Some(Ok(_)) => log::info!("avatar: {op:?} done by the game's own cf ({} ms)", since.elapsed().as_millis()),
                Some(Err(())) => {
                    log::error!("avatar: {op:?} FAILED — the look is right again on the next load");
                    self.queue.clear();
                }
            }
        }
        let words = crate::guest_build::h2_avatar_resolve(&self.wanted, class, sheets);
        if self.applied != Some(words) {
            let changed: Vec<u8> = crate::guest_build::AvatarPatch::PARTS
                .iter()
                .enumerate()
                .filter(|(i, _)| self.applied.is_none_or(|a| a[*i] != words[*i]))
                .map(|(_, p)| *p)
                .collect();
            let mut ok = true;
            for (a, bytes) in crate::guest_build::h2_avatar_word_writes(block, &words) {
                ok &= emulator.write_memory(a, &bytes);
            }
            if !ok {
                log::warn!("avatar: the override words could not be written");
                return;
            }
            self.applied = Some(words);
            log::info!(
                "avatar: body {:?} · arms {:?} · legs {:?} · head {:?} · weapon {:?} · off-hand {:?} — (sprite, palette, type), -1 = the equipment's own (class {class:?})",
                words[0], words[1], words[2], words[3], words[4], words[5]
            );
            if in_game {
                for p in changed {
                    if !self.queue.contains(&H2PartOp::Reload(p)) {
                        self.queue.push(H2PartOp::Reload(p));
                    }
                }
            } else {
                for p in changed {
                    if !self.pre_world.contains(&p) {
                        self.pre_world.push(p);
                    }
                }
            }
        }
        if in_game && !self.pre_world.is_empty() {
            for p in std::mem::take(&mut self.pre_world) {
                if !self.queue.contains(&H2PartOp::Reload(p)) {
                    self.queue.push(H2PartOp::Reload(p));
                }
            }
        }
        // ★ 2026-10-01 (user: "장비 창에서 캐릭터가 착용 장비에 따라서 외형도 같이 보여졌으면"): the
        // EQUIPMENT window swaps records at once and reloads the parts only on 「적용」 예 — so
        // each change it makes gets the game's own per-slot refresh now, and the first look
        // after it closes catches a 아니오 that put things back (`hero2java::H2EquipLook`).
        let records = if in_game {
            crate::guest_build::hero2_java().and_then(|h2| {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_equip_records(&read, &h2)
            })
        } else {
            None
        };
        let slots = self.look.look(in_game && equip_window, records);
        if let (false, Some(now), Some(c)) = (slots.is_empty(), records, class) {
            let second = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_weapon_trait(&read, now[crate::guest_build::Hero2Java::EQUIP_OFFHAND_SLOT])
            };
            let ops = crate::guest_build::h2_equip_refresh(&slots, &now, c, second);
            log::info!("avatar: the EQUIPMENT window changed slot(s) {slots:?} — the game's own refresh {ops:?}");
            for op in ops {
                if !self.queue.contains(&op) {
                    self.queue.push(op);
                }
            }
        }
        // ⛔ A part this class never draws is never reloaded: the berserker's animation has no
        // part 7, and `cf.a(I)V(7)` over whatever sits in slot 5 then throws (probe, a class-0
        // bench holding a dagger there). Its words only ever pass through for that class.
        if let Some(c) = class {
            self.queue.retain(|op| op.part().is_none_or(|p| !crate::guest_build::AvatarPatch::typed(p) || crate::guest_build::AvatarPatch::path_type(p, c).is_some()));
        }
        if self.call.is_none() && in_game {
            if let Some(op) = self.queue.first().copied() {
                let (what, arg) = op.call();
                match java_call(emulator, what, JavaThis::None, &[wie_backend::JavaArg::Int(arg)]) {
                    Some(out) => {
                        self.queue.remove(0);
                        self.call = Some((out, op, Instant::now()));
                    }
                    None => {
                        log::error!("avatar: the {op:?} call was refused — the look is right again on the next load");
                        self.queue.clear();
                    }
                }
            }
        }
    }
}

/// ★★ 의상 아바타 A2 (2026-09-30): the app's side of `avatar_state::AvatarBook` — the live
/// `/g` files and the records under the app root, the vault's edge counters and journal.
struct AppAvatarIo {
    data_dir: PathBuf,
}

impl crate::avatar_state::AvatarIo for AppAvatarIo {
    fn live_g(&self, slot: u8) -> Option<Vec<u8>> {
        crate::vault::read_live_g(&self.data_dir, slot)
    }
    fn files(&self) -> Option<crate::avatar_state::AvatarFiles> {
        crate::vault::avatar_files(&self.data_dir)
    }
    fn g_edges(&self, slot: u8) -> u64 {
        crate::vault::g_edges(slot)
    }
    fn publish(&self, working: Option<&crate::avatar_state::AvatarState>) {
        crate::vault::publish_avatar_working(working);
    }
    fn rearm(&self, slot: u8) {
        crate::vault::rearm_avatar_journal(slot);
    }
    fn note(&self, warn: bool, line: String) {
        if warn {
            log::warn!("{line}");
        } else {
            log::info!("{line}");
        }
    }
}

/// ★★ 의상 아바타 A3 (2026-09-30) · ★ A3c (2026-10-01): the EQUIPMENT window's avatar side — the
/// character in the middle, the four avatar cells (투구 · 갑옷 · 신발 · 무기) on the right, a
/// cell's wardrobe over the panel. The model is `hero2java::H2EquipAvatar`, the picture
/// `h2_equip_avatar_patch` (the probe runs both); this is the worker's half: it follows the
/// window, switches the game's cursor frame off while the avatar side has the focus, takes the
/// keys and taps that are the host's, wears what a wardrobe picks in that slot
/// (`AvatarBook::wear_instance` — the parts then reload through [`Avatar`]), and sends the
/// picture whenever what it is drawn from changes.
///
/// Installed from the patch pass once the parts loader is: the four `adds` that move the cells
/// (`guest_anchors::equip_cell_shift`) and the cursor switch (`EquipCursorPatch`) together, all
/// or nothing — a window with its cells moved and no way to hide its cursor would be worse
/// than the shipped one.
struct EquipAvatar {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    model: crate::guest_build::H2EquipAvatar,
    /// The switch's HIDE word as last written.
    hidden: bool,
    font: Option<crate::guest_build::H2Font>,
    icons: Option<(u32, crate::guest_build::H2Image)>,
    blob: Option<(u32, Vec<u8>)>,
    catalog: Vec<crate::guest_build::H2CostumeDef>,
    /// What the picture on screen was drawn from (`None` = no picture).
    shown: Option<u64>,
    /// The key whose DOWN this took — its UP must not reach the game either.
    swallow_up: Option<KeyCode>,
}

impl EquipAvatar {
    fn new(catalog: Vec<crate::guest_build::H2CostumeDef>) -> Self {
        Self { block: None, reserved: None, refused: false, said: false, model: Default::default(), hidden: false, font: None, icons: None, blob: None, catalog, shown: None, swallow_up: None }
    }

    fn install(&mut self, emulator: &mut dyn Emulator, loader_ready: bool) {
        if self.block.is_some() || self.refused || !loader_ready {
            return;
        }
        let sites = crate::guest_build::equip_cell_shift();
        let Some(patch) = crate::guest_build::equip_cursor_patch().filter(|_| !sites.is_empty()) else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::EquipCursorPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("equipment avatar: no guest block — the shipped EQUIPMENT window");
                    self.refused = true;
                    return;
                }
            },
        };
        let (hook, (shift, already, total)) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_equip_cursor_install_writes(&read, &patch, block), crate::guest_build::h2_halfword_writes(&read, sites))
        };
        let Some(hook) = hook.filter(|_| shift.len() + already == total) else {
            if !self.said {
                self.said = true;
                log::warn!("equipment avatar: cc.b(II)V's definition or a cell site does not read as shipped — not installed yet");
            }
            return;
        };
        // The block, then the cells, then the def word LAST (the arming write).
        let mut ok = emulator.write_memory(hook[0].0, &hook[0].1);
        for (a, bytes) in &shift {
            ok &= emulator.write_memory(*a, bytes);
        }
        ok &= emulator.write_memory(hook[1].0, &hook[1].1);
        let verified = ok && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let (todo, done, n) = crate::guest_build::h2_halfword_writes(&read, sites);
            crate::guest_build::h2_equip_cursor_installed(&read, &patch, block) && todo.is_empty() && done == n
        };
        if verified {
            self.block = Some(block);
            log::info!("equipment avatar: the cells moved {} px left, the cursor frame switchable through {block:#x} (verified=true)", crate::guest_build::Hero2Java::EQUIP_CELL_SHIFT);
            return;
        }
        // Back to shipped: the def word first (the frame is the game's again), then the cells.
        let mut back = emulator.write_memory(patch.def, &(patch.body | 1).to_le_bytes());
        for (site, shipped, _) in sites {
            back &= emulator.write_memory(*site, &shipped.to_le_bytes());
        }
        self.refused = true;
        log::error!("equipment avatar: the install did not verify — restored (ok={back}), the shipped EQUIPMENT window");
    }

    /// The window as the avatar side sees it — only once installed, only with the cells moved.
    fn view(&self, emulator: &dyn Emulator) -> Option<crate::guest_build::H2EquipView> {
        self.block?;
        let h2 = crate::guest_build::hero2_java()?;
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_equip_view(&read, &h2).filter(|v| v.shifted)
    }

    /// ★ A3c: the character's items and what each avatar slot wears, from its state.
    fn wardrobe(book: &crate::avatar_state::AvatarBook) -> crate::guest_build::H2Wardrobe {
        let Some(s) = book.state() else { return crate::guest_build::H2Wardrobe::default() };
        crate::guest_build::H2Wardrobe {
            owned: s.items.iter().map(|i| crate::guest_build::H2Owned { id: i.id, avatar: i.avatar, slot: i.slot, mode: i.mode }).collect(),
            worn: s.wear,
        }
    }

    /// The character's class (`ag.a:B`) — which weapon sheets it can wear, which weapon icon.
    fn class(emulator: &dyn Emulator) -> Option<u8> {
        let h2 = crate::guest_build::hero2_java()?;
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_char_class(&read, &h2)
    }

    /// The cursor frame follows the model: off while the avatar side has the focus. Written
    /// with the window's repaint flags — it repaints only when told to.
    fn sync_hide(&mut self, emulator: &mut dyn Emulator, view: Option<&crate::guest_build::H2EquipView>) {
        let Some(block) = self.block else { return };
        let want = self.model.hides_cursor();
        if want == self.hidden {
            return;
        }
        let (a, bytes) = crate::guest_build::h2_equip_cursor_hide_write(block, want);
        if !emulator.write_memory(a, &bytes) {
            log::warn!("equipment avatar: the cursor switch could not be written");
            return;
        }
        self.hidden = want;
        if let Some(v) = view {
            for (a, b) in crate::guest_build::h2_repaint_writes(&v.chain) {
                java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, a, &b);
            }
        }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, book: &crate::avatar_state::AvatarBook, sheets: &crate::guest_build::AvatarSheets) {
        if self.block.is_none() {
            return;
        }
        let view = self.view(&*emulator);
        self.model.sync(view.as_ref().map(|v| v.win.handle));
        self.sync_hide(emulator, view.as_ref());
        // Nothing over the window, or a game box the picture can go around.
        let Some(v) = view.filter(|v| v.over != Some(None)) else {
            if self.shown.take().is_some() {
                let _ = proxy.send_event(UserEvent::EquipOverlay(Vec::new()));
            }
            return;
        };
        let wardrobe = Self::wardrobe(book);
        let class = Self::class(&*emulator);
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let parts = crate::guest_build::h2_parts_handles(&read);
        let blob_handle = crate::guest_build::h2_static_handle(&read, crate::guest_build::Hero2Java::CLASS_ANIM_SLOT);
        // ★ A3c: the strip BEFORE the key, and its presence IN it — the four cells are its icons,
        // so a first picture drawn before the strip read must not be kept once it does.
        if self.icons.is_none() {
            self.icons = crate::guest_build::h2_item_icons(&read);
        }
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (v.panel, v.over, self.model, &parts, &wardrobe, class, blob_handle, self.icons.is_some()).hash(&mut h);
            h.finish()
        };
        if self.shown == Some(key) {
            return;
        }
        if self.font.is_none() {
            self.font = crate::guest_build::h2_font_from_guest(&read);
        }
        if blob_handle.is_some() && self.blob.as_ref().map(|b| b.0) != blob_handle {
            self.blob = crate::guest_build::h2_class_blob(&read);
        }
        let Some(font) = self.font.as_ref() else { return };
        // The armed stance, facing front, with the weapon and off-hand (★ A3b: a costume draws them).
        let (seq, frame, dir) = crate::guest_build::H2_EQUIP_POSE;
        let pose = self.blob.as_ref().and_then(|(_, b)| crate::guest_build::h2_hero_pose_in(&read, b, seq, frame, dir, crate::guest_build::H2_EQUIP_POSE_SKIP)).unwrap_or_default();
        let wearable = |a: u16, slot: u8| crate::guest_build::h2_av_wearable(sheets, a, slot, class);
        let scene = crate::guest_build::H2EquipScene {
            font,
            panel: v.panel,
            cover: v.over.flatten(),
            model: &self.model,
            catalog: &self.catalog,
            wardrobe: &wardrobe,
            wearable: &wearable,
            class,
            pose: &pose,
            icons: self.icons.as_ref().map(|i| &i.1),
        };
        let patch = crate::guest_build::h2_equip_avatar_patch(&scene);
        self.shown = Some(key);
        let _ = proxy.send_event(UserEvent::EquipOverlay(vec![patch]));
    }

    /// A key while the EQUIPMENT window is on top. `true` = taken (the game never sees it).
    #[allow(clippy::too_many_arguments)]
    fn key(
        &mut self,
        emulator: &mut dyn Emulator,
        proxy: &EventLoopProxy<UserEvent>,
        book: &mut crate::avatar_state::AvatarBook,
        io: &dyn crate::avatar_state::AvatarIo,
        sheets: &crate::guest_build::AvatarSheets,
        code: KeyCode,
        down: bool,
    ) -> bool {
        if !down {
            return self.swallow_up.take_if(|k| *k == code).is_some();
        }
        let Some(v) = self.view(&*emulator).filter(|v| v.over.is_none()) else { return false };
        self.model.sync(Some(v.win.handle));
        use crate::guest_build::H2Nav;
        // ⛔ The digits too: the game reads raw 2/8/4/6 as its arrows and 5 as OK (`cc.a(II)Z`,
        // `j.d/e(II)Z`), so with an avatar cell focused they would otherwise move — or open a
        // popup on — the HIDDEN game cursor (a keyboard, the portrait keypad, a pad override).
        // If the model passes, the key goes on to the game as the digit it was.
        let k = match code {
            KeyCode::UP | KeyCode::NUM2 => H2Nav::Up,
            KeyCode::DOWN | KeyCode::NUM8 => H2Nav::Down,
            KeyCode::LEFT | KeyCode::NUM4 => H2Nav::Left,
            KeyCode::RIGHT | KeyCode::NUM6 => H2Nav::Right,
            KeyCode::OK | KeyCode::NUM5 => H2Nav::Ok,
            KeyCode::CLEAR => H2Nav::Clr,
            _ => {
                // The wardrobe is modal: every other key goes with it.
                if self.model.list.is_some() {
                    self.swallow_up = Some(code);
                    return true;
                }
                return false;
            }
        };
        let wardrobe = Self::wardrobe(book);
        let class = Self::class(&*emulator);
        let wearable = |a: u16, slot: u8| crate::guest_build::h2_av_wearable(sheets, a, slot, class);
        let r = self.model.key(k, v.cursor, &wardrobe, &wearable, class);
        self.apply(emulator, proxy, book, io, sheets, &v, r, Some(code))
    }

    /// A tap on the window. `true` = taken.
    #[allow(clippy::too_many_arguments)]
    fn tap(
        &mut self,
        emulator: &mut dyn Emulator,
        proxy: &EventLoopProxy<UserEvent>,
        book: &mut crate::avatar_state::AvatarBook,
        io: &dyn crate::avatar_state::AvatarIo,
        sheets: &crate::guest_build::AvatarSheets,
        fx: u32,
        fy: u32,
    ) -> bool {
        let Some(v) = self.view(&*emulator).filter(|v| v.over.is_none()) else { return false };
        self.model.sync(Some(v.win.handle));
        let wardrobe = Self::wardrobe(book);
        let class = Self::class(&*emulator);
        let wearable = |a: u16, slot: u8| crate::guest_build::h2_av_wearable(sheets, a, slot, class);
        let had_focus = self.model.focus.is_some();
        let r = self.model.tap(fx as i32, fy as i32, v.panel, v.cursor, &wardrobe, &wearable, class);
        // The focus left an avatar cell for the game's cursor, which stays on the cell it had. A
        // tap on THAT cell would be the game's OK (`h2_tap`'s focus-or-OK) — but while an avatar
        // cell had the focus no game cell looked focused, so this tap only shows the cursor again.
        if had_focus && r == crate::guest_build::H2EquipKey::Pass {
            let same_cell = crate::guest_build::hero2_java().is_some_and(|h2| {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_tap(&read, &h2, fx, fy, crate::inv_grid_shift()) == crate::guest_build::H2Tap::Activate
            });
            if same_cell {
                self.sync_hide(emulator, Some(&v));
                self.poll(emulator, proxy, book, sheets);
                return true;
            }
        }
        self.apply(emulator, proxy, book, io, sheets, &v, r, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        emulator: &mut dyn Emulator,
        proxy: &EventLoopProxy<UserEvent>,
        book: &mut crate::avatar_state::AvatarBook,
        io: &dyn crate::avatar_state::AvatarIo,
        sheets: &crate::guest_build::AvatarSheets,
        view: &crate::guest_build::H2EquipView,
        r: crate::guest_build::H2EquipKey,
        code: Option<KeyCode>,
    ) -> bool {
        match r {
            // The game's — with the cursor frame back first when the focus just left the slot.
            crate::guest_build::H2EquipKey::Pass => {
                self.sync_hide(emulator, Some(view));
                false
            }
            crate::guest_build::H2EquipKey::Take { wear } => {
                if let Some((slot, id)) = wear {
                    if !book.wear_instance(io, slot, id) {
                        log::warn!("equipment avatar: wearing {id:?} in avatar slot {slot} was refused (not owned, not that slot's, or nobody in the world)");
                    }
                }
                self.sync_hide(emulator, Some(view));
                if let Some(c) = code {
                    self.swallow_up = Some(c);
                }
                self.poll(emulator, proxy, book, sheets);
                true
            }
        }
    }
}

struct BagPaging {
    /// The host block once the patch is installed (`InventoryPagePatch::BLOCK_LEN`).
    block: Option<u32>,
    /// The page the window is on. Reset to 0 when the window leaves the chain.
    page: u8,
    /// Whose window that page belongs to (the inventory's or the warehouse's handle).
    bag: Option<u32>,
    /// The warehouse bags, once the account state has made them (see `AccountState`).
    account_bags: [Option<u32>; 2],
    /// Last published view, so the UI hears only changes.
    sent: Option<Option<crate::BagPageView>>,
    /// `true` once install was attempted and refused — say it once, not every poll.
    refused: bool,
    /// ★ 2026-10-09: the item PICKER being paged (`hero2java::h2_picker_window`) — no code patch, so it pages even
    /// while the bag trampolines are refused.
    picker: Option<PickerPage>,
    /// ★★ 2026-10-20 the 사용하기 → `a.g(B)V` site (`guest_anchors::ItemUsePagePatch`): its block once in, and whether
    /// it was refused. Installed only on top of the paging block — it reads that block's context.
    use_block: Option<u32>,
    use_refused: bool,
}

/// ★ 2026-10-09 (user: *"재료가방창도 127칸으로 … 신규아이템 조합서들을 구성할꺼야"*): one item picker `k` over a list
/// longer than a page — the alchemist's recipe list above all, with 92 recipe books in the game. `full` is the list as
/// the game built it (the array is that long); `shown` what the host left in it (the page), so a poll tells "the same
/// picker on the page we put it on" from "a new picker, or the game rebuilt the list" by the bytes, not by a handle that
/// the collector may have handed to the next window.
struct PickerPage {
    win: u32,
    list: u32,
    full: Vec<u8>,
    page: u8,
    shown: Vec<u8>,
}

impl BagPaging {
    fn new() -> Self {
        Self { block: None, page: 0, bag: None, account_bags: [None, None], sent: None, refused: false, picker: None, use_block: None, use_refused: false }
    }

    /// ★ 2026-10-09: the paged picker's view — `Some` only while a picker over more than a page is in the chain. A new
    /// picker (or a list the game rebuilt) is read whole and put on page 0: its length word goes to 28, its bytes are
    /// already that page's. Every write through the gate; a refused write leaves the list as the game built it.
    fn poll_picker(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> Option<crate::BagPageView> {
        let (view, data) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let view = crate::guest_build::h2_picker_window(&read, h2);
            let data = view.as_ref().map(|v| read(v.base, v.len as usize));
            (view, data)
        };
        let (Some(view), Some(data)) = (view, data) else {
            self.picker = None;
            return None;
        };
        if data.len() != view.len as usize {
            self.picker = None;
            return None;
        }
        let same = self.picker.as_ref().is_some_and(|p| p.win == view.win.handle && p.list == view.list && p.shown == data);
        if !same {
            let pages = crate::guest_build::h2_picker_pages(data.len());
            let mut state = PickerPage { win: view.win.handle, list: view.list, full: data.clone(), page: 0, shown: data };
            if pages > 1 {
                let first = crate::guest_build::h2_picker_page(&state.full, 0).to_vec();
                let len_at = view.base - 4;
                if java_write(emulator, crate::guest_build::JavaWrite::PickerList, len_at, &(first.len() as u32).to_le_bytes()) {
                    log::info!("picker pages: a list of {} entries on {pages} pages (window {:#x}) — page 1 shown", state.full.len(), view.win.handle);
                    state.shown = first;
                } else {
                    log::warn!("picker pages: the list's length could not be written — the picker shows its first 28 of {}", state.full.len());
                }
            }
            self.picker = Some(state);
        }
        let p = self.picker.as_ref()?;
        let pages = crate::guest_build::h2_picker_pages(p.full.len());
        if pages < 2 {
            return None;
        }
        // The strip stands under a box the game hangs on the picker; another window over it hides it.
        if !view.on_top && view.cover.is_none() {
            return None;
        }
        Some(crate::BagPageView {
            page: p.page,
            pages,
            cells: crate::guest_build::Hero2Java::BAG_PAGE as u8,
            panel: view.panel,
            on_top: view.on_top,
            cover: view.cover,
            shift: crate::picker_grid_shift(),
            cell_y0: crate::guest_build::Hero2Java::PICKER_CELL_Y0,
        })
    }

    /// ★ 2026-10-09: put the paged picker on `page`, cursor on `cursor` — `false` when no paged picker is on top.
    fn picker_go(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, proxy: &EventLoopProxy<UserEvent>, page: u8, cursor: u8) -> bool {
        let view = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_picker_window(&read, h2)
        };
        let (Some(view), Some(p)) = (view, self.picker.as_mut()) else { return false };
        if !view.on_top || p.win != view.win.handle || p.list != view.list {
            return false;
        }
        let pages = crate::guest_build::h2_picker_pages(p.full.len());
        if pages < 2 {
            return false;
        }
        let page = page.min(pages - 1);
        let Some(writes) = crate::guest_build::h2_picker_page_writes(&view, &p.full, page, cursor) else { return false };
        let mut ok = true;
        for (what, a, bytes) in &writes {
            ok &= java_write(emulator, *what, *a, bytes);
        }
        let from = p.page;
        p.page = page;
        p.shown = crate::guest_build::h2_picker_page(&p.full, page).to_vec();
        log::info!("picker pages: page {} -> {} ({} write(s), ok={ok})", from + 1, page + 1, writes.len());
        let published = Some(crate::BagPageView {
            page,
            pages,
            cells: crate::guest_build::Hero2Java::BAG_PAGE as u8,
            panel: view.panel,
            on_top: true,
            cover: None,
            shift: crate::picker_grid_shift(),
            cell_y0: crate::guest_build::Hero2Java::PICKER_CELL_Y0,
        });
        self.sent = Some(published);
        let _ = proxy.send_event(UserEvent::BagPage(published));
        true
    }

    /// ★ 2026-10-09: LEFT/RIGHT at a paged picker's wrap points cross its pages (`h2_picker_arrow`).
    fn picker_arrow(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, proxy: &EventLoopProxy<UserEvent>, left: bool) -> bool {
        let Some(p) = self.picker.as_ref() else { return false };
        let (full, page) = (p.full.clone(), p.page);
        let view = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_picker_window(&read, h2)
        };
        let Some(view) = view.filter(|v| v.on_top) else { return false };
        let cursor = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            read(view.win.fields + 4 + crate::guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map(|b| *b as i8)
        };
        let Some((to, cell)) = cursor.and_then(|c| crate::guest_build::h2_picker_arrow(&full, page, c, left)) else { return false };
        log::info!("picker pages: {} on cell {cursor:?} crosses to page {} cell {cell}", if left { "LEFT" } else { "RIGHT" }, to + 1);
        self.picker_go(emulator, h2, proxy, to, cell)
    }

    /// Install the patch, once. Reserves the block, verifies every site and def, writes
    /// block → cells → branches. Refusal (a site that does not read as shipped) leaves
    /// the game exactly as it was: the plan is all-or-nothing before the first write.
    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::inventory_page_patch() else { return };
        let Some(block) = emulator.alloc_guest(crate::guest_build::InventoryPagePatch::BLOCK_LEN) else {
            log::warn!("bag pages: no guest block — paging OFF");
            self.refused = true;
            return;
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_page_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            log::warn!("bag pages: a site or a definition does not read as shipped — paging OFF (block {block:#x} left unused)");
            self.refused = true;
            return;
        };
        // ⛔ What every site held BEFORE, so a patch that does not fully land can be put
        // back (2026-09-21, review). "All-or-nothing" used to describe only the planning
        // step: past it, a failed write left the game half-patched — the cells already
        // redirected into a host block whose trampoline never arrived — and the code then
        // latched `refused` and returned, so the guest kept `bx`-ing into whatever was at
        // that address for the rest of the process. Refusal has to mean the game is as
        // shipped, because that is the state the rest of the host assumes when paging is
        // off.
        let originals: Option<Vec<(u32, Vec<u8>)>> = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            writes
                .iter()
                .map(|(a, bytes)| {
                    let cur = read(*a, bytes.len());
                    (cur.len() == bytes.len()).then_some((*a, cur))
                })
                .collect()
        };
        let Some(originals) = originals else {
            log::warn!("bag pages: a site could not be read back for rollback — paging OFF (block {block:#x} left unused)");
            self.refused = true;
            return;
        };
        // The plan is ordered block → cells → branches, so the arming `bl` is last and a
        // stop partway through has never armed anything.
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("bag pages: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        // Verify BOTH ways: the patch as the reader re-derives it from `patch` (the sites
        // and the cells), and every byte this function meant to write — which is the only
        // thing that covers the host block itself. `h2_page_installed` alone reads TRUE
        // for a trampoline that never landed.
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_page_installed(&read, &patch, block) && writes.iter().all(|(a, bytes)| read(*a, bytes.len()) == *bytes)
        };
        if verified {
            self.block = Some(block);
            log::info!("bag pages: patch installed at {block:#x} ({} write(s), verified=true)", writes.len());
            return;
        }
        let mut undone = true;
        for (a, bytes) in originals.iter().take(done) {
            undone &= emulator.write_memory(*a, bytes);
        }
        log::warn!(
            "bag pages: patch did not verify after {done}/{} write(s) — paging OFF, {} (block {block:#x} left unused)",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }

    /// ★★ 2026-10-20 the eighth site — 사용하기's `a.g(B)V(j.h)`, which the bag trampolines never saw: without it a
    /// potion on page 2..5 used page 1's record at that cell (nothing, or another potion). Once, after the paging block,
    /// verified, rolled back on a partial write — the same discipline as [`Self::install`].
    fn install_use(&mut self, emulator: &mut dyn Emulator) {
        if self.use_block.is_some() || self.use_refused {
            return;
        }
        let Some(page_block) = self.block else { return };
        let Some(patch) = crate::guest_build::item_use_page_patch() else { return };
        let Some(block) = emulator.alloc_guest(crate::guest_build::ItemUsePagePatch::BLOCK_LEN) else {
            log::warn!("bag pages: no guest block for the 사용하기 site — page 2..5 items cannot be used from the bag");
            self.use_refused = true;
            return;
        };
        let (plan, originals) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let plan = crate::guest_build::h2_use_page_install_writes(&read, &patch, page_block, block);
            let originals: Option<Vec<(u32, Vec<u8>)>> =
                plan.as_ref().and_then(|w| w.iter().map(|(a, bytes)| Some((*a, read(*a, bytes.len()))).filter(|(_, c)| c.len() == bytes.len())).collect());
            (plan, originals)
        };
        let (Some(writes), Some(originals)) = (plan, originals) else {
            log::warn!("bag pages: the 사용하기 site does not read as shipped — left alone (block {block:#x} unused)");
            self.use_refused = true;
            return;
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_use_page_installed(&read, &patch, block) && writes.iter().all(|(a, bytes)| read(*a, bytes.len()) == *bytes)
        };
        if verified {
            self.use_block = Some(block);
            log::info!("bag pages: 사용하기 site installed at {block:#x} ({} write(s), verified=true)", writes.len());
            return;
        }
        let mut undone = true;
        for (a, bytes) in originals.iter().take(done) {
            undone &= emulator.write_memory(*a, bytes);
        }
        log::warn!(
            "bag pages: the 사용하기 site did not verify after {done}/{} write(s) — {}",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.use_refused = true;
    }

    /// Did the patch land? The bag may only be raised above one page while it did —
    /// without the trampolines nothing can reach a record past cell 27.
    fn paging_on(&self) -> bool {
        self.block.is_some()
    }

    /// The paged bags: the inventory, the game's 재료가방 (★ 2026-10-09 — user: *"재료가방창도 127칸으로"*)
    /// and the ACCOUNT warehouse once it exists. The inventory stays first: with no window up the
    /// context names it at offset 0 ([`Self::poll`]).
    fn bags(&self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> Vec<u32> {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let mut bags = Vec::with_capacity(4);
        bags.extend(crate::guest_build::h2_static_handle(&read, h2.inventory_slot));
        bags.extend(crate::guest_build::h2_static_handle(&read, h2.material_bag_slot));
        bags.extend(self.account_bags.iter().flatten());
        bags
    }

    /// The window as it is now, and the context kept in step with it. `account_bags` =
    /// the warehouse's two handles once they exist — their windows are paged like the
    /// inventory's, and since 2026-10-09 so is the game's own 재료가방 `ag.b` (127, five pages).
    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, proxy: &EventLoopProxy<UserEvent>, account_bags: [Option<u32>; 2]) {
        // ★ 2026-10-09: a paged item picker owns the strip while it is up (it is modal over whatever opened it), and
        //   it needs no trampoline — so it is asked first, before the bag patch's early return.
        let picker_view = self.poll_picker(emulator, h2);
        let Some(block) = self.block else {
            if self.sent != Some(picker_view) {
                self.sent = Some(picker_view);
                let _ = proxy.send_event(UserEvent::BagPage(picker_view));
            }
            return;
        };
        self.account_bags = account_bags;
        let bags = self.bags(emulator, h2);
        let (view, cur) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_bag_window(&read, h2, &bags), crate::guest_build::h2_page_ctx(&read, block))
        };
        // The page belongs to ONE window: gone, or a different bag's, and it is page 0.
        match view {
            None => self.page = 0,
            Some(v) if self.bag != Some(v.bag) => {
                self.page = 0;
                self.bag = Some(v.bag);
            }
            Some(_) => {}
        }
        // With no window up the context names the inventory at offset 0 — the trampolines
        // then add nothing whichever bag is asked.
        let bag = view.map_or_else(|| bags.first().copied().unwrap_or(0), |v| v.bag);
        let want = crate::guest_build::h2_page_ctx_writes(block, bag, self.page);
        crate::BAG_PAGE_CTX.store(
            (u64::from(bag) << 32) | u64::from(u32::from(self.page) * crate::guest_build::Hero2Java::BAG_PAGE as u32),
            std::sync::atomic::Ordering::Relaxed,
        );
        let want_ctx = (bag, u32::from_le_bytes([want[1].1[0], want[1].1[1], want[1].1[2], want[1].1[3]]));
        if cur != Some(want_ctx) {
            for (a, bytes) in &want {
                emulator.write_memory(*a, bytes);
            }
            log::debug!("bag pages: ctx {cur:x?} -> {want_ctx:x?}");
        }
        // The strip belongs to the window the player is IN: the paged window itself, or
        // that window with one of the game's boxes up over it. Another window over it (the
        // game's 재료가방 over the inventory) hides it — the page it keeps is still that
        // window's. ★ 2026-09-29 (user: "가방에서 계정창고로 넣을떄나 뺄때 등 팝업이 왼쪽
        // 가방5개 리스트에 가려지고있어"): the box is ABOVE the strip, so where it is goes
        // with the view and the UI keeps the game's pixels there (`cover`). A box the host
        // cannot place hides the strip instead of drawing it over a guess.
        let shown = view.and_then(|v| {
            if v.on_top {
                return Some((v, None));
            }
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_modal_over(&read, h2, &v).map(|cover| (v, Some(cover)))
        });
        // ★ 2026-10-02: the window's cell count follows its page (15 on the last) — `go` writes it with
        //   every page change; this puts it back should anything have left it otherwise, and keeps the
        //   cursor inside it, so the game's own keys never reach a cell the page does not have.
        if let Some(v) = view {
            self.keep_cells(emulator, h2, &v);
        }
        let published = picker_view.or_else(|| {
            shown.map(|(v, cover)| crate::BagPageView {
                page: self.page,
                pages: crate::guest_build::Hero2Java::BAG_PAGES as u8,
                cells: crate::guest_build::h2_page_cells(self.page),
                panel: v.panel,
                on_top: v.on_top,
                cover,
                shift: crate::inv_grid_shift(),
                cell_y0: crate::guest_build::Hero2Java::INV_CELL_Y0,
            })
        });
        if self.sent != Some(published) {
            self.sent = Some(published);
            let _ = proxy.send_event(UserEvent::BagPage(published));
        }
    }

    /// The window if the strip is live: installed, a paged bag's window on top.
    fn live_view(&self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> Option<crate::guest_build::H2InvWin> {
        self.block?;
        let bags = self.bags(emulator, h2);
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_bag_window(&read, h2, &bags).filter(|v| v.on_top)
    }

    /// ★ A LEFT/RIGHT key-down while the inventory window is on top: a page crossing if
    /// the cursor is on the grid's first/last cell (`h2_page_arrow`), performed here and
    /// the key SWALLOWED; otherwise `false` and the key is the game's. The player asked for
    /// arrows as well as tabs — this is the on-screen arrows, the pad and a keyboard alike,
    /// since all of them arrive as this key.
    fn arrow(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, proxy: &EventLoopProxy<UserEvent>, left: bool) -> bool {
        // ★ 2026-10-09: a paged picker on top crosses its own pages.
        if self.picker_arrow(emulator, h2, proxy, left) {
            return true;
        }
        let Some(view) = self.live_view(emulator, h2) else { return false };
        // ⛔ The same reset `poll` applies, BEFORE `self.page` is consulted (2026-09-21,
        // review). `poll` runs every five frames, and the top `q` can change between two
        // of them — pressing OK on the 재료가방 record hangs a second window over the
        // warehouse main one, and `h2_bag_window` searches the chain in reverse, so
        // `live_view` returns the new window while `self.page` still describes the old.
        // A LEFT on cell 0 then crossed to "the previous page" of a bag that has one.
        if self.bag != Some(view.bag) {
            self.page = 0;
            self.bag = Some(view.bag);
        }
        let cursor = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            read(view.win.fields + 4 + crate::guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map(|b| *b as i8)
        };
        let Some(cursor) = cursor else { return false };
        let Some((page, cell)) = crate::guest_build::h2_page_arrow(self.page, cursor, left) else { return false };
        log::info!("bag pages: {} on cell {cursor} crosses to page {page} cell {cell}", if left { "LEFT" } else { "RIGHT" });
        self.go(emulator, h2, proxy, view, page, cell);
        true
    }

    /// ★ 2026-10-02: an UP/DOWN key-down while a paged window is on top of its SHORT last page and the game's
    /// own move would leave the column (`h2_page_vertical`) — the cursor is put in the column here and the key
    /// SWALLOWED. `false` = the game's key.
    fn vertical(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, up: bool) -> bool {
        let Some(view) = self.live_view(emulator, h2) else { return false };
        if self.bag != Some(view.bag) {
            return false; // a window `poll` has not seen yet is on its page 0 — a full page
        }
        let cursor = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            read(view.win.fields + 4 + crate::guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map(|b| *b as i8)
        };
        let Some(cell) = cursor.and_then(|c| crate::guest_build::h2_page_vertical(self.page, c, up)) else { return false };
        let writes = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let mut w = vec![(view.win.fields + 4 + crate::guest_build::Hero2Java::WIN_CURSOR_OFF, vec![cell])];
            if let Some(chain) = crate::guest_build::h2_window_chain(&read, h2) {
                w.extend(crate::guest_build::h2_repaint_writes(&chain));
            }
            w
        };
        let mut ok = true;
        for (a, bytes) in &writes {
            ok &= java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, *a, bytes);
        }
        log::info!("bag pages: {} on page {} cell {cursor:?} -> cell {cell} (the column; ok={ok})", if up { "UP" } else { "DOWN" }, self.page);
        true
    }

    /// The window's `j.i` and cursor kept to its page's cells ([`Self::poll`]): writes only what differs.
    fn keep_cells(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, view: &crate::guest_build::H2InvWin) {
        let cells = crate::guest_build::h2_page_cells(self.page);
        let (count_at, cursor_at) = (crate::guest_build::h2_page_cells_write(view, self.page).0, view.win.fields + 4 + crate::guest_build::Hero2Java::WIN_CURSOR_OFF);
        // Every frame the window is up: two bytes, and the chain only when there is something to put back.
        let (count, cursor) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (read(count_at, 1).first().copied(), read(cursor_at, 1).first().map(|b| *b as i8))
        };
        let mut writes: Vec<(crate::guest_build::JavaWrite, u32, Vec<u8>)> = Vec::new();
        if count.is_some_and(|c| c != cells) {
            writes.push((crate::guest_build::JavaWrite::BagPageCells, count_at, vec![cells]));
        }
        if cursor.is_some_and(|c| c >= cells as i8) {
            writes.push((crate::guest_build::JavaWrite::WidgetCursor, cursor_at, vec![cells.max(1) - 1]));
        }
        if writes.is_empty() {
            return;
        }
        let chain = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_window_chain(&read, h2)
        };
        for (a, bytes) in chain.as_deref().map(crate::guest_build::h2_repaint_writes).unwrap_or_default() {
            writes.push((crate::guest_build::JavaWrite::WidgetCursor, a, bytes));
        }
        let mut ok = true;
        for (what, a, bytes) in &writes {
            ok &= java_write(emulator, *what, *a, bytes);
        }
        log::warn!("bag pages: page {}'s window read {count:?} cell(s), cursor {cursor:?} — put back to {cells} (ok={ok})", self.page);
    }

    /// Put the inventory window on the page that holds a given record, cursor on its
    /// cell — the item-use drive's way in, since it knows a record only by its absolute
    /// index in the bag. Goes through [`Self::go`] so the pager's own page, the context
    /// words and the on-screen strip stay in step; writing the context behind its back
    /// would be undone by the next five-frame poll.
    fn page_to(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, proxy: &EventLoopProxy<UserEvent>, page: u8, cell: u8) {
        let Some(view) = self.live_view(emulator, h2) else { return };
        if self.bag != Some(view.bag) || self.page != page {
            self.go(emulator, h2, proxy, view, page, cell);
        }
    }

    /// The player tapped tab `page`.
    fn select(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, proxy: &EventLoopProxy<UserEvent>, page: u8) {
        // ★ 2026-10-09: the strip is a paged picker's while one is on top.
        if self.picker_go(emulator, h2, proxy, page, 0) {
            return;
        }
        if self.block.is_none() {
            log::info!("bag pages: tab {page} tapped but paging is not installed");
            return;
        }
        let Some(view) = self.live_view(emulator, h2) else {
            log::info!("bag pages: tab {page} tapped but the inventory window is not on top — ignored");
            return;
        };
        self.go(emulator, h2, proxy, view, page, 0);
    }

    /// Put the window on `page` with its cursor on `cursor`, and tell the UI.
    fn go(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, proxy: &EventLoopProxy<UserEvent>, view: crate::guest_build::H2InvWin, page: u8, cursor: u8) {
        let Some(block) = self.block else { return };
        let page = page.min(crate::guest_build::Hero2Java::BAG_PAGES as u8 - 1);
        let writes = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_page_select_writes(&read, h2, block, &view, page, cursor)
        };
        // The two context words are HOST memory; the cursor and the repaint flags are
        // the window's Java fields and go through the gate like every other cursor write.
        // ★ 2026-10-02: and the page's cell count (`j.i`, 15 on the short last page) is its own kind.
        let cells_at = crate::guest_build::h2_page_cells_write(&view, page).0;
        let mut ok = true;
        for (a, bytes) in &writes {
            if (block..block + crate::guest_build::InventoryPagePatch::BLOCK_LEN).contains(a) {
                ok &= emulator.write_memory(*a, bytes);
            } else if *a == cells_at {
                ok &= java_write(emulator, crate::guest_build::JavaWrite::BagPageCells, *a, bytes);
            } else {
                ok &= java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, *a, bytes);
            }
        }
        self.page = page;
        self.bag = Some(view.bag);
        crate::BAG_PAGE_CTX.store((u64::from(view.bag) << 32) | u64::from(u32::from(page) * crate::guest_build::Hero2Java::BAG_PAGE as u32), std::sync::atomic::Ordering::Relaxed);
        log::info!("bag pages: page {} -> {page} ({} write(s), ok={ok})", self.sent.flatten().map_or(0, |v| v.page), writes.len());
        // Publish now rather than on the next poll: the strip should move with the tap.
        let published = Some(crate::BagPageView {
            page,
            pages: crate::guest_build::Hero2Java::BAG_PAGES as u8,
            cells: crate::guest_build::h2_page_cells(page),
            panel: view.panel,
            on_top: true,
            cover: None,
            shift: crate::inv_grid_shift(),
            cell_y0: crate::guest_build::Hero2Java::INV_CELL_Y0,
        });
        self.sent = Some(published);
        let _ = proxy.send_event(UserEvent::BagPage(published));
    }
}

/// ★★★ 2026-09-23: put 「기본공격」 back when a save carries the skill-reset bug.
///
/// The 망각의 구 스킬 as first shipped zeroed every positive slot of `a.p:[B` — but the
/// leading `a.i[class]` of those are combo STAGES, not skills (see
/// `guest_anchors::Hero2Java::char_combo_open_off`). Stage 1 is a level the game hands
/// out at creation, so a character that spent one of those orbs came back with `a.h = 0`,
/// a state the game has no way to produce and which the player reported as a crash on the
/// next point spent. The planner side is fixed; this is for the saves already made.
///
/// ⛔ This is the only save-bearing write in this host that happens without the player
/// asking for it, so it is kept as narrow as it can be: one array element back to 1 plus
/// the derived count beside it, only while a character is in the world, and only while
/// the array itself says the invariant is broken. `h2_combo_repair` is idempotent by
/// CONDITION — once stage 1 reads `>= 1` it plans nothing — so there is no latch here
/// that could go stale across a save-slot load, and a character that never met the bug
/// never has anything written to it.
struct ComboRepair {
    /// The toast, once per run. The repair itself may be replanned as often as it likes.
    said: bool,
    /// The refusal warning, once: this poll comes back every 5 frames, and an ungated
    /// `log::warn!` would be two lines a second (the lesson from #18's patch pass).
    warned: bool,
}

impl ComboRepair {
    fn new() -> Self {
        Self { said: false, warned: false }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, _proxy: &EventLoopProxy<UserEvent>) {
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_combo_repair(&read, h2)
        };
        let Some(plan) = plan else { return };
        for (a, bytes) in &plan.writes {
            if !java_write(emulator, crate::guest_build::JavaWrite::CharCombo, *a, bytes) {
                if !self.warned {
                    self.warned = true;
                    log::warn!("combo repair: the write at {a:#x} was refused — 기본공격 stays unlearned");
                }
                return;
            }
        }
        log::info!("combo repair: 기본공격 stage 1 restored ({} write(s))", plan.writes.len());
        if !self.said {
            self.said = true;
            host_notice("기본공격복구");
        }
    }
}

/// ★★ 2026-10-05 로그 「일풍격」: one block — the sixteenth skill slot, the quickbar value 7 ↔ kind 15, its SP /
/// cooldown / max level, the SKILL window's seventh cell, the plan (`ay.b(B)Z`) and the drive (`ay.a()Z`). The
/// usable / drive definitions CHAIN to what they hold at install, so this goes in once the accessory block has
/// settled (installed or refused). All or nothing; a failed verify puts every landed arm back as it read.
/// See `guest_anchors::IlpungPatch` / HERO2_GUEST_RE.md §8.48.
struct Ilpung {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl Ilpung {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let (Some(patch), Some(acc)) = (crate::guest_build::ilpung_patch(), crate::guest_build::accessory_patch()) else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::IlpungPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("일풍격: no guest block — the rogue keeps six actives");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let arms = crate::guest_build::h2_ilpung_chains(&read, &patch).and_then(|c| crate::guest_build::h2_ilpung_arms(&patch, block, c));
            crate::guest_build::h2_ilpung_install_writes(&read, &patch, &acc, block).zip(arms)
        };
        let Some((writes, arms)) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("일풍격: the rogue's definitions / the SKILL window's call sites do not read as shipped — not installed");
            }
            return;
        };
        // Every arm as it reads now — put back if the install does not verify.
        let originals: Vec<(u32, Vec<u8>)> = arms.iter().map(|(a, shipped, _)| (*a, emulator.read_memory(*a, shipped.len()))).collect();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("일풍격: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_ilpung_installed(&read, &patch, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!("일풍격: the rogue's seventh active through {block:#x} (verified=true, {} write(s)) — a.p[15], value 7 = kind 15", writes.len());
            return;
        }
        let mut undone = true;
        for (a, bytes) in originals.iter().rev() {
            if writes[..done].iter().any(|(w, _)| w == a) {
                undone &= emulator.write_memory(*a, bytes);
            }
        }
        log::warn!("일풍격: did not verify after {done}/{} write(s) — {}", writes.len(), if undone { "every arm put back" } else { "an arm could NOT be put back" });
        self.refused = true;
    }
}

/// ★★ 2026-10-07 버서커 분노태세 ON/OFF · 「도발」: one block — the berserker's SP / cooldown / max-level / cast-drive /
/// damage wrappers, a plain monster's update while a taunt runs, and the TAKEN function the combat block's 철괴 hook calls —
/// plus the two halfwords that take the shipped rage's ×2 attack · ×3/5 defence out. Every definition word CHAINS to what
/// it holds at install (the SkillPower and accessory blocks' wrappers), so this goes in once those, the combat block and
/// the 일풍격 block (the window / quickbar side of the sixteenth slot) have settled. All or nothing; a failed verify puts
/// every landed arm back. See `guest_anchors::BerserkerPatch` / HERO2_GUEST_RE.md §8.52.
struct Berserker {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl Berserker {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    /// `ready`: the blocks it chains onto have settled. `combat`: the combat block, whose 철괴 piece carries the hook word.
    /// ★ 2026-10-07 (2) `accessory`: the accessory block, whose 폭풍강타 channel reads its period from a word
    /// (`AccessoryPatch::STORM_PULSE_W`) that 격분 at 100 % shortens (`None` = no accessory block: the explosion skips it).
    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool, combat: Option<u32>, accessory: Option<u32>) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let (Some(patch), Some(cp)) = (crate::guest_build::berserker_patch(), crate::guest_build::combat_patch()) else {
            self.refused = true;
            return;
        };
        let rage_off = crate::guest_build::rage_shipped_off();
        let hook = combat.and_then(|c| crate::guest_build::h2_iron_hook_at(&cp).map(|h| c + h));
        let acc_pulse = accessory.map(|b| b + crate::guest_build::AccessoryPatch::STORM_PULSE_W);
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::BerserkerPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("berserker: no guest block — 분노태세 stays a timed ×2 and 도발 does nothing");
                    self.refused = true;
                    return;
                }
            },
        };
        // ★ 2026-10-09 the host's 「그아앗!」 · 「갈!」 take the place of the game's 기합 — when both clips decode.
        let voice = [crate::guest_build::H2Voice::FuryStart, crate::guest_build::H2Voice::FuryBlast]
            .iter()
            .all(|v| crate::voice::voice_clip(v.index()).is_some());
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_berserker_chain(&read, &patch, block).and_then(|chain| {
                crate::guest_build::h2_berserker_install_writes(&read, &patch, block, rage_off, hook, acc_pulse, voice).map(|w| (chain, w))
            })
        };
        let Some((chain, writes)) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("berserker: a definition is not a Thumb entry or a rage halfword / the hook word reads foreign — not installed");
            }
            return;
        };
        let arms = crate::guest_build::h2_berserker_arms(&patch, block, &chain, rage_off, hook);
        let originals: Vec<(u32, Vec<u8>)> = arms.iter().map(|(a, before, _)| (*a, emulator.read_memory(*a, before.len()))).collect();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("berserker: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_berserker_installed(&read, &patch, block, &chain, rage_off, hook)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            crate::BERSERKER_ON.store(true, std::sync::atomic::Ordering::Relaxed);
            log::info!(
                "berserker: 분노태세 ON/OFF (1 s), 「도발」 and 「격분」 through {block:#x} — {} write(s), the taken hook {}, the channel's period word {} (verified=true)",
                writes.len(),
                if hook.is_some() { "in" } else { "MISSING (no combat block)" },
                if acc_pulse.is_some() { "in" } else { "MISSING (no accessory block)" }
            );
            return;
        }
        let mut undone = true;
        for (a, bytes) in originals.iter().rev() {
            if writes[..done].iter().any(|(w, _)| w == a) {
                undone &= emulator.write_memory(*a, bytes);
            }
        }
        log::warn!("berserker: did not verify after {done}/{} write(s) — {}", writes.len(), if undone { "every arm put back" } else { "an arm could NOT be put back" });
        self.refused = true;
    }
}

/// ★ 2026-10-05 일풍격: a ROGUE whose 번개난타 (`a.p[10]`, the quest 「터줏대감」) is open gets `a.p[15]` opened
/// too — the slot the quest's script cannot name (a build without the patch has 15 slots and would throw). Same
/// kind of write as `IaidoUnlock`, polled beside it. ★ 2026-10-07 (2): every slot the host opens
/// (`hero2java::h2_extra_slot_unlocks`) — the sixteenth (일풍격 · 도발) and the class's AWAKENING (격분 · 천공의검 · 개화)
/// at LV 70 or more (user: *"각성기는 70레벨이 넘으면 얻을 수 있고"*).
struct IlpungUnlock {
    said: bool,
    warned: bool,
}

impl IlpungUnlock {
    fn new() -> Self {
        Self { said: false, warned: false }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) {
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_extra_slot_unlocks(&read, h2)
        };
        for (at, value) in plan {
            if !java_write(emulator, crate::guest_build::JavaWrite::CharSkillUnlock, at, &[value]) {
                if !self.warned {
                    self.warned = true;
                    log::warn!("slot unlock: the write at {at:#x} was refused — the slot stays locked");
                }
                return;
            }
            if !self.said {
                self.said = true;
                log::info!("slot unlock: an a.p slot opened at {at:#x} (the 16th: 로그 번개난타 → 일풍격 · 버서커 사자후 → 도발; the awakening at LV 70; ★ the rogue's 암살 a.p[17])");
            }
        }
    }
}

/// ★ 2026-10-07 (2) (user: *"몸통치기를 없앨꺼고"*): a BERSERKER whose 몸통치기 (`a.p[5]`) still reads 0 or more gets it
/// LOCKED (−1) and every point it held back as skill points, ONCE — the receipt is the character's record's
/// (`AvatarBook::bodyslam_refunded`, kept only with the save that carries the refund, like 심안's), so nothing
/// happens until the book has the character. `hero2java::h2_bodyslam_retire` proves the class and the byte (the
/// slot first, the points last); the level table's `movs r2,#8 → #99` (`GuestBuild::bodyslam_locked`) keeps the
/// slot from opening again at LV 8.
struct BodySlamRetire {
    said: bool,
    warned: bool,
}

impl BodySlamRetire {
    fn new() -> Self {
        Self { said: false, warned: false }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, book: &mut crate::avatar_state::AvatarBook, io: &dyn crate::avatar_state::AvatarIo) {
        let retired = book.bodyslam_refunded();
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bodyslam_retire(&read, h2, retired)
        };
        let Some((plan, receipt)) = plan else { return };
        let mut done = 0usize;
        for (at, bytes) in &plan.writes {
            if !java_write(emulator, crate::guest_build::JavaWrite::CharSkills, *at, bytes) {
                break;
            }
            done += 1;
        }
        if done != plan.writes.len() {
            if !self.warned {
                self.warned = true;
                log::warn!("몸통치기 retire: {done}/{} write(s) landed — refused (the slot stays as it was, or the refund of {} is short)", plan.writes.len(), plan.refund);
            }
            return;
        }
        if receipt {
            book.mark_bodyslam_refunded(io, plan.refund);
            log::info!("몸통치기 retire: a.p[5] locked at {:#x} — the refund: {} point(s)", plan.writes[0].0, plan.refund);
            return;
        }
        if !self.said {
            self.said = true;
            log::info!("몸통치기 retire: a.p[5] locked again at {:#x} — no charge", plan.writes[0].0);
        }
    }
}

/// ★ 2026-10-03 발도 (user: 「방어의지와 함께」 해금): a KNIGHT whose 방어의지 (`a.p[8]`) is open while
/// the 발도 slot (`a.p[9]`, 부활오라's — no quest ever opens it) still reads `−1` gets that byte
/// ← 0, the write the quest 「터줏대감」's script would have made (`a.h(B)V`). Polled with the
/// combo repair, in the world only; `hero2java::h2_iaido_unlock` proves the class and both
/// bytes, and a character that has nothing to open plans nothing. Save-bearing like the repair
/// — it only opens a slot — through `JavaWrite::CharSkillUnlock`.
struct IaidoUnlock {
    said: bool,
    warned: bool,
}

impl IaidoUnlock {
    fn new() -> Self {
        Self { said: false, warned: false }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) {
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_iaido_unlock(&read, h2)
        };
        let Some((at, value)) = plan else { return };
        if !java_write(emulator, crate::guest_build::JavaWrite::CharSkillUnlock, at, &[value]) {
            if !self.warned {
                self.warned = true;
                log::warn!("발도 unlock: the write at {at:#x} was refused — the slot stays locked");
            }
            return;
        }
        if !self.said {
            self.said = true;
            log::info!("발도 unlock: a.p[9] opened at {at:#x} (방어의지 is open) — the knight can invest in 발도");
        }
    }
}

/// ★ 2026-10-03 (user: *"나이트 중장비는 기본으로 배워지는 스킬이라 초기화되면안돼"*): a KNIGHT whose
/// 「중장비」 (`a.p[10]`) reads 0 gets it back at 1 — `bx.b()V` seeds it at creation, its cap is 1,
/// and only the skill reset before this date ever lowered it. Polled with the combo repair,
/// on the same terms (no point taken back, idempotent by condition, world only);
/// `hero2java::h2_born_skill_repair` proves the class and the byte.
///
/// ★ 2026-10-06 (user: *"독연마 스킬을 … 1레벨 자동 획득 패시브 스킬로 변경 … 로그는 투명한 몬스터들은 처음 겜 시작부터
/// 볼 수있는거야"* · *"독연마의 이미 찍은 포인트 전부를 돌려줘야해"*): the ROGUE's 「심안」 (`a.p[12]`, the old 독 연마) too —
/// held at 1, and what 독 연마 held before this build refunded ONCE, all of it (the slot first, the points last). Once is
/// the character's record's to say (`AvatarBook::sight_refunded` — a receipt bound to the save: the refund and its
/// receipt are kept by the same save, and a reload that did not save takes both back), so nothing happens for a rogue
/// until the book has the character. Its effect is `RoguePatch`'s.
struct BornSkillRepair {
    said: bool,
    warned: bool,
}

impl BornSkillRepair {
    fn new() -> Self {
        Self { said: false, warned: false }
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, book: &mut crate::avatar_state::AvatarBook, io: &dyn crate::avatar_state::AvatarIo) {
        let refunded = book.sight_refunded();
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_born_skill_repair(&read, h2, refunded)
        };
        let Some((plan, receipt)) = plan else { return };
        let mut done = 0usize;
        for (at, bytes) in &plan.writes {
            if !java_write(emulator, crate::guest_build::JavaWrite::CharSkills, *at, bytes) {
                break;
            }
            done += 1;
        }
        if done != plan.writes.len() {
            if !self.warned {
                self.warned = true;
                log::warn!("born skill repair: {done}/{} write(s) landed — refused (the slot stays as it was, or the refund of {} is short)", plan.writes.len(), plan.refund);
            }
            return;
        }
        if receipt {
            // the rogue's one-time 독 연마 refund: its receipt, committed with the save that carries it
            book.mark_sight_refunded(io, plan.refund);
            log::info!("born skill repair: 심안 learned at {:#x} — the 독 연마 refund: {} point(s)", plan.writes[0].0, plan.refund);
            return;
        }
        if !self.said {
            self.said = true;
            let (at, _) = plan.writes[0];
            log::info!("born skill repair: back to 1 at {at:#x} (중장비 for a knight · 심안 for a rogue) — no charge");
        }
    }
}

// ── ★★ 2026-10-20: host notices in the game's own box, at the left ─────────────────────────────────────────────────
//
// User: *"저장성공, 저장실패, 등 중앙 하단쪽에 토스트로 나오던 내용들 좌측에 나오게 해줘. 다음 보스로 이동중... 과같이 좌측중앙쪽에
// … 중앙하단에 나오는 토스트 문구들 전수 조사해서 해당사항들이 전부 좌측 중앙 쪽에 나오게"*. Every line the host used to say in
// its centre-bottom toast (`UserEvent::Toast` → `overlay::draw_toast`, both gone) is now a picture in the game's notice
// box and font (`h2_notice_picture` — the raid countdown's path), stacked at the left edge with the pickups
// (`UserEvent::PickupNotice` → `overlay::draw_notices`). The UI thread cannot draw that picture — the font is read
// from the guest — so it SENDS its lines here (`InputEvent::HostNotice`) and the worker renders them; the worker's own
// sites call [`host_notice`] directly. A line said before the game's font is loaded (the vault's boot notice, the
// first frame) waits in the queue for it. The strings are UTF-8 source → the game's EUC-KR (`h2_euc_kr`; every one the
// host says is in `H2_HOST_NOTICE_STRINGS`, which `euc_kr_tests` proves drawable).

static HOST_NOTICES: std::sync::Mutex<std::collections::VecDeque<&'static str>> = std::sync::Mutex::new(std::collections::VecDeque::new());

/// Say `text` in the game's notice box at the left — queued; [`HostNotices::poll`] renders it on the next loop.
pub(crate) fn host_notice(text: &'static str) {
    if let Ok(mut q) = HOST_NOTICES.lock() {
        if q.len() >= crate::overlay::NOTICE_MAX {
            q.pop_front();
        }
        q.push_back(text);
    }
}

/// The worker's end of [`host_notice`]: the game's font, read once (`h2_font_from_guest`), and the queue drained
/// into `UserEvent::PickupNotice`s every loop.
#[derive(Default)]
struct HostNotices {
    font: Option<crate::guest_build::H2Font>,
}

impl HostNotices {
    fn poll(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>) {
        if HOST_NOTICES.lock().is_ok_and(|q| q.is_empty()) {
            return;
        }
        if crate::guest_build::hero2_java().is_none() {
            // Not hero2: there is no game font to say it in. Logged, dropped.
            if let Ok(mut q) = HOST_NOTICES.lock() {
                for t in q.drain(..) {
                    log::info!("host notice (no game font on this build): {t}");
                }
            }
            return;
        }
        if self.font.is_none() {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            self.font = crate::guest_build::h2_font_from_guest(&read);
        }
        // The font is there once the game has drawn its first text; until then the line waits.
        let Some(font) = self.font.as_ref() else { return };
        let texts: Vec<&'static str> = HOST_NOTICES.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default();
        for text in texts {
            match crate::guest_build::h2_euc_kr(text) {
                Some(bytes) => {
                    let n = crate::guest_build::h2_notice_picture(font, &bytes);
                    let _ = proxy.send_event(UserEvent::PickupNotice { w: n.w as u16, h: n.h as u16, argb: n.argb });
                    log::info!("host notice: {text}");
                }
                None => log::warn!("host notice: {text:?} has a character the game's font cannot draw — not shown"),
            }
        }
    }
}

/// ★★ 2026-10-20 (`craft_guard.rs`): a crafting OK that would craft into a full bag — taken, and the game's own
/// 「빈공간이 없습니다.」 box raised on the crafting window instead, so the materials, the book and the gold stay.
/// `true` = the key was taken. The box goes through `AccountState::notice` (the warehouse's `new ba(..)` path); if a
/// JVM call is in flight the key is still taken — the craft would destroy the result — and the log says the box
/// was not shown.
fn craft_full_guard(emulator: &mut dyn Emulator, account: &mut AccountState, h2: &crate::guest_build::Hero2Java) -> bool {
    let over = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_craft_overflow(&read, h2)
    };
    let Some(o) = over else { return false };
    let shown = account.notice(emulator, h2, o.window, None, NoticeText::BagFull);
    log::info!(
        "craft guard: OK refused — the result (type {}, id {}) would go to bag {:#x} ({} of {} cells, {} freed by the craft, no stack to merge into); box shown={shown}",
        o.result.0,
        o.result.1,
        o.bag,
        o.count,
        o.capacity,
        o.freed
    );
    true
}

/// ★★ 2026-09-24 — every pickup notice the game raises goes to the UI, once.
///
/// The game's own painter (`n.d()V`) is off, and it was the only reader of the notice:
/// `ag.a()Z` renders the text into `n.b` and sets `n.o = 16`, and nothing else looks.
/// So `n.o > 0` means "raised and not shown": the picture is copied out and sent
/// (`UserEvent::PickupNotice`) and `n.o` cleared (`JavaWrite::NoticeTaken`), and the
/// next notice sets it again. Polled every loop iteration — each 오토루팅 pickup raises
/// one, and they come one frame apart. One word read while nothing is up.
fn notice_feed(emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>) {
    if crate::guest_build::hero2_java().is_none() {
        return;
    }
    let got = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_notice_pending(&read)
    };
    let Some((timer_at, n)) = got else { return };
    java_write(emulator, crate::guest_build::JavaWrite::NoticeTaken, timer_at, &0u32.to_le_bytes());
    let _ = proxy.send_event(UserEvent::PickupNotice { w: n.w as u16, h: n.h as u16, argb: n.argb });
}

/// ★★ 2026-09-24 — the game's 환경설정 window with six host rows after its five
/// (드랍률 · 난이도 · 리스폰 · 오토루팅 · 경험치배율 · 조작방식 — 난이도 since 2026-09-29, 드랍률 since 2026-10-16),
/// scrolled five at a time. The model — cursor, scroll,
/// what a key or a tap does, and the picture — is `hero2java::H2Options` /
/// `h2_options_patch`, shared with the probe; this is the worker's half: it follows the
/// window, owns UP/DOWN (and LEFT/RIGHT/OK on a host row), applies a host row's change,
/// and sends the picture to the UI whenever it changes.
#[derive(Default)]
struct OptionsExt {
    state: crate::guest_build::H2Options,
    font: Option<crate::guest_build::H2Font>,
    sent: Vec<crate::guest_build::H2Patch>,
    /// The key whose DOWN this took — its UP must not reach the game either.
    swallow_up: Option<KeyCode>,
    /// ★ 2026-10-28: a save action row was pressed (`H2_OPT_SAVE_*`) — `SaveXfer`'s to take.
    action: Option<usize>,
}

/// ★ 2026-10-28: what is over the 환경설정 window. The save rows hang the game's own message box
/// (`ba`) on it; while that box is up the window keeps its host rows and scroll (the list must not
/// jump to the game's top five under the box), the host draws around the box, and the keys are
/// the box's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OptionsCover {
    /// The window is the top.
    Open,
    /// One box is over it, painted at this framebuffer rect (`h2_modal_box`).
    Box((u32, u32, u32, u32)),
    /// Something the host cannot place is over it: draw nothing.
    Unknown,
}

impl OptionsExt {
    fn ground(emulator: &dyn Emulator, chain: &[crate::guest_build::H2Win]) -> Option<(i32, i32, u32, u32)> {
        let h2 = crate::guest_build::hero2_java()?;
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_options_ground(&read, &h2, chain)
    }

    fn window(emulator: &dyn Emulator) -> Option<(crate::guest_build::H2Win, Vec<crate::guest_build::H2Win>, i32, i32, i8)> {
        match Self::window_covered(emulator)? {
            (w, OptionsCover::Open) => Some(w),
            _ => None,
        }
    }

    /// The window wherever it is in the chain, and what is over it.
    #[allow(clippy::type_complexity)]
    fn window_covered(emulator: &dyn Emulator) -> Option<((crate::guest_build::H2Win, Vec<crate::guest_build::H2Win>, i32, i32, i8), OptionsCover)> {
        let h2 = crate::guest_build::hero2_java()?;
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let chain = crate::guest_build::h2_window_chain(&read, &h2)?;
        let pos = chain.iter().position(|w| w.class == h2.classes.option_window)?;
        let win = chain[pos];
        let cover = match &chain[pos + 1..] {
            [] => OptionsCover::Open,
            [child] => crate::guest_build::h2_modal_box(&read, &h2, child).map_or(OptionsCover::Unknown, OptionsCover::Box),
            _ => OptionsCover::Unknown,
        };
        let cx = crate::guest_build::h2_rd_u32(&read, h2.screen.2)? as i32;
        let cy = crate::guest_build::h2_rd_u32(&read, h2.screen.3)? as i32;
        let cur = read(win.fields + 4 + crate::guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map_or(0, |b| *b as i8);
        Some(((win, chain, cx, cy, cur), cover))
    }

    fn take_action(&mut self) -> Option<usize> {
        self.action.take()
    }

    /// The host rows' current values, as the window shows them.
    fn host_values() -> [&'static [u8]; 6] {
        let exp = crate::guest_build::EXP_RATES.iter().position(|m| *m == crate::exp_rate()).unwrap_or(0);
        [
            crate::guest_build::H2_OPT_DROP_RATE_VALUES[usize::from(crate::guest_build::drop_rate_sanitise(crate::drop_rate()))],
            crate::guest_build::H2_OPT_DIFFICULTY_VALUES[usize::from(crate::guest_build::difficulty_sanitise(crate::difficulty()))],
            crate::guest_build::H2_OPT_RESPAWN_VALUES[crate::respawn::RATES.iter().position(|v| *v == crate::respawn::rate()).unwrap_or(0)],
            if crate::auto_loot() { b"ON" } else { b"OFF" },
            crate::guest_build::H2_OPT_EXP_VALUES[exp],
            crate::guest_build::H2_OPT_CONTROL_VALUES[usize::from(crate::touch_type()).min(2)],
        ]
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>) {
        let Some(((win, chain, cx, cy, cur), cover)) = Self::window_covered(&*emulator) else {
            if !self.sent.is_empty() {
                self.sent.clear();
                let _ = proxy.send_event(UserEvent::OptionsOverlay(Vec::new()));
            }
            self.state = crate::guest_build::H2Options::default();
            return;
        };
        // ★ 2026-10-28: under a box the host's cursor and scroll stand (`sync` would read the game's
        //   cursor resting on its last row as news only while the list is at its top anyway).
        if cover == OptionsCover::Open {
            self.state.sync(&win, cur);
        }
        if cover == OptionsCover::Unknown {
            if !self.sent.is_empty() {
                self.sent.clear();
                let _ = proxy.send_event(UserEvent::OptionsOverlay(Vec::new()));
            }
            return;
        }
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        if self.font.is_none() {
            self.font = crate::guest_build::h2_font_from_guest(&read);
        }
        let Some(font) = self.font.as_ref() else { return };
        let Some(rows) = crate::guest_build::h2_options_rows(&read, Self::host_values()) else { return };
        let upto = chain.iter().position(|w| w.handle == win.handle).map_or(chain.len(), |i| i + 1);
        let Some(ground) = Self::ground(&*emulator, &chain[..upto]) else { return };
        let mut patch = crate::guest_build::h2_options_patch(font, &rows, &self.state, cx, cy, ground);
        // The game's box is above the rows: its pixels are left to the frame.
        if let OptionsCover::Box((bx, by, bw, bh)) = cover {
            for p in &mut patch {
                for y in 0..p.h {
                    for x in 0..p.w {
                        let (fx, fy) = (p.x + x as i32, p.y + y as i32);
                        if fx >= bx as i32 && fy >= by as i32 && fx < (bx + bw) as i32 && fy < (by + bh) as i32 {
                            p.argb[(y * p.w + x) as usize] = 0;
                        }
                    }
                }
            }
        }
        if patch != self.sent {
            self.sent = patch.clone();
            let _ = proxy.send_event(UserEvent::OptionsOverlay(patch));
        }
    }

    /// A key while the window is up. `true` = taken (the game never sees it).
    fn key(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, code: KeyCode, down: bool) -> bool {
        if !down {
            return self.swallow_up.take_if(|k| *k == code).is_some();
        }
        use crate::guest_build::H2OptDir;
        let dir = match code {
            KeyCode::UP => H2OptDir::Up,
            KeyCode::DOWN => H2OptDir::Down,
            KeyCode::LEFT => H2OptDir::Left,
            KeyCode::RIGHT => H2OptDir::Right,
            KeyCode::OK => H2OptDir::Ok,
            _ => return false,
        };
        let Some((win, chain, _, _, cur)) = Self::window(&*emulator) else { return false };
        self.state.sync(&win, cur);
        let taken = self.state.key(dir, &win, &chain);
        if self.apply(emulator, data_dir, proxy, taken) {
            self.swallow_up = Some(code);
            self.poll(emulator, proxy);
            return true;
        }
        false
    }

    /// A tap on the window's rows. `true` = taken.
    fn tap(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, fx: u32, fy: u32) -> bool {
        let Some((win, chain, cx, cy, cur)) = Self::window(&*emulator) else { return false };
        self.state.sync(&win, cur);
        let Some(taken) = self.state.tap(fx as i32, fy as i32, cx, cy, &win, &chain) else { return false };
        if self.apply(emulator, data_dir, proxy, taken) {
            self.poll(emulator, proxy);
            return true;
        }
        false
    }

    fn apply(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, proxy: &EventLoopProxy<UserEvent>, taken: crate::guest_build::H2OptKey) -> bool {
        let crate::guest_build::H2OptKey::Take { writes, change } = taken else { return false };
        for (a, bytes) in &writes {
            java_write(emulator, crate::guest_build::JavaWrite::WidgetCursor, *a, bytes);
        }
        if let Some((row, dir)) = change {
            match row {
                // ★ 드랍률 (2026-10-16): 낮음 → 보통 → 높음, wrapping either way. The worker's `DropRate`
                //   poll moves the guest filter's KEEP word within a few frames; nothing is written here.
                5 => {
                    let n = crate::guest_build::DROP_RATE_KEEP.len() as i32;
                    let now = (i32::from(crate::drop_rate()) + i32::from(dir)).rem_euclid(n) as u8;
                    crate::set_drop_rate(now);
                    config::write_drop_rate(data_dir, now);
                    log::info!("환경설정: 드랍률 -> {now} (0 낮음 · 1 보통 · 2 높음) = {}% of the equipment drops", crate::guest_build::drop_rate_pct(now));
                }
                // ★ 난이도 (2026-09-29): EASY → NORMAL → HARD, wrapping either way. The
                //   worker's `Difficulty` poll rescales every monster on the stage within a
                //   few frames of this; nothing is written here.
                6 => {
                    let n = crate::guest_build::DIFFICULTY_PCT.len() as i32;
                    let now = (i32::from(crate::difficulty()) + i32::from(dir)).rem_euclid(n) as u8;
                    crate::set_difficulty(now);
                    config::write_difficulty(data_dir, now);
                    log::info!(
                        "환경설정: 난이도 -> {} ({}%)",
                        String::from_utf8_lossy(crate::guest_build::H2_OPT_DIFFICULTY_VALUES[usize::from(now)]),
                        crate::guest_build::difficulty_pct(now)
                    );
                }
                7 => crate::respawn::cycle(data_dir, dir),
                8 => {
                    let now = !crate::auto_loot();
                    crate::set_auto_loot(now);
                    config::write_auto_loot(data_dir, now);
                    log::info!("환경설정: 오토루팅 -> {now}");
                }
                // ★ 2026-10-28: the save actions — pressed, not stepped (LEFT never gets here).
                crate::guest_build::H2_OPT_SAVE_EXPORT..=crate::guest_build::H2_OPT_SAVE_RESTORE => {
                    if dir > 0 {
                        self.action = Some(row);
                    }
                }
                9 => {
                    let rates = crate::guest_build::EXP_RATES;
                    let i = rates.iter().position(|m| *m == crate::exp_rate()).unwrap_or(0) as i32;
                    let now = rates[(i + i32::from(dir)).rem_euclid(rates.len() as i32) as usize];
                    crate::set_exp_rate(now);
                    config::write_exp_rate(data_dir, now);
                    log::info!("환경설정: 경험치배율 -> x{now}");
                }
                10 => {
                    let now = (i32::from(crate::touch_type()) + i32::from(dir)).rem_euclid(3) as u8;
                    crate::set_touch_type(now);
                    config::write_touch_type(data_dir, now);
                    let _ = proxy.send_event(UserEvent::TouchTypeChanged);
                    log::info!("환경설정: 조작방식 -> {now} (0 free, 1 fixed_A, 2 fixed_B)");
                }
                other => log::warn!("환경설정: row {other} has no setting"),
            }
        }
        true
    }
}

/// ★★ 오토루팅 (2026-09-24, user: "2칸 … 전부 주울꺼야 … 한번에 줍게하고 … 빈공간없을떄는
/// 시도안하고"). The game picks up only what lies under the player, by its own `ag.a()Z`;
/// this chooses the nearest drop within two tiles that its bag WILL take
/// (`hero2java::h2_auto_loot_pick`, the game's room check read rather than guessed),
/// moves that drop's tile onto the player's (`JavaWrite::GroundDropMove`) and calls
/// `ag.a()Z` — the game files it, removes it and raises its notice as for an OK press.
/// One call in flight at a time, so a pile of drops is taken one per frame and its
/// notices arrive in order. A drop that does not fit is never tried.
#[derive(Default)]
struct AutoLoot {
    call: Option<(Arc<wie_backend::GuestCallResult>, u32, Instant)>,
    /// A drop the game refused anyway (the room check and the game disagreed, or the
    /// player stepped off the tile between the move and the call): left alone briefly.
    refused: Option<(u32, Instant)>,
}

/// How long a call may take before it is given up on (it normally completes next tick).
const AUTO_LOOT_CALL_MAX: Duration = Duration::from_millis(2000);
const AUTO_LOOT_REFUSED_WAIT: Duration = Duration::from_millis(600);

impl AutoLoot {
    fn poll(&mut self, emulator: &mut dyn Emulator, drive: &Option<UseDrive>) {
        if let Some((out, handle, since)) = self.call.take() {
            match out.poll() {
                None if since.elapsed() < AUTO_LOOT_CALL_MAX => {
                    self.call = Some((out, handle, since));
                    return;
                }
                None => log::warn!("auto loot: ag.a()Z on drop {handle:#x} never returned — given up"),
                Some(Ok(0)) | Some(Err(())) => self.refused = Some((handle, Instant::now())),
                Some(Ok(_)) => {}
            }
        }
        if !crate::auto_loot() || drive.as_ref().is_some_and(|d| d.owns) {
            return;
        }
        // On the field, standing or walking — `n.i()V` itself skips the pickup in the
        // attack state, and a window or a script is not the field.
        let Some(w) = field_walk(emulator) else { return };
        if w.state != crate::guest_build::H2_WALK_STANDING && w.state != crate::guest_build::H2_WALK_WALKING {
            return;
        }
        let (Some(h2), Some(sv)) = (crate::guest_build::hero2_java(), crate::guest_build::hero2_save()) else { return };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_loot_near(&read, &h2, sv.area.stage_slot).and_then(|(tile, near)| {
                let pick = crate::guest_build::h2_auto_loot_pick(&near, |h| {
                    self.refused.is_some_and(|(r, at)| r == h && at.elapsed() < AUTO_LOOT_REFUSED_WAIT)
                })?;
                Some((tile, pick, crate::guest_build::h2_drop_tile_at(&read, pick.handle)?))
            })
        };
        let Some((tile, pick, at)) = plan else { return };
        if (pick.entry[0], pick.entry[1]) != tile
            && !java_write(emulator, crate::guest_build::JavaWrite::GroundDropMove, at, &[tile.0 as u8, tile.1 as u8])
        {
            return;
        }
        // The drop now lies under the player: should the game refuse it after all, its notice names the right bag.
        sync_bag_full_row(emulator);
        if let Some(out) = java_call(emulator, crate::guest_build::JavaCall::FieldPickup, JavaThis::None, &[]) {
            log::debug!("auto loot: {:?} at distance {}", pick.entry, pick.dist);
            self.call = Some((out, pick.handle, Instant::now()));
        }
    }
}

/// ★ 2026-09-24 (user: "빈공간없는데 아이템 근처가서 ok로 주우려고하면 빈공간이 없습니다.
/// 안내문구를 템 드랍때 처럼 띄우면돼"): with 오토루팅 on, an OK near a drop (within its two
/// tiles) that no bag has room for raises the game's own 「빈공간이 없습니다.」 — the notice
/// the game gives only for a drop UNDER the player (`ag.a()Z` @0x1125e8, which then
/// returns false so the OK goes on to talk or attack; so does this: the key is forwarded
/// unchanged). Nothing when a drop under the player exists (the game says it itself) or
/// when something nearby fits (오토루팅 is about to take it).
fn bag_full_notice_on_ok(emulator: &mut dyn Emulator) {
    if !crate::auto_loot() {
        return;
    }
    let Some(w) = field_walk(emulator) else { return };
    if w.state != crate::guest_build::H2_WALK_STANDING && w.state != crate::guest_build::H2_WALK_WALKING {
        return;
    }
    let (Some(h2), Some(sv)) = (crate::guest_build::hero2_java(), crate::guest_build::hero2_save()) else { return };
    let text = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_loot_near(&read, &h2, sv.area.stage_slot)
            .filter(|(_, near)| !near.is_empty() && near.iter().all(|l| !l.fits && l.dist > 0))
            // ★★ 2026-10-18: 「재료가방이 가득 참」 when every refused drop is a 재료가방 type.
            .and_then(|(_, near)| crate::guest_build::h2_near_full_text(&read, &near))
    };
    if let Some(text) = text {
        let args = [wie_backend::JavaArg::Object(text), wie_backend::JavaArg::Int(16), wie_backend::JavaArg::Int(0)];
        if java_call(emulator, crate::guest_build::JavaCall::Notice, JavaThis::None, &args).is_some() {
            log::info!("auto loot: OK near a drop no bag has room for — the bag-full notice");
        }
    }
}

/// ★★ 2026-10-18 (user: 인벤토리에 빈 칸이 있는데 「빈공간이 없습니다.」가 떠서 안 주워진다 → 1 「재료가방 문구를 따로」): before an OK
/// reaches the game — and before 오토루팅's own pickup call — the row the game's pickup refusal names is set from the drops
/// under the player: 「재료가방이 가득 참」 when every one is a 조합서 · 조합 재료 (they go to the 재료가방), 「빈공간이 없습니다.」
/// otherwise (`hero2java::h2_bag_full_row`, the site = `GuestBuild::bag_full_row_site`). The game raises the notice
/// itself as before; this only changes WHICH of its text rows. Every OK re-decides — the immediate is never left to a
/// moment it was not chosen for. A site that reads as neither form is left alone.
fn sync_bag_full_row(emulator: &mut dyn Emulator) {
    let Some(site) = crate::guest_build::bag_full_row_site() else { return };
    let (Some(h2), Some(sv)) = (crate::guest_build::hero2_java(), crate::guest_build::hero2_save()) else { return };
    let write = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let row = crate::guest_build::h2_loot_near(&read, &h2, sv.area.stage_slot).map_or(
            crate::guest_build::Hero2Java::ITEM_TYPE_ROW_BAG_FULL,
            |(_, near)| crate::guest_build::h2_bag_full_row(&near, crate::guest_build::h2_material_full_text(&read).is_some()),
        );
        crate::guest_build::h2_bag_full_row_write(&read, site, row).map(|w| (w, row))
    };
    if let Some(((at, bytes), row)) = write {
        if emulator.write_memory(at, &bytes) {
            log::info!("pickup refusal: the notice row -> {row} ({})", if row == crate::guest_build::Hero2Java::ITEM_TYPE_ROW_BAG_FULL { "빈공간이 없습니다." } else { "재료가방이 가득 참" });
        } else {
            log::warn!("pickup refusal: the row write at {at:#x} failed");
        }
    }
}

/// ★★ 부활의 구 (2026-09-25, user: *"부활의 서라는 아이템도 상점에 추가할수있나? 죽으면 해당
/// 아이템을 사용해서 제자리에서 부활하는거야"* → 5,000원 · HP·SP 전부 · 매번 묻기 · 파란 구슬,
/// named 「부활의 구」). The game has no revive. What it has is nine field frames between HP
/// reaching 0 and the GAME OVER event in which nothing irreversible has happened (the
/// `hero2java` 「부활의 구」 section, HERO2_GUEST_RE.md §8.18). This owns them:
///
///  * **ask** — the fall is let play out (user: *"쓰러지는 애니메이션나오고 그뒤에 뜨는게 더
///    자연스러울것같아"*): the first poll that finds it on its second-to-last frame
///    (`h2_fall_frame` ≥ `H2_FALL_ASK_FRAME`) with an orb in the inventory raises the host's
///    box (`h2_confirm_patch`: the game's own 예/아니오 box, redrawn over the frozen frame of
///    the fallen character) and from then on THE WORKER DOES NOT TICK THE GUEST
///    ([`Revive::holding`]): the fall cannot finish under the question, no monster swings,
///    and the GAME OVER cannot start. Keys and taps go to the box; everything else the UI
///    sends is dropped (a plate, a map key) or refused with a word (SAVE).
///  * **예** — the stand-up writes (`h2_revive_writes`, [`JavaWrite::CharRevive`]: HP and SP to
///    their maxima, ★ 2026-10-26 the action chain cleared as `a.i()V` clears it — a fall
///    mid-action otherwise left a chain that refused every attack and class-skill key — then
///    the motion words `g.b(1)` writes), then ONE orb consumed with the
///    game's own `bq.a(BI)V`, then the game's own notice with `/t/itmtp`[64]. The ticks
///    resume and the character is standing where it fell.
///  * **아니오** — the ticks resume and the game's own death runs its course: GAME OVER, then
///    the last save. Not asked again for this fall.
///
/// ⛔ The orb is consumed only AFTER the writes landed, never before: a revive that did not
/// happen must not cost the player an orb. The opposite failure — writes landed, consume
/// refused — is a free revive, logged; the same order `AccountState::start_reset` keeps.
///
/// [`JavaWrite::CharRevive`]: crate::guest_build::JavaWrite::CharRevive
/// ★ 2026-09-26 (user: "그냥 보스를 잡는순간 HP,SP를 풀로 채워줘"): the moment a boss starts
/// to die, the character is refilled — HP and SP to their maxima — and if it is already
/// FALLING (the two killing blows landed together) it stands back up where it fell, with the
/// same stand-up writes the 부활의 구 makes ([`JavaWrite::CharRevive`], the chain cleared, `g.t` last). For
/// [`BOSS_REFILL_WINDOW`] after that a fall is still undone — the boss's death animation
/// runs on, and a hit already in flight can land in it.
///
/// Why: a boss killed in the instant the character fell played its post-kill dialogue and
/// then the GAME OVER, and the game's script system kept that unfinished dialogue across the
/// reload — re-entering the boss room drew the room's entry dialogue over it, both texts in
/// one box (probe, the S25 save's 해저터널 → 「소문의 검사」). Refilled at the kill, the two
/// deaths cannot land together. "Boss" is the game's own `instanceof ah`
/// (`hero2java::h2_dying_bosses`).
///
/// Nothing new is saved: HP and SP are the character's own fields, the game serialises them
/// as it always does, and a refill is what a full potion would have done.
///
/// [`JavaWrite::CharRevive`]: crate::guest_build::JavaWrite::CharRevive
#[derive(Default)]
struct BossRefill {
    /// Bosses already answered, while they are still dying.
    answered: Vec<u32>,
    /// Until when a fall is undone after a boss death.
    window: Option<Instant>,
}

const BOSS_REFILL_WINDOW: Duration = Duration::from_millis(3000);

impl BossRefill {
    fn poll(&mut self, emulator: &mut dyn Emulator) {
        let (Some(h2), Some(sv), Some(st @ crate::guest_build::AppState::Absolute { addr, field, .. })) =
            (crate::guest_build::hero2_java(), crate::guest_build::hero2_save(), crate::guest_build::app_state())
        else {
            return;
        };
        let (fresh, writes) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            // The field, or the script screen a boss's death opens onto (the fall carries on
            // under it — that is how the dialogue came before the GAME OVER).
            let state = read(addr, 4);
            let state = (state.len() == 4).then(|| u32::from_le_bytes([state[0], state[1], state[2], state[3]]));
            if !state.is_some_and(|v| v == field || st.is_script(v) == Some(true)) {
                if self.window.is_some_and(|w| Instant::now() >= w) {
                    self.window = None;
                }
                return;
            }
            let dying = crate::guest_build::h2_dying_bosses(&read, sv.area.stage_slot);
            self.answered.retain(|b| dying.contains(b));
            let fresh: Vec<u32> = dying.iter().copied().filter(|b| !self.answered.contains(b)).collect();
            let open = !fresh.is_empty() || self.window.is_some_and(|w| Instant::now() < w);
            let writes = if open { crate::guest_build::h2_refill_writes(&read, &h2) } else { None };
            // Inside the window only a FALL is answered — a plain refill is the kill's.
            let writes = writes.filter(|w| !fresh.is_empty() || crate::guest_build::h2_plan_stands_up(w));
            (fresh, writes)
        };
        if !fresh.is_empty() {
            self.answered.extend(&fresh);
            self.window = Some(Instant::now() + BOSS_REFILL_WINDOW);
        } else if self.window.is_some_and(|w| Instant::now() >= w) {
            self.window = None;
        }
        let Some(writes) = writes else {
            if !fresh.is_empty() {
                log::info!("boss refill: boss {:#x?} is dying — the character is full already", fresh);
            }
            return;
        };
        let stand_up = crate::guest_build::h2_plan_stands_up(&writes);
        let mut ok = true;
        for (a, bytes) in &writes {
            // In order, and stop at the first refusal: `g.t` is last, so a batch that stops
            // early leaves the fall to run its course, never a character standing at 0 HP.
            ok = java_write(emulator, crate::guest_build::JavaWrite::CharRevive, *a, bytes);
            if !ok {
                break;
            }
        }
        log::info!(
            "boss refill: {} — HP/SP full{} (writes ok={ok})",
            if fresh.is_empty() { "a fall inside the window after a boss death".to_string() } else { format!("boss {fresh:#x?} is dying") },
            if stand_up { ", and the falling character stands up" } else { "" }
        );
    }
}

/// ★★ 2026-10-17 보스 레이드 = 스토리 보스 아홉을 차례로 (user: *"보스레이드의 보스가 기존 보스 순차로 … 보스를 죽인뒤에 좌측
/// 템나오는 부분에 다음 보스로 이동 중...5 … 1이후 다음보스로 … 마지막에 … 최종 클리어한 보스만큼 보상 … 이지 100만 · 노말 200만 ·
/// 하드 300만"*). The model is `hero2java::H2BossRaid` (shared with the probe); this is the worker's half: it reads the map, the
/// field and the kill flag each loop, and does what the model says —
/// * a countdown notice a second, in the game's notice box and font (`h2_notice_picture`), each replacing the last
///   (`UserEvent::RaidCountdown`);
/// * at 0, the stage map's advance group through the game's own `bb.a(B)V` (`JavaCall::ScriptGroup`) — the warp to the next
///   boss (after the last, the first again: ★★ (2) the next lap, `N바퀴 시작!`, its monsters ×1.3 HP · ×1.05 ATK through
///   `Difficulty::poll(…, lap)`) is the map's script;
/// * on the way out of the raid (the door's 「포기하시겠습니까?」), `ag.g(I)V` with the gold for the bosses killed this run —
///   each at the 난이도 · lap of its kill (`h2_raid_gold`) — and two notices. Nothing else is written; the gold is saved when
///   the player saves.
struct BossRaid {
    model: crate::guest_build::H2BossRaid,
    font: Option<crate::guest_build::H2Font>,
    epoch: Instant,
}

impl BossRaid {
    fn new() -> Self {
        Self { model: crate::guest_build::H2BossRaid::default(), font: None, epoch: Instant::now() }
    }

    /// `revive_waits` — the 부활의 구 box is up or about to be (`Revive::waits`): a fall is then the box's, not the raid's.
    fn poll(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, revive_waits: bool) {
        let (Some(h2), Some(sv), Some(crate::guest_build::AppState::Absolute { addr, field, .. })) =
            (crate::guest_build::hero2_java(), crate::guest_build::hero2_save(), crate::guest_build::app_state())
        else {
            return;
        };
        let view = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let map = crate::guest_build::h2_live_map(&read, &sv.area);
            let raid = map.and_then(crate::guest_build::GuardianPatch::raid_stage).is_some();
            crate::guest_build::H2RaidView {
                in_world: crate::guest_build::h2_demon_in_world(&read, &h2),
                map,
                field: crate::guest_build::h2_rd_u32(&read, addr) == Some(field),
                // the flag only matters on a raid map — one array read there, none elsewhere
                flag: raid && crate::guest_build::h2_event_flags(&read, &sv.area).is_some_and(|f| crate::guest_build::h2_flag_on(&f, crate::guest_build::H2_RAID_FLAG)),
                difficulty: crate::difficulty(),
                // ★★ (3) the fall played out to the 부활의 구's frame, and no box to ask first
                fallen: raid
                    && !revive_waits
                    && crate::guest_build::h2_fall_frame(&read, &h2, addr, field).is_some_and(|f| f >= crate::guest_build::H2_FALL_ASK_FRAME),
            }
        };
        let now = self.epoch.elapsed().as_millis() as u64;
        for action in self.model.step(now, view) {
            match action {
                crate::guest_build::H2RaidAction::Countdown(text) => {
                    if let Some(n) = self.picture(emulator, &text) {
                        let _ = proxy.send_event(UserEvent::RaidCountdown { w: n.w as u16, h: n.h as u16, argb: n.argb });
                    }
                    log::info!("boss raid: map {:?} — countdown {}", view.map, text.last().map_or('?', |&c| c as char));
                }
                crate::guest_build::H2RaidAction::Notice(text) => {
                    if let Some(n) = self.picture(emulator, &text) {
                        let _ = proxy.send_event(UserEvent::PickupNotice { w: n.w as u16, h: n.h as u16, argb: n.argb });
                    }
                    log::info!("boss raid: map {:?} — lap {} begins", view.map, self.model.lap() + 1);
                }
                crate::guest_build::H2RaidAction::Advance => {
                    let group = i32::from(crate::guest_build::H2_RAID_ADVANCE_GROUP) + 1;
                    match java_call(emulator, crate::guest_build::JavaCall::ScriptGroup, JavaThis::None, &[wie_backend::JavaArg::Int(group)]) {
                        Some(_) => log::info!("boss raid: map {:?} — advance (bb.a(B)V({group}))", view.map),
                        None => {
                            log::warn!("boss raid: map {:?} — the advance call was refused; again at the next poll", view.map);
                            self.model.advance_refused();
                        }
                    }
                }
                crate::guest_build::H2RaidAction::Escape => {
                    // ★★ (3) the home group first; only once it is taken does the character stand up (HP · SP full) — a refused
                    //   call leaves the fall to the game, never a character standing in the raid with nowhere to go
                    let group = i32::from(crate::guest_build::H2_RAID_HOME_GROUP) + 1;
                    match java_call(emulator, crate::guest_build::JavaCall::ScriptGroup, JavaThis::None, &[wie_backend::JavaArg::Int(group)]) {
                        Some(_) => {
                            let writes = {
                                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                                crate::guest_build::h2_revive_writes(&read, &h2)
                            };
                            let mut done = 0usize;
                            let total = writes.as_ref().map_or(0, Vec::len);
                            for (a, bytes) in writes.iter().flatten() {
                                if !java_write(emulator, crate::guest_build::JavaWrite::CharRevive, *a, bytes) {
                                    break;
                                }
                                done += 1;
                            }
                            log::info!("boss raid: map {:?} — fell; home (bb.a(B)V({group})) with {:?} — stood up {done}/{total}", view.map, self.model.tally());
                        }
                        None => {
                            log::warn!("boss raid: map {:?} — fell, and the home call was refused; again at the next poll", view.map);
                            self.model.escape_refused();
                        }
                    }
                }
                crate::guest_build::H2RaidAction::ClearStage => {
                    // the boss's guardians · hands go with it — each through the game's own death (EXP · drops as any kill)
                    let standing: Vec<u32> = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_monsters(&read, sv.area.stage_slot).iter().filter(|m| m.hp > 0).map(|m| m.handle).collect()
                    };
                    for &m in &standing {
                        let _ = java_call(emulator, crate::guest_build::JavaCall::MonsterDeath, JavaThis::Handle(m), &[]);
                    }
                    log::info!("boss raid: map {:?} — the boss is down; {} monster(s) left on the stage go with it", view.map, standing.len());
                }
                crate::guest_build::H2RaidAction::Pay { bosses, gold } => {
                    let paid = java_call(emulator, crate::guest_build::JavaCall::GoldAdd, JavaThis::None, &[wie_backend::JavaArg::Int(gold as i32)]).is_some();
                    log::info!("boss raid: out of the raid — {bosses} boss(es), {gold} gold (ag.g(I)V issued={paid})");
                    if paid {
                        for text in crate::guest_build::h2_raid_reward_texts(bosses, gold) {
                            if let Some(n) = self.picture(emulator, &text) {
                                let _ = proxy.send_event(UserEvent::PickupNotice { w: n.w as u16, h: n.h as u16, argb: n.argb });
                            }
                        }
                    }
                }
            }
        }
    }

    /// The notice picture in the game's own font — read from the guest once (`h2_font_from_guest`).
    fn picture(&mut self, emulator: &mut dyn Emulator, text: &[u8]) -> Option<crate::guest_build::H2Notice> {
        if self.font.is_none() {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            self.font = crate::guest_build::h2_font_from_guest(&read);
            if self.font.is_none() {
                log::warn!("boss raid: the game's font is not loaded yet — no notice");
            }
        }
        self.font.as_ref().map(|f| crate::guest_build::h2_notice_picture(f, text))
    }
}

#[derive(Default)]
struct Revive {
    /// The box on screen: its cursor and the orb it would spend.
    prompt: Option<RevivePrompt>,
    /// The player said 아니오 to THIS fall (or it could not be revived): not asked again
    /// until the character is up — the reload that follows the GAME OVER, or a revive.
    declined: bool,
    /// The consume call in flight after a revive.
    consume: Option<(Arc<wie_backend::GuestCallResult>, Instant)>,
    font: Option<crate::guest_build::H2Font>,
    /// Keys whose DOWN the box took — their UP must not reach the game either.
    swallow_up: Vec<KeyCode>,
    /// A fall with no orb to offer, said once per fall.
    said: bool,
}

struct RevivePrompt {
    confirm: crate::guest_build::H2Confirm,
    orb: crate::guest_build::H2ReviveOrb,
    lines: Vec<Vec<u8>>,
    /// `(ax.g, ax.i, ax.j)` when the box was raised — the frame it is drawn over.
    screen: (i32, i32, i32),
    /// When the picture was last sent. Resent every [`REVIVE_RESEND`]: the game is held
    /// under the box, so a UI that lost the picture (an activity rebuilt behind a paused
    /// game) would otherwise show a frozen frame with nothing to press.
    sent: Instant,
}

const REVIVE_RESEND: Duration = Duration::from_millis(1000);
const REVIVE_CONSUME_MAX: Duration = Duration::from_millis(3000);

impl Revive {
    /// The box is up: the guest must not tick, and every UI event goes through here first.
    fn holding(&self) -> bool {
        self.prompt.is_some()
    }

    /// ★★ 2026-10-17 (3) 보스 레이드: a fall is this box's — it is up, the orb is being spent, or an orb is in the bag and the
    /// player has not said 아니오 (nor has the box been given up on: no font, `said`). Otherwise a fall in the raid goes home.
    fn waits(&self, emulator: &dyn Emulator) -> bool {
        if self.prompt.is_some() || self.consume.is_some() {
            return true;
        }
        if self.declined || self.said {
            return false;
        }
        let (Some(h2), Some(crate::guest_build::AppState::Absolute { addr, field, .. })) = (crate::guest_build::hero2_java(), crate::guest_build::app_state()) else {
            return false;
        };
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        // cheapest first: the bag is walked only while the character is down (a word or four every loop otherwise)
        crate::guest_build::h2_fall_frame(&read, &h2, addr, field).is_some() && crate::guest_build::h2_revive_orb(&read, &h2).is_some()
    }

    /// The tick loop's cheap question (a few guest words): the fall has reached the frame
    /// the box is raised on and this would ask. The loop stops ticking for the frame on
    /// `true`, so the poll that raises the box comes before the fall's last frame even when
    /// a slow device runs several ticks per iteration.
    fn would_ask(&self, emulator: &dyn Emulator) -> bool {
        if self.prompt.is_some() || self.declined || self.consume.is_some() {
            return false;
        }
        let (Some(h2), Some(crate::guest_build::AppState::Absolute { addr, field, .. })) = (crate::guest_build::hero2_java(), crate::guest_build::app_state()) else {
            return false;
        };
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_fall_frame(&read, &h2, addr, field).is_some_and(|f| f >= crate::guest_build::H2_FALL_ASK_FRAME)
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>) {
        if let Some((out, since)) = self.consume.take() {
            match out.poll() {
                None if since.elapsed() < REVIVE_CONSUME_MAX => self.consume = Some((out, since)),
                None => log::error!("revive: bq.a(BI)V never returned — the revive stands, the orb may still be there"),
                Some(Err(())) => log::error!("revive: the orb could not be consumed — the revive stands, the orb is still there"),
                Some(Ok(_)) => {
                    log::info!("revive: one 부활의 구 consumed");
                    self.notice(emulator);
                }
            }
        }
        if self.prompt.as_ref().is_some_and(|p| p.sent.elapsed() >= REVIVE_RESEND) {
            self.send(proxy);
        }
        if self.prompt.is_some() {
            return;
        }
        let (Some(h2), Some(crate::guest_build::AppState::Absolute { addr, field, .. })) = (crate::guest_build::hero2_java(), crate::guest_build::app_state()) else {
            return;
        };
        let fall = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_fall_frame(&read, &h2, addr, field)
        };
        let Some(fall) = fall else {
            // The latches outlive the fall only until the character is up again (the load
            // after the GAME OVER, or a revive) — read only while one of them is set.
            if self.declined || self.said {
                let down = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    crate::guest_build::h2_char_down(&read, &h2)
                };
                if !down {
                    self.declined = false;
                    self.said = false;
                }
            }
            return;
        };
        // ★ Let the fall play out first (user, 2026-09-25: "쓰러지는 애니메이션나오고 그뒤에
        //   뜨는게 더 자연스러울것같아") — the box goes up on its second-to-last frame, see
        //   `H2_FALL_ASK_FRAME` for why not the last.
        if self.declined || self.consume.is_some() || fall < crate::guest_build::H2_FALL_ASK_FRAME {
            return;
        }
        let orb = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            if self.font.is_none() {
                self.font = crate::guest_build::h2_font_from_guest(&read);
            }
            crate::guest_build::h2_revive_orb(&read, &h2)
        };
        let Some(orb) = orb else {
            if !self.said {
                self.said = true;
                log::info!("revive: fell with no 부활의 구 in the inventory — the game's own death runs");
            }
            return;
        };
        let screen = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let rd = |slot: u32| {
                let b = read(slot, 4);
                (b.len() == 4).then(|| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            (rd(h2.screen.0), rd(h2.screen.1), rd(h2.screen.2))
        };
        let (Some(sw), Some(sh), Some(cx)) = screen else { return };
        if self.font.is_none() {
            if !self.said {
                self.said = true;
                log::warn!("revive: the game's font is not readable — cannot ask, the game's own death runs");
            }
            return;
        }
        let lines = crate::guest_build::h2_revive_lines(orb.total);
        self.prompt = Some(RevivePrompt { confirm: crate::guest_build::H2Confirm::new(), orb, lines, screen: (sw, sh, cx), sent: Instant::now() });
        self.send(proxy);
        log::info!("revive: fell with {} 부활의 구 — asking on fall frame {fall}; the guest is held until the answer", orb.total);
    }

    /// The box's picture to the UI (and the resend clock).
    fn send(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        let (Some(p), Some(font)) = (self.prompt.as_mut(), self.font.as_ref()) else { return };
        let (sw, sh, cx) = p.screen;
        let patch = crate::guest_build::h2_confirm_patch(font, &p.lines, p.confirm.yes, sw, sh, cx);
        p.sent = Instant::now();
        let _ = proxy.send_event(UserEvent::RevivePrompt(vec![patch]));
    }

    /// A key. While the box is up every DOWN is taken (the box is modal) and so is its UP;
    /// the UP of a key that went down BEFORE the fall is the player letting go of something
    /// held — a direction they were walking — and goes on to the game and to `held`.
    /// `true` = taken.
    fn key(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, code: KeyCode, down: bool) -> bool {
        if !down {
            if let Some(i) = self.swallow_up.iter().position(|k| *k == code) {
                self.swallow_up.remove(i);
                return true;
            }
            return false;
        }
        let Some(p) = self.prompt.as_mut() else { return false };
        if !self.swallow_up.contains(&code) {
            self.swallow_up.push(code);
        }
        use crate::guest_build::H2OptDir;
        let dir = match code {
            KeyCode::UP | KeyCode::NUM2 => Some(H2OptDir::Up),
            KeyCode::DOWN | KeyCode::NUM8 => Some(H2OptDir::Down),
            KeyCode::LEFT | KeyCode::NUM4 => Some(H2OptDir::Left),
            KeyCode::RIGHT | KeyCode::NUM6 => Some(H2OptDir::Right),
            KeyCode::OK | KeyCode::NUM5 => Some(H2OptDir::Ok),
            _ => None,
        };
        let act = p.confirm.key(dir);
        self.act(emulator, proxy, act);
        true
    }

    /// A tap at framebuffer `(fx, fy)` while the box is up — always taken.
    fn tap(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, fx: u32, fy: u32) -> bool {
        let Some(p) = self.prompt.as_mut() else { return false };
        let (sw, sh, cx) = p.screen;
        let hit = crate::guest_build::h2_confirm_hit(p.lines.len(), sw, sh, cx, fx as i32, fy as i32);
        let act = p.confirm.tap(hit);
        self.act(emulator, proxy, act);
        true
    }

    fn act(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, act: crate::guest_build::H2ConfirmAct) {
        match act {
            crate::guest_build::H2ConfirmAct::Moved => self.send(proxy),
            crate::guest_build::H2ConfirmAct::Swallowed => {}
            crate::guest_build::H2ConfirmAct::Chose(yes) => self.answer(emulator, proxy, yes),
        }
    }

    fn answer(&mut self, emulator: &mut dyn Emulator, proxy: &EventLoopProxy<UserEvent>, yes: bool) {
        let Some(p) = self.prompt.take() else { return };
        let _ = proxy.send_event(UserEvent::RevivePrompt(Vec::new()));
        if !yes {
            self.declined = true;
            log::info!("revive: 아니오 — the game's own death runs (GAME OVER, then the last save)");
            return;
        }
        let Some(h2) = crate::guest_build::hero2_java() else {
            self.declined = true;
            return;
        };
        // Re-read: the guest has not ticked since the box went up, so this is the same
        // fall and the same bag — and if it somehow is not, nothing is written or spent.
        let (plan, at) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_revive_writes(&read, &h2), crate::guest_build::h2_bag_index_of(&read, p.orb.bag, p.orb.record))
        };
        let (Some(writes), Some(at)) = (plan, at) else {
            self.declined = true;
            log::warn!("revive: 예, but the fall or the orb no longer reads as it did — nothing written, nothing spent");
            return;
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !java_write(emulator, crate::guest_build::JavaWrite::CharRevive, *a, bytes) {
                break;
            }
            done += 1;
        }
        if done != writes.len() {
            // The motion state is the LAST write, so an early stop leaves the fall running
            // as the game would have — and the orb where it is.
            self.declined = true;
            log::error!("revive: {done}/{} write(s) landed — the fall runs its course, the orb is kept", writes.len());
            return;
        }
        log::info!("revive: 예 — standing where it fell, HP/SP full ({} write(s)); consuming 1 of {}", writes.len(), p.orb.total);
        match java_call(
            emulator,
            crate::guest_build::JavaCall::BagConsumeAt,
            JavaThis::Handle(p.orb.bag),
            &[wie_backend::JavaArg::Int(at as i32), wie_backend::JavaArg::Int(1)],
        ) {
            Some(out) => self.consume = Some((out, Instant::now())),
            None => log::error!("revive: the consume call was refused — a free revive, the orb is still there"),
        }
    }

    /// The game's own notice, 「부활의 구 사용」 (`/t/itmtp`[64], appended by the asset tool)
    /// — it lands in the pickup-notice stack like any other.
    fn notice(&self, emulator: &mut dyn Emulator) {
        let text = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_revive_text(&read)
        };
        match text {
            Some(text) => {
                let args = [wie_backend::JavaArg::Object(text), wie_backend::JavaArg::Int(16), wie_backend::JavaArg::Int(0)];
                let _ = java_call(emulator, crate::guest_build::JavaCall::Notice, JavaThis::None, &args);
            }
            None => log::warn!("revive: /t/itmtp row {} is missing — was the asset regenerated?", crate::guest_build::Hero2Java::ITEM_TYPE_ROW_REVIVED),
        }
    }
}

/// ★★ 2026-09-24 (user: "아이템이 사라지는 시간을 대폭 늘려야해. 현재 너무짧아"): ground
/// drops live ~60 s instead of the game's ~6 s.
///
/// The game's counter is ONE signed byte per drop (e[5] of the `bf.b` element, 120 at
/// spawn, removed below 0), so no immediate can give more than 128 ticks, and the
/// routine that counts it down (`bf.a()V` @0x129698) also runs the rest of the stage's
/// per-frame work, so it cannot be slowed as a whole. The host instead refills each
/// drop's own byte from 40 back to 120 until it has been given its budget
/// (`hero2java::h2_drop_life_plan`), then leaves it to the game's blink and removal.
/// Polled with the HUD (every 5 frames); one Vector walk while drops are on the ground.
#[derive(Default)]
struct DropLife {
    ledger: Vec<crate::guest_build::H2DropLife>,
}

impl DropLife {
    fn poll(&mut self, emulator: &mut dyn Emulator) {
        let Some(sv) = crate::guest_build::hero2_save() else { return };
        let (plan, count) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let Some(drops) = crate::guest_build::h2_ground_drops(&read, sv.area.stage_slot) else {
                self.ledger.clear();
                return;
            };
            let plan: Vec<(u32, i8)> = crate::guest_build::h2_drop_life_plan(&drops, &mut self.ledger)
                .into_iter()
                .filter_map(|(h, v)| crate::guest_build::h2_drop_life_at(&read, h).map(|a| (a, v)))
                .collect();
            (plan, drops.len())
        };
        for (at, v) in plan {
            let ok = java_write(emulator, crate::guest_build::JavaWrite::GroundDropLife, at, &[v as u8]);
            log::debug!("drop life: {at:#x} <- {v} ({count} on the ground, ok = {ok})");
        }
    }
}

/// ★★ 경험치 배율 (2026-09-23): the EXP-gain multiplier this thread keeps in the guest.
///
/// The mechanism is `guest_anchors::ExpRatePatch`: `a.d(I)V`'s definition word is pointed
/// at a 16-byte host block that scales the argument. What lives here is WHEN. The block
/// is installed lazily — on the first poll that finds the setting above 기존 — so a
/// player who never touches the row runs an image with nothing redirected and the
/// headless baseline is byte-for-byte the shipped boot. Once in, it stays in, with the
/// multiplier word kept equal to the setting: 1 is the identity, so stepping back to
/// 기존 is one word, not an uninstall. Polled with the HUD (every 5 frames): one 4-byte
/// read while nothing changes.
/// ★★ 난이도 (2026-09-29): keep every monster on the stage at the setting's multiplier.
/// The model — which words, the base, the ratio for HP, the `ca` template — is
/// `hero2java::H2Difficulty` (shared with the probe's `--h2-difficulty`); this is the
/// worker's half: it polls with the EXP redirect, writes through the `MonsterStats` gate,
/// and says what it did (one line per poll that wrote anything).
#[derive(Default)]
struct Difficulty {
    plan: crate::guest_build::H2Difficulty,
}

impl Difficulty {
    /// `raid_lap` — the boss raid's lap (`BossRaid`'s model, 0 outside a run).
    fn poll(&mut self, emulator: &mut dyn Emulator, raid_lap: u32) {
        // ★ 2026-10-16 보스 레이드: the raid's maps hold their own percent — ★★ 2026-10-17 (user: *"환경설정 게임 난이도에 따라
        //   달라져야해 … 지금 구현되어있는게 노말 난이도야"*): ⅓ above the setting, 133 · 200 · 267 %, so NORMAL is the raid as first
        //   built (HARD's ×2 on every setting) — `hero2java::h2_difficulty_want`, the probe's rule too. ★★ (2) each lap after the
        //   first ×1.3 HP · ×1.05 ATK on top, ten rises at most (`h2_raid_want`).
        //   ⛔ The live stage's number, not `ag.h` — the load leaves −1 there (`h2_live_map`).
        //   ★ 2026-10-19: guardian mode on HARD is another ×1.5 (`h2_monster_want`) — the round byte `ag.i`.
        let Some(area) = crate::guest_build::hero2_save().map(|s| s.area) else { return };
        let (map, mode) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let mode = crate::guest_build::guardian_patch()
                .and_then(|p| crate::guest_build::h2_rd_u32(&read, p.mode_slot))
                .map(|w| w & 0xff)
                .unwrap_or(0);
            (crate::guest_build::h2_live_map(&read, &area), mode)
        };
        let (want_hp, want_atk) = crate::guest_build::h2_monster_want(map, crate::difficulty(), raid_lap, mode);
        let stage_slot = area.stage_slot;
        let writes = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            // ⛔ Not on the shell: the stage static may still hold the last map, whose
            //   monsters are nobody's — and the next map's start from their own words.
            // ★★ 2026-10-22 (a player: 상점주인의 혼 을 쓴 뒤 받는 피해 20 → 4,000): the gate was `items::in_game_state`
            //   — the state word's in-game SET (3 · 4 · 5 · 8). The shop is 7, the refiner 6: over a hunting map
            //   (the orb) the planner forgot its bases with the monsters standing, and the first field poll after
            //   the shop closed took their scaled words as fresh bases — ×m again on every shop (probe: HARD's
            //   atk 610 → 1,220 the frame the shop closed). Now the world test the demon uses (the title shell or
            //   no character is out — a map change passes through state 1 but the planner keys its memory on the
            //   STAGE OBJECT, `H2Difficulty::plan_hands`, so a new map still starts from its own words).
            let h2 = crate::guest_build::hero2_java();
            if !h2.as_ref().is_some_and(|h2| crate::guest_build::h2_demon_in_world(&read, h2)) {
                self.plan.forget();
                return;
            }
            // ★ 2026-10-21: the raid's 오딘 stage holds his two hands at half its HP percent (`h2_raid_hand_hp_pct`).
            self.plan.plan_hands(&read, stage_slot, want_hp, want_atk, crate::guest_build::h2_raid_hand_hp_pct(map))
        };
        if writes.is_empty() {
            return;
        }
        let mut ok = 0usize;
        for (a, bytes) in &writes {
            if java_write(emulator, crate::guest_build::JavaWrite::MonsterStats, *a, bytes) {
                ok += 1;
            }
        }
        log::info!(
            "난이도: {} (HP {want_hp}% · ATK {want_atk}%, mode {mode}, raid lap {raid_lap}) — {ok}/{} monster word(s) written",
            String::from_utf8_lossy(crate::guest_build::H2_OPT_DIFFICULTY_VALUES[usize::from(crate::guest_build::difficulty_sanitise(crate::difficulty()))]),
            writes.len()
        );
    }
}

/// ★★ 드랍률 (2026-10-16): the equipment-drop filter this thread keeps in the guest — the mechanism is
/// `guest_anchors::DropRatePatch` (`bf.a(BBBB)V`'s definition word names a host block). Installed lazily, the
/// first poll that finds the setting below 높음, so a player who never touches the row runs today's image
/// byte for byte; once in, the KEEP word follows the setting (0x10000 = 높음 = the identity). Polled with the
/// HUD (every 5 frames): one 4-byte read while nothing changes.
#[derive(Default)]
struct DropRate {
    /// The block, and where its data words sit, once installed.
    block: Option<(u32, u32)>,
    /// `alloc_guest` is never freed: a block reserved for a plan that could not be made yet is kept.
    reserved: Option<u32>,
    /// `true` once a write or the verify failed — the game drops as today, and stays so.
    refused: bool,
    /// The "def does not read as shipped" warning, once.
    said: bool,
    /// The counters last logged, so the log says what changed (one line per poll that saw drops).
    seen: (u32, u32),
}

impl DropRate {
    fn poll(&mut self, emulator: &mut dyn Emulator) {
        let want = crate::guest_build::drop_rate_keep(crate::drop_rate());
        let Some((block, words)) = self.block else {
            if want < 0x10000 {
                self.install(emulator, want);
            }
            return;
        };
        let cur = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_drop_rate_words(&read, words)
        };
        let Some((keep, skipped, kept)) = cur else { return };
        if keep != want {
            if emulator.write_memory(words + crate::guest_build::DropRatePatch::KEEP, &want.to_le_bytes()) {
                log::info!("drop rate: keep {want:#x}/0x10000 of the equipment drops (block {block:#x})");
            } else {
                log::warn!("drop rate: the KEEP write at {:#x} failed — still {keep:#x}", words + crate::guest_build::DropRatePatch::KEEP);
            }
        }
        if (skipped, kept) != self.seen {
            self.seen = (skipped, kept);
            log::debug!("drop rate: equipment drops refused {skipped} · passed {kept} since the install");
        }
    }

    /// Install the filter, once: reserve the block, verify the def, write block → def word (the arming write,
    /// last), read it back; a failed verify puts the shipped word back.
    fn install(&mut self, emulator: &mut dyn Emulator, keep: u32) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::drop_rate_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::DropRatePatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("drop rate: no guest block — the equipment drops stay at 높음");
                    self.refused = true;
                    return;
                }
            },
        };
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0x2545_f491, |d| d.subsec_nanos() ^ d.as_secs() as u32) | 1;
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_drop_rate_install_writes(&read, &patch, block, keep, seed)
        };
        let Some((writes, words)) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("drop rate: bf.a(BBBB)V's definition at {:#x} does not read as shipped — not filtered yet", patch.def);
            }
            return;
        };
        let original = emulator.read_memory(patch.def, 4);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("drop rate: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_drop_rate_installed(&read, &patch, block) && crate::guest_build::h2_drop_rate_words(&read, words).map(|w| w.0) == Some(keep)
        };
        if verified {
            self.block = Some((block, words));
            log::info!("drop rate: bf.a(BBBB)V filtered through {block:#x} — keep {keep:#x}/0x10000 of the equipment drops (verified=true)");
            return;
        }
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!(
            "drop rate: the filter did not verify after {done}/{} write(s) — drops stay at 높음, {} (block {block:#x} left unused)",
            writes.len(),
            if undone { "the game is back as it was" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }
}

struct ExpRate {
    /// The host block once the redirect is installed.
    block: Option<u32>,
    /// The block reserved for it, kept across a plan that could not be made yet (the
    /// def not readable): `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    /// `true` once a write or the verify failed — the game is as shipped, and stays so.
    refused: bool,
    /// The "def does not read as shipped" warning, once.
    said: bool,
}

impl ExpRate {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    /// Keep the guest's multiplier equal to the setting.
    fn poll(&mut self, emulator: &mut dyn Emulator) {
        let want = u32::from(crate::exp_rate());
        let Some(block) = self.block else {
            if want > 1 {
                self.install(emulator, want);
            }
            return;
        };
        let cur = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_exp_rate_mult(&read, block)
        };
        if cur != Some(want) {
            let (a, bytes) = crate::guest_build::h2_exp_rate_mult_write(block, want);
            if emulator.write_memory(a, &bytes) {
                log::info!("exp rate: x{want} (block {block:#x})");
            } else {
                log::warn!("exp rate: the multiplier write at {a:#x} failed — still x{}", cur.unwrap_or(0));
            }
        }
    }

    /// Install the redirect, once. Reserves the block, verifies the def, writes block →
    /// def word. Refusal leaves the game exactly as it was: the def word is the arming
    /// write and the last one, and it is put back if the verify fails.
    fn install(&mut self, emulator: &mut dyn Emulator, mult: u32) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::exp_rate_patch() else {
            // Not this build. Quietly: the row is drawn on every build and says 기존.
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::ExpRatePatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("exp rate: no guest block — the multiplier stays at x1");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_exp_rate_install_writes(&read, &patch, block, mult)
        };
        let Some(writes) = plan else {
            // Not latched: the same "too early or moved" ambiguity as the skill-point
            // curve, and the next poll is five frames away. Said once.
            if !self.said {
                self.said = true;
                log::warn!("exp rate: a.d(I)V's definition at {:#x} does not read as shipped — not redirected yet (x1)", patch.def);
            }
            return;
        };
        let original = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            read(patch.def, 4)
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("exp rate: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_exp_rate_installed(&read, &patch, block) && crate::guest_build::h2_exp_rate_mult(&read, block) == Some(mult)
        };
        if verified {
            self.block = Some(block);
            log::info!("exp rate: x{mult} — a.d(I)V redirected through {block:#x} (verified=true)");
            return;
        }
        // The def word is the last write: if it never landed nothing is armed; if it did,
        // put the shipped word back.
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!(
            "exp rate: the redirect did not verify after {done}/{} write(s) — the multiplier stays at x1, {} (block {block:#x} left unused)",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }
}

/// ★★ 2026-10-16 (user: *"엘릭서가 현재 구 아이콘 사용되고있는데 하이포션 아이콘이 포도주 아이콘색상 좀 섞은 듯한 아이콘으로"*): an
/// item's own icon cell — `guest_anchors::ItemIconPatch` on `n.a(IIB)V`: at the six callers that can draw a bag record, a
/// record in its table (엘릭서) draws the strip cell the asset appended (`tools_re/elixir_icon.py`). Installed once from the
/// patch pass, the definition word last, put back on a failed verify. Without it (or with an asset whose strip lacks the
/// cell — the cell is drawn from below the strip, i.e. nothing) the elixir is the orb as before.
struct ItemIcon {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl ItemIcon {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::item_icon_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::ItemIconPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("item icons: no guest block — the elixir keeps the orb's icon");
                    self.refused = true;
                    return;
                }
            },
        };
        let (plan, original) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_item_icon_install_writes(&read, &patch, block), read(patch.def, 4))
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("item icons: n.a(IIB)V's definition at {:#x} does not read as shipped — not installed yet", patch.def);
            }
            return;
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("item icons: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_item_icon_installed(&read, &patch, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!("item icons: n.a(IIB)V through {block:#x} (verified=true) — the elixir draws its own cell");
            return;
        }
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!(
            "item icons: did not verify after {done}/{} write(s) — {}",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }
}

/// ★★ 2026-10-02 (user: *"스킬 퀵슬롯 페이지전환을 했다가 다시 돌아오면 스킬 쿨타임이 원복되어있어"*):
/// the game's cooldown ticks, counted — `guest_anchors::CooldownClockPatch` on `ag.i()V`.
///
/// The game's counters are per SLOT; the plate window moves skills in and out of the slots.
/// So every host seating parks the counter of the skill that leaves (`quickpage::Window::park`,
/// stamped with this count) and seats the arriving one with what its own has left
/// ([`seat_cooldown`]). Installed once from the patch pass, all or nothing, the def word last
/// and put back on a failed verify. The block only counts — nothing in the game changes.
///
/// Without the block (a refused install) the clock reads 0 for ever: a parked counter then
/// does not age while its skill is off the bar — a longer wait, never a free cast.
struct CooldownClock {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

/// The installed block's address, for [`cd_clock_now`] — `apply_input` and the HUD poll both
/// read the clock, and neither is handed the struct. 0 = not installed.
static CD_CLOCK_BLOCK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

impl CooldownClock {
    fn new() -> Self {
        // A second emulator in one process (never today) starts without the last one's block.
        CD_CLOCK_BLOCK.store(0, std::sync::atomic::Ordering::Relaxed);
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::cooldown_clock_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::CooldownClockPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("cooldown clock: no guest block — a skill off the bar keeps its cooldown frozen");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_cd_clock_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("cooldown clock: ag.i()V's definition at {:#x} does not read as shipped — not installed yet", patch.def);
            }
            return;
        };
        let original = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            read(patch.def, 4)
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("cooldown clock: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_cd_clock_installed(&read, &patch, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            CD_CLOCK_BLOCK.store(block, std::sync::atomic::Ordering::Relaxed);
            log::info!("cooldown clock: ag.i()V counted through {block:#x} (verified=true) — a skill's cooldown follows it across page flips and seat changes");
            return;
        }
        // The def word is the last write: if it never landed nothing is armed; if it did, put
        // the shipped word back.
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!(
            "cooldown clock: did not verify after {done}/{} write(s) — parked cooldowns will not age, {}",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }
}

/// ★★ 2026-10-12 (user: *"hero3처럼 마지막에 실행했던 세이브 파일을 자동으로 선택되는 기능 … 메인메뉴나가자마자 ok막 연타해도
/// 3번째가 실행되는거지"*): the slot screen opens on the save slot last played — `guest_anchors::SlotFocusPatch` on
/// `be.a()V` (the screen's first-save finder, run inside the game's own key handler, so an OK queued in the same tick
/// already lands on it).
///
/// LAST = the save slot (`ag.c`) of the last shell → world entry — a load or a new game — kept in the block and in
/// `wie_save_last.txt` (read before the first title, so a restart keeps it). Installed once from the patch pass, all or
/// nothing, the def word last and put back on a failed verify. Nothing is written to a save.
struct SlotFocus {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// The slot the block names (0-based) — what the file said at boot, then each world entry's.
    last: Option<u8>,
}

impl SlotFocus {
    fn new(data_dir: &std::path::Path) -> Self {
        let last = crate::config::read_save_last(data_dir);
        if let Some(s) = last {
            log::info!("slot focus: the slot screen opens on SLOT {} (from wie_save_last.txt)", s + 1);
        }
        Self { block: None, reserved: None, refused: false, said: false, last }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::slot_focus_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::SlotFocusPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("slot focus: no guest block — the slot screen opens on the first save, as shipped");
                    self.refused = true;
                    return;
                }
            },
        };
        let last = crate::guest_build::h2_slot_focus_last(self.last);
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_slot_focus_install_writes(&read, &patch, block, last)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("slot focus: be.a()V's definition at {:#x} does not read as shipped — not installed yet", patch.def);
            }
            return;
        };
        let original = emulator.read_memory(patch.def, 4);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("slot focus: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_slot_focus_installed(&read, &patch, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!(
                "slot focus: be.a()V through {block:#x} (verified=true) — the slot screen opens on {}",
                self.last.map_or("the first save (no slot played yet)".to_string(), |s| format!("SLOT {} when it holds a save", s + 1))
            );
            return;
        }
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!(
            "slot focus: did not verify after {done}/{} write(s) — the slot screen opens on the first save, {}",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }

    /// The player entered the world from save slot `slot` (0-based): the block's LAST and the file follow it.
    fn note(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, slot: u8) {
        if slot >= crate::guest_build::SaveSlotsPatch::SLOTS {
            return;
        }
        if self.last != Some(slot) {
            self.last = Some(slot);
            crate::config::write_save_last(data_dir, slot);
            log::info!("slot focus: SLOT {} is the last played — the slot screen opens there", slot + 1);
        }
        if let Some(block) = self.block {
            let want = crate::guest_build::h2_slot_focus_last(Some(slot)).to_le_bytes();
            if emulator.read_memory(block + crate::guest_build::SlotFocusPatch::LAST, 4) != want {
                let _ = emulator.write_memory(block + crate::guest_build::SlotFocusPatch::LAST, &want);
            }
        }
    }
}

/// ★★ 2026-10-05 (user: *"버프스킬 발동되고있는 중에 스킬쿨이 돌아서 다시 사용하면 … 남은 10초만 지속되는거
/// 같아"*): a buff cast while the same buff is up restarts its duration —
/// `guest_anchors::BuffRefreshPatch` on `g.a(BI)V`, which as shipped refuses a buff already in
/// the list (the recast paid SP and cooldown and bought only the old buff's remainder).
///
/// Installed once from the patch pass, all or nothing, the def word last and put back on a failed
/// verify. Nothing is written to the save: the buffs are not saved, and without the block the
/// game is as shipped.
struct BuffRefresh {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl BuffRefresh {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::buff_refresh_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::BuffRefreshPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("buff refresh: no guest block — a recast buff keeps only what it had left");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_buff_refresh_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("buff refresh: g.a(BI)V / g.b(BBI)V definitions at {:#x} / {:#x} do not read as shipped — not installed yet", patch.def, patch.beast_def);
            }
            return;
        };
        // The two def words as shipped — the arming writes, put back if the install does not verify.
        let originals: Vec<(u32, Vec<u8>)> = [patch.def, patch.beast_def].iter().map(|d| (*d, emulator.read_memory(*d, 4))).collect();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("buff refresh: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_buff_refresh_installed(&read, &patch, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!("buff refresh: g.a(BI)V through {block:#x}, g.b(BBI)V through {:#x} (verified=true) — a recast buff, the beast's too, restarts its duration", block + crate::guest_build::BuffRefreshPatch::BEAST_AT);
            return;
        }
        // The def words are the last writes: every one that landed is put back as shipped (the
        // block alone arms nothing).
        let mut undone = true;
        for (d, word) in &originals {
            if writes[..done].iter().any(|(a, _)| a == d) {
                undone &= word.len() == 4 && emulator.write_memory(*d, word);
            }
        }
        log::warn!(
            "buff refresh: did not verify after {done}/{} write(s) — a recast buff keeps its remainder, {}",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }
}

/// ★★ 2026-10-06 (user: *"번개난타를 바꿀수도 있어? 살상연희 스킬로 바꾸고싶어"* · *"로그는 투명한 몬스터들은 처음 겜 시작부터
/// 볼 수있는거야"* → 「반투명」): the rogue's 번개난타 slot is 「살상연희」 (a 5×5 of nearest monsters, teleporting behind each
/// in turn, see-through and untouchable, 10 s) and a rogue with 「심안」 sees the ghosts half see-through —
/// `guest_anchors::RoguePatch` on `ay.b(B)Z` · `ay.a()Z` · `ay.a(I)S` · `a.a(BBII)V` (CHAINED onto the 일풍격 and accessory
/// wrappers — so it waits for them) and `bd.a(II)V`. Installed once from the patch pass, all or nothing, the definition words
/// last and put back on a failed verify. Nothing is written to the save.
struct Rogue {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// ★★ 2026-10-14 신규 아이템 2차 D: the 살상연희 gear words as last written (`RoguePatch::gear_words`).
    gear: [u32; 6],
}

impl Rogue {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false, gear: [0; 6] }
    }

    /// ★★ 2026-10-14 신규 아이템 2차 D (살상연희 타수 — 백 번째 칼날 · 무모한 칼춤 · 피의 연쇄 · 마지막 춤): the worn items and the
    /// class → the block's gear words (outside the world, or with no class, all 0 = the shipped run). The drive reads them
    /// at a run's frame 0 and on each hop / blow.
    fn poll(&mut self, emulator: &mut dyn Emulator, in_game: bool, class: Option<u8>) {
        type P = crate::guest_build::RoguePatch;
        let Some(block) = self.block else { return };
        let words = match (in_game, class, crate::guest_build::gear_patch()) {
            (true, Some(c), Some(gp)) => {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                P::gear_words(&crate::guest_build::h2_worn_all(&read, gp.char_slot), c)
            }
            _ => [0; 6],
        };
        if words == self.gear {
            return;
        }
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        const _: () = assert!(P::GEAR_FLAGS == P::GEAR_HITS + 4 && P::GEAR_MUL == P::GEAR_HITS + 16 && P::GEAR_MAX == P::GEAR_HITS + 20, "the six words in a row");
        if emulator.write_memory(block + P::GEAR_HITS, &bytes) {
            log::info!("rogue: 살상연희 gear words {words:?} (more blows · flags · a kill's · their cap · every blow's × · the run's cap)");
            self.gear = words;
        }
    }

    /// `ready`: the blocks it chains onto (일풍격, and through it the accessory block) are in or refused.
    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let (Some(patch), Some(acc)) = (crate::guest_build::rogue_patch(), crate::guest_build::accessory_patch()) else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::RoguePatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("rogue: no guest block — 번개난타 stays as shipped and the ghosts stay unseen");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_rogue_install_writes(&read, &patch, &acc, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("rogue: bd.a(II)V at {:#x} is not as shipped, or a chained definition is not a Thumb entry — not installed yet", patch.mob_paint.0);
            }
            return;
        };
        let arms = crate::guest_build::h2_rogue_arms(&patch, block);
        let originals: Vec<(u32, Vec<u8>)> = arms.iter().map(|(d, _)| (*d, emulator.read_memory(*d, 4))).collect();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("rogue: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_rogue_installed(&read, &patch, &acc, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!(
                "rogue: 살상연희 through {block:#x} (ay.b(B)Z · ay.a()Z · ay.a(I)S · a.a(BBII)V chained, cooldown {} frames), bd.a(II)V through {:#x} (a rogue with 심안 sees the ghosts at alpha {}) — verified=true",
                crate::guest_build::RoguePatch::COOLDOWN,
                block + crate::guest_build::RoguePatch::SIGHT_AT,
                crate::guest_build::RoguePatch::SIGHT_ALPHA
            );
            return;
        }
        let mut undone = true;
        for (d, word) in &originals {
            if writes[..done].iter().any(|(a, _)| a == d) {
                undone &= word.len() == 4 && emulator.write_memory(*d, word);
            }
        }
        log::warn!(
            "rogue: did not verify after {done}/{} write(s) — {}",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }
}

/// ★★ 2026-10-13 「악마화」 D1 — the 악의 block (user: *"대지분노 → 악의 … 변신"*): the host wraps the beast key cast,
/// the key arming, the three class drives, the character update, the beast SP / cooldown tables and 철괴's hook word
/// with ONE block, so that 악의 turns the character demon for a minute — received damage −15 %, the class's attack
/// skills refused, the 기본공격 stages the claw combo. See `guest_anchors::DemonPatch` (the mechanism, the user's
/// numbers) and `hero2java::h2_demon_*` (the pieces, the plan, the check). Installed AFTER every block whose words it
/// chains (the gear block last among them), all or nothing, rolled back when it does not read back.
struct Demon {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// ★★ 2026-10-16 THE SKIN ([`Self::poll_skin`]): the beast table / icon array the host exchanged into the 악의 versions,
    /// with the handle it put in the first exchanged slot — a container that no longer holds it is a fresh one.
    skin_rows: Option<(u32, u32)>,
    skin_icons: Option<(u32, u32)>,
    /// ★★ 2026-10-17 PAUSE as last written (`None` = not yet).
    paused: Option<bool>,
}

impl Demon {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false, skin_rows: None, skin_icons: None, paused: None }
    }

    /// ★★ 2026-10-17 (user: *"보스레이드도 보스를 잡은 뒤에는 멈추게하고싶어. 다시 진행하면 그때 다시 흐르는거고"*): the raid between
    /// a kill and the next stage (`H2BossRaid::paused`) → the block's PAUSE word, written on a change — the TICK piece stands
    /// the demon's clock while it is set (and by itself while a script is up). Nothing without the block.
    fn set_pause(&mut self, emulator: &mut dyn Emulator, on: bool) {
        let Some(block) = self.block else { return };
        if self.paused == Some(on) {
            return;
        }
        if emulator.write_memory(block + crate::guest_build::DemonPatch::PAUSE, &u32::from(on).to_le_bytes()) {
            if self.paused.is_some() || on {
                log::info!("demon: the clock {} (the raid between two stages)", if on { "stands" } else { "runs again" });
            }
            self.paused = Some(on);
        }
    }

    /// ★★ 2026-10-16 (user: *"악의로 스킬이 바뀌면 야수 스킬창에서도 이름·아이콘 변경 … 스킬설명도 간단히"*): THE SKIN — while
    /// the block is in STATE 2 the beast table `bw.b` and the icon array `cf.i` hold the 악의 versions of the six skills it
    /// changes (`DemonPatch::SKIN_KINDS`; the asset appends them after the shipped rows / frames), and the shipped order
    /// again when it ends: each changed kind's slot EXCHANGED with its appended one (`hero2java::h2_demon_skin_*` — every
    /// object stays referenced by the game's own array, nothing pinned). The host HUD's beast icons follow
    /// (`crate::DEMON_SKIN`). The window chain repaints after a change (an open beast window shows it at once).
    fn poll_skin(&mut self, emulator: &mut dyn Emulator) {
        type P = crate::guest_build::DemonPatch;
        let want = self.block.is_some() && self.state(&*emulator) == Some(P::STATE_ON as i32);
        crate::DEMON_SKIN.store(want, std::sync::atomic::Ordering::Relaxed);
        let (table, icons) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_beast_text_table(&read), crate::guest_build::h2_beast_icon_array(&read))
        };
        let rows = Self::skin_one(emulator, &mut self.skin_rows, table, want, true);
        let icons = Self::skin_one(emulator, &mut self.skin_icons, icons, want, false);
        if rows || icons {
            Accessory::repaint_chain(emulator);
        }
    }

    /// One container of [`Self::poll_skin`] (`rows`: the beast table, else the icon array): `true` when it was exchanged.
    fn skin_one(emulator: &mut dyn Emulator, state: &mut Option<(u32, u32)>, container: Option<u32>, want: bool, rows: bool) -> bool {
        let rd32 = |emulator: &dyn Emulator, a: u32| -> Option<u32> { emulator.read_memory(a, 4).try_into().ok().map(u32::from_le_bytes) };
        let pairs = container.and_then(|c| {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            if rows { crate::guest_build::h2_demon_skin_row_pairs(&read, c) } else { crate::guest_build::h2_demon_skin_icon_pairs(&read, c) }
        });
        // the remembered exchange still standing in the container in place now?
        let standing = match (*state, container, pairs.as_ref()) {
            (Some((c, placed)), Some(now), Some(p)) if c == now && !p.is_empty() => rd32(&*emulator, p[0].0) == Some(placed),
            _ => false,
        };
        if !standing {
            *state = None; //                          a fresh container (or none): the shipped order
        }
        if want == standing {
            return false;
        }
        let (Some(c), Some(p)) = (container, pairs) else { return false };
        let writes = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_exchange_writes(&read, &p)
        };
        let Some(writes) = writes else { return false };
        let kind = if rows { crate::guest_build::JavaWrite::TableRowRef } else { crate::guest_build::JavaWrite::IconArrayRef };
        let originals: Vec<Option<u32>> = writes.iter().map(|(a, _)| rd32(&*emulator, *a)).collect();
        for (i, (a, h)) in writes.iter().enumerate() {
            if !java_write(emulator, kind, *a, &h.to_le_bytes()) {
                for ((ra, _), orig) in writes[..i].iter().zip(&originals) {
                    if let Some(o) = orig {
                        java_write(emulator, kind, *ra, &o.to_le_bytes());
                    }
                }
                log::warn!("demon: skin {} write {i} at {a:#x} failed — put back", if rows { "row" } else { "icon" });
                return false;
            }
        }
        *state = want.then_some((c, writes[0].1));
        log::info!(
            "demon: skin — the beast {} at {c:#x} {} ({} slot(s))",
            if rows { "table" } else { "icons" },
            if want { "in the 악의 versions" } else { "back in the shipped order" },
            writes.len()
        );
        true
    }

    /// `ready`: the blocks it chains onto have settled. `combat`: the combat block, whose 철괴 piece carries the hook word.
    /// ★★ 2026-10-15 `gear`: the gear block (its 2차 C words — 악의 장비 — read through GEAR_PTR).
    /// ★★ 2026-10-17 `berserker`: the berserker block (격분's state — the claw's speed-up at 100 %, read through FURY_PTR).
    /// ★★ 2026-10-18 `accessory`: the accessory block (야수조련 목걸이's ring — ACC_PTR).
    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool, combat: Option<u32>, gear: Option<u32>, berserker: Option<u32>, accessory: Option<u32>) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let (Some(patch), Some(cp)) = (crate::guest_build::demon_patch(), crate::guest_build::combat_patch()) else {
            self.refused = true;
            return;
        };
        let hook = combat.and_then(|c| crate::guest_build::h2_iron_hook_at(&cp).map(|h| c + h));
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::DemonPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("demon: no guest block — 대지분노 stays the beast's own skill");
                    self.refused = true;
                    return;
                }
            },
        };
        // the host's roar takes the place of the game's 사자후 효과음 when the clip decodes
        let voice = crate::voice::voice_clip(crate::guest_build::H2Voice::Malice.index()).is_some();
        // ★★ 2026-10-15 HD: the host composites the body and the effects at the window's resolution (`wie_hd_fx.txt`)
        let hd = crate::HD_FX.load(std::sync::atomic::Ordering::Relaxed);
        // ★★ 2026-10-17 검풍 from a claw: the combat block's WIND piece · 격분's speed on a claw: the berserker block
        let wind = combat.map(|c| c + crate::guest_build::CombatPatch::WIND_AT);
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_demon_chain(&read, &patch, block, hook).and_then(|chain| {
                crate::guest_build::h2_demon_install_writes(&read, &patch, block, hook, voice, gear, hd, wind, berserker, accessory).map(|w| (chain, w))
            })
        };
        let Some((chain, writes)) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("demon: a definition is not a Thumb entry outside the block or the hook word reads foreign — not installed");
            }
            return;
        };
        let arms = crate::guest_build::h2_demon_arms(&patch, block, &chain, hook);
        let originals: Vec<(u32, Vec<u8>)> = arms.iter().map(|(a, before, _)| (*a, emulator.read_memory(*a, before.len()))).collect();
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("demon: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_demon_installed(&read, &patch, block, &chain, hook)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!(
                "demon: 악의 block at {block:#x} (verified=true, hook={hook:?}, voice={voice}, hd={hd}) — 대지분노 turns the character demon for {} ticks, cooldown {} from the cast",
                crate::guest_build::DemonPatch::FRAMES,
                crate::guest_build::DemonPatch::COOLDOWN
            );
            return;
        }
        let mut undone = done < writes.len();
        if !undone {
            undone = originals.iter().all(|(a, bytes)| emulator.write_memory(*a, bytes));
        }
        log::warn!(
            "demon: did not verify after {done}/{} write(s) — {}",
            writes.len(),
            if undone { "the game is back as it was" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }

    /// The block's words (`None` = no block).
    fn words(&self, emulator: &dyn Emulator) -> Option<crate::guest_build::H2Demon> {
        let block = self.block?;
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        crate::guest_build::h2_demon_words(&read, block)
    }

    /// STATE as the block holds it (`None` = no block).
    fn state(&self, emulator: &dyn Emulator) -> Option<i32> {
        self.words(emulator).map(|w| w.state)
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, in_world: bool, class: Option<u8>) {
        let Some(block) = self.block else {
            return;
        };
        let Some(w) = self.words(&*emulator) else { return };
        let body_is_demon = matches!(w.state, 1 | 2);
        let plan = crate::guest_build::h2_demon_wing_plan(w.state, w.look, class, in_world);
        let writes = crate::guest_build::h2_demon_look_writes(block, &plan, class);
        if writes.is_empty() {
            return;
        }
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("demon: the look write at {a:#x} failed");
                return;
            }
        }
        log::info!(
            "demon: look — state {} look {} body-is-demon {body_is_demon} in-world {in_world} → body {:?} look {:?} state {:?} table {} (class {class:?})",
            w.state, w.look, plan.body, plan.look, plan.state, plan.table
        );
    }

    /// ★ D2: 악의's buff cells for the HUD — the beast strip's 악의 icon with the whole seconds left (STATE 1 · 2); ★ D3
    /// and the 암살자 / 심판번개 cells shown ON while 지옥칼날 / 어둠심판 run.
    fn buffs(&self, emulator: &dyn Emulator) -> Vec<crate::guest_build::H2BuffShow> {
        self.words(emulator).map(|w| crate::guest_build::h2_demon_buffs(&w)).unwrap_or_default()
    }

    /// ★ D4: 추종's gauge (0..=100) while it runs under 악의 — the bar under its buff cell; `None` otherwise.
    fn gauge(&self, emulator: &dyn Emulator) -> Option<u8> {
        self.words(emulator).and_then(|w| crate::guest_build::h2_demon_gauge(&w))
    }
}

/// ★★ 2026-10-11 (user: *"도적 3마리있는 지역에서 로그로 순간이동 스킬 … 가운데 시체 … 에서 순간이동 스킬을 쓰면 또 넘어갈
/// 수있어"* → *"그냥 해당 퀘스트 상황에서는 순간이동 스킬 자체 못쓰게 막아버리는게 낫지 않을까?"*): while the three 도적's
/// corpses fence 맵 18 (event 0x10 on and a corpse NPC shown — ★ 2026-10-15: they stand until the next map load even
/// after the search sets 0x11) the rogue's teleports are not cast — `guest_anchors::TeleportGatePatch`,
/// the outermost wrapper on `ay.b(B)Z`, installed after the rogue block (the last other wrapper of that word). The poll
/// writes MASK (only on a change): the scene's kinds with the worn accessory's (일섬 · 극사), 0 everywhere else. Nothing
/// is written to a record or a save; without the block the teleports are as before.
struct TeleportGate {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// MASK as last written.
    mask: u32,
}

impl TeleportGate {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false, mask: 0 }
    }

    /// `ready`: the rogue block (the last other wrapper on `ay.b(B)Z`) is in or refused.
    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let Some(patch) = crate::guest_build::teleport_gate_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::TeleportGatePatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("teleport gate: no guest block — the rogue's teleports stay usable in the corpses' scene");
                    self.refused = true;
                    return;
                }
            },
        };
        let (plan, original) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_teleport_gate_install_writes(&read, &patch, block), read(patch.def, 4))
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("teleport gate: ay.b(B)Z's definition at {:#x} is not a Thumb entry — not installed yet", patch.def);
            }
            return;
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("teleport gate: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_teleport_gate_installed(&read, &patch, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!("teleport gate: ay.b(B)Z through {block:#x} (verified=true) — no rogue teleport while 맵 18's corpses lie there");
            return;
        }
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!(
            "teleport gate: did not verify after {done}/{} write(s) — {}",
            writes.len(),
            if undone { "the game is back as it was" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }

    /// The scene (map · events) and the worn accessory → MASK.
    fn poll(&mut self, emulator: &mut dyn Emulator, in_game: bool) {
        type P = crate::guest_build::TeleportGatePatch;
        let Some(block) = self.block else { return };
        let mask = if in_game {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let scene = crate::guest_build::hero2_save().and_then(|sv| {
                let map = crate::guest_build::h2_stage_map_no(&read, &sv.area)?;
                let flags = crate::guest_build::h2_event_flags(&read, &sv.area)?;
                // ★ 2026-10-15: the corpses stand until the next map load, 0x11 or not — ask the live NPCs.
                Some(P::fenced(map, &flags, || crate::guest_build::h2_npc_rec_shown(&read, sv.area.stage_slot, P::CORPSE_REC)))
            });
            if scene == Some(true) { P::kinds_mask(crate::ACCESSORY.load(std::sync::atomic::Ordering::Relaxed)) } else { 0 }
        } else {
            0
        };
        if mask != self.mask && emulator.write_memory(block + P::MASK, &mask.to_le_bytes()) {
            log::info!("teleport gate: MASK {:#x} -> {mask:#x}", self.mask);
            self.mask = mask;
        }
    }
}

/// ★ 2026-10-08 천공의검's picture (user: *"3갈래를 차례대로 그으면서 … 마지막 폭발"*) — `guest_anchors::SkyDrawPatch`: a
/// wrapper on the character draw, CHAINED onto what that definition holds (the accessory / rogue wrappers), drawing the
/// three tears, the cracks and the flash with the game's own Graphics; ★ 2026-10-08 (3) and a wrapper on the ground
/// draw `bf.a(Graphics;II)V` drawing 개화's floor (the rogue block's FLOWER word — its address goes in the data) under
/// every monster, fading out after. Installed once from the patch pass after the rogue block (the last to chain the
/// character draw), everything or nothing; both definition words go back on a failed verify. Nothing of the game's
/// changes without it — the knight's action just has no picture but its pose, 개화 no floor.
struct SkyDraw {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
}

impl SkyDraw {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false }
    }

    /// `ready`: the rogue block (the last wrapper on the character draw) is in or refused; `rogue` = its block (the
    /// floor reads its FLOWER word — none, no floor); ★ 2026-10-09 `berserker` = the berserker block (격분's blood
    /// reads its record — none, no blood).
    fn install(&mut self, emulator: &mut dyn Emulator, ready: bool, rogue: Option<u32>, berserker: Option<u32>) {
        if self.block.is_some() || self.refused || !ready {
            return;
        }
        let Some(patch) = crate::guest_build::sky_draw_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::SkyDrawPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("sky draw: no guest block — 천공의검 keeps its pose alone");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let flower = rogue.map_or(0, |b| b + crate::guest_build::RoguePatch::FLOWER);
            crate::guest_build::h2_sky_draw_install_writes(&read, &patch, block, flower, berserker.unwrap_or(0))
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!(
                    "sky draw: a.a(BBII)V's definition at {:#x}, bf.a(Graphics;II)V's at {:#x} or n.d()V's at {:#x} is not a Thumb entry outside the block — not installed yet",
                    patch.draw,
                    patch.ground_draw.0,
                    patch.hud_draw.0
                );
            }
            return;
        };
        let original = emulator.read_memory(patch.draw, 4);
        let original_ground = emulator.read_memory(patch.ground_draw.0, 4);
        let original_hud = emulator.read_memory(patch.hud_draw.0, 4);
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("sky draw: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let installed = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_sky_draw_installed(&read, &patch, block)
        };
        if installed && done == writes.len() {
            self.block = Some(block);
            log::info!(
                "sky draw: 천공의검's tears · cracks · flash, 개화's floor and 격분's blood through {block:#x} (a.a(BBII)V, bf.a(Graphics;II)V and n.d()V chained, floor {}, blood {}) — verified=true",
                if rogue.is_some() { "on" } else { "off: no rogue block" },
                if berserker.is_some() { "on" } else { "off: no berserker block" }
            );
            return;
        }
        let undone = original.len() == 4
            && original_ground.len() == 4
            && original_hud.len() == 4
            && emulator.write_memory(patch.draw, &original)
            && emulator.write_memory(patch.ground_draw.0, &original_ground)
            && emulator.write_memory(patch.hud_draw.0, &original_hud);
        log::warn!(
            "sky draw: did not verify after {done}/{} write(s) — {}",
            writes.len(),
            if undone { "the draw is back as it was" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }
}

/// The cooldown clock: how many times the game's `ag.i()V` has run. 0 without the block.
fn cd_clock_now(emulator: &dyn Emulator) -> u32 {
    let block = CD_CLOCK_BLOCK.load(std::sync::atomic::Ordering::Relaxed);
    if block == 0 {
        return 0;
    }
    let read = |a: u32, l: usize| emulator.read_memory(a, l);
    crate::guest_build::h2_cd_clock_read(&read, block).unwrap_or(0)
}

/// Guest slot `g` is about to take `value` (`None` = a clear) — by the host's write or the
/// game's own registration: park the counter of the skill the slot holds now. Returns the
/// counter of the slot `value` already sits in, when it is on the bar. Call it BEFORE the
/// write; that changes the bar.
fn park_evicted(emulator: &dyn Emulator, h2: &crate::guest_build::Hero2Java, window: &mut quickpage::Window, g: u8, value: Option<u8>) -> Option<i32> {
    let now = cd_clock_now(emulator);
    let seat = {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let seat = crate::guest_build::h2_seat_cooldown(&read, h2, g, value).unwrap_or_default();
        // ★★ 2026-10-17 쌍월: the slot its window freed is still cooling — 발도 leaves (or moves) with the counter it had
        let combat = COMBAT_BLOCK.load(std::sync::atomic::Ordering::Relaxed);
        let twin = if combat == 0 { None } else { crate::guest_build::h2_twin_window(&read, combat) };
        let value_slot = value.and_then(|v| crate::guest_build::h2_quickbar_bytes(&read, h2)?.iter().position(|b| *b > 0 && *b as u8 == v)).map(|i| i as u8);
        crate::guest_build::h2_twin_seat(seat, twin, g, value, value_slot)
    };
    if let Some((old, left)) = seat.evicted {
        window.park(old, left, items::h2_cd_peak(g), now);
        if left > 0 {
            log::info!("quickpage: skill {old} leaves guest slot {g} with {left} tick(s) of cooldown — parked at clock {now}");
        }
    }
    seat.moved
}

/// The HOST is about to write `value` (`None` = a clear) into guest slot `g`: park the
/// counter of the skill that slot holds ([`park_evicted`]), and return the counter the slot
/// is to start with — the arriving skill's own (still on the bar: its slot's; off the bar:
/// what is left of its parked one), `-1` when it is ready.
fn seat_cooldown(emulator: &dyn Emulator, h2: &crate::guest_build::Hero2Java, window: &mut quickpage::Window, g: u8, value: Option<u8>) -> i32 {
    let moved = park_evicted(emulator, h2, window, g, value);
    let now = cd_clock_now(emulator);
    let Some(v) = value else { return -1 };
    let (left, total) = match moved {
        Some(left) => {
            window.forget_parked(v);
            (left, left)
        }
        None => window.unpark(v, now).unwrap_or((-1, 0)),
    };
    items::h2_cd_peak_seated(g, total);
    left
}

/// ★★ 의상 아바타 A4 (2026-10-01, user: *"노말보상으로 이안시리즈 아이템을 4개 or 5개 … 하드코어
/// 클리어 보상으로는 율리케 시리즈 아바타 3개"*): the round-clear rewards.
///
/// `guest_anchors::ClearHookPatch` counts every run of `ag.y()V` — the game's next-round start
/// after an ending (its one caller is event 9 with `ag.e == 2`), the HELL clear included — and
/// keeps the mode it cleared. This thread installs it once from the patch pass (all or nothing,
/// the def word last and put back on a failed verify), polls the count with the HUD, and pays
/// that mode's sets to the character in the world (`AvatarBook::clear_rewards`: each set's items
/// for its class, once per character, none worn, kept only if the game then saves — ★ A5 the HELL
/// 수영장 파티 set (★ 2026-10-02 — 할로윈 until then) only for a round whose done quests were 95 % at
/// 난이도 NORMAL or above — the marks `AvatarBook::observe_quests` keeps from the HUD poll). Then the
/// game's own field notice per set paid (「이안 아바타 획득」 ·
/// 「율리케 아바타 획득」 · 「수영장 아바타 획득」 · 「할로윈 아바타 획득」, `/t/itmtp` rows the asset tool appends)
/// once the field is up again. The block only counts — nothing in the game changes.
struct ClearHook {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// The count paid up to.
    seen: u32,
    /// Notices still to raise (`/t/itmtp` rows) — in the field.
    notices: Vec<u32>,
}

impl ClearHook {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false, seen: 0, notices: Vec::new() }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::clear_hook_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::ClearHookPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("clear hook: no guest block — no round-clear avatar rewards");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_clear_hook_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("clear hook: ag.y()V's definition at {:#x} does not read as shipped — not installed yet", patch.def);
            }
            return;
        };
        let original = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            read(patch.def, 4)
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("clear hook: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let read_back = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_clear_hook_installed(&read, &patch, block), crate::guest_build::h2_clear_hook_read(&read, block))
        };
        if let (true, Some((count, _))) = read_back {
            if done == writes.len() {
                self.block = Some(block);
                self.seen = count;
                log::info!("clear hook: ag.y()V counted through {block:#x} (verified=true) — each mode pays its sets (avatar_state::CLEAR_REWARDS; a GUARDIAN clear pays HELL's too)");
                return;
            }
        }
        // The def word is the last write: if it never landed nothing is armed; if it did, put
        // the shipped word back.
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!(
            "clear hook: did not verify after {done}/{} write(s) ({read_back:?}) — no round-clear rewards, {}",
            writes.len(),
            if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" }
        );
        self.refused = true;
    }

    /// With the HUD poll: a new clear pays its mode's reward, and a notice waits for the field.
    #[allow(clippy::too_many_arguments)]
    fn poll(
        &mut self,
        emulator: &mut dyn Emulator,
        book: &mut crate::avatar_state::AvatarBook,
        io: &dyn crate::avatar_state::AvatarIo,
        catalog: &[crate::guest_build::H2CostumeDef],
        sheets: &crate::guest_build::AvatarSheets,
        class: Option<u8>,
    ) {
        // ★ 2026-10-02: the notices are not the hook's alone — the rewards a loaded save had earned
        //   already (`AvatarBook::grant_earned`, on entering the world) queue theirs here too, so they
        //   are raised with or without the hook installed.
        let (got, field) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let field = crate::guest_build::app_state().is_some_and(|st| items::app_state_word(&read) == Some(st.field()));
            (self.block.and_then(|block| crate::guest_build::h2_clear_hook_read(&read, block)), field)
        };
        if let Some((count, mode)) = got.filter(|(count, _)| *count != self.seen) {
            let runs = count.wrapping_sub(self.seen);
            self.seen = count;
            if runs > 1 {
                log::warn!("clear hook: {runs} clears since the last poll — the last one's mode ({mode}) is paid");
            }
            match u8::try_from(mode) {
                Ok(m) => {
                    log::info!("clear hook: ag.y()V ran — {} cleared ({})", crate::avatar_state::mode_label(m), crate::avatar_state::class_label(class));
                    let slots_for = |set: u16| crate::guest_build::h2_av_set_slots_for(catalog, sheets, set, class);
                    // ★ A5: every reward of the mode — HELL pays two when the round was played at
                    // NORMAL — each with its own notice, in the order paid.
                    for out in book.clear_rewards(io, m, &slots_for) {
                        if let crate::avatar_state::ClearOutcome::Paid { reward, .. } = out {
                            self.notices.push(reward.notice_row);
                        }
                    }
                    // ★ 2026-10-16 보스 레이드: a GUARDIAN clear at HARD opens the temple's gate — the next 신전 load (the way
                    //   back from 가디언) shows it. Kept only with a save, like the receipt it reads.
                    if book.cleared_guardian_hard() {
                        crate::raid_gate::set(true);
                    }
                }
                Err(_) => log::warn!("clear hook: ag.y()V ran with mode word {mode} — not a mode, nothing paid"),
            }
        }
        if self.notices.is_empty() || !field {
            return;
        }
        let row = self.notices.remove(0);
        let text = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_text_row(&read, crate::guest_build::Hero2Java::ITEM_TYPE_TABLE_SLOT, row)
        };
        match text {
            Some(text) => {
                let args = [wie_backend::JavaArg::Object(text), wie_backend::JavaArg::Int(16), wie_backend::JavaArg::Int(0)];
                let _ = java_call(emulator, crate::guest_build::JavaCall::Notice, JavaThis::None, &args);
            }
            None => log::warn!("clear hook: /t/itmtp row {row} is missing — was the asset regenerated?"),
        }
    }
}

/// ★★ 2026-10-22 가디언 레벨 (GL): the level-up's wrapper (`guest_anchors::GuardianLevelPatch`) — installed once in the
/// patch pass; armed for the character entering the world from its record's GL (`h2_guardian_level_arm_plan`) once the
/// field shows it; disarmed at the shell. Every frame its GL is copied into the record (`AvatarBook::set_guardian_level`
/// — a save carries it with the stat points the same guest routine gave), and a GL-up is said in the game's notice box.
/// The HUD's GL number and bar are laid over the stats ([`GuardianLevel::overlay`]).
struct GuardianLevel {
    block: Option<u32>,
    /// `alloc_guest` is never freed, so it must not be asked twice.
    reserved: Option<u32>,
    refused: bool,
    said: bool,
    /// The world was entered: arm at the first field frame that reads a character.
    want_arm: bool,
    /// The GL the record was last given (`None` = not armed since the world entry).
    told: Option<u16>,
    /// ★ P1: the block's save count last looked at.
    saves_seen: u32,
}

impl GuardianLevel {
    fn new() -> Self {
        Self { block: None, reserved: None, refused: false, said: false, want_arm: false, told: None, saves_seen: 0 }
    }

    fn install(&mut self, emulator: &mut dyn Emulator) {
        if self.block.is_some() || self.refused {
            return;
        }
        let Some(patch) = crate::guest_build::guardian_level_patch() else {
            self.refused = true;
            return;
        };
        let block = match self.reserved {
            Some(b) => b,
            None => match emulator.alloc_guest(crate::guest_build::GuardianLevelPatch::BLOCK_LEN) {
                Some(b) => {
                    self.reserved = Some(b);
                    b
                }
                None => {
                    log::warn!("guardian level: no guest block — LV 99 stays the shipped cap");
                    self.refused = true;
                    return;
                }
            },
        };
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_guardian_level_install_writes(&read, &patch, block)
        };
        let Some(writes) = plan else {
            if !self.said {
                self.said = true;
                log::warn!("guardian level: a.l()V's definition at {:#x} does not read as shipped — not installed yet", patch.def);
            }
            return;
        };
        let original = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            read(patch.def, 4)
        };
        let mut done = 0usize;
        for (a, bytes) in &writes {
            if !emulator.write_memory(*a, bytes) {
                log::warn!("guardian level: write {done} of {} at {a:#x} failed", writes.len());
                break;
            }
            done += 1;
        }
        let verified = done == writes.len() && {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_guardian_level_installed(&read, &patch, block)
        };
        if verified {
            self.block = Some(block);
            log::info!(
                "guardian level: a.l()V wrapped through {block:#x} (verified=true) — GL 1..={} after LV 99, {} stat points a GL, step {} + {} a GL",
                crate::guest_build::GuardianLevelPatch::MAX_GL,
                crate::guest_build::GuardianLevelPatch::POINTS_PER_GL,
                crate::guest_build::GuardianLevelPatch::BASE_EXP,
                crate::guest_build::GuardianLevelPatch::STEP_EXP
            );
            return;
        }
        let undone = done < writes.len() || (original.len() == 4 && emulator.write_memory(patch.def, &original));
        log::warn!("guardian level: did not verify after {done}/{} write(s) — no GL, {}", writes.len(), if undone { "the game is back as shipped" } else { "AND THE ROLLBACK FAILED" });
        self.refused = true;
    }

    /// ★ P1 (review 2026-10-22): a save the guest made since the last look — the GL its `/g` writer snapshotted, for the
    /// commit of that save ([`crate::avatar_state::AvatarBook::note_save_gl`]). Read right before the book commits.
    fn save_snapshot(&mut self, emulator: &mut dyn Emulator) -> Option<u16> {
        let block = self.block?;
        let (saves, gl) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_guardian_level_saved(&read, block)?
        };
        if saves == self.saves_seen {
            return None;
        }
        self.saves_seen = saves;
        u16::try_from(gl.min(crate::guest_build::GuardianLevelPatch::MAX_GL)).ok()
    }

    /// The world was entered from the shell (after the book took the character's record).
    fn enter_world(&mut self) {
        self.want_arm = self.block.is_some();
        self.told = None;
    }

    /// Back to the shell: the block waits for the next character.
    fn leave_world(&mut self, emulator: &mut dyn Emulator) {
        self.want_arm = false;
        self.told = None;
        if let Some(block) = self.block {
            let (a, bytes) = crate::guest_build::h2_guardian_level_disarm_write(block);
            if !emulator.write_memory(a, &bytes) {
                log::warn!("guardian level: the disarm write at {a:#x} failed");
            }
        }
    }

    /// Every frame: arm a character that entered the world; follow the block's GL into the record.
    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, book: &mut crate::avatar_state::AvatarBook, io: &dyn crate::avatar_state::AvatarIo) {
        let Some(block) = self.block else { return };
        let (field, live, words) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let field = crate::guest_build::app_state().is_some_and(|st| items::app_state_word(&read) == Some(st.field()));
            (field, crate::guest_build::h2_guardian_level_char(&read, h2.char_slot), crate::guest_build::h2_guardian_level_read(&read, block))
        };
        use crate::guest_build::H2GlStep;
        match crate::guest_build::h2_guardian_level_step(self.want_arm, self.told, field, live, words, book.guardian_level()) {
            H2GlStep::Idle => {}
            H2GlStep::Arm { owner, gl, fresh, level, record } => {
                if self.arm(emulator, block, owner, gl, fresh) {
                    self.want_arm = false;
                    self.told = Some(gl);
                    book.set_guardian_level(io, gl);
                    log::info!("guardian level: armed for the character at LV {level} — GL {gl} (record {record}){}", if fresh { ", met at 99: the bar from 0 (no back-pay)" } else { "" });
                }
            }
            H2GlStep::ReArm { owner, gl, was } => {
                if self.arm(emulator, block, owner, gl, false) {
                    log::warn!("guardian level: the character object moved ({was:#x} → {owner:#x}) — re-armed at GL {gl}");
                }
            }
            H2GlStep::Follow { gl, was } => {
                self.told = Some(gl);
                book.set_guardian_level(io, gl);
                if gl > was {
                    let w = words.unwrap_or(crate::guest_build::H2GuardianWords { armed: true, owner: 0, gl: u32::from(gl), next: 0, ups: 0 });
                    log::info!("guardian level: GL {was} → {gl} (EXP next {}, {} GL-up(s) since the install)", w.next, w.ups);
                    host_notice(guardian_notice_text(u32::from(gl)));
                }
            }
        }
    }

    fn arm(&self, emulator: &mut dyn Emulator, block: u32, owner: u32, gl: u16, fresh: bool) -> bool {
        for (a, bytes) in crate::guest_build::h2_guardian_level_arm_writes(block, owner, gl, fresh) {
            if !emulator.write_memory(a, &bytes) {
                log::warn!("guardian level: the arm write at {a:#x} failed — not armed");
                let (d, off) = crate::guest_build::h2_guardian_level_disarm_write(block);
                emulator.write_memory(d, &off);
                return false;
            }
        }
        true
    }

    /// The HUD's view: a LV 99 character the block is armed for shows its GL, and the EXP bar is the GL bar
    /// (the block's NEXT — `a.f:I` is the same while the block holds it, but a recompute rewrites it for a frame).
    fn overlay(&self, read: &impl Fn(u32, usize) -> Vec<u8>, h2: &crate::guest_build::Hero2Java, s: &mut Stats) {
        let Some(block) = self.block else { return };
        let (Some((handle, level)), Some(w)) = (crate::guest_build::h2_guardian_level_char(read, h2.char_slot), crate::guest_build::h2_guardian_level_read(read, block)) else { return };
        if w.armed && w.owner == handle && level >= i32::from(crate::guest_build::GuardianLevelPatch::LEVEL) && w.gl >= 1 && w.next > 0 {
            s.guardian = u16::try_from(w.gl.min(crate::guest_build::GuardianLevelPatch::MAX_GL)).ok();
            s.exp_next = w.next;
        }
    }
}

/// A GL-up's line for [`host_notice`] (which keeps `&'static str`s): made once per GL — at most 250.
fn guardian_notice_text(gl: u32) -> &'static str {
    static TEXTS: std::sync::Mutex<Vec<(u32, &'static str)>> = std::sync::Mutex::new(Vec::new());
    let Ok(mut texts) = TEXTS.lock() else { return "가디언 레벨 업" };
    if let Some((_, t)) = texts.iter().find(|(g, _)| *g == gl) {
        return t;
    }
    let t: &'static str = Box::leak(crate::guest_build::h2_guardian_level_notice(gl).into_boxed_str());
    texts.push((gl, t));
    t
}

/// ★★ 세이브 슬롯 5 (2026-09-27, user: "세이브 슬롯을 3개에서 5개로 늘릴 수 있어?"): the
/// game's own slot screen, saver and loader over five slots. The mechanism and every site
/// are `guest_anchors::SaveSlotsPatch`; what lives here is the ORDER, one step per poll,
/// polled every frame until it is done:
///
///   1. `ag` initialized by the host (`Emulator::init_java_class`) while nothing has
///      touched it — the logo. The game's own first touch builds the title in the same
///      tick, so without this there is no poll between the tables and their first reader.
///   2. two host `String[5]`s (`Emulator::new_java_string_array`), checked name by name,
///      written over the game's two three-name tables ([`JavaWrite::SaveFileNames`], the
///      `/g` half first and put back if the `/n` half does not land).
///   3. `ag.k()V`'s halfwords — the previews over five — once the names are five.
///   4. the slot screen's halfwords, only while the previews are null or five long and no
///      slot screen is up. A title built before step 2 (a key in the first frames) keeps
///      three previews; this then waits for the next title, and says so once.
///
/// Every step re-reads the guest before it acts, so a failed step resumes where it stopped
/// (the JVM not up yet is the common one), and a table this host did not make (`Foreign`)
/// is never written over — the game keeps three slots.
struct SaveSlots {
    step: SaveSlotsStep,
    table: HalfwordPatch,
    screen: HalfwordPatch,
    /// No new async step before this frame (a failed one waits a moment).
    retry_at: u64,
    /// The five-name tables read as ours — not re-read (ten strings) on every poll after.
    names_in: bool,
    /// Async steps issued (initializations + table pairs). A step that "succeeds" without
    /// the guest changing would otherwise be reissued forever — twelve objects a try.
    attempts: u8,
    /// "the title came first" — once.
    said_waiting: bool,
}

enum SaveSlotsStep {
    /// Nothing in flight: read the guest and decide.
    Idle,
    /// `init_java_class("ag")` issued.
    Init(Arc<wie_backend::GuestCallResult>),
    /// The two `String[5]`s issued: `/g` half, `/n` half.
    Arrays(Arc<wie_backend::GuestCallResult>, Arc<wie_backend::GuestCallResult>),
    /// Five slots.
    Done,
    /// Not this build, or a table this host will not touch — three slots.
    Off,
}

impl SaveSlots {
    /// Frames between two attempts at an async step that failed.
    const RETRY_FRAMES: u64 = 10;
    /// Async steps before giving up (three slots, said once). The happy path is two.
    const MAX_ATTEMPTS: u8 = 8;

    fn new() -> Self {
        Self { step: SaveSlotsStep::Idle, table: HalfwordPatch::new(), screen: HalfwordPatch::new(), retry_at: 0, names_in: false, attempts: 0, said_waiting: false }
    }

    fn finished(&self) -> bool {
        matches!(self.step, SaveSlotsStep::Done | SaveSlotsStep::Off)
    }

    fn poll(&mut self, emulator: &mut dyn Emulator, frame: u64) {
        let (Some(p), Some(h2)) = (crate::guest_build::save_slots_patch(), crate::guest_build::hero2_java()) else {
            self.step = SaveSlotsStep::Off;
            return;
        };
        use crate::guest_build::{H2SaveNames, SaveSlotsPatch};
        match std::mem::replace(&mut self.step, SaveSlotsStep::Idle) {
            s @ (SaveSlotsStep::Done | SaveSlotsStep::Off) => self.step = s,
            SaveSlotsStep::Init(out) => match out.poll() {
                None => self.step = SaveSlotsStep::Init(out),
                Some(Ok(_)) => log::info!("save slots: {} initialized by the host, ahead of the title", p.class),
                Some(Err(())) => {
                    log::warn!("save slots: {} could not be initialized yet — retrying", p.class);
                    self.retry_at = frame + Self::RETRY_FRAMES;
                }
            },
            SaveSlotsStep::Arrays(g, n) => match (g.poll(), n.poll()) {
                (Some(g), Some(n)) => self.put_names(emulator, &p, g, n, frame),
                _ => self.step = SaveSlotsStep::Arrays(g, n),
            },
            SaveSlotsStep::Idle => {
                if frame < self.retry_at {
                    return;
                }
                if self.names_in {
                    self.patch_code(emulator, &p, &h2, frame);
                    return;
                }
                let state = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    crate::guest_build::h2_save_names_state(&read, &p)
                };
                if matches!(state, H2SaveNames::Uninit | H2SaveNames::Shipped { .. }) {
                    if self.attempts >= Self::MAX_ATTEMPTS {
                        log::warn!("save slots: {} tries and the tables still read {state:?} — gave up, THREE slots", self.attempts);
                        self.step = SaveSlotsStep::Off;
                        return;
                    }
                    self.attempts += 1;
                }
                match state {
                    H2SaveNames::Uninit => {
                        let out = Arc::new(wie_backend::GuestCallResult::new());
                        emulator.init_java_class(p.class, out.clone());
                        self.step = SaveSlotsStep::Init(out);
                    }
                    H2SaveNames::Shipped { .. } => {
                        let names = |prefix| crate::guest_build::h2_save_file_names(prefix, SaveSlotsPatch::SLOTS);
                        let (g, n) = (names(SaveSlotsPatch::PROG_PREFIX), names(SaveSlotsPatch::CHAR_PREFIX));
                        let (gout, nout) = (Arc::new(wie_backend::GuestCallResult::new()), Arc::new(wie_backend::GuestCallResult::new()));
                        emulator.new_java_string_array(&g.iter().map(String::as_str).collect::<Vec<_>>(), gout.clone());
                        emulator.new_java_string_array(&n.iter().map(String::as_str).collect::<Vec<_>>(), nout.clone());
                        self.step = SaveSlotsStep::Arrays(gout, nout);
                    }
                    H2SaveNames::Extended => {
                        self.names_in = true;
                        self.patch_code(emulator, &p, &h2, frame);
                    }
                    H2SaveNames::NotYet => {}
                    H2SaveNames::Foreign => {
                        log::warn!("save slots: ag's file-name tables do not read as the game's or ours — left alone, THREE slots");
                        self.step = SaveSlotsStep::Off;
                    }
                }
            }
        }
    }

    /// Step 2's writes, once both arrays exist. The arrays are the host's to release either
    /// way: a static holds them after the writes, nothing does if they did not land.
    fn put_names(&mut self, emulator: &mut dyn Emulator, p: &crate::guest_build::SaveSlotsPatch, g: Result<u32, ()>, n: Result<u32, ()>, frame: u64) {
        use crate::guest_build::{H2SaveNames, JavaWrite, SaveSlotsPatch};
        self.retry_at = frame + Self::RETRY_FRAMES;
        let (Ok(g), Ok(n)) = (g, n) else {
            for h in [g, n].into_iter().flatten() {
                emulator.release_java_object(h);
            }
            log::warn!("save slots: the five-name tables could not be made — retrying");
            return;
        };
        let (state, fresh) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let fresh = crate::guest_build::h2_save_names_are(&read, g, SaveSlotsPatch::PROG_PREFIX, SaveSlotsPatch::SLOTS)
                && crate::guest_build::h2_save_names_are(&read, n, SaveSlotsPatch::CHAR_PREFIX, SaveSlotsPatch::SLOTS);
            (crate::guest_build::h2_save_names_state(&read, p), fresh)
        };
        match state {
            H2SaveNames::Shipped { prog: old_g, .. } if fresh => {
                let [(ga, gb), (na, nb)] = crate::guest_build::h2_save_names_writes(p, g, n);
                if !java_write(emulator, JavaWrite::SaveFileNames, ga, &gb) {
                    log::warn!("save slots: the /g table write failed — retrying");
                } else if !java_write(emulator, JavaWrite::SaveFileNames, na, &nb) {
                    let back = java_write(emulator, JavaWrite::SaveFileNames, ga, &old_g.to_le_bytes());
                    log::warn!("save slots: the /n table write failed — the /g table {} — retrying", if back { "is back as shipped" } else { "COULD NOT BE PUT BACK" });
                } else {
                    log::info!("save slots: ag's file names are /g1../g{0} and /n1../n{0} ({g:#x}, {n:#x})", SaveSlotsPatch::SLOTS);
                    self.retry_at = 0;
                }
            }
            H2SaveNames::Shipped { .. } => log::warn!("save slots: a made table does not read as its five names — retrying"),
            other => log::info!("save slots: the tables read {other:?} by the time the arrays came — not written"),
        }
        emulator.release_java_object(g);
        emulator.release_java_object(n);
    }

    /// Steps 3 and 4.
    fn patch_code(&mut self, emulator: &mut dyn Emulator, p: &crate::guest_build::SaveSlotsPatch, h2: &crate::guest_build::Hero2Java, frame: u64) {
        self.table.apply(emulator, "save slots", "the title's slot previews run over five", p.table_sites);
        if !self.table.done {
            if self.table.said {
                self.step = SaveSlotsStep::Off;
            }
            return;
        }
        let ready = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_save_screen_ready(&read, h2, p)
        };
        if !ready {
            if !self.said_waiting {
                self.said_waiting = true;
                log::warn!("save slots: the title was up before the tables were extended — the slot screen goes to five at the next title");
            }
            // The wait can last the session (a player who never goes back to the title):
            // a look every ten frames, not every frame.
            self.retry_at = frame + Self::RETRY_FRAMES;
            return;
        }
        self.screen.apply(emulator, "save slots", "the slot screen cycles five slots", p.screen_sites);
        if self.screen.done {
            self.step = SaveSlotsStep::Done;
        } else if self.screen.said {
            self.step = SaveSlotsStep::Off;
        }
    }
}

fn java_write(emulator: &mut dyn Emulator, what: crate::guest_build::JavaWrite, addr: u32, bytes: &[u8]) -> bool {
    if crate::guest_build::hero2_java().is_some() {
        if !crate::guest_build::java_write_ok(what) {
            log::warn!("java write: {what:?} at {addr:#x} REFUSED by the {} gate", crate::guest_build::name());
            return false;
        }
        let ok = emulator.write_memory(addr, bytes);
        log::debug!("java write: {what:?} {addr:#x} <- {bytes:02x?} ({})", if ok { "ok" } else { "FAILED" });
        return ok;
    }
    emulator.write_memory(addr, bytes)
}

/// The receiver of a [`java_call`]: a singleton the JVM resolves by name, an object handle
/// the host has just read (the class it must be comes from the [`JavaCall`]'s own spec, so
/// a caller cannot name one class and call another's method on it), or none for a
/// `static` method.
#[derive(Clone, Copy, Debug)]
pub enum JavaThis<'a> {
    Static { class: &'a str, field: &'a str, descriptor: &'a str },
    Handle(u32),
    None,
}

/// ★★ THE ONE GATE for hero2's JVM calls (2026-09-21) — the call-side twin of
/// [`java_write`]. Every host-initiated Java call names a [`JavaCall`], asks the build
/// gate, and is spelled from that call's spec; the `java call:` log line is the one place
/// to look for what the host asked the guest to do. `None` = refused (gate closed, or the
/// receiver shape does not fit the method), and nothing was issued.
///
/// The result slot completes on a later tick — `Some(Ok(raw))` is the method's return
/// word (a primitive's value, an object's handle, `0` for `void`), `Some(Err(()))` means
/// the backend refused it (a dead handle, a receiver of the wrong class, a Java exception)
/// and NOTHING of the call happened in the guest.
fn java_call(emulator: &mut dyn Emulator, what: crate::guest_build::JavaCall, this: JavaThis<'_>, args: &[wie_backend::JavaArg]) -> Option<Arc<wie_backend::GuestCallResult>> {
    if !crate::guest_build::java_call_ok(what) {
        log::warn!("java call: {what:?} REFUSED by the {} gate", crate::guest_build::name());
        return None;
    }
    let spec = what.spec();
    let out = Arc::new(wie_backend::GuestCallResult::new());
    use crate::guest_build::CallKind;
    let receiver = match (spec.kind, this, spec.singleton) {
        (CallKind::Static, JavaThis::None, _) => wie_backend::JavaReceiver::Class(spec.class),
        (CallKind::Virtual, JavaThis::Static { class, field, descriptor }, _) => wie_backend::JavaReceiver::Static { class, field, descriptor },
        (CallKind::Virtual, JavaThis::None, Some((class, field, descriptor))) => wie_backend::JavaReceiver::Static { class, field, descriptor },
        (CallKind::Virtual, JavaThis::Handle(handle), _) => wie_backend::JavaReceiver::Handle { handle, class: spec.class },
        (CallKind::Constructor, JavaThis::None, _) => {
            emulator.new_java_object(spec.class, spec.descriptor, args, out.clone());
            log::debug!("java call: {what:?} new {}{} with {args:?}", spec.class, spec.descriptor);
            return Some(out);
        }
        (kind, this, _) => {
            log::warn!("java call: {what:?} is {kind:?} but was given {this:?} — refused");
            return None;
        }
    };
    emulator.call_java_method(receiver, spec.method, spec.descriptor, args, out.clone());
    log::debug!("java call: {what:?} {}.{}{} on {receiver:?} with {args:?}", spec.class, spec.method, spec.descriptor);
    Some(out)
}

/// ★★ #13 steps 5–7 (2026-09-21): the ACCOUNT warehouse — two bags the host owns, shown
/// through the game's own inventory window.
///
/// The menu's row 6 (ACCOUNT, formerly CARD) is intercepted: instead of `cg`, the host
/// builds `new q(menu, warehouse, false)` and hangs it on the menu exactly as `bw.b()Z`
/// does for the game's own windows (`hero2java::h2_account_open_writes`). The warehouse
/// mirrors the inventory (user, 2026-09-21: "계정창고에도 재료가방이 있어야겠지"): a MAIN
/// bag whose cell 0 is a 재료가방 record, and a MATERIAL bag behind it — OK on that record
/// opens `new q(warehouseWindow, materials, false)` the way the game opens `ag.b` from the
/// inventory. Both are `new bq(127)`, made once and pinned for the life of the process
/// (they are not the character's — every save slot shares them); their contents are the
/// host's two files (`vault::{read,write}_account`), written when the game saves.
///
/// Moves (`PopupRow::Move`) pair the bags: inventory ↔ warehouse main, 재료가방 (`ag.b`)
/// ↔ warehouse materials — with one refinement on the way back: a material TYPE
/// (`bq.a(B)Z`: 23 · 30 · 31 · 32) goes to `ag.b` whichever warehouse bag it sits in, so
/// the game's own routing is never contradicted.
///
/// Everything asynchronous here is a queued JVM call polled from the worker loop, the
/// same shape as the save: issue, then `poll` until the slot completes.
struct AccountState {
    /// The two warehouse bags, pinned, once constructed: `[main, materials]`.
    bags: [Option<u32>; 2],
    /// The JVM call in flight.
    pending: Option<AccountStep>,
    /// The gm-table title swap in force `(slot address, original handle)`, while the
    /// warehouse MAIN window is the top `q` — put back when it is not.
    title: Option<(u32, u32)>,
    /// The key-up owed for an OK whose key-down opened a window.
    swallow_ok_up: bool,
    /// ★ Step 6: both blobs on disk have been read into the bags (or there were none).
    /// Windows open only after this — an empty bag shown before the load would be a lie
    /// the player could act on.
    loaded: bool,
    /// A blob on disk could not be deserialised: that bag stays empty and is NEVER
    /// written back this run, so a file the host does not understand is not replaced by
    /// an empty one.
    load_failed: [bool; 2],
    /// Something moved since the last commit.
    dirty: bool,
    /// `vault::save_edges()` as of the last poll — a change is the game saving.
    edges_seen: u64,
    /// ⛔ The commit each bag OWES, per bag (2026-09-21, review). A save edge used to be
    /// consumed by advancing `edges_seen` and calling the commit straight away — but the
    /// commit declines on four guards (no bag, load failed, not loaded, a JVM call in
    /// flight), and `ag.l()` produces exactly two edges, so one `AccountStep` spanning
    /// both of them dropped the whole save's commit with nothing left to retry it. The
    /// player had already taken the item OUT of `ag.a`, so the `n<slot>` written by that
    /// same save did not have it either: the item existed in no file at all. The
    /// obligation now outlives the edge and `pump_commit` discharges it when it can.
    ///
    /// ⛔ Per bag, not one flag, because the MATERIALS bag used to be reachable only
    /// through the MAIN bag's guards — a truncated `account/bag` at boot latched
    /// `load_failed[Main]`, and from then on every move into the warehouse's 재료가방 was
    /// lost on exit, for the whole run, silently.
    commit_owed: [bool; 2],
    /// A blob of the round now being discharged did not reach the disk, so the move it
    /// carried is still only in guest memory — `dirty` must survive to the next save.
    commit_failed: bool,
    /// ⛔ `new bq(..)` failed: there is no warehouse this run and none will be tried for
    /// (2026-09-21, review). Without this latch the failure arm only logged, so the
    /// five-frame poll re-entered `ensure_bags` and issued the construction again — about
    /// twelve spawned JVM tasks and three dozen `format!`ed logcat lines per second off
    /// the emulator thread, for the rest of the session, each one churning the very heap
    /// that had just refused. `BagPaging` already latched its own refusal; this is the
    /// same latch for the same reason.
    construction_refused: bool,
    /// The row pressed while the bags were still being made / loaded.
    open_after_load: Option<AccountOpen>,
    /// ★ The game's own 개수 box, while it is up over a stack the player is splitting.
    qty: Option<QtyMove>,
    /// ★ 2026-10-02 (user: "네트워크 창고에서 아이템을 꺼낸뒤에 저장을 안하면 해당 아이템이
    /// 증발되어버리는데"): the game went back to its SHELL, so the warehouse owes a return to
    /// the last save. These bags are host objects and outlive the game's own: a load builds
    /// a new `ag.a` from `n<slot>`, while the warehouse kept every move made since the last
    /// save — so an item taken OUT and not saved was, after a death and a reload, in
    /// neither bag (and the next save wrote the warehouse without it: gone for good), and
    /// an item put IN was in both. Every load goes through the shell (`ag.a(Z)V` ←
    /// `ag.r()V` ← the slot screen's event, and `bv` ← `be`), so that edge is where the
    /// unsaved moves end. Set by [`Self::leave_world`], discharged by [`Self::pump_reload`];
    /// while it stands the warehouse is not [`Self::ready`] — no window, no move.
    reload_owed: bool,
    /// ★ 2026-10-13: the load's fold rounds so far ([`Self::fold_next`]).
    fold_rounds: u32,
}

/// ★ A partial move waiting on `aq` (2026-09-21, user: "계정창고로 넣을때 갯수를 지정해서
/// 넣을 수 있었으면"). Only what IDENTIFIES the three things involved: everything the move
/// needs — the number, the record's count, the destination — is re-read at the OK, because
/// the box is up for as long as the player wants and the bag underneath is live all the
/// while (a drop, a 0-key use, a stack hitting zero all move records about).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct QtyMove {
    /// The `aq` instance the host hung on the window, in place of the record's popup.
    popup: u32,
    /// The bag window it belongs to, and the bag that window shows.
    window: u32,
    bag: u32,
    /// The record it is asking about — found again by HANDLE, never by index.
    record: u32,
}

/// Which warehouse bag.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Which {
    Main = 0,
    Materials = 1,
}

impl Which {
    const BOTH: [Which; 2] = [Which::Main, Which::Materials];

    fn file(self) -> &'static str {
        match self {
            Which::Main => crate::vault::ACCOUNT_MAIN,
            Which::Materials => crate::vault::ACCOUNT_MATERIALS,
        }
    }
}

/// What the player asked to open: the warehouse from the menu, or its 재료가방 from the
/// warehouse window (`parent` = the window the new one hangs on).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AccountOpen {
    Main { menu: crate::guest_build::H2Win },
    Materials { parent: crate::guest_build::H2Win },
}

/// ★★ 2026-10-21 (user: *"비행서 사용한 직후에 메뉴버튼 누르면 메뉴가 안열리고있어"*): close the field menu the way the
/// game's own close event 7 does (`ag.g()V` 0x11178a) — the state word ← 3 FIRST (in state 5 the paint and the key handler
/// build a fresh menu the moment the singleton reads null), then the release `bw.a(Z)V(1)` ([`JavaCall::MenuRelease`]:
/// images · chain · cursor · the singleton ← null, the full repaint). The 비행서 and the 상점주인의 혼 used to write the menu's
/// child ← 0 and the state ← 3 only: the next open reused that menu, whose images were loaded and which nothing marked to be
/// drawn — the state read 5 over an unchanged field (probe: CLR after a 비행서 warp → state 5, no menu on screen).
/// `None` = the call was refused: the old exit (the child ← 0) so no window is left hanging, and the caller goes on.
///
/// [`JavaCall::MenuRelease`]: crate::guest_build::JavaCall::MenuRelease
fn release_field_menu(emulator: &mut dyn Emulator, menu_fields: u32) -> Option<Arc<wie_backend::GuestCallResult>> {
    use crate::guest_build::{JavaCall, JavaWrite};
    let state = java_write(emulator, JavaWrite::FlightState, 0x0019_3aec, &3u32.to_le_bytes());
    match java_call(emulator, JavaCall::MenuRelease, JavaThis::None, &[wie_backend::JavaArg::Int(1)]) {
        Some(out) => Some(out),
        None => {
            log::warn!("menu: the release bw.a(Z)V was refused (state ← 3 ok={state})");
            menu_release_failed(emulator, menu_fields);
            None
        }
    }
}

/// The release could not run: at least take the window chain off the menu (the pre-2026-10-21 exit).
fn menu_release_failed(emulator: &mut dyn Emulator, menu_fields: u32) {
    use crate::guest_build::{Hero2Java, JavaWrite};
    let ok = java_write(emulator, JavaWrite::WindowChild, menu_fields + 4 + Hero2Java::WIN_CHILD_OFF, &0u32.to_le_bytes());
    log::warn!("menu: release failed — the menu's child cleared instead (ok={ok}); the next open may reuse it undrawn");
}

enum AccountStep {
    /// ★ 비행서: `bq.a(BI)V` on the bag the popup's window shows (`bag` — the inventory or a WAREHOUSE bag), then the warp.
    FlightConsume { out: Arc<wie_backend::GuestCallResult>, town: usize, menu_fields: u32, bag: u32 },
    /// ★★ 2026-10-21: the field menu being released (`bw.a(Z)V`, [`release_field_menu`]) before the warp.
    FlightMenu { out: Arc<wie_backend::GuestCallResult>, town: usize, menu_fields: u32 },
    FlightWarp { out: Arc<wie_backend::GuestCallResult> },
    /// `new bq(127)` for `which`.
    NewBag { out: Arc<wie_backend::GuestCallResult>, which: Which },
    /// The blob on disk as a guest `byte[]`, for `which`.
    LoadArray { out: Arc<wie_backend::GuestCallResult>, which: Which },
    /// `bag.a(byte[])` — the game's own deserialiser; `arr` is released after it.
    Load { out: Arc<wie_backend::GuestCallResult>, which: Which, arr: u32 },
    /// The 재료가방 record for the main bag's cell 0 (`ch.a(29, 0, …)`), then its insert.
    MaterialsItemNew { out: Arc<wie_backend::GuestCallResult> },
    MaterialsItemInsert { out: Arc<wie_backend::GuestCallResult>, rec: u32 },
    /// `new q(parent, bag, false)` for the window `open` names.
    NewWindow { out: Arc<wie_backend::GuestCallResult>, open: AccountOpen },
    /// `bag.a()` → the blob to write, for `which`.
    Serialize { out: Arc<wie_backend::GuestCallResult>, which: Which },
    /// ★ Step 7: a fresh label array for a popup whose own has no slack (`TableRows2/3`).
    MoveRowArray { out: Arc<wie_backend::GuestCallResult>, popup: u32, len: u32 },
    /// ★ Step 7: the move — insert into the destination, then remove from the source.
    MoveInsert { out: Arc<wie_backend::GuestCallResult>, src: u32, index: u32, rec: u32, to_account: bool },
    MoveRemove { out: Arc<wie_backend::GuestCallResult>, src: u32, rec: u32, to_account: bool },
    /// ★ The 개수 box for a stack: `new aq(window, 0, count)`, hung where the popup was.
    QtyNew { out: Arc<wie_backend::GuestCallResult>, window: crate::guest_build::H2InvWin, record: u32 },
    /// ★ A PARTIAL move, the game's own three calls (HERO2_GUEST_RE.md §2.7.8): a fresh
    /// record of the same `(type, id)`, merged into the destination for as many as it
    /// will take, and exactly that many consumed from the source.
    /// ★ 2026-10-13: also every whole move of a STACK (`h2_move_merges`), and the load's fold of a bag into itself.
    SplitNew { out: Arc<wie_backend::GuestCallResult>, src: u32, rec: u32, dst: u32, n: i32, goal: SplitGoal },
    SplitAdd { out: Arc<wie_backend::GuestCallResult>, src: u32, rec: u32, fresh: u32, n: i32, goal: SplitGoal },
    SplitConsume { out: Arc<wie_backend::GuestCallResult>, src: u32, rec: u32, placed: i32, goal: SplitGoal },
    /// ★ #22: the 망각의 구 that paid for a reset, being consumed. The reset itself is
    /// already written when this is in flight — this step only eats the item, and a
    /// failure here leaves the player with a free orb, which is the right way round.
    /// `bag` is the one the orb came from (its window gets the notice), `note` the gm row
    /// the notice says.
    ResetConsume { out: Arc<wie_backend::GuestCallResult>, refund: i32, bag: u32, note: u32 },
    /// ★★ 2026-10-17: an elixir drunk from a PLATE, being consumed — the refill is already written; no popup to close,
    /// nothing to say (a potion's silence).
    ElixirConsume { out: Arc<wie_backend::GuestCallResult> },
    /// ★★ 2026-10-20 상점주인의 혼: the game's own stock loader `z.a()` in flight — its `Vector[]` goes into the stock
    /// static, then the shop opens ([`AccountState::open_shop`]). `menu_fields` = the field menu to close first — ★★ 2026-10-21
    /// `None` from a PLATE, pressed on the field with no menu up.
    ShopStock { out: Arc<wie_backend::GuestCallResult>, menu_fields: Option<u32> },
    /// ★★ 2026-10-21: the field menu being released before the shop event ([`release_field_menu`]).
    ShopMenu { out: Arc<wie_backend::GuestCallResult>, menu_fields: u32 },
    /// ★★ 2026-10-20 상점주인의 혼: the shop event `(4, 0)` posted — the game runs `ag.h(7)` · `v.a()Lv;` · `v.a()V` on
    /// its next tick. Nothing to do when it lands; held so no other call is issued meanwhile.
    ShopEvent { out: Arc<wie_backend::GuestCallResult> },
    /// ★ 2026-09-26: a notice in the game's own message box — `new ba(window, text)` — to
    /// be hung on `window` in place of `replaces` (see [`AccountState::notice`]).
    NoticeNew { out: Arc<wie_backend::GuestCallResult>, window: u32, replaces: Option<u32>, text: NoticeText },
}

/// ★ 2026-09-26 (user: "안내팝업은 항상 기존 게임 팝업으로 만들어줘. 내가 직접그린
/// 안내토스트문구가 뜨는게아니라"): what a notice box says. Only the game's own words: a row of
/// the menu's `/t/gm` table (every notice here is given with the menu open, so the table is
/// loaded — `Hero2Java::GM_ROW_NOTE_*` are the rows the asset tool appends), or the game's
/// own 「빈공간이 없습니다.」 (`/t/itmtp`[7], the bag-full notice `ag.a()Z` raises itself).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NoticeText {
    Gm(u32),
    BagFull,
}

/// ★ 2026-10-13: what a split's three calls are for — a MOVE between a game bag and the warehouse (the player's,
/// told in the game's box when done), or the warehouse load's FOLD of one bag into itself (silent; whatever way it
/// ends, the load goes on — see [`AccountState::fold_next`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SplitGoal {
    Move { to_account: bool },
    Fold,
}

/// ★ 2026-09-26: what [`AccountState::start_move`] found, once the popup has been re-read.
enum MoveStart {
    /// Refused — say why in the game's box, over the popup the player chose it from.
    Refuse { p: crate::guest_build::H2BagPopup, text: NoticeText },
    /// A stack: the game's own 개수 box first.
    Split(crate::guest_build::H2BagPopup),
    /// ★ 2026-10-13: a stack of ONE — all of it, through the game's merge (`h2_move_merges`), so it joins a stack
    /// of its kind in the destination instead of taking a cell of its own.
    Merge { p: crate::guest_build::H2BagPopup, dst: u32, to_account: bool },
    /// The whole record, as the same object. `clear` is the stale worn flag of a record
    /// coming OUT of the warehouse, taken off before it goes (see `JavaWrite::RecordEquipSlot`).
    Whole { p: crate::guest_build::H2BagPopup, dst: u32, to_account: bool, clear: Option<(u32, [u8; 4])> },
}

impl AccountState {
    fn new() -> Self {
        Self {
            bags: [None, None],
            pending: None,
            title: None,
            swallow_ok_up: false,
            loaded: false,
            load_failed: [false, false],
            dirty: false,
            edges_seen: crate::vault::save_edges(),
            commit_owed: [false, false],
            commit_failed: false,
            construction_refused: false,
            open_after_load: None,
            qty: None,
            reload_owed: false,
            fold_rounds: 0,
        }
    }

    /// A bound on the load's fold — each round fills an earlier stack by at least one, so a real warehouse ends
    /// far sooner (127 cells); this only keeps a guest that answers strangely from spinning the load.
    const FOLD_ROUNDS_MAX: u32 = 256;

    fn bag(&self, which: Which) -> Option<u32> {
        self.bags[which as usize]
    }

    /// Both warehouse bags exist, hold what the disk holds, and are not waiting to be
    /// handed back to the last save.
    fn ready(&self) -> bool {
        self.bags.iter().all(Option::is_some) && self.loaded && !self.reload_owed
    }

    /// The bags a popup may sit over, in the order the popup readers want them: the
    /// inventory, the game's 재료가방, the warehouse main, the warehouse materials.
    fn popup_bags(&self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> Vec<u32> {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let mut bags = Vec::with_capacity(4);
        bags.extend(crate::guest_build::h2_static_handle(&read, h2.inventory_slot));
        bags.extend(crate::guest_build::h2_static_handle(&read, h2.material_bag_slot));
        bags.extend(self.bags.iter().flatten());
        bags
    }

    /// The GAME's two bags as named slots — `[ag.a inventory, ag.b 재료가방]`, either
    /// possibly absent. Unlike [`Self::popup_bags`] the positions carry meaning here.
    fn game_bags(&self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> [Option<u32>; 2] {
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        [
            crate::guest_build::h2_static_handle(&read, h2.inventory_slot),
            crate::guest_build::h2_static_handle(&read, h2.material_bag_slot),
        ]
    }

    /// Make the next missing bag if nothing is in flight. Called from the in-game poll;
    /// the JVM resolves `bq` by name, so any in-world frame will do. Once both exist the
    /// load runs, then the 재료가방 record is made sure of.
    fn ensure_bags(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path) {
        // ⛔ `reload_owed`: bags that are about to be let go are neither made nor LOADED —
        //   with both in hand this would run the deserialiser over full bags a second time.
        if self.pending.is_some() || self.ready() || self.construction_refused || self.reload_owed {
            return;
        }
        let Some(which) = Which::BOTH.into_iter().find(|w| self.bag(*w).is_none()) else {
            // Both made: load what is on disk, in order.
            self.start_load(emulator, data_dir, Which::Main);
            return;
        };
        let cap = crate::guest_build::Hero2Java::BAG_CAPACITY as i32;
        match java_call(emulator, crate::guest_build::JavaCall::BagNew, JavaThis::None, &[wie_backend::JavaArg::Int(cap)]) {
            Some(out) => {
                log::info!("account: constructing the warehouse {which:?} bag (bq({cap}))");
                self.pending = Some(AccountStep::NewBag { out, which });
            }
            None => self.refuse_construction("the constructor call was refused"),
        }
    }

    /// No warehouse this run. Latched so nothing retries, and the menu row handed back to
    /// the game so the player gets the game's own answer instead of a key that vanishes.
    fn refuse_construction(&mut self, why: &str) {
        if !self.construction_refused {
            log::warn!("account: no warehouse this run — {why}");
        }
        self.construction_refused = true;
        self.open_after_load = None;
    }

    /// The load of `which`: the blob on disk becomes a guest `byte[]`, then the bag's
    /// own deserialiser reads it. No blob = nothing to load; the materials follow the
    /// main, and the main's cell 0 is checked last.
    fn start_load(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, which: Which) {
        match crate::vault::read_account(data_dir, which.file()) {
            Some(bytes) if !bytes.is_empty() => {
                let out = Arc::new(wie_backend::GuestCallResult::new());
                emulator.new_java_byte_array(&bytes, out.clone());
                log::info!("account: loading the warehouse {which:?} blob ({} bytes)", bytes.len());
                self.pending = Some(AccountStep::LoadArray { out, which });
            }
            _ => {
                log::info!("account: no warehouse {which:?} blob on disk — starting empty");
                self.after_load(emulator, data_dir, which);
            }
        }
    }

    /// What follows the load of `which`.
    fn after_load(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path, which: Which) {
        match which {
            Which::Main => self.start_load(emulator, data_dir, Which::Materials),
            Which::Materials => {
                self.clear_worn_flags(emulator);
                // ★ 2026-10-13: stacks the old object move left apart are folded first (it ends in
                // `ensure_materials_item`, as this did).
                self.fold_rounds = 0;
                self.fold_next(emulator)
            }
        }
    }

    /// ★ 2026-09-26: nothing in the warehouse is worn. Before 2026-09-26 a WORN record could
    /// be moved in, and its flag (`aa.f`, serialised with the record) came with it: the game
    /// then refuses to drop it from the warehouse (gm[55]), and carried back into `ag.a` it
    /// would be put on at the next load (`a.n()V` rebuilds the character's seven from these
    /// flags) over whatever is worn in that slot by then. Taken off here, once per run, before
    /// any window shows the bags; the fixed blob is written with the next save.
    fn clear_worn_flags(&mut self, emulator: &mut dyn Emulator) {
        for which in Which::BOTH {
            let Some(bag) = self.bag(which) else { continue };
            let writes = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_bag_worn_flag_clears(&read, bag)
            };
            let Some(writes) = writes.filter(|w| !w.is_empty()) else { continue };
            let mut done = 0usize;
            for (a, bytes) in &writes {
                done += usize::from(java_write(emulator, crate::guest_build::JavaWrite::RecordEquipSlot, *a, bytes));
            }
            if done > 0 {
                self.dirty = true;
            }
            log::info!(
                "account: {done}/{} worn flag(s) taken off in the warehouse {which:?} — records moved in while equipped (before 2026-09-26)",
                writes.len()
            );
        }
    }

    /// The main bag's cell 0 must be a 재료가방 record (type 29) — what the player opens
    /// the materials behind. Made by the game's own factory and inserted once; a blob
    /// that already has one keeps it.
    ///
    /// 2026-09-24 (user: "계정창고에 맨첫번째 칸을 재료가방으로 고정해야함. 재료가방이
    /// 비어있어도 계정창고 내에 재료가방은 default로 존재해야함"): "has one" is not enough
    /// — it has to be in CELL 0. `bq.a(Lch;I)Z` appends, so a warehouse blob from before
    /// the 재료가방 lived here (2026-09-21 evening, S9) got the record BEHIND its items,
    /// and with 28+ items behind it was on page 2 — "not there" to the player. The record
    /// is now found by the game's index and, when it is anywhere but first, the Vector's
    /// slots are rotated ([`Self::front_materials_item`]) before any window shows the bag.
    fn ensure_materials_item(&mut self, emulator: &mut dyn Emulator) {
        let Some(main) = self.bag(Which::Main) else { return };
        let at = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_index_of_kind(&read, main, crate::guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG)
        };
        match at {
            Some(0) => {
                self.loaded = true;
                log::info!("account: warehouse ready");
                return;
            }
            Some(k) => {
                self.front_materials_item(emulator, k);
                self.loaded = true;
                log::info!("account: warehouse ready");
                return;
            }
            None => {}
        }
        let t = crate::guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG as i32;
        let args = [wie_backend::JavaArg::Int(t), wie_backend::JavaArg::Int(0), wie_backend::JavaArg::Int(1), wie_backend::JavaArg::Int(0)];
        match java_call(emulator, crate::guest_build::JavaCall::RecordNew, JavaThis::None, &args) {
            Some(out) => self.pending = Some(AccountStep::MaterialsItemNew { out }),
            None => self.loaded = true,
        }
    }

    /// The 재료가방 record sits at `index` (> 0) of the main bag: rotate the Vector's
    /// slots so it is cell 0 (`hero2java::h2_bag_front_writes`, gate
    /// [`JavaWrite::BagOrder`]). Only ever called while no window shows the bag — a
    /// window's cursor is an index into this very order.
    fn front_materials_item(&mut self, emulator: &mut dyn Emulator, index: u32) {
        let Some(main) = self.bag(Which::Main) else { return };
        let writes = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_front_writes(&read, main, index)
        };
        match writes {
            Some(w) if !w.is_empty() => {
                let mut ok = true;
                for (a, bytes) in &w {
                    ok &= java_write(emulator, crate::guest_build::JavaWrite::BagOrder, *a, bytes);
                }
                if ok {
                    self.dirty = true;
                    log::info!("account: 재료가방 record moved from cell {index} to cell 0 ({} slot write(s))", w.len());
                } else {
                    log::error!("account: moving the 재료가방 record from cell {index} to cell 0 did not complete ({} slot write(s))", w.len());
                }
            }
            Some(_) => {}
            None => log::warn!("account: the 재료가방 record at cell {index} could not be moved to cell 0 — bag unreadable"),
        }
    }

    /// Cell 0 of the main bag is the 재료가방, checked again right before a window opens
    /// over it (the load-time pass may have run on a blob that a later save reordered,
    /// or been skipped because the record could not be made then).
    fn front_materials_item_if_needed(&mut self, emulator: &mut dyn Emulator) {
        let Some(main) = self.bag(Which::Main) else { return };
        let at = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_index_of_kind(&read, main, crate::guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG)
        };
        if let Some(k) = at.filter(|k| *k > 0) {
            self.front_materials_item(emulator, k);
        }
    }

    /// The game just saved: the warehouse goes with it. Records the obligation; the
    /// writing itself is [`Self::pump_commit`]'s, because the moment a save edge arrives
    /// is exactly the moment a JVM call may be in flight.
    fn owe_commit(&mut self) {
        self.commit_owed = [true, true];
        self.commit_failed = false;
    }

    /// ★ 2026-10-28 (세이브내보내기, which saves first and then reads the disk): has every save edge so
    /// far reached the disk as warehouse blobs? `Some(false)` = not yet; `None` = a blob failed to
    /// serialise — the disk holds the warehouse of BEFORE the save beside the bags of after it, and
    /// an export of that pair would duplicate or lose what moved.
    fn commit_settled(&self) -> Option<bool> {
        // A save edge not seen yet owes a commit that clears a failure from before (`owe_commit`):
        // that failure is not this save's to answer for yet.
        if self.edges_seen != crate::vault::save_edges() {
            return Some(false);
        }
        if self.commit_failed {
            return None;
        }
        // A warehouse that was never loaded this run was never moved in: the blobs on the disk are
        // the last save's, which is what the bags beside them are too.
        if !self.loaded {
            return Some(true);
        }
        Some(self.edges_seen == crate::vault::save_edges() && self.commit_owed == [false, false] && !matches!(self.pending, Some(AccountStep::Serialize { .. })))
    }

    /// Discharge whatever commits are owed, one JVM call at a time.
    ///
    /// Each bag is considered on its own: a bag that cannot be written (no handle, its
    /// blob failed to load) clears its own flag and does not stand in the other's way.
    /// A bag whose turn cannot be taken yet (`pending`) keeps its flag and is tried on
    /// the next poll — that is the whole point of the flags.
    fn pump_commit(&mut self, emulator: &mut dyn Emulator, data_dir: &std::path::Path) {
        if self.pending.is_some() || !self.loaded || self.commit_owed == [false, false] {
            return;
        }
        for which in Which::BOTH {
            let i = which as usize;
            if !self.commit_owed[i] {
                continue;
            }
            // Nothing this run can write this bag: settle the debt so the other bag,
            // and `dirty`, are not held hostage by it.
            let Some(bag) = self.bag(which).filter(|_| !self.load_failed[i]) else {
                self.commit_owed[i] = false;
                continue;
            };
            // Nothing moved and the blob is already on disk: rewriting it would only
            // spend a serialise to produce the same bytes.
            if !self.dirty && crate::vault::account_path(data_dir, which.file()).is_some_and(|p| p.exists()) {
                self.commit_owed[i] = false;
                continue;
            }
            match java_call(emulator, crate::guest_build::JavaCall::BagSerialize, JavaThis::Handle(bag), &[]) {
                Some(out) => {
                    self.commit_owed[i] = false;
                    self.pending = Some(AccountStep::Serialize { out, which });
                }
                // Not transient — the gate and the receiver shape are compile-time
                // facts — so retrying would be the infinite loop, not the fix.
                None => {
                    self.commit_owed[i] = false;
                    log::error!("account: the warehouse {which:?} could not be serialised — the call was refused");
                }
            }
            return;
        }
    }

    /// ★ 2026-10-02: the game is back at its shell — what it held unsaved is gone, and the
    /// only way on is a load or a new game. The warehouse goes back with it. Nothing to do
    /// when no bag was ever made: the first in-world poll reads the disk anyway.
    fn leave_world(&mut self) {
        // A press that was waiting for the warehouse belonged to the world that just ended.
        self.open_after_load = None;
        if self.bags.iter().any(Option::is_some) {
            self.reload_owed = true;
        }
    }

    /// Discharge [`Self::reload_owed`]: let both bags go, so the next in-world poll
    /// ([`Self::ensure_bags`]) makes them again from the blobs on disk — and those are the
    /// warehouse of the LAST SAVE, because a save edge is the only thing that writes them.
    /// Deliberately the boot's own path rather than a "clear" of the host's invention.
    fn pump_reload(&mut self, emulator: &mut dyn Emulator) {
        if !self.reload_owed {
            return;
        }
        // Only a loaded warehouse can pay a commit; one still being made holds nothing the
        // disk does not, and must not wait on a debt it cannot discharge.
        let commit_payable = self.loaded && self.commit_owed != [false, false];
        match account_reload(self.pending.is_some(), commit_payable, self.title.is_some(), self.commit_failed) {
            AccountReload::Wait => {}
            AccountReload::Keep => {
                self.reload_owed = false;
                log::warn!("account: back at the title, but the last save's warehouse never reached the disk — keeping the bags in memory (they are the only copy)");
            }
            AccountReload::Drop => {
                self.reload_owed = false;
                let moved = self.dirty;
                for slot in &mut self.bags {
                    if let Some(bag) = slot.take() {
                        emulator.release_java_object(bag);
                    }
                }
                crate::ACCOUNT_BAG.store(0, std::sync::atomic::Ordering::Relaxed);
                crate::ACCOUNT_MATERIALS.store(0, std::sync::atomic::Ordering::Relaxed);
                self.loaded = false;
                self.load_failed = [false, false];
                self.dirty = false;
                self.qty = None;
                log::info!(
                    "account: back at the title — the warehouse returns to the last save ({}); it is read from disk again on the next world entry",
                    if moved { "moves made since then are dropped with the rest of the unsaved game" } else { "nothing had moved since" }
                );
            }
        }
    }

    /// An OK is about to go to the guest. Ours if the menu is alone on the ACCOUNT row
    /// (open the warehouse), or if the warehouse MAIN window is on top with its cursor on
    /// the 재료가방 record (open the materials). `true` = taken, the key is swallowed.
    fn intercept_ok(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> bool {
        // ★ The most specific state first: the 개수 box the host itself hung.
        if self.take_quantity_ok(emulator, h2) {
            return true;
        }
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let open = if let Some(menu) = crate::guest_build::h2_menu_on_account_row(&read, h2) {
            AccountOpen::Main { menu }
        } else if let Some(main) = self.bag(Which::Main)
            && let Some(v) = crate::guest_build::h2_bag_window(&read, h2, &[main]).filter(|v| v.on_top)
            && crate::guest_build::h2_cursor_record(&read, &v, crate::bag_page_ctx()).is_some_and(|r| r.kind == crate::guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG)
        {
            AccountOpen::Materials { parent: v.win }
        } else {
            return false;
        };
        if self.pending.is_some() && self.ready() {
            log::info!("account: OK on {open:?} while a call is in flight — ignored");
            return true;
        }
        // ⛔ Never promise an opening that can no longer happen (2026-09-21, review).
        // The row swallowed OK unconditionally, so once construction had failed the
        // ACCOUNT row was a key that did nothing at all, for ever, with nothing on
        // screen to say why. Swallowing is still right — the game's own row 6 would open
        // the CARD bag under a 계정창고 title, which is a lie rather than a fallback —
        // but it has to answer. ★ 2026-09-26: in the game's own box, over the window the
        // player pressed OK in (the menu, or the warehouse over its 재료가방 record).
        if self.construction_refused {
            let window = match open {
                AccountOpen::Main { menu } => menu.handle,
                AccountOpen::Materials { parent } => parent.handle,
            };
            if !self.notice(emulator, h2, window, None, NoticeText::Gm(crate::guest_build::Hero2Java::GM_ROW_NOTE_NO_ACCOUNT)) {
                log::warn!("account: no warehouse this run, and the game's box could not say so");
            }
            return true;
        }
        if self.ready() {
            self.open(emulator, open);
        } else {
            log::info!("account: OK on {open:?} before the warehouse is ready — opening when it is");
            self.open_after_load = Some(open);
        }
        true
    }

    fn open(&mut self, emulator: &mut dyn Emulator, open: AccountOpen) {
        // Before the MAIN window exists — its cursor is an index into the order this
        // may change — the 재료가방 goes to cell 0 if it has drifted (2026-09-24).
        if matches!(open, AccountOpen::Main { .. }) {
            self.front_materials_item_if_needed(emulator);
        }
        let (parent, bag) = match open {
            AccountOpen::Main { menu } => (menu, self.bag(Which::Main)),
            AccountOpen::Materials { parent } => (parent, self.bag(Which::Materials)),
        };
        let Some(bag) = bag else { return };
        let args = [wie_backend::JavaArg::Object(parent.handle), wie_backend::JavaArg::Object(bag), wie_backend::JavaArg::Int(0)];
        if let Some(out) = java_call(emulator, crate::guest_build::JavaCall::InventoryWindowNew, JavaThis::None, &args) {
            log::info!("account: opening {open:?} over bag {bag:#x}");
            self.pending = Some(AccountStep::NewWindow { out, open });
        }
    }

    /// ★ Step 7: the MOVE row, kept on whatever bag-window popup is up. Every frame, like
    /// the S7 relabel: `Present` is one chain walk and nothing else; `Slack` is a handful
    /// of writes once; `NeedArray` is one JVM call once (the game's own table builds the
    /// array `q.a(II)Z` would have built, one row longer).
    fn poll_popup(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) {
        if !self.ready() || self.pending.is_some() {
            return;
        }
        let bags = self.popup_bags(emulator, h2);
        let (popup, plan, labels) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            // ⛔ S7 first. A recovery popup in the INVENTORY gets 퀵슬롯등록/해제 appended by
            //   the S7 relabel (3 → 5 rows), and that relabel recognises only the native
            //   three-row shape — a move row appended ahead of it would leave the popup
            //   with the move row and no plate rows. So while S7 still has that popup to
            //   do, this waits a frame; once the popup is S7's five, the sixth is ours.
            if let Some(p) = crate::guest_build::h2_item_popup_walk(&read, h2, crate::bag_page_ctx())
                && p.shape() == Some(true)
                && crate::guest_build::h2_heal_cycle(&read, h2).is_some_and(|cycle| p.bindable(&cycle))
            {
                return;
            }
            let Some(p) = crate::guest_build::h2_bag_popup_walk(&read, h2, &bags, crate::bag_page_ctx()) else { return };
            // ★ 2026-10-21 (user: *"비행서 계정창고에 넣을 수 있어야해"*): a 비행서's popup is NOT skipped any more —
            //   its first stage is the game's own [사용하기 상세보기 버리기] and takes 창고에넣기 / 가방에넣기 like any
            //   other record's; its second stage (the town list `h2_flight_picker_writes` writes over the labels)
            //   is refused by `h2_move_row_plan` on its own, the town names being no popup rows it knows.
            // The 재료가방 record itself never moves: it is the door to the materials.
            if p.record.kind == crate::guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG {
                return;
            }
            // ★ #22: a popup can want TWO host rows — a 망각의 구 gets its own
            //   스탯초기화 / 스킬초기화 as well as the move row. They go on one per poll:
            //   the first append reallocates the label array into a bigger bucket
            //   (`h2_object_array_capacity`: 3 rows land in the 32-byte class, which holds
            //   6), so the second always finds slack. The reset row goes first, so the row
            //   that does the irreversible thing does not move when the other arrives.
            // ★★ 2026-10-17 …and an elixir in the inventory a potion's 퀵슬롯등록 / 해제 between them (`h2_popup_wanted_rows`
            //   — the probe's `--h2-moverow` asks the same), their two labels relabelled first, exactly as S7 does.
            let mut wanted = crate::guest_build::h2_popup_wanted_rows(&read, h2, &p, &self.bags);
            let mut labels = Vec::new();
            let plate_rows = [crate::guest_build::Hero2Java::GM_ROW_BIND, crate::guest_build::Hero2Java::GM_ROW_UNBIND];
            if wanted.contains(&plate_rows[0]) {
                match crate::guest_build::h2_register_label_writes(&read, plate_rows.map(|r| crate::guest_build::h2_gm_row_handle(&read, h2, r))) {
                    Some(w) => labels = w,
                    // Not the table this host knows: no plate rows (they would read `1번에 등록`).
                    None => wanted.retain(|r| !plate_rows.contains(r)),
                }
            }
            let mut chosen = None;
            for row in wanted {
                match crate::guest_build::h2_move_row_plan(&read, h2, &p, row) {
                    crate::guest_build::H2MoveRowPlan::Present(_) => continue,
                    plan => {
                        chosen = Some(plan);
                        break;
                    }
                }
            }
            (p, chosen, labels)
        };
        use crate::guest_build::{H2MoveRowPlan, JavaCall, JavaWrite};
        // ★★ 2026-10-17 the plate rows' labels first — a row is never on screen reading the game's `1번에 등록`.
        for (a, bytes) in &labels {
            java_write(emulator, JavaWrite::PopupLabel, *a, bytes);
        }
        let Some(plan) = plan else { return };
        match plan {
            H2MoveRowPlan::Present(_) | H2MoveRowPlan::Refused => {}
            H2MoveRowPlan::Slack { row, writes } => {
                let handle_slot = popup.labels_fields + 8 + popup.labels_len * 4;
                let mut ok = true;
                for (a, bytes) in &writes {
                    let what = if *a == handle_slot { JavaWrite::PopupLabelRef } else { JavaWrite::PopupRows };
                    ok &= java_write(emulator, what, *a, bytes);
                }
                log::info!("account: move row {} appended in slack on popup {:#x} (kind {}, ok={ok})", row, popup.handle, popup.kind);
            }
            H2MoveRowPlan::NeedArray { gm_rows } => {
                let args: Vec<wie_backend::JavaArg> = gm_rows.iter().map(|r| wie_backend::JavaArg::Int(*r as i32)).collect();
                let call = match gm_rows.len() {
                    2 => JavaCall::TableRows2,
                    3 => JavaCall::TableRows3,
                    n => {
                        log::warn!("account: a popup with {n} rows has no table factory — no move row");
                        return;
                    }
                };
                if let Some(out) = java_call(emulator, call, JavaThis::None, &args) {
                    self.pending = Some(AccountStep::MoveRowArray { out, popup: popup.handle, len: gm_rows.len() as u32 });
                }
            }
        }
    }

    /// The player chose 스탯초기화 / 스킬초기화 on popup `popup` (#22). Refund what the
    /// character spent, put the pool back to the class floor, and consume the orb.
    ///
    /// ── WHY THERE IS NO CONFIRMATION BOX ────────────────────────────────────────
    /// The plan asked for the game's own 예/아니오. Two things stand in for it, and they
    /// are better than one more box saying 하시겠습니까:
    ///  * **The row says what it does.** 스탯초기화 / 스킬초기화, not 사용하기 — the
    ///    generic label was available (gm row 30) and was deliberately not used.
    ///  * **Nothing is permanent until the player saves.** This writes the live character
    ///    object; `ag.l()V` is what puts it on disk. A misfire is undone by quitting
    ///    without saving — the same property #15's split relies on.
    ///
    /// ⛔ The orb is consumed ONLY when the reset actually wrote something. A plan that
    /// refused, or one that found nothing spent, must not eat a 5,000골드 item.
    fn start_flight(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, popup: u32, town: Option<usize>) {
        if self.pending.is_some() { return; }
        let bags = self.popup_bags(emulator, h2);
        let p = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_popup_walk(&read, h2, &bags, crate::bag_page_ctx())
        };
        let Some(p) = p.filter(|p| p.handle == popup && crate::guest_build::h2_is_flight(p)) else { return };
        let (allowed, destinations, picker) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (crate::guest_build::h2_flight_allowed(&read), crate::guest_build::h2_flight_destinations(&read), crate::guest_build::h2_flight_picker(&read, h2, &p))
        };
        if !allowed {
            self.notice(emulator, h2, p.window.win.handle, Some(popup), NoticeText::Gm(77));
            return;
        }
        if destinations.is_empty() {
            self.notice(emulator, h2, p.window.win.handle, Some(popup), NoticeText::Gm(crate::guest_build::H2_FLIGHT_GM_EMPTY));
            return;
        }
        if let Some(town) = town {
            if !destinations.contains(&town) || picker.as_ref().and_then(|rows| usize::try_from(p.sel).ok().and_then(|i| rows.get(i))) != Some(&town) { return; }
            if let Some(out) = java_call(emulator, crate::guest_build::JavaCall::BagConsumeAt, JavaThis::Handle(p.window.bag), &[wie_backend::JavaArg::Int(p.index as i32), wie_backend::JavaArg::Int(1)]) {
                self.pending = Some(AccountStep::FlightConsume { out, town, menu_fields: p.menu_fields, bag: p.window.bag });
            }
        } else if picker.is_none() {
            let writes = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_flight_picker_writes(&read, h2, &p)
            };
            if let Some(writes) = writes {
                for (addr, bytes) in writes {
                    java_write(emulator, crate::guest_build::JavaWrite::PopupLabelRef, addr, &bytes);
                }
            }
        }
    }

    /// The 비행서's last step: the game's own map transition to `town` (the menu is released by now).
    fn flight_warp(&mut self, emulator: &mut dyn Emulator, town: usize) {
        let (map, x, y) = crate::guest_build::H2_FLIGHT_TOWNS[town];
        match java_call(emulator, crate::guest_build::JavaCall::FlightWarp, JavaThis::None, &[map, x, y, 2].map(|v| wie_backend::JavaArg::Int(v as i32))) {
            Some(out) => self.pending = Some(AccountStep::FlightWarp { out }),
            None => log::error!("flight: the warp call was refused"),
        }
    }

    fn start_reset(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, popup: u32, what: crate::guest_build::H2Oblivion) {
        if self.pending.is_some() {
            log::info!("reset: something is already in flight");
            return;
        }
        use crate::guest_build::H2Oblivion;
        // ★★ 2026-10-20 상점주인의 혼: not a reset at all — the game's own shop, nothing consumed.
        if what == H2Oblivion::Shop {
            self.start_shop(emulator, h2, popup);
            return;
        }
        let bags = self.popup_bags(emulator, h2);
        // Re-walk: the row the player pressed was read frames ago, and every other action
        // in this host re-asks for the record under the cursor at action time.
        let (plan, bag, rec, window) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let Some(p) = crate::guest_build::h2_bag_popup_walk(&read, h2, &bags, crate::bag_page_ctx()) else {
                log::info!("reset: the popup went away");
                return;
            };
            if p.handle != popup || crate::guest_build::h2_oblivion_of(p.record.kind, p.record.id) != Some(what) {
                log::info!("reset: popup {popup:#x} no longer shows that orb — dropped");
                return;
            }
            let plan = match what {
                H2Oblivion::Stat => crate::guest_build::h2_stat_reset(&read, h2),
                H2Oblivion::Skill => crate::guest_build::h2_skill_reset(&read, h2),
                H2Oblivion::Elixir => crate::guest_build::h2_elixir_plan(&read, h2), // ★★ 2026-10-16: HP · SP to the maxima
                H2Oblivion::Shop => None, // branched to `start_shop` above — never a reset plan
            };
            (plan, p.window.bag, p.record.handle, p.window.win.handle)
        };
        use crate::guest_build::Hero2Java;
        // ★ 2026-09-26: every answer in the game's own box, in place of the orb's popup.
        let Some(plan) = plan else {
            log::warn!("reset: the plan refused — nothing written, the orb is kept");
            self.notice(emulator, h2, window, Some(popup), NoticeText::Gm(Hero2Java::GM_ROW_NOTE_RESET_FAILED));
            return;
        };
        if plan.writes.is_empty() {
            // ★★ 2026-10-16: the elixir on a full character — its own notice, the item kept
            let note = if what == H2Oblivion::Elixir { Hero2Java::GM_ROW_NOTE_VITALS_FULL } else { Hero2Java::GM_ROW_NOTE_NOTHING_TO_RESET };
            log::info!("reset: nothing to do for {what:?} — the item is kept");
            self.notice(emulator, h2, window, Some(popup), NoticeText::Gm(note));
            return;
        }
        let mut done = 0usize;
        for (a, bytes) in &plan.writes {
            let gate = {
                let read = |x: u32, l: usize| emulator.read_memory(x, l);
                crate::guest_build::h2_reset_gate(&read, h2, what, *a)
            };
            if !java_write(emulator, gate, *a, bytes) {
                break;
            }
            done += 1;
        }
        if done != plan.writes.len() {
            // ⛔ No rollback, no consumption. The point total is written LAST, so a stop
            // partway through has cleared some slots without paying for them — bad, but
            // strictly better than also eating the orb, and a second reset collects what
            // is left (the planner is idempotent: it only ever reads what is still spent).
            log::error!("reset: {done}/{} write(s) landed — the orb is kept", plan.writes.len());
            self.notice(emulator, h2, window, Some(popup), NoticeText::Gm(Hero2Java::GM_ROW_NOTE_RESET_FAILED));
            return;
        }
        log::info!(
            "reset: {} — {} point(s) back over {} slot(s), {} write(s)",
            match what {
                H2Oblivion::Stat => "stat",
                H2Oblivion::Skill => "skill",
                H2Oblivion::Elixir => "elixir",
                H2Oblivion::Shop => "shop",
            },
            plan.refund,
            plan.cleared,
            plan.writes.len()
        );
        // A skill reset can leave the quickbar holding a skill the character no longer
        // has, whose key would then do something undefined.
        if what == H2Oblivion::Skill {
            self.clear_quickbar(emulator, h2);
        }
        // ★★ 2026-10-17: the elixir is drunk — 술꾼의 장화 walks as after a potion.
        if what == H2Oblivion::Elixir {
            gear_drink(emulator);
        }
        let at = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_index_of(&read, bag, rec)
        };
        // From here the reset HAS happened, whatever becomes of the orb, and that is what
        // the notice says.
        // ★★ 2026-10-16: the elixir says nothing afterwards (a potion's silence) — note 0 = close the popup only
        let note = match what {
            H2Oblivion::Stat => Hero2Java::GM_ROW_NOTE_STAT_RESET,
            H2Oblivion::Skill => Hero2Java::GM_ROW_NOTE_SKILL_RESET,
            H2Oblivion::Elixir => 0,
            H2Oblivion::Shop => 0,
        };
        let Some(at) = at else {
            log::error!("reset: the orb is not in the bag any more — the reset stands, nothing consumed");
            self.after_reset(emulator, h2, bag, note);
            return;
        };
        match java_call(
            emulator,
            crate::guest_build::JavaCall::BagConsumeAt,
            JavaThis::Handle(bag),
            &[wie_backend::JavaArg::Int(at as i32), wie_backend::JavaArg::Int(1)],
        ) {
            Some(out) => self.pending = Some(AccountStep::ResetConsume { out, refund: plan.refund, bag, note }),
            None => {
                log::error!("reset: the consume call was refused — the reset stands, the orb is still there");
                self.after_reset(emulator, h2, bag, note);
            }
        }
    }

    /// ★★ 2026-10-20 상점주인의 혼 (user: *"사용하면 상점npc에게 대화한것처럼 아이템 구매와 판매를 할 수있는 휴대용 상점 …
    /// 사용하기 누르면 기존 상점 npc한테 말한것처럼 아이템 구매, 아이템 판매 선택지 뜨고 기존 상점 npc처럼 이용되게"*): the
    /// popup's 사용하기 on the shop orb. The game's own shop, opened the way a shop NPC's script opens it — the event
    /// `(4, 0)` ([`JavaCall::EventPost`], [`Hero2Java::SHOP_EVENT`]), which the dispatcher `ag.g()V` turns into
    /// `ag.h(7)` · `v.a()Lv;` · `v.a()V` on its next tick — with the two things a script never had to do: the shop STOCK
    /// `v.a:[Vector` is filled when it reads `null` (a shop map's load fills it, any other map's load clears it, and the
    /// buy list `p.a()V` indexes it unchecked — the game's own loader `z.a()` through [`JavaCall::ShopStockLoad`] →
    /// [`JavaWrite::ShopStock`]), and the field menu the popup sits in is closed first (its child reference cleared, the
    /// state word back to 3 — the flight scroll's exit), since the shop is a state of its own (7) over the field and its
    /// close `v.c()V` returns to 3, not to the menu. Nothing is consumed: the orb is permanent (user).
    fn start_shop(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, popup: u32) {
        if self.pending.is_some() {
            log::info!("shop orb: something is already in flight");
            return;
        }
        let bags = self.popup_bags(emulator, h2);
        let (menu_fields, stock) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let Some(p) = crate::guest_build::h2_bag_popup_walk(&read, h2, &bags, crate::bag_page_ctx()) else {
                log::info!("shop orb: the popup went away");
                return;
            };
            if p.handle != popup || crate::guest_build::h2_oblivion_of(p.record.kind, p.record.id) != Some(crate::guest_build::H2Oblivion::Shop) {
                log::info!("shop orb: popup {popup:#x} no longer shows the orb — dropped");
                return;
            }
            // ★★ (user: 「계정창고에서 사용은 안되어야해」) — the INVENTORY's popup only; the row is never appended elsewhere,
            // and this is the gate behind it.
            if crate::guest_build::h2_static_handle(&read, h2.inventory_slot) != Some(p.window.bag) {
                log::info!("shop orb: popup {popup:#x} is not on the inventory — refused");
                return;
            }
            (p.menu_fields, crate::guest_build::h2_static_handle(&read, crate::guest_build::Hero2Java::SHOP_STOCK_SLOT))
        };
        if stock.is_some() {
            log::info!("shop orb: stock already loaded ({stock:x?}) — opening the shop");
            self.open_shop(emulator, Some(menu_fields));
            return;
        }
        match java_call(emulator, crate::guest_build::JavaCall::ShopStockLoad, JavaThis::None, &[]) {
            Some(out) => self.pending = Some(AccountStep::ShopStock { out, menu_fields: Some(menu_fields) }),
            None => log::warn!("shop orb: the stock loader was refused — the shop is not opened"),
        }
    }

    /// ★★ 2026-10-21 (user: *"상점주인의 혼 아이템도 퀵슬롯등록해서 사용가능했으면 좋겠어. 물론 계정창고에 있는건 여전히
    /// 퀵슬롯등록되면 안돼고 실제 세이브별 인벤토리에 있을때만 퀵슬롯등록 및 사용 가능해야해."*): the 상점주인의 혼 PLATE pressed
    /// on the field — the popup's 사용하기 ([`Self::start_shop`]) without a popup or a menu: the live save's INVENTORY must
    /// hold one (`h2_item_stock` walks `ag.a` — a warehouse orb is not there), then the stock when `null`, then the shop
    /// event. Nothing is consumed. `Err` = why nothing happened ([`Self::SHOP_ORB_NONE`] = none in the inventory).
    fn use_shop_plate(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> Result<(), &'static str> {
        if self.pending.is_some() {
            return Err("a JVM call is in flight");
        }
        let stock = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            if !items::in_field_state(&read) {
                return Err("not on the field");
            }
            let inv = crate::guest_build::h2_inventory(&read, h2).ok_or("the bag cannot be read")?;
            crate::guest_build::h2_item_stock(&inv, crate::guest_build::H2_PLATE_SHOP_ORB).filter(|(_, n)| *n > 0).ok_or(Self::SHOP_ORB_NONE)?;
            crate::guest_build::h2_static_handle(&read, crate::guest_build::Hero2Java::SHOP_STOCK_SLOT)
        };
        if stock.is_some() {
            self.open_shop(emulator, None);
            return Ok(());
        }
        match java_call(emulator, crate::guest_build::JavaCall::ShopStockLoad, JavaThis::None, &[]) {
            Some(out) => {
                self.pending = Some(AccountStep::ShopStock { out, menu_fields: None });
                Ok(())
            }
            None => Err("the stock loader was refused"),
        }
    }

    const SHOP_ORB_NONE: &'static str = "no 상점주인의 혼 in the inventory";

    /// [`Self::start_shop`]'s last step, once the stock is there: the field menu closed (★★ 2026-10-21 RELEASED — state ← 3,
    /// `bw.a(Z)V`, [`release_field_menu`] — exactly the flight scroll's exit) and the shop event posted. ⛔ The menu first:
    /// `v.c()V` puts the state back to 3, and a menu still hanging its window chain would then be a field with an invisible,
    /// key-eating window on it — and a menu left alive is reused, undrawn, by the next open. `None` = a plate: no menu.
    fn open_shop(&mut self, emulator: &mut dyn Emulator, menu_fields: Option<u32>) {
        if let Some(menu_fields) = menu_fields
            && let Some(out) = release_field_menu(emulator, menu_fields)
        {
            self.pending = Some(AccountStep::ShopMenu { out, menu_fields });
            return;
        }
        self.post_shop_event(emulator);
    }

    /// The shop event `(4, 0)` — the menu (if any) is closed by now.
    fn post_shop_event(&mut self, emulator: &mut dyn Emulator) {
        use crate::guest_build::{Hero2Java, JavaCall};
        let (d, e) = Hero2Java::SHOP_EVENT;
        match java_call(emulator, JavaCall::EventPost, JavaThis::None, &[wie_backend::JavaArg::Int(d), wie_backend::JavaArg::Int(e)]) {
            Some(out) => {
                log::info!("shop orb: shop event ({d}, {e}) posted");
                self.pending = Some(AccountStep::ShopEvent { out });
            }
            None => log::warn!("shop orb: the event post was refused — the field is back, no shop"),
        }
    }

    /// ★★ 2026-10-17 (user: *"엘릭서가 퀵슬롯 등록이 안되고있는데 이거 포션종류 취급되어야해"*): an elixir PLATE pressed on
    /// the field — what the popup's 사용하기 does ([`Self::start_reset`] with `H2Oblivion::Elixir`), without a popup: HP ·
    /// SP to their maxima (`h2_elixir_plan`), 술꾼의 장화's walk ([`gear_drink`]), then one consumed from the bag's first
    /// (21, 7) record (`bq.a(BI)V`). A potion's ways: nothing is said; a full character keeps the elixir (the plan writes
    /// nothing); a falling or fallen one cannot drink (`h2_elixir_plan` refuses — the `0` key's `a.q()V` skips `g.t` 6 · 7).
    /// `Err` = why nothing happened ([`Self::ELIXIR_NONE`] = none in the bag, the plate's toast).
    fn use_elixir_plate(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> Result<(), &'static str> {
        if self.pending.is_some() {
            return Err("a JVM call is in flight");
        }
        let (plan, bag, rec) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            if !items::in_field_state(&read) {
                return Err("not on the field");
            }
            let inv = crate::guest_build::h2_inventory(&read, h2).ok_or("the bag cannot be read")?;
            let (i, _) = crate::guest_build::h2_item_stock(&inv, crate::guest_build::H2_PLATE_ELIXIR).ok_or(Self::ELIXIR_NONE)?;
            let rec = inv.items.get(usize::from(i)).map(|it| it.handle).ok_or(Self::ELIXIR_NONE)?;
            (crate::guest_build::h2_elixir_plan(&read, h2), inv.handle, rec)
        };
        let plan = plan.ok_or("the character cannot drink now")?;
        if plan.writes.is_empty() {
            return Err("HP and SP are full — the elixir is kept");
        }
        for (a, bytes) in &plan.writes {
            let gate = {
                let read = |x: u32, l: usize| emulator.read_memory(x, l);
                crate::guest_build::h2_reset_gate(&read, h2, crate::guest_build::H2Oblivion::Elixir, *a)
            };
            if !java_write(emulator, gate, *a, bytes) {
                return Err("a refill write was refused — the elixir is kept");
            }
        }
        gear_drink(emulator);
        let at = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_index_of(&read, bag, rec)
        };
        let Some(at) = at else {
            log::error!("elixir plate: refilled, but the record left the bag — nothing consumed");
            return Ok(());
        };
        match java_call(emulator, crate::guest_build::JavaCall::BagConsumeAt, JavaThis::Handle(bag), &[wie_backend::JavaArg::Int(at as i32), wie_backend::JavaArg::Int(1)]) {
            Some(out) => self.pending = Some(AccountStep::ElixirConsume { out }),
            None => log::error!("elixir plate: refilled, but the consume call was refused — the elixir is still there"),
        }
        Ok(())
    }

    const ELIXIR_NONE: &'static str = "none in the bag";

    /// Empty the game's 4-slot quickbar and its cooldowns.
    ///
    /// ⛔ The empty quickbar byte is `-1`, not 0 — the probe reads a fresh bar as
    /// `[-1, -1, -1, -1]` (HERO2_QUICKSLOT_TOUCH_PLAN.md) and 0 would be a real slot.
    fn clear_quickbar(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) {
        use crate::guest_build::{Hero2Java, JavaWrite};
        let (bar, cool) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            (
                crate::guest_build::h2_static_handle(&read, h2.quickbar_slot).and_then(|h| crate::guest_build::h2_array(&read, h)),
                crate::guest_build::h2_static_handle(&read, h2.cooldown_slot).and_then(|h| crate::guest_build::h2_array(&read, h)),
            )
        };
        if let Some((base, len)) = bar {
            for i in 0..len.min(Hero2Java::QUICKBAR_LEN) {
                java_write(emulator, JavaWrite::Quickbar, base + i, &[0xffu8]);
            }
        }
        if let Some((base, len)) = cool {
            for i in 0..len.min(Hero2Java::QUICKBAR_LEN) {
                java_write(emulator, JavaWrite::Cooldown, base + 4 * i, &0i32.to_le_bytes());
            }
        }
    }

    /// The player chose 창고에넣기 / 가방에넣기 on popup `popup`. The record under the
    /// window's cursor goes to the paired bag AS THE SAME OBJECT — insert there first, and
    /// only when the bag took it, out of here (HERO2_GUEST_RE.md §2.7.8).
    fn start_move(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, popup: u32) {
        if self.pending.is_some() {
            log::info!("account: a move is already in flight");
            return;
        }
        if !self.ready() {
            return;
        }
        let bags = self.popup_bags(emulator, h2);
        let game = self.game_bags(emulator, h2);
        // ⛔ Everything the guest is asked is read inside this block: opening the
        //   개수 box needs `emulator` mutably and the reader closure borrows it.
        let plan = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let Some(p) = crate::guest_build::h2_bag_popup_walk(&read, h2, &bags, crate::bag_page_ctx()) else {
                log::info!("account: the popup went away before the move");
                return;
            };
            if p.handle != popup {
                log::info!("account: the popup changed under the move — dropped");
                return;
            }
            let Some((dst, to_account)) = crate::guest_build::h2_move_destination(&p, &game, &self.bags) else {
                log::info!("account: no destination for a move out of bag {:#x}", p.window.bag);
                return;
            };
            // ★★ 2026-09-26 (user: "장착중인 장비는 계정창고에 들어가면 안돼. 창고에 넣기 누르면
            //   불가능하다고 안내팝업떠야해"): a WORN record stays with the character. The
            //   game's own test (`bq.a(Lch;)Z`, the one its 버리기 and 판매 refuse by), and
            //   the game's own box to say so. ⛔ Unreadable counts as worn — this guards the
            //   character's equipment, and a record the host cannot read is not one to move.
            if to_account && crate::guest_build::h2_record_worn(&read, p.record.handle) != Some(false) {
                log::info!(
                    "account: record {:#x} (type {} id {}) is WORN — it stays out of the warehouse",
                    p.record.handle, p.record.kind, p.record.id
                );
                MoveStart::Refuse { p, text: NoticeText::Gm(crate::guest_build::Hero2Java::GM_ROW_NOTE_EQUIPPED) }
            }
            // ★ A stack of more than one goes through the game's own 개수 box first; the
            //   host moves the WHOLE record only when there is nothing to choose
            //   (2026-09-21). The room check goes with it: a split asks
            //   `bq.a(Lch;B)I` instead, which merges into existing stacks and so has
            //   room that a record count alone would deny.
            else if crate::guest_build::h2_split_allowed(p.record.kind, p.record.count) {
                MoveStart::Split(p)
            }
            // ★ 2026-10-13 (user: *"계정창고에 재료를 따로따로 넣으면 99개가 되지않았는데도 서로 다른 칸으로
            //   들어가는데 이거 합쳐져야해"*): one of a stackable kind joins the destination's stack of it —
            //   the room question is `bq.a(Lch;B)I`'s (a full bag with a stack that has room still takes it).
            else if crate::guest_build::h2_move_merges(p.record.kind) {
                MoveStart::Merge { p, dst, to_account }
            } else {
                let Some(dst_head) = crate::guest_build::h2_bag_head_of(&read, dst) else { return };
                if !crate::guest_build::h2_bag_has_room(&dst_head) {
                    log::info!("account: no room in {dst:#x} ({} of {})", dst_head.count, dst_head.b);
                    MoveStart::Refuse { p, text: NoticeText::BagFull }
                } else {
                    // Out of the warehouse, a record carries no worn flag — see
                    // `clear_worn_flags`, which takes them off at load; this is the same
                    // rule at the door, for a flag that load did not see.
                    let clear = if to_account { None } else { crate::guest_build::h2_worn_flag_clear(&read, p.record.handle) };
                    MoveStart::Whole { p, dst, to_account, clear }
                }
            }
        };
        let (p, dst, to_account) = match plan {
            MoveStart::Refuse { p, text } => {
                self.notice(emulator, h2, p.window.win.handle, Some(p.handle), text);
                return;
            }
            MoveStart::Split(p) => return self.open_quantity(emulator, &p),
            MoveStart::Merge { p, dst, to_account } => {
                let (rec, kind, id, n) = (p.record.handle, p.record.kind, p.record.id, p.record.count);
                self.start_split(emulator, p.window.bag, rec, kind, id, dst, n, SplitGoal::Move { to_account });
                return;
            }
            MoveStart::Whole { p, dst, to_account, clear } => {
                if let Some((a, bytes)) = clear {
                    if !java_write(emulator, crate::guest_build::JavaWrite::RecordEquipSlot, a, &bytes) {
                        log::error!("account: record {:#x} still carries a worn flag and could not lose it — not moved", p.record.handle);
                        return;
                    }
                    log::info!("account: record {:#x} lost a stale worn flag on its way out of the warehouse", p.record.handle);
                }
                (p, dst, to_account)
            }
        };
        let args = [wie_backend::JavaArg::Object(p.record.handle), wie_backend::JavaArg::Int(p.record.count)];
        if let Some(out) = java_call(emulator, crate::guest_build::JavaCall::BagInsertRecord, JavaThis::Handle(dst), &args) {
            log::info!(
                "account: moving record {:#x} (type {} id {} x{}) from {:#x}[{}] to {:#x}",
                p.record.handle, p.record.kind, p.record.id, p.record.count, p.window.bag, p.index, dst
            );
            self.pending = Some(AccountStep::MoveInsert { out, src: p.window.bag, index: p.index, rec: p.record.handle, to_account });
        }
    }

    /// ★ The 개수 box over the record the player chose (2026-09-21, user:
    /// "계정창고로 넣을때 갯수를 지정해서 넣을 수 있었으면"): `new aq(window, 0, count)`, hung in
    /// place of the popup it was chosen from. The number is the GAME's to run — its own
    /// keys move it and clamp it — and the host takes only the OK
    /// ([`Self::take_quantity_ok`]), so nothing about the move is remembered here beyond
    /// the three handles that identify it.
    fn open_quantity(&mut self, emulator: &mut dyn Emulator, p: &crate::guest_build::H2BagPopup) {
        let max = crate::guest_build::h2_qty_ceiling(p.record.count);
        let args = [
            wie_backend::JavaArg::Object(p.window.win.handle),
            wie_backend::JavaArg::Int(0),
            wie_backend::JavaArg::Int(max),
        ];
        if let Some(out) = java_call(emulator, crate::guest_build::JavaCall::QuantityPopupNew, JavaThis::None, &args) {
            log::info!(
                "account: 개수 box for record {:#x} (type {} id {} x{}) in bag {:#x}, max {max}",
                p.record.handle, p.record.kind, p.record.id, p.record.count, p.window.bag
            );
            self.pending = Some(AccountStep::QtyNew { out, window: p.window, record: p.record.handle });
        }
    }

    /// ★ An OK while the 개수 box is up is the host's: the number the box shows, of the
    /// record it was opened over, into the bag that record's window pairs with.
    ///
    /// Everything is re-derived here and nothing is carried from the open — the box stays
    /// up for as long as the player wants it, and the bag under it is live the whole time
    /// (a drop, the `0` key, a stack hitting zero all move records about). `true` = taken,
    /// the key is swallowed.
    fn take_quantity_ok(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) -> bool {
        let Some(q) = self.qty else { return false };
        if self.pending.is_some() {
            log::info!("account: OK on the 개수 box while a call is in flight — ignored");
            return true;
        }
        let bags = self.popup_bags(emulator, h2);
        let game = self.game_bags(emulator, h2);
        let (live, what) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let live = crate::guest_build::h2_qty_popup(&read, h2, &bags)
                .filter(|live| live.handle == q.popup && live.window.win.handle == q.window && live.window.bag == q.bag);
            let what = live.and_then(|live| {
                // By HANDLE, never by the index it had: the Vector closes up behind a
                // removal at a lower index (`bq.b(B)V`), and the box has no clock.
                let index = crate::guest_build::h2_bag_index_of(&read, q.bag, q.record)?;
                let rec = crate::guest_build::h2_bag_record_at(&read, q.bag, index)?;
                (rec.handle == q.record).then_some(())?;
                let (dst, to_account) = crate::guest_build::h2_move_destination_of(q.bag, rec.kind, &game, &self.bags)?;
                Some((crate::guest_build::h2_qty_taken(live.value, rec.count), rec, index, dst, to_account))
            });
            (live.is_some(), what)
        };
        // ⛔ A box that is no longer on screen does not get to swallow the key. The poll
        //   that forgets it runs every five frames, and in between an OK belongs to
        //   whatever the player is actually looking at.
        if !live {
            log::info!("account: the 개수 box {:#x} is gone — the OK is the game's", q.popup);
            self.qty = None;
            return false;
        }
        let Some((n, rec, index, dst, to_account)) = what else {
            log::info!("account: the 개수 box no longer describes a move — OK dropped");
            return true;
        };
        // "All of it" of a record that does not stack is the object itself — its serial and
        // rolled stats travel with it. ★ 2026-10-13: all of a STACK goes through the game's
        // merge like a part of one (`h2_move_merges`) — the object move only appends, and a
        // stack moved whole sat beside the destination's stack of the same thing.
        if n >= rec.count && !crate::guest_build::h2_move_merges(rec.kind) {
            log::info!("account: 개수 box chose all {} — moving the record itself", rec.count);
            let args = [wie_backend::JavaArg::Object(rec.handle), wie_backend::JavaArg::Int(rec.count)];
            if let Some(out) = java_call(emulator, crate::guest_build::JavaCall::BagInsertRecord, JavaThis::Handle(dst), &args) {
                self.pending = Some(AccountStep::MoveInsert { out, src: q.bag, index, rec: rec.handle, to_account });
            }
            return true;
        }
        self.start_split(emulator, q.bag, rec.handle, rec.kind, rec.id, dst, n.min(rec.count), SplitGoal::Move { to_account });
        true
    }

    /// The split's first call — `ch.a(type, id, 1, 0)`, the fresh record `n` of `rec` (in `src`) go into `dst` as.
    /// The rest is the step machine's ([`AccountStep::SplitNew`] → `SplitAdd` → `SplitConsume`).
    #[allow(clippy::too_many_arguments)]
    fn start_split(&mut self, emulator: &mut dyn Emulator, src: u32, rec: u32, kind: i8, id: i8, dst: u32, n: i32, goal: SplitGoal) -> bool {
        let args = [
            wie_backend::JavaArg::Int(i32::from(kind)),
            wie_backend::JavaArg::Int(i32::from(id)),
            wie_backend::JavaArg::Int(1),
            wie_backend::JavaArg::Int(0),
        ];
        match java_call(emulator, crate::guest_build::JavaCall::RecordNew, JavaThis::None, &args) {
            Some(out) => {
                log::info!("account: {goal:?} — {n} of record {rec:#x} (type {kind} id {id}) out of bag {src:#x} into {dst:#x}");
                self.pending = Some(AccountStep::SplitNew { out, src, rec, dst, n, goal });
                true
            }
            None => false,
        }
    }

    /// ★ 2026-10-13: the warehouse load's FOLD — the next record of either bag that a stack of its kind before it
    /// has room for goes into that stack (`hero2java::h2_bag_merge_candidate`), one split at a time, the bag as
    /// both ends; nothing left (or [`Self::FOLD_ROUNDS_MAX`] done) → the load goes on to the 재료가방 cell, which
    /// makes the warehouse ready. Runs before `loaded`, so no window shows the bags while their order changes.
    /// What it fixes is the cells the old whole-record move left apart (four 제련석 put in one at a time = four
    /// cells): the next save writes them folded.
    fn fold_next(&mut self, emulator: &mut dyn Emulator) {
        if self.fold_rounds < Self::FOLD_ROUNDS_MAX {
            for which in Which::BOTH {
                let Some(bag) = self.bag(which) else { continue };
                let next = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    crate::guest_build::h2_bag_merge_candidate(&read, bag)
                };
                if let Some(rec) = next {
                    self.fold_rounds += 1;
                    if self.start_split(emulator, bag, rec.handle, rec.kind, rec.id, bag, rec.count, SplitGoal::Fold) {
                        return;
                    }
                    log::warn!("account: fold — the record call was refused; the warehouse stays as loaded");
                    break;
                }
            }
        } else {
            log::warn!("account: fold — stopped after {} rounds", self.fold_rounds);
        }
        if self.fold_rounds > 0 {
            log::info!("account: warehouse folded ({} round(s))", self.fold_rounds);
        }
        self.ensure_materials_item(emulator);
    }

    /// A split that ended before its last step: a move just stops, as it always did; the load's fold stops
    /// folding and the load goes on (the 재료가방 cell, then ready) — a warehouse that will not fold is still one.
    fn split_ended(&mut self, emulator: &mut dyn Emulator, goal: SplitGoal) {
        if goal == SplitGoal::Fold {
            log::warn!("account: fold stopped after {} round(s) — the rest stays as loaded", self.fold_rounds);
            self.ensure_materials_item(emulator);
        }
    }

    /// ★ Is the box the host hung still there? It closes itself on CLR
    /// (`aq.a(II)Z` → `parent.b(0)`), so the host learns it is gone by looking.
    fn drop_stale_quantity(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) {
        let Some(q) = self.qty else { return };
        if self.pending.is_some() {
            return;
        }
        let bags = self.popup_bags(emulator, h2);
        let read = |a: u32, l: usize| emulator.read_memory(a, l);
        let live = crate::guest_build::h2_qty_popup(&read, h2, &bags).is_some_and(|live| live.handle == q.popup);
        if !live {
            log::info!("account: the 개수 box {:#x} closed", q.popup);
            self.qty = None;
        }
    }

    /// The move's tail: the warehouse marked for the next save, the modal the move came
    /// from closed, and the game's own box saying where the item went (★ 2026-09-26: this
    /// was a CLR and a host toast).
    fn finish_move(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, src: u32, to_account: bool) {
        self.dirty = true;
        self.qty = None;
        let note = if to_account { crate::guest_build::Hero2Java::GM_ROW_NOTE_TO_ACCOUNT } else { crate::guest_build::Hero2Java::GM_ROW_NOTE_TO_BAG };
        self.notice_after(emulator, h2, src, NoticeText::Gm(note));
    }

    /// ★★ 2026-09-26 (user: "안내팝업은 항상 기존 게임 팝업으로 만들어줘. 내가 직접그린
    /// 안내토스트문구가 뜨는게아니라"): say `text` in the GAME's own message box — `new ba(window,
    /// text)`, hung on `window` once it is built, in place of `replaces` (the popup the player
    /// chose from, when NOTHING has happened under it; `None` = the window is the top). The
    /// game paints it in its own font and closes it on OK/CLR by itself (`ba.a(II)Z` →
    /// `parent.b(0)`). What the game does for its own 「장착중인 아이템은 버릴수 없습니다.」
    /// (`q.a(BB)V` @0x146a22). After something HAS happened, use [`Self::notice_after`].
    ///
    /// `false` when the box cannot even be asked for — a JVM call in flight, the text row
    /// missing (an asset from before 2026-09-26), the call refused. Logged; there is no
    /// host toast to fall back on, by the same instruction.
    fn notice(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, window: u32, replaces: Option<u32>, text: NoticeText) -> bool {
        if self.pending.is_some() {
            log::warn!("notice: {text:?} — a JVM call is in flight, not shown");
            return false;
        }
        let handle = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            match text {
                NoticeText::Gm(row) => crate::guest_build::h2_gm_row_handle(&read, h2, row),
                NoticeText::BagFull => crate::guest_build::h2_bag_full_text(&read),
            }
        };
        let Some(handle) = handle else {
            log::warn!("notice: {text:?} has no text — was the asset regenerated (gm rows 82..=89)?");
            return false;
        };
        let args = [wie_backend::JavaArg::Object(window), wie_backend::JavaArg::Object(handle)];
        match java_call(emulator, crate::guest_build::JavaCall::MessageBoxNew, JavaThis::None, &args) {
            Some(out) => {
                self.pending = Some(AccountStep::NoticeNew { out, window, replaces, text });
                true
            }
            None => {
                log::warn!("notice: {text:?} — new ba(..) refused");
                false
            }
        }
    }

    /// [`Self::notice`] over the innermost window that shows `bag`, in place of whatever
    /// modal is up over it right now — for a refusal, where nothing under that modal changed.
    fn notice_over(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, bag: u32, text: NoticeText) -> bool {
        let target = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_window(&read, h2, &[bag])
                .and_then(|v| crate::guest_build::h2_window_top_child(&read, h2, v.win.handle).map(|child| (v.win.handle, child)))
        };
        match target {
            Some((window, replaces)) => self.notice(emulator, h2, window, replaces, text),
            None => {
                log::info!("notice: {text:?} — no window shows bag {bag:#x} any more");
                false
            }
        }
    }

    /// A notice about something that HAS happened under the modal over `bag`'s window — a
    /// record moved, the character reset. That modal is STALE: its parent's cursor now names
    /// another record, so its 버리기 would act on a bystander. It is closed NOW, synchronously
    /// — exactly `j.b(I)V` @0x15e5cc, the game's own "close the child": `j.b ← null`, the
    /// repaint flag on the window and everything under it (`j.g()V` walks the parents), then
    /// `n.b()` — and the box goes up over the bare window when it is built. ⛔ Not a queued
    /// CLR (what the tail used to send): the box would then be racing the key it came after.
    fn notice_after(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, bag: u32, text: NoticeText) -> bool {
        let Some(window) = self.close_popup_of(emulator, h2, bag) else {
            log::info!("notice: {text:?} — no window shows bag {bag:#x} any more");
            return false;
        };
        self.notice(emulator, h2, window, None, text)
    }

    /// The popup on the window showing `bag` closed with an immediate write (`j.b ← null` + repaint, `HudDirty`) — the
    /// window's handle, or `None` when no window shows that bag any more. ★★ 2026-10-16: split out of [`Self::notice_after`]
    /// for the elixir, which closes its popup and says nothing (a potion's silence).
    fn close_popup_of(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, bag: u32) -> Option<u32> {
        let (window, writes) = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_window(&read, h2, &[bag]).and_then(|v| {
                let chain = crate::guest_build::h2_window_chain(&read, h2)?;
                Some((v.win.handle, crate::guest_build::h2_child_open_writes(&v.win, 0, &chain)))
            })?
        };
        for (kind, a, bytes) in &writes {
            java_write(emulator, *kind, *a, bytes);
        }
        let _ = java_call(emulator, crate::guest_build::JavaCall::HudDirty, JavaThis::None, &[]);
        Some(window)
    }

    /// After a 망각의 구 / 엘릭서 acted: the popup closes and — `note` ≠ 0 — the game's box says `note` (gm row); the elixir
    /// (note 0) closes the popup only.
    fn after_reset(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, bag: u32, note: u32) {
        if note == 0 {
            if self.close_popup_of(emulator, h2, bag).is_none() {
                log::info!("elixir: no window shows bag {bag:#x} any more — nothing to close");
            }
        } else {
            self.notice_after(emulator, h2, bag, NoticeText::Gm(note));
        }
    }

    /// Advance the call in flight; keep the title swap in step with the windows; write
    /// the warehouse when the game saves.
    fn poll(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java, data_dir: &std::path::Path) {
        match self.pending.take() {
            Some(AccountStep::FlightConsume { out, town, menu_fields, bag }) => match out.poll() {
                None => self.pending = Some(AccountStep::FlightConsume { out, town, menu_fields, bag }),
                Some(Err(())) => log::error!("flight: consume failed"),
                Some(Ok(_)) => {
                    // ★ 2026-10-21: a scroll used out of the WAREHOUSE window is gone from the warehouse bag — the next
                    //   save must write that bag, or the reload hands the scroll back (`dirty` is what the save edge
                    //   looks at). The destinations were the live save's own (`ag.q[31]`), never the scroll's.
                    if self.bags.contains(&Some(bag)) {
                        self.dirty = true;
                        log::info!("flight: consumed out of warehouse bag {bag:#x} — marked for the next save");
                    }
                    // ★★ 2026-10-21 (user: *"비행서 사용한 직후에 메뉴버튼 누르면 메뉴가 안열리고있어"*): the menu is RELEASED the way
                    //   the game's own close does it, then the warp — the child ← 0 alone left the singleton for the next
                    //   open to reuse, and a reused menu is never drawn.
                    match release_field_menu(emulator, menu_fields) {
                        Some(out) => self.pending = Some(AccountStep::FlightMenu { out, town, menu_fields }),
                        None => self.flight_warp(emulator, town),
                    }
                }
            },
            Some(AccountStep::FlightMenu { out, town, menu_fields }) => match out.poll() {
                None => self.pending = Some(AccountStep::FlightMenu { out, town, menu_fields }),
                Some(result) => {
                    if result.is_err() {
                        menu_release_failed(emulator, menu_fields);
                    }
                    self.flight_warp(emulator, town);
                }
            },
            Some(AccountStep::FlightWarp { out }) => match out.poll() {
                None => self.pending = Some(AccountStep::FlightWarp { out }),
                Some(Ok(_)) => log::info!("flight: arrived via native map transition"),
                Some(Err(())) => log::error!("flight: map transition failed"),
            },
            None => {}
            Some(AccountStep::NewBag { out, which }) => match out.poll() {
                None => self.pending = Some(AccountStep::NewBag { out, which }),
                Some(Err(())) => self.refuse_construction("the bq constructor failed"),
                Some(Ok(0)) => self.refuse_construction("the bq constructor returned null"),
                Some(Ok(bag)) => {
                    log::info!("account: warehouse {which:?} bag {bag:#x} (pinned)");
                    self.bags[which as usize] = Some(bag);
                    if which == Which::Main {
                        crate::ACCOUNT_BAG.store(bag, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        crate::ACCOUNT_MATERIALS.store(bag, std::sync::atomic::Ordering::Relaxed);
                    }
                    self.ensure_bags(emulator, data_dir);
                }
            },
            Some(AccountStep::LoadArray { out, which }) => match out.poll() {
                None => self.pending = Some(AccountStep::LoadArray { out, which }),
                Some(Err(())) | Some(Ok(0)) => {
                    log::warn!("account: could not make a byte[] for the warehouse {which:?} blob — bag stays empty, file left alone");
                    self.load_failed[which as usize] = true;
                    self.after_load(emulator, data_dir, which);
                }
                Some(Ok(arr)) => match self.bag(which).and_then(|bag| java_call(emulator, crate::guest_build::JavaCall::BagDeserialize, JavaThis::Handle(bag), &[wie_backend::JavaArg::Object(arr)])) {
                    Some(out) => self.pending = Some(AccountStep::Load { out, which, arr }),
                    None => {
                        emulator.release_java_object(arr);
                        self.load_failed[which as usize] = true;
                        self.after_load(emulator, data_dir, which);
                    }
                },
            },
            Some(AccountStep::Load { out, which, arr }) => match out.poll() {
                None => self.pending = Some(AccountStep::Load { out, which, arr }),
                Some(done) => {
                    emulator.release_java_object(arr);
                    match done {
                        Ok(_) => {
                            let n = {
                                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                                self.bag(which).and_then(|b| crate::guest_build::h2_bag_head_of(&read, b)).map(|h| h.count)
                            };
                            log::info!("account: warehouse {which:?} loaded ({n:?} record(s))");
                        }
                        Err(()) => {
                            log::warn!("account: the warehouse {which:?} blob did not deserialise — bag stays empty, file left alone");
                            self.load_failed[which as usize] = true;
                        }
                    }
                    self.after_load(emulator, data_dir, which);
                }
            },
            Some(AccountStep::MaterialsItemNew { out }) => match out.poll() {
                None => self.pending = Some(AccountStep::MaterialsItemNew { out }),
                Some(Err(())) | Some(Ok(0)) => {
                    log::warn!("account: the 재료가방 record could not be made — the materials have no door this run");
                    self.loaded = true;
                }
                Some(Ok(rec)) => match self.bag(Which::Main).and_then(|main| java_call(emulator, crate::guest_build::JavaCall::BagInsertRecord, JavaThis::Handle(main), &[wie_backend::JavaArg::Object(rec), wie_backend::JavaArg::Int(1)])) {
                    Some(out) => self.pending = Some(AccountStep::MaterialsItemInsert { out, rec }),
                    None => {
                        emulator.release_java_object(rec);
                        self.loaded = true;
                    }
                },
            },
            Some(AccountStep::MaterialsItemInsert { out, rec }) => match out.poll() {
                None => self.pending = Some(AccountStep::MaterialsItemInsert { out, rec }),
                Some(done) => {
                    emulator.release_java_object(rec);
                    match done {
                        Ok(1..) => {
                            log::info!("account: 재료가방 record {rec:#x} placed in the warehouse");
                            self.dirty = true;
                            // `bq.a(Lch;I)Z` appends: behind whatever the blob already
                            // held. Cell 0 is the rule, so rotate it there.
                            self.front_materials_item_if_needed(emulator);
                        }
                        _ => log::warn!("account: the warehouse did not take the 재료가방 record"),
                    }
                    self.loaded = true;
                    log::info!("account: warehouse ready");
                }
            },
            Some(AccountStep::NewWindow { out, open }) => match out.poll() {
                None => self.pending = Some(AccountStep::NewWindow { out, open }),
                Some(Err(())) => log::warn!("account: window construction FAILED"),
                Some(Ok(0)) => log::warn!("account: window construction returned null"),
                Some(Ok(win)) => {
                    // The parent must still be the top of the chain, and the same one: a
                    // CLR in the tick between the tap and here closed it, and a window hung
                    // on a closed one is a leak the player never sees.
                    let (parent, is_menu) = match open {
                        AccountOpen::Main { menu } => (menu, true),
                        AccountOpen::Materials { parent } => (parent, false),
                    };
                    let still = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_window_chain(&read, h2).and_then(|c| c.last().copied()).is_some_and(|top| top.handle == parent.handle)
                    };
                    if still {
                        let mut ok = true;
                        for (kind, a, bytes) in crate::guest_build::h2_account_open_writes(&parent, win, h2, is_menu) {
                            ok &= java_write(emulator, kind, a, &bytes);
                        }
                        // What the game does after hanging a window: the HUD flags.
                        let _ = java_call(emulator, crate::guest_build::JavaCall::HudDirty, JavaThis::None, &[]);
                        log::info!("account: window {win:#x} hung for {open:?} (writes ok={ok})");
                    } else {
                        log::info!("account: {open:?} — the parent went away before the window was ready — dropped");
                    }
                    // The parent's `b` holds it now (or nothing does): either way the pin is done.
                    emulator.release_java_object(win);
                }
            },
            Some(AccountStep::Serialize { out, which }) => match out.poll() {
                None => self.pending = Some(AccountStep::Serialize { out, which }),
                Some(Err(())) | Some(Ok(0)) => {
                    // ⛔ A blob that was never produced did not reach the disk either: without
                    //   this the OTHER bag's good write cleared `dirty`, and the move was
                    //   never retried (2026-10-02).
                    self.commit_failed = true;
                    log::warn!("account: the warehouse {which:?} did not serialise — not written");
                }
                Some(Ok(arr)) => {
                    let bytes = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_blob_array(&read, arr)
                    };
                    emulator.release_java_object(arr);
                    let wrote = match bytes {
                        Some(bytes) => match crate::vault::write_account(data_dir, which.file(), &bytes) {
                            Ok(()) => {
                                log::info!("account: warehouse {which:?} written ({} bytes)", bytes.len());
                                true
                            }
                            Err(e) => {
                                log::warn!("account: could not write the warehouse {which:?}: {e}");
                                false
                            }
                        },
                        None => {
                            log::warn!("account: the serialised {which:?} blob could not be read back");
                            false
                        }
                    };
                    // ⛔ The move is committed only once NOTHING is still owed and the
                    // bytes actually landed. `Which::Materials => self.dirty = false`
                    // stood here, which cleared the move on the materials bag's turn
                    // whether or not the main bag had been written — and cleared it on a
                    // write that had just failed, so the next save saw a clean state and
                    // never retried.
                    self.commit_failed |= !wrote;
                    if self.commit_owed == [false, false] && !self.commit_failed {
                        self.dirty = false;
                    }
                }
            },
            Some(AccountStep::MoveRowArray { out, popup, len }) => match out.poll() {
                None => self.pending = Some(AccountStep::MoveRowArray { out, popup, len }),
                Some(Err(())) | Some(Ok(0)) => log::warn!("account: the table did not make the label array — no move row on this popup"),
                Some(Ok(arr)) => {
                    let bags = self.popup_bags(emulator, h2);
                    let attach = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_bag_popup_walk(&read, h2, &bags, crate::bag_page_ctx())
                            .filter(|p| p.handle == popup && p.labels_len + 1 == len)
                            .map(|p| crate::guest_build::h2_move_row_attach_writes(&p, arr, len))
                    };
                    match attach {
                        Some(writes) => {
                            let mut ok = true;
                            for (i, (a, bytes)) in writes.iter().enumerate() {
                                let what = if i == 0 { crate::guest_build::JavaWrite::PopupLabels } else { crate::guest_build::JavaWrite::PopupRows };
                                ok &= java_write(emulator, what, *a, bytes);
                            }
                            log::info!("account: move row attached as a new {len}-row label array {arr:#x} on popup {popup:#x} (ok={ok})");
                        }
                        None => log::info!("account: the popup changed while its label array was being made — dropped"),
                    }
                    // The popup references the array now (or nothing does): the pin is done.
                    emulator.release_java_object(arr);
                }
            },
            Some(AccountStep::MoveInsert { out, src, index, rec, to_account }) => match out.poll() {
                None => self.pending = Some(AccountStep::MoveInsert { out, src, index, rec, to_account }),
                Some(Err(())) => log::warn!("account: bq.a(Lch;I)Z FAILED — nothing moved"),
                Some(Ok(0)) => {
                    log::info!("account: the destination refused the record — nothing moved");
                    // The game's own 「빈공간이 없습니다.」, over the popup (or 개수 box) the move
                    // came from: nothing moved, so that modal is still true and the box takes
                    // its place.
                    self.notice_over(emulator, h2, src, NoticeText::BagFull);
                }
                // ⛔ Remove by where the record IS, not by where it was (2026-09-21,
                // review). The insert ran on a spawned task one or more ticks ago and
                // nothing blocked the player's keys in between, so `index` is a claim
                // about the past: a drop through `bq.a(Lch;B)I`, or a stack hitting zero
                // and being removed by `bq.a(BI)V` at a lower index, shifts the Vector
                // under it. `removeElementAt` on a stale index destroys the NEIGHBOUR and
                // leaves the moved record referenced by BOTH Vectors, so it serialises
                // twice. `index` is kept only to say so in the log.
                Some(Ok(_)) => {
                    let at = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_bag_index_of(&read, src, rec)
                    };
                    match at {
                        Some(at) => {
                            if at != index {
                                log::warn!("account: record {rec:#x} slid from index {index} to {at} while the insert was in flight");
                            }
                            match java_call(emulator, crate::guest_build::JavaCall::BagRemoveAt, JavaThis::Handle(src), &[wie_backend::JavaArg::Int(at as i32)]) {
                                Some(out) => self.pending = Some(AccountStep::MoveRemove { out, src, rec, to_account }),
                                None => log::error!("account: record {rec:#x} is now in BOTH bags — the removal call was refused"),
                            }
                        }
                        // Refusing leaves a duplicate; guessing destroys a bystander.
                        // Only one of those is something the player can undo.
                        None => log::error!("account: record {rec:#x} is not in the source bag any more — removing nothing"),
                    }
                }
            },
            // ★ The 개수 box is ready: hang it where the popup is, and let go of the pin
            //   — the window's `j.b` holds it from here (and the popup it displaces is
            //   referenced by nothing else, so the collector takes it).
            Some(AccountStep::QtyNew { out, window, record }) => match out.poll() {
                None => self.pending = Some(AccountStep::QtyNew { out, window, record }),
                Some(Err(())) => log::warn!("account: new aq(Lj;II) FAILED — no 개수 box"),
                Some(Ok(0)) => log::warn!("account: new aq(Lj;II) returned null — no 개수 box"),
                Some(Ok(popup)) => {
                    let bags = self.popup_bags(emulator, h2);
                    let writes = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        // The same window, the same record, and its popup still on top:
                        // a CLR in the tick between the OK and here closed it, and a box
                        // hung on a window the player has left is a box they cannot answer.
                        crate::guest_build::h2_bag_popup_walk(&read, h2, &bags, crate::bag_page_ctx())
                            .filter(|p| p.window.win.handle == window.win.handle && p.record.handle == record)
                            .and_then(|_| crate::guest_build::h2_window_chain(&read, h2))
                            .map(|chain| crate::guest_build::h2_qty_open_writes(&window, popup, &chain))
                    };
                    match writes {
                        Some(writes) => {
                            // ⛔ The FIRST write is the child reference, and it alone decides
                            //   whether the box is on screen. If it lands, the host owns the
                            //   box whatever the repaint flags after it do — forgetting it
                            //   there would leave a box up that only the game can answer.
                            let mut it = writes.iter();
                            let hung = it.next().is_some_and(|(kind, a, bytes)| java_write(emulator, *kind, *a, bytes));
                            for (kind, a, bytes) in it {
                                java_write(emulator, *kind, *a, bytes);
                            }
                            if hung {
                                self.qty = Some(QtyMove { popup, window: window.win.handle, bag: window.bag, record });
                                log::info!("account: 개수 box {popup:#x} hung on window {:#x} over record {record:#x}", window.win.handle);
                            } else {
                                log::warn!("account: the 개수 box could not be hung — dropped");
                            }
                        }
                        None => log::info!("account: the popup went away before the 개수 box was ready — dropped"),
                    }
                    emulator.release_java_object(popup);
                }
            },
            // ★ A PARTIAL move, the game's own three calls: the fresh record, the merge,
            //   the consume. Nothing is taken from the source until the destination says
            //   how many it took — the same order, and for the same reason, as the whole
            //   record move above.
            Some(AccountStep::SplitNew { out, src, rec, dst, n, goal }) => match out.poll() {
                None => self.pending = Some(AccountStep::SplitNew { out, src, rec, dst, n, goal }),
                Some(Err(())) => {
                    log::warn!("account: ch.a(BBZZ) FAILED — nothing moved");
                    self.split_ended(emulator, goal);
                }
                Some(Ok(0)) => {
                    log::warn!("account: ch.a(BBZZ) returned null — nothing moved");
                    self.split_ended(emulator, goal);
                }
                Some(Ok(fresh)) => {
                    let args = [wie_backend::JavaArg::Object(fresh), wie_backend::JavaArg::Int(n)];
                    match java_call(emulator, crate::guest_build::JavaCall::BagAddCount, JavaThis::Handle(dst), &args) {
                        Some(out) => self.pending = Some(AccountStep::SplitAdd { out, src, rec, fresh, n, goal }),
                        None => {
                            log::warn!("account: bq.a(Lch;B)I refused — nothing moved");
                            emulator.release_java_object(fresh);
                            self.split_ended(emulator, goal);
                        }
                    }
                }
            },
            Some(AccountStep::SplitAdd { out, src, rec, fresh, n, goal }) => match out.poll() {
                None => self.pending = Some(AccountStep::SplitAdd { out, src, rec, fresh, n, goal }),
                Some(done) => {
                    // Placed or not, the host is done with the factory's record: in the bag
                    // it lives on the bag's Vector, out of it it is garbage.
                    emulator.release_java_object(fresh);
                    match done {
                        Err(()) => {
                            log::warn!("account: bq.a(Lch;B)I FAILED — nothing moved");
                            self.split_ended(emulator, goal);
                        }
                        // ⛔ `placed` is what the DESTINATION took, which is what the source
                        //   gives up — never `n`. The bag fills stacks to 99 and stops, so a
                        //   part-placed move that consumed `n` would destroy the difference.
                        //   (★ 2026-10-13 fold: what landed on the record itself is in `placed`
                        //   too, and comes back off it — the total never changes.)
                        Ok(placed) => {
                            let placed = placed as i32;
                            if placed <= 0 {
                                log::info!("account: the destination took none of the {n} — nothing moved");
                                match goal {
                                    SplitGoal::Move { .. } => {
                                        self.notice_over(emulator, h2, src, NoticeText::BagFull);
                                    }
                                    SplitGoal::Fold => self.split_ended(emulator, goal),
                                }
                            } else {
                                if placed < n {
                                    log::warn!("account: the destination took {placed} of the {n} asked for");
                                }
                                let at = {
                                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                                    crate::guest_build::h2_bag_index_of(&read, src, rec)
                                };
                                match at {
                                    Some(at) => match java_call(
                                        emulator,
                                        crate::guest_build::JavaCall::BagConsumeAt,
                                        JavaThis::Handle(src),
                                        &[wie_backend::JavaArg::Int(at as i32), wie_backend::JavaArg::Int(placed)],
                                    ) {
                                        Some(out) => self.pending = Some(AccountStep::SplitConsume { out, src, rec, placed, goal }),
                                        None => {
                                            log::error!("account: {placed} of record {rec:#x} are now in BOTH bags — the consume call was refused");
                                            self.split_ended(emulator, goal);
                                        }
                                    },
                                    // Refusing leaves a duplicate; guessing destroys a bystander.
                                    None => {
                                        log::error!("account: record {rec:#x} is not in the source bag any more — consuming nothing");
                                        self.split_ended(emulator, goal);
                                    }
                                }
                            }
                        }
                    }
                }
            },
            Some(AccountStep::SplitConsume { out, src, rec, placed, goal }) => match out.poll() {
                None => self.pending = Some(AccountStep::SplitConsume { out, src, rec, placed, goal }),
                Some(Err(())) => {
                    log::error!("account: bq.a(BI)V FAILED — {placed} of record {rec:#x} are now in BOTH bags");
                    self.split_ended(emulator, goal);
                }
                Some(Ok(_)) => match goal {
                    SplitGoal::Move { to_account } => {
                        log::info!("account: {placed} of record {rec:#x} moved {}", if to_account { "into the warehouse" } else { "into the inventory" });
                        self.finish_move(emulator, h2, src, to_account);
                    }
                    SplitGoal::Fold => {
                        log::info!("account: fold — {placed} of record {rec:#x} into an earlier stack of bag {src:#x}");
                        self.dirty = true;
                        self.fold_next(emulator);
                    }
                },
            },
            // The reset stands whichever way the consume went, and the notice says so.
            Some(AccountStep::ResetConsume { out, refund, bag, note }) => match out.poll() {
                None => self.pending = Some(AccountStep::ResetConsume { out, refund, bag, note }),
                Some(done) => {
                    match done {
                        Err(()) => log::error!("reset: the orb could not be consumed — the reset stands, the item is still there"),
                        Ok(_) => log::info!("reset: done, {refund} point(s) back"),
                    }
                    self.after_reset(emulator, h2, bag, note);
                }
            },
            Some(AccountStep::ElixirConsume { out }) => match out.poll() {
                None => self.pending = Some(AccountStep::ElixirConsume { out }),
                Some(Err(())) => log::error!("elixir plate: the consume failed — HP · SP refilled, the elixir is still there"),
                Some(Ok(_)) => log::info!("elixir plate: one consumed"),
            },
            // ★★ 2026-10-20 상점주인의 혼: the stock is loaded — into the static the buy list reads, then the shop.
            Some(AccountStep::ShopStock { out, menu_fields }) => match out.poll() {
                None => self.pending = Some(AccountStep::ShopStock { out, menu_fields }),
                Some(Err(())) | Some(Ok(0)) => log::warn!("shop orb: z.a() gave no stock — the shop is not opened"),
                Some(Ok(stock)) => {
                    let ok = java_write(emulator, crate::guest_build::JavaWrite::ShopStock, crate::guest_build::Hero2Java::SHOP_STOCK_SLOT, &stock.to_le_bytes());
                    // The static roots it from here; the call's pin is let go.
                    emulator.release_java_object(stock);
                    if ok {
                        log::info!("shop orb: stock {stock:#x} loaded into v.a — opening the shop");
                        self.open_shop(emulator, menu_fields);
                    } else {
                        log::warn!("shop orb: the stock write was refused — the shop is not opened");
                    }
                }
            },
            Some(AccountStep::ShopMenu { out, menu_fields }) => match out.poll() {
                None => self.pending = Some(AccountStep::ShopMenu { out, menu_fields }),
                Some(result) => {
                    if result.is_err() {
                        menu_release_failed(emulator, menu_fields);
                    }
                    self.post_shop_event(emulator);
                }
            },
            Some(AccountStep::ShopEvent { out }) => match out.poll() {
                None => self.pending = Some(AccountStep::ShopEvent { out }),
                Some(Err(())) => log::error!("shop orb: the shop event could not be posted"),
                Some(Ok(_)) => log::info!("shop orb: shop event posted — the game opens its shop"),
            },
            Some(AccountStep::MoveRemove { out, src, rec, to_account }) => match out.poll() {
                None => self.pending = Some(AccountStep::MoveRemove { out, src, rec, to_account }),
                Some(Err(())) => log::error!("account: bq.b(B)V FAILED — record {rec:#x} is now in BOTH bags"),
                Some(Ok(_)) => {
                    log::info!("account: record {rec:#x} moved {}", if to_account { "into the warehouse" } else { "into the inventory" });
                    self.finish_move(emulator, h2, src, to_account);
                }
            },
            // ★ 2026-09-26: the game's message box is built — hang it where it was meant to go,
            //   if that is still true (see `hero2java::h2_notice_target`), and let go of the pin:
            //   the window's `j.b` holds it from here, or nothing does and it is garbage.
            Some(AccountStep::NoticeNew { out, window, replaces, text }) => match out.poll() {
                None => self.pending = Some(AccountStep::NoticeNew { out, window, replaces, text }),
                Some(Err(())) | Some(Ok(0)) => log::warn!("notice: new ba(..) failed — {text:?} not shown"),
                Some(Ok(bx)) => {
                    let writes = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::h2_notice_target(&read, h2, window, replaces).map(|(win, chain)| crate::guest_build::h2_child_open_writes(&win, bx, &chain))
                    };
                    match writes {
                        Some(writes) => {
                            // The child reference first and alone decides it, as for the 개수 box.
                            let mut it = writes.iter();
                            let hung = it.next().is_some_and(|(kind, a, bytes)| java_write(emulator, *kind, *a, bytes));
                            for (kind, a, bytes) in it {
                                java_write(emulator, *kind, *a, bytes);
                            }
                            log::info!("notice: {text:?} in the game's box {bx:#x} over window {window:#x} (replacing {replaces:x?}, hung={hung})");
                        }
                        None => log::info!("notice: window {window:#x} changed before the box was ready — {text:?} dropped"),
                    }
                    emulator.release_java_object(bx);
                }
            },
        }
        // The player CLRed out of the box, or something else closed the window under it.
        self.drop_stale_quantity(emulator, h2);
        // A press that arrived before the warehouse was ready.
        if self.ready() && self.pending.is_none()
            && let Some(open) = self.open_after_load.take()
        {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            let top_ok = crate::guest_build::h2_window_chain(&read, h2).and_then(|c| c.last().copied()).is_some_and(|top| match open {
                AccountOpen::Main { menu } => top.handle == menu.handle,
                AccountOpen::Materials { parent } => top.handle == parent.handle,
            });
            if top_ok {
                self.open(emulator, open);
            }
        }
        // The game saved (every `n<slot>` / `g<slot>` edge counts): the warehouse goes
        // with it. The vault kept the OLD blobs beside the OLD pair before those edges, so
        // what is written now belongs to the pair that was just written.
        let edges = crate::vault::save_edges();
        if edges != self.edges_seen {
            self.edges_seen = edges;
            self.owe_commit();
        }
        self.pump_commit(emulator, data_dir);
        // The title swap follows the warehouse MAIN window: gm row 57 (what `q` titles a
        // non-usable bag with) reads 계정창고 while that window is the top `q`, and 재료
        // 가방 otherwise — the materials window behind it is a 재료가방 and says so.
        let want_swap = self.bag(Which::Main).is_some_and(|main| {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_bag_window(&read, h2, &[main]).is_some_and(|v| v.on_top || crate::guest_build::h2_popup_over(&read, h2, &v))
        });
        match (want_swap, self.title.is_some()) {
            (true, false) => self.swap_title(emulator, h2),
            (false, true) => self.restore_title(emulator, h2),
            _ => {}
        }
        // ★ 2026-10-02: last — after the last save's commit and the title row are settled.
        self.pump_reload(emulator);
    }

    fn swap_title(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) {
        let swap = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_account_title_swap(&read, h2)
        };
        if let Some((addr, original, replacement)) = swap {
            // ⛔ The row's own `byte[]` is about to have NO reference in the guest — the
            //   table slot was the only one — and the host means to put it back later. Pin
            //   it, or the collector frees it and the restore plants a dead handle in a
            //   table the collector walks (S9, 2026-09-21: `USE AFTER FREE … in gc mark`
            //   every collection, then the painter died on it).
            if !emulator.pin_java_object(original) {
                log::warn!("account: row 57's label is not a live byte[] — title left alone");
                return;
            }
            if java_write(emulator, crate::guest_build::JavaWrite::TableRowRef, addr, &replacement.to_le_bytes()) {
                self.title = Some((addr, original));
            } else {
                emulator.release_java_object(original);
            }
        }
    }

    fn restore_title(&mut self, emulator: &mut dyn Emulator, h2: &crate::guest_build::Hero2Java) {
        let Some((addr, original)) = self.title.take() else { return };
        // Only into the SAME table: the menu closing nulls it and the next open reads the
        // asset afresh, so a slot that is no longer row 57's is left alone.
        let same = {
            let read = |a: u32, l: usize| emulator.read_memory(a, l);
            crate::guest_build::h2_gm_row_addr(&read, h2, crate::guest_build::Hero2Java::GM_ROW_MATERIAL_BAG_TITLE) == Some(addr)
        };
        if same {
            let ok = java_write(emulator, crate::guest_build::JavaWrite::TableRowRef, addr, &original.to_le_bytes());
            log::debug!("account: title row put back (ok={ok})");
        }
        // The table references it again (or a new table will read the asset afresh and
        // this one is garbage): either way the host's hold on it ends here, AFTER the
        // write.
        emulator.release_java_object(original);
    }
}

/// What a warehouse that owes a return to the last save does on this poll.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AccountReload {
    /// Not yet: something of the world that just ended is still being finished.
    Wait,
    /// Not this time: the disk is known NOT to hold the last save's warehouse.
    Keep,
    /// Now.
    Drop,
}

/// ⛔ The order is the point (2026-10-02).
///
/// * A JVM call in flight may be the last save's own serialise, or a move's second half —
///   neither may find its bag gone.
/// * A commit still payable is the last SAVE's: it is written first, from these very bags,
///   and only then are they let go. Dropping first would hand back the save BEFORE it.
/// * The title swap pins a table row and is put back by the same poll that sees the window
///   gone; the bags go after it.
/// * A commit that FAILED means the blobs on disk are older than the save the game will
///   load — what moved INTO the warehouse before that save is in neither file. The bags
///   in memory are then the only copy, and going "back to the disk" would lose it.
fn account_reload(call_in_flight: bool, commit_payable: bool, title_swapped: bool, commit_failed: bool) -> AccountReload {
    if call_in_flight || commit_payable || title_swapped {
        AccountReload::Wait
    } else if commit_failed {
        AccountReload::Keep
    } else {
        AccountReload::Drop
    }
}

#[cfg(test)]
mod account_reload_tests {
    use super::{AccountReload, account_reload};

    #[test]
    fn a_quiet_warehouse_goes_back_to_the_last_save() {
        assert_eq!(account_reload(false, false, false, false), AccountReload::Drop);
    }

    #[test]
    fn the_last_saves_commit_is_written_before_the_bags_go() {
        assert_eq!(account_reload(false, true, false, false), AccountReload::Wait);
        // …and a write that then fails keeps the only copy.
        assert_eq!(account_reload(false, false, false, true), AccountReload::Keep);
    }

    #[test]
    fn nothing_in_flight_loses_its_bag() {
        for commit_failed in [false, true] {
            assert_eq!(account_reload(true, false, false, commit_failed), AccountReload::Wait);
            assert_eq!(account_reload(false, false, true, commit_failed), AccountReload::Wait);
            assert_eq!(account_reload(true, true, true, commit_failed), AccountReload::Wait);
        }
    }
}

/// Tell the UI the guest is gone, and say whether the vault can put the character back.
///
/// ★★ THE ONE THING THIS THREAD MUST DO ON ITS WAY OUT (2026-09-20, 잔여 작업 #10).
/// Everything else in the process survives the worker — the window, the compositor, the
/// audio thread and its music — so without this the failure mode is a frozen screen with
/// the soundtrack still going, which reads as a hang and not as a crash. See
/// [`crate::UserEvent::WorkerDied`].
///
/// ⛔ The vault lookup is a directory walk and it happens HERE, on the dying worker,
/// rather than on the UI thread: the UI is about to become the only responsive thing in
/// the process and must not go to the filesystem inside a paint.
fn announce_death(proxy: &EventLoopProxy<UserEvent>, data_dir: &std::path::Path, why: &str) {
    let vault = !crate::vault::list(data_dir).is_empty();
    log::error!("emulator worker STOPPED: {why} (save vault holds {})", if vault { "a backup" } else { "nothing" });
    let _ = proxy.send_event(UserEvent::WorkerDied { vault });
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    proxy: EventLoopProxy<UserEvent>,
    should_redraw: Arc<AtomicBool>,
    input_rx: Receiver<InputEvent>,
    data_dir: PathBuf,
    game_zip: Vec<u8>,
    paused: Arc<AtomicBool>,
    hud_y: crate::HudYShift,
) {
    // ★ Kept for the death notice, which has to work on the path where `run` never
    // returns normally.
    let (death_proxy, death_dir) = (proxy.clone(), data_dir.clone());
    thread::Builder::new()
        .name("wie-emu".into())
        .stack_size(EMU_STACK)
        .spawn(move || {
            // ⛔ A PANIC IS A DEATH TOO, and it is the one the player is most likely to
            // meet: `tick()` returning `Err` is the tidy case, while an `unwrap` deep in
            // the JVM unwinds straight out of this closure. `catch_unwind` here turns
            // both into the same message. (The default hook has already logged the
            // payload and the backtrace by the time this runs.)
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(proxy, should_redraw, input_rx, data_dir, game_zip, paused, hud_y)
            }));
            // ★ EVERY exit, not just the two loud ones. `run` also returns quietly on a
            // bad archive, a build it cannot boot, and the input channel closing — and
            // from the player's side those are the same event: the guest is gone and
            // nothing on screen will change again. (The last one is ordinary teardown,
            // where the event loop is already leaving and drops this.)
            announce_death(&death_proxy, &death_dir, if out.is_err() { "panicked" } else { "returned" });
        })
        .expect("failed to spawn emulator thread");
}

#[allow(clippy::too_many_arguments)]
fn run(
    proxy: EventLoopProxy<UserEvent>,
    should_redraw: Arc<AtomicBool>,
    input_rx: Receiver<InputEvent>,
    data_dir: PathBuf,
    game_zip: Vec<u8>,
    paused: Arc<AtomicBool>,
    hud_y: crate::HudYShift,
) {
    // The screen consumes `proxy` for Paint; keep a clone for the item-HUD channel.
    let item_proxy = proxy.clone();

    // ★★ The archive is opened and the BUILD is decided HERE, before anything is sized.
    // It used to happen further down, next to the emulator it selects - which was fine
    // while every build wanted the same framebuffer. It no longer is: hero2 reserves rows
    // at the bottom of the framebuffer for its own HUD, and how many is a per-build number
    // (`guest_build::hud_reserved_rows`). Deciding the build after `AndroidScreen::new`
    // would have asked for that reservation with the answer still `Unknown`, i.e. zero.
    let files = match extract_zip(&game_zip) {
        Ok(f) => f,
        Err(e) => {
            log::error!("failed to extract game zip: {e:?}");
            return;
        }
    };
    let is_ktf = KtfEmulator::loadable_archive(&files);
    if is_ktf {
        announce_hero2_ktf();
        // ★ 2026-10-16 보스 레이드: the temple with the gate (map 112's bytes, tools_re/raid_gate.py) that `raid_gate` serves
        //   for `/m/m/15.m.gz` while the character holds a GUARDIAN HARD receipt. An asset without it never shows the gate.
        match files.get(crate::guest_build::H2_RAID_GATE_VARIANT) {
            Some(v) => {
                crate::raid_gate::install_variant(v.clone());
                log::info!("raid gate: {} carries the gate-bearing 신전 ({} B)", crate::guest_build::H2_RAID_GATE_VARIANT, v.len());
            }
            None => log::warn!("raid gate: the asset has no {} — the temple's raid gate never shows (regenerate the asset)", crate::guest_build::H2_RAID_GATE_VARIANT),
        }
    }
    // ★ 의상 아바타: which part sheets the jar carries — the only numbers the costume override
    //   may name (`hero2java::AvatarSheets`; a missing sheet throws inside the game's reload).
    //   Read here because `files` is handed to the emulator below.
    let jar = files.iter().find(|(name, _)| name.ends_with(".jar") && !name.contains('/')).map(|(_, jar)| jar);
    let avatar_sheets = match jar {
        Some(jar) => crate::guest_build::h2_avatar_sheets(jar),
        None => crate::guest_build::AvatarSheets::shipped(),
    };
    // ★ A3: the costume catalog (names, bags) the costume window lists from — `hero/costume.txt`,
    //   written by the asset tool; empty on an asset from before A3 (owned costumes then list
    //   by number).
    let costume_catalog = jar.map(|jar| crate::guest_build::h2_costume_catalog(jar)).unwrap_or_default();
    if is_ktf {
        log::info!(
            "avatar: the jar carries costume sheets body {:?} · arms {:?} · legs {:?} · head {:?} · weapon {:?} · off-hand {:?} (beyond the shipped ones) and {} catalog item(s) {:?}",
            avatar_sheets.added(1),
            avatar_sheets.added(2),
            avatar_sheets.added(3),
            avatar_sheets.added(0),
            avatar_sheets.added(6),
            avatar_sheets.added(7),
            costume_catalog.len(),
            costume_catalog.iter().map(|d| format!("{} {}", d.avatar, crate::guest_build::H2_AV_SLOT_KEYS.get(usize::from(d.slot)).unwrap_or(&"?"))).collect::<Vec<_>>()
        );
        // ★ A3c: a one-shot costume grant an adb-placed marker asks for (user, 2026-10-01: 「s25
        //   첫번째 세이브 슬롯에 만들어진 모든 아바타를 넣어줘」) — every item the asset makes into
        //   the named slots' wardrobes, owned and not worn. Here: the jar has just said what the
        //   asset makes, and the game has not run yet, so no guest holds the character.
        // ★ A4: per slot, the items for the class its save holds (a berserker no off-hand).
        let items_for = |class: Option<u8>| crate::guest_build::h2_av_catalog_items(&costume_catalog, &avatar_sheets, class);
        for line in crate::vault::grant_costumes_on_request(&data_dir, std::path::Path::new(crate::config::SDCARD_FILES), &items_for) {
            log::warn!("avatar: {line}");
        }
    }

    let (logical_w, logical_h) = crate::logical();
    let (fb_w, fb_h) = crate::guest_fb();
    log::info!(
        "emulator: guest framebuffer {fb_w}x{fb_h}, presenting {logical_w}x{logical_h} ({} reserved rows dropped)",
        fb_h - logical_h
    );
    let screen = AndroidScreen::new(proxy, should_redraw.clone(), fb_w, fb_h);
    // The synth reads the same `paused` flag to emit silence in lock-step.
    let platform: Box<dyn Platform> = Box::new(AndroidPlatform::new(screen, data_dir.clone(), paused.clone()));

    // ★ Guest PC profiler, off unless `wie_profile.txt` says otherwise — see
    // `config::read_guest_profile`. The callback runs ON the emulator thread inside the
    // interpreter, so it does the cheapest possible thing: fold the batch into a
    // leaf-address histogram and let the 30 s gauge print the top of it. Formatting or
    // logging per batch here would be measuring the profiler.
    // Hoisted out of the `Options` literal: the tick loop needs to know whether samples
    // exist, so it can give each tick its own set for the hitch report.
    let profiling = config::read_guest_profile(&data_dir);
    let options = Options {
        enable_gdbserver: false,
        profile: if profiling {
            Some(Box::new(move |batch: Vec<wie_backend::ProfileSample>| {
                let mut hist = PROFILE_HIST.lock().unwrap_or_else(|e| e.into_inner());
                for s in batch {
                    if let Some(pc) = s.stack.first() {
                        // Thumb bit off: this is an ADDRESS, and it has to match what
                        // `tools_re/h3dis.py` prints.
                        *hist.entry(*pc & !1).or_insert(0) += s.count;
                    }
                }
            }))
        } else {
            None
        },
    };

    let mut emulator: Box<dyn Emulator> = if is_ktf {
        match KtfEmulator::from_archive(platform, files, options) {
            Ok(e) => Box::new(e),
            Err(e) => {
                log::error!("KTF load failed: {e:?}");
                return;
            }
        }
    } else if LgtEmulator::loadable_archive(&files) {
        crate::guest_build::set_lgt();
        // Not a downgrade of the byte-validation guards — see guest_build.rs for why
        // this has to be airtight now that LgtEmulator can run host-initiated calls.
        // The split is between DATA (ported: the CHAR record is a `.bss` static and
        // every container hop is one deref shorter) and guest CODE addresses (not
        // ported: none of the KTF routine addresses exist in this compile).
        // ★ The per-item state is READ OUT OF THE TABLES, not spelled out in the string
        // (2026-08-11). The hand-written list had gone stale in exactly the way §3.6's
        // per-item discipline invites: it still said "HUD skip, EXP … inactive" three
        // rounds after both had shipped their measured LGT addresses, so the one line a
        // device session trusts to say what is live was lying about two features.
        log::warn!(
            "{} build: data-side features active (CHAR gauges, quickbar scan, window/state gating, minimap, full-restore); containers {}; per-item guest-code patches: hud_bottom={} exp_divisor={} equip_relief={} band_x={} target_box={}; quickbar writes {} (skill_bind {}); cash-buff trigger (popup labels) {}; save chain (host quick-save + autosave + backup upload) {}; auto-loot {}; still gated (guest_code_addresses_valid=false): hud_y origin, band height; blit565_lgt + blend565_lgt + blit_lut565_lgt install via binary_patches",
            crate::guest_build::name(),
            match crate::guest_build::container_data_off() {
                Some(off) => format!("= *(handle) + {off:#x}"),
                None => "= direct pointers".to_string(),
            },
            crate::guest_build::hud_bottom_patch().map_or("-".to_string(), |(s, _, _)| format!("{s:#x}")),
            crate::guest_build::exp_divisor_site().map_or("-".to_string(), |s| format!("{s:#x}")),
            match crate::guest_build::equip_relief_site() {
                Some(crate::guest_build::EquipReliefSite::MoveHalfword { site }) => format!("{site:#x} (halfword)"),
                Some(crate::guest_build::EquipReliefSite::LevelBias { site }) => format!("{site:#x} (level-bias pair)"),
                None => "-".to_string(),
            },
            // The two HUD widgets the player asked to be moved out from under our own
            // chrome (2026-08-11): the game's buff band goes right of our gauges and its
            // target-monster box left of our minimap. Both are lists of one-byte
            // immediates, so "how many did this build supply" is the whole status.
            match crate::guest_build::band_x_sites().len() {
                0 => "-".to_string(),
                n => format!("{n} sites"),
            },
            match crate::guest_build::target_box_x_sites().len() {
                0 => "-".to_string(),
                n => format!("{n} sites"),
            },
            if crate::guest_build::quickbar_writes_ok() { "open" } else { "gated" },
            crate::guest_build::skill_bind_fn().map_or("unlocated -> raw ref writes".to_string(), |f| format!("{f:#x}")),
            // Without this the EXP and relief patches can never be ASKED for, however
            // right their sites are: `cash_use_pending` is the only thing that grants a
            // buff, and it reads the highlighted popup row's label through this array.
            match crate::guest_build::popup_handles() {
                Some(crate::guest_build::PopupHandles::Pointer { global }) => format!("via slot {global:#x}"),
                Some(crate::guest_build::PopupHandles::Static(base)) => format!("via .bss array {base:#x}"),
                None => "NOT LOCATED — no cash buff can start".to_string(),
            },
            // The control stays on screen either way (see `app::quicksave_zone`); this
            // is here so a device report says WHICH situation a refusal came from.
            if crate::guest_build::save_chain_supported() { "ACTIVE" } else { "guest entry points unported — taps/autosave cannot complete (MIGRATION_LGT.md §4-3)" },
            // ★ Read from the table for the same reason as everything above it. This line
            // said "still gated: autoloot" for six weeks while the feature was dead on the
            // shipped build, and the server had delisted the cash item that does the same
            // job on the strength of that claim. Never hand-write a capability again.
            match crate::guest_build::autoloot_flag() {
                // 50차: the flag is an ARRAY and the save slot picks the element, so the
                // line has to say that — a fixed address here is precisely the bug that
                // made this feature dead for every character not in slot 1.
                Some(f) => match f.slot_off {
                    Some(slot) => format!("forced ON (CHAR+{:#x} + saveSlot@CHAR+{slot:#x} <- {:#04x})", f.base_off, f.value),
                    None => format!("forced ON (CHAR+{:#x} <- {:#04x})", f.base_off, f.value),
                },
                None => "unlocated — the 오토루팅 cash item is the only way".to_string(),
            },
        );
        match LgtEmulator::from_archive(platform, files, options) {
            Ok(e) => Box::new(e),
            Err(e) => {
                log::error!("LGT load failed: {e:?}");
                return;
            }
        }
    } else if SktEmulator::loadable_archive(&files) {
        crate::guest_build::set_skt();
        log::warn!("SKT build: KTF guest-address features inactive");
        match SktEmulator::from_archive(platform, files) {
            Ok(e) => Box::new(e),
            Err(e) => {
                log::error!("SKT load failed: {e:?}");
                return;
            }
        }
    } else {
        // J2ME archives are jad/jar based, not zip; treat unknown zip as fatal.
        log::error!("unknown archive format (not KTF/LGT/SKT)");
        let _ = J2MEEmulator::from_jar; // keep the import meaningful for future jar support
        return;
    };

    log::info!("emulator loaded; entering tick loop");

    let mut held: HashMap<KeyCode, Instant> = HashMap::new();
    // 2026-09-24: the walk edges the guest can lose (a release it never looked at, a
    // re-press it refused). See `WalkGuard`.
    let mut walk_guard = WalkGuard::default();
    // ★★ 2026-10-16: a beast discard's 예, held until its refund has run (`BeastDiscard`).
    let mut beast_discard: Option<BeastDiscard> = None;
    // Input the pacing wait at the end of the loop took off the channel — a VecDeque so
    // it keeps arrival order (this is at most a handful of events per frame).
    let mut pending_input: std::collections::VecDeque<InputEvent> = std::collections::VecDeque::new();
    let mut frame: u64 = 0;
    // The item-use drive in flight, if any (hero2 ⑧). See `step_use_drive`.
    let mut drive: Option<UseDrive> = None;
    // Whether the one-shot auto-loot capability line has been logged for this run.
    let mut autoloot_notice_done = false;
    // Highest `stack_rss_kb` seen, so the log is a staircase and not a stream.
    let mut stack_rss_peak: u64 = 0;
    // Last HUD state pushed; only re-send on change.
    let mut last_slots: Vec<QuickSlot> = Vec::new();
    // The minimap as last published. Rendered by the host from the guest's own map
    // arrays, so it is simply whatever the current map looks like — there is no
    // acquisition state to track any more.
    let mut last_minimap: Option<crate::Minimap> = None;
    // hero2's minimap exits (blue bars) and treasure chests (green boxes), read from the
    // map's trigger tables once per map — see `hero2java::H2TriggerCache`. (Whether a chest
    // is still closed is read from the event flags on every render, not cached.)
    let mut mm_triggers = crate::guest_build::H2TriggerCache::default();
    // The stage's attribute-grid handle the last minimap was rendered from. The marker
    // poll compares it every other frame and re-renders at once when a door changed the
    // map (`hero2java::h2_stage_grid`).
    let mut last_mm_grid: Option<u32> = None;
    let mut last_stats: Stats = Stats::default();
    let mut last_field = false;
    let mut last_cutscene = false;
    let mut last_choice = false;
    let mut last_map_open = false;
    let mut last_shell = false;
    let mut last_bottom_overlay = false;
    let mut last_can_save = true;
    // The minimap marker as last published — see `MM_MARKER_EVERY`.
    let mut last_marker: Option<(f32, f32)> = None;
    let mut last_stage_map: Option<u32> = None;
    // …and its monsters as last published — see `MM_MOBS_EVERY`.
    let mut last_mobs: Vec<(f32, f32)> = Vec::new();
    // ★ A3b: the portrait's head — the parts table's head array and the class animation it
    // was read under, the animation's bytes (read again only when its handle changes), and
    // the head as last published (`UserEvent::Portrait`).
    let mut portrait_key: Option<(u32, u32)> = None;
    let mut portrait_blob: Option<(u32, Vec<u8>)> = None;
    let mut last_portrait: Option<crate::guest_build::H2Image> = None;
    // ★ 2026-10-07 (2): the rage gauge as last published (`UserEvent::BuffGauges`) — ★ 2026-10-13 D4 and 추종's.
    let mut last_buff_gauges: Vec<(u8, u8, u8)> = Vec::new();
    // ★ 2026-10-09: the awakenings' voices — the edges that cue them (`hero2java::H2VoiceWatch`).
    let mut voice_watch = crate::guest_build::H2VoiceWatch::default();
    // ★★ 2026-10-24 the thunder — 어둠심판's strikes · 벽력일섬's bolts (`hero2java::H2ThunderWatch`).
    let mut thunder_watch = crate::guest_build::H2ThunderWatch::default();
    // ★ 2026-10-05: the skill buffs as last published (`UserEvent::SkillBuffs`).
    let mut last_skill_buffs: Vec<crate::guest_build::H2BuffShow> = Vec::new();
    /// ★★ OFF since 2026-08-09, and this is a deliberate feature stand-down, not a tidy-up.
    ///
    /// 27차 hid the SAVE button and the minimap wherever it believed saving was refused,
    /// reading `(s8)*(CHAR+0xa280)`. That byte is the shared list widget's CURSOR ROW (see
    /// `crate::save::AREA_CAN_SAVE_OFF` for the disassembly), so the feature fired on an
    /// ordinary field and stayed fired — device-measured 2026-08-09, thirteen minutes with
    /// both controls gone, and the player found the mechanism before the RE did: opening any
    /// menu and closing it brought them back, because opening a menu is the only thing that
    /// writes that cursor to 0.
    ///
    /// The gate is now the game's REAL one (`items::area_can_save`). But its value comes out
    /// of each map's own data file, so `client.bin` cannot say which maps — if any — actually
    /// ship a refusal. Hiding UI on an unmeasured signal is exactly the mistake that produced
    /// this bug, so the signal is READ and LOGGED this round and hides nothing. Turn this on
    /// once a device log shows `area gate: can_save=Some(false)` in a boss room and
    /// `Some(true)` on the field; if `Some(false)` never appears anywhere, delete the feature.
    ///
    /// Nothing else changes: `save_begin` still asks the game, so a genuine refusal still
    /// answers with the 저장불가 toast instead of a silent no-op.
    const HIDE_UI_ON_NO_SAVE: bool = false;
    /// Consecutive "no-save" samples before the UI believes it. Was 24 (~2 s) to paper over
    /// the wrong byte's flicker; the real per-area byte only changes when the area does, so
    /// two samples are enough to ride out a map transition.
    const NO_SAVE_DEBOUNCE: u32 = 2;
    let mut no_save_run: u32 = 0;
    /// Field polls in a row (5 frames each) that must read a bag with none of an item
    /// plate's type before the plate is cleared — long enough to outlast a bag caught
    /// mid-load, short enough that the last potion's plate goes a moment after it.
    const ITEM_GONE_POLLS: u8 = 3;
    // ★ 2026-09-29: the same run for a SKILL plate whose value the character reads as not
    //   having (`Pages::unlearned_plates`) — three straight polls, the bar's own seats
    //   excepted.
    const SKILL_GONE_POLLS: u8 = 3;
    // Cash-buff status lines as last published (menu-mode HUD, ~minute granularity).
    let mut last_buff_lines: Vec<String> = Vec::new();
    // Whether the UI has been told the HUD state at least once (see below).
    let mut hud_published = false;
    // The game's rewritten context popup, as last published.
    let mut last_popup: Option<items::PopupView> = None;
    // The UI is highlighting the plates, waiting for one to be tapped.
    let mut picking = false;
    // Host-owned quickbar pages. Page 0 IS the guest's bar, so a fresh/empty store
    // behaves exactly as before the feature existed.
    let mut pages = config::read_quickpages(&data_dir);
    // Which plate sits in which of the guest's four slots — derived from the live bar on
    // every world entry, never persisted (`quickpage::Window`).
    let mut window = quickpage::Window::new();
    // A capture changed the store; written out after the HUD sample it happened in.
    let mut pages_dirty = false;
    // Per item TYPE: how many polls in a row the bag has held none of it (item plates leave
    // the bar at `ITEM_GONE_POLLS`).
    let mut item_gone = [0u8; quickpage::ITEM_KIND_MAX as usize];
    // Per quickbar VALUE (1..=13): polls in a row an unseated plate has held it while the
    // character read as not having it. Reset on every world entry.
    let mut skill_gone = [0u8; quickpage::BEAST_MAX as usize + 1];
    // ★ 2026-09-29: `vault::save_edges()` as of the last look — a change is the game
    //   writing its save files, and the plate rows are committed with them
    //   (`Pages::commit`), the way the warehouse blobs are. Checked every frame.
    let mut pages_edges_seen = crate::vault::save_edges();
    let _ = item_proxy.send_event(UserEvent::QuickPage(pages.current() as u8));
    // Host-owned cash-shop buffs (EXP x2/x4, equip level relief). Persisted as
    // absolute UNIX-ms expiries, so they run down while the app is closed.
    //
    // ★ With no buff live this costs the guest NOTHING: both patch helpers ask for the
    // ORIGINAL halfword, find it already there, and return `None`. So a build carrying
    // this feature is byte-identical in the guest until a player actually uses one of
    // the three items — which matters while we are still bisecting a crash.
    // Host-driven save (quick-save button + autosave timer). See `save.rs`.
    let mut saving = SaveState::new();
    let mut backing_up = BackupState::new(data_dir.clone());
    // A skill-plate tap being verified against guest state — see `SkillWatch`.
    let mut skill_watch: Option<SkillWatch> = None;
    // ★ #13: inventory paging — installed at the first patch pass, polled with the HUD.
    // ★ #18: the skill-point curve. `done` latches only on SUCCESS — a pass that finds
    //   the sites unreadable must be free to try again, because the very first patch pass
    //   is the only thing standing between "the image is not mapped yet" and "the site
    //   moved" and this code cannot tell those apart. `said` is what keeps the second
    //   case quiet: the patch pass runs every 30 frames, so an ungated warning would
    //   repeat twice a second for the rest of the process (the log-spam of 2026-09-18).
    let mut skill_curve = HalfwordPatch::new();
    let mut skill_hotkey_badges = HalfwordPatch::new();
    let mut combo_sp = HalfwordPatch::new();
    let mut strength_sp = HalfwordPatch::new();
    let mut counter_stance = HalfwordPatch::new();
    let mut counter_length = HalfwordPatch::new();
    let mut buff_cd = HalfwordPatch::new();
    let mut skill_cd = HalfwordPatch::new();
    let mut card_drop = HalfwordPatch::new();
    let mut honor_penalty = HalfwordPatch::new();
    let mut item_refresh = HalfwordPatch::new();
    let mut title_menu = HalfwordPatch::new();
    let mut skill_cancel = HalfwordPatch::new();
    let mut qty_wrap = HalfwordPatch::new();
    let mut craft_mark = HalfwordPatch::new();
    let mut craft_worn = HalfwordPatch::new();
    let mut roar_scan = HalfwordPatch::new();
    let mut rogue_sp = HalfwordPatch::new();
    let mut agility = HalfwordPatch::new();
    let mut wave_range = HalfwordPatch::new();
    let mut knight_str = HalfwordPatch::new(); // ★ 2026-10-20 나이트 공격력 힘 × 0.85
    let mut active_seven = HalfwordPatch::new();
    let mut roar_cd = HalfwordPatch::new();
    let mut blessing = HalfwordPatch::new();
    let mut stance_invuln = HalfwordPatch::new();
    let mut iron_wall = HalfwordPatch::new();
    let mut beast_recovery = HalfwordPatch::new();
    let mut beast_int = HalfwordPatch::new();
    let mut beast_sp = HalfwordPatch::new();
    let mut beast_cd = HalfwordPatch::new();
    let mut beast_power = HalfwordPatch::new();
    let mut seal_top = HalfwordPatch::new();
    let mut bodyslam_lock = HalfwordPatch::new();
    let mut damage_cap = HalfwordPatch::new();
    let mut hp_int = HalfwordPatch::new();
    let mut combat = Combat::new();
    let mut venom = Venom::new();
    let mut shop_sell = ShopSell::new();
    let mut boss_crit = BossCrit::new();
    let mut boss_pierce = BossPierce::new();
    let mut grade_line = GradeLine::new();
    let mut hell_drop = HellDrop::new();
    // ★ 2026-10-08 가디언: the mode tables (every frame from the first, before SaveSlots) and the template block.
    let mut mode_tables = ModeTables::new();
    let mut guardian = Guardian::new();
    let mut gear = Gear::new();
    let mut accessory = Accessory::new();
    let mut clear_hook = ClearHook::new();
    // ★★ 2026-10-22 가디언 레벨 (GL): the level after 99. See `GuardianLevel`.
    let mut guardian_level = GuardianLevel::new();
    let mut cd_clock = CooldownClock::new();
    let mut item_icon = ItemIcon::new(); // ★★ 2026-10-16: the elixir's own icon cell
    // ★ 2026-10-12: the slot screen opens on the save slot last played (`wie_save_last.txt`). See `SlotFocus`.
    let mut slot_focus = SlotFocus::new(&data_dir);
    let mut buff_refresh = BuffRefresh::new();
    let mut rogue = Rogue::new();
    let mut teleport_gate = TeleportGate::new();
    let mut demon = Demon::new();
    let mut sky_draw = SkyDraw::new();
    // ★ 2026-10-05 일풍격: the rogue's seventh active — after the accessory block (it chains to it).
    let mut ilpung = Ilpung::new();
    // ★ 2026-10-07: the berserker's 분노태세 toggle numbers and 「도발」 — after SkillPower · accessory · combat · 일풍격.
    let mut berserker = Berserker::new();
    // ★ 의상 아바타 (2026-09-30): the parts-loader override. The WORN costume comes from the
    //   character's own saved state (A2 — `avatar_book`, bound to the character's `/g`) and
    //   from nothing else (2026-10-01: the global test file is gone).
    let mut avatar = Avatar::new();
    let avatar_io = AppAvatarIo { data_dir: data_dir.clone() };
    let mut avatar_book = crate::avatar_state::AvatarBook::new(&avatar_io);
    // ★ A3: the EQUIPMENT window's avatar side (the character, the avatar slot, the costume window).
    let mut equip_avatar = EquipAvatar::new(costume_catalog);
    let mut skill_power = SkillPower::new();
    let mut bag_paging = BagPaging::new();
    // ★ 경험치 배율: the EXP redirect, installed on the first poll that wants it.
    let mut exp_rate = ExpRate::new();
    let mut drop_rate = DropRate::default();
    // ★ 난이도 (2026-09-29): every monster's HP / attack words at the setting's multiplier.
    let mut difficulty = Difficulty::default();
    // ★ 세이브 슬롯 5: ag's file-name tables and the slot screen's code, during the logo.
    let mut save_slots = SaveSlots::new();
    // ★ 2026-09-27: did the character leaving the shell come out of the class pick (a new
    //   game)? Then its slot's old plate rows go — see `quickpage::NewGameWatch`.
    let mut new_game_watch = quickpage::NewGameWatch::default();
    let mut drop_life = DropLife::default();
    let mut auto_loot = AutoLoot::default();
    let mut options = OptionsExt::default();
    // ★★ 2026-10-28: 환경설정 맨 밑 세 행 — 세이브내보내기 · 불러오기 · 되돌리기 (`save_xfer_host.rs`).
    let mut save_xfer = SaveXfer::default();
    let mut combo_repair = ComboRepair::new();
    let mut iaido_unlock = IaidoUnlock::new();
    let mut ilpung_unlock = IlpungUnlock::new();
    let mut born_skill_repair = BornSkillRepair::new();
    let mut bodyslam_retire = BodySlamRetire::new();
    // ★ 부활의 구 (2026-09-25): the box offered when the character falls, and the hold on
    //   the guest while it is up.
    let mut revive = Revive::default();
    let mut boss_refill = BossRefill::default();
    let mut boss_raid = BossRaid::new();
    // The key-up still owed for a LEFT/RIGHT whose key-down became a page crossing.
    let mut bag_swallow_up: Option<KeyCode> = None;
    // ★★ 2026-10-20: the key-up owed for an OK the craft guard took (`craft_full_guard`).
    let mut craft_swallow_up: Option<KeyCode> = None;
    // ★★ 2026-10-20: the host's lines in the game's notice box (`host_notice`).
    let mut host_notices = HostNotices::default();
    // ★ #13 step 5: the ACCOUNT warehouse — the bag, the window in flight, the title swap.
    let mut account = AccountState::new();
    let mut refine_protection = RefineProtection::default();
    // ★★ 2026-10-16: the inventory's 정렬 button and the sell window's 일괄판매 (`bag_tools_host.rs`).
    let mut bag_tools = BagTools::default();
    let mut buffs = config::read_buffs(&data_dir);
    // 천상의 영약 level-ups consumed but not yet delivered (they fire on the field).
    let mut levelups = config::read_pending_levelups(&data_dir);
    if levelups > 0 {
        log::info!("level elixir: {levelups} pending level-up(s) restored from disk");
    }
    // ★ A rollback note that survived means the app died between a cash item's effect
    //   and the save that would have paid for it — so the guest save about to be loaded
    //   is the PRE-USE one, item and all. Put the host back where that save left it:
    //   both sides then roll back together instead of the player keeping the effect
    //   AND the item. See `buffs::CashBaseline`.
    if let Some(base) = config::read_cash_baseline(&data_dir) {
        if base.buffs != buffs || base.levelups != levelups {
            log::warn!(
                "cash rollback: a save never landed after a cash-item use — reverting host state \
                 (exp_x2 {} -> {}, exp_x4 {} -> {}, relief {} -> {}, levelups {levelups} -> {})",
                buffs.exp_x2_until,
                base.buffs.exp_x2_until,
                buffs.exp_x4_until,
                base.buffs.exp_x4_until,
                buffs.relief_until,
                base.buffs.relief_until,
                base.levelups
            );
            buffs = base.buffs;
            levelups = base.levelups;
            config::write_buffs(&data_dir, &buffs);
            config::write_pending_levelups(&data_dir, levelups);
        }
        config::clear_cash_baseline(&data_dir);
    }
    {
        let now = now_ms();
        if buffs.any_live(now) {
            log::info!(
                "cash buffs live: exp x{}, relief {} levels, {} min left",
                buffs.exp_multiplier(now),
                buffs.relief_levels(now),
                buffs.minutes_left(now)
            );
        } else {
            log::info!("cash buffs: none live (the guest is left exactly as shipped)");
        }
    }

    // Wall clock for the throughput gauge below. Counted from here, not from process
    // start, so boot and asset extraction do not drag the average down.
    // Measurement switch for the host sprite blitter — see `config::read_blit_hle`.
    let blit_hle = config::read_blit_hle(&data_dir);
    wie_util::stats::set_blit_hle(blit_hle);
    log::warn!("host sprite blitter (HLE): {}", if blit_hle { "ON" } else { "OFF (wie_blit_hle.txt)" });

    // ★ Guest frame-timer ceiling — see `config::read_frame_timer_cap`. This is the one
    //   term §2.24 could not remove from the modal screens (the game asks for 244 ms
    //   there and 50 on the field), and it is a behaviour change, so it ships as a
    //   switch that can be flipped without reinstalling.
    let timer_cap = config::read_frame_timer_cap(&data_dir);
    wie_util::stats::set_timer_clamp_ms(timer_cap);
    log::warn!(
        "guest frame-timer cap: {}",
        if timer_cap == 0 {
            "OFF (honour the guest exactly)".to_string()
        } else {
            format!("{timer_cap} ms (wie_frame_timer_cap.txt)")
        }
    );


    let (mut gauge_at, mut gauge_instr, mut gauge_frames) = (Instant::now(), 0u64, 0u64);
    // ★ WHERE DOES THE WORKER'S WALL CLOCK GO (2026-08-09). On the low-spec target the
    // guest ran at 12.3 fps while `wie-emu` sat at 58 % of a core — half the target
    // framerate with 40 % of the thread idle, which no amount of interpreter tuning can
    // explain. These say whether the loop is WORK-bound (ticks long, budget capped) or
    // PACED (ticks short, sleeping out the rest of every 16 ms).
    let (mut gauge_tick_us, mut gauge_ticks, mut gauge_short_ticks) = (0u64, 0u64, 0u64);
    let (mut gauge_capped, mut gauge_sleep_us, mut gauge_loops) = (0u64, 0u64, 0u64);
    // Last hitch report, so a stall that repeats every frame cannot flood logcat.
    let mut hitch_at: Option<Instant> = None;
    // The world map is up: the GAME must not run, the way a menu window stops the field —
    // ★ 2026-10-21 (user: *"월드맵에서 배경음이 멈춰있는데 메뉴오픈했을때처럼 배경음은 계속
    // 흘러갔으면"*): the guest's own tasks are PARKED (`Emulator::park_guest`) and the
    // emulator keeps ticking, so the music player — a host task — plays on. A tick stop
    // (what this was) silenced it: the SMAF/MA-2 player sleeps on the executor's clock.
    let mut world_hold = false;

    loop {
        // 0) Backgrounded / screen locked: do not tick (so the guest generates no
        //    new audio and burns no battery) — the synth emits silence in parallel.
        //    Still drain+discard input so the channel can't back up and we notice
        //    teardown; clear held keys so nothing auto-repeats on resume.
        if paused.load(Ordering::Relaxed) {
            loop {
                match input_rx.try_recv() {
                    Ok(_) => {}
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        log::info!("input channel disconnected while paused; stopping emulator");
                        return;
                    }
                }
            }
            held.clear();
            // ⛔ The key-up latches go with the held keys (2026-09-21, review). Both are
            // debts owed by a key-up that this drain has just thrown away, so leaving
            // them armed makes the NEXT genuine release the one that gets eaten: the
            // guest never sees that `Keyup`, the key stays in `held`, and the 100 ms
            // auto-repeat then fires it at 10 Hz — an unstoppable repeating confirm in a
            // shop or dialogue, or a character that will not stop walking.
            account.swallow_ok_up = false;
            bag_swallow_up = None;
            // …and the 부활의 구 box's: a press it took whose release this drain threw away
            // would otherwise eat the next genuine release of that key after the box closed.
            revive.swallow_up.clear();
            refine_protection.dialog.swallow_up.clear();
            thread::sleep(Duration::from_millis(50));
            continue;
        }

        let frame_start = Instant::now();

        // 1) Redraw if the guest asked for one. ★ Not while the 부활의 구 box holds the
        //    guest: the request would only queue behind the hold, so it is left pending.
        if !revive.holding() && !refine_protection.dialog.holding() && !save_xfer.holding() && should_redraw.swap(false, Ordering::SeqCst) {
            emulator.handle_event(Event::Redraw);
        }

        // 1b) The net layer dropped a connection that Android killed while we were in
        //     the background. Tell the player straight away rather than letting them
        //     discover it by waiting out a loading screen (player request 2026-08-15).
        //     The flag is raised on the emulator thread by `network::check_resume`, and
        //     this is the nearest place with the event proxy.
        if crate::network::take_drop_notice() {
            host_notice("네트워크 연결이 끊어졌습니다");
            show_disconnect_modal(&mut *emulator);
        }

        // 2) Drain input from the UI thread straight to the guest. (It used to be
        //    stashed while the hidden minimap refresh held the guest's map window
        //    open; there is no such window any more.)
        //    `pending_input` is what the interruptible nap at the end of the previous
        //    iteration picked up (see step 5); it is applied first so events keep their
        //    order.
        loop {
            let got = match pending_input.pop_front() {
                Some(ev) => Ok(ev),
                None => input_rx.try_recv(),
            };
            match got {
                Ok(ev) => {
                    // ★ 부활의 구 (2026-09-25): while the box holds the guest it is modal — a
                    //   stick turn is a key for the box (its release half still goes on to
                    //   `held`), a key or a tap goes to it, SAVE is refused with a word, and
                    //   everything else is dropped. The release of a key whose press the box
                    //   took is owed to it even after it has closed (`Revive::key`).
                    let ev = match ev {
                        InputEvent::Turn { from, to } if refine_protection.busy(&*emulator) => {
                            refine_protection.key(&mut *emulator, &item_proxy, to, true);
                            InputEvent::Key { code: from, down: false }
                        }
                        other => other,
                    };
                    match &ev {
                        InputEvent::Key { code, down } if refine_protection.key(&mut *emulator, &item_proxy, *code, *down) => continue,
                        InputEvent::MenuTap { fx, fy } if refine_protection.tap(&mut *emulator, &item_proxy, *fx, *fy) => continue,
                        InputEvent::Key { down: false, .. } | InputEvent::QuickHidden { .. } => {},
                        _ if refine_protection.busy(&*emulator) => continue,
                        _ => {},
                    }
                    let ev = match ev {
                        InputEvent::Turn { from, to } if revive.holding() => {
                            revive.key(&mut *emulator, &item_proxy, to, true);
                            InputEvent::Key { code: from, down: false }
                        }
                        other => other,
                    };
                    match &ev {
                        InputEvent::Key { code, down } if revive.key(&mut *emulator, &item_proxy, *code, *down) => continue,
                        InputEvent::MenuTap { fx, fy } if revive.tap(&mut *emulator, &item_proxy, *fx, *fy) => continue,
                        InputEvent::QuickSave if revive.holding() => {
                            log::info!("save: refused — the character is down and the 부활의 구 box is up");
                            host_notice("저장불가");
                            continue;
                        }
                        InputEvent::Key { down: false, .. } | InputEvent::QuickHidden { .. } => {}
                        _ if revive.holding() => {
                            log::debug!("revive: dropped a UI event while the box is up");
                            continue;
                        }
                        _ => {}
                    }
                    // ★★ 2026-10-28 세이브 행: a question holds the guest and takes every key and tap (a stick turn is
                    //   a key for it, its release half goes on); while 세이브내보내기 waits for its save, the
                    //   player's key-downs and taps are dropped (the window the answer comes to must stay).
                    let ev = match ev {
                        InputEvent::Turn { from, to } if save_xfer.holding() => {
                            save_xfer.key(&mut *emulator, &data_dir, &item_proxy, to, true);
                            InputEvent::Key { code: from, down: false }
                        }
                        // The press half is the export's to drop; the release must still reach `held`.
                        InputEvent::Turn { from, .. } if save_xfer.owns_input() => InputEvent::Key { code: from, down: false },
                        other => other,
                    };
                    match &ev {
                        InputEvent::Key { code, down } if save_xfer.key(&mut *emulator, &data_dir, &item_proxy, *code, *down) => continue,
                        InputEvent::MenuTap { fx, fy } if save_xfer.tap(&mut *emulator, &data_dir, &item_proxy, *fx, *fy) => continue,
                        InputEvent::Key { down: false, .. } | InputEvent::QuickHidden { .. } => {}
                        _ if save_xfer.holding() || save_xfer.owns_input() => {
                            log::debug!("save xfer: dropped a UI event while a question is up or the export saves");
                            continue;
                        }
                        _ => {}
                    }
                    // ★★ 2026-10-20: a crafting OK into a full bag — taken, with the game's own box (`craft_full_guard`).
                    //   Its key-up is dropped too, so the game sees neither half.
                    if let InputEvent::Key { code: code @ (KeyCode::OK | KeyCode::NUM5), down } = ev {
                        if !down && craft_swallow_up == Some(code) {
                            craft_swallow_up = None;
                            continue;
                        }
                        if down
                            && let Some(h2) = crate::guest_build::hero2_java()
                            && craft_full_guard(&mut *emulator, &mut account, &h2)
                        {
                            craft_swallow_up = Some(code);
                            continue;
                        }
                    }
                    // ★ 2026-09-24: a stick turn becomes the new press alone on the field,
                    //   and the old release-then-press pair anywhere else (`apply_turn`).
                    let Some(ev) = apply_turn(&*emulator, &mut held, &mut pending_input, &drive, ev) else {
                        continue;
                    };
                    // ★ While the item-use drive owns the screen (hero2 ⑧) the player's
                    //   key-DOWNS are dropped: the guest is in a menu the player cannot
                    //   see, and a walk key would move its cursor. Key-ups still pass
                    //   (a finger lifted off the stick must release its key), and so does
                    //   the editor's hidden-plate mask (no guest side).
                    let owned = drive.as_ref().is_some_and(|d| d.owns);
                    if owned && !matches!(ev, InputEvent::Key { down: false, .. } | InputEvent::QuickHidden { .. }) {
                        log::debug!("item drive: dropped a UI event while the drive owns the screen");
                        continue;
                    }
                    // ★ 2026-09-24: the game's 환경설정 with the host's three rows — UP/DOWN
                    //   and a host row's keys and taps are the host's (`OptionsExt`).
                    match &ev {
                        InputEvent::Key { code, down } if options.key(&mut *emulator, &data_dir, &item_proxy, *code, *down) => {
                            if let Some(row) = options.take_action() {
                                save_xfer.press(&mut *emulator, &data_dir, &item_proxy, &mut saving, row);
                            }
                            continue;
                        }
                        InputEvent::MenuTap { fx, fy } if options.tap(&mut *emulator, &data_dir, &item_proxy, *fx, *fy) => {
                            if let Some(row) = options.take_action() {
                                save_xfer.press(&mut *emulator, &data_dir, &item_proxy, &mut saving, row);
                            }
                            continue;
                        }
                        _ => {}
                    }
                    // ★ 의상 아바타 A3: the EQUIPMENT window's avatar side — the slot's focus and the
                    //   costume window are the host's (`EquipAvatar`).
                    match &ev {
                        InputEvent::Key { code, down } if equip_avatar.key(&mut *emulator, &item_proxy, &mut avatar_book, &avatar_io, &avatar_sheets, *code, *down) => continue,
                        InputEvent::MenuTap { fx, fy } if equip_avatar.tap(&mut *emulator, &item_proxy, &mut avatar_book, &avatar_io, &avatar_sheets, *fx, *fy) => continue,
                        _ => {}
                    }
                    // ★★ 2026-10-16: the 정렬 / 일괄판매 buttons, and OK / CLR on the 일괄판매 popups (`BagTools`).
                    match &ev {
                        InputEvent::Key { code, down } if bag_tools.key(&mut *emulator, &mut AppBagIo, *code, *down) => continue,
                        InputEvent::MenuTap { fx, fy } if bag_tools.tap(&mut *emulator, &mut AppBagIo, *fx, *fy) => continue,
                        _ => {}
                    }
                    // ★★ 2026-10-20: a line the UI wants said in the game's notice box (`host_notice`).
                    if let InputEvent::HostNotice(text) = ev {
                        host_notice(text);
                        continue;
                    }
                    if let InputEvent::WorldHold(on) = ev {
                        world_hold = on;
                        emulator.park_guest(on);
                        continue;
                    }
                    // ★ #13: a page tab. Handled here, with the loop's own paging state,
                    //   rather than threaded through `apply_input`'s dozen parameters.
                    if let InputEvent::BagPage(page) = ev {
                        if let Some(h2) = crate::guest_build::hero2_java() {
                            bag_paging.select(&mut *emulator, &h2, &item_proxy, page);
                        }
                        continue;
                    }
                    // ★ …and LEFT/RIGHT at the grid's two ends cross pages instead of
                    //   wrapping inside one (`BagPaging::arrow`). The key-down is taken
                    //   here; its key-up is dropped too, so the game sees neither half.
                    if let InputEvent::Key { code: code @ (KeyCode::LEFT | KeyCode::RIGHT), down } = ev {
                        // Only the up of the key whose down was taken (2026-10-02: UP/DOWN take theirs too).
                        if !down && bag_swallow_up == Some(code) {
                            bag_swallow_up = None;
                            continue;
                        }
                        if down
                            && let Some(h2) = crate::guest_build::hero2_java()
                            && bag_paging.arrow(&mut *emulator, &h2, &item_proxy, code == KeyCode::LEFT)
                        {
                            bag_swallow_up = Some(code);
                            continue;
                        }
                    }
                    // ★ 2026-10-02: …and UP/DOWN on the short fifth page keep the column (`BagPaging::vertical`).
                    if let InputEvent::Key { code: code @ (KeyCode::UP | KeyCode::DOWN), down } = ev {
                        if !down && bag_swallow_up == Some(code) {
                            bag_swallow_up = None;
                            continue;
                        }
                        if down
                            && let Some(h2) = crate::guest_build::hero2_java()
                            && bag_paging.vertical(&mut *emulator, &h2, code == KeyCode::UP)
                        {
                            bag_swallow_up = Some(code);
                            continue;
                        }
                    }
                    apply_input(&mut *emulator, &mut held, &mut picking, &mut pages, &mut window, &mut buffs, &mut saving, &mut skill_watch, &mut levelups, &mut drive, &mut account, &data_dir, &item_proxy, &mut walk_guard, &mut beast_discard, bag_paging.paging_on(), ev)
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::info!("input channel disconnected; stopping emulator");
                    return;
                }
            }
        }

        // ★★ 2026-10-16: a beast discard's 예 goes on once its refund has run.
        step_beast_discard(&mut *emulator, &mut beast_discard, &mut held, &drive, &mut walk_guard);

        // 3) Key auto-repeat. Not while the item-use drive owns the screen: a held
        //    walk key repeating into the bag it is walking would move the bag's cursor.
        //    Nor while the 부활의 구 box holds the guest: a repeat would only queue.
        let now = Instant::now();
        if !world_hold && !drive.as_ref().is_some_and(|d| d.owns) && !revive.holding() && !refine_protection.busy(&*emulator) && !save_xfer.holding() && !save_xfer.owns_input() {
            for (code, last) in held.iter_mut() {
                if now.duration_since(*last).as_millis() > KEYREPEAT_MS {
                    emulator.handle_event(Event::Keyrepeat(*code));
                    *last = now;
                }
            }
        }

        // 3b) The walk edges the guest can lose (2026-09-24, `WalkGuard`): a direction
        //     release owed to a character still walking, a re-press waiting for it to
        //     stand. Costs nothing while neither is pending.
        walk_guard.poll(&mut *emulator, &mut held, &drive);

        // 3c) Pickup notices to the UI, and 오토루팅 (2026-09-24). Both every iteration:
        //     the pickups are one call per frame and each raises one notice.
        notice_feed(&mut *emulator, &item_proxy);
        host_notices.poll(&mut *emulator, &item_proxy);
        auto_loot.poll(&mut *emulator, &drive);
        options.poll(&mut *emulator, &item_proxy);
        equip_avatar.poll(&mut *emulator, &item_proxy, &avatar_book, &avatar_sheets);
        if let Some(buttons) = bag_tools.poll(&mut *emulator, &mut AppBagIo) {
            let _ = item_proxy.send_event(UserEvent::BagTools(buttons));
        }

        // 3d) ★ 부활의 구 (2026-09-25): a fall with an orb in the bag raises the box, and
        //     while it is up the guest is not ticked at all (the step below is skipped) —
        //     see `Revive`. Four words a frame while nothing has fallen.
        // ★ 2026-09-26: a boss that starts to die refills the character (and stands it up if
        //   the two killing blows landed together) — BEFORE the revive box would ask about
        //   that same fall. See `BossRefill`.
        boss_refill.poll(&mut *emulator);
        // ★★ 2026-10-17: the boss raid's countdown · advance · pay-out (`BossRaid`) — a map read a loop, the flag on a raid map.
        let revive_waits = revive.waits(&*emulator);
        boss_raid.poll(&mut *emulator, &item_proxy, revive_waits);
        // ★★ 2026-10-17: 악의's clock stands between a raid kill and the next stage (a word, on a change)
        demon.set_pause(&mut *emulator, boss_raid.model.paused());
        revive.poll(&mut *emulator, &item_proxy);
        refine_protection.poll(&mut *emulator, &item_proxy, &mut account, &data_dir);

        // 4) Adaptive multi-tick: keep running while the game stays work-bound
        //    this frame, up to a hard wall-clock cap.
        while !revive.holding() && !refine_protection.dialog.holding() && !save_xfer.holding() {
            // Sample before the tick so a hitch can be attributed to what happened INSIDE
            // it — see `TickCounters` for why the 30 s gauge cannot answer this. The
            // profile swap gives the tick its own PC samples without costing anything per
            // sample; with the profiler off it does not happen at all.
            let carry = profiling.then(profile_take);
            let before = TickCounters::now();
            let tick_start = Instant::now();
            if let Err(e) = emulator.tick() {
                // The notice itself is raised by the spawn wrapper, which covers every
                // way out of this function; this line is the WHY for the log.
                log::error!("emulator tick error: {e:?}");
                return;
            }
            let took = tick_start.elapsed();
            gauge_tick_us += took.as_micros() as u64;
            gauge_ticks += 1;
            let hitch = took.as_millis() >= SLOW_TICK_MS && hitch_at.is_none_or(|t| t.elapsed() >= HITCH_REPORT_GAP);
            // Always restore, hitch or not — a skipped merge would quietly empty the 30 s
            // aggregate the throughput gauge prints.
            let hot = carry.map_or(String::new(), |c| profile_merge_back(c, if hitch { 8 } else { 0 }));
            if hitch {
                hitch_at = Some(Instant::now());
                report_hitch(took, &before, &TickCounters::now(), &hot);
            }
            if took.as_millis() < TICK_FULL_MS {
                gauge_short_ticks += 1;
                break; // game went idle -> done this frame
            }
            if frame_start.elapsed().as_millis() >= FRAME_BUDGET_MS {
                gauge_capped += 1;
                break; // hard cap
            }
            // ★ 부활의 구: a fall inside a work-bound multi-tick frame stops the frame here,
            //   so the next poll asks while the nine dying frames are still ahead.
            if revive.would_ask(&*emulator) || refine_protection.state(&*emulator).is_some_and(|(s, _)| s == 1) {
                break;
            }
        }

        // 4a) The item-use drive (hero2 ⑧): one look at the screen, at most one key.
        //     After the tick, so the key sent last poll has been consumed and the chain
        //     it was meant to change has changed.
        if drive.is_some() {
            step_use_drive(&mut *emulator, &mut drive, &item_proxy, &mut bag_paging, &mut account);
        }
        let drive_owns = drive.as_ref().is_some_and(|d| d.owns);

        // 4b) DEBUG (Phase 1 of the skill-HUD track): honour a guest-RAM dump
        //     request every ~0.5 s. Drop a `dump_trigger` file (lines of
        //     "addr_hex len_hex") into the app data dir to snapshot live memory —
        //     used to live-diff and locate the equipped-skill fields.
        frame = frame.wrapping_add(1);
        if frame % 30 == 0 {
            debug_ram_dump(&*emulator, &data_dir);
        }

        // 4b0) ★ 세이브 슬롯 5 (2026-09-27): the file-name tables, then the code — EVERY
        //      frame until done, because the window it needs is the logo and the key that
        //      leaves the logo builds the title within one tick. Idle, it is one static read
        //      a frame for the first handful of frames and nothing after. See `SaveSlots`.
        // ★ 2026-10-08 가디언 ①: the mode tables' halfwords BEFORE `ag` initialises — SaveSlots asks for that
        //   initialisation on its first poll, so this goes first on the same frame. See `ModeTables`.
        if !mode_tables.finished() {
            mode_tables.poll(&mut *emulator, frame);
        }
        if !save_slots.finished() {
            save_slots.poll(&mut *emulator, frame);
        }
        // 4b0') ★ 2026-09-27 (user: "새로하기로 삭제하고 다시 해당 슬롯에서 시작하면 이전 데이터의
        //       퀵슬롯의 잔재가 남아있어"): the slot screen's mode, EVERY frame while the
        //       shell is up (a class pick answered within one HUD poll would slip past a
        //       5-frame sample). `items::shell_state`, not the bare state word: a `1` with no
        //       title object is a scene fade in a running game, and it must not count as
        //       the title standing alone. A few reads a frame in the shell, one outside.
        if let Some(h2) = crate::guest_build::hero2_java() {
            let seen = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                items::shell_state(&read).then(|| crate::guest_build::h2_slot_screen(&read, &h2).map(|s| s.mode))
            };
            new_game_watch.sample(seen, i32::from(crate::guest_build::Hero2Java::SLOT_MODE_CLASS_PICK));
        }

        // 4b2) STACK HIGH-WATER, ~every 30 s. See `stack_rss_kb`: this is the gauge that
        //      turns the intermittent 21-35 minute crash into a number that can be read
        //      off a single play session. A healthy run plateaus within the first minute;
        //      a rising staircase is the leak, and its slope says how long a player has.
        if frame % 1800 == 0
            && let Some(kb) = stack_rss_kb()
            && kb > stack_rss_peak
        {
            stack_rss_peak = kb;
            log::warn!("emu stack high-water {kb} KB of {} KB", EMU_STACK / 1024);
        }

        // 4b3) THROUGHPUT, ~every 30 s. The one number that says whether the emulator can
        //      keep up on THIS device: the game needs about 123 M guest instructions per
        //      second of real time to run at its intended 25 fps, so `M inst/s` below 123
        //      is exactly how much the player is missing. `blit` is the share of sprite
        //      blits the host draws instead of interpreting (2026-08-06: 99.99% headless,
        //      and 36% of all guest instructions); a collapse there means the binary
        //      patch did not install and the device is back to interpreting everything.
        if frame % 1800 == 900 {
            let (instr, svc, _) = wie_util::stats::snapshot();
            let frames = wie_util::stats::frames();
            let ([calls, hle, _px, _px_hle], _, _, refused) = wie_util::stats::blit_snapshot();
            // ★ INTERVAL, not cumulative. A running average is unreadable here: it mixes the
            // title screen (where the loop sleeps out most of every 16 ms because the guest
            // has nothing to do) with real field play, so a low number could mean either
            // "too slow" or "nothing to draw". The delta since the last report is the rate
            // the player is actually getting right now.
            let now = Instant::now();
            let secs = now.duration_since(gauge_at).as_secs_f64().max(1e-9);
            // `tint` is the critical-hit colour transform (49차). Separate from `blit`
            // because it draws no sprite — see `wie_util::stats::TINT_CALLS`.
            let (tint_calls, tint_hle, tint_px) = wie_util::stats::tint_snapshot();
            log::warn!(
                "emu throughput {:.1} M inst/s (need ~123 for 25 fps), guest {:.1} fps, svc {svc}, blit {hle}/{calls}, refused {refused:?}, tint {tint_hle}/{tint_calls} ({tint_px} px), guest mem {} MB",
                (instr - gauge_instr) as f64 / secs / 1e6,
                (frames - gauge_frames) as f64 / secs,
                // Committed guest pages. Pinned at 256 MB+ before the lazy-map change, which is
                // what made this process the low-memory killer's first choice; if it ever climbs
                // back toward 256 the saving has been undone somewhere.
                wie_util::stats::guest_page_bytes() / (1024 * 1024),
            );
            // ★ The guest SOUND path, which runs on THIS thread. Every `MC_mdaPlay` used to
            // re-parse the whole SMAF file (chunk walk + event Vec + sort) inside the frame
            // budget, and every `MC_mdaClipPutData` minted a handle nothing ever removed.
            // Reading this line: `parses` should settle at the number of DISTINCT sound
            // files the session has touched (Hero3 ships 33) while `plays` climbs freely.
            // **`parses` tracking `plays` means the cache is not being hit** and the crit
            // hitch is back. `handles` climbing without bound means the clip lifecycle
            // fix stopped working. `catchup` counts songs re-based after the emulator was
            // paused (backgrounded) — a few per resume is expected, a stream is not.
            let pct = |us: u64| (us as f64 / (secs.max(1e-9) * 1e4)).min(999.9);
            log::warn!(
                "emu loop: {} frames/s, tick {:.2} ms avg ({} ticks, {} short, {} budget-capped), in-tick {:.0}% of wall, sleeping {:.0}%",
                (gauge_loops as f64 / secs) as u64,
                gauge_tick_us as f64 / gauge_ticks.max(1) as f64 / 1000.0,
                gauge_ticks,
                gauge_short_ticks,
                gauge_capped,
                pct(gauge_tick_us),
                pct(gauge_sleep_us),
            );
            (gauge_tick_us, gauge_ticks, gauge_short_ticks) = (0, 0, 0);
            (gauge_capped, gauge_sleep_us, gauge_loops) = (0, 0, 0);
            // ★ 57차: how many guest paints actually reached the UI thread vs were
            // dropped as bit-identical (platform.rs frame hash). On a static screen
            // (title, open menu) `skipped` should carry nearly the whole guest
            // framerate — `skipped 0` there means the gate has stopped working and
            // the UI is back to one full blit+present per identical frame.
            // ★ roadmap #12: what the CONSERVATIVE ROOT SCAN costs on a real phone.
            // Headless it reported 13~16 words per collection, but that is the scan's
            // OUTPUT (candidates that survived the heap-range filter); the work is
            // `words`, and the part of it that grows with `run_function` nesting is
            // `stack` — a 24-deep nest (지옥불) walks a deeper `[sp, top)` span on every
            // thread. `max` is the deepest single scan in the window, because a mean over
            // 30 s hides exactly the spike this line exists to catch.
            let (cs_n, cs_words, cs_stack, cs_kept, cs_max) = wie_util::stats::cons_scan_take();
            if cs_n > 0 {
                log::warn!(
                    "gc scan: {} collections/s, {} words/scan ({} max, {} B of stack), {} candidates/scan, {} words/s",
                    (cs_n as f64 / secs) as u64,
                    cs_words / cs_n,
                    cs_max,
                    cs_stack / cs_n,
                    cs_kept / cs_n,
                    (cs_words as f64 / secs) as u64,
                );
            }
            let (paint_sent, paint_skipped) = crate::platform::paint_stats_take();
            log::warn!(
                "emu paint: sent {}/s, skipped {}/s identical",
                (paint_sent as f64 / secs) as u64,
                (paint_skipped as f64 / secs) as u64,
            );
            // ★ Synth load (2026-09-19). `voices` is what the audio callback renders per
            // sample — the AudioTrack thread's DSP cost is linear in it. A field BGM at ~12
            // note-ons/s should sit well under the 64 cap; `max` pinned at 64 means voices are
            // not being released (stuck notes / no note-off from the guest) and the synth is
            // rendering a full pool of inaudible tails.
            let (v_last, v_max, v_avg, q60, q80, ons, offs) = crate::audio_sink::audio_stats_take();
            log::warn!(
                "audio: voices {v_last} (avg {v_avg}, max {v_max}; avg below -60 dB {q60}, below -80 dB {q80}), note-on {}/s, note-off {}/s",
                (ons as f64 / secs) as u64,
                (offs as f64 / secs) as u64,
            );
            // ★ WHICH SVCs, and what the recv throttle absorbed (§2.24). `svc` above is a
            // count with no attribution: on LGT the C stdlib is an SVC too, so a stall can
            // be `memcpy` (a Rust helper, ~nothing) or `MC_netSocketRecv` (a socket
            // syscall). These two lines are how the warehouse frame drop was pinned down,
            // and they are what says whether it has come back: `recv polls` in the tens of
            // thousands per second with `absorbed` near 100 % is healthy; `absorbed 0 %` at
            // that rate means the throttle is gone and the game is back to 5 fps.
            let (cats, hot) = wie_util::stats::svc_mix_take(8);
            let hot = hot
                .iter()
                .map(|(id, n)| format!("{id:#x}={}", (*n as f64 / secs) as u64))
                .collect::<Vec<_>>()
                .join(" ");
            log::warn!(
                // ★ 2026-09-18: `jif` (the KTF Java INTERFACE, category 2) and `java`
                //   (category 4) were missing from this line — and on hero2 they are
                //   essentially the whole of it. Standing still on the field (S25) the
                //   total was 86,000 svc/s while the four categories printed here added
                //   up to 1,861: the gauge was reporting 2 % of the traffic it exists to
                //   attribute. Print every category the dispatcher has.
                "emu svc/s by category [init {} jif {} wipic {} java {} stdlib {} hook {}] hot ids/s: {hot}",
                (cats[1] as f64 / secs) as u64,
                (cats[2] as f64 / secs) as u64,
                (cats[3] as f64 / secs) as u64,
                (cats[4] as f64 / secs) as u64,
                (cats[5] as f64 / secs) as u64,
                (cats[0] as f64 / secs) as u64,
            );
            let (sets, tmin, tmax, tmean, naps, nap_ms) = wie_util::stats::frame_timing_take();
            let (clamped, cap) = wie_util::stats::timer_clamp_take();
            let (unsets, stale_drops) = wie_util::stats::timer_cancel_take();
            if sets > 0 || naps > 0 {
                // `min/mean/max` are what the GUEST asked for; `capped` is how much of
                // that we overrode. In a modal screen with the cap on, expect max 244 and
                // capped ≈ 100 % — that pair is what says the clamp is the thing moving
                // the frame rate, and not some other change.
                // `unset`/`stale` are the timer-cancellation health pair (§41 item 7):
                // across a network round trip `guest set N/s` must come back to its
                // field value, and `stale` > 0 is the direct proof that a replaced or
                // cancelled shot was actually discarded instead of firing.
                log::warn!(
                    "emu frame timing: guest set {}/s timers (min {tmin} ms, mean {tmean} ms, max {tmax} ms); \
                     capped {}% to {cap} ms; unset {}/s, stale dropped {}/s; pump napped {}/s for {:.1} ms avg",
                    (sets as f64 / secs) as u64,
                    if sets == 0 { 0 } else { clamped * 100 / sets },
                    (unsets as f64 / secs) as u64,
                    (stale_drops as f64 / secs) as u64,
                    (naps as f64 / secs) as u64,
                    nap_ms as f64 / naps.max(1) as f64,
                );
            }
            let (polls, absorbed, socket_ns) = crate::network::recv_stats_take();
            if polls > 0 {
                log::warn!(
                    "emu net: recv polls {}/s, absorbed {:.0}% by the 2 ms gate, socket {:.0} ms/s ({} ns per real poll)",
                    (polls as f64 / secs) as u64,
                    absorbed as f64 * 100.0 / polls as f64,
                    socket_ns as f64 / secs / 1e6,
                    socket_ns / (polls - absorbed).max(1),
                );
            }
            let hot = profile_top(12);
            if !hot.is_empty() {
                log::warn!("emu profile{hot}");
            }
            let a = wie_util::stats::audio_snapshot();
            log::warn!(
                "emu sound loads {} plays {} parses {} ({} KB) cache {} handles {} catchup {}",
                a[0],
                a[1],
                a[2],
                a[3] / 1024,
                a[4],
                a[5],
                a[6],
            );
            (gauge_at, gauge_instr, gauge_frames) = (now, instr, frames);
        }

        // 4c) HUD channel. The field/menu flag decides the ENTIRE overlay control
        //     set (field: skill plates + gauges + minimap; game window: menu
        //     navigation keys), so it is polled ~12x/s — the cheap read — while the
        //     pointer-chasing quickbar scan stays at ~4x/s. Gauges keep their LAST
        //     GOOD value on an implausible read instead of collapsing to zero
        //     (which is what drew HP/MP bars as empty whenever a window was open).
        // 4c0) AUTO-LOOT forced ON (player request 2026-07-30: "auto-loot should be
        //      on from character creation"). The game only ever enables it through
        //      the 오토루팅 cash item and clears it in CHAR_init / the new-game reset,
        //      and it is not serialised — so the host re-asserts it. One byte read,
        //      and a write only when it is actually off, so this is free and needs no
        //      "is this character creation?" logic. See items::autoloot_to_force.
        //      ★ 2026-08-13: the offset is per BUILD now, and on LGT it is unknown, so
        //      this does nothing there. It used to do nothing there SILENTLY — gated
        //      behind the KTF-only guest_code_addresses_valid() — which is how the
        //      shipped app came to have neither the forcing nor the cash item that does
        //      the same job. Say so once at boot so the next regression is visible.
        if frame % 30 == 0 {
            if !autoloot_notice_done {
                autoloot_notice_done = true;
                if items::autoloot_forcible() {
                    log::info!("auto-loot: host will keep it ON for this build");
                } else {
                    log::warn!(
                        "auto-loot: NOT forcible on this build (offset unlocated) — the \
                         player must buy the game's own 오토루팅 item; see \
                         guest_anchors::autoloot_off"
                    );
                }
            }
            let want = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::autoloot_to_force(&read)
            };
            if let Some((a, v)) = want {
                // The VALUE comes from the table too: KTF's arm writes 1, LGT's writes
                // 0x81. Writing the wrong constant is half of why the first LGT attempt
                // did nothing even once it had an address.
                let ok = emulator.write_memory(a, &[v]);
                log::info!("auto-loot: forcing ON at {a:#x} <- {v:#04x} (write ok = {ok})");
            }

            // ★ 2026-08-13: let 영웅-이상 (gold/white CASH) equipment into the network
            //   warehouse. The shipped client refuses deposit for grade > 3 with
            //   "아이템 등급이 영웅 이상인 아이템은 올릴 수 없습니다"; this one-halfword
            //   patch (cmp #3 -> cmp #7) routes every grade to the same 05 41 upload.
            //   Written once — the site then reads the patched value, so this is a
            //   no-op on later polls. See items::warehouse_grade_patch.
            let wh_patch = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::warehouse_grade_patch(&read)
            };
            if let Some((a, bytes)) = wh_patch {
                let ok = emulator.write_memory(a, &bytes);
                log::info!("warehouse deposit: un-gating 영웅+ equipment at {a:#x} (write ok = {ok})");
            }

            // ★ 천상의 영약: deliver pending level-ups, one per poll, only on the
            //   field — the exact context the game's own kill path runs checkLevelUp
            //   in. The mechanism is the game's own: exp = expToNext + 1, then its
            //   checkLevelUp (LGT 0x4d715, guest_anchors::level_up_fn), which grants
            //   points, refills HP/SP, honours the 70 cap and plays the level-up
            //   effect itself. At the cap the pending grant is burned with a toast
            //   rather than armed forever.
            if levelups > 0
                && let Some(f) = crate::guest_build::level_up_fn()
            {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                if items::in_field_state(&read) {
                    match items::level_elixir_writes(&read) {
                        Some((addr, bytes, level)) => {
                            let ok = emulator.write_memory(addr, &bytes);
                            emulator.call_guest_function(f, &[]);
                            levelups -= 1;
                            config::write_pending_levelups(&data_dir, levelups);
                            host_notice("천상의 영약: 레벨 UP!");
                            log::info!(
                                "level elixir fired: level {level} -> checkLevelUp({f:#x}), exp write ok = {ok}, {levelups} pending left"
                            );
                        }
                        // resolve_char refuses a non-live record, and at level 70 the
                        // writes helper declines; tell the two apart for the player.
                        None => {
                            if items::read_stats(&read).is_some_and(|s| s.level >= items::LEVEL_CAP) {
                                levelups = 0;
                                config::write_pending_levelups(&data_dir, 0);
                                host_notice("이미 최대 레벨입니다");
                                log::info!("level elixir: at the level cap — pending grant(s) cleared");
                            }
                        }
                    }
                }
            }

            // Push the game's own TOP HUD band (buff/status list + target-monster
            // box) below our gauges, by writing the y-origin global it adds to every
            // y up there. Same deal as auto-loot: read first, write only on a
            // mismatch, re-assert forever (the value has no other writers, so this
            // cannot fight the game). The UI thread publishes the amount.
            let packed = hud_y.load(Ordering::Relaxed);
            let want = packed & 0xff;
            let band_x = ((packed >> 8) & 0xff) as u8;
            let band_h = ((packed >> 16) & 0xff) as u16;
            let box_dx = (packed >> 24) & 0xff;
            let h_poke = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::band_height_poke(&read, band_h)
            };
            if let Some((a, bytes)) = h_poke {
                let ok = emulator.write_memory(a, &bytes);
                log::info!("buff band height: {a:#x} <- {} (write ok = {ok})", u16::from_le_bytes(bytes));
            }
            let band_patches = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::band_x_patches(&read, band_x)
            };
            for (a, b) in band_patches {
                let ok = emulator.write_memory(a, &[b]);
                log::info!("buff band x: {a:#x} <- {b} (write ok = {ok})");
            }
            // Slide the game's own target-monster box LEFT, out of the top-right
            // corner our minimap now owns (player request 2026-07-30: minimap at the
            // very top-right, the monster box beside it on the left). Six one-byte
            // immediates, all or nothing — see items::target_box_x_patches.
            // ★ hero2 (2026-09-25, "hero3처럼 미니맵 왼쪽에 몹이름이랑 피통"): one site,
            //   `subs r7,#0x42` in `n.d()V` — the name/level image and its HP bar both
            //   take their x from it (guest_anchors::target_box_x_sites).
            let box_patches = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::target_box_x_patches(&read, box_dx)
            };
            for (a, bytes) in box_patches {
                let ok = emulator.write_memory(a, &bytes);
                log::info!(
                    "target box x: {a:#x} <- {:#06x} (shift {box_dx}, write ok = {ok})",
                    u16::from_le_bytes(bytes)
                );
            }
            let y_addr = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::hud_y_origin_to_set(&read, want)
            };
            if let Some(a) = y_addr {
                let ok = emulator.write_memory(a, &want.to_le_bytes());
                log::info!("hud y-origin: {a:#x} <- {want} (write ok = {ok})");
            }

            // Stop the game drawing its own BOTTOM field HUD (panel + HP/SP/EXP bars
            // + quickbar cells), which our overlay replaces. One Thumb branch, and
            // the world that was already drawn underneath shows through — masking it
            // black was the wrong answer. See items::hud_bottom_patch.
            // ★ hero2 (2026-09-25): two branches, TAIL first, so the game's own
            //   target-monster panel (name · LV · HP bar) survives — the one widget of that
            //   painter the host does not replace. The plan is ordered; a failed write
            //   stops it, so the head can never land without the tail.
            let patch = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::hud_bottom_patch(&read)
            };
            for (a, bytes) in patch {
                let ok = emulator.write_memory(a, &bytes);
                log::info!("game bottom HUD: skipping its draw at {a:#x} (write ok = {ok})");
                if !ok {
                    break;
                }
            }
            // ★ hero2 (2026-09-18): and let the world use the rows that HUD kept — the
            //   viewport constructor's `− 26` becomes `− 0`, so the framebuffer we show
            //   is the whole picture and the dialogue box overlays the world. Written
            //   once, from the first pass, before any in-world screen is constructed.
            //   See guest_anchors::viewport_full_patch.
            let vp = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::viewport_full_patch(&read)
            };
            if let Some((a, bytes)) = vp {
                let ok = emulator.write_memory(a, &bytes);
                log::info!("game viewport: full framebuffer height at {a:#x} (write ok = {ok})");
            }
            // ★ #14 (2026-09-21): move the `q` grid 7 px right so the page-tab rail fits
            //   INSIDE the window (user: "왼쪽 영역 가방5개. 그리고 그 바로 옆에 28칸").
            //   Two halfwords, all-or-nothing, and the shift is PUBLISHED only once every
            //   site reads as patched — the host draws and hit-tests where the pixels
            //   actually are, never where they were asked to be. See
            //   `guest_anchors::inventory_grid_shift`.
            {
                let (todo, already, total) = {
                    let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                    items::inventory_grid_shift(&read)
                };
                if total > 0 {
                    let mut landed = already;
                    for (a, bytes) in &todo {
                        if emulator.write_memory(*a, bytes) {
                            landed += 1;
                        } else {
                            log::warn!("inventory grid: the write at {a:#x} failed");
                        }
                    }
                    let shift = if landed == total { crate::guest_build::Hero2Java::INV_GRID_SHIFT } else { 0 };
                    if !todo.is_empty() || crate::inv_grid_shift() != shift {
                        log::info!("inventory grid: {landed}/{total} site(s) shifted — the rail is {}", if shift > 0 { "INSIDE the window" } else { "OUTSIDE (unshifted)" });
                    }
                    crate::INV_GRID_SHIFT.store(shift, std::sync::atomic::Ordering::Relaxed);
                }
            }
            // ★ 2026-10-09: the item PICKER `k`'s grid the same 7 px right — its page tabs (a recipe list or any
            //   picker over more than 28 entries, `PickerPaging`) stand in its left margin like the inventory's.
            //   Published the same way: only once both sites read as patched. See `guest_anchors::picker_grid_shift`.
            {
                let (todo, already, total) = {
                    let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                    crate::guest_build::h2_halfword_writes(&read, crate::guest_build::picker_grid_shift())
                };
                if total > 0 {
                    let mut landed = already;
                    for (a, bytes) in &todo {
                        if emulator.write_memory(*a, bytes) {
                            landed += 1;
                        } else {
                            log::warn!("picker grid: the write at {a:#x} failed");
                        }
                    }
                    let shift = if landed == total { crate::guest_build::Hero2Java::INV_GRID_SHIFT } else { 0 };
                    if !todo.is_empty() || crate::picker_grid_shift() != shift {
                        log::info!("picker grid: {landed}/{total} site(s) shifted — its page tabs are {}", if shift > 0 { "INSIDE the window" } else { "OUTSIDE (unshifted)" });
                    }
                    crate::PICKER_GRID_SHIFT.store(shift, std::sync::atomic::Ordering::Relaxed);
                }
            }
            // ★ #18 (2026-09-23): the SKILL-POINT curve — one point per level up to 20,
            //   then the shipped every-other-level rule (user: "레벨 20까지는 스킬포인트
            //   1씩 상승 21레벨부터 기존처럼 2레벨당 1"). Ten bytes rewritten in place in
            //   the level-up `a.l()V`; see `guest_anchors::skill_point_curve` for why this
            //   is NOT a trampoline (a `bl` cannot reach the guest heap from there).
            //
            //   ⛔ The five halfwords are one instruction sequence: a partial landing
            //   leaves the level-up testing a half-rewritten condition. `h2_halfword_writes`
            //   makes that nearly unreachable — it hands back a plan only when EVERY site
            //   reads as shipped or as already patched — so the remaining window is a
            //   `write_memory` that fails midway. That is logged at error and retried on
            //   the next pass (the plan is re-derived from what the memory now says, so a
            //   retry asks only for the halfwords that did not land).
            skill_curve.apply(&mut *emulator, "skill points", "1/level to LV 20, then 1 per 2 levels", crate::guest_build::skill_point_curve());
            skill_hotkey_badges.apply(&mut *emulator, "skill hotkey badges", "hide guest digits in skill and beast windows", crate::guest_build::skill_hotkey_badges_off());
            // ★ #16 (2026-09-23): the COMBO's SP — spent, now gained (user: "콤보
            //   2,3,4,5 단 사용시 sp 소모에서 획득으로 변경"). Three halfwords in `a.f()Z`:
            //   the negation that turns the cost into a subtraction, and (2026-09-24) the
            //   two that put it back on the SKILL branch, which shares that call — the
            //   first version made skills gain their SP too ("스킬사용시 SP소모가
            //   안되고있음"). The game's own `a.a(I)V` clamps at the maximum, so the gain
            //   cannot overflow a save. See `guest_anchors::combo_sp_gain`.
            combo_sp.apply(&mut *emulator, "combo SP", "basic hits GAIN their cost; skills still pay theirs", crate::guest_build::combo_sp_gain());
            // ★ 2026-09-26 (user: "콤보스킬이 sp를 소모하지 않으면서 버서커의 근력단련스킬이
            //   리뉴얼이 필요할거같아" → "스킬데미지 증가로 바꿀수도있어?"): 근력단련 took 7 SP a
            //   level off the basic hit — since #16 off the GAIN. One immediate in `bl.c()V`
            //   zeroes it; the new effect (+3 % skill damage a level) is the berserker's
            //   SkillPower block below, the text is the asset's (t/sk0.tdf[17]).
            //   See `guest_anchors::strength_training_sp_off`.
            strength_sp.apply(&mut *emulator, "근력단련 SP", "근력단련 no longer cuts the basic hit's SP (it raises skill damage)", crate::guest_build::strength_training_sp_off());
            // ★ 2026-09-26 (user: "반격태세 스킬이 캐릭터의 공격력에 비례해서 … 그냥 반격시 나의
            //   공격력만큼 반격데미지를 입히면돼" → "너가말한대로 적용하고"): the counter's damage —
            //   `bl.a(BILbd;)I` case 8, a flat `lv·14 + 20` — becomes the game's own attack number
            //   `a.a()I` × the level factor the SkillPower block passes in (100 % at level 1,
            //   +10 % a level; exactly 100 % if that block is missing). Eleven halfwords in that
            //   case. See `guest_anchors::counter_stance_attack`.
            counter_stance.apply(&mut *emulator, "반격태세", "the counter hits for the character's attack × (100 + 10·(lv−1)) %", crate::guest_build::counter_stance_attack());
            // ★ 2026-09-26 (user: "지속시간도 1레벨을 6초고 5렙찍으면 20초까지"): the stance's buff
            //   is registered for `70·lv + 50` frames instead of 120 — 6 s .. 20 s. Three
            //   halfwords at the cast. See `guest_anchors::counter_stance_duration`.
            // ★ 2026-10-06 (user: 버프 8종 쿨 1분 · 「만렙 지속 75초」 · 「전부 ×1.25」): `250·lv + 250` = 25 → 75 s.
            counter_length.apply(&mut *emulator, "반격태세 지속", "the stance lasts 25 s at level 1 up to 75 s at level 5", crate::guest_build::counter_stance_duration());
            // ★ 2026-10-06: the buff cooldowns at one minute — 분노태세 · 반격태세 (bl.<clinit>, before a berserker loads) and
            //   the beast's 빛의수호 · 빛의회복 (bn.a(B)I). 신의가호 · 철괴 are 60 s already, 맹독감염's is the venom block's;
            //   얼음장막 and 민첩태세 as they were. See `guest_anchors::buff_cooldown_minute`. The lengths that go with them:
            //   counter_length above, blessing · iron_wall · venom below, 분노태세 and the two beast buffs in `buff_refresh`.
            buff_cd.apply(&mut *emulator, "버프 쿨타임", "분노태세 · 반격태세 · 빛의수호 · 빛의회복 cool down in 60 s", crate::guest_build::buff_cooldown_minute());
            // ⛔ #20's DROP WINDOW patch is gone (2026-09-25): it let NORMAL roll each
            //   enemy's HARDCORE/HELL gear slots — LV 36~73 items from LV 2~4 monsters.
            //   The shipped window is the level rule; see `guest_anchors` (the note where
            //   `drop_window_all` used to be).
            // ★ 2026-09-24 (user: "네트워크가 필요없어져서 카드 드랍이 되면 안돼"): the
            //   kill routine's hardcoded 11 % spell-card roll never fires — one compare
            //   immediate in `bd.q()V`, 89 -> 100 against a 1..=100 roll. The card
            //   window is ACCOUNT's since #13, so a dropped card was loot the player
            //   could pick up but never see. See `guest_anchors::card_drop_off`.
            card_drop.apply(&mut *emulator, "card drop", "the 11 % spell-card roll never fires", crate::guest_build::card_drop_off());
            // ★ 2026-09-25 (user: "명예등급이 낮으면 장비 능력이 감소되는 기능 … 네트워크로만
            //   올릴수있었던거라 현재 버전에는 있으면 안되는 기능이야"): the weapon getters
            //   `bo.c()S`/`bo.d()S` no longer scale attack by `(10 − 요구명예 + 명예)/10` — one
            //   `blt` each becomes a `nop`. There was no honour EQUIP gate to remove (census in
            //   `guest_anchors::honor_penalty_off`). A code patch, nothing is saved.
            honor_penalty.apply(&mut *emulator, "honour penalty", "weapons keep their full attack at any 명예", crate::guest_build::honor_penalty_off());
            // ★★ 2026-10-17 (user: "이름 가격 자동 갱신 진행하고 요구레벨도 표 따르게 해줘"): an owned item loads with the CURRENT
            //   table's name, price and 요구레벨 — the parsers read them on the load path too and the loaders skip the saved
            //   copies (the only other writers). Must land before a save loads (이어하기 · the warehouse) — the first pass does.
            //   See `guest_anchors::item_table_refresh`.
            item_refresh.apply(&mut *emulator, "item table refresh", "owned items take the item table's name, price and 요구레벨 on load", crate::guest_build::item_table_refresh());
            // ★ 2026-09-24 (user: "메인 메뉴내에서 네트워크 창고라는 메뉴를 없애줘"): the
            //   title carousel runs 6 stops and its jump-table entries 4·5 take the
            //   정보이용료·게임종료 handlers; the LABELS move in the asset
            //   (`make_hero2_asset.py` rewrites com.tdf[4]/[5]), so without the
            //   regenerated asset this patch shows the right rows with the OLD names —
            //   and without the patch the new names sit on the OLD actions. Both halves
            //   ship together. See `guest_anchors::title_menu_no_network`.
            title_menu.apply(&mut *emulator, "title menu", "the 네트워크창고 stop is gone (6 stops)", crate::guest_build::title_menu_no_network());
            // ★ 2026-09-24 (S9 logcat, "오류로중단됨"): a SHIPPED bug — CLR on the SKILL
            //   window's PASSIVE-tab popup reached the 스킬상세정보 branch with sel = -1
            //   and built a detail window for a combo stage, whose negative table index
            //   killed the worker. Three halfwords in `s.a(BB)V` make it a three-way
            //   split like the ACTIVE tab's. See `guest_anchors::skill_popup_cancel`.
            skill_cancel.apply(&mut *emulator, "skill popup cancel", "CLR on a PASSIVE-tab skill popup closes it", crate::guest_build::skill_popup_cancel());
            // ★★ 2026-10-16 (user: "1에서 좌측이나 아래로 갯수를 내리면 99 및 최대 갯수로"): the game's 개수 box wraps —
            //   LEFT / DOWN on 1 go to its ceiling (99 when buying, the stack when selling), and (user: "99에서 위 or 우측
            //   버튼누르면 다시 1") UP / RIGHT on the ceiling go back to 1. See `guest_anchors::qty_box_wrap`.
            qty_wrap.apply(&mut *emulator, "qty box wrap", "the 개수 box wraps: LEFT / DOWN on 1 → ceiling, UP / RIGHT on the ceiling → 1", crate::guest_build::qty_box_wrap());
            // ★ 2026-10-09 (user: "재료가방창도 127칸으로 만들꺼고. 추가로 신규아이템 조합서들을 구성할꺼야 … 종결급 템들을
            //   재료로"): the alchemist's 아이템 조합 on 127-cell bags — the material mark keeps a 재료가방 index past 77
            //   non-negative (the +50 is gone), and the craft consumes the spare, never the worn copy (the game checked
            //   with the worn-skipping finder and consumed with the other — the guardian recipes take 종결 gear, which is
            //   what a player wears). See `guest_anchors::{craft_material_mark, craft_worn_skip}`.
            craft_mark.apply(&mut *emulator, "craft material mark", "a 재료가방 material past cell 77 shows as present", crate::guest_build::craft_material_mark());
            craft_worn.apply_cells(&mut *emulator, "craft worn skip", "crafting consumes the spare, never the worn copy", crate::guest_build::craft_worn_skip());
            // ★ 2026-09-26 (user: "버서커로 사자후 스킬 사용하면 오류로 중단됨" + range
            //   「맵 전체」): the roar's scan `a.e(B)V` sized its square from the SCREEN width
            //   (`n.a / 16`) and never checked the map, so on a landscape guest (28~29 tiles)
            //   any map with a shorter side ran it off the grid — AIOOBE, worker dead. Now the
            //   window is the map itself. See `guest_anchors::roar_whole_map`.
            roar_scan.apply(&mut *emulator, "roar whole map", "사자후 hits every monster on the map", crate::guest_build::roar_whole_map());
            // ★ #17 (2026-09-24): class skill COOLDOWNS halved — one shift per job in
            //   `<job>.a(I)S`. Published so the plate ring divides the same way.
            skill_cd.apply(&mut *emulator, "skill cooldown", "class skill cooldowns halved", crate::guest_build::skill_cooldown_half());
            crate::SKILL_CD_HALVED.store(skill_cd.done, std::sync::atomic::Ordering::Relaxed);
            // ★ #17 (2026-09-24): class skill DAMAGE ×1.5 — three definition words to a
            //   host block, once, all or none. See `SkillPower` /
            //   `guest_anchors::SkillPowerPatch`. ★ 2026-09-27: + the knight's 검기·둔기강타
            //   +30 %, 방패치기 +50 %, and under 신의가호 둔기강타 ×2 · 방패치기 ×1.5.
            skill_power.install(&mut *emulator);
            // ★ 2026-09-27 전투 밸런스 (user: 「중간」 · *"로그의 sp소모량을 전체적으로 줄이고
            //   민첩을 찍을때 치명및회피가 올라가는 계수를 상향"* · *"검기는 사거리5칸으로"* ·
            //   *"인내오라는 신의가호 라는 버프스킬로"* · *"로그의 민첩태세를 일정시간 무적으로"*):
            //   rogue skill SP −40 % · 치명 민첩/5 (로그 /3) · 회피 민첩/3 · 검기 5 tiles · 신의가호
            //   10→20 s (+2 s per 오라수련 level), cooldown 30 s, the old HP/SP boost gone · 민첩태세 = invulnerable 2→4 s,
            //   cooldown 30 s, the old 민첩 bonus gone. The `<clinit>` halves of the last two land
            //   here, long before a knight or rogue class is loaded. Then the COMBAT block (weapon
            //   attack by type, the front sweep, three-way 검기, the invulnerability guards) — once,
            //   all or none. See `guest_anchors::{rogue_skill_sp_cut, agility_coefficients,
            //   sword_wave_range, blessing_aura_rework, agility_stance_invuln, CombatPatch}`.
            rogue_sp.apply(&mut *emulator, "rogue skill SP", "로그 스킬 SP −40 % (일격 is the basic hit's)", crate::guest_build::rogue_skill_sp_cut());
            agility.apply(&mut *emulator, "agility", "치명 민첩/5 (로그 민첩/3) · 회피 민첩/3", crate::guest_build::agility_coefficients());
            wave_range.apply(&mut *emulator, "검기 range", "검기 flies 5 tiles", crate::guest_build::sword_wave_range());
            // ★ 2026-10-20 밸런스 전수 조사 ②: the knight's a.h = 힘' × 17/20 (was 7/10) — bx.c()V's two immediates.
            knight_str.apply(&mut *emulator, "knight strength", "나이트 공격력 힘 × 0.85 (was 0.7)", crate::guest_build::knight_strength_coefficient());
            // ★ 2026-10-03 발도: the SKILL window counts the knight's actives as seven (its seventh,
            //   부활오라's slot, is 발도 now — the cast / SP / cooldown live in the combat block).
            active_seven.apply(&mut *emulator, "SKILL window actives", "the SKILL window: 버서커 8 (도발) · 나이트 7 (발도) · 로그 7 (일풍격)", crate::guest_build::knight_active_seven());
            // ★ 2026-10-03 (user: *"사자후 쿨타임 50% 늘려줘"*): bl.a[6] 160 → 240 frames in bl.<clinit>.
            roar_cd.apply(&mut *emulator, "사자후 cooldown", "사자후 cooldown table ×1.5 (80 → 120 frames after halving)", crate::guest_build::roar_cooldown_up());
            blessing.apply(&mut *emulator, "신의가호", "buff 18.75→37.5 s + 7.5 s per 오라수련 level (75 s maxed), cooldown 60 s, the HP/SP boost gone", crate::guest_build::blessing_aura_rework());
            stance_invuln.apply(&mut *emulator, "민첩태세", "invulnerable 2→4 s, cooldown 30 s, the 민첩 bonus gone", crate::guest_build::agility_stance_invuln());
            // ★ 2026-10-05 (user: *"1로 구현하고 이름은 철괴야. 받는피해는 1lv : 10%~ lv5 : 20%까지 … 지속시간은 1lv : 20초,
            //   5lv : 40초, 오라 숙련 레벨 1당 받는 피해감소량이 1%씩 … 지속시간도 오라숙련 1레벨당 4초씩 … 쿨타임은 60초 고정"*):
            //   방어의지 → 철괴 — the cast's length, the cooldown, no break on an attack, the plain walk. The damage
            //   cut itself is the combat block's (`CombatPatch::IRON_AT`), installed right below.
            iron_wall.apply(&mut *emulator, "철괴", "buff 35 25→50 s + 5 s per 오라수련 level (75 s maxed), cooldown 60 s, kept through attacks", crate::guest_build::iron_wall_rework());
            // ★ 2026-10-07 (2) (user: *"몸통치기를 없앨꺼고"*): the level table no longer opens the berserker's 몸통치기 at LV 8
            //   (`a.<clinit>`'s `movs r2,#8 → #99`) — the slot stays −1 on a new berserker and after `BodySlamRetire`.
            bodyslam_lock.apply(&mut *emulator, "몸통치기 lock", "the level table never opens a.p[5] (LV 8 → 99)", crate::guest_build::bodyslam_locked());
            // ★ 2026-10-07 (2) (user: *"데미지가 점점 커짐에 따라 max데미지를 풀려고하는데"*): the monster HP delta `bd.a(IB)V`
            //   kept HP − damage as a SHORT (`lsls/asrs #16`) — a hit past HP + 32,768 wrapped the HP positive, the
            //   effective per-hit cap of 32,767. Two halfwords keep the word; the HP field is a word already.
            damage_cap.apply(&mut *emulator, "damage cap", "bd.a(IB)V keeps HP − damage as a word (a hit past 32,767 no longer wraps)", crate::guest_build::damage_cap_off());
            // ★★ 2026-10-16 (user: *"상한을 아예 없애줘. 보스레이드는 보스 종류로 만들꺼야."*): the other five monster HP stores
            //   (spawn · shipped poison tick · two boss heals · template scale) keep the word — monster HP is an int.
            hp_int.apply(&mut *emulator, "monster HP int", "monster HP stores keep the word (no 32,767 ceiling)", crate::guest_build::monster_hp_int());
            // ★ 2026-10-05 (user: *"빛의 회복은 안나오고있어 … 수정해줘"* → 「진짜 버프로」): the beast's 빛의회복 (type 4) went
            //   to the EFFECT list, so the game's own type-4 regen tick (`a.a(Lav;)V`, HP / 30 frames · SP / 10) never ran
            //   and the buff HUD had nothing to show. Five halfwords send it through `bn.c(4)` like 얼음장막 — the buff list.
            //   See `guest_anchors::beast_recovery_buff`.
            beast_recovery.apply(&mut *emulator, "빛의회복", "the beast's 빛의회복 is a buff (HP/SP regen tick, the HUD timer, a recast restarts it)", crate::guest_build::beast_recovery_buff());
            // ★ 2026-10-06 (user: *"지력으로 영향받는 야수 스킬데미지를 상향할꺼야 … 지력 3를 올려야 올라가던 데미지가 이제 2만
            //   올려도 동일하게"* · *"야수 공격스킬의 sp소모량을 조금 감소해줘. 버프스킬은 건들 필요없어."*): 지력 ×1.5 in the rings
            //   and the heavy three; 암살자 · rings · heavy SP −20 %, the buffs as shipped. Halfwords in bn.d(B)V · bn.f()V ·
            //   bn.b(B)I and its jump table. See `guest_anchors::{beast_int_weight, beast_attack_sp_cut}`.
            // ★ 2026-10-12 (user: *"야수 스킬의 지력 계수를 50% 더 상향하고, 야수스킬 데미지50%상승, 쿨타임과 SP소모량 20%감소"* →
            //   「공격 스킬만, 지금 값에서 −20 %」): 지력 ×2 (also the 암살자 / status-7 ticks) · damage ×1.5 (the four `/3` → `/2`) ·
            //   attack cooldown ×0.8 · attack SP ×0.8 once more. See `guest_anchors::{beast_attack_cooldown_cut, beast_attack_power}`.
            beast_int.apply(&mut *emulator, "야수 지력", "지력 counts ×2 in the beast's ring and heavy skills and its two status ticks", crate::guest_build::beast_int_weight());
            beast_sp.apply(&mut *emulator, "야수 SP", "the beast's attack skills cost ×0.64 of the shipped SP (the buffs as shipped)", crate::guest_build::beast_attack_sp_cut());
            beast_cd.apply(&mut *emulator, "야수 쿨타임", "the beast's attack skills cool down 20 % faster (the buffs as they were)", crate::guest_build::beast_attack_cooldown_cut());
            beast_power.apply(&mut *emulator, "야수 데미지", "every beast attack hits ×1.5 (the four damage formulas /3 → /2)", crate::guest_build::beast_attack_power());
            // ★ 2026-10-12 인장 상급 (user: *"인장류의 상급 … 15~20 스탯 랜덤 부여"*): the alchemist rolls 15..20 for a seal of id 3 and
            //   charges 20,000 (the asset adds the five seals and the guardian drop). See `guest_anchors::seal_top_tier`.
            seal_top.apply(&mut *emulator, "인장 상급", "a seal of id 3 gives +15 … +20 and costs 20,000 to apply", crate::guest_build::seal_top_tier());
            combat.install(&mut *emulator);
            // ★ 2026-09-29 (user: *"맹독감염의 효과를 버프기로 바꾸고 기본공격시 중독을 거는 효과로"*
            //   · *"0.5프레임당 300데미지로"*): 맹독감염 = a buff whose 기본공격 poisons, the tick
            //   a share of the attack. See `Venom` / `guest_anchors::VenomPatch`.
            venom.install(&mut *emulator);
            // ★ 2026-09-30 (user: *"야수진화제가 판매가격은 5만원, 적은 7만원인데 이걸 팔떄는 만원,
            //   적은 12000원으로"*): the sell window's price per item — 야수진화제 백·흑 2,500,
            //   적 4,000 (2026-10-02, were 10,000 / 12,000), the rest price / 4 as shipped.
            //   See `ShopSell` / `guest_anchors::ShopSellPatch`.
            shop_sell.install(&mut *emulator);
            // ★ 2026-10-10 (user: *"버서커 무기에 투지의 구 발랐을떄 효과 발동시 보스몹한테 데미지 0뜨는 버그"* → 「2」):
            //   a boss crit (and every hit while 투지 is up) is never below the weapon's crit multiplier — the
            //   shipped ⌊crit % / 10⌋ made it 0 under 10 %. See `BossCrit` / `guest_anchors::BossCritPatch`.
            boss_crit.install(&mut *emulator);
            // ★★ 2026-10-21 (user: *"전체 보스들의 데미지중 일부를 방어무시 데미지로 … 20%"*): 20 % of a boss's hit passes the
            //   defence and the shield block. See `BossPierce` / `guest_anchors::BossPiercePatch`.
            boss_pierce.install(&mut *emulator);
            // ★ 2026-10-13 (user: *"이름이 긴 아이템들은 등급과 이름이 겹쳐져서 보이는 경우가 있는데 겹쳐지면 등급을
            //   두번째줄 우측끝으로"*): the info painter's grade goes to the empty line under a long name, right-aligned.
            //   See `GradeLine` / `guest_anchors::GradeLinePatch`.
            grade_line.install(&mut *emulator);
            // ★ 2026-10-02 (user: *"헬 장비드랍이 하드코어보다 조금 더 좋아야해"* · *"hell모드에서
            //   하드코어보다 장신구도 잘 나와야해"*): a HELL kill rolls the enemy's HELL-only slots once
            //   more — since the 드랍 티어 (same day, evening): 상급 gear by grade · 종결 accessory 0.3 % ·
            //   종결 gear 1 % · the new six behind the 난이도 gate (NORMAL+ since 2026-10-03). See `HellDrop` /
            //   `guest_anchors::HellDropPatch`.
            hell_drop.install(&mut *emulator);
            // ★ 2026-10-08 가디언 ②: the monster template wrapper (HELL LV 60..80 → 70..105, HP ×1.5, ATK/DEF ×1.2 over HELL) and the
            //   four level-correction hooks — one block, inert unless ag.i reads 3. See `Guardian` /
            //   `guest_anchors::GuardianPatch`.
            guardian.install(&mut *emulator);
            refine_protection.install(&mut *emulator);
            // ★ 2026-10-02 스킬을 바꾸는 장신구 6종: one block, once the SkillPower block has settled
            //   (two pieces chain to what the damage definitions hold then). See `Accessory` /
            //   `guest_anchors::AccessoryPatch`.
            accessory.install(&mut *emulator, skill_power.block.is_some() || skill_power.refused);
            // ★ 2026-10-05 (user: *"로그의 스킬을 한개 추가할꺼야. 일풍격"*): the rogue's seventh active — its slot
            //   a.p[15], the value 7 ↔ kind 15, the plan / the drive (which CHAIN to what the accessory block left in
            //   the two definitions — so after it). See `Ilpung` / `guest_anchors::IlpungPatch`.
            ilpung.install(&mut *emulator, accessory.block.is_some() || accessory.refused);
            // ★ 2026-10-07 (user: 분노태세 → ON/OFF · *"도발이라는 액티브 스킬"*): the berserker block — its wrappers chain to the
            //   SkillPower / accessory blocks' words and its taken function hangs on the combat block's 철괴 hook. See
            //   `Berserker` / `guest_anchors::BerserkerPatch`.
            berserker.install(
                &mut *emulator,
                (skill_power.block.is_some() || skill_power.refused)
                    && (accessory.block.is_some() || accessory.refused)
                    && (combat.block.is_some() || combat.refused)
                    && (ilpung.block.is_some() || ilpung.refused),
                combat.block,
                accessory.block,
            );
            // ★ 의상 아바타 A4 (2026-10-01): count each round clear (`ag.y()V`) — the rewards
            //   are paid from the HUD poll. See `ClearHook` / `guest_anchors::ClearHookPatch`.
            clear_hook.install(&mut *emulator);
            // ★★ 2026-10-22 가디언 레벨: the level-up's wrapper — GL 1..=250 after LV 99. See `GuardianLevel`.
            guardian_level.install(&mut *emulator);
            // ★ 2026-10-02: count the game's cooldown ticks (`ag.i()V`) — a skill the plate
            //   window takes off the bar keeps cooling by them. See `CooldownClock`.
            cd_clock.install(&mut *emulator);
            // ★★ 2026-10-16: an item's own icon cell (엘릭서). See `ItemIcon`.
            item_icon.install(&mut *emulator);
            // ★ 2026-10-12: the slot screen's cursor on the slot last played. See `SlotFocus`.
            slot_focus.install(&mut *emulator);
            // ★ 2026-10-05: a buff recast while it is still up restarts its duration (`g.a(BI)V`
            //   refused it as shipped). See `BuffRefresh` / `guest_anchors::BuffRefreshPatch`.
            buff_refresh.install(&mut *emulator);
            // ★ 2026-10-08 밤 「가디언」 장비의 개성 옵션 13: one block whose wrappers chain to whatever the SkillPower ·
            //   combat · accessory · 일풍격 · berserker · buff-refresh blocks left in the SP / damage / buff / strike words
            //   and 철괴's hook word — so after all of them — and which the guardian and venom blocks point at. See
            //   `Gear` / `guest_anchors::GearPatch`.
            gear.install(
                &mut *emulator,
                (skill_power.block.is_some() || skill_power.refused)
                    && (combat.block.is_some() || combat.refused)
                    && (accessory.block.is_some() || accessory.refused)
                    && (ilpung.block.is_some() || ilpung.refused)
                    && (berserker.block.is_some() || berserker.refused)
                    && (buff_refresh.block.is_some() || buff_refresh.refused)
                    && (guardian.block.is_some() || guardian.refused)
                    && (venom.block.is_some() || venom.refused)
                    && (hell_drop.block.is_some() || hell_drop.refused)
                    && (cd_clock.block.is_some() || cd_clock.refused)
                    // ★★ 2026-10-14 the cooldown tables `<job>.a(I)S` too — after the rogue block (installed below, so a frame later):
                    //   the gear wrapper is then the outermost on `ay.a(I)S` and 시간의 외투 cuts 살상연희 · 개화 as well
                    && (rogue.block.is_some() || rogue.refused),
                combat.block,
                guardian.block,
                venom.block,
                cd_clock.block.map(|b| b + crate::guest_build::CooldownClockPatch::COUNT),
                berserker.block,
                ilpung.block,
            );
            // ★ 2026-10-06 (user: *"번개난타를 … 앞3칸(3방향 각1칸씩)으로 범위기로"* · *"로그는 투명한 몬스터들은 처음 겜
            //   시작부터 볼 수있는거야"*): the rogue's 번개난타 three tiles and 심안's half see-through ghosts. See `Rogue` /
            //   `guest_anchors::RoguePatch`.
            rogue.install(&mut *emulator, ilpung.block.is_some() || ilpung.refused);
            // ★ 2026-10-11: no rogue teleport while 맵 18's three corpses fence the pass — the outermost wrapper on the rogue
            //   usable check, so after the rogue block. See `TeleportGate` / `guest_anchors::TeleportGatePatch`.
            teleport_gate.install(&mut *emulator, rogue.block.is_some() || rogue.refused);
            // ★ 2026-10-08 천공의검's picture: the knight's prongs · cracks · flash, drawn on the character draw — chained
            //   after the rogue block (the last to wrap that word). See `SkyDraw` / `guest_anchors::SkyDrawPatch`.
            //   ★ 2026-10-09 and 격분's blood on the HUD painter — it reads the berserker block's record of each blast.
            sky_draw.install(
                &mut *emulator,
                (rogue.block.is_some() || rogue.refused) && (berserker.block.is_some() || berserker.refused),
                rogue.block,
                berserker.block,
            );
            // ★★ 2026-10-13 「악마화」 D1: the 악의 block — the OUTERMOST wrapper on the beast key cast · the key arming · the
            //   three class drives · the character update · the beast SP / cooldown tables, and the last piece in 철괴's hook
            //   word — so after every block above that wraps one of those (the gear block the last of them). See `Demon` /
            //   `guest_anchors::DemonPatch`.
            demon.install(
                &mut *emulator,
                (gear.block.is_some() || gear.refused)
                    && (sky_draw.block.is_some() || sky_draw.refused)
                    && (teleport_gate.block.is_some() || teleport_gate.refused)
                    && (berserker.block.is_some() || berserker.refused)
                    && (combat.block.is_some() || combat.refused),
                combat.block,
                gear.block,
                berserker.block,
                accessory.block,
            );
            // ★ 의상 아바타 (2026-09-30): the parts loader's body / arms / legs choice, the
            //   host's when it says so. See `Avatar` / `guest_anchors::AvatarPatch`.
            avatar.install(&mut *emulator);
            // ★ 의상 아바타 A3: the EQUIPMENT cells to the left third and the cursor switch — once
            //   the loader is in (the avatar side has nothing to show without it).
            equip_avatar.install(&mut *emulator, avatar.block.is_some());
            // ★ #13 (2026-09-21): the inventory paging patch — seven `bl` retargets, seven
            //   cells and three trampolines in a host block, once, verified first. See
            //   `BagPaging::install` / `guest_anchors::InventoryPagePatch`.
            bag_paging.install(&mut *emulator);
            // ★★ 2026-10-20: …and 사용하기's `a.g(B)V` call, which reads the paging context (`ItemUsePagePatch`).
            bag_paging.install_use(&mut *emulator);

            // Host-owned cash-shop buffs. Re-asserted the same way as auto-loot —
            // read, compare, write only on a mismatch — which makes it idempotent
            // across New Game / Continue / death / map change with no ordering logic,
            // AND makes expiry self-cleaning: once the week is up the helpers ask for
            // the ORIGINAL halfword and put the shipped instruction back.
            //
            // ★ While no buff is live both helpers find what they want already in
            // place and return `None`, so this block performs ZERO guest writes for a
            // player who has never used one of the three items.
            let now = now_ms();
            let relief = buffs.relief_levels(now);
            let mult = buffs.exp_multiplier(now);
            let relief_patch = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::equip_relief_patch(&read, relief)
            };
            if let Some((a, bytes)) = relief_patch {
                let ok = emulator.write_memory(a, &bytes);
                log::info!("equip level relief: {a:#x} <- -{relief} levels (write ok = {ok})");
            }
            let exp_patch = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::exp_multiplier_patch(&read, mult)
            };
            if let Some((a, bytes)) = exp_patch {
                let ok = emulator.write_memory(a, &bytes);
                log::info!("exp multiplier: {a:#x} <- x{mult} (write ok = {ok})");
            }
        }

        // 4d) Host-driven save. Polled EVERY frame while a job is in flight: each step
        //     is a queued guest call that lands on the next `System::tick`, and holding
        //     a serialised heap buffer for longer than necessary is the one thing that
        //     could go wrong here. The autosave timer only ever ARMS a job — the gate
        //     inside `save_begin` is what decides whether it may actually run, and it is
        //     the game's own "may I save here" byte.
        save_poll(&mut *emulator, &mut saving, &mut backing_up, &item_proxy);
        backing_up.poll(&mut *emulator);
        // ⛔ AUTOSAVE STAYS OFF ON hero2 (2026-09-19). The one-call save is wired for the
        // SAVE BUTTON, which is a thing the player asked for by pressing it. An autosave
        // is different in kind: it overwrites the player's slot on a timer, and hero2 is
        // a game with three slots and no separate autosave slot, so a timer that fires
        // just after a bad fight replaces a good save with a worse one without anyone
        // asking. Turning it on is a decision for the player, not a side effect of
        // porting the button.
        let autosave_ok = crate::guest_build::hero2_save().is_none();
        if autosave_ok && frame % 30 == 0 && !saving.busy() {
            let now = now_ms();
            if save::autosave_due(saving.last_ms, now, save::AUTOSAVE_MS) {
                // Refusal is normal and silent: in a town shop, mid-cutscene or with a
                // menu open the game itself would not save either. The timer simply
                // tries again on the next tick until the player is back on the field.
                save_begin(&mut *emulator, &mut saving, save::Reason::Auto, now);
            }
        }
        // 4d2) Queued save retry — a manual tap that hit a busy/transient refusal, or a
        //      cash-item use waiting for the inventory to close. See `SaveState::retry`.
        if frame % 10 == 0
            && saving.job.is_none()
            && let Some((reason, until)) = saving.retry
        {
            let now = now_ms();
            if save_begin(&mut *emulator, &mut saving, reason, now) {
                saving.retry = None;
            } else if now > until {
                saving.retry = None;
                log::info!("save: queued {reason:?} retry expired unfulfilled");
                if reason == save::Reason::Manual {
                    host_notice("저장불가");
                }
            }
        }

        // 4d3) Skill-cast watch: dozens of cheap reads only while a tap is pending —
        //      see `SkillWatch` for what confirms and why only a confirmed cast may
        //      start a cooldown.
        if let Some(w) = skill_watch.as_ref() {
            let (sp_now, active) = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                (items::read_stats(&read).map(|s| s.sp), items::skill_cast_active(&read))
            };
            let spent = matches!((w.sp0, sp_now), (Some(before), Some(after)) if after < before);
            let active = active && !w.active0; // rising edge only — see `SkillWatch::active0`
            if spent || active {
                let _ = item_proxy.send_event(UserEvent::SkillCast { slot: w.slot });
                log::info!("skill watch: slot {} cast confirmed (sp spent={spent}, state={active})", w.slot);
                skill_watch = None;
            } else if Instant::now() >= w.deadline {
                // The game's own gates swallowed it (not idle / no SP / out of range):
                // no cast, so no cooldown — exactly the reported bug's fix.
                log::info!("skill watch: slot {} tap never became a cast — no cooldown", w.slot);
                skill_watch = None;
            }
        }

        // 4d5) Every 5 frames with the HUD poll.
        if frame % 5 == 2
            && let Some(h2) = crate::guest_build::hero2_java()
        {
            // 경험치 배율: keep the guest's multiplier word equal to the setting (and
            // install the redirect the first time the setting asks for more than x1).
            exp_rate.poll(&mut *emulator);
            // 드랍률: the equipment-drop filter's KEEP word at the setting (installed the first time it asks for less than 높음).
            drop_rate.poll(&mut *emulator);
            let current = emulator.read_memory(0x12ce9a, 2);
            if let Ok(current) = <[u8; 2]>::try_from(current.as_slice()) {
                if let Some(bytes) = crate::respawn::patch(current, crate::respawn::rate()) {
                    emulator.write_memory(0x12ce9a, &bytes);
                }
            }

            // 난이도: every monster on the stage at the setting's multiplier — new spawns
            // within a poll, a changed setting on the same poll it is seen.
            difficulty.poll(&mut *emulator, boss_raid.model.lap());
            // Ground drops: refill each one's own countdown until it has lived ~60 s.
            drop_life.poll(&mut *emulator);
            // The warehouse bags exist from the first in-world frame on; the windows they
            // open in are the player's to ask for.
            let in_game = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                items::in_game_state(&read)
            };
            if in_game {
                account.ensure_bags(&mut *emulator, &data_dir);
                // 기본공격: repair a character whose combo stage 1 an early 망각의 구 스킬
                // cleared. In here rather than beside `exp_rate` because it writes a
                // CHARACTER field and must see a character that is actually in the world.
                combo_repair.poll(&mut *emulator, &h2, &item_proxy);
                // ★ 2026-10-03 발도: a knight whose 방어의지 is open gets the 발도 slot opened too
                //   (the unlock the quest would have scripted). Same kind of write as the repair.
                iaido_unlock.poll(&mut *emulator, &h2);
                // ★ 2026-10-05 일풍격: a rogue whose 번개난타 is open (「터줏대감」) gets the slot a.p[15] opened.
                ilpung_unlock.poll(&mut *emulator, &h2);
                // ★ 2026-10-03: a knight's born 「중장비」 that an earlier skill reset zeroed. ★ 2026-10-06: and the
                //   rogue's 「심안」 with its one-time 독 연마 refund — the book's receipt says whether it was paid.
                born_skill_repair.poll(&mut *emulator, &h2, &mut avatar_book, &avatar_io);
                // ★ 2026-10-07 (2): the berserker's 몸통치기 retired — locked, its points back once (the book's receipt).
                bodyslam_retire.poll(&mut *emulator, &h2, &mut avatar_book, &avatar_io);
            }
            // 의상 아바타 A2: a slot whose `/g` the game just rewrote gets the character's
            // costume record committed with it; then the worn costume is the loader's override.
            // ★★ 2026-10-22 가디언 레벨 (review P1): the GL the save was made at, first — the commit takes it.
            if let Some(gl) = guardian_level.save_snapshot(&mut *emulator) {
                avatar_book.note_save_gl(&avatar_io, gl);
            }
            avatar_book.poll(&avatar_io);
            // ★ A3b: the class names the weapon / off-hand sheets (`hero/6/<type><nn>`).
            let class = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_char_class(&read, &h2)
            };
            // ★ 2026-10-02: the worn accessory (equipment slot 2) and the class → the block's ACTIVE.
            accessory.poll(&mut *emulator, in_game, class);
            // ★ 2026-10-11: 맵 18's corpses (event 0x10 on, 0x11 off) → the rogue's teleport kinds refused (after the
            //   accessory: 일섬 / 극사 count only while worn).
            teleport_gate.poll(&mut *emulator, in_game);
            // ★ 2026-10-08 밤: the worn guardian weapon (slot 3) · accessory (slot 2) and the class → the gear block's OPTS.
            // ★★ 2026-10-14 신규 아이템 2차 D: the worn 살상연희 weapons / ring → the rogue block's gear words.
            rogue.poll(&mut *emulator, in_game, class);
            if gear.poll(&mut *emulator, in_game, class, demon.block) {
                // ★ 2026-10-09 (3) 현자의 인장: the guest's four counters are −1 already; the skills off the bar too.
                window.forget_all_parked();
            }
            // ★★ 2026-10-16 악의 무기 M1 광마의 대검: 격분's hits take their ticks off 악의's cooldown — the seated cell's counter, else
            //   its parked one.
            let malice = gear.take_malice_ticks();
            if malice > 0 {
                let plan = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    crate::guest_build::h2_malice_value(&read, &h2).map(|v| (v, crate::guest_build::h2_cooldown_cut_write(&read, &h2, v, malice)))
                };
                match plan {
                    Some((_, Some((at, new)))) => {
                        java_write(&mut *emulator, crate::guest_build::JavaWrite::Cooldown, at, &new.to_le_bytes());
                    }
                    Some((v, None)) => window.shorten_parked_value(v, malice),
                    None => {}
                }
            }
            // ★★ 2026-10-14 (2차 B1 시간도둑): the seconds the block took off the guest's counters, off the parked ones too.
            let thief = gear.thief_ticks(&*emulator);
            if thief > 0 {
                window.shorten_parked(thief);
            }
            // ★ 2026-10-02 저녁: the HELL bonus roll's 난이도 gate (NORMAL+ since 2026-10-03) follows the setting.
            hell_drop.poll(&mut *emulator);
            // ★ A4: a round cleared since the last poll pays its mode's set (for this class).
            clear_hook.poll(&mut *emulator, &mut avatar_book, &avatar_io, &equip_avatar.catalog, &avatar_sheets, class);
            // ★ A5: the quest marks — each quest the game holds done takes the 난이도 it was done
            //   at (the HELL 수영장 파티 reward asks for 95 % of the round at NORMAL or above). AFTER the
            //   clear above: the next round's entry clears the game's quests in the same tick that
            //   runs `ag.y()V`, and the clear must judge the marks of the round it ended.
            if in_game {
                let done = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    crate::guest_build::hero2_save().and_then(|save| crate::guest_build::h2_quests_done(&read, &save.area))
                };
                if let Some(done) = done {
                    avatar_book.observe_quests(&avatar_io, &done, crate::difficulty());
                }
            }
            // ★★ 2026-10-13 「악마화」 D2: the demon's LOOK — the body sheet 악의 wants, LOOK up only once it is in,
            //   down before the human one comes back (`Demon::poll`), BEFORE the avatar's wish and poll below.
            //   ★★ 2026-10-15: its own 「in the world」 — the state word reads 1 for ~20 frames between two maps and 악의 ended at
            //   every exit when that counted as out (`h2_demon_in_world`: only the title shell or no character is out).
            let demon_world = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_demon_in_world(&read, &h2)
            };
            demon.poll(&mut *emulator, demon_world, class);
            demon.poll_skin(&mut *emulator); // ★★ 2026-10-16: the beast window's names · icons in their 악의 versions while it runs
            avatar.set_wanted(crate::avatar_state::costume_parts(avatar_book.worn(), |p, n| avatar_sheets.has(p, n, class)));
            // 의상 아바타: the override words, and the game's own reload of a part whose
            // costume changed while the character is in the world — ★ 2026-10-01 or whose
            // equipment the EQUIPMENT window just changed (its character follows it live).
            let equip_window = equip_avatar.view(&*emulator).is_some();
            avatar.poll(&mut *emulator, in_game, equip_window, class, &avatar_sheets);
        }
        // 4d6) The ACCOUNT calls in flight, every frame: a window should appear on the
        //      tick after the tap, not up to five later.
        //      ★ #13: then the inventory page context, kept in step with the window, and the
        //      strip the UI draws over it. ★ 2026-09-29: every frame (it was every fifth) and
        //      AFTER the account calls — the strip now stands under the game's box, and a box
        //      the host hung this frame (the 개수 box, 창고에 넣었습니다) is painted on the next
        //      tick: said here, the UI knows where it is before that frame arrives. On the
        //      field that is five word reads (the state word says there is no chain to walk);
        //      in a menu, the chain; the writes and the UI event happen only on a change.
        if let Some(h2) = crate::guest_build::hero2_java() {
            let visit = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                crate::guest_build::h2_flight_visit_write(&read)
            };
            if let Some((addr, bytes)) = visit {
                java_write(&mut *emulator, crate::guest_build::JavaWrite::TownVisits, addr, &bytes);
            }
            account.poll(&mut *emulator, &h2, &data_dir);
            bag_paging.poll(&mut *emulator, &h2, &item_proxy, account.bags);
            // ★★ 2026-10-22 가디언 레벨: every frame — the record follows the block's GL closely enough that a save made
            //   the frame after a GL-up carries it with its points.
            guardian_level.poll(&mut *emulator, &h2, &mut avatar_book, &avatar_io);
        }

        // 4d3b) ★ 2026-09-29 (user: "스킬을 배운뒤 퀵슬롯 등록하고 저장하지않고 죽으면 … 퀵슬롯에
        //      스킬이 남아있어"): the game saved → the plate rows are committed with it, and
        //      the next world entry hands back exactly these (`Pages::adopt_owner`). Only
        //      when the save is the OWNER's own — in the world, and the character the guest
        //      holds is the one the rows belong to — so a new game's first write (the shell
        //      edge that adopts the new owner has not run yet) cannot stamp the previous
        //      character's unsaved rows into its saved block.
        {
            let edges = crate::vault::save_edges();
            if edges != pages_edges_seen {
                pages_edges_seen = edges;
                let live_owner = crate::guest_build::hero2_java().and_then(|h2| {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    if items::in_game_state(&read) { items::h2_owner(&read, &h2) } else { None }
                });
                if live_owner.is_some() && live_owner == pages.owner() {
                    let changed = pages.commit();
                    pages_dirty = true;
                    log::info!("quickpage: the game saved — plate rows committed for {:?} ({})", pages.owner(), if changed { "changed" } else { "unchanged" });
                } else {
                    log::info!("quickpage: a save edge outside the owner's world (rows {:?}, live {:?}) — not committed", pages.owner(), live_owner);
                }
            }
        }

        // 4d3c) ★★ 2026-10-28 세이브 행: the file screens' answers, the box being built — and, while
        //       세이브내보내기 waits (for the game's save in the world; at the title and where the game may
        //       not save, for nothing but the commits — code review P1), whether every host commit riding
        //       on the saves (the warehouse blobs, the costume record bound to the new `/g`, the plate
        //       rows) has reached the disk: the file must be one moment.
        //       ★ 2026-10-28 (code review, second pass P1): and published every loop — the 진단 정보 export
        //       runs on the UI thread, outside this wait, and `vault::export_bundle` reads it for both.
        {
            let account_settled = account.commit_settled();
            let avatar_settled = avatar_book.caught_up(&avatar_io);
            let pages_settled = config::quickpages_write_ok().then_some(!pages_dirty && pages_edges_seen == crate::vault::save_edges());
            crate::vault::publish_host_commits(match (account_settled, avatar_settled) {
                (None, _) => crate::vault::HostCommits::AccountFailed,
                (_, None) => crate::vault::HostCommits::AvatarFailed,
                (Some(true), Some(true)) if pages_settled == Some(true) => crate::vault::HostCommits::Settled,
                // A plate store whose write failed is `export_bundle`'s own refusal (`quickpages_write_ok`).
                _ => crate::vault::HostCommits::Pending,
            });
            let settled = save_xfer.owns_input().then(|| XferSettled { save: saving.java_outcome, account: account_settled, avatar: avatar_settled, pages: pages_settled });
            save_xfer.poll(&mut *emulator, &data_dir, &item_proxy, settled);
        }

        // 4d4) ★ #13: the inventory's capacity, kept at 127 (`Hero2Java::BAG_CAPACITY`).
        //      The game constructs every bag at 28 and its load path builds a fresh bag
        //      (HERO2_GUEST_RE.md §2.7.7: "역직렬화가 용량을 안 덮어씀 — 로드 후 28 로 복귀"),
        //      so this is not a boot-time write but a standing one: four reads a second,
        //      one byte written on the boot, on every load and on a new game. Not while
        //      the shell is up — the bag is not the player's there.
        //      ⛔ ONLY while the paging patch is in (2026-09-21, review). The capacity and
        //      the paging are one feature: `bq.a(Lch;B)I` refuses only at `size > b`, so a
        //      bag raised to 127 with the trampolines absent keeps accepting pickups into
        //      records 28..126 that the 7x4 window cannot draw, the cursor cannot reach,
        //      and therefore nothing can use, drop or sell — and `bq.a()[B` then writes
        //      them into `n<slot>`, where they sit for good. Paging off is a state this
        //      code reaches by itself (no guest block, a site that does not read as
        //      shipped, a patch that did not verify), and leaving the bag at the game's
        //      own 28 is strictly better than either half of the feature alone.
        //      ★ 2026-10-09 (user: *"재료가방창도 127칸으로 만들꺼고"*): the game's 재료가방 `ag.b` too — the same class,
        //      the same constructor 28, the same load path (`ag.p()V` builds both), and since this date the same
        //      trampolines page its window (`BagPaging::bags`). Same rule: only while the paging patch is in.
        if frame % 30 == 3
            && bag_paging.paging_on()
            && let Some(h2) = crate::guest_build::hero2_java()
        {
            for slot in [h2.inventory_slot, h2.material_bag_slot] {
                let head = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    if items::in_game_state(&read) { crate::guest_build::h2_bag_head(&read, slot) } else { None }
                };
                if let Some(head) = head
                    && let Some((addr, cap)) = crate::guest_build::h2_bag_capacity_write(&head)
                {
                    if java_write(&mut *emulator, crate::guest_build::JavaWrite::BagCapacity, addr, &[cap]) {
                        log::info!("bag: capacity {} -> {cap} on {:#x} ({} record(s), slot {slot:#x})", head.b, head.handle, head.count);
                    }
                }
            }
        }

        // ★ Not while the item-use drive owns the screen: the guest is in a menu the
        //   player is not shown, and publishing that would flip the HUD to menu mode for
        //   a few frames (plates gone, arrows up) — the very flicker the hold prevents.
        if frame % 5 == 0 && !drive_owns {
            let (field, cutscene, choice, map_open, shell, bottom_overlay, can_save_raw, slots, stats) = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                let field = items::in_field_state(&read);
                // The game's own save-inhibitor byte: false marks a no-save AREA (boss
                // room), where the UI hides the SAVE button and the minimap.
                let can_save = items::can_save_here(&read);
                // ★ SCRIPT window (id 4) vs any other window: this is what makes
                //   "cutscene or menu?" answerable, and it is why a dialogue gets the
                //   OK + `#` pair instead of the menu-navigation set.
                let cutscene = items::cutscene_open(&read);
                // ★ …and the one bit that window id CANNOT give: is this script window
                //   showing a CHOICE the player has to answer? (2026-08-17 — the arrows
                //   come back for that and only that.) See `items::script_choice_open`.
                let choice = items::script_choice_open(&read);
                let map_open = items::map_window_open(&read);
                let shell = items::shell_state(&read);
                let bottom_overlay = items::bottom_overlay_open(&read);
                // ★ Scanned for the whole of app state 4, not just field free-roam:
                //   the plates stay on screen with a game window open (2026-08-03), so
                //   an empty list there would blank them the moment a menu opened.
                //   Boot and the title shell are still excluded — CHAR is not a live
                //   character there.
                let slots = if !items::in_game_state(&read) {
                    Vec::new()
                } else if let Some(h2) = crate::guest_build::hero2_java() {
                    // ★ The hidden-plate set, into the store, before the capture reads
                    //   it (hero3 82차): a value the game bound lands on a VISIBLE empty
                    //   plate first. Cheap: a relaxed atomic load and a byte store.
                    pages.set_hidden(crate::quick_hidden());
                    // ★ hero2 (S3): reconcile the plate store with the live bar first —
                    //   the store, not the bar, is what the plates show.
                    if let Some(live) = crate::guest_build::h2_quickbar_bytes(&read, &h2) {
                        if pages.capture(&mut window, live) {
                            pages_dirty = true;
                            log::info!("quickpage: capture changed the plates from bar {live:?}: {:?}", pages.plates());
                        }
                    }
                    // ★ 2026-09-26 (user: "퀵슬롯에 등록한 회복아이템을 다쓰면 퀵슬롯에서
                    //   제거되어야하는데 … 클릭하면 회복아이템 없음 이라고 떠"): an item plate
                    //   whose type is gone from the bag leaves the bar, on every page.
                    //   ⛔ Only in the field, only when the stored rows are THIS
                    //   character's (the owner swap runs after this sample, so the first
                    //   poll out of the slot list would otherwise judge the last
                    //   character's plates by the new one's bag), and only after
                    //   `ITEM_GONE_POLLS` straight reads of a readable bag with none of it
                    //   — a bag caught between the loader's `new bq` and its fill reads
                    //   empty for a moment, and that must never cost a plate.
                    if items::in_field_state(&read) && pages.owner().is_some() && pages.owner() == items::h2_owner(&read, &h2) {
                        let mut kinds: Vec<u8> = (0..pages.count()).filter_map(|i| pages.page(i)).flat_map(|p| p.iter().filter_map(|e| e.item())).collect();
                        kinds.sort_unstable();
                        kinds.dedup();
                        let inv = if kinds.is_empty() { None } else { crate::guest_build::h2_inventory(&read, &h2) };
                        if let Some(inv) = inv {
                            for kind in kinds {
                                let have = crate::guest_build::h2_item_stock(&inv, kind).is_some_and(|(_, n)| n > 0);
                                let Some(run) = item_gone.get_mut(usize::from(kind)) else { continue };
                                *run = if have { 0 } else { run.saturating_add(1) };
                                if *run >= ITEM_GONE_POLLS && pages.forget_item(kind) {
                                    *run = 0;
                                    pages_dirty = true;
                                    log::info!("quickpage: item type {kind} is gone from the bag — its plate is cleared");
                                }
                            }
                        }
                    }
                    // ★ 2026-09-29: a plate holding a skill the character does not HAVE —
                    //   its value on no seat, its level 0 / −1 in `a.p` or `bn.k` — leaves
                    //   the bar, on every page. The rollback at the world entry covers the
                    //   no-save reload; this covers what that cannot see: a 세이브 복원 to an
                    //   older generation, a 망각의 구 reset with rows behind the live page.
                    //   Same guards and the same run as the item rule above; a seated plate
                    //   is the game's own bar and is never questioned. The character is read
                    //   only while some unseated plate holds a value at all.
                    if items::in_field_state(&read) && pages.owner().is_some() && pages.owner() == items::h2_owner(&read, &h2) {
                        if pages.unlearned_plates(&window, |_| true).is_empty() {
                            skill_gone = [0u8; quickpage::BEAST_MAX as usize + 1];
                        } else if let Some(learned) = crate::guest_build::h2_learned(&read, &h2) {
                            let gone = pages.unlearned_plates(&window, |v| learned.holds(v) == Some(false));
                            for (v, run) in skill_gone.iter_mut().enumerate() {
                                *run = if gone.iter().any(|g| usize::from(g.2) == v) { run.saturating_add(1) } else { 0 };
                            }
                            for (page, plate, v) in gone {
                                if skill_gone.get(usize::from(v)).is_some_and(|r| *r >= SKILL_GONE_POLLS) && pages.clear_plate_on(page, plate) {
                                    pages_dirty = true;
                                    log::info!("quickpage: value {v} is not a skill this character has — plate {plate} of page {page} cleared");
                                }
                            }
                        }
                    }
                    items::h2_plate_slots(&read, &h2, &pages, &window, cd_clock_now(&*emulator))
                } else if frame % 15 == 0 {
                    items::scan_quickbar(&read)
                } else {
                    last_slots.clone()
                };
                (
                    field,
                    cutscene,
                    choice,
                    map_open,
                    shell,
                    bottom_overlay,
                    can_save,
                    slots,
                    // ★ 2026-09-29: a head that did not read this time keeps the last one —
                    //   the portrait must not blink empty on one missed pointer hop.
                    items::read_stats(&read)
                        .map(|mut s| {
                            s.head = s.head.or(last_stats.head);
                            // ★★ 2026-10-22 가디언 레벨: the GL number and the GL bar.
                            if let Some(h2) = crate::guest_build::hero2_java() {
                                guardian_level.overlay(&read, &h2, &mut s);
                            }
                            s
                        })
                        .unwrap_or(last_stats),
                )
            };
            // ★ DEBOUNCED (2026-08-08, player: "갑자기 미니맵이랑 save 버튼이 없어졌어 ... 잠시동안
            // ... 보스방에서만 없어져야 하는데"). 27차 hides the SAVE button AND the minimap the
            // instant the game's save-inhibitor byte reads non-zero, and that byte is reached
            // through a resolved CHAR pointer — a single sample taken mid-transition is enough
            // to blank both controls on an ordinary field.
            //
            // ASYMMETRIC on purpose: hiding waits for [`NO_SAVE_DEBOUNCE`] consecutive samples,
            // showing again is immediate. A boss room lasts minutes, so a ~2 s delay before the
            // controls go is invisible; the flicker it removes is what the player actually saw.
            // This does NOT decide whether the byte is the right signal — the `save gate:` log
            // below is what will answer that, with a place and a time.
            if can_save_raw {
                no_save_run = 0;
            } else {
                no_save_run = no_save_run.saturating_add(1);
            }
            // `HIDE_UI_ON_NO_SAVE` is false this round, so this is `true` always and the
            // SAVE button and the minimap stay on screen. See the constant.
            let can_save = !HIDE_UI_ON_NO_SAVE || can_save_raw || no_save_run < NO_SAVE_DEBOUNCE;

            // The live buff readout the menu HUD shows. Minute-granular, so this
            // only triggers a publish when a buff starts, ends, or ticks a minute.
            let buff_lines = buffs.status_lines(now_ms());
            // Always publish the FIRST sample, even when it equals our own initial
            // values: the UI cannot distinguish "nothing sent yet" from "nothing
            // changed", and both sides starting at a guessed default is how the
            // field HUD ended up drawn over the title screen.
            if !hud_published
                || map_open != last_map_open
                || shell != last_shell
                || bottom_overlay != last_bottom_overlay
                || field != last_field
                || cutscene != last_cutscene
                || choice != last_choice
                || can_save != last_can_save
                || slots != last_slots
                || stats != last_stats
                || buff_lines != last_buff_lines
            {
                hud_published = true;
                // ★ Leaving the out-of-world SHELL (title / slot select) means the game
                // just loaded a save or started a new one, i.e. it rewrote the quickbar
                // from a source we cannot audit. Any shadow page still held is now stamped
                // with someone else's playthrough, so drop the lot and go back to page 0 —
                // which IS the guest's own bar, so the player loses only their page-2
                // arrangement. The alternative is silently re-binding skills from a
                // different save (agreed policy 2026-08-04).
                if last_shell && !shell {
                    // ★ hero2 (S3): the rows belong to (save slot, class). Same owner keeps
                    //   them and is re-seated from the bar the load just produced (zero
                    //   writes); another owner's rows are parked and theirs come back.
                    let owner = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::hero2_java().and_then(|h2| items::h2_owner(&read, &h2))
                    };
                    // ★ 2026-09-27: …unless this is a NEW game. Then the slot's rows belong to
                    //   characters 새로하기 just deleted — a same-class restart is the same key
                    //   and would have been handed the old plates back. The slot is the game's
                    //   own `ag.c`, readable even when the character is not yet.
                    let new_game = new_game_watch.take();
                    if new_game {
                        let slot = {
                            let read = |a: u32, l: usize| emulator.read_memory(a, l);
                            crate::guest_build::hero2_java()
                                .and_then(|h2| read(h2.save_slot_slot, 1).first().copied())
                                .filter(|s| *s < crate::guest_build::SaveSlotsPatch::SLOTS)
                        };
                        match slot {
                            Some(s) => {
                                let dropped = pages.forget_slot(&mut window, s);
                                log::info!("quickpage: a NEW game in save slot {} — its old plate rows {}", s + 1, if dropped { "dropped" } else { "(there were none)" });
                            }
                            None => log::warn!("quickpage: a NEW game, but the save slot is unreadable — rows left as they are"),
                        }
                    }
                    skill_gone = [0u8; quickpage::BEAST_MAX as usize + 1];
                    match owner {
                        Some(o) => {
                            // ★ 2026-09-29: the rows of that owner's LAST SAVE — a rollback
                            //   for the same owner (what was registered since is gone with
                            //   the rest of the unsaved progress), a swap for another.
                            let adopted = pages.adopt_owner(&mut window, o);
                            log::info!(
                                "quickpage: left the shell as owner {o:?} -> rows {}",
                                match adopted {
                                    quickpage::Adopted::Kept => "kept (as saved)",
                                    quickpage::Adopted::RolledBack => "rolled back to the last save",
                                    quickpage::Adopted::Restored => "restored from that owner's last save",
                                    quickpage::Adopted::Fresh => "fresh (never saved)",
                                }
                            );
                            // ★ 2026-10-09: the page the loaded bar is from, before the first capture — on
                            //   any other page the capture copies that page's seats onto empty plates
                            //   (`quickpage::Pages::match_page`; the user's 「등록하지 않은 퀵슬롯」).
                            let live = {
                                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                                crate::guest_build::hero2_java().and_then(|h2| crate::guest_build::h2_quickbar_bytes(&read, &h2))
                            };
                            if let Some(live) = live {
                                let was = pages.current();
                                let now = pages.match_page(live);
                                if now != was {
                                    log::info!("quickpage: the loaded bar {live:?} is page {now}'s — showing it (was page {was})");
                                }
                            }
                        }
                        None => {
                            window.clear();
                            log::info!("quickpage: left the shell (owner unreadable) -> seats cleared");
                        }
                    }
                    pages_dirty = true;
                    let _ = item_proxy.send_event(UserEvent::QuickPage(pages.current() as u8));
                    // ★ 의상 아바타 A2: the character that just entered — a fresh one for
                    //   새로하기 (its first save replaces the slot's record), the loaded save's own
                    //   record for 이어하기 (resolved against the `/g` on disk, so a death and a
                    //   reload hand back exactly what was saved). File slots are 1-based.
                    let save_slot = {
                        let read = |a: u32, l: usize| emulator.read_memory(a, l);
                        crate::guest_build::hero2_java()
                            .and_then(|h2| read(h2.save_slot_slot, 1).first().copied())
                            .filter(|s| *s < crate::guest_build::SaveSlotsPatch::SLOTS)
                    };
                    // ★ 2026-10-12: this slot is now the last played — the slot screen opens on it (SlotFocus).
                    if let Some(s) = save_slot {
                        slot_focus.note(&mut *emulator, &data_dir, s);
                    }
                    match save_slot {
                        Some(s) => {
                            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
                            avatar_book.enter_world(&avatar_io, s + 1, new_game, seed);
                            // ★★ 2026-10-22 가디언 레벨: armed from this record's GL once the field shows the character.
                            guardian_level.enter_world();
                            // ★ A5: the starter set (클레르의 잠옷 202) — every character owns it from the
                            //   start: a new game here, a load of any save (one from before it existed
                            //   too) on its first entry.
                            let class = {
                                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                                crate::guest_build::hero2_java().and_then(|h2| crate::guest_build::h2_char_class(&read, &h2))
                            };
                            let starter: Vec<(u16, u8)> = crate::avatar_state::STARTER_SETS
                                .iter()
                                .flat_map(|&set| crate::guest_build::h2_av_set_slots_for(&equip_avatar.catalog, &avatar_sheets, set, class).into_iter().map(move |slot| (set, slot)))
                                .collect();
                            avatar_book.grant_starter(&avatar_io, &starter);
                            // ★ 2026-10-02 (배포 준비): what the loaded save had cleared before this build
                            //   — its mode and 달성도 (`g_progress`) — is paid as EASY clears, each with the
                            //   game's notice once the field is up. A new game has cleared nothing.
                            if !new_game {
                                let g = crate::avatar_state::AvatarIo::live_g(&avatar_io, s + 1);
                                match g.as_deref().and_then(crate::avatar_state::g_progress) {
                                    Some(progress) => {
                                        let class = class.or_else(|| g.as_deref().and_then(crate::avatar_state::g_class));
                                        let slots_for = |set: u16| crate::guest_build::h2_av_set_slots_for(&equip_avatar.catalog, &avatar_sheets, set, class);
                                        for reward in avatar_book.grant_earned(&avatar_io, progress, &slots_for) {
                                            clear_hook.notices.push(reward.notice_row);
                                        }
                                    }
                                    None => log::warn!("avatar: slot {} loaded but its /g does not read as a whole save — no earlier clears paid", s + 1),
                                }
                            }
                            // ★ 2026-10-16 보스 레이드: the temple's gate follows the character in the world — a new game
                            //   holds no receipt (and read no /g, which is what set it before this point on a load).
                            crate::raid_gate::set(!new_game && avatar_book.cleared_guardian_hard());
                        }
                        None => log::warn!("avatar: entered the world but the save slot is unreadable — no costume state"),
                    }
                }
                if shell && !last_shell {
                    // Back to the title / slot list: nothing is seated any more.
                    window.clear();
                    // ★ 의상 아바타 A2: nothing carries over to the next load.
                    avatar_book.leave_world(&avatar_io);
                    // ★★ 2026-10-22 가디언 레벨: nor does the GL — the block waits for the next character.
                    guardian_level.leave_world(&mut *emulator);
                    // ★ 2026-10-16 보스 레이드: nor does the temple's gate — the next load's /g read decides it again.
                    crate::raid_gate::set(false);
                    // ★ 2026-10-02: nor do the warehouse's unsaved moves — it goes back to the
                    //   last save with the character (`AccountState::reload_owed`).
                    account.leave_world();
                }
                // ★ 2026-08-08, player: "갑자기 미니맵이랑 save 버튼이 없어졌어 ... 보스방에서만
                // 없어져야 하는데 왜 필드에서?". 27차 uses the game's own save-inhibitor byte as
                // a stand-in for "boss room", and this says when that byte actually moves, so
                // the next report comes with a place and a time instead of a guess.
                if can_save != last_can_save {
                    log::warn!(
                        "save gate: canSave {last_can_save} -> {can_save} (raw={can_save_raw}, run={no_save_run}, field={field}, map_open={map_open}, cutscene={cutscene})"
                    );
                }
                last_field = field;
                last_cutscene = cutscene;
                last_choice = choice;
                last_map_open = map_open;
                last_shell = shell;
                last_bottom_overlay = bottom_overlay;
                last_can_save = can_save;
                last_slots = slots.clone();
                last_stats = stats;
                last_buff_lines = buff_lines.clone();
                let _ = item_proxy.send_event(UserEvent::Hud {
                    field,
                    cutscene,
                    choice,
                    map_open,
                    shell,
                    bottom_overlay,
                    can_save,
                    slots,
                    stats,
                    buff_lines,
                });
            }
            // The plate store changed under a capture or an owner change: persist it
            // now, outside the sample's borrows. ★ 2026-10-28 (code review P2): a write that did
            // not land stays owed and is tried again about once a second until one does — the store
            // on the disk is the previous one meanwhile (`write_quickpages` is temp + rename).
            if pages_dirty || (frame % 60 == 0 && !config::quickpages_write_ok()) {
                pages_dirty = !config::write_quickpages(&data_dir, &pages);
            }
        }

        // 4c2) The game's own context popup -> "퀵바등록".
        //
        //  ★ EVERY FRAME, not the HUD's ~12/s (player, 2026-08-06: "상세보기/1번등록/
        //  3번등록 박스가 한번 깜빡했다가 퀵슬롯등록 박스로 바뀐다"). The guest repaints the
        //  popup on every frame it is up, so the player sees the ORIGINAL list for as
        //  long as we take to rewrite it — at 5-frame polling that is 80-120 ms, which
        //  is exactly the flash being reported. One frame is the floor: the popup is
        //  composed inside the same tick that opens it, so the first repaint after our
        //  write is the earliest one that can be ours.
        //
        //  It is affordable because `popup_view` bails after ~3 reads when no menu
        //  overlay is up (`CHAR+0x9c6c == 0`), which is every frame of normal play.
        //
        //  The REWRITE is re-applied every sample rather than once on the open edge:
        //  the count byte `CHAR+0x9c85` is shared with other windows, so something else
        //  can put it back. `items::popup_relabel` returns nothing when the list is
        //  already ours, which is what makes that cheap.
        //
        //  ★ This writes into the guest's own label buffers, not over its pixels. The
        //    popup's screen anchor is NOT readable (draw_popup keeps x/y in its stack
        //    frame), so drawing over it is not an option — see items.rs.
        // Same gate for the popup rewrite: the drive's own item popup is not the
        // player's to register from, and its rows must stay the game's three.
        if POPUP_QUICKBIND && !drive_owns {
            // ★ #13 step 7: the MOVE row onto whatever bag-window popup is up, before the
            //   view is read — so the view that goes to the UI already has it.
            if let Some(h2) = crate::guest_build::hero2_java() {
                account.poll_popup(&mut *emulator, &h2);
            }
            let view = {
                let read = |a: u32, l: usize| emulator.read_memory(a, l);
                items::popup_view(&read)
            };
            if let Some(v) = view.as_ref() {
                let writes = {
                    let read = |a: u32, l: usize| emulator.read_memory(a, l);
                    items::popup_relabel(&read, v)
                };
                for (a, bytes) in &writes {
                    // hero2: the label bytes and the shape fields are two gate kinds (three
                    // with the item popup's handle slots, `PopupLabelRef`); the producer
                    // (`items::h2_popup_relabel`) only emits them when all are open, so one
                    // kind is enough here. hero3 keeps its ungated write.
                    if !java_write(&mut *emulator, crate::guest_build::JavaWrite::PopupLabel, *a, bytes) {
                        log::warn!("popup: relabel write failed at {a:#x}");
                    }
                }
                // ★ Only on the OPEN EDGE. This block now runs every frame, and 18차
                // measured what an unconditional log costs here: an `__android_log_write`
                // inside the frame budget, ~100/s, is a performance bug of its own.
                if !writes.is_empty() && last_popup.is_none() {
                    log::info!("popup: relabelled to 퀵슬롯등록 ({} write(s), kind {}{})", writes.len(), v.kind, if v.item { format!(", item type {}", v.item_kind) } else { String::new() });
                }
            } else if picking {
                // The popup closed under us while the plates were highlighted.
                picking = false;
                let _ = item_proxy.send_event(UserEvent::PickingSlot(false));
            }
            if view != last_popup {
                last_popup = view;
                let _ = item_proxy.send_event(UserEvent::Popup(last_popup));
            }
        }

        // 4c) MINIMAP. Rendered by the host straight from the guest's live map
        //     arrays — see the block comment on `items::render_minimap` for the
        //     decoded guest routine and the pixel-exact verification.
        //
        //     ★ 2026-08-03: this is the whole of it now. What used to be here was a
        //     six-phase state machine that pressed `*` for the player to make the
        //     guest repaint its own minimap buffer, froze the screen so the window it
        //     opened would not be seen, stashed and replayed the player's input while
        //     it ran, debounced a map-change signature to decide when to do it again,
        //     and gave up after three tries. Every player-reported minimap bug came
        //     out of that machinery. None of it was ever needed: the arrays the map
        //     window paints from are the field's own collision and warp data, live at
        //     all times, so the picture can simply be drawn.
        //
        //     ★ 2026-09-29: and AT ONCE when the map changed. The hero5-style view
        //     follows the dot, so the new map's position over the old map's tiles would
        //     scroll the OLD map to wherever the player arrived until the next scheduled
        //     render. A map load builds a new attribute grid, so its handle (three reads)
        //     is checked at the marker's cadence; `last_mm_grid` only advances on a
        //     render that produced a map, so a load caught half-way is retried.
        let map_changed = frame % MM_MARKER_EVERY == 0
            && crate::guest_build::hero2_save().is_some_and(|sv| {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                crate::guest_build::h2_stage_grid(&read, &sv.area) != last_mm_grid
            });
        if frame % MM_RENDER_EVERY == 7 || map_changed {
            let (mm, grid) = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                let grid = crate::guest_build::hero2_save().and_then(|sv| crate::guest_build::h2_stage_grid(&read, &sv.area));
                (items::render_minimap(&read, &mut mm_triggers), grid)
            };
            if mm.is_some() || grid.is_none() {
                last_mm_grid = grid;
            }
            if mm != last_minimap {
                last_minimap = mm.clone();
                let _ = item_proxy.send_event(UserEvent::MinimapFrame(mm));
            }
        }
        // 4c2) …and the dot on it, at its own (much faster) cadence. hero2 only: hero3
        //      stamps its marker into the picture, which is fine there because nothing
        //      has asked that port for a live one.
        if frame % MM_MARKER_EVERY == 0
            && let Some(sv) = crate::guest_build::hero2_save()
            && let Some(h2) = crate::guest_build::hero2_java()
        {
            let marker = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                crate::guest_build::h2_player_marker(&read, &sv.area, h2.char_slot)
            };
            if marker != last_marker {
                last_marker = marker;
                let _ = item_proxy.send_event(UserEvent::MinimapMarker(marker));
            }
            let map_no = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                crate::guest_build::h2_live_map(&read, &sv.area)
            };
            if map_no != last_stage_map {
                last_stage_map = map_no;
                if let Some(m) = map_no {
                    let _ = item_proxy.send_event(UserEvent::StageMap(m));
                }
            }
        }
        // 4c3) …and the MONSTERS on it (2026-09-29, player: the minimap *"레퍼런스처럼 +
        //      몬스터 점"*) — the stage's own entity list, at their own cadence.
        if frame % MM_MOBS_EVERY == 1
            && let Some(sv) = crate::guest_build::hero2_save()
        {
            let mobs = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                // ★ 2026-10-06 「심안」: a hidden ghost is a square for a rogue who sees it, for no one else.
                let see_ghosts = crate::guest_build::hero2_java().is_some_and(|h2| crate::guest_build::h2_sees_ghosts(&read, &h2));
                crate::guest_build::h2_monster_marks(&read, sv.area.stage_slot, see_ghosts)
            };
            if mobs != last_mobs {
                last_mobs = mobs.clone();
                let _ = item_proxy.send_event(UserEvent::MinimapMobs(mobs));
            }
        }
        // 4c4) ★ A3b: the status HUD's portrait — the head the game's loader loaded (a
        //      costume's head, a helmet in its own colours), facing the viewer. Two word
        //      reads a poll; the head itself is read again only when the parts table's head
        //      array or the class animation changes (a reload, a helmet swap, a new class).
        if frame % 5 == 4 && crate::guest_build::hero2_java().is_some() {
            let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
            let head = crate::guest_build::h2_parts_handles(&read).and_then(|h| h.first().copied()).filter(|h| *h != 0);
            let anim = crate::guest_build::h2_static_handle(&read, crate::guest_build::Hero2Java::CLASS_ANIM_SLOT);
            let key = head.zip(anim);
            if key != portrait_key {
                if anim.is_some() && portrait_blob.as_ref().map(|b| b.0) != anim {
                    portrait_blob = crate::guest_build::h2_class_blob(&read);
                }
                let live = key.and(portrait_blob.as_ref()).and_then(|(_, b)| crate::guest_build::h2_live_head(&read, b));
                // A head array without a readable head (a reload caught half way) is tried again
                // on the next poll rather than remembered as "no portrait".
                if key.is_none() || live.is_some() {
                    portrait_key = key;
                }
                if live != last_portrait {
                    last_portrait = live.clone();
                    let _ = item_proxy.send_event(UserEvent::Portrait(live));
                }
            }
        }
        // 4c4) ★ 2026-10-09 the awakenings' voices (user: *"격분 사용시 그아앗! … 격분 폭발 시 갈! … 천공의 검 … 하늘이
        //      열린다! … 개화 … 꽃이 핀다..."*): the edges every frame — the class byte, the action's kind and 격분's two
        //      block words, a few reads — and a cue goes to the effects' own queue (`audio_sink::play_host_clip`) at the
        //      game's sound level (`bs.b:B`; 0 = silent like its effects).
        if let Some(h2) = crate::guest_build::hero2_java() {
            let (class, action, fury, level) = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                (
                    read(h2.class_slot, 1).first().copied(),
                    crate::guest_build::h2_action_kind(&read, h2.char_slot),
                    berserker.block.and_then(|b| crate::guest_build::h2_berserker_fury(&read, b)).map(|w| (w.state, w.exploding)),
                    read(crate::guest_build::Hero2Java::SOUND_LEVEL_SLOT, 1).first().map(|b| *b as i8),
                )
            };
            // ★ 2026-10-13 악의's roar: the demon block's STATE edge
            let demon_state = demon.state(&*emulator);
            if let Some(cue) = voice_watch.step(class, action, fury, demon_state) {
                let pcm = level.and_then(|lv| crate::voice::voice_pcm(cue.index(), lv));
                let played = pcm.is_some_and(|(c, r, s)| crate::audio_sink::play_host_clip(c, r, s));
                log::info!("voice: {} ({cue:?}) at sound level {level:?} — {}", cue.file(), if played { "queued" } else { "silent" });
            }
            // ★★ 2026-10-24 (user: *"어둠심판도 소리안나오고있어 번개소리 나야해 … 번개의호흡이랑 벽력일섬 장착하면 번개이펙트 생길떄도"*):
            //    the demon block's strikes and the accessory block's bolt show — the game has no thunder, so the host plays it
            let (judge, bolt) = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                crate::guest_build::h2_thunder_words(&read, demon.block, accessory.block)
            };
            if thunder_watch.step(judge, bolt) {
                let pcm = level.and_then(|lv| crate::voice::voice_pcm(crate::guest_build::H2Voice::Thunder.index(), lv));
                let played = pcm.is_some_and(|(c, r, s)| crate::audio_sink::play_host_clip(c, r, s));
                log::info!("thunder: judge {judge:?} bolt {bolt:?} at sound level {level:?} — {}", if played { "queued" } else { "silent" });
            }
        }
        // 4c5) ★ 2026-10-05: the skill buffs running and their seconds left — the cells right of
        //      the status HUD's bars (user: *"사용한 버프 스킬의 남은 지속시간을 … 스킬아이콘과
        //      숫자로"*). The character's own list `g.b`, a handful of words a poll; sent when a
        //      buff comes, goes or ticks a second.
        if frame % 5 == 2
            && let Some(h2) = crate::guest_build::hero2_java()
        {
            let mut buffs = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                crate::guest_build::h2_buff_hud(&read, h2.char_slot, h2.class_slot)
            };
            // ★ 2026-10-09 (3) 용의 심장: the gear block's own buff (no game buff carries it) after the game's.
            let in_world = {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                items::in_game_state(&read)
            };
            if in_world
                && let Some(d) = gear.dragon_buff(&*emulator)
            {
                buffs.push(d);
            }
            // ★ 2026-10-13 「악마화」 D2: 악의's own cell (the demon block's clock, no game buff carries it) — ★ D3 and the
            //   toggles' cells (ON) while 지옥칼날 / 어둠심판 run.
            if in_world {
                buffs.extend(demon.buffs(&*emulator));
            }
            if buffs != last_skill_buffs {
                last_skill_buffs = buffs.clone();
                let _ = item_proxy.send_event(UserEvent::SkillBuffs(buffs));
            }
            // ★ 2026-10-07 (2) 격분: the rage gauge (0..=100) while it runs — the bar under its buff cell. ★ 2026-10-13 D4:
            //   and 추종's under its own (the demon block's).
            let fury = berserker.block.and_then(|b| {
                let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
                crate::guest_build::h2_berserker_fury(&read, b)
            });
            let mut gauges: Vec<(u8, u8, u8)> = Vec::with_capacity(2);
            if let Some(g) = fury.filter(|w| w.state != 0).map(|w| w.gauge.clamp(0, 100) as u8) {
                gauges.push((crate::guest_build::BerserkerPatch::FURY_ID, crate::guest_build::BerserkerPatch::CLASS, g));
            }
            if in_world
                && let Some(g) = demon.gauge(&*emulator)
            {
                gauges.push((crate::guest_build::DemonPatch::FOLLOW_BUFF_ID, crate::guest_build::H2_BUFF_BEAST_SHEET, g));
            }
            if gauges != last_buff_gauges {
                last_buff_gauges = gauges.clone();
                let _ = item_proxy.send_event(UserEvent::BuffGauges(gauges));
            }
        }

        // 4d) Quickbar read probe (~every 5s) — device-validation aid: confirms
        //     CHAR resolves + the per-slot type bytes. Remove once field-confirmed.
        if frame % 300 == 0 {
            let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
            // ★ The one line that settles whether the game HAS no-save areas at all
            // (2026-08-09). `Some(true)` on an ordinary field and `Some(false)` in a boss
            // room is what earns `HIDE_UI_ON_NO_SAVE`. `None` everywhere means the AREA
            // chain does not resolve and the RE is still not finished. `Some(true)`
            // everywhere — including boss rooms — means the feature has nothing to detect
            // and should be deleted rather than re-aimed.
            log::warn!("area gate: can_save={:?}", items::area_can_save(&read));
            let (c, field, win, types, state, sel) = items::debug_probe(&read);
            // `state` / `window` / `selector` are the three bytes the mode decision is made
            // of, and `in_game` is the derived gate that decides whether the slot list is
            // published at all — see `items::debug_probe`. Raw bytes AND the derived answer
            // on one line, so it says both "what did the guest look like" and "what did we
            // conclude", which is the pair a disagreement needs.
            log::info!(
                "quickbar probe: char={c:x?} field={field} window={win:?} state={state:?} selector={sel:?} in_game={} types={types:?} minimap={:?}",
                items::in_game_state(&read),
                // Tile dims of the last render (pixels / 2). `None` while the map
                // chain does not resolve — the single line that says whether the
                // minimap port is live on this build without opening the panel.
                last_minimap.as_ref().map(crate::Minimap::tiles)
            );
        }

        // 4d') Offset audit (~every 5 s, offset from the probe above so the two do not
        //      land on the same frame). MIGRATION_LGT.md §2.11: five features are ON
        //      on LGT with CHAR offsets that were only ever verified on KTF, so a
        //      moved offset shows a WRONG value rather than none. `wie_probe
        //      --verify-offsets` prints the same numbers but cannot reach the states
        //      that make them informative (a level-up, a bound quick slot, an
        //      an open item popup) — a human playing does, in
        //      minutes. Read-only; drop this block once the matrix has no 🔶 rows.
        if frame % 300 == 150 {
            let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
            if let Some(line) = items::offset_audit(&read) {
                log::warn!("{line}");
            }
            if let Some(line) = items::autoloot_candidates(&read) {
                log::warn!("{line}");
            }
        }

        // 4h) FLAG WINDOW watcher — how a 🔶 offset gets closed on a device instead of by
        //     more disassembly. It reports any byte in the character record's flag block
        //     that changed since the last look, so using the 오토루팅 item names the byte
        //     it sets. Polled faster than the audit (≈2x/s) because the interesting
        //     transitions are single-frame, and it is silent unless something moved.
        if frame % 30 == 15 {
            let read = |addr: u32, len: usize| emulator.read_memory(addr, len);
            if let Some(line) = items::flag_window_changes(&read) {
                log::warn!("{line}");
            }
        }

        // 5) Pace to the target cadence — but never sleep PAST the guest's own next
        //    wake-up (2026-08-13, `MIGRATION_LGT.md` §2.24).
        //
        //    `FRAME_PACE` is a ceiling on how often this loop runs, and treating it as
        //    the nap length quantises every guest timer up to the next 16 ms boundary.
        //    Hero3 re-arms its frame timer at ~21 ms, so a frame that finished at 5 ms
        //    was not looked at again until 16 — then 32 — and the panel-heavy screens
        //    ran at 7.5 fps with this thread 79 % idle. Asking the emulator when it next
        //    has work turns that back into "wake when the game asked to be woken".
        gauge_loops += 1;
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_PACE {
            let mut nap = FRAME_PACE - elapsed;
            // ★ Not while the 부활의 구 box holds the guest: its runnable task is exactly
            //   what the hold keeps from running, and `Some(0)` would spin this at 1 ms.
            if let Some(ms) = emulator.next_wakeup_ms().filter(|_| !revive.holding() && !refine_protection.dialog.holding()) {
                // 1 ms floor: `Some(0)` means a task is runnable, and this loop must not
                // become a spin (the multi-tick above is what serves a work-bound guest).
                nap = nap.min(Duration::from_millis(ms.max(1)));
            }
            // ★ BOOKED AFTER THE WAIT, not before (2026-09-20, review). The nap is
            //   interruptible, so charging the full `nap` here reported a thread that was
            //   woken early as having slept anyway — and `in-tick %` is the gauge this
            //   repo's A/B decisions are made on.
            let napped = Instant::now();
            // ★ THE NAP IS INTERRUPTIBLE (2026-09-18, player report: *"조이스틱 방향키도
            //   0.1초 뒤에 이동이 되는듯한 느낌"*). It used to be `thread::sleep(nap)`, so a
            //   key that arrived one microsecond into a 16 ms nap waited out the whole of
            //   it before step 2 even looked at the channel — up to 16 ms, ~8 on average,
            //   on top of the guest's own 50 ms frame. Waiting ON the channel instead
            //   costs nothing when nothing arrives (the same sleep) and returns the
            //   instant something does; the event is stashed and step 2 applies it at the
            //   top of the very next iteration, which is microseconds away.
            match input_rx.recv_timeout(nap) {
                Ok(ev) => pending_input.push_back(ev),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    log::info!("input channel disconnected; stopping emulator");
                    return;
                }
            }
            gauge_sleep_us += napped.elapsed().as_micros() as u64;
        }
    }
}

/// If `<data_dir>/dump_trigger` exists, read the regions it names and write a
/// snapshot to `<data_dir>/ram_dump.bin` (each region prefixed by u32 LE addr +
/// u32 LE length), logging the head of each. The trigger is consumed (deleted).
fn debug_ram_dump(emulator: &dyn Emulator, data_dir: &std::path::Path) {
    let trigger = data_dir.join("dump_trigger");
    let Ok(spec) = std::fs::read_to_string(&trigger) else {
        return;
    };
    let _ = std::fs::remove_file(&trigger);
    let mut out: Vec<u8> = Vec::new();
    for line in spec.lines() {
        let mut it = line.split_whitespace();
        let (Some(a), Some(l)) = (it.next(), it.next()) else {
            continue;
        };
        let hex = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16);
        let (Ok(addr), Ok(len)) = (hex(a), hex(l)) else {
            continue;
        };
        let (addr, len) = (addr as u32, (len as usize).min(0x20000));
        let bytes = emulator.read_memory(addr, len);
        let head: Vec<String> = bytes.iter().take(24).map(|b| alloc_hex(*b)).collect();
        log::info!("ramdump {addr:#010x} len={len} got={} head={}", bytes.len(), head.join(""));
        out.extend_from_slice(&addr.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&bytes);
    }
    if !out.is_empty() {
        let _ = std::fs::write(data_dir.join("ram_dump.bin"), &out);
        log::info!("ramdump -> ram_dump.bin ({} bytes)", out.len());
    }
}

fn alloc_hex(b: u8) -> String {
    format!("{b:02x}")
}

/// Announce the hero2 KTF build and open its read-only data anchors.
///
/// Split out of the emulator-selection branch 2026-09-10: the framebuffer height now
/// depends on the build (`guest_build::hud_reserved_rows`), so the decision has to be
/// made before the screen is constructed, several statements earlier than the emulator
/// that consumes it.
fn announce_hero2_ktf() {
        // ★★ 2026-09-09 — 「hero2 자체 앵커가 확보되면 … 여기서 연다」의 그 순간이다.
        //
        // 네 주 동안 이 자리는 **비어 있었다**: `guest_anchors.rs` 의 모든 값이 hero3
        // 리버싱 산출물이었으므로 `kind()` 를 세팅하지 않는 것이 유일하게 안전한 선택이고,
        // 그 대가로 호스트 모드가 영구 `Menu` 로 고착됐다(`HERO2_FEATURE_PARITY.md` §2).
        //
        // 이제 hero2 **자기** 앵커가 있다 (`HERO2_GUEST_RE.md`): CHAR 는 자바 static
        // 슬롯 `0x193b5c` 에서 두 번 역참조한 필드 블록이고, 게이지 오프셋 7개는 게임
        // 자신의 STATUS 화면과 교차검증됐고(LV 48 / HP 777·780 / SP 390·390 /
        // EXP 195·112128 / ATK 138 / DEF 115 / CRIT 43%), 상태 워드는 CHAR 상대가 아닌
        // 절대 정적 `0x193aec`(0=로고 2=타이틀 1=슬롯 3=필드 5=창)이다.
        //
        // ⛔ 그런데 이 한 줄이 hero3 주소를 여는 것은 **아니다.** `GuestBuild::Hero2Ktf`
        // 는 새 갈래이고, 그 갈래에서 코드패치·게스트콜·퀵슬롯 쓰기·세이브 체인·팝업·
        // 오토루팅은 전부 `None`/`false` 로 닫혀 있으며 `guest_code_addresses_valid()`
        // 는 `KTF` 에만 답한다. 열린 것은 **읽기 전용 데이터 세 축**뿐이다.
        crate::guest_build::set_hero2_ktf();
        log::warn!(
            "{} build: read-only data anchors ACTIVE — char_anchor={} layout={} app_state={};              everything guest-code-shaped stays CLOSED (code patches, guest calls, quickbar writes,              save chain, popup/cash, auto-loot, window-id gating). Evidence: HERO2_GUEST_RE.md",
            crate::guest_build::name(),
            match crate::guest_build::char_anchor() {
                Some(crate::guest_build::CharAnchor::JavaObject { slot }) => format!("java **{slot:#x} + 4"),
                Some(crate::guest_build::CharAnchor::Pointer { global, .. }) => format!("*{global:#x}"),
                Some(crate::guest_build::CharAnchor::Static(b)) => format!("static {b:#x}"),
                None => "none".to_string(),
            },
            match crate::guest_build::char_layout() {
                Some(l) => format!(
                    "lv+{:#x} hp+{:#x}({}B)/+{:#x} sp+{:#x}/{:?} exp+{:#x}/+{:#x}",
                    l.level, l.hp, l.hp_bytes, l.hp_max, l.sp, l.sp_max, l.exp, l.exp_next
                ),
                None => "-".to_string(),
            },
            match crate::guest_build::app_state() {
                Some(crate::guest_build::AppState::Absolute { addr, field, window, shell, in_world, bottom_overlay, script }) =>
                    format!("static {addr:#x} (field={field} window={window} shell={shell:?} in_world={in_world:?} bottom_overlay={bottom_overlay:?} script={script:?})"),
                Some(crate::guest_build::AppState::CharRelative { off, field, window, shell }) =>
                    format!("CHAR+{off:#x} (field={field} window={window} shell={shell:?})"),
                None => "-".to_string(),
            },
        );
        // ★★ 2026-10-14 「악마화」 이펙트 리뉴얼: the effect library the demon block's FX pieces draw from (`demon_fx.bin`,
        //   `tools_re/demon_fx.py`). Nothing draws from it unless a host block arms `FX_TAG`; a refused library only leaves
        //   악의's pictures out.
        match wie_backend::canvas::set_fx_library(include_bytes!("demon_fx.bin")) {
            Ok((sprites, effects)) => log::info!("demon fx: library of {sprites} sprites · {effects} effects registered"),
            Err(e) => log::warn!("demon fx: the library was refused ({e}) — 악의 draws no effects"),
        }
        const _: () = assert!(crate::guest_build::DemonPatch::FX_TAG == wie_backend::canvas::FX_TAG);
}

include!("refine_protection_host.rs");

include!("bag_tools_host.rs");
include!("save_xfer_host.rs");

/// ★★ 2026-10-16 정렬 · 일괄판매: the app's doors for [`BagTools`] — the ONE call gate and the ONE write gate.
struct AppBagIo;

impl BagToolsIo for AppBagIo {
    fn hero2(&self) -> Option<crate::guest_build::Hero2Java> {
        crate::guest_build::hero2_java()
    }

    fn call(&mut self, emu: &mut dyn Emulator, what: crate::guest_build::JavaCall, this: BtThis, args: &[wie_backend::JavaArg]) -> Option<Arc<wie_backend::GuestCallResult>> {
        let this = match this {
            BtThis::None => JavaThis::None,
            BtThis::Handle(h) => JavaThis::Handle(h),
            BtThis::Static(class, field, descriptor) => JavaThis::Static { class, field, descriptor },
        };
        java_call(emu, what, this, args)
    }

    fn write(&mut self, emu: &mut dyn Emulator, what: crate::guest_build::JavaWrite, addr: u32, bytes: &[u8]) -> bool {
        java_write(emu, what, addr, bytes)
    }
}
