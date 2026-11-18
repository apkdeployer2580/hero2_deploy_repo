#!/usr/bin/env python3
"""Generate the launcher icon set from 영웅서기2's OWN title art.

WHY THIS EXISTS
---------------
`AndroidManifest.xml` has claimed since the fork that "아이콘 리소스는
tools/gen_app_icon.py 가 원작 타이틀 아트에서 생성한다" — but the script was hero3's and
never came across, so what actually shipped was hero3's PNGs copied wholesale. On a phone
with both installed, 영웅서기2 and 영웅서기3 had the SAME icon (reported 2026-09-01).
This is that script, for hero2, and it reads the art out of the game archive so the
result cannot drift from the game again.

SOURCE
------
`img/sui` frame 3 (169x87) in the bundled archive — the title logo,
"영웅서기2 / 빙해의 검사 / Heroes Lore: Knight of Frozen Sea".
NOT `img/logo`, which is the publisher mark. See `tools_re/dump_mpd.py` for the
container format and for why the background is a COLOUR KEY (index 17 = rgb(1,0,0), a
sentinel near-black) rather than a `tRNS`.

OUTPUT (overwrites; run it, then rebuild)
-----------------------------------------
  mipmap-<density>/ic_launcher_background.png   full-bleed warm gradient
  mipmap-<density>/ic_launcher_foreground.png   keyed logo, centred, inside the safe zone
  mipmap-<density>/ic_launcher.png              composite, rounded-rect mask
  mipmap-<density>/ic_launcher_round.png        composite, circular mask

    python tools/gen_app_icon.py
"""

import io
import os
import sys

from PIL import Image, ImageDraw

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools_re'))
from dump_mpd import frames, key_corner, sources  # noqa: E402

ARCHIVE = 'android_app/app/src/main/assets/hero2_ktf.zip'
RES = 'android_app/app/src/main/res'
SPRITE, FRAME = 'img/sui', 3

# Adaptive icons are 108dp; only the central 66dp survives every launcher mask. A wide
# wordmark's binding constraint is its half-width at the vertical centre, where the
# circle is at full radius (0.33 * canvas), so 64% width sits inside it with margin.
LOGO_WIDTH_FRAC = 0.64
# The LEGACY icons are pre-masked squares, so no launcher mask can crop them further and
# the logo may run wider. At 48px it needs every pixel it can get.
LEGACY_WIDTH_FRAC = 0.84
# density -> (adaptive layer px, legacy icon px)
DENSITIES = {
    'mdpi': (108, 48),
    'hdpi': (162, 72),
    'xhdpi': (216, 96),
    'xxhdpi': (324, 144),
    'xxxhdpi': (432, 192),
}
EDGE = (1, 0, 0)  # the art's own sentinel near-black; hero3's icon used it too


def load_logo():
    for src in sources(ARCHIVE):
        for name in src.namelist():
            if name == SPRITE + '.mph':
                mph = src.read(name)
                mpd = src.read(SPRITE + '_0.mpd')
                return key_corner(frames(mph, mpd)[FRAME]['png'])
    raise SystemExit('%s not found in %s' % (SPRITE, ARCHIVE))


def warm_tone(logo):
    """A ground colour taken from the art itself, so background and logo agree.

    Mean of the saturated warm pixels (the gold ornament), then darkened well below the
    art so the logo still reads as the bright thing on the tile.
    """
    px = [p for p in logo.getdata() if p[3] > 0 and p[0] > p[2] + 40]
    if not px:
        return (78, 35, 8)
    n = len(px)
    r, g, b = (sum(p[i] for p in px) // n for i in range(3))
    return (int(r * 0.42), int(g * 0.42), int(b * 0.42))


def background(size, tone):
    """Radial warm pool, exactly like the sibling apps' icons: warm centre, dark edges."""
    img = Image.new('RGB', (size, size), EDGE)
    px = img.load()
    c = (size - 1) / 2.0
    for y in range(size):
        for x in range(size):
            d = ((x - c) ** 2 + (y - c) ** 2) ** 0.5 / (c * 1.42)
            t = max(0.0, 1.0 - d) ** 1.6
            px[x, y] = tuple(int(EDGE[i] + (tone[i] - EDGE[i]) * t) for i in range(3))
    return img


def foreground(size, logo, frac=LOGO_WIDTH_FRAC):
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    w = int(size * frac)
    h = max(1, round(w * logo.height / logo.width))
    # NEAREST: this is 169px pixel art blown up ~2.6x. Smoothing turns crisp feature-phone
    # lettering to mush, which is exactly what the device screenshot already looks like.
    img.paste(logo.resize((w, h), Image.NEAREST), ((size - w) // 2, (size - h) // 2))
    return img


def masked(comp, size, circle):
    m = Image.new('L', (size, size), 0)
    d = ImageDraw.Draw(m)
    if circle:
        d.ellipse((0, 0, size - 1, size - 1), fill=255)
    else:
        d.rounded_rectangle((0, 0, size - 1, size - 1), radius=int(size * 0.22), fill=255)
    out = comp.resize((size, size), Image.LANCZOS).convert('RGBA')
    out.putalpha(m)
    return out


def main():
    logo = load_logo()
    tone = warm_tone(logo)
    print('logo %dx%d, ground rgb%s' % (logo.width, logo.height, tone))
    for dens, (layer, legacy) in DENSITIES.items():
        d = os.path.join(RES, 'mipmap-' + dens)
        os.makedirs(d, exist_ok=True)
        bg = background(layer, tone)
        fg = foreground(layer, logo)
        bg.save(os.path.join(d, 'ic_launcher_background.png'))
        fg.save(os.path.join(d, 'ic_launcher_foreground.png'))
        wide = Image.alpha_composite(
            bg.convert('RGBA'), foreground(layer, logo, LEGACY_WIDTH_FRAC))
        masked(wide, legacy, False).save(os.path.join(d, 'ic_launcher.png'))
        masked(wide, legacy, True).save(os.path.join(d, 'ic_launcher_round.png'))
        print('  %-8s layer %d, legacy %d' % (dens, layer, legacy))


if __name__ == '__main__':
    main()
