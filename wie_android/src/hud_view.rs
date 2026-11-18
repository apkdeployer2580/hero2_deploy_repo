// hero2's STATUS HUD — the hero5-style block at the top-left and the EXP bar at the bottom
// centre (2026-09-29).
//
// ── WHAT THE PLAYER ASKED FOR ────────────────────────────────────────────────────
// *"게임 UI를 조금 더 세련되게 … Screenshot_20260929_023746_5 파일처럼 좌측 상단 현재 캐릭터
// 모습(외형), HP,MP,경험치바, 미니맵외형 등을 꾸미고싶어"* — the screenshot is 영웅서기5's own
// in-game HUD (`docs/reference/`, and `Hero5NativeAPP/docs/shots/hl5-08-gameplay-hud.png` at
// its native 400×240). Asked back, the player chose: the portrait is the character's own
// FIELD HEAD (so the helmet shows), the EXP bar goes to the bottom centre, the minimap loses
// its panel and gains monster dots (`minimap_view.rs`).
//
// ── WHAT IT IS ────────────────────────────────────────────────────────────────────
// Pixel art, drawn in HUD PIXELS: one hud pixel `u` = the game's own pixel size, rounded
// (`hv_unit` — 5 screen px on a 1080-tall phone), so the chrome has the same grain as the
// world under it, the way hero5's does. Every colour below was measured off hero5's frame
// (the rows of its HP/MP block, its ring, its EXP bar) — the DESIGN is borrowed, no art is:
// the one picture drawn here is hero2's own head sprite (`hero/0/<n>`, `atlas::head`).
//
//   * RING — a gold-rimmed disc with the head inside (`hero/0/<helmet sprite>`, frame 2 =
//     facing the viewer; `hero2java::h2_head_sprite` says which).
//   * BARS — HP over SP, one silver-framed block that starts UNDER the ring and ends in a
//     45° slant, each bar with its value in a dark box at its middle.
//   * LEVEL PLATE — under the ring's right side, the level in white.
//   * EXP — a thin bar at the bottom centre: cream fill in a grey frame, and an orange
//     track in a black one for what is still to earn (hero5 draws its empty bar orange).
//
// ── WHY A FILE OF ITS OWN ─────────────────────────────────────────────────────────
// Dependency-free, like `minimap_view.rs`: `overlay.rs` `include!`s it for the phone,
// `wie_probe --h2-status` `include!`s it to draw the identical block over a dumped frame
// (the only way to SEE it without a device), and `hud_view_selftest.rs` tests it with plain
// `rustc`. No `use crate::…`, edition-2021 syntax only.

/// Border between the game rect's top-left corner and the block, in hud px (hero5: 3).
pub const HV_MARGIN: u32 = 3;
/// The ring's diameter, in hud px.
pub const HV_RING: u32 = 24;
/// Where the bar block starts — under the ring's centre, so the ring hides its left end.
pub const HV_BAR_X0: u32 = 12;
/// The column a bar's fill starts from at 0 %: still under the ring on every row the bars
/// use, so an empty bar shows no sliver of colour beside it.
pub const HV_FILL_X0: u32 = 18;
/// The bars' right end on their top row, in the landscape layout (hero5: ~89).
pub const HV_BAR_R_MAX: u32 = 94;
/// …and never shorter than this, whatever the window.
pub const HV_BAR_R_MIN: u32 = 56;
/// The level plate's left edge and its right end on its top row.
pub const HV_PLATE_X0: u32 = 15;
pub const HV_PLATE_R: u32 = 42;
/// The plate's first row (it runs to `HV_PLATE_Y0 + 8`).
pub const HV_PLATE_Y0: u32 = 20;
/// The game's unspent-point badges are 7×7 art (`overlay::POINT_BADGE_*`), one hud px each.
pub const HV_BADGE: u32 = 7;
/// Rows the block covers: 0..29.
pub const HV_BLOCK_H: u32 = 29;
/// The EXP bar: 4 rows (frame · light · dark · frame), `HV_MARGIN` above the bottom edge.
pub const HV_EXP_H: u32 = 4;
/// ★ 2026-10-05: a BUFF CELL right of the bars (user: *"사용한 버프 스킬의 남은 지속시간을 좌측상단
/// HP SP바 우측에 표시 … 스킬아이콘과 숫자로"*): black outline · silver frame · the 12×12 skill icon
/// · under it the seconds left, white on the plate's dark — 16 × 23 hud px, cells 1 px apart,
/// the first `HV_BUFF_X_GAP` right of the bars' tip, top on the bars' top row.
pub const HV_BUFF_W: u32 = 16;
pub const HV_BUFF_H: u32 = 23;
pub const HV_BUFF_ICON: u32 = 12;
pub const HV_BUFF_GAP: u32 = 1;
pub const HV_BUFF_X_GAP: u32 = 3;
/// The game's target-monster panel (66 px at the right edge of the frame — the app moves it only
/// in landscape) and 2 px of air: a row of cells stops short of it and the next row starts under
/// the first cell. Portrait (176 wide) takes one cell a row; landscape seventeen.
pub const HV_BUFF_RIGHT_RESERVE: u32 = 68;

const HV_OUTLINE: u32 = 0xff00_0000;
const HV_SILVER: u32 = 0xffde_dbce;
const HV_SILVER_LO: u32 = 0xff9c_9684;
const HV_GAP: u32 = 0xff31_3431;
/// HP rows top to bottom: highlight · body ×4 · shadow (hero5's red, measured).
const HV_HP: [u32; 6] = [0xffef_2429, 0xff9c_1018, 0xff9c_1018, 0xff9c_1018, 0xff9c_1018, 0xff63_0008];
/// …and the same rows where the bar is empty.
const HV_HP_TRACK: [u32; 6] = [0xff4a_1418, 0xff2c_080c, 0xff2c_080c, 0xff2c_080c, 0xff2c_080c, 0xff1c_0406];
/// SP rows: highlight · body ×2 · shadow ×2 (hero5's blue).
const HV_SP: [u32; 5] = [0xff08_b2f7, 0xff00_719c, 0xff00_719c, 0xff00_5984, 0xff00_2431];
const HV_SP_TRACK: [u32; 5] = [0xff0c_3a50, 0xff06_222e, 0xff06_222e, 0xff04_1a24, 0xff02_0c10];
const HV_HP_INK: u32 = 0xffde_aeb5;
const HV_HP_INK_HI: u32 = 0xfff7_b6b5;
const HV_HP_BOX: u32 = 0xff31_0808;
const HV_HP_BOX_LO: u32 = 0xff21_0408;
const HV_SP_INK: u32 = 0xffad_e3f7;
const HV_SP_BOX: u32 = 0xff08_3c52;
const HV_PLATE_FILL: u32 = 0xff10_1010;
const HV_LV_INK: u32 = 0xfff4_f4f4;
/// The ring's inside, behind the head.
const HV_RING_IN: u32 = 0xff14_1820;
/// The rim, lit from the upper left: outer row dark → light, inner row dark → light.
const HV_RIM_OUT: (u32, u32) = (0xff7b_7556, 0xfffb_f7b4);
const HV_RIM_IN: (u32, u32) = (0xff5a_5542, 0xffc6_be8c);
/// EXP earned: grey frame, cream over tan. EXP still to earn: black frame, orange over brown.
const HV_EXP_FRAME: u32 = 0xff63_6164;
const HV_EXP_HI: u32 = 0xfff6_f3a4;
const HV_EXP_LO: u32 = 0xffce_9663;
const HV_EXP_TRACK_HI: u32 = 0xffb5_5500;
const HV_EXP_TRACK_LO: u32 = 0xff73_3400;

/// The digits, 3×5, one row per byte, bit 2 = the left column.
const HV_DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b110, 0b010, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

/// Screen pixels per hud pixel for a game rect `game_h` px tall that shows `logical_h`
/// guest rows: the game's own pixel, to the nearest whole pixel, never below 2.
pub fn hv_unit(game_h: u32, logical_h: u32) -> u32 {
    if logical_h == 0 {
        return 2;
    }
    ((game_h + logical_h / 2) / logical_h).max(2)
}

/// Where the block sits and how big its parts are. Screen px unless a name says `_l`
/// (hud px, relative to the block's origin).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HvGeom {
    /// Screen px per hud px.
    pub u: u32,
    /// The block's origin (its top-left hud pixel), screen px.
    pub x: u32,
    pub y: u32,
    /// The bars' right end on their top row, hud px.
    pub bar_r_l: u32,
    /// The game frame's width in hud px (= guest px) — where the buff cells must stop.
    pub lw_l: u32,
}

impl HvGeom {
    /// The block for a game rect `(x, y, w, h)` showing `lw × lh` guest pixels.
    pub fn new(game: (u32, u32, u32, u32), lw: u32, lh: u32) -> HvGeom {
        let u = hv_unit(game.3, lh);
        // hero5's 89 of 400 on a wide screen; on the 176-wide portrait game, 45 % of it.
        let bar_r_l = (lw * 45 / 100).clamp(HV_BAR_R_MIN, HV_BAR_R_MAX);
        HvGeom { u, x: game.0 + HV_MARGIN * u, y: game.1 + HV_MARGIN * u, bar_r_l, lw_l: lw }
    }

    fn at(&self, lx: u32, ly: u32, lw: u32, lh: u32) -> (u32, u32, u32, u32) {
        (self.x + lx * self.u, self.y + ly * self.u, lw * self.u, lh * self.u)
    }

    /// The ring's square.
    pub fn ring(&self) -> (u32, u32, u32, u32) {
        self.at(0, 0, HV_RING, HV_RING)
    }

    /// The bars as they SHOW (from the ring's right edge to the slant's tip, rows 1..=19).
    pub fn bars(&self) -> (u32, u32, u32, u32) {
        self.at(HV_RING, 1, self.bar_r_l + 1 - HV_RING, 19)
    }

    /// Unspent stat (`i == 0`) / skill (`i == 1`) point badge, beside the level plate.
    pub fn badge(&self, i: u32) -> (u32, u32, u32, u32) {
        self.at(HV_PLATE_R + 2 + i * (HV_BADGE + 1), HV_PLATE_Y0 + 1, HV_BADGE, HV_BADGE)
    }

    /// Buff cell `i` (0 = nearest the bars), screen px. ⛔ Not part of [`Self::bbox`]: the block's
    /// box is a layout obstacle, and widening it would send a saved layout back to the defaults.
    pub fn buff_cell(&self, i: u32) -> (u32, u32, u32, u32) {
        let (lx, ly) = self.buff_at_l(i);
        self.at(lx, ly, HV_BUFF_W, HV_BUFF_H)
    }

    /// Cells a row: as many as end short of [`HV_BUFF_RIGHT_RESERVE`], never fewer than one.
    fn buff_cols(&self) -> u32 {
        let x0 = self.bar_r_l + HV_BUFF_X_GAP;
        let room = self.lw_l.saturating_sub(HV_BUFF_RIGHT_RESERVE).saturating_sub(x0) + HV_BUFF_GAP;
        (room / (HV_BUFF_W + HV_BUFF_GAP)).max(1)
    }

    /// Cell `i`'s top-left, hud px.
    fn buff_at_l(&self, i: u32) -> (u32, u32) {
        let cols = self.buff_cols();
        (self.bar_r_l + HV_BUFF_X_GAP + (i % cols) * (HV_BUFF_W + HV_BUFF_GAP), 1 + (i / cols) * (HV_BUFF_H + HV_BUFF_GAP))
    }

    /// Everything the block can cover — the badges included even while none are shown, so a
    /// level-up never lands one on something else.
    pub fn bbox(&self) -> (u32, u32, u32, u32) {
        let right = (self.bar_r_l + 1).max(HV_PLATE_R + 2 + 2 * HV_BADGE + 1);
        self.at(0, 0, right, HV_BLOCK_H)
    }
}

/// The EXP bar's rectangle (screen px) and its width in hud px, for the same game rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HvExp {
    pub u: u32,
    pub x: u32,
    pub y: u32,
    pub w_l: u32,
}

impl HvExp {
    pub fn new(game: (u32, u32, u32, u32), lw: u32, lh: u32) -> HvExp {
        let u = hv_unit(game.3, lh);
        // ~36 % of the width, centred: clear of the stick on the left and of the quickbar's
        // page button on the right at the default seats.
        let w_l = (lw * 36 / 100).clamp(40, 200).min(game.2 / u.max(1));
        let x = game.0 + game.2.saturating_sub(w_l * u) / 2;
        let y = (game.1 + game.3).saturating_sub((HV_MARGIN + HV_EXP_H) * u);
        HvExp { u, x, y, w_l }
    }

    pub fn rect(&self) -> (u32, u32, u32, u32) {
        (self.x, self.y, self.w_l * self.u, HV_EXP_H * self.u)
    }
}

/// A picture for the ring: `w × h`, row-major ARGB, alpha 0 = transparent.
#[derive(Clone, Copy)]
pub struct HvIcon<'a> {
    pub w: u32,
    pub h: u32,
    pub px: &'a [u32],
}

/// A buff cell's content: the skill icon (if the atlas has it) and the whole seconds left.
#[derive(Clone, Copy)]
pub struct HvBuff<'a> {
    pub icon: Option<HvIcon<'a>>,
    /// Whole seconds left, or [`HV_BUFF_ON`] for a buff that is ON until switched off.
    pub secs: u32,
    /// ★ 2026-10-07 (2) 격분: a 0..=100 gauge drawn as a bar on the plate row above the seconds (`None` for the rest).
    pub gauge: Option<u32>,
}

/// The gauge bar's colours: the filled part (the rage's red) and the empty rest.
const HV_GAUGE_FILL: u32 = 0xffe6_3c2c;
const HV_GAUGE_REST: u32 = 0xff3a_1c18;

/// ★ 2026-10-07 (user: 분노태세 *"on 상태일떄는 버프기 표시하는 영역에 남은 지속시간 대신 on이라고 표시"*): a cell's
/// `secs` that reads ON instead of a number (= `hero2java::H2_BUFF_ON`).
pub const HV_BUFF_ON: u32 = u16::MAX as u32;

/// The letters O and N, 4×5 (a 3-wide N reads as M), bit 3 = the left column.
const HV_ON: [[u8; 5]; 2] = [[0b0110, 0b1001, 0b1001, 0b1001, 0b0110], [0b1001, 0b1101, 0b1011, 0b1001, 0b1001]];

/// What the block shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HvStats {
    pub hp: u32,
    pub hp_max: u32,
    pub sp: u32,
    pub sp_max: u32,
    pub level: u32,
}

/// `color` over the screen rect, opaque, clipped to the window.
fn hv_rect(buf: &mut [u32], win_w: u32, win_h: u32, r: (u32, u32, u32, u32), color: u32) {
    let (x, y, w, h) = r;
    let x1 = x.saturating_add(w).min(win_w);
    let y1 = y.saturating_add(h).min(win_h);
    if x >= x1 {
        return;
    }
    for yy in y..y1 {
        let row = (yy as usize) * (win_w as usize);
        if let Some(s) = buf.get_mut(row + x as usize..row + x1 as usize) {
            s.fill(color);
        }
    }
}

/// hud pixels `lx0..lx1` of row `ly` (block-relative) in `color`.
fn hv_run(buf: &mut [u32], win_w: u32, win_h: u32, g: &HvGeom, lx0: u32, lx1: u32, ly: u32, color: u32) {
    if lx1 > lx0 {
        hv_rect(buf, win_w, win_h, g.at(lx0, ly, lx1 - lx0, 1), color);
    }
}

/// The decimal digits of `v`, most significant first.
fn hv_digits(v: u32) -> Vec<u8> {
    v.to_string().bytes().map(|b| b - b'0').collect()
}

/// Width of `n` digits in hud px (3 each, 1 apart).
fn hv_text_w(n: u32) -> u32 {
    (n * 4).saturating_sub(1)
}

/// Digits at hud px `(lx, ly)`; row 0 in `ink_top`, the other four in `ink`.
fn hv_draw_number(buf: &mut [u32], win_w: u32, win_h: u32, g: &HvGeom, digits: &[u8], lx: u32, ly: u32, ink: u32, ink_top: u32) {
    for (i, d) in digits.iter().enumerate() {
        let glyph = HV_DIGITS[usize::from(*d).min(9)];
        for (r, bits) in glyph.iter().enumerate() {
            for c in 0..3u32 {
                if bits & (0b100 >> c) != 0 {
                    let x = lx + i as u32 * 4 + c;
                    hv_run(buf, win_w, win_h, g, x, x + 1, ly + r as u32, if r == 0 { ink_top } else { ink });
                }
            }
        }
    }
}

/// The right end (inclusive, the outline pixel) of bar row `ly`: 45° slants, HP rows 1..=10
/// from `bar_r` down-left, SP rows 11..=19 from one short of it — which leaves the notch
/// between the two bars that hero5's block has.
pub fn hv_bar_end(bar_r_l: u32, ly: u32) -> u32 {
    if ly <= 9 {
        bar_r_l - (ly - 1)
    } else if ly == 10 {
        // The separator is SP's top outline as much as HP's bottom one: it runs out to the
        // tip of SP's top row.
        bar_r_l
    } else {
        bar_r_l - (ly - 10)
    }
}

/// The fill's edge in hud px for `cur / max` — `HV_FILL_X0` at 0 %, the bar's inner end at
/// 100 %. A column is filled when its left edge is short of this.
fn hv_fill_edge(bar_r_l: u32, cur: u32, max: u32) -> f64 {
    let frac = if max == 0 { 0.0 } else { f64::from(cur.min(max)) / f64::from(max) };
    let span = f64::from(bar_r_l.saturating_sub(2 + HV_FILL_X0));
    f64::from(HV_FILL_X0) + frac * span
}

/// The whole block: bars first, then the level plate, then the ring over both.
pub fn hv_draw_status(buf: &mut [u32], win_w: u32, win_h: u32, g: &HvGeom, s: &HvStats, head: Option<HvIcon>) {
    if g.u == 0 {
        return;
    }
    let r = g.bar_r_l;
    // ── bars ──
    let hp_edge = hv_fill_edge(r, s.hp, s.hp_max);
    let sp_edge = hv_fill_edge(r, s.sp, s.sp_max);
    for ly in 1..=19u32 {
        let end = hv_bar_end(r, ly);
        match ly {
            // Outline rows: black to the end.
            1 | 10 | 19 => hv_run(buf, win_w, win_h, g, HV_BAR_X0, end + 1, ly, HV_OUTLINE),
            // The frame's lines: silver, then the outline pixel.
            2 | 11 | 18 => {
                hv_run(buf, win_w, win_h, g, HV_BAR_X0, end, ly, if ly == 18 { HV_SILVER_LO } else { HV_SILVER });
                hv_run(buf, win_w, win_h, g, end, end + 1, ly, HV_OUTLINE);
            }
            _ => {
                let inner_end = end.saturating_sub(1);
                if ly == 3 || ly == 12 {
                    hv_run(buf, win_w, win_h, g, HV_BAR_X0, inner_end, ly, HV_GAP);
                } else {
                    let (fill, track, edge) = if ly <= 9 {
                        let k = (ly - 4) as usize;
                        (HV_HP[k], HV_HP_TRACK[k], hp_edge)
                    } else {
                        let k = (ly - 13) as usize;
                        (HV_SP[k], HV_SP_TRACK[k], sp_edge)
                    };
                    // Filled columns are those whose left edge is short of `edge`.
                    let split = (edge.ceil().max(f64::from(HV_BAR_X0)) as u32).min(inner_end);
                    hv_run(buf, win_w, win_h, g, HV_BAR_X0, split, ly, fill);
                    hv_run(buf, win_w, win_h, g, split, inner_end, ly, track);
                }
                // The slant: silver inside, outline outside.
                hv_run(buf, win_w, win_h, g, inner_end, end, ly, if ly >= 17 { HV_SILVER_LO } else { HV_SILVER });
                hv_run(buf, win_w, win_h, g, end, end + 1, ly, HV_OUTLINE);
            }
        }
    }
    // The values, centred on each bar's visible middle, in a dark box as tall as the bar.
    for (value, top, rows, ink, ink_top, bx, bx_lo) in [
        (s.hp, 4u32, 6u32, HV_HP_INK, HV_HP_INK_HI, HV_HP_BOX, HV_HP_BOX_LO),
        (s.sp, 13, 5, HV_SP_INK, HV_SP_INK, HV_SP_BOX, HV_SP_BOX),
    ] {
        let digits = hv_digits(value);
        let tw = hv_text_w(digits.len() as u32);
        let visible_end = hv_bar_end(r, top).saturating_sub(2);
        let mid = (HV_RING + 1 + visible_end) / 2;
        let x0 = mid.saturating_sub(tw / 2).max(HV_RING + 2);
        for ly in top..top + rows {
            let c = if ly + 1 == top + rows && rows == 6 { bx_lo } else { bx };
            hv_run(buf, win_w, win_h, g, x0 - 1, x0 + tw + 1, ly, c);
        }
        hv_draw_number(buf, win_w, win_h, g, &digits, x0, top, ink, ink_top);
    }

    // ── level plate ──
    let y0 = HV_PLATE_Y0;
    for ly in y0..=y0 + 8 {
        let end = HV_PLATE_R - (ly - y0);
        if ly == y0 || ly == y0 + 8 {
            hv_run(buf, win_w, win_h, g, HV_PLATE_X0, end + 1, ly, HV_OUTLINE);
            continue;
        }
        let line = if ly == y0 + 7 { HV_SILVER_LO } else { HV_SILVER };
        hv_run(buf, win_w, win_h, g, HV_PLATE_X0, HV_PLATE_X0 + 1, ly, HV_OUTLINE);
        if ly == y0 + 1 || ly == y0 + 7 {
            hv_run(buf, win_w, win_h, g, HV_PLATE_X0 + 1, end, ly, line);
        } else {
            hv_run(buf, win_w, win_h, g, HV_PLATE_X0 + 1, HV_PLATE_X0 + 2, ly, HV_SILVER);
            hv_run(buf, win_w, win_h, g, HV_PLATE_X0 + 2, end - 1, ly, HV_PLATE_FILL);
            hv_run(buf, win_w, win_h, g, end - 1, end, ly, line);
        }
        hv_run(buf, win_w, win_h, g, end, end + 1, ly, HV_OUTLINE);
    }
    let digits = hv_digits(s.level);
    let tw = hv_text_w(digits.len() as u32);
    let visible_end = (HV_PLATE_R - 4).saturating_sub(2);
    let mid = (HV_RING + 1 + visible_end) / 2;
    hv_draw_number(buf, win_w, win_h, g, &digits, mid.saturating_sub(tw / 2), y0 + 2, HV_LV_INK, HV_LV_INK);

    // ── ring, over both ──
    hv_draw_ring(buf, win_w, win_h, g, head);
}

/// The buff cells, left to right in list order (the oldest buff nearest the bars):
/// ```text
/// row 0        outline
/// row 1        silver           col 0 outline · col 1 silver · cols 2..13 icon · col 14 silver lo · col 15 outline
/// rows 2..13   the icon (dark ground where it is transparent)
/// row 14       silver lo
/// rows 15..21  the plate's dark, the seconds in white on rows 16..20, centred (★ 2026-10-07 (2): a cell with a gauge
///              carries it as a bar on row 15, cols 2..13 — the filled share red, the rest dark red)
/// row 22       outline
/// ```
pub fn hv_draw_buffs(buf: &mut [u32], win_w: u32, win_h: u32, g: &HvGeom, buffs: &[HvBuff]) {
    if g.u == 0 {
        return;
    }
    let (w, h, n) = (HV_BUFF_W, HV_BUFF_H, HV_BUFF_ICON);
    for (i, b) in buffs.iter().enumerate() {
        let (x0, y0) = g.buff_at_l(i as u32);
        hv_run(buf, win_w, win_h, g, x0, x0 + w, y0, HV_OUTLINE);
        hv_run(buf, win_w, win_h, g, x0, x0 + w, y0 + h - 1, HV_OUTLINE);
        for ly in y0 + 1..y0 + h - 1 {
            hv_run(buf, win_w, win_h, g, x0, x0 + 1, ly, HV_OUTLINE);
            hv_run(buf, win_w, win_h, g, x0 + w - 1, x0 + w, ly, HV_OUTLINE);
        }
        hv_run(buf, win_w, win_h, g, x0 + 1, x0 + w - 1, y0 + 1, HV_SILVER);
        hv_run(buf, win_w, win_h, g, x0 + 1, x0 + w - 1, y0 + 2 + n, HV_SILVER_LO);
        for ly in y0 + 2..y0 + 2 + n {
            hv_run(buf, win_w, win_h, g, x0 + 1, x0 + 2, ly, HV_SILVER);
            hv_run(buf, win_w, win_h, g, x0 + w - 2, x0 + w - 1, ly, HV_SILVER_LO);
        }
        // The icon, centred in its 12 × 12 if the atlas cell is smaller.
        let (ix, iy) = b.icon.map_or((0, 0), |ic| ((n as i64 - ic.w as i64) / 2, (n as i64 - ic.h as i64) / 2));
        for ly in 0..n {
            for lx in 0..n {
                let p = b.icon.and_then(|ic| {
                    let (sx, sy) = (lx as i64 - ix, ly as i64 - iy);
                    if sx < 0 || sy < 0 || sx >= ic.w as i64 || sy >= ic.h as i64 {
                        return None;
                    }
                    ic.px.get((sy as u32 * ic.w + sx as u32) as usize).copied().filter(|p| p >> 24 != 0)
                });
                let c = p.map_or(HV_RING_IN, |p| p | 0xff00_0000);
                hv_run(buf, win_w, win_h, g, x0 + 2 + lx, x0 + 3 + lx, y0 + 2 + ly, c);
            }
        }
        for ly in y0 + 3 + n..y0 + h - 1 {
            hv_run(buf, win_w, win_h, g, x0 + 1, x0 + w - 1, ly, HV_PLATE_FILL);
        }
        if let Some(gauge) = b.gauge {
            let span = w - 4;
            let filled = (span * gauge.min(100) + 50) / 100;
            hv_run(buf, win_w, win_h, g, x0 + 2, x0 + 2 + filled, y0 + 3 + n, HV_GAUGE_FILL);
            hv_run(buf, win_w, win_h, g, x0 + 2 + filled, x0 + 2 + span, y0 + 3 + n, HV_GAUGE_REST);
        }
        if b.secs == HV_BUFF_ON {
            // 4 px glyphs, 5 apart: 9 px of text
            let lx = x0 + (w - 9) / 2;
            for (i, glyph) in HV_ON.iter().enumerate() {
                for (r, bits) in glyph.iter().enumerate() {
                    for c in 0..4u32 {
                        if bits & (0b1000 >> c) != 0 {
                            let x = lx + i as u32 * 5 + c;
                            hv_run(buf, win_w, win_h, g, x, x + 1, y0 + 4 + n + r as u32, HV_LV_INK);
                        }
                    }
                }
            }
            continue;
        }
        let digits = hv_digits(b.secs.min(999));
        let tw = hv_text_w(digits.len() as u32);
        hv_draw_number(buf, win_w, win_h, g, &digits, x0 + (w - tw) / 2, y0 + 4 + n, HV_LV_INK, HV_LV_INK);
    }
}

/// A 0..=1 mix of two opaque colours.
fn hv_mix(a: u32, b: u32, t: f64) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let (x, y) = (f64::from((a >> s) & 0xff), f64::from((b >> s) & 0xff));
        ((x + (y - x) * t).round() as u32) & 0xff
    };
    0xff00_0000 | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// The ring and the head inside it. Radii in hud px from the centre `(12, 12)`: outline
/// 11..12, rim 9..11 (lit from the upper left), inner outline 8..9, the inside below 8.
fn hv_draw_ring(buf: &mut [u32], win_w: u32, win_h: u32, g: &HvGeom, head: Option<HvIcon>) {
    let c = f64::from(HV_RING) / 2.0;
    // Where the head's top-left lands: centred.
    let (hx0, hy0) = match head {
        Some(h) => ((HV_RING as i64 - h.w as i64) / 2, (HV_RING as i64 - h.h as i64) / 2),
        None => (0, 0),
    };
    for ly in 0..HV_RING {
        for lx in 0..HV_RING {
            let (dx, dy) = (f64::from(lx) + 0.5 - c, f64::from(ly) + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt();
            let color = if d >= 12.0 {
                continue;
            } else if d >= 11.0 || (8.0..9.0).contains(&d) {
                HV_OUTLINE
            } else if d >= 9.0 {
                let lit = 0.5 - 0.5 * (0.6 * dx + 0.8 * dy) / d;
                let (lo, hi) = if d >= 10.0 { HV_RIM_OUT } else { HV_RIM_IN };
                hv_mix(lo, hi, lit)
            } else {
                // Inside: the head's pixel if it has one here, the dark ground if not.
                let hp = head.and_then(|h| {
                    let (sx, sy) = (lx as i64 - hx0, ly as i64 - hy0);
                    if sx < 0 || sy < 0 || sx >= h.w as i64 || sy >= h.h as i64 {
                        return None;
                    }
                    h.px.get((sy as u32 * h.w + sx as u32) as usize).copied().filter(|p| p >> 24 != 0)
                });
                hp.map_or(HV_RING_IN, |p| p | 0xff00_0000)
            };
            hv_run(buf, win_w, win_h, g, lx, lx + 1, ly, color);
        }
    }
}

/// The EXP bar: the columns up to `exp / next` of its width earned, the rest still to go.
pub fn hv_draw_exp(buf: &mut [u32], win_w: u32, win_h: u32, e: &HvExp, exp: u32, next: u32) {
    if e.u == 0 || e.w_l == 0 {
        return;
    }
    let frac = if next == 0 { 0.0 } else { f64::from(exp.min(next)) / f64::from(next) };
    let earned = ((frac * f64::from(e.w_l)).round() as u32).min(e.w_l);
    let col = |lx0: u32, lx1: u32, ly: u32, color: u32, buf: &mut [u32]| {
        if lx1 > lx0 {
            hv_rect(buf, win_w, win_h, (e.x + lx0 * e.u, e.y + ly * e.u, (lx1 - lx0) * e.u, e.u), color);
        }
    };
    for (lx0, lx1, frame, hi, lo) in [(0, earned, HV_EXP_FRAME, HV_EXP_HI, HV_EXP_LO), (earned, e.w_l, HV_OUTLINE, HV_EXP_TRACK_HI, HV_EXP_TRACK_LO)] {
        col(lx0, lx1, 0, frame, buf);
        col(lx0, lx1, 1, hi, buf);
        col(lx0, lx1, 2, lo, buf);
        col(lx0, lx1, 3, frame, buf);
    }
    // The end caps are frame, whichever part reaches them.
    for (lx, frame) in [(0, if earned > 0 { HV_EXP_FRAME } else { HV_OUTLINE }), (e.w_l - 1, if earned >= e.w_l { HV_EXP_FRAME } else { HV_OUTLINE })] {
        col(lx, lx + 1, 1, frame, buf);
        col(lx, lx + 1, 2, frame, buf);
    }
}

/// ★★ 2026-10-22 가디언 레벨: the GL plate's height in hud px (outline · 5 digit rows · outline), above the EXP bar.
pub const HV_GL_H: u32 = 7;
/// The GL in the guardian's violet (the 가디언 slot colour `co.b[3]` 0xb050ff, lightened to read on the dark plate).
const HV_GL_INK: u32 = 0xffc0_80ff;
const HV_GL_INK_TOP: u32 = 0xffe8_d0ff;
const HV_GL_PLATE: u32 = 0xff18_0c24;

/// ★★ 2026-10-22 가디언 레벨 (user: *"레벨 99를 찍은 뒤부터는 경험치바 바로 위쪽에 숫자가 생기고"*): the GL on a small dark
/// plate centred on the EXP bar, one hud px above it. Nothing for GL 0.
pub fn hv_draw_gl(buf: &mut [u32], win_w: u32, win_h: u32, e: &HvExp, gl: u32) {
    if e.u == 0 || e.w_l == 0 || gl == 0 {
        return;
    }
    let u = e.u;
    let digits = hv_digits(gl);
    let pw = hv_text_w(digits.len() as u32) + 4;
    let x0 = e.x + e.w_l.saturating_sub(pw) / 2 * u;
    let y0 = e.y.saturating_sub((HV_GL_H + 1) * u);
    hv_rect(buf, win_w, win_h, (x0, y0, pw * u, HV_GL_H * u), HV_OUTLINE);
    hv_rect(buf, win_w, win_h, (x0 + u, y0 + u, (pw - 2) * u, (HV_GL_H - 2) * u), HV_GL_PLATE);
    for (i, d) in digits.iter().enumerate() {
        for (r, bits) in HV_DIGITS[usize::from(*d).min(9)].iter().enumerate() {
            for c in 0..3u32 {
                if bits & (0b100 >> c) != 0 {
                    let ink = if r == 0 { HV_GL_INK_TOP } else { HV_GL_INK };
                    hv_rect(buf, win_w, win_h, (x0 + (2 + i as u32 * 4 + c) * u, y0 + (1 + r as u32) * u, u, u), ink);
                }
            }
        }
    }
}

#[cfg(test)]
mod hud_view_tests {
    use super::*;

    const W: u32 = 2340;
    const H: u32 = 1080;
    const BG: u32 = 0xff12_3456;

    fn window() -> Vec<u32> {
        vec![BG; (W * H) as usize]
    }

    fn at(buf: &[u32], x: u32, y: u32) -> u32 {
        buf[(y * W + x) as usize]
    }

    /// The screen pixel in the middle of hud pixel `(lx, ly)` of the block.
    fn hud(buf: &[u32], g: &HvGeom, lx: u32, ly: u32) -> u32 {
        at(buf, g.x + lx * g.u + g.u / 2, g.y + ly * g.u + g.u / 2)
    }

    fn geom() -> HvGeom {
        HvGeom::new((0, 0, W, H), 476, 220)
    }

    #[test]
    fn a_hud_pixel_is_the_games_own_pixel() {
        assert_eq!(hv_unit(1080, 220), 5, "S25 landscape: 1080 / 220 = 4.9");
        assert_eq!(hv_unit(1080, 219), 5, "S9 landscape");
        assert_eq!(hv_unit(1350, 220), 6, "portrait 176x220 on a 1080-wide screen");
        assert_eq!(hv_unit(200, 220), 2, "never below two");
        assert_eq!(hv_unit(1080, 0), 2, "no logical size: a floor, not a division by zero");
    }

    #[test]
    fn the_block_fits_the_window_and_holds_its_parts() {
        for (w, h, lw, lh) in [(2340u32, 1080u32, 476u32, 220u32), (2220, 1080, 452, 219), (1080, 1350, 176, 220), (1600, 720, 488, 220)] {
            let g = HvGeom::new((0, 0, w, h), lw, lh);
            let (bx, by, bw, bh) = g.bbox();
            assert!(bx + bw <= w && by + bh <= h, "{w}x{h}: bbox {:?}", g.bbox());
            for part in [g.ring(), g.bars(), g.badge(0), g.badge(1)] {
                assert!(part.0 >= bx && part.1 >= by && part.0 + part.2 <= bx + bw && part.1 + part.3 <= by + bh, "{w}x{h}: {part:?} outside {:?}", g.bbox());
            }
            assert!(g.badge(0).0 + g.badge(0).2 < g.badge(1).0, "the two badges do not touch");
        }
        let portrait = HvGeom::new((0, 0, 1080, 1350), 176, 220);
        assert_eq!(portrait.bar_r_l, 79, "45 % of the 176-wide game");
        assert_eq!(geom().bar_r_l, HV_BAR_R_MAX);
    }

    #[test]
    fn the_bars_fill_from_the_ring_and_carry_their_values() {
        let g = geom();
        let mut full = window();
        hv_draw_status(&mut full, W, H, &g, &HvStats { hp: 700, hp_max: 700, sp: 0, sp_max: 390, level: 48 }, None);
        // Near the right end of HP's body row: filled. The same column of SP: track.
        let x = hv_bar_end(g.bar_r_l, 6) - 3;
        assert_eq!(hud(&full, &g, x, 6), HV_HP[2], "a full HP bar is red to its end");
        assert_eq!(hud(&full, &g, hv_bar_end(g.bar_r_l, 14) - 3, 14), HV_SP_TRACK[1], "an empty SP bar is all track");
        // Just right of the ring, an empty bar shows track, not a sliver of colour.
        assert_eq!(hud(&full, &g, HV_RING + 1, 14), HV_SP_TRACK[1]);
        // Half HP: the left part filled, the right part track.
        let mut half = window();
        hv_draw_status(&mut half, W, H, &g, &HvStats { hp: 350, hp_max: 700, sp: 390, sp_max: 390, level: 48 }, None);
        assert_eq!(hud(&half, &g, HV_RING + 1, 5), HV_HP[1]);
        assert_eq!(hud(&half, &g, hv_bar_end(g.bar_r_l, 5) - 3, 5), HV_HP_TRACK[1]);
        // The value box sits on the bar's visible middle.
        let mid = (HV_RING + 1 + hv_bar_end(g.bar_r_l, 4) - 2) / 2;
        let boxed = hud(&half, &g, mid, 9);
        assert!(boxed == HV_HP_BOX_LO || boxed == HV_HP_INK, "HP value box at its bottom row: {boxed:#010x}");
        // The slant: the outline pixel of HP's top body row, and nothing beyond it.
        assert_eq!(hud(&half, &g, hv_bar_end(g.bar_r_l, 4), 4), HV_OUTLINE);
        assert_eq!(hud(&half, &g, hv_bar_end(g.bar_r_l, 4) + 1, 4), BG);
    }

    #[test]
    fn the_notch_between_the_bars_follows_the_reference() {
        let r = HV_BAR_R_MAX;
        assert_eq!(hv_bar_end(r, 1), r);
        assert_eq!(hv_bar_end(r, 9), r - 8);
        assert_eq!(hv_bar_end(r, 10), r, "the separator runs out to SP's top-right corner");
        assert_eq!(hv_bar_end(r, 11), r - 1);
        assert_eq!(hv_bar_end(r, 19), r - 9);
    }

    #[test]
    fn the_head_is_drawn_inside_the_ring_and_nowhere_else() {
        let g = geom();
        // A 16x14 head: all opaque white except a transparent top-left pixel.
        let mut px = vec![0xffff_ffffu32; 16 * 14];
        px[0] = 0;
        let mut buf = window();
        hv_draw_status(&mut buf, W, H, &g, &HvStats { hp: 1, hp_max: 1, sp: 1, sp_max: 1, level: 1 }, Some(HvIcon { w: 16, h: 14, px: &px }));
        assert_eq!(hud(&buf, &g, 12, 12), 0xffff_ffff, "the ring's centre shows the head");
        // Head pixel (0,0) lands on hud (4,5) — outside radius 8, so the ring draws there.
        assert_ne!(hud(&buf, &g, 4, 5), 0xffff_ffff, "clipped to the inside");
        // The rim is lit: its top-left brighter than its bottom-right.
        let lum = |p: u32| ((p >> 16) & 0xff) + ((p >> 8) & 0xff) + (p & 0xff);
        assert!(lum(hud(&buf, &g, 5, 5)) > lum(hud(&buf, &g, 18, 18)), "lit from the upper left");
        // No head: the inside is the dark ground.
        let mut none = window();
        hv_draw_status(&mut none, W, H, &g, &HvStats::default(), None);
        assert_eq!(hud(&none, &g, 12, 12), HV_RING_IN);
    }

    #[test]
    fn nothing_is_drawn_outside_the_block() {
        let g = geom();
        let mut buf = window();
        hv_draw_status(&mut buf, W, H, &g, &HvStats { hp: 99999, hp_max: 99999, sp: 65535, sp_max: 65535, level: 99 }, None);
        let (bx, by, bw, bh) = g.bbox();
        for y in 0..H {
            for x in 0..W {
                if !(x >= bx && x < bx + bw && y >= by && y < by + bh) {
                    assert_eq!(at(&buf, x, y), BG, "({x},{y}) is outside the block");
                }
            }
        }
    }

    /// ★★ 2026-10-22 가디언 레벨: the GL sits on its plate just above the bar's centre, in violet, and touches nothing
    /// else — not the bar either; GL 0 draws nothing.
    #[test]
    fn the_gl_sits_just_above_the_exp_bar() {
        let e = HvExp::new((0, 0, W, H), 476, 220);
        let mut none = window();
        hv_draw_gl(&mut none, W, H, &e, 0);
        assert!(none.iter().all(|p| *p == BG), "GL 0: nothing");
        let mut buf = window();
        hv_draw_gl(&mut buf, W, H, &e, 137);
        let (bx, by, bw, _) = e.rect();
        let painted: Vec<(u32, u32)> = (0..H).flat_map(|y| (0..W).map(move |x| (x, y))).filter(|(x, y)| at(&buf, *x, *y) != BG).collect();
        let (x0, x1) = (painted.iter().map(|p| p.0).min().unwrap(), painted.iter().map(|p| p.0).max().unwrap());
        let (y0, y1) = (painted.iter().map(|p| p.1).min().unwrap(), painted.iter().map(|p| p.1).max().unwrap());
        assert!(y1 < by && by - y1 <= e.u + 1, "just above the bar: plate rows {y0}..={y1}, bar at {by}");
        assert_eq!(y1 + 1 - y0, HV_GL_H * e.u);
        assert!(x0 >= bx && x1 < bx + bw && ((x0 + x1) / 2).abs_diff(bx + bw / 2) <= 2 * e.u, "centred on the bar");
        assert_eq!((x1 + 1 - x0) / e.u, hv_text_w(3) + 4, "three digits on the plate");
        assert!(painted.iter().any(|(x, y)| at(&buf, *x, *y) == HV_GL_INK), "the violet digits");
    }

    #[test]
    fn the_exp_bar_splits_earned_from_owed() {
        let e = HvExp::new((0, 0, W, H), 476, 220);
        let (x, y, w, h) = e.rect();
        assert!(x + w <= W && y + h <= H && y > H / 2, "bottom half: {:?}", e.rect());
        assert!((x + w / 2).abs_diff(W / 2) <= 1, "centred: {:?}", e.rect());
        let row = |buf: &[u32], lx: u32, ly: u32| at(buf, e.x + lx * e.u + e.u / 2, e.y + ly * e.u + e.u / 2);
        let mut empty = window();
        hv_draw_exp(&mut empty, W, H, &e, 0, 1000);
        assert_eq!(row(&empty, e.w_l / 2, 1), HV_EXP_TRACK_HI);
        let mut full = window();
        hv_draw_exp(&mut full, W, H, &e, 1000, 1000);
        assert_eq!(row(&full, e.w_l / 2, 1), HV_EXP_HI);
        assert_eq!(row(&full, e.w_l / 2, 0), HV_EXP_FRAME);
        let mut half = window();
        hv_draw_exp(&mut half, W, H, &e, 500, 1000);
        assert_eq!(row(&half, e.w_l / 4, 2), HV_EXP_LO);
        assert_eq!(row(&half, e.w_l * 3 / 4, 2), HV_EXP_TRACK_LO);
        assert_eq!(row(&half, e.w_l * 3 / 4, 3), HV_OUTLINE);
        // Nothing outside its rectangle.
        for yy in 0..H {
            for xx in 0..W {
                if !(xx >= x && xx < x + w && yy >= y && yy < y + h) {
                    assert_eq!(at(&half, xx, yy), BG);
                }
            }
        }
    }

    #[test]
    fn a_tiny_window_clips_instead_of_panicking() {
        let (w, h) = (40u32, 30u32);
        let mut buf = vec![BG; (w * h) as usize];
        let g = HvGeom::new((0, 0, w, h), 176, 220);
        hv_draw_status(&mut buf, w, h, &g, &HvStats { hp: 5, hp_max: 9, sp: 1, sp_max: 2, level: 7 }, None);
        hv_draw_exp(&mut buf, w, h, &HvExp::new((0, 0, w, h), 176, 220), 3, 9);
        hv_draw_buffs(&mut buf, w, h, &g, &[HvBuff { icon: None, secs: 30, gauge: None }; 4]);
    }

    /// ★ 2026-10-05: the buff cells sit right of the bars, on their rows, one after another —
    /// the icon in its frame, the seconds under it — and draw nothing else.
    #[test]
    fn the_buff_cells_sit_right_of_the_bars_with_icon_and_seconds() {
        for (w, h, lw, lh) in [(2340u32, 1080u32, 476u32, 220u32), (1080, 1350, 176, 220)] {
            let g = HvGeom::new((0, 0, w, h), lw, lh);
            let (bx, by, bw, _) = g.bbox();
            let c0 = g.buff_cell(0);
            let c1 = g.buff_cell(1);
            assert!(c0.0 >= bx + bw && c0.0 > g.bars().0 + g.bars().2, "{w}x{h}: right of the block and the bars");
            assert_eq!(c0.1, by + g.u, "{w}x{h}: on the bars' top row");
            // Every cell ends short of the game's target-monster panel at the right edge.
            let panel_x = (lw - 66) * w / lw;
            for i in 0..4 {
                let c = g.buff_cell(i);
                assert!(c.0 + c.2 <= panel_x, "{w}x{h}: cell {i} {c:?} reaches the target panel at {panel_x}");
            }
            if lw > 300 {
                assert_eq!((c1.0, c1.1), (c0.0 + (HV_BUFF_W + HV_BUFF_GAP) * g.u, c0.1), "landscape: one row");
            } else {
                assert_eq!((c1.0, c1.1), (c0.0, c0.1 + (HV_BUFF_H + HV_BUFF_GAP) * g.u), "portrait: the next under the first");
            }
        }
        let g = geom();
        // A 12×12 icon, opaque green but for a transparent corner.
        let mut px = vec![0xff00_ff00u32; 12 * 12];
        px[0] = 0;
        let mut buf = window();
        hv_draw_buffs(&mut buf, W, H, &g, &[HvBuff { icon: Some(HvIcon { w: 12, h: 12, px: &px }), secs: 30, gauge: None }, HvBuff { icon: None, secs: 7, gauge: Some(50) }]);
        let x0 = g.bar_r_l + HV_BUFF_X_GAP;
        assert_eq!(hud(&buf, &g, x0, 1), HV_OUTLINE, "the cell's corner");
        assert_eq!(hud(&buf, &g, x0 + 1, 2), HV_SILVER, "the frame");
        assert_eq!(hud(&buf, &g, x0 + 2, 3), HV_RING_IN, "a transparent icon pixel shows the dark ground");
        assert_eq!(hud(&buf, &g, x0 + 7, 8), 0xff00_ff00, "the icon");
        // "30": the 3's top row starts the number box, centred — (16 − 7) / 2 = 4 in.
        assert_eq!(hud(&buf, &g, x0 + 4, 17), HV_LV_INK, "the seconds' first digit");
        assert_eq!(hud(&buf, &g, x0 + 3, 17), HV_PLATE_FILL, "the number box around it");
        assert_eq!(hud(&buf, &g, x0, 23), HV_OUTLINE, "the bottom outline");
        // The second cell, no icon: dark ground, and its single digit "7" centred (16 − 3) / 2 = 6.
        let x1 = x0 + HV_BUFF_W + HV_BUFF_GAP;
        assert_eq!(hud(&buf, &g, x1 + 7, 8), HV_RING_IN);
        assert_eq!(hud(&buf, &g, x1 + 6, 17), HV_LV_INK);
        assert_eq!(hud(&buf, &g, x1 - 1, 10), BG, "the gap between cells");
        // Nothing outside the two cells.
        let (a, b) = (g.buff_cell(0), g.buff_cell(1));
        for y in 0..H {
            for x in 0..W {
                let inside = |r: (u32, u32, u32, u32)| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3;
                if !inside(a) && !inside(b) {
                    assert_eq!(at(&buf, x, y), BG, "({x},{y}) is outside the cells");
                }
            }
        }
    }

    /// ★ 2026-10-07: a buff that is ON (분노태세) writes ON — two 4-px letters centred, (16 − 9) / 2 = 3 in — not a number.
    #[test]
    fn an_on_buff_reads_on_instead_of_seconds() {
        let g = geom();
        let mut buf = vec![BG; (W * H) as usize];
        hv_draw_buffs(&mut buf, W, H, &g, &[HvBuff { icon: None, secs: HV_BUFF_ON, gauge: None }]);
        let x0 = g.bar_r_l + HV_BUFF_X_GAP;
        // O's top row: .##. at 3..7 · N's top row: #..# at 8..12
        let row = |ly: u32| (x0..x0 + HV_BUFF_W).map(|x| hud(&buf, &g, x, ly) == HV_LV_INK).collect::<Vec<bool>>();
        let ink = |r: Vec<bool>| r.iter().enumerate().filter(|(_, b)| **b).map(|(i, _)| i).collect::<Vec<usize>>();
        assert_eq!(ink(row(17)), vec![4, 5, 8, 11], "the top rows of O and N");
        assert_eq!(ink(row(18)), vec![3, 6, 8, 9, 11]);
        assert_eq!(ink(row(19)), vec![3, 6, 8, 10, 11]);
    }
}
