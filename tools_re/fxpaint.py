#!/usr/bin/env python3
"""「악마화」 이펙트의 그림판 (2026-10-14 2차) — 슈퍼샘플링 · 노이즈 · 색 띠 · 결정 조각.

1 차(`demon_fx.Frame`)는 게임 해상도(1×)에서 PIL 다각형을 바로 칠했다: 가장자리가 계단이고 모양이 전부 매끈한 도형이라
예상도(`docs/skill/astra_demon`)의 「손으로 칠한」 질감이 안 나왔다. 이 그림판은
* 모든 그림을 SS 배(기본 4) 해상도에서 합성하고 마지막에 면적 평균으로 줄인다 — 예상도를 176 폭으로 줄인 그림과 같은 성질
  (가장자리 반투명 · 가는 심이 1 px 로 남는다);
* 불 · 연기 · 기운은 도형이 아니라 **세기 필드**(모양 + fbm 노이즈)를 색 띠(`bands`)로 칠한다 — 바깥 띠가 어두운 테가 되어
  풀 위에서 윤곽이 읽힌다(예상도의 불혀가 그렇다);
* 검은 결정 · 바위는 면(밝은 면 · 어두운 면) + 빛 쪽 테로 그린다.

출력 형식은 1 차와 같다: `sprites()` = [(보통 RGBA, ax, ay) | None, (더하기 RGBA, ax, ay) | None] — `demon_fx.Library.parts_of`
가 그대로 받는다. 좌표는 전부 기준점(발바닥) 상대의 게임 px(float).
"""

import math
import random

import numpy as np
from PIL import Image, ImageDraw

SS = 4

# ★★ 2026-10-15 HD (사용자: *"이펙트와 외형 전부 업스케일링"*): 라이브러리가 1× 스프라이트마다 HD 쌍(예상도의 해상도 = 게임 3×)을 싣는다.
# 그림판의 내부 해상도(SS)를 3 으로 두면 `sprites()`(면적 평균 → 1×)와 `sprites_hd()`(SS 버퍼 그대로 = 3×)가 한 번의 그리기에서
# 나온다 — 오린 그림은 3× 원본을 그대로 붙이고(`demon_fx_cut.put`), 절차 그림은 SS 해상도로 그려진다. `set_ss(3)` 이 전환이다
# (SS 는 모듈 변수 — Canvas 는 만들 때 읽는다).
HD_SCALE = 3


def set_ss(n):
    global SS
    SS = int(n)


# ★★ 2026-10-17 (`bolt_fx.py` — 번개의 HD 쌍이 메모리 예산을 넘어서): 쌍을 SS 보다 성긴 배율로 — None 이면 SS 그대로(지금까지의 모든 효과).
# SS 의 약수면 `sprites_hd()` 가 SS 버퍼를 (SS / TWIN)² 블록 평균으로 줄인 쌍을 준다(가장자리가 반투명 = 안티앨리어싱된 쌍).
TWIN = None


def set_twin(n):
    global TWIN
    TWIN = None if n is None else int(n)


# ── 노이즈 ─────────────────────────────────────────────────────────────────────────────────────────────────
def _hash(ix, iy, seed):
    h = (ix * 374761393 + iy * 668265263 + seed * 1442695041) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    h = h ^ (h >> 16)
    return (h & 0xFFFFFF).astype(np.float32) / float(0xFFFFFF)


def vnoise(x, y, seed=0, period=None):
    """값 노이즈 0..1 (period = 격자 주기 — 이음매 없는 타일)."""
    x = np.asarray(x, np.float64)
    y = np.asarray(y, np.float64)
    xi = np.floor(x).astype(np.int64)
    yi = np.floor(y).astype(np.int64)
    xf = (x - xi).astype(np.float32)
    yf = (y - yi).astype(np.float32)
    u = xf * xf * (3 - 2 * xf)
    v = yf * yf * (3 - 2 * yf)
    x1, y1 = xi + 1, yi + 1
    if period:
        xi, yi, x1, y1 = xi % period, yi % period, x1 % period, y1 % period
    a = _hash(xi, yi, seed)
    b = _hash(x1, yi, seed)
    c = _hash(xi, y1, seed)
    d = _hash(x1, y1, seed)
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v


def fbm(x, y, seed=0, octaves=4, gain=0.5, period=None):
    """프랙털 값 노이즈 0..1 (평균 0.5)."""
    out = np.zeros(np.broadcast(np.asarray(x), np.asarray(y)).shape, np.float32)
    amp, norm, f = 1.0, 0.0, 1.0
    for o in range(octaves):
        p = int(period * f) if period else None
        out += amp * vnoise(np.asarray(x) * f, np.asarray(y) * f, seed + o * 101, p)
        norm += amp
        amp *= gain
        f *= 2.0
    return out / norm


def box_blur(a, r, axis_pair=(0, 1), passes=3):
    """가우스 근사 (상자 흐림 세 번) — r = SS px 반지름."""
    r = int(round(r))
    if r < 1:
        return a
    a = a.astype(np.float32)
    for _ in range(passes):
        for ax in axis_pair:
            pad = [(0, 0)] * a.ndim
            pad[ax] = (r + 1, r)
            p = np.pad(a, pad, mode="constant")
            c = np.cumsum(p, axis=ax)
            n = a.shape[ax]
            hi = np.take(c, np.arange(2 * r + 1, 2 * r + 1 + n), axis=ax)
            lo = np.take(c, np.arange(0, n), axis=ax)
            a = (hi - lo) / (2 * r + 1)
    return a


# ── 색 띠 ──────────────────────────────────────────────────────────────────────────────────────────────────
def bands(I, stops):
    """세기 → (rgb, alpha). stops = [(문턱, (r,g,b), alpha)] 오름차순 — 문턱을 넘은 가장 높은 띠의 색 (예상도의 단계 명암)."""
    rgb = np.zeros(I.shape + (3,), np.float32)
    a = np.zeros(I.shape, np.float32)
    for t, col, al in stops:
        m = I >= t
        rgb[m] = col
        a[m] = al
    return rgb, a


def smooth_bands(I, stops):
    """띠 사이를 짧게 섞는다 (문턱 ± 0.02) — 계단이 SS 뒤에도 남지 않게."""
    rgb = np.zeros(I.shape + (3,), np.float32)
    a = np.zeros(I.shape, np.float32)
    for t, col, al in stops:
        k = np.clip((I - t) / 0.04 + 0.5, 0, 1)[..., None]
        rgb = rgb * (1 - k) + np.array(col, np.float32) * k
        a = a * (1 - k[..., 0]) + al * k[..., 0]
    return rgb, a


FIRE = [(0.06, (64, 8, 16), 1.0), (0.17, (150, 18, 28), 1.0), (0.32, (224, 44, 28), 1.0), (0.5, (255, 112, 28), 1.0),
        (0.66, (255, 176, 52), 1.0), (0.8, (255, 228, 136), 1.0), (0.92, (255, 250, 226), 1.0)]
FIRE_DARK = [(0.06, (40, 6, 12), 1.0), (0.2, (112, 12, 24), 1.0), (0.38, (196, 30, 28), 1.0), (0.56, (250, 82, 26), 1.0),
             (0.72, (255, 150, 40), 1.0), (0.86, (255, 214, 110), 1.0)]
VIOLET = [(0.06, (34, 6, 52), 0.85), (0.2, (88, 22, 150), 0.95), (0.38, (150, 50, 220), 1.0), (0.56, (214, 70, 210), 1.0),
          (0.74, (255, 140, 236), 1.0), (0.9, (255, 232, 255), 1.0)]
CLAW = [(0.04, (54, 4, 16), 0.95), (0.16, (150, 10, 34), 1.0), (0.32, (226, 30, 40), 1.0), (0.52, (255, 86, 76), 1.0),
        (0.72, (255, 178, 160), 1.0), (0.88, (255, 248, 240), 1.0)]
GOLD = [(0.04, (70, 10, 10), 0.95), (0.16, (178, 40, 20), 1.0), (0.32, (242, 104, 28), 1.0), (0.5, (255, 168, 46), 1.0),
        (0.68, (255, 222, 110), 1.0), (0.86, (255, 250, 220), 1.0)]
CRIMSON = [(0.06, (40, 4, 14), 1.0), (0.2, (120, 10, 30), 1.0), (0.38, (206, 24, 44), 1.0), (0.58, (255, 70, 70), 1.0),
           (0.78, (255, 170, 160), 1.0), (0.92, (255, 244, 236), 1.0)]
SMOKE = [(0.08, (34, 28, 32), 0.55), (0.3, (52, 44, 46), 0.72), (0.55, (74, 64, 62), 0.82), (0.8, (100, 88, 82), 0.85)]
DUST = [(0.08, (112, 84, 54), 0.72), (0.3, (156, 122, 82), 0.9), (0.55, (196, 162, 116), 0.95), (0.8, (228, 204, 160), 1.0)]


# ── 그림판 ─────────────────────────────────────────────────────────────────────────────────────────────────
class Canvas:
    """기준점 (ax, ay) 둘레 w×h 게임 px 의 그림판 — 내부는 SS 배. base = 덮기(곱해진 알파), add = 더하기 빛."""

    def __init__(self, w, h, ax, ay, ss=None):
        ss = SS if ss is None else ss
        self.w, self.h, self.ax, self.ay, self.ss = w, h, ax, ay, ss
        self.base = np.zeros((h * ss, w * ss, 4), np.float32)
        self.add = np.zeros((h * ss, w * ss, 3), np.float32)

    # 좌표: 게임 px (기준점 상대) ↔ SS 픽셀
    def P(self, x, y):
        return (x + self.ax) * self.ss, (y + self.ay) * self.ss

    def window(self, x0, y0, x1, y1):
        """bbox(게임 px) → (sx0, sy0, X, Y) — X, Y = SS 픽셀 중심의 게임 좌표. 그림판 밖이면 None."""
        s = self.ss
        sx0 = max(int(math.floor((x0 + self.ax) * s)), 0)
        sy0 = max(int(math.floor((y0 + self.ay) * s)), 0)
        sx1 = min(int(math.ceil((x1 + self.ax) * s)) + 1, self.w * s)
        sy1 = min(int(math.ceil((y1 + self.ay) * s)) + 1, self.h * s)
        if sx0 >= sx1 or sy0 >= sy1:
            return None
        yy, xx = np.mgrid[sy0:sy1, sx0:sx1].astype(np.float32)
        return sx0, sy0, (xx + 0.5) / s - self.ax, (yy + 0.5) / s - self.ay

    def over(self, sx0, sy0, rgb, a, alpha=1.0):
        """(sx0, sy0) 에 rgb(H×W×3) · a(H×W) 를 덮는다."""
        h, w = a.shape
        a = np.clip(a * alpha, 0, 1)
        sl = self.base[sy0:sy0 + h, sx0:sx0 + w]
        rgb = np.broadcast_to(np.asarray(rgb, np.float32), a.shape + (3,))
        sl[..., :3] = rgb * a[..., None] + sl[..., :3] * (1 - a[..., None])
        sl[..., 3] = a + sl[..., 3] * (1 - a)

    def light(self, sx0, sy0, rgb, a, alpha=1.0):
        h, w = a.shape
        rgb = np.broadcast_to(np.asarray(rgb, np.float32), a.shape + (3,))
        self.add[sy0:sy0 + h, sx0:sx0 + w] += rgb * np.clip(a * alpha, 0, 4)[..., None]

    def put(self, sx0, sy0, rgb, a, alpha=1.0, layer="base"):
        (self.light if layer == "add" else self.over)(sx0, sy0, rgb, a, alpha)

    # ── 필드 칠하기 ──
    def field(self, bbox, fn, layer="base", alpha=1.0):
        """fn(X, Y) → (rgb, a) 를 bbox 에 칠한다."""
        win = self.window(*bbox)
        if win is None:
            return
        sx0, sy0, X, Y = win
        rgb, a = fn(X, Y)
        self.put(sx0, sy0, rgb, a, alpha, layer)

    # ── 도형 (SS 해상도 마스크) ──
    def _mask(self, bbox, draw_fn):
        s = self.ss
        x0, y0, x1, y1 = bbox
        sx0 = max(int(math.floor((x0 + self.ax) * s)) - s, 0)
        sy0 = max(int(math.floor((y0 + self.ay) * s)) - s, 0)
        sx1 = min(int(math.ceil((x1 + self.ax) * s)) + 2 * s, self.w * s)
        sy1 = min(int(math.ceil((y1 + self.ay) * s)) + 2 * s, self.h * s)
        if sx0 >= sx1 or sy0 >= sy1:
            return None
        im = Image.new("L", (sx1 - sx0, sy1 - sy0), 0)
        d = ImageDraw.Draw(im)
        tr = lambda x, y: ((x + self.ax) * s - sx0, (y + self.ay) * s - sy0)
        draw_fn(d, tr)
        return sx0, sy0, np.asarray(im, np.float32) / 255.0

    def poly(self, pts, color, alpha=1.0, layer="base"):
        if len(pts) < 3:
            return
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        mk = self._mask((min(xs), min(ys), max(xs), max(ys)), lambda d, tr: d.polygon([tr(x, y) for x, y in pts], fill=255))
        if mk:
            self.put(mk[0], mk[1], color, mk[2], alpha, layer)

    def ellipse(self, cx, cy, rx, ry, color, alpha=1.0, layer="base", width=0.0):
        def f(d, tr):
            (a, b), (c, e) = tr(cx - rx, cy - ry), tr(cx + rx, cy + ry)
            if width:
                d.ellipse([a, b, c, e], outline=255, width=max(1, int(round(width * self.ss))))
            else:
                d.ellipse([a, b, c, e], fill=255)
        mk = self._mask((cx - rx, cy - ry, cx + rx, cy + ry), f)
        if mk:
            self.put(mk[0], mk[1], color, mk[2], alpha, layer)

    def stroke(self, pts, width, color, alpha=1.0, layer="base"):
        """둥근 끝 꺾은선 (width = 게임 px)."""
        if len(pts) < 2:
            return
        s = self.ss
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        pad = width
        def f(d, tr):
            q = [tr(x, y) for x, y in pts]
            ww = max(1, int(round(width * s)))
            d.line(q, fill=255, width=ww, joint="curve")
            r = ww / 2
            for (x, y) in (q[0], q[-1]):
                d.ellipse([x - r, y - r, x + r, y + r], fill=255)
        mk = self._mask((min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad), f)
        if mk:
            self.put(mk[0], mk[1], color, mk[2], alpha, layer)

    def taper(self, pts, w0, w1, color, alpha=1.0, layer="base"):
        """꺾은선을 따라 굵기가 w0 → w1 로 변하는 띠 (붓질)."""
        if len(pts) < 2:
            return
        n = len(pts)
        left, right = [], []
        for i, (x, y) in enumerate(pts):
            a = pts[max(i - 1, 0)]
            b = pts[min(i + 1, n - 1)]
            dx, dy = b[0] - a[0], b[1] - a[1]
            L = math.hypot(dx, dy) or 1.0
            nx, ny = -dy / L, dx / L
            t = i / (n - 1)
            w = (w0 + (w1 - w0) * t) / 2
            left.append((x + nx * w, y + ny * w))
            right.append((x - nx * w, y - ny * w))
        self.poly(left + right[::-1], color, alpha, layer)

    def glow(self, cx, cy, rx, ry, color, alpha=1.0, layer="add"):
        """가우스 빛."""
        def f(X, Y):
            u = (X - cx) / max(rx, 0.2)
            v = (Y - cy) / max(ry, 0.2)
            return np.asarray(color, np.float32), np.exp(-(u * u + v * v) * 1.6)
        self.field((cx - rx * 2.4, cy - ry * 2.4, cx + rx * 2.4, cy + ry * 2.4), f, layer, alpha)

    def star(self, cx, cy, n, r_in, r_out, color, alpha=1.0, layer="add", phase=0.0, squash=1.0, rnd=None, core=None):
        """빛살 별 — 가는 살 n 개 (rnd 면 길이가 들쭉날쭉)."""
        pts = []
        for i in range(n * 2):
            a = phase + math.pi * i / n
            r = r_out if i % 2 == 0 else r_in
            if rnd is not None and i % 2 == 0:
                r *= 0.6 + 0.7 * rnd.random()
            pts.append((cx + math.cos(a) * r, cy + math.sin(a) * r * squash))
        self.poly(pts, color, alpha, layer)
        if core:
            self.ellipse(cx, cy, r_in * 1.3, r_in * 1.3 * squash, core, alpha, layer)

    # ── 재료 ──
    def shard(self, cx, cy, size, angle, alpha=1.0, rim=(236, 64, 170), rim_a=1.0, fill=(16, 10, 22), facet=(46, 38, 60),
              light=(-0.6, -0.8), stretch=1.0):
        """검은 결정: 긴 마름모 넷 꼭짓점 · 빛 쪽 면이 밝고 · 빛 쪽 두 변에 빛나는 테 · 바깥에 아주 어두운 윤곽."""
        L, W = size * stretch, size * 0.62
        ca, sa = math.cos(angle), math.sin(angle)
        loc = [(0, -L), (W, -L * 0.2), (W * 0.15, L * 0.8), (-W * 0.85, L * 0.05)]
        pts = [(cx + x * ca - y * sa, cy + x * sa + y * ca) for x, y in loc]
        ctr = (cx + (loc[0][0] + loc[2][0]) / 2 * ca - (loc[0][1] + loc[2][1]) / 2 * sa,
               cy + (loc[0][0] + loc[2][0]) / 2 * sa + (loc[0][1] + loc[2][1]) / 2 * ca)
        out = [(ctr[0] + (x - ctr[0]) * 1.18, ctr[1] + (y - ctr[1]) * 1.18) for x, y in pts]
        self.poly(out, (8, 4, 10), alpha * 0.9)
        self.poly(pts, fill, alpha)
        # 빛을 받는 면: 빛 방향에 가까운 두 꼭짓점 쪽 삼각형
        lx, ly = light
        score = [(x - ctr[0]) * lx + (y - ctr[1]) * ly for x, y in pts]
        k = int(np.argmax(score))
        tri = [pts[k], pts[(k + 1) % 4], ctr]
        tri2 = [pts[k], pts[(k - 1) % 4], ctr]
        self.poly(tri, facet, alpha)
        self.poly(tri2, tuple(int(c * 0.7) for c in facet), alpha)
        if rim is not None:
            self.stroke([pts[(k - 1) % 4], pts[k], pts[(k + 1) % 4]], max(0.7, size * 0.17), rim, alpha * rim_a)
            self.stroke([pts[(k - 1) % 4], pts[k], pts[(k + 1) % 4]], max(1.2, size * 0.32), rim, alpha * rim_a * 0.3, "add")

    def spike(self, bx, by, tx, ty, w, alpha=1.0, rim=(236, 64, 170), fill=(14, 8, 20), facet=(58, 44, 74), tip_glow=None):
        """검은 결정 가시: 밑 (bx, by) → 끝 (tx, ty) · 밑폭 w. 두 면(밝은 면 · 어두운 면) + 가운데 능선 + 빛 쪽 테 + 아주 어두운 윤곽."""
        dx, dy = tx - bx, ty - by
        L = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / L, dx / L
        b0 = (bx + nx * w / 2, by + ny * w / 2)
        b1 = (bx - nx * w / 2, by - ny * w / 2)
        mid = (bx + dx * 0.35, by + dy * 0.35)
        ol = [(b0[0] + nx * 0.8, b0[1] + ny * 0.8), (tx + dx / L * 1.2, ty + dy / L * 1.2), (b1[0] - nx * 0.8, b1[1] - ny * 0.8)]
        self.poly(ol, (6, 2, 8), alpha * 0.9)
        self.poly([b0, (tx, ty), b1], fill, alpha)
        lit_left = (nx * -0.6 + ny * -0.8) > 0  # 빛 = 왼쪽 위
        lit = [b0, (tx, ty), mid] if lit_left else [b1, (tx, ty), mid]
        self.poly(lit, facet, alpha)
        edge = [b0, (tx, ty)] if lit_left else [b1, (tx, ty)]
        if rim is not None:
            self.stroke(edge, max(0.7, min(1.3, w * 0.1)), rim, alpha)
            self.stroke(edge, max(1.4, min(2.6, w * 0.22)), rim, alpha * 0.18, "add")
            dark = [b1, (tx, ty)] if lit_left else [b0, (tx, ty)]
            self.stroke(dark, 0.6, tuple(int(v * 0.45) for v in rim), alpha * 0.8)
        if tip_glow is not None:
            self.glow(tx, ty, 2.2, 2.2, tip_glow, alpha * 0.8)

    def rock(self, cx, cy, size, alpha=1.0, rnd=None, hot=None, base=(46, 40, 44), hi=(96, 86, 88)):
        """돌: 들쭉날쭉한 육각 · 윗면 밝게 · 어두운 윤곽 · hot = 아래쪽 가장자리의 달군 테 색."""
        rnd = rnd or random.Random(int(cx * 31 + cy * 17))
        pts = []
        for i in range(7):
            a = i * 2 * math.pi / 7 + rnd.uniform(-0.25, 0.25)
            r = size * rnd.uniform(0.72, 1.08)
            pts.append((cx + math.cos(a) * r, cy + math.sin(a) * r * 0.82))
        out = [(cx + (x - cx) * 1.2, cy + (y - cy) * 1.2) for x, y in pts]
        self.poly(out, (10, 8, 10), alpha * 0.9)
        self.poly(pts, base, alpha)
        top = [p for p in pts if p[1] <= cy + size * 0.1]
        if len(top) >= 2:
            self.poly(top + [(cx, cy + size * 0.05)], hi, alpha)
        if hot is not None:  # 달군 테 — 가장자리 전체를 얇게, 아래쪽은 조금 굵게
            self.stroke(pts + pts[:1], max(0.4, size * 0.08), tuple(int(v * 0.55) for v in hot), alpha * 0.6)
            bot = sorted(pts, key=lambda p: -p[1])[:3]
            bot.sort(key=lambda p: p[0])
            self.stroke(bot, max(0.5, size * 0.16), hot, alpha)
            self.stroke(bot, max(0.8, size * 0.4), hot, alpha * 0.3, "add")

    def blob(self, cx, cy, rx, ry, stops, seed=0, rough=0.35, alpha=1.0, light=(-0.5, -0.7), layer="base", scale=0.35):
        """노이즈 덩어리 (연기 · 먼지 · 기운): 둥근 세기 + fbm · 빛 쪽이 밝다 → 색 띠."""
        def f(X, Y):
            u = (X - cx) / rx
            v = (Y - cy) / ry
            d = np.sqrt(u * u + v * v)
            n = fbm(X * scale + seed * 7.1, Y * scale - seed * 3.3, seed, 4)
            I = (1 - d) + (n - 0.5) * rough * 2
            I = I + (-(u * light[0] + v * light[1])) * 0.18
            return bands(I, stops)
        self.field((cx - rx * 1.3, cy - ry * 1.3, cx + rx * 1.3, cy + ry * 1.3), f, layer, alpha)

    @staticmethod
    def flame_I(X, Y, x, y, h, w, lean=0.0, t=0.0, seed=0, rough=0.5, warp=1.0, ang=None):
        """불혀 하나의 세기 필드 (밑 (x, y) · 높이 h · 밑폭 w). 노이즈가 모양을 깎고(rough) 위로 갈수록 옆으로 휜다(warp).
        ang = 불혀가 뻗는 방향(라디안, 기본 = 위) — 돌진의 갈기처럼 옆으로 흐르는 불."""
        if ang is not None:
            ux, uy = math.cos(ang), math.sin(ang)
            dx, dy = X - x, Y - y
            along = dx * ux + dy * uy
            across = -dx * uy + dy * ux
            X, Y = x + across, y - along
        v = (y - Y) / max(h, 0.5)
        vc = np.clip(v, 0, 1)
        wn = fbm(X * 0.22 + seed * 3.7, Y * 0.16 + t * 1.1 + seed, seed + 7, 3) - 0.5
        cxv = x + lean * vc ** 1.5 + wn * w * 1.6 * warp * vc
        hw = w / 2 * np.power(np.clip(1 - vc, 0, 1), 0.5) + 0.3
        hf = fbm(X * 0.9 + seed * 1.3, Y * 0.45 + t * 2.6, seed + 13, 2) - 0.5  # 띠 경계를 흔드는 잔물결
        u = (X - cxv + hf * w * 0.5 * warp) / hw
        core = (1 - np.clip(np.abs(u), 0, 2) ** 1.3) * (1 - vc) ** 0.9
        under = np.clip(1 + v * 3.0, 0, 1)
        n = fbm(X * 0.6 + seed * 5.3, Y * 0.32 + t * 1.9 + seed * 2.1, seed, 4)
        # 끝으로 갈수록 노이즈가 더 깎는다 → 끝이 여러 갈래로 찢어진다
        I = core * under * ((1 - rough) + 2 * rough * n) - (0.55 - n) * rough * 1.1 * vc
        I = np.where(core > 0, I, -1)
        return np.where((v > 1.2) | (v < -0.5), -1, I)

    def flame(self, x, y, h, w, lean=0.0, t=0.0, seed=0, stops=FIRE, alpha=1.0, rough=0.5, warp=1.0, layer="base"):
        """불혀 하나 → 색 띠."""
        if h < 0.8:
            return
        self.flames([(x, y, h, w, lean, seed)], t, stops, alpha, rough, warp, layer)

    def flames(self, tongues, t=0.0, stops=FIRE, alpha=1.0, rough=0.5, warp=1.0, layer="base", boost=None):
        """불혀 여럿을 한 덩어리로 (세기의 최대 → 띠 한 번 — 겹친 곳에 안쪽 테가 안 생긴다). tongues = [(x, y, h, w, lean, seed[, ang])].
        boost = fn(X, Y) → 더할 세기 (가운데 뜨거운 핵 등). 불혀마다 제 영역만 계산한다."""
        tongues = [tuple(tg) + (None,) * (7 - len(tg)) for tg in tongues if tg[2] >= 0.8]
        if not tongues:
            return

        def box(tg):  # 밑과 끝을 감싸는 상자 (불혀가 뻗는 쪽으로만)
            x, y, h, w, lean, seed, ang = tg
            a = -math.pi / 2 if ang is None else ang
            tx, ty = x + math.cos(a) * h * 1.25 + lean, y + math.sin(a) * h * 1.25
            m = w + abs(lean) * 0.3 + h * 0.25 + 3
            return min(x, tx) - m, min(y, ty) - m, max(x, tx) + m, max(y, ty) + m
        boxes = [box(tg) for tg in tongues]
        x0 = min(b[0] for b in boxes)
        y0 = min(b[1] for b in boxes)
        x1 = max(b[2] for b in boxes)
        y1 = max(b[3] for b in boxes)
        win = self.window(x0, y0, x1, y1)
        if win is None:
            return
        sx0, sy0, X, Y = win
        s = self.ss
        I = np.full(X.shape, -1.0, np.float32)
        H, W = X.shape
        for (x, y, h, w, lean, seed, ang), (bx0, by0, bx1, by1) in zip(tongues, boxes):
            j0 = max(int(math.floor((bx0 + self.ax) * s)) - sx0, 0)
            j1 = min(int(math.ceil((bx1 + self.ax) * s)) - sx0 + 1, W)
            i0 = max(int(math.floor((by0 + self.ay) * s)) - sy0, 0)
            i1 = min(int(math.ceil((by1 + self.ay) * s)) - sy0 + 1, H)
            if j0 >= j1 or i0 >= i1:
                continue
            sub = self.flame_I(X[i0:i1, j0:j1], Y[i0:i1, j0:j1], x, y, h, w, lean, t, seed, rough, warp, ang)
            I[i0:i1, j0:j1] = np.maximum(I[i0:i1, j0:j1], sub)
        if boost is not None:
            I = I + boost(X, Y)
        rgb, a = bands(I, stops)
        self.put(sx0, sy0, rgb, a, alpha, layer)

    def crescent(self, cx, cy, R, thick, a0, a1, stops, ry=None, seed=0, rough=0.18, alpha=1.0, layer="base", bias=0.22,
                 fade=1.0, erode=0.0, reveal=1.0, taper=0.75, power=1.0, soft=0.0, stripes=0.0, nstripes=3, glow=None):
        """초승달 참격: 원 (cx, cy, R) 의 호 a0 → a1 을 따라 가운데 굵기 thick · 끝 뾰족 · 볼록한 바깥 변 쪽(bias)에 가장 밝은 심.
        fade = 세기 배율(낮으면 어두운 띠로 식는다) · erode = 노이즈로 부서짐(0..1) · reveal = a0 쪽부터 그어진 비율."""
        ry = R if ry is None else ry
        span = a1 - a0

        def f(X, Y):
            lx, ly = X - cx, (Y - cy) * (R / ry)
            r = np.sqrt(lx * lx + ly * ly)
            th = np.arctan2(ly, lx)
            th = a0 + np.mod(th - a0 + np.pi, 2 * np.pi) - np.pi
            t = (th - a0) / span
            inside = (t > 0) & (t < reveal)
            tt = np.clip(t, 0, 1)
            sn = np.clip(np.sin(np.pi * tt), 0, 1)
            wv = thick * np.power(sn, taper)
            d = (R - r) / np.maximum(wv, 1e-3)
            prof = 1 - np.abs(d - bias) / np.where(d < bias, bias + 1e-3, 1 - bias)
            prof = np.where(prof > 0, np.power(np.clip(prof, 0, 1), power), prof)
            n = fbm(X * 0.55 + seed, Y * 0.55 - seed, seed, 3)
            I = prof * np.power(sn, 0.3) * fade + (n - 0.5) * rough * 2
            if stripes:  # 호를 따라 나란한 빛줄 (휘두른 속도선)
                I = I + stripes * np.cos(2 * np.pi * np.clip(d, 0, 1) * nstripes) * (prof > 0.1)
            if erode > 0:
                n2 = fbm(X * 0.9 - seed * 3, Y * 0.9 + seed * 2, seed + 50, 3)
                I = I - erode * (1.15 - n2) * 1.3
            if reveal < 1:  # 긋는 머리: 앞끝이 밝다
                head = np.clip(1 - (reveal - t) / 0.12, 0, 1)
                I = I + head * 0.25 * (prof > 0)
            if glow is not None:  # 빛 무리: 띠 대신 가운데서 멀어질수록 옅게
                g = np.clip(prof, 0, 1) ** 1.6 * np.power(sn, 0.6) * inside
                return np.asarray(glow, np.float32), g
            I = np.where(inside & (d > -0.5) & (d < 1.5), I, -1)
            rgb, a = bands(I, stops)
            if soft > 0:  # 안쪽(오목) 변은 테 없이 흐려진다 — 휘두른 잔상
                edge = bias + (1 - bias) * 0.55
                fall = np.clip((1.08 - d) / (1.08 - edge), 0, 1) ** (soft * 1.5)
                a = a * np.where(d > edge, fall, 1.0)
            return rgb, a
        ext = max(R, ry) + thick + 2
        self.field((cx - ext, cy - ext, cx + ext, cy + ext), f, layer, alpha)

    def slash(self, mx, my, chord, bulge, half, R, thick, stops, **kw):
        """현(chord) 가운데 (mx, my) · 현 방향 각 chord · 볼록한 쪽 단위 벡터 bulge(라디안 각) · 반각 half 의 초승달."""
        bx, by = math.cos(bulge), math.sin(bulge)
        cx, cy = mx - bx * R * math.cos(half), my - by * R * math.cos(half)
        # 호의 각: 현의 두 끝을 원 중심에서 본 각 — a0 이 현 방향의 뒤쪽 끝
        ex0, ey0 = mx - math.cos(chord) * R * math.sin(half), my - math.sin(chord) * R * math.sin(half)
        ex1, ey1 = mx + math.cos(chord) * R * math.sin(half), my + math.sin(chord) * R * math.sin(half)
        a0 = math.atan2(ey0 - cy, ex0 - cx)
        a1 = math.atan2(ey1 - cy, ex1 - cx)
        mid = math.atan2(by, bx)
        # a0 → mid → a1 순서가 되게 감는다
        d0 = (a0 - mid + math.pi) % (2 * math.pi) - math.pi
        d1 = (a1 - mid + math.pi) % (2 * math.pi) - math.pi
        self.crescent(cx, cy, R, thick, mid + d0, mid + d1, stops, **kw)
        return cx, cy

    def bolt_pts(self, x0, y0, x1, y1, segs, jitter, rnd):
        """중점 변위 번개 꺾은선."""
        pts = [(x0, y0), (x1, y1)]
        for _ in range(segs):
            out = [pts[0]]
            for (ax_, ay_), (bx_, by_) in zip(pts, pts[1:]):
                mx, my = (ax_ + bx_) / 2, (ay_ + by_) / 2
                L = math.hypot(bx_ - ax_, by_ - ay_)
                nx, ny = -(by_ - ay_) / (L or 1), (bx_ - ax_) / (L or 1)
                off = rnd.uniform(-1, 1) * jitter * L / 2
                out += [(mx + nx * off, my + ny * off), (bx_, by_)]
            pts = out
        return pts

    def lightning(self, pts, width, outer, mid, core=(255, 255, 255), alpha=1.0, glow=None):
        """번개: 바깥 빛(더하기) · 몸 · 흰 심."""
        if glow is not None:
            self.stroke(pts, width * 3.2, glow, alpha * 0.35, "add")
        self.stroke(pts, width * 1.6, outer, alpha)
        self.stroke(pts, width, mid, alpha)
        self.stroke(pts, max(0.5, width * 0.4), core, alpha)

    def sprite_tint(self, img, x, y, alpha, tint, k, scale=1):
        """RGBA 그림(PIL, `scale` px / 게임 px)을 (x, y) — 기준점 상대 왼쪽 위 — 에 SS 로 늘려 덮는다. tint 쪽으로 k 만큼 물들인다 (잔상)."""
        a = np.asarray(img.convert("RGBA"), np.float32)
        s = self.ss
        assert s % scale == 0, (s, scale)
        if s // scale > 1:
            a = a.repeat(s // scale, axis=0).repeat(s // scale, axis=1)
        h, w = a.shape[:2]
        X0, Y0 = int(round((x + self.ax) * s)), int(round((y + self.ay) * s))
        sx0, sy0 = max(0, -X0), max(0, -Y0)
        X0c, Y0c = max(0, X0), max(0, Y0)
        X1, Y1 = min(self.w * s, X0 + w), min(self.h * s, Y0 + h)
        if X0c >= X1 or Y0c >= Y1:
            return
        sub = a[sy0:sy0 + (Y1 - Y0c), sx0:sx0 + (X1 - X0c)]
        lum = sub[..., :3].mean(axis=2, keepdims=True) / 255.0
        rgb = sub[..., :3] * (1 - k) + np.array(tint, np.float32) * (0.5 + lum) * k
        self.over(X0c, Y0c, rgb, sub[..., 3] / 255.0, alpha)

    # ── 출력 ──
    def sprites(self):
        s = self.ss
        h, w = self.h, self.w
        base = self.base.reshape(h, s, w, s, 4).mean(axis=(1, 3))
        a = base[..., 3]
        rgb = base[..., :3] / np.maximum(a, 1e-6)[..., None]
        a = np.where(a < 4 / 255, 0, a)
        a = np.where(a > 0.94, 1.0, a)
        out = [_crop(np.dstack([np.clip(rgb, 0, 255), a * 255]).astype(np.uint8), self.ax, self.ay)]
        c = np.clip(self.add.reshape(h, s, w, s, 3).mean(axis=(1, 3)), 0, 255)
        m = c.max(axis=2)
        m = np.where(m < 4, 0, m)
        rgb2 = np.where(m[..., None] > 0, c * 255.0 / np.maximum(m, 1e-6)[..., None], 0)
        out.append(_crop(np.dstack([np.clip(rgb2, 0, 255), m]).astype(np.uint8), self.ax, self.ay))
        return out

    def sprites_hd(self):
        """★ HD: SS 해상도 그대로 — [(RGBA, ax·ss, ay·ss, ss) | None] × 2 (덮기 · 더하기). `sprites()` 의 쌍 — 같은 그림판의 같은 픽셀을
        1× 로 줄이기 전의 것이라 기준점이 정확히 ss 배다. 1× 쪽이 비면(None) 이쪽도 None 이어야 하므로 같은 문턱으로 자른다."""
        s = self.ss
        base, addl = self.base, self.add
        if TWIN and TWIN < s and s % TWIN == 0:  # ★★ 2026-10-17 a coarser twin (`set_twin`)
            f = s // TWIN
            base = base.reshape(self.h * TWIN, f, self.w * TWIN, f, 4).mean(axis=(1, 3))
            addl = addl.reshape(self.h * TWIN, f, self.w * TWIN, f, 3).mean(axis=(1, 3))
            s = TWIN
        a = base[..., 3]
        rgb = base[..., :3] / np.maximum(a, 1e-6)[..., None]
        a = np.where(a < 4 / 255, 0, a)
        a = np.where(a > 0.94, 1.0, a)
        out = []
        cr = _crop(np.dstack([np.clip(rgb, 0, 255), a * 255]).astype(np.uint8), self.ax * s, self.ay * s)
        out.append(None if cr is None else (cr[0], cr[1], cr[2], s))
        c = np.clip(addl, 0, 255)
        m = c.max(axis=2)
        m = np.where(m < 4, 0, m)
        rgb2 = np.where(m[..., None] > 0, c * 255.0 / np.maximum(m, 1e-6)[..., None], 0)
        cr = _crop(np.dstack([np.clip(rgb2, 0, 255), m]).astype(np.uint8), self.ax * s, self.ay * s)
        out.append(None if cr is None else (cr[0], cr[1], cr[2], s))
        return out


def _crop(arr, ax, ay):
    alpha = arr[..., 3]
    ys, xs = np.nonzero(alpha)
    if len(xs) == 0:
        return None
    x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
    sub = arr[y0:y1, x0:x1].copy()
    sub[sub[..., 3] == 0] = 0
    return (sub, ax - x0, ay - y0)
