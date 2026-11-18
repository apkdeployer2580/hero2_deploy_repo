//! wie_android — native aarch64 Android host for the wie WIPI/J2ME emulator.
//!
//! Ports `wie_cli` (the desktop winit+softbuffer host) to Android GameActivity.
//! Architecture (see NATIVE_REHOST_HANDOFF.md):
//!   * The emulator core (`wie_backend`/`wie_ktf`/… + vendored `arm32_cpu`) is a
//!     pure software interpreter and runs unchanged.
//!   * A dedicated worker thread with a large stack owns the `Emulator` + our
//!     `Platform` impl and drives the tick/redraw/input loop (the core-compat
//!     audit requires this — the GameActivity/JNI thread has a small stack and
//!     must not host the 512 KB `EmulatedMemory` construction + interpreter).
//!   * The winit UI thread owns the softbuffer surface, blits frames the worker
//!     pushes via `EventLoopProxy`, and forwards touch input to the worker.

mod app;
mod respawn;
mod atlas;
mod audio_sink;
/// Master-bus levels + soft limiter for the audio path. Dependency-free so
/// `audio_mix_selftest.rs` can `include!` it — do not add imports to that file.
mod audio_mix;
/// ★ 2026-10-01: hero2's songs through their own FM voices — the 2-operator synth the audio
/// thread renders the music with (`HERO2_BGM_FM_PLAN.md` §3). Dependency-free so
/// `ma2_fm_selftest.rs` and `tools/ma2_render` can `include!` it.
mod ma2_fm;
/// ★ 2026-10-02: hero2's effects (8 kHz ADPCM) to the device rate — band-limited, faded at
/// the end, with the octave above regenerated. Dependency-free so `sfx_upsample_selftest.rs`,
/// `tools/ma2_render` and `wie_probe` can `include!` it.
mod sfx_upsample;
/// ★ 2026-10-09: the awakenings' voices (the user's Ogg files, decoded once) — `lewton` only, so `wie_probe` can
/// `include!` it. When they play is `hero2java::H2VoiceWatch`.
mod voice;
/// Host-owned timed cash-shop buffs (EXP x2/x4, equip level relief). Dependency-free
/// so `buffs_selftest.rs` can `include!` it — do not add imports to that file.
mod buffs;
/// Host-driven recovery backup: the exact `04 3d` upload the game's own recovery
/// screen builds, taken after every save so 초기화 -> 복구 works without the player
/// ever opening the network menu. Dependency-free so `backup_selftest.rs` can
/// `include!` it.
mod backup;
mod config;
mod database;
mod emulator;
/// Private-server endpoint line parsing. Dependency-free on purpose so
/// `endpoint_selftest.rs` can `include!` it and test the real parser (the module
/// doc lives here because an `include!`d file cannot carry `//!` comments).
mod endpoint;
mod filesystem;
/// Gamepad/keyboard → guest phone keys: the mapping tables and the config format.
/// The event PLUMBING (JNI natives, `UserEvent` relay) lives in this file below —
/// see the module docs for why sticks cannot come through winit. 58차.
mod gamepad;
/// GENERATED 12x12 Hangul bitmaps for the closed set of Korean words the overlay
/// draws — see `tools_re/gen_hangul_glyphs.py`. Dependency-free, like the other
/// `include!`-able modules.
mod guest_build;
mod hangul;
/// Short-lived host-owned connection to the private server (backup upload).
mod hero3_link;
/// The typeface the host's own windows are lettered in (neodgm, all 11,172 syllables).
mod hostfont;
mod items;
mod network;
mod orientation;
mod overlay;
/// The PHYSICAL gamepad: keycodes, where each sits under the thumb, and the badge
/// geometry that puts a position glyph on a control. Position and not printed letters
/// because Android reports face buttons positionally and the three pad families print
/// different letters there. Dependency-free so `padhud_selftest.rs` can `include!` it.
mod padhud;
/// 패드 버튼 설정 (2026-10-09, with hero3): the swap rule, the stored-diff file and the
/// capture prompt's clock — dependency-free and shared byte for byte with hero3, so
/// `padmap_selftest.rs` can `include!` it.
mod padmap;
mod placement;
mod platform;
/// Host-owned quickbar pages. The game has five slots and no concept of a page, so a
/// page can only exist here. Dependency-free so `quickpage_selftest.rs` can `include!`
/// it — see the module docs for why it stores identities rather than guest bytes.
mod quickpage;
/// Host-driven save (quick-save + autosave), built out of the game's own four save
/// primitives. Dependency-free so `save_selftest.rs` can `include!` it.
mod save;
/// Save-vault POLICY: which of the guest's files are a character, how a kept
/// generation is named, and how many survive. Dependency-free so
/// `savevault_selftest.rs` can `include!` it and test the shipping rules (the
/// module doc lives here because an `include!`d file cannot carry `//!` comments).
mod savevault;
/// ★ 세이브 옮기기 (2026-10-02): the one file that carries a phone's whole game state — saves,
/// warehouse, costume records, plates — to another phone (설정 → 진단 정보 → 세이브 관리).
/// The import policy and the dialogs' lines; `vault.rs` reads and writes the phone's side.
mod save_transfer;
/// ★ 2026-10-28 세이브 내보내기 · 불러오기 · 되돌리기 (환경설정 맨 밑 세 행): the transfer file (a strict
/// stored zip), and the journaled apply at the next start — ChronosSword's engine with its four
/// code reviews' fixes. Dependency-free so `savebundle_selftest.rs` can `include!` it.
mod savebundle;
/// ★ 의상 아바타 A2: what costumes a character owns and wears, and how that record is bound
/// to the character's save (`/g<slot>`) so a death and a reload hand back exactly what was
/// saved. Dependency-free so `avatar_state_selftest.rs` and the probe can `include!` it.
mod avatar_state;
/// ★ 2026-10-16 「보스 레이드」: the temple's raid gate — serves the gate-bearing 신전 (map 112's bytes) for `/m/m/15.m.gz`
/// while the character holds a GUARDIAN HARD receipt. See the module docs.
mod raid_gate;
mod server_prompt;
/// TLS for the private-server connection (see the module docs for why the guest
/// cannot do this itself).
mod tls;
/// The plaintext-buffer handoff between the TLS record layer and the guest's
/// `recv`. Dependency-free so `tls_buf_selftest.rs` can `include!` it (the rest of
/// `tls.rs` needs rustls, which cannot be built for this host).
mod tls_buf;
/// Overlay button icons (coverage masks, tinted at draw time). See the module docs for
/// why colour is not stored.
mod ui_icons;
mod vault;
mod vibration;

use wie_backend::KeyCode;

/// What a field quickbar slot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotKind {
    Empty,
    Skill,
    Item,
}

/// Which decoded-atlas icon a slot draws. `None` = no icon (empty / unresolved).
///
/// ★ hero2 (2026-09-17, `tools/gen_h2_atlas.py`): `Skill(class 0..2, cell)` is a cell
/// of `img/gmico`'s per-class strip — cell = quickbar value − 1 — and `Beast(id)` is
/// `img/bico[id]`, the GLOBAL beast-skill id the beast's `bn.j[k]` maps slot `k` to.
/// `Item(frame)` is hero3's item_bm frame and has no hero2 producer. `ItemType(t)` is
/// hero2's `img/glb` frame-17 strip cell for `itmtp` type `t` — the icon the game's
/// own inventory draws for every record of that type (S7 potion plates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconRef {
    Item(u16),
    Skill(u8, u8),
    Beast(u8),
    ItemType(u8),
}

/// One field-quickbar slot (`0..QUICK_SLOTS`) for the on-screen action HUD. Its
/// activation key + label come from `quickslot_hotkey` — `None` for a HOST slot, which
/// has no guest key at all (see [`is_host_slot`]). `count` = item stack (0 for skills).
///
/// `cooldown` = the GAME's own cooldown for this slot, `(ticks left, ticks total)`,
/// when the guest keeps one (hero2: `ag.a:[I` per slot, total from the class's static
/// table). `None` = ready, or a build whose cooldown is host-timed (hero3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuickSlot {
    pub slot: u8,
    pub kind: SlotKind,
    pub count: u16,
    pub icon: Option<IconRef>,
    pub cooldown: Option<(u16, u16)>,
    /// Which GUEST slot this plate currently sits in, if any — hero2's plate window
    /// (`quickpage::Window`). The pad's `1/3/7/9` buttons press guest slots, so their
    /// badges go on the plate seated there. hero3: `Some(slot)` for the guest-width slots.
    pub guest: Option<u8>,
}

/// Live player gauges for the HUD (from CHAR RAM — see `items.rs` for the offsets
/// and their provenance). HP is real points against a computed max; SP is the
/// game's 0..1000 scale (its max IS the constant 1000).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// ★ 2026-10-22 (가디언 레벨): `u32` — a GL 체력 build passes 65,535 (the game keeps HP in an `int`).
    pub hp: u32,
    pub hp_max: u32,
    pub sp: u16,
    pub sp_max: u16,
    pub exp: u32,
    pub exp_next: u32,
    pub level: u16,
    /// Unspent stat points (`CHAR+0x18a`). Drives the red `+` badge beside the
    /// gauges — the game draws exactly this badge itself, in the bottom HUD block
    /// our overlay replaces. See `items::STAT_POINTS_OFF`.
    pub stat_points: u16,
    /// Unspent skill points (hero3 `CHAR+0x18c` u8, hero2 `a.d:S` @0x68 short). Drives
    /// the blue `+` badge.
    pub skill_points: u16,
    /// ★ 2026-09-29 (hero2): the head the character is drawn with — the `n` of `hero/0/<n>`
    /// (the helmet's part sprite, 100 bare-headed; `hero2java::h2_head_sprite`). The status
    /// HUD's portrait (`atlas::head`). `None` = not read (hero3, boot).
    pub head: Option<u8>,
    /// ★ 2026-10-22 가디언 레벨: the GL of a LV 99 character (1..=250), drawn above the EXP bar — `exp` / `exp_next`
    /// are then the GL bar's. Filled by the worker from the GL block (`guest_anchors::GuardianLevelPatch`); `None` below
    /// 99, before the block is armed, and on hero3.
    pub guardian: Option<u16>,
}

/// Plates on the host bar, and cells in one host-owned page: **SEVEN** — the same bar
/// as hero3 (2026-09-10 사용자 지시, "스킬 슬롯 UI는 hero3와 완전 동일하게"; 5 → 7).
///
/// This is deliberately WIDER than what the game keys. hero2 routes four keys to
/// quickbar slots; the other three plates are HOST-OWNED. Since 2026-09-17 the plate
/// WINDOW (`quickpage::Window`) seats any of the seven into the guest's four slots on
/// demand — a host plate is fired by re-seating it, not by a key of its own — so the
/// bar is one width everywhere that lays out, draws, hit-tests or pages it, and what a
/// plate cannot do without a seat is asked in exactly one place, [`is_host_slot`].
pub const QUICK_SLOTS: u8 = 7;

/// How many of [`QUICK_SLOTS`] the GAME itself has a key for: **FOUR** — `1 3 7 9`.
///
/// Also the width of the guest's own quickbar arrays as far as we may touch them
/// (`items::QB_SLOTS`), and of the game's own registration popup, which offers exactly
/// four rows (`t/gm.tdf[34..37]` "1번에 등록" … "9번에 등록").
///
/// ★ Measured 2026-09-17: the guest array IS four cells — `ag.j:[B` @`0x193bac`
/// (`new byte[4]` in the SKT decompile, length 4 in the KTF heap; `HERO2_GUEST_RE.md`
/// §2.3). `0`/`#` do not touch it (they are the recovery-item pair), so nothing the
/// game dispatches can reach a fifth index.
pub const GUEST_QUICK_SLOTS: u8 = 4;

/// Fixed quickbar slot -> (activation key, display label), or `None` for a HOST slot.
///
/// ★ 2026-09-01 — this table was HERO3's (`1 / 3 / 9 / 0 / #`) and it is wrong for
/// hero2 in the worst possible way: the keys all exist on both games and mean different
/// things, so a wrong entry misfires silently instead of failing. Two independent
/// derivations of the hero2 table, both from shipped bytes:
///
///   * the game's own control guide, decoded out of the KTF 1.0.7 archive —
///     `t/help.tdf[8]`: "스킬: 1, 3, 7, 9 ; 회복 아이템: 0 ; 회복 아이템 교체: #".
///   * the SKT build's decompiled `bd.java::keyPressed`, which branches on
///     49 / 51 / 55 / 57 (= `1 3 7 9`) plus 48 (`0`) and 35 (`#`).
///
/// So `0` and `#` are NOT slots — they are the game's own recovery-item pair. (They had an
/// on-screen control of their own beside the action disc, `0` on a tap and `#` on a long
/// press, until the player had it removed on 2026-09-25; see [`HEAL_KEY`].)
/// Sending `#` for "slot 4" the way the hero3 table did would have swapped the player's
/// potion every time they tapped the last plate.
pub fn quickslot_hotkey(slot: u8) -> Option<(KeyCode, char)> {
    Some(match slot {
        0 => (KeyCode::NUM1, '1'),
        1 => (KeyCode::NUM3, '3'),
        2 => (KeyCode::NUM7, '7'),
        3 => (KeyCode::NUM9, '9'),
        // 4 = host slot: the game has no key for it. See `is_host_slot`.
        _ => return None,
    })
}

/// Is `slot` a HOST-owned plate — on the bar, but with no key the game routes to it?
///
/// The one predicate that knows the bar is wider than the game's. A host slot has no
/// key of its own; on hero2 it is used by SEATING it (`quickpage::Window` moves its
/// value into a guest cell, evicting the LRU plate, then the guest key fires) and bound
/// through the same seat (`Pages::reserve_for_bind`), so the popup's four rows reach it.
/// On hero3 — and on hero2 before 2026-09-17 — it was inert, see [`quickslot_inert`].
pub fn is_host_slot(slot: u8) -> bool {
    slot >= GUEST_QUICK_SLOTS && slot < QUICK_SLOTS
}

/// A plate that cannot do anything if tapped: a host slot with nothing bound.
///
/// Drawn dimmed and swallows its tap. This was hero2's agreed interim behaviour while
/// the quickbar was unmeasured (2026-09-01: "비활성 (회색)"); the bar landed on
/// 2026-09-17 and hero2 answers `false` for every plate now. hero3 keeps the rule.
pub fn quickslot_inert(slot: &QuickSlot) -> bool {
    // ★ hero2 (2026-09-17): no plate is inert any more — the plate window seats any of
    // the seven into the guest's four slots (`quickpage.rs`), so an empty host plate is
    // simply empty, exactly like an empty guest plate.
    if guest_build::hero2_java().is_some() {
        return false;
    }
    is_host_slot(slot.slot) && matches!(slot.kind, SlotKind::Empty)
}

// The plate store and the HUD must agree on the two widths; `quickpage.rs` cannot name
// these (it stays dependency-free for its selftest), so the agreement is checked here.
const _: () = assert!(quickpage::PLATES == QUICK_SLOTS as usize);
const _: () = assert!(quickpage::GUEST_SLOTS == GUEST_QUICK_SLOTS as usize);

/// 회복 아이템 사용 (`0`) and 회복 아이템 교체 (`#`) — the game's own recovery-item pair.
///
/// NOT quickbar slots, and `#` is NOT a page: it chooses which recovery item `0` uses
/// (`t/help.tdf[8]`). They stay ordinary guest keys (2026-09-01 사용자 결정), so the page
/// flip had to go somewhere else — it kept its own button, `placement::CtrlId::QuickPage`.
///
/// ★ 2026-09-25: no on-screen button any more (player: "OK버튼 옆에 0번버튼 제거해줘").
/// `0` is sent by a 포션 plate on the quickbar (`quickpage::Entry::Item`), by the pad's
/// `L1` (since 2026-09-25 — `L2` is plate Q5 now, hero3's seat) and by the pull-out keypad.
pub const HEAL_KEY: KeyCode = KeyCode::NUM0;
/// See [`HEAL_KEY`]. Sent by the pull-out keypad, and by the pad's `L2` (plate Q5)
/// off the field — the scene skip (`gamepad::menu_digit`). On the field it has no default
/// pad button since 2026-09-25 (R2 is plate Q4); `wie_gamepad.txt` can give it one. (It rode
/// on the `0` button as a long press from 2026-09-17 until that button went on 2026-09-25.)
pub const HEAL_SWAP_KEY: KeyCode = KeyCode::HASH;

/// Toast for a tap on an inert host plate. Every syllable must exist in `hangul.rs`.
pub const HOST_SLOT_TOAST: &str = "사용불가";

/// The minimap the worker renders from the live map and the overlay draws in its panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Minimap {
    /// hero3's port: the picture the game's own map window paints (`items::
    /// render_minimap_rgb`, pixel-identical to it), `w*h` `0xAARRGGBB` pixels, row-major,
    /// fitted whole into the panel.
    Picture { w: u16, h: u16, px: Vec<u32> },
    /// hero2 (2026-09-29, the hero5-style panel): one tile class per entry
    /// (`guest_build::H2_MM_*` — wall · floor · exit), `w*h` row-major, drawn as a
    /// fixed-zoom window that follows the player (`minimap_view::mm_draw_tiles`).
    Tiles { w: u16, h: u16, cells: Vec<u8> },
}

impl Minimap {
    /// The map's size in tiles (the hero3 picture is two pixels per tile).
    pub fn tiles(&self) -> (u16, u16) {
        match self {
            Self::Picture { w, h, .. } => (w / 2, h / 2),
            Self::Tiles { w, h, .. } => (*w, *h),
        }
    }
}

/// ★ #13: what the UI needs to draw the inventory PAGE STRIP — the page the worker has
/// the window on, how many there are, where the game drew its panel (framebuffer px,
/// 120 × 117 from there), and whether the window is the TOP of the chain (a popup or the
/// 재료 가방 child over it keeps the page but takes the taps).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BagPageView {
    pub page: u8,
    pub pages: u8,
    /// ★ 2026-10-02: how many cells this page has (`hero2java::h2_page_cells` — 28, 15 on the last); the
    /// UI paints the panel over the rest.
    pub cells: u8,
    pub panel: (u32, u32),
    pub on_top: bool,
    /// ★ 2026-09-29: the game's box over the window, framebuffer `(x, y, w, h)` — the strip
    /// is drawn UNDER it (`hero2java::h2_modal_over`). `None` while the window is the top.
    pub cover: Option<(u32, u32, u32, u32)>,
    /// ★ 2026-10-09: the window's own grid — how far its cells have actually moved right ([`inv_grid_shift`] for a bag
    /// window `q`, [`picker_grid_shift`] for an item picker `k`) and its first cell row below the panel's top
    /// (`Hero2Java::INV_CELL_Y0` 31 · `PICKER_CELL_Y0` 23). The tabs stand beside those rows.
    pub shift: u32,
    pub cell_y0: u32,
}

/// Emulator worker thread -> winit UI thread (via `EventLoopProxy`).
#[derive(Debug)]
pub enum UserEvent {
    /// A freshly rendered content frame (`0xAARRGGBB`, `CONTENT_W * CONTENT_H`) — ★★ 2026-10-15 and, under the HD effect
    /// mode, the effects the guest paint RECORDED instead of drawing (`wie_backend::canvas::HdFrame`): the UI composites
    /// them at the window's resolution over the blitted frame (`compose_hd`).
    Paint(Vec<u32>, Option<Box<wie_backend::canvas::HdFrame>>),
    /// The live field HUD state (quickbar slots + player gauges), refreshed from
    /// guest RAM while the game runs. `field` = real-time field free-roam has
    /// control (vs one of the game's own windows being open), which selects the
    /// whole overlay control set; `slots` is empty when the quickbar is not usable.
    /// `stats` stays at the LAST GOOD reading rather than zeroing, so a gauge never
    /// flashes empty on a transient failed read.
    /// `map_open` = the game's own map window is up, so the overlay enlarges it.
    /// `shell` = the out-of-world title/slot screen, which is 240-px art on a wide
    /// framebuffer -> letterbox it.
    /// `cutscene` = the game's SCRIPT window (id 4) owns the screen — dialogue and
    /// story playback, where the only useful keys are OK and `#`.
    /// `buff_lines` = the live cash-buff status lines (ASCII — the overlay font has
    /// no Hangul), composed by the worker because it owns the buff clock; the UI
    /// draws them under the gauge block while a game window is open.
    /// `can_save` = the game's own save inhibitor byte is clear (see
    /// `items::can_save_here`). False marks a no-save area — in this game that is
    /// the boss rooms — where the UI hides the SAVE button and the minimap
    /// (player request 2026-08-07).
    Hud {
        field: bool,
        cutscene: bool,
        /// The script window is showing a CHOICE LIST (`items::script_choice_open`),
        /// i.e. the one state inside a cutscene where the player has to move a cursor.
        /// Its own field rather than a third `Mode`: only the arrow block reads it.
        choice: bool,
        map_open: bool,
        shell: bool,
        /// The game is painting a BOTTOM-ANCHORED overlay (hero2's NPC dialogue box,
        /// app state 4) into rows the `hud_reserved_rows` crop would drop — so the
        /// UI shows the bottom of the framebuffer instead of the top for as long as
        /// it lasts. See `items::bottom_overlay_open` / `App::game_source_window`.
        bottom_overlay: bool,
        can_save: bool,
        slots: Vec<QuickSlot>,
        stats: Stats,
        buff_lines: Vec<String>,
    },
    /// Quickbar slot `slot`'s skill ACTUALLY started casting — the worker watched
    /// guest state after the tap (SP spent / the action state machine armed) and
    /// confirmed it. This, not the tap, is what starts the UI-side cooldown
    /// (player request 2026-08-07: "스킬이 실행 됐을때만 쿨타임이 돌아야해").
    SkillCast { slot: u8 },
    /// The minimap (or `None` until a map has been read).
    MinimapFrame(Option<Minimap>),
    /// The player's live position on that map, in TILES, fractional, at the centre of
    /// the character's tile (`hero2java::h2_player_marker`) — the dot, and the point the
    /// hero5-style view follows.
    ///
    /// ★★ Its own event, at its own cadence (2026-09-19, player: "미니맵에 현재 내위치가
    /// … 실시간으로 내위치에 따라 … 반영되어야해"). The picture and the dot change at
    /// completely different rates — the picture only when the map does, the dot
    /// continuously — so sending them together forces one of the two to be wrong:
    /// either the dot lags the picture's cadence, or a 25x25 grid is re-sent every time
    /// the player takes a step. Split, each moves at the rate it actually has.
    ///
    /// `None` = no dot (no map loaded, or the actor chain did not resolve).
    MinimapMarker(Option<(f32, f32)>),
    /// ★ 2026-09-29: the stage's live monsters, in the marker's units — the minimap's red
    /// squares (`hero2java::h2_monster_marks`; player: the minimap *"레퍼런스처럼 + 몬스터
    /// 점"*). Its own event at its own cadence, sent only when the list changed.
    MinimapMobs(Vec<(f32, f32)>),
    /// The live stage's map number (`h2_live_map`), so the world map can open on the
    /// place the character is standing. Sent when it changes.
    StageMap(u32),
    /// ★ 의상 아바타 A3b (2026-10-01): the status HUD's portrait — the head the game's loader
    /// loaded, facing the viewer (`hero2java::h2_live_head`: a costume's head, a helmet in its
    /// own colours). Sent when the parts table changes; `None` = not readable (the HUD then
    /// draws the atlas head for `Stats::head`).
    Portrait(Option<guest_build::H2Image>),
    /// ★ 2026-10-05: the character's running skill buffs — icon and whole seconds left
    /// (`hero2java::h2_buff_hud`), drawn as cells right of the HP/SP bars
    /// (`hud_view::hv_draw_buffs`; user: *"사용한 버프 스킬의 남은 지속시간을 좌측상단 HP SP바 우측에
    /// 표시 … 스킬아이콘과 숫자로"*). Sent when the list or a count changes — about once a second
    /// while a buff runs.
    SkillBuffs(Vec<guest_build::H2BuffShow>),
    /// ★ 2026-10-07 (2) 격분: the rage gauge (0..=100) while the berserker's 격분 runs — drawn as a bar under its buff
    /// cell (`hud_view::HvBuff::gauge`) — ★ 2026-10-13 D4 and 추종's gauge under its cell: `(buff id, sheet, gauge)` for
    /// every gauge running (empty = none). Sent when one changes (every hit).
    BuffGauges(Vec<(u8, u8, u8)>),
    /// ★ 2026-09-24: a pickup notice the game just raised, copied out of its `n.b`
    /// image (`hero2java::h2_notice_pending`) — `w × h` ARGB in the game's own font and
    /// box. The UI stacks them at the left edge, newest at the bottom
    /// (`overlay::draw_notices`); the game's painter, which would draw one at the centre,
    /// is off.
    PickupNotice { w: u16, h: u16, argb: Vec<u32> },
    /// ★★ 2026-10-17 보스 레이드: the countdown notice (`다음 보스로 이동 중...N`, `hero2java::h2_notice_picture` — the game's
    /// notice box and font, widened for the text). Same stack as [`Self::PickupNotice`], but each one REPLACES the countdown
    /// notice already on screen, in its slot, so the number counts down in one place.
    RaidCountdown { w: u16, h: u16, argb: Vec<u32> },
    /// ★ 2026-09-24: the host's rows of the game's 환경설정 window, in framebuffer pixels
    /// (`hero2java::h2_options_patch`) — empty when the window is not up or the game is
    /// drawing all of what is on screen.
    OptionsOverlay(Vec<guest_build::H2Patch>),
    /// ★ 2026-09-25 (부활의 구): the host's 예/아니오 box over the frozen frame of a fall, in
    /// framebuffer pixels (`hero2java::h2_confirm_patch` — the game's own confirm box
    /// redrawn) — empty when there is none. While it is up the worker holds the guest, and
    /// a tap inside it goes to the worker as [`InputEvent::MenuTap`] whatever the mode.
    RevivePrompt(Vec<guest_build::H2Patch>),
    /// ★ 의상 아바타 A3 (2026-09-30): the EQUIPMENT window's avatar side — the character in the
    /// middle, the avatar slot, the costume window — as one picture over the panel, in
    /// framebuffer pixels (`hero2java::h2_equip_avatar_patch`); empty when the window is not
    /// up. Its keys and taps are the worker's (`emulator::EquipAvatar`).
    EquipOverlay(Vec<guest_build::H2Patch>),
    /// ★★ 2026-10-16: the inventory's 정렬 button / the sell window's 일괄판매 button, in framebuffer pixels
    /// (`hero2java::{h2_sort_button_patch, h2_bulk_button_patch}`); empty when neither window is the top. Their taps are
    /// the worker's (`emulator::BagTools`, reached as [`InputEvent::MenuTap`]).
    BagTools(Vec<guest_build::H2Patch>),
    /// ★ 2026-09-24: the touch type was changed from the game's 환경설정 — the UI must
    /// release the stick and relayout, the way the gear row does.
    TouchTypeChanged,
    /// The game's rewritten context popup: `Some` while a "퀵바등록" row is on
    /// screen. The UI needs it to know that a confirm belongs to the popup rather
    /// than to the game, and to draw the plate highlight.
    Popup(Option<items::PopupView>),
    /// ★ #13 (2026-09-21): the inventory window over the MAIN bag is up — draw the page
    /// strip beside it. `None` = no such window. The worker owns the page (it writes the
    /// guest context the trampolines read), so the UI never derives it from its taps.
    BagPage(Option<BagPageView>),
    /// The worker answered a [`InputEvent::PopupConfirm`] with "the player chose
    /// 퀵바등록" — start highlighting the plates. `false` ends plate-picking (bound,
    /// cancelled, or the popup went away).
    PickingSlot(bool),
    /// Which host-owned quickbar page is live, so the PAGE button can label itself.
    ///
    /// Its own event rather than a field on [`UserEvent::Hud`]: the worker owns the page
    /// store, and the UI must never derive the number from its own tap count — a tap
    /// that the worker refuses (no CHAR yet, page flip dropped) would leave the label
    /// lying about what the plates are showing.
    QuickPage(u8),
    // ★★ 2026-10-20: `Toast(&'static str)` is gone — the host's status lines ("저장완료", the editor's answers, …) are
    //   pictures in the game's own notice box now, at the left with the pickups: the worker renders them
    //   (`emulator::host_notice`) and they arrive as [`UserEvent::PickupNotice`]. See `InputEvent::HostNotice`.
    /// ★★ THE EMULATOR WORKER IS GONE (2026-09-20, 잔여 작업 #10).
    ///
    /// `wie-emu` owns the guest; when it returns — a `FatalError` out of `tick()`, or a
    /// panic — the UI thread and `AudioTrack` keep running, so the screen freezes on the
    /// last frame WITH THE MUSIC STILL PLAYING. To the player that is an infinite hang
    /// with no explanation, and the repo's own triage step for it is `top -H`, which a
    /// player does not have. So the worker says so on its way out.
    ///
    /// `vault` = the save vault holds at least one restorable generation, which is the
    /// one thing worth adding to "restart the app": it tells the player their character
    /// survives this even if the crash landed mid-save.
    WorkerDied { vault: bool },
    /// A gamepad button edge, relayed from `MainActivity.dispatchKeyEvent` through the
    /// JNI natives below (gamepad/joystick sources only — see `gamepad` module docs for
    /// why this cannot ride the winit key path). `code` = Android keycode; `repeat` =
    /// the system's own auto-repeat, which is dropped because the emulator worker
    /// already replays every held key every 100 ms.
    GamepadButton { code: i32, down: bool, repeat: bool },
    /// A gamepad stick/hat/trigger sample (`dispatchGenericMotionEvent`): stick and
    /// hat are -1..1 per axis, triggers 0..1. The stick+hat pair is quantised in
    /// `App` through `overlay::joy_dir` — the same one-latched-direction rule as the
    /// on-screen stick, because that is all the guest can walk on — and the triggers
    /// become synthetic `BUTTON_L2/R2` edges (`gamepad::trigger_held`), because many
    /// Bluetooth pads never send those as KeyEvents at all.
    GamepadAxes { x: f32, y: f32, hx: f32, hy: f32, lt: f32, rt: f32 },
}

/// winit UI thread -> emulator worker thread (via `mpsc`).
pub enum InputEvent {
    Key { code: KeyCode, down: bool },
    /// ★★ 2026-10-20: a line the UI wants said in the game's notice box at the left (what the host's centre-bottom
    /// toast used to say). The worker renders it in the game's font (`emulator::host_notice`) and sends it back as a
    /// [`UserEvent::PickupNotice`]; the UI cannot draw it itself because the font is read from the guest.
    HostNotice(&'static str),
    /// ★ 2026-09-24: the walk stick (touch or gamepad) moved from one live direction to
    /// another. On the field the worker forwards ONLY the new press — hero2's release is
    /// "queue a stop" whatever key it names (`n.keyNotify` → `ag.b()V`), and one that the
    /// guest defers behind a press in the same frame (`ax.f`/`ax.n`) lands AFTER the new
    /// direction and turns the turn into a stop. Off the field it is the old
    /// release-then-press pair. See `emulator::apply_turn`.
    Turn { from: KeyCode, to: KeyCode },
    /// The player confirmed on the game's rewritten context popup ("퀵바등록"). The
    /// UI cannot decide what that means, because the highlighted row it last heard
    /// about is up to 80 ms old — so the WORKER re-reads the row and does one of:
    /// pass a real confirm through, ask the UI to start picking a plate, or commit.
    /// See `items::popup_view`.
    PopupConfirm,
    /// The player tapped quickbar plate `slot` while picking one. The worker writes
    /// the guest's highlighted-row byte and lets the GAME do the registration.
    PopupBindSlot { slot: u8 },
    /// The player pressed a PAD digit (`1/3/7/9` = guest slot `guest`) while picking: the
    /// plate seated in that slot (or, unseated, plate `guest`) is the target.
    PopupBindGuest { guest: u8 },
    /// Leave plate-picking without binding anything.
    PopupCancelPick,
    /// The player tapped the quickbar PAGE button. The worker owns this end to end: it
    /// re-reads the live bar, captures it into the outgoing page, and re-binds the
    /// incoming one — all of which needs guest memory, so the UI only reports the tap.
    ///
    /// The UI does not send a target page. The store's `current` lives on the worker
    /// side, and a UI-chosen target could be stale by a frame and flip somewhere the
    /// player did not ask for.
    QuickPageNext,
    /// The layout editor switched a plate off / every plate back on (HIDE / SHOW, hero3
    /// 82차). The UI has already flipped the live copy ([`set_quick_hidden`]) and
    /// relaid the ring; the worker owns the store (`quickpage::Pages`) and its file, so
    /// persisting goes through here — one writer for `wie_quickpages.txt`.
    QuickHidden { mask: u8 },
    /// The player asked for a QUICK SAVE (settings panel -> SAVE). The worker owns the
    /// whole thing: it re-reads the guest's own "may I save here" byte and then drives
    /// the game's four save primitives in order. See `save.rs`.
    QuickSave,
    /// The player tapped a SKILL plate on the field. The worker sends the slot's
    /// hotkey to the guest itself (down+up, one shot) and then WATCHES guest state
    /// for a short window to see whether the cast actually happened — SP spent, or
    /// the combo state machine armed (see `items::skill_cast_active`). Only a
    /// confirmed cast answers with [`UserEvent::SkillCast`], which is what starts
    /// the cooldown. Item plates keep going through plain [`InputEvent::Key`] —
    /// potions have no cooldown and need no watch. (hero3 path.)
    SkillCast { slot: u8 },
    /// ★ hero2 (2026-09-17): the player tapped PLATE `plate` on the field. The worker
    /// owns the whole thing — a plate is not a guest slot (seven plates over the game's
    /// four), so only it can say which slot the plate sits in, or seat it first with a
    /// raw bind (`quickpage::Pages::activate`), and then press that slot's own hotkey.
    QuickActivate { plate: u8 },
    /// ★ hero2 (2026-09-17, 터치 전수 전환): a tap on the game's own window at FRAMEBUFFER
    /// `(fx, fy)`. The worker walks the window chain, turns the point into "focus row N"
    /// (one cursor byte, no key) / "OK" (the focused row tapped again) / a tab key, and
    /// performs it — `guest_build::h2_tap`. The UI only converts window pixels to guest
    /// pixels; every geometry decision needs guest memory.
    MenuTap { fx: u32, fy: u32 },
    /// ★ #13 (2026-09-21): the player tapped inventory page tab `page` (0-based) on the
    /// strip the UI draws beside the game's inventory window. The worker checks the
    /// window is still on top, writes the page context, resets the window's cursor and
    /// repaints — see `hero2java::h2_page_select_writes`.
    WorldHold(bool),
    BagPage(u8),
}

/// ★ #13: the inventory PAGING context the worker keeps in the trampolines' block —
/// `(bag handle, page * 28)` packed as `bag << 32 | off` — published here so the item
/// popup readers in `items.rs` can turn a window's cursor CELL into the record it names
/// without threading the worker's state through every signature. Written only by the
/// worker (`BagPaging`), read on the same thread; the atomic is for the type system.
pub static BAG_PAGE_CTX: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn bag_page_ctx() -> (u32, u32) {
    let v = BAG_PAGE_CTX.load(std::sync::atomic::Ordering::Relaxed);
    ((v >> 32) as u32, v as u32)
}

/// ★ #14: how far the `q` grid has actually been moved right, in guest pixels — 0 until
/// the worker's patch pass has verified [`guest_build::inventory_grid_shift`] at every
/// site, and 0 for ever on a build that has none.
///
/// ⛔ A RUNTIME fact, not a constant. The patch can be refused (a site that does not read
/// as shipped), and then the game still paints its grid at the old place: a host that
/// hit-tested or drew the rail at the shifted place would put the tabs ON the first
/// column of cells and send taps to the wrong record. Everything that needs the grid's
/// left edge asks here.
pub static INV_GRID_SHIFT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn inv_grid_shift() -> u32 {
    INV_GRID_SHIFT.load(std::sync::atomic::Ordering::Relaxed)
}

/// ★ 2026-10-09: the same runtime fact for the item PICKER `k` ([`guest_build::picker_grid_shift`]) — where a paged
/// picker's tabs go (inside its left margin once its grid has moved, outside otherwise).
pub static PICKER_GRID_SHIFT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn picker_grid_shift() -> u32 {
    PICKER_GRID_SHIFT.load(std::sync::atomic::Ordering::Relaxed)
}

/// ★ #17 (2026-09-24): `true` once the worker's patch pass has landed
/// [`guest_build::skill_cooldown_half`] at all three jobs — the game then starts a
/// skill's counter from HALF its table entry. The plate ring divides the STATIC table
/// the same way (`items::h2_scan_quickbar`), or it would open every cooldown at 50 %.
/// ⛔ A runtime fact for the same reason as [`INV_GRID_SHIFT`]: a refused patch leaves
/// the shipped totals, and the ring must then use them undivided.
pub static SKILL_CD_HALVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// ★★ 2026-10-15 「악마화」 HD: the effects and the demon body drawn by the host at the WINDOW's resolution from the
/// library's 3× twins (`wie_backend::canvas::set_hd_mode`) — read once at start-up from `wie_hd_fx.txt` (`0` = off,
/// the guest-resolution pictures as before). The demon block is installed with the matching HD word and body sheet.
pub static HD_FX: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// ★★ 2026-10-16 「악마화」: 악의 is on (the block's STATE 2) — the host HUD's beast icons show the 악의 versions
/// (`DemonPatch::skin_icon`) while the game's beast table and icon array hold them (`emulator::Demon::poll_skin`).
pub static DEMON_SKIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The atlas beast id the host draws for beast skill `id` — its 악의 version while [`DEMON_SKIN`] is up.
pub fn beast_icon_id(id: u8) -> u8 {
    if DEMON_SKIN.load(std::sync::atomic::Ordering::Relaxed) {
        crate::guest_build::DemonPatch::skin_icon(id).unwrap_or(id)
    } else {
        id
    }
}

pub fn skill_cd_halved() -> bool {
    SKILL_CD_HALVED.load(std::sync::atomic::Ordering::Relaxed)
}

/// ★ 2026-10-02: the accessory the worker last wrote into the block's ACTIVE word
/// (`guest_anchors::AccessoryPatch` codes, 0 = none). The plate ring reads the STATIC cooldown
/// table, so 광룡의 외침's halved 사자후 has to be divided here too (`items::h2_scan_quickbar`).
pub static ACCESSORY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn accessory() -> u32 {
    ACCESSORY.load(std::sync::atomic::Ordering::Relaxed)
}

/// ★★ 2026-10-14: the gear block's per-index cooldown cuts as the worker last wrote them (`GearPatch::KCD_AT` — a byte per
/// `(class, index)`, 0 = none; 번개의 호흡 + 벽력일섬 = the rogue's 일섬). The game's `<job>.a(I)S` answers through the block's
/// wrappers; the plate ring reads the STATIC table, so it cuts the same way (`items::h2_class_cd_total`).
pub static GEAR_CD_CUT: std::sync::Mutex<[[u8; crate::guest_build::GearPatch::KINDS as usize]; 3]> =
    std::sync::Mutex::new([[0; crate::guest_build::GearPatch::KINDS as usize]; 3]);

pub fn gear_cd_cut(class: u8, index: u8) -> u8 {
    GEAR_CD_CUT.lock().unwrap_or_else(|e| e.into_inner()).get(usize::from(class)).and_then(|r| r.get(usize::from(index))).copied().unwrap_or(0)
}

/// ★★ 2026-10-17: the gear block (`guest_anchors::GearPatch`) once it is in, 0 before or refused — the host's elixir starts
/// 술꾼의 장화's walk through it (`emulator::gear_drink`) the way unit F does for a potion.
pub static GEAR_BLOCK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// ★ 2026-10-07: `true` once the worker has installed the berserker block (`guest_anchors::BerserkerPatch`) — its
/// `bl.a(I)S` wrapper answers 분노태세's 1 s; the static table the plate ring reads still says a minute
/// (`items::h2_class_cd_total`). A refused block leaves the minute in the game too.
pub static BERSERKER_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn berserker_on() -> bool {
    BERSERKER_ON.load(std::sync::atomic::Ordering::Relaxed)
}

/// ★ #13: the ACCOUNT warehouse bag's handle once the worker has made it (0 = not yet),
/// for the popup readers that must tell the warehouse's window from the inventory's.
pub static ACCOUNT_BAG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn account_bag() -> Option<u32> {
    match ACCOUNT_BAG.load(std::sync::atomic::Ordering::Relaxed) {
        0 => None,
        h => Some(h),
    }
}

/// …and the materials bag behind the warehouse's 재료가방.
pub static ACCOUNT_MATERIALS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn account_materials() -> Option<u32> {
    match ACCOUNT_MATERIALS.load(std::sync::atomic::Ordering::Relaxed) {
        0 => None,
        h => Some(h),
    }
}

/// Where the game's own top HUD band should sit, packed as
/// `(band_x_guest_px << 16) | y_origin_guest_px`. The UI thread owns the layout, the
/// emulator worker owns guest memory, so it crosses on an atomic (see
/// `items::hud_y_origin_to_set` / `items::band_x_patch`).
pub type HudYShift = std::sync::Arc<std::sync::atomic::AtomicU32>;

/// Original WIPI game content resolution (영웅서기2 KTF = 176x220 portrait,
/// `__adf__` DisplaySize).
pub const CONTENT_W: u32 = 176;
pub const CONTENT_H: u32 = 220;

/// Landscape immersive mode: logical content height. The width is derived from
/// the window aspect so the game frame fills the screen with **no letterbox and
/// no distortion** (`w = h * win_w / win_h`).
///
/// Why this works at all (measured 2026-07-28 with `wie_probe`, the headless
/// host runner): Hero3 is **resolution-adaptive**. Given any framebuffer size it
/// draws as much of the map as fits, centres the title art / minimap / HUD and
/// stretches the dialogue bar to the full width. The old
/// `LANDSCAPE_FEASIBILITY.md` verdict ("game never reads screen w/h → content
/// pinned top-left → needs guest patching") is refuted by that experiment.
///
/// The only limit is the **map's own pixel extent**: a map narrower than the
/// logical width shows black at the sides (the world simply ends). Hero3 map
/// widths are 176..1120 px, median 512 (134 maps) — so a logical width around
/// 500 fills most maps, and a smaller base height buys more fill at the cost of
/// chunkier pixels. Hence the base height is a tunable, not a constant.
pub const LANDSCAPE_BASE_H: u32 = 220;
/// Guard rails for the derived logical width.
pub const LOGICAL_W_MIN: u32 = 176;
pub const LOGICAL_W_MAX: u32 = 1024;

/// Hard cap on the immersive logical WIDTH.
///
/// ★ hero3 에서는 게스트 팝업 렌더러의 8-bit 절단 버그(guest 0x12e450) 때문에 496 이었다.
/// 영웅서기2 는 다른 컴파일이라 그 버그의 존재/주소가 미확인 — 해당 화면(컨텍스트 팝업류)을
/// 넓은 프레임버퍼에서 실측하기 전까지는 상한을 두지 않는다(= LOGICAL_W_MAX 와 동일).
/// 만약 hero2 에서도 비슷한 절단이 발견되면 그때 실측값으로 되살릴 것.
pub const POPUP_SAFE_LOGICAL_W: u32 = LOGICAL_W_MAX;

static LOGICAL: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
static IMMERSIVE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
/// Touch type B: the walk stick is PINNED to its home instead of spawning under
/// the finger. An atomic and not a `OnceLock` like the two above, because this one
/// is a live preference the player flips in the settings modal — and not a field on
/// `App`, because `overlay::layout_immersive` / `draw_joystick` need it and both are
/// free functions reached from the drawing path with no `App` in hand.
static TOUCH_TYPE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(TOUCH_FREE);
/// 게임패드 HUD preference (59차), as a [`padhud::PadHud`] discriminant. Same reason
/// as `STICK_FIXED` for being a global atomic: the drawing path needs it and has no
/// `App`. 0/1/2 = AUTO/ON/OFF — see [`pad_hud_pref`].
static PAD_HUD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Was the most recent input a GAMEPAD (rather than a screen touch)? What AUTO
/// follows. Set by `App` at the two input entry points and nowhere else.
static PAD_LAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Has a pad been used at all this session? Only gates the SETTINGS ROW: a
/// touch-only player never sees a row that could do nothing for them.
static PAD_SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Which quickbar PLATES the player has switched off, one bit per plate (hero3 82차,
/// ported 2026-09-17 저녁). A global atomic for the same reason as `STICK_FIXED`:
/// `overlay::quick_buttons`, `overlay::plate_seat` and `placement::shown` all need it
/// and every one of them is a free function on the drawing path with no `App` in hand.
/// The rules that read it live in [`quickpage`] — see [`quickpage::plate_hidden`].
/// Persisted as the `hidden` line of `wie_quickpages.txt`, which the WORKER writes:
/// the UI flips this copy and sends [`InputEvent::QuickHidden`], the worker publishes
/// it into the store (`Pages::set_hidden`) and saves.
static QUICK_HIDDEN: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(quickpage::SHOW_ALL);

/// 효과음 크기 — an index into [`audio_mix::SFX_LEVELS`] (hero3 2026-08-27, ported
/// 2026-09-18). An atomic because the reader is the AUDIO thread: `audio_sink` applies
/// the trim as each clip is converted, so a change is heard at the next sound rather
/// than at the next app start.
static SFX_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(audio_mix::SFX_LEVEL_DEFAULT);

/// 경험치 배율 — the EXP multiplier itself (1 = 기존, one of `guest_build::EXP_RATES`;
/// 2026-09-23, user request). An atomic because the reader is the WORKER: it keeps the
/// guest's multiplier word equal to this every few frames (`emulator::ExpRate`), so a
/// change on the settings row reaches the next kill rather than the next app start.
/// Seeded from `wie_exp_rate.txt` before the worker starts.
static EXP_RATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(guest_build::EXP_RATE_DEFAULT);

/// Apply `pct` percent to `v`. Never returns 0 (a zero-sized control would be
/// invisible AND untappable), and the percentage is bounded so a hand-edited or
/// corrupt layout file cannot produce absurd geometry. See [`placement`], which is
/// where every player-chosen size goes through this.
pub fn scaled(v: u32, pct: u32) -> u32 {
    ((u64::from(v) * u64::from(pct.clamp(placement::PCT_MIN, placement::PCT_MAX)) / 100) as u32).max(1)
}


/// Resolve the guest framebuffer size ONCE, from the window the game will run
/// in. `None` (or a portrait window) keeps the original 240x320.
///
/// Called before the emulator thread starts; the WIPI screen framebuffer is
/// allocated once at `MC_grpGetScreenFrameBuffer` and its pointer is cached by
/// the guest, so this cannot change while the game runs.
/// `landscape_pref` = the persisted orientation choice, NOT `win_w > win_h`.
/// Device-confirmed race (2026-07-28): `orientation::set(true)` is issued before
/// the event loop, so the FIRST `resumed` can still report the pre-rotation
/// PORTRAIT size (seen: launch A 2340x1080, launch B 1080x2340 on the same
/// device with the same config). Sizing off that transient value silently
/// dropped immersive mode every other launch — so derive the aspect from the
/// window's long/short sides plus the preference the activity will settle into.
pub fn init_logical(win_w: u32, win_h: u32, landscape_immersive: bool, landscape_pref: bool, base_h: u32) -> (u32, u32) {
    let (long, short) = (win_w.max(win_h), win_w.min(win_h));
    let size = if landscape_immersive && landscape_pref && short > 0 {
        let base = base_h.max(120);
        // even width keeps the 2-byte-per-pixel guest stride tidy
        let w = ((u64::from(base) * u64::from(long) / u64::from(short)) as u32)
            .clamp(LOGICAL_W_MIN, LOGICAL_W_MAX.min(POPUP_SAFE_LOGICAL_W))
            & !1;
        // Derive the height BACK from the (possibly clamped) width so the frame
        // still fills the window 1:1 — a clamp applied to the width alone would
        // stretch the picture. On aspects up to 2.07:1 nothing is clamped and this
        // returns `base` unchanged (±1 px of integer rounding).
        let h = ((u64::from(w) * u64::from(short) / u64::from(long)) as u32).max(120);
        (w, h)
    } else {
        (CONTENT_W, CONTENT_H)
    };
    let _ = LOGICAL.set(size);
    let _ = IMMERSIVE.set(size != (CONTENT_W, CONTENT_H));
    logical()
}

/// The size the game is PRESENTED at (240x320 until [`init_logical`] runs) - the one
/// every piece of layout geometry means by "logical".
pub fn logical() -> (u32, u32) {
    *LOGICAL.get().unwrap_or(&(CONTENT_W, CONTENT_H))
}

/// The framebuffer the GUEST is given, which on a build that reserves rows for its own
/// HUD is taller than [`logical`] by exactly that reservation.
///
/// ★ Two sizes, not one, since 2026-09-10. The game derives its world viewport from the
/// framebuffer height and keeps the rest for its own bottom HUD, so asking for the extra
/// rows and dropping them at present time puts the WORLD where that HUD used to be -
/// same scale, more world, no black strip and no guest write. The full reasoning and the
/// measurement are on [`guest_build::GuestBuild::hud_reserved_rows`].
///
/// ⚠ Only the blit and the emulator's screen construction may use this. Everything else
/// - control seats, the HUD, hit-testing - is about what the player can SEE, which is
/// [`logical`].
pub fn guest_fb() -> (u32, u32) {
    let (w, h) = logical();
    (w, h + guest_build::hud_reserved_rows())
}

/// True when the guest runs at an aspect-matched wide framebuffer, i.e. the
/// window is filled 1:1 by the game and the controls are translucent overlays.
pub fn immersive_active() -> bool {
    *IMMERSIVE.get().unwrap_or(&false)
}

/// TOUCH TYPE B (player request 2026-08-16, revised the same day): the field walks
/// with the four-arrow D-pad (cut from `placement::CtrlId::Stick`'s square) and the
/// stick is hidden outright — the first B was a stick pinned to its seat, and the
/// player asked for arrows only ("화살표만 나와야해 … 조이스틱은 터치타입 B에서
/// 히든처리"). False (the default, TOUCH A) is the shipped behaviour: the whole
/// window is the stick zone and the base spawns wherever the finger lands.
pub fn stick_fixed() -> bool {
    touch_type() == TOUCH_FIXED_B
}

/// ★ 2026-09-24 (user: "조작 방식은 free, fixed_A(조이스틱형 고정), fixed_B(화살표형 고정)"):
/// the three touch types. `free` = the floating stick (touch type A), `fixed_A` = the
/// same stick PINNED to its home seat (the base never moves; the direction is taken from
/// the seat's centre), `fixed_B` = the four arrows (touch type B). Chosen in the game's
/// own 환경설정 (`hero2java::H2Options`, row 7) or the gear settings.
pub const TOUCH_FREE: u8 = 0;
pub const TOUCH_FIXED_A: u8 = 1;
pub const TOUCH_FIXED_B: u8 = 2;

pub fn touch_type() -> u8 {
    TOUCH_TYPE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_touch_type(t: u8) {
    TOUCH_TYPE.store(t.min(TOUCH_FIXED_B), std::sync::atomic::Ordering::Relaxed);
}

/// fixed_A: the stick is drawn and used, but pinned to its home seat.
pub fn stick_pinned() -> bool {
    touch_type() == TOUCH_FIXED_A
}

/// The plates the player has switched off (hero3 82차), as a bitmask. Sanitised on the
/// way in, so every reader can trust that at least one plate is visible.
pub fn quick_hidden() -> u8 {
    QUICK_HIDDEN.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_quick_hidden(mask: u8) {
    QUICK_HIDDEN.store(quickpage::sanitise_hidden(mask), std::sync::atomic::Ordering::Relaxed);
}

/// Effect-bus level. See [`audio_mix::SFX_LEVELS`].
pub fn sfx_level() -> u8 {
    SFX_LEVEL.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_sfx_level(level: u8) {
    SFX_LEVEL.store(level, std::sync::atomic::Ordering::Relaxed);
}

/// 경험치 배율 — the multiplier (1 = 기존). See [`guest_build::EXP_RATES`].
pub fn exp_rate() -> u8 {
    EXP_RATE.load(std::sync::atomic::Ordering::Relaxed)
}

/// 오토루팅 (2026-09-24) — pick up every drop within two tiles while the bag has room.
/// Seeded from `wie_auto_loot.txt` before the worker starts; default ON.
static AUTO_LOOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn auto_loot() -> bool {
    AUTO_LOOT.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_auto_loot(on: bool) {
    AUTO_LOOT.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn set_exp_rate(mult: u8) {
    EXP_RATE.store(guest_build::exp_rate_sanitise(mult), std::sync::atomic::Ordering::Relaxed);
}

/// 난이도 (2026-09-29) — 0 EASY · 1 NORMAL · 2 HARD (`guest_build::DIFFICULTY_PCT`: monsters'
/// HP and attack ×1 / ×1.5 / ×2). An atomic for the same reason as [`exp_rate`]: the WORKER
/// keeps every monster's words at it a few frames after a change (`emulator::Difficulty`).
/// Seeded from `wie_difficulty.txt` before the worker starts; one file for the device, not
/// per save (user decision 2026-09-29).
static DIFFICULTY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(guest_build::DIFFICULTY_DEFAULT);

pub fn difficulty() -> u8 {
    DIFFICULTY.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_difficulty(d: u8) {
    DIFFICULTY.store(guest_build::difficulty_sanitise(d), std::sync::atomic::Ordering::Relaxed);
}

/// 드랍률 (2026-10-16) — 0 낮음 · 1 보통 · 2 높음 (`guest_build::DROP_RATE_KEEP`: the share of the equipment
/// drops kept, 1/16 / ¼ / all — ×¼ a step). The WORKER keeps the guest's filter at it (`emulator::DropRate`), so a change on
/// the 환경설정 row reaches the next kill. Seeded from `wie_drop_rate.txt` before the worker starts; one file for
/// the device, like 난이도.
static DROP_RATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(guest_build::DROP_RATE_DEFAULT);

pub fn drop_rate() -> u8 {
    DROP_RATE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_drop_rate(d: u8) {
    DROP_RATE.store(guest_build::drop_rate_sanitise(d), std::sync::atomic::Ordering::Relaxed);
}

/// 게임패드 전용 HUD (59차). See [`padhud::PadHud`] for the three states and
/// [`pad_hud_active`] for what the HUD actually asks.
pub fn pad_hud_pref() -> padhud::PadHud {
    match PAD_HUD.load(std::sync::atomic::Ordering::Relaxed) {
        1 => padhud::PadHud::On,
        2 => padhud::PadHud::Off,
        _ => padhud::PadHud::Auto,
    }
}

pub fn set_pad_hud_pref(p: padhud::PadHud) {
    let v = match p {
        padhud::PadHud::Auto => 0,
        padhud::PadHud::On => 1,
        padhud::PadHud::Off => 2,
    };
    PAD_HUD.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// Is the pad HUD showing right now? The walk controls (arrows / stick) stand down
/// and every pad-bound control wears a position badge.
pub fn pad_hud_active() -> bool {
    pad_hud_pref().active(PAD_LAST.load(std::sync::atomic::Ordering::Relaxed))
}

/// Record which input source was used last. `true` from the gamepad entry points,
/// `false` from a screen touch — under AUTO this is what flips the HUD, so it must
/// be called from the INPUT path and never from the drawing path.
pub fn note_input_source(pad: bool) {
    PAD_LAST.store(pad, std::sync::atomic::Ordering::Relaxed);
    if pad {
        PAD_SEEN.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Has this session seen any pad input? Gates the settings row only.
pub fn pad_seen() -> bool {
    PAD_SEEN.load(std::sync::atomic::Ordering::Relaxed)
}

/// ★ 2026-10-28 환경설정의 세이브 행: the file screens' answers, `(request id, status)`, oldest first.
/// A QUEUE, not one slot (chronos review 2): the late answer of a request the worker gave up on must
/// not overwrite the answer of the one it is waiting for. Capped — a run of late answers drops the
/// oldest.
static SAVE_DOC_RESULTS: std::sync::Mutex<Vec<(u32, i32)>> = std::sync::Mutex::new(Vec::new());
const SAVE_DOC_RESULTS_MAX: usize = 64;

pub fn push_save_doc_result(id: u32, status: i32) {
    log::info!("save doc: the file screen of request {id} answered {status}");
    if let Ok(mut q) = SAVE_DOC_RESULTS.lock() {
        if q.len() >= SAVE_DOC_RESULTS_MAX {
            q.remove(0);
        }
        q.push((id, status));
    }
}

pub fn take_save_doc_results() -> Vec<(u32, i32)> {
    SAVE_DOC_RESULTS.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

/// For v1 the game archive is bundled as an APK asset (no file picker yet).
pub const BUNDLED_GAME_ASSET: &str = "hero2_ktf.zip";

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use std::sync::{
        Arc,
        atomic::AtomicBool,
        mpsc::channel,
    };

    use winit::{event_loop::EventLoop, platform::android::EventLoopBuilderExtAndroid};

    // ── ★ WHY THIS IS A PER-MODULE FILTER AND NOT JUST A LEVEL ──────────────────
    //
    // Device-measured 2026-08-04, 38 s of a live session on versionCode 5:
    //
    //     3495x  wie_core_arm::thread_wrapper: native thread; id=2
    //     1746x  wie_wipi_c::api::kernel: timer;
    //     1227x  wie_core_arm::thread_wrapper: native thread; id=3
    //
    // = **~124 `__android_log_write` calls per second, all from inside the emulator
    // tick loop.** Each one is a write to logd, on the thread whose frame budget is
    // 24 ms. This is the same class of mistake as the synchronous vibrate binder
    // call that 13차 traced (see vibration.rs): expensive I/O in the hot path.
    //
    // The old comment here assumed Info was quiet and only Debug was chatty. It is
    // not: `#[tracing::instrument]` spans default to **INFO**, and `wie` puts them on
    // `ArmCoreThreadWrapper::poll` and on the WIPI timer callback — the two things
    // that run most. We deliberately enable tracing's `log` feature (see Cargo.toml)
    // to see wie's internals, and that is what routes those spans here.
    //
    // So: everything defaults to Warn — we keep every wie warning and error,
    // including the `stub MC_knl*` notices that are how missing APIs get found — and
    // only OUR crate stays at Info. The cdylib is named `main`, so `main` is the
    // module path of every `log::` call in this app (e.g. `main::database`, which
    // NATIVE_REHOST_HANDOFF's save-verification procedure greps for).
    //
    // To get the old firehose back for a debugging session, change "warn" to "info"
    // (or "debug" for the WIPI-C DB events) in the parse string below.
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_filter(android_logger::FilterBuilder::new().parse("warn,main=info").build())
            .with_tag("wie"),
    );
    std::panic::set_hook(Box::new(|info| {
        log::error!("PANIC: {info}");
    }));
    log::info!("android_main: starting wie_android");

    // Read the bundled game + resolve app-private storage BEFORE the AndroidApp
    // is moved into the event loop builder.
    let game_zip = match read_asset(&app, BUNDLED_GAME_ASSET) {
        Some(bytes) => {
            log::info!("loaded asset {BUNDLED_GAME_ASSET} ({} bytes)", bytes.len());
            bytes
        }
        None => {
            log::error!("bundled game asset {BUNDLED_GAME_ASSET} not found; aborting");
            return;
        }
    };
    let data_dir = match app.internal_data_path() {
        Some(p) => p,
        None => {
            log::error!("no internal_data_path; aborting");
            return;
        }
    };
    log::info!("internal data dir = {data_dir:?}");
    let _ = DATA_DIR.set(data_dir.clone());
    // ★ 2026-10-02 세이브 옮기기: the archive's AID — the directory an import writes into — read from
    //   the bundled game itself, so a phone that has never saved (no such directory yet) knows it too.
    let archive_aid = save_transfer::archive_aid(&game_zip);
    match &archive_aid {
        Some(aid) => {
            let _ = ARCHIVE_AID.set(aid.clone());
        }
        None => log::warn!("save transfer: the bundled game's AID could not be read — imports will be refused"),
    }

    // ★ BEFORE the emulator exists, so no guest holds the character in its heap
    // and nothing can write over what we put back. A save-vault restore that the
    // settings dialog scheduled is applied exactly here and nowhere else — see
    // `vault::request_restore` for why it cannot be applied while the game runs.
    // ★ 세이브 옮기기 / ★ 2026-10-28 세이브 불러오기 · 되돌리기: a staged file — or one an earlier start
    //   left half-applied — FIRST, before anything reads a save (`savebundle::apply_pending`). A start
    //   that cannot make the disk one whole phone does not run the game at all (`Outcome::Blocked`):
    //   nothing below runs, `App` does not boot and says why. A panic in it counts as blocked too —
    //   the journal it was working from is still on the disk for the next start.
    let bundle = std::panic::catch_unwind(|| vault::apply_pending_bundle(&data_dir, archive_aid.as_deref())).unwrap_or_else(|_| {
        log::error!("save bundle: the apply panicked — not starting the game on it");
        vault::set_boot_blocked();
        savebundle::Outcome::Blocked
    });
    if bundle == savebundle::Outcome::Blocked {
        log::error!("save bundle: this start is BLOCKED — the game does not run until the disk is one whole save");
    } else {
        if bundle == savebundle::Outcome::Idle {
            if let Some(what) = vault::apply_pending_restore(&data_dir) {
                log::warn!("vault: {what}");
            }
        } else {
            vault::drop_restore_request(&data_dir);
        }
        // ★ 2026-10-03: replacement save files an adb user staged in `save_put/<aid>/…` — only the
        //   files staged are written (each touched slot kept in the vault first). The way IN that
        //   matches the export marker below. See `vault::put_live_on_request`.
        if let Some(what) = vault::put_live_on_request(&data_dir, std::path::Path::new(config::SDCARD_FILES), archive_aid.as_deref()) {
            log::warn!("vault: {what}");
        }
        // ★ 2026-09-26: a support copy of the LIVE saves, only when an adb-placed marker asks
        //   (the release build is not debuggable). After a scheduled restore, so the copy is
        //   what the game will load. See `vault::export_live_on_request`.
        if let Some(what) = vault::export_live_on_request(&data_dir, std::path::Path::new(config::SDCARD_FILES)) {
            log::warn!("vault: {what}");
        }
    }

    // Restore the saved orientation preference before the window appears.
    if config::read_landscape(&data_dir) {
        orientation::set(true);
    }

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .with_android_app(app)
        .build()
        .expect("failed to build android event loop");
    let proxy = event_loop.create_proxy();
    // Gamepad JNI relay (58차): MainActivity's dispatch hooks post through this.
    let _ = GAMEPAD_PROXY.set(std::sync::Mutex::new(event_loop.create_proxy()));

    let (input_tx, input_rx) = channel::<InputEvent>();
    let should_redraw = Arc::new(AtomicBool::new(true));
    // Shared background/lock gate. The UI thread sets it on suspend / focus-loss;
    // while set, the emulator worker stops ticking (no new sound events, saves
    // battery) and the MIDI synth emits silence — so audio does NOT keep playing
    // when the app is backgrounded or the screen is locked. Cleared on resume /
    // focus-gain (BGM resumes seamlessly).
    let paused = Arc::new(AtomicBool::new(false));
    // UI thread -> worker: how far to push the game's own top HUD band down.
    let hud_y: HudYShift = Arc::new(std::sync::atomic::AtomicU32::new(0));

    // The emulator boot is DEFERRED to the first `resumed` so the guest
    // framebuffer can be sized from the real window (landscape immersive mode).
    let pending_boot = app::PendingBoot {
        proxy,
        should_redraw,
        input_rx,
        game_zip,
        hud_y: hud_y.clone(),
    };
    let mut handler = app::App::new(input_tx, data_dir, paused, hud_y, pending_boot);
    if let Err(e) = event_loop.run_app(&mut handler) {
        log::error!("event loop exited with error: {e}");
    }
}

/// The app-private data directory, for the JNI entry points below — they run on
/// the Java main thread and have no `App` to ask.
static DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// ★ 세이브 옮기기: the AID of the bundled archive (`save_transfer::archive_aid`), for the same JNI
/// entry points. Unset until `android_main` has read the asset — they fall back to the root on disk.
static ARCHIVE_AID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Save-vault entry points for `MainActivity`'s restore dialog.
///
/// Java asks Rust rather than being handed the list, because the vault's contents
/// change while the app runs — every save rotates it — and a list captured when
/// the diagnostics dialog opened would offer generations that no longer exist.
///
/// ★ cfg-gated because `jni` is an android-only dependency: without the gate the
/// HOST build (`cargo test -p wie_android`, the documented Linux test path) does
/// not compile at all.
#[cfg(target_os = "android")]
mod vault_jni {
    use jni::{
        objects::{JObject, JString},
        sys::{jboolean, jclass, jint, jstring},
    };

    /// ⛔ EVERY BODY IN THIS MODULE RUNS INSIDE [`guard`] (2026-09-20, review).
    /// `MainActivity.showSaveRestore` wraps both natives in `catch (Throwable)`, which
    /// reads as "a native failure degrades to an empty list" — but a Rust panic reaching
    /// an `extern "system"` boundary ABORTS the process before Java sees anything, so the
    /// catch is not the safety net it looks like. This is the dialog a player opens when
    /// their character is already in trouble; it is the last place that may take the app
    /// down with it.
    fn guard<T>(what: &str, fallback: T, body: impl FnOnce() -> T) -> T {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
            Ok(v) => v,
            Err(_) => {
                // The panic hook has already logged the payload and the location.
                log::error!("{what}: panicked — returning the safe answer instead of aborting");
                fallback
            }
        }
    }

    /// `String nativeVaultList()` — `slot|unix_ms|char_len` lines, newest first,
    /// empty when the vault holds nothing restorable.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeVaultList(
        env: *mut jni::sys::JNIEnv,
        _class: jclass,
    ) -> jstring {
        guard("nativeVaultList", std::ptr::null_mut(), || vault_list(env))
    }

    fn vault_list(env: *mut jni::sys::JNIEnv) -> jstring {
        let lines = match crate::DATA_DIR.get() {
            Some(dir) => {
                // Opening this dialog is also the moment support gets involved, so
                // drop a copy of the whole vault where a PC can reach it without
                // root. The release build is not debuggable, so this is the only
                // channel a player's backups can travel on.
                let dest = std::path::PathBuf::from(crate::config::SDCARD_FILES).join("save_vault");
                match crate::vault::export(dir, &dest) {
                    Ok(n) if n > 0 => log::warn!("vault: exported {n} file(s) to {dest:?}"),
                    Ok(_) => {}
                    Err(e) => log::info!("vault: export skipped ({e})"),
                }
                crate::vault::offer_lines(dir)
            }
            None => String::new(),
        };
        let Ok(env) = (unsafe { jni::JNIEnv::from_raw(env) }) else {
            log::error!("nativeVaultList: null JNIEnv");
            return std::ptr::null_mut();
        };
        match env.new_string(lines) {
            Ok(s) => s.into_raw(),
            Err(e) => {
                log::error!("nativeVaultList: {e}");
                std::ptr::null_mut()
            }
        }
    }

    /// `boolean nativeVaultRestore(String key)` — schedule the restore of the
    /// generation one of those lines names. Applied on the NEXT launch (see
    /// `vault::apply_pending_restore`), so the caller closes the app.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeVaultRestore(
        env: *mut jni::sys::JNIEnv,
        _class: jclass,
        key: jstring,
    ) -> jboolean {
        guard("nativeVaultRestore", 0, || vault_restore(env, key))
    }

    fn vault_restore(env: *mut jni::sys::JNIEnv, key: jstring) -> jboolean {
        let Ok(mut env) = (unsafe { jni::JNIEnv::from_raw(env) }) else {
            log::error!("nativeVaultRestore: null JNIEnv");
            return 0;
        };
        let key: JString = unsafe { JObject::from_raw(key) }.into();
        let Ok(key) = env.get_string(&key) else {
            log::error!("nativeVaultRestore: unreadable key");
            return 0;
        };
        let key: String = key.into();
        let ok = match crate::DATA_DIR.get() {
            Some(dir) => crate::vault::request_restore(dir, &key),
            None => false,
        };
        u8::from(ok)
    }

    // ── 세이브 옮기기 (2026-10-02) ───────────────────────────────────────────
    // `MainActivity`'s 세이브 관리 dialog. Java hands over its own `getFilesDir()` (the directory
    // `internal_data_path` is) because these can run before `android_main` has set `DATA_DIR` — a
    // process the system recreated while its file picker was open — and it only moves bytes
    // between the picker / share sheet and a file in its cache dir. Every answer is a string:
    // `ok…`, or `err|<a sentence for the player>`.

    fn read_jstring(env: &mut jni::JNIEnv, s: jstring) -> Option<String> {
        if s.is_null() {
            return None;
        }
        let s: JString = unsafe { JObject::from_raw(s) }.into();
        env.get_string(&s).ok().map(Into::into)
    }

    /// The data dir `android_main` was given once it has run, else Java's.
    fn transfer_dir(java: Option<String>) -> Option<std::path::PathBuf> {
        match (crate::DATA_DIR.get(), java) {
            (Some(dir), Some(j)) if std::path::Path::new(&j) != dir.as_path() => {
                log::warn!("save transfer: Java's files dir {j:?} is not the data dir {dir:?} — using the data dir");
                Some(dir.clone())
            }
            (Some(dir), _) => Some(dir.clone()),
            (None, j) => j.map(std::path::PathBuf::from),
        }
    }

    /// Run `body` with a usable env and hand its text back as a Java string.
    fn reply(env: *mut jni::sys::JNIEnv, what: &str, body: impl FnOnce(&mut jni::JNIEnv) -> String) -> jstring {
        let Ok(mut env) = (unsafe { jni::JNIEnv::from_raw(env) }) else {
            log::error!("{what}: null JNIEnv");
            return std::ptr::null_mut();
        };
        let text = body(&mut env);
        match env.new_string(text) {
            Ok(s) => s.into_raw(),
            Err(e) => {
                log::error!("{what}: {e}");
                std::ptr::null_mut()
            }
        }
    }

    /// The pieces every transfer call needs, or the player's reason it cannot run.
    fn transfer_args(env: &mut jni::JNIEnv, files_dir: jstring, path: jstring) -> Result<(std::path::PathBuf, String), String> {
        match (transfer_dir(read_jstring(env, files_dir)), read_jstring(env, path)) {
            (Some(dir), Some(path)) => Ok((dir, path)),
            _ => Err("err|앱 정보를 읽지 못했습니다.".to_string()),
        }
    }

    /// `String nativeSaveExport(String filesDir, String outPath, int appVersion)` — write this
    /// phone's transfer file to `outPath`: `ok|<slots, comma separated>` or `err|…`.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeSaveExport(
        env: *mut jni::sys::JNIEnv,
        _class: jclass,
        files_dir: jstring,
        out_path: jstring,
        app: jint,
    ) -> jstring {
        guard("nativeSaveExport", std::ptr::null_mut(), || {
            reply(env, "nativeSaveExport", |env| {
                let (dir, out) = match transfer_args(env, files_dir, out_path) {
                    Ok(a) => a,
                    Err(e) => return e,
                };
                let aid = crate::ARCHIVE_AID.get().map(String::as_str);
                match crate::vault::export_to(&dir, aid, app.max(0) as u32, std::path::Path::new(&out)) {
                    Ok(slots) => format!("ok|{}", slots.iter().map(u8::to_string).collect::<Vec<_>>().join(",")),
                    Err(e) => format!("err|{e}"),
                }
            })
        })
    }

    /// `void nativeSetAppInfo(int offsetMinutes, int versionCode)` — the device's UTC offset (the dates
    /// the save dialogs show) and this build's versionCode (stamped into an exported file; a NEWER
    /// app's file is refused). Called from `MainActivity.onCreate` BEFORE `super.onCreate` (which
    /// starts `android_main`), so the start's own dialog has them.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeSetAppInfo(_env: *mut jni::sys::JNIEnv, _class: jclass, offset_min: jint, version: jint) {
        guard("nativeSetAppInfo", (), || {
            crate::vault::set_tz_offset_min(offset_min);
            crate::vault::set_app_version(version.max(0) as u32);
        });
    }

    /// `void nativeSaveDocResult(int id, int status)` — a file screen's answer (★ 2026-10-28 환경설정의
    /// 세이브 행): request `id`, `status` 0 done · 1 cancelled · 2 failed. Java answers every request
    /// exactly once (its `finally`), from its copy thread; the worker takes it on its next loop.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeSaveDocResult(_env: *mut jni::sys::JNIEnv, _class: jclass, id: jint, status: jint) {
        guard("nativeSaveDocResult", (), || crate::push_save_doc_result(id.max(0) as u32, status));
    }

    /// `String nativeSaveImportCheck(String filesDir, String inPath, int appVersion)` — the file
    /// at `inPath` as the confirm dialog shows it (`vault::import_summary`), or `err|…`.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeSaveImportCheck(
        env: *mut jni::sys::JNIEnv,
        _class: jclass,
        files_dir: jstring,
        in_path: jstring,
        app: jint,
    ) -> jstring {
        guard("nativeSaveImportCheck", std::ptr::null_mut(), || {
            reply(env, "nativeSaveImportCheck", |env| {
                let (dir, path) = match transfer_args(env, files_dir, in_path) {
                    Ok(a) => a,
                    Err(e) => return e,
                };
                let aid = crate::ARCHIVE_AID.get().map(String::as_str);
                crate::vault::import_summary(&dir, aid, app.max(0) as u32, std::path::Path::new(&path)).unwrap_or_else(|e| format!("err|{e}"))
            })
        })
    }

    /// `String nativeSaveImportSchedule(String filesDir, String inPath, int appVersion)` — stage
    /// the file for the next launch (`vault::stage_import`): `ok` or `err|…`. The caller then
    /// closes the app; `android_main` applies it before the emulator exists.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeSaveImportSchedule(
        env: *mut jni::sys::JNIEnv,
        _class: jclass,
        files_dir: jstring,
        in_path: jstring,
        app: jint,
    ) -> jstring {
        guard("nativeSaveImportSchedule", std::ptr::null_mut(), || {
            reply(env, "nativeSaveImportSchedule", |env| {
                let (dir, path) = match transfer_args(env, files_dir, in_path) {
                    Ok(a) => a,
                    Err(e) => return e,
                };
                let aid = crate::ARCHIVE_AID.get().map(String::as_str);
                // ★ 2026-10-28: `Unsure` is a stage that may be on the disk — the app closes all the same
                //   (Java's `ok…` path), since going on would let the next start apply it over what is played.
                match crate::vault::stage_import(&dir, aid, app.max(0) as u32, std::path::Path::new(&path)) {
                    Ok(crate::vault::Staged::Done) => "ok".to_string(),
                    Ok(crate::vault::Staged::Unsure) => "ok|unsure".to_string(),
                    Err(e) => format!("err|{e}"),
                }
            })
        })
    }
}

/// Route from the Java main thread (where `MainActivity.dispatch*` runs) to the
/// winit thread: gamepad JNI natives post `UserEvent`s through this proxy. Set once
/// in `android_main` before the event loop runs; events arriving before that (the
/// activity is not even focused yet) are dropped harmlessly.
#[cfg(target_os = "android")]
static GAMEPAD_PROXY: std::sync::OnceLock<std::sync::Mutex<winit::event_loop::EventLoopProxy<UserEvent>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "android")]
fn send_gamepad_event(ev: UserEvent) {
    if let Some(proxy) = GAMEPAD_PROXY.get()
        && let Ok(proxy) = proxy.lock()
    {
        // Fails only while the loop is shutting down — nothing to do about it.
        let _ = proxy.send_event(ev);
    }
}

/// The `native` methods `MainActivity` declares. Resolved by SYMBOL NAME when Java
/// calls them (`System.loadLibrary("main")` has already loaded this cdylib), so the
/// JNI naming convention in the function names is load-bearing — as is
/// `extern "system"`, the JNI calling convention.
///
/// These run on the JAVA MAIN THREAD, not the winit thread: they must only relay,
/// never touch `App` state — hence the `EventLoopProxy` hop above.
#[cfg(target_os = "android")]
mod gamepad_jni {
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeGamepadButton(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        code: jni::sys::jint,
        down: jni::sys::jboolean,
        repeat: jni::sys::jboolean,
    ) {
        crate::send_gamepad_event(crate::UserEvent::GamepadButton {
            code,
            down: down != 0,
            repeat: repeat != 0,
        });
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_wie_emulator_MainActivity_nativeGamepadAxes(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        x: jni::sys::jfloat,
        y: jni::sys::jfloat,
        hx: jni::sys::jfloat,
        hy: jni::sys::jfloat,
        lt: jni::sys::jfloat,
        rt: jni::sys::jfloat,
    ) {
        crate::send_gamepad_event(crate::UserEvent::GamepadAxes { x, y, hx, hy, lt, rt });
    }
}

/// Read a file bundled in the APK's `assets/` via the NDK AssetManager.
#[cfg(target_os = "android")]
fn read_asset(app: &winit::platform::android::activity::AndroidApp, name: &str) -> Option<Vec<u8>> {
    use std::io::Read;

    let manager = app.asset_manager();
    let cname = std::ffi::CString::new(name).ok()?;
    let mut asset = manager.open(&cname)?;
    let mut buf = Vec::new();
    asset.read_to_end(&mut buf).ok()?;
    Some(buf)
}
