// hero2's WORLD MAP — the window the MAP tab opens (2026-10-19, hero5-shaped
// 2026-10-20).
//
// The shape is hero5's world map (StateMap::DrawWorldMap @0x126cec in
// libHeroesLore5.so, read 2026-10-20): the painted continent at its own
// resolution behind a camera window, a black band with the title plate across
// the top, a dark vignette along the four edges of the picture, a small grey
// stud on every place with its name in a half-transparent box underneath, and a
// pin plus a glowing stud on the place the cursor is on. Touching a place, or an
// arrow, moves the cursor there and the camera GLIDES after it (hero5 halves the
// remaining distance every frame — `wm_glide`). Nothing here is written to a
// save.
//
// What differs from hero5 on purpose: it is a popup over the dimmed field rather
// than a full screen (user, 2026-10-20).
//
// ★ 2026-10-20 (2), user: "현재 너무 많은 맵이 보이고있어 hero5처럼 어느정도만
// 보이게 … 맵간에 직선으로 연결되고 그 직선에 이름이 가려지지 않게 … 반란군캠프에서
// 위나 왼쪽을 누르면 실바나르 남부로 가야하고 오른쪽이나 아래를 눌렀을땐 아무대로도
// 이동이 되면 안돼". So, like hero5's zone map: the camera shows two fifths of the
// picture (the place, its neighbours, a few beyond — not the whole continent),
// every road the character can actually walk is a straight line between its two
// places (`WM_ROADS` = the maps' own `39 <map>` step warps, collapsed to places —
// `tools_re/worldmap_roads.py` derives the table from the asset and refuses a
// drift), the names are drawn OVER the lines, and an arrow follows the roads
// only: the neighbour that lies that way (a positive component in that
// direction, the smallest angle wins) or nothing at all. The first draft's
// "nearest place anywhere in that direction" let an arrow jump to a place with
// no road to it (반란군 캠프 → UP went to 어둠동굴).
//
// Dependency-free: `worldmap_view_selftest.rs` compiles it with `include!`.

/// One named place on the picture. `x` and `y` are thousandths of the picture
/// (0 = left/top, 1000 = right/bottom): the camera centres here, the stud is
/// drawn here, and the name sits under it.
#[derive(Clone, Copy)]
pub struct WmPlace {
    pub name: &'static str,
    pub x: u16,
    pub y: u16,
}

pub const WM_WINTER: usize = 0;
pub const WM_WINTER_SOUTH: usize = 1;
pub const WM_NORDOST: usize = 2;
pub const WM_NORDEN: usize = 3;
pub const WM_CARDIFF_N: usize = 4;
pub const WM_CARDIFF_S: usize = 5;
pub const WM_SILVA_S: usize = 6;
pub const WM_CRYPT: usize = 7;
pub const WM_SILVA: usize = 8;
pub const WM_ROAD_S: usize = 9;
pub const WM_CAVE: usize = 10;
pub const WM_ROAD_N: usize = 11;
pub const WM_FROST: usize = 12;
pub const WM_TOMB: usize = 13;
pub const WM_MULLAK: usize = 14;
pub const WM_SYNDRAF: usize = 15;
pub const WM_ISLAND: usize = 16;
pub const WM_TEMPLE: usize = 17;
pub const WM_TEMPLE_UNDER: usize = 18;
pub const WM_REID: usize = 19;
pub const WM_UNDER: usize = 20;
pub const WM_SOUTH2: usize = 21;
pub const WM_THIEF: usize = 22;
pub const WM_NORDEN_S: usize = 23;
pub const WM_TUNNEL: usize = 24;
pub const WM_LAB: usize = 25;
pub const WM_CAMP: usize = 26;
pub const WM_GORGE: usize = 27;
pub const WM_NORDOST_N: usize = 28;

/// Places sit on the painted landmarks of `docs/worldmap/worldmap_detailed.png`.
/// The legacy drawing table in `tools_re/worldmap_draw.py` uses the same coordinates.
/// Names are the game's own map names (`P/m/m/NN.m.gz` headers, 2026-10-20):
/// 82 · 83 are 서리골짜기 남부 · 북부 (the first draft called them 남부 길 · 북부
/// 길), 18 is 윈터하츠 남부1, 98 · 99 are 카디프숲 북부 · 남부.
pub const WM_PLACES: &[WmPlace] = &[
    WmPlace { name: "윈터하츠", x: 480, y: 520 },
    WmPlace { name: "윈터하츠 남부1", x: 480, y: 620 },
    WmPlace { name: "노르도스트 평원", x: 660, y: 550 },
    WmPlace { name: "노르덴항", x: 860, y: 510 },
    WmPlace { name: "카디프숲 북부", x: 480, y: 820 },
    WmPlace { name: "카디프숲 남부", x: 460, y: 940 },
    WmPlace { name: "실바나르 남부", x: 250, y: 800 },
    WmPlace { name: "납골당", x: 145, y: 815 },
    WmPlace { name: "실바나르", x: 250, y: 620 },
    WmPlace { name: "서리골짜기 남부", x: 250, y: 470 },
    WmPlace { name: "어둠동굴", x: 315, y: 480 },
    WmPlace { name: "서리골짜기 북부", x: 200, y: 330 },
    WmPlace { name: "서리골짜기", x: 110, y: 290 },
    WmPlace { name: "예언자의 무덤", x: 120, y: 160 },
    WmPlace { name: "멀락협곡", x: 710, y: 300 },
    WmPlace { name: "신드라프", x: 880, y: 170 },
    WmPlace { name: "버려진 섬", x: 950, y: 805 },
    WmPlace { name: "신전", x: 480, y: 320 },
    WmPlace { name: "신전지하", x: 545, y: 270 },
    WmPlace { name: "리이드 지하", x: 400, y: 440 },
    WmPlace { name: "언더시티", x: 560, y: 440 },
    WmPlace { name: "윈터하츠 남부2", x: 480, y: 720 },
    WmPlace { name: "도적굴", x: 600, y: 740 },
    WmPlace { name: "노르덴 남부", x: 800, y: 710 },
    WmPlace { name: "해저터널", x: 880, y: 680 },
    WmPlace { name: "지하연구소", x: 965, y: 925 },
    WmPlace { name: "반란군 캠프", x: 380, y: 840 },
    // 협곡지하 (maps 95–97) opens from both 멀락협곡 남부 and 노르도스트 북부 평원.
    // The north plain is its own place so each of those exits still has an arrow.
    WmPlace { name: "협곡지하", x: 786, y: 317 },
    WmPlace { name: "노르도스트 북부", x: 720, y: 440 },
];

/// The roads: every pair of places the character can WALK between — the maps'
/// step-trigger warps (`39 <map>`, record kind 3) collapsed through
/// `wm_place_index`, both ways. Cutscene warps (kind 0: 신드라프 → 윈터하츠 after
/// the boss, 납골당 → 신전, 신전지하 ↔ 윈터하츠/언더시티) are not roads. The lines
/// on the picture and the arrows both come from this table alone.
/// `tools_re/worldmap_roads.py <hero2_ktf.zip>` recomputes it from the asset and
/// exits 1 when it differs.
pub const WM_ROADS: &[(usize, usize)] = &[
    (WM_WINTER, WM_WINTER_SOUTH),
    (WM_WINTER, WM_TEMPLE),
    (WM_WINTER, WM_REID),
    (WM_WINTER, WM_UNDER),
    (WM_WINTER_SOUTH, WM_NORDOST),
    (WM_WINTER_SOUTH, WM_SOUTH2),
    (WM_NORDOST, WM_NORDOST_N),
    (WM_NORDOST, WM_THIEF),
    (WM_NORDOST, WM_NORDEN_S),
    (WM_NORDEN, WM_NORDEN_S),
    (WM_CARDIFF_N, WM_CARDIFF_S),
    (WM_CARDIFF_N, WM_SOUTH2),
    (WM_CARDIFF_S, WM_SILVA_S),
    (WM_SILVA_S, WM_CRYPT),
    (WM_SILVA_S, WM_SILVA),
    (WM_SILVA_S, WM_CAMP),
    (WM_SILVA, WM_ROAD_S),
    (WM_ROAD_S, WM_CAVE),
    (WM_ROAD_S, WM_ROAD_N),
    (WM_ROAD_N, WM_FROST),
    (WM_FROST, WM_TOMB),
    (WM_MULLAK, WM_SYNDRAF),
    (WM_MULLAK, WM_UNDER),
    (WM_MULLAK, WM_GORGE),
    (WM_NORDOST_N, WM_MULLAK),
    (WM_NORDOST_N, WM_GORGE),
    (WM_ISLAND, WM_TUNNEL),
    (WM_ISLAND, WM_LAB),
    (WM_TEMPLE, WM_TEMPLE_UNDER),
    (WM_SOUTH2, WM_THIEF),
    (WM_NORDEN_S, WM_TUNNEL),
];

/// The places a road leads to from `from`.
pub fn wm_neighbours(from: usize) -> impl Iterator<Item = usize> {
    WM_ROADS.iter().filter_map(move |&(a, b)| if a == from { Some(b) } else if b == from { Some(a) } else { None })
}

/// Which place a live map number is. Indoors and dungeon floors use the outdoor
/// place you enter them from; a number with no row is 윈터하츠.
#[cfg_attr(not(test), allow(dead_code))]
pub fn wm_place(map: u32) -> &'static WmPlace {
    &WM_PLACES[wm_place_index(map)]
}

pub fn wm_place_index(map: u32) -> usize {
    match map {
        15 | 112 | 113 | 114 | 115 | 116 | 117 | 118 | 119 | 120 | 121 => WM_TEMPLE,
        58 | 59 | 60 | 62 | 106 | 108 | 109 => WM_TEMPLE_UNDER,
        16 | 29 | 30 | 31 | 33 | 103 => WM_REID,
        34 | 35 | 36 | 37 | 38 | 39 => WM_UNDER,
        0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 17 => WM_WINTER,
        18 | 19 => WM_WINTER_SOUTH,
        20 => WM_SOUTH2,
        89 | 90 | 91 | 101 => WM_THIEF,
        25 => WM_NORDOST_N,
        26 => WM_NORDOST,
        27 | 70 => WM_NORDEN_S,
        67 | 68 | 69 | 71 => WM_NORDEN,
        84 | 85 | 86 | 87 | 88 | 102 => WM_TUNNEL,
        28 => WM_ISLAND,
        40 | 41 | 42 | 43 | 44 | 107 => WM_LAB,
        98 => WM_CARDIFF_N,
        99 => WM_CARDIFF_S,
        24 => WM_SILVA_S,
        100 => WM_CAMP,
        46 | 47 | 48 | 49 | 50 | 104 => WM_CRYPT,
        64 | 65 | 66 => WM_SILVA,
        82 => WM_ROAD_S,
        92 | 93 | 94 => WM_CAVE,
        83 => WM_ROAD_N,
        76 | 77 | 78 => WM_FROST,
        52 | 53 | 54 | 55 | 56 | 105 => WM_TOMB,
        80 | 81 => WM_MULLAK,
        95 | 96 | 97 => WM_GORGE,
        72 | 73 | 74 | 75 | 79 | 110 => WM_SYNDRAF,
        _ => WM_WINTER,
    }
}

// ── the popup ─────────────────────────────────────────────────────────────────

/// A rectangle in window pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WmBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl WmBox {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x as i32 && py >= self.y as i32 && px < (self.x + self.w) as i32 && py < (self.y + self.h) as i32
    }
}

/// Where everything sits: the chrome frame, the black title band inside it, the
/// picture window under the band, the round back button in the band's right end
/// and the 미니맵 toggle box in the picture's bottom-left corner (hero5 puts the
/// same four things in the same four corners). `unit` is one hero5 logical pixel
/// in window pixels (the picture window is 240 of them tall), so every stud, pin
/// and box keeps hero5's proportions at any resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WmPanel {
    pub frame: WmBox,
    pub band: WmBox,
    pub view: WmBox,
    pub back: WmBox,
    pub mini: WmBox,
    pub unit: u32,
}

/// Frame thickness of the chrome around the band and the picture (window px).
pub const WM_EDGE: u32 = 4;

pub fn wm_panel(win_w: u32, win_h: u32) -> WmPanel {
    if win_w == 0 || win_h == 0 {
        return WmPanel::default();
    }
    // A popup, not a full screen: 4 % in from the sides, and the top starts below
    // the hanging SAVE/MAP tabs (6.5 % of the height) so the MAP tab that opened it
    // stays visible and closes it again.
    let (fx, fw) = if win_w >= win_h { (win_w * 4 / 100, win_w - win_w * 8 / 100) } else { (win_w * 2 / 100, win_w - win_w * 4 / 100) };
    let fy = win_h * 65 / 1000;
    let fh = win_h.saturating_sub(fy + win_h * 4 / 100).max(1);
    let frame = WmBox { x: fx, y: fy, w: fw, h: fh };
    let inner_w = fw.saturating_sub(2 * WM_EDGE).max(1);
    let inner_h = fh.saturating_sub(2 * WM_EDGE).max(1);
    // One hero5 pixel: hero5's screen is 400 × 240, so the unit is whichever of
    // the picture window's height / 240 and width / 400 is smaller — a tall
    // window takes it from its width, or the studs and names would be giants.
    // hero5: a 22-px band over a 240-px screen → the band is 1/11 of the inside,
    // capped at 24 units so a tall window does not grow a tall band.
    let unit0 = ((inner_h + 120) / 240).min((inner_w + 200) / 400).max(1);
    let band_h = (inner_h / 11).min(24 * unit0).max(8);
    let view_h = inner_h.saturating_sub(band_h).max(1);
    let unit = ((view_h + 120) / 240).min((inner_w + 200) / 400).max(1);
    let band = WmBox { x: fx + WM_EDGE, y: fy + WM_EDGE, w: inner_w, h: band_h };
    let view = WmBox { x: fx + WM_EDGE, y: band.y + band_h, w: inner_w, h: view_h };
    // Back button: a circle the band's height, inset by half of it.
    let bd = band_h.saturating_sub(2).max(4);
    let back = WmBox { x: (band.x + band.w).saturating_sub(bd + band_h / 2), y: band.y + 1, w: bd, h: bd };
    // 미니맵 toggle: hero5's 17-px-high box in the bottom-left of the picture.
    let mh = (17 * unit).max(8).min(view_h);
    let mw = (72 * unit).max(16).min(inner_w);
    let mini = WmBox { x: view.x, y: (view.y + view.h).saturating_sub(mh), w: mw, h: mh };
    WmPanel { frame, band, view, back, mini, unit }
}

// ── the camera ────────────────────────────────────────────────────────────────

/// The window of the picture currently on screen, in picture pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WmCam {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// How much of the picture's width the camera shows, as a fraction. hero5 shows
/// 400 of its 480 px — but its picture holds a dozen islands, ours twenty-seven
/// places, and the user wants hero5's density ("어느정도만 보이게"): two fifths
/// puts 2–10 places in a landscape window (average six, like hero5's screen).
pub const WM_CAM_FRACTION: (u32, u32) = (2, 5);

/// Frame `place`: the camera centred on it, clamped inside the picture, showing
/// `WM_CAM_FRACTION` of the picture's width — the place, its neighbours and a
/// few beyond, with the names readable. A tall window gets the full height and
/// whatever width that leaves.
#[cfg_attr(not(test), allow(dead_code))]
pub fn wm_open(map: u32, img_w: u32, img_h: u32, view_w: u32, view_h: u32) -> WmCam {
    wm_open_at(wm_place_index(map), img_w, img_h, view_w, view_h)
}

pub fn wm_open_at(index: usize, img_w: u32, img_h: u32, view_w: u32, view_h: u32) -> WmCam {
    let place = &WM_PLACES[index];
    let (vw, vh) = wm_view_size(img_w, img_h, view_w, view_h);
    let ax = u32::from(place.x) * img_w / 1000;
    let ay = u32::from(place.y) * img_h / 1000;
    WmCam {
        x: ax.saturating_sub(vw / 2).min(img_w.saturating_sub(vw)),
        y: ay.saturating_sub(vh / 2).min(img_h.saturating_sub(vh)),
        w: vw,
        h: vh,
    }
}

fn wm_view_size(img_w: u32, img_h: u32, view_w: u32, view_h: u32) -> (u32, u32) {
    if img_w == 0 || img_h == 0 || view_w == 0 || view_h == 0 {
        return (img_w, img_h);
    }
    let mut vw = (img_w * WM_CAM_FRACTION.0 / WM_CAM_FRACTION.1).clamp(1, img_w);
    let vh = ((vw as u64 * view_h as u64) / view_w as u64).clamp(1, img_h as u64) as u32;
    if vh == img_h {
        vw = ((vh as u64 * view_w as u64) / view_h as u64).clamp(1, img_w as u64) as u32;
    }
    (vw, vh)
}

/// One frame of hero5's camera motion: the remaining distance is halved every
/// frame (never less than a pixel), so the window rushes off and settles. `true`
/// when it arrived.
pub fn wm_glide(cam: &mut WmCam, target: WmCam) -> bool {
    let step = |from: u32, to: u32| -> u32 {
        if from == to {
            return to;
        }
        let d = from.abs_diff(to);
        let s = (d / 2).max(1);
        if to > from { from + s } else { from - s }
    };
    cam.x = step(cam.x, target.x);
    cam.y = step(cam.y, target.y);
    cam.w = target.w;
    cam.h = target.h;
    cam.x == target.x && cam.y == target.y
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WmDir {
    Left,
    Right,
    Up,
    Down,
}

/// The place an arrow reaches from `from`: along the roads only. Of the
/// neighbours that lie that way at all (a positive component in the arrow's
/// direction — 반란군 캠프's one road runs up-left, so UP and LEFT both take it),
/// the one at the smallest angle to the arrow, distance breaking ties; `None`
/// when no road leaves `from` that way (RIGHT or DOWN from the camp does
/// nothing). The cursor can only go where the character could walk.
pub fn wm_step(from: usize, dir: WmDir) -> Option<usize> {
    let a = WM_PLACES[from];
    let mut best: Option<(usize, i64)> = None;
    for i in wm_neighbours(from) {
        let b = WM_PLACES[i];
        let dx = i64::from(b.x) - i64::from(a.x);
        let dy = i64::from(b.y) - i64::from(a.y);
        let (primary, side) = match dir {
            WmDir::Right => (dx, dy.abs()),
            WmDir::Left => (-dx, dy.abs()),
            WmDir::Down => (dy, dx.abs()),
            WmDir::Up => (-dy, dx.abs()),
        };
        if primary <= 0 {
            continue;
        }
        let angle = side * 100_000 / primary;
        let rank = angle * 1_000_000 + primary * primary + side * side;
        if best.is_none_or(|(_, r)| rank < r) {
            best = Some((i, rank));
        }
    }
    best.map(|(i, _)| i)
}

/// Picture point (thousandths) → window pixel inside `view`, clipped or not.
pub fn wm_screen_raw(cam: WmCam, px: u16, py: u16, img_w: u32, img_h: u32, view: WmBox) -> (i32, i32) {
    if cam.w == 0 || cam.h == 0 || img_w == 0 || img_h == 0 {
        return (0, 0);
    }
    let sx = i64::from(px) * i64::from(img_w) / 1000;
    let sy = i64::from(py) * i64::from(img_h) / 1000;
    let x = (sx - i64::from(cam.x)) * i64::from(view.w) / i64::from(cam.w);
    let y = (sy - i64::from(cam.y)) * i64::from(view.h) / i64::from(cam.h);
    (x as i32 + view.x as i32, y as i32 + view.y as i32)
}

/// Same, `None` when the point is outside the window.
pub fn wm_screen(cam: WmCam, px: u16, py: u16, img_w: u32, img_h: u32, view: WmBox) -> Option<(i32, i32)> {
    let (x, y) = wm_screen_raw(cam, px, py, img_w, img_h, view);
    view.contains(x, y).then_some((x, y))
}

/// The place a touch at `(x, y)` lands on: the nearest stud within 12 hero5 px,
/// or the name box under it. `None` off every place.
pub fn wm_hit_place(cam: WmCam, img_w: u32, img_h: u32, view: WmBox, unit: u32, label_w: impl Fn(usize) -> u32, x: i32, y: i32) -> Option<usize> {
    if !view.contains(x, y) {
        return None;
    }
    let reach = (12 * unit) as i64;
    let mut best: Option<(usize, i64)> = None;
    for (i, p) in WM_PLACES.iter().enumerate() {
        let Some((cx, cy)) = wm_screen(cam, p.x, p.y, img_w, img_h, view) else { continue };
        let d = (i64::from(x - cx)).pow(2) + (i64::from(y - cy)).pow(2);
        let hit_stud = d <= reach * reach;
        let hit_box = wm_label_box(cx, cy, label_w(i), unit).contains(x, y);
        if (hit_stud || hit_box) && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// The half-transparent name box hero5 draws under a stud: `text_w` wide plus a
/// 4-px margin each side, 17 px high, starting 10 px under the stud (all hero5
/// px, i.e. times `unit`). Centred on the stud.
pub fn wm_label_box(cx: i32, cy: i32, text_w: u32, unit: u32) -> WmBox {
    let w = text_w + 8 * unit;
    let h = 17 * unit;
    WmBox { x: (cx - (w / 2) as i32).max(0) as u32, y: (cy + (10 * unit) as i32).max(0) as u32, w, h }
}

/// A name box pushed back inside `view` when it spills over an edge (a place
/// near the picture's edge keeps its name instead of losing it); `None` when the
/// box is wider or taller than the window.
pub fn wm_label_inside(b: WmBox, view: WmBox) -> Option<WmBox> {
    if b.w > view.w || b.h > view.h {
        return None;
    }
    let x = b.x.max(view.x).min(view.x + view.w - b.w);
    let y = b.y.max(view.y).min(view.y + view.h - b.h);
    Some(WmBox { x, y, w: b.w, h: b.h })
}

/// How high the pin hops after a selection: hero5's pin motion is a short bounce
/// that settles (a frame every 33 ms; `frame` counts from the selection). In
/// hero5 px.
pub fn wm_pin_lift(frame: u32) -> i32 {
    const LIFT: [i32; 12] = [10, 8, 5, 2, 0, 3, 5, 3, 0, 1, 2, 0];
    LIFT.get(frame as usize).copied().unwrap_or(0)
}

// ── drawing ───────────────────────────────────────────────────────────────────

fn rgb565(p: u16) -> u32 {
    let r = u32::from((p >> 11) & 31) * 255 / 31;
    let g = u32::from((p >> 5) & 63) * 255 / 63;
    let b = u32::from(p & 31) * 255 / 31;
    0xff00_0000 | (r << 16) | (g << 8) | b
}

/// Bilinear blit of `cam` into `dest`. `px` is RGB565, length `iw * ih`. A short
/// buffer draws nothing. Bilinear, not nearest: the picture is an xBR 2× of the
/// painted base and the window stretches it 1.3–2× again — nearest sampling put
/// the staircase straight back.
pub fn wm_blit(buf: &mut [u32], scr_w: u32, scr_h: u32, px: &[u16], iw: u32, ih: u32, cam: WmCam, dest: WmBox) {
    let WmBox { x: dx, y: dy, w: dw, h: dh } = dest;
    if iw == 0 || ih == 0 || cam.w == 0 || cam.h == 0 || dw == 0 || dh == 0 || px.len() < (iw as usize) * (ih as usize) {
        return;
    }
    if buf.len() < (scr_w as usize) * (scr_h as usize) || dx + dw > scr_w || dy + dh > scr_h {
        return;
    }
    // 16.16 fixed-point source coordinates, sampled at pixel centres.
    let xs: Vec<(u32, u32, u32)> = (0..dw)
        .map(|x| {
            let fx = ((x as u64 * 2 + 1) * cam.w as u64 * 32768 / dw as u64).saturating_sub(32768) as u32 + (cam.x << 16);
            let x0 = (fx >> 16).min(iw - 1);
            (x0, (x0 + 1).min(iw - 1), fx & 0xffff)
        })
        .collect();
    let at = |x: u32, y: u32| px[(y * iw + x) as usize];
    for y in 0..dh {
        let fy = ((y as u64 * 2 + 1) * cam.h as u64 * 32768 / dh as u64).saturating_sub(32768) as u32 + (cam.y << 16);
        let y0 = (fy >> 16).min(ih - 1);
        let y1 = (y0 + 1).min(ih - 1);
        let wy = fy & 0xffff;
        let row = ((dy + y) * scr_w + dx) as usize;
        for (x, &(x0, x1, wx)) in xs.iter().enumerate() {
            let (p00, p10, p01, p11) = (rgb565(at(x0, y0)), rgb565(at(x1, y0)), rgb565(at(x0, y1)), rgb565(at(x1, y1)));
            let mut out = 0xff00_0000u32;
            for sh in [16u32, 8, 0] {
                let c = |p: u32| ((p >> sh) & 0xff) as u64;
                let top = c(p00) * (65536 - wx as u64) + c(p10) * wx as u64;
                let bot = c(p01) * (65536 - wx as u64) + c(p11) * wx as u64;
                let v = (top * (65536 - wy as u64) + bot * wy as u64) >> 32;
                out |= (v as u32 & 0xff) << sh;
            }
            buf[row + x] = out;
        }
    }
}

fn blend(buf: &mut [u32], i: usize, color: u32, a: u32) {
    let d = buf[i];
    let inv = 255 - a;
    let mix = |sh: u32| ((((color >> sh) & 0xff) * a + ((d >> sh) & 0xff) * inv) / 255) & 0xff;
    buf[i] = 0xff00_0000 | (mix(16) << 16) | (mix(8) << 8) | mix(0);
}

/// Fill `b` with `color` at alpha `a`, clipped to `clip`.
pub fn wm_fill_a(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, b: WmBox, color: u32, a: u32) {
    if a == 0 || buf.len() < (scr_w as usize) * (scr_h as usize) {
        return;
    }
    let x0 = b.x.max(clip.x);
    let y0 = b.y.max(clip.y);
    let x1 = (b.x + b.w).min(clip.x + clip.w).min(scr_w);
    let y1 = (b.y + b.h).min(clip.y + clip.h).min(scr_h);
    for y in y0..y1 {
        let row = (y * scr_w) as usize;
        for x in x0..x1 {
            if a >= 255 {
                buf[row + x as usize] = color | 0xff00_0000;
            } else {
                blend(buf, row + x as usize, color, a);
            }
        }
    }
}

/// hero5's edge shading: 35 one-pixel frames from the picture's edge inward, the
/// outermost at alpha 0x88 and each next one 4 less (DrawWorldMap @0x126edc).
/// `width` is the band in window px (35 hero5 px); the alpha ramps over it.
pub fn wm_vignette(buf: &mut [u32], scr_w: u32, scr_h: u32, view: WmBox, width: u32) {
    if width == 0 || view.w < 2 * width || view.h < 2 * width || buf.len() < (scr_w as usize) * (scr_h as usize) {
        return;
    }
    for d in 0..width {
        let a = 0x88 * (width - d) / width;
        if a == 0 {
            continue;
        }
        let x0 = view.x + d;
        let x1 = view.x + view.w - 1 - d;
        let y0 = view.y + d;
        let y1 = view.y + view.h - 1 - d;
        for x in x0..=x1 {
            blend(buf, (y0 * scr_w + x) as usize, 0, a);
            blend(buf, (y1 * scr_w + x) as usize, 0, a);
        }
        for y in (y0 + 1)..y1 {
            blend(buf, (y * scr_w + x0) as usize, 0, a);
            blend(buf, (y * scr_w + x1) as usize, 0, a);
        }
    }
}

/// Anti-aliased disc, clipped to `clip`. `r` in window px.
pub fn wm_disc(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, cx: i32, cy: i32, r: f32, color: u32, a: u32) {
    if buf.len() < (scr_w as usize) * (scr_h as usize) || a == 0 {
        return;
    }
    let ri = r.ceil() as i32 + 1;
    for y in (cy - ri).max(clip.y as i32)..(cy + ri + 1).min((clip.y + clip.h).min(scr_h) as i32) {
        for x in (cx - ri).max(clip.x as i32)..(cx + ri + 1).min((clip.x + clip.w).min(scr_w) as i32) {
            let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f32).sqrt();
            let cov = (r + 0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(buf, (y as u32 * scr_w + x as u32) as usize, color, (a as f32 * cov) as u32);
            }
        }
    }
}

/// Anti-aliased ring of thickness `t`, clipped.
pub fn wm_ring(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, cx: i32, cy: i32, r: f32, t: f32, color: u32, a: u32) {
    if buf.len() < (scr_w as usize) * (scr_h as usize) || a == 0 {
        return;
    }
    let ri = r.ceil() as i32 + 1;
    for y in (cy - ri).max(clip.y as i32)..(cy + ri + 1).min((clip.y + clip.h).min(scr_h) as i32) {
        for x in (cx - ri).max(clip.x as i32)..(cx + ri + 1).min((clip.x + clip.w).min(scr_w) as i32) {
            let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f32).sqrt();
            let cov = (r + 0.5 - d).clamp(0.0, 1.0) * (d - (r - t) + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(buf, (y as u32 * scr_w + x as u32) as usize, color, (a as f32 * cov) as u32);
            }
        }
    }
}

/// Anti-aliased line of thickness `t` from `(x0, y0)` to `(x1, y1)`, clipped to
/// `clip`. Walks the major axis and touches only the few pixels around the
/// line at each step, so a road that crosses the whole window costs its visible
/// length, not its bounding box — the popup redraws every frame while the
/// camera glides.
pub fn wm_line(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, x0: i32, y0: i32, x1: i32, y1: i32, t: f32, color: u32, a: u32) {
    if buf.len() < (scr_w as usize) * (scr_h as usize) || a == 0 || clip.w == 0 || clip.h == 0 {
        return;
    }
    let cx0 = clip.x as i32;
    let cy0 = clip.y as i32;
    let cx1 = (clip.x + clip.w).min(scr_w) as i32; // exclusive
    let cy1 = (clip.y + clip.h).min(scr_h) as i32;
    let (dx, dy) = ((x1 - x0) as f32, (y1 - y0) as f32);
    let len2 = dx * dx + dy * dy;
    if len2 < 0.25 {
        return;
    }
    let len = len2.sqrt();
    let half = t / 2.0;
    // Distance from a pixel centre to the segment.
    let dist = |px: f32, py: f32| -> f32 {
        let u = (((px - x0 as f32) * dx + (py - y0 as f32) * dy) / len2).clamp(0.0, 1.0);
        let (qx, qy) = (x0 as f32 + u * dx, y0 as f32 + u * dy);
        ((px - qx).powi(2) + (py - qy).powi(2)).sqrt()
    };
    let horizontal = dx.abs() >= dy.abs();
    // Reach across the minor axis: the half thickness over the cosine of the
    // slope, plus the end caps' rounding.
    let reach = (half * len / if horizontal { dx.abs() } else { dy.abs() } + 1.5).ceil() as i32;
    let (ma0, ma1) = if horizontal { (x0.min(x1), x0.max(x1)) } else { (y0.min(y1), y0.max(y1)) };
    let (lo, hi) = if horizontal { (cx0, cx1) } else { (cy0, cy1) };
    let m_start = (ma0 - reach).max(lo);
    let m_end = (ma1 + reach + 1).min(hi);
    for m in m_start..m_end {
        // The minor coordinate where the line crosses this column/row.
        let u = if horizontal { (m as f32 + 0.5 - x0 as f32) / dx } else { (m as f32 + 0.5 - y0 as f32) / dy };
        let u = u.clamp(0.0, 1.0);
        let centre = if horizontal { y0 as f32 + u * dy } else { x0 as f32 + u * dx };
        let (nlo, nhi) = if horizontal { (cy0, cy1) } else { (cx0, cx1) };
        let n_start = (centre.floor() as i32 - reach).max(nlo);
        let n_end = (centre.ceil() as i32 + reach + 1).min(nhi);
        for n in n_start..n_end {
            let (px, py) = if horizontal { (m, n) } else { (n, m) };
            let d = dist(px as f32 + 0.5, py as f32 + 0.5);
            let cov = (half + 0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(buf, (py as u32 * scr_w + px as u32) as usize, color, (a as f32 * cov) as u32);
            }
        }
    }
}

/// A road between two places on the picture: a soft dark shadow under a light
/// grey stroke. Drawn before the studs and name boxes, so a name always sits on
/// top of a line.
pub fn wm_draw_road(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, x0: i32, y0: i32, x1: i32, y1: i32, unit: u32) {
    let u = unit as f32;
    wm_line(buf, scr_w, scr_h, clip, x0, y0, x1, y1, 2.1 * u, 0x0000_0000, 110);
    wm_line(buf, scr_w, scr_h, clip, x0, y0, x1, y1, 1.1 * u, 0xffe8_e8f0, 210);
}

/// What a stud is: an ordinary place, the place the character is in, or the one
/// the cursor is on (`glow` ∈ 0..=255 is the pulse — hero5's stud breathes
/// between its green and a lighter blue-white).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WmStud {
    Plain,
    Here,
    Cursor { glow: u32, here: bool },
}

/// hero5's stud: a 7-px dark grey disc with a light rim; green when it is the
/// character's place, green or blue and glowing under the cursor.
pub fn wm_draw_stud(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, cx: i32, cy: i32, unit: u32, kind: WmStud) {
    let u = unit as f32;
    match kind {
        WmStud::Plain => {
            wm_disc(buf, scr_w, scr_h, clip, cx, cy, 3.6 * u, 0xff20_2028, 255);
            wm_disc(buf, scr_w, scr_h, clip, cx, cy, 2.6 * u, 0xff60_6068, 255);
            wm_ring(buf, scr_w, scr_h, clip, cx, cy, 3.6 * u, 1.0 * u, 0xffd0_d0d8, 200);
        }
        WmStud::Here => {
            wm_disc(buf, scr_w, scr_h, clip, cx, cy, 3.6 * u, 0xff10_4018, 255);
            wm_disc(buf, scr_w, scr_h, clip, cx, cy, 2.6 * u, 0xff40_d850, 255);
            wm_ring(buf, scr_w, scr_h, clip, cx, cy, 3.6 * u, 1.0 * u, 0xffd0_ffd8, 200);
        }
        WmStud::Cursor { glow, here } => {
            let (core, halo) = if here { (0xff40_e050u32, 0xff90_ff90u32) } else { (0xff40_a0ff, 0xffa0_e0ff) };
            wm_disc(buf, scr_w, scr_h, clip, cx, cy, (6.0 + 2.0 * glow as f32 / 255.0) * u, halo, 60 + glow * 70 / 255);
            wm_disc(buf, scr_w, scr_h, clip, cx, cy, 3.6 * u, 0xff10_2030, 255);
            wm_disc(buf, scr_w, scr_h, clip, cx, cy, 2.6 * u, core, 255);
            wm_disc(buf, scr_w, scr_h, clip, cx - (u * 0.8) as i32, cy - (u * 0.8) as i32, 0.9 * u, 0xffff_ffff, 150 + glow * 100 / 255);
            wm_ring(buf, scr_w, scr_h, clip, cx, cy, 3.6 * u, 1.0 * u, 0xffff_ffff, 230);
        }
    }
}

/// hero5's cursor pin: an orange teardrop, tip just above the stud, head about
/// 9 px wide, `lift` px higher while it bounces.
pub fn wm_draw_pin(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, cx: i32, cy: i32, unit: u32, lift: i32) {
    if buf.len() < (scr_w as usize) * (scr_h as usize) {
        return;
    }
    let u = unit as f32;
    let tip_y = cy as f32 - 5.0 * u - (lift * unit as i32) as f32;
    let head_r = 4.5 * u;
    let head_y = tip_y - 9.0 * u;
    let x0 = (cx as f32 - head_r - 1.0).floor() as i32;
    let x1 = (cx as f32 + head_r + 1.0).ceil() as i32;
    let y0 = (head_y - head_r - 1.0).floor() as i32;
    let y1 = (tip_y + 1.0).ceil() as i32;
    for y in y0.max(clip.y as i32)..(y1 + 1).min((clip.y + clip.h).min(scr_h) as i32) {
        for x in x0.max(clip.x as i32)..(x1 + 1).min((clip.x + clip.w).min(scr_w) as i32) {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            // Signed distance to the teardrop: the head disc, and the cone from the
            // disc's sides down to the tip.
            let dh = ((fx - cx as f32).powi(2) + (fy - head_y).powi(2)).sqrt() - head_r;
            let dc = if fy >= head_y && fy <= tip_y {
                let half = head_r * (tip_y - fy) / (tip_y - head_y);
                (fx - cx as f32).abs() - half
            } else {
                f32::MAX
            };
            let d = dh.min(dc);
            let cov = (0.5 - d).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let edge = (-d - 1.2 * u).clamp(0.0, 1.0); // 0 at the outline, 1 inside
            let hi = (1.0 - ((fx - (cx as f32 - 1.5 * u)).powi(2) + (fy - (head_y - 1.5 * u)).powi(2)).sqrt() / (2.5 * u)).clamp(0.0, 1.0);
            let base = (0xf0u32, 0x58u32, 0x18u32);
            let r = (base.0 as f32 * edge + 0x60 as f32 * (1.0 - edge) + 0x0f as f32 * hi).min(255.0) as u32;
            let g = (base.1 as f32 * edge + 0x20 as f32 * (1.0 - edge) + 0x60 as f32 * hi).min(255.0) as u32;
            let b = (base.2 as f32 * edge + 0x10 as f32 * (1.0 - edge) + 0x30 as f32 * hi).min(255.0) as u32;
            blend(buf, (y as u32 * scr_w + x as u32) as usize, (r << 16) | (g << 8) | b, (255.0 * cov) as u32);
        }
    }
}

/// hero5's round back button: a translucent dark disc with a light rim and a
/// counter-clockwise arrow.
pub fn wm_draw_back(buf: &mut [u32], scr_w: u32, scr_h: u32, clip: WmBox, b: WmBox, pressed: bool) {
    let cx = (b.x + b.w / 2) as i32;
    let cy = (b.y + b.h / 2) as i32;
    let r = b.w.min(b.h) as f32 / 2.0;
    wm_disc(buf, scr_w, scr_h, clip, cx, cy, r, if pressed { 0xff60_6070 } else { 0xff20_2028 }, 190);
    wm_ring(buf, scr_w, scr_h, clip, cx, cy, r, (r / 9.0).max(1.0), 0xffd0_d0d8, 220);
    // The arrow: a three-quarter ring with a wedge head at its open end.
    let ar = r * 0.5;
    let t = (r / 7.0).max(1.0);
    let ri = (ar + t + 1.0).ceil() as i32;
    for y in (cy - ri).max(clip.y as i32)..(cy + ri + 1).min((clip.y + clip.h).min(scr_h) as i32) {
        for x in (cx - ri).max(clip.x as i32)..(cx + ri + 1).min((clip.x + clip.w).min(scr_w) as i32) {
            let fx = (x - cx) as f32;
            let fy = (y - cy) as f32;
            let d = (fx * fx + fy * fy).sqrt();
            let ang = fy.atan2(fx); // -π..π, 0 = right, +π/2 = down
            // Leave the gap in the upper-right quadrant where the head sits.
            let open = ang > -1.5 && ang < -0.2;
            let cov = (ar + t / 2.0 + 0.5 - d).clamp(0.0, 1.0) * (d - (ar - t / 2.0) + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 && !open {
                blend(buf, (y as u32 * scr_w + x as u32) as usize, 0xffff_ffff, (230.0 * cov) as u32);
            }
        }
    }
    // Head: a small triangle pointing left, at the top of the ring's gap.
    let hx = cx as f32 + ar * (-0.2f32).cos();
    let hy = cy as f32 + ar * (-0.2f32).sin();
    let hs = t * 2.2;
    for y in ((hy - hs) as i32).max(clip.y as i32)..((hy + hs) as i32 + 1).min((clip.y + clip.h).min(scr_h) as i32) {
        for x in ((hx - hs) as i32).max(clip.x as i32)..((hx + hs) as i32 + 1).min((clip.x + clip.w).min(scr_w) as i32) {
            let fx = x as f32 + 0.5 - hx;
            let fy = y as f32 + 0.5 - hy;
            // Triangle: apex up-left, base along the ring.
            if fy.abs() <= hs && fx <= hs * 0.3 && fx >= -hs + fy.abs() {
                blend(buf, (y as u32 * scr_w + x as u32) as usize, 0xffff_ffff, 230);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: WmBox = WmBox { x: 100, y: 80, w: 1600, h: 900 };

    fn cam(map: u32) -> WmCam {
        wm_open(map, 2560, 1440, VIEW.w, VIEW.h)
    }

    #[test]
    fn the_crypt_the_two_forests_and_the_cave_are_different_places() {
        assert_eq!(wm_place(98).name, "카디프숲 북부");
        assert_eq!(wm_place(99).name, "카디프숲 남부");
        assert_eq!(wm_place(50).name, "납골당");
        assert_eq!(wm_place(104).name, "납골당");
        assert_eq!(wm_place(92).name, "어둠동굴");
        assert_eq!(wm_place(82).name, "서리골짜기 남부");
        assert_eq!(wm_place(83).name, "서리골짜기 북부");
        assert_eq!(wm_place(52).name, "예언자의 무덤");
        assert_eq!(wm_place(105).name, "예언자의 무덤");
        assert_eq!(wm_place(76).name, "서리골짜기");
        assert_eq!(wm_place(28).name, "버려진 섬");
        assert_eq!(wm_place(0).name, "윈터하츠");
        assert_eq!(wm_place(21).name, "윈터하츠");
        assert_eq!(wm_place(18).name, "윈터하츠 남부1");
        assert_eq!(wm_place(20).name, "윈터하츠 남부2");
        assert_eq!(wm_place(89).name, "도적굴");
        assert_eq!(wm_place(15).name, "신전");
        assert_eq!(wm_place(100).name, "반란군 캠프");
        assert_eq!(wm_place(27).name, "노르덴 남부");
    }

    #[test]
    fn opening_frames_that_place_and_stays_inside_the_picture() {
        let c = cam(50);
        let ax = 150 * 2560 / 1000;
        let ay = 735 * 1440 / 1000;
        assert!(ax >= c.x && ax < c.x + c.w, "the crypt is in the window");
        assert!(ay >= c.y && ay < c.y + c.h, "the crypt is in the window");
        assert!(c.x + c.w <= 2560 && c.y + c.h <= 1440);
        let winter = cam(0);
        assert!(winter.w < 2560 && winter.h < 1440, "the window is a part of the picture");
        assert_eq!(winter.w, 2560 * WM_CAM_FRACTION.0 / WM_CAM_FRACTION.1, "two fifths of the picture's width");
        assert_eq!(winter.w, 1024);
        let (wx, wy) = wm_screen(winter, 480, 520, 2560, 1440, VIEW).expect("윈터하츠 is on screen");
        let mx = VIEW.x as i32 + VIEW.w as i32 / 2;
        assert!((wx - mx).abs() <= 2, "and centred across: {wx} vs {mx}");
        // And centred down the window too (the castle is far enough from the top).
        let my = VIEW.y as i32 + VIEW.h as i32 / 2;
        assert!((wy - my).abs() <= 2, "and centred down: {wy} vs {my}");
        assert_eq!(winter.y, 520 * 1440 / 1000 - winter.h / 2);
        // The tomb under the north-west peaks clamps to the top-left corner instead.
        assert_eq!(cam(52), WmCam { x: 0, y: 0, w: winter.w, h: winter.h });
    }

    #[test]
    fn the_roads_are_a_connected_graph_of_distinct_places() {
        for &(a, b) in WM_ROADS {
            assert!(a < WM_PLACES.len() && b < WM_PLACES.len() && a != b, "road {a}-{b}");
        }
        for (i, &(a, b)) in WM_ROADS.iter().enumerate() {
            for &(c, d) in &WM_ROADS[i + 1..] {
                assert!(!((a, b) == (c, d) || (a, b) == (d, c)), "road {a}-{b} listed twice");
            }
        }
        // One continent: walking the roads from the castle reaches every place.
        let mut seen = vec![false; WM_PLACES.len()];
        let mut stack = vec![WM_WINTER];
        while let Some(i) = stack.pop() {
            if seen[i] {
                continue;
            }
            seen[i] = true;
            stack.extend(wm_neighbours(i));
        }
        let missing: Vec<&str> = seen.iter().enumerate().filter(|(_, s)| !**s).map(|(i, _)| WM_PLACES[i].name).collect();
        assert!(missing.is_empty(), "no road leads to: {missing:?}");
        assert_eq!(wm_neighbours(WM_CAMP).collect::<Vec<_>>(), vec![WM_SILVA_S]);
    }

    #[test]
    fn an_arrow_follows_a_road_or_does_nothing() {
        // The user's example: 반란군 캠프's one road runs up-left to 실바나르 남부 —
        // UP and LEFT take it, RIGHT and DOWN do nothing.
        assert_eq!(wm_step(WM_CAMP, WmDir::Up), Some(WM_SILVA_S));
        assert_eq!(wm_step(WM_CAMP, WmDir::Left), Some(WM_SILVA_S));
        assert_eq!(wm_step(WM_CAMP, WmDir::Right), None);
        assert_eq!(wm_step(WM_CAMP, WmDir::Down), None);
        // And never a place with no road to it: the cave is up-right of the camp
        // and used to be where UP went.
        for d in [WmDir::Left, WmDir::Right, WmDir::Up, WmDir::Down] {
            assert_ne!(wm_step(WM_CAMP, d), Some(WM_CAVE));
        }
        // The castle's four roads are its four arrows.
        assert_eq!(wm_step(WM_WINTER, WmDir::Up), Some(WM_TEMPLE));
        assert_eq!(wm_step(WM_WINTER, WmDir::Down), Some(WM_WINTER_SOUTH));
        assert_eq!(wm_step(WM_WINTER, WmDir::Left), Some(WM_REID));
        assert_eq!(wm_step(WM_WINTER, WmDir::Right), Some(WM_UNDER));
        // The frost valley climbs north from the village: from 서리골짜기 남부 DOWN
        // is 실바나르, UP 서리골짜기 북부, RIGHT 어둠동굴; from the village UP is the
        // valley and DOWN 실바나르 남부, nothing sideways.
        assert_eq!(wm_step(WM_ROAD_S, WmDir::Right), Some(WM_CAVE));
        assert_eq!(wm_step(WM_ROAD_S, WmDir::Up), Some(WM_ROAD_N));
        assert_eq!(wm_step(WM_ROAD_S, WmDir::Down), Some(WM_SILVA));
        assert_eq!(wm_step(WM_SILVA, WmDir::Up), Some(WM_ROAD_S));
        assert_eq!(wm_step(WM_SILVA, WmDir::Down), Some(WM_SILVA_S));
        assert_eq!(wm_step(WM_SILVA, WmDir::Left), None);
        assert_eq!(wm_step(WM_SILVA, WmDir::Right), None);

        assert_eq!(wm_step(WM_SILVA_S, WmDir::Left), Some(WM_CRYPT));
        assert_eq!(wm_step(WM_SILVA_S, WmDir::Right), Some(WM_CAMP));
        assert_eq!(wm_step(WM_SILVA_S, WmDir::Down), Some(WM_CARDIFF_S));
        // Inside the castle: 신전 up, 리이드 지하 left, 언더시티 right, the gate road down.
        assert_eq!(wm_step(WM_UNDER, WmDir::Right), Some(WM_MULLAK));
        assert_eq!(wm_step(WM_MULLAK, WmDir::Up), Some(WM_SYNDRAF));
        // 협곡지하 is the cave east of the gorge. RIGHT from the gorge, UP from the
        // north plain; it does not continue to the port or to 신드라프.
        assert_eq!(wm_step(WM_MULLAK, WmDir::Right), Some(WM_GORGE));
        assert_eq!(wm_step(WM_MULLAK, WmDir::Down), Some(WM_NORDOST_N));
        assert_eq!(wm_step(WM_NORDOST, WmDir::Up), Some(WM_NORDOST_N));
        assert_eq!(wm_step(WM_NORDOST_N, WmDir::Up), Some(WM_MULLAK));
        assert_eq!(wm_step(WM_NORDOST_N, WmDir::Right), Some(WM_GORGE));
        assert_eq!(wm_step(WM_NORDOST_N, WmDir::Down), Some(WM_NORDOST));
        assert_eq!(wm_step(WM_GORGE, WmDir::Up), Some(WM_MULLAK));
        assert_eq!(wm_step(WM_GORGE, WmDir::Left), Some(WM_MULLAK));
        assert_eq!(wm_step(WM_GORGE, WmDir::Right), None);
        assert_eq!(wm_step(WM_GORGE, WmDir::Down), Some(WM_NORDOST_N));
        assert_eq!(wm_place(95).name, "협곡지하");
        assert_eq!(wm_place(97).name, "협곡지하");
        assert_eq!(wm_place(25).name, "노르도스트 북부");
        assert_eq!(wm_place(26).name, "노르도스트 평원");
        assert_eq!(wm_place(80).name, "멀락협곡");
        assert_eq!(wm_step(WM_NORDEN_S, WmDir::Up), Some(WM_NORDEN));
        assert_eq!(wm_step(WM_NORDEN, WmDir::Down), Some(WM_NORDEN_S));
        assert_eq!(wm_step(WM_NORDEN_S, WmDir::Right), Some(WM_TUNNEL));
        assert_eq!(wm_step(WM_TUNNEL, WmDir::Down), Some(WM_ISLAND));
        assert_eq!(wm_step(WM_ISLAND, WmDir::Down), Some(WM_LAB));
        // A dead end answers one way only.
        assert_eq!(wm_step(WM_LAB, WmDir::Right), None);
        assert_eq!(wm_step(WM_LAB, WmDir::Down), None);
        assert_eq!(wm_step(WM_LAB, WmDir::Left), Some(WM_ISLAND));
        // Every road can be taken with some arrow from either end — no neighbour
        // is shadowed by another in all four directions.
        for &(a, b) in WM_ROADS {
            for (from, to) in [(a, b), (b, a)] {
                let ok = [WmDir::Left, WmDir::Right, WmDir::Up, WmDir::Down].iter().any(|&d| wm_step(from, d) == Some(to));
                assert!(ok, "no arrow takes {} → {}", WM_PLACES[from].name, WM_PLACES[to].name);
            }
        }
        // So the arrows alone still reach every place.
        let mut seen = vec![false; WM_PLACES.len()];
        let mut stack = vec![WM_CAMP];
        while let Some(i) = stack.pop() {
            if seen[i] {
                continue;
            }
            seen[i] = true;
            for d in [WmDir::Left, WmDir::Right, WmDir::Up, WmDir::Down] {
                if let Some(n) = wm_step(i, d) {
                    stack.push(n);
                }
            }
        }
        assert!(seen.iter().all(|&s| s), "arrows do not reach every place");
    }

    #[test]
    fn a_road_line_stays_inside_its_clip_and_lies_along_the_road() {
        let clip = WmBox { x: 10, y: 10, w: 80, h: 60 };
        let mut buf = vec![0xff00_0000u32; 100 * 100];
        // A diagonal road from outside the clip to outside it on the other side.
        wm_line(&mut buf, 100, 100, clip, 0, 0, 99, 99, 3.0, 0x00ff_ffff, 255);
        let lit = |x: usize, y: usize| buf[y * 100 + x] & 0xff > 0x40;
        assert!(lit(30, 30) && lit(50, 50) && lit(60, 60), "on the line, inside the clip");
        assert!(!lit(5, 5) && !lit(95, 95), "clipped");
        assert!(!lit(30, 60) && !lit(80, 20), "not off the line");
        assert!(lit(69, 69) && !lit(70, 70), "the clip's far edge is exclusive");
        // Nothing lit outside the clip anywhere.
        for y in 0..100 {
            for x in 0..100 {
                if !clip.contains(x as i32, y as i32) {
                    assert!(!lit(x, y), "lit outside the clip at ({x}, {y})");
                }
            }
        }
        // A short buffer draws nothing.
        let mut short = vec![0u32; 10];
        wm_line(&mut short, 100, 100, clip, 0, 0, 99, 99, 3.0, 0x00ff_ffff, 255);
        assert!(short.iter().all(|&p| p == 0));
    }

    #[test]
    fn southern_forest_follows_the_map_exit_directions() {
        assert_eq!(wm_step(WM_SOUTH2, WmDir::Down), Some(WM_CARDIFF_N));
        assert_eq!(wm_step(WM_CARDIFF_N, WmDir::Down), Some(WM_CARDIFF_S));
        assert_eq!(wm_step(WM_CARDIFF_S, WmDir::Left), Some(WM_SILVA_S));
        assert_eq!(wm_step(WM_SILVA_S, WmDir::Right), Some(WM_CAMP));
        assert!(WM_PLACES[WM_SOUTH2].y < WM_PLACES[WM_CARDIFF_N].y);
        assert!(WM_PLACES[WM_CARDIFF_N].y < WM_PLACES[WM_CARDIFF_S].y);
    }

    #[test]
    fn the_camera_glides_and_settles() {
        let mut c = cam(0);
        let target = cam(28);
        assert_ne!(c, target);
        let mut frames = 0;
        while !wm_glide(&mut c, target) {
            frames += 1;
            assert!(frames < 40, "halving the distance settles within a few frames");
        }
        assert_eq!(c, target);
        assert!(wm_glide(&mut c, target), "and stays");
    }

    #[test]
    fn a_touch_lands_on_the_stud_or_its_name_and_nowhere_else() {
        let c = cam(0);
        let lw = |_: usize| 60;
        let (x, y) = wm_screen(c, 480, 520, 2560, 1440, VIEW).unwrap();
        assert_eq!(wm_hit_place(c, 2560, 1440, VIEW, 3, lw, x + 10, y - 10), Some(WM_WINTER));
        let b = wm_label_box(x, y, 60, 3);
        assert_eq!(wm_hit_place(c, 2560, 1440, VIEW, 3, lw, b.x as i32 + 2, b.y as i32 + 2), Some(WM_WINTER));
        assert_eq!(wm_hit_place(c, 2560, 1440, VIEW, 3, lw, x + 200, y + 200), None, "open ground");
        assert_eq!(wm_hit_place(c, 2560, 1440, VIEW, 3, lw, 5, 5), None, "outside the window");
    }

    #[test]
    fn the_panel_is_a_popup_under_the_tabs_with_hero5_proportions() {
        let p = wm_panel(2340, 1080);
        assert!(p.frame.x > 0 && p.frame.x + p.frame.w < 2340, "not edge to edge");
        assert!(p.frame.y >= 1080 * 65 / 1000, "starts under the hanging tabs");
        assert_eq!(p.view.y, p.band.y + p.band.h, "the picture starts under the band");
        assert_eq!(p.unit, ((p.view.h + 120) / 240).min((p.view.w + 200) / 400));
        assert!(p.band.h <= 24 * p.unit + 1);
        assert!(p.back.x + p.back.w <= p.band.x + p.band.w && p.back.y >= p.band.y, "back button in the band");
        assert_eq!(p.mini.x, p.view.x);
        assert_eq!(p.mini.y + p.mini.h, p.view.y + p.view.h, "toggle in the bottom-left corner");
        let q = wm_panel(1080, 2340);
        assert!(q.unit >= 1 && q.view.w > 0 && q.view.h > 0);
        assert_eq!(wm_panel(0, 0), WmPanel::default());
    }

    #[test]
    fn the_blit_interpolates_instead_of_stepping() {
        // A 2×2 picture of black/white columns stretched to 8 px wide: nearest
        // gives 4 black then 4 white; bilinear ramps through the middle.
        let px = [0u16, 0xffff, 0, 0xffff];
        let mut buf = vec![0u32; 8 * 2];
        wm_blit(&mut buf, 8, 2, &px, 2, 2, WmCam { x: 0, y: 0, w: 2, h: 2 }, WmBox { x: 0, y: 0, w: 8, h: 2 });
        let lum: Vec<u32> = (0..8).map(|x| buf[x] & 0xff).collect();
        assert_eq!(lum[0], 0);
        assert_eq!(lum[7], 255);
        assert!(lum.windows(2).all(|w| w[0] <= w[1]), "monotone: {lum:?}");
        assert!(lum[3] > 0 && lum[4] < 255, "a ramp, not a step: {lum:?}");
        // A short buffer draws nothing.
        let mut short = vec![0u32; 4];
        wm_blit(&mut short, 8, 2, &px, 2, 2, WmCam { x: 0, y: 0, w: 2, h: 2 }, WmBox { x: 0, y: 0, w: 8, h: 2 });
        assert!(short.iter().all(|&p| p == 0));
    }

    #[test]
    fn a_name_box_at_the_edge_is_pushed_inside_not_dropped() {
        let view = WmBox { x: 100, y: 100, w: 400, h: 200 };
        let b = WmBox { x: 450, y: 280, w: 120, h: 40 };
        assert_eq!(wm_label_inside(b, view), Some(WmBox { x: 380, y: 260, w: 120, h: 40 }));
        let inside = WmBox { x: 150, y: 150, w: 120, h: 40 };
        assert_eq!(wm_label_inside(inside, view), Some(inside), "an inside box is unchanged");
        assert_eq!(wm_label_inside(WmBox { x: 0, y: 0, w: 401, h: 40 }, view), None, "wider than the window");
    }

    #[test]
    fn the_pin_bounces_and_rests() {
        assert!(wm_pin_lift(0) > wm_pin_lift(3));
        assert_eq!(wm_pin_lift(11), 0);
        assert_eq!(wm_pin_lift(1000), 0);
    }
}
