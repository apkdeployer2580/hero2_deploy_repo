#!/usr/bin/env python3
"""용암불 바닥 4 차 (2026-10-15) — 칸마다의 사각 타일 → 초안 11 「용암불 연소」의 용암 지대를 그대로 오려 고리 12 칸에 나눠 붙인다.

사용자: *"용암불이 타일에 불이 남는것처럼 나와서 사각형으로 이펙트가 남아서 어색한데, 이거 초안처럼 자연스럽게 불 위치에 불 남게
해줘. 초안이랑 최대한 똑같이 하고싶어"* — 초안 = `docs/skill/astra_demon/ingame/03_judgment_fire_lava.png` 의 11 「용암불 연소」 ·
12 「용암불 냉각」(악마 발 둘레의 넓고 납작한 용암 지대: 적갈색 · 검은 판 사이 빨강 → 주황 → 노랑 균열, 들쭉날쭉한 가장자리와 풀로
번지는 붉은 빛, 둘레의 검은 돌, 균열 위로 솟는 불혀; 식으면 판이 청회색이 되고 균열이 가늘고 검붉어진다).

옛 그림(`demon_fx_art.lava_tile`)은 64 × 64 토러스 질감을 칸 (x & 3, y & 3) 이 22 × 22 로 잘라 쓴 것이라 고리 12 칸(가운데 칸을 둘러싼
3 × 3 + 네 끝)이 그대로 「十 자 사각 덩어리」였다. 이번 그림의 규칙 (⛔ 「초안과 똑같이」는 다시 그리지 말고 초안 픽셀을 오린다 —
`HERO2_GUEST_RE.md` §8.73 의 3 차 규율):

* **조각 = 고리 안 자리**: 지옥불 고리는 야수 칸 + `co.a/co.b[dir]` 열두 칸이고(probe: 12 칸이 같은 틱에 적힌다), 게스트의 LAVA 조각이
  칸마다 그 칸의 고리 안 자리(PIECE_OF — 아래 `RING` 순서 0..11, 그 밖은 12..15)를 `LAVA_PIECE` 에 남긴다 → FXG/FXT 가 `FX_LAVA +
  조각` 을 칸 왼쪽 위에 그린다. 그래서 열두 조각을 이으면 **초안의 지대 한 장**이 된다(초안 속 악마 발 = 고리 가운데 칸의 발).
* **나누기 = 판을 따라**: 지대를 균열로 갈린 판(연결 성분)마다 가장 가까운 고리 칸에 준다 — 맵 가장자리 · 다시 쓴 칸으로 조각이
  빠져도 잘린 자리가 직선이 아니라 균열을 따른다. 균열 픽셀은 가장 가까운 칸.
* 초안 속 악마 · 몬스터 자리는 같은 타원 반지름의 반대편에서 옮겨 메우고(`astra_cut` 인장과 같은 극좌표 메우기), 초안의 불혀 넷은 땅
  그림에서 지우고(인페인트) 위 층의 일렁이는 불혀(`astra_cut` lava_flame0/1)로 같은 자리에 다시 세운다.
* 나이 0..5 = 조각마다 그 칸 가운데에서 반지름이 자라며 드러난다(불이 떨어진 자리에서 번진다) · 6..55 연소(초안 11 그대로, 붉은
  빛이 조금 일렁인다) · 55..99 냉각(초안 12 의 색으로 다시 칠함 — 판은 청회색, 가장 밝은 균열만 붉게 남는다) · 92.. 사라짐.

    python tools_re/lava_fx.py replace [--src <bin>] [--out <bin>]   # 지금 bin 의 다른 효과는 바이트 그대로, 용암 32 효과만 새로
                                                                      # (★★ 2026-10-18 목걸이의 2 배 24 효과도 새 조각에서 — lava_wide_fx)
    python tools_re/lava_fx.py preview [--src <bin>] [--out <dir>]   # 초안 11 · 12 와 나란히 (고리 12 칸 · 나이 여럿)
    python tools_re/lava_fx.py field [--out <png>]                    # 오린 지대 · 나눈 조각 확인 그림
"""

import argparse
import math
import os
import random
import sys

import numpy as np
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools_re"))

LEN = 100  # = demon_fx.LAVA_LEN = DemonPatch::LAVA_FRAMES
LEVELS = 10  # 냉각 단계
SRC = 10  # 초안 장면 번호 (11 「용암불 연소」)
# 고리 안 자리 (dx, dy) — 조각 번호 = 이 표의 순서 (= DemonPatch::LAVA_RING; probe 의 LAVA_TAB 순서 = 방향 1..12)
RING = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1), (0, -2), (0, 2), (-2, 0), (2, 0)]
SPARE = 4  # 조각 12..15 = 고리 밖 자리(다른 고리 표) — 그 칸 둘레만의 작은 덩어리
POOL = (84, 122, 70, 26)  # 지대를 찾는 타원 (가운데 · 반지름 — 176 폭 초안 좌표)
DEMON_BOX = (56, 96, 103, 131)  # 초안 11 의 웅크린 악마 (발 (82, 126)) — 메울 자리
# 초안의 불혀 (상자 · 밑동 · 크기 · 지대 윗선 위로 솟았나) — 땅 그림에서 지우고 위 층에 다시 세운다
FLAMES = [((80, 84, 89, 102), (84, 101), 15, True), ((110, 86, 121, 104), (115, 103), 16, True),
          ((50, 103, 60, 119), (55, 118), 14, False), ((117, 116, 127, 135), (122, 134), 17, False)]
GROW = [5.0, 9.0, 13.0, 18.0, 24.0, 99.0]  # 나이 0..5 에 조각이 드러나는 반지름 (게임 px, 세로는 ×1.5 로 잰다)

_C = {}
HD_K = 3  # ★ HD 배율 (= astra_cut.HD = fxpaint.HD_SCALE)


def _mock():
    import astra_cut as ac
    return ac.mock(SRC)


def centre():
    """초안 속 고리 가운데 칸의 왼쪽 위 = 악마 발 − FEET(8, 10) — 발이 칸 가운데 (8, 10) 이다 (`DemonPatch::FEET`)."""
    import astra_cut as ac
    fx, fy = ac.FEET[SRC]
    return fx - 8, fy - 10


def cell_tl(dx, dy):
    cx, cy = centre()
    return cx + 16 * dx, cy + 16 * dy


def _components(mask):
    """4 이웃 연결 성분 번호 (−1 = 밖)."""
    H, W = mask.shape
    lab = np.full((H, W), -1, np.int32)
    n = 0
    for y in range(H):
        for x in range(W):
            if mask[y, x] and lab[y, x] < 0:
                st = [(y, x)]
                lab[y, x] = n
                while st:
                    cy, cx = st.pop()
                    for ny, nx in ((cy - 1, cx), (cy + 1, cx), (cy, cx - 1), (cy, cx + 1)):
                        if 0 <= ny < H and 0 <= nx < W and mask[ny, nx] and lab[ny, nx] < 0:
                            lab[ny, nx] = n
                            st.append((ny, nx))
                n += 1
    return lab, n


def _exemplar_fill(img, member, hole, region, k=3, seed=11):
    """견본 메우기 (Criminisi 꼴): 구멍 테두리에서 아는 이웃이 가장 많은 점부터, 그 (2k+1)² 창과 아는 픽셀이 가장 닮은
    「region 안의 온전한 창」을 찾아 구멍 픽셀만 옮긴다 — 색과 member(지대인가)를 함께. 거울 · 확산과 달리 무늬가 그대로 이어진다."""
    img = img.copy()
    member = member.copy()
    hole = hole.copy()
    H, W = hole.shape
    rnd = np.random.RandomState(seed)
    ok = region & ~hole
    win = 2 * k + 1
    cand = []
    for y in range(k, H - k):
        for x in range(k, W - k):
            if ok[y - k:y + k + 1, x - k:x + k + 1].all():
                cand.append((y, x))
    cand = np.array(cand)
    if len(cand) == 0:
        return img, member
    offs = np.array([(dy, dx) for dy in range(-k, k + 1) for dx in range(-k, k + 1)])
    cy = cand[:, 0][:, None] + offs[None, :, 0]
    cx = cand[:, 1][:, None] + offs[None, :, 1]
    patches = img[cy, cx]  # N × win² × 3
    while hole.any():
        p = np.pad(hole, 1)
        edge = hole & ~(p[:-2, 1:-1] & p[2:, 1:-1] & p[1:-1, :-2] & p[1:-1, 2:])
        ys, xs = np.nonzero(edge)
        best, by, bx = -1, 0, 0
        for y, x in zip(ys, xs):
            y0, y1, x0, x1 = max(0, y - k), min(H, y + k + 1), max(0, x - k), min(W, x + k + 1)
            n = (~hole[y0:y1, x0:x1]).sum()
            if n > best:
                best, by, by, bx = n, y, y, x
        ty = np.clip(by + offs[:, 0], 0, H - 1)
        tx = np.clip(bx + offs[:, 1], 0, W - 1)
        known = ~hole[ty, tx]
        t = img[ty, tx]
        d = (((patches - t[None]) ** 2).sum(axis=2) * known[None]).sum(axis=1)
        order = np.argsort(d)[:3]
        pick = cand[order[rnd.randint(len(order))]]
        fill = hole[ty, tx]
        sy = pick[0] + offs[:, 0]
        sx = pick[1] + offs[:, 1]
        img[ty[fill], tx[fill]] = img[sy[fill], sx[fill]]
        member[ty[fill], tx[fill]] = member[sy[fill], sx[fill]]
        hole[ty[fill], tx[fill]] = False
    return img, member


def field():
    """초안 11 에서 오린 지대 — (색 H×W×3 float, 알파 H×W, 균열 정도 R, 판 번호) 를 176 폭 초안 좌표로."""
    if "field" in _C:
        return _C["field"]
    import astra_cut as ac
    m = _mock()
    H, W = m.shape[:2]
    r, g, b = m[..., 0], m[..., 1], m[..., 2]
    lum = m.mean(axis=2)
    yy, xx = np.mgrid[0:H, 0:W]
    cx, cy, rx, ry = POOL
    ell = ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 <= 1.0
    grass = (g > r + 4) & (g >= b - 4)
    foliage = (b > r + 8) | grass  # 나무 그늘 (어두운 청록) 도 풀 쪽
    sand = (r > 110) & (g > 90) & (b > 60) & (r - g < 55) & (g - b < 50)  # 흙길 · 나무 밑동 (채도 낮은 밝은 갈색)
    hotish = (lum < 100) | (r - g > 48)  # 용암 = 어둡거나 붉다
    # ① 초안 속 악마 → 상자의 가까운 세로 변에 비친 자리(이음매 없이 이어진다) · 몬스터 → 같은 타원 반지름의 다른 각도에서
    #    옮겨 메운다 (색과 「지대인가」를 함께)
    hole = np.zeros((H, W), bool)
    ac._rects(hole, [DEMON_BOX] + ac.MOBS[SRC], 0, 0, True)
    src = m.copy()
    lavaish = ell & ~foliage & ~sand & hotish
    # 악마 = 상자 안의 푸른 몸 · 검은 털 (붉은 발톱은 용암과 같은 색이라 둘레를 한 칸 넓혀 덮는다) → 견본 메우기
    box = np.zeros((H, W), bool)
    ac._rects(box, [DEMON_BOX], 0, 0, True)
    body = box & ((b > r + 4) | ((lum < 34) & (b >= r - 6)))
    body = ac.grow(body, 2) & box
    src, lavaish = _exemplar_fill(src, lavaish, body, (lavaish & ~hole) | body)  # 견본 = 온전히 용암인 창만
    hole &= ~box
    ys, xs = np.nonzero(hole & ell)
    u, v = (xs - cx) / rx, (ys - cy) / ry
    rr, th = np.hypot(u, v), np.arctan2(v, u)
    done = np.zeros(len(xs), bool)
    for dth in (math.pi, 2 * math.pi / 3, -2 * math.pi / 3, math.pi / 2, -math.pi / 2, math.pi / 3, -math.pi / 3):
        sx = np.round(cx + rr * np.cos(th + dth) * rx).astype(int)
        sy = np.round(cy + rr * np.sin(th + dth) * ry).astype(int)
        ok = ~done & (sx >= 0) & (sy >= 0) & (sx < W) & (sy < H)
        ok[ok] &= ~hole[sy[ok], sx[ok]]
        src[ys[ok], xs[ok]] = m[sy[ok], sx[ok]]
        lavaish[ys[ok], xs[ok]] = lavaish[sy[ok], sx[ok]]
        done |= ok
    lavaish &= ~(hole & ~ell)
    # ② 초안의 불혀 → 지운다: 지대 윗선 위는 밖, 그 밑은 둘레의 용암으로 메운다
    flame = np.zeros((H, W), bool)
    rises = np.zeros((H, W), bool)
    for (x0, y0, x1, y1), _, _, up in FLAMES:
        box = np.zeros((H, W), bool)
        box[y0:y1, x0:x1] = True
        flame |= box & (lum > 118) & (r > 150)
        if up:
            rises |= box
    flame = ac.grow(flame, 1) & ell
    lavaish |= flame  # 하얀 심(붉지 않다)도 지대 — 아래에서 밖이거나 메운다
    top = np.full(W, H, int)  # 열마다 불혀가 아닌 지대의 맨 위
    for x in range(W):
        col = np.nonzero(lavaish[:, x] & ~flame[:, x] & ell[:, x])[0]
        if len(col):
            top[x] = col.min()
    above = flame & rises & (yy < top[None, :])
    lavaish &= ~above
    fill = flame & ~above
    src, lavaish = _exemplar_fill(src, lavaish, fill, lavaish & ~fill, seed=12)  # 확산은 매끈한 붉은 얼룩이 된다 — 판 무늬로
    # ③ 지대 = 가운데에 이어진 「풀 · 모래 아닌」 픽셀 + 작은 구멍 메움; 둘레의 돌 · 붉은 빛은 따로(알파는 배경과의 차이로)
    lab, n = _components(lavaish)
    sizes = np.bincount(lab[lab >= 0], minlength=n)
    keep = np.zeros(n, bool)
    keep[sizes >= 30] = True  # 큰 덩어리(지대) — 작은 것(돌 · 불티)은 아래 ④ 에서
    core = (lab >= 0) & keep[np.maximum(lab, 0)]
    holes = ~core & ell
    hl, hn = _components(holes)
    hs = np.bincount(hl[hl >= 0], minlength=hn)
    small = np.zeros(hn, bool)
    small[hs <= 12] = True
    core |= (hl >= 0) & small[np.maximum(hl, 0)]
    # ④ 지대 밖 6 px 안의 붉은 빛 · 검은 돌 = 배경(풀을 인페인트) 위의 반투명 층
    near = ac.grow(core, 6) & ~core & (yy < H)
    r, g, b = src[..., 0], src[..., 1], src[..., 2]  # 메운 뒤의 색으로 다시 가른다
    lum = src.mean(axis=2)
    foliage = (b > r + 8) | ((g > r + 4) & (g >= b - 4))
    sand = (r > 110) & (g > 90) & (b > 60) & (r - g < 55) & (g - b < 50)
    # 검은 돌 = 지대 둘레의 작고 무채색인 어두운 덩어리 (나무 그늘처럼 크게 이어진 어둠은 뺀다)
    dark = near & ~foliage & ~sand & (lum < 60) & (np.abs(r - g) < 14) & (np.abs(b - r) < 16)
    dl, dn = _components(ac.grow(dark, 1) & near & (lum < 80))
    ds = np.bincount(dl[dl >= 0], minlength=dn)
    rock = dark & (dl >= 0) & (ds[np.maximum(dl, 0)] <= 40)
    extra = near & ~foliage & ~sand & (ac.rule(src, ["red"], near=2) & (lum >= 60) | rock)
    B = ac.inpaint(src, core | extra, 200)
    F, a = ac.matte(src, B, extra)
    alpha = np.where(core, 1.0, a)
    col = np.where(core[..., None], src, F)
    red = np.clip((col[..., 0] - np.maximum(col[..., 1], col[..., 2]) - 15) / 140, 0, 1)
    # 판 = 균열이 아닌 지대 픽셀의 연결 성분 (나누기의 단위)
    crack = (red > 0.4) & (col.mean(axis=2) > 70)
    plates, _ = _components((alpha > 0) & ~crack)
    _C["field"] = (col, alpha.astype(np.float32), red.astype(np.float32), plates, crack)
    _C["glow"] = extra & ~rock  # 풀 위의 붉은 빛 — 식으면 사라진다 (돌은 남는다)
    return _C["field"]


def pieces():
    """지대 픽셀마다 조각 번호 (−1 = 밖) — 판마다 가장 가까운 고리 칸(판 무게중심), 균열은 그 픽셀에서 가장 가까운 칸."""
    if "pieces" in _C:
        return _C["pieces"]
    col, alpha, red, plates, crack = field()
    H, W = alpha.shape
    centres = np.array([[cell_tl(dx, dy)[0] + 8, cell_tl(dx, dy)[1] + 8] for dx, dy in RING], np.float32)

    def nearest(px, py):
        d = (centres[:, 0][:, None] - px[None, :]) ** 2 + ((centres[:, 1][:, None] - py[None, :]) * 1.15) ** 2
        return np.argmin(d, axis=0)
    yy, xx = np.mgrid[0:H, 0:W]
    out = np.full((H, W), -1, np.int32)
    inside = alpha > 0
    out[inside] = nearest(xx[inside].astype(np.float32), yy[inside].astype(np.float32))
    n = plates.max() + 1
    if n > 0:
        idx = plates[plates >= 0]
        sx = np.bincount(idx, weights=xx[plates >= 0], minlength=n)
        sy = np.bincount(idx, weights=yy[plates >= 0], minlength=n)
        cnt = np.maximum(np.bincount(idx, minlength=n), 1)
        owner = nearest((sx / cnt).astype(np.float32), (sy / cnt).astype(np.float32))
        sel = plates >= 0
        out[sel] = owner[plates[sel]]
    _C["pieces"] = out
    return out


def recolor(col, red, h):
    """초안 11 의 색 → 열 h (1 = 그대로 · 0 = 초안 12 의 식은 색). 판 = 청회색 돌(밝기만 남긴다), 붉은 빛 = 식으며 회색으로,
    밝은 균열은 가늘고 검붉게 남는다 (초안 12: 판 (39, 43, 48) · 균열 (203, 60, 45) · (115, 32, 37))."""
    if h >= 0.999:
        return col
    lum = col.mean(axis=2)
    base = np.clip((lum - 30) * 0.8 + 36, 16, 92)
    grey = np.stack([base, base + 3, base + 8], axis=-1)
    k = np.clip((red - 0.28) / 0.4, 0, 1) * np.clip((lum - 45) / 60, 0, 1) * min(1.0, 0.6 + h)
    yel = np.clip((col[..., 1] - 70) / 120, 0, 1)
    crackc = np.stack([180 + 40 * yel, 44 + 30 * yel, 36 + 12 * yel], axis=-1)
    cool = grey * (1 - k[..., None]) + crackc * k[..., None]
    t = np.clip(h, 0, 1) ** 1.3
    return cool * (1 - t) + col * t


def field_hd():
    """★ HD (2026-10-15): 지대를 3× 로 — 마스크(알파 · 붉기 · 빛 · 조각 번호)는 1× 의 것을 늘이고(알파 · 붉기는 겹선형, 조각은 칸째),
    색은 1× 가 손대지 않은 픽셀이면 3× 원본(`astra_cut.mock_hd`)의 진짜 도트, 메운 자리(악마 · 몬스터 · 불혀)는 1× 메운 색을 늘인다.
    → (색 3H×3W×3, 알파, 붉기, 빛 마스크, 배율)."""
    if "field_hd" in _C:
        return _C["field_hd"]
    import astra_cut as ac
    col, alpha, red, _, _ = field()
    K = ac.HD
    H, W = alpha.shape
    mh = ac.mock_hd(SRC)[:H * K, :W * K]
    same = np.abs(col - _mock()).max(axis=2) < 0.5
    colh = ac.upsample(col, K)
    sameh = np.repeat(np.repeat(same, K, axis=0), K, axis=1)
    colh = np.where(sameh[..., None], mh, colh)
    alphah = np.clip(ac.upsample(alpha, K), 0, 1)
    redh = np.clip(ac.upsample(red, K), 0, 1)
    glowh = np.repeat(np.repeat(_C["glow"], K, axis=0), K, axis=1)
    _C["field_hd"] = (colh.astype(np.float32), alphah.astype(np.float32), redh.astype(np.float32), glowh, K)
    return _C["field_hd"]


def _piece_rgba(piece, level, grow_r, k=1):
    """조각 하나의 RGBA (칸 왼쪽 위 기준 스프라이트) — 냉각 단계 · 드러난 반지름. ★ HD: k = 3 이면 `field_hd` 위에서 같은 규칙
    (기준점 · 반지름은 게임 px 로 재고 그림은 3×) — 1× 조각의 쌍."""
    key = ("spr", piece, level, grow_r, k)
    if key in _C:
        return _C[key]
    import fxpaint as fp
    if k == 1:
        col, alpha, red, _, _ = field()
        glow = _C["glow"]
    else:
        col, alpha, red, glow, kk = field_hd()
        assert kk == k
    P = pieces()
    H, W = P.shape
    if piece < len(RING):
        dx, dy = RING[piece]
        sel = P == piece
    else:
        dx, dy = RING[(2, 3, 0, 1)[piece - len(RING)]]
        x0, y0 = cell_tl(dx, dy)
        yy, xx = np.mgrid[0:H, 0:W]
        sel = (((xx - x0 - 8) / 13.0) ** 2 + ((yy - y0 - 8) / 10.0) ** 2 <= 1.0) & (field()[1] > 0)
    if k > 1:
        sel = np.repeat(np.repeat(sel, k, axis=0), k, axis=1)
    x0, y0 = cell_tl(dx, dy)
    yy, xx = np.mgrid[0:H * k, 0:W * k].astype(np.float32)
    if k > 1:
        xx, yy = (xx + 0.5) / k, (yy + 0.5) / k
    shown = sel & (np.hypot(xx - x0 - 8, (yy - y0 - 8) * 1.5) <= grow_r)
    h = level / (LEVELS - 1)
    rgb = recolor(col, red, h)
    if level == LEVELS - 1:  # 연소의 일렁임은 프레임 단위 알파로 (아래 lava_effect) — 여기선 그대로
        pass
    a = np.where(shown, alpha, 0)
    a = np.where(glow, a * min(1.0, 0.15 + h * 1.1), a)
    arr = np.dstack([np.clip(rgb, 0, 255), a * 255]).astype(np.uint8)
    arr[arr[..., 3] == 0] = 0
    spr = fp._crop(arr, x0 * k, y0 * k)
    _C[key] = spr
    return spr


def heat_of(age):
    if age < 55:
        return 1.0
    return max(0.0, 1.0 - (age - 55) / 40)


def _flames_of(piece):
    """이 조각 안에 밑동이 있는 초안 불혀 — (칸 왼쪽 위 기준 밑동, 크기)."""
    if piece >= len(RING):
        return []
    P = pieces()
    dx, dy = RING[piece]
    x0, y0 = cell_tl(dx, dy)
    out = []
    for _, (bx, by), size, _ in FLAMES:
        if P[by - 1, bx] == piece or (P[by - 1, bx] < 0 and P[by - 3, bx] == piece):
            out.append((bx - x0, by - y0, size))
    return out


def _flame_src(i):
    """초안 불기둥 `astra_cut` lava_flame0/1 — 오릴 때 하얀 심이 알파 0 으로 빠진 구멍을 심 색으로 메운다(게임에서 그 구멍으로 풀이
    비쳐 불 속에 초록 줄이 보였다)."""
    key = ("flsrc", i)
    if key in _C:
        return _C[key]
    import demon_fx_cut as cx
    src = cx.cut(f"lava_flame{i}").copy()
    a = src.a[..., 3]
    low = a < 0.5
    H, W = low.shape
    reach = np.zeros((H, W), bool)
    st = [(y, x) for y in range(H) for x in (0, W - 1) if low[y, x]] + [(y, x) for x in range(W) for y in (0, H - 1) if low[y, x]]
    for y, x in st:
        reach[y, x] = True
    while st:
        y, x = st.pop()
        for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
            if 0 <= ny < H and 0 <= nx < W and low[ny, nx] and not reach[ny, nx]:
                reach[ny, nx] = True
                st.append((ny, nx))
    hole = low & ~reach
    src.a[hole, :3] = np.array([255, 242, 186], np.float32)
    src.a[hole, 3] = 1.0
    _C[key] = src
    return src


def _flame(lib, k, size):
    """일렁이는 불혀 (`demon_fx_art._draft_flame` 와 같은 움직임 — 구멍 메운 원본으로)."""
    import demon_fx_cut as cx
    src = _flame_src(k % 2)
    if (k // 2) % 2:
        src = src.flip()
    s = size / 14.0
    sy = (1.0, 0.84, 1.1, 0.92)[k % 4] * s
    b = src.wobble(k * 1.3, 0.9, ground=0.0, height=18.0, seed=k % 3).scale(0.9 * s + 0.1, sy, pivot=(0.0, 0.0))
    c = cx.canvas_for(b, 6)
    cx.put(c, b)
    c.glow(0, -size * 0.35, size * 0.4, size * 0.35, (255, 120, 40), 0.22, "add")
    return lib.parts_of(c)


def lava_effect(lib, piece):
    """용암 한 조각의 100 프레임 → (땅 층 부품, 위 층 부품) 목록 둘."""
    import demon_fx_art as art
    if id(lib) not in art._LAVA_PARTS:
        art._LAVA_PARTS.clear()
        art._LAVA_PARTS[id(lib)] = {"flame": {}, "ember": art._ember(lib), "smoke": [art._smoke(lib, s) for s in range(4)]}
    P = art._LAVA_PARTS[id(lib)]

    def flame_parts(k, size):
        key = ("lava4", k % 8, size)
        if key not in P["flame"]:
            P["flame"][key] = _flame(lib, k % 8, size)
        return P["flame"][key]
    flames = _flames_of(piece)
    rnd = random.Random(8800 + piece)
    embers = [(rnd.uniform(0, 16), rnd.uniform(2, 14)) for _ in range(2)]
    ground, top = [], []
    for age in range(LEN):
        lv = int(round(heat_of(age) * (LEVELS - 1)))
        gr = GROW[age] if age < len(GROW) else 99.0
        alpha = 255 if age < 92 else int(255 * (LEN - age) / 8)
        spr = _piece_rgba(piece, lv, gr)
        twin = None
        if spr is not None and getattr(lib, "hd", False):  # ★ HD: 같은 조각의 3× 쌍
            th = _piece_rgba(piece, lv, gr, HD_K)
            if th is not None:
                twin = (th[0], th[1], th[2], HD_K)
        ground.append([(lib.sprite(spr, twin), 0, 0, alpha, 0)] if spr is not None else [])
        t = []
        if 2 <= age < 84:
            for i, (fx, fy, full) in enumerate(flames):
                size = full if age < 50 else max(3, int(round(full - (age - 50) * full / 34)))
                if age < 7:
                    size = min(full, 3 + (age - 2) * 3)
                for (s, dx, dy, a, fl) in flame_parts(age + 3 * i + piece, size):
                    t.append((s, dx + fx, dy + fy, a, fl))
        if age % 4 == piece % 4 and 3 <= age < 90:
            ex, ey = embers[(age // 4) % 2]
            for (s, dx, dy, a, fl) in P["ember"]:
                t.append((s, dx + int(ex), dy + int(ey) - (age % 12), a, fl))
        if 52 <= age < 97 and (age + piece) % 10 < 5 and piece < len(RING):
            si = ((age + piece) // 3) % 4
            for (s, dx, dy, a, fl) in P["smoke"][si]:
                t.append((s, dx + 8, dy + 6 - ((age + piece) % 10), min(a, 150), fl))
        top.append(t)
    return ground, top


def add_lava_effects(lib, which):
    """`which` = "ground" 이면 lava0..15, "top" 이면 lava_top0..15 를 lib 에 (효과 순서는 demon_fx.EFFECTS)."""
    key = ("frames", id(lib))
    if key not in _C:
        _C[key] = [lava_effect(lib, k) for k in range(len(RING) + SPARE)]
    for k, (g, t) in enumerate(_C[key]):
        if which == "ground":
            lib.add_effect(f"lava{k}", g)
        else:
            lib.add_effect(f"lava_top{k}", t)


def replace(src, out):
    """지금 bin 의 다른 효과(스프라이트 바이트 그대로)는 두고 용암 32 효과만 새로 — 전체 재생성은 옛 스프라이트가 흔들린다
    (`ilseom_fx.extend` 와 같은 이유)."""
    import demon_fx
    sprites, effects = demon_fx.read_bin(src)
    assert [n for n, _ in effects] == [n for n, _ in demon_fx.EFFECTS], "bin 의 효과 목록이 EFFECTS 와 다르다"
    lib = demon_fx.Library()
    remap = {}

    def keep(parts):
        out_parts = []
        for (s, dx, dy, a, fl) in parts:
            if s not in remap:
                arr, ax, ay = sprites[s]
                remap[s] = lib.sprite((np.ascontiguousarray(arr), ax, ay))
            out_parts.append((remap[s], dx, dy, a, fl))
        return out_parts
    import lava_wide_fx
    wide = {n for n, _ in lava_wide_fx.effect_names()}
    for name, frames in effects:
        # ★★ 2026-10-18 목걸이의 2 배 지대는 새 조각에서 다시 (옛 조각의 사본을 가리키면 안 된다)
        if name in wide:
            if name == "lava_w0":
                lava_wide_fx.add_effects(lib)
            continue
        # add_lava_effects 는 16 개를 한꺼번에 넣는다 — 첫 이름(lava0 · lava_top0)에서만 부르고 나머지 15 는 건너뛴다
        if name.startswith("lava_top"):
            if name == "lava_top0":
                add_lava_effects(lib, "top")
            continue
        if name.startswith("lava") and name[4:].isdigit():
            if name == "lava0":
                add_lava_effects(lib, "ground")
            continue
        lib.add_effect(name, [keep(p) for p in frames])
    assert len(lib.effects) == len(demon_fx.EFFECTS), f"{len(lib.effects)} effects"
    size, shape, n = lib.write(out)
    print(f"{out}: {size:,} B · atlas {shape[1]}×{shape[0]} · {n} sprites · {len(lib.effects)} effects")


# ── 확인 그림 ──────────────────────────────────────────────────────────────────────────────────────────────
def field_png(out):
    col, alpha, red, plates, crack = field()
    P = pieces()
    H, W = alpha.shape
    z = 4
    rnd = random.Random(3)
    pal = [(rnd.randrange(60, 255), rnd.randrange(60, 255), rnd.randrange(60, 255)) for _ in range(len(RING))]
    a = np.asarray(_mock(), np.uint8)
    cut = np.dstack([np.clip(col, 0, 255), alpha * 255]).astype(np.uint8)
    over = np.zeros((H, W, 3), np.uint8) + 40
    over = (cut[..., :3] * (cut[..., 3:] / 255) + over * (1 - cut[..., 3:] / 255)).astype(np.uint8)
    seg = np.zeros((H, W, 3), np.uint8)
    for k in range(len(RING)):
        seg[P == k] = pal[k]
    seg = (seg * 0.6 + over * 0.4).astype(np.uint8)
    for k, (dx, dy) in enumerate(RING):
        x0, y0 = cell_tl(dx, dy)
        seg[y0:y0 + 16, x0] = 255
        seg[y0, x0:x0 + 16] = 255
    sheet = Image.new("RGB", (W * z * 3 + 16, H * z), (20, 20, 26))
    for i, im in enumerate((a, over, seg)):
        sheet.paste(Image.fromarray(im).resize((W * z, H * z), Image.NEAREST), (i * (W * z + 8), 0))
    sheet.save(out)
    print(f"field → {out}")


def preview(out, src=None):
    import demon_fx as d
    sprites, effects = d.read_bin(src or d.OUT_BIN)
    E = {n: i for i, (n, _) in enumerate(effects)}
    os.makedirs(out, exist_ok=True)
    mob = d.real_monster()
    FX, FY = 82, 126  # 악마 발 = 초안의 자리 (고리 가운데 칸 = 발 − (8, 10))
    ctx, cty = (FX - 8) // 16, (FY - 10) // 16
    ox, oy = FX - 8 - 16 * ctx, FY - 10 - 16 * cty  # 칸 격자를 초안 자리에 맞춘다 (미리보기만)
    shots = []

    def scene(title, age, ring=tuple(range(len(RING))), at=(0, 0)):
        c = d._background()
        tiles = [((ctx + RING[k][0] + at[0]), (cty + RING[k][1] + at[1]), k) for k in ring]
        for (tx, ty, k) in tiles:
            d.draw_effect(c, sprites, effects, E["lava0"] + k, age, tx * 16 + ox, ty * 16 + oy)
        items = [(FY, "demon", FX), (98, "mob", 24), (64, "mob", 140)]
        for (yy, kind, xx) in sorted(items):
            if kind == "mob":
                d._paste(c, mob[0], xx - mob[1][0], yy - mob[1][1])
            else:
                d._paste(c, d.demon("stand", d.RIGHT), xx + d.DEMON_OFF[0], yy + d.DEMON_OFF[1])
        for (tx, ty, k) in tiles:
            d.draw_effect(c, sprites, effects, E["lava_top0"] + k, age, tx * 16 + ox, ty * 16 + oy)
        shots.append((title, Image.fromarray(np.clip(c, 0, 255).astype(np.uint8))))

    shots.append(("초안 11 연소", d.mockup_panel(10).crop((0, 0, 176, 160))))
    scene("연소 (나이 30)", 30)
    shots.append(("초안 12 냉각", d.mockup_panel(11).crop((0, 0, 176, 160))))
    scene("냉각 (나이 85)", 85)
    for age in (0, 2, 4, 60, 72, 95):
        scene(f"나이 {age}", age)
    scene("왼쪽 넷 빠짐 (나이 30)", 30, ring=[k for k in range(len(RING)) if RING[k][0] >= 0])
    scene("두 번 겹침 (나이 30)", 30, ring=tuple(range(len(RING))))
    z = 3
    tw, th = 176 * z, 160 * z
    cols = 4
    rows = (len(shots) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (tw + 8) + 8, rows * (th + 26) + 8), (20, 20, 26))
    dr = ImageDraw.Draw(sheet)
    try:
        from PIL import ImageFont
        font = ImageFont.truetype(os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf"), 16)
    except Exception:
        font = None
    for i, (title, im) in enumerate(shots):
        x, y = 8 + (i % cols) * (tw + 8), 8 + (i // cols) * (th + 26)
        sheet.paste(im.resize((tw, th), Image.NEAREST), (x, y + 20))
        dr.text((x, y), title, fill=(230, 230, 230), font=font)
    sheet.save(os.path.join(out, "sheet.png"))
    print(f"preview → {out}/sheet.png")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["replace", "preview", "field"])
    ap.add_argument("--src", default=None)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    sys.stdout.reconfigure(errors="replace")
    import demon_fx as _d
    if args.cmd == "replace":
        replace(args.src or _d.OUT_BIN, args.out or _d.OUT_BIN)
    elif args.cmd == "field":
        field_png(args.out or os.path.join(ROOT, "docs", "skill", "demon_lava", "field.png"))
    else:
        preview(args.out or os.path.join(ROOT, "docs", "skill", "demon_lava"), args.src)
