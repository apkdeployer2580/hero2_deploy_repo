//! winit UI-thread handler. Blits frames the emulator worker pushes, draws the
//! orientation-aware control overlay + settings gear, and forwards touch input.
//! The softbuffer surface is recreated across suspend/resume (Android destroys
//! the native window when backgrounded).

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::{Duration, Instant},
};

use softbuffer::{Context, Surface};
use winit::{
    application::ApplicationHandler,
    event::{Touch, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    window::{Window, WindowId},
};

use wie_backend::KeyCode;

use crate::{
    CONTENT_H, CONTENT_W, InputEvent, QuickSlot, SlotKind, Stats, UserEvent, config, orientation, overlay, placement,
    quickslot_hotkey, server_prompt,
};
use std::sync::mpsc::Receiver;

pub struct App {
    input_tx: Sender<InputEvent>,
    data_dir: PathBuf,

    /// Shared background/lock gate (see lib.rs). Set while the app is not in the
    /// foreground so the emulator stops ticking and the synth emits silence.
    paused: Arc<AtomicBool>,

    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,

    /// RENDER size — the surface buffer the frame is drawn at, and the coordinate
    /// space of every layout rect and (scaled) touch point. Equal to the physical
    /// window unless the HEAT SAVER halves it (see [`Self::render_div`]).
    win_w: u32,
    win_h: u32,
    /// HEAT SAVER render divisor (57차): 1 = native, 2 = draw at half resolution.
    /// `Surface::resize` maps to `ANativeWindow_setBuffersGeometry`, whose contract
    /// is exactly this: a buffer smaller than the window is scaled to the screen by
    /// the compositor, in hardware, for free — so every pixel-proportional cost on
    /// this thread (clear, game blit, overlay draws, the present swizzle) drops 4x.
    /// Fixed at process start (`config::read_heat_saver`); physical touch
    /// coordinates are divided by this on entry so everything downstream — layout,
    /// hit-tests, the editor, the stored (fractional) placements — stays in render
    /// space and needs no other change.
    render_div: u32,

    layout: overlay::Layout,
    settings_items: Vec<overlay::SettingsItem>,
    show_settings: bool,
    /// ★ 2026-09-24: how far the settings list is scrolled (window px), and the finger
    /// that may be scrolling it: `(touch id, landing x, landing y, scroll at landing,
    /// moved past the tap slop)`. A press inside the list is decided on RELEASE — a tap
    /// if it never moved, a scroll if it did.
    settings_scroll: u32,
    settings_drag: Option<(u64, f64, f64, u32, bool)>,
    /// The layout editor, while it is open (player request 2026-07-31: place and
    /// size every control by hand). `Some` takes over ALL touch input.
    editor: Option<Editor>,
    /// 설정 → 패드 버튼 설정, while it is open (2026-10-09). Modal like the editor: every
    /// touch and every pad / keyboard press goes to it.
    padmap: Option<Box<PadMapUi>>,

    /// Last content frame from the worker (CONTENT_W x CONTENT_H, 0xAARRGGBB).
    last_frame: Vec<u32>,
    /// ★★ 2026-10-15 「악마화」 HD: the effects the frame in `last_frame` recorded, composited over its blit.
    hd_frame: Option<Box<wie_backend::canvas::HdFrame>>,
    /// Nearest-neighbour COLUMN map for the game blit: entry `dx` is the source column
    /// that destination column `dx` samples. See the free function [`sx_lut`].
    sx_lut: Vec<u32>,
    /// The `(game.w, logical_w)` the cached [`Self::sx_lut`] was built for.
    ///
    /// ★ Those two values are the COMPLETE input set of `sx_lut(dst_w, src_w)` — it is a
    /// pure function of nothing else — so this key cannot silently go stale. That is the
    /// whole risk of caching here: a map kept across a rotation, a layout edit or a window
    /// resize would sample the wrong columns, and a sheared picture has nothing in the code
    /// to point at. `logical_w` is fixed for the life of the process after `init_logical`,
    /// but it is IN the key rather than assumed, because assuming it is exactly the kind of
    /// invariant that quietly stops holding.
    sx_lut_key: (u32, u32),
    /// Rasterised touch controls, re-rasterised only when they actually change (#4).
    /// See [`overlay::ControlsLayer`] — on a phone standing still this is the
    /// difference between re-drawing every soft disc and ring per present and
    /// compositing the pixels that were already correct.
    hud_controls: overlay::ControlsLayer,
    /// The seven plates, same mechanism — see [`overlay::QuickbarLayer`].
    hud_quickbar: overlay::QuickbarLayer,

    /// Active touch id -> the key it currently maps to (game controls).
    touches: HashMap<u64, KeyCode>,
    /// Touch ids consumed by UI (gear/settings) — excluded from game input.
    ui_touches: HashSet<u64>,
    /// Touch ids that STARTED on a control. Only these may change which key they
    /// hold by sliding (`TouchPhase::Moved`).
    ///
    /// ★ 2026-07-30: without this, ANY touch that had not been adopted by the
    /// joystick — e.g. a second finger while the first one is steering — acquired
    /// whatever button it happened to slide over. With the minimap panel sitting in
    /// the top-right corner as a plain `KeyCode::STAR` button, that is precisely the
    /// player-reported "the minimap keeps getting clicked by itself".
    slide_touches: HashSet<u64>,
    /// In-flight deliberate tap on the minimap panel: `(id, down_x, down_y)`.
    /// The panel is no longer a Button, so it cannot fire on touch-down or by being
    /// slid over; it fires on RELEASE, and only for a real tap.
    mm_tap: Option<(u64, f64, f64)>,
    /// In-flight deliberate tap on the Lv/HP gauge block, which opens the game's
    /// main menu (`CLEAR`). Same tap discipline as `mm_tap`, for the same reason —
    /// see `overlay::stats_tap_box`.
    stats_tap: Option<(u64, f64, f64)>,
    /// Gamepad button overrides from `wie_gamepad.txt` (58차; empty = built-in
    /// layout). Read once — remapping is an app restart, like every other file knob.

    /// Guest keys currently held DOWN by a gamepad button or the keyboard, with
    /// whether that down was diverted to `InputEvent::PopupConfirm` (same pairing
    /// rule as `popup_confirm_touches`: the UP must match what the DOWN actually
    /// DID, not the popup state at release time). Keyed by GUEST key so a down/up
    /// pair stays matched even if the player remaps mid-hold cannot happen (map is
    /// fixed per run) — and so focus loss can release exactly what is held.
    pad_held: HashMap<KeyCode, bool>,
    /// The one latched walk direction the gamepad stick/hat currently holds —
    /// `overlay::joy_dir` state, exactly like the touch joystick's `Joy::dir`.
    pad_stick: Option<KeyCode>,
    /// Analog-trigger digital states `(L2, R2)` — see `gamepad::trigger_held`.
    pad_triggers: (bool, bool),
    /// Keys currently held by any touch or the joystick (for highlight).
    pressed: HashSet<KeyCode>,
    /// Left control style: floating joystick (true) vs classic D-pad.
    joystick: bool,
    /// The active joystick drag, if any.
    joy: Option<Joy>,
    /// ★ A finger that WAS steering and is still down, but whose stick the app had to
    /// let go of because the field went away for a moment (a map banner, a scripted
    /// camera move — anything that flips `mode` off Field and takes the stick's zone
    /// with it). Without this the player must lift and press again: the stick is gone,
    /// so `TouchPhase::Moved` matches nothing and their thumb steers a control that no
    /// longer exists. Player report (2026-09-18): *"손을 뗀적이 없는데 갑자기 조이스틱이
    /// 손을 뗀것처럼 한번 끊긴다음에 현재 위치로 조이스틱이 구성"*. The next move from this
    /// id re-forms the stick under the thumb. See `joy_park`.
    joy_parked: Option<u64>,
    /// Field HUD vs game-window (menu) controls — driven by the guest state the
    /// worker reports. See `overlay::Mode`.
    mode: overlay::Mode,
    /// The pull-out full keypad (immersive mode) is showing.
    keypad_open: bool,
    /// The script window is showing a CHOICE LIST the player has to answer
    /// (`items::script_choice_open`). The ONE thing that separates "a cutscene is
    /// playing" from "a dialogue is asking me something" — window id 4 is both.
    choice: bool,
    /// The game's own map window is open -> paint a big copy of the map over it.
    map_open: bool,
    /// The host world map (the MAP tab) is open. The picture is larger than the
    /// screen; `world_cam` is the window of it, framed on `stage_map` when it opens.
    world_open: bool,
    world_cam: overlay::WmCam,
    /// Last live stage map number. 0 until the worker has read one.
    stage_map: u32,
    /// Stick direction already used to slide the world map, so a held stick pans once.
    world_stick: Option<KeyCode>,
    /// The window is still the one `wm_open` chose. A later map-number reading may
    /// reframe it; the first arrow the player presses ends that.
    world_pinned: bool,
    /// Which place the cursor is on. Starts as the character's place.
    world_sel: usize,
    /// Where the camera is heading: `world_cam` glides toward this a frame at a
    /// time (hero5 halves the distance each frame — `overlay::wm_glide`).
    world_target: overlay::WmCam,
    /// When the cursor last moved — the pin bounces for a moment after it.
    world_sel_at: Instant,
    /// Next animation frame of the open map (glide, pin bounce, stud pulse).
    /// `None` until the map is open.
    world_blink_at: Option<Instant>,
    /// Out-of-world title/slot screen -> letterbox its 240-px art.
    shell: bool,
    /// The game is painting a bottom-anchored overlay (hero2 dialogue box) -> show the
    /// BOTTOM of the framebuffer, not the top. See `game_source_window`.
    ///
    /// ★ 2026-09-18: the worker derives this from the script box's own MODE byte
    /// (`items::bottom_overlay_open`), not from the state word alone and not from the
    /// pixels. A pixel detector was tried the same day and withdrawn: with the game's
    /// HUD painter switched off nothing repaints the reserved rows, so the LAST box
    /// stays there as stale ink and read as "a box is up" for ever after.
    bottom_overlay: bool,
    /// Published to the emulator worker: how far down the game's own top HUD band
    /// must go so it sits below our gauges (guest px). See `overlay::hud_y_shift`.
    hud_y: crate::HudYShift,

    /// Live field quickbar (5 slots) for the action HUD (from the worker).
    quickbar: Vec<QuickSlot>,
    /// Live player gauges for the top-left HUD.
    stats: Stats,
    /// Live cash-buff status lines (ASCII, from the worker — it owns the clock).
    /// Drawn under the gauge block while a game window is open.
    buff_lines: Vec<String>,
    /// Host-side skill cooldowns, keyed by SKILL IDENTITY (atlas sheet, strip cell)
    /// rather than by seat, so a page flip or re-bind cannot transplant a running
    /// cooldown onto a different skill. Value = (when the cast was CONFIRMED, that
    /// skill's cooldown length in ms). Entries are only ever inserted by
    /// [`UserEvent::SkillCast`] — the worker's verdict that the cast actually
    /// started — never by the tap itself (2026-08-07: "스킬이 실행 됐을때만 쿨타임이
    /// 돌아야해"). A tap on a plate whose cooldown is still running is swallowed,
    /// and the plate draws a bottom-up recovery fill.
    skill_cd: HashMap<(u8, u8), (Instant, u64)>,
    /// Per-skill cooldown lengths from `wie_cooldowns.txt` (see
    /// [`config::read_skill_cooldowns`]); a skill not listed there has NO cooldown
    /// — the game's own cast delay is the only gate, which is the original
    /// behaviour. Host-owned data: the game has no cooldown state to read.
    skill_cds_cfg: HashMap<(u8, u8), u64>,
    // ★★ 2026-10-20: the host's status line (`toast`, drawn centre-bottom) is gone — every such line is a notice in
    //   the game's own box at the left now (`host_notice` → the worker → `UserEvent::PickupNotice` → `notices`).
    /// The emulator worker has stopped — the lines to keep on screen for the rest of
    /// this process's life. `None` while the guest is alive.
    ///
    /// ★ Kept as state rather than raised as a toast because there is no next frame from
    /// the game: the window, the compositor and the music all carry on, so a notice that
    /// expires leaves a player looking at a frozen picture of a running game. See
    /// [`crate::UserEvent::WorkerDied`] and `overlay::draw_fatal_notice`.
    dead: Option<Vec<&'static str>>,
    /// Wall clock for time-based overlay animation. Only the unspent-point badges
    /// use it (~2 Hz blink); a frame counter would blink at a device-dependent rate,
    /// and the guest's own blink flag is frozen by the HUD-skip patch — see
    /// [`overlay::BADGE_BLINK_MS`].
    started: Instant,
    /// The game's minimap (top-right panel); `None` until it has been painted.
    minimap: Option<crate::Minimap>,
    /// Cached button rects for `quickbar`, recomputed on relayout / change.
    quick_rects: Vec<overlay::Rect>,
    /// The touch id + index currently pressing a quickbar button (for highlight).
    quick_press: Option<(u64, usize)>,
    /// The game's rewritten context popup ("퀵바등록"), while one is on screen. Its
    /// only job on this side is to tell OK apart: with a popup up, a confirm belongs
    /// to the popup and the WORKER decides what it means (see `InputEvent::PopupConfirm`).
    popup: Option<crate::items::PopupView>,
    /// ★ #13: the inventory page strip, while the game's inventory window is up.
    bag_page: Option<crate::BagPageView>,
    /// Touch ids whose DOWN was diverted to [`InputEvent::PopupConfirm`] instead of
    /// being sent as a real key.
    ///
    /// ★ 2026-08-06. Without this the UP edge re-decided the question by reading
    /// `popup` a second time — and `popup` CHANGES between the two edges, because the
    /// press is what opens or closes the popup. Both orders are wrong:
    ///
    /// * a press that OPENS the popup sends keydown (popup was `None`), then finds
    ///   `Some` at release and SUPPRESSES the keyup — the guest is left holding OK, and
    ///   the worker's `held` map auto-repeats it every 100 ms;
    /// * a press that CONFIRMS on the popup sends no keydown (popup was `Some`), then
    ///   finds `None` at release and sends a BARE KEYUP — exactly the "key the guest
    ///   never saw pressed" the code below warns about.
    ///
    /// They alternate, so every popup cycle inverts the guest's OK state and the next
    /// press is spent putting it back: "선택을 한번하면 아무동작없고 두번째 한번 더
    /// 선택하면 그때 뜬다" (player, 2026-08-06). Remembering the DOWN decision is what
    /// makes the pair symmetric.
    popup_confirm_touches: HashSet<u64>,
    /// The worker asked us to highlight the quickbar plates and wait for a tap.
    picking_slot: bool,
    /// Which host-owned quickbar page is live, as last published by the WORKER (never
    /// derived from our own taps — a refused flip would leave the label lying).
    quick_page: u8,
    /// Whether the minimap panel is shown. Player request 2026-08-04: the panel is a
    /// toggle you can travel with on or off, and tapping it hides it. ★ Since 2026-10-01
    /// the `MAP` tab on the top edge ([`placement::CtrlId::MinimapToggle`], hero3's form)
    /// is on screen either way and flips it both ways — it used to be a round button drawn
    /// in the panel's place only while the panel was hidden. Runtime only: a hidden
    /// minimap is a per-session choice, not a setting worth a file.
    minimap_on: bool,
    /// The game's own save-inhibitor byte is clear (from the worker's HUD sample).
    /// False marks a no-save AREA — in this game the boss rooms — where the SAVE
    /// button and the whole minimap UI (panel, toggle, tap zone) are hidden
    /// (player request 2026-08-07).
    can_save: bool,
    /// The live player dot on the minimap, in the picture's own pixel space — see
    /// `UserEvent::MinimapMarker`. Kept apart from `minimap` because it changes on a
    /// different clock.
    minimap_marker: Option<(f32, f32)>,
    /// ★ 2026-09-29: the monsters' squares on that minimap, same units — see
    /// `UserEvent::MinimapMobs`.
    minimap_mobs: Vec<(f32, f32)>,
    /// ★ A3b: the portrait's head as the game loaded it (a costume's included) — see
    /// `UserEvent::Portrait`. `None` = the atlas head for `stats.head`.
    portrait: Option<crate::guest_build::H2Image>,
    /// ★ 2026-10-05: the skill buffs running, with their seconds left — see `UserEvent::SkillBuffs`.
    skill_buffs: Vec<crate::guest_build::H2BuffShow>,
    /// ★ 2026-10-07 (2): the berserker's 격분 gauge while it runs — ★ 2026-10-13 D4 and 추종's: `(buff id, sheet, gauge)`,
    /// see `UserEvent::BuffGauges`.
    buff_gauges: Vec<(u8, u8, u8)>,
    /// ★ 2026-09-24: the pickup notices on screen, oldest first — see
    /// `UserEvent::PickupNotice` and `overlay::draw_notices`.
    notices: std::collections::VecDeque<overlay::Notice>,
    /// ★ 2026-09-24: the host's rows of the game's 환경설정 — see
    /// `UserEvent::OptionsOverlay`. Laid over the game's frame, in its pixels.
    options_patches: Vec<crate::guest_build::H2Patch>,
    /// ★ 2026-09-25: the 부활의 구 box — see `UserEvent::RevivePrompt`. Same space; a tap
    /// inside it is the worker's (`revive_prompt_hit`).
    revive_patches: Vec<crate::guest_build::H2Patch>,
    /// ★ 의상 아바타 A3: the EQUIPMENT window's avatar side — see `UserEvent::EquipOverlay`.
    equip_patches: Vec<crate::guest_build::H2Patch>,
    /// ★★ 2026-10-16: the 정렬 / 일괄판매 buttons — see `UserEvent::BagTools`.
    bag_tool_patches: Vec<crate::guest_build::H2Patch>,

    /// Emulator boot is DEFERRED until the first `resumed`, because the guest
    /// framebuffer size is derived from the real window size (landscape
    /// immersive mode) and the WIPI screen framebuffer can only be allocated
    /// once. `None` after the worker has been spawned.
    pending_boot: Option<PendingBoot>,
    /// ★ 2026-10-28: the blocked start's dialog is up (`vault::boot_blocked`) — shown once.
    blocked_shown: bool,
}

/// Everything the emulator worker needs, held until the window size is known.
pub struct PendingBoot {
    pub proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    pub should_redraw: Arc<AtomicBool>,
    pub input_rx: Receiver<InputEvent>,
    pub game_zip: Vec<u8>,
    pub hud_y: crate::HudYShift,
}

/// Was `(x, y)` close enough to the touch-down point to count as a TAP rather than
/// a drag? Deliberately generous (fingers wobble) but far below the size of the
/// panel it guards, so a swipe that merely crosses the panel never counts.
const TAP_SLOP: f64 = 28.0;

/// How long the stick must keep asking for a NEW direction before that turn is sent to
/// the guest — see [`App::settle_joy`] for the measurement this comes from.
///
/// ★★ 0 since 2026-09-24 (player: *"좌측으로 이동하다 우측으로 바꾸면 0.1초뒤에 우측으로
/// 이동"*). It was 50 ms = one guest frame, and it bought protection from two things
/// that are no longer there:
///  * "queued commitments" — §8.14 measured the game's walk as a ONE-SLOT request
///    (`ag.k`/`ag.l`, last request wins at the tile edge), so a sweeping thumb queues
///    nothing; the ~44 px per keydown of 2026-09-19 was a release deferred by a frame,
///    not a queue;
///  * a turn turning into a stop when two turns fell into one guest frame — the deferred
///    release of the middle direction landed last. A turn is now `InputEvent::Turn`, and
///    on the field the worker forwards only its press, so there is no release to defer.
/// What it cost was every turn, 50 ms, on top of the game's own tile-edge wait. Kept as
/// a constant (not deleted) so the knob is one line if a device ever needs it back.
const TURN_SETTLE: Duration = Duration::ZERO;

fn is_tap(dx: f64, dy: f64, x: f64, y: f64) -> bool {
    (x - dx).abs() <= TAP_SLOP && (y - dy).abs() <= TAP_SLOP
}

/// Nearest-neighbour column map for the game blit: entry `dx` is `(dx * src_w) / dst_w`
/// — exactly what the old inner loop recomputed with an integer DIVIDE for every single
/// destination pixel. At 1080x2340 that was ~2.5 M divides per presented frame, on top of
/// the 2.46 M guest instructions the same frame already costs (measured with `wie_probe`,
/// 2026-08-06); the map is `dst_w` entries and changes only when the geometry does.
///
/// A FREE function taking only its two inputs, deliberately. `cargo test` cannot run in
/// this crate (the linked winit/android deps kill the test binary on this host) and
/// `app.rs` is not one of the files the standalone root selftests `include!`, so the only
/// review this logic can get is a human reading it — which needs it to be independent of
/// `App`.
///
/// Bound the blit relies on: every entry is `< src_w`. For `dx <= dst_w - 1`,
/// `dx * src_w <= (dst_w - 1) * src_w < dst_w * src_w`, so the floor division by `dst_w`
/// yields at most `src_w - 1`. Done in u64 so the product cannot wrap at any window size
/// (u32 would be fine for real devices, but "fine for real devices" is not a proof).
pub(crate) fn sx_lut(dst_w: u32, src_w: u32) -> Vec<u32> {
    let d = u64::from(dst_w.max(1));
    (0..dst_w).map(|dx| (u64::from(dx) * u64::from(src_w) / d) as u32).collect()
}

/// 설정 → 패드 버튼 설정 (2026-10-09, with hero3). The rules — swap, refusal, the stored
/// diff — are `padmap.rs`; what is on screen is `overlay::padmap_scene` /
/// `overlay::draw_padmap`.
///
/// User decisions 2026-10-09: a conflict SWAPS; the screen is driven by TOUCH (pick a
/// control) and the PAD (press the button) — no pad navigation, because the button being
/// remapped may be the very OK that would confirm it. Second form, the same day, for both
/// games: the control is picked ON THE IN-GAME HUD, lit where it is played (*"실제 인게임
/// 화면상에 버튼들을 하이라이트 처리하고 각 버튼을 눌르고 패드 버튼을 누르는 방식 … hero2도
/// 마찬가지야"*) — not from a list.
struct PadMapUi {
    /// The table being edited — a copy of the one in force. Nothing reaches
    /// `gamepad::set_overrides` or the file until 저장; 취소 just drops this.
    draft: HashMap<i32, Option<crate::gamepad::PadAction>>,
    /// The function picked last (a `gamepad::REMAP_ROWS` index): lit while it waits for a
    /// button, and after — so the player sees which one they just changed.
    sel: Option<usize>,
    /// Waiting for a button for `sel` since this instant (`padmap::CAPTURE_MS`).
    capture: Option<Instant>,
    /// The function a BARE press (nothing picked) fires — lit in blue beside the answer.
    probe: Option<usize>,
    /// The window's two lines: what happened, and what to do. The second gives way to the
    /// countdown while a button is awaited.
    lines: [String; 2],
}

/// What `redraw` draws for the pad screen (`App::padmap_view`).
struct PadMapView {
    scene: overlay::PadMapScene,
    /// The plates as the field has them — the worker's last list by plate, an empty plate
    /// past it (the title screen publishes none).
    slots: Vec<crate::QuickSlot>,
    /// One per target: the button that fires it in the draft.
    badges: Vec<Option<crate::padhud::PadPos>>,
    sel: Option<usize>,
    probe: Option<usize>,
    /// `Some` while a button is awaited — the lit control breathes, and frames are owed.
    pulse: Option<f64>,
    lines: [String; 2],
    can_reset: bool,
    /// Does the draft differ from the table in force — is there anything for 저장 to save?
    dirty: bool,
}

/// The layout editor's state.
///
/// ★★ There is no `mode` here any more (2026-09-18, with `placement::Table`). The
/// editor always arranges the FIELD control set, in simulation — the settings gear does
/// not exist in the field (`overlay::layout` gives it a zero rect there), so that is the
/// only way those controls can be arranged at all, and the menu controls are a subset
/// that now READ THE SAME SEATS. The old "게임진행 / 메뉴오픈" page switch is gone with
/// the second table: moving a control on the GAME page used to leave the same control
/// somewhere else the moment a window opened.
struct Editor {
    sel: Option<placement::CtrlId>,
    /// The layout as it was when the editor opened, for `CANCEL`. Edits are written to
    /// disk continuously (the app can be killed from the background at any moment), so
    /// backing out has to restore a snapshot rather than "not save".
    entry_snapshot: placement::Table,
    /// The hidden-plate set as it was when the editor opened (hero3 82차), for the same
    /// reason and with the same rule: HIDE / SHOW apply and persist immediately, so
    /// CANCEL has to put this back too.
    hidden_snapshot: u8,
    /// The PAD preview (2026-09-25): the in-play gamepad badges drawn on the boxes. A view
    /// toggle, not an edit — nothing is stored, CANCEL has nothing to undo, and it starts
    /// OFF every time the editor opens.
    pad: bool,
    /// The finger moving the selection.
    drag: Option<Drag>,
    /// Two-finger resize of the selection.
    pinch: Option<Pinch>,
    /// Live position of every finger currently down.
    ///
    /// ★ Needed because winit's Android backend reports `Moved` for EVERY pointer in
    /// the MotionEvent, not just the one that moved (platform_impl/android/mod.rs:
    /// `TouchPhase::Moved | Cancelled => Box::new(motion_event.pointers())`). A pinch
    /// computed from per-event deltas would therefore double-apply; the size is
    /// derived from the anchor captured at pinch start instead.
    points: HashMap<u64, (f64, f64)>,
}

/// One in-progress move. Stored as "where the finger went down" plus "where the
/// control was then", so the control follows the finger's DELTA — a drag that starts
/// on empty screen (which is normal now that the selection is sticky) must not
/// teleport the control under the fingertip.
#[derive(Clone, Copy)]
struct Drag {
    id: u64,
    from: (f64, f64),
    centre0: (f64, f64),
}

struct Pinch {
    a: u64,
    b: u64,
    /// Finger distance when the pinch started.
    dist0: f64,
    /// The control's size percent when the pinch started.
    pct0: u32,
}

fn distance(p: (f64, f64), q: (f64, f64)) -> f64 {
    ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()
}

/// One in-progress joystick drag: which touch owns it, the base centre, the current
/// thumb position, and the single direction key it holds.
///
/// `cx`/`cy` is where the finger went DOWN and it does NOT move for the life of the
/// gesture — a new centre costs a lift and a fresh touch. `App::joy_update` has the
/// three reverted attempts at a centre that follows.
struct Joy {
    id: u64,
    cx: f64,
    cy: f64,
    tx: f64,
    ty: f64,
    /// The direction the GUEST is holding — what `joy_end` has to release.
    dir: Option<KeyCode>,
    /// The direction the maths currently asks for, which may not have been sent yet.
    /// See `App::settle_joy`.
    want: Option<KeyCode>,
    /// When `want` last changed, so a turn can be required to last.
    want_since: Instant,
}

/// The notice stack at the left (`overlay::draw_notices`): on the field under the controls, lifted clear of the
/// walk stick's seat and capped just under the gauges (★ 2026-09-25, measured on this screen); anywhere else —
/// the shell, a window, the editor — the game's own row with no ceiling. Returns whether the stack is still
/// sliding or waiting to expire (the caller asks for the next frame). A free function over the fields it needs:
/// the frame buffer borrows the window while it is drawn, so `&mut self` is not available here.
#[allow(clippy::too_many_arguments)]
fn notices_pass(
    notices: &mut std::collections::VecDeque<overlay::Notice>,
    game: overlay::Rect,
    joystick_on: bool,
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    fb: (u32, u32, u32, u32),
    field: bool,
) -> bool {
    if notices.is_empty() {
        return false;
    }
    let avoid = if field { placement::rect_of(placement::CtrlId::Stick, overlay::Mode::Field, win_w, win_h).filter(|_| joystick_on || win_w > win_h) } else { None };
    let ceiling = field.then(|| overlay::stats_bottom(game));
    overlay::draw_notices(buf, win_w, win_h, game, fb, notices, avoid, ceiling)
}

impl App {
    pub fn new(
        input_tx: Sender<InputEvent>,
        data_dir: PathBuf,
        paused: Arc<AtomicBool>,
        hud_y: crate::HudYShift,
        pending_boot: PendingBoot,
    ) -> Self {
        let joystick = config::read_joystick(&data_dir);
        // Per-skill cooldown overrides, read once: the file only changes by hand.
        let skill_cds_cfg = config::read_skill_cooldowns(&data_dir);
        // The custom layout lives in a process global (the overlay geometry has no
        // `data_dir` in hand), so it has to be seeded from disk BEFORE the first
        // layout is built — otherwise the player's arrangement would only appear
        // after the next relayout.
        placement::set_table(config::read_layout(&data_dir));
        // Same reasoning as the layout table above: the touch type is read by
        // `overlay::layout_immersive` and `draw_joystick`, neither of which has a
        // `data_dir`, so it lives in a process global that must be seeded before the
        // first layout is built.
        crate::set_touch_type(config::read_touch_type(&data_dir));
        // Same again for the hidden quickslot set (hero3 82차): `quick_buttons`,
        // `plate_seat` and `placement::shown` all read it while building the FIRST
        // layout, so a plate the player switched off has to be off before that runs —
        // otherwise it flashes on screen for one frame and, worse, takes its old ring
        // seat until something else forces a relayout.
        crate::set_quick_hidden(config::read_quick_hidden(&data_dir));
        crate::set_sfx_level(config::read_sfx_level(&data_dir));
        // 경험치 배율 (2026-09-23): seeded before the worker starts, because the worker
        // reads it from its first polls and installs the guest redirect on the first one
        // that finds it above 기존.
        crate::set_exp_rate(config::read_exp_rate(&data_dir));
        crate::respawn::load(&data_dir);
        crate::set_auto_loot(config::read_auto_loot(&data_dir));
        // 난이도 (2026-09-29): the same reason — the worker rescales the stage from its
        // first polls.
        crate::set_difficulty(config::read_difficulty(&data_dir));
        // 드랍률 (2026-10-16): the worker installs the filter on its first poll below 높음.
        crate::set_drop_rate(config::read_drop_rate(&data_dir));
        let render_div = if config::read_heat_saver(&data_dir) { 2 } else { 1 };
        // ★ The pad override table goes into a global (`gamepad::set_overrides`) rather
        // than a field: the HUD badges are painted by free functions in `overlay.rs`
        // that have no `App` in hand, and one store is what keeps "what the button
        // does" and "what its badge says" from ever disagreeing.
        crate::gamepad::set_overrides(config::read_gamepad_map(&data_dir));
        // 게임패드 HUD preference (59차) — applies at once, so it is loaded into the
        // global the layout reads instead of being re-read per frame.
        crate::set_pad_hud_pref(config::read_pad_hud(&data_dir));
        Self {
            input_tx,
            data_dir,
            paused,
            window: None,
            context: None,
            surface: None,
            win_w: CONTENT_W,
            win_h: CONTENT_H,
            render_div,
            layout: overlay::layout(CONTENT_W, CONTENT_H, joystick, overlay::Mode::Field, false, false),
            choice: false,
            settings_items: overlay::settings_items(CONTENT_W, CONTENT_H, 0),
            show_settings: false,
            settings_scroll: 0,
            settings_drag: None,
            editor: None,
            padmap: None,
            last_frame: vec![0xff00_0000u32; (CONTENT_W * CONTENT_H) as usize],
            // Empty + a key that no live geometry can equal (`redraw` only blits with
            // `g.w > 0`), so the first draw always builds it.
            sx_lut: Vec::new(),
            sx_lut_key: (0, 0),
            hd_frame: None,
            hud_controls: overlay::ControlsLayer::default(),
            hud_quickbar: overlay::QuickbarLayer::default(),
            touches: HashMap::new(),
            popup_confirm_touches: HashSet::new(),
            ui_touches: HashSet::new(),
            slide_touches: HashSet::new(),
            mm_tap: None,
            stats_tap: None,
            pad_held: HashMap::new(),
            pad_stick: None,
            pad_triggers: (false, false),
            pressed: HashSet::new(),
            joystick,
            joy: None,
            joy_parked: None,
            // Start in MENU mode: at boot the game is on its title screen, not in
            // the field, so the field HUD must not be up. (Device-caught: with
            // `Field` as the initial value the gauges + minimap panel were drawn
            // over the title screen, because the worker's first sample matches its
            // own initial `last_field` and so used to publish nothing.)
            mode: overlay::Mode::Menu,
            keypad_open: false,
            map_open: false,
            world_open: false,
            world_cam: overlay::WmCam { x: 0, y: 0, w: 0, h: 0 },
            stage_map: 0,
            world_stick: None,
            world_pinned: false,
            world_sel: 0,
            world_target: overlay::WmCam { x: 0, y: 0, w: 0, h: 0 },
            world_sel_at: Instant::now(),
            world_blink_at: None,
            shell: false,
            bottom_overlay: false,
            hud_y,
            quickbar: Vec::new(),
            stats: Stats::default(),
            buff_lines: Vec::new(),
            skill_cd: HashMap::new(),
            skill_cds_cfg,
            dead: None,
            started: Instant::now(),
            minimap: None,
            quick_rects: Vec::new(),
            quick_press: None,
            popup: None,
            bag_page: None,
            picking_slot: false,
            quick_page: 0,
            // Shown by default: this is how the HUD looked before the toggle existed.
            minimap_on: true,
            can_save: true,
            minimap_marker: None,
            minimap_mobs: Vec::new(),
            portrait: None,
            skill_buffs: Vec::new(),
            buff_gauges: Vec::new(),
            notices: std::collections::VecDeque::new(),
            options_patches: Vec::new(),
            revive_patches: Vec::new(),
            equip_patches: Vec::new(),
            bag_tool_patches: Vec::new(),
            pending_boot: Some(pending_boot),
            blocked_shown: false,
        }
    }

    /// The minimap panel as a TAP TARGET, or `None` when it is not on screen.
    ///
    /// ★ `overlay::minimap_panel` returns geometry unconditionally, but the panel is
    /// only built, drawn and tappable in the LANDSCAPE IMMERSIVE field HUD. Without
    /// this gate the tap region would also swallow touches in the portrait /
    /// non-immersive layouts, where that rect lands on a real key.
    fn minimap_tap_zone(&self) -> Option<overlay::Rect> {
        // ★ 2026-10-21: only while the panel is UP — a hidden panel's rect used to stay a
        //   touch-eating dead zone (a tap there "hid" a minimap that was already hidden).
        (self.mode == overlay::Mode::Field && self.minimap_ui_on() && self.minimap_on && crate::immersive_active() && self.win_w > self.win_h)
            .then(|| overlay::minimap_panel(self.win_w, self.win_h))
    }

    /// The round minimap RESTORE button as drawn, or `None` when it is not on screen — one
    /// predicate for the draw and the hit-test. Only while the panel is HIDDEN, in the
    /// panel's own corner (`overlay::minimap_restore_button`; player 2026-10-21: *"사라진
    /// 미니맵 자리에 이전에 사용하던 미니맵 on/off 아이콘이 나타나게 해서 해당 아이콘 누르면 다시
    /// 미니맵이 나타나게"*). Not gated on the immersive landscape like the panel's tap zone:
    /// the panel is DRAWN on any field layout, so a panel hidden in landscape must be
    /// recoverable after a rotation too.
    fn minimap_restore_zone(&self) -> Option<overlay::Rect> {
        (self.mode == overlay::Mode::Field && self.minimap_ui_on() && !self.minimap_on)
            .then(|| overlay::minimap_restore_button(overlay::minimap_panel(self.win_w, self.win_h), self.win_w, self.win_h))
    }

    /// Whether the minimap UI (panel + toggle) exists at all right now. One predicate
    /// for the panel, the toggle, their tap zones and the pad badge.
    ///
    /// ★ 2026-09-17 (first hero2 device run): `placement::shown` hides the minimap on
    /// builds without the hero3 map chain, but every path in here read
    /// `placement::rect_of` directly and never asked it — so the S25 showed an empty
    /// "MAP" box and a SAVE button over 영웅서기2's field. `rect_of` is geometry, not
    /// permission; this is where the permission is asked.
    fn minimap_ui_on(&self) -> bool {
        self.can_save && placement::shown(overlay::Mode::Field, placement::CtrlId::Minimap)
    }

    /// The Lv/HP gauge block as a tap target, or `None` when it is not on screen.
    /// Gated exactly like the block is DRAWN (`redraw`: field + landscape immersive)
    /// — a zone that outlived its drawing would be an invisible region that opens the
    /// menu, which is indistinguishable from a bug.
    fn stats_tap_zone(&self) -> Option<overlay::Rect> {
        (self.mode == overlay::Mode::Field && crate::immersive_active() && self.win_w > self.win_h)
            .then(|| overlay::stats_tap_box(self.layout.game))
    }

    /// SAVE NOW as a TAP TARGET, or `None` when it is not on screen.
    ///
    /// One predicate for both the draw and the hit-test, for the reason `stats_tap_zone`
    /// gives: a zone that outlives its drawing is an invisible button, and here it would
    /// be an invisible button that saves. Landscape-immersive FIELD only — the field
    /// because the guest's own gate passes nowhere else, landscape-immersive because that
    /// is the only HUD with placeable controls (every other layout builds a fixed keypad
    /// from the screen edges, and a floating disc would land on top of a real key).
    /// `can_save` too (2026-08-07): in a no-save AREA (boss room) the button is gone
    /// entirely rather than present-but-refusing.
    /// ★ NOT gated on whether the save chain is ported (2026-08-11, reverted the same
    /// day): a feature that already shipped does not get hidden because a carrier port
    /// is unfinished. Hiding it was a temporary paper-over; the fix is to finish
    /// MIGRATION_LGT.md §4-3, and until then the control stays where the player put it.
    /// ★ And gated on `placement::shown` (2026-09-17): on a build with no save chain at
    /// all (hero2) the button cannot do anything, and it was being drawn anyway.
    /// ★ 2026-10-01: the rect is the TAB as drawn (`overlay::draw_top_tab`); the touch
    /// target is bigger — `overlay::top_tab_hit` of it (player: "클릭영역은 실제버튼보다 더
    /// 커야해").
    fn quicksave_zone(&self) -> Option<overlay::Rect> {
        (self.mode == overlay::Mode::Field
            && self.can_save
            && placement::shown(overlay::Mode::Field, placement::CtrlId::QuickSave)
            && crate::immersive_active()
            && self.win_w > self.win_h)
            .then(|| placement::rect_of(placement::CtrlId::QuickSave, overlay::Mode::Field, self.win_w, self.win_h))
            .flatten()
    }

    /// The `MAP` tab (the minimap toggle) as drawn, or `None` when it is not on screen —
    /// one predicate for the draw, the hit-test and the pad badge, like [`Self::quicksave_zone`].
    ///
    /// ★ 2026-10-01 (hero3's form): a tab on the top edge beside SAVE, shown WHENEVER the
    /// minimap UI is — the panel up or not, it is the switch both ways. Gated like SAVE on
    /// the landscape-immersive field, the only HUD the minimap panel is tappable in
    /// (`minimap_tap_zone`); the touch target is `overlay::top_tab_hit` of this rect.
    fn map_tab_zone(&self) -> Option<overlay::Rect> {
        (self.mode == overlay::Mode::Field && self.minimap_ui_on() && crate::immersive_active() && self.win_w > self.win_h)
            .then(|| placement::rect_of(placement::CtrlId::MinimapToggle, overlay::Mode::Field, self.win_w, self.win_h))
            .flatten()
    }

    fn relayout(&mut self) {
        self.layout = overlay::layout(self.win_w, self.win_h, self.joystick, self.mode, self.keypad_open, self.choice);
        self.settings_scroll = self.settings_scroll.min(overlay::settings_scroll_max(self.win_w, self.win_h));
        self.settings_items = overlay::settings_items(self.win_w, self.win_h, self.settings_scroll);
        self.relayout_quick();
        // Tell the worker where the game's own top band has to go, now that we know
        // how tall our gauge block is at this window size.
        // Packed for the worker: [31:24] target-box left shift, [23:16] band height,
        // [15:8] band x, [7:0] y origin.
        let packed = if crate::immersive_active() && self.win_w > self.win_h {
            let g = self.layout.game;
            (overlay::target_box_shift(g).min(255) << 24)
                | (overlay::band_h_shift(g).min(255) << 16)
                | (overlay::band_x_shift(g).min(255) << 8)
                | overlay::hud_y_shift(g).min(255)
        } else {
            (25 << 16) | (1 << 8) // the game's own defaults
        };
        self.hud_y.store(packed, Ordering::Relaxed);
    }

    /// Recompute quickbar-button rects for the current game rect + slot count.
    fn relayout_quick(&mut self) {
        self.quick_rects = overlay::quick_buttons(self.layout.game, self.layout.gear, self.quickbar.len(), self.mode);
        // ★ The whole of the "메뉴 열면 우측 퀵바가 안 보인다" diagnosis, in one line, on the
        // UI side — where the worker's `quickbar probe` cannot see. Static reading has
        // ruled out everything it can (`MENU_IDS` holds `Quick(0..4)`; `resolve_with`
        // returns a built-in even when a stored placement is rejected, so it never drops
        // a seat; `layout_immersive` gives `game` the whole window in EVERY mode; nothing
        // in `draw_quickbar` looks at the mode; the guest frame is blitted BEFORE the
        // overlay, so its window cannot cover the plates). What is left needs values:
        //
        //   mode=Cutscene  -> a menu was misclassified as the SCRIPT window and
        //                     `quick_buttons` returns empty by design.
        //   slots=0        -> the worker emptied the list (`in_game_state` false).
        //   rects=0 w/ slots>0 and mode=Menu -> `quick_buttons`/the resolver.
        //   rects=5, kinds all Empty -> they ARE drawn, but an empty plate is "nothing
        //                     but its dim rim" at alpha 70, which is indistinguishable
        //                     from absent on a dark menu.
        //
        // Cheap: one line per relayout, and a relayout only happens on a mode change, a
        // resize or a slot-list change.
        //
        //   hidden=0x60 -> the player switched plates 6 and 7 off in the layout editor,
        //                  and `off` counts the zero-sized rects that answers with. A
        //                  report of "퀵슬롯이 안 보인다" is that line or it is a bug.
        let hidden = crate::quick_hidden();
        log::info!(
            "quickbar ui: mode={:?} immersive={} landscape={} game={}x{} slots={} rects={} hidden={:#04x} off={} kinds={:?}",
            self.mode,
            crate::immersive_active(),
            self.win_w > self.win_h,
            self.layout.game.w,
            self.layout.game.h,
            self.quickbar.len(),
            self.quick_rects.len(),
            hidden,
            self.quick_rects.iter().filter(|r| r.w == 0 || r.h == 0).count(),
            self.quickbar.iter().map(|s| s.kind).collect::<Vec<_>>(),
        );
    }

    /// Fraction of the skill cooldown ELAPSED for quickbar slot `slot` (0..1), or
    /// `None` when the slot is ready. Resolved through the slot's skill IDENTITY
    /// (sheet, cell) — see `skill_cd`. Expired entries are cleared lazily here, so
    /// nothing has to tick a timer.
    fn cooldown_frac(&mut self, slot: u8) -> Option<f64> {
        let icon = self.quickbar.iter().find(|s| s.slot == slot).and_then(|s| s.icon)?;
        let crate::IconRef::Skill(sheet, cell) = icon else { return None };
        let (t0, dur) = *self.skill_cd.get(&(sheet, cell))?;
        let ms = t0.elapsed().as_millis();
        if dur == 0 || ms >= u128::from(dur) {
            self.skill_cd.remove(&(sheet, cell));
            return None;
        }
        Some(ms as f64 / dur as f64)
    }

    /// Is the gauge block (LV plate + HP/SP/EXP + badges) shown right now? The field
    /// always shows it; since 2026-08-05 a game WINDOW shows it too (player request:
    /// "메뉴 버튼 열렸을때 좌측상단 UI는 다 보이게") — but only in the immersive
    /// landscape HUD, only once a live character has been seen (`level > 0` filters
    /// the boot default), and never over the title/slot shell.
    fn stats_hud_on(&self) -> bool {
        match self.mode {
            overlay::Mode::Field => true,
            overlay::Mode::Menu => {
                // ★ 2026-09-09: on a build whose OWN gauges the host paints over
                // (`guest_hud_band` — hero2), the orientation condition is dropped. It
                // existed because hero3 keeps its own HP/SP strip on screen, so our
                // block was a portrait-mode duplicate of information the player already
                // had. Where the strip has been masked away there is no duplicate and no
                // fallback: hiding the block in portrait would leave NO readout at all.
                let replaced = crate::guest_build::guest_hud_band().is_some();
                !self.shell && self.stats.level > 0 && (replaced || (crate::immersive_active() && self.win_w > self.win_h))
            }
            overlay::Mode::Cutscene => false,
        }
    }

    /// Which rows of the guest framebuffer to actually show: `(first row, row count)`.
    ///
    /// ★★ 2026-09-10. hero2 is handed a framebuffer `hud_reserved_rows` TALLER than the
    /// picture we mean to present, because the game sizes its world viewport from what it
    /// is given and keeps the remainder for its own bottom HUD. Showing only the top
    /// `logical().1` rows therefore puts the WORLD where that HUD would have been - the
    /// same scale as before, more world in view, and the black strip the mask used to
    /// leave is simply not part of the picture. See
    /// [`crate::guest_build::GuestBuild::hud_reserved_rows`] for the measurement.
    ///
    /// ⛔ EXCEPT on the shell. The title, the slot list and the publisher splash are
    /// BOTTOM-ANCHORED - the slot screen's lower decorative band and the splash's version
    /// line live in the last rows - so cropping there would cut them off. Those screens
    /// get the whole framebuffer, which costs a ~10% vertical squeeze on static art and
    /// loses nothing.
    ///
    /// ⛔ AND except under a bottom-anchored overlay (2026-09-17, first device run). The
    /// NPC dialogue box is drawn into the last ~38 rows of the framebuffer, i.e. mostly
    /// into the reserved band, and the crop showed its top 13 rows with the text sliced
    /// through. There the picture SCROLLS instead: same row count, same scale, starting
    /// `hud_reserved_rows` further down, so the box is whole at the bottom and the world
    /// loses its top rows for the length of the conversation. Not the shell's whole-frame
    /// squeeze - sprites in motion are what a squeeze is most visible on.
    ///
    /// ★ 2026-09-18: `hud_reserved_rows` is **0 on every build** since the viewport patch
    /// (`guest_build::viewport_full_patch` — the game sizes its world to the whole
    /// framebuffer), so both exceptions above are dormant and this always answers
    /// `(0, present_h)`. They stay for a build that reserves rows again; the two lessons
    /// (state 4 outlives the box; stale ink reads as a box) are in CLAUDE.md.
    fn game_source_window(&self) -> (u32, u32) {
        let (_, present_h) = crate::logical();
        let reserved = crate::guest_build::hud_reserved_rows();
        if self.shell {
            (0, present_h + reserved)
        } else if self.bottom_overlay {
            (reserved, present_h)
        } else {
            (0, present_h)
        }
    }

    /// Should the game's own HUD band be painted over this frame?
    ///
    /// `false` on every build that has no measured band (hero3 — see
    /// `guest_anchors::GuestBuild::guest_hud_band`). On hero2 it is `true` for the whole
    /// of "a character is in the world", **including with a game window open**: the band
    /// is a reserved strip BELOW the world and the game's own panels never reach it, so
    /// standing down off-field would just let the strip we removed flicker back. The
    /// shell and the boot screens are excluded — they put their own art in those rows
    /// (the publisher mark under the title, the slot list's frame).
    fn guest_hud_mask_on(&self) -> bool {
        // ★ 2026-09-10: on a build whose reservation we simply DO NOT PRESENT
        // (`game_source_window`), there is nothing left to paint over - the band is off the
        // bottom of the picture. Masking would now black out live world rows. The flag
        // stays for a build that has a measured band but no reservation to drop.
        // ★ 2026-09-18: and nothing to paint over on a build whose painter is switched
        // OFF (`hud_bottom_patch`) — the band rows hold world now.
        crate::guest_build::hud_reserved_rows() == 0
            && crate::guest_build::hud_bottom_patch().is_none()
            && crate::guest_build::guest_hud_band().is_some()
            && !self.shell
            && self.stats.level > 0
    }

    /// Window pixel -> guest FRAMEBUFFER pixel, or `None` outside the game rect. The shown
    /// rows start at `game_source_window().0`, so that offset is added back: the result
    /// is a framebuffer `y`, which is what the tap router's geometry is in.
    fn fb_point(&self, x: f64, y: f64) -> Option<(u32, u32)> {
        let g = self.layout.game;
        if g.w == 0 || g.h == 0 || x < f64::from(g.x) || y < f64::from(g.y) {
            return None;
        }
        let (lw, _) = crate::guest_fb();
        let (y0, lh) = self.game_source_window();
        let fx = ((x - f64::from(g.x)) * f64::from(lw) / f64::from(g.w)) as u32;
        let fy = ((y - f64::from(g.y)) * f64::from(lh) / f64::from(g.h)) as u32;
        (fx < lw && fy < lh).then_some((fx, y0 + fy))
    }

    /// ★ 부활의 구 (2026-09-25): a touch inside the box, as the framebuffer point the worker
    /// hit-tests — `None` when no box is up or the touch is outside it (the controls around
    /// it keep their meaning: OK and the stick answer the box as keys).
    fn revive_prompt_hit(&self, x: f64, y: f64) -> Option<(u32, u32)> {
        let (fx, fy) = self.fb_point(x, y)?;
        let inside = self.revive_patches.iter().any(|p| {
            let (px, py) = (i64::from(p.x), i64::from(p.y));
            let (fx, fy) = (i64::from(fx), i64::from(fy));
            fx >= px && fy >= py && fx < px + i64::from(p.w) && fy < py + i64::from(p.h)
        });
        inside.then_some((fx, fy))
    }

    /// ★ #13: the inventory page tabs in window px, while the strip is up (MENU mode with
    /// the game's inventory window in the chain). Empty otherwise.
    fn bag_tab_rects(&self) -> Vec<overlay::Rect> {
        let Some(v) = self.bag_page else { return Vec::new() };
        if self.mode != overlay::Mode::Menu || self.shell {
            return Vec::new();
        }
        let (lw, _) = crate::guest_fb();
        let (y0, lh) = self.game_source_window();
        overlay::bag_tab_rects(self.layout.game, lw, lh, y0, v.panel, v.pages, v.shift, v.cell_y0)
    }

    /// ★ 2026-10-02 (user: *"마지막 5번쨰가방에서 칸 몇개를 지워야해"*): the cells the page does not have (the
    /// fifth has 15 of the 28 the game paints — `hero2java::h2_page_hidden_cells`), as the window px that show
    /// them. Up exactly when the strip is (same mode test), empty on a full page.
    fn bag_hidden_cell_rects(&self) -> Vec<overlay::Rect> {
        let Some(v) = self.bag_page else { return Vec::new() };
        if self.mode != overlay::Mode::Menu || self.shell {
            return Vec::new();
        }
        let (lw, _) = crate::guest_fb();
        let (y0, lh) = self.game_source_window();
        crate::guest_build::h2_page_hidden_cells(v.panel, v.cells, v.shift)
            .into_iter()
            .filter_map(|fb| overlay::fb_rect_to_win(self.layout.game, lw, lh, y0, fb))
            .collect()
    }

    /// ★ 2026-09-29: the game's box over the window, in the window px that show it — the
    /// strip is drawn under it (see `hero2java::h2_modal_over`). `None` while nothing is
    /// over the window, or while the strip is not up at all.
    fn bag_tab_cover(&self) -> Option<overlay::Rect> {
        let cover = self.bag_page?.cover?;
        let (lw, _) = crate::guest_fb();
        let (y0, lh) = self.game_source_window();
        overlay::fb_rect_to_win(self.layout.game, lw, lh, y0, cover)
    }

    /// Rebuild `pressed` (highlight set) from the held touch keys + joystick dir.
    fn refresh_pressed(&mut self) {
        let mut p: HashSet<KeyCode> = self.touches.values().copied().collect();
        if let Some(k) = self.joy.as_ref().and_then(|j| j.dir) {
            p.insert(k);
        }
        self.pressed = p;
    }

    /// Update the joystick from a new thumb position: resolve the single walk
    /// direction against the FIXED centre this drag spawned at, and emit keyup(old)
    /// before keydown(new) so the guest re-latches on every turn. `active` = finger
    /// down.
    ///
    /// ★ `joy.cx/cy` is written ONCE, at touch-down, and never again for the life of
    /// the touch (player request 2026-08-03 — see the note on `overlay::joy_dir` for
    /// what the trailing base used to do and why it went).
    fn joy_update(&mut self, x: f64, y: f64, active: bool) {
        // ★ The ENGAGE radius, not the pin radius (hero3 91차-B): the base disc is the
        // arrows' square now and the thumb pins inside it (`joy_radius`, used by the
        // renderer's clamp), but the distance a thumb travels before walking starts is
        // still the pre-91차 one — see `overlay::joy_engage_radius`.
        let radius = self
            .layout
            .joystick
            .as_ref()
            .map(|z| overlay::joy_engage_radius(z, self.win_w, self.win_h))
            .unwrap_or(1.0);
        // ★★ THE CENTRE DOES NOT MOVE (2026-09-20, user: *"한번 손을 데면 그자리에서
        // 고정된채로 작동해야하고 손을뗐다가 다시 대는 위치가 조이스틱의 중앙이 되서
        // 다시 고정된채로 작동해야하는데 지금은 조이스틱이 내가 이동한방향으로 같이
        // 움직이고있어 … hero3처럼 되어야해"*). A floating stick, hero3's: touch down
        // fixes the centre at the landing point (`self.joy = Some(Joy { cx: x, cy: y … })`
        // on press), it stays there for the whole gesture, and lifting and touching again
        // is how you get a new one. Nothing here writes `joy.cx`/`joy.cy`.
        //
        // ⛔ Do not reintroduce a following centre. Three attempts are on record and each
        // one was reported as a defect by the player:
        //   * 2026-08-03, trailing at the ENGAGE radius (27~96 px) — the base ran across
        //     the screen under an ordinary steer (*"조이스틱 자체가 따라와서 이동이 좀
        //     불편해"*). Reverted.
        //   * 2026-09-18, trailing at the PIN radius (0.56 R ≈ 175 px) — that is exactly
        //     where the thumb disc rests against the rim, so full tilt (the intended
        //     gesture) slid the base; modelled on a 300 px circling thumb the direction
        //     stopped matching the thumb **50 %** of the time and the centre drifted
        //     **235 px** (*"점점점점 … 이상한 방향으로 이동을 한다"*).
        //   * 2026-09-19, trailing at the BASE DISC radius (312 px) — the model said 7 %
        //     and 0 px drift, which is why it looked settled, but on the device the base
        //     still visibly walks with a long drag. That is this report.
        // What every version was buying is cheaper REVERSALS: with a fixed centre a thumb
        // 400 px below has to come 400 px back before `joy_dir` sees an upward vector.
        // The player's answer to that is the lift — which is one tap, is instant, and is
        // the gesture they already expect from hero3. So the trade is settled: pay the
        // reversal, keep the centre where the finger put it.
        let Some(joy) = self.joy.as_mut() else { return };
        joy.tx = x;
        joy.ty = y;
        // ★★ THE HYSTERESIS ANCHOR IS `want`, NOT `dir` (2026-09-20, review). `joy_dir`
        // keeps its `active` argument unless a challenger beats it by `TURN_RATIO`, and
        // `dir` is what the GUEST holds — which lags `want` for the whole `TURN_SETTLE`
        // window. Anchored on the stale direction there is only ONE threshold instead of
        // two, so a thumb parked near a diagonal flips `want` back and forth; every flip
        // restamps `want_since`, `elapsed()` never reaches `TURN_SETTLE`, and the turn
        // never commits — the "it will not turn" symptom the settle was added to cure,
        // arrived at from the other side. Anchored on `want`, the pending direction has
        // to be beaten to be replaced, which is what two-sided hysteresis means.
        let want = if active {
            overlay::joy_dir(x - joy.cx, y - joy.cy, radius, joy.want)
        } else {
            None
        };
        if want != joy.want {
            joy.want = want;
            joy.want_since = Instant::now();
        }
        self.settle_joy();
    }

    /// Hand `joy.want` to the guest once it has earned it, and keep `pressed` in step.
    ///
    /// ★★ WHY A TURN HAS TO LAST (2026-09-19, player: *"조이스틱을 한바퀴 돌리고
    /// 손을뗴면 마지막 방향으로 갑자기 쭉 이동을 한다"*). hero2 does not walk "while
    /// the key is down": ONE keydown commits a fixed run of movement that the matching
    /// keyup cannot cancel. Measured headless (`wie_probe`, on the field, the map scroll
    /// used as an odometer, DOWN pressed at f=1200, guest pacing 50 ms):
    ///
    /// | key held for | walked | movement ran until |
    /// |---|---|---|
    /// | 20 ms (one probe frame) | 44 px | t = 120 ms — **100 ms past the keyup** |
    /// | 60 ms | 44 px | t = 120 ms |
    /// | 800 ms | 60 px (into a wall) | t = 180 ms |
    ///
    /// So every keydown the host sends buys ~44 px in that direction whatever the finger
    /// does next. `joy_update` used to send one on every turn, and a thumb sweeping round
    /// turns every ~60 ms — twice what the guest can retire — so the commitments queue
    /// and the character plays the gesture back: it walks stale directions while the
    /// thumb is already somewhere else, and when the finger lifts it still owes the last
    /// one.
    ///
    /// The rule: a CHANGE from one live direction to another must have been asked for
    /// continuously for [`TURN_SETTLE`] before it is sent. Starting to walk and stopping
    /// stay instant — those are what the player feels as latency (2026-09-18's *"0.1초
    /// 뒤에 이동이 되는듯한 느낌"*) — and a turn waits less than the guest frame it
    /// would have had to wait for anyway.
    fn settle_joy(&mut self) {
        let Some(joy) = self.joy.as_mut() else { return };
        let want = joy.want;
        if want != joy.dir {
            // Instant: the first engage (nothing is held, so nothing can queue) and every
            // release (whatever IS held must come up). Only a turn between two live
            // directions waits.
            let instant = joy.dir.is_none() || want.is_none();
            if instant || joy.want_since.elapsed() >= TURN_SETTLE {
                // A turn is ONE event (`InputEvent::Turn`): the worker forwards only the
                // new press on the field, so the guest never sees a release it could
                // mistake for "stop". Engage and release stay plain key edges.
                match (joy.dir, want) {
                    (Some(old), Some(new)) => {
                        let _ = self.input_tx.send(InputEvent::Turn { from: old, to: new });
                    }
                    (Some(old), None) => {
                        let _ = self.input_tx.send(InputEvent::Key { code: old, down: false });
                    }
                    (None, Some(new)) => {
                        let _ = self.input_tx.send(InputEvent::Key { code: new, down: true });
                    }
                    (None, None) => {}
                }
                joy.dir = want;
                // Only here: `pressed` is a function of `dir`, and this is the one place
                // it moves. `redraw` polls this every frame while a finger is down, so an
                // unconditional rebuild would mint a HashSet per present for nothing.
                self.refresh_pressed();
            } else {
                // Still ripening, and no touch event need ever arrive to ask again (a
                // thumb held still at its new angle emits nothing), so the redraw poll
                // has to come back for it.
                self.request_redraw();
            }
        }
    }

    /// Adopt touch `id` as the walk stick and resolve its first direction. TOUCH
    /// TYPE A only — under B the layout publishes no joystick zone (the field walks
    /// with the D-pad arrows), so nothing can ever call this. The finger's own
    /// landing point becomes the centre, so the first press is always neutral.
    fn joy_start(&mut self, id: u64, x: f64, y: f64) {
        self.joy_parked = None;
        // ★ fixed_A (2026-09-24): the base does not come to the finger — it stays on its
        // home seat and the finger's offset from THAT centre is the direction.
        let (cx, cy) = if crate::stick_pinned() {
            let (hx, hy, _) = overlay::joy_home(self.win_w, self.win_h);
            (hx, hy)
        } else {
            (x, y)
        };
        self.joy = Some(Joy { id, cx, cy, tx: x, ty: y, dir: None, want: None, want_since: Instant::now() });
        // Harmless (dx = dy = 0, so `joy_dir` answers `None`).
        self.joy_update(x, y, true);
        self.request_redraw();
    }

    /// Let go of the stick because its zone went away (the mode left the field), while
    /// the finger is still down: release the held direction, but REMEMBER the touch so
    /// the next move from it can re-form the stick. See [`Self::joy_parked`].
    fn joy_park(&mut self) {
        // ★ 2026-09-24: a map change is THREE mode flips, not one — field → script (the
        // exit's fade, state 4) → loading (state 1) → field — and each one that leaves the
        // stick zone empty lands here. The second call found `self.joy` already `None`,
        // took `id = None`, and `joy_end` cleared `joy_parked` on the way: the finger the
        // first flip had parked was forgotten, so when the field came back the thumb
        // still on the glass steered nothing until the player lifted and pressed again
        // (measured headless: state 3 → 4 → 1 → 3 over ~0.5 s on the Sindraf exit). A
        // finger already parked stays parked.
        let id = self.joy.as_ref().map(|j| j.id).or(self.joy_parked);
        self.joy_end();
        self.joy_parked = id;
    }

    /// The touch type changed (the gear row, or the game's own 환경설정 via the worker):
    /// anything the old type was holding is released first, exactly like
    /// `set_control_style` — the zone changes under the finger, so a held direction
    /// would have no touch left that could ever end it — and the layout is rebuilt.
    fn touch_type_changed(&mut self) {
        self.joy_end();
        self.relayout();
        self.request_redraw();
    }

    /// Release the joystick entirely (held direction up).
    fn joy_end(&mut self) {
        self.joy_parked = None;
        if let Some(joy) = self.joy.take()
            && let Some(k) = joy.dir
        {
            let _ = self.input_tx.send(InputEvent::Key { code: k, down: false });
        }
        self.refresh_pressed();
    }

    fn configure_surface(&mut self) {
        let (Some(context), Some(window)) = (self.context.as_ref(), self.window.as_ref()) else {
            return;
        };
        if self.surface.is_none() {
            match Surface::new(context, window.clone()) {
                Ok(s) => self.surface = Some(s),
                Err(e) => {
                    log::error!("Surface::new failed: {e}");
                    return;
                }
            }
        }
        // RENDER size, not the physical window (57차): with the HEAT SAVER divisor
        // this asks `ANativeWindow_setBuffersGeometry` for a half-size buffer and
        // the compositor upscales it to the window in hardware.
        let (Some(w), Some(h)) = (NonZeroU32::new(self.win_w), NonZeroU32::new(self.win_h)) else {
            return;
        };
        if let Some(surface) = self.surface.as_mut()
            && let Err(e) = surface.resize(w, h)
        {
            log::error!("surface resize failed: {e}");
        }
    }

    /// Request a coalesced redraw instead of drawing inline. winit merges any
    /// number of requests between frames into a single `RedrawRequested`, so the
    /// joystick's high-rate touch-move stream (which on a 120/240 Hz-sampling
    /// panel would otherwise trigger one full-screen blit + `present()` per
    /// event — the cause of the drag lag) collapses to at most one draw per frame.
    fn request_redraw(&self) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    fn redraw(&mut self) {
        // A turn whose dwell expired while the thumb sat still (see `settle_joy`).
        self.settle_joy();
        let (win_w, win_h) = (self.win_w, self.win_h);
        let landscape = win_w > win_h;
        // ★ Everything below is resolved BEFORE the surface is borrowed: `buffer_mut`
        // holds a mutable borrow of `self` for the whole of the draw, so neither a
        // `&self` method (`quicksave_zone`, `stats_hud_on`) nor a `&mut self` one
        // (`cooldown_frac`, which expires entries lazily) borrow-checks inside it.
        // Per-plate cooldown progress; 1.0 = ready.
        // ★ The GUEST's own counter wins when the slot carries one (hero2: `ag.a[slot]`
        // ticks left / class total) — the host timer is hero3's stand-in for a game that
        // keeps no such counter.
        let bag_tabs = self.bag_tab_rects();
        let bag_cover = self.bag_tab_cover();
        let bag_hidden = self.bag_hidden_cell_rects();
        let cooldowns: Vec<f64> = (0..self.quick_rects.len())
            .map(|i| {
                let slot = self.quickbar.get(i).copied();
                match slot {
                    Some(s) => match s.cooldown {
                        Some((left, total)) if total > 0 => 1.0 - (f64::from(left) / f64::from(total)).min(1.0),
                        _ => self.cooldown_frac(s.slot).unwrap_or(1.0),
                    },
                    None => 1.0,
                }
            })
            .collect();
        let stats_on = self.stats_hud_on();
        // Hoisted for the same borrow reason as `stats_on` above — the mask is issued
        // inside the `buf` scope, which holds `self` mutably.
        let mask_guest_hud = self.guest_hud_mask_on();
        let quicksave = self.quicksave_zone();
        let map_tab = self.map_tab_zone();
        let minimap_ui = self.minimap_ui_on();
        let minimap_restore = self.minimap_restore_zone();
        // 설정 → 패드 버튼 설정 (2026-10-09) — the rows, and the prompt's expiry, resolved here
        // for the same `buffer_mut` reason.
        let padmap = self.padmap_view();

        // ── game blit setup, while `self` is still freely usable ────────────────────
        let g = self.layout.game;
        let (lw, fb_h) = crate::guest_fb();
        let (y0, lh) = self.game_source_window();
        // Everything the blit's index arithmetic needs, established HERE rather than
        // inherited from `overlay::layout`:
        //
        //   * `last_frame.len() == lw * fb_h` and `sy <= y0 + lh - 1 <= fb_h - 1`
        //       ->  `sy * lw + lw <= len`;
        //   * `g` lies wholly inside the window and `dy <= g.h - 1`
        //       ->  `(g.y + dy) * win_w + g.x + g.w <= (win_h - 1) * win_w + win_w`
        //         = `win_w * win_h` = `buf.len()` (checked just below).
        //
        // The two containment tests and the `lw > 0` test are NEW. They only ADD to the
        // old guard (`len` match + non-empty rect) — nothing that used to blit stops
        // blitting, because every `overlay::layout` branch already satisfies them: the
        // immersive branch hands `game` the whole window, and the other two `contain()` a
        // sub-rect of an on-screen area. They are here so the no-panic argument is local
        // to this function instead of living in another module that is free to change.
        // ⚠ The buffer is `fb_h` rows; `lh` is how many of them we SHOW, from row `y0`. On
        // every build that reserves nothing `y0 == 0`, `lh == fb_h` and this is the old
        // test verbatim.
        // Set by the notice pass: its stack is still sliding or has a notice to expire,
        // so this frame asks for the next one (nothing else would).
        let mut notices_animating = false;
        let blit = self.last_frame.len() == (lw * fb_h) as usize
            && lw > 0
            && lh > 0
            && y0.saturating_add(lh) <= fb_h
            && g.w > 0
            && g.h > 0
            && g.x.saturating_add(g.w) <= win_w
            && g.y.saturating_add(g.h) <= win_h;
        // ★ Refreshed before the surface is borrowed, for the reason the note above gives:
        // `buffer_mut` pins `self`, so this `&mut self` cache update would not borrow-check
        // once the draw has started. The length is part of the test as belt-and-braces —
        // with it, `zip` in the gather loop cannot even in principle read past the map.
        if blit && (self.sx_lut_key != (g.w, lw) || self.sx_lut.len() != g.w as usize) {
            self.sx_lut = sx_lut(g.w, lw);
            self.sx_lut_key = (g.w, lw);
        }

        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let mut buf = match surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                log::warn!("buffer_mut failed: {e}");
                return;
            }
        };
        if buf.len() != (win_w * win_h) as usize {
            return;
        }

        // ★ Clear only the pixels the game blit will NOT overwrite (letterbox + control
        // margins). The full-window `buf.fill(0xff00_0000)` that stood here wrote every
        // pixel of the window — 10 MB of stores per frame at 1080x2340 — and the blit then
        // immediately overwrote the whole `layout.game` rect. In the immersive HUD that
        // rect IS the window, so the fill was pure waste on the layout the game is
        // actually played in.
        //
        // The fallback is exact, not approximate: when `blit` is false NOTHING covers the
        // game rect this frame, so the old full-window fill has to stay or the rect would
        // show whatever the previous frame left in the recycled surface buffer.
        if blit {
            let (w, gx, gy, gw, gh) = (win_w as usize, g.x as usize, g.y as usize, g.w as usize, g.h as usize);
            // `gy * w` and `(gy + gh) * w` are both `<= win_h * w == buf.len()` because
            // `g.y + g.h <= win_h`, so neither slice can be out of range.
            buf[..gy * w].fill(0xff00_0000);
            // Skipped when the game spans the full width (the immersive case), where this
            // would be `win_h` iterations of two empty fills for nothing.
            if gx > 0 || gx + gw < w {
                for row in gy..gy + gh {
                    let base = row * w;
                    buf[base..base + gx].fill(0xff00_0000);
                    buf[base + gx + gw..base + w].fill(0xff00_0000);
                }
            }
            buf[(gy + gh) * w..].fill(0xff00_0000);
        } else {
            buf.fill(0xff00_0000);
        }

        // Blit the game into its layout rect (nearest-neighbour, aspect-correct).
        if blit {
            let (lut, last) = (&self.sx_lut, &self.last_frame);
            let gw = g.w as usize;
            // ★ Vertical runs are COPIED, not re-gathered. The guest framebuffer is ~240
            // logical rows and the device window is 2340 device rows, so about 90 % of
            // destination rows resolve to the same source row as the row above them — the
            // old loop ran the full per-pixel gather ~9 times over each source row to
            // produce nine identical destination rows.
            let mut prev: Option<(u32, usize)> = None;
            for dy in 0..g.h {
                // `sy < y0 + lh <= fb_h`: `dy <= g.h - 1`, so `dy * lh < g.h * lh`.
                let sy = y0 + (dy * lh) / g.h;
                let dst_row = ((g.y + dy) * win_w + g.x) as usize;
                match prev {
                    // Same source row as the last row we gathered: memcpy that finished
                    // destination row instead of re-reading the source. `from` is a row
                    // STRICTLY ABOVE `dst_row` (both start at `g.x` and `dy` only grows),
                    // and `from + gw <= dst_row` because `g.x + g.w <= win_w` — so the two
                    // ranges are disjoint and this is a plain memcpy, not a memmove.
                    Some((psy, from)) if psy == sy => buf.copy_within(from..from + gw, dst_row),
                    _ => {
                        let src_row = (sy * lw) as usize;
                        let src = &last[src_row..src_row + lw as usize];
                        // `zip` (not `for dx in 0..gw`) is what makes this unable to panic
                        // on a stale map: a wrong-length LUT would draw fewer columns
                        // rather than index out of range. `src[sx]` is in range because
                        // every LUT entry is `< src_w`, and `src_w` is `lw` by the key.
                        for (d, &sx) in buf[dst_row..dst_row + gw].iter_mut().zip(lut.iter()) {
                            *d = src[sx as usize];
                        }
                        prev = Some((sy, dst_row));
                    }
                }
            }
            // ★★ 2026-10-15 「악마화」 HD: the effects this frame recorded, composited at the window's resolution over the
            //   blit — the library's 3× twins where it has them, in the game's paint order (the record's layer map).
            if let Some(hd) = self.hd_frame.as_deref()
                && let Some(lib) = wie_backend::canvas::fx_library()
            {
                let view = wie_backend::canvas::HdView { rect: (g.x as i32, g.y as i32, g.w, g.h), src_w: lw, src_y0: y0, src_h: lh };
                wie_backend::canvas::compose_hd(&lib, hd, &mut buf, win_w, win_h, &view);
            }
        }

        // Joystick view (thumb clamped to the base radius for rendering).
        let joy_view = self.joy.as_ref().and_then(|j| {
            let zone = self.layout.joystick.as_ref()?;
            let r = overlay::joy_radius(zone, win_w, win_h);
            let (dx, dy) = (j.tx - j.cx, j.ty - j.cy);
            let dist = (dx * dx + dy * dy).sqrt();
            let thumb = if dist > r && dist > 0.0 {
                (j.cx + dx / dist * r, j.cy + dy / dist * r)
            } else {
                (j.tx, j.ty)
            };
            Some(overlay::JoyView { center: (j.cx, j.cy), thumb })
        });
        // FIELD only: cover the game's own field HUD (bottom strip + top-left skill
        // box) so our custom HUD replaces it, then draw our HUD on top. When one of
        // the game's own windows is open, all of that must stand down — the mask
        // would black out part of the window, and the gauges/minimap panel would
        // cover it (that, plus a zeroed Stats, is the reported "HP/MP read 0 in the
        // menu" bug). The buttons alone stay, now as menu-navigation keys.
        let field = self.mode == overlay::Mode::Field;
        if field || mask_guest_hud {
            overlay::mask_game_hud(&mut buf, win_w, win_h, self.layout.game);
        }
        // Title / slot screens are 240-px art that the game mirrors outwards to fill
        // a wide framebuffer; letterbox it so it reads as centred, not cropped.
        if self.shell {
            overlay::letterbox_shell(&mut buf, win_w, win_h, self.layout.game);
        }
        // ★ 2026-09-24: the host's rows of the game's 환경설정, in the game's own pixels —
        // part of the game's picture, so before any of our chrome.
        if !self.options_patches.is_empty() {
            overlay::draw_guest_patches(&mut buf, win_w, win_h, self.layout.game, (lw, fb_h, y0, lh), &self.options_patches);
        }
        // ★ 의상 아바타 A3: the EQUIPMENT window's avatar side — the game's window, redrawn in part.
        if !self.equip_patches.is_empty() {
            overlay::draw_guest_patches(&mut buf, win_w, win_h, self.layout.game, (lw, fb_h, y0, lh), &self.equip_patches);
        }
        // ★★ 2026-10-16: the 정렬 / 일괄판매 buttons — on the game's windows, in their pixels.
        if !self.bag_tool_patches.is_empty() && self.mode != overlay::Mode::Field {
            overlay::draw_guest_patches(&mut buf, win_w, win_h, self.layout.game, (lw, fb_h, y0, lh), &self.bag_tool_patches);
        }
        // ★ 2026-09-25: the 부활의 구 box — part of the game's picture too (the game's own
        //   confirm box, redrawn over the frozen frame of the fall).
        if !self.revive_patches.is_empty() {
            overlay::draw_guest_patches(&mut buf, win_w, win_h, self.layout.game, (lw, fb_h, y0, lh), &self.revive_patches);
        }
        // ★ The centred full-screen map copy is GONE (player request 2026-08-04:
        // "가운데 확대해서 표시하는 기능은 제거하고"). The minimap is now a panel you
        // travel with, shown or hidden by tapping it — blowing it up over the middle of
        // the screen was the old answer to "the guest draws its map window tiny", and
        // the toggle replaced the need for it.
        //
        // `map_open` is still tracked — it is part of what selects the overlay control
        // set — but nothing draws over the guest's own map window any more.
        // ★ 설정 → 패드 버튼 설정 (2026-10-09): modal over the game picture, like the editor —
        // the FIELD HUD as it is played, every pad function lit on it (`overlay::draw_padmap`).
        if let Some(v) = padmap.as_ref() {
            let look = overlay::PadMapLook {
                badges: &v.badges,
                sel: v.sel,
                probe: v.probe,
                pulse: v.pulse,
                title: Self::PADMAP_TITLE,
                lines: [&v.lines[0], &v.lines[1]],
                can_reset: v.can_reset,
                dirty: v.dirty,
            };
            overlay::draw_padmap(&mut buf, win_w, win_h, &v.scene, &v.slots, &look);
            if let Some(lines) = self.dead.as_deref() {
                overlay::draw_fatal_notice(&mut buf, win_w, win_h, self.layout.game, lines);
            }
            if let Err(e) = buf.present() {
                log::warn!("present failed: {e}");
            }
            // The countdown and the breath are drawn from the clock, so a prompt is owed
            // frames until it is answered or runs out — the game's own frames may be paused
            // under us.
            if v.pulse.is_some() {
                self.request_redraw();
            }
            return;
        }
        // ★ The editor replaces the whole control layer: it shows the set for the
        // mode BEING EDITED, which is usually not the mode the game is in.
        if let Some(ed) = self.editor.as_ref() {
            let items = overlay::edit_items(win_w, win_h);
            overlay::draw_layout_editor(&mut buf, win_w, win_h, ed.sel, &items, ed.pad.then_some(&self.quickbar[..]));
            // ★★ The notices, HERE too (hero3 82차). This arm RETURNS, so the shared notice
            // pass below is unreachable from the editor — and HIDE / SHOW raise notices that
            // are the whole answer to a tap. Without this, refusing looks exactly like a
            // broken button.
            if notices_pass(&mut self.notices, self.layout.game, self.layout.joystick.is_some(), &mut buf, win_w, win_h, (lw, fb_h, y0, lh), false) {
                notices_animating = true;
            }
            if let Some(lines) = self.dead.as_deref() {
                overlay::draw_fatal_notice(&mut buf, win_w, win_h, self.layout.game, lines);
            }
            if let Err(e) = buf.present() {
                log::warn!("present failed: {e}");
            }
            if notices_animating {
                self.request_redraw();
            }
            return;
        }
        // ★ 2026-09-24: the pickup notices, under the controls (a control must never be
        // hidden by a message) and clear of the walk stick's seat.
        // ★★ 2026-10-20: in every mode, not just the field — the host's own lines (저장완료 · the
        // settings rows' answers · the vault's boot notice on the title) are notices now.
        // ⛔ Not while the settings modal is up: its dim pass and opaque panel would cover them
        // (the toast this replaces was drawn after the modal for the same reason — the
        // 발열 절감 row's "재시작 후 적용" is raised BY that modal). They are drawn after it below.
        if !self.show_settings && notices_pass(&mut self.notices, self.layout.game, self.layout.joystick.is_some(), &mut buf, win_w, win_h, (lw, fb_h, y0, lh), field) {
            notices_animating = true;
        }
        // ★ 2026-09-29: the EXP bar at the bottom centre (hero5's place for it, player
        // choice) — UNDER the controls, because unlike the block at the top-left it is not
        // an obstacle to them. Same gate as the gauges.
        if stats_on {
            overlay::draw_exp_bar(&mut buf, win_w, win_h, self.layout.game, &self.stats);
        }
        self.hud_controls
            .draw(&mut buf, win_w, win_h, &self.layout, &self.pressed, joy_view, self.keypad_open);
        // The status block (portrait ring · HP/SP · level) first, skill plates ON TOP.
        // (The EXP bar is drawn above, under the controls.)
        //
        // ★ The gauges are drawn in MENU mode too since 2026-08-05 (player request:
        // "메뉴 버튼 열렸을때 좌측상단 UI는 다 보이게" — LV, HP, MP and EXP all stay
        // visible with a game window open). `stats_hud_on` gates out the shell and
        // the boot default. The live cash-buff readout rides below them, MENU only:
        // on the field those pixels belong to the game world.
        if stats_on {
            let blink = overlay::badge_blink_on(self.started.elapsed().as_millis());
            overlay::draw_stats(&mut buf, win_w, win_h, self.layout.game, &self.stats, self.portrait.as_ref(), &self.skill_buffs, &self.buff_gauges, blink);
            if self.mode == overlay::Mode::Menu {
                overlay::draw_buff_lines(&mut buf, win_w, win_h, self.layout.game, &self.buff_lines);
            }
        }
        // ★ #13: the inventory page strip, beside the game's own inventory panel.
        //
        // ⛔ OUTSIDE `if stats_on` (2026-09-21, review). It was nested inside it by a
        // stray brace, while `bag_tab_rects` and the touch arm ask only about the mode —
        // so whenever `stats_hud_on()` was false the five tabs were live and invisible.
        // That is not a corner: it needs `stats.level > 0`, which is 0 until the worker's
        // first good `read_stats` and again for any stretch where `resolve_char` does not
        // resolve (the probe logs `CHAR unresolved` as an ordinary transient). A tap in
        // the margin then paged the open inventory with its cursor reset to cell 0, with
        // nothing on screen to say a control was there — and a 버리기 afterwards discards
        // a record up to 112 slots from the one the player was looking at.
        if let Some(v) = self.bag_page
            && !bag_tabs.is_empty()
        {
            overlay::draw_bag_tabs(&mut buf, win_w, win_h, &bag_tabs, v.page, v.on_top, bag_cover);
            // ★ 2026-10-02: the cells the short fifth page does not have — painted as the panel, under the
            //   game's box like the strip.
            overlay::draw_bag_hidden_cells(&mut buf, win_w, win_h, &bag_hidden, bag_cover);
        }
        // ★ The plates are drawn in MENU mode too since 2026-08-03 (the minimap is
        // not — it belongs to the field HUD the game replaces with its window).
        // `quick_rects` is empty during a cutscene and off-field in the
        // non-immersive layouts, so this is one call for every mode that has them.
        self.hud_quickbar.draw(
            &mut buf,
            win_w,
            win_h,
            &self.quick_rects,
            &self.quickbar,
            self.quick_press.map(|(_, i)| i),
            &cooldowns,
        );
        // The game is asking which plate to register into: ring them all.
        if self.picking_slot {
            overlay::draw_pick_highlight(&mut buf, win_w, win_h, &self.quick_rects);
        }
        // ★ The PAGE button is FIELD or PICKING. Off-field it is normally hidden (a page
        // flip re-binds guest quickbar slots, which is not a thing to do with a game
        // window open) — but the 퀵바등록 flow is the one exception the player asked for
        // (2026-08-04): "등록할때도 페이지 전환 버튼 있어야해. 페이지 전환해서 등록도
        // 가능해야해". While the plates are lit the flip is the point, so it is allowed
        // and taken from the CURRENT mode's seats, the same ones the lit plates came from.
        //
        // No page DIGIT any more (player request 2026-08-04: "퀵바 페이지 번호 제거").
        if field || self.picking_slot {
            if let Some(r) = placement::rect_of(placement::CtrlId::QuickPage, self.mode, win_w, win_h) {
                overlay::draw_mask_button_spanned(
                    &mut buf,
                    win_w,
                    win_h,
                    r,
                    crate::ui_icons::Icon::QuickPage,
                    false,
                    overlay::QUICKPAGE_ICON_SPAN,
                );
            }
        }
        if field {
            // The minimap stays FIELD only: hiding the minimap is meaningless with one of
            // the game's own windows covering it. In a no-save AREA (boss room) the whole
            // minimap UI disappears — panel AND toggle (2026-08-07: "보스방일 때는 미니맵도
            // 꺼지고").
            if minimap_ui && self.minimap_on {
                overlay::draw_minimap(&mut buf, win_w, win_h, self.layout.game, self.minimap.as_ref(), self.minimap_marker, &self.minimap_mobs);
            } else if let Some(r) = minimap_restore {
                // ★ 2026-10-21: the panel tapped away leaves its folded-map button in its
                //   corner — the only way back besides the world map's corner box.
                overlay::draw_mask_button(&mut buf, win_w, win_h, r, crate::ui_icons::Icon::MinimapToggle, false);
            }
        }
        // ★ The two TOP TABS (2026-10-01, hero3's form): `MAP` — the minimap's switch, on
        // screen whether the panel is up or not — then SAVE NOW. See `map_tab_zone` and
        // `quicksave_zone` for the only gates they get.
        if let Some(r) = map_tab {
            overlay::draw_top_tab(&mut buf, win_w, win_h, r, "MAP");
        }
        if let Some(r) = quicksave {
            overlay::draw_top_tab(&mut buf, win_w, win_h, r, "SAVE");
        }
        // ★ The key-'7' button drew its own face here (the equipped secondary weapon's
        // icon) until 2026-09-01. hero2 has no secondary weapon, `7` is skill slot 3, and
        // the plate that sends it already draws that skill's icon — so there is nothing
        // extra to paint. `0` and `#` are plain labelled buttons in `layout.buttons` and
        // `draw_controls` paints them.
        // ★ 59차: 게임패드 HUD — one pass over every control that a pad button fires,
        // stamping the POSITION of that button on it (`overlay::draw_pad_badges`). Last,
        // so a badge is never painted under a face that some control paints in two
        // steps, and before the settings modal, whose dim pass should cover it like it
        // covers everything else.
        //
        // The three host-owned controls have no guest key to look up, so they are badged
        // by ACTION — and each is badged only where it is actually drawn above, or the
        // badge would be a promise about a control that is not on screen.
        if crate::pad_hud_active() && self.editor.is_none() {
            overlay::draw_pad_badges(&mut buf, win_w, win_h, &self.layout, &self.quick_rects, &self.quickbar);
            if field || self.picking_slot {
                if let Some(r) = placement::rect_of(placement::CtrlId::QuickPage, self.mode, win_w, win_h) {
                    overlay::draw_pad_badge_action(&mut buf, win_w, win_h, r, crate::gamepad::PadAction::QuickPage);
                }
            }
            if let Some(r) = quicksave {
                overlay::draw_pad_badge_action(&mut buf, win_w, win_h, r, crate::gamepad::PadAction::QuickSave);
            }
            // The minimap TOGGLE is a host control, but the key that repaints the map is
            // the guest's `*` — so the badge is the one the pad sends for `*`, which is
            // also what the panel itself responds to. Only while the panel is hidden, as
            // when the toggle was drawn only then: with the panel up the map is already
            // open. (The `MAP` tab itself is on screen either way since 2026-10-01.)
            if let Some(r) = map_tab.filter(|_| !self.minimap_on) {
                overlay::draw_pad_badge_action(&mut buf, win_w, win_h, r, crate::gamepad::PadAction::Key(KeyCode::STAR));
            }
        }
        if self.world_open {
            // The cursor's stud breathes on a 900 ms triangle; the pin bounces for
            // 12 frames of 33 ms after a selection (hero5's pin motion).
            let t = self.started.elapsed().as_millis() % 900;
            let glow = if t < 450 { t * 255 / 450 } else { (900 - t) * 255 / 450 } as u32;
            let lift = overlay::wm_pin_lift((self.world_sel_at.elapsed().as_millis() / 33) as u32);
            overlay::draw_worldmap(&mut buf, win_w, win_h, self.world_cam, self.stage_map, self.world_sel, glow, lift, self.minimap_on);
            overlay::draw_menu_dpad(&mut buf, win_w, win_h);
            if let Some(r) = map_tab {
                overlay::draw_top_tab(&mut buf, win_w, win_h, r, "MAP");
            }
        }
        if self.show_settings {
            overlay::draw_settings(
                &mut buf,
                win_w,
                win_h,
                &self.settings_items,
                landscape,
                self.joystick,
                config::read_immersive(&self.data_dir),
                config::read_base_h(&self.data_dir),
                config::read_heat_saver(&self.data_dir),
                // What the button says under "SERVER". Loopback = the adb-reverse
                // dev loop; otherwise it matters a lot whether the connection is
                // encrypted, because on a public server the account id travels on
                // it — so an unencrypted remote server is called out as PLAIN.
                {
                    let ep = config::read_server_endpoint(&self.data_dir);
                    let local = ep.host == "127.0.0.1" || ep.host == "localhost" || ep.host == "::1";
                    match (local, ep.tls) {
                        (true, _) => "LOCAL",
                        (false, true) => "SECURE",
                        (false, false) => "PLAIN!",
                    }
                },
                self.settings_scroll,
            );
            // ★★ 2026-10-20: the notices, over the modal — the settings rows' own answers
            // (발열 절감 · 게임패드 HUD) are raised while it is up. ★ 57차 (hero3): the toast
            // used to sit above the quickbar, where the modal's dim pass painted over it and
            // silently ate every line a settings row raised.
            if notices_pass(&mut self.notices, self.layout.game, self.layout.joystick.is_some(), &mut buf, win_w, win_h, (lw, fb_h, y0, lh), false) {
                notices_animating = true;
            }
        }
        // Last of all, over everything including the notices and the settings modal: the
        // guest is gone and nothing drawn under this is true any more.
        if let Some(lines) = self.dead.as_deref() {
            overlay::draw_fatal_notice(&mut buf, win_w, win_h, self.layout.game, lines);
        }

        if let Err(e) = buf.present() {
            log::warn!("present failed: {e}");
        }
        if notices_animating {
            self.request_redraw();
        }
    }

    fn update_touch(&mut self, id: u64, new_key: Option<KeyCode>) {
        let old_key = self.touches.get(&id).copied();
        if old_key == new_key {
            return;
        }
        match new_key {
            Some(k) => {
                self.touches.insert(id, k);
            }
            None => {
                self.touches.remove(&id);
            }
        }
        // ★ The UP is paired with what the DOWN actually DID, not with the popup state
        // now — see `popup_confirm_touches`. Cleared unconditionally, so a touch that
        // slides from OK onto another key does not leave a stale record behind.
        let was_diverted = self.popup_confirm_touches.remove(&id);
        if let Some(ok) = old_key
            && !self.touches.values().any(|&v| v == ok)
            // A diverted OK was never pressed as far as the guest is concerned, so it
            // must not be released.
            && !was_diverted
        {
            let _ = self.input_tx.send(InputEvent::Key { code: ok, down: false });
        }
        if let Some(nk) = new_key
            && self.touches.values().filter(|&&v| v == nk).count() == 1
        {
            // ★ With the game's rewritten context popup on screen, a CONFIRM belongs to
            // the popup, and only the WORKER can say what the highlighted row means:
            // our copy of it is up to ~80 ms old, and the row decides which guest
            // function runs. So OK becomes `PopupConfirm` and the worker re-reads the
            // row before doing anything — including passing a real OK through when the
            // row is the game's own 사용하기 / 포인트투자.
            //
            // Only the DOWN edge is diverted. The matching UP is dropped rather than
            // sent, because the worker synthesises the whole down+up pair itself for
            // the pass-through case; sending a bare keyup for a key the guest never saw
            // pressed is how you get a stuck key.
            if nk == KeyCode::OK && self.popup.is_some() {
                self.popup_confirm_touches.insert(id);
                let _ = self.input_tx.send(InputEvent::PopupConfirm);
            } else {
                // CLR while the plates are lit means "never mind": drop out of picking
                // AND let the key through, which closes the game's popup too. Leaving
                // the highlight on after the popup is gone would be a HUD that lies.
                if nk == KeyCode::CLEAR && self.picking_slot {
                    let _ = self.input_tx.send(InputEvent::PopupCancelPick);
                }
                let _ = self.input_tx.send(InputEvent::Key { code: nk, down: true });
            }
        }
        self.refresh_pressed();
        self.request_redraw();
    }

    /// Which input source was used last (59차). Under the default AUTO preference this
    /// is what decides whether the HUD shows on-screen walk controls or pad badges, so
    /// it relayouts the moment the answer changes — and also the first time a pad is
    /// seen at all, because that is when the PAD HUD settings row appears.
    ///
    /// Called from the INPUT path only. Calling it from the drawing path would make the
    /// HUD depend on what was last drawn, which is a loop.
    /// Returns true if the HUD changed as a result — the caller may then want to
    /// SWALLOW the input that caused the change (see the touch handler).
    fn note_input(&mut self, pad: bool) -> bool {
        let (was_active, was_seen) = (crate::pad_hud_active(), crate::pad_seen());
        crate::note_input_source(pad);
        let flipped = crate::pad_hud_active() != was_active;
        if flipped || crate::pad_seen() != was_seen {
            self.relayout();
            self.request_redraw();
        }
        flipped
    }

    fn key_dir(key: KeyCode) -> Option<overlay::WmDir> {
        match key {
            KeyCode::LEFT => Some(overlay::WmDir::Left),
            KeyCode::RIGHT => Some(overlay::WmDir::Right),
            KeyCode::UP => Some(overlay::WmDir::Up),
            KeyCode::DOWN => Some(overlay::WmDir::Down),
            _ => None,
        }
    }

    /// The game stands still while the world map is up, the same way a menu
    /// window stops the field — ★ 2026-10-21 but the music plays on: the worker
    /// PARKS the guest's tasks instead of stopping its ticks (`Emulator::park_guest`),
    /// as a menu keeps the music going. The key-ups go out first so a held walk does
    /// not resume.
    fn hold_world(&mut self, on: bool) {
        self.world_open = on;
        if !on {
            self.world_stick = None;
        }
        let _ = self.input_tx.send(InputEvent::WorldHold(on));
    }

    /// MAP opens the world map on the place the character is in, and closes it.
    /// A held walk key is released first, so the character does not keep walking
    /// under the picture.
    fn toggle_worldmap(&mut self) {
        if self.world_open {
            self.hold_world(false);
        } else if self.mode == overlay::Mode::Field {
            let touches: Vec<_> = self.touches.keys().copied().collect();
            for id in touches {
                self.update_touch(id, None);
            }
            self.slide_touches.clear();
            self.joy_end();
            if let Some(old) = self.pad_stick.take() {
                let _ = self.input_tx.send(InputEvent::Key { code: old, down: false });
            }
            for (key, diverted) in self.pad_held.drain() {
                if !diverted {
                    let _ = self.input_tx.send(InputEvent::Key { code: key, down: false });
                }
            }
            let (iw, ih) = overlay::worldmap_size();
            let (vw, vh) = overlay::worldmap_view_size(self.win_w, self.win_h);
            self.world_sel = overlay::wm_place_index(self.stage_map);
            self.world_cam = overlay::wm_open_at(self.world_sel, iw, ih, vw, vh);
            self.world_target = self.world_cam;
            self.world_sel_at = Instant::now();
            self.world_stick = None;
            self.world_pinned = true;
            self.hold_world(iw > 0 && ih > 0);
        }
        self.request_redraw();
    }

    /// An arrow: the cursor steps along a road to the neighbour that lies that
    /// way (`overlay::wm_step` — nothing happens when no road leaves that way),
    /// and the camera sets off after it.
    fn step_world(&mut self, dir: overlay::WmDir) {
        let Some(next) = overlay::wm_step(self.world_sel, dir) else { return };
        self.select_world(next);
    }

    /// The cursor lands on `index` (an arrow or a touch on the place): the pin
    /// bounces there and the camera glides to frame it. Snaps only while the map
    /// is still pinned to a live map-number reading (`UserEvent::StageMap`).
    fn select_world(&mut self, index: usize) {
        let (iw, ih) = overlay::worldmap_size();
        let (vw, vh) = overlay::worldmap_view_size(self.win_w, self.win_h);
        self.world_sel = index;
        self.world_target = overlay::wm_open_at(index, iw, ih, vw, vh);
        self.world_sel_at = Instant::now();
        self.world_pinned = false;
        // Wake the animation now rather than at the next scheduled frame.
        self.world_blink_at = None;
        self.request_redraw();
    }

    /// One gamepad/keyboard control edge (58차). The DOWN mirrors `update_touch`'s
    /// three-way split exactly — popup diversion for OK, `PopupCancelPick` for CLR
    /// while the plates are lit, plain key otherwise — because as far as the guest
    /// is concerned a pad button IS a phone key, and two entry points with
    /// different popup rules would be two OK behaviours to debug.
    fn gamepad_action(&mut self, action: crate::gamepad::PadAction, down: bool, repeat: bool) {
        use crate::gamepad::PadAction;
        if self.world_open {
            if let PadAction::Key(key) = action {
                if let Some(dir) = Self::key_dir(key) {
                    if down {
                        self.step_world(dir);
                    }
                } else if key == KeyCode::CLEAR && down && !repeat {
                    self.hold_world(false);
                    self.request_redraw();
                }
            }
            return;
        }
        match action {
            PadAction::Quick(plate) => {
                // ★★ hero3 79차-B, ported 2026-09-25: a pad button names a PLATE. It mirrors
                // the touch plate exactly — lit plates = "register into MY plate", the field
                // = the worker seats it and presses its slot, anywhere else = nothing — with
                // hero3 81차's one exception: off the field a pad has no keypad, so the first
                // five plates become the digits `menu_digit` gives (1·3·7·9·#), through the
                // `Key` arm so they get its held/release bookkeeping. The release is matched
                // by what is HELD, not by the mode at release time, or a digit pressed in a
                // window whose close lands on the field would stick.
                if let Some(key) = crate::gamepad::menu_digit(plate) {
                    let pressed_off_field = down && !self.picking_slot && self.mode != overlay::Mode::Field;
                    let releasing_held = !down && self.pad_held.contains_key(&key);
                    if pressed_off_field || releasing_held {
                        self.gamepad_action(PadAction::Key(key), down, repeat);
                        return;
                    }
                }
                if !down || repeat {
                    return;
                }
                // A plate switched off in the editor is not reachable from a pad either — a
                // pad never consults `quick_rects`, so it has to be said here. The mapping
                // stays IDENTITY: `Quick(n)` is always plate n.
                if crate::quickpage::plate_hidden(crate::quick_hidden(), plate) {
                    log::info!("pad: plate {} is hidden — ignoring", plate + 1);
                    return;
                }
                if self.picking_slot {
                    let _ = self.input_tx.send(InputEvent::PopupBindSlot { slot: plate });
                    return;
                }
                if self.mode != overlay::Mode::Field {
                    return;
                }
                // The same gates as a tap: an empty plate does nothing, and a skill still
                // recovering does not recast (the game's own counter would refuse it anyway).
                let Some(slot) = self.quickbar.iter().find(|s| s.slot == plate).copied() else { return };
                if matches!(slot.kind, SlotKind::Empty) {
                    return;
                }
                if slot.cooldown.is_some_and(|(left, total)| left > 0 && total > 0) {
                    return;
                }
                let _ = self.input_tx.send(InputEvent::QuickActivate { plate });
            }
            PadAction::QuickPage => {
                // Same gate as the on-screen PAGE button: field, or while the game
                // is asking which plate to register into. A flip re-binds guest
                // quickbar slots, which is not a thing to do with a window open.
                if down && !repeat && (self.mode == overlay::Mode::Field || self.picking_slot) {
                    let _ = self.input_tx.send(InputEvent::QuickPageNext);
                }
            }
            PadAction::QuickSave => {
                // Same shape as the SAVE row: the UI decides nothing. Whether a save is
                // possible depends on the guest's own "may I save here" byte, which only
                // the worker can read without a race — it answers with a `Toast` either
                // way, which is also the feedback that the button did anything at all.
                if down && !repeat {
                    let _ = self.input_tx.send(InputEvent::QuickSave);
                }
            }
            PadAction::Key(key) => {
                if down {
                    // The system's auto-repeat and any double-down are dropped: the
                    // WORKER replays every held key each 100 ms (`Keyrepeat`), and a
                    // second keydown would make the guest re-latch mid-hold.
                    if repeat || self.pad_held.contains_key(&key) {
                        return;
                    }
                    // ★ While the plates are lit, a slot digit means "register into the
                    //   plate sitting in that slot" (hero3 81차: the pad addresses guest
                    //   slots by digit; the worker knows which plate that is).
                    if self.picking_slot
                        && let Some(guest) = (0..crate::GUEST_QUICK_SLOTS).find(|g| crate::quickslot_hotkey(*g).is_some_and(|(k, _)| k == key))
                    {
                        self.pad_held.insert(key, true);
                        let _ = self.input_tx.send(InputEvent::PopupBindGuest { guest });
                        return;
                    }
                    let diverted = key == KeyCode::OK && self.popup.is_some();
                    self.pad_held.insert(key, diverted);
                    if diverted {
                        let _ = self.input_tx.send(InputEvent::PopupConfirm);
                    } else {
                        if key == KeyCode::CLEAR && self.picking_slot {
                            let _ = self.input_tx.send(InputEvent::PopupCancelPick);
                        }
                        let _ = self.input_tx.send(InputEvent::Key { code: key, down: true });
                    }
                } else if let Some(diverted) = self.pad_held.remove(&key) {
                    // A diverted OK was never pressed as far as the guest is
                    // concerned, so it must not be released (stuck-key rule —
                    // see `popup_confirm_touches`).
                    if !diverted {
                        let _ = self.input_tx.send(InputEvent::Key { code: key, down: false });
                    }
                }
            }
        }
    }

    /// One stick/hat/trigger sample. Stick and hat are SUMMED before quantising:
    /// the hat is digital ±1 so it dominates whenever pressed, and nobody deflects
    /// both at once on purpose. `joy_dir` then applies the same engage/keep/turn
    /// hysteresis as the touch stick, and the transition is
    /// release-old-THEN-press-new because the guest latches its walk direction on
    /// keydown. Triggers become synthetic `BUTTON_L2/R2` edges through the SAME
    /// mapping table, so a `wie_gamepad.txt` remap of 104/105 applies to the analog
    /// triggers too.
    fn gamepad_axes(&mut self, x: f32, y: f32, hx: f32, hy: f32, lt: f32, rt: f32) {
        if self.world_open {
            if let Some(old) = self.pad_stick.take() {
                let _ = self.input_tx.send(InputEvent::Key { code: old, down: false });
            }
            let dir = overlay::joy_dir((x + hx) as f64, (y + hy) as f64, 1.0, None);
            if dir != self.world_stick {
                self.world_stick = dir;
                if let Some(key) = dir
                    && let Some(d) = Self::key_dir(key)
                {
                    self.step_world(d);
                }
            }
            return;
        }
        // ★ 패드 버튼 설정 (2026-10-09): the stick and the hat do nothing under the screen
        // (directions are not remappable — `padmap::capturable`), but an analog trigger's edge
        // is a button press there like any other, so the triggers still run.
        if self.padmap.is_some() {
            self.pad_trigger(true, lt);
            self.pad_trigger(false, rt);
            return;
        }
        let (dx, dy) = ((x + hx) as f64, (y + hy) as f64);
        let new = overlay::joy_dir(dx, dy, 1.0, self.pad_stick);
        if new != self.pad_stick {
            // A sample that changes the latched direction is real pad input — an idle
            // stick's noise is not (see the `GamepadAxes` arm).
            let _ = self.note_input(true);
            // Same turn rule as the touch stick (`InputEvent::Turn`).
            match (self.pad_stick, new) {
                (Some(old), Some(nk)) => {
                    let _ = self.input_tx.send(InputEvent::Turn { from: old, to: nk });
                }
                (Some(old), None) => {
                    let _ = self.input_tx.send(InputEvent::Key { code: old, down: false });
                }
                (None, Some(nk)) => {
                    let _ = self.input_tx.send(InputEvent::Key { code: nk, down: true });
                }
                (None, None) => {}
            }
            self.pad_stick = new;
        }
        self.pad_trigger(true, lt);
        self.pad_trigger(false, rt);
    }

    /// One analog trigger's edge detection + dispatch (`left` = L2, else R2).
    fn pad_trigger(&mut self, left: bool, value: f32) {
        let held = if left { self.pad_triggers.0 } else { self.pad_triggers.1 };
        let now = crate::gamepad::trigger_held(held, value);
        if now == held {
            return;
        }
        if left {
            self.pad_triggers.0 = now;
        } else {
            self.pad_triggers.1 = now;
        }
        let code = if left {
            crate::gamepad::KEYCODE_BUTTON_L2
        } else {
            crate::gamepad::KEYCODE_BUTTON_R2
        };
        // ⛔ The edge is tracked above whether or not the pad screen is open, so a trigger
        // held across 저장 releases cleanly instead of re-pressing on the next sample.
        if self.padmap.is_some() {
            self.padmap_button(code, now, false);
            return;
        }
        if let Some(action) = crate::gamepad::button_action(code) {
            self.gamepad_action(action, now, false);
        }
    }

    /// Release everything the gamepad holds. Called from the same teardown sites
    /// as the touch cleanup (suspend, focus loss): once the activity loses focus
    /// Java stops dispatching, so a still-held key would auto-repeat at the guest
    /// forever — the exact stuck-key failure the touch paths already guard against.
    fn release_gamepad(&mut self) {
        for (key, diverted) in self.pad_held.drain() {
            if !diverted {
                let _ = self.input_tx.send(InputEvent::Key { code: key, down: false });
            }
        }
        if let Some(old) = self.pad_stick.take() {
            let _ = self.input_tx.send(InputEvent::Key { code: old, down: false });
        }
        // Their keyups (if any) just went out through `pad_held` above.
        self.pad_triggers = (false, false);
    }

    /// Toggle the shared background/lock gate. Idempotent; logs only on change.
    /// While set, the emulator worker stops ticking and the MIDI synth emits
    /// silence, so no audio continues when the app is backgrounded or locked.
    fn set_paused(&self, paused: bool) {
        if self.paused.swap(paused, Ordering::SeqCst) != paused {
            log::info!("{}", if paused { "app -> PAUSED (background/lock): audio+emu halted" } else { "app -> RESUMED (foreground): audio+emu running" });
            // ★ 2026-08-15 — Android RESETS our sockets when the app leaves the
            // foreground (server log: two `connection reset` 8 s after the last
            // heartbeat). Tell the net layer so the first network call after the
            // resume re-probes instead of trusting a connection that is already gone.
            // Recorded on the way DOWN: the resume itself must not be the trigger,
            // because a socket opened after the resume is healthy and must not be
            // suspected. See `network::note_backgrounded`.
            if paused {
                crate::network::note_backgrounded();
            }
        }
    }

    fn handle_settings(&mut self, action: overlay::SettingsAction) {
        match action {
            overlay::SettingsAction::SetPortrait => {
                orientation::set(false);
                config::write_landscape(&self.data_dir, false);
                self.show_settings = false;
            }
            overlay::SettingsAction::SetLandscape => {
                orientation::set(true);
                config::write_landscape(&self.data_dir, true);
                self.show_settings = false;
            }
            overlay::SettingsAction::SetDpad => {
                self.set_control_style(false);
            }
            overlay::SettingsAction::SetJoystick => {
                self.set_control_style(true);
            }
            overlay::SettingsAction::ToggleImmersive => {
                let now = !config::read_immersive(&self.data_dir);
                config::write_immersive(&self.data_dir, now);
                log::info!("immersive pref -> {now} (applies at next start)");
            }
            overlay::SettingsAction::CycleBaseH => {
                let cur = config::read_base_h(&self.data_dir);
                let i = config::BASE_H_CHOICES.iter().position(|h| *h == cur).unwrap_or(1);
                let next = config::BASE_H_CHOICES[(i + 1) % config::BASE_H_CHOICES.len()];
                config::write_base_h(&self.data_dir, next);
                log::info!("immersive base height -> {next} (applies at next start)");
            }
            overlay::SettingsAction::EditLayout => self.enter_editor(),
            overlay::SettingsAction::ToggleTouchType => {
                // Applies at once — the layout is rebuilt below, which is what swaps
                // the whole-window stick zone (free), the seat-sized one (fixed_A) and
                // the field D-pad buttons (fixed_B).
                let now = (crate::touch_type() + 1) % 3;
                crate::set_touch_type(now);
                config::write_touch_type(&self.data_dir, now);
                self.touch_type_changed();
                log::info!("touch type -> {now} (0 free, 1 fixed_A, 2 fixed_B)");
            }
            overlay::SettingsAction::ToggleHeatSaver => {
                // Applies at next start: the render scale fixes the guest
                // framebuffer/window coordinate mapping and the audio synth cost at
                // boot, so flipping either live would remap every rect mid-frame.
                let now = !config::read_heat_saver(&self.data_dir);
                config::write_heat_saver(&self.data_dir, now);
                self.host_notice(if now { "발열 절감: 켬 (재시작 후 적용)" } else { "발열 절감: 끔 (재시작 후 적용)" });
                log::info!("heat saver -> {now} (applies at next start)");
            }
            // 효과음 크기 (hero3 2026-08-27, ported 2026-09-18). Applies AT ONCE and
            // the modal stays open: the only way to pick a level is to step through
            // them and listen, so closing the panel on each tap would make the row
            // unusable. `audio_sink` reads the level per clip.
            overlay::SettingsAction::CycleSfxLevel => {
                let now = crate::audio_mix::sfx_level_next(crate::sfx_level());
                crate::set_sfx_level(now);
                config::write_sfx_level(&self.data_dir, now);
                self.request_redraw();
                log::info!("sfx level -> {now} ({})", crate::audio_mix::sfx_trim(now));
            }
            // 경험치 배율 (2026-09-23). Same shape as 효과음 크기: applies AT ONCE (the
            // worker keeps the guest's multiplier word equal to `crate::exp_rate` every
            // few frames — `emulator::ExpRate`) and the modal stays open, so the player
            // steps 기존 → x2 → x4 → x6 → 기존 and sees the row say which is live. No
            // toast: the row IS the state, and the toast font is the closed 12×12 list.
            overlay::SettingsAction::CycleExpRate => {
                let now = crate::guest_build::exp_rate_next(crate::exp_rate());
                crate::set_exp_rate(now);
                config::write_exp_rate(&self.data_dir, now);
                self.request_redraw();
                log::info!("exp rate -> x{now}");
            }
            // 오토루팅 (2026-09-24). Applies at once and the modal stays open, like the
            // other game-rule rows; the worker reads `crate::auto_loot` every poll.
            overlay::SettingsAction::ToggleAutoLoot => {
                let now = !crate::auto_loot();
                crate::set_auto_loot(now);
                config::write_auto_loot(&self.data_dir, now);
                self.request_redraw();
                log::info!("auto loot -> {now}");
            }
            overlay::SettingsAction::TogglePadHud => {
                // Applies AT ONCE, unlike HEAT SAVER: the layout is rebuilt from the
                // preference, so `relayout` is the whole "apply" step. The row stays
                // reachable for the next tap (the modal is not closed) because this is a
                // three-state cycle a player will step through to see what each does.
                let now = crate::pad_hud_pref().next();
                crate::set_pad_hud_pref(now);
                config::write_pad_hud(&self.data_dir, now);
                self.relayout();
                self.host_notice(match now {
                    crate::padhud::PadHud::Auto => "게임패드 HUD: 자동",
                    crate::padhud::PadHud::On => "게임패드 HUD: 항상 켬",
                    crate::padhud::PadHud::Off => "게임패드 HUD: 끔",
                });
                log::info!("pad hud -> {now:?}");
            }
            overlay::SettingsAction::EditServer => {
                // Platform text dialog (Java), pre-filled with the endpoint in
                // force. It writes `wie_server.txt` itself and `network.rs`
                // re-reads it on the game's next connect, so there is nothing to
                // apply here and no restart.
                let ep = config::read_server_endpoint(&self.data_dir);
                let scheme = if ep.tls { "tls://" } else { "" };
                server_prompt::prompt(&format!("{scheme}{}:{}", ep.host, ep.port));
                self.show_settings = false;
            }
            overlay::SettingsAction::EditAccount => {
                // Same one-way shape as the server dialog: Java shows the code,
                // validates a replacement and writes `wie_account_id` itself. The
                // network layer re-reads the identity for every login packet
                // (`network::AndroidNetwork::send`), so a change takes effect on the
                // game's NEXT LOGIN — no app restart, which is what the dialog says.
                server_prompt::prompt_account(&config::account_id_hex(&self.data_dir));
                self.show_settings = false;
            }
            overlay::SettingsAction::QuickSave => {
                // The UI does not decide anything: whether a save is possible depends on
                // the guest's own "may I save here" byte, which only the worker can read
                // without a race. It answers with a `Toast` either way.
                let _ = self.input_tx.send(InputEvent::QuickSave);
                self.show_settings = false;
            }
            overlay::SettingsAction::Diagnostics => {
                // The Rust half of the text; Java prepends app/OS/device versions and
                // offers 복사. See `audio_sink::audio_status` for why this exists.
                let (lw, lh) = crate::logical();
                let info = format!(
                    "audio: {}\nrender {}x{} (heat saver div {}), logical {}x{}, immersive {}\nmode {:?}, landscape pref {}, blit hle {}, heat saver {}\npad hud {:?} (now {}), pad seen {}\n\n{}",
                    crate::audio_sink::audio_status(),
                    self.win_w,
                    self.win_h,
                    self.render_div,
                    lw,
                    lh,
                    crate::immersive_active(),
                    self.mode,
                    config::read_landscape(&self.data_dir),
                    config::read_blit_hle(&self.data_dir),
                    config::read_heat_saver(&self.data_dir),
                    crate::pad_hud_pref(),
                    if crate::pad_hud_active() { "PAD" } else { "TOUCH" },
                    crate::pad_seen(),
                    // ★ 59차: the pad map in Korean, by POSITION. This dialog is Java, so
                    // it can hold Hangul the overlay's 5x7 font cannot — which makes it
                    // the one place a player can read "면버튼 왼쪽 = 퀵슬롯 1" instead of
                    // guessing what the letter printed on their pad means.
                    crate::gamepad::legend(),
                );
                server_prompt::show_diagnostics(&info);
                self.show_settings = false;
            }
            overlay::SettingsAction::PadMap => self.padmap_open(),
            overlay::SettingsAction::Close => {
                self.show_settings = false;
            }
        }
        self.request_redraw();
    }

    // ── 설정 → 패드 버튼 설정 (2026-10-09, with hero3) ─────────────────────────────
    //
    // User: *"상단 톱니바퀴 설정으로 패드 키를 맵핑하는 기능을 넣을꺼야 … hero2에도 마찬가지로
    // 넣어야해"*, then — the same day, for both games — *"줄을 누르고 패드버튼을 누르는게 아니라
    // 그냥 실제 인게임 화면상에 버튼들을 하이라이트 처리하고 각 버튼을 눌르고 패드 버튼을 누르는
    // 방식으로 변경해줘. hero2도 마찬가지야"*. So the screen is the FIELD HUD as it is played,
    // every control a pad button can be given lit where it sits (`overlay::padmap_scene` /
    // `draw_padmap`), and the functions with no control on screen (the 회복 아이템 keys, a
    // hidden plate) as cells in the screen's own window — this game's window, with 취소 /
    // 저장 on its title band and 기본값 under the cells. A tap on a control starts a five-second
    // prompt, and the next pad button pressed is that control's button. A conflict swaps
    // (`padmap::Pad::assign`). A bare press — nothing picked — says what that button does and
    // lights its control. 저장 installs the table, writes `wie_gamepad.txt` and goes back to
    // the settings window, whose 「패드 버튼 설정」 row then says 「사용자 배치」; 취소 drops the
    // copy and goes back too.
    //
    // ⛔ No toast: this game's toast font is the closed 12×12 list (`hangul.rs`). The window's
    // lines are drawn with `hostfont`, which has every syllable.

    const PADMAP_TITLE: &'static str = "패드 버튼 설정";

    /// What the window says when nothing is going on.
    fn padmap_hint() -> [String; 2] {
        ["화면의 버튼을 누르고 패드 버튼을 누르세요".to_string(), "패드 버튼만 누르면 하는 일을 보여줍니다".to_string()]
    }

    /// Open the screen. Everything held is let go first — the fingers like the editor, and
    /// the PAD too: while it is open every button is an answer, not a game key.
    fn padmap_open(&mut self) {
        self.show_settings = false;
        // `enter_editor`'s release, in its order (`joy_end` rebuilds `pressed` from
        // `touches`, so the touches go first).
        self.touches.clear();
        self.popup_confirm_touches.clear();
        for key in self.pressed.drain() {
            let _ = self.input_tx.send(InputEvent::Key { code: key, down: false });
        }
        self.joy_end();
        self.pressed.clear();
        self.ui_touches.clear();
        self.slide_touches.clear();
        self.mm_tap = None;
        self.stats_tap = None;
        self.quick_press = None;
        self.keypad_open = false;
        self.release_gamepad();
        self.padmap = Some(Box::new(PadMapUi {
            draft: crate::gamepad::overrides_snapshot(),
            sel: None,
            capture: None,
            probe: None,
            lines: Self::padmap_hint(),
        }));
        log::info!("pad map: open");
        self.request_redraw();
    }

    /// A function's name — its own label, or its file token for an action this screen does
    /// not offer (an adb-written `5`, say).
    fn padmap_label(action: Option<crate::gamepad::PadAction>) -> String {
        match action {
            None => "없음".to_string(),
            Some(a) => crate::gamepad::REMAP_ROWS
                .iter()
                .find(|(x, _)| *x == a)
                .map_or_else(|| crate::gamepad::token(Some(a)), |(_, l)| (*l).to_string()),
        }
    }

    /// What `redraw` draws, resolved before the surface is borrowed — and where an unanswered
    /// prompt runs out.
    fn padmap_view(&mut self) -> Option<PadMapView> {
        self.padmap.as_ref()?;
        let scene = overlay::padmap_scene(self.win_w, self.win_h, self.joystick);
        let slots: Vec<crate::QuickSlot> = (0..crate::quickpage::PLATES as u8)
            .map(|i| {
                self.quickbar.get(usize::from(i)).copied().unwrap_or(crate::QuickSlot {
                    slot: i,
                    kind: crate::SlotKind::Empty,
                    count: 0,
                    icon: None,
                    cooldown: None,
                    guest: None,
                })
            })
            .collect();
        let ui = self.padmap.as_deref_mut()?;
        let left = ui.capture.and_then(|at| crate::padmap::seconds_left(at.elapsed().as_millis()));
        if ui.capture.is_some() && left.is_none() {
            ui.capture = None;
            ui.sel = None;
            ui.lines = ["시간이 지났습니다".to_string(), "다시 화면의 버튼을 누르세요".to_string()];
            log::info!("pad map: the prompt ran out");
        }
        let badges = scene
            .targets
            .iter()
            .map(|t| crate::gamepad::pos_for_action_in(&ui.draft, crate::gamepad::REMAP_ROWS[t.row].0))
            .collect();
        // One breath every 1.1 s, from the moment the control was picked.
        let pulse = ui
            .capture
            .filter(|_| left.is_some())
            .map(|at| 0.5 - 0.5 * (at.elapsed().as_secs_f64() * std::f64::consts::TAU / 1.1).cos());
        let lines = [
            ui.lines[0].clone(),
            match left {
                Some(s) => format!("바꿀 패드 버튼을 누르세요 ({s}초)"),
                None => ui.lines[1].clone(),
            },
        ];
        let pad = crate::gamepad::pad();
        let mut normal = ui.draft.clone();
        pad.normalize(&mut normal);
        let mut in_force = crate::gamepad::overrides_snapshot();
        pad.normalize(&mut in_force);
        Some(PadMapView {
            scene,
            slots,
            badges,
            sel: ui.sel,
            probe: ui.probe,
            pulse,
            lines,
            dirty: normal != in_force,
            can_reset: !normal.is_empty(),
        })
    }

    /// A finger went down: on a lit control (or a cell in the window) it picks that function
    /// and starts the prompt; on 취소 · 기본값 · 저장 it does that; on the game picture it lets
    /// go of whatever was picked. Answered on the way DOWN — nothing here scrolls, so there is
    /// no gesture to tell a tap from.
    fn padmap_started(&mut self, x: f64, y: f64) {
        if self.padmap.is_none() {
            return;
        }
        let scene = overlay::padmap_scene(self.win_w, self.win_h, self.joystick);
        let hit = overlay::padmap_hit(&scene, x, y);
        match hit {
            Some(overlay::PadMapHit::Cancel) => return self.padmap_close("cancelled"),
            Some(overlay::PadMapHit::Save) => return self.padmap_save(),
            _ => {}
        }
        let Some(ui) = self.padmap.as_deref_mut() else { return };
        match hit {
            Some(overlay::PadMapHit::Reset) => {
                ui.draft.clear();
                ui.sel = None;
                ui.capture = None;
                ui.probe = None;
                ui.lines = ["기본값으로 되돌렸습니다".to_string(), "저장을 누르면 적용됩니다".to_string()];
                log::info!("pad map: draft reset to the built-in layout");
            }
            Some(overlay::PadMapHit::Row(row)) => {
                let (action, label) = crate::gamepad::REMAP_ROWS[row];
                let now = crate::padmap::describe(&crate::gamepad::pad().codes_for(&ui.draft, action), crate::gamepad::button_name);
                ui.sel = Some(row);
                ui.capture = Some(Instant::now());
                ui.probe = None;
                ui.lines = [format!("{label}: 지금 {now}"), String::new()];
                log::info!("pad map: waiting for a button for {action:?}");
            }
            Some(_) => {}
            // The game picture: "never mind".
            None => {
                if ui.sel.is_some() || ui.probe.is_some() {
                    ui.sel = None;
                    ui.capture = None;
                    ui.probe = None;
                    ui.lines = Self::padmap_hint();
                }
            }
        }
        self.request_redraw();
    }

    /// A pad button (a real key, or a trigger's synthetic L2 / R2 edge) while the screen is
    /// open. Only a PRESS means anything: with a control picked and the prompt up it becomes
    /// that control's button; with nothing picked the window says what the button does and
    /// lights its control. Every edge is swallowed — the game must not see a button the
    /// player is pressing at this screen.
    ///
    /// ⛔ A release is NOT matched to anything on the way out: an up for a button held across
    /// 저장 / 취소 reaches `gamepad_action` with nothing in `pad_held` (opening cleared it), and
    /// every arm there ignores an unmatched release.
    fn padmap_button(&mut self, code: i32, down: bool, repeat: bool) {
        let Some(ui) = self.padmap.as_deref_mut() else { return };
        if !down || repeat {
            return;
        }
        let pad = crate::gamepad::pad();
        let name = crate::gamepad::button_name(code);
        let waiting = ui.capture.is_some_and(|at| crate::padmap::seconds_left(at.elapsed().as_millis()).is_some());
        let Some(row) = ui.sel.filter(|_| waiting) else {
            if ui.capture.take().is_some() {
                // The redraw that expires it may not have run yet; the press is still late.
                ui.sel = None;
                ui.lines = ["시간이 지났습니다".to_string(), "다시 화면의 버튼을 누르세요".to_string()];
            } else if !crate::padmap::capturable(code) {
                ui.probe = None;
                ui.lines = ["방향키는 바꿀 수 없습니다".to_string(), "걷기와 메뉴 이동에 쓰입니다".to_string()];
            } else {
                let action = pad.effective(&ui.draft, code);
                ui.probe = action.and_then(crate::gamepad::remap_row);
                ui.sel = None;
                ui.lines = [format!("{name} = {}", Self::padmap_label(action)), "바꾸려면 화면의 버튼을 먼저 누르세요".to_string()];
                log::info!("pad map: code {code} ({name}) fires {action:?}");
            }
            self.request_redraw();
            return;
        };
        let (action, label) = crate::gamepad::REMAP_ROWS[row];
        let result = pad.assign(&mut ui.draft, action, code);
        ui.lines = match result {
            // The prompt stays up: a direction is not an answer, it is a wrong key.
            crate::padmap::Assign::NotCapturable => ["방향키는 바꿀 수 없습니다".to_string(), String::new()],
            crate::padmap::Assign::Same => [format!("{label}: 이미 {name} 입니다"), Self::padmap_hint()[0].clone()],
            crate::padmap::Assign::Refused { lost } => [
                format!("{} 버튼이 없어지므로 바꿀 수 없습니다", Self::padmap_label(Some(lost))),
                "다른 패드 버튼으로 다시 해 보세요".to_string(),
            ],
            // The second line says where the swap sent the other function, in the first
            // line's own form — `기능 = 버튼` both, so no particle has to agree with a digit.
            crate::padmap::Assign::Done { displaced: Some(d), to: Some(from) } => [
                format!("{label} = {name}"),
                format!("{} = {}", Self::padmap_label(Some(d)), crate::gamepad::button_name(from)),
            ],
            crate::padmap::Assign::Done { displaced: Some(d), to: None } => {
                [format!("{label} = {name}"), format!("{} = 없음", Self::padmap_label(Some(d)))]
            }
            crate::padmap::Assign::Done { displaced: None, .. } => [format!("{label} = {name}"), "저장을 누르면 적용됩니다".to_string()],
        };
        if result != crate::padmap::Assign::NotCapturable {
            ui.capture = None;
        }
        log::info!("pad map: {action:?} <- code {code} ({name}): {result:?}");
        self.request_redraw();
    }

    /// 저장: install the copy, write it, back to the settings window.
    fn padmap_save(&mut self) {
        let Some(ui) = self.padmap.as_deref() else { return };
        let pad = crate::gamepad::pad();
        let mut table = ui.draft.clone();
        pad.normalize(&mut table);
        let text = pad.serialize(&table, crate::gamepad::MAP_FILE_HEADER, crate::gamepad::token);
        let written = config::write_gamepad_map(&self.data_dir, &text);
        // ⛔ Applied for this run even when the file could not be written — the log says
        // so; this game has no toast that can.
        log::info!("pad map: saved {} override(s), written = {written}", table.len());
        crate::gamepad::set_overrides(table);
        self.padmap_close("saved");
    }

    /// Take the screen down and put the settings window back — where the player came from,
    /// and whose 「패드 버튼 설정」 row says whether anything is now changed.
    fn padmap_close(&mut self, why: &str) {
        self.padmap = None;
        log::info!("pad map: {why}");
        self.show_settings = true;
        let _ = self.note_input(false);
        // The badges are a reverse lookup of the table and the panel's rows depend on it;
        // `relayout` rebuilds both.
        self.relayout();
        self.request_redraw();
    }

    // ── layout editor ───────────────────────────────────────────────────────

    /// Enter the editor. Everything the player is holding is released first: the
    /// worker replays held keys every 100 ms, so a key still down when input routing
    /// changes would be auto-repeated at the guest forever. Same body as the
    /// suspend / focus-loss teardown, for the same reason.
    fn enter_editor(&mut self) {
        // ★ Only the landscape-immersive HUD has placeable controls. Every other
        // layout family (portrait, and non-immersive landscape) builds a fixed D-pad
        // + keypad from the screen edges and would ignore every stored placement, so
        // opening the editor there would let the player arrange something that never
        // appears. Refuse and say why rather than writing entries for a layout that
        // cannot express them.
        if !overlay::layout_editor_available(self.win_w, self.win_h) {
            // The row is drawn dimmed with "FULL ONLY" in this case and its tap is
            // swallowed by `settings_hit`, so this is belt-and-braces.
            log::info!("layout editor: only available in landscape full-screen (FULL) mode");
            return;
        }
        self.show_settings = false;
        // ★ Order matters: `joy_end` ends with `refresh_pressed`, which rebuilds
        // `pressed` FROM `touches`. Clearing the touches afterwards would leave every
        // still-held key re-inserted into `pressed` after its key-up had already gone
        // to the guest — a button drawn lit for a key nobody is holding.
        self.touches.clear();
        self.popup_confirm_touches.clear();
        for key in self.pressed.drain() {
            let _ = self.input_tx.send(InputEvent::Key { code: key, down: false });
        }
        self.joy_end();
        self.pressed.clear();
        self.ui_touches.clear();
        self.slide_touches.clear();
        self.mm_tap = None;
        self.stats_tap = None;
        self.quick_press = None;
        self.keypad_open = false;
        // The FIELD set: that is the HUD the player spends the game in, it is the one
        // they cannot reach the gear from, and since 2026-09-18 it is the ONLY one —
        // the menu controls read the same seats.
        self.editor = Some(Editor {
            sel: None,
            entry_snapshot: placement::table(),
            hidden_snapshot: crate::quick_hidden(),
            pad: false,
            drag: None,
            pinch: None,
            points: HashMap::new(),
        });
        self.relayout();
        self.request_redraw();
        log::info!("layout editor: open");
    }

    /// Switch the SELECTED quickbar plate off (editor `HIDE`, hero3 82차).
    ///
    /// ★★ 2026-10-20: say `text` in the game's own notice box at the left — the worker renders it in the game's font
    /// (`emulator::host_notice`) and it comes back as a `UserEvent::PickupNotice`. The host's centre-bottom toast is
    /// gone; this is what every site that raised one calls now. ⛔ The text must be drawable by the game's font —
    /// KS X 1001 Hangul and uppercase ASCII — and is listed in `H2_HOST_NOTICE_STRINGS` (its test proves it).
    fn host_notice(&self, text: &'static str) {
        let _ = self.input_tx.send(InputEvent::HostNotice(text));
    }

    /// Applies AT ONCE, like TOUCH TYPE and PAD HUD: the ring is laid out from the
    /// preference, so `relayout` IS the apply step and the player watches the survivors
    /// close the gap. The selection is cleared because the thing it named has just left
    /// the screen. Persisting goes through the WORKER (`InputEvent::QuickHidden`), which
    /// owns `wie_quickpages.txt`.
    ///
    /// ⛔ The refusal cases both say something. A silent no-op here is the "full-width
    /// button that does nothing" the settings panel has a rule against, and the reason
    /// the last plate cannot go is spelled out in `quickpage::hide_plate`.
    fn editor_hide_plate(&mut self) {
        let Some(placement::CtrlId::Quick(plate)) = self.editor.as_ref().and_then(|ed| ed.sel) else {
            self.host_notice("퀵슬롯먼저선택");
            log::info!("layout editor: HIDE needs a quickslot plate selected");
            return;
        };
        let Some(mask) = crate::quickpage::hide_plate(crate::quick_hidden(), plate) else {
            self.host_notice("퀵슬롯하나는남겨야함");
            log::info!("layout editor: refusing to hide plate {} — it is the last visible one", plate + 1);
            return;
        };
        self.apply_quick_hidden(mask);
        if let Some(ed) = self.editor.as_mut() {
            ed.sel = None;
            ed.drag = None;
            ed.pinch = None;
        }
        // A `&'static str`, not the plate number: the player has just watched that
        // plate leave the ring, which says which one better than a digit would.
        self.host_notice("퀵슬롯숨김");
        log::info!(
            "layout editor: plate {} hidden — mask {:#04x}, {} visible",
            plate + 1,
            mask,
            crate::quickpage::visible_count(mask)
        );
    }

    /// Switch every hidden plate back on (editor `SHOW`).
    fn editor_show_plates(&mut self) {
        let was = crate::quick_hidden();
        if was == crate::quickpage::SHOW_ALL {
            self.host_notice("숨긴퀵슬롯없음");
            return;
        }
        self.apply_quick_hidden(crate::quickpage::SHOW_ALL);
        self.host_notice("퀵슬롯전체복원");
        log::info!("layout editor: plates restored — was mask {was:#04x}");
    }

    /// The one place the hidden set changes: live copy, worker (which persists), ring.
    fn apply_quick_hidden(&mut self, mask: u8) {
        crate::set_quick_hidden(mask);
        let _ = self.input_tx.send(InputEvent::QuickHidden { mask: crate::quick_hidden() });
        self.relayout();
        self.request_redraw();
    }

    /// Drop any half-finished drag / pinch without closing the editor. Called from
    /// the same places that release held keys: a gesture whose `Ended` will never
    /// arrive (backgrounded, focus stolen) must not stay armed, and a resize
    /// invalidates the anchors outright.
    fn clear_editor_gesture(&mut self) {
        if let Some(ed) = self.editor.as_mut() {
            ed.drag = None;
            ed.pinch = None;
            ed.points.clear();
        }
    }

    fn exit_editor(&mut self) {
        self.editor = None;
        config::write_layout(&self.data_dir, &placement::table());
        self.relayout();
        self.request_redraw();
        log::info!("layout editor: closed, layout saved");
    }

    /// Commit a new centre and size percent for `id`, but only if the result keeps
    /// the whole set legal.
    ///
    /// ★ The size is passed in as a PERCENT, never re-derived from a pixel rect.
    /// Round-tripping px -> percent -> px floors twice, so a drag that carried its
    /// own width through would shave ~1 % off the control on every touch event: a
    /// 130 %-sized plate (98 px built-in -> 127 px) reads back as 129 %, then 128 %,
    /// and a control the player merely dragged around would visibly wither.
    ///
    /// What IS round-tripped is the rect: it is rebuilt from the stored fractions
    /// before being validated, so the editor accepts exactly what `resolve` will
    /// reconstruct later.
    fn editor_commit(&mut self, id: placement::CtrlId, cx: f64, cy: f64, pct: u32) -> bool {
        if self.editor.is_none() {
            return false;
        }
        let (w, h) = (self.win_w, self.win_h);
        let place = placement::Place {
            cx: (cx / f64::from(w.max(1))) as f32,
            cy: (cy / f64::from(h.max(1))) as f32,
            pct: pct.clamp(placement::PCT_MIN, placement::PCT_MAX),
        };
        // Both modes — see `would_accept_place`. The editor shows the FIELD set, but the
        // table it writes is the one menu mode reads too.
        if !placement::would_accept_place(w, h, id, place) {
            return false;
        }
        placement::set(id, Some(place));
        self.relayout();
        self.request_redraw();
        true
    }

    /// The size percent `id` is currently stored at (100 = built-in).
    fn editor_pct(&self, id: placement::CtrlId) -> u32 {
        self.editor
            .as_ref()
            .and_then(|_| placement::get(id))
            .map(|p| p.pct)
            .unwrap_or(placement::PCT_DEFAULT)
    }

    /// The centre `id` is anchored at, in window px — the STORED fraction where there
    /// is one, so a pinch against a screen edge cannot walk the control sideways.
    fn editor_centre(&self, id: placement::CtrlId) -> (f64, f64) {
        let (w, h) = (self.win_w, self.win_h);
        if self.editor.is_none() {
            return (0.0, 0.0);
        }
        match placement::get(id) {
            Some(p) => (f64::from(p.cx) * f64::from(w), f64::from(p.cy) * f64::from(h)),
            None => {
                let b = placement::builtin(id, overlay::Mode::Field, w, h);
                (f64::from(b.x) + f64::from(b.w) / 2.0, f64::from(b.y) + f64::from(b.h) / 2.0)
            }
        }
    }

    fn editor_started(&mut self, id: u64, x: f64, y: f64) {
        let (w, h) = (self.win_w, self.win_h);
        let items = overlay::edit_items(w, h);
        if let Some(action) = overlay::edit_hit(&items, x, y) {
            match action {
                overlay::EditAction::Pad => {
                    // A preview only: the badges come and go, the selection and every
                    // placement stay exactly as they are.
                    if let Some(ed) = self.editor.as_mut() {
                        ed.pad = !ed.pad;
                        log::info!("layout editor: pad badges {}", if ed.pad { "on" } else { "off" });
                    }
                    self.request_redraw();
                }
                overlay::EditAction::Reset => {
                    if let Some(ed) = self.editor.as_mut() {
                        ed.sel = None;
                        ed.drag = None;
                        ed.pinch = None;
                        placement::clear();
                    }
                    config::write_layout(&self.data_dir, &placement::table());
                    self.relayout();
                    self.request_redraw();
                }
                overlay::EditAction::Flip => {
                    if let Some(ed) = self.editor.as_mut() {
                        ed.drag = None;
                        ed.pinch = None;
                        let mut t = placement::table();
                        t.mirror(w, h);
                        placement::set_table(t);
                        config::write_layout(&self.data_dir, &placement::table());
                        log::info!("layout editor: mirrored left<->right");
                    }
                    self.relayout();
                    self.request_redraw();
                }
                overlay::EditAction::HidePlate => self.editor_hide_plate(),
                overlay::EditAction::ShowPlates => self.editor_show_plates(),
                overlay::EditAction::Cancel => {
                    // Put back exactly what was on disk when the editor opened.
                    if let Some(ed) = self.editor.as_ref() {
                        placement::set_table(ed.entry_snapshot.clone());
                        let mask = ed.hidden_snapshot;
                        if mask != crate::quick_hidden() {
                            crate::set_quick_hidden(mask);
                            let _ = self.input_tx.send(InputEvent::QuickHidden { mask: crate::quick_hidden() });
                        }
                    }
                    config::write_layout(&self.data_dir, &placement::table());
                    self.editor = None;
                    self.relayout();
                    self.request_redraw();
                    log::info!("layout editor: cancelled, layout restored");
                }
                overlay::EditAction::Save => self.exit_editor(),
            }
            if let Some(ed) = self.editor.as_mut() {
                ed.points.insert(id, (x, y));
            }
            return;
        }

        let Some(ed) = self.editor.as_mut() else { return };
        ed.points.insert(id, (x, y));
        // ★ THE SELECTION IS STICKY. A touch that misses every control does NOT clear
        // it (player-reported 2026-07-31: "두 손가락으로 확대하려면 선택이 풀려버려서
        // 동작이 안 돼"). Two fingers never land in the same event — the second one
        // arrives as its own `Started`, almost always on empty screen — so treating a
        // miss as "deselect" made the pinch unreachable by construction. While
        // something is selected, every touch either MOVES or RESIZES it; the selection
        // changes only by tapping a different control.
        let mode = overlay::Mode::Field;
        // `in_editor`: what the editor does not draw must not be selectable by a blind
        // tap either — a plate switched off, and the two fixed top tabs (2026-10-01).
        let hit = placement::resolve(mode, w, h)
            .into_iter()
            .find(|(id, r)| placement::in_editor(mode, *id) && overlay::rect_hit(r, x, y));
        if let Some((cid, _)) = hit
            && Some(cid) != ed.sel
        {
            // Tapping a DIFFERENT control is the one thing that moves the selection.
            ed.sel = Some(cid);
            ed.drag = None;
            ed.pinch = None;
        }

        let Some(sel) = ed.sel else {
            // Nothing selected and nothing under the finger: nothing to do.
            self.request_redraw();
            return;
        };

        // Second finger while something is selected = resize it, from wherever the
        // fingers are. Pinching ON the control is not workable: a skill plate is ~50 px
        // and will not hold two fingertips.
        if ed.pinch.is_none() && ed.points.len() >= 2 {
            // ★ Pair the finger that just landed with the one actually driving the
            // gesture — the dragging finger, else the nearest other contact. Taking the
            // two LOWEST pointer ids (Android hands out the lowest free id) would pick
            // the OLDEST contacts, i.e. a thumb resting on the screen since before the
            // gesture began, and the resize would be measured against a finger that
            // never moves.
            let other = ed
                .drag
                .map(|d| d.id)
                .filter(|d| *d != id && ed.points.contains_key(d))
                .or_else(|| {
                    ed.points
                        .iter()
                        .filter(|(k, _)| **k != id)
                        .min_by(|a, b| distance(*a.1, (x, y)).total_cmp(&distance(*b.1, (x, y))))
                        .map(|(k, _)| *k)
                });
            if let Some(a) = other {
                let dist0 = distance(ed.points[&a], (x, y));
                if dist0 > 1.0 {
                    let pct0 = placement::get(sel).map(|p| p.pct).unwrap_or(placement::PCT_DEFAULT);
                    ed.pinch = Some(Pinch { a, b: id, dist0, pct0 });
                    ed.drag = None;
                    self.request_redraw();
                    return;
                }
            }
        }

        // Single finger, anywhere: MOVE the selection. Tracked as a delta from where
        // the finger went down (not as an offset inside the rect), so a drag that
        // starts off the control does not teleport it under the fingertip.
        if ed.pinch.is_none() {
            let centre = self.editor_centre(sel);
            if let Some(ed) = self.editor.as_mut() {
                ed.drag = Some(Drag { id, from: (x, y), centre0: centre });
            }
        }
        self.request_redraw();
    }

    fn editor_moved(&mut self, id: u64, x: f64, y: f64) {
        let Some(ed) = self.editor.as_mut() else { return };
        ed.points.insert(id, (x, y));
        let sel = ed.sel;
        let Some(sel) = sel else { return };

        if let Some(p) = ed.pinch.as_ref() {
            let (Some(&pa), Some(&pb)) = (ed.points.get(&p.a), ed.points.get(&p.b)) else {
                return;
            };
            // ABSOLUTE, from the anchor captured at pinch start — never a running
            // product of per-event deltas, which winit's per-pointer Moved would
            // apply twice per MotionEvent.
            let ratio = distance(pa, pb) / p.dist0.max(1.0);
            let pct = ((f64::from(p.pct0) * ratio) as u32).clamp(placement::PCT_MIN, placement::PCT_MAX);
            // Resize about the control's own anchor, so it grows in place.
            let (cx, cy) = self.editor_centre(sel);
            self.editor_commit(sel, cx, cy, pct);
            return;
        }

        if let Some(d) = ed.drag
            && d.id == id
        {
            // Carry the size through as a PERCENT (see `editor_commit`) and move by the
            // finger's DELTA from where it went down.
            let pct = self.editor_pct(sel);
            let cx = d.centre0.0 + (x - d.from.0);
            let cy = d.centre0.1 + (y - d.from.1);
            // A rejected position is simply not committed — the control stays where
            // it last legally was, which reads as sliding along the obstacle.
            self.editor_commit(sel, cx, cy, pct);
        }
    }

    fn editor_ended(&mut self, id: u64) {
        let Some(ed) = self.editor.as_mut() else { return };
        ed.points.remove(&id);
        if ed.drag.map(|d| d.id) == Some(id) {
            ed.drag = None;
        }
        if ed.pinch.as_ref().is_some_and(|p| p.a == id || p.b == id) {
            ed.pinch = None;
            // One finger of the pinch is still down: hand it back to MOVE, so lifting
            // one of two fingers slides the control instead of freezing it.
            let still: Vec<u64> = ed.points.keys().copied().collect();
            if let (Some(sel), [other]) = (ed.sel, still.as_slice()) {
                let at = ed.points[other];
                let centre = self.editor_centre(sel);
                if let Some(ed) = self.editor.as_mut() {
                    ed.drag = Some(Drag { id: *other, from: at, centre0: centre });
                }
            }
        }
        // Persist on every finger-up rather than only on DONE: the app can be killed
        // from the background at any moment, and a lost layout is worse than a write.
        config::write_layout(&self.data_dir, &placement::table());
        self.request_redraw();
    }

    fn set_control_style(&mut self, joystick: bool) {
        if self.joystick != joystick {
            self.joy_end(); // release any held joystick dirs before switching
            self.joystick = joystick;
            config::write_joystick(&self.data_dir, joystick);
            self.relayout();
        }
        self.show_settings = false;
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        // Foregrounded: resume emulation + audio (BGM continues from where it froze).
        self.set_paused(false);

        let window = match event_loop.create_window(Window::default_attributes().with_title("WIE")) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("create_window failed: {e}");
                return;
            }
        };
        let context = match Context::new(window.clone()) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Context::new failed: {e}");
                return;
            }
        };

        let size = window.inner_size();
        // HEAT SAVER (57차): everything from here on happens in RENDER space —
        // possibly half the physical window on each axis, upscaled by the
        // compositor. Touch input is divided by the same factor on entry.
        self.win_w = (size.width.max(1) / self.render_div).max(1);
        self.win_h = (size.height.max(1) / self.render_div).max(1);
        log::info!(
            "resumed: window size {}x{} -> render {}x{} (heat saver div {})",
            size.width,
            size.height,
            self.win_w,
            self.win_h,
            self.render_div
        );

        self.window = Some(window);
        self.context = Some(context);
        self.surface = None;

        // First resume: now that the real window size is known, fix the guest
        // framebuffer size and start the emulator. Landscape immersive mode gives
        // the guest an aspect-matched wide framebuffer so the game frame fills the
        // screen with no letterbox and no distortion (see `crate::init_logical`).
        // ★ 2026-10-28: a start whose staged save could not be made one whole save does not run the
        //   game (`savebundle::Outcome::Blocked`) — the boot stays pending, the event loop keeps
        //   running (returning from `android_main` would end the activity under the dialog) and the
        //   dialog's only button closes the app. If the dialog cannot be shown, the app closes now.
        if self.pending_boot.is_some()
            && let Some(text) = crate::vault::boot_blocked()
        {
            if !self.blocked_shown {
                self.blocked_shown = true;
                if !server_prompt::show_blocked(&text) {
                    log::error!("the blocked start's dialog could not be shown — closing the app");
                    std::process::exit(0);
                }
            }
            return;
        }
        if let Some(boot) = self.pending_boot.take() {
            // ★ A slot the vault put back at boot is announced on the first frame.
            // The player asked for the restore and closed the app for it, so the
            // relaunch has to say whether it happened — the log reaches nobody.
            if let Some(msg) = crate::vault::take_boot_notice() {
                self.host_notice(msg);
            }
            // ★ 2026-10-02 세이브 옮기기: so is an import (or why it was not applied) — in the
            //   platform dialog the rest of 세이브 관리 uses, not a host toast.
            if let Some(text) = crate::vault::take_import_notice() {
                server_prompt::show_save_message(&text);
            }
            let immersive = config::read_immersive(&self.data_dir);
            let landscape_pref = config::read_landscape(&self.data_dir);
            let base_h = config::read_base_h(&self.data_dir);
            let (lw, lh) = crate::init_logical(self.win_w, self.win_h, immersive, landscape_pref, base_h);
            log::info!(
                "boot: window {}x{} landscape_pref={landscape_pref} -> guest framebuffer {lw}x{lh} (immersive={immersive}, base_h={base_h})",
                self.win_w,
                self.win_h
            );
            self.last_frame = vec![0xff00_0000u32; (lw * (lh + crate::guest_build::hud_reserved_rows())) as usize];
            crate::emulator::spawn(
                boot.proxy,
                boot.should_redraw,
                boot.input_rx,
                self.data_dir.clone(),
                boot.game_zip,
                self.paused.clone(),
                boot.hud_y,
            );
        }

        self.relayout();
        self.configure_surface();
        self.request_redraw();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        log::info!("suspended: dropping surface/context/window");
        // Backgrounded: halt emulation + audio so nothing keeps playing.
        self.set_paused(true);
        self.surface = None;
        self.context = None;
        self.window = None;
        self.release_gamepad();
        for key in self.pressed.drain() {
            let _ = self.input_tx.send(InputEvent::Key { code: key, down: false });
        }
        self.touches.clear();
        self.popup_confirm_touches.clear();
        self.ui_touches.clear();
        self.slide_touches.clear();
        self.mm_tap = None;
        self.stats_tap = None;
        self.joy = None;
        self.joy_parked = None;
        self.quick_press = None;
        self.clear_editor_gesture();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GamepadButton { code, down, repeat } => {
                let _ = self.note_input(true);
                // ★ 패드 버튼 설정 (2026-10-09): while it is open a button is an ANSWER, looked
                // at by its raw keycode — before the table, which is the thing being changed.
                if self.padmap.is_some() {
                    self.padmap_button(code, down, repeat);
                    return;
                }
                if let Some(action) = crate::gamepad::button_action(code) {
                    self.gamepad_action(action, down, repeat);
                }
            }
            UserEvent::GamepadAxes { x, y, hx, hy, lt, rt } => {
                // A stick at rest still streams samples on some pads, so this is the one
                // pad entry point that must NOT claim the input source on its own — only
                // a sample that actually moves something does (`gamepad_axes`).
                self.gamepad_axes(x, y, hx, hy, lt, rt)
            }
            UserEvent::Paint(data, hd) => {
                // ★ The frame we are DISPLACING goes back to the worker's pool instead of
                // being freed (2026-08-06). `AndroidScreen::paint` allocated a fresh
                // ~476 KB `Vec<u32>` for every guest frame and this line dropped the
                // previous one — an allocate+free pair per frame for a buffer whose size
                // never changes. See `platform::recycle_frame`.
                let retired = std::mem::replace(&mut self.last_frame, data);
                crate::platform::recycle_frame(retired);
                self.hd_frame = hd;
                self.request_redraw();
            }
            UserEvent::MinimapFrame(mm) => {
                // ★ `None` CLEARS the panel (2026-08-03). It used to be dropped, which
                // was a hedge against flicker back when the worker could produce a
                // spurious `None`; it no longer can — the only sender is a confirmed
                // map change — and the hedge meant that walking through a door left the
                // PREVIOUS room's map on screen, stated as confidently as a correct one.
                // An empty panel says "tap me", a wrong one says nothing at all.
                if self.minimap != mm {
                    self.minimap = mm;
                    self.request_redraw();
                }
            }
            UserEvent::OptionsOverlay(p) => {
                if self.options_patches != p {
                    self.options_patches = p;
                    self.request_redraw();
                }
            }
            UserEvent::RevivePrompt(p) => {
                if self.revive_patches != p {
                    self.revive_patches = p;
                    self.request_redraw();
                }
            }
            UserEvent::EquipOverlay(p) => {
                if self.equip_patches != p {
                    self.equip_patches = p;
                    self.request_redraw();
                }
            }
            UserEvent::BagTools(p) => {
                if self.bag_tool_patches != p {
                    self.bag_tool_patches = p;
                    self.request_redraw();
                }
            }
            UserEvent::TouchTypeChanged => self.touch_type_changed(),
            UserEvent::PickupNotice { w, h, argb } => {
                self.notices.push_back(overlay::Notice::new(w, h, argb));
                while self.notices.len() > overlay::NOTICE_MAX {
                    self.notices.pop_front();
                }
                self.request_redraw();
            }
            UserEvent::RaidCountdown { w, h, argb } => {
                // ★★ 2026-10-17 보스 레이드: the countdown counts in one place — the one on screen takes the new picture.
                match self.notices.iter_mut().find(|n| n.is_countdown()) {
                    Some(n) => n.refresh(w, h, argb),
                    None => {
                        self.notices.push_back(overlay::Notice::countdown(w, h, argb));
                        while self.notices.len() > overlay::NOTICE_MAX {
                            self.notices.pop_front();
                        }
                    }
                }
                self.request_redraw();
            }
            UserEvent::StageMap(m) => {
                if self.stage_map != m {
                    self.stage_map = m;
                    if self.world_open && self.world_pinned {
                        let (iw, ih) = overlay::worldmap_size();
                        let (vw, vh) = overlay::worldmap_view_size(self.win_w, self.win_h);
                        self.world_sel = overlay::wm_place_index(m);
                        self.world_cam = overlay::wm_open_at(self.world_sel, iw, ih, vw, vh);
                        self.world_target = self.world_cam;
                    }
                    if self.world_open {
                        self.request_redraw();
                    }
                }
            }
            UserEvent::MinimapMarker(m) => {
                // Only a real move costs a redraw: the worker already suppresses
                // unchanged values, and this is the second guard for the case where the
                // panel is not on screen at all.
                if self.minimap_marker != m {
                    self.minimap_marker = m;
                    if self.minimap_ui_on() && self.minimap_on {
                        self.request_redraw();
                    }
                }
            }
            UserEvent::MinimapMobs(m) => {
                // Same two guards as the marker.
                if self.minimap_mobs != m {
                    self.minimap_mobs = m;
                    if self.minimap_ui_on() && self.minimap_on {
                        self.request_redraw();
                    }
                }
            }
            UserEvent::Portrait(p) => {
                if self.portrait != p {
                    self.portrait = p;
                    self.request_redraw();
                }
            }
            UserEvent::SkillBuffs(b) => {
                if self.skill_buffs != b {
                    self.skill_buffs = b;
                    self.request_redraw();
                }
            }
            UserEvent::BuffGauges(g) => {
                if self.buff_gauges != g {
                    self.buff_gauges = g;
                    self.request_redraw();
                }
            }
            UserEvent::Popup(view) => {
                if self.popup != view {
                    self.popup = view;
                    self.request_redraw();
                }
            }
            UserEvent::BagPage(view) => {
                if self.bag_page != view {
                    self.bag_page = view;
                    self.request_redraw();
                }
            }
            UserEvent::PickingSlot(on) => {
                if self.picking_slot != on {
                    self.picking_slot = on;
                    self.request_redraw();
                }
            }
            UserEvent::WorkerDied { vault } => {
                // ⛔ Korean, and every syllable is in `hangul.rs` — see
                // `tools_re/gen_hangul_glyphs.py`. A missing one draws blank, which here
                // would be a blank box over a frozen game.
                let mut lines = vec!["오류로중단됨", "앱을다시실행하세요"];
                if vault {
                    // What the player actually wants to know next. The route is
                    // 설정 → 진단정보 → 세이브복원 (`MainActivity.showSaveRestore`).
                    lines.push("세이브백업있음");
                }
                log::error!("worker died; showing the stop notice (vault backup: {vault})");
                self.dead = Some(lines);
                self.request_redraw();
            }
            UserEvent::QuickPage(page) => {
                if self.quick_page != page {
                    self.quick_page = page;
                    self.request_redraw();
                }
            }
            UserEvent::SkillCast { slot } => {
                // The worker confirmed the cast actually started: NOW the cooldown
                // begins, for this skill's own length from wie_cooldowns.txt.
                //
                // ★ A skill NOT in the file has NO cooldown (2026-08-07, player:
                // "미기재 스킬은 스킬쿨타임이 없는거야. 0으로 해야해"). Safe by the
                // same logic that gates this event: the game's own cast delay
                // (선딜레이) already refuses a re-tap while the animation runs, and
                // a refused tap never confirms — so 0 cannot enable real spamming.
                if let Some(crate::IconRef::Skill(sheet, cell)) =
                    self.quickbar.iter().find(|s| s.slot == slot).and_then(|s| s.icon)
                {
                    let dur = self.skill_cds_cfg.get(&(sheet, cell)).copied().unwrap_or(0);
                    if dur > 0 {
                        self.skill_cd.insert((sheet, cell), (Instant::now(), dur));
                        self.request_redraw();
                    }
                }
            }
            UserEvent::Hud { field, cutscene, choice, map_open, shell, bottom_overlay, can_save, slots, stats, buff_lines } => {
                let mode = match (field, cutscene) {
                    (true, _) => overlay::Mode::Field,
                    (_, true) => overlay::Mode::Cutscene,
                    _ => overlay::Mode::Menu,
                };
                let choice_changed = choice != self.choice;
                self.choice = choice;
                let mut changed = slots != self.quickbar
                    || stats != self.stats
                    || map_open != self.map_open
                    || shell != self.shell
                    || bottom_overlay != self.bottom_overlay
                    || can_save != self.can_save
                    || buff_lines != self.buff_lines;
                self.buff_lines = buff_lines;
                self.map_open = map_open;
                self.shell = shell;
                self.bottom_overlay = bottom_overlay;
                self.can_save = can_save;
                // ★ The choice flag changes the BUTTON SET (the arrows appear for it),
                // so it needs the same relayout the mode does — a redraw alone would
                // paint the old layout. Folded into the mode branch below by `||`.
                if mode != overlay::Mode::Field && self.world_open {
                    self.hold_world(false);
                    changed = true;
                }
                if self.mode != mode || choice_changed {
                    // Field <-> game-window <-> cutscene: the whole control set
                    // changes, and so does which touch controls exist at all.
                    self.mode = mode;
                    // The pull-out keypad is a transient thing you open for one key.
                    // It used to STAY open forever (nothing ever cleared the flag), so
                    // it silently reappeared the next time any game window opened —
                    // with its '*' cell sitting in the left half of the screen, i.e.
                    // under a steering thumb. Close it whenever the mode changes.
                    self.keypad_open = false;
                    self.relayout();
                    // ★ The stick zone is FIELD-only (2026-08-17), so leaving the field
                    // mid-drag would otherwise leave a held direction with nothing on
                    // screen to explain it — and `joy_end` is what releases that key.
                    if self.layout.joystick.is_none() {
                        self.joy_park();
                    }
                    changed = true;
                }
                self.stats = stats;
                if slots != self.quickbar {
                    self.quickbar = slots;
                    // A shrunk list can orphan a held press index; clear it.
                    if self.quick_press.is_some_and(|(_, i)| i >= self.quickbar.len()) {
                        self.quick_press = None;
                    }
                    self.relayout_quick();
                }
                if changed {
                    self.request_redraw();
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.world_open {
            self.world_blink_at = None;
            return;
        }
        let now = Instant::now();
        if self.world_blink_at.is_none_or(|t| now >= t) {
            // One animation frame: the camera halves its remaining distance (hero5),
            // the pin bounces for ~0.4 s after a selection, and otherwise only the
            // stud's pulse needs a redraw, at a lazier pace.
            let moving = self.world_cam != self.world_target && !overlay::wm_glide(&mut self.world_cam, self.world_target);
            let bouncing = self.world_sel_at.elapsed() < Duration::from_millis(420);
            let step = if moving || bouncing { 33 } else { 120 };
            self.world_blink_at = Some(now + Duration::from_millis(step));
            self.request_redraw();
        }
        if let Some(t) = self.world_blink_at {
            event_loop.set_control_flow(ControlFlow::WaitUntil(t));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => {
                self.win_w = (new_size.width.max(1) / self.render_div).max(1);
                self.win_h = (new_size.height.max(1) / self.render_div).max(1);
                log::info!("resized: {}x{} -> render {}x{}", new_size.width, new_size.height, self.win_w, self.win_h);
                // Every in-flight gesture anchor is in the OLD coordinate space.
                self.clear_editor_gesture();
                self.relayout();
                self.configure_surface();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            // Hardware keyboards and D-pad-only devices (58차). Gamepad-source
            // events normally never arrive here — `MainActivity.dispatchKeyEvent`
            // consumes them into the JNI route — but winit's `Unidentified` Android
            // scancodes go through the same button table as belt-and-braces.
            WindowEvent::KeyboardInput { event, .. } => {
                // ★ 패드 버튼 설정: a pad button that came the winit way (belt-and-braces —
                // the Java hooks normally take them) is an answer too; a keyboard key is not
                // a pad button and is swallowed.
                if self.padmap.is_some() {
                    if let winit::keyboard::PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Android(sc)) = event.physical_key {
                        self.padmap_button(sc as i32, event.state.is_pressed(), event.repeat);
                    }
                    return;
                }
                if let Some(action) = crate::gamepad::winit_key_action(&event.physical_key) {
                    // A hardware keyboard / TV remote is not a pad, but it is not a
                    // TOUCH either: it wants the same "no on-screen walk controls" HUD,
                    // and leaving the source as touch would draw arrows nobody can reach.
                    let _ = self.note_input(true);
                    self.gamepad_action(action, event.state.is_pressed(), event.repeat);
                }
            }
            // Lost/gained foreground focus. This catches screen-lock (and other
            // focus-stealing overlays) even when the surface is NOT destroyed, so
            // it complements suspended()/resumed(): either signal halts audio.
            WindowEvent::Focused(focused) => {
                self.set_paused(!focused);
                if !focused {
                    // Also release any held game keys so nothing sticks on return.
                    self.release_gamepad();
                    for key in self.pressed.drain() {
                        let _ = self.input_tx.send(InputEvent::Key { code: key, down: false });
                    }
                    self.touches.clear();
                    self.popup_confirm_touches.clear();
                    self.ui_touches.clear();
                    self.slide_touches.clear();
                    self.mm_tap = None;
                    self.stats_tap = None;
                    self.joy = None;
                    self.joy_parked = None;
                    self.quick_press = None;
                    self.clear_editor_gesture();
                }
            }
            WindowEvent::Touch(Touch { phase, location, id, .. }) => {
                // Physical -> render space (HEAT SAVER divisor; 1 = identity).
                let s = self.render_div as f64;
                let (x, y) = (location.x / s, location.y / s);
                // ★ The editor is the FIRST branch of every phase. It has to sit above
                // `map_open` (which the worker can set on its own and which swallows
                // any tap into a CLEAR) and above `show_settings` (whose "tap outside
                // the panel closes it" rule would end the editor on the first drag).
                // ★ 패드 버튼 설정: modal — every touch is the screen's, and only a touch going
                // DOWN means anything (nothing on it moves or scrolls). ⛔ Above `note_input`:
                // picking (touch) and assigning (pad) alternate here, so the AUTO HUD must not
                // flip — and swallow a tap — under them. The tap that closes the screen is
                // recorded instead (`padmap_close`), or the next tap on the settings window
                // would be eaten (S9, 2026-10-09).
                if self.padmap.is_some() {
                    if phase == TouchPhase::Started {
                        self.padmap_started(x, y);
                    }
                    return;
                }
                if self.world_open {
                    if phase == TouchPhase::Started {
                        if self.map_tab_zone().is_some_and(|r| overlay::rect_hit(&overlay::top_tab_hit(r, self.win_w, self.win_h), x, y)) {
                            self.hold_world(false);
                        } else if let Some(key) = overlay::menu_dpad_hit(self.win_w, self.win_h, x, y)
                            && let Some(dir) = Self::key_dir(key)
                        {
                            self.step_world(dir);
                        } else {
                            // hero5: a place is touched straight on the picture (the
                            // stud or its name), the back button closes, the corner box
                            // toggles the minimap. Off the popup closes it too.
                            match overlay::worldmap_hit(self.win_w, self.win_h, self.world_cam, x, y) {
                                overlay::WmHit::Back | overlay::WmHit::Outside => self.hold_world(false),
                                overlay::WmHit::MiniToggle => self.minimap_on = !self.minimap_on,
                                overlay::WmHit::Place(i) => self.select_world(i),
                                overlay::WmHit::Panel => {}
                            }
                        }
                        self.ui_touches.insert(id);
                        self.request_redraw();
                    } else if phase == TouchPhase::Ended || phase == TouchPhase::Cancelled {
                        self.ui_touches.remove(&id);
                    }
                    return;
                }
                if self.editor.is_some() {
                    match phase {
                        TouchPhase::Started => self.editor_started(id, x, y),
                        TouchPhase::Moved => self.editor_moved(id, x, y),
                        TouchPhase::Ended | TouchPhase::Cancelled => self.editor_ended(id),
                    }
                    return;
                }
                if phase == TouchPhase::Started && self.note_input(false) {
                    // ★ AUTO: the last input source wins, so a finger on the glass brings
                    // the touch HUD (arrows, stick) back — and THAT touch does nothing
                    // else. Swallowing it is the point: the controls it would be
                    // hit-tested against did not exist when the finger came down, so
                    // dispatching it could walk the character or open the map by
                    // "accident" from the player's side. `ui_touches` is how every other
                    // consumed-by-the-UI touch is marked, so its keyup is dropped too.
                    self.ui_touches.insert(id);
                    return;
                }
                match phase {
                    TouchPhase::Started => {
                        if self.map_open {
                            // Enlarged map is up: ANY tap closes it. The game's map
                            // window closes on any key (guest 0x124360), so one CLEAR
                            // tap dismisses both it and our overlay copy.
                            let _ = self.input_tx.send(InputEvent::Key { code: KeyCode::CLEAR, down: true });
                            let _ = self.input_tx.send(InputEvent::Key { code: KeyCode::CLEAR, down: false });
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if self.show_settings {
                            // ★ Inside the row list the press may be the start of a scroll
                            //   (2026-09-24), so it is decided on release. 닫기 and the
                            //   outside-the-panel close stay instant.
                            let in_list = overlay::rect_hit(&overlay::settings_list_view(self.win_w, self.win_h), x, y)
                                && overlay::settings_hit(&self.settings_items, self.win_w, self.win_h, x, y) != Some(overlay::SettingsAction::Close);
                            if in_list {
                                self.settings_drag = Some((id, x, y, self.settings_scroll, false));
                            } else if let Some(action) = overlay::settings_hit(&self.settings_items, self.win_w, self.win_h, x, y) {
                                self.handle_settings(action);
                            }
                            self.ui_touches.insert(id);
                        } else if overlay::gear_hit(&self.layout.gear, x, y) {
                            self.show_settings = true;
                            // The list opens at its top, where the five familiar rows are.
                            self.settings_scroll = 0;
                            self.settings_drag = None;
                            self.settings_items = overlay::settings_items(self.win_w, self.win_h, 0);
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if overlay::rect_hit(&self.layout.keypad_toggle, x, y) {
                            // Pull-out full keypad: the only way to reach the keys
                            // immersive mode has no permanent seat for (2/4/5/6/8/0/
                            // */LSK/RSK).
                            self.keypad_open = !self.keypad_open;
                            self.relayout();
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if let Some((fx, fy)) = self.revive_prompt_hit(x, y) {
                            // ★ 부활의 구 (2026-09-25): a touch inside the box is the
                            //   worker's (`Revive::tap`) whatever the mode — ahead of every
                            //   control that could sit under it (a plate, the floating
                            //   stick's zone). Outside it OK and the stick answer as keys.
                            let _ = self.input_tx.send(InputEvent::MenuTap { fx, fy });
                            self.ui_touches.insert(id);
                        } else if self
                            .minimap_restore_zone()
                            .is_some_and(|r| overlay::rect_hit(&overlay::minimap_restore_hit(r, self.win_w, self.win_h), x, y))
                        {
                            // ★ 2026-10-21: the hidden panel's restore button brings it back.
                            //   Tested BEFORE the plates (a control dragged on top of it must not
                            //   make the minimap unrecoverable), live only while it is drawn.
                            self.minimap_on = true;
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if self
                            .map_tab_zone()
                            .is_some_and(|r| overlay::rect_hit(&overlay::top_tab_hit(r, self.win_w, self.win_h), x, y))
                        {
                            // The MAP tab opens the world map on the region the character is
                            // in (hero5: the window starts there, the arrows slide it). It
                            // used to flip the corner minimap; that panel stays as it is.
                            self.toggle_worldmap();
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if (self.mode == overlay::Mode::Field || self.picking_slot)
                            && placement::rect_of(placement::CtrlId::QuickPage, self.mode, self.win_w, self.win_h)
                                .is_some_and(|r| overlay::rect_hit(&r, x, y))
                        {
                            // The worker owns the flip end to end (it needs guest memory for
                            // the capture and the re-bind), so this only reports the tap.
                            //
                            // ★ Tested BEFORE the plates, and while PICKING too: flipping mid-
                            // registration is what lets the player put a skill on page 2 without
                            // closing the popup. Picking survives the flip on purpose — the
                            // popup is untouched, so the pending 퀵바등록 is still pending.
                            let _ = self.input_tx.send(InputEvent::QuickPageNext);
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if self
                            .quicksave_zone()
                            .is_some_and(|r| overlay::rect_hit(&overlay::top_tab_hit(r, self.win_w, self.win_h), x, y))
                        {
                            // Same one-way report as the page button: the worker arms the save
                            // chain, checks the guest's own gate and answers with a toast.
                            // ★ The target is bigger than the SAVE tab drawn (`top_tab_hit`,
                            // 2026-10-01).
                            let _ = self.input_tx.send(InputEvent::QuickSave);
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if let Some(i) = overlay::quick_hit(&self.quick_rects, x, y) {
                            // ★ 2026-09-01 — the HOST plate, tested before everything
                            // else. The bar is one plate wider than the game's own bar
                            // (`crate::QUICK_SLOTS` vs `GUEST_QUICK_SLOTS`), and that
                            // last plate has no key to send AND cannot be registered by
                            // the game's own 퀵바등록 popup, which offers exactly four
                            // rows (`t/gm.tdf[34..37]`). Until the host can bind it
                            // itself (순서표 #12) it is inert — drawn dim, and it says so
                            // rather than eating the tap in silence, which is the agreed
                            // interim behaviour (사용자 결정: "비활성 (회색)").
                            // (hero2, 2026-09-17: no plate is host-inert any more — the plate
                            // window seats any of the seven; `quickslot_inert` answers false.)
                            if self.quickbar.get(i).is_some_and(crate::quickslot_inert) {
                                self.host_notice(crate::HOST_SLOT_TOAST);
                            }
                            // ★ While the plates are HIGHLIGHTED the tap means "register
                            // here", not "use this" — that is the whole point of the
                            // 퀵바등록 flow. The worker re-validates and lets the GAME do
                            // the binding; see `InputEvent::PopupBindSlot`.
                            else if self.picking_slot {
                                let slot = self.quickbar.get(i).map_or(i as u8, |s| s.slot);
                                let _ = self.input_tx.send(InputEvent::PopupBindSlot { slot });
                            } else if crate::guest_build::hero2_java().is_some() {
                                // ★★ hero2 (2026-09-17, plate window): on the FIELD every
                                // plate goes to the worker as a PLATE — it decides the guest
                                // slot (or seats one) and presses the hotkey. With a game
                                // window open a plate does nothing: hero2's windows are
                                // driven by 2/4/6/8/5/CLR, and the four slot digits mean
                                // nothing there (hero3 needed them for its «1»«3»«7»«9»
                                // tab badges; hero2 has no such badges).
                                if self.mode == overlay::Mode::Field
                                    && let Some(slot) = self.quickbar.get(i).copied()
                                    && !matches!(slot.kind, SlotKind::Empty)
                                {
                                    // A running cooldown swallows the tap (the game's own
                                    // counter would refuse the cast anyway — `bd.a(int)`).
                                    let blocked = slot.cooldown.is_some_and(|(left, total)| left > 0 && total > 0);
                                    if !blocked {
                                        let _ = self.input_tx.send(InputEvent::QuickActivate { plate: slot.slot });
                                    }
                                }
                            } else if let Some(slot) = self.quickbar.get(i).copied()
                                && let Some((code, _)) = quickslot_hotkey(slot.slot)
                            {
                                // ★ Skill cooldown (2026-08-05, host-owned; reworked
                                // 2026-08-07): a skill plate whose cooldown is still
                                // running swallows the tap instead of casting again; the
                                // plate's bottom-up fill shows the recovery. Field only —
                                // with a game window open the same digits navigate the
                                // window, which must never be blocked. Items are exempt
                                // (potions have no cooldown).
                                let is_skill = matches!(slot.kind, SlotKind::Skill);
                                let on_field = self.mode == overlay::Mode::Field;
                                let blocked =
                                    is_skill && on_field && self.cooldown_frac(slot.slot).is_some();
                                if !blocked {
                                    if is_skill && on_field {
                                        // ★ The WORKER sends the hotkey and then watches
                                        // guest state to see whether the cast actually
                                        // started (the game's own gates can swallow it:
                                        // not idle, no SP, out of range). Only a confirmed
                                        // cast comes back as `UserEvent::SkillCast`, and
                                        // THAT is what starts the cooldown — a tap that
                                        // cast nothing costs nothing (2026-08-07:
                                        // "스킬이 실행 됐을때만 쿨타임이 돌아야해").
                                        let _ = self.input_tx.send(InputEvent::SkillCast { slot: slot.slot });
                                    } else {
                                        // One-shot: send the slot's fixed hotkey (down+up) so
                                        // the game natively activates it (potion use / window
                                        // navigation). Pure key event — no memory injection
                                        // or guest call.
                                        //
                                        // ★ Not gated to the field any more (2026-08-03): the
                                        // plates stay on screen with a game window open, where
                                        // the same digits navigate the window. `quick_rects` is
                                        // empty during a cutscene, and it is rebuilt from the
                                        // mode the slot list arrived with, so it can never be a
                                        // stale rect from the other mode.
                                        let _ = self.input_tx.send(InputEvent::Key { code, down: true });
                                        let _ = self.input_tx.send(InputEvent::Key { code, down: false });
                                    }
                                }
                            }
                            self.quick_press = Some((id, i));
                            self.ui_touches.insert(id);
                            self.request_redraw();
                        } else if self.minimap_tap_zone().is_some_and(|p| overlay::rect_hit(&p, x, y)) {
                            // Minimap panel: a DELIBERATE TAP opens the map (that is
                            // also what makes the guest repaint the buffer we mirror).
                            // Nothing is sent yet — see TouchPhase::Ended.
                            self.mm_tap = Some((id, x, y));
                            self.ui_touches.insert(id);
                        } else if self.stats_tap_zone().is_some_and(|p| overlay::rect_hit(&p, x, y)) {
                            // Lv/HP block: a DELIBERATE TAP opens the main menu
                            // (`CLEAR`). Nothing is sent yet — see TouchPhase::Ended.
                            self.stats_tap = Some((id, x, y));
                            self.ui_touches.insert(id);
                        } else if let Some(key) = overlay::hit(&self.layout.buttons, x, y) {
                            // ★ The TOUCH TYPE B "stick before the buttons" special
                            // case that used to sit above this arm is GONE (2026-08-16
                            // second round): B has no stick at all any more — the
                            // field D-pad it walks with is an ordinary button set, so
                            // the plain order below covers every type.
                            self.slide_touches.insert(id);
                            self.update_touch(id, Some(key));
                        } else if self.joy.is_none() && overlay::in_joystick_zone(&self.layout, x, y) {
                            self.joy_start(id, x, y);
                        } else if self.mode == overlay::Mode::Menu
                            && !self.picking_slot
                            && self.bag_page.is_some_and(|v| v.on_top)
                            && let Some(page) = overlay::bag_tab_hit(&self.bag_tab_rects(), x, y)
                        {
                            // ★ #13: a page tab beside the inventory window. The worker
                            // owns the page (it writes the guest context); the UI only
                            // reports which tab. Before the window router: the strip sits in
                            // the margin the game does not draw on, but a tap there would
                            // otherwise be "nothing here" and swallowed.
                            let _ = self.input_tx.send(InputEvent::BagPage(page));
                            self.ui_touches.insert(id);
                        } else if self.mode == overlay::Mode::Menu
                            && !self.picking_slot
                            && crate::guest_build::hero2_java().is_some()
                            && let Some((fx, fy)) = self.fb_point(x, y)
                        {
                            // ★ hero2 touch router (2026-09-17): a tap on the game's own
                            // window. LAST arm on purpose — every host control above keeps
                            // its meaning; what is left is the panel the game drew. The
                            // worker owns the decision (`InputEvent::MenuTap`).
                            let _ = self.input_tx.send(InputEvent::MenuTap { fx, fy });
                            self.ui_touches.insert(id);
                        } else if let Some(key) = overlay::tap_anywhere_key(self.mode) {
                            // ★ CUTSCENE: the whole screen is OK (2026-08-17). LAST arm
                            // on purpose — every control above it, the `#` skip included,
                            // keeps its own meaning, and what is left is the scene itself.
                            //
                            // NOT added to `slide_touches`: that set exists so a finger can
                            // slide between D-pad arms, and re-running the button hit-test
                            // on every move would release this OK the moment the finger
                            // drifted. Held until the touch ends, like a button press.
                            self.update_touch(id, Some(key));
                        }
                    }
                    TouchPhase::Moved if self.settings_drag.is_some_and(|d| d.0 == id) => {
                        // The settings list follows the finger once it has moved past the
                        // tap slop; before that it is still a tap.
                        let Some((_, dx, dy, from, moved)) = self.settings_drag else { return };
                        if moved || !is_tap(dx, dy, x, y) {
                            let max = overlay::settings_scroll_max(self.win_w, self.win_h);
                            let to = (f64::from(from) - (y - dy)).clamp(0.0, f64::from(max)) as u32;
                            self.settings_drag = Some((id, dx, dy, from, true));
                            if to != self.settings_scroll {
                                self.settings_scroll = to;
                                self.settings_items = overlay::settings_items(self.win_w, self.win_h, to);
                                self.request_redraw();
                            }
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled if self.settings_drag.is_some_and(|d| d.0 == id) => {
                        let Some((_, dx, dy, _, moved)) = self.settings_drag.take() else { return };
                        self.ui_touches.remove(&id);
                        if moved {
                            // Settle on a whole row, so the list at rest never shows a
                            // plate cut in half.
                            let pitch = overlay::settings_row_pitch(self.win_w, self.win_h).max(1);
                            let max = overlay::settings_scroll_max(self.win_w, self.win_h);
                            self.settings_scroll = ((self.settings_scroll + pitch / 2) / pitch * pitch).min(max);
                            self.settings_items = overlay::settings_items(self.win_w, self.win_h, self.settings_scroll);
                            self.request_redraw();
                        } else if phase == TouchPhase::Ended
                            && let Some(action) = overlay::settings_hit(&self.settings_items, self.win_w, self.win_h, dx, dy)
                        {
                            self.handle_settings(action);
                        }
                    }
                    TouchPhase::Moved => {
                        // ★ The field came back while this finger was still down: re-form
                        //   the stick under it (at its CURRENT position — the thumb has
                        //   moved since, and the landing point is meaningless now) instead
                        //   of making the player lift and press again. See `joy_parked`.
                        if self.joy_parked == Some(id) && self.layout.joystick.is_some() && overlay::in_joystick_zone(&self.layout, x, y) {
                            // info, not debug: the app installs `warn,main=info`, so a
                            // `debug!` here can never reach logcat in ANY build — and this
                            // line is the only evidence that the re-adoption happened at
                            // all (it is invisible on screen: the stick simply keeps
                            // working). It fires once per parked finger, at human rates.
                            log::info!("joystick: re-adopting the finger the mode change parked");
                            self.joy_start(id, x, y);
                            self.request_redraw();
                        } else if self.joy.as_ref().is_some_and(|j| j.id == id) {
                            self.joy_update(x, y, true);
                            self.request_redraw();
                        } else if self.mm_tap.is_some_and(|(tid, dx, dy)| tid == id && !is_tap(dx, dy, x, y)) {
                            // Smeared into a drag: not a tap, so it will not open the map.
                            self.mm_tap = None;
                        } else if self.stats_tap.is_some_and(|(tid, dx, dy)| tid == id && !is_tap(dx, dy, x, y)) {
                            self.stats_tap = None;
                        } else if self.slide_touches.contains(&id) {
                            // Sliding between adjacent keys is legitimate (the D-pad
                            // depends on it) — but ONLY for a touch that began on a
                            // control. A stray finger can no longer pick up keys.
                            let key = overlay::hit(&self.layout.buttons, x, y);
                            self.update_touch(id, key);
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.slide_touches.remove(&id);
                        // A real tap that both started AND ended inside the panel HIDES the
                        // minimap (player request 2026-08-04: "미니맵을 클릭하면 미니맵이
                        // 사라지면서 미니맵 버튼으로 나타나게"). It used to send `*` to open
                        // the guest's own map window, which is the feature that got
                        // removed along with the centred blow-up.
                        //
                        // The tap discipline is kept exactly as it was, and it matters more
                        // now: the panel is large, so without "started and ended inside,
                        // and not a drag" a thumb sliding across the corner would make the
                        // minimap vanish mid-fight. `Cancelled` still never counts.
                        if let Some((tid, dx, dy)) = self.mm_tap
                            && tid == id
                        {
                            self.mm_tap = None;
                            if phase == TouchPhase::Ended
                                && is_tap(dx, dy, x, y)
                                && self.minimap_tap_zone().is_some_and(|p| overlay::rect_hit(&p, x, y))
                            {
                                self.minimap_on = false;
                                self.request_redraw();
                            }
                        }
                        // Lv/HP block -> main menu. `CLEAR` is the game's own
                        // open-menu key on the field (device-probed, CUSTOM_HUD.md),
                        // so this is the CLR button's key with a bigger target.
                        if let Some((tid, dx, dy)) = self.stats_tap
                            && tid == id
                        {
                            self.stats_tap = None;
                            if phase == TouchPhase::Ended
                                && is_tap(dx, dy, x, y)
                                && self.stats_tap_zone().is_some_and(|p| overlay::rect_hit(&p, x, y))
                            {
                                let _ = self.input_tx.send(InputEvent::Key { code: KeyCode::CLEAR, down: true });
                                let _ = self.input_tx.send(InputEvent::Key { code: KeyCode::CLEAR, down: false });
                            }
                        }
                        let was_quick = self.quick_press.map(|(pid, _)| pid) == Some(id);
                        if was_quick {
                            self.quick_press = None;
                        }
                        if self.joy_parked == Some(id) {
                            // Parked by a mode change and now lifted for real: nothing is
                            // held (the park released it), just forget the finger.
                            self.joy_parked = None;
                        }
                        if self.joy.as_ref().is_some_and(|j| j.id == id) {
                            self.joy_end();
                            self.request_redraw();
                        } else if !self.ui_touches.remove(&id) {
                            self.update_touch(id, None);
                        } else if was_quick {
                            self.request_redraw();
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
