#!/usr/bin/env python3
r"""번개 이펙트 7 차 (2026-10-17) — 어둠심판의 낙뢰 · 일섬 「번개의 호흡」의 노란 번개를 절차 그림 + HD(3×)로.

사용자: *"현재 번개 이펙트(악의 어둠 번개와 일섬 번개)에서 번개 말고 이상한 이펙트가 상단쪽에 나오는거같은데 이거 수정해주고 이펙트
업스케일링 해줘."*

옛 낙뢰(3 차 · `demon_fx_cut.bolt`)는 예상도 09 에서 **오린 한 장**이었다 — 줄기 위쪽에 HUD 의 「LV」 글자 조각, 오른쪽에 살색 조각,
가운데 인페인트의 회색 사각, 줄기 둘레의 검은 얼룩이 같이 오려져 있었고(`docs/skill/bolt_fx/before.png`), 일섬의 노란 낙뢰
(`ilbolt0..3`)는 그것을 그대로 노랗게 물들인 것이라 같은 조각을 달고 나왔다. 일섬의 줄기 · 섬광 · 베기(`ilstreak_*` · `ilflash` ·
`ilslash0/1`)는 `extend` 로 붙어 HD 쌍이 없었다(1× 를 창에서 키움).

이 파일:
* `bolt(variant, age, pal)` — 하늘에서 떨어지는 낙뢰 (4 그리기). 하늘 끝은 위로 갈수록 투명해진다(잘린 끝 없음) · 어두운 테 + 몸 + 흰 심
  의 꺾인 줄기와 가지 · 발밑 충격(빛살 · 섬광) · 몸을 감는 잔전기 · 땅을 퍼지는 전기 고리 · 튀는 불티 · (어둠) 보라 연기.
  0 = 내리꽂힘 · 1 = 한 번 더 번쩍(곁줄기) · 2 = 줄기가 위에서부터 끊긴다 · 3 = 고리와 불티만 남아 사라진다.
  팔레트 둘 — `DARK`(어둠심판: 검보라 테 · 보라 몸 · 자홍 빛 · 흰 심) · `GOLD`(일섬: 호박 테 · 금빛 몸 · 노랑 · 흰 심), 모양은 같다
  (「악의의 암흑 번개 노란 번개색 버전」).
* `replace` — `fx_swap.replace` 로 bin 의 `bolt0..3` · `ilbolt0..3` 를 이 그림으로, `ilstreak_*` · `ilflash` · `ilslash0/1` 는
  `ilseom_fx` 의 그림 그대로 3× 그림판에서 다시 그려 HD 쌍을 싣는다. 다른 효과는 바이트 그대로. 효과 번호 · 길이(계약) 불변.

    python tools_re/bolt_fx.py replace [--src wie_android/src/demon_fx.bin] [--out …]
    python tools_re/bolt_fx.py preview --out docs/skill/bolt_fx     # 3× 시트 + GIF (실제 게임 배경 · 몬스터)
"""

import argparse
import math
import os
import random
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fxpaint as fp  # noqa: E402
from fxpaint import Canvas  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

BOLT_LEN = 4  # = demon_fx.BOLT_LEN · AccessoryPatch::FX_IL_BOLT_LEN
TOP = -186  # 줄기의 하늘 끝 (발 기준 게임 px — 화면 220 의 위로 넉넉히)
FADE_TO = -128  # 하늘 끝에서 여기까지 알파 0 → 1
HIT_Y = -6  # 줄기가 꽂히는 높이 (몸 가운데 아래)
TOPS = (-8, 11, -15, 6)  # 변형마다 하늘 끝의 x

DARK = dict(
    glow=(150, 40, 230), rim=(30, 4, 48), body=(132, 30, 200), mid=(222, 104, 255), core=(255, 236, 255),
    spark=(240, 168, 255), ring=(196, 64, 232), flash=(255, 206, 255), scorch=(38, 6, 50), smoke=True, haze=0.42,
)
GOLD = dict(
    glow=(255, 194, 40), rim=(104, 56, 0), body=(250, 182, 18), mid=(255, 236, 92), core=(255, 255, 238),
    spark=(255, 248, 178), ring=(255, 206, 56), flash=(255, 244, 196), scorch=(64, 36, 0), smoke=False, haze=0.0,
)


def _smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def _fade_top(c, y0, y1):
    """y0(투명) → y1(그대로) 의 세로 램프를 그림판 전체에 곱한다 — 하늘 끝이 잘리지 않고 스러진다."""
    s = c.ss
    y = (np.arange(c.h * s, dtype=np.float32) + 0.5) / s - c.ay
    f = np.clip((y - y0) / (y1 - y0), 0, 1)
    f = f * f * (3 - 2 * f)
    c.base *= f[:, None, None]
    c.add *= f[:, None, None]


def _bolt_line(c, pts, w, pal, alpha):
    """번개 한 줄: (어둠) 검보라 아지랑이 · 넓은 빛(더하기) 둘 · 어두운 테 · 몸 · 밝은 몸 · 흰 심."""
    if pal["haze"]:
        c.stroke(pts, w * 4.4, pal["rim"], pal["haze"] * alpha)
    c.stroke(pts, w * 6.0, pal["glow"], 0.16 * alpha, "add")
    c.stroke(pts, w * 3.0, pal["glow"], 0.3 * alpha, "add")
    c.stroke(pts, w * 2.0, pal["rim"], 0.85 * alpha)
    c.stroke(pts, w * 1.3, pal["body"], alpha)
    c.stroke(pts, w * 0.8, pal["mid"], alpha)
    c.stroke(pts, max(0.45, w * 0.34), pal["core"], alpha)


def _path(variant, flicker=0):
    rnd = random.Random(8800 + 37 * variant + 1000 * flicker)
    x0 = TOPS[variant % 4] + (rnd.uniform(-5, 5) if flicker else 0)
    return Canvas.bolt_pts(None, x0, TOP, 0.0, HIT_Y, 6, 0.17, rnd), rnd


def _branches(pts, rnd, n):
    """줄기의 위 ¾ 에서 아래 옆으로 갈라지는 가지 (가지의 잔가지 하나씩)."""
    out = []
    for b in range(n):
        i = rnd.randrange(len(pts) // 8, len(pts) * 3 // 4)
        x0, y0 = pts[i]
        side = -1 if b % 2 == 0 else 1
        reach = rnd.uniform(14, 30)
        x1, y1 = x0 + side * reach, y0 + rnd.uniform(10, 26)
        bp = Canvas.bolt_pts(None, x0, y0, x1, y1, 3, 0.36, rnd)
        out.append((bp, 1.0))
        j = rnd.randrange(len(bp) // 3, len(bp) - 1)
        sx, sy = bp[j]
        sp = Canvas.bolt_pts(None, sx, sy, sx + side * rnd.uniform(5, 11), sy + rnd.uniform(5, 12), 2, 0.4, rnd)
        out.append((sp, 0.7))
    return out


def _ring(c, rx, ry, pal, alpha, seed, w=1.0):
    """땅을 퍼지는 전기 고리: 매끈한 타원 테(빛) + 그 위를 달리는 꺾인 전기."""
    if alpha <= 0.01:
        return
    c.ellipse(0, 0, rx, ry, pal["glow"], 0.35 * alpha, "add", width=2.6 * w)
    c.ellipse(0, 0, rx, ry, pal["rim"], 0.5 * alpha, width=1.6 * w)
    c.ellipse(0, 0, rx, ry, pal["ring"], 0.9 * alpha, width=0.9 * w)
    rnd = random.Random(seed)
    n = 26
    pts = []
    for i in range(n + 1):
        th = 2 * math.pi * i / n
        k = 1.0 + rnd.uniform(-0.12, 0.12)
        pts.append((math.cos(th) * rx * k, math.sin(th) * ry * k))
    # 고리를 세 토막으로 — 끊긴 전기
    for a0 in range(0, n, 9):
        seg = pts[a0:a0 + 6]
        if len(seg) >= 2:
            c.stroke(seg, 0.9 * w, pal["mid"], 0.85 * alpha)
            c.stroke(seg, 0.45, pal["core"], 0.8 * alpha)


def _arcs(c, cx, cy, n, r0, r1, pal, alpha, rnd, w=0.55):
    """몸을 감는 잔전기 — 가운데 둘레의 짧은 번개 n 개."""
    for _ in range(n):
        th = rnd.uniform(0, 2 * math.pi)
        rr = rnd.uniform(r0, r1)
        x0, y0 = cx + math.cos(th) * rr * 0.4, cy + math.sin(th) * rr * 0.5
        x1, y1 = cx + math.cos(th + rnd.uniform(0.8, 1.6)) * rr, cy + math.sin(th + rnd.uniform(0.8, 1.6)) * rr * 1.1
        pts = Canvas.bolt_pts(None, x0, y0, x1, y1, 2, 0.5, rnd)
        c.stroke(pts, w * 3.2, pal["glow"], 0.3 * alpha, "add")
        c.stroke(pts, w * 1.6, pal["body"], 0.9 * alpha)
        c.stroke(pts, w * 0.8, pal["core"], alpha)


def _sparks(c, age, pal, alpha, seed, n=16):
    """튀는 불티: 발밑에서 위 · 옆으로 튀었다가 떨어진다 — 움직인 쪽으로 꼬리."""
    rnd = random.Random(seed)
    t = age + 1.0
    for _ in range(n):
        th = rnd.uniform(math.pi * 1.08, math.pi * 1.92)
        sp = rnd.uniform(2.6, 5.4)
        vx, vy = math.cos(th) * sp * 1.5, math.sin(th) * sp
        x, y = vx * t, HIT_Y + 2 + vy * t + 0.9 * t * t
        tx, ty = x - vx * 0.45, y - (vy + 1.8 * t) * 0.45
        a = alpha * rnd.uniform(0.6, 1.0)
        c.stroke([(tx, ty), (x, y)], 0.8, pal["spark"], a, "add")
        c.ellipse(x, y, 0.55, 0.55, pal["core"], a, "add")


def bolt(variant, age, pal):
    """하늘에서 떨어지는 낙뢰 한 그림 (기준점 = 맞는 개체의 발바닥)."""
    c = Canvas(120, 218, 60, 200)
    pts, rnd = _path(variant)
    branches = _branches(pts, rnd, 5)
    seed = 9100 + 53 * variant
    if age == 0:
        c.glow(0, HIT_Y - 4, 16, 22, pal["glow"], 0.5)  # 땅에 닿는 빛기둥
        for bp, k in branches:
            _bolt_line(c, bp, 1.25 * k, pal, 0.95)
        _bolt_line(c, pts, 3.0, pal, 1.0)
        c.glow(0, HIT_Y, 11, 8, pal["flash"], 0.95)
        c.star(0, HIT_Y + 1, 12, 1.6, 27, pal["flash"], 0.95, phase=0.25, squash=0.5, rnd=random.Random(seed))
        c.star(0, HIT_Y + 1, 6, 1.2, 14, pal["core"], 1.0, phase=0.0, squash=0.6, core=pal["core"])
        _ring(c, 11, 4.0, pal, 0.85, seed + 1, 1.2)
        _arcs(c, 0, HIT_Y - 6, 4, 6, 13, pal, 1.0, random.Random(seed + 2))
        _sparks(c, 0, pal, 1.0, seed + 3)
    elif age == 1:
        c.glow(0, HIT_Y - 4, 13, 18, pal["glow"], 0.35)
        for bp, k in branches[:4]:
            _bolt_line(c, bp, 0.9 * k, pal, 0.75)
        twin, _ = _path(variant, flicker=1)  # 한 번 더 번쩍 — 엇갈린 곁줄기
        _bolt_line(c, twin, 1.1, pal, 0.55)
        _bolt_line(c, pts, 2.2, pal, 0.95)
        c.glow(0, HIT_Y, 9, 6, pal["flash"], 0.7)
        c.star(0, HIT_Y + 1, 8, 1.2, 15, pal["flash"], 0.7, phase=0.55, squash=0.5, rnd=random.Random(seed + 4))
        _ring(c, 17, 6.0, pal, 0.8, seed + 5)
        _arcs(c, 0, HIT_Y - 6, 3, 7, 15, pal, 0.85, random.Random(seed + 6))
        _sparks(c, 1, pal, 0.9, seed + 3)
    elif age == 2:
        # 줄기가 위에서부터 끊긴다: 아래 마디일수록 남는다
        r2 = random.Random(seed + 7)
        n = len(pts) - 1
        for i in range(n):
            keep = 0.15 + 0.85 * (i / n) ** 1.4
            if r2.random() < keep:
                _bolt_line(c, [pts[i], pts[i + 1]], 1.2, pal, 0.55 + 0.35 * (i / n))
        c.glow(0, HIT_Y + 1, 9, 5, pal["glow"], 0.4)
        _ring(c, 23, 8.0, pal, 0.6, seed + 8)
        _arcs(c, 0, HIT_Y - 4, 2, 6, 13, pal, 0.6, random.Random(seed + 9), w=0.45)
        _sparks(c, 2, pal, 0.7, seed + 3)
        if pal["smoke"]:
            for k in range(3):
                c.blob((k - 1) * 8, HIT_Y - 6, 6, 4, fp.VIOLET, seed=seed + k, rough=0.6, alpha=0.32, scale=0.35)
    else:
        c.glow(0, 0, 14, 5, pal["glow"], 0.22)
        _ring(c, 28, 9.5, pal, 0.3, seed + 10, 0.8)
        _arcs(c, 0, -2, 1, 5, 11, pal, 0.35, random.Random(seed + 11), w=0.4)
        _sparks(c, 3, pal, 0.4, seed + 3, n=10)
        if pal["smoke"]:
            for k in range(3):
                c.blob((k - 1) * 9, HIT_Y - 11, 7, 4.6, fp.VIOLET, seed=seed + k, rough=0.6, alpha=0.2, scale=0.35)
    # 그을음 (덮기, 땅) — 1 그림부터
    if age >= 1:
        c.glow(0, 1, 13 + 2 * age, 4 + age * 0.8, pal["scorch"], (0.0, 0.42, 0.34, 0.22)[age], "base")
    _fade_top(c, TOP, FADE_TO)
    return c


# ── 일섬 「번개의 호흡」 — ilseom_fx 의 그림 그대로 · HD 쌍 ──────────────────────────────────────────────────────
# ★ 메모리: 쌍 픽셀 예산(`test_demon_fx` 45 M) 안에서 — 다른 효과가 42.4 M 을 쓰고 이 그림들은 3× 로 7.5 M 이라 넘쳤다. 그래서
#   6× 그림판(`DRAW_SS`)에 그리고 쌍은 3:1 블록 평균의 **2×**(`TWIN`, 가장자리 안티앨리어싱 · 픽셀 4/9) · 1× 는 6:1 평균 ·
#   낙뢰 변형 2 · 3 은 0 · 1 의 좌우 반전(같은 스프라이트) — 합 ≈ 2.4 M. 창(S25 ≈ 4.9×)에서는 2× 쌍이 최근접으로 커진다.
DRAW_SS = 6
TWIN = 2
FLIP = 1  # = demon_fx.FLIP


def _hd(fn):
    """fn(age) → 그림판 | [(그림판, dx, dy[, 알파[, 플래그]])] 를 6× 그림판 · 2× 쌍으로 — 그 설정이 살아 있는 동안 1× 와 쌍을 미리
    계산해 둔다(`ilseom_fx.Fixed`; `fx_swap.replace` 는 그림 함수가 돌아온 뒤에 쌍을 묻는다)."""
    import ilseom_fx as il

    def g(age):
        old = (fp.SS, fp.TWIN)
        fp.set_ss(DRAW_SS)
        fp.set_twin(TWIN)
        try:
            r = fn(age)
            pieces = r if isinstance(r, list) else [(r, 0, 0)]
            return [((p[0] if isinstance(p[0], il.Fixed) else il.Fixed(p[0])),) + tuple(p[1:]) for p in pieces]
        finally:
            fp.set_ss(old[0])
            fp.set_twin(old[1])
    return g


def ilseom_gens():
    """`fx_swap.replace` 의 그림 함수들 — 줄기 · 섬광 · 베기는 `ilseom_fx` 의 함수(그림 같음), 낙뢰는 `bolt` 의 금빛."""
    import ilseom_fx as il
    cache = {}

    def drawing(axis, k, L, v):
        key = (axis, k, L, v)
        if key not in cache:
            cache[key] = il._streak(axis, k, L, v)  # an `il.Fixed`, made under `_hd`'s 6× · 2× (the first call is inside it)
        return cache[key]

    gens = {}
    for di, d in enumerate(il.DIRS):
        for k in range(1, il.DASH + 1):
            for col in (-1, 0, 1):
                axis, L, dx, dy = il.streak_part(di, k, col)

                def streak(age, axis=axis, k=k, L=L, dx=dx, dy=dy):
                    v, a = il.STREAK_FRAMES[age]
                    return [(drawing(axis, k, L, v), dx, dy, a)]
                gens[f"ilstreak_{d}_{k}{'m0p'[col + 1]}"] = _hd(streak)
    gens["ilflash"] = _hd(il.flash)
    for v in range(2):
        gens[f"ilslash{v}"] = _hd(lambda age, v=v: il.slash(v, age))
    gens.update(_bolt_gens("ilbolt", GOLD))
    return gens


def _bolt_gens(stem, pal):
    """낙뢰 넷: 0 · 1 은 그림, 2 · 3 은 그 좌우 반전 (같은 스프라이트)."""
    import ilseom_fx as il
    cache = {}

    def drawn(v, age):
        if (v, age) not in cache:
            cache[(v, age)] = il.Fixed(bolt(v, age, pal))  # under `_hd`'s 6× · 2× (called inside it)
        return cache[(v, age)]
    gens = {}
    for v in range(4):
        gens[f"{stem}{v}"] = _hd(lambda age, v=v: [(drawn(v % 2, age), 0, 0, 255, FLIP if v >= 2 else 0)])
    return gens


def all_gens():
    gens = _bolt_gens("bolt", DARK)
    gens.update(ilseom_gens())
    return gens


def preview(out, src=None, k=3):
    import fx_swap
    os.makedirs(out, exist_ok=True)
    mobs = [(70, 92), (122, 100)]
    seq = []
    for age in range(BOLT_LEN + 1):
        seq.append((f"어둠심판 낙뢰 · {age}", [("t", "bolt0", age, 70, 92), ("t", "bolt2", age - 1, 122, 100)]))
    for age in range(BOLT_LEN + 1):
        seq.append((f"일섬 노란 낙뢰 · {age}", [("t", "ilbolt0", age, 70, 92), ("t", "ilbolt1", age - 1, 122, 100)]))
    for age in range(8):
        lay = [("t", "ilstreak_right_50", age, 150, 104), ("t", "ilflash", age, 150, 104)]
        for i, (mx, my) in enumerate([(108, 104), (124, 104)]):
            s = age
            lay.append(("t", f"ilslash{i & 1}", s, mx, my))
            lay.append(("t", f"ilbolt{i & 3}", s - 1, mx, my))
        seq.append((f"일섬 오른쪽 5칸 · {age}", lay))
    fx_swap.preview_scenes(out, "bolt", seq, src=src, k=k, mobs=mobs + [(108, 104), (124, 104)], duration=100, cols=5)


def main():
    import demon_fx
    import fx_swap
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("replace")
    r.add_argument("--src", default=demon_fx.OUT_BIN)
    r.add_argument("--out", default=demon_fx.OUT_BIN)
    p = sub.add_parser("preview")
    p.add_argument("--out", default=os.path.join(ROOT, "docs", "skill", "bolt_fx"))
    p.add_argument("--src", default=None)
    a = ap.parse_args()
    sys.stdout.reconfigure(errors="replace")
    if a.cmd == "replace":
        fx_swap.replace(a.src, a.out, all_gens())
    else:
        preview(a.out, a.src)


if __name__ == "__main__":
    main()
