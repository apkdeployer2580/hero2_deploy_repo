#!/usr/bin/env python3
"""★★ 2026-10-16 — 효과 라이브러리의 일부 효과만 새 그림으로 바꾸는 공용 길 (`fire_fx.replace` 를 일반화).

`replace(src, out, gens)`: 지금 bin 의 다른 효과는 1× 스프라이트 · HD 쌍의 색인 · 팔레트를 **바이트 그대로**(`fire_fx.read_raw` →
`Library.sprite_q`) 옮기고, `gens` 에 이름이 있는 효과만 그림 함수로 다시 그린다 — 효과 번호 · 길이는 `demon_fx.EFFECTS` 계약 그대로.
`blade_fx.py`(지옥칼날) · `charge_fx.py`(돌진) 가 쓴다; `preview_scenes` 는 실제 게임 배경 + 악마 HD 몸 위에 장면들을 3× 로 그려
시트 · GIF 를 만든다(`fire_fx.preview` 의 꼴).
"""

import math
import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def pieces_of(fn, age):
    """효과 그림 함수 → [(그림판, dx, dy)] (그림판 하나를 돌려주면 (c, 0, 0) 하나)."""
    r = fn(age)
    return r if isinstance(r, list) else [(r, 0, 0)]


def parts_of(lib, fn, age):
    """★★ 2026-10-17 조각은 (그림판, dx, dy) 또는 (그림판, dx, dy, 알파[, 플래그]) — 일섬 줄기의 프레임 알파 · 낙뢰 변형의 좌우 반전
    (`bolt_fx.all_gens`)."""
    out = []
    for piece in pieces_of(fn, age):
        c, dx, dy = piece[:3]
        alpha = piece[3] if len(piece) > 3 else 255
        flags = piece[4] if len(piece) > 4 else 0
        out += lib.parts_of(c, dx, dy, flags=flags, alpha=alpha)
    return out


def replace(src, out, gens):
    """`gens` = {효과 이름: fn(age) → 그림판 | [(그림판, dx, dy)]}. 나머지 효과는 그대로."""
    import demon_fx
    import fire_fx
    import fxpaint as fp
    fp.set_ss(demon_fx.HD)
    sprites, effects, qtwins = fire_fx.read_raw(src)
    names = [n for n, _ in effects]
    want = [n for n, _ in demon_fx.EFFECTS]
    missing = [n for n in want if n not in names]
    assert not missing, f"bin 의 효과 목록이 EFFECTS 와 다르다: {missing}"
    unknown = [n for n in gens if n not in want]
    assert not unknown, f"EFFECTS 에 없는 효과: {unknown}"
    lens = dict(demon_fx.EFFECTS)
    lib = demon_fx.Library(hd=True)
    remap = {}

    def keep(parts):
        out_parts = []
        for (s, dx, dy, a, fl) in parts:
            if s not in remap:
                arr, ax, ay = sprites[s]
                remap[s] = lib.sprite_q((np.ascontiguousarray(arr), ax, ay), qtwins.get(s))
            out_parts.append((remap[s], dx, dy, a, fl))
        return out_parts
    by = dict(effects)
    for name in want:
        if name in gens:
            lib.add_effect(name, [parts_of(lib, gens[name], a) for a in range(lens[name])])
        else:
            lib.add_effect(name, [keep(p) for p in by[name]])
    size, shape, n, hd_shape, twins = lib.write(out)
    print(f"{out}: {size:,} B · atlas {shape[1]}×{shape[0]} · {n} sprites · {len(lib.effects)} effects · HD twins {twins}"
          + (f" (atlas {hd_shape[1]}×{hd_shape[0]})" if hd_shape else "") + f" · regenerated: {sorted(gens)}")


# ── 미리보기 ──────────────────────────────────────────────────────────────────────────────────────────
def preview_scenes(out, stem, seq, src=None, k=3, demon_pose=("stand", 1), mobs=None, duration=50, cols=4, pick=None):
    """`seq` = [(제목, [(층 'g'|'t', 효과 이름, 나이, x, y)])] — 땅 층은 개체 전에, 위 층은 개체 뒤에. 실제 게임 배경(176 × 150) ·
    악마 HD 몸(`demon_pose` = (자세, 방향)) · 몬스터 `mobs` = [(x, y)] (발 기준) 위에 3× 로 합성 → `<out>/<stem>.gif` + `<stem>_sheet.png`."""
    import demon_fx as d
    import fire_fx as ff
    path = src or d.OUT_BIN
    sprites, effects, twins = d.read_bin_hd(path)
    E = {n: i for i, (n, _) in enumerate(effects)}
    os.makedirs(out, exist_ok=True)
    bg = d.real_background()[:150, :176]
    W, H = 176, 150
    FX, FY = 88, 104
    big_bg = np.asarray(Image.fromarray(bg).resize((W * k, H * k), Image.NEAREST)).astype(np.float32)
    mob_img, (mfx, mfy) = d.real_monster()
    mob_big = np.asarray(mob_img.resize((mob_img.width * k, mob_img.height * k), Image.NEAREST))
    mobs = [(FX + 58, FY - 26), (FX - 62, FY + 8), (FX + 30, FY + 34)] if mobs is None else mobs
    demon_im, ds = d.demon_any(demon_pose[0], demon_pose[1])
    dem = np.asarray(demon_im.convert("RGBA"))
    if ds != k:
        dem = np.asarray(demon_im.resize((demon_im.width * k // ds, demon_im.height * k // ds), Image.NEAREST))
    dem_feet = (-d.DEMON_OFF[0] * k, -d.DEMON_OFF[1] * k)

    def entities(c):
        ents = [(FY, "demon")] + [(my, ("mob", mx, my)) for (mx, my) in mobs]
        for _, e in sorted(ents, key=lambda q: q[0]):
            if e == "demon":
                ff._blend_twin(c, (dem, dem_feet[0], dem_feet[1], k), FX * k, FY * k)
            else:
                ff._blend_twin(c, (mob_big, mfx * k, mfy * k, k), e[1] * k, e[2] * k)

    frames = []
    for title, lay in seq:
        c = big_bg.copy()
        for (l, name, age, x, y) in lay:
            if l == "g":
                ff.draw_hd(c, sprites, effects, twins, E[name], age, x, y, k)
        entities(c)
        for (l, name, age, x, y) in lay:
            if l == "t":
                ff.draw_hd(c, sprites, effects, twins, E[name], age, x, y, k)
        frames.append((title, np.clip(c, 0, 255).astype(np.uint8)))
    ims = [Image.fromarray(f) for _, f in frames]
    ims[0].save(os.path.join(out, f"{stem}.gif"), save_all=True, append_images=ims[1:], duration=duration, loop=0, disposal=1)
    pick = list(range(len(frames))) if pick is None else [p for p in pick if p < len(frames)]
    rows = (len(pick) + cols - 1) // cols
    from PIL import ImageDraw, ImageFont
    try:
        font = ImageFont.truetype(os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf"), 16)
    except Exception:
        font = None
    cw, ch = W * k, H * k
    sheet = Image.new("RGB", (cols * (cw + 8) + 8, rows * (ch + 28) + 8), (20, 20, 26))
    dr = ImageDraw.Draw(sheet)
    for j, p in enumerate(pick):
        x = 8 + (j % cols) * (cw + 8)
        y = 8 + (j // cols) * (ch + 28)
        dr.text((x, y), frames[p][0], fill=(230, 230, 230), font=font)
        sheet.paste(ims[p], (x, y + 22))
    sheet.save(os.path.join(out, f"{stem}_sheet.png"))
    print(f"preview → {out}/{stem}.gif · {stem}_sheet.png ({len(frames)} frames)")
    return (FX, FY), mobs


def mob_feet_default():
    return [(88 + 58, 104 - 26), (88 - 62, 104 + 8), (88 + 30, 104 + 34)]


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def ease(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def lerp3(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def ell(t, rx, ry):
    return math.cos(t) * rx, math.sin(t) * ry
