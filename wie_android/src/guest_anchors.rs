// Build-specific guest anchors — the DATA half of `guest_build.rs`.
//
// ── WHY THIS FILE EXISTS SEPARATELY ──────────────────────────────────────────
// It carries no `//!` docs, no `use`, and no dependencies, so it can be `include!`d
// into more than one crate:
//   * `wie_android/src/guest_build.rs` splices it in and adds the "which build
//     booted" atomic on top, so the app keeps calling `guest_build::char_anchor()`
//     exactly as before;
//   * `wie_probe/src/main.rs` splices it into a module of the same name.
// The probe used to carry a hard-coded COPY of the KTF numbers (`0x1b3084` /
// `0x1b7b6c` / `+0x7c`), which is why `--map-check`, `--char-dump`, `--find-fb` and
// `--trace-state` all read unrelated memory on an LGT boot (MIGRATION_LGT.md §3.4).
// A shared table is the fix: a copy drifts, and `items_selftest.rs` is this repo's
// standing proof of that.
//
// One thing the probe deliberately does NOT share: the "is a character alive yet"
// plausibility test for `Static` anchors. That test is made of CHAR-record offsets
// (level / hp / sp) which live in `items.rs`, and a diagnostic tool wants the raw
// base even when the record reads dead — that is precisely the state one debugs.

/// Which carrier build of the game is running. The two that matter here are
/// different COMPILES of the same game (MIGRATION_LGT.md §1): no byte-level overlap,
/// so a guest address lifted from one is meaningless in the other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuestBuild {
    /// KTF `client.bin` md5 `3792a476…`, Ver 01.00.03 — every guest CODE address in
    /// `wie_android` was reverse engineered from this one.
    Ktf,
    /// LGT `binary.mod` md5 `6a352944bb43c0789ca9f4b6f0702f4b` (Ver 01.00.07, 519,952 B
    /// — full hash measured 2026-09-04 from hero3's own shipping asset) or
    /// `e76fafdd…` (01.00.04, prefix only: no such archive on this disk).
    Lgt,
    /// SKT. Boots, but nothing here has been measured on it.
    Skt,
    /// ★★ **영웅서기2** KTF `client.bin1120` md5 `08a53cc43e57a1173df5d03121fa32ed`
    /// (PID PD004517 / AID 01035D0B / Ver 01.00.07 / 176x220).
    ///
    /// This is a DIFFERENT GAME, not another carrier build of the same one — and it is
    /// not even the same kind of binary: hero3 is a C image with zero symbols, hero2 is
    /// an **AOT-compiled Java MIDlet** whose class/field/method tables are still in the
    /// image (`HERO2_FEATURE_PARITY.md` §7). So nothing hero3-shaped can be reused, and
    /// every accessor below returns the CLOSED value for this variant except the three
    /// read-only data axes that were actually measured on it:
    /// [`char_anchor`](Self::char_anchor) + [`char_layout`](Self::char_layout) +
    /// [`app_state`](Self::app_state). Evidence = **`HERO2_GUEST_RE.md`**.
    ///
    /// ⛔ [`guest_code_addresses_valid`](crate::guest_build::guest_code_addresses_valid)
    /// stays FALSE here, so no code patch, no guest call and no guest write can fire.
    Hero2Ktf,
}

/// How to reach the character record on a given build.
///
/// The two carriers disagree about something more basic than an address: KTF
/// allocates the record and keeps a pointer to it in a global, LGT compiles the
/// record itself into `.bss` and never needs a pointer at all.
#[derive(Clone, Copy)]
pub enum CharAnchor {
    /// KTF `client.bin`: `*(sl+0x444)` = `0x1b3084` holds the record's address, with
    /// `0x1b7b6c` as the observed static fallback when that global reads wrong.
    Pointer { global: u32, fallback: u32 },
    /// LGT `binary.mod`: the record itself lives at this `.bss` address. Nothing to
    /// dereference, so "is a character loaded" has to be answered by the CONTENTS
    /// (see `items::resolve_char`) instead of by a pointer being non-null.
    ///
    /// Measured 2026-08-10 on Ver 01.00.07 — the STATUS screen's own numbers
    /// (HP 167/167, SP 100/100, LV 1, EXP 1/814) read back at the KTF offsets from
    /// this base, and `+0xa0c0` carried KTF's window ids (3 = none, 4 = script).
    Static(u32),
    /// ★★ hero2: the record is a **Java object**, so reaching it is two dereferences
    /// and the resolved base is the object's FIELD BLOCK, not the object.
    ///
    /// ```text
    /// slot (a Java `static` field's own storage, a fixed guest address)
    ///   -> *(slot)  = the object          (guest heap)
    ///        -> *(obj + 0) = ptr_fields   (guest heap)
    ///             -> field = ptr_fields + 4 + abs_off
    /// ```
    ///
    /// `resolve_char` therefore returns `ptr_fields + 4` and every offset below it is
    /// the ABSOLUTE offset from `HERO2_GUEST_RE.md` §2 (= ancestors' `fields_size` sum
    /// plus the declared offset — hero2 lays parent fields out first).
    ///
    /// Measured 2026-09-09: slot `0x193b5c` = class `ag`'s static `La; a`, holding an
    /// instance of `ay extends a extends g extends ad`. Seven of the game's own STATUS
    /// numbers read back from it (LV 48 · HP 777/780 · SP 390/390 · EXP 195/112128 ·
    /// ATK 138 · DEF 115 · CRIT 43%).
    JavaObject { slot: u32 },
}

/// Where the character record keeps the gauges the HUD draws, per build.
///
/// ── WHY THIS IS A TABLE NOW (2026-09-09) ────────────────────────────────────
/// It used to be a block of `const`s in `items.rs`, which was fine while every build
/// was the same GAME. hero2 is not: its record is a Java object with a different
/// layout, wider fields (HP/SP/EXP are all `int`), and — unlike hero3 — a real
/// `sp_max` FIELD instead of a fixed 1000 cap. Keeping hero3's constants and adding
/// "if hero2" at each read site is how a HUD ends up drawing one build's HP with
/// another build's stride.
///
/// Offsets are relative to whatever [`CharAnchor`] resolved to. `None` in `sp_max`
/// means "this build has no such field — use `sp_max_const`".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CharLayout {
    /// Level. Read as u16 LE; both builds store it in a field at least that wide.
    pub level: u32,
    /// Current HP. `hp_signed` says whether a dead character carries a negative here.
    pub hp: u32,
    /// ⚠ WIDTH of the current-HP field, in bytes. **hero3 stores it as a HALFWORD**
    /// (`CHAR+0xf2`, with an unrelated field at `+0xf4`), hero2 as an `int`. Reading a
    /// word at hero3's offset would splice the neighbour in as the high half — that is
    /// why this is a field and not an assumption.
    pub hp_bytes: usize,
    pub hp_max: u32,
    pub sp: u32,
    /// `Some(off)` = a real max-SP field; `None` = the build has none.
    pub sp_max: Option<u32>,
    /// Used when `sp_max` is `None`.
    pub sp_max_const: u16,
    pub exp: u32,
    pub exp_next: u32,
    /// Unspent stat / skill points, when the build's badges have been located.
    pub points: Option<(u32, u32)>,
    /// Width of the SKILL-point field: hero3 keeps a `u8` (`ldrb`), hero2 a Java
    /// `short` in a 4-byte slot. Reading hero2's low byte alone would wrap at 256.
    pub skill_points_bytes: usize,
    /// Upper bounds `items::plausible_stats` holds the two counts to — the game's own
    /// clamps where they are known (hero3: `addStatPoints` 276 / `addSkillPoints` 44).
    /// hero2's clamps are not located, so its bounds are the field widths: the other
    /// gates (level, HP) do the "is this a character yet" work there.
    pub point_caps: (u16, u16),
    /// hero3 reads current HP with `ldrsh` at its death gates, so a corpse holds a
    /// negative. hero2 stores HP in an `int` and the same rule applies, but the read
    /// width differs — this flag is what tells the reader to sign-extend.
    pub hp_signed: bool,
    /// Plausibility bounds for [`items::plausible_stats`]. A record that fails these
    /// is not (yet) a character, and the HUD keeps its previous copy.
    pub level_cap: u16,
    pub hp_cap: u32,
}

/// Where the coarse "which screen is the game on" word lives, and what its values mean.
///
/// hero3 keeps it INSIDE the character record; hero2 keeps it in a Java `static`, so
/// it is readable before any character exists. Two different questions that used to be
/// one `state_off()`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppState {
    /// hero3: `CHAR + off`. Needs [`CharAnchor`] resolved first.
    CharRelative {
        off: u32,
        field: u32,
        window: u32,
        /// Values that mean "out-of-world shell" (title / slot select / class pick).
        shell: &'static [u32],
    },
    /// ★★ hero2: an absolute static slot — no character needed.
    ///
    /// Measured 2026-09-09 by driving the probe from boot to an open window and reading
    /// the slot on the same frames the screenshots were taken:
    /// `0` = publisher logo, `2` = title, `1` = save-slot select, `3` = FIELD,
    /// `5` = a game window is open (field menu / STATUS).
    ///
    /// ★ `4` and `8` were answered by the first device run (S25, 2026-09-17): a save
    /// made mid-conversation loaded straight into `4` with the alchemist's line in the
    /// bottom dialogue box, and OK on it opened the 장비 능력 부여 service window at `8`.
    /// Both are "a character is in the world" (`in_world`), and `4` is the one screen
    /// the game draws BOTTOM-ANCHORED — the dialogue box sits in the last ~38 rows of
    /// whatever framebuffer it was given, so the `hud_reserved_rows` crop cut it in half
    /// on the device. `bottom_overlay` is what makes `App` scroll instead of crop there.
    ///
    /// ⛔ `HERO2_FEATURE_PARITY.md` §9.3 claimed hero2 used hero3's values
    /// (`3` = shell, `4` = field, `5` = selector). It does not — see `HERO2_GUEST_RE.md`
    /// §3. That claim is why this enum spells the values out per build.
    Absolute {
        addr: u32,
        field: u32,
        window: u32,
        shell: &'static [u32],
        /// Further values that still mean "a character is in the world", beyond
        /// `field` and `window` (hero2: `4` = NPC 대화, `8` = NPC 기능창).
        in_world: &'static [u32],
        /// Values on which the game paints a bottom-anchored overlay into the rows the
        /// host would otherwise crop away (hero2: `4`, the dialogue box).
        bottom_overlay: &'static [u32],
        /// Values that mean the SCRIPT system owns the screen — a story scene, NPC
        /// dialogue, a quest notice, or the ending (hero2: `4`, `12`). What hero3 reads from its window id
        /// (`items::WIN_SCRIPT`); the host's `Mode::Cutscene` follows from it, and
        /// whether a CHOICE is up inside it is a separate byte ([`Hero2Java::script_mode_slot`]).
        script: &'static [u32],
    },
}

impl AppState {
    pub fn field(self) -> u32 {
        match self {
            Self::CharRelative { field, .. } | Self::Absolute { field, .. } => field,
        }
    }

    pub fn window(self) -> u32 {
        match self {
            Self::CharRelative { window, .. } | Self::Absolute { window, .. } => window,
        }
    }

    pub fn shell(self) -> &'static [u32] {
        match self {
            Self::CharRelative { shell, .. } | Self::Absolute { shell, .. } => shell,
        }
    }

    /// "A character is in the world", whichever of its screens owns the display.
    pub fn in_game(self, v: u32) -> bool {
        v == self.field()
            || v == self.window()
            || match self {
                Self::CharRelative { .. } => false,
                Self::Absolute { in_world, .. } => in_world.contains(&v),
            }
    }

    /// Is the game drawing a bottom-anchored overlay on this state — one the
    /// `hud_reserved_rows` crop would cut? hero3 reserves nothing, so it never says yes.
    pub fn bottom_overlay(self, v: u32) -> bool {
        match self {
            Self::CharRelative { .. } => false,
            Self::Absolute { bottom_overlay, .. } => bottom_overlay.contains(&v),
        }
    }

    /// Is `v` a state the script system owns (see [`AppState::Absolute::script`])?
    /// `None` = this build answers the question from somewhere else (hero3: window id).
    pub fn is_script(self, v: u32) -> Option<bool> {
        match self {
            Self::CharRelative { .. } => None,
            Self::Absolute { script, .. } => Some(script.contains(&v)),
        }
    }

    pub fn is_shell(self, v: u32) -> bool {
        self.shell().contains(&v)
    }
}

/// The two extra bytes hero3 needs before calling the field "free-roaming": which of
/// the game's own windows owns the screen, and whether the quick-item selector overlay
/// is up. `None` = this build says it with the state word alone.
///
/// ★ hero2 is `None` and that is not a gap: its state word is already 3-vs-5 for
/// "field" vs "a window is open" (`HERO2_GUEST_RE.md` §3), so there is nothing left for
/// a second byte to add. hero3's word cannot do that — its `4` covers both.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FieldGate {
    pub window_id_off: u32,
    /// The window id that means "no window".
    pub window_none: u8,
    pub selector_off: u32,
}

/// How to reach `popup_handle[8]` — the array holding one allocation per row of the
/// game's context popup, whose row 0 text is what tells the host a `사용하기` is about
/// to run `useItem` on a cash item (`items::cash_use_pending`).
///
/// Same disagreement as [`CharAnchor`], for the same reason: KTF keeps the array
/// behind a data slot, LGT compiles the array itself into `.bss`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PopupHandles {
    /// KTF `client.bin`: the `sl`-relative data slot at `global` HOLDS the array's
    /// address. Read at runtime rather than hardcoding `0x1b7a2c` so a relocated
    /// build still resolves. [CONFIRMED `sl+0x1b4`.]
    Pointer { global: u32 },
    /// LGT `binary.mod`: the eight-word array IS at this `.bss` address.
    Static(u32),
}

/// How the host makes the GAME take the popup row it has just highlighted — the commit
/// half of the 퀵슬롯등록 rewrite.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PopupCommit {
    /// KTF: write the highlighted-row byte, then call the game's own popup key handler
    /// with its select key. `fn_addr` carries the Thumb bit.
    GuestCall { fn_addr: u32, select_key: u32 },
    /// LGT: write the highlighted-row byte and let an ORDINARY `OK` keypress reach the
    /// game — the identical path a finger on the real handset takes. No guest address is
    /// involved, so nothing here can be stale.
    ///
    /// ── MEASURED, NOT ASSUMED (2026-08-11, 8차) ──────────────────────────────────
    /// Two deterministic probe runs, identical except that one poked the row byte
    /// (`CHAR+0x9c86`) to 1 ten frames before the `OK`:
    ///   * untouched (row 0 = `사용하기`): the quickbar stayed all zeros and the potion
    ///     was consumed, i.e. the game ran row 0;
    ///   * poked to row 1 (`1키등록`): `CHAR+0x252` became **2** (kind = item) and
    ///     `CHAR+0x257` became **6** — inventory slot 6, exactly the potion the cursor
    ///     was on. The popup closed itself either way.
    /// So the game's own popup dispatcher reads that byte when the key arrives, and the
    /// whole bind (kind, ref, the existing-binding cleanup, the teardown) is its code.
    /// The same run also verified the quickbar offsets on LGT, which MIGRATION_LGT.md
    /// §2.11 had listed as a silent-bug watch item.
    ///
    /// ★ The caller must restore the NATIVE row count before the key arrives. Our
    /// relabel shrinks `CHAR+0x9c85` to 4, and a row byte above that could be clamped by
    /// the guest's own cursor logic in the tick between the write and the keypress —
    /// which is a hazard the KTF path does not have, because a direct call leaves no
    /// tick in between. See `items::popup_count_restore`.
    SyntheticOk,
}

/// ★★ hero2's WRITE gate (2026-09-17, HERO2_QUICKSLOT_TOUCH_PLAN.md §4.5) — one item per
/// kind of Java DATA the host may write. Deliberately separate from
/// `guest_code_addresses_valid()` (code patches, guest calls — still closed on hero2) and
/// from `quickbar_writes_ok()` (hero3's four parallel arrays): every write here is a
/// byte or a word into a named field of a live object, computed by a reader that has
/// just read the same bytes, and the game's own code does the rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JavaWrite {
    /// A window's `j.h` cursor byte (touch = "focus this row", 1 B, `< j.i`).
    WidgetCursor,
    /// The 10 EUC-KR bytes of a popup label (`1번에 등록` → `퀵슬롯등록`), same length.
    PopupLabel,
    /// The popup's row count, label-array length and box height (shrink / restore).
    PopupRows,
    /// `ag.j[slot]` — a quickbar byte (bind / unbind for the plate window).
    Quickbar,
    /// `ag.a[slot]` — a cooldown counter (set to READY alongside a raw bind).
    Cooldown,
    /// ★ S7 (2026-09-17 저녁): `bq.a:B` + `bq.c:B` — the inventory's selected recovery
    /// item (Vector index) and its ordinal in the game's own cycle table `bq.j`. The two
    /// bytes the `#` key writes; the `0` key then re-derives the index from the ordinal
    /// (SKT `ci.a()`), so both are written together, always to values the cycle could
    /// have produced itself.
    HealSelect,
    /// A `byte[]` HANDLE into a popup label array's slot BEYOND its native length — the
    /// slack of its 32-byte bucket (`byte[3][]` = 20 B, `byte[5][]` = 28 B, same class).
    /// The handle is one of the `/t/gm` table's own elements (`퀵슬롯등록` / `해제`), so
    /// the collector sees an object it already keeps alive.
    PopupLabelRef,
    /// ★ #13 (2026-09-21): `bq.b:B` — the INVENTORY's capacity byte, `28` from the
    /// constructor, raised to [`Hero2Java::BAG_CAPACITY`] by the host. The one field
    /// outside `bq` nobody reads (HERO2_GUEST_RE.md §2.7.5), and the save serialises
    /// records, not capacity, so a load comes back at 28 and the host writes it again.
    BagCapacity,
    /// ★ #13 step 5 (2026-09-21): the field menu's CHILD reference `j.b` (+0x4) — the
    /// handle of a window the host constructed (`JavaCall::InventoryWindowNew`), hung on
    /// the menu the way `bw.b()Z` hangs the game's own. The object is pinned until this
    /// write has landed.
    WindowChild,
    /// `bw.a:B` @0x19424c — the row the menu remembers (`bw.b()Z` writes `j.h` into it
    /// before it dispatches). Written with the ACCOUNT row so the host-opened window
    /// leaves the same state the game's own would.
    MenuLastRow,
    /// A `byte[]` HANDLE into a slot of the `/t/gm` table's own row array (`i.a:[Object`)
    /// — the 재료 가방 title row (57) is pointed at the 계정창고 row (19) while the ACCOUNT
    /// window is up, and put back when it closes. The table is rebuilt from the asset
    /// every time the menu opens, so nothing here outlives the menu.
    TableRowRef,
    /// A popup's LABEL ARRAY reference (`d` +0x2c) — pointed at a fresh `byte[][]` the
    /// game's own table made when the native array has no slack for the move row.
    PopupLabels,
    /// ★ #22 (2026-09-23): `a.c:S` and the five base abilities `a.e:S`..`a.i:S` — the
    /// 망각의 구 스탯 reset.
    ///
    /// ⛔ **This is a different kind of write from the eleven above it.** Those are UI
    /// data on a live widget: a cursor, a label, a row count — things the game rebuilds
    /// every time it opens a window, so a wrong one is a wrong pixel and the next frame
    /// fixes it. These are CHARACTER fields, and the game serialises them into the next
    /// save (`ag.l()V`). A wrong one is permanent. Everything written through this and
    /// [`Self::CharSkills`] / [`Self::BeastSkills`] must therefore be a value the game
    /// itself could have produced, computed from what was just read, in one batch.
    CharStats,
    /// ★ #22: `a.d:S` and the elements of `a.p:[B` — the 망각의 구 스킬 reset. Same
    /// warning as [`Self::CharStats`]. ⛔ Only the slots that read `> 0` are zeroed:
    /// `-1` means the class does not have that skill and writing 0 would open it.
    CharSkills,
    /// ★ #22: the elements of `bn.k:[B` — the beast's skill levels, zeroed by the same
    /// 망각의 구 스킬 (the user asked for 야수스킬까지 전부). Same warning as
    /// [`Self::CharStats`].
    BeastSkills,
    /// ★★★ 2026-09-23: `a.p[0]` and `a.h:B` — the REPAIR of a character whose 기본공격
    /// this host zeroed. Its own kind because it is the only write here that is not
    /// something the player asked for: it restores an invariant the game establishes at
    /// creation and never breaks (see [`Hero2Java::char_combo_open_off`]), and it is
    /// therefore the one write that may happen without a row being pressed. Same
    /// save-bearing warning as [`Self::CharStats`].
    CharCombo,
    /// ★ 2026-09-24 (user: "계정창고에 맨첫번째 칸을 재료가방으로 고정해야함"): the element
    /// slots of the ACCOUNT warehouse's `bq.a:Ljava/util/Vector;` — `elementData[0..=k]`
    /// rotated by one word each so the 재료가방 record (type 29) sits at cell 0. The
    /// handles already in the Vector, permuted and nothing else (`elementCount` and every
    /// reference stay as they were, so the collector sees the same live set), on a bag
    /// the host owns and only while no window shows it. Not save-bearing in the game's
    /// sense — the warehouse is the host's own two files — but it IS what `bq.a()[B`
    /// serialises next, so the order written here is the order that comes back.
    /// Why a write and not a call: `java/util/Vector.insertElementAt` is a runtime class,
    /// and `call_java_method` compares a `Handle` receiver against a GAME class record
    /// (`JavaClassDefinition::ptr_raw`) — a host class has none, so the call is refused
    /// by design. The game itself has no reorder method on `bq` (append and remove only).
    BagOrder,
    /// ★ 2026-09-24 (조이스틱 turn lag): the character's facing `g.u:B` (absolute +0x40),
    /// flipped to the exact opposite while it walks between two tiles — the value the
    /// game's own `ag.c()V` would write at the next tile edge, one frame early. Only
    /// through `hero2java::h2_reverse_write`, which proves `g.t == 2`, the current facing
    /// is the opposite, and the position is off-grid on that axis. The facing is the one
    /// thing the mover reads (`g.c(I)V` steps `8 × dir-table[g.u]` and re-files the actor
    /// on the occupancy grid itself), so nothing else has to agree with it.
    CharFacing,
    /// ★ 2026-09-24 (user: "아이템이 사라지는 시간을 대폭 늘려야해"): the lifetime byte e[5]
    /// of a ground drop (`bf.b` element, a `byte[6]`), refilled to the game's own spawn
    /// value 120 while it is between 17 and 40, up to `hero2java::H2_DROP_LIFE_TOTAL`
    /// ticks in all (`h2_drop_life_plan`). Stage state, never serialised.
    GroundDropLife,
    /// ★ 오토루팅 (2026-09-24): a ground drop's tile bytes e[0], e[1], set to the player's
    /// own tile just before [`JavaCall::FieldPickup`] — the game picks up only what is
    /// under the player, and this is how a drop two tiles away becomes that. Only for a
    /// drop the host has just checked will FIT (`hero2java::h2_drop_fits`). Stage state.
    GroundDropMove,
    /// ★ 2026-09-24: `n.o:I` ← 0 once the host has copied the pickup notice out of `n.b`
    /// (the game's own painter, which would count it down, stays off). The next notice
    /// sets it to 16 again — that edge is how each one is seen exactly once.
    NoticeTaken,
    /// ★ 2026-09-25 (부활의 구): a character that has just FALLEN (`g.t == 6`, HP 0 —
    /// `hero2java::h2_revive_writes` proves both) stood back up where it fell: HP and SP
    /// ← their own maxima (`a.a:I` ← `a.d:I`, `a.b:I` ← `a.e:I`), ★ 2026-10-26 the action
    /// chain cleared as the game's own `a.i()V` clears it (`a.o[]` ← −1, `a.i` ← −1,
    /// `a.o:I` ← 0), then `g.w` ← −1 and `g.t` ← 1 — the two bytes the game's own `g.b(1)`
    /// writes, `g.t` last because it is what stops the death. Save-bearing like [`Self::CharStats`] for the HP/SP half
    /// (the save serialises both), which is why the values are only ever the character's
    /// own maxima — the numbers a level-up or a full 물약 top-up leave there; the motion
    /// half is transient state a stage load rewrites anyway. Only after the player said
    /// 예 in the host's box, and only while the game is held still under it.
    CharRevive,
    /// ★★ 2026-10-16 엘릭서 (user: *"HP:100%, SP : 100% 회복해주는 아이템"*): HP ← max HP · SP ← max SP on a LIVE character
    /// (`h2_elixir_plan` — the 부활의 구's two vitals words, without the stand-up pair; a full character plans nothing and
    /// keeps the item). The game's own `a.m()V` clamps to the maxima anyway; nothing here outlives the next load unless saved.
    CharRefill,
    /// ★ 2026-09-26: an equipment record's `aa.f` word ([`Hero2Java::ITEM_EQUIP_SLOT_OFF`])
    /// ← `−1` — the value the game's own unequip `a.e(B)Z` writes — on a record in the
    /// host's ACCOUNT warehouse, never on one in a game bag. Before 2026-09-26 the
    /// warehouse took a WORN record (the flag travels with the record and is serialised
    /// with it, `aa.a()[B` @0x10da50), and nothing in the warehouse can be worn: the flag
    /// there is a leftover that makes the game refuse to drop it (gm[55]) and, carried
    /// back into `ag.a`, would put it on at the next load (`a.n()V`) over whatever the
    /// character wears in that slot by then. Host-owned bag, written with the next save
    /// edge like [`Self::BagOrder`].
    RecordEquipSlot,
    /// ★ 세이브 슬롯 5 (2026-09-27): the `String[]` HANDLE in one of `ag`'s two save-file
    /// name statics ([`SaveSlotsPatch::prog_names`] / [`SaveSlotsPatch::char_names`]) — a
    /// host-made `String[5]` whose first three names are the ones `ag.<clinit>` put there
    /// (`/g1../g3`, `/n1../n3`) and whose last two are `/g4 /g5` / `/n4 /n5`. The same word
    /// the game's own `putstatic` wrote a moment before; the old array is garbage after it.
    /// Only while nothing has read the tables yet (`hero2java::h2_save_names_state` is
    /// `Shipped`), and the array is pinned until this write has landed. Not save-bearing:
    /// the names are how the game FINDS a save, and slots 1..3 keep theirs.
    SaveFileNames,
    /// ★ 난이도 (2026-09-29): a MONSTER's three stat words — `bd.c:S` max HP, `bd.d:S` HP,
    /// `bd.e:S` attack ([`Hero2Java::MONSTER_MAX_HP_OFF`]‥) — and, for the boss class `ca`,
    /// its template's attack word `cq.b:S`. Kept at the setting's multiplier by
    /// `emulator::Difficulty` from `hero2java::H2Difficulty`'s plan: base × m for max HP
    /// and attack, the ratio for HP. Not save-bearing — monsters live and die inside a
    /// stage and nothing of them is serialised — so EASY (×1) is the shipped game again.
    MonsterStats,
    /// ★ 2026-10-02 (user: *"마지막 5번쨰가방에서 칸 몇개를 지워야해"*): a PAGED bag window's cell count `j.i`
    /// ([`Hero2Java::WIN_ROWS_OFF`]) — `q.<init>`'s 28 on pages 1..4, the 15 that are left of the 127 on the
    /// fifth (`hero2java::h2_page_cells_write`). The game's cursor wraps and clamps at it and the host's taps
    /// hit-test it; the painter does not read it. UI data on a live window: rebuilt by every `q.<init>`.
    BagPageCells,
    /// ★ 2026-10-03 발도: `a.p[9]` ← 0 on a KNIGHT whose `a.p[9]` reads `-1` while `a.p[8]`
    /// (방어의지) reads `≥ 0` — the unlock the quest 「터줏대감」 would have scripted (`1a 09`,
    /// what `a.h(B)V` writes) had the skill existed when the quest was written. Save-bearing
    /// like [`Self::CharCombo`] and, like it, the one kind of write that happens without a row
    /// pressed: it only OPENS a slot (level 0 = investable), never spends or grants a point, and
    /// only through `hero2java::h2_iaido_unlock`, which proves the class and both bytes.
    CharSkillUnlock,
    /// ★ 2026-10-08 가디언: the FOURTH cell of a mode-indexed static array `ag.<clinit>` / `co.<clinit>` built —
    /// four long only because [`GuestBuild::guardian_mode_tables`] made their `movs r1,#3` read `#4` before they ran:
    /// `ag.i:[B[3]` ← [`GuardianPatch::QUEST_MULT`] (quest EXP ×10 after 1·4·7) · `co.b:[I[3]` ← [`GuardianPatch::COLOUR`]
    /// (the slot screen's 「GUARDIAN」 purple); `ag.m:[B[3]` stays the 0 the allocation gave it. Only into an array that
    /// reads exactly [`GuardianPatch::TABLE_LEN`] long (`hero2java::h2_mode_tables`), and only the fourth cell — the
    /// three the game filled are never touched. Not save-bearing: the arrays are constants the game rebuilds every
    /// boot; what IS save-bearing (the mode byte 3) the game writes itself at a round clear.
    ModeTables,
    /// ★ 2026-10-09 (user: *"재료가방창도 127칸으로 … 신규아이템 조합서들을 구성할꺼야"*): a PAGED item picker's list
    /// `k.i:[B` — the bag indices `bq.b(BB)[B` built for the 7 × 4 picker `k` (the recipe list, EQUIPMENT's candidates,
    /// 능력 부여's lists) — rewritten IN PLACE to one page of the list the game built: its length word (never above the
    /// length it was built with) and that many bytes, each one of the game's own indices from that list
    /// (`hero2java::h2_picker_page_writes`). The picker reads its records through it (`k.a(I)Lch;`) and hands the parent
    /// `list[cursor]` on OK, so a page shows — and picks — exactly the records it names. UI data: every picker builds a
    /// fresh array (`aj.a()V`, `cc.a(BB)V`, `x.a(II)Z`, `ap.a(II)Z` keep no reference to it), nothing is saved.
    PickerList,
    /// ★★ 2026-10-16 (user: *"악의로 스킬이 바뀌면 야수 스킬창에서도 이름·아이콘 변경"*): an element of the beast skill
    /// icon array `cf.i:[Image` ([`Hero2Java::BEAST_ICONS_OFF`]) — always a handle that array already holds: the host
    /// EXCHANGES a changed kind's icon with its 악의 version appended after the shipped 13 (`DemonPatch::SKIN_KINDS`) while
    /// 악의 runs, and back when it ends, so every image stays referenced by the game's own array. UI data: `cf.r()V`
    /// builds the array afresh from `img/bico`, nothing is saved.
    IconArrayRef,
    /// ★★ 2026-10-16 일괄판매 (user: *"아이템 판매 창에서 … 일괄 판매 버튼"*): the INVENTORY's `bq.a:Ljava/util/Vector;`
    /// after the bulk sale — its `elementData` slots from the first sold record on (the records kept, in their order,
    /// then `null`s) and its `elementCount`, exactly what one `Vector.removeElementAt` per sold record leaves
    /// (`vector.rs::remove_element_at`: shift, null the old last, count − 1). The sold records are `ag.a`'s and nothing
    /// else's (a worn one is never sold — `aa.f`), so they become garbage. Save-bearing (`n<slot>` serialises the bag),
    /// and only ever after the game's own `ag.g(I)V` has paid their price ([`JavaCall::GoldAdd`],
    /// `hero2java::h2_bulk_sell_plan`).
    BagSold,
    /// ★★ 2026-10-16 일괄판매: the sell window `cl`'s list fields after that — `cl.a:I` (the rows = bag size − 1), `cl.c:I`
    /// ← 0 and the cursor `j.h` clamped below the rows (what its own `cl.a()V` refresh writes), and `cl.a:B` ← 2 (what
    /// its own `cl.b()V` sale leaves: an OK then closes the window when the bag is empty instead of reading a record that
    /// is no longer there). Written in the same batch as [`Self::BagSold`], so no paint or key ever sees the two disagree.
    SellWindow,
    TownVisits,
    FlightState,
    /// ★★ 2026-10-20 상점주인의 혼: the shop's STOCK static `v.a:[Ljava/util/Vector;` @0x19454c ← the `Vector[11]` the
    /// game's own loader `z.a()[Ljava/util/Vector;` returned ([`JavaCall::ShopStockLoad`]). The game fills this slot only on
    /// a shop map (`cf.o()V` @0x1578f4: map ∈ `cf.m` → `v.a ← z.a()`, else `null`), and the buy list `p.a()V` @0x146070
    /// indexes it unchecked — so the portable shop fills it the same way before it posts the shop event. Written only
    /// when the slot reads `null`; the next map load overwrites it as the game always did.
    ShopStock,
}

impl JavaWrite {
    /// Every kind, so the selftests can iterate instead of mirroring.
    ///
    /// ⛔ The array plus the length assertion beside it is the gate, not a convenience
    /// (2026-09-21, review). `anchors_selftest` used to spell this list out TWICE by
    /// hand — once for "hero3 refuses all of them", once for "hero2 opens all of them" —
    /// so a new variant was proven neither unless someone remembered to edit both. Five
    /// were added in one commit; a sixth that `java_write_ok` accidentally answered
    /// `true` for on a hero3 build would simply not have been tested, which is the one
    /// thing this selftest exists to make impossible. [`JavaCall`] got `ALL` and a length
    /// assertion when it was added; this is the same for the older gate.
    #[allow(dead_code)] // the selftests' iteration
    pub const ALL: [JavaWrite; 36] = [
        JavaWrite::WidgetCursor,
        JavaWrite::PopupLabel,
        JavaWrite::PopupRows,
        JavaWrite::Quickbar,
        JavaWrite::Cooldown,
        JavaWrite::HealSelect,
        JavaWrite::PopupLabelRef,
        JavaWrite::BagCapacity,
        JavaWrite::WindowChild,
        JavaWrite::MenuLastRow,
        JavaWrite::TableRowRef,
        JavaWrite::PopupLabels,
        JavaWrite::CharStats,
        JavaWrite::CharSkills,
        JavaWrite::BeastSkills,
        JavaWrite::CharCombo,
        JavaWrite::BagOrder,
        JavaWrite::CharFacing,
        JavaWrite::GroundDropLife,
        JavaWrite::GroundDropMove,
        JavaWrite::NoticeTaken,
        JavaWrite::CharRevive,
        JavaWrite::CharRefill,
        JavaWrite::RecordEquipSlot,
        JavaWrite::SaveFileNames,
        JavaWrite::MonsterStats,
        JavaWrite::BagPageCells,
        JavaWrite::CharSkillUnlock,
        JavaWrite::ModeTables,
        JavaWrite::PickerList,
        JavaWrite::IconArrayRef,
        JavaWrite::BagSold,
        JavaWrite::SellWindow,
        JavaWrite::TownVisits,
        JavaWrite::FlightState,
        JavaWrite::ShopStock,
    ];
}

/// ★★ hero2's CALL gate (2026-09-21, HERO2_NEXT_SESSION.md 「🔜 ACCOUNT 창고 + 가방 5페이지」)
/// — one item per Java METHOD the host may invoke, the way [`JavaWrite`] is one item per
/// kind of field the host may write. Each call goes through the JVM BY NAME
/// (`Emulator::call_java_method`), so what is listed here is a `(class, method, descriptor)`
/// and not an address — see [`JavaCall::spec`]. The receiver and the arguments are handles
/// the host has JUST READ (or a static the JVM resolves), and the backend refuses a handle
/// that is not a live instance of the named class before anything enters the guest.
///
/// Why these and not the three the plan first listed (`bq.a(I)Lch;` · `bq.a(Lch;B)I` ·
/// `bq.a(BI)V`): the disassembly of `bq.a(Lch;B)I` @0x1369a4 shows it inserts the record
/// object IT IS GIVEN into the bag's Vector (first item of a non-stackable, or a stackable
/// with no stack to merge into) and overwrites that record's count. Passing a record that
/// is still in the SOURCE bag therefore puts one object in two Vectors, and the consume
/// that follows (`bq.a(BI)V` → `ch.a(I)V` → `c -= n`) zeroes it in both. The game itself
/// only ever hands that method a FRESH record from the factory `ch.a(BBZZ)Lch;` — so the
/// host does the same for stackables, and MOVES the object for equipment: a `z`
/// (equipment) record carries four stats rolled by `h.a(II)I` at creation, so re-creating
/// one re-rolls it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // a whitelist: the probe's drives (`--h2-give`) name calls the app does not (yet)
pub enum JavaCall {
    /// `ag.l()V` — the game's own save, on the `ag.a:Lag;` singleton (see [`Hero2Save`]).
    Save,
    /// ★ 오토루팅 (2026-09-24): `ag.a()Z` @0x1121e4, static — the pickup the OK key tries
    /// first (`n.i()V` @0x162afc). It takes the first drop on the player's own tile that
    /// fits its bag (`bf.a(BB)[B`), files it, and raises the notice (`n.a([BII)V`);
    /// `false` when nothing on the tile fits (and then, if something IS there, the
    /// 「빈공간이 없습니다.」 notice).
    FieldPickup,
    /// ★ 2026-09-24: `n.a([BII)V` @0x1645e4 on the HUD singleton — raise a notice with
    /// the game's own renderer: `(text, 16, 0)`, as `ag.a()Z` calls it. Used for the
    /// bag-full notice when OK is pressed NEAR a drop that does not fit (the game says it
    /// only for one under the player).
    Notice,
    /// `bq.a(Lch;I)Z` @0x136664 — put THIS record object into the bag with count `I`
    /// (`rec.c ← I`, `Vector.addElement(rec)`); `false` when `size > b`. The
    /// identity-preserving half of a MOVE: the record keeps its serial and its stats.
    BagInsertRecord,
    /// `bq.b(B)V` @0x136e04 — `Vector.removeElementAt(B)`. The other half of a move, and
    /// only ever AFTER the insert succeeded, so a full destination cannot lose the item.
    BagRemoveAt,
    /// `ch.a(BBZZ)Lch;` @0x1408fc — the record FACTORY (`static`): `(type, id, roll, ?)`
    /// picks the subclass by type, parses the item DB row, and with `roll` rolls the
    /// serial (`ch.f`) and, for equipment, the stats. What every drop and purchase calls.
    RecordNew,
    /// `bq.a(Lch;B)I` @0x1369a4 — add `B` of the record's `(type, id)` to the bag: merges
    /// into existing stacks (cap 99 each), inserts the record for the rest. Returns the
    /// number PLACED (0 when the room check fails, `n` on full success, in between when
    /// the bag filled up part-way) — so the caller consumes exactly that many from the
    /// source. ⛔ Only ever with a FRESH record from [`JavaCall::RecordNew`] (see above).
    BagAddCount,
    /// `bq.a(BI)V` @0x136d3c — consume `I` from the record at index `B`
    /// (`rec.c -= I`, remove when `<= 0`). The SKT `ci.a(h,1)` that 사용하기 calls.
    BagConsumeAt,
    /// `new bq(B)` — `bq.<init>(B)V` @0x136334, the bag constructor: the ACCOUNT warehouse
    /// is one of these, made by the host and pinned for the life of the process (#13
    /// step 5, 2026-09-21).
    BagNew,
    /// `new q(Lj;Lbq;Z)` — `q.<init>(Lj;Lbq;Z)V` @0x146574, the game's own inventory
    /// window over any bag: `(parent, bag, usable)`. The ACCOUNT window is one with
    /// `usable = false` (storage only: no 사용하기/장비장착 rows, title = `gm[57]`), exactly
    /// how the game opens the 재료 가방.
    InventoryWindowNew,
    /// `n.b()V` @0x16362c on the `n.a:Ln;` singleton — what `bw.b()Z` calls after it has
    /// hung a new window on the menu: `n.g ← true`, `n.a:Z ← true` (the HUD's "redraw
    /// me" flags). Mirrored so the host-opened window leaves the same state behind.
    HudDirty,
    /// `bq.a()[B` @0x13771c — the bag's own serialiser, the blob the save writes. The
    /// ACCOUNT warehouse is persisted as exactly this (#13 step 6).
    BagSerialize,
    /// `bq.a([B)V` @0x137a6c — its inverse; what the load runs on every bag.
    BagDeserialize,
    /// `i.a(II)[[B` / `i.a(III)[[B` on the `/t/gm` table singleton (`bw.a:Li;`) — a fresh
    /// `byte[][]` of those rows, exactly what `q.a(II)Z` calls to build a popup's labels.
    /// The move row's way onto a popup whose own array has no slack (#13 step 7).
    /// ⚠ CURRENTLY UNREACHABLE, and listed anyway (2026-09-21, review). The two-row
    /// factory can only be asked for by a popup whose label array holds ONE row and has
    /// no slack, and by the bucket model in `h2_object_array_capacity` a one-row array
    /// (12 B) lands in the 16-byte class, which holds two — so that popup always takes
    /// the `Slack` path and `NeedArray` is reachable only at two rows, which asks for
    /// `TableRows3`. It stays because the arm that would use it is correct and the
    /// reasoning above is about the ALLOCATOR's bucket table, not about this gate; but
    /// nothing in the app or the probe exercises it, so do not read its presence here as
    /// "measured against a live popup" the way the other twelve are.
    TableRows2,
    TableRows3,
    /// ★ `new aq(Lj;II)` — `aq.<init>(Lj;II)V` @0x11dc94, the game's own 개수 box:
    /// `(parent, unitPrice, max)`. `super(parent, 1)` then `j.h = 1`, so it opens on 1 and
    /// the player walks it with LEFT/RIGHT (±1) and DOWN/UP (±10, clamped to `max`).
    /// Passed a price of 0 so the `price x n` line under the number is not drawn — the
    /// warehouse does not charge. HERO2_GUEST_RE.md §2.7.12.
    QuantityPopupNew,
    /// ★ 경험치 배율 (2026-09-23): `a.d(I)V` @0x14fa64 on the `ag.a:La;` character — the
    /// game's ONE "gain EXP" method (`a.c:I += I`, then the beast's `bn.a(I)V` with the
    /// same amount, then `n.b:Z ← 1`). Not something the app ever calls: it is the
    /// PROBE's end-to-end check of the [`ExpRatePatch`] trampoline — call it with 100
    /// and read `+0x74` back, and the multiplier either shows or it does not
    /// (`--h2-exp-gain`). HERO2_GUEST_RE.md §8.8.
    CharGainExp,
    /// ★ #20/#21 (2026-09-24), PROBE only: `a.a()Lbd;` @0x151330 on the character — the
    /// monster standing in the tile it faces (`m.a(BLad;)Lad;` then an `instanceof bd`),
    /// or null. How `--h2-kill` finds a live monster to hand to [`Self::MonsterDeath`].
    CharFacingMonster,
    /// ★ #20/#21 (2026-09-24), PROBE only: `bd.q()V` @0x128080 on a monster — the game's
    /// own death routine: the drop roll over `enm/enm`'s slots (the mode window), gold,
    /// 제련석, 물약, 스펠카드, then the EXP. Called N times in a row on one monster by
    /// `--h2-kill`, which tallies what lands in the stage's ground list (`bf.b` Vector).
    MonsterDeath,
    /// ★ #17 (2026-09-24), PROBE only: `a(BILbd;)I` on the character — the class's skill
    /// multiplier (`ay`/`bl`/`bx` override it). `--h2-skill` calls it with an index, a
    /// damage of 1000 and a null monster, before and after [`SkillPowerPatch`], and prints
    /// what the game's own formula answers.
    CharSkillPower,
    /// ★ #17 (2026-09-24), PROBE only: `a(I)S` on the character — the class's cooldown
    /// TOTAL for an active skill, halved by [`GuestBuild::skill_cooldown_half`].
    CharCooldownTotal,
    /// ★ 2026-09-27, PROBE only: `a(I)I` on the character — the class's SP COST table for a
    /// skill past the first (`a.f()Z` 0x14f852 asks it), cut −40 % for the rogue by
    /// [`GuestBuild::rogue_skill_sp_cut`]. `--h2-skill` prints it for every `a.p` index.
    CharSkillSpCost,
    /// ★ 부활의 구 (2026-09-25), PROBE only: `a.b(I)V` @0x14f978 on the character — the
    /// game's ONE HP-delta method (`hp += n`, clamped to `[0, a.d:I]`, and at 0 the fall:
    /// `this.b(6)` then `g.w ← 0`). `--h2-die` calls it with `−hp`, which is exactly what a
    /// killing blow does, so the fall the revive answers is the game's own.
    CharHpDelta,
    /// ★★ 2026-09-26 (user: "안내팝업은 항상 기존 게임 팝업으로 만들어줘. 내가 직접그린
    /// 안내토스트문구가 뜨는게아니라"): `new ba(Lj;[B)` — `ba.<init>(Lj;[B)V` @0x121404, the
    /// game's own MESSAGE BOX `(parent, text)`. It wraps the text at 101 px itself
    /// (`ac.a([BI)I(text, 101)` → `ba.b` lines, `ba.a` = the height) and closes itself on OK
    /// or CLR (`ba.a(II)Z` @0x12156c → `parent.b(0)` = `j.b(I)V` @0x15e5cc: `this.b ← null`,
    /// `n.b()`, `this.g()`), so the parent is back exactly as it was. The constructor does NOT
    /// hang it: the game does that itself right after — `q.a(BB)V` @0x146a22 builds
    /// `new ba(this, gm.a(55))` 「장착중인 아이템은 버릴수 없습니다.」 and stores it in `this.b`
    /// (0x146a38) — and so does the host, through [`JavaWrite::WindowChild`]
    /// (`hero2java::h2_child_open_writes`).
    MessageBoxNew,
    /// ★ 의상 아바타 (2026-09-30): `cf.a(I)V` @0x153d18 on the parts singleton `cf.a:Lcf;` —
    /// the game's OWN loader for one of the character's sprite parts (`0` head · `1` body ·
    /// `2` arms · `3` legs · `6` weapon · `7` shield): reads the equipped record's sprite
    /// number and palette — through [`AvatarPatch`]'s override for all six (★ A3b: 0, 6 and 7
    /// too) — builds `/hero/<part>/<nn>` (weapon / off-hand: `<type><nn>`), loads the `am`
    /// sheet and refills `cf.a[part]` / `cf.a[part+8]`.
    /// What the equipment window calls after a change (`cc.b(B)V`, 7 sites); the host calls
    /// it for the parts whose costume changed while the character is in the world.
    PartsReload,
    /// ★ 2026-10-01 (user: *"장비 창에서 캐릭터가 착용 장비에 따라서 외형도 같이 보여졌으면"*):
    /// `cf.b(I)V` @0x154890 on the parts singleton — `cf.a[part] ← null`, `cf.a[part + 8] ←
    /// null`. What `cc.b(B)V` @0x13e62c runs for a weapon (6) or off-hand (7) TAKEN OFF: the
    /// loader's case 6/7 returns on an empty slot without touching the images, so only this
    /// makes the hand empty. The EQUIPMENT window's character follows the equipment through
    /// `hero2java::h2_slot_refresh`.
    PartsClear,
    /// ★ 2026-10-01: `cf.a(Z)V` @0x1548f8 — part 5's sparkle frames ← null: `true` = 0..4 (the
    /// weapon's trait), `false` = 5..9 (the rogue's second dagger). `cc.b(B)V` runs it after a
    /// weapon goes (true) and for a rogue whose second dagger has no trait (false).
    PartsShine,
    /// ★ 2026-10-01: `cf.a(B)V` @0x156b14 — the swing sound (`br.a(4, 0x33 | 0x34 | 0x35)` by
    /// the weapon type, 0x33 out of 0..=4). `cc.b(B)V` runs it with 0x33 for a rogue left with
    /// only the second dagger; the loader's case 6 runs it for every weapon it loads.
    PartsSwing,
    RefineAttempt,
    /// ★★ 2026-10-16 (user: *"야수스킬 찍은 후에 야수를 버려버리면 스킬포인트가 증발되는 버그"*): `bn.d()V` @0x135254 — the
    /// beast's own skill-point return (Σ `bn.k[i > 0]` onto the character's `a.d`, the levels to 0), the evolution window's
    /// call (`ap.b()V`, its ONLY caller — the beast list's discard `bz.a(BB)V` kind 10 just `removeElementAt`s). The host
    /// calls it on the beast about to be discarded (`hero2java::h2_beast_discard_pending`) before the 예 goes to the game.
    BeastRefund,
    /// ★★ 2026-10-16 일괄판매: `i.a(IIII)[[B` @0x15dc14 on the item-type table `cf.c:Li;` (`/t/itmtp`, alive for the whole run)
    /// — the four grade names 일반 · 레어 · 에픽 · 영웅 (rows 35..38, the info painter's own `35 + aa.b`) as a fresh
    /// `byte[][]`, the labels of the grade list. (The confirm box's three lines are [`Self::TableRows3`] on the SHOP's
    /// table `v.a:Li;` — `/t/shop`, loaded by `v.a()Lv;` and nulled by `v.c()V`, alive exactly while the shop is — the
    /// receiver given at the call: the same method, so not a call of its own.)
    ItemTypeRows4,
    /// ★★ 2026-10-16 일괄판매: `j.a(BI[[B)V` @0x15dffc on the sell window `cl` — the game's own "open a popup on me":
    /// `new d(this, kind, (byte)I, labels)`, hung as `this.b`. What `cl.a(II)Z` calls for its own 「판매하시겠습니까?」
    /// (`(2, 2, [shop[22]])`); the host opens the grade list `(3, 4, grades)` and the confirm `(10, 2, lines)` with it.
    /// Their answers never reach the window: the host takes the OK / CLR while its popup is the window's child (and
    /// `cl.a(BB)V` would only close it — 3 and 10 are not kinds it acts on; it acts on 2 and 50).
    SellPopupNew,
    /// ★★ 2026-10-16 일괄판매: `ag.g(I)V` @0x112644, static — the game's own "add to the gold" (`ag.f:I += n`, clamped
    /// 0..=999,999,999), the call `cl.b()V` makes for a sale. The bulk sale pays the SUM of the sold records' prices
    /// through it, before the records leave the bag ([`JavaWrite::BagSold`]).
    GoldAdd,
    /// ★★ 2026-10-17 보스 레이드: `bb.a(B)V` @0x122008 on the script system's singleton `bb.a:Lbb;` (@0x193d7c) — run the AUTO
    /// (kind 0) records of trigger group `B − 1`: `bb.a(BB)Z(0, B)` walks the group, takes the first record whose conditions
    /// pass, sets `bb.c` to its script and starts it (`bb.f()V`). The very call `bd.q()V` makes for a boss's kill group
    /// (`cq.l`). The raid's countdown runs a stage map's advance group with it (`hero2java::H2_RAID_ADVANCE_GROUP`), so the
    /// warp to the next boss is the game's own script.
    ScriptGroup,
    FlightWarp,
    /// ★★ 2026-10-20 상점주인의 혼: `z.a()[Ljava/util/Vector;` @0x140a90, static — the game's own shop-stock loader: a
    /// `Vector[11]` (one list per weapon / armour class and one 기타 list) filled from `/itm/forshop` through the record
    /// factory `z.a(BBZZ)Lch;`. The very call `cf.o()V` makes on a shop map's load; its result goes into the static the
    /// buy list reads ([`JavaWrite::ShopStock`]).
    ShopStockLoad,
    /// ★★ 2026-10-20 상점주인의 혼: `ag.a(BB)V` @0x11085c, static — post a game EVENT `(ag.d, ag.e)` (`ag.a(BBBB)V(d, e, 0, 0)`
    /// under the game's own monitor; the dispatcher `ag.g()V` @0x111648 runs it on the next tick and clears `ag.d`).
    /// `(4, 0)` is what a shop NPC's script posts: `ag.h(7)` · `v.a()Lv;` · `v.a()V` — the state word 7, the shop
    /// singleton, its buy/sell menu ([`Hero2Java::SHOP_EVENT`]).
    EventPost,
    /// ★★ 2026-10-21 (user: *"비행서 사용한 직후에 메뉴버튼 누르면 메뉴가 안열리고있어"*): `bw.a(Z)V` @0x13ab98 on the field
    /// menu's singleton `bw.a:Lbw;` (@0x19422c) — the game's own RELEASE of the menu: cursor `j.h` ← 0, the menu images
    /// `bw.a:Li;` · `bw.b:Li;` · `bw.c` ← null, the window chain `j.b` ← null, the singleton ← null, `bs.a.f()V`, the full
    /// repaint `n.a:Z` ← 1. What the close event 7 runs (`ag.g()V` 0x11178a: `ag.a` ← 3, then `bw.a(Z)V(1)` unless `ag.e`
    /// is 1 — the 귀환서's own exit, `ag.e` 2, warps right after it). A menu left alive (child ← 0 · state ← 3 alone) is
    /// REUSED by the next open (`bw.a()Lbw;` builds one only when the static is null) and `bw.a()V` finds its images
    /// loaded — nothing marks it to be drawn, so the state reads 5 over an unchanged field: the menu does not open.
    MenuRelease,
}

/// Whether a [`JavaCall`] takes a receiver, is `static`, or constructs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallKind {
    /// An instance method: the receiver is a handle the host holds, or the singleton
    /// named by [`JavaMethodSpec::singleton`].
    Virtual,
    Static,
    /// `new <class><descriptor>` — `Emulator::new_java_object`.
    Constructor,
}

/// How a [`JavaCall`] is spelled to the JVM.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JavaMethodSpec {
    /// The declaring class — and, for an instance method, the class the receiver handle
    /// must be a live instance of.
    pub class: &'static str,
    pub method: &'static str,
    pub descriptor: &'static str,
    pub kind: CallKind,
    /// For a `Virtual` call on a game singleton: the static field that holds the receiver,
    /// `(class, field, descriptor)`. `None` = the caller supplies a handle.
    pub singleton: Option<(&'static str, &'static str, &'static str)>,
}

impl JavaCall {
    #[allow(dead_code)] // the selftests' iteration
    pub const ALL: [JavaCall; 38] = [
        JavaCall::Save,
        JavaCall::FieldPickup,
        JavaCall::Notice,
        JavaCall::BagInsertRecord,
        JavaCall::BagRemoveAt,
        JavaCall::RecordNew,
        JavaCall::BagAddCount,
        JavaCall::BagConsumeAt,
        JavaCall::BagNew,
        JavaCall::InventoryWindowNew,
        JavaCall::HudDirty,
        JavaCall::BagSerialize,
        JavaCall::BagDeserialize,
        JavaCall::TableRows2,
        JavaCall::TableRows3,
        JavaCall::QuantityPopupNew,
        JavaCall::CharGainExp,
        JavaCall::CharFacingMonster,
        JavaCall::MonsterDeath,
        JavaCall::CharSkillPower,
        JavaCall::CharCooldownTotal,
        JavaCall::CharSkillSpCost,
        JavaCall::CharHpDelta,
        JavaCall::MessageBoxNew,
        JavaCall::PartsReload,
        JavaCall::PartsClear,
        JavaCall::PartsShine,
        JavaCall::PartsSwing,
        JavaCall::RefineAttempt,
        JavaCall::BeastRefund,
        JavaCall::ItemTypeRows4,
        JavaCall::SellPopupNew,
        JavaCall::GoldAdd,
        JavaCall::ScriptGroup,
        JavaCall::FlightWarp,
        JavaCall::ShopStockLoad,
        JavaCall::EventPost,
        JavaCall::MenuRelease,
    ];

    /// The `(class, method, descriptor)` the JVM resolves — measured in the image's own
    /// method table (`tools_re/dump_hero2_java.py --class bq / ch / q / n`) and read in the
    /// disassembly (`tools_re/hero2_dis.py`), HERO2_GUEST_RE.md §2.7.3 / §2.7.8 / §2.7.11.
    pub const fn spec(self) -> JavaMethodSpec {
        const fn v(class: &'static str, method: &'static str, descriptor: &'static str) -> JavaMethodSpec {
            JavaMethodSpec { class, method, descriptor, kind: CallKind::Virtual, singleton: None }
        }
        match self {
            JavaCall::Save => JavaMethodSpec { class: "ag", method: "l", descriptor: "()V", kind: CallKind::Virtual, singleton: Some(("ag", "a", "Lag;")) },
            JavaCall::FieldPickup => JavaMethodSpec { class: "ag", method: "a", descriptor: "()Z", kind: CallKind::Static, singleton: None },
            JavaCall::Notice => JavaMethodSpec { class: "n", method: "a", descriptor: "([BII)V", kind: CallKind::Virtual, singleton: Some(("n", "a", "Ln;")) },
            JavaCall::BagInsertRecord => v("bq", "a", "(Lch;I)Z"),
            JavaCall::BagRemoveAt => v("bq", "b", "(B)V"),
            JavaCall::RecordNew => JavaMethodSpec { class: "ch", method: "a", descriptor: "(BBZZ)Lch;", kind: CallKind::Static, singleton: None },
            JavaCall::BagAddCount => v("bq", "a", "(Lch;B)I"),
            JavaCall::RefineAttempt => v("ck", "d", "()V"),
            JavaCall::BeastRefund => v("bn", "d", "()V"),
            JavaCall::BagConsumeAt => v("bq", "a", "(BI)V"),
            JavaCall::BagNew => JavaMethodSpec { class: "bq", method: "<init>", descriptor: "(B)V", kind: CallKind::Constructor, singleton: None },
            JavaCall::InventoryWindowNew => JavaMethodSpec { class: "q", method: "<init>", descriptor: "(Lj;Lbq;Z)V", kind: CallKind::Constructor, singleton: None },
            JavaCall::HudDirty => JavaMethodSpec { class: "n", method: "b", descriptor: "()V", kind: CallKind::Virtual, singleton: Some(("n", "a", "Ln;")) },
            JavaCall::BagSerialize => v("bq", "a", "()[B"),
            JavaCall::BagDeserialize => v("bq", "a", "([B)V"),
            JavaCall::TableRows2 => JavaMethodSpec { class: "i", method: "a", descriptor: "(II)[[B", kind: CallKind::Virtual, singleton: Some(("bw", "a", "Li;")) },
            JavaCall::TableRows3 => JavaMethodSpec { class: "i", method: "a", descriptor: "(III)[[B", kind: CallKind::Virtual, singleton: Some(("bw", "a", "Li;")) },
            JavaCall::QuantityPopupNew => JavaMethodSpec { class: "aq", method: "<init>", descriptor: "(Lj;II)V", kind: CallKind::Constructor, singleton: None },
            JavaCall::CharGainExp => JavaMethodSpec { class: "a", method: "d", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) },
            JavaCall::CharFacingMonster => JavaMethodSpec { class: "a", method: "a", descriptor: "()Lbd;", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) },
            JavaCall::MonsterDeath => v("bd", "q", "()V"),
            JavaCall::CharSkillPower => JavaMethodSpec { class: "a", method: "a", descriptor: "(BILbd;)I", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) },
            JavaCall::CharCooldownTotal => JavaMethodSpec { class: "a", method: "a", descriptor: "(I)S", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) },
            JavaCall::CharSkillSpCost => JavaMethodSpec { class: "a", method: "a", descriptor: "(I)I", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) },
            JavaCall::CharHpDelta => JavaMethodSpec { class: "a", method: "b", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) },
            JavaCall::MessageBoxNew => JavaMethodSpec { class: "ba", method: "<init>", descriptor: "(Lj;[B)V", kind: CallKind::Constructor, singleton: None },
            // `cf.a:Lcf;` @0x1945ec is the parts singleton (`cf.<clinit>` makes it; `cf.a()V`,
            // `cc.b(B)V` and `n.a(B)V` all call through it).
            JavaCall::PartsReload => JavaMethodSpec { class: "cf", method: "a", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) },
            JavaCall::PartsClear => JavaMethodSpec { class: "cf", method: "b", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) },
            JavaCall::PartsShine => JavaMethodSpec { class: "cf", method: "a", descriptor: "(Z)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) },
            JavaCall::PartsSwing => JavaMethodSpec { class: "cf", method: "a", descriptor: "(B)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) },
            JavaCall::ItemTypeRows4 => JavaMethodSpec { class: "i", method: "a", descriptor: "(IIII)[[B", kind: CallKind::Virtual, singleton: Some(("cf", "c", "Li;")) },
            JavaCall::SellPopupNew => v("cl", "a", "(BI[[B)V"),
            JavaCall::GoldAdd => JavaMethodSpec { class: "ag", method: "g", descriptor: "(I)V", kind: CallKind::Static, singleton: None },
            JavaCall::FlightWarp => JavaMethodSpec { class: "ag", method: "b", descriptor: "(BBBB)V", kind: CallKind::Static, singleton: None },
            JavaCall::ScriptGroup => JavaMethodSpec { class: "bb", method: "a", descriptor: "(B)V", kind: CallKind::Virtual, singleton: Some(("bb", "a", "Lbb;")) },
            JavaCall::ShopStockLoad => JavaMethodSpec { class: "z", method: "a", descriptor: "()[Ljava/util/Vector;", kind: CallKind::Static, singleton: None },
            JavaCall::EventPost => JavaMethodSpec { class: "ag", method: "a", descriptor: "(BB)V", kind: CallKind::Static, singleton: None },
            JavaCall::MenuRelease => JavaMethodSpec { class: "bw", method: "a", descriptor: "(Z)V", kind: CallKind::Virtual, singleton: Some(("bw", "a", "Lbw;")) },
        }
    }
}

/// ★★ 경험치 배율 (2026-09-23, user: "환경설정에 경험치배율 … 기존, x2, x4, x6") — every
/// EXP the character gains, multiplied, by redirecting ONE method-definition word.
///
/// ── THE GAIN PATH (`tools_re/find_field_uses.py a.c --desc I --writes`) ─────────────
/// Four methods write `a.c:I` (EXP, `+0x74`): new-character setup, save load, the
/// level-up (`a.l()V`, which only SUBTRACTS `next`), and **`a.d(I)V` @0x14fa64** — the
/// one and only gain:
///
/// ```text
/// a.d(I)V:  r4 = this, r5 = amount
///           a.c:I  ← a.c:I + r5                       0x14fa7c  adds r2,r0,r5
///           a.a:Lbn; (the beast) . bn.a(I)V(r5)        0x14fa9a  the beast gains the SAME amount
///           n.b:Z ← 1                                  (HUD repaint flag)
/// ```
/// Two callers, both through the VIRTUAL helper 0x16917c: `bd.q()V` @0x128120 — the
/// monster kill, `amount = mon_lv * 17 * clamp(20 + mon_lv - player_lv, 0, 28) / 20` —
/// and `bb.m()V` @0x124638, the script system's quest reward. Neither displays the
/// number afterwards (`bd.q` copies it to `r5` and overwrites `r5` before reading it), so
/// there is no "EXP +N" text to keep in step. `ay`/`bl`/`bx` do NOT override `d(I)V`
/// (`dump_hero2_java.py --json`: all three carry `a`'s own def, `own: false`), so this
/// one record is the path for every class.
///
/// ── WHY THE DEFINITION RECORD, NOT THE CALL SITES AND NOT THE BODY ────────────────
/// The body has no room: the multiply needs `movs rX,#N; muls`, two halfwords with a
/// free register, and the only spare halfword is the `adds r5,r2,#0` at 0x14fa76 (a
/// single `lsls` does x2/x4 but not x6). The call sites could be retargeted like #13's
/// seven, but there is a smaller lever above both of them: BOTH dispatch helpers end by
/// reading the body pointer out of the method's runtime definition record at CALL TIME —
/// the virtual one (`0x1674cc`) takes `vtable[index]`, which IS the def pointer, and the
/// helper does `ldr r1,[r0]` on it; the direct one (`0x16738c`) does `ldr r0,[r4]` on the
/// cell's def. So word 0 of `a.d(I)V`'s def (`0x185644 + 906 * 0x1c` = **0x18b95c**,
/// `def_fo` 571740 in `dump_hero2_java.py --json`) is the single place every call goes
/// through, including a host call by name (`JavaMethod::run` re-reads `fn_body` per call).
/// Pointing it at a 16-byte host block that scales `r2` and `bx`es to the real body
/// multiplies the character's AND the beast's gain, for both callers, with no call-site
/// rewrite and nothing in the image's code bytes changed.
///
/// ```text
/// block+0   4801  ldr r0, [pc, #4]    ; block+8  = the multiplier
/// block+2   4342  muls r2, r0, r2     ; r2 = amount * multiplier   (r2 is the argument)
/// block+4   4801  ldr r0, [pc, #4]    ; block+12 = body | 1
/// block+6   4700  bx r0
/// block+8   .word multiplier          ← the only word the settings row ever changes
/// block+12  .word 0x14fa65
/// ```
/// `r0` is the helper's method pointer, which the body overwrites at its third
/// instruction before reading it; `r1` (this) and `lr` (the caller) are untouched, so the
/// body's `pop {r4,r5,pc}` returns straight to the caller as before.
///
/// ── WHAT IT CHANGES AND WHAT IT DOES NOT ───────────────────────────────────────────
/// Nothing is written to the character: the game adds a bigger number through its own
/// code, and its own level-up loop consumes it. The multiplier is a host setting
/// (`wie_exp_rate.txt`), applied live — the worker keeps `block+8` equal to it — and
/// 기존 (x1) is the identity, so a player who steps back to it sees the shipped rule at
/// the next kill. ⛔ It scales SCRIPT rewards too (a quest that says 500 pays 500 × N).
///
/// Same regime as [`InventoryPagePatch`]: measured in OUR image, read-verified before
/// the write (word 0 must read `body | 1`), all-or-nothing with rollback, proven
/// headless (probe `--h2-exp <n> --h2-exp-gain <f:amount>`), locked against hero3 by
/// `anchors_selftest`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ExpRatePatch {
    /// The RUNTIME method-definition record of `a.d(I)V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
}

impl ExpRatePatch {
    /// The host block: four halfwords of code, the multiplier, the body pointer.
    pub const BLOCK_LEN: u32 = 0x10;
    /// Offset of the multiplier word inside the block.
    pub const MULT: u32 = 0x08;
    /// Offset of the `body | 1` word inside the block.
    pub const BODY: u32 = 0x0c;
}

/// ★★ 가디언 레벨 (GL, 2026-10-22, user: *"레벨 99를 찍으면 따로 레벨을 관리하는쪽에서 1부터 시작하는 경험치바 위에
/// 가디언레벨(가칭)을 관리하고싶은거야 … 가디언 레벨은 250이 최대치 … 레벨 보정같은 시스템엔 영향을 주면 안돼."*) — a
/// level AFTER 99 that only shows and gives stat points. Plan = `HERO2_GUARDIAN_LEVEL_PLAN.md` (D1 3 points · D2 the
/// 경험치배율 applies · D3 +2 % of the first step per GL · D4 GL 1 at 99, no back-pay).
///
/// ── THE LEVEL-UP (`tools_re/hero2_dis.py 'a.l()V'`, 0x14fac8..0x14fc1e) ──────────────────────
/// ```text
/// a.l()V (this = r6):  if a.c:I (EXP) < a.f:I (next) return
///                      a.c:I ← a.c − a.f                                  0x14fb16
///                      if a.a:B (level) > 98 (0x14fb24):                  ← the shipped cap
///                          if a.c < 0: a.c ← 0 · n.b:Z ← 1 · return       (so a LV 99 bar DROPS 968,418 a lap)
///                      level += 1 · a.m() · a.c:S (stat points) += 3 · odd level: a.d:S += 1 (#18's curve)
///                      a.d() (unlocks) · a.e(100) · a.f(100) (HP · SP full) · the LEVEL UP effect · n.b ← 1
/// ```
/// One level a call, and ONE caller — the character's update `a.g()V` @0x14ddee (virtual helper), so it runs every
/// field frame. `a.f:I` is written only by `a.m()V` (0x150178 ← 0, 0x1501d2 ← `L³ − L² + 80L`; every recompute — a hit,
/// gear, a level) and read only by `an.a(II)V` (the STATUS window: a 64-px bar `a.c·64/a.f` clamped, and the numbers
/// `a.c / a.f`), `a.l()V` and the off HUD painter `n.d()V`. `ay`/`bl`/`bx` inherit `a.l()V`'s definition
/// (`dump_hero2_java.py --json`: `own: false`, def 0x18b978 — `0x185644 + 907 × 0x1c`).
///
/// ── THE WRAPPER (def word 0 → `block + CODE_AT`, the third machine of [`ExpRatePatch`]) ─────────────
/// ```text
/// level (fb+0x60) < 99            → the body, r1 / lr as they came (99 is reached the shipped way)
/// !ARMED || OWNER != this          → return (no body — the shipped cap must not eat the GL bar before the host arms)
/// FRESH                            → EXP ← 0 once (D4: a character already at 99 when this build met it starts at 0)
/// GL < 1                           → GL ← 1 (99 reached in play: the EXP over the line carries into GL 1)
/// up to ROUNDS times, GL < MAX, gnext(GL) > 0, EXP ≥ gnext:  EXP −= gnext · stat points += POINTS (≤ 32,767) · GL += 1 · UPS += 1
/// GL == MAX                        → EXP ← min(EXP, gnext(MAX))     (a full bar, never an int overflow)
/// NEXT ← gnext(GL) · a.f:I ← gnext(GL)                                (the STATUS window shows the GL bar)
/// ```
/// GL and the points move in ONE guest routine, so no save can hold one without the other. Nothing the game reads its
/// level from changes (`a.a:B` stays 99 — the level formulas, unlocks, map triggers, equipment), and `a.f:I` is not in
/// the save (`a.b()[B` reads `a.c`, not `a.f`). The GL itself lives in the costume record (`avatar_state::AvatarState::gl`,
/// bound to the save like the costumes); the host writes it into the block when the character enters the world (ARMED
/// last) and reads it back as it moves.
///
/// ── ★ 2026-10-22 리뷰 (P1 · P2): two more entries in the same block ──────────────────────────────────────────
/// * **The save's snapshot** — `ag.g(B)V` @0x114110 (def 0x186364; the ONE `/g` writer, called only by `ag.l()V`
///   @0x113f94 through the direct helper) → `SNAP`: while armed, `SAVED_GL ← GL · SAVES += 1`, then the body (r1 · r2 ·
///   lr as they came; the body writes r3 at 0x11411c before reading it). The host's costume journal is written from the
///   last state it PUBLISHED, which can lag a GL-up made in the same tick as a quick save; the commit after the save
///   takes the GL from this snapshot instead (`avatar_state::AvatarBook::note_save_gl`) — the GL the save was made at,
///   with the stat points that save holds.
/// * **The recompute** — `a.m()V` (def 0x18b9cc, the [`GearPatch`]'s too: whatever its word 0 names when the block is
///   installed is CHAINED — the body, or a block that chains the body) → `RECALC`: call the chain, then, for the armed
///   character at 99, `a.f:I ← gnext(GL)`. `a.m()V` rewrites `a.f` with LV 99's 968,418 at every recompute — a stat
///   point spent in the STATUS window is one — and the menu stops the character's update, so the GL bar would read
///   wrong until the field came back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GuardianLevelPatch {
    /// The RUNTIME method-definition record of `a.l()V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
    /// `ag.g(B)V` (the `/g` writer): its definition record and body.
    pub save_def: u32,
    pub save_body: u32,
    /// `a.m()V` (the recompute): its definition record — chained, not required to read as shipped.
    pub recalc_def: u32,
}

impl GuardianLevelPatch {
    pub const BLOCK_LEN: u32 = 0x140;
    /// The code starts here; the words before it are the host's and the block's.
    pub const CODE_AT: u32 = 0x40;
    /// The host's words: armed (0/1, written LAST) · the character (its handle) · the GL · zero the EXP once.
    pub const ARMED: u32 = 0x00;
    pub const OWNER: u32 = 0x04;
    pub const GL: u32 = 0x08;
    pub const FRESH: u32 = 0x0c;
    /// The curve and the reward, written with the block: gnext(g) = BASE + STEP · (g − 1).
    pub const BASE: u32 = 0x10;
    pub const STEP: u32 = 0x14;
    pub const MAX: u32 = 0x18;
    pub const POINTS: u32 = 0x1c;
    /// The block's outputs: the current GL's next EXP · the GL-ups since the install.
    pub const NEXT: u32 = 0x20;
    pub const UPS: u32 = 0x24;
    /// ★ P1: the GL at the last save's `/g` write (while armed), and how many such saves since the install.
    pub const SAVED_GL: u32 = 0x28;
    pub const SAVES: u32 = 0x2c;
    /// ★ P2: what `a.m()V`'s definition named when the block was installed — the recompute wrapper calls it.
    pub const CHAIN_RECALC: u32 = 0x30;
    /// The level GL runs at — the shipped cap (`cmp r0,#0x62` @0x14fb24 is `> 98`).
    pub const LEVEL: u8 = 99;
    /// D1: the stat points one GL gives (a level-up's three).
    pub const POINTS_PER_GL: u32 = 3;
    /// The top GL.
    pub const MAX_GL: u32 = 250;
    /// The first GL step = LV 99's own next EXP, `L³ − L² + 80L` at 99 (`a.m()V` 0x15018e..0x1501d2).
    pub const BASE_EXP: u32 = 968_418;
    /// D3 (2026-10-22, user: +2 %/GL): each GL's step grows by 2 % of the first — 19,368 (968,418 × 2 / 100, floored).
    pub const STEP_EXP: u32 = Self::BASE_EXP * 2 / 100;
    /// GL-ups one call may make (a huge reward rolls over a few frames, never a runaway loop).
    pub const ROUNDS: u8 = 8;
    /// The character's words, from the level word (`fb + 4 + 0x5c`): stat points `a.c:S` 0x64 · EXP `a.c:I` 0x74 ·
    /// next `a.f:I` 0x80 (absolute; the field block's +4 header folded into the base).
    pub const FIELD_BASE: u32 = 4 + 0x5c;
    pub const F_POINTS: u32 = 0x64 - 0x5c;
    pub const F_EXP: u32 = 0x74 - 0x5c;
    pub const F_NEXT: u32 = 0x80 - 0x5c;
    /// The stat points' ceiling (`a.c:S` is a short).
    pub const POINTS_CAP: u32 = 32_767;

    /// The EXP from GL `gl` to `gl + 1` (`gl` 1..=MAX; 0 reads as 1).
    pub const fn gnext(gl: u32) -> u32 {
        let g = if gl == 0 { 1 } else { gl };
        Self::BASE_EXP + Self::STEP_EXP * (g - 1)
    }
}

/// ★★ 드랍률 (2026-10-16, user: *"인게임 내에 난이도 조절 옵션 위에 드랍률을 낮추거나 올리는거 옵션 … 낮음,보통,높음이고
/// 지금 현재가 높음이야"*) — the equipment drops, thinned on the way to the ground.
///
/// Every item a monster leaves goes through ONE method: `bf.a(BBBB)V(x, y, type, id)` @0x12d37c, the stage
/// putting `{x, y, type, id, 1, 120}` on its ground list. Its five call sites are all in the death routine
/// `bd.q()V` (the table drop, 제련석, the potions, the card, HELL's potion), and the HELL/guardian bonus pass
/// ([`HellDropPatch`]) calls it through the same cell — so word 0 of its definition record (`0x185644 + 415 ×
/// 0x1c` = **0x1883a8**; the virtual helper `0x16917c` reads it at call time and `bx`es there with `lr` =
/// the caller) is the single place every drop passes. A host block there looks at the type (`[sp]`, the
/// caller's outgoing argument word): an EQUIPMENT type (0..=[`Self::EQUIP_MAX`] — weapons, armour, shield,
/// necklace, ring) is kept with probability KEEP/65536 by the block's own 32-bit LCG (high half), anything
/// else always passes. A dropped one returns at once (`bx lr` — the caller owns its argument area and reads
/// nothing back from a `void`), a kept one jumps on to the body with r1..r3 and the stack as they came.
///
/// 높음 is the shipped rule and the default: the block is installed lazily, the first time the setting asks
/// for less, and KEEP = 0x10000 thereafter is the identity. 보통 = ¼ · 낮음 = 1/16 of every equipment slot's
/// chance — in every mode, the HELL and guardian bonus passes included. Gold, 제련석, potions, quest
/// items, materials (인장 · 조합서 · 결정 …) are untouched. Nothing is written to a record or a save.
///
/// ```text
/// ldr r0,[sp] · lsls #24 · lsrs #24 · cmp r0,#12 · bhi pass          ; not equipment: the body
/// push {r1,r2} · adr r1,words · ldr r0,[r1,#KEEP] · lsrs r2,r0,#16 · bne keep
/// ldr r0,[r1,#STATE] · r0 = r0·A + C · str r0,[r1,#STATE] · lsrs r0,#16
/// ldr r2,[r1,#KEEP] · cmp r0,r2 · blo keep
/// SKIPPED += 1 · pop {r1,r2} · bx lr                                  ; no drop
/// keep: KEPT += 1 · pop {r1,r2}
/// pass: ldr r0,=body|1 · bx r0
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DropRatePatch {
    /// The RUNTIME method-definition record of `bf.a(BBBB)V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
}

impl DropRatePatch {
    /// The block: 31 halfwords of code, four data words, three pool words.
    pub const BLOCK_LEN: u32 = 0x60;
    /// The last equipment type: 0..=4 weapons · 5 투구 · 6 갑옷 · 8 장갑 · 9 신발 · 10 방패 · 11 목걸이 · 12 반지
    /// (7 is the table's "nothing" and never reaches the call) — the host's gear line elsewhere too (`h2_split_allowed`).
    pub const EQUIP_MAX: u8 = 12;
    /// The data words, from the `words` label: the keep threshold (0..=0x10000), the LCG state, and the two
    /// counters the probe and the log read — equipment drops refused and passed since the install.
    pub const KEEP: u32 = 0x0;
    pub const STATE: u32 = 0x4;
    pub const SKIPPED: u32 = 0x8;
    pub const KEPT: u32 = 0xc;
    /// The LCG (Numerical Recipes' `ranqd1` constants); the HIGH half is the draw.
    pub const LCG_A: u32 = 1_664_525;
    pub const LCG_C: u32 = 1_013_904_223;
}

/// ★★ 의상 아바타 A4 (2026-10-01, user: *"노말보상으로 이안시리즈 아이템을 … 하드코어 클리어
/// 보상으로는 율리케 시리즈"*) — the moment a game mode is CLEARED, counted by the host.
///
/// ```text
/// ag.y()V @0x114e60 (instance, on the ag singleton):
///   ag.j += ag.a()B ; clamp 100         (달성도)
///   if ag.i:B <= 1: ag.i += 1           (0 NORMAL → 1 HARDCORE → 2 HELL; a HELL clear stays 2)
///   bq.c() · HP/SP full · ag.z() · ag.u() · a.a(a.r) · a.f()
/// ```
/// Its ONE caller is the event dispatcher `ag.g()V`, event 9 (world entry, jump table
/// @0x16fc80 entry 9 → 0x1118c2), and only when the entry kind `ag.e:B` @0x193abc is 2 — the
/// next round after an ending (0 = 새로하기 → `ag.x()V`, 1 = 이어하기 → `ag.r()V`). So a call of
/// `ag.y()V` IS a clear, the HELL clear included, whose mode byte does not move (a watch on
/// `ag.i` would miss it). Same lever as [`ExpRatePatch`]: word 0 of the method's definition
/// record (`0x185644 + 126 × 0x1c` = **0x18640c**, word 0 = `0x8f51` + 0x10bf10 = `0x114e61`)
/// → a host block that counts the call and keeps `ag.i` as it was BEFORE the body moves it,
/// then jumps on to the body. The dispatcher calls through the direct helper (`bl 0x169160`
/// @0x111946), which reads word 0 at call time.
///
/// ```text
/// +0x00  a306  add  r3, pc, #24      ; r3 = block + COUNT
/// +0x02  681a  ldr  r2, [r3, #0]
/// +0x04  3201  adds r2, #1
/// +0x06  601a  str  r2, [r3, #0]     ; COUNT += 1
/// +0x08  4a02  ldr  r2, [pc, #8]     ; +0x14 = &ag.i
/// +0x0a  6812  ldr  r2, [r2, #0]
/// +0x0c  605a  str  r2, [r3, #4]     ; MODE = ag.i (the mode just cleared)
/// +0x0e  4802  ldr  r0, [pc, #8]     ; +0x18 = body | 1
/// +0x10  4700  bx   r0
/// +0x12  46c0  nop
/// +0x14  .word 0x193c4c (ag.i)  +0x18 .word 0x114e61  +0x1c COUNT  +0x20 MODE
/// ```
/// `r0` is the helper's method pointer (the body overwrites it at its 4th instruction), `r2`
/// and `r3` are written by the body before it reads them, `r1` (this) and `lr` are untouched.
/// Nothing in the game changes: the block only counts. The host polls COUNT and pays the
/// mode's reward to the character in the world (`avatar_state::AvatarBook::clear_reward`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClearHookPatch {
    /// The RUNTIME method-definition record of `ag.y()V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
    /// `ag.i:B` — the game mode (0 NORMAL · 1 HARDCORE · 2 HELL), a static slot.
    pub mode_slot: u32,
}

impl ClearHookPatch {
    /// Ten halfwords of code, two literals, the two words the host reads.
    pub const BLOCK_LEN: u32 = 0x24;
    /// The `&ag.i` literal.
    pub const MODE_LIT: u32 = 0x14;
    /// The `body | 1` literal.
    pub const BODY: u32 = 0x18;
    /// How many times `ag.y()V` has run since the install.
    pub const COUNT: u32 = 0x1c;
    /// `ag.i` as the last run found it, before the body moved it.
    pub const MODE: u32 = 0x20;
}

/// ★★ 2026-10-02 (user: *"스킬 퀵슬롯 페이지전환을 했다가 다시 돌아오면 스킬 쿨타임이 원복되어있어"*)
/// — the game's cooldown TICKS, counted by the host.
///
/// ```text
/// ag.i()V @0x1120a8:   for i in 0..=3: if ag.a[i] >= 0: ag.a[i] -= 1
/// ```
/// Its one caller is the character update `a.g()V` (@0x14de44), and only in the field (state
/// word 3) on a frame the hit-stop counter `a.j` is not running — so wall time, painted
/// frames and the game-speed option all disagree with it. The counters are per SLOT, and the
/// host's plate window moves skills in and out of the four slots: a skill taken off the bar
/// keeps its counter in `quickpage::Window::park`, stamped with this count, and is seated
/// again with `counter − (count now − count then)`. Same lever as [`ClearHookPatch`]: word 0
/// of the method's definition record (`0x185644 + 99 × 0x1c` = **0x186118**, word 0 =
/// `0x6199` + 0x10bf10 = `0x1120a9`) → a host block that counts the call and jumps on to the
/// body; the caller goes through the virtual helper (`bl 0x16916e`), which reads word 0 at
/// call time.
///
/// ```text
/// +0x00  a302  add  r3, pc, #8       ; r3 = block + COUNT
/// +0x02  681a  ldr  r2, [r3, #0]
/// +0x04  3201  adds r2, #1
/// +0x06  601a  str  r2, [r3, #0]     ; COUNT += 1
/// +0x08  4801  ldr  r0, [pc, #4]     ; +0x10 = body | 1
/// +0x0a  4700  bx   r0
/// +0x0c  COUNT  +0x10 .word 0x1120a9
/// ```
/// `r0` is the helper's method pointer and `r3` is written by the body's fourth instruction;
/// `r2` is written (@0x1120dc) before the body reads it; `r1` and `lr` are untouched. Nothing
/// in the game changes: the block only counts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CooldownClockPatch {
    /// The RUNTIME method-definition record of `ag.i()V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
}

/// ★★ 2026-10-16 (user: *"엘릭서가 현재 구 아이콘 사용되고있는데 하이포션 아이콘이 포도주 아이콘색상 좀 섞은 듯한 아이콘으로 사용해줘"*):
/// an ITEM's own icon cell. The game draws every item icon with the static `n.a(IIB)V` @0x16388c `(x, y, type)`: one 12 × 12 cell
/// of the item-type strip `cf.q` (`img/glb` frame 17, 34 cells shipped), cell = `type ≤ 7 ? type : type − 1` — so the icon is
/// the TYPE's, and every type-21 item (마력구 · 망각의 구 · 부활의 구 · 보증서 · 엘릭서) is the blue orb. The asset appends cells
/// after the shipped 34 (`tools_re/elixir_icon.py` — the strip grows by 12 px a cell); this block on `n.a(IIB)V`'s definition
/// word 0 draws a record listed in [`Self::CELLS`] as its DISPLAY type (the appended cell + 1).
///
/// The drawer is not told the record — so the block reads it where the caller holds it. The static call helper `0x16916e`
/// keeps `lr` (the caller's return address) when it jumps to the body, so `lr` names the call site; the six sites that can
/// draw a bag record ([`Self::SITES`] — `tools_re` scan of every `n.a(IIB)V` caller) hold it in a callee-saved register:
/// the inventory / warehouse grid `q.a(II)V` · the picker `k.a(II)V` · `e.a(II)V` in r8, the shop list `p.a(II)V` in fp,
/// the sell list `cl.a(II)V` in sb, the detail window `cd.a(II)V` in `fp.cd.a` ([`AccessoryPatch::DETAIL_RECORD_OFF`]). A record
/// whose type (the drawer's own argument) and id match an entry draws its cell; any other call, site or record is the
/// shipped draw. Nothing is written but the drawer's type argument.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ItemIconPatch {
    /// The RUNTIME method-definition record of `n.a(IIB)V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
}

impl ItemIconPatch {
    pub const BLOCK_LEN: u32 = 0x100;
    /// `(return address of the site's `bl 0x16916e` | 1, where the record is)` — `REC_*` below.
    pub const SITES: [(u32, u8); 6] = [
        (0x0014_6e45, Self::REC_R8), // q.a(II)V — the inventory and warehouse grid
        (0x0016_13cf, Self::REC_R8), // k.a(II)V — the item picker
        (0x0015_a989, Self::REC_R8), // e.a(II)V
        (0x0014_6263, Self::REC_FP), // p.a(II)V — the shop's buy list
        (0x0014_2fed, Self::REC_SB), // cl.a(II)V — the shop's sell list
        (0x0013_f23d, Self::REC_DETAIL), // cd.a(II)V — the item detail window (this = fp)
    ];
    pub const REC_R8: u8 = 0;
    pub const REC_FP: u8 = 1;
    pub const REC_SB: u8 = 2;
    pub const REC_DETAIL: u8 = 3;
    /// `(type, id, display type)` — the display type's strip cell is `display − 1` (≥ 8), appended after the shipped 34.
    /// ★★ 2026-10-20: + 상점주인의 혼 `(21, 8)` → 36 (cell 35: the orb cell hue-rotated to red, `tools_re/elixir_icon.py`).
    /// ★★ 2026-10-21: + 비행서 `(28, 1)` → 37 (cell 36: the 귀환서 scroll cell hue-rotated to blue — user: *"비행서는 귀환서
    /// 아이콘이 파란색위주의 색상으로 변경된 아이콘"*).
    pub const CELLS: [(u8, u8, u8); 3] = [(21, 7, 35), (21, 8, 36), (28, 1, 37)];
    /// The shipped strip's cells (`img/glb` frame 17 is 12 × 408).
    pub const SHIPPED_CELLS: u8 = 34;
}

impl CooldownClockPatch {
    /// Six halfwords of code, the count, one literal.
    pub const BLOCK_LEN: u32 = 0x14;
    /// How many times `ag.i()V` has run since the install (wrapping).
    pub const COUNT: u32 = 0x0c;
    /// The `body | 1` literal.
    pub const BODY: u32 = 0x10;
}

/// ★★ 2026-10-12 (user: *"hero3처럼 마지막에 실행했던 세이브 파일을 자동으로 선택되는 기능 … 3번쨰 세이브 슬롯에서 세이브 후에
/// 메인메뉴 나갔다가 다시 이어하기하면 3번째 세이브슬롯이 자동으로 포커싱 … 메인메뉴나가자마자 ok막 연타해도 3번째가 실행"*):
/// the slot screen opens on the slot last played.
///
/// The slot screen `be` is built by `be.<init>(Lj;B)V`, which — for every mode but 새로하기 (10) — calls `be.a()V`
/// @0x1288d8: the first-save finder, `for i ≤ 4 (#27's five) { if ag.a(B)Z(j.h) break; j.h++ }` from 0. That is the
/// ONLY place the screen's cursor gets a value before the player's keys, and it runs inside the game's own key
/// handler — so a host poll after the tick would lose to a second OK queued in the same tick (hero3 161차 had to cut
/// its input drain for exactly that). The lever is [`CooldownClockPatch`]'s: word 0 of `be.a()V`'s RUNTIME
/// definition record → a block that runs the shipped finder, then — when the host's [`Self::LAST`] word names a slot
/// inside the screen (`< j.i`) that holds a save (`ag.a(B)Z`, the finder's own call through its own cell) — writes
/// `j.h ← LAST`. Otherwise the shipped cursor stands (no record, an emptied slot, a three-row screen).
///
/// The host keeps LAST = the save slot (`ag.c:B`) the player last entered the world from (a load or a new game) and
/// `wie_save_last.txt`, so it survives a restart. Nothing is written to a save.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotFocusPatch {
    /// The RUNTIME method-definition record of `be.a()V` (word 0 must read `body | 1`) and its body.
    pub def: u32,
    pub body: u32,
    /// The finder's own cell of `ag.a(B)Z` (a save in that slot?) and the static-call helper it goes through.
    pub cell_has_save: u32,
    pub call_static: u32,
}

impl SlotFocusPatch {
    pub const BLOCK_LEN: u32 = 0x60;
    /// The slot the screen should open on (0-based), or [`Self::NONE`] — written by the host.
    pub const LAST: u32 = 0x5c;
    pub const NONE: u32 = u32::MAX;
    /// The window's cursor `j.h` and row count `j.i` in its field block (`[handle] + 0xc` / `+ 0x10` — #27's `be.b()V`
    /// rewrite stores there).
    pub const CURSOR_OFF: u16 = 0x0c;
    pub const ROWS_OFF: u16 = 0x10;
}

/// ★★ 2026-10-11 (user: *"도적 3마리있는 지역에서 로그로 순간이동 스킬 못쓰게 막으려고하는데 시체 3개중에 가운데 시체 …에서
/// 순간이동 스킬을 쓰면 또 넘어갈 수있어"* → *"그냥 해당 퀘스트 상황에서는 순간이동 스킬 자체 못쓰게 막아버리는게 낫지
/// 않을까?"*) — the rogue's teleports refused while the story fences the player in.
///
/// The scene: 맵 19 (윈터하츠 남부1, the ambush copy) — the three 도적 die, its script sets event 0x10 and warps to
/// 맵 18 (19,8). There the three corpses lie on (19,9) · (21,9) · (20,10) and close the 3-wide pass south; talking to
/// the left one (trigger group 4) sets 0x11 and the quest 「볼프강에게 돌아가기」. The rogue's teleports (일섬's dash over
/// the middle corpse from (20,9), and every skill that lands by a monster) crossed it. So while the corpses stand
/// ([`Self::fenced`]) the host writes MASK = the kinds that move the rogue — 살상연희 10 · 일풍격 15 · 개화 16 always,
/// 일격 5 with 벽력일섬 (일섬) worn, 급소가격 7 with 극사의 목걸이 worn ([`Self::kinds_mask`]) — and 0 everywhere else.
///
/// ★★ 2026-10-15 (user: *"맨왼쪽 도적으로부터 아이템얻어서 퀘스트를 한번 진행하면 그뒤에 다시 일부 위치에서 순간이동 스킬을
/// 쓸수있는데, 진행한번 한뒤에도 순간이동 못하게 해야해"*): the corpses do NOT go when 0x11 is set. Who shows them is the
/// map's LOAD TABLE (the tail after the cutscenes — `decode_map_events.py` prints it), run ONCE per map load by
/// `bf.a([BI)I` @0x12af98 (its one caller is the loader `bf.a([B)V`), first match: 0x11 → hide all six NPCs · 0x10 →
/// hide the three standing 도적 (rec 51, row 12) · 0x0f → hide the corpses (rec 58) · else hide all six. Searching sets
/// 0x11 while the map is up, so the corpses stand — drawn and solid — until the player leaves the map (the way on is
/// north to the town); only the next load hides them. The first gate keyed on 0x11 and opened over standing corpses
/// (probe, S25 slot 4: MASK 0 at the search, corpses drawn 1,700 frames on). So the gate asks the live NPCs
/// ([`Self::CORPSE_REC`] shown, `hero2java::h2_npc_rec_shown`), and the events only when those cannot be read.
///
/// The lever: the rogue's usable check `ay.b(B)Z` (def **0x1874ac**) — CHAINED, the outermost wrapper (the accessory,
/// 일풍격 and rogue blocks already wrap it; installed after the rogue block, NEXT = what word 0 held then). A refused
/// kind returns false: not cast, no SP, no cooldown, the way 일풍격 without a landing is refused.
///
/// ```text
/// +0x00  4b03  ldr  r3, [pc, #12]    ; MASK
/// +0x02  40d3  lsrs r3, r2           ; >> kind (r2 = the byte argument; ≥ 32 or negative → 0)
/// +0x04  07db  lsls r3, r3, #31      ; bit 0 → N
/// +0x06  d401  bmi  refuse
/// +0x08  4802  ldr  r0, [pc, #8]     ; NEXT
/// +0x0a  4700  bx   r0               ; on down the chain, r1 · r2 · lr untouched
/// +0x0c  2000  movs r0, #0           ; refuse:
/// +0x0e  4770  bx   lr               ;   false
/// +0x10  MASK   +0x14 NEXT
/// ```
/// `r3` is not an argument of `ay.b(B)Z` (one byte, in `r2`); every wrapper down the chain loads its own. Nothing is
/// written to a record or a save, and with MASK 0 the block is a jump.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TeleportGatePatch {
    /// The RUNTIME method-definition record of `ay.b(B)Z` (word 0 = a Thumb entry — the body or a wrapper).
    pub def: u32,
}

impl TeleportGatePatch {
    /// Eight halfwords of code, MASK, NEXT.
    pub const BLOCK_LEN: u32 = 0x18;
    /// The kinds refused now — bit `kind`. The host's word.
    pub const MASK: u32 = 0x10;
    /// What word 0 held at the install (`| 1`).
    pub const NEXT: u32 = 0x14;
    /// 윈터하츠 남부1 (the corpses' map; 맵 19 is the ambush).
    pub const MAP: u8 = 18;
    /// The three 도적 are dead (맵 19's script) …
    pub const EVENT_ON: u8 = 0x10;
    /// … and not yet searched (맵 18 trigger group 4's script) — only the fallback when the NPCs cannot be read.
    pub const EVENT_OFF: u8 = 0x11;
    /// The corpses' `npc/npc` record (맵 18 NPCs 3 · 4 · 5; the standing 도적 are 51).
    pub const CORPSE_REC: u8 = 58;
    /// The rogue's kinds that move it: 일격 (→ 일섬) · 급소가격 (→ 극사) · 살상연희 · 일풍격 · 개화.
    pub const KIND_STRIKE: u8 = 5;
    pub const KIND_VITAL: u8 = 7;
    pub const KIND_SLAUGHTER: u8 = 10;
    pub const KIND_ILPUNG: u8 = 15;
    pub const KIND_BLOOM: u8 = 16;

    /// MASK for the fenced scene, given the accessory block's ACTIVE ([`AccessoryPatch`]'s code — 일섬 / 극사 turn
    /// 일격 / 급소가격 into teleports; without them those two only strike).
    pub const fn kinds_mask(accessory: u32) -> u32 {
        let mut m = (1 << Self::KIND_SLAUGHTER) | (1 << Self::KIND_ILPUNG) | (1 << Self::KIND_BLOOM);
        if accessory == AccessoryPatch::ILSEOM {
            m |= 1 << Self::KIND_STRIKE;
        }
        if accessory == AccessoryPatch::GEUKSA {
            m |= 1 << Self::KIND_VITAL;
        }
        m
    }

    /// Is the scene up: this map, event 0x10 on (`flags` = `ag.q`, bit `f & 7` of byte `f >> 3`), and a corpse
    /// shown — `corpses` asked only then: `Some(shown)` from the live NPCs, `None` when they cannot be read (then
    /// 0x11 off stands in for it, the rule before 2026-10-15).
    pub fn fenced(map: u8, flags: &[u8], corpses: impl FnOnce() -> Option<bool>) -> bool {
        let on = |f: u8| flags.get(usize::from(f >> 3)).is_some_and(|b| (b >> (f & 7)) & 1 == 1);
        map == Self::MAP && on(Self::EVENT_ON) && corpses().unwrap_or(!on(Self::EVENT_OFF))
    }
}

/// ★★ 2026-10-05 (user: *"버프스킬 발동되고있는 중에 스킬쿨이 돌아서 다시 사용하면 스킬 지속시간이 다시
/// 시작하는게 아닌거 같아 … 남은 10초만 지속되는거 같아"*) — a buff cast while the same buff is still up
/// RESTARTS its duration.
///
/// ```text
/// g.a(BI)V(id, len) @0x15c09c:   if (!g.a(B)Z(id)) return;          // already in g.b → refused
///                                g.b.addElement(new bm(id, len));
/// g.a(B)Z(id)       @0x15bf60:   for i in size−1..=0: if ((av) g.b[i]).c == id return false; return true
/// ```
/// So a buff whose cooldown (per slot, `ag.a:[I`) runs out before its duration did was cast — SP
/// and cooldown spent, the stance flag set again — and the old `bm` kept counting: the new cast
/// bought only what the old one had left. The callers (`find_method_callers.py 'g.a(BI)V'`):
/// 버서커 `bl.a()Z` ×2 = 30 분노태세 (`a.n`) · 31 반격태세 (`a.o`) — 나이트 `bx.a()Z` ×2 = 34 신의가호 (`a.q`) ·
/// 35 방어의지 (`a.r`) — 로그 `ay.a()Z` = 36 민첩태세 (`a.s`), and 33 맹독감염 through the same cell — and
/// ONE that is not the player's: `bl.a(BI)Z` puts **32 on the struck MONSTER** (`g.i ← 1`, 200
/// frames; `bd.a(Lav;)Z` clears `g.i` at its end). ⛔ So the block refreshes only when `this` is the
/// character (`*char_slot`): a monster's 32 stays as shipped — refreshing it would make the debuff
/// last for ever under repeated hits, a balance change nobody asked for. The monsters' hit spark
/// `bm(33, 4)` is built directly (0x1279d4) and never comes here, and no subclass of `g` overrides it.
///
/// The lever is [`CooldownClockPatch`]'s: word 0 of the method's RUNTIME definition record
/// (`0x185644 + 1046 × 0x1c` = **0x18c8ac**, shipped `0x5018d` + 0x10bf10 = `0x15c09d`) → a host
/// block that walks `this.g.b` the way `g.a(B)Z` does and, on the same id, writes the three words a
/// fresh `bm` would have — `av.b:S` (elapsed) 0, `av.a:Z` (ended) 0, `av.a:I` (length) `len`
/// (≤ 0 → 1, as `bm.<init>(BI)V` clamps) — and returns; otherwise it is a tail call into the body
/// with `r1`–`r3`, `lr` and the stack as the caller left them (`r4`–`r7` pushed and popped first).
/// The stance flags (`a.n/o/q/r/s`) are untouched: the cast set them again and only the buff's END
/// (`g.v()V` → `a.a(Lav;)Z`) clears them — now one full duration after the recast.
/// ★ Clearing the ended flag matters for a buff `bm.a()V` marked ended on the frame of the
/// recast but `g.v()V` has not removed yet — without it the refreshed buff is removed (and its
/// flag cleared) on the next update.
///
/// ★ 2026-10-05 (2) — the BEAST's buffs too (user: *"야수 스킬도 … 버프스킬 남은 지속시간 표시되어야해"*).
/// They are a second adder with the same refusal: **`g.b(BBI)V(type, a, len)`** @0x15c19c (def
/// `0x185644 + 1048 × 0x1c` = **0x18c8e4**, word 0 `0x5028d` + 0x10bf10 = `0x15c19d`) — `if
/// (!g.a(B)Z(type)) return; g.b.addElement(new l(type, a, len))`, the length the caller's
/// STACK word (`[sp]` at entry) narrowed to a signed byte by `l.<init>(BBI)V`. Callers: `bn.c(B)V`
/// (the beast's 1 빛의수호 · 5 얼음장막 onto the character) and `bn.a(BBBI)V` (암살자's status 3 onto a
/// MONSTER — left as shipped, the same `*char_slot` test). `l` keeps `av.a()V`, so it ends at
/// `elapsed ≥ a:S · a:I` (`a:S` = the animation's length, written by its painter `l.a(II)V`;
/// `a:I = −1` never ends): the refresh writes the same three words, the length unclamped. The
/// second entry sits at [`Self::BEAST_AT`] of the same block. 4 빛의회복 only plays a sound — no buff.
///
/// ★★ 2026-10-06 — the block also SETS three lengths (user: 버프 8종 「쿨타임 1분 · 지속 1분」 → 얼음장막 빼고 →
/// 「만렙 지속 75초」 → 「전부 ×1.25」 = **25 · 37.5 · 50 · 62.5 · 75 s** for levels 1..5, the cooldowns 60 s —
/// `GuestBuild::buff_cooldown_minute`). Every buff the character gets passes through here, so the two whose own
/// code has no level to compute from get their length on the way in:
///
/// | | the shipped length | here |
/// |---|---|---|
/// | 30 분노태세 (`g.a(BI)V`, `r3`) | `bl.a()Z` case 7 passes a flat 140 — the cast never reads its level | ~~`r3 ← 250 · (a.p[7] + 1)`~~ ★ 2026-10-07 the TOGGLE: absent → `r3 ← BerserkerPatch::RAGE_ON_FRAMES` (never ends by itself — ON), present → that buff's ended flag ← 1 (OFF: the game's `g.v()V` runs the end handler, `a.n ← 0`) and nothing else |
/// | 1 빛의수호 · 4 빛의회복 (`g.b(BBI)V`, `[sp]`) | `bn.b(B)B`: `15·lv + bn.l[slot]` (+13 · +23 from `b/b.bdf`) CYCLES of the effect's loop (8 · 6 frames) — 11.2 → 35.2 s · 11.4 → 29.4 s | the caller's stack word ← [`Self::beast_cycles`] = ⌈250·(lv + 1) / unit⌉ cycles (63 · 94 · 125 · 157 · 188 and 84 · 125 · 167 · 209 · 250), `lv` = the beast's `bn.k[slot]` (`slot` = `r3`, `bn.g`) — 25.2 · 37.6 · 50 · 62.8 · 75.2 s and 25.2 · 37.5 · 50.1 · 62.7 · 75 s |
///
/// ⛔ The cycles do not fit the byte the game kept them in: `l.<init>(BBI)V` narrows the length to a signed byte
/// (0x1616d4 `lsls r2,#24` · 0x1616d6 `asrs r2,#24`) — 127 cycles = 50.8 s · 38.1 s at most. So the install also
/// takes those two halfwords out ([`Self::widen`], `nop` · `nop`: the word goes in whole) — the identity for every
/// shipped caller (`bn.c(B)V` narrows before it stacks the word, `bn.f()V`'s effect tail and the 4-argument
/// constructor pass small numbers), and the refresh path here no longer narrows either. 5 얼음장막 keeps its own
/// `15·lv/9 + 7` (≤ 15 cycles) — the user left it out. The other lengths are their casts' own (31 · 33 · 34 · 35:
/// `counter_stance_duration` · `VenomPatch` · `blessing_aura_rework` · `iron_wall_rework`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BuffRefreshPatch {
    /// The RUNTIME method-definition record of `g.a(BI)V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
    /// The RUNTIME definition record of `g.b(BBI)V` (the beast's buffs) and its body.
    pub beast_def: u32,
    pub beast_body: u32,
    /// The character's static slot (`ag.a:La;`, the CHAR anchor) — only ITS buffs are refreshed.
    pub char_slot: u32,
    /// ★ 2026-10-06: `(site, as shipped, patched)` — `l.<init>(BBI)V`'s narrowing of the length, taken out.
    pub widen: [(u32, u16, u16); 2],
    /// Absolute field offsets: the character's `a.p:[B` (skill levels) and `a.a:Lbn;` (the beast), the beast's
    /// `bn.k:[B` (its slots' levels).
    pub skills_off: u32,
    pub beast_off: u32,
    pub beast_levels_off: u32,
}

impl BuffRefreshPatch {
    /// Two entries (`hero2java::h2_buff_refresh_block`): the player's at 0, the beast's at
    /// [`Self::BEAST_AT`], each about fifty halfwords, a literal or two and (the beast's) two
    /// five-byte tables.
    pub const BLOCK_LEN: u32 = 0x120;
    pub const BEAST_AT: u32 = 0x80;
    /// ★ 2026-10-06: the lengths this block sets. Frames a level (and the base): `LEN_STEP · (lv + 1)`.
    pub const LEN_STEP: u32 = 250;
    /// 분노태세's buff id — the toggle's (★ 2026-10-07, [`BerserkerPatch`]).
    pub const RAGE_ID: u8 = 30;
    /// The beast's 빛의수호 and 빛의회복 (`t/bsk.tdf` kinds) and their effect loops in frames (`l.a(II)V` writes the
    /// length into `a:S` — measured: 8 and 6 on every form that has them, 얼음장막 9).
    pub const BEAST_GUARD: u8 = 1;
    pub const BEAST_REGEN: u8 = 4;
    pub const fn beast_unit(kind: u8) -> u32 {
        if kind == Self::BEAST_GUARD { 8 } else { 6 }
    }
    /// The cycles a beast buff gets at a level (1..=5): the frames rounded UP to whole loops (never under 75 s at 5).
    pub const fn beast_cycles(kind: u8, lv: u8) -> u32 {
        let unit = Self::beast_unit(kind);
        (Self::LEN_STEP * (lv as u32 + 1) + unit - 1) / unit
    }
    /// `g.b:Ljava/util/Vector;`, absolute (`ad` is 0x34 bytes, `g.b` its own +4) — the buff list.
    pub const BUFFS_OFF: u32 = 0x38;
    /// `av` (the buff's base class, no parent), absolute: `c:B` the id, `b:S` the frames elapsed,
    /// `a:I` the length in frames, `a:Z` ended. Every one a word in the guest (B/S sign-extended).
    pub const AV_ID_OFF: u32 = 0x00;
    /// `a:S`: `bm`'s animation unit (not a multiplier) — `l`'s animation length, the multiplier
    /// of its `a:I` (`av.a()V`: ends at `elapsed ≥ a:S · a:I`).
    pub const AV_UNIT_OFF: u32 = 0x04;
    pub const AV_ELAPSED_OFF: u32 = 0x08;
    pub const AV_LEN_OFF: u32 = 0x0c;
    pub const AV_ENDED_OFF: u32 = 0x10;
}

/// ★★ 의상 아바타 A3 (2026-09-30) — the EQUIPMENT window's cursor frame, switched off while
/// the host's avatar side has the focus (the slot right of the character, or the costume
/// window over the panel). Without it two cursors would stand in one window: the game's
/// 11 × 11 frame on the cell it last had, and the host's on the avatar slot.
///
/// `cc.b(II)V` @0x13ed6c (virtual, v=30, one per window class) draws that frame and nothing
/// else — `k.a(IIII)V` at the cell of `j.h`, with its own copies of the cell origins (the
/// last two sites of [`GuestBuild::equip_cell_shift`]). The lever is the one [`ExpRatePatch`]
/// uses: word 0 of its runtime definition record (`0x185644 + 683 × 0x1c` = **0x18a0f8**,
/// shipped `0x13ed6d`), which both dispatch helpers read at call time, points at a 20-byte
/// host block:
/// ```text
/// block+0   4802  ldr r0, [pc, #8]    ; block+12 = HIDE
/// block+2   2800  cmp r0, #0
/// block+4   d101  bne block+10
/// block+6   4802  ldr r0, [pc, #8]    ; block+16 = body | 1
/// block+8   4700  bx r0               ; the frame, as shipped
/// block+10  4770  bx lr               ; hidden: nothing drawn
/// block+12  .word HIDE                ← the only word the host ever changes
/// block+16  .word 0x13ed6d
/// ```
/// `r0` is the helper's method pointer; the body's fifth instruction (`ldr r0,[pc,#0x80]`)
/// overwrites it before anything reads it, and `r1`–`r3` (this, x, y) and `lr` are left
/// alone. The window repaints only when its `j.e` is set, so every flip of HIDE is written
/// with the chain's repaint flags (`h2_repaint_writes`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EquipCursorPatch {
    /// The RUNTIME method-definition record of `cc.b(II)V` (word 0 must read `body | 1`).
    pub def: u32,
    /// Its body.
    pub body: u32,
}

impl EquipCursorPatch {
    pub const BLOCK_LEN: u32 = 0x14;
    /// Offset of the HIDE word (0 = the frame is drawn).
    pub const HIDE: u32 = 0x0c;
    /// Offset of the `body | 1` word.
    pub const BODY: u32 = 0x10;
}

/// ★★ 세이브 슬롯 5 (2026-09-27, user: *"세이브 슬롯을 3개에서 5개로 늘릴 수 있어?"*) — the
/// game's own slot screen, saver and loader over FIVE slots instead of three.
/// HERO2_GUEST_RE.md §8.27.
///
/// ── WHERE "THREE" LIVES (every site read in the disassembly) ─────────────────────────
/// | | |
/// |---|---|
/// | `ag.a:[Ljava/lang/String;` @0x1939ec | `{"/g1","/g2","/g3"}` — `ag.<clinit>` builds it from string constants (pool entries 14..16). Read by the saver `ag.g(B)V`, the loader `ag.a(BZ)V`, the preview loader `ag.a(B)[B` and `bs.i()V`, always as `names[slot]` |
/// | `ag.b:[Ljava/lang/String;` @0x1939fc | `{"/n1","/n2","/n3"}` (pool 139..141) — `ag.f(B)V` (save) · `ag.h(B)V` (load) · `bs.i()V` |
/// | `ag.k()V` @0x112ad8 | the slot PREVIEWS: `ag.a:[Object ← new Object[3]` (**0x112b0c**), `for i ≤ 2` (**0x112b20**) `a[i] = ag.a(B)[B(i)`, `ag.b:B` = how many are non-null. Called by the title's constructor path `t.a()Lt;` every time the title is entered |
/// | `be.<init>(Lj;B)V` @0x12886c | the slot screen: `super(parent, 3)` (**0x12887e**) = `j.i`, the LEFT/RIGHT wrap |
/// | `be.a()V` @0x1288d8 | skip to the first save: `for i ≤ 2` (**0x1288e8**) |
/// | `be.a(II)Z` @0x128948 | in 이어하기 (mode 1) a LEFT/RIGHT that lands on an empty slot steps on, `% 3` — UNROLLED to two more steps, which is exactly enough for three slots and not for five. **0x1289c4..0x128a07** becomes a bounded loop over five (entry and the tail `0x128a58` the 새로하기 popup path jumps to are untouched; nothing else branches into the block — image-wide scan) |
/// | `be.b()V` @0x128dcc | 새로하기 → class pick (mode 11) in the SAME window, whose rows are the three CLASSES: `j.i` must go back to 3 there. The putfield `j.h ← 0` (**0x128df8..0x128e03**) becomes two direct stores `j.h ← 0` · `j.i ← 3` |
///
/// Everything else is per slot already: the preview painter indexes `ag.a[j.h]`, the save
/// slot `ag.c:B` is the cursor, the files are `names[slot]`. `bs.i()V`'s "no /cf → wipe
/// slots 0..2" is left alone (it only ever runs without the archive's `P/cf`).
///
/// ── WHY THE HOST HAS TO INITIALIZE `ag` ITSELF ───────────────────────────────────────
/// There is no `/g4` in the image, so the longer tables are host-made
/// (`Emulator::new_java_string_array`, written with [`JavaWrite::SaveFileNames`]). But
/// `ag.<clinit>` and the first `ag.k()` run in ONE tick — the key that leaves the logo
/// touches `ag` for the first time (`n.keyNotify` reads the state word) and builds the
/// title right there (probe: both statics 0 through f=320, both set at f=321). So the host
/// runs the initializer early (`Emulator::init_java_class("ag")` — pure static data plus
/// `new ag()`, whose constructor is `Object.<init>`), replaces the two tables, then patches
/// the code, all during the logo. The JVM marks `ag` initialized, so the guest's own
/// `register_class` for it returns at `has_class` and `<clinit>` never runs twice.
///
/// ⛔ ORDER. `ag.k` over five with three names is an `ArrayIndexOutOfBounds` at the title;
/// the slot screen over five with three previews is one at the first ◀▶. So: names first,
/// then [`Self::table_sites`], then [`Self::screen_sites`] only while the previews are
/// null or already five long. A title built before the host got there (a key within the
/// first frames) keeps three previews, and the screen patch waits for the next title.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SaveSlotsPatch {
    /// The class whose `<clinit>` builds the tables — initialized by the host.
    pub class: &'static str,
    /// `ag.a:[Ljava/lang/String;` — the `/g<slot>` names (캐릭터·퀵바·재료가방 half).
    pub prog_names: u32,
    /// `ag.b:[Ljava/lang/String;` — the `/n<slot>` names (가방 half).
    pub char_names: u32,
    /// `ag.a:[Ljava/lang/Object;` — the slot previews `ag.k()V` builds, null outside the
    /// title (`t.a()V` clears it).
    pub previews: u32,
    /// `ag.k()V`'s two sites — safe once the names are [`Self::SLOTS`] long.
    pub table_sites: &'static [(u32, u16, u16)],
    /// The slot screen's sites — safe once the previews are null or [`Self::SLOTS`] long.
    pub screen_sites: &'static [(u32, u16, u16)],
}

impl SaveSlotsPatch {
    /// Slots as shipped.
    pub const SHIPPED: u8 = 3;
    /// Slots with the patch.
    pub const SLOTS: u8 = 5;
    /// The `/g<slot>` half's file prefix, as the game's own names spell it.
    pub const PROG_PREFIX: &'static str = "/g";
    /// The `/n<slot>` half's file prefix.
    pub const CHAR_PREFIX: &'static str = "/n";
}

/// ★★ hero2's SKILL POWER (#17, 2026-09-24) — every class skill hit does ×1.5.
/// User (2026-09-24): 「모든 직업 스킬 상향 … 야수 대비 약하다」, method delegated, then
/// 「데미지 ×1.5 + 쿨타임 절반」.
///
/// ── WHERE A SKILL'S DAMAGE IS DECIDED ──────────────────────────────────────────────
/// Every character hit on a monster ends in `bd.a(La;BIZBBBB)Z` @0x127738: hit roll,
/// `damage − defence` (★ 2026-10-09 (11) corrected: the WHOLE `cq.c` — `defence/2` only under the
/// monster's `g.i:Z` debuff, 0x12785a; HERO2_GUEST_RE.md §8.61), the element table, then ONE call into the attacker's class:
/// `<job>.a(BILbd;)I` with `r2` = the `a.p` index of the attack and `r3` = the damage so
/// far. That method IS the per-skill multiplier (HERO2_GUEST_RE.md §8.11):
///
/// | job | index | shipped |
/// |---|---|---|
/// | 버서커 `bl` | 4 휘두르기 · 6 가르기 | `d + d/(8−lv)` = ×1.14..1.33 |
/// | | 5 몸통치기 · 9 폭풍강타 | `d + d/(6−lv)` = ×1.2..2.0 |
/// | | 10 사자후 | a level/luck roll: a kill (the monster's HP back), or `d·(lv+3)/3` on one class (bosses?); `d` when the roll fails |
/// | | 8 반격태세 | a FLAT `lv·14 + 20`, not a function of `d` |
/// | | 7 분노태세 | `d` (a stance) |
/// | 나이트 `bx` | 4 검기 · 5 둔기강타 | `d·(lv+4)/3` · `d·(lv+7)/3` |
/// | 로그 `ay` | 5 일격 | `d + d/(8−lv)` |
/// | | 9 단검난무 · 10 번개난타 | `d·(lv+6)/3` |
/// | all | the 기본공격 stages `0 .. a.i[class]`, and every other index | `d` |
///
/// ── THE REDIRECT ───────────────────────────────────────────────────────────────────
/// The third patch machine (after #13's call-site trampoline and #18's in-place rewrite),
/// the one [`ExpRatePatch`] introduced: word 0 of each job's method DEFINITION is the
/// body the virtual dispatch helper jumps to, read at call time. Point it at a host
/// block that scales the damage ARGUMENT and jumps on (one [`Self::JOB_BLOCK`] per job,
/// the `body | 1` literal at [`Self::BODY`] in both shapes):
///
/// ```text
/// +0   2a0L  cmp  r2, #L          ; L = 기본공격 stages of this job (5 · 4 · 3)
/// +2   db01  blt  +8              ; a combo stage: leave it alone
/// +4   1058  asrs r0, r3, #1      ; r0 = d / 2
/// +6   181b  adds r3, r3, r0      ; d = d + d/2
/// +8   4808  ldr  r0, [pc, #32]   ; +44 = body | 1
/// +10  4700  bx   r0
/// +12  (zero to +44)
/// +44  .word body | 1
/// ```
/// Scaling the INPUT is ×1.5 for every formula that is linear in `d` — every skill hit
/// but two: 사자후's outright kill returns the monster's HP (still a kill), and 반격태세
/// ignores `d` (shipped: a flat `lv·14 + 20`; since 2026-09-26 the character's own attack
/// `a.a()I` — [`GuestBuild::counter_stance_attack`] — so neither multiplier reaches the
/// counter). `r0` is free on entry: each body loads it at its 3rd–5th instruction before
/// reading it. `r1` (this), `lr`, the stack argument are untouched, so the body returns to
/// the hit routine exactly as before. `d ≥ 0` here (the hit routine branches away on a
/// negative one before this call), so `asrs` is `/ 2`.
///
/// ── ★ 2026-09-26: 버서커 「근력단련」 = SKILL DAMAGE per level ─────────────────────────
/// User: *"콤보스킬이 sp를 소모하지 않으면서 버서커의 근력단련스킬이 리뉴얼이 필요할거같아"* →
/// *"스킬데미지 증가로 바꿀수도있어?"* → (the same evening, from +10 %) *"근력단력 스킬데미지 증가를
/// 레벨당 3%로 바꿔줘"*. The passive (`a.p[4 + 8]`, max level 3 — `bl.a(I)B` answers 3 for
/// 11..=14 but 13) used to take `7 × level` off the basic hit's SP, which #16 turned into a
/// GAIN — so every point in it cut the berserker's SP income (level 3 = 1 SP a hit below
/// LV 38). [`GuestBuild::strength_training_sp_off`] zeroes that term, and the job whose tuple
/// names a passive gets the skill-damage half here — `d += d·lv·246 >> 13` (3.003 % a level,
/// [`Self::BOOST_MUL`] / [`Self::BOOST_SHIFT`]) after the ×1.5, read straight out of `this`:
///
/// ```text
/// +0   2a04  cmp  r2, #4              +24  4358  muls r0, r3          ; lv · d
/// +2   db10  blt  +38                 +26  b410  push {r4}
/// +4   1058  asrs r0, r3, #1          +28  24f6  movs r4, #246
/// +6   181b  adds r3, r3, r0          +30  4360  muls r0, r4
/// +8   6808  ldr  r0, [r1]    ; this's field block      +32  bc10  pop  {r4}
/// +10  3080  adds r0, #0x80           +34  1340  asrs r0, r0, #13     ; ≈ lv·d·3 %
/// +12  3080  adds r0, #0x80           +36  181b  adds r3, r3, r0
/// +14  6bc0  ldr  r0, [r0, #0x3c] ; +4 + a.p's 0x138 = the a.p handle
/// +16  6800  ldr  r0, [r0]        ; the array's block (length +4, elements +8)
/// +18  7d00  ldrb r0, [r0, #20]   ; a.p[12]
/// +20  2803  cmp  r0, #3          ; the passive's cap
/// +22  d806  bhi  +38             ; unsigned — a locked −1 reads 255 and leaves d alone
/// +38  4801  ldr  r0, [pc, #4]  · +40 4700 bx r0 · +42 pad · +44 .word body | 1
/// ```
/// Why 246 / 2^13 and not /2^11: `movs` takes an 8-bit immediate, and at `>> 11` the nearest
/// multipliers are 61 (2.98 %) and 62 (3.03 %); at `>> 13`, 246 is 3.003 % — a 1500 hit gets
/// exactly +45 a level. `lv·d·246` stays far inside 32 bits (lv ≤ 3, `d` under 2.9 M).
/// `r4` is callee-saved, so it goes round the multiply on the stack and is back before
/// the jump (the body finds `sp` and the stack argument where the hit routine left them).
/// The loads are the same shapes
/// `hero2java.rs` reads with (`h2_fields` / `h2_array`), and `this` is always the live
/// `bl` whose own body reads `a.p` a few instructions later, so none of them can be null.
/// ⛔ Only the SKILL indices come through the boost — the 기본공격 stages take the
/// `blt` straight to the jump, as before; the stat passive 「힘단련」 is what scales those.
///
/// ── ★ 2026-09-26: 버서커 「반격태세」 = the attack × a LEVEL factor ─────────────────────
/// User: *"반격시 나의 공격력만큼"* (→ [`GuestBuild::counter_stance_attack`], case 8 returns
/// `a.a()I`) and then, told the level no longer did anything, *"너가말한대로 적용하고"* —
/// 100 % at level 1, +10 % a level, 140 % at 5. The body's case 8 has no room to read a level
/// as well, so the job whose tuple names a COUNTER skill gets a prefix here: for that kind
/// (8) it reads `a.p[8]` the way the boost reads `a.p[12]` and hands the body, in place of
/// `d` (which case 8 ignores — the counter call's attack is 0), the factor
/// `(lv + 9) · 205` = 2050 · 2255 · 2460 · 2665 · 2870 = 100.1 · 110.1 · 120.1 · 130.1 ·
/// 140.1 % of [`Self::COUNTER_ONE`] (2048). The body multiplies `a.a()I` by it and shifts by
/// [`Self::COUNTER_SHIFT`]; a `d` of 255 or less — this block missing, or a level byte
/// above [`Self::COUNTER_MAX_LEVEL`] (the prefix then leaves `d` alone) — counts as
/// exactly 100 %. The counter skips the ×1.5 and 근력단련: it is the attack, as asked.
/// ```text
/// +0   2a08  cmp  r2, #8        +16  2805  cmp  r0, #5
/// +2   d10b  bne  +28 (below)   +18  d816  bhi  done
/// +4   6808  ldr  r0, [r1]      +20  3009  adds r0, #9
/// +6   3080  adds r0, #0x80     +22  23cd  movs r3, #205
/// +8   3080  adds r0, #0x80     +24  4343  muls r3, r0     ; r3 = (lv + 9)·205
/// +10  6bc0  ldr  r0, [r0,#0x3c] +26 e012  b    done
/// +12  6800  ldr  r0, [r0]
/// +14  7c00  ldrb r0, [r0, #16] ; a.p[8]
/// +28  (the ×1.5 / 근력단련 code above, shifted down 28) … done: ldr r0,[pc] · bx r0
/// ```
///
/// ⛔ The beast does NOT come through here (its skills build `ce` effects that call
/// `bd.a(IBB)V` with their own damage) — this is class skills only, which is the request.
/// ⛔ Nothing is written to the character or the save: remove the block and the def word
/// and the shipped game is back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SkillPowerPatch {
    /// `(def, body, 기본공격 stages, skill-damage passive, counter skill)` for `ay` (로그) ·
    /// `bl` (버서커) · `bx` (나이트). The passive is the `a.p` index whose level adds
    /// [`Self::BOOST_MUL`]` >> `[`Self::BOOST_SHIFT`] of the damage per level — 버서커
    /// 근력단련 = 12. The counter skill is the `a.p` index whose calls get the level factor in
    /// place of `d` — 버서커 반격태세 = 8.
    pub jobs: [(u32, u32, u8, Option<u8>, Option<u8>); 3],
    /// `a.p:[B`'s ABSOLUTE offset in the character's field block (the same field as
    /// [`Hero2Java::char_skill_levels_off`]) — where the boost reads the passive's level.
    pub skills_off: u32,
    /// ★ 2026-09-27: per-skill EXTRA multipliers after the ×1.5, one list per job (same
    /// order as `jobs`): `(a.p index, mul, buffed)` → `d += d·mul >> EXTRA_SHIFT`, and then,
    /// while the word at [`Self::buff_off`] is set, `d += d·buffed >> BUFF_SHIFT` (0 = none).
    /// The knight's 검기·둔기강타 +30 % and 방패치기 +50 % (user: 「중간」 — *"검기와 둔기강타
    /// 데미지를 상향 … 방패치기도 데미지 올리고"*), and under 신의가호 (the renamed 인내오라,
    /// `a.q:Z`) 둔기강타 ×2 and 방패치기 ×1.5 (*"둔기강타의 공격력이 2배, 방패치기 … 데미지1.5배"*).
    /// `d` is the hit routine's attack-based damage, so every raise scales with the
    /// character's own attack like every other skill hit.
    /// ★ 2026-10-03 저녁 (user: *"신의 가호 중 검기 : lv5기준 대략 공격력의 12배 … 신의가호 중 방패치기 … 7.8배 …
    /// 신의 가호 발도 … 18배"*): 검기 gets a buffed share (69 = ×1.539: Lv5 7.80 → 12.0 × the attack), 방패치기's
    /// 64 → 72 (×1.5625: Lv5 5.0 → 7.81 with the sweep's new level factor) and 발도 (a.p[9]) an entry of its own
    /// with `mul` 0 — no unbuffed raise, the code skips it — and 102 (×1.797: Lv5 10.0 → 17.97).
    pub extras: [&'static [(u8, u8, u8)]; 3],
    /// ABSOLUTE offset of the flag the `buffed` multipliers test — 나이트 신의가호 `a.q:Z`.
    pub buff_off: u32,
}

impl SkillPowerPatch {
    /// One 128-byte block per job, back to back (the berserker's = counter prefix 14 + ×1.5 4 +
    /// boost 15 + jump 2 halfwords; the knight's = ×1.5 4 + three extras 7 · 17 · 17 + jump 2 —
    /// then the literal). ★ 2026-09-27: 0x50 → 0x80 for the knight's buffed extras.
    /// ★ 2026-10-03 저녁: 0x80 → 0xa0 — the knight's four extras are 17 · 17 · 17 · 12 halfwords now
    /// (검기 buffed, 발도 added): ×1.5 4 + 63 + jump 2 = 138 bytes before the literal.
    pub const JOB_BLOCK: u32 = 0xa0;
    pub const BLOCK_LEN: u32 = 3 * Self::JOB_BLOCK;
    /// Offset of the `body | 1` word inside a job's block — the last word, in every shape.
    pub const BODY: u32 = Self::JOB_BLOCK - 4;
    /// The counter factor is `(lv + COUNTER_OFFSET) · COUNTER_MUL`, read against
    /// `COUNTER_ONE = 1 << COUNTER_SHIFT` (the body's shift — `counter_stance_attack`).
    pub const COUNTER_OFFSET: u8 = 9;
    pub const COUNTER_MUL: u8 = 205;
    pub const COUNTER_SHIFT: u8 = 11;
    pub const COUNTER_ONE: u32 = 1 << Self::COUNTER_SHIFT;
    /// 반격태세's top level (`bl.a(I)B`: 4..=10 → 5); a byte above it gets no factor.
    pub const COUNTER_MAX_LEVEL: u8 = 5;
    /// The passive's share of the damage per level is `BOOST_MUL >> BOOST_SHIFT`:
    /// 246 / 8192 = 3.003 % (level 3 = +9 %) — user, 2026-09-26: 「레벨당 3%」 (it was
    /// 205 >> 11 = 10.01 % for the first hour). `movs` takes the multiplier as an imm8.
    pub const BOOST_MUL: u8 = 246;
    pub const BOOST_SHIFT: u8 = 13;
    /// The passive's top level (`bl.a(I)B`); any byte above it leaves the damage alone.
    pub const BOOST_MAX_LEVEL: u8 = 3;
    /// [`Self::extras`]' multipliers are `mul / 2^EXTRA_SHIFT` of the damage on top:
    /// 77 = +30.1 %, 128 = +50 %.
    pub const EXTRA_SHIFT: u8 = 8;
    /// … and the buffed ones `buffed / 2^BUFF_SHIFT`: 128 = ×2, 64 = ×1.5, 72 = ×1.5625,
    /// 69 = ×1.539, 102 = ×1.797.
    pub const BUFF_SHIFT: u8 = 7;
}

/// ★★ hero2's COMBAT host block (2026-09-27) — the behaviours that need code the image has no
/// room for, in ONE host-allocated block, installed all or nothing (HERO2_GUEST_RE.md §8.26).
/// User: *"단검류의 데미지를 … 조금씩 상향 … 나이트도 검과 둔기의 공격력을 조금 상향"* · *"둔기강타와
/// 방패치기 모두 캐릭터가 바라보는 방향으로 3칸"* (the front ROW: straight ahead and both front
/// diagonals) · *"로그의 민첩태세를 일정시간 무적으로"* · 인내오라 → *"신의가호 … 사용중에는 검기가
/// 3방향으로 나가고 … 방패치기의 범위도 앞3칸이아니라 앞6칸"* (→ 정면 + 앞 대각선 둘 · 앞 2줄 × 3칸).
///
/// | piece | mechanism | effect |
/// |---|---|---|
/// | **weapon getters** | def word 0 of `bo.a()S` / `bo.b()S` (min / max + refine — the ONLY two readers of a weapon's attack: `bo.c()S`/`bo.d()S` → combat, and the item-detail painter) → a wrapper that CALLS the body (`blx`) and adds `v · weapon_extra[ch.g] >> 10` | 도검 · 둔기 +10 %, 단검 +20 %, by item TYPE — the items already in a save, the shop and the detail window all agree, nothing is written to a save (the attack is saved per item; an asset change would reach new drops only) |
/// | **front sweep** | def word 0 of `bx.h(B)Z` (방패치기's own body, one caller) → a routine that reads the stage's entity grid itself for the tiles in front (3 = the front row; 방패치기 under 신의가호 6 = two rows — the second row's diagonals have no direction code, so the game's own `a.a(B)Lbd;` cannot name them), keeps what is a monster (class descriptor ∈ [`Self::monster_descs`] — `a.a(B)Lbd;`'s `instanceof bd`), drops repeats (a big monster on two tiles is hit once), then per target the game's hit `bd.a(La;BIZBBBB)Z` with ONE `a.a()I` roll and, for 방패치기, the game's stun `bd.a(I)V(20 + 5·lv)`; 둔기강타's call (`bx.a()Z` case 5) is pointed at `bx.h(B)Z` by its literal | both skills hit the front row; 방패치기 stuns every one it hits |
/// | **검기 ×3** | def word 0 of `ak.<init>(La;I)V` (the 검기 projectile's constructor, one caller) → run it, then under 신의가호 make two more `ak` the way `bx.a()Z` does (`new` at the class slot `*(sl+0x20)+0x104` → the constructor with the knight's facing briefly set to each front diagonal → `stage.bf.d(Lad;)V`) | three waves: ahead and the two front diagonals. A diagonal spawn tile off the map is skipped (the projectile reads its tile unchecked on its first step) |
/// | **검기 sprite** | def word 0 of `ak.b(II)V` (its painter) → a direction 5..=8 paints with the owner's facing | the game has 4 directional frame groups only (`n.a([BIBB[Ljava/lang/Object;ZBBII)B` walks `dir` groups into the sprite data — past 4 it would read beyond it); movement and the monsters' hit shake index the 17-entry `co.a`/`co.b`, which have the diagonals |
/// | **HP guard** | def word 0 of `a.b(I)V` (the ONE HP-delta method) → `if delta < 0 && a.s:Z return` | nothing takes HP while 민첩태세 is up (poison, boss specials) |
/// | **hit guard** | `a.a(Lbd;SB)V` 0x151c04: the `a.r:Z`(방어의지) load becomes `ldr r0,=block; bx r0` (its pool word) → `if a.s` the game's own guard branch 0x151c12, else 0x151c52 | a monster's hit on the invulnerable rogue shows the shield-block mark and does nothing. ★ 2026-10-05: `a.r` no longer guards — 방어의지 is 철괴 (below) |
/// | **철괴** (2026-10-05) | the same routine's `a.m:Z` load 0x151d6a (the game's own ×½ branch) becomes `ldr r0,=block; bx r0` → keep the ×½, then `if a.r` take [`Self::iron_reduction_permille`] of the damage off, back to the min-3 clamp 0x151d7e | a hit on a knight under 철괴 does 10 … 20 % less (+1 % per 오라수련 level; ★ 2026-10-12 +1 % per 5 지력, 50 % at most); the number shown is the reduced one (the popup reads the same `r8`) |
/// | **치유오라 × 지력** (2026-10-12) | the knight cast's heal call: pool word 0x13b7a4 → [`Self::HEAL_AT`], `add r0,sl` 0x13b540 → `blx r0` — the piece scales `r7` (the heal) by [`Self::heal_amount`] and returns the cell | 치유오라 heals × (1 + 지력/100) |
///
/// Every game call from the block goes through the game's own helpers — the VIRTUAL dispatch
/// ([`Self::dispatch`] = 0x16917c: `r0` = cell, `r1` = receiver, with the cells `bx.h(B)Z` and
/// `bx.a()Z` themselves used, so resolution is the game's) and `new` ([`Self::new_instance`]).
/// BLX (register) is the one ARMv5 instruction in it — the arm32 core runs it (`thumb.rs`
/// HiRegBx, `LR = next | 1`).
///
/// ⛔ Install order: block → the six def words → the two pool words → the guard's `bx r0`
/// LAST (the only halfword the hit routine executes differently). A verify that fails puts
/// every def word, pool word and halfword back. Nothing is written to the character or a
/// save; unpatched, the shipped rules are back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CombatPatch {
    /// `(def, body)` of `bo.a()S` and `bo.b()S`.
    pub weapon_getters: [(u32, u32); 2],
    /// Per item type `ch.g` 0..=4 (`itm/00`..`04`): the attack added, in 1/1024.
    pub weapon_extra: [u8; 5],
    /// `(def, body)` of `bx.h(B)Z` — the sweep replaces it.
    pub sweep: (u32, u32),
    /// `(def, body)` of `ak.<init>(La;I)V` and `ak.b(II)V`.
    pub wave_ctor: (u32, u32),
    pub wave_paint: (u32, u32),
    /// Absolute cells (sl + literal): `a.a()I` · `bd.a(La;BIZBBBB)Z` · `bd.a(I)V` · `bf.d(Lad;)V`.
    pub cell_attack: u32,
    pub cell_hit: u32,
    pub cell_stun: u32,
    pub cell_stage_add: u32,
    /// The game's virtual-call helper and its `new`.
    pub dispatch: u32,
    pub new_instance: u32,
    /// `sl + 0x20` (the class table's pointer) and the `ak` entry in it — what `bx.a()Z` hands `new`.
    pub class_table_slot: u32,
    pub wave_class_off: u32,
    /// The five concrete monster classes: `bd` · `al` · `ca` · `ci` · `m`.
    pub monster_descs: [u32; 5],
    /// Static slots: the stage `ag.a:Lbf;`, the map's width `bf.d:B` and height `bf.e:B`.
    pub stage_slot: u32,
    pub map_w_slot: u32,
    pub map_h_slot: u32,
    /// The class static `ag.a:B` (0 버서커 · 1 나이트 · 2 로그) = `Hero2Java::class_slot` — the sweep's kind 9 is
    /// the knight's 발도 and, called by another class, the berserker's 폭풍강타 channel.
    pub class_slot: u32,
    /// Absolute offsets: `bf.a:[[Lad;` (grid[y][x]) in the stage · `ad.d:B` / `ad.e:B` the tile.
    pub grid_off: u32,
    pub tile_x_off: u32,
    pub tile_y_off: u32,
    /// `co.c` / `co.d` (`co.<clinit>` literals): facing 1..=4 → the two front diagonals.
    pub front_diag: [[u8; 5]; 2],
    /// `co.a` / `co.b` for codes 0..=8 (the spawn tile of a diagonal wave).
    pub code_dx: [i8; 9],
    pub code_dy: [i8; 9],
    /// `(pool word, shipped literal, patched literal)`: 둔기강타's `a.f(B)Z` cell literal
    /// → `bx.h(B)Z`'s (both sl-relative; `bx.a()Z` 0x13b664 is its only reader).
    pub bludgeon_pool: (u32, u32, u32),
    /// `(def, body)` of `a.b(I)V`.
    pub hp_delta: (u32, u32),
    /// `(pool word, shipped literal)` the hit routine's `a.r:Z` load reads (0x151c04 only).
    pub guard_pool: (u32, u32),
    /// `(site, shipped, patched)`: `adds r1,r4,#0` → `bx r0`, right after that load.
    pub guard_arm: (u32, u16, u16),
    /// The hit routine's guard branch and its fall-through.
    pub guard_hit: u32,
    pub guard_miss: u32,
    /// ★ 2026-10-05 철괴: `(pool word, shipped literal)` the hit routine's `a.m:Z` load reads (0x151d6a
    /// only — the game's own ×½ branch, every path to the HP loss passes it: the plain hit, the knight's
    /// shield block and the clamp-to-0 path).
    pub iron_pool: (u32, u32),
    /// `(site, shipped, patched)`: `adds r1,r4,#0` → `bx r0`, right after that load.
    pub iron_arm: (u32, u16, u16),
    /// Where the block goes back: the min-3 clamp `mov r2,r8` right after the ×½ (`r8` = the damage).
    pub iron_back: u32,
    /// ★ 2026-10-12 치유오라 × 지력: `(pool word, shipped literal)` the knight cast's heal call reads — `bx.a()Z` case 3
    /// loads the cell of `a.b(I)V` (the HP delta) at 0x13b53c from pool word 0x13b7a4 (its only reader), with the heal
    /// already in `r7` (`adds r7,r5,r4` 0x13b53e). The pool word becomes [`Self::HEAL_AT`]'s entry …
    pub heal_pool: (u32, u32),
    /// … and `(site, shipped, patched)`: `add r0,sl` 0x13b540 → `blx r0` — the piece scales `r7` and hands back the
    /// cell the shipped `add` would have made ([`Self::heal_cell`]), so the call that follows is the game's own.
    pub heal_arm: (u32, u16, u16),
    pub heal_cell: u32,
    /// `a.m:Z` (absolute) — the flag of the game's ×½ that the block keeps.
    pub halve_off: u32,
    /// Absolute field offsets. The character: `g.u` facing · `a.l:B` · `a.m:B` (the weapon
    /// bytes every hit passes on) · `a.p:[B` · `a.q:Z` 신의가호 · `a.s:Z` 민첩태세 · `a.r:Z` 방어의지 (철괴).
    /// The wave: `cb.j:B` its direction · `ak.a:La;` its owner.
    pub facing_off: u32,
    pub weapon_l_off: u32,
    pub weapon_m_off: u32,
    pub skills_off: u32,
    pub blessing_off: u32,
    pub invuln_off: u32,
    pub stance_off: u32,
    pub wave_dir_off: u32,
    pub wave_owner_off: u32,
    /// `ak.a:I` (absolute) — the attack the constructor is handed; 발도's flying slash carries its mark there.
    pub wave_attack_off: u32,
    /// ★ 2026-10-01 (신의가호 이펙트): `(def, body)` of the knight's action → sequence map
    /// `bx.a(B)B` (the identity — the drawn sequence is `kind + 4`; its one caller is the
    /// character's draw `a.a(II)V` @0x14e82e, so nothing but the picture follows it).
    pub seq_map: (u32, u32),
    /// The wave's frame `cb.d:S` (absolute) — what `ak.b(II)V` hands the drawer.
    pub wave_frame_off: u32,
    /// The static call helper and the cell of `n.a(BBBII)V(seq, frame, dir, x, y)` — the
    /// drawer `ak.b(II)V` calls with sequence 17 (`movs r1,#0x11` @0x117b9e, pool 0x117bc0).
    pub static_call: u32,
    pub cell_draw_seq: u32,
    /// ★★ 2026-10-03 발도 · 검기 범위 · 둔기강타 +1 (HERO2_GUEST_RE.md §8.45): `(def, body)` of the
    /// knight's CAST `bx.a()Z` (the per-kind switch — kind 9 has no case, kind 4 spawns the
    /// wave), its SP cost `bx.a(I)I` (kind 9 falls off its 3..=8 table to 0) and its cooldown
    /// `bx.a(I)S` (`bx.a:[S` has six entries — index 6 throws).
    pub cast: (u32, u32),
    pub sp_cost: (u32, u32),
    pub cooldown: (u32, u32),
    /// ★ 2026-10-07 천공의검: `(def, body)` of the knight's max level `bx.a(I)B` (kind 15 tops out at 1).
    pub max_level: (u32, u32),
    /// The game's signed divide (`r0 / r1`, a leaf) — the wave's `atk / (8 − lv)` and 발도's `/ 3`.
    pub divide: u32,
    /// The cell of `br.a(B)V` (the sound) that `bx.a()Z` case 4 uses (`movs r1,#7` after it), and
    /// the cell of `bx.h(B)Z` its 방패치기 case calls — what the cast wrapper calls the sweep by
    /// (the def it resolves to is this block's, so the game's own dispatch lands here).
    pub cell_sound: u32,
    pub cell_sweep: u32,
    /// Absolute field offsets: `g.w:B` the action frame · `a.o:[B` the combo chain · `a.i:B` its
    /// index (`a.o[a.i]` is the kind `bx.a()Z` switches on) · `a.e:I` max SP · the wave's
    /// lifetime `ak.a:B` (tiles left to fly).
    pub action_frame_off: u32,
    pub combo_table_off: u32,
    pub combo_index_off: u32,
    pub max_sp_off: u32,
    pub wave_life_off: u32,
    /// `(site, shipped, patched)`: `ak.b()V`'s `bne 0x117a60` (a monster on the wave's tile →
    /// hit it) → `b 0x117950` (step on) — the wave FLIES but hits nothing; the damage is the
    /// cast's sweep, the wave is the picture.
    pub wave_no_hit: (u32, u16, u16),
    /// ★ 2026-10-08 천공의검: the static `a.n:[B` (the action lengths `cf.a()V` fills from the sequences) — the cast
    /// wrapper stores [`Self::SKY_FRAMES`] into `a.n[15]` on every frame of kind 15 (sequence 19 gives it six).
    pub frames_slot: u32,
    /// ★★ 2026-10-17 쌍월's window: `(def, body)` of the character's update `a.g()V` (the [`Self::UPDATE_AT`] piece counts the
    /// window down there, before the body) · the statics `ag.j:[B` (the quickbar's four values) and `ag.a:[I` (the four slots'
    /// cooldown counters, −1 = ready) — the slot that holds 발도 is the one whose counter the window frees.
    pub update: (u32, u32),
    pub quickbar_slot: u32,
    pub cooldown_slot: u32,
    /// ★★ 2026-10-17 the window counts the updates the game's own cooldown tick runs on (`a.g()V` calls `ag.i()V` only when the
    /// hit-stop counter `a.j:B` — absolute **0xb4** = `a` 0x5c + 0x58 — is not above 0 and the state word `ag.a:I` @0x193aec is 3,
    /// the field): a knight staggered by a hit (probe: the f721..736 freeze after 발도 — the presses dropped, the counters held)
    /// keeps his 0.5 s.
    pub hitstop_off: u32,
    pub state_slot: u32,
}

impl CombatPatch {
    /// The whole block and where each piece sits in it. ★ 2026-10-01: 0x400 → 0x500 — the
    /// painter's 신의가호 branch, the sweep's sixteen-tile ring and the sequence map.
    /// ★ 2026-10-03: 0x500 → 0x800 — the cast / SP / cooldown wrappers and a sweep that carries
    /// six tile tables (up to 36 targets). ★ same day, later: 0x800 → 0x900 — the cast wrapper
    /// also spawns 발도's flying slash (its slot 0xc0 → 0x140 B; the pieces after it moved 0x80).
    /// ★ 2026-10-05: 0x900 → 0x980 — 철괴's damage cut at the end ([`Self::IRON_AT`]).
    /// ★ 2026-10-07: 0x980 → 0xa00 — that piece also carries a HOOK word (`hero2java::h2_iron_hook_at`): the berserker
    /// block's taken-damage function (분노태세 · 도발, [`BerserkerPatch::TAKEN_AT`]), 0 until that block puts it there.
    /// ★ 2026-10-07 천공의검: 0xa00 → 0xb00 — the cast wrapper's room 0x140 → 0x1e0 (kind 15: two sweeps and three
    /// prongs), SP · cooldown · the sweep moved up, 철괴 at 0xa00, the max-level wrapper at [`Self::MAX_AT`].
    /// ★ 2026-10-12: 0xb00 → 0xb80 — 철괴 reads 지력 (its room 0x80 → 0xc0, the max-level wrapper moved to 0xac0) and
    /// 치유오라's heal piece at [`Self::HEAL_AT`].
    /// ★★ 2026-10-15 (협곡 지하 곡괭이 · 스프링 부츠): 0xb80 → 0xc00 — the RANGE piece makes 둔기강타's tile count (its room
    /// 0x3c → 0xbc, GEAR_RANGE_PTR at the end).
    /// ★★ 2026-10-17 (발도 특화 아이템 · 쌍월 재작업): 0xc00 → 0x1000 — 발도's piece moved to the end ([`Self::IAIDO_AT`], it makes
    /// each cast's tile list now and opens 쌍월's window), the sweep's room runs on to 철괴 (0x660), the sequence map moved out of
    /// its 32 bytes ([`Self::SEQ_MAP_AT`] — it picks the bigger ring), the character update's piece ([`Self::UPDATE_AT`]), the
    /// reach helper ([`Self::REACH_AT`]) and the cast's tile list ([`Self::IAIDO_TILES`]) and words after it.
    pub const BLOCK_LEN: u32 = 0x1000;
    pub const WEAPON_AT: [u32; 2] = [0x000, 0x030];
    pub const HP_GUARD_AT: u32 = 0x060;
    pub const HIT_GUARD_AT: u32 = 0x080;
    /// ★★ 2026-10-17 0x0a0 → 0xc00 (its room was 32 bytes; 0x0a0..0x0c0 is unused now).
    pub const SEQ_MAP_AT: u32 = 0xc00;
    pub const WAVE_PAINT_AT: u32 = 0x0c0;
    pub const WAVE_CTOR_AT: u32 = 0x120;
    pub const CAST_AT: u32 = 0x160;
    /// ★★ 2026-10-15 the cast piece's spare room: 천공의검's factor, out of the sweep (`hero2java::h2_sky_formula_block`).
    pub const SKYF_AT: u32 = 0x2b0;
    pub const SP_COST_AT: u32 = 0x340;
    pub const COOLDOWN_AT: u32 = 0x370;
    pub const SWEEP_AT: u32 = 0x3a0;
    /// ★★ 2026-10-17 (user: *"야수 악의 시 검풍의 기본공격시 검기 발동 옵션이 적용이 안되고있어"*) 검풍's 검기 as a piece of its own
    /// (`hero2java::h2_wind_block`; r1 = the knight): the cast wrapper calls it after a 기본공격 stage's hit and the demon block's
    /// CLAW piece after a claw stage's (through `DemonPatch::WIND_PTR`) — the claw never reaches the knight's cast. In the
    /// sweep's tail room: the sweep runs on to here now (0x5a0), the piece to 철괴.
    pub const WIND_AT: u32 = 0x940;
    /// ★★ 2026-10-15 (2차 H1 쌍월) 발도's piece, out of the cast wrapper (its room was full): the cast piece jumps here for
    /// kind 9 (`hero2java::h2_iaido_block`) — the sweep, 신의가호's flying slash, the sound, and the worn 쌍월. ★★ 2026-10-17
    /// 0x900 → 0xc40 (room 0x200): it also writes this cast's tiles ([`Self::IAIDO_TILES`]) before the sweep and opens 쌍월's
    /// window at the last frame — the sweep's room runs on to [`Self::IRON_AT`] (0x660).
    pub const IAIDO_AT: u32 = 0xc40;
    /// ★★ 2026-10-17 쌍월's window counter — the character update's piece (`a.g()V`'s def word, before the body;
    /// `hero2java::h2_update_block`).
    pub const UPDATE_AT: u32 = 0xe40;
    /// ★★ 2026-10-17 the reach helper (`hero2java::h2_iaido_reach_block`): `r0` = 발도's radius ([`Self::IAIDO_RADIUS`] + the worn
    /// reach items, at most [`GearPatch::IAIDO_REACH_MAX`]) — `r1` clobbered. The sequence map and 발도's piece call it.
    pub const REACH_AT: u32 = 0xed8;
    /// ★★ 2026-10-17 this cast's tiles, `(dx, dy)` byte pairs ([`Self::iaido_tiles`] — 발도's piece writes them at the hit frame,
    /// the install puts the plain disc there) and their count; the sweep walks them for the knight's kind 9.
    pub const IAIDO_TILES: u32 = 0xf00;
    pub const IAIDO_N: u32 = 0xfb0;
    /// ★★ 2026-10-17 쌍월 (user: *"발도 사용시 0.5초내로 재사용하면 쿨타임 없이 재사용가능하고, 재사용 한뒤에 기존 발도 쿨타임이 적용"*):
    /// the updates the window has left (0 = shut) · the quickbar slot it freed · that slot's counter when it opened · 1 while the
    /// 발도 running is the re-cast (its end opens no window).
    pub const TWIN_LEFT: u32 = 0xfb4;
    pub const TWIN_SLOT: u32 = 0xfb8;
    pub const TWIN_SAVED: u32 = 0xfbc;
    pub const TWIN_RECAST: u32 = 0xfc0;
    pub const IRON_AT: u32 = 0xa00;
    pub const MAX_AT: u32 = 0xac0;
    /// ★★ 2026-10-15 (2차 E3 · E4) in the max-level piece's spare room: the sweep's RANGE helper (신의가호 or the gear's range
    /// bit, `hero2java::h2_sweep_range_block`) and the gear block's W_RANGE word's address (the gear install writes it; 0 = no
    /// gear block — the tables as before).
    pub const RANGE_AT: u32 = 0xb40;
    /// ★★ 2026-10-17 the KINT piece (나이트 공격 스킬 × 지력, [`Self::knight_int_bonus`]) in the range helper's spare room —
    /// the sweep calls it right after its attack roll (`hero2java::h2_knight_int_block`). The range helper keeps 0x60.
    pub const KINT_AT: u32 = 0xba0;
    pub const GEAR_RANGE_PTR: u32 = 0xbfc;
    pub const HEAL_AT: u32 = 0xb00;
    // ── ★★ 2026-10-07 천공의검 — the knight's awakening (user: *"천공의검 : 캐릭터가 바라보는 방향 기준 신의가호를 사용한 검기의
    //    범위보다 2배많은 범위에 공간을 가르는 검 이펙트(이펙트는 캐릭터 바라보는 방향 기준 3갈레로 나눠지지만 실제 데미지는 검기처럼
    //    중간도 전부 받음) 그리고 공간을 가르는 검이 깨지며 한번더 데미지 입힘(데미지는 공간가르기 공격력 x (지력/8 + 힘/12 + 의지/8),
    //    깨기 데미지 공격력 x (지력/6 + 힘/10 + 의지/6)) … 각성기라서 훨씬 강해야해, 약 4배 이상 강하면 좋아. 쿨타임 40초"*) ──
    //
    // `a.p[15]` = kind 15 (the knight's first free slot), quickbar value 8, the eighth ACTIVE cell; LV 70, top level 1;
    // SP = max SP >> 2 (as 발도), cooldown 40 s. ★ 2026-10-08 (user, with ten DNF stills: *"3갈래를 차례대로 그으면서
    // 데미지가 들어가는거야 그러니까 총 4번의 데미지가 들어가는거야. 그리고 마지막 폭발 데미지가 들어가고"*): the action is
    // [`Self::SKY_FRAMES`] (18) frames — the cast wrapper writes that into `a.n[15]` itself (the slot's length comes from
    // sequence 19, 점프 내려찍기's six) — and sweeps the SKY lobe ([`Self::sky_lobe_tiles`] — 57 tiles: ahead 1..=7, each
    // side up to the depth and at most 5 — twice the blessed 검기's 27) FIVE times: at each of [`Self::SKY_SLASH_FRAMES`]
    // (the three prongs drawn one after another — a front diagonal, ahead, the other diagonal) with the first factor, at
    // [`Self::SKY_BREAK_FRAME`] (the glass breaks) and at [`Self::SKY_BLAST_FRAME`] (the explosion) with the second. The
    // factors are the user's, with the class multiplier's ×1.5 (`SkillPowerPatch`, every kind ≥ 3) taken out of them here
    // so the damage before defence is exactly `atk × (INT/8 + STR/12 + WIL/8)` and `atk × (INT/6 + STR/10 + WIL/6)`: the
    // sweep passes `atk × (SKY1_INT·INT + SKY1_STR·STR + SKY1_WIL·WIL) / SKY1_DIV` = `atk × (3 INT + 2 STR + 3 WIL) / 36`
    // (×1.5 → /24 = the user's) and `(5 INT + 3 STR + 5 WIL) / 45` (×1.5 → /30). INT · STR · WIL = the base ability + the
    // equipment's `a.b[k]`. The second factor's sweeps are told apart by `r3` = [`Self::SKY2_MAGIC`] (as the channel's
    // marker). The picture — the three prongs growing, the cracks, the flash — is the host's own [`SkyDrawPatch`] (lines
    // and rectangles through the game's Graphics, nothing in the sheets); the flying `ak` waves of 10-07 are gone.
    pub const SKY: u8 = 15;
    pub const SKY_VALUE: u8 = 8;
    pub const SKY_CD_INDEX: u8 = Self::SKY_VALUE - 1;
    pub const SKY_COOLDOWN: u16 = 800;
    pub const SKY_MAX_LEVEL: u8 = IlpungPatch::AWAKEN_MAX_LEVEL;
    /// The frames the three prongs are drawn (and the lobe swept with the first factor), in order — ★ 2026-10-08 (4)
    /// (user: *"각 찌르기 별 간격을 조금 더 좁혀줘"*) 0.1 s apart (2 · 5 · 8 = 0.15 s before; the game's tick is 0.05 s).
    /// ★ 2026-10-08 (6) (user: *"스킬자체를 1.3초에서 훨씬 늘려도 괜찮으니까 그림을 더 많이그려서"*): 0.3 s apart (4 · 10 · 16).
    pub const SKY_SLASH_FRAMES: [u8; 3] = [4, 10, 16];
    pub const SKY_HIT_FRAME: u8 = Self::SKY_SLASH_FRAMES[0];
    /// The break (the fourth sweep, the second factor) and the explosion (the fifth, the second factor again). ★ 2026-10-08
    /// (3) (user: *"시전속도가 너무빨라서 찌르고 깨지고 터지는 이펙트가 잘 안보이는데 … 1번,2번,3번 찌를때 0.15초 간격으로 찌르고 3번째
    /// 찌른뒤에 0.3초 뒤 깨지고. 깨진뒤에 0.3초뒤 폭발"*): 11 · 14 → 14 · 20 — six frames (0.3 s at the game's 20 fps) after the
    /// third prong, six more after the break.
    /// ★ (6): the break 0.4 s after the third prong (24), the blast 0.6 s after the break (36).
    pub const SKY_BREAK_FRAME: u8 = Self::SKY_SLASH_FRAMES[2] + 8;
    pub const SKY_BLAST_FRAME: u8 = Self::SKY_BREAK_FRAME + 12;
    /// `hero/hero1` sequence 25 (the map answers 21 for kind 15) — 발도's poses, no ring, stretched over SKY_FRAMES
    /// (`tools_re/awakenings.py`). ⛔ `a.n[15]` is NOT read from it: `cf.a()V` fills `a.n[k]` from sequence `k + 4` = 19 (six
    /// frames) — the cast wrapper stores SKY_FRAMES there on every frame of the action (`CombatPatch::frames_slot`).
    /// ★ 2026-10-08 (3): 18 → 27 — the blast's picture plays [`SkyDrawPatch::BLAST_DRAWS`] frames after SKY_BLAST_FRAME.
    pub const SKY_SEQ: u8 = 25;
    /// ★ (6): the action lasts SKY_TAIL frames past the blast — the shards fly, the dark lifts (60 frames = 3 s).
    pub const SKY_TAIL: u8 = 24;
    pub const SKY_FRAMES: u8 = Self::SKY_BLAST_FRAME + Self::SKY_TAIL;
    /// The three slashes of 10-07 flew as `ak` waves marked −10 (the painter drew sequence 26 for them). Kept for the
    /// painter's branch and the sheet (sequence 26 is still in the asset); the cast wrapper spawns none since 2026-10-08.
    /// ★★ 2026-10-17 the mark is the sequence (the painter draws `−attack`) — 10 → 26.
    pub const SKY_WAVE_MARK: u8 = Self::SKY_WAVE_SEQ;
    pub const SKY_WAVE_LIFE: u8 = 7;
    pub const SKY_WAVE_SEQ: u8 = 26;
    /// ★ 2026-10-08 (5) (user: *"데미지 범위는 현재 앞쪽만 되고있는데 이걸 캐릭터 기준으로 모든 방향으로 … 바라보는 방향기준 뒤로
    /// 5칸 + 바라보는 방향으로 28칸, 이 길이고 너비는 9칸"*): the damage area was a BOX in the facing's frame — ahead
    /// −SKY_BACK..=SKY_DEPTH, side −SKY_HALF..=SKY_HALF (34 × 9 = 306 tiles). ★★ 2026-10-09 (11) (user: *"천공의검이 맵
    /// 전체에 데미지를 입히도록 수정해줘. 이펙트는 수정할 필요없어"*): the damage is the WHOLE MAP — the sweep walks the
    /// stage's entity list and keeps every live monster, up to SKY_SLOTS of them (a map places at most 20 —
    /// `tools_re/map_monsters.py`). SKY_DEPTH / SKY_BACK / SKY_HALF are only the PICTURE's reach now ([`SkyDrawPatch`]'s
    /// blades — the user kept the effect as it is).
    pub const SKY_DEPTH: u8 = 28;
    pub const SKY_BACK: u8 = 5;
    pub const SKY_HALF: u8 = 4;
    pub const SKY_SLOTS: u8 = 60;
    pub const SKY2_MAGIC: u32 = 0xc500_003a;
    /// `(INT weight, STR weight, WIL weight, divisor)` of the two factors as the sweep computes them (before ×1.5).
    pub const SKY1: (u8, u8, u8, u8) = (3, 2, 3, 36);
    pub const SKY2: (u8, u8, u8, u8) = (5, 3, 5, 45);
    /// The abilities' absolute field offsets (힘 · 체력 · 민첩 · 의지 · 지력 = `a.e..a.i:S`, declared +0x28.. on class `a`'s 0x5c
    /// base — `Hero2Java::char_abilities_off` 0x84) and the equipment bonus array `a.b:[S` (declared +0x3c → absolute
    /// **0x98**; ⛔ the declared offset read as absolute was the first crash — the word there is a plain int, and the sweep
    /// dereferenced it as a handle).
    pub const ABILITY_STR_OFF: u32 = 0x84;
    pub const ABILITY_WIS_OFF: u32 = 0x90;
    pub const ABILITY_INT_OFF: u32 = 0x94;
    pub const EQUIP_BONUS_OFF: u32 = 0x98;
    /// The first factor's share of the attack (per mille) for an ability triple — the user's formula × 1000.
    pub const fn sky_factor_permille(which: (u8, u8, u8, u8), int: i32, str_: i32, wis: i32) -> i32 {
        let (wi, ws, ww, div) = which;
        (wi as i32 * int + ws as i32 * str_ + ww as i32 * wis) * 1000 * 3 / 2 / div as i32
    }
    /// ★★ 2026-10-05 철괴 (user: *"1로 구현하고 이름은 철괴야. 받는피해는 1lv : 10%~ lv5 : 20%까지 줄일꺼야. 지속시간은
    /// 1lv : 20초, 5lv : 40초, 오라 숙련 레벨 1당 받는 피해감소량이 1%씩 증가하고, 지속시간도 오라숙련 1레벨당 4초씩 늘어나면 돼.
    /// 쿨타임은 60초 고정이야."*): 방어의지 (`a.p[8]`, buff 35, flag `a.r:Z`) no longer blocks every hit for 2–4 s and
    /// no longer breaks on an attack — it takes `IRON_CUT_BASE + IRON_CUT_PER_LEVEL·lv + IRON_CUT_PER_AURA·오라수련`
    /// per mille off every monster hit (Lv1 10 % … Lv5 20 %, +1 % per 오라수련 level → 25 % with both maxed) for
    /// `IRON_FRAMES_BASE + IRON_FRAMES_PER_LEVEL·lv + IRON_FRAMES_PER_AURA·오라수련` frames (20 … 40 s + 4 s a level
    /// → 60 s with both maxed = the cooldown), cooldown 60 s ([`GuestBuild::iron_wall_rework`]). A level byte
    /// outside `0..=IRON_MAX_LEVEL` (−1 = 오라수련 not learned yet — the quest 「무사의 장갑」 opens it, the level
    /// table never does) counts as 0.
    pub const IRON: u8 = 8;
    pub const AURA_TRAINING: u8 = 14;
    pub const IRON_MAX_LEVEL: u8 = 5;
    pub const IRON_CUT_BASE: u8 = 75;
    pub const IRON_CUT_PER_LEVEL: u8 = 25;
    pub const IRON_CUT_PER_AURA: u8 = 10;
    /// ★ 2026-10-06 (버프 쿨 1분 · 「만렙 지속 75초」 · 「전부 ×1.25」): 375 · 125 · 100 — 25 … 50 s + 5 s a 오라수련 level →
    /// 75 s with both maxed (the cooldown stays 60 s; why 75: `GuestBuild::counter_stance_duration`). Were 300 · 100 · 80.
    pub const IRON_FRAMES_BASE: u16 = 375;
    pub const IRON_FRAMES_PER_LEVEL: u8 = 125;
    pub const IRON_FRAMES_PER_AURA: u8 = 100;
    /// ★★ 2026-10-12 (user: *"철괴의 데미지 감소도 지력에 비례해서 받는 피해량 최대 50%까지 감소 가능해"* → 물어서 「지력 5당 +1%p」):
    /// every point of 지력 (the base `a.i:S` + the equipment's `a.b[4]` — [`Self::ABILITY_INT_OFF`] · [`Self::EQUIP_BONUS_OFF`])
    /// adds IRON_CUT_PER_INT per mille, and the whole cut stops at IRON_CUT_MAX — Lv5 + 오라수련 5 (25 %) reaches 50 % at 지력
    /// 125. A negative 지력 adds nothing.
    pub const IRON_CUT_PER_INT: u8 = 2;
    pub const IRON_CUT_MAX: u16 = 500;

    /// 철괴's cut in per mille for a level byte and an 오라수련 byte as the game stores them (signed;
    /// anything outside `0..=5` counts as 0) and the character's 지력 — what `hero2java::h2_iron_reduce_block` computes.
    pub const fn iron_reduction_permille(lv: i8, aura: i8, int: i32) -> u32 {
        let lv = if lv >= 0 && lv <= Self::IRON_MAX_LEVEL as i8 { lv as u32 } else { 0 };
        let aura = if aura >= 0 && aura <= Self::IRON_MAX_LEVEL as i8 { aura as u32 } else { 0 };
        let int = if int > 0 { int as u32 } else { 0 };
        let cut = Self::IRON_CUT_BASE as u32 + Self::IRON_CUT_PER_LEVEL as u32 * lv + Self::IRON_CUT_PER_AURA as u32 * aura + Self::IRON_CUT_PER_INT as u32 * int;
        if cut > Self::IRON_CUT_MAX as u32 { Self::IRON_CUT_MAX as u32 } else { cut }
    }

    /// The damage a hit of `damage` does under 철괴 (before the game's min-3 clamp) — floor division,
    /// as the block's `divide` call does it.
    pub const fn iron_damage(damage: i32, lv: i8, aura: i8, int: i32) -> i32 {
        damage - damage * Self::iron_reduction_permille(lv, aura, int) as i32 / 1000
    }

    /// ★★ 2026-10-12 (user: *"나이트의 치유오라가 지력계수에 비례해서 HP회복량이 올라가게"* → 물어서 「회복량 × (1 + 지력/100)」): the
    /// heal `bx.a()Z` case 3 computes (`최대HP × Lv / 15 + 오라수련 × 10`, shipped) × (HEAL_INT_BASE + 지력) / HEAL_INT_BASE —
    /// 지력 100 doubles it. A negative 지력 counts as 0; floor division (the game's `divide`).
    pub const HEAL_INT_BASE: u8 = 100;
    pub const fn heal_amount(heal: i32, int: i32) -> i32 {
        let int = if int > 0 { int } else { 0 };
        heal * (Self::HEAL_INT_BASE as i32 + int) / Self::HEAL_INT_BASE as i32
    }

    /// ★★ 2026-10-17 나이트 공격 스킬 × 지력 (user: *"나이트의 모든 공격스킬에 지력계수가 붙게해줘 … 올힘나이트보다 올지력 나이트가
    /// 50%정도 데미지가 나오게"* → 물어서 「지력 효율 = 힘의 절반」): the knight's attack is `힘' × 7/10 + the weapon` (`bx.c()V`), so the
    /// SKILLS' attack gets `지력' × KNIGHT_INT_MUL >> KNIGHT_INT_SHIFT` on top — 179/512 = 0.3496 = half of 힘's 0.7. 지력' = the base
    /// `a.i:S` + the equipment's `a.b[4]`; a negative 지력' adds nothing. The sweep adds it to its ONE attack roll (the KINT piece,
    /// [`Self::KINT_AT`]) for kinds 4 검기 · 5 둔기강타 · 6 방패치기 (the 크루세이더 channel too) · 9 발도, from the knight only —
    /// before each kind's level factor, so 지력 counts exactly as attack does through every multiplier after it (the class ×1.5,
    /// the extras, 신의가호, the gear). ⛔ Not 15 천공의검 (its factor is the user's own `INT/8 + STR/12 + WIL/8`, already 지력) and not
    /// the 기본공격 (no sweep).
    pub const KNIGHT_INT_MUL: u8 = 179;
    pub const KNIGHT_INT_SHIFT: u8 = 9;
    /// The attack 지력' adds to a knight skill's roll — what the KINT piece computes.
    pub const fn knight_int_bonus(int: i32) -> i32 {
        if int > 0 { (int * Self::KNIGHT_INT_MUL as i32) >> Self::KNIGHT_INT_SHIFT } else { 0 }
    }

    /// 철괴's length in frames (20 a second) for the same two bytes — what the rewritten cast computes.
    pub const fn iron_frames(lv: i8, aura: i8) -> u32 {
        let lv = if lv >= 0 && lv <= Self::IRON_MAX_LEVEL as i8 { lv as u32 } else { 0 };
        let aura = if aura >= 0 && aura <= Self::IRON_MAX_LEVEL as i8 { aura as u32 } else { 0 };
        Self::IRON_FRAMES_BASE as u32 + Self::IRON_FRAMES_PER_LEVEL as u32 * lv + Self::IRON_FRAMES_PER_AURA as u32 * aura
    }
    /// ★★ 2026-10-03 발도 (user: *"나이트의 스킬에 발도라는 스킬을 추가할꺼야 … 방어의지 밑에 … 둥그렇게
    /// 이펙트가 생기고 그 동그란 범위 내의 적에게 모두 데미지"*): the knight's SEVENTH active, `a.p[9]`
    /// — the slot of 부활오라 (네트워크용: no cast case, no cooldown entry, no script ever unlocks
    /// it, and the SKILL window counts the knight's actives as six to hide it). The sweep hits
    /// [`Self::disc_tiles`] (36) at frame [`Self::IAIDO_HIT_FRAME`] of its own sequence 13 (the
    /// asset redraws it with [`Self::IAIDO_FRAMES`] frames — `a.n[9]` follows the sheet), with the
    /// attack × `(lv + IAIDO_POWER_ADD) · (lv + IAIDO_POWER_ADD_2) / IAIDO_POWER_DIV` (×1.5 on top from
    /// `SkillPowerPatch`: Lv1 ×3 · Lv2 ×4.4 · Lv3 ×6 · Lv4 ×7.9 · Lv5 ×10 — ★ 2026-10-03 저녁, user: *"발도 : 레벨
    /// 배율 추가, lv5 기준 대략 공격력의 10배"*; it was `(lv + 5) / 3` = Lv1 ×3 · Lv5 ×5, so no level loses),
    /// SP = max SP / [`Self::IAIDO_SP_DIV`], cooldown [`Self::IAIDO_COOLDOWN`]
    /// frames (10 s — the wrapper's own value, not halved again). The sweep's sound is 검기's 7.
    pub const IAIDO: u8 = 9;
    /// 방어의지 — the slot whose unlock (quest 「터줏대감」) opens 발도 too (`hero2java::h2_iaido_unlock`).
    pub const IAIDO_GUARD: u8 = 8;
    pub const IAIDO_CD_INDEX: u8 = 6;
    pub const IAIDO_HIT_FRAME: u8 = 3;
    /// ★★ 2026-10-17 쌍월 reworked (user: *"현재 쌍월이 발도가 2번 나가는게 아니라 발도 사용시 0.5초내로 재사용하면 쿨타임 없이
    /// 재사용가능하고, 재사용 한뒤에 기존 발도 쿨타임이 적용되게"* · 「발도 동작이 끝난 뒤 0.5초」 — the 10-15 replay is gone): at 발도's
    /// last frame the slot holding [`Self::IAIDO_VALUE`] gets its counter freed (−1, the game's 「ready」) for [`Self::TWIN_UPDATES`]
    /// character updates; a 발도 cast in them is the RE-CAST — its own cooldown start (the game's, at `g.w == 0`) stands, and its
    /// end opens no window; no re-cast → the counter goes back to what it was less the window. SP is paid each cast as ever.
    /// The quickbar value of 발도 (kind 9 − the knight's 3 stages + 1 = cooldown index 6 + 1).
    pub const IAIDO_VALUE: u8 = Self::IAIDO_CD_INDEX + 1;
    /// 10 updates (0.5 s at the game's 20) after the action's end — the window opens on the last frame, one update before.
    pub const TWIN_UPDATES: u8 = 11;
    /// ★★ 2026-10-17 발도 특화 아이템 (user: *"발도의 범위를 늘려주는 아이템과 데미지 증가, sp와 쿨타임을 감소시켜 주는 아이템"* ·
    /// 「아이템 하나마다 반경 +1칸」 · *"발도 고리에 맞춰서 키워지고 날라가는 초승달 모양 발도의 검기도 크기에 맞게 폭이 늘어나야해.(입히는
    /// 데미지폭과 이펙트 모두)"* → 「초승달도 피해를 줌」): the disc's radius is IAIDO_RADIUS + the reach items worn
    /// ([`GearPatch::IAIDO_REACH_MAX`] at most) — 36 · 56 · 80 tiles — and under 신의가호 the flying crescent's band hits too:
    /// ahead 1..=IAIDO_BAND_AHEAD (where the crescent flies), `|side| ≤ R − 2` (3 · 5 · 7 wide). One sweep, so a monster in
    /// both is hit once. See [`Self::iaido_tiles`].
    pub const IAIDO_RADIUS: i8 = 3;
    pub const IAIDO_BAND_AHEAD: i8 = 5;
    /// The most tiles one cast lists (radius 5 + the 7-wide band = 86) — the sweep's target slots ([`Self::SWEEP_SLOTS`]).
    pub const IAIDO_TILES_MAX: u8 = 88;
    /// The sweep's target slots: 천공의검's entity walk keeps [`Self::SKY_SLOTS`] of them, 발도's tiles one each.
    pub const SWEEP_SLOTS: u8 = Self::IAIDO_TILES_MAX;
    /// ★★ 2026-10-17 the bigger rings: `hero/hero1` sequences 54 (radius 4) · 55 (radius 5) — 발도's poses with the ring scaled
    /// (`tools_re/knight_iaido.py`, appended after the demon copies) — the sequence map answers them; the crescents under
    /// 신의가호: 56 · 57 (the painter draws a wave whose attack is `−seq`).
    pub const IAIDO_REACH_SEQ: u8 = 54;
    pub const IAIDO_REACH_WAVE_SEQ: u8 = 56;
    /// The tiles 발도's piece writes for a cast, in its order (row by row, `dy` −5..=5 outer, `dx` −5..=5 inner, the knight's own
    /// tile left out): the disc `|dx|, |dy| ≤ radius, |dx| + |dy| ≤ radius + 1`, and when `band > 0` the band in the facing's
    /// frame — ahead 1..=IAIDO_BAND_AHEAD, `|side| ≤ band` (facing 1 up: ahead −dy · 2 down: dy · 3 left: −dx · 4 right: dx).
    pub const fn iaido_tiles(radius: i8, band: i8, facing: u8) -> ([(i8, i8); 88], u8) {
        let mut out = [(0i8, 0i8); 88];
        let mut n = 0usize;
        let mut dy = -5i8;
        while dy <= 5 {
            let mut dx = -5i8;
            while dx <= 5 {
                if dx != 0 || dy != 0 {
                    let (ax, ay) = (if dx < 0 { -dx } else { dx }, if dy < 0 { -dy } else { dy });
                    let disc = ax <= radius && ay <= radius && ax + ay <= radius + 1;
                    let (ahead, side) = if facing <= 2 {
                        (if facing == 1 { -dy } else { dy }, ax)
                    } else {
                        (if facing == 4 { dx } else { -dx }, ay)
                    };
                    let band_hit = band > 0 && ahead >= 1 && ahead <= Self::IAIDO_BAND_AHEAD && side <= band;
                    if disc || band_hit {
                        out[n] = (dx, dy);
                        n += 1;
                    }
                }
                dx += 1;
            }
            dy += 1;
        }
        (out, n as u8)
    }
    /// ★★ 2026-10-15 (2차 H2 검풍) the knight's 기본공격 stages (kinds 0..3) hit at `g.w == 2` — `bx.a()Z`'s cases 0..2 (jump
    /// table @0x1706d4 → 0x13b4a2: `g.w == 2` → `a.f(B)Z(kind)`); 검풍's 검기 goes right after that hit.
    pub const BASIC_HIT_FRAME: u8 = 2;
    pub const IAIDO_FRAMES: u8 = 6;
    pub const IAIDO_SEQ: u8 = 13;
    pub const IAIDO_SP_DIV: u8 = 4;
    /// ★★ 2026-10-14 (b) 나이트 SP 완화 (user 「스킬 SP ×0.75 (약하게)」, HERO2_NEW_ITEM_DRAFT.md §7 — the knight's bottleneck
    /// is SP: 검기 ran 3 % of the time in the model): the SP wrapper answers `cost − (cost >> SP_CUT_SHIFT)` = ×0.75 for the
    /// knight's ATTACK skills [`Self::SP_CUT_KINDS`] — 검기 · 둔기강타 · 방패치기 · 발도. ⚠ Claude's reading: the buffs keep
    /// theirs (치유오라 = kind 3 pays `a.j` and never asks this table · 신의가호 7 · 철괴 8) and so does 천공의검 (every
    /// awakening costs max SP / 4). The 크루세이더 channel asks the same wrapper, so its per-second share follows.
    pub const SP_CUT_KINDS: [u8; 4] = [4, 5, 6, Self::IAIDO];
    /// ★ 2026-10-20 (밸런스 전수 조사, user approved ②): 2 → **1** — the knight's attack skills cost ×0.5 (was ×0.75).
    pub const SP_CUT_SHIFT: u16 = 1;
    /// What the wrapper answers for a kind whose cost (the body's, or 발도's max SP / 4) is `cost`.
    pub const fn sp_cut(kind: u8, cost: i32) -> i32 {
        if kind >= 4 && kind <= 6 || kind == Self::IAIDO { cost - (cost >> Self::SP_CUT_SHIFT) } else { cost }
    }
    /// ★ 2026-10-03 later (user: *"발도 쿨타임 30%로 줄여주고"*): 200 → 140 frames (7 s).
    pub const IAIDO_COOLDOWN: u16 = 140;
    pub const IAIDO_POWER_ADD: u8 = 5;
    pub const IAIDO_POWER_ADD_2: u8 = 3;
    pub const IAIDO_POWER_DIV: u8 = 12;
    /// ★ 2026-10-03 저녁 (user: *"방패치기 : 레벨 배율 추가, lv 5 기준 대략 공격력의 5배"*): the sweep hands
    /// 방패치기 the attack × `(lv + SHIELD_POWER_ADD) · 2 / SHIELD_POWER_DIV` (lv = a.p[6], past 5 = 5) — with
    /// `SkillPowerPatch`'s ×1.5 and +50 % that is `(lv + 5) / 2` × the attack: Lv1 ×3 · Lv5 ×5 (it was ×2.25 at
    /// every level). The accessory channel's call (크루세이더, `r3` = [`Self::CHANNEL_MAGIC`]) keeps the plain
    /// attack — it hits every half second.
    pub const SHIELD_POWER_ADD: u8 = 5;
    pub const SHIELD_POWER_DIV: u8 = 9;
    pub const IAIDO_SOUND: u8 = 7;
    /// 검기 (`a.p[4]`): the cast's case spawns the wave at frame 5 of sequence 8; the sweep runs
    /// at the same frame over [`Self::lobe_tiles`] — 13 tiles, 27 under 신의가호 — with the
    /// wave's own attack `atk + atk / (8 − lv)`. The wave flies on as the picture, hitting
    /// nothing ([`Self::wave_no_hit`]), [`Self::WAVE_LIFE`] tiles (5 blessed).
    pub const WAVE: u8 = 4;
    pub const WAVE_HIT_FRAME: u8 = 5;
    pub const WAVE_LIFE: u8 = 3;
    pub const BLESSED_WAVE_LIFE: u8 = 5;
    /// The wave's pictures: the knight's sequences 21 (blue) and 22 (red, bigger), drawn by the
    /// painter in place of the shipped 17 — one frame per tile flown, the list of the wave's own
    /// direction `cb.j` (as the shipped painter passed it). ★ 2026-10-03, later (user: *"검기도 …
    /// 부메랑같은 모양의 검기가 날라가야해. 원이 날라가는게 아니야"*): the pictures are CRESCENTS now,
    /// convex toward the flight, one per direction — rings before (the names stayed).
    pub const WAVE_RING_SEQ: u8 = 21;
    pub const BLESSED_WAVE_RING_SEQ: u8 = 22;
    /// ★ 2026-10-03 (user: *"발도가 신의가호 안쓰면 지금과 같고 썻을때는 바라보는 방향으로 발도의 이펙트가
    /// 같이날라가도록 … 부메랑모양의 발도 이펙트가 이어서 더 날라가기를 … 날라가는 거리는 5칸"*): under
    /// 신의가호 the cast wrapper spawns one wave `ak` at 발도's hit frame — the shipped constructor
    /// with [`Self::IAIDO_WAVE_MARK`] as its attack (`ak.a:I`, unread since the waves hit nothing),
    /// lifetime [`Self::BLESSED_WAVE_LIFE`] — and the painter draws a wave carrying that mark as
    /// sequence [`Self::IAIDO_WAVE_SEQ`] (the black-and-grey crescent). A picture only: the hit
    /// stays the disc's.
    pub const IAIDO_WAVE_SEQ: u8 = 24;
    /// The mark is `−IAIDO_WAVE_MARK`: no attack is negative. ★★ 2026-10-17 a marked wave carries its SEQUENCE (the painter
    /// draws `−attack`), so the radius-4/5 crescents ([`Self::IAIDO_REACH_WAVE_SEQ`]) need no branch of their own — 9 → 24.
    pub const IAIDO_WAVE_MARK: u8 = Self::IAIDO_WAVE_SEQ;
    /// ★ 2026-10-01 (user: 신의가호 중 검기 = 율리케 보스의 십자 불기둥을 붉게, 3방향 그대로 ·
    /// 둔기강타 = 이안 보스의 점프 내려찍기를 파랗게, 타격은 8방향 직선 2칸씩): the knight's class
    /// animation gains two sequences (`tools_re/knight_blessing.py`) — the blessed 둔기강타's
    /// jump slam and the blessed 검기's red flame. The slam is drawn instead of 둔기강타's own
    /// sequence 9 (`seq_map`), the flame instead of the wave's 17 (the painter), only while
    /// `a.q:Z` is up. Both keep their shipped frame counts — the action lasts `a.n[kind]` =
    /// sequence `kind + 4`'s frames (set by `cf.a()V`), never the drawn one's.
    pub const SHIPPED_WAVE_SEQ: u8 = 17;
    pub const BLESSED_SLAM_SEQ: u8 = 19;
    pub const BLESSED_WAVE_SEQ: u8 = 20;
    /// The blessed 둔기강타's tiles: the eight lines out of the knight, two tiles each.
    pub const RING_TILES: u8 = 16;
    pub const fn ring_tiles() -> [(i8, i8); 16] {
        let dirs: [(i8, i8); 8] = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
        let mut out = [(0i8, 0i8); 16];
        let mut k = 0;
        while k < 2 {
            let mut d = 0;
            while d < 8 {
                out[k * 8 + d] = (dirs[d].0 * (k as i8 + 1), dirs[d].1 * (k as i8 + 1));
                d += 1;
            }
            k += 1;
        }
        out
    }
    /// ★ 2026-10-03 (user: 둔기강타 *"현재 범위에서 8방향 1칸씩만 추가"* → both forms +1): the eight
    /// lines out of the knight, THREE tiles each — the blessed 둔기강타 (the ring's sixteen first,
    /// then the third tile of every line).
    pub const STAR_TILES: u8 = 24;
    pub const fn star_tiles() -> [(i8, i8); 24] {
        let dirs: [(i8, i8); 8] = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
        let mut out = [(0i8, 0i8); 24];
        let mut k = 0;
        while k < 3 {
            let mut d = 0;
            while d < 8 {
                out[k * 8 + d] = (dirs[d].0 * (k as i8 + 1), dirs[d].1 * (k as i8 + 1));
                d += 1;
            }
            k += 1;
        }
        out
    }
    /// ★★ 2026-10-15 (협곡 지하 곡괭이 · 스프링 부츠, user: *"신의가호 쓴 둔기강타가 8방향 2칸이라면 해당아이템 2개 착용하면 … 8방향
    /// 4칸"* · 「반 칸 = 상하좌우만 한 칸 더」): 둔기강타 hits the first [`Self::bludgeon_tiles`] of these — the eight lines out of
    /// the knight ring by ring, each ring's four orthogonal tiles before its four diagonals (so a half ring = the orthogonals).
    /// [`Self::neighbour_tiles`] and [`Self::star_tiles`] are its first 8 and 24.
    pub const RAY_RINGS: u8 = 6;
    pub const RAY_TILES: u8 = 8 * Self::RAY_RINGS;
    pub const fn ray_tiles() -> [(i8, i8); 48] {
        let dirs: [(i8, i8); 8] = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
        let mut out = [(0i8, 0i8); 48];
        let mut k = 0;
        while k < Self::RAY_RINGS as usize {
            let mut d = 0;
            while d < 8 {
                out[k * 8 + d] = (dirs[d].0 * (k as i8 + 1), dirs[d].1 * (k as i8 + 1));
                d += 1;
            }
            k += 1;
        }
        out
    }
    /// 둔기강타's tile count: the reach in half rings `H = base × (2 + items)` — base 1 ring (3 under 신의가호, or with the gear's
    /// [`GearPatch::RANGE_STAR`]), +50 % a reach item (at most [`GearPatch::RANGE_REACH_MAX`]) — then 8 a whole ring and 4 (the
    /// orthogonals) a half: 8 · 12 · 16 plain, 24 · 36 · 48 blessed.
    pub const fn bludgeon_tiles(blessed: bool, items: u32) -> u8 {
        let base: u32 = if blessed { 3 } else { 1 };
        let n = if items > GearPatch::RANGE_REACH_MAX { GearPatch::RANGE_REACH_MAX } else { items };
        let h = base * (2 + n);
        (8 * (h / 2) + 4 * (h % 2)) as u8
    }
    /// The unblessed 둔기강타: the eight tiles around the knight (the front row and five more).
    pub const NEIGHBOUR_TILES: u8 = 8;
    pub const fn neighbour_tiles() -> [(i8, i8); 8] {
        [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)]
    }
    /// 발도's disc (user: *"둔기강타 범위에서 대각선방향 1칸씩 줄어들고, 그 사이 공간들까지"* → 반경 3):
    /// every tile with `|dx| ≤ 3`, `|dy| ≤ 3`, `|dx| + |dy| ≤ 4` but the knight's own — 36, the
    /// 7×7 square without its corners, nearest ring first.
    pub const DISC_TILES: u8 = 36;
    /// ★ 2026-10-03 (user: *"바바리안의 의지 … 폭풍강타의 범위 … 3x3인데 이걸 5x5로"*): the 5×5 around the
    /// character less its own tile = the first 24 of [`Self::disc_tiles`] (rings 1 and 2 — the
    /// table is written ring by ring). The sweep's kind 9 uses them, at the plain attack, when the
    /// caller is not a knight (`class_slot`): the accessory block's 폭풍강타 channel
    /// (`AccessoryPatch`, `bx.h(B)Z(9)` through the game's own cell).
    pub const STORM_TILES: u8 = 24;
    /// ★ 2026-10-03 the CHANNEL marker: `bx.h(B)Z` takes `(this, kind)` — `r3` is free, the game's
    /// call helpers pass it through untouched (`push {r1-r4}` … `pop`), and the accessory block's
    /// 방패치기 channel hands this in it. The sweep then answers for kind 6 with BOTH front rows
    /// (2×3, user: *"1x3만큼 … 2x3으로 한칸만 더 확장"*) whatever the blessing, WITHOUT the stun
    /// (user: *"크루세이더 장착시 방패치기는 기절이 없고"*), and rolls no attack when no monster
    /// stands there (the channel asks every frame — *"부딪히면 그떄마다 데미지"*). Not a pointer, not
    /// a small integer: no shipped caller leaves it in `r3`. Built in code (`movs · lsls · adds`),
    /// so the sweep needs no literal this far from its pool.
    pub const CHANNEL_MAGIC: u32 = 0xc500_0039;
    pub const fn disc_tiles() -> [(i8, i8); 36] {
        let mut out = [(0i8, 0i8); 36];
        let mut n = 0;
        let mut ring = 1;
        while ring <= 3 {
            let mut dy = -3i8;
            while dy <= 3 {
                let mut dx = -3i8;
                while dx <= 3 {
                    let (ax, ay) = (if dx < 0 { -dx } else { dx }, if dy < 0 { -dy } else { dy });
                    let r = if ax > ay { ax } else { ay };
                    if r == ring && ax + ay <= 4 {
                        out[n] = (dx, dy);
                        n += 1;
                    }
                    dx += 1;
                }
                dy += 1;
            }
            ring += 1;
        }
        out
    }
    /// 검기's LOBE (user's picture: 깊이 5 · 높이 9, the far rows pulled forward): per facing, the
    /// tiles `(ahead, side)` → `(dx, dy)`. Blessed = 27 (ahead 1..=5 on side 0 · ±1, 2..=4 on ±2,
    /// 3..=4 on ±3, 4 on ±4); plain = 13 (ahead 1..=3 on side 0 · ±1, 2..=3 on ±2). Nearest first.
    pub const LOBE_TILES: u8 = 13;
    pub const BLESSED_LOBE_TILES: u8 = 27;
    const fn lobe_rows(big: bool) -> [(i8, i8, i8); 9] {
        // (side, ahead from, ahead to); a row with from > to is empty.
        if big {
            [(0, 1, 5), (1, 1, 5), (-1, 1, 5), (2, 2, 4), (-2, 2, 4), (3, 3, 4), (-3, 3, 4), (4, 4, 4), (-4, 4, 4)]
        } else {
            [(0, 1, 3), (1, 1, 3), (-1, 1, 3), (2, 2, 3), (-2, 2, 3), (3, 1, 0), (-3, 1, 0), (4, 1, 0), (-4, 1, 0)]
        }
    }
    /// `(fx, fy, sx, sy)`: ahead and side unit vectors for facing 1 up · 2 down · 3 left · 4 right (★ 2026-10-07 also
    /// 천공의검's lobe table in the sweep block).
    pub const fn axes(facing: u8) -> (i8, i8, i8, i8) {
        match facing {
            1 => (0, -1, 1, 0),
            2 => (0, 1, 1, 0),
            3 => (-1, 0, 0, 1),
            _ => (1, 0, 0, 1),
        }
    }
    pub const fn lobe_tiles(facing: u8, big: bool) -> [(i8, i8); 27] {
        let (fx, fy, sx, sy) = Self::axes(facing);
        let rows = Self::lobe_rows(big);
        let mut out = [(0i8, 0i8); 27];
        let mut n = 0;
        let mut ahead = 1i8;
        while ahead <= 5 {
            let mut r = 0;
            while r < 9 {
                let (side, from, to) = rows[r];
                if from <= ahead && ahead <= to {
                    out[n] = (fx * ahead + sx * side, fy * ahead + sy * side);
                    n += 1;
                }
                r += 1;
            }
            ahead += 1;
        }
        out
    }
    /// The hit direction `g.u` of a tile hit AWAY from the knight: the line's own way, a diagonal
    /// counted as its left/right (what the ring did).
    pub const fn away(dx: i8, dy: i8) -> u8 {
        match (dx, dy) {
            (0, y) if y < 0 => 1,
            (0, _) => 2,
            (x, _) if x < 0 => 3,
            _ => 4,
        }
    }
    /// The weapon multiplier's shift: `weapon_extra / 1024`.
    pub const WEAPON_SHIFT: u8 = 10;
    /// The skills the sweep serves (`a.p` indices of the knight, 3 기본공격 stages first).
    pub const BLUDGEON: u8 = 5;
    pub const SHIELD_BASH: u8 = 6;
    /// Tiles in front: the row, and two rows for 방패치기 under 신의가호.
    pub const FRONT_ROW: u8 = 3;
    pub const FRONT_TWO_ROWS: u8 = 6;
    /// Hit effects: the skill spark `a.a(Lbd;BIBB)Z` passes, and 방패치기's own.
    pub const FX_SKILL: u8 = 0x16;
    pub const FX_SHIELD: u8 = 0x21;
    /// 방패치기's stun, as `bx.h(B)Z` computed it: `20 + 5·lv` frames.
    pub const STUN_BASE: u8 = 20;
    pub const STUN_PER_LEVEL: u8 = 5;

    /// The sweep's tiles `(dx, dy)` for facing 1 up · 2 down · 3 left · 4 right: the front row
    /// (ahead, ahead ± side), then the row beyond it — the order the sweep hits them.
    pub const fn front_tiles(facing: u8) -> [(i8, i8); 6] {
        let (fx, fy, sx, sy): (i8, i8, i8, i8) = match facing {
            1 => (0, -1, 1, 0),
            2 => (0, 1, 1, 0),
            3 => (-1, 0, 0, 1),
            _ => (1, 0, 0, 1),
        };
        [(fx, fy), (fx + sx, fy + sy), (fx - sx, fy - sy), (2 * fx, 2 * fy), (2 * fx + sx, 2 * fy + sy), (2 * fx - sx, 2 * fy - sy)]
    }
}

/// ★★ hero2's 맹독감염 rework (2026-09-29) — the rogue's poison STRIKE becomes a BUFF: while it
/// runs, every 기본공격 that lands poisons the monster it hit, and the poison's tick deals a share
/// of the rogue's attack instead of the shipped `3 × level` (HERO2_GUEST_RE.md §8.30). User:
/// *"맹독감염의 효과를 버프기로 바꾸고 기본공격시 중독을 거는 효과로 … 데미지는 공격력과 스킬레벨에
/// 비례하고, 독연마를 올리면 중독데미지가 독연마 레벨당 추가로 증가하고 버프 지속시간도 추가로 증가
/// … 20초지속 30초 쿨타임에서 29초지속 30초 쿨타임"* → *"0.5프레임당 300데미지로"* (per 0.5 s tick,
/// the game's own cadence: 300 % of the attack at 맹독감염 5 + 독 연마 3).
///
/// | piece | mechanism | effect |
/// |---|---|---|
/// | **cast** | `ay.a()Z` case 8, frame 4: the strike `ay.a(BBI)Z(kind, 26, 10·lv + 80)` becomes buff [`Self::BUFF_ID`] through `g.a(BI)V` — the call 민첩태세 makes; the case's own pool word (0x1203f4, one reader) is re-pointed at 민첩태세's cell for it | the buff lasts `50·맹독감염 + 60·독연마 + 150` frames = 7.5 s + 2.5 s a level + 3 s a 독 연마 level (Lv5 20 s → 29 s). ★ 2026-10-06: **`250·맹독감염 + 250`** = 25 → 75 s ([`Self::BUFF_BASE`]) |
/// | **cooldown** | `ay.<clinit>` 0x120e28: `ay.a:[S[3]` 180 → 1200 (the 민첩태세 trick, [`GuestBuild::agility_stance_invuln`]) | 600 frames after #17's ½ = **30 s**. ★ 2026-10-06: **2400 → 60 s** |
/// | **on-hit** | def word 0 of `ay.a(BB)Z` (the rogue's weapon strike: the 기본공격 stages, and 단검난무·번개난타's hits) → a wrapper that CALLS it and, when it hit, the kind is a 기본공격 stage and the buff runs (`g.a(B)Z(33)` answers false), poisons `this.a()Lbd;` — the facing monster, the strike's own target — through the game's `a.a(Lbd;BI)V(target, 26, POISON_LEN)` | 5 s of poison per landing (10 ticks); the game refuses a second poison while one runs (`bd.a(B)Z`), so a monster hit again keeps its poison and takes a fresh one when it ends. A falling monster is skipped by `a.a(Lbd;BI)V` itself |
/// | **tick** | `bd.a(Lav;)V` 0x126772 (every monster class shares the body): past the game's `av.b % 10 == 0`, the raw `HP −= a.p:B` (the rogue's `3·lv` byte) and its number become `ldr r0,=block ; bx r0` (the HP load's pool word, one reader) → the damage `a.a()I × (42·맹독감염 + 30·독연마) / 100` through the monster's own `bd.a(IB)V`, then back to the shipped bubble effect at 0x126818 | one tick every 10 frames (0.5 s) from the moment it lands. A poison kill falls and pays EXP and drops like a hit; a falling or dead monster (`g.t` 6/7) or HP ≤ 0 takes nothing (the raw write would have restarted its fall) |
///
/// | **aura** | `bm.a(II)V` 0x133d3c (every effect's draw): the `av.b` read's pool word and the `adds r1,r4,#0` after it become `ldr r0,=block ; bx r0` → sequence [`Self::AURA_SEQ`] for a `bm` of type 33 that lives longer than [`Self::SPARK_LEN`], then back to 0x133d40 | the buff draws the poison motes the asset appends to the rogue's sheet; the 4-frame hit spark keeps its shipped sprite |
///
/// Buff 33 is free: the skills use 30 · 31 · 32 (버서커) · 34 · 35 (나이트) · 36 (민첩태세);
/// `bm.i[33 − 30]` gives it 분노태세's aura, the per-frame handler `a.a(Lav;)V` ignores it and
/// the end handler `a.a(Lav;)Z` takes its default case. (`bd.a(La;BIZBBBB)Z`'s 4-frame
/// `bm(33, 4)` hit spark goes to the MONSTER's effect list, never the character's.)
/// Every game call goes through the game's helpers — the virtual dispatch with cells the
/// game resolves itself, and its signed divide — as in [`CombatPatch`].
///
/// ⛔ Install order: blocks → the cooldown `<clinit>` halfwords → the cast pool word → the
/// cast halfwords → the strike def word → the tick pool word → the tick's `bx r0` LAST (the
/// only halfword the status handler runs differently). A verify that fails puts every arm
/// back. Nothing is written to the character or a save: unpatched, the shipped 맹독감염 is back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VenomPatch {
    /// `(def, body)` of `ay.a(BB)Z`.
    pub strike: (u32, u32),
    /// Absolute cells (sl + literal): `g.a(B)Z` (can this status be added) · `a.a()Lbd;`
    /// (the facing monster) · `a.a(Lbd;BI)V` (inflict) · `a.a()I` (the attack roll) ·
    /// `bd.a(IB)V` (the monster takes damage).
    pub cell_has_status: u32,
    pub cell_front: u32,
    pub cell_inflict: u32,
    pub cell_attack: u32,
    pub cell_damage: u32,
    /// The game's virtual-call helper and its signed divide (`r0 / r1`, a leaf).
    pub dispatch: u32,
    pub divide: u32,
    /// Static slot of the character (`ag.a:La;`).
    pub char_slot: u32,
    /// ★ 2026-10-07 격분: the class static `ag.a:B` — the aura pick draws the berserker's own sequence for buff 33.
    pub class_slot: u32,
    /// `sl + 0x20`: the class table pointer the status handler keeps in `sb` for its `new`.
    pub class_table_slot: u32,
    /// Absolute field offsets: the character's `a.p:[B` · the monster's state `g.t` · HP `bd.d:S`.
    pub skills_off: u32,
    /// ★★ 2026-10-16 (user: *"맹독데미지에 지력 계수도 추가할꺼야"*) the character's `a.i:S` (지력) and `a.b:[S` (the equipment's
    /// ability bonus — `[4]` = 지력) — the tick's attack + 지력' × [`Self::INT_NUM`] / [`Self::INT_DEN`] (= `DemonPatch::int_off` ·
    /// `bonus_off`, the claw's 지력').
    pub int_off: u32,
    pub bonus_off: u32,
    pub mob_state_off: u32,
    pub mob_hp_off: u32,
    /// `(pool word, shipped literal)` the tick's HP load reads (0x126772 only).
    pub tick_pool: (u32, u32),
    /// `(site, shipped, patched)`: `adds r1,r7,#0` → `bx r0`, right after that load.
    pub tick_arm: (u32, u16, u16),
    /// Where the status handler goes on: its poison bubble (`new bt(21)`), `r4` = the monster.
    pub tick_resume: u32,
    /// `(pool word, shipped literal, patched literal)`: case 8's call cell → 민첩태세's buff cell.
    pub cast_pool: (u32, u32, u32),
    /// `(site, as shipped, patched)` — the cast, then the cooldown.
    pub cast: &'static [(u32, u16, u16)],
    pub cooldown: &'static [(u32, u16, u16)],
    /// `(pool word, shipped literal)` of `bm.a(II)V`'s `av.b` read at 0x133d3c (one reader).
    pub effect_pool: (u32, u32),
    /// `(site, shipped, patched)`: `adds r1,r4,#0` → `bx r0`, right after that load.
    pub effect_arm: (u32, u16, u16),
    /// Where the effect draw goes on: `add r0,sl` of the `av.b` read.
    pub effect_resume: u32,
    /// Absolute field offset of `av.a:I`, an effect's lifetime in frames (`bm.a()V` ends it at
    /// `av.b >= av.a`) — the buff's frames, or the hit spark's 4.
    pub effect_life_off: u32,
}

impl VenomPatch {
    /// The block: the strike wrapper, the tick, then the aura pick.
    pub const BLOCK_LEN: u32 = 0x180;
    pub const STRIKE_AT: u32 = 0x000;
    pub const TICK_AT: u32 = 0x080;
    pub const EFFECT_AT: u32 = 0x140;
    /// ★ 2026-10-08 밤 (가디언 장비): a data word past the three pieces — the address of the gear block's effect words
    /// ([`GearPatch`], 0 until that block is in). The tick adds `percent × W_VENOM >> 8` (터줏대감의 송곳니 · 독사의 장갑).
    /// Written by the host at the gear install (not part of the pieces the verify compares).
    pub const GEAR_PTR: u32 = 0x17c;
    /// The rogue sheet's poison motes: the sequence `tools_re/venom_effect.py` appends to
    /// `hero/hero2` (the shipped sheet has 0..=16).
    pub const AURA_SEQ: u8 = 17;
    /// `bd.a(La;BIZBBBB)Z`'s hit spark is `bm(33, SPARK_LEN)` — it keeps its shipped sprite.
    pub const SPARK_LEN: u8 = 4;
    /// The rogue's 기본공격 stages (`a.i[2]`): kinds `0..STAGES` are the combo, the rest skills.
    pub const STAGES: u8 = 5;
    /// The buff (a character status) and the poison (a monster status, the game's own id).
    pub const BUFF_ID: u8 = 33;
    pub const POISON_ID: u8 = 26;
    /// Frames of poison a landed hit applies: 5 s, the ticks at 0 · 10 · … · 90.
    pub const POISON_LEN: u8 = 100;
    /// `a.p` indices (the rogue has 5 기본공격 stages first) and top levels (`ay.a(I)B`).
    pub const VENOM_SKILL: u8 = 5 + 3;
    pub const VENOM_MAX: u8 = 5;
    pub const MASTERY_SKILL: u8 = 5 + 7;
    pub const MASTERY_MAX: u8 = 3;
    /// A tick = the attack × (`VENOM_PCT`·맹독감염 + `MASTERY_PCT`·독연마) / 100 — **300 %** at 5 + 3. ★ 2026-10-09 (4) it went
    /// 300 → 200 % (user, after the guardian balance §6 「로그 너무 쉬움」: 42·5 + 30·3 → 28·5 + 20·3); ★★ 2026-10-14 back to
    /// 300 % (user: *"맹독도 300%롤백"* — that verdict stood on a model that counted per-second values 2.5× too large,
    /// HERO2_NEW_ITEM_DRAFT.md §4.9). Lv1 = 42 + 90 = 132 %.
    pub const VENOM_PCT: u8 = 42;
    pub const MASTERY_PCT: u8 = 30;
    /// ★★ 2026-10-16 (user: *"맹독데미지에 지력 계수도 추가할꺼야"* — ⚠ the coefficient is Claude's, 100 %): a tick's attack is
    /// `a.a()I + 지력' × INT_NUM / INT_DEN` (지력 1 = attack 1) before its percent — a 민첩 rogue (지력' ≈ 10) gets a few %, a 지력
    /// rogue (악의 · 악몽의 독니) its 지력 in its poison.
    pub const INT_NUM: u8 = 100;
    pub const INT_DEN: u8 = 100;
    /// The buff = `BUFF_BASE + BUFF_PER_VENOM`·맹독감염 `+ BUFF_PER_MASTERY`·독연마 frames (20 fps).
    /// ★ 2026-10-06 (user: 버프 쿨 1분 · 「만렙 지속 75초」 · 「전부 ×1.25」): `250 + 250·맹독감염` = 25 · 37.5 · 50 · 62.5 ·
    /// 75 s, nothing from 독연마 (folded in, and the length is the skill's alone now); was `150 + 50·v + 60·d`.
    pub const BUFF_BASE: u8 = 250;
    pub const BUFF_PER_VENOM: u8 = 250;
    pub const BUFF_PER_MASTERY: u8 = 0;

    /// A level byte as the blocks read it: unsigned, anything past the top (−1 = 255, a
    /// skill not learned) counts as 0.
    pub const fn level(byte: u8, max: u8) -> u32 {
        if byte <= max { byte as u32 } else { 0 }
    }
    /// ★ 2026-10-06 (user: *"로그의 독연마 패시브 스킬 효과를 맹독 액티브 스킬에 바로 추가할꺼야 … 맹독은 이제 항상
    /// 독연마를 마스터한 효과를 가지게 되는거야"*): the cast and the tick take 독연마 as [`Self::MASTERY_MAX`] whatever
    /// `a.p[12]` holds — that slot is the passive 「심안」 now (see [`GuestBuild::rogue_patch`]). These two are what they
    /// compute: 맹독감염 Lv1 = 25 s · 132 %, Lv5 = 75 s · 300 % (the length since 2026-10-06; it was 19 → 29 s; ★ 2026-10-09 (4) 300 → 200 %,
    /// ★★ 2026-10-14 back to 300 %).
    pub const fn buff_frames_mastered(venom: u8) -> u32 {
        Self::buff_frames(venom, Self::MASTERY_MAX)
    }
    pub const fn tick_percent_mastered(venom: u8) -> u32 {
        Self::tick_percent(venom, Self::MASTERY_MAX)
    }
    /// The buff's length in frames for the two level bytes — the formula (the blocks use [`Self::buff_frames_mastered`]).
    pub const fn buff_frames(venom: u8, mastery: u8) -> u32 {
        Self::BUFF_BASE as u32 + Self::BUFF_PER_VENOM as u32 * Self::level(venom, Self::VENOM_MAX) + Self::BUFF_PER_MASTERY as u32 * Self::level(mastery, Self::MASTERY_MAX)
    }
    /// A tick's share of the attack in % for the two level bytes — what the tick computes.
    pub const fn tick_percent(venom: u8, mastery: u8) -> u32 {
        Self::VENOM_PCT as u32 * Self::level(venom, Self::VENOM_MAX) + Self::MASTERY_PCT as u32 * Self::level(mastery, Self::MASTERY_MAX)
    }
}

/// ★★ 2026-10-06 — the ROGUE's two: 「살상연희」 (번개난타's slot) and 「심안」 (the ghosts).
///
/// User: *"번개난타를 … 살상연희 스킬로 바꾸고싶어. 살상연희는 잠시동안 반투명 상태로 주변 5x5(캐릭터 위치 중앙 기준)타일 내의
/// 가장 가까운 순서대로 적의 뒤로 순간이동하며 공격을 하는 스킬이고 총 8마리의 적을 공격할 수있어. … 순간이동할 공간이 없다면 적의
/// 뒤가 아닌 앞으로 … 이동할 공간이나 5x5내의 몬스터가 없다면 사용 불가능해. 한명의 적은 5lv기준 최대 3번까지만 맞을 수있어(1lv
/// 기준 최대 1번) … 2마리를 번갈아가며 … 최대 타격수는 마스터 기준 8이야. 1lv 기준은 3이야. 이펙트는 반투명 잔상을 남기며 순간이동해서
/// 적을 칼로 긋는 듯한"* + *"때릴때 빗나갈 수 있으니 극사처럼 … 8방향 때릴수있게"* → asked: the levels between = 3·1 · 4·1 · 5·2 ·
/// 6·2 · 8·3 ([`Self::TOTALS`]; ★ 2026-10-08 (5) the cap per monster is gone — see [`Self::REACH`]) · a hit = one 번개난타 blow, its SP, cooldown **10 s** · untouchable while it lasts ·
/// the 5×5 fixed where it was cast (★ 2026-10-06 11×11 — [`Self::REACH`]). And 「심안」 (the old 독 연마, see below).
///
/// ── 살상연희 (`a.p[10]`, kind 10, quickbar value 6, action sequence 14) ────────────────────────────────
/// Five definition words, four of them CHAINED (the install reads what each holds — the 일풍격 / accessory wrappers or the
/// shipped body — into the block's data, and calls that for everything that is not 살상연희):
///
/// | hook | does |
/// |---|---|
/// | `ay.b(B)Z` (usable) | kind 10 · 16: the chain first with kind 10 (the shipped weapon gate — a main hand and an off-hand), then the PLAN: ★ (5) the ONE nearest live monster within the kind's reach of the caster ([`Self::REACH`] 25 = 51×51 · [`Self::BLOOM_REACH`] ★ 2026-10-16 255 = the whole map; Chebyshev, the least squared distance, a tie to the earlier on the list), usable only when it has a LANDING (`lands`, below) — else false: not cast, no SP, no cooldown |
/// | `ay.a()Z` (the drive) | kind 10 · 16, once per action frame: frame 0 starts the run (TOTAL from `a.p[10]` 1..=5 — 개화 BLOOM_TOTAL —, no target, the anchor = the caster's tile); hop h at frames 3h (★ (5) ENSURE the target: the one struck now if it still stands on the stage's list, else the nearest live monster within the reach OF THE ANCHOR — 살상연희: the last target's tile; 개화: the cast tile —, none → the end; then TELEPORT: `lands` enumerates the free tiles touching the target's BODY on the grid (a one-tile monster's eight neighbours, 오딘's 3×5 rim) ordered by the target's facing — its back · back diagonals · sides · front · front diagonals —, the blow's turn (ROT mod the count) picks one, an afterimage of the pose last drawn is left and the move `g.u()` · x/y · `ad.a()` · `g.t()` · the camera, facing it; no landing: the rogue stays) and 3h + 1 (ENSURE again — one acquired here is teleported to first — then the blow ON THE TARGET BY HANDLE: `this.a(Lbd;BIBB)Z(t, 10, a.a()I, a.l, a.m)` = 번개난타's own `a.f(B)Z` blow, the class's ×(lv + 6)/3 and SkillPower ×1.5 on top, 개화 ×3/2 — the monster's own evasion / guard the only miss; HOPS · ROT + 1, 살상연희's anchor ← its tile); all [`Self::TOTALS`] spent, or nobody left, ENDS it: `g.w ← a.n[kind] − 1`, so the driver's `g.w == a.n[kind]` closes the action at the next tick. The shipped case 10 (two blows at frames 3 · 6) never runs |
/// | `a.a(BBII)V` (the character draw) | the player: its pose is remembered (the next afterimage); the afterimages (up to [`Self::TRAILS`], each [`Self::TRAIL_DRAWS`] draws, fading) drawn through the game's own `n.a(BBBII)V` under `Graphics.setAlpha(ALPHA_TAG | a)`; in sequence 14 the body at [`Self::SELF_ALPHA`], then the slash-only sequence [`Self::SLASH_SEQ`] at the same frame opaque over it |
/// | `ay.a(I)S` (the cooldown) | index 5 → [`Self::COOLDOWN`] (10 s: the answer, so #17's halving of the table does not apply) |
/// | `bd.a(II)V` (the plain monster's paint) | 심안 — below. Not chained: nothing else wraps it |
///
/// **Untouchable**: from the first hop the drive sets `a.s:Z` (민첩태세's flag — the COMBAT block's HP guard and hit guard
/// read it) when it is clear, remembering that it did ([`Self::OWN_S`]), and gives it back at the end; the draw wrapper's
/// watchdog gives it back too if the action ended any other way (`g.t` no longer 3 or the queued kind no longer 10).
/// Every handle the run keeps across frames is checked on the stage's entity list before it is dereferenced (a monster
/// that died and left the list may be collected).
///
/// ── 심안 (`a.p[12]`, the old 독 연마) ───────────────────────────────────────────────────
/// A monster whose TEMPLATE has `cq.a:Z` (`enm/enm` body[20] — 스펙터 · 밴쉬 · 고스트 · 쉐도우 · 고대의 망령 · 떠도는 망령 ·
/// 떠도는 원혼 · 커럽티드 · 레버넌트) is born with `bd.h:Z` set (`bd.<init>(SSBB)V` 0x125ed4) — and the plain monster's
/// paint `bd.a(II)V` @0x12601c then skips its shadow and body (0x1260f2); a landed hit clears it
/// (`bd.a(La;BIZBBBB)Z` 0x127826, `bd.a(IBB)V` 74 % 0x127d5c). The same flag hides a body at the end of its fall
/// (`bd.c()V`, `bd.q` 3) — those stand at `g.t` 6. The paint's definition word 0 (**0x187b90**) goes to
/// [`Self::SIGHT_AT`]: a hidden ghost that is not falling or dead, seen by a rogue whose `a.p[12]` is 1 or more,
/// is drawn by the body with `bd.h` cleared for the call, under `Graphics.setAlpha(ALPHA_TAG | SIGHT_ALPHA)` (the
/// accessory block's host draw alpha), and `bd.h` set back after; anything else is the body as called. The four
/// boss classes override the paint (`ah`/`m`), and no boss template has the ghost flag — their own hiding stays.
/// Nothing here is saved: the flag is the monster's, and the level is held by `hero2java::h2_born_skill_repair`.
/// ★★ 2026-10-08 천공의검's picture — the host draws it (user, with ten DNF stills of 검제's awakening: *"나이트 천공의 검
/// 이펙트를 수정할꺼야. 스크린샷이 첫번째부터 이펙트 순서라고 생각하면돼. 3갈래를 차례대로 그으면서 데미지가 들어가는거야
/// … 그리고 마지막 폭발 데미지가 들어가고"*).
///
/// The knight's sheet `hero/4/1` is 4.8 KB under its 16-bit offset ceiling, so the three huge slashes, the cracks and the
/// flash are not sprites: a wrapper on the CHARACTER DRAW `a.a(BBII)V` (chained, as the rogue block chains it) draws the
/// body first and then, for the player knight in sequence [`Self::SEQ`] (천공의검's), lines and rectangles through the
/// game's own Graphics (`ax.a`, the one the sprites draw through) — `setAlpha` with the host tags for translucency and
/// for the STROKE (`wie_backend::canvas`: a line drawn under it is an anti-aliased capsule whose half-width tapers from its
/// start to its end, hard-edged or soft), `setColor`, `drawLine`, `fillRect` through the interface helper. Everything is
/// a function of the action frame `g.w`, the facing `g.u` and the draw's (x, y): no state.
///
/// ★★ 2026-10-08 (3) (user: *"검으로 공간이 찢어지는 듯한 느낌 … 캐릭터 가까운쪽은 뭉뚝하고 멀어지는 끝으로 갈수록 … 날카로운 …
/// 1번,2번,3번 찌를때 0.15초 간격으로 찌르고 3번째 찌른뒤에 0.3초 뒤 깨지고. 깨진뒤에 0.3초뒤 폭발"*): a prong is a TEAR — a
/// soft light-blue glow, a white rim and a black void, both following a torn polyline ([`Self::ALONG`] · [`Self::JAG`])
/// and tapering from blunt at the knight ([`Self::TAIL`], round cap) to a point at the head; it is THRUST (most of its
/// length on its first draw, a spark at the head on two); the break and the blast wait six frames each
/// (`CombatPatch::SKY_BREAK_FRAME` · `SKY_BLAST_FRAME`), the cracks are tapered rims over a white flash, the blast plays
/// [`Self::BLAST_DRAWS`] draws.
/// ```text
/// frame 2 · 5 · 8   a prong is thrust (a front diagonal · ahead · the other diagonal, CombatPatch::SKY_SLASH_FRAMES):
///                   HEADS 64 · 104 · 120 px over its first draws, SPARK at the head on two; the earlier ones stay
/// frame 14 …        the break: BREAK_FLASH over each prong for two draws, the cracks (revealed 6 · 10 · 14) stay
/// frame 20 … 26     the blast: eight tapered rays and a white disc over a light-blue one, growing and fading
/// ```
///
/// ★★ 2026-10-08 (3) — and 개화's FLOOR (the rogue's awakening; user, with a red snowflake and the anime's glowing compass
/// needle: *"바닥에 꽃모양을 그린다고했는데 … 같은 모양으로 바닥에 그러져야하고 스킬이 종료된 후에 서서히 사라지는 듯 없어져야해 …
/// 발도 사용시 원형이아니라 살짝 눕혀진듯한 느낌이 드는 타원형이었던것처럼 이번에도 마찬가지야"*): a second entry on the GROUND
/// DRAW `bf.a(Graphics;II)V` (`r1` the stage · `r3` the camera's x · `[sp]` its y — screen = world + camera), which the
/// stage draw `bf.e()V` calls once a frame right before it walks the entity list (0x12c026 → 0x12c02a) — so the floor is
/// under every monster and the rogue, not only under the body. It reads the rogue block's FLOWER word (`RoguePatch::FLOWER`
/// — the cast tile, set while 개화 runs; its address is a data word the install writes): a new value starts the bloom
/// ([`Self::UNFOLD`]), every draw while it lasts holds the fade level at [`Self::FADE_FULL`], and once it is 0 the level
/// falls one a draw — the picture's alphas × level / 32 — then nothing. The picture: a faint lit ellipse
/// ([`Self::TINT`]), the snowflake ([`Self::floor_segments`] — twelve spokes, the long six with two barb pairs and a
/// hexagon, the short six with one pair and a diamond, a hexagon at the heart — laid on the ground at
/// [`Self::FLOOR_SQUASH`], 발도's ring's tilt) as a soft cyan glow under a thin white core, a dot at each spoke's tip and
/// light rising from the long spokes' hexagons ([`Self::PILLAR_H`]), all breathing ([`Self::PULSE`]). The rogue block
/// no longer draws its sprite flower (sequence 22 stays in the sheet, unused).
///
/// ★★ 2026-10-08 (5) (user, with the DNF stills again: *"천공의 검 이펙트와 데미지 범위를 업그레이드 하자 … 캐릭터기준 바닥과 공간이
/// 깨진듯한 연출을 참고하면돼 … 화면 위아래로 시전했을때와 좌우로 시전했을때 각 천공의 검 이펙트간의 간격이 달라야해 … 개화도 이펙트가
/// 조금 더 누워져야해"*): the whole 천공의검 picture but the blast moved to the GROUND draw, under every entity, drawn in GROUND
/// coordinates (ahead · side in the facing's frame) and laid at [`Self::FLOOR_SQUASH`] (now 9/16) — so a horizontal cast
/// fans its prongs ±19° on the screen and a vertical one ±36°, and the knight stands in front of the void. The character
/// draw's wrapper only STORES his state (`SKY_CX` · `SKY_CY` · `SKY_FRAME` · `SKY_FACING`, `SKY_SEEN` 0) for the ground
/// draw of the next frame (one frame late — 0.05 s) and draws the blast over the body. New on the ground: a CRACKED FLOOR
/// under the knight from the first frame ([`Self::SKY_FLOOR`] two lit ellipses, [`Self::sky_floor_burst`] eight soft rays,
/// [`Self::sky_floor_cracks`] twelve kinked cracks), unfolding ([`Self::SKY_UNFOLD`]) and fading after the action
/// ([`Self::SKY_LEVEL`] counts down once the character draw stops storing — `SKY_SEEN` past [`Self::SKY_LIVE`]). The
/// tears reach [`CombatPatch::SKY_DEPTH`] tiles ahead (448 px, [`Self::HEADS`]) and [`CombatPatch::SKY_BACK`] behind
/// ([`Self::ALONG`] signed), the fan's slopes [`Self::PRONG_S`] = ∓½ · 0 · ±½; the break's cracks are prong-relative
/// ([`Self::CRACK_SEGS`] — four branches and a shard edge off each). Six code pieces (the layout below).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SkyDrawPatch {
    /// The definition record of `a.a(BBII)V` (= `AccessoryPatch::draw.0`) — CHAINED: the block keeps what the word held
    /// when it went in (the accessory / rogue wrappers, or the body) and calls that first.
    pub draw: u32,
    /// ★ 2026-10-08 (3): `(definition record, body)` of the ground draw `bf.a(Graphics;II)V` (the stage's dropped items;
    /// definition #417 at **0x1883e0**, image word 0x215d5 + 0x10bf10 = 0x12d4e5) — chained the same way (nothing else
    /// wraps it today, so the chain is the body).
    pub ground_draw: (u32, u32),
    /// ★ 2026-10-09 `(definition record, body)` of the HUD painter `n.d()V` (definition #1165 at **0x18d5b0**, image word
    /// 0x57a55 + 0x10bf10 = 0x163965) — `n.paint` calls it right after the stage's paint `bf.e()V`, so a wrapper on it
    /// draws over the whole field (tiles · entities · canopies) and under the HUD it chains to. Chained like the others
    /// (nothing else wraps it today; the target panel's patches are inside its body, not on this word).
    pub hud_draw: (u32, u32),
    /// The interface call helper · the Graphics the sprites draw through (`ax.a`) · the game's own reference words for
    /// `setAlpha(I)V` · `setColor(I)V` · `fillRect(IIII)V` (`*(sl + 0x17c / 0x88 / 0x9c)` = pointers to the
    /// `(name index << 16) | class` word the resolver rewrites in place).
    pub iface_call: u32,
    pub graphics_slot: u32,
    pub set_alpha_ref: u32,
    pub set_color_ref: u32,
    pub fill_rect_ref: u32,
    /// Statics: the class `ag.a:B` · the character `ag.a:La;`.
    pub class_slot: u32,
    pub char_slot: u32,
    /// The character (absolute): `g.u` (the facing, 1..=4 on the field) · `g.w` (the action frame).
    pub facing_off: u32,
    pub frame_off: u32,
}

impl SkyDrawPatch {
    /// ★ 2026-10-08 (3): 0x800 → 0x1000 (the floor's code and its snowflake table). ★ (5): 0x1000 → 0x1c00 — FIVE code
    /// pieces, each with its own literal pool (an `ldr =` reaches 1020 bytes): the character draw's entry at SKY_AT (it
    /// stores the knight's state and draws the blast over the body), the blast at BLAST_AT, the ground draw's entry at
    /// FLOOR_AT (the two states — 개화's flower, 천공의검's — then the pieces that paint), 개화's snowflake at FLOWER_AT,
    /// 천공의검's cracked floor at SKYFLOOR_AT and its tears and break at TEAR_AT.
    /// ★ 2026-10-09: 0x2400 → 0x2c00 — 격분's blood: its data at BLOOD_DATA and three pieces.
    pub const BLOCK_LEN: u32 = 0x2c00;
    /// Data words: the chain, then our `drawLine` reference word, then scratch the routines share.
    pub const CHAIN_DRAW: u32 = 0x00;
    /// No game word references `drawLine(IIII)V` — ours: name index 0x59 of class 0x169 (Graphics), the shape the
    /// game's own words have before the resolver (0x1675d0) rewrites them with the code pointer (it rewrites ours too).
    pub const LINE_REF: u32 = 0x04;
    pub const LINE_REF_WORD: u32 = (0x59 << 16) | 0x169;
    /// ★ (3): the ground draw's chained word · the address of the rogue block's FLOWER word (0 = no rogue block: no floor).
    pub const CHAIN_GROUND: u32 = 0x08;
    pub const FLOWER_PTR: u32 = 0x0c;
    /// The picture's centre on the screen · the prong's ground direction (ahead · side, 16ths — [`Self::PRONG_A`] ·
    /// [`Self::PRONG_S`]) · its head (px) · the frame and facing the painting piece works from · a count · a segment.
    pub const CX: u32 = 0x10;
    pub const CY: u32 = 0x14;
    pub const DX: u32 = 0x18;
    pub const DY: u32 = 0x1c;
    pub const HEAD: u32 = 0x20;
    pub const FRAME: u32 = 0x24;
    pub const FACING: u32 = 0x28;
    pub const COUNT: u32 = 0x2c;
    pub const X1: u32 = 0x30;
    pub const Y1: u32 = 0x34;
    pub const X2: u32 = 0x38;
    pub const Y2: u32 = 0x3c;
    /// ★ (3) the floor: the FLOWER value last seen (0 once it is gone) · the tile it named · draws since it appeared
    /// (capped at 255) · the fade level (FADE_FULL while it lasts, one less a draw after; 0 = nothing) · this draw's
    /// scale (16ths) · the alpha factor `PULSE × level` (/ 512).
    pub const LAST_FLOWER: u32 = 0x40;
    pub const TILE: u32 = 0x44;
    pub const AGE: u32 = 0x48;
    pub const LEVEL: u32 = 0x4c;
    pub const SCALE: u32 = 0x50;
    pub const SHADE: u32 = 0x54;
    /// ★ (5) 천공의검's state, stored by the CHARACTER draw (the only one that knows the sequence) for the GROUND draw of
    /// the next frame: the centre on the screen · the action frame · the facing (1..=4) · draws of the ground since the
    /// character draw last stored (0 there; the ground draw adds one — ≤ SKY_LIVE means the knight is in the action) ·
    /// the fade level of the cracked floor (FADE_FULL while the action lasts, one less a draw after) · draws since the
    /// action began (the floor's unfolding, capped at 255).
    pub const SKY_CX: u32 = 0x58;
    pub const SKY_CY: u32 = 0x5c;
    pub const SKY_FRAME: u32 = 0x60;
    pub const SKY_FACING: u32 = 0x64;
    pub const SKY_SEEN: u32 = 0x68;
    pub const SKY_LEVEL: u32 = 0x6c;
    pub const SKY_AGE: u32 = 0x70;
    /// ★ (6) scratch: the blade's width factor (quarters) for the poly routine.
    pub const KW: u32 = 0x74;
    pub const SKY_LIVE: u8 = 2;
    pub const DATA_LEN: u32 = 0x80;
    pub const CODE_AT: u32 = 0x80;
    pub const SKY_AT: u32 = Self::CODE_AT;
    pub const BLAST_AT: u32 = 0x200;
    pub const FLOOR_AT: u32 = 0x600;
    pub const FLOWER_AT: u32 = 0x900;
    pub const SKYFLOOR_AT: u32 = 0x1000;
    pub const TEAR_AT: u32 = 0x1400;
    /// ★ (6) the glass: the fracture web, the dark (★ (7) and the spray of light) — on the ground draw too.
    pub const WEB_AT: u32 = 0x1b00;
    // ── ★ 2026-10-09 격분's BLOOD over the whole field (user: *"격분을 재사용해서 폭발할떄 이펙트가 전맵에 피가 폭발하는 듯한
    //    이팩트가 있었으면 좋겠어"*). The berserker block's blast records where it struck ([`BerserkerPatch::BLOOD_SEQ`] ..
    //    [`BerserkerPatch::BLOOD_XY`]); a wrapper on the HUD painter ([`Self::hud_draw`] — after the whole field) sees a new
    //    BLOOD_SEQ and paints BLOOD_DRAWS draws: a red flash over the field, sixteen blood streaks bursting out of the
    //    berserker and falling, splats landing across the field in a wave, and at every monster the blast hit a dark splat
    //    with eight drops flying off it. Positions are world px + the camera the ground draw got this frame
    //    ([`Self::CAM_X`] — the floor entry stores it), so the blood stays where it fell while the view moves. ──
    /// The blood's data words (beyond the first DATA_LEN — the block grew): the HUD painter's chain · the berserker block
    /// (0 = none: no blood) · the BLOOD_SEQ last seen · draws since it changed (BLOOD_DRAWS = idle) · the camera · the
    /// burst's centre on the screen this draw · the monster index and count (the mob piece's loop).
    pub const BLOOD_CHAIN: u32 = 0x2400;
    pub const BLOOD_PTR: u32 = 0x2404;
    pub const BLOOD_LAST: u32 = 0x2408;
    pub const BLOOD_AGE: u32 = 0x240c;
    pub const CAM_X: u32 = 0x2410;
    pub const CAM_Y: u32 = 0x2414;
    pub const BCX: u32 = 0x2418;
    pub const BCY: u32 = 0x241c;
    pub const BM: u32 = 0x2420;
    pub const BN: u32 = 0x2424;
    pub const BLOOD_DATA_LEN: u32 = 0x40;
    /// The entry on `n.d()V` · the flash, the burst and the splats · the monsters' bursts.
    pub const BLOOD_AT: u32 = 0x2440;
    pub const BLOOD_PAINT_AT: u32 = 0x2540;
    pub const BLOOD_MOB_AT: u32 = 0x2900;
    /// 1.6 s at 20 draws a second.
    pub const BLOOD_DRAWS: u8 = 32;
    /// The red flash over the field by draw (alpha of [`Self::BLOOD_FLASH`]).
    pub const BLOOD_FLASH_A: [u8; 32] = [170, 100, 54, 28, 12, 5, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// Everything else's alpha by draw: whole for half the run, then fading out.
    pub const BLOOD_ALPHA: [u8; 32] = [255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 240, 220, 200, 180, 160, 140, 120, 100, 82, 66, 52, 40, 28, 18, 10, 4];
    /// The burst out of the berserker: sixteen streaks along [`Self::DIRS16`] — the head BURST_R2[t] × 2 px out
    /// (× BURST_LEN[i] / 8, a ragged rim), the tail at 3/8 of it, the stroke tapering from nothing at the tail to
    /// BURST_W[t] at the head (quarter px) over a dark underlay BURST_UNDER wider; both ends fall `t² >> 3` px.
    pub const BURST_R2: [u8; 32] = [12, 30, 48, 64, 78, 90, 100, 108, 114, 119, 123, 126, 128, 129, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130];
    pub const BURST_W: [u8; 32] = [22, 25, 25, 24, 23, 21, 19, 17, 15, 13, 12, 11, 10, 9, 8, 7, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6];
    pub const BURST_UNDER: u8 = 7;
    pub const BURST_LEN: [u8; 16] = [8, 6, 7, 5, 8, 7, 6, 8, 5, 7, 8, 6, 7, 5, 8, 6];
    /// A monster's burst: a dark splat (a hard dot of half-width SPLAT_W[t]) and eight drops along DROP_DIRS (64ths) —
    /// the head DROP_R[t] px × DROP_LEN[(i + m) & 7] / 8 out, the tail at 3/8, tapering to DROP_W[t]; they fall `t² >> 4`.
    pub const SPLAT_W: [u8; 32] = [8, 20, 30, 38, 44, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48];
    pub const DROP_R: [u8; 32] = [4, 11, 18, 24, 29, 33, 36, 38, 40, 41, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42];
    pub const DROP_W: [u8; 32] = [12, 16, 15, 14, 13, 12, 11, 10, 9, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8];
    pub const DROP_DIRS: [(i8, i8); 8] = [(64, 6), (42, 47), (-4, 64), (-47, 43), (-64, -8), (-44, -46), (6, -64), (46, -44)];
    pub const DROP_LEN: [u8; 8] = [8, 6, 7, 5, 8, 6, 7, 5];
    /// The splats across the field: SPLATS of [`Self::blood_splats`] — each lands on its draw and grows over three.
    pub const SPLATS: usize = 24;
    /// `(dx, dy px from the berserker's centre — the vertical laid at FLOOR_SQUASH, the draw it lands on, its half-width
    /// in quarter px)`: around the berserker at 50..300 px, landing outward (a draw per 24 px), 4..11 px across.
    pub fn blood_splats() -> Vec<(i16, i16, u8, u8)> {
        let mut seed = 0x5eed_0b10u32;
        (0..Self::SPLATS)
            .map(|i| {
                let th = (i as f64 * 360.0 / Self::SPLATS as f64 + f64::from(Self::lcg(&mut seed, 13) - 6)).to_radians();
                let r = 50.0 + f64::from(Self::lcg(&mut seed, 250));
                let (dx, dy) = ((th.cos() * r).round() as i16, ((th.sin() * r * f64::from(Self::FLOOR_SQUASH.0)) / f64::from(1u8 << Self::FLOOR_SQUASH.1)).round() as i16);
                let delay = (1.0 + r / 24.0) as u8;
                let size = 16 + Self::lcg(&mut seed, 29) as u8;
                (dx, dy, delay, size)
            })
            .collect()
    }
    pub const CLASS: u8 = 1;
    pub const SEQ: u8 = CombatPatch::SKY_SEQ;
    /// The picture's centre is the tile's (x + 8, y + 6) — the body's middle.
    pub const CENTRE: (u8, u8) = (8, 6);
    pub const BLACK: u8 = 0;
    pub const WHITE: u8 = 1;
    pub const LBLUE: u8 = 2;
    pub const FTINT: u8 = 3;
    pub const FGLOW: u8 = 4;
    pub const FCORE: u8 = 5;
    pub const PURPLE: u8 = 6;
    pub const DARK: u8 = 7;
    /// ★ (7) The space the tail opens onto (the sheet's colour from the blast).
    pub const SPACE: u8 = 8;
    /// ★ 2026-10-09 격분's blood: the bright drops · the dark splats and underlay · the field's red flash.
    pub const BLOOD: u8 = 9;
    pub const BLOOD_DARK: u8 = 10;
    pub const BLOOD_FLASH: u8 = 11;
    pub const COLOURS: [u32; 12] = [0x080919, 0xfff6e6, 0x9bdcff, 0x384ab0, 0x74bfff, 0xf4efff, 0xbd91ed, 0x171225, 0x302750, 0xed3440, 0x640f26, 0x9c1630];
    /// `wie_backend::canvas::{STROKE_TAG, STROKE_SOFT_TAG}` (the app asserts them equal): `setAlpha(tag | w0 << 8 | w1)`
    /// makes the next lines tapered capsules, half-widths in quarter pixels; `STROKE_TAG | 0` = the plain line again.
    pub const STROKE_TAG: u32 = 0x4833_0000;
    pub const STROKE_SOFT_TAG: u32 = 0x4834_0000;
    /// ★ 2026-10-09 (user: *"천공의 검 이펙트가 범위에 비해 너무 큰거같아 … 퀄리티 그대로두고 이펙트의 크기만 살짝 줄여줘"*): the
    /// whole 천공의검 picture × SIZE.0 / SIZE.1 — every length (the blades, the floor, the web, the spray, the flash, the
    /// rays, the shards) and the bold shapes' widths (the blade's rim · void · glows, the break's flash, the shards' slabs);
    /// the hairlines (the web, the spray, the cracks, the rays' cores) and every alpha as they were. The damage box is
    /// untouched (`CombatPatch::SKY_*`). One knob: (1, 1) is the (7) picture byte for byte.
    pub const SIZE: (i32, i32) = (4, 5);
    /// `v × SIZE`, rounded half away from zero.
    pub const fn sz(v: i32) -> i32 {
        let (m, d) = Self::SIZE;
        if v < 0 {
            -((-v * m * 2 + d) / (2 * d))
        } else {
            (v * m * 2 + d) / (2 * d)
        }
    }
    const fn sz8(v: u8) -> u8 {
        Self::sz(v as i32) as u8
    }
    const fn szi8(v: i8) -> i8 {
        Self::sz(v as i32) as i8
    }
    const fn szw(w: (u8, u8)) -> (u8, u8) {
        (Self::sz8(w.0), Self::sz8(w.1))
    }
    const fn sz_arr<const N: usize>(a: [u8; N]) -> [u8; N] {
        let mut o = a;
        let mut i = 0;
        while i < N {
            o[i] = Self::sz8(a[i]);
            i += 1;
        }
        o
    }
    /// SIZE as an `f64` for the picture's generators (the web, the shards, the floor's cracks).
    fn szf() -> f64 {
        f64::from(Self::SIZE.0) / f64::from(Self::SIZE.1)
    }

    // ── 천공의검: the tears — ★ (5) on the GROUND plane ──
    //
    // User (with the DNF stills): *"캐릭터기준 바닥과 공간이 깨진듯한 연출을 참고하면돼 … 화면 위아래로 시전했을때와 좌우로 시전했을때
    // 각 천공의 검 이펙트간의 간격이 달라야해 … 좌우와 위아래 이펙트가 동일하면안돼"*. The whole picture (the floor, the tears,
    // the cracks) is drawn in GROUND coordinates — ahead along the facing, side across it — and laid on the screen like
    // 개화's floor: a ground vector (a, s) lands at `(CX + fx·a + sx·s, CY + (fy·a + sy·s) × FLOOR_SQUASH)` with the
    // facing's axes (`CombatPatch::axes`). So a knight facing right fans his prongs ±19° on the screen (the side
    // squashed), facing up ±36° (the length squashed): the two casts differ, as asked. It is drawn by the ground draw,
    // under every entity — the knight stands in front of the void, as in the stills (10-08 (3) drew it over the body).
    /// A prong's ground direction: ahead PRONG_A per 16, side PRONG_S[k] per 16 — the three prongs in the order they are
    /// thrust (for a knight facing right: up-right, right, down-right — the stills' order), slopes ∓½ · 0 · ±½.
    pub const PRONG_A: i8 = 16;
    pub const PRONG_S: [i8; 3] = [-8, 0, 8];
    /// A prong's points along it, in 64ths of the head (signed — ★ (5) the blade reaches [`CombatPatch::SKY_BACK`] tiles
    /// BEHIND the knight too, as the damage does: −12/64 of 448 = 84 px): point 0 is the back end, point 1 the widest (just
    /// in front of the knight — «뭉뚝»), point 6 the head; straight (10-08 (4): «구부러진 부분을 곧게»).
    pub const ALONG: [i8; 7] = [-12, 2, 14, 26, 38, 51, 64];
    /// The white rim's and the black void's half-widths at the points (quarter px) — the void stops a point short, so the
    /// tip is the rim's; both blunt at the back, widest by the knight, a point at the head.
    pub const RIM_W: [u8; 7] = Self::sz_arr([24, 40, 34, 27, 19, 10, 0]);
    pub const VOID_W: [u8; 6] = Self::sz_arr([14, 26, 21, 16, 10, 4]);
    /// The glow: one soft stroke from the back end to GLOW_OVER px past the head, half-widths GLOW_W (quarter px).
    pub const GLOW_W: (u8, u8) = Self::szw((56, 8));
    pub const GLOW_OVER: u8 = Self::sz8(12);
    pub const GLOW_ALPHA: u8 = 150;
    /// ★ (7) A wide white glow under the light-blue one — the light pouring out of the tear.
    pub const WHITE_GLOW_W: (u8, u8) = Self::szw((96, 24));
    pub const WHITE_GLOW_ALPHA: u8 = 110;
    /// A prong's head by draws since it appeared — ★ (6) the full [`CombatPatch::SKY_DEPTH`] tiles (28 × 16 = 448 px) at
    /// once: the first draw is a thin CUT the whole way (the stills' second frame), the blade then widens on it.
    /// ★ 2026-10-09 × SIZE: 358 px ≈ 22 tiles (the box still reaches 28).
    pub const HEADS: [u16; 3] = [Self::sz(448) as u16; 3];
    /// ★ (6) The cut: on a prong's first THIN_ALPHA.len() draws a soft purple glow (THIN_GLOW_W) under a thin white core
    /// (THIN_W, quarter px) from the back end to the head, fading; the blade's widths × BLADE_K[draws since] / 4 (0 on
    /// the first draw: the cut alone, a bright opening followed by a brief recoil).
    pub const THIN_W: (u8, u8) = (7, 3);
    pub const THIN_GLOW_W: (u8, u8) = Self::szw((44, 16));
    pub const THIN_ALPHA: [u8; 3] = [255, 210, 115];
    pub const BLADE_K: [u8; 5] = [0, 2, 4, 3, 4];
    /// The thrust's spark at the head on its first two draws: `(soft half-width in quarter px, alpha)`.
    pub const SPARK: [(u8, u8); 2] = [(36, 240), (22, 140)];
    // ── the break ──
    /// The cracks off a prong, each `(a1, p1, a2, p2)` — along in 64ths of the head, across in px — four branches leaving
    /// it alternately, each with a shard edge off its tip (the glass in the stills); the same eight on every prong, revealed
    /// CRACK_REVEAL[draws since the break] of them (the branches first).
    pub const CRACK_SEGS: [(u8, i8, u8, i8); 8] = Self::sz_cracks([(12, 0, 17, 14), (26, 0, 31, -16), (40, 0, 45, 13), (54, 0, 58, -12), (17, 14, 25, 22), (31, -16, 39, -26), (45, 13, 53, 24), (58, -12, 64, -20)]);
    /// The crack segments' ACROSS offsets × SIZE (the along is in 64ths of the head, which SIZE already shortened).
    const fn sz_cracks(c: [(u8, i8, u8, i8); 8]) -> [(u8, i8, u8, i8); 8] {
        let mut o = c;
        let mut i = 0;
        while i < 8 {
            o[i] = (c[i].0, Self::szi8(c[i].1), c[i].2, Self::szi8(c[i].3));
            i += 1;
        }
        o
    }
    pub const CRACK_REVEAL: [u8; 3] = [3, 6, 8];
    /// A crack = a white rim then a black core over it, tapering from where it leaves the prong (quarter px).
    pub const CRACK_RIM: (u8, u8) = (6, 1);
    pub const CRACK_CORE: (u8, u8) = (3, 0);
    /// The break's white flash along each prong on its first two draws (a soft stroke, half-widths in quarter px).
    pub const BREAK_FLASH_W: (u8, u8) = Self::szw((60, 14));
    pub const BREAK_FLASH: [u8; 2] = [210, 110];
    /// ★ (7) The tears are drawn up to here — the break's flash is their last; the dark and the web take over.
    pub const TEAR_END: u8 = CombatPatch::SKY_BREAK_FRAME + Self::BREAK_FLASH.len() as u8;
    // ── the blast (over the body, on the character draw) ──
    /// Draws the explosion plays from `CombatPatch::SKY_BLAST_FRAME` — ★ (6) ten (0.5 s), the shards for SHARD_DRAWS.
    pub const BLAST_DRAWS: u8 = 10;
    /// The explosion by draw since SKY_BLAST_FRAME: the white disc's radius · its alpha (the light-blue one under it is
    /// half again as big at half the alpha) · the eight rays' length (diagonals ×3/4) · their alpha. A disc is 32
    /// `fillRect` bands tiling it exactly: for k in 0..16 a band below and one above the centre, `R·k >> 4` ..
    /// `R·(k+1) >> 4` from it, `2·(R·CHORD16[k] >> 8)` wide. ★ (5) bigger — the box is four times the lobe.
    pub const FLASH_HALF: [u8; 10] = Self::sz_arr([20, 44, 68, 92, 112, 128, 140, 148, 154, 158]);
    pub const FLASH_ALPHA: [u8; 10] = [255, 235, 185, 130, 88, 60, 40, 25, 14, 6];
    pub const RAY_LEN: [u8; 10] = Self::sz_arr([60, 96, 132, 164, 190, 208, 220, 228, 234, 238]);
    pub const RAY_ALPHA: [u8; 10] = [230, 210, 185, 155, 120, 90, 60, 38, 20, 8];
    /// ★ (6) The shards: SHARDS of them fly out from the blast (over the body) for SHARD_DRAWS draws — each `(dir x · y in
    /// 64ths, start radius px, speed px a draw, three vertex offsets px)` from [`Self::shards`], the triangle's three
    /// edges as hard white lines (SHARD_W quarter px) at SHARD_ALPHA[draw].
    pub const SHARDS: usize = 18;
    pub const SHARD_DRAWS: u8 = 16;
    pub const SHARD_W: (u8, u8) = (5, 5);
    /// ★ (7) Each shard is a slab: its first edge drawn thick and white under the light-blue outline.
    pub const SHARD_SLIVER_W: (u8, u8) = Self::szw((11, 11));
    pub const SHARD_ALPHA: [u8; 16] = [255, 245, 225, 245, 210, 190, 215, 175, 150, 170, 125, 105, 120, 70, 42, 18];
    /// A ray = a soft light-blue taper under a hard white one (quarter px, from the centre to its end).
    pub const RAY_GLOW_W: (u8, u8) = Self::szw((28, 2));
    pub const RAY_CORE_W: (u8, u8) = (8, 0);
    /// The eight directions clockwise on the screen (y down): E · SE · S · SW · W · NW · N · NE, with the length scale
    /// in eighths (a diagonal step is longer, so its rays take 6/8 of the length).
    pub const DIRS8: [(i8, i8, u8); 8] = [(1, 0, 8), (1, 1, 6), (0, 1, 8), (-1, 1, 6), (-1, 0, 8), (-1, -1, 6), (0, -1, 8), (1, -1, 6)];
    // ── ★ (6) the GLASS (user, the stills again: *"스크린샷 처럼 깨지는듯한 이펙트 연출"*): on the ground draw, between the
    //    cracked floor and the tears — a fracture WEB of thin white lines (a dark underlay for contrast) radiating from the
    //    knight across the screen, revealed outward with the prongs and complete at the break; ★ (7) a SPRAY of white light
    //    rays out of the knight (the (6) black WEDGES beside the blades were fat black pills on the dark — removed); the DARK —
    //    a translucent purple-black sheet over the whole ground from the break, deepest at the blast, then the SPACE's
    //    blue-purple lifting over the tail. All in screen space around the knight (no squash), turned with the facing. ──
    /// The web: WEB_RAYS polylines of three segments each from WEB_R0 px out to WEB_R3, two rings of connectors between
    /// neighbouring rays and a branch off each — `(x1, y1, x2, y2)` in ground px (ahead = +x), sorted by the radius each
    /// starts at ([`Self::web_segments`]); revealed by that radius: 0 before the first prong, growing WEB_RADIUS_END px by
    /// the break ([`Self::web_reveal`]), gone from the blast.
    pub const WEB_RAYS: usize = 16;
    pub const WEB_RADIUS_END: i32 = Self::sz(420);
    /// The web's two passes (quarter px, alpha): a dark underlay, a white core.
    pub const WEB_UNDER: (u8, u8) = (7, 170);
    pub const WEB_CORE: (u8, u8) = (3, 245);
    /// The dark: DARK_ALPHA[draws since the break] over the whole ground (a sheet DARK_HALF px each way from the knight).
    pub const DARK_HALF: u16 = 640;
    pub const DARK_ALPHA: [u8; 36] = [40, 75, 105, 125, 140, 150, 160, 165, 170, 175, 180, 185, 205, 185, 170, 155, 140, 125, 110, 96, 84, 72, 61, 51, 42, 34, 27, 21, 16, 12, 9, 6, 4, 2, 1, 0];
    /// ★ (7) The spray: SPRAY_RAYS white rays of light out of the knight in screen space (between the web's rays), from
    /// SPRAY_R0 px to `spray_q(frame) × 4` px — growing to SPRAY_END by the break, held to the blast, gone after.
    pub const SPRAY_RAYS: usize = 16;
    pub const SPRAY_W: (u8, u8) = (5, 1);
    pub const SPRAY_ALPHA: u8 = 200;
    pub const SPRAY_R0: u8 = Self::sz8(24);
    pub const SPRAY_END: i32 = Self::sz(560);
    /// Sixteen directions at 11.25° + k·22.5° (y down), in 64ths.
    pub const DIRS16: [(i8, i8); 16] = [(63, 12), (53, 36), (36, 53), (12, 63), (-12, 63), (-36, 53), (-53, 36), (-63, 12), (-63, -12), (-53, -36), (-36, -53), (-12, -63), (12, -63), (36, -53), (53, -36), (63, -12)];
    /// The spray's reach at an action frame, in quarter px (a byte for the Thumb's table).
    pub const fn spray_q(frame: u8) -> u8 {
        let (first, brk, blast) = (CombatPatch::SKY_SLASH_FRAMES[0], CombatPatch::SKY_BREAK_FRAME, CombatPatch::SKY_BLAST_FRAME);
        if frame < first || frame >= blast {
            0
        } else if frame >= brk {
            (Self::SPRAY_END / 4) as u8
        } else {
            ((frame - first) as i32 * Self::SPRAY_END / (brk - first) as i32 / 4) as u8
        }
    }
    /// [`Self::spray_q`] for every action frame.
    pub fn spray_table() -> Vec<u8> {
        (0..CombatPatch::SKY_FRAMES).map(Self::spray_q).collect()
    }

    /// A small deterministic generator for the glass (the picture must be the same bytes in the app, the probe and the
    /// tests) — a 32-bit LCG, `next(n)` in 0..n.
    fn lcg(seed: &mut u32, n: i32) -> i32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((*seed >> 16) as i32).rem_euclid(n.max(1))
    }

    /// The web's segments `(x1, y1, x2, y2, radius)`, sorted by the radius they start at — see [`Self::WEB_RAYS`].
    pub fn web_segments() -> Vec<(i16, i16, i16, i16, u16)> {
        let mut seed = 0x5eed_0601u32;
        let n = Self::WEB_RAYS;
        let mut pts: Vec<[(f64, f64); 4]> = Vec::with_capacity(n);
        for i in 0..n {
            let base = i as f64 * 360.0 / n as f64 + f64::from(Self::lcg(&mut seed, 17) - 8);
            let radii = [
                (28.0 + f64::from(Self::lcg(&mut seed, 16))) * Self::szf(),
                (100.0 + f64::from(Self::lcg(&mut seed, 40))) * Self::szf(),
                (200.0 + f64::from(Self::lcg(&mut seed, 50))) * Self::szf(),
                (320.0 + f64::from(Self::lcg(&mut seed, 70))) * Self::szf(),
            ];
            let mut ray = [(0.0, 0.0); 4];
            for (k, r) in radii.iter().enumerate() {
                let th = (base + f64::from(Self::lcg(&mut seed, 15) - 7) * (k as f64) * 0.5).to_radians();
                ray[k] = (th.cos() * r, th.sin() * r);
            }
            pts.push(ray);
        }
        let mut out: Vec<(i16, i16, i16, i16, u16)> = Vec::new();
        let rad = |p: (f64, f64)| (p.0 * p.0 + p.1 * p.1).sqrt();
        let seg = |out: &mut Vec<(i16, i16, i16, i16, u16)>, a: (f64, f64), b: (f64, f64)| {
            out.push((a.0.round() as i16, a.1.round() as i16, b.0.round() as i16, b.1.round() as i16, rad(a).min(rad(b)).round() as u16));
        };
        for ray in &pts {
            for k in 0..3 {
                seg(&mut out, ray[k], ray[k + 1]);
            }
        }
        for i in 0..n {
            let j = (i + 1) % n;
            for k in 1..=2 {
                let scale = 1.0 + f64::from(Self::lcg(&mut seed, 31) - 15) / 100.0;
                let b = (pts[j][k].0 * scale, pts[j][k].1 * scale);
                seg(&mut out, pts[i][k], b);
            }
            // a branch off the second segment
            let (a, b) = (pts[i][1], pts[i][2]);
            let t = 0.3 + f64::from(Self::lcg(&mut seed, 40)) / 100.0;
            let from = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            let ang = (b.1 - a.1).atan2(b.0 - a.0) + (if i % 2 == 0 { 1.0 } else { -1.0 }) * (0.9 + f64::from(Self::lcg(&mut seed, 30)) / 100.0);
            let len = (18.0 + f64::from(Self::lcg(&mut seed, 24))) * Self::szf();
            seg(&mut out, from, (from.0 + ang.cos() * len, from.1 + ang.sin() * len));
        }
        out.sort_by_key(|s| s.4);
        out
    }

    /// The web's radius at an action frame: 0 before the first prong, WEB_RADIUS_END at the break, 0 from the blast.
    pub const fn web_radius(frame: u8) -> i32 {
        let (first, brk, blast) = (CombatPatch::SKY_SLASH_FRAMES[0], CombatPatch::SKY_BREAK_FRAME, CombatPatch::SKY_BLAST_FRAME);
        if frame < first || frame >= blast {
            0
        } else if frame >= brk {
            Self::WEB_RADIUS_END
        } else {
            (frame - first) as i32 * Self::WEB_RADIUS_END / (brk - first) as i32
        }
    }

    /// How many of the web's segments show at each action frame (the table the Thumb reads).
    pub fn web_reveal() -> Vec<u8> {
        let segs = Self::web_segments();
        (0..CombatPatch::SKY_FRAMES).map(|f| segs.iter().filter(|s| i32::from(s.4) <= Self::web_radius(f)).count().min(255) as u8).collect()
    }

    /// The shards: `(dir x, dir y — 64ths, start radius px, speed px a draw, (ox, oy) × 3 px)`.
    pub fn shards() -> Vec<(i8, i8, u8, u8, [(i8, i8); 3])> {
        let mut seed = 0x5eed_0602u32;
        (0..Self::SHARDS).map(|i| {
            let th = (i as f64 * 360.0 / Self::SHARDS as f64 + f64::from(Self::lcg(&mut seed, 14) - 7)).to_radians();
            let dir = ((th.cos() * 64.0).round() as i8, (th.sin() * 64.0).round() as i8);
            let r0 = Self::sz(20 + Self::lcg(&mut seed, 30)) as u8;
            let v = Self::sz(5 + Self::lcg(&mut seed, 6)).max(1) as u8;
            let size = (8.0 + f64::from(Self::lcg(&mut seed, 17))) * Self::szf();
            let rot = f64::from(Self::lcg(&mut seed, 360)).to_radians();
            let mut verts = [(0i8, 0i8); 3];
            for (j, v) in verts.iter_mut().enumerate() {
                let a = rot + j as f64 * 2.094_395_1 + f64::from(Self::lcg(&mut seed, 40) - 20) / 100.0;
                let r = size * (0.7 + f64::from(Self::lcg(&mut seed, 50)) / 100.0);
                let stretch = if i % 3 == 0 { 0.45 } else { 0.8 };
                *v = ((a.cos() * r).round() as i8, (a.sin() * r * stretch).round() as i8);
            }
            (dir.0, dir.1, r0, v, verts)
        })
        .collect()
    }

    // ── ★ (5) the cracked floor under the knight (the stills' first frame: a lit disc on the ground, cracks radiating) ──
    /// Two lit ellipses `(radius px at full scale, alpha)`, as 개화's TINT, in FGLOW then FCORE.
    pub const SKY_FLOOR: [(u8, u8); 2] = [(84, 44), (48, 56)];
    /// The floor's scale in 16ths by draws since the action began (then 16) — ★ 2026-10-09 × SIZE (the radii, the
    /// cracks and the burst all ride it).
    pub const SKY_UNFOLD: [u8; 6] = Self::sz_arr([6, 9, 12, 14, 15, 16]);
    /// The floor's cracks: a white rim then a black core (quarter px, tapering outward).
    pub const SKY_CRACK_RIM: (u8, u8) = (7, 1);
    pub const SKY_CRACK_CORE: (u8, u8) = (4, 0);
    /// The cyan burst under the cracks: eight soft rays from the centre, `(half-width quarter px, alpha)`.
    pub const SKY_BURST: (u8, u8) = (22, 160);
    /// The twelve floor cracks, each two segments (a kink half way), `(x1, y1, x2, y2)` in screen px at full scale (the
    /// vertical already × FLOOR_SQUASH) — ground angles k·30° with a jitter, lengths 64..96 px.
    pub fn sky_floor_cracks() -> Vec<(i8, i8, i8, i8)> {
        const JIT: [f64; 12] = [7.0, -9.0, 5.0, -6.0, 8.0, -7.0, 6.0, -8.0, 9.0, -5.0, 7.0, -6.0];
        const LEN: [f64; 12] = [72.0, 92.0, 64.0, 88.0, 76.0, 96.0, 68.0, 90.0, 80.0, 94.0, 66.0, 86.0];
        const KINK: [f64; 12] = [6.0, -8.0, 7.0, -6.0, 9.0, -7.0, 6.0, -9.0, 7.0, -6.0, 8.0, -7.0];
        let mut out = Vec::with_capacity(24);
        for k in 0..12 {
            let th = (f64::from(k as u8) * 30.0 + JIT[k]).to_radians();
            let (c, s) = (th.cos(), th.sin());
            let (mx, my) = (c * LEN[k] * 0.5 - s * KINK[k], s * LEN[k] * 0.5 + c * KINK[k]);
            let (tx, ty) = (c * LEN[k], s * LEN[k]);
            out.push((0, 0, Self::fx(mx), Self::fy(my)));
            out.push((Self::fx(mx), Self::fy(my), Self::fx(tx), Self::fy(ty)));
        }
        out
    }
    /// The burst's eight tips, `(x, y)` in screen px at full scale — 52 px out at k·45° + 22°, laid.
    pub fn sky_floor_burst() -> Vec<(i8, i8)> {
        (0..8).map(|k| {
            let th = (f64::from(k as u8) * 45.0 + 22.0).to_radians();
            (Self::fx(th.cos() * 52.0), Self::fy(th.sin() * 52.0))
        })
        .collect()
    }

    // ── 개화: the floor ──
    /// The floor's centre is the cast tile's (x + 8, y + FLOOR_FEET) — where the rogue's feet were.
    pub const FLOOR_FEET: u8 = 10;
    /// The fade: the level the floor holds while 개화 runs and counts down a draw at a time after (≈ 1.6 s at 20 draws/s);
    /// every alpha is × level / FADE_FULL (a shift — it must stay a power of two).
    pub const FADE_FULL: u8 = 32;
    pub const FADE_SHIFT: u8 = 5;
    /// The bloom: the pattern's scale in 16ths by draws since it appeared (then 16).
    pub const UNFOLD: [u8; 6] = [5, 8, 11, 13, 15, 16];
    /// The breath: the glow's · dots' · light's alpha × PULSE[(age >> 1) & 7] / 16 (and each pillar a step further on).
    pub const PULSE: [u8; 8] = [16, 15, 14, 13, 12, 13, 14, 15];
    /// Laid on the ground: a vertical distance × FLOOR_SQUASH.0 >> FLOOR_SQUASH.1 — ★ (5) 9/16 = 0.5625 (user: *"개화도
    /// 이펙트가 조금 더 누워져야해"*; 11/16 before, 발도's ring's tilt). 천공의검's ground picture lies at the same tilt.
    pub const FLOOR_SQUASH: (u8, u8) = (9, 4);
    /// The lit ground under it: `(radius px at scale 16, alpha)`, an outer and an inner ellipse (bands of 16 per half,
    /// tiling exactly: band k spans `R·k·SQ >> 8` .. `R·(k+1)·SQ >> 8` with half-width `R·CHORD16[k] >> 8`, CHORD16 =
    /// `cos(asin((k + ½)/16))·256` — the band's middle; SQ = FLOOR_SQUASH.0 << 4).
    pub const TINT: [(u8, u8); 2] = [(98, 34), (60, 40)];
    pub const CHORD16: [u8; 16] = [255, 255, 253, 250, 246, 240, 234, 226, 217, 206, 193, 178, 160, 137, 108, 63];
    /// The snowflake's two passes: a soft glow, then a thin hard core (half-widths in quarter px, alpha at full).
    pub const FLOOR_GLOW: (u8, u8) = (18, 150);
    pub const FLOOR_CORE: (u8, u8) = (4, 250);
    /// A soft dot at each spoke's tip.
    pub const FLOOR_DOT: (u8, u8) = (18, 220);
    /// The light rising from the long spokes' hexagons: px up (× the breath), a soft taper (quarter px), alpha.
    pub const PILLAR_H: [u8; 6] = [44, 32, 50, 36, 46, 30];
    pub const PILLAR_W: (u8, u8) = (18, 2);
    pub const PILLAR_ALPHA: u8 = 120;

    /// Where a ground vector `(a, s)` (ahead, side — px) off the centre lands on the screen for `facing` 1..=4 (else as
    /// facing down): the facing's axes, the vertical laid at FLOOR_SQUASH — the spec the Thumb is held to (`>> 4` of the
    /// product, as the code).
    pub const fn ground_to_screen(facing: u8, a: i32, s: i32) -> (i32, i32) {
        let (fx, fy, sx, sy) = CombatPatch::axes(if facing >= 1 && facing <= 4 { facing } else { 2 });
        let x = fx as i32 * a + sx as i32 * s;
        let y = fy as i32 * a + sy as i32 * s;
        (x, (y * Self::FLOOR_SQUASH.0 as i32) >> Self::FLOOR_SQUASH.1)
    }
    /// A point `along` px along prong `k` and `across` px to its side, in ground px: `((A·along − S·across) >> 4,
    /// (S·along + A·across) >> 4)`.
    pub const fn prong_point(k: usize, along: i32, across: i32) -> (i32, i32) {
        let (pa, ps) = (Self::PRONG_A as i32, Self::PRONG_S[k] as i32);
        ((pa * along - ps * across) >> 4, (ps * along + pa * across) >> 4)
    }

    /// The snowflake on the ground at full scale, relative to the floor's centre — `(x1, y1, x2, y2)` in screen px (the
    /// vertical already × FLOOR_SQUASH). Ground angles k·30° (y down): the six odd k are the LONG spokes (12 → 66 px, petal
    /// outlines from 16 to 66, a hexagon of radius 9 at 75), the six even k the SHORT ones (the heart's edge → 50, a barb pair
    /// at 32, a diamond of radius 7 at 57), the heart a hexagon of radius 12 with its corners on the long spokes; a barb
    /// leaves at ±45° outward. 6 + 6 × 11 + 6 × 7 = 114 segments, in that order.
    pub fn floor_segments() -> Vec<(i8, i8, i8, i8)> {
        let mut out = Vec::with_capacity(114);
        let seg = |out: &mut Vec<(i8, i8, i8, i8)>, a: (f64, f64), b: (f64, f64)| out.push((Self::fx(a.0), Self::fy(a.1), Self::fx(b.0), Self::fy(b.1)));
        for k in 0..6 {
            let (a, b) = (Self::ground(2 * k + 1, 12.0), Self::ground((2 * k + 3) % 12, 12.0));
            seg(&mut out, a, b);
        }
        for k in 0..12usize {
            let long = k % 2 == 1;
            let (c, s) = Self::unit(k);
            let at = |r: f64| (c * r, s * r);
            let barb = |r: f64, l: f64, side: f64| {
                let h = core::f64::consts::FRAC_1_SQRT_2;
                let (bx, by) = (c * h - side * s * h, s * h + side * c * h);
                ((c * r, s * r), (c * r + bx * l, s * r + by * l))
            };
            let (start, end) = if long { (12.0, 66.0) } else { (12.0 * 0.866_025_403_784_438_6, 50.0) };
            seg(&mut out, at(start), at(end));
            if long {
                let root = at(16.0);
                let tip = at(66.0);
                for side in [1.0, -1.0] {
                    let wing = (c * 40.0 - s * side * 15.0, s * 40.0 + c * side * 15.0);
                    seg(&mut out, root, wing);
                    seg(&mut out, wing, tip);
                }
            } else {
                for side in [1.0, -1.0] {
                    let (a, b) = barb(32.0, 10.0, side);
                    seg(&mut out, a, b);
                }
            }
            let (centre, radius, corners) = if long { (75.0, 9.0, 6usize) } else { (57.0, 7.0, 4usize) };
            let (nx, ny) = at(centre);
            let step = 12 / corners;
            for j in 0..corners {
                let (c1, s1) = Self::unit((k + j * step) % 12);
                let (c2, s2) = Self::unit((k + (j + 1) * step) % 12);
                seg(&mut out, (nx + c1 * radius, ny + s1 * radius), (nx + c2 * radius, ny + s2 * radius));
            }
        }
        out
    }

    /// A soft dot at each spoke's tip (long 91 px, short 70), k = 0..12 — `(x, y)` in screen px.
    pub fn floor_dots() -> Vec<(i8, i8)> {
        (0..12).map(|k| {
            let r = if k % 2 == 1 { 91.0 } else { 70.0 };
            let (x, y) = Self::ground(k, r);
            (Self::fx(x), Self::fy(y))
        })
        .collect()
    }

    /// Where the light rises: the long spokes' hexagon centres (75 px), k = 1, 3, … 11 — `(x, y)` in screen px.
    pub fn floor_pillars() -> Vec<(i8, i8)> {
        (0..6).map(|i| {
            let (x, y) = Self::ground(2 * i + 1, 75.0);
            (Self::fx(x), Self::fy(y))
        })
        .collect()
    }

    /// The ground point at angle k·30°, `r` px out — exact cosines (no trig: the twelve are 0, ½, √3/2, 1).
    fn ground(k: usize, r: f64) -> (f64, f64) {
        let (c, s) = Self::unit(k);
        (c * r, s * r)
    }

    fn unit(k: usize) -> (f64, f64) {
        const H: f64 = 0.866_025_403_784_438_6;
        const C: [f64; 12] = [1.0, H, 0.5, 0.0, -0.5, -H, -1.0, -H, -0.5, 0.0, 0.5, H];
        (C[k % 12], C[(k + 9) % 12])
    }

    fn fx(x: f64) -> i8 {
        x.round() as i8
    }

    fn fy(y: f64) -> i8 {
        (y * f64::from(Self::FLOOR_SQUASH.0) / f64::from(1u8 << Self::FLOOR_SQUASH.1)).round() as i8
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RoguePatch {
    /// `(definition record, body)` of `bd.a(II)V` — the plain monster's paint.
    pub mob_paint: (u32, u32),
    /// The chained definitions: `ay.b(B)Z` · `ay.a()Z` · `ay.a(I)S` · `a.a(BBII)V` (= `IlpungPatch::{usable, drive,
    /// cooldown}.0` · `AccessoryPatch::draw.0`).
    pub usable: u32,
    pub drive: u32,
    pub cooldown: u32,
    pub draw: u32,
    /// ★ 2026-10-07 개화: the definition of the rogue's kind → sequence map `ay.a(B)B` (chained too — kind 16 draws
    /// sequence [`Self::BLOOM_SEQ`]).
    pub seq_map: u32,
    /// Cells: the attack roll `a.a()I` and the player's hit `a.a(Lbd;BIBB)Z` (both `ay.a(BBI)Z`'s own, virtual) · the sound
    /// `br.a(B)V` (static) · the sprite draw `n.a(BBBII)V` (static — `a.a(BBII)V`'s own).
    pub cell_attack: u32,
    pub cell_hit: u32,
    pub cell_sound: u32,
    pub cell_draw: u32,
    /// The game's helpers: virtual dispatch · static call · the interface call (`Graphics.setAlpha`) · ★ 2026-10-09 (11)
    /// the signed divide (`r0 / r1`, a leaf that keeps r4..r7 — 개화's blow by its level).
    pub dispatch: u32,
    pub call_static: u32,
    pub iface_call: u32,
    pub divide: u32,
    /// Statics: the class `ag.a:B` · the character `ag.a:La;` · the Graphics the sprites draw through (`ax.a`) · the action
    /// lengths `a.n:[B`; and `*(sl + 0x17c)` = the `Graphics.setAlpha(I)V` reference.
    pub class_slot: u32,
    pub char_slot: u32,
    pub graphics_slot: u32,
    pub frames_slot: u32,
    pub set_alpha_ref: u32,
    /// The character (absolute): `a.p:[B` · `a.s:Z` (민첩태세 — `CombatPatch::invuln_off`) · `a.l:B` / `a.m:B` (the blow's
    /// last two arguments, as `a.f(B)Z` passes them) · `g.w` (the action frame) · `a.o:[B` / `a.i:B` (the queued kind).
    pub skills_off: u32,
    pub invuln_off: u32,
    pub weapon_l_off: u32,
    pub weapon_m_off: u32,
    pub frame_off: u32,
    pub queue_off: u32,
    pub queue_at_off: u32,
    /// The monster (absolute): its motion `g.t` · template `bd.a:Lcq;` · hidden `bd.h:Z`; the template's ghost `cq.a:Z`.
    pub mob_state_off: u32,
    pub mob_template_off: u32,
    pub mob_hidden_off: u32,
    pub template_ghost_off: u32,
}

impl RoguePatch {
    pub const CLASS: u8 = 2;
    /// 살상연희 = 번개난타's slot: `a.p[10]` = kind 10 (quickbar value 6), its action = sequence `10 + 4` of `hero/hero2`
    /// (`tools_re/rogue_slaughter.py` redraws it — [`Self::SEQ_FRAMES`] long, which is its `a.n[10]`).
    pub const KIND: u8 = 10;
    pub const SEQ: u8 = 14;
    /// The slash streaks alone, frame for frame with sequence 14 — drawn opaque over the see-through body.
    pub const SLASH_SEQ: u8 = 20;
    /// The cooldown: `ay.a(I)S` index `10 − 5` → 10 s at 20 fps (user: 「쿨타임은 10초」).
    pub const COOLDOWN_INDEX: u8 = 5;
    pub const COOLDOWN: u8 = 200;
    /// ★★ 2026-10-08 (5) — the run restructured (user: *"50x50칸 범위 내의 적을 죽을때까지 때린뒤 다음 적을 찾는거야 … A를 3대
    /// 때리고 A에서 50x50내의 가까운 적 B를 또 찾아서 3대 때리고 … 한마리가 최대 3대만 맞는로직은 없앨꺼야 … 타격하고있는 몬스터
    /// 기준으로 이동할 수 있는 방향으로 순간이동하며 … 8방향을 번갈아가며 … 첫타격의 우선순위는 뒤쪽방향>뒤쪽 대각선방향>옆쪽>앞쪽
    /// … 보스라면 보스를 타격할 수 있는 위치로 계속 이동하며 … 공격자체가 아예 타격이 안되는식으로 빗나가는 케이스는 있어선 안돼"*):
    /// the run keeps ONE target and strikes it until it is down, then the nearest live monster within REACH tiles OF THAT
    /// TARGET (Chebyshev; the first search from the caster), and so on until the level's blows are spent or nobody is
    /// left. No cap per monster. Every blow goes to the target by handle through the player's own hit (`a.a(Lbd;BIBB)Z`
    /// has no range check — the monster's receive rolls its evasion / guard, that is the only «miss» left). Before each
    /// blow the rogue teleports to a LANDING by the target's BODY: every free tile ([`AccessoryPatch`]'s `attr == 0` and
    /// the stage's `bf.a(Lg;II)Z`, the player's own tile always) of the (2·[`Self::LAND_WINDOW`] + 1)² window around the
    /// target's own tile that touches (8-neighbourhood) a grid cell the target stands on — a one-tile monster's eight
    /// neighbours, 오딘's 3×5 body's rim. They are ordered by the target's facing (`g.u`; not 1..=4 → the player → target
    /// axis as its front): `ahead · 16 + |side|`, so its back first, the back diagonals, the sides, the front, the front
    /// diagonals; the blows on one target take them in turn ([`Self::ROT`] mod the count — «번갈아가며»; one tile only →
    /// that tile every time; none → struck from where the rogue stands). The area: tiles each way — 25 = 51×51 (the
    /// user's 50×50 centred on a tile; ★ 2026-10-06 5 = 11×11, 2 = 5×5 before).
    pub const REACH: u8 = 25;
    /// The landing window's half-side around the target's own tile — 오딘's body reaches y ±2, its rim ±3.
    pub const LAND_WINDOW: u8 = 3;
    /// The most landings kept, ordered (a 3×5 body's rim is 20).
    pub const LANDS: u8 = 24;
    /// Per level 1..=5: the blows — the user's 3 and 8, evenly between (asked, 2026-10-06). ★ (5): no cap per monster.
    pub const TOTALS: [u8; 6] = [0, 3, 4, 5, 6, 8];
    pub const MAX_LEVEL: u8 = 5;
    /// A hop's frames: the move · the blow · the follow-through. The sequence = 8 hops and the recovery.
    pub const HOP_FRAMES: u8 = 3;
    pub const SEQ_FRAMES: u8 = 25;
    /// The body while it lasts, and the afterimages: `TRAIL_ALPHAS[draws left]` of 255, gone after `TRAIL_DRAWS` draws.
    pub const SELF_ALPHA: u8 = 140;
    pub const TRAILS: u8 = 3;
    pub const TRAIL_DRAWS: u8 = 6;
    pub const TRAIL_ALPHAS: [u8; 7] = [0, 16, 32, 48, 64, 80, 96];
    /// The sound of a blow that landed (발도's, as 일풍격's).
    pub const SOUND: u8 = 7;
    /// The ghosts a rogue sees: half see-through (user: 「반투명」).
    pub const SIGHT_ALPHA: u8 = 128;
    /// `g.t` while a monster falls and once it is dead.
    pub const MOTION_DYING: u8 = 6;
    pub const MOTION_DEAD: u8 = 7;

    // ── ★★ 2026-10-07 개화 — the rogue's awakening (user: *"개화 : 사용시 캐릭터 현재 위치 기준으로 바닥에 꽃 모양 이펙트(꽃
    //    이펙트는 10칸정도까지)가 생기며 0.5초간 선딜(양손으로 단검을 위로 던지는 듯한 모션)이후 첫 사용 위치 기준 14x14 타일 내의
    //    적에게 순간이동하며 공격(살상연희와 비슷). 살상연희와 비슷하지만 살상연희 보다 데미지가 1.5배 강해야하고 타수가 최대 30타까지
    //    늘어나. 하지만 살상연희와 다르게 1명이 최대 30대를 맞을 수도 있어. 쿨타임 40초"*) ─────────────────────────────────
    //
    // `a.p[16]` = kind 16, quickbar value 8, the eighth ACTIVE cell; LV 70, top level 1; SP = max SP >> 2 (the 일풍격
    // wrapper's), cooldown 40 s. The same plan and drive as 살상연희, with the kind's own numbers ([`Self::BLOOM_REACH`] ★
    // 2026-10-08 (5) 9 = 19×19 around the cast (user: *"개화 범위 캐릭터기준 18x18로"* — an 18×18 has no centre tile, so the
    // smallest centred square that holds one; 7 = 15×15 before) — ★ (5) every search, the first and the ones after a target
    // falls, is from the CAST tile (살상연희's go on from the fallen target); [`Self::BLOOM_TOTAL`]
    // 30 blows, no cap per monster), a WIND-UP of [`Self::BLOOM_WINDUP`] ticks before the first hop (the dagger toss —
    // sequence [`Self::BLOOM_SEQ`]'s first frames) and the blow's attack ×3/2 (the kind handed to the hit stays 10, so
    // 살상연희's multiplier applies — 1.5 × 살상연희 exactly). The action frame LOOPS: `a.n[16]` is sequence 20's 25 (`cf.a()V`),
    // so when `g.w` reaches [`Self::BLOOM_LOOP_AT`] with blows left the drive puts it back to [`Self::BLOOM_LOOP_TO`]. The
    // flower on the ground is ★ 2026-10-08 (3) the host's own — [`SkyDrawPatch`]'s floor piece reads [`Self::FLOWER`]
    // (the cast tile while the run lasts) on the ground draw, under every monster, and fades it out after.
    pub const BLOOM_KIND: u8 = 16;
    pub const BLOOM_VALUE: u8 = 8;
    pub const BLOOM_CD_INDEX: u8 = Self::BLOOM_VALUE - 1;
    pub const BLOOM_COOLDOWN: u16 = 800;
    pub const BLOOM_MAX_LEVEL: u8 = IlpungPatch::AWAKEN_MAX_LEVEL;
    /// ★ 2026-10-09: 9 → 12 = 25×25 (user: *"개화 범위를 25x25로"* — odd, so centred on the cast tile).
    /// ★ 2026-10-16: 12 → 255 = the WHOLE MAP (user: *"개화의 탐색범위를 맵 전체로"*). A map's W and H are one byte each
    /// (`bf.a([B)V` — `tools_re/map_monsters.py`; the largest of the 111 is 80), so every tile of any map is within 255
    /// of the cast tile; it stays a `movs` immediate and `2·reach` (510) leaves the nearest search's box test unsigned-safe.
    pub const BLOOM_REACH: u8 = 255;
    pub const BLOOM_TOTAL: u8 = 30;
    pub const BLOOM_WINDUP: u8 = 10;
    /// The hops start at action frame BLOOM_WINDUP; at frame BLOOM_LOOP_AT (the sequence's last) with blows left the drive
    /// puts `g.w` back to BLOOM_LOOP_TO (phase 2 of a hop — the next tick is a hop; the drawn pose stays a hop pose).
    pub const BLOOM_LOOP_AT: u8 = Self::SEQ_FRAMES - 1;
    pub const BLOOM_LOOP_TO: u8 = Self::BLOOM_WINDUP + 2;
    /// The blow's attack: `a.a()I × BLOOM_ATK_MUL / BLOOM_ATK_DIV` — ★ 2026-10-09 (11) at Lv3; by 개화's level
    /// `a.a()I × 3 · (lv + 2) / (2 · 5)` ([`IlpungPatch::awaken_power`]: Lv1 ×0.9 · Lv3 ×1.5 · Lv5 ×2.1).
    pub const BLOOM_ATK_MUL: u8 = 3;
    pub const BLOOM_ATK_DIV: u8 = 2;
    /// `hero/hero2`: 21 = the body (the map answers 17 for kind 16): 10 toss frames then the hop poses; 22 = the flower on
    /// the ground (its frames = the bloom, then it stays) — ★ 2026-10-08 (3) no longer drawn (the floor piece is), left in
    /// the sheet.
    pub const BLOOM_SEQ: u8 = 21;
    pub const FLOWER_SEQ: u8 = 22;
    pub const FLOWER_FRAMES: u8 = 10;
    /// The block: data · the cooldown · the sequence map · 심안 · the draw · the plan unit · the drive. ★ 2026-10-07 개화:
    /// 0xd00 → 0xe00 and the data 0x100 → 0x140 (the kind running, the ticks, the cast tile, the map's chain word).
    /// ★ 2026-10-08 (5): 0xe00 → 0x1300 — the plan unit grew the nearest search, a landing enumerator (the window scan ·
    /// the ordering) got a piece of its own at LANDS_AT, the drive its re-acquire; the data holds the landings and the run's
    /// single target.
    pub const BLOCK_LEN: u32 = 0x1300;
    /// The plan (the usable wrapper's): how many (0 · 1) and the nearest live monster's handle, its squared distance.
    pub const PLAN_N: u32 = 0x00;
    pub const PLAN_T: u32 = 0x04;
    pub const PLAN_D2: u32 = 0x08;
    /// The run: the kind it is (10 · 16, 0 = none), the level's blows, the blows made, the target struck now (0 = none),
    /// the anchor the next search goes from (살상연희: the last target's tile; 개화: the cast tile), the blows on the
    /// current target (the landing's turn), the last frame acted (−1 after a plan), `a.s` set by this run.
    pub const RUN_KIND: u32 = 0x0c;
    pub const TOTAL: u32 = 0x10;
    pub const HOPS: u32 = 0x14;
    pub const CUR_T: u32 = 0x18;
    pub const ANCHOR_X: u32 = 0x1c;
    pub const ANCHOR_Y: u32 = 0x20;
    pub const ROT: u32 = 0x24;
    pub const LAST: u32 = 0x28;
    pub const OWN_S: u32 = 0x2c;
    /// The player's pose as last drawn (sequence · direction · frame) — the next afterimage's; the ring's next entry.
    pub const LAST_SEQ: u32 = 0x30;
    pub const LAST_DIR: u32 = 0x34;
    pub const LAST_FRAME: u32 = 0x38;
    pub const TRAIL_HEAD: u32 = 0x3c;
    /// The chained words (written by the install): usable · drive · cooldown · draw · ★ 개화 the sequence map.
    pub const CHAIN_USABLE: u32 = 0x40;
    pub const CHAIN_DRIVE: u32 = 0x44;
    pub const CHAIN_COOLDOWN: u32 = 0x48;
    pub const CHAIN_DRAW: u32 = 0x4c;
    pub const CHAIN_SEQ: u32 = 0x50;
    /// ★ 개화: the cast tile (`1 << 16 | y << 8 | x`, 0 = no flower).
    pub const FLOWER: u32 = 0x54;
    /// ★ (5) The landings by the current target, as the last hop enumerated them: how many, then LANDS words of
    /// `score << 16 | y << 8 | x` ordered by score.
    pub const LAND_N: u32 = 0x58;
    pub const LAND: u32 = 0x5c;
    /// ★★ 2026-10-14 신규 아이템 2차 D (살상연희 타수 — 타수는 무기만, user ③-3): host-written from the worn items
    /// ([`Self::gear_words`], 0 = shipped): GEAR_HITS more blows (백 번째 칼날 +5 · 무모한 칼춤 +6) · GEAR_FLAGS
    /// bit 0 = no 무적 (the hops never set `a.s` — 무모한 칼춤's 디메리트 until 2026-10-15; no item sets it now) · GEAR_KILL more blows
    /// each time a blow fells its target, up to GEAR_KILL_MAX a run (무모한 칼춤 +2 / +8 — 피의 연쇄's until 2026-10-15) · GEAR_MUL every
    /// 살상연희 blow's attack × (마지막 춤 5) · GEAR_MAX the run's blows at most (마지막 춤 3, 0 = no cap but TOTAL_CAP — the weapons' and
    /// the kills' blows stop there too); the run's KILL_GOT. The blows
    /// stop at [`Self::TOTAL_CAP`] (= 개화's). More than eight hops loop the action like 개화's ([`Self::LOOP_AT`] → [`Self::LOOP_TO`]).
    /// ★★ 2026-10-17 (user: *"마지막춤 옵션을 살상연희가 3타로(8->3) 줄어들고 데미지 5배 증가로"*): GEAR_LAST (the last blow ×3) became
    /// GEAR_MUL (every blow ×5) and GEAR_MAX joined the words; KILL_GOT moved one word up.
    pub const GEAR_HITS: u32 = 0xc0;
    pub const GEAR_FLAGS: u32 = 0xc4;
    pub const GEAR_KILL: u32 = 0xc8;
    pub const GEAR_KILL_MAX: u32 = 0xcc;
    pub const GEAR_MUL: u32 = 0xd0;
    pub const GEAR_MAX: u32 = 0xd4;
    pub const KILL_GOT: u32 = 0xd8;
    pub const GEAR_NO_INVULN: u32 = 1;
    pub const TOTAL_CAP: u8 = 30;
    /// 살상연희's loop: hop 7's last frame (phase 2) with blows left → hop 0's phase 2 (the next tick is hop 1's move).
    pub const LOOP_AT: u8 = 3 * 8 - 1;
    pub const LOOP_TO: u8 = 2;
    /// The afterimages: `TRAILS` × 16 B = world x · y (px) · pose (`seq | frame << 8 | dir << 16`) · draws left.
    pub const TRAIL: u32 = 0x100;
    pub const DATA_LEN: u32 = 0x140;
    pub const COOLDOWN_AT: u32 = 0x140;
    pub const SEQ_AT: u32 = 0x160;
    pub const SIGHT_AT: u32 = 0x180;
    pub const DRAW_AT: u32 = 0x260;
    pub const PLAN_AT: u32 = 0x4a0;
    pub const LANDS_AT: u32 = 0x900;
    pub const DRIVE_AT: u32 = 0xc00;

    /// ★★ 2026-10-14 the 2nd-gen 살상연희 items: `(type, id, more blows, flags, blows a kill, their cap, the run's blows at most,
    /// every blow's ×)`.
    /// ★★ 2026-10-15 (user: *"피의연쇄 효과를 무모한 칼춤으로 옮기고 무모한 칼춤의 옵션을 극사 옵션으로"*): 무모한 칼춤 carries what was
    /// 피의 연쇄's (+6 · +2 a kill, 8 a run); its +12 and «no 무적» are gone (no item sets [`Self::GEAR_NO_INVULN`] now — the
    /// mechanism stays). 피의 연쇄 is 극사's now ([`AccessoryPatch::CHAIN_BLADE`]).
    /// ★★ 2026-10-17 마지막 춤: 3 blows at most (Lv5's 8 → 3 — the weapons' +blows and the kill blows stop there too) · every
    /// blow's attack ×5 (the old «the last blow ×3» is gone). ⚠ The attack ×5 goes in before the monster's defence, as the old ×3
    /// did — a blow lands a little over 5×.
    pub const GEAR: [(u8, u8, u8, u32, u8, u8, u8, u8); 3] = [
        (3, 35, 5, 0, 0, 0, 0, 0),  // D1 백 번째 칼날 (and 살상연희 피해 +30 % — GearPatch)
        (3, 36, 6, 0, 2, 8, 0, 0),  // D2 무모한 칼춤 (★★ 2026-10-15 피의 연쇄's effect)
        (12, 24, 0, 0, 0, 0, 3, 5), // D8 마지막 춤 (★★ 2026-10-17 3타 · ×5)
    ];

    /// The gear words `[GEAR_HITS, GEAR_FLAGS, GEAR_KILL, GEAR_KILL_MAX, GEAR_MUL, GEAR_MAX]` for the worn `(type, id)`s of a
    /// character of `class` — summed over the slots (부위 합산), the blow's × the largest, the run's cap the smallest; nothing for
    /// another class.
    pub fn gear_words(worn: &[(u8, u8)], class: u8) -> [u32; 6] {
        let mut w = [0u32; 6];
        if class != Self::CLASS {
            return w;
        }
        for &(t, i) in worn {
            for &(gt, gi, hits, flags, kill, cap, max, mul) in Self::GEAR.iter() {
                if (t, i) == (gt, gi) {
                    w[0] += u32::from(hits);
                    w[1] |= flags;
                    w[2] += u32::from(kill);
                    w[3] += u32::from(cap);
                    w[4] = w[4].max(u32::from(mul));
                    if max != 0 {
                        w[5] = if w[5] == 0 { u32::from(max) } else { w[5].min(u32::from(max)) };
                    }
                }
            }
        }
        w
    }
}

// ★★ 야수진화제 판매가 (2026-09-30) — the shop's SELL price per item, instead of the shipped
/// `price / 4`. User: *"야수진화제가 판매가격은 5만원, 적은 7만원인데 이걸 팔떄는 만원, 적은
/// 12000원으로 팔도록"* — 백 · 흑 sell for 10,000 and 적 for 12,000 (they sold for 12,500 and
/// 17,500, which made farming and selling them the early game's money); the shop still asks
/// 50,000 / 70,000. ★ 2026-10-02 the user lowered them again: 백 · 흑 **2,500**, 적 **4,000**.
///
/// ── THE GAME (`tools_re/hero2_dis.py`, HERO2_GUEST_RE.md §8.36) ──────────────────────
/// An item's price is its record's `ch.d:I` (+0xc) — the itm record's `u32 LE price`, and it is
/// SAVED with the record (`ch.a()[B` writes it after the name), so an asset price only reaches
/// records made after it. The sell window `cl` (`v.a(II)Z`: the shop menu's row 1 → `new
/// cl(v)`) prices a sale in exactly two places, both `ch.d >> 2`:
///   * `cl.a(BB)V` 0x142c1e..0x142c28 — its popup answers (예 = kind 2, the quantity box's OK =
///     kind 50) start with `cl.b ← ch.d >> 2`: the unit price the quantity box `aq(this, cl.b,
///     count)` shows and the sale `cl.b()V` pays (`ag.g(I)V(cl.b × cl.c)`, gold = `ag.f:I`);
///   * the item-info painter `j.a(Graphics,II,ch,ZZZ)V` 0x15f986..0x15f99a — with its THIRD
///     flag (the sell window passes 1,1,1; the buy window `p` 1,1,0; the bag and the
///     equipment window 0,0,0) it draws `ch.d >> 2` instead of `ch.d`.
/// The buy side (`p.a(BB)V`) reads `ch.d` whole, three times — untouched.
///
/// ── THE PATCH ─────────────────────────────────────────────────────────────────────────
/// Both sites begin `ldr r0,[pc,#…] ; <r1 = the record>`: the `ch.d` getfield's cell literal,
/// its pool word read by that one `ldr` (a whole-image scan). The pool word becomes the address
/// of a host block and the instruction after the load `bx r0` (the combat hit guard's way). The
/// block reads `ch.g` / `ch.h` / `ch.d` off the record's field block and answers
/// [`Self::PRICES`]' price for its `(type << 8) | id`, `ch.d >> 2` for anything else, then goes
/// on where the shipped code had its result:
///   * SELL (`sb` = the record, `r4` = the window): `r5` = the price → 0x142c2a (`cl.b ← r5`);
///   * PAINT (`r4` = the record, `[sp,#0xc0]` = the sell flag): `fp` = the whole price, or with
///     the flag the sell price → 0x15f99c.
/// A null record goes back the shipped way (the cell literal, the replaced instruction, the
/// getfield) — the game's own NullPointerException, as shipped.
///
/// Nothing is written to a record or a save: the potions already carried sell at the new price
/// the moment the patch is in, and without it the shipped `/ 4` is back.
///
/// ⛔ Install order: the block → the painter's pool word and `bx r0` → the sale's pool word →
/// the sale's `bx r0` LAST. A verify that fails puts every arm back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShopSellPatch {
    /// `(pool word, shipped literal)` of the `ch.d` read in `cl.a(BB)V` (0x142c1e its only reader).
    pub sell_pool: (u32, u32),
    /// `(site, shipped, patched)`: `mov r1,sb` → `bx r0`, right after that load.
    pub sell_arm: (u32, u16, u16),
    /// Where the window goes on: the `cl.b` putfield of `r5`.
    pub sell_resume: u32,
    /// `(pool word, shipped literal)` of the `ch.d` read in the info painter (0x15f986 its only reader).
    pub paint_pool: (u32, u32),
    /// `(site, shipped, patched)`: `adds r1,r4,#0` → `bx r0`, right after that load.
    pub paint_arm: (u32, u16, u16),
    /// Where the painter goes on, `fp` = the price it draws.
    pub paint_resume: u32,
    /// The painter's sell flag (its third boolean argument), off its `sp`.
    pub paint_sell_flag_sp: u16,
    /// Absolute offsets in the record's field block: `ch.g` type · `ch.h` id · `ch.d` price.
    pub type_off: u32,
    pub id_off: u32,
    pub price_off: u32,
}

impl ShopSellPatch {
    /// The block: the sale's lookup, then the painter's.
    pub const BLOCK_LEN: u32 = 0x100;
    pub const SELL_AT: u32 = 0x00;
    pub const PAINT_AT: u32 = 0x80;
    /// The user's sell prices by `(type << 8) | id` — `itm/22` 야수진화제 [0] 백 · [1] 흑 · [2] 적.
    /// ★ 2026-10-02 (user: *"흑,백은 2500원, 적은 4000원으로 수정해줘"*): 10,000 / 12,000 → 2,500 / 4,000.
    /// `itm/20` [0] 제련석: **200** — ★ 2026-10-05 (user: *"상점에서 … 재련석을 500원에 구매할 수있어야하는데 …"* then
    /// *"판매가 200원으로 바꿔"*): the asset now SELLS it in the shop at 500 (its price 1,000 → 500, forshop 0x1400),
    /// which would make the shipped quarter 125 — the table gives 200 for the new 500 records and the old 1,000
    /// ones alike. (2026-10-03 misread 「상점에서 … 500원씩으로 팔아줘」 as the SELL price: 500.)
    pub const PRICES: [(u16, u32); 4] = [(0x1400, 200), (0x1600, 2_500), (0x1601, 2_500), (0x1602, 4_000)];
    /// Gold — `ag.f:I`, the static `ag.g(I)V` adds a sale to (clamped to 0..=999,999,999).
    /// The probe's `[h2-gold]` reads it; the app does not.
    #[allow(dead_code)]
    pub const GOLD_SLOT: u32 = 0x0019_3c0c;

    /// What the sell window charges and draws for a record — the blocks' own rule, for the
    /// tests and the probe: [`Self::PRICES`] by `(type << 8) | id` (the words as the record
    /// holds them, sign-extended), else `price >> 2` (the shipped `asrs`).
    #[allow(dead_code)]
    pub const fn sell_price(kind: i32, id: i32, price: i32) -> i32 {
        let key = (kind << 8) | id;
        let mut i = 0;
        while i < Self::PRICES.len() {
            if Self::PRICES[i].0 as i32 == key {
                return Self::PRICES[i].1 as i32;
            }
            i += 1;
        }
        price >> 2
    }
}

/// ★★ 2026-10-16 일괄판매 (user: *"아이템 판매 창에서 좌측 하단쪽(우측 끝 현재 소지금 표시되는것과 대칭으로 좌측 끝에) 일괄
/// 판매 버튼 … 팝업내에서 일반~ 영웅을 선택해서 일괄판매 … 에픽 이상을 선택하면 높은 가치의 아이템 등급이 선택되었습니다. 정말
/// 판매하시겠습니까? 팝업창이 뜨고 레어까지는 그냥 바로 판매 … 판매 가치만큼 소지금이 제대로 들어오는것 확인"*). HERO2_GUEST_RE.md §8.85.
///
/// ── THE GAME (the shop, state 7 — `ag.g()V` case 7: `ag.h(7)` · `v.a()Lv;` · `v.a()V`) ────────────────────────────
/// The shop menu `v` (singleton `v.a:Lv;`, its origin `v.a:I` / `v.b:I` — `v.a()Lv;` writes them, `v.b()V` hands them to
/// the chain) opens the SELL window `cl` (`new cl(v)` from its row 1): a list of the inventory `ag.a`'s records 1.. (0 is
/// the 재료가방), its `cl.a:Ljava/util/Vector;` (+0x34) the bag's OWN Vector, `cl.a:I` (+0x28) the row count = size − 1.
/// A sale (`cl.b()V` @0x142d2c) = `bq.a(BI)V(h + 1, cl.c)` · `ag.g(I)V(cl.b × cl.c)` · `cl.a()V` (rows ← size − 1, `cl.c`
/// ← 0, cursor clamped) · `new ba(this, shop[21])` 「판매하였습니다.」 · `cl.a:B ← 2` (+0x38; in mode 2 an OK closes the
/// window when nothing is left, `cl.a(II)Z` @0x142ad4, instead of reading a record that is gone). The price is
/// `cl.b ← ch.d >> 2` — [`ShopSellPatch::sell_price`] with the host's table. The painter `cl.a(II)V` @0x142e2c draws the
/// gold at `(x + 113, y + 110)` right-aligned in the 8-row strip under the list (`k.c(Graphics,III)V`).
///
/// ── THE HOST ───────────────────────────────────────────────────────────────────────────────────────────────────
/// A button (the game's font, the panel's own colours) at the strip's LEFT end; its tap opens the game's own list popup
/// (kind 3, rows = `/t/itmtp`[35..38] 일반 · 레어 · 에픽 · 영웅 — the names the info painter writes as `35 + aa.b`) on the
/// sell window, and an OK on a row sells every equipment record of THAT grade that is not worn: 에픽 · 영웅 ask first in
/// the game's 예/아니오 box (kind 10 — its cursor starts on 아니오) with the three `/t/shop` lines the asset appends. The
/// sale pays the sum through the game's `ag.g(I)V`, then takes the records out of the bag in one batch
/// ([`JavaWrite::BagSold`] + [`JavaWrite::SellWindow`]) and says `shop[21]` (or `shop[23]` 「판매할 아이템이 없습니다.」 when
/// the grade has none) in the game's message box.
pub struct BulkSell;

impl BulkSell {
    /// The state word while the shop is up (`ag.h(7)`).
    pub const SHOP_STATE: u32 = 7;
    /// `v.a:Lv;` — the shop menu, the chain's root — and its class descriptor.
    pub const SHOP_SLOT: u32 = 0x0019_453c;
    pub const SHOP_CLASS_DESC: u32 = 0x0019_2d0c;
    /// `v.a:I` / `v.b:I` — the panel origin of every window of the shop.
    pub const SHOP_ORIGIN: (u32, u32) = (0x0019_451c, 0x0019_452c);
    /// `v.a:Li;` — `/t/shop` while the shop is up.
    pub const SHOP_TABLE_SLOT: u32 = 0x0019_450c;
    /// `cl` — the sell window.
    pub const SELL_CLASS_DESC: u32 = 0x0019_2b38;
    /// `cl`'s own fields, absolute (`j` is 0x28): `a:I` rows · `c:I` the count being sold · `a:Ljava/util/Vector;` the
    /// bag's Vector · `a:B` the mode (0 browse · 1 sell now · 2 told).
    pub const ROWS_OFF: u32 = 0x28;
    pub const COUNT_OFF: u32 = 0x30;
    pub const VECTOR_OFF: u32 = 0x34;
    pub const MODE_OFF: u32 = 0x38;
    pub const MODE_TOLD: u32 = 2;
    /// `/t/shop` rows: 「판매하였습니다.」 — and the four the asset appends after the 26 shipped ones (`make_hero2_asset.py`
    /// SHOP_APPEND): the confirm box's three lines, and 「판매할 아이템이;없습니다.」 — the game's own row 23 in two lines, because
    /// the message box `ba` wraps at 101 px and row 23 (103 px) leaves its last syllable alone on a second line.
    pub const SHOP_ROW_SOLD: u32 = 21;
    #[allow(dead_code)] // the asset tool's SHOP_SHIPPED_ROWS — the selftest pins the appended rows right after it
    pub const SHOP_ROWS_SHIPPED: u32 = 26;
    pub const SHOP_ROWS_ASK: [u32; 3] = [26, 27, 28];
    pub const SHOP_ROW_NONE: u32 = 29;
    /// The game's own 「판매할 아이템이 없습니다.」 — what an asset without row 29 says instead.
    pub const SHOP_ROW_NONE_SHIPPED: u32 = 23;
    /// `/t/itmtp` row of grade 0 (일반 · 레어 · 에픽 · 영웅 = 35..38).
    pub const GRADE_ROW0: u32 = 35;
    pub const GRADES: u8 = 4;
    /// From this grade on the sale asks first (에픽 · 영웅).
    pub const ASK_FROM: u8 = 2;
    /// `aa.b:B` — an equipment record's grade (body[2]), absolute (`ch` 0x20 + `aa`'s own +0x4).
    pub const GRADE_OFF: u32 = 0x24;
    /// The popup kinds the host opens with `j.a(BI[[B)V`: the LIST family (3 — the inventory's own 상세보기 · 버리기 kind;
    /// `d.a(II)Z` walks it with UP/DOWN) and the 예/아니오 family's 10 (starts on 아니오). Neither is a kind `cl.a(BB)V`
    /// acts on (it acts on 2 — sell this record — and 50, the 개수 box's OK).
    pub const LIST_KIND: u8 = 3;
    pub const ASK_KIND: u8 = 10;
    pub const ASK_BUTTONS: i32 = 2;
    /// The button, panel-relative `(x, y, w, h)` — ★ 2026-10-16 (2) (user: *"일괄판매 버튼이 창을 벗어나고있어서 차라리 좌측에서
    /// 좀 떨어뜨린채로(판매창에서 맨아래 아이템과 클릭 최대한 겹치지 않게) 폰트를 줄이고 버튼크기를 줄여서 판매 메뉴 창안을 벗어나지
    /// 않게해줘"*): exactly the 8-row strip under the list (y + 109..y + 116 — the panel's black edge is y + 117, measured),
    /// 8 px right of the list column (x + 3..x + 18, whose last cell sits right above the strip) and clear of the gold
    /// (right-aligned at x + 113; at 999,999,999 its coin reaches back to ≈ x + 69). The first cut `(3, 109, 43, 13)` sat
    /// under that last cell and hung four rows below the panel — the game's Hangul is ten rows.
    pub const BUTTON: (i32, i32, u32, u32) = (26, 109, 36, 8);
    /// 「일괄판매」 as a host 7-row bitmap (`#` = ink), drawn at `(+2, +1)`: the game's font is ten rows and the strip has
    /// eight. Its bottom row is the button's, open on the panel's black edge (the frame is the top and the sides).
    pub const LABEL: [&'static str; 7] = [
        ".##..#.###..#..#####.#..####.#.#",
        "#..#.#...#..##..#.#..##.#..#.#.#",
        ".##..#.####.#...#.#..#..#..#.###",
        "#####..#####...#####.#..#..#.#.#",
        "....#......#...#........####.#.#",
        "####...####....#.............#.#",
        "#####..#####...#####.........#.#",
    ];
}

/// ★★ 2026-10-16 정렬 (user: *"인벤토리에서 소지금 우측에 작은 새로고침같이 생긴 버튼을 두고 그 버튼을 누르면 가방 내 아이템이
/// 정렬됐으면 좋겠어. 정렬순서는 hero3에서 가방 내 정렬버튼의 정렬순서랑 동일해"*). hero3's `bagsort.rs` (136차): 부위 → 요구레벨 ↓ →
/// 등급 ↓ → idx → 제련도 ↓ → 개수 ↓ → 인스턴스 키 → 원래 자리; what is not equipment comes after it, quest items first, then the
/// 조합서, then the rest by type; the bag-item cell (here the 재료가방, type 29) never moves. hero3 does not pull worn gear
/// forward — ★ hero2 does (2026-10-16, user: *"가방정리 시 현재 착용하고 있는 아이템은 가장 앞으로 정렬되고 그 뒤에 현재
/// 정렬조건대로 다른 아이템들이 전부 정렬"*): right after the 재료가방, in that same order among itself
/// (`hero2java::h2_bag_sort_order`). HERO2_GUEST_RE.md §8.85.
pub struct BagSort;

impl BagSort {
    /// `aa`'s own fields, absolute (`ch` is 0x20): `a:B` 요구 레벨 (body[1]) · `b:B` 등급 (body[2]) · `d:B` 제련도 — the parser
    /// `aa.a([BIZZ)I` @0x10d760 writes `a` · `b` · `c` from body[1..=3] in that order, and the smithy `ck.d()V` adds to `d`.
    pub const LEVEL_OFF: u32 = 0x20;
    pub const GRADE_OFF: u32 = 0x24;
    pub const REFINE_OFF: u32 = 0x2c;
    /// The button, panel-relative `(x, y, w, h)`: in the inventory's 소지금 bar (black, `y + 14..y + 27`), right of the
    /// 「소지금」 label (`x + 11`, 28 px) and clear of the gold, which is right-aligned at `x + 111` and reaches back to
    /// `x + 63` at 999,999,999.
    pub const BUTTON: (i32, i32, u32, u32) = (41, 15, 12, 12);
    pub const QUEST_TYPE: i8 = 33;
    pub const RECIPE_TYPE: i8 = 23;

    /// hero3's 부위 order on hero2's types (`/t/itmtp` 0..12): the weapons (양손대검 0 · 양손도끼 1 · 도검 2 · 단검 3 · 둔기 4),
    /// then 방패 10 · 투구 5 · 갑옷 6 · 장갑 8 · 신발 9 · 목걸이 11 · 반지 12. `None` = not equipment.
    pub const fn part_rank(kind: i8) -> Option<u8> {
        Some(match kind {
            0..=4 => kind as u8,
            10 => 5,
            5 => 6,
            6 => 7,
            8 => 8,
            9 => 9,
            11 => 10,
            12 => 11,
            _ => return None,
        })
    }

    /// The coarse group: equipment, quest items, 조합서, the rest.
    pub const fn group(kind: i8) -> u8 {
        if Self::part_rank(kind).is_some() {
            0
        } else if kind == Self::QUEST_TYPE {
            1
        } else if kind == Self::RECIPE_TYPE {
            2
        } else {
            3
        }
    }
}

/// ★ 2026-10-13 등급 줄 — a long item name no longer runs into its grade. User: *"이름이 긴 아이템들은 등급과
/// 이름이 겹쳐져서 보이는 경우가 있는데 겹쳐지면 등급을 두번째줄 우측끝으로 정렬할꺼야 … 그림자 사냥꾼의 두건은
/// 두건이라는 글자와 영웅(아이템 등급) 글자가 겹쳐지는데 … 바로 밑에 비어있는공간인 두번쨰줄의 우측끝에"*.
///
/// ── THE GAME ─────────────────────────────────────────────────────────────────────────
/// The info painter `j.a(Graphics,II,ch,ZZZ)V` @0x15ec48 (상세보기 · the shop panels — the bag's name strip
/// leaves before this) draws an equipment record's name at `(x, y)` and its grade (`/t/itmtp`[35 + `aa.b`]:
/// 일반 · 레어 · 에픽 · 영웅) at `(x + 73, y)` on the SAME line (0x15edb6..0x15edf6), whatever the name's
/// width — and then steps the line TWICE before 「LV」 (0x15edfa · 0x15ee16): the line under the name is
/// empty, except for a set item, whose set name (`/t/itmtp`[58 + `aa.c`], 바람S … 팬텀S) the painter puts
/// there right-aligned at `x + 93` (0x15f596..0x15f602, anchor 8). The game's width rule `ac.a([B)I`
/// @0x10ebf4 is `1 + 6 per ASCII + 9 per syllable`: the guardian set's seven 85-level names (79 · 「그림자
/// 사냥꾼의 두건」 94) cross `x + 73`; the widest of the rest is 70.
///
/// ── THE PATCH ────────────────────────────────────────────────────────────────────────
/// The grade draw's cell load (0x15ede8, its pool word's only reader) → a block that, when the name is wider
/// than [`Self::NAME_MAX`] and the record carries no set, moves the grade one line down (`+ [sp,#0x60]`, the
/// painter's own line height) and right-aligns it to the set name's right end (`x + 93 − width`); otherwise
/// nothing changes. Then the replaced `movs r5,#1` and the shipped literal, back at 0x15edec. Nothing is
/// written to a record or a save.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GradeLinePatch {
    /// `(pool word, shipped literal)` of `ac.a(Graphics,II[BI)V`'s cell for the grade (0x15ede8 its only reader).
    pub pool: (u32, u32),
    /// `(site, shipped, patched)`: `movs r5,#1` → `bx r0`, right after that load.
    pub arm: (u32, u16, u16),
    /// The painter's frame: the record (its fourth argument), `x`, the line height.
    pub record_sp: u16,
    pub x_sp: u16,
    pub line_sp: u16,
    /// Absolute offsets in the record's field block: `ch.j` the name · `aa.c` the set (`ch` 0x20 + 0x8).
    pub name_off: u32,
    pub set_off: u32,
}

impl GradeLinePatch {
    pub const BLOCK_LEN: u32 = 0x80;
    /// The grade's shipped x — `x + 73` (`adds r2,#0x49` at 0x15edba).
    pub const GRADE_X: i32 = 0x49;
    /// The second line's right end — where the painter right-aligns a set name (`[sp,#0x7c] + 0x5d`, 0x15f5be).
    pub const RIGHT: i32 = 0x5d;
    /// A name wider than this reaches the grade: 70 (the widest shipped name) leaves 3 px before `x + 73`.
    pub const NAME_MAX: u8 = 71;
    /// `aa.c` 1..=5 = a set (바람S · 테러S · 사자S · 베르S · 팬텀S) — the second line's right end is its.
    pub const SETS: u8 = 5;

    /// `ac.a([B)I`: 1 + 6 per ASCII byte + 9 per EUC-KR syllable (the block's loop, for the tests).
    #[allow(dead_code)]
    pub fn text_width(bytes: &[u8]) -> i32 {
        let (mut w, mut i) = (1, 0);
        while i < bytes.len() {
            if bytes[i] >= 0x80 {
                w += 9;
                i += 2;
            } else {
                w += 6;
                i += 1;
            }
        }
        w
    }

    /// Where the grade goes, `(dx, dy)` from the name: the shipped `(73, 0)`, or under a long name of a record
    /// without a set `(93 − its width, line)`.
    #[allow(dead_code)]
    pub fn grade_at(name: &[u8], grade: &[u8], set: i32, line: i32) -> (i32, i32) {
        if (1..=i32::from(Self::SETS)).contains(&set) || Self::text_width(name) <= i32::from(Self::NAME_MAX) {
            (Self::GRADE_X, 0)
        } else {
            (Self::RIGHT - Self::text_width(grade), line)
        }
    }
}

/// ★★ 보스 치명 (2026-10-10) — a crit (or a 투지 hit) on a boss is never weaker than on any other
/// monster. User: *"버서커 무기에 투지의 구 발랐을떄 효과 발동시 보스몹한테 데미지 0뜨는 버그"* →
/// of the two fixes offered, *"2로 가자"*: stop the 0, keep the shipped boss multiplier otherwise.
///
/// ── THE GAME (HERO2_GUEST_RE.md §8.63) ───────────────────────────────────────────────────
/// Both hit routines multiply a hit that crits (the roll, `fp`) or lands while 투지 is up (the
/// character's `a.h:Z`, 6 s after the 투지 orb's proc) — the ordinary monster's
/// `bd.a(La;BIZBBBB)Z` at 0x1278e8 by the weapon's crit multiplier, `d · a.k:B / 10` (`a.k` = the
/// weapon record's body[32], 13..26), the boss's `ah.a(La;BIZBBBB)Z` (오딘 · 고대수호물 heads `al`,
/// the hands `m`, `ca`, `ci`) at 0x1161ba by `(a.l:I / 10) · d` — `a.l:I` is the crit CHANCE in %,
/// the division first. Under 10 % that is 0 (a berserker without agility, `8 + 민첩'/5`), and the
/// boss routine never lifts a 0 to the ordinary monster's minimum 3; 60..90 % (the rogue) is ×6..9.
///
/// ── THE PATCH ─────────────────────────────────────────────────────────────────────────
/// The boss's multiplier becomes `max(10 · ⌊a.l / 10⌋, a.k)` tenths ([`Self::boss_crit`]): the
/// shipped product exactly whenever it is at least the weapon's (the rogue's ×6..9 stays), the
/// ordinary monster's `d · a.k / 10` below it — never 0. Same machinery as [`ShopSellPatch`]:
/// 0x1161ba `ldr r0,[pc,#0x1b0]` is the only reader of its pool word 0x11636c (a whole-image scan)
/// → that word = the block, the instruction after the load (`mov r1,r8`) = `bx r0`. The block reads
/// `a.l:I` and `a.k:B` off the attacker's field block (`r8`, the character), the hit off
/// `[sp,#0x28]`, divides with the game's own helper (the one both routines call), stores the product
/// at `[sp,#0x28]` and goes on at 0x1161d2 (`ldr r3,[sp,#0x28]`), where the shipped code had stored
/// its own. A null attacker goes back the shipped way (the cell literal, `mov r1,r8`, the getfield).
/// The registers it uses (r0..r6) are all written again before the routine reads them; r7 (the boss),
/// r8, sb, sl, fp and sp are untouched. Nothing is written to a record or a save.
///
/// ⛔ Install order: the block → the pool word → the `bx r0` LAST; a verify that fails puts both back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BossCritPatch {
    /// `(pool word, shipped literal)` of the attacker's `a.l:I` read at 0x1161ba (cell 0x176b58).
    pub pool: (u32, u32),
    /// `(site, shipped, patched)`: `mov r1,r8` → `bx r0`, right after that load.
    pub arm: (u32, u16, u16),
    /// Where the routine goes on: `ldr r3,[sp,#0x28]`, the hit just stored.
    pub resume: u32,
    /// The hit on the routine's stack (`[sp,#0x28]` — its fifth argument, rewritten in place).
    pub damage_sp: u16,
    /// The game's signed divider `r0 = r0 / r1` (a leaf that keeps r4..r7) — the one both routines call.
    pub divide: u32,
    /// The character's ABSOLUTE field offsets: `a.l:I` crit % (0x64 declared + the 0x5c parent) ·
    /// `a.k:B` the weapon's crit multiplier in tenths (0x68 + 0x5c; `a.m()V` copies it off `bo.j`).
    pub crit_off: u32,
    pub crit_mul_off: u32,
}

impl BossCritPatch {
    pub const BLOCK_LEN: u32 = 0x60;

    /// The patched boss crit, for the tests and the probe: `d · max(10 · ⌊crit / 10⌋, mul) / 10`,
    /// Java's truncating division as the game's helper does it — `⌊crit / 10⌋ · d` (the shipped
    /// product) whenever `10 · ⌊crit / 10⌋ ≥ mul`, the ordinary monster's `d · mul / 10` below it.
    #[allow(dead_code)]
    pub const fn boss_crit(d: i32, crit: i32, mul: i32) -> i32 {
        let shipped = crit / 10 * 10;
        let m = if shipped >= mul { shipped } else { mul };
        d.wrapping_mul(m) / 10
    }

    /// The shipped boss crit — `⌊crit / 10⌋ · d` — for the tests' comparisons.
    #[allow(dead_code)]
    pub const fn shipped(d: i32, crit: i32) -> i32 {
        (crit / 10).wrapping_mul(d)
    }
}

/// ★★ 보스 방어 관통 (2026-10-21) — a boss's hit carries a share that defence does not stop. User:
/// *"전체 보스들의 데미지중 일부를 방어무시 데미지로 만들어줘. 데미지중 20%를 방어무시데미지로 전환하고싶어."*
///
/// ── THE GAME ──────────────────────────────────────────────────────────────────────────
/// Every monster hit on the character is the hit routine `a.a(Lbd;SB)V` (callers `bd.d()V` · `al.w` · `al.d(II)V` ·
/// `at.b()V` (a projectile, its owner as the attacker) · `ca.d` · `m.d`; `sb` = the attacker): `R = atk ± atk/10`
/// (0x151cb0..0x151ce0, in `r5`), then `R − a.i:I` (the character's defence, absolute 0xa4) at 0x151cec — negative is 0
/// (0x151fb6), the knight's shield block takes the shield's own defence `bu.a()I` off once more (0x152002..0x15200e), then
/// the monster's critical ×2.5 · 얼음장막 · ×½ · 철괴 and the gear's cuts (the hook at 0x151d6a) · the minimum 3. A boss's hit
/// against a defence above it was 3 — the raid's 12 % attack made that the rule for a full build.
///
/// ── THE PATCH ─────────────────────────────────────────────────────────────────────────
/// The hit SPLITS: [`Self::PIERCE_PCT`] % of `R` passes defence, the rest meets it — `max(R·(1 − p) − DEF, 0) + R·p` =
/// `max(R − DEF, ⌊R·p/100⌋)`: unchanged while the defence is below 80 % of the hit, never less than its true part above
/// it. Two arms of one block, both [`BossCritPatch`]'s shape (the pool word of a load → the block, the instruction after
/// it → `bx r0`):
/// * **DEF** — 0x151ce2 `ldr r0,[pc,#0x210]` is the only reader of pool word 0x151ef4 (the `a.i:I` cell 0x17ee0c); the
///   block reads `a.i` off the character (`r4`) itself and, when the attacker is a boss (`instanceof ah` — its class
///   descriptor one of [`Self::boss_descs`]), hands back `min(DEF, R − ⌊R·p/100⌋)` (the hit becomes the formula above) and
///   leaves the true part in [`Self::TRUE_AT`] (0 for anything else, every hit); on at 0x151cec `subs r6,r5,r0`.
/// * **shield** — 0x152002 `ldr r0,[pc,#0x94]` (pool word 0x152098, the `bu.a()I` cell 0x17ee24): the block makes the
///   shipped virtual call itself, then the shield may take at most `dmg − true part` — a block stops the defended part,
///   not the part that ignores defence; on at 0x15200c `mov r5,r8`.
///
/// What stays: the dodge (before), the critical (on the whole hit), 얼음장막's 3, ×½, 철괴 and every gear cut (after —
/// they are damage cuts, not defence), the minimum 3. `m`'s HP drain (state 13, `HP/20`) never met defence. Every boss
/// everywhere — story, guardian, raid — and only bosses: a plain `bd`'s hit is the shipped one. [`Self::PCT_AT`] is the
/// host's word (0 = the shipped hit, bit for bit). Nothing is written to a record or a save.
///
/// ⛔ Install order: the block (the words, then the code) → each site's pool word → its `bx r0` LAST; a verify that fails
/// puts every arm back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BossPiercePatch {
    /// `(pool word, shipped literal)` of the character's `a.i:I` read at 0x151ce2.
    pub def_pool: (u32, u32),
    /// `(site, shipped, patched)`: `adds r1,r4,#0` → `bx r0`, right after that load.
    pub def_arm: (u32, u16, u16),
    /// `subs r6,r5,r0` — the hit less the defence the block hands back.
    pub def_resume: u32,
    /// `a.i:I`, the character's defence — ABSOLUTE (0x48 declared + the 0x5c parent).
    pub def_off: u32,
    /// `(pool word, shipped literal)` of the shield's `bu.a()I` call at 0x152002.
    pub shield_pool: (u32, u32),
    /// `(site, shipped, patched)`: `adds r1,r6,#0` → `bx r0`.
    pub shield_arm: (u32, u16, u16),
    /// `mov r5,r8` — the hit less the shield's defence.
    pub shield_resume: u32,
    /// The `bu.a()I` cell (`sl` + the shipped literal) the block calls through.
    pub shield_cell: u32,
    /// The game's virtual dispatch helper (`r0` = the cell, `r1` = this) and its signed divider `r0 = r0 / r1`.
    pub dispatch: u32,
    pub divide: u32,
    /// `ah`'s four concrete classes — what the game calls a boss.
    pub boss_descs: [u32; 4],
}

impl BossPiercePatch {
    pub const BLOCK_LEN: u32 = 0xc0;
    /// The share of a boss's hit that passes defence, in %. The host's word; 0 = the shipped hit.
    pub const PCT_AT: u32 = 0;
    /// The true part of the hit in progress (0 unless a boss's) — the DEF arm writes it, the shield arm reads it.
    pub const TRUE_AT: u32 = 4;
    /// The code, past the two words.
    pub const CODE_AT: u32 = 0x10;
    /// User: 20 %.
    pub const PIERCE_PCT: u32 = 20;
    pub const PCT_MAX: u32 = 100;

    /// The part of a boss's hit `r` that passes defence: `⌊r · pct / 100⌋` (0 for a hit of 0 or less).
    #[allow(dead_code)]
    pub const fn true_part(r: i32, pct: i32) -> i32 {
        if r <= 0 || pct <= 0 { 0 } else { r.wrapping_mul(pct) / 100 }
    }

    /// The defence a boss's hit `r` meets: `min(def, r − true part)` — so the hit is `max(r − def, true part)`.
    #[allow(dead_code)]
    pub const fn defence(r: i32, def: i32, pct: i32) -> i32 {
        let t = Self::true_part(r, pct);
        if t <= 0 {
            return def;
        }
        let cap = r - t;
        if def > cap { cap } else { def }
    }

    /// What the knight's shield block takes off a boss's hit `dmg` whose true part is `t`: `min(shield, max(dmg − t, 0))`.
    #[allow(dead_code)]
    pub const fn shield(dmg: i32, shield: i32, t: i32) -> i32 {
        if t <= 0 {
            return shield;
        }
        let cap = if dmg - t < 0 { 0 } else { dmg - t };
        if shield > cap { cap } else { shield }
    }
}

/// ★★ HELL 보너스 드랍 (2026-10-02) — HELL rolls two HELL-only slots of the enemy's drop
/// table once more, after the shipped death routine. User: *"헬 장비드랍이 하드코어보다 조금
/// 더 좋아야해."* + *"hell모드에서 하드코어보다 장신구도 잘 나와야해."*
///
/// ── THE GAME (HERO2_GUEST_RE.md §8.9 「HARDCORE 와 HELL」) ─────────────────────────────
/// `bd.q()V` @0x128080 rolls ONE table drop, `h.a(1, 0x7fff)` against the slot window
/// `ag.i > 0 ? 2·3·4 : 0·1·2` — HARDCORE and HELL take one branch (0x12822a): the same slots,
/// the same odds (probe, 2,000 kills each: 13.8 % / 13.5 % gear). Only the monsters differ —
/// `cq.a()V` gives HELL +20 levels and ≈1.9× HP and attack — so HELL dropped HARDCORE's gear:
/// gear at the HELL monster's level − 10 or above was 0.55 % a kill, HARDCORE's 8.29 %.
///
/// ── THE PATCH ─────────────────────────────────────────────────────────────────────────
/// The asset gives every drop-gated enemy two more slots (`make_hero2_asset.py` HELL 칸, the
/// record's slot count 5 → 7): slot 5 HELL-level gear, slot 6 a 영웅 necklace or ring, their
/// cumulatives on their OWN axis from 0. The shipped routine reads slots 0..4 only (the window
/// above), so NORMAL and HARDCORE roll exactly what they rolled. Word 0 of `bd.q()V`'s
/// definition record (`0x185644 + 369 × 0x1c` = **0x187ea0**; its two callers, `bd.c()V` and
/// `m.c()V`, dispatch virtually and read it at call time) → a wrapper that CALLS the body as
/// shipped, then — HELL only (`ag.i > 1`, the shipped HELL potion's own test) — rolls
/// `h.a(1, 0x7fff)` once more: `roll < c5` → slot 5, `c5 < roll < c6` → slot 6 (both bounds
/// strict, as the game's), the id `h.a(lo, hi)`, onto the ground at the monster's tile through
/// the game's own `stage.bf.a(BBBB)V(x, y, type, id)` — the shipped table drop's very call.
/// So HELL = everything HARDCORE drops + one bonus roll.
///
/// A template with no drop gate (`cq.f == 0`: the bosses, the shipped routine's own skip) or
/// whose slot bytes are shorter than [`Self::MIN_SLOT_BYTES`] (an asset made before the two
/// slots) gets nothing: the guard is in the block, so the code never reads past an old table
/// and an old asset is simply the shipped game. Nothing is written to a record or a save.
///
/// ★★★ 2026-10-02 저녁 — 드랍 티어 (user: *"종결아이템급은 드랍률을 좀 낮추고 … 헬모드에서는 1%확률로"* ·
/// *"신규 6종 장신구는 … 0.1%수준"*, the new six dropping in HELL + 난이도 HARD only). The asset now
/// gives FOUR bonus slots (n 5 → 9: 5 upper gear · 6 종결 accessory · 7 종결 gear · 8 the new six), so
/// the roll walks every slot from 5 to the array's end on the one axis — `c[k−1] < roll < c[k]`,
/// both bounds strict ([`Self::pick_slot`]) — and a seven-slot asset rolls exactly as before. A slot
/// whose type byte carries [`Self::GATE`] drops (as `type − GATE`) only while the block's gate word
/// is non-zero: the host writes 1 there while the 환경설정 난이도 is NORMAL or above ([`Self::gate_open`];
/// HARD only until 2026-10-03), 0 otherwise — so that slot's share of the roll is "nothing" at EASY,
/// the default. The shipped routine
/// never reads past slot 4, and an older block (two slots, `type > 34` → nothing) skips a gated slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HellDropPatch {
    /// `(def, body)` of `bd.q()V` — the monster's death (EXP, the table drop, gold, potions).
    pub death: (u32, u32),
    /// Absolute cells (sl + literal) the shipped routine itself calls through: `h.a(II)I` (the
    /// game's random range — static, `min + |nextInt| % (max − min + 1)`) and `bf.a(BBBB)V`
    /// (the stage puts an item on the ground — virtual).
    pub cell_random: u32,
    pub cell_put: u32,
    /// The game's static-call and virtual-call helpers (r0 = cell, then the arguments).
    pub call_static: u32,
    pub dispatch: u32,
    /// Static slots: `ag.i:B` the mode (0 NORMAL · 1 HARDCORE · 2 HELL) · `ag.a:Lbf;` the stage.
    pub mode_slot: u32,
    pub stage_slot: u32,
    /// Absolute field offsets — the monster: its template `bd.a:Lcq;` and its tile `ad.d:B` /
    /// `ad.e:B`; the template: the drop gate `cq.f:S` and the slot bytes `cq.b:[B`.
    pub template_off: u32,
    pub tile_x_off: u32,
    pub tile_y_off: u32,
    pub gate_off: u32,
    pub slots_off: u32,
}

impl HellDropPatch {
    /// The wrapper, its gate word and literal pool (0xc0 until the four-slot walk — 2026-10-02 저녁; 0x100 until the
    /// guardian pass — 2026-10-08).
    pub const BLOCK_LEN: u32 = 0x140;
    /// `ag.i` above this is HELL (the shipped HELL potion's `cmp r0,#1 ; bgt`).
    pub const LAST_NOT_HELL: u8 = 1;
    /// The bonus slots: 5 (gear) and 6 (accessory), five bytes each from byte 25 —
    /// `(type, id lo, id hi, u16 LE cumulative)`, the cumulatives from 0.
    pub const FIRST_SLOT: u8 = 5;
    pub const SLOT_BYTES: u8 = 5;
    /// Seven slots' bytes: a record with fewer has no HELL slots.
    pub const MIN_SLOT_BYTES: u8 = 35;
    /// `h.a(1, 0x7fff)` — the shipped table roll's range.
    pub const ROLL_MAX: u32 = 0x7fff;
    /// Slot type 7 is "nothing" (the shipped routine's own skip; `itm/07` does not exist), and
    /// no item type is above 34 (스펠카드).
    pub const TYPE_NONE: u8 = 7;
    pub const TYPE_MAX: u8 = 34;
    /// ★ 2026-10-02 저녁: a bonus slot's type byte with this bit drops only while the gate is open.
    pub const GATE: u8 = 0x80;
    /// The 환경설정 난이도 (`crate::difficulty()`, an index of `DIFFICULTY_PCT`) that opens the gate — this
    /// one and above. ★ 2026-10-03 (user: *"신규 장신구를 헬 노말난이도부터 나오게 수정하자"*): NORMAL (1);
    /// it was HARD (2) from the 드랍 티어 of 2026-10-02 저녁 until then. EASY, the default, never rolls the slot.
    pub const GATE_DIFFICULTY: u8 = 1;

    /// The gate word the host keeps in the block for the 난이도 `difficulty`.
    #[allow(dead_code)]
    pub const fn gate_open(difficulty: u8) -> bool {
        difficulty >= Self::GATE_DIFFICULTY
    }

    /// What the bonus roll picks for `roll` (1..=0x7fff) between the two cumulatives: slot 5,
    /// slot 6, or nothing — the block's own rule, for the tests and the tools.
    #[allow(dead_code)]
    pub const fn pick(roll: u32, c5: u32, c6: u32) -> Option<u8> {
        if roll < c5 {
            Some(5)
        } else if roll > c5 && roll < c6 {
            Some(6)
        } else {
            None
        }
    }

    /// The same rule over any number of bonus slots: `cums[i]` is slot `5 + i`'s cumulative; the slot
    /// whose interval holds `roll` strictly (`cums[i−1] < roll < cums[i]`, `cums[−1]` = 0), else none.
    #[allow(dead_code)]
    pub const fn pick_slot(roll: u32, cums: &[u32]) -> Option<u8> {
        let mut prev = 0;
        let mut i = 0;
        while i < cums.len() {
            if roll < cums[i] {
                return if roll > prev { Some(Self::FIRST_SLOT + i as u8) } else { None };
            }
            prev = cums[i];
            i += 1;
        }
        None
    }

    /// ★ 2026-10-08 가디언: the HELL pass of the bonus roll ends at this slot (the asset's 5·6·7·8) — the
    /// slots after it are the GUARDIAN pass's ([`GuardianPatch`]), rolled once more only while `ag.i ≥ 3`.
    /// Before, the one pass walked to the array's end, so a guardian slot appended to the table would have
    /// rolled in HELL too.
    /// ★ 2026-10-09 (user: 가디언 조합서 「HELL + 가디언 드랍」): 8 → **9** — the asset's slot 9 is the guardian recipes
    /// (`make_hero2_asset.py` HELL_SLOTS), and the guardian pass is 10..14. The block and the asset ship together; an
    /// asset of the 10-08 layout under this block would roll its guardian gear slot 9 in HELL.
    pub const LAST_HELL_SLOT: u8 = 9;
    /// `ag.i` from which the guardian pass rolls.
    pub const GUARDIAN_MODE: u8 = 3;

    /// [`Self::pick_slot`] over one pass of the table: HELL (`guardian == false`) sees the bonus slots up to
    /// [`Self::LAST_HELL_SLOT`] on their own axis; the guardian pass sees the slots after it, on THEIR own axis
    /// (`cums` = every bonus cumulative from slot 5 on). `None` when the pass has no slot.
    #[allow(dead_code)]
    pub fn pick_pass(roll: u32, cums: &[u32], guardian: bool) -> Option<u8> {
        let hell = usize::from(Self::LAST_HELL_SLOT - Self::FIRST_SLOT + 1).min(cums.len());
        if !guardian {
            return Self::pick_slot(roll, &cums[..hell]);
        }
        if cums.len() <= hell {
            return None;
        }
        Self::pick_slot(roll, &cums[hell..]).map(|s| s + hell as u8)
    }
}

/// ★★★ 2026-10-08 「가디언」 모드 (HERO2_GUARDIAN_PLAN.md, user: *"헬모드를 클리어하면 가디언 모드로 … 몬스터 레벨은
/// 최대 105 … 헬모드보다 강해야하고 HP도 많아야해"*): the fourth round. The mode byte `ag.i:B` goes to 3 by the
/// game's own round clear once `ag.y()V`'s `cmp r0,#1` reads `#2` ([`GuestBuild::guardian_mode_open`]), and three
/// things make 3 survive:
///
/// * **the mode-indexed arrays** `ag.i:[B` (quest EXP multipliers 1·4·7) · `ag.m:[B` (달성도 weights 50·30·20) ·
///   `co.b:[I` (slot-screen colours) are built three long in `<clinit>` — `ag.i = 3` is an
///   ArrayIndexOutOfBounds in the slot preview, the QUEST window and the script EXP. [`GuestBuild::guardian_mode_tables`]
///   makes the three `movs r1,#3` read `#4` BEFORE `ag`/`co` initialise (the logo, with the save-slot tables), and the
///   host writes the fourth cells ([`Self::QUEST_MULT`] · 0 · [`Self::COLOUR`], `JavaWrite::ModeTables`). Only when all
///   three arrays read four long does the round clear get its `#2`.
/// * **the monster template** `cq.a()V` (definition #779) scales every template once per map load with
///   `m = (ag.i > 1) ? 2 : 1` and clamps the level at 99 — mode 3 would be HELL exactly. Word 0 of its definition
///   goes to the host block: not guardian → straight to the body; guardian → `ag.i ← 2`, the body (HELL numbers),
///   `ag.i ← 3`, then the template's level `cq.h` += [`Self::LEVEL_ADD`] (cap [`Self::LEVEL_CAP`]), HP `cq.a` ×
///   [`Self::HP_MUL`]/256 (★★ 2026-10-16 no cap — monster HP is an int, [`GuestBuild::monster_hp_int`]; the other three
///   stay capped at [`Self::SHORT_CAP`]), attack `cq.b` × [`Self::ATK_MUL`]/256,
///   defence `cq.c` × [`Self::DEF_MUL`]/256, agility `cq.e` × [`Self::AGI_MUL`]/256 — six words in the block the host
///   and the probe can retune (`h2_guardian_words`).
/// * **the level correction** (Q3): the four formulas that add `monster LV − my LV` — the player's miss
///   `bd.a(La;BIZBBBB)Z` 0x127794, the boss's `ah.a(La;BIZBBBB)Z` 0x116074, the player's evade `a.a(Lbd;)Z` 0x150994
///   and the monster's critical + the status roll `a.a(Lbd;SB)V` 0x151d0a (one read, saved) — read `bd.n` through
///   `ldr r0,[pc,#pool] · <r1 = the monster> · add r0,sl · bl 0x1680a4`. Each site's pool word becomes the block's
///   READ piece, `add r0,sl` → `blx r0`, the `bl` → two `nop`s: the piece reads `bd.n` from the monster and, while
///   `ag.i ≥ 3`, hands back `bd.n − `[`Self::LEVEL_SHIFT`] (floor 1) — so a LV 105 guardian hits, crits and is dodged
///   like a LV 78 HELL monster. EXP · gold · the player's own critical second roll (0x127c56, 0x1164b2) · the target
///   panel keep the true level. ⛔ A LV 99 character is not exempt: against LV 105 its miss is 29 % and the monster's
///   critical 26 % (HELL: 8 % · 8 %) — the user asked that this be checked (Q3) and it is why the correction is on.
///
/// Everything here is a code patch — nothing is written to a record or a save; the mode byte the GAME writes is
/// the one save-bearing thing, and the game writes it (Q10: no retroactive grant).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GuardianPatch {
    /// `(def, body)` of `cq.a()V` — the monster template scale.
    pub template: (u32, u32),
    /// `ag.i:B` — the mode (0 NORMAL · 1 HARDCORE · 2 HELL · 3 GUARDIAN).
    pub mode_slot: u32,
    /// ★ 2026-10-16 보스 레이드: `ag.h:B` — the map number being loaded (static; `bf.<init>(B)V` copies it into `bf.b`). On map
    /// [`Self::W_RAID_MAP`] the wrapper and the read piece run as the fourth round whatever `ag.i` says.
    /// ⛔ ★★ 2026-10-17 (probe): `ag.h` is the map BEING LOADED — the load ends by writing −1 (255) into it, so it names the
    /// raid only while the templates are built (the wrapper's moment). The read piece runs in the fight, after: it reads the
    /// live stage's own number instead ([`Self::stage_slot`] · [`Self::stage_map_off`]).
    pub map_slot: u32,
    /// ★★ 2026-10-17: `ag.a:Lbf;` — the live stage — and its map number `bf.b:B` (absolute offset; `h2_stage_map_no`).
    pub stage_slot: u32,
    pub stage_map_off: u32,
    /// `cq`'s absolute field offsets (no parent): level `cq.h:B` · HP `cq.a:S` · attack `cq.b:S` · defence `cq.c:S` ·
    /// agility `cq.e:S`.
    pub level_off: u32,
    pub hp_off: u32,
    pub atk_off: u32,
    pub def_off: u32,
    pub agi_off: u32,
    /// `bd.n:B` — a monster's level, absolute.
    pub monster_level_off: u32,
    /// ★ 2026-10-08 밤 (A1): `cq.f:S` — the drop gate (`enm/enm` body[15..16]); 0 = a boss (or an event monster) —
    /// those get [`Self::BOSS_HP_MUL`] instead of [`Self::HP_MUL`].
    pub gate_off: u32,
    /// The four `bd.n` reads the correction hooks: `(ldr site, pool word, the pool word as shipped, the bl as shipped)`.
    pub hooks: [(u32, u32, u32, [u16; 2]); 4],
    /// ★ 2026-10-08 밤 (A3): the kill EXP — `bd.q()V`'s `a.d(I)V` call `(ldr site, pool word, the pool word as shipped)`:
    /// `ldr r0,[pc,#imm]` (the cell) · `adds r1,r4,#0` (the character) · `add r0,sl` · `mov r2,r8` (the EXP) · `mov r5,r8` ·
    /// `bl 0x16917c`. The pool word becomes the block's EXP piece, `add r0,sl` → `blx r0`: the piece scales r8 by
    /// [`Self::KILL_EXP_MUL`]/256 while `ag.i ≥ 3` and hands back r0 = the cell (sl + the shipped word), so the `bl` that
    /// follows dispatches as shipped — through `a.d(I)V`'s definition word, i.e. whatever ExpRate put there. The script
    /// reward `bb.m()V` calls `a.d(I)V` from its own site and is not touched (quest EXP stays).
    pub exp_hook: (u32, u32, u32),
    /// The mode-indexed statics the host extends: `ag.i:[B` · `ag.m:[B` · `co.b:[I`.
    pub quest_mult_slot: u32,
    pub weight_slot: u32,
    pub colour_slot: u32,
}

impl GuardianPatch {
    /// The wrapper, the read piece, the exp piece, the words table and the literal pool. ★ 2026-10-08 밤: 0x140 → 0x1c0
    /// (the exp piece and the gear reads). ★ 2026-10-16: → 0x200 (the raid map test in the wrapper and the read piece,
    /// three words more). ★ 2026-10-17: the nine-map range test (a word and three compares more) — still 0x200 (the test checks).
    pub const BLOCK_LEN: u32 = 0x380;
    pub const RAID_EVADE_AT: u32 = 0x280;
    pub const RAID_STUN_AT: u32 = 0x300;
    pub const RAID_EVADE_DEF: u32 = 0x0018_b9e8;
    pub const RAID_EVADE_BODY: u32 = 0x0015_097c;
    pub const RAID_STUN_DEF: u32 = 0x0018_c8c8;
    pub const RAID_STUN_BODY: u32 = 0x0015_c118;
    pub const RAID_EVADE_RETAIN_PERCENT: u8 = 70;
    /// `ag.i` of the fourth round.
    pub const MODE: u32 = 3;
    /// `ag.i` the body runs under (HELL).
    pub const BODY_MODE: u32 = 2;
    /// ★ 2026-10-08 저녁 (user: *"lv 70언저리에서 lv 70~80의 몬스터를 사냥하고 lv 80 언저리에서 lv 80~90 … lv90언저리에서 90~105 …
    /// 가디언모드 진입 레벨 자체를 70내외로"*): the guardian level STRETCHES the HELL band 60..80 (`2·L0/5 + 60`, L0 1..50) onto
    /// 70..105 — `LEVEL_BASE + (hell − HELL_BASE) × LEVEL_MUL / 256`, capped at LEVEL_CAP: 벌떼 60 → 70 · 반란전투병 68 → 84 ·
    /// 용암수호자 78 → 101 · 오딘 80 → 105. (The first cut was a flat +27, which put LV 87+ in the first field.)
    pub const HELL_BASE: u32 = 60;
    pub const LEVEL_BASE: u32 = 70;
    pub const LEVEL_MUL: u32 = 448;
    pub const LEVEL_CAP: u32 = 105;
    /// The level the four formulas see (Q3): the HELL level that guardian level came from — `HELL_BASE + (bd.n − LEVEL_BASE) ×
    /// SEEN_MUL / 256` (4/7 ≈ 146/256; a level below LEVEL_BASE is seen as it is). CORRECT 0 turns the correction off (the true
    /// level everywhere), for an A/B.
    pub const SEEN_MUL: u32 = 146;
    pub const CORRECT: u32 = 1;
    /// ×256 multipliers over HELL (Q1 권장: HP ×1.5 · attack ×1.2 · defence ×1.2 · agility ×1.0).
    pub const HP_MUL: u32 = 384;
    pub const ATK_MUL: u32 = 307;
    pub const DEF_MUL: u32 = 307;
    pub const AGI_MUL: u32 = 256;
    /// ★ 2026-10-08 밤 (A1, user: *"A 는 보스 x1.2로"*): a template whose drop gate `cq.f` is 0 — the 15 bosses (도적 두목 ·
    /// 이안 · 소문의 검사 · 리이드가 당주 · 디미트리 · 하인츠 · 율리케 · 고대수호물 · 오딘 · 수호물 넷 · 손 둘) and the event
    /// monsters — gets this instead of HP_MUL: 19k..29k HP at ×1.5 was ×0.1..0.3 margin even for a LV 99 knight in guardian
    /// gear (HERO2_GUARDIAN_BALANCE.md §3b). Attack · defence · agility · level scale as every other template.
    pub const BOSS_HP_MUL: u32 = 307;
    /// ★ 2026-10-08 밤 (A3, user: *"EXP는 배율설정으로 해결하는게 아니라 기본 경험치 자체를 좀 늘릴꺼야. 배율설정은 치트"*): the
    /// kill EXP (`bd.q()V`'s `bd.n·17·clamp(20 + bd.n − LV, 0, 28)/20`) ×this/256 in mode 3 — ⚠ ×1.5 is Claude's number
    /// (90 대 340..600 kills a level → 230..400), the table decides. Quest EXP keeps the game's own mode table (×10).
    pub const KILL_EXP_MUL: u32 = 384;
    /// The cap of the template's attack · defence · agility — still shorts. ★★ 2026-10-16 (user: *"상한을 아예 없애줘"*):
    /// this was `HP_CAP` and capped HP too (Q4, "a short in five places"); [`GuestBuild::monster_hp_int`] took the five
    /// 16-bit wraps out, so the wrapper scales HP uncapped.
    pub const SHORT_CAP: u32 = 32767;
    /// The fourth cells: quest EXP ×10 (after 1·4·7) · 달성도 weight 0 (the byte would wrap past 100, §1.2) · the
    /// slot-screen colour 보라 (Q8 · Q9).
    pub const QUEST_MULT: u8 = 10;
    pub const WEIGHT: u8 = 0;
    pub const COLOUR: u32 = 0x00b0_50ff;
    /// The three arrays' length as shipped, and extended.
    pub const TABLE_SHIPPED: u32 = 3;
    pub const TABLE_LEN: u32 = 4;
    /// ★ 2026-10-17 (user: *"맨처음에 구동한 뒤에 이어하기 누르면 … 가디언 난이도가 공백으로 뜨는 현상"*): the class that owns
    /// [`Self::colour_slot`]. Left to the game, `co` initialises on the slot screen's own first paint — that paint reads
    /// `co.b[3]` as the allocation's 0 (black on the black box: no GUARDIAN), and the screen keeps the picture until a key.
    /// `emulator::ModeTables` initialises it from the host during the logo, as `SaveSlots` does `ag` — `co.<clinit>` is
    /// static data only (the slot colours and the direction tables `co.a`..`co.g`).
    pub const COLOUR_CLASS: &'static str = "co";
    /// The halfwords a hook site changes: `add r0,sl` → `blx r0`, the `bl` pair → `nop nop`.
    pub const ADD_SL: u16 = 0x4450;
    pub const BLX_R0: u16 = 0x4780;
    pub const NOP: u16 = 0x46c0;
    /// The index of each tunable word in `h2_guardian_words`' table (and the probe's `--h2-guardian-word`).
    pub const W_HELL_BASE: usize = 0;
    pub const W_LEVEL_BASE: usize = 1;
    pub const W_LEVEL_MUL: usize = 2;
    pub const W_LEVEL_CAP: usize = 3;
    pub const W_HP_MUL: usize = 4;
    pub const W_ATK_MUL: usize = 5;
    pub const W_DEF_MUL: usize = 6;
    pub const W_AGI_MUL: usize = 7;
    pub const W_SEEN_MUL: usize = 8;
    pub const W_CORRECT: usize = 9;
    pub const W_BOSS_HP_MUL: usize = 10;
    pub const W_KILL_EXP_MUL: usize = 11;
    /// ★ 2026-10-08 밤 (gear): the address of the gear block's effect words ([`GearPatch`]), 0 until that block is in —
    /// the read piece reads `W_EVADE` at the evade site. Not a knob: the host writes it at the gear install.
    pub const W_GEAR_PTR: usize = 12;
    /// ★ 2026-10-09 (4) (user, balance §6 「로그 너무 쉬움」 → *"1. a만"*): the player's EVADE formula sees the TRUE guardian
    /// level — the read piece skips the correction at the evade hook alone when this is 1 (the other three formulas keep
    /// the HELL-level correction). A rogue's `a.m` sat on the 90 % cap against a level seen as ≤ 79.
    pub const W_EVADE_TRUE: usize = 13;
    pub const EVADE_TRUE: u32 = 1;
    /// ★ 2026-10-16 「보스 레이드」 (user: *"보스의 레벨은 110 고정이고 HP와 공격력은 기존 가디언 모드 하드난이도보다 1.5배 높으면 돼"*):
    /// on map W_RAID_MAP (111 = `tools_re/raid_gate.py`'s copy of 신전지하 G구역, entered from the temple's gate) the wrapper runs
    /// as the fourth round whatever `ag.i` is — the HELL body, the guardian multipliers (boss HP ×1.2 · attack · defence ×1.2) —
    /// then the level is W_RAID_LEVEL (110) instead of the stretch, and HP · attack take W_RAID_MUL/256 (×1.5) more. The read
    /// piece corrects the level on that map too (the HELL level it came from, Q3; the evade site sees 110 as everywhere). The
    /// host holds the 환경설정 난이도 at HARD on that map (`emulator::Difficulty`), so the boss is 「가디언 HARD × 1.5」 in every mode at
    /// every setting. The mode byte is restored to what it was (a raid in NORMAL stays NORMAL). ⚠ 255 turns the raid off — 0 is
    /// 윈터하츠 중앙 다운타운's number.
    /// ★★ 2026-10-17 (user: *"보스레이드의 보스가 기존 보스 순차로 … 스토리에서 진행되는 보스 순서대로"*): the raid is nine maps,
    /// W_RAID_MAP .. W_RAID_MAP + W_RAID_COUNT − 1 (113..121 = `tools_re/raid_gate.py` STAGES: 도적 두목 · 리이드가 당주 · 이안 ·
    /// 소문의 검사 · 고대수호물 · 하인츠 · 디미트리 · 율리케 · 오딘), and every test is `(ag.h − W_RAID_MAP) <u W_RAID_COUNT`.
    /// The single 111 room is gone. The 난이도 the host holds there now follows the setting (`hero2java::raid_difficulty_pct`).
    pub const W_RAID_MAP: usize = 14;
    pub const W_RAID_LEVEL: usize = 15;
    pub const W_RAID_MUL: usize = 16;
    pub const W_RAID_COUNT: usize = 17;
    pub const RAID_MAP: u32 = 113;
    pub const RAID_LEVEL: u32 = 110;
    pub const RAID_MUL: u32 = 384;
    pub const RAID_COUNT: u32 = 9;
    pub const WORDS: usize = 18;

    /// ★★ 2026-10-17: the raid stage (0-based) a map number is, the way the block tests it.
    pub const fn raid_stage(map: u32) -> Option<u32> {
        let k = map.wrapping_sub(Self::RAID_MAP);
        if k < Self::RAID_COUNT { Some(k) } else { None }
    }
    /// The hook (index into `hooks`) whose read is the player's evade `a.a(Lbd;)Z` — the one 응보의 단검 lowers.
    pub const EVADE_HOOK: usize = 1;

    /// The shipped tunable words, in table order.
    pub const fn words() -> [u32; Self::WORDS] {
        [Self::HELL_BASE, Self::LEVEL_BASE, Self::LEVEL_MUL, Self::LEVEL_CAP, Self::HP_MUL, Self::ATK_MUL, Self::DEF_MUL, Self::AGI_MUL, Self::SEEN_MUL, Self::CORRECT, Self::BOSS_HP_MUL, Self::KILL_EXP_MUL, 0, Self::EVADE_TRUE, Self::RAID_MAP, Self::RAID_LEVEL, Self::RAID_MUL, Self::RAID_COUNT]
    }

    /// The HP multiplier a template gets: a drop gate of 0 is a boss (or an event monster).
    #[allow(dead_code)]
    pub const fn hp_mul_for(gate: i32) -> u32 {
        if gate == 0 {
            Self::BOSS_HP_MUL
        } else {
            Self::HP_MUL
        }
    }

    /// The kill EXP a guardian kill hands `a.d(I)V`: `exp × KILL_EXP_MUL >> 8` in mode 3, as it is elsewhere.
    #[allow(dead_code)]
    pub const fn kill_exp(exp: i32, mode: u32) -> i32 {
        if mode < Self::MODE {
            exp
        } else {
            ((exp as i64 * Self::KILL_EXP_MUL as i64) >> 8) as i32
        }
    }

    /// What the wrapper does to one HELL template value: `v × mul / 256`, capped at `cap` (for the tests and the tools;
    /// HP's cap is `i32::MAX` since 2026-10-16, the others' [`Self::SHORT_CAP`]).
    #[allow(dead_code)]
    pub const fn scaled(v: i32, mul: u32, cap: i32) -> i32 {
        let r = (v as i64 * mul as i64) >> 8;
        if r > cap as i64 {
            cap
        } else {
            r as i32
        }
    }

    /// The guardian level of a HELL level: `min(LEVEL_BASE + max(hell − HELL_BASE, 0) × LEVEL_MUL / 256, LEVEL_CAP)`.
    #[allow(dead_code)]
    pub const fn level(hell: i32) -> i32 {
        let d = hell - Self::HELL_BASE as i32;
        let d = if d < 0 { 0 } else { d };
        let l = Self::LEVEL_BASE as i32 + ((d * Self::LEVEL_MUL as i32) >> 8);
        if l > Self::LEVEL_CAP as i32 {
            Self::LEVEL_CAP as i32
        } else {
            l
        }
    }

    /// What a corrected formula sees of a monster's level in mode `mode`: the HELL level it came from.
    #[allow(dead_code)]
    pub const fn seen_level(level: i32, mode: u32) -> i32 {
        if mode < Self::MODE || Self::CORRECT == 0 || level < Self::LEVEL_BASE as i32 {
            return level;
        }
        let l = Self::HELL_BASE as i32 + (((level - Self::LEVEL_BASE as i32) * Self::SEEN_MUL as i32) >> 8);
        if l < 1 {
            1
        } else {
            l
        }
    }
}

/// ★★★ 2026-10-09 「가디언」 장비의 개성 옵션 31 (HERO2_GUARDIAN_DECISIONS.md C1 → user 2026-10-09: 무기 넷 옵션 교체 · 율리케의
/// 단두도끼 → 제스의 수련도끼 · 방어구는 세트처럼 보이지 않게 이름을 새로, 부위·종류마다 하나씩 더해 18종 + 저마다 개성 옵션 ·
/// 방패 하나는 방패치기 +50 % · 장신구 셋도 「가디언」 없는 이름 · 서약 반지는 최대 HP +10 % — ⚠ every other value is Claude's draft):
/// the ten weapons, eighteen armour pieces and three accessories (`tools_re/guardian_gear.py`) each carry an option while
/// WORN. The records hold numbers only (nothing here is written to a record or a save); the options live in one host
/// block, table-driven:
///
/// * **the effect words** (the block's first [`Self::EFFECTS_LEN`] bytes, [`Self::effects_for`]): every 5 frames the host
///   reads the seven equipment slots and the class, sums the worn items' [`GearEffect`]s and writes the words — received
///   damage ×, reflect, SP cost ×, crit +, evade +, venom +, heal on kill, buff frames by id and a per-class per-kind
///   damage table. All zero / ×1 = the shipped game.
/// * **the pieces** read those words: the SP cost post-wrappers `<job>.a(I)I` ×3, the damage pre-wrappers
///   `<job>.a(BILbd;)I` ×3 (the kind's u16 add from the class's row), 철괴's hook (received ×, then the previous hook, then
///   the reflect to `sb` = the monster through its own `bd.a(IB)V`), the buff add `g.a(BI)V` (the character's id 30..=37),
///   the crit hook in `bd.a(La;BIZBBBB)Z` (`a.l:I` + crit, 0x127c46), and the guardian / venom blocks' pointer words
///   (their read piece's evade site and the venom tick); the kill wrapper heals HP/SP.
/// * **max HP / SP** cannot be a host word: `a.m()V` recomputes the maxima on every equip change, level, buff end and
///   hit, and clamps the current HP/SP to them INSIDE itself (0x1503f4 · 0x1503d8), before any host poll could follow.
///   So its post-wrapper walks the equipment itself against a table built from [`Self::ITEMS`] (`GearEffect::MaxHp` ·
///   `MaxSp`), raises the maxima, and puts back `min(the HP/SP before the call, the new maximum)` — the body's clamp used
///   the unraised one.
/// * ★★ **the HP follows the max HP over an equipment change** (2026-10-21, user: *"HP가 늘어나는 아이템을 끼면 현재 체력도
///   그만큼 같이 늘어나야해. 최대체력이 늘어난 퍼센트만큼"*): the wrapper also keeps the max HP before the call and a signature
///   of the seven slots' records; when the slots changed it hands back `HP × new max / old max` (the full HP stays full,
///   never under 1; taking the item off brings it down by the same ratio — equipping and unequipping cannot heal), and
///   with the same slots the `min` above. A level, a stat point, a buff end and a hit change no slot: as before.
///   And a LOAD (the bug: an HP over the unequipped maximum came back cut — the character's blob is read and recomputed
///   before `a.n()V` puts the equipment on): the blob reader `a.a([B)V` is wrapped too ([`Self::load`]) — its post-wrapper
///   keeps the saved HP (the blob's bytes [`Self::BLOB_HP_AT`]..+4, big-endian), and the recomputes that follow hand
///   back `min(saved, new maximum)` until it fits — while nothing else has moved the HP ([`Self::HP_PEND_LAST`]).
///
/// Every wrapped word CHAINS to what it held when this block went in (this block is installed after every other one
/// that wraps them, `emulator::Gear`); the chain words are DATA in the block, read at run time.
/// ⛔ The player's strike `a.a(Lbd;BIBB)Z(target, kind, a.a()I, a.l, a.m)` passes no crit: its `a.l:B` is the attack
/// ELEMENT and `a.m:B` the STATUS kind; the crit is rolled at 0x127c38 (`h.a(0,99) < attacker.a.l:I`, then a second roll
/// `clamp(20 + bd.n − LV, 8, 50)`), so crit + n raises the FIRST roll only (HERO2_GUEST_RE.md §8.56).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GearPatch {
    /// `(def, body)` of the three classes' SP cost `<job>.a(I)I` — `bl` 버서커 · `bx` 나이트 · `ay` 로그.
    pub sp: [(u32, u32); 3],
    /// `(def, body)` of the three damage functions — `bl.a(BILbd;)I` · `bx.a(BILbd;)I` · `ay.a(BILbd;)I`.
    pub damage: [(u32, u32); 3],
    /// `(def, body)` of `g.a(BI)V` (the buff add — the buff-refresh block's word).
    pub buff: (u32, u32),
    /// `(def, body)` of `a.m()V` (the stat recompute — max HP / SP).
    pub stats: (u32, u32),
    /// The crit site in `bd.a(La;BIZBBBB)Z`: `(ldr site, pool word, the pool word as shipped)` of the attacker's `a.l:I`
    /// read (`ldr r0,[pc,#0x94]` · `mov r1,sb` · `add r0,sl` · `bl 0x1680a4`) — the pool word becomes the CRIT piece,
    /// `add r0,sl` → `blx r0`, the `bl` → `nop nop`; `r1` = the attacker.
    pub crit_hook: (u32, u32, u32),
    /// `a.l:I` — the character's crit %, ABSOLUTE (0x64 declared + the 0x5c parent).
    pub crit_off: u32,
    /// The character's static slot (`ag.a:La;`).
    pub char_slot: u32,
    /// `bd.a(IB)V`'s cell and the virtual-call helper (the reflection).
    pub cell_damage: u32,
    pub dispatch: u32,
    /// ★ 2026-10-09 (3): `(def, body)` of `ag.a(IZ)V` — the quickbar cooldown start (r1 = the skill, r2 = a beast skill:
    /// it finds the slot holding `r1` (+10 for a beast) and writes `ag.a[slot]` = the class table's `a(I)S` or the beast's
    /// `bn.a:[S`) — and of `bd.q()V`, the kill (the HELL drop block's word too).
    pub cooldown_start: (u32, u32),
    pub kill: (u32, u32),
    /// Static slots: the quickbar values `ag.j:[B` and their counters `ag.a:[I` (`-1` = ready).
    pub quickbar_slot: u32,
    pub cooldown_slot: u32,
    /// The game's random `h.a(II)I` (its cell) and the static-call helper.
    pub cell_random: u32,
    pub call_static: u32,
    /// ★★ 2026-10-14 `(def, body)` of the three cooldown tables `<job>.a(I)S` (`bl` 버서커 · `bx` 나이트 · `ay` 로그 — index = the
    /// quickbar value − 1): post-wrappers take [`GearEffect::KindCd`]'s cut. Chained after the accessory · combat · 일풍격 ·
    /// berserker wrappers (the rogue block wraps `ay`'s after us — its 살상연희 · 개화 answers are its own).
    pub cd_getter: [(u32, u32); 3],
    /// ★★ 2026-10-14 (2차 B2) `(def, body)` of `a.f()Z` — the combo gate: a 기본공격 stage and the class's FIRST skill (kind ==
    /// the stages — 휘두르기 · 치유오라 · 일격/일섬) pay `a.j:I` there, the later skills ask `<job>.a(I)I`; the SP moves through
    /// `a.a(I)V` inside it (#16: the stages gain, the skills pay). A post-wrapper reads the SP before and after.
    pub combo: (u32, u32),
    /// `a.a(I)V`'s definition — the SP delta (clamped to [0, max]); the wrapper adds through it.
    pub sp_delta_def: u32,
    /// The character's 연계 table `a.o:[B` and its index `a.i:B`, ABSOLUTE — the queued kind is `a.o[a.i]` (RoguePatch's).
    pub queue_off: u32,
    pub queue_at_off: u32,
    /// ★★ 2026-10-15 (2차 F2 술꾼의 장화) `(def, body)` of `a.g(B)V` — use the inventory record at that index (the `0` key's
    /// `a.q()V` and the inventory popup's 사용하기 `q.a(BB)V` both; types 24 포션 · 25 하이포션 · 26 포도주 · 27 해독제/정화제 have
    /// effects, `bq.a(BI)V` takes one) · `(def, body)` of `a.g()V`, the character's update (the berserker block's wrapper under
    /// it, the demon block's over it) · the inventory `ag.a:Lbq;` static slot and `bq.a(I)Lch;`'s cell (the record at an index) ·
    /// a record's type `ch.g:B` and count `ch.c:I` (absolute) · the walk: `g.t` (absolute), the walk request `ag.k:B`, `g.b()Z`'s
    /// definition (the update's own step — `DemonPatch::step_def`).
    pub use_item: (u32, u32),
    pub update: (u32, u32),
    pub bag_slot: u32,
    pub cell_bag_record: u32,
    pub record_type_off: u32,
    pub record_count_off: u32,
    pub state_off: u32,
    pub walk_request_slot: u32,
    pub step_def: u32,
    /// ★★ 2026-10-17 `a.i(B)V`'s definition and `a.v:Z`'s absolute offset (= `DemonPatch::arrive_def` · `moved_off`): the
    /// walk's extra step (지옥 발굽 · 술꾼의 장화) always lands ON a tile — the game checks a tile's auto event only after its own
    /// step, so every tile that step reached went unchecked (밟으면 나오는 대사 · 계단). It now runs the game's post-step check.
    pub arrive_def: u32,
    pub moved_off: u32,
    /// ★★ 2026-10-15 (2차 H2 검풍) `(def, body)` of `bx.b(B)Z` — the knight's usable check (`r2` = the kind; false = not cast:
    /// no SP, no cooldown — `a.k()V` asks it at `g.w == 0` before `a.f()Z` pays). Nothing else wraps it.
    pub usable: (u32, u32),
    /// ★★ 2026-10-21 `(def, body)` of `a.a([B)V` — the character blob's reader (`IlpungPatch::load`; the 일풍격 block's wrapper
    /// sits under ours): the saved HP waits there for the equipment ([`Self::HP_PEND`]).
    pub load: (u32, u32),
    /// The game's signed divide (r0 / r1 → r0, a leaf that keeps r4..r7) — the HP's ratio.
    pub divide: u32,
}

/// One option effect (the host sums them over the worn items, [`GearPatch::effects_for`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GearEffect {
    /// Received damage × n/256 (multiplied over the worn items).
    Taken(u8),
    /// n/256 of the damage taken back to the monster (summed).
    Reflect(u8),
    /// Every skill's SP cost × n/256 (multiplied).
    SpCost(u8),
    /// The first crit roll + n (percentage points).
    Crit(u8),
    /// Evade + n (percentage points, before the formula's clamp).
    Evade(u8),
    /// The 맹독감염 poison tick + n/256.
    Venom(u8),
    /// A kill heals n/256 of the max HP / SP, including kills that pay no EXP.
    HealHp(u8),
    HealSp(u8),
    /// The character's buff `id` (30..=37) lasts n more seconds.
    Buff(u8, u8),
    /// `(class, first kind, last kind, add)`: those kinds' damage + add/256 for that class (0 버서커 · 1 나이트 · 2 로그).
    Dmg(u8, u8, u8, u16),
    /// The wearer's 기본공격 kinds (below its class's stages) + add/256.
    Basic(u8),
    /// The maximum HP / SP + n/256 (the `a.m()V` piece's own table).
    MaxHp(u8),
    MaxSp(u8),
    Guard(u8),
    /// ★ 2026-10-09 (3): a 기본공격 hit heals n/256 of the damage it dealt (`<job>.a(BILbd;)I`'s answer, before the crit roll).
    Lifesteal(u8),
    /// The beast's skill cooldowns × n/256 (multiplied) — the counter `ag.a(IZ)V` starts for a beast skill.
    BeastCd(u8),
    /// A kill has n % to give the 「용의 심장」 buff: [`GearPatch::DRAGON_FRAMES`] of received damage × [`GearPatch::DRAGON_MUL`].
    DragonProc(u8),
    /// A hit has n % to reset every skill cooldown (the four guest counters, and the host's parked ones).
    ResetProc(u8),
    /// ★★ 2026-10-14 `(class, kind, cut)`: that class's skill `kind`'s SP cost − cut/256 (`<job>.a(I)I`; cuts multiply).
    KindSp(u8, u8, u8),
    /// ★★ 2026-10-14 `(class, index, cut)`: that class's cooldown table entry `index` (`<job>.a(I)S` — the quickbar value − 1:
    /// 일격/일섬 = the rogue's 0) − cut/256 (cuts multiply).
    KindCd(u8, u8, u8),
    /// ★★ 2026-10-14 the effects count only while `(type, id)` is worn too — a set's part (번개의 호흡 + 벽력일섬 = 일섬).
    With(u8, u8, &'static [GearEffect]),
    /// ★★ 2026-10-14 (2차 B5) the wearer's class skills — every kind from its 기본공격 stages up but its awakening
    /// (`IlpungPatch::AWAKEN`, 한 방 스킬은 올리지 않는다 — 초안 §1-3) — + add/256.
    Skills(u8),
    /// ★★ 2026-10-14 (2차 B4) every cooldown table index of the wearer's class − cut/256 (cuts multiply) — the awakening's
    /// too (its class block's `a(I)S` answer runs through the gear block's wrapper after it).
    AllCd(u8),
    /// ★★ 2026-10-14 (2차 G1 · G4) the berserker's counter (반격태세 — kind 8 through `bl.a(BILbd;)I`, whose answer is the
    /// counter's own `a.a()I × 배율`) + n/256 of that answer.
    Counter(u8),
    /// ★★ 2026-10-14 (2차 G4 · G6) a counter heals n/256 of its answer.
    CounterLeech(u8),
    /// ★★ 2026-10-14 (2차 A1 · A2) a hit the berserker deals while 격분 runs lengthens it by n frames (once a tick; summed).
    FuryHit(u8),
    /// ★★ 2026-10-14 (2차 A3) a hit that lands on him while 격분 runs: + n frames (summed).
    FuryHurt(u8),
    /// ★★ 2026-10-14 (2차 A3) a kill while 격분 runs: + n frames (summed).
    FuryKill(u8),
    /// ★★ 2026-10-14 (2차 A3) received damage × n/256 while 격분 runs (multiplied).
    FuryTaken(u8),
    /// ★★ 2026-10-14 (2차 A4) the explosion does not end 격분 — the gauge stays at n % (the largest worn).
    FuryKeep(u8),
    /// ★★ 2026-10-14 (2차 A4) the explosion's cooldown in ticks instead of [`BerserkerPatch::FURY_COOLDOWN`] (the shortest worn).
    FuryCd(u16),
    /// ★★ 2026-10-14 (2차 A5 · A5′) 격분 starts at n % of the gauge (the largest worn).
    FuryStart(u8),
    /// ★★ 2026-10-14 (2차 A5 · A5′) every damage the berserker deals while 격분 runs + n/256 (the hits and the explosion; summed).
    FuryDmg(u8),
    /// ★★ 2026-10-14 (2차 G2) received damage × n/256 while 반격태세's stance (`a.o:Z`) is up (multiplied).
    StanceTaken(u8),
    /// ★★ 2026-10-14 (2차 G3) 반격태세 never ends by itself — the cast's buff 31 is ON (the length that never ends), a cast
    /// while it is up switches it OFF.
    StanceHold,
    /// ★★ 2026-10-14 (2차 G3) received damage × n/256 while 도발 runs (multiplied; on top of 도발's own).
    TauntTaken(u8),
    /// ★★ 2026-10-14 (2차 G5) a counter's answer × n/256 to every other live monster on the 3×3 round its target (summed).
    CounterSplash(u8),
    /// ★★ 2026-10-14 (2차 B1 시간도둑) every n 기본공격 hits that land, every running cooldown − [`GearPatch::THIEF_TICKS`] — the
    /// four guest counters here, the host's parked ones off the bar through THIEF_SECS (the fewest hits worn).
    TimeThief(u8),
    /// ★★ 2026-10-14 (2차 B2 흐름의 장화) the SP a 기본공격 stage GAINS (#16) + n/256 (summed) — `a.f()Z`'s post-wrapper.
    ComboSp(u8),
    /// ★★ 2026-10-15 (2차 C — 악의 장비, the demon block's DGEAR): the claw's attack + n/256 (summed) · the claw a fifth faster
    /// (the % in the text — a source, not summed) · the walk a quarter faster while 악의 runs (the %) · 악의 n seconds longer
    /// (summed) · a kill while it runs + n frames (summed; ★★ 2026-10-17 no ceiling — what was left stopped at 75 s).
    DemonClaw(u8),
    /// ★★ 2026-10-15 (C1 → user *"악의 시 공격속도 옵션도 기본 공격 속도"*) 기본공격 a fifth faster — the claw while 악의 runs (DGEAR's
    /// OP_FAST) and every class's stages otherwise (the demon block's FASTX); the % is the text's (a source, not summed).
    BasicFast(u8),
    DemonRun(u8),
    DemonDur(u8),
    DemonKill(u8),
    /// ★★ 2026-10-15 (협곡 지하 곡괭이 · 스프링 부츠) 둔기강타's reach +n % a worn item (n = 50): the items counted into W_RANGE's
    /// [`GearPatch::RANGE_REACH_SHIFT`] bits (at most [`GearPatch::RANGE_REACH_MAX`]) — the combat block's RANGE piece makes the
    /// tile count ([`CombatPatch::bludgeon_tiles`]).
    BluntReach(u8),
    /// ★★ 2026-10-17 (발도 특화 아이템) 발도's radius + n tiles a worn item (n = 1): the items counted into W_RANGE's
    /// [`GearPatch::IAIDO_REACH_SHIFT`] bits (at most [`GearPatch::IAIDO_REACH_MAX`]) — the combat block's REACH helper makes the
    /// radius, 발도's piece the tiles ([`CombatPatch::iaido_tiles`]) and the sequence map the ring.
    IaidoReach(u8),
    /// ★★ 2026-10-15 (2차 C2 지옥 발굽) the walk n % faster all the time (25 → one extra step a four, the gear block's update
    /// wrapper — [`GearPatch::W_WALK`]); the largest worn.
    Walk(u8),
    /// ★★ 2026-10-15 (2차 C5 풀린 목줄) received damage × n/256 while 악의 runs (multiplied; after 악의's own × 85/100) — the
    /// demon block's TAKEN piece reads the cut ([`GearPatch::W_DMN_TAKEN`]) through its GEAR_PTR.
    DemonTaken(u8),
    /// ★★ 2026-10-15 (2차 F1 중력의 갑주) every equipment record the wearer carries — the inventory's five pages, the worn
    /// seven among them (they live in `ag.a` too) — adds n‰ to final monster damage.
    /// The host counts equipment only; materials and the warehouse are excluded.
    Gravity(u8),
    Ability(u8, u8),
    /// ★★ 2026-10-15 (2차 E) range bits ([`GearPatch::RANGE_STAR`] …) — OR-ed.
    Range(u8),
    /// ★★ 2026-10-15 (2차 F2 술꾼의 장화) a potion drunk (types 24..27, its count down — `a.g(B)V`, the `0` key and 사용하기 alike)
    /// → this many character updates (20 a second) of one extra walk step every other update (+50 %); a second potion starts it
    /// over. The largest worn.
    Drunk(u16),
    /// ★★ 2026-10-15 (2차 H2 검풍) the KNIGHT's 검기 becomes passive: every 기본공격 stage's hit frame fires one (the combat block's
    /// cast wrapper, [`GearPatch::RANGE_WIND`]), its hits × n/256 (the knight's damage wrapper, [`GearPatch::W_WIND`]), and 검기
    /// can no longer be cast (the usable wrapper). Another class wearing it: nothing (a plain 도검).
    Wind(u8),
    /// ★★ 2026-10-16 악의 무기 M1 광마의 대검 (HERO2_DEMON_WEAPON_DRAFT.md): every claw hit that lands while 악의 runs banks n % of
    /// the gauge for the NEXT 격분 (at most 100; the cast spends it) — the HOST counts (the demon block's HITS) and lifts
    /// W_FURY_START to it ([`GearPatch::effects_for_live`]). The berserker's only.
    ClawBank(u8),
    /// ★★ 2026-10-16 M1: every hit the berserker lands while 격분 runs takes n ticks off 악의's cooldown — the HOST counts (the gear
    /// block's HIT_COUNT, once a tick at most) and shortens the counter (the guest's or its parked one).
    FuryMaliceCd(u8),
    /// ★★ 2026-10-16 M2 피바람 도끼: the claw's first two stages rake the four sides at once (the window turned to every facing — the
    /// demon block's CLAW reads [`GearPatch::FLAG_SPIN`]); the picture is the four facings' cut together (`claw{c}_spin`).
    ClawSpin,
    /// ★★ 2026-10-16 M2: a kill while 악의 runs heals n/256 of the max HP — the HOST adds it to W_HEAL_HP while STATE 2.
    DemonKillHeal(u8),
    /// ★★ 2026-10-16 M3 타락한 성검: while 신의가호 runs the claw's windows reach a tile deeper and a tile wider (the HOST sets
    /// [`GearPatch::FLAG_WIDE`] only then); the picture is the cut at [`DemonPatch::CLAW_BIG_SCALE`] (`claw{c}_{d}_big`). The knight's only.
    ClawWideBlessed,
    /// ★★ 2026-10-16 M3: 지옥칼날 · 어둠심판's SP a second × n/256 (multiplied) — the demon block's drain reads W_DMN_TOGSP (the cut).
    ToggleSp(u8),
    /// ★★ 2026-10-16 M4 악몽의 독니: a claw hit that lands poisons the monster (the game's status 26 for
    /// [`VenomPatch::POISON_LEN`] — its ticks are the venom block's, 맹독감염's level) — [`GearPatch::FLAG_VENOM`].
    ClawVenom,
    /// ★★ 2026-10-16 M4: the claw on a monster already poisoned + n/256 (summed — two daggers worn, two shares).
    ClawPoisoned(u8),
    /// ★★ 2026-10-16 M5 계약의 철퇴: 둔기강타 may be cast while 악의 runs (the demon block's ARM lets the knight's value 3 through) —
    /// [`GearPatch::FLAG_SLAM`]. The knight's only.
    DemonSlam,
    /// ★★ 2026-10-16 M5 「짐승의 박자」 (user: *"짐승의박자로 결정"*): a beast skill cast (its cooldown starting — the gear block's
    /// `ag.a(IZ)V` wrapper) opens a window of `frames` clock ticks; the next 둔기강타 in it costs no SP (the knight's SP wrapper
    /// answers 0) and spends the window (the combo gate's post-wrapper), and its hits + `add`/256 (the knight's damage
    /// wrapper, while BEAT_ON). The knight's only.
    Beat(u8, u8),
}

/// ★★ 2026-10-16 (악의 무기) what the HOST knows each poll that a few options follow ([`GearPatch::effects_for_live`]): 악의 runs on
/// this character (the demon block's STATE 2) · 신의가호 runs (the knight's buff 34) · the 격분 gauge % banked by the claw (M1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GearLive {
    pub demon_on: bool,
    pub blessed: bool,
    pub fury_bank: u8,
}

/// One item and its option: `(type, id)`, the option as the player reads it (the drop site prints it, a selftest checks
/// its numbers against the effects), the effects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GearItem {
    pub item_type: u8,
    pub id: u8,
    pub text: &'static str,
    pub effects: &'static [GearEffect],
}

/// The running sums of [`GearPatch::effects_for`] (one item's effects at a time — a [`GearEffect::With`] goes in when its partner is
/// worn too).
struct GearAcc {
    taken: u32,
    reflect: u32,
    sp: u32,
    crit: u32,
    evade: u32,
    venom: u32,
    heal_hp: u32,
    heal_sp: u32,
    lifesteal: u32,
    beast_cd: u32,
    reset: u32,
    dragon: u32,
    buff: [u32; GearPatch::BUFF_IDS as usize],
    dmg: [[u16; GearPatch::KINDS as usize]; 3],
    /// The kept fraction (/256) per (class, kind) SP and (class, index) cooldown — cuts multiply.
    ksp: [[u32; GearPatch::KINDS as usize]; 3],
    kcd: [[u32; GearPatch::KINDS as usize]; 3],
    counter: u32,
    counter_ls: u32,
    fury_hit: u32,
    fury_hurt: u32,
    fury_kill: u32,
    fury_taken: u32,
    fury_keep: u32,
    fury_cd: u32,
    fury_start: u32,
    fury_dmg: u32,
    stance_taken: u32,
    stance_hold: u32,
    taunt_taken: u32,
    counter_splash: u32,
    thief: u32,
    combo_sp: u32,
    dmn_claw: u32,
    dmn_fast: u32,
    dmn_run: u32,
    dmn_dur: u32,
    dmn_kill: u32,
    dmn_taken: u32,
    walk: u32,
    blunt_reach: u32,
    iaido_reach: u32,
    gravity: u32,
    range: u32,
    drunk: u32,
    wind: u32,
    /// ★★ 2026-10-16 악의 무기: the flags · the toggles' kept SP (/256) · the poisoned claw's add · 짐승의 박자 · the host-only
    /// shares (the bank % · 악의's ticks a fury hit · the kill's heal while 악의 runs).
    dmn_flags: u32,
    dmn_togsp: u32,
    dmn_poisoned: u32,
    beat: u32,
    beat_add: u32,
    claw_bank: u32,
    fury_malice_cd: u32,
    demon_kill_heal: u32,
}

impl GearAcc {
    fn new() -> Self {
        Self {
            taken: 256,
            reflect: 0,
            sp: 256,
            crit: 0,
            evade: 0,
            venom: 0,
            heal_hp: 0,
            heal_sp: 0,
            lifesteal: 0,
            beast_cd: 256,
            reset: 0,
            dragon: 0,
            buff: [0; GearPatch::BUFF_IDS as usize],
            dmg: [[0; GearPatch::KINDS as usize]; 3],
            ksp: [[256; GearPatch::KINDS as usize]; 3],
            kcd: [[256; GearPatch::KINDS as usize]; 3],
            counter: 0,
            counter_ls: 0,
            fury_hit: 0,
            fury_hurt: 0,
            fury_kill: 0,
            fury_taken: 256,
            fury_keep: 0,
            fury_cd: 0,
            fury_start: 0,
            fury_dmg: 0,
            stance_taken: 256,
            stance_hold: 0,
            taunt_taken: 256,
            counter_splash: 0,
            thief: 0,
            combo_sp: 0,
            dmn_claw: 0,
            dmn_fast: 0,
            dmn_run: 0,
            dmn_dur: 0,
            dmn_kill: 0,
            dmn_taken: 256,
            walk: 0,
            blunt_reach: 0,
            iaido_reach: 0,
            gravity: 0,
            range: 0,
            drunk: 0,
            wind: 0,
            dmn_flags: 0,
            dmn_togsp: 256,
            dmn_poisoned: 0,
            beat: 0,
            beat_add: 0,
            claw_bank: 0,
            fury_malice_cd: 0,
            demon_kill_heal: 0,
        }
    }

    fn apply(&mut self, e: &GearEffect, worn: &[(u8, u8)], class: u8) {
        type P = GearPatch;
        match *e {
            GearEffect::Taken(m) => self.taken = self.taken * u32::from(m) >> 8,
            GearEffect::Reflect(n) => self.reflect += u32::from(n),
            GearEffect::SpCost(m) => self.sp = self.sp * u32::from(m) >> 8,
            GearEffect::Crit(n) => self.crit += u32::from(n),
            GearEffect::Evade(n) => self.evade += u32::from(n),
            GearEffect::Venom(n) => self.venom += u32::from(n),
            GearEffect::HealHp(n) => self.heal_hp += u32::from(n),
            GearEffect::HealSp(n) => self.heal_sp += u32::from(n),
            GearEffect::Buff(id, s) => {
                if let Some(k) = id.checked_sub(P::BUFF_FIRST).filter(|k| *k < P::BUFF_IDS) {
                    self.buff[usize::from(k)] += 20 * u32::from(s);
                }
            }
            GearEffect::Dmg(c, lo, hi, add) => {
                if usize::from(c) < 3 {
                    for k in lo..=hi.min(P::KINDS - 1) {
                        self.dmg[usize::from(c)][usize::from(k)] += add;
                    }
                }
            }
            GearEffect::Basic(add) => {
                if usize::from(class) < 3 {
                    for k in 0..P::STAGES[usize::from(class)] {
                        self.dmg[usize::from(class)][usize::from(k)] += u16::from(add);
                    }
                }
            }
            GearEffect::Lifesteal(n) => self.lifesteal += u32::from(n),
            GearEffect::BeastCd(m) => self.beast_cd = self.beast_cd * u32::from(m) >> 8,
            GearEffect::DragonProc(n) => self.dragon += u32::from(n),
            GearEffect::ResetProc(n) => self.reset += u32::from(n),
            GearEffect::MaxHp(_) | GearEffect::MaxSp(_) | GearEffect::Guard(_) | GearEffect::Ability(_, _) => {}
            GearEffect::KindSp(c, k, cut) => {
                if usize::from(c) < 3 && k < P::KINDS {
                    let keep = &mut self.ksp[usize::from(c)][usize::from(k)];
                    *keep = *keep * (256 - u32::from(cut)) >> 8;
                }
            }
            GearEffect::KindCd(c, k, cut) => {
                if usize::from(c) < 3 && k < P::KINDS {
                    let keep = &mut self.kcd[usize::from(c)][usize::from(k)];
                    *keep = *keep * (256 - u32::from(cut)) >> 8;
                }
            }
            GearEffect::With(t, i, inner) => {
                if worn.contains(&(t, i)) {
                    for e in inner {
                        self.apply(e, worn, class);
                    }
                }
            }
            GearEffect::Skills(add) => {
                if usize::from(class) < 3 {
                    let awaken = IlpungPatch::AWAKEN[usize::from(class)].0;
                    for k in P::STAGES[usize::from(class)]..P::KINDS {
                        if k != awaken {
                            self.dmg[usize::from(class)][usize::from(k)] += u16::from(add);
                        }
                    }
                }
            }
            GearEffect::AllCd(cut) => {
                if usize::from(class) < 3 {
                    for keep in self.kcd[usize::from(class)].iter_mut() {
                        *keep = *keep * (256 - u32::from(cut)) >> 8;
                    }
                }
            }
            GearEffect::Counter(n) => self.counter += u32::from(n),
            GearEffect::CounterLeech(n) => self.counter_ls += u32::from(n),
            GearEffect::FuryHit(n) => self.fury_hit += u32::from(n),
            GearEffect::FuryHurt(n) => self.fury_hurt += u32::from(n),
            GearEffect::FuryKill(n) => self.fury_kill += u32::from(n),
            GearEffect::FuryTaken(m) => self.fury_taken = self.fury_taken * u32::from(m) >> 8,
            GearEffect::FuryKeep(n) => self.fury_keep = self.fury_keep.max(u32::from(n)),
            GearEffect::FuryCd(t) => self.fury_cd = if self.fury_cd == 0 { u32::from(t) } else { self.fury_cd.min(u32::from(t)) },
            GearEffect::FuryStart(n) => self.fury_start = self.fury_start.max(u32::from(n)),
            GearEffect::FuryDmg(n) => self.fury_dmg += u32::from(n),
            GearEffect::StanceTaken(m) => self.stance_taken = self.stance_taken * u32::from(m) >> 8,
            GearEffect::StanceHold => self.stance_hold = 1,
            GearEffect::TauntTaken(m) => self.taunt_taken = self.taunt_taken * u32::from(m) >> 8,
            GearEffect::CounterSplash(n) => self.counter_splash += u32::from(n),
            GearEffect::TimeThief(n) => self.thief = if self.thief == 0 { u32::from(n) } else { self.thief.min(u32::from(n)) },
            GearEffect::ComboSp(n) => self.combo_sp += u32::from(n),
            GearEffect::DemonClaw(n) => self.dmn_claw += u32::from(n),
            GearEffect::BasicFast(_) => self.dmn_fast = 1,
            GearEffect::DemonRun(_) => self.dmn_run = 1,
            GearEffect::DemonDur(s) => self.dmn_dur += 20 * u32::from(s),
            GearEffect::DemonKill(f) => self.dmn_kill += u32::from(f),
            GearEffect::DemonTaken(m) => self.dmn_taken = self.dmn_taken * u32::from(m) >> 8,
            GearEffect::Walk(n) => self.walk = self.walk.max(u32::from(n)),
            GearEffect::BluntReach(_) => self.blunt_reach += 1,
            GearEffect::IaidoReach(_) => self.iaido_reach += 1,
            GearEffect::Gravity(n) => self.gravity += u32::from(n),
            GearEffect::Range(bits) => self.range |= u32::from(bits),
            GearEffect::Drunk(n) => self.drunk = self.drunk.max(u32::from(n)),
            GearEffect::Wind(n) => {
                if class == 1 {
                    self.range |= u32::from(GearPatch::RANGE_WIND);
                    self.wind = u32::from(n);
                }
            }
            // ★★ 2026-10-16 악의 무기 — the class-bound ones count only for their class (a weapon another class cannot wear anyway)
            GearEffect::ClawBank(n) => {
                if class == 0 {
                    self.claw_bank = self.claw_bank.max(u32::from(n));
                }
            }
            GearEffect::FuryMaliceCd(t) => {
                if class == 0 {
                    self.fury_malice_cd += u32::from(t);
                }
            }
            GearEffect::ClawSpin => self.dmn_flags |= GearPatch::FLAG_SPIN,
            GearEffect::DemonKillHeal(n) => self.demon_kill_heal += u32::from(n),
            GearEffect::ClawWideBlessed => {
                if class == 1 {
                    self.dmn_flags |= GearPatch::FLAG_WIDE;
                }
            }
            GearEffect::ToggleSp(m) => self.dmn_togsp = self.dmn_togsp * u32::from(m) >> 8,
            GearEffect::ClawVenom => self.dmn_flags |= GearPatch::FLAG_VENOM,
            GearEffect::ClawPoisoned(n) => self.dmn_poisoned += u32::from(n),
            GearEffect::DemonSlam => {
                if class == 1 {
                    self.dmn_flags |= GearPatch::FLAG_SLAM;
                }
            }
            GearEffect::Beat(frames, add) => {
                if class == 1 {
                    self.beat = self.beat.max(u32::from(frames));
                    self.beat_add = self.beat_add.max(u32::from(add));
                }
            }
        }
    }
}

impl GearPatch {
    /// The block: the effect words (host-written), the chain words, then the code (with the max-stat table and the pool).
    /// ★★ 2026-10-14 0x600 → 0x800 and the effect words 0x100 → 0x200: the per-skill SP / cooldown cut tables (KSP_AT · KCD_AT)
    /// join the host-written words (one write), the chain words · the state · the code move up past them.
    /// ★★ 2026-10-14 (2차 B1) 0x800 → 0x900 — the code outgrew 0x5a0 B (unit D, 시간도둑). ★★ 2026-10-15 (2차 F2) → 0xa00 — unit F
    /// (the potion use · the drunk walk) took the code to 0x704 B. ★★ 2026-10-16 (악의 무기) → 0xc00 — unit G (짐승의 박자) and the
    /// beat's checks took the code past 0xa00 (0xab0).
    pub const BLOCK_LEN: u32 = 0xe00;
    pub const BASE_STATS_AT: u32 = 0xc00;
    /// ★★ 2026-10-21 the HP unit (after the base-stats unit): the recompute's HP ([`Self::HP_PEND`] · [`Self::HP_SIG`]) and the
    /// blob reader's post-wrapper.
    pub const VITALS_AT: u32 = 0xcc0;
    /// The saved HP in the character blob (`a.b()[B` @0x152af0: [0] level · [1..2] stat points · [3..4] skill points ·
    /// [5..8] HP `a.a:I`, big-endian — `tools_re/h2save.py` CHAR_HP).
    pub const BLOB_HP_AT: u32 = 5;
    /// The ratio runs only while both maxima are under this (`HP × max` stays in a signed word); past it the HP keeps the
    /// shipped `min`.
    pub const RATIO_MAX: u32 = 0x8000;
    pub const EFFECTS_LEN: u32 = 0x200;
    /// The effect words, offsets from the block (= what the guardian / venom blocks' pointer words hold).
    pub const W_TAKEN: u32 = 0x00;
    pub const W_REFLECT: u32 = 0x04;
    pub const W_SP: u32 = 0x08;
    pub const W_CRIT: u32 = 0x0c;
    pub const W_EVADE: u32 = 0x10;
    pub const W_VENOM: u32 = 0x14;
    pub const W_HEAL_HP: u32 = 0x18;
    pub const W_HEAL_SP: u32 = 0x1c;
    /// Buff frames by id: `BUFF_AT + 4 × (id − BUFF_FIRST)`, ids BUFF_FIRST..BUFF_FIRST + BUFF_IDS.
    pub const BUFF_AT: u32 = 0x20;
    pub const BUFF_FIRST: u8 = 30;
    pub const BUFF_IDS: u8 = 8;
    /// The damage rows: class c's kind k add (u16, /256) at `DMG_AT + 2 × (KINDS × c + k)` — the kinds stop at 17 (`a.p`).
    pub const DMG_AT: u32 = 0x40;
    pub const KINDS: u8 = 24;
    /// ★ 2026-10-09 (3): lifesteal (/256 of a 기본공격 hit) · the beast cooldown factor (/256, 256 = ×1) · the cooldown reset
    /// chance (%) · the 용의 심장 chance (%).
    pub const W_LIFESTEAL: u32 = 0xd0;
    pub const W_BEAST_CD: u32 = 0xd4;
    pub const W_RESET: u32 = 0xd8;
    pub const W_DRAGON: u32 = 0xdc;
    /// ★★ 2026-10-16 악의 무기 (HERO2_DEMON_WEAPON_DRAFT.md) — in the room past the damage rows (0xe0..0x100): the demon flags
    /// (the demon block's CLAW · ARM read them through its GEAR_PTR) · the cut on 지옥칼날 · 어둠심판's SP drain (/256, the cut) ·
    /// the claw's add on a poisoned monster (/256) · 짐승의 박자's window (clock ticks, 0 = not worn) and its 둔기강타 add (/256).
    pub const W_DMN_FLAGS: u32 = 0xe0;
    pub const W_DMN_TOGSP: u32 = 0xe4;
    pub const W_DMN_POISONED: u32 = 0xe8;
    pub const W_BEAT: u32 = 0xec;
    pub const W_BEAT_ADD: u32 = 0xf0;
    pub const W_GRAVITY: u32 = 0xf4;
    /// The flags: M2's four-sided claw · M3's wider claw (the host sets it only while 신의가호 runs) · M4's poisoning claw · M5's
    /// 둔기강타 allowed while 악의 runs.
    pub const FLAG_SPIN: u32 = 1;
    pub const FLAG_WIDE: u32 = 2;
    pub const FLAG_VENOM: u32 = 4;
    pub const FLAG_SLAM: u32 = 8;
    /// The knight's 둔기강타: its kind (`a.o` — the SP · damage wrappers see it) and its quickbar value (`a.d(B)Z` sees it).
    pub const SLAM_KIND: u8 = 5;
    pub const SLAM_VALUE: u8 = 3;
    /// 신의가호's buff id (the knight's `g.a(BI)V(34, …)` — M3's condition, read off the character's buff list).
    pub const BLESSING_BUFF: i8 = 34;
    /// 짐승의 박자: 5 s at the clock's 20 ticks · +50 %.
    pub const BEAT_FRAMES: u8 = 100;
    pub const BEAT_ADD: u8 = 128;
    /// ★★ 2026-10-14 (2차 G): the counter's answer + W_COUNTER/256 and the HP it heals (W_COUNTER_LS/256 of it).
    pub const W_COUNTER: u32 = 0x190;
    pub const W_COUNTER_LS: u32 = 0x194;
    /// The berserker's 반격태세 kind (`bl.a(BILbd;)I` case 8).
    pub const COUNTER_KIND: u8 = 8;
    /// ★★ 2026-10-14 (2차 A — 격분 장비): read by the berserker block through its GEAR_PTR (= this block) — frames a hit · a hit
    /// taken · a kill lengthens 격분 by, its received-damage cut (/256, the cut not the kept), the gauge % the explosion keeps
    /// (0 = it ends 격분 as shipped), the explosion's cooldown (0 = shipped), the gauge % 격분 starts at, its damage add (/256).
    /// The kill is this block's: its `bd.q()V` wrapper calls the berserker block's FGEAR (BSK_EXT) with W_FURY_KILL frames.
    pub const W_FURY_HIT: u32 = 0x198;
    pub const W_FURY_HURT: u32 = 0x19c;
    pub const W_FURY_KILL: u32 = 0x1a0;
    pub const W_FURY_TAKEN: u32 = 0x1a4;
    pub const W_FURY_KEEP: u32 = 0x1a8;
    pub const W_FURY_CD: u32 = 0x1ac;
    pub const W_FURY_START: u32 = 0x1b0;
    pub const W_FURY_DMG: u32 = 0x1b4;
    /// ★★ 2026-10-14 (2차 G2 · G3 · G5): the cut (/256) on a hit taken while 반격태세's stance is up · 반격태세 held ON (1) · the
    /// cut while 도발 runs · a counter's splash on the 3×3 (/256 of its answer). The cuts and the splash are the berserker
    /// block's (its TAKEN function / damage wrapper call FGEAR); the hold is this block's buff wrapper (buff 31).
    pub const W_STANCE_TAKEN: u32 = 0x1b8;
    pub const W_STANCE_HOLD: u32 = 0x1bc;
    pub const W_TAUNT_TAKEN: u32 = 0x1c0;
    pub const W_COUNTER_SPLASH: u32 = 0x1c4;
    /// 반격태세's buff (`bl.a()Z` case 8 → `g.a(BI)V(31, …)`).
    pub const STANCE_ID: u8 = 31;
    /// ★★ 2026-10-14 (2차 B1 시간도둑): the 기본공격 hits a second off takes (0 = none worn).
    pub const W_THIEF: u32 = 0x1c8;
    /// What one proc takes off every running counter — 1 s of the game's 20 ticks.
    pub const THIEF_TICKS: u8 = 20;
    /// ★★ 2026-10-14 (2차 B2) the combo gate's post-wrapper ([`Self::combo`]): a stage's SP gain + W_COMBO_SP/256; and the class's
    /// FIRST skill (kind W_FIRST_KIND = the class's stages) — which pays `a.j`, not `<job>.a(I)I`, so the SP table never sees
    /// it — gets back cost × W_FIRST_CUT >> 8 (its KSP byte: 번개의 호흡's 일섬 SP −20 % is exactly this). 0xff = no class.
    pub const W_COMBO_SP: u32 = 0x1cc;
    pub const W_FIRST_KIND: u32 = 0x1d0;
    pub const W_FIRST_CUT: u32 = 0x1d4;
    /// ★★ 2026-10-15 (2차 C — 악의 장비): read by the demon block's DGEAR through its GEAR_PTR (= this block), five words in a row
    /// from W_DMN_CLAW — the claw's attack add (/256, C1 · C3) · a claw speed source (C1 — 1 = a fifth more frames) · the walk's
    /// quarter (C2 — 1 = a step more every fourth tick) · 악의's frames more (C4) · the frames a kill adds (C5).
    pub const W_DMN_CLAW: u32 = 0x1d8;
    pub const W_DMN_FAST: u32 = 0x1dc;
    pub const W_DMN_RUN: u32 = 0x1e0;
    pub const W_DMN_DUR: u32 = 0x1e4;
    pub const W_DMN_KILL: u32 = 0x1e8;
    /// ★★ 2026-10-15 (2차 C5 풀린 목줄) the cut (/256 — the cut, not the kept) on a hit taken while 악의 runs: the demon block's TAKEN
    /// piece reads it through GEAR_PTR, after 악의's own × 85/100 (0 = none worn).
    pub const W_DMN_TAKEN: u32 = 0x1f8;
    /// ★★ 2026-10-15 (2차 C2 지옥 발굽 → 「이동속도 +25%」, user: *"악의 시 이동속도옵션도 그냥 이동속도 옵션으로"*): the half steps an
    /// extra step pays on the update wrapper's walk while no potion's runs (0 = none worn; 4 = one extra a four = +25 % — the
    /// potion's pays 2, +50 %). Standing aside while 악의's own extra step ran the update before ([`Self::DEMON_RUN`]).
    pub const W_WALK: u32 = 0x1fc;
    /// ★★ 2026-10-15 (2차 E — 범위) the worn range bits — read where the range is decided: the combat block's sweep and wave
    /// constructor (E3 · E4) and the ilpung block's drive (E7), through a pointer word in their blocks
    /// ([`CombatPatch::GEAR_RANGE_PTR`] · [`IlpungPatch::GEAR_RANGE_PTR`]) that the gear install points at THIS word.
    /// (E2 휘두르기 도끼 is gone — 2026-10-15 사용자 *"휘두르기 관련 옵션 아이템은 다 없애도돼"*: the shipped 휘두르기 already hits
    /// all round — `a.b(BB)Z` → `a.e(BBI)Z` per direction, the four sides at the full attack, a diagonal at ⅔, two out at ½ —
    /// and never reaches `bl.h(B)Z(4)`.)
    pub const W_RANGE: u32 = 0x1ec;
    /// E3: 둔기강타 = 신의가호's 24-tile star · E4: 검기 = 신의가호's 27-tile lobe (two tiles deeper) · E7: 일풍격's daggers fly
    /// three tiles. (Bit 1 was E2's.)
    pub const RANGE_STAR: u8 = 2;
    pub const RANGE_LOBE: u8 = 4;
    pub const RANGE_DAGGER3: u8 = 8;
    /// ★★ 2026-10-15 (2차 H1 쌍월) not a range — in this word because the combat block already reads it (`hero2java::h2_iaido_block`).
    /// ★★ 2026-10-17: no longer 「발도 twice」 — 발도's last frame opens a 0.5 s window to cast it again with no cooldown
    /// ([`CombatPatch::TWIN_LEFT`]).
    pub const IAIDO_TWICE: u8 = 0x10;
    /// ★★ 2026-10-15 (2차 H2 검풍) the knight's 기본공격 fires a 검기 (`hero2java::h2_cast_block`) — set with [`Self::W_WIND`].
    pub const RANGE_WIND: u8 = 0x20;
    /// ★★ 2026-10-15 (2차 H3 회오리 송곳니) 일풍격 that kills goes again (the ilpung block's drive) — not a range either.
    pub const RANGE_WHIRL: u8 = 0x40;
    /// ★★ 2026-10-15 (협곡 지하 곡괭이 · 스프링 부츠) the 둔기강타 reach items worn, in W_RANGE's bits from here (0..=REACH_MAX).
    pub const RANGE_REACH_SHIFT: u32 = 8;
    pub const RANGE_REACH_MAX: u32 = 2;
    /// ★★ 2026-10-17 (발도 특화 아이템) the 발도 reach items worn ([`GearEffect::IaidoReach`]), in W_RANGE's bits from here
    /// (0..=IAIDO_REACH_MAX) — the combat block's REACH helper makes the radius ([`CombatPatch::IAIDO_RADIUS`] + them).
    pub const IAIDO_REACH_SHIFT: u32 = 10;
    pub const IAIDO_REACH_MAX: u32 = 2;
    /// ★★ 2026-10-15 (2차 F2 술꾼의 장화) the updates a potion's walk lasts (0 = none worn) — the block's `a.g(B)V` wrapper starts
    /// [`Self::DRUNK_LEFT`] from it when a potion's count went down.
    pub const W_DRUNK: u32 = 0x1f0;
    /// ★★ 2026-10-15 (2차 H2 검풍) the knight's kind-4 (검기) hits × this/256 (0 = not worn) — the knight's damage wrapper; also
    /// the usable wrapper's 「검기 cannot be cast」.
    pub const W_WIND: u32 = 0x1f4;
    /// 검풍's share of a 검기: 30 % (77/256 = 30.1 %).
    pub const WIND_KEEP: u8 = 77;
    /// 10 s at the game's 20 updates a second.
    pub const DRUNK_FRAMES: u16 = 200;
    /// ★★ 2026-10-14 the cut tables, a byte per `(class, kind)` / `(class, index)` — `KSP_AT + KINDS·class + kind` ·
    /// `KCD_AT + KINDS·class + index`: the answer − answer × cut >> 8 (0 = as it is).
    pub const KSP_AT: u32 = 0x100;
    pub const KCD_AT: u32 = 0x148;
    /// The chain words: SP ×3 · damage ×3 · 철괴's hook · the buff add · the stat recompute · the cooldown start · the kill ·
    /// ★★ 2026-10-14 the cooldown tables ×3 · ★★ (2차 B2) the combo gate · ★★ (2차 F2) the item use `a.g(B)V` · the character's
    /// update `a.g()V` · ★★ 2026-10-21 the character blob's reader `a.a([B)V`.
    pub const CHAIN_AT: u32 = 0x200;
    pub const CHAINS: u32 = 19;
    pub const CHAIN_SP: u32 = Self::CHAIN_AT;
    pub const CHAIN_DMG: u32 = Self::CHAIN_AT + 12;
    pub const CHAIN_HOOK: u32 = Self::CHAIN_AT + 24;
    pub const CHAIN_BUFF: u32 = Self::CHAIN_AT + 28;
    pub const CHAIN_STATS: u32 = Self::CHAIN_AT + 32;
    pub const CHAIN_CD: u32 = Self::CHAIN_AT + 36;
    pub const CHAIN_KILL: u32 = Self::CHAIN_AT + 40;
    pub const CHAIN_CDG: u32 = Self::CHAIN_AT + 44;
    pub const CHAIN_COMBO: u32 = Self::CHAIN_AT + 56;
    pub const CHAIN_USE: u32 = Self::CHAIN_AT + 60;
    pub const CHAIN_UPDATE: u32 = Self::CHAIN_AT + 64;
    /// ★★ 2026-10-15 (2차 H2) the knight's usable check `bx.b(B)Z`.
    pub const CHAIN_USABLE: u32 = Self::CHAIN_AT + 68;
    /// ★★ 2026-10-21 the character blob's reader `a.a([B)V` (the 일풍격 block's wrapper, or the body).
    pub const CHAIN_LOAD: u32 = Self::CHAIN_AT + 72;
    /// The block's own state past the chains (★★ 2026-10-15 three words later — three more chains · ★★ 2026-10-21 one more —
    /// the loader's): the cooldown clock's COUNT address
    /// (host-written at the install, 0 = no clock — the 용의 심장 buff is then off) · the clock count the buff runs until · the
    /// resets done (the host clears its parked cooldowns when this moves).
    pub const CLOCK_PTR: u32 = 0x24c;
    pub const DRAGON_UNTIL: u32 = 0x250;
    pub const RESETS: u32 = 0x254;
    /// ★★ 2026-10-14 the berserker block's FGEAR entry (`| 1`, written at the install; 0 = no berserker block — a kill then
    /// lengthens nothing).
    pub const BSK_EXT: u32 = 0x258;
    /// ★★ 2026-10-14 (2차 B1) the 기본공격 hits counted toward the next second off · the seconds taken so far (the host takes
    /// as many off its parked cooldowns when this moves).
    pub const THIEF_HITS: u32 = 0x25c;
    pub const THIEF_SECS: u32 = 0x260;
    /// ★★ 2026-10-15 (2차 F2) the drunk walk's updates left (the update wrapper counts it down; odd = an extra step). The demon
    /// block's DGEAR reads it through its GEAR_PTR: while it runs, 악의's own extra step (돌진 · 추종 · C2) stands aside — one
    /// +50 % at a time.
    pub const DRUNK_LEFT: u32 = 0x264;
    /// ★★ 2026-10-15 (2차 F2) the drunk walk's half steps owed: +1 an update while walking (0 when not), an extra step pays 2
    /// and runs only BETWEEN two tiles (it finishes the half-tile the game's own step began — every tile the walker enters,
    /// every turn and the corner assist stay the game's; an extra step on a tile ran a tile past the assist's turn). So 6
    /// steps in 4 updates = +50 %. At most 3 owed.
    pub const DRUNK_DEBT: u32 = 0x268;
    /// ★★ 2026-10-15 (2차 C2) the demon block's DGEAR writes its OP_RUN answer here (≠ 0 = 악의's own extra step — 돌진 · 추종 —
    /// runs this update); the update wrapper's W_WALK step reads it and clears it — one extra step source at a time.
    pub const DEMON_RUN: u32 = 0x26c;
    /// ★★ 2026-10-16 (M5 짐승의 박자) the clock tick the window closes at (0 = shut — a beast skill's cooldown start opens it) · 1
    /// while the 둔기강타 that spent it runs (its hits get W_BEAT_ADD; the next 둔기강타 without a window puts it back to 0).
    pub const BEAT_UNTIL: u32 = 0x270;
    pub const BEAT_ON: u32 = 0x274;
    /// ★★ 2026-10-16 (M1) every hit any class lands (the damage wrappers' tail, unit D) — the host counts 격분's from it.
    pub const HIT_COUNT: u32 = 0x278;
    /// ★★ 2026-10-21 (user: *"HP가 늘어나는 아이템을 끼면 현재 체력도 그만큼 같이 늘어나야해"*) the HP's state, four words: the
    /// saved HP the loader's post-wrapper read off the blob · the character it waits for (0 = none) · the HP as the loader /
    /// the last recompute left it (a recompute that finds the HP moved since — a hit, a potion, the regen — drops the wait:
    /// a load's HP never comes back over a later change) · the equipment's signature at the last recompute (0 = no
    /// equipment array — a load before `a.n()V`; a change only carries the HP from one array to another).
    pub const HP_PEND: u32 = 0x27c;
    pub const HP_PEND_OWNER: u32 = 0x280;
    pub const HP_PEND_LAST: u32 = 0x284;
    pub const HP_SIG: u32 = 0x288;
    /// The code (and its table and pool) from here (★★ 2026-10-15 0x260 → 0x270, the debt word · ★★ 2026-10-16 → 0x280, the beat ·
    /// the hit count · ★★ 2026-10-21 → 0x290, the loader's chain and the HP's state; 16-aligned).
    pub const CODE_AT: u32 = 0x290;
    /// 용의 심장's buff: 60 s at the game's 20 ticks · received damage × 230/256 (−10 %).
    pub const DRAGON_FRAMES: u32 = 1200;
    pub const DRAGON_MUL: u8 = 230;
    /// The HUD's buff cell for it (`hero2java::H2BuffShow`): its own id (no game buff has it) and the host icon sheet's cell.
    pub const DRAGON_BUFF_ID: u8 = 200;
    /// The crit site's halfwords: `add r0,sl` → `blx r0`, the `bl 0x1680a4` pair → `nop nop`.
    pub const CRIT_BL: [u16; 2] = [0xf040, 0xfa2a];
    /// The equipment slots read: all seven (0 장갑 · 1 투구 · 2 장신구 · 3 무기 · 4 갑옷 · 5 방패/보조 · 6 신발).
    pub const SLOTS: usize = 7;
    /// 기본공격 stages per class (`a.i:[B` = 4 · 3 · 5).
    pub const STAGES: [u8; 3] = [4, 3, 5];
    /// The character's HP · SP and their maxima (absolute: `a.a:I` · `a.b:I` · `a.d:I` · `a.e:I`).
    pub const CHAR_HP_OFF: u32 = 0x6c;
    pub const CHAR_SP_OFF: u32 = 0x70;
    pub const CHAR_HP_MAX_OFF: u32 = 0x78;
    pub const CHAR_SP_MAX_OFF: u32 = 0x7c;
    /// The character's equipment array (`Hero2Java::CHAR_EQUIP_OFF`), absolute.
    pub const CHAR_EQUIP_OFF: u32 = 0x12c;

    /// ⚠ All Claude's draft except the user's (오딘 기본공격 +50 % · 제스 폭풍강타 +50 % · 파멸 방패치기 +50 % · 응보 살상연희 +50 % ·
    /// 방패 하나 방패치기 +50 % · 서약 최대 HP +10 % — and ★ 2026-10-09 (3): 이안 검기 +20 % / 일격·일섬 +30 % · 파멸 + 급소가격·극사
    /// +20 % · 파쇄자 폭풍강타·둔기강타 +25 % · 독사 기본공격 +50 % · 산맥 야수 쿨 −20 % · 진군 기본공격 피해의 10 % HP · 그림자 일격·일섬
    /// +100 % · 비호 기본공격 피해의 20 % HP · 용의 심장 + 처치 1 % 「1 분 받는 피해 −10 %」 · 현자 + 타격 1 % 쿨타임 초기화 — and (4) the balance
    /// §6 「버서커 90 대 NORMAL 진다」 → user *"방어구 받는 피해 옵션 상향"*: 용암군주의 투구 −5 → −15 % · 심연의 판금갑옷 −8 → −25 %). Order: weapons, armour (by type: heavy · light · heavy · light), shields,
    /// accessories. Kinds: 버서커 기본 0..=3 · 가르기 6 · 폭풍강타 9 / 나이트 기본 0..=2 · 검기 4 · 둔기강타 5 · 방패치기 6 /
    /// 로그 기본 0..=4 · 급소가격 7 · 살상연희 10. Buffs: 맹독감염 33 · 신의가호 34 · 철괴 35.
    pub const ITEMS: [GearItem; 89] = {
        use GearEffect::*;
        [
            GearItem { item_type: 0, id: 33, text: "기본공격 피해 +50%", effects: &[Dmg(0, 0, 3, 128)] },
            GearItem { item_type: 0, id: 34, text: "처치 시 HP 5% 회복", effects: &[HealHp(13)] },
            // ★★ 2026-10-20 LV 85 상향평준화 (user: *"효과가 거의없는것들이나 같은부위의 상위호환에 밀리는 것들만 상향해줘"* · *"권장으로 수정할만한건
            //    다 수정해줘"*) — the weak or dominated rows below got their numbers raised or a second option; numbers Claude's.
            GearItem { item_type: 1, id: 28, text: "받는 피해 -15%, 최대 HP +15%", effects: &[Taken(218), MaxHp(38)] }, // ★★ 2026-10-20 was 받는 피해 -8 %
            GearItem { item_type: 1, id: 29, text: "폭풍강타 피해 +100%", effects: &[Dmg(0, 9, 9, 256)] }, // ★ 2026-10-20 +50 → +100 % (밸런스 전수 조사 ④)
            // ★★ 2026-10-14 (user: *"도검에 일격,일섬 데미지 붙어있는 도검의 일섬 데미지를 더 늘려야겠네"*): 일섬 (= kind 5 with 벽력일섬 on)
            // +30 % more → +60 % — 번개의 호흡 (일섬 +30 %) is the dagger of the set; this sword stays the 일섬 damage piece.
            // ★★ 2026-10-15 (user: *"일격 피해는 30%고 일섬 피해는 +60%인데 이거 일격, 일섬 피해 +60%으로 고쳐야해"*): 일격 too — one row.
            GearItem { item_type: 2, id: 28, text: "검기 피해 +40%, 일격·일섬 피해 +60%", effects: &[Dmg(1, 4, 4, 102), Dmg(2, 5, 5, 154)] }, // ★★ 2026-10-20 검기 +20 → +40 %
            GearItem { item_type: 2, id: 29, text: "방패치기 피해 +50%, 급소가격·극사 피해 +20%", effects: &[Dmg(1, 6, 6, 128), Dmg(2, 7, 7, 51)] },
            GearItem { item_type: 3, id: 33, text: "맹독감염 중독 피해 +50%", effects: &[Venom(128)] },
            GearItem { item_type: 3, id: 34, text: "살상연희 피해 +50%", effects: &[Dmg(2, 10, 10, 128)] },
            // ★★ 2026-10-14 번개의 호흡 (user: *"옵션은 일섬 데미지 30%증가, 일섬 쿨타임 -20%, 일섬 sp소모량 -20% 감소야. 벽력일섬과 세트"*):
            // all three only with 벽력일섬 (12/19) worn — without it kind 5 is 일격; the lightning is the accessory block's.
            GearItem { item_type: 3, id: 39, text: "일섬 피해 +30%, 일섬 쿨타임 -20%, 일섬 SP 소모 -20%", effects: &[With(12, 19, &[Dmg(2, 5, 5, 77), KindCd(2, 0, 51), KindSp(2, 5, 51)])] },
            GearItem { item_type: 4, id: 33, text: "둔기강타 피해 +20%", effects: &[Dmg(1, 5, 5, 51)] },
            GearItem { item_type: 4, id: 34, text: "받는 피해 -10%, 받는 피해의 20% 반사", effects: &[Taken(230), Reflect(51)] }, // ★★ 2026-10-20 was 반사 10 %
            GearItem { item_type: 5, id: 28, text: "받는 피해 -15%", effects: &[Taken(218)] },
            GearItem { item_type: 5, id: 29, text: "치명 +10%p, 기본공격 피해 +20%", effects: &[Crit(10), Basic(51)] }, // ★★ 2026-10-20 was 치명 +5 %p
            GearItem { item_type: 5, id: 30, text: "최대 HP +15%, 받는 피해의 10% 반사", effects: &[MaxHp(38), Reflect(26)] }, // ★★ 2026-10-20 was 반사 5 %
            GearItem { item_type: 5, id: 31, text: "스킬 SP 소모 -10%, 스킬 쿨타임 -10%", effects: &[SpCost(230), AllCd(26)] }, // ★★ 2026-10-20 + 쿨타임 (집중의 관 SP -30 % 와 갈림)
            GearItem { item_type: 6, id: 51, text: "최대 HP +40%, 받는 피해 -15%", effects: &[MaxHp(102), Taken(218)] }, // ★★ 2026-10-20 + 받는 피해 (심연의 판금갑옷과 유효 HP 비슷하게)
            GearItem { item_type: 6, id: 52, text: "회피 +5%p, 민첩 +50", effects: &[Evade(5), Ability(2, 50)] },
            GearItem { item_type: 6, id: 53, text: "받는 피해 -25%, 최대 HP +20%", effects: &[Taken(192), MaxHp(51)] },
            GearItem { item_type: 6, id: 54, text: "최대 SP +10%, 의지 +50", effects: &[MaxSp(26), Ability(3, 50)] },
            GearItem { item_type: 8, id: 51, text: "기본공격 피해 +35%, 받는 피해 -8%", effects: &[Basic(90), Taken(235)] }, // ★★ 2026-10-20 was 기본공격 +15 %
            GearItem { item_type: 8, id: 52, text: "기본공격 피해 +50%", effects: &[Basic(128)] },
            GearItem { item_type: 8, id: 53, text: "폭풍강타·둔기강타 피해 +50%", effects: &[Dmg(0, 9, 9, 128), Dmg(1, 5, 5, 128)] },
            GearItem { item_type: 8, id: 54, text: "처치 시 SP 10% 회복, 스킬 SP 소모 -20%", effects: &[HealSp(26), SpCost(205)] }, // ★★ 2026-10-20 was SP 5 %
            GearItem { item_type: 9, id: 51, text: "야수 스킬 쿨타임 -20%", effects: &[BeastCd(205)] },
            GearItem { item_type: 9, id: 52, text: "일격·일섬 피해 +100%", effects: &[Dmg(2, 5, 5, 256)] },
            GearItem { item_type: 9, id: 53, text: "기본공격 피해의 15% HP 회복, 받는 피해 -8%", effects: &[Lifesteal(38), Taken(235)] }, // ★★ 2026-10-20 was 10 %
            GearItem { item_type: 9, id: 54, text: "기본공격 피해의 20% HP 회복", effects: &[Lifesteal(51)] },
            GearItem { item_type: 10, id: 28, text: "최대 HP +20%, 방어확률 +10%P", effects: &[MaxHp(51), Guard(10)] }, // ★★ 2026-10-20 was 최대 HP +5 %
            GearItem { item_type: 10, id: 29, text: "방패치기 피해 +50%", effects: &[Dmg(1, 6, 6, 128)] },
            GearItem { item_type: 11, id: 20, text: "받는 피해 -5%, 처치 시 1% 확률로 1분간 받는 피해 -10%", effects: &[Taken(243), DragonProc(1)] },
            GearItem { item_type: 12, id: 20, text: "스킬 SP 소모 -15%, 타격 시 1% 확률로 모든 스킬 쿨타임 초기화", effects: &[SpCost(218), ResetProc(1)] },
            GearItem { item_type: 12, id: 21, text: "최대 HP +10%", effects: &[MaxHp(26)] },
            // ── ★★ 2026-10-14 신규 아이템 2차 (HERO2_NEW_ITEM_DRAFT.md §3, tools_re/skill_gear.py) — 착수 3: the options the table
            //    alone can carry. The rest of each item (살상연희 타수 · 격분 · 반격태세 유지 · 범위 · 속도 …) comes with its own piece. ──
            GearItem { item_type: 0, id: 36, text: "반격 피해 +80%", effects: &[Counter(205)] }, // G1 응징의 대검
            // ★★ 2026-10-15 (user: *"백번째 칼날은 살상연희 피해 +30%로 수정해줘. 그리고 살상연희 타수 +5 도 설명에 추가"*) — +50 → +30 %.
            GearItem { item_type: 3, id: 35, text: "살상연희 타수 +5, 살상연희 피해 +30%", effects: &[Dmg(2, 10, 10, 77)] }, // D1 (타수 +5: the rogue block)
            // ★★ 2026-10-20 D3 피의 연쇄 joins the table: 급소가격 · 극사 (the rogue's kind 7) +80 %; its SP on a full-HP one-shot stays the
            //    accessory block's (GK) — that part is no word of ours
            GearItem { item_type: 3, id: 37, text: "급소가격·극사 피해 +80%, 극사로 HP가 가득한 적을 한 방에 처치시 SP 5% 회복", effects: &[Dmg(2, 7, 7, 205)] }, // D3 피의 연쇄
            // ★★ 2026-10-15 (user: *"둔기강타 둔기랑 심판의 철퇴 옵션이 겹치는데 … 이름을 협곡 지하 곡괭이로 … 데미지+100% 옵션을 둔기강타 범위
            // +50%, 둔기강타 쿨타임 -20% 옵션으로"* · 「범위 24칸」 없앰): 둔기강타 is the knight's kind 5 — cooldown index 2 (5 − 3 stages).
            GearItem { item_type: 4, id: 35, text: "둔기강타 범위 +50%, 둔기강타 쿨타임 -20%", effects: &[BluntReach(50), KindCd(1, 2, 51)] }, // E3 협곡 지하 곡괭이
            GearItem { item_type: 5, id: 34, text: "스킬 SP 소모 -30%", effects: &[SpCost(179)] }, // B3 집중의 관
            // ★★ 2026-10-20 무희 셋은 +20 % 그대로 — +40/50/40 으로 올렸다가 되돌렸다: probe 실게임에서 살상연희는 한 마리 19 타(무리 27 타)라
            //    세트 단일 DPS 가 8,618 → 10,447 (+21 %), 원래도 로그 최강 세팅이었다(사용자 결정 「원래대로」)
            GearItem { item_type: 5, id: 36, text: "살상연희 피해 +20%", effects: &[Dmg(2, 10, 10, 51)] }, // D4 무희의 두건
            GearItem { item_type: 6, id: 57, text: "스킬 쿨타임 -15% (각성기 포함)", effects: &[AllCd(38)] }, // B4 시간의 외투
            GearItem { item_type: 6, id: 59, text: "살상연희 피해 +20%", effects: &[Dmg(2, 10, 10, 51)] }, // D5 무희의 옷
            GearItem { item_type: 8, id: 55, text: "반격 피해 +30%, 반격 피해의 6% HP 회복", effects: &[Counter(77), CounterLeech(15)] }, // G4 되갚는 손
            GearItem { item_type: 8, id: 56, text: "직업 스킬 피해 +20%", effects: &[Skills(51)] }, // B5 연환의 장갑
            GearItem { item_type: 8, id: 58, text: "살상연희 피해 +40%", effects: &[Dmg(2, 10, 10, 102)] }, // D6 칼춤 장갑 (★★ 2026-10-16 +20 → +40 % — 연환의 장갑(직업 스킬 +20 %)이 같은 장갑 칸에서 살상연희까지 덮었다)
            GearItem { item_type: 8, id: 59, text: "검기 피해 +30%, 검기 SP -50%, 사거리 +2칸", effects: &[Dmg(1, 4, 4, 77), KindSp(1, 4, 128), Range(GearPatch::RANGE_LOBE)] }, // E4 소드마스터의수련장갑 (★★ 2026-10-15 이름)
            GearItem { item_type: 9, id: 59, text: "살상연희 피해 +20%", effects: &[Dmg(2, 10, 10, 51)] }, // D7 잔영의 신
            GearItem { item_type: 9, id: 60, text: "일풍격 피해 +30%, 범위 3칸", effects: &[Dmg(2, 15, 15, 77), Range(GearPatch::RANGE_DAGGER3)] }, // E7 바람날개 신발 (★★ 2026-10-17 이름 — 일풍격 신발)
            // ★★ 2026-10-17 쌍월 reworked (user: *"발도 사용시 0.5초내로 재사용하면 쿨타임 없이 재사용가능하고, 재사용 한뒤에 기존 발도
            //    쿨타임이 적용"*) — the same bit, the combat block's window instead of the replay
            // ★★ 2026-10-17 (user: *"쌍월도 아이템 설명 문구 수정해줘. 짧은 시간내에 발도 두번 사용가능 으로"*) — the text only
            GearItem { item_type: 2, id: 30, text: "짧은 시간내에 발도 두번 사용가능", effects: &[Range(GearPatch::IAIDO_TWICE)] }, // H1 쌍월
            GearItem { item_type: 2, id: 31, text: "기본공격마다 검기, 피해 30%, 나이트만", effects: &[Wind(GearPatch::WIND_KEEP)] }, // H2 검풍
            GearItem { item_type: 3, id: 38, text: "일풍격으로 처치 시 재시전 (최대 3번)", effects: &[Range(GearPatch::RANGE_WHIRL)] }, // H3 회오리 송곳니
            GearItem { item_type: 11, id: 21, text: "반격 피해의 6% HP 회복", effects: &[CounterLeech(15)] }, // G6 피의 서약
            // ── 2차 A 격분 무한 (the berserker block's FGEAR; 0.05 s = 1 frame at the game's 20). ⚠ «50 %» = the gauge at 50 and
            //    HITS = 12, so the next hit shows 52 (the gauge is HITS × 4) ──
            GearItem { item_type: 0, id: 35, text: "격분 시 게이지 50%, 격분 중 피해 +60%", effects: &[FuryStart(50), FuryDmg(154)] }, // A5′ 멈추지 않는 대검
            GearItem { item_type: 1, id: 30, text: "격분 시 게이지 50%, 격분 중 피해 +60%", effects: &[FuryStart(50), FuryDmg(154)] }, // A5 멈추지 않는 도끼
            GearItem { item_type: 5, id: 32, text: "격분 중 타격마다 격분 +0.25초", effects: &[FuryHit(5)] }, // A1 분노의 투구
            GearItem { item_type: 6, id: 55, text: "격분 중 처치 +1초, 맞으면 +0.3초, 받는 피해 -15%", effects: &[FuryKill(20), FuryHurt(6), FuryTaken(218)] }, // A3 상처의 갑주
            GearItem { item_type: 9, id: 55, text: "격분 중 타격마다 격분 +0.15초", effects: &[FuryHit(3)] }, // A2 광전사의 발걸음
            // ★★ 2026-10-20 50 % · 20 s → 90 % · 10 s (the model had it a loss in its own 격분 build: the re-ramp from 50 % cost more than the blasts gave)
            GearItem { item_type: 12, id: 22, text: "폭발 뒤 격분 유지, 게이지 90% 남음, 폭발 쿨 10초", effects: &[FuryKeep(90), FuryCd(200)] }, // A4 꺼지지 않는 불씨
            // ── 2차 G 반격 무한 — the rest (G1 · G4 · G6 above) ──
            GearItem { item_type: 5, id: 33, text: "반격태세 유지, 도발 중 받는 피해 -10%", effects: &[StanceHold, TauntTaken(230)] }, // G3 도발자의 투구
            GearItem { item_type: 6, id: 56, text: "반격태세 중 받는 피해 -25%", effects: &[StanceTaken(192)] }, // G2 가시 판금
            GearItem { item_type: 9, id: 56, text: "반격이 주변 적에게 25% 퍼진다", effects: &[CounterSplash(64)] }, // G5 울림의 장화
            // ── 2차 B1 · B2 (★★ B2 = the combo gate `a.f()Z`'s post-wrapper — its definition 0x18b908 measured on Windows) ──
            GearItem { item_type: 12, id: 23, text: "기본공격 10회마다 모든 스킬 쿨 -1초 (각성기, 야수 포함)", effects: &[TimeThief(10)] }, // B1 시간도둑
            GearItem { item_type: 9, id: 57, text: "기본공격 SP 획득 +50%", effects: &[ComboSp(128)] }, // B2 흐름의 장화
            // ── ★★ 2026-10-15 2차 C 악의 (the demon block's DGEAR — 60 s = 1200 frames, a kill 20 = 1 s) ──
            // ★★ 2026-10-15 (user: *"악의중 할퀴기 말고 기본공격 데미지 증가로"* · *"악의 시 공격속도 옵션도 기본 공격 속도, 악의 시 이동속도옵션도
            // 그냥 이동속도"*): C1 · C2 · C3 hold all the time — 기본공격 is the claw while 악의 runs, so it gets them there too.
            GearItem { item_type: 8, id: 57, text: "기본공격 속도 +20%, 기본공격 피해 +20%", effects: &[BasicFast(20), Basic(51)] }, // C1 악마의 손톱
            GearItem { item_type: 9, id: 58, text: "이동속도 +25%", effects: &[Walk(25)] }, // C2 지옥 발굽
            GearItem { item_type: 6, id: 58, text: "기본공격 피해 +30%", effects: &[Basic(77)] }, // C3 검은 비늘
            GearItem { item_type: 5, id: 35, text: "악의 지속 60초 -> 75초", effects: &[DemonDur(15)] }, // C4 뒤틀린 뿔
            // ★★ 2026-10-15 (user: *"풀린목줄 … 악의 시 받는피해 -10%도 추가해줘"*) — on top of 악의's own −15 % (DemonPatch::TAKEN_NUM).
            // ★★ 2026-10-17 (user: *"풀린목줄도 처치시 현재 +1초고 최대 75초인데 이거 최대 제한 없애줘"*) — the demon block's DGEAR has no ceiling now
            GearItem { item_type: 11, id: 22, text: "악의 중 받는 피해 -10%, 처치마다 +1초", effects: &[DemonTaken(230), DemonKill(20)] }, // C5 풀린 목줄
            // ── ★★ 2026-10-15 2차 F1 (the host counts the bag's equipment) ──
            // ★★ 2026-10-17 (user: *"중력의 갑주 옵션을 상향 … +0.4%가 아니라 1%"*): 4‰ → 10‰
            // ★★ 2026-10-20 (user: *"중력의 갑주만 장비 1개당 +0.8%로"*): 10‰ → 8‰ · ★★ 2026-10-21 (user: 0.6 %): → 6‰ (no ceiling — a full 127-cell bag = +76.2 %)
            // ★ 2026-10-23 (user): the drop-table site's wording — 「장비」 → 「인벤토리 내의 장비」 (the in-game 3 lines in skill_gear.py already say 「(가방 + 입은 장비)」)
            GearItem { item_type: 6, id: 60, text: "인벤토리 내의 장비 1개당 주는 피해 +0.6%", effects: &[Gravity(6)] }, // F1 중력의 갑주
            GearItem { item_type: 9, id: 61, text: "포션을 마시면 10초간 이동속도 +50%", effects: &[Drunk(GearPatch::DRUNK_FRAMES)] }, // F2 술꾼의 장화
            // ★★ 2026-10-15 (user: *"스프링 부츠라는 중갑 신발 하나 만들고 둔기강타 범위 +50% 옵션"* — 곡괭이와 둘이면 +100 %)
            GearItem { item_type: 9, id: 62, text: "둔기강타 범위 +50%, 둔기강타 피해 +30%", effects: &[BluntReach(50), Dmg(1, 5, 5, 77)] }, // E8 스프링 부츠 (★★ 2026-10-20 + 피해 +30 %)
            // ── ★★ 2026-10-16 악의 무기 다섯 (HERO2_DEMON_WEAPON_DRAFT.md · user: *"짐승의박자로 결정하고 아이템 5개 추가로 진행하자"*) —
            //    numbers Claude's (the draft's), the class-bound ones count for their class only ──
            GearItem { item_type: 0, id: 37, text: "할퀴기마다 다음 격분 게이지 +4% (최대 100%), 격분 중 타격마다 악의 쿨타임 -0.1초", effects: &[ClawBank(4), FuryMaliceCd(2)] }, // M1 광마의 대검
            GearItem { item_type: 1, id: 31, text: "할퀴기 1·2단이 사방을 할퀸다, 악의 중 처치 시 HP 3% 회복", effects: &[ClawSpin, DemonKillHeal(8)] }, // M2 피바람 도끼
            GearItem { item_type: 2, id: 32, text: "신의가호 중 할퀴기 범위 +1칸, 지옥칼날·어둠심판 SP 소모 -40%", effects: &[ClawWideBlessed, ToggleSp(154)] }, // M3 타락한 성검
            GearItem { item_type: 3, id: 40, text: "악의 중 할퀴기가 맹독을 건다, 중독된 적에게 할퀴기 피해 +25%", effects: &[ClawVenom, ClawPoisoned(64)] }, // M4 악몽의 독니
            // ★★ 2026-10-17 (user: *"계약의 철퇴 옵션 수정하자 악의중 둔기강타 사용가능, 둔기강타 데미지 10%증가로"*): 짐승의 박자 (Beat) is no
            //    item's now — the mechanism stays (unit G · BEAT_UNTIL / BEAT_ON), as GEAR_NO_INVULN did; +10 % = 26/256 on 둔기강타 (kind 5)
            GearItem { item_type: 4, id: 36, text: "악의 중에도 둔기강타 사용, 둔기강타 피해 +10%", effects: &[DemonSlam, Dmg(1, 5, 5, 26)] }, // M5 계약의 철퇴
            // ── ★★ 2026-10-17 발도 특화 여섯 (user: *"발도의 범위를 늘려주는 아이템과 데미지 증가, sp와 쿨타임을 감소시켜 주는 아이템류
            //    두개로 나눠서 … 도검류 2종류와 방어구 장갑 2개, 투구 1개(데미지)"* + 「갑옷 1개 추가 (SP·쿨)」 · 「보통」) — 범위·피해 =
            //    도검 · 장갑 · 투구, SP·쿨 = 도검 · 장갑 · 갑옷; names Claude's. 발도 = the knight's kind 9 · cooldown index 6 ──
            GearItem { item_type: 2, id: 33, text: "발도 범위 +1칸, 발도 피해 +30%", effects: &[IaidoReach(1), Dmg(1, 9, 9, 77)] }, // J1 회천도
            GearItem { item_type: 8, id: 60, text: "발도 범위 +1칸, 발도 피해 +30%", effects: &[IaidoReach(1), Dmg(1, 9, 9, 77)] }, // J2 원월의 장갑
            GearItem { item_type: 5, id: 37, text: "발도 피해 +50%", effects: &[Dmg(1, 9, 9, 128)] }, // J3 검귀의 투구
            GearItem { item_type: 2, id: 34, text: "발도 SP 소모 -30%, 발도 쿨타임 -30%", effects: &[KindSp(1, 9, 77), KindCd(1, 6, 77)] }, // J4 찰나의 검
            GearItem { item_type: 8, id: 61, text: "발도 SP 소모 -20%, 발도 쿨타임 -20%", effects: &[KindSp(1, 9, 51), KindCd(1, 6, 51)] }, // J5 납도의 장갑
            GearItem { item_type: 6, id: 61, text: "발도 SP 소모 -20%, 발도 쿨타임 -20%", effects: &[KindSp(1, 9, 51), KindCd(1, 6, 51)] }, // J6 납도의 갑옷
            GearItem { item_type: 5, id: 38, text: "검기 피해 +50%", effects: &[Dmg(1, 4, 4, 128)] },
            GearItem { item_type: 5, id: 39, text: "방패치기 피해 +50%, 최대 HP +20%, 방어확률 +8%P", effects: &[Dmg(1, 6, 6, 128), MaxHp(51), Guard(8)] },
            GearItem { item_type: 6, id: 62, text: "검기 피해 +70%", effects: &[Dmg(1, 4, 4, 179)] },
            GearItem { item_type: 6, id: 63, text: "발도 피해 +70%", effects: &[Dmg(1, 9, 9, 179)] },
            GearItem { item_type: 6, id: 64, text: "방패치기 피해 +70%, 최대 HP +30%, 방어확률 +12%P", effects: &[Dmg(1, 6, 6, 179), MaxHp(77), Guard(12)] },
            GearItem { item_type: 9, id: 63, text: "검기 피해 +40%", effects: &[Dmg(1, 4, 4, 102)] },
            GearItem { item_type: 9, id: 64, text: "발도 피해 +40%", effects: &[Dmg(1, 9, 9, 102)] },
            GearItem { item_type: 9, id: 65, text: "방패치기 피해 +40%, 최대 HP +15%, 방어확률 +6%P", effects: &[Dmg(1, 6, 6, 102), MaxHp(38), Guard(6)] },
            GearItem { item_type: 8, id: 62, text: "방패치기 피해 +30%, 최대 HP +10%, 방어확률 +4%P", effects: &[Dmg(1, 6, 6, 77), MaxHp(26), Guard(4)] },
        ]
    };

    /// The item a worn `(type, id)` is, if it is one of ours.
    pub fn item(item_type: u8, id: u8) -> Option<&'static GearItem> {
        Self::ITEMS.iter().find(|g| g.item_type == item_type && g.id == id)
    }

    /// The effect words for the worn `(type, id)`s and the class (0..=2; anything else = no class-bound effect): ×1 /
    /// zero everywhere for nothing of ours.
    pub fn effects_for(worn: &[(u8, u8)], class: u8) -> [u8; Self::EFFECTS_LEN as usize] {
        Self::effects_for_counted(worn, class, 0)
    }

    /// ★★ 2026-10-15 does a worn item want the bag counted ([`GearEffect::Gravity`])? — the host walks the 127 cells only then.
    pub fn wants_count(worn: &[(u8, u8)]) -> bool {
        worn.iter().any(|&(t, i)| Self::item(t, i).is_some_and(|g| g.effects.iter().any(|e| matches!(e, GearEffect::Gravity(_)))))
    }

    /// The highest equipment item type (weapons 0..4 · armour 5..10 · accessories 11 · 12) — what [`GearEffect::Gravity`] counts.
    pub const EQUIPMENT_LAST: i8 = 12;

    /// [`Self::effects_for`] with `counted` equipment records in the bag (2차 F1): Σ Gravity ‰ × counted onto the class's
    /// skill and stage rows (/256, rounded), the awakening not.
    pub fn effects_for_counted(worn: &[(u8, u8)], class: u8, counted: u32) -> [u8; Self::EFFECTS_LEN as usize] {
        Self::effects_for_live(worn, class, counted, GearLive::default())
    }

    /// ★★ 2026-10-16 (악의 무기) the worn items' host-only shares: `(the 격분 gauge % a landed claw banks, the ticks a fury hit takes off
    /// 악의's cooldown)` — the host's counters ([`GearEffect::ClawBank`] · [`GearEffect::FuryMaliceCd`]); `(0, 0)` = none worn.
    pub fn live_shares(worn: &[(u8, u8)], class: u8) -> (u8, u8) {
        let mut acc = GearAcc::new();
        for &(t, i) in worn {
            let Some(item) = Self::item(t, i) else { continue };
            for e in item.effects {
                acc.apply(e, worn, class);
            }
        }
        (acc.claw_bank.min(100) as u8, acc.fury_malice_cd.min(255) as u8)
    }

    /// [`Self::effects_for_counted`] with what the host knows this poll ([`GearLive`]): M2's kill heal joins W_HEAL_HP only while
    /// 악의 runs, M3's wider claw is flagged only while 신의가호 runs, and M1's bank lifts W_FURY_START (the largest of it and a
    /// worn start) while the sword is worn.
    pub fn effects_for_live(worn: &[(u8, u8)], class: u8, counted: u32, live: GearLive) -> [u8; Self::EFFECTS_LEN as usize] {
        let mut acc = GearAcc::new();
        for &(t, i) in worn {
            let Some(item) = Self::item(t, i) else { continue };
            for e in item.effects {
                acc.apply(e, worn, class);
            }
        }
        if !live.blessed {
            acc.dmn_flags &= !Self::FLAG_WIDE;
        }
        if live.demon_on {
            acc.heal_hp += acc.demon_kill_heal;
        }
        if acc.claw_bank > 0 {
            acc.fury_start = acc.fury_start.max(u32::from(live.fury_bank.min(100)));
        }
        // ★★ 2026-10-15 (2차 H2 검풍, user: *"검풍으로 발생하는 검기는 기본공격 +x% 옵션이 적용되어야해 … 검기 데미지가 100이라면 검풍을 끼면 …
        // 30짜리 검기 … 기본공격 +50%옵션 아이템을 착용하면 검기의 데미지가 45"*): while 검풍 is worn every 검기 is a 기본공격's (casting it
        // is refused), so the knight's 기본공격 add (its stage-0 row — Basic · stage Dmg) joins kind 4's
        // row — the wrapper then takes × W_WIND: (100 + 50) × 30 % = 45.
        if acc.wind != 0 {
            acc.dmg[1][usize::from(CombatPatch::WAVE)] = acc.dmg[1][usize::from(CombatPatch::WAVE)].saturating_add(acc.dmg[1][0]);
        }
        let mut out = [0u8; Self::EFFECTS_LEN as usize];
        let mut put = |at: u32, v: u32| out[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
        put(Self::W_TAKEN, acc.taken);
        put(Self::W_REFLECT, acc.reflect);
        put(Self::W_SP, acc.sp);
        put(Self::W_CRIT, acc.crit);
        put(Self::W_EVADE, acc.evade);
        put(Self::W_VENOM, acc.venom);
        put(Self::W_HEAL_HP, acc.heal_hp);
        put(Self::W_HEAL_SP, acc.heal_sp);
        put(Self::W_LIFESTEAL, acc.lifesteal);
        put(Self::W_BEAST_CD, acc.beast_cd);
        put(Self::W_RESET, acc.reset);
        put(Self::W_DRAGON, acc.dragon);
        put(Self::W_COUNTER, acc.counter);
        put(Self::W_COUNTER_LS, acc.counter_ls);
        put(Self::W_FURY_HIT, acc.fury_hit);
        put(Self::W_FURY_HURT, acc.fury_hurt);
        put(Self::W_FURY_KILL, acc.fury_kill);
        put(Self::W_FURY_TAKEN, u32::from(Self::cut_of(acc.fury_taken)));
        put(Self::W_FURY_KEEP, acc.fury_keep);
        put(Self::W_FURY_CD, acc.fury_cd);
        put(Self::W_FURY_START, acc.fury_start);
        put(Self::W_FURY_DMG, acc.fury_dmg);
        put(Self::W_STANCE_TAKEN, u32::from(Self::cut_of(acc.stance_taken)));
        put(Self::W_STANCE_HOLD, acc.stance_hold);
        put(Self::W_TAUNT_TAKEN, u32::from(Self::cut_of(acc.taunt_taken)));
        put(Self::W_COUNTER_SPLASH, acc.counter_splash);
        put(Self::W_THIEF, acc.thief);
        put(Self::W_COMBO_SP, acc.combo_sp);
        put(Self::W_DMN_CLAW, acc.dmn_claw);
        put(Self::W_DMN_FAST, acc.dmn_fast);
        put(Self::W_DMN_RUN, acc.dmn_run);
        put(Self::W_DMN_DUR, acc.dmn_dur);
        put(Self::W_DMN_KILL, acc.dmn_kill);
        put(Self::W_DMN_TAKEN, u32::from(Self::cut_of(acc.dmn_taken)));
        put(Self::W_WALK, if acc.walk == 0 { 0 } else { (100 / acc.walk).max(2) });
        put(
            Self::W_RANGE,
            acc.range
                | acc.blunt_reach.min(Self::RANGE_REACH_MAX) << Self::RANGE_REACH_SHIFT
                | acc.iaido_reach.min(Self::IAIDO_REACH_MAX) << Self::IAIDO_REACH_SHIFT,
        );
        put(Self::W_DRUNK, acc.drunk);
        put(Self::W_WIND, acc.wind);
        put(Self::W_DMN_FLAGS, acc.dmn_flags);
        put(Self::W_DMN_TOGSP, u32::from(Self::cut_of(acc.dmn_togsp)));
        put(Self::W_DMN_POISONED, acc.dmn_poisoned);
        put(Self::W_BEAT, acc.beat);
        put(Self::W_BEAT_ADD, acc.beat_add);
        put(Self::W_GRAVITY, ((counted * acc.gravity * 256 + 500) / 1000).min(u32::from(u16::MAX)));
        // the class's first skill (kind = its stages) and its SP cut — its own KSP byte AND every skill's W_SP (it pays `a.j`,
        // so neither SP wrapper ever saw it: 「스킬 SP 소모 −15 %」 skipped 일격 · 휘두르기 · 치유오라 until now); 0xff · 0 without a class
        // or without a cut (so nothing of ours worn reads as `effects_none` whatever the class)
        let first = Self::STAGES
            .get(usize::from(class))
            .map(|&k| (k, Self::cut_of(acc.sp * acc.ksp[usize::from(class)][usize::from(k)] >> 8)))
            .filter(|&(_, cut)| cut != 0);
        put(Self::W_FIRST_KIND, first.map_or(0xff, |(k, _)| u32::from(k)));
        put(Self::W_FIRST_CUT, first.map_or(0, |(_, cut)| u32::from(cut)));
        for (k, v) in acc.buff.iter().enumerate() {
            put(Self::BUFF_AT + 4 * k as u32, *v);
        }
        for (c, row) in acc.dmg.iter().enumerate() {
            for (k, v) in row.iter().enumerate() {
                let at = (Self::DMG_AT + 2 * (u32::from(Self::KINDS) * c as u32 + k as u32)) as usize;
                out[at..at + 2].copy_from_slice(&v.to_le_bytes());
            }
        }
        for (table, rows) in [(Self::KSP_AT, &acc.ksp), (Self::KCD_AT, &acc.kcd)] {
            for (c, row) in rows.iter().enumerate() {
                for (k, keep) in row.iter().enumerate() {
                    out[(table + u32::from(Self::KINDS) * c as u32 + k as u32) as usize] = Self::cut_of(*keep);
                }
            }
        }
        out
    }

    /// A kept fraction (/256, 256 = all) as the table's cut byte: `256 − keep`, at most 255.
    pub const fn cut_of(keep: u32) -> u8 {
        if keep >= 256 {
            0
        } else {
            (256 - keep) as u8
        }
    }

    /// What the block's tables do to an answer: `v − v × cut >> 8` (for the tests and the tools).
    #[allow(dead_code)]
    pub const fn cut(v: i32, cut: u8) -> i32 {
        v - ((v as i64 * cut as i64) >> 8) as i32
    }

    /// The effect words when nothing of ours is worn — what the install writes.
    pub fn effects_none() -> [u8; Self::EFFECTS_LEN as usize] {
        Self::effects_for(&[], 0xff)
    }

    /// The `a.m()V` piece's table: `(type << 8 | id, max HP add, max SP add)` for the items that raise a maximum.
    pub fn stat_table() -> Vec<(u32, u32, u32, u32)> {
        let mut out = Vec::new();
        for g in Self::ITEMS.iter() {
            let (mut hp, mut sp, mut guard) = (0u32, 0u32, 0u32);
            for e in g.effects {
                match *e {
                    GearEffect::MaxHp(n) => hp += u32::from(n),
                    GearEffect::MaxSp(n) => sp += u32::from(n),
                    GearEffect::Guard(n) => guard += u32::from(n),
                    _ => {}
                }
            }
            if hp > 0 || sp > 0 || guard > 0 {
                out.push(((u32::from(g.item_type) << 8) | u32::from(g.id), hp, sp, guard));
            }
        }
        out
    }

    /// What one /256 factor does to a number, for the tests and the tools: `v × mul >> 8`.
    #[allow(dead_code)]
    pub const fn scaled(v: i32, mul: u32) -> i32 {
        ((v as i64 * mul as i64) >> 8) as i32
    }
}

// ★★ 2026-10-02 스킬을 바꾸는 장신구 6종 (HERO2_ACCESSORY_PLAN.md) — what the accessory in the
/// character's ring/necklace slot does to one of its skills, all in one host block.
///
/// The six (`make_hero2_asset.py` ITM_ADDS, type 11 목걸이 · 12 반지, id 17..19):
///
/// | code | item | class | effect |
/// |---|---|---|---|
/// | 1 | 12/17 광룡의 외침 | berserker | 사자후: cooldown −50 %, SP −50 % (its damage as shipped — the −30 % was dropped 2026-10-02) |
/// | 2 | 11/17 바바리안의 의지 | berserker | 폭풍강타 = a toggle channel: every 0.35 s the 5×5 around (24 tiles, each monster once), ¼ of a cast in SP per second |
/// | 3 | 11/18 야수조련 목걸이 | every | the beast's attack skills: damage ×2 (range: a later stage) |
/// | 4 | 12/18 크루세이더 | knight | 방패치기 = a toggle channel: the front sweep + a one-tile push every second, ¼ of a cast in SP |
/// | 5 | 12/19 벽력일섬 | rogue | 일격 → 일섬: dash to a free tile of the 5×3 ahead (the furthest row), hitting the rows passed (1×3..5×3), no combo |
/// | 6 | 11/19 극사의 목걸이 | rogue | 급소가격 → 극사: to the nearest monster's back (→ side → front), ×3, stunned |
///
/// The HOST decides which one is on: it reads the worn record in equipment slot 2
/// (`a.a:[Laa;` [2] — necklace and ring share it, so one at a time) and writes the code
/// into the block's ACTIVE word, 0 when the class does not match or nothing is worn. Every
/// piece below reads that word first and, when it is not its own, runs the game as shipped
/// — so with no such accessory the game is byte for byte the shipped one, and nothing is
/// written to a record or a save (the items are plain records the game keeps).
///
/// Mechanics (definition-record word 0 redirects unless noted; HERO2_ACCESSORY_PLAN.md §2 for
/// every address):
/// * **description** — the info painter `j.a(Graphics,II,ch,ZZZ)V` draws `ch.k` only for
///   non-equipment. Its shared exit 0x15f210 (`add sp,#0x8c` …, every path's return) becomes
///   `ldr r0,[pc,#0]; bx r0; .word DESC|1`; DESC draws `ch.k` of a type 11/12 record with
///   the game's own wrapped-text routine `ac.a(Graphics,IIII[BIII)V` (the non-equipment
///   branch's very call: width 86, grey) at the painter's running line `[sp,#0x78]`, then
///   runs the painter's epilogue itself. ⛔ Only when the painter's FIRST flag `[sp,#0xb8]` is
///   set — the full sheet. A bag draws its one-line name strip under the grid with the same
///   painter and that flag clear (0x15eda2 → 0x15f6f0 → this very exit), and a description
///   drawn from there floats over the grid's cells (the user's "상세보기 글자가 떠있어"). Only our six carry a description (the shipped 34
///   accessories have none — the parser leaves `ch.k` null).
/// * **광룡** — `bl.a(I)I` (SP cost) and `bl.a(I)S` (cooldown) wrapped after: index 10 / 6
///   halved. Its damage `bl.a(BILbd;)I` is not wrapped (SkillPower's word alone).
/// * **야수조련** — `bd.a(IBB)V`, the only place beast damage lands (`ce.a(IIB)V` + the
///   beast statuses): the amount doubled. And the range doubled per family: the ring skills
///   (빛의파동 · 지옥불 · 대지분노, `bn.a(BIBI)V` tiers 0..2, one effect `ce` per direction 1..12)
///   get one more `ce` on every tile whose halved offset is a direction's (`RING_EXTRA` — the
///   cross / 3×3 / arms become radius 2 / 5×5 / 4), spawned the spawner's own way (`new ce` ·
///   `ce.<init>(BBIBBBI)V` · `bf.d(Lad;)V`); 암살자 (`bn.g()V`) also reaches distance 2 in its four
///   directions; the heavy skills' scatter (`ce.a(BII)V`, offsets up to ±4 from the parent) also
///   drops a child at twice each offset (each child still hits its own cross).
/// * **극사** — `ay.b(B)Z` (usability, before SP): kind 7 needs a PLAN — the nearest monster on
///   the whole map and a free `attr == 0` tile behind it (→ either side → in front; ★ 2026-10-06 a tile its own
///   body covers — 오딘's 3×5 — steps on past the body, [`Self::BODY_PASS`]), else
///   false (no SP, the game's own refusal mark). `ay.a()Z` at frame 0 (SP paid, auto-aim
///   done): the player is moved there (`g.u()` · x/y · `ad.a()` · `g.t()` — what `g.c(I)V`
///   does), the camera snapped (`ag.b/c` = `n.c/d` − x/y → `ag.d/e`, `a.a(BB)V`'s own), and
///   turned to face it; the shipped frame-4 strike then hits it, `ay.a(BILbd;)I` × 3. ★ 2026-10-05
///   (user: *"그냥 빗나가는 케이스에선 캐릭터기준 8방향 내의 적을 타격하는게 낫지않아?"*): the strike picks
///   its target through `a.a()Lbd;` (the monster on the FACED tile, `m.a(BLad;)Lad;` + instanceof) and
///   hits that object — so when the faced tile holds no live monster (it stepped off between the move
///   and the strike), [`Self::target_query`]'s wrapper answers one of the eight tiles around: the
///   monster the move aimed at if it stands there (GK_LOCK, compared by handle — never dereferenced),
///   else the first clockwise from the facing. No second move; the facing stays.
/// * **일섬** — `ay.a()Z` at frame 0 of kind 5: the LANDING is picked among the `DASH_TILES`×3
///   ahead (rows 1..5 × columns −1..1) — the furthest row holding a free `attr == 0` tile, its centre before
///   its sides, a tile counting only when it and one of the (up to three) tiles behind it in the
///   row before are terrain-passable (monsters and trigger tiles are passed over, like 몸통치기;
///   row 1 opens onto the player) — so the dash goes around a rock or a pack, never through a
///   wall. k = the landing's row (0 = nowhere to land). Every monster on rows 1..max(k,1) ahead
///   × the three columns is hit as `ay.a(BBI)Z` hits (`a.a(Lbd;BIBB)Z(m, 5, a.a()I + a.a(B)I(3),
///   a.l, 0)` + stun 25), then the player is moved there (`g.u()` · x/y · `ad.a()` · `g.t()`,
///   what `g.c(I)V` does along the facing) and the camera snapped; the shipped frame 0 (the
///   `a.o:I = 1` combo link) is skipped and its frame-2 strike `ay.a(BBI)Z(5, …)` answers the
///   frame-0 result instead of striking again. The path shows the player's afterimages: the draw
///   wrapper below calls the shipped `a.a(BBII)V` once more every `TRAIL_STEP` rows passed (the
///   current pose, `16·i` px back along the dash's facing and its share of the column aside,
///   anchored where the dash ended in the world) for its next `TRAIL_FRAMES` draws —
///   TRANSLUCENT: each under `Graphics.setAlpha(ALPHA_TAG | a)` (this emulator's host draw
///   alpha, `wie_backend::canvas`), `a = TRAIL_ALPHAS[i / TRAIL_STEP − 1] · left / 8`, fading
///   out, and 255 put back after.
/// * **channel** (바바리안 · 크루세이더) — `ag.c(B)V` (the arming every attack key goes
///   through): the skill's own value (6 · 4) toggles CHANNEL instead of arming, any other value
///   switches it off and arms as shipped. `a.w()V` (the per-frame update) wrapped after:
///   while on — not acting / down / dead, a combat map — every pulse (폭풍강타 7 frames ·
///   방패치기 20) pay SP (`a.a(I)V`) or stop — a quarter of a cast `a.a(I)I(kind)` per second —
///   then hit: 폭풍강타 = `bx.h(B)Z(9)` through the game's own cell = the COMBAT block's sweep,
///   which for a caller that is not a knight hits the 5×5 around (`CombatPatch::STORM_TILES`,
///   every monster once, away from the character, the plain attack `a.a()I` as kind 9) — ★
///   2026-10-03; before: `a.a(BBIBB)Z(9, dir, …)` for the 8 directions and the four tiles two
///   away at half through `a.c(BBI)Z` (the shipped spin's own reach). Without the COMBAT block
///   the shipped `bx.h` strikes the monster in front, nothing worse. 방패치기 =
///   the knight's `bx.h(B)Z(6)` (the COMBAT front sweep) and each non-boss monster on the three
///   front tiles pushed one tile along the facing when it can stand there (`bf.a(Lg;II)Z`, then
///   `bd.a(SS)V`). `a.a(BBII)V` (every stand/walk draw) wrapped before: sequences 0..3 drawn as
///   the skill's (8 · 10) with the channel's own frame.
/// * **names** (일섬 · 극사) — no guest code: the field menu builds its class skill text table
///   `bw.c:Li;` from `t/sk<class>` every time it opens (`bw.a()V`) and nulls it when it closes;
///   the asset appends 「일섬」·「극사」 and their descriptions to `t/sk2.tdf` (rows 20..23), and
///   while the accessory is worn the HOST points the skill's two row slots (name · description,
///   `2·skill` and `+1`) at them — [`JavaWrite::TableRowRef`], the ACCOUNT title swap's write —
///   and back when it comes off with the menu still up. The SKILL list and the detail window
///   (`cj.<init>`) read those slots.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AccessoryPatch {
    /// The info painter's shared exit and its first four halfwords as shipped.
    pub desc_exit: u32,
    pub desc_exit_shipped: [u16; 4],
    /// `*(sl + 0x88)` — the `Graphics.setColor(I)V` reference the painter hands the interface helper.
    pub set_color_ref: u32,
    /// The interface-call helper (`r0` = reference, `r1` = receiver) and the text routine's cell.
    pub iface_call: u32,
    pub text_cell: u32,
    /// ★ 2026-10-09 (3): `fillRect(IIII)V`'s reference pointer (`SkyDrawPatch::fill_rect_ref`) — the stat rows' clear.
    pub fill_rect_ref: u32,
    /// `(def, body)` of every method wrapped.
    pub sp_cost: (u32, u32),
    pub cooldown: (u32, u32),
    pub damage_rogue: (u32, u32),
    pub beast_damage: (u32, u32),
    pub usable_rogue: (u32, u32),
    pub drive_rogue: (u32, u32),
    pub strike_rogue: (u32, u32),
    pub arm: (u32, u32),
    pub update: (u32, u32),
    pub draw: (u32, u32),
    /// The game's call helpers.
    pub dispatch: u32,
    pub call_static: u32,
    /// Cells the game's own code calls through (virtual unless noted).
    pub cell_attack: u32,
    pub cell_bonus: u32,
    pub cell_hit: u32,
    pub cell_status: u32,
    pub cell_tile_strike: u32,
    pub cell_sp_cost: u32,
    pub cell_sp_delta: u32,
    pub cell_sweep: u32,
    /// Static `n.a(BBBII)V` (the sprite draw `a.a(BBII)V` makes).
    pub cell_draw: u32,
    /// `a.c(BBI)Z` — `a.e(BBI)Z`'s own cell: the tile two away in a direction, at the damage given.
    pub cell_far_strike: u32,
    /// The Graphics the sprite routine draws through (`ax.a`, static) and `*(sl + 0x17c)` — the
    /// `Graphics.setAlpha(I)V` reference (name-table index 0x10 of class 0x169, as `set_color_ref`
    /// is 0x95): the afterimages' translucency.
    pub graphics_slot: u32,
    pub set_alpha_ref: u32,
    /// Bodies called directly (the player's and a plain monster's are these exact ones).
    pub body_unlink: u32,
    pub body_retile: u32,
    pub body_link: u32,
    pub body_move: u32,
    pub body_can_stand: u32,
    pub body_relocate: u32,
    /// Static slots: the character, its class, the stage, the combat-map flag `bf.a:Z`, the
    /// map size `bf.d/bf.e:B`, the frame counts `a.n:[B`, the camera (`n.c/d` centre, `ag.b/c`
    /// target, `ag.d/e` live).
    pub char_slot: u32,
    pub class_slot: u32,
    pub stage_slot: u32,
    pub combat_map_slot: u32,
    pub map_w_slot: u32,
    pub map_h_slot: u32,
    pub frames_slot: u32,
    pub cam_centre: (u32, u32),
    pub cam_target: (u32, u32),
    pub cam_live: (u32, u32),
    /// Monster class descriptors (`bd` first, then the four `ah` bosses).
    pub monster_descs: [u32; 5],
    /// The knight's front diagonals per facing (CombatPatch's) and the direction table.
    pub front_diag: [[u8; 5]; 2],
    pub code_dx: [i8; 9],
    pub code_dy: [i8; 9],
    /// ★ 야수조련's range (the beast's three attack families): `bn.a(BIBI)V` (the ring skills'
    /// spawner, one effect per direction — private, the direct helper reads word 0 too), `bn.g()V`
    /// (암살자, private) and `ce.a(BII)V` (the heavy skills' scattered children, an offset from
    /// the parent).
    pub ring_spawn: (u32, u32),
    pub assassin: (u32, u32),
    pub scatter: (u32, u32),
    /// What the ring spawner itself calls: `new` (the helper, the class table, `ce`'s entry),
    /// `ce.<init>(BBIBBBI)V` (direct) and the stage's `bf.d(Lad;)V` (virtual); and 암살자's
    /// per-direction `bn.a(BBBI)V` (direct — `bn.g()V`'s own cell).
    pub new_instance: u32,
    pub direct_call: u32,
    pub class_table_slot: u32,
    pub ce_class_off: u32,
    pub cell_ce_init: u32,
    pub cell_stage_add: u32,
    pub cell_assassin_tile: u32,
    /// ★ 2026-10-05 극사's target: `(the call site in ay.a(BBI)Z, its cell, a.a()Lbd;'s definition, its body)` —
    /// the site's `bl dispatch` becomes `bl direct_call` and the cell → a definition copy in the block (the
    /// literal for that cell occurs once in the whole image: only this call reads it). ay · bl · bx all
    /// inherit the body, so the direct call is the virtual one's answer.
    pub target_query: (u32, u32, u32, u32),
    /// ★ The skill NAMES while worn: the field menu `bw.a:Lbw;` and its class skill text table
    /// `bw.c:Li;` (an `i` — checked against `i`'s descriptor).
    pub menu_slot: u32,
    pub text_table_desc: u32,
    /// ★★ 2026-10-14 장비 옵션 페이지 (신규 아이템 2차 착수 1 — user 「페이지 넘김」): the item DETAIL window `cd` (a `j`, the
    /// record `cd.a:Lch;` at absolute [`Self::DETAIL_RECORD_OFF`]) — `(def, body)` of its key method `cd.a(II)Z` (the shipped body
    /// closes on CLR · OK and swallows every other key) and of its paint `cd.a(II)V` (the box `k.a(Graphics,II)V`, then the info
    /// painter at `(X + 24, Y + 18)` with flags 1 · 0 · 0); the anchored text routine `ac.a(Graphics,II[BII)V`'s cell (the set
    /// name's: width by `ac.a([B)I`, anchor 8 = right-aligned) and `cf.c` — `/t/itmtp` (an `i`) — whose rows are the hints.
    pub detail_key: (u32, u32),
    pub detail_paint: (u32, u32),
    pub anchor_text_cell: u32,
    pub item_text_slot: u32,
    /// ★★ 2026-10-14 일섬 「번개의 호흡」: `(def, body)` of the HUD painter `n.d()V` (= `SkyDrawPatch::hud_draw`) — the yellow
    /// lightning is drawn on its entry, over every entity (this block is the first to wrap it; the sky and 악의 blocks chain on).
    pub hud_draw: (u32, u32),
    /// ★★ 2026-10-17 벽력일섬's lightning damage: a monster's `bd.a(IB)V`'s cell (the HP delta every monster class shares — the
    /// venom tick's and the 악의 block's way: `DemonPatch::cell_damage`; the number rises from it, a kill is the game's).
    pub cell_damage: u32,
}

impl AccessoryPatch {
    /// Accessory codes — the ACTIVE word.
    pub const NONE: u32 = 0;
    pub const GWANGRYONG: u32 = 1;
    pub const BARBARIAN: u32 = 2;
    pub const BEAST: u32 = 3;
    pub const CRUSADER: u32 = 4;
    pub const ILSEOM: u32 = 5;
    pub const GEUKSA: u32 = 6;
    /// `(type, id, code, class)` — class 0 berserker · 1 knight · 2 rogue · 3 any.
    pub const ITEMS: [(u8, u8, u32, u8); 6] = [
        (12, 17, Self::GWANGRYONG, 0),
        (11, 17, Self::BARBARIAN, 0),
        (11, 18, Self::BEAST, 3),
        (12, 18, Self::CRUSADER, 1),
        (12, 19, Self::ILSEOM, 2),
        (11, 19, Self::GEUKSA, 2),
    ];
    /// The equipment slot both kinds sit in.
    pub const SLOT: usize = 2;
    /// The code for a worn `(type, id)` on a character of `class`, [`Self::NONE`] otherwise.
    pub const fn code_for(item_type: u8, id: u8, class: u8) -> u32 {
        let mut i = 0;
        while i < Self::ITEMS.len() {
            let (t, d, code, c) = Self::ITEMS[i];
            if t == item_type && d == id && (c == 3 || c == class) {
                return code;
            }
            i += 1;
        }
        Self::NONE
    }

    /// The block: the data words, then one piece per entry (fixed offsets). ★ 2026-10-05 0x1400 → 0x1600 for
    /// 극사's target piece (still one allocation). ★★ 2026-10-14 0x16c0 → 0x1940 — the gear option page (below). ★★ 2026-10-14
    /// (밤) 0x1940 → 0x1e80 — 일섬 「번개의 호흡」's lightning (below): its words at BOLT_SET, its unit in 일섬's old room (BOLT_AT
    /// 0x700) and 일섬's unit — 96 B past its 1 KB with the hits it now keeps — in a room of its own at the end (ILSEOM_AT).
    /// ★★ 2026-10-17 0x1e80 → 0x2200 — 벽력일섬's lightning damage (below): its words at LIGHT_T and its unit at LIGHT_AT.
    pub const BLOCK_LEN: u32 = 0x2200;
    /// ★★ 2026-10-14 일섬 「번개의 호흡」 (user: *"단검중에 번개의 호흡이라는 단검을 추가 … 벽력일섬과 세트고 … 일섬 사용시 번개이펙트
    /// (악의의 암흑 번개 노란 번개색 버전)가 멋있게 … 번개처럼 내지르면서 적을 베는 듯한 연출"* · *"번개 이펙트 조건이 번개의 호흡 2개착용이 아니라
    /// 1개 착용"*): while 벽력일섬 is on (ACTIVE = [`Self::ILSEOM`]) and a 번개의 호흡 ([`Self::BOLT_BLADE`]) sits in the weapon or the
    /// off-hand slot, the HOST sets BOLT_SET. Then the 일섬 piece records its dash (the facing · the rows · the landing's column and
    /// tile) and the tile and row of every monster it hit (BOLT_TAB, BOLT_MAX at most), and starts BOLT_LEFT at BOLT_FRAMES; the
    /// player's draw (a pre-wrapper on `a.a(BBII)V`, before the channel's) keeps the camera (`x − px` · `y − py`); and a wrapper on
    /// the HUD painter `n.d()V` draws — over every entity — the effect library's yellow lightning for BOLT_FRAMES paints: the streak
    /// along the dash ([`Self::FX_ILSTREAK`], at the landing's feet), the landing's flash, and on each monster hit (its row's half
    /// later — the bolt runs out from the start) a slash and, one paint after, 어둠심판's bolt in yellow. Nothing of the game's is
    /// written; the set off, every piece is the shipped path. Effect ids = `tools_re/ilseom_fx.py` after `demon_fx.EFFECTS`' 79.
    pub const BOLT_BLADE: (u8, u8) = (3, 39);
    /// The equipment slots a 번개의 호흡 counts in (3 the weapon · 5 the off-hand — the rogue's second dagger).
    pub const BOLT_SLOTS: [usize; 2] = [3, 5];
    pub const BOLT_SET: u32 = 0x1940;
    pub const BOLT_LEFT: u32 = 0x1944;
    pub const BOLT_DIR: u32 = 0x1948;
    pub const BOLT_K: u32 = 0x194c;
    pub const BOLT_C: u32 = 0x1950;
    pub const BOLT_X: u32 = 0x1954;
    pub const BOLT_Y: u32 = 0x1958;
    pub const BOLT_CAMX: u32 = 0x195c;
    pub const BOLT_CAMY: u32 = 0x1960;
    /// The block's own `drawLine` reference word (`SkyDrawPatch::LINE_REF_WORD` at the install — the game holds none).
    pub const BOLT_LINE_REF: u32 = 0x1964;
    pub const BOLT_N: u32 = 0x1968;
    /// BOLT_MAX words: `tile x | tile y << 8 | row << 16` (the row 1..=DASH_TILES the monster stood on).
    pub const BOLT_TAB: u32 = 0x196c;
    pub const BOLT_MAX: u8 = 15;
    /// The data words' span (zeroed at the install but the reference word) and the pieces after it.
    pub const BOLT_DATA_LEN: u32 = Self::BOLT_TAB + 4 * Self::BOLT_MAX as u32 - Self::BOLT_SET;
    pub const BOLT_AT: u32 = 0x700;
    /// Paints the show lasts (the streak 8, a far monster's slash + bolt to the end).
    pub const BOLT_FRAMES: u8 = 12;
    /// The library's ids (`demon_fx.EFFECTS` — the effect test reads the names): the streak `FX_ILSTREAK + 15·(facing − 1) +
    /// 3·(k − 1) + (c + 1)` · the flash · the slash `+ (i & 1)` · the bolt `+ (i & 3)`.
    pub const FX_ILSTREAK: u8 = 79;
    pub const FX_ILFLASH: u8 = 139;
    pub const FX_ILSLASH: u8 = 140;
    pub const FX_ILBOLT: u8 = 142;
    pub const FX_IL_STREAK_LEN: u8 = 8;
    pub const FX_IL_FLASH_LEN: u8 = 6;
    pub const FX_IL_SLASH_LEN: u8 = 6;
    pub const FX_IL_BOLT_LEN: u8 = 4;
    /// An entity's feet from its world px (`DemonPatch::FEET`).
    pub const FEET: (u8, u8) = (8, 10);
    /// ★★ 2026-10-17 「벽력일섬」 (user: *"번개의 호흡과 벽력일섬 아이템 2개를 장착하면 일섬이 벽력일섬으로 변경되며 기존 일섬 데미지 +
    /// 번개데미지(번개 데미지는 일섬 데미지의 1.5배)로"*): while the set is on, every monster the 일섬 unit hits and leaves standing
    /// is kept for a second hit — LIGHT_MOB[n] the handle · LIGHT_DMG[n] its HP before the hit, beside BOLT_TAB[n] — and the show
    /// starts LIGHT_T at 1. The player's update (`a.w()V`, LIGHT_AT wraps the channel's tick) counts LIGHT_T up, and every step
    /// goes through the monster standing on its kept tile (`mob_at` — alive — == the handle; else it is forgotten): at
    /// [`Self::LIGHT_EVAL`] (the dash's own update — the 일섬 unit runs in the drive, inside `a.w()V`) LIGHT_DMG[n] becomes
    /// [`Self::light_damage`] = 1.5 × the HP the hit took (`dealt + dealt >> 1`; a felled one is off its tile — none), and at the
    /// row's [`Self::light_due`] the monster takes it through its own `bd.a(IB)V(dmg, 0)` — no defence (the 일섬 hit already took
    /// it), the game's own number and kill — on the update whose paint draws that row's bolt (the slash a row's half later, the
    /// bolt one paint after: `BOLT_FRAMES` show). Past LIGHT_LAST the counter stops (0). The 일섬 unit keeps only the handle and
    /// the HP (its first loads sit ~1 KB above its pool — the arithmetic lives here).
    pub const LIGHT_T: u32 = 0x1f00;
    pub const LIGHT_MOB: u32 = 0x1f04;
    pub const LIGHT_DMG: u32 = Self::LIGHT_MOB + 4 * Self::BOLT_MAX as u32;
    pub const LIGHT_DATA_END: u32 = Self::LIGHT_DMG + 4 * Self::BOLT_MAX as u32;
    pub const LIGHT_AT: u32 = 0x1f80;
    /// The counter as the dash's own update ends (the show sets 1, the wrapper's +1 runs after the drive in the same `a.w()V`):
    /// row 1's bolt is painted after the next update → due 3 for rows 1 · 2, 4 for 3 · 4, 5 for 5.
    pub const LIGHT_DUE0: u8 = 3;
    /// The counter the damage is worked out on (the dash's own update: the show's 1 + the wrapper's 1).
    pub const LIGHT_EVAL: u8 = 2;
    pub const LIGHT_LAST: u8 = Self::LIGHT_DUE0 + (Self::DASH_TILES - 1) / 2;
    /// The update a row's lightning lands on.
    pub const fn light_due(row: u8) -> u8 {
        Self::LIGHT_DUE0 + (row - 1) / 2
    }
    /// The lightning's damage from the 일섬 hit's (`dealt` ≤ 0 or a kill → none): 1.5 ×, rounded down.
    pub const fn light_damage(dealt: i32, hp_after: i32) -> i32 {
        if hp_after <= 0 || dealt <= 0 {
            0
        } else {
            dealt + (dealt >> 1)
        }
    }
    /// The skill-name key of 「벽력일섬」 (`SKILL_NAME_SWAPS`): 벽력일섬 worn with a 번개의 호흡 — the set's own name rows.
    pub const ILSEOM_BOLT_NAMES: u32 = 0x100 | Self::ILSEOM;
    /// The key the name swap follows: [`Self::ILSEOM_BOLT_NAMES`] while the set is on, else the accessory's code.
    pub const fn name_key(code: u32, bolt: bool) -> u32 {
        if bolt && code == Self::ILSEOM {
            Self::ILSEOM_BOLT_NAMES
        } else {
            code
        }
    }
    /// Is the set on: 벽력일섬 the active accessory and a 번개의 호흡 in either weapon slot (`worn` = `(slot, type, id)`).
    pub fn bolt_set(active: u32, worn: &[(usize, u8, u8)]) -> bool {
        active == Self::ILSEOM && worn.iter().any(|&(slot, t, id)| Self::BOLT_SLOTS.contains(&slot) && (t, id) == Self::BOLT_BLADE)
    }
    /// The streak id of a dash (`facing` 1..=4, `k` 1..=DASH_TILES, `c` −1..=1), `None` outside them.
    pub const fn streak_id(facing: u32, k: u32, c: i32) -> Option<u8> {
        if facing < 1 || facing > 4 || k < 1 || k > Self::DASH_TILES as u32 || c < -1 || c > 1 {
            return None;
        }
        Some(Self::FX_ILSTREAK + (15 * (facing - 1) + 3 * (k - 1)) as u8 + (c + 1) as u8)
    }
    /// ★★ 2026-10-14 장비 옵션 페이지 — an equipment record's description (its `ch.k`, the asset's option text) on a SECOND
    /// page of the item detail window: the first page is the game's own with a hint 「옵션>」 right-aligned on the last line,
    /// LEFT / RIGHT (or a tap on the box) turns to the option page — the rows under the name cleared, the description in the
    /// rolled-stat green, the hint 「<정보」. Data: GEAR_PAGE (the record whose option page is open, 0 = none — a new record
    /// opens on the first page) · DETAIL_PAINTING (1 while `cd.a(II)V` paints: the painter's other full-sheet callers — the
    /// craft window among them — never see the hint). Pieces: the page (from the DESC body, types ≤ [`Self::GEAR_LAST`]) ·
    /// the detail key wrapper · the detail paint wrapper.
    pub const GEAR_PAGE: u32 = 0x16c0;
    pub const DETAIL_PAINTING: u32 = 0x16c4;
    pub const GEAR_AT: u32 = 0x16c8;
    pub const DETAIL_KEY_AT: u32 = 0x1840;
    pub const DETAIL_PAINT_AT: u32 = 0x1900;
    /// Item types 0..=10 are equipment (weapons 0..4, armour and shields 5..10).
    pub const GEAR_LAST: u8 = 10;
    /// `cd.a:Lch;` — absolute (`j`'s 0x28 + 0).
    pub const DETAIL_RECORD_OFF: u32 = 0x28;
    /// The option text's colour (the painter's rolled-stat green) and the hints' (the description grey).
    pub const GEAR_DESC_COLOUR: u32 = 0x84c584;
    pub const GEAR_HINT_COLOUR: u32 = 0x7f7f7f;
    /// `/t/itmtp` rows (`make_hero2_asset.py` ITMTP_APPEND): 「옵션>」 on the first page · 「<정보」 on the option page. A table
    /// without them (an older asset) draws no hint — the page still turns.
    pub const GEAR_HINT_OPEN_ROW: u32 = 75;
    pub const GEAR_HINT_BACK_ROW: u32 = 76;
    /// The hint is right-aligned at `x + 93` (the set name's end) on the running line — the painter leaves equipment at
    /// `y + 72`: a weapon's element (x) and orb trait (x + 45 .. x + 63) share it, armour leaves it empty.
    pub const GEAR_HINT_DX: u8 = 93;
    pub const ANCHOR_RIGHT: u8 = 8;
    /// The option page: black from the running line − 49 (y + 23 — the name and a long name's grade on the second line
    /// stay) for 61 rows (to the box's last black row, y + 83), the text from − 48 (y + 24), three 14-px lines above the
    /// hint's row (the asset checks three lines at [`Self::DESC_WIDTH`]).
    pub const GEAR_CLEAR_UP: u8 = 49;
    pub const GEAR_CLEAR_H: u8 = 61;
    pub const GEAR_DESC_UP: u8 = 48;
    pub const GEAR_DESC_LINES: u8 = 3;
    /// The detail window's keys as `cd.a(II)Z` gets them (`r2` translated · `r3` raw — the help pager `bj.a(II)Z` @0x12ed08
    /// pages on exactly these): LEFT 2 · RIGHT 5 · '4' · '6' turn the page; CLR 0x63 · 8 · '5' close (the shipped body's own
    /// test) and forget it.
    pub const DETAIL_PAGE_KEYS: [u8; 2] = [2, 5];
    pub const DETAIL_PAGE_RAW: [u8; 2] = [0x34, 0x36];
    pub const DETAIL_CLOSE_KEYS: [u8; 2] = [0x63, 8];
    pub const DETAIL_CLOSE_RAW: u8 = 0x35;
    /// Data: ACTIVE (host-written) · CHANNEL (0 off, else the channelled kind 9 · 6) · CTICK
    /// (frames since on) · PULSE (frames to the next pulse) · MFRAME (the motion frame) ·
    /// ILSEOM (the frame-0 result: 1 hit · 2 missed · 0 none) · the 극사 plan TARGET · X · Y ·
    /// TRAIL (일섬's afterimages: draws left) · TRAIL_K (the rows the dash covered, 1..=DASH_TILES)
    /// · TRAIL_C (the column it landed on, −1..=1) · TRAIL_DIR (its facing) · TRAIL_X/Y (where it
    /// ended, world px).
    pub const ACTIVE: u32 = 0x00;
    pub const CHANNEL: u32 = 0x04;
    pub const CTICK: u32 = 0x08;
    pub const PULSE: u32 = 0x0c;
    pub const MFRAME: u32 = 0x10;
    pub const ILSEOM_HIT: u32 = 0x14;
    pub const GK_TARGET: u32 = 0x18;
    pub const GK_X: u32 = 0x1c;
    pub const GK_Y: u32 = 0x20;
    pub const TRAIL: u32 = 0x24;
    pub const TRAIL_K: u32 = 0x28;
    pub const TRAIL_C: u32 = 0x2c;
    pub const TRAIL_DIR: u32 = 0x30;
    pub const TRAIL_X: u32 = 0x34;
    pub const TRAIL_Y: u32 = 0x38;
    /// ★ 2026-10-03 방패치기 채널: frames until the next contact hit (0 = a monster in reach is hit now).
    pub const HITWAIT: u32 = 0x3c;
    /// ★ 2026-10-05 극사's 8-tile target (past the beast-range piece, which ends well before): GK_WANT (the
    /// strike wrapper's «this strike is 극사's», consumed by the target query it runs first) · GK_LOCK (the
    /// monster the move aimed at) · the `a.a()Lbd;` definition copy · the query wrapper.
    pub const GK_WANT: u32 = 0x12c0;
    pub const GK_LOCK: u32 = 0x12c4;
    /// ★ 2026-10-07 격분: the 폭풍강타 channel's pulse period, `frames − 1` (the install writes `STORM_PULSE_FRAMES − 1`;
    /// the berserker block writes `STORM_PULSE_FAST − 1` at 100 % and puts it back).
    pub const STORM_PULSE_W: u32 = 0x12c8;
    /// ★★ 2026-10-15 「피의 연쇄」 (user: *"무모한 칼춤의 옵션을 극사 옵션으로"* → *"극사로 처치 시 sp 5%회복"* · *"HP 가득한 적을 한
    /// 번에"* · 「SP 5%만 (초기화 없음)」): while 극사's necklace is on (ACTIVE = [`Self::GEUKSA`]) and a 피의 연쇄
    /// ([`Self::CHAIN_BLADE`]) sits in either weapon slot, a 극사 strike that fells a monster it found at FULL HP gives back
    /// GK_SP/256 of the max SP ([`Self::chain_sp`] — 5 % a dagger, summed; host-written). The TARGET piece writes what the strike
    /// is given (GK_HIT, the handle or 0) and whether its HP was full then (GK_FULL); the drive wrapper — the strike runs inside the
    /// shipped drive's frame 4 — reads them right after the body: GK_HIT consumed, the monster down (`g.t` ≥ STATE_DOWN) → SP.
    pub const GK_HIT: u32 = 0x12cc;
    pub const GK_FULL: u32 = 0x12d0;
    pub const GK_SP: u32 = 0x12d4;
    pub const CHAIN_BLADE: (u8, u8) = (3, 37);
    /// 5 % of the max SP a 피의 연쇄 (13/256 = 5.08 %, `GearPatch`'s HealSp's 5 %).
    pub const CHAIN_SP: u32 = 13;
    /// GK_SP for the worn accessory `code` and `worn` = `(slot, type, id)`: CHAIN_SP per 피의 연쇄 in the weapon or the off-hand slot
    /// while 극사's necklace is the accessory, else 0.
    pub fn chain_sp(code: u32, worn: &[(usize, u8, u8)]) -> u32 {
        if code != Self::GEUKSA {
            return 0;
        }
        Self::CHAIN_SP * worn.iter().filter(|&&(slot, t, id)| Self::BOLT_SLOTS.contains(&slot) && (t, id) == Self::CHAIN_BLADE).count() as u32
    }
    pub const FAKE_TARGET: u32 = 0x12e0;
    pub const TARGET_AT: u32 = 0x1300;
    pub const DEF_LEN: u32 = 0x1c;
    pub const DESC_AT: u32 = 0x40;
    /// ★ 2026-10-09 (3): the description's body (it outgrew 0x40..0xc0 with the stat rows' clear) — DESC_AT is now a jump here.
    pub const DESC_BODY_AT: u32 = 0x15c0;
    pub const SP_COST_AT: u32 = 0xc0;
    pub const COOLDOWN_AT: u32 = 0x100;
    pub const DAMAGE_ROGUE_AT: u32 = 0x180;
    pub const BEAST_AT: u32 = 0x1c0;
    pub const ROGUE_AT: u32 = 0x200;
    pub const GK_AT: u32 = 0x300;
    /// ★★ 2026-10-14 0x700 → 0x1a00 (1,152 B to the block's end — the 일섬 unit grew past its old 1 KB; BOLT_AT has that room).
    pub const ILSEOM_AT: u32 = 0x1a00;
    pub const CHANNEL_AT: u32 = 0xb00;
    /// ★ 2026-10-13: 0xe80 → 0xe40 — the push grew the game's stun for a monster it cannot move (604 B; the
    /// channel ends at 0xe38).
    pub const KNOCK_AT: u32 = 0xe40;
    pub const BEAST_RANGE_AT: u32 = 0x10c0;
    /// What the pieces do.
    pub const ROAR_KIND: u8 = 10;
    pub const ROAR_COOLDOWN_INDEX: u8 = 6;
    pub const STRIKE_KIND: u8 = 5;
    pub const VITAL_KIND: u8 = 7;
    pub const VITAL_MUL: u8 = 3;
    /// 일섬 looks DASH_TILES rows ahead (× the three columns) for its landing — 5 since 2026-10-02
    /// night (the user: *"5칸으로 … 5x3에서 가장 먼곳까지"*; 3 before).
    pub const DASH_TILES: u8 = 5;
    /// 일섬's afterimages: the player's own pose along the path for the player's next 8 draws
    /// (0.4 s at the game's 20). Counted in draws, not updates: right after a heavy frame the game
    /// draws the field several times per paint. One afterimage every TRAIL_STEP tiles back from
    /// the landing (steps 2 · 4 of a 5-tile dash — two at most, none for a one-tile dash; the
    /// user asked for fewer than the one-per-row of before), the j-th (0 = nearest) drawn at
    /// `TRAIL_ALPHAS[j] · left / 8` of 255 — a fifth at most, fading to nothing.
    pub const TRAIL_FRAMES: u8 = 8;
    pub const TRAIL_STEP: u8 = 2;
    pub const TRAIL_ALPHAS: [u8; 2] = [52, 30];
    /// `Graphics.setAlpha` arguments with these upper 24 bits are the host draw alpha —
    /// `wie_backend::canvas::DRAW_ALPHA_TAG` (the app asserts the two equal).
    pub const ALPHA_TAG: u32 = 0x4832_4100;
    pub const STUN: u8 = 25;
    pub const STUN_BASE: u8 = 18;
    pub const STORM_KIND: u8 = 9;
    pub const STORM_VALUE: u8 = 6;
    pub const STORM_SEQ: u8 = 8;
    pub const BASH_KIND: u8 = 6;
    pub const BASH_VALUE: u8 = 4;
    /// ★ 2026-10-03 (user: *"방패들고 앞으로 돌진하는듯한 … 바람이 뒤로 지나가는 듯한 이펙트"*): the
    /// channel draws the knight as sequence 23 — 방패치기's own six frames (sequence 10) with wind
    /// streaks flowing away behind the facing; the asset appends it (`tools_re/knight_iaido.py`
    /// `CHARGE_SEQ`). 10 before.
    pub const BASH_SEQ: u8 = 23;
    /// ★ 2026-10-03 (user: *"데미지가 1초당 이아니라 부딪히면 그떄마다"*): 방패치기's channel hits the
    /// moment a monster stands on the two front rows, then not again for this many frames (0.5 s
    /// at the game's 20) — a monster it cannot push away (a boss, a wall behind) is hit twice a
    /// second, one it pushes is hit again when the knight catches up. The SP stays by the second.
    pub const BASH_HIT_FRAMES: u8 = 10;
    /// One pulse a second at the game's 20 frames; 폭풍강타 pulses every 7 frames = 0.35 s (★ 2026-10-03,
    /// user: *"데미지 들어가는 틱을 0.33으로"* — a frame is 0.05 s, 7 is the nearest; 10 = 0.5 s
    /// before, 20 = 1 s at first).
    pub const PULSE_FRAMES: u8 = 20;
    pub const STORM_PULSE_FRAMES: u8 = 7;
    /// A channel's SP per second = the skill's cast cost `>> 2`; 폭풍강타 pays its pulse's share of
    /// that, `× STORM_COST_MUL >> STORM_COST_SHIFT` = × 90/256 ≈ 7/20 (the same SP a second as before).
    pub const CHANNEL_COST_SHIFT: u16 = 2;
    pub const STORM_COST_MUL: u8 = 90;
    pub const STORM_COST_SHIFT: u16 = 8;
    /// `g.t`: 3 acting · 6 down · 7 dead.
    pub const STATE_ACTING: u8 = 3;
    pub const STATE_DOWN: u8 = 6;
    /// How far the plan search walks the entity list.
    pub const LIST_MAX: u8 = 200;
    /// ★ 2026-10-06 — a landing past the target's BODY (user: *"특정 보스전에서 살상연희, 극사, 일풍격의 순간이동이 동작을
    /// 안하는거같은데 … 상단 박힌 머리만 남아있을땐 순간이동이 안되고 양쪽 손으로는 잘 돼. 이동가능한 타일이 없는거야?"*).
    /// Every landing candidate is a neighbour of the target's own tile (`ad.d/e`), but a boss stands on more than
    /// that tile: 오딘 (map 109) and 고대수호물 (map 107) are `al`, whose grid link `al.t()V` @0x11888c puts the body
    /// on 3×5 tiles around its own (x −1..1, y −2..2), and both sit in the room's top wall (오딘's tile row 7, the floor
    /// starts at row 9) — so with the hands gone every neighbour of the head's tile is a wall or the head itself,
    /// and the plans said «nowhere to stand». A candidate the TARGET stands on (the grid's live monster there,
    /// `mob_at`) is now stepped on through the body by the same offset, at most this many times, to the first
    /// tile past it — the one the shipped hit reaches the body from (오딘 seen from below: (x, 10)). A one-tile
    /// monster is never on its own neighbour, so nothing changes for it (one caught between two tiles mid-step
    /// now lands past both instead of failing). 극사 · 일풍격 · 살상연희 all take it (`hero2java::acc_emit_pass_body`).
    pub const BODY_PASS: u8 = 3;
    /// ★ 2026-10-06 — the knocked-back strike (user, after the landing fix: *"일단 명중하게 만드는게 나을것같아"*). 오딘
    /// answers a rogue that lands under its body with a counter that pushes the player one tile back — even through
    /// 살상연희's `a.s` (the hit is BLOCKed, the push is not) — between 극사's landing (frame 0) and its strike (frame 4),
    /// so the strike found nobody on the eight tiles around and missed every time (probe: five casts out of five); a
    /// 살상연희 hop lost its blow the same way. When nobody stands on the eight tiles, the tiles at this Chebyshev distance
    /// are looked at too, for the monster the move aimed at only (극사 `GK_LOCK` · the hop's target — a handle compared
    /// against `mob_at`'s answers, never dereferenced): it takes the strike. A knockback is one tile, so 2.
    pub const KNOCKBACK_REACH: u8 = 2;
    /// Absolute field offsets (`*(handle) + 4 + off`). Every entity: x/y px `ad.a/b:S` · tiles
    /// `ad.d/e:B` · width `ad.f:B` · next `ad.b:Lad;` · `g.t` · `g.u` · `g.w`. The character:
    /// SP `a.b:I` · `a.o:[B` (queued kinds) · `a.i:B` (their index) · `a.l/m:B` · `a.p:[B` (skill
    /// levels). The stage: attributes `bf.c:[[B` · entities `bf.a:[[Lad;` · the list head `bf.a:Lad;`.
    /// A record: `ch.g` type · `ch.k` description.
    pub const X_OFF: u32 = 0x0;
    pub const Y_OFF: u32 = 0x4;
    pub const TILE_X_OFF: u32 = 0x10;
    pub const TILE_Y_OFF: u32 = 0x14;
    pub const WIDTH_OFF: u32 = 0x20;
    pub const NEXT_OFF: u32 = 0x2c;
    pub const STATE_OFF: u32 = 0x3c;
    pub const FACING_OFF: u32 = 0x40;
    pub const FRAME_OFF: u32 = 0x48;
    pub const SP_OFF: u32 = 0x70;
    /// ★★ 2026-10-15 the character's max SP `a.e:I` (피의 연쇄's 5 %) · a monster's HP `bd.d` and max HP `bd.c` (its FULL test).
    pub const SP_MAX_OFF: u32 = 0x7c;
    pub const MOB_HP_OFF: u32 = 0x74;
    pub const MOB_MAX_HP_OFF: u32 = 0x70;
    pub const QUEUE_OFF: u32 = 0xac;
    pub const QUEUE_AT_OFF: u32 = 0xb0;
    pub const WEAPON_L_OFF: u32 = 0xd4;
    pub const WEAPON_M_OFF: u32 = 0xd8;
    pub const SKILLS_OFF: u32 = 0x138;
    pub const STAGE_ATTR_OFF: u32 = 0x20;
    pub const STAGE_GRID_OFF: u32 = 0x30;
    pub const AMBUSH_MAP: u8 = 18;
    pub const AMBUSH_CORPSE: u8 = 58;
    pub const NPC_DESC: u32 = 0x0019_2280;
    pub const NPC_KIND_OFF: u32 = 0x5c;
    pub const STAGE_HEAD_OFF: u32 = 0x24;
    pub const ITEM_TYPE_OFF: u32 = 0x0;
    pub const ITEM_DESC_OFF: u32 = 0x1c;
    /// The beast (`bn`, `ad`'s child): skill levels `bn.k:[B` · the casting skill `bn.g:B` · `bn.l:B`.
    pub const BEAST_LEVELS_OFF: u32 = 0x40;
    pub const BEAST_SKILL_OFF: u32 = 0x68;
    pub const BEAST_L_OFF: u32 = 0x7c;
    /// Ring tiers (빛의파동 · 지옥불 · 대지분노 = 0 · 1 · 2; the heavy skills' are 3 · 5 · 7) and directions.
    pub const RING_TIER_MAX: u8 = 2;
    pub const RING_DIRS: u8 = 12;
    /// The ring's extra tiles per direction 1..=12 — `(skip at level ≥ 3, [(dx, dy)])`: the radius
    /// doubled as every tile whose halved offset (`sign(v)·⌈|v|/2⌉` per axis) is the direction's own
    /// (level 1 cross → the radius-2 cross · level 2 3×3 → 5×5 · level 3 adds the arms to 4). Level 3
    /// already spawns (0,±2)/(±2,0) as directions 9..12, so the four plain directions skip theirs then.
    pub const RING_EXTRA: [(bool, &'static [(i8, i8)]); 12] = [
        (true, &[(0, -2)]),
        (true, &[(0, 2)]),
        (true, &[(-2, 0)]),
        (true, &[(2, 0)]),
        (false, &[(-2, -1), (-1, -2), (-2, -2)]),
        (false, &[(2, -1), (1, -2), (2, -2)]),
        (false, &[(-2, 1), (-1, 2), (-2, 2)]),
        (false, &[(2, 1), (1, 2), (2, 2)]),
        (false, &[(0, -3), (0, -4)]),
        (false, &[(0, 3), (0, 4)]),
        (false, &[(-3, 0), (-4, 0)]),
        (false, &[(3, 0), (4, 0)]),
    ];
    /// `bw.c:Li;` — `bw`'s own field after `j`'s 0x28; `i.a:[Object` (the rows) is `i`'s first.
    pub const MENU_SKILL_TABLE_OFF: u32 = 0x28;
    pub const TEXT_TABLE_ROWS_OFF: u32 = 0x0;
    /// `t/sk2.tdf` as shipped has 20 rows (name · description × 10 skills); the asset appends six
    /// — ★ 2026-10-05 20 「일풍격」 · 21 its description (the rogue's seventh active `a.p[15]`, whose
    /// detail window reads rows `2·(15 − 5)` — [`IlpungPatch`]), then 22 「일섬」 · 23 · 24 「극사」 · 25
    /// (`make_hero2_asset.py` `SK2_APPEND`; 일섬/극사 sat at 20/22 before 일풍격 took 20).
    /// `(code, the sk2 skill index whose two rows are re-pointed, the appended name row)`.
    pub const SK2_SHIPPED_ROWS: u32 = 20;
    /// ★ 2026-10-07 개화: its two rows go in at 22 · 23 (`16 − 5 = 11`, the detail window's index), 일섬 · 극사 move to 24 · 26.
    /// ★★ 2026-10-09 (11) 「암살」: its two rows at 24 · 25 (`17 − 5 = 12`), 일섬 · 극사 move to 26 · 28.
    /// ★★ 2026-10-17 「벽력일섬」: its two rows at 30 · 31 (appended last), keyed by [`Self::ILSEOM_BOLT_NAMES`] ([`Self::name_key`]).
    pub const SKILL_NAME_SWAPS: [(u32, u32, u32); 3] = [(Self::ILSEOM, 0, 26), (Self::GEUKSA, 2, 28), (Self::ILSEOM_BOLT_NAMES, 0, 30)];
    /// 암살자: `bn.a(BBBI)V(3, 0, dir, n)` puts status 3 on whatever stands n tiles away in
    /// direction `dir` — `ad.a(BBB)Lad;` answers only 1..4 (the shipped loop's 5..8 find nothing),
    /// so the shipped reach is the radius-1 cross; doubled = the same four at distance 2.
    pub const ASSASSIN_STATUS: u8 = 3;
    pub const ASSASSIN_DIRS: u8 = 4;
    pub const ASSASSIN_FAR: u8 = 2;
    /// The rogue's 「급소공부」-style stun term `a.p[14]` (the shipped 일격's `3·a.p[14] + 18`).
    pub const STUN_SKILL: u8 = 14;
    /// The painter's frame at its exit: the running line, x, the Graphics, the record, and its
    /// first flag — the full sheet (상세보기 · the shop panels); clear = a bag's name strip, which
    /// leaves through the same exit right after the name.
    pub const PAINT_LINE: u16 = 0x78;
    pub const PAINT_X: u16 = 0x80;
    pub const PAINT_G: u16 = 0x84;
    pub const PAINT_REC: u16 = 0xb4;
    pub const PAINT_FULL: u16 = 0xb8;
    /// The description: the text routine's width and colour. ★ 2026-10-03 (user: new descriptions
    /// with stats, e.g. "사자후 쿨타임-50%, SP소모-50%, 의지 +20"): 86 (the non-equipment branch's) → **94**.
    /// The box's inner black spans x 49..=144 and the text starts at the painter's x = 52, so 94 is
    /// the most that keeps every glyph pixel inside: the routine wraps when `pen + advance > width`
    /// (pen from 1, ASCII 6, Hangul 9) and a glyph that fits ends at most at 52 + 94 − 1 = 145 as a
    /// CELL, whose painted columns (ASCII ≤ 5 of 8, Hangul 8 of 9) stop by 144. A space that would
    /// overflow is dropped, not carried to the next line (probe, three descriptions). The asset's
    /// descriptions pad a comma with spaces to land their line ends (`make_hero2_asset.py` ITM_ADDS).
    pub const DESC_WIDTH: u8 = 94;
    pub const DESC_COLOUR: u32 = 0x7f7f7f;
    /// Where the first line goes relative to the painter's running line (px). ★ 2026-10-09 (3): the accessory branch
    /// (`0x15f400..`) RESERVES its stat rows whether a stat is there or not — 공격/방어 at 107 · HP/SP (red) at 119 · the
    /// stat pairs (green) at 131, the running line ending at 141 for every accessory — so the old −36 (row 105, measured on
    /// a record whose stats were 0: the probe's give rolled none then) laid the description over the game's own stat rows
    /// once a record carried real ones (the guardian three, the six's single stat). Now a record WITH a description gets
    /// those rows cleared (black, `DESC_CLEAR_*`) and its text from row 101 (−40): four 14-px lines end on row 152, the
    /// box's last black row (LV on 95..99). The description carries the stats itself (the asset writes them, ≤ 4 lines).
    pub const DESC_DY: i8 = -40;
    /// The clear: from x − 3 (the box's inner black starts at 49 for x = 52), 96 wide, the 40 rows above the running line.
    pub const DESC_CLEAR_DX: u8 = 3;
    pub const DESC_CLEAR_W: u8 = 96;
    pub const DESC_CLEAR_H: u8 = 40;
    /// Lines the description may wrap to (`make_hero2_asset.py` checks it with the same width rule).
    pub const DESC_LINES: u8 = 4;
}

/// ★★ 2026-10-05 로그 「일풍격」 (user: *"일풍격은 … 가장 가까운 적에게 순간이동하여 주변을 한바퀴 돌며 단검을
/// 투척하는 스킬이야. 발도와 마찬가지로 터줏대감 퀘스트 클리어시 획득 … 단검 투척은 캐릭터 기준 8방향 2칸씩"* · asked: the
/// search reaches 20 tiles each way, nowhere to land = not cast, the numbers = 발도 under 신의가호 (cooldown and SP too),
/// a monster may be hit more than once). `HERO2_GUEST_RE.md` §8.48.
///
/// **Where it lives.** The rogue's `a.p:[B` is full (5 combo stages + 10 skills = 15) and its seventh active's
/// natural slot `a.p[5 + 6] = 11` is 동체시력 — so 일풍격 is a SIXTEENTH slot: `a.b()V` allocates `byte[16]` for
/// every class and fills index 15 with `-1` too ([`Self::slots`]). The character blob writes `a.p` whole
/// (`a.b()[B`: `write(byte[])`) and reads it with `read(b, 0, b.length)` (`a.a([B)V`), so a new save is one byte
/// longer and an old one simply leaves index 15 at the `-1` `a.b()V` put there. The level-up unlock `a.d()V` stops at
/// 14 (`cmp r6,#0xe`), so only the host opens it (`hero2java::h2_ilpung_unlock`: 번개난타 `a.p[10]` open → `a.p[15]`
/// 0 — 「터줏대감」's rogue script teaches 번개난타).
///
/// **Its kind is 15** = its `a.p` index, so everything the game indexes by kind is natural: the action sequence
/// `kind + 4 = 19` (`hero/hero2`, appended by `tools_re/rogue_ilpung.py`), its length `a.n[15]` (the sheet's frame
/// count, `cf.a()V`), `SkillPowerPatch`'s ×1.5 (`kind ≥ stages`), the rogue body `ay.b(B)Z`'s «kind > 10 → usable».
/// (Kind 11 would have needed none of the value remaps but shares `a.n[11]` with 민첩태세's aura, sequence 15.)
/// What converts the quickbar VALUE 7 to and from it is remapped, and what the shipped tables do not cover is answered:
///
/// | | hook | does |
/// |---|---|---|
/// | quickbar → kind | `a.d(B)Z` def word 0 | rogue · value 7 → 11 (so `v + L − 1` = 15) |
/// | kind → quickbar (cooldown start) | `a.f()Z`'s own `ag.a(IZ)V` cell → a definition copy | rogue · value 11 (`15 − L + 1`) → 7 — a beast skill's value 11 comes through another call site |
/// | SP | `ay.a(I)I` def | kind 15 → `a.e:I >> 2` (발도's) |
/// | cooldown | `ay.a(I)S` def | index 6 → [`Self::COOLDOWN`] (the shipped table has six; index 6 threw) |
/// | max level | `ay.a(I)B` def | index 15 → 5 (shipped: ≥ 11 → 3, the passives') |
/// | SKILL window cursor | `s.a()B` def | ACTIVE-tab cell 6 of a rogue with 16 slots: 11 → 15 (invest · 퀵슬롯등록 · 상세보기 all take it from here) |
/// | SKILL window grid | the painter's right-column `a.b(B)B` / `a.a(I)B` calls (`bl 0x16917c` → `bl 0x169160` + their cells → definition copies) | rogue · 11 → 15 (level) / 11 → 5 (max) |
/// | SKILL window cell | the right column's cell drawer `s.a(Graphics,IIIBBZ)V` (already a direct call; its cell → a definition copy) | rogue · the icon index `[sp+4]` 6 → 10: the drawer takes the icon (`img/gmico` rogue strip cell) AND the name (`bw.c` row `2·i`) from it — 6 is 동체시력's row 12, 10 is 일풍격's 20 (`tools_re/rogue_ilpung.py` puts the icon in cells 6 and 10) |
/// | usable | `ay.b(B)Z` def (chained — the accessory block's wrapper or the body) | kind 15: a PLAN — the nearest monster within [`Self::RANGE`] tiles with a free tile next to it (a boss: next to its BODY — [`AccessoryPatch::BODY_PASS`]), or false (not cast: no SP, no cooldown) |
/// | drive | `ay.a()Z` def (chained) | kind 15: frame 0 → to the plan's tile, facing the target · frames 1..=8 → one dagger per frame, clockwise from the facing, 2 tiles each, every monster on them hit (no dedupe — a monster may take several) |
/// | load | `a.a([B)V` def | CALL the body, then a blob with fewer than 16 skill bytes (a save from before) leaves `a.p[15]` at −1 |
///
/// ⛔ **The load wrapper is not optional.** The loader hands `a.a([B)V` the decrypted blob, which is two bytes longer
/// than its data ([`Self::BLOB_FULL`] — the checksum's place and `bg.b`'s `+ 1`, both 0 — measured: an old save loaded
/// `a.p[15] = 0`), so a 16-slot `a.p` read from an old 44-byte blob takes a 0 as its sixteenth byte — «unlocked»
/// without the quest.
pub struct IlpungPatch {
    /// `(definition, body)` of the wrapped methods.
    pub queue: (u32, u32),
    pub sp_cost: (u32, u32),
    pub cooldown: (u32, u32),
    pub max_level: (u32, u32),
    pub cursor: (u32, u32),
    pub usable: (u32, u32),
    pub drive: (u32, u32),
    /// `a.a([B)V` — the character blob's reader (one caller, the loader `ag.a(BZ)V`). ★★ 2026-10-21 the gear block's HP
    /// post-wrapper chains over this one ([`GearPatch::load`] — the saved HP waits for the equipment).
    pub load: (u32, u32),
    /// `(the call site's cell, the real definition, its body)` — `ag.a(IZ)V` from `a.f()Z` (static).
    pub cd_start: (u32, u32, u32),
    /// The painter's two right-column calls: `(bl site, its shipped halfwords, the cell, the real definition[, its body])`.
    pub paint_level: (u32, [u16; 2], u32, u32, u32),
    pub paint_max: (u32, [u16; 2], u32, u32),
    /// The right column's cell drawer call: `(the cell, the real definition, its body)` — the call is direct already.
    pub paint_cell: (u32, u32, u32),
    /// ★ 2026-10-07 각성기 · 버서커 행 재배치: the painter's LEFT column — the same three calls for cells 0..=3, each
    /// through its own cell: `(bl site, its shipped halfwords, the cell)` for the level and the max read, the drawer's
    /// cell (its call is direct already). The definitions are the right column's.
    pub paint_level_left: (u32, [u16; 2], u32),
    pub paint_max_left: (u32, [u16; 2], u32),
    pub paint_cell_left: u32,
    /// ★ 2026-10-07: the register popup's write `ag.a(IIZ)V(slot, h + 1, beast)` from `s.a(BB)V` — `(the call site's cell,
    /// the real definition, its body, ★ 2026-10-08 the call site itself)`. Its value is the CELL's `h + 1`; the wrapper makes
    /// it the kind's quickbar value. ⛔ The site calls through the VIRTUAL helper ([`Self::dispatch`]), which does not take the
    /// body from the cell's record — the cell → copy alone changed nothing (S25: 격분's cell registered 도발, 도발's 분노태세),
    /// so the `bl` becomes the DIRECT helper ([`Self::direct`]) like the painter's, and the copy's word 0 is what runs.
    pub register: (u32, u32, u32, u32),
    /// `a.b()V`: `new byte[15]` → [`Self::SLOTS`] and the `-1` fill's last index 14 → `SLOTS − 1`.
    pub slots: [(u32, u16, u16); 2],
    /// ★★ 2026-10-09 (11) 「암살」 — the SKILL window's PASSIVE tab: `s.a()V`'s rogue cell count `movs r1,#9` → 10 (`(site,
    /// shipped, patched)`), and the right column's three calls, each through its own cell (the definitions are the ACTIVE
    /// right column's — `paint_level.3` · `paint_max.3` · `paint_cell.1`): `(bl site, its shipped halfwords, the cell)` for the
    /// level and the max read, the drawer's cell (its call is direct already).
    pub passive_cells: (u32, u16, u16),
    pub passive_level: (u32, [u16; 2], u32),
    pub passive_max: (u32, [u16; 2], u32),
    pub passive_cell: u32,
    /// ★ 「암살」's damage: `(definition, body)` of the player's hit `a.a(Lbd;BIBB)Z` (every class's — one definition, no
    /// override; every rogue blow passes it, the host's runs too) and the rogue's damage function `ay.a(BILbd;)I` (the
    /// receive calls it on the attacker after the defence and the element). The element table `co.a:[[B` (static).
    pub hit: (u32, u32),
    pub damage: (u32, u32),
    pub element_table_slot: u32,
    /// Helpers: virtual dispatch · direct call · static call · divide.
    pub dispatch: u32,
    pub direct: u32,
    pub call_static: u32,
    pub divide: u32,
    /// Cells: the attack roll `a.a()I` · the player's hit `a.a(Lbd;BIBB)Z` (ay.a(BBI)Z's own) · `br.a(B)V` (the sound).
    pub cell_attack: u32,
    pub cell_hit: u32,
    pub cell_sound: u32,
    /// The class static `ag.a:B` and the character static `ag.a:La;`.
    pub class_slot: u32,
    pub char_slot: u32,
    /// Absolute field offsets: the window cursor `j.h` · `a.e:I` max SP · `a.l:B` weapon · `a.p:[B` · `a.o:[B` /
    /// `a.i:B` (the queued kinds) · `g.w` (the action frame).
    pub cursor_off: u32,
    pub max_sp_off: u32,
    pub weapon_l_off: u32,
    pub skills_off: u32,
    pub queue_off: u32,
    pub queue_at_off: u32,
    pub frame_off: u32,
}

impl IlpungPatch {
    pub const CLASS: u8 = 2;
    /// The rogue's combo stages (`a.i[2]`) — `a.p[5..]` are its skills.
    pub const STAGES: u8 = 5;
    /// 일풍격 = `a.p[15]` = kind 15; the quickbar value 7 (the window's cell 6 + 1).
    pub const KIND: u8 = 15;
    pub const VALUE: u8 = 7;
    /// What value 7 means as shipped (`7 + 5 − 1`) — 동체시력's slot, which the window's cell 6 also names.
    pub const SHIPPED_KIND: u8 = 11;
    /// The value `a.d(B)Z` must see for kind 15 (`15 − 5 + 1`) — and what `a.f()Z` hands the cooldown start.
    pub const QUEUE_VALUE: u8 = 11;
    /// ★ 2026-10-07 각성기: 17 — every class's `a.p` has a SEVENTEENTH slot now ([`Self::AWAKEN`]); 16 since 일풍격.
    /// ★★ 2026-10-09 (11): 18 — the rogue's passive 「암살」 is `a.p[17]` ([`Self::AMSAL`]; the other classes' stays −1).
    pub const SLOTS: u32 = 18;
    /// The character blob: `a.p` starts at byte 29 (`tools_re/h2save.py` CHAR_SKILLS), and the decrypted array the reader
    /// gets is TWO bytes past the data — the file's `[len][data · checksum]` read whole, then `bg.b([B[B)[B` makes
    /// `new byte[in.length + 1]` (@0x12e80e) and decrypts the data only. So a blob that holds all eighteen is at least
    /// `29 + 18 + 2` long; one that holds seventeen (a save of the 각성기 builds, 10-07 … 10-09) is 48, sixteen (일풍격 · 도발)
    /// 47 (measured: an old 15-slot save's is 46 and loaded `a.p[15] = 0`). The load wrapper locks what the blob does not
    /// carry — every slot past it, from the top down.
    pub const BLOB_SKILLS_AT: u32 = 29;
    pub const BLOB_FULL: u32 = Self::BLOB_SKILLS_AT + Self::SLOTS + 2;
    pub const BLOB_SEVENTEEN: u32 = Self::BLOB_SKILLS_AT + 17 + 2;
    pub const BLOB_SIXTEEN: u32 = Self::BLOB_SKILLS_AT + 16 + 2;
    /// `a.p[10]` (번개난타 as shipped — 「살상연희」 since 2026-10-06, [`RoguePatch`]) — 「터줏대감」 opens it (the rogue's script 10 on
    /// map 77), and 일풍격 with it.
    pub const UNLOCK_WITH: u8 = 10;
    pub const COOLDOWN_INDEX: u8 = 6;
    /// = `CombatPatch::IAIDO_COOLDOWN` (발도 — user: 「쿨타임도 발도와 같아」).
    pub const COOLDOWN: u8 = 140;
    pub const MAX_LEVEL: u8 = 5;
    /// ★ 2026-10-06 「심안」 (user: *"독연마 스킬을 … 1레벨 자동 획득 패시브 스킬로 변경 … 맵 내의 투명한 몬스터를 확인 할 수있는
    /// 스킬"* — the name is the user's pick of 「다른 이름」, the shipped 동체시력 (`a.p[11]`, 회피율) stays): the rogue's
    /// `a.p[12]`, 독 연마 as shipped (its effect is 맹독감염's now — `VenomPatch::buff_frames_mastered`), is a passive
    /// whose top level is 1. This block's `ay.a(I)B` wrapper answers it next to 일풍격's 15 (shipped: 11.. → 3), so the
    /// SKILL window shows 1/1 and refuses an investment; the level itself is the host's — every rogue holds exactly 1
    /// (`hero2java::h2_born_skill_repair`), what it does is [`RoguePatch`]'s.
    pub const SIGHT_SLOT: u8 = 12;
    pub const SIGHT_MAX: u8 = 1;
    /// The SKILL window: the ACTIVE tab's seventh cell (the cursor `j.h`).
    pub const WINDOW_CELL: u8 = 6;
    /// What the cell drawer gets for it instead of 6: `kind − stages` — the index the detail window `cj` uses too
    /// (its icon cell and name row `2·10 = 20`).
    pub const WINDOW_ICON: u8 = Self::KIND - Self::STAGES;
    /// The nearest monster is looked for this many tiles each way (Chebyshev).
    pub const RANGE: u8 = 20;
    /// The action: frame 0 the landing, frames 1..=THROWS one dagger each (`tools_re/rogue_ilpung.py` draws them).
    pub const FRAMES: u8 = 10;
    pub const THROWS: u8 = 8;
    /// Each dagger's reach in tiles.
    pub const REACH: u8 = 2;
    /// The eight directions clockwise from north (screen y down), and where the turn starts per facing
    /// (`g.u` 1 up · 2 down · 3 left · 4 right) — `rogue_ilpung.{DIR8, START8}`.
    pub const DIR8: [(i8, i8); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];
    pub const START8: [u8; 5] = [0, 0, 4, 6, 2];
    /// The landing: the target's neighbours in the order tried, as `(along, across)` the player → target axis —
    /// behind it first, then its sides, its front, the four diagonals; one the target itself stands on (a boss's body)
    /// steps on by the same offset past it ([`AccessoryPatch::BODY_PASS`]).
    pub const LANDING: [(i8, i8); 8] = [(1, 0), (0, -1), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, -1), (-1, 1)];
    /// Damage = `a.a()I · (lv + 5) · (lv + 3) · POWER_MUL / POWER_DIV`, lv = `a.p[15]` (1..=5), then the game's
    /// class multiplier (`SkillPowerPatch` ×1.5). = 발도's sweep factor `/12` × 신의가호 발도's extra 1 + 102/128 ≈
    /// 115/64 (1.797): Lv5 ≈ attack × 17.97, as 신의가호 발도.
    pub const POWER_MUL: u8 = 115;
    pub const POWER_DIV: u16 = 768;
    /// The sound on a hit (발도's).
    pub const SOUND: u8 = 7;
    /// The value `a.d(B)Z` must see for kind 15 on a class with `stages` combo stages (`v + stages − 1 = 15`).
    pub const fn queue_value(stages: u8) -> u8 {
        Self::KIND - stages + 1
    }
    /// What the SKILL window's cell drawer gets for the sixteenth slot: `kind − stages` (the detail window's index too).
    pub const fn window_icon(stages: u8) -> u8 {
        Self::KIND - stages
    }

    // ── ★★ 2026-10-07 각성기 · 버서커 행 재배치 (user: *"버서커의 스킬들 위치를 재구성할꺼야. 몸통치기를 없앨꺼고 가르기와
    //    분노태세를 위로 한칸씩 올리고 현재 분노태세 위치에 도발을 넣을꺼야 … 그리고 마지막 각성기를 추가할꺼야. 각성기는 70레벨이
    //    넘으면 얻을 수 있고"*) ──────────────────────────────────────────────────────────────────────────────────
    //
    // The ACTIVE tab is a TABLE now: cell → `a.p` index ([`Self::ROWS`]), one row per class, eight cells each. Quickbar
    // values, icons and names stay PHYSICAL (`kind − stages`, the detail window's index — [`Self::value_of_kind`] /
    // [`Self::kind_of_value`]), so a save's registered slots keep their skills and the strips are indexed as before;
    // only the window's order changes, and the register popup's `h + 1` is turned into the cell's kind's value. The
    // berserker's 몸통치기 (`a.p[5]`) is in no row: its points come back once (`hero2java::h2_bodyslam_retire`), the
    // byte is −1 and its level-table entry 99 (`GuestBuild::bodyslam_locked`), so nothing reopens it. The awakening
    // ([`Self::AWAKEN`]) is each class's first free slot — the 17th for the berserker and the rogue, the 16th for the
    // knight — opened by the host at LV [`Self::AWAKEN_LEVEL`] (`hero2java::h2_extra_slot_unlocks`), top level 1.
    /// The SKILL window's tab flag `s.b:Z` (absolute: `j` 0x28 + 4 — set = the PASSIVE tab).
    pub const TAB_OFF: u32 = 0x2c;
    /// The ACTIVE tab, per class: cell 0..=7 → `a.p` index.
    pub const CELLS: u8 = 8;
    pub const ROWS: [[u8; 8]; 3] = [
        [4, 6, 7, 15, 8, 9, 10, 16], // 버서커: 휘두르기 · 가르기 · 분노태세 · 도발 · 반격태세 · 폭풍강타 · 사자후 · 격분
        [3, 4, 5, 6, 7, 8, 9, 15],   // 나이트: 치유오라 · 검기 · 둔기강타 · 방패치기 · 신의가호 · 철괴 · 발도 · 천공의검
        [5, 6, 7, 8, 9, 10, 15, 16], // 로그: 일격 · 민첩태세 · 급소가격 · 맹독감염 · 단검난무 · 살상연희 · 일풍격 · 개화
    ];
    /// The classes' 기본공격 stage counts (`a.i`) — the first class skill's `a.p` index.
    pub const CLASS_STAGES: [u8; 3] = [BerserkerPatch::STAGES, 3, Self::STAGES];
    /// The awakening per class: `(a.p index = kind, quickbar value, ACTIVE cell)` — 격분 · 천공의검 · 개화.
    pub const AWAKEN: [(u8, u8, u8); 3] = [(16, 9, 7), (15, 8, 7), (16, 8, 7)];
    pub const AWAKEN_LEVEL: u8 = 70;
    /// ★★ 2026-10-09 (11) 1 → 5 (user: *"각 직업 각성기도 5lv까지 찍을 수 있게 해줘. 3lv까지 찍으면 지금의 데미지고 5lv까지
    /// 찍으면 지금 데미지의 1.4배까지 데미지가 올라간다고 생각하면 돼"*): every awakening's damage is
    /// [`Self::awaken_power`] of what it was — Lv3 = the 10-08/10-09 damage exactly, Lv5 ×1.4, and the same 0.2 a level
    /// below Lv3 (Lv1 ×0.6 · Lv2 ×0.8 — ⚠ Claude's linear reading of 「3lv까지 … 지금의 데미지」). SP and cooldown keep
    /// no level. 격분: the explosion (`BerserkerPatch` drive) · 천공의검: the five sweeps (`CombatPatch` sweep) · 개화: every
    /// blow (`RoguePatch` drive).
    pub const AWAKEN_MAX_LEVEL: u8 = 5;
    pub const AWAKEN_POWER_ADD: u8 = 2;
    pub const AWAKEN_POWER_DIV: u8 = 5;
    /// The awakening's level as the blocks read it: the signed byte `a.p[kind]`, anything outside `1..=5` counts as 1.
    pub const fn awaken_level(lv: i8) -> u8 {
        if lv >= 1 && lv <= Self::AWAKEN_MAX_LEVEL as i8 { lv as u8 } else { 1 }
    }
    /// What `damage` (the Lv3 = shipped-to-10-09 number) becomes at the level byte `lv` — floor division, as the blocks'
    /// divide calls do it.
    pub const fn awaken_power(damage: i32, lv: i8) -> i32 {
        damage * (Self::awaken_level(lv) + Self::AWAKEN_POWER_ADD) as i32 / Self::AWAKEN_POWER_DIV as i32
    }
    /// 몸통치기 — the berserker's `a.p[5]`, retired (no row, locked at −1).
    pub const BODYSLAM: u8 = 5;
    /// The kinds past a class's shipped list and the quickbar values that name them: `(class, kind, value)` — the
    /// sixteenth slot's two (일풍격 7 · 도발 8) and the three awakenings.
    pub const EXTRA: [(u8, u8, u8); 5] = [(2, 15, 7), (0, 15, 8), (0, 16, 9), (1, 15, 8), (2, 16, 8)];
    /// The quickbar value of `kind` for `class`: `kind − stages + 1` below the extras, the table's value for them.
    pub const fn value_of_kind(class: u8, kind: u8) -> u8 {
        let mut i = 0;
        while i < Self::EXTRA.len() {
            let (c, k, v) = Self::EXTRA[i];
            if c == class && k == kind {
                return v;
            }
            i += 1;
        }
        kind - Self::CLASS_STAGES[class as usize] + 1
    }
    /// The `a.p` index quickbar value `v` names for `class` (the inverse of [`Self::value_of_kind`]).
    pub const fn kind_of_value(class: u8, v: u8) -> u8 {
        let mut i = 0;
        while i < Self::EXTRA.len() {
            let (c, k, val) = Self::EXTRA[i];
            if c == class && val == v {
                return k;
            }
            i += 1;
        }
        Self::CLASS_STAGES[class as usize] + v - 1
    }
    /// The largest quickbar value `class` holds (its awakening's).
    pub const fn value_max(class: u8) -> u8 {
        Self::AWAKEN[class as usize].1
    }
    // ── ★★ 2026-10-09 (11) 로그 패시브 「암살」 (user: *"로그의 패시브 스킬 암살을 추가할꺼야. 상대방에게 가하는 첫번째 공격이 100% 방어
    //    무시 추가 피해를 입히는 패시브야 lv은 5lv까지 있고 5lv까지 찍으면 20%부터 100%까지 추가피해를 입히는거야"*) ─────────────
    //
    // `a.p[17]` (every class's array grows to 18; the rogue's alone ever opens — at LV [`Self::AMSAL_UNLOCK_LEVEL`], by the
    // host, `hero2java::h2_extra_slot_unlocks`), top level 5, the PASSIVE tab's tenth cell (right column, under 급소공부).
    // THE FIRST BLOW: a hit that lands on a monster still at full HP (`bd.d ≥ bd.c` — plain monsters never heal, so it is the
    // first hit it takes; ⚠ a boss that heals back to full counts again) gets `lv · 20 %` of its damage WITHOUT THE DEFENCE
    // added (Lv1 20 % … Lv5 100 %): the receive computes `d = (atk − DEF) · element / 10` and hands it to the job's damage
    // function — this block's wrapper there runs the job's chain on `d` (the hit as it was) and again on `atk · element / 10`
    // (the same hit as if the monster had no defence — the skill's own multiplier included) and adds `lv / 5` of the second.
    // The receive then multiplies the crit and applies it all — one number on the screen, the kill · EXP · drop as usual.
    // `atk` and the element come from the hit's own arguments: a pre-wrapper on `a.a(Lbd;BIBB)Z` (every player blow of every
    // class — the rogue's basic attack, its skills, the host's 살상연희 · 개화 · 일풍격) stashes them with the target.
    pub const AMSAL: u8 = 17;
    pub const AMSAL_MAX_LEVEL: u8 = 5;
    pub const AMSAL_PCT_PER_LEVEL: u8 = 20;
    /// ⚠ The user named no unlock — open from LV 1 (Claude's choice: testable at once; raise it here).
    pub const AMSAL_UNLOCK_LEVEL: u8 = 1;
    /// The PASSIVE tab: the rogue's cells 0..=9 (the five combo stages, then 동체시력 · 심안 · 직감 · 급소공부 · 암살). The body
    /// maps a right-column cell to `stages + a.j[2] + row` = 15 for the tenth (일풍격's slot) — every reader of it (the cursor,
    /// the painter's level · max · drawer) is remapped 15 → 17.
    pub const PASSIVE_CELLS: u8 = 10;
    pub const PASSIVE_SHIPPED_KIND: u8 = 15;
    /// The drawer's · detail window's index (`kind − stages`): 10 as shipped → 12 (`t/sk2.tdf` rows 24 · 25, `img/gmico`
    /// strip 2's cell 12).
    pub const PASSIVE_SHIPPED_ICON: u8 = Self::PASSIVE_SHIPPED_KIND - Self::STAGES;
    pub const AMSAL_ICON: u8 = Self::AMSAL - Self::STAGES;
    /// The monster (absolute): HP `bd.d` · max HP `bd.c` · the template `bd.a:Lcq;`; the template's element `cq.f:B`.
    pub const MOB_HP_OFF: u32 = 0x74;
    pub const MOB_MAX_HP_OFF: u32 = 0x70;
    pub const MOB_TEMPLATE_OFF: u32 = 0x60;
    pub const TEMPLATE_ELEMENT_OFF: u32 = 0x18;
    /// The bonus for a first blow whose damage without the defence is `x0`, at the level byte `lv` (0 outside 1..=5) —
    /// `x0 · lv · 20 / 100`, floor, as the block's divide does it.
    pub const fn amsal_bonus(x0: i32, lv: i8) -> i32 {
        if lv < 1 || lv > Self::AMSAL_MAX_LEVEL as i8 { 0 } else { x0 * lv as i32 * Self::AMSAL_PCT_PER_LEVEL as i32 / 100 }
    }
    /// The block: data · the definition copies · the wrappers · the row table · the plan unit · the drive unit.
    /// ★ 2026-10-07: 0xa00 → 0xb00 — the class-generic wrappers answer two classes (bigger rooms). ★ Same day, 각성기:
    /// 0xb00 → 0xd00 — the register wrapper, the left-column arms and the row table. ★★ 2026-10-09 (11) 「암살」: 0xd00 →
    /// 0x1180 — the PASSIVE tab's three copies and wrappers, the max answers (four now), the cursor (its PASSIVE remap) and
    /// the loader (three tiers) moved up past the drive, the hit's stash and the damage wrapper.
    pub const BLOCK_LEN: u32 = 0x1180;
    pub const TARGET: u32 = 0x00;
    pub const X: u32 = 0x04;
    pub const Y: u32 = 0x08;
    /// The last action frame the drive acted on (−1 after a plan) — a frame acts once.
    pub const LAST: u32 = 0x0c;
    /// A cell of our own holding the real `a.a(I)B` definition — the max wrapper's way back to the class's own.
    pub const MAX_CELL: u32 = 0x10;
    pub const DEF_LEN: u32 = 0x1c;
    pub const FAKE_CD: u32 = 0x20;
    pub const FAKE_LEVEL: u32 = 0x40;
    pub const FAKE_MAX: u32 = 0x60;
    pub const QUEUE_AT: u32 = 0x80;
    pub const CD_START_AT: u32 = 0xe0;
    pub const SP_AT: u32 = 0x140;
    pub const COOLDOWN_AT: u32 = 0x160;
    /// ★★ 2026-10-15 (2차 H3 회오리 송곳니) in the cooldown piece's spare room: 1 when a dagger of THIS 일풍격 left a monster at
    /// HP ≤ 0 (cleared at each landing) · the extra casts of this chain (cleared at a real cast's frame 0).
    pub const WHIRL_KILLED: u32 = 0x198;
    pub const WHIRL_EXTRA: u32 = 0x19c;
    /// ★★ 2026-10-15 (H3) at most this many 일풍격 after the first (the first + 3 = 4).
    pub const WHIRL_EXTRA_MAX: u8 = 3;
    /// The register write's definition copy and its wrapper.
    pub const FAKE_REG: u32 = 0x1a0;
    pub const REG_AT: u32 = 0x1c0;
    pub const LEVEL_AT: u32 = 0x2a0;
    pub const PAINT_MAX_AT: u32 = 0x2e0;
    pub const FAKE_DRAW: u32 = 0x360;
    pub const DRAW_AT: u32 = 0x380;
    /// [`Self::ROWS`] as the wrappers read it: 24 bytes, `class · 8 + cell`.
    pub const ROWS_AT: u32 = 0x3c0;
    pub const USABLE_AT: u32 = 0x3e0;
    pub const DRIVE_AT: u32 = 0x7a0;
    // ── ★★ 2026-10-09 (11) 「암살」 — past the drive (the max answers · the cursor · the loader moved here; their old rooms
    //    0x180 · 0x220 · 0x320 are free) ──
    /// The hit's stash — written by the pre-wrapper on every player blow, consumed (zeroed) by the damage wrapper: the
    /// target's handle · the blow's raw attack (before the defence) · its element.
    pub const STASH_T: u32 = 0xd00;
    pub const STASH_ATK: u32 = 0xd04;
    pub const STASH_ELEM: u32 = 0xd08;
    /// What the hit's and the damage function's definitions held at install (the wrappers go on through them).
    pub const CHAIN_HIT: u32 = 0xd0c;
    pub const CHAIN_DAMAGE: u32 = 0xd10;
    /// First blows given a bonus, and the last bonus (nothing of the game reads them — the probe does).
    pub const AMSAL_COUNT: u32 = 0xd14;
    pub const AMSAL_LAST: u32 = 0xd18;
    /// ★★ 2026-10-15 (2차 E7) the gear block's W_RANGE word's address (the gear install writes it; 0 = none — REACH as before).
    pub const GEAR_RANGE_PTR: u32 = 0xd1c;
    /// The PASSIVE tab's three definition copies (level `a.b(B)B` · max `a.a(I)B` · the cell drawer) and their wrappers.
    pub const FAKE_P_LEVEL: u32 = 0xd20;
    pub const FAKE_P_MAX: u32 = 0xd40;
    pub const FAKE_P_DRAW: u32 = 0xd60;
    pub const P_LEVEL_AT: u32 = 0xd80;
    pub const P_MAX_AT: u32 = 0xdc0;
    pub const P_DRAW_AT: u32 = 0xe00;
    pub const MAX_AT: u32 = 0xe40;
    pub const CURSOR_AT: u32 = 0xe80;
    pub const LOAD_AT: u32 = 0xf40;
    pub const HIT_AT: u32 = 0xfa0;
    pub const AMSAL_AT: u32 = 0xfe0;
}

/// ★★ 2026-10-07 버서커 「분노태세」 = ON/OFF · 새 액티브 「도발」 (user: *"버서커의 분노태세를 on off토글형 스킬로 바꾸고싶어. on
/// 상태일떄는 버프기 표시하는 영역에 남은 지속시간 대신 on이라고 표시 … 1lv기준 받는피해 20%증가, 주는피해 20증가이고, 5lv 기준
/// 받는피해 10% 증가, 주는피해 30%증가"* · *"버서커의 추가스킬로 도발이라는 액티브 스킬 … 사자후처럼 모든 맵에 몬스터를 나에게
/// 도발 … 5lv짜리 … 1lv기준 도발당한 몬스터는 나에게 주는피해가 10%감소하고 내가 해당 몬스터에게 주는 피해가 10% 증가 … 5lv 기준
/// … 20% 감소 … 20% 증가 … 내가 몬스터를 공격했을떄처럼 나에게 어그로가 끌려서 다가와야해"* → asked: 분노태세 = 켤 때만 SP · 쿨 1 초;
/// 도발 = 쿨 8 초 · 지속 10 초 · SP = 사자후의 ¼ · 사자후를 배우면 함께 열림). `HERO2_GUEST_RE.md` §8.52.
///
/// **분노태세** (`a.p[7]`, buff 30, flag `a.n:Z`): the cast (`bl.a()Z` case 7) is as shipped — `a.n ← 1` · `a.m()V` ·
/// `g.a(BI)V(30, 140)` · sound 5 — and `BuffRefreshPatch` turns its buff call into the TOGGLE: absent → the buff goes in
/// for [`Self::RAGE_ON_FRAMES`] (never ends by itself; the HUD shows ON), present → it is marked ended (the game's
/// own end handler clears `a.n` the next frame). The SP wrapper answers 0 while `a.n` is up (off is free), the cooldown
/// wrapper [`Self::RAGE_COOLDOWN`] for its index. The shipped effects (roll ×2 · defence × 3/5) are off
/// (`GuestBuild::rage_shipped_off`); this block's damage wrapper multiplies what the berserker DEALS by
/// `(RAGE_DEALT_BASE + 10·lv) / 400` and its 철괴 hook what he TAKES by `(RAGE_TAKEN_BASE − 10·lv) / 400`.
///
/// **도발** (`a.p[15]` = kind 15, quickbar value 8, SKILL window cell 3 since 2026-10-07 (2) — [`IlpungPatch::ROWS`]; sequence 19 of
/// `hero/hero0` = 사자후's shout in red, `tools_re/berserker_taunt.py`): the drive plays sound 5 at
/// [`Self::TAUNT_SOUND_FRAME`] and at [`Self::TAUNT_FRAME`] taunts — every live monster on the stage gets the player as
/// its chase target (`bd.c`, what a plain monster's `bd.e()V` walks toward) and its aggro flag (`bd.g`, what a boss's
/// AI reads after a hit), [`Self::TAUNT`] ← the level and the player gets buff [`Self::TAUNT_ID`] for
/// [`Self::TAUNT_FRAMES`] (the HUD's timer; `BuffRefreshPatch` restarts it on a recast). While that buff is up (checked
/// by walking the player's buff list — a stale TAUNT is cleared): a plain monster's update `bd.f()V` keeps its target
/// on the player and runs even off-screen (its own gate freezes a monster more than 4 tiles off-screen and drops its
/// target — the wrapper raises its visible flag `bd.f:Z` around the call and puts it back), the damage wrapper adds
/// `(TAUNT_DEALT_BASE + 10·lv) / 400` and the hook takes `(TAUNT_TAKEN_BASE − 10·lv) / 400`. SP = the body's 사자후
/// (`a.e · 4/5`) `>> 2` · cooldown [`Self::TAUNT_COOLDOWN`] · max level 5 · unlock = 사자후 open (`hero2java::h2_slot16_unlock`).
///
/// Every definition word is CHAINED (read at install — the SkillPower / accessory blocks' wrappers or the bodies);
/// nothing is written to the character beyond what the game itself writes; unpatched, the shipped berserker is back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BerserkerPatch {
    /// `(definition, body)`: `bl.a(I)I` SP · `bl.a(I)S` cooldown · `bl.a(I)B` max level · `bl.a()Z` the cast drive ·
    /// `bl.a(BILbd;)I` the damage multiplier · `bd.f()V` a plain monster's update.
    pub sp_cost: (u32, u32),
    pub cooldown: (u32, u32),
    pub max_level: (u32, u32),
    pub drive: (u32, u32),
    pub damage: (u32, u32),
    pub mob_update: (u32, u32),
    /// ★ 2026-10-07 격분: `bl.b(B)Z` usable · `bl.a(B)B` the kind → sequence map · `a.g()V` the character's update (the
    /// per-frame hook the auto-explosion waits in).
    pub usable: (u32, u32),
    pub seq_map: (u32, u32),
    pub update: (u32, u32),
    /// Helpers: virtual dispatch · static call · divide.
    pub dispatch: u32,
    pub call_static: u32,
    pub divide: u32,
    /// Cells: `g.a(BI)V` (bl.a()Z case 7's own — the buff) · `br.a(B)V` (its sound).
    pub cell_buff: u32,
    pub cell_sound: u32,
    /// ★ 격분: cells of `a.a()I` (the attack roll, the combat block's) · `bd.a(IB)V` (a monster takes damage — the venom
    /// tick's) · `ag.c(B)V` (the slot key's cast entry, `n.a(I)V`'s own static cell); the definition record of `a.b(I)V`
    /// (the HP delta — its word 0 is read at call time, so whatever guards it is honoured).
    pub cell_attack: u32,
    pub cell_damage: u32,
    pub cell_cast: u32,
    pub hp_delta_def: u32,
    /// Statics: the character `ag.a:La;` · the stage `ag.a:Lbf;` · the class `ag.a:B`.
    pub char_slot: u32,
    pub stage_slot: u32,
    pub class_slot: u32,
    /// The monster classes' descriptors (`bd` and the bosses — `AccessoryPatch::monster_descs`).
    pub monster_descs: [u32; 5],
    /// Absolute field offsets: the character's `a.n:Z` (rage) · `a.p:[B` · `a.o:[B` / `a.i:B` (the queued kind) ·
    /// `g.w` (the action frame) · `g.b:Vector` (buffs) · `a.e:I` (max SP) · `g.t` (state); a monster's `bd.c:Lad;` (chase
    /// target) · `bd.f:Z` (visible) · `bd.g:Z` (aggro) · `g.t` (state); an entity's next `ad.b`; the stage's list head;
    /// the static `a.n:[B` (action lengths).
    pub rage_off: u32,
    pub skills_off: u32,
    pub queue_off: u32,
    pub queue_at_off: u32,
    pub frame_off: u32,
    pub buffs_off: u32,
    pub max_sp_off: u32,
    pub mob_target_off: u32,
    pub mob_visible_off: u32,
    pub mob_aggro_off: u32,
    pub state_off: u32,
    pub next_off: u32,
    pub stage_head_off: u32,
    pub frames_slot: u32,
}

impl BerserkerPatch {
    pub const CLASS: u8 = 0;
    pub const STAGES: u8 = 4;
    /// 분노태세 = `a.p[7]` (sk0 3), its buff and its cooldown index (quickbar value 4 − 1).
    pub const RAGE: u8 = 7;
    pub const RAGE_ID: u8 = 30;
    pub const RAGE_CD_INDEX: u8 = 3;
    /// 1 s (the wrapper's answer is final — not halved again).
    pub const RAGE_COOLDOWN: u8 = 20;
    /// The ON buff's length: the tick (`bm.a()V`, `b:S += 1; b ≥ a:I → ended`) never reaches it.
    pub const RAGE_ON_FRAMES: u32 = 0x7fff_ffff;
    /// 도발 = `a.p[15]`, quickbar value 8; the kind's cooldown index (value − 1). ★ Same day, later: its ACTIVE cell is 3
    /// (the user's order — `IlpungPatch::ROWS`), the value stays 8.
    pub const TAUNT_KIND: u8 = IlpungPatch::KIND;
    pub const TAUNT_CELL: u8 = 3;
    pub const TAUNT_VALUE: u8 = 8;
    pub const TAUNT_CD_INDEX: u8 = Self::TAUNT_VALUE - 1;
    /// 8 s · 10 s.
    pub const TAUNT_COOLDOWN: u8 = 160;
    pub const TAUNT_FRAMES: u8 = 200;
    /// The player's buff for the taunt: `bm.i[32 − 30]` exists (30..=36 are valid) and only monsters ever get 32 from the
    /// game (`bl.a(BI)Z`'s stun) — on the character it is free. The end handler `a.a(Lav;)Z` does nothing for it.
    pub const TAUNT_ID: u8 = 32;
    pub const TAUNT_MAX_LEVEL: u8 = 5;
    /// 사자후 (`a.p[10]`): its SP is the body's `a.e · 4 / 5`; 도발 takes `>> TAUNT_SP_SHIFT` of it. Its unlock opens 도발.
    pub const ROAR: u8 = 10;
    pub const TAUNT_SP_SHIFT: u16 = 2;
    /// The drive's frames (sequence 19 = 사자후's 11): the shout's sound at 1, the taunt at 9 (사자후's own blast frame).
    pub const TAUNT_SOUND_FRAME: u8 = 1;
    pub const TAUNT_FRAME: u8 = 9;
    pub const TAUNT_SOUND: u8 = 5;
    /// The factors are `(base ± 10·lv) / FACTOR_DIV`, lv 1..=5 (a byte outside counts as 1): rage dealt 1.10 … 1.20 (★ 2026-10-14
    /// (c) 버서커 배율 겹침 완화, user 「×1.20」 — was 1.20 … 1.30, base 470) ·
    /// rage taken 1.20 … 1.10 · taunt dealt 1.10 … 1.20 · taunt taken 0.90 … 0.80.
    pub const FACTOR_DIV: u16 = 400;
    pub const FACTOR_STEP: u8 = 10;
    pub const RAGE_DEALT_BASE: u16 = 430;
    pub const RAGE_TAKEN_BASE: u16 = 490;
    pub const TAUNT_DEALT_BASE: u16 = 430;
    pub const TAUNT_TAKEN_BASE: u16 = 370;
    /// `(numerator, denominator)` of a factor at a level byte (the game's signed byte; outside `1..=5` = 1).
    pub const fn factor(base: u16, plus: bool, lv: i8) -> (u32, u32) {
        let lv = if lv >= 1 && lv <= 5 { lv as u32 } else { 1 };
        let step = Self::FACTOR_STEP as u32 * lv;
        ((if plus { base as u32 + step } else { base as u32 - step }), Self::FACTOR_DIV as u32)
    }
    /// What a hit of `damage` becomes under a factor — the block's own arithmetic (floor division; ≤ 0 untouched).
    pub const fn scaled(damage: i32, (num, den): (u32, u32)) -> i32 {
        if damage <= 0 {
            damage
        } else {
            damage * num as i32 / den as i32
        }
    }
    // ── ★★ 2026-10-07 격분 — the berserker's awakening (user: *"격분: 사용시 1분간 지속되는 버프를 줌(캐릭터 주변 붉은색 핏방울
    //    이펙트) 격분 버프 상태에서 적에게 피해를 입히면 분노게이지가 일정량씩 참(타수에 비례하여 참 40타를 채우면) 분노게이지당
    //    데미지가 1%증가, 총 100%까지 증가. 100%까지 차면 분노폭발이 가능해지고 분노 폭발 … 시 전맵에 공격력의 10배에 해당하는
    //    데미지를 주며 격분 종료. 또한 격분 100% 상태에서는 휘두르기, 폭풍강타, 기본공격에 생명령 흡수 10% … 버프와 … 공격속도
    //    (새로운 스탯)가 50% 상승한다. (바바리안의 의지를 착용하고있을시 폭풍강타 데미지가 0.33틱이 아닌 0.22틱 마다 들어감)
    //    100%까지 차지 않아도 1분이 지나면 자동으로 격분 사용(모인 분노게이지 10%마다 격분 데미지는 공격력의 1배, 2배,3배~10배)
    //    쿨타임은 격분 종료 후 30초"*) ─────────────────────────────────────────────────────────────────────────────
    //
    // `a.p[16]` = kind 16, quickbar value 9, the ACTIVE tab's eighth cell; opened at LV 70, top level 1. Two casts of
    // the same kind: with FURY_STATE 0 the drive STARTS it (buff 33 for FURY_FRAMES, HITS · GAUGE 0 — the cooldown
    // table answers 0 so the key stays live); with FURY_STATE 1 the cast is the EXPLOSION (usable only at 100 %, or when
    // the buff ran out — AUTO; the table answers FURY_COOLDOWN then, so the 30 s begin as it ends): every live monster on
    // the stage takes `a.a()I × mul` through its own `bd.a(IB)V`, mul = 10 at 100 % and GAUGE / 10 at the auto-cast,
    // the buff is marked ended, the state cleared. While it runs the damage wrapper counts every hit (HITS, to
    // FURY_HITS_FULL — ★ 2026-10-09 25, and the TAKEN function counts every hit that lands on the berserker the same
    // way), GAUGE = HITS · 100 / FURY_HITS_FULL, and multiplies what the berserker deals by `(100 + GAUGE) / 100`; at
    // 100 % the hits of the speed kinds (기본공격 · 휘두르기 · 폭풍강타) give back a tenth as HP (`a.b(I)V`), those actions
    // run 1.5 frames a tick (the drive steps the body a second frame every other tick) and the accessory block's
    // 폭풍강타 channel pulses every STORM_PULSE_FAST frames (its PULSE word, [`AccessoryPatch::STORM_PULSE_W`]). The
    // character's update `a.g()V` is the clock: a FURY_STATE 1 whose buff is gone sets AUTO, and an AUTO berserker who
    // stands idle in the field is cast through the game's own `ag.c(B)V(9)`.
    pub const FURY_KIND: u8 = 16;
    pub const FURY_VALUE: u8 = 9;
    pub const FURY_CELL: u8 = 7;
    pub const FURY_CD_INDEX: u8 = Self::FURY_VALUE - 1;
    pub const FURY_MAX_LEVEL: u8 = IlpungPatch::AWAKEN_MAX_LEVEL;
    /// Buff 33 — 맹독감염's id on the rogue, free on the berserker (its painter draws [`Self::FURY_AURA_SEQ`], the venom
    /// block's aura pick made class-aware). 60 s; the cooldown 30 s from the end.
    pub const FURY_ID: u8 = 33;
    pub const FURY_FRAMES: u16 = 1200;
    pub const FURY_COOLDOWN: u16 = 600;
    /// ★ 2026-10-09 40 → 25 (user: *"A랑 C로 진행할꺼고 C는 맞을때마다 +4%"* — A = 4 % a hit dealt, C = 4 % a hit
    /// taken): a hit either way is one of these, 100 / 25 = 4 % of the gauge.
    pub const FURY_HITS_FULL: u8 = 25;
    pub const FURY_GAUGE_FULL: u8 = 100;
    /// The explosion's attack multiple at 100 % (and the auto-cast's at GAUGE / 10 — 1 … 10).
    pub const FURY_BLAST_MUL: u8 = 10;
    /// A tenth of the damage dealt comes back as HP at 100 % (the speed kinds only).
    pub const FURY_LIFESTEAL_DIV: u8 = 10;
    /// The kinds the 100 % state speeds up and drains for: the 기본공격 stages (0..STAGES), 휘두르기 4, 폭풍강타 9.
    pub const FURY_SPEED_KINDS: [u8; 6] = [0, 1, 2, 3, 4, 9];
    /// 바바리안의 의지's 폭풍강타 channel: 7 frames a pulse as shipped (0.35 s), 5 at 100 % (0.25 s — the user's 0.33 → 0.22,
    /// at 20 frames a second the nearest whole frames). The accessory block's word holds `frames − 1`.
    pub const STORM_PULSE_FAST: u8 = 5;
    /// The drive's frames in sequence 20 (the start) and 21 (the explosion, 8 frames each): the buff / the blast.
    pub const FURY_START_FRAME: u8 = 3;
    pub const FURY_BLAST_FRAME: u8 = 4;
    pub const FURY_SOUND: u8 = 5;
    /// `hero/hero0`: 20 = the start (what kind 16 draws as shipped — the map is the identity), 21 = the explosion
    /// (the map answers 17 while EXPLODING), 22 = the aura (buff 33's painter on a berserker).
    pub const FURY_SEQ: u8 = 20;
    pub const FURY_EXPLODE_SEQ: u8 = 21;
    pub const FURY_AURA_SEQ: u8 = 22;
    /// Frames the explosion is drawn for (sequence 21's count = `a.n[16]`, sequence 20's) — EXPLODING counts them down.
    pub const FURY_FRAMES_DRAWN: u8 = 8;
    /// SP = max SP >> 2 to start; the explosion cast is free.
    pub const FURY_SP_SHIFT: u16 = 2;
    /// `GAUGE` for `hits` (0..=100).
    pub const fn gauge_of(hits: u32) -> u32 {
        let h = if hits > Self::FURY_HITS_FULL as u32 { Self::FURY_HITS_FULL as u32 } else { hits };
        h * Self::FURY_GAUGE_FULL as u32 / Self::FURY_HITS_FULL as u32
    }
    /// The block: data · the buff walk · the wrappers · the drive · the monster update · the update hook.
    /// ★ 격분: 0x500 → 0x800 — usable · the sequence map · the update hook, the damage wrapper and the drive grown.
    /// ★ 2026-10-09: 0x800 → 0x880 — the blast's record of where it struck ([`Self::BLOOD_XY`]). ★ 2026-10-09 (2) → 0x8e0:
    /// the TAKEN function counts 격분's hits taken (192 → 256 B) and the drive (736 B in 736) gets room to grow (768).
    /// ★ 2026-10-09 (11) → 0x920: the explosion scales by 격분's level (`IlpungPatch::awaken_power`) — the drive 832 B.
    /// ★★ 2026-10-14 (2차 A) → 0xb00: the gear tail (GEAR_PTR · HIT_TICK · the FGEAR piece). ★★ (2차 G) → 0xc00 — FGEAR's guard,
    /// its post-hit (the counter's splash) and the splash walk.
    pub const BLOCK_LEN: u32 = 0xc00;
    /// The taunt's level while it runs (0 = none) — written by the drive, cleared by whoever finds the buff gone.
    pub const TAUNT: u32 = 0x00;
    /// The last action frame the drive acted on — a frame acts once.
    pub const LAST: u32 = 0x04;
    /// 격분: 0 idle · 1 running (the buff is up, or AUTO waits for its explosion).
    pub const FURY_STATE: u32 = 0x08;
    pub const FURY_HITS: u32 = 0x0c;
    pub const FURY_GAUGE: u32 = 0x10;
    /// The buff ran out before 100 %: the next idle field frame casts the explosion.
    pub const FURY_AUTO: u32 = 0x14;
    /// The speed-up's tick parity.
    pub const FURY_TICK: u32 = 0x18;
    /// Frames of the explosion left to draw (the sequence map reads it).
    pub const EXPLODING: u32 = 0x1c;
    /// The accessory block's STORM_PULSE_W address (0 = no accessory block).
    pub const ACC_PULSE: u32 = 0x20;
    /// The explosion's multiple as the blast computes it (the probe reads it).
    pub const FURY_LAST_MUL: u32 = 0x24;
    /// ★ The character 격분 started on (its handle): the update hook clears the state when another character is in the
    /// world (a reload after a death, another save, another class) — the block outlives the character, and a fresh one
    /// must not inherit a running gauge, an AUTO explosion or the channel's shortened period (probe, 2026-10-08).
    pub const FURY_OWNER: u32 = 0x28;
    /// ★ 2026-10-09 the blood (user: *"격분을 재사용해서 폭발할떄 이펙트가 전맵에 피가 폭발하는 듯한 이팩트가 있었으면 좋겠어"*): the
    /// blast records where it struck for [`SkyDrawPatch`]'s blood pieces — BLOOD_SEQ counts the blasts that hit something
    /// (the painter starts on a new value), BLOOD_N the monsters recorded (at most BLOOD_MAX), BLOOD_PX / BLOOD_PY the
    /// berserker's own px; BLOOD_XY holds BLOOD_MAX `(x, y)` word pairs — the monsters' px (`ad.a:S` · `ad.b:S`, absolute
    /// 0 · 4) as the blast found them. Nothing of the game reads these.
    pub const BLOOD_SEQ: u32 = 0x2c;
    pub const BLOOD_N: u32 = 0x30;
    pub const BLOOD_PX: u32 = 0x34;
    pub const BLOOD_PY: u32 = 0x38;
    /// ★ 2026-10-09 the host plays the awakenings' voices (`voice.rs` — user: *"격분 사용시 그아앗! … 격분 폭발 시 갈!"*):
    /// non-zero = the drive leaves out the game's own 기합 (sound [`Self::FURY_SOUND`] = `P/snd/45`, a berserker's shout) at
    /// 격분's start and explosion, so two shouts do not land on one frame. Written at install (0 = the 기합 as before).
    pub const FURY_VOICE: u32 = 0x3c;
    pub const BLOOD_XY: u32 = 0x8a0;
    pub const BLOOD_MAX: u8 = 16;
    pub const HAS_BUFF_AT: u32 = 0x40;
    pub const SP_AT: u32 = 0xa0;
    pub const COOLDOWN_AT: u32 = 0x100;
    pub const MAX_AT: u32 = 0x140;
    pub const SEQ_AT: u32 = 0x160;
    pub const USABLE_AT: u32 = 0x180;
    pub const DAMAGE_AT: u32 = 0x1e0;
    pub const TAKEN_AT: u32 = 0x320;
    pub const MOB_AT: u32 = 0x420;
    pub const UPDATE_AT: u32 = 0x4a0;
    pub const DRIVE_AT: u32 = 0x560;
    /// ★★ 2026-10-14 신규 아이템 2차 A (격분 장비 — 분노의 투구 · 광전사의 발걸음 · 상처의 갑주 · 꺼지지 않는 불씨 · 멈추지 않는
    /// 도끼/대검; user: *"싸우는 동안만 무한"*): GEAR_PTR = the gear block (its effect words — [`GearPatch::W_FURY_HIT`] …), written by
    /// THAT block's install (0 = none: every piece as before); HIT_TICK = the cooldown clock's count a hit last lengthened 격분 at
    /// (one a tick). FGEAR = one entry, the op in r0 and its argument in r1, r4..r7 kept: OP_HIT d → d + W_FURY_DMG and 격분 +
    /// W_FURY_HIT frames (once a tick) · OP_TAKEN d → d − W_FURY_TAKEN and + W_FURY_HURT frames · OP_START → GAUGE ← W_FURY_START %
    /// (answers 1 when set) · OP_KEEP → the explosion keeps 격분 with the gauge at W_FURY_KEEP % (answers 1; never for an AUTO
    /// explosion — the minute is over) · OP_CD → the explosion's cooldown (0 = [`Self::FURY_COOLDOWN`]) · OP_EXTEND f → 격분 + f
    /// frames (the gear block's kill) · OP_SCALE d → d + W_FURY_DMG (the explosion). 「+ f frames」 = buff 33's elapsed − f, at
    /// least 0 — the minute is the cap — and only while 격분 runs, not AUTO, on its owner.
    pub const GEAR_PTR: u32 = 0x920;
    pub const HIT_TICK: u32 = 0x924;
    /// The explosion's cooldown leaf (the cooldown wrapper jumps here while 격분 runs — its own room had 8 bytes too few).
    pub const FURY_CD_AT: u32 = 0x930;
    pub const FGEAR_AT: u32 = 0x960;
    pub const OP_HIT: u8 = 1;
    pub const OP_TAKEN: u8 = 2;
    pub const OP_START: u8 = 3;
    pub const OP_KEEP: u8 = 4;
    pub const OP_CD: u8 = 5;
    pub const OP_EXTEND: u8 = 6;
    pub const OP_SCALE: u8 = 7;
    /// ★★ 2026-10-14 (2차 G2 · G3) OP_GUARD d (r2 = the character) → d − W_STANCE_TAKEN while its `a.o:Z` (반격태세) is up,
    /// − W_TAUNT_TAKEN while TAUNT runs — the TAKEN function's last word. (2차 G5 · A) OP_POST d (r2 = the monster; the
    /// CALLER's r7 = the kind — the damage wrapper's, read off the stack FGEAR pushed it to) → a counter (kind 8) splashes
    /// d × W_COUNTER_SPLASH >> 8 on every other live monster of the 3×3 round the monster (`bd.a(IB)V`); then, while 격분 runs,
    /// OP_HIT's work — the damage wrapper's last word (d ≤ 0: nothing).
    pub const OP_GUARD: u8 = 8;
    pub const OP_POST: u8 = 9;
    /// `a.o:Z` (absolute) — 반격태세's stance flag (`bl.a()Z` case 8 sets it, buff 31's end handler clears it).
    pub const STANCE_OFF: u32 = 0x108;
    /// The counter's kind (`bl.a(BILbd;)I` case 8).
    pub const COUNTER_KIND: u8 = 8;
}

/// ★★ 「악마화」 D1 — 악의 (2026-10-13, HERO2_DEMON_PLAN.md §3 · §7 D1; HERO2_GUEST_RE.md §8.68).
///
/// The beast's 대지분노 (kind 9) becomes 악의: the character itself turns demon for [`Self::FRAMES`] — received
/// damage × [`Self::TAKEN_NUM`]/[`Self::TAKEN_DEN`], the class's attack skills and awakenings refused (only its
/// buffs, [`Self::BUFF_VALUES`]), and the 기본공격 stages become the CLAW COMBO: the host's own hits on the six
/// tiles ahead (stages 0 · 1 — 2 rows deep · 3 wide, the 신의가호 방패치기 range) and on the 5×5 around (stage 2,
/// the slam), at the attack `a.a()I + 지력' × CLAW_INT` ([`Self::claw_attack`] — 1.0 × an all-힘 berserker's
/// 기본공격 for the LV 85 체1의1지1 knight, `tools_re/demon_balance.py` §1), kind = the stage (so every
/// 기본공격 option of the gear — the 비호 lifesteal, 독사's +50 % — applies as the user decided: *"그냥 냅두자"*).
/// The cast costs max SP >> [`Self::SP_SHIFT`] and starts the slot's cooldown at [`Self::COOLDOWN`] ticks
/// FROM THE CAST (user: *"악의가 켜지고 1분30초야 쿨타임 템 맞추면 무한 지속 가능해"* — the gear's 야수 쿨 −20 %
/// scales it through `ag.a(IZ)V`'s own chain). A press while it runs ends it. ⛔ Only 악의 is intercepted in
/// D1 — the six other 데몬 skills keep their shipped behaviour until D3–D5, and outside 악의 everything is as
/// shipped (user ⑫: *"악의가 켜졌을때만 기존 스킬들이 변경되는거야. 악의가 풀리면 옛스킬로 돌아가야해."*).
///
/// One host block ([`Self::BLOCK_LEN`]), `hero2java::h2_demon_*`, `emulator::Demon` — installed AFTER every
/// block that wraps the words it chains (the berserker's `bl.a()Z` · `a.g()V`, the combat block's `bx.a()Z`,
/// the accessory / 일풍격 / rogue blocks' `ay.a()Z`, 일풍격's `a.d(B)Z`, and the gear block's piece in 철괴's
/// hook word), so that it is the outermost wrapper of each:
///
/// | word | wrapper |
/// |---|---|
/// | `a.f(B)V` 0x18bd30 (the beast key: `n.a(I)V` → this, r2 = the slot) | CAST: `bn.j[slot]` ≠ 9 → the chain; 9 → the game's own checks in its order (combat map `bf.a:Z` · `bn.a:B` 0 · `bn.k[slot] > 0` · SP ≥ `bn.b(B)I(slot)`), SP −= cost, `ag.a(IZ)V(slot, 1)`, STATE 2 · OWNER · UNTIL, the roar (the host voice, else the game's 사자후 효과음 5). The beast itself is never started — the demon is the player |
/// | `a.d(B)Z` 0x18b898 (the key arming, r2 = the quickbar value) | ARM: while STATE 2 on OWNER — value 0 (기본공격) refused during the recovery lockout and past the third stage (`a.i + 1 ≥ 3`); a value not in the class's [`Self::BUFF_VALUES`] → 0 (not armed: no SP, no cooldown, nothing cast); else the chain |
/// | `bl.a()Z` 0x188aa8 · `bx.a()Z` 0x189c44 · `ay.a()Z` 0x187404 (the class drives, r1 = this) | DRIVE ×3: STATE 2 · OWNER · the queued kind `a.o[a.i]` < 3 → the CLAW piece (the body never runs for those frames); else the chain |
/// | `a.g()V` 0x18b6bc (the character's update) | TICK: the chain, then for the player: TICKS += 1; STATE on another character → cleared (a reload); TICKS ≥ UNTIL → STATE 0 |
/// | 철괴's hook word (`CombatPatch::IRON_AT`'s HOOK) | TAKEN: r2 × 85 / 100 while STATE 2 on OWNER, then the previous hook (the gear's, the berserker's) |
/// | `bn.b(B)I` 0x188eec · `bn.a(B)I` 0x188ed0 (the beast's SP · cooldown by slot) | kind 9 → max SP >> 2 · 1800; else the chain. The caches `bn.b:[S` / `bn.a:[S` the game fills at a load (`bn.i()V` / `bn.h()V`) read them |
/// | ★ D2 `n.a(BBBII)V` 0x18d428 (the static sprite drawer every player draw ends in — r1 = the sequence, r2 = `g.w`, r3 = the direction, `[sp]` x · `[sp+4]` y; `a.a(BBII)V`, the stand `a.a(IIB)V`, the walk `a.e(II)V` and the fallen `a.d(II)V` all call it) | DRAW: STATE 1 → sequence [`Self::STAGE0_SEQ`] (the borrowed stage) becomes TRANSFORM_SEQ; STATE 2 · 3 with LOOK → the sequence is looked up in SEQ_TABLE (0xff = as it came); then the chain. Nothing else is touched |
///
/// ★★ D2 (2026-10-13): the LOOK. 악의 turns the character into the demon on screen: the demon's every picture is a
/// frame of the BODY sheet `hero/1/300` (`tools_re/demon_look.py`: frames 0..8 blank, 9.. the demon — 12 poses × 4
/// directions + 3 fallen), and each class animation carries, after its shipped sequences, a DEMON COPY of each of its
/// [`Self::MAPPED`] body sequences (the same frame count — `g.w` is the game's — with one body sprite in place of the
/// head · body · arms · legs · weapon · off-hand and the part-4 effects kept) plus a TRANSFORM sequence of human sprites
/// (crouch · leap · spin · land, [`Self::TRANSFORM_FRAMES`]). The cast is STATE 1 (transforming): the block arms the
/// game's own 기본공격 through `ag.c(B)V` (the OK key's path — the accessory block's wrapper honoured) with ARMING set so
/// its own ARM lets value 0 through, the TRANSFORM piece holds `a.n[0]` at TRANSFORM_FRAMES while that stage runs, and
/// the TICK piece restores `a.n[0]` at T1_UNTIL and goes to STATE 2 once the host says LOOK (or after T1_TIMEOUT
/// without it — then the demon fights in its human look, as D1). The host (`emulator::Demon` + `Avatar`) puts the body
/// override word at [`Self::COSTUME`] while STATE is 1 · 2, reloads the body through the game's own `cf.a(I)V`, and only
/// when that reload is done writes LOOK 1 (with the class's SEQ_TABLE and TRANSFORM_SEQ); the end is STATE 3
/// (reverting: the claw · the refusals · the damage cut are off, the LOOK stays) — the host writes LOOK 0 FIRST, then
/// reloads the human body, then STATE 0 (the TICK piece does it alone after T3_TIMEOUT). ⛔ A demon sequence names body
/// frames 9.. which a human body sheet (9 frames) does not have — the drawer throws on an index past the array — so the
/// LOOK protocol is what keeps the game alive: never LOOK 1 before the demon sheet is in, never a human sheet before
/// LOOK 0. Leaving the world (a death, the title) clears STATE and LOOK from the host before the next load's `cf.a()V`.
///
/// The CLAW piece (r1 = this, called at every frame of a stage): frame 0 clears LAST; a hit frame of the stage
/// ([`Self::HIT_FRAMES`]: stage 0 at 1 and 3, stages 1 · 2 at 2) not yet acted on → the attack (`a.a()I` through its
/// cell — the stage multiplier `(a.p[stage] + 9)/10` included, as the game's own 기본공격 — plus `지력' × 473/100`,
/// 지력' = `a.i:S` + `a.b[4]`), the targets (the stage's entity list: every live monster of the five classes whose
/// tile — `ad.d` · `ad.e` — lies in the stage's window, at most [`Self::SLOTS`]), one `a.a(Lbd;BIBB)Z` each
/// (definition word 0 read at call time — the 암살 wrapper is honoured; the monster's receive routine takes the
/// facing, DEF, element, the class damage function with kind = the stage, the crit roll); after the third stage's
/// hit RECOVER_UNTIL ← TICKS + [`Self::RECOVER`] (the ARM wrapper refuses a new combo until then — the user's
/// 「3 단 뒤 회복 10」, as a lockout rather than frames so the human sequence need not be lengthened before D2's
/// demon sequences). Answers 1 (the action goes on; the driver's `g.w == a.n[kind]` closes the stage).
///
/// Nothing is written to a record or a save; a block with STATE 0 is a jump on every word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DemonPatch {
    /// `(definition, body)`: `a.f(B)V` the beast key cast · `a.d(B)Z` the key arming · the three class drives
    /// `bl.a()Z` · `bx.a()Z` · `ay.a()Z` (버서커 · 나이트 · 로그) · `a.g()V` the character's update · `bn.b(B)I` the
    /// beast's SP by slot · `bn.a(B)I` its cooldown by slot.
    pub cast: (u32, u32),
    pub arm: (u32, u32),
    pub drives: [(u32, u32); 3],
    pub update: (u32, u32),
    pub beast_sp: (u32, u32),
    pub beast_cooldown: (u32, u32),
    /// ★ D2: `n.a(BBBII)V` — the static sprite drawer (definition, body); the DRAW piece chains onto its word.
    pub draw: (u32, u32),
    /// The definition record of `a.a(Lbd;BIBB)Z` (the character's hit on one monster — its word 0 is read at call time).
    pub hit_def: u32,
    /// ★ D2: the definition record of `ag.c(B)V` (the attack keys' arming — the OK key's path; `AccessoryPatch::arm.0`,
    /// whose wrapper the cast's arming goes through) — its word 0 is read at call time.
    pub key_arm: u32,
    /// ★ D2: the static `a.n:[B` (the action lengths by kind; `CombatPatch::frames_slot`).
    pub frames_slot: u32,
    /// ★ D3: `(definition, body)` of the ground draw `bf.a(Graphics;II)V` (`SkyDrawPatch::ground_draw`) and of the HUD
    /// painter `n.d()V` (`SkyDrawPatch::hud_draw`) — the GROUND and HUD pieces chain onto their words (the sky block's
    /// wrappers are under them).
    pub ground_draw: (u32, u32),
    pub hud_draw: (u32, u32),
    /// ★ D3: the Graphics calls (`SkyDrawPatch`'s five): the interface call helper · `ax.a` · the game's reference words
    /// for `setAlpha` · `setColor` · `fillRect` (`drawLine` has none — the block's own LINE_REF word).
    pub iface_call: u32,
    pub graphics_slot: u32,
    pub set_alpha_ref: u32,
    pub set_color_ref: u32,
    pub fill_rect_ref: u32,
    /// ★ D3: the cell of `bd.a(IB)V` (a monster's HP loss with no defence — the poison tick's way in, virtual through
    /// `dispatch`; `VenomPatch::cell_damage`) · the beast's `bn.d:S` (its power) · the quickbar values `ag.j:[B` and their
    /// counters `ag.a:[I` (the toggles' one-second lock) · an entity's world px `ad.a:S` / `ad.b:S` (the pictures).
    pub cell_mob_hurt: u32,
    pub beast_power_off: u32,
    pub quickbar_slot: u32,
    pub cooldown_slot: u32,
    pub px_x_off: u32,
    pub px_y_off: u32,
    /// Helpers: virtual dispatch · static call · divide.
    pub dispatch: u32,
    pub call_static: u32,
    pub divide: u32,
    /// Cells: `ag.a(IZ)V` (the cooldown start — `bn.a(BBBB)V`'s own) · `br.a(B)V` (a sound) · `a.a()I` (the attack).
    pub cell_cd_start: u32,
    pub cell_sound: u32,
    pub cell_attack: u32,
    /// Statics: the character `ag.a:La;` · the stage `ag.a:Lbf;` · the class `ag.a:B` · the combat-map flag `bf.a:Z`.
    pub char_slot: u32,
    pub stage_slot: u32,
    pub class_slot: u32,
    pub combat_slot: u32,
    /// The monster classes' descriptors (`AccessoryPatch::monster_descs`).
    pub monster_descs: [u32; 5],
    /// Absolute field offsets — the character's `a.a:Lbn;` (the beast) · the beast's `bn.j:[B` (skill ids) · `bn.k:[B`
    /// (levels) · `bn.a:B` (its state; 0 = idle) · the character's `a.b:I` SP · `a.e:I` max SP · `a.i:S` 지력 · `a.b:[S`
    /// (the equipment's ability bonus) · `a.o:[B` / `a.i:B` (the action queue and its index) · `g.w` (the action frame)
    /// · `g.u` (facing) · `g.t` (state) · an entity's `ad.d` / `ad.e` (its tile) · `a.l:B` / `a.m:B` (the hit's two
    /// bytes) · an entity's next `ad.b` · the stage's list head.
    pub beast_off: u32,
    pub beast_ids_off: u32,
    pub beast_levels_off: u32,
    pub beast_state_off: u32,
    pub sp_off: u32,
    pub max_sp_off: u32,
    pub int_off: u32,
    pub bonus_off: u32,
    pub queue_off: u32,
    pub queue_at_off: u32,
    pub frame_off: u32,
    pub facing_off: u32,
    pub state_off: u32,
    pub tile_x_off: u32,
    pub tile_y_off: u32,
    pub attr_l_off: u32,
    pub attr_m_off: u32,
    pub next_off: u32,
    pub stage_head_off: u32,
    /// ★ D4: `(definition, body)` of the three class damage functions `bl.a(BILbd;)I` · `bx.a(BILbd;)I` · `ay.a(BILbd;)I`
    /// (`GearPatch::damage`) — the DMG pieces chain onto their words (the gear's wrappers under them); the definition
    /// records of `a.b(I)V` (the HP delta — `BerserkerPatch::hp_delta_def`), `a.a(Lbd;BI)V` (a status on a monster: 25 =
    /// the stun) and `g.b()Z` (the walk's step — the update's own call), each read at call time; the stage's
    /// `bf.a(Lg;II)Z` and a monster's `bd.a(SS)V` bodies (`AccessoryPatch::{body_can_stand, body_relocate}`); an entity's
    /// width (+0x20, `AccessoryPatch::WIDTH_OFF`).
    pub damage: [(u32, u32); 3],
    pub hp_delta_def: u32,
    pub status_def: u32,
    /// ★★ 2026-10-16 (악의 무기 M4) the cell of `g.a(B)Z` — can this status be added (false while it runs; `VenomPatch::cell_has_status`):
    /// the claw asks it of a monster for the poison (26) before its poisoned add.
    pub cell_has_status: u32,
    pub step_def: u32,
    /// ★★ 2026-10-17 (user: 악의 중 퀘스트 대화가 안 될 수 있나 → 고쳐): the definition of `a.i(B)V` — what the update `a.g()V` runs
    /// after its own step (`HERO2_GUEST_RE.md` §8.97): the arrival check `bb.a(La;)Z` (an auto group on the tile the walker
    /// stands on — 도착 컷신 · NPC 앞 칸 대사 · 밟는 계단) once a step cleared the character's `a.v:Z`, and the bump `bb.b()Z` (a
    /// door · a 잠긴 문's line) when a walking step was blocked. The extra step runs it after its own step the same way (its
    /// argument 2 = walking), clearing `a.v:Z` ([`Self::moved_off`]) when it moved — without it the tile the extra step reached
    /// was never checked and a door it stopped at was never bumped.
    pub arrive_def: u32,
    /// `a.v:Z`'s absolute field offset (declared +0xf0 after the 0x5c parent) — 0 = this tile's arrival is still to be checked.
    pub moved_off: u32,
    /// ★★ 2026-10-17 the state word (`AppState::Absolute` addr — 4 = a script is up: a dialog · a cutscene). The TICK piece
    /// stops the demon's clock there (user: *"컷신중에는 … 타이머나 sp소모 멈추게"*).
    pub app_state_word: u32,
    pub body_can_stand: u32,
    pub body_relocate: u32,
    pub width_off: u32,
    /// ★ D4: the field's walk request `ag.k:B` (`Hero2Java::walk_request_slots.0` — 1 = stop · 2 = turn, applied at the next
    /// aligned tile; 0 = none). The extra step runs only while it is 0, so a release or a turn lands where it would have.
    pub walk_request_slot: u32,
    /// ★ D5: `(definition, body)` of the beast's ring spawner `bn.a(BIBI)V` (r1 = the beast · r2 = the tier · r3 = the damage ·
    /// `[sp]` the direction 1..12 · `[sp+4]` extra — one `ce` on `beast tile + co[dir]`; `AccessoryPatch::ring_spawn`, whose
    /// BEAST_RANGE wrapper sits under this block's LAVA piece) · the direction tables `co.a:[B` (dx) · `co.b:[B` (dy) the spawner
    /// adds · the map's size in tiles `bf.d:B` · `bf.e:B` (`AccessoryPatch::map_w_slot` · `map_h_slot`).
    pub spawn: (u32, u32),
    /// ★★ 2026-10-15 (신규 아이템 2차 C5): `(definition, body)` of `bd.q()V` — a monster's death (`GearPatch::kill`: the gear
    /// block's wrapper sits under this block's DKILL piece — the demon installs after it).
    pub kill: (u32, u32),
    /// ★ 2026-10-16: `(definition, body)` of `bd.a(IB)V` — a monster's HP loss (every monster class shares the one definition
    /// record — no override; nothing else wraps its word today, so the MOBHP piece's chain is the body).
    pub mob_damage: (u32, u32),
    pub dir_dx_slot: u32,
    pub dir_dy_slot: u32,
    pub map_w_slot: u32,
    pub map_h_slot: u32,
    /// ★★ 2026-10-15: the live stage's map number — a byte at this absolute field offset of `ag.a:Lbf;` (`stage_slot`; the
    /// minimap's `Hero2SaveArea::stage_type_off`, `h2_stage_map_no`). The lava keeps the map it was cast on (`LAVA_MAP`).
    pub stage_map_off: u32,
}

impl DemonPatch {
    /// 대지분노's beast skill id — 악의.
    pub const KIND: u8 = 9;
    /// 60 s on, the slot's cooldown 90 s from the cast (user ④).
    pub const FRAMES: u32 = 1200;
    pub const COOLDOWN: u32 = 1800;
    /// SP = max SP >> 2.
    pub const SP_SHIFT: u16 = 2;
    /// Received damage × 85 / 100 (−15 %).
    pub const TAKEN_NUM: u8 = 85;
    pub const TAKEN_DEN: u8 = 100;
    /// The claw: `a.a()I + 지력' × 473 / 100` — M = (722 − 230) / 104 for the LV 85 benchmark (an all-힘 berserker's
    /// 722 · the 체1의1지1 knight's 230 and 지력' 104; `tools_re/demon_balance.py` §1, user ② 「1.0 배」 · ① 「공격력
    /// 안 줄여도 돼」).
    pub const CLAW_INT_NUM: u32 = 473;
    pub const CLAW_INT_DEN: u8 = 100;
    /// The claw combo's stages (the game's own `a.h` opens fewer when fewer are learned).
    pub const STAGES: u8 = 3;
    /// The hit frames of each stage (`g.w` as the drive sees it): 0 → two claws at 1 and 3 · 1 → the sweep-up at 2 ·
    /// 2 → the slam at 2. The stages keep the human sequences' lengths (`a.n[0..3]`, 5 each as shipped).
    pub const HIT_FRAMES: [&'static [u8]; 3] = [&[1, 3], &[2], &[2]];
    /// Ticks after the third stage's hit before another 기본공격 is armed (user ③ 「17 + 회복 10」).
    pub const RECOVER: u32 = 10;
    /// ★★ 2026-10-15 (user: *"맞는 영역은 항상 이펙트보다 넓은 범위여야해 … 앞에 1.5칸을 긁는 이펙트 … 데미지가 들어가는 영역은 2칸"*):
    /// every window is the effect's footprint in tiles, FLOORED AND + 1 (`tools_re/demon_fx.py footprint`, the right-facing
    /// cuts). The claw's three stages: 1..=depth tiles ahead by the facing, ±half aside — the X (5..38 px · ±27) → (3, 2) ·
    /// the crescent (9..43 · −42..7) → (3, 2) · the rend (13..55) → (4, 2). The old 5×5 slam is gone: the third stage rakes
    /// the ground ahead like the others.
    pub const CLAW_WINDOWS: [(u8, u8); 3] = [(3, 2), (3, 2), (4, 2)];
    /// Target slots a hit frame fills at most (the rend's 4 × 5 window holds 20 tiles).
    pub const SLOTS: u8 = 25;
    /// The game's 사자후 shout (`P/snd/45`) when the host has no voice clip.
    pub const ROAR_SOUND: u8 = 5;
    /// STATE: 0 idle · 1 transforming (★ D2: the cast's borrowed stage plays, the host loads the demon body) · 2 demon ·
    /// 3 reverting (★ D2: the look stays while the host puts the human body back; nothing else of 악의 is on).
    pub const STATE_TRANSFORM: u32 = 1;
    pub const STATE_ON: u32 = 2;
    pub const STATE_REVERT: u32 = 3;
    /// ★ D2: the transform — the borrowed 기본공격 stage 0 held to this many frames (`a.n[0]`), then T1_GRACE ticks more before
    /// the TICK piece restores `a.n[0]` and looks for the host's LOOK; without it after T1_TIMEOUT more, STATE 2 anyway
    /// (the human look — the sheet is missing or the host is gone). T3_TIMEOUT: STATE 3 ends itself when the host
    /// never says STATE 0.
    pub const TRANSFORM_FRAMES: u8 = 20;
    pub const T1_GRACE: u32 = 4;
    pub const T1_TIMEOUT: u32 = 60;
    pub const T3_TIMEOUT: u32 = 100;
    /// ★ D2: the demon costume number — the body sheet `hero/1/300` the host puts in the avatar override while the state
    /// runs — and where its frames begin (0..8 are the human body's places, blank).
    pub const COSTUME: i32 = 300;
    pub const BODY_BASE: u8 = 9;
    /// ★★ 2026-10-15 HD: the BLANK body sheet `hero/1/301` (the same 9 + 115 frames, 1×1 transparent each) the host puts in
    /// under HD mode — the game's drawer draws nothing for the body, the host composites the HD body over the frame.
    pub const COSTUME_HD: i32 = 301;
    /// The body's part (the animation piece's part byte) — the demon copies' one body piece is `(1, BODY_BASE + k, dx, dy)`.
    pub const BODY_PART: u8 = 1;
    /// ★ D2: the sequence the borrowed stage 0 draws (kind 0 + 4) — the TRANSFORM sequence stands in for it in STATE 1.
    pub const STAGE0_SEQ: u8 = 4;
    /// ★ D2: per class, how many sequences the class animation has before `demon_look.py` appends its own (the asset
    /// pipeline's other patches included — berserker_taunt · awakenings / knight_iaido · awakenings / rogue_* · awakenings),
    /// and WHICH human sequences get a demon copy, in the order the copies are appended (the asset's `MAPPED` — the two
    /// lists are one contract): demon sequence of `s` = SHIPPED_SEQS + its place in MAPPED; the transform sequence follows
    /// them at SHIPPED_SEQS + MAPPED_COUNT.
    pub const SHIPPED_SEQS: [u8; 3] = [23, 27, 23];
    pub const MAPPED_COUNT: usize = 18;
    pub const MAPPED: [[u8; Self::MAPPED_COUNT]; 3] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 17, 19, 20, 21],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 18, 19, 23, 25],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 19, 21],
    ];
    /// ★ D2: the sequence table the host writes — one byte per human sequence 0..63 (0xff = drawn as it came).
    pub const SEQ_TABLE_LEN: usize = 64;
    pub const SEQ_NONE: u8 = 0xff;
    /// ★ D2: the HUD buff cell — 악의's icon is the beast strip's cell 9 (`img/bico` frame 9, redrawn by the asset), the id
    /// a number no game buff uses (`GearPatch::DRAGON_BUFF_ID` is 200).
    pub const BUFF_ID: u8 = 201;
    pub const ICON_CELL: u8 = 9;
    /// The quickbar VALUES (as `a.d(B)Z` receives them — before 일풍격's remap) a class may still arm while 악의 runs:
    /// its buffs only (user: *"일반 스킬은 버프기 밖에 사용못해"*). 버서커 분노태세 4 · 반격태세 5 · 도발 8 / 나이트 치유오라 1
    /// · 신의가호 5 · 철괴 6 / 로그 민첩태세 2 · 맹독감염 4 (0 = no entry). Value 0 (기본공격) is the claw combo.
    pub const BUFF_VALUES: [[u8; 3]; 3] = [[4, 5, 8], [1, 5, 6], [2, 4, 0]];
    /// ★★ D3 (2026-10-13): the two TOGGLES while 악의 runs — the beast's 암살자 (kind [`Self::BLADE_KIND`]) is 지옥칼날, its
    /// 심판번개 (kind [`Self::JUDGE_KIND`]) is 어둠심판 (user ⑫: only while 악의 runs — outside it, and the moment it ends, both
    /// are the beast's own skills again). A press of the slot while 악의 runs on this character flips the toggle (the
    /// TOGGLE piece, reached from the CAST piece): on — one second's SP paid up front, the words set; off — the words
    /// cleared; either way [`Self::TOGGLE_CD`] on the slot's quickbar counter (the beast's own cooldown table is not
    /// consulted — it would lock the toggle for seconds). The TICK piece calls the BLADE and JUDGE pieces each update of the
    /// demon and clears both toggles when 악의 ends (the minute, a re-press, another character); each piece turns itself off
    /// when SP runs short or the map is not a combat map.
    pub const BLADE_KIND: u8 = 3;
    pub const JUDGE_KIND: u8 = 10;
    /// 지옥칼날: every live monster within the seal (BLADE_WINDOW — ★ 2026-10-15 an ellipse 6 wide · 3 tall in tiles, the seal's
    /// picture ±82 × −44..20 px + 1; before, a ±3 square) of the demon takes BLADE_PCT % of the 암살자 status tick's
    /// damage ([`Self::assassin_tick`]) every BLADE_EVERY ticks — ten ticks a second = 70 %/s (user ⑤) — through `bd.a(IB)V`
    /// (no defence, as the status tick itself; a kill is the game's own fall · EXP · drop); SP: BLADE_SP_PCT % of max SP a
    /// second (user ⑦, [`Self::SP_EVERY`]).
    pub const BLADE_WINDOW: u32 = Self::window(6, 3, true);
    pub const BLADE_EVERY: u32 = 2;
    /// ★★ 2026-10-16 (user: *"지옥칼날 SP소모량을 올려줘. 의지가 얼마 안 올라갔는데도 불구하고 무한지속이 되고있어. 그리고 데미지도 좀 더 낮춰줘"*): the tick 7 % → **5 %**
    /// (50 %/s), the drain 6 % → **12 %** of max SP a second **and never under BLADE_SP_MIN a second** — a low-의지 character's
    /// max SP is small, so a plain percentage was under its natural regeneration and the toggle ran for ever (the floor is
    /// Claude's number). 어둠심판 (8 %, no floor) is as it was.
    pub const BLADE_PCT: u8 = 5;
    pub const BLADE_SP_PCT: u8 = 12;
    pub const BLADE_SP_MIN: u8 = 15;
    /// 어둠심판: a monster entering the ±JUDGE_HALF (9×9) is struck at once, then every JUDGE_EVERY ticks (0.2 s — the user's
    /// 0.22 s on the 20 fps grid), each strike JUDGE_PCT % of the heavy attack's damage ([`Self::heavy_hit`], user ⑥);
    /// JUDGE_WINDUP ticks (1 s — 「손 들고 주먹」, drawn as spokes closing in) pass before the first strike; SP JUDGE_SP_PCT %/s.
    pub const JUDGE_HALF: u8 = 4;
    pub const JUDGE_EVERY: u32 = 4;
    pub const JUDGE_PCT: u8 = 20;
    pub const JUDGE_WINDUP: u32 = 20;
    pub const JUDGE_SP_PCT: u8 = 8;
    pub const SP_EVERY: u32 = 20;
    /// A toggle (on or off) puts this many ticks on the slot's quickbar counter: one second between presses (the game's
    /// `n.a(I)V` refuses the key while a counter runs).
    pub const TOGGLE_CD: u32 = 20;
    /// The judgement table: JUDGE_SLOTS entries of `(handle, next strike tick, last strike tick, seen this tick)`.
    pub const JUDGE_SLOTS: u32 = 32;
    pub const JUDGE_ENTRY: u32 = 16;
    /// Targets a tick collects at most (the COLLECT piece's table in the caller's frame).
    pub const TOGGLE_TARGETS: u8 = 25;
    /// The HUD cells while a toggle runs (ON, no seconds): the beast strip's 암살자 and 심판번개 cells.
    pub const BLADE_BUFF_ID: u8 = 202;
    pub const JUDGE_BUFF_ID: u8 = 203;
    /// A tile's centre on the ground from an entity's world px: `(px + 8, py + 10)` (`SkyDrawPatch::FLOOR_FEET`).
    pub const FEET: (u8, u8) = (8, 10);
    /// The block: the state words, then the pieces (★ D3 0x800 → 0x1600 · ★ D4 → 0x2640: the D4 words and pieces after the
    /// HUD piece · ★ D5 → 0x3740: the D5 words · tables · pieces after the BLASTD piece · ★ (5) → 0x3a00: the POSE words and
    /// the DRAW piece).
    /// ★★ 2026-10-16 (악의 무기) → 0x4000: the CLAW piece (720 B with M2 · M3 · M4) moved behind the COLLECT piece to
    /// [`Self::CLAW_AT`], the ARM piece (M5) into the claw's old room.
    pub const BLOCK_LEN: u32 = 0x4000;
    /// The character 악의 runs on (its handle): another character in the world clears the state (a reload).
    pub const OWNER: u32 = 0x00;
    pub const STATE: u32 = 0x04;
    /// TICKS at which it ends (the TICK piece's clock — the character's updates).
    pub const UNTIL: u32 = 0x08;
    pub const TICKS: u32 = 0x0c;
    /// The last action frame the claw acted on (−1 = none; frame 0 of a stage resets it).
    pub const LAST: u32 = 0x10;
    /// TICKS before which value 0 is not armed (the recovery after the slam).
    pub const RECOVER_UNTIL: u32 = 0x14;
    /// Non-zero = the host plays the roar (the cast leaves the game's 효과음 out). Written at the install.
    pub const VOICE: u32 = 0x18;
    /// Counts for the probe: claw hits that landed · casts · ends.
    pub const HITS: u32 = 0x1c;
    pub const CASTS: u32 = 0x20;
    pub const ENDS: u32 = 0x24;
    /// ★ D2 words: LOOK (the host: 1 = the demon body sheet is in — the DRAW piece maps sequences only then) · T1_UNTIL
    /// (STATE 1 ends its stage hold here) · T3_UNTIL (STATE 3 gives up here) · HUMAN_N0 (`a.n[0]` as it was at the cast,
    /// −1 = nothing to restore) · ARMING (1 while the cast arms the stage through `ag.c(B)V` — the ARM piece's pass) ·
    /// TRANSFORM_SEQ (the host: this class's transform sequence) · SEQ_TABLE (the host: 64 bytes, human → demon sequence).
    pub const LOOK: u32 = 0x28;
    pub const T1_UNTIL: u32 = 0x2c;
    pub const T3_UNTIL: u32 = 0x30;
    pub const HUMAN_N0: u32 = 0x34;
    pub const ARMING: u32 = 0x38;
    pub const TRANSFORM_SEQ: u32 = 0x3c;
    /// ★ D3 words (0x40..0x80 — within an `ldr rt,[rn,#imm]` of the block register): the two toggles (0 off · 1 on) · the
    /// tick each next drains SP at · the blade's next hit tick · the tick the judgement's wind-up ends at · hit counts ·
    /// the beast slot each toggle was pressed from · toggle presses (the probe) · the last tick damages (the probe).
    pub const BLADE: u32 = 0x40;
    pub const JUDGE: u32 = 0x44;
    pub const BLADE_SP_AT: u32 = 0x48;
    pub const JUDGE_SP_AT: u32 = 0x4c;
    pub const BLADE_HIT_AT: u32 = 0x50;
    pub const JUDGE_FROM: u32 = 0x54;
    pub const BLADE_HITS: u32 = 0x58;
    pub const JUDGE_HITS: u32 = 0x5c;
    pub const BLADE_SLOT: u32 = 0x60;
    pub const JUDGE_SLOT: u32 = 0x64;
    pub const TOGGLES: u32 = 0x68;
    pub const BLADE_DMG: u32 = 0x6c;
    pub const JUDGE_DMG: u32 = 0x70;
    /// ★★ 2026-10-14: the claw's picture cue — the CLAW piece, at each hit, writes `(n << 8) | effect` (n counts the hits, the
    /// effect is `FX_CLAW + cut·4 + facing − 1`); the FXT piece plays that effect draw by draw from a new cue (CLAW_LAST ·
    /// CLAW_AGE), at the demon's feet. A stage is 4..5 updates and a paint can skip some, so the picture is cued by the hit
    /// and aged by the painter (as the blast) rather than read off `g.w` — every frame of a cut is seen.
    pub const CLAW: u32 = 0x74;
    pub const CLAW_LAST: u32 = 0x78;
    pub const CLAW_AGE: u32 = 0x7c;
    pub const SEQ_TABLE: u32 = 0x80;
    /// ★ D3 (past the table, reached by address): the block's own `drawLine` reference word (`SkyDrawPatch::LINE_REF_WORD`)
    /// · the chains of the ground draw and the HUD painter · the camera the ground draw saw (world px + camera = screen px)
    /// · the drawing pieces' scratch (a centre, a table pointer) · the judgement table.
    pub const LINE_REF: u32 = 0xc0;
    pub const CHAIN_GROUND: u32 = 0xc4;
    pub const CHAIN_HUD: u32 = 0xc8;
    pub const CAM_X: u32 = 0xcc;
    pub const CAM_Y: u32 = 0xd0;
    pub const CX: u32 = 0xd4;
    pub const CY: u32 = 0xd8;
    pub const EPTR: u32 = 0xdc;
    pub const JUDGE_TAB: u32 = 0xe0;
    /// The pieces (D1 sizes: the cast 260 B · the arm 116 · SP 56 · the cooldown 40 · TAKEN 60 · the tick 76 · a drive stub
    /// 64 · the claw 424; ★ D2 the cast and the tick grow, the draw and the transform are new; ★ D3 the cast gains the
    /// toggle dispatch, the tick the two calls, and TOGGLE · COLLECT · BLADE · JUDGE · GROUND · HUD are new) — each
    /// measured against its room by `demon_piece_sizes_fit_their_rooms`.
    pub const CAST_AT: u32 = 0x2e0;
    /// ★★ 2026-10-16 0x4a0 → 0x820 (the claw's old room; M5's 둔기강타 check took it past its 160 B — 0x4a0..0x540 is empty now).
    pub const ARM_AT: u32 = 0x820;
    pub const SP_AT: u32 = 0x540;
    pub const CD_AT: u32 = 0x580;
    pub const TAKEN_AT: u32 = 0x5c0;
    pub const TICK_AT: u32 = 0x600;
    /// The three drive stubs (버서커 · 나이트 · 로그) and the claw they share.
    pub const DRIVE_AT: [u32; 3] = [0x700, 0x760, 0x7c0];
    /// ★★ 2026-10-16 0x820 → 0x3c40, behind the COLLECT piece (악의 무기 grew it to 720 B — room to BLOCK_LEN).
    pub const CLAW_AT: u32 = 0x3c40;
    /// ★ D2: the DRAW piece (the sprite drawer's wrapper) and the TRANSFORM piece (the drive of the borrowed stage).
    /// ★★ 2026-10-14 (5): the DRAW piece moved past the D5 room to [`Self::D6`] (it grew the POSES; GROUND now uses 0xa00..0xa60).
    pub const DRAW_AT: u32 = 0x3780;
    pub const TRANSFORM_AT: u32 = 0xa60;
    /// ★ D3: the TOGGLE piece (a press of kind 3 · 10 while 악의 runs) · the COLLECT piece (the live monsters within a square
    /// of the demon, into the caller's table) · the BLADE and JUDGE ticks · the GROUND piece (the ground draw's wrapper:
    /// the camera, the blades, the wind-up) · the HUD piece (the HUD painter's wrapper: the bolts).
    /// ★★ 2026-10-24 0xa80 → 0x3580, in the FXT piece's spare room (the switch-on sound took it past its 320 B; 0xa80..0xbc0
    /// is the TRANSFORM piece's spare room now).
    pub const TOGGLE_AT: u32 = 0x3580;
    /// ★★ 2026-10-24 THE SOUNDS (user: *"현재 악의 사용후 변경되는 스킬중에 효과음 안나는거 전수조사해서 효과음 부여해줘"*). The survey of
    /// what 악의 changes — the claw (`br.a(B)V(1)`, 2026-10-23), 추종 · 돌진 · 멸화's casts (BUFF_SOUND) and 멸화's detonation
    /// (FIRE_SOUND) sounded; 지옥칼날 and 어둠심판 made none at all (the TOGGLE piece switches them on silently and their ticks
    /// hurt through `bd.a(IB)V`, which plays nothing — the shipped skills' hits went through the ring `ce`, whose hits play
    /// `br.a(B)V(3)`), and neither did 돌진's hits, 추종's end blast or 용암불's burning. The SND piece (r0 = a cue below; r4..r7
    /// kept) plays its sound through the game's `br.a(B)V`; a cue with an interval waits that many TICKS since it last played
    /// (its word at `SND_WORDS + 4 · cue` holds the tick it may play again — a clock gone back plays at once), so a hit every
    /// other tick sounds a few times a second, not ten. 어둠심판's strikes are the host's thunder (`H2ThunderWatch` — the game
    /// has no such sound).
    pub const SND_AT: u32 = 0x3500;
    /// The throttled cues' words (three free words before CHARGE_TAB — the throttled cues come first, see `SND_CUES`).
    pub const SND_WORDS: u32 = 0x1674;
    /// How many cues have an interval (the first ones — their words are the three at SND_WORDS).
    pub const SND_THROTTLED: usize = 3;
    pub const SND_BLADE: u8 = 0;
    pub const SND_CHARGE: u8 = 1;
    pub const SND_LAVA: u8 = 2;
    pub const SND_TOGGLE: u8 = 3;
    pub const SND_BLAST: u8 = 4;
    /// The monster-hit sound (`cf.a(B)V` fills slot 4 with 51 · 52 · 53 by the weapon — the hit routines `ah` · `bd` play it).
    pub const HIT_SOUND: u8 = 4;
    /// Per cue `(the br.a(B)V slot, ticks between two)`: 지옥칼날's cuts and 돌진's hits = the hit sound (0.4 s · 0.3 s) ·
    /// 용암불's burning = the ring's 속성공격합 (0.5 s) · a toggle switched on = the beast's buff sound · 추종's blast = 속성공격합
    /// (멸화's detonation's own).
    pub const SND_CUES: [(u8, u8); 5] =
        [(Self::HIT_SOUND, 8), (Self::HIT_SOUND, 6), (Self::FIRE_SOUND, 10), (Self::BUFF_SOUND, 0), (Self::FIRE_SOUND, 0)];
    /// ★★ 2026-10-15 HD: the COLLECT piece grew its ellipse test past the 0xc0 room at 0xbc0 → it moved behind the DRAW piece
    /// (0x3b00..0x3c00); 0xbc0..0xc80 is the TOGGLE piece's spare room now (★★ 2026-10-24 RESET · SLOTCD's — TOGGLE moved).
    pub const COLLECT_AT: u32 = 0x3b00;
    /// ★★ 2026-10-16: two small pieces in that spare room — RESET (the toggles · 추종 · 돌진 · 멸화 words to 0, called by a
    /// fresh cast and by the tick's “another character” path — user: *"지옥칼날이 on/off가 세이브 슬롯이 다른데도 적용되고있어"*) and
    /// SLOTCD (r1 = the beast slot · r2 = the counter: the quickbar cell holding `10 + slot` ← r2 — 돌진's own cooldown).
    pub const RESET_AT: u32 = 0xbc0;
    pub const SLOTCD_AT: u32 = 0xc00;
    /// The words the RESET piece (and the host's out-of-world write) zero: BLADE · JUDGE · FOLLOW · CHARGE · FIRE.
    pub const RESET_WORDS: [u32; 5] = [Self::BLADE, Self::JUDGE, Self::FOLLOW, Self::CHARGE, Self::FIRE];
    pub const BLADE_AT: u32 = 0xc80;
    pub const JUDGE_AT: u32 = 0xe00;
    pub const POWER_AT: u32 = 0x900;
    pub const GROUND_AT: u32 = 0xa00;
    /// ★★ 2026-10-15 신규 아이템 2차 C (악의 장비) — in the GROUND piece's spare room: GEAR_PTR (the gear block, written at this
    /// block's install — its effect words W_DMN_CLAW.. are read through it; 0 = no gear block: every op the shipped path) ·
    /// GEAR_KILLS (the kills that lengthened 악의, for the probe) · the DGEAR piece (r0 = the op, r1 = its argument → r0; r4..r7
    /// kept): OP_CLAW the claw's attack + attack × W_DMN_CLAW >> 8 (C1 · C3) · OP_FAST the claw's extra-frame sources (추종 at
    /// the full gauge + W_DMN_FAST — C1, each a fifth of a frame) · OP_RUN the extra step's mask (1 = every other tick: 돌진 or
    /// 추종 at the full gauge · 3 = every fourth: W_DMN_RUN while STATE 2 — C2 · 0 none) · OP_DUR the cast's frames + W_DMN_DUR
    /// (C4) · OP_KILL while STATE 2, UNTIL + W_DMN_KILL (C5 — ★★ 2026-10-17 no ceiling; it stopped at 75 s left) · the DKILL piece (the `bd.q()V`
    /// wrapper: the death as before, then OP_KILL).
    pub const GEAR_PTR: u32 = 0x1000;
    pub const GEAR_KILLS: u32 = 0x1004;
    /// ★★ 2026-10-15 (2차 C1 악마의 손톱 → 「기본공격 속도 +20%」, user: *"악의 시 공격속도 옵션도 기본 공격 속도"*): the FASTX piece's
    /// count of 기본공격 frames toward the next extra one (FOLLOW_SWING_EVERY — a fifth more).
    pub const FAST_TICK: u32 = 0x1008;
    /// ★★ 2026-10-17 (user: *"야수 악의 시 검풍의 기본공격시 검기 발동 옵션이 적용이 안되고있어. 악의 기본공격 시에서 기본공격시 발동되는
    /// 옵션들이 발동되어야해"*): the combat block's WIND piece (`CombatPatch::WIND_AT` | 1, written at this block's install; 0 = no
    /// combat block) — the CLAW piece calls it for a knight at each claw stage's first hit, as the knight's cast wrapper does after
    /// each 기본공격 stage's hit (the piece itself asks whether 검풍 is worn).
    pub const WIND_PTR: u32 = 0x100c;
    pub const DGEAR_AT: u32 = 0x1010;
    pub const DKILL_AT: u32 = 0x1200;
    /// ★★ 2026-10-15 the FASTX piece — every class drive's way out to its chain (r0 = the chain | 1 · r1 = this · r2 = the class's
    /// 기본공격 stages): the chain for this frame; then, a 기본공격 stage queued and W_DMN_FAST worn (the gear block through
    /// GEAR_PTR), every FOLLOW_SWING_EVERYth such frame a second frame (`g.w + 1` under `a.n[kind] − 1`, the chain again; a fifth
    /// with no room — a stage's last frames — holds its credit for the next frame that has) — the
    /// claw's own extra frames (CLAWX, OP_FAST) are 악의's; this is every other time. Answers the last chain's answer.
    pub const FASTX_AT: u32 = 0x1240;
    pub const OP_CLAW: u8 = 0;
    pub const OP_FAST: u8 = 1;
    pub const OP_RUN: u8 = 2;
    pub const OP_DUR: u8 = 3;
    pub const OP_KILL: u8 = 4;
    /// ★★ 2026-10-15 (2차 C5 풀린 목줄) a hit taken while 악의 runs, after its own × 85/100: d − d × W_DMN_TAKEN >> 8 (the TAKEN piece
    /// asks; no gear block / none worn = d).
    pub const OP_TAKEN: u8 = 5;
    /// The extra step's masks: a step on the ticks with `STEP_TICK & mask == mask`.
    pub const RUN_HALF: u8 = 1;
    pub const RUN_QUARTER: u8 = 3;
    pub const HUD_AT: u32 = 0x1300;
    /// ★★ 2026-10-17 the HOLD piece (the clock stands: a script up or [`Self::PAUSE`] — `h2_dmn_hold`), in the HUD piece's
    /// spare room before the D4 words. ★★ 2026-10-18 0x15c0 → 0x2a40, the LAVA piece's old room (LAVA took the HUD piece's).
    pub const HOLD_AT: u32 = 0x2a40;
    /// ★★ D4 (2026-10-13): 추종 (피의계약, kind [`Self::FOLLOW_KIND`]) and 돌진 (암흑폭발, kind [`Self::CHARGE_KIND`]) while
    /// 악의 runs (user ⑫) — the beast's own skills outside it. 추종: a minute's buff ([`Self::FOLLOW_FRAMES`]) whose gauge
    /// (0..=100) fills — ★ 2026-10-16 (user) — by [`Self::FOLLOW_GAIN_CLAW`] % per landed claw hit, [`Self::FOLLOW_GAIN_TAKEN`] %
    /// per hit taken (the TAKEN piece) and [`Self::FOLLOW_GAIN_DEALT`] % per other damage a monster takes (the MOBHP piece on
    /// `bd.a(IB)V`'s definition — every monster's HP loss passes it; the DMG pieces on `<job>.a(BILbd;)I` mark a claw kind's
    /// hit in CLAW_MARK on its way there; before: 25 claw hits = 100 %);
    /// at 100 % the walk gains a step every other tick (+50 %: `g.b()Z` once more in the update's tick), the claw a frame
    /// every [`Self::FOLLOW_SWING_EVERY`]th (+20 %: the CLAWX piece runs the CLAW twice), and every claw hit returns
    /// [`Self::FOLLOW_LEECH_PCT`] % as HP through `a.b(I)V` (user ⑧); the end (the minute, a re-press, 악의's own end) is
    /// the DARK BLAST (the FEND piece): every live monster on the stage takes the heavy attack × [`Self::FOLLOW_BLAST_MUL`]
    /// (`bd.a(IB)V`, no defence) and a plain one is stunned [`Self::FOLLOW_STUN_FRAMES`] (the game's status 25 through
    /// `a.a(Lbd;BI)V` — a boss never, user ⑧), the quickbar counter of the slot then holds [`Self::FOLLOW_COOLDOWN`]. 돌진:
    /// [`Self::CHARGE_FRAMES`] of the +50 % walk; every tick the live monsters on the 3×3 around the demon take the heavy
    /// attack once per [`Self::CHARGE_REHIT`] ticks each (a table of [`Self::CHARGE_SLOTS`] `(handle, until)`) and a plain
    /// one-tile monster is pushed a tile along the facing when it can stand there (the 방패치기 knockback's moves —
    /// `bf.a(Lg;II)Z` then `bd.a(SS)V`). A cast's SP and the slot's counter are the beast's own (`bn.b(B)I` through its
    /// word · `ag.a(IZ)V`); a cast while 돌진 runs does nothing; a press while 추종 runs ends it (the blast).
    pub const FOLLOW_KIND: u8 = 2;
    pub const CHARGE_KIND: u8 = 12;
    pub const FOLLOW_FRAMES: u32 = 1200;
    /// ★ 2026-10-16 (user): the gauge's gains — a landed claw hit (a kind below [`Self::STAGES`]) · a hit taken · any other
    /// damage a monster takes.
    pub const FOLLOW_GAIN_CLAW: u8 = 4;
    pub const FOLLOW_GAIN_TAKEN: u8 = 2;
    pub const FOLLOW_GAIN_DEALT: u8 = 2;
    pub const FOLLOW_GAUGE_FULL: u8 = 100;
    pub const FOLLOW_LEECH_PCT: u8 = 5;
    pub const FOLLOW_SWING_EVERY: u32 = 5;
    pub const FOLLOW_BLAST_MUL: u8 = 3;
    /// ★★ 2026-10-15: the blast hits only within its picture — an ellipse 6 wide · 4 tall in tiles (the burst ±77..82 × −103..29
    /// px + 1); before, every live monster on the stage.
    pub const BURST_WINDOW: u32 = Self::window(6, 4, true);
    pub const FOLLOW_STUN: u8 = 25;
    pub const FOLLOW_STUN_FRAMES: u8 = 40;
    /// ★ 2026-10-16 (user): 30 s after the blast (before 60 s).
    pub const FOLLOW_COOLDOWN: u32 = 600;
    /// The beast's buff sound (`bn.f()V`'s cases play `br.a(B)V(2)`).
    pub const BUFF_SOUND: u8 = 2;
    /// ★★ 2026-10-16 (user: *"돌진(암흑폭발) 지속시간을 15초 쿨타임을 20초로 변경해줘"*): 5 s → **15 s** (300 ticks), and the slot's
    /// counter ← **CHARGE_COOLDOWN** (20 s) right after the beast's own (`ag.a(IZ)V`) — the SLOTCD piece, the way FEND sets 추종's.
    pub const CHARGE_FRAMES: u32 = 300;
    pub const CHARGE_COOLDOWN: u32 = 400;
    /// ★ 2026-10-15: ±2 (the charge's front flames reach two tiles ahead; before ±1).
    pub const CHARGE_HALF: u8 = 2;
    pub const CHARGE_REHIT: u32 = 10;
    pub const CHARGE_SLOTS: u32 = 8;
    pub const CHARGE_ENTRY: u32 = 8;
    /// The HUD cells: 추종 with its seconds on the beast strip's 피의계약 cell, 돌진 on the 암흑폭발 cell.
    pub const FOLLOW_BUFF_ID: u8 = 204;
    pub const CHARGE_BUFF_ID: u8 = 205;
    /// The blast's record: at most BLAST_MAX monsters' px (the picture's small bursts).
    pub const BLAST_MAX: u8 = 16;
    /// A push along the facing: the tile delta by facing − 1.
    pub const PUSH_DX: [i8; 4] = [0, 0, -1, 1];
    pub const PUSH_DY: [i8; 4] = [-1, 1, 0, 0];
    /// ★ D4 words (0x1600.. — within an `ldr rt,[rn,#imm]` of a register holding `block + D4`): 추종's switch · its end tick
    /// · hits · gauge · slot · the swing counter; the blasts and the last blast's damage (the probe); 돌진's switch · end tick
    /// · hits · damage · slot; the HP leeched · the extra steps · the extra swings (the probe); the blast picture's cue ·
    /// the monsters recorded · the demon's px · the painter's age and last cue; the casts; the step parity; the pushes.
    pub const D4: u32 = 0x1600;
    pub const FOLLOW: u32 = 0x1600;
    pub const FOLLOW_UNTIL: u32 = 0x1604;
    pub const FOLLOW_HITS: u32 = 0x1608;
    pub const FOLLOW_GAUGE: u32 = 0x160c;
    pub const FOLLOW_SLOT: u32 = 0x1610;
    pub const FOLLOW_TICK: u32 = 0x1614;
    pub const BLASTS: u32 = 0x1618;
    pub const BLAST_DMG: u32 = 0x161c;
    pub const CHARGE: u32 = 0x1620;
    pub const CHARGE_UNTIL: u32 = 0x1624;
    pub const CHARGE_HITS: u32 = 0x1628;
    pub const CHARGE_DMG: u32 = 0x162c;
    pub const CHARGE_SLOT: u32 = 0x1630;
    pub const LEECHED: u32 = 0x1634;
    pub const STEPS: u32 = 0x1638;
    pub const SWINGS: u32 = 0x163c;
    pub const BLAST_SEQ: u32 = 0x1640;
    pub const BLAST_N: u32 = 0x1644;
    pub const BLAST_PX: u32 = 0x1648;
    pub const BLAST_PY: u32 = 0x164c;
    pub const BLAST_AGE: u32 = 0x1650;
    pub const BLAST_LAST: u32 = 0x1654;
    pub const FOLLOWS: u32 = 0x1658;
    pub const CHARGES: u32 = 0x165c;
    pub const STEP_TICK: u32 = 0x1660;
    pub const PUSHES: u32 = 0x1664;
    /// ★ 2026-10-16: the claw's mark for the MOBHP piece (TICKS + 1 while a claw kind's hit is on its way from the damage
    /// function to `bd.a(IB)V`, 0 once consumed) · the hits taken and the other damage dealt that fed the gauge (the probe).
    pub const CLAW_MARK: u32 = 0x1668;
    pub const FOLLOW_TAKEN: u32 = 0x166c;
    pub const FOLLOW_DEALT: u32 = 0x1670;
    /// The charge table (CHARGE_SLOTS × CHARGE_ENTRY) and the blast record (BLAST_MAX `(x, y)` pairs).
    pub const CHARGE_TAB: u32 = 0x1680;
    pub const BLAST_XY: u32 = 0x16c0;
    /// ★ D4 pieces: FOLLOW (a press of kind 2 while 악의 runs) · FEND (추종's end — the blast) · CHARGE (a press of kind 12) ·
    /// DTICK (the demon's tick: 추종's minute · 돌진's hits and pushes · the extra step) · CLAWX (the drives' claw entry — the
    /// extra swing) · DMG ×3 (the damage functions' wrappers: the gauge, the leech) and the body they share · ★ 2026-10-14 FXG
    /// (the ground draw's effects — the room of D4's AURA and BLASTD pieces, which the effect library replaced).
    pub const FOLLOW_AT: u32 = 0x1740;
    pub const FEND_AT: u32 = 0x1880;
    pub const CHARGE_AT: u32 = 0x1a80;
    pub const DTICK_AT: u32 = 0x1b80;
    /// ★★ 2026-10-17 0x1ec0 → 0x1e00 (격분's speed-up grew it past 160 B; DTICK keeps 640 B for its 528).
    pub const CLAWX_AT: u32 = 0x1e00;
    pub const DMG_AT: [u32; 3] = [0x1f60, 0x1f80, 0x1fa0];
    pub const DMGC_AT: u32 = 0x1fc0;
    pub const FXG_AT: u32 = 0x2080;
    /// ★ 2026-10-16: the MOBHP piece in FXG's spare room (FXG's room now ends here) — the wrapper on `bd.a(IB)V`'s definition
    /// word (the gauge from every damage a monster takes: CLAW_MARK → FOLLOW_GAIN_CLAW, else FOLLOW_GAIN_DEALT) and, at
    /// [`Self::FTAKEN_AT`] inside the same piece, the body the TAKEN piece calls for a hit taken (FOLLOW_GAIN_TAKEN, then
    /// DGEAR's OP_TAKEN as before — TAKEN's room of 64 B had no space for the gauge).
    pub const MOBHP_AT: u32 = 0x2400;
    pub const FTAKEN_AT: u32 = Self::MOBHP_AT + 0x100;
    /// ★★ D5 (2026-10-13): 용암불 (지옥불, kind [`Self::LAVA_KIND`]) and 멸화 (학살화염, kind [`Self::FIRE_KIND`]) while 악의 runs
    /// (user ⑫) — the beast's own skills outside it. 용암불: the beast's 지옥불 runs as shipped (the CAST piece chains kind 8 —
    /// the beast steps out and casts its ring), and this block's LAVA piece, the outermost wrapper on the ring spawner
    /// `bn.a(BIBI)V`, records every tile the ring's tier-[`Self::LAVA_TIER`] `ce` lands on (the spawner's own arithmetic: the
    /// beast's tile + `co.a/co.b[dir]`, inside the map) as LAVA for [`Self::LAVA_FRAMES`] ticks with the ring's damage `d` — the
    /// same tile again replaces the entry (user ⑩ 「같은 칸 최신 것만」; a full table records nothing). Every [`Self::LAVA_EVERY`]
    /// ticks each live monster standing on a lava tile takes `d × LAVA_PCT / 100` (never below 1) through `bd.a(IB)V` (no
    /// defence; the 지옥불 `ce`'s own hits are untouched — 「지옥불 피해 그대로 · 중첩」). 멸화: a press is the beast's checks ·
    /// cost · counter (the 학살화염 numbers), then [`Self::FIRE_WINDUP`] ticks later every live monster within FIRE_WINDOW
    /// (★ 2026-10-15 an ellipse 6 wide · 4 tall in tiles — the blast's picture ±88 px + 1; before a ±6 square) of the demon
    /// takes the heavy attack × [`Self::FIRE_MUL`] (user ⑪, `bd.a(IB)V`); a press while one is pending
    /// does nothing. Both end with 악의 (STATE ≠ 2 or another owner: the D5TICK piece clears the lava and a pending 멸화).
    pub const LAVA_KIND: u8 = 8;
    pub const FIRE_KIND: u8 = 11;
    /// ★★ 2026-10-16 (user: *"악의로 스킬이 바뀌면 야수 스킬창에서도 이름·아이콘 변경 (피의계약 · 지옥불 · 심판번개 → 추종 ·
    /// 용암불 · 어둠심판 …) 스킬설명도 간단히"*): THE SKIN — the six beast kinds 악의 changes, in the order the asset appends
    /// their 악의 versions (`tools_re/demon_skin.py` `SKIN`, the contract — `test_demon_look` reads this line): `t/bsk.tdf`
    /// rows [`Self::SKIN_ROW_FIRST`] + 2j (name) · + 2j + 1 (description), `img/bico` frames [`Self::SKIN_ICON_FIRST`] + j.
    /// While the block is in STATE 2 the host exchanges the shipped row / icon slot of kind `SKIN_KINDS[j]` with the
    /// appended one in the game's own table and icon array ([`JavaWrite::TableRowRef`] · [`JavaWrite::IconArrayRef`],
    /// `hero2java::h2_demon_skin_*`), and back when 악의 ends — and the host HUD's beast icons follow
    /// ([`Self::skin_icon`]). Nothing of the block, a record or a save.
    pub const SKIN_KINDS: [u8; 6] = [Self::FOLLOW_KIND, Self::CHARGE_KIND, Self::BLADE_KIND, Self::JUDGE_KIND, Self::LAVA_KIND, Self::FIRE_KIND];
    pub const SKIN_ROW_FIRST: u32 = 26;
    pub const SKIN_ICON_FIRST: u8 = 13;
    /// The 악의 version's icon (the appended `img/bico` frame — the atlas's beast id too) of beast skill `kind`, `None` for a
    /// kind 악의 leaves alone.
    pub const fn skin_icon(kind: u8) -> Option<u8> {
        let mut j = 0;
        while j < Self::SKIN_KINDS.len() {
            if Self::SKIN_KINDS[j] == kind {
                return Some(Self::SKIN_ICON_FIRST + j as u8);
            }
            j += 1;
        }
        None
    }
    /// The ring tier `bn.d(B)V(kind − 7)` passes for 지옥불 (빛의파동 0 · 지옥불 1 · 대지분노 2).
    pub const LAVA_TIER: u8 = 1;
    pub const LAVA_FRAMES: u32 = 100;
    pub const LAVA_EVERY: u32 = 20;
    pub const LAVA_PCT: u8 = 50;
    /// The lava table: LAVA_SLOTS entries of `(x, y, until, d, next tick, due this tick)`.
    pub const LAVA_SLOTS: u32 = 32;
    pub const LAVA_ENTRY: u32 = 24;
    /// ★★ 2026-10-15: the hellfire ring's offsets from the beast's tile in piece order (the probe's LAVA_TAB order = the spawner's
    /// directions 1..12: the eight neighbours, then the four tips) — `tools_re/lava_fx.py` `RING` (the test reads both).
    pub const LAVA_RING: [(i8, i8); 12] = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1), (0, -2), (0, 2), (-2, 0), (2, 0)];
    /// The pieces the effects hold: the ring's twelve + four spares (`12 + (x & 1) + 2·(y & 1)`).
    pub const LAVA_PIECES: u8 = 16;
    pub const LAVA_PIECE_SPARE: u8 = 12;
    /// ★★ 2026-10-18 (user: *"야수목걸이로 범위가 늘어난 스킬들이 실제 영향은 범위가 늘어난대로 되고있는데 이펙트는 안늘어나고 그대로야.
    /// 이거 이펙트도 늘어나게"*) — 야수조련 목걸이 (the accessory block's ACTIVE = [`AccessoryPatch::BEAST`], read through
    /// [`Self::ACC_PTR`]) doubles the ring's radius: its BEAST_RANGE wrapper spawns a `ce` on each of the direction's
    /// [`AccessoryPatch::RING_EXTRA`] tiles straight through `ce.<init>`, past this block's LAVA piece, so under 악의 the
    /// 지옥불 hit 32 tiles while the lava stayed on the ring's 12. Now the LAVA piece records those tiles too (the same level
    /// rule: the four plain directions' at level ≥ 3 are directions 9..12's own), and the pictures follow: a ring tile's
    /// piece is `LAVA_WIDE + k` — the same picture as the ring piece, drawn on that tile ([`Self::FX_LAVA_WIDE`] · its
    /// top). A monster on the tile or the next one over burns, because the art spills past the 16px tile. An added
    /// tile's piece is [`Self::LAVA_PIECE_NONE`] (no picture of its own; it still burns, and the neighbouring ring
    /// piece's spill covers it).
    pub const LAVA_WIDE: u8 = 16;
    pub const LAVA_PIECE_NONE: u8 = 0xff;
    pub const FIRE_WINDUP: u32 = 10;
    pub const FIRE_WINDOW: u32 = Self::window(6, 4, true);
    pub const FIRE_MUL: u8 = 2;
    /// The `ce` hit sound (`br.a(B)V(3)`) at the detonation; the cast is the beast's buff sound.
    pub const FIRE_SOUND: u8 = 3;
    /// The detonation's record (the monsters struck, for the picture's small bursts).
    pub const FIRE_MAX: u8 = 16;
    /// ★ D5 words (0x2640.. — within an `ldr rt,[rn,#imm]` of a register holding `block + D5`): the tiles recorded · the lava
    /// hits · the last lava tick's damage · whether the table may hold entries · 멸화 pending · the tick it detonates at · its
    /// slot · hits · damage · casts · the explosion's cue · the monsters recorded · the demon's px · the painter's age and last
    /// cue · the tick it detonated at.
    pub const D5: u32 = 0x2640;
    pub const LAVAS: u32 = 0x2640;
    pub const LAVA_HITS: u32 = 0x2644;
    pub const LAVA_DMG: u32 = 0x2648;
    pub const LAVA_LIVE: u32 = 0x264c;
    pub const FIRE: u32 = 0x2650;
    pub const FIRE_DUE: u32 = 0x2654;
    pub const FIRE_SLOT: u32 = 0x2658;
    pub const FIRE_HITS: u32 = 0x265c;
    pub const FIRE_DMG: u32 = 0x2660;
    pub const FIRES: u32 = 0x2664;
    pub const FIRE_SEQ: u32 = 0x2668;
    pub const FIRE_N: u32 = 0x266c;
    pub const FIRE_PX: u32 = 0x2670;
    pub const FIRE_PY: u32 = 0x2674;
    pub const FIRE_AGE: u32 = 0x2678;
    pub const FIRE_LAST: u32 = 0x267c;
    pub const FIRE_TICK: u32 = 0x2680;
    /// ★★ 2026-10-15 (user: *"용암불이 타일에 불이 남는것처럼 나와서 사각형으로 이펙트가 남아서 어색한데, 이거 초안처럼 자연스럽게 불 위치에
    /// 불 남게 해줘"* · *"악의 실행하고 다음맵으로 넘어가면 악의가 풀리고있어"*): LAVA_MAP = the map the table's tiles belong to (map
    /// number + 1, written with every record — [`Self::stage_map_off`]); the D5TICK piece tears the lava down and FXG / FXT do not
    /// draw it on another map (악의 itself goes on through an exit — `h2_demon_in_world`). LAVA_PIECE = a byte per LAVA_TAB entry:
    /// the tile's place in the ring ([`Self::LAVA_RING`] → 0..11, any other offset 12 + (x & 1) + 2·(y & 1)) — the effect it draws
    /// is `FX_LAVA + piece` (`tools_re/lava_fx.py`: the draft 11's lava field cut into the ring's twelve pieces, so the twelve tiles
    /// of one ring draw that field whole).
    pub const LAVA_MAP: u32 = 0x2684;
    /// ★★ 2026-10-18: the accessory block (written at this block's install; 0 = none — the lava as the ring alone): its
    /// ACTIVE word says whether 야수조련 목걸이 doubles the ring ([`Self::LAVA_WIDE`]).
    pub const ACC_PTR: u32 = 0x2688;
    pub const LAVA_PIECE: u32 = 0x2690;
    /// The detonation record (FIRE_MAX `(x, y)` pairs) and the lava table (LAVA_SLOTS × LAVA_ENTRY).
    pub const FIRE_XY: u32 = 0x26c0;
    pub const LAVA_TAB: u32 = 0x2740;
    /// ★ D5 pieces: LAVA (the ring spawner's wrapper) · FIRE (a press of kind 11 while 악의 runs) · D5TICK (the demon's tick: the
    /// lava ticks · the detonation · the teardown) · ★ 2026-10-14 FXT (the HUD painter's effects — the room of D5's LAVAD and
    /// FIRED pieces). ★★ 2026-10-18 LAVA 0x2a40 → 0x1340, in the HUD piece's spare room up to the D4 words (the necklace's
    /// tiles outgrew its 448 B — the HOLD piece took that room).
    pub const LAVA_AT: u32 = 0x1340;
    pub const FIRE_AT: u32 = 0x2c00;
    pub const D5TICK_AT: u32 = 0x2d00;
    /// ★★ 2026-10-24 its room ends at SND_AT (the SND and TOGGLE pieces took its spare room's tail).
    pub const FXT_AT: u32 = 0x30c0;

    /// ★★ 2026-10-14 (5) — THE DEMON'S POSES (user: *"악의의 모든 외형과 이펙트를 최대한 높은 퀄리티로 … 연결동작"*). Until now the
    /// demon stood or walked through every one of its skills — only the claw had in-between poses. The asset now carries,
    /// after each class's transform sequence, [`Self::POSE_SEQS`] POSE sequences (`tools_re/demon_look.py` `CAST_SEQS` —
    /// frame f = the pose at tick f of the skill): 0 지옥칼날 · 1 어둠심판 · 2 멸화 · 3 용암불 · 4 추종 · 5 its end blast (each
    /// [`Self::POSE_LENS`] ticks) · 6 the 돌진 run (a loop of [`Self::POSE_RUN_FRAMES`]) · 7 the standing breath (a loop of
    /// [`Self::POSE_IDLE_FRAMES`] frames of `1 << IDLE_SHIFT` ticks). The DRAW piece picks one in place of a stand or walk
    /// sequence (0..3) while STATE 2 with the LOOK: it watches the skills' own words — the toggles BLADE · JUDGE turning on,
    /// the counters FIRES · LAVAS · FOLLOWS · BLASTS moving (each against a shadow here) — and a change starts that pose at
    /// TICKS (the same pose already running is not restarted — the lava ring records its tiles over several ticks); a timed
    /// pose plays its ticks, then 돌진 (CHARGE on) runs, else the stand breathes. A new 악의 (UNTIL changed) re-reads every
    /// shadow without a pose. Nothing of the game's state is written; the pose is a drawing — the claw (sequences 4..6) and
    /// every other action draw as before.
    pub const D6: u32 = 0x3740;
    pub const SH_UNTIL: u32 = 0x3740;
    pub const SH_BLADE: u32 = 0x3744;
    pub const SH_JUDGE: u32 = 0x3748;
    pub const SH_FIRES: u32 = 0x374c;
    pub const SH_LAVAS: u32 = 0x3750;
    pub const SH_FOLLOWS: u32 = 0x3754;
    pub const SH_BLASTS: u32 = 0x3758;
    /// The pose running (`POSE_NONE` = none) · the tick it started · the poses started (the probe).
    pub const POSE_KIND: u32 = 0x375c;
    pub const POSE_START: u32 = 0x3760;
    pub const POSES_STARTED: u32 = 0x3764;
    /// ★★ 2026-10-15 HD (0x3768, the install writes it): 1 = the host composites the effect library's HD twins at the window's
    /// resolution — the DRAW piece then emits the demon's BODY as an effect draw ([`Self::FX_BODY`], frame = the body sheet
    /// frame − BODY_BASE, at the drawer's origin + the animation piece's (dx, dy)) BEFORE the drawer runs, and the drawer
    /// runs over the BLANK body sheet [`Self::COSTUME_HD`]; 0 = the sheet-300 body at the guest's resolution as before.
    pub const HD: u32 = 0x3768;
    /// ★★ 2026-10-17 (user: 격분 100 % 의 기본공격 속도도 할퀴기에 — *"3도 적용되어야해"*): the berserker block (`BerserkerPatch`,
    /// written at this block's install; 0 = none) — the CLAWX piece reads its FURY_STATE · FURY_GAUGE · FURY_OWNER — and the
    /// claw's own parity under 격분 at 100 % (every odd claw frame one more, as the berserker's drive does for its stages).
    pub const FURY_PTR: u32 = 0x376c;
    pub const FURY_CLAW_TICK: u32 = 0x3770;
    /// ★★ 2026-10-17 (user: *"보스레이드도 보스를 잡은 뒤에는 멈추게 … 다시 진행하면 그때 다시 흐르는거고"*): the host's pause
    /// (0x3774, the install writes 0): ≠ 0 = the demon's clock stands — the raid between a stage's kill and the next stage's
    /// map (`H2BossRaid::paused`). The TICK piece also stands it while the state word is [`Self::SCRIPT_STATE`]. Either way
    /// nothing that counts TICKS moves — 악의's minute · the toggles' SP and hits · 추종 · 돌진 · the lava · 멸화 — and it all
    /// goes on from where it stood. Not in STATE 1 (the transform finishes: its action is the game's and ends on its own).
    pub const PAUSE: u32 = 0x3774;
    /// The state word's value while a script (a dialog · a cutscene) is up.
    pub const SCRIPT_STATE: u8 = 4;
    pub const POSE_NONE: u32 = 0xff;
    pub const POSE_BLADE: u8 = 0;
    pub const POSE_JUDGE: u8 = 1;
    pub const POSE_FIRE: u8 = 2;
    pub const POSE_LAVA: u8 = 3;
    pub const POSE_FOLLOW: u8 = 4;
    pub const POSE_BURST: u8 = 5;
    pub const POSE_RUN: u8 = 6;
    pub const POSE_IDLE: u8 = 7;
    pub const POSE_SEQS: usize = 8;
    /// The timed poses' lengths in ticks (= their sequences' frame counts — `test_demon_look` reads this line): 지옥칼날
    /// 0.8 s · 어둠심판 1.2 s (its wind-up is [`Self::JUDGE_WINDUP`] — the strike frames at 20) · 멸화 1.2 s (the gather
    /// is [`Self::FIRE_WINDUP`], the roar from the detonation) · 용암불 0.8 s · 추종 0.9 s · the blast 0.8 s.
    pub const POSE_LENS: [u8; 6] = [16, 24, 24, 16, 18, 16];
    pub const POSE_RUN_FRAMES: u8 = 8;
    pub const POSE_IDLE_FRAMES: u8 = 4;
    pub const IDLE_SHIFT: u16 = 3;

    /// ★ (5): the pose sequence `kind` of `class` (right after its transform sequence).
    pub const fn pose_seq(class: u8, kind: u8) -> Option<u8> {
        match Self::transform_seq(class) {
            Some(t) if (kind as usize) < Self::POSE_SEQS => Some(t + 1 + kind),
            _ => None,
        }
    }

    /// ★★ 2026-10-14 「악마화」 이펙트 리뉴얼 (user: *"docs\skill\astra_demon 안에 이펙트들을 모아놨어 … 악의 관련 이펙트 전수
    /// 변경해줘"*): every picture of 악의 is now an effect of the host's EFFECT LIBRARY (`wie_android/src/demon_fx.bin`, drawn by
    /// `tools_re/demon_fx.py` from the twelve in-game mockups of `docs/skill/astra_demon/ingame`; registered with the canvas at
    /// boot — `wie_backend::canvas::set_fx_library`). A drawing piece asks for one with `setAlpha(FX_TAG | 1)` · `drawLine(x, y,
    /// x + effect, y + age)` · `setAlpha(FX_TAG | 0)`: frame `age` of `effect` anchored at `(x, y)` — the anchor an entity's feet
    /// (world px + FEET + the camera), a lava tile's top-left corner. ★★ 2026-10-18 야수조련 목걸이: the range skills arm
    /// `FX_TAG | 2` instead (`set_fx_scale(2)` — the same picture at twice the size about the anchor), matching the doubled
    /// windows; 용암불 keeps `| 1` and draws [`Self::FX_LAVA_WIDE`] (its own 2× pieces about the ring's centre). An age or
    /// effect the library lacks draws nothing, so the pieces never bound an age by the library. The ids below are
    /// `tools_re/demon_fx.py`'s `EFFECTS` order (its `check` and the effect test read the library's names at them). Layers:
    /// the FXG piece (the ground draw — under every entity) and the FXT piece (the HUD painter's entry — over every entity).
    /// Nothing of the game's state is written but the two draw-aged counters (BLAST_AGE · FIRE_AGE), as the D4 · D5 pictures did.
    pub const FX_TAG: u32 = 0x4835_0000;
    pub const FX_TRANSFORM: u8 = 0;
    pub const FX_TRANSFORM_GROUND: u8 = 1;
    /// `FX_CLAW + cut × 4 + (facing − 1)` (facing 1 up · 2 down · 3 left · 4 right) — the cut: 0 stage 0's first claw · 1 its
    /// second (the X) · 2 stage 1's sweep · 3 stage 2's slam (the CLAW piece's cue).
    pub const FX_CLAW: u8 = 2;
    pub const FX_CHARGE_GROUND: u8 = 18;
    pub const FX_CHARGE_TOP: u8 = 22;
    /// `+ min(FOLLOW_HITS >> 3, 3)`.
    pub const FX_FOLLOW_BUILD: u8 = 26;
    pub const FX_FOLLOW_DOME: u8 = 30;
    pub const FX_BURST: u8 = 31;
    pub const FX_BURST_MOB: u8 = 32;
    pub const FX_SEAL: u8 = 33;
    /// `+ (i & 1)` — the i-th monster's blade, the two curves alternating.
    pub const FX_BLADE_RISE: u8 = 34;
    pub const FX_JUDGE_ORB: u8 = 36;
    /// `+ ((the strike's tick >> 2) & 3)` — a fresh bolt every strike.
    pub const FX_BOLT: u8 = 37;
    pub const FX_FIRE_CORE: u8 = 41;
    pub const FX_FIRE_GROUND: u8 = 42;
    pub const FX_FIRE_BLAST: u8 = 43;
    pub const FX_FIRE_MOB: u8 = 44;
    pub const FX_FIRE_SCORCH: u8 = 45;
    /// `+ LAVA_PIECE[entry]` — ★★ 2026-10-15 the tile's piece of the ring's field (before: `(x & 3) + 4 × (y & 3)` of one
    /// world-aligned crack texture, which drew the ring as twelve squares).
    pub const FX_LAVA: u8 = 46;
    pub const FX_MARKER: u8 = 62;
    pub const FX_LAVA_TOP: u8 = 63;
    /// ★★ 2026-10-14 (밤) 79 → 146: 일섬 「번개의 호흡」's yellow lightning after the 79 (`AccessoryPatch::FX_IL*`).
    /// ★★ 2026-10-22 (user: *"악마로 변신한 외형 모습은 완전히 버리고 … 클레르의 외형에 검은색 날개"*): the WINGS — one effect of
    /// FX_BODY_LEN frames = facing (g.u − 1: back · front · left · right) × [`Self::WING_FLAPS`] flap phases, a frame one sprite
    /// anchored at the wing root with the piece's (dx, dy) = the character's origin → her shoulders (`tools_re/malice_wings.py`).
    /// The DRAW piece emits age `(facing − 1) × WING_FLAPS + ((TICKS >> WING_FLAP_SHIFT) & (WING_FLAPS − 1))` at the drawer's
    /// x · y — before the drawer (the body covers the root) except for the back view, where it runs after it (the wings sit on
    /// her back). Before 2026-10-22 this effect was the demon's body (115 frames) drawn over a blank body sheet; the game now
    /// draws her own parts (the sequence copies keep head · body · arms · legs and drop the weapon · offhand).
    pub const FX_BODY: u8 = 146;
    pub const FX_BODY_LEN: u32 = 16;
    pub const WING_FLAPS: u8 = 4;
    pub const WING_FLAP_SHIFT: u16 = 3;
    /// g.u of the back view (facing up) — the one facing whose wings draw over the body.
    pub const BACK_FACING: u8 = 1;
    /// ★★ 2026-10-15 (5): 멸화's explosion is 3D particle fire (`tools_re/fire_fx.py`) split by depth — the half in front of the
    /// demon is FX_FIRE_BLAST (FXT, over every entity), the half behind it this one (FXG, under every entity). Both take the
    /// same draw age: FXG draws before FXT in a paint, so it predicts the age FXT is about to give FIRE_AGE (a new FIRE_SEQ = 0,
    /// else FIRE_AGE + 1; FX_FIRE_BLAST_LEN or more = not drawn) and writes nothing.
    pub const FX_FIRE_BACK: u8 = 147;
    /// ★★ 2026-10-16 악의 무기 (`tools_re/demon_weapon_fx.py`, appended — the effects before them byte for byte): M2's four-sided
    /// cut `claw{c}_spin` (the four facings' parts of `claw{c}_{d}` together — no new sprite) at FX_CLAW_SPIN + cut, and M3's
    /// bigger cut `claw{c}_{d}_big` (the sprites and their HD twins at [`Self::CLAW_BIG_SCALE`] / 100 about their anchors) at
    /// FX_CLAW_BIG + cut·4 + facing − 1.
    pub const FX_CLAW_SPIN: u8 = 148;
    pub const FX_CLAW_BIG: u8 = 152;
    /// ★★ 2026-10-18 야수조련 목걸이's lava (`tools_re/lava_wide_fx.py`, appended): `lava{k}` · `lava_top{k}` of the ring's twelve
    /// at 2× about the centre tile's centre — the same sprites, the parts' offsets doubled and flagged twice the size
    /// (`wie_backend::canvas::FX_X2`), anchored on the same tile ([`Self::LAVA_WIDE`]).
    pub const FX_LAVA_WIDE: u8 = 168;
    pub const FX_LAVA_WIDE_TOP: u8 = 180;
    /// ★★ 2026-10-22 (4) (user: *"캐릭터 발밑에 검은색,회색 섞은 계열의 오오라"*): the AURA under her feet while 악의 runs — the
    /// FXG piece draws it first of the player's ground effects (STATE 2), frame `(TICKS >> FX_AURA_SHIFT) & FX_AURA_MASK`
    /// (`tools_re/malice_wings.py aura_frames`, appended after the wide lava).
    pub const FX_AURA: u8 = 192;
    pub const FX_AURA_LEN: u32 = 16;
    pub const FX_AURA_MASK: u8 = 15;
    pub const FX_AURA_SHIFT: u16 = 1;
    pub const FX_COUNT: u8 = 193;
    pub const CLAW_BIG_SCALE: u32 = 135;
    /// ★★ 2026-10-16 M3's windows (`GearPatch::FLAG_WIDE`): every stage a tile deeper and a tile wider each side —
    /// [`Self::CLAW_WINDOWS`] + (1, 1), the picture [`Self::CLAW_BIG_SCALE`] % (the cuts' 3 · 3 · 4 tiles × 1.35 ≈ 4 · 4 · 5).
    pub const CLAW_WINDOWS_WIDE: [(u8, u8); 3] = [(4, 3), (4, 3), (5, 3)];
    /// ★★ 2026-10-16 M2: the stages that rake all four sides (`GearPatch::FLAG_SPIN`) — the first two; the third (the rend) stays
    /// ahead.
    pub const SPIN_STAGES: u8 = 2;
    /// The transform's effect runs from the cast (`T1_UNTIL − TRANSFORM_FRAMES − T1_GRACE`) for FX_TRANSFORM_LEN ticks — STATE 1
    /// and the demon's first second. A claw cut plays FX_CLAW_LEN draws from its cue.
    pub const FX_TRANSFORM_LEN: u32 = 44;
    pub const FX_CLAW_LEN: u32 = 6;
    /// The looping effects take `TICKS & mask`.
    pub const FX_CHARGE_MASK: u8 = 7;
    pub const FX_FOLLOW_MASK: u8 = 15;
    pub const FX_SEAL_MASK: u8 = 31;
    pub const FX_BLADE_MASK: u8 = 15;
    /// The i-th blade's phase: `(TICKS + i × FX_BLADE_STAGGER) & FX_BLADE_MASK`.
    pub const FX_BLADE_STAGGER: u8 = 5;
    /// The draw-aged ones (a new BLAST_SEQ / FIRE_SEQ = age 0, then + 1 a draw) and the tick-aged ones.
    pub const FX_BURST_LEN: u32 = 20;
    pub const FX_BURST_MOB_LEN: u32 = 12;
    pub const FX_FIRE_BLAST_LEN: u32 = 24;
    pub const FX_FIRE_MOB_LEN: u32 = 12;
    pub const FX_BOLT_LEN: u32 = 4;
    pub const FX_SCORCH_LEN: u32 = 64;

    /// ★ D2: the demon sequence of human sequence `seq` for `class` (the asset's `demon_look.demon_seq`), `None` when the
    /// sequence has no demon copy (an effect-only sequence — drawn as it came).
    pub const fn demon_seq(class: u8, seq: u8) -> Option<u8> {
        if class as usize >= Self::MAPPED.len() {
            return None;
        }
        let row = Self::MAPPED[class as usize];
        let mut i = 0;
        while i < row.len() {
            if row[i] == seq {
                return Some(Self::SHIPPED_SEQS[class as usize] + i as u8);
            }
            i += 1;
        }
        None
    }
    /// ★ D2: the transform sequence of `class` (after the demon copies).
    pub const fn transform_seq(class: u8) -> Option<u8> {
        if class as usize >= Self::SHIPPED_SEQS.len() {
            return None;
        }
        Some(Self::SHIPPED_SEQS[class as usize] + Self::MAPPED_COUNT as u8)
    }
    /// ★ D2: the SEQ_TABLE for `class` — what the host writes into the block before LOOK 1.
    pub const fn seq_table(class: u8) -> [u8; Self::SEQ_TABLE_LEN] {
        let mut t = [Self::SEQ_NONE; Self::SEQ_TABLE_LEN];
        let mut s = 0;
        while s < Self::SEQ_TABLE_LEN {
            if let Some(d) = Self::demon_seq(class, s as u8) {
                t[s] = d;
            }
            s += 1;
        }
        t
    }
    /// The claw's attack from the game's attack roll and 지력'.
    pub const fn claw_attack(atk: i32, int: i32) -> i32 {
        atk + int * Self::CLAW_INT_NUM as i32 / Self::CLAW_INT_DEN as i32
    }
    /// Received damage under 악의 (≤ 0 untouched, as the block).
    pub const fn taken(d: i32) -> i32 {
        if d <= 0 {
            d
        } else {
            d * Self::TAKEN_NUM as i32 / Self::TAKEN_DEN as i32
        }
    }
    /// May `class` arm quickbar `value` while 악의 runs (0 = the claw combo, always).
    pub const fn value_allowed(class: u8, value: u8) -> bool {
        if value == 0 {
            return true;
        }
        if class as usize >= Self::BUFF_VALUES.len() {
            return false;
        }
        let row = Self::BUFF_VALUES[class as usize];
        (row[0] != 0 && row[0] == value) || (row[1] != 0 && row[1] == value) || (row[2] != 0 && row[2] == value)
    }
    /// Is `(dx, dy)` — a monster's tile minus the player's — in the window of `stage` when facing `facing`
    /// (1 up · 2 down · 3 left · 4 right)? The block's own arithmetic (★ 2026-10-15: `CLAW_WINDOWS[stage]` for every stage).
    pub const fn in_window(stage: u8, facing: u8, dx: i32, dy: i32) -> bool {
        if stage as usize >= Self::CLAW_WINDOWS.len() {
            return false;
        }
        let (depth, half) = Self::CLAW_WINDOWS[stage as usize];
        let (f, l) = match facing {
            1 => (-dy, dx),
            2 => (dy, dx),
            3 => (-dx, dy),
            4 => (dx, dy),
            _ => return false,
        };
        f >= 1 && f <= depth as i32 && l.abs() <= half as i32
    }
    /// ★ D3: is `(dx, dy)` within ±`half` tiles (a square around the demon)?
    pub const fn in_square(half: u8, dx: i32, dy: i32) -> bool {
        dx.abs() <= half as i32 && dy.abs() <= half as i32
    }
    /// ★★ 2026-10-15: a COLLECT window word — `hx | hy << 8 | ellipse << 16`: a ±hx × ±hy rectangle (hy 0 = hx — the old
    /// squares, `movs` reachable), or the ellipse `dx²·hy² + dy²·hx² ≤ hx²·hy²` (the seal · the fire · the blast are drawn as
    /// ellipses on the ground). The COLLECT piece and the FEND piece decode it the same way.
    pub const fn window(hx: u8, hy: u8, ellipse: bool) -> u32 {
        hx as u32 | (hy as u32) << 8 | (ellipse as u32) << 16
    }
    pub const fn in_collect_window(word: u32, dx: i32, dy: i32) -> bool {
        let hx = (word & 0xff) as i32;
        let mut hy = ((word >> 8) & 0xff) as i32;
        if hy == 0 {
            hy = hx;
        }
        if word >> 16 == 0 {
            dx.abs() <= hx && dy.abs() <= hy
        } else {
            dx * dx * hy * hy + dy * dy * hx * hx <= hx * hx * hy * hy
        }
    }
    pub const fn skill_int_power(int: i32) -> i32 {
        let i = if int < 0 { 0 } else { int };
        if i > 200 { return 1000 + (i - 200) * 5; }
        let curved = i + i * i / 50;
        let original = 20 + 2 * i;
        if curved > original { curved } else { original }
    }
    pub const fn skill_boost(int: i32) -> i32 {
        600 + if int < 0 { 0 } else if int > 200 { 200 } else { int }
    }
    pub const fn skill_power(power: i32, int: i32) -> i32 {
        Self::skill_power_with_attack(power, int, 0)
    }
    pub const fn skill_power_with_attack(power: i32, int: i32, attack: i32) -> i32 {
        (power + Self::skill_int_power(int) * 85 / 100 + attack * 30 / 100) * Self::skill_boost(int) / 500
    }
    pub const fn lava_power(damage: i32, int: i32) -> i32 {
        let old = 50 + int / 2;
        let factor = if old < 100 { 100 } else { old };
        damage * factor / 100 * Self::skill_boost(int) / 500
    }
    pub const fn assassin_tick(power: i32, int: i32, lv: i32) -> i32 {
        Self::skill_power(power * 27 / 10, int) * lv
    }
    pub const fn heavy_hit(power: i32, int: i32, lv: i32) -> i32 {
        Self::skill_power(power * 5 / 2, int) * (lv + 2) / 2
    }
    /// ★ D3: one 지옥칼날 tick (never below 1).
    pub const fn blade_tick(power: i32, int: i32, lv: i32) -> i32 {
        let d = Self::assassin_tick(power, int, lv) * Self::BLADE_PCT as i32 / 100;
        if d < 1 { 1 } else { d }
    }
    /// ★ D3: one 어둠심판 strike (never below 1).
    pub const fn judge_tick(power: i32, int: i32, lv: i32) -> i32 {
        let d = Self::heavy_hit(power, int, lv) * Self::JUDGE_PCT as i32 / 100;
        if d < 1 { 1 } else { d }
    }
    /// ★ D3: a toggle's SP a second — `max SP × pct / 100`.
    pub const fn toggle_cost(max_sp: i32, pct: u8) -> i32 {
        max_sp * pct as i32 / 100
    }
    /// ★★ 2026-10-16: a toggle's SP a second with a floor — `max(max SP × pct / 100, min)` (지옥칼날: BLADE_SP_PCT · BLADE_SP_MIN;
    /// 어둠심판: JUDGE_SP_PCT · 0).
    pub const fn toggle_drain(max_sp: i32, pct: u8, min: u8) -> i32 {
        let d = Self::toggle_cost(max_sp, pct);
        if d < min as i32 { min as i32 } else { d }
    }
    /// ★ D4 (★ 2026-10-16): 추종's gauge after a gain — capped at FOLLOW_GAUGE_FULL.
    pub const fn follow_gauge_add(gauge: u32, gain: u8) -> u32 {
        let g = gauge + gain as u32;
        if g > Self::FOLLOW_GAUGE_FULL as u32 { Self::FOLLOW_GAUGE_FULL as u32 } else { g }
    }
    /// ★ 2026-10-16: whether `kind` (an `a.p` index) is a claw stage.
    pub const fn is_claw_kind(kind: u32) -> bool {
        kind < Self::STAGES as u32
    }
    /// ★ D4: the HP a claw hit of `d` returns at the full gauge (nothing at or below 0).
    pub const fn follow_leech(d: i32) -> i32 {
        if d <= 0 { 0 } else { d * Self::FOLLOW_LEECH_PCT as i32 / 100 }
    }
    /// ★ D4: the dark blast — the heavy attack × FOLLOW_BLAST_MUL.
    pub const fn blast_damage(power: i32, int: i32, lv: i32) -> i32 {
        Self::heavy_hit(power, int, lv) * Self::FOLLOW_BLAST_MUL as i32
    }
    /// ★ D4: the push's tile delta for a facing (1 up · 2 down · 3 left · 4 right), `None` otherwise.
    pub const fn push_delta(facing: u8) -> Option<(i8, i8)> {
        if facing < 1 || facing > 4 {
            return None;
        }
        Some((Self::PUSH_DX[(facing - 1) as usize], Self::PUSH_DY[(facing - 1) as usize]))
    }
    /// ★ D5: one lava tick from the ring's damage `d` (never below 1).
    pub const fn lava_tick(d: i32) -> i32 {
        let t = d * Self::LAVA_PCT as i32 / 100;
        if t < 1 { 1 } else { t }
    }
    /// ★ D5: 멸화's one hit — the heavy attack × FIRE_MUL.
    pub const fn fire_damage(power: i32, int: i32, lv: i32) -> i32 {
        Self::heavy_hit(power, int, lv) * Self::FIRE_MUL as i32
    }
}

/// ★★ 의상 아바타 (2026-09-30, HERO2_AVATAR_PLAN.md A0/A1) — the character's BODY · ARMS ·
/// LEGS sprites chosen by the host on top of the equipment: a costume the game itself
/// loads and draws.
///
/// ── THE LOADER (`tools_re/hero2_dis.py 'cf.a(I)V'`, HERO2_GUEST_RE.md §8.35) ──────────
/// The player is a composite of PARTS, each a `.mph/.mpd` sheet under `/hero/<part>/<nn>`:
/// `0` head · `1` body (5 pieces) · `2` arms (11 frames) · `3` legs (11 frames) · `6` weapon
/// · `7` shield. `cf.a(I)V` @0x153d18 loads ONE part: a jump table (@0x170a6c) picks the
/// case; the case reads the equipped record for that part — `a.b()Lbu;` = slot 4 갑옷 for
/// the body, `a.d()Lbu;` = slot 0 장갑 for the arms, `a.e()Lbu;` = slot 6 신발 for the legs
/// — and takes its sprite `bu.l:B` (+0x40, into `sb`) and palette `bu.a:S` (+0x44, into
/// `[sp,#0x28]`; −1 = none, the prologue's value); no record → `100 + class` (body, legs)
/// or `100` (arms), the bare part. Then a builder copied into every case makes
/// `"/hero/" + part + "/" + (n < 10 ? "0" : "") + n`, `new am(path)`, remaps the palette
/// when `bu.a >= 0` (`cf.a(Lam;SLjava/lang/String;)V`: `(from, to)` colour pairs, record
/// `bu.a` of `/hero/h.p` — 189 records of `[u16 BE len][u8 n][n × (u32 BE, u32 BE)]`),
/// and refills `cf.a[part]` / `cf.a[part + 8]` with the frames the class's sheet lists
/// (`cf.b[part]`, from `/hero/hero<class>`). Callers: `cf.a()V` (every part, entering the
/// world — `n.a(B)V` mode 2) and `cc.b(B)V` (the equipment window, after a change) — so
/// the window's preview and the field draw the same choice, and so does this override.
///
/// ── THE PATCH: the choice, overridden IN PLACE ───────────────────────────────────────
/// Each of the three cases ends its record path with 24 (arms: 25) halfwords that call the
/// getter again twice and read `bu.l` / `bu.a` — the record is still in `r0` after the
/// null check, so the repeats are dead weight. They are rewritten as: the two reads off
/// `r7 = r0` (13 halfwords, the same `bl 0x1680a4` getfield helper), then a CHECK —
/// `ldr r3,=words; ldr r2,[r3]; cmp r2,#0; blt builder; mov sb,r2; ldr r2,[r3,#4]; str
/// r2,[sp,#0x28]` — with the pointer to the host's `(sprite, palette)` pair as a literal
/// word inside the region, and the null-record path of each case routed THROUGH the check
/// too (body / legs: the `b builder` of the `100 + class` stub retargeted; arms: the null
/// path fell into the builder, so the builder's first two halfwords move into the region
/// and its head becomes `b check`). Sprite < 0 = pass through (the equipment's own);
/// otherwise `sb` = the host's sprite, `[sp,#0x28]` = the host's palette, and the game
/// does the rest — the path, the sheet, the palette, the tables, the draw. Nothing in a
/// save changes: the equipment records are only READ.
///
/// The bytes were assembled and decoded back with capstone (every `bl` reaches 0x1680a4,
/// every `ldr` the case's own literal pool word, the literal words are 4-aligned); the
/// probe `--h2-avatar` is the end-to-end check (the `cf.a[part+8]` handles change on the
/// reload, the frame dump after it shows the costume). ⛔ A sprite the host names must
/// EXIST as `/hero/<part>/<nn>` — `am.<init>` on a missing path throws inside the game's
/// own reload (the equipment window) and kills the worker; [`AvatarPatch::sprite_shipped`]
/// is the asset's list, measured, and a costume added as new sheets extends it. ⛔ A
/// palette must be `< PALETTE_COUNT` — the game indexes `h.p` without a check.
///
/// ── ★ A3b (2026-10-01): HEAD · WEAPON · OFF-HAND ─────────────────────────────────────
/// The same rewrite on cases 0, 6 and 7 (HERO2_GUEST_RE.md §8.35.11). Head = the arms'
/// shape: the record path (0x153f5a, `a.c()Lbu;` = slot 1 투구) ends `b builder`, and the
/// no-helmet path (`sb = 100`) falls into the builder at 0x153d6a, whose first two
/// halfwords move into the region. Weapon (`a.a()Lbo;` = slot 3) and off-hand
/// (`a.a()Lbu;` = slot 5) build their path from TWO numbers —
/// `"/hero/6/" + type + (n < 10 ? "0" : "") + n` with the type `ch.g` in `[sp,#0x2c]` —
/// so their check also takes a TYPE word (`[r3,#8]` → `[sp,#0x2c]`): a costume sheet is
/// named by the host's type, not the equipped one ([`AvatarPatch::path_type`]: weapon
/// `0200` · `2200` · `3200`, off-hand `10200` · `3200` — no shipped stem has that length).
/// The weapon's check sits AFTER `cf.a(B)V(type)` (the swing sound, `br.a(4, 0x33..0x35)`),
/// so the sound stays the equipped weapon's; the off-hand's region ends with the palette
/// in `r0` (the builder begins interleaved with the last read's `str r0,[sp,#0x28]`). An
/// EMPTY weapon or off-hand slot returns before any load (no `branch`): nothing is drawn
/// there, and a costume does not invent one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AvatarRegion {
    /// The part this region loads: 0 head · 1 body · 2 arms · 3 legs · 6 weapon · 7 off-hand.
    pub part: u8,
    /// The first halfword of the region.
    pub at: u32,
    /// The region as shipped, halfword by halfword.
    pub shipped: &'static [u16],
    /// The region as patched — the two halfwords at `lit_at` are 0 here and take the
    /// address of this part's `(sprite, palette)` pair at install (`block + words_off`).
    pub patched: &'static [u16],
    /// The literal word inside the region (4-aligned) the check reads its pointer from.
    pub lit_at: u32,
    /// Byte offset of this part's `(sprite, palette, type)` slot in the host block
    /// (`SLOT` × the region's index).
    pub words_off: u32,
    /// `(site, shipped, patched)`: the one halfword OUTSIDE the region that routes the
    /// null-record path (nothing equipped in that slot) through the check as well — `None`
    /// for the weapon and the off-hand, whose empty slot loads nothing.
    pub branch: Option<(u32, u16, u16)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AvatarPatch {
    /// Body, arms, legs, head, weapon, off-hand — the order of [`Self::PARTS`] and of the
    /// host block's slots.
    pub regions: [AvatarRegion; AvatarPatch::N],
}

impl AvatarPatch {
    /// The parts the override covers.
    pub const N: usize = 6;
    /// One part's slot in the host block: `(sprite, palette, type)` words — the type is read
    /// only by the weapon and off-hand checks.
    pub const SLOT: u32 = 12;
    /// The host block: six slots (72 B).
    pub const BLOCK_LEN: u32 = Self::SLOT * Self::N as u32;
    /// A sprite word below zero: the loader keeps the equipment's own choice.
    pub const PASS: i32 = -1;
    /// The parts the override covers, in region order (A0: 1..=3 · A3b: 0, 6, 7).
    pub const PARTS: [u8; AvatarPatch::N] = [1, 2, 3, 0, 6, 7];
    /// Costume sheet numbers (`tools_re/costume_sheets.py` `COSTUME_MIN..=COSTUME_MAX`): above
    /// every shipped sprite, so a costume never names a shipped file.
    pub const COSTUME_MIN: i32 = 200;
    pub const COSTUME_MAX: i32 = 999;

    /// The TYPE the loader puts in a weapon / off-hand path for a costume of this class
    /// (`ag.a:B` — 0 버서커 · 1 나이트 · 2 로그): the class's own weapon family (양손대검 0 ·
    /// 도검 2 · 단검 3, `t/itmtp.tdf`) and off-hand (방패 10 · 둘째 단검 3). `None` = the part
    /// has no type, or the class never draws it — the berserker's animation `hero/hero0`
    /// lists parts 0..=6 only, so a shield it may wear (「중장비」) is never on screen.
    pub const fn path_type(part: u8, class: u8) -> Option<i32> {
        match (part, class) {
            (6, 0) => Some(0),
            (6, 1) => Some(2),
            (6, 2) => Some(3),
            (7, 1) => Some(10),
            (7, 2) => Some(3),
            _ => None,
        }
    }

    /// Does this part's path carry a type ([`Self::path_type`])?
    pub const fn typed(part: u8) -> bool {
        matches!(part, 6 | 7)
    }
    /// Records in `/hero/h.p`: the shipped 189 (measured 2026-09-30, `[u16 BE len][u8 n]` walk
    /// lands on EOF) + the costume palettes `make_hero2_asset.py` appends (`HP_APPEND`, 2 —
    /// 「이안의 낡은 옷」 body 189 · legs 190). A palette word must be `-1` (none) or below
    /// this. ⛔ Grows with `HP_APPEND`, and only with it — a number past the file's end is
    /// read without a check by the game.
    pub const PALETTE_SHIPPED: i32 = 189;
    pub const PALETTE_COUNT: i32 = Self::PALETTE_SHIPPED + 2;

    /// Does `/hero/<part>/<nn>` exist in the shipped asset? (`hero/1`: 00..=20 + 100..=102,
    /// `hero/2`: 00..=14 + 100, `hero/3`: 00..=17 + 100..=102 — the jar's member list.)
    /// A costume shipped as NEW sheets must be added here, or it is refused.
    pub const fn sprite_shipped(part: u8, n: i32) -> bool {
        match part {
            1 => (0 <= n && n <= 20) || (100 <= n && n <= 102),
            2 => (0 <= n && n <= 14) || n == 100,
            3 => (0 <= n && n <= 17) || (100 <= n && n <= 102),
            _ => false,
        }
    }
}

/// ★★ hero2's inventory PAGING patch (#13 step 4, 2026-09-21) — how one 127-slot bag is
/// shown as five windows of 28 by the game's OWN inventory window.
///
/// ── THE PROBLEM ─────────────────────────────────────────────────────────────────────
/// `q.<init>` is `super(parent, 28, 4, 7)`: the inventory window is 7 × 4 cells and it
/// hands the bag `cell index` (the painter's `row*7+col`, the cursor `j.h`) straight to
/// `bq.a(I)Lch;` / `bq.a(BI)V` / `bq.b(B)V`. Records 28.. exist (measured, §2.7.9) and are
/// simply never drawn. Rotating the Vector instead was rejected: the last page wraps
/// page 1's records into its empty cells, and every rotation variant has the host
/// rewriting a live `java.util.Vector` and un-rotating it under the game's own removes.
///
/// ── THE MECHANISM ───────────────────────────────────────────────────────────────────
/// Every AOT call site is `ldr r0, [pc, #cell]; …; bl <dispatch helper>`, where the CELL
/// is a word the runtime resolves lazily (odd = method-table index, even = pointer to the
/// 0x1c-byte method DEFINITION whose word 0 is the body). Two helpers matter:
///
///   * `0x16917c` → resolver `0x1674cc`: VIRTUAL — reads the def only for its
///     `index_in_vtable` (+0x14) and jumps through the receiver's vtable;
///   * `0x169160` → resolver `0x16738c`: DIRECT — an even cell is `def`, jump `*(def)`.
///
/// The seven `q` sites that carry an index use the virtual helper. Each is retargeted to
/// the DIRECT helper (one `bl` immediate, 4 bytes, read-verified against the shipped
/// encoding first) and its cell is pointed at a host-written copy of the target's
/// definition whose body is a 32-byte TRAMPOLINE in a host-allocated guest block:
///
/// ```text
///   ldr r0, [pc, #16]  ; &ctx.bag           ┐ if this bag is the INVENTORY (the same
///   ldr r0, [r0]                             │ class `q` shows 재료 가방 through the
///   cmp r0, r1         ; r1 = this           │ same sites — those must see offset 0)
///   bne +14                                  ┘
///   ldr r0, [pc, #12]  ; &ctx.off
///   ldr r0, [r0]
///   adds r2, r2, r0    ; index += page * 28  (r2 = the index in all three targets;
///   ldr r0, [pc, #12]  ; body | 1               r3 = `bq.a(BI)V`'s count is untouched,
///   bx  r0                                       r0 is what every body clobbers first)
/// ```
///
/// The bag's OWN callers (drops, the `0` key, the save) keep their own cells and helpers
/// and are never offset. `bq.a(I)Lch;` answers `null` past `size`, so the fifth page's
/// 13 spare cells draw empty. The host keeps `ctx = (inventory handle, page*28)` current
/// and resets the page when the window opens.
///
/// Same regime as [`GuestBuild::hud_bottom_patch`]: addresses measured in OUR image
/// (`tools_re/hero2_dis.py`), read-verified before the write, proven headless (probe
/// `--h2-pages 1 --h2-page <f:k>`), locked against hero3 by `anchors_selftest`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InventoryPagePatch {
    /// The seven index-carrying call sites in `q`, root-first in the disassembly.
    pub sites: [PageSite; 7],
    /// The three bag methods those sites reach, indexed by [`PageSite::target`]:
    /// the RUNTIME method-definition record (word 0 must read `body | 1`) and the body.
    pub targets: [PageTarget; 3],
    /// `bl` target the sites ship with (virtual dispatch).
    pub helper_virtual: u32,
    /// `bl` target they are retargeted to (direct dispatch through the cell).
    pub helper_direct: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PageSite {
    /// Address of the 4-byte `bl` instruction.
    pub bl: u32,
    /// The site's resolution cell (a data word in the image's literal area).
    pub cell: u32,
    /// Index into [`InventoryPagePatch::targets`].
    pub target: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PageTarget {
    pub def: u32,
    pub body: u32,
}

impl InventoryPagePatch {
    /// A method definition record is 0x1c bytes (`JavaMethodDefinition`, dump_hero2_java.py).
    pub const DEF_LEN: u32 = 0x1c;
    /// The host block: `ctx.bag` · `ctx.off` · three defs · three trampolines.
    pub const CTX_BAG: u32 = 0x00;
    pub const CTX_OFF: u32 = 0x04;
    pub const DEFS: u32 = 0x08;
    pub const TRAMPS: u32 = 0x60;
    pub const TRAMP_LEN: u32 = 0x20;
    pub const BLOCK_LEN: u32 = 0xc0;

    pub const fn def_at(block: u32, target: u8) -> u32 {
        block + Self::DEFS + Self::DEF_LEN * target as u32
    }

    pub const fn tramp_at(block: u32, target: u8) -> u32 {
        block + Self::TRAMPS + Self::TRAMP_LEN * target as u32
    }
}

/// ★★ 2026-10-20 인벤 「사용하기」 on bag pages 2..5 (user: *"아이템창에서 포션을 직접 사용하기하면 사용이 안되고 포션갯수도
/// 안줄어든다"*). The popup answer `q.a(BB)V` kind 6 row 0 calls the CHARACTER's `a.g(B)V(j.h)` (@0x146b54, cell
/// 0x17d320) — not the bag — so [`InventoryPagePatch`]'s seven bag sites never saw it, and `a.g(B)V` looks the index up
/// in `ag.a` itself (`bq.a(I)Lch;` through its own cell 0x17eed8): the page-local cursor picked page 1's record at that
/// cell. A non-recovery record there = nothing happens (the report); a recovery one = THAT item is drunk instead.
///
/// The fix is the paging patch's mechanism for one more site: the `bl` → the direct helper, the cell → a copy of
/// `a.g(B)V`'s runtime definition whose word 0 is a trampoline. The trampoline adds the paging context's offset when the
/// paged bag is the inventory (`ag.a`), then jumps through the LIVE definition word — not the body — so a wrapper chained
/// there (GearPatch's 술꾼의 장화 unit) still runs whichever block installed first. No job class overrides `g(B)V`
/// (`ay` · `bl` · `bx` have none), so the direct dispatch reaches the same method the virtual one did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ItemUsePagePatch {
    /// The `bl` in `q.a(BB)V` and its resolution cell.
    pub bl: u32,
    pub cell: u32,
    /// `a.g(B)V`'s runtime definition record (#943) and its shipped body.
    pub def: u32,
    pub body: u32,
    /// The inventory static `ag.a:Lbq;`.
    pub inventory_slot: u32,
    pub helper_virtual: u32,
    pub helper_direct: u32,
}

impl ItemUsePagePatch {
    /// The block: the definition copy, then the trampoline.
    pub const DEF_AT: u32 = 0x00;
    pub const CODE_AT: u32 = 0x20;
    pub const BLOCK_LEN: u32 = 0x60;
}

/// Where the two record base-name strings ("GameSave…" / "SlotSave…") live on a build.
/// The save chain passes these to the record writer, which appends `"_<slot>"` itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveNames {
    /// KTF `client.bin`: the RWPI `sl` table holds POINTERS to the strings
    /// (`"./Hero3GameSave"` / `"./Hero3SlotSave"`) — read them at runtime so a
    /// relocated build still resolves.
    SlPtrs { game: u32, slot: u32 },
    /// LGT `binary.mod`: absolute address model, the strings themselves are compiled
    /// into `.text` — pass the addresses straight through, nothing to read.
    Direct { game: u32, slot: u32 },
}

/// Everything per-build the host-driven save (quick-save / autosave) needs: the three
/// callable primitives, the out-parameter scratch word, the CHAR-relative slot-number
/// offset, and where the record names come from. `None` = the chain is not ported to
/// this build and the save UI must not offer it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SaveChain {
    /// `serialiseSave(u32* outLen) -> buf`. Thumb bit set.
    pub fn_serialise: u32,
    /// `slotSummary() -> buf16`. Thumb bit set.
    pub fn_slot_summary: u32,
    /// `writeSaveRecord(baseName, buf, len)` — appends `"_<slot>"` itself, then the
    /// 0x10-header + encrypt + WIPI DB write. Thumb bit set.
    pub fn_write_record: u32,
    /// Four bytes of guest memory PROVEN out of the game's reach, used as the
    /// serialise step's out-parameter. See each arm for that build's proof.
    pub scratch: u32,
    /// CHAR-relative offset of the save slot number (`"%s_%d"`'s `%d`).
    pub slot_off: u32,
    pub names: SaveNames,
}

/// Shape of the equip-level-relief rewrite — see [`GuestBuild::equip_relief_site`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EquipReliefSite {
    /// KTF: rewrite the one-halfword register move (`adds r1,r3,#0` → `subs r1,r3,#N`)
    /// at `site`. Encoded by `buffs::equip_relief_halfword`.
    MoveHalfword { site: u32 },
    /// LGT: rewrite the two-halfword PLAYER-LEVEL load (`movs r3,#0;
    /// ldrsh r1,[r4,r3]` → `ldrh r1,[r4,#0]; adds r1,#N`) at `site`, so the check
    /// compares `playerLevel + N` against the shipped requirement. Encoded by
    /// `buffs::equip_relief_level_bias`.
    ///
    /// ── WHY THIS SIDE OF THE COMPARISON AND NOT THE OTHER ────────────────────────
    /// §2.8 proposed rewriting the REQUIREMENT load instead (`movs r3,#2;
    /// ldrsb r3,[r2,r3]` → `ldrb r3,[r2,#2]; subs r3,#N`, four bytes at `0x477d2`).
    /// That works arithmetically and it was fully plumbed and tested in 6차, but it
    /// silently changes the SIGNEDNESS of the field it loads: the shipped instruction
    /// is `ldrsb` and the replacement `ldrb`, so any item whose requirement byte is
    /// ≥ 0x80 flips from "allowed" (`ldrsb` → negative → the signed `bge` passes) to
    /// "blocked" (`ldrb` → 128..255, minus N, still far above any level). A `-1`
    /// sentinel for "no requirement" is exactly the encoding a C `signed char` field
    /// invites, and nothing in the item data had been checked for one. That is the
    /// silent-bug class this migration keeps finding, so the patch moved to the other
    /// operand instead of arguing the hazard away.
    ///
    /// Biasing the player level is provably equivalent, not merely plausible:
    /// * The base is the CHAR record's level. `0x477cc adds r4,#0xba` on top of the
    ///   accessor that §2.8 showed returns `CHAR+0x28` puts `r4` at `CHAR+0xe2`, which
    ///   is `items::LEVEL_OFF` — the same halfword the HUD reads as `lv` and the same
    ///   one `--verify-offsets` printed as `lv=Some(1)`.
    /// * Level is a small positive: the game's own level-up routine caps it at 70
    ///   (`items::LEVEL_CAP`), and `items::resolve_char` refuses a record whose level
    ///   is outside `1..=70`. For every value the field can hold, `ldrh` and `ldrsh`
    ///   return the same thing — so this rewrite changes the comparison and nothing
    ///   else, for any item in the game.
    ///
    /// Both halfwords were read back out of RUNNING guest memory before this shipped
    /// (`wie_probe --poke 7850:0x477ce:21880531` logged `0023e15e`, exactly what the
    /// static disassembly predicted), and the same run's frame hashes matched the
    /// unpatched run's.
    LevelBias { site: u32 },
}

/// The words the game's own `showModal` writes to raise its "연결이 끊어졌습니다" box —
/// how the host both TELLS the player the link is gone and gets them back out of the
/// network screens. `None` on a build where they have not been located.
///
/// ── WHY THE HOST HAS TO WRITE THESE AT ALL (2026-08-15, 48차) ────────────────────
/// 47차 established, on the device, three things that between them kill every simpler
/// idea (PLAY_FEEDBACK_FIXES.md §47차-D):
///   1. closing the socket does not change the screen — the guest ran its own
///      `MC_netSocketClose` + `MC_netClose` and still sat on its loading modal;
///   2. a synthetic reply frame only opens a box on SOME screens (`05 42` status-0
///      works in the cash shop and not in the network warehouse);
///   3. closing that box leaves the player exactly where they were.
/// So the message and the way OUT are two different problems, and the game already
/// solves both — in code that Android backgrounding never reaches.
///
/// ── WHAT WAS FOUND, AND RE-VERIFIED HERE BEFORE IT WAS USED ─────────────────────
/// The online module (title / slot / single-warehouse / network-warehouse / cash shop,
/// i.e. everything `items::shell_state` calls state 3) draws and keys through two
/// tables indexed by ONE word:
/// ```text
///   0x60ac  moduleDraw(base): screenDraw[*(base+0xa3d4)](base)
///                             modalDraw [*(base+0xa438)](base)   <- painted on top
///   0x6164  moduleKey (base,key): if (modalKey[*(base+0xa438)](base,key)) return
///                                 screenKey[*(base+0xa3d4)](base,key)
/// ```
/// `getBase()` (`0x1068`, `ldr r0,[pc,#0]; bx lr`) returns **`0x1503ab0`**, the same
/// `CHAR+0x28` the EXP and auto-loot work already stand on. Both tables were rebuilt
/// from their initializer at `0x59f0` rather than trusted second-hand, and entry **5**
/// is the pair that matters:
/// ```text
///   modalDraw[5] = 0xb608   panel if *(base+0xa7fc) != 0, then the text at base+0xa43c
///   modalKey [5] = 0xbe30   key == -5 (OK):  closeModal(0)          (0x4604)
///                                            netTeardown(0)         (0x18608)
///                                            if (screen != 9) screen = 1
/// ```
/// **That is the whole missing half**: the game's own OK handler tears the connection
/// down and puts the player back on the online module's main screen. The game uses the
/// same modal itself in exactly this situation — `0x1417c` (the WIPI *pause* notifier,
/// which on a real handset fired on an incoming call) and the 5-minute idle watchdog
/// `0x18a0c` both do `netTeardown(0); showModal("서버와의 접속이 끊어졌습니다.", 5, 1, 1)`.
/// Android never calls `pauseClet`, and by the time anything else notices, the guest's
/// own teardown has cleared `sessionActive` (`0x150ea94`) — which is the precondition
/// of `0x1417c`, so the shipped notifier can no longer fire. Hence: write what it writes.
///
/// The five writes below are `showModal(text, 5, 1, 1)` (`0x4684`) verbatim, in its
/// order, with the state word LAST — see [`NetDisconnectModal::writes`].
///
/// ★ It STAYS up: of the 16 code sites in the image that load the `0xa438` offset,
/// only `showModal`/`closeModal`, the `cat 0x64` kick handler (`0x4a10`), the connect
/// paths and modal-key 2 write it — nothing re-asserts a modal per frame, so the box
/// cannot be overwritten by a link that is already dead. And `netTeardown` (`0x18608`)
/// touches neither this word nor the screen word, so the OK path is not self-erasing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NetDisconnectModal {
    /// u32. The modal id, index into BOTH modal tables. `0` = no modal, so writing it
    /// is what makes the box exist.
    pub state_word: u32,
    /// The id whose OK key runs the game's own teardown and leaves the network screens.
    pub disconnect_state: u32,
    /// The EUC-KR text buffer the drawer prints (NUL-terminated, `text_cap` bytes).
    pub text_buf: u32,
    pub text_cap: usize,
    /// u8, non-zero = draw the translucent panel behind the text. The drawer skips the
    /// panel entirely when this is 0, which would leave the words floating on the map.
    pub panel_flag: u32,
    /// u8, the per-modal sub-flag. Modal 5's drawer does not read it, but `showModal`
    /// always writes it and a stale value belongs to whatever box was up before.
    pub sub_flag: u32,
    /// u32 the game's own `showModal` zeroes on every open (`ctx+0x40`).
    pub clear_word: u32,
}

/// "네트워크 연결이 끊어졌습니다." in EUC-KR — the payload for
/// [`NetDisconnectModal::writes`], and the same wording the host toast uses.
///
/// Hardcoded bytes rather than an encoder, because this file may not grow a dependency.
/// They are not eyeballed: every syllable here also appears in a string the game itself
/// ships — `끊어졌습니다.` is the tail of `0x5c5b4` ("서버와의 접속이 끊어졌습니다.") and
/// `연결이 ` the middle of `0x5c13c`, byte for byte.
pub const NET_DISCONNECT_TEXT_EUCKR: &[u8] = &[
    0xb3, 0xd7, 0xc6, 0xae, 0xbf, 0xf6, 0xc5, 0xa9, 0x20, // 네트워크
    0xbf, 0xac, 0xb0, 0xe1, 0xc0, 0xcc, 0x20, // 연결이
    0xb2, 0xf7, 0xbe, 0xee, 0xc1, 0xb3, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e, // 끊어졌습니다.
];

impl NetDisconnectModal {
    /// The guest writes that open the box, as `(address, bytes)` in the order they must
    /// be applied.
    ///
    /// This is `showModal(text, 5, 1, 1)` (`0x4684`) rewritten as data:
    /// ```text
    ///   if (*textBuf) memset(textBuf, 0, 0x100);   ; -> one zero-filled buffer write
    ///   sprintf(textBuf, "%s", text);              ;    with the text at the front
    ///   *(u8*) panelFlag = 1
    ///   *(u32*)stateWord = 5
    ///   *(u32*)clearWord = 0
    ///   *(u8*) subFlag   = 1
    /// ```
    /// ★ The state word goes LAST even though the game writes it third. The host writes
    /// from the emulator thread between guest ticks, so a frame cannot land mid-sequence
    /// — but ordering it this way means no ordering argument is needed at all: until the
    /// last write the tables still index the modal that was there, and after it every
    /// field the drawer reads is already in place.
    ///
    /// The text is truncated (never wrapped) to leave room for the terminator: the
    /// drawer stops at the NUL and a buffer with no NUL would run off the end.
    pub fn writes(&self, text: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut buf = vec![0u8; self.text_cap];
        let n = text.len().min(self.text_cap.saturating_sub(1));
        buf[..n].copy_from_slice(&text[..n]);
        vec![
            (self.text_buf, buf),
            (self.panel_flag, vec![1]),
            (self.sub_flag, vec![1]),
            (self.clear_word, vec![0, 0, 0, 0]),
            (self.state_word, self.disconnect_state.to_le_bytes().to_vec()),
        ]
    }
}

/// Where the auto-loot flag lives, and what has to be in it. See
/// [`GuestBuild::autoloot_flag`] for the disassembly of both the writer and the reader.
///
/// It is a THREE-part answer and every part has drawn blood: the address is an array,
/// not a constant; the index is the save slot; and the value has to have bit 7 set
/// because the reader tests the byte signed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AutolootFlag {
    /// CHAR-relative address of element 0.
    pub base_off: u32,
    /// `Some(off)` = the flag is a byte ARRAY and the u32 at `CHAR+off` picks the
    /// element. `None` = a single byte at `base_off` (KTF, where only slot 0 was ever
    /// observed).
    pub slot_off: Option<u32>,
    /// What the game's own `useItem` arm stores. NOT a boolean: the LGT reader
    /// sign-extends the byte and branches on `< 0`, so a value without bit 7 is ignored
    /// no matter how right the address is.
    pub value: u8,
}

impl AutolootFlag {
    /// Highest slot the array is allowed to be indexed by, matching the save-slot bound
    /// the rest of the crate enforces (`save::SAVE_SLOT_MAX`). A word outside this is a
    /// record that is not loaded yet, and indexing on it would write a byte somewhere
    /// arbitrary in the record — the exact hazard this table exists to prevent.
    pub const MAX_SLOT: u32 = 15;

    /// The flag's CHAR-relative offset for a given save-slot word, or `None` when that
    /// word is not a slot number this may be indexed by.
    pub fn offset_for(&self, slot: Option<u32>) -> Option<u32> {
        match (self.slot_off, slot) {
            (None, _) => Some(self.base_off),
            (Some(_), Some(s)) if s <= Self::MAX_SLOT => Some(self.base_off + s),
            _ => None,
        }
    }
}

/// hero2's host-driven save — the answer to "can the SAVE button save the game the way
/// hero3's does?" (2026-09-19).
///
/// ★★ It is NOT a [`SaveChain`], and the difference is the whole point. hero3's chain is
/// three raw ARM addresses the host calls in sequence, threading a serialised buffer and
/// its length through a scratch word, because hero3's guest is a symbol-less C binary
/// and there was no other way in. hero2's guest is an AOT-compiled Java MIDlet whose
/// metadata tables survived, so its save is one PUBLIC METHOD — `ag.l()V` — which does
/// both halves itself:
///
/// ```text
/// ag.l()V  @0x113f7c
///   slot = ag.c:B                    (the slot the character was loaded from)
///   this.g(slot)   ag.g(B)V  @0x114110   -> writes /n<slot>   (character, 1352 B)
///   ag.f(slot)     ag.f(B)V  @0x1147d8   -> writes /g<slot>   (progress,   889 B)
/// ```
///
/// The host therefore does not serialise, does not allocate, does not own a buffer and
/// cannot get the length wrong — it names the method and the JVM runs the same dispatch
/// the guest's own menu runs (`Emulator::call_java_singleton_void`). Verified headless
/// against a save pulled off the device: one call produced the same two records, with
/// `/n1` byte-identical to what the game's own SYSTEM -> 세이브 -> 예 produces, and a
/// following run BOOTED that save and read back the state as of the call.
///
/// [`Hero2Save::area`] is the game's own refusal, mirrored rather than skipped — see
/// [`Hero2SaveArea`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hero2Save {
    /// The receiver: `ag.a:Lag;` — resolved through the JVM by name, so these three
    /// strings and not an address are what the call is made of.
    pub class: &'static str,
    pub field: &'static str,
    pub descriptor: &'static str,
    /// The method: `l`, descriptor `()V`.
    pub method: &'static str,
    /// The game's own "may I save here?" test.
    pub area: Hero2SaveArea,
}

/// The boss-map refusal, read exactly as `az.a()Z` @0x120fc0 reads it before it offers
/// the `저장하시겠습니까?` confirm:
///
/// ```text
/// area = *(ag.a:Lbf; @0x193afc)          ; the live stage object
/// if  (i8) area.b:B (+0x0)  <  (i8) bf.a:B @0x193fbc   -> allowed
/// else                                   -> gm.tdf[70] 보스맵에서는 저장할 수 없습니다.
/// ```
///
/// ★ The host mirrors this instead of just calling `ag.l()V`, because `ag.l()` itself
/// has NO guard — it writes whatever it is told to write. A SAVE button that ignored
/// the test would happily persist the player inside a boss room the game deliberately
/// refuses to save in, and the player would discover it only on the next load. The
/// check is two reads and it fails CLOSED: an unreadable chain means "do not save",
/// which costs a tap and cannot cost a character.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hero2SaveArea {
    /// `ag.a:Lbf;` — the static slot holding the live stage object.
    pub stage_slot: u32,
    /// `bf.b:B` — the stage's type byte, at this offset inside the field block. It is the
    /// MAP NUMBER (`P/m/m/<n>.m.gz`; 72 on 신드라프, 101+ are the boss rooms the guard
    /// refuses), which is also what the minimap's exit test compares a warp's target with.
    pub stage_type_off: u32,
    /// `bf.a:B` — the static threshold the type is compared against.
    pub max_saveable_slot: u32,
    /// `bf.a:[Object` — the live map's TRIGGER GROUPS (2026-09-29, minimap exits): one
    /// `byte[m][31]` per group (`null` when the group has no records), rebuilt by the event
    /// loader `bf.a([BI)I` @0x12af98 on every map load. A tile belongs to group
    /// `|(i8)attr| − 1` (`HERO2_GUEST_RE.md` §8.29).
    pub trigger_groups_slot: u32,
    /// `bf.b:[Object` — the live map's SCRIPTS, one `byte[m][3]` (command · arg · arg)
    /// each, filled by the same loader. A record's byte 30 indexes it.
    pub scripts_slot: u32,
    /// `ag.q:[B` — the game's EVENT FLAGS (2026-09-29, minimap chests): flag `i` is bit
    /// `i & 7` of byte `i >> 3`, exactly as `ag.a(I)Z` @0x111a70 reads it; script command
    /// `12 f 00` sets one. A treasure chest's give script sets the flag its "비어있다" record
    /// checks, so this is also where the host reads "already opened".
    pub event_flags_slot: u32,
    /// `ag.r:[B` — the game's QUEST STATES (★ 2026-10-01, 의상 아바타 A5): quest `i` is two bits,
    /// `(ag.r[i >> 2] >> ((i & 3) << 1)) & 3` exactly as `ag.a(I)B` @0x111ba8 reads it, 2 = done —
    /// what `ag.a()B` @0x115008 counts over slots 0..=50 for the round's 달성도. Cleared by `ag.t()V`
    /// @0x111e34 on every world entry (event 9); a load then reads the save's back.
    pub quest_states_slot: u32,
}

/// ★★ hero2's JAVA-side anchors (2026-09-17, `HERO2_QUICKSLOT_TOUCH_PLAN.md` §3 / §8.2).
///
/// hero2 is an AOT-compiled Java MIDlet whose class/field/method tables survive in the
/// image, so every anchor here is a NAMED static slot or a named instance offset,
/// measured twice: once from the SKT decompile (same source, different obfuscation —
/// the class map is in the plan's §3.1) and once live with `wie_probe --dump-mem`
/// (`docs/quickslot_touch_plan/hero2_chain.py`, `rd_static_array.py`).
///
/// Shapes to remember when reading through these:
/// * a static slot holds the VALUE (an int/byte) or an object HANDLE;
/// * an object handle `h` → `*(h) = ptr_fields`, `*(h+4) = ptr_class`;
/// * an instance field is at `ptr_fields + 4 + abs_off` (parents' fields first);
/// * an ARRAY handle → `len = *(ptr_fields+4)`, elements from `ptr_fields+8`;
/// * `ptr_class` → a 0x14-byte `JavaClass`; `*(ptr_class+8)` is the class DESCRIPTOR's
///   guest address, which is what the `*_desc` fields below compare against.
///
/// Everything here is READ-ONLY data unless `guest_build::java_write_ok` opens a
/// specific write — this table opens no hero3 address and no guest call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hero2Java {
    /// `ag.a:La;` — the character object (the same slot `CharAnchor::JavaObject` names;
    /// repeated here so this table is self-contained for the dependency-free readers).
    pub char_slot: u32,
    /// `ag.j:[B` — the quickbar, a `byte[4]`: `-1` empty · `1..7` = the class's n-th
    /// ACTIVE skill · `10+k` = the beast's k-th skill. Measured `[1, 10, 5, 2]` against
    /// the game's own HUD cells and the skill window's `1/9/7` badges.
    pub quickbar_slot: u32,
    /// `ag.a:[I` — per-slot cooldown, `int[4]`, `-1` = ready, else ticks left (the game
    /// decrements it every frame in `ag.i()`).
    pub cooldown_slot: u32,
    /// `ag.a:B` — the character's class index 0..2 (버서커 · 나이트 · 로그). Selects the
    /// `img/gmico` strip and the cooldown table.
    pub class_slot: u32,
    /// `ag.c:B` — the save slot the character was loaded from (0..2): the slot list's
    /// cursor at load (SKT `ca.java` writes it, `b.a()` serialises it first). Half of
    /// the plate store's owner key.
    pub save_slot_slot: u32,
    /// `a.a:Lbn;` at this ABSOLUTE offset in the character's field block — the beast
    /// object. Its `bn.j:[B` (at [`Hero2Java::beast_skill_ids_off`]) maps beast skill
    /// slot `k` to the GLOBAL beast skill id, which is the `img/bico` frame.
    pub beast_off: u32,
    pub beast_skill_ids_off: u32,
    /// ★ #22 (2026-09-23): `bn.k:[B` — the beast's skill LEVEL per slot, the array
    /// `af.a(BB)V` increments when the player spends a point on a beast skill
    /// (`0x10fb44`, an array store of `bn.k[j.h] + 1` right beside its `a.d -= 1`) and
    /// the array `bn.d()V` sums back into `a.d:S` when the beast evolves. That pair is
    /// what identifies it: one hand takes a point per level, the other gives them back.
    pub beast_skill_levels_off: u32,
    /// ★ #22: the character's own growth fields, absolute in its field block
    /// (`HERO2_GUEST_RE.md` §8.6 — the level-up `a.l()V` writes the first two).
    /// `a.c:S` unspent STAT points, `a.d:S` unspent SKILL points, `a.e:S`..`a.i:S` the
    /// five base abilities 힘·체력·민첩·의지·지력 at `CHAR_ABILITY_STRIDE` apart, and
    /// `a.p:[B` the per-skill LEVEL array (`CHAR_SKILL_SLOTS` long, `-1` where the class
    /// has no such skill — `a.b()V` fills it that way at character creation).
    pub char_stat_points_off: u32,
    pub char_skill_points_off: u32,
    pub char_abilities_off: u32,
    pub char_skill_levels_off: u32,
    /// ★★★ 2026-09-23 (기본공격 버그): `a.h:B` — how many COMBO stages are open — and
    /// `a.i:[B` @0x19455c — the per-class combo CEILING `[4, 3, 5]`, indexed by
    /// [`Self::class_slot`] (버서커 4 · 나이트 3 · 로그 5).
    ///
    /// ⭐ These two settle what the FIRST slots of `a.p:[B` are, and it is not skills.
    /// `a.p()V` @0x15234c walks `a.p[0 .. a.i[class])`, stops at the first element that is
    /// `<= 0`, and stores the count in `a.h:B` — so those leading slots are the stages of
    /// 「기본공격」 (gm row 25) and the class's own skills begin AFTER them. Three
    /// independent checks agree: the per-slot caps `<job>.a(I)B` return `i + 1` over
    /// exactly that leading run (a 3rd stage cannot pass level 3) and a flat 5/3 after it;
    /// the remaining slots come to 11 · 12 · 10, which is `t/sk0` · `sk1` · `sk2` exactly;
    /// and 4+11 = 3+12 = 5+10 = 15 = [`Self::CHAR_SKILL_SLOTS`].
    ///
    /// ⛔ `a.p[0] >= 1` is an INVARIANT, not a level the player bought: `a.b()V` writes
    /// `a.p[0] = 1` and `a.h = 1` at creation (@0x14d774, @0x14d7b0) before a point can be
    /// spent anywhere. Zeroing it leaves `a.h = 0`, which the game has no way to produce —
    /// that is what the skill reset used to do and what the player reported as a crash.
    pub char_combo_open_off: u32,
    pub combo_limits_slot: u32,
    /// ★ 2026-09-24 (조이스틱): the field's WALK REQUEST pair, `ag.k:B` @0x193bec and
    /// `ag.l:B` @0x193bfc — what a direction key turns into. `n.keyNotify(II)Z` @0x16261c
    /// routes a PRESS in state 3 to `n.a(II)V` @0x162838 → `ag.b(B)V(dir)` @0x110980:
    /// standing (`g.t == 1`) starts the walk on the spot (`ag.d(2, dir)`), walking
    /// (`g.t == 2`) queues `k = 2, l = dir`. A RELEASE while walking queues `k = 1,
    /// l = facing` (`ag.b()V` @0x110a08). `ag.c()V` @0x110c90, called from `n.paint`,
    /// applies the pair at the next step boundary and clears it — ONE slot, newest wins.
    ///
    /// ⛔ And a release is looked at ONLY in state 3: `0x162652..0x162660` returns before
    /// the walk check for every other state word (a script, a window, a load). A release
    /// the host sends while one of those is up is simply gone, and the character walks
    /// on until something else stops it. `hero2java::h2_field_walk` is the reader the
    /// host's walk guard (`emulator::WalkGuard`) and the probe's `[h2-walk]` gauge use;
    /// the model is written up in `HERO2_GUEST_RE.md` §8.14.
    pub walk_request_slots: (u32, u32),
    /// `(class descriptor guest address, static `[S a` slot)` for the three character
    /// classes: the cooldown TOTAL per active skill. Only the loaded class's slot is
    /// non-null (the other two classes' `<clinit>` never ran), which is why the lookup
    /// goes through the character object's own `ptr_class` rather than `class_slot`.
    pub cooldown_tables: [(u32, u32); 3],
    /// ★ #22: `(class descriptor guest address, the five base abilities)` — what
    /// `<job>.b()V` writes as LITERALS right after calling `a.b()V`, so it is the
    /// class's bare floor with no level scaling and no spent points (`ay.b()V`
    /// @0x11fb00, `bl.b()V` @0x132bfc, `bx.b()V` @0x13b39c; `HERO2_GUEST_RE.md` §8.6).
    /// The stat reset puts the abilities back to these and refunds the difference.
    ///
    /// ⛔ **Keyed by descriptor, never by position.** The factory `a.a(BSS)La;`
    /// @0x14d5dc dispatches `class == 0 -> new bl`, `1 -> new bx`, `2 -> new ay`, so the
    /// alphabetical order `ay, bl, bx` is 로그, 버서커, 나이트 — reading an index here as
    /// a class number buffs the wrong job. Same reason `cooldown_tables` is a lookup.
    pub class_base_stats: [(u32, [i16; 5]); 3],
    /// `ag.a:Lbq;` — the INVENTORY (SKT `u.a : ci`): `+0 a:Vector` of item records
    /// (`ch` = SKT `cp`), `+8 a:B` = the index of the SELECTED recovery item — what the
    /// `0` key uses (`b.r()`: `bq.a()` → record → use; count < 1 → `bq.b()` = the `#`
    /// swap). RE 2026-09-17, SKT `b.r()`/`bd` case 48/35. The next two slots are the
    /// two sibling bags below (settled 2026-09-20, HERO2_GUEST_RE.md §2.7.2).
    pub inventory_slot: u32,
    /// `ag.b:Lbq;` — the 재료 가방 (built 28; ★ 2026-10-09 the host keeps it at 127 and pages its window like
    /// the inventory's), opened from inside the inventory by the 재료가방 record (type 29); and `ag.c:Lbq;` —
    /// the 카드 보관함 (15) behind the CARD row. Same class as the inventory, so `hero2java::h2_bag` reads all
    /// three (HERO2_GUEST_RE.md §2.7.2).
    pub material_bag_slot: u32,
    pub card_bag_slot: u32,
    /// `bw.a:Lbw;` — the field-menu window singleton, non-null while the menu (or any
    /// window opened from it) is up. Root of the window chain: `.b` (+4) is the child.
    pub menu_slot: u32,
    /// `bw.a:I` / `bw.b:I` — the field menu panel's top-left, in framebuffer pixels.
    /// COMPUTED by the game from the framebuffer size ((28,51) at 176x220, (166,64) at
    /// 452x245), which is why it is read every time and never hard-coded.
    pub menu_origin: (u32, u32),
    /// `t.a:Lt;` — the TITLE carousel singleton (SKT `bg.a`), non-null from the title
    /// screen on: the root of the SHELL window chain (slot list, class pick, OPTION,
    /// HELP and their popups hang off its `.b`). `t` = SKT `bg` by its fields alone —
    /// statics `a:I b:I a:Lt;`, one `Z a`, `<init>(Z)V` — the only `j` subclass shaped
    /// so (`dump_hero2_java.py --fields-size 4`, 2026-09-17).
    pub title_slot: u32,
    /// `t.a:I` / `t.b:I` — the shell's panel origin (`bg`: `ax.j − 60, ax.k − 58`), which
    /// the title passes to the HELP pager and the message box it opens. The field
    /// menu's `bw.a/b` are NOT set on the shell (they are written when the menu opens).
    pub title_origin: (u32, u32),
    /// `bw.a:Li;` — the `/t/gm` text table instance (`i.a:[Object` = `byte[][]`). The
    /// popup's label array holds references to THESE objects, not copies.
    pub gm_table_slot: u32,
    /// `bw.a:B` — the field menu's remembered row (see [`JavaWrite::MenuLastRow`]).
    pub menu_last_row_slot: u32,
    /// `bq.j:[B` — the recovery-item CYCLE the `#` key walks and the `0` key uses
    /// (SKT `ci.j = {24, 25, 26}` = 포션 · 하이포션 · 포도주; 27 치료제 is a recovery
    /// type for the popup but NOT in the cycle). Read live, never assumed.
    pub heal_cycle_slot: u32,
    /// `ax.g` / `ax.i` / `ax.j` / `ax.k` — screen width, FULL framebuffer height, and
    /// their halves. Every centred window (the popup box included) is placed from these.
    pub screen: (u32, u32, u32, u32),
    /// `s.b:B` — the SKILL window's static that the cursor painter tests (`== 4` = the
    /// ACTIVE tab's cell boxes sit 10 px lower and 14 px narrower than PASSIVE's). Read,
    /// not derived: the decompile shows the store but not the value's meaning.
    pub skill_static_b: u32,
    /// `bb.b:B` — the SCRIPT system's box MODE (2026-09-18, `HERO2_GUEST_RE.md` §3.3):
    /// `0` none · `1` auto-advancing narration · `2` a line waiting for OK · **`3` a
    /// 예/아니오 CHOICE** · `4` (unobserved; OK sets `bb.h`) · `5` 처리중. Read straight
    /// out of `bb.a(II)V`, the key handler: mode 3 is the only mode where LEFT/RIGHT do
    /// anything (they flip [`Self::script_choice_sel_slot`]), and the mode-3 painter
    /// `bb.b(Graphics)V` is the one that calls the popup class's two-button painter
    /// `d.a(Graphics, I, Z)V`. This is hero2's `items::script_choice_open`.
    pub script_mode_slot: u32,
    /// `bb.d:Z` — which of the two choice buttons is lit (the `Z` handed to
    /// `d.a(Graphics, I, Z)`); `1` on every intro frame, i.e. the default is the
    /// right-hand 아니오 like the slot screen's own confirm. Read-only for now.
    pub script_choice_sel_slot: u32,
    /// Descriptor guest addresses of the classes the router tells apart by `ptr_class`.
    pub classes: Hero2Classes,
}

/// See [`Hero2Java::classes`]. Names are the KTF obfuscated ones (`dump_hero2_java.py`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hero2Classes {
    /// `j` — the abstract window base every screen extends: `+0 a` parent, `+4 b` child,
    /// `+8 h` cursor, `+0xc i` row count.
    pub window: u32,
    /// `d` — the context popup (드랍박스): `+0x28 kind:B`, `+0x2c labels:[[B`,
    /// `+0x30 height:I`, plus the inherited cursor/rows.
    pub popup: u32,
    /// ★ `aq` — the game's own QUANTITY box, 「개수를 입력하세요.」 (`com.tdf[51]`), with
    /// `+0x28 a:I` a unit price (0 = no total line) and `+0x2c a:B` the maximum; the
    /// number itself is the inherited cursor `j.h`. ⛔ NOTHING IN THIS BUILD CONSTRUCTS IT
    /// — the only caller was the network warehouse that died with its server, and an
    /// image-wide cell xref finds no `aq.<init>` call site. The host builds it
    /// ([`JavaCall::QuantityPopupNew`]) and hangs it where the record's popup was; the
    /// game paints it, runs its keys and answers `parent.a(50, n)` on OK, which every
    /// window in this game treats as "close the child". HERO2_GUEST_RE.md §2.7.12.
    pub quantity_popup: u32,
    /// `s` — the SKILL window: `+0x2c b:Z` tab (true = PASSIVE, false = ACTIVE),
    /// `+0x30 d:B` column split, inherited `h`/`i`.
    pub skill_window: u32,
    /// `af` — the BEAST skill window (same popup; registers `h + 10`).
    pub beast_window: u32,
    /// `bw` — the field menu (8 rows).
    pub field_menu: u32,
    /// `bn` — the beast.
    pub beast: u32,
    /// `an` — STATUS (SKT `bp`): a 3x2 stat grid whose second tap SPENDS a point; only
    /// live while `+0x34 a:Z` (has points) is set.
    pub status_window: u32,
    /// `cc` — EQUIPMENT (SKT `co`): 7 slots in a 3x3 layout.
    pub equip_window: u32,
    /// `q` — INVENTORY (SKT `bf`): 28 cells, 7 per row.
    pub inventory_window: u32,
    /// `az` — SYSTEM (SKT `ad`, 4 rows), drawn with the shared centred list painter.
    /// ★ `be` was listed here as "the shell's slot list, same painter" until 2026-09-17;
    /// SKT `ca.b(Graphics)` is its own painter (label · `SLOT n` arrow box · summary) and
    /// it is [`Self::slot_window`] now.
    pub list_windows: [u32; 1],
    /// `t` — the TITLE carousel (SKT `bg`, 9 rows: 새로하기 · 이어하기 · 옵션 · 도움말 ·
    /// 삭제 · 3 web links · 종료) shown as ONE label in an arrow box at
    /// `(ax.j, ax.k + ax.k/2 − 12)`; LEFT/RIGHT step the row (`j.c(3/4)`, wrapping).
    pub title_window: u32,
    /// `be` — the slot screen (SKT `ca`): `+0x28 a:B` mode (1 이어하기 · 10 새로하기 ·
    /// 4 삭제 · 11 class pick). Modes 1/4/10: arrow box `SLOT n` at `(ax.j, ax.k − 10)`;
    /// mode 11: the class name's box at `(ax.j, ax.k − 25)`. LEFT/RIGHT cycle, OK picks.
    pub slot_window: u32,
    /// `r` — OPTION (SKT `l`, `<init>(Lj;Z)V`): five rows at `ax.k − 36 + 18·i`, label
    /// at `ax.j − 45`, value at `ax.j + 24`; LEFT/RIGHT change the focused row's value.
    pub option_window: u32,
    /// `ao` — QUEST (SKT `y`): one accepted quest at a time; `j.i` = how many. LEFT/RIGHT
    /// step the quest (arrow span at `x + 7 .. + 33`, `y + 22`), UP/DOWN toggle the text
    /// between description and objective (the little triangle at `x + 60, y + 103`).
    pub quest_window: u32,
    /// `cg` — CARD (SKT `at`, a network-window subclass — SKT `r` = KTF `y`): a 5 x 3 grid of
    /// 15 px cells at `x + 20 + col·17 − 1, y + 15 + row·17 − 1`, `h = row·5 + col`, `j.i` = 15.
    pub card_window: u32,
    /// `cj` — the skill DETAIL window (SKT `av`): OK closes it.
    pub detail_window: u32,
    /// ★★ 2026-10-14 `cd` — the ITEM detail window (the inventory popup's 상세보기): the record `cd.a:Lch;` at absolute
    /// `AccessoryPatch::DETAIL_RECORD_OFF`, painted at the panel origin — its text box at `(px + 21, py + 15, 96, 87)`
    /// (`j.a(Graphics,II)V` @0x15ea60). OK closes it; LEFT / RIGHT turn an equipment record's option page.
    pub item_detail_window: u32,
    /// `ba` — the message box (SKT `bx`, e.g. 보스맵에서는 저장할 수 없습니다): OK closes it.
    pub message_window: u32,
    /// `bz` — the beast sub-list (SKT `cm`): rows of pitch 17 from `y + 29`.
    pub beast_list_window: u32,
    /// `bj` — the HELP pager (SKT `ai`, SYSTEM → 도움말): LEFT/RIGHT page, UP/DOWN scroll.
    pub help_window: u32,
}

// Some of these constants only document the measurement (`*_H` of a cell whose touch pad
// is the pitch, the parent link); they are kept for the next reader, not for the compiler.
#[allow(dead_code)]
impl Hero2Java {
    /// `g.t:B` / `g.u:B` — an entity's MOTION STATE and FACING, absolute in the character's
    /// field block. The layout is parent-first: `ad` (the root: `a:S +0` / `b:S +4`, the
    /// position the minimap reads) has `fields_size=0x34`, and `g` follows it declaring
    /// `t` at its own +0x8 and `u` at +0xc → absolute 0x3c / 0x40. (`dump_hero2_java.py`
    /// gives the sizes; 0x34 + `g`'s 0x28 = 0x5c is where `a`'s block — the level the
    /// gauges read — begins, which is the cross-check.) ⛔ The first cut used the DECLARED
    /// offsets as absolute and read `ad.c`/`ad.d` (8/9) for a standing character; the
    /// probe's `[h2-walk]` gauge showing `t=8` was the tell.
    ///
    /// `t` is `1` standing, `2` walking, `3` an ACTION (attack or skill: `ag.c(B)V`
    /// @0x110a50 — the OK and skill-key path — starts one with `ag.d(3, …)` when the
    /// character stands or walks), `6` DYING and `7` DEAD. ⛔ 2026-09-25 correction: 6/7
    /// were written down here as "the attack states"; they are the death. `a.b(I)V`
    /// @0x14f978 (HP += n, clamped) calls `this.b(6)` the moment HP reads 0, and
    /// `a.w()V` @0x14e2e4 turns 6 into 7 nine frames later while it posts the GAME OVER
    /// event (HERO2_GUEST_RE.md §8.18) — which is also why `n.a(II)V` refuses items and
    /// `n.i()V` skips the pickup in exactly those two. Set by `g.b(B)V` @0x15b158
    /// (through `ag.d(BB)V` among others), reset to `1` by `g.r()V` @0x15b040 — which
    /// `a.r()V` runs on every stage load (`ag.m()V` / `ag.o()V`), so a load ends a walk;
    /// a script or a window does not.
    pub const CHAR_WALK_STATE_OFF: u32 = 0x3c;
    pub const CHAR_FACING_OFF: u32 = 0x40;
    /// `g.w:B` — the motion FRAME counter (absolute +0x48: `ad`'s 0x34 + `g`'s own +0x14).
    /// `g.b(B)V` sets it to −1 with every state change and `a.g()V` @0x14dd84 adds one per
    /// field frame. ★ 부활의 구 (2026-09-25): in state 6 it is the death clock — `a.w()V`
    /// posts GAME OVER once it passes 8, so a revive puts it back to −1 exactly as the
    /// game's own `g.b(1)` would.
    pub const CHAR_MOTION_FRAME_OFF: u32 = 0x48;
    /// `a.o:[B` / `a.i:B` — the ACTION QUEUE (the kinds of the running chain: combo stages, then a skill) and the index
    /// of the one running, absolute (the blocks' `queue_off` / `queue_at_off`). While `t` is an action the kind is
    /// `a.o[a.i]`; ⛔ for the frame after a cast `a.i` is still −1 (the next update's action driver fills it — §8.57).
    pub const CHAR_ACTION_QUEUE_OFF: u32 = 0xac;
    pub const CHAR_ACTION_AT_OFF: u32 = 0xb0;
    /// `a.o:I` — the chain's stage OFFSET (absolute; declared +0x98): `a.d(B)Z` @0x14f154 admits a new entry only while
    /// `a.i + 1 < a.h + a.o:I`, `a.e()Z` lets a basic attack follow only a stage below `a.i[class] + a.o:I`. `a.i()V` @0x14f358
    /// — the chain's own reset — puts it to 0 with every `a.o[]` ← −1 and `a.i` ← −1 (★ 2026-10-26 the 부활의 구's stand-up).
    pub const CHAR_CHAIN_BASE_OFF: u32 = 0xf4;
    /// `bs.b:B` @0x19419c — the game's SOUND LEVEL (the 환경설정 window's first row, the bar): `br.a(Clip,Z)V` @0x138948
    /// plays nothing at `≤ 0` and otherwise sets the clip volume through `br.a(I)V` @0x1389e4 = `clamp(0, 10) × 100 / 10`.
    /// ★ 2026-10-09 the host's voices follow it (`voice::voice_gain`).
    pub const SOUND_LEVEL_SLOT: u32 = 0x0019_419c;
    /// Instance offsets shared by every `j` window (parent-first layout, `j` has no
    /// ancestors, so these are absolute).
    pub const WIN_PARENT_OFF: u32 = 0x0;
    pub const WIN_CHILD_OFF: u32 = 0x4;
    pub const WIN_CURSOR_OFF: u32 = 0x8;
    pub const WIN_ROWS_OFF: u32 = 0xc;
    /// `j.e:Z` — "repaint me": `j.c()` paints a window only while this is set.
    pub const WIN_DIRTY_OFF: u32 = 0x20;
    /// ★★ 2026-10-14 The item detail window's text box from the panel origin: `j.a(Graphics,II)V`'s
    /// `fillRect(X + 21, Y + 15, 96, 87)` (`cd.a(II)V` paints at the origin itself — probe: 28 · 51).
    pub const ITEM_DETAIL_BOX: (u32, u32, u32, u32) = (21, 15, 96, 87);
    /// `d` (popup) own fields, absolute (= `j`'s 0x28 + declared).
    pub const POPUP_KIND_OFF: u32 = 0x28;
    pub const POPUP_LABELS_OFF: u32 = 0x2c;
    pub const POPUP_HEIGHT_OFF: u32 = 0x30;
    /// ★ `aq` (the 개수 box) own fields, absolute: `a:I` a unit price — drawn as
    /// `price x n` under the number when it is > 0, so the host passes 0 — and `a:B` the
    /// ceiling the RIGHT/UP keys clamp to. The number is `j.h`, the inherited cursor.
    /// ★ 2026-10-16 보스 레이드: `ag.h:B` @0x193b4c — the map number the game is in or loading (`cf.n()V` builds
    /// `/m/m/<n>.m.gz` from it; `bf.<init>(B)V` copies it into the stage's `bf.b`). A sign-extended word like every static byte.
    pub const MAP_SLOT: u32 = 0x0019_3b4c;
    pub const QTY_PRICE_OFF: u32 = 0x28;
    pub const QTY_MAX_OFF: u32 = 0x2c;
    /// The most `aq` can carry: `j.h` and `a:B` are signed bytes, and OK hands the value
    /// to `parent.a(BB)V` as a byte too.
    pub const QTY_MAX: i32 = 127;
    /// Where `aq` paints, from the game's own screen statics (`j.a(Graphics,II)I`
    /// @0x15fe50: a `117 x h` box at `((ax.g - 117) >> 1, (ax.i - h) >> 1)`), and the
    /// value field inside it (`aq.a(II)V` @0x11def8: `42 x 15` at `(ax.j - 21, top + 31)`).
    /// The touch router's only geometry for this window — see [`H2Tap`].
    pub const QTY_BOX_W: u32 = 117;
    pub const QTY_BOX_H: u32 = 0x38;
    pub const QTY_FIELD_DX: u32 = 21;
    pub const QTY_FIELD_DY: u32 = 31;
    pub const QTY_FIELD_W: u32 = 42;
    pub const QTY_FIELD_H: u32 = 15;
    /// ★ `ba` (the game's message box) own field, absolute (`j`'s 0x28 + declared +0x4):
    /// `a:I`, the box HEIGHT its constructor computed from the wrapped line count `b`
    /// (`ba.<init>(Lj;[B)V` @0x121404: `10·b + 12 + 4·(b − 1)`). Its painter `ba.a(II)V`
    /// @0x1215dc hands exactly that to `j.a(Graphics,II)I` — a `117 × a` box at
    /// `((ax.g − 117) >> 1, (ax.i − a) >> 1)`, the `aq` / 예·아니오 family.
    pub const MSG_BOX_W: u32 = 117;
    pub const MSG_BOX_HEIGHT_OFF: u32 = 0x2c;
    /// `s` (skill window) own fields, absolute: `a:Z` = the cursor is on the tab header,
    /// `b:Z` = which tab (true PASSIVE / false ACTIVE), `d:B` = the column split.
    pub const SKILL_HEADER_OFF: u32 = 0x28;
    pub const SKILL_TAB_OFF: u32 = 0x2c;
    pub const SKILL_SPLIT_OFF: u32 = 0x30;
    /// The game's menu PANEL: 120 x 117 at the origin `bw.a/b` (SKT `g.a(Graphics,int,int,..)`
    /// `setClip(x, y, 120, 117)`), rows of the field menu at `y + 12 + 12·i`, 10 tall,
    /// `x + 2 .. x + 118` (HERO2_GUEST_RE.md §5.2.1).
    pub const PANEL_W: u32 = 120;
    pub const PANEL_H: u32 = 117;
    /// `bq` (inventory) own fields, absolute (no parent): the item Vector, three bytes.
    pub const INV_VECTOR_OFF: u32 = 0x0;
    pub const INV_B_OFF: u32 = 0x4;
    pub const INV_HEAL_OFF: u32 = 0x8;
    pub const INV_C_OFF: u32 = 0xc;
    /// ★★ The inventory's capacity once the host has raised it (#13, 2026-09-20/21).
    ///
    /// 127 is not a choice but a CEILING: every index into a bag is a signed byte
    /// (`bq.a(I)B` narrows with `lsls #24 / asrs #24`, `bq.a(BI)V` · `bq.a(B)Lch;` · the
    /// cursor `j.h` are all `B`), so 128 would be −128. Measured headless (2026-09-21):
    /// with `b = 127` the game's own give path (`ch.a(BBZZ)` + `bq.a(Lch;B)I`) placed
    /// records 23..62 without refusing, the window kept drawing its first 28, and the
    /// save (`n1` 1,352 → 3,502 B) came back with all 62 records byte-identical.
    pub const BAG_CAPACITY: i8 = 127;
    /// What the game's own inventory window draws: `q.<init>` = `super(parent, 28, 4, 7)`,
    /// 7 × 4 hard-coded. A PAGE of the 127-slot bag is one window's worth.
    pub const BAG_PAGE: usize = 28;
    /// `ceil(127 / 28)` = 5 pages — the user's 「가방 5개」. The fifth holds 15, and since 2026-10-02 SHOWS 15
    /// (`hero2java::h2_page_cells` — its window's `j.i` is 15 and the UI paints the panel over the other 13).
    pub const BAG_PAGES: usize = 5;
    /// `java.util.Vector` as the runtime lays it out: `elementData:[Object` then `elementCount:I`.
    pub const VECTOR_DATA_OFF: u32 = 0x0;
    pub const VECTOR_COUNT_OFF: u32 = 0x4;
    /// ★ #20/#21 (2026-09-24): the stage's GROUND list, `bf.b:Ljava/util/Vector;` (+0x50 of
    /// the `ag.a:Lbf;` object). Every drop the kill routine makes lands here as a `byte[6]`:
    /// an item from `bf.a(BBBB)V` @0x12d37c is `{x, y, type, id, 1, 120}` and gold from
    /// `bf.a(BBS)V` @0x12d428 is `{x, y, -1, amount / 100, amount % 100, 120}` — the last
    /// byte a lifetime in frames. Read by the probe's drop census (`--h2-kill`) only.
    pub const STAGE_GROUND_OFF: u32 = 0x50;
    pub const GROUND_GOLD: i8 = -1;
    /// ★ 2026-09-26 (보스 처치 회복): the stage's ENTITY LIST — `bf.a:Lad;` (+0x24 of the stage)
    /// is its head and `ad.b:Lad;` (+0x2c of every entity, `ad` being the root class) the next.
    /// It is the list the stage update `bf.a()V` @0x129698 walks every frame (0x12975c head,
    /// 0x1297d2 next), so a monster on it is a monster the game is running.
    pub const STAGE_ENTITY_HEAD_OFF: u32 = 0x24;
    pub const ENTITY_NEXT_OFF: u32 = 0x2c;
    /// ★ 2026-10-15 (the teleport gate): the live map's NPCs — `bf.a:[Lab;` (+0x44 of the stage), one `ab` per
    /// placement of the map's NPC section in file order, built by the map loader (`new ab(x<<4, y<<4, rec, slot)`
    /// @0x12ab92). `ab`'s own block starts past `ad` 0x34 + `g` 0x28: `ab.a:B` = the placement's `npc/npc` record
    /// (+0x5c) and `ab.e:Z` = shown (+0x60) — 1 from the constructor, 0 from the map's load table (`0b n`, run once
    /// per map load) or a script (`0a n 1`); while 0 the NPC is not drawn and does not stand in the way.
    pub const STAGE_NPCS_OFF: u32 = 0x44;
    pub const NPC_REC_OFF: u32 = 0x5c;
    pub const NPC_SHOWN_OFF: u32 = 0x60;
    /// The game's own BOSS is `instanceof ah`: 사자후's damage arm in `bl.a(BILbd;)I` (0x133a1a)
    /// kills any monster outright EXCEPT that class — its `instanceof` reads the class table at
    /// `*(sl + 0x20) + 0xc8`, which is `ah`'s `JavaClass` (the table's `bd` entry at +0x280 is
    /// `bd`'s, so the base is `bd.jc − 0x280` and +0xc8 lands on `ah.jc` exactly). `ah` is
    /// abstract; these are its four concrete classes' descriptors: `al` · `ca` · `ci` · `m`.
    /// Measured: 해저터널 3구역's boss 「소문의 검사」 (map 102) is a `ca`; every field monster
    /// seen so far (camp · 해저터널) is a plain `bd`.
    pub const BOSS_CLASS_DESCS: [u32; 4] = [0x0019_23e8, 0x0019_29ac, 0x0019_2acc, 0x0019_3090];
    /// ★ 난이도 (2026-09-29): a monster's three stat words, ABSOLUTE. `bd`'s own block starts
    /// past its parents (`ad` 0x34 + `g` 0x28 = 0x5c; `dump_hero2_java.py --class bd` lists
    /// the fields at +0x14 / +0x18 / +0x1c): `bd.c:S` max HP, `bd.d:S` HP (the poison patch's
    /// `mob_hp_off`), `bd.e:S` attack. `bd.<init>(SSBB)V` @0x125cb4 fills them from the enemy
    /// template — `bd.c ← cq.a:S + 20·k`, `bd.e ← cq.b:S + 3·k`, `bd.d ← bd.c` — and nothing
    /// writes `bd.c` / `bd.e` after that; every monster hit hands `bd.e` to the character's
    /// hit routine `a.a(Lbd;SB)V` as the damage (`bd.d()V` 0x126e0c, `al.d`, `al.w`, `at.b`,
    /// `m.d`). Each is a sign-extended WORD (the putfield helper stores `lsls/asrs` 16 → 32).
    pub const MONSTER_MAX_HP_OFF: u32 = 0x70;
    pub const MONSTER_HP_OFF: u32 = 0x74;
    pub const MONSTER_ATK_OFF: u32 = 0x78;
    /// `bd.a:Lcq;` (+0x4 → 0x60): the monster's enemy template, and on it `cq.b:S` (+0x28 —
    /// `cq` has no parent), the attack the boss class `ca` reads at hit time instead of its
    /// own `bd.e` (`ca.d()V` 0x13d33a; the only reader of `cq.b` besides the constructor).
    pub const MONSTER_TEMPLATE_OFF: u32 = 0x60;
    pub const TEMPLATE_ATK_OFF: u32 = 0x28;
    /// `ca` — the second of [`Self::BOSS_CLASS_DESCS`] (`dump_hero2_java.py --class ca`).
    pub const CA_CLASS_DESC: u32 = 0x0019_29ac;
    /// ★ 2026-10-21: `m` — the fourth of [`Self::BOSS_CLASS_DESCS`]: 오딘's two hands (보복 · 심판의 손, enemies 100 · 101) and
    /// 고대수호물's two guardians (98 · 99) — the heads are `al` (HERO2_GUEST_RE.md §8.51; `m.d()V` drains the character into
    /// `al.a`). On the raid's 오딘 stage the only `m` are the hands.
    pub const HAND_CLASS_DESC: u32 = 0x0019_3090;
    /// `ch` (item record, SKT `cp`): `g:B` type (= `t/itmtp.tdf` index — 24 포션 · 25 하이포션 ·
    /// 26 포도주 · 27 치료제 are the recovery kinds), `h:B` id within the type, `c:I` count,
    /// `d/e/f:I`, `j:[B` name (EUC-KR), `k:[B` description.
    pub const ITEM_TYPE_OFF: u32 = 0x0;
    pub const ITEM_ID_OFF: u32 = 0x4;
    pub const ITEM_COUNT_OFF: u32 = 0x8;
    pub const ITEM_D_OFF: u32 = 0xc;
    pub const ITEM_E_OFF: u32 = 0x10;
    pub const ITEM_F_OFF: u32 = 0x14;
    pub const ITEM_NAME_OFF: u32 = 0x18;
    pub const ITEM_DESC_OFF: u32 = 0x1c;
    /// ★ 2026-09-26: the EQUIPPED flag — `aa.f:B`, absolute +0x34 (`ch`'s fields_size 0x20 +
    /// `aa`'s own +0x14). The slot of `a.a:[Laa;` (the character's seven) the record is
    /// worn in, `−1` when it is not; a sign-extended WORD like every `B` field here
    /// (`a.e(B)Z` @0x150efc stores `−1` with the word putfield 0x168004).
    ///
    /// It is the ONLY place the equip state lives: the save has no equipment block —
    /// `a.n()V` @0x150ce0 rebuilds `a.a` on every load from the inventory's records whose
    /// flag reads `>= 0` — so a worn record lives in `ag.a` like any other, and a move that
    /// takes it out of `ag.a` takes it off the character at the next load while it keeps
    /// its flag. The game's own predicate is `bq.a(Lch;)Z` @0x1380dc, `rec instanceof aa &&
    /// rec.f >= 0`: what 「장착중인 아이템은 버릴수 없습니다.」 (`q.a(BB)V` @0x1469e2, gm[55]) and
    /// 「장착중인 장비는 판매할 수 없습니다.」 (`cl.a(II)Z` @0x1429d8) refuse by.
    pub const ITEM_EQUIP_SLOT_OFF: u32 = 0x34;
    /// The classes that CARRY that flag: `aa` and its three subclasses — `bu` (armour, whose
    /// `bu.o` is the 중갑 byte, §8.1), `bo` (a weapon, `bu`'s child) and `z` (the rolled-stat
    /// accessory). The whole `aa` subtree, by `dump_hero2_java.py --json` ancestry; nothing
    /// else declares or inherits `aa.f`.
    pub const EQUIP_RECORD_CLASS_DESCS: [u32; 4] = [0x0019_225c, 0x0019_28d4, 0x0019_27fc, 0x0019_2d9c];
    /// ★ 2026-09-29 (상태 HUD 초상): the character's EQUIPMENT, `a.a:[Laa;` — declared +0xd0
    /// of `a`, absolute +0x12c (`ad` 0x34 + `g` 0x28 + 0xd0), seven slots (§8.25).
    pub const CHAR_EQUIP_OFF: u32 = 0x12c;
    /// Its HELMET slot. `a.c()Lbu;` @0x150b6c is `(bu) a.a[1]`, and it is what the part
    /// loader `cf.a(I)V` @0x153d18 asks for the HEAD (its `switch` case 0): the head sprite
    /// is `hero/0/<helmet's sprite>`, or `hero/0/100` when the slot is empty.
    pub const EQUIP_HELMET_SLOT: u32 = 1;
    /// `bu` — the armour record a helmet is (also in [`Self::EQUIP_RECORD_CLASS_DESCS`]).
    pub const ARMOUR_CLASS_DESC: u32 = 0x0019_28d4;
    /// `bu.l:B`, absolute +0x40 (`ch` 0x20 + `aa` 0x1c + 4) — body[16], the part SPRITE the
    /// record is drawn with (`bu.a([BIZ)I` @0x139fc4 reads it second, after `bu.k` =
    /// body[15]). For the 28 helmets it runs 0..=16 (`HERO2_ITEM_DB.md`), one per
    /// `hero/0/00`..`16`. A sign-extended word, like every `B` field.
    pub const ARMOUR_SPRITE_OFF: u32 = 0x40;
    /// ★ 2026-10-01: the equipment's seven slots (`a.a:[Laa;`, [`Self::CHAR_EQUIP_OFF`]) — 0 장갑 ·
    /// 1 투구 · 2 반지·목걸이 · 3 무기 · 4 갑옷 · 5 보조손 (방패 / the rogue's second dagger) · 6 신발.
    pub const EQUIP_SLOTS: usize = 7;
    pub const EQUIP_WEAPON_SLOT: usize = 3;
    pub const EQUIP_OFFHAND_SLOT: usize = 5;
    /// `bo` — a weapon record (the rogue's second dagger in slot 5 is one too).
    pub const WEAPON_CLASS_DESC: u32 = 0x0019_27fc;
    /// `bo.i:B`, absolute +0x58 (`ch` 0x20 + `aa` 0x1c + `bu` 0x1c) — the weapon's trait 1..=6
    /// (`t/itmatt.tdf`), the sparkle's colour; `cc.b(B)V` @0x13e754 reads the second dagger's.
    pub const WEAPON_TRAIT_OFF: u32 = 0x58;
    /// The head with NO helmet: `cf.a(I)V` loads `hero/0/100` (`movs r0,#0x64` @0x153d66).
    pub const HEAD_BARE_SPRITE: u8 = 100;
    /// The highest helmet head sprite (`hero/0/16`).
    pub const HEAD_SPRITE_MAX: u8 = 16;
    /// `bd` — the monster. Every field monster seen is a plain `bd`; the bosses are `ah`'s
    /// four concrete classes ([`Self::BOSS_CLASS_DESCS`]). The same five `CombatPatch::
    /// monster_descs` names, for the minimap's monster squares (`h2_monster_marks`).
    pub const MONSTER_CLASS_DESC: u32 = 0x0019_2670;
    /// `cf.a:[Ljava/lang/Object;` @0x19467c — the parts table `cf.a(I)V` fills: `[part]` =
    /// the sheet's every frame (`am.a()[Image`), `[part + PARTS_IMAGES_BASE]` = the frames
    /// the class's sheet lists, by frame index (`Image[]`). A reload makes NEW arrays, so
    /// the handle at `[part + 8]` changing is the proof that one happened (probe
    /// `--h2-avatar`).
    pub const PARTS_TABLE_SLOT: u32 = 0x0019_467c;
    pub const PARTS_IMAGES_BASE: u32 = 8;
    /// ★ 의상 아바타 A3: what `n.a(BBBII)V` @0x162fa8 hands the sprite drawer
    /// `n.a([BIBB[Ljava/lang/Object;ZBBII)B` — the class's animation blob `cf.j:[B` (the
    /// `hero/hero<class>` bytes: per sequence `[frames]` then per frame four directions of
    /// `[count][count × (p|0x80, f, dx, dy)]`) and the sequence offsets into it `cf.a:[I`.
    /// With [`Self::PARTS_TABLE_SLOT`] they are everything the host needs to draw the
    /// character exactly as the field does (the equipment window's preview).
    pub const CLASS_ANIM_SLOT: u32 = 0x0019_468c;
    pub const SEQ_OFFSETS_SLOT: u32 = 0x0019_469c;
    /// ★★ 2026-10-15 (user: *"현재 장착된 야수의 스킬만 초기화되는거야? 장착되지 않은 야수의 스킬도 초기화 시켜야돼"*):
    /// `ag.a:Ljava/util/Vector;` @0x193b6c — EVERY beast the character owns. The beast shop `ar.a(II)Z` @0x11e218 pays gold
    /// and appends `new bn(false)` here; the character's `a.a(B)V` @0x14d9b0 equips one by index (`a.r:B` + `a.a:Lbn;`,
    /// the field [`Hero2Java::beast_off`] reads); the save `ag.c()[B` @0x113938 writes every element with `bn.a()[B`
    /// (`bn.m` · `bn.k` per slot) and the load `ag.b([B)V` @0x113c64 rebuilds the list. So a beast that is not equipped
    /// still holds the skill levels the character's points bought — the 망각의 구 skill reset walks this list, not only
    /// the equipped beast.
    pub const BEAST_LIST_SLOT: u32 = 0x0019_3b6c;
    /// ★★ 2026-10-16: the beast list window's singleton `bz.a:Lbz;` (`dump_hero2_java.py --class bz`) — the window whose
    /// 「버리시겠습니까?」 (popup kind 10) discards `ag.a[j.h]`.
    pub const BEAST_LIST_WINDOW_SLOT: u32 = 0x0019_427c;
    /// ★★ 2026-10-16 (user: *"악의로 스킬이 바뀌면 야수 스킬창에서도 이름·아이콘 변경"*): `bw.b:Li;` @0x19421c — the
    /// `t/bsk.tdf` table (`i`, rows at [`Self::TABLE_ROWS_OFF`]): beast skill `kind`'s name is row `2·kind`, its description
    /// `2·kind + 1`. The beast window's row painter `af.e(G;III)V` @0x1104c0, the detail window `cj.<init>(Lj;BZ)V` and the
    /// bottom HUD read it with `i.a(I)[B`; `bw.a()V` loads it, `bw.a(Z)V` nulls it — every menu has a fresh table.
    pub const BEAST_TEXT_TABLE_SLOT: u32 = 0x0019_421c;
    /// `cf.a:Lcf;` @0x1945ec — the parts singleton (the [`JavaCall::PartsReload`] receiver) — and its `cf.i:[Image`
    /// (absolute +0xc: `cf` has no parent): EVERY frame of `img/bico` (`cf.r()V` @0x157170: `new am(…).a()`; `cf.i()V`
    /// nulls it), one 12 × 12 beast skill icon per kind, drawn by `n.c(BII)V` @0x163788 as `cf.i[kind]` — the beast window,
    /// the detail window and the bottom HUD.
    pub const PARTS_SINGLETON_SLOT: u32 = 0x0019_45ec;
    pub const BEAST_ICONS_OFF: u32 = 0xc;
    /// ★ A3: `cf.q:Lorg/kwis/msp/lcdui/Image;` @0x1948ec — the item-type icon strip
    /// (`img/glb` frame 17, 12 × 408 = 34 cells). `n.a(IIB)V` @0x16388c is its one reader:
    /// it clips to `(x, y, 12, 12)` and draws the strip at `y − 12·cell`, `cell = type < 8 ?
    /// type : type − 1` (HERO2_GUEST_RE.md §2.5). The costume window takes two cells from it:
    /// the 재료가방 (type 29) for a bag, the 갑옷 (type 6) for a costume. ★ A3c (the four
    /// avatar cells, `hero2java::h2_av_slot_icon`): 투구 (type 5) · 갑옷 (6) · 신발 (type 9 →
    /// cell 8) · the class's weapon — 버서커 양손대검 (0) · 나이트 도검 (2) · 로그 단검 (3)
    /// (`t/itmtp.tdf` 0..10).
    pub const ITEM_ICONS_SLOT: u32 = 0x0019_48ec;
    pub const ITEM_ICON_SIDE: u32 = 12;
    /// The shipped strip's cells — the fewest a strip may have. ★ 2026-10-16 the asset appends an
    /// item's own cell after them ([`ItemIconPatch`] — 엘릭서 = cell 34), so the strip is 34 OR MORE.
    pub const ITEM_ICON_CELLS: u32 = 34;
    /// The most cells the host reads from the strip (a cap on the read, not a format limit).
    pub const ITEM_ICON_CELLS_MAX: u32 = 64;
    pub const ITEM_ICON_BAG: u32 = 28;
    pub const ITEM_ICON_ARMOUR: u32 = 6;
    pub const ITEM_ICON_HELMET: u32 = 5;
    pub const ITEM_ICON_SHOES: u32 = 8;
    /// Per class (0 버서커 · 1 나이트 · 2 로그): the icon of the weapon it fights with.
    pub const ITEM_ICON_WEAPONS: [u32; 3] = [0, 2, 3];
    /// ★ A4 (2026-10-01): 방패 (type 10 → cell 9) — the knight's off-hand avatar cell. The
    /// rogue's shows its second dagger (단검, cell 3).
    pub const ITEM_ICON_SHIELD: u32 = 9;
    pub const ITEM_TYPE_RECOVERY: std::ops::RangeInclusive<i8> = 24..=27;
    /// `i` (text table): `a:[Object` = the `byte[][]` of rows, `a:S` = the count.
    pub const TABLE_ROWS_OFF: u32 = 0x0;
    /// ★ 2026-09-24 (획득 안내 · 오토루팅): the HUD singleton `n.a:Ln;` — the pickup
    /// notice lives on it: `n.o:I` (+0x40, set to 16 by `n.a([BII)V` @0x1645e4), `n.h:Z`
    /// (+0x44, "a notice was still up") and `n.b:Image` (+0x48, the notice pre-rendered
    /// in the game's font, 96×16). ⛔ MEASURED, not derived: the declared +0x1c/+0x20/
    /// +0x24 plus a 0xc parent block (the first reading) gave +0x28/+0x30, which hold a
    /// constant 1 and 0. Dumping the object across five probe pickups (slot 2 forest,
    /// `--h2-kill 700:3 --h2-autoloot 1`) shows +0x40 going 0 → 16, +0x44 0 → 1 on the
    /// second pickup, and +0x48 a fresh handle on every one — i.e. the parent is 0x24.
    /// ★ 2026-09-24 (환경설정 rows): the game font's statics, filled by `ac.a()V`
    /// @0x10ed4c from `ftd/table` · `ftd/kor` · `ftd/eng5` · `ftd/etc` — see
    /// `hero2java::H2Font`, which replicates the renderer they feed.
    pub const FONT_TABLE_SLOT: u32 = 0x0019_393c;
    pub const FONT_KOR_SLOT: u32 = 0x0019_396c;
    pub const FONT_ENG_SLOT: u32 = 0x0019_397c;
    pub const FONT_MAPS_SLOT: u32 = 0x0019_398c;
    pub const FONT_CHO_BUL_SLOT: u32 = 0x0019_399c;
    pub const FONT_JUNG_BUL_SLOT: u32 = 0x0019_39ac;
    pub const FONT_JONG_BUL_SLOT: u32 = 0x0019_39bc;
    pub const NOTICE_SLOT: u32 = 0x0019_4acc;
    pub const NOTICE_TIMER_OFF: u32 = 0x40;
    pub const NOTICE_IMAGE_OFF: u32 = 0x48;
    /// `cf.c:Li;` — the item-TYPE text table (`t/itmtp.tdf`): what a pickup notice says,
    /// and row 7 「빈공간이 없습니다.」, the bag-full notice (`ag.a()Z` @0x112600).
    pub const ITEM_TYPE_TABLE_SLOT: u32 = 0x0019_466c;
    /// ★ 부활의 구 (2026-09-25): the row `tools_re/make_hero2_asset.py` APPENDS to that
    /// table — 「부활의 구 사용」, the notice raised after a revive. The game reads the table
    /// by constant index only and never past 63 (`58 + set` is its largest), and it is the
    /// one text table that lives for the whole run (`/t/gm` is rebuilt per menu and nulled
    /// on close), which is why the field-time notice is here.
    pub const ITEM_TYPE_ROW_REVIVED: u32 = 64;
    /// `cf.a:Li;` — `/t/com`, the shell's text table: 환경설정's labels are rows 26..=30
    /// (사운드 배경음 효과음 게임속도 시선이동), ON/OFF 24/25, 게임속도 SLOW..FASTER 32..=35.
    pub const COM_TABLE_SLOT: u32 = 0x0019_464c;
    pub const ITEM_TYPE_ROW_BAG_FULL: u32 = 7;
    /// ★★ 2026-10-18: 「재료가방이 가득 참」 — the row `tools_re/make_hero2_asset.py` appends after the 75/76 gear hints,
    /// the 재료가방 version of row 7 ([`GuestBuild::bag_full_row_site`]). [`Self::MATERIAL_FULL_TEXT`] is its EUC-KR: the
    /// host reads the row and compares before it names it (an asset without it — older, or built without the skill gear,
    /// where the line sits at 75 — keeps row 7).
    pub const ITEM_TYPE_ROW_MATERIAL_FULL: u32 = 77;
    pub const MATERIAL_FULL_TEXT: &'static [u8] = &[0xc0, 0xe7, 0xb7, 0xe1, 0xb0, 0xa1, 0xb9, 0xe6, 0xc0, 0xcc, 0x20, 0xb0, 0xa1, 0xb5, 0xe6, 0x20, 0xc2, 0xfc];
    /// `movs r2,#row` — the instruction at [`GuestBuild::bag_full_row_site`] for a notice row.
    pub const fn bag_full_row_movs(row: u32) -> u16 {
        0x2200 | (row as u16 & 0xff)
    }
    /// The character's TILE, `ad.d:B` +0x10 / `ad.e:B` +0x14 — what `ag.a()Z` hands the
    /// stage to find the drop under the player (`ad.a()V` @0x10f230 keeps them = px >> 4).
    pub const CHAR_TILE_X_OFF: u32 = 0x10;
    pub const CHAR_TILE_Y_OFF: u32 = 0x14;
    /// A ground drop's type byte: `-1` gold (always taken), `34` a card (`ag.c`) —
    /// `bf.a(BB)[B` @0x12d8c4's routing, with the materials (`bq.a(B)Z`) to `ag.b` and
    /// everything else to the inventory.
    pub const DROP_TYPE_GOLD: i8 = -1;
    pub const DROP_TYPE_CARD: i8 = 34;
    /// `bq.a(BBI)Z` @0x13687c: a stack can take `n` more while `count + n <= 99`.
    pub const STACK_MAX: i32 = 99;
    /// `/t/gm` rows the popups are built from (`tools_re/dump_tdf.py`): the inventory
    /// popup's three (`사용하기 · 상세보기 · 버리기`) and the two register rows this host
    /// relabels (`1번에 등록` → `퀵슬롯등록`, `3번에 등록` → `퀵슬롯해제`).
    pub const GM_ROW_USE: u32 = 30;
    pub const GM_ROW_DETAIL: u32 = 31;
    pub const GM_ROW_DISCARD: u32 = 28;
    pub const GM_ROW_BIND: u32 = 34;
    pub const GM_ROW_UNBIND: u32 = 35;
    /// ★ #13 (2026-09-21) — the field menu's ACCOUNT row (was CARD; the asset's `gm.tdf`
    /// rows 11/19 now read ACCOUNT / 계정창고, `tools_re/make_hero2_asset.py`), the
    /// focused-row label that doubles as the ACCOUNT window's title, the 재료 가방 title
    /// row `q` shows for a non-usable bag, and the two rows the asset tool APPENDS for
    /// the move popups (nothing in the game reads past row 77).
    pub const MENU_ROW_ACCOUNT: u8 = 6;
    pub const GM_ROW_ACCOUNT_FOCUSED: u32 = 19;
    pub const GM_ROW_MATERIAL_BAG_TITLE: u32 = 57;
    pub const GM_ROW_TO_ACCOUNT: u32 = 78;
    pub const GM_ROW_TO_BAG: u32 = 79;
    /// ★ #22: 스탯초기화 / 스킬초기화 — the 망각의 구's own popup rows, appended to
    /// `t/gm.tdf` by `tools_re/make_hero2_asset.py` the same way 78/79 were. ⛔ The
    /// generic 사용하기 (row 30) is deliberately NOT used: this row does something that
    /// cannot be undone, so it has to say what it does.
    pub const GM_ROW_RESET_STAT: u32 = 80;
    pub const GM_ROW_RESET_SKILL: u32 = 81;
    /// ★ 2026-09-26 (user: "장착중인 장비는 계정창고에 들어가면 안돼 … 안내팝업은 항상 기존 게임
    /// 팝업으로 만들어줘. 내가 직접그린 안내토스트문구가 뜨는게아니라"): the NOTICE rows the asset
    /// tool appends after 81 — every notice the warehouse and the 망각의 구 give is one of
    /// these (or the game's own 「빈공간이 없습니다.」, [`Self::ITEM_TYPE_ROW_BAG_FULL`]),
    /// handed to the game's own message box ([`JavaCall::MessageBoxNew`]) instead of a host
    /// toast. Worded after the game's own: 82 after `shop.tdf[25]` 「장착중인 장비는 판매할 수
    /// 없습니다.」, 83/84 after `shop.tdf[20/21]` 「구입/판매하였습니다.」.
    pub const GM_ROW_NOTE_EQUIPPED: u32 = 82;
    pub const GM_ROW_NOTE_TO_ACCOUNT: u32 = 83;
    pub const GM_ROW_NOTE_TO_BAG: u32 = 84;
    pub const GM_ROW_NOTE_STAT_RESET: u32 = 85;
    pub const GM_ROW_NOTE_SKILL_RESET: u32 = 86;
    pub const GM_ROW_NOTE_NOTHING_TO_RESET: u32 = 87;
    pub const GM_ROW_NOTE_RESET_FAILED: u32 = 88;
    pub const GM_ROW_NOTE_NO_ACCOUNT: u32 = 89;
    /// ★★ 2026-10-16 엘릭서: its own 사용하기 row (the text of row 30, a row of its own so the popup reader can tell the
    /// host's row from the game's) and the notice when there is nothing to refill.
    pub const GM_ROW_USE_ELIXIR: u32 = 90;
    pub const GM_ROW_NOTE_VITALS_FULL: u32 = 91;
    /// ★★ 2026-10-20 상점주인의 혼: its popup's 사용하기 is the SAME host row 90 — the popup reader tells the two orbs
    /// apart by the record under the cursor (`h2_oblivion_of`), never by the row's text.
    pub const GM_ROW_USE_SHOP: u32 = Self::GM_ROW_USE_ELIXIR;
    /// Popup kind of the inventory's RECOVERY-item context menu (SKT `bf.a(int,int)`:
    /// `ci.b(g)` → `a(6, 3, gm[30,31,28])`). Kind 7 is 귀환서, kind 3 the rest.
    pub const POPUP_KIND_ITEM_USE: u8 = 6;
    /// Popup kind of the 귀환서 context menu (`bf.a(int,int)`: `g == 28` → `a(7, 3,
    /// gm[30,31,28])`) — the SAME three rows as kind 6. What differs is the popup's answer
    /// (`bf.a(byte,byte)`): kind 6 row 0 is `b.d(h)` (use the record under the cursor),
    /// kind 7 row 0 first checks the map (`u.a(ah).b >= ah.a` → a message box, gm[77]) and
    /// then asks with a kind-11 box; its 예 consumes one (`ci.a(h, 1)`), closes the
    /// inventory (`parent.b(0)`) and warps (`u.a(7, 2)`). 2026-09-18, ⑧.
    pub const POPUP_KIND_ITEM_RETURN: u8 = 7;
    /// The two other TWO-BUTTON popups. They share kind 2's constructor arm and painter
    /// (`ba.<init>` / `ba.a(int,int)`: `case 2: case 10: case 11:`), so kind 2's button
    /// geometry is theirs too. 10 = 버리시겠습니까 (its cursor STARTS on 아니오, `h = 1`),
    /// 11 = 귀환하시겠습니까 (starts on 예).
    pub const POPUP_KIND_DISCARD_CONFIRM: u8 = 10;
    pub const POPUP_KIND_RETURN_CONFIRM: u8 = 11;
    /// The two usable item types the `0` key cannot reach (`bq.a()` re-derives its
    /// record from the cycle `bq.j = [24, 25, 26]` every time): 27 치료제 is a kind-6
    /// item whose `b.d(h)` arm cures (`cp.h == 0` → status 26, `== 1` → 27), 28 귀환서
    /// is kind 7. A plate holding one of these goes through the game's own inventory
    /// popup instead — `h2_use_step`.
    pub const ITEM_TYPE_CURE: i8 = 27;
    pub const ITEM_TYPE_RETURN: i8 = 28;
    /// 재료가방 — the record the inventory keeps in cell 0 as the door to `ag.b`
    /// (`q.a(II)Z` @0x146696: `type == 29` opens `new q(this, ag.b, false)`). The
    /// warehouse keeps one in ITS cell 0 as the door to its materials.
    pub const ITEM_TYPE_MATERIAL_BAG: i8 = 29;
    /// ★ #22: the 마법구 type the two 망각의 구 records were appended to, and their ids.
    /// There is no free item type — `t/itmtp.tdf` is item types at `[0..34]` and GRADE
    /// names from `[35]`, and `[7]` is a UI string (which is why `itm/07` does not
    /// exist) — so the orbs borrow one. 21 is the only type nothing else special-cases:
    /// not a material (so it stays in the main bag), not in the `0` key's recovery cycle,
    /// not a card, and not counted by any host plate. And the game cannot act on it from
    /// the bag at all: a 마법구's popup is kind 3 with `[상세보기 버리기]` and no 사용하기
    /// (probe-measured 2026-09-23), so the reset is entirely the host's.
    pub const ITEM_TYPE_ORB: i8 = 21;
    pub const ORB_ID_STAT: i8 = 3;
    pub const ORB_ID_SKILL: i8 = 4;
    /// ★ 2026-09-25: 부활의 구 — `itm/21[5]`, the same borrowed type for the same reason.
    /// The game cannot use it from the bag either; the host offers it when the character
    /// falls (`emulator::Revive`, HERO2_GUEST_RE.md §8.18).
    pub const ORB_ID_REVIVE: i8 = 5;
    /// ★★ 2026-10-16: 엘릭서 — `itm/21[7]` (6 is 대장장이의 보증서, the smith's), the same borrowed type; its popup gets a
    /// host 사용하기 row ([`Self::GM_ROW_USE_ELIXIR`]) and the host refills HP · SP (`h2_elixir_plan`) and consumes one.
    pub const ORB_ID_ELIXIR: i8 = 7;
    /// ★★ 2026-10-20 상점주인의 혼 — `itm/21[8]` (user: *"영구 아이템이야 … 사용하면 상점npc에게 대화한것처럼 아이템 구매와
    /// 판매를 할 수있는 휴대용 상점"*, 5,000,000 gold): the same borrowed type; its popup gets the host 사용하기 row
    /// ([`Self::GM_ROW_USE_SHOP`]) and the host opens the game's own shop (`emulator::AccountState::start_shop`): the stock
    /// static filled when `null` ([`JavaCall::ShopStockLoad`] → [`JavaWrite::ShopStock`]), the field menu closed, the shop
    /// EVENT posted ([`JavaCall::EventPost`] with [`Self::SHOP_EVENT`]). Nothing is consumed — a permanent item.
    pub const ORB_ID_SHOP: i8 = 8;
    /// `v.a:[Ljava/util/Vector;` @0x19454c — the shop stock the buy list `p.a()V` indexes (`[p.a × 5 + p.c]`, unchecked).
    pub const SHOP_STOCK_SLOT: u32 = 0x0019_454c;
    /// `(ag.d, ag.e)` of the shop event: `ag.g()V`'s jump table @0x16fc80 entry 4 → 0x1116ca reads `ag.e` — 0 the shop
    /// (0x11172a), 1 the alchemist, 2 the smith, 3 `bz`.
    pub const SHOP_EVENT: (i32, i32) = (4, 0);
    /// The five base abilities and the stride between them in the field block.
    pub const CHAR_ABILITIES: u32 = 5;
    pub const CHAR_ABILITY_STRIDE: u32 = 4;
    /// Length of `a.p:[B` — `a.b()V` @0x14d71c allocates `byte[15]` for every class, and
    /// it splits the same way for all three: `a.i[class]` combo stages then the class's
    /// own skills (4+11 · 3+12 · 5+10, `t/sk0` · `sk1` · `sk2`). A SANITY bound only —
    /// every walk reads the live array's length and this just refuses a stage count that
    /// could not leave room for a skill. See [`Self::char_combo_open_off`].
    pub const CHAR_SKILL_SLOTS: u32 = 15;
    /// ★ 2026-10-07 각성기: the character's level `a.a:B`, absolute (= `CharLayout::lv` +0x5c) — what the LV 70 unlock reads.
    pub const CHAR_LEVEL_OFF: u32 = 0x5c;
    /// The one skill a class is BORN with beyond `a.b()V`'s two (기본공격 1단 · the first class
    /// skill): `(class, a.p index)` = 나이트's 「중장비」, `sk1[7]` = `a.p[3 + 7]`. `bx.b()V`
    /// @0x13b39c ends `a.p[10] ← 1` (`movs r1,#0xa; movs r2,#1; bl bastore` @0x13b404); `bl.b()V`
    /// and `ay.b()V` write the five abilities and nothing else. Its cap is 1 (`bx`'s
    /// `i == 10 → 1`), so a knight always holds it at exactly 1 — the skill reset keeps it
    /// (`hero2java::h2_skill_born_level`) and `hero2java::h2_born_skill_repair` puts it back.
    ///
    /// ★ 2026-10-06: a LIST — `(class, a.p index, the slot was a skill the player could buy before)`. The
    /// rogue's 「심안」 ([`IlpungPatch::SIGHT_SLOT`], 독 연마 as shipped, which only a quest opened) is held at
    /// 1 from the game's start (user: *"1레벨 자동 획득"* · *"처음 겜 시작부터 볼 수있는거야"*) — and nothing in
    /// the game gives it, so the host opens it from −1 and lays down 1 (the knight's −1 is left alone, as
    /// before: `bx.b()V` gives every knight 중장비 at creation). Whatever 독 연마 held before this build comes
    /// back ONCE, every level (user: *"독연마의 이미 찍은 포인트 전부를 돌려줘야해"*) — once being the
    /// character record's receipt (`avatar_state::RECEIPT_SIGHT_REFUND`), not the byte's.
    pub const BORN_CLASS_SKILLS: [(u8, u32, bool); 2] = [(1, 10, false), (2, IlpungPatch::SIGHT_SLOT as u32, true)];
    /// The types the game routes to the 재료가방 on a drop or a grant — `bq.a(B)Z`
    /// @0x13722c: 23 · 30 · 31 · 32. A move back out of the warehouse follows the same rule.
    pub const ITEM_TYPES_MATERIAL: [i8; 4] = [23, 30, 31, 32];
    /// The field menu's INVENTORY row (`bw.b()` / SKT `ck.b()`: `case 1: new bf(this,
    /// u.a, true)` — STATUS · INVENTORY · EQUIPMENT · SKILL · BEAST · QUEST · CARD ·
    /// SYSTEM), of [`Self::MENU_ROWS`].
    pub const MENU_ROW_INVENTORY: u8 = 1;
    pub const MENU_ROWS: u8 = 8;
    /// `q` (inventory window, SKT `bf`) own field `a:Lbq;` — the bag it shows. Only the
    /// MAIN bag (`ag.a`) feeds the `0` key; the nested 재료 가방 (`ag.b`) opens the same
    /// window class over another bag and is refused by comparing this handle.
    pub const INV_WIN_BAG_OFF: u32 = 0x28;
    /// ★ 2026-10-09 the item PICKER `k` (desc `0x193048`, `j` + 0x10): the 7 × 4 window four screens open over a list
    /// of bag indices — `k.<init>(Lj;B[B[BZ)V` @0x160fe8 = `super(parent, kind, 28, 4, 7)`, then `k.i:[B` (+0x28) ← the
    /// list, `k.j:[B` (+0x2c) ← the title, `k.a:B` (+0x30) ← the kind, `k.a:Lbq;` (+0x34) ← `ag.a` or `ag.b`. Its painter
    /// `k.a(II)V` draws cell `i` = `k.a(I)Lch;(row·7 + col)` at `x + 7 + col·15, y + 23 + row·15` (14 × 14), the record =
    /// `bag.a(list[i])` while `i < list.length` and nothing after; OK hands the parent `list[cursor]`.
    pub const PICKER_CLASS_DESC: u32 = 0x0019_3048;
    pub const PICKER_LIST_OFF: u32 = 0x28;
    pub const PICKER_BAG_OFF: u32 = 0x34;
    /// The picker's first cell row, below its title (the inventory's [`Self::INV_CELL_Y0`] is 31 — it has the 소지금 bar).
    pub const PICKER_CELL_Y0: u32 = 23;
    /// ★ 2026-10-09 the ALCHEMIST's menu `aw` (desc `0x192574`): the root of state 8's window chain while the alchemist is
    /// up — `ag.g()V`'s event opens it as `ag.h(8)` · `aw.a()Law;` · `aw.a()V`, the singleton in `aw.a:Law;` @0x193d5c,
    /// its rows 장비 능력 부여 (`x`) and 아이템 조합 (`aj`, whose recipe list is a picker) hang off its `.b`.
    pub const ALCHEMIST_CLASS_DESC: u32 = 0x0019_2574;
    pub const ALCHEMIST_SLOT: u32 = 0x0019_3d5c;
    /// `aw.a:I` / `aw.b:I` — the origin `aw.c()V` hands its whole chain (`k.c(II)V(aw.a, aw.b)`): every window of the
    /// alchemist's paints its panel from here, as the field menu's do from `bw.a/b` ([`Self::menu_origin`]).
    pub const ALCHEMIST_ORIGIN: (u32, u32) = (0x0019_3d3c, 0x0019_3d4c);
    /// The state word while an NPC's service window is up (the alchemist, the blacksmith, the shop's own is 7).
    pub const NPC_WINDOW_STATE: u32 = 8;
    /// That popup's row counts: the game's three, ours with 퀵슬롯등록 / 해제 appended.
    pub const ITEM_POPUP_NATIVE_ROWS: u8 = 3;
    pub const ITEM_POPUP_OUR_ROWS: u8 = 5;
    /// `bq.a(int,int)` (SKT `ci.a()`) finds the record by `(type, id 0)` — only an
    /// id-0 record of a cycle type can be the `0` key's item.
    pub const HEAL_ITEM_ID: i8 = 0;
    /// The shell's ARROW BOX (`g.a(Graphics,int,int,boolean)`: image `as.o[5]` centred on
    /// `x`, top at `y`, with `◀ ▶` at `x ± (w/2 + 6)`). Measured on the S25 title
    /// (2026-09-17): 66 x 17 at (238, 172) in a 476x245 framebuffer = `(ax.j, ¾·h − 12)`.
    /// The hit box is a little taller than the art; the arrow zones run out to ±62.
    pub const SHELL_BOX_HALF_W: u32 = 33;
    pub const SHELL_BOX_H: u32 = 22;
    pub const SHELL_ARROW_REACH: u32 = 62;
    /// Slot screen (SKT `ca.b`/`ca.c`): the box top is `ax.k − 45 + 35` in modes 1/4/10
    /// and `ax.k − 51 + 26` in mode 11 (class pick). `+0x28 a:B` is the mode.
    pub const SLOT_MODE_OFF: u32 = 0x28;
    pub const SLOT_BOX_DY: u32 = 10;
    pub const SLOT_CLASS_BOX_DY: u32 = 25;
    pub const SLOT_MODE_CLASS_PICK: u8 = 11;
    /// OPTION (SKT `l.a(int,int)`): rows at `ax.k − 36 + 18·i`, labels from `ax.j − 45`,
    /// values at `ax.j + 24`.
    pub const OPTION_ROWS: u32 = 5;
    pub const OPTION_ROW_DY: u32 = 36;
    pub const OPTION_ROW_PITCH: u32 = 18;
    pub const OPTION_ROW_LEFT: u32 = 50;
    pub const OPTION_ROW_RIGHT: u32 = 72;
    /// HELP (SKT `ai.a(int,int)`): header line at `ax.k − 43` in-game / `ax.k − 36` on the
    /// shell, its arrow span `ax.j − 50 .. + 50`, the text from `+ 23` (≤ 82 rows in the
    /// in-game panel). The header's outer thirds page, the body scrolls.
    pub const HELP_HEADER_DY: u32 = 43;
    pub const HELP_HEADER_DY_SHELL: u32 = 36;
    pub const HELP_HEADER_H: u32 = 18;
    pub const HELP_HALF_W: u32 = 66;
    pub const HELP_ARROW_DX: u32 = 25;
    pub const HELP_BODY_DY: u32 = 23;
    pub const HELP_BODY_H: u32 = 90;
    /// QUEST (SKT `y.b(int,int)`): the LEFT/RIGHT arrow span `b(g, x+7, y+22, 26, …)` — the
    /// hit band is that span plus slack; everything else on the panel is the toggle.
    pub const QUEST_ARROWS_X0: u32 = 4;
    pub const QUEST_ARROWS_W: u32 = 34;
    pub const QUEST_ARROWS_Y0: u32 = 16;
    pub const QUEST_ARROWS_H: u32 = 16;
    /// CARD (SKT `at.b(int,int)`): `x + 20 + col·17 − 1`, `y + 15 + row·17 − 1`, 15 x 15, 5 columns.
    pub const CARD_CELL_X0: u32 = 19;
    pub const CARD_CELL_Y0: u32 = 14;
    pub const CARD_CELL_PITCH: u32 = 17;
    pub const CARD_CELL_W: u32 = 15;
    pub const CARD_COLS: u32 = 5;
    pub const MENU_ROW_Y0: u32 = 12;
    pub const MENU_ROW_PITCH: u32 = 12;
    pub const MENU_ROW_H: u32 = 10;
    /// SKILL window cells (SKT `m.b(int,int)` cursor box): `x + 5 + col·57 (+15 on ACTIVE)`,
    /// `y + 12 + row·19 (+10 on ACTIVE)`, `52 (−14 on ACTIVE) x 13`; the tab header
    /// (`◀ PASSIVE/ACTIVE ▶`) at `y + 112`: the text box is 62 wide from `x + 8`, the two
    /// arrows sit just outside it (probe: `◀` at ~x+4, `▶` at ~x+76), so the hit box is
    /// the box plus 7 px each side.
    pub const SKILL_CELL_X0: u32 = 5;
    pub const SKILL_CELL_COL_PITCH: u32 = 57;
    pub const SKILL_CELL_ACTIVE_DX: u32 = 15;
    pub const SKILL_CELL_Y0: u32 = 12;
    pub const SKILL_CELL_ROW_PITCH: u32 = 19;
    pub const SKILL_CELL_ACTIVE_DY: u32 = 10;
    pub const SKILL_CELL_W: u32 = 52;
    pub const SKILL_CELL_ACTIVE_DW: u32 = 14;
    pub const SKILL_CELL_H: u32 = 13;
    pub const SKILL_HEADER_Y: u32 = 112;
    pub const SKILL_HEADER_X0: u32 = 8;
    pub const SKILL_HEADER_W: u32 = 62;
    pub const SKILL_HEADER_H: u32 = 12;
    /// STATUS (SKT `bp.b(int,int)`): `x + 5 + col·37`, `y + 91 + row·12`, 34 x 13, `h = row·3 + col`;
    /// `+0x34 a:Z` = unspent points exist (the grid is only drawn then).
    pub const STATUS_CELL_X0: u32 = 5;
    pub const STATUS_CELL_COL_PITCH: u32 = 37;
    pub const STATUS_CELL_Y0: u32 = 91;
    pub const STATUS_CELL_ROW_PITCH: u32 = 12;
    pub const STATUS_CELL_W: u32 = 34;
    pub const STATUS_CELL_H: u32 = 13;
    pub const STATUS_COLS: u32 = 3;
    pub const STATUS_HAS_POINTS_OFF: u32 = 0x34;
    /// EQUIPMENT (SKT `co.b(int,int)`): `x + 40 + col·14`, `y + 35 + row·14 (−7 in column 1)`,
    /// 11 x 11 for `h = 0..5`; `h = 6` at `(x + 54, y + 56)`.
    pub const EQUIP_CELL_X0: u32 = 40;
    pub const EQUIP_CELL_Y0: u32 = 35;
    pub const EQUIP_CELL_PITCH: u32 = 14;
    pub const EQUIP_CELL_W: u32 = 11;
    pub const EQUIP_COL1_DY: u32 = 7;
    pub const EQUIP_LAST_X: u32 = 54;
    pub const EQUIP_LAST_Y: u32 = 56;
    /// ★ 의상 아바타 A3: how far [`GuestBuild::equip_cell_shift`] moves the seven cells
    /// LEFT (the six-cell loop from `x + 40` to `x + 8`, the seventh from `x + 54` to
    /// `x + 22`) — the panel is 120 wide, so the cells take its left third and leave the
    /// middle to the character preview and the right to the avatar cell.
    pub const EQUIP_CELL_SHIFT: u32 = 32;
    /// ★ A3: the painter's two `adds` that place the cells ([`GuestBuild::equip_cell_shift`]'s
    /// first two sites) — `adds r0,#imm8` (0x30xx) for the six-cell origin, `adds r2,#imm8`
    /// (0x32xx) for the seventh. The host reads the immediates back to hit-test and to lay its
    /// layer where the game ACTUALLY draws (`h2_equip_cells_x`), never where it was asked to.
    pub const EQUIP_CELLS_X_SITE: u32 = 0x0013_e98a;
    pub const EQUIP_LAST_X_SITE: u32 = 0x0013_e9fe;
    /// ★ A3: the cells the game's RIGHT leaves from to reach the host's avatar slot — the
    /// right column (`col = h % 3 == 2`: h 2 and 5).
    pub const EQUIP_RIGHT_CELLS: [i8; 2] = [2, 5];
    /// ★ A3: the character preview's box in the panel (panel-relative; the cells end at
    /// x 47 once shifted, the avatar slot starts at x 93, the name box at y 93) and where the
    /// character's origin (its feet — the head is drawn at −25, the legs end at +4) stands.
    pub const EQUIP_PREVIEW_X: i32 = 50;
    pub const EQUIP_PREVIEW_Y: i32 = 14;
    pub const EQUIP_PREVIEW_W: u32 = 40;
    pub const EQUIP_PREVIEW_H: u32 = 78;
    pub const EQUIP_PREVIEW_OX: i32 = 70;
    pub const EQUIP_PREVIEW_OY: i32 = 62;
    /// INVENTORY (SKT `bf.b(int,int)`): `x + 7 + col·15`, `y + 31 + row·15`, 13 x 13, 7 per row.
    ///
    /// ⛔ As SHIPPED. Once [`GuestBuild::inventory_grid_shift`] is in, the game draws at
    /// `x + 7 + INV_GRID_SHIFT + col·15`, and the host must hit-test where the pixels
    /// actually are — ask `crate::inv_grid_shift()`, never this constant alone.
    pub const INV_CELL_X0: u32 = 7;
    pub const INV_CELL_Y0: u32 = 31;
    pub const INV_CELL_PITCH: u32 = 15;
    pub const INV_CELL_W: u32 = 13;
    pub const INV_COLS: u32 = 7;
    /// The square `q.a(II)V` fills black for a cell (`setColor(0)`, `fillRect(x, y, 14, 14)` @0x146dd2 — the
    /// 1-px gap of the 15-px pitch is the panel's body colour). ★ 2026-10-02: what the UI paints over a cell the
    /// short last page does not have (`hero2java::h2_page_hidden_cells`).
    pub const INV_CELL_FILL: u32 = 14;
    /// How far [`GuestBuild::inventory_grid_shift`] moves the `q` grid right, in guest
    /// pixels. The window's left margin becomes `INV_CELL_X0 + INV_GRID_SHIFT` = 14,
    /// which is what the page-tab rail is drawn into.
    pub const INV_GRID_SHIFT: u32 = 7;
    /// The rail the tabs live in once the grid has moved: `x + 1 .. x + 13` of the panel,
    /// beside the four grid rows (`INV_CELL_Y0 .. + 4·INV_CELL_PITCH` = 60 px, which is
    /// five 12-px tabs exactly). 1 px of panel body is left either side so the rail reads
    /// as sitting IN the window rather than replacing its edge.
    pub const BAG_RAIL_X: u32 = 1;
    pub const BAG_RAIL_W: u32 = 12;
    /// BEAST (SKT `bm.b(int,int)`): `x + 19 + col·57`, `y + 69 + row·19`, 38 x 13, 2 per row;
    /// only with a beast (`+0x28 a:Lbn;` non-null).
    pub const BEAST_CELL_X0: u32 = 19;
    pub const BEAST_CELL_COL_PITCH: u32 = 57;
    pub const BEAST_CELL_Y0: u32 = 69;
    pub const BEAST_CELL_ROW_PITCH: u32 = 19;
    pub const BEAST_CELL_W: u32 = 38;
    pub const BEAST_CELL_H: u32 = 13;
    pub const BEAST_OBJ_OFF: u32 = 0x28;
    /// The beast sub-list (SKT `cm.b(int,int)`): rows at `y + 29 + h·17`, `x + 19 .. x + 107`.
    pub const BEAST_LIST_X0: u32 = 19;
    pub const BEAST_LIST_Y0: u32 = 29;
    pub const BEAST_LIST_PITCH: u32 = 17;
    pub const BEAST_LIST_W: u32 = 88;
    /// The shared centred LIST painter (SKT `g.a(Graphics,int,int,int,int,boolean)`, used by
    /// SYSTEM): rows 64 wide centred on `ax.j`, 14 tall on a 15 pitch, the block centred on
    /// `ax.k` (`top = cy − 15·rows/2`).
    pub const LIST_ROW_W: u32 = 64;
    pub const LIST_ROW_H: u32 = 14;
    pub const LIST_ROW_PITCH: u32 = 15;
    /// The two-button popup (kind 2, SKT `ba.a(Graphics,byte[][],int,boolean)`): a 117-wide box
    /// of height `16·(2 + lines)` centred on both axes; the buttons (42 x 15) at
    /// `y = top + 12 + 16·lines + 1`, 예 at `cx − 44`, 아니오 at `cx + 2`. `h == 0` = 예.
    pub const POPUP_KIND_CONFIRM: u8 = 2;
    pub const CONFIRM_BOX_W: u32 = 117;
    pub const CONFIRM_BTN_W: u32 = 42;
    pub const CONFIRM_BTN_H: u32 = 15;
    pub const CONFIRM_BTN_LEFT_DX: u32 = 44;
    pub const CONFIRM_BTN_RIGHT_DX: u32 = 2;
    pub const CONFIRM_LINE_H: u32 = 16;
    pub const CONFIRM_TOP_PAD: u32 = 12;
    /// ★ 2026-09-25 (부활의 구 — the host draws a box of this family itself): what the
    /// painters DRAW, which is not quite the tap geometry above. Read out of
    /// `d.a(Graphics,IZ)V` @0x15a088 and measured on the 새로하기 confirm (176x220):
    /// the 아니오 button's outline starts at `cx + 1` (its black fill at `cx + 2`), the
    /// labels are drawn at `cx − 27` (예) / `cx + 10` (아니오), 3 px below the button top,
    /// and the one outline colour of box and button is `j.a:[I`[0] = `#e6d7c0`.
    pub const CONFIRM_BTN_RIGHT_RECT_DX: u32 = 1;
    pub const CONFIRM_LABEL_YES_DX: u32 = 27;
    pub const CONFIRM_LABEL_NO_DX: u32 = 10;
    pub const CONFIRM_LABEL_DY: u32 = 3;
    pub const CONFIRM_OUTLINE: u32 = 0xffe6_d7c0;
    /// Popup kind of the skill / beast-skill context menu (SKT `ba` ctor arg).
    pub const POPUP_KIND_CONTEXT: u8 = 3;
    /// Popup box geometry (SKT `ba.b(g, 74, height)` / `ba.a(int,int)`): width 74, height
    /// `6 + 16 * rows`, centred on `(ax.j, ax.k)`, row `i` at `y + 6 + 16 * i`.
    pub const POPUP_W: u32 = 74;
    pub const POPUP_ROW_H: u32 = 16;
    pub const POPUP_PAD_Y: u32 = 6;
    /// The guest's quickbar width — `ag.j.length`.
    pub const QUICKBAR_LEN: u32 = 4;
    /// Quickbar values at and above this are BEAST skills (`v - 10` = beast skill slot).
    pub const BEAST_BASE: u8 = 10;
    /// …and a beast has this many skill slots (`bn.j:[B` / `bn.k:[B` are `byte[4]`), so
    /// beast values are `10..=13`.
    pub const BEAST_SKILL_SLOTS: u32 = 4;
}

impl GuestBuild {
    pub fn char_anchor(self) -> Option<CharAnchor> {
        match self {
            Self::Ktf => Some(CharAnchor::Pointer {
                global: 0x1b_3084,
                fallback: 0x1b_7b6c,
            }),
            Self::Lgt => Some(CharAnchor::Static(0x150_3a88)),
            Self::Skt => None,
            // ★★ hero2, 2026-09-09. Two dereferences and the base is the field block —
            // see `CharAnchor::JavaObject` and `HERO2_GUEST_RE.md` §2.
            Self::Hero2Ktf => Some(CharAnchor::JavaObject { slot: 0x0019_3b5c }),
        }
    }

    /// May the host perform this kind of Java data write on this build? Only hero2, and
    /// only the kinds listed in [`JavaWrite`] — each one was measured (address, meaning,
    /// bucket-class consequence) before it was allowed. No hero3 build has Java data.
    pub fn java_write_ok(self, _what: JavaWrite) -> bool {
        matches!(self, Self::Hero2Ktf)
    }

    /// May the host make this JVM call on this build? Only hero2, and only the methods
    /// listed in [`JavaCall`] — each one read in the disassembly for what it does to the
    /// record it is given before it was allowed. No hero3 build has a JVM to call into:
    /// hero3's guest is C, and its host calls are [`GuestBuild::save_chain`] addresses.
    pub fn java_call_ok(self, _what: JavaCall) -> bool {
        matches!(self, Self::Hero2Ktf)
    }

    /// hero2's Java-side anchor table. `None` on every other build — nothing in it means
    /// anything outside `client.bin1120` (md5 `08a53cc4…`).
    /// hero2's one-call save, on hero2 only — see [`Hero2Save`]. Every other build
    /// answers `None` and keeps using [`GuestBuild::save_chain`].
    pub fn hero2_save(self) -> Option<Hero2Save> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(Hero2Save {
                class: "ag",
                field: "a",
                descriptor: "Lag;",
                method: "l",
                area: Hero2SaveArea {
                    stage_slot: 0x0019_3afc,
                    stage_type_off: 0x0,
                    max_saveable_slot: 0x0019_3fbc,
                    trigger_groups_slot: 0x0019_404c,
                    scripts_slot: 0x0019_405c,
                    event_flags_slot: 0x0019_3a4c,
                    quest_states_slot: 0x0019_3a5c,
                },
            }),
        }
    }

    pub fn hero2_java(self) -> Option<Hero2Java> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(Hero2Java {
                char_slot: 0x0019_3b5c,
                quickbar_slot: 0x0019_3bac,
                cooldown_slot: 0x0019_3bbc,
                class_slot: 0x0019_3a6c,
                save_slot_slot: 0x0019_3a9c,
                beast_off: 0x134,
                beast_skill_ids_off: 0x3c,
                // `bn` declares `[B j +0x8` / `[B k +0xc` in its own block, and the ids
                // array is known to sit at absolute 0x3c — so the block starts at 0x34
                // and the levels are at 0x40.
                beast_skill_levels_off: 0x40,
                // `a` declares `S c +0x8` / `S d +0xc` / `S e +0x28` / `[B p +0xdc` in its
                // own block, which starts at 0x5c (the level `a.a:B +0x0` is the gauges'
                // `+0x5c`). 0x5c + those = 0x64 / 0x68 / 0x84 / 0x138.
                char_stat_points_off: 0x64,
                char_skill_points_off: 0x68,
                char_abilities_off: 0x84,
                char_skill_levels_off: 0x138,
                char_combo_open_off: 0xa8,
                combo_limits_slot: 0x0019_455c,
                // `ag.k:B` / `ag.l:B` — the only writers are `ag.b(BB)V` @0x110bc4 and
                // `<clinit>`; the only reader is `ag.c()V` (find_field_uses.py, 2026-09-24).
                walk_request_slots: (0x0019_3bec, 0x0019_3bfc),
                // ay / bl / bx — `[S a` statics at 0x193d6c / 0x1940cc / 0x19425c.
                cooldown_tables: [(0x0019_25bc, 0x0019_3d6c), (0x0019_2790, 0x0019_40cc), (0x0019_2940, 0x0019_425c)],
                // ay = 로그 (class 2) · bl = 버서커 (class 0) · bx = 나이트 (class 1).
                class_base_stats: [
                    (0x0019_25bc, [11, 9, 13, 10, 10]),
                    (0x0019_2790, [13, 12, 9, 11, 10]),
                    (0x0019_2940, [10, 11, 8, 12, 12]),
                ],
                inventory_slot: 0x0019_3b7c,
                material_bag_slot: 0x0019_3b8c,
                card_bag_slot: 0x0019_3b9c,
                menu_slot: 0x0019_422c,
                menu_origin: (0x0019_41ec, 0x0019_41fc),
                title_slot: 0x0019_448c,
                title_origin: (0x0019_446c, 0x0019_447c),
                gm_table_slot: 0x0019_420c,
                menu_last_row_slot: 0x0019_424c,
                heal_cycle_slot: 0x0019_40fc,
                screen: (0x0019_4b4c, 0x0019_4b5c, 0x0019_4b6c, 0x0019_4b7c),
                skill_static_b: 0x0019_444c,
                script_mode_slot: 0x0019_3d8c,
                script_choice_sel_slot: 0x0019_3e3c,
                classes: Hero2Classes {
                    window: 0x0019_3000,
                    popup: 0x0019_2e74,
                    quantity_popup: 0x0019_249c,
                    skill_window: 0x0019_2ca0,
                    beast_window: 0x0019_2310,
                    field_menu: 0x0019_291c,
                    beast: 0x0019_27d8,
                    status_window: 0x0019_2430,
                    equip_window: 0x0019_29f4,
                    inventory_window: 0x0019_2c58,
                    list_windows: [0x0019_25e0],
                    title_window: 0x0019_2cc4,
                    slot_window: 0x0019_2694,
                    option_window: 0x0019_2c7c,
                    quest_window: 0x0019_2454,
                    card_window: 0x0019_2a84,
                    detail_window: 0x0019_2af0,
                    item_detail_window: 0x0019_2a18,
                    message_window: 0x0019_2604,
                    beast_list_window: 0x0019_2988,
                    help_window: 0x0019_2748,
                },
            }),
        }
    }

    /// Where this build keeps the HUD gauges inside the character record.
    ///
    /// hero3's numbers are the ones that were already in `items.rs`; hero2's are the
    /// absolute offsets from `HERO2_GUEST_RE.md` §2.1, every one of them cross-checked
    /// against the game's own STATUS screen on the same probe frame.
    pub fn char_layout(self) -> Option<CharLayout> {
        match self {
            // hero3 (both carriers share the record layout — only the state byte moved).
            Self::Ktf | Self::Lgt => Some(CharLayout {
                level: 0xe2,
                hp: 0xf2,
                hp_bytes: 2,
                hp_max: 0xec,
                sp: 0xf8,
                sp_max: None,
                sp_max_const: 1000,
                exp: 0xe4,
                exp_next: 0xe8,
                points: Some((0x18a, 0x18c)),
                skill_points_bytes: 1,
                point_caps: (276, 44),
                hp_signed: true,
                level_cap: 70,
                hp_cap: 30000,
            }),
            Self::Skt => None,
            // ★★ hero2. All seven gauges are `int`-wide and the record HAS a max-SP
            // field, which hero3 does not — that is why `sp_max` is an Option.
            //
            // ★ The unspent points (2026-09-18, user: "레벨업하면 좌측상단 HP MP 바 밑으로
            // 스킬이나 스탯 찍을 수 있다고 깜빡깜빡 안내 … 옮겨줘"): the character class `a`
            // (KTF, `dump_hero2_java.py`) lays its own fields after `ad` (0x34) + `g`
            // (0x28) = 0x5c, every slot 4 bytes wide, so `a.a:B` = level @0x5c, then
            // `a.g:B` @0x60, **`a.c:S` @0x64 = STAT points**, **`a.d:S` @0x68 = SKILL
            // points**, `a.a:I` hp @0x6c … `a.f:I` exp-to-next @0x80 — the gauges this
            // table already had, which is the cross-check. Who reads them says which is
            // which (`tools_re/hero2_dis.py` cell decoding): the STATUS window's
            // `an.a()Z` ("has points to spend", `HERO2_GUEST_RE.md` §5.2) and the game's
            // own bottom-HUD painter `n.d()V` test `a.c:S`; the SKILL window `s.a(B)V`
            // and the beast-skill window `af.a(BB)V` spend `a.d:S`; the save serialiser
            // `a.b()[B` carries both. Probe CHAR dump of the LV 48 save: `a.c:S` = 0,
            // `a.d:S` = 2, matching the game's own STATUS/SKILL screens.
            Self::Hero2Ktf => Some(CharLayout {
                level: 0x5c,
                hp: 0x6c,
                hp_bytes: 4,
                hp_max: 0x78,
                sp: 0x70,
                sp_max: Some(0x7c),
                sp_max_const: 0,
                exp: 0x74,
                exp_next: 0x80,
                points: Some((0x64, 0x68)),
                skill_points_bytes: 2,
                point_caps: (u16::MAX, u16::MAX),
                hp_signed: true,
                // 영웅서기2 의 레벨 상한은 아직 실측 전이다. 관측된 최대가 48 이므로
                // 넉넉히 99 로 두되, 이 값은 **가설**이고 상한을 실측하면 좁힌다.
                // (게이트로서의 역할은 "0 이나 쓰레기 값을 캐릭터로 착각하지 않는다" 뿐이다.)
                level_cap: 99,
                // ★ 2026-10-22 가디언 레벨: GL 250 gives 747 more stat points (D1 = 3 a GL) — a 체력 build passes 30,000
                // max HP, and a gate at 30,000 would read the character as "not a character" (the field HUD, the plates,
                // the warehouse and the difficulty all fall with `resolve_char`). The game keeps HP in an `int`.
                hp_cap: 1_000_000,
            }),
        }
    }

    /// Where the coarse screen-state word is on this build, and its values.
    /// `None` = not measured, and every mode gate then stays closed.
    pub fn app_state(self) -> Option<AppState> {
        match self {
            Self::Ktf => Some(AppState::CharRelative { off: 0x7c, field: 4, window: 5, shell: &[3] }),
            Self::Lgt => Some(AppState::CharRelative { off: 0x78, field: 4, window: 5, shell: &[3] }),
            Self::Skt => None,
            // ★★ hero2: an absolute Java static, and the values are NOT hero3's.
            // 0 = publisher logo · 2 = title · 1 = slot select · 3 = FIELD · 5 = window ·
            // 4 = NPC 대화 (bottom dialogue box) · 8 = NPC 기능창 (device, 2026-09-17).
            // The shell is TWO values here, where hero3 folds title and slot into one.
            Self::Hero2Ktf => Some(AppState::Absolute {
                addr: 0x0019_3aec,
                field: 3,
                window: 5,
                shell: &[1, 2],
                in_world: &[4, 8],
                bottom_overlay: &[4],
                script: &[4, 12],
            }),
        }
    }

    /// Does this build's character record have the layout **every CHAR offset spelled
    /// out in `items.rs` was measured against** (the quickbar arrays at `+0x252`, the
    /// inventory grid handle at `+0x9e28`, the area object, the popup block, the skill
    /// strips, the cast-state byte …)?
    ///
    /// ── WHY THIS FLAG HAD TO EXIST THE MOMENT hero2 GOT AN ANCHOR (2026-09-09) ──
    /// Those readers are all "safe" in the sense that they only READ — but they are not
    /// safe in the sense that matters. They are gated by nothing except
    /// `resolve_char`, and `resolve_char` now succeeds on hero2. Left alone they would
    /// read hero2's Java field block at hero3's offsets and hand the HUD **confident
    /// nonsense**: quickbar plates with invented item kinds and stack counts, an item
    /// icon per plate from the atlas, and a save-inhibitor byte pulled out of whatever
    /// `+0x9c90` happens to hold. That is worse than a blank plate, because a blank
    /// plate tells the truth.
    ///
    /// [`char_layout`](Self::char_layout) is deliberately NOT this flag: the gauges were
    /// measured per build and travel; everything else did not.
    pub fn char_record_is_hero3(self) -> bool {
        match self {
            Self::Ktf | Self::Lgt => true,
            // SKT boots but nothing was measured on it; hero2 is a different game.
            Self::Skt | Self::Hero2Ktf => false,
        }
    }

    /// The band the GAME paints its own field HUD into, in **logical guest pixels**, as
    /// `(y, height)` — or `None` when the host must not paint over it.
    ///
    /// ── WHY THIS IS `None` ON hero3 AND `Some` ON hero2 ─────────────────────────
    /// Both games draw a bottom HUD; the difference is whether the WORLD is under it.
    /// * hero3 paints its 176x24 box **on top of live world pixels**, bottom-centre.
    ///   Masking it means painting that box opaque, and the player rejected that black
    ///   band twice (see `overlay::mask_game_hud`). So: `None`, and the game's strip
    ///   stays visible.
    /// * hero2 gives its HUD a **reserved strip**. Measured 2026-09-09 at logical widths
    ///   176 / 352 / 440: the world viewport ends at y = 193 in all three (the game's own
    ///   `n.b` static reads 194 at every width, `HERO2_GUEST_RE.md` §3.1), and rows
    ///   194..217 hold nothing but the HUD art — outside its box those rows are already
    ///   black. So masking costs **zero world pixels**, which is what makes hero3's
    ///   objection not apply and lets the host own the bottom band outright.
    pub fn guest_hud_band(self) -> Option<(u32, u32)> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some((194, 24)),
        }
    }

    /// Rows the GAME reserves at the bottom of the framebuffer for its own HUD, which the
    /// host asks for ON TOP of the size it means to show and then does not present.
    ///
    /// ── WHY THIS BEATS MASKING (2026-09-10) ─────────────────────────────────────────
    /// Painting the band black (what [`guest_hud_band`] was for) leaves a dead strip
    /// across the bottom of the screen — 24 of 220 logical rows, 11% of the window, and
    /// the player reported it as broken. Scaling the picture up to hide it would trade
    /// the strip for a 13% zoom, i.e. LESS world.
    ///
    /// This does neither. The game sizes its world viewport from the framebuffer it is
    /// given, and the reservation is a CONSTANT: measured 2026-09-10 by reading the
    /// game's own `n.b` static (`HERO2_GUEST_RE.md` §3.1) at three heights —
    ///
    /// ```text
    ///   framebuffer h   n.b (world viewport height)
    ///        220            194   = h - 26
    ///        246            220   = h - 26
    ///        300            274   = h - 26
    /// ```
    ///
    /// — so asking for `want + 26` and presenting only the top `want` rows puts the world
    /// exactly where the black strip used to be, at the SAME scale, with MORE world in
    /// view and no guest write anywhere. The HUD is still drawn; it is simply drawn into
    /// rows nobody looks at.
    ///
    /// ⚠ 26, not 24: rows 218..219 of the 176x220 original are black filler below the
    /// 24-row art, and they are part of the reservation the game makes.
    ///
    /// ⛔ The SHELL (title, slot list, publisher splash) is BOTTOM-ANCHORED — the slot
    /// screen's lower decorative band and the splash's version line both sit in the last
    /// rows — so those screens must be presented WHOLE. That is the caller's job; see
    /// `App::game_source_window`.
    pub fn hud_reserved_rows(self) -> u32 {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => 0,
            // ★ 26 from 2026-09-10 to 2026-09-18. Zero since [`Self::viewport_full_patch`]:
            // the game now sizes its world viewport to the WHOLE framebuffer, so there is
            // nothing to reserve and nothing to scroll — dialogue boxes overlay the world
            // the way the original phone showed them.
            Self::Hero2Ktf => 0,
        }
    }

    /// hero2's inventory paging patch — see [`InventoryPagePatch`]. `None` everywhere else:
    /// no other build has the AOT cell/helper shape it relies on, and no hero3 build has
    /// a `q` class at all.
    ///
    /// Measured 2026-09-21 (`tools_re/hero2_dis.py 'q.a(II)Z' / 'q.a(BB)V' / 'q.a(II)V'`,
    /// cells decoded with `0x185644 + ((w & 0xffff) >> 1) * 0x1c`, defs' word 0 read back
    /// at runtime as `0x1365c5 / 0x136d3d / 0x136e05`):
    ///
    /// | site `bl` | in | cell | target |
    /// |---|---|---|---|
    /// | 0x146688 | `q.a(II)Z` key: `bq.a(j.h)` | 0x17d280 | `bq.a(I)Lch;` |
    /// | 0x146984 | `q.a(BB)V` 사용하기: `bq.a(j.h, 1)` | 0x17d37c | `bq.a(BI)V` |
    /// | 0x1469dc | `q.a(BB)V`: `bq.a(j.h)` | 0x17d350 | `bq.a(I)Lch;` |
    /// | 0x146a5e | `q.a(BB)V` 버리기: `bq.b(j.h)` | 0x17d370 | `bq.b(B)V` |
    /// | 0x146a9e | `q.a(BB)V`: `bq.a(j.h)` | 0x17d338 | `bq.a(I)Lch;` |
    /// | 0x146dfe | `q.a(II)V` grid: `bq.a(row*7+col)` | 0x17d398 | `bq.a(I)Lch;` |
    /// | 0x146f10 | `q.a(II)V` name line: `bq.a(j.h)` | 0x17d3c4 | `bq.a(I)Lch;` |
    pub fn inventory_page_patch(self) -> Option<InventoryPagePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(InventoryPagePatch {
                sites: [
                    PageSite { bl: 0x0014_6688, cell: 0x0017_d280, target: 0 },
                    PageSite { bl: 0x0014_6984, cell: 0x0017_d37c, target: 1 },
                    PageSite { bl: 0x0014_69dc, cell: 0x0017_d350, target: 0 },
                    PageSite { bl: 0x0014_6a5e, cell: 0x0017_d370, target: 2 },
                    PageSite { bl: 0x0014_6a9e, cell: 0x0017_d338, target: 0 },
                    PageSite { bl: 0x0014_6dfe, cell: 0x0017_d398, target: 0 },
                    PageSite { bl: 0x0014_6f10, cell: 0x0017_d3c4, target: 0 },
                ],
                targets: [
                    // bq.a(I)Lch; · bq.a(BI)V · bq.b(B)V
                    PageTarget { def: 0x0018_91a8, body: 0x0013_65c4 },
                    PageTarget { def: 0x0018_926c, body: 0x0013_6d3c },
                    PageTarget { def: 0x0018_92c0, body: 0x0013_6e04 },
                ],
                helper_virtual: 0x0016_917c,
                helper_direct: 0x0016_9160,
            }),
        }
    }

    /// ★★ 2026-10-20 the 사용하기 → `a.g(B)V` site the paging patch missed — see [`ItemUsePagePatch`].
    pub fn item_use_page_patch(self) -> Option<ItemUsePagePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(ItemUsePagePatch {
                bl: 0x0014_6b54,
                cell: 0x0017_d320,
                def: 0x0018_bd68,
                body: 0x0015_21dc,
                inventory_slot: 0x0019_3b7c,
                helper_virtual: 0x0016_917c,
                helper_direct: 0x0016_9160,
            }),
        }
    }

    /// The `− 26` in hero2's screen constructor, as `(site, orig, patched)`, or `None`.
    ///
    /// ── WHAT IT IS (`n.<init>()V`, `tools_re/hero2_dis.py`, 2026-09-18) ──────────────
    /// `n.b = ax.i − 26` at **0x1624ec** (`subs r1, #0x1a` = 0x391a) is the world
    /// viewport height: the game keeps the last 26 framebuffer rows for its own HUD band
    /// and clamps the camera to `n.b` (`n.d = n.b / 2` follows). With the band's painter
    /// switched off ([`Self::hud_bottom_patch`]) those rows are empty, so the host used
    /// to ask for 26 extra rows and crop them (`hud_reserved_rows`), scrolling only when
    /// a bottom-anchored box was up. User (2026-09-18, "hero3방식 그대로 가"): patch the
    /// constant instead — `subs r1, #0` (0x3900), size-neutral — so the viewport IS the
    /// framebuffer, no reservation, no scroll, and the dialogue box overlays the world's
    /// bottom rows as on the original 176x220 phone.
    ///
    /// ── MEASURED (wie_probe, 176x220, 2026-09-18) ────────────────────────────────────
    /// `--poke 10:0x1624ec:0039`: `n.b` 194 → 220, `n.d` 97 → 110, the world fills all
    /// 220 rows on the field, the menu centres on the full height, the intro's dialogue
    /// box sits over the world's bottom rows. A/B against the unpatched run (field →
    /// menu → STATUS → back): primitive statics differ only in `n.b`/`n.d` and the two
    /// camera offsets `ag.c:I`/`ag.e:I` (both by 13 = 26/2, the camera centre); the
    /// character's field block differs only in heap POINTERS (the larger buffers shift
    /// allocation) — no primitive game state.
    ///
    /// ⛔ Must be in place before the field's `n` is constructed (the static is written
    /// in the constructor): the worker's patch pass runs from frame 30, the title's `n`
    /// is made around frame 330 and the field's later still, and every screen change
    /// makes a new `n`.
    pub fn viewport_full_patch(self) -> Option<(u32, u16, u16)> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some((0x0016_24ec, 0x391a, 0x3900)),
        }
    }

    /// ★★ #14 (2026-09-21, user: *"인벤토리 및 계정창고 칸들을 전체적으로 오른쪽으로 조금
    /// 밀고 그 메뉴박스 안으로 넣을 수 있어? … 왼쪽 영역 가방5개. 그리고 그 바로 옆에 28칸"*).
    ///
    /// Move the `q` grid (INVENTORY · the 재료가방 · both ACCOUNT warehouse windows —
    /// they are all `q`) right by [`INV_GRID_SHIFT`] px, so the page-tab rail can live
    /// INSIDE the window instead of floating in the world to its left.
    ///
    /// ── WHY A GUEST PATCH AT ALL ─────────────────────────────────────────────────
    /// The 28 cells are the GAME's pixels. Measured on the shipped image (176x220,
    /// inventory open, `wie_probe --dump`): the window's posts are x 20..24 and
    /// 151..155, the panel is `bw.a .. +120` = 27..147, and the grid box is 34..140 —
    /// 7 px of margin each side. A 16 px rail does not fit in 7, and no amount of host
    /// drawing can move a pixel the guest paints. So either the grid moves or the rail
    /// stays outside.
    ///
    /// ── THE TWO SITES (`tools_re/hero2_dis.py`, own methods of `q`) ──────────────
    /// ```text
    /// q.a(II)V @0x146cec   the grid painter: loops col 0..6, draws 14x14 and calls
    ///   0x146d9a  movs r6,#0xf        ; pitch 15
    ///   0x146da4  adds r4,#7          ; x + 7          <-- SITE A (T2, imm8)
    ///   0x146db0  adds r4,#0x1f       ; y + 31
    ///   0x146dfa  bl bq.a(I)Lch;      ; record row*7+col
    /// q.b(II)V @0x147090   the 13x13 cursor/selection rect:
    ///   0x1470a4  adds r6,r2,#0       ; r6 = x         <-- SITE B (T1, imm3)
    ///   0x1470ce  adds r0,r6,#7       ; x + 7          (imm3 already at its max)
    /// ```
    /// Both immediates are the `INV_CELL_X0`/`INV_CELL_PITCH`/`INV_CELL_W` the host has
    /// been hit-testing with since 2026-09-17 — the disassembly CONFIRMED the constants
    /// rather than supplying them, which is why 7 is the number this shifts.
    ///
    /// ⛔ **+7 is a CEILING, not a taste.** Site B's `adds r0,r6,#7` is the 3-bit-immediate
    /// form already at 7, so the only room left is its input: `adds r6,r2,#0` → `#7`.
    /// 7 + 7 = 14 is as far as two-byte patches reach, and it is exactly enough — it
    /// turns the 7 px left margin into 14 and spends the 7 px right margin, leaving the
    /// grid flush with the panel's right edge (where the 소지금 bar already ends).
    ///
    /// ── MEASURED, NOT ARGUED (wie_probe, 2026-09-21) ────────────────────────────
    /// Two runs of the same key script differing only by these four bytes, dumping the
    /// open inventory at f=860: the grid's column separators moved 34,49,…,139 →
    /// 41,56,…,146 (exactly +7, every one), the item icons and the blue selection frames
    /// moved WITH them, and the window frame, title, 소지금 bar and 재료가방 label did
    /// not move by a pixel. Guest heap counters (`allocs`/`frees`/`LIVE`) unchanged: this
    /// writes no game state, it only relocates a draw.
    pub fn inventory_grid_shift(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            // (site, as shipped, patched)
            Self::Hero2Ktf => &[
                // q.a(II)V: `adds r4,#7` -> `adds r4,#14`
                (0x0014_6da4, 0x3407, 0x340e),
                // q.b(II)V: `adds r6,r2,#0` -> `adds r6,r2,#7` (its `+7` below stays)
                (0x0014_70a4, 0x1c16, 0x1dd6),
            ],
        }
    }

    /// ★★ 의상 아바타 A3 (2026-09-30, user: *"장비창: 기존 장착 장비 영역은 왼쪽, 캐릭터는 가운데,
    /// 아바타 영역은 오른쪽"*): move the EQUIPMENT window's seven cells LEFT by
    /// [`Hero2Java::EQUIP_CELL_SHIFT`] px, so the character preview can stand in the middle
    /// of the panel and the avatar cell on its right (both host-drawn).
    ///
    /// ── THE FOUR SITES (`tools_re/hero2_dis.py`) ─────────────────────────────────
    /// ```text
    /// cc.a(II)V @0x13e918   the window painter
    ///   0x13e98a  adds r0,#0x28   ; x + 40   the six-cell loop's origin (col·14, row·14,
    ///                                         −7 in column 1 → cc.b(Graphics,h,cx,cy))
    ///   0x13e9fe  adds r2,#0x36   ; x + 54   the seventh cell (신발), cc.b(G,6,x+54,y+56)
    /// cc.b(II)V @0x13ed6c   the cursor frame (11 x 11, `k.a(IIII)V`) — its OWN copy of both
    ///   0x13edaa  adds r1,#0x28   ; x + 40
    ///   0x13edea  adds r0,#0x36   ; x + 54
    /// ```
    /// Measured: with only the painter's two moved, the cells went left and the cursor frame
    /// stayed where they had been (probe, f=830..935). All four are `adds Rdn,#imm8` (T2) —
    /// any value 0..255 — so the shift has no ceiling here, unlike #14's 3-bit site. The
    /// rest of the painter (frame, ATK/DEF/HP line, the selected record's name box at
    /// `y + 0x5d`) and the popup (centred on the panel) do not move.
    pub fn equip_cell_shift(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            // (site, as shipped, patched) — 0x28 − 32 = 0x08, 0x36 − 32 = 0x16.
            Self::Hero2Ktf => &[
                (0x0013_e98a, 0x3028, 0x3008),
                (0x0013_e9fe, 0x3236, 0x3216),
                (0x0013_edaa, 0x3128, 0x3108),
                (0x0013_edea, 0x3036, 0x3016),
            ],
        }
    }

    /// hero2's SKILL-POINT curve, as `(site, as shipped, patched)` halfwords — empty on
    /// every other build. User (2026-09-23): *"레벨 20까지는 스킬포인트 1씩 상승 21레벨부터
    /// 기존처럼 2레벨당 1 스킬포인트."*
    ///
    /// ── THE SHIPPED RULE, AND WHERE IT LIVES ────────────────────────────────────
    /// `a.l()V` @0x14fac8 is the level-up (`tools_re/find_field_uses.py a.a --desc B
    /// --writes` found it: the only three methods that write the level are new-character
    /// setup, save load, and this). Its body, per `tools_re/hero2_dis.py`:
    ///
    /// ```text
    /// while (exp >= next) {
    ///     exp -= next;                        0x14fb0c
    ///     if (level > 98) { …cap…; break; }   0x14fb24  cmp r0,#0x62
    ///     level = (byte)(level + 1);          0x14fb6e  a.a:B
    ///     a.m();
    ///     a.c:S (stat points)  += 3;          0x14fb88  adds r4,#3   ← unconditional
    ///     if (level % 2 == 1)                 0x14fba6  bl 0x1695b4, then cmp r0,#1
    ///         a.d:S (skill points) += 1;      0x14fc0a
    ///     a.d(); a.e(100); a.f(100); …box…
    /// }
    /// ```
    /// `0x1695b4` is the signed div/mod helper (abs both, then long division); with
    /// `r1 = 2` the `cmp r0,#1` after it reads "level is odd" — a point every two levels,
    /// exactly the rule the user described. The spend side agrees that a level costs one
    /// point: `af.a(BB)V` @0x10fae2 does `a.d -= 1`.
    ///
    /// ── WHY TEN BYTES IN PLACE AND NOT A TRAMPOLINE ─────────────────────────────
    /// The obvious plan was #13's: retarget the `bl` into a host block that answers
    /// `level <= 20 ? 1 : level & 1`. It cannot work — `alloc_guest` hands out java-heap
    /// addresses around 0x48000000 and a Thumb-1 `bl` reaches ±4 MB from 0x14fba6. But
    /// nothing needs a helper at all: the remainder is only ever compared to 1, so the
    /// whole test fits in the ten bytes the call already occupies.
    ///
    /// ```text
    ///            shipped                          patched
    /// 0x14fba4   2102  movs r1,#2                 2814  cmp  r0,#20     ; r0 = the NEW level
    /// 0x14fba6   f019 ┐ bl 0x1695b4               dd2a  ble  0x14fbfe   ; ≤20 → award
    /// 0x14fba8   fd05 ┘                           07c0  lsls r0,r0,#31  ; test bit 0
    /// 0x14fbaa   2801  cmp r0,#1                  d128  bne  0x14fbfe   ; odd → award
    /// 0x14fbac   d027  beq 0x14fbfe               46c0  nop             ; fall through = no award
    /// ```
    /// 0x14fbfe is the award block and 0x14fbae the common tail — both are where the
    /// shipped branches already went, so this changes the CONDITION and nothing else.
    ///
    /// ⛔ `r0` at 0x14fba4 is the **already incremented** level (0x14fb6e wrote it, 0x14fb9a
    /// read it back), so the threshold is the level being REACHED: 2..20 each award one,
    /// 21 and up keep the odd rule. ⛔ `lsls` destroys `r0` and the patch drops `r1 = 2`;
    /// neither survives the shipped code either — both paths out of here reload `r0` from
    /// a cell and `r1` from `r6` before using them.
    ///
    /// ── NOT RETROACTIVE, ON PURPOSE (user, 2026-09-23: "필요없다") ───────────────
    /// A LV 48 save does not get the ten points it "should" have. That decision is what
    /// makes this a pure code patch: nothing writes `a.d:S` here, so removing the patch
    /// restores the shipped game exactly, and no character field is changed behind the
    /// player's back on a build they might roll back.
    pub fn skill_point_curve(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            // (site, as shipped, patched) — a.l()V, the level-up's skill-point test
            Self::Hero2Ktf => &[
                (0x0014_fba4, 0x2102, 0x2814), // movs r1,#2      -> cmp  r0,#20
                (0x0014_fba6, 0xf019, 0xdd2a), // bl 0x1695b4 hi  -> ble  0x14fbfe
                (0x0014_fba8, 0xfd05, 0x07c0), // bl 0x1695b4 lo  -> lsls r0,r0,#31
                (0x0014_fbaa, 0x2801, 0xd128), // cmp r0,#1       -> bne  0x14fbfe
                (0x0014_fbac, 0xd027, 0x46c0), // beq 0x14fbfe    -> nop
            ],
        }
    }

    /// `j.c(Graphics,IIIZ)` only draws 1/3/7/9 below skill/beast icons from `ag.j`.
    /// Return before its prologue; guest execution slots are not host plate numbers.
    pub fn skill_hotkey_badges_off(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Hero2Ktf => &[(0x0015_ff70, 0xb5f0, 0x4770)],
            Self::Ktf | Self::Lgt | Self::Skt => &[],
        }
    }

    /// hero2's 경험치 배율 patch — see [`ExpRatePatch`]. `None` everywhere else: no other
    /// build dispatches through method-definition records, and no hero3 build has an
    /// `a.d(I)V` at all (its EXP path is the cash-shop divisor in `buffs.rs`).
    ///
    /// Measured 2026-09-23: `tools_re/find_field_uses.py a.c --desc I --writes` (the
    /// gain method), `tools_re/hero2_dis.py 'a.d(I)V'` (its body, 0x14fa64..0x14fac8),
    /// `tools_re/find_method_callers.py 'a.d(I)V'` (the two virtual callers),
    /// `tools_re/dump_hero2_java.py --json` (`def_fo` 571740 → def 0x18b95c; `ay`/`bl`/`bx`
    /// inherit it), `tools_re/hero2_dis.py 0x1674cc` / `0x16738c` (both helpers read the
    /// body out of the def at call time).
    pub fn exp_rate_patch(self) -> Option<ExpRatePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(ExpRatePatch { def: 0x0018_b95c, body: 0x0014_fa64 }),
        }
    }

    /// ★★ 2026-10-22 가디언 레벨: the level-up's wrapper — see [`GuardianLevelPatch`]. `None` everywhere else.
    /// Measured 2026-10-22: `tools_re/dump_hero2_java.py --json` (`a.l()V` def 0x18b978 · body 0x14fac8; `ay`/`bl`/`bx`
    /// `own: false`), the one caller `a.g()V` @0x14ddee (cell 0x17e448, virtual helper), `tools_re/find_field_uses.py
    /// a.f --desc I` (written only by `a.m()V`, read by `an.a(II)V` · `a.l()V` · `n.d()V`).
    pub fn guardian_level_patch(self) -> Option<GuardianLevelPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            // ★ P1 · P2 (same day, review): `ag.g(B)V` def 0x186364 · body 0x114110 (`find_method_callers.py`: one site,
            // `ag.l()V` @0x113f94) · `a.m()V` def 0x18b9cc (GearPatch's `stats` chain).
            Self::Hero2Ktf => Some(GuardianLevelPatch { def: 0x0018_b978, body: 0x0014_fac8, save_def: 0x0018_6364, save_body: 0x0011_4110, recalc_def: 0x0018_b9cc }),
        }
    }

    /// ★ 드랍률 (2026-10-16): the equipment-drop filter — see [`DropRatePatch`]. `None` everywhere else.
    /// Measured 2026-10-16: `tools_re/find_method_callers.py 'bf.a(BBBB)V'` (five sites, all in `bd.q()V`;
    /// the HELL/guardian block calls it through the same cell 0x179734), `tools_re/hero2_dis.py 'bf.a(BBBB)V'`
    /// (0x12d37c: `push {r4-r7,lr} · push {sl}` → the type and id at `[sp,#0x18]`/`[sp,#0x1c]` = the caller's
    /// `[sp]`/`[sp,#4]`), `tools_re/dump_hero2_java.py --json` (`def_fo` 0x883a8 → def 0x1883a8, #415).
    pub fn drop_rate_patch(self) -> Option<DropRatePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(DropRatePatch { def: 0x0018_83a8, body: 0x0012_d37c }),
        }
    }

    /// ★ 의상 아바타 A4: the round-clear counter — see [`ClearHookPatch`]. `None` everywhere
    /// else. Measured 2026-10-01: `tools_re/find_method_callers.py 'ag.y()V'` (one caller,
    /// `ag.g()V` @0x111946, direct), the jump table @0x16fc80 read back (entry 9 → 0x1118c2:
    /// `ag.e == 2 → ag.y()`), the call site's cell 0x176284 = `0x5400fd` → def index 126 →
    /// 0x18640c, its word 0 `0x8f51` + 0x10bf10 = 0x114e61 (`tools_re/hero2_dis.py 'ag.y()V'`).
    pub fn clear_hook_patch(self) -> Option<ClearHookPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(ClearHookPatch { def: 0x0018_640c, body: 0x0011_4e60, mode_slot: 0x0019_3c4c }),
        }
    }

    /// ★ 2026-10-02: the cooldown tick counter — see [`CooldownClockPatch`]. `None` everywhere
    /// else. Measured: `tools_re/find_method_callers.py 'ag.i()V'` (one caller, `a.g()V`
    /// @0x14de44, virtual), the def table scanned for word 0 `0x6199` (+ 0x10bf10 = 0x1120a9)
    /// → index 99 → 0x186118 (`tools_re/hero2_dis.py 'ag.i()V'` for the body).
    pub fn cooldown_clock_patch(self) -> Option<CooldownClockPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(CooldownClockPatch { def: 0x0018_6118, body: 0x0011_20a8 }),
        }
    }

    /// ★★ 2026-10-16: an item's own icon cell (엘릭서) — see [`ItemIconPatch`]. `None` everywhere else. Measured: `n.a(IIB)V`
    /// def 0x18d594 (the caller scan's `METHOD_DEFS + idx × 0x1c`), body 0x16388c (`tools_re/hero2_dis.py 'n.a(IIB)V'`).
    pub fn item_icon_patch(self) -> Option<ItemIconPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(ItemIconPatch { def: 0x0018_d594, body: 0x0016_388c }),
        }
    }

    /// ★ 2026-10-12: the slot screen opens on the slot last played — see [`SlotFocusPatch`]. `None` everywhere else.
    /// Measured: `be.a()V` def 0x187ef4 (`tools_re/dump_hero2_java.py`, word 0 = body | 1), its one caller
    /// `be.<init>(Lj;B)V` 0x1288ae (virtual, `find_method_callers.py`); the finder's `ag.a(B)Z` cell 0x179820 and its
    /// `bl 0x16916e` @0x1288fe.
    pub fn slot_focus_patch(self) -> Option<SlotFocusPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(SlotFocusPatch { def: 0x0018_7ef4, body: 0x0012_88d8, cell_has_save: 0x0017_9820, call_static: 0x0016_916e }),
        }
    }

    /// ★ 2026-10-11: the rogue's teleports refused in the corpses' scene — see [`TeleportGatePatch`]. `None` everywhere
    /// else. The def is the rogue usable check the accessory / 일풍격 / rogue blocks already chain (`usable_rogue`).
    pub fn teleport_gate_patch(self) -> Option<TeleportGatePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(TeleportGatePatch { def: 0x0018_74ac }),
        }
    }

    /// ★ 2026-10-05: a recast buff restarts its duration — see [`BuffRefreshPatch`]. `None`
    /// everywhere else. Measured: `tools_re/hero2_dis.py 'g.a(BI)V'` (body 0x15c09c),
    /// `dump_hero2_java.py` (`g.a(BI)V` def_fo 0x8c8ac, v=34, no override in any class), the def
    /// word read back from the image (`0x5018d` + 0x10bf10 = 0x15c09d).
    pub fn buff_refresh_patch(self) -> Option<BuffRefreshPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(BuffRefreshPatch {
                def: 0x0018_c8ac,
                body: 0x0015_c09c,
                beast_def: 0x0018_c8e4,
                beast_body: 0x0015_c19c,
                char_slot: 0x0019_3b5c,
                // l.<init>(BBI)V: 0x1616d0 `ldr r2,[sp,#0x10]` (the length) · 0x1616d4/6 the narrowing · 0x1616dc
                // `bl putfield av.a:I`.
                widen: [(0x0016_16d4, 0x0612, 0x46c0), (0x0016_16d6, 0x1612, 0x46c0)],
                skills_off: 0x138,
                beast_off: 0x134,
                beast_levels_off: 0x40,
            }),
        }
    }

    /// ★ 의상 아바타 A3: the EQUIPMENT window's cursor-frame switch — see [`EquipCursorPatch`].
    /// Measured 2026-09-30: `dump_hero2_java.py --class cc` (`b(II)V` body 0x13ed6c, v=30),
    /// the def record read back from the image (word 0 `0x32e5d` + 0x10bf10 = 0x13ed6d).
    pub fn equip_cursor_patch(self) -> Option<EquipCursorPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(EquipCursorPatch { def: 0x0018_a0f8, body: 0x0013_ed6c }),
        }
    }

    /// hero2's FIVE save slots — see [`SaveSlotsPatch`]. `None` everywhere else: hero3's
    /// slots are record-store records, not `ag`'s file-name tables.
    ///
    /// Measured 2026-09-27: `tools_re/dump_hero2_java.py --class ag` (the three statics),
    /// `tools_re/find_field_uses.py` on each (every reader), `tools_re/hero2_dis.py` on
    /// `ag.<clinit>()V` · `ag.k()V` · `be.<init>(Lj;B)V` · `be.a()V` · `be.a(II)Z` · `be.b()V`,
    /// the string pool walked from 0x181392 (entries 14..16 and 139..141 are the names), and
    /// an image-wide branch scan for anything landing inside the rewritten block.
    pub fn save_slots_patch(self) -> Option<SaveSlotsPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(SaveSlotsPatch {
                class: "ag",
                prog_names: 0x0019_39ec,
                char_names: 0x0019_39fc,
                previews: 0x0019_3a7c,
                table_sites: &[
                    (0x0011_2b0c, 0x2103, 0x2105), // ag.k: new Object[3] -> [5]
                    (0x0011_2b20, 0x2e02, 0x2e04), // ag.k: for i <= 2   -> <= 4
                ],
                screen_sites: &[
                    (0x0012_887e, 0x2303, 0x2305), // be.<init>: super(parent, 3) -> 5 rows
                    (0x0012_88e8, 0x2d02, 0x2d04), // be.a()V: first save among i <= 2 -> <= 4
                    // be.a(II)Z 0x1289c4: an empty slot under the cursor in 이어하기 steps on
                    // the way the key went, up to five looks (r4 = step, r5 = looks left).
                    (0x0012_89c4, 0x48ae, 0x4643), // mov   r3, r8          key
                    (0x0012_89c6, 0x1c39, 0x2401), // movs  r4, #1          RIGHT = +1
                    (0x0012_89c8, 0x4450, 0x2b05), // cmp   r3, #5
                    (0x0012_89ca, 0xf03f, 0xd007), // beq   0x1289dc
                    (0x0012_89cc, 0xfb6b, 0x2e36), // cmp   r6, #'6'
                    (0x0012_89ce, 0x1c05, 0xd005), // beq   0x1289dc
                    (0x0012_89d0, 0x48ac, 0x2404), // movs  r4, #4          LEFT = +4 (−1 mod 5)
                    (0x0012_89d2, 0x1c29, 0x2b02), // cmp   r3, #2
                    (0x0012_89d4, 0x4450, 0xd002), // beq   0x1289dc
                    (0x0012_89d6, 0xf040, 0x2e34), // cmp   r6, #'4'
                    (0x0012_89d8, 0xfbca, 0xd000), // beq   0x1289dc
                    (0x0012_89da, 0x0600, 0x2400), // movs  r4, #0          other key: look only
                    (0x0012_89dc, 0x2800, 0x2505), // movs  r5, #5
                    (0x0012_89de, 0xd1ea, 0x6838), // ldr   r0, [r7]        this.fields
                    (0x0012_89e0, 0x4643, 0x68c1), // ldr   r1, [r0, #0xc]  j.h
                    (0x0012_89e2, 0x2b05, 0x48a8), // ldr   r0, [pc, #0x2a0] the ag.a(B)Z cell (0x128c84, the shipped call's own literal)
                    (0x0012_89e4, 0xd03b, 0x4450), // add   r0, sl
                    (0x0012_89e6, 0x2e36, 0xf040), // bl    0x16916e        ag.a(B)Z(h): a save there?
                    (0x0012_89e8, 0xd039, 0xfbc2), //
                    (0x0012_89ea, 0x2b02, 0x0600), // lsls  r0, r0, #24
                    (0x0012_89ec, 0xd001, 0xd10b), // bne   0x128a06        yes: done
                    (0x0012_89ee, 0x2e34, 0x2c00), // cmp   r4, #0
                    (0x0012_89f0, 0xd1e1, 0xd009), // beq   0x128a06        not a direction: leave it
                    (0x0012_89f2, 0x48a5, 0x3d01), // subs  r5, #1
                    (0x0012_89f4, 0x1c39, 0xd007), // beq   0x128a06        five looks: stop
                    (0x0012_89f6, 0x4450, 0x6838), // ldr   r0, [r7]
                    (0x0012_89f8, 0xf03f, 0x68c1), // ldr   r1, [r0, #0xc]
                    (0x0012_89fa, 0xfb54, 0x1909), // adds  r1, r1, r4
                    (0x0012_89fc, 0x1c04, 0x2905), // cmp   r1, #5
                    (0x0012_89fe, 0x3402, 0xdb00), // blt   0x128a02
                    (0x0012_8a00, 0x1c20, 0x3905), // subs  r1, #5
                    (0x0012_8a02, 0x2103, 0x60c1), // str   r1, [r0, #0xc]  j.h = (h + step) mod 5
                    (0x0012_8a04, 0xf040, 0xe7eb), // b     0x1289de
                    (0x0012_8a06, 0xfdd6, 0xe7d6), // b     0x1289b6        return true (the shipped exit)
                    // be.b()V: 새로하기 → class pick keeps the window, and its rows are the
                    // three classes — j.h ← 0 (as shipped) and j.i ← 3 (the new half).
                    (0x0012_8df8, 0x4808, 0x6820), // ldr   r0, [r4]        this.fields
                    (0x0012_8dfa, 0x1c21, 0x2200), // movs  r2, #0
                    (0x0012_8dfc, 0x4450, 0x60c2), // str   r2, [r0, #0xc]  j.h ← 0
                    (0x0012_8dfe, 0x2200, 0x2203), // movs  r2, #3
                    (0x0012_8e00, 0xf03f, 0x6102), // str   r2, [r0, #0x10] j.i ← 3
                    (0x0012_8e02, 0xf900, 0x46c0), // nop
                ],
            }),
        }
    }

    /// hero2's COMBO SP, as `(site, as shipped, patched)` — empty on every other build.
    /// User (2026-09-23): *"콤보 2,3,4,5 단 사용시 sp 소모에서 획득으로 변경"*.
    ///
    /// ── WHAT THE COMBO IS, IN THE GAME'S OWN FIELDS ─────────────────────────────
    /// Found with `tools_re/find_field_uses.py a.b --desc I --writes` (who writes SP) and
    /// `tools_re/find_method_callers.py` (who calls them). The chain:
    ///
    /// | | |
    /// |---|---|
    /// | `a.i:B` (abs +0xb0) | the COMBO STAGE. `a.k()V` @0x14f464 advances it (`a.i += 1` at 0x14f608) and stops when `a.o[a.i + 1] == -1` |
    /// | `a.o:[B` (abs +0xac) | the chain table, `-1` past the end |
    /// | `a.i:[B` @0x19455c | **`[4, 3, 5]`** — how far the chain goes per class: 버서커 4 · 나이트 3 · **로그 5**. That 5 is the "5단" in the request |
    /// | `a.j:I` (abs +0xb8) | the SP the hit costs. `ay.c()V` / `bl.c()V` set it per job |
    /// | `a.f()Z` @0x14f6b8 | the gate: enough SP? then SPEND and return true |
    /// | `a.a(I)V` @0x14f8c4 | `sp += delta`, **clamped at `a.e:I`** — the game's own ceiling |
    ///
    /// The game says the same thing in its own text: `t/sk0.tdf[17]` 「근력단련 — 콤보
    /// 공격시 SP 소모량 감소」 is a passive on exactly this cost.
    ///
    /// ── THE ONE HALFWORD ────────────────────────────────────────────────────────
    /// `a.f()Z` loads the cost into `r7` at the top and, at its single `a.a(I)V` call,
    /// NEGATES it:
    /// ```text
    /// 0x14f7e2  ldr  r0, [pc, #0xc4]   ; a.a(I)V
    /// 0x14f7e4  rsbs r5, r7, #0        ; r5 = -cost        <-- THE SITE (427d)
    /// 0x14f7ea  adds r2, r5, #0
    /// 0x14f7ec  bl   0x16917c          ; sp += r2
    /// ```
    /// `adds r5, r7, #0` (1c3d) is the same register move without the negation, so the
    /// hit ADDS the cost instead of subtracting it.
    ///
    /// ── ⛔ BUT SKILLS GO THROUGH THE SAME CALL (2026-09-24, user: "스킬사용시 SP소모가
    /// 안되고있음") ─────────────────────────────────────────────────────────────────
    /// The first version stopped at the halfword above, reading the branch at 0x14f7f6
    /// as "the chain is at its class limit". It is not. `a.o[a.i]` is compared with
    /// `L = a.i:[B[class]` at 0x14f7de, and the entries the chain holds are `a.p`
    /// indices: `0..L` are the basic hits, `L..` are the class SKILLS — `a.d(B)Z`
    /// @0x14f154 arms skill `k` by writing `a.o[a.i + 1] ← k + L − 1` (0x14f29c..0x14f2aa),
    /// and `ag.c(B)V` @0x110a50 (what the 1/3/7/9 keys reach for a quickbar value 1..9)
    /// calls exactly that. So `a.o[a.i] >= L` at 0x14f7e0 IS a skill: the branch starts
    /// its cooldown (`ag.a(IZ)V(entry − L + 1, 0)` at 0x14f828 — the same `a.o[a.i] − L + 1`
    /// that is the skill's number) and then `b 0x14f7e2` into the ONE `a.a(I)V` call the
    /// basic hits use. One flipped halfword therefore flipped every skill's cost too:
    /// measured on the LV26 knight, a class skill ADDED its SP (or, at full SP, did
    /// nothing visible — the report).
    ///
    /// The two paths reach 0x14f7e2 with the cost in `r7` and nothing else in common, so
    /// the skill path negates `r7` before it jumps. Two dead-ish halfwords in that path
    /// carry it: `0x14f80c adds r5,r0,#0 · 0x14f80e adds r1,r5,#0` only feed `a.i` into
    /// `r1` for the array read at 0x14f812 (`r5` is overwritten at 0x14f7e4 before anyone
    /// reads it), so they become `adds r1,r0,#0 · rsbs r7,r7,#0`. `r7` is callee-saved and
    /// the function returns right after the call, so nobody sees the negated copy. Result:
    ///
    /// ```text
    /// basic hit  (a.o[a.i] <  L): 0x14f7e0 falls through → r5 = +cost → SP GAINS   (#16)
    /// class skill(a.o[a.i] >= L): 0x14f7f6 … r7 = −cost … b 0x14f7e2 → r5 = −cost → SP PAYS (as shipped)
    /// ```
    /// Skill 1 of a class (`a.o[a.i] == L`, e.g. the rogue's 「일격」) pays `a.j` like a
    /// hit — that is the shipped rule too (`0x14f71a cmp r4,r8 ; ble` reads the table
    /// cost only for `> L`). Measured (probe, knight LV26, `--h2-quickbar` seating
    /// class skill 2): shipped 222 → 212 · first version 222 → 228 (gain) · this 222 → 212.
    ///
    /// ⛔ **Do not clamp this in the host.** `a.a(I)V` already refuses to go past
    /// `a.e:I`: it writes, re-reads, and rewrites the maximum when it overshot. That is
    /// why turning a subtraction into an addition cannot put `410 / 390` into a save —
    /// the plan's worry about the ceiling is answered by the game, not by us.
    pub fn combo_sp_gain(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                // a.f()Z, the shared a.a(I)V call: `rsbs r5,r7,#0` -> `adds r5,r7,#0` (basic hits gain)
                (0x0014_f7e4, 0x427d, 0x1c3d),
                // a.f()Z, the skill branch: `adds r5,r0,#0` -> `adds r1,r0,#0` (r1 = a.i, as before)
                (0x0014_f80c, 0x1c05, 0x1c01),
                // a.f()Z, the skill branch: `adds r1,r5,#0` -> `rsbs r7,r7,#0` (skills pay)
                (0x0014_f80e, 0x1c29, 0x427f),
            ],
        }
    }

    /// hero2: 버서커 「근력단련」 no longer cuts the combo's SP (2026-09-26). `(site, as
    /// shipped, patched)`. The skill's other half — what it does INSTEAD, +3 % skill damage a
    /// level — is the berserker's [`SkillPowerPatch`] block; the text is the asset's
    /// (`make_hero2_asset.py` rewrites `t/sk0.tdf[17]` 「콤보 공격시 $SP@ 소모량 감소」 →
    /// 「스킬공격력 증가」). User: *"콤보스킬이 sp를 소모하지 않으면서 버서커의
    /// 근력단련스킬이 리뉴얼이 필요할거같아"* → *"스킬데미지 증가로 바꿀수도있어?"*.
    ///
    /// ── WHAT IT DID ───────────────────────────────────────────────────────────────────
    /// `a.j:I` (abs +0xb8) — the SP one basic hit is worth — is written by three methods
    /// (`find_field_uses.py a.j --desc I`): the stat recompute `a.m()V` sets it to
    /// `max-attack / 5` from the weapon (0x14ff20), calls the job's `c()V` (0x1500fe) — the
    /// rogue's `ay.c()V` and the berserker's `bl.c()V` adjust it, the knight's does not —
    /// and clamps it to ≥ 1 after (0x150338). The only other reader is the combo gate
    /// `a.f()Z`. The berserker's `bl.c()V` @0x13358c:
    /// ```text
    /// 0x1336c4  ble  0x133730          ; a.p[12] <= 0 → r4 = 0 (movs r4,#0 · b 0x1336de)
    /// 0x1336d8  asrs r4, r0, #0x18     ; r4 = a.p[12]   (근력단련 = sk0 스킬 8, 4 + 8)
    /// 0x1336da  movs r0, #7            <-- THE SITE (2007)
    /// 0x1336dc  muls r4, r0, r4        ; r4 = 7 × level
    /// 0x1336de  ldr  r0, [pc, #0xa0]   ; a.j cell   (the level-0 path joins here)
    /// 0x1336e0  subs r5, r5, r4        ; a.j − 7 × level
    /// 0x1336e8  bl   0x168004          ; a.j ←
    /// ```
    /// Shipped, that was 「콤보 공격시 SP 소모량 감소」. Since #16 the hit GAINS `a.j`
    /// ([`Self::combo_sp_gain`]), so the same subtraction cut the berserker's SP income:
    /// level 3 left 1 SP a hit up to LV 38 (weapon max 105 at LV 30: 21 → 1). `movs r0,#0`
    /// keeps the instruction and its register and makes the product 0 — `a.j` is the
    /// weapon's value for every level. `r0` is reloaded at 0x1336de before anyone reads it.
    /// ⚠ Skill 1 (휘두르기) pays `a.j` too (the shipped `ble` at 0x14f71c, §8.8), so its
    /// cost rises back to the weapon's value with it — the ratio 1 hit = 1 휘두르기 is kept.
    pub fn strength_training_sp_off(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_36da, 0x2007, 0x2000), // bl.c()V: movs r0,#7 -> movs r0,#0 (a.j − 0 × 근력단련)
            ],
        }
    }

    /// hero2: 버서커 「반격태세」 counters with the character's OWN ATTACK (2026-09-26).
    /// `(site, as shipped, patched)`. User: *"버서커의 반격태세 스킬이 캐릭터의 공격력에 비례해서
    /// 반격데미지가 증가했으면좋겠어 지금은 고정값인거같아"* → *"그냥 반격시 나의 공격력만큼
    /// 반격데미지를 입히면돼."*
    ///
    /// ── HOW THE COUNTER WORKS (read 2026-09-26) ─────────────────────────────────────
    /// Casting it (`bl.a()Z`, the berserker's action driver, jump-table entry 8 = 0x13309c)
    /// sets the stance flag `a.o:Z` (abs +0x108) at animation frame 10, and — dead — stores
    /// `lv·14 + 20` into `a.p:I` (nobody reads that field: `find_field_uses.py a.p --desc I`
    /// = three puts). While the flag is up, the damage-taken routine `a.a(Lbd;SB)V` answers
    /// every monster hit with the monster's own hit routine run BACKWARDS (0x152046):
    /// ```text
    /// bd.a(La;BIZBBBB)Z(this = the monster, attacker = the character, kind 8, ATTACK 0, …, 22)
    /// ```
    /// — attack 0, so `0 − defence/2` floors at 0, and the class multiplier
    /// `bl.a(BILbd;)I` case 8 (jump table @0x1705e4, entry 8 = 0x133990) ignores that `d`
    /// and returns the flat `a.p[8]·14 + 20`: 34..90 at any level, the shipped counter.
    ///
    /// ── THE PATCH: case 8 returns `this.a()I` × the level factor ─────────────────────
    /// `a.a()I` @0x15109c is the game's own attack number — `a.a(B)I(4)` = the STR share
    /// `a.h:I` + the weapon's roll `a.a(B)S`, doubled in 분노태세, with the combo-stage bonus
    /// — the value `bl.h(B)Z` and `bl.a(BI)Z` hand the hit routine for every other hit.
    /// Case 8 already starts with a cell load and a helper call on `this` (`sb`), so it
    /// becomes that call, and then (same evening, user *"너가말한대로 적용하고"* — 100 % at level
    /// 1, +10 % a level) the result is scaled by the factor the berserker's
    /// [`SkillPowerPatch`] block hands in as `d` (`fp` keeps the body's `d` across the call):
    /// ```text
    /// 0x133990  ldr  r0, [pc, #0xdc]    ; literal @0x133a70: a.p's cell -> a.a()I's cell
    /// 0x133992  mov  r1, sb             ; this                           (unchanged)
    /// 0x133994  add  r0, sl             ;                                (unchanged)
    /// 0x133996  bl   0x1680a4 -> bl 0x16917c   ; field get -> VIRTUAL call a.a()I
    /// 0x13399a  mov  r1, r8        -> mov  r1, fp        ; the factor (×2048)
    /// 0x13399c  bl 0x1689e4 (hi)   -> cmp  r1, #255
    /// 0x13399e  (lo)               -> bhi  0x1339a4
    /// 0x1339a0  lsls r0, r0, #24   -> movs r1, #1        ; no block (d = the counter's 0):
    /// 0x1339a2  asrs r4, r0, #24   -> lsls r1, r1, #11   ;   exactly 100 %
    /// 0x1339a4  movs r5, #14       -> muls r0, r1        ; attack × factor
    /// 0x1339a6  muls r4, r5, r4    -> asrs r6, r0, #11   ; / 2048
    /// 0x1339a8  adds r6, r4, #0    -> b    0x133950      ; the common return: r0 = r6
    /// ```
    /// The literal is read by this one `ldr` (every pc-relative load that could reach it
    /// scanned) and its value occurs nowhere else in the image; the new value is the cell
    /// `bl.h(B)Z` already calls `a.a()I` through (0x17ac34) — a resolved cell holds the
    /// method's definition, the same for every site. Only its LOW halfword differs
    /// (0xfffe_50a4 -> 0xfffe_4f90). The input `d` is only the factor now, so the ×1.5 and
    /// 근력단련 of [`SkillPowerPatch`] do not touch the counter (its prefix jumps past them).
    /// `a.a()I · 2870` stays far inside 32 bits. 0x1339aa/ac (`adds r6,#20 · b`) stay, dead.
    pub fn counter_stance_attack(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_3996, 0xf034, 0xf035), // bl.a(BILbd;)I case 8: bl 0x1680a4 -> bl 0x16917c (hi)
                (0x0013_3998, 0xfb85, 0xfbf1), //   (lo)
                (0x0013_399a, 0x4641, 0x4659), // mov r1, r8 -> mov r1, fp (the factor)
                (0x0013_399c, 0xf035, 0x29ff), // bl 0x1689e4 (hi) -> cmp r1, #255
                (0x0013_399e, 0xf822, 0xd801), //   (lo) -> bhi 0x1339a4
                (0x0013_39a0, 0x0600, 0x2101), // lsls r0,r0,#24 -> movs r1, #1
                (0x0013_39a2, 0x1604, 0x0000 | ((SkillPowerPatch::COUNTER_SHIFT as u16) << 6) | (1 << 3) | 1), // -> lsls r1, r1, #11
                (0x0013_39a4, 0x250e, 0x4348), // movs r5,#14 -> muls r0, r1
                (0x0013_39a6, 0x436c, 0x1000 | ((SkillPowerPatch::COUNTER_SHIFT as u16) << 6) | 6), // -> asrs r6, r0, #11
                (0x0013_39a8, 0x1c26, 0xe7d2), // adds r6,r4,#0 -> b 0x133950 (return r6)
                (0x0013_3a70, 0x50a4, 0x4f90), // the ldr's literal (low half): a.p cell 0x17ad48 -> a.a()I cell 0x17ac34
            ],
        }
    }

    /// hero2: 버서커 「반격태세」 lasts 6 s at level 1 and 20 s at level 5 (2026-09-26, user:
    /// *"지속시간도 1레벨을 6초고 5렙찍으면 20초까지 지속가능하게 해줘"*). `(site, as shipped,
    /// patched)`.
    ///
    /// ── WHAT ENDS THE STANCE ──────────────────────────────────────────────────────────
    /// Casting (`bl.a()Z` entry 8, animation frame 10) sets `a.o:Z` and registers buff 31 with
    /// `g.a(BI)V(31, 120)` — a `bm` (`av`) that `bm.a()V` ages one per game frame and marks
    /// done at its length (`av.b >= av.a:I`). `g.v()V` hands a done buff to the character's
    /// `a.a(Lav;)Z` @0x14e024, whose switch on the buff id clears `a.o:Z` (0x14e0d4 — the cell
    /// load and the `putfield` are a branch apart, so `find_field_uses.py` does not see that
    /// put; the pool scan does). Measured: the flag is up 600..900 and down by 1000. So the
    /// buff's LENGTH is the stance's length — 120 game frames = 6 s at 20 fps.
    ///
    /// ── THE PATCH: length = 70·lv + 50 ───────────────────────────────────────────────
    /// The cast already computes `lv·14 + 20` into `r4` for `a.p:I` — a field nothing reads
    /// (pool scan: three puts, no get) — and `r4` survives the `putfield` helper (it pushes
    /// r4..r6). So the two immediates become 70 and 50 and the buff call takes `r4`:
    /// ```text
    /// 0x1330cc  movs r3, #14   -> movs r3, #70
    /// 0x1330ce  asrs r4, r0, #24               ; lv            (unchanged)
    /// 0x1330d0  muls r4, r3, r4                ; lv × 70       (unchanged)
    /// 0x1330d4  adds r4, #20   -> adds r4, #50 ; 120 · 190 · 260 · 330 · 400
    /// 0x1330dc  bl   putfield                  ; a.p:I ← it    (still unread)
    /// 0x1330e8  movs r3, #120  -> adds r3, r4, #0   ; g.a(BI)V(31, length)
    /// ```
    /// = 6 · 9.5 · 13 · 16.5 · 20 s. The cooldown (130 frames after #17) is untouched: a
    /// re-cast while the stance holds finds buff 31 present (`g.a(B)Z`) and adds nothing.
    ///
    /// ★ 2026-10-06 (user: 버프 8종 「쿨타임 1분 · 지속 1분」 → 「만렙 지속 75초」 → 「전부 ×1.25」): the two
    /// immediates are **250 and 250** — `250·lv + 250` = 500 · 750 · 1000 · 1250 · 1500 frames = **25 · 37.5 · 50 ·
    /// 62.5 · 75 s**, with the cooldown 60 s ([`Self::buff_cooldown_minute`]). 75 s, not 60: the cooldown counter
    /// stands still on the field ticks the game spends in an action (measured: 16 ticks per 20 painted frames while
    /// attacking), the buff ages on every painted frame — so a 60-s buff on a 60-s cooldown went dark for up to
    /// 15 s a minute in a fight; 75 = 60 / 0.8. Every level is ×1.25 of the 20 → 60 s the first draft had.
    pub fn counter_stance_duration(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_30cc, 0x230e, 0x23fa), // bl.a()Z case 8: movs r3,#14 -> movs r3,#250
                (0x0013_30d4, 0x3414, 0x34fa), //   adds r4,#20 -> adds r4,#250 (r4 = 250·lv + 250)
                (0x0013_30e8, 0x2378, 0x1c23), //   movs r3,#120 -> adds r3,r4,#0 (the buff's length)
            ],
        }
    }

    /// ★ 2026-10-06 — the buff skills' cooldowns at ONE MINUTE (user: 분노태세 · 반격태세 · 신의가호 · 철괴 ·
    /// 맹독감염 · 빛의수호 · 빛의회복 → 「쿨타임 1분」; 얼음장막 left out, 민첩태세 left as it is). `(site, as
    /// shipped, patched)`. 신의가호 · 철괴 already were (`bx.a[4]` · `[5]` = 2400, [`Self::blessing_aura_rework`] ·
    /// [`Self::iron_wall_rework`]); 맹독감염's is [`VenomPatch`]'s `cooldown`. This set is the rest:
    ///
    /// | | where | shipped | patched |
    /// |---|---|---|---|
    /// | 분노태세 · 반격태세 | `bl.<clinit>` 0x133b92 / 0x133b9e: `bl.a[3]` and `[4]` both store `r4` | `movs r4,#130 ; lsls r4,#1` = 260 (130 frames after #17's ½ = 6.5 s) | `movs r4,#150 ; lsls r4,#4` = **2400** (½ → 1200 = **60 s**) |
    /// | 빛의수호 · 빛의회복 | the beast's cooldown `bn.a(B)I` @0x1354b8 — a switch on the slot's kind (jump table @0x170634, entries relative to it; NOT halved by #17) | kind 1 → 0x135510 `movs r0,#245` · kind 4 → 0x13551e `movs r0,#130 ; lsls r0,#1` = 260 | kind 4's case `movs r0,#150 ; lsls r0,#3` = **1200**, and kind 1's entry → that case (low halfword; the high one, `0xfffc`, is the same for both). 5 얼음장막's 230 (0x135524) is its own case — untouched |
    ///
    /// `r4` is read by `bl.a[3]`/`[4]` only (0x133bae · 0x133bb8; 사자후's `[6]` is its own `movs`,
    /// [`Self::roar_cooldown_up`]). ⛔ A `<clinit>` patch: it lands in the first patch pass, before a berserker is
    /// ever loaded — the table it builds is what `bl.a(I)S` and the host plate ring read. The beast's `bn.a:[S`
    /// is filled from `bn.a(B)I` by `bn.h()V` when the beast loads (`bn.b(Z)V`), after the patch pass too.
    pub fn buff_cooldown_minute(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_3b92, 0x2482, 0x2496), // bl.<clinit>: movs r4,#130 -> movs r4,#150
                (0x0013_3b9e, 0x0064, 0x0124), //              lsls r4,r4,#1 -> lsls r4,r4,#4 (bl.a[3] = bl.a[4] = 2400)
                (0x0013_551e, 0x2082, 0x2096), // bn.a(B)I kind 4: movs r0,#130 -> movs r0,#150
                (0x0013_5520, 0x0040, 0x00c0), //                  lsls r0,r0,#1 -> lsls r0,r0,#3 (1200)
                (0x0017_0638, 0x4edc, 0x4eea), // jump table [1] 빛의수호: 0x135510 (245) -> 0x13551e (1200)
            ],
        }
    }

    // ⛔ hero2's DROP WINDOW patch (#20, 2026-09-24, `drop_window_all`) was REMOVED on
    // 2026-09-25 — do not bring it back. It made the kill routine `bd.q()V` roll all five
    // `enm/enm` slots in every mode, but slots 3·4 are each enemy's HARDCORE/HELL gear
    // (items levelled for the mode's enemy level, 2·lv/5 + 40 or + 60), so in NORMAL a LV 2
    // 거미 dropped LV 36 gloves 10 % of the time (user: "2랩 3랩 몬스터를 잡았는데 40랩제
    // 아이템들이 쏟아지고있어"). The shipped window — NORMAL 0·1·2, HARDCORE/HELL 2·3·4 —
    // is the level rule; the asset plan (`make_hero2_asset.py` `plan_drops`) now places
    // items per window. HERO2_GUEST_RE.md §8.9.

    /// hero2's SKILL COOLDOWN, halved (#17, 2026-09-24). `(site, as shipped, patched)`.
    /// User (2026-09-24): 「데미지 ×1.5 + 쿨타임 절반」 — this is the second half; see
    /// [`Self::skill_power_patch`] for the first.
    ///
    /// Each job's cooldown lookup is `return <job>.a[i]` and nothing else (§8.2): the
    /// static `[S` read through the short-array helper, then sign-extended with
    /// `lsls r0,r0,#16 ; asrs r0,r0,#16`. Shifting right by ONE more (`asrs #17`) returns
    /// `a[i] >> 1` — every entry is a positive frame count (35..260), so that is exactly
    /// half, rounded down. Both callers see it: `ag.a(IZ)V` @0x1121ac starts the counter
    /// from it on a cast, and `n.d(BII)V` draws the cooldown bar as `left / total` — so
    /// the game's own bar stays right. ⛔ The HOST's plate ring reads the STATIC table
    /// directly (`h2_cooldown_table`), so it must halve too — `crate::SKILL_CD_HALVED` is
    /// published when these three land, and `items::h2_scan_quickbar` divides by it.
    ///
    /// ⛔ The three bodies are `ay` = 로그 · `bl` = 버서커 · `bx` = 나이트 — the
    /// alphabetical order is NOT the class order (factory `a.a(BSS)La;`: 0 `bl` · 1 `bx` ·
    /// 2 `ay`). All three are patched, so which class is loaded does not matter.
    pub fn skill_cooldown_half(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0012_0c2e, 0x1400, 0x1440), // ay.a(I)S (로그)   asrs r0,r0,#16 -> #17
                (0x0013_38e2, 0x1400, 0x1440), // bl.a(I)S (버서커) asrs r0,r0,#16 -> #17
                (0x0013_bd76, 0x1400, 0x1440), // bx.a(I)S (나이트) asrs r0,r0,#16 -> #17
            ],
        }
    }

    /// hero2: CANCEL on the SKILL window's PASSIVE-tab popup closes it instead of
    /// crashing (2026-09-24, a SHIPPED bug found from an S9 logcat). `(site, as shipped,
    /// patched)`.
    ///
    /// ── THE BUG ──────────────────────────────────────────────────────────────────
    /// The popup `d` answers CLR with `parent.a(kind, -1)` (`d.a(II)Z` @0x159c90). The
    /// SKILL window's callback `s.a(BB)V` @0x1481dc splits its PASSIVE tab (`s.b`) on
    /// `sel != 0`, so the -1 falls into the 스킬상세정보 branch and builds `cj(this, idx,
    /// false)` for the cell under the cursor. `cj.<init>` turns that into a class-skill
    /// table index `idx − a.i[class]` — NEGATIVE for a combo stage — and reads
    /// `sk{class}[2·that]`: ArrayIndexOutOfBounds, uncaught, worker dead.
    ///
    /// The ACTIVE tab (`0x148310`) and the BEAST window (`af.a(BB)V`) both treat a
    /// negative `sel` as "return"; only this one branch forgot the cancel code. It was
    /// out of reach until the level-1 skill reset: the combo popup `[포인트투자]` opens
    /// only for an UNLEARNED stage (combo max level is 1) while points are unspent
    /// (`bw.a:Z = a.d > 0`, `bw.b()V`), and a high-level save has neither.
    ///
    /// ── THE PATCH: a three-way split, sel > 0 detail · sel == 0 invest · sel < 0 return
    /// ```text
    /// 0x1482c2  mov  r3, sb          ; r3 = sel
    /// 0x1482c4  cmp  r3, #0
    /// 0x1482c6  bne  0x1482da   -> bgt  0x1482da      (detail only for a real row)
    /// 0x1482c8  ldr  r0, [pc, #0x114]                 (s.a(B)V — NOT moved)
    /// 0x1482ca  adds r1, r7, #0
    /// 0x1482cc  add  r0, sl
    /// 0x1482ce  mov  r2, r8
    /// 0x1482d0  adds r5, r7, #0 -> cmp  r3, #0        (r3 still = sel)
    /// 0x1482d2  mov  r4, r8     -> blt  0x1482b0      (cancel: the epilogue)
    /// 0x1482d4  bl   s.a(B)V                          (invest, sel == 0 only)
    /// ```
    /// The two replaced moves are dead: the call is followed by `b 0x1482b0`, whose
    /// `pop {r4-r7, pc}` overwrites both, and the dispatch helper reads only r0..r3.
    /// A PASSIVE-tab cancel on a real passive (whose index is ≥ the combo count) used to
    /// open its detail window; now it closes, as every other tab and window does.
    pub fn skill_popup_cancel(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0014_82c6, 0xd108, 0xdc08), // bne 0x1482da -> bgt 0x1482da
                (0x0014_82d0, 0x1c3d, 0x2b00), // adds r5,r7,#0 -> cmp r3,#0
                (0x0014_82d2, 0x4644, 0xdbed), // mov r4,r8 -> blt 0x1482b0
            ],
        }
    }

    /// ★★ 2026-10-16 (user: *"갯수가 있는 아이템을 구매하거나 판매할때 1에서 좌측이나 아래로 갯수를 내리면 99 및 최대 갯수로
    /// 변경되도록"*): the game's 개수 box `aq` WRAPS at 1 — LEFT or DOWN on 1 goes to the box's ceiling `aq.a:B` (the buy
    /// window's 99, the sell window's stack count, the warehouse split's count). `(site, as shipped, patched)`.
    /// HERO2_GUEST_RE.md §2.7.12 / §8.85.
    ///
    /// ── THE GAME (`aq.a(II)Z` @0x11dcf0, `j.h` = the number) ─────────────────────────────
    /// LEFT `ble 0x11dd4e` when `h ≤ 1` (return, nothing) · DOWN `ble 0x11dd4e` when `h ≤ 10` · UP's clamp ends at
    /// **0x11de20** = `h ← aq.a:B` (read the ceiling, `b 0x11dd8c` = the putfield of `j.h`) — the one "set to max" the
    /// method has, and both new branches land on it.
    /// ```text
    /// LEFT  0x11dda6  ble 0x11dd4e -> ble 0x11de20            (h ≤ 1 → max)
    /// DOWN  0x11ddca  adds r6,r0,#0 -> cmp r0,#1              (r0 = h from the getfield above; r6 is dead — every exit
    ///       0x11ddcc  cmp r0,#10    -> beq 0x11de20            pops it or overwrites it)
    ///       0x11ddce  ble 0x11dd4e  -> cmp r0,#10              (2..10: nothing, as shipped)
    ///       0x11ddd0  ldr r0,[pc]   -> ble 0x11dd4e
    ///       0x11ddd2  mov r1,r8     -> adds r4,r0,#0           (the shipped path read `j.h` a second time — r0 has it)
    ///       0x11ddd4  add r0,sl     -> subs r4,#10
    ///       0x11ddd6  bl (1st half) -> b 0x11ddde              (the shipped `lsls r3,r4,#24` … putfield; 0x11ddd8..dd
    ///                                                           are unreachable from here on)
    /// ```
    ///
    /// ★★ The other way round (2026-10-16, user: *"99에서 위 or 우측 버튼누르면 다시 1되는것도 마찬가지로"*): UP or RIGHT
    /// on the ceiling goes to **1**. The game had no "set to 1", so both paths now compute the number in r4 and join the
    /// RIGHT path's shipped tail **0x11dd84** = `lsls r3,r4,#24 · ldr r0,=j.h · mov r6,r8 · asrs · putfield · return`.
    /// ```text
    /// RIGHT (r6 = h, r0 = ceiling, `cmp r6,r0` at 0x11dd72 as shipped)
    ///       0x11dd74  bge 0x11dd4e  -> bge 0x11dd7a            (h ≥ ceiling → 1; shipped: return)
    ///       0x11dd76  ldr r0,[pc]   -> adds r4,r6,#1           (the shipped path read `j.h` a second time — r6 has it)
    ///       0x11dd78  mov r1,r8     -> b 0x11dd84
    ///       0x11dd7a  add r0,sl     -> movs r4,#1
    ///       0x11dd7c  bl (1st half) -> b 0x11dd84              (0x11dd7e..83 unreachable)
    /// UP    (r6 = h, r0 = ceiling, `cmp r6,r0` at 0x11ddfe as shipped)
    ///       0x11de00  blt 0x11de32  -> bge 0x11de0c            (h ≥ ceiling → 1; shipped: re-read, clamp, return)
    ///       0x11de02  ldr r0,[pc]   -> adds r4,r6,#0
    ///       0x11de04  mov r1,r8     -> adds r4,#10
    ///       0x11de06  add r0,sl     -> cmp r4,r0
    ///       0x11de08  bl (1st half) -> ble 0x11dd84            (h + 10 ≤ ceiling → write it)
    ///       0x11de0a  bl (2nd half) -> b 0x11de20              (over → the shipped `h ← aq.a:B`: min(h + 10, ceiling)
    ///       0x11de0c  adds r6,r0,#0 -> movs r4,#1                as shipped)
    ///       0x11de0e  ldr r0,[pc]   -> b 0x11dd84
    /// ```
    /// The shipped +10 path (0x11de32..0x11de50: write h + 10, then `b 0x11de02` to re-read and clamp) is reached only
    /// from 0x11de00 and is dead from here on, so is 0x11de10..0x11de1f; 0x11de20 (the clamp the LEFT / DOWN wraps land
    /// on) stays as shipped.
    pub fn qty_box_wrap(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                // LEFT / DOWN on 1 → the ceiling
                (0x0011_dda6, 0xddd2, 0xdd3b),
                (0x0011_ddca, 0x1c06, 0x2801),
                (0x0011_ddcc, 0x280a, 0xd028),
                (0x0011_ddce, 0xddbe, 0x280a),
                (0x0011_ddd0, 0x483a, 0xddbd),
                (0x0011_ddd2, 0x4641, 0x1c04),
                (0x0011_ddd4, 0x4450, 0x3c0a),
                (0x0011_ddd6, 0xf04a, 0xe002),
                // RIGHT on the ceiling → 1
                (0x0011_dd74, 0xdaeb, 0xda01),
                (0x0011_dd76, 0x484b, 0x1c74),
                (0x0011_dd78, 0x4641, 0xe004),
                (0x0011_dd7a, 0x4450, 0x2401),
                (0x0011_dd7c, 0xf04a, 0xe002),
                // UP on the ceiling → 1
                (0x0011_de00, 0xdb17, 0xda04),
                (0x0011_de02, 0x4832, 0x1c34),
                (0x0011_de04, 0x4641, 0x340a),
                (0x0011_de06, 0x4450, 0x4284),
                (0x0011_de08, 0xf04a, 0xddbc),
                (0x0011_de0a, 0xf94c, 0xe009),
                (0x0011_de0c, 0x1c06, 0x2401),
                (0x0011_de0e, 0x4830, 0xe7b9),
            ],
        }
    }

    /// ★★ hero2: the ALCHEMIST's 아이템 조합 on 127-cell bags (2026-10-09, user: *"재료가방창도 127칸으로 만들꺼고. 추가로
    /// 신규아이템 조합서들을 구성할꺼야"*). `(site, as shipped, patched)`. HERO2_GUEST_RE.md §8.60.
    ///
    /// ── THE GAME ─────────────────────────────────────────────────────────────────────
    /// The crafting window `aj` (opened by the alchemist's menu `aw`, state 8) lists the 재료가방's recipe books
    /// (`bq.b(BB)[B(23, −1)` → the picker `k`), and for the chosen one `aj.c()V` @0x116b4c looks up each of its three
    /// materials: a material type (`bq.a(B)Z`) in `ag.b` and the rest in the inventory `ag.a`, both through
    /// `bq.b(BB)B` (the first record of `(type, id)` that is NOT worn), into `aj.i:[B` — the inventory index as it is,
    /// the 재료가방 index **+ 50** (`adds r5,#0x32` @0x116c44), −1 = missing. Nothing decodes the +50: `aj.i` is read
    /// by `aj.a()Z` (craftable = no −1) and the painter `aj.e(Graphics,III)V` (the ✓ is `aj.i[k] ≥ 0`) only.
    /// On a 127-cell 재료가방 an index from 78 on is 128+ — a NEGATIVE byte: the material shows as missing although
    /// `aj.a()Z` crafts. The +50 goes; every index is then 0..126, a plain non-negative byte.
    pub fn craft_material_mark(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0011_6c44, 0x3532, 0x3500), // aj.c()V: adds r5,#0x32 -> adds r5,#0 (the 재료가방 index as it is)
            ],
        }
    }

    /// ★★ hero2: the item PICKER `k`'s grid 7 px right (2026-10-09), so the page tabs of a picker with more than 28
    /// entries ([`Hero2Java::BAG_PAGE`]) stand inside its left margin like the inventory's (#14,
    /// [`Self::inventory_grid_shift`]). `k` = the 7 × 4 picker four windows open over a `[B` list of bag indices —
    /// the alchemist's recipe list (`aj.a()V`), its 능력 부여 target/item (`x.a(II)Z`), the EQUIPMENT window's
    /// candidates (`cc.a(BB)V`) and `ap.a(II)Z` — `k.<init>(Lj;B[B[BZ)V` @0x160fe8 = `super(parent, kind, 28, 4, 7)`.
    /// Its painter draws the cells at `x + 7 + col·15`, `y + 23 + row·15` and its cursor the same; the two `+7`s:
    /// ```text
    /// k.a(II)V @0x16134e  adds r4,#7          -> adds r4,#14        (the cell: x + 14 + col·15)
    /// k.b(II)V @0x161648  adds r6,r2,#0       -> adds r6,r2,#7      (the cursor's x, then its own `adds r0,r6,#7`)
    /// ```
    /// (`k.b`'s own `adds r0,r6,#7` is a 3-bit immediate already at 7 — the move goes on the copy before it, as
    /// #14's `q.b` did.) The last column ends at `x + 118`, inside the 120-wide panel. Nothing else reads the cells'
    /// place: `k` has no touch arm in the host (`h2_tap`), and its record is `k.a(I)Lch;` of the list.
    pub fn picker_grid_shift(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0016_134e, 0x3407, 0x340e), // k.a(II)V: adds r4,#7 -> adds r4,#14
                (0x0016_1648, 0x1c16, 0x1dd6), // k.b(II)V: adds r6,r2,#0 -> adds r6,r2,#7
            ],
        }
    }

    /// ★★ hero2: crafting never eats a WORN item (2026-10-09). `(cell, as shipped unresolved, as the runtime resolves
    /// it, patched)` — the cells are words the game itself rewrites on first use (an odd index → the definition's
    /// address, HERO2_GUEST_RE.md §1), so both of their shipped states are accepted.
    ///
    /// ── THE GAME'S BUG ───────────────────────────────────────────────────────────────
    /// `aj.c()V` checks the materials with `bq.b(BB)B` @0x1375e4 — the first `(type, id)` record that is not worn
    /// (`aa.f ≥ 0` is skipped) — but `aj.d()V` @0x116d90 CONSUMES them with `bq.a(BBB)V` @0x136c2c, whose finder is
    /// `bq.a(BB)B` @0x1374f4: the first `(type, id)` record, worn or not. With one copy worn and a spare in the bag the
    /// check passes on the spare and the craft takes whichever comes first — the worn one, as a rule (it was picked
    /// up first): it leaves the Vector while the character still wears it, and after the next load the slot is
    /// empty. The new guardian recipes take 종결 gear and accessories — exactly what a player wears — so this is the
    /// recipe set's own precondition.
    ///
    /// ── THE PATCH ────────────────────────────────────────────────────────────────────
    /// `bq.a(BBB)V`'s two finder calls are virtual (`bl 0x16917c`) through the cells 0x17b404 (the first find) and
    /// 0x17b418 (the loop's next find): both → `bq.b(BB)B`'s definition 0x189410 (vtable 36, beside `bq.a(BB)B`'s 35
    /// at 0x1893f4). The callers of `bq.a(BBB)V` (census): `aj.d()V` ×3 (the 재료가방 materials, the inventory ones,
    /// the recipe book itself) · `bb.q()V` (the script's take-item, `31 t i · 16 −n`) · `x.c()V` (능력 부여: the orb /
    /// seal / crystal) · `ck.d()V` (제련: 제련석). Only an `aa` (gear) record is ever skipped, so for everything but gear
    /// the two finders are one; the only script that takes gear is 맵 98 「무사의 장갑」 (강철사슬장갑 ×1) — now it
    /// takes the spare, and nothing (the quest still completes) when the only copy is worn.
    pub fn craft_worn_skip(self) -> &'static [(u32, u32, u32, u32)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0017_b404, 0x0225_0469, 0x0018_93f4, 0x0018_9410), // bq.a(BBB)V's first find -> bq.b(BB)B
                (0x0017_b418, 0x0225_0469, 0x0018_93f4, 0x0018_9410), // bq.a(BBB)V's next find  -> bq.b(BB)B
            ],
        }
    }

    /// hero2: no more HONOUR penalty on weapons (2026-09-25). `(site, as shipped, patched)`.
    /// User: *"2,3편은 명예등급이 낮으면 장비 능력이 감소되는 기능이 있는데 이 명예등급이
    /// 네트워크로만 올릴수있었던거라 현재 버전에는 있으면 안되는 기능이야 … 장비 능력 감소되는
    /// 기능은 제거해야해. 그리고 만약 명예등급으로 착용제한 같은게 있다면 제거해야해."*
    ///
    /// ── WHAT THE PENALTY IS (HERO2_GUEST_RE.md §8.19) ─────────────────────────────
    /// The weapon class `bo` (`bo` → `bu` → `aa` → `ch`) answers its attack through two
    /// getters, and they are the only code in the image that turns honour into a number:
    /// ```text
    /// bo.c()S @0x1360c4 (max)            bo.d()S @0x136164 (min)
    ///   v = bo.a()S = c + refine×3/2       v = bo.b()S = b + refine×3/2
    ///   h = ag.a:La;.g:B                   ; the player's 명예 (CHAR+0x60)
    ///   k = this.bu.k:B                    ; the weapon's 요구 명예 (itm body[15])
    ///   cmp h, k ; blt  -> v*(10 − k + h)/10         <-- THE SITES 0x136102 / 0x1361a2
    ///   return v
    /// ```
    /// Honour only ever rose through network 대전 (`y.a([B)Z`, the login packet, is its
    /// one writer besides character creation and the save loader), so offline every weapon
    /// with `k > 0` lost `(k − h)×10 %` of its attack — 요구 10 meant attack 0. Each `blt`
    /// becomes the canonical Thumb `nop` (`mov r8, r8`): the getter returns `v` whatever
    /// the honour. Armour (plain `bu`) and accessories (`z`, which has no `bu.k` at all)
    /// never had the term, and the item-detail screen always drew the UNPENALISED
    /// `bo.a/bo.b`, so nothing the player reads changes — only the fight.
    ///
    /// ── THERE IS NO EQUIP GATE TO REMOVE (census) ──────────────────────────────────
    /// `find_field_uses.py`: `bu.k:B` is read by these two getters, the item-detail painter
    /// `j.a(Graphics,II,ch,ZZZ)V` (it prints the requirement's rank NAME) and the item
    /// serialiser — nothing else. `a.g:B` by the two getters, the STATUS window
    /// (`an.<init>`, the player's rank name), the save serialisers and the login parser.
    /// The equip window `cc.a(BB)V` reads neither: its refusals are level (gm[69]
    /// 「요구레벨이 아닙니다.」) and heavy armour (gm[59]), and the archive has no honour
    /// refusal string at all (HERO2_ITEM_DB.md §0).
    pub fn honor_penalty_off(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_6102, 0xdb03, 0x46c0), // bo.c()S: blt 0x13610c -> nop (max attack in full)
                (0x0013_61a2, 0xdb03, 0x46c0), // bo.d()S: blt 0x1361ac -> nop (min attack in full)
            ],
        }
    }

    /// ★★ 2026-10-17 hero2: an item the player already owns takes its NAME, PRICE and 요구레벨 from the CURRENT item table when the
    /// save loads (user: *"아이템을 업데이트했을때 사용자들이 이미 가지고있는 아이템을 업데이트한 이름 및 설명, 옵션 등으로 자동으로
    /// 업데이트 되게는 안되는거야?"* → *"이름 가격 자동 갱신 진행하고 요구레벨도 표 따르게 해줘"*). `(site, as shipped, patched)`.
    ///
    /// ── HOW THE GAME LOADS AN ITEM (HERO2_GUEST_RE.md §8.96) ─────────────────────
    /// `ch.a([B)Lch;` @0x140ff4 (the bag's `bq.a([B)V` and the host warehouse alike) = the factory `ch.a(BBZZ)Lch;(type, id,
    /// false, false)` → `ch.a(ZZ)V` parses the itm row with the two flags FALSE → the saved overlay `rec.a([BI)I`. Flag 1 false
    /// is the save loader's alone — every other factory caller (12 sites: drops, shop, craft, script rewards, the bag's stack
    /// split…) passes 1. With it false the base parser `ch.a([BIZZ)I` SKIPS the row's price (`beq` 0x140d7c) and name
    /// (`bne` 0x140d96) — it reads only the description, which is why a description always follows the table — and the
    /// equipment parser `aa.a([BIZZ)I` returns before body[1..=3] (요구레벨 `aa.a` · 등급 `aa.b` · 세트 `aa.c`, `bne` 0x10d792).
    /// Then the loader `ch.a([BI)I` stores the SAVED name (0x14120a..0x1412a6) and price (0x14126e), `aa.a([BI)I` the saved
    /// level (0x10db90). `find_field_uses.py`: `ch.j` (name), `ch.d` (price) and `aa.a` (level) have exactly two writers each,
    /// the table parser and the loader — so a saved one is only ever a frozen copy of the table at the item's creation.
    ///
    /// ── THE PATCH ───────────────────────────────────────────────────────────────────
    /// The table parser reads them on the load path too, the loader skips its stores: price `beq` → nop (falls into the
    /// fresh path's price read, which returns to the name check) · name `bne` → `b` · equipment `bne` → `b` (with flag 2 also
    /// false — the loader's — the read path stops after the set byte and skips the same 11 bytes the early return did, so no
    /// random roll runs and the offset is the same 14) · loader name: `ble null` → `ble` past the name, the array allocation →
    /// `b` to the offset advance · loader price · level: the putfield `bl` → nop nop (the value is read and dropped — the
    /// stream offset is untouched). The grade and set the loader still stores as saved.
    ///
    /// ⛔ The equipment checksum `aa.e` (`ch.a(Lch;)B` @0x141430 → `bo.a()B` · `bu.a()B` · `z.a()B` · `aa.a()B`) reads the
    /// rolled stats, the refine level and the serial — not the name, price or level — so a refreshed item still verifies.
    /// The next save writes the table's values (`ch.a()[B` · `aa`'s serialiser write the record's fields). Rolled numbers
    /// (attack / defence / stats / random options / refine) stay the item's own.
    pub fn item_table_refresh(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0014_0d7c, 0xd000, 0x46c0), // ch.a([BIZZ)I: beq 0x140d80 (flag 1 false: skip the price) -> nop = read it
                (0x0014_0d96, 0xd139, 0xe039), // ch.a([BIZZ)I: bne 0x140e0c (flag 1: read the name) -> b 0x140e0c
                (0x0010_d792, 0xd10a, 0xe00a), // aa.a([BIZZ)I: bne 0x10d7aa (flag 1: read level · grade · set) -> b 0x10d7aa
                (0x0014_120c, 0xdd47, 0xdd24), // ch.a([BI)I: ble 0x14129e (j ← null) -> ble 0x141258 (leave the table's name)
                (0x0014_120e, 0x4b2c, 0xe022), // ch.a([BI)I: the saved name's new byte[] -> b 0x141256 (skip its bytes)
                (0x0014_126e, 0xf026, 0x46c0), // ch.a([BI)I: bl putfield ch.d (the saved price) -> nop
                (0x0014_1270, 0xfec9, 0x46c0), //            its second half -> nop
                (0x0010_db90, 0xf05a, 0x46c0), // aa.a([BI)I: bl putfield aa.a (the saved 요구레벨) -> nop
                (0x0010_db92, 0xfa38, 0x46c0), //            its second half -> nop
            ],
        }
    }

    /// hero2: no more SPELL-CARD drops (2026-09-24). `(site, as shipped, patched)`.
    /// User: *"네트워크가 필요없어져서 카드 드랍이 되면 안돼"*.
    ///
    /// The card drop is NOT in `enm/enm` — no enemy's table has a type-34 slot (§8.9).
    /// It is the one hardcoded roll in the kill routine `bd.q()V`:
    /// ```text
    /// 0x12847e  bl   h.a(1, 100)        ; roll ∈ 1..=100
    /// 0x128482  cmp  r0, #0x59          ; 89                        <-- THE SITE
    /// 0x128484  bgt  0x1284e0           ; > 89 (11 %) -> drop 34:(적인덱스/3 − 1)
    /// ```
    /// Raising the compare to 100 keeps the instruction (`cmp r0,#imm8`, only the
    /// immediate moves) and makes the branch UNREACHABLE — `h.a(II)I` @0x1421d0 is
    /// `min + |nextInt| % (max−min+1)`, so the roll is never above 100.
    ///
    /// Why removal and not a rarity tweak: the cards' only home was the network era —
    /// the CARD window (`cg`) has been replaced by ACCOUNT since #13, so a dropped card
    /// is loot the player can pick up but never see again. Cards already in a save's
    /// card bag (`ag.c`) are untouched: this is a code patch, nothing is written to the
    /// bag, and unpatching restores the shipped 11 %.
    pub fn card_drop_off(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            // bd.q()V: `cmp r0,#89` -> `cmp r0,#100` — the 11 % card branch never taken
            Self::Hero2Ktf => &[(0x0012_8482, 0x2859, 0x2864)],
        }
    }

    /// hero2: the ROW the game's own pickup refusal reads — `(site, as shipped)`. ★★ 2026-10-18 (user: 인벤토리에 빈 칸이
    /// 있는데 「빈공간이 없습니다.」가 떠서 안 주워진다 → 1 「재료가방 문구를 따로」).
    ///
    /// `ag.a()Z` @0x1121e4 (the OK pickup) asks `bf.a(BB)[B` @0x12d8c4 for a drop under the player that FITS — and
    /// `bf.a(BB)[B` routes by type: 23 · 30 · 31 · 32 (`bq.a(B)Z`, 조합서 · 조합 재료) to the 재료가방 `ag.b`, 34 to the card bag,
    /// the rest to the inventory. When a drop lies there and none fits, the tail @0x1125e8 raises
    /// `n.a(cf.c.i.a(7), 16, …)` — the one generic 「빈공간이 없습니다.」 for every bag. So a full 재료가방 reads as "the
    /// inventory is full" while the inventory shows empty cells:
    /// ```text
    /// 0x1125fc  ldr  r0, [pc, #0x3c]   ; cell i.a(I)[B
    /// 0x1125fe  adds r1, r5, #0        ; the /t/itmtp table
    /// 0x112600  movs r2, #7            ; THE ROW                    <-- THE SITE
    /// 0x112602  add  r0, sl
    /// 0x112604  bl   0x16917c
    /// ```
    /// The host rewrites the immediate, nothing else (`movs r2,#imm8` both ways — [`Hero2Java::bag_full_row_movs`]): on every OK
    /// on the field it reads the drops under the player and names [`Hero2Java::ITEM_TYPE_ROW_MATERIAL_FULL`] when every
    /// one of them is a 재료가방 type, row 7 otherwise (`emulator::BagFullRow`). It is the game's own notice either way;
    /// row 7 itself is untouched (the warehouse's message boxes share it). The ARM core decodes from memory every step —
    /// a rewrite takes effect on the next pickup.
    pub fn bag_full_row_site(self) -> Option<(u32, u16)> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some((0x0011_2600, Hero2Java::bag_full_row_movs(Hero2Java::ITEM_TYPE_ROW_BAG_FULL))),
        }
    }

    /// hero2: 사자후 (the berserker's 7th active, `a.p[10]`) hits the WHOLE MAP, and no
    /// longer kills the worker on a map smaller than the screen (2026-09-26). `(site, as
    /// shipped, patched)`. User: *"버서커로 사자후 스킬 사용하면 오류로 중단됨 … 수정해줘.
    /// 그리고 사자후 범위를 더 늘릴수도있어?"* → range decision 「맵 전체」.
    ///
    /// ── THE SCAN (`a.e(B)V` @0x15193c, HERO2_GUEST_RE.md §8.21) ──────────────────────
    /// `bl.a()Z` runs the roar on animation frame 9 through `a.e(B)V` — its ONLY caller
    /// (`find_method_callers.py`) — which walks a square of the stage's entity grid
    /// `bf.a:[[Lad;` (+0x30, `[y][x]`) and hits every `bd` in it:
    /// ```text
    /// w = h = n.a:I / 16                  ; the SCREEN WIDTH in tiles, twice (both cells
    ///                                     ;   0x17ed8c/0x17ed90 are n.a — not n.b)
    /// sx = this.d − w/2 ; if sx < 0 → 0 ; else if sx > bf.d − w → bf.d − w
    /// sy = this.e − h/2 ; if sy < 0 → 0 ; else if sy > bf.e − h → bf.e − h
    /// for i < w, j < h:  e = grid[sy+j][sx+i] ; if e instanceof bd → a.a(e, skill, atk, l, m)
    /// ```
    /// The upper clamp comes AFTER the `< 0` test and the loop never looks at the map
    /// size, so the routine assumes the map is at least `w` tiles each way. On the phone it
    /// was written for that held — 176 px is 11 tiles and the smallest map is 15×15 — but
    /// the landscape app gives the guest `220 × long/short` px (476 on the S25 = 29 tiles,
    /// 452 on the S9 = 28), and most maps have a side under that (20×20 rooms and dungeon
    /// floors, 60×20 / 80×20 fields). Then `sx`/`sy` go negative or the loop runs past the
    /// row: ArrayIndexOutOfBounds in `n.paint`, uncaught, worker dead. Measured: probe
    /// `--w 560` in the 30×30 camp → `FP = 0xfffffffb` (sy = 30 − 35) at 0x151a22, called
    /// from 0x133206; `--w 476` on 79 신드라프 남부 (60×20) → the same.
    ///
    /// ── THE PATCH: the window IS the map ─────────────────────────────────────────────
    /// The fourteen halfwords after `str r0,[sp,#0x20]` (the stage) are rewritten to hand
    /// the game's own loop `w = bf.d`, `h = bf.e`, `sx = sy = 0`, then jump to its set-up at
    /// 0x1519ec (the attack read, `i = 0`). Everything the loop reads keeps its home — `w`
    /// `[sp,#0x1c]`, `h` `[sp,#0x18]`, `sx` `[sp,#0x14]`, `sy` in `fp` — and the two reads
    /// reuse the routine's own literal-pool cells for `bf.d`/`bf.e` (0x151b14 → 0x17ed9c,
    /// 0x151b18 → 0x17eda4) through the static-read helper 0x167e84, so no new literal and
    /// no host block. The rest of the shipped window code (0x15197a..0x1519ec) and its
    /// clamp tails (0x151ad2..0x151af8) become dead. Three of the fourteen already read as
    /// the patched instruction (`add r0,sl` and the first `bl`); they stay in the table so
    /// the verify covers the whole new sequence.
    ///
    /// ── …AND THE MONSTERS IT KILLS OFF-SCREEN FINISH DYING (`bd.f()V` @0x1264a4) ───────
    /// The game freezes what it cannot see: a plain `bd` whose `bd.f:Z` (+0x3c — "drawn
    /// this frame", written by its painter `bd.a(II)V` every frame) is false only runs its
    /// state machine (`this.k()` → `c()` → the kill routine `q()`, EXP and drops) while it
    /// is within 4 tiles of the player; otherwise `bd.f()V` returns early. A whole-map
    /// roar therefore left its far kills in `g.t` 6 until the player walked up to each
    /// one — measured: 5 of 6 off-screen monsters stuck dying, EXP unchanged. The
    /// not-visible branch re-reads `this.a.i` (the sight range) for its second compare —
    /// javac's second evaluation, already in `r5` — and those ten halfwords now hold:
    /// ```text
    /// 0x126530  ldr r1,[r6] ; ldr r1,[r1,#0x40]   ; g.t (a B field is a word at +4+0x3c)
    ///           cmp r1,#6   ; beq 0x126582        ; dying: always run the state machine
    ///           cmp r7,r5   ; bgt 0x126552        ; dx > sight -> the target check, as shipped
    ///           mov r4,r8   ; cmp r4,r5           ; dy vs sight (r4 = dy, as shipped)
    ///           ble 0x126562 ; b 0x126552         ; the shipped two exits
    /// ```
    /// Only a DYING monster changes path; the rest is the shipped decision on the same
    /// values (0x126544..0x126550 become dead). The update path reads `this` from `r6` and
    /// sets `r4` before using it. The `ah` family (`al`·`ca`·`ci`·`m`) has its own `ah.f()V`
    /// with no such cut, so it always resolved.
    ///
    /// Nothing else changes: the hit itself (`a.a(Lbd;BIBB)Z` → `bd.a(La;BIZBBBB)Z`) skips
    /// a monster already dying or dead (`g.t` 6/7) before anything else, so a multi-tile
    /// monster or a second roar cannot double-kill, and the kill roll, the boss branch and
    /// the ×1.5 of #17 are the game's. A code patch — nothing is saved, and unpatching
    /// restores the screen-sized square (and its crash). The two sites are ONE table: the
    /// wide scan without the second site would leave its kills frozen.
    pub fn roar_whole_map(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0015_195e, 0x4869, 0x486d), // ldr r0,[pc,#0x1a4](n.a) -> ldr r0,[pc,#0x1b4] (pool 0x151b14 = bf.d)
                (0x0015_1960, 0x4450, 0x4450), // add r0, sl                               (unchanged)
                (0x0015_1962, 0xf016, 0xf016), // bl 0x167e84 (static read), high half    (unchanged)
                (0x0015_1964, 0xfa8f, 0xfa8f), // bl 0x167e84, low half                    (unchanged)
                (0x0015_1966, 0x2110, 0x9007), // movs r1,#16 -> str r0,[sp,#0x1c]         w = map width
                (0x0015_1968, 0xf017, 0x486b), // bl idiv (hi) -> ldr r0,[pc,#0x1ac]       (pool 0x151b18 = bf.e)
                (0x0015_196a, 0xfe48, 0x4450), // bl idiv (lo) -> add r0, sl
                (0x0015_196c, 0x4680, 0xf016), // mov r8,r0 -> bl 0x167e84 (hi)
                (0x0015_196e, 0x9007, 0xfa8a), // str r0,[sp,#0x1c] -> bl 0x167e84 (lo)
                (0x0015_1970, 0x4865, 0x9006), // ldr r0,[pc,#0x194] -> str r0,[sp,#0x18]  h = map height
                (0x0015_1972, 0x4450, 0x2000), // add r0,sl -> movs r0,#0
                (0x0015_1974, 0xf016, 0x9005), // bl (hi) -> str r0,[sp,#0x14]             sx = 0
                (0x0015_1976, 0xfa86, 0x4683), // bl (lo) -> mov fp, r0                     sy = 0
                (0x0015_1978, 0x2110, 0xe038), // movs r1,#16 -> b 0x1519ec                 the game's loop set-up
                // bd.f()V: a dying monster runs its state machine wherever it is
                (0x0012_6530, 0x4287, 0x6831), // cmp r7,r0 -> ldr r1,[r6]                   ptr_fields
                (0x0012_6532, 0xdc0e, 0x6c09), // bgt 0x126552 -> ldr r1,[r1,#0x40]         g.t
                (0x0012_6534, 0x4830, 0x2906), // ldr r0,[pc,#0xc0](bd.a) -> cmp r1,#6       dying?
                (0x0012_6536, 0x1c31, 0xd024), // adds r1,r6,#0 -> beq 0x126582              update
                (0x0012_6538, 0x4450, 0x42af), // add r0,sl -> cmp r7,r5                     dx vs sight
                (0x0012_653a, 0xf041, 0xdc0a), // bl (hi) -> bgt 0x126552
                (0x0012_653c, 0xfdb3, 0x4644), // bl (lo) -> mov r4,r8
                (0x0012_653e, 0x1c05, 0x42ac), // adds r5,r0,#0 -> cmp r4,r5                 dy vs sight
                (0x0012_6540, 0x482e, 0xdd0f), // ldr r0,[pc,#0xb8](cq.i) -> ble 0x126562
                (0x0012_6542, 0x1c29, 0xe006), // adds r1,r5,#0 -> b 0x126552
            ],
        }
    }

    /// hero2: the title carousel loses its 네트워크창고 stop (2026-09-24).
    /// `(site, as shipped, patched)`. User: *"메인 메뉴내에서 네트워크 창고라는 메뉴를
    /// 없애줘"*.
    ///
    /// ── WHAT THE TITLE MENU IS ──────────────────────────────────────────────────
    /// The carousel `t` (singleton `t.a:Lt;` @0x19448c) shows ONE label at a time and
    /// wraps with LEFT/RIGHT. Three coordinates, all measured (HERO2_GUEST_RE.md §8.12):
    ///
    /// | | |
    /// |---|---|
    /// | stop count | passed ONCE, `movs r3,#7` @0x1495e2 in `t.<init>(Z)V` — the shared `j.c(B)V` wraps `j.h` against `j.i`, so the ctor is the only place the 7 lives |
    /// | label | `com.tdf[j.h]` — `t.a(II)V` @0x149ab4 reads the cursor and feeds it straight to `i.a(I)[B` on the com table (`cf.a:Li;`) |
    /// | OK | a 7-entry jump table @0x17093c (`t.a(II)Z` @0x1496d0), each word BASE-RELATIVE: 0 새로하기 · 1 이어하기 · 2 환경설정 · 3 도움말 · **4 네트워크창고** · 5 정보이용료(`bh`) · 6 게임종료 |
    ///
    /// ── THE PATCH: count 6, stops 4·5 shift down one ────────────────────────────
    /// The count drops to 6 and jump-table entries 4·5 take over the 정보이용료 and
    /// 게임종료 handlers (the entries are base-relative and their high halfwords are all
    /// 0xfffd, so only the LOW halfword of each word moves). The OK handler's range
    /// check tightens with it. ⛔ The LABELS are the asset's half of this patch —
    /// `make_hero2_asset.py` rewrites `com.tdf[4]` ← 정보이용료 · `[5]` ← 게임종료 —
    /// and `bh`(정보이용료 화면) draws its header from `com[5]`, so its one index
    /// constant comes along to `com[4]`.
    ///
    /// ⛔ The label rewrite is safe by CENSUS, not by hope: of the 39 constant reads of
    /// the com table in the image, only `bh.a(II)V` reads #5 and nobody reads #4 or #6;
    /// the dynamic reads are `com[mode]` in the slot screen `be` (modes 1·10·11 once
    /// this lands — mode 4's only constructor was title case 4, `find_method_callers.py
    /// "be.<init>(Lj;B)V"` = 3 sites, all in `t.a(II)Z`) and `com[14+i]` 예/아니오 in
    /// the popup `d`. Runtime bytes at every site were probed equal to the file's
    /// (`--dump-mem` f=60): the table is not touched by the boot relocation.
    pub fn title_menu_no_network(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0014_95e2, 0x2307, 0x2306), // t.<init>(Z)V: movs r3,#7 -> #6 (stop count)
                (0x0014_96cc, 0x2806, 0x2805), // t.a(II)Z: cmp r0,#6 -> #5 (OK range check)
                (0x0017_094c, 0x8e4a, 0x8ebe), // jt[4].lo: 0x149786(네트워크창고) -> 0x1497fa(정보이용료)
                (0x0017_0950, 0x8ebe, 0x8ee4), // jt[5].lo: 0x1497fa(정보이용료) -> 0x149820(게임종료)
                (0x0012_e9ec, 0x2205, 0x2204), // bh.a(II)V: header com[5] -> com[4] (label moved by the asset)
            ],
        }
    }

    /// hero2's SKILL POWER redirect (#17, 2026-09-24) — see [`SkillPowerPatch`]. `None`
    /// everywhere else.
    ///
    /// Measured: `tools_re/hero2_dis.py 'bd.a(La;BIZBBBB)Z'` (the hit routine; its call to
    /// the class multiplier at 0x1278b8 with `r2` = the `a.p` index and `r3` = the damage),
    /// `hero2_dis.py '<job>.a(BILbd;)I'` (the three multipliers — `r0` is overwritten at
    /// each body's 5th instruction before it is read), `dump_hero2_java.py` (the three
    /// defs: `def_fo` + 0x100000).
    pub fn skill_power_patch(self) -> Option<SkillPowerPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(SkillPowerPatch {
                // (def, body, 기본공격 stages = a.i[class], skill-damage passive, counter skill) —
                // ay 로그 5 · bl 버서커 4 + 근력단련 a.p[4 + 8] + 반격태세 a.p[4 + 4] (2026-09-26) ·
                // bx 나이트 3
                jobs: [
                    (0x0018_7490, 0x0012_0c40, 5, None, None),
                    (0x0018_8b6c, 0x0013_38f4, 4, Some(12), Some(8)),
                    (0x0018_9d24, 0x0013_be3c, 3, None, None),
                ],
                skills_off: 0x138,
                // 2026-09-27: 나이트 검기 a.p[3+1] · 둔기강타 a.p[3+2] +30 % · 방패치기 a.p[3+3] +50 %,
                // and under 신의가호 (a.q:Z) 둔기강타 ×2 · 방패치기 ×1.5.
                // ★ 2026-10-03 저녁: under 신의가호 검기 ×1.539 · 방패치기 ×1.5625 · 발도 a.p[9] ×1.797
                // (Lv5 × the attack: 12.0 · 7.81 · 17.97 — the user's 12 · 7.8 · 18).
                extras: [&[], &[], &[(4, 77, 69), (5, 77, 128), (6, 128, 72), (9, 0, 102)]],
                buff_off: 0x114,
            }),
        }
    }

    /// hero2: the ROGUE's skill SP costs, −40 % (2026-09-27, user: *"로그의 sp소모량을
    /// 전체적으로 줄이고"* → 「중간」). `(site, as shipped, patched)`.
    ///
    /// `ay.a(I)I` @0x120af8 is the rogue's cost table — the only thing `a.f()Z` asks
    /// (0x14f852) for a skill past the first. Its jump table (@0x16fdc0, kind 5..10) has
    /// four shapes, each ending in an immediate that sets the size (HERO2_GUEST_RE.md §8.26):
    ///
    /// | kind | skill | shipped | patched |
    /// |---|---|---|---|
    /// | 6 · 8 | 민첩태세 · 맹독감염 | `최대SP >> 3` (`asrs r0,r0,#3`) | `최대SP / 13` (−38.5 %) — the `asrs` becomes `movs r1,#13` and the case's `b` goes to the shared divide 0x120b76 instead of the return |
    /// | 7 | 급소가격 | `최대SP / 10` | `/ 17` (−41.2 %) |
    /// | 9 · 10 | 단검난무 · 번개난타 | `(양손 최대공 합) · 3 / 2` | `· 3 / 4` — the sum is of the daggers' attack, which [`CombatPatch`] raises +20 %, so ×1.2 × 3/4 = the shipped ×1.5 × 0.6 (−40 %; measured LV48 162 → 96) |
    /// | 5 | 일격 | (never read — §8.8's `ble` makes skill 1 pay the basic hit's `a.j`) | unchanged |
    ///
    /// ⛔ 일격 stays: its cost IS `a.j`, the value every basic combo hit GAINS since #16, so it
    /// cannot be cut without cutting the rogue's SP income. `r0` holds 최대SP at the case-6/8
    /// divide (the getfield's return, copied to `r5` one instruction earlier) and the divide
    /// helper 0x1695fc is the one case 7 already calls. A code patch — nothing is saved.
    pub fn rogue_skill_sp_cut(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0012_0b72, 0x210a, 0x2111), // case 7 급소가격: movs r1,#10 -> #17 (최대SP / 17)
                (0x0012_0b88, 0x10c0, 0x210d), // case 6·8: asrs r0,r0,#3 -> movs r1,#13
                (0x0012_0b8a, 0xe7ea, 0xe7f4), // case 6·8: b 0x120b62 (return) -> b 0x120b76 (bl divide; b return)
                (0x0012_0bd4, 0x2102, 0x2104), // case 9·10: movs r1,#2 -> #4 (sum · 3 / 4; the sum is +20 % daggers)
            ],
        }
    }

    /// hero2: AGILITY's share of 치명 and 회피, raised (2026-09-27, user: *"민첩을 찍을때
    /// 치명및회피가 올라가는 계수를 상향"* → 「중간」). `(site, as shipped, patched)`.
    ///
    /// | | shipped | patched | where |
    /// |---|---|---|---|
    /// | 치명 버서커·나이트 (`a.l:I`) | `8 + 민첩 / 8` | `8 + 민첩 · 205 >> 10` = **민첩 / 5** (exact below 1020) | `bl.c()V` 0x13368a · `bx.c()V` 0x13bb52 |
    /// | 치명 로그 | `10 + 민첩 / 4 + 직감 · 3/2` | `10 + 민첩 · 171 >> 9` = **민첩 / 3** (exact below 510) + 직감 | `ay.c()V` 0x1206be |
    /// | 회피 (`a.m:I`, all classes) | `레벨 + 10 + 민첩 / 5` | `… + 민첩 / 3` | `a.m()V` 0x1500e2 |
    ///
    /// 민첩 = base `a.g:S` + the equipment's bonus `a.b[2]` (`a.b:[S` = equipment stat bonuses), as shipped. The crit sites have no free
    /// register and no room for a divide call (and the rogue's keeps a live `r3` across that
    /// stretch), so each is a MULTIPLY-SHIFT in the same five halfwords: the Java short cast's
    /// `lsls/asrs #16` pair goes (a stat is a small positive short, the saload helper already
    /// returned it sign-extended) and the freed slots carry `movs r0,#k ; muls r4,r0 ; asrs`.
    /// `r0`/`r5` are rewritten by the very next instructions in all three bodies. The game's
    /// clamps stay — 치명 8..=90 in `a.m()V` (0x150348), 회피 8..=90 at the roll (`a.a(Lbd;)Z`).
    /// ⚠ 치명 is rolled twice (`bd.a(La;BIZBBBB)Z` 0x127c38): `rand < a.l` and then `rand <
    /// clamp(20 + 몹Lv − 내Lv, 8, 50)` — this raises the first; the second is shipped.
    pub fn agility_coefficients(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                // bl.c()V (버서커): r4 = a.g:S, r0 = a.b[2]
                (0x0013_368a, 0x0400, 0x1824), // lsls r0,r0,#16 -> adds r4,r4,r0   (민첩)
                (0x0013_368c, 0x1400, 0x20cd), // asrs r0,r0,#16 -> movs r0,#205
                (0x0013_368e, 0x1824, 0x4344), // adds r4,r4,r0  -> muls r4,r0
                (0x0013_3690, 0x10e4, 0x12a4), // asrs r4,r4,#3  -> asrs r4,r4,#10  (= 민첩 / 5)
                // bx.c()V (나이트): the same shape
                (0x0013_bb52, 0x0400, 0x1824),
                (0x0013_bb54, 0x1400, 0x20cd),
                (0x0013_bb56, 0x1824, 0x4344),
                (0x0013_bb58, 0x10e4, 0x12a4),
                // ay.c()V (로그): r4 = a.g:S, r0 = a.b[2] (shipped sign-extends into r5)
                (0x0012_06be, 0x0400, 0x1824), // lsls r0,r0,#16 -> adds r4,r4,r0
                (0x0012_06c0, 0x1405, 0x20ab), // asrs r5,r0,#16 -> movs r0,#171
                (0x0012_06c2, 0x1964, 0x4344), // adds r4,r4,r5  -> muls r4,r0
                (0x0012_06c4, 0x10a4, 0x1264), // asrs r4,r4,#2  -> asrs r4,r4,#9   (= 민첩 / 3)
                // a.m()V: 회피 = level + 10 + 민첩 / N
                (0x0015_00e2, 0x2105, 0x2103), // movs r1,#5 -> #3
            ],
        }
    }

    /// hero2: the knight's 검기 flies 5 tiles instead of 3 (2026-09-27, user: *"검기는
    /// 사거리5칸으로 늘릴꺼고"*). `(site, as shipped, patched)`.
    ///
    /// The 검기 projectile `ak` (made by `bx.a()Z` case 4 at animation frame 5, 0x13b5be) is
    /// ALREADY a piercing line: `ak.b()V` @0x1178e4 checks its own tile every step, hits
    /// whatever monster stands there with the attack it was born with (kind 4 → the knight's
    /// multiplier) and moves on without dying, until its life `ak.a:B` runs out — one tile a
    /// step. The constructor `ak.<init>(La;I)V` sets that life to 3; 5 is the range.
    pub fn sword_wave_range(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0011_7884, 0x2203, 0x2205), // ak.<init>: movs r2,#3 -> #5 (ak.a:B, the life in tiles)
            ],
        }
    }

    /// hero2: the knight's attack from 힘 — `a.h = 힘' × 7/10` → **× 17/20 (0.85)** (★ 2026-10-20 밸런스 전수 조사, user
    /// approved ②: the knight was last in the LV 99 same-gear measurement — `HERO2_BALANCE_SURVEY.md` §8, 3,578..5,698 DPS
    /// against the berserker's 12,933 and the rogue's 8,964). `(site, as shipped, patched)`.
    ///
    /// `bx.c()V` @0x13ba8c computes `a.h:I` at 0x13bb1a..0x13bb28: `adds r5,r5,r4` (힘 = `a.e:S` + `a.b[0]`) · `movs r4,#7` ·
    /// `muls r5,r4` · `movs r1,#10` · `adds r0,r5` · `bl divide` — two immediates become 17 and 20. The berserker (`bl.c()V`:
    /// × (10 + 힘단련)/10) and the rogue ((힘' + 민첩')/2) are untouched; the knight's SKILL attack adds 지력' × 179/512 on top
    /// (`CombatPatch::KNIGHT_INT_MUL`) and is untouched too.
    pub fn knight_strength_coefficient(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_bb1c, 0x2407, 0x2411), // bx.c()V: movs r4,#7 -> #17
                (0x0013_bb20, 0x210a, 0x2114), // bx.c()V: movs r1,#10 -> #20   (a.h = 힘' × 17 / 20)
            ],
        }
    }

    /// hero2: 인내오라 becomes 「신의가호」 — a BUFF that changes three skills (2026-09-27, user:
    /// *"인내오라는 신의가호 라는 버프스킬로 변경하고 해당 버프를 사용중에는 검기가 3방향으로 나가고
    /// 둔기강타의 공격력이 2배, 방패치기의 범위도 앞3칸이아니라 앞6칸에 데미지1.5배"* → 「10→20초,
    /// 쿨 40초」 → 2026-09-27 저녁 *"만렙시 지속시간 20초에 쿨 30초로"* → *"오라수련 레벨당 신의가호 지속시간
    /// 2초 증가 … 오라수련 다올리면 신의가호는 쿨타임과 지속시간이 같아지는거야"*). The static half; what the buff DOES lives in [`CombatPatch`] (검기 3 ways,
    /// 방패치기 6 tiles) and [`SkillPowerPatch::extras`] (둔기강타 ×2, 방패치기 ×1.5), all keyed to
    /// the flag the cast already sets, `a.q:Z` (abs 0x114), for as long as buff 34 runs.
    /// `(site, as shipped, patched)`.
    ///
    /// | | shipped | patched |
    /// |---|---|---|
    /// | length (`bx.a()Z` case 7) | buff 34 for `10·lv + 70` frames (4 … 6 s) | `50·lv + 40·오라수련 + 150` = **10 … 20 s + 2 s per 오라수련 level** — Lv5 with 오라수련 5 = 600 frames = **30 s = the cooldown** (both levels top out at 5, `bx.a(I)B`) |
    /// | old effect (`a.m()V` 0x1501ee) | while `a.q:Z` is set: 최대 HP·SP × (11 + lv/2)/10 | **gone** — the branch into it is a `nop` |
    /// | cooldown (`bx.<clinit>` 0x13bfe8, `bx.a:[S[4]`) | 180 (90 after #17's ½ = 4.5 s) | **1200** (600 after ½ = **30 s**; 40 s = 1600 for the first hour) |
    ///
    /// The length needs TWO levels and the shipped case reads one through the byte-array helper
    /// (`movs r1,#7 ; bl 0x1689e4 ; lsls/asrs` — the Java cast). Its eight halfwords after the
    /// `a.p` getfield become direct loads out of the array block (`ldr r0,[r0] ; ldrb r1,[r0,#8+7] ;
    /// ldrb r2,[r0,#8+14]`, the shape `h2_array` reads — `a.p` is 15 long, both indices in it) and
    /// the sum (`movs r3,#50 ; muls r1,r3 ; movs r3,#40 ; muls r2,r3 ; adds r7,r1,r2`); the shipped
    /// `adds r7,#70` then adds 150. `r1`–`r3` are all rewritten before the buff call. 오라수련 =
    /// `a.p[3 + 11]` (sk1 스킬 11), shipped read only by 치유오라's heal (+10 HP a level) — it now
    /// also lengthens 신의가호, which its text 「오라 효과 증가」 already promised.
    ///
    /// The cooldown table is filled by `movs` immediates, and 1200 is not one: the shipped
    /// `adds r0,r4,#0` before the fifth store is redundant — the short-array store helper
    /// 0x16880c returns with `r0` (the array) untouched — so its slot and the two after it
    /// become `movs r1,#4 ; movs r2,#150 ; lsls r2,r2,#3`. ⛔ A `<clinit>` patch: it lands
    /// before a knight is ever loaded (the first patch pass), and the table it builds is what
    /// `bx.a(I)S` and the host plate ring both read. The name and text are the asset's
    /// (`t/sk1.tdf[8..9]`).
    ///
    /// ⛔ ★ 2026-10-05 fix: `ldrb` read 오라수련 UNSIGNED, and a knight who has not done the quest
    /// 「무사의 장갑」 (map 98 — the level table never opens it: `a.b:[[B` row 1 = 99) holds −1 there →
    /// 255 → 신의가호 lasted `120·255` frames more = **26 minutes** (probe: buff 34 length 31,200 on a
    /// 오라수련 −1 save, 1,200 on 오라수련 5). The rewrite now starts five halfwords earlier — the cast's
    /// `a.m()V` call goes: its one 신의가호 duty was the HP/SP boost (`a.q:Z`'s only reader, 0x1501e0,
    /// already `nop`'d below), the same call 민첩태세's rewrite dropped — and spends them on
    /// `cmp r2,#5 ; bls ; movs r2,#0` (anything outside 0..=5 counts as 0, as in 철괴).
    ///
    /// ★ 2026-10-06 (user: 버프 8종 쿨 1분 · 「만렙 지속 75초」 · 신의가호·철괴도 「전부 ×1.25」): the length is ×1.25 of the
    /// 10-03 one — **18.75 → 37.5 s over the levels + 7.5 s a 오라수련 level = 75 s with both at 5**; the cooldown
    /// stays 60 s. The table row above is the 09-27 history; the code below is the rule.
    pub fn blessing_aura_rework(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                // bx.a()Z case 7, after `a.q ← 1` (0x13b72a..=0x13b734, kept):
                (0x0013_b736, 0x483c, 0x483d), // ldr r0,=a.m()V cell -> ldr r0,=a.p cell (pool 0x13b82c)
                //  0x13b738 adds r1,r6,#0 · 0x13b73a add r0,sl — kept
                (0x0013_b73c, 0xf02d, 0xf02c), // bl 0x16917c (a.m(), hi) -> bl 0x1680a4 (getfield a.p, hi)
                (0x0013_b73e, 0xfd1e, 0xfcb2), // bl 0x16917c (lo)        -> bl 0x1680a4 (lo)
                (0x0013_b740, 0x483a, 0x6800), // ldr r0,=a.p cell  -> ldr  r0,[r0]        its element block
                (0x0013_b742, 0x1c31, 0x7bc1), // adds r1,r6,#0     -> ldrb r1,[r0,#15]    a.p[7]  = 신의가호's level
                (0x0013_b744, 0x4450, 0x7d82), // add r0,sl         -> ldrb r2,[r0,#22]    a.p[14] = 오라수련's level
                (0x0013_b746, 0xf02c, 0x2a05), // bl getfield (hi)  -> cmp  r2,#5
                (0x0013_b748, 0xfcad, 0xd900), // bl getfield (lo)  -> bls  +2
                (0x0013_b74a, 0x2107, 0x2200), // movs r1,#7        -> movs r2,#0          (−1 = not learned)
                // ★ 2026-10-03 (user: *"신의가호 스킬을 쿨타임 1분, 지속시간 30초의 스킬로 만들어주고, 오라수련을
                //   5레벨까지 올리면 쿨타임1분에 지속시간 1분으로"*): 75·lv + 120·오라수련 + 225 frames — Lv5 = 600
                //   (30 s), + 오라수련 5 = 1200 (60 s = the cooldown); cooldown 2400 (1200 after #17's ½ = 60 s).
                //   Before: 50 · 40 · 150 (10 … 20 s + 2 s a level) and 1200 (30 s).
                // ★ 2026-10-06 (user: 버프 쿨 1분 · 「만렙 지속 75초」 · 「전부 ×1.25」): ×1.25 of the above — the level
                //   part `(75·lv + 225) · 5/4` (375 · 468 · 562 · 656 · 750 = 18.75 … 37.5 s) and 150 a 오라수련 level
                //   (7.5 s) → Lv5 + 오라수련 5 = 1500 (75 s; the cooldown stays 60 s — see `counter_stance_duration` for
                //   why 75). The two `nop`s of the 10-03 version hold the extra quarter.
                (0x0013_b74c, 0xf02d, 0x234b), // bl 0x1689e4 (hi)  -> movs r3,#75
                (0x0013_b74e, 0xf94a, 0x4359), // bl 0x1689e4 (lo)  -> muls r1,r3
                (0x0013_b750, 0x0600, 0x31e1), // lsls r0,r0,#24    -> adds r1,#225        75·lv + 225
                (0x0013_b752, 0x230a, 0x108b), // movs r3,#10       -> asrs r3,r1,#2       its quarter
                (0x0013_b754, 0x1600, 0x18cf), // asrs r0,r0,#24    -> adds r7,r1,r3       ×1.25 (rounded down)
                (0x0013_b756, 0x4358, 0x2396), // muls r0,r3        -> movs r3,#150        (7.5 s a 오라수련 level)
                (0x0013_b758, 0x1c07, 0x435a), // adds r7,r0,#0     -> muls r2,r3
                //  0x13b75a ldr r0,=buff cell — kept (it touches neither r2 nor r7)
                (0x0013_b75c, 0x3746, 0x18bf), // adds r7,#70 -> adds r7,r7,r2: buff 34 lasts ⌊(75·lv + 225)·5/4⌋ + 150·오라수련
                (0x0015_01ee, 0xe195, 0x46c0), // a.m()V: b 0x15051c (the HP/SP boost) -> nop
                (0x0013_bfe8, 0x1c20, 0x2104), // bx.<clinit>: adds r0,r4,#0 (redundant) -> movs r1,#4
                (0x0013_bfea, 0x2104, 0x2296), //              movs r1,#4 -> movs r2,#150
                (0x0013_bfec, 0x22b4, 0x0112), //              movs r2,#180 -> lsls r2,r2,#4 (2400)
            ],
        }
    }

    /// hero2: the ROGUE's 민첩태세 becomes a short INVULNERABILITY (2026-09-27, user: *"인내오라를
    /// 일정시간 무적이 아니라 로그의 민첩태세를 일정시간 무적으로 바꿔줘"* — the values chosen for
    /// 인내오라 an hour earlier: 2 s at level 1 to 4 s at 5, cooldown 30 s, the old effect gone).
    /// The static half; the hit itself is refused by [`CombatPatch`]'s two guards, which test
    /// the flag the cast sets, `a.s:Z` (abs 0x120). `(site, as shipped, patched)`.
    ///
    /// | | shipped | patched |
    /// |---|---|---|
    /// | cast (`ay.a()Z` case 6, frame 7, 0x11fffa) | `putfield a.s ← 1` · `a.m()V` · buff 36 for a flat 110 frames | `a.s ← 1` stored directly, `lv = a.p[6]`, buff 36 for **`10·lv + 30`** = 40 … 80 frames = 2 … 4 s (the recompute is gone with the effect) |
    /// | old effect (`ay.c()V` 0x1204e2) | while `a.s:Z` is set: the 민첩 bonus | **gone** — the branch into it is a `nop` |
    /// | cooldown (`ay.<clinit>` 0x120e14, `ay.a:[S[1]`) | 200 (100 after ½ = 5 s) | **1200** (600 after ½ = **30 s**) |
    ///
    /// The eleven cast halfwords are one sequence ending right where the shipped buff call's
    /// own `ldr` (0x120010) begins, so that call — its cell, `r1 = this`, `r2 = 36` — runs as
    /// shipped with `r3` = the length this computed (its `movs r3,#110` becomes a `nop`).
    /// `r6` is `this` throughout the case. `a.s:Z`'s only readers are `ay.c()V` (the bonus
    /// now skipped) and the end-of-buff handler (`a.a(Lav;)Z` 0x14e0fe, which clears it).
    /// ⛔ Using another skill does not cancel it (`ag.c(B)V` only ever cancels 방어의지's 35).
    pub fn agility_stance_invuln(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0011_fffa, 0x483a, 0x6830), // ldr r0,=a.s cell   -> ldr r0,[r6]          this's field block
                (0x0011_fffc, 0x1c31, 0x3080), // adds r1,r6,#0       -> adds r0,#0x80
                (0x0011_fffe, 0x4450, 0x3080), // add r0,sl           -> adds r0,#0x80
                (0x0012_0000, 0x2201, 0x2101), // movs r2,#1          -> movs r1,#1
                (0x0012_0002, 0xf047, 0x6241), // bl putfield (hi)    -> str r1,[r0,#0x24]   a.s ← 1 (abs 0x120)
                (0x0012_0004, 0xffff, 0x6bc0), // bl putfield (lo)    -> ldr r0,[r0,#0x3c]   a.p (abs 0x138)
                (0x0012_0006, 0x4838, 0x6800), // ldr r0,=a.m() cell  -> ldr r0,[r0]         its element block
                (0x0012_0008, 0x1c31, 0x7b83), // adds r1,r6,#0       -> ldrb r3,[r0,#14]    a.p[6] = 민첩태세's level
                (0x0012_000a, 0x4450, 0x200a), // add r0,sl           -> movs r0,#10
                (0x0012_000c, 0xf049, 0x4343), // bl a.m() (hi)       -> muls r3,r0
                (0x0012_000e, 0xf8b6, 0x331e), // bl a.m() (lo)       -> adds r3,#30          10·lv + 30 frames
                (0x0012_0018, 0x236e, 0x46c0), // movs r3,#110        -> nop                  (the buff call takes r3)
                (0x0012_04e2, 0xe29f, 0x46c0), // ay.c()V: b 0x120a24 (the 민첩 bonus) -> nop
                (0x0012_0e14, 0x1c20, 0x2101), // ay.<clinit>: adds r0,r4,#0 (redundant) -> movs r1,#1
                (0x0012_0e16, 0x2101, 0x2296), //              movs r1,#1 -> movs r2,#150
                (0x0012_0e18, 0x22c8, 0x00d2), //              movs r2,#200 -> lsls r2,r2,#3 (1200)
            ],
        }
    }

    /// ★★ hero2's knight 방어의지 → 「철괴」 (2026-10-05, [`CombatPatch::IRON`]) — the static half; the damage cut
    /// itself is the combat block's ([`CombatPatch::IRON_AT`]). `(site, as shipped, patched)`:
    ///
    /// | part | where | what |
    /// |---|---|---|
    /// | **length** | `bx.a()Z` case 8 0x13b856..=0x13b880 | the shipped case re-reads `a.r` (just written 1 — always true) and builds `8·lv + 30` from one `a.p` read; the rewrite drops the re-read and spends its room on two direct byte loads: `lv = a.p[8]`, `aura = a.p[14]` (each outside 0..=5 → 0 — 오라수련 reads −1 until the quest 「무사의 장갑」), `r3 = 100·lv + 80·aura + 300` frames for the same buff-35 call. The two pool loads are re-aimed (`a.p`'s cell 0x13b8a8, the buff call's 0x13b8ac); the `bl getfield` at 0x13b85c stays where it was (it was the re-read's, same helper) |
    /// | **cooldown** | `bx.<clinit>` 0x13bff2/0x13bff4 | `bx.a[5]` 180 → `150 << 4` = 2400 (1200 = 60 s after #17's ½); the shipped `adds r0,r4,#0` it drops is redundant — the store helper 0x16880c keeps `r0` (the 신의가호 trick, index 4 right before) |
    /// | **no break** | `ag.c(B)V` 0x110a9e | `bne 0x110b12` (`a.r` set → `a.c(B)Z(35)` drops the buff) → `nop`: a basic attack or a skill (both start through `ag.c(B)V`) no longer ends it |
    /// | **walk** | `bx.e(II)V` 0x13ba3e | `beq` → `b`: the walking draw under `a.r` drew sequence 13 — the shipped guard walk, but 발도's swing since 2026-10-03 (`knight_iaido.py` redrew it) — now the plain walk. Standing keeps the shipped guard stance (sequence 12, `bx.a(IIB)V`) as the sign 철괴 is up |
    ///
    /// ★ 2026-10-06 (user: 버프 쿨 1분 · 「만렙 지속 75초」 · 「전부 ×1.25」): the length is **`125·lv + 100·aura + 375`**
    /// (×1.25 of the row above) = 25 → 50 s over the levels + 5 s a 오라수련 level → 75 s with both at 5; the cooldown
    /// stays 60 s. Why 75 and not 60: [`Self::counter_stance_duration`].
    pub fn iron_wall_rework(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                // bx.a()Z case 8, after `a.r ← 1` (0x13b84a..=0x13b854, kept):
                (0x0013_b856, 0x4813, 0x4814), // ldr r0,=a.r cell (re-read)   -> ldr r0,=a.p cell (pool 0x13b8a8)
                //  0x13b858 adds r1,r6,#0 · 0x13b85a add r0,sl · 0x13b85c bl getfield — kept
                (0x0013_b860, 0x1c05, 0x6800), // adds r5,r0,#0     -> ldr  r0,[r0]        a.p's element block
                (0x0013_b862, 0x2800, 0x7c01), // cmp r0,#0         -> ldrb r1,[r0,#16]    lv   = a.p[8]
                (0x0013_b864, 0xd100, 0x2905), // bne 0x13b868      -> cmp  r1,#5
                (0x0013_b866, 0xe624, 0xd900), // b 0x13b4b2        -> bls  +2
                (0x0013_b868, 0x480f, 0x2100), // ldr r0,=a.p cell  -> movs r1,#0          (outside 0..=5)
                (0x0013_b86a, 0x1c31, 0x7d82), // adds r1,r6,#0     -> ldrb r2,[r0,#22]    aura = a.p[14] 오라수련
                (0x0013_b86c, 0x4450, 0x2a05), // add r0,sl         -> cmp  r2,#5
                (0x0013_b86e, 0xf02c, 0xd900), // bl getfield (hi)  -> bls  +2
                (0x0013_b870, 0xfc19, 0x2200), // bl getfield (lo)  -> movs r2,#0          (−1 = not learned)
                // ★ 2026-10-06 (user: 버프 쿨 1분 · 「만렙 지속 75초」 · 「전부 ×1.25」): ×1.25 of the 10-05 rule —
                //   125·lv + 100·오라수련 + 375 = 25 … 50 s over the levels + 5 s a 오라수련 level → 75 s with both at 5.
                (0x0013_b872, 0x2108, 0x237d), // movs r1,#8        -> movs r3,#125        6.25 s a level
                (0x0013_b874, 0xf02d, 0x4359), // bl 0x1689e4 (hi)  -> muls r1,r3
                (0x0013_b876, 0xf8b6, 0x2364), // bl 0x1689e4 (lo)  -> movs r3,#100        5 s an 오라수련 level
                (0x0013_b878, 0x0600, 0x435a), // lsls r0,r0,#24    -> muls r2,r3
                (0x0013_b87a, 0x1540, 0x188f), // asrs r0,r0,#21    -> adds r7,r1,r2
                (0x0013_b87c, 0x1c07, 0x37bb), // adds r7,r0,#0     -> adds r7,#187
                (0x0013_b87e, 0x480b, 0x37bc), // ldr r0,=buff cell -> adds r7,#188        + 375 = 18.75 s
                (0x0013_b880, 0x371e, 0x480a), // adds r7,#30       -> ldr  r0,=buff cell  (pool 0x13b8ac)
                //  0x13b882 add r0,sl · adds r1,r6,#0 · movs r2,#0x23 (buff 35) · adds r3,r7,#0 · bl — kept
                (0x0013_bff2, 0x22b4, 0x2296), // bx.<clinit>: movs r2,#180 -> movs r2,#150
                (0x0013_bff4, 0x1c20, 0x0112), //              adds r0,r4,#0 (redundant) -> lsls r2,r2,#4 (2400)
                (0x0011_0a9e, 0xd138, 0x46c0), // ag.c(B)V: bne 0x110b12 (drop buff 35) -> nop
                (0x0013_ba3e, 0xd012, 0xe012), // bx.e(II)V: beq 0x13ba66 -> b 0x13ba66 (the plain walk)
            ],
        }
    }

    /// ★ 2026-10-05 (user: *"다른 버프들은 다 잘 나오는데 빛의 회복은 안나오고있어 … 수정해줘"* → 「진짜 버프로」):
    /// the beast's 빛의회복 (type 4, `t/bsk.tdf` 「주인공 HP, SP 회복 속도 증가」) was never a buff. The
    /// beast drive `bn.f()V`'s jump table sends type 4 to 0x1349d6: the sound `br.a(B)V(2)` and then the
    /// shared tail 0x13485e, which builds `new l(type, bn.g, bn.b(bn.g))` and hands it to `g.b(Lav;)V`
    /// — the EFFECT list `g.a` (drawn and aged, never ticked). The regen it promises IS in the game:
    /// the character's buff tick `a.a(Lav;)V` @0x14de98 gives HP every 30 frames and SP every 10 to a
    /// type-4 element of the BUFF list `g.b` — which only `bn.c(B)V` fills, and type 4 never called it.
    /// So the regen never ran and the HUD (`hero2java::h2_char_buffs`, `g.b`) had nothing to show.
    ///
    /// Five halfwords make type 4 what type 5 (얼음장막) is: `bn.c(4)` (→ `g.b(BBI)V(4, bn.g, len)` on
    /// the character — the same `l`, now on the buff list), then 얼음장막's own sound and exit. The
    /// literal is 얼음장막's — `ldr r0,[pc,#0x108]` from 0x1349d6 reads 0x134ae0, the `bn.c(B)V` cell
    /// 0x17afd0 the type-5 code at 0x1349f0 loads (case 4's own 0x134ad8 = `br.a`'s cell stays put).
    /// The branch lands on that code's `bl 0x169160` @0x1349f8 with r0 = the cell, r1 = this, r2 = 4.
    /// 0x1349e0 (`b 0x13485e`) is left as shipped and is now unreachable.
    pub fn beast_recovery_buff(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_49d6, 0x4840, 0x4842), // ldr r0,=br.a cell (0x134ad8)  -> ldr r0,=bn.c cell (0x134ae0)
                (0x0013_49d8, 0x2102, 0x4649), // movs r1,#2                    -> mov  r1,sb   (this beast)
                //  0x1349da add r0,sl — kept
                (0x0013_49dc, 0xf034, 0x2204), // bl 0x16916e (hi)              -> movs r2,#4   (빛의회복)
                (0x0013_49de, 0xfbc7, 0xe00b), // bl 0x16916e (lo)              -> b 0x1349f8   (얼음장막's bn.c call · sound · exit)
            ],
        }
    }

    /// ★ 2026-10-06 (user: *"지력으로 영향받는 야수 스킬데미지를 상향할꺼야 … 지력 3를 올려야 올라가던 데미지가 이제
    /// 2만 올려도 동일하게 올라가게"*): 지력 counts ×1.5 in the beast's attack skills. `(site, as shipped, patched)`.
    ///
    /// 지력 = the base `a.i:S` (abs 0x94) + the equipment's `a.b[4]` (`a.b:[S` = the equipment bonuses), and it reaches
    /// exactly two damage formulas (HERO2_GUEST_RE.md §8.42 · §8.49):
    ///
    /// | | shipped | patched |
    /// |---|---|---|
    /// | rings 7 빛의파동 · 8 지옥불 · 9 대지분노 — `bn.d(B)V` @0x134e88 | `(bn.d + 30 + 지력) · (lv + 3) / 3` | `(bn.d + 30 + 지력 + 지력/2) · (lv + 3) / 3` |
    /// | heavy 10 심판번개 · 11 학살화염 · 12 암흑폭발 — `bn.f()V` 0x134afc | `(bn.d·5/2 + 20 + 지력) · (lv + 2) / 3` | `(bn.d·5/2 + 20 + 지력 + 지력/2) · (lv + 2) / 3` |
    ///
    /// (`bn.d:S` = the beast's own power, `lv` = the skill's level.) 3 암살자 inflicts a status and reads no 지력, the
    /// rest are buffs and heals — untouched. Both sums were built one term at a time; each has a Java short cast after
    /// the saload of `a.b[4]` (`lsls/asrs #16` — the helper 0x1687b0 already sign-extends) and a pair of moves that
    /// park the next index in a register the formula never reads again (`r3`, `fp`). Those freed halfwords hold the
    /// running 지력 in a spare register (`r6` in the ring — its loop counter is written later; `r5` in the heavy case —
    /// it is set to 1 only at the call), add it, then add half of it. Nothing is saved; the beast's own power, levels
    /// and the character's stats are as they were.
    ///
    /// ★★ 2026-10-12 (user: *"야수 스킬의 지력 계수를 50% 더 상향"*): ×1.5 → **×2** — Claude's reading of 「50% 더」 = another
    /// half of the shipped weight (1 → 1.5 → 2; ×2.25 would need more room than the formulas leave). In the two formulas the
    /// 「지력/2」 halfword now keeps 지력 whole (0x134ee0 · 0x134b60). And the two beast STATUS ticks the monster's
    /// `bd.a(Lav;)V` deals (3 = 암살자 `(bn.d·27/10 + 20 + 지력) · lv · 2/3` · 7 = `(bn.d + 30 + 지력) · lv · 2/3`, each with
    /// the same short cast after `a.b[4]`) read 지력 too — the 2026-10-06 note above that 암살자 「reads no 지력」 was about the
    /// cast, not its tick — so they get ×2 as well: the constant moves into the freed cast pair and both 지력 terms are added
    /// twice. The table is still `(site, as shipped, patched)`.
    pub fn beast_int_weight(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                // bn.d(B)V (rings): r5 = bn.d + 30, r0 = a.i
                (0x0013_4ec0, 0x182d, 0x1c06), // adds r5,r5,r0  (+a.i)       -> adds r6,r0,#0   r6 = a.i
                (0x0013_4ed2, 0x0400, 0x1836), // lsls r0,r0,#16 (a.b[4])     -> adds r6,r6,r0   r6 = 지력
                (0x0013_4ed4, 0x1404, 0x19ad), // asrs r4,r0,#16              -> adds r5,r5,r6   + 지력
                (0x0013_4ee0, 0x192d, 0x1c36), // adds r5,r5,r4  (+a.b[4])    -> adds r6,r6,#0   지력 (★ 2026-10-12: was asrs r6,r6,#1 = 지력 / 2)
                (0x0013_4eee, 0x1c03, 0x1c01), // adds r3,r0,#0  (bn.g)       -> adds r1,r0,#0   the index, as the next did
                (0x0013_4ef0, 0x1c19, 0x19ad), // adds r1,r3,#0               -> adds r5,r5,r6   + 지력 (the second)
                // bn.f()V kinds 10..12 (heavy): r7 = bn.d·5/2 + 20, r0 = a.i
                (0x0013_4b4c, 0x183f, 0x1c05), // adds r7,r7,r0  (+a.i)       -> adds r5,r0,#0   r5 = a.i
                (0x0013_4b5e, 0x0400, 0x182d), // lsls r0,r0,#16 (a.b[4])     -> adds r5,r5,r0   r5 = 지력
                (0x0013_4b60, 0x1406, 0x1c2e), // asrs r6,r0,#16              -> adds r6,r5,#0   지력 (0x134b6c adds r6 as shipped; ★ 2026-10-12: was 지력 / 2)
                (0x0013_4b7a, 0x4683, 0x1c01), // mov fp,r0      (bn.g)       -> adds r1,r0,#0   the index, as the next did
                (0x0013_4b7c, 0x4659, 0x197f), // mov r1,fp                   -> adds r7,r7,r5   + 지력
                // ★ 2026-10-12 — the two beast statuses' ticks (`bd.a(Lav;)V`), 지력 ×1 as shipped → ×2: the base term's
                //   constant moves into the freed short-cast pair, the 지력 terms are added twice.
                // status 3 (암살자): r4 = bn.d·27/10 (+20 +a.i), then r5 = a.b[4] (added at 0x1266ee)
                (0x0012_66c2, 0x3414, 0x1824), // adds r4,#20                 -> adds r4,r4,r0   + a.i (the first)
                (0x0012_66e0, 0x0400, 0x1805), // lsls r0,r0,#16 (a.b[4])     -> adds r5,r0,r0   2·a.b[4]
                (0x0012_66e2, 0x1405, 0x3514), // asrs r5,r0,#16              -> adds r5,#20     + 20 (0x1266c4 adds a.i again)
                // status 7: r4 = bn.d (+30 +a.i), then r5 = a.b[4] (added at 0x1268ba)
                (0x0012_688e, 0x341e, 0x1824), // adds r4,#30                 -> adds r4,r4,r0   + a.i (the first)
                (0x0012_68ac, 0x0400, 0x1805), // lsls r0,r0,#16 (a.b[4])     -> adds r5,r0,r0   2·a.b[4]
                (0x0012_68ae, 0x1405, 0x351e), // asrs r5,r0,#16              -> adds r5,#30     + 30 (0x126890 adds a.i again)
            ],
        }
    }

    /// ★ 2026-10-06 (user: *"야수 공격스킬의 sp소모량을 조금 감소해줘. 버프스킬은 건들 필요없어."*): the beast's ATTACK
    /// skills cost 20 % less SP (Claude's reading of 「조금」). `(site, as shipped, patched)`.
    ///
    /// The cost is `bn.b(B)I` @0x135550 — a switch on the slot's kind (jump table @0x170668, entries relative to it)
    /// whose cases are constants or `a − b·lv` through the shared tail 0x13559c (`r1 − r0·lv`); `bn.a(BBBB)V` takes the
    /// slot's `bn.b:[S` from the SP before the cast (`bn.i()V` / `bn.b(B)V` fill it from this method — not saved):
    ///
    /// | kind | shipped | patched |
    /// |---|---|---|
    /// | 3 암살자 | 70 | **56** |
    /// | 7 빛의파동 · 8 지옥불 · 9 대지분노 | `(14 − lv) · 10` (130 … 90) | **`(14 − lv) · 8`** (104 … 72) |
    /// | 10 심판번개 · 11 학살화염 · 12 암흑폭발 | `180 − 6·lv` (174 … 150) | **`144 − 5·lv`** (139 … 119) |
    /// | 5 얼음장막 · 6 정화의불 (and 0 · 1 · 2 · 4) | as shipped | as shipped |
    ///
    /// ⛔ The rings shared their case with 5 얼음장막 (a buff — 「버프스킬은 건들 필요없어」) and its tail `(base − lv) · 10`
    /// with 6 정화의불. So that tail becomes three stubs of `r1 − r0·lv` on the 치유의물 case's own `movs r0,#10` (0x13559a,
    /// = `r1 − 10·lv`) and tail: 얼음장막 `140 − 10·lv` and 정화의불 `70 − 10·lv` (the same numbers) and the rings
    /// `112 − 8·lv`; the jump table's entries 5 · 6 · 7..9 are pointed at them (low halfwords — the high one, `0xfffc`,
    /// is the same for every target here, as `title_menu_no_network` did). 0x1355c8 (`b 0x1355bc`) is left unreachable.
    ///
    /// ★★ 2026-10-12 (user: *"쿨타임과 SP소모량 20%감소"* → 물어서 「공격 스킬만, 지금 값에서 −20 %」): the three attack families
    /// once more × 0.8 — 암살자 56 → **45** · rings `(112 − 8·lv)` → **`89 − 6·lv`** (83 … 59; ×0.8 exactly would be 83.2 …
    /// 57.6 — the stub's `r1 − r0·lv` keeps whole numbers) · heavy `(144 − 5·lv)` → **`115 − 4·lv`** (111 … 95). The buffs as
    /// they were. Their cooldown is [`Self::beast_attack_cooldown_cut`].
    pub fn beast_attack_sp_cut(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_55b0, 0x2046, 0x202d), // case 3 암살자: movs r0,#70 -> #45 (★ 2026-10-12: was #56)
                (0x0013_55ca, 0x21b4, 0x2173), // case 10..12: movs r1,#180 -> #115   (★ 2026-10-12: was #144)
                (0x0013_55cc, 0x2006, 0x2004), //              movs r0,#6   -> #4     (115 − 4·lv; was 144 − 5·lv)
                (0x0013_55ba, 0x2107, 0x218c), // 5 얼음장막:   movs r1,#7 (정화의불's) -> movs r1,#140
                (0x0013_55bc, 0x1b89, 0xe7ed), //              subs r1,r1,r6 -> b 0x13559a   (140 − 10·lv, as shipped)
                (0x0013_55be, 0x240a, 0x2146), // 6 정화의불:   movs r4,#10   -> movs r1,#70
                (0x0013_55c0, 0x1c08, 0xe7eb), //              adds r0,r1,#0 -> b 0x13559a   (70 − 10·lv, as shipped)
                (0x0013_55c2, 0x4360, 0x2159), // 7..9 rings:  muls r0,r4,r0 -> movs r1,#89    (★ 2026-10-12: was #112)
                (0x0013_55c4, 0xe7ed, 0x2006), //              b return      -> movs r0,#6     (was #8)
                (0x0013_55c6, 0x210e, 0xe7e9), //              movs r1,#14   -> b 0x13559c   (89 − 6·lv; was 112 − 8·lv)
                (0x0017_067c, 0x4f5e, 0x4f52), // jump table [5]: 0x1355c6 -> 0x1355ba
                (0x0017_0680, 0x4f52, 0x4f56), // jump table [6]: 0x1355ba -> 0x1355be
                (0x0017_0684, 0x4f5e, 0x4f5a), // jump table [7]: 0x1355c6 -> 0x1355c2
                (0x0017_0688, 0x4f5e, 0x4f5a), // jump table [8]
                (0x0017_068c, 0x4f5e, 0x4f5a), // jump table [9]
            ],
        }
    }

    /// ★★ 2026-10-12 (user: *"야수 스킬 … 쿨타임과 SP소모량 20%감소"* → 「공격 스킬만, 지금 값에서 −20 %」): the beast's ATTACK
    /// skills cool down 20 % faster. `(site, as shipped, patched)`.
    ///
    /// The cooldown is `bn.a(B)I` @0x1354b8 — a switch on the slot's kind (jump table @0x170634, entries relative to it):
    /// 3 암살자 = `movs r0,#80` (its own case 0x13551a) · 7..9 `(14 − lv) · 10` (0x13552c) and 10..12 `(21 − lv) · 10`
    /// (0x135538 → `b 0x13552e`) share the tail's `movs r4,#10` (0x135530) — no buff shares either case (1 · 4 go to
    /// 0x13551e since [`Self::buff_cooldown_minute`], 0 · 2 · 5 · 6 have their own). So two halfwords: 80 → **64** and
    /// ×10 → **×8** — exactly ×0.8 for all seven. The gear option 「야수 스킬 쿨타임 −20 %」 (`GearPatch` W_BEAST_CD) still
    /// scales the counter after the start.
    pub fn beast_attack_cooldown_cut(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_551a, 0x2050, 0x2040), // case 3 암살자:      movs r0,#80 -> #64
                (0x0013_5530, 0x240a, 0x2408), // 7..12 tail:        movs r4,#10 -> #8   ((14|21 − lv) · 8)
            ],
        }
    }

    /// ★★ 2026-10-12 (user: *"야수스킬 데미지50%상승"*): every beast attack hits ×1.5. `(site, as shipped, patched)`.
    ///
    /// The four damage formulas all end in `… · (lv + k) · m / 3` through the game's divide (`movs r1,#3 · bl 0x1695fc`):
    /// rings `bn.d(B)V` 0x134f00 · heavy `bn.f()V` 0x134b8c · and the two status ticks of `bd.a(Lav;)V` (3 = 암살자 0x126710 ·
    /// 7 = 0x1268dc). `/3 → /2` is exactly ×1.5 before the floor. Every beast hit lands in `bd.a(IBB)V` with one of those
    /// numbers (the `ce` effects carry the formula's result; 야수조련 목걸이 still doubles there).
    pub fn beast_attack_power(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_4f00, 0x2103, 0x2102), // rings 7..9  bn.d(B)V:   movs r1,#3 -> #2   ((…)·(lv+3)/2)
                (0x0013_4b8c, 0x2103, 0x2102), // heavy 10..12 bn.f()V:   movs r1,#3 -> #2   ((…)·(lv+2)/2)
                (0x0012_6710, 0x2103, 0x2102), // status 3 (암살자) tick: movs r1,#3 -> #2   ((…)·lv·2/2)
                (0x0012_68dc, 0x2103, 0x2102), // status 7 tick:          movs r1,#3 -> #2
            ],
        }
    }

    /// ★★ 2026-10-12 인장 「상급」 (user: *"가디언 노말 이상부터 떨어지는 인장류의 상급 을 추가할꺼야. 기존 인장과 로직은 같고 15~20
    /// 스탯 랜덤 부여"*): the seal with id [`Self::SEAL_TOP_ID`] gives +15 … +20 and costs 20,000 to apply. `(site, as shipped,
    /// patched)`.
    ///
    /// The alchemist's 「장비 능력 부여」 `x.c()V` @0x14b614 applies a seal (types 13..17) by OVERWRITING the weapon's stat
    /// `aa.i[type − 13] ← h.a(min, max)` with `min = 2·id + id/2`, `max = 3·id + 4` — 0~4 · 2~7 · 5~10 for the shipped ids 0..2,
    /// and 7~13 for 3. Its code recomputes them from three fresh `ch.h` reads; the rewrite at 0x14b6d0 (the first read's
    /// result in `r0`) computes them once — the shipped arithmetic for id < 3 (`asrs #1` = the divide for 0..2) — and
    /// [`Self::SEAL_TOP_MIN`] / `MAX − 4` above, then `ldr r3,[sp,#8]` (`this`, what 0x14b71e wants) and on to 0x14b71c, where
    /// the shipped code takes `r2` as `max − 4` and `r5` as `min`. 0x14b6e6..0x14b71a are left unreachable (image scan: no
    /// branch lands in 0x14b6d2..0x14b71a).
    ///
    /// The item list `x.a()[B` @0x14ba90 offers, for each of the five stats, the bag's FIRST record of `(13 + k, id)` for
    /// `id ≤ 2` (`cmp r6,#2` @0x14bb3a; a weapon with two stats already gets only those two's seals) — so an id-3 seal never
    /// showed (measured: the picker listed the three 「…의인장」 and not the 상급 in the same bag). `#2 → #3`; the list array
    /// is `new byte[bag size]` and every entry a distinct bag index, so one more id cannot overflow it.
    ///
    /// The cost `x.b()V` @0x14ae84 (`x.a:I`, taken from the gold in `x.c()V`) set 1,000 · 5,000 · 10,000 for ids 0 · 1 · 2
    /// and NOTHING for 3 (the field kept whatever it held). Its redundant second `cmp r0,#1` makes room: id 2 loads 10,000,
    /// id ≥ 3 doubles it — **20,000** (Claude's number, the user named none).
    #[allow(dead_code)] // the tests' and the tools' numbers (the halfwords below carry them)
    pub const SEAL_TOP_ID: u8 = 3;
    #[allow(dead_code)]
    pub const SEAL_TOP_MIN: u8 = 15;
    #[allow(dead_code)]
    pub const SEAL_TOP_MAX: u8 = 20;
    #[allow(dead_code)]
    pub const SEAL_TOP_COST: u32 = 20_000;
    /// What `x.c()V` rolls for a seal id (inclusive), shipped for 0..2 and [`Self::SEAL_TOP_MIN`]..=MAX from 3 on.
    #[allow(dead_code)]
    pub const fn seal_range(id: u8) -> (u8, u8) {
        if id >= Self::SEAL_TOP_ID { (Self::SEAL_TOP_MIN, Self::SEAL_TOP_MAX) } else { (2 * id + id / 2, 3 * id + 4) }
    }
    pub fn seal_top_tier(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                // x.c()V: r0 = ch.h (the seal's id)
                (0x0014_b6d0, 0x9e02, 0x2803), // ldr r6,[sp,#8]      -> cmp r0,#3
                (0x0014_b6d2, 0x0043, 0xda04), // lsls r3,r0,#1       -> bge 0x14b6de
                (0x0014_b6d4, 0x9001, 0x0042), // str r0,[sp,#4]      -> lsls r2,r0,#1     2·id
                (0x0014_b6d6, 0x48a5, 0x1045), // ldr r0,[pc,#0x294]  -> asrs r5,r0,#1     id/2
                (0x0014_b6d8, 0x1c31, 0x18ad), // adds r1,r6,#0       -> adds r5,r5,r2     min = 2·id + id/2
                (0x0014_b6da, 0x4450, 0x1812), // add r0,sl           -> adds r2,r2,r0     3·id (= max − 4)
                (0x0014_b6dc, 0x9301, 0xe001), // str r3,[sp,#4]      -> b 0x14b6e2
                (0x0014_b6de, 0xf01c, 0x250f), // bl (hi)             -> movs r5,#15       SEAL_TOP_MIN
                (0x0014_b6e0, 0xfce1, 0x2210), // bl (lo)             -> movs r2,#16       SEAL_TOP_MAX − 4
                (0x0014_b6e2, 0x1c06, 0x9b02), // adds r6,r0,#0       -> ldr r3,[sp,#8]    this
                (0x0014_b6e4, 0x48a2, 0xe01a), // ldr r0,[pc,#0x288]  -> b 0x14b71c        (ldr x.a:Lbu; · adds r1,r3 · … · adds r4,r2)
                // x.a()[B (the 능력 부여 item list): `for id in 0..=2 { idx = bq.a(BB)B(13 + k, id); if idx ≥ 0 add }`
                (0x0014_bb3a, 0x2e02, 0x2e03), // cmp r6,#2          -> cmp r6,#3         ids 0..=3 (the list is the bag's size — no overflow)
                // x.b()V: r0 = ch.h, already compared with 1 (beq 0x14aefc)
                (0x0014_aed8, 0x2801, 0xdd09), // cmp r0,#1 (again)   -> ble 0x14aeee      (id ≤ 0: 1,000 as shipped)
                (0x0014_aeda, 0xdd08, 0x4a12), // ble 0x14aeee        -> ldr r2,[pc,#0x48] 10,000 (the shipped literal 0x14af24)
                //  0x14aedc cmp r0,#2 — kept
                (0x0014_aede, 0xd1de, 0xd000), // bne return          -> beq 0x14aee2      (id 2: 10,000)
                (0x0014_aee0, 0x4a10, 0x0052), // ldr r2,[pc,#0x40]   -> lsls r2,r2,#1     (id ≥ 3: 20,000) — then ldr r0 / adds r1 / store as shipped
            ],
        }
    }

    /// hero2's COMBAT host block (2026-09-27) — see [`CombatPatch`]. `None` everywhere else.
    pub fn combat_patch(self) -> Option<CombatPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(CombatPatch {
                weapon_getters: [(0x0018_9058, 0x0013_603c), (0x0018_9074, 0x0013_6080)],
                // itm/00 양손대검 · 01 양손도끼 · 02 도검 +10 % · 03 단검 +20 % · 04 둔기 +10 %
                weapon_extra: [0, 0, 103, 205, 103],
                sweep: (0x0018_9c60, 0x0013_b8b4),
                wave_ctor: (0x0018_678c, 0x0011_77d0),
                wave_paint: (0x0018_67c4, 0x0011_7b70),
                cell_attack: 0x0017_bbf8,
                cell_hit: 0x0017_bc08,
                cell_stun: 0x0017_bc10,
                cell_stage_add: 0x0017_bb4c,
                dispatch: 0x0016_917c,
                new_instance: 0x0016_7c10,
                class_table_slot: 0x0019_5cc4,
                wave_class_off: 0x104,
                monster_descs: [0x0019_2670, 0x0019_23e8, 0x0019_29ac, 0x0019_2acc, 0x0019_3090],
                stage_slot: 0x0019_3afc,
                map_w_slot: 0x0019_3fcc,
                map_h_slot: 0x0019_3fdc,
                class_slot: 0x0019_3a6c,
                grid_off: 0x30,
                tile_x_off: 0x10,
                tile_y_off: 0x14,
                front_diag: [[0, 6, 7, 5, 8], [0, 5, 8, 7, 6]],
                code_dx: [0, 0, 0, -1, 1, -1, 1, -1, 1],
                code_dy: [0, -1, 1, 0, 0, -1, -1, 1, 1],
                bludgeon_pool: (0x0013_b7f0, 0xfffe_5ed8, 0xfffe_5efc),
                hp_delta: (0x0018_b940, 0x0014_f978),
                guard_pool: (0x0015_1ec8, 0xfffe_9140),
                guard_arm: (0x0015_1c06, 0x1c21, 0x4700),
                guard_hit: 0x0015_1c12,
                guard_miss: 0x0015_1c52,
                // ★ 2026-10-05 철괴: the `a.m:Z` load at 0x151d6a reads pool word 0x151f0c (0xfffe919c + sl =
                // the cell 0x17ee40) · `adds r1,r4,#0` at 0x151d6c · the clamp `mov r2,r8` at 0x151d7e ·
                // a.m:Z = 0x5c + 0xa4.
                iron_pool: (0x0015_1f0c, 0xfffe_919c),
                iron_arm: (0x0015_1d6c, 0x1c21, 0x4700),
                iron_back: 0x0015_1d7e,
                // ★ 2026-10-12 치유오라 × 지력: bx.a()Z case 3's `ldr r0,[pc,#0x264]` 0x13b53c → pool word 0x13b7a4
                // (0xfffe5f48 + sl = the cell 0x17bbec, `a.b(I)V`) · `add r0,sl` 0x13b540 (image scan: one reader).
                heal_pool: (0x0013_b7a4, 0xfffe_5f48),
                heal_arm: (0x0013_b540, 0x4450, 0x4780),
                heal_cell: 0x0017_bbec,
                halve_off: 0x100,
                facing_off: 0x40,
                weapon_l_off: 0xd4,
                weapon_m_off: 0xd8,
                skills_off: 0x138,
                blessing_off: 0x114,
                invuln_off: 0x120,
                stance_off: 0x118,
                wave_dir_off: 0x40,
                wave_owner_off: 0x4c,
                wave_attack_off: 0x54,
                // ★ 2026-10-01: bx.a(B)B — def 642 (word = body − 0x10bf10 | 1 in the file) ·
                // cb.d:S = ad 0x34 + 8 · the static helper ak.b(II)V's `bl 0x16916e` calls and
                // its pool word 0x117bc0 (0xfffe1268 + sl = 0x176f0c).
                seq_map: (0x0018_9c7c, 0x0013_b990),
                wave_frame_off: 0x3c,
                static_call: 0x0016_916e,
                cell_draw_seq: 0x0017_6f0c,
                // ★ 2026-10-03: defs 640 · 647 · 646 (bx.a()Z 0x13b45c · bx.a(I)I 0x13bd88 · bx.a(I)S
                // 0x13bd58) · the divide 0x1695fc (ay.a(I)I, bx.a()Z) · br.a(B)V's cell at case 4
                // (0x13b5d8) · g.w = ad 0x34 + 0x14 · a.o = 0x5c + 0x50 · a.i = 0x5c + 0x54 ·
                // a.e = 0x5c + 0x20 · ak.a:B = ak 0x4c + 4 · ak.b()V 0x117a5c `bne 0x117a60` → `b 0x117950`.
                cast: (0x0018_9c44, 0x0013_b45c),
                sp_cost: (0x0018_9d08, 0x0013_bd88),
                cooldown: (0x0018_9cec, 0x0013_bd58),
                // ★ 2026-10-07 천공의검: bx.a(I)B — def 0x189c28, body 0x13b438 (`i ≤ 2 → i+1 · 10 → 1 · 12 → 3 · else 5`)
                max_level: (0x0018_9c28, 0x0013_b438),
                divide: 0x0016_95fc,
                cell_sound: 0x0017_bb50,
                cell_sweep: 0x0017_bba0,
                action_frame_off: 0x48,
                combo_table_off: 0xac,
                combo_index_off: 0xb0,
                max_sp_off: 0x7c,
                wave_life_off: 0x50,
                wave_no_hit: (0x0011_7a5c, 0xd100, 0xe778),
                // ★ 2026-10-08 천공의검: a.n:[B's static slot (`RoguePatch::frames_slot`, the same word).
                frames_slot: 0x0019_45bc,
                // ★★ 2026-10-17 쌍월: a.g()V #882 def 0x18b6bc (body 0x14dd84 — the gear / berserker / demon blocks chain it later) ·
                // ag.j:[B 0x193bac · ag.a:[I 0x193bbc (`Hero2Java::{quickbar_slot, cooldown_slot}`).
                update: (0x0018_b6bc, 0x0014_dd84),
                quickbar_slot: 0x0019_3bac,
                cooldown_slot: 0x0019_3bbc,
                // a.g()V 0x14ddb0: `a.j:B (inst +0x58)` on a's 0x5c → 0xb4; `ble 0x14de38` → the state word 0x193aec == 3 → ag.i()V
                hitstop_off: 0xb4,
                state_slot: 0x0019_3aec,
            }),
        }
    }

    /// ★ 2026-10-03 발도: the SKILL window shows the knight's SEVENTH active. `(site, as shipped,
    /// patched)`. `s.a()V` @0x149326 sets the ACTIVE tab's row count — `class == 0 → 7, else 6`
    /// (the berserker has seven, the knight's seventh is 부활오라 the game hides, the rogue has
    /// six) — and the painter `s.a(II)V` @0x148b3c draws `class == 0 → 3, else 2` cells in the
    /// right column. The grid code (`k`) already walks seven for the berserker; `s.a()B` maps
    /// cursor → `a.p[L + h]`; the register popup gives value `h + 1` = 7.
    /// ★ 2026-10-05 일풍격: EVERY class seven / three now — the rogue's seventh is 일풍격 (`a.p[15]`,
    /// [`IlpungPatch`] remaps that cell's slot). Both class tests are left alone and their branches made
    /// constant (the knight-and-berserker form of 10-03 was `cmp r0,#2` + the branch flipped).
    /// ★ 2026-10-07 도발: the BERSERKER eight / four — its eighth active is 도발 (`a.p[15]`, [`BerserkerPatch`];
    /// [`IlpungPatch::ROWS`] names every cell's kind — 2026-10-07 (2)), the knight and the rogue eight as well. The
    /// class tests answer again: `s.a()V`'s `bne` stays shipped (class 0 → `movs r4,#7` made 8, the rest →
    /// `movs r4,#6` made 7) and the painter's `beq` becomes `bne` (class ≠ 0 → 3 at 0x148b4a; class 0 falls to
    /// `b 0x148d04` → `movs r6,#2` made 4). Eight rows = two columns of four (`s.b:B` 4) — the left column's
    /// four, the grid code walks `j.i` and the host tap reads the same two fields.
    pub fn knight_active_seven(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            // ★ 2026-10-07 각성기: EVERY class eight rows / four right-column cells (the eighth = the awakening,
            // `IlpungPatch::ROWS`): both `movs r4` made 8, both `movs r6` made 4 — the class tests stay as they are
            // (each path now says the same number).
            Self::Hero2Ktf => &[
                (0x0014_9332, 0x2407, 0x2408), // s.a()V:   class 0: movs r4,#7 -> #8
                (0x0014_9356, 0x2406, 0x2408), //           else:    movs r4,#6 -> #8
                (0x0014_8b46, 0xd000, 0xd100), // s.a(II)V: beq 0x148b4a -> bne 0x148b4a (class ≠ 0 → movs r6 below)
                (0x0014_8b4a, 0x2603, 0x2604), //           movs r6,#3 -> #4 (four cells)
                (0x0014_8d04, 0x2602, 0x2604), //           class 0 (via b 0x148d04): movs r6,#2 -> #4 (four cells)
            ],
        }
    }

    /// ★ 2026-10-07 (user: 분노태세 → ON/OFF 토글 · Lv1 받는피해 +20 % 주는피해 +20 % … Lv5 +10 % / +30 %): the shipped
    /// 분노태세 effects OFF — the rage's numbers are [`BerserkerPatch`]'s now (the dealt share in its damage wrapper, the
    /// taken share in the combat block's 철괴 hook). `(site, as shipped, patched)`. The flag `a.n:Z` (abs 0x104) stays:
    /// the cast still sets it and the buff's end handler still clears it — the two readers below were its only effect:
    ///
    /// | | where | shipped |
    /// |---|---|---|
    /// | the attack roll | `a.a(B)I` 0x151106 `beq 0x15110a` over `lsls r6,r6,#1` | rolled attack ×2 |
    /// | the stats | `a.m()V` 0x1502e4 `beq 0x1502e8` over `b 0x150444` | `a.g` (the shown attack) ×2 · `a.i` (defence) × 3/5 |
    ///
    /// Each `beq` becomes `b` (the not-raging path always). Part of [`BerserkerPatch`]'s install — all or nothing with the
    /// new numbers (alone it would be a rage that does nothing).
    pub fn rage_shipped_off(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0015_1106, 0xd000, 0xe000), // a.a(B)I: beq 0x15110a -> b 0x15110a (no ×2 on the roll)
                (0x0015_02e4, 0xd000, 0xe000), // a.m()V:  beq 0x1502e8 -> b 0x1502e8 (no ×2 attack · ×3/5 defence)
            ],
        }
    }

    /// ★ 2026-10-07 (user: *"몸통치기를 없앨꺼고"*): the berserker's 몸통치기 (`a.p[5]`) is retired — its level-table entry
    /// (`a.<clinit>` builds `a.b:[[B` row 0 with one `movs r2,#n` per skill; index 1 = LV 8 @0x1536d0) becomes 99, so the
    /// level-up's `a.d()V` never opens the slot again once `hero2java::h2_bodyslam_retire` has locked it at −1. Part of the
    /// SKILL window's row table (`IlpungPatch::ROWS`), which shows no cell for it.
    pub fn bodyslam_locked(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0015_36d0, 0x2208, 0x2263), // a.<clinit>: movs r2,#8 -> #99 (a.b[0][1] = 몸통치기's level: never)
            ],
        }
    }

    /// ★ 2026-10-07 (user: *"데미지가 점점 커짐에 따라 max데미지를 풀려고하는데"*): the ONE place a hit is capped — every monster
    /// class shares `bd.a(IB)V` @0x127f6c, which stores `HP − damage` as a SHORT (`lsls r3,r5,#16 ; asrs r5,r3,#16` @0x127fa6):
    /// a hit past `HP + 32,768` wrapped the HP back to positive and the monster lived. Both halfwords become `adds r5,r5,#0`
    /// (r3 is dead there), so the difference goes in whole — a hit past the HP is a kill, nothing else changes (the HP
    /// field is a word in the guest, the death test is `≤ 0`). Nothing in a save.
    pub fn damage_cap_off(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0012_7fa6, 0x042b, 0x1c2d), // bd.a(IB)V: lsls r3,r5,#16 -> adds r5,r5,#0
                (0x0012_7faa, 0x141d, 0x1c2d), //            asrs r5,r3,#16 -> adds r5,r5,#0
            ],
        }
    }

    /// ★★ 2026-10-16 (user: *"상한을 아예 없애줘. 보스레이드는 보스 종류로 만들꺼야."*): monster HP is a 32-bit int. The fields
    /// `bd.c:S` max HP · `bd.d:S` HP · `cq.a:S` the template's HP are words in the guest and the get/put helpers
    /// (0x1680a4 / 0x168004) load and store the whole word — the 32,767 ceiling was only the compiled code's `lsls #16 ;
    /// asrs #16` before each store. A field cross-reference (`find_field_uses.py bd.c/bd.d/cq.a --desc S --writes`) finds
    /// six writers; [`Self::damage_cap_off`] is one (the hit), these are the other five: the spawn (`bd.<init>` max HP =
    /// template + 20·bonus), the shipped poison tick (`bd.a(Lav;)V`, bypassed while `VenomPatch` is in), the two boss heals
    /// (`ca.d()V` · `m.d()V`, HP + max/100-ish) and the template scale (`cq.a()V`'s `(x << 15) >> 16` = x/2 wrapped at
    /// 16 bits → `x >> 1`). Each pair becomes plain moves (`adds rD,rS,#0`; the temp `r3` keeps its value, no flag is read
    /// before the next `adds`/`bl`). Every reader is int already: the HP bar `(hp − 1) × 28 / max` (overflows past
    /// ≈ 76.7 M HP), the boss phase thresholds `ah.c:I` (max/2 · 3·max/5), the instant kills that deal `bd.c`. Below
    /// 32,768 every value is the same as shipped. Nothing in a save (monsters are never saved).
    pub fn monster_hp_int(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0012_5dde, 0x0413, 0x1c13), // bd.<init>: lsls r3,r2,#16 -> adds r3,r2,#0   (max HP = template + 20k)
                (0x0012_5de0, 0x141a, 0x1c1a), //            asrs r2,r3,#16 -> adds r2,r3,#0
                (0x0012_6794, 0x042b, 0x1c2b), // bd.a(Lav;)V: lsls r3,r5,#16 -> adds r3,r5,#0 (the shipped poison tick)
                (0x0012_6798, 0x141d, 0x1c1d), //              asrs r5,r3,#16 -> adds r5,r3,#0
                (0x0013_d730, 0x042b, 0x1c2b), // ca.d()V: lsls r3,r5,#16 -> adds r3,r5,#0     (boss heal)
                (0x0013_d734, 0x141d, 0x1c1d), //          asrs r5,r3,#16 -> adds r5,r3,#0
                (0x0016_220a, 0x043b, 0x1c3b), // m.d()V: lsls r3,r7,#16 -> adds r3,r7,#0      (boss heal)
                (0x0016_220e, 0x141f, 0x1c1f), //         asrs r7,r3,#16 -> adds r7,r3,#0
                (0x0014_5454, 0x03c3, 0x1043), // cq.a()V: lsls r3,r0,#15 -> asrs r3,r0,#1     (template HP = x/2)
                (0x0014_5458, 0x141c, 0x1c1c), //          asrs r4,r3,#16 -> adds r4,r3,#0
            ],
        }
    }

    /// ★ 2026-10-08 가디언 ①: the three mode-indexed arrays four long — `ag.<clinit>`'s two `new byte[3]`
    /// (`ag.i:[B` quest multipliers 1·4·7 @0x115178 · `ag.m:[B` 달성도 weights 50·30·20 @0x1151b0) and
    /// `co.<clinit>`'s `new int[3]` (`co.b:[I` slot-screen colours @0x1442da). The fourth cell is the
    /// allocation's 0 until the host writes it ([`JavaWrite::ModeTables`]). ⛔ Must land BEFORE the two
    /// `<clinit>`s run — the logo, before the host's own `init_java_class("ag")` of the save-slot patch — so
    /// `emulator::ModeTables` applies it every frame from the first, and a table that still reads three long
    /// afterwards keeps the game at HELL ([`Self::guardian_mode_open`] is not applied). See [`GuardianPatch`].
    pub fn guardian_mode_tables(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0011_5178, 0x2103, 0x2104), // ag.<clinit>: movs r1,#3 -> #4  (new byte[] for ag.i:[B)
                (0x0011_51b0, 0x2103, 0x2104), // ag.<clinit>: movs r1,#3 -> #4  (new byte[] for ag.m:[B)
                (0x0014_42da, 0x2103, 0x2104), // co.<clinit>: movs r1,#3 -> #4  (new int[] for co.b:[I)
            ],
        }
    }

    /// ★ 2026-10-08 가디언 ②: the round clear `ag.y()V` @0x114e60 raises `ag.i` only while it is ≤ 1
    /// (`cmp r0,#1 ; bgt` @0x114e9e — HELL stays HELL). `#2` lets a HELL clear go to 3 and keeps 3 at 3.
    /// ⛔ Applied only once all three mode tables read four long (`emulator::ModeTables`): with a three-long
    /// table a mode-3 save is an ArrayIndexOutOfBounds on the title's slot screen.
    pub fn guardian_mode_open(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[(0x0011_4e9e, 0x2801, 0x2802)], // ag.y()V: cmp r0,#1 -> cmp r0,#2
        }
    }

    /// ★ 2026-10-08 가디언 ③: the labels. The slot screen `be.a(Graphics,IIB)V` clamps the preview's mode at 2
    /// (`cmp r7,#1 ; ble` @0x1292e6, else `movs r4,#2` @0x1292ea) before `co.b[m]` / `com[10 + m]` → `#2` and
    /// `#3`: 0..2 pass, anything above draws as 3 — `com[13]` in `co.b[3]`. (The first cut changed only the
    /// compare and still drew mode 3 as HELL — probe f=480 dump, 2026-10-08.) The restart notice `n.o()V` (state 14) drew `com[ag.i > 0 ? 9 : 8]` of the mode BEFORE the
    /// clear (0 → 「하드코어 모드로」 · 1, 2 → 「헬 모드로」): `cmp r0,#0 → #1` @0x165c84 and the else branch's
    /// `movs r2,#8` @0x165d08 → `adds r2,r0,#7`, so 0 → `com[7]` · 1 → `com[8]` · 2, 3 → `com[9]`. The asset moves
    /// the rows with it (`make_hero2_asset.py` COM_REPLACE: [7] 하드코어 모드로 · [8] 헬 모드로 · [9] 가디언 모드로 ·
    /// [13] GUARDIAN · the 정보이용료 fee text from [13] to a new [56]) and `bh.a(II)V`'s `movs r2,#0xd` @0x12ea3e →
    /// `#0x38` follows the fee text — its one constant reader (the 39 com-table readers walked 2026-10-08; rows 7 · 8 · 9
    /// have none but `n.o()V`, row 13 none but `bh.a`). ⛔ Asset and code ship together: without the regenerated
    /// asset these show the shipped rows under new indices.
    ///
    /// ★ 2026-10-17 (user: *"NORMAL 노랑 · HARDCORE 주황 · HELL 빨강 · GUARDIAN 보라 으로 세이브슬롯 내 문구 색상만 수정해주면돼"*):
    /// the slot screen's colours are `co.b:[I` (its one reader is that screen), built in `co.<clinit>` from literals — NORMAL
    /// `0xffff00` · HARDCORE **`0xff4600`** · HELL `0xff0000` (`movs #0xff ; lsls #16`) · GUARDIAN = the host's fourth cell
    /// [`GuardianPatch::COLOUR`]. HARDCORE's shipped `#ff4600` reads as red beside HELL's on the phone (S25 capture
    /// (255, 70, 0)), so its pool word @0x1446ac (one `ldr r6,[pc,#0x3c0]` @0x1442ea loads it; the word is once in the
    /// image) becomes `0xff9900` — its low halfword. Lands with the rest at the logo; `co` initialises at the title.
    pub fn guardian_labels(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0012_92e6, 0x2f01, 0x2f02), // be.a(G,IIB)V: cmp r7,#1 -> cmp r7,#2  (0..2 pass)
                (0x0012_92ea, 0x2402, 0x2403), //               movs r4,#2 -> movs r4,#3  (3 and above draw as 3 — it is a byte)
                (0x0016_5c84, 0x2800, 0x2801), // n.o()V: cmp r0,#0 -> cmp r0,#1
                (0x0016_5d08, 0x2208, 0x1dc2), // n.o()V: movs r2,#8 -> adds r2,r0,#7
                (0x0012_ea3e, 0x220d, 0x2238), // bh.a(II)V: movs r2,#0xd -> movs r2,#0x38
                (0x0014_46ac, 0x4600, 0x9900), // co.<clinit>: co.b[1] HARDCORE 0xff4600 -> 0xff9900 (orange; pool word, low half)
            ],
        }
    }

    /// ★ 2026-10-03 (user: *"사자후 쿨타임 50% 늘려줘"*): the berserker's cooldown table `bl.a:[S`
    /// is built in `bl.<clinit>` @0x133b7e.. with one `movs r2,#n` per entry — index 6 (사자후) is
    /// `movs r2,#0xa0` @0x133bc8 = 160 frames (80 after #17's halving) → `#0xf0` = 240 (120 after,
    /// 6 s; 광룡의 외침 halves that again to 60). The host plate ring reads the same table.
    pub fn roar_cooldown_up(self) -> &'static [(u32, u16, u16)] {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => &[],
            Self::Hero2Ktf => &[
                (0x0013_3bc8, 0x22a0, 0x22f0), // bl.<clinit>: movs r2,#0xa0 -> #0xf0 (bl.a[6] = 사자후)
            ],
        }
    }

    /// ★ 2026-10-08: 천공의검's picture, drawn by the host on the character draw — see [`SkyDrawPatch`]. `None`
    /// everywhere but hero2. The words are the accessory / rogue blocks' (the anchors selftest holds them equal);
    /// `fillRect`'s reference pointer `*(sl + 0x9c)` = 0x195d40 (its word reads `0x160169` in the image — name index
    /// 0x16, class 0x169, as `setColor`'s `0x950169` and `setAlpha`'s `0x100169`); `drawLine` = name index 0x59.
    pub fn sky_draw_patch(self) -> Option<SkyDrawPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(SkyDrawPatch {
                draw: 0x0018_b7d4,
                ground_draw: (0x0018_83e0, 0x0012_d4e4),
                hud_draw: (0x0018_d5b0, 0x0016_3964),
                iface_call: 0x0016_9248,
                graphics_slot: 0x0019_4bac,
                set_alpha_ref: 0x0019_5e20,
                set_color_ref: 0x0019_5d2c,
                fill_rect_ref: 0x0019_5d40,
                class_slot: 0x0019_3a6c,
                char_slot: 0x0019_3b5c,
                facing_off: 0x40,
                frame_off: 0x48,
            }),
        }
    }

    /// ★ 2026-10-06: the rogue's 살상연희 and 심안 — see [`RoguePatch`]. `None` everywhere else.
    /// Measured: `tools_re/hero2_dis.py 'bd.a(II)V'` (body 0x12601c, the `bd.h` test at 0x1260f2) · `'a.k()V'` (the driver:
    /// `g.w == 0` → `a.b(B)Z` usable · `a.f()Z`; `g.w == a.n[a.o[a.i]]` → the end, else the drive `a.a()Z`) · `'a.g()V'` (the
    /// update: `g.w += 1` first) · `'a.f(B)Z'` (번개난타's blow = `a.a(BBIBB)Z(kind, g.u, a.a()I, a.l, a.m)`) · `'a.a(BBII)V'`
    /// (the character draw = `n.a(BBBII)V(seq, g.w, dir, x, y)`) · the definition records read back from the image (word 0 +
    /// 0x10bf10 = body | 1) · `dump_hero2_java.py --klass bd/cq` (`bd.a:Lcq;` +0x4 · `bd.h:Z` +0x44 on `ad` 0x34 + `g` 0x28;
    /// `cq.a:Z` +0x48, no parent). The cells and statics are the 일풍격 / accessory blocks' (the anchors selftest holds them equal).
    pub fn rogue_patch(self) -> Option<RoguePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(RoguePatch {
                mob_paint: (0x0018_7b90, 0x0012_601c),
                usable: 0x0018_74ac,
                drive: 0x0018_7404,
                cooldown: 0x0018_7474,
                draw: 0x0018_b7d4,
                // ★ 2026-10-07 개화: ay.a(B)B — def 0x187420 (body 0x12037c, the identity)
                seq_map: 0x0018_7420,
                cell_attack: 0x0017_80c4,
                cell_hit: 0x0017_80d0,
                cell_sound: 0x0017_bb50,
                cell_draw: 0x0017_e674,
                dispatch: 0x0016_917c,
                call_static: 0x0016_916e,
                iface_call: 0x0016_9248,
                divide: 0x0016_95fc,
                class_slot: 0x0019_3a6c,
                char_slot: 0x0019_3b5c,
                graphics_slot: 0x0019_4bac,
                frames_slot: 0x0019_45bc,
                set_alpha_ref: 0x0019_5e20,
                skills_off: 0x138,
                invuln_off: 0x120,
                weapon_l_off: 0xd4,
                weapon_m_off: 0xd8,
                frame_off: 0x48,
                queue_off: 0xac,
                queue_at_off: 0xb0,
                mob_state_off: 0x3c,
                mob_template_off: 0x60,
                mob_hidden_off: 0xa0,
                template_ghost_off: 0x48,
            }),
        }
    }

    /// hero2's 맹독감염 rework (2026-09-29) — see [`VenomPatch`]. `None` everywhere else.
    pub fn venom_patch(self) -> Option<VenomPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(VenomPatch {
                strike: (0x0018_73cc, 0x0011_fb90),
                effect_pool: (0x0013_3d94, 0xfffe_5118),
                effect_arm: (0x0013_3d3e, 0x1c21, 0x4700),
                effect_resume: 0x0013_3d40,
                effect_life_off: 0xc,
                cell_has_status: 0x0017_fe18,
                cell_front: 0x0017_80c0,
                cell_inflict: 0x0017_80d4,
                cell_attack: 0x0017_bbf8,
                cell_damage: 0x0017_9670,
                dispatch: 0x0016_917c,
                divide: 0x0016_95fc,
                char_slot: 0x0019_3b5c,
                class_slot: 0x0019_3a6c,
                class_table_slot: 0x0019_5cc4,
                skills_off: 0x138,
                int_off: 0x94, //  ★★ 2026-10-16 = DemonPatch::int_off · bonus_off
                bonus_off: 0x98,
                mob_state_off: 0x3c,
                mob_hp_off: 0x74,
                tick_pool: (0x0012_6960, 0xfffe_35b8),
                tick_arm: (0x0012_6774, 0x1c39, 0x4700),
                tick_resume: 0x0012_6818,
                // 0x1781c4 (ay.a(BBI)Z, this case's own) -> 0x178190 (g.a(BI)V, 민첩태세's)
                cast_pool: (0x0012_03f4, 0xfffe_2520, 0xfffe_24ec),
                // ay.a()Z case 8 at frame 4, after `bl getfield a.p` (r0 = the a.p handle,
                // r6 = this): the strike becomes buff 33 for 50·v + 60·d + 150 frames.
                cast: &[
                    (0x0012_01ce, 0x2108, 0x6800), // movs r1,#8            -> ldr  r0,[r0]        a.p's element block
                    (0x0012_01d0, 0xf048, 0x7c01), // bl 0x1689e4 (hi)      -> ldrb r1,[r0,#16]    a.p[8]  = 맹독감염
                    // ★ 2026-10-06: 독 연마 is folded into 맹독감염 at its top level — a.p[12] is 「심안」 now, so the
                    //   cast no longer reads it (first version: `ldrb r2,[r0,#20]`, 0x7d02).
                    (0x0012_01d2, 0xfc08, 0x2203), // bl 0x1689e4 (lo)      -> movs r2,#3          독 연마 = mastered (MASTERY_MAX)
                    (0x0012_01d4, 0x0600, 0x2905), // lsls r0,r0,#24        -> cmp  r1,#5
                    (0x0012_01d6, 0x230a, 0xd900), // movs r3,#10           -> bls  (over the next)
                    (0x0012_01d8, 0x1600, 0x2100), // asrs r0,r0,#24        -> movs r1,#0          past the top = not learned
                    (0x0012_01da, 0x4358, 0x2a03), // muls r0,r3            -> cmp  r2,#3
                    (0x0012_01dc, 0x1c03, 0xd900), // adds r3,r0,#0         -> bls  (over the next)
                    (0x0012_01de, 0x4885, 0x2200), // ldr r0,=ay.a(BBI)Z    -> movs r2,#0
                    // ★ 2026-10-06 (버프 쿨 1분 · 만렙 75초 · ×1.25): 250·v + 0·d + 250 — was 50 · 60 · 150.
                    (0x0012_01e0, 0x3350, 0x23fa), // adds r3,#80           -> movs r3,#250        BUFF_PER_VENOM
                    (0x0012_01e2, 0x9300, 0x4359), // str r3,[sp]           -> muls r1,r3
                    (0x0012_01e4, 0x4450, 0x2300), // add r0,sl             -> movs r3,#0          BUFF_PER_MASTERY
                    (0x0012_01e6, 0x1c31, 0x435a), // adds r1,r6,#0         -> muls r2,r3
                    (0x0012_01e8, 0x1c3a, 0x188b), // adds r2,r7,#0         -> adds r3,r1,r2
                    (0x0012_01ea, 0x231a, 0x33fa), // movs r3,#26           -> adds r3,#250        the buff's frames (BUFF_BASE)
                    (0x0012_01ec, 0xf048, 0x4881), // bl 0x16917c (hi)      -> ldr  r0,[pc,#0x204] the pool word 0x1203f4
                    (0x0012_01ee, 0xffc6, 0x4450), // bl 0x16917c (lo)      -> add  r0,sl
                    (0x0012_01f0, 0x0600, 0x1c31), // lsls r0,r0,#24        -> adds r1,r6,#0       this
                    (0x0012_01f2, 0x1c34, 0x2221), // adds r4,r6,#0         -> movs r2,#33         the buff
                    (0x0012_01f4, 0x1c3d, 0xf048), // adds r5,r7,#0         -> bl 0x16917c (hi)    g.a(BI)V(33, frames)
                    (0x0012_01f6, 0x2800, 0xffc2), // cmp r0,#0             -> bl 0x16917c (lo)
                    (0x0012_01f8, 0xd100, 0xe5d9), // bne 0x1201fc          -> b 0x11fdae          return 1, as 민첩태세
                ],
                cooldown: &[
                    (0x0012_0e28, 0x1c20, 0x2103), // ay.<clinit>: adds r0,r4,#0 (redundant) -> movs r1,#3
                    (0x0012_0e2a, 0x2103, 0x2296), //              movs r1,#3 -> movs r2,#150
                    // ★ 2026-10-06: 2400 (½ → 1200 = 60 s), was `lsls r2,r2,#3` = 1200 (30 s).
                    (0x0012_0e2c, 0x22b4, 0x0112), //              movs r2,#180 -> lsls r2,r2,#4 (2400)
                ],
            }),
        }
    }

    /// hero2's 야수진화제 sell price (2026-09-30) — see [`ShopSellPatch`]. `None` everywhere else.
    pub fn shop_sell_patch(self) -> Option<ShopSellPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(ShopSellPatch {
                // cl.a(BB)V: 0x142c1e `ldr r0,[pc,#0xdc]` (cell 0x17cc0c = ch.d) · 0x142c20
                // `mov r1,sb` · `add r0,sl` · `bl 0x1680a4` · `asrs r5,r0,#2` · 0x142c2a `cl.b ← r5`.
                sell_pool: (0x0014_2cfc, 0xfffe_6f68),
                sell_arm: (0x0014_2c20, 0x4649, 0x4700),
                sell_resume: 0x0014_2c2a,
                // j.a(Graphics,II,ch,ZZZ)V: 0x15f986 `ldr r0,[pc,#0x118]` (cell 0x180358 = ch.d) ·
                // 0x15f988 `adds r1,r4,#0` · … · `asrs r2,r0,#2 ; mov fp,r2` when the flag · 0x15f99c.
                paint_pool: (0x0015_faa0, 0xfffe_a6b4),
                paint_arm: (0x0015_f988, 0x1c21, 0x4700),
                paint_resume: 0x0015_f99c,
                paint_sell_flag_sp: 0xc0,
                type_off: Hero2Java::ITEM_TYPE_OFF,
                id_off: Hero2Java::ITEM_ID_OFF,
                price_off: Hero2Java::ITEM_D_OFF,
            }),
        }
    }

    /// hero2's grade line (2026-10-13) — see [`GradeLinePatch`]. `None` everywhere else.
    pub fn grade_line_patch(self) -> Option<GradeLinePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(GradeLinePatch {
                // j.a(Graphics,II,ch,ZZZ)V: 0x15ede6 `str r0,[sp]` (the grade text) · 0x15ede8 `ldr r0,[pc,#0x220]`
                // (pool 0x15f00c → cell 0x180388 = ac.a(Graphics,II[BI)V) · 0x15edea `movs r5,#1` · 0x15edec
                // `add r0,sl` · `ldr r1,[sp,#0x84]` · `mov r2,sb` (x + 73) · `adds r3,r7,#0` (y) · `str r5,[sp,#4]` ·
                // 0x15edf6 `bl 0x16916e`.
                pool: (0x0015_f00c, 0xfffe_a6e4),
                arm: (0x0015_edea, 0x2501, 0x4700),
                record_sp: 0xb4,
                x_sp: 0x80,
                line_sp: 0x60,
                name_off: Hero2Java::ITEM_NAME_OFF,
                set_off: 0x28,
            }),
        }
    }

    /// hero2's boss crit floor (2026-10-10) — see [`BossCritPatch`]. `None` everywhere else.
    pub fn boss_crit_patch(self) -> Option<BossCritPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(BossCritPatch {
                // ah.a(La;BIZBBBB)Z: 0x1161ba `ldr r0,[pc,#0x1b0]` (0x486c; cell 0x176b58 = a.l:I) · 0x1161bc
                // `mov r1,r8` · `add r0,sl` · `ldr r5,[sp,#0x28]` · `bl 0x1680a4` · `movs r1,#10` · `bl 0x1695fc` ·
                // `adds r2,r0,#0` · `muls r2,r5,r2` · `str r2,[sp,#0x28]` · 0x1161d2 `ldr r3,[sp,#0x28]`.
                pool: (0x0011_636c, 0xfffe_0eb4),
                arm: (0x0011_61bc, 0x4641, 0x4700),
                resume: 0x0011_61d2,
                damage_sp: 0x28,
                divide: 0x0016_95fc,
                crit_off: 0xc0,
                crit_mul_off: 0xc4,
            }),
        }
    }

    /// hero2's boss defence pierce (2026-10-21) — see [`BossPiercePatch`]. `None` everywhere else.
    pub fn boss_pierce_patch(self) -> Option<BossPiercePatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(BossPiercePatch {
                // a.a(Lbd;SB)V: 0x151ce2 `ldr r0,[pc,#0x210]` (0x4884; cell 0x17ee0c = a.i:I) · 0x151ce4 `adds r1,r4,#0` ·
                // `add r0,sl` · `bl 0x1680a4` · 0x151cec `subs r6,r5,r0`.
                def_pool: (0x0015_1ef4, 0xfffe_9168),
                def_arm: (0x0015_1ce4, 0x1c21, 0x4700),
                def_resume: 0x0015_1cec,
                def_off: 0xa4,
                // 0x152000 `adds r6,r0,#0` (the shield, a.a()Lbu;) · 0x152002 `ldr r0,[pc,#0x94]` (0x4825; cell 0x17ee24 =
                // bu.a()I) · 0x152004 `adds r1,r6,#0` · `add r0,sl` · `bl 0x16917c` · 0x15200c `mov r5,r8` · `subs r5,r5,r0`.
                shield_pool: (0x0015_2098, 0xfffe_9180),
                shield_arm: (0x0015_2004, 0x1c31, 0x4700),
                shield_resume: 0x0015_200c,
                shield_cell: 0x0017_ee24,
                dispatch: 0x0016_917c,
                divide: 0x0016_95fc,
                boss_descs: Hero2Java::BOSS_CLASS_DESCS,
            }),
        }
    }

    /// hero2's HELL bonus drop (2026-10-02) — see [`HellDropPatch`]. `None` everywhere else.
    pub fn hell_drop_patch(self) -> Option<HellDropPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(HellDropPatch {
                // bd.q()V: definition #369, word 0 = 0x1c171 + 0x10bf10 = 0x128081.
                death: (0x0018_7ea0, 0x0012_8080),
                // The cells bd.q()V's own table drop calls through: 0x12820c (h.a(II)I, the
                // roll) and 0x128400 (bf.a(BBBB)V, the item onto the ground).
                cell_random: 0x0017_96ec,
                cell_put: 0x0017_9734,
                call_static: 0x0016_916e,
                dispatch: 0x0016_917c,
                mode_slot: 0x0019_3c4c,
                stage_slot: 0x0019_3afc,
                // bd's fields start at 0x5c (ad 0x34 + g 0x28): bd.a:Lcq; +0x4 → 0x60.
                // ad.d / ad.e are ad's own 0x10 / 0x14. cq has no parent: cq.f:S 0x44 · cq.b:[B 0x54.
                template_off: Hero2Java::MONSTER_TEMPLATE_OFF,
                tile_x_off: 0x10,
                tile_y_off: 0x14,
                gate_off: 0x44,
                slots_off: 0x54,
            }),
        }
    }

    /// ★ 2026-10-08 hero2's 「가디언」 mode — see [`GuardianPatch`]. `None` everywhere else.
    pub fn guardian_patch(self) -> Option<GuardianPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(GuardianPatch {
                // cq.a()V: definition #779 @0x18ab78, word 0 = 0x39491 + 0x10bf10 = 0x1453a1.
                template: (0x0018_ab78, 0x0014_53a0),
                mode_slot: 0x0019_3c4c,
                // ag.h:B @0x193b4c — the map number (find_field_uses ag.h --desc B --static: written by the warp ag.b(BBBB)V,
                // the load ag.a(BZ)V, ag.m()V, ag.z()V; read by the map loader and cf.n()V).
                map_slot: Hero2Java::MAP_SLOT,
                // ★★ 2026-10-17: ag.a:Lbf; @0x193afc and bf.b:B at +0x0 (Hero2SaveArea's stage_slot / stage_type_off).
                stage_slot: 0x0019_3afc,
                stage_map_off: 0x0,
                // cq has no parent (dump_hero2_java.py): h +0x20 · a:S +0x24 · b:S +0x28 · c:S +0x2c · e:S +0x34.
                level_off: 0x20,
                hp_off: 0x24,
                atk_off: 0x28,
                def_off: 0x2c,
                agi_off: 0x34,
                // bd.n:B +0x10 after ad (0x34) + g (0x28).
                monster_level_off: 0x6c,
                // cq.f:S +0x44 — the drop gate (0 = boss / event monster).
                gate_off: 0x44,
                // The four bd.n reads (find_field_uses bd.n --desc B, 2026-10-08) that feed a `monster LV − my LV`
                // formula: each `ldr r0,[pc,#imm]` and the pool word it loads (align4(site + 4) + imm), the word
                // as shipped (the sl-relative offset of cell 0x17956c · 0x17ebe0 · 0x17ee30 · 0x176b00 — each word
                // occurs once in the image) and the `bl 0x1680a4` pair at site + 6.
                hooks: [
                    (0x0012_7794, 0x0012_7a98, 0xfffe_38c8, [0xf040, 0xfc83]), // bd.a(La;BIZBBBB)Z — the player's miss
                    (0x0015_0994, 0x0015_0a14, 0xfffe_8f3c, [0xf017, 0xfb83]), // a.a(Lbd;)Z — the player's evade
                    (0x0015_1d0a, 0x0015_1efc, 0xfffe_918c, [0xf016, 0xf9c8]), // a.a(Lbd;SB)V — the monster's critical + status
                    (0x0011_6074, 0x0011_6328, 0xfffe_0e5c, [0xf052, 0xf813]), // ah.a(La;BIZBBBB)Z — the boss's miss
                ],
                // bd.q()V @0x128120: ldr r0,[pc,#0x1b8] (pool 0x1282dc = the sl offset of cell 0x1797d4 → a.d(I)V, once in
                // the image) · adds r1,r4,#0 · add r0,sl @0x128124 · mov r2,r8 · mov r5,r8 · bl 0x16917c @0x12812a.
                exp_hook: (0x0012_8120, 0x0012_82dc, 0xfffe_3b30),
                quest_mult_slot: 0x0019_39cc,
                weight_slot: 0x0019_39dc,
                colour_slot: 0x0019_438c,
            }),
        }
    }

    /// ★ 2026-10-08 밤 hero2's guardian gear options — see [`GearPatch`]. `None` everywhere else.
    pub fn gear_patch(self) -> Option<GearPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(GearPatch {
                // dump_hero2_java.py --json (def_fo + 0x100000): bl.a(I)I · bx.a(I)I · ay.a(I)I
                sp: [(0x0018_8b34, 0x0013_3794), (0x0018_9d08, 0x0013_bd88), (0x0018_7458, 0x0012_0af8)],
                // bl.a(BILbd;)I · bx.a(BILbd;)I · ay.a(BILbd;)I (SkillPowerPatch::jobs[1] · [2] · [0])
                damage: [(0x0018_8b6c, 0x0013_38f4), (0x0018_9d24, 0x0013_be3c), (0x0018_7490, 0x0012_0c40)],
                // g.a(BI)V (BuffRefreshPatch::def)
                buff: (0x0018_c8ac, 0x0015_c09c),
                // a.m()V: definition 0x18b9cc (= 0x185644 + 910 × 0x1c, dump_hero2_java def_fo 0x8b9cc), body 0x14fd00
                stats: (0x0018_b9cc, 0x0014_fd00),
                // bd.a(La;BIZBBBB)Z @0x127c46: ldr r0,[pc,#0x94] (pool 0x127cdc = the sl offset of cell 0x1795b0 → a.l:I, once in
                // the image) · mov r1,sb · add r0,sl @0x127c4a · bl 0x1680a4 @0x127c4c (f040 fa2a) — then cmp r5 (h.a(0,99)), r0.
                crit_hook: (0x0012_7c46, 0x0012_7cdc, 0xfffe_390c),
                // a.l:I: declared +0x64 after the 0x5c parent (§8.53: 「선언 오프셋 ≠ 절대 오프셋」; the probe's [h2-stats] crit)
                crit_off: 0xc0,
                char_slot: 0x0019_3b5c,
                // VenomPatch::cell_damage · dispatch
                cell_damage: 0x0017_9670,
                dispatch: 0x0016_917c,
                // ag.a(IZ)V (CombatPatch::cd_start's definition and body) · bd.q()V (HellDropPatch::death)
                cooldown_start: (0x0018_6134, 0x0011_2108),
                kill: (0x0018_7ea0, 0x0012_8080),
                // Hero2Java::quickbar_slot · cooldown_slot
                quickbar_slot: 0x0019_3bac,
                cooldown_slot: 0x0019_3bbc,
                // HellDropPatch::cell_random · call_static
                cell_random: 0x0017_96ec,
                call_static: 0x0016_916e,
                // ★★ 2026-10-14 bl.a(I)S (AccessoryPatch::cooldown) · bx.a(I)S (CombatPatch::cooldown) · ay.a(I)S (IlpungPatch::cooldown)
                cd_getter: [(0x0018_8b50, 0x0013_38c4), (0x0018_9cec, 0x0013_bd58), (0x0018_7474, 0x0012_0c10)],
                // ★★ 2026-10-14 (2차 B2) a.f()Z: definition #903 0x18b908 (= 0x185644 + 903 × 0x1c; word 0 = the body 0x14f6b8|1,
                // measured in client.bin1120) · a.a(I)V #904 0x18b924 (body 0x14f8c4) · a.o:[B / a.i:B (RoguePatch's queue words)
                combo: (0x0018_b908, 0x0014_f6b8),
                sp_delta_def: 0x0018_b924,
                queue_off: 0xac,
                queue_at_off: 0xb0,
                // ★★ 2026-10-15 (2차 F2) a.g(B)V #943 0x18bd68 (body 0x1521dc) · a.g()V #882 0x18b6bc (body 0x14dd84) — word 0 as
                // stored (W + 0x10bf10) in client.bin1120 · ag.a:Lbq; 0x193b7c · bq.a(I)Lch; cell 0x17eed8 (a.g(B)V's own) ·
                // ch.g:B +0 · ch.c:I +8 · g.t 0x3c · ag.k 0x193bec · g.b()Z 0x18c740
                use_item: (0x0018_bd68, 0x0015_21dc),
                update: (0x0018_b6bc, 0x0014_dd84),
                bag_slot: 0x0019_3b7c,
                cell_bag_record: 0x0017_eed8,
                record_type_off: 0x0,
                record_count_off: 0x8,
                state_off: 0x3c,
                walk_request_slot: 0x0019_3bec,
                step_def: 0x0018_c740,
                // ★★ 2026-10-17 a.i(B)V · a.v:Z (= DemonPatch::arrive_def · moved_off)
                arrive_def: 0x0018_b780,
                moved_off: 0x14c,
                // ★★ 2026-10-15 (2차 H2) bx.b(B)Z #649 0x189d40 (word 0 = the body 0x13bea4|1, as stored — W + 0x10bf10)
                usable: (0x0018_9d40, 0x0013_bea4),
                // ★★ 2026-10-21 a.a([B)V (IlpungPatch::load) · the divide 0x1695fc (CombatPatch::divide)
                load: (0x0018_be2c, 0x0015_2ed4),
                divide: 0x0016_95fc,
            }),
        }
    }

    /// hero2's skill-changing accessories (2026-10-02) — see [`AccessoryPatch`].
    /// ★ 2026-10-05: the rogue's 「일풍격」 — see [`IlpungPatch`]. `None` everywhere else.
    pub fn ilpung_patch(self) -> Option<IlpungPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(IlpungPatch {
                // a.d(B)Z · ay.a(I)I · ay.a(I)S · ay.a(I)B · s.a()B · ay.b(B)Z · ay.a()Z — defs 0x185644 + idx × 0x1c
                queue: (0x0018_b898, 0x0014_f154),
                sp_cost: (0x0018_7458, 0x0012_0af8),
                cooldown: (0x0018_7474, 0x0012_0c10),
                max_level: (0x0018_73b0, 0x0011_fb74),
                cursor: (0x0018_af68, 0x0014_93a8),
                usable: (0x0018_74ac, 0x0012_0cd4),
                drive: (0x0018_7404, 0x0011_fd34),
                load: (0x0018_be2c, 0x0015_2ed4),
                // a.f()Z @0x14f81e: ldr r0,=(0x17e8dc − sl) (pool 0x14f8b4) · add r0,sl · … · bl 0x16916e — ag.a(IZ)V
                cd_start: (0x0017_e8dc, 0x0018_6134, 0x0011_2108),
                // s.a(II)V's right column: a.b(B)B @0x148cac..0x148cb6 · a.a(I)B @0x148cca..0x148cd2 (each its own cell —
                // one pool word in the image names each)
                paint_level: (0x0014_8cb6, [0xf020, 0xfa61], 0x0017_d804, 0x0018_bdd8, 0x0015_246c),
                paint_max: (0x0014_8cd2, [0xf020, 0xfa53], 0x0017_d80c, 0x0018_b518),
                // s.a(II)V's right column draws each cell through bl 0x169160 @0x148cf6 with the cell 0x17d810 (pool 0x148d90)
                paint_cell: (0x0017_d810, 0x0018_af14, 0x0014_8d94),
                // ★ 2026-10-07: the LEFT column's three — a.b(B)B @0x148c1e..0x148c28 (cell 0x17d7f0) · a.a(I)B
                // @0x148c3e..0x148c44 (cell 0x17d7f8) · the drawer bl 0x169160 @0x148c68 (cell 0x17d7fc)
                paint_level_left: (0x0014_8c28, [0xf020, 0xfaa8], 0x0017_d7f0),
                paint_max_left: (0x0014_8c44, [0xf020, 0xfa9a], 0x0017_d7f8),
                paint_cell_left: 0x0017_d7fc,
                // s.a(BB)V @0x1482ac: ag.a(IIZ)V(slot, h + 1, 0) through the cell 0x17d6d8 (pool 0x1483d8) — def 0x1860c4
                register: (0x0017_d6d8, 0x0018_60c4, 0x0011_1f44, 0x0014_82ac),
                slots: [
                    (0x0014_d71c, 0x210f, 0x2112), // a.b()V: movs r1,#15 (new byte[15]) -> #18 (★ 2026-10-09 (11) 암살; 17 · 16 before)
                    (0x0014_d732, 0x2c0e, 0x2c11), //          cmp r4,#14 (fill -1 through 14) -> #17
                ],
                // ★★ 2026-10-09 (11) 「암살」 — s.a()V @0x14931e: the rogue's PASSIVE count `s.a:B ← 9` (movs r1,#9 @0x149320) → 10;
                // s.a(II)V's PASSIVE right column (loop r8, `a.p` index 11 + r8): a.b(B)B @0x14893a..0x148944 (cell 0x17d7c0, pool
                // 0x1489fc) · a.a(I)B @0x148958..0x148960 (cell 0x17d7c8, pool 0x148a04) · the drawer bl 0x169160 @0x148980 (cell
                // 0x17d7cc, pool 0x148a08) — each pool word once in the image
                passive_cells: (0x0014_9320, 0x2109, 0x210a),
                passive_level: (0x0014_8944, [0xf020, 0xfc1a], 0x0017_d7c0),
                passive_max: (0x0014_8960, [0xf020, 0xfc0c], 0x0017_d7c8),
                passive_cell: 0x0017_d7cc,
                // a.a(Lbd;BIBB)Z — def 0x18bbe0, word 0 0x151451 (no class overrides it) · ay.a(BILbd;)I — def 0x187490 (body
                // 0x120c40; SkillPower · accessory wrap it before this block, the gear block after) · co.a:[[B @0x19440c (the
                // receive's element factor `co.a[elem][cq.f]`, 0x127868..0x1278a6)
                hit: (0x0018_bbe0, 0x0015_1450),
                damage: (0x0018_7490, 0x0012_0c40),
                element_table_slot: 0x0019_440c,
                dispatch: 0x0016_917c,
                direct: 0x0016_9160,
                call_static: 0x0016_916e,
                divide: 0x0016_95fc,
                cell_attack: 0x0017_80c4,
                cell_hit: 0x0017_80d0,
                cell_sound: 0x0017_bb50,
                class_slot: 0x0019_3a6c,
                char_slot: 0x0019_3b5c,
                cursor_off: 0x8,
                max_sp_off: 0x7c,
                weapon_l_off: 0xd4,
                skills_off: 0x138,
                queue_off: 0xac,
                queue_at_off: 0xb0,
                frame_off: 0x48,
            }),
        }
    }

    /// ★ 2026-10-07: the berserker's 분노태세 toggle numbers and 「도발」 — see [`BerserkerPatch`]. `None` everywhere else.
    /// Measured: the definition records read back from the image (word 0 + 0x10bf10 = body | 1 — `bl.a(I)B` 0x188a70 ·
    /// `bl.a()Z` 0x188aa8 · `bd.f()V` 0x187be4; the SP / cooldown / damage defs are the accessory and SkillPower patches'),
    /// `tools_re/hero2_dis.py 'bl.a()Z'` (case 7 = 0x133058: `a.n ← 1` · `a.m()V` · `g.a(BI)V(30, 140)` through the cell
    /// 0x17abe4 · `br.a(B)V(5)` through 0x17abe8; case 10 사자후 = 0x133178: sound at frame 1, the blast `a.e(B)V` at 9) ·
    /// `'bd.f()V'` (the off-screen gate: `bd.f:Z` +0x3c, else beyond 4 tiles `bd.c ← null` and return) · `'bd.e()V'`
    /// (`bd.c == player` → step toward it) · `'bl.a(I)I'` (사자후 = `a.e << 2 / 5`) · `'bl.a(I)B'` (15 → 3 shipped) ·
    /// `dump_hero2_java.py` (`bd` fields on `ad` 0x34 + `g` 0x28 = 0x5c: `c:Lad;` +0x38 · `f:Z` +0x3c · `g:Z` +0x40;
    /// `a.n:Z` +0xa8 on 0x5c).
    pub fn berserker_patch(self) -> Option<BerserkerPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(BerserkerPatch {
                sp_cost: (0x0018_8b34, 0x0013_3794),
                cooldown: (0x0018_8b50, 0x0013_38c4),
                max_level: (0x0018_8a70, 0x0013_2c70),
                drive: (0x0018_8aa8, 0x0013_2cac),
                damage: (0x0018_8b6c, 0x0013_38f4),
                mob_update: (0x0018_7be4, 0x0012_64a4),
                // ★ 격분: bl.b(B)Z def 0x188b88 (body 0x133a9c) · bl.a(B)B 0x188a8c (0x132c94) · a.g()V 0x18b6bc (0x14dd84)
                usable: (0x0018_8b88, 0x0013_3a9c),
                seq_map: (0x0018_8a8c, 0x0013_2c94),
                update: (0x0018_b6bc, 0x0014_dd84),
                dispatch: 0x0016_917c,
                call_static: 0x0016_916e,
                divide: 0x0016_95fc,
                cell_buff: 0x0017_abe4,
                cell_sound: 0x0017_abe8,
                // a.a()I (the combat block's cell) · bd.a(IB)V (the venom tick's) · ag.c(B)V (n.a(I)V @0x162bce, pool 0x162bf4)
                // · the a.b(I)V definition record (= CombatPatch::hp_delta.0)
                cell_attack: 0x0017_bbf8,
                cell_damage: 0x0017_9670,
                cell_cast: 0x0018_0ac8,
                hp_delta_def: 0x0018_b940,
                char_slot: 0x0019_3b5c,
                stage_slot: 0x0019_3afc,
                class_slot: 0x0019_3a6c,
                monster_descs: [0x0019_2670, 0x0019_23e8, 0x0019_29ac, 0x0019_2acc, 0x0019_3090],
                rage_off: 0x104,
                skills_off: 0x138,
                queue_off: 0xac,
                queue_at_off: 0xb0,
                frame_off: 0x48,
                buffs_off: 0x38,
                max_sp_off: 0x7c,
                mob_target_off: 0x94,
                mob_visible_off: 0x98,
                mob_aggro_off: 0x9c,
                state_off: 0x3c,
                next_off: 0x2c,
                stage_head_off: 0x24,
                frames_slot: 0x0019_45bc,
            }),
        }
    }

    /// ★★ 「악마화」 D1 (2026-10-13): the 악의 block — see [`DemonPatch`]. Measured: `tools_re/hero2_dis.py` on
    /// `a.f(B)V` (0x1520d4, def 0x18bd30 — `bn.a(BBBB)V` through its cell 0x17eec4) · `bn.a(BBBB)V` (0x13419c: the
    /// checks, `ag.a(IZ)V` through the static cell 0x17ae6c) · `ag.a(IZ)V` (0x112108: the slot holding `value + 10`,
    /// `ag.a[slot]` ← `bn.a:[S[value]`) · `a.d(B)Z` (0x14f154, def 0x18b898) · `a.k()V` (the drive called at every
    /// frame `g.w ≠ a.n[kind]`) · `bn.b(B)I` / `bn.a(B)I` (0x135550 / 0x1354b8 — by SLOT: `bn.j[slot]`'s jump table)
    /// · `a.a(Lbd;BIBB)Z` (0x151450, def 0x18bbe0: r1 this · r2 the monster · r3 the kind · `[sp]` the attack ·
    /// `[sp+4]` `a.l:B` · `[sp+8]` `a.m:B`); `dump_hero2_java.py --json` for the field offsets (`bn` on `ad`'s
    /// 0x34 — ⛔ not `g`'s 0x5c: `bn.a:B` is absolute 0x60, `bn.j` 0x3c, `bn.k` 0x40). The whitelist = `t/sk0..2.tdf`
    /// row order (`dump_tdf.py`): 분노태세 · 반격태세 · 도발 / 치유오라 · 신의가호 · 철괴 / 민첩태세 · 맹독감염.
    pub fn demon_patch(self) -> Option<DemonPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(DemonPatch {
                cast: (0x0018_bd30, 0x0015_20d4),
                arm: (0x0018_b898, 0x0014_f154),
                drives: [(0x0018_8aa8, 0x0013_2cac), (0x0018_9c44, 0x0013_b45c), (0x0018_7404, 0x0011_fd34)],
                update: (0x0018_b6bc, 0x0014_dd84),
                beast_sp: (0x0018_8eec, 0x0013_5550),
                beast_cooldown: (0x0018_8ed0, 0x0013_54b8),
                // ★ D2: n.a(BBBII)V — definition #1151 (dump_hero2_java --json: def_fo 0x8d428, body_ptr 0x57099 + 0x10bf10)
                draw: (0x0018_d428, 0x0016_2fa8),
                hit_def: 0x0018_bbe0,
                // ★ D2: ag.c(B)V's definition (= AccessoryPatch::arm.0) · a.n:[B's static slot (= CombatPatch::frames_slot)
                key_arm: 0x0018_5d98,
                frames_slot: 0x0019_45bc,
                // ★ D3: the ground draw and the HUD painter (= SkyDrawPatch::ground_draw · hud_draw) and its Graphics refs
                ground_draw: (0x0018_83e0, 0x0012_d4e4),
                hud_draw: (0x0018_d5b0, 0x0016_3964),
                iface_call: 0x0016_9248,
                graphics_slot: 0x0019_4bac,
                set_alpha_ref: 0x0019_5e20,
                set_color_ref: 0x0019_5d2c,
                fill_rect_ref: 0x0019_5d40,
                // ★ D3: bd.a(IB)V's cell (= VenomPatch::cell_damage) · bn.d:S (ad 0x34 + 0x20) · ag.j:[B · ag.a:[I · ad.a:S · ad.b:S
                cell_mob_hurt: 0x0017_9670,
                beast_power_off: 0x54,
                quickbar_slot: 0x0019_3bac,
                cooldown_slot: 0x0019_3bbc,
                px_x_off: 0x0,
                px_y_off: 0x4,
                dispatch: 0x0016_917c,
                call_static: 0x0016_916e,
                divide: 0x0016_95fc,
                cell_cd_start: 0x0017_ae6c,
                cell_sound: 0x0017_abe8,
                cell_attack: 0x0017_bbf8,
                char_slot: 0x0019_3b5c,
                stage_slot: 0x0019_3afc,
                class_slot: 0x0019_3a6c,
                combat_slot: 0x0019_400c,
                monster_descs: [0x0019_2670, 0x0019_23e8, 0x0019_29ac, 0x0019_2acc, 0x0019_3090],
                beast_off: 0x134,
                beast_ids_off: 0x3c,
                beast_levels_off: 0x40,
                beast_state_off: 0x60,
                sp_off: 0x70,
                max_sp_off: 0x7c,
                int_off: 0x94,
                bonus_off: 0x98,
                queue_off: 0xac,
                queue_at_off: 0xb0,
                frame_off: 0x48,
                facing_off: 0x40,
                state_off: 0x3c,
                tile_x_off: 0x10,
                tile_y_off: 0x14,
                attr_l_off: 0xd4,
                attr_m_off: 0xd8,
                next_off: 0x2c,
                stage_head_off: 0x24,
                // ★ D4: the damage functions (= GearPatch::damage) · a.b(I)V (= BerserkerPatch::hp_delta_def) · a.a(Lbd;BI)V
                //   (dump_hero2_java --json: def_fo 0x8bcdc, body 0x151b48) · g.b()Z (def_fo 0x8c740, body 0x15b1c0 — the
                //   update a.g()V's `m.b()Z` call, cell 0x17e450) · the accessory block's can_stand / relocate · ad's width
                damage: [(0x0018_8b6c, 0x0013_38f4), (0x0018_9d24, 0x0013_be3c), (0x0018_7490, 0x0012_0c40)],
                hp_delta_def: 0x0018_b940,
                status_def: 0x0018_bcdc,
                cell_has_status: 0x0017_fe18, // = VenomPatch::cell_has_status (g.a(B)Z)
                step_def: 0x0018_c740,
                // ★★ 2026-10-17 a.i(B)V — dump_hero2_java --json: def_fo 0x8b780 (word 0 = the body 0x14e5cc|1), private, called
                //   by a.g()V @0x14de14 with g.t as it was before the step · a.v:Z declared +0xf0 → 0x14c · the state word
                arrive_def: 0x0018_b780,
                moved_off: 0x14c,
                app_state_word: 0x0019_3aec,
                body_can_stand: 0x0012_ccf8,
                body_relocate: 0x0012_5fc4,
                width_off: 0x20,
                walk_request_slot: 0x0019_3bec,
                // ★ D5: bn.a(BIBI)V (= AccessoryPatch::ring_spawn — hero2_dis.py: x = ad.d + co.a[dir] · y = ad.e + co.b[dir], cells
                //   0x17b058 / 0x17b060; dump_hero2_java: co.a:[B @0x19439c · co.b:[B @0x1943ac) · the map size bf.d / bf.e
                spawn: (0x0018_8d48, 0x0013_4d3c),
                // ★★ 2026-10-15 bd.q()V (= GearPatch::kill · HellDropPatch::death)
                kill: (0x0018_7ea0, 0x0012_8080),
                // ★ 2026-10-16 bd.a(IB)V — dump_hero2_java --json: def_fo 0x87e84 (+0x100000), body 0x127f6c (v66, the one record
                //   for bd · ah · al · ca · ci · m); the cell = VenomPatch::cell_damage
                mob_damage: (0x0018_7e84, 0x0012_7f6c),
                dir_dx_slot: 0x0019_439c,
                dir_dy_slot: 0x0019_43ac,
                map_w_slot: 0x0019_3fcc,
                map_h_slot: 0x0019_3fdc,
                // ★★ 2026-10-15 bf.b:B (= Hero2SaveArea::stage_type_off)
                stage_map_off: 0x0,
            }),
        }
    }

    pub fn accessory_patch(self) -> Option<AccessoryPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(AccessoryPatch {
                // j.a(Graphics,II,ch,ZZZ)V's exit: add sp,#0x8c · pop {r3-r6} · mov r8,r3 · mov sb,r4.
                desc_exit: 0x0015_f210,
                desc_exit_shipped: [0xb023, 0xbc78, 0x4698, 0x46a1],
                set_color_ref: 0x0019_5d2c,
                iface_call: 0x0016_9248,
                text_cell: 0x0018_0454,
                fill_rect_ref: 0x0019_5d40,
                sp_cost: (0x0018_8b34, 0x0013_3794),
                cooldown: (0x0018_8b50, 0x0013_38c4),
                damage_rogue: (0x0018_7490, 0x0012_0c40),
                beast_damage: (0x0018_7e30, 0x0012_7cec),
                usable_rogue: (0x0018_74ac, 0x0012_0cd4),
                drive_rogue: (0x0018_7404, 0x0011_fd34),
                strike_rogue: (0x0018_73e8, 0x0011_fc88),
                arm: (0x0018_5d98, 0x0011_0a50),
                update: (0x0018_b748, 0x0014_e2e4),
                draw: (0x0018_b7d4, 0x0014_ea98),
                dispatch: 0x0016_917c,
                call_static: 0x0016_916e,
                // ay.a(BBI)Z's own: a.a()I · a.a(B)I · a.a(Lbd;BIBB)Z · a.a(Lbd;BI)V.
                cell_attack: 0x0017_80c4,
                cell_bonus: 0x0017_80c8,
                cell_hit: 0x0017_80d0,
                cell_status: 0x0017_80d4,
                // a.e(BBI)Z's a.a(BBIBB)Z · a.f()Z's a.a(I)I and a.a(I)V · the knight's bx.h(B)Z ·
                // a.a(BBII)V's static n.a(BBBII)V.
                cell_tile_strike: 0x0017_ed7c,
                cell_sp_cost: 0x0017_e8a8,
                cell_sp_delta: 0x0017_e8e0,
                cell_sweep: 0x0017_bba0,
                cell_draw: 0x0017_e674,
                cell_far_strike: 0x0017_ed84,
                graphics_slot: 0x0019_4bac,
                set_alpha_ref: 0x0019_5e20,
                // g.u()V · ad.a()V · g.t()V · g.c(I)V · bf.a(Lg;II)Z · bd.a(SS)V.
                body_unlink: 0x0015_b43c,
                body_retile: 0x0010_f230,
                body_link: 0x0015_b2c0,
                body_move: 0x0015_b5a0,
                body_can_stand: 0x0012_ccf8,
                body_relocate: 0x0012_5fc4,
                char_slot: 0x0019_3b5c,
                class_slot: 0x0019_3a6c,
                stage_slot: 0x0019_3afc,
                combat_map_slot: 0x0019_400c,
                map_w_slot: 0x0019_3fcc,
                map_h_slot: 0x0019_3fdc,
                frames_slot: 0x0019_45bc,
                cam_centre: (0x0019_4afc, 0x0019_4b0c),
                cam_target: (0x0019_3b0c, 0x0019_3b1c),
                cam_live: (0x0019_3b2c, 0x0019_3b3c),
                monster_descs: [0x0019_2670, 0x0019_23e8, 0x0019_29ac, 0x0019_2acc, 0x0019_3090],
                front_diag: [[0, 6, 7, 5, 8], [0, 5, 8, 7, 6]],
                code_dx: [0, 0, 0, -1, 1, -1, 1, -1, 1],
                code_dy: [0, -1, 1, 0, 0, -1, -1, 1, 1],
                // bn.a(BIBI)V · bn.g()V (both private: the direct helper reads word 0 too) · ce.a(BII)V.
                ring_spawn: (0x0018_8d48, 0x0013_4d3c),
                assassin: (0x0018_8d64, 0x0013_4e40),
                scatter: (0x0018_a280, 0x0013_fc64),
                // bn.a(BIBI)V's own: `*(sl+0x20) + 0x49c` → 0x167c10 · ce.<init> · bf.d(Lad;)V;
                // bn.g()V's own: bn.a(BBBI)V.
                new_instance: 0x0016_7c10,
                direct_call: 0x0016_9160,
                class_table_slot: 0x0019_5cc4,
                ce_class_off: 0x49c,
                cell_ce_init: 0x0017_b070,
                cell_stage_add: 0x0017_b074,
                cell_assassin_tile: 0x0017_b078,
                menu_slot: 0x0019_422c,
                text_table_desc: 0x0019_2fdc,
                // ay.a(BBI)Z @0x11fca4: bl 0x16917c through the cell 0x1780c0 (literal 0xfffe241c @0x11fd1c, once
                // in the image) → a.a()Lbd; definition 0x18bba8 (index 927), body 0x151330 (= ay/bl/bx's)
                target_query: (0x0011_fca4, 0x0017_80c0, 0x0018_bba8, 0x0015_1330),
                // ★★ 2026-10-14: cd.a(II)Z def 0x18a184 (index 688) · cd.a(II)V def 0x18a1a0 (689) — word 0 relocated = body | 1;
                // the set name's call 0x15f5ee..0x15f602 (cell 0x1803a0 = ac.a(Graphics,II[BII)V, body 0x10e2f8, args 1 · 8);
                // cf.c `/t/itmtp` @0x19466c (Hero2Java::ITEM_TYPE_TABLE_SLOT).
                detail_key: (0x0018_a184, 0x0013_f134),
                detail_paint: (0x0018_a1a0, 0x0013_f190),
                anchor_text_cell: 0x0018_03a0,
                item_text_slot: 0x0019_466c,
                // ★★ 2026-10-14: n.d()V def #1165 (= SkyDrawPatch::hud_draw).
                hud_draw: (0x0018_d5b0, 0x0016_3964),
                // ★★ 2026-10-17: bd.a(IB)V's cell (= DemonPatch::cell_damage · VenomPatch's tick).
                cell_damage: 0x0017_9670,
            }),
        }
    }

    /// hero2's 의상 아바타 override on the parts loader (2026-09-30) — see [`AvatarPatch`].
    /// Every address is `cf.a(I)V`'s own (`tools_re/hero2_dis.py 'cf.a(I)V'`; the jump
    /// table @0x170a6c: case 1 → 0x153f8c, case 2 → 0x154128, case 3 → 0x15422c).
    pub fn avatar_patch(self) -> Option<AvatarPatch> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some(AvatarPatch {
                regions: [
                    // ── body: case 1's record path 0x153f9c..0x153fcc, builder at 0x153fcc,
                    //    null stub 0x154114 (`sb = 100 + class`) ending `b 0x153fcc` @0x154126.
                    AvatarRegion {
                        part: 1,
                        at: 0x0015_3f9c,
                        shipped: &[
                            0x4858, 0x4641, 0x4450, 0xf015, 0xf8eb, // ldr r0,=a.b; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x4856, 0x1c39, 0x4450, 0xf014, 0xf879, // adds r7,r0; ldr r0,=bu.l; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4854, 0x4641, 0x4450, 0xf015, 0xf8df, // ldr r0,=a.b; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x4852, 0x1c39, 0x4450, 0xf014, 0xf86d, // adds r7,r0; ldr r0,=bu.a; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, // str r0,[sp,#0x28]
                        ],
                        patched: &[
                            0x1c07, 0x4859, 0x1c39, 0x4450, 0xf014, 0xf87e, // adds r7,r0; ldr r0,=bu.l (0x154104); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4858, 0x1c39, 0x4450, 0xf014, 0xf878, // ldr r0,=bu.a (0x15410c); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, // str r0,[sp,#0x28]
                            0x4b04, 0x681a, 0x2a00, 0xdb06, // 0x153fb6: ldr r3,=words; ldr r2,[r3]; cmp r2,#0; blt 0x153fcc
                            0x4691, 0x685a, 0x920a, // mov sb,r2; ldr r2,[r3,#4]; str r2,[sp,#0x28]
                            0xe002, 0x46c0, // b 0x153fcc; nop
                            0x0000, 0x0000, // 0x153fc8: .word block + 0
                        ],
                        lit_at: 0x0015_3fc8,
                        words_off: 0,
                        branch: Some((0x0015_4126, 0xe751, 0xe746)), // b 0x153fcc -> b 0x153fb6 (the check)
                    },
                    // ── arms: case 2's record path 0x1541fa..0x15422c (ends `b 0x15413a`); the
                    //    null path `movs r3,#0x64; mov sb,r3` @0x154136 falls into the builder at
                    //    0x15413a, whose first two halfwords (`ldr r0,=0x20; mov r4,sl`) move into
                    //    the region; the head becomes `b check`, the region rejoins at 0x15413e.
                    AvatarRegion {
                        part: 2,
                        at: 0x0015_41fa,
                        shipped: &[
                            0x4894, 0x4641, 0x4450, 0xf014, 0xffbc, // ldr r0,=a.d; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x4892, 0x1c39, 0x4450, 0xf013, 0xff4a, // adds r7,r0; ldr r0,=bu.l; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4890, 0x4641, 0x4450, 0xf014, 0xffb0, // ldr r0,=a.d; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x488e, 0x1c39, 0x4450, 0xf013, 0xff3e, // adds r7,r0; ldr r0,=bu.a; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, 0xe786, // str r0,[sp,#0x28]; b 0x15413a
                        ],
                        patched: &[
                            0x1c07, 0x4894, 0x1c39, 0x4450, 0xf013, 0xff4f, // adds r7,r0; ldr r0,=bu.l (0x154450); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4893, 0x1c39, 0x4450, 0xf013, 0xff49, // ldr r0,=bu.a (0x154458); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, // str r0,[sp,#0x28]
                            0x4b04, 0x681a, 0x2a00, 0xdb02, // 0x154214: ldr r3,=words; ldr r2,[r3]; cmp r2,#0; blt 0x154222
                            0x4691, 0x685a, 0x920a, // mov sb,r2; ldr r2,[r3,#4]; str r2,[sp,#0x28]
                            0x4882, 0x4654, 0xe78a, // 0x154222: ldr r0,=0x20 (0x15442c); mov r4,sl; b 0x15413e
                            0x0000, 0x0000, // 0x154228: .word block + 8
                        ],
                        lit_at: 0x0015_4228,
                        words_off: AvatarPatch::SLOT,
                        branch: Some((0x0015_413a, 0x48bc, 0xe06b)), // ldr r0,[pc,#0x2f0] -> b 0x154214 (the check)
                    },
                    // ── legs: case 3's record path 0x15423c..0x15426c, builder at 0x15426c, null
                    //    stub 0x15432c (`sb = 100 + class`) ending `b 0x15426c` @0x15433e.
                    AvatarRegion {
                        part: 3,
                        at: 0x0015_423c,
                        shipped: &[
                            0x4888, 0x4641, 0x4450, 0xf014, 0xff9b, // ldr r0,=a.e; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x4886, 0x1c39, 0x4450, 0xf013, 0xff29, // adds r7,r0; ldr r0,=bu.l; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4884, 0x4641, 0x4450, 0xf014, 0xff8f, // ldr r0,=a.e; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x4882, 0x1c39, 0x4450, 0xf013, 0xff1d, // adds r7,r0; ldr r0,=bu.a; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, // str r0,[sp,#0x28]
                        ],
                        patched: &[
                            0x1c07, 0x4889, 0x1c39, 0x4450, 0xf013, 0xff2e, // adds r7,r0; ldr r0,=bu.l (0x154464); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4888, 0x1c39, 0x4450, 0xf013, 0xff28, // ldr r0,=bu.a (0x15446c); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, // str r0,[sp,#0x28]
                            0x4b04, 0x681a, 0x2a00, 0xdb06, // 0x154256: ldr r3,=words; ldr r2,[r3]; cmp r2,#0; blt 0x15426c
                            0x4691, 0x685a, 0x920a, // mov sb,r2; ldr r2,[r3,#4]; str r2,[sp,#0x28]
                            0xe002, 0x46c0, // b 0x15426c; nop
                            0x0000, 0x0000, // 0x154268: .word block + 16
                        ],
                        lit_at: 0x0015_4268,
                        words_off: 2 * AvatarPatch::SLOT,
                        branch: Some((0x0015_433e, 0xe795, 0xe78a)), // b 0x15426c -> b 0x154256 (the check)
                    },
                    // ── ★ A3b head: case 0's record path 0x153f5a..0x153f8c (25 hw, ends
                    //    `b 0x153d6a`); the no-helmet path `movs r0,#0x64; mov sb,r0` @0x153d66
                    //    falls into the builder at 0x153d6a, whose first two halfwords
                    //    (`ldr r0,=0x20; mov r1,sl`) move into the region — the arms' shape.
                    AvatarRegion {
                        part: 0,
                        at: 0x0015_3f5a,
                        shipped: &[
                            0x4864, 0x4641, 0x4450, 0xf015, 0xf90c, // ldr r0,=a.c; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x4862, 0x1c39, 0x4450, 0xf014, 0xf89a, // adds r7,r0; ldr r0,=bu.l; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4860, 0x4641, 0x4450, 0xf015, 0xf900, // ldr r0,=a.c; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x485e, 0x1c39, 0x4450, 0xf014, 0xf88e, // adds r7,r0; ldr r0,=bu.a; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, 0xe6ee, // str r0,[sp,#0x28]; b 0x153d6a
                        ],
                        patched: &[
                            0x1c07, 0x4864, 0x1c39, 0x4450, 0xf014, 0xf89f, // adds r7,r0; ldr r0,=bu.l (0x1540f0); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x4681, // mov sb,r0
                            0x4863, 0x1c39, 0x4450, 0xf014, 0xf899, // ldr r0,=bu.a (0x1540f8); adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900a, // str r0,[sp,#0x28]
                            0x4b04, 0x681a, 0x2a00, 0xdb02, // 0x153f74: ldr r3,=words; ldr r2,[r3]; cmp r2,#0; blt 0x153f82
                            0x4691, 0x685a, 0x920a, // mov sb,r2; ldr r2,[r3,#4]; str r2,[sp,#0x28]
                            0x4846, 0x4651, 0xe6f2, // 0x153f82: ldr r0,=0x20 (0x15409c); mov r1,sl; b 0x153d6e
                            0x0000, 0x0000, // 0x153f88: .word block + 36
                        ],
                        lit_at: 0x0015_3f88,
                        words_off: 3 * AvatarPatch::SLOT,
                        branch: Some((0x0015_3d6a, 0x48cc, 0xe103)), // ldr r0,[pc,#0x330] -> b 0x153f74 (the check)
                    },
                    // ── ★ A3b weapon: case 6's reads 0x15453e..0x154596 (44 hw; `r7 = r0` = the
                    //    record from 0x154536), builder at 0x154596. The reads once each, the
                    //    swing sound `cf.a(B)V(type)` as shipped, THEN the check — type word
                    //    included. An empty slot returns at 0x15453c (no branch).
                    AvatarRegion {
                        part: 6,
                        at: 0x0015_453e,
                        shipped: &[
                            0x48be, 0x4641, 0x4450, 0xf014, 0xfe1a, // ldr r0,=a.a()Lbo; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x48bc, 0x1c39, 0x4450, 0xf013, 0xfda8, // adds r7,r0; ldr r0,=ch.g; adds r1,r7; add r0,sl; bl 0x1680a4
                            0x900b, // str r0,[sp,#0x2c]
                            0x48ba, 0x4641, 0x4450, 0xf014, 0xfe0e, // ldr r0,=a.a()Lbo; …
                            0x1c07, 0x48b8, 0x1c39, 0x4450, 0xf013, 0xfd9c, // adds r7,r0; ldr r0,=bu.l; …
                            0x4681, // mov sb,r0
                            0x48b6, 0x4641, 0x4450, 0xf014, 0xfe02, // ldr r0,=a.a()Lbo; …
                            0x1c07, 0x48b4, 0x1c39, 0x4450, 0xf013, 0xfd90, // adds r7,r0; ldr r0,=bu.a; …
                            0x9f0d, 0x9c0b, 0x900a, // ldr r7,[sp,#0x34]; ldr r4,[sp,#0x2c]; str r0,[sp,#0x28]
                            0x48b1, 0x1c39, 0x1c22, 0x4450, 0xf014, 0xfdf3, // ldr r0,=cf.a(B)V; adds r1,r7; adds r2,r4; add r0,sl; bl 0x16917c
                        ],
                        patched: &[
                            0x48bf, 0x1c39, 0x4450, 0xf013, 0xfdae, 0x900b, // ldr r0,=ch.g (0x15483c); adds r1,r7; add r0,sl; bl 0x1680a4; str r0,[sp,#0x2c]
                            0x48be, 0x1c39, 0x4450, 0xf013, 0xfda8, 0x4681, // ldr r0,=bu.l (0x154844); …; mov sb,r0
                            0x48bd, 0x1c39, 0x4450, 0xf013, 0xfda2, 0x900a, // ldr r0,=bu.a (0x15484c); …; str r0,[sp,#0x28]
                            0x9f0d, 0x9c0b, 0x48ba, 0x1c39, 0x1c22, 0x4450, 0xf014, 0xfe05, // the swing sound, as shipped (cf.a(B)V @0x154850)
                            0x4b05, 0x681a, 0x2a00, 0xdb0d, // 0x154572: ldr r3,=words; ldr r2,[r3]; cmp r2,#0; blt 0x154596
                            0x4691, 0x685a, 0x920a, // mov sb,r2; ldr r2,[r3,#4]; str r2,[sp,#0x28]
                            0x689a, 0x920b, // ldr r2,[r3,#8]; str r2,[sp,#0x2c] (the path's type)
                            0xe007, 0x46c0, // b 0x154596; nop
                            0x0000, 0x0000, // 0x154588: .word block + 48
                            0x46c0, 0x46c0, 0x46c0, 0x46c0, 0x46c0, // unreached
                        ],
                        lit_at: 0x0015_4588,
                        words_off: 4 * AvatarPatch::SLOT,
                        branch: None,
                    },
                    // ── ★ A3b off-hand: case 7's reads 0x154696..0x1546dc (35 hw; the record is
                    //    in r0 after the null check at 0x15466e). The builder begins with the
                    //    last read's store interleaved (`str r0,[sp,#0x28]` @0x1546e2), so the
                    //    check leaves the palette in r0. An empty slot returns (the rogue's
                    //    after `cf.a(Z)V(false)`, no branch).
                    AvatarRegion {
                        part: 7,
                        at: 0x0015_4696,
                        shipped: &[
                            0x4873, 0x4641, 0x4450, 0xf014, 0xfd6e, // ldr r0,=a.a()Lbu; mov r1,r8; add r0,sl; bl 0x16917c
                            0x1c07, 0x4871, 0x1c39, 0x4450, 0xf013, 0xfcfc, // adds r7,r0; ldr r0,=ch.g; …
                            0x900b, // str r0,[sp,#0x2c]
                            0x486f, 0x4641, 0x4450, 0xf014, 0xfd62, // ldr r0,=a.a()Lbu; …
                            0x1c07, 0x486d, 0x1c39, 0x4450, 0xf013, 0xfcf0, // adds r7,r0; ldr r0,=bu.l; …
                            0x4681, // mov sb,r0
                            0x486b, 0x4641, 0x4450, 0xf014, 0xfd56, // ldr r0,=a.a()Lbu; …
                            0x1c07, 0x4869, 0x1c39, 0x4450, 0xf013, 0xfce4, // adds r7,r0; ldr r0,=bu.a; …
                        ],
                        patched: &[
                            0x1c07, // adds r7,r0
                            0x4873, 0x1c39, 0x4450, 0xf013, 0xfd01, 0x900b, // ldr r0,=ch.g (0x154868); …; str r0,[sp,#0x2c]
                            0x4872, 0x1c39, 0x4450, 0xf013, 0xfcfb, 0x4681, // ldr r0,=bu.l (0x154870); …; mov sb,r0
                            0x4871, 0x1c39, 0x4450, 0xf013, 0xfcf5, // ldr r0,=bu.a (0x154878); … — the palette stays in r0
                            0x4b04, 0x681a, 0x2a00, 0xdb0c, // 0x1546ba: ldr r3,=words; ldr r2,[r3]; cmp r2,#0; blt 0x1546dc
                            0x4691, 0x6858, 0x689a, 0x920b, // mov sb,r2; ldr r0,[r3,#4]; ldr r2,[r3,#8]; str r2,[sp,#0x2c]
                            0xe007, // b 0x1546dc
                            0x0000, 0x0000, // 0x1546cc: .word block + 60
                            0x46c0, 0x46c0, 0x46c0, 0x46c0, 0x46c0, 0x46c0, // unreached
                        ],
                        lit_at: 0x0015_46cc,
                        words_off: 5 * AvatarPatch::SLOT,
                        branch: None,
                    },
                ],
            }),
        }
    }

    /// The extra field-gate bytes, when this build needs them. See [`FieldGate`].
    pub fn field_gate(self) -> Option<FieldGate> {
        match self {
            Self::Ktf | Self::Lgt => Some(FieldGate {
                window_id_off: 0xa0c0,
                window_none: 3,
                selector_off: 0x9c6c,
            }),
            Self::Skt => None,
            // hero2 needs none — its state word already separates field from window.
            Self::Hero2Ktf => None,
        }
    }

    /// Offset of the coarse game-state byte inside the character record.
    ///
    /// The one struct offset that did NOT survive the KTF → LGT revision gap:
    /// everything else this crate reads (level/exp/hp/sp, the inventory grid handle
    /// at `+0x9e28`, the window id at `+0xa0c0`) sits at the same offset in both,
    /// but the state byte moved back one word. Measured the same way — 4 while
    /// free-roaming, 5 with the menu open, matching KTF's `STATE_FIELD` /
    /// `STATE_SELECTOR`.
    pub fn state_off(self) -> u32 {
        match self {
            Self::Lgt => 0x78,
            _ => 0x7c,
        }
    }

    /// How a field that holds a WIPI allocation references its payload.
    ///
    /// `Some(off)` — the field is a HANDLE and the payload is `*(handle) + off`.
    /// `None` — the field is already the payload address.
    ///
    /// ★ This is NOT a property of the game's compile but of the carrier runtime the
    /// game is linked against, which is why one flag settles it for every container
    /// at once (inventory grid, area object, item DB, map arrays, framebuffers):
    ///   * `wie_ktf`'s `WIPICContext::data_ptr` is `*(handle) + 8`
    ///     (`vendor/wie/wie_ktf/src/runtime/wipi_c/context.rs`), so everything the
    ///     game gets back from `MC_knlCalloc` / `MC_grpCreateOffScreenFrameBuffer`
    ///     is a handle;
    ///   * `wie_lgt`'s is the IDENTITY (`Ok(memory.0)`), so the same allocation call
    ///     hands the game a direct pointer.
    /// Independently corroborated by the sprite-blitter port, where the descriptor's
    /// pixel/palette words needed exactly this split
    /// (`blit565::SpriteLayout::{KtfHandles, LgtDirect}`, MIGRATION_LGT.md §2.4).
    pub fn container_data_off(self) -> Option<u32> {
        match self {
            Self::Lgt => None,
            // SKT is untested; the KTF shape is the safer default because every
            // caller validates each hop lands in the guest heap and gives up if not.
            _ => Some(8),
        }
    }

    /// Where `popup_handle[8]` lives on this build, or `None` where it has not been
    /// located. See [`PopupHandles`] for the two shapes.
    ///
    /// ── WHY THIS MATTERS MORE THAN IT LOOKS (2026-08-11, 7차) ────────────────────
    /// It is the ONLY thing standing between LGT and three of §2.11's matrix rows.
    /// Every cash buff this app supplies — EXP x2/x4 (#9), the equip-level relief
    /// (#10), the random boxes' full restore (#16) — is granted from exactly one
    /// place, `items::cash_use_pending`, and that reads the highlighted popup row's
    /// label through this array. On LGT the KTF slot address is not even mapped
    /// (device-measured: `handles=None`), so no buff can ever start, which makes the
    /// EXP-divisor and relief patch sites unreachable however correct they are.
    ///
    /// LGT value found by LIFETIME, not by address (MIGRATION_LGT.md §3.3(b) widened
    /// from a word to a run): `free_popup` NULLs every handle, so with a popup open
    /// the array is N consecutive plausible heap pointers and after the close the same
    /// words are zero. Diffing `.bss` between an open-popup frame and a closed-popup
    /// frame gave **exactly one** run in the whole section — eight words at
    /// `0x1512934` — and dereferencing them produced the two rows that were on screen,
    /// `상세보기` (`bb f3 bc bc ba b8 b1 e2 00`) and `버리기`, in EUC-KR, with the
    /// remaining six pointing at allocated-but-zeroed 10-byte buffers.
    ///
    /// ★ The label buffers are reached with ONE hop fewer, and that falls out of
    /// [`Self::container_data_off`] rather than being a second discovery: the dumped
    /// pointers land directly on the text, with no 8-byte allocator header in front.
    pub fn popup_handles(self) -> Option<PopupHandles> {
        match self {
            Self::Ktf => Some(PopupHandles::Pointer { global: 0x1b_2df4 }),
            Self::Lgt => Some(PopupHandles::Static(0x151_2934)),
            Self::Skt => None,
            // ⛔ hero2: a different game. Every address in this arm's siblings was
            // reverse engineered from hero3, so hero2 answers CLOSED — see `Self::Hero2Ktf`.
            Self::Hero2Ktf => None,
        }
    }

    /// CHAR-relative bytes that PRE-EMPT the inventory page's key routing: with any of
    /// them non-zero the page never routes a key into its popup handler, so the popup
    /// on screen is not really live. `items::popup_view` refuses the ITEM popup while
    /// one is set. Empty = no pre-emptor is checked on this build.
    ///
    /// ── WHY THIS IS PER-BUILD, AND WHY LGT IS EMPTY (2026-08-15, 51차) ────────────
    /// The check exists for KTF's commit shape: [`PopupCommit::GuestCall`] BYPASSES the
    /// game's own key router (guest `0x13aa24`/`0x13aa56` is where these bytes are
    /// tested), so the host must apply the same refusal itself or it commits a row into
    /// a popup the game considers dead. LGT's [`PopupCommit::SyntheticOk`] is the
    /// opposite by construction: the key travels the game's OWN router, pre-emptors
    /// included, so the game applies whatever its real gate is — the host checking a
    /// guessed address adds nothing it does not already get.
    ///
    /// And the addresses WERE only guesses on LGT. Both offsets are KTF measurements;
    /// `0xa220` in particular sits outside the popup cluster the 647-callsite analysis
    /// verified on LGT (`+0x9c6c..+0x9c87`) and inside the region where the LGT record
    /// is DOCUMENTED to shift (`0xa384`→`0xa370`, `0xa3f8`→`0xa410`), so reading it on
    /// LGT reads some other live field. That is the device report this row fixes
    /// (2026-08-15): skills relabel to 퀵슬롯등록 but consumables never do — the skill
    /// popup skips this gate entirely, while a stray non-zero byte at the KTF offset
    /// failed the item popup every time. §2.16's probe passed all ten gates only
    /// because a fresh-boot character happens to have zeros there.
    pub fn popup_preempt_offsets(self) -> &'static [u32] {
        match self {
            Self::Ktf => &[0x9c89, 0xa220],
            // The real LGT pre-emptors (if that compile has any) are unlocated. Locate
            // them the §3.3(b) way — diff the record between a routed and a pre-empted
            // key state — before ever putting an address here.
            Self::Lgt => &[],
            Self::Skt => &[],
            // ⛔ hero2: a different game. Every address in this arm's siblings was
            // reverse engineered from hero3, so hero2 answers CLOSED — see `Self::Hero2Ktf`.
            Self::Hero2Ktf => &[],
        }
    }

    /// How to commit a popup row on this build, or `None` where neither mechanism has
    /// been established. See [`PopupCommit`].
    ///
    /// ── WHY LGT DOES NOT NEED A GUEST ADDRESS (2026-08-11, 8차) ───────────────────
    /// The 퀵슬롯등록 rewrite looked like it was blocked on finding LGT's `popup_key`,
    /// because that is what KTF calls. It was not: the host already synthesises `OK` for
    /// the popup's row 0 (`emulator::InputEvent::PopupConfirm`), so the same synthesis
    /// commits ANY row once the row byte says which one. A probe A/B proved it (see
    /// [`PopupCommit::SyntheticOk`]), which removes a whole reverse-engineering step and
    /// a whole class of risk — a stale call address jumps into unrelated code, a key
    /// press cannot.
    ///
    /// KTF keeps its measured call: it is proven in the field and the two mechanisms are
    /// not interchangeable in one detail (a direct call leaves no tick in which the guest
    /// could re-clamp the row byte).
    pub fn popup_commit(self) -> Option<PopupCommit> {
        match self {
            // `popup_key(kind, key)`; the jump table at guest 0x1a745c is indexed by
            // `key + 0x10` and -5 is its select arm (0x13a8be).
            Self::Ktf => Some(PopupCommit::GuestCall {
                fn_addr: 0x0013_a86c | 1,
                select_key: (-5i32) as u32,
            }),
            Self::Lgt => Some(PopupCommit::SyntheticOk),
            Self::Skt => None,
            // ★ hero2 (2026-09-17): the popup's OK handler (SKT `ba.a(int,int)`) reads the
            // popup's own cursor `d.h` and hands it to the parent window's callback, so a
            // synthesised OK after the cursor write is the whole commit — no address.
            // The rewrite itself is gated per write by [`GuestBuild::java_write_ok`].
            Self::Hero2Ktf => Some(PopupCommit::SyntheticOk),
        }
    }

    /// The global whose value SUPPRESSES the minimap's corner blend, when it is known
    /// for this build (guest `0x17b97e` on KTF reads `sl - 0x8084 + 8`). `None` = not
    /// located on this build, so the blend is never suppressed — which is also what
    /// KTF does in practice, since the word there holds none of the three tile
    /// colours. `wie_probe --map-check` is what proves the choice right.
    pub fn minimap_noblend_global(self) -> Option<u32> {
        match self {
            Self::Ktf => Some(0x001a_4bc4),
            _ => None,
        }
    }

    /// The one-instruction patch that stops the game drawing its own BOTTOM field HUD
    /// (panel + HP/SP/EXP bars + the 5 quickbar cells), all of which the overlay
    /// replaces: `(site, original_halfword, branch_halfword)`.
    ///
    /// The caller writes `branch` over `site` only after reading back `original`, so a
    /// revision this was never measured on is left alone. `None` = not located on this
    /// build, and then the game keeps drawing its own HUD (which is a cosmetic
    /// duplicate of the overlay's, not a malfunction).
    ///
    /// ── WHY THE SAME PATCH EXISTS TWICE ─────────────────────────────────────────
    /// The KTF and LGT `.text` share no bytes (MIGRATION_LGT.md §1) but they are
    /// compiled from the same C, so the block is recognisable by its ARITHMETIC. Both
    /// builds compute the panel's origin as `((s16)screen_w - 0xaa)/2 + 2` and
    /// `screen_h - 0x18`, and in both the instruction that loads `screen_w` for that
    /// sum is the first instruction of the block — everything above it (buff band,
    /// target-monster box) has already been drawn, and everything below it is the
    /// bottom panel. Branching from there to the function's own epilogue drops exactly
    /// that block.
    ///
    /// KTF `client.bin` (Ver 01.00.03), renderer `0x149b30`:
    ///   `0x149f7a  ldr r3,[sp,#0x100]` → `b 0x14a324`  (imm11 467 = 0xe1d3)
    ///
    /// LGT `binary.mod` (Ver 01.00.07), renderer **`0x22a4`..`0x3050`**
    /// (`push {r4-r7,lr}` / `sub sp,#0xd4` … `add sp,#0xd4; pop {r4-r7}; pop {r0};
    /// bx r0`), located 2026-08-10 by searching the ELF for `subs rX,#0xaa` — 14 hits,
    /// exactly one of them followed by `asrs #1` + `adds #2`:
    ///   ```
    ///   0x2860  bl  0x5a6fc          ; last draw of the target box (bx r5 veneer)
    ///   0x2864  ldr r5,[sp,#0x94]    ; <= PATCH SITE (screen_w)
    ///   0x2868  lsls r5,r5,#0x10
    ///   0x286a  asrs r3,r5,#0x10
    ///   0x286c  subs r3,#0xaa
    ///   0x286e  asrs r3,r3,#1
    ///   0x2870  adds r3,#2
    ///   ...
    ///   0x287c  subs r3,#0x18        ; screen_h - 0x18
    ///   ```
    ///   `b 0x304a` = imm11 `(0x304a - 0x2868)/2` = 0x3f1 → `0xe3f1`.
    ///
    /// ── MEASURED, NOT ARGUED (wie_probe, 2026-08-10) ────────────────────────────
    /// Two deterministic LGT runs differing only by `--poke 7860:0x2864:f1e3`:
    ///   * the halfword at the site read back `0x9d25`, as the static disassembly said;
    ///   * the bottom panel disappears and the world shows through it (the pond and its
    ///     lily pads continue into the rows the panel used to own);
    ///   * the 45 KB character record is **byte-identical** in both runs at every
    ///     sampled frame — the skipped block writes no game state;
    ///   * the only other bytes that differ are `.bss 0x1500000`/`0x1500004`, whose
    ///     ONLY pc-relative loaders (`0x2f64`, `0x2f78`, `0x2fa8`, `0x2ffc`) are inside
    ///     the skipped range, i.e. HUD-private blink counters — the same finding KTF's
    ///     adoption note records for `0x14a2a0`/`0x14a2bc`/`0x14a2d0` — plus one word
    ///     at `.bss 0x150ec58` that the block caches and nothing else writes (its
    ///     neighbourhood holds the screen geometry at `0x150ec88`/`0x150ec8c`).
    pub fn hud_bottom_patch(self) -> Option<(u32, u16, u16)> {
        match self {
            Self::Ktf => Some((0x14_9f7a, 0x9b40, 0xe1d3)),
            Self::Lgt => Some((0x00_2864, 0x9d25, 0xe3f1)),
            Self::Skt => None,
            // ★★ hero2 (2026-09-18, user: "hero3처럼 페인터 자체를 끄고싶어"): the bottom
            // HUD is the AOT Java method **`n.d()V` @ 0x163964** — it opens with
            // `(ax.g − 120) / 2` (the 120-px panel's x) and `ax.i − 26` (the band's y),
            // then draws the panel frame (`n.b(II)V`), the HP/SP bars and the six quick
            // cells (`n.d(BII)V`, which reads `ag.j`/`ag.a:[I`). Called from `n.paint`
            // (field), `n.a(Lj;)V` (under every window) and `bb.a(Graphics)` (under the
            // dialogue box), so ONE early return covers all three.
            //
            // The patch is the method's first halfword: `push {r4-r7, lr}` (0xb5f0) →
            // `bx lr` (0x4770). Size-neutral; the caller's `bl` returns to the next
            // instruction with nothing pushed or popped.
            //
            // ── MEASURED, NOT ARGUED (wie_probe, 2026-09-18) ────────────────────────
            // Two deterministic runs differing only by `--poke 540:0x163964:7047`
            // (field → menu → STATUS → back, 1500 frames):
            //   * the halfword read back `0xf0b5` (LE of 0xb5f0), as the disassembly said;
            //   * the band rows are empty in every dumped frame; the world above and every
            //     window are pixel-identical between the runs;
            //   * the gauges (lv/hp/sp/exp) and the state word agree at every sample;
            //   * of the 1,3xx PRIMITIVE statics, exactly ONE differs: `n.a:Z` @0x194b1c
            //     (0 → 1) — the HUD's own "redraw me" flag, set by `keyNotify` /
            //     `ag.m()` / `bb.g()` and consumed only by the `n.d` family. The cooldown
            //     tick (`ag.i()`) is not on this path.
            // Found with `tools_re/hero2_dis.py` (cell decoding) — `HERO2_GUEST_RE.md` §4.
            //
            // ★ 2026-09-24: the painter also drew the PICKUP NOTICE (`HERO2_GUEST_RE.md`
            // §4.2) — a notice-only variant of this patch was built and verified that day
            // and then superseded the same evening: the user wanted the notices STACKED at
            // the left edge, which the game's one-image notice cannot do, so the HOST
            // draws them (`hero2java::h2_notice_pending`, `emulator::NoticeFeed`).
            //
            // ★★ 2026-09-25 (user: "몹 잡을때 몹 이름이랑 피통이 안보이는데 hero3처럼 미니맵
            // 왼쪽에 몹이름이랑 피통이 나오도록"): the painter ALSO drew the TARGET-MONSTER
            // panel, and the entry `bx lr` took it away with everything else. So the patch
            // is no longer an early return but hero3's shape — branch OVER what the host
            // replaces, keep the one widget it does not — in two halfwords, this HEAD and
            // [`Self::hud_bottom_tail`]:
            //
            // ```text
            // 0x163964  push … ; ax.g/ax.i/ag.a loaded, fp = ax.i − 26   (kept: setup only)
            // 0x1639a0  str r0,[sp,#0x1c]  -> b 0x1639f6   (0xe029)  <= THIS: skip the
            //           stat-point badge blink (bottom band), go straight to the panel test
            // 0x1639f6  if (n.e > 0 && n.a:Lbd; != null) b 0x1640c4   ; the panel, see
            //           `target_box_x_sites`; it ends in `bl 0x163a1e` (a far `b`)
            // 0x163a14  n.e()V  — the idle arm: clears the target + its image (no drawing)
            // 0x163a1e  ldr r0,[pc,#0x2e4] -> b 0x163f14 (TAIL) — the pickup notice, the
            //           panel frame, HP/SP bars and quick cells are never reached
            // ```
            // The panel keeps its own lifetime this way: `n.a(Lbd;Z)V` (a hit, either way
            // round) sets `n.e ← 24` and builds the 66x28 name/level image, the panel
            // decrements it per paint, and a dead target (`g.t == 7`) takes the idle arm.
            // Under `bx lr` none of that ran — the timer stuck at 24 and the last target
            // stayed referenced until the next one replaced it.
            //
            // ⛔ The HEAD must never land without the tail: alone it runs the panel and then
            // falls into the notice block and the whole bottom HUD. `items::hud_bottom_patch`
            // writes the tail first and the head only once the tail reads as patched.
            // Verified headless (`--h2-hud`, slot 2 forest): 「반란전투병 LV22」 + its bar at
            // the top right while the fight lasts, the bottom rows empty, notices untouched.
            Self::Hero2Ktf => Some((0x0016_39a0, 0x9007, 0xe029)),
        }
    }

    /// The SECOND halfword of a two-site bottom-HUD patch, `(site, original, branch)` —
    /// written BEFORE [`Self::hud_bottom_patch`], which must never land alone. `None` on
    /// every build whose one branch already lands on the epilogue (hero3's KTF/LGT).
    ///
    /// hero2 (2026-09-25): `0x163a1e` is where the target-monster panel and its idle arm
    /// meet — the panel reaches it through `bl 0x163a1e` (a far branch; `lr` was saved in
    /// the prologue) and the idle `n.e()V` call falls through into it — and it is the first
    /// instruction of the pickup-notice block. `b 0x163f14` = imm11 `(0x163f14 − 0x163a22)
    /// / 2` = 0x279, the method's own epilogue (`add sp,#0x24; pop …`), the same target the
    /// 2026-09-24 notice variant's tail used from `0x163ac0`. Stack depth is identical at
    /// both ends: the panel only uses `[sp]`, `[sp+4]` and `[sp+0xc]` inside the 0x24 frame.
    pub fn hud_bottom_tail(self) -> Option<(u32, u16, u16)> {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => None,
            Self::Hero2Ktf => Some((0x0016_3a1e, 0x48b9, 0xe279)),
        }
    }

    /// The bottom-HUD patch as an ORDERED write plan, `(address, halfword_le)`: the tail
    /// (when the build has one) and then the head, each only while it still reads as
    /// shipped. Empty once everything is in place, and empty outright when the tail reads
    /// as neither shipped nor patched — not this build's code, so nothing is touched.
    ///
    /// ⛔ The head is planned only behind a tail that is in place or in this same plan: on
    /// hero2 the head alone runs the target panel and then falls into the pickup notice
    /// and the whole bottom HUD. The caller writes in order and stops at the first failed
    /// write, which is what keeps a failed tail from ever leaving the head landed.
    pub fn hud_bottom_writes(self, read: &impl Fn(u32, usize) -> Vec<u8>) -> Vec<(u32, [u8; 2])> {
        let rd = |a: u32| {
            let b = read(a, 2);
            (b.len() == 2).then(|| u16::from_le_bytes([b[0], b[1]]))
        };
        let Some((site, orig, branch)) = self.hud_bottom_patch() else { return Vec::new() };
        let mut out = Vec::new();
        if let Some((tail, tail_orig, tail_branch)) = self.hud_bottom_tail() {
            match rd(tail) {
                Some(cur) if cur == tail_branch => {}
                Some(cur) if cur == tail_orig => out.push((tail, tail_branch.to_le_bytes())),
                _ => return Vec::new(),
            }
        }
        if rd(site) == Some(orig) {
            out.push((site, branch.to_le_bytes()));
        }
        out
    }

    /// The game's TARGET-MONSTER box in GUEST px, `(w, h)`, right-edge anchored at the top
    /// of the frame — what `overlay::game_target_box` turns into a window rect, and so what
    /// the minimap dodge and the editor's no-go zone are measured against.
    ///
    /// hero3 (KTF/LGT): the `ui_bm` frame-4 box is 85x26 at `(W − 87, 2)`; the host
    /// reserves `(W − 90, 0, 90, 30)` around it. hero2: `n.a(Lbd;Z)V` @0x164338 builds the
    /// panel as `Image.createImage(0x42, 0x1c)` = **66x28** and `n.d()V` draws it at
    /// `(ax.g − 0x42, 0)`, flush with the top and the right edge — no margin to reserve.
    pub fn target_box_guest_size(self) -> (u32, u32) {
        match self {
            Self::Ktf | Self::Lgt | Self::Skt => (90, 30),
            Self::Hero2Ktf => (66, 28),
        }
    }

    /// The network-warehouse deposit (`올리기` / `05 41`) GRADE gate, as
    /// `(site, orig_halfword, patched_halfword)`, or `None` when not located.
    ///
    /// ── WHAT IT IS (LGT `binary.mod` 01.00.07, RE 2026-08-13 from the player-supplied
    /// game zip, `binary.mod` md5-identical to the shipped one) ────────────────────
    /// The deposit handler validates the item by category. For EQUIPMENT (cat ≤ 0x0c)
    /// it loads the item's grade byte from the in-memory item DB —
    /// `grade = *(perCatBase[cat] + idx*0x20 + 0x10)` at `0x6f8a` — and at **`0x6f9a`**
    /// does `cmp grade, #3; ble 0x6fe6`. `0x6fe6` is the branch that BUILDS AND SENDS
    /// the `05 41` upload (it writes the bytes `0x05`,`0x41` then the 20-byte record
    /// through the frame serializer). Grade `> 3` falls through to `0x6f9e`, which
    /// shows **"아이템 등급이 영웅 이상인 아이템은 올릴 수 없습니다"** (rodata `0x5bd08`,
    /// the string that NAMES this gate) and returns without sending.
    ///
    /// The colour ladder is the grade: 0 무색 · 1 노랑 · 2 초록 · 3 하늘 · 4 금색/빨강
    /// (영웅) · 5 흰색. So the shipped client refuses every 영웅-이상 item — which is
    /// exactly the gold `}` and white `@` CASH equipment the shop sells (green `'`
    /// tier-2 gear deposits fine), the player's "네트워크 창고로 못 들어가고" report.
    ///
    /// ── THE PATCH ────────────────────────────────────────────────────────────────
    /// One size-neutral halfword: `cmp r3,#3` (`0x2b03`) → `cmp r3,#7` (`0x2b07`).
    /// Grade maxes at 5, so `ble` is then taken for every real item and all grades
    /// reach the same `05 41` upload the game already uses — nothing else changes, the
    /// record is the game's own, and the server stores it unchanged. Applied the
    /// `hud_bottom_patch` way (write only when the site still reads the original), so a
    /// build where `0x6f9a` is not this instruction is left untouched.
    ///
    /// KTF: not located (the shipped APK carries the LGT asset).
    pub fn warehouse_grade_gate(self) -> Option<(u32, u16, u16)> {
        match self {
            Self::Ktf => None,
            Self::Lgt => Some((0x00_6f9a, 0x2b03, 0x2b07)),
            Self::Skt => None,
            // ⛔ hero2: a different game. Every address in this arm's siblings was
            // reverse engineered from hero3, so hero2 answers CLOSED — see `Self::Hero2Ktf`.
            Self::Hero2Ktf => None,
        }
    }

    /// Address of the `movs r1,#100` that divides the kill-EXP formula, which the EXP
    /// x2 / x4 cash item halves and quarters. `None` = not located on this build.
    ///
    /// The instruction is the SAME halfword on both builds (`0x2164`), so only the
    /// address is per-build; `items::exp_multiplier_patch` still validates that what is
    /// there is a `movs r1,#imm8` whose immediate divides 100.
    ///
    /// KTF `client.bin`: `0x14654e`, inside `addExpFromKill` (`0x14650c`).
    ///
    /// LGT `binary.mod`: **`0x4d3cc`**, inside `addExpFromKill` = **`0x4d388`**, found
    /// 2026-08-10 by scanning the ELF for a window holding all four of the formula's
    /// constants (`#0xd`, `#0x82`, `#0xa`, `#0x64`) plus two `muls` — **one** hit in the
    /// whole binary. The twenty instructions then match KTF's one for one:
    /// `movs #0xd; muls; adds #0xb4; ldrsh; adds #0x82; muls; <<1; adds #0xa; muls;
    /// movs r1,#0x64; bl divide; if (gain<=0) gain=1; [r4,#0x30] += gain;
    /// [r4,#0x30] += enemy[0xb4]`.
    ///
    /// ★ Corroborated ARITHMETICALLY, which is stronger than the pattern match: the
    /// routine has exactly one call site in the image (it is reached through a function
    /// pointer, so the only reference is the literal `0x4d389` at `0x3a6b0`), and that
    /// caller does `r0 = getCHAR-ish(); r0 += 0x8c; addExpFromKill(r0, enemy);` then
    /// calls `checkLevelUp` (`0x4d714`). The accessor (`0x1068`) returns `0x1503ab0`,
    /// i.e. `CHAR + 0x28`, so `+0x8c` is `CHAR + 0xb4` — the stats sub-struct `H` this
    /// crate already documents — and the routine's `[r4,#0x30]` is therefore
    /// `CHAR + 0xe4`, which is exactly `items::EXP_OFF` as measured on LGT in §2.7.
    /// A wrong site could not produce that agreement.
    pub fn exp_divisor_site(self) -> Option<u32> {
        match self {
            Self::Ktf => Some(0x14_654e),
            Self::Lgt => Some(0x04_d3cc),
            Self::Skt => None,
            // ⛔ hero2: a different game. Every address in this arm's siblings was
            // reverse engineered from hero3, so hero2 answers CLOSED — see `Self::Hero2Ktf`.
            Self::Hero2Ktf => None,
        }
    }

    /// The game's own `checkLevelUp`, THUMB-bit included, when located on this build —
    /// the whole of what the 천상의 영약 cash item calls. `None` = the item consumes
    /// with no effect on this build, so the host must not arm a grant.
    ///
    /// LGT `binary.mod` (01.00.07): **`0x4d715`** (`0x4d714|1`), named by the
    /// `exp_divisor_site` RE above — the kill path's caller invokes `addExpFromKill`
    /// and then this. Disassembled in full (2026-08-13, from the player-supplied
    /// game zip whose `binary.mod` is md5-identical to the shipped one):
    ///
    /// * `checkLevelUp()` takes NO arguments — it fetches `CHAR+0x28` through the
    ///   game's own accessor (`0x1068`) and loops (≤15 times): while
    ///   `exp (CHAR+0xe4) > expToNext (CHAR+0xe8)`, call `doLevelUp` (`0x4d690`).
    /// * `doLevelUp`: level `CHAR+0xe2` +1 — at the 0x46 (70) cap it clamps, pins
    ///   `exp = expToNext` and returns; otherwise it recomputes the derived block
    ///   (`0x48f48`, `0x49000` — expToNext among them), refills HP to `CHAR+0xec`
    ///   and SP to 1000 (the full-restore pair `full_restore_writes` mirrors),
    ///   grants +4 stat points (`0x4d610(4)`; cap 276 = 69 levels × 4 ✓) and +1
    ///   skill point (`0x4d648(1)`) for levels ≤ 20 and even levels after (total 44
    ///   = `SKILL_POINT_CAP` ✓), and fires the level-up effect through the callback
    ///   at `CHAR+0x9bb0`.
    ///
    /// So the host-side item is two steps, both of them the game's own mechanism:
    /// write `exp = expToNext + 1`, call this. Everything downstream — points, caps,
    /// refills, visuals, the multi-level loop — is game code. The call is deferred to
    /// field state (`items::in_field_state`) so it runs in exactly the context the
    /// kill path runs it in.
    ///
    /// KTF: not located (the shipped APK carries the LGT asset; port on demand).
    pub fn level_up_fn(self) -> Option<u32> {
        match self {
            Self::Ktf => None,
            Self::Lgt => Some(0x04_d715),
            Self::Skt => None,
            // ⛔ hero2: a different game. Every address in this arm's siblings was
            // reverse engineered from hero3, so hero2 answers CLOSED — see `Self::Hero2Ktf`.
            Self::Hero2Ktf => None,
        }
    }

    /// How to relax the equip-level check ("장비를 장착할수 없습니다") on this build,
    /// when the site is located AND verified: `None` means the buff stays host-side
    /// only and the game keeps enforcing the shipped requirement.
    ///
    /// The check is one site in the whole image on BOTH builds — the only caller of
    /// `showMessage(0x67)` — but the two compiles need different rewrites, which is why
    /// the variant carries the SHAPE and not just an address (the full disassembly of
    /// both sites lives with the patch logic, `items::equip_relief_patch`):
    ///
    /// * KTF keeps the required level in a register move (`adds r1,r3,#0` at
    ///   `0x13d0aa`), so one halfword (`subs r1,r3,#N`) does it.
    /// * LGT compares the freshly-loaded byte directly, so there is no move to rewrite.
    ///   The patch goes on the OTHER operand instead: the player-level load
    ///   `movs r3,#0; ldrsh r1,[r4,r3]` at **`0x477ce`** becomes
    ///   `ldrh r1,[r4,#0]; adds r1,#N` (check `0x477cc`..`0x477dc`, `showMessage` =
    ///   `0x46b7c`). [`EquipReliefSite::LevelBias`] carries the full argument for why
    ///   that is provably equivalent and why the requirement-side rewrite §2.8
    ///   proposed is not.
    pub fn equip_relief_site(self) -> Option<EquipReliefSite> {
        match self {
            Self::Ktf => Some(EquipReliefSite::MoveHalfword { site: 0x13_d0aa }),
            Self::Lgt => Some(EquipReliefSite::LevelBias { site: 0x4_77ce }),
            Self::Skt => None,
            // ⛔ hero2: a different game. Every address in this arm's siblings was
            // reverse engineered from hero3, so hero2 answers CLOSED — see `Self::Hero2Ktf`.
            Self::Hero2Ktf => None,
        }
    }

    /// The three one-byte immediates that place the game's own BUFF BAND horizontally,
    /// as `(address, pristine opcode high byte, this site's delta from the band origin)`.
    /// Empty = not located on this build, and then the band keeps the game's own `x = 1`.
    ///
    /// The band's x has no origin global (unlike its y): each of the three draws computes
    /// `cellWidth * i + K` with its own `adds rN, #imm8`, so all three have to move
    /// together or the band tears apart (measured on KTF: patching only the frame left
    /// the icons at the far left). `items::band_x_patches` re-checks the opcode byte
    /// before writing, so a build this was not measured on is left alone.
    ///
    /// ── KTF `client.bin`, renderer `0x149b30` [workflow wf_69d1cba7 RC1, HIGH] ────
    ///   `0x149c2e adds r1,#1` frame · `0x149c8c adds r4,#3` icon · `0x149d2c adds r1,#1`
    ///   tick number (its own x is `w*i + w/2 + K`).
    ///
    /// ── LGT `binary.mod` 01.00.07, renderer `0x22a4` (this table's addition, 10차) ──
    /// Disassembled `0x22a4..0x2484` — the band loop, bounded by its own guard
    /// (`0x2320 ldrsh r3,[CHAR+0x16c]; bgt` → else `b 0x2484`, the target box) and its
    /// counter (`[sp,#0xa0]`, `i` as a u8, compared against that same halfword). The
    /// three sites fall out of the arithmetic, not out of a pattern match:
    /// ```text
    ///   0x233e  w  = ldrsh [LAY,#0x30]      ; ui_bm frame-3 width (the cell pitch, 20)
    ///   0x2350  r1 = w * i                  ; i = (s8)[sp,#0xa0]
    ///   0x2372  adds r1,#1                  ; <= FRAME   x = w*i + 1
    ///   0x23ba  r5 = w * i
    ///   0x23ec  adds r5,#3                  ; <= ICON    x = w*i + 3   (+2 of the frame)
    ///   0x2444  r1 = w * i
    ///   0x2446  r3 = w >> 1 ; adds r1,r1,r3
    ///   0x2450  adds r1,#1                  ; <= NUMBER  x = w*i + w/2 + 1
    /// ```
    /// ⚠ `0x237c movs r3,#0x14` in the middle of that range is the 20-byte BUFF RECORD
    /// stride (`muls r3,r5,r3` with `r5 = i`), not a coordinate — the same trap KTF's
    /// `0x149c48` documents. The y side is a single global at `.bss 0x1503a50`
    /// (`+2` frame, `+5` icon, `+h-7` number; the target box below shares it), so it is
    /// the x that has to be patched per site.
    pub fn band_x_sites(self) -> &'static [(u32, u16, u8)] {
        match self {
            Self::Ktf => &[
                (0x14_9c2e, 0x3100, 0), // frame:  K = X
                (0x14_9c8c, 0x3400, 2), // icon:   K = X + 2
                (0x14_9d2c, 0x3100, 0), // number: K = X
            ],
            Self::Lgt => &[
                (0x00_2372, 0x3100, 0), // frame:  K = X
                (0x00_23ec, 0x3500, 2), // icon:   K = X + 2   (r5 here, r4 on KTF)
                (0x00_2450, 0x3100, 0), // number: K = X
            ],
            Self::Skt => &[],
            // ⛔ hero2: these are hero3 code-patch immediates. Closed.
            Self::Hero2Ktf => &[],
        }
    }

    /// Every immediate that places the game's own TARGET-MONSTER box horizontally, as
    /// `(address, pristine halfword, this site's signed constant)`. Empty = not located,
    /// and then the box stays right-edge anchored — i.e. underneath our minimap panel.
    ///
    /// Every x in the block is `W - FW + K` (W = screen width, FW = the 85-px frame-4
    /// width) or `W + K`, and each ends in its own add/sub-imm8 (on r1 in hero3; hero2's
    /// one site is on r7 — the encoder keeps whichever register the pristine halfword
    /// names), so shifting the box left by N just makes every K become `K - N`. Both
    /// encodings are 16-bit Thumb, so the instruction never changes length, and on every
    /// build every one of these is followed by another flag-setting ALU op or a `bl`
    /// before any conditional — which is what makes rewriting ADDS as SUBS safe. The
    /// largest shift is `255 + min(K)` over the build's sites (`target_box_shift_max`):
    /// 211 on hero3 (the `subs r1,#0x2c` site), 189 on hero2 (`subs r7,#0x42`).
    ///
    /// ── KTF `client.bin`: TWELVE sites, `0x149db2..0x149f78` + a byte-identical second
    /// copy at `0x14a6aa..0x14a88c` (the selector at `0x149d5a` reads `(s8)CHAR[0x1ea]`
    /// and reaches the second copy through a `bl` used as a long branch). Patching one
    /// copy moved the box for one target kind and not the other.
    ///
    /// ── LGT `binary.mod` 01.00.07: **TEN**, and the difference is structural, not a
    /// missed site (10차). The same two paths exist — selector `0x2484` on the same byte,
    /// path A `0x2498..0x2608`, path B `0x2660..0x27e2` — but the two HP-bar-fill draws
    /// are **shared**: path A ends in `b 0x27e4` and path B falls through into the same
    /// `0x27e4`, so the last two immediates are reached by both and exist once.
    /// ```text
    ///   path A                     path B                     both
    ///   0x252a subs r1,#2          0x2706 subs r1,#2          0x282e adds r1,#0x1f
    ///   0x2560 adds r1,#1          0x273c adds r1,#1          0x285a adds r1,#0x1f
    ///   0x259a subs r1,#0x2c       0x2776 subs r1,#0x2c
    ///   0x25f4 adds r1,#0xf        0x27d0 adds r1,#0xf
    /// ```
    /// W is `(s16)[sp,#0x94]` (`ldr r3,[ctx,#0x44]` at `0x22f2`) and FW is
    /// `ldrsh [[r7],#0x40]`, re-derived at every site exactly as on KTF — which is why
    /// there is no single shared source to patch instead of these ten.
    pub fn target_box_x_sites(self) -> &'static [(u32, u16, i32)] {
        match self {
            Self::Ktf => &[
                // PATH A - CHAR[0x1ea] > 0
                (0x14_9df2, 0x3902, -2),
                (0x14_9e1e, 0x3101, 1),
                (0x14_9e58, 0x392c, -44),
                (0x14_9ebc, 0x310f, 15),
                (0x14_9f42, 0x311f, 31),
                (0x14_9f68, 0x311f, 31),
                // PATH B - the outlined else arm, same six elements, same six immediates
                (0x14_a73e, 0x3902, -2),
                (0x14_a76a, 0x3101, 1),
                (0x14_a7a4, 0x392c, -44),
                (0x14_a808, 0x310f, 15),
                (0x14_a862, 0x311f, 31),
                (0x14_a888, 0x311f, 31),
            ],
            Self::Lgt => &[
                // PATH A - the same selector byte > 0
                (0x00_252a, 0x3902, -2),  // the 85x26 frame
                (0x00_2560, 0x3101, 1),   // HP-bar background rect
                (0x00_259a, 0x392c, -44), // name / level text (x = W - 44, no FW)
                (0x00_25f4, 0x310f, 15),  // second text run
                // PATH B - the else arm, inline here rather than outlined
                (0x00_2706, 0x3902, -2),
                (0x00_273c, 0x3101, 1),
                (0x00_2776, 0x392c, -44),
                (0x00_27d0, 0x310f, 15),
                // SHARED TAIL - both paths reach these two
                (0x00_282e, 0x311f, 31), // HP-bar fill, left end
                (0x00_285a, 0x311f, 31), // HP-bar fill, right end
            ],
            Self::Skt => &[],
            // ★★ hero2 (2026-09-25, user: "hero3처럼 미니맵 왼쪽에 몹이름이랑 피통이 나오도록"):
            // ONE site, because hero2's panel is a pre-rendered IMAGE plus one bar, and both
            // take their x from the same register:
            // ```text
            // 0x1640e0  r7 = ax.g                    ; the framebuffer width (static)
            // 0x1640f2  subs r7,#0x42                ; <= THE SITE   x = W − 66
            // 0x1640f4  mov r8,r7                    ;   (no flags)
            // 0x1640f6  adds r7,r0,#0                ;   flags overwritten before any branch
            // 0x164116  drawImage(n.a:Image, r8, 0, LEFT)          ; the 66x28 name/level box
            // 0x164140  r2 = r8 + 0x1d ; fillRect(r2, 20, w, 2)   ; the HP fill, x + 29
            // ```
            // `subs r7,#(66 + N)` moves both N px left and keeps the instruction's length;
            // the imm8 caps N at 189 (`target_box_shift_max`). Nothing else in the image
            // draws this panel (`find_field_uses.py n.a --desc Lbd;`: `n.d()V` is the only
            // reader outside the setter and the clear), so there is no second copy to miss —
            // the trap hero3's KTF build had. Verified headless at 476x220 with N = 50: the
            // box and its bar moved together, the name and level with them.
            Self::Hero2Ktf => &[(0x0016_40f2, 0x3f42, -66)],
        }
    }

    /// The largest left shift EVERY target-box site of this build can still encode:
    /// `255 + min(K)` (a site's new constant `K − N` must stay an imm8 `subs`). 211 on
    /// hero3, 189 on hero2; 0 where no site is located, so nothing is ever asked for.
    pub fn target_box_shift_max(self) -> u32 {
        self.target_box_x_sites().iter().map(|&(_, _, k)| (255 + k).clamp(0, 255) as u32).min().unwrap_or(0)
    }

    /// `adds Rd,#v` / `subs Rd,#−v` (16-bit Thumb, imm8) for a signed constant, in the
    /// register `like` names — any add/sub-imm8 halfword carries it in bits 10:8. `None`
    /// when `|v| > 255`.
    pub fn add_sub_imm8(like: u16, v: i32) -> Option<u16> {
        let rd = like & 0x0700;
        match v {
            0..=255 => Some(0x3000 | rd | v as u16),
            -255..=-1 => Some(0x3800 | rd | (-v) as u16),
            _ => None,
        }
    }

    /// Every target-box x site that is not already encoding a left shift of `shift`
    /// guest px, as `(address, halfword_le)`. Idempotent, so the periodic poke pass can
    /// re-assert it forever — and a later, smaller shift (the minimap moved) rewrites the
    /// sites back towards their pristine constants, `shift == 0` included.
    ///
    /// Only what it recognises is rewritten: the pristine instruction, or an add/sub-imm8
    /// on the SAME register (one of our own earlier encodings). Anything else — another
    /// build, a relocated routine — is left alone, site by site. A shift past
    /// [`Self::target_box_shift_max`] asks for nothing at all rather than half the box.
    pub fn target_box_x_writes(self, read: &impl Fn(u32, usize) -> Vec<u8>, shift: u32) -> Vec<(u32, [u8; 2])> {
        let mut out = Vec::new();
        if shift > self.target_box_shift_max() {
            return out;
        }
        for &(addr, orig, base) in self.target_box_x_sites() {
            let b = read(addr, 2);
            if b.len() != 2 {
                continue;
            }
            let cur = u16::from_le_bytes([b[0], b[1]]);
            let Some(want) = Self::add_sub_imm8(orig, base - shift as i32) else { continue };
            if cur == want {
                continue; // already applied
            }
            if cur != orig && (cur & 0xf700) != (0x3000 | (orig & 0x0700)) {
                continue;
            }
            out.push((addr, want.to_le_bytes()));
        }
        out
    }

    /// May the host WRITE the five-slot quickbar arrays on this build?
    ///
    /// The four arrays are `CHAR+0x252` (kind), `+0x257` (item ref), `+0x25c` (skill
    /// ref) and `+0x26c` (strip cell), all read-only until now because a wrong offset
    /// here does not fail loudly — it sets a byte in the middle of the character record.
    ///
    /// ── WHY LGT IS TRUE (MIGRATION_LGT.md §2.16, 8차) ────────────────────────────
    /// The 퀵슬롯등록 A/B measured them from the other side: poking the popup's row byte
    /// to `1키등록` and letting the GAME do the bind made `CHAR+0x252` become 2 (kind =
    /// item) and `CHAR+0x257` become 6 (inventory slot 6 — the potion the cursor was
    /// on), with the untouched run leaving both at zero. The game's own writer landing
    /// on exactly these offsets is the verification; nothing here is inferred from KTF.
    ///
    /// SKT is `false` for the reason the flag exists: nothing has been measured on it.
    pub fn quickbar_writes_ok(self) -> bool {
        match self {
            Self::Ktf | Self::Lgt => true,
            Self::Skt => false,
            // ⛔ hero2's quickbar array is not confirmed yet (`HERO2_GUEST_RE.md` §6 #1):
            // the length-4 candidates read half-empty while the on-screen HUD shows four
            // filled cells. A write here would rebind a real player's skills.
            Self::Hero2Ktf => false,
        }
    }

    /// `skill_bind(cat, cell, slot)` — the game's own quickbar skill binder, Thumb bit
    /// set. `None` = not located on this build, and then a page restore writes the four
    /// arrays itself from the stored ref byte (`quickpage::Action::BindSkill` carries the
    /// argument for why that is equivalent). It is NOT a reason to disable page flips:
    /// clears and item binds never needed this function at all.
    ///
    /// KTF `0x145c48`: callable because it takes its arguments in registers and never
    /// touches the caller's frame (verified across `0x145c48..0x145cf8`).
    ///
    /// LGT: unlocated. The two static routes both came up empty — no literal anywhere in
    /// `.text`/`.data` points into the quickbar arrays (they are reached as
    /// `getter() + imm`, and `getter()` is `CHAR+0x28`), and the game's own binder runs
    /// inside the popup dispatcher, which §2.16 already showed the host can drive with a
    /// synthetic `OK` instead of a call.
    pub fn skill_bind_fn(self) -> Option<u32> {
        match self {
            Self::Ktf => Some(0x0014_5c48 | 1),
            _ => None,
        }
    }

    /// Offset of the auto-loot flag inside the character record, per build.
    /// `None` = not located, so the host must not force it (a guest WRITE at a shifted
    /// offset silently corrupts the record — the reason §2.13 deferred this).
    ///
    /// ── WHY THIS IS ITS OWN GATE NOW (2026-08-13) ────────────────────────────────
    /// [`items::autoloot_to_force`] used to ask [`guest_code_addresses_valid`], which is
    /// true only on KTF. The APK ships LGT, so the forcing loop in `emulator.rs` had
    /// **never once written the byte on a real device** — and the server had meanwhile
    /// delisted the `오토루팅` cash item on the grounds that the host did it, so the
    /// player ended up with neither. The item is listed again
    /// (`hero3_protocol.AUTOLOOT_IDX`) and this table is what will turn the host path
    /// back on for LGT without loosening the flag for everything else.
    ///
    /// ── KTF ──────────────────────────────────────────────────────────────────────
    /// `CHAR+0x2d7`, and the whole chain is verified: `useItem`'s dispatcher bounds the
    /// item-DB index with `cmp #0x10` at `0x153f02` and jumps through the 17-entry table
    /// at `0x1a923c` (entries are offsets relative to the table itself, reached as
    /// `sl + 0xffff65fc`). Reading that table out of `client.bin` reproduces every arm
    /// this repo has ever cited — `[0x07] 오브원석 -> 0x15403e`, `[0x0a] 비밀의상자` and
    /// `[0x0b] 부활의서 -> 0x154542` (the consume-only tail, i.e. no client effect),
    /// `[0x0c] 컵라면 -> 0x1540cc`, `[0x0e] 여행가방 -> 0x154106` — and `[0x0d] 오토루팅
    /// -> 0x1540f6`, whose body is the five instructions this offset comes from:
    ///
    /// ```text
    /// 0x1540f6  bl   0x14ad10          ; -> r0 = CHAR
    /// 0x1540fc  ldr  r1,[pc,#0x12c]    ; literal 0x000002d7
    /// 0x1540fe  adds r2,r3,r1
    /// 0x154100  movs r3,#1
    /// 0x154102  strb r3,[r2]           ; CHAR+0x2d7 = 1
    /// 0x154104  b    0x154542          ; shared tail (decrement quantity)
    /// ```
    ///
    /// ── The FIRST LGT attempt, which was wrong — kept because the trap is a good one ──
    /// This nearly went in as `Some(0x2d7)` on evidence that looked conclusive and was
    /// not.
    ///
    /// The 오토루팅 item was bought on an S25 and used while
    /// [`items::flag_window_changes`] watched `CHAR+0x280..0x300`. Exactly one non-counter
    /// byte moved, to exactly the value KTF's arm writes, and stayed there:
    ///
    /// ```text
    /// 03:23:32.401  CHAR+0x2d7 (0x1503d5f)  0x81 -> 0x01   <- and never moved again
    /// ```
    ///
    /// KTF's offset, KTF's value, and auto-loot did start working. Three independent
    /// agreements. **All three were coincidence.** Writing `1` there from the host on a
    /// fresh install (vc51, `forcing ON at 0x1503d5f (write ok = true)`, byte held at 1
    /// for minutes) produced NO auto-loot. The item working in the earlier trial was the
    /// ITEM working, not the byte.
    ///
    /// The static evidence agreed all along and was not weighed: resolving all 647 CHAR
    /// getter call sites found NOTHING that reads or writes game offset `0x2af`
    /// (= this crate's `0x2d7`). A byte no code touches cannot be a gate. Its `0x00 ->
    /// 0x81` five seconds into the process — i.e. at save deserialisation — says what it
    /// really is: collateral of a BLOCK write, and `0x81 -> 0x01` was another one.
    ///
    /// Also refuted: `CHAR+0x2dd`, the static candidate below — it never changed at all.
    ///
    /// The lesson, and the reason the window is now the whole record: a byte changing at
    /// the right moment is NOT evidence on its own, because block writes move whole
    /// neighbourhoods — and a window chosen around the answer you expect can only confirm
    /// or mislead, never find. MIGRATION_LGT.md §2.22-(5).
    ///
    /// ── The static search that did NOT get there, kept so it is not repeated ──────
    /// * The game's own base is `CHAR+0x28`: the accessor thunk `0x1068`
    ///   (`ldr r0,[pc,#0]; bx lr`) returns **`0x1503ab0`**, called indirectly as
    ///   `ldr rN,[pc]=0x1069; bl <bx rN veneer>` from **647** sites. So a game offset of
    ///   `k` is this crate's `k + 0x28` — which independently re-confirms twelve of the
    ///   offsets in §2.11 (level `0xba`, exp `0xbc`, maxHP `0xc4`, HP `0xca`, SP `0xd0`,
    ///   stat/skill points `0x162`/`0x164`, the popup cluster `0x9c44..0x9c5f`, the
    ///   inventory grid handle `0x9e00`, window id `0xa098`), including the popup TAG
    ///   byte, whose six writers store exactly 100..105 — the `0x64` §2.13 measured.
    /// * Resolving all 647 call sites by local abstract interpretation yields exactly ONE
    ///   `strb #1` anywhere in `game+0x200..0x300`: **`0x51152`, into `game+0x2b5` =
    ///   `0x1503d65` = this crate's `CHAR+0x2dd`**, from an arm shaped exactly like KTF's
    ///   (getter, literal offset, `movs #1`, `strb`, `b` to a tail that decrements a
    ///   quantity at `+0x80` with a `0x14` stride).
    /// * But that arm is entry **`[0x05]`** of the dispatcher at `0x508fc` (bound
    ///   `cmp #0x10`, table `0x5e224`, tail `0x51248`), and that dispatcher is NOT
    ///   `useItem`: its `[0x00]..[0x03]` are four distinct arms where cat-0x12's
    ///   포션/하이포션/미라클포션/엘릭서 must share one, and its records are a `0x14`-stride
    ///   array with id at `+0x7c`, not the `0x3c4`/`0x3c` inventory grid KTF's `useItem`
    ///   indexes. It is most likely skill/action use.
    /// ── LGT: `CHAR+0x2d8`, VALUE `0x81` — found by watching the WHOLE record ─────
    /// The second trial replaced the guessed window with all 43 KB of the record, and the
    /// answer was one byte away from KTF's and carried a different value — which is
    /// exactly why the first attempt failed even after it "matched":
    ///
    /// ```text
    /// 03:47:16.427  +0x9c84=1 +0x9c85=3 +0x9c87=0x69     item popup opens (tag 105)
    /// 03:47:33.490  CHAR+0x2d8 (0x1503d60)  0x00 -> 0x81  <== and still 0x81 a minute on
    ///               +0x9c84 1->0  +0x9c85 3->0            popup closes = item consumed
    /// ```
    ///
    /// In the whole record that was the ONLY persistent 0 -> non-zero at the moment of
    /// use, and the same run reports `+0x2d5=0 +0x2d7=0 +0x2dd=0` while auto-loot is
    /// live, which retires all three earlier candidates on the spot.
    ///
    /// **The value is why this needs a pair and not just an offset.** KTF's arm stores
    /// literal `1`; LGT's stores `0x81`. Writing `1` (to the wrong byte, at that) is what
    /// the failed attempt did, and a reader that tests a particular bit would reject it
    /// even at the right address. So the host writes what the game writes.
    ///
    /// ── ★★ SOLVED (2026-08-15, 50차): IT IS AN ARRAY, INDEXED BY THE SAVE SLOT ──
    /// Everything below this line supersedes the doubt recorded in 49차 and the three
    /// "refuted" candidates above. **All of them were the same array, seen from
    /// different save slots.**
    ///
    /// LGT's `useItem` was finally located — the dispatcher at `0x550c8`
    /// (`ldrsh [item+0x3b6]; cmp #0x12` category gate, then `ldrsh [item+0x3b8];
    /// cmp #0x10; bls`, jump table `0x5e538`) with the `0x3c4`/`0x3c` inventory-grid
    /// strides KTF's has. 12차 examined a DIFFERENT dispatcher (`0x508fc`, table
    /// `0x5e224`) and rightly rejected it; this is the real one, cross-checked by three
    /// arms against behaviour this repo had already documented:
    /// * `[12]` = `HP = *(base+0xc4); SP = 1000` — the full-restore 컵라면, and those
    ///   game offsets are `CHAR+0xec`/`+0xf2`/`+0xf8` exactly as `items` has them;
    /// * `[14]` = "byte <= 4 ? +1" — 여행가방, KTF's `[0x0e]`;
    /// * `[13]` = 오토루팅, and `hero3_protocol.AUTOLOOT_IDX` is `0x0D`.
    ///
    /// The 오토루팅 arm, `0x5523c`:
    /// ```text
    ///   r4 = 0x1069 (getBase) ; base = getBase()          ; = CHAR + 0x28
    ///   r3 = *(u32*)(base + 0xa3e8)                       ; = *(CHAR + 0xa410)
    ///   r3 = r3 + base + 0x2ac
    ///   movs r2,#0x7f ; rsbs r2,r2,#0                     ; 0x81, built as -0x7f
    ///   strb r2,[r3,#3]                                   ; *(base + N + 0x2af) = 0x81
    /// ```
    /// `CHAR+0xa410` is not a new discovery — it is the SAVE SLOT NUMBER this table
    /// already names as [`SaveChain::slot_off`], which is why the save chain's own code
    /// (`0x1a5f0`, `0x1a6e4`, `0x1a982`, `0x1ad94`) reads the same word. So:
    ///
    /// ```text
    ///   auto-loot flag = CHAR + 0x2d7 + saveSlot        (one byte per slot)
    /// ```
    ///
    /// and every earlier observation falls out of it:
    /// * KTF's documented `CHAR+0x2d7` is the **slot 0** element;
    /// * 12차's device measurement of `CHAR+0x2d8` was **slot 1**;
    /// * `CHAR+0x2dd`, "refuted" above, is just **slot 6**;
    /// * the vc51 test that wrote the byte and got no auto-loot was on a slot the host
    ///   was not writing — the write was real, the address was another slot's.
    ///
    /// ★ And the VALUE is confirmed from the other side. The reader at `0x3ac02`
    /// recomputes the identical address and tests it SIGNED:
    /// ```text
    ///   ldrb r3,[r3,#3] ; lsls r3,#0x18 ; asrs r3,#0x18 ; cmp r3,#0 ; bge (skip)
    /// ```
    /// i.e. the gate is **bit 7**, so `1` can never pass it however right the address is.
    /// That retires the last mystery: 6차's "KTF value, KTF offset, and it worked" was a
    /// slot-0 character using the ITEM, and the host writing `1` afterwards did nothing
    /// because `1 >= 0`.
    ///
    /// ⚠ The reader CLEARS the byte on the branch it takes, so this is one-shot per
    /// pickup — which is exactly why the host re-arms it on a timer rather than once.
    ///
    /// ── the earlier, superseded record, kept because the METHOD errors are worth ──
    /// ── remembering even though the conclusions are now explained away ───────────
    /// ── ⚠ THE LGT PAIR WAS IN DOUBT (2026-08-15, 49차) ───────────────────────────
    /// Player: "오토루팅이 안돼", with the device log showing `+0x2d8=Some(129)` — the
    /// host writing exactly what this table asks for, and no auto-loot. Three static
    /// searches over the shipped `binary.mod` (md5 `6a352944…`) then came back empty:
    ///
    /// | question | answer |
    /// |---|---|
    /// | any pc-relative literal `0x2b0` (= this offset seen from the game's `CHAR+0x28` base) | **0 sites** |
    /// | any neighbouring sub-base literal (`0x290`..`0x2b0`) reaching it by displacement | **0** — all five use displacement 0 |
    /// | any `movs rX,#0x81` followed by a `strb` | **0 sites** (10 materialise `0x81`, all `strh` or shifted offsets) |
    ///
    /// The third is the damaging one: if the game never stores this VALUE as a byte, then
    /// the `0x00 -> 0x81` transition 12차 watched cannot have been a store aimed at this
    /// field — it was collateral of a wider write, which is the exact trap this comment
    /// already documents for `0x2d7` and `0x2dd`. Widening the window from a guess to the
    /// whole record fixed the WINDOW; it did not stop "a byte that changed at the right
    /// moment" from being weak evidence on its own.
    ///
    /// ★ Left in place rather than flipped to `None`, deliberately: the searches above are
    /// literal/displacement analysis and would MISS a runtime-computed index
    /// (`ldrb rX,[base, rIdx]` walking the record), so this is strong grounds for doubt and
    /// not a proof of absence. Turning the write off is a behaviour change and wants device
    /// evidence, not a second inference. What has changed is that nothing may now DESCRIBE
    /// this as a working feature — see the boot line in `emulator.rs`.
    ///
    /// To settle it: find LGT's `useItem` dispatcher (KTF's is `0x153f02`, `cmp #0x10` into
    /// the 17-entry table at `0x1a923c`, arm `[0x0d]` = 오토루팅 -> `0x1540f6`). The LGT
    /// candidate at `0x508fc` was examined in 12차 and rejected — its records are a
    /// `0x14`-stride array with the id at `+0x7c`, not the `0x3c4`/`0x3c` inventory grid —
    /// so the real one is still unlocated. Its 오토루팅 arm names the true flag in one step.
    pub fn autoloot_flag(self) -> Option<AutolootFlag> {
        match self {
            // KTF's arm stores a plain `1` at a plain `CHAR+0x2d7`, which is the slot-0
            // element of the same array — but no KTF build has been checked on a second
            // slot, so it stays unindexed rather than being assumed symmetrical.
            Self::Ktf => Some(AutolootFlag {
                base_off: 0x2d7,
                slot_off: None,
                value: 0x01,
            }),
            Self::Lgt => Some(AutolootFlag {
                base_off: 0x2d7,
                slot_off: Some(0xa410),
                value: 0x81,
            }),
            // ⛔ hero2: no such feature located (and no cash items exist in hero2 at all).
            Self::Hero2Ktf => None,
            Self::Skt => None,
        }
    }

    /// The host-driven save chain (MIGRATION_LGT.md §4-3). `None` = unported.
    ///
    /// ── KTF ──────────────────────────────────────────────────────────────────────
    /// The values `save.rs` carried since 19차 (RE_SAVE_LEVELCAP_EXP_POINTS.md):
    /// `serialiseSave 0x1960bc` / `slotSummary 0x196b90` / `writeDbRecord 0x1951b0`,
    /// scratch = 4 bytes inside the reachability-proven dead range
    /// `[0x14a32e, 0x14a6aa)`, slot at `CHAR+0xa3f8`, names read out of the `sl` table.
    ///
    /// ── LGT (this table's addition, 2026-08-11 9차) ──────────────────────────────
    /// The five sites loading BOTH name strings (§2.13) were disassembled; the save
    /// routine is the one at `0x2515e..0x25186`, and it hands the whole contract over:
    ///
    /// ```text
    /// 0x2515e  outLen = 0 (stack); r0 = &outLen
    /// 0x25166  bx [0x25200]=0x1ad89   →  serialiseSave(&outLen) -> r0 = buf
    /// 0x25172  bx [0x25204]=0x1a265   →  writeSaveRecord([0x25208]="GameSave_11"@0x5bbb4, buf, outLen)
    /// 0x25178  bx [0x2520c]=0x1a6d1   →  slotSummary() -> r0 = buf16
    /// 0x25182  bx [0x25204]           →  writeSaveRecord([0x25210]="SlotSave_11"@0x5bbc0, buf16, 0x10)
    /// ```
    ///
    /// `writeSaveRecord = 0x1a264` and the caller's argument shapes were already
    /// established independently in §2.13 (sprintf `"%s_%d"` with `*(CHAR+0xa410)`),
    /// so the two new functions cross-check: the routine that calls the §2.13 writer
    /// twice with the §2.13 strings is the game's own save, and the two producers of
    /// its 2nd argument are exactly the two functions §4-3 was missing.
    ///
    /// ★ Scratch = `.bss` TAIL, not dead code — a stronger property than KTF's:
    /// `.bss` is declared `0x1500000..0x1512958` but the loader maps sections
    /// page-rounded (`core.load` → `next_multiple_of(0x1000)`), so
    /// `0x1512958..0x1513000` is mapped RW yet outside everything the ELF declares.
    /// Proof, three independent legs (2026-08-11):
    ///   1. STATIC: no literal word anywhere in `.text`/`.data` points at or past
    ///      `0x1512958` (the maximum is `0x1512954`, the last declared word), and the
    ///      largest literal+imm5 reach (`+0x7c`, Thumb-1 word load) tops out at
    ///      `0x15129d0` — `0x1512f00` keeps an 0x530-byte margin past it.
    ///   2. MAPPED: a probe `--poke 100:0x1512f00:5aa5c33c` wrote and read back fine.
    ///   3. LIVE: that sentinel survived byte-identical through boot, 7,800 frames of
    ///      play and the game's OWN save (GameSave_11_0 1408 B dropped) to frame 9200.
    pub fn save_chain(self) -> Option<SaveChain> {
        match self {
            Self::Ktf => Some(SaveChain {
                fn_serialise: 0x0019_60bc | 1,
                fn_slot_summary: 0x0019_6b90 | 1,
                fn_write_record: 0x0019_51b0 | 1,
                scratch: 0x0014_a400,
                slot_off: 0xa3f8,
                names: SaveNames::SlPtrs {
                    game: 0x1b_2c40 + 0x138,
                    slot: 0x1b_2c40 + 0x13c,
                },
            }),
            Self::Lgt => Some(SaveChain {
                fn_serialise: 0x0001_ad88 | 1,
                fn_slot_summary: 0x0001_a6d0 | 1,
                fn_write_record: 0x0001_a264 | 1,
                scratch: 0x0151_2f00,
                slot_off: 0xa410,
                names: SaveNames::Direct {
                    game: 0x0005_bbb4,
                    slot: 0x0005_bbc0,
                },
            }),
            // ⛔ hero2: the save codec is class `ag`'s seven serialisers and none of them is
            // ported (`HERO2_NEXT_SESSION.md` §9.2 P1-a). Host-driven save stays off.
            Self::Hero2Ktf => None,
            Self::Skt => None,
        }
    }

    /// The game's own "connection lost" modal, per build. See [`NetDisconnectModal`]
    /// for the disassembly and for why the host has to raise it by hand.
    ///
    /// LGT `binary.mod` 01.00.07 — every address below was re-derived from the shipped
    /// asset (`assets/hero3_fixed.zip` → `0002865E.jar` → `binary.mod`, md5
    /// `6a352944…`) before it shipped, because the handover that produced them was
    /// stopped before its verification pass ran:
    /// * `getBase 0x1068` literal = `0x1503ab0`, so every `base+off` below is that
    ///   plus the offset the instruction actually carries;
    /// * `showModal 0x4684`'s literal pool: `0xa39c` ctx, `0xa43c` text, `0xa7fc` type,
    ///   `0xa438` state, `0xa7fd` flag — the exact five this struct names;
    /// * both dispatch tables replayed out of the initializer at `0x59f0`:
    ///   `modalKey[5] = 0xbe31`, `modalDraw[5] = 0xb609`, and `0x6164`/`0x60ac` index
    ///   them with `*(base+0xa438)`.
    ///
    /// KTF: not located. The APK ships LGT; the equivalent box exists in that compile
    /// too (`showMessage`-class), and this stays `None` until someone finds it, which is
    /// the per-item discipline [`hud_bottom_patch`](Self::hud_bottom_patch) documents.
    pub fn net_disconnect_modal(self) -> Option<NetDisconnectModal> {
        match self {
            Self::Ktf => None,
            Self::Lgt => Some(NetDisconnectModal {
                state_word: 0x0150_dee8,      // base + 0xa438
                disconnect_state: 5,          // teardown + back to the online main screen
                text_buf: 0x0150_deec,        // base + 0xa43c
                text_cap: 0x100,              // the memset width in showModal
                panel_flag: 0x0150_e2ac,      // base + 0xa7fc
                sub_flag: 0x0150_e2ad,        // base + 0xa7fd
                clear_word: 0x0150_de8c,      // base + 0xa3dc (ctx + 0x40)
            }),
            // ⛔ hero2: unlocated.
            Self::Hero2Ktf => None,
            Self::Skt => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ktf => "KTF",
            Self::Lgt => "LGT",
            Self::Skt => "SKT",
            // ⛔ hero2: a different game. Every address in this arm's siblings was
            // reverse engineered from hero3, so hero2 answers CLOSED — see `Self::Hero2Ktf`.
            Self::Hero2Ktf => "hero2-KTF",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RefineProtectionPatch;
impl RefineProtectionPatch {
    pub const BLOCK_LEN: u32 = 0x200;
    pub const CODE: u32 = 0x20;
    pub const NOTE: u32 = 0x100;
    pub const STATE: u32 = 0;
    pub const WINDOW: u32 = 4;
    pub const ITEM_TYPE: i8 = 21;
    pub const ITEM_ID: i8 = 6;
    pub const NOTE_ROW: u8 = 15;
    pub const WINDOW_SLOT: u32 = 0x0019_4a6c;
}
