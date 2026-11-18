#!/usr/bin/env python3
"""★★ 2026-10-18 야수조련 목걸이의 용암불 — 고리 반경 ×2 의 용암 지대를 `wie_android/src/demon_fx.bin` 뒤에 붙인다.

사용자: *"현재 야수목걸이로 범위가 늘어난 스킬들이 실제 영향은 범위가 늘어난대로 되고있는데 이펙트는 안늘어나고 그대로야. 이거 이펙트도
늘어나게 변경해줘"* — 악의 중 용암불(= 야수의 지옥불)은 목걸이가 링을 반경 2 배(32 칸)로 넓히는데, 5 초 용암 지대는 고리 12 칸 그대로였다.

* `lava_w{k}` · `lava_wtop{k}` 는 `lava{k}` · `lava_top{k}` 와 같은 그림, 같은 칸 왼쪽 위에 그린다. 거리를 늘려 옮기면
  불이 피해 칸을 떠나서, 이펙트 위에 선 몬스터가 안 맞았다. 피해는 그 칸과 맞닿은 칸(그림이 16px 타일 밖으로 번진 곳)이다.

다른 효과는 1× 스프라이트 · HD 쌍의 색인 · 팔레트가 바이트 그대로 옮겨진다(`demon_weapon_fx.build` 의 꼴).
효과 번호 · 이름 · 길이는 `demon_fx.EFFECTS` 의 계약(`DemonPatch::FX_LAVA_WIDE` · `FX_LAVA_WIDE_TOP` · `FX_COUNT`).

    python tools_re/lava_wide_fx.py              # bin 에 붙이기 (이미 붙어 있으면 다시 만든다)
    python tools_re/lava_wide_fx.py preview      # docs/skill/demon_lava/wide.png — 고리 12 칸 · 2 배 지대 (나이 여럿)
"""

import os
import sys

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RING = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1), (0, -2), (0, 2), (-2, 0), (2, 0)]
LAVA_LEN = 100  # = demon_fx.LAVA_LEN (checked at build)


def effect_names():
    return [(f"lava_w{k}", LAVA_LEN) for k in range(len(RING))] + [(f"lava_wtop{k}", LAVA_LEN) for k in range(len(RING))]


def widen(frames, _k):
    """The same parts, drawn on the same tile. Moving them off the tile made the fire miss its damage."""
    return [[(s, px, py, a, fl & ~64) for (s, px, py, a, fl) in parts] for parts in frames]


def add_effects(lib):
    """The twenty-four from the ring's pieces already in `lib` (lava0..11 · lava_top0..11)."""
    by = {name: frames for name, frames in lib.effects}
    for k in range(len(RING)):
        lib.add_effect(f"lava_w{k}", widen(by[f"lava{k}"], k))
    for k in range(len(RING)):
        lib.add_effect(f"lava_wtop{k}", widen(by[f"lava_top{k}"], k))


def build(src=None, out=None):
    """The bin's other effects byte for byte (`fire_fx.read_raw` → `Library.sprite_q`), ours made at their place."""
    import demon_fx
    import fire_fx
    import lava_fx
    assert demon_fx.LAVA_LEN == LAVA_LEN and lava_fx.RING == RING
    src = src or demon_fx.OUT_BIN
    out = out or demon_fx.OUT_BIN
    sprites, effects, qtwins = fire_fx.read_raw(src)
    by = dict(effects)
    ours = {n for n, _ in effect_names()}
    missing = [n for n, _ in demon_fx.EFFECTS if n not in ours and n not in by]
    assert not missing, f"bin 에 없는 효과: {missing}"
    lib = demon_fx.Library(hd=True)
    remap = {}

    def keep(s):
        if s not in remap:
            arr, ax, ay = sprites[s]
            remap[s] = lib.sprite_q((np.ascontiguousarray(arr), ax, ay), qtwins.get(s))
        return remap[s]

    for name, _ in demon_fx.EFFECTS:
        if name in ours:
            if name == "lava_w0":
                add_effects(lib)
            continue
        lib.add_effect(name, [[(keep(s), dx, dy, a, fl) for (s, dx, dy, a, fl) in parts] for parts in by[name]])
    assert len(lib.effects) == len(demon_fx.EFFECTS)
    size, shape, n, hd_shape, twins = lib.write(out)
    print(f"{out}: {size:,} B · atlas {shape[1]}×{shape[0]} · {n} sprites · {len(lib.effects)} effects · HD twins {twins}"
          + (f" (atlas {hd_shape[1]}×{hd_shape[0]})" if hd_shape else "") + f" · lava on its tile: {len(ours)} effects, no new sprite")


def preview(out=None, src=None):
    """The ring's lava as shipped beside the necklace's (2×) on a game background, a few ages, 3×."""
    import demon_fx as d
    out = out or os.path.join(ROOT, "docs", "skill", "demon_lava", "wide.png")
    sprites, effects = d.read_bin(src or d.OUT_BIN)
    E = {n: i for i, (n, _) in enumerate(effects)}
    W, H = 176, 220
    FX, FY = 88, 118
    ctx, cty = (FX - 8) // 16, (FY - 10) // 16
    ox, oy = FX - 8 - 16 * ctx, FY - 10 - 16 * cty
    extra = [(-2, -1), (-1, -2), (-2, -2), (2, -1), (1, -2), (2, -2), (-2, 1), (-1, 2), (-2, 2), (2, 1), (1, 2), (2, 2),
             (0, -3), (0, -4), (0, 3), (0, 4), (-3, 0), (-4, 0), (3, 0), (4, 0)]
    shots = []
    for age in (4, 30, 70):
        for wide in (False, True):
            c = np.zeros((H, W, 3), np.float32)
            bg = d._background()
            c[:bg.shape[0], :bg.shape[1]] = bg[:H, :W]
            g, t = ("lava_w0", "lava_wtop0") if wide else ("lava0", "lava_top0")
            for k, (dx, dy) in enumerate(RING):
                d.draw_effect(c, sprites, effects, E[g] + k, age, (ctx + dx) * 16 + ox, (cty + dy) * 16 + oy)
            d._paste(c, d.demon("stand", d.RIGHT), FX + d.DEMON_OFF[0], FY + d.DEMON_OFF[1])
            for k, (dx, dy) in enumerate(RING):
                d.draw_effect(c, sprites, effects, E[t] + k, age, (ctx + dx) * 16 + ox, (cty + dy) * 16 + oy)
            im = Image.fromarray(np.clip(c, 0, 255).astype(np.uint8))
            dr = ImageDraw.Draw(im)
            for (dx, dy) in RING + (extra if wide else []):
                x0, y0 = (ctx + dx) * 16 + ox, (cty + dy) * 16 + oy
                dr.rectangle((x0, y0, x0 + 15, y0 + 15), outline=(90, 90, 90))
            shots.append((f"{'목걸이 (32 칸 · 2 배)' if wide else '고리 12 칸'} · 나이 {age}", im))
    z = 3
    sheet = Image.new("RGB", (2 * (W * z + 8) + 8, 3 * (H * z + 26) + 8), (20, 20, 26))
    dr = ImageDraw.Draw(sheet)
    try:
        from PIL import ImageFont
        font = ImageFont.truetype(os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf"), 16)
    except Exception:
        font = None
    for i, (title, im) in enumerate(shots):
        x, y = 8 + (i % 2) * (W * z + 8), 8 + (i // 2) * (H * z + 26)
        sheet.paste(im.resize((W * z, H * z), Image.NEAREST), (x, y + 20))
        dr.text((x, y), title, fill=(230, 230, 230), font=font)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    sheet.save(out)
    print(f"preview → {out}")


if __name__ == "__main__":
    sys.stdout.reconfigure(errors="replace")
    if len(sys.argv) > 1 and sys.argv[1] == "preview":
        preview(sys.argv[2] if len(sys.argv) > 2 else None)
    else:
        build()
