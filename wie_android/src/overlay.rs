//! On-screen touch controls + settings gear + settings modal, drawn directly
//! into the softbuffer framebuffer. Orientation-aware: portrait places the game
//! on top with controls below; landscape centres the game with the D-pad on the
//! left and the keypad on the right.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use wie_backend::KeyCode;

use crate::{CONTENT_H, CONTENT_W, IconRef, QuickSlot, SlotKind, Stats, atlas, ui_icons};

const BTN: u32 = 0xff2c_3040;
const BTN_DPAD: u32 = 0xff26_3548;
const BTN_PRESSED: u32 = 0xff4a_6cff;
const BORDER: u32 = 0xff70_80a0;
const LABEL: u32 = 0xffff_ffff;
const PANEL: u32 = 0xff1a_1c26;
/// Quickbar action buttons — skills blue, items green, empty dim; pressed = bright.
const QB_SKILL: u32 = 0xff2a_4a8a;
const QB_ITEM: u32 = 0xff1f_5a3a;
const QB_EMPTY: u32 = 0xff2a_2e3a;
/// A HOST plate with nothing bound (`crate::quickslot_inert`) — darker than empty,
/// because it is not "you may put something here", it is "this cannot be pressed yet".
const QB_INERT: u32 = 0xff18_1a22;
const QB_PRESSED: u32 = 0xff4a_6cff;
// (The player-gauge capsule colours lived here until 2026-09-29; the status block's
// palette is hero5's now, in `hud_view.rs`.)

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub(crate) fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }
    fn contains(&self, px: f64, py: f64) -> bool {
        let (xi, yi) = (px as i64, py as i64);
        xi >= self.x as i64 && xi < (self.x + self.w) as i64 && yi >= self.y as i64 && yi < (self.y + self.h) as i64
    }
    /// Half-open overlap test — what keeps [`crate::placement`]'s controls off the
    /// OBSTACLES (controls may overlap each other freely since 2026-08-07).
    /// Touching edges do NOT count as overlapping.
    pub(crate) fn intersects(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
    /// Grown by `n` on every side, clipped at the window origin. Used to keep a
    /// visible gap between a control and the things it must NOT touch (the game's own
    /// HUD bands, our menu chrome, the gauge tap zone).
    pub(crate) fn grown(&self, n: u32) -> Rect {
        Rect::new(self.x.saturating_sub(n), self.y.saturating_sub(n), self.w + n * 2, self.h + n * 2)
    }
    /// The part of `self` that is inside `o` and inside a `w × h` window, if any.
    pub(crate) fn overlap(&self, o: &Rect, w: u32, h: u32) -> Option<Rect> {
        let (x0, y0) = (self.x.max(o.x), self.y.max(o.y));
        let x1 = (self.x + self.w).min(o.x + o.w).min(w);
        let y1 = (self.y + self.h).min(o.y + o.h).min(h);
        (x0 < x1 && y0 < y1).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }

}

/// The standard control margin for a window — one place, so every producer agrees.
pub(crate) fn pad_of(win_w: u32, win_h: u32) -> u32 {
    (win_w.min(win_h) / 110).max(3)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Hash)]
enum Label {
    Text(&'static str),
    Arrow(Dir),
}

/// How a button paints its background.
///
/// ★ 2026-07-30 player request: "the 1/3/7/9/0/#/CANCEL keys and the 1/3/7/9 keys
/// that appear with the menu open are square — make them ALL round". So every
/// immersive on-screen key is [`Face::Round`]; only the opaque portrait layouts
/// (which have no game showing through) keep [`Face::Solid`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Face {
    /// Opaque disc — non-immersive portrait D-pad / keypad layouts, which sit on a
    /// black control column rather than on the game, so their face is opaque.
    /// (Round too: the player said "make them ALL round", and those layouts hold a
    /// third copy of the same 1/3/7/9/0/#/CLR keys.)
    Solid,
    /// Translucent anti-aliased disc. softbuffer has no alpha channel, so the
    /// blend is done per pixel here.
    Round,
    /// The big bottom-right ACTION disc: [`Face::Round`] plus an inner gold ring so
    /// the one key that matters most still stands out from its neighbours.
    Action,
    /// Paint nothing — the widget draws its own face later (the minimap panel).
    None,
    /// A translucent HEXAGON for the arrow pointing `Dir`, inked inside the carried
    /// rect, with a small arrow glyph in its middle.
    ///
    /// ★ hero3 91차 (2026-09-13), player: "이제 삼각형이 아니라 각각 동서남북이 육각형
    /// 모형이 되는거야." It replaces the 2026-08-09 triangle (`Face::Triangle`), which
    /// was itself the replacement of a round key with a triangle glyph — the BUTTON is
    /// a hexagon now, and since six identical corners say nothing about direction, the
    /// glyph is back inside it.
    ///
    /// ★ It carries its own rect because the arrows are the one control whose TOUCH
    /// TARGET is deliberately larger than its ink (2026-08-17, "표시되는 영역보다 조금
    /// 더 큰범위를 눌러도 동작하게"): `Button::rect` is the arm's square, `wedge` its
    /// quarter, this is [`Arm::draw`]. Every other face inks its whole button.
    Hexagon(Dir, Rect),
}

pub struct Button {
    rect: Rect,
    pub key: KeyCode,
    label: Label,
    dpad: bool,
    face: Face,
    /// The touch target when it is NOT `rect`: a D-pad arrow owns a diagonal
    /// QUARTER of its square (hero3 91차-B), and a quarter is not a rectangle. `None`
    /// for every other button — `rect` is the target. See [`Wedge`].
    wedge: Option<Wedge>,
}

impl Button {
    /// Does a touch at `(x, y)` belong to this button? The one place the two target
    /// shapes are told apart, so `hit` and the slide-between-keys path agree.
    fn contains(&self, x: f64, y: f64) -> bool {
        match self.wedge {
            Some(w) => w.contains(x, y),
            None => self.rect.contains(x, y),
        }
    }
}

pub struct Layout {
    pub game: Rect,
    pub buttons: Vec<Button>,
    pub gear: Rect,
    /// The floating-joystick zone (left control area) when joystick mode is on;
    /// `None` in classic D-pad mode. A touch here (that misses the corner
    /// buttons) drives the joystick.
    pub joystick: Option<Rect>,
    /// The full-keypad toggle (immersive mode only; zero-sized elsewhere, where a
    /// real keypad is already on screen).
    pub keypad_toggle: Rect,
}

/// What the overlay is showing. The emulator worker tells us whether the
/// real-time field has control or one of the game's OWN windows (main menu,
/// inventory, dialogue, shop, …) is open — the controls a player needs differ
/// completely, and drawing the field HUD over a game window is what produced the
/// "HP/MP bars read 0 in the menu" report.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// Real-time field free-roam: skill plates + gauges + minimap panel.
    #[default]
    Field,
    /// A game window is open: menu-navigation keys on the LEFT, the quickbar
    /// plates still on the right.
    Menu,
    /// The game's SCRIPT window (id 4) is playing — a story scene or NPC dialogue.
    /// Only OK and `#` do anything there, so only those two are drawn (player
    /// request 2026-08-03). Placement-wise this is NOT a third layout: it borrows
    /// the MENU set's seats (see `placement::ids_for`), so a player who arranges
    /// their menu controls gets the cutscene pair in the same two places.
    Cutscene,
}

/// The keys the game's own windows need and which the immersive layout otherwise
/// has NO seat for. Player-reported: `1`/`3` move between menus (tabs), `7`/`9`
/// page within one, and `#` (added 2026-08-03) confirms / registers in the game's
/// own context popups — without it the "1·3·7·9 에 등록" list could be opened but
/// not answered.
///
/// ★ These are drawn on the LEFT since 2026-08-03. They used to take the same
/// bottom-right ring seats the quickbar uses on the field, which meant the two sets
/// were mutually exclusive; the player asked for both at once — nav keys under the
/// left thumb, quickbar plates under the right.
/// The pull-out full keypad (immersive mode). Every remaining phone key the
/// game's windows can ask for, so no game state is ever unreachable: the 12-key
/// pad plus both soft keys. `None` = an empty cell (keeps the grid aligned).
#[allow(clippy::type_complexity)]
const PAD_KEYS: [[Option<(&str, KeyCode)>; 3]; 5] = [
    [Some(("1", KeyCode::NUM1)), Some(("2", KeyCode::NUM2)), Some(("3", KeyCode::NUM3))],
    [Some(("4", KeyCode::NUM4)), Some(("5", KeyCode::NUM5)), Some(("6", KeyCode::NUM6))],
    [Some(("7", KeyCode::NUM7)), Some(("8", KeyCode::NUM8)), Some(("9", KeyCode::NUM9))],
    [Some(("*", KeyCode::STAR)), Some(("0", KeyCode::NUM0)), Some(("#", KeyCode::HASH))],
    [Some(("LSK", KeyCode::LEFT_SOFT_KEY)), None, Some(("RSK", KeyCode::RIGHT_SOFT_KEY))],
];

/// A live view of the active joystick, for rendering (base at `center`, thumb at
/// `thumb`, both in window pixels).
#[derive(Clone, Copy)]
pub struct JoyView {
    pub center: (f64, f64),
    pub thumb: (f64, f64),
}

/// ★ `SetPortrait` .. `CycleBaseH` are RETAINED but unreachable: their settings rows
/// were removed on player request 2026-07-31 (the modal is LAYOUT + SERVER only). The
/// features themselves still work from their persisted config files, and putting a row
/// back is one line in [`settings_items`] — so this is a deliberate, reversible
/// amputation of the UI, not a deletion of working code.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    SetPortrait,
    SetLandscape,
    SetDpad,
    SetJoystick,
    /// Toggle landscape immersive (full-screen) mode — applies on next start.
    ToggleImmersive,
    /// Cycle the immersive logical height (zoom vs fill) — applies on next start.
    CycleBaseH,
    /// Open the layout editor: drag a control to move it, two fingers to resize it
    /// (player request 2026-07-31).
    EditLayout,
    /// Flip TOUCH TYPE A <-> B (player request 2026-08-16): A = the floating stick
    /// that spawns under the finger, B = the four-arrow D-pad pinned bottom-left
    /// (second round of the same day — B started as a pinned stick and the player
    /// asked for arrows instead, with the stick hidden outright). Takes effect at
    /// once — see [`crate::stick_fixed`].
    ToggleTouchType,
    /// Ask for the private-server address (복구 / 네트워크 창고 / 캐시샵 destination).
    /// Opens the platform text dialog — the overlay font is a 5x7 ASCII bitmap with
    /// no input machinery, and a hostname needs a real keyboard.
    EditServer,
    /// Show / replace the account code. The only way to carry the server-side
    /// warehouse and character backups to another phone.
    EditAccount,
    /// Step the 효과음 크기 row (hero3 2026-08-27, ported 2026-09-18).
    CycleSfxLevel,
    /// Step the 경험치 배율 row: 기존 → x2 → x4 → x6 (user request 2026-09-23). Applies
    /// at once — the worker keeps the guest's multiplier word equal to
    /// [`crate::exp_rate`] (`emulator::ExpRate`).
    CycleExpRate,
    /// Toggle 오토루팅 (user request 2026-09-24): drops within two tiles are picked up by
    /// the game's own pickup, all of them, while the bag has room. Applies at once —
    /// the worker reads [`crate::auto_loot`] each poll (`emulator::AutoLoot`).
    ToggleAutoLoot,
    /// Save the game NOW, without walking the menu (player request 2026-08-04:
    /// "빠른저장 버튼"). The worker owns the whole thing — it re-reads the guest's own
    /// "may I save here" byte and drives the game's four save primitives.
    QuickSave,
    /// Show the on-device diagnostics dialog (2026-08-07): audio pipeline status,
    /// app/OS/device versions, window + logical sizes — with a copy button. Exists
    /// because remote testers cannot run adb, and "소리가 안 나요" from another
    /// phone is undebuggable without the init lines it shows.
    Diagnostics,
    /// 발열 절감 (57차): half-resolution render (hardware-upscaled) + lite synth,
    /// for low-spec devices that run hot (S9). Applies at next start — see
    /// `config::read_heat_saver`.
    ToggleHeatSaver,
    /// 게임패드 전용 HUD (59차): cycle AUTO -> ON -> OFF. Applies AT ONCE (the layout
    /// is rebuilt from the preference every relayout) — see `crate::pad_hud_active`.
    TogglePadHud,
    /// 패드 버튼 설정 (2026-10-09, with hero3): open the remap screen — pick an action,
    /// press the button that should do it (`App::padmap_open`, rules in `padmap.rs`).
    PadMap,
    Close,
}

/// See [`SettingsAction`]: the first six are retained for rows that were removed.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SKind {
    Portrait,
    Landscape,
    Dpad,
    Joystick,
    Immersive,
    BaseH,
    EditLayout,
    TouchType,
    Server,
    Account,
    SfxLevel,
    ExpRate,
    AutoLoot,
    Save,
    Diag,
    HeatSaver,
    PadHud,
    PadMap,
    Close,
}

pub struct SettingsItem {
    rect: Rect,
    pub action: SettingsAction,
    kind: SKind,
    /// A row that is drawn but cannot be pressed in the current configuration.
    /// Swallows the tap rather than closing the modal, and is drawn dimmed with a
    /// reason — a full-width button that silently no-ops reads as a broken app.
    enabled: bool,
}

// --------------------------------------------------------------------------
// Layout
// --------------------------------------------------------------------------

// ── regions the GAME owns (never draw overlay chrome there) ──────────────────
//
// Both are drawn by the game's own field HUD, in GUEST framebuffer pixels
// [RE: workflow wf_477f0b16 QA/QB, both CONFIRMED HIGH]:
//
//  * TOP-LEFT — the active buff / status list. Up to 3 slots of `/comm/ui_bm`
//    frame 3 (20x25) at `x = 20*i + 1, y = 2`, each with its remaining-ticks
//    number at y = 18. Absolute top-left coordinates, NOT screen-relative.
//    Rect (1, 2, 60, 25) at 3 slots. The verifier of the companion question read
//    the SAME band as the party portrait row (19 px stride, level digit centred at
//    idx*19+10), i.e. up to 4 entries out to x ~= 76 — the two readings disagree on
//    what the band IS but agree on where it is, so reserve the wider (0, 0, 80, 28)
//    and be correct either way.
//  * TOP-RIGHT — the target monster's name + level + HP bar (`ui_bm` frame 4,
//    85x26) at `x = screen_w - 87, y = 2`. RIGHT-EDGE anchored, so it tracks the
//    widened framebuffer. Reserve (screen_w - 90, 0, 90, 30).
//    ★ hero2 (2026-09-25) draws the same widget — a 66x28 name/level image + HP bar at
//    `(screen_w - 66, 0)` — so its size is a per-build fact now:
//    `guest_build::target_box_guest_size` (90x30 here, 66x28 there).
//
// Player-reported (2026-07-30): our minimap panel covered the monster box, and the
// gauges + gear + keypad toggle covered the buff list.
// Kept (with the selftest mirror) because the Y0 shift below is derived from it:
// how far the game's band has to move is a function of its own height.
#[allow(dead_code)]
const GUEST_BUFFLIST_W: u32 = 80;
#[allow(dead_code)]
const GUEST_BUFFLIST_H: u32 = 28;

/// Map a length in GUEST pixels to window pixels along x / y of `game`.
fn gx(game: Rect, n: u32) -> u32 {
    let (lw, _) = crate::logical();
    n * game.w / lw.max(1)
}
fn gy(game: Rect, n: u32) -> u32 {
    let (_, lh) = crate::logical();
    n * game.h / lh.max(1)
}

/// How far DOWN (in GUEST px) the game's own top HUD band has to be pushed so it
/// clears our gauge block. The band's y is `Y0 + const` for a y-origin the game
/// reads from a global, and the host writes that global — so instead of our HUD
/// dodging the game's, the game's band moves below ours (player request
/// 2026-07-30: "gauges at the extreme top-left, put the buff list under them").
///
/// Derived from our own chrome, so it follows any restyle: the band has to clear
/// BOTH the gauge block (top-left) and the minimap panel (top-right), because the
/// game draws its buff list under the former and its target-monster box under the
/// latter.
pub fn hud_y_shift(_game: Rect) -> u32 {
    // 0 = leave the game's own y-origin alone. The band used to be pushed DOWN to
    // clear our gauges; now it is pushed RIGHT instead (see `band_x_shift`), which
    // is where the player wants it — on the same row as the gauges, in the strip the
    // gear and keypad toggle used to occupy.
    0
}

/// Height (GUEST px) we would like the game's buff band to have.
///
/// ★ 2026-08-05: the ORIGINAL 25, i.e. "do not shrink the band". It used to ask for
/// our HP+SP+EXP stack height so the two would read as one row, which on a tall
/// phone came out below the natural height and was clamped to `items::BAND_H_MIN`
/// (20) — exactly tall enough for the hardcoded 15x15 icon at y+5 but NOT for the
/// frame's own bottom border row, so every buff icon looked shaved at the bottom
/// (player report: "버프 항목의 아이콘 하단 살짝 잘린것처럼 보이는데"). At 25 the
/// full frame fits and nothing is cropped; `items::band_height_poke` also restores
/// a previously shrunk record back to 25 by the same read-compare-write pass.
pub fn band_h_shift(_game: Rect) -> u32 {
    25
}

/// Where the game's own buff band should START, in GUEST px: just right of our
/// gauge block. The band's x is `cell_w * i + K` with K a one-byte immediate in the
/// renderer, so the host can move it (see `items::band_x_patch`).
pub fn band_x_shift(game: Rect) -> u32 {
    if game.w == 0 {
        return 1;
    }
    let b = stats_block(game);
    let right = (b.bars.x + b.bars.w).saturating_sub(game.x);
    let (lw, _) = crate::logical();
    // window px -> guest px, + a little air; the immediate is a u8.
    ((right * lw.max(1) / game.w) + 3).min(200).max(1)
}

/// Window rect of the game's own buff/status list (top-left), at its SHIFTED
/// position (see [`hud_y_shift`]).
pub(crate) fn game_buff_box(game: Rect) -> Rect {
    Rect::new(
        game.x,
        game.y + gy(game, hud_y_shift(game)),
        gx(game, GUEST_BUFFLIST_W),
        gy(game, GUEST_BUFFLIST_H),
    )
}

/// Window rect of the game's own target-monster box (top-right, right-anchored),
/// BEFORE any dodge — see [`game_target_box_at`]. Its y comes from the SAME y-origin
/// global as the buff list, so the shift moves this too (hero3; hero2 has no such
/// global and `hud_y_shift` is 0).
pub(crate) fn game_target_box(game: Rect) -> Rect {
    let (bw, bh) = crate::guest_build::target_box_guest_size();
    let w = gx(game, bw);
    Rect::new(
        (game.x + game.w).saturating_sub(w),
        game.y + gy(game, hud_y_shift(game)),
        w,
        gy(game, bh),
    )
}

/// Largest logical-aspect rect fitting inside `area`, centered. Uses the LIVE
/// guest size (not the 240x320 constants) so an immersive wide frame is never
/// letterboxed with the portrait ratio during a rotation.
fn contain(area: Rect) -> Rect {
    let (lw, lh) = crate::logical();
    let scale = (area.w as f32 / lw as f32).min(area.h as f32 / lh as f32).max(0.0);
    let w = (lw as f32 * scale) as u32;
    let h = (lh as f32 * scale) as u32;
    Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 2, w, h)
}

/// Landscape **immersive** layout: the game frame is the WHOLE window (the guest
/// already runs at an aspect-matched logical size, so filling it is 1:1 and
/// distortion-free), there are NO side control columns, and the controls are
/// translucent overlays drawn on top of the game:
///
/// * a right-edge 2x4 grid of keys — `1 3 / 5 9 / 0 # / OK CLR`
///   (the label is ASCII "CLR", not "취소": `draw_glyph` is a 5x7 ASCII bitmap
///   font, so a Hangul label renders BLANK — device-confirmed 2026-07-28.)
/// * everything outside those buttons (and the gear) is the joystick zone, so a
///   touch anywhere on the field starts a stick there.
///
/// `OK` is included even though the user's 7-key sketch omitted it: dialogue
/// advance / menu confirm has no other binding, so without it the mode would be
/// unusable. Flagged rather than silently dropped.
/// Bounding square of the big bottom-right ACTION disc (the reference layout's
/// primary button). Shared by the layout and the skill cluster so they cannot
/// drift apart.
/// Centre of the whole bottom-right action cluster (disc + ring + the key row beside
/// the disc). FIXED, i.e. derived from the cluster's nominal radius rather than from
/// the disc's own size — shrinking the disc must not drag this toward the corner,
/// or the ring gets squeezed off-screen.
fn action_centre(win_w: u32, win_h: u32, pad: u32) -> (u32, u32) {
    let nominal = (win_h / 6).clamp(56, 190);
    (
        win_w.saturating_sub(pad * 3 + nominal),
        win_h.saturating_sub(pad * 3 + nominal),
    )
}

/// Bounding square of the ACTION disc (key OK). Smaller than the cluster slot it
/// sits in: the player asked for OK and the button beside it to be a same-size pair
/// side by side (2026-07-30), and the space for the second one comes out of the disc.
/// (That neighbour was key `7` then and is 회복 아이템 `0` now — see
/// [`disc_side_button`] — but the geometry is the same and so is the reason for it.)
/// ★ BUILT-IN geometry only. The player's own size/position for OK is applied by
/// [`crate::placement`] AFTER this, never inside it — that is what stops a resized
/// OK from dragging the skill ring around (`ring_slots` derives its radius from the
/// disc, so a disc that grew with the player's knob used to shrink every plate:
/// measured 98 px -> 52 px at the old +40 % knob).
pub(crate) fn action_disc(win_w: u32, win_h: u32, pad: u32) -> Rect {
    let (cx, cy) = action_centre(win_w, win_h, pad);
    let r = (win_h / 8).clamp(48, 140);
    Rect::new(cx.saturating_sub(r), cy.saturating_sub(r), r * 2, r * 2)
}

/// The persistent joystick home (centre + radius): the STICK seat's inscribed
/// circle. This is also the radius of the LIVE stick's base disc.
///
/// ★ The stick is PLACEABLE since 2026-07-31. A left-handed player moves the buttons
/// to the left, and the hint used to stay bottom-LEFT underneath them — measured:
/// after a FLIP the OK disc covered it outright, and because controls win the hit
/// test the stick could no longer be started there. So it moves with the layout.
///
/// ★ And RESIZABLE since 2026-08-16, which is why the radius comes out of the
/// RESOLVED rect rather than out of the built-in seat. Reading the built-in radius
/// here was what made the old size knob a no-op.
///
/// ★★★ 91차-B (2026-09-13): the seat is the ONE walk control's square — the square
/// the menu arrows are cut from (`stick_seat` / `dpad_arms`) — and the stick is cut
/// from that same square. Player: "화살표 전체가 조이스틱 전체고 조이스틱 전체가 화살표
/// 전체인 느낌". The first 91차 cut drew a small disc at the square's centre; that read
/// as "a stick in the middle of the arrows", which is exactly what the player said it
/// must not be. ⛔ **The radius is no longer the square's INSCRIBED circle** — since
/// 2026-09-20 it is [`arm_centre_radius`], the circle through the arrows' centres, which
/// is the same shape one notch smaller. Same report, refined; see that function.
///
/// What the ring does NOT set any more is the walk-start distance — see
/// [`joy_engage_radius`]: a 312 px ring with the old 30 % engage rule would need a
/// 94 px thumb travel (7 mm) before the character moved. The thumb pins INSIDE the
/// base ([`joy_radius`]), so the reason it stops is drawn: it is the cap in its well.
/// ★★ 2026-09-20, user: *"레이아웃 수정에서 화살표의 크기랑 조이스틱 크기를 똑같게 해놨는데
/// 화살표 크기보다 조이스틱이 조금 더 작아야해. 화살 육각형들의 각 중심점을 원으로 연결한게
/// 조이스틱크기라고 생각하면돼"*. The base is no longer the seat's inscribed circle
/// (91차-B) but the circle through the four arrow hexagons' CENTRES — a little inside
/// the arrows rather than flush with them.
///
/// Derived from the SAME three numbers [`dpad_arms`] lays the arms out with, so the
/// disc cannot drift away from the hexagons it is measured against: the UP arm's box
/// starts `inset` inside the edge and is `len` long, so its centre is `inset + len/2`
/// in, against the seat centre's `side/2`. With the shipped percentages that is
/// `0.5 − 0.02 − 0.17 = 0.31` of the side, where it used to be `0.5`.
///
/// The smaller of the two axes, so a seat that is not square keeps the disc inside the
/// nearer pair of arms rather than poking past them.
pub(crate) fn arm_centre_radius(r: &Rect) -> f64 {
    let s = r.w.min(r.h);
    let inset = f64::from((s / 50).max(1));
    let half_len = f64::from((s * ARM_LEN_PCT / 100).max(3)) / 2.0;
    let dv = f64::from(r.h) / 2.0 - inset - half_len;
    let dh = f64::from(r.w) / 2.0 - inset - half_len;
    dv.min(dh).max(1.0)
}

pub fn joy_home(win_w: u32, win_h: u32) -> (f64, f64, f64) {
    let seat = crate::placement::rect_of(crate::placement::CtrlId::Stick, Mode::Field, win_w, win_h)
        .unwrap_or_else(|| stick_seat(win_w, win_h));
    (
        f64::from(seat.x) + f64::from(seat.w) / 2.0,
        f64::from(seat.y) + f64::from(seat.h) / 2.0,
        arm_centre_radius(&seat),
    )
}

/// The thumb disc's radius as a fraction of the base's. The thumb's CENTRE travels at
/// most `1 - JOY_THUMB` of the base radius ([`joy_radius`]), so the thumb never leaves
/// the base — the pin has a drawn reason.
pub const JOY_THUMB: f64 = 0.44;

/// The stick's pre-91차-B radius: `win_h / 12`, clamped. It is no longer drawn
/// anywhere — it survives as the WALK-START basis ([`stick_engage_radius`]), so that
/// the base disc could grow to the arrows' size without the deadzone growing with it.
fn joy_r_legacy(win_h: u32) -> f64 {
    f64::from((win_h / 12).clamp(34, 96))
}

/// The deadzone basis for a resolved STICK seat: the legacy radius scaled by the
/// seat's size percent. `joy_dir` starts walking at 30 % of this and stops at 15 %,
/// so at a 100 % seat the character moves after the same 27 px it always did, and
/// the player's one knob still makes a bigger stick start walking later.
pub(crate) fn stick_engage_radius(seat: &Rect, win_w: u32, win_h: u32) -> f64 {
    let b = stick_seat(win_w, win_h);
    let base = f64::from(b.w.min(b.h).max(1));
    joy_r_legacy(win_h) * f64::from(seat.w.min(seat.h)) / base
}
/// Angular span of the control ring, in screen degrees (`cy = acy - r*sin`, so 90 is UP
/// and 180 is LEFT). The action disc sits in the bottom-right corner, so the reachable
/// arc runs from just right-of-top, over the top, down to just below-left.
const RING_A0: f64 = 66.0;
/// The arc stops short of 204 so the last seat is not hard against the bottom edge.
const RING_A1: f64 = 195.0;
/// Widest angular PITCH the ring will use: the six-seat ring's. It binds only when the
/// ring is SHORTER than six seats; the eight-seat ring's own pitch is already smaller.
const RING_STEP_MAX: f64 = (RING_A1 - RING_A0) / 5.0;

/// Ring geometry around the action disc.
///
/// ★★ 2026-09-10 — THIS IS hero3's ARC, ported verbatim, and it REPLACES the
/// "hero5식 우측 세로 칼럼" grid this app carried from 2026-08-19.
///
/// That grid was not a mistake: `HERO2_REMAKE_PLAN.md` §4 set hero5's in-game HUD as the
/// target, so the plates stacked up the right edge and wrapped into a second column.
/// The player looked at both apps side by side on the same phone and asked for hero3's
/// instead — *"스킬 슬롯 UI는 hero3와 완전 동일하게 나와야해"*, 2026-09-10, with the
/// count settled at seven in the same breath. So the HUD target for the quickbar is now
/// hero3, and CLAUDE.md's opening rule was amended to say so.
///
/// `n` equal steps from [`RING_A0`] toward [`RING_A1`] — reaching it exactly when the
/// pitch is not capped by [`RING_STEP_MAX`] — at a radius that (a) clears the disc and
/// (b) keeps ADJACENT square plates apart. Squares need more than "chord >= side": a
/// diagonal offset can leave both |dx| and |dy| under `side`, hence the 1.485 factor.
fn ring_slots(win_w: u32, win_h: u32, n: usize, pad: u32) -> Vec<Rect> {
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    let a = action_disc(win_w, win_h, pad);
    let ar = f64::from(a.w) / 2.0;
    let (acx, acy) = {
        let (cx, cy) = action_centre(win_w, win_h, pad);
        (f64::from(cx), f64::from(cy))
    };
    // The ring must also clear the secondary-weapon button, which sits between the
    // disc and the ring on the disc's centre row.
    let clear = acx - f64::from(disc_side_button(win_w, win_h).x);
    // ★★ The pitch is `arc / (n - 1)` as it always was, but CAPPED (2026-09-10, when
    // plates became hideable). Both halves are measured, not chosen:
    //
    //  * Uncapped, a short ring re-spreads over the WHOLE arc, and with one plate
    //    visible that puts it at `RING_A1` — the arc's far end, up and LEFT of the
    //    action disc, as far from the thumb as the ring can reach. It also drags CLR
    //    out from between that plate and OK (the landscape selftest caught exactly
    //    that, at 2340x1080 with one plate left).
    //  * Pinned to the full ring's pitch instead, the survivors never grow at all: the
    //    pitch is what sets the radius (`need` below is inversely proportional to
    //    `sin(step/2)`), so keeping it tight keeps the ring far out and the plate small.
    //    Measured at 2340x1080: six seats came out 86 px, the same as eight.
    //
    // The cap is the pitch of the SIX-SEAT ring — the five-plate arrangement that
    // shipped from 2026-07-30 until 79차 (2026-09-04) — so hiding down to five plates
    // reproduces that layout exactly (98 px at 1080, the number 79차-E measured), and
    // hiding further keeps that pitch and simply ends the arc earlier. ⛔ Do not raise
    // it: every degree past this is a plate further from the thumb than any shipped
    // layout has ever put one.
    let step = if n > 1 {
        ((RING_A1 - RING_A0) / (n - 1) as f64).min(RING_STEP_MAX)
    } else {
        0.0
    };
    // Lay the ring out at `side`; `None` = some seat would leave the window.
    let try_side = |side: f64| -> Option<Vec<Rect>> {
        let need = if step > 0.0 {
            side * 1.485 / (2.0 * (step.to_radians() / 2.0).sin())
        } else {
            0.0
        };
        let radius = need.max(ar + side * 0.75).max(clear + side / 2.0 + f64::from(pad * 2));
        let mut seats = Vec::with_capacity(n);
        for i in 0..n {
            let ang = (RING_A0 + step * i as f64).to_radians();
            let cx = acx + radius * ang.cos();
            let cy = acy - radius * ang.sin();
            let (x0, y0) = (cx - side / 2.0, cy - side / 2.0);
            if x0 < 0.0 || y0 < 0.0 || x0 + side > f64::from(win_w) || y0 + side > f64::from(win_h) {
                return None;
            }
            seats.push(Rect::new(x0 as u32, y0 as u32, side as u32, side as u32));
        }
        Some(seats)
    };
    // ★★ The LARGEST plate that fits, not the first 10 % step that does (2026-09-04,
    // player: *"퀵슬롯 버튼 크기를 전부 조금씩만 키워줘"*). Going from 5 plates to 7 made the
    // ring crowded enough to need a shrink, and the old `side *= 0.9` loop then
    // overshot: on the S25 (2340x1080) the largest side that fits is **87**, and the
    // loop handed back **79** — the missing 9 % was quantisation, not geometry.
    // The base stays the ceiling, so a ring that already fits is UNCHANGED (six seats
    // still measure 98 px there).
    let base = f64::from((win_h / 11).clamp(40, 120));
    if let Some(seats) = try_side(base) {
        return seats;
    }
    // Bisect: `lo` always fits (or is degenerate), `hi` never does. 14 rounds put the
    // answer inside a pixel for any window this runs on.
    let (mut lo, mut hi) = (0.0_f64, base);
    let mut best: Option<Vec<Rect>> = None;
    for _ in 0..14 {
        let mid = (lo + hi) / 2.0;
        match try_side(mid) {
            Some(seats) => {
                best = Some(seats);
                lo = mid;
            }
            None => hi = mid,
        }
    }
    if let Some(seats) = best {
        return seats;
    }
    // ★ Never return an EMPTY ring. The old code fell out of the loop with `out`
    // cleared, and every consumer took that literally: `cancel_key` degraded to a
    // 1x1 CLR at pixel (0,0) and the whole quickbar disappeared — with the game's
    // own bottom HUD patched off, that leaves a field with no controls at all.
    // Clamp the last attempt into the window instead.
    out.clear();
    let side = lo.max(16.0);
    for i in 0..n {
        let ang = (RING_A0 + step * i as f64).to_radians();
        let radius = (ar + side * 0.75).max(clear + side / 2.0 + f64::from(pad * 2));
        let cx = (acx + radius * ang.cos()).clamp(side / 2.0, f64::from(win_w) - side / 2.0);
        let cy = (acy - radius * ang.sin()).clamp(side / 2.0, f64::from(win_h) - side / 2.0);
        out.push(Rect::new((cx - side / 2.0) as u32, (cy - side / 2.0) as u32, side as u32, side as u32));
    }
    out
}

/// BUILT-IN rect of the CANCEL disc — between the first quickbar plate and the OK disc,
/// hugging the right edge, derived from the clearance it needs rather than picked.
///
/// ★★ Ported from hero3 with the arc (2026-09-10) because the arc brings hero3's bug
/// with it. CLR is sized like the disc-side button while sitting on a PLATE-sized ring
/// seat; seat 0 is the arc's rightmost, so a disc that much bigger runs off the right
/// edge, `placement::builtin`'s own clamp shoves it back LEFT, and it lands on plate 1.
/// ⛔ Do NOT "fix" that by rotating it further along the arc: the arc runs UP from seat
/// 0, i.e. toward plate 1. The disc has to come off the ring and go DOWN, toward OK.
///
/// The rule: put the centre as far right as the window allows, then take it down from
/// plate 1 by exactly the distance that makes the two circles clear by `pad * 2`. One
/// number, and the position follows from the sizes — so a bigger CLR, a bigger plate or
/// another seat all move it on their own.
pub(crate) fn cancel_seat(win_w: u32, win_h: u32, pad: u32) -> Rect {
    let d = disc_side_button(win_w, win_h)
        .w
        .max(ring_seat(win_w, win_h, pad, 0).w)
        .min(win_w.max(1))
        .min(win_h.max(1));
    let r = f64::from(d) / 2.0;
    let q = ring_seat(win_w, win_h, pad, 1);
    let (qcx, qcy) = (f64::from(q.x) + f64::from(q.w) / 2.0, f64::from(q.y) + f64::from(q.h) / 2.0);
    // Circle-to-circle, because both are drawn as discs: a rect test would call them
    // clear while the strokes still touch.
    let need = r + f64::from(q.w.min(q.h)) / 2.0 + f64::from(pad * 2);
    let cx = (f64::from(win_w) - f64::from(pad * 2) - r).max(r);
    let dx = (cx - qcx).abs();
    // BELOW the plate — toward OK. `dx` already buys part of the distance.
    let dy = (need * need - dx * dx).max(0.0).sqrt();
    let cy = (qcy + dy).clamp(r, f64::from(win_h) - r);
    Rect::new((cx - r).max(0.0) as u32, (cy - r).max(0.0) as u32, d, d)
}

/// One LIVE ring seat: 0 = CLR, then one per VISIBLE plate in plate order (see
/// [`ring_seats_live`]). The single accessor [`crate::placement`] uses for its
/// built-ins — through [`plate_seat`] for the plates themselves, since a seat index is
/// no longer the same thing as a plate number.
pub(crate) fn ring_seat(win_w: u32, win_h: u32, pad: u32, i: usize) -> Rect {
    seat_or_corner(ring_all(win_w, win_h, pad), win_w, win_h, pad, i)
}

/// Seat `i` of `ring`, or the bottom-right corner fallback. Shared by the live and the
/// full ring so the out-of-range answer cannot differ between them.
fn seat_or_corner(ring: Vec<Rect>, win_w: u32, win_h: u32, pad: u32, i: usize) -> Rect {
    ring.get(i).copied().unwrap_or_else(|| {
        let s = (win_h / 11).clamp(40, 120);
        Rect::new(win_w.saturating_sub(s + pad), win_h.saturating_sub(s + pad), s, s)
    })
}

/// [`ring_seat`] on the full ring — see [`ring_all_full`].
pub(crate) fn ring_seat_full(win_w: u32, win_h: u32, pad: u32, i: usize) -> Rect {
    seat_or_corner(ring_all_full(win_w, win_h, pad), win_w, win_h, pad, i)
}

/// BUILT-IN rect of one quickbar PLATE, hidden set applied (hero3 82차).
///
/// A visible plate takes its place in the live ring; a HIDDEN one is answered with its
/// seat on the full ring. That fallback is never drawn or hit-tested — `quick_buttons`
/// zeroes hidden plates and `placement::shown` keeps them out of the editor — but it
/// has to be a real rect all the same: `placement::accepts` rejects a zero-sized
/// candidate, `Place::from_rect` divides a stored placement by it, and the resolver
/// wants one built-in per id in `ids_for`. Deterministic and off the live ring is what
/// makes it harmless.
pub(crate) fn plate_seat(win_w: u32, win_h: u32, pad: u32, plate: u8) -> Rect {
    match crate::quickpage::visible_index(crate::quick_hidden(), plate) {
        Some(seat) => ring_seat(win_w, win_h, pad, seat + 1),
        None => ring_seat_full(win_w, win_h, pad, usize::from(plate) + 1),
    }
}

/// BUILT-IN seat of the PAGE-FLIP button: one plate-pitch LEFT of the leftmost
/// quickbar plate, level with it — "보조공격 버튼 좌측 스킬버튼의 좌측" (player,
/// 2026-08-04).
///
/// ★ This replaces `ring_seat(.., quickpage::SLOTS + 1)`, which asked for a seat that
/// DOES NOT EXIST: the ring has `RING_SEATS` = 6 seats (0 = CLR, 1..=5 = the five
/// plates), so seat 6 fell straight into `ring_seat`'s bottom-right-corner fallback.
/// That is the whole of the "페이지 버튼이 구석에 박혀 있다" report — not a layout
/// preference, an out-of-range index quietly answered with a fallback rect.
/// ★ 2026-08-06 (player: "페이지 아이콘이 너무 크고 지금 위치보다 더 내려가야해. 더 아래
/// 스킬버튼 좌측에 존재해야해"): SMALLER than a plate, and dropped half a plate so it
/// reads as *below*-left of the lowest plate rather than level with it. The anchor is
/// now the LOWEST plate explicitly (`max y`) instead of the leftmost (`min x`) — on the
/// current arc those are the same seat, but saying which one we mean is what keeps this
/// correct if the arc is ever re-spaced.
pub(crate) fn quickpage_seat(win_w: u32, win_h: u32, pad: u32) -> Rect {
    below_left_of_lowest_plate(win_w, win_h, pad)
}

/// How far the PAGE button's icon spans across its disc, as a multiple of the disc
/// radius. The shared default in [`draw_mask_button`] is 1.5 (= 75% of the button).
///
/// ★ 2026-08-06, and it is a correction of a misreading: "페이지 아이콘이 너무 크고"
/// was read as "the BUTTON is too big" and the button was shrunk to 70%. The player
/// meant the icon INSIDE it — "버튼크기는 지금이랑 동일하게 해도 돼". So the button is
/// plate-sized again like every other control, and only the glyph shrinks.
pub const QUICKPAGE_ICON_SPAN: f64 = 1.05;

/// The game's own `#SKIP` lettering in a script box, in LOGICAL pixels: measured on the
/// headless intro (2026-09-18, 176x220 framebuffer) at x 146..175, y 213..219 — a 30x7
/// glyph run flush with the bottom-right corner, 1 px in. The box is BOTTOM-anchored,
/// so on the app's 246-row framebuffer it sits in rows 239..245, which the state-4
/// source window (`App::game_source_window`, rows 26..246) shows as the last rows of
/// the picture — i.e. still the bottom-right corner of `game`.
const SKIP_HINT_W: f64 = 30.0;
const SKIP_HINT_H: f64 = 7.0;
const SKIP_HINT_MARGIN: f64 = 1.0;
/// How much further out than the ink the target reaches, in logical px: a thumb aiming
/// at a 7 px-tall word needs more than the word.
const SKIP_HINT_REACH: f64 = 6.0;

/// The invisible touch target over the game's own `#SKIP` lettering (hero3 63차/64차):
/// from a little above-left of the ink's corner OUT to the window's true bottom-right
/// corner, so "the target contains the ink" holds by construction and the 1 px margin
/// — where a thumb aiming at a corner lands — is inside it too.
pub(crate) fn game_skip_hint(game: Rect) -> Rect {
    let (lw, lh) = crate::logical();
    game_skip_hint_at(game, lw, lh)
}

/// [`game_skip_hint`] with the logical size passed in — the pure half.
pub(crate) fn game_skip_hint_at(game: Rect, lw: u32, lh: u32) -> Rect {
    if game.w == 0 || game.h == 0 || lw == 0 || lh == 0 {
        return Rect::new(0, 0, 0, 0);
    }
    let sx = f64::from(game.w) / f64::from(lw);
    let sy = f64::from(game.h) / f64::from(lh);
    let x = f64::from(game.x) + (f64::from(lw) - SKIP_HINT_MARGIN - SKIP_HINT_W - SKIP_HINT_REACH) * sx;
    let y = f64::from(game.y) + (f64::from(lh) - SKIP_HINT_MARGIN - SKIP_HINT_H - SKIP_HINT_REACH) * sy;
    let (x, y) = (x.max(0.0) as u32, y.max(0.0) as u32);
    Rect::new(x, y, (game.x + game.w).saturating_sub(x), (game.y + game.h).saturating_sub(y))
}

/// BUILT-IN seat of the QUICK-SAVE button: the next step along the same row as the
/// page button.
///
/// It is a placeable control rather than a settings row because the settings row was
/// UNREACHABLE BY CONSTRUCTION (2026-08-04): the gear that opens the panel exists
/// only OFF the field, and `items::save_gate` — the game's own "may I save here" byte
/// — passes only ON the field. Every press could therefore only ever answer
/// `CANNOT SAVE HERE`. The panel row stays for the layouts that have no placeable
/// controls at all; this is the one a player on the field can actually hit.
/// ★ 2026-08-06 (player: "페이지 버튼 좌측 save버튼은 우측 상단 몬스터 이름 및 HP표시하는
/// 박스의 좌측으로 옮겨줘"): it leaves the plate row entirely and goes to the TOP-RIGHT,
/// immediately left of the box the GAME draws for the targeted monster.
///
/// The anchor is [`game_target_box_at`] — where that box will ACTUALLY be drawn, after
/// the host has poked its x to dodge the minimap panel — so this tracks the dodge
/// instead of assuming the box is still right-anchored. The RESOLVED minimap rect is
/// avoided deliberately: this function is itself a built-in seat (the value a placement
/// falls back to), and asking the placement resolver here would make one built-in
/// depend on the resolver that is asking for it.
///
/// ★ 2026-09-25: but the dodge is measured against the minimap the player STORED (seat
/// and size, read straight from the table — no resolver), not the built-in one. hero2
/// draws its target panel again, and on the S9's own layout (`field map 0.9401 0.1211
/// 130` at 2220x1080) the panel sits ~70 px further left than the built-in minimap
/// implies, so a SAVE seated from the built-in covered the panel's `LV`. A stored rect the
/// resolver later refuses only puts SAVE further left than it needed to be.
///
/// ★★ 2026-10-01 — a TAB hanging from the window's top edge, no longer a disc (player:
/// *"세이브 버튼과 map버튼을 hero3처럼 상단에 붙은 버튼형태로 변경해줘. 클릭영역은 실제버튼보다
/// 더 커야해"*). The form is hero3's (vc422, measured on the S25 at 2340×1080: two 86×51
/// tabs at y 0, 43 px apart, MAP left of SAVE, SAVE ~37 px left of the game's target box).
/// See [`top_tab_size`] for the size, [`minimap_toggle_seat`] for the MAP tab, and
/// [`top_tab_hit`] for the touch target, which is bigger than what is drawn.
pub(crate) fn quicksave_seat(win_w: u32, win_h: u32, pad: u32) -> Rect {
    let (w, h) = top_tab_size(win_w, win_h, pad);
    let game = Rect::new(0, 0, win_w, win_h);
    let map = minimap_panel_builtin(win_w, win_h);
    let stored_map = crate::placement::get(crate::placement::CtrlId::Minimap).map_or(map, |p| p.to_rect(map, win_w, win_h));
    let target = game_target_box_at(game, stored_map);
    // Left of whichever of the two top-right occupants reaches FURTHER left. The box
    // alone is not enough: it only dodges the panel in landscape immersive
    // (`target_box_shift_for` returns 0 otherwise), so in portrait the panel juts out
    // past it and a box-relative seat lands exactly on the panel's edge — caught by the
    // selftest at 1080x2340 before it could reach a screen.
    let block_x = target.x.min(map.x);
    // `pad * 4` = hero3's gap (37 px at 2340×1080, pad 9). Wider than the old disc's
    // `pad * 2` on purpose: the touch target reaches a quarter tab past the face
    // ([`top_tab_hit`]), and it must not reach the box. `saturating_sub` then a clamp: on
    // a window too narrow for anything to fit to the left, hugging the left edge is still
    // a reachable button.
    let x = block_x.saturating_sub(w + pad * 4).min(win_w.saturating_sub(w));
    Rect::new(x, 0, w, h)
}

/// Width and height of a TOP TAB (SAVE, MAP): a quickbar plate wide, three fifths of that
/// tall — hero3's 86×51 at 2340×1080.
///
/// ★ The FULL ring's plate, like the old minimap toggle: a player who switches plates off
/// has not asked for two unrelated top buttons to change size. And the RING's actual plate
/// rather than the `win_h / 11` formula it starts from — `ring_slots` shrinks itself until
/// the arc fits, so on a narrow window the formula is bigger than any real plate.
pub(crate) fn top_tab_size(win_w: u32, win_h: u32, pad: u32) -> (u32, u32) {
    let w = ring_seat_full(win_w, win_h, pad, 1).w.max(1);
    (w, (w * 3 / 5).max(1))
}

/// BUILT-IN seat of the MINIMAP TOGGLE — the `MAP` tab: half a tab left of the SAVE tab,
/// same size, on the same top edge (hero3's order, MAP then SAVE).
///
/// ★ Always on screen with the minimap UI since 2026-10-01 — it used to stand in for the
/// panel at the panel's own corner, drawn only while the panel was hidden; now it is the
/// one switch, both ways (`app.rs`).
pub(crate) fn minimap_toggle_seat(win_w: u32, win_h: u32, pad: u32) -> Rect {
    let save = quicksave_seat(win_w, win_h, pad);
    Rect::new(save.x.saturating_sub(save.w + save.w / 2), save.y, save.w, save.h)
}

/// The TOUCH target of a top tab drawn at `r`: a square one and a half tabs wide, hanging
/// from the same edge, centred on the tab — a quarter of a tab wider on each side, so two
/// tabs half a tab apart meet exactly in the middle of the gap and neither steals the
/// other's touches, and 1½ tabs deep. The player asked for a click area bigger than the
/// button (2026-10-01): the face is 86×51 on the S25 (about 5×3 mm), the target 129×129 —
/// close to Android's 48 dp minimum touch size (135 px there). The top needs nothing, the
/// window's edge is already there.
pub fn top_tab_hit(r: Rect, win_w: u32, win_h: u32) -> Rect {
    if r.w == 0 || r.h == 0 {
        return r;
    }
    let side = r.w / 4;
    let x0 = r.x.saturating_sub(side);
    let x1 = (r.x + r.w + side).min(win_w).max(x0);
    let y1 = (r.y + r.h.max(r.w * 3 / 2)).min(win_h).max(r.y);
    Rect::new(x0, r.y, x1 - x0, y1 - r.y)
}

/// One small control tucked BELOW-LEFT of the lowest quickbar plate.
///
/// The STICK valve is kept from the row layout this replaces, and for the same measured
/// reason: `potion_button` was caught by the selftest at 1080x2340 doing exactly this —
/// "a row extending left along the bottom edge runs into the stick's home on a narrow
/// screen" — and a control under the walking thumb steals steering. Built-ins are the
/// rect a rejected placement falls BACK to, so an overlapping built-in has no way out.
/// It turns the other way here: with no room to the left the button goes straight DOWN
/// from the plate, which is the direction the player asked for in the first place.
fn below_left_of_lowest_plate(win_w: u32, win_h: u32, pad: u32) -> Rect {
    let ring = ring_all(win_w, win_h, pad);
    let base = ring
        .get(1..)
        .unwrap_or_default()
        .iter()
        .copied()
        // ★ The LIVE seat count, not `RING_SEATS`: with plates switched off the ring is
        // shorter, and asking for a seat past its end is exactly the out-of-range index
        // quietly answered with the corner fallback that this function exists to fix.
        .max_by_key(|r| r.y)
        .unwrap_or_else(|| ring_seat(win_w, win_h, pad, ring_seats_live().saturating_sub(1)));
    // Plate-sized: the player asked for the ICON inside it to shrink, not the button
    // (see `QUICKPAGE_ICON_SPAN`).
    let side = base.w;
    let seat = stick_seat(win_w, win_h);
    let stick_right = seat.x + seat.w + pad * 2;
    // Half a plate lower than the plate's own top = clearly "under" it, not "level".
    let y = (base.y + base.h / 2).min(win_h.saturating_sub(side));
    let x = base.x.saturating_sub(side + pad * 2);
    if x >= stick_right {
        Rect::new(x, y, side, side)
    } else {
        Rect::new(base.x, (base.y + base.h + pad).min(win_h.saturating_sub(side)), side, side)
    }
}

/// BUILT-IN bounding square of the ONE walk control, [`crate::placement::CtrlId::Stick`]
/// — bottom-left, inset `pad * 4` from both edges.
///
/// ★★★ 91차 (2026-09-13) — the stick and the four-arrow D-pad are ONE control. Player:
/// "인게임 조이스틱이랑 메뉴열렸을때 화살표를 그냥 stick이라는 하나의 옵션으로 관리하고
/// 싶어". Until then `CtrlId::Dpad` was a second control, and since the layout editor
/// shows the FIELD set filtered by the live touch type, a TYPE A player saw only the
/// stick disc and had NO way to resize the menu arrows once 83차 removed the MENU page
/// ("현재 설정창에서 인게임 메뉴가 열렸을떄 화살표 크기는 조절이 불가능한데"). Now this
/// square is what every role is cut from:
///
///  * the MENU arrows (and a dialogue CHOICE's arrows) — `dpad_arms` tiles it,
///  * TOUCH TYPE B's field arrows — the same tiling,
///  * TOUCH TYPE A's stick home — a disc at the square's CENTRE ([`joy_home`]).
///
/// ★ SIZE: `2 x` the old three-cell key column (`3 * cell + 2 * pad`), up from `7/4`
/// — "누르기가 좀 어렵다는 말이 있어서 버튼 크기를 좀 늘리는게 나을것 같아". At 1080 tall
/// that is 546 -> 624 px. The left inset went from `win_w / 26` (85 px) to `pad * 4`
/// (36 px, the same as the bottom inset), which is what keeps the grown square clear
/// of the game's own menu panel: on an 18:9 window the panel's left edge is at 684 px
/// and the old inset would have put the square's right band under it.
///
/// ⛔ The history behind the numbers: 2026-08-09 5/4 ("버튼 크기도 조금더 크게해줘"),
/// 2026-08-11 11/8 ("default 크기를 조금만 더 키워줘"), 2026-08-17 7/4 ("높이랑 밑변
/// 둘다 좀 더 늘려줘"), 91차 2. The square is the only free variable — `dpad_arms`
/// explains why the arm RATIOS cannot give height and base at once.
///
/// ★★ 80차-F: BOTTOM-ANCHORED. 79차-B removed the `1 3 7 9 #` key block that used to
/// sit above the arrows and let the square "centre on its own", which walked it into
/// the vertical middle of the screen; the player asked for the seat they had
/// (2026-09-05: "좌측에 화살표 나오는것도 원래 좌측하단쪽에 위치했엇는데 … 갑자기 좌측
/// 중앙정렬로 바뀌었어"). ⛔ Do not give the menu arrows an anchor of their own again —
/// two anchors means the arrows jump when a window opens. Since 91차 there is not even
/// a second function to give one to.
///
/// Clamped into the window for short screens; the player moves and resizes it in the
/// layout editor like any other control (`placement::Table` wins over this).
pub(crate) fn stick_seat(win_w: u32, win_h: u32) -> Rect {
    let pad = pad_of(win_w, win_h);
    let cell = (win_h / 11).clamp(40, 120);
    let span = (cell * 3 + pad * 2) * 2;
    let span = span.min(win_h.saturating_sub(pad * 4).max(1)).min(win_w.max(1));
    let x = (pad * 4).min(win_w.saturating_sub(span));
    let y = win_h.saturating_sub(span + pad * 4);
    Rect::new(x, y, span, span)
}

/// One arrow of the D-pad: where the hexagon is INKED and where a touch counts.
pub(crate) struct Arm {
    /// The hexagon's bounding box. Its vertex on the `dir` axis points OUTWARD.
    pub draw: Rect,
    /// The touch target — the arrow's diagonal QUARTER of the square. Always a
    /// superset of the hexagon, and disjoint from every other arm's.
    pub hit: Wedge,
    pub dir: Dir,
}

/// A quarter of the D-pad square cut by BOTH diagonals — one arrow's touch target
/// (91차-B). The four quarters tile the square exactly: a point is in the quarter
/// whose axis it is closest to, ties on a diagonal go to UP / DOWN, and a small
/// DEAD DISC at the centre belongs to nobody (a thumb resting in the middle must not
/// press anything).
///
/// ★ Why quarters and not the 2026-08-17 horizontal BANDS: a band is only as tall as
/// its share of the square, so the arrows had to be short to be wide, and the
/// player's "뚱뚱한 육각형 … 크기 더 늘려줘" needed both. A quarter is `s/2` deep AND
/// `s` wide at the rim, so a hexagon 0.44 wide and 0.34 tall fits in every one of
/// them at once — and sliding a thumb from one arrow to the next crosses a line, not
/// a dead strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Wedge {
    pub square: Rect,
    pub dir: Dir,
}

impl Wedge {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let sq = &self.square;
        if !sq.contains(x, y) {
            return false;
        }
        let cx = f64::from(sq.x) + f64::from(sq.w) / 2.0;
        let cy = f64::from(sq.y) + f64::from(sq.h) / 2.0;
        let (dx, dy) = (x - cx, y - cy);
        let dead = f64::from(sq.w.min(sq.h)) * f64::from(DEAD_PCT) / 100.0;
        if dx * dx + dy * dy < dead * dead {
            return false;
        }
        match self.dir {
            Dir::Up => -dy >= dx.abs(),
            Dir::Down => dy >= dx.abs(),
            Dir::Left => -dx > dy.abs(),
            Dir::Right => dx > dy.abs(),
        }
    }
}

/// Length of an arrow's hexagon ALONG its axis, as a percent of the square's side.
///
/// ★ 91차-B (2026-09-13), player: "화살표 크기 더 늘려줘야해. 정육각형이 될필요없어
/// 뚱뚱한 육각형인게 더 좋아." The three numbers below are what fits a diagonal
/// quarter: with the tip at `inset` from the rim and the flat sides ending `TIP` short
/// of the length, the far corners sit `inset + LEN - TIP` in from the rim, where the
/// quarter is still `2 * (50 - inset - LEN + TIP)` % wide — `0.484` of the side at
/// these values, so a `0.44` hexagon clears the diagonal by ~0.02 s on each corner.
pub(crate) const ARM_LEN_PCT: u32 = 34;
/// Width of the hexagon ACROSS its axis, as a percent of the side. Wider than it is
/// long — the "뚱뚱한" — and 1.57x the 91차 regular-ish hexagon's 0.28.
pub(crate) const ARM_WIDTH_PCT: u32 = 44;
/// Each tip's share of the hexagon's length. 30 % leaves the two flat sides 40 % of
/// the length; a regular hexagon's 25 % would give the fat shape a roof too shallow to
/// read as pointed.
pub(crate) const HEX_TIP_PCT: u32 = 30;
/// Radius of the dead disc at the square's centre, as a percent of the side. The
/// hexagons' inner tips sit at `50 - inset - LEN` = 14 % from the centre, so 8 %
/// leaves clear ground between the disc and every arrow.
pub(crate) const DEAD_PCT: u32 = 8;

/// The four arrows inside a D-pad bounding square, in draw / hit-test order: up, down,
/// left, right.
///
/// ★ ONE control, four buttons. The player asked for the arrows to be sized and moved
/// **together** ("화살표는 한개씩 크기조절 및 위치 조정하는게 아니라 조이스틱처럼 한번에
/// 4개의 화살표를 모두 조절하는 방식"), so `placement` knows only [`crate::placement::
/// CtrlId::Stick`] — a single square — and this derives the four targets from whatever
/// square that resolves to. Nothing else in the layout system, the editor or the
/// on-disk format has to know there are four of them.
///
/// ── GEOMETRY, 91차-B (2026-09-13) ───────────────────────────────────────────────
/// **INK** — each arm is a FAT HEXAGON: [`ARM_WIDTH_PCT`] of the side across its axis,
/// [`ARM_LEN_PCT`] along it, tips of [`HEX_TIP_PCT`] of the length at both ends (the
/// outer one at `inset` from the rim, the inner one pointing at the centre). All four
/// are congruent by rotation. At a 624 px square that is 275 x 212 px, against the
/// 176 x 188 of 91차's first cut and the 229 x 140 triangles before it — ~65 % more
/// ink than the first cut.
///
/// **HIT** — the square cut by its two diagonals into four [`Wedge`]s, plus a dead
/// disc at the centre. Every pixel of the square belongs to exactly one arrow (or to
/// the disc), the quarters share no pixel — the "겹치는건 안돼" requirement holds by
/// construction — and each hexagon lies inside its own quarter, which is what makes
/// the ink a subset of the target:
/// ```text
///   +---------------+
///   |\     UP      /|
///   | \    /\     / |   quarter = the axis a point is nearest to
///   |  \  |  |   /  |   ties on a diagonal -> UP / DOWN
///   | L  \|__|/  R  |   centre disc       -> nobody
///   |     /  \      |
///   |    /    \     |
///   |   /      \    |
///   |  /  DOWN  \   |
///   | /          \  |
///   +---------------+
/// ```
pub(crate) fn dpad_arms(r: Rect) -> [Arm; 4] {
    let s = r.w.min(r.h);
    let inset = (s / 50).max(1);
    let len = (s * ARM_LEN_PCT / 100).max(3);
    let wid = (s * ARM_WIDTH_PCT / 100).max(3);
    let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
    let hit = |dir| Wedge { square: r, dir };
    [
        Arm { draw: Rect::new(cx.saturating_sub(wid / 2), r.y + inset, wid, len), hit: hit(Dir::Up), dir: Dir::Up },
        Arm {
            draw: Rect::new(cx.saturating_sub(wid / 2), (r.y + r.h).saturating_sub(inset + len), wid, len),
            hit: hit(Dir::Down),
            dir: Dir::Down,
        },
        Arm { draw: Rect::new(r.x + inset, cy.saturating_sub(wid / 2), len, wid), hit: hit(Dir::Left), dir: Dir::Left },
        Arm {
            draw: Rect::new((r.x + r.w).saturating_sub(inset + len), cy.saturating_sub(wid / 2), len, wid),
            hit: hit(Dir::Right),
            dir: Dir::Right,
        },
    ]
}

/// The six corners of an arm's hexagon, clockwise from the OUTWARD tip, for a
/// bounding box `r` and a pointing axis `dir`. The two vertices on the axis are the
/// tips, each [`HEX_TIP_PCT`] of the box's length on that axis; the two flat sides
/// are what is left between them.
pub(crate) fn hexagon(r: Rect, dir: Dir) -> [(f64, f64); 6] {
    let (x0, y0) = (f64::from(r.x), f64::from(r.y));
    let (x1, y1) = (x0 + f64::from(r.w), y0 + f64::from(r.h));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let tip = f64::from(HEX_TIP_PCT) / 100.0;
    match dir {
        Dir::Up | Dir::Down => {
            let t = (y1 - y0) * tip;
            [(cx, y0), (x1, y0 + t), (x1, y1 - t), (cx, y1), (x0, y1 - t), (x0, y0 + t)]
        }
        Dir::Left | Dir::Right => {
            let t = (x1 - x0) * tip;
            [(x1, cy), (x1 - t, y1), (x0 + t, y1), (x0, cy), (x0 + t, y0), (x1 - t, y0)]
        }
    }
}
/// Ring position 0 (nearest the disc's right side) is the CANCEL key; 1..=5 are the
/// quickbar slots. Keeping them on ONE ring is what gives the reference look of the
/// action button being encircled by its skills.
fn ring_all(win_w: u32, win_h: u32, pad: u32) -> Vec<Rect> {
    ring_slots(win_w, win_h, ring_seats_live(), pad)
}

/// Seats the ring actually lays out right now: CLR plus one per VISIBLE plate.
///
/// ★★ hero3 82차 (player: "퀵슬롯이 너무많아서 불편하다는 유저도있어서"): a plate switched
/// off in the layout editor gives its SEAT back, so the survivors spread further along
/// the arc — up to [`RING_STEP_MAX`] — and grow: `ring_slots` takes the largest side
/// that fits, and a wider pitch pulls the ring in and lets that side be bigger. Two
/// ceilings bound it, so a ring that already fits is unchanged: the pitch cap and
/// `ring_slots`'s own `win_h / 11` base size.
///
/// Everything derived from the ring follows for free, which is the point of deriving
/// them: `cancel_seat` re-measures its clearance from the FIRST VISIBLE plate,
/// `quickpage_seat` from the lowest visible one. (The two top tabs, SAVE and MAP, size
/// themselves from the FULL ring instead — see `top_tab_size`.) ⛔ Do not go back to a
/// literal seat count here.
fn ring_seats_live() -> usize {
    1 + crate::quickpage::visible_count(crate::quick_hidden())
}

/// The ring as it would be with every plate switched on.
///
/// For the controls that are SIZED from a ring seat without being on the ring (the two
/// top tabs, SAVE and MAP): a player who switches two plates off has not asked for an
/// unrelated top button to change size. ⛔ Not for anything that sits ON the ring.
fn ring_all_full(win_w: u32, win_h: u32, pad: u32) -> Vec<Rect> {
    ring_slots(win_w, win_h, RING_SEATS, pad)
}

/// Seats on the FULL action ring: CLR + every quickbar plate.
/// 0 = CLR, 1..=7 = the quickbar plates. Was 6 (five plates) until 2026-09-10.
const RING_SEATS: usize = 8;
const _: () = assert!(RING_SEATS == 1 + crate::quickpage::PLATES);

/// The disc's SIDE-BUTTON geometry: immediately left of the action disc, level with it,
/// one gap apart — larger than a skill plate, smaller than the disc. The player asked
/// for the two to read as a pair (2026-07-30).
///
/// ★ This was `secondary_button`, the key-`7` (보조무기) button. hero2 has no secondary
/// weapon — not a key (`7` is skill slot 3, `t/help.tdf[8]`) and not even an equipment
/// slot (`t/shop.tdf` sells 무기 / 방어구 / 기타, and the off-hand is a 방패). The button
/// is gone (2026-09-01); only this GEOMETRY survives, as the size `placement` gives the
/// default CLR. (It was also the base of `disc_row_seat`, the key row the 회복 아이템 `0`
/// button sat on — that button was removed on 2026-09-25 and the row helper with it.) (hero3 의 호 배치가 이 버튼을 피해
/// 반지름을 키웠던 것과 달리, hero5식 세로 칼럼은 우측 가장자리에 붙으므로 서로 간섭하지
/// 않는다 — 다만 링 쪽에서 이 함수를 되부르지 않는 규칙은 재귀 방지를 위해 유지한다.)
pub fn disc_side_button(win_w: u32, win_h: u32) -> Rect {
    let pad = (win_w.min(win_h) / 110).max(3);
    let a = action_disc(win_w, win_h, pad);
    let d = (a.w * 3 / 5).max(56);
    Rect::new(
        a.x.saturating_sub(pad * 2 + d),
        (a.y + a.h / 2).saturating_sub(d / 2),
        d,
        d,
    )
}

/// BUILT-IN minimap geometry: the EXTREME top-right corner (player request
/// 2026-07-30 — "the minimap has to be at the very top-right, and the monster
/// name/HP/LV box to its left"). Grown twice on request (win_h/5 -> /4 -> this).
///
/// Pure function of the window — the player's own position and size are applied on
/// top by [`crate::placement`], never in here.
pub(crate) fn minimap_panel_builtin(win_w: u32, win_h: u32) -> Rect {
    let pad = pad_of(win_w, win_h);
    let side = (win_h * 26 / 100).clamp(120, 280);
    Rect::new(win_w.saturating_sub(pad * 2 + side), pad * 2, side, side)
}

/// The minimap panel as the player actually sees it. Tapping it presses `*`, which
/// is what makes the game (re)paint the offscreen minimap buffer we mirror here.
pub fn minimap_panel(win_w: u32, win_h: u32) -> Rect {
    crate::placement::rect_of(crate::placement::CtrlId::Minimap, Mode::Field, win_w, win_h)
        .unwrap_or_else(|| minimap_panel_builtin(win_w, win_h))
}

/// The round MINIMAP RESTORE button, drawn only while the panel is hidden: the panel's own
/// top-right corner, a ring key big — the folded-map icon that stood in for the panel until
/// 2026-10-01, back (player 2026-10-21: *"미니맵 터치하면 사라지는데 다시 나타나게할 방법이없어.
/// 사라진 미니맵 자리에 이전에 사용하던 미니맵 on/off 아이콘이 나타나게"*). Since that day the `MAP` tab
/// opens the world map, so a panel tapped away had no way back but the world map's corner box.
///
/// `panel` is the panel as placed ([`minimap_panel`]), so the button follows a panel the player
/// moved. The FULL ring's key, like the top tabs: switching a plate off must not resize it.
pub fn minimap_restore_button(panel: Rect, win_w: u32, win_h: u32) -> Rect {
    let k = ring_seat_full(win_w, win_h, pad_of(win_w, win_h), 0);
    let side = k.w.min(k.h).min(panel.w).min(panel.h).max(1);
    Rect::new(panel.x + panel.w.saturating_sub(side), panel.y, side, side)
}

/// The TOUCH target of the restore button drawn at `r`: half a button bigger on every side,
/// clamped to the window — the click area bigger than the face, as the top tabs have it
/// ([`top_tab_hit`]). It only ever reaches into the hidden panel's empty corner.
pub fn minimap_restore_hit(r: Rect, win_w: u32, win_h: u32) -> Rect {
    let grow = r.w / 4;
    let x0 = r.x.saturating_sub(grow);
    let y0 = r.y.saturating_sub(grow);
    let x1 = (r.x + r.w + grow).min(win_w).max(x0);
    let y1 = (r.y + r.h + grow).min(win_h).max(y0);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// How far LEFT (in GUEST px) the game's own target-monster box has to move so it
/// does not end up underneath our minimap panel.
///
/// ★ Rewritten 2026-07-31 for free placement. This used to invert the panel's
/// right-edge anchor unconditionally — correct only while the panel was nailed to
/// the top-right corner. Dragged to the left half of the screen the old formula
/// asked for ~476 guest px, clamped to the build's shift ceiling, and
/// parked the game's own monster box in the middle of the field, dodging something
/// that was no longer there. It is now an OVERLAP QUERY: the box moves only when
/// the panel actually covers it, and only by as much as it takes.
///
/// ★ hero2 (2026-09-25, "hero3처럼 미니맵 왼쪽에 몹이름이랑 피통이 나오도록"): the same
/// query, now that hero2's own panel is drawn again — its size and its one site are
/// the build's (`guest_build::target_box_guest_size` / `target_box_x_sites`).
pub fn target_box_shift(game: Rect) -> u32 {
    target_box_shift_for(game, minimap_panel(game.w, game.h))
}

fn target_box_shift_for(game: Rect, map: Rect) -> u32 {
    if game.w == 0 || !crate::immersive_active() || game.w <= game.h {
        return 0;
    }
    let base = game_target_box(game);
    if !base.intersects(&map) {
        return 0;
    }
    // Move it left until its right edge clears the panel's left edge, + a small gap.
    let need = (base.x + base.w).saturating_sub(map.x);
    let (lw, _) = crate::logical();
    // The binding guest site's imm8 is the ceiling: 255-44 on hero3 (`subs r1,#44`),
    // 255-66 on hero2 (`subs r7,#0x42`).
    (need * lw.max(1) / game.w + 2).min(crate::guest_build::target_box_shift_max())
}

/// The game's target-monster box where it will ACTUALLY be drawn, i.e. after the
/// host has poked its anchor to dodge `map`.
pub(crate) fn game_target_box_at(game: Rect, map: Rect) -> Rect {
    let base = game_target_box(game);
    let dx = gx(game, target_box_shift_for(game, map));
    Rect::new(base.x.saturating_sub(dx), base.y, base.w, base.h)
}

/// Can the game's box still get out of the way of a panel at `map`? False when the
/// dodge the panel demands no longer encodes, in which case the game's own monster
/// name and HP bar would be drawn underneath our panel — which is why the editor
/// refuses such a placement instead of letting the player discover it in a fight.
pub(crate) fn target_box_can_dodge(win_w: u32, win_h: u32, map: Rect) -> bool {
    let game = Rect::new(0, 0, win_w, win_h);
    if !crate::immersive_active() || win_w <= win_h {
        return true;
    }
    !game_target_box_at(game, map).intersects(&map)
}

/// The settings gear + the `123` keypad toggle, as a pure function of the window.
///
/// ★ 2026-07-30: TOP-CENTRE, nudged slightly RIGHT of the middle (player request).
/// The old spot was beside the gauge block on the left, which the player kept
/// meeting when they did not want it. `123` is a touch larger than the gear
/// ("아주 조금만 크게").
///
/// Extracted from `layout()` so [`crate::placement`] can treat both as no-go areas:
/// they are hit-tested BEFORE the button list, so a control parked on one is a
/// control that silently opens the settings instead of doing its job. NOTE these are
/// the MENU-mode positions — in the field both are hidden and `layout()` zeroes them.
pub(crate) fn gear_row(win_w: u32, win_h: u32) -> (Rect, Rect) {
    let pad = pad_of(win_w, win_h);
    let gs = (win_w.min(win_h) / 22).clamp(30, 64);
    let ts = (gs * 6 / 5).min(win_h / 8);
    let gap = pad * 2;
    let total = gs + gap + ts;
    let x0 = (win_w / 2 + win_w / 14).min(win_w.saturating_sub(total + pad)) - total / 2;
    let row_y = pad * 2;
    (
        Rect::new(x0, row_y + (ts.saturating_sub(gs)) / 2, gs, gs),
        Rect::new(x0 + gs + gap, row_y, ts, ts),
    )
}

/// The pull-out keypad panel: CENTRED, 3 columns x 5 rows of [`PAD_KEYS`].
///
/// ★ Moved off the left edge 2026-08-03. It used to be left + vertically centred,
/// which is where the 1/3/7/9/# key block lived until 2026-09-17 (the STICK seat now);
/// the pad is pushed into the button list FIRST (it takes touch priority over
/// everything it covers), so a nav key underneath it would look fine and send the
/// wrong key. The pad is a transient thing you pull out for one keypress and it
/// already covers the game while it is open, so the middle costs nothing.
pub fn pad_panel(win_w: u32, win_h: u32) -> Rect {
    let cell = (win_h / 7).clamp(40, 112);
    let (w, h) = ((cell * 3).min(win_w), (cell * 5).min(win_h));
    Rect::new((win_w.saturating_sub(w)) / 2, (win_h.saturating_sub(h)) / 2, w, h)
}

fn build_pad(out: &mut Vec<Button>, panel: Rect, pad: u32) {
    let cw = panel.w / 3;
    let ch = panel.h / 5;
    for (r, row) in PAD_KEYS.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let Some((label, key)) = cell else { continue };
            out.push(Button {
                rect: Rect::new(
                    panel.x + c as u32 * cw + pad,
                    panel.y + r as u32 * ch + pad,
                    cw.saturating_sub(2 * pad),
                    ch.saturating_sub(2 * pad),
                ),
                key: *key,
                label: Label::Text(label),
                dpad: false,
                face: Face::Round,
                wedge: None,
            });
        }
    }
}

fn layout_immersive(win_w: u32, win_h: u32, pad: u32, mode: Mode, keypad_open: bool, choice: bool) -> (Rect, Vec<Button>, Option<Rect>) {
    let game = Rect::new(0, 0, win_w, win_h);
    let mut buttons = Vec::new();
    // The pull-out keypad takes touch PRIORITY over everything it covers, so it is
    // pushed first: `hit` returns the first rect containing the point.
    // The pull-out keypad has no toggle since 2026-09-18 (see `layout`), so `keypad_open`
    // can never be true here; the branch stays so the pad code has a reader.
    if keypad_open && mode == Mode::Menu {
        build_pad(&mut buttons, pad_panel(win_w, win_h), pad);
    }
    // ★ Every rect below comes from the placement resolver, which is the built-in
    // geometry unless the player has moved or resized that control in the layout
    // editor — and which guarantees the whole set stays disjoint and on screen.
    let placed = crate::placement::resolve(mode, win_w, win_h);
    let at = |id: crate::placement::CtrlId| {
        placed.iter().find(|(c, _)| *c == id).map(|(_, r)| *r).unwrap_or_else(|| Rect::new(0, 0, 0, 0))
    };
    // Big ACTION disc, bottom-right. NOT in a cutscene (player request 2026-08-16:
    // "컷신 일때는 … 확인버튼이 노출되면 안돼" — the script window gets the `#` skip
    // alone; 2026-08-03's "OK and #" pair is superseded).
    // ★ NEVER in the script window, choice or not (2026-08-17, player: "선택지가 있을떄
    // 화살표있고, OK버튼은 없고"). Confirming is what the whole screen already does
    // (`tap_anywhere_key`), so a disc would be a second copy of a control that is
    // already the size of the window.
    if mode != Mode::Cutscene {
        buttons.push(Button {
            rect: at(crate::placement::CtrlId::Ok),
            key: KeyCode::OK,
            label: Label::Text(""),
            dpad: false,
            face: Face::Action,
            wedge: None,
        });
    }
    // CANCEL beside the first skill plate. The script window reuses this seat for `#`
    // (below), so it must not also get a CLR.
    //
    // ★ A "취소" button for a dialogue CHOICE was tried on 2026-08-17 and REVERTED the
    //   same day, by the player's own reasoning: the guest has no cancel to call. The
    //   game's own choice widget advertises ONE key — it draws an `OK` bar under the
    //   rows and nothing else — and declining is a ROW in the list ("지금은 별로"), not
    //   a key. Faking it host-side would mean writing the cursor index
    //   (`CHAR+0xa284`) to "the row that means no" and synthesising OK, and NOTHING
    //   tells us which row that is: it is last in the 의뢰 list, but a list may have no
    //   decline row at all, and guessing wrong ACCEPTS a quest the player refused.
    //   A button that cannot do what its label says is worse than no button.
    if mode != Mode::Cutscene {
        buttons.push(Button {
            rect: at(crate::placement::CtrlId::Cancel),
            key: KeyCode::CLEAR,
            label: Label::Text("CLR"),
            dpad: false,
            face: Face::Round,
            wedge: None,
        });
    }
    // Menu mode: the navigation keys 1/3/7/9/# down the LEFT edge, while the
    // quickbar plates stay in their ring seats on the right (drawn separately, from
    // `quick_buttons`). Both sets at once is the 2026-08-03 request; before that the
    // ring seats were shared, so opening a window took the plates away.
    //
    // ★ What those right-hand plates DO changes with the mode and cannot not: they
    // send the quickslot hotkeys 1/3/7/9, and while one of the game's own windows
    // is up it is the WINDOW's key handler that receives them, not the field
    // dispatcher — so they navigate the window instead of casting. That is the
    // game's own key routing; the buttons are still the fastest way to send those
    // digits with the right thumb.
    // ★★ The permanent 1/3/7/9/# block that stood here until 2026-09-17 is GONE (hero3
    // 79차-B's logic, applied when the STICK merge below made its column the stick's
    // seat): every hero2 window is a tap target now (`hero2java::h2_tap` — rows, tab
    // headers, grids, popups), and the pull-out keypad still carries every digit, so
    // five permanent keys were a second way to do what a tap already does. Stored
    // `k0`..`k4` layout lines simply stop parsing, which drops them.
    //
    // ★ 2026-08-09: the four-arrow D-pad. A FIXED control —
    // unlike the floating stick, it is exactly where it is drawn — for the one
    // thing the stick is bad at: stepping a menu cursor one row at a time. The
    // stick latches a direction and auto-repeats it, which is right for walking
    // and wrong for choosing.
    //
    // ★ On the FIELD too under TOUCH TYPE B since 2026-08-16 (player: "화살표만
    // 나와야해" — the pinned stick of the first TYPE B round is gone). Same control,
    // same bottom-left seat; holding an arm walks (the key stays down for the life of
    // the touch, and sliding between arms turns — `app.rs` slide_touches), which is
    // exactly what the pinned stick did.
    //
    // ★★★ hero3 91차 (ported 2026-09-17): the square they are cut from is
    // `CtrlId::Stick`'s — the ONE walk control. `CtrlId::Dpad` is gone; the stick disc
    // (type A, field) and these arrows (menu / choice / type B field) share a seat and
    // a size knob. See `stick_seat`.
    //
    // Pushed BEFORE the joystick zone
    // is consulted at all (`app.rs` hit-tests `layout.buttons` first), which is
    // what stops an arm tap from being read as a walk. NEVER in a cutscene: the
    // script window takes the `#` skip alone (2026-08-16).
    //
    // `dpad: true` gives them the same tint as the classic layout's D-pad: these
    // are the same four keys, so they should not read as a different kind of
    // control.
    // ★ 2026-08-17, in TWO steps, and the second one is the rule:
    //
    //   (a) 54차 removed the arrows from the script window entirely, and that made the
    //       game's own dialogue CHOICES unanswerable — the player could not pick 예/
    //       아니오 on a quest ("선택지를 못 골랐다"). Window id 4 is BOTH story narration
    //       and NPC dialogue (`items::cutscene_open`'s own doc, and device-confirmed:
    //       the quest guide reads `win=4` exactly like the prologue).
    //   (b) So they are told apart by a different byte instead —
    //       `items::script_choice_open` (`CHAR+0xa0dc`, 1 only while a choice list is
    //       up) — which is what `choice` carries here. Player's rule, verbatim:
    //       "컷신일때는 안나오고 인게임 내에서 대화에 선택지가 있을때는 나오고".
    //
    // A cutscene with no choice therefore has NO arrows (the 54차 look is intact), and
    // the moment a choice appears they are there at full strength, on the MENU seat —
    // which is where `placement` puts a cutscene's controls anyway (Cutscene aliases
    // Menu), i.e. the same arrows the player already uses to walk a menu.
    //
    // ★ 59차: and NOT in 게임패드 HUD mode, anywhere — field, menu or choice (player,
    // 2026-08-19: "패드모드일때 화살표는 인게임과 메뉴 전부 없애도 될것같고"). The pad's
    // own hat/stick walks and steps the cursor through the same quantiser
    // (`gamepad::gamepad_axes` -> `joy_dir`), so on-screen arrows would be a second
    // copy of a control the player's thumb is already on. Under the default AUTO
    // preference a screen touch brings them straight back — see `crate::pad_hud_active`.
    if !crate::pad_hud_active()
        && (mode == Mode::Menu
            || (mode == Mode::Cutscene && choice)
            || (mode == Mode::Field && crate::stick_fixed()))
    {
        for arm in dpad_arms(at(crate::placement::CtrlId::Stick)) {
            buttons.push(Button {
                // ★ The QUARTER, not the hexagon: a touch anywhere in the arrow's
                // diagonal quarter of the square counts, corners included (2026-08-17's
                // "표시되는 영역보다 조금 더 큰범위" rule, on the 91차-B partition). The
                // ink rect rides along in the face; `rect` is the whole square for the
                // callers that want a bounding box, `wedge` is the real target.
                rect: arm.hit.square,
                wedge: Some(arm.hit),
                key: match arm.dir {
                    Dir::Up => KeyCode::UP,
                    Dir::Down => KeyCode::DOWN,
                    Dir::Left => KeyCode::LEFT,
                    Dir::Right => KeyCode::RIGHT,
                },
                // The BUTTON is the hexagon and it paints its own glyph. `Label` is
                // unused for this face and is only here because `Button` requires one.
                label: Label::Text(""),
                dpad: true,
                face: Face::Hexagon(arm.dir, arm.draw),
            });
        }
    }
    // Cutscene: the only control of ours is the hit target over the GAME'S OWN
    // `#SKIP` lettering in the bottom-right corner (hero3 63차/64차, player: "우측하단
    // skip 이라는 문구 클릭했을때 skip 되도록" … "따로 기존 skip버튼은 필요없을것 같아").
    // `Face::None` draws NOTHING: the game already painted the words, and anything over
    // them would hide the very thing being tapped. hero2 draws that lettering itself in
    // every script box (narration, dialogue, quest notice — `HERO2_GUEST_RE.md` §3.3),
    // and `#` is its skip key (`bb.a(II)V` sets the skip flag on `'#'` in every mode).
    //
    // ★ The translucent `#` disc that used to sit on the CANCEL seat (2026-08-16) went
    // with this on 2026-09-18, together with `Face::Skip`.
    //
    // ★ …and NOT while a pad is driving: `#` on the pad skips, so a scene overlay would
    // be a second copy of a control the player's thumb is already on.
    if mode == Mode::Cutscene && !crate::pad_hud_active() {
        buttons.push(Button {
            rect: game_skip_hint(game),
            key: KeyCode::HASH,
            label: Label::Text(""),
            dpad: false,
            face: Face::None,
            wedge: None,
        });
    }
    // Field-only: the minimap panel, which doubles as the `*` key (that is what repaints
    // it). Off-field it is not drawn, and leaving it hit-testable would fire its key on a
    // blind tap (a source of the "map opened by itself" report).
    //
    // ★ The key-`7` button was here too until 2026-09-01. hero2 has no secondary
    // weapon: `7` is SKILL SLOT 3, which quickbar plate 3 already sends, so the button
    // was a second control for a key that already had one. See `disc_side_button`.
    //
    // ★ …and the 회복 아이템 `0` button (long press = `#` 회복 아이템 교체) until 2026-09-25,
    // when the player asked for it to go ("OK버튼 옆에 0번버튼 제거해줘"). Both keys are
    // still reachable: a 포션 plate on the quickbar sends `0` (`quickpage::Entry::Item`),
    // the pad has `0` on L2 and `#` on R2, and the pull-out keypad carries both.
    if mode == Mode::Field {
        // ★ The minimap panel is deliberately NOT a Button. As one it fired STAR on
        // touch-DOWN and — worse — any touch that had not been adopted by the joystick
        // acquired it just by SLIDING over the corner, which is the player-reported
        // "the minimap keeps getting clicked by itself". app.rs gives it real
        // tap semantics (down and up inside, little movement) instead.
    }
    // LSK/RSK and 2/4/5/6/8/0/* live on the pull-out keypad (toggle beside the
    // gear) rather than permanently on screen.
    //
    // ★ The stick zone is the WHOLE WINDOW under TOUCH TYPE A (a touch that misses
    // every control starts a stick right there) — and NOTHING under TYPE B: the
    // second 2026-08-16 round removed the pinned stick outright ("조이스틱은 터치타입
    // B에서 히든처리 되어야해"); B walks with the D-pad arrows pushed above, which are
    // ordinary buttons and need no zone.
    //
    // ★ FIELD ONLY since 2026-08-17 (player: "메뉴 열렸을때 좌측하단에 조이스틱
    // placeholder 없애줘"). The zone carries the idle HOME HINT with it, so off-field
    // it drew a stick under the menu's own D-pad — two walking controls on a screen
    // where walking does not exist. A cutscene was already excluded; a menu is the
    // same argument, and the menu D-pad (pushed above, in EVERY touch type) is what
    // steps a cursor.
    //
    // ★ Dropping the ZONE and not just the drawing is deliberate: a zone that outlives
    // what is drawn is an invisible control, which is the rule `App::stats_tap_zone`
    // states — here it would swallow menu taps into a stick nobody can see.
    //
    // ★ And none in 게임패드 HUD mode (59차), for the reason the arrows go too: the
    // stick is on the pad. Dropping the ZONE and not just the drawing matters here as
    // much as it does for touch type B — a live zone with nothing drawn is an
    // invisible control, and it would eat the very tap that switches AUTO back to the
    // touch HUD.
    let zone = if crate::stick_fixed() || crate::pad_hud_active() || mode != Mode::Field {
        None
    } else if crate::stick_pinned() {
        // ★ fixed_A (2026-09-24): the base never leaves its seat, so the zone is the seat
        // itself, a quarter larger each way so a thumb that lands on the rim still takes
        // it — a whole-window zone would let a tap anywhere steer from a centre the
        // finger is nowhere near.
        let seat = crate::placement::rect_of(crate::placement::CtrlId::Stick, Mode::Field, win_w, win_h).unwrap_or_else(|| stick_seat(win_w, win_h));
        let (gx, gy) = (seat.w / 4, seat.h / 4);
        let x = seat.x.saturating_sub(gx);
        let y = seat.y.saturating_sub(gy);
        Some(Rect::new(x, y, (seat.w + 2 * gx).min(win_w - x), (seat.h + 2 * gy).min(win_h - y)))
    } else {
        Some(Rect::new(0, 0, win_w, win_h))
    };
    (game, buttons, zone)
}

pub fn layout(win_w: u32, win_h: u32, joystick: bool, mode: Mode, keypad_open: bool, choice: bool) -> Layout {
    let landscape = win_w > win_h;
    let pad = (win_w.min(win_h) / 110).max(3);
    let mut buttons = Vec::new();
    let game;
    let mut joy_zone = None;

    // Landscape + immersive: the whole window is the game, controls float on top.
    if landscape && crate::immersive_active() {
        let (g, b, jz) = layout_immersive(win_w, win_h, pad, mode, keypad_open, choice);
        // Immersive: the gear + keypad toggle are SMALL and sit immediately right of
        // the player-gauge block, centred on it. Player request (2026-07-30): they
        // were big and on the left, where they covered the game's own active-buff
        // list and were themselves painted over by the gauges.
        // ★ FIELD: no gear, no keypad toggle. The player asked for that strip back
        // (2026-07-30) — the game's own buff band is moved into it instead. Both
        // buttons stay available on every NON-field screen (title, menus), which is
        // where settings are actually used: orientation applies at once, and
        // immersive / ZOOM apply at the next start anyway.
        // ★ Cutscene joins the field in having NO chrome: the player asked for OK and
        // `#` alone during a script (2026-08-03), and a gear there would be the third
        // thing on a screen meant to hold two. Settings stay reachable from every
        // menu screen, including the title, which is where they are actually used.
        let (gear, _toggle) = if mode == Mode::Menu {
            gear_row(win_w, win_h)
        } else {
            (Rect::new(0, 0, 0, 0), Rect::new(0, 0, 0, 0))
        };
        // ★ The `123` pull-out keypad toggle is GONE (user, 2026-09-18: "상단 톱니바퀴 옆에
        // 123없애줘"). Every hero2 window is a tap target (`hero2java::h2_tap`), the
        // arrows are hexagons, OK/CLR/`0` have discs and `#` is the `0` long press — the
        // keypad had no key left that a finger could not reach otherwise, except `*`
        // (the game's minimap), which the host has no seat for on hero2 yet. The seat
        // stays in `gear_row` so the gear keeps its place and `placement::obstacles`
        // keeps its shape; a zero rect draws and hits nothing.
        let toggle = Rect::new(0, 0, 0, 0);
        return Layout {
            game: g,
            buttons: b,
            gear,
            joystick: jz,
            keypad_toggle: toggle,
        };
    }

    // Build the left control region either as a D-pad or a floating joystick.
    let mut build_left = |buttons: &mut Vec<Button>, region: Rect| {
        if joystick {
            joy_zone = Some(build_joystick(buttons, region, pad));
        } else {
            build_dpad(buttons, region, pad);
        }
    };

    if !landscape {
        // Portrait "최대 확대": the game fills the FULL WIDTH (its maximal
        // non-distorted size), controls take the strip below (>= 30% of height).
        // Was capped at 60% height, which left a thin side letterbox on tall
        // phones; filling width removes it.
        let game_area_h = (win_w * CONTENT_H / CONTENT_W).min(win_h * 70 / 100);
        game = contain(Rect::new(0, 0, win_w, game_area_h));
        let strip = Rect::new(0, game_area_h, win_w, win_h - game_area_h);
        let lw = strip.w * 42 / 100;
        build_left(&mut buttons, Rect::new(strip.x, strip.y, lw, strip.h));
        let kx = strip.x + strip.w * 44 / 100;
        build_keypad(&mut buttons, Rect::new(kx, strip.y, strip.x + strip.w - kx, strip.h), pad);
    } else {
        // Landscape "최대 확대": the game fills the FULL HEIGHT (its maximal
        // non-distorted size — a 3:4 game cannot fill a 16:9 screen without
        // distortion), centred, and the controls fill the side margins so they
        // are never left black. Was capped at 60% width + 2*pad tall margin.
        let game_h = win_h;
        let game_w = (game_h * CONTENT_W / CONTENT_H).min(win_w);
        let gx = (win_w - game_w) / 2;
        game = contain(Rect::new(gx, 0, game_w, game_h));
        build_left(&mut buttons, Rect::new(0, 0, gx, win_h));
        build_keypad(&mut buttons, Rect::new(gx + game_w, 0, win_w - (gx + game_w), win_h), pad);
    }

    let gs = (win_w.min(win_h) / 11).clamp(60, 150);
    let gear = Rect::new(win_w.saturating_sub(gs + pad * 3), pad * 3, gs, gs);
    // Non-immersive layouts already put the whole keypad on screen, so no toggle.
    Layout { game, buttons, gear, joystick: joy_zone, keypad_toggle: Rect::new(0, 0, 0, 0) }
}

/// Floating joystick: LSK/RSK/CLR/OK at the four corners of the region, and the
/// whole region is the joystick zone (a touch that misses the corners spawns the
/// stick at that point). Returns the joystick zone rect.
fn build_joystick(out: &mut Vec<Button>, region: Rect, pad: u32) -> Rect {
    let cw = region.w / 3;
    let ch = region.h / 3;
    let corner = |c: u32, r: u32| {
        Rect::new(
            region.x + c * cw + pad,
            region.y + r * ch + pad,
            cw.saturating_sub(2 * pad),
            ch.saturating_sub(2 * pad),
        )
    };
    out.push(Button { rect: corner(0, 0), key: KeyCode::LEFT_SOFT_KEY, label: Label::Text("LSK"), dpad: false, face: Face::Solid, wedge: None });
    out.push(Button { rect: corner(2, 0), key: KeyCode::RIGHT_SOFT_KEY, label: Label::Text("RSK"), dpad: false, face: Face::Solid, wedge: None });
    out.push(Button { rect: corner(0, 2), key: KeyCode::CLEAR, label: Label::Text("CLR"), dpad: false, face: Face::Solid, wedge: None });
    out.push(Button { rect: corner(2, 2), key: KeyCode::OK, label: Label::Text("OK"), dpad: false, face: Face::Solid, wedge: None });
    region
}

/// Max drag (px) for full joystick deflection.
///
/// ★ Takes the WINDOW size, not just the zone rect (2026-08-16). It used to be
/// `joy_radius(zone)` and passed `zone.w / zone.h` into [`joy_home`] as if they were
/// the window's — which was only ever correct because the immersive zone happened to
/// BE the whole window. A resizable stick makes `joy_home` resolve a placement
/// against whatever it is handed, so the old form would have silently mis-scaled
/// both the deflection radius and the stick's own seat. (The first TOUCH TYPE B —
/// the pinned stick, since replaced by the field D-pad — also shrank the zone to the
/// stick's disc, which is what surfaced the bug.)
pub fn joy_radius(zone: &Rect, win_w: u32, win_h: u32) -> f64 {
    if crate::immersive_active() {
        // ★ 91차-B: the thumb's PIN radius — as far as the thumb's centre may go from
        // the base's centre, which is the base radius less the thumb's own, so the
        // thumb disc stays inside the base. Deriving this from the (window-sized) zone
        // gave ~450 px once; the base is the placed seat's inscribed circle now.
        return joy_home(win_w, win_h).2 * (1.0 - JOY_THUMB);
    }
    (zone.w.min(zone.h) as f64) * 0.42
}

// ⛔ `joy_follow_radius` LIVED HERE AND IS GONE (2026-09-20, user: *"hero3처럼 …
// 고정된채로 작동해야"*). It answered "how far may the base centre fall behind the
// thumb", and every answer it ever gave — the engage radius (08-03), the pin radius
// (09-18), the base disc's own radius (09-19) — was reported as the base walking with
// the finger. The centre is fixed at the landing point now; the reasoning and the
// three measured attempts are in `App::joy_update`, which was its only caller.

/// The deadzone basis `joy_dir` is given — how far a thumb travels before walking
/// starts (30 %) and how close it may come back before it stops (15 %).
///
/// ★ 91차-B split this from [`joy_radius`]: the pin radius follows the base disc,
/// which is now the whole arrows' square, while the walk-start distance stays what
/// the player has been walking with since 2026-07-30 ([`stick_engage_radius`]). The
/// classic (non-immersive) layouts keep the two equal, as they always were.
pub fn joy_engage_radius(zone: &Rect, win_w: u32, win_h: u32) -> f64 {
    if crate::immersive_active() {
        let seat = crate::placement::rect_of(crate::placement::CtrlId::Stick, Mode::Field, win_w, win_h)
            .unwrap_or_else(|| stick_seat(win_w, win_h));
        return stick_engage_radius(&seat, win_w, win_h);
    }
    joy_radius(zone, win_w, win_h)
}
/// The ONE direction key a joystick deflection `(dx, dy)` maps to, where `(dx, dy)`
/// is measured from the FIXED centre the stick spawned at.
///
/// ★ The centre is fixed as of 2026-08-03. It used to TRAIL the finger (a
/// `joy_follow` that kept the base `radius` behind once the drag exceeded `radius`),
/// which was itself a fix for "walking up, then sliding left without lifting, keeps
/// walking up". The player tested that on device and reported the cure as worse than
/// the disease: the base visibly ran across the screen under the thumb and steering
/// became unpredictable ("조이스틱 자체가 따라와서 이동이 좀 불편해"). Now: touch down
/// = centre, lift and touch again = new centre, and nothing moves in between.
///
/// The trade-off is real and worth stating. With a fixed centre the direction is the
/// angle of the WHOLE vector from the spawn point, so a thumb that has wandered far
/// past the ring needs a proportionally large sideways move to turn. In normal play
/// it does not arise — the thumb disc pins at the ring edge, which is the feedback
/// that says "you are already at full tilt" — and how far the vector can grow is
/// bounded by how far the player chooses to drag.
///
/// The mapping itself is 4-way, dominant axis, with hysteresis.
///
/// Why one key and not the old 8-way two-key set: the game walks in ONE latched
/// direction, so holding UP+LEFT through a diagonal slide does not walk diagonally
/// — it keeps walking UP and drops LEFT on the floor, and the later keyup(UP)
/// leaves the guest with no fresh keydown to latch. Emitting exactly one direction
/// means every turn is a clean keyup(old) + keydown(new), which the guest acts on.
///
/// `active` = the direction currently held; it is KEPT unless the challenging axis
/// beats it by [`TURN_RATIO`], so a thumb resting near a diagonal cannot flutter
/// between two directions.
pub fn joy_dir(dx: f64, dy: f64, radius: f64, active: Option<KeyCode>) -> Option<KeyCode> {
    /// Deflection needed (as a fraction of `radius`) to start / to keep walking.
    const DEAD_ENGAGE: f64 = 0.30;
    const DEAD_KEEP: f64 = 0.15;
    /// How far the challenging axis must beat the held one to take the turn.
    const TURN_RATIO: f64 = 1.2;

    let dist = (dx * dx + dy * dy).sqrt();
    let dead = radius * if active.is_some() { DEAD_KEEP } else { DEAD_ENGAGE };
    if dist <= dead || dist == 0.0 {
        return None;
    }
    // Signed deflection along each key's own direction.
    let comp = |k: KeyCode| match k {
        KeyCode::UP => -dy,
        KeyCode::DOWN => dy,
        KeyCode::LEFT => -dx,
        _ => dx,
    };
    let cand = if dx.abs() >= dy.abs() {
        if dx >= 0.0 { KeyCode::RIGHT } else { KeyCode::LEFT }
    } else if dy >= 0.0 {
        KeyCode::DOWN
    } else {
        KeyCode::UP
    };
    if let Some(cur) = active
        && cur != cand
        && comp(cur) > 0.0
        && comp(cand) < comp(cur) * TURN_RATIO
    {
        return Some(cur); // not a decisive turn — keep walking the same way
    }
    Some(cand)
}

fn build_dpad(out: &mut Vec<Button>, region: Rect, pad: u32) {
    let cw = region.w / 3;
    let ch = region.h / 3;
    let cell = |c: u32, r: u32| -> Rect {
        Rect::new(
            region.x + c * cw + pad,
            region.y + r * ch + pad,
            cw.saturating_sub(2 * pad),
            ch.saturating_sub(2 * pad),
        )
    };
    let mut push = |c, r, key, label, dpad| out.push(Button { rect: cell(c, r), key, label, dpad, face: Face::Solid, wedge: None });
    push(0, 0, KeyCode::LEFT_SOFT_KEY, Label::Text("LSK"), false);
    push(1, 0, KeyCode::UP, Label::Arrow(Dir::Up), true);
    push(2, 0, KeyCode::RIGHT_SOFT_KEY, Label::Text("RSK"), false);
    push(0, 1, KeyCode::LEFT, Label::Arrow(Dir::Left), true);
    push(1, 1, KeyCode::OK, Label::Text("OK"), true);
    push(2, 1, KeyCode::RIGHT, Label::Arrow(Dir::Right), true);
    push(0, 2, KeyCode::CLEAR, Label::Text("CLR"), false);
    push(1, 2, KeyCode::DOWN, Label::Arrow(Dir::Down), true);
    push(2, 2, KeyCode::OK, Label::Text("OK"), true);
}

fn build_keypad(out: &mut Vec<Button>, region: Rect, pad: u32) {
    let cw = region.w / 3;
    let ch = region.h / 4;
    let keys: [[(&'static str, KeyCode); 3]; 4] = [
        [("1", KeyCode::NUM1), ("2", KeyCode::NUM2), ("3", KeyCode::NUM3)],
        [("4", KeyCode::NUM4), ("5", KeyCode::NUM5), ("6", KeyCode::NUM6)],
        [("7", KeyCode::NUM7), ("8", KeyCode::NUM8), ("9", KeyCode::NUM9)],
        [("*", KeyCode::STAR), ("0", KeyCode::NUM0), ("#", KeyCode::HASH)],
    ];
    for (r, row) in keys.iter().enumerate() {
        for (c, (txt, key)) in row.iter().enumerate() {
            let rect = Rect::new(
                region.x + c as u32 * cw + pad,
                region.y + r as u32 * ch + pad,
                cw.saturating_sub(2 * pad),
                ch.saturating_sub(2 * pad),
            );
            out.push(Button {
                rect,
                key: *key,
                label: Label::Text(txt),
                dpad: false,
                face: Face::Solid,
                wedge: None,
            });
        }
    }
}

pub fn hit(buttons: &[Button], x: f64, y: f64) -> Option<KeyCode> {
    buttons.iter().find(|b| b.contains(x, y)).map(|b| b.key)
}

/// Quickbar action-button rects: a top-to-bottom column of round slots along the
/// RIGHT edge of the game rect (H5-style), starting below the top-right gear.
/// One rect per quickbar slot passed.
pub fn quick_buttons(game: Rect, gear: Rect, n: usize, mode: Mode) -> Vec<Rect> {
    let mut out = Vec::new();
    if n == 0 || game.w == 0 || game.h == 0 || mode == Mode::Cutscene {
        return out;
    }
    if crate::immersive_active() && game.w > game.h {
        // Immersive (reference-style HUD): the slots are SKILL BUTTONS on an arc
        // up-and-left of the big action disc, not a keypad column — and each seat is
        // individually placeable, so they come from the resolver like every other
        // control instead of from the ring formula directly.
        //
        // Resolved for the CURRENT mode, not always for the field: since 2026-08-03
        // the plates stay on screen with a game window open, and the editor keeps a
        // separate arrangement per mode, so asking the field resolver here would draw
        // them where the player put their FIELD plates while hit-testing somewhere
        // else.
        let placed = crate::placement::resolve(mode, game.w, game.h);
        let hidden = crate::quick_hidden();
        return (0..n.min(usize::from(crate::QUICK_SLOTS)) as u8)
            .map(|i| {
                // ★★ A HIDDEN plate is a ZERO-SIZED rect AT ITS OWN INDEX, never a
                // dropped entry (hero3 82차). `Rect::contains` is half-open so it can
                // never be hit, `draw_pick_highlight` and `padhud::badge_at` already
                // skip zero rects, and the tap falls through to the arm below the
                // plates — the joystick on the field, the game's own panel in a menu.
                //
                // ⛔ Do NOT filter these out. `app.rs` addresses `self.quickbar`, the
                // cooldown vector, `quick_press` and the pad badges BY THIS INDEX
                // (`quickbar[i].slot == i`), so a shorter vector makes every later plate
                // fire, draw and badge as its neighbour.
                if crate::quickpage::plate_hidden(hidden, i) {
                    return Rect::new(0, 0, 0, 0);
                }
                placed
                    .iter()
                    .find(|(c, _)| *c == crate::placement::CtrlId::Quick(i))
                    .map(|(_, r)| *r)
                    .unwrap_or_else(|| Rect::new(0, 0, 0, 0))
            })
            .collect();
    }
    if mode != Mode::Field {
        // Non-immersive layouts already put a full keypad on screen; the plates are a
        // field-HUD extra there, not a second way to press the same keys.
        return out;
    }
    let s = (game.w / 6).clamp(28, 84).min(game.h / 6);
    let gap = (s / 6).max(3);
    let x = game.x + game.w.saturating_sub(s + gap);
    let mut y = (gear.y + gear.h + gap).max(game.y + gap);
    // ★ Capped like the immersive branch above (2026-09-01). This loop had NO upper
    // bound, so a scanner reporting more slots than the bar has would draw plates that
    // hit-test fine and then find no slot behind them. `QUICK_SLOTS` is the one place
    // the bar's width is stated.
    let hidden = crate::quick_hidden();
    for i in 0..n.min(usize::from(crate::QUICK_SLOTS)) {
        // Same rule as the immersive path: a hidden plate keeps its INDEX and gets no
        // rect, and it does not advance `y` — so the column closes up instead of
        // leaving a hole, and the indices still line up with `self.quickbar`.
        if crate::quickpage::plate_hidden(hidden, i as u8) {
            out.push(Rect::new(0, 0, 0, 0));
            continue;
        }
        if y + s > game.y + game.h {
            break;
        }
        out.push(Rect::new(x, y, s, s));
        y += s + gap;
    }
    out
}

/// Live cash-buff status lines, drawn under the unspent-point badges while a game
/// window (menu) is open — player request 2026-08-05: "내가 어떤 캐시템이 현재
/// 적용중인지 he mp바 하단에 글자로 보여줄거야 ... 스탯 및 스킬 + 아래로".
///
/// ★ 2026-08-06 — KOREAN item names, at a smaller size (player: "초월의 문장, 경험의
/// 보주 등등 현재 버프 적용되고있는거 한글로 표시했으면 좋겠어. 그리고 글자크기도 살짝
/// 줄여줘"). Until now this was ASCII-only because `draw_glyph` is a 5x7 ASCII font;
/// [`crate::hangul`] supplies 12x12 bitmaps for the CLOSED set of syllables these
/// lines can contain, which is what makes shipping Korean here possible without a
/// font. The strings are composed by `crate::buffs::status_lines` on the worker (it
/// owns the clock).
///
/// The scale drops from `game.h / 200` to `game.h / 300` — on a 1080-tall window that
/// is the ASCII run going 35 px -> 21 px tall. The Hangul cell stays FULL-WIDTH at the
/// same scale (12s against the Latin 7s), which is not an inconsistency: a Hangul
/// syllable fills its em where a Latin capital is about 70% of it, so equal scales are
/// what makes the two look the same size.
pub fn draw_buff_lines(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect, lines: &[String]) {
    if game.w == 0 || game.h == 0 || lines.is_empty() {
        return;
    }
    let b = stats_block(game);
    let s = (game.h / 300).clamp(2, 4);
    let x = b.bars.x;
    let line_h = crate::hangul::HANGUL_H * s;
    // Under the whole block (ring, level plate and badges all reach its bottom row).
    let mut y = stats_bottom(game) + (2 * s).max(4);
    for line in lines {
        let tw = mixed_text_w(line, s);
        // Dim pad behind the glyphs so they stay readable over the game's window art.
        blend_rect(
            buf,
            win_w,
            win_h,
            x.saturating_sub(s),
            y.saturating_sub(s),
            tw + 2 * s,
            line_h + 2 * s,
            0xff05_070c,
            140,
        );
        draw_mixed_text(buf, win_w, win_h, x, y, s, line, LABEL);
        y += line_h + (2 * s).max(4);
    }
}

/// Rendered width of a mixed Hangul/ASCII string at `scale`.
fn mixed_text_w(text: &str, scale: u32) -> u32 {
    text.chars()
        .map(|ch| if ch.is_ascii() { 6 * scale } else { (crate::hangul::HANGUL_W + 1) * scale })
        .sum()
}

/// Draw a string that mixes Hangul (12x12, [`crate::hangul`]) and ASCII (5x7,
/// [`draw_glyph`]).
///
/// `y` is the TOP of the Hangul cell; the shorter ASCII glyphs are dropped to sit on
/// the same baseline instead of hanging from the same top, because a line reading
/// "경험의보주 6일 23시간" with the digits floating above the syllables is worse than
/// no Korean at all. A character with no glyph in either font falls through to
/// `draw_glyph`, which has its own unknown-character box.
fn draw_mixed_text(buf: &mut [u32], win_w: u32, win_h: u32, mut x: u32, y: u32, scale: u32, text: &str, color: u32) {
    let baseline_drop = (crate::hangul::HANGUL_H - 7) * scale;
    for ch in text.chars() {
        if ch.is_ascii() {
            draw_glyph(buf, win_w, win_h, x, y + baseline_drop, scale, ch, color);
            x += 6 * scale;
        } else {
            draw_hangul(buf, win_w, win_h, x, y, scale, ch, color);
            x += (crate::hangul::HANGUL_W + 1) * scale;
        }
    }
}

fn draw_hangul(buf: &mut [u32], win_w: u32, win_h: u32, x: u32, y: u32, scale: u32, ch: char, color: u32) {
    let Some(rows) = crate::hangul::glyph(ch) else {
        return;
    };
    let w = crate::hangul::HANGUL_W;
    for (r, bits) in rows.iter().enumerate() {
        for c in 0..w {
            if bits & (1 << (w - 1 - c)) != 0 {
                fill(buf, win_w, win_h, x + c * scale, y + r as u32 * scale, scale, scale, color);
            }
        }
    }
}

/// Draw a round control whose face is a UI icon COVERAGE MASK, tinted `color`.
///
/// Separate from `blit_icon`: that samples a decoded game icon, which carries its own
/// colour and a 1-bit silhouette. These masks carry only shape (see `ui_icons.rs`), so
/// the colour is the caller's and comes from the same `LABEL` palette as the hand-drawn
/// glyphs — which is the whole reason the packer throws the source RGB away.
///
/// Bilinear on the coverage, for the same reason `blit_icon` is bilinear: a 96px mask
/// scaled to a ~90px disc at a non-integer factor looks blocky under nearest.
///
/// The colour is NOT a parameter: it is `LABEL`, the same foreground every other control
/// uses, and the palette belongs to this module. A caller that could pick the colour is a
/// caller that can make this button look like it belongs to a different HUD.
///
/// ★ Its one user is the round minimap RESTORE button ([`minimap_restore_button`]) — gone
/// 2026-10-01 when the toggle became the `MAP` top tab, back 2026-10-21 in the hidden panel's
/// corner.
pub fn draw_mask_button(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, icon: ui_icons::Icon, pressed: bool) {
    draw_mask_button_spanned(buf, win_w, win_h, r, icon, pressed, 1.5);
}

/// [`draw_mask_button`] with the icon's span across the disc given explicitly, as a
/// multiple of the disc RADIUS (1.5 = the shared default, i.e. 75% of the button).
///
/// Split out so one control can carry a smaller glyph without every control changing —
/// the disc, its rim and its pressed state all stay identical, which is what keeps it
/// reading as one of the set. See [`QUICKPAGE_ICON_SPAN`].
pub fn draw_mask_button_spanned(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    r: Rect,
    icon: ui_icons::Icon,
    pressed: bool,
    icon_span: f64,
) {
    let color = LABEL;
    if r.w == 0 || r.h == 0 {
        return;
    }
    let cx = f64::from(r.x) + f64::from(r.w) / 2.0;
    let cy = f64::from(r.y) + f64::from(r.h) / 2.0;
    let rad = f64::from(r.w.min(r.h)) / 2.0;
    // Same face as every other round control, so this reads as one of them.
    blend_disc_soft(buf, win_w, win_h, cx, cy, rad, 0xff0a_0c12, if pressed { 150 } else { 90 }, (rad * 0.09).max(1.2));
    let Some(m) = ui_icons::icons().get(icon) else {
        return; // Face stays a bare disc: a missing asset must not blank the control.
    };
    // Inset so the disc's rim stays visible; 1.5 matches `blit_icon`'s box_side ratio.
    let side = rad * icon_span;
    if side < 1.0 || m.w == 0 || m.h == 0 {
        return;
    }
    let dw = side.round().max(1.0) as u32;
    let (mw, mh) = (f64::from(m.w), f64::from(m.h));
    let ox = (cx - side / 2.0).round().max(0.0) as u32;
    let oy = (cy - side / 2.0).round().max(0.0) as u32;
    for py in 0..dw {
        for px in 0..dw {
            let (x, y) = (ox + px, oy + py);
            if x >= win_w || y >= win_h {
                continue;
            }
            let sx = (f64::from(px) + 0.5) / f64::from(dw) * mw - 0.5;
            let sy = (f64::from(py) + 0.5) / f64::from(dw) * mh - 0.5;
            let a = sample_coverage(m, sx, sy);
            if a > 0 {
                blend_px(buf, (y * win_w + x) as usize, color, u32::from(a));
            }
        }
    }
}

/// Draw a round control whose face is a short ASCII LABEL.
///
/// For controls with no icon art: `ui_icons.bin` is packed from author-supplied PNGs
/// (`android_app/tools/gen_ui_icons.py`), so a new control cannot invent itself a
/// glyph. Same disc face as [`draw_mask_button`], so it still reads as one of the set.
///
/// ★ No caller since 2026-10-01 — SAVE, its one user, is the top tab [`draw_top_tab`].
#[allow(dead_code)]
pub fn draw_label_button(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, text: &str, pressed: bool) {
    if r.w == 0 || r.h == 0 {
        return;
    }
    let cx = f64::from(r.x) + f64::from(r.w) / 2.0;
    let cy = f64::from(r.y) + f64::from(r.h) / 2.0;
    let rad = f64::from(r.w.min(r.h)) / 2.0;
    blend_disc_soft(buf, win_w, win_h, cx, cy, rad, 0xff0a_0c12, if pressed { 150 } else { 90 }, (rad * 0.09).max(1.2));
    draw_centered_text(buf, win_w, win_h, label_box(r), text, LABEL);
}

/// The TOP TAB's chrome, measured off hero3's own SAVE / MAP tabs (vc422 on the S25,
/// 2026-10-01): an opaque gold rim (185, 131, 35) and a dark face that lets about a third
/// of the world through. Host chrome, not game art — hero3 draws its tabs itself too — so
/// this is taking a design, the way `hud_view.rs` takes hero5's.
const TAB_RIM: u32 = 0xffb9_8323;
const TAB_FACE: u32 = 0xff0a_0c12;
const TAB_FACE_A: u32 = 170;
/// Characters in the longest tab label (`SAVE`). Every tab's lettering is sized for it,
/// so `MAP` and `SAVE` read at one scale side by side.
const TAB_LABEL_CHARS: u32 = 4;

/// Rim thickness and bottom-corner radius of a tab `w` px wide: hero3's 3 and 8 at 86.
fn tab_rim(w: u32) -> (u32, u32) {
    let t = (w / 28).max(1);
    (t, (w / 10).max(t + 1))
}

/// Draw a TOP TAB (SAVE, MAP) at `r`: flush with the window's top edge, square at the top
/// and round at the bottom, a gold rim on the three sides that are not the screen's edge,
/// and `text` in the HUD's 5×7 ASCII font (the CLR disc's), centred in the face above the
/// bottom rim. What it draws is smaller than what it answers to — see [`top_tab_hit`].
///
/// ★ The lettering is that font made BOLD — every glyph laid twice, one screen pixel
/// apart — because hero3's tabs letter in its own heavy game font, which hero2 does not
/// get to borrow (the `gamefont_data.rs` rule). One pixel and not one glyph cell: a whole
/// cell fills the `M`'s and `V`'s diagonals in; a pixel only thickens the strokes.
pub fn draw_top_tab(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, text: &str) {
    if r.w == 0 || r.h == 0 {
        return;
    }
    let (t, rad) = tab_rim(r.w);
    draw_tab_face(buf, win_w, win_h, r, (TAB_FACE, TAB_FACE_A), (TAB_RIM, 255), t, rad);
    let inner_h = r.h.saturating_sub(t);
    // A glyph advances 6 cells and its 6th is blank, so a run of n is `6n - 1` cells wide
    // (+1 px for the bold pass).
    let s = (r.w.saturating_sub(t * 2 + 5) / (TAB_LABEL_CHARS * 6 - 1)).min(inner_h * 3 / 5 / 7).max(1);
    let bold = u32::from(s >= 2);
    let n = text.chars().count() as u32;
    let tw = (n * 6).saturating_sub(1) * s + bold;
    let x = r.x + r.w.saturating_sub(tw) / 2;
    let y = r.y + inner_h.saturating_sub(7 * s) / 2;
    for dx in 0..=bold {
        draw_text(buf, win_w, win_h, x + dx, y, s, text, LABEL);
    }
}

/// A tab's face: `face` over the whole shape and `rim` over a `t`-px band on its left,
/// right and bottom — the top is the window's edge — with the two bottom corners rounded
/// to `rad`. Each colour is `(argb, alpha)`. Anti-aliased on the two arcs only: the
/// straight edges are whole pixels, which keeps a 3-px rim crisp at every size.
#[allow(clippy::too_many_arguments)]
fn draw_tab_face(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, face: (u32, u32), rim: (u32, u32), t: u32, rad: u32) {
    let rad = rad.min(r.w / 2).min(r.h);
    let (tf, rf) = (f64::from(t), f64::from(rad));
    // The corner circles' centres, in continuous coordinates.
    let ccy = f64::from(r.y + r.h) - rf;
    let (ccl, ccr) = (f64::from(r.x) + rf, f64::from(r.x + r.w) - rf);
    for y in r.y..(r.y + r.h).min(win_h) {
        let row = (y * win_w) as usize;
        let py = f64::from(y) + 0.5;
        let corner_row = y + rad >= r.y + r.h;
        for x in r.x..(r.x + r.w).min(win_w) {
            // (how much of the pixel is inside the tab, how much of THAT is face not rim)
            let (cov, face_k) = if corner_row && (x < r.x + rad || x + rad >= r.x + r.w) {
                let cx = if x < r.x + rad { ccl } else { ccr };
                let d = ((f64::from(x) + 0.5 - cx).powi(2) + (py - ccy).powi(2)).sqrt();
                ((rf - d + 0.5).clamp(0.0, 1.0), ((rf - tf) - d + 0.5).clamp(0.0, 1.0))
            } else {
                let edge = (x - r.x).min(r.x + r.w - 1 - x).min(r.y + r.h - 1 - y);
                (1.0, if edge < t { 0.0 } else { 1.0 })
            };
            if cov <= 0.0 {
                continue;
            }
            let i = row + x as usize;
            if face_k > 0.0 {
                blend_px(buf, i, face.0, (f64::from(face.1) * cov * face_k).round() as u32);
            }
            if face_k < 1.0 {
                blend_px(buf, i, rim.0, (f64::from(rim.1) * cov * (1.0 - face_k)).round() as u32);
            }
        }
    }
}

/// Bilinear coverage from a mask at fractional `(sx, sy)`, clamped at the edges.
fn sample_coverage(m: &ui_icons::Mask, sx: f64, sy: f64) -> u8 {
    let x0 = sx.floor();
    let y0 = sy.floor();
    let fx = sx - x0;
    let fy = sy - y0;
    let cl = |v: f64, hi: u32| -> u32 { v.max(0.0).min(f64::from(hi.saturating_sub(1))) as u32 };
    let (xi, yi) = (cl(x0, m.w), cl(y0, m.h));
    let (xj, yj) = (cl(x0 + 1.0, m.w), cl(y0 + 1.0, m.h));
    let p = |x: u32, y: u32| f64::from(m.coverage(x, y));
    let top = p(xi, yi) * (1.0 - fx) + p(xj, yi) * fx;
    let bot = p(xi, yj) * (1.0 - fx) + p(xj, yj) * fx;
    (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8
}

/// Index of the quickbar button under `(x, y)`, if any.
pub fn quick_hit(rects: &[Rect], x: f64, y: f64) -> Option<usize> {
    rects.iter().position(|r| r.contains(x, y))
}

// Skill cooldowns (2026-08-05, reworked 2026-08-07) are host-owned — the original
// game has no cooldown state at all, skills are gated by SP + the cast delay. A
// cooldown exists ONLY for a skill listed in `wie_cooldowns.txt` (see
// `config::read_skill_cooldowns`; unlisted = none), and it only starts once the
// WORKER confirms the cast actually ran (`UserEvent::SkillCast`). A tap on a
// plate whose cooldown is still running is swallowed; the plate fills bottom-up
// as the cooldown recovers and is ready when the fill reaches the top.

/// The rising cooldown fill: a dark veil over the not-yet-recovered part of the
/// disc, and a coloured fill that grows from the BOTTOM as `frac` (elapsed /
/// total, 0..1) approaches 1 — "아래서부터 색감이 차오르는 방식".
fn draw_cooldown_overlay(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, rad: f64, frac: f64) {
    const VEIL: u32 = 0xff05_070c; // not recovered yet: darkened
    const FILL: u32 = 0xff4a_6cff; // recovered from the bottom up: the pressed blue
    let frac = frac.clamp(0.0, 1.0);
    if rad < 1.0 || frac >= 1.0 {
        return;
    }
    let fill_top = cy + rad - 2.0 * rad * frac;
    let x0 = (cx - rad).max(0.0) as u32;
    let x1 = ((cx + rad).max(0.0) as u32 + 1).min(win_w);
    let y0 = (cy - rad).max(0.0) as u32;
    let y1 = ((cy + rad).max(0.0) as u32 + 1).min(win_h);
    for y in y0..y1 {
        let dy = f64::from(y) + 0.5 - cy;
        let row = (y * win_w) as usize;
        for x in x0..x1 {
            let dx = f64::from(x) + 0.5 - cx;
            if dx * dx + dy * dy > rad * rad {
                continue;
            }
            if f64::from(y) + 0.5 >= fill_top {
                blend_px(buf, row + x as usize, FILL, 96);
            } else {
                blend_px(buf, row + x as usize, VEIL, 120);
            }
        }
    }
}

/// Draw the quickbar as ROUND action buttons (H5-style): filled disc + rim,
/// coloured by kind. Occupied slots show the DECODED skill/potion icon (from the
/// embedded atlas, number-free) + for items the live stack count; empty slots
/// show the hotkey char as a placeholder.
///
/// `cooldowns[i]` = the slot's cooldown progress (elapsed fraction 0..1); `>= 1`
/// or missing = ready, nothing drawn. Index-aligned with `rects`.
pub fn draw_quickbar(buf: &mut [u32], win_w: u32, win_h: u32, rects: &[Rect], slots: &[QuickSlot], pressed: Option<usize>, cooldowns: &[f64]) {
    if crate::immersive_active() {
        draw_skill_buttons(buf, win_w, win_h, rects, slots, pressed, cooldowns);
        return;
    }
    let at = atlas::atlas();
    for (i, r) in rects.iter().enumerate() {
        let Some(slot) = slots.get(i) else {
            break;
        };
        // ⛔ A ZERO-SIZED rect is a plate the player has switched off (see
        // `quick_buttons`), and it must draw NOTHING. Every disc/icon helper below
        // already bails on a tiny radius, but the item STACK COUNT block does not —
        // it picks its own scale from the rect height and would print the digits at
        // the rect's corner, i.e. in the top-left of the screen.
        if r.w == 0 || r.h == 0 {
            continue;
        }
        let base = if pressed == Some(i) {
            QB_PRESSED
        } else if crate::quickslot_inert(slot) {
            QB_INERT
        } else {
            match slot.kind {
                SlotKind::Skill => QB_SKILL,
                SlotKind::Item => QB_ITEM,
                SlotKind::Empty => QB_EMPTY,
            }
        };
        let cx = r.x as f64 + r.w as f64 / 2.0;
        let cy = r.y as f64 + r.h as f64 / 2.0;
        let rad = (r.w.min(r.h) as f64) / 2.0;
        fill_disc(buf, win_w, win_h, cx, cy, rad, base);
        blend_ring_soft(buf, win_w, win_h, cx, cy, rad - 1.0, 2.0, BORDER, 255);

        let icon = slot.icon.and_then(|ic| match ic {
            IconRef::Item(f) => at.item(f),
            IconRef::Skill(sheet, cell) => at.skill(sheet, cell),
            IconRef::Beast(id) => at.beast(id),
            IconRef::ItemType(t) => at.item_type(t),
        });
        if let Some(ic) = icon {
            blit_icon(buf, win_w, win_h, cx, cy, rad, ic);
            if matches!(slot.kind, SlotKind::Item) {
                // Our own clean stack count (bottom-right), since the atlas icon has none.
                let cnt = format!("{}", slot.count.min(99));
                let bot = Rect::new(r.x, r.y + r.h * 3 / 5, r.w, r.h * 2 / 5);
                draw_centered_text(buf, win_w, win_h, bot, &cnt, LABEL);
            }
        }
        // No icon = an empty slot, and it stays blank. Same reasoning as the immersive
        // plates above: the hotkey character is the mechanism, not the label.
        if let Some(f) = cooldowns.get(i).copied().filter(|f| *f < 1.0) {
            draw_cooldown_overlay(buf, win_w, win_h, cx, cy, rad, f);
        }
    }
}

/// Top-right minimap panel: the game's own offscreen minimap buffer, nearest-
/// neighbour enlarged to fill the panel. Until the player opens the map once the
/// game has not painted that buffer, so the panel shows a `MAP` hint instead
/// (tapping it sends `*`, which is what triggers the paint).
#[allow(clippy::too_many_arguments)]
pub fn draw_minimap(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect, mm: Option<&crate::Minimap>, marker: Option<(f32, f32)>, mobs: &[(f32, f32)]) {
    if !crate::immersive_active() || game.w == 0 {
        return;
    }
    draw_minimap_in(buf, win_w, win_h, minimap_panel(win_w, win_h), mm, true, marker, mobs);
}

// `map_overlay_panel` / `draw_map_overlay` were REMOVED 2026-08-04 (player request:
// "가운데 확대해서 표시하는 기능은 제거하고"). They painted a big centred copy of the
// mirrored minimap over the game's own map window — the 6차/7차 answer to "the guest
// draws its map at 1:1, which is tiny on a wide framebuffer".
//
// The show/hide toggle replaced the need for it: the minimap is now a panel you travel
// with, so there is nothing to blow up. Deleted rather than left dead — the panel
// geometry and the 82% figure are recoverable from git history, and carrying an unused
// full-screen draw path invites someone to call it again.

// hero2's hero5-style view (fixed zoom, follows the player, exits as blue bars) — its own
// dependency-free file so `wie_probe --h2-minimap` draws the identical panel headless.
include!("minimap_view.rs");

// The MAP tab's world map (picture + camera + arrows). The picture bytes are the
// painted continent; the labels and the green dot are drawn over it so a later
// picture can replace the bytes without moving the places.
include!("worldmap_view.rs");

fn worldmap_tex() -> &'static (u32, u32, Vec<u16>) {
    static TEX: std::sync::OnceLock<(u32, u32, Vec<u16>)> = std::sync::OnceLock::new();
    TEX.get_or_init(|| {
        let raw = include_bytes!("worldmap.rgb565");
        if raw.len() < 8 {
            return (0, 0, Vec::new());
        }
        let w = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let h = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
        let px: Vec<u16> = raw[8..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        if px.len() != (w as usize).saturating_mul(h as usize) {
            return (0, 0, Vec::new());
        }
        (w, h, px)
    })
}

pub fn worldmap_size() -> (u32, u32) {
    let (w, h, _) = worldmap_tex();
    (*w, *h)
}

/// The picture window's size for the camera (`wm_open_at` wants the box the
/// picture is drawn into, not the whole window).
pub fn worldmap_view_size(win_w: u32, win_h: u32) -> (u32, u32) {
    let p = wm_panel(win_w, win_h);
    (p.view.w, p.view.h)
}

/// hero5 letters its place names 12 px tall; the host font's em is 16 px, so the
/// scale is 12 hero5 units over 16, never under 2.
fn wm_text_scale(unit: u32) -> u32 {
    ((12 * unit + 8) / 16).clamp(2, 6)
}

/// What a touch on the open world map lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WmHit {
    /// The round back button in the title band.
    Back,
    /// The 미니맵 ON/OFF box in the picture's corner.
    MiniToggle,
    /// A place's stud or its name box.
    Place(usize),
    /// Somewhere else on the panel (nothing happens).
    Panel,
    /// Off the panel (closes it, like any popup).
    Outside,
}

pub fn worldmap_hit(win_w: u32, win_h: u32, cam: WmCam, x: f64, y: f64) -> WmHit {
    let p = wm_panel(win_w, win_h);
    let (iw, ih) = worldmap_size();
    let (xi, yi) = (x as i32, y as i32);
    if p.back.contains(xi, yi) {
        return WmHit::Back;
    }
    if p.mini.contains(xi, yi) {
        return WmHit::MiniToggle;
    }
    let scale = wm_text_scale(p.unit);
    if let Some(i) = wm_hit_place(cam, iw, ih, p.view, p.unit, |i| crate::hostfont::text_w(WM_PLACES[i].name, scale), xi, yi) {
        return WmHit::Place(i);
    }
    if p.frame.contains(xi, yi) { WmHit::Panel } else { WmHit::Outside }
}

/// The world map popup (hero5's StateMap::DrawWorldMap, in hero2's window chrome):
/// the field dimmed behind it, a black title band with the continent's name and
/// the round back button, the painted picture through the camera window with
/// hero5's dark edge shading, the roads as straight lines, a stud on every place with its name in a
/// half-transparent box under it, the character's place lit green, the cursor's
/// place under an orange pin (`pin_lift` px up while it bounces) over a stud that
/// breathes (`glow` 0..=255), and the 미니맵 toggle in the bottom-left corner. The
/// menu arrows are the field's own d-pad, drawn by the caller on top.
pub fn draw_worldmap(buf: &mut [u32], win_w: u32, win_h: u32, cam: WmCam, here: u32, selected: usize, glow: u32, pin_lift: i32, minimap_on: bool) {
    let (iw, ih, px) = worldmap_tex();
    let p = wm_panel(win_w, win_h);
    if p.view.w == 0 || p.view.h == 0 {
        return;
    }
    // Dim everything behind the popup (the same 40 % the settings modal uses).
    for v in buf.iter_mut() {
        let c = *v;
        let r = ((c >> 16) & 0xff) * 4 / 10;
        let g = ((c >> 8) & 0xff) * 4 / 10;
        let b = (c & 0xff) * 4 / 10;
        *v = 0xff00_0000 | (r << 16) | (g << 8) | b;
    }
    // hero2's window chrome: [dark edge][bevel] around a black inside.
    let f = p.frame;
    fill(buf, win_w, win_h, f.x, f.y, f.w, f.h, H2_EDGE);
    let e = WM_EDGE / 2;
    fill(buf, win_w, win_h, f.x + e, f.y + e, f.w.saturating_sub(2 * e), f.h.saturating_sub(2 * e), H2_BEVEL);
    fill(buf, win_w, win_h, p.band.x, p.band.y, p.band.w, p.band.h, H2_BAND);
    let full = WmBox { x: 0, y: 0, w: win_w, h: win_h };

    // The picture, then hero5's 35-px edge shading.
    wm_blit(buf, win_w, win_h, px, *iw, *ih, cam, p.view);
    wm_vignette(buf, win_w, win_h, p.view, 35 * p.unit);

    // The roads first (hero5's zone-map lines): every walkable pair of places as a
    // straight line, clipped to the picture window — a road whose both ends are
    // off screen still shows where it crosses. The studs and the name boxes go
    // on top, so a line never hides a name.
    for &(a, b) in WM_ROADS {
        let (x0, y0) = wm_screen_raw(cam, WM_PLACES[a].x, WM_PLACES[a].y, *iw, *ih, p.view);
        let (x1, y1) = wm_screen_raw(cam, WM_PLACES[b].x, WM_PLACES[b].y, *iw, *ih, p.view);
        wm_draw_road(buf, win_w, win_h, p.view, x0, y0, x1, y1, p.unit);
    }

    // Studs and names. The cursor's stud and the pin go last so nothing covers them.
    let here_i = wm_place_index(here);
    let scale = wm_text_scale(p.unit);
    let text_h = 16 * scale;
    let view_i = (p.view.x as i32, p.view.y as i32, (p.view.x + p.view.w) as i32, (p.view.y + p.view.h) as i32);
    let margin = (16 * p.unit) as i32;
    let mut cursor_at = None;
    for (i, place) in WM_PLACES.iter().enumerate() {
        let (x, y) = wm_screen_raw(cam, place.x, place.y, *iw, *ih, p.view);
        if x < view_i.0 - margin || y < view_i.1 - margin || x > view_i.2 + margin || y > view_i.3 + margin {
            continue;
        }
        if i == selected {
            cursor_at = Some((x, y));
        } else {
            let kind = if i == here_i { WmStud::Here } else { WmStud::Plain };
            wm_draw_stud(buf, win_w, win_h, p.view, x, y, p.unit, kind);
        }
        let tw = crate::hostfont::text_w(place.name, scale);
        let b = wm_label_box(x, y, tw, p.unit);
        // The host font has no clip, so a name box that would spill over the
        // picture's edge is pushed back inside (★ 2026-10-20 (2): 지하연구소 in the
        // corner had no name at all); one that would not fit even then is left out.
        let Some(b) = wm_label_inside(b, p.view) else { continue };
        // Nor one under the 미니맵 toggle box in the corner.
        let m = p.mini;
        if b.x < m.x + m.w && m.x < b.x + b.w && b.y < m.y + m.h && m.y < b.y + b.h {
            continue;
        }
        wm_fill_a(buf, win_w, win_h, p.view, b, 0x0000_0000, 150);
        let tx = b.x + 4 * p.unit;
        let ty = b.y + b.h.saturating_sub(text_h) / 2;
        crate::hostfont::draw(buf, win_w, win_h, tx + 1, ty + 1, scale, place.name, 0xff10_1218);
        crate::hostfont::draw(buf, win_w, win_h, tx, ty, scale, place.name, 0xffff_fff4);
    }
    if let Some((x, y)) = cursor_at {
        wm_draw_stud(buf, win_w, win_h, p.view, x, y, p.unit, WmStud::Cursor { glow, here: selected == here_i });
        wm_draw_pin(buf, win_w, win_h, p.view, x, y, p.unit, pin_lift);
    }

    // The title band: the continent's name on a lighter plate at the left (hero5's
    // THE ORBIS ISLANDS plate), the back button at the right.
    let title = "에레보스";
    let ts = ((p.band.h * 7 / 10) / 16).clamp(2, 8);
    let tw = crate::hostfont::text_w(title, ts);
    let plate = WmBox { x: p.band.x, y: p.band.y, w: tw + 12 * p.unit + p.band.h, h: p.band.h };
    for dx in 0..plate.w {
        // The plate fades out to the right like hero5's.
        let a = 110 - 110 * dx / plate.w.max(1);
        if a == 0 {
            continue;
        }
        wm_fill_a(buf, win_w, win_h, full, WmBox { x: plate.x + dx, y: plate.y, w: 1, h: plate.h }, 0xffff_ffff, a / 2);
    }
    let ty = p.band.y + p.band.h.saturating_sub(16 * ts) / 2;
    crate::hostfont::draw(buf, win_w, win_h, p.band.x + 6 * p.unit + 2, ty + 2, ts, title, 0xff10_1218);
    crate::hostfont::draw(buf, win_w, win_h, p.band.x + 6 * p.unit, ty, ts, title, 0xfff4_f0e0);
    wm_draw_back(buf, win_w, win_h, full, p.back, false);

    // hero5's 미니맵 toggle box: a dark plate in the bottom-left of the picture.
    wm_fill_a(buf, win_w, win_h, p.view, p.mini, 0x0000_0000, 170);
    let label = if minimap_on { "*미니맵 ON" } else { "*미니맵 OFF" };
    let ms = ((p.mini.h * 7 / 10) / 16).clamp(1, 6);
    let my = p.mini.y + p.mini.h.saturating_sub(16 * ms) / 2;
    crate::hostfont::draw(buf, win_w, win_h, p.mini.x + 3 * p.unit + 1, my + 1, ms, label, 0xff10_1218);
    crate::hostfont::draw(buf, win_w, win_h, p.mini.x + 3 * p.unit, my, ms, label, if minimap_on { 0xffd0_ffd8 } else { 0xffff_fff4 });
}

/// The four menu arrows, at the same seat a menu uses. Drawn over the world map
/// so the cursor is stepped with the control the player already knows.
pub fn draw_menu_dpad(buf: &mut [u32], win_w: u32, win_h: u32) {
    let Some(buttons) = menu_dpad_buttons(win_w, win_h) else { return };
    let layout = Layout {
        game: Rect::new(0, 0, win_w, win_h),
        buttons,
        gear: Rect::new(0, 0, 0, 0),
        joystick: None,
        keypad_toggle: Rect::new(0, 0, 0, 0),
    };
    draw_controls(buf, win_w, win_h, &layout, &std::collections::HashSet::new(), false, false);
}

pub fn menu_dpad_hit(win_w: u32, win_h: u32, x: f64, y: f64) -> Option<KeyCode> {
    menu_dpad_buttons(win_w, win_h).and_then(|b| hit(&b, x, y))
}

fn menu_dpad_buttons(win_w: u32, win_h: u32) -> Option<Vec<Button>> {
    let seat = crate::placement::rect_of(crate::placement::CtrlId::Stick, Mode::Menu, win_w, win_h)?;
    if seat.w == 0 || seat.h == 0 {
        return None;
    }
    let mut buttons = Vec::new();
    for arm in dpad_arms(seat) {
        buttons.push(Button {
            rect: arm.hit.square,
            wedge: Some(arm.hit),
            key: match arm.dir {
                Dir::Up => KeyCode::UP,
                Dir::Down => KeyCode::DOWN,
                Dir::Left => KeyCode::LEFT,
                Dir::Right => KeyCode::RIGHT,
            },
            label: Label::Text(""),
            dpad: true,
            face: Face::Hexagon(arm.dir, arm.draw),
        });
    }
    Some(buttons)
}

// The view's tile classes are the worker's, repeated because neither file may name the
// other (both are `include!`d into dependency-free crates). A drift would draw floor as
// wall or exits as nothing — silently — so it is a build error instead.
const _: () = assert!(
    MM_WALL == crate::guest_build::H2_MM_WALL
        && MM_FLOOR == crate::guest_build::H2_MM_FLOOR
        && MM_EXIT == crate::guest_build::H2_MM_EXIT
        && MM_CHEST == crate::guest_build::H2_MM_CHEST
);

/// Shared painter: the map (hero2: the panel IS the view, no frame since 2026-09-29).
/// `hint` = draw a "MAP" placeholder when there is no map yet (the corner panel
/// wants it; the full-screen one should stay out of the way instead). `mobs` = the
/// monster squares (hero2 only).
#[allow(clippy::too_many_arguments)]
fn draw_minimap_in(buf: &mut [u32], win_w: u32, win_h: u32, p: Rect, mm: Option<&crate::Minimap>, hint: bool, marker: Option<(f32, f32)>, mobs: &[(f32, f32)]) {
    if p.w == 0 || p.h == 0 {
        return;
    }
    let t = (p.w / 40).max(2);
    let frame = |buf: &mut [u32], a: u32| {
        blend_rect(buf, win_w, win_h, p.x, p.y, p.w, t, MM_FRAME, a);
        blend_rect(buf, win_w, win_h, p.x, p.y + p.h.saturating_sub(t), p.w, t, MM_FRAME, a);
        blend_rect(buf, win_w, win_h, p.x, p.y, t, p.h, MM_FRAME, a);
        blend_rect(buf, win_w, win_h, p.x + p.w.saturating_sub(t), p.y, t, p.h, MM_FRAME, a);
    };
    match mm {
        // ★★ hero2 (2026-09-29, player: *"hero5처럼 미니맵을 현재 내 위치로 살짝 확대되서 …
        // 확대된 미니맵의 크기는 어떤 크기의 맵을 가든 고정"* + exits as blue bars; later the
        // same day *"레퍼런스처럼 + 몬스터 점"* — no panel, monsters as red squares). The
        // whole panel is `minimap_view.rs`, the same call `wie_probe --h2-minimap` makes.
        Some(crate::Minimap::Tiles { w, h, cells }) => {
            if mm_draw_panel(buf, win_w, win_h, (p.x, p.y, p.w, p.h), u32::from(*w), u32::from(*h), cells, marker, mobs) {
                return;
            }
        }
        Some(crate::Minimap::Picture { w, h, px }) if *w > 0 && *h > 0 && px.len() == usize::from(*w) * usize::from(*h) => {
            draw_minimap_picture(buf, win_w, win_h, p, t, (u32::from(*w), u32::from(*h), px), marker);
            frame(buf, 140);
            return;
        }
        _ => {}
    }
    // No map (yet): the old panel and its hint.
    blend_rect(buf, win_w, win_h, p.x, p.y, p.w, p.h, MM_BACKDROP, 60);
    frame(buf, 140);
    if hint {
        draw_centered_text(buf, win_w, win_h, p, "MAP", LABEL);
    }
}

/// hero3's panel: the game's own map picture (two pixels per tile) fitted whole into the
/// panel, over a dark backdrop. `marker` is in TILES like hero2's.
fn draw_minimap_picture(buf: &mut [u32], win_w: u32, win_h: u32, p: Rect, t: u32, pic: (u32, u32, &[u32]), marker: Option<(f32, f32)>) {
    // ★ Translucent since 2026-08-05 (player request: "미니맵에 가려서 뒤에
    // 게임화면이 가려지는 경우가 있어" — the world must stay visible through the
    // panel). The backdrop went 120 -> 60, the frame 190 -> 140, and the map's own
    // pixels — previously OPAQUE — are blended at MAP_A, so monsters and NPCs
    // walking under the corner stay readable behind the room outline.
    const MAP_A: u32 = 150;
    let (mw, mh, px) = pic;
    blend_rect(buf, win_w, win_h, p.x, p.y, p.w, p.h, MM_BACKDROP, 60);
    // Fit + centre, integer-free nearest sampling (pixel art: no smoothing).
    let inner = t * 2;
    let (aw, ah) = (p.w.saturating_sub(inner * 2), p.h.saturating_sub(inner * 2));
    if aw == 0 || ah == 0 {
        return;
    }
    let sc = (f64::from(aw) / f64::from(mw)).min(f64::from(ah) / f64::from(mh));
    let dw = ((f64::from(mw) * sc) as u32).max(1);
    let dh = ((f64::from(mh) * sc) as u32).max(1);
    let ox = p.x + inner + (aw.saturating_sub(dw)) / 2;
    let oy = p.y + inner + (ah.saturating_sub(dh)) / 2;
    for y in 0..dh {
        let sy = y * mh / dh;
        let row = ((oy + y) * win_w) as usize;
        for x in 0..dw {
            if ox + x >= win_w || oy + y >= win_h {
                continue;
            }
            let c = px[(sy * mw + x * mw / dw) as usize];
            if c & 0x00ff_ffff != 0 {
                blend_px(buf, row + (ox + x) as usize, c, MAP_A);
            }
        }
    }

    // ★★ THE LIVE MARKER (2026-09-19). Drawn HERE, over the scaled picture, rather
    // than stamped into the pixels — and that is what makes it follow the player: a
    // baked dot would wait for the picture's refresh and snap to its 16-guest-px grid.
    // `marker` is in TILES, two picture pixels each.
    let Some((mx, my)) = marker else { return };
    if !(mx.is_finite() && my.is_finite()) {
        return;
    }
    let (mx, my) = (f64::from(mx) * 2.0, f64::from(my) * 2.0);
    let sx = f64::from(ox) + mx.clamp(0.0, f64::from(mw)) * f64::from(dw) / f64::from(mw);
    let sy = f64::from(oy) + my.clamp(0.0, f64::from(mh)) * f64::from(dh) / f64::from(mh);
    // One tile across, floored so it never vanishes on a tiny panel.
    let rad = (f64::from(dw) / f64::from(mw)).max(1.5);
    blend_disc_soft(buf, win_w, win_h, sx, sy, rad, 0xffec_2d2d, 255, (rad * 0.5).max(1.0));
}

/// Reference-style skill buttons — ROUND (player request 2026-07-30: "make the
/// 1/3/7/9/0/# keys all round"). A translucent disc with a bright anti-aliased rim
/// (gold for a skill, steel for an item, dim when empty), the decoded icon feathered
/// into the disc, and the live stack count for items. Empty slots stay dim with just
/// the hotkey character so the layout never jumps.
fn draw_skill_buttons(buf: &mut [u32], win_w: u32, win_h: u32, rects: &[Rect], slots: &[QuickSlot], pressed: Option<usize>, cooldowns: &[f64]) {
    const PLATE: u32 = 0xff10_1420;
    const RIM_SKILL: u32 = 0xffd8_aa38;
    const RIM_ITEM: u32 = 0xff9d_b4d0;
    const RIM_EMPTY: u32 = 0xff4a_5568;
    // ★ 2026-09-01: a HOST plate with nothing bound is dimmer still, and greyer than the
    // empty rim — the agreed reading for "this plate exists but cannot be pressed yet"
    // (사용자 결정: "비활성 (회색)"). It stops being inert the moment the host can bind
    // one, so this is a state, not a decoration.
    const RIM_INERT: u32 = 0xff2e_3440;
    let at = atlas::atlas();
    for (i, r) in rects.iter().enumerate() {
        let Some(slot) = slots.get(i) else {
            break;
        };
        // ⛔ Same guard as `draw_quickbar`: a zero-sized rect is a hidden plate.
        if r.w == 0 || r.h == 0 {
            continue;
        }
        let down = pressed == Some(i);
        let empty = matches!(slot.kind, SlotKind::Empty);
        let inert = crate::quickslot_inert(slot);
        let rim = if inert {
            RIM_INERT
        } else {
            match slot.kind {
                SlotKind::Skill => RIM_SKILL,
                SlotKind::Item => RIM_ITEM,
                SlotKind::Empty => RIM_EMPTY,
            }
        };
        let plate = if down { BTN_PRESSED } else { PLATE };
        // User feedback: more see-through. Empty slots almost vanish.
        let a = if inert { 40 } else if empty { 70 } else { 108 };
        let ra = if inert { 70 } else if empty { 120 } else { 185 };
        let (cx, cy, rad) = draw_round_face(buf, win_w, win_h, *r, plate, rim, a, ra);

        let icon = slot.icon.and_then(|ic| match ic {
            IconRef::Item(f) => at.item(f),
            IconRef::Skill(sheet, cell) => at.skill(sheet, cell),
            IconRef::Beast(id) => at.beast(id),
            IconRef::ItemType(t) => at.item_type(t),
        });
        match icon {
            Some(ic) => {
                // `blit_icon` fits the art to the inscribed circle and fades its
                // outer band out, so an icon that overflows dissolves at the rim
                // instead of being cut by a hard arc.
                blit_icon(buf, win_w, win_h, cx, cy, rad * 0.88, ic);
                if matches!(slot.kind, SlotKind::Item) && slot.count > 0 {
                    // Bottom-CENTRE inside the disc: the old bottom-right corner is
                    // outside a circle inscribed in the same rect.
                    let cnt = format!("{}", slot.count.min(99));
                    let cs = (r.h / 20).max(2);
                    let tw = cnt.len() as u32 * 6 * cs;
                    let ty = (cy + rad * 0.42) as u32;
                    let tx = (cx as u32).saturating_sub(tw / 2);
                    // Dim pad behind the digits so they read over bright icon art.
                    blend_disc_soft(
                        buf,
                        win_w,
                        win_h,
                        cx,
                        ty as f64 + 3.5 * f64::from(cs),
                        f64::from(tw.max(6 * cs)) * 0.72,
                        0xff05_070c,
                        150,
                        f64::from(cs) * 1.5,
                    );
                    draw_text(buf, win_w, win_h, tx, ty, cs, &cnt, LABEL);
                }
            }
            // ★ An empty slot draws NOTHING but its dim rim (2026-08-03). It used to
            // print the slot's hotkey character — 1 / 3 / 9 / 0 / # — which was honest
            // when these plates were understood as "a second way to press that number
            // key", and is a lie now that they are quickbar slots: "이제 우측에 스킬
            // 퀵바는 숫자로써의 역할이 아니야. 퀵바로써의 역할인거야."
            //
            // The hotkey itself is untouched — `quickslot_hotkey` is still what a press
            // SENDS (that mapping is baked into the game's field dispatcher). It simply
            // is not the plate's identity any more, so it is not its label.
            None => {}
        }
        // Cooldown fill LAST, over the icon: the whole face reads as recovering.
        if let Some(f) = cooldowns.get(i).copied().filter(|f| *f < 1.0) {
            draw_cooldown_overlay(buf, win_w, win_h, cx, cy, rad, f);
        }
    }
}

/// Ring every quickbar plate in gold: "the game is asking which plate — tap one".
///
/// Drawn as a THICK OUTER RING rather than a filled tint so the icon underneath stays
/// readable: the player is choosing between five things they identify by their icons,
/// and a wash over all five is exactly the wrong moment to hide them.
///
/// This is the visible half of the 퀵바등록 flow (`items::popup_view`). The other half
/// is that a tap now REGISTERS instead of activating — see `App::window_event`.
pub fn draw_pick_highlight(buf: &mut [u32], win_w: u32, win_h: u32, rects: &[Rect]) {
    const PICK: u32 = 0xffe0_9a3c; // the action disc's gold — "this is the one to touch"
    for r in rects {
        if r.w == 0 || r.h == 0 {
            continue;
        }
        let rad = f64::from(r.w.min(r.h)) / 2.0;
        let cx = f64::from(r.x) + f64::from(r.w) / 2.0;
        let cy = f64::from(r.y) + f64::from(r.h) / 2.0;
        blend_ring_soft(buf, win_w, win_h, cx, cy, rad - 1.0, (rad / 7.0).max(2.5), PICK, 255);
        blend_ring_soft(buf, win_w, win_h, cx, cy, rad * 0.74, (rad / 14.0).max(1.5), PICK, 150);
    }
}

/// Alpha-aware bilinear blit of an atlas icon into `dst` (aspect preserved,
/// centred). The disc-clipped variant is [`blit_icon`], which every key uses
/// (2026-07-30: all round); the bag tabs (`draw_bag_tabs`) are the rectangular face.
fn blit_icon_rect(buf: &mut [u32], win_w: u32, win_h: u32, dst: Rect, ic: &atlas::Icon) {
    if ic.w == 0 || ic.h == 0 || dst.w == 0 || dst.h == 0 {
        return;
    }
    let (iw, ih) = (f64::from(ic.w), f64::from(ic.h));
    let sc = (f64::from(dst.w) / iw).min(f64::from(dst.h) / ih);
    let dw = (iw * sc).round() as u32;
    let dh = (ih * sc).round() as u32;
    if dw == 0 || dh == 0 {
        return;
    }
    let ox = dst.x + (dst.w.saturating_sub(dw)) / 2;
    let oy = dst.y + (dst.h.saturating_sub(dh)) / 2;
    for py in 0..dh {
        for px in 0..dw {
            let (x, y) = (ox + px, oy + py);
            if x >= win_w || y >= win_h {
                continue;
            }
            let sx = (f64::from(px) + 0.5) / f64::from(dw) * iw - 0.5;
            let sy = (f64::from(py) + 0.5) / f64::from(dh) * ih - 0.5;
            if let Some(c) = sample_bilinear(ic, sx, sy) {
                buf[(y * win_w + x) as usize] = c;
            }
        }
    }
}

/// Scale a decoded atlas icon into the button's inscribed circle with alpha-aware
/// BILINEAR sampling (not nearest). The old nearest path stretched a ~15px icon to
/// a big disc at a non-integer factor, so source texels mapped to uneven 4/5px
/// blocks — the "뭉개짐" (mushy/blocky) look. Bilinear gives a smooth, H5-style
/// icon; edge texels blend only their OPAQUE neighbours (weight-normalised) so the
/// 1-bit-alpha silhouette stays clean with no dark halo, and the disc colour still
/// shows through transparent areas.
fn blit_icon(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, rad: f64, ic: &atlas::Icon) {
    if ic.w == 0 || ic.h == 0 || rad < 3.0 {
        return;
    }
    let box_side = rad * 1.5; // icon box; slightly < diameter so a rim shows
    let (iw, ih) = (f64::from(ic.w), f64::from(ic.h));
    if box_side < 1.0 || iw < 1.0 || ih < 1.0 {
        return;
    }
    // Fit the icon into the box preserving aspect (skill cells and item frames
    // are both 15x15), so nothing stretches; then centre it on the disc. The
    // cells are full-bleed square art, so the disc clip below is what gives the
    // round H5 look — no colour keying is applied to the game's own pixels.
    let sc = (box_side / iw).min(box_side / ih);
    let dw = (iw * sc).round() as u32;
    let dh = (ih * sc).round() as u32;
    if dw == 0 || dh == 0 {
        return;
    }
    let dx0 = (cx - dw as f64 / 2.0) as i64;
    let dy0 = (cy - dh as f64 / 2.0) as i64;
    // Player request (2026-07-30): "it's fine if the icon is clipped by the circle —
    // fade the icon into the circle so it looks natural". So instead of a hard
    // `d > rad -> skip` clip (which leaves a staircase arc across the artwork), the
    // icon's alpha ramps to 0 over the outer `feather` band. Icons that fit well
    // inside the disc are untouched; ones that overflow dissolve at the rim.
    let feather = (rad * 0.20).max(2.0);
    let inner = (rad - feather).max(0.0);
    // ★ 2026-08-05: the icon's OWN rectangular border also gets a gradient. The
    // source cells are full-bleed 15x15 squares, and a square ending in a hard edge
    // inside a round button is the "사각형의 스킬아이콘 ... 살짝 어색한" look the
    // player reported. Alpha now ramps up over the outer `edge_band` of the SQUARE
    // as well (per-axis distance to the nearest icon edge), so the artwork melts
    // into the disc on all four sides instead of only where the circle clips it.
    let edge_band = (f64::from(dw.min(dh)) * 0.16).max(1.5);
    for oy in 0..dh {
        for ox in 0..dw {
            let px = dx0 + ox as i64;
            let py = dy0 + oy as i64;
            if px < 0 || py < 0 || px as u32 >= win_w || py as u32 >= win_h {
                continue;
            }
            let (ddx, ddy) = (px as f64 + 0.5 - cx, py as f64 + 0.5 - cy);
            let d = (ddx * ddx + ddy * ddy).sqrt();
            if d >= rad {
                continue;
            }
            let cov = if d <= inner { 1.0 } else { (rad - d) / feather };
            let mut cov = cov * cov * (3.0 - 2.0 * cov); // smoothstep
            // Distance (px) from the destination pixel's centre to the nearest edge
            // of the icon rectangle, smoothstepped over `edge_band`.
            let ex = (ox as f64 + 0.5).min(f64::from(dw) - ox as f64 - 0.5);
            let ey = (oy as f64 + 0.5).min(f64::from(dh) - oy as f64 - 0.5);
            let e = (ex.min(ey) / edge_band).clamp(0.0, 1.0);
            cov *= e * e * (3.0 - 2.0 * e);
            // Dest pixel centre -> source coords (half-pixel corrected).
            let fx = (ox as f64 + 0.5) * iw / dw as f64 - 0.5;
            let fy = (oy as f64 + 0.5) * ih / dh as f64 - 0.5;
            if let Some(c) = sample_bilinear(ic, fx, fy) {
                let a = (255.0 * cov) as u32;
                if a >= 254 {
                    buf[(py as u32 * win_w + px as u32) as usize] = 0xff00_0000 | (c & 0x00ff_ffff);
                } else if a > 0 {
                    blend_px(buf, (py as u32 * win_w + px as u32) as usize, c, a);
                }
            }
        }
    }
}

/// Alpha-aware bilinear sample of `ic` at fractional source coords `(fx, fy)`.
/// Only opaque (alpha != 0) texels contribute, weighted by bilinear coverage and
/// re-normalised by the opaque weight; returns `None` (transparent) when opaque
/// coverage < half so the icon keeps a crisp silhouette without edge darkening.
fn sample_bilinear(ic: &atlas::Icon, fx: f64, fy: f64) -> Option<u32> {
    let (w, h) = (i64::from(ic.w), i64::from(ic.h));
    let x0 = fx.floor();
    let y0 = fy.floor();
    let (tx, ty) = (fx - x0, fy - y0);
    let (x0i, y0i) = (x0 as i64, y0 as i64);
    let texel = |xi: i64, yi: i64| -> Option<(f64, f64, f64)> {
        if xi < 0 || yi < 0 || xi >= w || yi >= h {
            return None;
        }
        let c = ic.px[(yi * w + xi) as usize];
        if (c >> 24) == 0 {
            return None;
        }
        Some((f64::from((c >> 16) & 0xff), f64::from((c >> 8) & 0xff), f64::from(c & 0xff)))
    };
    let corners = [
        (texel(x0i, y0i), (1.0 - tx) * (1.0 - ty)),
        (texel(x0i + 1, y0i), tx * (1.0 - ty)),
        (texel(x0i, y0i + 1), (1.0 - tx) * ty),
        (texel(x0i + 1, y0i + 1), tx * ty),
    ];
    let (mut r, mut g, mut b, mut wsum) = (0.0, 0.0, 0.0, 0.0);
    for (c, wgt) in corners {
        if let Some((cr, cg, cb)) = c {
            r += cr * wgt;
            g += cg * wgt;
            b += cb * wgt;
            wsum += wgt;
        }
    }
    if wsum < 0.5 {
        return None;
    }
    let r = (r / wsum).round() as u32;
    let g = (g / wsum).round() as u32;
    let b = (b / wsum).round() as u32;
    Some((r << 16) | (g << 8) | b)
}

/// Paint over the game's own field HUD so our custom HUD replaces it: the
/// bottom full-width strip (quickbar window + HP/SP/EXP bars) and the top-left
/// 2-skill box. Regions are content-frame fractions (240x320) mapped into the
/// game rect; opaque black to blend with the letterbox. Device-tunable.
pub fn mask_game_hud(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect) {
    let mask = 0xff00_0000u32;
    let (lw, lh) = crate::logical();
    // ★★ 2026-09-09 — hero2 takes this branch and hero3 does not, for a measured
    // reason: hero2 reserves rows 194..217 for its own HUD and draws no world there,
    // so painting them out costs nothing. hero3's box sits ON the world, which is why
    // `guest_hud_band()` answers `None` for it and the code below is left untouched.
    // Full logical WIDTH, not just the art's own box: outside the box those rows are
    // already black, and masking the whole band is what lets the host own the strip
    // without a seam appearing when the game's art is narrower than the screen.
    // ★ A build that RESERVES rows never gets here: `App::guest_hud_mask_on` is false for
    // it because the reservation is dropped at present time instead of painted over.
    // ★ Nor one whose painter is switched OFF (`hud_bottom_patch`, 2026-09-18): the band
    // rows hold live world there, and blacking them out would be the dead strip again.
    if let Some((band_y, band_h)) = crate::guest_build::guest_hud_band()
        .filter(|_| crate::guest_build::hud_reserved_rows() == 0 && crate::guest_build::hud_bottom_patch().is_none())
    {
        if game.w == 0 || game.h == 0 || lh == 0 {
            return;
        }
        let y = game.y + game.h * band_y / lh;
        let h = (game.h * band_h / lh).max(1);
        // Clip to the game rect: a rounding-up height must not bleed into the letterbox
        // (harmless there, but `fill` is cheaper than reasoning about it).
        let h = h.min((game.y + game.h).saturating_sub(y));
        fill(buf, win_w, win_h, game.x, y, game.w, h, mask);
        return;
    }
    if crate::immersive_active() {
        // Measured with wie_probe at logical widths 240/300/426/514/640: the game's
        // own field HUD is a FIXED **176 x 24 logical px** box, horizontally
        // CENTRED and bottom-anchored (the run length never changed and its centre
        // tracked the screen centre to within 1 px). So mask exactly that box —
        // masking the whole width would black out live world pixels either side.
        const HUD_W: u32 = 176;
        const HUD_H: u32 = 24;
        // ★ 2026-07-30: in immersive mode we no longer mask anything.
        //
        // The game paints its own HUD (HP/SP/EXP bars + the 5 quickbar cells) into a
        // fixed 176x24 guest box, bottom-centre. Hiding it means painting that box
        // OPAQUE — nothing can restore the world pixels underneath — and the player
        // reported that black band as wrong twice: first as a wide translucent tint,
        // then as the box itself. Since our EXP bar has moved up to the HP/SP stack
        // there is nothing of ours left in there, so the honest option is to leave
        // the game's own strip visible (it is the original art, and it doubles as the
        // quickbar readout) instead of covering the world with black.
        //
        // `game_hud_box` still reports the region for layout code that must avoid it.
        let _ = (mask, HUD_W, HUD_H, lw, lh);
        return;
    }
    // Bottom HUD strip: content y >= 288 (of 320).
    let strip_y = game.y + game.h * 288 / 320;
    fill(buf, win_w, win_h, game.x, strip_y, game.w, (game.y + game.h).saturating_sub(strip_y), mask);
    // Top-left 2-skill box: content x<48, y<28.
    fill(buf, win_w, win_h, game.x, game.y, game.w * 48 / 240, game.h * 28 / 320, mask);
}

/// Original content width the game's out-of-world screens are laid out for.
const SHELL_ART_W: u32 = CONTENT_W; // hero2: 176 (hero3 was 240)

/// Letterbox the title / slot-select screens.
///
/// Those screens are 240-px art. Given a wider framebuffer the game fills the extra
/// width by MIRRORING its background outwards (measured on a device capture: the
/// frame is non-black across the full logical width and the two halves mirror about
/// the centre) — so nothing is missing, but the edges show headless copies of the
/// artwork and read as "the title is cut off" (player-reported 2026-07-30).
/// Blacking those bands out makes it a clean centred title instead.
///
/// Only the shell state is affected; the field and the game's windows are untouched.
pub fn letterbox_shell(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect) {
    if !crate::immersive_active() || game.w == 0 {
        return;
    }
    let (lw, _) = crate::logical();
    if lw <= SHELL_ART_W {
        return;
    }
    let art = game.w * SHELL_ART_W / lw;
    let side = (game.w - art) / 2;
    if side == 0 {
        return;
    }
    // FEATHERED gradient that starts INSIDE the artwork.
    //
    // What the player is actually seeing (2026-07-30, third pass): the hard edge is
    // the ARTWORK'S OWN. The title screen is authored for a 240-px-wide portrait
    // screen; landscape has width to spare, so the game paints the art in the middle
    // and its rectangle simply STOPS — a razor-straight vertical edge. A gradient
    // that only starts outside that edge cannot soften it, because the discontinuity
    // is at the edge itself.
    //
    // So the ramp straddles the boundary: alpha is already non-zero a little way
    // INSIDE the art (`FEATHER`), reaching full dark at the screen edge. The art's
    // own outer strip fades out instead of ending, so there is no line anywhere.
    // Eased (t^2) so the darkening is imperceptible where it begins.
    // Measured on device (row-luminance profile of a real capture): OUTSIDE the art
    // the game's own output is already black at most rows. So darkening the outside
    // achieves nothing — the visible razor edge is the ARTWORK'S OWN last column
    // meeting that black. The ramp therefore has to run the other way: transparent
    // some way INSIDE the art, reaching FULL dark AT (and just past) its edge, so the
    // art's outer strip dissolves into the surrounding black.
    // `f_in` = how far the fade reaches into the art, `f_out` = how far past the
    // nominal edge it takes to become fully opaque. The overshoot matters: the art is
    // a couple of logical px wider than the nominal 240, and stopping exactly at 240
    // would itself cut a fresh hard edge.
    let f_in = (art * 10 / 100).max(2);
    let f_out = (art * 4 / 100).max(1);
    let art_left = game.x + side;
    let art_right = game.x + side + art;
    let span = f_in + f_out;
    // Left: alpha 0 at art_left + f_in -> full at art_left - f_out, then solid.
    edge_ramp(buf, win_w, win_h, game, art_left.saturating_sub(f_out), art_left + f_in, false, span);
    fill(buf, win_w, win_h, game.x, game.y, side.saturating_sub(f_out), game.h, 0xff00_0000);
    // Right: mirrored.
    edge_ramp(buf, win_w, win_h, game, art_right.saturating_sub(f_in), art_right + f_out, true, span);
    let solid_x = art_right + f_out;
    fill(
        buf,
        win_w,
        win_h,
        solid_x,
        game.y,
        (game.x + game.w).saturating_sub(solid_x),
        game.h,
        0xff00_0000,
    );
}

/// One column-wise alpha ramp over `[from, to)`, eased `t^2`, black.
/// `invert` = the dark end is on the RIGHT (mirrored side).
#[allow(clippy::too_many_arguments)]
fn edge_ramp(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect, from: u32, to: u32, invert: bool, span: u32) {
    let span = f64::from(span.max(1));
    for x in from..to.min(game.x + game.w) {
        let d = if invert { x.saturating_sub(from) } else { to.saturating_sub(x) };
        let t = (f64::from(d) / span).clamp(0.0, 1.0);
        let a = (255.0 * t * t) as u32;
        if a > 0 {
            blend_rect(buf, win_w, win_h, x, game.y, 1, game.h, 0xff00_0000, a);
        }
    }
}

// hero2's STATUS HUD (2026-09-29) — the hero5-style portrait ring · HP/SP bars · level
// plate, and the EXP bar at the bottom centre. Its own dependency-free file so
// `wie_probe --h2-status` draws the identical block headless.
include!("hud_view.rs");

/// The status block's geometry for this game rect (`hud_view::HvGeom` — one pixel of it is
/// the game's own pixel, so it scales with the game, not with the window).
fn status_geom(game: Rect) -> HvGeom {
    let (lw, lh) = crate::logical();
    HvGeom::new((game.x, game.y, game.w, game.h), lw, lh)
}

/// A `hud_view` `(x, y, w, h)` as a [`Rect`].
fn status_rect(r: (u32, u32, u32, u32)) -> Rect {
    Rect::new(r.0, r.1, r.2, r.3)
}

/// Geometry of the player-gauge block — since 2026-09-29 hero5's (player: *"좌측 상단 현재
/// 캐릭터 모습(외형), HP,MP,경험치바 … 꾸미고싶어"*, the reference being 영웅서기5's own HUD):
/// the portrait RING, the HP/SP bars beside it, the level plate under it. `hud_view.rs`
/// owns the numbers; this is the same block in `Rect`s for the callers that place things
/// around it. ONE source of truth, so the controls that sit next to it cannot drift on top
/// of it — which is exactly what happened on the first device build.
pub struct StatsBlock {
    /// The HP and SP bars, as they show right of the ring. (EXP left the block: it is the
    /// thin bar at the bottom centre now — [`draw_exp_bar`].)
    pub bars: Rect,
}

/// Player-gauge block: extreme TOP-LEFT with a small margin (player request 2026-07-30 —
/// an earlier version pushed it right, to dodge the game's own buff band, and it read as
/// "floating in the middle"). ⚠ On hero3 this block drove the game's buff band x
/// ([`band_x_shift`]); hero2 has no such poke (`band_x_sites` is empty for it).
pub fn stats_block(game: Rect) -> StatsBlock {
    StatsBlock { bars: status_rect(status_geom(game).bars()) }
}

/// The gauge block as a TAP TARGET: tapping the level plate / HP-SP-EXP bars opens
/// the game's main menu (player request 2026-08-03 — "제일 좌측 상단에 Lv과 HP
/// 표시하는곳 클릭하면 메뉴가 열리게").
///
/// It sends `CLEAR`, which IS the game's open-main-menu key on the field (and its
/// back key everywhere else) — device-probed, see `CUSTOM_HUD.md` §키 역할 실측. So
/// this is not a new capability, it is a second, bigger target for a key the CLR
/// button already sends: the block is ~490x82 px against a ~98 px disc, and it is
/// where a player's eye already is when they think "menu".
///
/// GROWN by EXACTLY ONE pad on every side: the drawn bars are thin, and a target you
/// have to hit precisely is worse than no target. It stays a thin top-left strip, so
/// it does not eat the area a steering thumb uses.
///
/// ★ ONE pad, not two, and this is a correctness bound rather than taste. This zone
/// is hit-tested BEFORE `layout.buttons` (app.rs), so every pixel it owns is a pixel
/// the control underneath never gets — and the only thing keeping a control out from
/// under it is [`crate::placement::obstacles_fixed`], which reserves the UNGROWN
/// [`stats_block_box`]. `accepts` there admits a candidate as soon as
/// `cand.grown(pad)` misses that rect, i.e. a control is legal at exactly `pad` of
/// clearance. With half-open intersection, growing by `pad` puts this zone's edge
/// precisely ON the nearest legal control edge and therefore never inside it; a
/// second pad reaches `pad` past it.
///
/// That is not theoretical: a `pad`-wide annulus of legal-but-swallowed placements
/// was found by an adversarial review of this change (2026-08-03) and brute-force
/// confirmed — over a thousand accepted positions for the OK disc alone at
/// 2340x1080, all of them ones the editor's "slide along the obstacle" drag actively
/// encourages, where tapping the disc's outer sliver opened the main menu instead of
/// attacking. `the_gauge_tap_zone_never_shadows_a_legal_control` in the selftest pins
/// the rule so the growth cannot creep back.
///
/// ★ Field only, and `app.rs` treats it as a deliberate TAP (down and up inside,
/// little movement) rather than a button — the same rule the minimap panel uses, and
/// for the same reason: the immersive joystick zone is the whole window, so a region
/// that fired on touch-down would steal the start of a walk.
pub fn stats_tap_box(game: Rect) -> Rect {
    // The same `pad` the resolver uses: the zone only exists in landscape immersive,
    // where `game` IS the whole window, so the two agree by construction.
    let pad = pad_of(game.w.max(1), game.h.max(1));
    stats_block_box(game).grown(pad)
}

// ── unspent stat / skill point badges ────────────────────────────────────────
//
// ★ NOT invented art: these are the GAME'S OWN two badges, lifted pixel-for-pixel
// out of JAR `/comm/ui_bm` **frame 6** (a 14x7 strip = two 7x7 cells). The game
// draws them at the end of its bottom-HUD block — `0x14a372` blits cell srcX 0 when
// `(s16)*(CHAR+0x18a) > 0` and `0x14a32e` blits cell srcX 7 when `*(CHAR+0x18c) != 0`
// — both at `baseX + 0x99 / +0xa2, baseY - 1`, i.e. the RIGHT END of that block, just
// above it. That block is exactly what `items::hud_bottom_patch` branches over, so
// the badges disappeared with it; this redraws them beside our own gauges, which is
// the same place relative to the HP/SP bars.
//
// Index 0 of a `_bm` palette is the 0xf81f transparent sentinel, which is why the
// four corners are 0 here.
/// Red `+` = unspent STAT points. `/comm/ui_bm` frame 6, cell at srcX 0.
/// Rim 0x940000, fill 0xff0000, glyph 0xfffaff.
const POINT_BADGE_STAT: [u32; 49] = [
    0x0000_0000, 0xff94_0000, 0xff94_0000, 0xff94_0000, 0xff94_0000, 0xff94_0000, 0x0000_0000,
    0xff94_0000, 0xffff_0000, 0xffff_0000, 0xffff_faff, 0xffff_0000, 0xffff_0000, 0xff94_0000,
    0xff94_0000, 0xffff_0000, 0xffff_0000, 0xffff_faff, 0xffff_0000, 0xffff_0000, 0xff94_0000,
    0xff94_0000, 0xffff_faff, 0xffff_faff, 0xffff_faff, 0xffff_faff, 0xffff_faff, 0xff94_0000,
    0xff94_0000, 0xffff_0000, 0xffff_0000, 0xffff_faff, 0xffff_0000, 0xffff_0000, 0xff94_0000,
    0xff94_0000, 0xffff_0000, 0xffff_0000, 0xffff_faff, 0xffff_0000, 0xffff_0000, 0xff94_0000,
    0x0000_0000, 0xff94_0000, 0xff94_0000, 0xff94_0000, 0xff94_0000, 0xff94_0000, 0x0000_0000,
];
/// Blue `+` = unspent SKILL points. `/comm/ui_bm` frame 6, cell at srcX 7.
/// Rim 0x000094, fill 0x0061ff, glyph 0xfffaff.
const POINT_BADGE_SKILL: [u32; 49] = [
    0x0000_0000, 0xff00_0094, 0xff00_0094, 0xff00_0094, 0xff00_0094, 0xff00_0094, 0x0000_0000,
    0xff00_0094, 0xff00_61ff, 0xff00_61ff, 0xffff_faff, 0xff00_61ff, 0xff00_61ff, 0xff00_0094,
    0xff00_0094, 0xff00_61ff, 0xff00_61ff, 0xffff_faff, 0xff00_61ff, 0xff00_61ff, 0xff00_0094,
    0xff00_0094, 0xffff_faff, 0xffff_faff, 0xffff_faff, 0xffff_faff, 0xffff_faff, 0xff00_0094,
    0xff00_0094, 0xff00_61ff, 0xff00_61ff, 0xffff_faff, 0xff00_61ff, 0xff00_61ff, 0xff00_0094,
    0xff00_0094, 0xff00_61ff, 0xff00_61ff, 0xffff_faff, 0xff00_61ff, 0xff00_61ff, 0xff00_0094,
    0x0000_0000, 0xff00_0094, 0xff00_0094, 0xff00_0094, 0xff00_0094, 0xff00_0094, 0x0000_0000,
];
/// Source cell size of both badges (`ui_bm` frame 6 is 14x7 = 2 x 7x7).
pub const BADGE_SRC: u32 = 7;

// ★★ 2026-10-20: `TOAST_MS` and `draw_toast` are gone — the host's status lines are notices in the game's own box
//   at the left now (`Notice` / `draw_notices`, the pickups' stack).

/// A short status line, centred low on the game rect so it sits clear of the gauges at
/// the top-left and the action ring at the bottom-right.
///
/// ★ KOREAN AND SMALLER since 2026-08-08 (player: "Saved! 가 아니라 더 작은 글씨체로
/// 저장완료"). Drawn with [`draw_mixed_text`] rather than `draw_centered_text_in`, for two
/// reasons: the 5x7 ASCII font has no Hangul, and the old width used `text.len()`, which is
/// BYTES — a 4-syllable Korean string is 12 of those, so the panel would have come out three
/// times too wide.
///
/// The scale is the same derivation as [`draw_buff_lines`], which is the Korean size already
/// on screen in the HUD; on a 1080-tall window that is a 36 px cell against the 56 px the
/// ASCII toast used, i.e. genuinely smaller as asked.
// ── the inventory PAGE STRIP (#13 step 4, 2026-09-21) ────────────────────────────
//
// Five bag tabs down the margin beside the game's own inventory panel — "가방 5개".
// The panel is the game's (120 × 117 at `bw.a/b`, framebuffer px); the strip is ours, in
// window px, laid out from the panel's framebuffer origin through the same mapping the
// game frame is shown with. The icon is the 재료가방 sprite turned 90° left (user's
// choice, baked into the atlas — `Atlas::bag`); the tabs differ by border and ink only,
// plus a digit so five identical bags can be told apart.

/// Tab side in framebuffer px, and the pitch between tabs — the OUTSIDE strip, used when
/// the guest grid could not be shifted.
const BAG_TAB_FB: u32 = 16;
const BAG_TAB_PITCH_FB: u32 = 20;
/// The strip's gap from the panel's edge, and its top offset inside the panel.
const BAG_TAB_GAP_FB: u32 = 4;
const BAG_TAB_TOP_FB: u32 = 14;

/// ★ #14: the INSIDE rail's geometry, once [`crate::guest_build::inventory_grid_shift`]
/// has moved the game's own 28 cells 7 px right (user, 2026-09-21: *"왼쪽 영역 가방5개.
/// 그리고 그 바로 옆에 28칸"*).
///
/// The five tabs stand beside the four grid rows and nowhere else: the panel's left
/// margin is free only over `INV_CELL_Y0 .. + 4·INV_CELL_PITCH`, because the 소지금 bar
/// above and the 재료가방 label below both start at the panel's own left edge. 60 px for
/// five tabs is a 12 px pitch — which is why the tab is 12 wide and 11 tall here instead
/// of 16 square. That is the price of being inside, and it is the whole of it.
fn bag_rail_geometry(cell_y0: u32) -> (u32, u32, u32, u32, u32) {
    use crate::guest_build::Hero2Java as H;
    let band = H::INV_CELL_PITCH * 4;
    let pitch = band / 5;
    (H::BAG_RAIL_X, cell_y0, H::BAG_RAIL_W, pitch, pitch.saturating_sub(1))
}

/// Where the tabs are, in WINDOW px: `game` is the game rect, `(lw, lh)` the framebuffer
/// size shown in it starting at row `y0`, `panel` the panel's framebuffer origin.
///
/// INSIDE the panel, beside the cells, when the grid shift is in (`shift > 0`); otherwise
/// the old strip left of the panel (or right of it when there is no room on the left).
/// ★ 2026-10-09: `cell_y0` = the window's first cell row (the inventory's 31, an item picker's 23 — `BagPageView`).
pub fn bag_tab_rects(game: Rect, lw: u32, lh: u32, y0: u32, panel: (u32, u32), pages: u8, shift: u32, cell_y0: u32) -> Vec<Rect> {
    if game.w == 0 || game.h == 0 || lw == 0 || lh == 0 {
        return Vec::new();
    }
    let need = BAG_TAB_FB + BAG_TAB_GAP_FB;
    let (fx, top, side_w, pitch, side_h) = if shift > 0 {
        let (rx, ry, w, pitch, h) = bag_rail_geometry(cell_y0);
        (panel.0 + rx, ry, w, pitch, h)
    } else {
        let fx = if panel.0 >= need { panel.0 - need } else { panel.0 + crate::guest_build::Hero2Java::PANEL_W + BAG_TAB_GAP_FB };
        (fx, BAG_TAB_TOP_FB, BAG_TAB_FB, BAG_TAB_PITCH_FB, BAG_TAB_FB)
    };
    let sx = f64::from(game.w) / f64::from(lw);
    let sy = f64::from(game.h) / f64::from(lh);
    let to_win = |x: u32, y: u32| -> (u32, u32) {
        let wx = f64::from(game.x) + f64::from(x) * sx;
        let wy = f64::from(game.y) + f64::from(y.saturating_sub(y0)) * sy;
        (wx.round() as u32, wy.round() as u32)
    };
    (0..pages)
        .map(|i| {
            let fy = panel.1 + top + u32::from(i) * pitch;
            let (x0, y0w) = to_win(fx, fy);
            let (x1, y1) = to_win(fx + side_w, fy + side_h);
            Rect::new(x0, y0w, x1.saturating_sub(x0).max(1), y1.saturating_sub(y0w).max(1))
        })
        .collect()
}

/// The tab under `(x, y)`, if any.
pub fn bag_tab_hit(rects: &[Rect], x: f64, y: f64) -> Option<u8> {
    rects.iter().position(|r| r.contains(x, y)).map(|i| i as u8)
}

/// ★ A FRAMEBUFFER rect `(x, y, w, h)` as the WINDOW pixels that show it — the exact inverse
/// of the game blit's nearest-neighbour map (`App::redraw`, `app::sx_lut`: window column `dx`
/// samples framebuffer column `⌊dx · lw / game.w⌋`, row `dy` samples `y0 + ⌊dy · lh / game.h⌋`),
/// so framebuffer column `s` is drawn at `⌈s · game.w / lw⌉ .. ⌈(s + 1) · game.w / lw⌉`. No more
/// and no fewer pixels than those that sample inside the rect: an edge that has to meet one of
/// the game's own edges (the page strip stopping at the game's box) meets it exactly, where the
/// `round()` the tabs are placed with would be a pixel out either way.
pub fn fb_rect_to_win(game: Rect, lw: u32, lh: u32, y0: u32, (x, y, w, h): (u32, u32, u32, u32)) -> Option<Rect> {
    if game.w == 0 || game.h == 0 || lw == 0 || lh == 0 {
        return None;
    }
    // ⌈s · dst / src⌉, in u64 so no window size can wrap it.
    let up = |s: u32, dst: u32, src: u32| (u64::from(s) * u64::from(dst)).div_ceil(u64::from(src)) as u32;
    let (fx0, fx1) = (x.min(lw), x.saturating_add(w).min(lw));
    let shown = |fy: u32| fy.clamp(y0, y0 + lh) - y0;
    let (fy0, fy1) = (shown(y), shown(y.saturating_add(h)));
    let (wx0, wx1) = (up(fx0, game.w, lw), up(fx1, game.w, lw));
    let (wy0, wy1) = (up(fy0, game.h, lh), up(fy1, game.h, lh));
    (wx0 < wx1 && wy0 < wy1).then(|| Rect::new(game.x + wx0, game.y + wy0, wx1 - wx0, wy1 - wy0))
}

/// The pixels of `r` (inside a `win_w`-wide buffer), row by row; `None` if any row is not
/// in the buffer.
fn copy_out(buf: &[u32], win_w: u32, r: Rect) -> Option<Vec<u32>> {
    let mut out = Vec::with_capacity((r.w * r.h) as usize);
    for y in r.y..r.y + r.h {
        let s = (y * win_w + r.x) as usize;
        out.extend_from_slice(buf.get(s..s + r.w as usize)?);
    }
    Some(out)
}

/// Put back what [`copy_out`] took.
fn copy_in(buf: &mut [u32], win_w: u32, r: Rect, px: &[u32]) {
    for (row, y) in px.chunks_exact(r.w.max(1) as usize).zip(r.y..r.y + r.h) {
        let s = (y * win_w + r.x) as usize;
        if let Some(dst) = buf.get_mut(s..s + row.len()) {
            dst.copy_from_slice(row);
        }
    }
}

/// Paint the tabs: the game's window chrome (`H2_*`), the selected tab in the game's
/// emphasis ink with full-bright art, the others dimmed. `on_top` = the window is the
/// top of the chain and the strip takes taps; under a popup it is drawn dimmer still.
///
/// ★ `cover` (2026-09-29, user: *"가방에서 계정창고로 넣을떄나 뺄때 등 팝업이 왼쪽 가방5개
/// 리스트에 가려지고있어"*): the game's box over the window, in window px. The box is ABOVE
/// the strip — the 117-wide family (the 개수 box, 창고에 넣었습니다, 예/아니오) spans the
/// panel's whole width, rail included — so what the frame holds where a tab meets it is kept
/// aside first and put back after: the primitives stay unclipped, and the box comes out
/// exactly as the game painted it, with the tabs showing around it like the rest of the panel.
pub fn draw_bag_tabs(buf: &mut [u32], win_w: u32, win_h: u32, rects: &[Rect], page: u8, on_top: bool, cover: Option<Rect>) {
    let kept: Vec<(Rect, Vec<u32>)> = cover.map_or_else(Vec::new, |c| {
        rects
            .iter()
            .filter_map(|r| r.overlap(&c, win_w, win_h))
            .filter_map(|o| copy_out(buf, win_w, o).map(|px| (o, px)))
            .collect()
    });
    draw_bag_tab_faces(buf, win_w, win_h, rects, page, on_top);
    for (o, px) in &kept {
        copy_in(buf, win_w, *o, px);
    }
}

/// ★ 2026-10-02 (user: *"마지막 5번쨰가방에서 칸 몇개를 지워야해. 현재는 28 * 5(가방5개) 로 보여져서 140칸처럼
/// 보여."*): paint the cells a page does not have as the panel — the body colour the game shows between its cells
/// ([`H2_BODY`] = `#6a5b44`, measured on the frame), so the 13 black squares the painter draws past 127 are no cells
/// at all. `rects` = their window px (`App::bag_hidden_cell_rects`); `cover` = the game's box over the window, kept
/// as the game painted it, as [`draw_bag_tabs`] keeps it.
pub fn draw_bag_hidden_cells(buf: &mut [u32], win_w: u32, win_h: u32, rects: &[Rect], cover: Option<Rect>) {
    if rects.is_empty() {
        return;
    }
    let kept: Vec<(Rect, Vec<u32>)> = cover.map_or_else(Vec::new, |c| {
        rects
            .iter()
            .filter_map(|r| r.overlap(&c, win_w, win_h))
            .filter_map(|o| copy_out(buf, win_w, o).map(|px| (o, px)))
            .collect()
    });
    for r in rects {
        fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, H2_BODY);
    }
    for (o, px) in &kept {
        copy_in(buf, win_w, *o, px);
    }
}

fn draw_bag_tab_faces(buf: &mut [u32], win_w: u32, win_h: u32, rects: &[Rect], page: u8, on_top: bool) {
    let icon = atlas::atlas().bag();
    for (i, r) in rects.iter().enumerate() {
        let selected = i as u8 == page;
        // Frame: outer edge, bevel, body — the panel's own three-line border.
        fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, if selected { H2_GOLD } else { H2_EDGE });
        let b = (r.w.min(r.h) / 12).max(1);
        if r.w > 2 * b && r.h > 2 * b {
            fill(buf, win_w, win_h, r.x + b, r.y + b, r.w - 2 * b, r.h - 2 * b, if selected { H2_BEVEL } else { H2_BODY });
        }
        if r.w > 4 * b && r.h > 4 * b {
            fill(buf, win_w, win_h, r.x + 2 * b, r.y + 2 * b, r.w - 4 * b, r.h - 4 * b, H2_BODY);
        }
        if let Some(ic) = icon {
            let pad = (r.w.min(r.h) / 8).max(1);
            if r.w > 2 * pad && r.h > 2 * pad {
                blit_icon_rect(buf, win_w, win_h, Rect::new(r.x + pad, r.y + pad, r.w - 2 * pad, r.h - 2 * pad), ic);
            }
        }
        // The page number, top-left, in the game's 5x7 ink.
        let scale = (r.w.min(r.h) / 20).max(1);
        let digit = [b'1' + i as u8];
        draw_text(buf, win_w, win_h, r.x + 2 * b, r.y + 2 * b, scale, std::str::from_utf8(&digit).unwrap_or("?"), if selected { H2_INK } else { H2_INK_DIM });
        // Dim what is not selected, and everything while the strip is not tappable.
        let dim = match (selected, on_top) {
            (true, true) => 0,
            (false, true) => 64,
            (true, false) => 48,
            (false, false) => 112,
        };
        if dim > 0 {
            let x1 = (r.x + r.w).min(win_w);
            let y1 = (r.y + r.h).min(win_h);
            for yy in r.y..y1 {
                for xx in r.x..x1 {
                    blend_px(buf, (yy * win_w + xx) as usize, 0xff00_0000, dim);
                }
            }
        }
    }
}

#[cfg(test)]
mod bag_cover_tests {
    use super::*;

    /// The framebuffer pixel a window pixel shows, the way the game blit samples it
    /// (`App::redraw`): columns through the blit's own `sx_lut`, rows as `y0 + dy·lh / game.h`.
    fn sampled(game: Rect, lw: u32, lh: u32, y0: u32) -> impl Fn(u32, u32) -> (u32, u32) {
        let lut = crate::app::sx_lut(game.w, lw);
        move |dx, dy| (lut[(dx - game.x) as usize], y0 + (dy - game.y) * lh / game.h)
    }

    /// ★ The cover is exactly the window pixels whose sample lands inside the box — no more
    /// (a tab would lose a sliver next to the box), no fewer (a sliver of tab would still sit
    /// on the box's edge, the very bug). Brute force over every pixel of the game rect, for
    /// phone-sized portrait windows, a landscape one, a framebuffer shown from a row offset,
    /// and rects on the edges and outside the rows that are shown.
    #[test]
    fn the_cover_is_exactly_the_pixels_that_show_the_box() {
        let cases = [
            (Rect::new(0, 297, 1080, 1350), 176, 220, 0),
            (Rect::new(0, 184, 1440, 1800), 176, 220, 0),
            (Rect::new(118, 0, 2104, 1080), 452, 232, 0),
            (Rect::new(3, 5, 500, 700), 176, 246, 26),
        ];
        let boxes = [(29, 82, 117, 56), (29, 99, 117, 22), (51, 83, 74, 54), (0, 0, 5, 5), (170, 200, 6, 20), (100, 240, 10, 10)];
        for (game, lw, lh, y0) in cases {
            let at = sampled(game, lw, lh, y0);
            for fb in boxes {
                let r = fb_rect_to_win(game, lw, lh, y0, fb);
                let mut shown = 0u64;
                for dy in game.y..game.y + game.h {
                    for dx in game.x..game.x + game.w {
                        let (sx, sy) = at(dx, dy);
                        let in_box = sx >= fb.0 && sx < fb.0 + fb.2 && sy >= fb.1 && sy < fb.1 + fb.3;
                        let in_cover = r.is_some_and(|r| dx >= r.x && dx < r.x + r.w && dy >= r.y && dy < r.y + r.h);
                        assert_eq!(in_box, in_cover, "window ({dx},{dy}) shows ({sx},{sy}) — box {fb:?} on {game:?} {lw}x{lh}+{y0}, cover {r:?}");
                        shown += u64::from(in_box);
                    }
                }
                assert_eq!(r.is_some(), shown > 0, "a box nothing shows has no cover, and one that shows has");
            }
        }
    }

    /// Under the box the frame is left exactly as the game painted it; the rest of each tab
    /// is drawn as before, and nothing outside the tabs is touched.
    #[test]
    fn the_tabs_are_drawn_everywhere_but_under_the_box() {
        let (w, h) = (200u32, 200u32);
        let frame: Vec<u32> = (0..w * h).map(|i: u32| 0xff00_0000 | ((i.wrapping_mul(2_654_435_761) >> 8) & 0x00ff_ffff)).collect();
        let tabs = [Rect::new(20, 20, 40, 30), Rect::new(20, 60, 40, 30)];
        let cover = Rect::new(40, 30, 120, 50);
        let mut plain = frame.clone();
        draw_bag_tabs(&mut plain, w, h, &tabs, 0, false, None);
        let mut under = frame.clone();
        draw_bag_tabs(&mut under, w, h, &tabs, 0, false, Some(cover));
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let in_tab = tabs.iter().any(|t| x >= t.x && x < t.x + t.w && y >= t.y && y < t.y + t.h);
                let in_cover = x >= cover.x && x < cover.x + cover.w && y >= cover.y && y < cover.y + cover.h;
                let want = if in_cover || !in_tab { frame[i] } else { plain[i] };
                assert_eq!(under[i], want, "({x},{y}) tab={in_tab} cover={in_cover}");
            }
        }
        assert_ne!(plain, frame, "the tabs do draw");
        // A cover that misses the tabs, or runs off the window, changes nothing about them.
        let mut far = frame.clone();
        draw_bag_tabs(&mut far, w, h, &tabs, 0, false, Some(Rect::new(150, 150, 400, 400)));
        assert_eq!(far, plain);
    }

    /// ★ 2026-10-02: the short fifth page — its 13 missing cells are the window px of the game's 14 × 14 black
    /// squares past cell 14 (`h2_page_hidden_cells`), painted the panel's body colour; everything else, and the
    /// game's box over them, stays as the game painted it.
    #[test]
    fn the_fifth_page_paints_its_missing_cells_as_the_panel() {
        use crate::guest_build::{h2_page_cells, h2_page_hidden_cells, Hero2Java};
        assert_eq!((0..5).map(h2_page_cells).collect::<Vec<_>>(), vec![28, 28, 28, 28, 15], "127 = 4 × 28 + 15");
        assert!(h2_page_hidden_cells((28, 51), 28, 7).is_empty(), "a full page hides nothing");
        let hidden = h2_page_hidden_cells((28, 51), 15, 7);
        assert_eq!(hidden.len(), 13);
        // Cell 15 = row 2, col 1; cell 27 = row 3, col 6 — where `q.a(II)V` fills them (grid moved 7 right).
        assert_eq!(hidden[0], (28 + 7 + 7 + 15, 51 + 31 + 2 * 15, 14, 14));
        assert_eq!(hidden[12], (28 + 7 + 7 + 6 * 15, 51 + 31 + 3 * 15, 14, 14));
        assert!(hidden.iter().all(|r| r.0 + r.2 <= 28 + Hero2Java::PANEL_W && r.1 + r.3 <= 51 + Hero2Java::PANEL_H), "inside the panel");
        // On a 4× window: the cells' window px become the body colour, the rest of the frame is untouched, and
        // a box over two of them keeps its pixels.
        let game = Rect::new(0, 0, 704, 880);
        let rects: Vec<Rect> = hidden.iter().filter_map(|fb| fb_rect_to_win(game, 176, 220, 0, *fb)).collect();
        assert_eq!(rects.len(), 13);
        assert_eq!(rects[0], Rect::new(57 * 4, 112 * 4, 56, 56));
        let (w, h) = (704u32, 880u32);
        let frame: Vec<u32> = (0..w * h).map(|i: u32| 0xff00_0000 | ((i.wrapping_mul(2_654_435_761) >> 8) & 0x00ff_ffff)).collect();
        let cover = Rect::new(rects[0].x, rects[0].y, rects[1].x + rects[1].w - rects[0].x, rects[0].h);
        let mut out = frame.clone();
        draw_bag_hidden_cells(&mut out, w, h, &rects, Some(cover));
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let in_cell = rects.iter().any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h);
                let in_cover = x >= cover.x && x < cover.x + cover.w && y >= cover.y && y < cover.y + cover.h;
                let want = if in_cell && !in_cover { H2_BODY } else { frame[i] };
                assert_eq!(out[i], want, "({x},{y}) cell={in_cell} cover={in_cover}");
            }
        }
    }
}

// ── pickup notices (2026-09-24) ─────────────────────────────────────────────────
//
// User: *"만약 아이템 3개를 한번에 주웠다면 안내가 위로 조금씩 밀리면서 최신 주운 아이템이
// 밑으로 나오게 … 그리고 안내문구 위치를 좌측 끝으로"*. The game shows ONE notice at a
// time, centred (`n.d()V`, now off); these are the same pictures — copied out of the
// game's own `n.b` image, its font and its box — stacked at the left edge: the newest
// takes the game's own row (`ax.i − 42`), the ones before it slide up a slot each.

/// A memory bound on the queue, not the on-screen limit: how many fit is measured every
/// frame from the space between the bottom row and the gauges (`draw_notices`), and on
/// a real phone that is well under this.
pub const NOTICE_MAX: usize = 16;
/// How long each stays up. The game's own was 16 of its frames (~0.8 s) for a single
/// notice that the next one simply replaced; a stack needs long enough to read three.
const NOTICE_LIFE_MS: u128 = 2000;
/// How long a notice takes to slide up one slot.
const NOTICE_SLIDE_MS: f32 = 140.0;
/// Guest pixels between two stacked notices, and from the left edge and the gauges.
/// ★ 2026-09-25: 2 → 1 (player: "각 표시박스별 간격을 줄이고") — with the black margin
/// rows cropped off each notice, 1 px of world between two gold frames is the gap.
const NOTICE_GAP: f32 = 1.0;
/// The game's own row for its notice: `ax.i − 42` (`n.d()V` @0x163a9a: fp − 16, fp = ax.i − 26).
const NOTICE_FROM_BOTTOM: u32 = 42;

pub struct Notice {
    w: u16,
    h: u16,
    argb: Vec<u32>,
    born: std::time::Instant,
    /// The slot it is DRAWN at (0 = the bottom row), sliding towards its real one.
    shown: f32,
    at: std::time::Instant,
    /// ★★ 2026-10-17 보스 레이드: the raid's countdown — the next one replaces this one in place (`UserEvent::RaidCountdown`).
    countdown: bool,
}

impl Notice {
    /// ★ 2026-09-25 (player: "표시되는 박스의 높이 … 줄이고"): the picture is CROPPED to its
    /// box. The game's notice is `k.a(g, …, 96, 16, …)` @`n.a([BII)V` 0x164670 — a 96x16
    /// opaque image with a black margin round a gold frame (rows 1..13) and one blank row
    /// under the top border; drawn whole, the margin read as part of a 16-row box. Kept:
    /// the frame's bounding box (every non-black pixel), minus that one blank row — 16 → 12
    /// rows, 96 → 93 columns, the glyphs untouched. Measured from the pixels rather than
    /// assumed, so a picture of another shape loses only margin.
    pub fn new(w: u16, h: u16, argb: Vec<u32>) -> Self {
        let now = std::time::Instant::now();
        let (w, h, argb) = crop_notice(w, h, argb);
        Self { w, h, argb, born: now, shown: 0.0, at: now, countdown: false }
    }

    /// ★★ 2026-10-17: the raid's countdown notice — cropped the same way, marked so the next number replaces it.
    pub fn countdown(w: u16, h: u16, argb: Vec<u32>) -> Self {
        Self { countdown: true, ..Self::new(w, h, argb) }
    }

    pub fn is_countdown(&self) -> bool {
        self.countdown
    }

    /// The next number: a new picture and a new life, the slot it is drawn in kept (no slide, no jump).
    pub fn refresh(&mut self, w: u16, h: u16, argb: Vec<u32>) {
        let (w, h, argb) = crop_notice(w, h, argb);
        (self.w, self.h, self.argb, self.born) = (w, h, argb, std::time::Instant::now());
    }
}

/// See [`Notice::new`]. `(w, h, argb)` in, the cropped picture out — or the input
/// unchanged when it has no ink or is not the size it claims.
fn crop_notice(w: u16, h: u16, argb: Vec<u32>) -> (u16, u16, Vec<u32>) {
    let (wu, hu) = (usize::from(w), usize::from(h));
    if wu == 0 || hu == 0 || argb.len() != wu * hu {
        return (w, h, argb);
    }
    let ink = |x: usize, y: usize| {
        let p = argb[y * wu + x];
        p >> 24 >= 0x80 && p & 0x00ff_ffff != 0
    };
    let rows: Vec<usize> = (0..hu).filter(|&y| (0..wu).any(|x| ink(x, y))).collect();
    let cols: Vec<usize> = (0..wu).filter(|&x| (0..hu).any(|y| ink(x, y))).collect();
    let (Some(&top), Some(&bottom), Some(&left), Some(&right)) = (rows.first(), rows.last(), cols.first(), cols.last()) else {
        return (w, h, argb);
    };
    // Inside the frame, only the row right under the top border goes — and only when it is
    // blank between the side borders. ⛔ NOT every blank row: an ASCII notice (「100G」)
    // inks rows 6..10 where a Hangul one inks 3..12, and cropping per picture would stack
    // boxes of two heights. Every notice measured (Hangul and ASCII) has that one row
    // blank, so every notice comes out the same 12 rows.
    let pad_blank = top + 1 < bottom && right > left + 1 && !(left + 1..right).any(|x| ink(x, top + 1));
    let keep: Vec<usize> = (top..=bottom).filter(|&y| !(pad_blank && y == top + 1)).collect();
    let nw = right - left + 1;
    let mut out = Vec::with_capacity(nw * keep.len());
    for &y in &keep {
        out.extend_from_slice(&argb[y * wu + left..=y * wu + right]);
    }
    (nw as u16, keep.len() as u16, out)
}

/// Draw the stack, oldest highest. `fb` = `(guest fb width, fb height, first shown row,
/// shown rows)` — the game blit's own mapping. `avoid` is the walk stick's seat: the
/// stack is lifted clear of it rather than drawn under a control the thumb is on.
/// `ceiling` is the window row the stack may not climb above — just under the gauges
/// ([`stats_bottom`]); the oldest notices beyond what fits under it leave early.
/// Returns whether anything is still up (the caller keeps redrawing for the slide and
/// the expiry).
#[allow(clippy::too_many_arguments)]
pub fn draw_notices(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    game: Rect,
    fb: (u32, u32, u32, u32),
    notices: &mut std::collections::VecDeque<Notice>,
    avoid: Option<Rect>,
    ceiling: Option<u32>,
) -> bool {
    let now = std::time::Instant::now();
    notices.retain(|n| now.duration_since(n.born).as_millis() < NOTICE_LIFE_MS);
    let (lw, fb_h, y0, lh) = fb;
    if notices.is_empty() || lw == 0 || lh == 0 || game.w == 0 || game.h == 0 {
        return !notices.is_empty();
    }
    let (sx, sy) = (game.w as f32 / lw as f32, game.h as f32 / lh as f32);
    let h = f32::from(notices.back().map_or(16, |n| n.h));
    let pitch = (h + NOTICE_GAP) * sy;
    let x = game.x as f32 + NOTICE_GAP * sx;
    let mut top = game.y as f32 + (fb_h.saturating_sub(NOTICE_FROM_BOTTOM) as f32 - y0 as f32) * sy;
    if let Some(a) = avoid {
        let w = f32::from(notices.back().map_or(96, |n| n.w)) * sx;
        let overlaps_x = x < (a.x + a.w) as f32 && (a.x as f32) < x + w;
        if overlaps_x && top + h * sy > a.y as f32 {
            top = a.y as f32 - h * sy - NOTICE_GAP * sy;
        }
    }
    // ★ 2026-09-25 (player: "HP바와 SP바 경험치바 표시되는 영역 바로 밑에까지만 … 핸드폰마다
    // 높이가 달라서 고정으로 하면안되고"): as many as fit between the bottom row and the
    // gauges on THIS screen — slot k's top is `top − k·pitch`, and it has to stay at or
    // below the ceiling. The newest always shows (its row is the game's own, far below
    // the gauges on any real screen). The oldest beyond that leave now, before the slide
    // targets are set, so the survivors slide into the space they will keep.
    if let Some(c) = ceiling {
        let limit = c as f32 + NOTICE_GAP * sy;
        let fit = if top >= limit && pitch > 0.0 { ((top - limit) / pitch).floor() as usize + 1 } else { 1 };
        while notices.len() > fit {
            notices.pop_front();
        }
    }
    let count = notices.len();
    for (i, n) in notices.iter_mut().enumerate() {
        let target = (count - 1 - i) as f32;
        let step = now.duration_since(n.at).as_secs_f32() * 1000.0 / NOTICE_SLIDE_MS;
        n.shown = (n.shown + step).min(target);
        n.at = now;
    }
    for n in notices.iter() {
        let (dw, dh) = ((f32::from(n.w) * sx).round() as i64, (f32::from(n.h) * sy).round() as i64);
        let (x0, y0) = (x.round() as i64, (top - n.shown * pitch).round() as i64);
        for dy in 0..dh {
            let wy = y0 + dy;
            if wy < i64::from(game.y) || wy >= i64::from(win_h) {
                continue;
            }
            let src_y = ((dy as f32 / sy) as usize).min(n.h as usize - 1);
            let row = &n.argb[src_y * n.w as usize..(src_y + 1) * n.w as usize];
            let out = wy as usize * win_w as usize;
            for dx in 0..dw {
                let wx = x0 + dx;
                if wx < 0 || wx >= i64::from(win_w) {
                    continue;
                }
                let p = row[((dx as f32 / sx) as usize).min(n.w as usize - 1)];
                // The game's box is opaque; a transparent pixel (none measured) is skipped
                // rather than blended — the picture is a copy, not a new style.
                if p >> 24 >= 0x80 {
                    buf[out + wx as usize] = 0xff00_0000 | (p & 0x00ff_ffff);
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod notice_tests {
    use super::*;

    const GOLD: u32 = 0xffdc_c364;
    const INK: u32 = 0xffff_ffff;

    /// The game's notice as measured (probe, 2026-09-25): 96x16 opaque black, a gold frame
    /// on rows 1/13 and columns 1/93, text inked on `text` rows (3..=12 Hangul, 6..=10 ASCII).
    fn picture(text: std::ops::RangeInclusive<usize>) -> Vec<u32> {
        let w = 96usize;
        let mut p = vec![0xff00_0000u32; w * 16];
        for x in 1..=93 {
            p[w + x] = GOLD;
            p[13 * w + x] = GOLD;
        }
        for y in 1..=13 {
            p[y * w + 1] = GOLD;
            p[y * w + 93] = GOLD;
        }
        for y in text {
            for x in 20..40 {
                p[y * w + x] = INK;
            }
        }
        p
    }

    /// ★ Hangul and ASCII notices crop to the SAME box — a per-picture crop of every blank
    /// row would have stacked 12-row and 7-row boxes.
    #[test]
    fn every_notice_crops_to_the_same_twelve_row_box() {
        for text in [3..=12usize, 6..=10] {
            let n = text.clone().count();
            let (w, h, px) = crop_notice(96, 16, picture(text));
            assert_eq!((w, h), (93, 12));
            assert!(px[..93].iter().all(|p| *p == GOLD), "row 0 is the top border");
            assert!(px[11 * 93..].iter().all(|p| *p == GOLD), "the last row is the bottom border");
            assert_eq!(px.iter().filter(|p| **p == INK).count(), n * 20, "every glyph pixel survives");
        }
    }

    #[test]
    fn a_picture_without_ink_or_of_the_wrong_length_is_left_alone() {
        let blank = vec![0xff00_0000u32; 96 * 16];
        assert_eq!(crop_notice(96, 16, blank.clone()), (96, 16, blank));
        let short = vec![INK; 10];
        assert_eq!(crop_notice(96, 16, short.clone()), (96, 16, short));
    }

    /// ★ The stack stops just under the gauges, however tall the screen is: the fit is
    /// `(bottom row − ceiling) / pitch + 1`, the oldest beyond it leave, and the newest
    /// always stays.
    #[test]
    fn the_stack_is_as_tall_as_the_space_under_the_gauges() {
        let (w, h) = (1920u32, 880u32);
        let game = Rect::new(0, 0, w, h);
        // 384x176 guest at 5x: the bottom row's top is (176 − 42)·5 = 670, pitch (12 + 1)·5 = 65.
        let fb = (384, 176, 0, 176);
        let fill = |q: &mut std::collections::VecDeque<Notice>| {
            q.clear();
            for _ in 0..NOTICE_MAX {
                let (pw, ph, px) = (96, 16, picture(3..=12));
                q.push_back(Notice::new(pw, ph, px));
            }
        };
        let mut buf = vec![0u32; (w * h) as usize];
        let mut q = std::collections::VecDeque::new();
        // 600 + 5 = 605 = 670 − 65 exactly: the second slot still fits; 610 leaves one.
        for (ceiling, want) in [(Some(100u32), 9usize), (Some(600), 2), (Some(610), 1), (Some(700), 1), (None, NOTICE_MAX)] {
            fill(&mut q);
            assert!(draw_notices(&mut buf, w, h, game, fb, &mut q, None, ceiling));
            assert_eq!(q.len(), want, "ceiling {ceiling:?}");
        }
        // Lifted clear of a stick seat under the bottom row, the stack loses the rows it
        // gave up: the newest moves to 400 − 60 − 5 = 335, so (335 − 105) / 65 + 1 = 4.
        fill(&mut q);
        draw_notices(&mut buf, w, h, game, fb, &mut q, Some(Rect::new(0, 400, 600, 480)), Some(100));
        assert_eq!(q.len(), 4);
    }
}

/// Lay framebuffer-space pictures over the game blit, with the blit's own mapping
/// (`fb` = guest width, height, first shown row, shown rows): each guest pixel becomes the
/// same window block the blit gave it, so the picture sits exactly on the game's pixels.
/// Alpha-0 pixels are left alone.
pub fn draw_guest_patches(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect, fb: (u32, u32, u32, u32), patches: &[crate::guest_build::H2Patch]) {
    let (lw, _fb_h, y0, lh) = fb;
    if lw == 0 || lh == 0 || game.w == 0 || game.h == 0 {
        return;
    }
    // Guest column/row -> first window column/row, the inverse of the blit's
    // `(d * src) / dst` sampling: the block for guest g starts at ceil(g * dst / src).
    let wx = |g: i64| i64::from(game.x) + (g * i64::from(game.w) + i64::from(lw) - 1) / i64::from(lw);
    let wy = |g: i64| i64::from(game.y) + ((g - i64::from(y0)) * i64::from(game.h) + i64::from(lh) - 1) / i64::from(lh);
    for p in patches {
        for py in 0..p.h as i64 {
            let (ya, yb) = (wy(i64::from(p.y) + py), wy(i64::from(p.y) + py + 1));
            for px in 0..p.w as i64 {
                let c = p.argb[(py * i64::from(p.w) + px) as usize];
                if c >> 24 == 0 {
                    continue;
                }
                let (xa, xb) = (wx(i64::from(p.x) + px), wx(i64::from(p.x) + px + 1));
                for y in ya.max(i64::from(game.y))..yb.min(i64::from(game.y + game.h)).min(i64::from(win_h)) {
                    let row = y as usize * win_w as usize;
                    for x in xa.max(i64::from(game.x))..xb.min(i64::from(game.x + game.w)).min(i64::from(win_w)) {
                        buf[row + x as usize] = 0xff00_0000 | (c & 0x00ff_ffff);
                    }
                }
            }
        }
    }
}

/// The emulator worker is gone: say so, and keep saying it.
///
/// ★★ NOT A TOAST (2026-09-20, 잔여 작업 #10). A toast is 1.6 s and this is the last
/// thing the app will ever draw — the guest is not coming back, so the notice has to be
/// there whenever the player looks, including when they look ten minutes later because
/// the music was still playing. It dims the frozen frame behind it for the same reason:
/// the last painted frame is a picture of a running game and reads as one.
///
/// ⛔ Every syllable must exist in `hangul.rs`, or the line draws BLANK
/// (`draw_hangul` early-returns) — the failure this whole closed set exists to prevent.
pub fn draw_fatal_notice(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect, lines: &[&str]) {
    if lines.is_empty() || win_w == 0 || win_h == 0 {
        return;
    }
    // ★ The WINDOW when there is no game rect yet: this notice also covers dying during
    // boot (a bad archive, a build that cannot start), and that is exactly when the
    // layout has nothing in it. A notice that silently declines to draw on the one path
    // where the player has seen nothing at all would be the worst of the set.
    let game = if game.w == 0 || game.h == 0 { Rect::new(0, 0, win_w, win_h) } else { game };
    blend_rect(buf, win_w, win_h, game.x, game.y, game.w, game.h, 0xff00_0000, 150);

    let s = (game.h / 260).clamp(2, 5);
    let line_h = crate::hangul::HANGUL_H * s;
    let gap = line_h / 2;
    let pad = (6 * s).max(8);
    let tw = lines.iter().map(|l| mixed_text_w(l, s)).max().unwrap_or(0);
    let bw = (tw + pad * 2).min(game.w);
    let bh = line_h * lines.len() as u32 + gap * (lines.len() as u32 - 1) + pad * 2;
    let bx = game.x + (game.w - bw) / 2;
    let by = game.y + game.h.saturating_sub(bh) / 2;
    blend_rect(buf, win_w, win_h, bx, by, bw, bh, 0xff10_0000, 235);
    stroke(buf, win_w, win_h, (bx, by, bw, bh), 0xffc8_5a3c);
    for (i, line) in lines.iter().enumerate() {
        let lw = mixed_text_w(line, s);
        let x = bx + bw.saturating_sub(lw) / 2;
        let y = by + pad + (line_h + gap) * i as u32;
        draw_mixed_text(buf, win_w, win_h, x, y, s, line, LABEL);
    }
}

/// Blink period of the badges, in ms per half-cycle: 250 on / 250 off = 2 Hz.
///
/// The original toggles its flag on 49 of every 50 HUD frames (`(counter+1) % 50`
/// gates the toggle at guest 0x14a2a2), i.e. it blinks at the frame rate — on a
/// feature phone that reads as a 50 %-brightness flicker, on a 60 fps phone it is
/// just noise. Player choice (2026-08-04): a readable ~2 Hz instead.
///
/// ★ Driven by HOST ELAPSED TIME, never by the guest's own blink flag
/// (`*(u8*)*(sl+0x434)`): the counter that flips it lives at guest 0x14a296, which
/// is INSIDE the range `items::hud_bottom_patch` branches over, so it never advances
/// — reading it would pin the badges permanently on or permanently off.
pub const BADGE_BLINK_MS: u128 = 250;

/// Is the badge visible this instant? `ms` = host milliseconds since app start.
pub fn badge_blink_on(ms: u128) -> bool {
    (ms / BADGE_BLINK_MS) % 2 == 0
}

/// Where the two badges go: side by side, one game pixel per art pixel, just right of
/// the level plate — under the SP bar (2026-09-29; before that they sat under the old
/// capsule stack, after the player moved them off its right side on 2026-08-05).
pub struct PointBadges {
    pub stat: Rect,
    pub skill: Rect,
}

pub fn point_badges(game: Rect) -> PointBadges {
    // One hud pixel per art pixel: the scale is a whole number by construction, which
    // matters — 7 is odd, and a fractional scale smears the `+`'s one-pixel arms.
    let g = status_geom(game);
    PointBadges { stat: status_rect(g.badge(0)), skill: status_rect(g.badge(1)) }
}

/// The gauge block as ONE rect (ring + bars + level plate + both badges), for the no-go
/// set: it is drawn after the controls, so anything placed under it just disappears
/// behind it.
///
/// ★ The badges are included UNCONDITIONALLY, even though they only draw while points
/// are unspent. Reserving space that is usually empty is the safe direction: the
/// alternative is a control that is legal at level 1 and then gets covered the first
/// time the player levels up. This also widens [`stats_tap_box`] by the same amount,
/// which keeps that function's "exactly one pad of growth" argument intact — both the
/// reserved rect and the tap zone move together — and is a bonus for the player,
/// since tapping a badge now opens the menu the points are spent in.
pub(crate) fn stats_block_box(game: Rect) -> Rect {
    status_rect(status_geom(game).bbox())
}

/// Nearest-neighbour blit of a 7x7 badge into `r`, skipping the transparent corners.
/// No blending: every non-transparent source pixel is fully opaque (the `_bm` palette
/// has one transparent entry and no alpha), which is also what softbuffer wants.
fn blit_badge(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, src: &[u32; 49]) {
    if r.w == 0 || r.h == 0 {
        return;
    }
    let x1 = (r.x + r.w).min(win_w);
    let y1 = (r.y + r.h).min(win_h);
    for py in r.y..y1 {
        let sy = ((py - r.y) * BADGE_SRC / r.h).min(BADGE_SRC - 1);
        for px in r.x..x1 {
            let sx = ((px - r.x) * BADGE_SRC / r.w).min(BADGE_SRC - 1);
            let c = src[(sy * BADGE_SRC + sx) as usize];
            if c != 0 {
                buf[(py * win_w + px) as usize] = c;
            }
        }
    }
}

/// Draw whichever badges have unspent points, if the blink says "on".
fn draw_point_badges(buf: &mut [u32], win_w: u32, win_h: u32, stat: Rect, skill: Rect, s: &Stats, blink: bool) {
    if !blink {
        return;
    }
    // `> 0` for both: the game tests stat points with `ble` (signed) and skill points
    // with `beq`, and `read_stats` already folded a negative stat count to 0.
    if s.stat_points > 0 {
        blit_badge(buf, win_w, win_h, stat, &POINT_BADGE_STAT);
    }
    if s.skill_points > 0 {
        blit_badge(buf, win_w, win_h, skill, &POINT_BADGE_SKILL);
    }
}

/// Window rect of the game's own (masked) HUD box in immersive mode — the region
/// our overlay must not fight with. `None` outside immersive mode.
/// No longer masked (see `mask_game_hud`) — kept because it documents the one
/// region of the frame the game owns at the bottom.
#[allow(dead_code)]
pub fn game_hud_box(game: Rect) -> Option<Rect> {
    let (lw, lh) = crate::logical();
    // hero2: the band is measured and MASKED, so the region the game owns is the whole
    // logical-width strip — and after the mask it is ours. Reported anyway so layout
    // code that wants to know where the game's strip was can ask one place.
    if let Some((band_y, band_h)) = crate::guest_build::guest_hud_band()
        .filter(|_| crate::guest_build::hud_reserved_rows() == 0 && crate::guest_build::hud_bottom_patch().is_none())
    {
        if game.w == 0 || game.h == 0 || lh == 0 {
            return None;
        }
        let y = game.y + game.h * band_y / lh;
        let h = (game.h * band_h / lh).max(2).min((game.y + game.h).saturating_sub(y));
        return Some(Rect::new(game.x, y, game.w, h));
    }
    if !crate::immersive_active() {
        return None;
    }
    let bw = (game.w * 176 / lw.max(1)).min(game.w);
    let bh = (game.h * 24 / lh.max(1)).max(2);
    Some(Rect::new(game.x + (game.w - bw) / 2, (game.y + game.h).saturating_sub(bh), bw, bh))
}

/// Draw the player's status block at the top-left of the game rect — the custom
/// replacement for the game's own bottom HUD.
///
/// ★★ 2026-09-29: hero5's block (player: *"Screenshot_20260929_023746_5 파일처럼 좌측 상단
/// 현재 캐릭터 모습(외형), HP,MP,경험치바 … 꾸미고싶어"*) — the character's head in a gold
/// ring (`Stats::head`, the helmet showing), HP and SP in a silver-framed block with their
/// values, the level on a plate under the ring. The EXP bar left the block for the bottom
/// centre ([`draw_exp_bar`], player choice). One layout for both orientations now: its
/// pixel is the game's pixel (`hud_view::hv_unit`), so it scales with the game, and the
/// bars are shorter on the 176-wide portrait game. The drawing is `hud_view.rs`.
/// (Was, until 2026-09-28: a LV disc and three thin capsules — HP · SP · EXP.)
///
/// ★ 2026-10-05: and the running skill buffs right of the bars — the skill's icon and its seconds
/// left (`hud_view::hv_draw_buffs`, user: *"HP SP바 우측에 … 스킬아이콘과 숫자로"*). Not part of the
/// block's layout box (`HvGeom::bbox`), so a saved layout is untouched.
// ★ 2026-10-07: a buff that is ON until switched off (분노태세) — the worker's mark is the HUD's.
const _: () = assert!(HV_BUFF_ON == crate::guest_build::H2_BUFF_ON as u32);

pub fn draw_stats(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    game: Rect,
    s: &Stats,
    portrait: Option<&crate::guest_build::H2Image>,
    buffs: &[crate::guest_build::H2BuffShow],
    gauges: &[(u8, u8, u8)],
    blink: bool,
) {
    if game.w == 0 || game.h == 0 {
        return;
    }
    let g = status_geom(game);
    // ★ A3b: the head the game loaded (a costume's too); the baked atlas head when it is unread.
    let head = portrait
        .map(|p| HvIcon { w: p.w, h: p.h, px: &p.argb })
        .or_else(|| s.head.and_then(|n| atlas::atlas().head(n)).map(|ic| HvIcon { w: u32::from(ic.w), h: u32::from(ic.h), px: &ic.px }));
    let hv = HvStats {
        hp: u32::from(s.hp),
        hp_max: u32::from(s.hp_max),
        sp: u32::from(s.sp),
        sp_max: u32::from(s.sp_max),
        level: u32::from(s.level),
    };
    hv_draw_status(buf, win_w, win_h, &g, &hv, head);
    if !buffs.is_empty() {
        let cells: Vec<HvBuff> = buffs
            .iter()
            .map(|b| HvBuff {
                icon: if b.is_beast() { atlas::atlas().beast(b.cell) } else { atlas::atlas().skill(b.sheet, b.cell) }.map(|ic| HvIcon { w: u32::from(ic.w), h: u32::from(ic.h), px: &ic.px }),
                secs: u32::from(b.secs),
                // ★ 2026-10-07 (2): the rage gauge rides 격분's cell (id 33 on the berserker strip) — ★ 2026-10-13 D4 and
                //   추종's its own: whichever gauge names this cell
                gauge: gauges.iter().find(|g| g.0 == b.id && g.1 == b.sheet).map(|g| u32::from(g.2)),
            })
            .collect();
        hv_draw_buffs(buf, win_w, win_h, &g, &cells);
    }
    // The game's own unspent-point badges, beside the level plate.
    let pb = point_badges(game);
    draw_point_badges(buf, win_w, win_h, pb.stat, pb.skill, s, blink);
}

/// The EXP bar at the bottom centre of the game rect (2026-09-29 — hero5's place for it;
/// it was the third capsule under HP/SP). Drawn BEFORE the controls, unlike the block at
/// the top-left: it is not an obstacle to them, so a control the player parks over the
/// bottom edge covers it rather than the other way round.
pub fn draw_exp_bar(buf: &mut [u32], win_w: u32, win_h: u32, game: Rect, s: &Stats) {
    if game.w == 0 || game.h == 0 {
        return;
    }
    let (lw, lh) = crate::logical();
    let e = HvExp::new((game.x, game.y, game.w, game.h), lw, lh);
    hv_draw_exp(buf, win_w, win_h, &e, s.exp, s.exp_next);
    // ★★ 2026-10-22 가디언 레벨: after LV 99 the bar is the GL bar, and the GL sits just above it.
    if let Some(gl) = s.guardian {
        hv_draw_gl(buf, win_w, win_h, &e, u32::from(gl));
    }
}

/// The window row just below everything `draw_stats` puts at the top-left — the ring,
/// the bars, the level plate and the point-badge pair (reserved even while no points are
/// unspent, like `stats_block_box`, so a level-up never lands a badge on a notice).
pub(crate) fn stats_bottom(game: Rect) -> u32 {
    let b = stats_block_box(game);
    b.y + b.h
}

pub fn gear_hit(gear: &Rect, x: f64, y: f64) -> bool {
    gear.contains(x, y)
}

/// Whether `(x, y)` is inside a plain UI rect (keypad toggle, panels). A
/// zero-sized rect never hits, which is how the disabled toggle is expressed.
pub fn rect_hit(r: &Rect, x: f64, y: f64) -> bool {
    r.contains(x, y)
}

/// Whether `(x, y)` falls in the joystick zone (joystick mode only).
pub fn in_joystick_zone(layout: &Layout, x: f64, y: f64) -> bool {
    layout.joystick.as_ref().is_some_and(|z| z.contains(x, y))
}

/// The key a touch that hit NOTHING sends: `OK` during a CUTSCENE, nothing anywhere
/// else. Player request 2026-08-17, immediately after the previous day's "컷신에서는
/// `#` 하나만" shipped and left the script with no way to advance: "컷신에서 다음
/// 대화로 넘어갈 수가 없어 … `#`을 제외한 어디를 눌러도 대화가 넘어가게 …
/// 화면전체가 OK버튼인 느낌이야. 이 정책은 컷신일때만 작동해야 해."
///
/// ★ It is a FALLBACK, not a rect: `App` runs it as the last arm of the same
/// hit-test chain the `#` sits in, so "everywhere except the controls" needs no
/// geometry at all and can never drift from what is drawn. That also keeps the `#`
/// working as the skip — it is tested first and wins its own disc.
///
/// ★ Cutscene ONLY. On the field the same fallback would fire the game's attack key
/// on every stray touch of open ground, and in a menu it would confirm whatever the
/// cursor happens to sit on.
pub fn tap_anywhere_key(mode: Mode) -> Option<KeyCode> {
    (mode == Mode::Cutscene).then_some(KeyCode::OK)
}

// --------------------------------------------------------------------------
// Settings modal
// --------------------------------------------------------------------------

fn settings_panel(win_w: u32, win_h: u32) -> Rect {
    // Lower bound must never exceed the upper bound (tiny placeholder dims in
    // App::new would otherwise make clamp panic with min > max).
    // Grown 56 % -> 72 % of the height for the two size-knob rows (2026-07-31). It was
    // 80 % for one day (2026-09-23, the 경험치 배율 row squeezed in as a sixth); since
    // 2026-09-24 the panel is back at 72 % and rows past [`SETTINGS_VISIBLE_ROWS`]
    // SCROLL instead (user: "넘치는 오토루팅, 경험치 배율은 스크롤로 관리 … 항목 디자인은
    // 기존과 완전 동일하게 하고 행만 늘려서").
    let pw = (win_w * 74 / 100).clamp(360.min(win_w), win_w.max(1));
    let ph = (win_h * 72 / 100).clamp(340.min(win_h), win_h.max(1));
    Rect::new((win_w - pw) / 2, (win_h - ph) / 2, pw, ph)
}

/// Height of the modal's HEADER strip — since #11 the 설정 title bar, with the 닫기
/// plate at its right. Shared by the layout and the drawing, so the title can never
/// be sized onto row 1.
fn settings_head(p: Rect) -> u32 {
    let close_s = (p.w / 8).clamp(56, 110);
    (p.h / 30 + close_s + p.h / 40).max(p.h * 18 / 100)
}

/// The settings modal — GEOMETRY only. What it looks like is [`draw_settings`], and
/// since #11 (2026-09-20) that is the game's own window:
///
/// ```text
///   ######################################
///   #           설정           [ 닫기 ]   #   black title bar
///   # o -------------------------------o #
///   #   [ 화면 배치       사용자 배치 ]   #   black plates on the khaki body:
///   #   [ 진단 정보       세이브 관리 ]   #   name left, current state right
///   #   [ …                          ]   #
///   #   [ 경험치 배율             x2 ]   #   (2026-09-23: the sixth row)
///   # o -------------------------------o #   (o = the corner studs)
///   ######################################
/// ```
///
/// ★ Trimmed twice on player request. 2026-07-31 ("layout edit 외 나머지는 다
/// 지워도 될 것 같아") removed PORTRAIT/LANDSCAPE, D-PAD/JOYSTICK, FULL and ZOOM —
/// one-time setup knobs whose persisted values still work, so putting a row back is
/// one line; `config::read_landscape` DEFAULTS to landscape for the same reason.
///
/// 2026-08-05 removed two more: SAVE moved OUT of the modal onto the on-field
/// SAVE NOW button (`crate::placement::CtrlId::QuickSave`, 20차) — the modal row
/// could never succeed anyway, since the gear exists only OFF the field while the
/// game's save gate passes only ON it — and SERVER went entirely ("설정 버튼
/// 눌렀을때 사설 서버 정보 나오는 항목 제거"). The server address machinery
/// (`wie_server.txt`, the platform dialog, `SettingsAction::EditServer`) is
/// retained unreferenced, so restoring the row is one entry here.
///
/// The header height is RESERVED rather than assumed: an earlier layout put row 1 at
/// 20 % of the panel, which the "SET" title (up to scale 22 = 154 px) reached into.
pub fn settings_items(win_w: u32, win_h: u32, scroll: u32) -> Vec<SettingsItem> {
    let g = settings_geom(win_w, win_h);
    let p = g.panel;
    let close_s = (p.w / 8).clamp(56, 110);
    let close = Rect::new(p.x + p.w - close_s - p.w / 30, p.y + p.h / 30, close_s, close_s);
    let scroll = scroll.min(settings_scroll_max(win_w, win_h));

    let mut items = vec![SettingsItem { rect: close, action: SettingsAction::Close, kind: SKind::Close, enabled: true }];
    for (i, (action, kind, enabled)) in settings_rows(win_w, win_h).into_iter().enumerate() {
        // Row `i` at the fixed pitch, moved up by the scroll. A row wholly outside the
        // list's window is not built at all (nothing to draw, nothing to hit); one that
        // is cut by an edge is built whole — `draw_settings` covers the part outside
        // with the frame, and `settings_hit` only answers inside the window. The window
        // starts well below the screen top, so `top` never has to be clamped at 0.
        let top = i64::from(g.view.y + i as u32 * g.pitch + (g.pitch - g.plate) / 2) - i64::from(scroll);
        if top + i64::from(g.plate) <= i64::from(g.view.y) || top >= i64::from(g.view.y + g.view.h) {
            continue;
        }
        items.push(SettingsItem { rect: Rect::new(g.x0, top.max(0) as u32, g.bw, g.plate), action, kind, enabled });
    }
    items
}

/// How many rows the settings list shows at once — the panel's size at five rows is
/// the one the player knows (2026-09-18), and every row past it scrolls.
pub const SETTINGS_VISIBLE_ROWS: u32 = 5;

/// The settings list's fixed geometry: the panel, the list's window (`view`), the
/// row pitch and the plate inside it, and the plate's width and left edge.
struct SettingsGeom {
    panel: Rect,
    view: Rect,
    pitch: u32,
    plate: u32,
    bw: u32,
    x0: u32,
}

fn settings_geom(win_w: u32, win_h: u32) -> SettingsGeom {
    let p = settings_panel(win_w, win_h);
    let head = settings_head(p);
    let rows_h = p.h.saturating_sub(head + p.h / 24).max(3);
    // The pitch is always that of FIVE rows, whatever the list holds — so the plates
    // and the lettering never shrink as rows are added (the plate is 70 % of it, the
    // 2026-09-18 proportion).
    let pitch = (rows_h / SETTINGS_VISIBLE_ROWS).max(1);
    let plate = (pitch * 70 / 100).max(1);
    let bw = p.w * 78 / 100;
    let x0 = p.x + (p.w - bw) / 2;
    SettingsGeom { panel: p, view: Rect::new(p.x, p.y + head, p.w, pitch * SETTINGS_VISIBLE_ROWS), pitch, plate, bw, x0 }
}

/// Every row the panel holds, in order: `(action, kind, enabled)`.
fn settings_rows(win_w: u32, win_h: u32) -> Vec<(SettingsAction, SKind, bool)> {
    // ★ FIVE base rows since 2026-09-18 (hero3's 99차 set): 화면 배치 · 진단 정보 ·
    // 조작 방식 · 발열 절감 · 효과음 크기. The ACCOUNT row went with it — hero2 has no
    // online warehouse to carry an account to. 화면 배치 and 조작 방식 are gated alike:
    // only the landscape-immersive HUD has placeable controls and the stick / D-pad
    // pair to choose between.
    let editor = layout_editor_available(win_w, win_h);
    let mut rows = vec![
        (SettingsAction::EditLayout, SKind::EditLayout, editor),
        // Always available: it is precisely the broken configurations (no audio, wrong
        // sizes) that need this row reachable.
        (SettingsAction::Diagnostics, SKind::Diag, true),
        // ⛔ 조작 방식 is NOT here either (2026-09-24, user: "조작 스틱을 상단 톱니바퀴가
        // 아니라 인게임 내의 환경설정에 넣고싶다는 말이었어"): free / fixed_A / fixed_B is
        // the host's row 7 of the game's own 환경설정 (`hero2java::H2Options`).
        (SettingsAction::ToggleHeatSaver, SKind::HeatSaver, true),
        (SettingsAction::CycleSfxLevel, SKind::SfxLevel, true),
        // ⛔ 오토루팅 and 경험치 배율 are NOT here (2026-09-24, user: "상단 톱니바퀴에 …
        // 넣는다는 말이아니라 인게임 내의 환경설정에 넣고싶다"): they are rows of the game's
        // own 환경설정 window (`hero2java::H2Options`, the host's rows 5 and 6), next to
        // the settings the game already has. Past the fifth row this list scrolls, which
        // today only the pad row can reach.
    ];
    // ★ TWO MORE ROWS when there is a gamepad (59차; the second 2026-10-09) — and only
    // then. They also have to stay while either setting is anything but the default, or a
    // player who changed one could not change it back without first picking up the pad.
    if crate::pad_seen() || crate::pad_hud_pref() != crate::padhud::PadHud::Auto || crate::gamepad::remapped() {
        rows.push((SettingsAction::TogglePadHud, SKind::PadHud, true));
        rows.push((SettingsAction::PadMap, SKind::PadMap, true));
    }
    rows
}

/// How far the list can scroll, in window pixels (0 while every row fits).
pub fn settings_scroll_max(win_w: u32, win_h: u32) -> u32 {
    let g = settings_geom(win_w, win_h);
    (settings_rows(win_w, win_h).len() as u32).saturating_sub(SETTINGS_VISIBLE_ROWS) * g.pitch
}

/// One row's pitch — the scroll snaps to it when the finger lifts.
pub fn settings_row_pitch(win_w: u32, win_h: u32) -> u32 {
    settings_geom(win_w, win_h).pitch
}

/// The list's window: a finger that lands here may be starting a scroll, so the tap
/// is decided on release rather than on press.
pub fn settings_list_view(win_w: u32, win_h: u32) -> Rect {
    settings_geom(win_w, win_h).view
}

/// Is the layout editor usable at this window? Shared by the row's enabled state
/// and by `App::enter_editor`, so the button cannot say one thing and the handler do
/// another.
pub fn layout_editor_available(win_w: u32, win_h: u32) -> bool {
    crate::immersive_active() && win_w > win_h
}

pub fn settings_hit(items: &[SettingsItem], win_w: u32, win_h: u32, x: f64, y: f64) -> Option<SettingsAction> {
    // A row cut by the list's edge answers only inside the window: its hidden part is
    // under the title bar or the bottom band.
    let view = settings_list_view(win_w, win_h);
    if let Some(item) = items.iter().find(|i| i.rect.contains(x, y) && (i.kind == SKind::Close || view.contains(x, y))) {
        // A disabled row swallows the tap: falling through would hand the point to
        // the "outside the panel closes it" rule below and shut the modal.
        return item.enabled.then_some(item.action);
    }
    // Tap outside the panel closes the settings.
    if !settings_panel(win_w, win_h).contains(x, y) {
        return Some(SettingsAction::Close);
    }
    None
}

// --------------------------------------------------------------------------
// Layout editor (player request 2026-07-31)
// --------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditAction {
    /// Show / hide the GAMEPAD badges on the controls being arranged (2026-09-25, player:
    /// "FLIP 왼쪽에 PAD 라는 노란색 배경의 버튼 … 누르면 현재 설정화면 내의 버튼들에 패드에서
    /// 어떤 버튼을 눌러야하는지 … 표시 … 껐다켰다 … ON/OFF 표시는 배경색으로"). A preview
    /// toggle, not a setting: nothing is stored, the layout is untouched, and it is off
    /// every time the editor opens. The badges are the in-play ones (`draw_pad_badges`),
    /// put on the editor's boxes by [`editor_pad_pos`].
    Pad,
    /// Mirror the whole layout left<->right — a one-tap left-handed layout.
    ///
    /// ★ "the whole layout", not "this mode's": there is one arrangement since
    /// 2026-09-18 (see `crate::placement::Table`), and the GAME / MENU page switch that
    /// used to sit ahead of this button is gone with it.
    Flip,
    /// Switch the SELECTED quickbar plate off (hero3 82차: "스킬 퀵슬롯을 선택 후 비활성화
    /// 처리 … 숨김처리를 하는거야"). No-op unless a plate is selected, and refused for
    /// the last visible one — see [`crate::quickpage::hide_plate`].
    HidePlate,
    /// Switch every hidden plate back on.
    ///
    /// All of them at once rather than one at a time, and that is a consequence of the
    /// editor showing the layout AS IT WILL BE: a hidden plate has no seat on the ring
    /// and nothing to tap. Restoring the row and switching off the ones you still do
    /// not want is two taps more and needs no ghost furniture.
    ShowPlates,
    /// Put every control back to the built-in layout.
    Reset,
    /// Discard everything done since the editor was opened, and close.
    Cancel,
    /// Keep the changes and close.
    Save,
}

pub struct EditItem {
    pub rect: Rect,
    pub action: EditAction,
}

/// The editor's own chrome: one compact row across the TOP band.
///
/// ★ Moved to the TOP and made smaller 2026-08-03 (player: "설정에서 save, 취소 등등
/// 버튼들이 가운데 쪽에 몰려있는데 상단으로 옮겨주고 크기도 조금 줄여줘 — 드래그하는
/// 과정에서 자꾸 설정 버튼을 누르게 된다"). The chrome is hit-tested BEFORE the
/// controls, and dragging happens across the middle and lower screen where the
/// controls live, so a block sitting in the bottom-centre was directly in the path of
/// every long drag. The top band is the one strip a drag does not cross: our gauges
/// own its left end, the minimap panel owns its right end, and the middle was empty.
/// (It was on the left edge before 2026-07-31 and in the bottom-centre after — both
/// were in the way of something the player wanted to do.)
///
/// The span is DERIVED, not centred: from the right edge of our gauge block to the
/// left edge of the minimap's built-in corner. Both are pure functions of the window,
/// which they have to be — this function is itself an obstacle in
/// [`crate::placement::obstacles`], so reading a PLACED rect here would recurse.
/// Keeping the row inside that band is what stops the chrome from burying a control:
/// anything under it could never be picked up again, and the tap meant to grab it
/// would hit RESET instead.
///
/// Labels are ASCII: `draw_glyph` is a 5x7 ASCII bitmap font, so "메뉴오픈" would
/// render as blank boxes. GAME = 게임진행, MENU = 메뉴오픈, FLIP = 좌우 반전.
pub fn edit_items(win_w: u32, win_h: u32) -> Vec<EditItem> {
    let pad = pad_of(win_w, win_h);
    let gap = pad * 2;
    let screen = Rect::new(0, 0, win_w, win_h);
    let gauges = stats_block_box(screen);
    let left = (gauges.x + gauges.w) + gap;
    let right = minimap_panel_builtin(win_w, win_h).x.saturating_sub(gap).max(left + 6);
    let avail = right - left;
    let cell_w = |cols: u32| ((avail + gap) / cols).saturating_sub(gap);
    // One row of six where the band can hold six tappable buttons, two rows of three
    // where it cannot (small windows). Never wider than the band.
    //
    // ★ Eight for a while: HIDE / SHOW joined the row (hero3 82차) and GAME / MENU left
    // it when the second seat table went (2026-09-18). The ROW COUNT is what matters and
    // it is unchanged either way (6/3 and 8/4 both mean "one row or two"), which is what
    // keeps this block's height — and therefore its footprint in `placement::obstacles`
    // — where it was.
    let cols6: u32 = if cell_w(6) >= 56 { 6 } else { 3 };
    let w6 = cell_w(cols6).clamp(40, 170);
    let h = (win_h / 18).clamp(26, 64);
    let span = w6 * cols6 + gap * (cols6 - 1);
    // ★★ SEVEN since 2026-09-25 (PAD, left of FLIP) — inside the footprint the SIX had,
    // to the pixel. The block is an obstacle in `placement::obstacles`, so a wider row
    // is not cosmetic: every stored placement that cleared the old ends by less than a
    // cell would be silently rejected and snap back to its built-in. Measured on the S9's
    // own `wie_layout.txt` (2220x1080, `field map 0.9401 0.1211 130`): a row of seven
    // 170 px cells ends at x 1853, the 130 % minimap starts at 1856 and needs a 9 px gap,
    // so the player's minimap would have fallen back to 100 % at the corner. So the span
    // stays, the seventh cell is paid for by the gaps BETWEEN cells (half as wide — dead
    // space either way, nothing fits in a gap) and ~12 % narrower cells; the row pitch
    // and the second row, when there is one, stay where they were.
    let cols = cols6 + 1;
    let cgap = (gap / 2).max(2);
    let w = (span.saturating_sub(cgap * (cols - 1)) / cols).max(1);
    let x0 = (left + avail.saturating_sub(span) / 2).min(win_w.saturating_sub(span));
    let y0 = pad * 2;
    let cell = |i: u32| Rect::new(x0 + (i % cols) * (w + cgap), y0 + (i / cols) * (h + gap), w, h);
    vec![
        // PAD first — "FLIP 왼쪽에" — and it is the one cell that changes nothing, so the
        // far end from SAVE is where a stray tap costs least.
        EditItem { rect: cell(0), action: EditAction::Pad },
        EditItem { rect: cell(1), action: EditAction::Flip },
        // ★ HIDE / SHOW after FLIP, not at the end: RESET / CANCEL / SAVE keep their
        // order and stay at the right-hand end where the player's thumb already looks
        // for them, and CANCEL keeps its own colour next to SAVE.
        EditItem { rect: cell(2), action: EditAction::HidePlate },
        EditItem { rect: cell(3), action: EditAction::ShowPlates },
        EditItem { rect: cell(4), action: EditAction::Reset },
        EditItem { rect: cell(5), action: EditAction::Cancel },
        EditItem { rect: cell(6), action: EditAction::Save },
    ]
}

pub fn edit_hit(items: &[EditItem], x: f64, y: f64) -> Option<EditAction> {
    items.iter().find(|i| i.rect.contains(x, y)).map(|i| i.action)
}

/// The editor's PAD button (2026-09-25): yellow in both states, the shade is the state.
const PAD_BTN_ON: u32 = 0xfff2_c230;
const PAD_BTN_OFF: u32 = 0xff5a_4812;
const PAD_INK_ON: u32 = 0xff20_1804;
const PAD_INK_OFF: u32 = 0xffe0_c878;

/// The gamepad position whose badge `id` carries IN PLAY — what the editor's PAD
/// preview (2026-09-25) stamps on that control's box. The same rules as
/// [`draw_pad_badges`] plus the three action badges `app.rs` adds after it:
///
/// * a key control → the pad position that sends its key ([`crate::gamepad::pos_for_key`]);
/// * a quickbar plate → the button that NAMES it (`PadAction::Quick`, hero3 79차-B —
///   X/Y/R1/R2/L2 = Q1..Q5 by default), whatever it holds and wherever the plate window
///   has it seated. Q6 · Q7 show none unless `wie_gamepad.txt` binds them. (The first
///   version followed the guest SEAT, so only a plate holding a skill had a badge —
///   device report 2026-09-25: Q1 showed one, Q2~Q7 did not);
/// * PAGE / SAVE → by action ([`crate::gamepad::pos_for_action`]) — no guest key means that;
/// * the minimap TOGGLE → the pad's `*`, the key that repaints the map;
/// * the minimap PANEL and the STICK → none, as in play: the panel is the map already,
///   and the pad walks with its own hat / stick (in pad mode the on-screen arrows are not
///   even drawn).
///
/// Every lookup is the live reverse map, so a `wie_gamepad.txt` remap moves the badge
/// here the same way it moves it in play.
pub(crate) fn editor_pad_pos(id: crate::placement::CtrlId, slots: &[crate::QuickSlot]) -> Option<crate::padhud::PadPos> {
    use crate::gamepad::{PadAction, pos_for_action, pos_for_key};
    use crate::placement::CtrlId;
    match id {
        CtrlId::Ok => pos_for_key(KeyCode::OK),
        CtrlId::Cancel => pos_for_key(KeyCode::CLEAR),
        CtrlId::Quick(i) => {
            let plate = slots.get(usize::from(i)).map_or(i, |s| s.slot);
            pos_for_action(PadAction::Quick(plate))
        }
        CtrlId::QuickPage => pos_for_action(PadAction::QuickPage),
        CtrlId::QuickSave => pos_for_action(PadAction::QuickSave),
        CtrlId::MinimapToggle => pos_for_action(PadAction::Key(KeyCode::STAR)),
        CtrlId::Minimap | CtrlId::Stick => None,
    }
}

/// Draw the editor: every FIELD control as a labelled box, the selected one
/// highlighted, the game's own HUD widgets marked as no-go areas, and the chrome.
///
/// ★ The mode parameter is gone with the second seat table (2026-09-18): there is one
/// arrangement and the field set is its whole vocabulary, so the editor always shows
/// that set in simulation.
///
/// `pad` = the PAD toggle (2026-09-25): `Some(quickbar)` stamps each box with the
/// gamepad badge that control carries in play — see [`editor_pad_pos`]; the live
/// quickbar is needed because a plate's badge follows the GUEST SEAT it sits in.
pub fn draw_layout_editor(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    sel: Option<crate::placement::CtrlId>,
    items: &[EditItem],
    pad: Option<&[crate::QuickSlot]>,
) {
    const SEL: u32 = 0xffe0_9a3c; // the action disc's gold — "this is the one"
    let mode = Mode::Field;
    const BLOCKED: u32 = 0xffc0_3a3a;
    // Dim the live frame so the boxes read as an overlay, not as the game.
    for px in buf.iter_mut() {
        let v = *px;
        let r = ((v >> 16) & 0xff) * 45 / 100;
        let g = ((v >> 8) & 0xff) * 45 / 100;
        let b = (v & 0xff) * 45 / 100;
        *px = 0xff00_0000 | (r << 16) | (g << 8) | b;
    }

    let placed = crate::placement::resolve(mode, win_w, win_h);
    // The two widgets the GAME draws itself. Shown so the refusal to place a control
    // there reads as a rule rather than as a bug.
    let map = placed
        .iter()
        .find(|(c, _)| *c == crate::placement::CtrlId::Minimap)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| Rect::new(0, 0, 0, 0));
    for o in crate::placement::obstacles(mode, win_w, win_h, map) {
        blend_rect(buf, win_w, win_h, o.x, o.y, o.w, o.h, BLOCKED, 60);
        stroke(buf, win_w, win_h, (o.x, o.y, o.w, o.h), BLOCKED);
    }

    for (id, r) in &placed {
        // The walk control the live touch type does NOT use is skipped outright —
        // the editor must offer exactly what the HUD will draw (player 2026-08-16:
        // "터치타입 B일때는 layout edit 에서 조이스틱 크기조절이 아닌 화살표
        // 크기조절로 나와야해"). `app.rs` filters its hit-test with the same
        // predicate, so the hidden one cannot be selected blind either. ★ And the two top
        // tabs are not offered at all (2026-10-01, `placement::in_editor`): fixed seats,
        // under this editor's own button row.
        if !crate::placement::in_editor(mode, *id) {
            continue;
        }
        let on = sel == Some(*id);
        let colour = if on { SEL } else { BORDER };
        // ★ Draw each control as the SHAPE IT WILL BE (player request 2026-07-31): every
        // key is a disc in play (`Face::Round`/`Action`), and only the minimap is a
        // square panel. Outlining a disc as a box made the editor look like it was
        // arranging something else.
        if id.is_round() {
            let rad = f64::from(r.w.min(r.h)) / 2.0;
            let cx = f64::from(r.x) + f64::from(r.w) / 2.0;
            let cy = f64::from(r.y) + f64::from(r.h) / 2.0;
            blend_disc_soft(buf, win_w, win_h, cx, cy, rad, if on { SEL } else { PANEL }, if on { 70 } else { 120 }, (rad * 0.08).max(1.0));
            blend_ring_soft(buf, win_w, win_h, cx, cy, rad - 1.0, 2.0, colour, 255);
            if on {
                // Inner ring too: unmistakable at a glance on a busy field, where a
                // one-pixel colour change is not.
                blend_ring_soft(buf, win_w, win_h, cx, cy, rad * 0.8, (rad / 16.0).max(1.2), colour, 200);
            }
        } else {
            blend_rect(buf, win_w, win_h, r.x, r.y, r.w, r.h, if on { SEL } else { PANEL }, if on { 70 } else { 120 });
            stroke(buf, win_w, win_h, (r.x, r.y, r.w, r.h), colour);
            if on {
                let i = (r.w.min(r.h) / 12).max(2);
                stroke(buf, win_w, win_h, (r.x + i, r.y + i, r.w.saturating_sub(i * 2), r.h.saturating_sub(i * 2)), colour);
            }
        }
        // ★★ hero3 91차: the STICK box shows everything the one knob governs — the four
        // hexagon arrows it is cut into (menu / choice, and the field under touch type
        // B) and, under type A, the stick's base (91차-B — "조이스틱 전체가 화살표 전체";
        // since 2026-09-20 the circle through the arrows' centres, a notch inside them).
        // Without this a TYPE A player pinching the box could not see the arrows they
        // were sizing, which is the report that merged the two controls in the first
        // place.
        if *id == crate::placement::CtrlId::Stick {
            if !crate::stick_fixed() {
                // The same circle `joy_home` gives the live stick: through the arrow
                // hexagons' centres, a little inside the arrows (2026-09-20).
                let rad = arm_centre_radius(r);
                let cx = f64::from(r.x) + f64::from(r.w) / 2.0;
                let cy = f64::from(r.y) + f64::from(r.h) / 2.0;
                blend_disc(buf, win_w, win_h, cx, cy, rad, 0xff1a_2436, 50);
                blend_ring_soft(buf, win_w, win_h, cx, cy, rad - 1.5, 3.0, colour, 200);
            }
            for arm in dpad_arms(*r) {
                draw_hex_face(buf, win_w, win_h, arm.draw, arm.dir, BTN_DPAD, colour, 70, 150);
            }
        }
        let label = id.label();
        // ★ The STICK's caption lives in the DEAD CENTRE between the four arrows, not
        // across the whole square: fitted to a 624 px box the letters would be 85 px
        // tall and bury the very arrows and disc the box is previewing.
        let lr = if *id == crate::placement::CtrlId::Stick {
            // The hole between the four inner tips: `50 - inset - ARM_LEN` from the
            // centre each way (~0.14 of the side).
            let s = r.w.min(r.h);
            let hole = (s.saturating_sub(s / 50 * 2 + s * ARM_LEN_PCT / 50)).max(1);
            Rect::new(r.x + (r.w - hole.min(r.w)) / 2, r.y + (r.h - hole.min(r.h)) / 2, hole, hole)
        } else {
            *r
        };
        if on {
            // Name over live size percent, both fitted — a 98 px plate cannot hold
            // two lines placed by eye.
            let base = crate::placement::builtin(*id, mode, win_w, win_h);
            let pct = crate::placement::Place::from_rect(*r, base, win_w, win_h).pct;
            // Every control shows a live percent since 2026-08-16 — the stick used to
            // say "MOVE" here because a pinch could not change it.
            let txt = format!("{pct}%");
            let (s, y1, y2) = two_line(lr, label.len().max(txt.len()) as u32 + 1);
            draw_centered_text_in(buf, win_w, win_h, lr.x, y1, lr.w, &label, LABEL, s);
            draw_centered_text_in(buf, win_w, win_h, lr.x, y2, lr.w, &txt, SEL, s);
        } else {
            let s = fit_scale(lr, label.len() as u32 + 1);
            draw_centered_text_in(buf, win_w, win_h, lr.x, lr.y + lr.h.saturating_sub(7 * s) / 2, lr.w, &label, LABEL, s);
        }
    }

    // ★ PAD ON (2026-09-25): the in-play badges, on the boxes being arranged. After every
    // box and caption for the same reason the live pass runs last — a badge painted in
    // with its own control would end up under the next box's fill wherever two overlap.
    if let Some(slots) = pad {
        for (id, r) in &placed {
            if !crate::placement::in_editor(mode, *id) {
                continue;
            }
            if let Some(pos) = editor_pad_pos(*id, slots) {
                draw_pad_badge(buf, win_w, win_h, *r, pos);
            }
        }
    }

    for item in items {
        let r = item.rect;
        // ★ HIDE / SHOW say on their face whether they can do anything right now — the
        // settings panel's own rule for a row that cannot be pressed ("a full-width
        // button that silently no-ops reads as a broken app"). HIDE needs a selected
        // plate that `quickpage::hide_plate` will accept (i.e. not the last visible
        // one); SHOW needs something to restore.
        let dim = match item.action {
            EditAction::HidePlate => !matches!(
                sel,
                Some(crate::placement::CtrlId::Quick(i))
                    if crate::quickpage::hide_plate(crate::quick_hidden(), i).is_some()
            ),
            EditAction::ShowPlates => crate::quick_hidden() == crate::quickpage::SHOW_ALL,
            _ => false,
        };
        let pad_on = pad.is_some();
        let face = match item.action {
            _ if dim => PANEL,
            // ★ PAD is yellow in both states, and the state IS the shade (player: "노란색
            // 배경의 버튼 … ON/OFF 표시는 배경색으로"): lit gold while the badges are up,
            // dark mustard while they are not.
            EditAction::Pad if pad_on => PAD_BTN_ON,
            EditAction::Pad => PAD_BTN_OFF,
            // Leaving without keeping the changes is the one destructive exit; give it
            // its own colour so it is not tapped by mistake next to SAVE.
            EditAction::Cancel | EditAction::Reset => 0xff3a_2430,
            _ => BTN,
        };
        draw_button_face(buf, win_w, win_h, r, face);
        // Single line: the block is compact now, and every caption is one short word.
        let label = match item.action {
            EditAction::Pad => "PAD",
            EditAction::Flip => "FLIP",
            // HIDE = 선택한 퀵슬롯 숨김, SHOW = 숨긴 것 전부 복원. ASCII like every
            // other caption here: this row is drawn with `draw_centered_text`, a 5x7
            // ASCII bitmap font, so Korean would come out as blank boxes.
            EditAction::HidePlate => "HIDE",
            EditAction::ShowPlates => "SHOW",
            EditAction::Reset => "RESET",
            EditAction::Cancel => "CANCEL",
            EditAction::Save => "SAVE",
        };
        let s = fit_scale(r, label.len() as u32 + 1);
        let ink = match item.action {
            _ if dim => BORDER,
            // White on bright gold washes out; the lit face takes a dark caption, the dark
            // one a pale-gold caption so the button still reads as the yellow one.
            EditAction::Pad if pad_on => PAD_INK_ON,
            EditAction::Pad => PAD_INK_OFF,
            _ => LABEL,
        };
        draw_centered_text_in(buf, win_w, win_h, r.x, r.y + r.h.saturating_sub(7 * s) / 2, r.w, label, ink, s);
    }

    // One-line instructions, just BELOW the chrome row (which now sits at the top —
    // above it there is nothing but the screen edge). ★ `=` is NOT in the 5x7 font
    // (device-caught: it rendered as blank space, so the line read "DRAG  MOVE  2
    // FINGERS  SIZE"). The font has `- + / . %` + letters. Held to the chrome's OWN
    // span, not the window's: centred on the window it ran out under the ring plates
    // and read as scribbled over them (device-caught).
    let pad = pad_of(win_w, win_h);
    let (hx, hw) = match (
        items.iter().map(|i| i.rect.x).min(),
        items.iter().map(|i| i.rect.x + i.rect.w).max(),
    ) {
        (Some(a), Some(b)) => (a, b.saturating_sub(a)),
        _ => (0, win_w),
    };
    let hint = "TAP - DRAG - PINCH";
    let s = ((win_h / 90).clamp(2, 6)).min((hw * 96 / 100 / (hint.len() as u32 * 6)).max(1));
    let hint_y = items
        .iter()
        .map(|i| i.rect.y + i.rect.h)
        .max()
        .map(|b| b + pad * 2)
        .unwrap_or_else(|| win_h.saturating_sub(pad * 3 + 7 * s));
    draw_centered_text_in(buf, win_w, win_h, hx, hint_y, hw, hint, LABEL, s);
}

// --------------------------------------------------------------------------
// Drawing
// --------------------------------------------------------------------------

/// Gap (px) below which two runs on a row are merged — see [`CachedLayer::runs`].
const RUN_GAP: usize = 24;

/// A HUD layer rasterised once and composited per frame (#4, 2026-09-18).
///
/// ★★ WHY. simpleperf on an S25 (field, standing still, 20 s, 17,216 samples) put
/// the UI thread at 46.2 % of the process — level with the emulator thread — and
/// 68.8 % of THAT was overlay compositing (`blend_disc_soft` 21.3 · `blend_ring_soft`
/// 16.1 · `blend_disc` 14.6 · `blend_ring` 9.5 · `blit_icon` 6.3). An S9 shows the
/// same shape as thread CPU (`android_main` 45~48 % while nothing moves). There was
/// NO INPUT during either window: the controls were being re-rasterised — per-pixel
/// `sqrt` + smoothstep — every single present, to produce the identical pixels.
///
/// So the pixels are rasterised ONCE into this layer and only re-rasterised when
/// the caller's `key` changes. The key is the caller's whole claim: two frames with
/// the same key draw the same pixels. [`ControlsLayer`] and [`QuickbarLayer`] each
/// own one of these and each hash everything their painter reads
/// ([`controls_key`], [`quickbar_key`]); a new input to a painter has to be added
/// to its key in the same commit.
///
/// The layer is premultiplied ARGB with 0 = untouched; see [`blend_px`] for why the
/// same draw code fills both a window and a transparent layer correctly.
#[derive(Default)]
pub struct CachedLayer {
    /// Premultiplied ARGB, `win_w * win_h`, 0 where nothing was drawn.
    buf: Vec<u32>,
    /// The non-zero pixels as a list of `(row start index, x0, x1)` runs, in raster
    /// order. Compositing walks these, so an idle frame costs the HUD's own area and
    /// not the window's.
    ///
    /// ★ RUNS, not one extent per row (2026-09-19). The first version kept one
    /// `[first non-zero, last non-zero)` per row, and simpleperf on S9 put
    /// `composite` at 37 % of the UI thread: the left stick and the right-hand
    /// controls share rows, so most rows' extent was the window's width and the loop
    /// was reading and testing ~2 M transparent pixels per present. Gaps narrower than
    /// [`RUN_GAP`] are bridged so a dithered edge does not become hundreds of runs.
    runs: Vec<(usize, u32, u32)>,
    key: u64,
    w: u32,
    h: u32,
    /// False until the first rasterisation, so a zero `key` cannot pass as a hit.
    valid: bool,
}

impl CachedLayer {
    /// Composite the layer into `dst`, running `paint` into the layer first if `key`
    /// (or the window size) differs from the last rasterisation.
    pub fn draw(&mut self, dst: &mut [u32], win_w: u32, win_h: u32, key: u64, paint: impl FnOnce(&mut [u32], u32, u32)) {
        let n = (win_w as usize) * (win_h as usize);
        if n == 0 || dst.len() < n {
            return;
        }
        if !self.valid || self.w != win_w || self.h != win_h || self.key != key {
            self.rasterise(win_w, win_h, paint);
            self.key = key;
            self.valid = true;
        }
        self.composite(dst, win_w);
    }

    fn rasterise(&mut self, win_w: u32, win_h: u32, paint: impl FnOnce(&mut [u32], u32, u32)) {
        let n = (win_w as usize) * (win_h as usize);
        if self.w != win_w || self.h != win_h || self.buf.len() != n {
            // A resize throws the old pixels away wholesale; the span-wise clear
            // below only knows about a layer of the CURRENT size.
            self.buf.clear();
            self.buf.resize(n, 0);
            self.runs.clear();
            self.w = win_w;
            self.h = win_h;
        } else {
            // Zero exactly what was written last time — the runs are that record,
            // so this is a clear of the HUD's area, not of the window.
            for &(row, x0, x1) in &self.runs {
                self.buf[row + x0 as usize..row + x1 as usize].fill(0);
            }
        }
        paint(&mut self.buf, win_w, win_h);
        // One full read to find the new runs. It is the only full-window pass, and it
        // happens on a visual change, not on a frame.
        self.runs.clear();
        for y in 0..win_h as usize {
            let row = y * win_w as usize;
            let line = &self.buf[row..row + win_w as usize];
            let mut x = 0usize;
            while let Some(off) = line[x..].iter().position(|&p| p != 0) {
                let x0 = x + off;
                let mut x1 = x0 + line[x0..].iter().position(|&p| p == 0).unwrap_or(line.len() - x0);
                // Bridge short gaps: a run that ends within `RUN_GAP` of the next one
                // is cheaper to composite through than to split.
                loop {
                    let probe = &line[x1..(x1 + RUN_GAP).min(line.len())];
                    match probe.iter().position(|&p| p != 0) {
                        Some(g) => {
                            let nx = x1 + g;
                            x1 = nx + line[nx..].iter().position(|&p| p == 0).unwrap_or(line.len() - nx);
                        }
                        None => break,
                    }
                }
                self.runs.push((row, x0 as u32, x1 as u32));
                x = x1;
            }
        }
    }

    fn composite(&self, dst: &mut [u32], _win_w: u32) {
        // ★ Branch-free (2026-09-19). The first version tested `a == 0` / `a == 255` per
        // pixel and was 44 % of the UI thread at ~24 cycles a pixel — the branches and
        // the two indexed accesses, not the arithmetic (a packed blend changed nothing).
        // `over_premul` is exact at both ends (`a == 0` yields `d`, `a == 255` yields
        // `p`), so the run is one straight zip the compiler can vectorise.
        for &(row, x0, x1) in &self.runs {
            let (lo, hi) = (row + x0 as usize, row + x1 as usize);
            let Some(dst) = dst.get_mut(lo..hi) else { continue };
            for (d, &p) in dst.iter_mut().zip(&self.buf[lo..hi]) {
                *d = over_premul(p, *d, p >> 24);
            }
        }
    }
}

/// `p` (premultiplied, alpha `a`) OVER the opaque `d`: per channel
/// `(p_c + d_c * (255 - a) / 255) & 0xff`, exactly as the per-channel loop it replaced
/// computed it — but R and B ride in one multiply and `/ 255` is the exact
/// `(t + (t >> 8) + 1) >> 8` identity (holds for every `t <= 65280`; `t <= 65025`
/// here), so this is three multiplies and no divides per pixel. Field arithmetic
/// cannot carry: each 16-bit field holds at most `255 * 255 + 255 + 1 = 65281`
/// before the shift and at most `255 + 255` after the add, and the final masks
/// reproduce the reference's `& 0xff`. At `a == 0` (untouched layer pixel, `p == 0`)
/// it returns `d`; at `a == 255` it returns `p`'s colour — the two cases the old loop
/// special-cased. Pinned by `over_premul_matches_reference`.
#[inline]
fn over_premul(p: u32, d: u32, a: u32) -> u32 {
    let inv = 255 - a;
    let rb = (d & 0x00ff_00ff) * inv;
    let rb = ((rb + ((rb >> 8) & 0x00ff_00ff) + 0x0001_0001) >> 8) & 0x00ff_00ff;
    let g = ((d >> 8) & 0xff) * inv;
    let g = (g + (g >> 8) + 1) >> 8;
    let rb = ((p & 0x00ff_00ff) + rb) & 0x00ff_00ff;
    let g = (((p >> 8) & 0xff) + g) & 0xff;
    0xff00_0000 | rb | (g << 8)
}

#[cfg(test)]
mod over_premul_tests {
    use super::over_premul;

    fn reference(p: u32, d: u32, a: u32) -> u32 {
        let inv = 255 - a;
        let mix = |sh: u32| (((p >> sh) & 0xff) + ((d >> sh) & 0xff) * inv / 255) & 0xff;
        0xff00_0000 | (mix(16) << 16) | (mix(8) << 8) | mix(0)
    }

    #[test]
    fn division_identity_is_exact() {
        for t in 0..=65280u32 {
            assert_eq!((t + (t >> 8) + 1) >> 8, t / 255, "t = {t}");
        }
    }

    #[test]
    fn over_premul_matches_reference() {
        // Deterministic LCG so the run is reproducible; 2 M cases across every alpha.
        let mut x = 0x2545_f491u32;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        };
        for a in 0..=255u32 {
            for _ in 0..8000 {
                let p = next() & 0x00ff_ffff;
                let d = next() | 0xff00_0000;
                assert_eq!(over_premul(p, d, a), reference(p, d, a), "p {p:#x} d {d:#x} a {a}");
            }
        }
    }
}

/// The cached control layer: [`draw_controls`] keyed by [`controls_key`].
///
/// ★★ THE LIVE STICK IS NOT IN THE LAYER (2026-09-19, player: *"조이스틱 이동이 렉이 좀
/// 걸린거같아"*). The first cut hashed [`JoyView`]'s exact `f64` bits into
/// [`controls_key`], so a thumb that moved one pixel invalidated the whole layer: every
/// present during a drag paid `draw_controls` (per-pixel `sqrt` + smoothstep over every
/// button) PLUS [`CachedLayer::rasterise`]'s one full-window scan for the new runs. That
/// is the cost the cache exists to remove, reappearing exactly while the player steers.
///
/// Measured on S9 (`<S9_SERIAL>`, field, standing still, `top -H`) while dragging
/// the stick 22 px — INSIDE the walk deadzone, so the guest had nothing extra to do:
///
/// | | idle | dragging |
/// |---|---|---|
/// | `android_main` | 17.2~18.3 % | **75.0~77.3 %** |
/// | `wie-emu` | 58.6~64.6 % | **37.9~44.0 %** |
/// | guest fps | 26.0 | 23.6 |
///
/// The emulator lost a third of its CPU to the HUD painter *because the player touched
/// the stick*. So the layer now holds only what a drag does not change — the IDLE stick
/// hint included, which is static — and the live base + thumb are painted straight into
/// the window afterwards ([`draw_joystick_live`]): two discs and a ring bounded by the
/// stick's own box, against a whole-window re-rasterise. The key carries `live` (is a
/// finger down) and nothing else about the stick, so the layer is rebuilt once on press
/// and once on release.
#[derive(Default)]
pub struct ControlsLayer(CachedLayer);

impl ControlsLayer {
    /// Draw the controls into `dst`, rasterising them first if anything changed.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        dst: &mut [u32],
        win_w: u32,
        win_h: u32,
        layout: &Layout,
        pressed: &HashSet<KeyCode>,
        joy: Option<JoyView>,
        keypad_open: bool,
    ) {
        let live = joy.is_some();
        let key = controls_key(win_w, win_h, layout, pressed, live, keypad_open);
        self.0
            .draw(dst, win_w, win_h, key, |buf, w, h| draw_controls(buf, w, h, layout, pressed, live, keypad_open));
        // ★ Straight into the window, not into the layer — see the note on this type.
        if let (Some(zone), Some(view)) = (layout.joystick, joy) {
            draw_joystick_live(dst, win_w, win_h, &zone, view);
        }
    }
}

/// The cached quickbar layer: [`draw_quickbar`] keyed by [`quickbar_key`] (#4 remainder,
/// 2026-09-19).
///
/// simpleperf on S9 after the control layer went in still had `blend_disc_soft` 20 ·
/// `blend_ring_soft` 7 · `blit_icon` 6 % of the UI thread — all of it the seven plates,
/// re-rasterised every present. While a cooldown runs its fraction changes every guest
/// tick and the key with it, so that plate set is re-drawn ~20 times a second — the
/// same work as before, for the one interval that actually animates.
#[derive(Default)]
pub struct QuickbarLayer(CachedLayer);

impl QuickbarLayer {
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        dst: &mut [u32],
        win_w: u32,
        win_h: u32,
        rects: &[Rect],
        slots: &[QuickSlot],
        pressed: Option<usize>,
        cooldowns: &[f64],
    ) {
        let key = quickbar_key(win_w, win_h, rects, slots, pressed, cooldowns);
        self.0
            .draw(dst, win_w, win_h, key, |buf, w, h| draw_quickbar(buf, w, h, rects, slots, pressed, cooldowns));
    }
}

/// Everything [`draw_quickbar`] reads, hashed — see [`controls_key`] for the contract.
/// The atlas is immutable for the process and `quickslot_inert` is a function of the
/// slot plus the build, so neither is an input.
fn quickbar_key(win_w: u32, win_h: u32, rects: &[Rect], slots: &[QuickSlot], pressed: Option<usize>, cooldowns: &[f64]) -> u64 {
    let mut h = DefaultHasher::new();
    (win_w, win_h, crate::immersive_active(), pressed).hash(&mut h);
    // ★ `draw_quickbar` asks `crate::quickslot_inert`, whose answer depends on the BUILD
    // (`guest_build::KIND`, an atomic the worker stores once at boot) and not on anything
    // else hashed here — so a layer rasterised before the kind is known would keep its
    // dimmed host plates afterwards. The same contract as `controls_key`: every input the
    // painter reads is an input here.
    crate::guest_build::hero2_java().is_some().hash(&mut h);
    rects.hash(&mut h);
    slots.hash(&mut h);
    // Exact bits: a fraction that moved by less than a pixel still re-rasterises, which
    // keeps the layer pixel-identical to drawing straight into the window.
    cooldowns.len().hash(&mut h);
    for f in cooldowns {
        f.to_bits().hash(&mut h);
    }
    h.finish()
}

/// Everything [`draw_controls`] reads, hashed. Two frames with the same key draw the
/// same pixels — that is the ONLY claim this cache rests on, so a new input to
/// `draw_controls` has to be added here in the same commit.
fn controls_key(
    win_w: u32,
    win_h: u32,
    layout: &Layout,
    pressed: &HashSet<KeyCode>,
    live: bool,
    keypad_open: bool,
) -> u64 {
    let mut h = DefaultHasher::new();
    (win_w, win_h, keypad_open, crate::immersive_active()).hash(&mut h);
    (layout.gear, layout.keypad_toggle, layout.joystick).hash(&mut h);
    layout.buttons.len().hash(&mut h);
    for b in &layout.buttons {
        // `wedge` is hit-testing only and paints nothing.
        //
        // ★★ PER BUTTON, `pressed.contains(&b.key)` — exactly what the painter asks
        // (2026-09-20, review). This used to hash the rect/face/label here and XOR-fold
        // the WHOLE `pressed` set below, which was wrong in both directions. Too coarse:
        // `App::refresh_pressed` puts the joystick's walk key in `pressed`, and in touch
        // type A no `Button` carries it — so every settled turn changed the key while
        // `draw_controls` painted identical pixels, and the layer was re-rasterised (ten
        // per-pixel discs and rings, plus a full-window run scan) exactly while the
        // player steers, which is the cost this cache exists to remove. Too loose: `key`
        // itself was not hashed at all, so a relayout that re-aims a seat at a different
        // KeyCode without moving it kept the old highlight.
        (b.rect, b.dpad, b.face, b.label, pressed.contains(&b.key)).hash(&mut h);
    }
    // ★ `live`, not the JoyView (2026-09-19). The base and thumb POSITIONS are not an
    // input to `draw_controls` any more — the live stick is painted into the window, not
    // into the layer (see [`ControlsLayer`]). What the layer still owes the stick is the
    // idle hint, which is drawn only when `!live`, so the flag is the whole dependency.
    live.hash(&mut h);
    h.finish()
}

pub fn draw_controls(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    layout: &Layout,
    pressed: &HashSet<KeyCode>,
    live: bool,
    keypad_open: bool,
) {
    for b in &layout.buttons {
        let base = if pressed.contains(&b.key) {
            BTN_PRESSED
        } else if b.dpad {
            BTN_DPAD
        } else {
            BTN
        };
        match b.face {
            // The big ACTION disc keeps its inner gold ring: it is the one key that
            // should read as "the" button. Every other key gets the standard face.
            Face::Action => {
                let (cx, cy, rad) = draw_round_face(buf, win_w, win_h, b.rect, base, 0xffe0_9a3c, 95, 180);
                blend_ring_soft(buf, win_w, win_h, cx, cy, rad * 0.72, (rad / 16.0).max(1.4), 0xffe0_9a3c, 140);
            }
            Face::Round => {
                draw_round_face(buf, win_w, win_h, b.rect, base, BORDER, 92, 165);
            }
            Face::Solid => {
                let rad = f64::from(b.rect.w.min(b.rect.h)) / 2.0;
                let cx = f64::from(b.rect.x) + f64::from(b.rect.w) / 2.0;
                let cy = f64::from(b.rect.y) + f64::from(b.rect.h) / 2.0;
                fill_disc(buf, win_w, win_h, cx, cy, rad, base);
                blend_ring_soft(buf, win_w, win_h, cx, cy, rad - 1.0, 2.0, BORDER, 255);
            }
            // The INK rect, not the button's — the arrows are hit-tested over a wider
            // quarter than they are drawn in (see `dpad_arms`).
            Face::Hexagon(dir, ink) => {
                draw_hex_face(buf, win_w, win_h, ink, dir, base, BORDER, 92, 165);
                draw_hex_glyph(buf, win_w, win_h, ink, dir, LABEL, 215);
            }
            Face::None => {}
        }
        if b.face == Face::None {
            continue; // the widget paints its own face and label (minimap panel)
        }
        if matches!(b.face, Face::Hexagon(..)) {
            continue; // the face painted its own glyph
        }
        // A round face has to size its label against the circle's INSCRIBED SQUARE,
        // not the button rect: "CLR" sized to the full rect overhangs the disc on
        // both sides (device-confirmed 2026-07-30).
        let ink = LABEL;
        let lbl = label_box(b.rect);
        match b.label {
            Label::Text(t) => draw_centered_text(buf, win_w, win_h, lbl, t, ink),
            Label::Arrow(dir) => {
                let s = (b.rect.w.min(b.rect.h) / 4).max(6);
                draw_arrow(buf, win_w, win_h, b.rect.x + b.rect.w / 2, b.rect.y + b.rect.h / 2, s, dir, ink);
            }
        }
    }
    if let Some(zone) = layout.joystick
        && !live
    {
        // The IDLE hint only. The live stick is [`draw_joystick_live`], painted into the
        // window after this layer is composited — see [`ControlsLayer`].
        draw_joystick_idle(buf, win_w, win_h, &zone);
    }
    draw_gear(buf, win_w, win_h, layout.gear);
    // Keypad toggle (immersive only — zero-sized otherwise).
    let t = layout.keypad_toggle;
    if t.w > 0 && t.h > 0 {
        draw_round_face(buf, win_w, win_h, t, if keypad_open { BTN_PRESSED } else { BTN }, BORDER, 92, 165);
        draw_centered_text(buf, win_w, win_h, label_box(t), "123", LABEL);
    }
}

/// The stick with NO finger on it. Cached with the rest of the controls
/// ([`ControlsLayer`]) because nothing about it moves.
///
/// The idle stick is only a HINT about where to put your thumb — a touch anywhere
/// walks — so it is deliberately faint (player request 2026-07-30: the old placeholder
/// read as a real control). TOUCH TYPE A only: under B there is no stick at all (the
/// layout publishes no zone, so this is never reached), and a cutscene has no zone
/// either. The classic (non-immersive) stick has no idle state — it is drawn only
/// while a finger is down — so this does nothing there.
fn draw_joystick_idle(buf: &mut [u32], win_w: u32, win_h: u32, _zone: &Rect) {
    if !crate::immersive_active() {
        return;
    }
    let (hx, hy, hr) = joy_home(win_w, win_h);
    blend_disc(buf, win_w, win_h, hx, hy, hr, 0xff1a_2436, 18);
    blend_ring(buf, win_w, win_h, hx, hy, hr, BORDER, 34);
    blend_disc(buf, win_w, win_h, hx, hy, hr * JOY_THUMB, 0xff9f_b6d4, 40);
}

/// The stick with a finger on it. Painted STRAIGHT INTO THE WINDOW, never into the
/// cached layer — see [`ControlsLayer`] for the measurement that put it here.
///
/// ★ hero3 91차-B: the base is cut from the STICK seat — the whole arrows' square — and
/// the live stick spawns under the finger at that size ([`arm_centre_radius`]: the circle
/// through the arrows' own centres, 2026-09-20). The walk-start distance is NOT this
/// radius ([`joy_engage_radius`]), so the base's size does not bring a dead feel with it.
/// While a finger is down the stick is brighter than the idle hint so the deflection
/// stays legible.
pub fn draw_joystick_live(buf: &mut [u32], win_w: u32, win_h: u32, zone: &Rect, view: JoyView) {
    let (cx, cy) = view.center;
    let (tx, ty) = view.thumb;
    if crate::immersive_active() {
        let hr = joy_home(win_w, win_h).2;
        blend_disc(buf, win_w, win_h, cx, cy, hr, 0xff1a_2436, 55);
        blend_ring(buf, win_w, win_h, cx, cy, hr, BORDER, 90);
        // The thumb is pinned INSIDE the base by `App` (`joy_radius`), so drawn at its
        // own radius it never crosses the ring.
        blend_disc(buf, win_w, win_h, tx, ty, hr * JOY_THUMB, 0xff9f_b6d4, 105);
        return;
    }
    // On-demand floating stick: nothing is drawn until a finger is down, then the
    // base renders centred at the touch point (so "first press = centre", with no
    // fixed idle base that reads as a stick pinned to one spot).
    let r = joy_radius(zone, win_w, win_h);
    // Base disc (opaque — softbuffer does not alpha-blend) + rim.
    fill_disc(buf, win_w, win_h, cx, cy, r, 0xff26_406a);
    ring(buf, win_w, win_h, cx, cy, r, BORDER);
    // Thumb at the finger.
    let tr = r * 0.42;
    fill_disc(buf, win_w, win_h, tx, ty, tr, BTN_PRESSED);
    ring(buf, win_w, win_h, tx, ty, tr, BORDER);
}

fn fill_disc(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, r: f64, color: u32) {
    if r <= 0.0 || (color >> 24) == 0 {
        return;
    }
    let r2 = r * r;
    let x0 = (cx - r).max(0.0) as u32;
    let x1 = ((cx + r) as u32).min(win_w.saturating_sub(1));
    let y0 = (cy - r).max(0.0) as u32;
    let y1 = ((cy + r) as u32).min(win_h.saturating_sub(1));
    for y in y0..=y1 {
        let dy = y as f64 + 0.5 - cy;
        for x in x0..=x1 {
            let dx = x as f64 + 0.5 - cx;
            if dx * dx + dy * dy <= r2 {
                buf[(y * win_w + x) as usize] = color;
            }
        }
    }
}

fn ring(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, r: f64, color: u32) {
    if r <= 1.0 {
        return;
    }
    let n = ((2.0 * std::f64::consts::PI * r) as u32).max(24);
    let thick = (r / 22.0).max(1.5);
    for i in 0..n {
        let a = i as f64 / n as f64 * std::f64::consts::TAU;
        let (s, c) = a.sin_cos();
        let mut t = -thick;
        while t <= thick {
            let px = cx + c * (r + t);
            let py = cy + s * (r + t);
            if px >= 0.0 && py >= 0.0 && (px as u32) < win_w && (py as u32) < win_h {
                buf[(py as u32 * win_w + px as u32) as usize] = color;
            }
            t += 1.0;
        }
    }
}

// ── hero2's OWN window palette (#11, measured on the device 2026-09-20, S9) ──
//
// Read off THIS game drawing its own field menu (캐릭터 / INVENTORY / EQUIPMENT …)
// and its slot screen, at 2220x1080 = logical x4.91:
//
//   sides   [1gp #3d2f1b][1gp #9b8a71][ body #6a5b44 ]
//   top/bot [8gp #000000 band][2gp #3d2f1b][ body ][2gp #3d2f1b][8gp band]
//   rows    #000000 plates on the body, 11gp tall with a 1gp gap
//   ink     #7f7f7f unlit (INVENTORY …) / #ffffff lit (캐릭터, the cursor row)
//   corners a metal stud in each corner of the body
//
// ⛔ These are NOT hero3's `G_*`. Those were sampled from hero3's window ART —
// parchment, a brown scroll band, a checker weave — and this game has none of it:
// hero2 builds its windows out of flat `drawRect` / `drawLine` fills, so the values
// below ARE the window rather than a likeness of one. Copying the other game's
// palette here would be the `icons.bin` mistake with colours instead of sprites.
const H2_BAND: u32 = 0xff00_0000;
const H2_EDGE: u32 = 0xff3d_2f1b;
const H2_BEVEL: u32 = 0xff9b_8a71;
const H2_BODY: u32 = 0xff6a_5b44;
const H2_INK: u32 = 0xffff_ffff;
const H2_INK_DIM: u32 = 0xff7f_7f7f;
/// The game's emphasis ink, sampled from 달성도 on the slot screen. ⛔ Not its
/// `#ffff00` — that yellow is the difficulty word (NORMAL) and reads as a warning.
const H2_GOLD: u32 = 0xffdc_c364;
const H2_RIVET_RIM: u32 = 0xffbf_bfbf;
const H2_RIVET_RING: u32 = 0xff7f_7f7f;
const H2_RIVET_CORE: u32 = 0xff1f_1f3f;

/// One GUEST pixel, in window pixels. The chrome above was measured in the game's
/// own 452-wide frame, so every part of it is sized in this unit — that is what
/// makes the panel carry the same weight as the window it is imitating instead of
/// thinning to a hairline on a big screen.
fn h2_unit(win_w: u32) -> u32 {
    (win_w / 452).clamp(2, 10)
}

/// The stud the game puts in each corner of a window body.
fn h2_rivet(buf: &mut [u32], win_w: u32, win_h: u32, cx: u32, cy: u32, r: u32) {
    if r == 0 {
        return;
    }
    fill_circle(buf, win_w, win_h, cx, cy, r, H2_BAND);
    fill_circle(buf, win_w, win_h, cx, cy, r * 3 / 4, H2_RIVET_RING);
    fill_circle(buf, win_w, win_h, cx, cy, r / 2, H2_RIVET_CORE);
    // The lit rim is off-centre towards the top left, which is where the game puts
    // the highlight on every round thing it draws.
    let off = (r / 3).max(1);
    fill_circle(buf, win_w, win_h, cx.saturating_sub(off), cy.saturating_sub(off), (r / 4).max(1), H2_RIVET_RIM);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_settings(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    items: &[SettingsItem],
    landscape_active: bool,
    joystick_active: bool,
    immersive_pref: bool,
    base_h: u32,
    // 발열 절감 preference (57차) — drives the HEAT SAVER row's face + state line.
    heat_saver: bool,
    // Short ASCII tag for the configured server — "LOCAL" for the loopback dev
    // loop, "REMOTE" for anything else. See `SKind::Server`.
    server_label: &str,
    // ★ 2026-09-24: how far the row list is scrolled (window px) — the scrollbar.
    scroll: u32,
) {
    // Dim everything behind the modal (read + darken each pixel).
    for px in buf.iter_mut() {
        let v = *px;
        let r = ((v >> 16) & 0xff) * 4 / 10;
        let g = ((v >> 8) & 0xff) * 4 / 10;
        let b = (v & 0xff) * 4 / 10;
        *px = 0xff00_0000 | (r << 16) | (g << 8) | b;
    }

    // ── the GAME'S own window chrome (#11, 2026-09-20) ───────────────────────
    //
    // Player: *"상단 톱니바퀴 설정이 현재 hero3랑 뭔가 다른거같은데? 디자인도 다르고"*.
    // The ROWS and the GEOMETRY were aligned with hero3 on 2026-09-18; what stayed
    // the fork's own until now was the PAINT — a dark blue host modal with a blue
    // button face, two centred lines per row and an X close box, lettered in a 5x7
    // ASCII font plus a 12x12 closed Hangul list.
    //
    // hero3's answer could not be copied, in either half:
    //
    //  * its COLOURS are sampled from hero3's window art, which this game does not
    //    have — so they were measured off hero2's own windows instead (`H2_*`).
    //  * its LETTERING is hero3's game font, and hero2's archive carries no font at
    //    all — so it goes through `crate::hostfont` (see that module).
    //
    // What IS shared with hero3 is the SHAPE of a row: name on the left, current
    // state on the right, one line. Two centred lines cannot be made big enough on
    // a phone, and one line across is also the shape the game's own option rows have.
    let p = settings_panel(win_w, win_h);
    let u = h2_unit(win_w);
    let band = u * 8;
    let line = u * 2;

    // Sides first, then the body over them: [1gp dark][1gp bevel][body].
    fill(buf, win_w, win_h, p.x, p.y, p.w, p.h, H2_EDGE);
    fill(buf, win_w, win_h, p.x + u, p.y + u, p.w.saturating_sub(u * 2), p.h.saturating_sub(u * 2), H2_BEVEL);
    let (ix, iy) = (p.x + u * 2, p.y + u * 2);
    let (iw, ih) = (p.w.saturating_sub(u * 4), p.h.saturating_sub(u * 4));
    fill(buf, win_w, win_h, ix, iy, iw, ih, H2_BODY);

    for item in items {
        let r = item.rect;
        // Every row is the same plate; only the words and the state differ. One
        // drawing path is what stops the rows drifting apart as they are added.
        let (title, sub, state) = match item.kind {
            // 닫기 sits ON the black title bar, so unlike every other row it needs a
            // frame: a black plate on a black bar is not a control. The frame is the
            // body colour, which is the same inset idiom the game's own rows use —
            // there it is the plate that is dark and the surround that is khaki.
            // (Drawn after the chrome, below — it sits ON the title bar.)
            SKind::Close => continue,
            // The layout editor. Lit once the player has actually moved something,
            // so "did my layout save?" is answerable from here.
            SKind::EditLayout => {
                let custom = crate::placement::is_customised();
                let st = if !item.enabled {
                    RowState::Disabled
                } else if custom {
                    RowState::Active
                } else {
                    RowState::Normal
                };
                // Unavailable outside landscape full-screen — say so on the row
                // itself instead of only in logcat, or it reads as a dead control.
                let sub = if !item.enabled {
                    "가로 전용"
                } else if custom {
                    "사용자 배치"
                } else {
                    "편집"
                };
                ("화면 배치", sub, st)
            }
            // Diagnostics — and the way to the save vault's restore (2026-09-19) and, since
            // 2026-10-02, to 세이브 옮기기 (export / import): the dialog's 「세이브 관리」 button.
            SKind::Diag => ("진단 정보", "세이브 관리", RowState::Normal),
            // ★ The effect bus's own trim, because one global number cannot be right
            // on two handsets. `기본` is what every build before it sounded like.
            SKind::SfxLevel => {
                let lv = crate::sfx_level();
                let st = if lv == crate::audio_mix::SFX_LEVEL_DEFAULT { RowState::Normal } else { RowState::Active };
                ("효과음 크기", crate::audio_mix::sfx_level_label(lv), st)
            }
            // ★ 경험치 배율 (2026-09-23). Lit while off 기존: this is the one row that
            // changes the game's rules, so "have I changed anything?" matters most here.
            // ★ 오토루팅 (2026-09-24). Lit while ON, like 발열 절감.
            SKind::AutoLoot => {
                let on = crate::auto_loot();
                ("오토루팅", if on { "켬" } else { "끔" }, if on { RowState::Active } else { RowState::Normal })
            }
            SKind::ExpRate => {
                let m = crate::exp_rate();
                let st = if m == crate::guest_build::EXP_RATE_DEFAULT { RowState::Normal } else { RowState::Active };
                ("경험치 배율", crate::guest_build::exp_rate_label(m), st)
            }
            // TOUCH TYPE. The state line names the state rather than the action: a
            // row that said "십자키로" would be ambiguous about which one is live.
            SKind::TouchType => {
                let t = crate::touch_type();
                let st = if !item.enabled {
                    RowState::Disabled
                } else if t != crate::TOUCH_FREE {
                    RowState::Active
                } else {
                    RowState::Normal
                };
                // The user's own names (2026-09-24) — the same three the game's 환경설정
                // row shows (there in upper case: the game font has no lower case).
                let sub = if !item.enabled {
                    "가로 전용"
                } else {
                    match t {
                        crate::TOUCH_FIXED_A => "fixed_A",
                        crate::TOUCH_FIXED_B => "fixed_B",
                        _ => "free",
                    }
                };
                ("조작 방식", sub, st)
            }
            // 발열 절감 (57차). Lit while ON: the panel's standing job is to answer
            // "have I changed anything?" at a glance.
            SKind::HeatSaver => (
                "발열 절감",
                if heat_saver { "켬" } else { "끔" },
                if heat_saver { RowState::Active } else { RowState::Normal },
            ),
            // 게임패드 HUD (59차). AUTO is the default and is NOT lit even though it
            // may be showing the pad HUD right now — the highlight is about the
            // SETTING, not the current screen.
            SKind::PadHud => {
                let pref = crate::pad_hud_pref();
                let sub = match pref {
                    crate::padhud::PadHud::Auto => "자동",
                    crate::padhud::PadHud::On => "항상 켬",
                    crate::padhud::PadHud::Off => "끔",
                };
                let st = if pref == crate::padhud::PadHud::Auto { RowState::Normal } else { RowState::Active };
                ("패드 표시", sub, st)
            }
            // 패드 버튼 설정 (2026-10-09). Lit while the table differs from the built-in
            // layout — the panel's standing "have I changed anything?" answer.
            SKind::PadMap => {
                if crate::gamepad::remapped() {
                    ("패드 버튼 설정", "사용자 배치", RowState::Active)
                } else {
                    ("패드 버튼 설정", "기본", RowState::Normal)
                }
            }
            // Rows that were removed from the panel but whose actions are retained
            // (see `SettingsAction`). They never reach here; naming them keeps the
            // match exhaustive without a catch-all that would hide a new kind.
            SKind::Save => ("빠른 저장", "지금", RowState::Active),
            SKind::Portrait => ("세로", "", RowState::Normal),
            SKind::Landscape => ("가로", "", RowState::Normal),
            SKind::Dpad => ("십자키", "", RowState::Normal),
            SKind::Joystick => ("스틱", "", RowState::Normal),
            SKind::Immersive => ("전체 화면", if immersive_pref { "켬" } else { "끔" }, RowState::Normal),
            SKind::BaseH => ("화면 높이", "", RowState::Normal),
            SKind::Server => ("서버", server_label, RowState::Normal),
            SKind::Account => ("계정 코드", "", RowState::Normal),
        };
        // Retained for the rows above that are no longer built into the panel.
        let _ = (landscape_active, joystick_active, base_h);
        draw_h2_row(buf, win_w, win_h, r, title, sub, state);
    }

    // ── the chrome, OVER the rows (2026-09-24) ─────────────────────────────────
    // Drawn after them so a row the scroll has cut in half is cut by the frame: the
    // title bar covers everything above the list's window, the body and the bottom
    // band everything below it. Every fill here is opaque, which is what makes the
    // order a clip.
    // ⛔ The bands are drawn INSIDE that frame, where the game runs them the full
    // width of the window. The game can: its windows always sit on the map, so a
    // black band has bright pixels to be a band against. This one sits on a screen
    // this very function has just dimmed to 40 %, and a black band bleeding into a
    // near-black backdrop took the panel's whole top edge away (device, first cut).
    // Two units of brown all the way round is what says where the window ends.
    //
    // ★ And the TOP band is grown to cover the header strip and carry the title,
    // instead of being 8gp of decoration with a separate title plate below it: both
    // are black, so at the measured height the two merged into one smudge with a
    // khaki sliver trapped between them. A black bar with the name in it is what the
    // game's own titled windows look like anyway.
    let head = settings_head(p);
    let top = band.max(head.saturating_sub(u * 2));
    fill(buf, win_w, win_h, ix, iy, iw, top, H2_BAND);
    fill(buf, win_w, win_h, ix, iy + top, iw, line, H2_EDGE);
    // The bottom band takes what the row layout left over, not the measured 8gp: the
    // rows reserve `p.h/24` below themselves and the band plus its line is wider than
    // that, so at the measured height the last row sat flush against it with three
    // pixels of khaki showing — read on the device as a row half swallowed by the
    // frame. The rows own that space; the band gets the remainder.
    // ★ 2026-09-24: the rows' END is the list window's, not the last row's — the rows
    // scroll, and a row cut by the window must not move the band.
    let view = settings_list_view(win_w, win_h);
    let rows_end = view.y + view.h;
    let floor = (iy + ih).saturating_sub(rows_end + u * 2 + line);
    let band_b = band.min(floor).max(u);
    let bot = iy + ih.saturating_sub(band_b);
    // Body over whatever a half-scrolled row put between the window and the band.
    fill(buf, win_w, win_h, ix, rows_end, iw, bot.saturating_sub(rows_end), H2_BODY);
    fill(buf, win_w, win_h, ix, bot, iw, band_b, H2_BAND);
    fill(buf, win_w, win_h, ix, bot.saturating_sub(line), iw, line, H2_EDGE);

    // The studs, in the four corners of the BODY (not of the window — the bands are
    // in front of them, which is how the game's own corners sit).
    let rr = u * 5 / 2;
    let inset = u * 2;
    let (bx0, bx1) = (ix + inset, ix + iw.saturating_sub(inset));
    let (by0, by1) = (iy + top + line + inset, bot.saturating_sub(line + inset));
    for (cx, cy) in [(bx0, by0), (bx1, by0), (bx0, by1), (bx1, by1)] {
        h2_rivet(buf, win_w, win_h, cx, cy, rr);
    }

    // The title, centred on the bar. It is centred on the ROWS' span rather than on
    // the panel, so it does not drift towards the 닫기 plate as that plate grows.
    if let Some(r0) = items.iter().find(|i| i.kind != SKind::Close).map(|i| i.rect) {
        let s = crate::hostfont::fit_scale("설정", r0.w * 80 / 100, top * 52 / 100);
        crate::hostfont::draw_centered(
            buf,
            win_w,
            win_h,
            r0.x,
            r0.w,
            iy + top.saturating_sub(crate::hostfont::CELL_H * s) / 2,
            s,
            "설정",
            H2_INK,
        );
    }

    // 닫기 sits ON the black title bar, so unlike every other row it needs a frame: a
    // black plate on a black bar is not a control. The frame is the body colour, which
    // is the same inset idiom the game's own rows use.
    if let Some(r) = items.iter().find(|i| i.kind == SKind::Close).map(|i| i.rect) {
        fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, H2_BODY);
        let inner = Rect::new(r.x + u, r.y + u, r.w.saturating_sub(u * 2), r.h.saturating_sub(u * 2));
        draw_h2_row(buf, win_w, win_h, inner, "닫기", "", RowState::Normal);
    }

    // The scrollbar, only while there are rows to scroll to: a groove in the frame's
    // dark brown with a bevel-coloured thumb, in the right margin between the plates
    // and the studs — the window's own two edge colours, nothing new.
    let max = settings_scroll_max(win_w, win_h);
    if max > 0 {
        let g = settings_geom(win_w, win_h);
        let right = g.x0 + g.bw;
        let gx = right + (ix + iw).saturating_sub(right) / 2;
        let gw = u * 2;
        let (ty, th) = (view.y, view.h);
        fill(buf, win_w, win_h, gx.saturating_sub(gw / 2), ty, gw, th, H2_EDGE);
        let content = th + max;
        let thumb = (u64::from(th) * u64::from(th) / u64::from(content)) as u32;
        let at = ty + (u64::from(th - thumb) * u64::from(scroll.min(max)) / u64::from(max)) as u32;
        fill(buf, win_w, win_h, gx.saturating_sub(gw / 2), at, gw, thumb.max(u * 4), H2_BEVEL);
    }
}

// ── 설정 → 패드 버튼 설정 (2026-10-09) — the in-game HUD, lit ──────────────────────
//
// User: *"상단 톱니바퀴 설정으로 패드 키를 맵핑하는 기능을 넣을꺼야 … hero2에도 마찬가지로
// 넣어야해"*, and the same day, for both games: *"패드 설정버튼 화면을 줄을 누르고 패드버튼을
// 누르는게 아니라 그냥 실제 인게임 화면상에 버튼들을 하이라이트 처리하고 각 버튼을 눌르고 패드
// 버튼을 누르는 방식으로 변경해줘. hero2도 마찬가지야"*.
//
// So there is no list (the first form was the settings window with a scrolling list of
// functions). The screen is the FIELD HUD as it is played — the same controls, in the seats
// the resolver gives them, painted by the same painters (`draw_controls`, `draw_quickbar`,
// `draw_top_tab`, the PAGE mask) — over the live frame dimmed, and every control a pad
// button can be given is ringed in gold and stamped with the badge of the button that fires
// it IN THE DRAFT (the table being edited, not the one in force). Tap a control, press the
// pad button. A function with no control on screen — the 회복 아이템 keys (`0` / `#`, whose
// button went on 2026-09-25) and any plate the player has hidden — is a CELL in the screen's
// own small window: this game's window (black title band with 취소 · 저장, khaki body,
// studs), holding the two status lines, the cells and 기본값. hero3 does the same with its
// own chrome; the rules are `padmap.rs`, shared byte for byte.
//
// ★ The FIELD set, as the layout editor shows it: the gear only exists OFF the field, so the
//   HUD this is opened from is never the one being configured.
// ⛔ Not drawn: the walk control and the minimap panel. A direction is not remappable
//   (`padmap::capturable`), and `*` rides the MAP tab — where its badge sits in play.

/// The action disc's gold (the 퀵바등록 rings'): "a control you can give a button".
const PM_OFFER: u32 = 0xffe0_9a3c;
/// The badges' lit gold: "this one is waiting for a button" / "this is the one you changed".
const PM_SEL: u32 = BADGE_LIT;
/// "What the pad button you just pressed does" — a bare press, nothing picked.
const PM_PROBE: u32 = 0xff6c_d0ff;

/// How a target is drawn and lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadTargetShape {
    /// A disc control: OK, CLR, a plate, PAGE.
    Round,
    /// A flat one: SAVE, MAP, a key of the classic keypad layouts.
    Square,
    /// No control on screen: a cell in the window ([`PadMapPanel`]).
    Chip,
}

/// One thing the pad screen offers a button for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadTarget {
    /// Index into `gamepad::REMAP_ROWS`.
    pub row: usize,
    pub rect: Rect,
    pub shape: PadTargetShape,
}

/// The pad screen's own window, in window px ([`padmap_panel`]).
#[derive(Clone, Debug)]
pub struct PadMapPanel {
    pub panel: Rect,
    /// The black title band and the two plates on it.
    band: Rect,
    cancel: Rect,
    save: Rect,
    /// The plate holding the two status lines.
    status: Rect,
    /// The cells of the functions with no control on screen and their captions, in the
    /// order their targets follow the on-screen ones in [`PadMapScene::targets`].
    chips: Vec<(Rect, String)>,
    reset: Rect,
    /// The body's corners, for the studs.
    body: Rect,
    /// One guest pixel ([`h2_unit`]) and the font scale.
    u: u32,
    ts: u32,
}

/// Everything the pad screen draws and answers to, for one window size: the FIELD HUD's own
/// pieces, the targets on them, and the window. Rebuilt per frame and per touch from the
/// resolver, so where a control is lit is always where it is played.
pub struct PadMapScene {
    /// The FIELD layout's buttons that are pad functions — OK, CLR (and on the classic
    /// keypad layouts `0`, `#`, `*`); no gear, no arrows, no stick.
    layout: Layout,
    /// The plates by plate index — zero-sized = hidden (`quick_buttons`' contract).
    quick: Vec<Rect>,
    page: Option<Rect>,
    save: Option<Rect>,
    map: Option<Rect>,
    /// The controls on screen first, then the chips.
    pub targets: Vec<PadTarget>,
    pub panel: PadMapPanel,
}

/// What the pad screen lights — `App`'s state, resolved before the surface is borrowed.
pub struct PadMapLook<'a> {
    /// One per target, in [`PadMapScene::targets`] order: the position of the button that
    /// fires it in the DRAFT (`None` = no button, or one with no known position).
    pub badges: &'a [Option<crate::padhud::PadPos>],
    /// The function picked last (pale gold), and the one a bare press just named (blue).
    pub sel: Option<usize>,
    pub probe: Option<usize>,
    /// The picked control's breath while it waits for a button, 0..=1; `None` = not waiting.
    pub pulse: Option<f64>,
    pub title: &'a str,
    pub lines: [&'a str; 2],
    /// Does the draft differ from the built-in layout — is there anything for 기본값 to do?
    pub can_reset: bool,
    /// Does it differ from the table in force — has 저장 anything to keep? (Lit gold.)
    pub dirty: bool,
}

/// What a touch on the pad screen is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadMapHit {
    Cancel,
    Reset,
    Save,
    /// A control on screen, or a chip: the `gamepad::REMAP_ROWS` index it offers.
    Row(usize),
    /// The window, but nothing on it.
    Panel,
}

/// A chip's caption: the function's name — and why it has no control on screen when that
/// was the player's own choice (a hidden plate).
fn padmap_chip_label(row: usize) -> String {
    let (action, label) = crate::gamepad::REMAP_ROWS[row];
    match action {
        crate::gamepad::PadAction::Quick(n) if crate::quickpage::plate_hidden(crate::quick_hidden(), n) => format!("{label} (숨김)"),
        _ => label.to_string(),
    }
}

/// The pad screen for this window: the FIELD controls a pad button can be given, in their
/// seats, and a chip in the window for every function left without one.
pub fn padmap_scene(win_w: u32, win_h: u32, joystick: bool) -> PadMapScene {
    use crate::gamepad::{PadAction, remap_row};
    use crate::placement::CtrlId;
    let mode = Mode::Field;
    let mut layout = layout(win_w, win_h, joystick, mode, false, false);
    // The plates BEFORE the gear goes: the classic layouts seat them below it.
    let quick = quick_buttons(layout.game, layout.gear, usize::from(crate::QUICK_SLOTS), mode);
    layout.buttons.retain(|b| !b.dpad && b.face != Face::None && remap_row(PadAction::Key(b.key)).is_some());
    layout.joystick = None;
    layout.gear = Rect::new(0, 0, 0, 0);
    layout.keypad_toggle = Rect::new(0, 0, 0, 0);
    let immersive = crate::immersive_active() && win_w > win_h;

    let mut targets: Vec<PadTarget> = Vec::new();
    // First come, first served — one control per function (a classic layout's `*` key and a
    // MAP tab never both stand, but the rule should not depend on that).
    let mut offer = |action: PadAction, rect: Rect, shape: PadTargetShape| {
        if rect.w == 0 || rect.h == 0 {
            return;
        }
        if let Some(row) = remap_row(action)
            && !targets.iter().any(|t| t.row == row)
        {
            targets.push(PadTarget { row, rect, shape });
        }
    };
    for b in &layout.buttons {
        let shape = match b.face {
            Face::Action | Face::Round | Face::Solid => PadTargetShape::Round,
            _ => PadTargetShape::Square,
        };
        offer(PadAction::Key(b.key), b.rect, shape);
    }
    for (i, r) in quick.iter().enumerate() {
        offer(PadAction::Quick(i as u8), *r, PadTargetShape::Round);
    }
    // The three host-owned controls of the immersive HUD, where the resolver put them — and
    // only the ones this build draws in play (`placement::shown`: SAVE needs a save route,
    // MAP the minimap).
    let (mut page, mut save, mut map) = (None, None, None);
    if immersive {
        let placed = crate::placement::resolve(mode, win_w, win_h);
        let at = |id: CtrlId| {
            placed
                .iter()
                .find(|(c, _)| *c == id)
                .map(|(_, r)| *r)
                .filter(|r| r.w > 0 && r.h > 0 && crate::placement::shown(mode, id))
        };
        page = at(CtrlId::QuickPage);
        save = at(CtrlId::QuickSave);
        map = crate::placement::shown(mode, CtrlId::Minimap).then(|| at(CtrlId::MinimapToggle)).flatten();
        if let Some(r) = page {
            offer(PadAction::QuickPage, r, PadTargetShape::Round);
        }
        if let Some(r) = save {
            offer(PadAction::QuickSave, r, PadTargetShape::Square);
        }
        // `*` — the key the MAP tab's badge names in play (the panel itself is not drawn here).
        if let Some(r) = map {
            offer(PadAction::Key(KeyCode::STAR), r, PadTargetShape::Square);
        }
    }

    let avoid: Vec<Rect> = targets.iter().map(|t| t.rect).collect();
    let missing: Vec<usize> = (0..crate::gamepad::REMAP_ROWS.len()).filter(|row| !targets.iter().any(|t| t.row == *row)).collect();
    let labels: Vec<String> = missing.iter().map(|row| padmap_chip_label(*row)).collect();
    let panel = padmap_panel(win_w, win_h, &avoid, &labels);
    for (row, (rect, _)) in missing.iter().zip(&panel.chips) {
        targets.push(PadTarget {
            row: *row,
            rect: *rect,
            shape: PadTargetShape::Chip,
        });
    }
    PadMapScene {
        layout,
        quick,
        page,
        save,
        map,
        targets,
        panel,
    }
}

/// The window: centred across the screen, as high as it can sit without covering a control —
/// the first free spot from the top (under the top-row tabs, as a rule) — or the middle of
/// the screen when every spot covers one.
///
/// ```text
///   ###########################################
///   # [취소]       패드 버튼 설정       [저장] #   black band, the settings window's
///   # o ------------------------------------ o #
///   #   [ what happened / what to do      ]    #   one black plate, two lines
///   #   [ 회복 아이템 (0) ●] [ … (#) ●]        #   the cells (functions off screen)
///   #                            [ 기본값 ]    #
///   # o                                    o #
///   ###########################################
/// ```
fn padmap_panel(win_w: u32, win_h: u32, avoid: &[Rect], chips: &[String]) -> PadMapPanel {
    let u = h2_unit(win_w);
    // The settings rows' letter size on a landscape phone (scale 2 at 1080 tall).
    let ts = (win_h.min(win_w) / 500).clamp(1, 4);
    let cell = crate::hostfont::CELL_H * ts;
    let w = (win_w * 46 / 100).clamp(360.min(win_w), win_w);
    let band = cell + u * 8;
    let pad = u * 3;
    let inset = u * 2; // edge + bevel
    let inner = w.saturating_sub(inset * 2 + pad * 2);
    let line = cell + u * 2;
    let status_h = line * 2 + u * 2;
    let chip_h = cell + u * 3;
    let gap = u * 2;
    // The chips, wrapped into rows: caption + a badge-sized square + margins.
    let chip_w = |t: &str| (crate::hostfont::text_w(t, ts) + chip_h + u * 4).min(inner);
    let mut rows: Vec<Vec<u32>> = Vec::new();
    let mut used = u32::MAX;
    for t in chips {
        let cw = chip_w(t);
        if used.saturating_add(gap + cw) > inner {
            rows.push(Vec::new());
            used = 0;
        } else {
            used += gap;
        }
        used += cw;
        if let Some(row) = rows.last_mut() {
            row.push(cw);
        }
    }
    let chip_block = rows.len() as u32 * (chip_h + gap);
    let reset_w = (crate::hostfont::text_w("기본값", ts) + u * 10).min(inner);
    let h = (inset + band + u * 2 + pad + status_h + gap + chip_block + chip_h + pad + inset).min(win_h);
    let x0 = win_w.saturating_sub(w) / 2;
    let margin = u * 2;
    let step = u.max(1);
    let free = |y: u32| {
        let r = Rect::new(x0, y, w, h).grown(margin);
        !avoid.iter().any(|a| a.intersects(&r))
    };
    let y0 = (0u32..)
        .map(|k| margin + k * step)
        .take_while(|y| y + h + margin <= win_h)
        .find(|y| free(*y))
        .unwrap_or(win_h.saturating_sub(h) / 2);
    let (ix, iy, iw) = (x0 + inset, y0 + inset, w.saturating_sub(inset * 2));
    let band_r = Rect::new(ix, iy, iw, band);
    // 취소 / 저장: the settings window's square 닫기 plate, at each end of the band.
    let plate = band.saturating_sub(u * 2);
    let pw = (crate::hostfont::text_w("취소", ts) + u * 8).max(plate);
    let cancel = Rect::new(ix + u, iy + u, pw, plate);
    let save = Rect::new((ix + iw).saturating_sub(u + pw), iy + u, pw, plate);
    let body_top = iy + band + u * 2;
    let status = Rect::new(ix + pad, body_top + pad, inner, status_h);
    let mut placed_chips = Vec::with_capacity(chips.len());
    let mut labels = chips.iter();
    for (k, row) in rows.iter().enumerate() {
        let row_w: u32 = row.iter().sum::<u32>() + gap * (row.len() as u32).saturating_sub(1);
        let mut x = ix + pad + inner.saturating_sub(row_w) / 2;
        let y = status.y + status.h + gap + k as u32 * (chip_h + gap);
        for cw in row {
            placed_chips.push((Rect::new(x, y, *cw, chip_h), labels.next().cloned().unwrap_or_default()));
            x += cw + gap;
        }
    }
    let reset_y = status.y + status.h + gap + chip_block;
    let reset = Rect::new((ix + pad + inner).saturating_sub(reset_w), reset_y, reset_w, chip_h);
    PadMapPanel {
        panel: Rect::new(x0, y0, w, h),
        band: band_r,
        cancel,
        save,
        status,
        chips: placed_chips,
        reset,
        body: Rect::new(ix, body_top, iw, (y0 + h).saturating_sub(inset + body_top)),
        u,
        ts,
    }
}

/// What a touch at `(x, y)` is on the pad screen. `None` = the game picture.
pub fn padmap_hit(sc: &PadMapScene, x: f64, y: f64) -> Option<PadMapHit> {
    let p = &sc.panel;
    // The window is drawn over whatever it covers, so it answers first.
    if p.panel.contains(x, y) {
        if p.cancel.contains(x, y) {
            return Some(PadMapHit::Cancel);
        }
        if p.save.contains(x, y) {
            return Some(PadMapHit::Save);
        }
        if p.reset.contains(x, y) {
            return Some(PadMapHit::Reset);
        }
        let chip = sc.targets.iter().find(|t| t.shape == PadTargetShape::Chip && t.rect.contains(x, y));
        return Some(chip.map_or(PadMapHit::Panel, |t| PadMapHit::Row(t.row)));
    }
    // Controls may overlap each other (2026-08-07); the smallest one under the finger is the
    // one a thumb can only have meant on purpose.
    sc.targets
        .iter()
        .filter(|t| t.shape != PadTargetShape::Chip && t.rect.contains(x, y))
        .min_by_key(|t| u64::from(t.rect.w) * u64::from(t.rect.h))
        .map(|t| PadMapHit::Row(t.row))
}

/// How one target is lit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PmLit {
    Offer,
    Sel,
    Probe,
}

fn padmap_lit(row: usize, look: &PadMapLook) -> PmLit {
    if look.sel == Some(row) {
        PmLit::Sel
    } else if look.probe == Some(row) {
        PmLit::Probe
    } else {
        PmLit::Offer
    }
}

/// A rectangle's outline `t` px thick, blended.
fn frame_blend(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, t: u32, color: u32, a: u32) {
    let t = t.min(r.w / 2).min(r.h / 2).max(1);
    blend_rect(buf, win_w, win_h, r.x, r.y, r.w, t, color, a);
    blend_rect(buf, win_w, win_h, r.x, (r.y + r.h).saturating_sub(t), r.w, t, color, a);
    blend_rect(buf, win_w, win_h, r.x, r.y + t, t, r.h.saturating_sub(t * 2), color, a);
    blend_rect(buf, win_w, win_h, (r.x + r.w).saturating_sub(t), r.y + t, t, r.h.saturating_sub(t * 2), color, a);
}

/// Ring one control: gold = it can be given a button; pale gold and washed = the one picked
/// (breathing while it waits for the button); blue = what the last bare press fires.
fn padmap_light(buf: &mut [u32], win_w: u32, win_h: u32, t: &PadTarget, lit: PmLit, pulse: Option<f64>) {
    let r = t.rect;
    let colour = match lit {
        PmLit::Offer => PM_OFFER,
        PmLit::Sel => PM_SEL,
        PmLit::Probe => PM_PROBE,
    };
    let breath = pulse.unwrap_or(0.0);
    if t.shape == PadTargetShape::Round {
        let rad = f64::from(r.w.min(r.h)) / 2.0;
        let cx = f64::from(r.x) + f64::from(r.w) / 2.0;
        let cy = f64::from(r.y) + f64::from(r.h) / 2.0;
        let th = (rad / 7.0).max(2.5);
        if lit == PmLit::Sel {
            blend_disc_soft(buf, win_w, win_h, cx, cy, rad, PM_SEL, 60 + (70.0 * breath) as u32, (rad * 0.08).max(1.0));
            if pulse.is_some() {
                blend_ring_soft(buf, win_w, win_h, cx, cy, rad + th * (1.4 + 1.6 * breath), th * 0.8, PM_SEL, (220.0 * (1.0 - 0.7 * breath)) as u32);
            }
        }
        let (thick, a) = if lit == PmLit::Offer { (th, 215) } else { (th * 1.5, 255) };
        blend_ring_soft(buf, win_w, win_h, cx, cy, rad + thick * 0.15, thick, colour, a);
    } else {
        let th = (r.w.min(r.h) / 10).max(3);
        if lit == PmLit::Sel {
            blend_rect(buf, win_w, win_h, r.x, r.y, r.w, r.h, PM_SEL, 50 + (70.0 * breath) as u32);
            if pulse.is_some() {
                let g = th * 2 + (f64::from(th) * 1.5 * breath) as u32;
                frame_blend(buf, win_w, win_h, r.grown(g), (th * 2 / 3).max(2), PM_SEL, (220.0 * (1.0 - 0.7 * breath)) as u32);
            }
        }
        let (t2, a) = if lit == PmLit::Offer { (th, 215) } else { (th * 3 / 2, 255) };
        frame_blend(buf, win_w, win_h, r.grown(t2 / 2), t2, colour, a);
    }
}

/// The largest whole font scale, at most `ts`, at which `text` fits `avail` window px.
fn fit_ts(text: &str, avail: u32, ts: u32) -> u32 {
    (avail / crate::hostfont::width(text).max(1)).min(ts).max(1)
}

/// Draw the pad screen: the HUD as it is played, every target lit and badged, the window.
pub fn draw_padmap(buf: &mut [u32], win_w: u32, win_h: u32, sc: &PadMapScene, slots: &[QuickSlot], look: &PadMapLook) {
    // The live frame dimmed — lighter than the layout editor's 45 %: this is the game with
    // its buttons lit, not a blueprint of it.
    for px in buf.iter_mut() {
        let v = *px;
        let r = ((v >> 16) & 0xff) * 55 / 100;
        let g = ((v >> 8) & 0xff) * 55 / 100;
        let b = (v & 0xff) * 55 / 100;
        *px = 0xff00_0000 | (r << 16) | (g << 8) | b;
    }
    // The HUD as it is played, by its own painters.
    draw_controls(buf, win_w, win_h, &sc.layout, &HashSet::new(), false, false);
    draw_quickbar(buf, win_w, win_h, &sc.quick, slots, None, &[]);
    if let Some(r) = sc.page {
        draw_mask_button_spanned(buf, win_w, win_h, r, ui_icons::Icon::QuickPage, false, QUICKPAGE_ICON_SPAN);
    }
    if let Some(r) = sc.map {
        draw_top_tab(buf, win_w, win_h, r, "MAP");
    }
    if let Some(r) = sc.save {
        draw_top_tab(buf, win_w, win_h, r, "SAVE");
    }
    for t in sc.targets.iter().filter(|t| t.shape != PadTargetShape::Chip) {
        padmap_light(buf, win_w, win_h, t, padmap_lit(t.row, look), look.pulse);
    }
    // The badges over everything — the in-play badge pass's rule.
    for (t, b) in sc.targets.iter().zip(look.badges) {
        if t.shape != PadTargetShape::Chip
            && let Some(pos) = b
        {
            draw_pad_badge(buf, win_w, win_h, t.rect, *pos);
        }
    }
    draw_padmap_panel(buf, win_w, win_h, sc, look);
}

/// The window, in this game's own chrome — the settings window's frame, band, studs and
/// black plates — holding the lines, the cells and the three buttons.
fn draw_padmap_panel(buf: &mut [u32], win_w: u32, win_h: u32, sc: &PadMapScene, look: &PadMapLook) {
    let p = &sc.panel;
    let (u, ts) = (p.u, p.ts);
    let pr = p.panel;
    fill(buf, win_w, win_h, pr.x, pr.y, pr.w, pr.h, H2_EDGE);
    fill(buf, win_w, win_h, pr.x + u, pr.y + u, pr.w.saturating_sub(u * 2), pr.h.saturating_sub(u * 2), H2_BEVEL);
    fill(buf, win_w, win_h, pr.x + u * 2, pr.y + u * 2, pr.w.saturating_sub(u * 4), pr.h.saturating_sub(u * 4), H2_BODY);
    // The title band and its edge line.
    let b = p.band;
    fill(buf, win_w, win_h, b.x, b.y, b.w, b.h, H2_BAND);
    fill(buf, win_w, win_h, b.x, b.y + b.h, b.w, u * 2, H2_EDGE);
    let h = crate::hostfont::CELL_H;
    let left = p.cancel.x + p.cancel.w;
    let span = p.save.x.saturating_sub(left);
    let tts = fit_ts(look.title, span * 90 / 100, ts);
    crate::hostfont::draw_centered(buf, win_w, win_h, left, span, b.y + b.h.saturating_sub(h * tts) / 2, tts, look.title, H2_INK);
    // 취소 / 저장 — on the black band, so framed in the body colour like the settings
    // window's 닫기. 저장 is lit in the game's emphasis gold while there is something to keep.
    for (r, label, lit) in [(p.cancel, "취소", false), (p.save, "저장", look.dirty)] {
        fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, H2_BODY);
        let inner = Rect::new(r.x + u, r.y + u, r.w.saturating_sub(u * 2), r.h.saturating_sub(u * 2));
        fill(buf, win_w, win_h, inner.x, inner.y, inner.w, inner.h, H2_BAND);
        let ls = crate::hostfont::fit_scale(label, inner.w * 96 / 100, inner.h * 70 / 100);
        let y = inner.y + inner.h.saturating_sub(h * ls) / 2;
        crate::hostfont::draw_centered(buf, win_w, win_h, inner.x, inner.w, y, ls, label, if lit { H2_GOLD } else { H2_INK });
    }
    // The studs at the body's corners.
    let rr = u * 5 / 2;
    let body = p.body;
    let (bx0, bx1) = (body.x + u * 2, (body.x + body.w).saturating_sub(u * 2));
    let (by0, by1) = (body.y + u * 2, (body.y + body.h).saturating_sub(u * 2));
    for (cx, cy) in [(bx0, by0), (bx1, by0), (bx0, by1), (bx1, by1)] {
        h2_rivet(buf, win_w, win_h, cx, cy, rr);
    }
    // The two lines on one black plate: what happened (white), what to do (grey).
    let s = p.status;
    fill(buf, win_w, win_h, s.x, s.y, s.w, s.h, H2_BAND);
    let line = (s.h.saturating_sub(u * 2)) / 2;
    let avail = s.w.saturating_sub(u * 6);
    for (k, text) in look.lines.iter().enumerate() {
        let lts = fit_ts(text, avail, ts);
        let y = s.y + u + k as u32 * line + line.saturating_sub(h * lts) / 2;
        crate::hostfont::draw(buf, win_w, win_h, s.x + u * 3, y, lts, text, if k == 0 { H2_INK } else { H2_GOLD });
    }
    // The cells: a black plate, the caption, the badge at the right end — lit like the
    // controls are.
    let mut chips = p.chips.iter();
    for (t, bdg) in sc.targets.iter().zip(look.badges) {
        if t.shape != PadTargetShape::Chip {
            continue;
        }
        let Some((r, label)) = chips.next() else { break };
        let lit = padmap_lit(t.row, look);
        fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, H2_BAND);
        if lit == PmLit::Sel {
            blend_rect(buf, win_w, win_h, r.x, r.y, r.w, r.h, PM_SEL, 60 + (70.0 * look.pulse.unwrap_or(0.0)) as u32);
        }
        let (colour, t2, a) = match lit {
            PmLit::Offer => (PM_OFFER, u.max(1), 200),
            PmLit::Sel => (PM_SEL, u.max(1) * 2, 255),
            PmLit::Probe => (PM_PROBE, u.max(1) * 2, 255),
        };
        frame_blend(buf, win_w, win_h, *r, t2, colour, a);
        let br = f64::from(r.h) * 0.34;
        let badge = crate::padhud::Badge {
            cx: f64::from(r.x + r.w) - f64::from(r.h) / 2.0,
            cy: f64::from(r.y) + f64::from(r.h) / 2.0,
            r: br,
        };
        blend_disc(buf, win_w, win_h, badge.cx, badge.cy, br, BADGE_FILL, 225);
        blend_ring(buf, win_w, win_h, badge.cx, badge.cy, br, BADGE_RIM, 190);
        if let Some(pos) = bdg {
            draw_pad_glyph(buf, win_w, win_h, badge, *pos);
        }
        let cap = r.w.saturating_sub(r.h + u * 4);
        let cts = fit_ts(label, cap, ts);
        crate::hostfont::draw(buf, win_w, win_h, r.x + u * 2, r.y + r.h.saturating_sub(h * cts) / 2, cts, label, H2_INK);
    }
    // 기본값: a plate like 취소 / 저장's, greyed with nothing to undo — in the cells' letter
    // size. ⛔ Not through `draw_h2_row`: its 70 %-of-height rule puts a chip-high plate a
    // whole scale step under the cells beside it (S9: half their height).
    let r = p.reset;
    fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, H2_EDGE);
    let inner = Rect::new(r.x + u, r.y + u, r.w.saturating_sub(u * 2), r.h.saturating_sub(u * 2));
    fill(buf, win_w, win_h, inner.x, inner.y, inner.w, inner.h, H2_BAND);
    let rts = fit_ts("기본값", inner.w * 96 / 100, ts);
    let ink = if look.can_reset { H2_INK } else { H2_INK_DIM };
    crate::hostfont::draw_centered(buf, win_w, win_h, inner.x, inner.w, inner.y + inner.h.saturating_sub(h * rts) / 2, rts, "기본값", ink);
}

/// One settings row, in the game's window style: a black plate on the body, the
/// name on the left and the current state on the right.
///
/// ★ ONE LINE, not two centred ones. The font scales in whole pixels, so a row that
/// can hold two 16 px lines can hold one 32 px line and never two of those — and
/// splitting the row across instead of down is also the shape the game's own option
/// rows have.
fn draw_h2_row(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, title: &str, sub: &str, st: RowState) {
    fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, H2_BAND);
    // The game says "this one" with INK, not with a different plate: its cursor row
    // is white where the rest are grey. Same grammar here, with its slot-screen gold
    // for a value the player has moved off the default.
    let ink = if st == RowState::Disabled { H2_INK_DIM } else { H2_INK };
    let sub_ink = match st {
        RowState::Active => H2_GOLD,
        RowState::Normal | RowState::Disabled => H2_INK_DIM,
    };
    let h = crate::hostfont::CELL_H;
    let pad = (r.w / 24).clamp(4, 40);
    let inner = r.w.saturating_sub(pad * 2);

    if sub.is_empty() {
        // A centred label is not budgeted against `inner`: the padding exists to keep
        // a LEFT-anchored name off the plate edge, and spending it here costs a whole
        // scale step on the only row that has one — 닫기 in its square plate dropped
        // from 3 to 2, i.e. to half the height of everything around it.
        let s = crate::hostfont::fit_scale(title, r.w * 96 / 100, r.h * 70 / 100);
        crate::hostfont::draw_centered(buf, win_w, win_h, r.x, r.w, r.y + r.h.saturating_sub(h * s) / 2, s, title, ink);
        return;
    }
    // Both halves take ONE scale — a bigger name over a smaller state reads as two
    // different controls. The budget is the two of them plus a cell of daylight, so
    // the longest pair in the panel is what sizes them all.
    let need = (crate::hostfont::width(title) + crate::hostfont::width(sub) + h).max(1);
    let s = (inner / need).min(r.h * 70 / 100 / h).max(1);
    let y = r.y + r.h.saturating_sub(h * s) / 2;
    crate::hostfont::draw(buf, win_w, win_h, r.x + pad, y, s, title, ink);
    let sw = crate::hostfont::text_w(sub, s);
    crate::hostfont::draw(buf, win_w, win_h, r.x + r.w.saturating_sub(pad + sw), y, s, sub, sub_ink);
}

/// What a settings row is saying. ⛔ Said in INK, not in the plate's colour (#11,
/// 2026-09-20): the game's own list marks its current row by drawing that row's
/// letters white where the rest are grey, and it has no second plate colour to
/// borrow. See [`draw_h2_row`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowState {
    /// At its default.
    Normal,
    /// The player has changed it — the panel's standing job is to answer
    /// "have I changed anything?" at a glance.
    Active,
    /// Not available here; the state line says why.
    Disabled,
}


/// Largest glyph scale that keeps a `chars`-long ASCII label inside `r`.
fn fit_scale(r: Rect, chars: u32) -> u32 {
    let by_h = (r.h / 12).clamp(2, 8);
    let by_w = (r.w * 86 / 100 / (chars.max(1) * 6)).max(1);
    by_h.min(by_w).max(1)
}

/// Scale and the two baselines for a STACKED two-line button caption, sized so both
/// lines fit inside `r` with a gap.
///
/// ★ Device-caught 2026-07-31: the rows used to place the lines at fixed `h/5` and
/// `3h/5` with a scale derived only from `h/12`. That works at the old three-row
/// height (172 px) and collides at the four-row height (111 px) — glyphs are `7*s`
/// tall, so at `s = 8` line 1 ran 22..78 and line 2 started at 66 and spilled 11 px
/// out of the button. FULL/ON, ZOOM/220 and LAYOUT/EDIT were all printed on top of
/// themselves. Derive the positions from the glyph height instead of guessing.
fn two_line(r: Rect, chars: u32) -> (u32, u32, u32) {
    // 16 = two 7-tall lines + a ~2 gap, against 88 % of the button height.
    let by_h = ((r.h * 88 / 100) / 16).max(1);
    let by_w = (r.w * 86 / 100 / (chars.max(1) * 6)).max(1);
    let s = by_h.min(by_w).clamp(1, 8);
    let gap = (s * 3 / 7).max(1);
    let total = 14 * s + gap;
    let y1 = r.y + r.h.saturating_sub(total) / 2;
    (s, y1, y1 + 7 * s + gap)
}

/// Alpha-blend `color` over the pixel at `i`. `a` = 0..=255.
///
/// ★ 2026-09-18 (#4): this is Porter-Duff OVER in PREMULTIPLIED space, and the
/// alpha channel is now carried instead of being forced to `0xff`. On the window
/// buffer that is a no-op — every window pixel starts opaque, so the new alpha
/// evaluates to `a + 255*(255-a)/255 = 255` and the colour arithmetic is the old
/// expression verbatim. What it BUYS is the other destination: [`ControlsLayer`]
/// accumulates the same draw calls into a transparent buffer, and a transparent
/// destination only composites correctly if the blend tracks coverage.
///
/// The layer therefore holds PREMULTIPLIED colour (`rgb <= a`), which is exactly
/// what `s * a / 255` produces for the first write onto a cleared pixel.
fn blend_px(buf: &mut [u32], i: usize, color: u32, a: u32) {
    let dst = buf[i];
    let inv = 255 - a;
    let mix = |sh: u32| {
        let s = (color >> sh) & 0xff;
        let d = (dst >> sh) & 0xff;
        ((s * a + d * inv) / 255) & 0xff
    };
    let da = (dst >> 24) & 0xff;
    let na = ((a * 255 + da * inv) / 255) & 0xff;
    buf[i] = (na << 24) | (mix(16) << 16) | (mix(8) << 8) | mix(0);
}

/// Alpha-blended disc (softbuffer has no alpha channel - see [`blend_px`]).
/// Hard-edged; [`blend_disc_soft`] is the anti-aliased one used by every key.
#[allow(dead_code)]
fn blend_disc(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, r: f64, color: u32, a: u32) {
    if r <= 0.0 {
        return;
    }
    let x0 = (cx - r).max(0.0) as u32;
    let x1 = ((cx + r).max(0.0) as u32 + 1).min(win_w);
    let y0 = (cy - r).max(0.0) as u32;
    let y1 = ((cy + r).max(0.0) as u32 + 1).min(win_h);
    let r2 = r * r;
    for y in y0..y1 {
        let dy = f64::from(y) + 0.5 - cy;
        let row = (y * win_w) as usize;
        for x in x0..x1 {
            let dx = f64::from(x) + 0.5 - cx;
            if dx * dx + dy * dy <= r2 {
                blend_px(buf, row + x as usize, color, a);
            }
        }
    }
}

/// Alpha-blended disc with an ANTI-ALIASED, optionally FEATHERED edge.
///
/// `feather` = width in px of the fade band ending at `r`. `feather <= 1` gives a
/// crisp (but still anti-aliased) edge; a larger value makes the disc dissolve into
/// whatever is behind it. Coverage is smoothstepped, so there is no visible
/// "shading starts here" line — the same reason the title-edge ramp uses a curve.
///
/// This is what makes a round key read as round on a busy game background: a
/// hard-clipped circle over pixel art shows staircase edges at this scale.
fn blend_disc_soft(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, r: f64, color: u32, a: u32, feather: f64) {
    if r <= 0.0 || a == 0 {
        return;
    }
    let f = feather.max(0.9);
    let x0 = (cx - r - 1.0).max(0.0) as u32;
    let x1 = ((cx + r + 1.0).max(0.0) as u32 + 1).min(win_w);
    let y0 = (cy - r - 1.0).max(0.0) as u32;
    let y1 = ((cy + r + 1.0).max(0.0) as u32 + 1).min(win_h);
    for y in y0..y1 {
        let dy = f64::from(y) + 0.5 - cy;
        let row = (y * win_w) as usize;
        for x in x0..x1 {
            let dx = f64::from(x) + 0.5 - cx;
            let d = (dx * dx + dy * dy).sqrt();
            let cov = ((r - d) / f).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let cov = cov * cov * (3.0 - 2.0 * cov); // smoothstep
            let aa = (f64::from(a) * cov) as u32;
            if aa > 0 {
                blend_px(buf, row + x as usize, color, aa);
            }
        }
    }
}

/// Anti-aliased ring of thickness `thick` centred on radius `r`, alpha `a`.
fn blend_ring_soft(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, r: f64, thick: f64, color: u32, a: u32) {
    if r <= 1.0 || a == 0 {
        return;
    }
    let half = (thick / 2.0).max(0.6);
    let pad = half + 1.5;
    let x0 = (cx - r - pad).max(0.0) as u32;
    let x1 = ((cx + r + pad).max(0.0) as u32 + 1).min(win_w);
    let y0 = (cy - r - pad).max(0.0) as u32;
    let y1 = ((cy + r + pad).max(0.0) as u32 + 1).min(win_h);
    for y in y0..y1 {
        let dy = f64::from(y) + 0.5 - cy;
        let row = (y * win_w) as usize;
        for x in x0..x1 {
            let dx = f64::from(x) + 0.5 - cx;
            let d = (dx * dx + dy * dy).sqrt();
            // Coverage falls off over ~1px either side of the band.
            let cov = (1.0 - ((d - r).abs() - half).max(0.0)).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let aa = (f64::from(a) * cov) as u32;
            if aa > 0 {
                blend_px(buf, row + x as usize, color, aa);
            }
        }
    }
}

/// The ONE round-key face used by every on-screen key (player request 2026-07-30:
/// "the 1/3/7/9/0/#/CANCEL keys and the menu 1/3/7/9 keys are square — make them
/// ALL round"). Inscribed circle of `r`, translucent fill + two anti-aliased rims
/// (outer bright, inner dim) so the key reads on any background.
/// The D-pad arrow's HEXAGON face (91차): the six corners from [`hexagon`], rounded
/// and anti-aliased by [`draw_convex_face`].
///
/// ★ ROUNDED CORNERS, as the triangles had ("뾰족한 부분을 살짝만 둥글게 처리해줘").
/// A hexagon's corners are 120°, so a tenth of the short side is plenty — the
/// triangles needed a fifth to soften their 50° tips.
#[allow(clippy::too_many_arguments)]
fn draw_hex_face(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, dir: Dir, fill_color: u32, rim_color: u32, fill_a: u32, rim_a: u32) {
    if r.w < 4 || r.h < 4 {
        return;
    }
    let round_r = (f64::from(r.w.min(r.h)) * 0.10).clamp(1.5, 16.0);
    // Rim thickness scaled off the button so a big D-pad does not get a hairline
    // outline and a small one does not get a fat one.
    let t = (f64::from(r.w.min(r.h)) / 13.0).clamp(1.4, 4.0);
    // The mask is the same for UP and DOWN (and for LEFT and RIGHT): the hexagon is
    // symmetric on both axes, so the key carries the AXIS, not the direction.
    let axis = matches!(dir, Dir::Up | Dir::Down) as u8;
    let verts = hexagon(Rect::new(0, 0, r.w, r.h), dir);
    draw_convex_face(buf, win_w, win_h, r, &verts, (FACE_HEX, axis), round_r, t, fill_color, rim_color, fill_a, rim_a);
}

/// The arrow glyph inside a hexagon arm: a squat rounded triangle pointing `dir`,
/// about two fifths of the arm's length. The hexagon's own six corners are identical, so
/// without this a D-pad reads as four blank tiles.
fn draw_hex_glyph(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, dir: Dir, color: u32, a: u32) {
    // Half-height of the glyph: relative to the arm's LENGTH (the axis it points
    // along), so the four glyphs are the same size on a square seat.
    let along = match dir {
        Dir::Up | Dir::Down => r.h,
        Dir::Left | Dir::Right => r.w,
    };
    // ★ 91차-B: 0.19 of the length (was 0.16) — the hexagon got fat, the mark keeps up.
    let g = f64::from(along) * 0.19;
    if g < 3.0 {
        return;
    }
    // Squat: base 2.3g wide, 1.65g tall — the 2026-08-09 "양쪽끝이 조금더 넓었으면"
    // proportion, so the glyph is the old arrow in miniature. Built in a box of its
    // own, centred on the arm's centre.
    let (hb, tip, base) = (g * 1.15, g, g * 0.65);
    let side = (hb * 2.0 + 4.0).ceil() as u32;
    let (cx, cy) = (f64::from(side) / 2.0, f64::from(side) / 2.0);
    let v = match dir {
        Dir::Up => [(cx, cy - tip), (cx + hb, cy + base), (cx - hb, cy + base)],
        Dir::Down => [(cx, cy + tip), (cx - hb, cy - base), (cx + hb, cy - base)],
        Dir::Left => [(cx - tip, cy), (cx + base, cy - hb), (cx + base, cy + hb)],
        Dir::Right => [(cx + tip, cy), (cx - base, cy + hb), (cx - base, cy - hb)],
    };
    let bbox = Rect::new(
        (f64::from(r.x) + f64::from(r.w) / 2.0 - cx).max(0.0) as u32,
        (f64::from(r.y) + f64::from(r.h) / 2.0 - cy).max(0.0) as u32,
        side,
        side,
    );
    // No rim: the glyph is a flat mark, the rim belongs to the button around it.
    draw_convex_face(buf, win_w, win_h, bbox, &v, (FACE_GLYPH, dir as u8), (g * 0.22).max(1.0), 0.0, color, color, a, 0);
}

/// Mask-cache key kinds — see [`convex_mask`].
const FACE_HEX: u8 = 1;
const FACE_GLYPH: u8 = 2;

/// `(kind, variant, w, h, round_r * 10, rim_t * 10)`: everything a mask's pixels
/// depend on. Colours and alphas are applied at blit time, so the live HUD and the
/// layout editor share one mask per shape.
type FaceKey = (u8, u8, u32, u32, u32, u32);

/// Rounded-convex-polygon masks, keyed by shape (see [`convex_mask`]). Each pixel is
/// `cover << 8 | edge * cover`, both 0..=255. A handful of entries: two hexagon axes
/// and four glyph directions per seat size.
static FACE_MASKS: std::sync::Mutex<Option<std::collections::HashMap<FaceKey, std::sync::Arc<Vec<u16>>>>> =
    std::sync::Mutex::new(None);
const FACE_MASKS_MAX: usize = 48;

/// Anti-aliased, ROUNDED convex polygon with a rim band just inside its outline,
/// drawn into `bbox` from vertices given RELATIVE to the box's origin.
///
/// The shape is the polygon ERODED by `round_r` and dilated back: its signed distance
/// is `sd(eroded) - round_r`, which rounds every corner with radius `round_r` exactly
/// (offsetting the edge lines and calling that rounding does not work — opening a
/// convex set with a disc returns the set unchanged; the corners have to be built).
/// The erosion is exact for any convex polygon: each edge line is pushed inward by
/// `round_r` along its normal and adjacent lines are intersected, which is what
/// [`inset_convex`] does. `round_r` is capped just under the inradius so the eroded
/// polygon cannot turn inside out.
///
/// ★ Generalised from the 2026-08-09 triangle painter (which eroded by a similarity
/// about the incentre — exact for a triangle, wrong for anything else) when the arrows
/// became hexagons (91차). The same routine draws the arrow GLYPH inside them.
///
/// ★ The per-pixel distance field is MEMOISED ([`convex_mask`]): four ~180 px
/// hexagons cost ~1.7x what the four triangles did if rasterised every repaint, and
/// the S9 is the low-end reference. A mask depends only on the shape, so the per-frame
/// work is one blend per pixel — the `scaled_icon` pattern from 72차.
#[allow(clippy::too_many_arguments)]
fn draw_convex_face(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    bbox: Rect,
    verts: &[(f64, f64)],
    kind: (u8, u8),
    round_r: f64,
    rim_t: f64,
    fill_color: u32,
    rim_color: u32,
    fill_a: u32,
    rim_a: u32,
) {
    if verts.len() < 3 || bbox.w == 0 || bbox.h == 0 || win_w == 0 || win_h == 0 {
        return;
    }
    let key: FaceKey = (kind.0, kind.1, bbox.w, bbox.h, (round_r * 10.0) as u32, (rim_t * 10.0) as u32);
    let mask = convex_mask(key, bbox.w, bbox.h, verts, round_r, rim_t);
    let x_end = (bbox.x + bbox.w).min(win_w);
    let y_end = (bbox.y + bbox.h).min(win_h);
    for py in bbox.y..y_end {
        let row = (py * win_w) as usize;
        let mrow = ((py - bbox.y) * bbox.w) as usize;
        for px in bbox.x..x_end {
            let m = mask[mrow + (px - bbox.x) as usize];
            let cover = u32::from(m >> 8);
            if cover == 0 {
                continue;
            }
            blend_px(buf, row + px as usize, fill_color, fill_a * cover / 255);
            let edge_cover = u32::from(m & 0xff);
            if rim_a > 0 && edge_cover > 0 {
                blend_px(buf, row + px as usize, rim_color, rim_a * edge_cover / 255);
            }
        }
    }
}

/// The distance-field mask of one rounded convex polygon in a `w x h` box — computed
/// once per `key` and kept (see [`draw_convex_face`]).
fn convex_mask(key: FaceKey, w: u32, h: u32, verts: &[(f64, f64)], round_r: f64, rim_t: f64) -> std::sync::Arc<Vec<u16>> {
    if let Ok(g) = FACE_MASKS.lock()
        && let Some(hit) = g.as_ref().and_then(|m| m.get(&key).cloned())
    {
        return hit;
    }
    let n = verts.len();
    let mut out = vec![0u16; (w * h) as usize];
    // Winding: +1 for counter-clockwise in screen coordinates (y down), -1 otherwise.
    let area2: f64 = (0..n)
        .map(|i| {
            let (ax, ay) = verts[i];
            let (bx, by) = verts[(i + 1) % n];
            ax * by - bx * ay
        })
        .sum();
    if area2.abs() < 1e-6 {
        return std::sync::Arc::new(out);
    }
    let wind = area2.signum();
    let (cx, cy) = verts.iter().fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
    let (cx, cy) = (cx / n as f64, cy / n as f64);
    // Inradius bound: the nearest edge to the centroid.
    let r_in = (0..n)
        .map(|i| {
            let (ax, ay) = verts[i];
            let (bx, by) = verts[(i + 1) % n];
            let (ex, ey) = (bx - ax, by - ay);
            let len = (ex * ex + ey * ey).sqrt().max(1e-9);
            ((ex * (cy - ay) - ey * (cx - ax)) / len).abs()
        })
        .fold(f64::INFINITY, f64::min);
    let round_r = round_r.min(r_in * 0.9).max(0.0);
    let v = inset_convex(verts, round_r, wind);

    let aa = 1.2f64;
    for py in 0..h {
        let fy = f64::from(py) + 0.5;
        for px in 0..w {
            let fx = f64::from(px) + 0.5;
            // Exact distance to the ERODED polygon (nearest point on each SEGMENT, not
            // on its infinite line — that difference is what makes the corners round).
            let mut best = f64::INFINITY;
            let mut inside = true;
            for i in 0..n {
                let (ax, ay) = v[i];
                let (ex, ey) = (v[(i + 1) % n].0 - ax, v[(i + 1) % n].1 - ay);
                let (wx, wy) = (fx - ax, fy - ay);
                let hh = ((wx * ex + wy * ey) / (ex * ex + ey * ey).max(1e-9)).clamp(0.0, 1.0);
                let (qx, qy) = (wx - ex * hh, wy - ey * hh);
                best = best.min(qx * qx + qy * qy);
                if (ex * wy - ey * wx) * wind < 0.0 {
                    inside = false;
                }
            }
            // `d` > 0 inside the ROUNDED shape, in pixels.
            let d = round_r - best.sqrt() * if inside { -1.0 } else { 1.0 };
            if d < -aa {
                continue;
            }
            let cover = ((d + aa) / (2.0 * aa)).clamp(0.0, 1.0);
            // Rim: a band just inside the outline.
            let edge = if rim_t > 0.0 && d < rim_t { 1.0 - (d / rim_t).clamp(0.0, 1.0) } else { 0.0 };
            let c = (cover * 255.0).round() as u16;
            let e = (edge * cover * 255.0).round() as u16;
            out[(py * w + px) as usize] = (c << 8) | e;
        }
    }
    let out = std::sync::Arc::new(out);
    if let Ok(mut g) = FACE_MASKS.lock() {
        let m = g.get_or_insert_with(std::collections::HashMap::new);
        if m.len() >= FACE_MASKS_MAX {
            m.clear();
        }
        m.insert(key, out.clone());
    }
    out
}

/// `verts` with every edge line pushed INWARD by `e`, adjacent lines re-intersected.
/// `wind` is the polygon's winding sign (see [`convex_mask`]). An edge pair that will
/// not intersect (parallel — impossible for adjacent edges of a convex polygon, kept
/// for safety) keeps its original vertex.
fn inset_convex(verts: &[(f64, f64)], e: f64, wind: f64) -> Vec<(f64, f64)> {
    let n = verts.len();
    // Each edge as a line `nx * x + ny * y = c`, with the unit normal pointing INWARD.
    let lines: Vec<(f64, f64, f64)> = (0..n)
        .map(|i| {
            let (ax, ay) = verts[i];
            let (bx, by) = verts[(i + 1) % n];
            let (ex, ey) = (bx - ax, by - ay);
            let len = (ex * ex + ey * ey).sqrt().max(1e-9);
            // The left normal of the edge direction is inward for a positive winding
            // (`wind` says which side that is in y-down screen space).
            let (nx, ny) = (-ey / len * wind, ex / len * wind);
            (nx, ny, nx * ax + ny * ay + e)
        })
        .collect();
    (0..n)
        .map(|i| {
            let (n1x, n1y, c1) = lines[(i + n - 1) % n];
            let (n2x, n2y, c2) = lines[i];
            let det = n1x * n2y - n2x * n1y;
            if det.abs() < 1e-9 {
                return verts[i];
            }
            ((c1 * n2y - c2 * n1y) / det, (n1x * c2 - n2x * c1) / det)
        })
        .collect()
}
fn draw_round_face(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    r: Rect,
    fill_color: u32,
    rim_color: u32,
    fill_a: u32,
    rim_a: u32,
) -> (f64, f64, f64) {
    let rad = f64::from(r.w.min(r.h)) / 2.0;
    let cx = f64::from(r.x) + f64::from(r.w) / 2.0;
    let cy = f64::from(r.y) + f64::from(r.h) / 2.0;
    if rad < 1.0 {
        return (cx, cy, rad);
    }
    // Fade the fill out over the last ~9% of the radius so the disc melts into the
    // game instead of ending on a hard rasterised arc.
    blend_disc_soft(buf, win_w, win_h, cx, cy, rad, fill_color, fill_a, (rad * 0.09).max(1.2));
    let t = (rad / 13.0).clamp(1.4, 4.0);
    blend_ring_soft(buf, win_w, win_h, cx, cy, rad - t / 2.0, t, rim_color, rim_a);
    (cx, cy, rad)
}

/// Alpha-blended 2px ring (the opaque [`ring`] is too heavy over the game).
#[allow(dead_code)]
fn blend_ring(buf: &mut [u32], win_w: u32, win_h: u32, cx: f64, cy: f64, r: f64, color: u32, a: u32) {
    if r <= 1.0 {
        return;
    }
    let x0 = (cx - r - 1.0).max(0.0) as u32;
    let x1 = ((cx + r + 1.0).max(0.0) as u32 + 1).min(win_w);
    let y0 = (cy - r - 1.0).max(0.0) as u32;
    let y1 = ((cy + r + 1.0).max(0.0) as u32 + 1).min(win_h);
    for y in y0..y1 {
        let dy = f64::from(y) + 0.5 - cy;
        let row = (y * win_w) as usize;
        for x in x0..x1 {
            let dx = f64::from(x) + 0.5 - cx;
            let d = (dx * dx + dy * dy).sqrt();
            if (d - r).abs() <= 1.4 {
                blend_px(buf, row + x as usize, color, a);
            }
        }
    }
}

fn blend_rect(buf: &mut [u32], win_w: u32, win_h: u32, x: u32, y: u32, w: u32, h: u32, color: u32, a: u32) {
    let x1 = (x + w).min(win_w);
    let y1 = (y + h).min(win_h);
    for yy in y..y1 {
        let row = (yy * win_w) as usize;
        for xx in x..x1 {
            blend_px(buf, row + xx as usize, color, a);
        }
    }
}

/// Immersive-mode RECTANGULAR button face. Superseded by [`draw_round_face`]
/// (2026-07-30: every key is round); kept for the settings modal's rows.
#[allow(dead_code)]
fn draw_button_face_translucent(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, color: u32) {
    const FILL_A: u32 = 92; // ~36% — user asked for more see-through
    const RIM_A: u32 = 165;
    blend_rect(buf, win_w, win_h, r.x, r.y, r.w, r.h, color, FILL_A);
    let t = 2;
    blend_rect(buf, win_w, win_h, r.x, r.y, r.w, t, BORDER, RIM_A);
    blend_rect(buf, win_w, win_h, r.x, r.y + r.h.saturating_sub(t), r.w, t, BORDER, RIM_A);
    blend_rect(buf, win_w, win_h, r.x, r.y, t, r.h, BORDER, RIM_A);
    blend_rect(buf, win_w, win_h, r.x + r.w.saturating_sub(t), r.y, t, r.h, BORDER, RIM_A);
}

#[allow(dead_code)]
fn draw_button_face(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, color: u32) {
    fill(buf, win_w, win_h, r.x, r.y, r.w, r.h, color);
    stroke(buf, win_w, win_h, (r.x, r.y, r.w, r.h), BORDER);
}

/// The largest axis-aligned square that fits INSIDE the circle inscribed in `r`
/// (side = diameter / sqrt(2), 0.707..). Text sized against this can never be
/// clipped by the disc, whatever the string length.
fn label_box(r: Rect) -> Rect {
    let d = r.w.min(r.h);
    let side = (u64::from(d) * 707 / 1000) as u32;
    Rect::new(r.x + (r.w.saturating_sub(side)) / 2, r.y + (r.h.saturating_sub(side)) / 2, side.max(1), side.max(1))
}

fn draw_centered_text(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, t: &str, color: u32) {
    let scale = (r.h / 14).min(r.w / (t.len() as u32 * 6 + 1)).max(2);
    draw_centered_text_in(buf, win_w, win_h, r.x, r.y + (r.h.saturating_sub(7 * scale)) / 2, r.w, t, color, scale);
}

fn draw_centered_text_in(buf: &mut [u32], win_w: u32, win_h: u32, x: u32, y: u32, w: u32, t: &str, color: u32, scale: u32) {
    let tw = t.len() as u32 * 6 * scale;
    let tx = x + (w.saturating_sub(tw)) / 2;
    draw_text(buf, win_w, win_h, tx, y, scale, t, color);
}

fn draw_gear(buf: &mut [u32], win_w: u32, win_h: u32, g: Rect) {
    if g.w == 0 || g.h == 0 {
        return;
    }
    // Round like every other key (2026-07-30). The cog glyph below is already
    // circular, so it sits inside the disc without clipping.
    draw_round_face(buf, win_w, win_h, g, BTN, BORDER, 92, 165);
    let cx = g.x + g.w / 2;
    let cy = g.y + g.h / 2;
    let r = (g.w.min(g.h) / 2).saturating_sub(g.w / 7).max(6);
    let dirs: [(i64, i64); 8] = [(0, -10), (7, -7), (10, 0), (7, 7), (0, 10), (-7, 7), (-10, 0), (-7, -7)];
    let tooth = (r / 3).max(3);
    for (dx, dy) in dirs {
        let tx = cx as i64 + dx * r as i64 / 10 - tooth as i64 / 2;
        let ty = cy as i64 + dy * r as i64 / 10 - tooth as i64 / 2;
        fill(buf, win_w, win_h, tx.max(0) as u32, ty.max(0) as u32, tooth, tooth, LABEL);
    }
    fill_circle(buf, win_w, win_h, cx, cy, r, LABEL);
    fill_circle(buf, win_w, win_h, cx, cy, r * 11 / 20, BTN);
    fill_circle(buf, win_w, win_h, cx, cy, r / 4, LABEL);
}

// --------------------------------------------------------------------------
// Low-level raster helpers
// --------------------------------------------------------------------------

fn fill(buf: &mut [u32], win_w: u32, win_h: u32, x: u32, y: u32, w: u32, h: u32, color: u32) {
    let x1 = (x + w).min(win_w);
    let y1 = (y + h).min(win_h);
    for yy in y..y1 {
        let row = (yy * win_w) as usize;
        for xx in x..x1 {
            buf[row + xx as usize] = color;
        }
    }
}

fn stroke(buf: &mut [u32], win_w: u32, win_h: u32, r: (u32, u32, u32, u32), color: u32) {
    let (x, y, w, h) = r;
    let t = 2;
    fill(buf, win_w, win_h, x, y, w, t, color);
    fill(buf, win_w, win_h, x, y + h.saturating_sub(t), w, t, color);
    fill(buf, win_w, win_h, x, y, t, h, color);
    fill(buf, win_w, win_h, x + w.saturating_sub(t), y, t, h, color);
}

fn fill_circle(buf: &mut [u32], win_w: u32, win_h: u32, cx: u32, cy: u32, r: u32, color: u32) {
    let r2 = (r * r) as i64;
    let ri = r as i64;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if dx * dx + dy * dy <= r2 {
                let px = cx as i64 + dx;
                let py = cy as i64 + dy;
                if px >= 0 && py >= 0 && (px as u32) < win_w && (py as u32) < win_h {
                    buf[(py as u32 * win_w + px as u32) as usize] = color;
                }
            }
        }
    }
}

fn draw_text(buf: &mut [u32], win_w: u32, win_h: u32, mut x: u32, y: u32, scale: u32, text: &str, color: u32) {
    for ch in text.chars() {
        draw_glyph(buf, win_w, win_h, x, y, scale, ch, color);
        x += 6 * scale;
    }
}

fn draw_glyph(buf: &mut [u32], win_w: u32, win_h: u32, x: u32, y: u32, scale: u32, ch: char, color: u32) {
    for (r, bits) in glyph(ch).iter().enumerate() {
        for c in 0..5u32 {
            if bits & (1 << (4 - c)) != 0 {
                fill(buf, win_w, win_h, x + c * scale, y + r as u32 * scale, scale, scale, color);
            }
        }
    }
}

fn draw_arrow(buf: &mut [u32], win_w: u32, win_h: u32, cx: u32, cy: u32, s: u32, dir: Dir, color: u32) {
    let (cx, cy, s) = (cx as i64, cy as i64, s as i64);
    for dy in -s..=s {
        for dx in -s..=s {
            let inside = match dir {
                Dir::Up => dx.abs() <= (dy + s) / 2,
                Dir::Down => dx.abs() <= (s - dy) / 2,
                Dir::Left => dy.abs() <= (dx + s) / 2,
                Dir::Right => dy.abs() <= (s - dx) / 2,
            };
            if inside {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && py >= 0 && (px as u32) < win_w && (py as u32) < win_h {
                    buf[(py as u32 * win_w + px as u32) as usize] = color;
                }
            }
        }
    }
}

/// Minimal 5x7 bitmap font (low 5 bits per row, MSB = leftmost column).
fn glyph(ch: char) -> [u8; 7] {
    // Uppercase-only table; fold so lowercase labels render too. Anything outside
    // it (Hangul, punctuation not listed) is a BLANK cell — keep overlay labels
    // inside this set. Device-confirmed 2026-07-28: a missing glyph is silent.
    match ch.to_ascii_uppercase() {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        '*' => [0b00000, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0b00000],
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b01010, 0b00001],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        '%' => [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011],
        _ => [0; 7],
    }
}

// --------------------------------------------------------------------------
// 게임패드 HUD — position badges (59차)
// --------------------------------------------------------------------------
//
// One visual language for every pad control: DRAW THE SET, LIGHT THE BOUND MEMBER.
// The four face buttons are a diamond of four dots with one lit; the shoulders are a
// 2x2 of bars (left/right x upper/lower) with one lit; the two middle buttons are two
// pills; the sticks are two rings. No letters anywhere — which is the point (see
// `padhud.rs`: the letter printed on the bottom face button is `A` on an Xbox pad,
// `B` on a Switch pad and nothing at all on a PlayStation pad), and it also dodges
// the problem that a menu key's badge is only ~24 px across.

/// Badge fill and rim. Near-opaque so the glyph reads over bright skill art, and the
/// same slate/steel family as the rest of the HUD.
const BADGE_FILL: u32 = 0xff08_0b14;
const BADGE_RIM: u32 = 0xff9f_b6d4;
/// The lit member of a set (warm, matching the action disc's gold ring) and the
/// unlit ones (the HUD's dim label grey).
const BADGE_LIT: u32 = 0xffff_e0a0;
const BADGE_UNLIT: u32 = 0xff56_607a;

/// Paint the position badge for `pos` on the control at `r`.
pub fn draw_pad_badge(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, pos: crate::padhud::PadPos) {
    let Some(b) = crate::padhud::badge_at(r.x, r.y, r.w, r.h, win_w, win_h) else {
        return;
    };
    blend_disc(buf, win_w, win_h, b.cx, b.cy, b.r, BADGE_FILL, 225);
    blend_ring(buf, win_w, win_h, b.cx, b.cy, b.r, BADGE_RIM, 190);
    draw_pad_glyph(buf, win_w, win_h, b, pos);
}

/// Badge a control by what it DOES rather than by a guest key — the host-owned
/// controls (PAGE, SAVE) have no `KeyCode` to look up.
pub fn draw_pad_badge_action(buf: &mut [u32], win_w: u32, win_h: u32, r: Rect, action: crate::gamepad::PadAction) {
    if let Some(pos) = crate::gamepad::pos_for_action(action) {
        draw_pad_badge(buf, win_w, win_h, r, pos);
    }
}

/// Badge every control in the layout and every skill plate, in ONE pass after the
/// whole HUD is drawn.
///
/// A separate pass and not a step inside each control's own draw call: a control whose
/// face is painted in more than one step (an icon laid over a round face, say) would
/// have the badge end up underneath if the badge went in with the face. Badges are the
/// layer that answers "which button is this?" — they go on top of everything.
pub fn draw_pad_badges(buf: &mut [u32], win_w: u32, win_h: u32, layout: &Layout, quick: &[Rect], slots: &[QuickSlot]) {
    for b in &layout.buttons {
        // The walk keys are not badged. In pad mode the arrows are not drawn at all
        // (see `layout_immersive`), and in the opaque portrait layouts — which have a
        // permanent D-pad — four copies of the same hat glyph is noise, not help.
        if b.dpad || b.face == Face::None {
            continue;
        }
        if let Some(pos) = crate::gamepad::pos_for_key(b.key) {
            draw_pad_badge(buf, win_w, win_h, b.rect, pos);
        }
    }
    // The plates. A pad button NAMES a plate since 2026-09-25 (hero3 79차-B,
    // `gamepad::PadAction::Quick`), so the badge is a straight reverse lookup of that — no
    // residency in it at all, which keeps it still while the plate window shuffles which
    // plate sits in which guest slot. (Until then it followed the SEAT, so an empty plate —
    // one no slot held — had no badge: the editor's PAD preview showed Q1 alone.)
    //
    // ★ A plate nothing is bound to gets NO badge, which is the truth: Q6 · Q7 have no
    // default button (`gamepad::default_button_action`); a `Q6` line in `wie_gamepad.txt`
    // gives one, and the badge follows.
    for (i, r) in quick.iter().enumerate() {
        let plate = match slots.get(i) {
            Some(s) => s.slot,
            None => i as u8,
        };
        if let Some(pos) = crate::gamepad::pos_for_action(crate::gamepad::PadAction::Quick(plate)) {
            draw_pad_badge(buf, win_w, win_h, *r, pos);
        }
    }
}

fn draw_pad_glyph(buf: &mut [u32], win_w: u32, win_h: u32, b: crate::padhud::Badge, pos: crate::padhud::PadPos) {
    use crate::padhud::{FacePos, PadPos};
    // A dot/bar for a member of the set: `lit` members are bigger as well as
    // brighter, so the badge still reads on a screen the player is looking past.
    let dot = |buf: &mut [u32], dx: f64, dy: f64, lit: bool| {
        let rr = b.r * if lit { 0.30 } else { 0.16 };
        let col = if lit { BADGE_LIT } else { BADGE_UNLIT };
        fill_disc(buf, win_w, win_h, b.cx + dx, b.cy + dy, rr, col);
    };
    // Horizontal bar (shoulders, middle buttons), centred on `(dx, dy)`.
    let bar = |buf: &mut [u32], dx: f64, dy: f64, half_w: f64, half_h: f64, lit: bool| {
        let col = if lit { BADGE_LIT } else { BADGE_UNLIT };
        let x = (b.cx + dx - half_w).max(0.0) as u32;
        let y = (b.cy + dy - half_h).max(0.0) as u32;
        let w = (half_w * 2.0).max(1.0) as u32;
        let h = (half_h * 2.0).max(1.0) as u32;
        fill(buf, win_w, win_h, x, y, w, h, col);
    };
    match pos {
        // Diamond of four: up, down, left, right — exactly how the buttons sit.
        PadPos::Face(f) => {
            let d = b.r * 0.46;
            dot(buf, 0.0, -d, f == FacePos::Up);
            dot(buf, 0.0, d, f == FacePos::Down);
            dot(buf, -d, 0.0, f == FacePos::Left);
            dot(buf, d, 0.0, f == FacePos::Right);
        }
        // 2x2 of bars: the top row is the bumpers, the bottom row the triggers, left
        // column left hand. "The lower-right one" needs no letter.
        PadPos::Shoulder { right, lower } => {
            let dx = b.r * 0.40;
            let dy = b.r * 0.36;
            let hw = b.r * 0.30;
            for (rr, ll) in [(false, false), (true, false), (false, true), (true, true)] {
                let lit = rr == right && ll == lower;
                let hh = if lit { b.r * 0.15 } else { b.r * 0.09 };
                bar(
                    buf,
                    if rr { dx } else { -dx },
                    if ll { dy } else { -dy },
                    hw,
                    hh.max(1.0),
                    lit,
                );
            }
        }
        // Two pills side by side, as they sit either side of the pad's centre.
        PadPos::Middle { right } => {
            let dx = b.r * 0.42;
            let hw = b.r * 0.30;
            bar(buf, -dx, 0.0, hw, b.r * 0.14, !right);
            bar(buf, dx, 0.0, hw, b.r * 0.14, right);
        }
        // Two rings; the bound stick is filled in — "press this one".
        PadPos::StickPress { right } => {
            let dx = b.r * 0.44;
            let rr = b.r * 0.34;
            for side in [false, true] {
                let lit = side == right;
                let cx = b.cx + if side { dx } else { -dx };
                blend_ring(buf, win_w, win_h, cx, b.cy, rr, if lit { BADGE_LIT } else { BADGE_UNLIT }, 255);
                if lit {
                    fill_disc(buf, win_w, win_h, cx, b.cy, rr * 0.55, BADGE_LIT);
                }
            }
        }
        // The hat as a whole: a cross, all four arms lit.
        PadPos::Dpad => {
            let arm = b.r * 0.62;
            let t = (b.r * 0.16).max(1.0);
            bar(buf, 0.0, 0.0, arm, t, true);
            bar(buf, 0.0, 0.0, t, arm, true);
        }
    }
}

#[cfg(test)]
mod ring_layout_tests {
    use super::*;

    /// 대표 창: 가로 이머시브 폰 여럿(18:9 · 18.5:9 · 19:9 · 19.5:9 · 21:9 · 16:9) + 세로 폰.
    const WINDOWS: [(u32, u32); 8] =
        [(1920, 1080), (2160, 1080), (2220, 1080), (2340, 1080), (2400, 1080), (2960, 1440), (1600, 720), (1080, 2340)];

    fn centre(r: Rect) -> (f64, f64, f64) {
        (f64::from(r.x) + f64::from(r.w) / 2.0, f64::from(r.y) + f64::from(r.h) / 2.0, f64::from(r.w.min(r.h)) / 2.0)
    }

    /// ★ 2026-09-20, user: *"화살표 크기보다 조이스틱이 조금 더 작아야해. 화살 육각형들의
    /// 각 중심점을 원으로 연결한게 조이스틱크기"*. [`arm_centre_radius`] re-derives the
    /// arms' geometry from the same percentages [`dpad_arms`] uses, and two copies of a
    /// formula is exactly how a mirror drifts — so check it against the arms themselves
    /// rather than against a number written down here.
    #[test]
    fn the_stick_disc_passes_through_the_arrow_centres() {
        for side in [120u32, 240, 400, 624, 900] {
            let r = Rect::new(37, 53, side, side);
            let want = arm_centre_radius(&r);
            let (cx, cy) = (f64::from(r.x) + f64::from(r.w) / 2.0, f64::from(r.y) + f64::from(r.h) / 2.0);
            for arm in dpad_arms(r) {
                let (ax, ay) = (
                    f64::from(arm.draw.x) + f64::from(arm.draw.w) / 2.0,
                    f64::from(arm.draw.y) + f64::from(arm.draw.h) / 2.0,
                );
                let d = ((ax - cx).powi(2) + (ay - cy).powi(2)).sqrt();
                assert!((d - want).abs() <= 1.0, "side {side}: arm centre at {d}, disc radius {want}");
            }
            // And it really is SMALLER than the square it is cut from — that is the whole
            // point of the change, and `min/2` is what it used to be.
            assert!(want < f64::from(side) / 2.0, "side {side}: disc {want} is not inside the arrows' square");
            // The thumb still pins inside the base, and walking still starts well before
            // the pin: a disc small enough to invert that order would be a dead stick.
            assert!(want * (1.0 - JOY_THUMB) > 1.0, "side {side}: pin radius collapsed");
        }
    }

    fn discs_clear(a: Rect, b: Rect) -> bool {
        let (ax, ay, ar) = centre(a);
        let (bx, by, br) = centre(b);
        ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt() >= ar + br
    }

    /// hero3 원호(2026-09-10 이식)의 불변식: 좌석 8개(CLR + 플레이트 7)가 (a) 전부 화면 안에
    /// 있고 (b) 서로 겹치지 않으며 (c) 어떤 플레이트도 필드의 다른 원형 컨트롤 — OK 디스크 ·
    /// CLR 디스크 · 페이지 버튼 — 과 **원(circle) 기준으로** 겹치지 않는다. (회복 아이템 `0`
    /// 버튼도 이 목록에 있었다 — 2026-09-25 에 버튼째 제거.)
    ///
    /// ★ 2026-09-17: (c) 가 이 파일에 없어서 09-10 의 8좌석 링이 `#` 버튼(`disc_row_seat(1)`)
    /// 위에 앉은 채 출고됐다 — 플레이트 6 이 96 px, 7 이 18 px (S9·S25·16:9 전부). 그 버튼을
    /// 걷어내고(`placement::CtrlId::Heal` 의 롱프레스로) 이 검사를 넣었다. 둘 다 디스크로
    /// 그려지므로 사각형이 아니라 원으로 잰다 — hero3 80차-H 가 CLR 에서 같은 함정을 적었다.
    #[test]
    fn no_field_control_touches_a_quickbar_plate() {
        // ★ hero3 82차: the same invariants hold for every SHORTER ring the hidden set
        // can produce — the six-seat (5-plate) ring the cap reproduces, the single
        // plate that used to fly to the arc's far end, and a gap in the middle. The
        // mask is a process global, so this loop runs the variants in ONE test and
        // puts SHOW_ALL back at the end.
        const MASKS: [u8; 5] = [0, 0b110_0000, 0b111_1110, 0b000_1110, 0b011_1111];
        // ★ 2026-09-18 (code review): a FAILING variant must put the global back too —
        // an assert unwinds past the restore at the end, and the next test to read the
        // mask would then be judged against this loop's leftovers.
        struct ShowAllAgain;
        impl Drop for ShowAllAgain {
            fn drop(&mut self) {
                crate::set_quick_hidden(crate::quickpage::SHOW_ALL);
            }
        }
        let _restore = ShowAllAgain;
        for mask in MASKS {
            crate::set_quick_hidden(mask);
            let n = ring_seats_live();
            assert_eq!(n, 1 + crate::quickpage::visible_count(mask));
            for (w, h) in WINDOWS {
                let pad = pad_of(w, h);
                let seats: Vec<Rect> = (0..n).map(|i| ring_seat(w, h, pad, i)).collect();
                let others = [
                    ("OK", action_disc(w, h, pad)),
                    ("CLR", cancel_seat(w, h, pad)),
                    ("PAGE", quickpage_seat(w, h, pad)),
                ];
                for (i, s) in seats.iter().enumerate() {
                    assert!(s.w > 0 && s.h > 0, "no degenerate seat at {w}x{h} mask {mask:#04x}");
                    assert!(s.x + s.w <= w && s.y + s.h <= h, "seat {i} on-screen at {w}x{h} mask {mask:#04x}: {s:?}");
                    for (j, t) in seats.iter().enumerate().skip(i + 1) {
                        assert!(discs_clear(*s, *t), "seats {i}/{j} overlap at {w}x{h} mask {mask:#04x}: {s:?} {t:?}");
                    }
                }
                // Seat 0 is where CLR's seat is derived from, not a plate; the plates are 1..
                for (i, s) in seats.iter().enumerate().skip(1) {
                    for (name, o) in &others {
                        assert!(discs_clear(*s, *o), "plate {i} {s:?} touches {name} {o:?} at {w}x{h} mask {mask:#04x}");
                    }
                }
                // Every VISIBLE plate's built-in is a live seat; a hidden plate answers
                // with a real (full-ring) rect the resolver can divide by, and the
                // survivors never shrink below the full ring's plate.
                let full = ring_seat_full(w, h, pad, 1).w;
                for p in 0..crate::quickpage::PLATES as u8 {
                    let r = plate_seat(w, h, pad, p);
                    assert!(r.w > 0 && r.h > 0, "plate {p} built-in degenerate at {w}x{h} mask {mask:#04x}");
                    if let Some(vi) = crate::quickpage::visible_index(mask, p) {
                        assert_eq!(r, seats[vi + 1], "plate {p} sits at live seat {} at {w}x{h} mask {mask:#04x}", vi + 1);
                        assert!(r.w >= full, "a survivor never shrinks: {} < {full} at {w}x{h} mask {mask:#04x}", r.w);
                    }
                }
            }
        }
    }

    /// ★ 2026-10-01, player: *"세이브 버튼과 map버튼을 hero3처럼 상단에 붙은 버튼형태로 변경해줘.
    /// 클릭영역은 실제버튼보다 더 커야해"*. SAVE and MAP are tabs on the top edge, MAP left of
    /// SAVE, each answering to more than it draws — and the bigger targets must neither
    /// overlap each other nor reach the game's target box or the minimap panel beside them.
    #[test]
    fn save_and_map_are_top_tabs_with_bigger_targets() {
        for (w, h) in WINDOWS {
            let pad = pad_of(w, h);
            let save = quicksave_seat(w, h, pad);
            let map = minimap_toggle_seat(w, h, pad);
            let size = top_tab_size(w, h, pad);
            assert_eq!(((save.w, save.h), (map.w, map.h)), (size, size), "one size at {w}x{h}");
            assert_eq!(size, (ring_seat_full(w, h, pad, 1).w, ring_seat_full(w, h, pad, 1).w * 3 / 5), "a plate wide, 3/5 tall at {w}x{h}");
            assert!(save.y == 0 && map.y == 0, "hanging from the top edge at {w}x{h}: {save:?} {map:?}");
            assert!(save.x + save.w <= w && map.x + map.w <= w, "on screen at {w}x{h}");
            assert!(map.x + map.w < save.x, "MAP left of SAVE, apart, at {w}x{h}: {map:?} {save:?}");
            let (hs, hm) = (top_tab_hit(save, w, h), top_tab_hit(map, w, h));
            for (tab, hit) in [(save, hs), (map, hm)] {
                assert!(
                    hit.x <= tab.x && hit.y <= tab.y && hit.x + hit.w >= tab.x + tab.w && hit.y + hit.h >= tab.y + tab.h,
                    "the target covers the tab at {w}x{h}: {hit:?} {tab:?}"
                );
                assert!(hit.w > tab.w && hit.h > tab.h, "and is bigger than it at {w}x{h}: {hit:?} {tab:?}");
                assert!(hit.x + hit.w <= w && hit.y + hit.h <= h, "and on screen at {w}x{h}");
            }
            assert!(!hs.intersects(&hm), "the two targets never overlap at {w}x{h}: {hs:?} {hm:?}");
            let game = Rect::new(0, 0, w, h);
            let target = game_target_box_at(game, minimap_panel_builtin(w, h));
            assert!(!hs.intersects(&target), "SAVE's target stops short of the game's box at {w}x{h}: {hs:?} {target:?}");
            assert!(!hs.intersects(&minimap_panel_builtin(w, h)), "…and of the minimap panel at {w}x{h}");
            // Landscape only: the tabs are drawn and hit-tested in the landscape-immersive
            // field alone (`App::quicksave_zone` / `map_tab_zone`), and a portrait window is
            // too narrow for the built-in seat to clear a gauge block it never shares a
            // screen with (1080x2340: MAP lands at x ≈ 270, inside the block).
            if w > h {
                assert!(!hm.intersects(&stats_block_box(game)), "MAP's target clears our gauges at {w}x{h}");
            }
        }
        // hero3's own numbers on the S25 (vc422, measured 2026-10-01): 86×51, half a tab apart.
        let pad = pad_of(2340, 1080);
        let (save, map) = (quicksave_seat(2340, 1080, pad), minimap_toggle_seat(2340, 1080, pad));
        assert_eq!((save.w, save.h), (86, 51), "hero3's tab size at 2340x1080");
        assert_eq!(save.x - (map.x + map.w), 43, "hero3's gap at 2340x1080");
    }

    #[test]
    fn a_top_tab_is_open_at_the_top_round_at_the_bottom_and_lettered() {
        const BG: u32 = 0xff40_4040;
        let (w, h) = (300u32, 120u32);
        let mut buf = vec![BG; (w * h) as usize];
        let r = Rect::new(50, 0, 86, 51);
        draw_top_tab(&mut buf, w, h, r, "SAVE");
        let at = |x: u32, y: u32| buf[(y * w + x) as usize];
        for y in 0..h {
            for x in 0..w {
                if !(r.x..r.x + r.w).contains(&x) || !(r.y..r.y + r.h).contains(&y) {
                    assert_eq!(at(x, y), BG, "({x},{y}) is outside the tab");
                }
            }
        }
        // The rim on the three sides that are not the window's edge — opaque gold.
        assert_eq!(at(r.x, 25), TAB_RIM, "left rim");
        assert_eq!(at(r.x + r.w - 1, 25), TAB_RIM, "right rim");
        assert_eq!(at(r.x + r.w / 2, r.h - 1), TAB_RIM, "bottom rim");
        // …and none along the top: the screen's edge is the tab's top. The face there is
        // the dark see-through one.
        let top = at(r.x + r.w / 2, 0);
        assert_ne!(top, TAB_RIM, "no rim on top");
        assert!((top >> 16) & 0xff < (BG >> 16) & 0xff && top != 0xff0a_0c12, "a translucent dark face: {top:#010x}");
        // Round bottom corners: the very corner pixel is outside the arc.
        assert_eq!(at(r.x, r.h - 1), BG, "bottom-left corner cut");
        assert_eq!(at(r.x + r.w - 1, r.h - 1), BG, "bottom-right corner cut");
        assert_eq!(at(r.x, 0), TAB_RIM, "the top corners are square");
        // The label is drawn in the HUD's white.
        assert!(buf.iter().any(|p| *p == LABEL), "lettered");
    }

    /// The two tabs are fixed seats (2026-10-01): no stored placement for them — from the
    /// editor, FLIP or an old `wie_layout.txt` line for the round buttons they replaced.
    #[test]
    fn the_top_tabs_are_not_placeable() {
        use crate::placement::{CtrlId, Place, Table, in_editor, is_top_tab_line, parse_line};
        let mut t = Table::default();
        let p = Place { cx: 0.5, cy: 0.5, pct: 120 };
        for id in [CtrlId::QuickSave, CtrlId::MinimapToggle] {
            assert!(id.is_top_tab() && !id.is_round());
            t.set(id, Some(p));
            assert_eq!(t.get(id), None, "{id:?} keeps no placement");
            assert!(!in_editor(Mode::Field, id), "{id:?} is not in the editor");
        }
        t.set(CtrlId::QuickPage, Some(p));
        assert_eq!(t.get(CtrlId::QuickPage), Some(p), "the other controls still place");
        assert!(is_top_tab_line("field qsave 0.6812 0.0823 100") && is_top_tab_line("field mapt 0.9 0.1 100"));
        assert!(!is_top_tab_line("field qpage 0.6 0.5 100") && !is_top_tab_line("field map 0.94 0.12 130"));
        assert_eq!(parse_line("field qsave 0.6812 0.0823 100"), None);
        assert!(parse_line("field map 0.9401 0.1211 130").is_some());
    }

    /// 시각 목업 (기기 없이 배치 확인용): `HUD_MOCK_OUT=<파일.ppm>` 을 주면 필드 모드의
    /// 빌트인 배치를 P6 PPM 으로 그린다. 평소에는 아무것도 하지 않는 no-op 테스트.
    ///     HUD_MOCK_OUT=/tmp/hud.ppm cargo test -p wie_android write_hud_mock
    #[test]
    fn write_hud_mock() {
        let Ok(out) = std::env::var("HUD_MOCK_OUT") else { return };
        let (w, h) = (2340u32, 1080u32);
        let mut buf = vec![[18u8, 22, 28]; (w * h) as usize];
        let mut fill = |r: Rect, c: [u8; 3]| {
            for y in r.y..(r.y + r.h).min(h) {
                for x in r.x..(r.x + r.w).min(w) {
                    let p = &mut buf[(y * w + x) as usize];
                    for k in 0..3 {
                        p[k] = ((p[k] as u16 * 2 + c[k] as u16 * 3) / 5) as u8;
                    }
                }
            }
        };
        use crate::placement::{CtrlId, FIELD_IDS, builtin};
        for id in FIELD_IDS {
            let r = builtin(id, Mode::Field, w, h);
            let c = match id {
                CtrlId::Ok => [230, 90, 60],
                CtrlId::Cancel => [240, 200, 70],
                CtrlId::Quick(_) => [90, 170, 240],
                CtrlId::Minimap => [90, 220, 140],
                CtrlId::MinimapToggle => [50, 140, 90],
                CtrlId::Stick => [160, 160, 170],
                CtrlId::QuickPage => [240, 150, 40],
                CtrlId::QuickSave => [240, 60, 120],
            };
            fill(r, c);
        }
        fill(stats_block_box(Rect::new(0, 0, w, h)), [220, 220, 90]);
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        ppm.extend(buf.iter().flatten());
        std::fs::write(&out, ppm).unwrap();
    }
}
