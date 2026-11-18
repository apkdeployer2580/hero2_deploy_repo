#!/usr/bin/env python3
"""「악마화」 지옥칼날 이펙트 6 차 (2026-10-16) — 오린 그림 대신 절차 그림: 지옥에서 칼날이 휘몰아치는 인장.

사용자: *"지옥칼날 시전시 이펙트의 끝쪽이 뭔가 한번에 잘린느낌이라 그라데이션으로 점점 투명화되는게 좀 더 자연스러울것 같아. 업스케일링
해줘"* · *"지옥칼날 이펙트가 뭔가 그냥 캡쳐해서 이미지 잘린게 있는 듯한 느낌이야. 지옥에서 칼날이 휘몰아치고 있는 듯한 느낌으로
업스케일링 해줘"*.

옛 그림(3 차 · `demon_fx_cut.seal` · `blade_rise`)은 예상도 08 에서 오린 두 장 — 인장은 극좌표로 메운 타원(테가 한 번에 끊긴다) ·
칼날은 보라 수정 한 장이 솟았다 가라앉았다(위 끝이 그림 상자에서 잘린다). 이 파일은 둘 다 **절차 그림**으로 다시 그린다:

* `seal` (땅 층, 32 반복 — FXG 가 `TICKS & 31`): 발밑의 검붉은 웅덩이(가우스 — 가장자리가 점점 투명) · 부드러운 고리 둘(가우스 띠) ·
  돌아가는 룬 띠 · **고리를 따라 휘몰아치는 칼날 다섯**(초승달 참격, 뒤로 흐르는 잔상 둘, 앞쪽이 크고 뒤쪽이 작다 — 뛰어올랐다
  내려앉으며 돈다) · 칼날이 고리를 긁는 불꽃 · 떠오르는 불티 · 뒤에서 오르는 검은 연기. 32 프레임에 칼날 한 바퀴 · 룬 1/16 바퀴 ·
  불티 · 연기 한 주기 = 이음매 없음.
* `blade_rise0 · 1` (땅 층, 16 반복 — FXG 가 몬스터마다 `(TICKS + 5i) & 15`, 변형 1 = 거울): 몬스터 발밑의 붉은 균열 웅덩이에서
  **칼날 셋이 나선으로 솟아오르며 위로 갈수록 옅어진다**(알파 = (1 − p)^1.3 — 끝이 그라데이션으로 사라진다, 잘리지 않는다) · 불꽃.

판정은 그대로(`DemonPatch::BLADE_WINDOW` 타원 6 × 3 칸 = ±96 × ±48 px): 인장 ±80 × ±24 + 칼날 ≤ ±92 · 칼날 솟음 ≤ 56 px 위.

    python tools_re/blade_fx.py replace [--src wie_android/src/demon_fx.bin] [--out …]   # 다른 효과는 바이트 그대로, 인장 · 칼날만 새로
    python tools_re/blade_fx.py preview --out docs/skill/demon_blade_hd                    # 3× 시트 + GIF (실제 게임 배경 · 악마 HD 몸)
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
import demon_fx_art as art  # noqa: E402  — RUNES
from fx_swap import ell, smooth  # noqa: E402

SEAL_LEN = 32  # = demon_fx.SEAL_LEN (FXG: TICKS & 31)
BLADE_LEN = 16  # = demon_fx.BLADE_LEN (FXG: (TICKS + 5i) & 15)
RX, RY = 80.0, 24.0  # 인장 타원 — 판정 BLADE_WINDOW(6 × 3 칸 = ±96 × ±48) 안
N_BLADES = 5
RISE_H = 50.0  # 몬스터 곁 칼날이 솟는 높이 (끝은 투명)

# ── 색 ─────────────────────────────────────────────────────────────────────────────────────────────────────
# 칼날: 검은 강철 → 진홍 → 자홍 날 → 흰 심 (crescent 의 I 값 띠)
# ⛔ `bands` 는 첫 문턱 아래를 투명으로 둔다 — 첫 문턱은 0 보다 커야 한다 (crescent 는 호 밖을 −1 로 둔다; −9 면 상자 전체가 칠해진다)
# 검은 강철 몸(I 낮음) → 날 쪽(I ≥ 0.72)만 진홍 · 자홍 · 흰 심 — bias 0.78 이라 볼록한 날에 밝은 띠가 몰린다
STEEL = [(0.02, (12, 4, 12), 1.0), (0.38, (36, 12, 32), 1.0), (0.6, (96, 20, 52), 1.0), (0.74, (184, 36, 78), 1.0),
         (0.86, (240, 96, 136), 1.0), (0.94, (255, 214, 226), 1.0), (1.02, (255, 248, 250), 1.0)]
GHOST = [(0.06, (40, 6, 24), 0.75), (0.4, (100, 18, 50), 0.8), (0.75, (160, 40, 90), 0.7)]  # 잔상 — 흐리고 옅다 (알파는 호출자가)
SMOKE = [(0.08, (24, 6, 16), 0.5), (0.35, (46, 14, 32), 0.65), (0.7, (70, 26, 50), 0.55)]
POOL = (120, 0, 16)
POOL_DEEP = (36, 0, 8)
RING = (210, 24, 36)
RING_HOT = (255, 92, 70)
RUNE = (255, 70, 60)
EMBER = (255, 120, 60)
SPARK = (255, 150, 90)


def _cut(a, lo=0.035):
    """가우스 꼬리를 0 으로 — HD 쌍의 양자화(FASTOCTREE, 알파 포함)가 1~3 % 알파의 넓은 꼬리를 진한 팔레트 칸에 붙여 어두운 상자를
    만든다. 문턱 아래는 투명, 문턱 바로 위는 짧게 섞는다."""
    return np.where(a < lo, 0.0, a * smooth(lo, lo * 2.2, a))


def _soft_ring(c, rx, ry, width, color, alpha, layer="base", inner=1.0, outer=1.0):
    """타원 (rx, ry) 위의 부드러운 띠 — 반지름 방향 가우스(안쪽 폭 inner · 바깥 outer, 단위 = width). 테가 끊기지 않는다."""
    def f(X, Y):
        d = np.sqrt((X / rx) ** 2 + (Y / ry) ** 2)
        s = (d - 1.0) * rx / width
        g = np.where(s < 0, np.exp(-(s / inner) ** 2), np.exp(-(s / outer) ** 2))
        return np.asarray(color, np.float32), _cut(g * alpha)
    ext_x, ext_y = rx + width * 3.5, ry + width * 3.5 * ry / rx + 2
    c.field((-ext_x, -ext_y, ext_x, ext_y), f, layer, 1.0)


def _pool_glow(c, cx, cy, rx, ry, color, alpha, layer="base", edge=1.45):
    """가장자리가 점점 투명한 웅덩이 빛 — 가우스 × 바깥 `edge` 반지름에서 0 으로 닫히는 문턱 (꼬리 없음)."""
    def f(X, Y):
        u = (X - cx) / rx
        v = (Y - cy) / ry
        d = np.sqrt(u * u + v * v)
        g = np.exp(-d * d * 1.6) * smooth(edge, edge * 0.7, d)
        return np.asarray(color, np.float32), _cut(g * alpha)
    c.field((cx - rx * edge, cy - ry * edge, cx + rx * edge, cy + ry * edge), f, layer, 1.0)


def _pool(c, u, scale=1.0, alpha=1.0):
    """검붉은 웅덩이: 가장자리가 점점 투명한 빛 둘 + 뜨거운 가운데."""
    pulse = 0.5 + 0.5 * math.sin(2 * math.pi * u)
    _pool_glow(c, 0, 0, RX * 1.08 * scale, RY * 1.12 * scale, POOL_DEEP, 0.6 * alpha)
    _pool_glow(c, 0, 0, RX * 0.86 * scale, RY * 0.88 * scale, POOL, (0.6 + 0.08 * pulse) * alpha)
    c.glow(0, 1, RX * 0.36 * scale, RY * 0.36 * scale, (255, 70, 36), (0.16 + 0.08 * pulse) * alpha, "add")


def _blade(c, x, y, chord, bulge, size, alpha, seed, ghost=False, stripes=0.0):
    """참격 하나: 현의 가운데 (x, y) · 현 방향 chord · 볼록한 쪽 bulge · 크기 size(현 길이 ≈ 1.5 · size). 몸은 검은 강철, 볼록한
    날에만 진홍 → 흰 심; 잔상은 흐린 띠."""
    R, half, thick = 0.78 * size, 0.8, 0.3 * size
    if ghost:
        c.slash(x, y, chord, bulge, half, R, thick * 1.05, GHOST, alpha=alpha, soft=1.4, erode=0.25, seed=seed, rough=0.1, fade=0.8)
        return
    c.slash(x, y, chord, bulge, half, R, thick, STEEL, alpha=alpha, seed=seed, rough=0.1, stripes=stripes, nstripes=2, bias=0.78, power=1.3)
    # 날 쪽 빛 무리 (더하기)
    c.slash(x, y, chord, bulge, half, R, thick * 1.4, STEEL, alpha=alpha * 0.4, seed=seed, glow=(255, 70, 110))


def _finish(c, lo=0.1):
    """그림판의 덮기 층에서 알파 `lo` 아래를 지운다(HD 쌍의 양자화가 옅은 꼬리를 진한 칸에 붙이지 않게 — 덮기 층은 곱해진 알파)."""
    a = c.base[..., 3]
    keep = a >= lo
    c.base[~keep] = 0.0
    return c


def seal(f):
    """지옥칼날 인장 (땅 층, 32 반복)."""
    u = f / SEAL_LEN
    c = Canvas(204, 110, 102, 68)  # x −102..102 · y −68..42 (발 = 0, 0)
    _pool(c, u)
    # 고리 둘 + 뜨거운 바깥 테 (가우스 띠 — 끊기지 않는다)
    _soft_ring(c, RX, RY, 2.6, RING, 0.9, inner=1.0, outer=1.6)
    _soft_ring(c, RX, RY, 5.0, RING_HOT, 0.28, "add", inner=0.8, outer=1.8)
    _soft_ring(c, RX * 0.74, RY * 0.74, 1.8, (160, 14, 30), 0.8, inner=1.0, outer=1.3)
    _soft_ring(c, RX * 0.74, RY * 0.74, 4.0, RING_HOT, 0.18, "add")
    # 룬 띠 (16 칸 · 32 프레임에 1/16 바퀴 → 이음매 없음) — 앞쪽이 크고 밝다
    ph = u * 2 * math.pi / 16
    for k in range(16):
        t = k * 2 * math.pi / 16 + ph
        x, y = ell(t, RX * 0.87, RY * 0.87)
        depth = (math.sin(t) + 1) / 2
        sc = 1.05 + 0.5 * depth
        g = art.RUNES[k % len(art.RUNES)]
        for line in g:
            pts = [(x + (px - 2) * sc, y + (py - 2) * sc * 0.55) for px, py in line]
            c.stroke(pts, 0.8, RUNE, 0.55 + 0.45 * depth)
            c.stroke(pts, 2.0, (255, 50, 50), 0.18 + 0.12 * depth, "add")
    # 검은 연기 — 뒤쪽 고리에서 오른다 (한 주기 = 32)
    rnd = random.Random(6100)
    for k in range(4):
        t = (u + k / 4) % 1.0
        a0 = rnd.uniform(math.pi * 1.15, math.pi * 1.85)  # 뒤쪽 호
        x, y = ell(a0, RX * 0.95, RY * 0.95)
        sa = 0.6 * math.sin(math.pi * t) ** 0.8
        if sa >= 0.2:
            c.blob(x + 4 * math.sin(t * 6 + k), y - 6 - t * 30, 7 + 6 * t, 5 + 5 * t, SMOKE, seed=70 + k, rough=0.45, alpha=sa, scale=0.45)
    # 칼날 다섯 — 고리를 따라 한 바퀴 (32 프레임), 뛰어올랐다 내려앉으며 (두 번), 뒤쪽은 작고 옅다
    blades = []
    for k in range(N_BLADES):
        th = 2 * math.pi * (u + k / N_BLADES)
        depth = (math.sin(th) + 1) / 2  # 0 = 뒤 · 1 = 앞
        hop = 8 + 10 * (0.5 + 0.5 * math.sin(2 * math.pi * (2 * u + 2 * k / N_BLADES)))
        blades.append((depth, th, hop, k))
    for depth, th, hop, k in sorted(blades):  # 뒤에서 앞으로
        x, y = ell(th, RX, RY)
        tx, ty = -RX * math.sin(th), RY * math.cos(th)  # 접선 (진행 방향)
        chord = math.atan2(ty, tx) + 0.22 * (1 if math.cos(th) >= 0 else -1) * -1
        # 볼록한 쪽 = 위 + 바깥
        nx, ny = math.cos(th) / RX, math.sin(th) / RY
        nn = math.hypot(nx, ny) or 1.0
        bulge = math.atan2(ny / nn * 0.45 - 1.0, nx / nn * 0.45)
        size = (26 + 14 * depth)
        alpha = 0.8 + 0.2 * depth
        # 잔상 둘 (뒤쪽 각으로 — 흐리고 옅다)
        for j, (dth, ga) in enumerate(((0.16, 0.45), (0.32, 0.22))):
            gx, gy = ell(th - dth, RX, RY)
            ghop = 8 + 10 * (0.5 + 0.5 * math.sin(2 * math.pi * (2 * (u - dth / (2 * math.pi)) + 2 * k / N_BLADES)))
            _blade(c, gx, gy - ghop, chord, bulge, size * (1 - 0.08 * (j + 1)), alpha * ga, seed=300 + k * 7 + j, ghost=True)
        _blade(c, x, y - hop, chord, bulge, size, alpha, seed=200 + k * 7 + f % 4)
        # 고리를 긁는 불꽃 (칼날이 낮을 때 밝다)
        low = 1 - min(1.0, (hop - 8) / 10)
        c.glow(x, y, 7, 3, SPARK, (0.22 + 0.5 * low) * alpha)
        if low > 0.55:
            rnd2 = random.Random(900 + k * 13 + f)
            for _ in range(3):
                ang = rnd2.uniform(math.pi * 1.1, math.pi * 1.9)
                L = rnd2.uniform(3, 9)
                c.stroke([(x, y), (x + math.cos(ang) * L, y + math.sin(ang) * L * 0.7)], 0.7, (255, 200, 150), 0.7 * low, "add")
    # 불티 (한 주기 = 32 — 안쪽에서 떠오른다)
    rnd = random.Random(8000)
    for k in range(16):
        t = (u + k / 16) % 1.0
        a = rnd.uniform(0, 6.28)
        rr = rnd.uniform(0.25, 0.95)
        x, y = ell(a, RX * rr, RY * rr)
        x += 3 * math.sin(t * 7 + k)
        c.ellipse(x, y - 2 - t * 26, 0.9, 0.9, EMBER, math.sin(math.pi * t) * 0.9, "add")
        if k % 4 == 0:
            c.glow(x, y - 2 - t * 26, 2.2, 2.2, (255, 60, 40), 0.35 * math.sin(math.pi * t))
    return _finish(c)


def blade_rise(variant, f):
    """몬스터 곁에서 나선으로 솟는 칼날 셋 (땅 층, 16 반복) — 위로 갈수록 옅어져 사라진다 (잘리지 않는다). 변형 1 = 거울."""
    u = f / BLADE_LEN
    sgn = 1.0 if variant == 0 else -1.0
    c = Canvas(100, 112, 50, 88)  # x −50..50 · y −88..24 (발 = 0, 0)
    pulse = 0.5 + 0.5 * math.sin(2 * math.pi * u)
    # 발밑의 붉은 웅덩이 + 균열
    _pool_glow(c, 0, 1, 17, 6.5, POOL_DEEP, 0.55)
    _pool_glow(c, 0, 1, 12, 4.6, POOL, 0.6 + 0.1 * pulse)
    c.glow(0, 1, 6, 2.4, (255, 70, 36), 0.18 + 0.1 * pulse, "add")
    rnd = random.Random(500 + variant)
    for k in range(4):
        th = k * math.pi / 2 + 0.35 + 0.4 * variant
        pts = [(0, 1), (math.cos(th) * 7, 1 + math.sin(th) * 2.4), (math.cos(th + 0.35) * 13, 1 + math.sin(th + 0.35) * 3.8)]
        c.taper(pts, 1.6, 0.3, (46, 4, 10), 0.85)
        c.taper(pts, 0.8, 0.25, RING_HOT, 0.75 + 0.2 * pulse)
    # 칼날 셋 — 나선으로 솟는다
    blades = []
    for k in range(3):
        p = (u + k / 3) % 1.0
        th = 2 * math.pi * (p * 0.9 + k / 3) * sgn
        r = 14 - 7 * p
        x = math.cos(th) * r
        depth = (math.sin(th) + 1) / 2
        blades.append((depth, p, th, x, k))
    for depth, p, th, x, k in sorted(blades):
        y = -6 - RISE_H * p
        fade_in = min(1.0, p * 5 + 0.15)
        alpha = (1 - p) ** 1.2 * fade_in * (0.75 + 0.25 * depth)
        size = (29 - 8 * p) * (0.8 + 0.3 * depth)
        # 진행 방향 = 나선의 접선 (옆으로 돌며 위로)
        tx, ty = -math.sin(th) * r * sgn * 0.9, -RISE_H / (2 * math.pi) * 0.5
        chord = math.atan2(ty, tx)
        bulge = math.atan2(-1.0, math.cos(th) * 0.6)  # 위 · 바깥
        # 잔상 (조금 아래 · 조금 뒤)
        pg = max(0.0, p - 0.07)
        thg = 2 * math.pi * (pg * 0.9 + k / 3) * sgn
        xg = math.cos(thg) * (14 - 7 * pg)
        _blade(c, xg, -6 - RISE_H * pg, chord, bulge, size * 0.92, alpha * 0.4, seed=400 + k * 11 + variant, ghost=True)
        _blade(c, x, y, chord, bulge, size, alpha, seed=600 + k * 11 + variant + f % 3, stripes=0.12)
    # 불꽃 — 웅덩이에서 솟아 흩어진다
    rnd = random.Random(7000 + variant)
    for k in range(10):
        t = (u + k / 10) % 1.0
        a = rnd.uniform(0, 6.28)
        x0 = math.cos(a) * rnd.uniform(2, 11)
        vx = rnd.uniform(-6, 6)
        c.ellipse(x0 + vx * t, 1 - t * 34 - 2 * t * t, 0.8, 0.8, SPARK, math.sin(math.pi * t) * 0.9, "add")
    return _finish(c)


EFFECTS = {"seal": seal, "blade_rise0": lambda f: blade_rise(0, f), "blade_rise1": lambda f: blade_rise(1, f)}


def preview(out, src=None, k=3):
    import fx_swap
    seq = []
    FX, FY = 88, 104
    mobs = fx_swap.mob_feet_default()
    for a in range(SEAL_LEN):
        lay = [("g", "seal", a, FX, FY)] + [("g", f"blade_rise{i & 1}", (a + 5 * i) & 15, mx, my) for i, (mx, my) in enumerate(mobs)]
        seq.append((f"지옥칼날 {a}", lay))
    fx_swap.preview_scenes(out, "blade", seq, src=src, k=k, demon_pose=("blade_thrust", 1), mobs=mobs, duration=50,
                           pick=[0, 2, 4, 6, 8, 10, 12, 14, 16, 20, 24, 28])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("replace")
    r.add_argument("--src", default=None)
    r.add_argument("--out", default=None)
    p = sub.add_parser("preview")
    p.add_argument("--out", required=True)
    p.add_argument("--src", default=None)
    p.add_argument("--k", type=int, default=3)
    a = ap.parse_args()
    import demon_fx
    import fx_swap
    if a.cmd == "replace":
        src = a.src or demon_fx.OUT_BIN
        out = a.out or demon_fx.OUT_BIN
        fx_swap.replace(src, out, EFFECTS)
    else:
        preview(a.out, a.src, a.k)


if __name__ == "__main__":
    main()
