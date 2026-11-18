#!/usr/bin/env python3
"""월드맵 팝업 미리보기 — `worldmap_view.rs` 의 기하(`wm_panel` · 카메라 = 그림 폭의 2/5 · 길 선 · hero5 비례 단위)를
파이썬으로 따라 그려 폰 없이 모양을 본다. 픽셀 단위로 같지는 않다(글꼴 · 안티에일리어싱이 다르다) — 배치 · 크기 ·
라벨 겹침을 보는 용도.

    python tools_re/worldmap_preview.py out.png [맵번호=0] [W=2340] [H=1080]
"""

import os
import re
import struct
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RS = os.path.join(ROOT, "wie_android", "src", "worldmap_view.rs")
RGB = os.path.join(ROOT, "wie_android", "src", "worldmap.rgb565")
FONT = os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf")


def places():
    src = open(RS, encoding="utf-8").read()
    return [(n, int(x), int(y)) for n, x, y in re.findall(r'WmPlace \{ name: "([^"]+)", x: (\d+), y: (\d+) \}', src)]


def roads(src):
    consts = dict(re.findall(r"pub const (WM_\w+): usize = (\d+);", src))
    body = re.search(r"pub const WM_ROADS: &\[\(usize, usize\)\] = &\[(.*?)\];", src, re.S).group(1)
    return [(int(consts[a]), int(consts[b])) for a, b in re.findall(r"\((WM_\w+), (WM_\w+)\)", body)]


def place_index(src, m):
    body = re.search(r"pub fn wm_place_index\(map: u32\) -> usize \{\s*match map \{(.*?)\n    \}", src, re.S).group(1)
    names = [n for n, _, _ in places()]
    consts = dict(re.findall(r"pub const (WM_\w+): usize = (\d+);", src))
    for line in body.strip().splitlines():
        line = line.strip().rstrip(",")
        if not line or line.startswith("_"):
            continue
        pat, target = line.split("=>")
        nums = [int(t) for t in pat.replace("|", " ").split()]
        if m in nums:
            return int(consts[target.strip()])
    return 0


def panel(W, H):
    edge = 4
    if W >= H:
        fx, fw = W * 4 // 100, W - W * 8 // 100
    else:
        fx, fw = W * 2 // 100, W - W * 4 // 100
    fy = H * 65 // 1000
    fh = H - fy - H * 4 // 100
    iw, ih = fw - 2 * edge, fh - 2 * edge
    unit0 = max(1, min((ih + 120) // 240, (iw + 200) // 400))
    band_h = max(8, min(ih // 11, 24 * unit0))
    view_h = ih - band_h
    unit = max(1, min((view_h + 120) // 240, (iw + 200) // 400))
    band = (fx + edge, fy + edge, iw, band_h)
    view = (fx + edge, band[1] + band_h, iw, view_h)
    bd = max(4, band_h - 2)
    back = (band[0] + band[2] - bd - band_h // 2, band[1] + 1, bd, bd)
    mh = min(view_h, max(8, 17 * unit))
    mw = min(iw, max(16, 72 * unit))
    mini = (view[0], view[1] + view[3] - mh, mw, mh)
    return dict(frame=(fx, fy, fw, fh), band=band, view=view, back=back, mini=mini, unit=unit, edge=edge)


def main():
    out = sys.argv[1]
    m = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    W = int(sys.argv[3]) if len(sys.argv) > 3 else 2340
    H = int(sys.argv[4]) if len(sys.argv) > 4 else 1080
    src = open(RS, encoding="utf-8").read()
    pl = places()
    sel = place_index(src, m)
    with open(RGB, "rb") as f:
        iw, ih = struct.unpack("<II", f.read(8))
        pic = Image.frombytes("RGB", (iw, ih), f.read(), "raw", "BGR;16")
    p = panel(W, H)
    vx, vy, vw, vh = p["view"]
    u = p["unit"]
    # camera: WM_CAM_FRACTION of the picture's width, centred on the place, clamped
    num, den = map(int, re.search(r"WM_CAM_FRACTION: \(u32, u32\) = \((\d+), (\d+)\)", src).groups())
    cw = iw * num // den
    ch = min(ih, cw * vh // vw)
    if ch == ih:
        cw = min(iw, ch * vw // vh)
    px, py = pl[sel][1] * iw // 1000, pl[sel][2] * ih // 1000
    cx = min(max(0, px - cw // 2), iw - cw)
    cy = min(max(0, py - ch // 2), ih - ch)
    img = Image.new("RGB", (W, H), (40, 44, 36))
    d = ImageDraw.Draw(img, "RGBA")
    fx, fy, fw, fh = p["frame"]
    d.rectangle((fx, fy, fx + fw - 1, fy + fh - 1), fill=(0x3D, 0x2F, 0x1B))
    e = p["edge"] // 2
    d.rectangle((fx + e, fy + e, fx + fw - 1 - e, fy + fh - 1 - e), fill=(0x9B, 0x8A, 0x71))
    bx, by, bw, bh = p["band"]
    d.rectangle((bx, by, bx + bw - 1, by + bh - 1), fill=(0, 0, 0))
    view = pic.crop((cx, cy, cx + cw, cy + ch)).resize((vw, vh), Image.BILINEAR)
    img.paste(view, (vx, vy))
    d = ImageDraw.Draw(img, "RGBA")
    width = 35 * u
    for k in range(width):
        a = 0x88 * (width - k) // width
        d.rectangle((vx + k, vy + k, vx + vw - 1 - k, vy + vh - 1 - k), outline=(0, 0, 0, a))
    scale = max(2, min(6, (12 * u + 8) // 16))
    font = ImageFont.truetype(FONT, 16 * scale)
    here = sel
    # roads: straight lines between walkable places, under the studs and names
    road = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    rd = ImageDraw.Draw(road)
    for a, b in roads(src):
        pa = (vx + (pl[a][1] * iw // 1000 - cx) * vw // cw, vy + (pl[a][2] * ih // 1000 - cy) * vh // ch)
        pb = (vx + (pl[b][1] * iw // 1000 - cx) * vw // cw, vy + (pl[b][2] * ih // 1000 - cy) * vh // ch)
        rd.line([pa, pb], fill=(0, 0, 0, 110), width=max(1, int(2.1 * u)))
        rd.line([pa, pb], fill=(0xE8, 0xE8, 0xF0, 210), width=max(1, int(1.1 * u)))
    clip = Image.new("L", (W, H), 0)
    ImageDraw.Draw(clip).rectangle((vx, vy, vx + vw - 1, vy + vh - 1), fill=255)
    road.putalpha(ImageChops.multiply(road.getchannel("A"), clip))
    img.paste(road, (0, 0), road)
    d = ImageDraw.Draw(img, "RGBA")
    for i, (n, x, y) in enumerate(pl):
        sx = vx + (x * iw // 1000 - cx) * vw // cw
        sy = vy + (y * ih // 1000 - cy) * vh // ch
        # the game clips studs to the picture window; here one outside it is simply skipped
        if not (vx <= sx < vx + vw and vy <= sy < vy + vh):
            continue
        r = 3.6 * u
        if i == sel:
            d.ellipse((sx - 8 * u, sy - 8 * u, sx + 8 * u, sy + 8 * u), fill=(144, 255, 144, 110))
            d.ellipse((sx - r, sy - r, sx + r, sy + r), fill=(0x40, 0xE0, 0x50), outline=(255, 255, 255), width=u)
            tip = sy - 5 * u
            hr = 4.5 * u
            hy = tip - 9 * u
            d.polygon([(sx - hr, hy), (sx + hr, hy), (sx, tip)], fill=(0xF0, 0x58, 0x18), outline=(0x60, 0x20, 0x10))
            d.ellipse((sx - hr, hy - hr, sx + hr, hy + hr), fill=(0xF0, 0x58, 0x18), outline=(0x60, 0x20, 0x10), width=max(1, u))
        else:
            col = (0x40, 0xD8, 0x50) if i == here else (0x60, 0x60, 0x68)
            d.ellipse((sx - r, sy - r, sx + r, sy + r), fill=col, outline=(0xD0, 0xD0, 0xD8), width=u)
        tw = int(d.textlength(n, font=font))
        lw, lh = tw + 8 * u, 17 * u
        lx, ly = sx - lw // 2, sy + 10 * u
        if lw > vw or lh > vh:
            continue
        lx = min(max(lx, vx), vx + vw - lw)
        ly = min(max(ly, vy), vy + vh - lh)
        mx, my, mw, mh = p["mini"]
        if lx < mx + mw and mx < lx + lw and ly < my + mh and my < ly + lh:
            continue
        d.rectangle((lx, ly, lx + lw, ly + lh), fill=(0, 0, 0, 150))
        d.text((lx + 4 * u + 1, ly + (lh - 16 * scale) // 2 + 1), n, font=font, fill=(0x10, 0x12, 0x18))
        d.text((lx + 4 * u, ly + (lh - 16 * scale) // 2), n, font=font, fill=(255, 255, 244))
    ts = max(2, min(8, (bh * 7 // 10) // 16))
    tf = ImageFont.truetype(FONT, 16 * ts)
    title = "에레보스"
    tw = int(d.textlength(title, font=tf))
    pw = tw + 12 * u + bh
    for k in range(pw):
        a = (110 - 110 * k // pw) // 2
        d.line((bx + k, by, bx + k, by + bh - 1), fill=(255, 255, 255, a))
    d.text((bx + 6 * u + 2, by + (bh - 16 * ts) // 2 + 2), title, font=tf, fill=(0x10, 0x12, 0x18))
    d.text((bx + 6 * u, by + (bh - 16 * ts) // 2), title, font=tf, fill=(0xF4, 0xF0, 0xE0))
    kx, ky, kw, kh = p["back"]
    d.ellipse((kx, ky, kx + kw, ky + kh), fill=(0x20, 0x20, 0x28, 190), outline=(0xD0, 0xD0, 0xD8), width=max(1, kw // 18))
    d.arc((kx + kw * 0.25, ky + kh * 0.25, kx + kw * 0.75, ky + kh * 0.75), 10, 275, fill=(255, 255, 255), width=max(1, kw // 14))
    mx, my, mw, mh = p["mini"]
    d.rectangle((mx, my, mx + mw, my + mh), fill=(0, 0, 0, 170))
    ms = max(1, min(6, (mh * 7 // 10) // 16))
    mf = ImageFont.truetype(FONT, 16 * ms)
    d.text((mx + 3 * u, my + (mh - 16 * ms) // 2), "*미니맵 ON", font=mf, fill=(0xD0, 0xFF, 0xD8))
    img.save(out)
    print(out, "unit", u, "scale", scale, "cam", (cx, cy, cw, ch), "sel", pl[sel][0])


if __name__ == "__main__":
    main()
