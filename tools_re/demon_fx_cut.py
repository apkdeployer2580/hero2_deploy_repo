#!/usr/bin/env python3
"""「악마화」 이펙트 3 차 (2026-10-14 밤) — 예상도에서 오린 정점 프레임(`astra_cut.py` → `costumes/astra_fx/`)을 움직인다.

사용자: *"진짜 완전 똑같게 구현하고싶은거야"* → 결정 「A 먼저, B 오면 교체」. 초안에 있는 순간(할퀴기 세 단 · 돌진 · 추종 100 % ·
추종 폭발 · 지옥칼날 인장과 낫날 · 어둠심판 낙뢰 · 멸화 · 변신 출현)은 초안의 픽셀 그대로이고, 이 파일은 그 그림에 시간만 입힌다:
나타남(커짐 · 솟음 · 섬광) · 머묾(일렁임 · 빛이 도는 테 · 깜빡임) · 사라짐(알파 · 노이즈로 부서짐 · 조각이 튐). 초안에 없는 단계
(변신 앞부분 · 추종 게이지 · 어둠심판 선딜 · 멸화 선딜 · 몬스터 피격 · 용암)는 `demon_fx_art.py` 의 그림 그대로다.

B(연속 프레임 시트)가 오면 같은 이름의 오린 그림 대신 그 프레임을 쓴다 — 함수 이름 · 프레임 수는 `demon_fx.EFFECTS` 계약 그대로.
좌표 = 기준점(개체의 발바닥) 상대 게임 px. 오린 그림의 기준점 = 초안 속 악마 · 몬스터의 발바닥(`astra_cut.FEET`).
"""

import math
import random

import numpy as np

import astra_cut
import astra_frames
import demon_fx_art as art
import fxpaint as fp
from fxpaint import Canvas

UP, DOWN, LEFT, RIGHT = 0, 1, 2, 3
WHITE = (255, 255, 255)
CHEST = (0, -20)

# ★★ 2026-10-15 HD: 오린 그림을 예상도 해상도(3×, `astra_cut.load_hd`)로 싣는다 — 좌표는 여전히 게임 px(기준점 상대)이고 `Cut.s` 가
# 그림 px / 게임 px 다. 그림판(`fxpaint.SS`)이 3 이면 `put` 이 3× 그림을 그대로 붙이고 `Canvas.sprites_hd` 가 3× 스프라이트를 낸다.
# 0 = 1× 그대로 (옛 빌드), `demon_fx.build` 가 켠다.
HD = 0


# ── 오린 그림 ──────────────────────────────────────────────────────────────────────────────────────────────
class Cut:
    """RGBA(float, 알파 0..1, 곱하지 않은 색) + 기준점 (ax, ay) = 그림 왼쪽 위에서 발바닥까지(그림 px) + ★ HD: 배율 s = 그림 px / 게임 px.
    메서드의 좌표 · 거리 인수는 전부 게임 px 다."""

    def __init__(self, rgba, ax, ay, s=1):
        self.a = np.asarray(rgba, np.float32)
        self.ax, self.ay = float(ax), float(ay)
        self.s = int(s)

    @classmethod
    def load(cls, name):
        if HD > 1:
            hd = astra_cut.load_hd(name)
            if hd is not None and hd[2] == HD:
                arr, (ax, ay), s = hd
                a = arr.astype(np.float32)
                a[..., 3] /= 255.0
                return cls(a, ax, ay, s)
        arr, (ax, ay) = astra_cut.load(name)
        a = arr.astype(np.float32)
        a[..., 3] /= 255.0
        return cls(a, ax, ay)

    @property
    def h(self):
        return self.a.shape[0]

    @property
    def w(self):
        return self.a.shape[1]

    def copy(self):
        return Cut(self.a.copy(), self.ax, self.ay, self.s)

    def grid(self):
        """그림 픽셀 가운데의 기준점 상대 좌표 (X, Y) — 게임 px."""
        yy, xx = np.mgrid[0:self.h, 0:self.w].astype(np.float32)
        return (xx + 0.5 - self.ax) / self.s, (yy + 0.5 - self.ay) / self.s

    def flip(self):
        return Cut(self.a[:, ::-1].copy(), self.w - self.ax, self.ay, self.s)

    def rot(self, k, pivot=(0.0, 0.0)):
        """시계 방향 90°·k 회전 (화면 좌표 — y 아래) — pivot(기준점 상대, 게임 px) 둘레."""
        c = self
        px, py = pivot[0] * self.s, pivot[1] * self.s
        for _ in range(k % 4):
            h = c.h
            # (qx, qy) → pivot + (-(qy − py), qx − px) : 픽셀 (x, y) → (h−1−y, x)
            c = Cut(np.rot90(c.a, -1).copy(), h - c.ay - px - py, c.ax + px - py, c.s)
        return c

    def shift(self, dx, dy):
        return Cut(self.a, self.ax - dx * self.s, self.ay - dy * self.s, self.s)

    def mul_alpha(self, k):
        out = self.copy()
        out.a[..., 3] *= k
        return out

    def mask(self, m):
        out = self.copy()
        out.a[..., 3] *= np.clip(m, 0, 1)
        return out

    def brighten(self, k, col=WHITE):
        out = self.copy()
        out.a[..., :3] += (np.array(col, np.float32) - out.a[..., :3]) * k
        return out

    def tint_where(self, w, col, k):
        out = self.copy()
        out.a[..., :3] += (np.array(col, np.float32) - out.a[..., :3]) * (np.clip(w, 0, 1) * k)[..., None]
        return out

    def dissolve(self, t, seed=0, scale=0.22, edge=0.1, bias=None):
        """노이즈 순서로 부서진다 — t = 0 그대로 · 1 다 사라짐. 옅은 픽셀 · bias(0..1, 클수록 먼저)가 큰 곳부터."""
        if t <= 0:
            return self.copy()
        X, Y = self.grid()
        n = fp.fbm(X * scale + 40.3, Y * scale + 17.7, 300 + seed, 3)
        rnd = np.random.RandomState(900 + seed).uniform(0, 1, n.shape).astype(np.float32)
        n = n * 0.65 + rnd * 0.2 + self.a[..., 3] * 0.15
        if bias is not None:
            n = n * (1 - 0.5 * bias) + (1 - bias) * 0.25
        keep = np.clip((n - t * 1.05) / edge + 0.5, 0, 1)
        return self.mask(keep)

    def warp(self, A, t=(0.0, 0.0), pivot=(0.0, 0.0), blur=None):
        """앞 사상 q' = pivot + A·(q − pivot) + t (2×2 A, t · pivot 은 게임 px) — 겹선형 역사상, 줄일 때는 미리 흐린다 (곱한 색으로)."""
        A = np.asarray(A, np.float64)
        Ai = np.linalg.inv(A)
        s = self.s
        px, py = pivot[0] * s, pivot[1] * s
        t = (t[0] * s, t[1] * s)
        pre = np.dstack([self.a[..., :3] * self.a[..., 3:4], self.a[..., 3:4]])
        sx = math.sqrt(abs(np.linalg.det(A)))
        if blur is None:
            blur = max(0.0, (1.0 / max(sx, 1e-3) - 1.0) * 0.5) * s
        if blur > 0.15:
            r = max(1, int(round(blur)))
            pre = fp.box_blur(pre, r, passes=2) if blur >= 1 else pre * 0.6 + fp.box_blur(pre, 1, passes=1) * 0.4
        corners = np.array([[-self.ax, -self.ay], [self.w - self.ax, -self.ay], [-self.ax, self.h - self.ay], [self.w - self.ax, self.h - self.ay]])
        piv = np.array([px, py])
        out_c = (corners - piv) @ A.T + piv + np.array(t)
        x0, y0 = np.floor(out_c.min(axis=0)) - 1
        x1, y1 = np.ceil(out_c.max(axis=0)) + 1
        W, H = int(x1 - x0), int(y1 - y0)
        yy, xx = np.mgrid[0:H, 0:W].astype(np.float64)
        qx, qy = xx + 0.5 + x0, yy + 0.5 + y0
        vx, vy = qx - px - t[0], qy - py - t[1]
        sx_ = Ai[0, 0] * vx + Ai[0, 1] * vy + px
        sy_ = Ai[1, 0] * vx + Ai[1, 1] * vy + py
        fx, fy = sx_ + self.ax - 0.5, sy_ + self.ay - 0.5
        out = _bilinear(pre, fx, fy)
        a = np.clip(out[..., 3], 0, 1)
        rgb = out[..., :3] / np.maximum(a, 1e-4)[..., None]
        res = np.dstack([np.clip(rgb, 0, 255), a]).astype(np.float32)
        return Cut(res, -x0, -y0, s)

    def scale(self, s, sy=None, pivot=(0.0, 0.0), t=(0.0, 0.0)):
        sy = s if sy is None else sy
        if abs(s - 1) < 1e-3 and abs(sy - 1) < 1e-3 and t == (0.0, 0.0):
            return self.copy()
        return self.warp([[s, 0], [0, sy]], t, pivot)

    def wobble(self, ph, amp, freq=0.16, seed=0, ground=0.0, height=30.0):
        """불혀처럼 일렁인다 — 세로줄마다 위아래로 · 줄마다 옆으로 (발바닥 줄 ground 에서 0, 위로 갈수록 커진다)."""
        X, Y = self.grid()
        up = np.clip((ground - Y) / height, 0, 1)
        dy = (fp.fbm(X * freq + 3.1, np.full_like(X, ph * 0.35), 70 + seed, 2) - 0.5) * 2 * amp * up
        dx = (fp.fbm(np.full_like(Y, ph * 0.35) + 9.7, Y * freq * 0.8, 80 + seed, 2) - 0.5) * 1.6 * amp * up
        pre = np.dstack([self.a[..., :3] * self.a[..., 3:4], self.a[..., 3:4]])
        yy, xx = np.mgrid[0:self.h, 0:self.w].astype(np.float64)
        out = _bilinear(pre, xx + dx * self.s, yy + dy * self.s)
        a = np.clip(out[..., 3], 0, 1)
        rgb = out[..., :3] / np.maximum(a, 1e-4)[..., None]
        return Cut(np.dstack([np.clip(rgb, 0, 255), a]), self.ax, self.ay, self.s)

    def split(self, sel):
        """sel(bool) 픽셀 → (고른 것, 나머지)."""
        a, b = self.copy(), self.copy()
        a.a[..., 3] *= sel
        b.a[..., 3] *= ~sel
        return a, b


def _bilinear(img, fx, fy):
    H, W = img.shape[:2]
    x0 = np.floor(fx).astype(int)
    y0 = np.floor(fy).astype(int)
    tx = (fx - x0)[..., None].astype(np.float32)
    ty = (fy - y0)[..., None].astype(np.float32)
    pad = np.zeros((H + 2, W + 2, img.shape[2]), np.float32)
    pad[1:-1, 1:-1] = img

    def at(yi, xi):
        return pad[np.clip(yi + 1, 0, H + 1), np.clip(xi + 1, 0, W + 1)]
    return (at(y0, x0) * (1 - tx) * (1 - ty) + at(y0, x0 + 1) * tx * (1 - ty) + at(y0 + 1, x0) * (1 - tx) * ty
            + at(y0 + 1, x0 + 1) * tx * ty)


def put(c, cut, alpha=1.0, layer="base"):
    """오린 그림을 그림판에 — 기준점끼리 맞춘다 (정수 자리면 픽셀이 그대로 남는다). ★ HD: 그림판 SS 가 그림 배율의 배수여야
    한다 (3× 그림은 SS 3 그림판에 그대로, 1× 그림은 SS 배로 늘려)."""
    if cut is None or alpha <= 0:
        return
    s = c.ss
    assert s % cut.s == 0, f"그림판 SS {s} 와 그림 배율 {cut.s}"
    k = s // cut.s
    X0 = int(round(c.ax * s - cut.ax * k))
    Y0 = int(round(c.ay * s - cut.ay * k))
    up = cut.a.repeat(k, axis=0).repeat(k, axis=1) if k > 1 else cut.a
    h, w = up.shape[:2]
    sx0, sy0 = max(0, -X0), max(0, -Y0)
    X0c, Y0c = max(0, X0), max(0, Y0)
    X1, Y1 = min(c.w * s, X0 + w), min(c.h * s, Y0 + h)
    if X0c >= X1 or Y0c >= Y1:
        return
    sub = up[sy0:sy0 + (Y1 - Y0c), sx0:sx0 + (X1 - X0c)]
    if layer == "add":
        c.light(X0c, Y0c, sub[..., :3], sub[..., 3], alpha)
    else:
        c.over(X0c, Y0c, sub[..., :3], sub[..., 3], alpha)


def merge(dst, src):
    """그림판 src 를 dst 위에 덮는다 (기준점끼리, 같은 SS)."""
    s = dst.ss
    X0, Y0 = (dst.ax - src.ax) * s, (dst.ay - src.ay) * s
    h, w = src.base.shape[:2]
    x0, y0 = max(0, X0), max(0, Y0)
    x1, y1 = min(dst.w * s, X0 + w), min(dst.h * s, Y0 + h)
    if x0 >= x1 or y0 >= y1:
        return
    sb = src.base[y0 - Y0:y1 - Y0, x0 - X0:x1 - X0]
    db = dst.base[y0:y1, x0:x1]
    sa = sb[..., 3:4]
    db[..., :3] = sb[..., :3] + db[..., :3] * (1 - sa)
    db[..., 3:4] = sa + db[..., 3:4] * (1 - sa)
    dst.add[y0:y1, x0:x1] += src.add[y0 - Y0:y1 - Y0, x0 - X0:x1 - X0]


def canvas_for(cut, margin=24, extra=None):
    """오린 그림이 다 들어가는 그림판 (기준점 = 발바닥; margin 은 게임 px)."""
    x0, y0 = -cut.ax / cut.s - margin, -cut.ay / cut.s - margin
    x1, y1 = (cut.w - cut.ax) / cut.s + margin, (cut.h - cut.ay) / cut.s + margin
    if extra:
        x0, y0, x1, y1 = min(x0, extra[0]), min(y0, extra[1]), max(x1, extra[2]), max(y1, extra[3])
    x0, y0 = math.floor(x0), math.floor(y0)
    return Canvas(int(math.ceil(x1) - x0), int(math.ceil(y1) - y0), -x0, -y0)


def ease(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


_CACHE = {}


def cut(name):
    if name not in _CACHE:
        _CACHE[name] = Cut.load(name)
    return _CACHE[name]


def imported(name, age, length):
    """B: 들인 프레임 시트가 있으면 그 프레임(나이 age, 계약 길이 length)을 그린 그림판 — 없으면 None (A 그대로)."""
    fr = astra_frames.frame_at(name, age, length)
    if fr is None:
        return None
    arr, (ax, ay) = fr
    a = arr.astype(np.float32)
    a[..., 3] /= 255.0
    src = Cut(a, ax, ay)
    c = canvas_for(src, 4)
    put(c, src)
    return c


def _dark(c, lum=72, sat=0.55):
    """검은 조각 픽셀 (튀는 돌 · 결정) — 어둡고 채도가 낮다."""
    rgb = c.a[..., :3]
    mx, mn = rgb.max(axis=2), rgb.min(axis=2)
    return (rgb.mean(axis=2) < lum) & ((mx - mn) / (mx + 1) < sat) & (c.a[..., 3] > 0.5)


def _components(mask, max_px=60):
    """4-이웃 연결 성분 (작은 것만) → [(ys, xs)]."""
    H, W = mask.shape
    seen = np.zeros_like(mask)
    out = []
    for y0, x0 in zip(*np.nonzero(mask)):
        if seen[y0, x0]:
            continue
        stack = [(y0, x0)]
        seen[y0, x0] = True
        ys, xs = [], []
        while stack:
            y, x = stack.pop()
            ys.append(y)
            xs.append(x)
            for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                yy, xx = y + dy, x + dx
                if 0 <= yy < H and 0 <= xx < W and mask[yy, xx] and not seen[yy, xx]:
                    seen[yy, xx] = True
                    stack.append((yy, xx))
        if len(ys) <= max_px:
            out.append((np.array(ys), np.array(xs)))
    return out


def chips_of(c, near=1, lum=72, sat=0.55, max_px=60):
    """그림 → (조각 성분 목록 [(ys, xs)] (테 1 px 포함), 조각을 뺀 나머지). near · max_px 는 1× 기준 (★ HD: 배율로 늘린다)."""
    m = _dark(c, lum, sat)
    comps = _components(m, max_px * c.s * c.s)
    sel = np.zeros(m.shape, bool)
    grown = []
    for ys, xs in comps:
        k = np.zeros(m.shape, bool)
        k[ys, xs] = True
        for _ in range(near * c.s):
            p = np.pad(k, 1)
            k = k | p[:-2, 1:-1] | p[2:, 1:-1] | p[1:-1, :-2] | p[1:-1, 2:]
        k &= c.a[..., 3] > 0.05
        k &= ~sel
        if not k.any():  # swallowed by an earlier chip's ring (★ HD: the rings are wider)
            continue
        sel |= k
        yy, xx = np.nonzero(k)
        grown.append((yy, xx))
    rest = c.copy()
    rest.a[..., 3] *= ~sel
    return grown, rest


def fly_chips(c, comps, src, f, center, speed=0.1, up=1.2, grav=0.5, alpha=1.0):
    """조각 성분을 center(기준점 상대)에서 바깥으로 날린다 (프레임 f) — 새 그림판에 덮는다."""
    if f <= 0:
        for ys, xs in comps:
            piece = np.zeros_like(src.a)
            piece[ys, xs] = src.a[ys, xs]
            put(c, Cut(piece, src.ax, src.ay, src.s), alpha)
        return
    for k, (ys, xs) in enumerate(comps):
        if len(ys) == 0:
            continue
        cx, cy = (xs.mean() + 0.5 - src.ax) / src.s, (ys.mean() + 0.5 - src.ay) / src.s
        vx, vy = (cx - center[0]) * speed, (cy - center[1]) * speed * 0.8 - up
        dx = vx * f
        dy = vy * f + grav * f * f * 0.5
        y0, y1, x0, x1 = ys.min(), ys.max() + 1, xs.min(), xs.max() + 1
        piece = np.zeros((y1 - y0, x1 - x0, 4), np.float32)
        piece[ys - y0, xs - x0] = src.a[ys, xs]
        put(c, Cut(piece, src.ax - x0, src.ay - y0, src.s).shift(int(round(dx)), int(round(dy))), alpha)


# ── 할퀴기 ─────────────────────────────────────────────────────────────────────────────────────────────────
CLAW_LEN = 6  # = demon_fx.CLAW_LEN
CLAW_ALPHA = [1.0, 1.0, 0.96, 0.88, 0.72, 0.5]
CLAW_DIS = [0.0, 0.0, 0.06, 0.16, 0.28, 0.42]
CLAW_FLASH = [0.32, 0.0, 0.0, 0.0, 0.0, 0.0]
# ★★ 2026-10-15 (사용자: *"기본 핥퀴기 공격시에도 뭔가 이상한 이펙트들이 주변으로 나와. 간단하게 핥퀴는 이펙트와 흙먼지가 튀는 이펙트만
# 있으면 돼"*): 참격 = 초안의 X · 초승달 · 세 줄 **만** — 초안의 검은 조각 · 접촉 별 · 타격 섬광 · 빛무리는 뺀다; 그 아래 땅에서 흙먼지가
# 튄다. 초안의 참격은 악마보다 커서(X 는 앞 5 칸) 발톱의 사정거리로 줄인다 (⚠ Claude — 가슴 둘레로 줄이므로 손 쪽은 제자리):
CLAW_SCALE = {0: 0.5, 1: 0.5, 2: 0.55, 3: 0.6}
# 줄인 참격을 손 앞으로 (게임 px) — 가슴 둘레로 줄이면 X 가 팔 위에 앉는다
CLAW_AHEAD = {0: 6, 1: 6, 2: 3, 3: 3}


def _claw_base(k):
    """오른쪽을 보는 원본 — 0 = 첫 손(／) · 1 = X · 2 = 쓸어올리기 초승달 · 3 = 땅 가르기 — 검은 조각 · 별 없이, 줄여서.
    답 = (그림, 참격 가운데 (cx, cy) — 줄인 뒤의 게임 px)."""
    key = ("claw", k)
    if key in _CACHE:
        return _CACHE[key]
    if k in (0, 1):
        x = keep_large(cut("claw_x"))
        X, Y = x.grid()
        w = x.a[..., 3] * (x.a[..., 3] > 0.3)
        cx, cy = float((X * w).sum() / max(w.sum(), 1e-6)), float((Y * w).sum() / max(w.sum(), 1e-6))  # X 의 가운데 = 무게중심
        if k == 0:
            dx, dy = X - cx, Y - cy
            sel = (dx * dy < 0) | ((np.abs(dx) < 2.5) & (np.abs(dy) < 2.5))
            x = x.mask(sel.astype(np.float32))
        _comps, body = chips_of(x, max_px=130)
        res, centre = body, (cx, cy)
    else:
        c = cut("claw_crescent" if k == 2 else "claw_rend")
        if k == 3:  # 세 줄 + 흙먼지가 한 그림이다 — 흙먼지(밝은 갈색 · 회색, 채도 낮음)는 버리고 세 줄만 (흙먼지는 따로 튄다)
            rgb = c.a[..., :3]
            mx, mn = rgb.max(axis=2), rgb.min(axis=2)
            sat = (mx - mn) / (mx + 1)
            streak = (rgb[..., 0] > rgb[..., 2] + 60) & (sat > 0.45)
            c = c.mask(streak.astype(np.float32))
        X, Y = c.grid()
        _comps, body = chips_of(c, max_px=130)
        w = body.a[..., 3]
        res, centre = body, (float((X * w).sum() / max(w.sum(), 1e-6)), float((Y * w).sum() / max(w.sum(), 1e-6)))
    res = keep_large(res)  # 초안의 작은 반짝임 · 부스러기는 버린다
    s = CLAW_SCALE[k]
    if s != 1.0:
        res = res.scale(s, pivot=CHEST)
        centre = (CHEST[0] + (centre[0] - CHEST[0]) * s, CHEST[1] + (centre[1] - CHEST[1]) * s)
    if CLAW_AHEAD.get(k):
        res = res.shift(CLAW_AHEAD[k], 0)
        centre = (centre[0] + CLAW_AHEAD[k], centre[1])
    _CACHE[key] = (res, centre)
    return _CACHE[key]


def keep_large(c, frac=0.08, thresh=0.3):
    """알파 thresh 이상의 연결 성분 중 가장 큰 것의 frac 배보다 작은 것을 지운다 (★ 초안의 접촉 반짝임 · 부스러기)."""
    m = c.a[..., 3] >= thresh
    comps = _components(m, max_px=10 ** 9)
    if not comps:
        return c
    big = max(len(ys) for ys, _ in comps)
    out = c.copy()
    for ys, xs in comps:
        if len(ys) < big * frac:
            out.a[ys, xs, 3] = 0
    return out


def _claw_dir(k, d):
    key = ("clawd", k, d)
    if key in _CACHE:
        return _CACHE[key]
    base, (cx, cy) = _claw_base(k)
    if d in (RIGHT, LEFT):
        res = (base, (cx, cy))
    elif k == 3:  # 땅 가르기 — 세 줄은 늘 화면 아래로, 자리만 앞으로
        tx, ty = (-52, 26) if d == DOWN else (-52, -30)
        res = (base.shift(tx, ty), (cx + tx, cy + ty))
    else:
        piv = CHEST
        if d == DOWN:
            r = base.rot(1, piv)
            ncx, ncy = piv[0] - (cy - piv[1]), piv[1] + (cx - piv[0])
        else:
            r = base.rot(3, piv).shift(0, 12)
            ncx, ncy = piv[0] + (cy - piv[1]), piv[1] - (cx - piv[0]) + 12
        res = (r, (ncx, ncy))
    _CACHE[key] = res
    return res


def _claw_imported(k, d, f):
    """B: 오른쪽 할퀴기 프레임 시트(`claw<k>`)가 있으면 그 프레임을 방향대로 (위 · 아래 = 가슴 둘레로 돌림, 3 단 = 자리만 옮김)."""
    fr = astra_frames.frame_at(f"claw{k}", f, CLAW_LEN)
    if fr is None:
        return None
    arr, (ax, ay) = fr
    a = arr.astype(np.float32)
    a[..., 3] /= 255.0
    src = Cut(a, ax, ay)
    if d == DOWN:
        src = src.shift(-52, 26) if k == 3 else src.rot(1, CHEST)
    elif d == UP:
        src = src.shift(-52, -30) if k == 3 else src.rot(3, CHEST).shift(0, 12)
    c = canvas_for(src, 4)
    put(c, src, 0.9 if d == UP else 1.0)
    return c


def _claw_dust(c, k, d, f, alpha):
    """★ 참격 아래 땅에서 튀는 흙먼지 — 바라보는 쪽 앞 반 칸 ~ 한 칸 반, 낮게 넓게; 프레임마다 커지고 조금 오르며 옅어진다."""
    if f >= 5:
        return
    fx, fy = {RIGHT: (1.0, 0.0), LEFT: (-1.0, 0.0), DOWN: (0.0, 1.0), UP: (0.0, -1.0)}[d]
    lx, ly = -fy, fx  # 옆
    reach = (14, 14, 18, 22)[k]
    rnd = random.Random(4100 + k * 10 + d)
    for j in range(4):
        along = reach + (j - 1.5) * 4 + f * 2.2
        side = (j - 1.5) * 5 * (1.0 + f * 0.25)
        x = fx * along + lx * side * (0.6 if d in (UP, DOWN) else 1.0)
        y = fy * along * 0.6 + ly * side * (0.35 if d in (LEFT, RIGHT) else 0.5)
        rx, ry = 4.5 + f * 1.4 - abs(j - 1.5) * 0.6, 2.8 + f * 0.7
        c.blob(x, y - 1 - f * 0.9, rx, ry, fp.DUST, seed=90 + j * 3 + f, rough=0.45, alpha=0.8 * (1 - f / 5.5) * alpha, scale=0.5)


def claw_frame(k, d, f):
    """참격 한 컷 (6 그리기) — 프레임 0 = 타격 순간: 초안의 참격 그대로 → 노이즈로 부서지며 사라진다. 조각 · 섬광 · 별은 없다,
    땅의 흙먼지만 같이 튄다 (★ 2026-10-15)."""
    imp = _claw_imported(k, d if d != LEFT else RIGHT, f)
    if imp is not None:
        return imp
    src, _center = _claw_dir(k, d if d != LEFT else RIGHT)
    c = canvas_for(src, 30)
    body = src.dissolve(CLAW_DIS[f], seed=11 + k) if CLAW_DIS[f] > 0 else src
    k_up = 0.9 if d == UP else 1.0
    _claw_dust(c, k, d if d != LEFT else RIGHT, f, k_up)
    put(c, body, CLAW_ALPHA[f] * k_up)
    return c


# ── 돌진 ───────────────────────────────────────────────────────────────────────────────────────────────────
def charge_ground(d, f):
    """돌진 땅 층 (8 반복): 초안의 남청 잔상 · 붉은 속도선 — 먼 쪽부터 일렁이며 깜빡이고, 흙먼지 · 돌이 흘러간다."""
    if d in (UP, DOWN):
        return art.charge_ground(d, f)
    src = cut("charge_trail") if d == RIGHT else cut("charge_trail").flip()
    X, Y = src.grid()
    far = np.clip((-X if d == RIGHT else X) / 90.0, 0, 1)
    ph = f / 8 * 2 * math.pi
    body = src.dissolve(0.1 + 0.08 * math.sin(ph), seed=40 + f, bias=far)
    body = body.mask(1.0 - 0.25 * far * (0.5 + 0.5 * np.sin(ph + X * 0.15)))
    c = canvas_for(src, 20)
    bx, by = art.BACK[d]
    put(c, body.shift(bx * (f % 2), 0), 0.92 + 0.08 * math.cos(ph))
    for j in range(4):  # 흙먼지 — 발 뒤로 낮게, 프레임마다 흘러간다
        t = ((f + j * 2) % 8) / 8
        dist = 6 + t * 30
        c.blob(bx * dist, 1 - t * 2, 4.5 + t * 6, 2.6 + t * 2.4, fp.DUST, seed=50 + j * 7 + f, alpha=0.75 * (1 - t) ** 0.8, scale=0.5)
    rnd = random.Random(4000 + d * 10)
    for j in range(3):
        t = ((f + j * 3) % 8) / 8
        c.rock(bx * (8 + t * 26) + rnd.uniform(-4, 4), -2 - math.sin(t * math.pi) * 6, 2.0, 1.0 - t, rnd=random.Random(j),
               base=(46, 38, 36), hi=(100, 88, 80))
    return c


def charge_top(d, f):
    """돌진 위 층 (8 반복): 머리 위를 긋는 붉은 호와 앞쪽 충돌 섬광 — 네 프레임마다 번쩍인다."""
    if d in (UP, DOWN):
        return art.charge_top(d, f)
    src = cut("charge_front") if d == RIGHT else cut("charge_front").flip()
    k = f % 4
    a = (1.0, 0.86, 0.6, 0.42)[k]
    body = src.dissolve((0.0, 0.08, 0.22, 0.34)[k], seed=60 + f)
    if k == 0:
        body = body.brighten(0.25, (255, 230, 230))
    c = canvas_for(src, 16)
    put(c, body, a)
    if k == 0:
        c.glow(20 if d == RIGHT else -20, -14, 6, 6, (255, 120, 150), 0.5)
    return c


# ── 추종 ───────────────────────────────────────────────────────────────────────────────────────────────────
def follow_dome(f):
    """추종 100 % (16 반복): 초안의 보라 반구 — 빛 띠가 아래에서 위로 쓸고, 안의 검은 결정이 떠서 흔들리고, 테를 따라 불티가 오른다."""
    src = cut("dome")
    X, Y = src.grid()
    ph = f / 16
    yb = 8 - ph * 96  # 바닥 → 꼭대기
    band = np.exp(-((Y - yb) / 7.0) ** 2) * (src.a[..., 3] > 0.25)
    comps, body = chips_of(src, lum=60, sat=0.6, max_px=50)
    body = body.tint_where(band, (255, 214, 255), 0.42)
    pulse = 0.94 + 0.06 * math.sin(ph * 2 * math.pi)
    c = canvas_for(src, 12)
    put(c, body, pulse)
    for k, (ys, xs) in enumerate(comps):  # 결정이 위아래로 흔들린다
        piece = np.zeros_like(src.a)
        piece[ys, xs] = src.a[ys, xs]
        dy = int(round(1.4 * math.sin(ph * 2 * math.pi + k * 1.7)))
        put(c, Cut(piece, src.ax, src.ay, src.s).shift(0, dy), 1.0)
    for k in range(6):  # 테를 타고 오르는 불티
        t = (ph + k / 6) % 1.0
        a = math.pi * (0.1 + 0.8 * ((k * 0.37) % 1))
        c.ellipse(math.cos(a) * 46, -math.sin(a) * 66 - t * 6, 0.9, 0.9, (255, 220, 255), math.sin(math.pi * t), "add")
    return c


def burst(age):
    """추종 폭발 (20 그리기): 섬광 → 초안의 결정 가시 부채가 솟아 커지고(0..3) → 머물다(4..8) → 부서지며 바깥으로 흩어진다(9..19)."""
    src = cut("burst")
    piv = (0.0, -6.0)
    c = canvas_for(src, 30)
    if age < 4:
        s = (0.36, 0.64, 0.86, 1.0)[age]
        body = src.scale(s, s * 0.92 + 0.08, pivot=piv)
        if age >= 2:
            body = body.brighten((0.0, 0.0, 0.3, 0.16)[age], (255, 220, 255))
        put(c, body, (0.75, 0.9, 1.0, 1.0)[age])
    elif age < 9:
        put(c, src.brighten(0.08 if age == 4 else 0.0, (255, 220, 255)), 1.0)
    else:
        t = (age - 9) / 10
        comps, body = chips_of(src, lum=58, sat=0.6, max_px=90)
        body = body.scale(1.0 + 0.08 * t, pivot=piv).dissolve(0.15 + 0.85 * t, seed=72 + age)
        put(c, body, 1.0 - 0.6 * t)
        fly_chips(c, comps, src, int(round(t * 10)) + 1, (0.0, -24.0), speed=0.05, up=0.9, grav=0.35, alpha=1.0 - t)
    if age < 4:  # 섬광
        a = 1.0 - age / 4
        c.glow(0, -14, 14, 12, (255, 120, 220), 0.9 * a)
        c.star(0, -14, 8, 1.6, 20 - age * 3, (255, 150, 230), a, phase=0.2, squash=0.85, rnd=random.Random(1))
        c.star(0, -14, 4, 1.2, 12 - age * 2, WHITE, a, core=art.CREAM)
    if age >= 9:  # 보라 기운이 흩어진다
        t = age - 9
        a = 1.0 - max(0, t - 4) / 6
        for k in range(8):
            th = k * 2 * math.pi / 8 + t * 0.08
            r = 22 + t * 4
            c.blob(math.cos(th) * r, -6 + math.sin(th) * r * 0.45 - t * 1.5, 7, 4, fp.VIOLET, seed=k + t, rough=0.6, alpha=0.45 * a, scale=0.35)
    return c


# ── 지옥칼날 ───────────────────────────────────────────────────────────────────────────────────────────────
SEAL_C = (0.0, -4.0)
SEAL_R = (84.0, 23.0)


def seal(f):
    """지옥칼날 인장 (32 반복): 초안의 붉은 마법진 그대로 — 밝은 빛이 고리를 따라 한 바퀴 돌고, 전체가 숨쉬듯 밝아지며, 불티가 오른다."""
    src = cut("seal")
    X, Y = src.grid()
    u, v = (X - SEAL_C[0]) / SEAL_R[0], (Y - SEAL_C[1]) / SEAL_R[1]
    th = np.arctan2(v, u)
    th0 = f / 32 * 2 * math.pi
    dth = np.angle(np.exp(1j * (th - th0)))
    rgb = src.a[..., :3]
    red = (rgb[..., 0] > rgb[..., 1] + 50) & (src.a[..., 3] > 0.3)
    comet = np.exp(-(dth / 0.5) ** 2) + 0.5 * np.exp(-((dth + 0.7) / 0.6) ** 2)
    body = src.tint_where(comet * red, (255, 186, 150), 0.6)
    pulse = 0.5 + 0.5 * math.sin(f / 32 * 4 * math.pi)
    body = body.tint_where(red.astype(np.float32), (255, 80, 60), 0.12 * pulse)
    c = canvas_for(src, 20)
    put(c, body, 1.0)
    rnd = random.Random(8000)
    for k in range(12):  # 불티 (32 프레임에 한 번 떠오른다)
        t = (f / 32 + k / 12) % 1.0
        a = rnd.uniform(0, 6.28)
        rr = rnd.uniform(0.3, 0.95)
        x, y = math.cos(a) * SEAL_R[0] * rr, SEAL_C[1] + math.sin(a) * SEAL_R[1] * rr
        c.ellipse(x, y - t * 20, 0.8, 0.8, (255, 140, 70), math.sin(math.pi * t), "add")
    return c


BLADE_RISE = [0.25, 0.55, 0.82, 1.0]


def _imported_cut(name, age, length):
    fr = astra_frames.frame_at(name, age, length)
    if fr is None:
        return None
    arr, (ax, ay) = fr
    a = arr.astype(np.float32)
    a[..., 3] /= 255.0
    return Cut(a, ax, ay)


def blade_rise(variant, f):
    """몬스터 곁에서 솟는 낫날 (16 반복): 0..3 땅에서 솟음 · 4..9 머묾(테가 빛난다) · 10..13 가라앉음 · 14..15 남은 불씨.
    B: `blade_rise0` 시트가 있으면 그 프레임 (변형 1 = 거울)."""
    imp = _imported_cut("blade_rise0", f, 16)
    if imp is not None:
        imp = imp.flip() if variant == 1 else imp
        c = canvas_for(imp, 4)
        put(c, imp)
        return c
    src = cut("blade")
    if variant == 1:
        src = src.flip()
    X, Y = src.grid()
    base_sel = Y > -5  # 발치의 불 · 불씨
    blade, embers = src.split(~base_sel)
    if f < 4:
        p = BLADE_RISE[f]
    elif f < 10:
        p = 1.0
    elif f < 14:
        p = 1.0 - (f - 9) / 4.5
    else:
        p = 0.0
    H = float(-Y[src.a[..., 3] > 0.2].min()) if (src.a[..., 3] > 0.2).any() else 50.0
    c = canvas_for(src, 16)
    ea = min(1.0, (f + 1) / 2) if f < 10 else max(0.0, 1.0 - (f - 9) / 7)
    put(c, embers, ea)
    if p > 0:
        dy = int(round((1 - p) * H))
        b = blade.shift(0, dy)
        Xb, Yb = b.grid()
        b = b.mask((Yb < -4 + 1).astype(np.float32))  # 땅 밑은 안 보인다
        if 4 <= f < 10:
            b = b.brighten(0.12 * (0.5 + 0.5 * math.sin(f * 1.3)), (255, 210, 245))
        put(c, b, 1.0)
    if 1 <= f <= 3:  # 솟을 때 튀는 조각
        rnd = random.Random(8500 + variant * 50 + f)
        bx = 12 if variant == 0 else -12
        for k in range(5):
            th = rnd.uniform(math.pi * 1.1, math.pi * 1.9)
            r = 4 + f * 4
            c.shard(bx + math.cos(th) * r, -2 + math.sin(th) * r * 0.8, 2.2, th, 1.0, rim=(255, 60, 60))
    return c


# ── 어둠심판 ───────────────────────────────────────────────────────────────────────────────────────────────
BOLT_LEN = 4  # = demon_fx.BOLT_LEN


def _bolt_variant(variant, base=None):
    key = ("bolt", variant, id(base))
    if key in _CACHE:
        return _CACHE[key]
    src = base if base is not None else cut("bolt")
    if variant in (1, 3):
        src = src.flip()
    if variant in (2, 3):  # 줄기가 다른 모양으로 꺾인다 — 줄마다 옆으로 (발치는 그대로)
        X, Y = src.grid()
        sh = (fp.fbm(np.full_like(Y, 2.0 + variant), Y * 0.045, 520 + variant, 3) - 0.5) * 22 * np.clip(-Y / 40, 0, 1) * src.s
        pre = np.dstack([src.a[..., :3] * src.a[..., 3:4], src.a[..., 3:4]])
        yy, xx = np.mgrid[0:src.h, 0:src.w].astype(np.float64)
        out = _bilinear(pre, xx - sh, yy)
        a = np.clip(out[..., 3], 0, 1)
        src = Cut(np.dstack([np.clip(out[..., :3] / np.maximum(a, 1e-4)[..., None], 0, 255), a]), src.ax, src.ay, src.s)
    _CACHE[key] = src
    return src


def bolt(variant, age):
    """낙뢰 (4 그리기): 0 = 초안의 줄기 + 흰 섬광 · 1 = 그대로 · 2..3 = 위에서부터 끊기며 사라진다 (땅의 고리는 퍼진다).
    B: `bolt0` 시트가 있으면 그 프레임 (변형 1 · 3 = 거울, 2 · 3 = 줄기를 옆으로 흔든 것)."""
    imp = _imported_cut("bolt0", age, BOLT_LEN)
    if imp is not None:
        b = _bolt_variant(variant, imp) if variant else imp
        c = canvas_for(b, 4)
        put(c, b)
        return c
    src = _bolt_variant(variant)
    X, Y = src.grid()
    c = canvas_for(src, 20)
    top = np.clip(-Y / 130.0, 0, 1)
    if age == 0:
        put(c, src.brighten(0.3, (255, 236, 255)), 1.0)
        c.glow(0, -14, 7, 16, (255, 210, 255), 0.7)
        c.star(0, -5, 10, 1.8, 24, (255, 170, 250), 0.95, phase=0.3, squash=0.6, rnd=random.Random(9600 + variant))
    elif age == 1:
        put(c, src, 1.0)
        c.star(0, -5, 6, 1.4, 13, WHITE, 0.6, phase=0.0, squash=0.7, core=art.CREAM)
    else:
        t = (0.45, 0.8)[age - 2]
        put(c, src.dissolve(t, seed=90 + variant, bias=top), (0.8, 0.5)[age - 2])
    ra = (1.0, 0.85, 0.6, 0.32)[age]
    rx, ry = 14 + age * 5, 4.6 + age * 1.6
    if age >= 1:
        c.ellipse(0, 0, rx, ry, art.MAGENTA, ra, width=1.2)
        c.ellipse(0, 0, rx, ry, (255, 200, 255), 0.6 * ra, width=0.5)
        for k in range(3):  # 보라 연기
            c.blob((k - 1) * 9, -6 - age * 3, 6 + age, 4 + age * 0.6, fp.VIOLET, seed=k + 9, rough=0.6, alpha=0.35 * ra, scale=0.35)
    return c


# ── 멸화 ───────────────────────────────────────────────────────────────────────────────────────────────────
FIRE_GROW = [0.3, 0.55, 0.78, 0.94]


def _fire_frame(name, age):
    """멸화 분출의 한 그림 (앞 층 · 뒤 층 같은 규칙): 0..3 땅에서 솟아 커짐 · 4..11 일렁임 · 12..23 내려앉으며 부서짐."""
    src = cut(name)
    if age < 4:
        s = FIRE_GROW[age]
        b = src.scale(0.6 + 0.4 * s, s, pivot=(0.0, 4.0))
        return b.brighten((0.35, 0.25, 0.12, 0.05)[age], (255, 240, 200)), 1.0
    if age < 12:
        b = src.wobble(age * 1.7, 1.8, ground=4.0, height=60.0, seed=age % 3)
        sy = 1.0 + 0.03 * math.sin(age * 1.9)
        return b.scale(1.0, sy, pivot=(0.0, 4.0)), 1.0
    t = (age - 12) / 11
    b = src.wobble(age * 1.7, 1.5 + t, ground=4.0, height=60.0, seed=age % 3)
    b = b.scale(1.0 + 0.04 * t, 1.0 - 0.32 * t, pivot=(0.0, 4.0))
    X, Y = b.grid()
    hi = np.clip(-Y / 90.0, 0, 1)
    b = b.dissolve(0.1 + 0.85 * t, seed=33 + age, bias=hi)
    return b, 1.0 - 0.55 * t


def fire_blast(age):
    """멸화 폭발 (위 층, 24 그리기): 초안의 불 기둥 무리 (악마 자리는 비었다 — 뒤 층이 메운다) + 처음 두 그림의 흰 섬광."""
    body, a = _fire_frame("fire", age)
    c = canvas_for(cut("fire"), 24)
    put(c, body, a)
    if age < 3:
        k = 1.0 - age / 3
        c.glow(0, -16, 20, 14, (255, 210, 150), 0.55 * k)
        c.star(0, -16, 10, 1.8, 28 - age * 7, (255, 236, 200), k, phase=0.2, squash=0.7, rnd=random.Random(3), core=art.CREAM)
    return c


def fire_scorch(age):
    """멸화 땅 층 (64 틱): 처음 24 = 악마 뒤의 불 (폭발과 같은 규칙) · 그 밑은 2 차 그림의 그을음 · 식는 재 · 잔불."""
    c = art.fire_scorch(age, back=False)
    if age < 24:
        body, a = _fire_frame("fire_back", age)
        put(c, body, a)
    return c


# ── 변신 ───────────────────────────────────────────────────────────────────────────────────────────────────
REVEAL_AT = 24


def _reveal(name, age):
    src = cut(name)
    t = age - REVEAL_AT
    piv = (0.0, -16.0)
    if t < 3:
        s = (0.55, 0.8, 1.0)[t]
        return src.scale(s, pivot=piv).brighten((0.4, 0.25, 0.1)[t], (255, 220, 255)), 1.0
    if t < 8:
        return src, 1.0
    u = (t - 8) / 11
    return src.scale(1.0 + 0.1 * u, pivot=piv).dissolve(0.1 + 0.85 * u, seed=21 + t), 1.0 - 0.5 * u


def transform_top(age):
    """변신 (위 층, 44): 0..23 = 2 차 그림 (분홍 띠 · 그림자 · 모이는 조각 · 붉은 핵) · 24.. = 초안의 출현 (보라 결정 · 흙먼지) + 불티."""
    if age < REVEAL_AT:
        return art.transform_top(age)
    c = Canvas(170, 170, 85, 125)
    body, a = _reveal("transform_reveal", age)
    put(c, body, a)
    t = age - REVEAL_AT
    if t < 3:
        c.star(0, -18, 8, 1.4, 22 - t * 5, (255, 120, 220), 0.95 - t * 0.25, phase=0.2, squash=0.9, rnd=random.Random(5))
        c.glow(0, -18, 7, 9, art.CREAM, 0.6 - t * 0.2, "add")
    r3 = random.Random(5)
    for k in range(12):  # 불티
        x = r3.uniform(-28, 28) * (0.4 + t * 0.06)
        y = -r3.uniform(4, 36) + t * t * 0.1 - t * 1.2
        c.ellipse(x, y, 0.8, 0.8, (255, 150, 70) if k % 2 else art.PINK, 1.0 - max(0, age - 34) / 9, "add")
    return c


def transform_ground(age):
    """변신 땅 층 (44): 출현부터 악마 뒤의 기운(초안) + 2 차 그림의 흙먼지 고리 · 튀는 돌."""
    g = art.transform_ground(age)
    if age < REVEAL_AT:
        return g
    c = Canvas(170, 170, 85, 125)
    body, a = _reveal("transform_reveal_back", age)
    put(c, body, a * 0.85)
    merge(c, g)
    return c
