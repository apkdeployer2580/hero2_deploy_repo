//! The typeface the HOST's own windows are lettered in.
//!
//! ⛔ It is NOT "the game's font", and it cannot be. hero2's archive carries none:
//! the 148 jar members hold no font resource, and the guest binary never calls
//! `drawString` — its Graphics surface is `drawImage`/`drawRect`/`drawLine` only, so
//! the game blits its lettering as IMAGES. There is nothing to borrow. (hero3 is the
//! other case, and copying its `gamefont_data.rs` here would repeat the `icons.bin`
//! mistake: another game's art, worn confidently.)
//!
//! What this IS: `vendor/wie/fonts/neodgm.ttf` (둥근모꼴), the font the emulator core
//! already ships for guest text, rendered by us. Two properties make it the right
//! answer rather than merely an available one:
//!
//!  1. **It is a pixel font.** At a 16 px em it rasterises to 3 coverage levels, of
//!     which the middle one is 6 pixels in a whole line of text — i.e. it is 1-bit.
//!     Thresholded at that size and blown up by WHOLE pixels it behaves as a bitmap
//!     font, which is what keeps host lettering next to the game's own hard-edged
//!     lettering instead of floating above it as smooth modern type.
//!  2. **It has all 11,172 syllables.** That retires [`crate::hangul`]'s closed list
//!     for anything drawn through here — a label containing an unlisted syllable used
//!     to draw a SILENT BLANK (`draw_hangul` early-returns), so every new Korean
//!     string was a trap that only a device screenshot could spring.
//!
//! The cell is the classic CJK terminal grid: **16x16 per wide glyph, 8x16 per
//! narrow one**, measured (not assumed) from the font's own advances. Ascent is 12,
//! so the baseline sits on row 12 of the cell and Hangul reaches rows 1..14.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{Font, FontRef, ScaleFont};

/// Cell height in FONT pixels — one line of text before the integer scale.
pub const CELL_H: u32 = 16;

/// The em size we rasterise at. See the module docs: this is the size at which
/// neodgm is 1-bit, and rasterising anywhere else would hand us grey edges that the
/// integer scale would then magnify into mush.
const EM: f32 = 16.0;

/// Coverage at or above which a font pixel is ON. Only ~0.15 % of pixels land
/// between the extremes at [`EM`], so the exact threshold is not load-bearing; a
/// half-covered pixel belongs to the stroke.
const ON: f32 = 0.5;

/// One rasterised cell. `bits[row]` has bit `x` set for an inked pixel, bit 0 =
/// leftmost, so a 16-wide glyph fills the `u16` exactly.
#[derive(Clone, Copy)]
struct Glyph {
    /// Advance in font pixels — 16 for a wide glyph, 8 for a narrow one.
    w: u8,
    bits: [u16; CELL_H as usize],
}

fn font() -> &'static FontRef<'static> {
    static F: OnceLock<FontRef<'static>> = OnceLock::new();
    // ⛔ The same bytes are also linked into `wie_backend` (which renders GUEST text
    // with them). That duplicate is deliberate: what the host letters its own panels
    // in is a HOST decision, and sharing the core's handle would let a core update
    // silently restyle this app's windows. The cost is 650 KB in a 37 MB APK.
    F.get_or_init(|| {
        FontRef::try_from_slice(include_bytes!("../../vendor/wie/fonts/neodgm.ttf"))
            .expect("bundled neodgm.ttf is not a readable font")
    })
}

/// The rasterised cells seen so far. A panel is ~40 distinct characters, so this
/// fills on the first frame it is opened and is pure bit-blitting after that.
fn cache() -> &'static Mutex<HashMap<char, Glyph>> {
    static C: OnceLock<Mutex<HashMap<char, Glyph>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn rasterise(ch: char) -> Glyph {
    let f = font();
    let scaled = f.as_scaled(EM);
    let id = f.glyph_id(ch);
    // The font's own advance decides wide vs narrow — the alternative (deciding from
    // the code point) is a table that drifts from the font it claims to describe.
    let w = scaled.h_advance(id).round().clamp(0.0, CELL_H as f32) as u8;
    let mut bits = [0u16; CELL_H as usize];
    if let Some(outline) = f.outline_glyph(id.with_scale(EM)) {
        let b = outline.px_bounds();
        // `px_bounds` is relative to the glyph ORIGIN, which is on the baseline, so
        // rows above it are negative. Ascent puts the baseline in the cell.
        let x0 = b.min.x.floor() as i32;
        let y0 = scaled.ascent().round() as i32 + b.min.y.floor() as i32;
        outline.draw(|gx, gy, cov| {
            if cov < ON {
                return;
            }
            let px = x0 + gx as i32;
            let py = y0 + gy as i32;
            if (0..CELL_H as i32).contains(&px) && (0..CELL_H as i32).contains(&py) {
                bits[py as usize] |= 1u16 << px;
            }
        });
    }
    Glyph { w, bits }
}

fn glyph(ch: char) -> Glyph {
    // A poisoned cache must not take the panel down with it: rasterising afresh is
    // correct, just slower, and this is a drawing path.
    let Ok(mut c) = cache().lock() else {
        return rasterise(ch);
    };
    *c.entry(ch).or_insert_with(|| rasterise(ch))
}

/// Width of `s` in FONT pixels (before the integer scale).
pub fn width(s: &str) -> u32 {
    s.chars().map(|c| u32::from(glyph(c).w)).sum()
}

/// Width of `s` on screen at `scale`.
pub fn text_w(s: &str, scale: u32) -> u32 {
    width(s) * scale
}

/// Largest whole scale that keeps `s` inside `max_w` x `max_h`. Never 0 — a label
/// that cannot fit is drawn small and clipped rather than vanishing, because a
/// missing row reads as a bug while a cramped one reads as a cramped row.
pub fn fit_scale(s: &str, max_w: u32, max_h: u32) -> u32 {
    let w = width(s).max(1);
    (max_w / w).min(max_h / CELL_H).max(1)
}

/// Draw `s` with its top-left cell corner at `(x, y)`, each font pixel a
/// `scale` x `scale` block.
pub fn draw(buf: &mut [u32], win_w: u32, win_h: u32, x: u32, y: u32, scale: u32, s: &str, color: u32) {
    if scale == 0 {
        return;
    }
    let mut pen = x;
    for ch in s.chars() {
        let g = glyph(ch);
        for (row, &bits) in g.bits.iter().enumerate() {
            if bits == 0 {
                continue;
            }
            let py = y + row as u32 * scale;
            if py >= win_h {
                break;
            }
            for col in 0..u32::from(g.w) {
                if bits & (1u16 << col) == 0 {
                    continue;
                }
                let px = pen + col * scale;
                if px >= win_w {
                    break;
                }
                let h = scale.min(win_h - py);
                let w = scale.min(win_w - px);
                for yy in py..py + h {
                    let base = (yy * win_w + px) as usize;
                    buf[base..base + w as usize].fill(color);
                }
            }
        }
        pen += u32::from(g.w) * scale;
    }
}

/// Draw `s` centred in the horizontal span `[x, x + w)`.
pub fn draw_centered(buf: &mut [u32], win_w: u32, win_h: u32, x: u32, w: u32, y: u32, scale: u32, s: &str, color: u32) {
    let tw = text_w(s, scale);
    draw(buf, win_w, win_h, x + w.saturating_sub(tw) / 2, y, scale, s, color);
}
