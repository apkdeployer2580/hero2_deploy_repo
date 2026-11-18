#!/usr/bin/env python3
"""★★ 2026-10-16 — 엘릭서's own item icon (user: *"엘릭서가 현재 구 아이콘 사용되고있는데 하이포션 아이콘이 포도주 아이콘색상 좀
섞은 듯한 아이콘으로 사용해줘"*).

The game draws item icons by TYPE from the strip `img/glb` frame 17 (12 × 408, 34 cells; cell = type ≤ 7 ? type : type − 1),
so the elixir (type 21, like every orb) drew the blue orb. This appends cells after the shipped 34 — one per entry of
`OWN_CELLS` — and `guest_anchors::ItemIconPatch` draws a listed record as its display type (= cell + 1). The elixir cell is
the 하이포션 flask (type 25 → cell 24) with its liquid hue pulled toward the 포도주 bottle's (type 26 → cell 25): every
saturated pixel's hue moves `MIX` of the way to `WINE_HUE`, value and saturation kept, then the strip palette's nearest
colour (the strip is indexed; the palette is not touched). Outline · background · highlights stay the flask's.

`OWN_CELLS` order = the appended order = `ItemIconPatch::CELLS` (display type = 35 + its index).
"""

import colorsys
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import costume_sheets as cs  # noqa: E402

SHEET = "img/glb"
STRIP_FRAME = 17
CELL = 12
SHIPPED_CELLS = 34
HIGH_POTION_CELL = 24  # type 25 하이포션
WINE_CELL = 25  # type 26 포도주
WINE_HUE = 0.84  # 포도주 bottle's magenta-violet (its colours sit at 0.77 … 0.89)
MIX = 0.7
KEEP = {12, 14, 61}  # the cell background (black), the dark outline and the cork stay the flask's
# ★★ 2026-10-20 상점주인의 혼 (user: *"상점 아이템 구는 기존 구에서 색상만 빨간색으로 변경하면돼"*): the orb cell itself
# (type 21 → cell 20) with every saturated pixel's hue turned to red — value and saturation kept, the strip palette's
# nearest colour; the black background and the white highlights stay.
ORB_CELL = 20  # type 21 마법구
RED_HUE = 0.0
# ★★ 2026-10-21 비행서 (user: *"비행서는 귀환서 아이콘이 파란색위주의 색상으로 변경된 아이콘을 사용했으면"*): the 귀환서 scroll
# cell (type 28 → cell 27) with every non-background pixel's hue turned to blue — the scroll's parchment is low in
# saturation (0.2 … 0.6), so the saturation is lifted to at least `SKY_SAT` for the blue to read as blue; value kept;
# the strip palette's nearest colour. The black background stays.
RETURN_CELL = 27  # type 28 귀환서
SKY_HUE = 0.62
SKY_SAT = 0.55
# (type, id, name, painter) per appended cell — the painter takes (palette, strip_pixels) and returns 12 rows
OWN_CELLS = [(21, 7, "엘릭서", "elixir_pixels"), (21, 8, "상점주인의 혼", "shop_orb_pixels"), (28, 1, "비행서", "flight_pixels")]


def _rgb(palette, i):
    return tuple(palette[i][:3])


def _nearest(palette, rgb):
    best, bi = None, 1
    for i, c in enumerate(palette):
        if i == 0:
            continue
        d = sum((int(a) - int(b)) ** 2 for a, b in zip(c[:3], rgb))
        if best is None or d < best:
            best, bi = d, i
    return bi


def _blend_hue(h):
    d = (WINE_HUE - h + 0.5) % 1.0 - 0.5  # the short way round the circle
    return (h + MIX * d) % 1.0


def elixir_pixels(palette, strip_pixels):
    """The elixir cell (12 rows of palette indices) from the strip's 하이포션 cell."""
    out = []
    for y in range(CELL):
        row = []
        for x in range(CELL):
            v = strip_pixels[HIGH_POTION_CELL * CELL + y][x]
            r, g, b = (c / 255 for c in _rgb(palette, v))
            h, s, val = colorsys.rgb_to_hsv(r, g, b)
            if v in KEEP or s < 0.3:
                row.append(v)
                continue
            nr, ng, nb = colorsys.hsv_to_rgb(_blend_hue(h), s, val)
            row.append(_nearest(palette, (round(nr * 255), round(ng * 255), round(nb * 255))))
        out.append(row)
    return out


def _luma(rgb):
    r, g, b = rgb
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255


def shop_orb_pixels(palette, strip_pixels):
    """상점주인의 혼's cell: the orb cell (type 21 → cell 20) with every saturated pixel turned red.

    The blue orb's shading is in its HUE (cyan → deep blue at one value), so a plain hue swap flattens it to one red.
    Red carries shading in VALUE instead: each saturated pixel keeps its saturation and takes the value from its own
    luminance (normalised to the cell's brightest saturated pixel), so the gradient and the highlights survive.
    """
    cell = [strip_pixels[ORB_CELL * CELL + y] for y in range(CELL)]
    sat = []
    for row in cell:
        for v in row:
            r, g, b = (c / 255 for c in _rgb(palette, v))
            if v != 0 and colorsys.rgb_to_hsv(r, g, b)[1] >= 0.3:
                sat.append(_luma(_rgb(palette, v)))
    top = max(sat) if sat else 1.0
    out = []
    for row in cell:
        new = []
        for v in row:
            r, g, b = (c / 255 for c in _rgb(palette, v))
            _h, s, _val = colorsys.rgb_to_hsv(r, g, b)
            if v == 0 or s < 0.3:
                new.append(v)  # the keyed background, black and the white highlights stay
                continue
            val = min(1.0, 0.35 + 0.65 * _luma(_rgb(palette, v)) / top)
            nr, ng, nb = colorsys.hsv_to_rgb(RED_HUE, max(s, 0.75), val)
            new.append(_nearest(palette, (round(nr * 255), round(ng * 255), round(nb * 255))))
        out.append(new)
    return out


def flight_pixels(palette, strip_pixels):
    """비행서's cell: the 귀환서 scroll cell (type 28 → cell 27) turned blue, the black background kept."""
    out = []
    for y in range(CELL):
        row = []
        for x in range(CELL):
            v = strip_pixels[RETURN_CELL * CELL + y][x]
            r, g, b = (c / 255 for c in _rgb(palette, v))
            h, s, val = colorsys.rgb_to_hsv(r, g, b)
            if v == 0 or val < 0.05:
                row.append(v)  # the keyed background and black stay
                continue
            nr, ng, nb = colorsys.hsv_to_rgb(SKY_HUE, max(s, SKY_SAT), val)
            row.append(_nearest(palette, (round(nr * 255), round(ng * 255), round(nb * 255))))
        out.append(row)
    return out


PAINTERS = {"elixir_pixels": elixir_pixels, "shop_orb_pixels": shop_orb_pixels, "flight_pixels": flight_pixels}


def patch_glb(mph, mpd):
    """`img/glb` with the own-icon cells appended to frame 17 (the other frames and the palette byte-identical)."""
    sheet = cs.sheet_decode(mph, mpd, SHEET)
    strip = sheet.frames[STRIP_FRAME]
    if (strip.w, strip.h) != (CELL, CELL * SHIPPED_CELLS):
        raise ValueError(f"{SHEET} frame {STRIP_FRAME}: {strip.w}×{strip.h}, expected {CELL}×{CELL * SHIPPED_CELLS} (shipped)")
    rows = [list(r) for r in strip.pixels]
    for (ty, _id, _name, painter) in OWN_CELLS:
        assert ty in (21, 28), "only the orb and the 귀환서 types borrow an own cell so far"
        rows += PAINTERS[painter](sheet.palette, strip.pixels)
    frames = list(sheet.frames)
    frames[STRIP_FRAME] = cs.Frame(CELL, len(rows), rows, flags=strip.flags, lead=strip.lead, extra=strip.extra)
    new_mph, new_mpd = cs.sheet_encode(cs.Sheet(sheet.palette, frames, head=sheet.head, trns=sheet.trns), any_head=True)
    back = cs.sheet_decode(new_mph, new_mpd, SHEET)
    for k, (a, b) in enumerate(zip(back.frames, sheet.frames)):
        if k != STRIP_FRAME:
            assert (a.w, a.h, a.pixels) == (b.w, b.h, b.pixels), f"{SHEET} frame {k} moved"
    got = back.frames[STRIP_FRAME]
    assert (got.w, got.h) == (CELL, CELL * (SHIPPED_CELLS + len(OWN_CELLS))) and got.pixels[: CELL * SHIPPED_CELLS] == strip.pixels
    assert back.palette == sheet.palette
    return new_mph, new_mpd


def preview(out_png, mph=None, mpd=None):
    """하이포션 · 포도주 · 마법구 · 귀환서, then every own cell, at 10×."""
    import io
    import zipfile

    from PIL import Image, ImageDraw

    if mph is None:
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        z = zipfile.ZipFile(os.path.join(root, "android_app", "app", "src", "main", "assets", "hero2_ktf.zip"))
        jar = zipfile.ZipFile(io.BytesIO(z.read([n for n in z.namelist() if n.endswith(".jar")][0])))
        mph, mpd = jar.read(SHEET + ".mph"), jar.read(SHEET + "_0.mpd")
    sh = cs.sheet_decode(mph, mpd, SHEET)
    f = sh.frames[STRIP_FRAME]
    cells = [(f.pixels[HIGH_POTION_CELL * CELL:(HIGH_POTION_CELL + 1) * CELL], "하이포션"), (f.pixels[WINE_CELL * CELL:(WINE_CELL + 1) * CELL], "포도주")]
    cells.append((f.pixels[ORB_CELL * CELL:(ORB_CELL + 1) * CELL], "마법구"))
    cells.append((f.pixels[RETURN_CELL * CELL:(RETURN_CELL + 1) * CELL], "귀환서"))
    for k, (_ty, _id, name, painter) in enumerate(OWN_CELLS):
        if f.h < CELL * (SHIPPED_CELLS + k + 1):  # not in this asset yet: painted now, from the shipped cells
            cells.append((PAINTERS[painter](sh.palette, f.pixels), name))
        else:
            cells.append((f.pixels[(SHIPPED_CELLS + k) * CELL:(SHIPPED_CELLS + k + 1) * CELL], f"{name} (asset)"))
    k = 10
    im = Image.new("RGB", (len(cells) * (CELL * k + 8), CELL * k), (40, 40, 40))
    d = ImageDraw.Draw(im)
    for j, (px, _label) in enumerate(cells):
        for y in range(CELL):
            for x in range(CELL):
                v = px[y][x]
                if v:
                    ox = j * (CELL * k + 8)
                    d.rectangle([ox + x * k, y * k, ox + x * k + k - 1, y * k + k - 1], fill=_rgb(sh.palette, v))
    im.save(out_png)
    print(f"preview → {out_png}")


if __name__ == "__main__":
    if len(sys.argv) > 2 and sys.argv[1] == "preview":
        preview(sys.argv[2])
    else:
        print(__doc__)
