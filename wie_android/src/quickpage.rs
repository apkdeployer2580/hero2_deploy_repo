// Host-owned quickbar PLATES and PAGES — hero2's plate WINDOW over the game's 4-slot bar.
//
// ── WHAT THE GUEST HAS (2026-09-17, HERO2_QUICKSLOT_TOUCH_PLAN.md §3.1) ──────────
//
// 영웅서기2 keeps ONE static `byte[4]` — `ag.j` (SKT `u.j`) — and routes exactly four keys
// to it (`1 3 7 9` → slots 0..3, `bd.a(int)`). A byte is `-1` (empty), `1..=7` (the
// character class's n-th ACTIVE skill) or `10 + k` (the current beast's k-th skill).
// The save serialises those four bytes verbatim (`ag.b()`), the game's own register
// routine `ag.a(slot, idx, beast)` clears the same value from every other slot before
// writing it (a skill is never in two slots), and a beast change clears every `>= 10`
// entry (`ag.h()`). There is no page, no item slot, and no fifth key.
//
// ── WHAT THIS FILE DOES (hero3's 79차 model, cut down to hero2's bar) ────────────
//
// The player gets [`PLATES`] plates per page. The guest's four slots are a WINDOW onto
// them: a plate is RESIDENT when it currently occupies a guest slot, and pressing a
// resident plate presses that slot's own hotkey. Pressing a non-resident plate first
// gives it a seat — an empty slot, else the least-recently-used one — with a raw write
// of its value (plus the dedup clear and the skill's own cooldown — see below), then
// presses the hotkey. Every state the guest is ever left in is one the game could have
// produced, so a save landing at any moment records something legitimate.
//
// ★ 2026-10-02 — the cooldown follows the SKILL, not the seat. The game's counters are per
// slot (`ag.a:[I`), so a skill taken off the bar by a page flip or an eviction used to come
// back READY (the seating write said so). `Window::park` keeps the counter of a skill that
// leaves a slot, stamped with the game's own tick count (`CooldownClockPatch` counts the
// runs of `ag.i()V`), and `Window::unpark` hands back what is left of it when the skill is
// seated again. A plate that is off the bar and still cooling is not seated at all.
//
// ── WHY IDENTITY IS ONE BYTE HERE (and was a stamped tuple on hero3) ─────────────
//
// hero3's `(cat, cell)` named a PARTY SLOT's skill, which changed meaning when the
// party changed, and its item refs were bag coordinates that shifted. hero2's byte is
// stable: the class never changes inside a save, and beast values are invalidated by
// the GAME (`ag.h()`), which the capture simply follows — a guest slot that reads `-1`
// after having been confirmed is a plate the game emptied.
//
// ── THE CAPTURE, AND ITS TWO BUDGETS ─────────────────────────────────────────────
//
// Every poll reads the live four bytes and reconciles:
//   * a seated plate whose slot reads its value → CONFIRMED;
//   * a seated plate whose slot reads EMPTY → the game unbound it — plate emptied and
//     the seat freed — but only once CONFIRMED, or after [`PENDING_POLLS`] polls: a
//     registration hands the write to the GAME (popup cursor + synthetic OK), so the
//     byte lands some frames later, and reading "empty" before that must not conclude
//     anything (hero3 79차 §3-2);
//   * an unconfirmed seat ignores ALL mismatches until the pending budget expires;
//     on expiry it releases the seat so the old guest value can find its own plate;
//   * a confirmed plate whose slot reads ANOTHER value → the game rebound the slot; the
//     plate follows the game (its old entry is gone from the bar, so it is gone here);
//   * a slot no plate owns that reads a value → if an unseated plate holds that value it
//     takes the seat, else the first empty plate does (the game bound something the
//     host did not initiate — a keypad registration, a save load).
// A value is held by at most one plate, mirroring the game's dedup.
//
// ── ITEM PLATES (S7, 2026-09-17 저녁) — a plate that presses `0` ─────────────────
//
// The game has no item quickbar: `0` uses the bag's SELECTED recovery item (`bq.a`, an
// index the `#` key cycles through `bq.j = {24, 25, 26}` by ordinal `bq.c`). So an item
// plate names a TYPE (`Entry::Item(24)` = 포션), never a guest slot: it takes no seat,
// no window bookkeeping, and pressing it is "select that type in the bag (the two
// bytes `#` would write), then press `0`" — done by the worker, which walks the bag on
// the frame it acts. A type appears on one plate at most, like a skill value.
// ★★ 2026-10-17: the 엘릭서 borrows the orb type 21, so its plate names its DISPLAY type 35
// (`hero2java::H2_PLATE_ELIXIR`, its own icon cell) and pressing it is the host's drink, not `0`.
//
// ── PERSISTENCE — the rows follow the SAVE (2026-09-29) ──────────────────────────
//
// Plates 4..6 (and every page but the live one) have no home in the guest, so the
// store is a text file (`config::write_quickpages`): pages, the live page, the hidden
// mask, and the OWNER — the save slot + class the rows belong to.
//
// ★ User (2026-09-29): *"스킬을 배운뒤 퀵슬롯 등록하고 저장하지않고 죽으면 다시 해당 세이브
// 슬롯을 실행했을때 퀵슬롯에 스킬이 남아있어"*. The game's own four bytes come back from the
// save; the host's rows used to be written on every change and kept for the same owner
// across a reload — so a registration made after the last save came back on a character
// whose reloaded save did not even have the skill (a beast skill in the report, a class
// skill the same way). Now the rows follow the save: the LIVE rows (`pages`) are scratch
// since the character's last save, and what the store persists — and what a reload hands
// back — are the SAVED rows, taken at each save edge (`Pages::commit`, called by the
// worker when the game writes `n<slot>` / `g<slot>`, the way the warehouse blobs are).
// Leaving the shell (title / slot list → world) puts the owner's saved rows back
// (`adopt_owner`: a rollback for the same owner, a swap for another, `arc` blocks in the
// file), then reconciles against the freshly loaded bar with ZERO writes — seats are
// derived, not imposed. What was not saved is gone, exactly as the game forgets it.
//
// And a plate is never left holding a skill the character does not HAVE — the game's own
// rule for offering 퀵슬롯등록 (the popup on an unlearned skill has no register row):
// `unlearned_plates` names the unseated plates whose value the character lacks, and the
// worker clears them after a few polls. The rollback covers the no-save reload; this
// covers what it cannot see — a 세이브 복원 to an older generation, a 망각의 구 reset with
// rows behind the live page.
//
// ── DEPENDENCY-FREE ON PURPOSE ───────────────────────────────────────────────────
//
// `quickpage_selftest.rs` at the repo root `include!`s **this file** (not a copy), so it
// must not `use crate::...`. `lib.rs` asserts `PLATES == QUICK_SLOTS` and
// `GUEST_SLOTS == GUEST_QUICK_SLOTS` where both names are visible.

/// Plates the player sees, per page. `crate::QUICK_SLOTS` is asserted equal.
pub const PLATES: usize = 7;
/// The game's bar: `ag.j.length`. `crate::GUEST_QUICK_SLOTS` is asserted equal.
pub const GUEST_SLOTS: usize = 4;
/// Pages of plates. Page 0 is where a fresh install starts.
pub const PAGES: usize = 2;
/// How many mismatched captures an UNCONFIRMED seat tolerates before registration
/// expires. Eight polls ≈ 0.7 s at the worker's ~12 Hz.
pub const PENDING_POLLS: u8 = 8;
/// How many owners' SAVED rows are kept, the live owner's included (five save slots ×
/// three classes is the whole reachable set — five since 2026-09-27, the host's
/// `SaveSlotsPatch`).
pub const ARCHIVE_MAX: usize = 15;
/// Guest values at and above this are beast skills. Mirrors `Hero2Java::BEAST_BASE`.
pub const BEAST_BASE: u8 = 10;
/// …and a beast has four skill slots (`bn.j:[B` is `byte[4]`), so `10..=13`.
pub const BEAST_MAX: u8 = BEAST_BASE + 3;
/// The largest character-skill value: seven actives (`b.j = {7,7,6}`) — ★ 2026-10-07 eight: the berserker's
/// 「도발」 is its eighth (`guest_anchors::BerserkerPatch::TAUNT_VALUE`); ★ (2) nine: the awakenings — 격분 is the
/// berserker's value 9, 천공의검 / 개화 the knight's / the rogue's 8 (`IlpungPatch::AWAKEN`).
pub const SKILL_MAX: u8 = 9;
/// `t/itmtp.tdf` has 64 rows; an item token above that is a hand-edited file. Which
/// of them the `0` key can use is the GAME's cycle table, checked live at press time.
pub const ITEM_KIND_MAX: u8 = 64;

const _: () = assert!(PLATES >= GUEST_SLOTS);
const _: () = assert!(PLATES <= 8, "the hidden mask is one byte");

// ── the HIDDEN set (hero3 82차, ported 2026-09-17 저녁) ──────────────────────────
//
// A plate can be switched OFF in the layout editor (HIDE): it is not drawn, not
// tappable, never given a guest seat by the host, and refused by every register /
// activate path — and switching it back on (SHOW) restores the binding that was on it,
// which is why hiding NEVER clears the entry.
//
// ★ A `u8` bitmask, bit `plate`, and the rules live HERE rather than in `overlay` /
// `app` for one reason: drawing, touch, pad and the worker all have to give the same
// answer, and this file is the one `quickpage_selftest.rs` `include!`s — the only place
// the rules can be tested on the Windows host at all.
//
// ★ The mask is a DEVICE preference, not part of a character's row: it sits at the top
// level of the store (`hidden <mask>` in `wie_quickpages.txt`), `adopt_owner` neither
// parks nor swaps it, and the live copy the UI thread draws from is
// `crate::quick_hidden()` — the worker republishes that into the store every poll.

/// Is `plate` switched off?
///
/// Out-of-range plates read as VISIBLE, deliberately: this is asked on the drawing and
/// firing paths, and "a plate number I do not understand" must not silently disable a
/// control. The range checks that matter are the ones already in `activate`/`reserve`.
pub fn plate_hidden(mask: u8, plate: u8) -> bool {
    usize::from(plate) < PLATES && mask & (1 << plate) != 0
}

/// The plates that are switched on, ascending. This is also the RING ORDER: seat `n`
/// of the live ring holds `visible_plates(mask)[n]`, which is what makes the remaining
/// plates close the gap and grow when some are hidden.
pub fn visible_plates(mask: u8) -> Vec<u8> {
    (0..PLATES as u8).filter(|p| !plate_hidden(mask, *p)).collect()
}

/// The plates that are switched off, ascending.
pub fn hidden_plates(mask: u8) -> Vec<u8> {
    (0..PLATES as u8).filter(|p| plate_hidden(mask, *p)).collect()
}

/// How many plates are switched on. Never zero for a sanitised mask.
pub fn visible_count(mask: u8) -> usize {
    PLATES - hidden_plates(mask).len()
}

/// Where `plate` comes in the visible order, or `None` when it is hidden. The ring
/// seat the geometry uses is `1 + this` (seat 0 belongs to CLR) — see
/// `overlay::plate_seat`.
pub fn visible_index(mask: u8, plate: u8) -> Option<usize> {
    if plate_hidden(mask, plate) || usize::from(plate) >= PLATES {
        return None;
    }
    visible_plates(mask).iter().position(|p| *p == plate)
}

/// The mask with `plate` switched off, or `None` when that is refused.
///
/// ⛔ The LAST visible plate cannot be hidden: with no plate on screen the 퀵슬롯등록
/// popup has no touch target to register into, the page-flip button is sized from a
/// plate that would not exist, and the one screen that could switch a plate back on is
/// the layout editor — reached past a modal the player has to know is there.
pub fn hide_plate(mask: u8, plate: u8) -> Option<u8> {
    if usize::from(plate) >= PLATES || plate_hidden(mask, plate) || visible_count(mask) <= 1 {
        return None;
    }
    Some(mask | (1 << plate))
}

/// Everything visible again — what the editor's SHOW button does, and the ONLY way
/// back. There is deliberately no per-plate `show`: a hidden plate has no ring seat,
/// so there is nothing on screen to select it by.
pub const SHOW_ALL: u8 = 0;

/// Force a mask read from disk (or from a build with a different [`PLATES`]) into the
/// domain this build can honour: bits past the plate row are dropped, and a mask that
/// would leave NOTHING on screen degrades to all-visible — `ring_slots` has its own
/// ⛔ note about a field with no controls at all.
pub fn sanitise_hidden(mask: u8) -> u8 {
    let m = mask & ((1u16 << PLATES) - 1) as u8;
    if PLATES - hidden_plates(m).len() == 0 { SHOW_ALL } else { m }
}

/// What a plate holds — the guest byte, typed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Entry {
    #[default]
    Empty,
    /// `1..=SKILL_MAX` — the class's n-th active skill.
    Skill(u8),
    /// The raw guest value `10 + k` — the beast's k-th skill.
    Beast(u8),
    /// An `itmtp` type the `0` key can use (a member of the game's cycle `bq.j`).
    /// Host-only: never in the guest's bar, so [`Entry::value`] is `None`.
    Item(u8),
}

impl Entry {
    /// From a live guest byte. Anything outside the game's own domain reads as empty.
    pub fn from_value(v: i8) -> Entry {
        match v {
            v if v <= 0 => Entry::Empty,
            v if (v as u8) <= SKILL_MAX => Entry::Skill(v as u8),
            v if (BEAST_BASE..=BEAST_MAX).contains(&(v as u8)) => Entry::Beast(v as u8),
            _ => Entry::Empty,
        }
    }

    /// The guest byte this plate binds as, or `None` for an empty plate — or an ITEM
    /// plate, which has no guest byte.
    pub fn value(self) -> Option<u8> {
        match self {
            Entry::Empty | Entry::Item(_) => None,
            Entry::Skill(v) | Entry::Beast(v) => Some(v),
        }
    }

    /// The item type an item plate names.
    pub fn item(self) -> Option<u8> {
        match self {
            Entry::Item(t) => Some(t),
            _ => None,
        }
    }

    /// From a persisted token: a guest value, or `i<type>` for an item plate.
    pub fn from_token(tok: &str) -> Entry {
        if let Some(t) = tok.strip_prefix('i') {
            return match t.parse::<u8>() {
                Ok(t) if t > 0 && t < ITEM_KIND_MAX => Entry::Item(t),
                _ => Entry::Empty,
            };
        }
        match tok.parse::<i8>() {
            Ok(v) => Entry::from_value(v),
            Err(_) => Entry::Empty,
        }
    }

    pub fn token(self) -> String {
        match self {
            Entry::Empty => "-".to_string(),
            Entry::Skill(v) | Entry::Beast(v) => v.to_string(),
            Entry::Item(t) => format!("i{t}"),
        }
    }

    pub fn is_empty(self) -> bool {
        matches!(self, Entry::Empty)
    }
}

pub type Page = [Entry; PLATES];

/// Who the rows belong to: the save slot (`ag.c:B`, the slot list's cursor at load) and
/// the class (`ag.a:B`). Two owners with the same key ARE the same character as far as
/// a quickbar is concerned — a new game in a slot that used to hold another class gets
/// fresh rows because the class differs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Owner {
    pub slot: u8,
    pub class: u8,
}

/// ★ 2026-09-27: did the character that is leaving the shell come out of the CLASS PICK —
/// a new game — or out of 이어하기? Sampled every frame while the shell is up; the answer
/// is taken once, at the shell exit, where the owner is adopted.
///
/// The slot screen is one window that changes mode in place (새로하기 10 → class pick 11,
/// 이어하기 1), so "the last slot-screen mode seen before the world" is the answer. The
/// title alone for [`NewGameWatch::BARE_FRAMES`] frames in a row clears it (a class pick
/// cancelled with CLR), while the frames between the pick's OK and the shell going down
/// do not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NewGameWatch {
    pick: bool,
    bare: u8,
}

impl NewGameWatch {
    /// Title frames without a slot screen that cancel a seen class pick.
    pub const BARE_FRAMES: u8 = 5;

    /// One frame's look: `None` = the shell is not up (nothing learned), `Some(None)` = the
    /// shell with no slot screen, `Some(Some(mode))` = the slot screen in `mode`.
    pub fn sample(&mut self, seen: Option<Option<i32>>, class_pick: i32) {
        match seen {
            None => {}
            Some(Some(mode)) => {
                self.pick = mode == class_pick;
                self.bare = 0;
            }
            Some(None) => {
                self.bare = self.bare.saturating_add(1);
                if self.bare >= Self::BARE_FRAMES {
                    self.pick = false;
                }
            }
        }
    }

    /// At the shell exit: a new game? Consumed — the next exit starts from nothing.
    pub fn take(&mut self) -> bool {
        let pick = self.pick;
        *self = Self::default();
        pick
    }
}

/// The cooldown of a skill that is NOT in a guest slot right now (2026-10-02).
///
/// ★ User: *"스킬 퀵슬롯 페이지전환을 했다가 다시 돌아오면 스킬 쿨타임이 원복되어있어"*. The game
/// keeps one counter per SLOT (`ag.a:[I`, ticked by `ag.i()V`), not per skill — so a skill
/// the host takes off the bar (a page flip, an LRU eviction) has nowhere in the guest to keep
/// cooling, and the host's seating write used to put it back READY. The counter it had goes
/// here instead, with the game's own tick count at that moment; what it is seated with later
/// is that counter less the ticks the game has run since.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Parked {
    value: u8,
    /// The slot's counter when the skill left it (`> 0`).
    left: i32,
    /// What the counter started from, as far as the host saw (`>= left`) — the ring's full
    /// circle for a beast skill, whose total no table names.
    total: i32,
    /// The cooldown clock (`ag.i()V` runs counted) at that moment.
    stamp: u32,
}

/// Which plate sits in which guest slot right now, plus the LRU clock and the two
/// per-seat budgets the capture needs — and the cooldowns of the skills that are off the
/// bar. Process-lifetime state, never persisted: seats are re-derived from the live bar
/// on every world entry, and a load resets the game's own counters (`ag.a(Z)V`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Window {
    /// guest slot -> plate.
    owner: [Option<u8>; GUEST_SLOTS],
    /// Last use tick per guest slot, for eviction.
    used: [u64; GUEST_SLOTS],
    clock: u64,
    /// Mismatched polls left before an unconfirmed seat expires (0 = confirmed).
    pending: [u8; GUEST_SLOTS],
    /// Skills off the bar whose cooldown is still running. A value appears once.
    parked: Vec<Parked>,
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

// `plate_at` / `is_pending` / `residents` are the store's inspection API — used by the
// selftest and the probe-side reasoning, not (yet) by the app.
#[allow(dead_code)]
impl Window {
    pub fn new() -> Self {
        Window { owner: [None; GUEST_SLOTS], used: [0; GUEST_SLOTS], clock: 1, pending: [0; GUEST_SLOTS], parked: Vec::new() }
    }

    /// Everything — the seats and the parked cooldowns (a world entry, the shell).
    pub fn clear(&mut self) {
        *self = Window::new();
    }

    /// The seats only: the parked cooldowns outlive a page flip.
    fn clear_seats(&mut self) {
        let parked = std::mem::take(&mut self.parked);
        *self = Window::new();
        self.parked = parked;
    }

    /// `value` is leaving a guest slot whose counter reads `left` (of `total`, as far as
    /// the host saw it start) at clock `now`. A counter that is not running (`<= 0`, the
    /// game's own "ready") parks nothing — and drops what was parked for that value, which
    /// is older than the slot it just left.
    pub fn park(&mut self, value: u8, left: i32, total: i32, now: u32) {
        self.parked.retain(|p| p.value != value);
        if left > 0 && value != 0 {
            self.parked.push(Parked { value, left, total: total.max(left), stamp: now });
        }
    }

    /// What `value`'s counter reads at clock `now`, had its slot kept ticking: `(left,
    /// total)`, or `None` when nothing is parked or it has run out. The game takes one off
    /// per tick (`ag.i()V`); the clock is a wrapping count of them.
    pub fn parked(&self, value: u8, now: u32) -> Option<(i32, i32)> {
        let p = self.parked.iter().find(|p| p.value == value)?;
        let left = i64::from(p.left) - i64::from(now.wrapping_sub(p.stamp));
        (left > 0).then_some((left as i32, p.total))
    }

    /// `value` is going into a guest slot: what is left of its parked counter, `(left,
    /// total)` — `None` = ready — and the entry is gone (the guest keeps it now).
    pub fn unpark(&mut self, value: u8, now: u32) -> Option<(i32, i32)> {
        let left = self.parked(value, now);
        self.forget_parked(value);
        left
    }

    /// ★ 2026-10-09 (3) 현자의 인장: every skill's cooldown reset — nothing stays parked.
    pub fn forget_all_parked(&mut self) {
        self.parked.clear();
    }

    /// ★★ 2026-10-14 (2차 B1 시간도둑): `ticks` off every parked cooldown — what the gear block took off the guest's own four;
    /// one that runs out is gone (ready).
    pub fn shorten_parked(&mut self, ticks: i32) {
        for p in self.parked.iter_mut() {
            p.left = p.left.saturating_sub(ticks);
        }
        self.parked.retain(|p| p.left > 0);
    }

    /// ★★ 2026-10-16 (악의 무기 M1): `ticks` off `value`'s parked counter only (악의's — the guest cell, when it is seated, the
    /// host writes itself).
    pub fn shorten_parked_value(&mut self, value: u8, ticks: i32) {
        for p in self.parked.iter_mut().filter(|p| p.value == value) {
            p.left = p.left.saturating_sub(ticks);
        }
        self.parked.retain(|p| p.left > 0);
    }

    /// Drop `value`'s parked cooldown: the game has its counter (or the skill is gone).
    pub fn forget_parked(&mut self, value: u8) {
        self.parked.retain(|p| p.value != value);
    }

    pub fn guest_of(&self, plate: u8) -> Option<u8> {
        self.owner.iter().position(|o| *o == Some(plate)).map(|g| g as u8)
    }

    pub fn plate_at(&self, guest: u8) -> Option<u8> {
        self.owner.get(usize::from(guest)).copied().flatten()
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Put `plate` in `guest`, unconfirmed (the byte is expected to appear within
    /// [`PENDING_POLLS`]). A plate can sit in only one seat.
    pub fn seat(&mut self, plate: u8, guest: u8) {
        let g = usize::from(guest);
        if g >= GUEST_SLOTS {
            return;
        }
        for o in self.owner.iter_mut() {
            if *o == Some(plate) {
                *o = None;
            }
        }
        self.owner[g] = Some(plate);
        self.used[g] = self.tick();
        self.pending[g] = PENDING_POLLS;
    }

    /// Same as [`Window::seat`] but already confirmed — the byte has been READ there.
    pub fn seat_confirmed(&mut self, plate: u8, guest: u8) {
        self.seat(plate, guest);
        if let Some(p) = self.pending.get_mut(usize::from(guest)) {
            *p = 0;
        }
    }

    pub fn vacate(&mut self, guest: u8) {
        if let Some(o) = self.owner.get_mut(usize::from(guest)) {
            *o = None;
        }
        if let Some(p) = self.pending.get_mut(usize::from(guest)) {
            *p = 0;
        }
    }

    pub fn note_used(&mut self, guest: u8) {
        let t = self.tick();
        if let Some(u) = self.used.get_mut(usize::from(guest)) {
            *u = t;
        }
    }

    pub fn is_pending(&self, guest: u8) -> bool {
        self.pending.get(usize::from(guest)).is_some_and(|p| *p > 0)
    }

    /// The seat a non-resident plate gets: the first EMPTY guest slot, else the least
    /// recently used one. Returns `(guest, evicted plate)`; does not seat.
    pub fn pick_seat(&self) -> (u8, Option<u8>) {
        if let Some(g) = self.owner.iter().position(|o| o.is_none()) {
            return (g as u8, None);
        }
        let g = (0..GUEST_SLOTS).min_by_key(|g| self.used[*g]).unwrap_or(0);
        (g as u8, self.owner[g])
    }

    pub fn residents(&self) -> Vec<(u8, u8)> {
        self.owner.iter().enumerate().filter_map(|(g, o)| o.map(|p| (p, g as u8))).collect()
    }
}

/// What pressing a plate on the field means.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Activation {
    /// Already in a guest slot: press that slot's hotkey, write nothing.
    Resident { guest: u8 },
    /// Not seated: write `value` into `guest` (the raw bind — the caller also clears any
    /// other slot holding `value`, parks the counter of the skill the slot held and sets
    /// the slot's to what `value` has left, [`Window::unpark`]), then press.
    Bind { guest: u8, value: u8 },
    /// An item plate: select `kind` in the bag (`bq.c` / `bq.a`), then press `0`.
    Item { kind: u8 },
    /// An empty, hidden or out-of-range plate.
    Nothing,
}

/// What [`Pages::adopt_owner`] did to the live rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Adopted {
    /// Same owner, and the live rows already were its saved ones.
    Kept,
    /// Same owner; the live rows differed from its saved ones — something registered,
    /// captured or flipped since the last save — and were rolled back to them.
    RolledBack,
    /// Another owner, whose saved rows came back.
    Restored,
    /// Another owner with nothing saved: empty plates.
    Fresh,
}

/// The store: the LIVE rows of the owner, the live page, the hidden mask, the owner, and
/// the SAVED rows of every owner.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pages {
    /// The live rows of `owner` — scratch since that character's last save.
    pages: Vec<Page>,
    current: usize,
    /// Bit `p` set = plate `p` is switched off in the layout editor (hero3 82차).
    hidden: u8,
    owner: Option<Owner>,
    /// The rows of every owner — the live one included — as they stood at that
    /// character's last save edge ([`Pages::commit`]). This, never `pages`, is what the
    /// store writes and what a world entry hands back. Most recently touched last.
    saved: Vec<(Owner, Vec<Page>)>,
}

impl Default for Pages {
    fn default() -> Self {
        Self::new()
    }
}

// `hidden` / `set_hidden` carry hero3's per-plate HIDE/SHOW (82차) through the store;
// the layout editor flips the live copy (`crate::set_quick_hidden`) and the worker
// republishes it here every poll. `count` / `page` / `owner` are inspection API.
#[allow(dead_code)]
impl Pages {
    pub fn new() -> Self {
        Pages { pages: vec![[Entry::Empty; PLATES]; PAGES], current: 0, hidden: 0, owner: None, saved: Vec::new() }
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn count(&self) -> usize {
        self.pages.len()
    }

    pub fn page(&self, i: usize) -> Option<&Page> {
        self.pages.get(i)
    }

    pub fn plates(&self) -> &Page {
        &self.pages[self.current]
    }

    pub fn plate(&self, plate: u8) -> Entry {
        self.pages[self.current].get(usize::from(plate)).copied().unwrap_or_default()
    }

    pub fn hidden(&self) -> u8 {
        self.hidden
    }

    /// Sanitised on the way in, so no reader below has to repeat it.
    pub fn set_hidden(&mut self, mask: u8) {
        self.hidden = sanitise_hidden(mask);
    }

    pub fn plate_hidden(&self, plate: u8) -> bool {
        plate_hidden(self.hidden, plate)
    }

    pub fn owner(&self) -> Option<Owner> {
        self.owner
    }

    pub fn next_page(&self) -> usize {
        (self.current + 1) % self.pages.len()
    }

    fn set(&mut self, plate: u8, e: Entry) {
        if let Some(slot) = self.pages[self.current].get_mut(usize::from(plate)) {
            *slot = e;
        }
    }

    /// The plate on the live page holding `value`, if any.
    pub fn plate_holding(&self, value: u8) -> Option<u8> {
        self.pages[self.current].iter().position(|e| e.value() == Some(value)).map(|p| p as u8)
    }

    /// The plate on the live page holding item type `kind`, if any.
    pub fn plate_holding_item(&self, kind: u8) -> Option<u8> {
        self.pages[self.current].iter().position(|e| e.item() == Some(kind)).map(|p| p as u8)
    }

    /// Put item type `kind` on `plate` (popup 퀵슬롯등록 on a recovery item). The type
    /// leaves any other plate (on every page); a skill the plate held loses its plate,
    /// and if it was RESIDENT its guest seat is freed and returned so the caller clears
    /// the guest byte — otherwise the next capture would hand that skill to an empty
    /// plate and it would look like it never left. `None` for a hidden / bad plate.
    pub fn set_item(&mut self, window: &mut Window, plate: u8, kind: u8) -> Option<Option<u8>> {
        if usize::from(plate) >= PLATES || self.plate_hidden(plate) || kind == 0 || kind >= ITEM_KIND_MAX {
            return None;
        }
        for page in self.pages.iter_mut() {
            for e in page.iter_mut() {
                if e.item() == Some(kind) {
                    *e = Entry::Empty;
                }
            }
        }
        let freed = window.guest_of(plate);
        if let Some(g) = freed {
            window.vacate(g);
        }
        self.set(plate, Entry::Item(kind));
        Some(freed)
    }

    /// Clear every plate (on every page) holding item type `kind` (popup 퀵슬롯해제).
    /// Returns whether anything changed.
    pub fn forget_item(&mut self, kind: u8) -> bool {
        let mut changed = false;
        for page in self.pages.iter_mut() {
            for e in page.iter_mut() {
                if e.item() == Some(kind) {
                    *e = Entry::Empty;
                    changed = true;
                }
            }
        }
        changed
    }

    /// Clear every plate (on every page) holding `value` — the game's dedup, mirrored —
    /// and free its seat. Returns whether anything changed.
    pub fn forget_value_in(&mut self, window: &mut Window, value: u8) -> bool {
        let mut changed = false;
        for (pi, page) in self.pages.iter_mut().enumerate() {
            for (p, e) in page.iter_mut().enumerate() {
                if e.value() == Some(value) {
                    *e = Entry::Empty;
                    changed = true;
                    if pi == self.current {
                        if let Some(g) = window.guest_of(p as u8) {
                            window.vacate(g);
                        }
                    }
                }
            }
        }
        changed
    }

    /// Reconcile the live page with the live bar. See the header for the rules.
    /// Returns whether a plate changed (so the caller can persist / republish).
    pub fn capture(&mut self, window: &mut Window, live: [i8; GUEST_SLOTS]) -> bool {
        let mut changed = false;
        // Pass 1: seated plates follow their slot.
        for g in 0..GUEST_SLOTS {
            let read = Entry::from_value(live[g]);
            let Some(p) = window.owner[g] else { continue };
            let held = self.plate(p);
            if read == held && !held.is_empty() {
                window.pending[g] = 0;
                continue;
            }
            if window.pending[g] > 0 {
                window.pending[g] -= 1;
                if window.pending[g] == 0 {
                    self.set(p, Entry::Empty);
                    window.vacate(g as u8);
                    changed = true;
                }
                continue;
            }
            if read.is_empty() {
                if !held.is_empty() {
                    self.set(p, Entry::Empty);
                    changed = true;
                }
                window.vacate(g as u8);
                continue;
            }
            // The slot holds another value: the game rebound it. The plate follows, and
            // any other plate that held that value loses it (the game's dedup).
            if let Some(v) = read.value() {
                for q in 0..PLATES as u8 {
                    if q != p && self.plate(q).value() == Some(v) {
                        self.set(q, Entry::Empty);
                        if let Some(og) = window.guest_of(q) {
                            window.vacate(og);
                        }
                    }
                }
            }
            self.set(p, read);
            window.pending[g] = 0;
            changed = true;
        }
        // Pass 2: unowned slots with a value find a plate.
        for g in 0..GUEST_SLOTS {
            if window.owner[g].is_some() {
                continue;
            }
            let read = Entry::from_value(live[g]);
            let Some(v) = read.value() else { continue };
            // ★ A VISIBLE empty plate first (hero3 82차's `adopt_guest_bar`): a binding
            //   the save just restored must land where the player can see and press it,
            //   not on a plate they switched off. A hidden plate still takes the value
            //   when nothing visible is free — a slot no plate owns is never captured
            //   and never cleared, yet its digit still fires from the keypad.
            let plate = match self.plate_holding(v) {
                Some(p) => p,
                None => {
                    let hidden = self.hidden;
                    let row = &self.pages[self.current];
                    let empty_at = |want_hidden: bool| {
                        row.iter().enumerate().position(|(p, e)| e.is_empty() && plate_hidden(hidden, p as u8) == want_hidden)
                    };
                    match empty_at(false).or_else(|| empty_at(true)) {
                        Some(p) => p as u8,
                        // No empty plate: the guest slot's own index is the least
                        // surprising home (plate g used to be slot g before the window
                        // existed).
                        None => g as u8,
                    }
                }
            };
            if self.plate(plate) != read {
                self.set(plate, read);
                changed = true;
            }
            window.seat_confirmed(plate, g as u8);
        }
        // A value READ in its seat cools in the guest's own counter: whatever is parked for
        // it is older than that (a registration the game refused, a bar the load produced).
        for g in 0..GUEST_SLOTS {
            if window.pending[g] != 0 {
                continue;
            }
            if let Some(v) = window.owner[g].and_then(|p| self.plate(p).value()) {
                window.forget_parked(v);
            }
        }
        changed
    }

    /// A plate the player wants to REGISTER a skill into (popup flow): give it a guest
    /// seat and return it. The GAME does the write. `value` is what will land there; any
    /// other plate holding it is cleared now so the row never shows a skill twice.
    pub fn reserve_for_bind(&mut self, window: &mut Window, plate: u8, value: u8) -> Option<u8> {
        if usize::from(plate) >= PLATES || self.plate_hidden(plate) {
            return None;
        }
        for q in 0..PLATES as u8 {
            if q != plate && self.plate(q).value() == Some(value) {
                self.set(q, Entry::Empty);
                if let Some(og) = window.guest_of(q) {
                    window.vacate(og);
                }
            }
        }
        let g = match window.guest_of(plate) {
            Some(g) => g,
            None => window.pick_seat().0,
        };
        self.set(plate, Entry::from_value(value as i8));
        window.seat(plate, g);
        Some(g)
    }

    /// What pressing `plate` on the field means. A `Bind` also seats the plate (the
    /// caller performs the write and then the press).
    pub fn activate(&mut self, window: &mut Window, plate: u8) -> Activation {
        if usize::from(plate) >= PLATES || self.plate_hidden(plate) {
            return Activation::Nothing;
        }
        if let Some(kind) = self.plate(plate).item() {
            return Activation::Item { kind };
        }
        let Some(value) = self.plate(plate).value() else { return Activation::Nothing };
        if let Some(g) = window.guest_of(plate) {
            window.note_used(g);
            return Activation::Resident { guest: g };
        }
        let (g, _evicted) = window.pick_seat();
        // The evicted plate keeps its entry — it is simply not resident any more. It
        // will get a seat back the next time it is pressed.
        window.seat(plate, g);
        Activation::Bind { guest: g, value }
    }

    /// The raw writes a page FLIP needs: guest slots 0.. take the incoming page's first
    /// non-empty plates in order, the rest are cleared. `(guest, Option<value>)`.
    /// Seats the window accordingly (unconfirmed).
    pub fn flip_to(&mut self, window: &mut Window, to: usize) -> Vec<(u8, Option<u8>)> {
        if to >= self.pages.len() {
            return Vec::new();
        }
        self.current = to;
        window.clear_seats();
        let mut plan = Vec::with_capacity(GUEST_SLOTS);
        let mut g = 0u8;
        for p in 0..PLATES as u8 {
            if g as usize >= GUEST_SLOTS {
                break;
            }
            if self.plate_hidden(p) {
                continue;
            }
            if let Some(v) = self.plate(p).value() {
                plan.push((g, Some(v)));
                window.seat(p, g);
                g += 1;
            }
        }
        while (g as usize) < GUEST_SLOTS {
            plan.push((g, None));
            g += 1;
        }
        plan
    }

    /// World entry (shell → field): `owner` is who just loaded. The live rows become that
    /// owner's SAVED rows — the same owner is rolled back to its last save, another owner
    /// gets its own rows back (or fresh ones). Then the caller reconciles with a capture.
    /// ⛔ Nothing of the live rows is kept or parked here: what was not saved is gone,
    /// exactly as the game forgets its own four bytes (2026-09-29, see the header).
    pub fn adopt_owner(&mut self, window: &mut Window, owner: Owner) -> Adopted {
        window.clear();
        let same = self.owner == Some(owner);
        self.owner = Some(owner);
        let saved = self.take_saved(owner);
        let had_saved = saved.is_some();
        let rows = saved.unwrap_or_else(|| vec![[Entry::Empty; PLATES]; PAGES]);
        if same {
            if self.pages == rows {
                return Adopted::Kept;
            }
            self.pages = rows;
            return Adopted::RolledBack;
        }
        self.pages = rows;
        self.current = 0;
        if had_saved { Adopted::Restored } else { Adopted::Fresh }
    }

    /// ★ 2026-10-09 (user: *"내가 등록하지 않은 퀵슬롯에 스킬이 등록되어있는데 … 격분스킬이 뭔가 퀵슬롯 등록관련 오류"*, S25 log):
    /// a world entry hands back the bar the game SAVED — the seats of whichever page was live then (a flip rewrites all
    /// four slots, a seating one) — while the live page is whatever [`Pages::adopt_owner`] left: 0 for another owner, the
    /// last one shown for the same. On the wrong page the capture reads those seats as bindings the host never made and
    /// puts them on that page's empty plates (the report: saved on page 1 with 격분 · 5 · 4 · 10 seated, back on page 0 →
    /// `[6, 5, 8, B10, 4, 9, …]`). So right after the adoption, before the first capture: the page whose plates hold the
    /// most of `live`'s values becomes the live page — the live one keeps a tie, and a bar no page holds (an empty one, a
    /// keypad registration) changes nothing. Returns the page now live.
    pub fn match_page(&mut self, live: [i8; GUEST_SLOTS]) -> usize {
        let held = |page: &Page| live.iter().filter_map(|v| Entry::from_value(*v).value()).filter(|v| page.iter().any(|e| e.value() == Some(*v))).count();
        let mut best = (self.current, held(&self.pages[self.current]));
        for (i, page) in self.pages.iter().enumerate() {
            let n = held(page);
            if n > best.1 {
                best = (i, n);
            }
        }
        self.current = best.0;
        best.0
    }

    /// `owner`'s saved rows, moved to the end of the list (most recently touched last) and
    /// brought to [`PAGES`] pages. `None` when that owner has never saved.
    fn take_saved(&mut self, owner: Owner) -> Option<Vec<Page>> {
        let i = self.saved.iter().position(|(o, _)| *o == owner)?;
        let block = self.saved.remove(i);
        self.saved.push(block);
        let mut rows = self.saved.last().map(|(_, r)| r.clone())?;
        if rows.len() != PAGES {
            rows.resize(PAGES, [Entry::Empty; PLATES]);
        }
        Some(rows)
    }

    /// The game just wrote its save files: the live rows are what the owner keeps, and
    /// what the next world entry hands back. Returns whether the saved rows changed (so
    /// the caller persists). Rows that belong to nobody are never saved.
    pub fn commit(&mut self) -> bool {
        let Some(owner) = self.owner else { return false };
        if self.take_saved(owner).is_some_and(|rows| rows == self.pages) {
            return false;
        }
        self.saved.retain(|(o, _)| *o != owner);
        self.saved.push((owner, self.pages.clone()));
        // Oldest first out — never the owner just committed, which is last.
        while self.saved.len() > ARCHIVE_MAX {
            self.saved.remove(0);
        }
        true
    }

    /// Plates on every page whose value the character does not HAVE, by `unlearned`, and
    /// which are not seated in the live bar — `(page, plate, value)`. A seated plate is the
    /// game's own bar and is never questioned; an unseated plate's value has no witness in
    /// the guest, which is where a skill the character no longer has can linger.
    pub fn unlearned_plates(&self, window: &Window, unlearned: impl Fn(u8) -> bool) -> Vec<(usize, u8, u8)> {
        let mut out = Vec::new();
        for (pi, page) in self.pages.iter().enumerate() {
            for (p, e) in page.iter().enumerate() {
                let Some(v) = e.value() else { continue };
                if pi == self.current && window.guest_of(p as u8).is_some() {
                    continue;
                }
                if unlearned(v) {
                    out.push((pi, p as u8, v));
                }
            }
        }
        out
    }

    /// Clear one plate on one page (an unseated one — see [`Pages::unlearned_plates`]; a
    /// seat is left as it is). Returns whether the plate held anything.
    pub fn clear_plate_on(&mut self, page: usize, plate: u8) -> bool {
        match self.pages.get_mut(page).and_then(|pg| pg.get_mut(usize::from(plate))) {
            Some(e) if !e.is_empty() => {
                *e = Entry::Empty;
                true
            }
            _ => false,
        }
    }

    /// A NEW GAME started in save slot `slot` (2026-09-27, user: *"세이브 데이터를 새로하기로
    /// 삭제하고 다시 해당 슬롯에서 시작하면 이전 데이터의 퀵슬롯의 잔재가 남아있어"*): every row
    /// that slot has ever had — the live rows if they are its, and every saved block of
    /// it, whatever the class — belonged to characters that no longer exist. Dropped, so
    /// the [`Pages::adopt_owner`] that follows starts the new character on empty plates.
    /// ⛔ The owner key alone cannot tell: a new game of the SAME class in the same slot
    /// IS the same key, and [`Pages::adopt_owner`] hands a same-key owner its saved rows
    /// (that is what makes a plain reload keep them). Returns whether anything went.
    pub fn forget_slot(&mut self, window: &mut Window, slot: u8) -> bool {
        let mut dropped = false;
        if self.owner.is_some_and(|o| o.slot == slot) {
            self.owner = None;
            self.pages = vec![[Entry::Empty; PLATES]; PAGES];
            self.current = 0;
            window.clear();
            dropped = true;
        }
        let before = self.saved.len();
        self.saved.retain(|(o, _)| o.slot != slot);
        dropped | (self.saved.len() != before)
    }

    /// `own <slot> <class>` / `arc <slot> <class>` / `page <i> v v v v v v v` (`-` = empty,
    /// `i<type>` = an item plate) / `current <i>` / `hidden <mask>`. Append-only in spirit:
    /// unknown lines are ignored.
    ///
    /// ⛔ The `own` block is the owner's SAVED rows, never the live ones (2026-09-29): the
    /// file is what a reload hands back, and a registration the player has not saved must
    /// not be in it. An owner that has never saved writes empty rows.
    pub fn serialize(&self) -> String {
        fn row(page: &Page) -> String {
            page.iter().map(|e| e.token()).collect::<Vec<_>>().join(" ")
        }
        let empty = vec![[Entry::Empty; PLATES]; PAGES];
        let mut out = String::from("# hero2 quickslot pages (host-owned) — see quickpage.rs\n");
        if let Some(o) = self.owner {
            out.push_str(&format!("own {} {}\n", o.slot, o.class));
        }
        let own_rows = self.owner.and_then(|o| self.saved.iter().find(|(s, _)| *s == o)).map_or(&empty, |(_, r)| r);
        for (i, p) in own_rows.iter().enumerate() {
            out.push_str(&format!("page {i} {}\n", row(p)));
        }
        out.push_str(&format!("current {}\nhidden {}\n", self.current, self.hidden));
        for (o, rows) in &self.saved {
            if Some(*o) == self.owner {
                continue;
            }
            out.push_str(&format!("arc {} {}\n", o.slot, o.class));
            for (i, p) in rows.iter().enumerate() {
                out.push_str(&format!("page {i} {}\n", row(p)));
            }
        }
        out
    }

    /// The `own` rows come back as both the owner's saved rows and the live ones — the
    /// store was written at a save, so at start-up the two are the same thing.
    pub fn parse(text: &str) -> Self {
        let mut out = Pages::new();
        // `None` = the owner's rows; `Some(owner)` = another owner's block being read.
        let mut block: Option<(Owner, Vec<Page>)> = None;
        let mut hidden = 0u8;
        let mut current = 0usize;
        for line in text.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("own") => {
                    let (Some(s), Some(c)) = (it.next().and_then(|x| x.parse().ok()), it.next().and_then(|x| x.parse().ok())) else { continue };
                    out.owner = Some(Owner { slot: s, class: c });
                }
                Some("arc") => {
                    if let Some(b) = block.take() {
                        out.saved.push(b);
                    }
                    let (Some(s), Some(c)) = (it.next().and_then(|x| x.parse().ok()), it.next().and_then(|x| x.parse().ok())) else { continue };
                    block = Some((Owner { slot: s, class: c }, vec![[Entry::Empty; PLATES]; PAGES]));
                }
                Some("page") => {
                    let Some(i) = it.next().and_then(|x| x.parse::<usize>().ok()) else { continue };
                    if i >= PAGES {
                        continue;
                    }
                    let mut page = [Entry::Empty; PLATES];
                    for (k, tok) in it.take(PLATES).enumerate() {
                        page[k] = Entry::from_token(tok);
                    }
                    match block.as_mut() {
                        Some((_, rows)) => rows[i] = page,
                        None => out.pages[i] = page,
                    }
                }
                Some("current") => current = it.next().and_then(|x| x.parse().ok()).unwrap_or(0),
                Some("hidden") => hidden = it.next().and_then(|x| x.parse().ok()).unwrap_or(0),
                _ => {}
            }
        }
        if let Some(b) = block.take() {
            out.saved.push(b);
        }
        out.current = current.min(PAGES - 1);
        out.hidden = sanitise_hidden(hidden);
        // A value may appear on the live page once; a hand-edited file is the only way
        // to break that, and the bar would then hold whichever came first.
        let mut seen: Vec<Entry> = Vec::new();
        for e in out.pages[out.current].iter_mut() {
            if !e.is_empty() {
                if seen.contains(e) {
                    *e = Entry::Empty;
                } else {
                    seen.push(*e);
                }
            }
        }
        if let Some(o) = out.owner {
            // The owner's block is the one the file wrote under `own`; an `arc` block with
            // the same key (a hand-edited file) loses to it.
            out.saved.retain(|(s, _)| *s != o);
            out.saved.push((o, out.pages.clone()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(a: i8, b: i8, c: i8, d: i8) -> [i8; GUEST_SLOTS] {
        [a, b, c, d]
    }

    /// The domain: `-1` empty, `1..=9` skills (`SKILL_MAX`), `10+` beasts, and the values the probe read.
    #[test]
    fn entry_follows_the_guest_byte_domain() {
        assert_eq!(Entry::from_value(-1), Entry::Empty);
        assert_eq!(Entry::from_value(0), Entry::Empty);
        assert_eq!(Entry::from_value(1), Entry::Skill(1));
        assert_eq!(Entry::from_value(7), Entry::Skill(7));
        assert_eq!(Entry::from_value(8), Entry::Skill(8), "the berserker's 도발 (2026-10-07)");
        assert_eq!(Entry::from_value(9), Entry::Skill(9), "the berserker's 격분 (2026-10-07 (2)) — the ninth value; 10+ is a beast");
        assert_eq!(Entry::from_value(10), Entry::Beast(10));
        assert_eq!(Entry::from_value(13), Entry::Beast(13));
        assert_eq!(Entry::from_value(14), Entry::Empty, "a beast has four skill slots");
        for v in [1i8, 10, 5, 2] {
            assert_eq!(Entry::from_value(v).value(), Some(v as u8));
        }
    }

    /// A fresh store meets the probe's bar `[1, 10, 5, 2]`: plates 0..3 take the four
    /// values, seated and confirmed, plates 4..6 stay empty, and nothing is written.
    #[test]
    fn a_fresh_store_adopts_the_live_bar_into_plates_0_to_3() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        assert!(pages.capture(&mut w, bar(1, 10, 5, 2)));
        assert_eq!(pages.plates()[..4], [Entry::Skill(1), Entry::Beast(10), Entry::Skill(5), Entry::Skill(2)]);
        assert!(pages.plates()[4..].iter().all(|e| e.is_empty()));
        for g in 0..4u8 {
            assert_eq!(w.plate_at(g), Some(g));
            assert!(!w.is_pending(g));
        }
        // A second identical capture changes nothing.
        assert!(!pages.capture(&mut w, bar(1, 10, 5, 2)));
    }

    /// Registration through the popup: the plate gets a seat, the game writes the byte
    /// later, and the capture must not empty the plate while it waits.
    #[test]
    fn a_reserved_seat_waits_for_the_games_write() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, -1, -1, -1));
        // Plate 5 registers skill 3: an empty seat (slot 1) is picked.
        let g = pages.reserve_for_bind(&mut w, 5, 3).expect("a seat");
        assert_eq!(g, 1);
        assert_eq!(pages.plate(5), Entry::Skill(3));
        assert!(w.is_pending(1));
        // The game has not written yet: the slot still reads empty, for a few polls.
        for _ in 0..PENDING_POLLS - 1 {
            pages.capture(&mut w, bar(1, -1, -1, -1));
            assert_eq!(pages.plate(5), Entry::Skill(3), "still waiting");
        }
        // Then it lands.
        pages.capture(&mut w, bar(1, 3, -1, -1));
        assert_eq!(w.plate_at(1), Some(5));
        assert!(!w.is_pending(1));
        assert_eq!(pages.plate(5), Entry::Skill(3));
    }

    #[test]
    fn delayed_registration_preserves_all_seven_plates() {
        let values = [1, 10, 2, 11, 3, 12, 4];
        let mut pages = Pages::new();
        let mut window = Window::new();
        pages.adopt_owner(&mut window, Owner { slot: 0, class: 0 });
        let mut live = [-1; GUEST_SLOTS];
        for (plate, value) in values.into_iter().enumerate() {
            let guest = pages.reserve_for_bind(&mut window, plate as u8, value).unwrap();
            for _ in 0..PENDING_POLLS - 1 {
                pages.capture(&mut window, live);
                for p in 0..=plate {
                    assert_eq!(pages.plate(p as u8).value(), Some(values[p]));
                }
            }
            live[guest as usize] = value as i8;
            pages.capture(&mut window, live);
        }
        let expected = *pages.plates();
        for plate in 0..PLATES as u8 {
            if let Activation::Bind { guest, value } = pages.activate(&mut window, plate) {
                live[guest as usize] = value as i8;
            }
            pages.capture(&mut window, live);
            assert_eq!(*pages.plates(), expected);
        }
        assert!(pages.commit(), "the game saved: the seven plates go to file");
        let restored = Pages::parse(&pages.serialize());
        assert_eq!(restored.plates(), &expected);
    }

    #[test]
    fn expired_registration_restores_the_evicted_seat_without_moving_its_skill() {
        let mut pages = Pages::new();
        let mut window = Window::new();
        let live = bar(1, 10, 2, 11);
        pages.capture(&mut window, live);
        let original = *pages.plates();
        let guest = pages.reserve_for_bind(&mut window, 6, 12).unwrap();
        for _ in 0..=PENDING_POLLS {
            pages.capture(&mut window, live);
        }
        assert_eq!(*pages.plates(), original);
        assert_eq!(window.plate_at(guest), Some(guest));
    }

    /// If the game never writes (its own guard refused), the budget runs out and the
    /// plate is emptied rather than showing a skill the bar does not have.
    #[test]
    fn an_unfulfilled_reservation_expires_to_empty() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(-1, -1, -1, -1));
        pages.reserve_for_bind(&mut w, 2, 4);
        for _ in 0..=PENDING_POLLS {
            pages.capture(&mut w, bar(-1, -1, -1, -1));
        }
        assert_eq!(pages.plate(2), Entry::Empty);
        assert_eq!(w.plate_at(0), None);
    }

    /// Seven plates over four slots: a fifth skill pressed on the field evicts the
    /// least recently used seat, and pressing the evicted plate later gets it back.
    #[test]
    fn a_non_resident_plate_is_bound_into_the_lru_seat() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, 2, 3, 4));
        // Put skill 5 on plate 4 as the popup flow would (evicts the LRU seat = slot 0,
        // never used since adoption... all equal; slot 0 is the min).
        // Use slots 1..3 first so slot 0 is the LRU.
        for g in 1..4u8 {
            w.note_used(g);
        }
        let g = pages.reserve_for_bind(&mut w, 4, 5).unwrap();
        assert_eq!(g, 0, "slot 0 was least recently used");
        pages.capture(&mut w, bar(5, 2, 3, 4));
        assert_eq!(w.plate_at(0), Some(4));
        assert_eq!(pages.plate(0), Entry::Skill(1), "the evicted plate keeps its skill");
        assert_eq!(w.guest_of(0), None, "…but is not resident");
        // Pressing plate 0 now binds it into the LRU seat.
        match pages.activate(&mut w, 0) {
            Activation::Bind { guest, value } => {
                assert_eq!(value, 1);
                assert!(guest < 4);
            }
            other => panic!("expected a bind, got {other:?}"),
        }
        // Pressing a resident plate is a plain press; an empty plate is nothing.
        assert!(matches!(pages.activate(&mut w, 2), Activation::Resident { guest: 2 }));
        assert!(matches!(pages.activate(&mut w, 6), Activation::Nothing));
    }

    /// The game's own dedup, mirrored: registering a value that another plate holds
    /// moves it (the other plate is cleared).
    #[test]
    fn a_value_lives_on_one_plate_only() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, 2, -1, -1));
        let g = pages.reserve_for_bind(&mut w, 6, 1).unwrap();
        assert_eq!(pages.plate(0), Entry::Empty, "plate 0 lost skill 1 to plate 6");
        assert_eq!(pages.plate(6), Entry::Skill(1));
        assert_eq!(g, 0, "plate 0's freed seat is the empty one");
        // The game writes 1 into slot 0 (it was 1 already): plate 6 is confirmed there,
        // and skill 1 is on exactly one plate.
        pages.capture(&mut w, bar(1, 2, -1, -1));
        assert_eq!(w.plate_at(0), Some(6));
        assert_eq!(pages.plates().iter().filter(|e| **e == Entry::Skill(1)).count(), 1);
        // And if the GAME rebinds a seated slot behind our back, the plate follows.
        pages.capture(&mut w, bar(4, 2, -1, -1));
        assert_eq!(pages.plate(6), Entry::Skill(4));
        assert_eq!(pages.plate_holding(1), None);
    }

    /// A beast change clears `>= 10` bytes in the guest; a confirmed plate follows.
    #[test]
    fn a_slot_the_game_empties_empties_its_plate() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, 10, 5, 2));
        assert!(pages.capture(&mut w, bar(1, -1, 5, 2)));
        assert_eq!(pages.plate(1), Entry::Empty);
        assert_eq!(w.plate_at(1), None);
    }

    /// A flip warms the incoming page's first four plates into the bar, in order,
    /// skipping hidden plates, and clears the rest.
    #[test]
    fn a_flip_plans_the_incoming_pages_first_four_plates() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, 2, 3, 4));
        // Page 1: plates 0, 2, 5, 6 hold skills; plate 2 hidden.
        pages.current = 1;
        pages.set(0, Entry::Skill(6));
        pages.set(2, Entry::Skill(7));
        pages.set(5, Entry::Beast(11));
        pages.set(6, Entry::Skill(5));
        pages.current = 0;
        pages.set_hidden(1 << 2);
        let plan = pages.flip_to(&mut w, 1);
        assert_eq!(plan, vec![(0, Some(6)), (1, Some(11)), (2, Some(5)), (3, None)]);
        assert_eq!(w.plate_at(0), Some(0));
        assert_eq!(w.plate_at(1), Some(5));
        assert_eq!(w.plate_at(2), Some(6));
        assert_eq!(w.plate_at(3), None);
        assert_eq!(pages.current(), 1);
    }

    /// Round trip through the text file, owner and the saved rows of every owner included.
    #[test]
    fn the_store_round_trips_through_text() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        assert_eq!(pages.adopt_owner(&mut w, Owner { slot: 0, class: 2 }), Adopted::Fresh);
        pages.capture(&mut w, bar(1, 10, 5, 2));
        pages.reserve_for_bind(&mut w, 5, 3);
        pages.set_hidden(0b100_0000);
        assert!(pages.commit(), "the game saved");
        // Another owner comes; the first one's saved rows stay on file.
        assert_eq!(pages.adopt_owner(&mut w, Owner { slot: 1, class: 0 }), Adopted::Fresh);
        pages.capture(&mut w, bar(2, -1, -1, -1));
        assert!(pages.commit());
        let text = pages.serialize();
        let back = Pages::parse(&text);
        assert_eq!(back, pages, "\n{text}");
        // Coming back to the first owner restores their rows.
        let mut w2 = Window::new();
        let mut again = back;
        assert_eq!(again.adopt_owner(&mut w2, Owner { slot: 0, class: 2 }), Adopted::Restored);
        assert_eq!(again.plate(5), Entry::Skill(3));
        assert_eq!(again.plate(1), Entry::Beast(10));
        assert_eq!(again.hidden(), 0b100_0000);
        // …and the file, re-read, has the same picture.
        let text2 = again.serialize();
        assert_eq!(Pages::parse(&text2), again, "\n{text2}");
    }

    /// ★ 2026-09-29, the report: learn a skill, register it, die without saving. The save
    /// that reloads has neither the skill nor the registration; nor do the plates — beast
    /// skill (the report) and class skill alike, on a host plate or in a guest seat.
    #[test]
    fn a_registration_after_the_last_save_does_not_survive_death() {
        let owner = Owner { slot: 1, class: 0 };
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.adopt_owner(&mut w, owner);
        pages.capture(&mut w, bar(1, -1, -1, -1));
        assert!(pages.commit(), "the last save: skill 1 in slot 0, nothing else");
        // A beast skill learned and put on plate 4, a class skill on plate 5 — the game
        // writes both into the bar.
        let gb = pages.reserve_for_bind(&mut w, 4, 10).unwrap();
        let gs = pages.reserve_for_bind(&mut w, 5, 2).unwrap();
        let mut live = bar(1, -1, -1, -1);
        live[usize::from(gb)] = 10;
        live[usize::from(gs)] = 2;
        pages.capture(&mut w, live);
        assert_eq!(pages.plate(4), Entry::Beast(10));
        assert_eq!(pages.plate(5), Entry::Skill(2));
        // What is on disk meanwhile is the SAVED picture.
        let on_disk = Pages::parse(&pages.serialize());
        assert_eq!(on_disk.plate(0), Entry::Skill(1));
        assert!(on_disk.plate(4).is_empty() && on_disk.plate(5).is_empty(), "unsaved registrations are not on file");
        // Death: the title comes up (seats cleared), 이어하기 loads the save — bar [1, -1, -1, -1].
        w.clear();
        assert_eq!(pages.adopt_owner(&mut w, owner), Adopted::RolledBack);
        assert!(!pages.capture(&mut w, bar(1, -1, -1, -1)), "nothing to reconcile: the rows already are the save's");
        assert_eq!(pages.plate(0), Entry::Skill(1));
        assert!(pages.plate(4).is_empty(), "the beast skill is gone with the unsaved progress");
        assert!(pages.plate(5).is_empty(), "…and the class skill the same way");
        assert_eq!(w.plate_at(0), Some(0));
        assert_eq!(w.residents().len(), 1);
        // The same through a restart: the file says the same.
        let mut w2 = Window::new();
        let mut restarted = on_disk;
        assert_eq!(restarted.adopt_owner(&mut w2, owner), Adopted::Kept);
        assert!(restarted.plate(4).is_empty() && restarted.plate(5).is_empty());
    }

    /// ★ 2026-10-09, the report (S25 log): *"내가 등록하지 않은 퀵슬롯에 스킬이 등록되어있는데"* — saved on page 1 (격분 9 ·
    /// 5 · 4 · 10 in the guest bar), went to another character and back: the rows came back on page 0 and the capture put
    /// page 1's seats onto page 0's empty plates (`[6, 5, 8, B10, 4, 9, …]`). A world entry must show the page the bar is
    /// from, and leave every plate as saved.
    #[test]
    fn a_world_entry_shows_the_page_the_saved_bar_came_from() {
        let berserker = Owner { slot: 0, class: 0 };
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.adopt_owner(&mut w, berserker);
        // page 0: 격분 · 6 · 7 · 8 · beast 10 · two empty plates; page 1: 격분 · 5 · 4 · 10 · 11
        for (p, e) in [Entry::Skill(9), Entry::Skill(6), Entry::Skill(7), Entry::Skill(8), Entry::Beast(10)].into_iter().enumerate() {
            pages.set(p as u8, e);
        }
        pages.current = 1;
        for (p, e) in [Entry::Skill(9), Entry::Skill(5), Entry::Skill(4), Entry::Beast(10), Entry::Beast(11)].into_iter().enumerate() {
            pages.set(p as u8, e);
        }
        let saved_rows = pages.pages.clone();
        // live on page 1: the flip seated its first four plates; the game saves
        pages.flip_to(&mut w, 1);
        pages.capture(&mut w, bar(9, 5, 4, 10));
        assert!(pages.commit());
        // another character, then this one again: the save's bar is page 1's
        pages.adopt_owner(&mut w, Owner { slot: 3, class: 2 });
        assert_eq!(pages.adopt_owner(&mut w, berserker), Adopted::Restored);
        assert_eq!(pages.match_page(bar(9, 5, 4, 10)), 1, "the page that holds the bar");
        assert!(!pages.capture(&mut w, bar(9, 5, 4, 10)), "nothing captured: every value is on its plate");
        assert_eq!(pages.pages, saved_rows, "no plate on either page changed");
        assert_eq!(w.residents().len(), 4);
        // the same owner rolled back (died on page 0 after saving on page 1) — the same answer
        pages.flip_to(&mut w, 0);
        w.clear();
        assert_eq!(pages.adopt_owner(&mut w, berserker), Adopted::Kept);
        assert_eq!(pages.current(), 0, "the live page as it was left");
        assert_eq!(pages.match_page(bar(9, 5, 4, 10)), 1);
        assert!(!pages.capture(&mut w, bar(9, 5, 4, 10)));
        assert_eq!(pages.pages, saved_rows);
        // a tie keeps the live page: 격분 alone is on both
        assert_eq!(pages.match_page(bar(9, -1, -1, -1)), 1);
        pages.current = 0;
        assert_eq!(pages.match_page(bar(9, -1, -1, -1)), 0);
        // a bar no page holds (a keypad registration, an empty bar) keeps it too
        assert_eq!(pages.match_page(bar(-1, -1, -1, -1)), 0);
        assert_eq!(pages.match_page(bar(1, 2, -1, -1)), 0);
    }

    /// `commit` writes the live rows over the owner's saved block, once per change, never
    /// for rows that belong to nobody, and the ring drops the oldest block, never the one
    /// just committed.
    #[test]
    fn a_commit_makes_the_live_rows_the_saved_ones() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        assert!(!pages.commit(), "no owner: nothing to save under");
        pages.adopt_owner(&mut w, Owner { slot: 2, class: 1 });
        pages.capture(&mut w, bar(1, 2, -1, -1));
        assert!(pages.commit());
        assert!(!pages.commit(), "unchanged");
        pages.reserve_for_bind(&mut w, 6, 3);
        assert!(pages.commit(), "changed");
        assert_eq!(Pages::parse(&pages.serialize()).plate(6), Entry::Skill(3));
        // Every reachable owner saves, then some a bigger build might have: the ring drops
        // the oldest — (2, 1), saved first — and keeps the owner just committed.
        for slot in 0..8u8 {
            for class in 0..3u8 {
                if (slot, class) == (2, 1) {
                    continue;
                }
                pages.adopt_owner(&mut w, Owner { slot, class });
                pages.capture(&mut w, bar(class as i8 + 1, -1, -1, -1));
                pages.commit();
            }
        }
        assert_eq!(pages.saved.len(), ARCHIVE_MAX);
        assert_eq!(pages.owner(), Some(Owner { slot: 7, class: 2 }));
        assert_eq!(pages.plate(0), Entry::Skill(3));
        assert_eq!(pages.adopt_owner(&mut w, Owner { slot: 2, class: 1 }), Adopted::Fresh, "the oldest block is gone");
    }

    /// The guard for rows the rollback cannot see (2026-09-29): unseated plates holding a
    /// value the character does not have are named, on every page; seated plates never are.
    #[test]
    fn unlearned_plates_are_the_unseated_ones_on_every_page() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, 10, -1, -1));
        // Plate 5 holds skill 3 unseated (an evicted registration), plate 6 an item; page 1
        // holds beast 11 and skill 4.
        pages.set(5, Entry::Skill(3));
        pages.set(6, Entry::Item(24));
        pages.current = 1;
        pages.set(0, Entry::Beast(11));
        pages.set(2, Entry::Skill(4));
        pages.current = 0;
        // The character has 1 and 4 only.
        let has = |v: u8| v == 1 || v == 4;
        assert_eq!(pages.unlearned_plates(&w, |v| !has(v)), vec![(0, 5, 3), (1, 0, 11)], "seated 1 and 10 are the bar's; items have no value");
        // Even a seated plate whose value the character lacks is left alone (it is the
        // bar's, and the game's own bind rule vouches for it).
        let all = pages.unlearned_plates(&w, |_| true);
        assert_eq!(all.iter().map(|g| (g.0, g.1)).collect::<Vec<_>>(), vec![(0, 5), (1, 0), (1, 2)]);
        assert!(pages.clear_plate_on(0, 5));
        assert!(!pages.clear_plate_on(0, 5), "already empty");
        assert!(pages.clear_plate_on(1, 0));
        assert!(!pages.clear_plate_on(9, 0), "no such page");
        assert_eq!(pages.plate(5), Entry::Empty);
        assert_eq!(pages.page(1).unwrap()[0], Entry::Empty);
        assert_eq!(pages.page(1).unwrap()[2], Entry::Skill(4), "learned: kept");
        assert_eq!(pages.plate(0), Entry::Skill(1));
        assert_eq!(pages.plate(1), Entry::Beast(10));
        assert_eq!(w.residents().len(), 2, "no seat was touched");
    }

    /// ★ 2026-09-27: a NEW GAME in a slot drops every row that slot has had — its live
    /// rows (same class = same key, which a reload rightly keeps) and its saved blocks of
    /// every class — and nothing of another slot's.
    #[test]
    fn a_new_game_in_a_slot_starts_on_empty_plates() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        // Slot 3 once held a 버서커, then a 로그; slot 0 a 나이트. Each of them saved.
        pages.adopt_owner(&mut w, Owner { slot: 3, class: 0 });
        pages.capture(&mut w, bar(1, 2, -1, -1));
        pages.commit();
        pages.adopt_owner(&mut w, Owner { slot: 0, class: 1 });
        pages.capture(&mut w, bar(4, -1, -1, -1));
        pages.commit();
        pages.adopt_owner(&mut w, Owner { slot: 3, class: 2 });
        pages.capture(&mut w, bar(5, 6, -1, -1));
        pages.commit();
        assert_eq!(pages.plate(0), Entry::Skill(5));
        // 새로하기 in slot 3 (a 로그 again — the same key).
        assert!(pages.forget_slot(&mut w, 3));
        assert_eq!(pages.owner(), None);
        assert_eq!(pages.adopt_owner(&mut w, Owner { slot: 3, class: 2 }), Adopted::Fresh, "a fresh owner, not a kept one");
        assert!((0..PLATES as u8).all(|p| pages.plate(p).is_empty()), "the old 로그's plates are gone");
        // …and a 버서커 later in slot 3 gets no leftovers either; slot 0 keeps its own.
        pages.adopt_owner(&mut w, Owner { slot: 3, class: 0 });
        assert!((0..PLATES as u8).all(|p| pages.plate(p).is_empty()), "the older 버서커's saved rows went too");
        assert!(pages.commit(), "…and its empty rows are saved anew");
        pages.adopt_owner(&mut w, Owner { slot: 0, class: 1 });
        assert_eq!(pages.plate(0), Entry::Skill(4), "another slot is untouched");
        // The saved (empty) 버서커 block of slot 3 is still a block; then nothing is left.
        pages.adopt_owner(&mut w, Owner { slot: 1, class: 1 });
        assert!(pages.forget_slot(&mut w, 3), "slot 3's empty 버서커 block is still a block");
        assert!(!pages.forget_slot(&mut w, 3));
    }

    /// The watch: the last slot-screen mode before the world decides; a cancelled pick
    /// (CLR back to a bare title) does not survive; the answer is taken once.
    #[test]
    fn the_new_game_watch_remembers_the_class_pick_until_the_world() {
        const PICK: i32 = 11;
        let mut w = NewGameWatch::default();
        w.sample(Some(Some(10)), PICK); // 새로하기
        w.sample(Some(Some(PICK)), PICK); // class pick
        w.sample(Some(None), PICK); // the pick's OK: a frame or two of bare title
        w.sample(Some(None), PICK);
        w.sample(None, PICK); // the shell is down: the intro
        assert!(w.take(), "new game");
        assert!(!w.take(), "taken once");
        // 이어하기 after a cancelled pick.
        w.sample(Some(Some(PICK)), PICK);
        for _ in 0..NewGameWatch::BARE_FRAMES {
            w.sample(Some(None), PICK); // CLR: the bare title
        }
        w.sample(Some(Some(1)), PICK); // 이어하기
        assert!(!w.take(), "a load, not a new game");
        // A pick then straight back to 이어하기 without a bare title also says load.
        w.sample(Some(Some(PICK)), PICK);
        w.sample(Some(Some(1)), PICK);
        assert!(!w.take());
    }

    /// A reload hands the same owner the rows of its LAST SAVE, and seats are re-derived
    /// from the live bar with no writes: plates whose value is in the bar are seated, the
    /// rest unseated. What was saved comes back; what was registered after the save does
    /// not (2026-09-29).
    #[test]
    fn a_reload_puts_back_the_rows_of_the_last_save() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.adopt_owner(&mut w, Owner { slot: 0, class: 2 });
        pages.capture(&mut w, bar(1, 10, 5, 2));
        // Plate 5 registers skill 3. The bar is full, so the LRU seat — slot 0, seated
        // first at adoption — is taken from plate 0, which keeps its skill unseated.
        let g = pages.reserve_for_bind(&mut w, 5, 3).unwrap();
        assert_eq!(g, 0);
        pages.capture(&mut w, bar(3, 10, 5, 2)); // the game wrote 3 into slot 0
        assert_eq!(w.plate_at(0), Some(5));
        assert_eq!(pages.plate(0), Entry::Skill(1));
        assert_eq!(w.guest_of(0), None);
        // The player SAVES: the bar [3, 10, 5, 2] goes into the save, the rows into the store.
        assert!(pages.commit());
        // Then registers skill 4 on plate 6 (the LRU seat) — and does not save.
        let g = pages.reserve_for_bind(&mut w, 6, 4).unwrap();
        let mut live = bar(3, 10, 5, 2);
        live[usize::from(g)] = 4;
        pages.capture(&mut w, live);
        assert_eq!(pages.plate(6), Entry::Skill(4));
        assert!(Pages::parse(&pages.serialize()).plate(6).is_empty(), "the unsaved registration is not on file");
        // The app restarts (or the character dies); the same owner loads the save, whose
        // bar is [3, 10, 5, 2] again.
        let mut w2 = Window::new();
        assert_eq!(pages.adopt_owner(&mut w2, Owner { slot: 0, class: 2 }), Adopted::RolledBack);
        assert_eq!(pages.plate(6), Entry::Empty, "registered after the save: gone");
        pages.capture(&mut w2, bar(3, 10, 5, 2));
        // Every plate whose value is in the bar is seated where the bar has it; the
        // plate holding skill 1 stays unseated. No write was needed for any of it.
        assert_eq!(w2.plate_at(0), Some(5));
        assert_eq!(w2.plate_at(1), Some(1));
        assert_eq!(w2.plate_at(2), Some(2));
        assert_eq!(w2.plate_at(3), Some(3));
        assert_eq!(w2.guest_of(0), None);
        assert_eq!(pages.plate(0), Entry::Skill(1), "saved before: kept, unseated");
        assert_eq!(w2.residents().len(), 4);
        // A second entry with nothing changed since is a plain keep.
        let mut w3 = Window::new();
        assert_eq!(pages.adopt_owner(&mut w3, Owner { slot: 0, class: 2 }), Adopted::Kept);
    }

    /// A parsed file never yields two plates with the same value or a page/hidden
    /// index outside the store.
    #[test]
    fn parse_is_defensive() {
        let p = Pages::parse("page 0 1 1 2 x 99 - 10\ncurrent 0\nhidden 255\npage 9 1 2 3\n");
        assert_eq!(Pages::parse("current 7\n").current(), PAGES - 1, "page index clamped");
        assert_eq!(p.hidden(), SHOW_ALL, "a file that hides every plate degrades to none hidden");
        assert_eq!(Pages::parse("hidden 190\n").hidden(), 0b011_1110, "bits past the row dropped");
        assert_eq!(p.plates()[0], Entry::Skill(1));
        assert_eq!(p.plates()[1], Entry::Empty, "duplicate dropped");
        assert_eq!(p.plates()[3], Entry::Empty, "junk token");
        assert_eq!(p.plates()[4], Entry::Empty, "99 is outside the domain");
        assert_eq!(p.plates()[6], Entry::Beast(10));
    }

    /// S7: an item plate names a type, takes no seat, presses `0`, and round-trips the
    /// file as `i<type>`. Registering one over a RESIDENT skill hands back the seat to
    /// clear, so the skill cannot resurface on an empty plate at the next capture.
    #[test]
    fn item_plates_take_no_seat_and_hand_back_the_one_they_replace() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.adopt_owner(&mut w, Owner { slot: 1, class: 0 });
        pages.capture(&mut w, bar(1, 10, -1, -1));
        // Plate 5 (unseated, empty) takes 포션.
        assert_eq!(pages.set_item(&mut w, 5, 24), Some(None));
        assert_eq!(pages.plate(5), Entry::Item(24));
        assert_eq!(pages.activate(&mut w, 5), Activation::Item { kind: 24 });
        assert_eq!(w.residents().len(), 2, "no seat was taken");
        // The same type moves: plate 6 takes it, plate 5 empties.
        assert_eq!(pages.set_item(&mut w, 6, 24), Some(None));
        assert_eq!(pages.plate(5), Entry::Empty);
        assert_eq!(pages.plate_holding_item(24), Some(6));
        // Over the resident skill on plate 0: seat 0 comes back for the caller to clear.
        assert_eq!(pages.set_item(&mut w, 0, 25), Some(Some(0)));
        assert_eq!(w.guest_of(0), None);
        assert_eq!(w.plate_at(0), None);
        // The bar still says slot 0 holds skill 1 until the caller's clear lands; a
        // capture that reads it cleared changes nothing.
        assert!(!pages.capture(&mut w, bar(-1, 10, -1, -1)));
        assert_eq!(pages.plate(0), Entry::Item(25));
        // The file carries item plates as `i<type>` — once saved.
        assert!(pages.commit());
        let text = pages.serialize();
        assert!(text.contains("page 0 i25 10 - - - - i24"), "{text}");
        let back = Pages::parse(&text);
        assert_eq!(back.plate(0), Entry::Item(25));
        assert_eq!(back.plate(6), Entry::Item(24));
        assert_eq!(Entry::from_token("i0"), Entry::Empty);
        assert_eq!(Entry::from_token("i64"), Entry::Empty);
        assert_eq!(Pages::parse("page 0 i24 i24 - - - - -
").plate(1), Entry::Empty, "duplicate item dropped");
        assert!(pages.forget_item(24));
        assert!(!pages.forget_item(24));
        assert_eq!(pages.plate(6), Entry::Empty);
        // Hidden plates refuse, like a skill bind.
        pages.set_hidden(1 << 3);
        assert_eq!(pages.set_item(&mut w, 3, 26), None);
    }

    // ── the HIDDEN set (hero3 82차) ──────────────────────────────────────────

    /// The mask rules: bit per plate, out-of-range reads visible, the visible order is
    /// the ring order, and the last plate cannot go.
    #[test]
    fn hidden_mask_rules() {
        assert!(!plate_hidden(SHOW_ALL, 0));
        assert!(plate_hidden(0b100_0000, 6));
        assert!(!plate_hidden(0xff, 7), "plate 7 does not exist — reads visible");
        assert_eq!(visible_plates(0b010_0010), vec![0, 2, 3, 4, 6]);
        assert_eq!(hidden_plates(0b010_0010), vec![1, 5]);
        assert_eq!(visible_count(0b010_0010), 5);
        assert_eq!(visible_index(0b010_0010, 2), Some(1), "plate 2 is the second visible");
        assert_eq!(visible_index(0b010_0010, 1), None, "hidden plates have no seat");
        assert_eq!(visible_index(SHOW_ALL, 9), None, "out of range has no seat");
        // Hiding one at a time down to the last.
        let mut m = SHOW_ALL;
        for p in 0..(PLATES as u8 - 1) {
            m = hide_plate(m, p).expect("not the last plate");
        }
        assert_eq!(visible_count(m), 1);
        assert_eq!(hide_plate(m, PLATES as u8 - 1), None, "the last visible plate is refused");
        assert_eq!(hide_plate(m, 0), None, "already hidden");
        assert_eq!(hide_plate(m, 9), None, "out of range");
        // Sanitising: junk bits dropped, "hide everything" degrades to nothing hidden.
        assert_eq!(sanitise_hidden(0xff), SHOW_ALL);
        assert_eq!(sanitise_hidden(0b1000_0001), 0b000_0001);
        assert_eq!(sanitise_hidden(m), m);
        let mut pages = Pages::new();
        pages.set_hidden(0x7f);
        assert_eq!(pages.hidden(), SHOW_ALL, "the store sanitises on the way in");
    }

    /// Hidden plates neither fire nor take a seat, and hiding one never clears its
    /// binding — SHOW brings it back exactly as it was.
    #[test]
    fn a_hidden_plate_keeps_its_binding_but_never_fires() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, 2, 3, 4));
        pages.reserve_for_bind(&mut w, 5, 6);
        assert_eq!(pages.plate(5), Entry::Skill(6));
        pages.set_hidden(1 << 5 | 1 << 1);
        assert_eq!(pages.activate(&mut w, 5), Activation::Nothing, "hidden plates do not fire");
        assert_eq!(pages.activate(&mut w, 1), Activation::Nothing);
        assert_eq!(pages.reserve_for_bind(&mut w, 5, 7), None, "…and cannot be registered into");
        assert_eq!(pages.plate(5), Entry::Skill(6), "the binding is untouched");
        assert_eq!(pages.plate(1), Entry::Skill(2));
        pages.set_hidden(SHOW_ALL);
        assert!(matches!(pages.activate(&mut w, 5), Activation::Resident { .. }), "SHOW: back as it was");
    }

    /// A value the game bound that no plate holds lands on a VISIBLE empty plate,
    /// even when a hidden one comes first in plate order; only when nothing visible is
    /// free does a hidden plate take it.
    #[test]
    fn the_bar_is_adopted_onto_visible_plates_first() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.set_hidden(0b000_0011);
        assert!(pages.capture(&mut w, bar(1, 2, -1, -1)));
        assert_eq!(pages.plate(0), Entry::Empty, "hidden plate 0 skipped");
        assert_eq!(pages.plate(1), Entry::Empty, "hidden plate 1 skipped");
        assert_eq!(pages.plate(2), Entry::Skill(1));
        assert_eq!(pages.plate(3), Entry::Skill(2));
        assert_eq!(w.plate_at(0), Some(2));
        assert_eq!(w.plate_at(1), Some(3));
        // Every visible plate full: the next value falls back to a hidden plate rather
        // than being lost (the slot would otherwise be owned by nothing).
        pages.set_hidden(0b111_1100);
        let mut pages2 = Pages::new();
        let mut w2 = Window::new();
        pages2.set_hidden(0b111_1100);
        assert!(pages2.capture(&mut w2, bar(1, 2, 3, -1)));
        assert_eq!(pages2.plate(0), Entry::Skill(1));
        assert_eq!(pages2.plate(1), Entry::Skill(2));
        assert_eq!(pages2.plate(2), Entry::Skill(3), "overflow onto the first hidden plate");
        assert_eq!(w2.plate_at(2), Some(2));
        let _ = pages;
    }

    // ── parked cooldowns (2026-10-02) ────────────────────────────────────────

    /// A parked counter loses one per game tick and reads nothing once it has run out;
    /// a counter that was not running parks nothing (and drops an older entry).
    #[test]
    fn a_parked_cooldown_ages_by_the_games_own_ticks() {
        let mut w = Window::new();
        w.park(5, 600, 900, 1000);
        assert_eq!(w.parked(5, 1000), Some((600, 900)));
        assert_eq!(w.parked(5, 1250), Some((350, 900)), "250 ticks later");
        assert_eq!(w.parked(5, 1599), Some((1, 900)));
        assert_eq!(w.parked(5, 1600), None, "0 is the game's ready (`cd <= 0` casts)");
        assert_eq!(w.parked(5, 9000), None);
        assert_eq!(w.parked(6, 1000), None, "another skill");
        // The clock is a wrapping count; a total the host never saw is at least what is left.
        w.park(7, 100, 0, u32::MAX - 9);
        assert_eq!(w.parked(7, 20), Some((70, 100)), "30 ticks across the wrap");
        // Seating takes the entry: what is left, or ready.
        assert_eq!(w.unpark(5, 1250), Some((350, 900)));
        assert_eq!(w.unpark(5, 1250), None, "taken — the guest's counter has it now");
        assert_eq!(w.unpark(6, 1250), None, "never parked: ready");
        // A ready counter parks nothing and drops the older entry.
        w.park(7, 0, 100, 50);
        assert_eq!(w.parked(7, 50), None);
        w.park(8, 40, 40, 50);
        w.park(8, -1, 40, 60);
        assert_eq!(w.unpark(8, 60), None);
    }

    /// ★★ 2026-10-14 시간도둑: `shorten_parked` takes ticks off every parked counter; one that runs out is gone.
    #[test]
    fn the_time_thief_shortens_the_parked_cooldowns() {
        let mut w = Window::new();
        w.park(5, 600, 900, 1000);
        w.park(6, 15, 300, 1000);
        w.shorten_parked(20);
        assert_eq!(w.parked(5, 1000), Some((580, 900)));
        assert_eq!(w.parked(6, 1000), None, "15 left − 20: ready");
        w.shorten_parked(20);
        assert_eq!(w.parked(5, 1100), Some((460, 900)), "and it still ages by the game's ticks");
    }

    /// The report itself: page A's skill is cast, the page is flipped away and back — the
    /// seats are rebuilt both times, the parked counter is not.
    #[test]
    fn a_page_flip_keeps_the_parked_cooldowns() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.adopt_owner(&mut w, Owner { slot: 0, class: 0 });
        pages.capture(&mut w, bar(5, -1, -1, -1));
        // The worker parks the outgoing bar's running counters, then flips.
        w.park(5, 600, 600, 100);
        let plan = pages.flip_to(&mut w, 1);
        assert_eq!(plan, vec![(0, None), (1, None), (2, None), (3, None)]);
        assert_eq!(w.parked(5, 100), Some((600, 600)), "the flip cleared the seats, not the cooldown");
        pages.capture(&mut w, bar(-1, -1, -1, -1));
        // 200 ticks on page B, then back.
        let plan = pages.flip_to(&mut w, 0);
        assert_eq!(plan[0], (0, Some(5)));
        assert_eq!(w.unpark(5, 300), Some((400, 600)), "the slot is seated with what is left");
        // A world entry starts clean — the load reset the game's counters too.
        w.park(5, 400, 600, 300);
        pages.adopt_owner(&mut w, Owner { slot: 0, class: 0 });
        assert_eq!(w.parked(5, 300), None);
    }

    /// A skill READ in its seat cools in the guest: the capture drops what was parked for
    /// it (a registration the game refused puts the evicted skill straight back).
    #[test]
    fn a_confirmed_seat_drops_its_parked_cooldown() {
        let mut pages = Pages::new();
        let mut w = Window::new();
        pages.capture(&mut w, bar(1, 2, 3, 4));
        // Plate 6 registers skill 5 into the LRU seat (slot 0): skill 1's counter is parked.
        let g = pages.reserve_for_bind(&mut w, 6, 5).unwrap();
        assert_eq!(g, 0);
        w.park(1, 300, 300, 10);
        // The game never writes; the reservation expires and plate 0 gets its seat back.
        for _ in 0..=PENDING_POLLS {
            pages.capture(&mut w, bar(1, 2, 3, 4));
        }
        assert_eq!(w.plate_at(0), Some(0));
        assert_eq!(w.parked(1, 10), None, "slot 0 never stopped counting for skill 1");
        // …while a pending seat does not touch the entry of the value it waits for.
        w.park(6, 50, 50, 10);
        pages.reserve_for_bind(&mut w, 5, 6).unwrap();
        pages.capture(&mut w, bar(1, 2, 3, 4));
        assert_eq!(w.parked(6, 10), Some((50, 50)));
    }
}
