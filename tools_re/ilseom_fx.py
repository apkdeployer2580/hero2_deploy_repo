#!/usr/bin/env python3
r"""일섬 「번개의 호흡」 세트 연출 (2026-10-14) — 호스트 이펙트 라이브러리(`demon_fx.bin`)의 뒤에 붙는 노란 번개.

사용자: *"단검중에 번개의 호흡이라는 단검을 추가하고싶어 … 벽력일섬과 세트고 … 일섬 사용시 번개이펙트(악의의 암흑 번개 노란 번개색
버전)가 멋있게 나와야해. 번개처럼 내지르면서 적을 베는 듯한 연출"* · *"번개 이펙트 조건이 번개의 호흡 2개착용이 아니라 1개 착용"*.

그림 넷 (모두 기준점 = 개체의 발바닥, `DemonPatch::FEET` 규약 — 게스트 그림 조각이 `drawLine` 한 번으로 고른다):
* **줄기** `ilstreak_<dir>_<k><c>` — 일섬이 지나간 길을 따라 내지른 번개(흰 심 · 노란 몸 · 금빛 바깥빛 · 앞으로 갈라지는 가지 ·
  튀는 불꽃). 기준점 = 착지한 플레이어의 발, 길 = 출발 칸 → 착지 칸(가슴 높이), k = 지나간 줄 수 1..5, c = 착지 열 −1..1.
  그림은 두 가족뿐이다 — 가로 H(k, L)(왼쪽 끝 기준, 오른쪽으로 k 칸 · 옆으로 L 칸)와 세로 V(k, L)(위 끝 기준) — 방향 넷은 그 그림을
  부품 (dx, dy) 로 옮겨 쓴다(`streak_part`). 프레임 = 번쩍임 둘(A · B 가 번갈아) → 남은 불티(R)가 사라짐.
* **섬광** `ilflash` — 착지한 자리의 전격(빛살 · 고리 · 몸을 감는 잔전기).
* **베기** `ilslash0/1` — 맞은 적 몸을 가르는 번개 사선(/ · \) + 불꽃.
* **낙뢰** `ilbolt0..3` — 어둠심판의 낙뢰(`bolt0..3`)를 그대로 노랗게 물들인 것(같은 모양 · 같은 4 그리기). 「악의의 암흑 번개 노란 번개색 버전」.

⛔ 효과 번호는 계약이다 — `demon_fx.EFFECTS` 의 순서 = `AccessoryPatch::FX_IL*` (러스트 테스트가 bin 의 이름 · 길이를 번호마다 본다).

    python tools_re/ilseom_fx.py extend          # 지금 bin 의 79 효과(스프라이트 바이트 그대로) + 이 효과들 → demon_fx.bin
    python tools_re/ilseom_fx.py preview [--out docs/skill/ilseom_bolt]
"""

import argparse
import colorsys
import math
import os
import random
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fxpaint as fp  # noqa: E402
from fxpaint import Canvas  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

TILE = 16
DASH = 5  # AccessoryPatch::DASH_TILES
CHEST = -11  # 줄기가 지나는 높이 (발바닥 기준)
STREAK_LEN = 8
FLASH_LEN = 6
SLASH_LEN = 6
BOLT_LEN = 4  # = demon_fx.BOLT_LEN (어둠심판 낙뢰와 같은 길이)
DIRS = ["up", "down", "left", "right"]  # facing − 1

# 노란 번개 팔레트 — 어둠심판(보라 · 자홍 · 흰 심)의 자리마다 같은 밝기의 노랑
GOLD_DARK = (120, 72, 0)
GOLD = (255, 196, 24)
YELLOW = (255, 236, 90)
CORE = (255, 255, 236)
GLOW = (255, 200, 40)
SPARK = (255, 250, 180)


def effect_names():
    """demon_fx.EFFECTS 뒤에 붙는 이름과 프레임 수 — 순서가 계약."""
    out = []
    for d in DIRS:
        for k in range(1, DASH + 1):
            for c in (-1, 0, 1):
                out.append((f"ilstreak_{d}_{k}{'m0p'[c + 1]}", STREAK_LEN))
    out.append(("ilflash", FLASH_LEN))
    out += [(f"ilslash{v}", SLASH_LEN) for v in range(2)]
    out += [(f"ilbolt{v}", BOLT_LEN) for v in range(4)]
    return out


class Fixed:
    """한 번 계산한 sprites() — 같은 그림을 다른 알파 · 자리로 여러 번 쓴다. ★★ 2026-10-17 HD: 3× 그림판이면 그 쌍(`sprites_hd`)도
    (`Library.parts_of` 가 `ss` 와 `sprites_hd` 를 묻는다 — 없던 때는 일섬 그림이 전부 1× 만 실렸다)."""

    def __init__(self, canvas):
        self._s = canvas.sprites()
        self.ss = canvas.ss
        self._hd = canvas.sprites_hd() if canvas.ss > 1 else [None, None]

    def sprites(self):
        return self._s

    def sprites_hd(self):
        return self._hd


# ── 줄기 ────────────────────────────────────────────────────────────────────────────────────────────────────
def _path(x1, y1, rnd, long_axis):
    """(0, CHEST) → (x1, y1): 중점 변위 꺾은선 — 길이에 맞춰 마디 수."""
    L = math.hypot(x1, y1 - CHEST)
    segs = 3 if L < 24 else 4 if L < 56 else 5
    pts = Canvas.bolt_pts(None, 0.0, CHEST, float(x1), float(y1), segs, 0.34, rnd)
    return pts


def _streak(axis, k, L, variant):
    """한 그림: axis 'h' = 왼쪽 끝 기준 (0,0) → (16k, 16L), 'v' = 위 끝 기준 (0,0) → (16L, 16k). variant 0/1 = 번쩍임, 2 = 잔불."""
    if axis == "h":
        x1, y1 = TILE * k, TILE * L + CHEST
    else:
        x1, y1 = TILE * L, TILE * k + CHEST
    lo_x, hi_x = min(0, x1) - 22, max(0, x1) + 22
    lo_y, hi_y = min(CHEST, y1) - 22, max(CHEST, y1) + 22
    c = Canvas(int(hi_x - lo_x), int(hi_y - lo_y), int(-lo_x), int(-lo_y))
    rnd = random.Random(7100 + 97 * k + 13 * (L + 1) + 1000 * variant + (0 if axis == "h" else 50000))
    pts = _path(x1, y1, rnd, axis)
    fwd = (x1 - 0.0, y1 - CHEST)
    n = math.hypot(*fwd) or 1.0
    ux, uy = fwd[0] / n, fwd[1] / n
    if variant < 2:
        w = 2.5 if variant == 0 else 2.0
        # 바깥빛 · 어두운 테 · 노란 몸 · 흰 심 — 어둠심판 낙뢰의 층 그대로
        c.stroke(pts, w * 6.0, GLOW, 0.34, "add")
        c.stroke(pts, w * 2.7, GOLD_DARK, 0.6)
        c.lightning(pts, w, GOLD, YELLOW, core=CORE, alpha=1.0, glow=None)
        # 곁 번개: 같은 길을 엇갈려 감는 가는 줄기 하나
        twin = Canvas.bolt_pts(None, 0.0, CHEST + rnd.uniform(-2, 2), float(x1), float(y1), 4 if n > 30 else 3, 0.22, rnd)
        c.lightning(twin, w * 0.45, GOLD, YELLOW, core=SPARK, alpha=0.8)
        # 내지른 끝: 착지 쪽으로 뾰족해지는 창끝 빛
        tip = [(x1 - ux * min(n, 26), y1 - uy * min(n, 26)), (x1 + ux * 4, y1 + uy * 4)]
        c.taper(tip, w * 2.2, 0.4, YELLOW, 0.75)
        c.taper(tip, w * 1.0, 0.3, CORE, 0.9)
        # 앞으로 갈라지는 가지 (길이에 비례)
        nb = 2 + k
        for b in range(nb):
            i = rnd.randrange(max(1, len(pts) // 6), max(2, len(pts) - 1))
            x0, y0 = pts[i]
            side = rnd.choice((-1, 1))
            reach = rnd.uniform(6, 13)
            bx = x0 + ux * reach * 0.8 - uy * side * reach
            by = y0 + uy * reach * 0.8 + ux * side * reach
            bp = c.bolt_pts(x0, y0, bx, by, 2, 0.4, rnd)
            c.lightning(bp, w * 0.45, GOLD, YELLOW, core=SPARK, alpha=0.9)
        # 출발 칸의 잔전기 · 착지 칸의 빛
        c.glow(0, CHEST, 6, 6, GLOW, 0.55 if variant == 0 else 0.35)
        c.glow(x1, y1, 9, 8, (255, 230, 120), 0.75 if variant == 0 else 0.5)
        for s in range(3 + k):
            t = rnd.random()
            j = min(len(pts) - 1, int(t * (len(pts) - 1)))
            px, py = pts[j]
            c.ellipse(px + rnd.uniform(-4, 4), py + rnd.uniform(-5, 5), 0.7, 0.7, SPARK, 0.9, "add")
    else:
        # 잔불: 끊긴 마디 + 금빛 잔광 + 떨어지는 불티
        keep = [i for i in range(len(pts) - 1) if rnd.random() < 0.55]
        for i in keep:
            seg = [pts[i], pts[i + 1]]
            c.stroke(seg, 3.0, GLOW, 0.25, "add")
            c.stroke(seg, 1.0, YELLOW, 0.85)
            c.stroke(seg, 0.5, CORE, 0.8)
        c.stroke(pts, 5.0, GLOW, 0.14, "add")
        for s in range(5 + 2 * k):
            j = rnd.randrange(len(pts))
            px, py = pts[j]
            c.ellipse(px + rnd.uniform(-5, 5), py + rnd.uniform(-2, 6), 0.65, 0.65, SPARK, 0.85, "add")
    return Fixed(c)


# 프레임: (그림 변형, 알파) — 번쩍임 A · B 가 번갈아 넷 → 잔불이 사라진다
STREAK_FRAMES = [(0, 255), (1, 255), (0, 235), (1, 200), (2, 230), (2, 170), (2, 105), (2, 45)]


def streak_part(d, k, c):
    """facing d(0 위 · 1 아래 · 2 왼 · 3 오른)의 일섬 (k 줄, 착지 열 c) → (축, L, dx, dy): 그림 H/V(k, L) 을 착지 발 기준 (dx, dy) 에.

    열 규칙(`h2_acc_ilseom_unit`): 착지 칸 = 출발 + row·(dx, dy) + col·(dy², dx²) — 위 · 아래는 col +1 = 오른쪽, 왼 · 오른은 col +1 = 아래.
    """
    if d == 0:  # 위: 착지가 위 끝, 출발 = (−c, +k)
        return "v", -c, 0, 0
    if d == 1:  # 아래: 출발이 위 끝 (−c, −k), 착지 = 그로부터 (+c, +k)
        return "v", c, -TILE * c, -TILE * k
    if d == 2:  # 왼: 착지가 왼 끝, 출발 = (+k, −c)
        return "h", -c, 0, 0
    return "h", c, -TILE * k, -TILE * c  # 오른: 출발이 왼 끝 (−k, −c), 착지 = (+k, +c)


# ── 섬광 · 베기 ─────────────────────────────────────────────────────────────────────────────────────────────
def flash(age):
    """착지 자리의 전격 (6): 빛살 별 → 고리가 퍼짐 · 몸을 감는 잔전기 · 불티."""
    c = Canvas(80, 72, 40, 56)
    rnd = random.Random(7300 + age)
    t = age / (FLASH_LEN - 1)
    a = 1.0 - 0.75 * t
    cy = -12
    if age < 3:
        c.glow(0, cy, 12 - age * 2, 14 - age * 2, (255, 236, 140), 0.85 - 0.2 * age)
        c.star(0, cy, 8, 1.6, 22 - age * 5, YELLOW, 0.95 - 0.25 * age, phase=0.2 + age * 0.3, squash=0.8, rnd=rnd)
        c.star(0, cy, 5, 1.2, 11 - age * 2, CORE, 1.0 - 0.3 * age, phase=0.0, squash=0.85, core=CORE)
    r = 9 + age * 4
    c.ellipse(0, -1, r, r * 0.36, GOLD_DARK, 0.5 * a, width=2.2)
    c.ellipse(0, -1, r, r * 0.36, GOLD, a, width=1.2)
    c.ellipse(0, -1, r, r * 0.36, CORE, 0.55 * a, width=0.5)
    for b in range(3 if age < 4 else 1):  # 몸을 감는 잔전기
        th = rnd.uniform(0, 2 * math.pi)
        x0, y0 = math.cos(th) * 7, cy + math.sin(th) * 10
        x1, y1 = math.cos(th + 1.3) * 9, cy + math.sin(th + 1.3) * 12
        pts = c.bolt_pts(x0, y0, x1, y1, 2, 0.5, rnd)
        c.lightning(pts, 0.6, GOLD, YELLOW, core=SPARK, alpha=0.9 * a)
    for s in range(10):
        th = rnd.uniform(math.pi * 1.05, math.pi * 1.95)
        sp = rnd.uniform(4, 9) * (age + 1) * 0.8
        c.ellipse(math.cos(th) * sp, cy + math.sin(th) * sp * 0.7 + 0.6 * age * age, 0.6, 0.6, SPARK, a, "add")
    return Fixed(c)


def slash(variant, age):
    """맞은 적을 가르는 번개 사선 (6): 0 = 가는 흰 금 → 1..2 = 금빛 칼선 + 잔전기 → 3..5 = 가늘어지며 사라짐."""
    c = Canvas(72, 72, 36, 56)
    rnd = random.Random(7500 + 31 * variant + age)
    s = 1 if variant == 0 else -1
    x0, y0, x1, y1 = -14 * s, -26, 14 * s, -2  # 몸을 가로지르는 사선 (발 기준)
    thin = [1.0, 1.0, 0.85, 0.6, 0.4, 0.22][age]
    a = [1.0, 1.0, 0.95, 0.8, 0.55, 0.3][age]
    grow = [0.55, 1.0, 1.0, 1.0, 1.0, 1.0][age]
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    ax0, ay0 = mx + (x0 - mx) * grow, my + (y0 - my) * grow
    ax1, ay1 = mx + (x1 - mx) * grow, my + (y1 - my) * grow
    pts = c.bolt_pts(ax0, ay0, ax1, ay1, 3, 0.12, rnd)
    c.stroke(pts, 7 * thin, GLOW, 0.3 * a, "add")
    c.taper(pts, 0.6, 3.2 * thin, GOLD, a)
    c.taper(pts, 0.4, 1.8 * thin, YELLOW, a)
    c.taper(pts, 0.3, 0.9 * thin, CORE, a)
    if age <= 3:
        for b in range(3):
            i = rnd.randrange(1, len(pts) - 1)
            px, py = pts[i]
            th = rnd.uniform(0, 2 * math.pi)
            bp = c.bolt_pts(px, py, px + math.cos(th) * 9, py + math.sin(th) * 9, 2, 0.5, rnd)
            c.lightning(bp, 0.5, GOLD, YELLOW, core=SPARK, alpha=0.85 * a)
    if age < 2:
        c.glow(mx, my, 8, 8, (255, 240, 160), 0.7)
        c.star(mx, my, 6, 1.0, 12, CORE, 0.85, phase=0.4, squash=0.9, rnd=rnd)
    for k in range(8):
        th = rnd.uniform(0, 2 * math.pi)
        sp = rnd.uniform(3, 7) * (age + 1)
        c.ellipse(mx + math.cos(th) * sp, my + math.sin(th) * sp * 0.8 + 0.5 * age * age, 0.6, 0.6, SPARK, a, "add")
    return Fixed(c)


# ── 낙뢰: 어둠심판 낙뢰를 노랗게 ────────────────────────────────────────────────────────────────────────────────
def yellow(arr):
    """RGBA 스프라이트의 보라 · 자홍 · 분홍을 같은 밝기의 노랑 · 금빛으로 (흰 심은 따뜻한 흰색, 무채색은 그대로)."""
    out = arr.copy()
    rgb = arr[..., :3].astype(np.float32) / 255.0
    mx = rgb.max(axis=2)
    mn = rgb.min(axis=2)
    sat = np.where(mx > 1e-6, (mx - mn) / np.maximum(mx, 1e-6), 0)
    # 목표: 채도 높은 픽셀 → 색상 48° (금빛), 밝을수록 노랑 · 흰색 쪽
    h = np.full_like(mx, 48.0 / 360.0)
    h = h - (1.0 - mx) * (20.0 / 360.0)  # 어두운 몸은 호박색 쪽
    s2 = np.clip(sat * 1.05, 0, 1)
    v2 = np.clip(mx * 1.04 + 0.03, 0, 1)
    flat = np.stack([h.ravel(), s2.ravel(), v2.ravel()], axis=1)
    conv = np.array([colorsys.hsv_to_rgb(*p) for p in flat], np.float32).reshape(rgb.shape)
    k = np.clip((sat - 0.08) / 0.2, 0, 1)[..., None]  # 무채색(흰 심 · 검은 조각)은 그대로
    warm = rgb * np.array([1.0, 0.98, 0.86], np.float32)  # 흰 심을 살짝 따뜻하게
    res = conv * k + warm * (1 - k)
    out[..., :3] = np.clip(res * 255.0 + 0.5, 0, 255).astype(np.uint8)
    return out


# ── 묶기 ───────────────────────────────────────────────────────────────────────────────────────────────────
def add_effects(lib):
    """`demon_fx.Library` 뒤에 효과를 붙인다 — 낙뢰는 같은 라이브러리의 `bolt0..3` 부품을 노랗게 물들여 쓴다."""
    import demon_fx
    drawings = {}
    for axis in ("h", "v"):
        for k in range(1, DASH + 1):
            for L in (-1, 0, 1):
                for v in range(3):
                    drawings[(axis, k, L, v)] = _streak(axis, k, L, v)
    for di, d in enumerate(DIRS):
        for k in range(1, DASH + 1):
            for c in (-1, 0, 1):
                axis, L, dx, dy = streak_part(di, k, c)
                frames = [lib.parts_of(drawings[(axis, k, L, v)], dx, dy, alpha=a) for (v, a) in STREAK_FRAMES]
                lib.add_effect(f"ilstreak_{d}_{k}{'m0p'[c + 1]}", frames)
    lib.add_effect("ilflash", [lib.parts_of(flash(a)) for a in range(FLASH_LEN)])
    for v in range(2):
        lib.add_effect(f"ilslash{v}", [lib.parts_of(slash(v, a)) for a in range(SLASH_LEN)])
    names = [n for n, _ in lib.effects]
    recoloured = {}
    for v in range(4):
        src = lib.effects[names.index(f"bolt{v}")][1]
        frames = []
        for parts in src:
            new = []
            for (s, dx, dy, a, fl) in parts:
                if s not in recoloured:
                    arr, ax, ay = lib.sprites[s]
                    tw = lib.twins.get(s)  # ★ HD: the twin recoloured alike
                    recoloured[s] = lib.sprite((yellow(arr), ax, ay), None if tw is None else (yellow(tw[0]), tw[1], tw[2], tw[3]))
                new.append((recoloured[s], dx, dy, a, fl))
            frames.append(new)
        lib.add_effect(f"ilbolt{v}", frames)
    assert [n for n, _ in lib.effects[-len(effect_names()):]] == [n for n, _ in effect_names()]
    _ = demon_fx


def extend(src, out):
    """지금 bin(79 효과)의 스프라이트 바이트를 그대로 두고 이 효과들을 붙여 쓴다 — 전체 재생성은 이 환경의 numpy/PIL 에서
    옛 스프라이트 16 장이 ±22 다르게 나와서(2026-10-14 실측) 옛 그림을 건드리지 않는 길."""
    import demon_fx
    sprites, effects = demon_fx.read_bin(src)
    base = len(demon_fx.EFFECTS) - len(effect_names())
    if len(effects) > base:
        effects = effects[:base]  # 다시 돌릴 때: 우리 효과는 새로 만든다
    lib = demon_fx.Library()
    for (arr, ax, ay) in sprites:
        lib.sprite((np.ascontiguousarray(arr), ax, ay))
    for name, frames in effects:
        lib.add_effect(name, [[tuple(p) for p in parts] for parts in frames])
    add_effects(lib)
    size, shape, n = lib.write(out)
    print(f"{out}: {size:,} B · atlas {shape[1]}×{shape[0]} · {n} sprites · {len(lib.effects)} effects")


def preview(out, src=None):
    """노란 번개 장면 — 게임 배경 위 (176 × 150, 3 배)."""
    import demon_fx
    from PIL import Image, ImageDraw
    sprites, effects = demon_fx.read_bin(src or demon_fx.OUT_BIN)
    E = {n: i for i, (n, _) in enumerate(effects)}
    os.makedirs(out, exist_ok=True)
    mob = demon_fx.real_monster()
    shots = []

    def scene(title, d, k, c, age, mobs):
        cv = demon_fx._background().copy()
        fx, fy = 120, 96
        if d == 0:
            fx, fy = 88, 50
        if d == 1:
            fx, fy = 88, 130
        if d == 2:
            fx, fy = 40, 96
        for (mx, my) in mobs:
            demon_fx._paste(cv, mob[0], mx - mob[1][0], my - mob[1][1])
        demon_fx.draw_effect(cv, sprites, effects, E[f"ilstreak_{DIRS[d]}_{k}{'m0p'[c + 1]}"], min(age, STREAK_LEN - 1), fx, fy)
        demon_fx.draw_effect(cv, sprites, effects, E["ilflash"], age, fx, fy)
        for i, (mx, my) in enumerate(mobs):
            demon_fx.draw_effect(cv, sprites, effects, E[f"ilslash{i & 1}"], age, mx, my)
            demon_fx.draw_effect(cv, sprites, effects, E[f"ilbolt{i & 3}"], age - 1, mx, my)
        shots.append((title, Image.fromarray(np.clip(cv, 0, 255).astype(np.uint8))))

    for age in (0, 1, 3, 5):
        scene(f"오른쪽 5칸 · {age}", 3, 5, 0, age, [(72, 96), (100, 96)])
    scene("위 4칸 · 왼쪽 열 · 1", 0, 4, -1, 1, [(88, 98), (80, 82)])
    scene("아래 3칸 · 0", 1, 3, 0, 0, [(88, 98)])
    scene("왼쪽 5칸 · 오른 열 · 1", 2, 5, 1, 1, [(72, 98), (104, 112)])
    scene("낙뢰 (노랑) · 0", 3, 1, 0, 1, [(104, 96)])
    z = 3
    tw, th = 176 * z, 150 * z
    cols = 4
    rows = (len(shots) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (tw + 8) + 8, rows * (th + 26) + 8), (20, 20, 26))
    dr = ImageDraw.Draw(sheet)
    font = None
    try:
        from PIL import ImageFont
        font = ImageFont.truetype(os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf"), 16)
    except Exception:
        pass
    for i, (title, im) in enumerate(shots):
        x, y = 8 + (i % cols) * (tw + 8), 8 + (i // cols) * (th + 26)
        sheet.paste(im.resize((tw, th), Image.NEAREST), (x, y + 20))
        dr.text((x, y), title, fill=(230, 230, 230), font=font)
    sheet.save(os.path.join(out, "sheet.png"))
    print(f"preview → {out}/sheet.png")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["extend", "preview"])
    ap.add_argument("--src", default=None)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    sys.stdout.reconfigure(errors="replace")
    import demon_fx as _d
    if args.cmd == "extend":
        extend(args.src or _d.OUT_BIN, args.out or _d.OUT_BIN)
    else:
        preview(args.out or os.path.join(ROOT, "docs", "skill", "ilseom_bolt"), args.src)
