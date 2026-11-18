//! Field action-quickbar HUD support: locate the live CHAR, read the 5 quickbar
//! slots (skill / item / empty), resolve item stack counts, and gate on the
//! real-time field state. The HUD buttons ACTIVATE a slot by sending its fixed
//! hotkey (see `crate::quickslot_hotkey`) — a normal key event the game handles
//! natively (dispatcher 0x143048), so there is NO memory injection or guest call
//! on the trigger path.
//!
//! All guest-RAM reads go through a caller-supplied `read(addr, len) -> Vec<u8>`
//! (backed by `Emulator::read_memory`), so the parsing is pure + unit-testable.
//!
//! ── RE PROVENANCE (Hero3 client.bin, md5 3792a476) ───────────────────────────
//! CONFIRMED (HIGH) by workflows wf_b06784c0 / wf_9fcbeeaf + live device dumps:
//!   getCHAR (guest 0x14ad10) = *(sl+0x444), sl = 0x1b2c40  → CHAR ptr @ 0x1b3084.
//!   Quickbar (CHAR-relative, stride-1 parallel arrays over 5 slots):
//!     type[s]      u8 @ CHAR+0x252+s   (0=empty, 1=skill, 2=item)
//!     item-ref[s]  u8 @ CHAR+0x257+s   (tab = ref>>5, slot = ref&0x1f)
//!     skill-ref[s] u8 @ CHAR+0x25c+s   (skill id = ref&0x1f)
//!     current      u8 @ CHAR+0x261
//!   Slot->hotkey (static, from dispatcher 0x143048 jump table): 0=1,1=3,2=9,3=0,4=#.
//!   Item stack count lives in the inventory grid (double-indirect WIPI handle):
//!     base = *(*(CHAR+0x9e28)) + 8 ; record = base + tab*0x3c4 + slot*0x3c ;
//!     count u16 @ record+0x3ba.  (Live-resolve every read; the base moves.)
//!   State = *(u32)(CHAR+0x7c); field free-roam == 4; selector overlay == 5.
//!   Field gate = (state==4) && (*(u8)(CHAR+0x9c6c)==0).

use crate::{IconRef, QuickSlot, SlotKind, Stats};

// Guest address model: guest = file + 0x100000; static base sl = 0x1b2c40.

/// Pointer global read by getCHAR (guest 0x14ad10 = *(sl+0x444)); sl+0x444 = 0x1b3084.
/// Loose plausible-pointer window (reject null/tiny). TWO live regions: binary/
/// static ~0x10_0000.. and dynamic heap 0x4000_0000+ (grid handles / GameSave).
const HEAP_LO: u32 = 0x10_0000;
const HEAP_HI: u32 = 0x8000_0000;

/// Quickbar arrays, relative to CHAR (stride 1). The OFFSETS are hero3 RE and are
/// asleep behind the CHAR-anchor gate; the COUNT is the GUEST width and comes from
/// [`crate::GUEST_QUICK_SLOTS`] — four, because that is how many keys hero2 routes to
/// slots and how many rows its own registration popup offers.
///
/// ⚠ Deliberately NOT `crate::QUICK_SLOTS` (5). The host bar is one plate wider than
/// the guest's array; every read and every write below must stop at the guest width or
/// it walks past the end of four byte arrays in CHAR.
const QB_SLOTS: u32 = crate::GUEST_QUICK_SLOTS as u32;
const QB_TYPE_OFF: u32 = 0x252;
const QB_ITEMREF_OFF: u32 = 0x257;
const QB_SKILLREF_OFF: u32 = 0x25c;
const TY_SKILL: u8 = 1;
const TY_ITEM: u8 = 2;
/// Consumable item category (i18_dat); its icon frame comes from the item-DB.
const CONSUMABLE_CAT: u16 = 0x12;

/// Inventory grid — used only to resolve an item slot's count. Reached through
/// [`deref_container`], which is where the per-build hop count lives.
const GRID_HANDLE_OFF: u32 = 0x9e28;
const TAB_STRIDE: u32 = 0x3c4;
const SLOT_STRIDE: u32 = 0x3c;
const COUNT_OFF: u32 = 0x3ba;

// Game-state gate. A COARSE app state — sole writer `setGameState` (KTF guest
// 0x148bf8): 2 = boot, 3 = title/out-of-world shell, 4 = real-time field free-roam,
// 5 = quick-item selector overlay. ★ It does NOT change when one of the game's
// windows (menu / map / save / inventory) opens, so it is not enough on its own to
// tell "is a window covering the screen?" — see [`WINDOW_ID_OFF`].
//
// ★★ 2026-09-09 — BOTH the offset and the values moved into
// `guest_anchors::AppState`, and the constants that used to sit here are gone.
// They were `STATE_FIELD = 4` / `STATE_SELECTOR = 5`, device-measured on KTF and
// probe-measured on LGT (opening the menu moves `CHAR+0x7c` 4 -> 5 while the window id
// stays 3 and the selector flag goes to 1; 134 probe samples
// `window=3 state=5 selector=1 types=[1,1,1,1,2]` — and a character is just as live
// there as in the field, which is why `in_game_state` accepts it).
//
// hero2 forced the move: its state word is not in the record at all (an absolute Java
// static) and its values are different (0 logo / 2 title / 1 slot / 3 FIELD / 5 window).
// Two constants here plus a per-build offset could not express that, and a SECOND copy
// of the values next to the table is the mirror-drift trap this repo keeps paying for.
// Every state test now goes through [`app_state_word`] + `AppState`.
const SELECTOR_FLAG_OFF: u32 = 0x9c6c;

/// ★ The window manager's "current window id" (int8 at `CHAR+0xa0c0`, i.e. the
/// window object `CHAR+0x9e30` + 0x290). SOLE writer = `openWindow` (guest
/// 0x12c6a4, store at 0x12ca7c); three parallel 16-entry KEY / PAINT / ENTER jump
/// tables are all indexed by `id - 3`:
///   3 = NO window (gameplay / field)   4 = script / cutscene playback
///   7, 17, 18 = confirm windows        8 = MAP window
///   14 = save window                   15, 16 = inventory / equipment grid
/// This is the discriminator the overlay needs: `id != 3` means one of the game's
/// own windows owns the screen, so the field HUD must stand down and the buttons
/// must become menu-navigation keys. [RE: workflow wf_6d270fb0 Q3]
/// ★ The offset AND the "no window" id live in `guest_anchors::FieldGate` since
/// 2026-09-09 (hero2 has neither byte — its windows are Java objects with their own
/// static singletons). This constant is kept only for the readers below that are
/// already behind `char_record_is_hero3`; the GATE reads the table.
const WINDOW_ID_OFF: u32 = 0xa0c0;
/// SCRIPT / cutscene playback: story scenes and NPC dialogue. Distinct from every
/// other window id, which is what makes "is this a cutscene or a menu?" answerable
/// at all (player question 2026-08-03) — the overlay strips the control set down to
/// OK + `#` here instead of showing menu-navigation keys that do nothing.
pub const WIN_SCRIPT: u8 = 4;
/// The game's own map window (what `*` toggles, and what paints the minimap
/// buffer we mirror). Its key handler closes on ANY key (guest 0x124360).
pub const WIN_MAP: u8 = 8;
/// Script-window CHOICE-LIST flag — see [`script_choice_open`], which is also where
/// the device trace that found it is written down.
const SCRIPT_CHOICE_OFF: u32 = 0xa0dc;

// ── Player gauges ────────────────────────────────────────────────────────────
//
// ★ CORRECTED 2026-07-30 (workflow wf_6d270fb0 Q1, CONFIRMED HIGH by an
// independent verifier + this session's own re-disassembly): the previous mapping
// had **HP and SP SWAPPED**, which is exactly the player-reported bug "the HP bar
// does not refill when I drink a potion" — the red bar was tracking SP, and the
// blue bar was tracking HP.
//
// Everything lives in a stats sub-struct `H = CHAR + 0xb4`, so the same fields
// appear in the game's code as `H + (off - 0xb4)`:
//   AddHP(H, delta)  @ guest 0x1467a8:  cur = (s16)H[0x3e] (= CHAR+0xf2); cur += delta;
//                                       clamp to (s32)H[0x38] (= CHAR+0xec) and to 0.
//   AddSP(H, delta)  @ guest 0x1467d0:  cur = (s32)H[0x44] (= CHAR+0xf8); clamp 0..1000.
//   ComputeMaxHP(H)  @ guest 0x145f4c:  H[0x38] = (36*level + 10*CHAR[0xd9])
//                                                 * (100 + (s16)CHAR[0xf0]) / 100
//                                       — the SOLE writer of max HP.
//   Level-up @ 0x146c28: level++ (cap 70), ComputeMaxHP, AddHP(+maxHP), AddSP(+1000).
//   Full restore (revive / new game) @ 0x124458 / 0x1540cc / 0x1570c4:
//                                       HP = *(CHAR+0xec); SP = 1000.
// The game's own field bars corroborate the split: HP bar (red) = len * CHAR[0xf2]
// / CHAR[0xec] (proportional), SP bar (blue) = len * (CHAR[0xf8]/10) / 100 (i.e.
// /1000, which is where the old "max HP = hardcoded 1000" reading came from — it
// is SP's max, not HP's).
const HP_OFF: u32 = 0xf2; // s16 current HP
const HP_MAX_OFF: u32 = 0xec; // s32 max HP (ComputeMaxHP writes it)
const SP_OFF: u32 = 0xf8; // s32 current SP, range 0..1000
/// Max SP really IS a hardcoded constant — the AddSP clamp at guest 0x1467d0.
const SP_MAX: u16 = 1000;
const EXP_OFF: u32 = 0xe4; // u32 current EXP (within level)
const EXP_NEXT_OFF: u32 = 0xe8; // u32 EXP-to-next
const LEVEL_OFF: u32 = 0xe2; // u16 level
/// Level cap, from the level-up routine's `cmp level, #70` (KTF guest 0x146c3a/0x146c44;
/// LGT `doLevelUp` 0x4d690 clamps at the same `#0x46`).
pub const LEVEL_CAP: u16 = 70;
// ★ The plausibility bounds moved into `guest_anchors::CharLayout` (`level_cap` /
// `hp_cap`) on 2026-09-09, because they are per-build: hero3's level cap is 70 (from
// `cmp level, #70` at KTF 0x146c3a / LGT 0x4d690) and hero2's is not measured yet.
// `LEVEL_CAP` above stays because callers outside this module use it as hero3's cap.
// The old `HP_CAP = 30000` (ComputeMaxHP at the level cap cannot plausibly reach it)
// is now `CharLayout::hp_cap` and is the same number on every build so far.

// ── unspent stat / skill points (the red + / blue + badges) ──────────────────
//
// The game's OWN bottom-HUD renderer draws these two badges at the end of its
// block (guest 0x14a2d2..0x14a3ae) — which is inside the range our HUD-skip patch
// branches over (see `hud_bottom_patch`), so they vanished when we took that block
// away. The overlay redraws them; these are the two counters it reads.
//
//   addStatPoints  (guest 0x146834): *(u16*)(CHAR+0x18a) += n, clamped to 276, >=0
//   addSkillPoints (guest 0x146868): *(u8*) (CHAR+0x18c) += n, clamped to 44
//
// Both caps corroborate the offsets arithmetically, which is why they are pinned
// here rather than left as magic numbers:
//   * level-up calls addStatPoints(4) EVERY level -> 69 level-ups * 4 = 276 ✓
//   * level-up calls addSkillPoints(1) every level to 19, then only on EVEN levels
//     (guest 0x146c88 skips when `level > 19 && (level & 1)`) -> 18 + 26 = 44 ✓
// A reading above either cap therefore means the bytes are not (yet) a character,
// so `plausible_stats` rejects it and the HUD keeps its last good copy.
const STAT_POINTS_OFF: u32 = 0x18a; // u16, read as s16 by the game (`ldrsh`)
const SKILL_POINTS_OFF: u32 = 0x18c; // u8 (`ldrb`)
// The game's own clamps — hero3 `addStatPoints` (guest 0x146834: `> 0x8a<<1` -> 276)
// and `addSkillPoints` (0x146868: `> 0x2c` -> 44) — live in `CharLayout::point_caps`
// since 2026-09-18, per build.

/// Skill icon sheet id for the active member (byte, value 4..10 = s4..s10_bm);
/// atlas sheet index = value - 4. [CONFIRMED wf_9add52f0, helper 0x19c280.]
const SKILL_SHEET_OFF: u32 = 0xd1;

// ── skill icon = (per-slot party member's sheet, strip cell) ──────────────────
// Mirrors the game's own HUD draw, guest 0x14a51a (static disasm 2026-07-28) —
// see the CHAR+0x9bf8 note below. Per skill slot `s`:
//   onfield = ((skill-ref[s] as i8) >> 6) != 0    (0 = first on-field hero, else
//             the second; the game only ever loads TWO skill sheets on the field
//             and picks between them with this 2-bit test — it does NOT index by
//             the raw member 0..3)
//   sheetId = u8 @ CHAR+0xd1 + onfield (0 -> 7 = s7, 1 -> 8 = s8, …)
//   atlas   = sheetId - 4              (s4..s10 -> 0..6)
//   cell    = int8 @ CHAR+0x26c+s      (0..8, the base-skill strip column stored
//             directly by the bind at guest 0x145cf0; <0 = empty), blitted as
//             srcX = cell*15, srcY = 0, 15x15.
// Ground truth this save: slot0 ref 0x6a idx 5 -> hero1/s8/cell5 = 일심;
// slot1 ref 0x2d idx 5 -> hero0/s7/cell5 = 장탄.
// (The earlier sk+idx*0x38+0x173 pointer-chain was the skill-PICKER draw, not the
// field HUD; it read the wrong byte and collapsed both slots to cell 0 = 사격.)
//
// ── where the game's HUD icons come from (guest 0x140448, RE 2026-07-28) ──────
// The HUD skill branch samples the image array `*(*(CHAR+0x9bf8))+8`, a handle
// distinct from the class-sheet manager at CHAR+0x9bf0 — which is why an earlier
// pass concluded "the HUD draws some other image". It does not: the setter at
// guest 0x140448 allocates a second 2-slot manager and fills it from the name
// template `*(sl+0x328)` = "/skill/s4_bm" with the digit patched to the sheet id
// `CHAR+0xd1+i`, i = 0,1. Same s{N}_bm family the atlas is baked from, so the
// (sheet, cell, 15x15) triple above is exactly what the game blits.
const QB_SKILLIDX_OFF: u32 = 0x26c; // int8 strip cell 0..8 per slot
const SKILL_STRIP_CELLS: i8 = 9;    // s{N}_bm = 9 cells (0..8)
/// Item-DB access for the consumable icon frame (CONFIRMED wf_9add52f0, icon-draw
/// 0x1533a8): itemDB = *(*(grid_base+0x38))+8; rec = itemDB + idx*0x18; frame u8
/// @rec+0x14 (raw 0-based item_bm frame). Consumables = category 0x12.
const ITEMDB_HANDLE_OFF: u32 = 0x38;
const ITEMDB_STRIDE: u32 = 0x18;
const ITEMDB_ICON_OFF: u32 = 0x14;
const ITEM_ICON_FALLBACK: u16 = 0x11; // generic item frame if DB lookup fails

// ── low-level guest reads ────────────────────────────────────────────────────

fn rd_u16(read: &impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<u16> {
    let b = read(addr, 2);
    (b.len() == 2).then(|| u16::from_le_bytes([b[0], b[1]]))
}

fn rd_u32(read: &impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<u32> {
    let b = read(addr, 4);
    (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rd_u8(read: &impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<u8> {
    let b = read(addr, 1);
    (b.len() == 1).then(|| b[0])
}

fn rd_i8(read: &impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<i8> {
    let b = read(addr, 1);
    (b.len() == 1).then(|| b[0] as i8)
}

fn in_heap(p: u32) -> bool {
    (HEAP_LO..HEAP_HI).contains(&p)
}

// ── CHAR + grid-base resolution ──────────────────────────────────────────────

/// Follow a field that holds a WIPI allocation to its payload, validating every hop.
///
/// ★ The number of hops is a CARRIER property, not a game one (2026-08-10): KTF's
/// runtime hands the game HANDLES, so the payload is `*(handle) + 8`, while LGT's
/// `data_ptr` is the identity and the field is already the payload address. Before
/// this existed, every container here double-dereferenced unconditionally, so on an
/// LGT boot the inventory grid / area object / item DB resolved to the first word of
/// the real payload and read neighbouring bytes as records — a silently wrong
/// quickbar rather than an absent one. See `guest_build::container_data_off`.
fn deref_container(read: &impl Fn(u32, usize) -> Vec<u8>, field: u32) -> Option<u32> {
    let first = rd_u32(read, field)?;
    if !in_heap(first) {
        return None;
    }
    let Some(off) = crate::guest_build::container_data_off() else {
        return Some(first);
    };
    let block = rd_u32(read, first)?;
    if !in_heap(block) {
        return None;
    }
    let base = block.checked_add(off)?;
    in_heap(base).then_some(base)
}

/// Resolve the live CHAR pointer (pointer global first, validated in-heap; else
/// the observed static fallback). `None` if neither looks sane.
pub fn resolve_char(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u32> {
    match crate::guest_build::char_anchor()? {
        crate::guest_build::CharAnchor::Pointer { global, fallback } => {
            if let Some(p) = rd_u32(read, global)
                && in_heap(p)
            {
                return Some(p);
            }
            let probe = read(fallback, 4);
            (probe.len() == 4).then_some(fallback)
        }
        // A static always reads back, so unlike the pointer build there is no address
        // question to answer — only "has a character been created yet". Before the
        // class-select screen commits one the record is still zeroed, and a zero level
        // fails this. Cheap: four reads, and it is the same plausibility test
        // `read_stats` applies to every gauge it publishes.
        crate::guest_build::CharAnchor::Static(base) => live_char(read, base).then_some(base),
        // ★★ hero2: two hops, then the record IS the field block (`ptr_fields + 4`).
        // Both hops are validated against the JAVA heap window specifically (the bucket
        // half of `HEAP_BASE`), not the wide `in_heap` range — a stale slot on this build
        // holds either 0 or an image address, and both must fail rather than resolve to
        // something that reads like a character.
        crate::guest_build::CharAnchor::JavaObject { slot } => {
            let obj = rd_u32(read, slot).filter(|p| in_java_heap(*p))?;
            let fields = rd_u32(read, obj).filter(|p| in_java_heap(*p))?;
            let base = fields.checked_add(4)?;
            live_char(read, base).then_some(base)
        }
    }
}

/// The guest's Java heap. Narrower than [`in_heap`] on purpose: [`CharAnchor::JavaObject`]
/// chases pointers that are only ever heap allocations, so accepting an image address
/// there would let a zeroed/stale static resolve to bytes that pass the liveness test by
/// luck. (`wie_core_arm::HEAP_BASE` = `0x4000_0000`, `HEAP_SIZE` = `0x1000_0000`.)
fn in_java_heap(p: u32) -> bool {
    (0x4000_0000..0x5000_0000).contains(&p)
}

/// "Has a character been created yet", read through the per-build gauge layout.
///
/// Anchors that always read back (a `.bss` static, or a resolved Java field block) have
/// no pointer question to answer — only a CONTENTS question. Before the class-select
/// screen commits a character the record is zeroed, and a zero level fails this.
/// `false` when the build has no measured layout, which keeps every gauge closed.
fn live_char(read: &impl Fn(u32, usize) -> Vec<u8>, base: u32) -> bool {
    let Some(l) = crate::guest_build::char_layout() else { return false };
    let Some(level) = rd_u16(read, base + l.level) else { return false };
    let Some(hp_max) = rd_u32(read, base + l.hp_max) else { return false };
    // Same width discipline as `read_stats` — see `CharLayout::hp_bytes`.
    let Some(hp) = (match l.hp_bytes {
        2 => rd_u16(read, base + l.hp).map(u32::from),
        _ => rd_u32(read, base + l.hp),
    }) else {
        return false;
    };
    let Some(sp) = rd_u32(read, base + l.sp) else { return false };
    let sp_max = match l.sp_max {
        Some(off) => match rd_u32(read, base + off) {
            Some(v) => v,
            None => return false,
        },
        None => u32::from(l.sp_max_const),
    };
    (1..=l.level_cap).contains(&level)
        && (1..=l.hp_cap).contains(&hp_max)
        && hp <= hp_max
        && sp <= sp_max
}

/// The CHAR record's address WITHOUT the liveness gate — for fields that are valid
/// before a character exists (the app-state word the shell test reads). Pointer
/// builds still have to chase/validate the pointer; a static anchor simply IS the
/// address. Everything that reads character data must keep using [`resolve_char`].
fn char_base_raw(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u32> {
    match crate::guest_build::char_anchor()? {
        crate::guest_build::CharAnchor::Pointer { global, fallback } => {
            if let Some(p) = rd_u32(read, global)
                && in_heap(p)
            {
                return Some(p);
            }
            let probe = read(fallback, 4);
            (probe.len() == 4).then_some(fallback)
        }
        crate::guest_build::CharAnchor::Static(base) => Some(base),
        // Same two hops as `resolve_char`, minus the liveness test — the point of this
        // function. On hero2 the state word is an absolute static anyway, so the one
        // caller that used to need this (`shell_state`) no longer goes through it here.
        crate::guest_build::CharAnchor::JavaObject { slot } => {
            let obj = rd_u32(read, slot).filter(|p| in_java_heap(*p))?;
            let fields = rd_u32(read, obj).filter(|p| in_java_heap(*p))?;
            fields.checked_add(4)
        }
    }
}

/// Resolve the inventory grid base — `*(*(CHAR+0x9e28)) + 8` on KTF, one hop fewer
/// on LGT (see [`deref_container`]).
fn resolve_grid_base(read: &impl Fn(u32, usize) -> Vec<u8>, char_ptr: u32) -> Option<u32> {
    // ⛔ hero3-measured offsets — see `guest_build::char_record_is_hero3`.
    if !crate::guest_build::char_record_is_hero3() {
        return None;
    }
    deref_container(read, char_ptr + GRID_HANDLE_OFF)
}

/// Resolve the current AREA object at `CHAR+0x9c90`. Same container shape as the
/// inventory grid — see [`crate::save::AREA_HANDLE_OFF`] for why the guest's own
/// cached copy must not be used instead.
fn resolve_area_base(read: &impl Fn(u32, usize) -> Vec<u8>, char_ptr: u32) -> Option<u32> {
    // ⛔ hero3-measured offsets — see `guest_build::char_record_is_hero3`.
    if !crate::guest_build::char_record_is_hero3() {
        return None;
    }
    deref_container(read, char_ptr + crate::save::AREA_HANDLE_OFF)
}

/// The game's own "may I save in this area?" byte, or `None` while it cannot be read
/// (boot, teardown, the moment of a map change).
///
/// `None` is deliberately distinct from `Some(false)`: an unreadable chain is "ask me
/// again next frame", while `Some(false)` is the game saying no. Callers that must
/// answer anyway fail OPEN — the alternative is what 27차 shipped, where one
/// unresolvable read turned into a dead autosave and two vanished controls.
pub fn area_can_save(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<bool> {
    let c = resolve_char(read)?;
    let base = resolve_area_base(read, c)?;
    Some(rd_i8(read, base + crate::save::AREA_CAN_SAVE_OFF)? != 0)
}

/// (count, cat, idx) of the inventory item at (tab, slot); zeros if unresolved.
fn item_record(read: &impl Fn(u32, usize) -> Vec<u8>, char_ptr: u32, tab: u8, slot: u8) -> (u16, u16, u16) {
    let Some(base) = resolve_grid_base(read, char_ptr) else {
        return (0, 0, 0);
    };
    let rec = base + u32::from(tab) * TAB_STRIDE + u32::from(slot) * SLOT_STRIDE;
    let count = rd_u16(read, rec + COUNT_OFF).unwrap_or(0);
    let cat = rd_u16(read, rec + 0x3b6).unwrap_or(0);
    let idx = rd_u16(read, rec + 0x3b8).unwrap_or(0);
    (count, cat, idx)
}

/// item (cat, idx) -> IconRef::Item(frame). Consumables (cat 0x12) read the icon
/// frame live from the item-DB, exactly as the game (0x1533a8) does; else default.
fn item_icon(read: &impl Fn(u32, usize) -> Vec<u8>, char_ptr: u32, cat: u16, idx: u16) -> IconRef {
    // Same rule the game uses (see `icon_frame`); the generic frame is only a
    // last resort for a record we could not resolve at all.
    IconRef::Item(icon_frame(read, char_ptr, cat, idx).unwrap_or(ITEM_ICON_FALLBACK))
}

/// Live item-DB icon-frame byte for consumable index `idx`, or None.
fn item_db_icon(read: &impl Fn(u32, usize) -> Vec<u8>, char_ptr: u32, idx: u16) -> Option<u16> {
    let base = resolve_grid_base(read, char_ptr)?;
    let db = deref_container(read, base + ITEMDB_HANDLE_OFF)?;
    let rec = db + u32::from(idx) * ITEMDB_STRIDE;
    rd_u8(read, rec + ITEMDB_ICON_OFF).map(u16::from)
}

/// The (atlas sheet 0..6, cell 0..8) a field quickbar SKILL slot draws — the
/// on-field hero is chosen by the skill-ref's top 2 bits being nonzero, the cell
/// is the int8 at CHAR+0x26c+slot (see the module note; mirrors guest 0x14a51a).
/// Returns `None` if the cell is empty (int8 < 0) or the sheet id is out of
/// range 4..10.
fn skill_icon(read: &impl Fn(u32, usize) -> Vec<u8>, char_ptr: u32, slot: u32) -> Option<IconRef> {
    let cell = rd_i8(read, char_ptr + QB_SKILLIDX_OFF + slot)?;
    if cell < 0 || cell >= SKILL_STRIP_CELLS {
        return None;
    }
    // `(ref as i8) >> 6 != 0 ? 1 : 0` — the game's own 2-way on-field hero pick.
    let onfield = u32::from((rd_i8(read, char_ptr + QB_SKILLREF_OFF + slot)? >> 6) != 0);
    let sheet_id = rd_u8(read, char_ptr + SKILL_SHEET_OFF + onfield)?;
    let atlas = sheet_id.checked_sub(4).filter(|&s| s <= 6)?;
    Some(IconRef::Skill(atlas, cell as u8))
}

/// Read the player gauges for the HUD. `None` when the reading does not look like
/// a live character (boot/teardown, a moved pointer, a partially written struct):
/// the caller then KEEPS its previous copy, because a gauge that flashes to empty
/// reads as "you are about to die" to the player.
pub fn read_stats(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<Stats> {
    let c = resolve_char(read)?;
    let l = crate::guest_build::char_layout()?;
    // Read HP as a full word and sign-extend when the build says so: hero3's death gates
    // (guest 0x12426c / 0x124568) test `HP <= 0`, so a corpse carries a negative — show
    // that as an empty bar, not as 65535 (which would fail the plausibility gate below
    // and freeze the gauge). hero2 stores HP in an `int`, which is why the read is a word
    // on both and only the SIGN handling is per-build.
    let hp = match l.hp_bytes {
        2 => {
            let raw = rd_u16(read, c + l.hp)?;
            u32::from(if l.hp_signed { (raw as i16).max(0) as u16 } else { raw })
        }
        _ => {
            let raw = rd_u32(read, c + l.hp)?;
            if l.hp_signed && (raw as i32) < 0 { 0 } else { raw }
        }
    };
    let sp_max = match l.sp_max {
        Some(off) => rd_u32(read, c + off)?.min(u32::from(u16::MAX)) as u16,
        None => l.sp_max_const,
    };
    let (stat_points, skill_points) = match l.points {
        // The game reads the stat count SIGNED (`ldrsh` at 0x14a2de) and only draws the
        // badge when it is > 0, so a negative value means "none" — not 65000-odd.
        Some((sp_off, kp_off)) => (
            (rd_u16(read, c + sp_off)? as i16).max(0) as u16,
            // hero3 keeps a byte here, hero2 a `short` (see `CharLayout::skill_points_bytes`).
            match l.skill_points_bytes {
                2 => (rd_u16(read, c + kp_off)? as i16).max(0) as u16,
                _ => u16::from(rd_u8(read, c + kp_off)?),
            },
        ),
        // Not located on this build: no badge can be drawn, and 0 is exactly what
        // `draw_point_badges` treats as "nothing to show".
        None => (0, 0),
    };
    let s = Stats {
        hp,
        // ★ 2026-10-22: the whole 32-bit field (a GL 체력 build passes a u16; `CharLayout::hp_cap` bounds it).
        hp_max: rd_u32(read, c + l.hp_max)?,
        sp: rd_u32(read, c + l.sp)?.min(u32::from(sp_max)) as u16,
        sp_max,
        exp: rd_u32(read, c + l.exp)?,
        exp_next: rd_u32(read, c + l.exp_next)?,
        level: rd_u16(read, c + l.level)?,
        stat_points,
        skill_points,
        // ★ 2026-09-29: the status HUD's portrait — the helmet's head sprite (hero2 only).
        head: crate::guest_build::hero2_java().and_then(|h2| crate::guest_build::h2_head_sprite(read, &h2)),
        // ★ 2026-10-22: the worker lays the GL block over this (`emulator::GuardianLevel::overlay_stats`).
        guardian: None,
    };
    plausible_stats(&s).then_some(s)
}

/// Sanity gate for [`read_stats`]: a live character has a level in 1..=LEVEL_CAP
/// and a max HP its current HP cannot exceed. Anything else means the bytes are
/// not (yet) a character.
fn plausible_stats(s: &Stats) -> bool {
    let Some(l) = crate::guest_build::char_layout() else { return false };
    (1..=l.level_cap).contains(&s.level)
        && (1..=l.hp_cap).contains(&s.hp_max)
        && s.hp <= s.hp_max
        && s.sp <= s.sp_max
        // The game's own clamps bound these two, so exceeding them is proof the
        // bytes are not a character yet (boot / teardown / a moved pointer).
        && s.stat_points <= l.point_caps.0
        && s.skill_points <= l.point_caps.1
}

// ── quickbar enumeration (read-only, safe) ───────────────────────────────────

/// Read all 5 field-quickbar slots. Empty slots are included (so the HUD can
/// show placeholders); skill/item slots carry their kind + (for items) count.
pub fn scan_quickbar(read: &impl Fn(u32, usize) -> Vec<u8>) -> Vec<QuickSlot> {
    let Some(char_ptr) = resolve_char(read) else {
        return Vec::new();
    };
    // ★★ CONTENTS vs BUTTONS (2026-09-09). The four parallel arrays at `+0x252`/`+0x257`/
    // `+0x25c`/`+0x26c` are hero3's. hero2's quickbar array is NOT confirmed yet — the
    // length-4 candidates found so far read half-empty while the game's own HUD shows
    // four filled cells, so it is one experiment short (`HERO2_GUEST_RE.md` §6 #1).
    //
    // So on hero2 the bar is published as `QUICK_SLOTS` EMPTY plates rather than not
    // published at all, and that is the honest, useful answer: the plates still draw,
    // still take a tap, and still send the game's own `1 / 3 / 7 / 9`
    // (`crate::quickslot_hotkey`) — which is the whole of "스킬 슬롯을 퀵슬롯으로".
    // What is missing is only the ICON and the stack COUNT, and an empty slot already
    // renders as its hotkey character. Decoding hero2's record at hero3's offsets would
    // instead invent an item kind and a count per plate, which looks like data.
    // ★★ 2026-09-17: hero2's quickbar IS confirmed now — `ag.j:[B`, a static `byte[4]`
    // (HERO2_QUICKSLOT_TOUCH_PLAN.md §3.1) — so hero2 decodes its own bar here.
    if let Some(h2) = crate::guest_build::hero2_java() {
        return h2_scan_quickbar(read, &h2, char_ptr);
    }
    if !crate::guest_build::char_record_is_hero3() {
        return (0..u32::from(crate::QUICK_SLOTS))
            .map(|s| QuickSlot {
                slot: s as u8,
                kind: SlotKind::Empty,
                count: 0,
                icon: None,
                cooldown: None,
                guest: None,
            })
            .collect();
    }
    let mut out = Vec::with_capacity(QB_SLOTS as usize);
    for s in 0..QB_SLOTS {
        let ty = rd_u8(read, char_ptr + QB_TYPE_OFF + s).unwrap_or(0);
        let (kind, count, icon) = match ty {
            TY_SKILL => (SlotKind::Skill, 0, skill_icon(read, char_ptr, s)),
            TY_ITEM => {
                let r = rd_u8(read, char_ptr + QB_ITEMREF_OFF + s).unwrap_or(0);
                let (tab, slot) = (r >> 5, r & 0x1f);
                let (count, cat, idx) = item_record(read, char_ptr, tab, slot);
                (SlotKind::Item, count, Some(item_icon(read, char_ptr, cat, idx)))
            }
            _ => (SlotKind::Empty, 0, None),
        };
        out.push(QuickSlot {
            slot: s as u8,
            kind,
            count,
            icon,
            cooldown: None,
            guest: Some(s as u8),
        });
    }
    // ★ The HOST plates (2026-09-01). The bar is `QUICK_SLOTS` wide and the guest array
    // is `QB_SLOTS`, so the tail is reported here as ordinary empty slots. Doing it in
    // the SCANNER and not in each consumer is the point: rect building, drawing,
    // hit-testing and the page store then all see ONE bar of one width, and none of
    // them has to know the tail is special. What the tail cannot do is asked in exactly
    // one place — `crate::is_host_slot`.
    for s in QB_SLOTS..u32::from(crate::QUICK_SLOTS) {
        out.push(QuickSlot { slot: s as u8, kind: SlotKind::Empty, count: 0, icon: None, cooldown: None, guest: None });
    }
    out
}

// ── hero2: the quickbar as HUD slots (HERO2_QUICKSLOT_TOUCH_PLAN.md §3, 2026-09-17) ──
//
// The pointer walks live in `guest_build` (`hero2java.rs`, dependency-free, shared with
// `wie_probe`); this is the crate-typed layer on top: `QuickSlot`s, `IconRef`s, and the
// plate store's view of the bar.

fn h2_icon_ref(i: crate::guest_build::H2Icon) -> IconRef {
    match i {
        crate::guest_build::H2Icon::Skill { class, cell } => IconRef::Skill(class, cell),
        crate::guest_build::H2Icon::Beast { id } => IconRef::Beast(crate::beast_icon_id(id)), // ★★ 2026-10-16: the 악의 version while it runs
    }
}

/// hero2's four GUEST slots as HUD slots (plates `4..QUICK_SLOTS` padded empty).
///
/// Cooldown: the game keeps `ag.a[slot]` = ticks left (`-1` ready). The TOTAL is the
/// class table entry for a character skill; a beast skill's total is not read (its table
/// hangs off the beast), so the largest value seen while the counter runs stands in.
fn h2_scan_quickbar(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &crate::guest_build::Hero2Java, char_base: u32) -> Vec<QuickSlot> {
    use crate::guest_build::{Hero2Java, h2_beast_skill_ids, h2_cooldown_table, h2_cooldowns, h2_quickbar_bytes, h2_slot_icon};
    let empty = |s: u8| QuickSlot { slot: s, kind: SlotKind::Empty, count: 0, icon: None, cooldown: None, guest: None };
    let Some(bar) = h2_quickbar_bytes(read, h2) else {
        return (0..crate::QUICK_SLOTS).map(empty).collect();
    };
    let class = rd_u8(read, h2.class_slot).filter(|c| *c < 3).unwrap_or(0);
    let beast_ids = h2_beast_skill_ids(read, h2, char_base);
    let cds = h2_cooldowns(read, h2);
    let totals = h2_cooldown_table(read, h2);
    let mut peaks = H2_CD_PEAK.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::with_capacity(crate::QUICK_SLOTS as usize);
    for (g, v) in bar.iter().copied().enumerate() {
        let icon = h2_slot_icon(v, class, beast_ids.as_deref()).map(h2_icon_ref);
        let left = cds.map_or(-1, |c| c[g]);
        let cooldown = if left > 0 && v > 0 {
            let total = match h2_class_cd_total(totals.as_deref(), class, v as u8) {
                Some(t) => t,
                None => {
                    peaks[g] = peaks[g].max(left);
                    peaks[g]
                }
            };
            Some((left.min(i32::from(u16::MAX)) as u16, total.clamp(left, i32::from(u16::MAX)) as u16))
        } else {
            peaks[g] = 0;
            None
        };
        out.push(QuickSlot {
            slot: g as u8,
            kind: if v < 0 { SlotKind::Empty } else { SlotKind::Skill },
            count: 0,
            icon,
            cooldown,
            guest: Some(g as u8),
        });
    }
    for s in Hero2Java::QUICKBAR_LEN as u8..crate::QUICK_SLOTS {
        out.push(empty(s));
    }
    out
}

/// The full cooldown of the class's skill `v` (a quickbar value `1..=7`) as the game starts
/// its counter — `None` for a beast skill (`v >= 10`), whose total no table names.
fn h2_class_cd_total(totals: Option<&[i16]>, class: u8, v: u8) -> Option<i32> {
    // ★★ 2026-10-14 the gear block's per-index cut (번개의 호흡 + 벽력일섬 = the rogue's 일섬 −20 % · 시간의 외투 every index −15 %) —
    // its `<job>.a(I)S` wrappers are the outermost (after every class block's answer, the rogue's too), so every total takes it.
    let cut = crate::gear_cd_cut(class, v.wrapping_sub(1));
    h2_class_cd_total_uncut(totals, class, v).map(|t| crate::guest_build::GearPatch::cut(t, cut)).filter(|t| *t > 0)
}

fn h2_class_cd_total_uncut(totals: Option<&[i16]>, class: u8, v: u8) -> Option<i32> {
    if v == 0 || v >= crate::guest_build::Hero2Java::BEAST_BASE {
        return None;
    }
    // ★ #17: with `skill_cooldown_half` in, the game starts the counter from
    // `table >> 1` (its `a(I)S` now shifts by 17) — the same division here, or
    // the ring would open every cooldown half-empty.
    let halved = crate::skill_cd_halved();
    // ★ 2026-10-03 발도: the knight's seventh active has no table entry — the combat block's
    // `bx.a(I)S` wrapper answers IAIDO_COOLDOWN frames for its index, and so does the ring.
    type C = crate::guest_build::CombatPatch;
    if class == 1 && v == C::IAIDO_CD_INDEX + 1 {
        return Some(i32::from(C::IAIDO_COOLDOWN));
    }
    // ★ 2026-10-07 (2) 각성기: 천공의검 (the knight's value 8, 40 s) · 개화 (the rogue's 8, 40 s) · 격분 (the berserker's 9 —
    // 30 s, started by the explosion; the cast itself starts nothing) — each class block's `a(I)S` wrapper answers them.
    if class == 1 && v == C::SKY_CD_INDEX + 1 {
        return Some(i32::from(C::SKY_COOLDOWN));
    }
    // ★ 2026-10-05 일풍격: the rogue's seventh active, the same way — `ay.a(I)S`'s wrapper answers its index.
    type I = crate::guest_build::IlpungPatch;
    if class == I::CLASS && v == I::COOLDOWN_INDEX + 1 {
        return Some(i32::from(I::COOLDOWN));
    }
    // ★ 2026-10-06 살상연희: 번개난타's slot (value 6) — the rogue block's `ay.a(I)S` wrapper answers its 10 s, and so does
    // the ring.
    type R = crate::guest_build::RoguePatch;
    if class == R::CLASS && v == R::COOLDOWN_INDEX + 1 {
        return Some(i32::from(R::COOLDOWN));
    }
    if class == R::CLASS && v == R::BLOOM_CD_INDEX + 1 {
        return Some(i32::from(R::BLOOM_COOLDOWN));
    }
    // ★ 2026-10-07: the berserker's 분노태세 (value 4 — a toggle, 1 s) and 도발 (value 8, 8 s) — the berserker block's
    // `bl.a(I)S` wrapper answers both indices (the static table still says 2400 for 분노태세 and has no eighth entry).
    type B = crate::guest_build::BerserkerPatch;
    if class == B::CLASS && v == B::RAGE_CD_INDEX + 1 && crate::berserker_on() {
        return Some(i32::from(B::RAGE_COOLDOWN));
    }
    if class == B::CLASS && v == B::TAUNT_CD_INDEX + 1 {
        return Some(i32::from(B::TAUNT_COOLDOWN));
    }
    if class == B::CLASS && v == B::FURY_CD_INDEX + 1 {
        return Some(i32::from(B::FURY_COOLDOWN));
    }
    // ★ 2026-10-02 광룡의 외침: the block halves 사자후's (the berserker's index 6)
    // once more after the game's own `a(I)S` — so does the ring.
    type P = crate::guest_build::AccessoryPatch;
    let roar = class == 0 && v - 1 == P::ROAR_COOLDOWN_INDEX && crate::accessory() == P::GWANGRYONG;
    totals
        .and_then(|t| t.get(usize::from(v - 1)).copied())
        .map(|t| if halved { i32::from(t) >> 1 } else { i32::from(t) })
        .map(|t| if roar { t >> 1 } else { t })
        .filter(|t| *t > 0)
}

/// Largest cooldown value seen per guest slot while its counter runs — the stand-in
/// total for beast skills. See [`h2_scan_quickbar`].
static H2_CD_PEAK: std::sync::Mutex<[i32; 4]> = std::sync::Mutex::new([0; 4]);

/// The peak guest slot `g`'s running counter was seen at — what a skill leaving that slot
/// takes along as its total (`quickpage::Window::park`).
pub fn h2_cd_peak(g: u8) -> i32 {
    H2_CD_PEAK.lock().unwrap_or_else(|e| e.into_inner()).get(usize::from(g)).copied().unwrap_or(0)
}

/// The host just wrote guest slot `g`'s counter (a seating): what the peak saw there belonged
/// to the skill that left, and the one arriving brings its own `total`.
pub fn h2_cd_peak_seated(g: u8, total: i32) {
    if let Some(p) = H2_CD_PEAK.lock().unwrap_or_else(|e| e.into_inner()).get_mut(usize::from(g)) {
        *p = total.max(0);
    }
}

/// hero2: the PLATES as the HUD draws them — the store's entries with the guest's own
/// cooldown on the seated ones, and the PARKED one (`quickpage::Window::parked`, aged to the
/// cooldown clock `now`) on a skill that is off the bar. `slot` is the plate index.
pub fn h2_plate_slots(
    read: &impl Fn(u32, usize) -> Vec<u8>,
    h2: &crate::guest_build::Hero2Java,
    pages: &crate::quickpage::Pages,
    window: &crate::quickpage::Window,
    now: u32,
) -> Vec<QuickSlot> {
    let Some(char_base) = resolve_char(read) else { return Vec::new() };
    let guest = h2_scan_quickbar(read, h2, char_base);
    let class = rd_u8(read, h2.class_slot).filter(|c| *c < 3).unwrap_or(0);
    let totals = crate::guest_build::h2_cooldown_table(read, h2);
    let beast_ids = crate::guest_build::h2_beast_skill_ids(read, h2, char_base);
    // S7: item plates show the bag's live stock of their type (walked once per poll,
    // only when some plate is an item — the bag is ~20 records).
    let inv = if pages.plates().iter().any(|e| e.item().is_some()) { crate::guest_build::h2_inventory(read, h2) } else { None };
    (0..crate::QUICK_SLOTS)
        .map(|p| {
            let entry = pages.plate(p);
            if let Some(kind) = entry.item() {
                let count = inv.as_ref().and_then(|i| crate::guest_build::h2_item_stock(i, kind)).map_or(0, |(_, n)| n);
                return QuickSlot {
                    slot: p,
                    kind: SlotKind::Item,
                    count: count.clamp(0, i32::from(u16::MAX)) as u16,
                    icon: Some(IconRef::ItemType(kind)),
                    cooldown: None,
                    guest: None,
                };
            }
            let icon = entry.value().and_then(|v| crate::guest_build::h2_slot_icon(v as i8, class, beast_ids.as_deref())).map(h2_icon_ref);
            // ★ 2026-10-02: a skill off the bar still cools — the ring (and the UI's tap gate,
            //   which swallows a press on a running cooldown) reads the parked counter.
            let cooldown = match window.guest_of(p) {
                Some(g) => guest.get(usize::from(g)).and_then(|s| s.cooldown),
                None => entry.value().and_then(|v| window.parked(v, now).map(|(left, total)| (v, left, total))).map(|(v, left, total)| {
                    let total = h2_class_cd_total(totals.as_deref(), class, v).unwrap_or(total);
                    (left.min(i32::from(u16::MAX)) as u16, total.clamp(left, i32::from(u16::MAX)) as u16)
                }),
            };
            QuickSlot {
                slot: p,
                kind: if entry.is_empty() { SlotKind::Empty } else { SlotKind::Skill },
                count: 0,
                icon,
                cooldown,
                guest: window.guest_of(p),
            }
        })
        .collect()
}

/// hero2: who the plate rows belong to — `(save slot, class)`. `ag.c:B` is the slot
/// list's cursor at load time (SKT `u.c`, the first byte `b.a()` serialises), `ag.a:B`
/// the class. `None` until a character is live.
pub fn h2_owner(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &crate::guest_build::Hero2Java) -> Option<crate::quickpage::Owner> {
    resolve_char(read)?;
    // ★ 2026-09-27: five slots (`SaveSlotsPatch::SLOTS`) — the plates of slots 4 and 5
    // are kept like everyone else's.
    let slot = rd_u8(read, h2.save_slot_slot).filter(|s| *s < crate::guest_build::SaveSlotsPatch::SLOTS)?;
    let class = rd_u8(read, h2.class_slot).filter(|c| *c < 3)?;
    Some(crate::quickpage::Owner { slot, class })
}

// ── (hero3's quickbar PAGES readers/writers — `class_of` / `item_at` / `quickbar_identities`
//    / `slot_writes` / `skill_slot_writes` — were REMOVED on 2026-09-17 with the hero2 plate
//    window: they indexed hero3's four parallel CHAR arrays and the hero3 `quickpage`
//    identity model, neither of which this app boots. hero2's bar is `ag.j` — see the
//    `h2_*` readers above. hero3NativeAPP still has them.)

/// The window id currently owning the screen (see `FieldGate::window_none` / [`WIN_MAP`]).
/// `None` when CHAR cannot be resolved (boot / teardown).
pub fn window_id(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u8> {
    // ★ Gated on the build actually HAVING this byte (2026-09-09). hero2 does not — its
    // windows are Java objects, each with its own static singleton slot
    // (`HERO2_GUEST_RE.md` §5.2), and `CHAR+0xa0c0` there is an unrelated field. Every
    // caller of this already treats `None` as "no window claim", so the map overlay and
    // the cutscene policy stay off on hero2 rather than firing on a mis-read byte.
    let g = crate::guest_build::field_gate()?;
    let char_ptr = resolve_char(read)?;
    rd_u8(read, char_ptr + g.window_id_off)
}

/// Whether the game is on its out-of-world SHELL — the title screen, the slot list,
/// the class picker. Those screens are laid out for the original art width; on a wider
/// framebuffer the game fills the extra width by mirroring its background art, which
/// reads as "the title is cut off at both sides" (player-reported 2026-07-30). The
/// overlay letterboxes it instead.
///
/// ★ Must NOT resolve CHAR through [`resolve_char`]: the shell screens are exactly
/// where a live character does not exist yet, and a content-gated anchor refuses there
/// — which is why the letterbox never drew (MIGRATION_LGT.md §2.18-(3)). The state word
/// is valid before the record is.
///
/// ⛔ **WHICH VALUES mean "shell" is per build, and this used to claim otherwise.** The
/// old comment here said hero2 carried "the same values as KTF, so the `== 3` test
/// itself ports unchanged". It does not: hero2's shell is **1 (slot list) or 2 (title)**
/// and its `3` is the FIELD. A `== 3` test would have reported free-roam as the title
/// screen and letterboxed live gameplay. The values live in
/// `guest_anchors::AppState::shell` now — measured, per build (`HERO2_GUEST_RE.md` §3).
pub fn shell_state(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    let Some(st) = crate::guest_build::app_state() else { return false };
    if !app_state_word(read).is_some_and(|v| st.is_shell(v)) {
        return false;
    }
    // ★ hero2 (2026-09-18): state `1` is ALSO what the game shows between two maps of a
    //   scripted scene (a black fade, measured headlessly in the intro at three
    //   transitions) — not only the slot screen. The title singleton `t.a` tells the two
    //   apart: it is non-null from the title on and null once a game is running, so a
    //   `1` with no title object is a transition, and calling it "the shell" would swap
    //   the source window to the whole frame and clear the plate seats for one frame.
    match crate::guest_build::hero2_java() {
        Some(h2) => rd_u32(read, h2.title_slot).is_some_and(|p| p != 0),
        None => true,
    }
}

/// The coarse screen-state word for the build that booted, whatever shape it takes.
///
/// ★★ 2026-09-09. This used to be `char_base_raw(read) + state_off()` everywhere,
/// which silently assumed the word lives INSIDE the character record. hero2 keeps it in
/// a Java `static` at a fixed guest address instead — readable during boot, before any
/// character exists, which is precisely when the shell test needs it
/// (`HERO2_GUEST_RE.md` §3). Both shapes are now spelled out in
/// [`guest_anchors::AppState`] and every state test goes through here.
pub fn app_state_word(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u32> {
    match crate::guest_build::app_state()? {
        crate::guest_build::AppState::Absolute { addr, .. } => rd_u32(read, addr),
        crate::guest_build::AppState::CharRelative { off, .. } => {
            let c = char_base_raw(read)?;
            rd_u32(read, c + off)
        }
    }
}

/// Whether the game's own MAP window is the one on screen (window id 8). The
/// overlay then paints a big centred copy of the mirrored map over it — the game
/// draws its map 1:1, which is tiny on a wide framebuffer.
pub fn map_window_open(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    window_id(read) == Some(WIN_MAP)
}

/// Whether the real-time field owns the screen: no window open, app state ==
/// field, and the quick-item selector overlay closed — the context where the
/// quickbar hotkeys are valid and where our HUD (gauges, skill plates, minimap)
/// belongs. Everything else is "a game window is up": the overlay then hides the
/// HUD and offers menu-navigation keys instead.
pub fn in_field_state(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    let Some(st) = crate::guest_build::app_state() else { return false };
    // A live character is still required: this is the context our HUD gauges draw in,
    // and the gauges have nothing to say without one.
    let Some(char_ptr) = resolve_char(read) else {
        return false;
    };
    if app_state_word(read) != Some(st.field()) {
        return false;
    }
    match crate::guest_build::field_gate() {
        // hero3: its state word's `4` covers both free-roam and a window, so the window
        // id and the selector flag are what actually separate them.
        Some(g) => {
            let window = rd_u8(read, char_ptr + g.window_id_off);
            let selector = rd_u8(read, char_ptr + g.selector_off);
            matches!((window, selector), (Some(w), Some(0)) if w == g.window_none)
        }
        // ★ hero2: the state word already said it — `3` IS free-roam and a window makes
        // it `5`. Nothing to add, so this is not a fail-open: it is the whole test.
        None => true,
    }
}

/// Whether the SCRIPT window (id 4 — story scene / NPC dialogue) owns the screen.
/// See [`WIN_SCRIPT`].
pub fn cutscene_open(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    // ★ hero2 (2026-09-18): the state word says it — `4` is the script system's screen
    //   (story scene, NPC dialogue, quest notice), measured headlessly across the whole
    //   intro (`HERO2_GUEST_RE.md` §3.3); `12` is the ending. hero3 keeps reading its window id.
    if let Some(st) = crate::guest_build::app_state()
        && let Some(script) = app_state_word(read).and_then(|v| st.is_script(v))
    {
        return script;
    }
    window_id(read) == Some(WIN_SCRIPT)
}

/// The script window's CHOICE-LIST flag — 1 while the 예/아니오 (or n-row) selection
/// box is up inside a dialogue, 0 for plain narration and plain dialogue text.
///
/// ★ THE byte that splits the two things window id 4 conflates (2026-08-17). The
/// player asked for exactly that split — "컷신일때는 안나오고 인게임 내에서 대화에
/// 선택지가 있을때는 나오고" — and the window id cannot give it: `cutscene_open`'s own
/// doc says id 4 is "story scene / NPC dialogue", and a device trace confirmed the
/// quest guide's dialogue reads `win=4` exactly like the prologue narration.
///
/// ── HOW IT WAS FOUND (device, 2026-08-17, LGT vc92) ─────────────────────────────
/// `items::flag_window_changes` (the 43 KB whole-record watcher) across three
/// transitions in the 안내원 quest dialogue:
///   * dialogue OPENS (text only): `0xa0c0 3->4` **and `0xa0dc 1->0`** — entering the
///     script window CLEARS it, which is what makes it safe to read while id == 4.
///   * choice box APPEARS:         **`0xa0dc 0->1`**, with the row table at
///     `0xa16c..0xa187` filled in the same sample (4 rows x 4 bytes, pointer-shaped).
///   * choice box CLOSES:          `0xa0c0 4->3` and `0xa148..0xa16f` zeroed.
/// The cursor index is a separate byte, `CHAR+0xa284` (3 = the 4th row highlighted;
/// UP moved it 3->2, DOWN 2->3) — that one is NOT the gate: it legitimately reaches 0
/// when the player picks the first row.
///
/// ★ Measured on LGT only. It is read for a DRAWING decision (do the arrows appear),
/// so a wrong value on another build costs a stray control, not corruption.
pub fn script_choice_open(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    // ★ hero2 (2026-09-18): the script box's MODE byte, `3` = a 예/아니오 choice. The
    //   only mode whose key handler answers LEFT/RIGHT — which is exactly the question
    //   this predicate exists to answer (the arrows come up for a choice and nothing
    //   else). See `Hero2Java::script_mode_slot`.
    if let Some(h2) = crate::guest_build::hero2_java() {
        return app_state_word(read) == Some(4) && rd_u8(read, h2.script_mode_slot) == Some(3);
    }
    if window_id(read) != Some(WIN_SCRIPT) {
        return false;
    }
    let Some(c) = resolve_char(read) else { return false };
    // Reached only when `window_id` answered, which needs `field_gate` — so this offset
    // is never applied to a build it was not measured on.
    rd_u8(read, c + SCRIPT_CHOICE_OFF) == Some(1)
}

/// Whether the game is drawing a bottom-anchored overlay right now — hero2's NPC
/// dialogue box (state `4`), which lives in the last ~38 rows of the framebuffer and
/// was cut in half by the `hud_reserved_rows` crop on the first device run
/// (2026-09-17). `false` on every build that reserves nothing.
pub fn bottom_overlay_open(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    let Some(st) = crate::guest_build::app_state() else { return false };
    if !app_state_word(read).is_some_and(|v| st.bottom_overlay(v)) {
        return false;
    }
    // ★ 2026-09-18: the state word is necessary, not sufficient. State 4 begins BEFORE
    //   the box is painted (the script pans the camera first) and outlives it, and with
    //   the game's HUD painter switched off (`hud_bottom_patch`) nothing repaints the
    //   reserved rows — so scrolling on the state alone shows either the HUD band or
    //   the LAST box as stale ink (both device-reported the same day). The script box's
    //   own MODE byte says whether a bottom box is drawn right now: `1` narration ·
    //   `2` line waiting for OK · `3` choice (`bb.a(Graphics)`'s switch — modes 4/5
    //   are the centred boxes and `0` is nothing). `HERO2_GUEST_RE.md` §3.3.
    match crate::guest_build::hero2_java() {
        Some(h2) => matches!(rd_u8(read, h2.script_mode_slot), Some(1..=3)),
        None => true,
    }
}

/// Whether a CHARACTER is in the world at all — app state 4, regardless of which
/// window is on top. The quickbar exists (and its icons are meaningful) for the
/// whole of that, not only during field free-roam, which is what lets the overlay
/// keep the skill/potion plates on screen with a game window open (player request
/// 2026-08-03). Boot (2) and the title/slot shell (3) are excluded: CHAR is not a
/// live character there and the slot bytes are whatever was left in the heap.
pub fn in_game_state(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    let Some(st) = crate::guest_build::app_state() else { return false };
    if resolve_char(read).is_none() {
        return false;
    }
    app_state_word(read).is_some_and(|v| st.in_game(v))
}

// ── the game's own context popup -> "퀵바등록" ────────────────────────────────
//
// The player asked for the game's five separate "1키등록 / 3키등록 / 9키등록 /
// 0키등록 / #키등록" rows to become ONE "퀵바등록" row, which then highlights our
// on-screen plates so they can pick the slot by tapping it.
//
// ── HOW, AND WHY THIS WAY ────────────────────────────────────────────────────
// The host REWRITES THE GUEST'S OWN LABEL BUFFERS in RAM and lets the guest draw
// its own list. Three designs were scoped and adjudicated (workflow wf_95a21c44,
// 5 RE questions x independent adjudicator + synthesis); the two obvious ones lose:
//
//   * DRAW OVER THE POPUP — impossible today. `draw_popup` (guest 0x12e450) takes
//     x/y as arguments and they never leave its stack frame, so the popup's screen
//     anchor is not readable. We cannot cover something we cannot locate.
//   * PATCH THE ROW TABLE — fatal on the skill side. `skill_bind`'s second caller
//     (guest 0x154462) hardcodes `movs r2,#5` in an unregister-all sweep, so
//     repointing the `slot==5` unbind sentinel at 0x145ca6 would make a routine
//     player action write one byte past EVERY 5-entry quickbar array. Silent save
//     corruption.
//
// What makes the RAM rewrite work: `draw_popup` recomputes only the label CACHE
// POINTER each frame — `text[i] = *handle[i] + 8` at guest 0x12e592..0x12e598 — and
// never rewrites the payload. So bytes the host writes into `*handle[i] + 8` persist
// and the guest renders them, at its own anchor, in its own style. `CHAR+0x9c85`
// (row count) is likewise read live by the draw loop, the box height and the up/down
// clamp, so shrinking it shrinks the whole widget consistently.
//
// Registration itself stays 100 % NATIVE: the host writes the highlighted-row byte
// and asks the guest to confirm. Nothing in this module builds a quickbar ref — the
// skill ref packing runs through nested class tables (0x1a86bc/0x1a86d8) and is
// exactly the kind of thing a host should never reimplement.
//
// ── THE ROW MAP (the part that silently mis-fires if you get it wrong) ────────
// `popup_choose` (guest 0x13c920) dispatches on (tag, RAW ROW INDEX) only — labels
// and the row count play no part. Item rows go through an 8-entry table at
// 0x1a77cc, bound-checked `cmp #7; bls` at 0x13cd04. Skill rows are a bare
// `row == 0 ? invest : skill_bind(cat, cursor, row - 1)` at 0x13d308 with NO upper
// bound at all. So the host is authoritative about turning a DISPLAY row back into
// a GUEST row, and about clamping.
//
//   item  (tag 0x66, 8 native rows):  0 사용하기 | 1..5 N키등록 | 6 상세보기 | 7 버리기
//   skill (tag 0x67, 7 native rows):  0 포인트투자 | 1..5 N키등록 | 6 등록취소
//
//   display 0 -> guest 0        (사용하기 / 포인트투자, passed straight through)
//   display 1 -> guest slot+1   (our 퀵바등록, resolved when the player taps a plate)
//   display 2 -> guest 6        (상세보기 / 등록취소)
//   display 3 -> guest 7        (버리기 — items only)

/// Menu page, and the `kind` argument `popup_key`/`popup_choose` expect: 2 = the
/// inventory page, 4 = the skill page. [CONFIRMED — page draw/key tables at guest
/// 0x1ae5e0 / 0x1ae608 are indexed by this byte.]
const MENU_PAGE_OFF: u32 = 0x9c6e;
pub const PAGE_ITEM: u8 = 2;
pub const PAGE_SKILL: u8 = 4;

/// Popup state block, all CHAR-relative. [CONFIRMED]
const POPUP_OPEN_OFF: u32 = 0x9c84;
const POPUP_COUNT_OFF: u32 = 0x9c85;
const POPUP_SEL_OFF: u32 = 0x9c86;
const POPUP_TAG_OFF: u32 = 0x9c87;
// The inventory page's pre-emptor bytes moved to `guest_build::popup_preempt_offsets()`
// (2026-08-15, 51차): they are KTF measurements (`0x9c89`, `0xa220`) needed only where
// the commit bypasses the game's own key router, and on LGT the second one reads a
// different live field — which is what kept every CONSUMABLE popup un-relabelled on
// the device while skills (no pre-emptor gate) worked. See the table for the argument.

/// Popup flavour tag. ONLY these two carry register rows; 0x64 (2 rows), 0x65
/// (equipment), 0x68 (skill invest-only) and 0x69 (3-row item) must be left alone.
const TAG_ITEM: u8 = 0x66;
const TAG_SKILL: u8 = 0x67;
/// Native row counts for those tags — part of the gate, because `CHAR+0x9c87` is
/// NEVER CLEARED (`free_popup` at guest 0x12d77c zeroes only the count), so a stale
/// tag from a closed popup would otherwise pass.
const ROWS_ITEM: i8 = 8;
const ROWS_SKILL: i8 = 7;
/// Row counts after our rewrite (used for the idempotency half of the gate).
const OUR_ROWS_ITEM: i8 = 4;
const OUR_ROWS_SKILL: i8 = 3;

/// Each label buffer is a 10-byte allocation (guest 0x12ce54 `movs r0,#0xa`).
const POPUP_LABEL_CAP: usize = 10;

// The commit mechanism — KTF's `popup_key(kind, -5)` call and LGT's synthesised `OK` —
// is a per-build table now: `guest_build::popup_commit()` / `PopupCommit`.

/// `퀵슬롯등록` in EUC-KR + NUL = 11 bytes (player, 2026-08-06: the feature is called
/// 퀵슬롯, not 퀵바, everywhere).
///
/// ★ This is ONE BYTE PAST the 10-byte label allocation (guest 0x12ce54 `movs r0,#0xa`),
/// and that is a deliberate, evidenced choice rather than an oversight. The GUEST
/// writes an 11-byte string into a buffer from this same allocator on every skill
/// popup it opens — `포인트투자` is also 5 syllables + NUL — so an 11-byte write here
/// has exactly the footprint the game itself has always had. A 10-byte-clean
/// alternative exists (`슬롯등록`, 9 bytes) and was rejected only because it is not
/// what was asked for; switch to it if this is ever suspected of corrupting a
/// neighbouring block.
const LABEL_QUICKSLOT: [u8; 11] = [0xc4, 0xfc, 0xbd, 0xbd, 0xb7, 0xd4, 0xb5, 0xee, 0xb7, 0xcf, 0x00];
/// `1키등록` + NUL — the label row 1 must ALREADY hold for the gate to pass. This is
/// the semantic check that no combination of stale bytes can fake.
const LABEL_KEY1: [u8; 8] = [0x31, 0xc5, 0xb0, 0xb5, 0xee, 0xb7, 0xcf, 0x00];

/// Skill "is this skill learned" table: `(i8)*(CHAR + 0x1a8 + (cat*9 + idx)*8)`.
/// `<= 0` means unlearned, and `skill_bind`'s guard at guest 0x145cbe routes that to
/// the UNBIND path — i.e. asking to register an unlearned skill silently clears the
/// slot instead. The host refuses instead of offering it.
const SKILL_LEARNED_BASE: u32 = 0x1a8;
const SKILL_CAT_OFF: u32 = 0x9c80;
const SKILL_CURSOR_BASE: u32 = 0x9c7d;

/// What the host needs to know about a live, rewritable register popup.
///
/// Two builds fill this. hero3 (`popup_view`): the field-menu overlay's context popup,
/// rows `0 = 사용하기/포인트투자 · 1 = 퀵슬롯등록 · 2.. = the native tail`. hero2
/// (`h2_popup_view`): the SKILL / BEAST window's `d` popup, rows `0..bind_row = the
/// game's own · bind_row = 퀵슬롯등록 · unbind_row = 퀵슬롯해제`. [`popup_row`] is the
/// one place that turns a display row into a meaning, for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupView {
    /// `kind` for `popup_key` / `popup_choose` (hero3: 2 = item page, 4 = skill page;
    /// hero2: the popup's own kind byte, 3).
    pub kind: u8,
    /// True for the item popup, false for the skill popup.
    pub item: bool,
    /// The DISPLAY row the guest is currently highlighting (0..rows-1).
    pub sel: i8,
    /// How many rows our rewritten list has (hero3 4 item / 3 skill; hero2 4 / 3).
    pub rows: i8,
    /// Whether the 퀵슬롯등록 row may actually be taken. False for a skill the
    /// character has not learned — see [`SKILL_LEARNED_BASE`] (hero3 only).
    pub can_bind: bool,
    /// hero2: the display row that reads `퀵슬롯등록` — also the guest row of `1번에
    /// 등록` once the native list is restored, so guest slot `g` commits as
    /// `bind_row + g`. hero3: always 1.
    pub bind_row: i8,
    /// hero2: the display row that reads `퀵슬롯해제` (host-handled). hero3: none.
    pub unbind_row: Option<i8>,
    /// hero2: the popup object's handle, so the commit can prove it is still talking
    /// to the popup it read. hero3: 0.
    pub popup: u32,
    /// hero2: the native row count (6 or 5) the restore puts back. hero3: 0 (it has
    /// its own table).
    pub native_rows: i8,
    /// hero2: the quickbar byte the game will write for this skill — `h + 1` for the
    /// SKILL window, `h + 10` for the BEAST window. What 퀵슬롯해제 searches for and
    /// what the plate store remembers. hero3: 0.
    pub value: u8,
    /// hero2, `item == true` (S7): the `itmtp` type of the record under the inventory
    /// cursor — what the plate store remembers for an item plate. 0 otherwise.
    pub item_kind: u8,
    /// ★ #13 step 7: the display row that reads 창고에넣기 / 가방에넣기 — the host's MOVE
    /// row on a bag window's item popup, when it is there. hero3: none.
    pub move_row: Option<i8>,
    /// ★ #22: the display row that reads 스탯초기화 / 스킬초기화 on a 망각의 구 record,
    /// and which orb it is. hero3: none.
    pub reset_row: Option<(i8, crate::guest_build::H2Oblivion)>,
    pub flight: Option<[u8; 6]>,
}

/// What a tap / OK on display row `display` of a live [`PopupView`] means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupRow {
    /// One of the game's own rows: hand it an ordinary `OK`, the highlight already
    /// points at it.
    PassThrough,
    /// `퀵슬롯등록` — ask the player which plate (`UserEvent::PickingSlot`).
    Bind,
    /// `퀵슬롯등록` on a skill the game would silently UNBIND instead (hero3's learned
    /// guard): refuse.
    BindRefused,
    /// `퀵슬롯해제` (hero2): the HOST clears the slot holding this skill.
    Unbind,
    /// A native tail row hidden behind our rewrite (hero3 상세보기 / 버리기 / 등록취소):
    /// commit it as guest row `.0`.
    Remap(u8),
    /// ★ 창고에넣기 / 가방에넣기 (hero2, #13): the HOST moves the record between the
    /// inventory and the ACCOUNT warehouse.
    Move,
    /// ★ 스탯초기화 / 스킬초기화 (hero2, #22): the HOST refunds the character's spent
    /// points and consumes the 망각의 구.
    Reset(crate::guest_build::H2Oblivion),
    Flight(Option<usize>),
    /// Not a row of this popup.
    None,
}

/// The one table that turns a display row into a meaning — see [`PopupView`].
pub fn popup_row(view: &PopupView, display: i8) -> PopupRow {
    if crate::guest_build::hero2_java().is_some() {
        if display < 0 || display >= view.rows {
            return PopupRow::None;
        }
        if let Some(action) = view.flight.as_ref().and_then(|rows| rows.get(display as usize)) {
            if *action == 254 { return PopupRow::Flight(None); }
            if *action < 6 { return PopupRow::Flight(Some(*action as usize)); }
        }
        if let Some((row, what)) = view.reset_row
            && display == row
        {
            return PopupRow::Reset(what);
        }
        if Some(display) == view.move_row {
            return PopupRow::Move;
        }
        if display == view.bind_row {
            return PopupRow::Bind;
        }
        if Some(display) == view.unbind_row {
            return PopupRow::Unbind;
        }
        return if display < view.bind_row { PopupRow::PassThrough } else { PopupRow::None };
    }
    match display {
        0 => PopupRow::PassThrough,
        1 if view.can_bind => PopupRow::Bind,
        1 => PopupRow::BindRefused,
        d => match popup_guest_row(view, d, 0) {
            Some(r) => PopupRow::Remap(r),
            None => PopupRow::None,
        },
    }
}

/// Base of `popup_handle[8]`, or `None` when this build's array has not been located
/// (or, on KTF, when the slot does not hold a heap address yet).
///
/// Build-aware since 2026-08-11 (7차): KTF keeps the array behind a data slot, LGT
/// compiles the array itself into `.bss` — the same disagreement as the CHAR record.
/// See `guest_build::PopupHandles` for the LGT hunt and why it unblocks three matrix
/// rows at once.
fn popup_handles(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u32> {
    match crate::guest_build::popup_handles()? {
        crate::guest_build::PopupHandles::Pointer { global } => {
            let base = rd_u32(read, global)?;
            in_heap(base).then_some(base)
        }
        // A static array needs no validation of its own: it is always mapped, and
        // "is a popup open" is answered by the HANDLES being non-NULL, which is
        // exactly what `popup_label_addr` tests one line later.
        crate::guest_build::PopupHandles::Static(base) => Some(base),
    }
}

/// Payload address of label buffer `i` — `*(handle[i]) + 8` on KTF, the pointer
/// itself on LGT.
///
/// ★ Resolved through the HANDLE array every time, never through the `popup_text`
/// cache: `free_popup` NULLs the handles but leaves the text cache pointing at freed
/// memory, so the cache is a dangling read after any close.
///
/// The hop count is [`deref_container`]'s business, not this function's: it is the
/// carrier runtime's pointer convention, the same one that settles the inventory grid
/// and the AREA object. Verified on LGT by dumping the array with a popup open — the
/// pointers land directly on the EUC-KR text with no allocator header in front.
fn popup_label_addr(read: &impl Fn(u32, usize) -> Vec<u8>, handles: u32, i: u32) -> Option<u32> {
    deref_container(read, handles + i * 4)
}

/// Is the game showing a context popup we may rewrite, and in what state?
///
/// Every clause below is load-bearing; the two subtle ones are the row count (the
/// tag is never cleared, so tag alone lets a closed popup through) and the label
/// check (the only test that cannot be satisfied by stale state).
pub fn popup_view(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<PopupView> {
    // ★★ hero2 has its own popup — a Java object, found by walking the window chain
    // (2026-09-17, HERO2_QUICKSLOT_TOUCH_PLAN.md §4.2).
    if let Some(h2) = crate::guest_build::hero2_java() {
        return h2_popup_view(read, &h2);
    }
    // ⛔ hero3-measured offsets — see `guest_build::char_record_is_hero3`.
    if !crate::guest_build::char_record_is_hero3() {
        return None;
    }
    let c = resolve_char(read)?;
    // 1) a menu overlay owns the screen. NOT the window id at CHAR+0xa0c0 — guest
    //    0x124208 tests this byte and branches away BEFORE the window id is loaded,
    //    so the window id can still read 3 (field) with a menu up.
    if rd_u8(read, c + SELECTOR_FLAG_OFF)? == 0 {
        return None;
    }
    // 2) the page, which is also `popup_key`'s `kind`.
    let kind = rd_u8(read, c + MENU_PAGE_OFF)?;
    if kind != PAGE_ITEM && kind != PAGE_SKILL {
        return None;
    }
    // 3) a popup is open.
    if rd_u8(read, c + POPUP_OPEN_OFF)? == 0 {
        return None;
    }
    // 4) …of a flavour that HAS register rows.
    let tag = rd_i8(read, c + POPUP_TAG_OFF)? as u8;
    let item = match (tag, kind) {
        (TAG_ITEM, PAGE_ITEM) => true,
        (TAG_SKILL, PAGE_SKILL) => false,
        _ => return None,
    };
    // 5) the row count is either the native one or the one WE wrote (idempotent).
    let (native_rows, our_rows) = if item {
        (ROWS_ITEM, OUR_ROWS_ITEM)
    } else {
        (ROWS_SKILL, OUR_ROWS_SKILL)
    };
    let rows = rd_i8(read, c + POPUP_COUNT_OFF)?;
    if rows != native_rows && rows != our_rows {
        return None;
    }
    // 6) none of this build's inventory-page pre-emptors is set. Per-build because the
    //    gate only serves a commit that BYPASSES the game's key router (KTF's direct
    //    call); LGT's synthetic OK rides the router itself, and its list is empty —
    //    the KTF offsets read unrelated LGT fields (see the anchors table).
    if item {
        for off in crate::guest_build::popup_preempt_offsets() {
            if rd_u8(read, c + off)? != 0 {
                return None;
            }
        }
    }
    // 7+8) the label buffers exist AND row 1 reads either the game's `1키등록` or the
    //      `퀵슬롯등록` we already wrote. This is the check that makes the whole gate
    //      trustworthy rather than merely plausible.
    let handles = popup_handles(read)?;
    let row1 = popup_label_addr(read, handles, 1)?;
    // Our label is 11 bytes — one past the allocation, see `LABEL_QUICKSLOT` — so the
    // idempotency read has to be long enough to see all of it, not `POPUP_LABEL_CAP`.
    let got = read(row1, POPUP_LABEL_CAP.max(LABEL_QUICKSLOT.len()));
    if got.len() < LABEL_KEY1.len() {
        return None;
    }
    let is_native = got[..LABEL_KEY1.len()] == LABEL_KEY1;
    let is_ours = got.len() >= LABEL_QUICKSLOT.len() && got[..LABEL_QUICKSLOT.len()] == LABEL_QUICKSLOT;
    if !is_native && !is_ours {
        return None;
    }
    // 9) items: the bag double-indirect must resolve — `popup_choose`'s item arm
    //    derefs it unguarded (guest 0x13c996/0x13c998).
    if item && resolve_grid_base(read, c).is_none() {
        return None;
    }
    // 10) skills: may this one actually be bound?
    let can_bind = if item {
        true
    } else {
        let cat = u32::from(rd_u8(read, c + SKILL_CAT_OFF)?);
        let cursor = rd_i8(read, c + SKILL_CURSOR_BASE + cat)?;
        if cursor < 0 {
            return None;
        }
        let idx = cat * 9 + cursor as u32;
        rd_i8(read, c + SKILL_LEARNED_BASE + idx * 8).is_some_and(|v| v > 0)
    };
    Some(PopupView {
        kind,
        item,
        sel: rd_i8(read, c + POPUP_SEL_OFF)?,
        rows: our_rows,
        can_bind,
        bind_row: 1,
        unbind_row: None,
        popup: 0,
        native_rows: 0,
        value: 0,
        item_kind: 0,
        move_row: None,
        reset_row: None,
        flight: None,
    })
}

/// The writes that turn the game's list into ours. Empty when it is already ours.
///
/// The tail labels (`상세보기` / `버리기` / `등록취소`) are COPIED OUT OF THE GUEST'S
/// OWN BUFFERS rather than hardcoded, so this survives a build whose string resource
/// differs. Returned as (address, bytes) so the caller does the poking and can log it.
pub fn popup_relabel(read: &impl Fn(u32, usize) -> Vec<u8>, view: &PopupView) -> Vec<(u32, Vec<u8>)> {
    // Same per-item gate as `popup_sel_addr`: never OFFER 퀵슬롯등록 on a build that
    // cannot then run it. Every write below is a data write — into the game's own label
    // allocations (reached through the per-build `popup_handles` anchor) and the count
    // byte — with no build-specific address anywhere.
    if crate::guest_build::popup_commit().is_none() {
        return Vec::new();
    }
    if let Some(h2) = crate::guest_build::hero2_java() {
        return h2_popup_relabel(read, &h2, view);
    }
    let Some(c) = resolve_char(read) else {
        return Vec::new();
    };
    let Some(handles) = popup_handles(read) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // Row 1 becomes 퀵슬롯등록. Skipped when it already is — this runs every frame the
    // popup is up (other windows write the count byte), so it has to be idempotent.
    if let Some(a) = popup_label_addr(read, handles, 1) {
        let cur = read(a, LABEL_QUICKSLOT.len());
        if cur.len() != LABEL_QUICKSLOT.len() || cur != LABEL_QUICKSLOT {
            out.push((a, LABEL_QUICKSLOT.to_vec()));
        }
    }
    // The native tail rows slide down into rows 2.. .
    let moves: &[(u32, u32)] = if view.item { &[(2, 6), (3, 7)] } else { &[(2, 6)] };
    for (dst, src) in moves {
        let (Some(da), Some(sa)) = (popup_label_addr(read, handles, *dst), popup_label_addr(read, handles, *src)) else {
            continue;
        };
        let text = read(sa, POPUP_LABEL_CAP);
        // Copy up to and including the terminator; a buffer with no NUL in 10 bytes is
        // not something the game produced, so leave it alone.
        let Some(n) = text.iter().position(|b| *b == 0) else { continue };
        let bytes = text[..=n].to_vec();
        if read(da, bytes.len()) != bytes {
            out.push((da, bytes));
        }
    }
    // …and the count shrinks, which shrinks the drawn box and the cursor clamp too.
    let want = view.rows as u8;
    if rd_u8(read, c + POPUP_COUNT_OFF) != Some(want) {
        out.push((c + POPUP_COUNT_OFF, vec![want]));
    }
    out
}

/// Turn a DISPLAY row into the guest row `popup_choose` expects. `slot` is the
/// quickbar plate the player picked, and is only consulted for display row 1.
///
/// Returns `None` for a row that should be passed through untouched (display 0) or
/// that does not exist.
pub fn popup_guest_row(view: &PopupView, display: i8, slot: u8) -> Option<u8> {
    // hero2: the native list is `[game rows..][1번][3번][7번][9번]`, so guest slot `g`
    // is native row `bind_row + g` — and only the bind row remaps (`popup_row` handles
    // the rest). The guest has exactly four slots (`Hero2Java::QUICKBAR_LEN`).
    if view.native_rows > 0 {
        return (display == view.bind_row && u32::from(slot) < crate::guest_build::Hero2Java::QUICKBAR_LEN)
            .then(|| view.bind_row as u8 + slot);
    }
    match display {
        // ★ `then`, not `then_some`: `then_some` evaluates its argument EAGERLY, so
        // `slot + 1` overflowed for slot 255 before the range test could reject it
        // (debug panic; a silent wrap in release). Caught by the selftest.
        1 => (slot < QB_SLOTS as u8).then(|| slot + 1),
        2 => Some(6),
        3 => view.item.then_some(7),
        _ => None,
    }
}

/// The address of the highlighted-row byte, for the commit poke. Separate from
/// [`popup_view`] so the caller re-resolves CHAR at commit time rather than trusting
/// a pointer captured a frame earlier.
pub fn popup_sel_addr(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u32> {
    if let Some(h2) = crate::guest_build::hero2_java() {
        // The popup's own cursor byte (`j.h`), re-walked at commit time.
        return crate::guest_build::h2_popup_walk(read, &h2)
            .map(|p| p.cursor_addr())
            .or_else(|| crate::guest_build::h2_item_popup_walk(read, &h2, crate::bag_page_ctx()).map(|p| p.cursor_addr()));
    }
    // Gated on "does this build know how to COMMIT a row" rather than on the blanket
    // flag (2026-08-11, §3.6 discipline): the write itself needs no build-specific
    // address, only an offset, and `POPUP_SEL_OFF` is verified on both builds — LGT read
    // 0 with the cursor on row 0 and took a poked 1 as row 1, which is what made the
    // whole rewrite portable (see `guest_build::PopupCommit`). Offering the row and then
    // being unable to run it is the combination that must not ship, and that is exactly
    // what `popup_commit()` answers.
    crate::guest_build::popup_commit()?;
    resolve_char(read).map(|c| c + POPUP_SEL_OFF)
}

/// `Some((address, native_row_count))` when the popup's row count is currently OURS and
/// has to be put back before a [`crate::guest_build::PopupCommit::SyntheticOk`] commit.
///
/// ★ Only the synthetic-OK path needs this, and the reason is a TICK. Our relabel shrinks
/// `CHAR+0x9c85` to 4 (or 3), while the row we commit can be as high as 5 (plate 4 →
/// guest row 5). A direct guest call reads the row byte with no guest code running in
/// between; a synthesised keypress is delivered on the next tick, and the guest's own
/// cursor clamp could pull an out-of-range row back inside our shrunken count first.
/// Restoring the native count removes the window — the popup is about to close anyway,
/// and the count byte is the one this rewrite already owns.
pub fn popup_count_restore(read: &impl Fn(u32, usize) -> Vec<u8>, view: &PopupView) -> Option<(u32, u8)> {
    let c = resolve_char(read)?;
    let native = if view.item { ROWS_ITEM } else { ROWS_SKILL } as u8;
    (rd_u8(read, c + POPUP_COUNT_OFF)? != native).then_some((c + POPUP_COUNT_OFF, native))
}

/// Every write that puts the popup's NATIVE shape back before a synthetic-OK commit —
/// hero3: the one count byte ([`popup_count_restore`]); hero2: the label array's length
/// field, the popup's row count and its box height (three fields that were shrunk
/// together and must grow back together, or the painter indexes past the array).
pub fn popup_restore_writes(read: &impl Fn(u32, usize) -> Vec<u8>, view: &PopupView) -> Vec<(u32, Vec<u8>)> {
    if let Some(h2) = crate::guest_build::hero2_java() {
        return h2_popup_restore_writes(read, &h2, view);
    }
    popup_count_restore(read, view).map(|(a, b)| vec![(a, vec![b])]).unwrap_or_default()
}

// ── hero2: the SKILL / BEAST window's context popup as a `PopupView` ────────────────
//
// The walk, the shape rules and the write producers are `guest_build::h2_popup_*`
// (`hero2java.rs`, shared with the probe). This layer types the result for the worker
// and applies the write gate.

/// hero2's [`popup_view`]: the register popup of the SKILL / BEAST window, in either the
/// native or the rewritten shape — or (S7) the INVENTORY window's recovery-item popup
/// over a record the `0` key can use, whose two host rows are APPENDED (rows 3 / 4).
fn h2_popup_view(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &crate::guest_build::Hero2Java) -> Option<PopupView> {
    if let Some(p) = crate::guest_build::h2_popup_walk(read, h2) {
        let shape = p.shape()?;
        let value = p.value()?;
        return Some(PopupView {
            kind: crate::guest_build::Hero2Java::POPUP_KIND_CONTEXT,
            item: false,
            sel: p.sel,
            rows: shape.our_rows as i8,
            can_bind: true,
            bind_row: shape.bind_row as i8,
            unbind_row: Some(shape.bind_row as i8 + 1),
            popup: p.handle,
            native_rows: shape.native_rows as i8,
            value,
            item_kind: 0,
            move_row: None,
        reset_row: None,
        flight: None,
        });
    }
    let page_ctx = crate::bag_page_ctx();
    if let Some(p) = crate::guest_build::h2_item_popup_walk(read, h2, page_ctx)
        && p.shape().is_some()
        && crate::guest_build::h2_heal_cycle(read, h2).is_some_and(|cycle| p.bindable(&cycle))
    {
        use crate::guest_build::Hero2Java;
        // 6 over a recovery item, 7 over 귀환서 (2026-09-18 ⑧) — the same three rows,
        // plus S7's two, plus — once the move-row mechanism has been past — a sixth.
        let rows = p.labels_len.max(u32::from(Hero2Java::ITEM_POPUP_OUR_ROWS)) as i8;
        return Some(PopupView {
            kind: p.kind,
            item: true,
            sel: p.sel,
            rows,
            can_bind: true,
            bind_row: Hero2Java::ITEM_POPUP_NATIVE_ROWS as i8,
            unbind_row: Some(Hero2Java::ITEM_POPUP_NATIVE_ROWS as i8 + 1),
            popup: p.handle,
            native_rows: Hero2Java::ITEM_POPUP_NATIVE_ROWS as i8,
            value: 0,
            item_kind: p.item_kind as u8,
            move_row: (rows as u32 > u32::from(Hero2Java::ITEM_POPUP_OUR_ROWS)).then_some(rows - 1),
            reset_row: None,
        flight: None,
        });
    }
    // ★ #13 step 7: any other bag-window popup that carries the host's MOVE row (a
    //   kind-3 상세보기[/버리기] popup, or a recovery popup in the WAREHOUSE window). Every
    //   row below ours is the game's own; there is no bind row.
    let account = [crate::account_bag(), crate::account_materials()];
    let bags: Vec<u32> = [
        crate::guest_build::h2_static_handle(read, h2.inventory_slot),
        crate::guest_build::h2_static_handle(read, h2.material_bag_slot),
        account[0],
        account[1],
    ]
    .into_iter()
    .flatten()
    .collect();
    let p = crate::guest_build::h2_bag_popup_walk(read, h2, &bags, page_ctx)?;
    // ★ 비행서 (28, 1): two stages. Stage 1 is the game's own [사용하기 상세보기 버리기] (+ ★ 2026-10-21 the host's
    //   창고에넣기 / 가방에넣기, appended by `AccountState::poll_popup` like any record's — the scroll goes into the
    //   warehouse and comes back; row 0 = 254 = "write the town list"); stage 2 is the town list the host wrote over
    //   the labels (every row = a town index), which has no move row. The towns are the LIVE save's visited ones
    //   (`ag.q[31]`), so a scroll carried through the warehouse lists what THIS character has visited.
    if crate::guest_build::h2_is_flight(&p) {
        let picker = crate::guest_build::h2_flight_picker(read, h2, &p);
        let mut actions = [255; 6];
        let move_row = if picker.is_none() {
            crate::guest_build::h2_gm_row_handle(read, h2, crate::guest_build::h2_move_row_for(&p, &account))
                .and_then(|h| p.labels.iter().position(|&l| l == h))
                .and_then(|i| i8::try_from(i).ok())
        } else {
            None
        };
        if let Some(towns) = picker {
            for (row, town) in towns.into_iter().enumerate() { actions[row] = town as u8; }
        } else { actions[0] = 254; }
        let flight = Some(actions);
        // ★★ 2026-10-21 (user: 「비행서 아이템 퀵슬롯에 등록 되게」): stage 1 in the INVENTORY also carries a potion's
        //   퀵슬롯등록 / 퀵슬롯해제 (`h2_popup_wanted_rows`, after the game's three rows, before the move row); the plate
        //   names the scroll's display type (`H2_PLATE_FLIGHT`). With them absent the bind row stays past the end, so
        //   rows 1 · 2 (상세보기 · 버리기) are the game's either way.
        let (bind, unbind) = crate::guest_build::h2_plate_bind_rows(read, h2, &p);
        return Some(PopupView {
            kind: p.kind, item: true, sel: p.sel, rows: p.rows,
            can_bind: bind.is_some(), bind_row: bind.unwrap_or(p.rows), unbind_row: unbind,
            popup: p.handle, native_rows: p.rows, value: 0, item_kind: crate::guest_build::H2_PLATE_FLIGHT,
            move_row, reset_row: None, flight,
        });
    }
    let gm_row = crate::guest_build::h2_move_row_for(&p, &account);
    let move_row = match crate::guest_build::h2_move_row_plan(read, h2, &p, gm_row) {
        crate::guest_build::H2MoveRowPlan::Present(row) => Some(row as i8),
        _ => None,
    };
    // ★ #22: a 망각의 구 carries its own row as well, and either one alone is enough to
    // make this a popup the host owns rows on — the two are appended on separate polls,
    // so for one frame only one of them is there.
    let reset_row = crate::guest_build::h2_oblivion_row(read, h2, &p);
    // ★★ 2026-10-17 (user: 엘릭서 = 「포션종류 취급」): an elixir's popup in the inventory carries a potion's 퀵슬롯등록 /
    //   퀵슬롯해제 too (`h2_popup_wanted_rows`); the plate it registers names the elixir's display type
    //   (`H2_PLATE_ELIXIR`). Each row counts as soon as it is on — the two arrive a poll apart.
    let (bind, unbind) = crate::guest_build::h2_plate_bind_rows(read, h2, &p);
    if move_row.is_none() && reset_row.is_none() && bind.is_none() && unbind.is_none() {
        return None;
    }
    let plate = bind.is_some() || unbind.is_some();
    Some(PopupView {
        kind: p.kind,
        item: true,
        sel: p.sel,
        rows: p.labels_len as i8,
        can_bind: bind.is_some(),
        bind_row: bind.unwrap_or(p.labels_len as i8),
        unbind_row: unbind,
        popup: p.handle,
        native_rows: p.labels_len as i8 - 1,
        value: 0,
        // ★ 2026-10-21: the plate code the popup registers — the elixir's or the 상점주인의 혼's display type.
        item_kind: if plate { crate::guest_build::h2_popup_plate_code(read, h2, &p).unwrap_or(crate::guest_build::H2_PLATE_ELIXIR) } else { p.record.kind as u8 },
        move_row,
        reset_row,
        flight: None,
    })
}

/// hero2's [`popup_relabel`]: the writes that turn the native popup into ours, empty
/// when it already is. Re-walks the chain rather than trusting `view`.
fn h2_popup_relabel(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &crate::guest_build::Hero2Java, view: &PopupView) -> Vec<(u32, Vec<u8>)> {
    use crate::guest_build::JavaWrite;
    if !crate::guest_build::java_write_ok(JavaWrite::PopupLabel) || !crate::guest_build::java_write_ok(JavaWrite::PopupRows) {
        return Vec::new();
    }
    if view.flight.is_some() { return Vec::new(); }
    if view.item {
        // S7: the appended rows also need a HANDLE write into the label array's slack.
        if !crate::guest_build::java_write_ok(JavaWrite::PopupLabelRef) {
            return Vec::new();
        }
        let Some(p) = crate::guest_build::h2_item_popup_walk(read, h2, crate::bag_page_ctx()) else { return Vec::new() };
        if p.handle != view.popup {
            return Vec::new();
        }
        return crate::guest_build::h2_item_popup_relabel_writes(read, &p);
    }
    let Some(p) = crate::guest_build::h2_popup_walk(read, h2) else { return Vec::new() };
    if p.handle != view.popup {
        return Vec::new();
    }
    crate::guest_build::h2_popup_relabel_writes(read, &p)
}

/// hero2's [`popup_restore_writes`]: the native shape back, for the popup `view` names.
fn h2_popup_restore_writes(read: &impl Fn(u32, usize) -> Vec<u8>, h2: &crate::guest_build::Hero2Java, view: &PopupView) -> Vec<(u32, Vec<u8>)> {
    if view.item {
        let Some(p) = crate::guest_build::h2_item_popup_walk(read, h2, crate::bag_page_ctx()) else { return Vec::new() };
        if p.handle != view.popup {
            return Vec::new();
        }
        return crate::guest_build::h2_item_popup_restore_writes(&p);
    }
    let Some(p) = crate::guest_build::h2_popup_walk(read, h2) else { return Vec::new() };
    if p.handle != view.popup {
        return Vec::new();
    }
    crate::guest_build::h2_popup_restore_writes(&p)
}

// ── auto-loot ────────────────────────────────────────────────────────────────
//
// The game's "오토루팅(싱글)" cash item (category 0x12 index 0x0d, "아이템 자동 루팅
// (게임종료시 까지)") does exactly ONE thing: set this byte to 1 (guest 0x1540fc,
// the `useItem` jump-table arm for DB index 0x0d). Readers, both in the field:
//   * the per-tick player update (guest 0x141c60) runs the ground-pickup routine
//     0x1527fc when the byte is non-zero,
//   * the OK/'7' key row (guest 0x1242ec) SKIPS its manual pickup attempt when the
//     byte is set (the tick already does it).
// Writers in the whole image: the cash item (=1), CHAR_init at app start (=0), and
// the new-game reset (=0) — nothing else, no bit sharing, and it is NOT serialised
// into the save (hence the item's "until you exit the game" wording). So the host
// can simply keep it at 1: idempotent, and correct for New Game / Continue / death
// / map change without any ordering logic. [RE: workflow wf_477f0b16 QD]
//
// ★ 2026-08-13 — THE OFFSET IS NOW PER BUILD, and that is a bug fix, not tidying. This
// used to read a KTF constant behind `guest_code_addresses_valid()`, which is true only
// on KTF; the APK ships LGT, so on every real device this returned `None` and the flag
// was never set. Worse, the server had delisted the `오토루팅` cash item *because* the
// host was supposed to be doing this, so the player had no way to get auto-loot at all
// (player report 2026-08-13). The item is listed again (`hero3_protocol.AUTOLOOT_IDX`)
// and the offset now comes from `guest_anchors::autoloot_off`, which is `Some(0x2d7)` on
// KTF and `None` on LGT until the LGT byte is actually located — that note records the
// search. Asking the anchor table instead of the global flag is the §3.6 discipline:
// when LGT's offset lands, this function starts working with no other change.

/// `Some((address, value))` when auto-loot is currently OFF and should be forced on;
/// `None` when it is already on, when CHAR is not resolvable yet, or when this build's
/// flag is unknown (in which case the player buys the game's own `오토루팅` item).
///
/// Writes ONLY when the byte reads exactly 0. The value is per build (`1` on KTF, `0x81`
/// on LGT) and may well be a bitfield, so a non-zero byte is left alone rather than
/// overwritten wholesale — the game or an earlier write already owns it.
/// ★ 50차: THE FLAG IS AN ARRAY, indexed by the save slot (`CHAR+0xa410` on LGT — the
/// same word the save chain uses). The host used to write a fixed `CHAR+0x2d8`, which is
/// element **1**, so it only ever armed auto-loot for a character in save slot 1 and did
/// nothing for everyone else — the player report that started this. The address is now
/// computed the way the game's own `useItem` arm computes it
/// (`guest_anchors::AutolootFlag::offset_for`).
pub fn autoloot_to_force(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, u8)> {
    // Per-ITEM gate, not the blanket one: this is a guest WRITE into the character
    // record, so it needs the offset AND the value for the build that actually booted.
    let flag = crate::guest_build::autoloot_flag()?;
    let c = resolve_char(read)?;
    // Unreadable = refuse, rather than falling back to element 0: a wrong index here
    // writes a byte somewhere else in the record, which is the failure mode with no
    // symptom.
    let slot = flag.slot_off.map(|off| rd_u32(read, c + off));
    let addr = c + flag.offset_for(slot.unwrap_or(None))?;
    (rd_u8(read, addr)? == 0).then_some((addr, flag.value))
}

/// Whether this build can have auto-loot forced by the host at all. False means the
/// player has to buy the game's own `오토루팅` item; the UI layer says so once at boot
/// rather than letting the feature vanish silently, which is how it went unnoticed.
pub fn autoloot_forcible() -> bool {
    crate::guest_build::autoloot_flag().is_some()
}

// ── the flag-window watcher, for closing an offset ON A DEVICE ────────────────
//
// Static RE got the LGT auto-loot byte down to a candidate and no further: the game's own
// base is `CHAR+0x28` (accessor thunk 0x1068 -> 0x1503ab0, 647 call sites) and exactly one
// `strb #1` reachable through it lands in `game+0x200..0x300` — `0x51152` into
// `CHAR+0x2dd` — but that arm belongs to a dispatcher that is NOT `useItem`, so it may be
// something else entirely. `guest_anchors::autoloot_off` therefore says `None` for LGT.
//
// The measurement that settles it needs no more disassembly: the byte is written by the
// game's OWN `오토루팅` arm, so buy the item, use it, and watch which byte flips. This
// snapshots a window of single-byte flags and reports every transition, so the answer
// appears in logcat named and timestamped:
//
//     adb -s <serial> logcat -s wie_android:* | findstr flagwatch
//
// Kept in the shipping build on purpose: it is read-only, it only speaks when a byte
// actually changes, and the same procedure closes any of §2.11's remaining 🔶 offsets.
/// ★ 2026-08-13 — the window is the WHOLE character record, not a guessed neighbourhood.
/// The first trial watched `0x280..0x300` because KTF's anchor is `0x2d7`, and the one
/// byte that moved in it (`0x2d7`, to exactly `0x01`, at exactly the right moment) turned
/// out NOT to be the flag — writing it from the host does nothing. A window chosen around
/// the answer you expect can only ever confirm or mislead; it cannot find. Reading 43 KB
/// twice a second is a memcpy, so there is no reason to narrow it.
const FLAGWATCH_LO: u32 = 0x000;
const FLAGWATCH_LEN: usize = 0xa800;
/// Cap on changes reported per sample. A save load rewrites the whole record, and one line
/// carrying thousands of entries is worse than useless; the count still comes through.
const FLAGWATCH_MAX_REPORT: usize = 48;
/// A byte that changes on this many consecutive samples is a COUNTER, not a flag, and is
/// suppressed from then on. Without this the report is unreadable: the first device trial
/// (2026-08-13, S25) drowned in `CHAR+0x2ba` and `CHAR+0x2d4`, which tick every sample.
/// A flag transitions once and so never reaches the limit.
const FLAGWATCH_COUNTER_RUN: u8 = 3;

struct FlagWatch {
    prev: Vec<u8>,
    /// Consecutive samples in which each byte changed; saturates at the limit.
    run: Vec<u8>,
}

static FLAGWATCH: std::sync::Mutex<Option<FlagWatch>> = std::sync::Mutex::new(None);

/// `Some(report)` listing every byte in the watched window that changed since the last
/// call. `None` on the first call (which seeds the baseline), when nothing moved, or when
/// CHAR is not resolvable yet.
///
/// Transitions to 1 from 0 are flagged `<== 0->1` because that is the auto-loot
/// signature; everything else is reported too, since a neighbour moving in step is how
/// you tell a real flag from a byte that belongs to a wider field.
pub fn flag_window_changes(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<String> {
    // ⛔ hero3-measured offsets — see `guest_build::char_record_is_hero3`.
    if !crate::guest_build::char_record_is_hero3() {
        return None;
    }
    let c = resolve_char(read)?;
    let now = read(c + FLAGWATCH_LO, FLAGWATCH_LEN);
    if now.len() != FLAGWATCH_LEN {
        return None;
    }
    let mut guard = FLAGWATCH.lock().ok()?;
    let Some(w) = guard.as_mut() else {
        *guard = Some(FlagWatch { prev: now, run: vec![0; FLAGWATCH_LEN] });
        return None;
    };
    let mut hits = String::new();
    let mut muted = 0usize;
    let mut changed = 0usize;
    let mut shown = 0usize;
    for i in 0..FLAGWATCH_LEN {
        let (a, b) = (w.prev[i], now[i]);
        if a == b {
            w.run[i] = 0;
            continue;
        }
        w.run[i] = w.run[i].saturating_add(1);
        if w.run[i] >= FLAGWATCH_COUNTER_RUN {
            muted += 1; // a counter: it has now changed on N samples in a row
            continue;
        }
        changed += 1;
        if shown >= FLAGWATCH_MAX_REPORT {
            continue;
        }
        shown += 1;
        let off = FLAGWATCH_LO + i as u32;
        hits += &format!(
            " CHAR+{off:#05x}({:#x}) {a:#04x}->{b:#04x}{}",
            c + off,
            if a == 0 && b != 0 { " <== SET" } else { "" },
        );
    }
    w.prev = now;
    (changed > 0).then(|| {
        format!(
            "flagwatch ({}) CHAR={c:#x} +{FLAGWATCH_LO:#x}..+{:#x} \
             ({changed} changed, {muted} counters muted{}):{hits}",
            crate::guest_build::name(),
            FLAGWATCH_LO + FLAGWATCH_LEN as u32,
            if changed > shown { format!(", {} not shown", changed - shown) } else { String::new() },
        )
    })
}

/// The auto-loot flag's live value, the save slot that selects it, and the whole
/// slot array — because 50차 established that the "three refuted candidates" were never
/// three candidates at all, just elements 0, 6 and (the one being written) 1 of one
/// array. Printing the row is what makes that visible at a glance, and it is what says
/// whether the host armed the element the GAME is going to read.
pub fn autoloot_candidates(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<String> {
    // ★ hero3-shaped, like `offset_audit` above it: `+0x2d7` is a hero3 record offset.
    // On hero2 this printed eight bytes of the Java field block every 5 s as if they
    // were an auto-loot slot row (first device run, 2026-09-17). Decline instead.
    if !crate::guest_build::char_record_is_hero3() {
        return None;
    }
    let c = resolve_char(read)?;
    let flag = crate::guest_build::autoloot_flag();
    let slot = flag.and_then(|f| f.slot_off).and_then(|off| rd_u32(read, c + off));
    let live = match flag {
        Some(f) => match f.offset_for(slot) {
            Some(off) => format!(
                "+{off:#x}={:?} (want {:#04x}, bit7 must be set)",
                rd_u8(read, c + off),
                f.value
            ),
            None => "no usable slot yet".to_string(),
        },
        None => "unlocated".to_string(),
    };
    let base = flag.map_or(0x2d7, |f| f.base_off);
    let row: Vec<String> = (0..=7).map(|i| format!("{:?}", rd_u8(read, c + base + i))).collect();
    Some(format!(
        "autoloot flag {live} | save slot {slot:?} | slots[0..8] @+{base:#x} = [{}]",
        row.join(" ")
    ))
}

// ── the game's own top-HUD y origin ──────────────────────────────────────────
//
// Every y in the game's TOP band — the buff/status list at the left and the target
// monster's name/level/HP box at the right — is `Y0 + const`, where
// `Y0 = *(*(sl+0x428))`: a plain int global (image value 0). Two independent RE
// passes plus a hand check of all four referencing sites found ONLY READS
// (0x149be8, 0x149dc6, 0x14a716 and 0x159cae — the last one loads it into a local),
// so the host owns that value.
//
// The player asked for our gauges at the extreme top-left with the game's buff list
// BELOW them (2026-07-30). Rather than shoving our HUD sideways, we push the game's
// band down by writing this global. Side effect, and it is inherent: the target
// monster box shares the origin, so it moves down by the same amount.
// [RE: workflow wf_477f0b16 QA/QB (both CONFIRMED) + wf_3503d69e RB]
const SL: u32 = 0x1b_2c40;
const SL_HUD_Y_ORIGIN: u32 = SL + 0x428;

/// `Some(address)` of the y-origin int when it does not already hold `want`.
/// The address is read from the relocated pointer at `sl+0x428`, never hardcoded.
pub fn hud_y_origin_to_set(read: &impl Fn(u32, usize) -> Vec<u8>, want: u32) -> Option<u32> {
    // Guest CODE address / unverified-offset WRITE: KTF only until the LGT
    // equivalent is reverse engineered. See `guest_build`.
    if !crate::guest_build::guest_code_addresses_valid() {
        return None;
    }
    let p = rd_u32(read, SL_HUD_Y_ORIGIN)?;
    if !in_heap(p) {
        return None;
    }
    (rd_u32(read, p)? != want).then_some(p)
}

// ── move the game's buff band sideways ───────────────────────────────────────
//
// The band's x has NO origin global (unlike y): each of its three draws computes
// `LAY[0x30] * i + K` with its own one-byte immediate, where LAY[0x30] is the ui_bm
// frame-3 width (the pitch, 20) and K is a bare Thumb ADD-imm8:
//
//   guest 0x149c2e  `adds r1, #1`  (0x3101)  frame  -> x = w*i + 1
//   guest 0x149c8c  `adds r4, #3`  (0x3403)  icon   -> x = w*i + 3
//   guest 0x149d2c  `adds r1, #1`  (0x3101)  tick number -> x = w*i + w/2 + 1
//
// ALL THREE must move together or the band tears apart (first attempt moved only
// the frame and left the icons at the far left). The relative +2 of the icon is
// preserved. [RE: workflow wf_69d1cba7 RC1, HIGH]
//
// ⚠ Do NOT touch guest 0x149c48 (`movs r3,#0x14`): that multiply by i is the 20-byte
// BUFF RECORD stride, not a coordinate.
//
// ★ 2026-08-11 (10차): the per-build addresses moved into
// `guest_anchors::GuestBuild::band_x_sites`, which is where the LGT trio
// (`0x2372`/`0x23ec`/`0x2450`) and its derivation live. The reasoning above is about the
// GAME, so it stays here at the call site — the same split as `hud_bottom_patch`.

/// Every band-x immediate that is not already at `want` (+ its own delta), as
/// `(address, byte)`. Only instructions that still look like the original
/// `adds rN, #imm8` are offered, so a different build is left alone.
///
/// Not gated on `guest_code_addresses_valid` any more: the per-build table answers "do we
/// know this build's sites" precisely, and the opcode check below is what makes the write
/// safe either way. An empty table means no writes at all.
pub fn band_x_patches(read: &impl Fn(u32, usize) -> Vec<u8>, want: u8) -> Vec<(u32, u8)> {
    let mut out = Vec::new();
    for &(addr, opcode, delta) in crate::guest_build::band_x_sites() {
        let Some(cur) = rd_u16(read, addr) else { continue };
        if cur & 0xff00 != opcode {
            continue; // not the instruction we know
        }
        let target = want.saturating_add(delta);
        if (cur & 0xff) as u8 != target {
            out.push((addr, target));
        }
    }
    out
}

// ── move the game's target-monster box LEFT ──────────────────────────────────
//
// Player request (2026-07-30): "the minimap has to be at the very top-right and the
// monster name/HP/LV box to its LEFT". Both want the same corner, and the game's box
// is RIGHT-EDGE anchored, so the box is what moves.
//
// Its block is guest 0x149db2..0x149f78 inside the field-HUD renderer (entered by the
// `bgt` at 0x149dac; the else-path branches straight to 0x149f7a, the bottom-HUD block
// the host already skips). EVERY x in it is right-anchored off the same two values —
// W = the screen width at [sp,#0x100], and FW = the frame-4 width (85) at [r6]+0x40 —
// and each ends in its OWN add/sub-imm8 on r1:
//
//   0x149df2  subs r1,#2      x = W - FW - 2     the 85x26 frame          -> 0x11aa4c
//   0x149e1e  adds r1,#1      x = W - FW + 1     HP-bar background rect   -> 0x10defc
//   0x149e58  subs r1,#0x2c   x = W - 44         name / level text        -> 0x13ecfc
//   0x149ebc  adds r1,#0xf    x = W - FW + 15    second text run          -> 0x13ecfc
//   0x149f42  adds r1,#0x1f   x = W - FW + 31    HP-bar fill, left end    -> 0x10ec80
//   0x149f68  adds r1,#0x1f   x = W - FW + r7+31 HP-bar fill, right end   -> 0x10ec80
//
// ★ AND THE WHOLE BLOCK EXISTS TWICE. The selector at 0x149d5a-0x149d6a reads
// `(s8)CHAR[0x1ea]`: > 0 takes PATH A inline, otherwise `bl #0x14a6aa` acts as a LONG
// BRANCH (0x93a bytes is out of b.n range; LR was already saved by the entry push and
// the block never pops) into PATH B at 0x14a6aa..0x14a88c — a byte-identical second
// copy with its own six immediates, exiting via `b #0x149f7a` or `bl #0x149f6a` into
// path A's shared tail. Patching only path A would move the box for one target kind
// and not the other. [caught by the workflow's two independent derivations after the
// main loop's own sweep covered only path A - both agreed, and both were byte-verified]
//
// ALL TWELVE must move together - the buff band already taught us that patching a
// subset tears the widget apart (the icons stayed at the far left). There is no single
// shared source to patch instead: each site recomputes `W - FW` from scratch. The two
// values they DO share are FW - which is also the blitter's 4bpp source row stride and
// must never be touched (same rule as the band's cell width) - and the screen-width
// local at [sp,#0x100], whose only producer is `ldr r3,[r0,#0x48]; lsls #16; asrs #16;
// str r3,[sp,#0x100]` at 0x149ba6. Overwriting the sign-extend pair there would be a
// 2-halfword single-point fix, but it would also DISCARD the sign extension of a field
// the game itself treats as 16-bit-in-32, so the 12 immediates are the safe route.
//
// Shifting left by N just makes each site's signed constant `orig - N`, re-encoded as
// ADDS when it stays >= 0 and SUBS when it goes negative — both are 16-bit Thumb, so
// the instruction never changes length. Flags are safe: every one of the six is
// immediately followed by another flag-setting ALU op before any conditional.
// [RE: main loop, byte-verified against md5 3792a476; cross-checked by workflow
//  wf_6dac323a-daf]
//
// ★ 2026-08-11 (10차): the per-build address list moved into
// `guest_anchors::GuestBuild::target_box_x_sites`. LGT has TEN sites rather than twelve
// because its two HP-bar-fill draws are SHARED by both paths — see that table.

// ★ 2026-09-25: hero2 has the same widget and ONE site (`subs r7,#0x42`, the panel image
// and its HP bar share one register), which is why the encoder now keeps each site's own
// register instead of assuming r1, and why the shift ceiling is a function of the build
// (211 here, 189 on hero2). Both moved into `guest_anchors` next to the sites, so the
// probe runs the same planner (`GuestBuild::target_box_x_writes`).

/// Every target-box x immediate that is not already encoding a left shift of
/// `shift` guest px, as `(address, halfword_le)`. Idempotent, so the periodic poke
/// pass can re-assert it forever. See `guest_anchors::GuestBuild::target_box_x_writes`.
pub fn target_box_x_patches(read: &impl Fn(u32, usize) -> Vec<u8>, shift: u32) -> Vec<(u32, [u8; 2])> {
    crate::guest_build::target_box_x_writes(read, shift)
}

// ── shrink the game's buff band ──────────────────────────────────────────────
//
// The band cell's size is read at RUNTIME from the ui_bm frame-3 record:
//   LAY = *(CHAR+0xa384) ; w = u16 @LAY+0x30 (20) ; h = u16 @LAY+0x32 (25).
//
// Poking **h** is SAFE: the blitter uses it only as the DESTINATION ROW COUNT — it
// crops 1:1, it never scales (left-edge clip advances srcX by a destination delta;
// the row loop advances src by w/2 bytes per row) — plus it anchors the tick
// number's y. Nothing outside this loop reads frame 3.
//
// ⚠ NEVER poke **w** (LAY+0x30): it is the 4bpp SOURCE ROW STRIDE as well as the
// cell pitch and the tick-centring divisor. Lowering it shears the graphic
// progressively down the cell, and an odd value diverts into a different blitter.
// [RE: workflow wf_69d1cba7 RC2, HIGH]
//
// Floor: the icon is a hardcoded 15x15 blit at y+5, so below 20 it gets clipped.
// Shrinking also crops the frame's own bottom border row (the alternative — repacking
// the jar asset — would keep it, at the cost of touching the game archive).
const LAY_PTR_OFF: u32 = 0xa384;
const BAND_FRAME3_H_OFF: u32 = 0x32;
pub const BAND_H_MIN: u16 = 20;
pub const BAND_H_ORIG: u16 = 25;

/// `Some((address, bytes))` when the band's cell height is not already `want`.
/// Re-applied for free by the periodic poke pass, which also covers the ui_bm
/// loader rebuilding the record.
pub fn band_height_poke(read: &impl Fn(u32, usize) -> Vec<u8>, want: u16) -> Option<(u32, [u8; 2])> {
    // Guest CODE address / unverified-offset WRITE: KTF only until the LGT
    // equivalent is reverse engineered. See `guest_build`.
    if !crate::guest_build::guest_code_addresses_valid() {
        return None;
    }
    let c = resolve_char(read)?;
    let lay = rd_u32(read, c + LAY_PTR_OFF)?;
    if !in_heap(lay) {
        return None;
    }
    let addr = lay + BAND_FRAME3_H_OFF;
    let cur = rd_u16(read, addr)?;
    // Only ever touch a record that still looks like frame 3.
    if !(BAND_H_MIN..=BAND_H_ORIG).contains(&cur) {
        return None;
    }
    let want = want.clamp(BAND_H_MIN, BAND_H_ORIG);
    (cur != want).then_some((addr, want.to_le_bytes()))
}

// ── stop the game drawing its own BOTTOM field HUD ───────────────────────────
//
// ★ The world IS drawn underneath it. The field paint (guest 0x1248a6..0x1248ba)
// runs the map/world renderers first and calls the HUD renderer 0x149b30 LAST, and
// the HUD's box is only 170 px wide, centred — world pixels are visible either side
// of it on the same rows. So the box is an overlay on top of finished world, which
// is why hiding it by painting black looked wrong: there is real world under there,
// it just gets overwritten. Skipping the draw shows it.
//
// Guest 0x149f7a is exactly where the renderer, having finished the buff band and
// the target-monster box, starts the bottom block: it computes that box's origin
// (`(screen_w - 0xaa)/2 + 2`, `screen_h - 0x18`) and then paints the panel, the
// HP/SP/EXP bars and the 5 quickbar cells — every one of which our overlay already
// replaces. Branching from there to the function's own epilogue (0x14a324) drops
// exactly that block and keeps everything above it.
//
// Checked before adopting: the only non-stack stores in the skipped range
// (0x14a2a0 / 0x14a2bc / 0x14a2d0) are sl-relative HUD blink counters, not game
// state. Nothing else in the range writes CHAR.
//
// ★ 2026-08-10: the LGT build has this block too (renderer 0x22a4, site 0x2864), so
// the three numbers moved into `guest_build::hud_bottom_patch()` — that table carries
// the per-build addresses and how each was found. The reasoning above is about the
// GAME, not the compile, so it stays here at the call site.

/// The writes that stop the game's bottom field HUD, IN ORDER, while it is still being
/// drawn. Empty once patched, if the code is not what we expect, or on a build where the
/// block has not been located.
///
/// ★ Two sites on hero2 (2026-09-25, `guest_anchors::GuestBuild::hud_bottom_tail`): the
/// TAIL comes first and the HEAD only if the tail already reads as patched or is in this
/// same plan — the head alone would run the target panel and then fall into the notice
/// block and the whole bottom HUD. A tail that reads as neither (not this build's code)
/// cancels the plan outright. The caller writes in order and stops at the first failure,
/// so a failed tail write can never leave the head landed. The planner itself is
/// `GuestBuild::hud_bottom_writes`, shared with the probe.
///
/// Not gated on `guest_code_addresses_valid` any more: the per-build table answers
/// "do we know this build's site" precisely, where the flag could only say "is this
/// KTF" — and the byte check below is what makes writing it safe either way.
pub fn hud_bottom_patch(read: &impl Fn(u32, usize) -> Vec<u8>) -> Vec<(u32, [u8; 2])> {
    crate::guest_build::kind().map_or_else(Vec::new, |k| k.hud_bottom_writes(read))
}

/// hero2's world-viewport `− 26` patch, `(address, bytes)`, while the site still reads
/// its original `subs r1, #0x1a`. Same contract as [`hud_bottom_patch`]; see
/// `guest_anchors::viewport_full_patch` for the RE and the A/B.
pub fn viewport_full_patch(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, [u8; 2])> {
    let (site, orig, patched) = crate::guest_build::viewport_full_patch()?;
    let cur = rd_u16(read, site)?;
    (cur == orig).then_some((site, patched.to_le_bytes()))
}

/// hero2's `q`-grid shift: every site that still reads as shipped, as `(address, bytes)`.
///
/// Same read-verify-then-write contract as [`hud_bottom_patch`], but a LIST — the shift
/// takes two sites and both must land or the grid and its selection rect disagree by
/// 7 px. The caller writes them all and only then publishes `crate::INV_GRID_SHIFT`;
/// `already` counts the sites that read as patched, so a second pass over a patched
/// image is a no-op that still confirms.
pub fn inventory_grid_shift(read: &impl Fn(u32, usize) -> Vec<u8>) -> (Vec<(u32, [u8; 2])>, usize, usize) {
    crate::guest_build::h2_halfword_writes(read, crate::guest_build::inventory_grid_shift())
}


/// The one-halfword network-warehouse deposit grade patch, `(address, bytes)`, while
/// the site still reads its original `cmp r3,#3`. `None` once patched (idempotent) or
/// on a build without the located site — the same contract as [`hud_bottom_patch`].
/// Lets 영웅-이상 (gold/white cash) equipment into the network warehouse; see
/// `guest_anchors::warehouse_grade_gate` for the full RE and the naming error string.
pub fn warehouse_grade_patch(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, [u8; 2])> {
    let (site, orig, patched) = crate::guest_build::warehouse_grade_gate()?;
    let cur = rd_u16(read, site)?;
    (cur == orig).then_some((site, patched.to_le_bytes()))
}

// ── may the host save right now? (see `crate::save`) ─────────────────────────

/// `Some((gameSaveName, slotSaveName))` when it is safe to run the save chain.
///
/// Every clause is the game's own condition, not a guess:
///
/// 1. **CHAR resolves.** Nothing to serialise before the character exists.
/// 2. **The AREA allows saving** — [`area_can_save`], i.e. `(s8)*(area+0x11d) != 0`,
///    which is what both of the game's own save paths test (guest 0x12c828 and
///    0x13c572) before answering "저장할 수 없는 지역입니다".
///
///    ★ Until 2026-08-09 this clause read `(s8)*(CHAR+0xa280) == 0` and it was wrong
///    twice over: wrong byte (that is the shared list widget's CURSOR ROW) and wrong
///    polarity. Because nothing ever resets that cursor, it sat non-zero for a whole
///    session and this gate refused nearly every autosave — device-measured on
///    2026-08-09, two autosaves in fifty minutes on a five-minute timer, with the
///    SAVE button and the minimap gone for thirteen straight minutes on an ordinary
///    field. The player found the giveaway before the disassembly did: opening any
///    menu and closing it brought both controls back, because opening a menu runs the
///    widget's constructor and that is the one thing that writes the cursor to 0.
///    See [`crate::save::AREA_CAN_SAVE_OFF`] for the full RE.
///
///    Unreadable (`None`) is treated as ALLOWED here, matching [`can_save_here`]: a
///    single unresolvable frame mid-map-change must not kill the save chain.
/// 3. **[`in_field_state`]** — real-time field free-roam owns the screen (no window
///    open, app state 4, selector overlay closed). The game's save runs from its own
///    save window; driving the primitives while another window is up would serialise a
///    state the player is in the middle of changing.
/// 4. **A sane slot number.** `writeDbRecord` formats the record name as `"%s_%d"` with
///    `*(CHAR+0xa3f8)`, so a wild value would create a record the loader never opens.
/// 5. **Both record-name pointers resolve** out of the guest's own `sl` table.
pub fn save_gate(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, u32)> {
    // Per-build table (MIGRATION_LGT.md §3.6): `None` = the chain is not ported to the
    // running build, so the primitives cannot be driven. This replaced the blanket
    // `guest_code_addresses_valid()` gate when LGT's chain landed (9차).
    let chain = crate::guest_build::save_chain()?;
    let c = resolve_char(read)?;
    if area_can_save(read) == Some(false) {
        return None;
    }
    if !in_field_state(read) {
        return None;
    }
    if rd_u32(read, c + chain.slot_off)? > crate::save::SAVE_SLOT_MAX {
        return None;
    }
    let (game, slot) = match chain.names {
        // KTF: the sl table holds POINTERS to the strings — read them.
        crate::guest_build::SaveNames::SlPtrs { game, slot } => (rd_u32(read, game)?, rd_u32(read, slot)?),
        // LGT: the strings' own `.text` addresses — nothing to read.
        crate::guest_build::SaveNames::Direct { game, slot } => (game, slot),
    };
    (crate::save::plausible_ptr(game) && crate::save::plausible_ptr(slot)).then_some((game, slot))
}

/// `(name pointer, slot)` for a host-driven recovery backup, or `None`.
///
/// Deliberately WEAKER than [`save_gate`]: that one refuses unless the game itself would
/// allow a save right now (field, `canSave`), because it is about to run the game's own
/// save chain. A backup only READS a record that is already on disk, so the only things
/// that must hold are "the slot number is real" and "the name pointer resolves".
pub fn backup_target(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, u8)> {
    // Guest CODE address: `backup::FN_LOAD_SAVE_RECORD` is still KTF-only — LGT's
    // `readSaveRecord 0x1a198` is a §2.13 CANDIDATE, not a verified contract, and the
    // record-body extraction (`backup.rs`) has not been checked against an LGT record.
    // The save chain moving to a per-build table does not move this one.
    if !crate::guest_build::guest_code_addresses_valid() {
        return None;
    }
    let chain = crate::guest_build::save_chain()?;
    let c = resolve_char(read)?;
    let slot = rd_u32(read, c + chain.slot_off)?;
    if u32::from(u8::try_from(slot).ok()?) > crate::backup::SLOT_MAX {
        return None;
    }
    let game = match chain.names {
        crate::guest_build::SaveNames::SlPtrs { game, .. } => rd_u32(read, game)?,
        crate::guest_build::SaveNames::Direct { game, .. } => game,
    };
    crate::save::plausible_ptr(game).then_some((game, slot as u8))
}

/// The 32-bit out-parameter the serialise step wrote (the payload length).
pub fn save_scratch(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u32> {
    rd_u32(read, crate::guest_build::save_chain()?.scratch)
}

/// The game's own AREA permission ALONE: is this a place the game would save in?
/// Deliberately ignores everything else [`save_gate`] checks (open windows, slot
/// sanity) — those are about "this exact moment", while this is about the PLACE.
/// A zero at `area+0x11d` is what makes the game answer "저장할 수 없는 지역입니다".
///
/// `true` when the chain does not resolve: boot/teardown is not a no-save AREA, and
/// the field HUD this feeds is gated on a live field elsewhere.
pub fn can_save_here(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    // Fail OPEN: an unreadable chain (boot, teardown, mid-map-change) is not a
    // no-save area, and answering "no" there is what made this feature erase two
    // controls for a whole session. See [`area_can_save`].
    area_can_save(read).unwrap_or(true)
}

// ── did a skill cast ACTUALLY start? ─────────────────────────────────────────
//
// From the wf_9fcbeeaf skill-dispatch RE (SKILL_BUTTON_FEASIBILITY.md, CONFIRMED
// items): the field quickslot dispatcher's skill branch (guest 0x1430dc) applies
// no effect itself — passing its gates ARMS the real-time combo state machine:
// it records the action CATEGORY in `CHAR+0x2c9` (the weapon-anim switch, values
// 0xa/0xb/0xc/0xd) and later frames consume it. The action-state gate 0x140024
// reads `CHAR+0x9bb5` and treats the top-bit modes (0x80/0x90) as "busy". A tap
// the gates REFUSE (not idle, no SP, no target in range) writes neither, so the
// PAIR — busy mode AND a skill category — is what distinguishes "the cast is
// running" from "the tap was swallowed". The cooldown watcher keys off this (or
// off SP dropping, whichever shows first) so a refused cast never starts one.

/// `CHAR`-relative `u8`: the action-state mode byte; `& 0x80` covers the 0x80/0x90
/// busy modes the game's own idle gate (guest 0x140024) tests.
const ACTION_MODE_OFF: u32 = 0x9bb5;
/// `CHAR`-relative `u8`: the action category the armed cast records — 0xa..=0xd
/// are the four skill weapon-anim switches (guest 0x1430dc).
const ACTION_CAT_OFF: u32 = 0x2c9;

/// Whether the combo state machine is running a SKILL action right now.
pub fn skill_cast_active(read: &impl Fn(u32, usize) -> Vec<u8>) -> bool {
    // ⛔ hero3-measured offsets — see `guest_build::char_record_is_hero3`.
    if !crate::guest_build::char_record_is_hero3() {
        return false;
    }
    let Some(c) = resolve_char(read) else { return false };
    let busy = rd_u8(read, c + ACTION_MODE_OFF).is_some_and(|m| m & 0x80 != 0);
    busy && matches!(rd_u8(read, c + ACTION_CAT_OFF), Some(0x0a..=0x0d))
}

// ── cash-shop buffs the HOST owns (see `crate::buffs`) ───────────────────────
//
// Three of the original cash items had no client-side effect (their `useItem` index
// misses the jump table's `cmp #0x10` bound at guest 0x153f02 and lands on the shared
// tail 0x154542, which only decrements the stack). The game still consumes them, so
// the host supplies the effect. Two guest code rewrites carry it (a halfword for the
// EXP divisor, a halfword or an LGT two-halfword pair for the equip check), all
// size-neutral and all written only after their ORIGINAL value is confirmed — the
// `hud_bottom_patch` contract.

// ── the equip-level check, on both builds ─────────────────────────────────────
//
// (This block documents the SITES; the patch itself is `equip_relief_patch` below,
// and which shape a build takes is `guest_build::equip_relief_site()`.)
//
// KTF: `adds r1, r3, #0` at guest 0x13d0aa — the move of the item's REQUIRED LEVEL
// into the register the equip check compares against.
//
// The check is one site in the whole image (the only caller of `showMessage(0x67)` =
// "장비를 장착할수 없습니다."):
// ```text
//   13d0a4  ldrb r3,[r3,#2]     ; required level, from the inventory record
//   13d0a8  asrs r3,r3,#0x18    ; sign-extended
//   13d0aa  adds r1,r3,#0       ; <-- we make this `subs r1,r3,#N`
//   13d0ac  ldrh r3,[r4]        ; (kills r3, so the subtract costs nothing)
//   13d0b6  cmp  r2, r3         ; playerLevel vs r1
//   13d0b8  bge  pass
// ```
// Patching the MOVE rather than the branch is what makes this a "-N levels" item
// instead of "ignore the requirement": everything below it runs unchanged, so the
// condition becomes `playerLevel >= requiredLevel - N`.
//
// `r1` is dead after 0x13d0b2 and `r3` is overwritten at 0x13d0ac, so the rewrite has
// no other effect; `subs` sets flags but `cmp` re-sets them before the branch.
//
// ★ NOT YET LIVE ON LGT, and the reason is worth writing down: the check itself IS
// located (2026-08-10, via the only `movs r0,#0x67` that feeds a `showMessage` call —
// LGT `showMessage` = `0x46b7c`, 12 callers, ours at `0x477dc`):
// ```text
//   477cc  adds  r4,#0xba
//   477ce  movs  r3,#0
//   477d0  ldrsh r1,[r4,r3]     ; player level
//   477d2  movs  r3,#2
//   477d4  ldrsb r3,[r2,r3]     ; item's required level (record+2)
//   477d6  cmp   r1,r3
//   477d8  bge   0x477e2        ; pass
//   477da  movs  r0,#0x67
//   477dc  bl    0x46b7c        ; "장비를 장착할수 없습니다."
// ```
// but LGT's codegen compares the freshly-loaded byte DIRECTLY — there is no
// `adds r1,r3,#0` move to turn into a `subs`, so the one-halfword trick has no
// analogue. Two four-byte rewrites are available instead, and WHICH ONE is not a
// matter of taste (7차, 2026-08-11):
//
//   * REQUIREMENT side, `0x477d2`: `movs r3,#2 ; ldrsb r3,[r2,r3]` ->
//     `ldrb r3,[r2,#2] ; subs r3,#N`. This is what §2.8 proposed and 6차 plumbed, and
//     it is REJECTED: it turns a SIGNED load into an unsigned one, so any item whose
//     requirement byte is >= 0x80 (a `-1` "no requirement" sentinel is exactly what a
//     C `signed char` invites) flips from allowed to blocked. Nothing had checked the
//     item data for such a value, and "the feature shows the wrong answer" is the
//     silent-bug class this migration keeps finding.
//   * LEVEL side, `0x477ce`: `movs r3,#0 ; ldrsh r1,[r4,r3]` ->
//     `ldrh r1,[r4,#0] ; adds r1,#N`. Shipped. `r4` is `CHAR+0xe2` (`LEVEL_OFF`) —
//     `0x477cc adds r4,#0xba` on top of the accessor §2.8 showed returns `CHAR+0x28` —
//     and the game caps level at `LEVEL_CAP` 70, so `ldrh` and `ldrsh` agree on every
//     value the field can hold. The rewrite changes the comparison and nothing else.
//
// Both sites' bytes were read back out of RUNNING guest memory before either decision
// (`wie_probe --poke`: `0x477ce` = `0023e15e`, `0x477d2` = `0223d356`, both exactly what
// the static disassembly predicted), and the poked run's frame hashes matched the clean
// run's. `guest_build::equip_relief_site()` says which shape a build takes.


/// `movs r1, #100`: the divisor of the kill-EXP formula in `addExpFromKill` (KTF guest
/// 0x14650c, whose single caller 0x120682 runs it for both party members and then calls
/// `checkLevelUp` 0x146ca8).
/// ```text
///   gain = (enemyLv*13 + 130) * enemy[0xb4] * (clamp(lvDiff,-9,9)*2 + 10) / 100
///   if (gain <= 0) gain = 1
///   CHAR[0xe4] += gain ;  CHAR[0xe4] += enemy[0xb4]
/// ```
/// Halving the divisor doubles `gain` exactly (100/50, 100/25 are integral), and the
/// level-up loop keeps running inside the game's own flow.
///
/// ⚠ Honest limit: the SECOND, flat `+= enemy[0xb4]` happens after the divide, so x2
/// is `2*gain + flat`, not `2*(gain + flat)`.
///
/// The ADDRESS is per-build (KTF 0x14654e / LGT 0x4d3cc) and lives in
/// `guest_build::exp_divisor_site()`; the formula above is the game's, not the
/// compile's, so it stays here.

/// `Some((address, bytes))` when the equip-level-relief instruction(s) are not already
/// what `levels` asks for. `levels == 0` restores the shipped code, so a lapsed buff
/// un-patches itself. The byte payload is one halfword on KTF and two on LGT — see the
/// site notes above.
///
/// Not gated on `guest_code_addresses_valid` any more (same per-item split as
/// [`hud_bottom_patch`]): `guest_build::equip_relief_site()` answers "is this build's
/// site known AND verified", and each arm refuses unless what is on site is either the
/// shipped original or one of our own rewrites — a different revision is left alone.
pub fn equip_relief_patch(read: &impl Fn(u32, usize) -> Vec<u8>, levels: u8) -> Option<(u32, Vec<u8>)> {
    match crate::guest_build::equip_relief_site()? {
        crate::guest_build::EquipReliefSite::MoveHalfword { site } => {
            let want = crate::buffs::equip_relief_halfword(levels)?;
            let cur = rd_u16(read, site)?;
            let orig = crate::buffs::equip_relief_halfword(0)?;
            // `subs r1,r3,#imm3` with imm3 masked out == the imm3=0 form of `subs`.
            let is_ours = (cur & !(7 << 6)) == (0x1E00 | (3 << 3) | 1);
            if cur != orig && !is_ours {
                return None;
            }
            (cur != want).then(|| (site, want.to_le_bytes().to_vec()))
        }
        crate::guest_build::EquipReliefSite::LevelBias { site } => {
            let cur = [rd_u16(read, site)?, rd_u16(read, site + 2)?];
            let want = equip_relief_bias_patch(cur, levels)?;
            Some((site, [want[0].to_le_bytes(), want[1].to_le_bytes()].concat()))
        }
    }
}

/// The pure half of the `LevelBias` arm: given the two halfwords currently on site,
/// the pair to WRITE, or `None` when nothing should be written (already right, or not
/// code we recognise). Split out so the decision is unit-testable without a guest.
fn equip_relief_bias_patch(cur: [u16; 2], levels: u8) -> Option<[u16; 2]> {
    let want = crate::buffs::equip_relief_level_bias(levels)?;
    let orig = crate::buffs::equip_relief_level_bias(0)?;
    // Ours: the exact `ldrh r1,[r4,#0]` followed by any `adds r1,#imm8`.
    let is_ours = cur[0] == 0x8821 && (cur[1] & 0xff00) == 0x3100;
    if cur != orig && !is_ours {
        return None;
    }
    (cur != want).then_some(want)
}

/// `Some((address, bytes))` when the EXP divisor is not already `100 / mult`.
/// `mult == 1` restores the shipped `movs r1,#100`.
///
/// Refuses unless the current halfword is a `movs r1,#imm8` whose imm8 divides 100
/// exactly — i.e. either the original or one of ours.
pub fn exp_multiplier_patch(read: &impl Fn(u32, usize) -> Vec<u8>, mult: u8) -> Option<(u32, [u8; 2])> {
    // Per-build site (`None` where the routine has not been located), then the same
    // shape check as before: only a `movs r1,#imm8` whose immediate divides 100 is ever
    // overwritten, so a revision whose codegen differs is left alone.
    let site = crate::guest_build::exp_divisor_site()?;
    let want = crate::buffs::exp_divisor_halfword(mult)?;
    let cur = rd_u16(read, site)?;
    let imm = cur & 0xff;
    if (cur & 0xff00) != 0x2100 || imm == 0 || 100 % u32::from(imm) != 0 {
        return None;
    }
    (cur != want).then_some((site, want.to_le_bytes()))
}

// ── "the player just confirmed 사용하기 on a buff item" ───────────────────────
//
// The inventory cursor, as `popup_choose` itself reads it (guest 0x13c99e/0x13c9b0):
//   tab  = u8 @ CHAR+0x9c71
//   slot = u8 @ CHAR+0x9c72 + tab      (a per-tab cursor array)
// and those are exactly the two arguments it hands to `useItem` (guest 0x13ce22), so
// the record they address is the item about to be consumed.
const INV_TAB_OFF: u32 = 0x9c71;
const INV_CURSOR_BASE: u32 = 0x9c72;

/// `사용하기` + NUL, from the game's own `dat/InGame_txt` id 0x21.
///
/// ★ The gate keys on the LABEL, not on the popup tag. The popup these items get is
/// built by guest 0x12d0ac, which writes a DYNAMIC tag (`*(r7-0x2d)`) and picks row
/// 0's text at 0x12d12a — id 0x21 `사용하기` for every item except 0x0b, else 0x20
/// `상세보기`. A tag test would therefore be guessing; the label is the thing that
/// actually tells us the highlighted row will run `useItem`.
const LABEL_USE: [u8; 9] = [0xbb, 0xe7, 0xbf, 0xeb, 0xc7, 0xcf, 0xb1, 0xe2, 0x00];

/// Is the player, RIGHT NOW, sitting on `사용하기` for a cash item? Returns that
/// item's `(category, index)`; the caller decides whether it is one the host
/// supplies an effect for (`buffs::grant_for` / `buffs::box_reward`).
///
/// Called from the worker the instant an OK keydown arrives, before it is forwarded —
/// so the read is never stale. (This is the same lesson as 17차's `PopupConfirm`: the
/// UI thread's copy of popup state is up to ~80 ms old, and here the answer decides
/// whether a week-long buff starts.)
///
/// `None` for every other popup and row. The host then forwards OK unchanged either
/// way — the GAME consumes the item, we only watch.
pub fn cash_use_pending(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u16, u16)> {
    // ⛔ hero3-measured offsets — see `guest_build::char_record_is_hero3`.
    if !crate::guest_build::char_record_is_hero3() {
        return None;
    }
    let c = resolve_char(read)?;
    // A menu overlay owns the screen (same byte, and same reason, as `popup_view`
    // gate 1: the window id is not loaded yet when the game tests this).
    if rd_u8(read, c + SELECTOR_FLAG_OFF)? == 0 {
        return None;
    }
    // The ITEM page, not the skill page.
    if rd_u8(read, c + MENU_PAGE_OFF)? != PAGE_ITEM {
        return None;
    }
    // A popup is open, with a highlighted row inside its row count.
    if rd_u8(read, c + POPUP_OPEN_OFF)? == 0 {
        return None;
    }
    let rows = rd_i8(read, c + POPUP_COUNT_OFF)?;
    let sel = rd_i8(read, c + POPUP_SEL_OFF)?;
    if rows <= 0 || sel < 0 || sel >= rows {
        return None;
    }
    // That row really reads 사용하기. The label buffers are resolved through the HANDLE
    // array every time, never through the dangling `popup_text` cache.
    let handles = popup_handles(read)?;
    let label = popup_label_addr(read, handles, sel as u32)?;
    let got = read(label, LABEL_USE.len());
    if got.len() < LABEL_USE.len() || got[..] != LABEL_USE[..] {
        return None;
    }
    // …and the item under the cursor exists, with something left to spend.
    let tab = rd_u8(read, c + INV_TAB_OFF)?;
    let slot = rd_u8(read, c + INV_CURSOR_BASE + u32::from(tab))?;
    let (count, cat, idx) = item_record(read, c, tab, slot);
    if count == 0 {
        return None;
    }
    Some((cat, idx))
}

/// The two data writes of a FULL HP+SP RESTORE — exactly what the game's own
/// revive / full-restore sites do (guest 0x124458 / 0x1540cc / 0x1570c4:
/// `HP = *(CHAR+0xec); SP = 1000`), so this invents no new state. Used as one of
/// the random-box payouts.
///
/// `None` when the character does not read as live (same plausibility gate as the
/// HUD) — a restore written into a half-built CHAR would be a corruption, not a heal.
///
/// ── NOT gated on `guest_code_addresses_valid` since 2026-08-11 (7차) ──────────
/// It never needed to be, and keeping it there was the blanket gate hiding a case it
/// does not describe. This function calls nothing and touches no build-specific
/// address: it writes [`HP_OFF`] and [`SP_OFF`], the two offsets the HUD gauges read,
/// which are VERIFIED on both builds (MIGRATION_LGT.md §2.7 read the status screen's
/// own HP 167/167 and SP back at exactly these, and §2.12 re-read them live on the
/// shipped archive). The value written is the game's own (`*(CHAR+0xec)` for HP,
/// the `AddSP` clamp constant for SP), so it invents no state either.
///
/// What still protects it is the thing that always did: [`resolve_char`] refuses a
/// record that does not read as a live character (level `1..=70`, `hp <= hp_max`,
/// `sp <= 1000`), so there is no build on which this can write into a half-built or
/// mis-anchored record.
pub fn full_restore_writes(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<[(u32, Vec<u8>); 2]> {
    let c = resolve_char(read)?;
    let s = read_stats(read)?;
    // HP is a s16 in the guest; a plausible max already fits (HP_CAP < i16::MAX).
    let hp = s.hp_max.min(i16::MAX as u32) as u16;
    Some([
        (c + HP_OFF, hp.to_le_bytes().to_vec()),
        (c + SP_OFF, u32::from(SP_MAX).to_le_bytes().to_vec()),
    ])
}

/// The 천상의 영약's ONE data write: `exp (CHAR+0xe4) = expToNext (CHAR+0xe8) + 1`,
/// which is the exact state a kill that crosses the threshold leaves behind — the
/// caller then invokes the game's own `checkLevelUp` (`guest_build::level_up_fn`)
/// and everything downstream (points, HP/SP refill, the 70 cap, visuals) is game
/// code. Returns `(write_addr, bytes, current_level)`.
///
/// `None` when the character does not read as live (same plausibility gate as
/// [`full_restore_writes`]) or is already at the cap — at 70 `doLevelUp` would
/// clamp-and-pin anyway, but declining here lets the caller keep the pending grant
/// instead of burning it on a no-op.
pub fn level_elixir_writes(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, Vec<u8>, u16)> {
    let c = resolve_char(read)?;
    let s = read_stats(read)?;
    if s.level >= LEVEL_CAP {
        return None;
    }
    let exp = s.exp_next.saturating_add(1);
    Some((c + EXP_OFF, exp.to_le_bytes().to_vec(), s.level))
}

// ── 소켓확장 — the host-side write is GONE (44차) ────────────────────────────
//
// 38차 implemented this item here: walk the seven equip slots, take the first item with
// room, write `+0x3c6`. Every byte of that RE was right and it is not why it was
// removed. It was removed because the player's ask was about CHOOSING:
//
//   "네트워크 창고에서 소켓확장 아이템을 사용하면 **오브처럼** 소켓확장을 시킬 아이템을
//    선택할 수 있게 해야해"
//
// "the first equipped item with room" cannot express that, and no host UI could either
// without stacking a new picker on top of a guest modal. The game already owns a picker
// for exactly this shape of action — 오브결합 — so the item now travels through it:
// delivered into the network warehouse as category 0x10, targeted with the game's own
// "장비를 선택하세요" flow, resolved by the server's `_orb_combine`. The equip-slot walk,
// the `SocketExpand` plan struct and the socket offsets all went with it.
//
// The RE that made it possible is not lost: the instance offsets live in
// `MIGRATION_LGT.md`, and the wire-side socket bytes (rec[16]=total, rec[18]=filled)
// are what the server now edits instead.

// ── the key-'7' SECONDARY WEAPON: deleted (2026-09-01) ───────────────────────
//
// hero3's `7` was a secondary-weapon attack, and this section read equip slot 6 so the
// button could wear the equipped weapon's icon. hero2 has NEITHER: `7` is SKILL SLOT 3
// (`t/help.tdf[8]` "스킬: 1, 3, 7, 9"), and there is no secondary-weapon equipment slot
// to read — the shop sells 무기 / 방어구 / 기타 and the off-hand is a 방패
// (`t/shop.tdf[4..16]`). So the control, its face and `secondary_weapon_icon` are gone,
// along with the three `EQUIP6_*` offsets they were the only readers of.
//
// `ITEM_FRAME_MAX` stays: `icon_frame` — which every quickbar ITEM icon goes through —
// range-checks against it.

/// item_bm has 29 frames (0..28); anything else means we misread the record.
const ITEM_FRAME_MAX: u16 = 28;

/// The item_bm frame the game itself would draw for `(category, db index)` —
/// mirrors its icon draw at guest 0x1533a8:
/// ```text
///   if (cat == 0x12) frame = (s8)itemDB[idx].icon      // consumables only
///   else             frame = cat                        // one icon per category
///   if (cat == 0x0e && idx == 3) frame = 0x1c           // one special case
/// ```
/// ★ This is what the first version of the key-'7' button got wrong: it ran the
/// CONSUMABLE lookup for a weapon, so it indexed the consumable DB with a weapon's
/// id and blitted whatever byte happened to be there ("the icon is wrong",
/// 2026-07-30). Only category 0x12 goes through that table.
fn icon_frame(read: &impl Fn(u32, usize) -> Vec<u8>, char_ptr: u32, cat: u16, idx: u16) -> Option<u16> {
    let frame = if cat == CONSUMABLE_CAT {
        item_db_icon(read, char_ptr, idx)?
    } else if cat == 0x0e && idx == 3 {
        0x1c
    } else {
        cat
    };
    (frame <= ITEM_FRAME_MAX).then_some(frame)
}

/// Diagnostic snapshot for a periodic logcat line: (CHAR ptr, field?, window id,
/// per-slot type bytes, app state). Aids device validation without a UI.
/// ★ Also returns the three bytes the HUD's whole mode decision is made of, because
/// "메뉴를 열면 우측 퀵바가 사라진다" (player, 2026-08-04) cannot be resolved by reading
/// the code — every candidate needs a value:
///
/// * **app state** `CHAR+state_off()` (KTF `0x7c` / LGT `0x78`, §2.7) — `in_game_state`
///   is `state == 4` and that is what decides whether the slot list is published at
///   all. This helper reads the per-build offset, not a hardcoded `0x7c`: on an LGT
///   boot a maintainer must look at `0x78`.
/// * **window id** `CHAR+0xa0c0` — `cutscene_open` is `window == 4`, and a menu taken for
///   the SCRIPT window makes `quick_buttons` return nothing by design.
/// * **selector / menu-overlay flag** `CHAR+0x9c6c` — `in_field_state` demands it be 0.
///   17차's RE is the reason to log it next to the window id: `0x124208` branches on THIS
///   byte before it ever reads the window id, so **a menu can be up while the window id
///   still reads 3 (= "field")**. The two bytes disagree, and which one the host trusts
///   decides the mode.
pub fn debug_probe(read: &impl Fn(u32, usize) -> Vec<u8>) -> (Option<u32>, bool, Option<u8>, [u8; 5], Option<u32>, Option<u8>) {
    let char_ptr = resolve_char(read);
    let field = in_field_state(read);
    let mut types = [0u8; 5];
    if let Some(c) = char_ptr {
        for s in 0..QB_SLOTS as usize {
            types[s] = rd_u8(read, c + QB_TYPE_OFF + s as u32).unwrap_or(0);
        }
    }
    // Per-build, and no longer CHAR-relative by assumption (`app_state_word`).
    let state = app_state_word(read);
    let selector = crate::guest_build::field_gate().and_then(|g| char_ptr.and_then(|c| rd_u8(read, c + g.selector_off)));
    (char_ptr, field, window_id(read), types, state, selector)
}

/// Every CHAR offset this crate reads that is **not yet verified on the running
/// build**, on one line, next to the offsets that ARE verified.
///
/// ── WHY THIS EXISTS (MIGRATION_LGT.md §2.11) ─────────────────────────────────
/// The LGT port has a class of risk the gates cannot cover: reads that are switched
/// ON with offsets lifted from KTF. If one of those moved — and two provably did,
/// the state byte `+0x7c`→`+0x78` and the HUD's `+0xa384`→`+0xa370` — the feature
/// does not disappear, it shows a WRONG value. §2.11 lists five such rows
/// (quickbar / badges / no-save area / skill cooldown).
///
/// `wie_probe --verify-offsets` prints the same numbers, but the probe cannot reach
/// the states that make them informative: a level-up (badges are 0 at level 1), a
/// bound quick slot (nothing is bound in a fresh game), an
/// open item popup. A human playing on the device reaches all of them in minutes, so
/// this is the same audit taken to where the evidence is. Compare each field with
/// what is on screen; a REF block that matches while a 🔶 field does not is exactly
/// a moved offset.
///
/// The popup cluster is here for a second reason: it is the whole trigger path for
/// every cash buff. Until 2026-08-11 [`cash_use_pending`] resolved the label buffers
/// through KTF's `sl`-relative slot, which on LGT is not even mapped (device-measured
/// `handles=None`), so no buff could start and the EXP x2/x4 and level-relief patches
/// were unreachable however correct their sites were. `handles` is now the build's own
/// anchor (`guest_build::popup_handles`), and `label` is the nine bytes at the
/// highlighted row — `bbe7bfebc7cfb1e200` is `사용하기`, the string that has to be
/// there before a buff is granted. A resolving `handles` with a readable `label` while
/// an item popup is open is what says the port is live.
///
/// Read-only and non-panicking: every access goes through the bounds-checked `rd_*`
/// helpers, so an unmapped address yields `None` rather than a fault.
pub fn offset_audit(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<String> {
    // ★ hero3-shaped audit: every offset below it (`0xa0c0`, `0x9c6c`, `0x252`, `0x9e28`,
    // the popup block …) was measured on hero3. `field_gate()` is the flag for "this build
    // has that record shape", so on hero2 the audit declines instead of printing numbers
    // that look measured. hero2's own gauges are checked with `wie_probe --gauges`.
    crate::guest_build::field_gate()?;
    let c = resolve_char(read)?;
    let mut s = format!("offset audit ({}) CHAR={c:#x}", crate::guest_build::name());
    // VERIFIED reference block: if these do not match the status screen the base is
    // wrong and nothing after it means anything.
    s += &format!(
        " | REF lv={:?} exp={:?}/{:?} hp={:?}/{:?} sp={:?} win={:?} state={:?} sel={:?}",
        rd_u16(read, c + LEVEL_OFF),
        rd_u32(read, c + EXP_OFF),
        rd_u32(read, c + EXP_NEXT_OFF),
        rd_u16(read, c + HP_OFF),
        rd_u32(read, c + HP_MAX_OFF),
        rd_u32(read, c + SP_OFF),
        rd_u8(read, c + WINDOW_ID_OFF),
        rd_u32(read, c + crate::guest_build::state_off()),
        rd_u8(read, c + SELECTOR_FLAG_OFF),
    );
    // 🔶 #6 badges — compare with the two "+" badges on the status screen.
    s += &format!(
        " | #6 badge stat={:?} skill={:?}",
        rd_u16(read, c + STAT_POINTS_OFF).map(|v| v as i16),
        rd_u8(read, c + SKILL_POINTS_OFF),
    );
    // 🔶 #5 quickbar — compare with the five cells the overlay draws.
    for slot in 0..QB_SLOTS {
        s += &format!(
            " | #5 qb{slot} t={:?} i={:?} s={:?} cell={:?}",
            rd_u8(read, c + QB_TYPE_OFF + slot),
            rd_u8(read, c + QB_ITEMREF_OFF + slot),
            rd_u8(read, c + QB_SKILLREF_OFF + slot),
            rd_i8(read, c + QB_SKILLIDX_OFF + slot),
        );
    }
    // 🔶 #5b on-field sheet ids — the byte `skill_icon` gates the atlas on (4..=10,
    // atlas index = value-4; anything else draws the empty box of §2.18-(2)). The
    // OFFSET is closed on LGT by static RE (loader fn 0x4957c reads
    // (CHAR+0x28)+0x8c+0x1d+i = CHAR+0xd1+i, same as KTF); this line closes the VALUE.
    s += &format!(
        " | #5b sheet={:?}/{:?}",
        rd_u8(read, c + SKILL_SHEET_OFF),
        rd_u8(read, c + SKILL_SHEET_OFF + 1),
    );
    // 🔶 #8 no-save AREA. The object itself is the one the minimap renders from, so a
    // resolving `area` means the container hop is right and only `+0x11d` is open.
    let area = resolve_area_base(read, c);
    s += &format!(
        " | #8 area={area:#x?} can_save={:?}",
        area.and_then(|a| rd_i8(read, a + crate::save::AREA_CAN_SAVE_OFF)),
    );
    // 🔶 #18 skill cooldown — mode's bit 7 plus a 0x0a..0x0d category is "casting".
    s += &format!(
        " | #18 cast mode={:?} cat={:?}",
        rd_u8(read, c + ACTION_MODE_OFF),
        rd_u8(read, c + ACTION_CAT_OFF),
    );
    // #17 popup cluster — the gate every cash buff has to pass. `label` is the 9 bytes
    // at the highlighted row; `bbe7bfebc7cfb1e200` is 사용하기 (`LABEL_USE`).
    let handles = popup_handles(read);
    s += &format!(
        " | #17 pop page={:?} open={:?} rows={:?} sel={:?} tag={:?} tab={:?} handles={handles:#x?}",
        rd_u8(read, c + MENU_PAGE_OFF),
        rd_u8(read, c + POPUP_OPEN_OFF),
        rd_i8(read, c + POPUP_COUNT_OFF),
        rd_i8(read, c + POPUP_SEL_OFF),
        rd_i8(read, c + POPUP_TAG_OFF),
        rd_u8(read, c + INV_TAB_OFF),
    );
    if let (Some(h), Some(sel)) = (handles, rd_i8(read, c + POPUP_SEL_OFF)) {
        let label = (sel >= 0)
            .then(|| popup_label_addr(read, h, sel as u32))
            .flatten()
            .map(|a| read(a, LABEL_USE.len()));
        s += &format!(" label={:02x?}", label.as_deref());
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn reader(map: HashMap<u32, u8>) -> impl Fn(u32, usize) -> Vec<u8> {
        move |addr, len| {
            let mut v = Vec::with_capacity(len);
            for i in 0..len as u32 {
                match map.get(&(addr + i)) {
                    Some(b) => v.push(*b),
                    None => return Vec::new(),
                }
            }
            v
        }
    }
    fn put_u16(m: &mut HashMap<u32, u8>, a: u32, v: u16) {
        for (i, b) in v.to_le_bytes().into_iter().enumerate() {
            m.insert(a + i as u32, b);
        }
    }
    fn put_u32(m: &mut HashMap<u32, u8>, a: u32, v: u32) {
        for (i, b) in v.to_le_bytes().into_iter().enumerate() {
            m.insert(a + i as u32, b);
        }
    }

    // The KTF pair, which is the build these tests exercise; the live values are
    // `guest_build::char_anchor()`.
    const CHAR_PTR_GLOBAL: u32 = 0x1b_3084;
    const CHAR: u32 = 0x1b_7b6c;
    const HANDLE: u32 = 0x4020_0000;
    const BLOCK: u32 = 0x4021_0000;
    /// KTF's container payload offset (`*(handle) + 8`) — these tests set the KTF
    /// build, so `deref_container` takes that arm. See `guest_build::container_data_off`.
    const KTF_DATA_ADJ: u32 = 8;
    fn gbase() -> u32 {
        BLOCK + KTF_DATA_ADJ
    }

    /// KTF's "free-roaming field" state value — what these tests used to spell
    /// `STATE_FIELD`.
    ///
    /// ⛔ ASKED OF THE BUILD TABLE, never re-declared here (2026-09-20). The constant
    /// and its sibling `WIN_NONE` moved into `guest_anchors::{AppState, FieldGate}` on
    /// 2026-09-09 because hero2's state word is a different offset with different values
    /// — and this module kept referring to the names, which is why `cargo test -p
    /// wie_android` has not compiled since (taking `overlay::ring_layout_tests` and
    /// `over_premul_tests` with it: nothing in this crate's test target could run).
    /// A local copy would compile and then drift, which is the trap the note at the top
    /// of this file describes.
    fn state_field() -> u32 {
        crate::guest_build::set_ktf();
        crate::guest_build::app_state().expect("the KTF build has an app state").field()
    }

    /// KTF's "no window owns the screen" id — what these tests used to spell `WIN_NONE`.
    fn win_none() -> u8 {
        crate::guest_build::set_ktf();
        crate::guest_build::field_gate().expect("the KTF build has a field gate").window_none
    }

    /// Seed the new-save quickbar: slot0=skill, slot1=skill, slot2=item(tab0,slot5),
    /// slots3,4=empty; item stack count 13 in the inventory grid.
    fn seed(m: &mut HashMap<u32, u8>, state: u32) {
        // `resolve_char` asks which carrier booted; nothing sets that in a unit test.
        crate::guest_build::set_ktf();
        put_u32(m, CHAR_PTR_GLOBAL, CHAR);
        put_u32(m, CHAR + crate::guest_build::state_off(), state);
        m.insert(CHAR + SELECTOR_FLAG_OFF, 0);
        // No window open = the field owns the screen (see WINDOW_ID_OFF).
        m.insert(CHAR + WINDOW_ID_OFF, win_none());
        // types
        m.insert(CHAR + QB_TYPE_OFF, TY_SKILL);
        m.insert(CHAR + QB_TYPE_OFF + 1, TY_SKILL);
        m.insert(CHAR + QB_TYPE_OFF + 2, TY_ITEM);
        m.insert(CHAR + QB_TYPE_OFF + 3, 0);
        m.insert(CHAR + QB_TYPE_OFF + 4, 0);
        // refs
        m.insert(CHAR + QB_SKILLREF_OFF, 0x6a); // skill id 0x6a&0x1f = 0x0a
        m.insert(CHAR + QB_SKILLREF_OFF + 1, 0x2d);
        m.insert(CHAR + QB_ITEMREF_OFF + 2, 5); // tab0 slot5
        // inventory grid for count
        put_u32(m, CHAR + GRID_HANDLE_OFF, HANDLE);
        put_u32(m, HANDLE, BLOCK);
        put_u16(m, gbase() + 0 * TAB_STRIDE + 5 * SLOT_STRIDE + COUNT_OFF, 13);
    }

    #[test]
    fn scans_five_slots_with_kinds_and_count() {
        let mut m = HashMap::new();
        seed(&mut m, state_field());
        let qb = scan_quickbar(&reader(m));
        assert_eq!(qb.len(), 5);
        assert_eq!(qb[0].kind, SlotKind::Skill);
        assert_eq!(qb[1].kind, SlotKind::Skill);
        assert_eq!(qb[2].kind, SlotKind::Item);
        assert_eq!(qb[2].count, 13);
        assert_eq!(qb[3].kind, SlotKind::Empty);
        assert_eq!(qb[4].kind, SlotKind::Empty);
        assert_eq!((qb[0].slot, qb[2].slot), (0, 2));
    }

    #[test]
    fn hotkey_map_matches_re() {
        // ★ hero2's table, not hero3's (2026-09-20). `quickslot_hotkey` is one function
        // for the app that booted, and this fork's app is hero2: its four guest slots are
        // `1 3 7 9` (`t/help.tdf[8]`, HERO2_GUEST_RE.md), `0` is the recovery-item key and
        // not a slot, and `#` is its long press — neither is a quickbar seat. The
        // assertions below were hero3's `1 3 9 0 #` and had not run since 2026-09-09.
        use wie_backend::KeyCode;
        assert_eq!(crate::quickslot_hotkey(0), Some((KeyCode::NUM1, '1')));
        assert_eq!(crate::quickslot_hotkey(1), Some((KeyCode::NUM3, '3')));
        assert_eq!(crate::quickslot_hotkey(2), Some((KeyCode::NUM7, '7')));
        assert_eq!(crate::quickslot_hotkey(3), Some((KeyCode::NUM9, '9')));
        // Every seat past the guest's four is a HOST plate: no key routes to it.
        for slot in crate::GUEST_QUICK_SLOTS..crate::QUICK_SLOTS {
            assert_eq!(crate::quickslot_hotkey(slot), None, "host plate {slot} has no guest key");
        }
    }

    #[test]
    fn field_gate_requires_no_window_state4_and_clear_selector() {
        let mut ok = HashMap::new();
        seed(&mut ok, state_field());
        assert!(in_field_state(&reader(ok)));

        let mut menu = HashMap::new();
        seed(&mut menu, 3);
        assert!(!in_field_state(&reader(menu)));

        // A window (map / inventory / save / cutscene) leaves CHAR+0x7c at 4, so the
        // window id is the only thing that catches it.
        for win in [WIN_MAP, 4, 14, 15, 16] {
            let mut m = HashMap::new();
            seed(&mut m, state_field());
            m.insert(CHAR + WINDOW_ID_OFF, win);
            assert!(!in_field_state(&reader(m.clone())), "window {win} is not the field");
            assert_eq!(window_id(&reader(m)), Some(win));
        }
    }

    #[test]
    fn gauges_use_the_corrected_hp_sp_offsets() {
        // Regression for the swap that made the red bar ignore HP potions: HP is the
        // s16 at CHAR+0xf2 over the s32 max at CHAR+0xec; SP is CHAR+0xf8 over 1000.
        let mut m = HashMap::new();
        seed(&mut m, state_field());
        put_u16(&mut m, CHAR + LEVEL_OFF, 12);
        put_u32(&mut m, CHAR + EXP_OFF, 1234);
        put_u32(&mut m, CHAR + EXP_NEXT_OFF, 15000);
        put_u32(&mut m, CHAR + HP_MAX_OFF, 632); // ComputeMaxHP(level 12, vit 20)
        put_u16(&mut m, CHAR + HP_OFF, 632); // full
        put_u32(&mut m, CHAR + SP_OFF, 400);
        // 19차 added the badge fields to `read_stats`, and a map that omits them
        // reads as "not a character" (`rd_u16` -> None). This test could never run
        // on the Windows host (the test binary dies at startup there), so the gap
        // only surfaced when `cargo test` first ran on a Linux host (6차).
        put_u16(&mut m, CHAR + STAT_POINTS_OFF, 3);
        m.insert(CHAR + SKILL_POINTS_OFF, 1);
        let s = read_stats(&reader(m)).expect("plausible");
        assert_eq!((s.hp, s.hp_max), (632, 632), "full HP must read full");
        assert_eq!((s.sp, s.sp_max), (400, SP_MAX));
        assert_eq!((s.stat_points, s.skill_points), (3, 1));
    }

    #[test]
    fn no_char_yields_empty() {
        crate::guest_build::set_ktf();
        let mut m = HashMap::new();
        put_u32(&mut m, CHAR_PTR_GLOBAL, 0x10); // out-of-heap
        assert!(scan_quickbar(&reader(m)).is_empty());
    }

    // ── skill_icon = per-slot (on-field hero sheet, cell) — mirrors 0x14a51a ──
    #[test]
    fn skill_icon_uses_member_sheet_and_cell() {
        // Live SLOT3 ground truth: slot0 ref 0x6a idx 5, slot1 ref 0x2d idx 5;
        // hero0 sheet 7 (s7/atlas3), hero1 sheet 8 (s8/atlas4).
        let mut m = HashMap::new();
        m.insert(CHAR + QB_SKILLREF_OFF, 0x6a); // slot0 -> member 1
        m.insert(CHAR + QB_SKILLREF_OFF + 1, 0x2d); // slot1 -> member 0
        m.insert(CHAR + QB_SKILLIDX_OFF, 5); // slot0 cell 5
        m.insert(CHAR + QB_SKILLIDX_OFF + 1, 5); // slot1 cell 5
        m.insert(CHAR + SKILL_SHEET_OFF, 7); // member 0 sheet = s7
        m.insert(CHAR + SKILL_SHEET_OFF + 1, 8); // member 1 sheet = s8
        let r = reader(m);
        assert_eq!(skill_icon(&r, CHAR, 0), Some(IconRef::Skill(4, 5))); // s8 cell5 = 일심
        assert_eq!(skill_icon(&r, CHAR, 1), Some(IconRef::Skill(3, 5))); // s7 cell5 = 장탄
    }

    #[test]
    fn skill_icon_empty_cell_is_none() {
        let mut m = HashMap::new();
        m.insert(CHAR + QB_SKILLIDX_OFF, 0xff); // int8 = -1 => empty
        m.insert(CHAR + QB_SKILLREF_OFF, 0);
        m.insert(CHAR + SKILL_SHEET_OFF, 7);
        assert_eq!(skill_icon(&reader(m), CHAR, 0), None);
    }

    #[test]
    fn skill_icon_bad_sheet_is_none() {
        let mut m = HashMap::new();
        m.insert(CHAR + QB_SKILLIDX_OFF, 3);
        m.insert(CHAR + QB_SKILLREF_OFF, 0x40); // top bits nonzero -> second sheet
        m.insert(CHAR + SKILL_SHEET_OFF + 1, 0); // sheet 0 -> invalid (< 4)
        assert_eq!(skill_icon(&reader(m), CHAR, 0), None);
    }

    #[test]
    fn skill_icon_clamps_to_two_onfield_sheets() {
        // The game tests `(ref as i8) >> 6 != 0`, so refs with top bits 2 or 3
        // still resolve to the SECOND sheet (CHAR+0xd2) — not 0xd3 / 0xd4.
        let mut m = HashMap::new();
        m.insert(CHAR + QB_SKILLIDX_OFF, 2);
        m.insert(CHAR + QB_SKILLREF_OFF, 0xc5); // ref>>6 = 3 (would be member 3)
        m.insert(CHAR + SKILL_SHEET_OFF, 7);
        m.insert(CHAR + SKILL_SHEET_OFF + 1, 8);
        m.insert(CHAR + SKILL_SHEET_OFF + 3, 10); // must NOT be picked
        assert_eq!(skill_icon(&reader(m), CHAR, 0), Some(IconRef::Skill(4, 2)));
    }

    /// The `LevelBias` arm of the equip relief, exercised through its pure half.
    #[test]
    fn equip_relief_bias_patch_rewrites_only_code_it_recognises() {
        const ORIG: [u16; 2] = [0x2300, 0x5EE1]; // movs r3,#0 ; ldrsh r1,[r4,r3]
        const OURS5: [u16; 2] = [0x8821, 0x3105]; // ldrh r1,[r4,#0] ; adds r1,#5
        // The shipped pair takes the -5 rewrite; the rewrite restores on 0.
        assert_eq!(equip_relief_bias_patch(ORIG, 5), Some(OURS5));
        assert_eq!(equip_relief_bias_patch(OURS5, 0), Some(ORIG));
        // Already what is asked for -> no write (the idempotence emulator.rs relies
        // on to make this block a no-op for a player with no live buff).
        assert_eq!(equip_relief_bias_patch(ORIG, 0), None);
        assert_eq!(equip_relief_bias_patch(OURS5, 5), None);
        // Re-tiering without restoring first: ours -> ours.
        assert_eq!(equip_relief_bias_patch(OURS5, 3), Some([0x8821, 0x3103]));
        // Anything else on site is a different revision: refuse both directions. The
        // last two are the REQUIREMENT-side pair 7차 rejected — if a table ever aimed
        // this arm four bytes further on, it must refuse rather than write an `ldrb`
        // where the game has an `ldrsb`.
        for alien in [[0x2300u16, 0x5EE2], [0x2301, 0x5EE1], [0x8821, 0x3205], [0, 0], [0x2302, 0x56D3], [0x7893, 0x3B05]] {
            assert_eq!(equip_relief_bias_patch(alien, 5), None, "{alien:04x?}");
            assert_eq!(equip_relief_bias_patch(alien, 0), None, "{alien:04x?}");
        }
        // Out-of-bound levels are the encoder's refusal, passed through.
        assert_eq!(equip_relief_bias_patch(ORIG, 8), None);
    }
}

// ── minimap: RENDER IT OURSELVES from the map arrays ─────────────────────────
//
// ★ REPLACES the old "mirror the guest's offscreen framebuffer" pipeline entirely
// (2026-08-03). That pipeline could only ever produce a picture the GUEST had
// painted, and the guest paints its minimap buffer in exactly one place — the map
// window's ENTER handler. So keeping the panel current meant driving that window
// open and closed behind a frozen screen, which is the flicker the player reported
// ("방에 입장하면 깜빡하면서 미니맵이 떴다가 꺼진다"), and identifying "which
// buffer is the current map's" was guesswork that sometimes latched a picture of
// the field instead ("미니맵 자리에 실제 게임 맵이 스크린샷돼서 뜬다").
//
// None of that is necessary: the map window does not have any private data. It
// paints from the SAME two arrays the movement code walks on, which are live on
// every map at every moment. So the host can draw the identical image at any time,
// from RAM, with no window, no key presses and no screen freeze.
//
// ── the guest routine, decoded (static disasm 2026-08-03) ────────────────────
// `openWindow(8)` -> guest 0x12c938 -> **0x17ba44**:
//     MAP   = *(*(CHAR+0x9c90)) + 8      KTF indirect handle (the `+8` data offset
//                                        the inventory grid uses -- GRID_DATA_ADJ)
//     w,h   = (i32)MAP+0x48 / +0x4c      in TILES
//     fb    = MC_grpCreateOffScreenFrameBuffer(w*2, h*2)
//     FillRect(fb, 0,0, w*2,h*2, 0,0,0)  then for every pixel:
//         c = pixel_colour(px, py)                          (0x17b8e4)
//         FillRect(fb, px, py, 1,1, c>>16, c>>8 & 0xff, c & 0xff)
// `pixel_colour(px, py)` — 2x supersample with a corner blend (0x17b8e4):
//     tx,ty = px>>1, py>>1 ; base = tile_colour(tx, ty)
//     nx = px odd ? tx+1 : tx-1 ;  ny = py odd ? ty+1 : ty-1
//     c1 = tile_colour(nx, ty) ; c2 = tile_colour(tx, ny)   (-1 -> use base)
//     blend only when base differs from BOTH, and base != the global at 0x1a4bc4;
//     then each channel = (base + c1 + c2) / 3   (0x19f874 = signed divide)
// `tile_colour(tx, ty)` (0x17b874), out of bounds -> -1:
//     blocked = (attr[ty*w+tx] & 0x80) != 0                 (0x17a6f8)
//     c = blocked ? 0x5b5b5b : 0xd9d4be
//     if !(tx==0 && ty==0) && (i8)event[ty*w+tx] > 0 -> c = 0x225acc   (0x17a6b4)
// with `attr = *(*(MAP+0x60)) + 8` (u8[w*h]) and `event = *(*(MAP+0x68)) + 8`
// (i8[w*h]).
//
// ── why those arrays are safe to read at any time ────────────────────────────
// They are not map-window state: `0x17a6f8` has five callers and `0x17a6b4` six,
// and only one each belongs to the minimap — the rest are the field's own
// collision and warp checks, which run while the player walks. If these were ever
// stale the player would walk through walls.
//
// ── verified, not assumed ────────────────────────────────────────────────────
// `wie_probe --map-check` renders this and diffs it against the buffer the guest
// itself painted, on the real save: **0 of 5040 pixels differ** (70x72 = a 35x36
// tile map), and the render is already correct at a frame where the map window has
// never been opened (`last_offscreen_copy() == None`).

// The renderer itself, in a dependency-free file so `wie_probe --map-check` can
// `include!` the SHIPPING source and diff its output against the buffer the guest
// paints. Measured on the real save: 0 of 5040 pixels differ on the first map and
// 0 of 1224 on a second one reached by walking through a door — and both renders
// are already correct at frames where the map window has never been opened.
include!("minimap_render.rs");

/// The build-specific half of the map pointer chain, from the shared anchor table.
/// One line, because `minimap_render.rs` is `include!`d into two crates and so cannot
/// name `crate::guest_build` itself.
fn map_chain() -> MapChain {
    MapChain {
        data_off: crate::guest_build::container_data_off(),
        noblend_global: crate::guest_build::minimap_noblend_global(),
    }
}

/// The game's minimap, rendered from the live map arrays — pixel-identical to what
/// the guest's own map window would paint, and available without opening it.
/// `None` when the map object is not resolvable (title screen, mid-load).
/// Includes the player marker — the red 2x2 dot the game's own map window draws
/// over the same picture (player request 2026-08-03). Without it the panel is a
/// room outline with no "you are here", which is most of what a minimap is for.
pub fn render_minimap(read: &impl Fn(u32, usize) -> Vec<u8>, triggers: &mut crate::guest_build::H2TriggerCache) -> Option<crate::Minimap> {
    // ★★ hero2 draws from its OWN map object (2026-09-19). Everything below this block
    // walks hero3's `CHAR + 0x9c90` map chain, and on hero2 `resolve_char` SUCCEEDS —
    // it just returns a Java field block — so without this branch the renderer would
    // decode that block as a hero3 map header and paint a confident picture of nothing.
    // See `hero2java::h2_map` for how each hero2 field was measured.
    if let Some(sv) = crate::guest_build::hero2_save() {
        let h2 = crate::guest_build::hero2_java()?;
        let map = crate::guest_build::h2_map(read, &sv.area, h2.char_slot)?;
        // ★ 2026-09-29 (hero5-style panel): TILE CLASSES, not a picture — the overlay
        // draws a fixed-zoom window of them around the player. The exits (blue bars) and
        // the unopened treasure chests (green boxes) come from the map's own trigger
        // tables (`hero2java::h2_trigger_groups`), the chests' state from the event flags.
        let cells = crate::guest_build::h2_minimap_cells_live(read, &sv.area, &map, triggers);
        // ⛔ NO player in the cells (2026-09-19). hero2's dot is drawn live by the
        // overlay from `UserEvent::MinimapMarker`, and baking a second one in would
        // put a stale dot on the map that only catches up every `MM_RENDER_EVERY`.
        //
        // It also keeps this STABLE: with the dot baked in, every step changed it, so the
        // whole grid was re-sent to the UI thread ~4x/s for as long as the player kept
        // walking. Now it is sent when the map changes, which is what the comparison in
        // the worker was always meant to detect.
        return Some(crate::Minimap::Tiles {
            w: map.w as u16,
            h: map.h as u16,
            cells,
        });
    }
    let char_ptr = resolve_char(read)?;
    let chain = map_chain();
    let (w, h, mut px) = render_minimap_rgb(read, char_ptr, chain)?;
    if let Some(tile) = player_tile(read, char_ptr, i32::from(w) / 2, i32::from(h) / 2, chain) {
        stamp_player(&mut px, w, h, tile);
    }
    Some(crate::Minimap::Picture {
        w,
        h,
        // The panel wants opaque `0xAARRGGBB`; the guest's colours are `0x00RRGGBB`.
        px: px.into_iter().map(|c| 0xff00_0000 | c).collect(),
    })
}
