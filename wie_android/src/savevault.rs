// Save-vault POLICY for hero2: which guest files ARE the character, how a kept
// generation is named, and how many survive.
//
// ── WHY THE VAULT EXISTS ─────────────────────────────────────────────────────
// Nothing on the device kept a second copy of a hero2 save. The game has three
// slots and no autosave slot, the host's SAVE button writes straight over the
// live pair, and a guest can only ever report a slot as EMPTY — so a character
// that goes wrong is a character that is gone. hero3 grew a vault for exactly
// this after two players lost characters in one week; this is hero2's.
//
// ── WHAT A GENERATION IS, AND WHY IT IS A PAIR ───────────────────────────────
// `ag.l()V` writes TWO files per save (HERO2_GUEST_RE.md, device-verified
// 2026-09-19):
//
//     ag.g(slot) -> /n<slot>   the character itself
//     ag.f(slot) -> /g<slot>   the progress
//
// Either one alone is a mismatched save — a character from one moment wearing
// another moment's progress. So the unit this module keeps, offers and restores
// is the PAIR, snapshotted at one instant, and a generation missing a half is
// never offered. (hero3 learned the same thing the expensive way at its 72차.)
//
// ── WHY THE RETENTION IS FLAT AND hero3's IS TIERED ──────────────────────────
// hero3 needed an age ladder because its saves are machine-paced: a 5-minute
// autosave, and 제련 attempts that write on every click. hero2 writes ONLY when
// a human presses SAVE — autosave is off here by an explicit decision (three
// slots, no autosave slot, so a timer firing after a bad fight would eat a good
// save). At that cadence [`KEEP_MAX`] generations is many sessions of history
// and the oldest-first rule cannot outrun a player's thumb.
//
// This module is DEPENDENCY-FREE so `savevault_selftest.rs` can `include!` the
// real source and test the shipping policy rather than a copy of it (the same
// contract as endpoint.rs / buffs.rs — see CLAUDE.md).
//
// ★ Plain `//` comments, not `//!`: an inner doc comment cannot appear inside an
// `include!`, so the module doc lives on the `mod savevault;` item in `lib.rs`.

/// Which half of a save pair a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    /// `n<slot>` — the character. 1,352 B on the S9's live slot 1.
    Char,
    /// `g<slot>` — the progress. 889 B on the same save.
    Prog,
}

impl Half {
    /// The single letter the guest names this half with.
    pub fn letter(self) -> char {
        match self {
            Half::Char => 'n',
            Half::Prog => 'g',
        }
    }

    /// Both halves, in the order a generation is written and read.
    pub const BOTH: [Half; 2] = [Half::Char, Half::Prog];
}

/// Lowest save slot the game offers.
pub const SLOT_MIN: u8 = 1;

/// Highest save slot the game offers. FIVE since 2026-09-27 — the game ships three (its
/// slot screen and the archive's `P/n1..P/n3` + `P/g1..P/g3`), and the host extends its
/// file-name tables to `/n1../n5` · `/g1../g5` (`guest_anchors::SaveSlotsPatch::SLOTS`,
/// which this must equal). ⛔ A save to a slot this rejects is invisible to the vault AND
/// to the save edge the ACCOUNT warehouse commits on — so it cannot be smaller than the
/// game's. Still one digit, which [`classify`] relies on.
pub const SLOT_MAX: u8 = 5;

/// Generations kept per slot.
///
/// One per manual save (see the header), and a pair is ~2.2 KB, so the whole
/// vault at its ceiling is ~260 KB across five slots — far below anything the
/// player would notice, and well worth it against losing a character.
pub const KEEP_MAX: usize = 24;

/// Shortest gap between two snapshots of the same slot.
///
/// ★ This is NOT a rate limit on the player, it is what makes ONE save produce
/// ONE generation. `ag.l()` writes `n<slot>` and `g<slot>` back to back, so the
/// second file's destruction edge arrives milliseconds after the first — and the
/// first edge has already copied BOTH halves (neither is touched yet at that
/// point, whichever order the game uses). Without the gap every save would spend
/// two seats on the same instant.
///
/// Two seconds, not hero3's ten: hero2 saves are hand-driven, and collapsing two
/// deliberate saves into one generation costs history for nothing. Two seconds
/// is three orders of magnitude above the burst it exists to absorb and far below
/// the time it takes a human to walk the menus twice.
pub const MIN_GAP_MS: u64 = 2_000;

/// Smallest file either half is ever allowed to be.
///
/// ⛔ This is a TORN-WRITE filter, not a format check. The real thing it catches
/// is a process killed between the guest's truncate-to-zero and its write, which
/// leaves a 0-byte file on disk; vaulting that would spend a seat on a generation
/// that can only destroy a character if it is ever restored.
pub const LEN_MIN: usize = 32;

/// Largest file either half is ever allowed to be.
///
/// ⛔ Also not a format check, and deliberately far above anything observed. The
/// lengths are CONTENT-dependent, not constants — measured across four saves,
/// `n` runs 1,338..1,592 B and `g` runs 447..960 B — and nothing in this repo has
/// established what the writer's upper bound actually is. So the ceiling is a
/// sanity limit whose only job is to refuse something that is obviously not a
/// save at all, and it must stay generous enough that a real save can never trip
/// it. Narrowing it is a claim about the writer, and that claim would have to be
/// measured in the guest first.
pub const LEN_MAX: usize = 65_536;

/// Is this guest path one half of a character save, and if so which one and whose?
///
/// Matched on the WHOLE path, because the guest names these at the filesystem root
/// (`P/n1` in the archive strips to `n1`) and a longer path that merely ends in
/// `n1` is some other file.
pub fn classify(guest_path: &str) -> Option<(Half, u8)> {
    let path = guest_path.trim_start_matches('/');
    let mut chars = path.chars();
    let half = match chars.next() {
        Some('n') => Half::Char,
        Some('g') => Half::Prog,
        _ => return None,
    };
    let rest: String = chars.collect();
    // Exactly one digit: the slots are named 1..SLOT_MAX, so `n10` is not slot 1 and
    // not a save.
    if rest.len() != 1 {
        return None;
    }
    let slot = rest.chars().next().and_then(|c| c.to_digit(10))? as u8;
    if !(SLOT_MIN..=SLOT_MAX).contains(&slot) {
        return None;
    }
    Some((half, slot))
}

/// The guest path a half of `slot` lives at — the inverse of [`classify`].
pub fn guest_path(half: Half, slot: u8) -> String {
    format!("{}{}", half.letter(), slot)
}

/// Is a file of this length worth keeping, and safe to hand back?
///
/// The same rule at both ends on purpose: a generation we would not keep is one
/// we must not offer, and a generation we would not offer is one whose seat is
/// better spent on an older pair that is intact.
pub fn usable_len(len: usize) -> bool {
    (LEN_MIN..=LEN_MAX).contains(&len)
}

/// The opaque key the restore dialog hands back to name one generation.
///
/// `slot|unix_ms`. Two fields because that is exactly what identifies a
/// generation: which character, and when it was taken.
pub fn key(slot: u8, unix_ms: u64) -> String {
    format!("{slot}|{unix_ms}")
}

/// Read a key produced by [`key`]. Returns `None` for anything that is not one —
/// the value crosses JNI from a Java dialog, so it is parsed, never trusted.
pub fn parse_key(key: &str) -> Option<(u8, u64)> {
    let mut parts = key.trim().split('|');
    let slot: u8 = parts.next()?.trim().parse().ok()?;
    let ms: u64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(SLOT_MIN..=SLOT_MAX).contains(&slot) {
        return None;
    }
    Some((slot, ms))
}

/// One line of the list the restore dialog renders: `slot|unix_ms|char_len`.
///
/// The character half's length is the one shown because it is the half that IS
/// the character; the progress half is carried, checked and restored with it but
/// says nothing a player could recognise their save by.
pub fn offer_line(slot: u8, unix_ms: u64, char_len: usize) -> String {
    format!("{}|{}", key(slot, unix_ms), char_len)
}

/// Should a slot be snapshotted now, given when it last was?
///
/// Fails OPEN — an unknown last time means "snapshot". The cost of an extra
/// generation is one seat; the cost of a missed one is the save this whole
/// module exists to keep.
///
/// ⛔ And that is why a clock that moved BACKWARDS also means "snapshot"
/// (2026-09-20, review). The caller's clock is `SystemTime::now()`, which Android
/// steps around — an NTP correction after boot or airplane mode, a manual change.
/// `now.saturating_sub(last)` reads a step back as "no time has passed", so a
/// single correction would suppress every snapshot until real time caught up,
/// failing CLOSED for exactly as long as the clock moved: the loss this module
/// exists to prevent, silently. A monotonic clock is not available to the policy
/// (it is a plain `u64` at both ends, and the timestamps are also generation
/// NAMES the player reads), so the rule is stated here instead.
pub fn should_snapshot(last_ms: Option<u64>, now_ms: u64) -> bool {
    match last_ms {
        None => true,
        Some(last) if now_ms < last => true,
        Some(last) => now_ms - last >= MIN_GAP_MS,
    }
}

/// Which generations of one slot go, given every one it holds.
///
/// Takes timestamps in any order and returns the ones to delete, oldest first.
/// Keeping the NEWEST [`KEEP_MAX`] is the whole policy — see the header for why
/// hero2 does not need hero3's age ladder.
pub fn drop_list(generations: &[u64], keep: usize) -> Vec<u64> {
    let mut sorted: Vec<u64> = generations.to_vec();
    sorted.sort_unstable();
    let excess = sorted.len().saturating_sub(keep);
    sorted.into_iter().take(excess).collect()
}
