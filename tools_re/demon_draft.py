#!/usr/bin/env python3
"""「악마화」 악마 외형 3 차 (2026-10-14 밤) — 예상도 장면 속 악마를 그대로 오려 몸통 시트 300 의 75 장을 만든다
(4 차: 초안 자세 사이의 연결 동작 = demon_rig 의 뼈대로 굽힌다 — RIG_POSES).

사용자: *"여전히 구현된 이펙트와 외형이 초안과 다르잖아 … 진짜 완전 똑같게"* → 결정 「풀컬러」. 예상도(`docs/skill/astra_demon/ingame`)는
게임 화면(176 폭)을 약 3 배로 그린 도트 그림이다 — 원본 해상도에서 격자 위상을 찾아 블록마다 대표색(중앙값)을 뽑으면 초안의 도트가 거의
그대로 게임 해상도로 돌아온다(`crisp`). 그 위에서 악마 색(파란 피부 · 남색 윤곽 · 검은 갈기 · 붉은 눈과 발톱 · 갈색 뿔)만 따라 퍼지는
영역으로 실루엣을 잡는다(`extract`). 투명/불투명 둘뿐인 게임 시트라 알파는 0 또는 255 다.

초안에 있는 자세 = 앞모습 서기 · 주먹 든 포효 · 두 팔 벌린 포효 · 웅크린 포효, 옆모습 웅크림 둘 · 할퀴기 돌진 · 달리기 · 뻗기.
초안에 없는 것(뒷모습 · 걷기 사이 프레임 · 쓰러짐)은 같은 그림에서 만든다(`derive`): 뒷모습 = 앞모습의 얼굴 · 가슴 문양을 지우고 갈기를
덮음, 걷기 = 몸 흔들림 + 다리 엇갈림, 쓰러짐 = 웅크림 → 무릎 → 눕힘. B(연속 프레임 시트)가 오면 그 자리를 바꾼다.

    python tools_re/demon_draft.py build             # costumes/demon_300/demon_<k>.png (75 장, 캔버스 CW × CH · 발바닥 FEET)
    python tools_re/demon_draft.py preview <out.png>  # 초안 장면 | 오린 악마 | 75 장 격자
"""

import argparse
import os
import sys

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import astra_cut as ac  # noqa: E402
import demon_fx as dfx  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_DIR = os.path.join(ROOT, "costumes", "demon_300")
CW, CH = 56, 50  # 캔버스 (게임 px) — 시트에는 그림이 있는 상자만 들어간다 (`demon_look.load_frames`)
FEET = (28, 46)  # 캔버스 안의 발바닥 = 사람 원점 (옛 30 × 36 의 (15, 34))
# ★★ 2026-10-15 HD (사용자: *"이펙트와 외형 전부 업스케일링"*): 같은 115 장을 예상도의 해상도(게임 3×)로 한 번 더 —
# `costumes/demon_300_hd/demon_<k>.png`(캔버스 168 × 150 · 발바닥 (84, 138) · 풀컬러). 원본 격자의 블록마다 중앙값을 뽑는 대신 블록을
# 3 × 3 으로 다시 샘플하고(`crisp`), 실루엣 · 파생 · 뼈대는 같은 규칙을 배율만큼 늘려 돈다(`set_scale`). 호스트 이펙트 라이브러리가
# 이 그림을 `demon_body` 의 쌍으로 싣고, HD 모드의 게임은 빈 시트 301 위에 이것을 합성한다. 시트 300(1×)은 그대로다.
S = 1
OUT_DIR_HD = os.path.join(ROOT, "costumes", "demon_300_hd")


def set_scale(k):
    """캔버스 배율 — 1 (시트 300) 또는 3 (HD). 원본 · 뼈대 캐시를 비운다."""
    global S, CW, CH, FEET
    S = int(k)
    CW, CH, FEET = 56 * S, 50 * S, (28 * S, 46 * S)
    _SRC.clear()
    _RIGS.clear()

_NATIVE = {}


# ── 초안 원본 → 게임 해상도 도트 ────────────────────────────────────────────────────────────────────────────
def native(i):
    if i not in _NATIVE:
        m = Image.open(os.path.join(dfx.MOCKUPS, dfx.MOCK_FILES[i // 4])).convert("RGB").crop(dfx.MOCK_BOXES[i % 4])
        _NATIVE[i] = np.asarray(m).astype(np.float32)
    return _NATIVE[i]


def crisp(i, box):
    """장면 i 의 게임 좌표 상자를 원본 격자에서 — 블록 경계 위상(±1 px, 0.5 단계)을 블록 안 분산이 가장 작은 쪽으로 고른다."""
    n = native(i)
    s = n.shape[1] / 176.0
    x0, y0, x1, y1 = box
    best = None
    for oy in np.arange(-1.0, 1.01, 0.5):
        for ox in np.arange(-1.0, 1.01, 0.5):
            xs = [int(round(x0 * s + ox + k * s)) for k in range(x1 - x0 + 1)]
            ys = [int(round(y0 * s + oy + k * s)) for k in range(y1 - y0 + 1)]
            if xs[0] < 0 or ys[0] < 0 or xs[-1] > n.shape[1] or ys[-1] > n.shape[0]:
                continue
            var = 0.0
            for a in range(len(ys) - 1):
                rows = n[ys[a]:ys[a + 1]]
                for b in range(len(xs) - 1):
                    blk = rows[:, xs[b]:xs[b + 1]]
                    var += float(((blk - blk.mean(axis=(0, 1))) ** 2).sum())
            if best is None or var < best[0]:
                best = (var, xs, ys)
    _, xs, ys = best
    if S == 1:
        out = np.zeros((len(ys) - 1, len(xs) - 1, 3), np.float32)
        for a in range(len(ys) - 1):
            for b in range(len(xs) - 1):
                out[a, b] = np.median(n[ys[a]:ys[a + 1], xs[b]:xs[b + 1]].reshape(-1, 3), axis=0)
        return out
    # ★ HD: the same blocks (the same phase), each resampled S × S (nearest at the sub-cell centres) — the draft's pixels
    out = np.zeros(((len(ys) - 1) * S, (len(xs) - 1) * S, 3), np.float32)
    for a in range(len(ys) - 1):
        bh = ys[a + 1] - ys[a]
        ri = [min(int((j + 0.5) * bh / S), bh - 1) for j in range(S)]
        for b in range(len(xs) - 1):
            bw = xs[b + 1] - xs[b]
            ci = [min(int((j + 0.5) * bw / S), bw - 1) for j in range(S)]
            blk = n[ys[a]:ys[a + 1], xs[b]:xs[b + 1]]
            out[a * S:(a + 1) * S, b * S:(b + 1) * S] = blk[np.ix_(ri, ci)]
    return out


# ── 실루엣 ─────────────────────────────────────────────────────────────────────────────────────────────────
def _grow(m, n=1):
    for _ in range(n):
        p = np.pad(m, 1)
        m = m | p[:-2, 1:-1] | p[2:, 1:-1] | p[1:-1, :-2] | p[1:-1, 2:]
    return m


def _components(mask):
    H, W = mask.shape
    seen = np.zeros_like(mask)
    out = []
    for y0, x0 in zip(*np.nonzero(mask)):
        if seen[y0, x0]:
            continue
        st = [(y0, x0)]
        seen[y0, x0] = True
        ys, xs = [], []
        while st:
            y, x = st.pop()
            ys.append(y)
            xs.append(x)
            for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                yy, xx = y + dy, x + dx
                if 0 <= yy < H and 0 <= xx < W and mask[yy, xx] and not seen[yy, xx]:
                    seen[yy, xx] = True
                    st.append((yy, xx))
        out.append((np.array(ys), np.array(xs)))
    return out


def _fill_holes(m):
    H, W = m.shape
    outside = np.zeros(m.shape, bool)
    st = [(y, x) for y in range(H) for x in (0, W - 1) if not m[y, x]] + [(y, x) for x in range(W) for y in (0, H - 1) if not m[y, x]]
    for y, x in st:
        outside[y, x] = True
    while st:
        y, x = st.pop()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            yy, xx = y + dy, x + dx
            if 0 <= yy < H and 0 <= xx < W and not m[yy, xx] and not outside[yy, xx]:
                outside[yy, xx] = True
                st.append((yy, xx))
    return m | ~outside


def classes(sub, horn=True, dark_lum=72):
    r, g, b = sub[..., 0], sub[..., 1], sub[..., 2]
    lum = sub.mean(axis=2)
    mx, mn = sub.max(axis=2), sub.min(axis=2)
    sat = (mx - mn) / (mx + 1)
    skin = (b > r + 12) & (b > g + 12) & (lum > 40)
    green = g > np.maximum(r, b) + 4
    dark = (lum < dark_lum) & ~green
    red = (r > 140) & (g < 110) & (b < 110) & (r > g + 60)
    hornc = (r >= g - 4) & (g >= b - 12) & (lum > 45) & (lum < 140) & (sat < 0.42) & ~green
    fire = (r > 220) & (g > 120) & (b < 140)
    pink = (r > 170) & (b > 130) & (g < r - 40)
    olive = (lum > 90) & (np.abs(r - g) < 25) & (b < g - 15)  # 다리 · 팔 사이로 보이는 흙 · 마른 풀 (뿔은 더 어둡다)
    allowed = (skin | dark | red | (hornc if horn else False)) & ~fire & ~pink & ~green & ~olive
    return skin, allowed


def extract(i, box, seed_min=30, drop=(), keep=(), horn=True, dark_lum=72, red_near=None, crimson=False, reach=7):
    """장면 i 의 상자에서 악마 (RGB, 불투명 마스크). drop = 뺄 상자, keep = 꼭 넣을 상자 (장면 좌표). ★ HD: 같은 규칙을 S 배로."""
    sub = crisp(i, box)
    x0, y0 = box[:2]
    sq = S * S
    rect = lambda a0, b0, a1, b1: (slice(max((b0 - y0) * S, 0), max((b1 - y0) * S, 0)), slice(max((a0 - x0) * S, 0), max((a1 - x0) * S, 0)))
    skin, allowed = classes(sub, horn, dark_lum)
    if red_near is not None:  # 붉은 픽셀은 피부 가까이만 (불 · 용암 속 발톱)
        r, g, b = sub[..., 0], sub[..., 1], sub[..., 2]
        red = (r > 140) & (g < 110) & (b < 110) & (r > g + 60)
        allowed &= ~red | _grow(skin, red_near * S)
    if crimson:  # 발톱 · 눈은 진홍(파랑이 초록만큼), 불은 주황(초록이 파랑보다 많다)
        r, g, b = sub[..., 0], sub[..., 1], sub[..., 2]
        allowed &= ~((r > 140) & (g > b + 18) & (sub.mean(axis=2) > 110))
    for bx in drop:
        allowed[rect(*bx)] = False
        skin[rect(*bx)] = False
    seeds = np.zeros(skin.shape, bool)
    for c in _components(skin):
        if len(c[0]) >= seed_min * sq:
            seeds[c] = True
    region = seeds.copy()
    while True:
        nxt = _grow(region) & (allowed | seeds)
        if (nxt == region).all():
            break
        region = nxt
    for bx in keep:
        region[rect(*bx)] |= allowed[rect(*bx)]
    filled = _fill_holes(region) & ~region
    r, g, b = sub[..., 0], sub[..., 1], sub[..., 2]
    lum = sub.mean(axis=2)
    floor = ((g > r + 4) & (g > b + 4)) | ((r > 170) & (b > 130)) | ((lum > 90) & (np.abs(r - g) < 25) & (b < g - 15))  # 다리 사이 바닥
    region |= filled & ~floor
    region &= _grow(skin, reach * S)  # 피부에서 reach px 밖은 배경 (갈기 · 발톱은 그 안에 든다)
    for _ in range(3 * S):  # 가시 떼기 — 피부가 아닌 픽셀 중 이웃이 하나뿐인 것
        p = np.pad(region, 1)
        nb = p[:-2, 1:-1].astype(int) + p[2:, 1:-1] + p[1:-1, :-2] + p[1:-1, 2:]
        region &= skin | (nb >= 2)
    for bx in drop:
        region[rect(*bx)] = False
    comps = _components(region)
    main = max(comps, key=lambda c: len(c[0])) if comps else None
    h, w = region.shape
    for c in comps:  # 떨어진 부스러기
        if len(c[0]) < 6 * sq:
            region[c] = False
        elif c is not main and (c[0].min() == 0 or c[1].min() == 0 or c[0].max() == h - 1 or c[1].max() == w - 1):
            # ★ HD (2026-10-15): 몸과 떨어진 조각이 상자 가장자리에 닿으면 상자가 잘라 낸 배경이다 (악마는 상자 안에 통째로 든다) —
            # side_reach 의 3× 추출이 윗변에 매달린 487 px 조각을 들여왔다 (1× 는 중앙값이 지웠다).
            region[c] = False
    return sub, region


# ── 초안 속 악마 (장면, 상자 = 발바닥 상대, 손질) ─────────────────────────────────────────────────────────────
SOURCES = {
    "front_stand": dict(i=5, box=(-22, -42, 22, 4), horn=False, drop=[(-13, -38, -9, -21), (-17, -6, -9, 4), (16, -6, 26, 4)]),
    "front_fist": dict(i=8, box=(-22, -42, 22, 4)),
    "front_roar": dict(i=9, box=(-24, -46, 24, 2), red_near=1, crimson=True),
    "side_crouch": dict(i=2, box=(-26, -36, 26, 4)),
    "side_crouch2": dict(i=3, box=(-26, -36, 19, 4)),
    "side_lunge": dict(i=1, box=(-24, -32, 30, 8), red_near=1, horn=False, drop=[(16, -5, 30, 8), (-5, -2, 3, 6)]),
    "side_reach": dict(i=7, box=(-28, -38, 30, 4), red_near=1),
    "side_lava": dict(i=10, box=(-28, -36, 28, 4), red_near=1, dark_lum=60),
}


def source(name):
    """(RGBA uint8 캔버스 CW × CH, 발바닥 = FEET)."""
    spec = SOURCES[name]
    i = spec["i"]
    fx, fy = ac.FEET[i]
    bx0, by0, bx1, by1 = spec["box"]
    box = (fx + bx0, fy + by0, fx + bx1, fy + by1)
    kw = {k: v for k, v in spec.items() if k in ("seed_min", "horn", "dark_lum", "red_near", "crimson", "reach")}
    drop = [(fx + a, fy + b, fx + c, fy + d) for (a, b, c, d) in spec.get("drop", ())]
    keep = [(fx + a, fy + b, fx + c, fy + d) for (a, b, c, d) in spec.get("keep", ())]
    sub, region = extract(i, box, drop=drop, keep=keep, **kw)
    out = np.zeros((CH, CW, 4), np.uint8)
    h, w = region.shape
    ox, oy = FEET[0] + bx0 * S, FEET[1] + by0 * S
    rgba = np.dstack([np.clip(sub, 0, 255), region * 255]).astype(np.uint8)
    ys, xs = np.nonzero(region)
    for y, x in zip(ys, xs):
        X, Y = ox + x, oy + y
        if 0 <= X < CW and 0 <= Y < CH:
            out[Y, X] = rgba[y, x]
    dx, dy = stance(out)
    if not name.startswith("front_"):  # 옆모습: 좌우 자리는 장면의 발 x (이펙트 기준점과 같은 값), 높이만 발바닥 줄로
        dx = 0
    return shift_canvas(out, -dx, -dy)


def stance(a):
    """발 자리의 어긋남 (dx, dy) — 가장 낮은 세 줄의 좌우 가운데가 FEET x, 가장 낮은 줄이 FEET y + 1 (옛 30 × 36 의 규약; ★ HD: × S)."""
    s = a.shape[1] // 56
    op = a[..., 3] > 0
    ys = np.nonzero(op.any(axis=1))[0]
    bot = ys.max()
    lx = np.nonzero(op[bot - 3 * s + 1:bot + 1].any(axis=0))[0]
    return int(round((lx.min() + lx.max()) / 2 - 28 * s)), int(bot - (46 * s + s))


def shift_canvas(a, dx, dy):
    out = np.zeros_like(a)
    ys, xs = np.nonzero(a[..., 3])
    Y, X = ys + dy, xs + dx
    ok = (Y >= 0) & (Y < CH) & (X >= 0) & (X < CW)
    out[Y[ok], X[ok]] = a[ys[ok], xs[ok]]
    return out


def measured_feet():
    """초안 장면마다 악마의 실제 발 자리 (장면 좌표) — `astra_cut.FEET` 를 이 값으로 맞춘다 (이펙트 기준점 = 악마 발)."""
    out = {}
    for name, spec in SOURCES.items():
        i = spec["i"]
        fx, fy = ac.FEET[i]
        bx0, by0, bx1, by1 = spec["box"]
        kw = {k: v for k, v in spec.items() if k in ("seed_min", "horn", "dark_lum", "red_near", "crimson", "reach")}
        drop = [(fx + a, fy + b, fx + c, fy + d) for (a, b, c, d) in spec.get("drop", ())]
        sub, region = extract(i, (fx + bx0, fy + by0, fx + bx1, fy + by1), drop=drop, **kw)
        ys = np.nonzero(region.any(axis=1))[0]
        bot = ys.max()
        lx = np.nonzero(region[bot - 2:bot + 1].any(axis=0))[0]
        x = fx + int(round(bx0 + (lx.min() + lx.max()) / 2)) if name.startswith("front_") else fx
        out[name] = (i, (int(x), int(fy + by0 + bot - 1)))
    return out


# ── 75 장 ─────────────────────────────────────────────────────────────────────────────────────────────────
BACK, FRONT, LEFT, RIGHT = 0, 1, 2, 3
MANE = [(24, 18, 24), (38, 30, 36), (56, 44, 50), (78, 62, 66)]  # 뒷모습 갈기 (옆모습 초안의 등 갈기 색)
_SRC = {}


def src(name):
    if name not in _SRC:
        _SRC[name] = source(name)
    return _SRC[name]


def _bbox(a):
    ys, xs = np.nonzero(a[..., 3])
    return xs.min(), ys.min(), xs.max() + 1, ys.max() + 1


def flip(a):
    """발바닥(FEET x) 둘레로 좌우 거울."""
    out = np.zeros_like(a)
    for x in range(CW):
        X = 2 * FEET[0] - x - 1
        if 0 <= X < CW:
            out[:, X] = a[:, x]
    return out


def shift(a, dx, dy, sel=None):
    out = np.zeros_like(a)
    src_ = a if sel is None else np.where(sel[..., None], a, 0)
    ys, xs = np.nonzero(src_[..., 3])
    Y, X = ys + dy, xs + dx
    ok = (Y >= 0) & (Y < CH) & (X >= 0) & (X < CW)
    out[Y[ok], X[ok]] = src_[ys[ok], xs[ok]]
    return out


def over(*layers):
    out = np.zeros_like(layers[0])
    for l in layers:
        m = l[..., 3] > 0
        out[m] = l[m]
    return out


def _regions(a, hip_frac=0.66):
    x0, y0, x1, y1 = _bbox(a)
    hip = int(round(y0 + (y1 - y0) * hip_frac))
    yy, xx = np.mgrid[0:CH, 0:CW]
    return x0, y0, x1, y1, hip, yy, xx


def walk(a, k, side=False):
    """걷기 k (0..3): 0 · 2 = 그대로, 1 · 3 = 몸이 1 px 내려앉고 한쪽 다리가 든다 (옆모습은 앞다리 · 뒷다리가 엇갈린다)."""
    if k % 2 == 0:
        return a.copy()
    x0, y0, x1, y1, hip, yy, xx = _regions(a)
    cx = FEET[0]
    upper = yy < hip
    legs = ~upper
    a_side = legs & (xx < cx)
    b_side = legs & (xx >= cx)
    if side:
        fwd, back_ = (b_side, a_side)
        lifted = shift(a, S if k == 1 else -S, 0, fwd)
        other = shift(a, -S if k == 1 else S, 0, back_)
        return over(other, lifted, shift(a, 0, S, upper))
    up_leg, down_leg = (a_side, b_side) if k == 1 else (b_side, a_side)
    return over(shift(a, 0, 0, down_leg), shift(a, 0, -S, up_leg), shift(a, 0, S, upper))


def crouch(a, k=0.3, drop=0):
    """다리 줄을 k 만큼 줄여 몸을 내린다 (구부린 다리 · 무릎 꿇기). drop = 게임 px."""
    drop *= S
    x0, y0, x1, y1, hip, yy, xx = _regions(a)
    out = np.zeros_like(a)
    n = y1 - hip
    keep = int(round(n * (1 - k)))
    rows = [hip + int(round(j * n / max(keep, 1))) for j in range(keep)]
    base = y1 - keep
    for j, r in enumerate(rows):
        out[base + j] = a[min(r, CH - 1)]
    lift = base - hip
    up = shift(a, 0, lift + drop, yy < hip)
    return over(shift(out, 0, drop), up)


def lie(a):
    """누운 모습 — 앞모습을 시계 방향 90° 돌려 발바닥 줄에 눕힌다 (머리 = 오른쪽)."""
    x0, y0, x1, y1 = _bbox(a)
    sub = a[y0:y1, x0:x1]
    r = np.rot90(sub, -1)
    h, w = r.shape[:2]
    out = np.zeros_like(a)
    X0 = FEET[0] - w // 2
    Y0 = FEET[1] + 1 - h
    out[max(Y0, 0):Y0 + h, max(X0, 0):X0 + w] = r[max(-Y0, 0):, max(-X0, 0):][:CH - max(Y0, 0), :CW - max(X0, 0)]
    return out


def back_of(a):
    """뒷모습 — 얼굴(붉은 눈 · 입) · 가슴 문양을 피부로 지우고, 뒤통수에서 허리까지 검은 갈기(보드 01 · 04 의 뒷모습: 머리 뒤와 등을
    덮는 덥수룩한 갈기)를 덮고, 좌우를 뒤집는다. 윤곽 픽셀은 그대로 둔다."""
    out = a.copy()
    x0, y0, x1, y1 = _bbox(a)
    H = y1 - y0
    rgb = out[..., :3].astype(int)
    op = out[..., 3] > 0
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    lum = rgb.mean(axis=2)
    skin = op & (b > r + 12) & (b > g + 12)
    skin_mid = np.median(rgb[skin], axis=0).astype(np.uint8) if skin.any() else np.array([70, 80, 160], np.uint8)
    yy, xx = np.mgrid[0:CH, 0:CW]
    head = yy < y0 + H * 0.36
    face = op & head & (((r > 150) & (r > b + 40)) | (lum > 160))
    out[face, :3] = skin_mid
    emblem = op & (lum > 150) & (b < r + 40)
    out[emblem & ~head, :3] = skin_mid
    p = np.pad(op, 1)
    inner = op & p[:-2, 1:-1] & p[2:, 1:-1] & p[1:-1, :-2] & p[1:-1, 2:]
    # 몸통 폭 = 어깨 줄의 폭
    widths = op.sum(axis=1)
    cx = FEET[0] + 0.5
    top = int(y0 + H * 0.12)
    bot = int(y0 + H * 0.62)
    shoulder = max(widths[int(y0 + H * 0.3):int(y0 + H * 0.45)].max(), 1) if H > 10 else widths.max()
    rnd = np.random.RandomState(11)
    jag = np.repeat(rnd.randint(-2, 2, CW // S), S) * S  # ★ HD: 게임 px 단위의 들쭉날쭉
    for y in range(top, bot + 3 * S):
        t = (y - top) / max(bot - top, 1)
        half = shoulder * (0.36 if t < 0.35 else 0.36 - (t - 0.35) * 0.42)
        for x in range(x0, x1):
            if not inner[y, x] or abs(x + 0.5 - cx) > half or y > bot + jag[x]:
                continue
            k = 1
            gx, gy = x // S, y // S
            if (gx * 3 + gy // 2) % 5 == 0:
                k = 2  # 머릿결
            if t < 0.15 and (gx + gy) % 3 == 0:
                k = 3
            if abs(x + 0.5 - cx) > half - 1.2 * S or y >= bot + jag[x] - S:
                k = 0  # 갈기 가장자리
            out[y, x, :3] = MANE[k]
    return flip(out)


# ★ 2026-10-14 밤 (4): 사이 동작 — 사용자: *"초안을 토대로 연결동작들을 유추해서 … 기본공격시 몸을 숙이는듯한 동작 … 전부 자세하게"*.
# 예상도에는 할퀴기 세 단의 「친 순간」 한 장씩뿐이라, 그 한 장(+ 앞모습 초안)을 뼈대(`demon_rig.py`)로 굽혀 단마다 네 자세를 만든다.
# 연산: ("crouch", dy) 엉덩이 · 윗몸을 내림(발 그대로 · 무릎 IK, 음수 = 다리를 편다) · ("lean", 도) 윗몸을 엉덩이 둘레로(+ = 앞으로 숙임,
# 오른쪽을 보는 옆모습 기준) · ("reach", 팔, dx, dy, 굽는 쪽) 손을 어깨 상대 자리로(팔꿈치 IK) · ("step", 다리, dx, 들기) 옆모습 발 ·
# ("lift", 다리, 들기) 앞모습 발(정강이를 줄인다) · ("bob", dy) 엉덩이 위 전부를 dy. 팔 n = 옆모습의 앞쪽 팔 / 앞모습의 화면 오른쪽 팔.
_UP = ["hip", "hp_n", "hp_f"]
RIG_POSES = {
    # 서기 · 걷기 — 옆 = 예상도 02 의 낮은 자세(side_crouch), 앞 = 예상도 05 의 선 자세
    "stand": (("front_stand", ()), ("side_crouch", ())),
    "walk0": (("front_stand", ()), ("side_crouch", (("step", "n", 3, 0), ("step", "f", -3, 0), ("reach", "n", 2, 9, -1)))),
    "walk1": (("front_stand", (("bob", 1), ("lift", "n", 3))), ("side_crouch", (("bob", 1), ("step", "n", 0, 2)))),
    "walk2": (("front_stand", ()), ("side_crouch", (("step", "n", -3, 0), ("step", "f", 3, 0), ("reach", "n", 6, 8, -1)))),
    "walk3": (("front_stand", (("bob", 1), ("lift", "f", 3))), ("side_crouch", (("bob", 1), ("step", "f", 0, 2)))),
    # 할퀴기 I (0 단, 타격 f1 · f3) — 친 순간 = 예상도 02(side_lunge) · 앞은 오른손을 든 예상도 08(front_fist)
    "c0_wind": (("front_fist", (("crouch", 1), ("reach", "n", 6, -10, -1))),
                ("side_lunge", (("crouch", 2), ("lean", -5), ("reach", "n", -4, 5, -1)))),
    "c0_hitA": (("front_fist", (("crouch", 2), ("reach", "n", -8, 8, 1))), ("side_lunge", ())),
    "c0_windB": (("front_fist", (("crouch", 1), ("reach", "n", 3, 8, -1), ("reach", "f", -5, -10, 1))),
                 ("side_lunge", (("crouch", 1), ("lean", 3), ("reach", "n", 4, 6, -1), ("reach", "f", -6, -5, -1)))),
    "c0_hitB": (("front_fist", (("crouch", 2), ("reach", "n", 3, 8, -1), ("reach", "f", 9, 8, -1))),
                ("side_lunge", (("crouch", 1), ("lean", 8), ("reach", "n", 9, -6, 1)))),
    # 할퀴기 II (1 단, 타격 f2) — 친 순간 = 예상도 03(side_crouch: 네 발로 낮게 · 앞발 휘두름) · 앞은 팔 벌린 예상도 09(front_roar)
    "c1_coil": (("front_roar", (("crouch", 3), ("reach", "n", 3, 4, 1), ("reach", "f", -3, 4, -1))),
                ("side_crouch", (("crouch", 2), ("lean", -8), ("reach", "n", 1, 5, -1)))),
    "c1_dash": (("front_roar", (("reach", "n", 7, -6, -1), ("reach", "f", -7, -6, 1))),
                ("side_crouch", (("lean", 4), ("reach", "n", 8, 1, 1)))),
    "c1_hit": (("front_roar", (("crouch", 2),)), ("side_crouch", ())),
    "c1_follow": (("front_roar", (("crouch", 3), ("reach", "n", -4, 8, 1), ("reach", "f", 4, 8, -1))),
                  ("side_crouch", (("crouch", 1), ("lean", 6), ("reach", "n", 2, 10, -1)))),
    # 할퀴기 III (2 단, 타격 f2) — 예상도 04(side_crouch2: 뒤쪽 팔을 등 뒤로 든 웅크림)에서 도약 → 내려찍기
    "c2_coil": (("front_roar", (("crouch", 3), ("reach", "n", 3, -9, -1), ("reach", "f", -3, -9, 1))),
                ("side_crouch2", (("crouch", 2), ("lean", -4)))),
    "c2_leap": (("front_roar", (("crouch", -1), ("reach", "n", 2, -11, -1), ("reach", "f", -2, -11, 1))),
                ("side_crouch2", (("crouch", -1), ("lean", -8), ("reach", "f", -7, -10, -1), ("reach", "n", 2, 4, -1)))),
    "c2_hit": (("front_roar", (("crouch", 3), ("reach", "n", -2, 10, 1), ("reach", "f", 2, 10, -1))),
               ("side_crouch2", (("crouch", 1), ("lean", 6), ("reach", "f", 10, 10, 1)), {"arm_f_up": 6, "arm_f_lo": 6})),
    "c2_impact": (("front_roar", (("crouch", 4), ("reach", "n", 0, 11, 1), ("reach", "f", 0, 11, -1))),
                  ("side_crouch2", (("crouch", 2), ("lean", 10), ("reach", "f", 11, 13, 1), ("reach", "n", 2, 7, -1)),
                   {"arm_f_up": 6, "arm_f_lo": 6})),
    # 버프 · 시전 = 포효 (옆은 예상도 09 의 주먹 든 3/4 모습)
    "roar": (("front_roar", ()), ("front_fist", ())),
    # ★★ 2026-10-14 (5): 서기 숨쉬기 · 스킬 시전 동작 · 돌진 달리기 (`demon_look.EXTRA_POSES` — 시트 프레임 84.. · DRAW 조각이
    #   서기 · 걷기 대신 그 스킬의 시퀀스를 그 틱부터 돌린다, `DemonPatch::POSE_*`). 정점 = 예상도: 지옥칼날 08(side_reach — 앞팔을
    #   곧게 뻗음) · 어둠심판 09(front_fist — 주먹을 듦) · 멸화 10(front_roar — 두 팔 벌린 포효, 옛 「roar」) · 용암불 11(side_lava —
    #   두 손을 땅에) · 돌진 05(side_crouch — 네 발로 낮게). 그 사이(준비 · 들어 올림 · 모음 · 디딤)는 같은 그림을 뼈대로 굽힌다.
    "idle": (("front_stand", (("bob", 1),)), ("side_crouch", (("bob", 1),))),
    "run0": (("front_roar", (("crouch", 3), ("reach", "n", 3, 10, 1), ("reach", "f", -6, 2, -1), ("lift", "f", 2))),
             ("side_crouch", (("lean", 5), ("step", "n", 4, 1), ("step", "f", -5, 0), ("reach", "n", 10, 6, 1), ("reach", "f", 1, 2, 1)))),
    "run1": (("front_roar", (("crouch", 2), ("bob", -1), ("reach", "n", 6, 2, 1), ("reach", "f", -3, 10, -1), ("lift", "n", 2))),
             ("side_crouch", (("bob", -2), ("step", "n", -1, 3), ("step", "f", 2, 2), ("reach", "n", 4, 9, -1), ("reach", "f", 4, 6, 1)))),
    "blade_wind": (("front_fist", (("crouch", 2), ("reach", "n", 2, 2, 1), ("reach", "f", -5, 5, -1))),
                   ("side_reach", (("crouch", 2), ("lean", -3), ("reach", "n", -1, 8, -1)))),
    "blade_thrust": (("front_fist", (("crouch", 1), ("reach", "n", 12, 4, -1))), ("side_reach", ())),
    "judge_raise": (("front_fist", (("crouch", 1), ("reach", "n", 7, -1, -1))),
                    ("side_reach", (("crouch", 1), ("lean", -2), ("reach", "n", 7, -5, -1)))),
    "judge_up": (("front_fist", ()), ("side_reach", (("lean", -8), ("reach", "n", 2, -13, -1)))),
    "fire_gather": (("front_roar", (("crouch", 3), ("reach", "n", -5, 6, 1), ("reach", "f", 5, 6, -1))),
                    ("front_fist", (("crouch", 3), ("reach", "n", -4, 7, 1), ("reach", "f", 6, 3, -1)))),
    "lava_rise": (("front_roar", (("crouch", -1), ("reach", "n", 3, -12, -1), ("reach", "f", -3, -12, 1))),
                  ("side_crouch2", (("crouch", -1), ("lean", -6), ("reach", "f", -4, -11, -1), ("reach", "n", 3, -10, -1)))),
    "lava_slam": (("front_roar", (("crouch", 4), ("reach", "n", 3, 14, 1), ("reach", "f", -3, 14, -1))), ("side_lava", ())),
}
_RIGS = {}


def rigged(name, ops=(), z=None):
    """원본 자세 name 을 연산 ops 로 굽힌 그림 (RGBA CW × CH)."""
    import demon_rig as R
    if name not in _RIGS:
        _RIGS[name] = R.Rig(name, src(name))
    rig = _RIGS[name]
    J = rig.joints()
    for op in ops:  # px 인수는 게임 px — ★ HD: 배율로
        k = op[0]
        if k == "crouch":
            R.crouch(rig, J, op[1] * S)
        elif k == "lean":
            R.lean(J, op[1])
        elif k == "reach":
            R.reach(rig, J, op[1], (op[2] * S, op[3] * S), op[4])
        elif k == "step":
            R.step(rig, J, op[1], op[2] * S, op[3] * S)
        elif k == "lift":
            R.lift_front(J, op[1], op[2] * S)
        elif k == "bob":
            R.move(J, R.UPPER + _UP, 0, op[1] * S)
        else:
            raise ValueError(f"rig op {op!r}")
    if not ops:
        return src(name).copy()
    return R.Rig.render(rig, J, z=z)


def _pose_art(spec):
    name, ops = spec[0], spec[1]
    return rigged(name, ops, spec[2] if len(spec) > 2 else None)


def pose_table():
    """{(자세, 방향): 그림} — 앞(FRONT) · 오른쪽(RIGHT) 은 `RIG_POSES`(초안 + 뼈대), 왼쪽 = 오른쪽의 거울, 뒤 = 앞에서."""
    import demon_look as dl
    T = {}
    for pose in dl.POSES + dl.EXTRA_POSES:
        front, side = RIG_POSES[pose]
        F = _pose_art(front)
        R_ = _pose_art(side)
        T[(pose, FRONT)] = F
        T[(pose, RIGHT)] = R_
        T[(pose, LEFT)] = flip(R_)
        T[(pose, BACK)] = back_of(F)
    T[("fall0", FRONT)] = crouch(src("front_stand"), 0.3, drop=1)
    T[("fall1", FRONT)] = crouch(src("front_stand"), 0.6, drop=2)
    T[("fall2", FRONT)] = lie(src("front_stand"))
    return T


def frames():
    import demon_look as dl
    T = pose_table()
    out = []
    for pose in dl.POSES:
        for view in (BACK, FRONT, LEFT, RIGHT):
            out.append(T[(pose, view)])
    for pose in dl.FALLS:
        out.append(T[(pose, FRONT)])
    for pose in dl.EXTRA_POSES:  # ★ (5): 숨쉬기 · 시전 · 달리기 — 쓰러짐 뒤
        for view in (BACK, FRONT, LEFT, RIGHT):
            out.append(T[(pose, view)])
    assert len(out) == dl.FRAMES
    return out


def grid(out_png, z=4):
    """51 장 격자 (자세 줄 × 방향 칸: 뒤 · 앞 · 왼 · 오른) + 쓰러짐 줄 — 발바닥 십자."""
    import demon_look as dl
    fr = frames()
    rows = len(dl.POSES) + 1 + len(dl.EXTRA_POSES)
    img = Image.new("RGB", (4 * (CW + 2) * z + 120, rows * (CH + 2) * z), (40, 44, 40))
    d = ImageDraw.Draw(img)
    n_main, n_fall = len(dl.POSES) * 4, len(dl.FALLS)
    for k, a in enumerate(fr):
        if k < n_main:
            r, c = k // 4, k % 4
        elif k < n_main + n_fall:
            r, c = len(dl.POSES), k - n_main
        else:  # ★ (5): 숨쉬기 · 시전 · 달리기 — 쓰러짐 줄 아래
            e = k - n_main - n_fall
            r, c = len(dl.POSES) + 1 + e // 4, e % 4
        bg = np.zeros((CH, CW, 3), np.uint8)
        bg[...] = (70, 104, 60)
        bg[FEET[1], :] = (90, 130, 80)
        al = a[..., 3:4] / 255.0
        cell = (a[..., :3] * al + bg * (1 - al)).astype(np.uint8)
        img.paste(Image.fromarray(cell).resize((CW * z, CH * z), Image.NEAREST), (c * (CW + 2) * z, r * (CH + 2) * z))
    for i, p in enumerate(dl.POSES + ["fall"] + dl.EXTRA_POSES):
        d.text((4 * (CW + 2) * z + 6, i * (CH + 2) * z + 4), p, fill=(255, 255, 0))
    img.save(out_png)
    print(out_png, img.size)


PALETTE = 255  # 8 비트 시트 (투명 0 번 + 255 색) — 「풀컬러」(사용자 2026-10-14) · 시트 용량은 `demon_look.members` 가 잰다


def quantize(fr, n=PALETTE):
    """51 장의 불투명 픽셀을 한 팔레트 n 색으로 (중앙값 자르기, 디더 없음) — 블록 중앙값이 만든 수천 색을 시트가 담는 수로."""
    px = np.concatenate([a[a[..., 3] > 0][:, :3] for a in fr])
    strip = Image.fromarray(px.reshape(1, -1, 3).astype(np.uint8), "RGB")
    q = strip.quantize(colors=n, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).convert("RGB")
    qpx = np.asarray(q).reshape(-1, 3)
    out, at = [], 0
    for a in fr:
        b = a.copy()
        m = b[..., 3] > 0
        k = int(m.sum())
        b[m, :3] = qpx[at:at + k]
        at += k
        out.append(b)
    return out


def with_imported(fr):
    """B: 들인 악마 시트(`astra_frames.py import … demon --demon`)의 칸이 있으면 그 프레임으로 바꾼다 (빈 칸은 A 그대로)."""
    import astra_frames
    out, used = [], []
    for k, a in enumerate(fr):
        imp = astra_frames.frame("demon", k)
        if imp is not None and imp[0].shape == a.shape:
            out.append(shift_canvas(imp[0], *[-v for v in stance(imp[0])]))
            used.append(k)
        else:
            out.append(a)
    if used:
        print(f"B: 들인 프레임 {len(used)} 장 — {used}")
    return out


def build(hd=True):
    set_scale(1)
    os.makedirs(OUT_DIR, exist_ok=True)
    fr = quantize(with_imported(frames()))
    for k, a in enumerate(fr):
        Image.fromarray(a, "RGBA").save(os.path.join(OUT_DIR, f"demon_{k}.png"))
    cols = set()
    for a in fr:
        for c in a[a[..., 3] > 0][:, :3]:
            cols.add(tuple(int(v) for v in c))
    print(f"{OUT_DIR}: {len(fr)} 장 · 캔버스 {CW}×{CH} · 발바닥 {FEET} · 색 {len(cols)}")
    if hd:  # ★ HD: the same frames at 3× — full colour (the effect library quantizes each twin on its own)
        set_scale(3)
        os.makedirs(OUT_DIR_HD, exist_ok=True)
        frh = frames()
        for k, a in enumerate(frh):
            Image.fromarray(a, "RGBA").save(os.path.join(OUT_DIR_HD, f"demon_{k}.png"))
        print(f"{OUT_DIR_HD}: {len(frh)} 장 · 캔버스 {CW}×{CH} · 발바닥 {FEET} (HD ×3)")
        with open(os.path.join(OUT_DIR_HD, "README.txt"), "w", encoding="utf-8", newline="\n") as f:
            f.write("악마 도트 HD — costumes/demon_300 의 115 장을 예상도 해상도(게임 3×)로: 캔버스 168 × 150 · 발바닥 (84, 138) · 풀컬러.\n"
                    "만든 것: python tools_re/demon_draft.py build. 호스트 이펙트 라이브러리(demon_fx.py)가 demon_body 효과의 HD 쌍으로 싣고,\n"
                    "HD 모드의 게임은 빈 시트 301 위에 이것을 창 해상도로 합성한다. 프레임 번호 = demon_300 과 같다.\n")
        set_scale(1)
    import demon_look as dl
    with open(os.path.join(OUT_DIR, "README.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("악마 도트 — 「악의」의 외형 (몸통 시트 hero/1/300 의 프레임 9 + k). 만든 것: python tools_re/demon_draft.py build\n"
                "예상도(docs/skill/astra_demon/ingame)의 악마를 픽셀째 오린 자세 8 장(demon_draft.SOURCES)이 정점이고, 사이 동작은 그 자세를\n"
                "뼈대로 굽혀 만든다(tools_re/demon_rig.py · demon_draft.RIG_POSES). 캔버스 56 × 50 · 발바닥 (28, 46) · 255 색.\n"
                "할퀴기 단마다 네 자세 = 그 단의 게임 프레임 0..3 (타격 = 0 단 f1 · f3 · 1 단 f2 · 2 단 f2 — DemonPatch::HIT_FRAMES).\n\n"
                f"프레임 번호 k (demon_<k>.png) = 자세 × 4 + 방향(0 뒤 · 1 앞 · 2 왼 · 3 오른), 쓰러짐 {len(dl.POSES) * 4}..{dl.FRAMES - 1} (앞만):\n")
        for i, pose in enumerate(dl.POSES):
            f.write(f"  {4 * i:2d}..{4 * i + 3:2d} {pose}\n")
        for i, pose in enumerate(dl.FALLS):
            f.write(f"  {len(dl.POSES) * 4 + i:2d}     {pose}\n")
        base = len(dl.POSES) * 4 + len(dl.FALLS)
        for i, pose in enumerate(dl.EXTRA_POSES):
            f.write(f"  {base + 4 * i:3d}..{base + 4 * i + 3:3d} {pose}  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)\n")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("sources")
    p.add_argument("out")
    p = sub.add_parser("build")
    p.add_argument("--no-hd", action="store_true", help="1× 만")
    p = sub.add_parser("grid")
    p.add_argument("out")
    p.add_argument("--zoom", type=int, default=4)
    a = ap.parse_args()
    if a.cmd == "build":
        build(hd=not a.no_hd)
    elif a.cmd == "grid":
        grid(a.out, a.zoom)
    elif a.cmd == "sources":
        names = list(SOURCES)
        z = 6
        img = Image.new("RGB", (len(names) // 2 * (CW * 2 + 4) * z + 40, 2 * (CH * z + 24)), (24, 24, 30))
        d = ImageDraw.Draw(img)
        for k, name in enumerate(names):
            spec = SOURCES[name]
            fx, fy = ac.FEET[spec["i"]]
            scene = np.asarray(dfx.mockup_panel(spec["i"]))[fy + FEET[1] - CH + 4 - 4:fy + 4, :][:, max(fx - FEET[0], 0):fx - FEET[0] + CW]
            canvas = np.zeros((CH, CW, 3), np.uint8)
            canvas[...] = (200, 0, 200)
            hh, ww = scene.shape[:2]
            canvas[CH - hh:, :ww] = scene
            s = source(name)
            chk = np.zeros((CH, CW, 3), np.uint8)
            chk[...] = (60, 60, 70)
            chk[::2, ::2] = (80, 80, 90)
            al = s[..., 3:4] / 255.0
            cut = (s[..., :3] * al + chk * (1 - al)).astype(np.uint8)
            pair = np.concatenate([canvas, np.full((CH, 2, 3), 20, np.uint8), cut], axis=1)
            x, y = (k % (len(names) // 2)) * (CW * 2 + 4) * z + 10, (k // (len(names) // 2)) * (CH * z + 24) + 4
            img.paste(Image.fromarray(pair).resize((pair.shape[1] * z, CH * z), Image.NEAREST), (x, y + 16))
            d.text((x, y), name, fill=(255, 255, 0))
        img.save(a.out)
        print(a.out)


if __name__ == "__main__":
    main()
