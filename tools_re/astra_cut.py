#!/usr/bin/env python3
"""ASTRA 예상도에서 이펙트 픽셀을 그대로 오려 낸다 (2026-10-14 밤 3차 — 「초안과 완전 똑같게」).

사용자: *"여전히 구현된 이펙트와 외형이 초안과 다르잖아 … 진짜 완전 똑같게 구현하고싶은거야"* → 결정 「A 먼저, B 오면 교체」:
초안의 픽셀을 오려 정점 프레임으로 쓰고(이 파일), 그 사이 프레임은 같은 그림을 움직여 만든다(`demon_fx_art.py`). B = 초안을 만든
이미지 생성 도구로 받은 연속 프레임 시트가 오면 그 프레임으로 바꾼다(`docs/skill/astra_demon/astra_frames/README.md`).

오리기 = 예상도 장면(`docs/skill/astra_demon/ingame`, 게임 폭 176 으로 줄인 것)의 한 상자에서
1. 색 규칙으로 이펙트 픽셀을 고른다 (붉은 · 보라 · 남색 …; 검은 조각은 이펙트 가까이 있는 것만),
2. 그 밖의 픽셀에서 확산으로 배경을 메워(인페인트) 이펙트 밑의 풀 · 땅을 추정하고,
3. 「배경 위에 얹힌 반투명 층」의 최소 알파를 푼다 (밝아진 만큼 · 어두워진 만큼 → 알파, 그 알파로 색을 되돌린다),
4. 초안 속 캐릭터(악마 · 몬스터)와 겹친 자리는 빼고(exclude) 필요하면 둘레에서 메운다(fill).
결과 = `costumes/astra_fx/<이름>.png`(RGBA, 게임 해상도) + `costumes/astra_fx/cuts.json`(기준점 = 초안 속 악마 · 대상의 발바닥).

    python tools_re/astra_cut.py build            # 전부 다시 오린다
    python tools_re/astra_cut.py preview <out.png> # 초안 | 오린 것을 실제 게임 배경에 얹은 것
"""

import argparse
import json
import math
import os
import random
import sys

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import demon_fx as dfx  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_DIR = os.path.join(ROOT, "costumes", "astra_fx")
# ★★ 2026-10-15 HD (사용자: *"이펙트와 외형 전부 업스케일링"*): 같은 오리기를 예상도의 해상도(게임 3×)에서 한 번 더 — `costumes/astra_fx_hd/`.
# 예상도는 게임 화면을 3 배로 그린 도트라 원본을 176 × 3 = 528 폭으로 맞추면(0.4 % 늘림) 1 px = 게임 ⅓ px 가 된다. 규칙 · 상자 ·
# 기준점은 1× 의 것 × HD, 배경 추정(인페인트)은 1× 의 결과를 3 배로 늘려 쓴다(확산은 반지름이 반복 수의 제곱근이라 3× 에서 9 배가 든다).
HD = 3
OUT_DIR_HD = os.path.join(ROOT, "costumes", "astra_fx_hd")

# 초안 장면마다 악마의 발바닥 (176 폭 좌표) — 1·2·3·5·7·8·9·10 은 `demon_draft.measured_feet()` 가 오린 악마에서 잰 값
# (앞모습 = 발 줄의 가운데, 옆모습 = 눈대중 x · 실제 발바닥 y), 나머지는 눈으로 잰 값. 악마 그림과 이펙트가 같은 기준점을 쓴다.
FEET = {0: (88, 126), 1: (57, 117), 2: (70, 125), 3: (58, 125), 4: (124, 108), 5: (84, 123), 6: (90, 123), 7: (80, 120), 8: (66, 125), 9: (86, 132), 10: (82, 126), 11: (77, 123)}
# 초안 속 몬스터 (대략의 상자) — 장면마다
MOBS = {
    0: [(14, 78, 34, 104), (124, 26, 144, 58), (144, 82, 166, 108)],
    1: [(14, 80, 34, 104), (110, 24, 146, 68), (142, 76, 164, 106)],
    2: [(14, 80, 34, 104), (110, 24, 146, 68), (142, 76, 164, 106)],
    3: [(14, 80, 34, 104), (110, 24, 146, 68), (142, 76, 164, 106)],
    4: [(14, 74, 34, 102), (124, 22, 146, 56), (148, 78, 170, 106)],
    5: [(14, 80, 34, 104), (124, 26, 146, 60), (142, 82, 164, 108)],
    6: [(14, 80, 34, 104), (124, 26, 146, 60), (142, 80, 164, 106)],
    7: [(16, 74, 31, 108), (106, 52, 124, 82), (138, 86, 154, 120)],
    8: [(14, 80, 34, 104), (118, 34, 140, 66), (106, 98, 130, 128)],
    9: [(14, 80, 34, 106), (124, 30, 146, 62), (144, 82, 166, 108)],
    10: [(14, 80, 34, 104), (124, 26, 146, 60), (142, 80, 166, 108)],
    11: [(14, 80, 34, 104), (124, 26, 146, 60), (142, 80, 166, 108)],
}


def mock(i):
    return np.asarray(dfx.mockup_panel(i)).astype(np.float32)


_MOCK_HD = {}


def mock_hd(i):
    """장면 i 를 게임 폭 × HD(528) 로 — 원본(526 폭)을 0.4 % 늘인 것. 1× 의 `mock` 은 이것의 3 × 3 면적 평균과 거의 같다."""
    if i not in _MOCK_HD:
        m = Image.open(os.path.join(dfx.MOCKUPS, dfx.MOCK_FILES[i // 4])).convert("RGB").crop(dfx.MOCK_BOXES[i % 4])
        w, h = m.size
        _MOCK_HD[i] = np.asarray(m.resize((176 * HD, round(h * 176 / w) * HD), Image.LANCZOS)).astype(np.float32)
    return _MOCK_HD[i]


def upsample(a, k):
    """H×W(×C) → kH×kW 겹선형 (1× 의 배경 추정 · 메운 층을 HD 로)."""
    if a.ndim == 2:
        return upsample(a[..., None], k)[..., 0]
    H, W, C = a.shape
    im = Image.fromarray(np.clip(a, 0, 255).astype(np.float32) if False else a.astype(np.float32), mode=None) if False else None
    yy, xx = np.mgrid[0:H * k, 0:W * k].astype(np.float32)
    fx, fy = (xx + 0.5) / k - 0.5, (yy + 0.5) / k - 0.5
    x0 = np.clip(np.floor(fx).astype(int), 0, W - 1)
    y0 = np.clip(np.floor(fy).astype(int), 0, H - 1)
    x1 = np.clip(x0 + 1, 0, W - 1)
    y1 = np.clip(y0 + 1, 0, H - 1)
    tx = np.clip(fx - x0, 0, 1)[..., None]
    ty = np.clip(fy - y0, 0, 1)[..., None]
    return (a[y0, x0] * (1 - tx) * (1 - ty) + a[y0, x1] * tx * (1 - ty) + a[y1, x0] * (1 - tx) * ty + a[y1, x1] * tx * ty).astype(np.float32)


def seal_valid(m):
    """★ HD: 인장 타원 안에서 「인장의 픽셀」 — 붉은 · 검붉은 · 어두운 것. 풀 · 나무(초록) · 모래길 · 병사 · 칼날은 구멍으로 보고 같은
    반지름의 다른 각도에서 메운다 (옛 오리기는 타원 안을 통째로 옮겨 초안의 나무 · 풀이 인장에 섞였다 — 사용자: *"원이 잘린 느낌"*)."""
    r, g, b, mx, sat, lum = _hsv(m)
    red = (r > g + 18) & (r > b + 10)
    dark = (lum < 78) & (g < r + 14)
    return red | dark


# ── 색 규칙 ────────────────────────────────────────────────────────────────────────────────────────────────
def _hsv(m):
    r, g, b = m[..., 0], m[..., 1], m[..., 2]
    mx, mn = m.max(axis=2), m.min(axis=2)
    return r, g, b, mx, (mx - mn) / (mx + 1), m.mean(axis=2)


def grow(mask, n):
    for _ in range(n):
        p = np.pad(mask, 1)
        mask = p[:-2, 1:-1] | p[2:, 1:-1] | p[1:-1, :-2] | p[1:-1, 2:] | mask
    return mask


def rule(m, kinds, near=4, dark_ok=True):
    """색 규칙 → 이펙트 픽셀 마스크 (near = 어두운 조각을 받아들이는 둘레 — 1× 에서 4 px; dark_ok = 어두운 조각을 받는가 —
    ★ 할퀴기는 안 받는다: 초안의 검은 조각은 이제 안 쓰고, 그 규칙이 나무 그늘까지 끌어왔다)."""
    r, g, b, mx, sat, lum = _hsv(m)
    core = np.zeros(m.shape[:2], bool)
    for k in kinds:
        if k == "red":  # 붉은 · 주황 · 노랑 (불 · 참격)
            core |= (r > g + 38) & (r > 110) & (r > b + 20)
            core |= (r > 200) & (g > 150) & (b < 120) & (r > b + 80)  # 노랑 심
        elif k == "violet":  # 보라 · 자홍 · 분홍
            core |= (b > g + 28) & (r > g + 8) & (mx > 80)
            core |= (r > 200) & (b > 150) & (g < r - 30)
        elif k == "navy":  # 남보라 잔상
            core |= (b > g + 18) & (b > r + 6) & (lum < 140) & (mx > 50)
        elif k == "dust":  # 흙먼지 (풀 위의 밝은 갈색 · 회색)
            core |= (r > g + 6) & (g > b + 4) & (lum > 110) & (sat < 0.45)
        elif k == "nongreen":  # 풀 · 모래길이 아닌 모든 것 (잔상처럼 어두운 남색 덩어리)
            grass = (g > r + 6) & (g > b + 2)
            sand = (r > 150) & (g > 120) & (b > 90) & (sat < 0.35)
            core |= ~grass & ~sand & (lum < 150)
    white = (lum > 190) & (sat < 0.3)
    dark = (lum < 46) & ~(g > r + 5)  # 어두운 조각 — 나뭇잎(초록이 더 많다)은 아니다 (★ HD 에서 참격 옆 나무가 딸려 왔다)
    ring = grow(core, near)
    out = core | (white & grow(core, 2))
    return out | (dark & ring) if dark_ok else out


# ── 배경 메우기 · 알파 풀기 ────────────────────────────────────────────────────────────────────────────────
def _blur5(a):
    p = np.pad(a, [(1, 1), (1, 1)] + [(0, 0)] * (a.ndim - 2), mode="edge")
    return (p[:-2, 1:-1] + p[2:, 1:-1] + p[1:-1, :-2] + p[1:-1, 2:] + p[1:-1, 1:-1]) / 5


def inpaint(img, hole, it=300):
    """hole 밖의 값으로 hole 안을 확산해 메운다 (img = H×W×C)."""
    known = (~hole).astype(np.float32)
    acc = img * known[..., None]
    w = known.copy()
    for _ in range(it):
        acc2, w2 = _blur5(acc), _blur5(w)
        acc = np.where(hole[..., None], acc2, img * known[..., None])
        w = np.where(hole, w2, known)
    return acc / np.maximum(w, 1e-4)[..., None]


def matte(m, B, mask, floor=0.12):
    up = np.where(m > B, (m - B) / np.maximum(255 - B, 1), 0)
    dn = np.where(m < B, (B - m) / np.maximum(B, 1), 0)
    a = np.maximum(up, dn).max(axis=2)
    a = np.where(mask, a, 0)
    a = np.where(a < floor, 0, np.clip((a - floor) / (1 - floor) * 1.1 + 0.08, 0, 1))
    F = np.clip((m - (1 - a[..., None]) * B) / np.maximum(a[..., None], 1e-3), 0, 255)
    return F, a


def fill_layer(F, a, hole, it=200):
    """오린 층의 구멍(초안 속 캐릭터 자리)을 둘레의 층으로 메운다 — 곱한 색과 알파를 같이 확산."""
    pre = np.dstack([F * a[..., None], a])
    known = (~hole) & (a > 0.05)
    out = inpaint(pre, hole & ~known, it) if hole.any() else pre
    out = np.where(hole[..., None], out, pre)
    a2 = np.clip(out[..., 3], 0, 1)
    F2 = out[..., :3] / np.maximum(a2, 1e-3)[..., None]
    return np.clip(F2, 0, 255), a2


# ── 오리기 표 ──────────────────────────────────────────────────────────────────────────────────────────────
# name: 결과 이름 · src: 초안 장면 · box: 오릴 상자 · kinds: 색 규칙 · ex: 뺄 상자(초안 속 캐릭터) · fill: 빼고 메울 상자 ·
# keep: ex 안에서도 남길 엄격한 규칙 · anchor: 기준점(기본 = 그 장면의 악마 발바닥, "mob<k>" = 몬스터 k 의 발)
def _demon_box(i, w=15, top=38, below=3):
    fx, fy = FEET[i]
    return (fx - w, fy - top, fx + w, fy + below)


CUTS = [
    # ★ 2026-10-15: the claws take no dark chips (no_dark) and leave the contact stars · the soldier's sword out (ex)
    dict(name="claw_x", src=1, box=(60, 58, 140, 134), kinds=["red"], ex=[_demon_box(1, 18, 36, 8), (100, 54, 146, 104)], mobs=True, no_dark=True),
    dict(name="claw_crescent", src=2, box=(72, 52, 142, 134), kinds=["red"], ex=[_demon_box(2, 20, 34, 8), (134, 60, 150, 92)], mobs=True, no_dark=True),
    dict(name="claw_rend", src=3, box=(62, 60, 152, 134), kinds=["red", "dust"], ex=[_demon_box(3, 20, 38, 8), (130, 82, 152, 106)], mobs=True, no_dark=True),
    dict(name="charge_trail", src=4, box=(26, 74, 112, 116), kinds=["red", "navy", "nongreen"], mobs=True),
    dict(name="charge_front", src=4, box=(100, 66, 168, 108), kinds=["red", "violet"], ex=[(108, 80, 138, 116), (153, 78, 170, 106)]),
    dict(name="dome", src=5, box=(38, 46, 140, 132), kinds=["violet"], ex=[_demon_box(5, 17, 40)], mobs=True, tint=(88, 123, 47, 72, 14)),
    dict(name="burst", src=6, box=(24, 36, 160, 134), kinds=["violet"], ex=[_demon_box(6, 14, 34)], mobs=True),
    dict(name="seal", src=7, box=(0, 92, 176, 142), copy=(80, 117, 84, 23), fill=[_demon_box(7, 24, 30)], mobs=True, polar_fill=True),
    dict(name="blade", src=7, box=(26, 54, 50, 116), kinds=["red", "violet"], mobs=True, anchor=(24, 105)),
    dict(name="bolt", src=8, box=(96, 0, 146, 132), kinds=["violet"], fill=[(108, 98, 128, 128)], mobs=True, anchor=(118, 125)),
    dict(name="fire", src=9, box=(0, 12, 176, 140), kinds=["red"], fill=[_demon_box(9, 14, 32)], mobs=True, split=True),
    dict(name="lava_flame0", src=10, box=(74, 78, 94, 103), kinds=["red"], poly=[(84, 79), (91, 92), (90, 102), (78, 102), (77, 92)], anchor=(84, 101)),
    dict(name="lava_flame1", src=10, box=(104, 84, 124, 104), kinds=["red"], poly=[(114, 85), (121, 95), (120, 103), (108, 103), (107, 95)], anchor=(114, 102)),
    dict(name="transform_reveal", src=0, box=(44, 56, 134, 134), kinds=["violet", "dust"], fill=[_demon_box(0, 14, 40)], mobs=True, split=True),
]


def _rects(mask, rects, x0, y0, value):
    for (a0, b0, a1, b1) in rects:
        mask[max(b0 - y0, 0):max(b1 - y0, 0), max(a0 - x0, 0):max(a1 - x0, 0)] = value


def cut(spec):
    return cut_at(spec, 1)[:3]


POLAR_STEPS = [math.pi, math.pi / 2, -math.pi / 2, 2 * math.pi / 3, -2 * math.pi / 3, math.pi / 3, -math.pi / 3, math.pi / 4, -math.pi / 4,
               3 * math.pi / 4, -3 * math.pi / 4, math.pi / 6, -math.pi / 6, 5 * math.pi / 6, -5 * math.pi / 6, math.pi / 12, -math.pi / 12]


def cut_at(spec, K, ref=None):
    """오리기 — K = 1 (게임 해상도) 또는 HD (예상도 해상도). ref = 1× 의 결과 `(F, a, anchor, B)` — HD 는 배경 추정과 메운 층을 그것에서
    늘려 쓴다. 답 = (F, a, (ax, ay), B) — 색(알파를 곱하지 않은) · 알파 0..1 · 기준점(그림 px) · 배경 추정."""
    i = spec["src"]
    m = mock(i) if K == 1 else mock_hd(i)
    sc = lambda v: v * K
    x0, y0, x1, y1 = [sc(v) for v in spec["box"]]
    boxes = [tuple(sc(v) for v in b) for b in spec.get("ex", [])]
    if spec.get("mobs"):
        boxes += [tuple(sc(v) for v in b) for b in MOBS[i]]
    fills = [tuple(sc(v) for v in b) for b in spec.get("fill", [])]
    if spec.get("copy"):  # 땅을 통째로 칠한 그림(인장) — 타원 안을 옮긴다 (★ HD 2026-10-15: 타원 전체 — 장면 밖 · 나무 · 풀은 구멍 → 극좌표 메우기)
        cx, cy, rx, ry = [sc(v) for v in spec["copy"]]
        if spec.get("polar_fill"):  # 타원이 장면 가장자리에 걸려도 통째로 — 상자를 타원 둘레로 넓힌다 (장면 밖은 구멍)
            x0, y0 = min(x0, int(math.floor(cx - rx)) - 2), min(y0, int(math.floor(cy - ry)) - 2)
            x1, y1 = max(x1, int(math.ceil(cx + rx)) + 3), max(y1, int(math.ceil(cy + ry)) + 3)
        H0, W0 = m.shape[:2]
        pad = np.zeros((y1 - y0, x1 - x0, 3), np.float32)
        outside = np.ones(pad.shape[:2], bool)
        sy0, sy1, sx0, sx1 = max(y0, 0), min(y1, H0), max(x0, 0), min(x1, W0)
        pad[sy0 - y0:sy1 - y0, sx0 - x0:sx1 - x0] = m[sy0:sy1, sx0:sx1]
        outside[sy0 - y0:sy1 - y0, sx0 - x0:sx1 - x0] = False
        sub = pad
        yy, xx = np.mgrid[y0:y1, x0:x1].astype(np.float32)
        d = np.sqrt(((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2)
        a = np.clip((1.0 - d) * min(rx, ry) / (3.0 * K), 0, 1)
        F = sub.copy()
        hole = np.zeros(sub.shape[:2], bool)
        _rects(hole, fills, x0, y0, True)
        _rects(hole, boxes, x0, y0, True)
        hole |= outside
        if spec.get("polar_fill"):
            hole |= ~seal_valid(sub)
        hole &= a > 0
        if spec.get("polar_fill"):  # 둥근 인장 — 구멍은 같은 반지름의 다른 각도(땅 평면에서 돌린 자리)에서, 가까운 각도부터
            ys, xs = np.nonzero(hole)
            u = (xs + x0 - cx) / rx
            v = (ys + y0 - cy) / ry
            r, th = np.hypot(u, v), np.arctan2(v, u)
            done = np.zeros(len(xs), bool)
            for dth in POLAR_STEPS:
                sx = np.round(cx + r * np.cos(th + dth) * rx - x0).astype(int)
                sy = np.round(cy + r * np.sin(th + dth) * ry - y0).astype(int)
                ok = ~done & (sx >= 0) & (sy >= 0) & (sx < a.shape[1]) & (sy < a.shape[0])
                ok[ok] &= ~hole[sy[ok], sx[ok]]
                F[ys[ok], xs[ok]] = sub[sy[ok], sx[ok]]
                done |= ok
            hole[ys[done], xs[done]] = False
        if hole.any():
            F = inpaint(F, hole, 150 * K)
        ax, ay = [sc(v) for v in spec.get("anchor", FEET[i])]
        return F, a, (ax - x0, ay - y0), F
    sub = m[y0:y1, x0:x1]
    fill = np.zeros(sub.shape[:2], bool)
    _rects(fill, fills, x0, y0, True)
    mask = rule(sub, spec["kinds"], near=4 * K, dark_ok=not spec.get("no_dark"))
    if spec.get("poly"):  # 손으로 그린 테두리 안만 (불기둥처럼 같은 색 위에 선 것)
        pm = Image.new("L", (x1 - x0, y1 - y0), 0)
        ImageDraw.Draw(pm).polygon([(sc(x) - x0, sc(y) - y0) for x, y in spec["poly"]], fill=255)
        mask &= np.asarray(pm) > 0
    _rects(mask, boxes, x0, y0, False)
    mask &= ~fill
    gm = grow(mask, 1) & ~fill  # 한 픽셀 둘레만 (★ HD 에서 K 픽셀을 더하자 풀 · 나뭇잎의 결이 이펙트로 딸려 왔다)
    _rects(gm, boxes, x0, y0, False)
    if K == 1 or ref is None:
        B = inpaint(sub, gm | fill)
    else:  # ★ HD: 1× 의 배경 추정을 늘린 뒤 짧게 확산 — 가장자리의 풀 결만 HD 로
        B = inpaint(np.where((gm | fill)[..., None], upsample(ref[3], K), sub), gm | fill, 40)
    F, a = matte(sub, B, gm)
    if spec.get("poly"):  # 테두리에 묻은 풀빛은 버린다
        a = np.where(F[..., 1] > F[..., 0] - 10, 0, a)
    if spec.get("tint"):  # 반구: 안이 비치는 보라 막 — 띠 밑에 고른 엷은 보라를 깐다
        cx, cy, rx, h, ry = [sc(v) for v in spec["tint"]]
        yy, xx = np.mgrid[y0:y1, x0:x1].astype(np.float32)
        u = (xx - cx) / rx
        v = np.where(yy < cy, (cy - yy) / h, (yy - cy) / ry)
        inside = (u * u + v * v) <= 1.0  # 초안 속 캐릭터 자리도 막 안이다 (악마는 반구 안에 선다)
        ta = np.where(inside, 0.16 + 0.22 * np.clip(np.sqrt(u * u + v * v) - 0.55, 0, 1) / 0.45, 0)
        tint = np.array([132, 56, 210], np.float32)
        out_a = a + ta * (1 - a)
        F = (F * a[..., None] + tint * (ta * (1 - a))[..., None]) / np.maximum(out_a, 1e-3)[..., None]
        a = out_a
    if fill.any():
        if K == 1 or ref is None:
            F, a = fill_layer(F, a, fill)
        else:  # ★ HD: 메운 층은 1× 의 것을 늘려 구멍에 (확산한 그림이라 늘려도 그대로다)
            F = np.where(fill[..., None], upsample(ref[0], K), F)
            a = np.where(fill, upsample(ref[1], K), a)
    ax, ay = [sc(v) for v in spec.get("anchor", FEET[i])]
    return F, a, (ax - x0, ay - y0), B


def _save(name, F, a, anchor, spec, meta, out_dir=OUT_DIR, scale=1):
    rgba = np.dstack([np.clip(F, 0, 255), a * 255]).astype(np.uint8)
    rgba[rgba[..., 3] == 0] = 0
    Image.fromarray(rgba, "RGBA").save(os.path.join(out_dir, name + ".png"))
    meta[name] = {"src": spec["src"], "box": list(spec["box"]), "anchor": [int(anchor[0]), int(anchor[1])], "scale": scale}
    print(f"{name}{' (HD)' if scale > 1 else ''}: {rgba.shape[1]}×{rgba.shape[0]} · anchor {anchor} · {int((a > 0).sum())} px")


def build(hd=True):
    os.makedirs(OUT_DIR, exist_ok=True)
    os.makedirs(OUT_DIR_HD, exist_ok=True)
    meta, meta_hd = {}, {}
    for spec in CUTS:
        res = cut_at(spec, 1)
        F, a, anchor = res[:3]
        passes = [(F, a, anchor, OUT_DIR, 1, meta)]
        if hd:
            Fh, ah, anchor_h, _ = cut_at(spec, HD, res)
            passes.append((Fh, ah, anchor_h, OUT_DIR_HD, HD, meta_hd))
        for F, a, anchor, out_dir, scale, mt in passes:
            if spec.get("split"):  # 악마 자리(메운 곳) = 뒤 층(땅), 나머지 = 앞 층 — 둘을 겹치면 메운 그림 전체
                x0, y0 = [v * scale for v in spec["box"][:2]]
                back = np.zeros(a.shape, bool)
                _rects(back, [tuple(v * scale for v in b) for b in spec["fill"]], x0, y0, True)
                _save(spec["name"], F, np.where(back, 0, a), anchor, spec, mt, out_dir, scale)
                _save(spec["name"] + "_back", F, np.where(back, a, 0), anchor, spec, mt, out_dir, scale)
            else:
                _save(spec["name"], F, a, anchor, spec, mt, out_dir, scale)
    with open(os.path.join(OUT_DIR, "cuts.json"), "w", encoding="utf-8") as fh:
        json.dump(meta, fh, ensure_ascii=False, indent=1)
    if hd:
        with open(os.path.join(OUT_DIR_HD, "cuts.json"), "w", encoding="utf-8") as fh:
            json.dump(meta_hd, fh, ensure_ascii=False, indent=1)
    build_lava()


# ── 용암 질감 (예상도 11 「용암불 냉각」의 판 · 균열을 64 × 64 토러스로 이어 붙인다) ─────────────────────────────────
LAVA_N = 64
LAVA_PATCH, LAVA_OVER = 20, 4


LAVA_POOL = {10: (84, 122, 62, 24), 11: (84, 122, 62, 24)}  # 웅덩이 타원 (가운데 · 반지름)


def _lava_valid(m, i):
    """웅덩이 안의 판 · 균열만 — 풀 · 모래 · 불기둥 · 초안 속 캐릭터는 뺀다."""
    r, g, b = m[..., 0], m[..., 1], m[..., 2]
    lum = m.mean(axis=2)
    cx, cy, rx, ry = LAVA_POOL[i]
    yy, xx = np.mgrid[0:m.shape[0], 0:m.shape[1]]
    ell = ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 <= 1.0
    grass = (g > r + 4) & (g >= b - 4)
    sand = (lum > 170) & (r < 245) & (b > 120)
    flame = lum > 185
    bad = grass | sand | grow(flame, 2) | ~ell
    _rects(bad, [_demon_box(i, 17, 34)] + MOBS[i], 0, 0, True)
    return ~bad


def _cut_path(err, axis):
    """겹침 띠의 최소 오차 경계 (DP) — axis 0: 위에서 아래로 내려가는 세로 경로 (열 번호), 1: 가로 경로 (행 번호)."""
    e = err if axis == 0 else err.T
    n, k = e.shape
    acc = e.copy()
    back = np.zeros((n, k), int)
    for y in range(1, n):
        for x in range(k):
            lo, hi = max(0, x - 1), min(k, x + 2)
            j = lo + int(np.argmin(acc[y - 1, lo:hi]))
            back[y, x] = j
            acc[y, x] += acc[y - 1, j]
    path = np.zeros(n, int)
    path[-1] = int(np.argmin(acc[-1]))
    for y in range(n - 1, 0, -1):
        path[y - 1] = back[y, path[y]]
    return path


def lava_quilt(i=11, seed=7, hd=False):
    """64 × 64 토러스 퀼트 (float32 RGB). ★ HD: hd=True 면 (1×, 후보 수, 3×) — 같은 조각 · 같은 이음매를 HD 원본에서 다시 오린다
    (1× 가 고른 (cx, cy) 와 take 마스크를 3 배로 늘려 mock_hd 에서 복사) → HD 질감이 1× 와 같은 구조다."""
    m = mock(i)
    mh = mock_hd(i) if hd else None
    K = HD
    valid = _lava_valid(m, i)
    P, O, N = LAVA_PATCH, LAVA_OVER, LAVA_N
    B = P - O
    cands = []
    H, W = valid.shape
    for y in range(0, H - P + 1):
        for x in range(0, W - P + 1):
            if valid[y:y + P, x:x + P].all():
                cands.append((x, y))
    assert len(cands) >= 4, f"lava: {len(cands)} clean patches"
    rnd = random.Random(seed)
    used = {}
    out = np.zeros((N, N, 3), np.float32)
    have = np.zeros((N, N), bool)
    outh = np.zeros((N * K, N * K, 3), np.float32) if hd else None
    for j in range(N // B):
        for k in range(N // B):
            x0, y0 = k * B, j * B
            ys = (np.arange(P) + y0) % N
            xs = (np.arange(P) + x0) % N
            known = have[np.ix_(ys, xs)]
            cur = out[np.ix_(ys, xs)]
            scores = []
            for (cx, cy) in cands:
                p = m[cy:cy + P, cx:cx + P]
                d = ((p - cur) ** 2).sum(axis=2)
                near = sum(n for (ux, uy), n in used.items() if abs(ux - cx) < B and abs(uy - cy) < B)
                scores.append(float((d * known).sum()) / max(1, known.sum()) + 900.0 * near)  # 같은 자리를 또 쓰면 벌점
            order = sorted(range(len(cands)), key=lambda q: scores[q])
            cx, cy = cands[rnd.choice(order[:4])]
            used[(cx, cy)] = used.get((cx, cy), 0) + 1
            p = m[cy:cy + P, cx:cx + P]
            take = np.ones((P, P), bool)
            err = ((p - cur) ** 2).sum(axis=2)
            if known[:, :O].all():  # 왼쪽 겹침: 경로 왼쪽은 옛 그림
                path = _cut_path(err[:, :O], 0)
                for y in range(P):
                    take[y, :path[y]] = False
            if known[:, P - O:].all():  # 오른쪽(첫 열로 감긴) 겹침: 경로 오른쪽은 옛 그림
                path = _cut_path(err[:, P - O:], 0)
                for y in range(P):
                    take[y, P - O + path[y] + 1:] = False
            if known[:O, :].all():
                path = _cut_path(err[:O, :], 1)
                for x in range(P):
                    take[:path[x], x] = False
            if known[P - O:, :].all():
                path = _cut_path(err[P - O:, :], 1)
                for x in range(P):
                    take[P - O + path[x] + 1:, x] = False
            take |= ~known
            blk = np.where(take[..., None], p, cur)
            out[np.ix_(ys, xs)] = blk
            have[np.ix_(ys, xs)] = True
            if hd:
                ysh = (np.arange(P * K) + y0 * K) % (N * K)
                xsh = (np.arange(P * K) + x0 * K) % (N * K)
                ph = mh[cy * K:(cy + P) * K, cx * K:(cx + P) * K]
                th = np.repeat(np.repeat(take, K, axis=0), K, axis=1)
                outh[np.ix_(ysh, xsh)] = np.where(th[..., None], ph, outh[np.ix_(ysh, xsh)])
    if hd:
        return out, len(cands), outh
    return out, len(cands)


def lava_hot(cool, scale=1):
    """식은 질감 → 끓는 질감: 같은 균열을 넓히고 노랑 · 주황으로, 판은 균열 가까이 붉게 달군다 (예상도 10 의 색).
    ★ HD: scale 배 질감이면 띠 폭도 scale 배 (거리를 게임 px 로 접는다)."""
    r, g = cool[..., 0], cool[..., 1]
    crack = (r > g + 50) & (r > 120)
    hot_core = crack & (g > 120)
    dist = np.full(crack.shape, 9.0, np.float32)
    k = crack.copy()
    dist[k] = 0
    for dd in range(1, 5 * scale + 1):
        p = np.pad(k, 1, mode="wrap")
        k = k | p[:-2, 1:-1] | p[2:, 1:-1] | p[1:-1, :-2] | p[1:-1, 2:]
        dist[k & (dist > dd)] = dd
    if scale > 1:
        dist = np.where(dist >= 9.0, 9.0, np.ceil(dist / scale))
    lum = cool.mean(axis=2, keepdims=True)
    plate = np.concatenate([lum * 1.02 + 10, lum * 0.56, lum * 0.5], axis=2)
    out = plate.copy()
    stops = [(0, (255, 168, 44)), (1, (226, 66, 20)), (2, (128, 26, 14)), (3, (96, 24, 16))]
    for d, col in reversed(stops):
        w = {0: 1.0, 1: 0.72, 2: 0.36, 3: 0.16}[d]
        sel = (dist == d)[..., None]
        out = np.where(sel, plate * (1 - w) + np.array(col, np.float32) * w, out)
    out = np.where(hot_core[..., None], np.array([255, 238, 150], np.float32), out)
    return np.clip(out, 0, 255)


def build_lava(hd=True):
    cool, n, coolh = lava_quilt(hd=True)
    hot = lava_hot(cool)
    Image.fromarray(cool.astype(np.uint8)).save(os.path.join(OUT_DIR, "lava_cool.png"))
    Image.fromarray(hot.astype(np.uint8)).save(os.path.join(OUT_DIR, "lava_hot.png"))
    print(f"lava: 64×64 cool/hot from {n} clean 20×20 patches")
    if hd:  # ★ HD: 같은 퀼트를 3× 원본 픽셀로 (demon_fx_art.lava_tile 의 HD 쌍)
        os.makedirs(OUT_DIR_HD, exist_ok=True)
        hoth = lava_hot(coolh, HD)
        Image.fromarray(np.clip(coolh, 0, 255).astype(np.uint8)).save(os.path.join(OUT_DIR_HD, "lava_cool.png"))
        Image.fromarray(np.clip(hoth, 0, 255).astype(np.uint8)).save(os.path.join(OUT_DIR_HD, "lava_hot.png"))
        print(f"lava HD: {LAVA_N * HD}×{LAVA_N * HD} cool/hot (same patches · seams ×{HD})")


def load(name):
    """(RGBA uint8 배열, 기준점 (ax, ay)) — 기준점 = 그림 왼쪽 위에서 발바닥까지."""
    with open(os.path.join(OUT_DIR, "cuts.json"), encoding="utf-8") as fh:
        meta = json.load(fh)[name]
    arr = np.asarray(Image.open(os.path.join(OUT_DIR, name + ".png")).convert("RGBA"))
    return arr, tuple(meta["anchor"])


def load_hd(name):
    """★ HD: (RGBA uint8 배열, 기준점 (ax, ay) — 그림 px, 배율) 또는 None (HD 오리기가 없으면 1× 를 쓴다)."""
    path = os.path.join(OUT_DIR_HD, "cuts.json")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as fh:
        meta = json.load(fh).get(name)
    if meta is None or not os.path.exists(os.path.join(OUT_DIR_HD, name + ".png")):
        return None
    arr = np.asarray(Image.open(os.path.join(OUT_DIR_HD, name + ".png")).convert("RGBA"))
    return arr, tuple(meta["anchor"]), int(meta.get("scale", HD))


def preview(out, zoom=3):
    bg = dfx.real_background().astype(np.float32)
    rows = []
    for spec in CUTS:
        i = spec["src"]
        m = mock(i)
        c = bg[:m.shape[0]].copy()
        fx, fy = spec.get("anchor", FEET[i])
        for name in ([spec["name"] + "_back"] if spec.get("split") else []) + [spec["name"]]:
            arr, (ax, ay) = load(name)
            h, w = arr.shape[:2]
            x, y = fx - ax, fy - ay
            al = arr[..., 3:4] / 255.0
            c[y:y + h, x:x + w] = arr[..., :3] * al + c[y:y + h, x:x + w] * (1 - al)
        a = m.copy()
        for img in (a, c):
            img[fy, max(fx - 3, 0):fx + 4] = (0, 255, 0)
            img[max(fy - 3, 0):fy + 4, fx] = (0, 255, 0)
        rows.append((spec["name"], np.concatenate([a, np.full((m.shape[0], 4, 3), 20, np.float32), c], axis=1)))
    cols = 2
    rh, rw = rows[0][1].shape[:2]
    sheet = Image.new("RGB", (cols * (rw * zoom + 10) + 10, (len(rows) + 1) // cols * (rh * zoom + 26) + 10), (20, 20, 26))
    d = ImageDraw.Draw(sheet)
    for k, (name, r) in enumerate(rows):
        x, y = 10 + (k % cols) * (rw * zoom + 10), 10 + (k // cols) * (rh * zoom + 26)
        d.text((x, y), name, fill=(230, 230, 230))
        im = Image.fromarray(np.clip(r, 0, 255).astype(np.uint8))
        sheet.paste(im.resize((rw * zoom, rh * zoom), Image.NEAREST), (x, y + 16))
    sheet.save(out)
    print(f"preview → {out}")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("build")
    p.add_argument("--no-hd", action="store_true", help="1× 만 (HD 오리기를 건너뛴다)")
    p = sub.add_parser("preview")
    p.add_argument("out")
    p.add_argument("--zoom", type=int, default=3)
    a = ap.parse_args()
    if a.cmd == "build":
        build(hd=not a.no_hd)
    else:
        preview(a.out, a.zoom)


if __name__ == "__main__":
    main()
