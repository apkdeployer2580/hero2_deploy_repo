#!/usr/bin/env python3
"""★★ 2026-10-16 「악마화」 — the beast skill window speaks the 악의 version of a skill while 악의 runs.

User: *"악의로 스킬이 바뀌면 야수 스킬창에서도 이름·아이콘 변경 … 스킬설명도 간단히"*.

The asset carries, AFTER the shipped data, one name + description pair per changed skill (`t/bsk.tdf` rows 26..37) and
one icon per changed skill (`img/bico` frames 13..18); the game never reads past its own 26 rows / 13 frames. While 악의
is on (STATE 2) the host EXCHANGES the shipped row / icon slot of each changed kind with its appended one — the table
`bw.b:Li;` the beast window, the detail window and the bottom HUD read (`i.a(I)[B(2·kind)`), and the icon array `cf.i`
the icon drawer `n.c(BII)V` reads by kind — and exchanges them back when it ends. An exchange keeps every object
referenced by the game's own arrays, so nothing is pinned.

`SKIN` order = the appended order = `guest_anchors::DemonPatch::SKIN_KINDS` (the contract; `test_demon_look` and the
asset build check both lists).

Descriptions follow the detail window's wrap (`1 + 6 / ASCII + 9 / syllable` ≤ `WRAP_PX` per line, at most three lines,
`;` = line break) — `check_text` refuses a line the window would cut mid-word.
"""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

ROW_FIRST = 26  # = demon_look.BSK_SHIPPED_ROWS
ICON_FIRST = 13  # = demon_look.ICON_FRAMES (shipped)
ICON = 12
WRAP_PX = 79  # «잠시동안 야수에게» fits (79), «검은 기운으로 범위» does not (85) — the 10-13 capture
MAX_LINES = 3

# (beast kind, name, description) — kind = the global beast skill id (bsk rows 2·kind / 2·kind + 1, bico frame kind)
SKIN = [
    (2, "추종", "{1분@간 싸울수록;강해지고 끝나면;}어둠@이 폭발한다"),
    (12, "돌진", "{15초@간 돌진하며;닿는 적을 밀친다"),
    (3, "지옥칼날", "켜면 }지옥 칼날@이;주위를 휘몰아친다;매초 $SP@ 소모"),
    (10, "어둠심판", "켜면 }어둠 번개@가;주위를 내리친다;매초 $SP@ 소모"),
    (8, "용암불", "{지옥불@이 지난;곳은 {5초@간;}용암@이 된다"),
    (11, "멸화", "잠시 뒤 {불길@이;터져 주위 적을;태운다"),
]
KINDS = [k for k, _n, _d in SKIN]
MARKS = "{}$@"


def line_px(line):
    """The detail window's width of one line (marks drawn as nothing)."""
    px = 1
    for ch in line:
        if ch in MARKS:
            continue
        px += 6 if ord(ch) < 0x80 else 9
    return px


def check_text():
    for kind, name, desc in SKIN:
        lines = desc.split(";")
        if len(lines) > MAX_LINES:
            raise SystemExit(f"demon_skin: {name}: {len(lines)} lines (the window shows {MAX_LINES})")
        for ln in lines:
            if line_px(ln) > WRAP_PX:
                raise SystemExit(f"demon_skin: {name}: line {ln!r} is {line_px(ln)} px (> {WRAP_PX}) — the window cuts it")
        opened = 0
        for ch in desc:
            if ch in "{}$":
                if opened:
                    raise SystemExit(f"demon_skin: {name}: a colour opens inside a colour")
                opened = 1
            elif ch == "@":
                if not opened:
                    raise SystemExit(f"demon_skin: {name}: '@' closes nothing")
                opened = 0
        if opened:
            raise SystemExit(f"demon_skin: {name}: a colour is left open")
        name.encode("euc-kr")
        desc.encode("euc-kr")


def bsk_append_rows():
    """The 12 rows appended after the shipped 26 (name, description per skill)."""
    check_text()
    out = []
    for _k, name, desc in SKIN:
        out += [name.encode("euc-kr"), desc.encode("euc-kr")]
    return out


# ── icons (48 × 48 painted, 4 × 4 box mean → 12 × 12, nearest colour of the sheet's palette) ─────────────────────────
S = 4 * ICON


def _grid():
    y, x = np.mgrid[0:S, 0:S].astype(np.float32)
    return (x + 0.5) / S, (y + 0.5) / S


def _mix(img, mask, rgb, a=1.0):
    m = np.clip(mask, 0, 1)[..., None] * a
    img[:] = img * (1 - m) + np.array(rgb, np.float32) * m


def _smooth(e0, e1, v):
    t = np.clip((v - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def _seg(x, y, ax, ay, bx, by):
    """Distance from (x, y) to the segment a–b."""
    vx, vy = bx - ax, by - ay
    t = np.clip(((x - ax) * vx + (y - ay) * vy) / (vx * vx + vy * vy), 0, 1)
    return np.hypot(x - ax - t * vx, y - ay - t * vy)


def _noise(seed):
    rng = np.random.default_rng(seed)
    n = rng.random((ICON, ICON)).astype(np.float32)
    return np.kron(n, np.ones((4, 4), np.float32))


def _follow():
    x, y = _grid()
    img = np.zeros((S, S, 3), np.float32)
    img[:] = np.array([31, 31, 63], np.float32) * (0.6 + 0.4 * (1 - y))[..., None]
    r = np.hypot(x - 0.5, y - 0.62)
    _mix(img, _smooth(0.46, 0.30, r), (63, 31, 127))  # the purple dome
    _mix(img, _smooth(0.30, 0.20, r), (95, 31, 63))
    _mix(img, _smooth(0.05, 0.0, np.abs(r - 0.36)) * (y < 0.75), (159, 95, 191), 0.9)  # its rim
    _mix(img, _smooth(0.10, 0.04, np.hypot(x - 0.36, y - 0.52)), (255, 63, 63))  # two eyes
    _mix(img, _smooth(0.10, 0.04, np.hypot(x - 0.64, y - 0.52)), (255, 63, 63))
    _mix(img, _smooth(0.045, 0.0, np.hypot(x - 0.36, y - 0.52)), (255, 191, 191))
    _mix(img, _smooth(0.045, 0.0, np.hypot(x - 0.64, y - 0.52)), (255, 191, 191))
    _mix(img, _smooth(0.07, 0.0, np.abs(y - 0.92)) * _smooth(0.5, 0.2, np.abs(x - 0.5)), (31, 31, 63))
    return img


def _charge():
    x, y = _grid()
    img = np.zeros((S, S, 3), np.float32)
    img[:] = np.array([63, 31, 63], np.float32) * (0.5 + 0.5 * y)[..., None]
    for (ax, ay, w, col) in [(0.05, 0.30, 0.07, (223, 95, 63)), (0.0, 0.55, 0.09, (255, 127, 63)), (0.08, 0.80, 0.06, (191, 63, 63))]:
        d = _seg(x, y, ax, ay, 0.62, ay - 0.10)
        _mix(img, _smooth(w, w * 0.3, d) * _smooth(0.0, 0.35, x), col)
    # the head: a dark horned silhouette leaning right
    head = _smooth(0.22, 0.17, np.hypot((x - 0.70) * 1.1, y - 0.50))
    _mix(img, head, (31, 31, 63))
    for hx in (0.62, 0.82):
        _mix(img, _smooth(0.05, 0.02, _seg(x, y, hx, 0.36, hx + 0.12, 0.10)), (191, 191, 191))
    _mix(img, _smooth(0.05, 0.0, np.hypot(x - 0.78, y - 0.50)), (255, 223, 63))
    return img


def _blades():
    x, y = _grid()
    img = np.zeros((S, S, 3), np.float32)
    r = np.hypot(x - 0.5, y - 0.5)
    img[:] = np.array([95, 31, 63], np.float32) * (1.1 - r)[..., None]
    _mix(img, _smooth(0.32, 0.05, r), (191, 31, 63), 0.6)  # the red pool
    th = np.arctan2(y - 0.5, x - 0.5)
    for k in range(3):
        a0 = k * 2 * np.pi / 3
        # a crescent: an arc band from a0 to a0 + 100°, thickest in the middle
        da = np.mod(th - a0, 2 * np.pi)
        along = da / np.deg2rad(100)
        band = (along < 1) * np.sin(np.clip(along, 0, 1) * np.pi)
        thick = 0.025 + 0.06 * band
        _mix(img, _smooth(thick, thick * 0.4, np.abs(r - 0.33)) * (along < 1), (223, 223, 255))
        _mix(img, _smooth(thick * 0.5, 0.0, np.abs(r - 0.36)) * (along < 1), (255, 255, 255))
    _mix(img, _smooth(0.10, 0.0, r), (255, 95, 63))
    return img


def _judge():
    x, y = _grid()
    img = np.zeros((S, S, 3), np.float32)
    img[:] = np.array([63, 31, 127], np.float32) * (1.2 - 0.9 * y)[..., None]
    _mix(img, _smooth(0.45, 0.0, np.hypot(x - 0.5, y - 0.05)), (95, 95, 127), 0.5)
    pts = [(0.62, 0.00), (0.40, 0.42), (0.58, 0.46), (0.36, 1.00)]
    d = np.minimum.reduce([_seg(x, y, *pts[i], *pts[i + 1]) for i in range(3)])
    _mix(img, _smooth(0.13, 0.05, d), (159, 95, 191), 0.8)  # the glow
    _mix(img, _smooth(0.07, 0.03, d), (255, 255, 0))
    _mix(img, _smooth(0.035, 0.0, d), (255, 255, 255))
    _mix(img, _smooth(0.12, 0.0, np.abs(y - 0.96)) * _smooth(0.4, 0.1, np.abs(x - 0.38)), (223, 223, 255), 0.6)
    return img


def _lava():
    x, y = _grid()
    img = np.zeros((S, S, 3), np.float32)
    img[:] = np.array([63, 31, 63], np.float32)
    n = _noise(11)
    ground = _smooth(0.42, 0.52, y + 0.06 * np.sin(x * 9))
    _mix(img, ground, (223, 95, 63))
    _mix(img, ground * (n > 0.45), (255, 159, 63))
    _mix(img, ground * (n > 0.78), (255, 255, 0))
    cracks = np.minimum(_seg(x, y, 0.1, 0.62, 0.5, 0.78), _seg(x, y, 0.5, 0.78, 0.92, 0.66))
    _mix(img, _smooth(0.04, 0.0, cracks), (63, 31, 63))
    for fx, h in ((0.25, 0.55), (0.55, 0.75), (0.80, 0.50)):
        fl = _smooth(0.10, 0.03, np.abs(x - fx) / np.clip(1.15 - (0.55 - y) / h * 1.0, 0.25, 1.2)) * (y < 0.55) * (y > 0.55 - h * 0.6)
        _mix(img, fl, (255, 127, 63))
        _mix(img, fl * _smooth(0.0, 0.5, y), (255, 223, 63))
    return img


def _blast():
    x, y = _grid()
    img = np.zeros((S, S, 3), np.float32)
    img[:] = np.array([31, 31, 63], np.float32) * 0.6
    r = np.hypot(x - 0.5, y - 0.55)
    th = np.arctan2(y - 0.55, x - 0.5)
    flame = 0.40 + 0.06 * np.sin(th * 7) + 0.05 * np.sin(th * 3 + 1)
    _mix(img, _smooth(flame, flame - 0.08, r), (159, 31, 63))
    _mix(img, _smooth(flame - 0.06, flame - 0.16, r), (255, 63, 63))
    _mix(img, _smooth(0.24, 0.14, r), (255, 127, 63))
    _mix(img, _smooth(0.15, 0.06, r), (255, 223, 63))
    _mix(img, _smooth(0.07, 0.0, r), (255, 255, 255))
    return img


PAINT = [_follow, _charge, _blades, _judge, _lava, _blast]


def icon_rgb(j):
    """Icon j (12 × 12 × 3, uint8) of SKIN[j]."""
    big = PAINT[j]()
    small = big.reshape(ICON, 4, ICON, 4, 3).mean(axis=(1, 3))
    return np.clip(np.round(small), 0, 255).astype(np.uint8)


def nearest_index(palette, rgb):
    best, bi = None, 1
    for i, c in enumerate(palette):
        if i == 0:
            continue
        d = sum((int(a) - int(b)) ** 2 for a, b in zip(c[:3], rgb))
        if best is None or d < best:
            best, bi = d, i
    return bi


def icon_pixels(palette, j):
    rgb = icon_rgb(j)
    return [[nearest_index(palette, rgb[y, x]) for x in range(ICON)] for y in range(ICON)]


def preview(out_png, mph=None, mpd=None):
    """The six icons beside the shipped ones they stand in for (8 × each)."""
    import io
    import zipfile

    from PIL import Image, ImageDraw

    import costume_sheets as cs

    if mph is None:
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        z = zipfile.ZipFile(os.path.join(root, "android_app", "app", "src", "main", "assets", "hero2_ktf.zip"))
        jar = zipfile.ZipFile(io.BytesIO(z.read([n for n in z.namelist() if n.endswith(".jar")][0])))
        mph, mpd = jar.read("img/bico.mph"), jar.read("img/bico_0.mpd")
    sh = cs.sheet_decode(mph, mpd, "img/bico")
    k = 8
    cell = ICON * k + 6
    im = Image.new("RGB", (len(SKIN) * cell * 2 + 8, ICON * k + 20), (24, 24, 30))
    d = ImageDraw.Draw(im)
    for j, (kind, _name, _desc) in enumerate(SKIN):
        for col, px in ((0, sh.frames[kind].pixels), (1, icon_pixels(sh.palette, j))):
            ox = j * cell * 2 + col * cell
            for y in range(ICON):
                for x in range(ICON):
                    v = px[y][x]
                    if v:
                        d.rectangle([ox + x * k, y * k, ox + x * k + k - 1, y * k + k - 1], fill=tuple(sh.palette[v][:3]))
        d.text((j * cell * 2, ICON * k + 4), f"{kind}", fill=(255, 255, 0))
    im.save(out_png)
    print(f"preview → {out_png}")


if __name__ == "__main__":
    check_text()
    for kind, name, desc in SKIN:
        print(kind, name, [line_px(ln) for ln in desc.split(";")])
    if len(sys.argv) > 2 and sys.argv[1] == "preview":
        preview(sys.argv[2])
