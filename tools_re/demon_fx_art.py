#!/usr/bin/env python3
"""「악마화」 이펙트 그림 2 차 (2026-10-14) — `docs/skill/astra_demon` 예상도 · 보드를 기준으로 다시 그린 효과들.

1 차(`demon_fx.py` 의 옛 그림 함수)는 게임 해상도에서 도형을 바로 칠해 예상도보다 작고 밋밋했다. 여기의 함수는 `fxpaint.Canvas`
(4 배 슈퍼샘플링 · 노이즈 세기 필드 · 색 띠 · 결정 조각)로 그리고, 크기는 예상도를 게임 폭 176 으로 줄여 잰 값을 따른다
(사용자 2026-10-14: *"먼저 이펙트 퀄리티를 훨씬 올리고 그 뒤에 판정범위를 이펙트에 맞게 재정의"* — 그래서 그림이 판정보다 클 수 있다).

함수 이름 · 인수 · 프레임 수는 1 차와 같다(효과 번호 · 길이는 `DemonPatch::FX_*` 와 한 계약). `demon_fx.build()` 가 이 모듈에 있는
함수를 옛 함수 대신 쓴다. 좌표 = 기준점(개체의 발바닥) 상대 게임 px.
"""

import math
import os
import random

import numpy as np
from PIL import Image

import fxpaint as fp
from fxpaint import Canvas

UP, DOWN, LEFT, RIGHT = 0, 1, 2, 3
WHITE = (255, 255, 255)
CREAM = (255, 246, 226)
PINK_LT = (255, 200, 236)
RED_GLOW = (170, 24, 30)
EMBER = (255, 120, 40)
HOT_RIM = (236, 40, 40)


# ── 방향 ───────────────────────────────────────────────────────────────────────────────────────────────────
def _frame_of(d):
    """방향 d 의 (앞 단위벡터, 옆 단위벡터, 회전각) — 오른쪽 그림의 국소 좌표 (앞 x, 옆 y) 를 화면으로 옮긴다."""
    return {RIGHT: ((1, 0), (0, 1), 0.0), DOWN: ((0, 1), (-1, 0), math.pi / 2), UP: ((0, -1), (1, 0), -math.pi / 2)}[d]


class Local:
    """국소 좌표 → 화면 좌표 (기준점 o)."""

    def __init__(self, d, o):
        self.F, self.S, self.rot = _frame_of(d)
        self.o = o

    def __call__(self, lx, ly):
        (fx, fy), (sx, sy) = self.F, self.S
        return self.o[0] + lx * fx + ly * sx, self.o[1] + lx * fy + ly * sy

    def ang(self, a):
        return a + self.rot


# ── 할퀴기 ─────────────────────────────────────────────────────────────────────────────────────────────────
CLAW_FADE = [1.0, 1.0, 0.96, 0.9, 0.84, 0.78]
CLAW_ERODE = [0.0, 0.0, 0.02, 0.06, 0.1, 0.14]
CLAW_ALPHA = [1.0, 1.0, 0.95, 0.85, 0.7, 0.5]
CLAW_THIN = [1.0, 0.94, 0.74, 0.54, 0.36, 0.22]  # 참격은 어두워지지 않고 가늘어지며 사라진다
CHEST = (0, -20)  # 참격이 시작되는 가슴 높이
REACH = 40  # 가슴에서 참격 현 가운데까지 (예상도 1 단 X 의 가운데 = 발에서 앞 +44 · 위 −27)
ROCK_DARK = dict(base=(30, 22, 28), hi=(78, 64, 70))


def _chips(c, L, f, rnd, n, origin, spread, hot=HOT_RIM, up=1.0, size=(3.0, 4.4), k=1.0):
    """참격에 튀는 검은 조각 (국소 좌표 origin 둘레의 고리에서 · 프레임마다 바깥으로)."""
    for i in range(n):
        th = (i / n) * 2 * math.pi + rnd.uniform(-0.3, 0.3)
        sp = rnd.uniform(0.85, 1.25) * spread
        lx = origin[0] + math.cos(th) * sp * (1.0 + f * 0.3)
        ly = origin[1] + math.sin(th) * sp * 0.8 * (1.0 + f * 0.3) - up * f * 1.6 + 0.4 * f * f
        x, y = L(lx, ly)
        a = (1.0 - max(0, f - 3) / 3) * k
        c.shard(x, y, rnd.uniform(*size), rnd.uniform(0, 6.3) + f * 0.6, a, rim=hot, rim_a=0.9, fill=(18, 10, 16), facet=(62, 44, 54))


def _spark(c, x, y, f, size=1.0, col=WHITE, rays=(255, 120, 150), k=1.0):
    if f > 2:
        return
    a = (1.0, 0.75, 0.35)[f] * k
    rnd = random.Random(int(x * 7 + y * 3))
    c.glow(x, y, 5 * size, 5 * size, (255, 110, 110), 0.55 * a)
    c.star(x, y, 6, 0.9 * size, (11 + f * 3) * size, rays, a * 0.9, phase=0.25, rnd=rnd)
    c.star(x, y, 4, 0.7 * size, (7 + f * 2) * size, col, a, phase=0.0, core=CREAM)


def _halo(c, mx, my, ch, bu, half, R, thick, col, a, rv=1.0):
    """참격 둘레의 옅은 기운 (덮기 — 더하기 빨강은 풀 위에서 올리브색이 된다)."""
    c.slash(mx, my, ch, bu, half, R, thick, None, alpha=a, rough=0.0, reveal=rv, bias=0.45, glow=col, taper=0.5)


def claw_frame(cut, d, f):
    """참격 한 컷 — 프레임 0 = 타격 순간, 그리기마다 한 프레임 (6). cut 0 = 1 단 첫 손(／) · 1 = 둘째 손(＼, X) ·
    2 = 2 단 쓸어올리기(세 겹 금빛 호) · 3 = 3 단 땅 가르기(세 줄 + 흙먼지 · 돌)."""
    c = Canvas(220, 220, 110, 150)
    rnd = random.Random(3000 + cut * 100 + d * 10)
    fade, erode, thin = CLAW_FADE[f], CLAW_ERODE[f], CLAW_THIN[f]
    k = (0.88 if d == UP else 1.0) * CLAW_ALPHA[f]
    if cut in (0, 1):
        L = Local(d, CHEST)
        M = L(REACH if d != UP else REACH - 10, 0)

        def cut_one(chord, bulge, fd, er, thick=9.0, seed=0, a=1.0):
            ch, bu = L.ang(chord), L.ang(bulge)
            thick *= thin
            _halo(c, M[0], M[1], ch, bu, 0.7, 55.5, thick * 2.4, (200, 16, 30), 0.55 * fd * k * a)
            c.slash(M[0], M[1], ch, bu, 0.68, 54, thick, fp.CLAW, fade=fd, erode=er, seed=seed, alpha=k * a, bias=0.32, taper=0.6, power=0.35,
                    soft=0.8)

        if cut == 0:
            cut_one(-0.87, 0.70, fade, erode)
        else:
            cut_one(-0.87, 0.70, 0.74 * fade, min(1.0, erode + 0.12), thick=7.5, seed=3, a=0.9)
            cut_one(0.87, -0.70, fade, erode, seed=7)
        hit = L(REACH + 18, 0)
        _spark(c, hit[0], hit[1], f, 1.1, k=k)
        _chips(c, L, f, rnd, 9, (REACH + 2, 0), 30, k=k)
        return c
    if cut == 2:
        L = Local(d, (0, -26))
        ch, bu = L.ang(-math.pi / 2), L.ang(0.0)
        for j, (back, thick, fd) in enumerate(((0, 18.0, 1.0), (11, 5.0, 0.75))):
            Mj = L(48 - back, 0)
            R = 46 - back * 0.4
            thick *= thin
            if j == 0:
                _halo(c, Mj[0], Mj[1], ch, bu, 1.0, R + 1.5, thick * 1.7, (230, 40, 20), 0.5 * fade * k)
            c.slash(Mj[0], Mj[1], ch, bu, 0.98, R, thick, fp.GOLD, fade=fd * fade, erode=erode + 0.08 * j, seed=11 + j, alpha=k,
                    bias=0.2 if j == 0 else 0.3, taper=0.7, power=0.35, soft=0.8,
                    stripes=0.18 if j == 0 else 0.0, nstripes=3)
        if f >= 1:  # 끝자락의 자홍 잔광 (보드 02 RISING RAKE 셋째 그림)
            M = L(48, 0)
            c.slash(M[0], M[1], ch, bu, 0.98, 46, 12, [(0.25, (170, 24, 120), 0.8), (0.55, (255, 80, 200), 0.9)], alpha=0.45 * k,
                    reveal=0.32, rough=0.3, erode=erode, seed=21, bias=0.2)
        top = L(50, -38)
        _spark(c, top[0], top[1], f, 1.1, rays=(255, 190, 90), k=k)
        g = {RIGHT: (30, 0), DOWN: (-22, 22), UP: (22, -26)}[d]
        trail = {RIGHT: (-1, 0), DOWN: (0, -1), UP: (0, 1)}[d]
        for j in range(4):  # 땅을 쓸고 간 흙먼지 꼬리
            if f < 5:
                c.blob(g[0] + trail[0] * j * 9 + f * 1.2, g[1] + trail[1] * j * 6 - f * 0.8 - 2, 8 + f * 1.8 - j, 5.0 + f * 0.6, fp.DUST, seed=f * 3 + j,
                       alpha=0.85 * (1 - f / 6) * (1 - j * 0.15) * k, scale=0.5)
        _chips(c, L, f, rnd, 9, (38, 6), 24, hot=EMBER, up=2.2, k=k)
        return c
    # cut 3: 땅 가르기 — 세 줄이 앞 땅으로 내리꽂힌다 (방향과 무관하게 화면 아래로; 앞으로 자리만 옮긴다)
    gx, gy = {RIGHT: (52, 0), DOWN: (0, 24), UP: (0, -28)}[d]
    for i in range(3):
        off = (i - 1) * 11
        sx, sy = gx - 36 + off * 0.6, gy - 44 + abs(i - 1) * 3
        ex, ey = gx + off + 4, gy - 1
        mx, my = (sx + ex) / 2, (sy + ey) / 2
        ch = math.atan2(ey - sy, ex - sx)
        _halo(c, mx, my, ch, ch - math.pi / 2, 0.31, 100, 18 * thin, (230, 50, 16), 0.45 * fade * k)
        c.slash(mx, my, ch, ch - math.pi / 2, 0.3, 100, (8.0 - abs(i - 1) * 1.2) * thin, fp.GOLD, fade=fade, erode=erode, seed=31 + i, alpha=k,
                bias=0.4, taper=0.6, power=0.35, soft=0.6)
    heat = max(0.0, 1.0 - f / 5)
    for i in range(3):  # 땅의 긁힘 (식어 가는 세 갈래)
        off = (i - 1) * 11
        pts = [(gx + off - 8, gy - 3), (gx + off + 1, gy), (gx + off + 10, gy + 3)]
        c.taper(pts, 3.0, 0.8, (34, 10, 8), 0.95 * k)
        c.taper(pts, 1.6, 0.5, (255, 160 - int(100 * (1 - heat)), 40), heat * k)
    if f < 3:
        c.glow(gx, gy - 2, 16, 6, (255, 190, 90), (0.75, 0.45, 0.2)[f] * k)
        c.star(gx, gy - 3, 7, 1.2, 15 - f * 2, CREAM, (0.95, 0.6, 0.3)[f] * k, phase=0.1, squash=0.55, rnd=random.Random(3))
    for j in range(5):  # 흙먼지 — 낮게 넓게
        th = (j - 2) * 0.55
        sp = 8 + f * 3.6
        c.blob(gx + math.sin(th) * sp * 2.0, gy - 4 - f * 1.0 - abs(math.cos(th)) * 2, 11 + f * 1.6, 5.5 + f * 0.7, fp.DUST,
               seed=40 + j + f * 5, alpha=0.92 * (1 - f / 6.5) * k, scale=0.45)
    r2 = random.Random(77 + d)
    for i in range(8):  # 튀는 돌
        th = -math.pi / 2 + r2.uniform(-1.3, 1.3)
        sp = r2.uniform(2.2, 4.2)
        x = gx + math.cos(th) * sp * (f + 0.7) * 2.6
        y = gy - 4 + math.sin(th) * sp * (f + 0.7) * 2.3 + 1.4 * (f + 0.7) ** 2
        if y > gy + 4:
            continue
        c.rock(x, y, r2.uniform(2.4, 3.6), (1.0 - max(0, f - 3) / 3) * k, rnd=random.Random(i), hot=HOT_RIM if i % 3 == 0 else None,
               base=(38, 32, 34), hi=(92, 82, 82))
    return c


# ── 돌진 ───────────────────────────────────────────────────────────────────────────────────────────────────
BACK = {UP: (0, 1), DOWN: (0, -1), LEFT: (1, 0), RIGHT: (-1, 0)}
# 갈기 불이 나오는 몸의 점 (발 기준 화면 좌표) — 옆모습은 머리 · 등 · 허리, 앞/뒷모습은 두 어깨와 머리
MANE = {RIGHT: [(-2, -29), (-7, -23), (-9, -16)], DOWN: [(-11, -24), (11, -24), (0, -33)], UP: [(-7, -22), (7, -22), (0, -28)]}
MANE_FIRE = [(0.06, (46, 4, 12), 1.0), (0.2, (126, 10, 26), 1.0), (0.4, (214, 26, 30), 1.0), (0.62, (255, 82, 34), 1.0),
             (0.82, (255, 170, 70), 1.0)]


def _demon_off():
    import demon_fx as dfx
    return dfx.DEMON_OFF


def _demon_img(pose, d):
    """(그림, 배율) — ★ HD: `costumes/demon_300_hd` 가 있으면 3× 그림."""
    import demon_fx as dfx
    return dfx.demon_any(pose, d)


def charge_ground(d, f):
    """돌진 땅 층(모든 개체 밑): 남청 잔상 둘 · 바닥에 깔린 붉은 속도선 · 낮게 이는 흙먼지 · 구르는 돌 (8 프레임 반복)."""
    c = Canvas(170, 140, 85, 90)
    bx, by = BACK[d]
    off = _demon_off()
    walk = _demon_img("walk%d" % ((f // 2) % 4), d)
    for k, (dist, a) in enumerate(((32, 0.5), (17, 0.78))):
        x, y = off[0] + bx * dist, off[1] + by * dist * 0.8
        c.sprite_tint(walk[0], x + bx * 2, y + by * 2, a * 0.75, (230, 40, 90), 0.95, scale=walk[1])
        c.sprite_tint(walk[0], x, y, a, (96, 80, 200), 0.6, scale=walk[1])
    for k in range(3):  # 바닥 속도선 (몸 뒤로)
        side = (k - 1) * 4
        px, py = -by * side, bx * side
        L = 26 + 8 * ((f + k * 3) % 4)
        x0, y0 = px + bx * 6, py + by * 6 - 2
        c.taper([(x0, y0), (x0 + bx * L * 0.5, y0 + by * L * 0.5), (x0 + bx * L, y0 + by * L)], 2.2, 0.3, (190, 20, 30), 0.55)
    for j in range(4):  # 흙먼지 — 발 뒤로 낮게, 프레임마다 흘러간다
        t = ((f + j * 2) % 8) / 8
        dist = 6 + t * 30
        c.blob(bx * dist, by * dist * 0.7 + 1 - t * 2, 4.5 + t * 6, 2.6 + t * 2.4, fp.DUST, seed=50 + j * 7 + f, alpha=0.9 * (1 - t) ** 0.8, scale=0.5)
    rnd = random.Random(4000 + d * 10)
    for j in range(3):
        t = ((f + j * 3) % 8) / 8
        x = bx * (8 + t * 26) + rnd.uniform(-4, 4)
        y = by * (8 + t * 26) * 0.7 - 2 - math.sin(t * math.pi) * 6
        c.rock(x, y, 2.0, 1.0 - t, rnd=random.Random(j), base=(46, 38, 36), hi=(100, 88, 80))
    return c


def charge_top(d, f):
    """돌진 위 층: 머리 · 등에서 뒤로 길게 흐르는 불꽃 갈기 + 검붉은 그을음 줄 + 앞쪽 충돌 섬광 (8 프레임 반복)."""
    c = Canvas(170, 150, 85, 100)
    bx, by = BACK[d]
    ang = math.atan2(by, bx)
    pts = MANE[d] if d != LEFT else [(-x, y) for x, y in MANE[RIGHT]]
    side = d in (LEFT, RIGHT)
    tongues = []
    for i, (x, y) in enumerate(pts):
        for j in range(2):  # 갈기 한 가닥에 불혀 둘 — 길이 · 기울기가 다르다
            h = ((40 if side else 22) - i * 5 - j * 12) + 5 * math.sin(f * 0.9 + i * 2.1 + j)
            a = ang + (j - 0.5) * 0.22 + 0.08 * math.sin(f * 0.8 + i)
            if d == DOWN and i < 2:  # 앞모습: 어깨 뒤에서 위 · 바깥으로 (얼굴을 덮지 않게)
                a = ang + (-0.45 if i == 0 else 0.45) + (j - 0.5) * 0.2
            tongues.append((x + bx * 3, y, h, 4.6 - i * 0.6 - j * 0.6, 0.0, 60 + i * 3 + j * 17, a))
    # 그을음: 불 뒤로 길게 끌리는 검붉은 기운
    for i, (x, y) in enumerate(pts[:2]):
        L = (40 if side else 26) + 4 * math.sin(f + i)
        wob = [(x + bx * L * t + (-by) * math.sin(f * 1.1 + t * 5 + i) * 2.5 * t, y + by * L * t + bx * math.sin(f * 1.1 + t * 5 + i) * 2.5 * t)
               for t in (0, 0.25, 0.5, 0.75, 1.0)]
        c.taper(wob, 5.5 - i, 0.6, (24, 6, 14), 0.75)
    c.flames(tongues, t=f * 0.9, stops=MANE_FIRE, rough=0.6, warp=1.6)
    rnd = random.Random(5000 + d * 10 + f)
    for j in range(5):  # 불티
        t = ((f + j * 1.7) % 8) / 8
        x, y = pts[j % len(pts)]
        c.ellipse(x + bx * t * 34 + rnd.uniform(-3, 3), y + by * t * 34 - t * 4 + rnd.uniform(-3, 3), 0.7, 0.7, (255, 180, 80), 1 - t, "add")
    fx, fy = -bx * 15, -15 - by * 10 if side else -14 - by * 14
    if f % 4 in (0, 1):  # 앞쪽 충돌 섬광 (반복 중 두 번)
        k = 1.0 if f % 4 == 0 else 0.55
        c.glow(fx, fy, 6, 6, (255, 120, 140), 0.6 * k)
        c.star(fx, fy, 6, 1.0, 14 * k + 3, (255, 110, 180), 0.85 * k, phase=0.3, rnd=rnd)
        c.star(fx, fy, 4, 0.8, 9 * k + 2, WHITE, k, phase=0.0, core=CREAM)
    return c


# ── 추종 ───────────────────────────────────────────────────────────────────────────────────────────────────
MAGENTA = (236, 54, 176)
VIOLET_LT = (226, 176, 255)
PURPLE = (132, 46, 214)
SHARD_RIM = (240, 70, 190)
DOME_RX, DOME_H, DOME_RY = 46, 70, 13  # 예상도 02 의 반구 ≈ 95 × 75 px


def _dome_I(X, Y, rx, h, ry):
    """반구의 정면 투영: 바깥 테까지의 정규 거리 (0 가운데 · 1 테). 아래는 바닥 타원."""
    u = X / rx
    up = np.where(Y < 0, -Y / h, Y / ry)
    return np.sqrt(u * u + up * up)


def follow_build(level, f):
    """추종 게이지 25/50/75 % 이하 단계 (땅 층): 발치의 보라 소용돌이 · 떠오르는 입자 · 돌던 검은 조각 · 75 % 이상 보라 불꽃."""
    c = Canvas(120, 110, 60, 80)
    R = 13 + level * 4
    ph = f / 16 * 2 * math.pi
    c.ellipse(0, 0, R + 3, (R + 3) * 0.32, (40, 8, 70), 0.35 + 0.08 * level)
    c.glow(0, 0, R, R * 0.3, PURPLE, 0.45 + 0.12 * level)
    for i in range(2 + level):  # 바닥을 도는 보라 호
        a0 = ph + i * 2 * math.pi / (2 + level)
        c.crescent(0, 0, R, 2.6, a0, a0 + 1.6, fp.VIOLET, ry=R * 0.32, seed=i, rough=0.15, alpha=0.9, taper=0.6, power=0.5)
    rnd = random.Random(700 + level)
    for k in range(4 + 3 * level):  # 떠오르는 입자
        t = ((f / 16) + rnd.random()) % 1.0
        x = rnd.uniform(-R, R) + math.sin(t * 6 + k) * 2
        y = -t * (16 + level * 7)
        c.ellipse(x, y, 0.8, 0.8, VIOLET_LT if k % 3 else MAGENTA, math.sin(math.pi * t), "add")
    if level >= 1:
        for k in range(level + 1):  # 도는 검은 조각
            th = ph + k * 2 * math.pi / (level + 1)
            c.shard(math.cos(th) * (R + 1), -5 - 3 * math.sin(ph * 2 + k) + math.sin(th) * R * 0.3, 2.6 + level * 0.4, th, 1.0, rim=SHARD_RIM)
    if level >= 3:
        tongues = [(math.cos(ph + k * 1.26) * R * 0.9, math.sin(ph + k * 1.26) * R * 0.28, 10 + 4 * math.sin(f * 0.8 + k), 5, 0, 90 + k)
                   for k in range(5)]
        c.flames(tongues, t=f * 0.8, stops=fp.VIOLET, rough=0.55)
    return c


def follow_dome(f):
    """추종 100 % (위 층): 안이 비치는 보라 반구 · 밝은 테 · 면을 기는 번개 · 떠 있는 검은 조각 · 바닥 고리 (16 프레임 반복)."""
    rx, h, ry = DOME_RX, DOME_H, DOME_RY
    c = Canvas(rx * 2 + 20, h + ry + 24, rx + 10, h + 12)
    ph = f / 16 * 2 * math.pi

    def shell(X, Y):
        d = _dome_I(X, Y, rx, h, ry)
        inside = d <= 1.0
        rim = np.clip((d - 0.55) / 0.45, 0, 1)
        n = fp.fbm(X * 0.18 + f * 0.05, Y * 0.18 - f * 0.2, 5, 3)
        a = (0.07 + 0.42 * rim ** 2.4 + (n - 0.5) * 0.08) * inside
        a = np.where(Y > 0, a * 0.55, a)  # 바닥 쪽은 옅게
        col = np.stack([90 + 120 * rim, 30 + 30 * rim, 170 + 70 * rim], axis=-1)
        return col.astype(np.float32), np.clip(a, 0, 1)

    c.field((-rx - 2, -h - 2, rx + 2, ry + 2), shell)
    # 면의 빛줄 — 세로 경선 몇 개 (반구 곡면이 읽히게)
    for k in range(-2, 3):
        u = k / 3.2 + 0.04 * math.sin(ph + k)
        pts = [(u * rx * math.cos(t), -h * math.sin(t)) for t in [i * math.pi / 2 / 10 for i in range(11)]]
        c.stroke(pts, 0.6, (190, 120, 255), 0.22)
    # 안쪽 번개 (2 프레임마다 모양이 바뀐다)
    rnd = random.Random(6000 + f // 2)
    for k in range(3):
        sx = rnd.choice((-1, 1)) * rnd.uniform(0.35, 0.85) * rx
        top = -math.sqrt(max(0.0, 1 - (sx / rx) ** 2)) * h * rnd.uniform(0.5, 0.9)
        pts = c.bolt_pts(sx, -2, sx + rnd.uniform(-10, 10), top, 4, 0.35, rnd)
        c.lightning(pts, 1.0, (110, 30, 190), (200, 110, 255), core=(250, 230, 255), alpha=0.85, glow=PURPLE)
    # 떠 있는 검은 조각 (가장자리 쪽 — 악마를 덮지 않게)
    for k in range(8):
        th = k * 2 * math.pi / 8 + 0.3
        rr = 0.62 + 0.18 * (k % 2)
        x = math.cos(th) * rx * rr
        y = -h * 0.45 - math.sin(th) * h * 0.3 * rr - 2.5 * math.sin(ph + k * 0.9)
        if abs(x) < 14 and y > -40:
            x += 16 if x >= 0 else -16
        c.shard(x, y, 4.6 + (k % 3) * 0.9, th * 0.5 + ph * 0.2, 0.95, rim=SHARD_RIM)
    # 테: 어두운 바깥선 + 자홍 + 밝은 심
    arc = [(math.cos(t) * rx, -math.sin(t) * h) for t in [i * math.pi / 48 for i in range(49)]]
    c.stroke(arc, 3.4, (46, 6, 70), 0.8)
    c.stroke(arc, 2.4, MAGENTA, 1.0)
    c.stroke(arc, 1.0, (255, 210, 255), 0.95)
    c.stroke(arc, 6.0, PURPLE, 0.35, "add")
    # 바닥 고리
    c.ellipse(0, 0, rx, ry, MAGENTA, 0.9, width=1.6)
    c.ellipse(0, 0, rx, ry, (255, 200, 255), 0.6, width=0.6)
    c.glow(0, 0, rx * 0.9, ry * 0.8, PURPLE, 0.35)
    for k in range(6):  # 테를 타고 오르는 불티
        t = ((f / 16) + k / 6) % 1.0
        a = math.pi * (0.1 + 0.8 * ((k * 0.37) % 1))
        c.ellipse(math.cos(a) * rx * 0.98, -math.sin(a) * h * 0.98 - t * 6, 0.9, 0.9, (255, 220, 255), math.sin(math.pi * t), "add")
    return c


def burst(age):
    """추종 끝의 어둠 폭발 (위 층, 20): 흰 섬광 → 검은 결정 가시가 부채꼴로 솟는다 → 부서지며 보라 기운으로 흩어진다."""
    c = Canvas(200, 150, 100, 110)
    cy = -14
    rnd = random.Random(7000)
    if age < 4:
        a = 1.0 - age / 4
        c.glow(0, cy, 14, 12, (255, 120, 220), 0.9 * a)
        c.star(0, cy, 8, 1.6, 20 - age * 3, (255, 150, 230), a, phase=0.2, squash=0.85, rnd=random.Random(1))
        c.star(0, cy, 4, 1.2, 12 - age * 2, WHITE, a, core=CREAM)
    if 1 <= age < 15:
        grow = fp_ease(min(1.0, (age - 1) / 3))
        shrink = 1.0 - fp_ease(max(0.0, (age - 9) / 6))
        L = 46 * grow * shrink
        spikes = []
        for k in range(9):  # 위로 왕관처럼 — 가운데(악마 몸 앞)는 비운다
            th = math.pi + 0.22 + (k + 0.5) * (math.pi - 0.44) / 9 + rnd.uniform(-0.08, 0.08)
            if abs(th - 1.5 * math.pi) < 0.3:
                th += 0.32 if th > 1.5 * math.pi else -0.32
            ln = L * rnd.uniform(0.7, 1.15) * (0.75 + 0.25 * abs(math.sin(th)))
            spikes.append((math.sin(th), th, ln, rnd.uniform(15, 21)))
        for k in range(4):  # 앞(아래)쪽 짧은 가시
            th = (k + 0.5) * math.pi / 4 + rnd.uniform(-0.15, 0.15)
            spikes.append((math.sin(th), th, L * rnd.uniform(0.3, 0.45), rnd.uniform(12, 15)))
        spikes.sort()
        for _, th, ln, w in spikes:
            ux, uy = math.cos(th), math.sin(th)
            bx, by = ux * 17, -2 + uy * 6
            tx, ty = bx + ux * ln * 0.95, by + uy * ln
            if ln > 2:
                c.spike(bx, by, tx, ty, w * (0.7 + 0.3 * grow), 1.0, rim=SHARD_RIM, tip_glow=(255, 120, 220) if age < 7 else None)
    if age < 16:  # 파편
        r3 = random.Random(72)
        for k in range(16):
            th = r3.uniform(0, 2 * math.pi)
            sp = r3.uniform(2.5, 5)
            x = math.cos(th) * sp * age * 1.6
            y = cy + math.sin(th) * sp * age * 0.8 - 5 * age + 0.5 * age * age
            if y > 6:
                continue
            c.shard(x, y, 3.0 + (k % 3) * 0.9, th + age, 1.0 - max(0, age - 10) / 6, rim=SHARD_RIM)
    if age >= 8:  # 보라 기운이 흩어진다
        t = age - 8
        a = 1.0 - max(0, t - 5) / 7
        for k in range(8):
            th = k * 2 * math.pi / 8 + t * 0.08
            r = 18 + t * 4
            c.blob(math.cos(th) * r, -6 + math.sin(th) * r * 0.45 - t * 1.5, 7, 4, fp.VIOLET, seed=k + t, rough=0.6, alpha=0.55 * a,
                   scale=0.35)
        c.ellipse(0, 0, 22 + t * 4, 7 + t * 1.3, MAGENTA, 0.6 * a, width=1.2)
    return c


def burst_mob(age):
    """폭발에 맞은 몬스터 (위 층, 12): 몸에서 검은 가시 다섯이 튀어나오고 분홍 섬광."""
    c = Canvas(80, 80, 40, 56)
    cy = -12
    a = 1.0 - max(0, age - 6) / 6
    if age < 4:
        c.star(0, cy, 6, 1.4, 14 - age * 2, (255, 190, 240), 1 - age / 4, phase=0.4, core=WHITE)
        c.glow(0, cy, 8, 8, (255, 90, 210), 0.6 * (1 - age / 4))
    L = 16 * fp_ease(min(1.0, age / 3)) * (1.0 - max(0, age - 7) / 5)
    for k in range(5):
        th = k * 2 * math.pi / 5 + 0.3
        if L > 1:
            c.spike(math.cos(th) * 3, cy + math.sin(th) * 3, math.cos(th) * L, cy + math.sin(th) * L * 0.8, 6.5, a, rim=SHARD_RIM)
    rnd = random.Random(7500)
    for k in range(6):
        c.ellipse(rnd.uniform(-12, 12), cy + rnd.uniform(-12, 8) - age, 0.8, 0.8, VIOLET_LT, a, "add")
    return c


def fp_ease(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


# ── 지옥칼날 ───────────────────────────────────────────────────────────────────────────────────────────────
SEAL_RX, SEAL_RY = 74, 26  # 예상도 02 의 바닥 인장 ≈ 160 × 55 px
CRIMSON = (196, 16, 34)
RUNE_RED = (255, 64, 54)
# 룬 글자 여섯 (5×5 상자 안의 꺾은선들)
RUNES = [
    [[(0, 4), (2, 0), (4, 4)], [(1, 2.5), (3, 2.5)]],
    [[(0, 0), (0, 4)], [(0, 0), (3, 1.2), (0, 2.4)], [(0, 2.4), (3, 4)]],
    [[(2, 0), (2, 4)], [(0, 1), (4, 3)]],
    [[(0, 0), (4, 0), (0, 4), (4, 4)]],
    [[(0, 4), (0, 0), (2, 2), (4, 0), (4, 4)]],
    [[(2, 0), (0, 2), (2, 4), (4, 2), (2, 0)]],
]


def _ell(t, rx, ry):
    return math.cos(t) * rx, math.sin(t) * ry


def seal(f):
    """지옥칼날 바닥 인장 (땅 층, 32 프레임 반복 — 대칭 한 칸씩 돌아 이음매가 없다): 붉은 두 겹 고리 · 룬 띠 · 반대로 도는 오각별 ·
    고리를 쓸고 도는 검은 소용돌이 셋 · 떠오르는 불티."""
    rx, ry = SEAL_RX, SEAL_RY
    c = Canvas(rx * 2 + 16, ry * 2 + 40, rx + 8, ry + 26)
    u = f / 32
    c.ellipse(0, 0, rx + 2, ry + 1, (40, 2, 10), 0.32)
    c.glow(0, 0, rx * 0.8, ry * 0.8, (150, 0, 20), 0.35, "base")
    for r, w, a in ((1.0, 1.6, 1.0), (0.9, 0.9, 0.9), (0.62, 1.1, 0.85), (0.56, 0.6, 0.7)):
        c.ellipse(0, 0, rx * r, ry * r, (70, 0, 12), a * 0.8, width=w + 1.2)
        c.ellipse(0, 0, rx * r, ry * r, CRIMSON, a, width=w)
        c.ellipse(0, 0, rx * r, ry * r, RUNE_RED, a * 0.35, "add", width=w + 1.5)
    # 룬 띠 (16 칸, 한 바퀴의 1/16 씩)
    ph = u * 2 * math.pi / 16
    for k in range(16):
        t = k * 2 * math.pi / 16 + ph
        x, y = _ell(t, rx * 0.95, ry * 0.95)
        sc = 1.25 + 0.4 * (math.sin(t) + 1) / 2  # 앞쪽(아래)이 조금 크다
        g = RUNES[k % len(RUNES)]
        for line in g:
            pts = [(x + (px - 2) * sc, y + (py - 2) * sc * 0.6) for px, py in line]
            c.stroke(pts, 0.8, RUNE_RED, 1.0)
            c.stroke(pts, 1.8, (255, 40, 40), 0.25, "add")
    # 안쪽 오각별 (반대 방향으로 1/5 바퀴)
    ps = -u * 2 * math.pi / 5
    star = [_ell(ps + k * 4 * math.pi / 5 - math.pi / 2, rx * 0.56, ry * 0.56) for k in range(6)]
    c.stroke(star, 1.6, (90, 0, 16), 0.9)
    c.stroke(star, 0.8, (255, 70, 60), 0.9)
    for k in range(5):  # 별 끝의 작은 원
        x, y = _ell(ps + k * 2 * math.pi / 5 - math.pi / 2, rx * 0.56, ry * 0.56)
        c.ellipse(x, y, 2.2, 1.1, RUNE_RED, 0.9, width=0.6)
    # 검은 소용돌이 (셋, 1/3 바퀴)
    ps2 = u * 2 * math.pi / 3
    for k in range(3):
        t0 = k * 2 * math.pi / 3 + ps2
        arc = [_ell(t0 + s * 0.08, rx * (1.0 + 0.006 * s), ry * (1.0 + 0.012 * s)) for s in range(22)]
        c.taper(arc, 0.6, 6.0, (12, 4, 10), 0.9)
        c.taper(arc[8:], 0.4, 2.0, (150, 12, 36), 0.7)
        arc2 = [_ell(t0 + 1.0 + s * 0.07, rx * (0.74 - 0.004 * s), ry * (0.74 - 0.004 * s)) for s in range(14)]
        c.taper(arc2, 0.5, 3.4, (12, 4, 10), 0.8)
    rnd = random.Random(8000)
    for k in range(10):  # 불티 (32 프레임에 한 번 떠오른다)
        t = (u + k / 10) % 1.0
        x, y = _ell(rnd.uniform(0, 6.28), rx * rnd.uniform(0.3, 0.95), ry * rnd.uniform(0.3, 0.95))
        c.ellipse(x, y - t * 18, 0.8, 0.8, (255, 140, 70), math.sin(math.pi * t), "add")
    return c


def blade_rise(variant, f):
    """몬스터 자리에서 솟는 검은 낫날 (땅 층, 16): 0..3 솟음 · 4..9 머묾 · 10..13 가라앉음. 바닥의 붉은 균열과 튀는 조각."""
    c = Canvas(110, 110, 55, 80)
    sgn = -1 if variant == 0 else 1
    scale = {0: 0.3, 1: 0.65, 2: 0.92, 3: 1.05, 10: 0.85, 11: 0.6, 12: 0.32, 13: 0.1}.get(f, 1.0 if 4 <= f <= 9 else 0.0)
    bx = sgn * 7
    if f < 14:
        heat = 1.0 if f < 10 else (14 - f) / 4
        for k in range(4):  # 땅의 균열
            th = k * math.pi / 2 + 0.4 + variant
            pts = [(bx, 1), (bx + math.cos(th) * 6, 1 + math.sin(th) * 2.2), (bx + math.cos(th + 0.3) * 11, 1 + math.sin(th + 0.3) * 3.6)]
            c.taper(pts, 1.8, 0.4, (40, 4, 10), 0.9 * heat)
            c.taper(pts, 0.9, 0.3, (255, 70, 50), 0.9 * heat)
        c.glow(bx, 1, 9, 3, (255, 40, 40), 0.5 * heat, "base")
    if scale > 0:
        H = 46 * scale
        outer, inner = [], []
        for i in range(17):
            t = i / 16
            x = bx + sgn * (19 * t * t) - sgn * 6 * math.sin(math.pi * t)
            y = -H * t
            w = 15.0 * math.sin(math.pi * (0.12 + 0.88 * t)) ** 0.7 * (1 - 0.15 * t)
            outer.append((x - sgn * w * 0.55, y))
            inner.append((x + sgn * w * 0.45, y + 1.2 * (1 - t)))
        body = outer + inner[::-1]
        c.poly([(x - sgn * 0.8, y) for x, y in outer] + [(x + sgn * 0.8, y) for x, y in inner[::-1]], (6, 2, 6), 0.9)
        c.poly(body, (16, 6, 14), 1.0)
        mid = [((a[0] + b[0]) / 2, (a[1] + b[1]) / 2) for a, b in zip(outer, inner)]
        c.poly(outer + mid[::-1], (54, 20, 40), 1.0)  # 날 쪽 밝은 면
        c.stroke(outer, 1.3, MAGENTA, 1.0)
        c.stroke(outer[3:12], 0.6, (255, 200, 240), 0.9)
        c.stroke(outer, 3.2, (236, 54, 176), 0.25 * (0.7 + 0.3 * math.sin(f * 1.3)), "add")
        c.stroke(inner, 0.7, (150, 14, 50), 0.8)
        if 1 <= f <= 3:  # 솟을 때 튀는 조각
            rnd = random.Random(8500 + variant * 50 + f)
            for k in range(5):
                th = rnd.uniform(math.pi * 1.1, math.pi * 1.9)
                r = 4 + f * 4
                c.shard(bx + math.cos(th) * r, -2 + math.sin(th) * r * 0.8, 2.2, th, 1.0, rim=(255, 60, 60))
    return c


# ── 어둠심판 ───────────────────────────────────────────────────────────────────────────────────────────────
def judge_orb(age):
    """어둠심판 선딜 (위 층, 20): 머리 위에 검은 구체가 응축된다 — 보라 띠가 감아 돌고 · 조각이 궤도를 돌며 · 빛 알갱이가 빨려 든다."""
    c = Canvas(110, 110, 55, 96)
    cy = -50
    g = fp_ease(min(1.0, age / 14))
    r = 3 + 6 * g
    c.glow(0, cy, r * 2.4, r * 2.4, PURPLE, 0.55 + 0.25 * g)
    rnd = random.Random(9000)
    for k in range(10):  # 빨려 드는 알갱이
        th = rnd.uniform(0, 6.28)
        t = ((age / 20) + k / 10) % 1.0
        rr = (r + 24) * (1 - t) + r * t
        c.ellipse(math.cos(th) * rr, cy + math.sin(th) * rr * 0.8, 0.8, 0.8, VIOLET_LT, math.sin(math.pi * t), "add")
    for k in range(3):  # 감아 도는 보라 띠
        a0 = k * 2 * math.pi / 3 + age * 0.42
        c.crescent(0, cy, r + 6 + k * 1.5, 2.4, a0, a0 + 2.0, fp.VIOLET, ry=(r + 6 + k * 1.5) * 0.7, seed=k, rough=0.15, power=0.5)
    c.ellipse(0, cy, r + 1.4, r + 1.4, (180, 90, 255), 0.95)
    c.ellipse(0, cy, r, r, (22, 6, 34), 1.0)
    c.crescent(0, cy, r, max(1.0, r * 0.35), math.pi * 0.95, math.pi * 1.55, [(0.1, (110, 50, 170), 1.0), (0.5, (200, 150, 255), 1.0)],
               rough=0.0, taper=0.8)  # 위왼쪽 빛
    c.ellipse(-r * 0.35, cy - r * 0.35, r * 0.18, r * 0.18, (255, 230, 255), 0.9, "add")
    for k in range(5):  # 궤도의 조각
        th = k * 2 * math.pi / 5 - age * 0.32
        c.shard(math.cos(th) * (r + 14), cy + math.sin(th) * (r + 14) * 0.6, 2.8, th, fp_ease(min(1.0, age / 5)), rim=SHARD_RIM)
    if age % 3 != 2:
        r2 = random.Random(9100 + age)
        th = r2.uniform(0, 6.28)
        pts = c.bolt_pts(math.cos(th) * r, cy + math.sin(th) * r, math.cos(th) * (r + 12), cy + math.sin(th) * (r + 12), 3, 0.4, r2)
        c.lightning(pts, 0.7, (120, 40, 200), (210, 140, 255), alpha=0.9)
    return c


def bolt(variant, age):
    """하늘에서 떨어지는 보라 낙뢰 (위 층, 4): 흰 심의 굵은 줄기와 가지 → 발밑 충격 고리 · 빛살 · 튀는 검은 조각 · 보라 연기."""
    c = Canvas(110, 260, 55, 236)
    rnd = random.Random(9500 + variant)
    pts = c.bolt_pts(rnd.uniform(-10, 10), -232, 0, -4, 5, 0.22, rnd)
    branches = []
    for b in range(6):
        i = rnd.randrange(len(pts) // 6, len(pts) * 4 // 5)
        x0, y0 = pts[i]
        side = rnd.choice((-1, 1))
        bp = c.bolt_pts(x0, y0, x0 + side * rnd.uniform(10, 22), y0 + rnd.uniform(14, 30), 3, 0.35, rnd)
        branches.append(bp)
    if age < 3:
        a = (1.0, 0.8, 0.45)[age]
        w = (4.6, 3.4, 2.0)[age]
        c.stroke(pts, w * 4.5, (150, 40, 230), 0.4 * a, "add")
        c.stroke(pts, w * 2.4, (70, 10, 120), 0.6 * a)
        c.lightning(pts, w, (150, 30, 200), (236, 100, 240), core=WHITE, alpha=a, glow=None)
        for bp in branches[: (6 if age == 0 else 3)]:
            c.lightning(bp, w * 0.4, (110, 30, 170), (210, 120, 255), core=(250, 220, 255), alpha=a)
        if age == 0:  # 땅에 닿는 빛기둥
            c.glow(0, -14, 7, 16, (255, 210, 255), 0.8)
    t = age
    ra = (1.0, 0.85, 0.6, 0.32)[t]
    c.ellipse(0, 0, 13 + t * 5, 4.5 + t * 1.6, (70, 10, 90), 0.6 * ra, width=2.6)
    c.ellipse(0, 0, 13 + t * 5, 4.5 + t * 1.6, MAGENTA, ra, width=1.4)
    c.ellipse(0, 0, 13 + t * 5, 4.5 + t * 1.6, (255, 200, 255), 0.7 * ra, width=0.6)
    c.glow(0, -2, 14 + t * 3, 5 + t, PURPLE, 0.7 * ra)
    if t < 2:
        c.star(0, -5, 10, 1.8, 24 - t * 6, (255, 170, 250), 0.95 - t * 0.35, phase=0.3, squash=0.6, rnd=random.Random(9600 + variant))
        c.star(0, -5, 6, 1.4, 13 - t * 4, WHITE, 1.0 - t * 0.4, phase=0.0, squash=0.7, core=CREAM)
        c.glow(0, -5, 9, 7, (255, 220, 255), 0.85 - t * 0.35)
    r3 = random.Random(9700 + variant)
    for k in range(10):  # 튀는 조각
        th = math.pi + r3.uniform(0.15, math.pi - 0.15)
        sp = r3.uniform(3, 6)
        x = math.cos(th) * sp * (t + 1) * 1.6
        y = -3 + math.sin(th) * sp * (t + 1) * 1.2 + 0.8 * (t + 1) ** 2
        c.shard(x, y, 2.2 + (k % 3) * 0.6, th + t, ra, rim=SHARD_RIM)
    if t >= 1:
        for k in range(3):  # 보라 연기
            c.blob((k - 1) * 9, -6 - t * 3, 6 + t, 4 + t * 0.6, fp.VIOLET, seed=k + 9, rough=0.6, alpha=0.4 * ra, scale=0.35)
    return c


# ── 멸화 ───────────────────────────────────────────────────────────────────────────────────────────────────
FIRE_RX, FIRE_RY = 62, 22  # 예상도 03 의 분출 ≈ 138 × 95 px (불혀 높이 포함)
ROCK_FIRE = dict(base=(30, 22, 24), hi=(78, 62, 60))


def _ring_tongues(rnd, n, rx, ry, front, grow, decay, t):
    """타원 둘레의 불혀들 — 바깥으로 기울어 부채꼴로 터진다. front = 앞(아래) 반쪽. 악마 몸(가운데) 앞의 불혀는 낮게 (몸이 보이게)."""
    out = []
    for k in range(n):
        th = (k + rnd.uniform(0.15, 0.85)) * math.pi / n + (0 if front else math.pi)
        ring = rnd.uniform(0.35, 1.0)
        x = math.cos(th) * rx * ring
        y = math.sin(th) * ry * ring
        side = abs(x) / rx
        if front:
            h = (18 + 40 * side ** 1.1) * rnd.uniform(0.75, 1.15)
            if abs(x) < 17:
                h = min(h, 12)
        else:
            h = (46 + 26 * (1 - side)) * rnd.uniform(0.8, 1.15)
        h *= grow * decay
        w = rnd.uniform(8, 12) * (0.75 + 0.25 * grow)
        ang = -math.pi / 2 + (x / rx) * (0.6 if front else 0.85) + rnd.uniform(-0.12, 0.12)
        out.append((x, y, h, w, 0.0, 120 + k + (0 if front else 50), ang))
    return out


def _blast_boost(grow, decay):
    """분출 밑동의 뜨거운 덩어리 — 불혀 사이를 메워 한 덩어리로 보이게."""
    def f(X, Y):
        u = X / (FIRE_RX * 0.8)
        v = (Y + 6) / (FIRE_RY * 1.1)
        n = fp.fbm(X * 0.3, Y * 0.3 + grow * 3, 13, 3)
        return (0.1 + 0.25 * n) * grow * decay * np.exp(-(u * u + v * v) * 2.6)
    return f


def fire_core(age):
    """멸화 선딜 (위 층, 10): 가슴의 붉은 핵이 커지고 · 붉은 기운 줄기가 빨려 들며 · 검은 조각이 떠오른다 → 마지막에 섬광."""
    c = Canvas(130, 130, 65, 100)
    cy = -18
    g = fp_ease(age / 9)
    c.glow(0, cy, 5 + 9 * g, 5 + 9 * g, (230, 30, 30), 0.65 + 0.3 * g, "base")
    for k in range(4):  # 빨려 드는 붉은 줄기
        a0 = k * math.pi / 2 + age * 0.55
        R = 26 - 14 * g
        c.crescent(0, cy, R, 3.2, a0, a0 + 1.7, fp.CLAW, ry=R * 0.8, seed=k, rough=0.2, power=0.5, taper=0.6)
    rnd = random.Random(10000)
    for k in range(7):  # 떠오르는 검은 조각
        th = rnd.uniform(0, 6.28)
        r = rnd.uniform(16, 28) * (1 - 0.3 * g)
        c.shard(math.cos(th) * r, cy + math.sin(th) * r * 0.6 - age * 1.2, 2.6 + (k % 3) * 0.6, th + age * 0.3, 1.0, rim=HOT_RIM)
    c.ellipse(0, cy, 1.5 + 2.5 * g, 1.5 + 2.5 * g, (255, 240, 220), 1.0)
    c.glow(0, cy, 3 + 3 * g, 3 + 3 * g, (255, 200, 160), 0.8, "add")
    if age >= 7:
        k = (age - 6) / 3
        c.star(0, cy, 8, 1.6, 10 + 16 * k, (255, 90, 70), 0.9, phase=0.2, rnd=random.Random(3))
        c.star(0, cy, 4, 1.2, 8 + 10 * k, WHITE, 1.0, phase=0.0, core=CREAM)
    return c


def fire_ground(age):
    """멸화 선딜 땅 층 (10): 발밑의 붉은 빛 · 바깥으로 갈라지는 빛나는 금 · 떠오르는 작은 돌."""
    c = Canvas(160, 80, 80, 40)
    g = fp_ease(age / 9)
    c.glow(0, 0, 10 + 22 * g, 4 + 7 * g, (220, 30, 20), 0.6, "base")
    rnd = random.Random(10500)
    for k in range(9):
        th = k * 2 * math.pi / 9 + rnd.uniform(-0.2, 0.2)
        L = (6 + 30 * g) * rnd.uniform(0.7, 1.1)
        pts = c.bolt_pts(math.cos(th) * 3, math.sin(th) * 1.2, math.cos(th) * L, math.sin(th) * L * 0.38, 3, 0.3, rnd)
        c.taper(pts, 2.2, 0.5, (40, 6, 6), 0.9 * g)
        c.taper(pts, 1.1, 0.3, (255, 150, 50), g)
    for k in range(5):
        th = rnd.uniform(0, 6.28)
        r = rnd.uniform(18, 30)
        c.rock(math.cos(th) * r, math.sin(th) * r * 0.35 - g * 5, 1.8, g, rnd=random.Random(k), hot=HOT_RIM, **ROCK_FIRE)
    return c


def fire_blast(age):
    """멸화 폭발 앞 층 (위 층, 24): 0..2 섬광 · 1..17 앞 · 옆의 불혀(몸 앞은 낮게) · 1..15 튀는 검은 바위 · 10.. 연기와 불티.
    뒤쪽 불혀는 같은 순간의 `fire_scorch`(땅 층 — 악마 · 몬스터 밑)가 그린다."""
    c = Canvas(240, 200, 120, 150)
    if age < 3:
        a = 1.0 - age / 3
        c.glow(0, -18, 34, 26, (255, 60, 30), 0.75 * a, "base")
        c.star(0, -18, 10, 2.2, 46 - age * 10, (255, 110, 70), a, phase=0.15, squash=0.85, rnd=random.Random(11001))
        c.star(0, -18, 6, 1.8, 24 - age * 6, CREAM, a, phase=0.0, core=WHITE)
    if 1 <= age < 18:
        t = age - 1
        grow = fp_ease(min(1.0, t / 3))
        decay = 1.0 - fp_ease(max(0.0, (t - 8) / 9))
        rnd = random.Random(11000)
        tongues = _ring_tongues(rnd, 20, FIRE_RX, FIRE_RY, True, grow, decay, t)
        c.glow(0, -6, FIRE_RX * grow, FIRE_RY * 1.5 * grow, (200, 20, 20), 0.3 * decay, "base")
        c.flames(tongues, t=t * 0.9, stops=fp.FIRE, rough=0.55, warp=0.9)
    if 1 <= age < 17:  # 검은 바위
        t = age - 1
        r2 = random.Random(11002)
        for k in range(14):
            th = r2.uniform(0, 2 * math.pi)
            sp = r2.uniform(2.4, 4.6)
            x = math.cos(th) * (10 + sp * t * 1.45)
            y = -10 + math.sin(th) * sp * t * 0.7 - 6.5 * t + 0.48 * t * t
            if y > 8:
                continue
            c.rock(x, y, 4.0 + (k % 4) * 1.1, 1.0 - max(0, t - 11) / 5, rnd=random.Random(k + 70), hot=HOT_RIM, **ROCK_FIRE)
    if age >= 10:  # 연기 · 불티
        t = age - 10
        a = 1.0 - max(0, t - 7) / 7
        r3 = random.Random(11003)
        for k in range(8):
            x = r3.uniform(-54, 54)
            y = 6 - r3.uniform(0, 10) - t * 1.6
            if abs(x) < 16:
                x += 20 if x >= 0 else -20
            c.blob(x, y, 8 + t * 0.7, 6 + t * 0.4, fp.SMOKE, seed=k + 40, alpha=0.8 * a, rough=0.45, scale=0.3)
        for k in range(14):
            x = r3.uniform(-56, 56)
            y = -r3.uniform(0, 34) - t * 2.4
            c.ellipse(x, y, 0.8, 0.8, (255, 170, 60) if k % 2 else (255, 230, 120), a, "add")
    return c


def fire_mob(age):
    """멸화에 맞은 몬스터 (위 층, 12): 몸을 감싸는 불혀 다섯 + 섬광 + 불티."""
    c = Canvas(80, 90, 40, 70)
    if age < 3:
        c.star(0, -12, 6, 1.4, 13 - age * 3, CREAM, 1 - age / 3, phase=0.3, core=WHITE)
    grow = fp_ease(min(1.0, age / 3))
    decay = 1.0 - fp_ease(max(0.0, (age - 5) / 7))
    rnd = random.Random(11500)
    tongues = [((k - 1) * 4.5 + rnd.uniform(-1, 1), 2 - abs(k - 1) * 0.6, (14 + rnd.uniform(0, 6) - abs(k - 1) * 3) * grow * decay, 6.5,
                0.0, 200 + k, -math.pi / 2 + (k - 1) * 0.25) for k in range(3)]

    def core(X, Y):
        u, v = X / 8, (Y + 5) / 7
        return 0.2 * grow * decay * np.exp(-(u * u + v * v) * 1.5)
    c.flames(tongues, t=age, stops=fp.FIRE, rough=0.45, boost=core)
    for k in range(5):
        c.ellipse(rnd.uniform(-12, 12), -rnd.uniform(6, 24) - age * 1.6, 0.8, 0.8, (255, 190, 80), 1 - age / 12, "add")
    return c


def fire_scorch(age, back=True):
    """멸화 뒤 땅 층 (64, 폭발 순간부터): 0..19 뒤쪽 큰 불혀(악마 · 몬스터 밑) · 그을린 땅과 식어 가는 금 · 14..40 남은 작은 불 · 불티."""
    c = Canvas(220, 170, 110, 120)
    rx, ry = FIRE_RX * 0.85, FIRE_RY * 0.95
    a = 1.0 - max(0, age - 40) / 24
    heat = max(0.0, 1.0 - age / 44)

    def burnt(X, Y):
        u, v = X / rx, Y / ry
        d = np.sqrt(u * u + v * v)
        n = fp.fbm(X * 0.2 + 3, Y * 0.3, 41, 4)
        I = (1 - d) + (n - 0.5) * 0.7
        rgb, al = fp.bands(I, [(0.0, (46, 30, 26), 0.45), (0.25, (30, 20, 18), 0.7), (0.5, (20, 14, 14), 0.8)])
        crack = np.abs(fp.fbm(X * 0.12, Y * 0.26, 77, 3) - 0.5) < 0.018 + 0.012 * heat
        hot = np.array([255, 70 + 90 * heat, 30], np.float32) * heat + np.array([90, 10, 10], np.float32) * (1 - heat)
        rgb = np.where((crack & (I > 0.05))[..., None], hot, rgb)
        al = np.where(crack & (I > 0.05), np.maximum(al, 0.9 * (0.4 + 0.6 * heat)), al)
        return rgb, al
    c.field((-rx - 8, -ry - 8, rx + 8, ry + 8), burnt, alpha=a)
    if back and age < 20:
        t = age
        grow = fp_ease(min(1.0, t / 3))
        decay = 1.0 - fp_ease(max(0.0, (t - 8) / 11))
        rnd = random.Random(11000)
        _ring_tongues(rnd, 20, FIRE_RX, FIRE_RY, True, 1, 1, 0)  # 앞 불혀와 같은 난수 흐름을 건너뛴다
        tongues = _ring_tongues(rnd, 17, FIRE_RX, FIRE_RY, False, grow, decay, t)
        c.flames(tongues, t=t * 0.9, stops=fp.FIRE, rough=0.55, warp=0.9, boost=_blast_boost(grow, decay))
    if 12 <= age < 44:  # 남은 작은 불
        t = age - 12
        r2 = random.Random(12000)
        for k in range(7):
            th = r2.uniform(0, 6.28)
            r = r2.uniform(0.35, 0.95)
            x, y = math.cos(th) * rx * r, math.sin(th) * ry * r
            if abs(x) < 14 and -6 < y < 6:
                continue
            h = (9 + r2.uniform(0, 5)) * (1 - t / 32)
            c.flame(x, y, h, 5, 0, t=t, seed=300 + k, stops=fp.FIRE, rough=0.5)
    r3 = random.Random(12500)
    for k in range(10):
        if age > 56:
            break
        tt = ((age / 16) + k / 10) % 1.0
        x = r3.uniform(-rx, rx)
        c.ellipse(x, -tt * 30, 0.8, 0.8, (255, 160, 60), math.sin(math.pi * tt) * heat, "add")
    return c


# ── 용암불 ─────────────────────────────────────────────────────────────────────────────────────────────────
LAVA_TEX = 64  # 월드에 정렬된 64 × 64 질감 — 칸 (x & 3, y & 3) 이 그 창을 잘라 쓴다 (이웃 칸이 이어진다)
LAVA_PAD = 3
LAVA_LEVELS = 10
LAVA_SS = 4
_LAVA = {}


def _lava_field():
    """토러스 보로노이 (SS 해상도): (경계까지 거리[게임 px], 칸 번호, 칸 가운데 쪽 방향 x, y)."""
    if "field" in _LAVA:
        return _LAVA["field"]
    S = LAVA_TEX * LAVA_SS
    rnd = random.Random(424242)
    pts = []
    for gy in range(5):
        for gx in range(5):
            pts.append(((gx + 0.5) * LAVA_TEX / 5 + rnd.uniform(-4, 4), (gy + 0.5) * LAVA_TEX / 5 + rnd.uniform(-4, 4)))
    yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
    xx = (xx + 0.5) / LAVA_SS
    yy = (yy + 0.5) / LAVA_SS
    nwx = fp.fbm(xx * 8 / LAVA_TEX, yy * 8 / LAVA_TEX, 81, 3, period=8) - 0.5
    nwy = fp.fbm(xx * 8 / LAVA_TEX, yy * 8 / LAVA_TEX, 82, 3, period=8) - 0.5
    wx = (xx + 1.6 * np.sin(yy * 2 * math.pi / LAVA_TEX) + 0.8 * np.sin((xx + yy) * 6 * math.pi / LAVA_TEX) + nwx * 4.5) % LAVA_TEX
    wy = (yy + 1.4 * np.sin(xx * 4 * math.pi / LAVA_TEX) + 0.6 * np.sin((yy - xx) * 6 * math.pi / LAVA_TEX) + nwy * 4.5) % LAVA_TEX
    d1 = np.full(xx.shape, 1e9, np.float32)
    d2 = np.full(xx.shape, 1e9, np.float32)
    idx = np.zeros(xx.shape, np.int32)
    vx = np.zeros(xx.shape, np.float32)
    vy = np.zeros(xx.shape, np.float32)
    for i, (px, py) in enumerate(pts):
        dx = wx - px
        dx = dx - LAVA_TEX * np.round(dx / LAVA_TEX)
        dy = wy - py
        dy = dy - LAVA_TEX * np.round(dy / LAVA_TEX)
        d = np.sqrt(dx * dx + dy * dy)
        closer = d < d1
        d2 = np.where(closer, d1, np.minimum(d2, d))
        idx = np.where(closer, i, idx)
        vx = np.where(closer, dx, vx)
        vy = np.where(closer, dy, vy)
        d1 = np.where(closer, d, d1)
    border = (d2 - d1) / 2
    n = np.sqrt(vx * vx + vy * vy) + 1e-3
    _LAVA["field"] = (border, idx, vx / n, vy / n, xx, yy)
    return _LAVA["field"]


def _lava_rocks():
    """질감에 박힌 현무암 덩어리 (월드 정렬이라 칸 경계를 넘어도 이어진다)."""
    if "rocks" in _LAVA:
        return _LAVA["rocks"]
    c = Canvas(LAVA_TEX, LAVA_TEX, 0, 0, ss=LAVA_SS)
    rnd = random.Random(515)
    spots = []
    for k in range(5):
        x, y = rnd.uniform(6, 58), rnd.uniform(6, 58)
        r = rnd.uniform(3.4, 5.4)
        c.rock(x, y, r, 1.0, rnd=random.Random(k + 900), base=(34, 28, 32), hi=(88, 78, 80))
        spots.append((x, y, r))
    _LAVA["rocks"] = (c.base.copy(), spots)
    return _LAVA["rocks"]


def lava_texture(heat):
    key = ("tex", round(heat, 4))
    if key not in _LAVA:
        _LAVA[key] = _lava_texture(heat)
    return _LAVA[key]


DRAFT_LAVA = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "costumes", "astra_fx")
DRAFT_LAVA_HD = DRAFT_LAVA + "_hd"  # ★ HD: astra_cut.build_lava 의 3× 퀼트 (lava_cool · lava_hot 192 × 192)
LAVA_HD = fp.HD_SCALE


def _draft_lava():
    """3 차: 예상도 「용암불 냉각」의 판 · 균열을 이어 붙인 64 × 64 질감 (`astra_cut.build_lava`) — (식음, 끓음, 균열까지 거리)."""
    if "draft" not in _LAVA:
        import astra_frames
        try:
            ic, ih = astra_frames.frame("lava_cool", 0), astra_frames.frame("lava_hot", 0)  # B: 들인 질감이 있으면 그것
            cool = ic[0][..., :3].astype(np.float32) if ic else np.asarray(Image.open(os.path.join(DRAFT_LAVA, "lava_cool.png")).convert("RGB"), np.float32)
            hot = ih[0][..., :3].astype(np.float32) if ih else np.asarray(Image.open(os.path.join(DRAFT_LAVA, "lava_hot.png")).convert("RGB"), np.float32)
        except FileNotFoundError:
            _LAVA["draft"] = None
            return None
        crack = (cool[..., 0] > cool[..., 1] + 50) & (cool[..., 0] > 120)
        dist = np.full(crack.shape, 9.0, np.float32)
        k = crack.copy()
        dist[k] = 0
        for dd in range(1, 9):
            p = np.pad(k, 1, mode="wrap")
            k = k | p[:-2, 1:-1] | p[2:, 1:-1] | p[1:-1, :-2] | p[1:-1, 2:]
            dist[k & (dist > dd)] = dd
        _LAVA["draft"] = (cool, hot, crack, dist)
    return _LAVA["draft"]


def _draft_lava_hd():
    """★ HD: (식음, 끓음, 균열) 3× — 없으면 None (그때 용암 바닥은 쌍 없이 1× 다)."""
    if "draft_hd" not in _LAVA:
        try:
            cool = np.asarray(Image.open(os.path.join(DRAFT_LAVA_HD, "lava_cool.png")).convert("RGB"), np.float32)
            hot = np.asarray(Image.open(os.path.join(DRAFT_LAVA_HD, "lava_hot.png")).convert("RGB"), np.float32)
        except FileNotFoundError:
            _LAVA["draft_hd"] = None
            return None
        if cool.shape[0] != LAVA_TEX * LAVA_HD or hot.shape != cool.shape:
            _LAVA["draft_hd"] = None
            return None
        crack = (cool[..., 0] > cool[..., 1] + 50) & (cool[..., 0] > 120)
        _LAVA["draft_hd"] = (cool, hot, crack)
    return _LAVA["draft_hd"]


def _blend_draft(cool, hot, crack, heat):
    if heat >= 0.35:
        u = (heat - 0.35) / 0.65
        return cool + (hot - cool) * u
    dim = 0.5 + 0.5 * heat / 0.35  # 다 식으면 균열이 검붉어진다
    dark = cool * np.array([0.9, 0.75, 0.75], np.float32)
    return np.where(crack[..., None], cool * dim + dark * (1 - dim), cool)


def lava_texture_hd(heat):
    """★ HD: 열 0..1 의 192 × 192 RGB — 1× 와 같은 섞기, 3× 퀼트 위에서 (없으면 None)."""
    key = ("hd", round(heat, 3))
    if key not in _LAVA:
        d = _draft_lava_hd()
        _LAVA[key] = None if d is None else _blend_draft(*d, heat)
    return _LAVA[key]


def _lava_texture(heat):
    """열 0..1 의 64 × 64 RGB (게임 해상도). 예상도 질감이 있으면 그것(식음 ↔ 끓음 — 같은 균열이 달아오르고 식는다),
    없으면 2 차 그림: 빗각 진 현무암 판 · 판 가장자리의 붉은 열기 · 노랑 → 주황 → 빨강 균열 · 박힌 바위."""
    d = _draft_lava()
    if d is not None:
        cool, hot, crack, _ = d
        return _blend_draft(cool, hot, crack, heat)
    border, idx, nx, ny, xx, yy = _lava_field()
    grain = fp.fbm(xx * 16 / LAVA_TEX, yy * 16 / LAVA_TEX, 33, 3, period=16)
    tone = ((idx * 37) % 5).astype(np.float32) * 4
    base = np.stack([46 + tone, 32 + tone, 32 + tone], axis=-1) + (grain[..., None] - 0.5) * 22
    # 빗각: 칸 가운데에서 본 방향이 왼쪽 위(빛)인 가장자리는 밝게, 반대는 어둡게
    lit = -(nx * -0.6 + ny * -0.8)  # 가장자리 점에서 칸 가운데로 가는 방향의 반대 = 바깥
    edge = np.clip(1 - (border - 0.9) / 2.2, 0, 1)
    base = base + (edge * np.clip(lit, 0, 1) * 34)[..., None] - (edge * np.clip(-lit, 0, 1) * 16)[..., None]
    # 판 위의 열기
    hm = fp.fbm(xx * 4 / LAVA_TEX, yy * 4 / LAVA_TEX, 74, 2, period=4)  # 넓은 열 얼룩 — 어떤 판은 더 달아올랐다
    glow = np.clip(1 - border / ((1.6 + 2.6 * heat) * (0.5 + hm)), 0, 1) ** 1.5 * heat * (0.35 + 1.3 * hm)
    base = base + glow[..., None] * np.array([96, 18, 4], np.float32)
    # 균열 띠 (열이 높을수록 넓고 밝다)
    wn = fp.fbm(xx * 12 / LAVA_TEX, yy * 12 / LAVA_TEX, 71, 3, period=12)
    w = (0.3 + 0.42 * heat) * (0.4 + 1.3 * wn)
    pool = (fp.fbm(xx * 12 / LAVA_TEX, yy * 12 / LAVA_TEX, 72, 3, period=12) > 0.64) & (border < 3.0)
    b = np.where(pool, np.minimum(border / w, 0.5 + border * 0.25), border / w)
    hot = [(0.3, (255, 250, 210)), (0.62, (255, 222, 96)), (0.98, (255, 136, 34)), (1.4, (220, 42, 22)), (1.9, (120, 18, 14))]
    cool = [(0.35, (170, 40, 20)), (0.75, (140, 26, 16)), (1.2, (96, 18, 14)), (1.75, (64, 16, 14)), (2.3, (50, 16, 16))]
    rgb = base.copy()
    mix = min(1.0, heat / 0.6)
    for (tb, ch), (_, cc) in reversed(list(zip(hot, cool))):
        col = np.array(ch, np.float32) * mix + np.array(cc, np.float32) * (1 - mix)
        m = b < tb
        rgb = np.where(m[..., None], col, rgb)
    # 균열 노이즈로 끊김 (식으면 더 많이 끊긴다)
    gap = fp.fbm(xx * 24 / LAVA_TEX, yy * 24 / LAVA_TEX, 61, 3, period=24) < 0.26 + 0.2 * (1 - heat)
    rgb = np.where((gap & (b < 2.3))[..., None], base * 0.9 + np.array([30, 4, 0]) * heat, rgb)
    rocks, _ = _lava_rocks()
    ra = rocks[..., 3:4]
    rgb = rocks[..., :3] + rgb * (1 - ra)
    S = LAVA_SS
    out = rgb.reshape(LAVA_TEX, S, LAVA_TEX, S, 3).mean(axis=(1, 3))
    return np.clip(out, 0, 255)


def lava_tile(variant, level, k=1):
    """칸 (tx, ty) = (variant & 3, variant >> 2) 의 창 (16 + 2·PAD)², 가장자리 알파가 들쭉날쭉 (용암 무리의 바깥 테).
    ★ HD: k = 3 이면 같은 창을 3× 퀼트에서 (기준점 PAD·k) — 들쭉날쭉은 HD px 단위라 더 잘다. 3× 퀼트가 없으면 None."""
    tex = lava_texture(level / (LAVA_LEVELS - 1)) if k == 1 else lava_texture_hd(level / (LAVA_LEVELS - 1))
    if tex is None:
        return None
    tx, ty = variant & 3, variant >> 2
    S = (16 + 2 * LAVA_PAD) * k
    big = np.tile(tex, (3, 3, 1))
    x0 = ((tx * 16 - LAVA_PAD) % LAVA_TEX + LAVA_TEX) * k
    y0 = ((ty * 16 - LAVA_PAD) % LAVA_TEX + LAVA_TEX) * k
    win = big[y0:y0 + S, x0:x0 + S]
    rnd = np.random.RandomState(100 + variant)
    yy, xx = (np.mgrid[0:S, 0:S].astype(np.float32) + 0.5) / k
    e = np.minimum(np.minimum(xx, S / k - xx), np.minimum(yy, S / k - yy))
    jag = rnd.uniform(0, 1.6, (S, S)).astype(np.float32)
    alpha = np.clip((e - 0.4 + jag * 0.6) / (LAVA_PAD + 0.2), 0, 1)
    alpha = np.where(e >= LAVA_PAD, 1.0, alpha)
    arr = np.dstack([win, alpha * 255]).astype(np.uint8)
    arr[arr[..., 3] == 0] = 0
    return arr, LAVA_PAD * k, LAVA_PAD * k


def _lava_spots(variant):
    """칸 안의 불 자리 둘 — 균열 위(경계 거리 최소)로 붙인다 (칸 왼쪽 위 기준 게임 px)."""
    d = _draft_lava()
    if d is not None:
        dist = d[3]
        border = np.repeat(np.repeat(dist, LAVA_SS, axis=0), LAVA_SS, axis=1)
    else:
        border, *_ = _lava_field()
    tx, ty = variant & 3, variant >> 2
    rnd = random.Random(300 + variant)
    out = []
    for _ in range(2):
        px, py = rnd.uniform(3, 13), rnd.uniform(4, 14)
        best = None
        for dy in range(-3, 4):
            for dx in range(-3, 4):
                x, y = px + dx, py + dy
                if not (1 <= x <= 15 and 2 <= y <= 15):
                    continue
                wx, wy = (tx * 16 + x) % LAVA_TEX, (ty * 16 + y) % LAVA_TEX
                b = border[int(wy * LAVA_SS), int(wx * LAVA_SS)]
                if best is None or b < best[0]:
                    best = (b, x, y)
        out.append((best[1], best[2]))
    return out


LAVA_FIRE = [(0.05, (90, 10, 10), 1.0), (0.14, (200, 30, 24), 1.0), (0.26, (255, 90, 30), 1.0), (0.4, (255, 160, 50), 1.0),
             (0.55, (255, 220, 110), 1.0), (0.72, (255, 248, 210), 1.0)]


def _lava_flame(lib, k, size):
    c = Canvas(30, 40, 15, 32)
    c.flame(0, 0, size, 3.6 + size * 0.3, 0.0, t=k * 0.8, seed=k % 4, stops=LAVA_FIRE, rough=0.45, warp=0.9)
    c.glow(0, -size * 0.3, size * 0.45, size * 0.4, (255, 120, 40), 0.25, "add")
    return lib.parts_of(c)


def _ember(lib):
    c = Canvas(8, 8, 4, 4)
    c.ellipse(0, 0, 0.75, 0.75, (255, 210, 120), 1.0, "add")
    c.glow(0, 0, 1.4, 1.4, (255, 110, 30), 0.5)
    return lib.parts_of(c)


def _smoke(lib, s):
    c = Canvas(30, 30, 15, 20)
    c.blob(0, -2 - s, 4 + s * 0.9, 3.2 + s * 0.6, fp.SMOKE, seed=s + 90, rough=0.45, scale=0.4)
    return lib.parts_of(c)


_LAVA_PARTS = {}
LAVA_FLAME_TILES = (0, 6, 9, 15)  # 4 × 4 변형 중 불이 서는 칸 (월드 정렬 — 네 칸에 하나꼴, 예상도의 불기둥 수)


def _draft_flame(lib, k, size):
    """3 차: 예상도 「용암불 연소」에서 오린 불기둥 둘(`astra_cut` lava_flame0/1) — 프레임 k 마다 다른 것 · 거울 · 키 · 일렁임."""
    import demon_fx_cut as cx
    try:
        src = cx.cut(f"lava_flame{k % 2}")
    except FileNotFoundError:
        return None
    if (k // 2) % 2:
        src = src.flip()
    s = size / 14.0
    sy = (1.0, 0.84, 1.1, 0.92)[k % 4] * s
    b = src.wobble(k * 1.3, 0.9, ground=0.0, height=18.0, seed=k % 3).scale(0.9 * s + 0.1, sy, pivot=(0.0, 0.0))
    c = cx.canvas_for(b, 6)
    cx.put(c, b)
    c.glow(0, -size * 0.35, size * 0.4, size * 0.35, (255, 120, 40), 0.22, "add")
    return lib.parts_of(c)


def lava_effect(lib, variant):
    """용암 한 칸의 100 프레임: [0] = 땅 층 판 (열 단계 10 · 알파), [1:] = 위 층 (불혀 · 불티 · 연기)."""
    if id(lib) not in _LAVA_PARTS:
        _LAVA_PARTS.clear()
        _LAVA_PARTS[id(lib)] = {
            "flame": {}, "ember": _ember(lib), "smoke": [_smoke(lib, s) for s in range(4)],
        }
    P = _LAVA_PARTS[id(lib)]
    hd = getattr(lib, "hd", False) and _draft_lava_hd() is not None  # ★ HD: 바닥 판의 3× 쌍 (불혀 · 불티 · 연기는 그림판이 준다)
    tiles = []
    for lv in range(LAVA_LEVELS):
        twin = None
        if hd:
            th = lava_tile(variant, lv, LAVA_HD)
            twin = (th[0], th[1], th[2], LAVA_HD)
        tiles.append(lib.sprite(lava_tile(variant, lv), twin))

    def flame_parts(k, size):
        key = (k % 8, size)
        if key not in P["flame"]:
            P["flame"][key] = _draft_flame(lib, k % 8, size) or _lava_flame(lib, k % 8, size)
        return P["flame"][key]
    spots = _lava_spots(variant)
    frames = []
    for age in range(100):
        if age < 6:
            heat = 0.5 + 0.5 * age / 5
        elif age < 55:
            heat = 0.92 + 0.08 * math.sin(age * 0.7 + variant)
        else:
            heat = max(0.06, 1.0 - (age - 55) / 44 * 0.94)
        lv = int(round(heat * (LAVA_LEVELS - 1)))
        alpha = 255
        if age < 4:
            alpha = int(255 * (age + 1) / 4)
        elif age >= 92:
            alpha = int(255 * (100 - age) / 8)
        parts = [(tiles[lv], 0, 0, alpha, 0)]
        if 2 <= age < 80:
            big = (12, 17, 14, 9)[variant % 4]
            size = big if age < 50 else max(3, int(round(big - (age - 50) * big / 30)))
            if age < 6:
                size = min(big, 3 + age * 2)
            for i, (sx, sy) in enumerate(spots):
                if i == 0 and variant not in LAVA_FLAME_TILES:
                    continue
                if i == 1:
                    continue
                sz = size if i == 0 else max(4, size - 4)
                for (s, dx, dy, a, fl) in flame_parts(age + i * 3 + variant, sz):
                    parts.append((s, dx + int(round(sx)), dy + int(round(sy)), a, fl))
        if age % 4 == variant % 4 and age < 90:
            for (s, dx, dy, a, fl) in P["ember"]:
                parts.append((s, dx + 3 + (variant * 5) % 10, dy + 10 - (age % 12), a, fl))
        if 50 <= age < 97 and (age + variant) % 10 < 6:
            si = ((age + variant) // 3) % 4
            for (s, dx, dy, a, fl) in P["smoke"][si]:
                parts.append((s, dx + 8, dy + 4 - ((age + variant) % 10), min(a, 170), fl))
        frames.append(parts)
    return frames


# ── 변신 ───────────────────────────────────────────────────────────────────────────────────────────────────
TRANSFORM_HEIGHT = [2, 2, 2, 2, -3, -8, -13, -18, -22, -25, -26, -26, -26, -26, -26, -26, -20, -13, -6, 0]  # = demon_look
PINK = (255, 110, 210)
RIBBON = [(0.06, (90, 10, 70), 0.9), (0.2, (190, 30, 140), 1.0), (0.4, (246, 70, 190), 1.0), (0.62, (255, 150, 230), 1.0),
          (0.84, (255, 240, 255), 1.0)]
TF_SHARDS = [(i * 2 * math.pi / 9 + 0.35 * (i % 2), 19 + 4 * (i % 3), 4.2 + (i % 3) * 1.1, 0.7 * i) for i in range(9)]


def _body_I(X, Y, h):
    """사람 몸 실루엣의 세기 (머리 원 + 몸통 캡슐 + 다리 둘) — h = 뜀 높이."""
    def capsule(ax, ay, bx, by, r):
        px, py = X - ax, Y - ay
        vx, vy = bx - ax, by - ay
        t = np.clip((px * vx + py * vy) / (vx * vx + vy * vy), 0, 1)
        dx, dy = px - vx * t, py - vy * t
        return 1 - np.sqrt(dx * dx + dy * dy) / r
    head = 1 - np.sqrt(X ** 2 + (Y - (-29 + h)) ** 2) / 5.5
    torso = capsule(0, -22 + h, 0, -9 + h, 7.5)
    legs = np.maximum(capsule(-3, -9 + h, -4, 0 + h, 3.2), capsule(3, -9 + h, 4, 0 + h, 3.2))
    arms = np.maximum(capsule(-7, -20 + h, -10, -10 + h, 2.6), capsule(7, -20 + h, 10, -10 + h, 2.6))
    return np.maximum(np.maximum(head, torso), np.maximum(legs, arms))


def transform_top(age):
    """변신 (위 층, 44) — start1~6: ① 0..7 허리를 도는 분홍 띠 ② 2..23 몸을 덮는 검은 그림자(자홍 테) ③ 4..43 검은 조각이 돌며
    떠오르다 12..23 모이고 24 에 터져 나간다 ④ 12..23 가슴의 붉은 핵 ⑤ 24..31 붉은 번개 테와 섬광 ⑥ 24..43 불티."""
    c = Canvas(170, 170, 85, 125)
    h = TRANSFORM_HEIGHT[age] if age < len(TRANSFORM_HEIGHT) else 0
    if age < 8:  # ① 허리 고리
        a = 1.0 - max(0, age - 4) / 4
        rot = age * 0.95
        cy = -13 + h
        c.crescent(0, cy, 19, 3.6, math.pi + rot, math.pi * 1.95 + rot, RIBBON, ry=6, seed=1, rough=0.12, alpha=0.55 * a, power=0.5)
        c.crescent(0, cy, 19, 4.2, rot, math.pi * 0.95 + rot, RIBBON, ry=6, seed=2, rough=0.12, alpha=a, power=0.5)
        c.glow(0, cy, 18, 6, (220, 40, 160), 0.4 * a, "base")
        for k in range(5):
            t = rot + k * 1.3
            c.star(math.cos(t) * 19, cy + math.sin(t) * 6, 4, 0.4, 2.6, (255, 230, 255), a, phase=0.0)
    if 2 <= age < 24:  # ② 그림자
        a = min(1.0, (age - 2) / 4) * (1.0 - max(0, age - 21) / 3)

        def shadow(X, Y):
            I = _body_I(X, Y, h) + (fp.fbm(X * 0.6 + age, Y * 0.6 - age * 1.3, 3, 3) - 0.5) * 0.42
            rgb, al = fp.bands(I, [(-0.28, (210, 30, 150), 0.7), (-0.14, (130, 10, 90), 0.9), (0.0, (14, 6, 16), 0.96)])
            return rgb, al
        c.field((-20, -42 + h, 20, 6 + h), shadow, alpha=a)
        if age >= 12:  # 붉은 눈
            ea = min(1.0, (age - 12) / 2) * a
            for ex in (-2, 2):
                c.ellipse(ex, -30 + h, 0.9, 0.6, (255, 60, 50), ea)
                c.glow(ex, -30 + h, 1.6, 1.2, (255, 60, 40), ea * 0.8)
    if 4 <= age < 22:  # 분홍 톱니 기운 (start2~3 의 날개)
        a = min(1.0, (age - 4) / 3) * (1.0 - max(0, age - 19) / 3)
        grow = fp_ease(min(1.0, (age - 4) / 8))
        tongues = []
        for sgn in (-1, 1):
            for k in range(3):
                tongues.append((sgn * (6 + k * 5 * grow), -4 + h, (14 - k * 3) * grow + 3, 6, sgn * (4 + k * 3), 400 + k + (sgn > 0) * 5,
                                -math.pi / 2 + sgn * (0.5 + k * 0.25)))
        c.flames(tongues, t=age, stops=RIBBON, rough=0.5, alpha=0.9 * a)
    if 6 <= age < 22:  # 솟는 붉은 번개
        r2 = random.Random(1000 + age)
        for k in range(3):
            if (age + k) % 3 == 0:
                continue
            x0 = r2.uniform(-7, 7)
            pts = c.bolt_pts(x0, -22 + h, x0 + r2.uniform(-10, 10), -50 + h - r2.uniform(0, 10), 3, 0.35, r2)
            c.lightning(pts, 1.2, (120, 0, 20), (255, 60, 50), core=(255, 230, 200), alpha=0.9)
    if 4 <= age < 43:  # ③ 검은 조각
        for i, (th0, r0, size, ph) in enumerate(TF_SHARDS):
            if age < 24:
                r = r0 * (1.0 - 0.6 * fp_ease(max(0.0, (age - 12) / 10)))
                th = th0 + age * 0.3
                x, y = math.cos(th) * r, -17 + h + math.sin(th) * r * 0.55 - min(1.0, (age - 4) / 10) * 7
                a = min(1.0, (age - 4) / 3)
            else:
                t = age - 24
                r = 16 + t * (3.2 + 0.5 * (i % 3))
                th = th0 + 24 * 0.3
                x = math.cos(th) * r
                y = -17 + math.sin(th) * r * 0.6 - 5 * t + 0.42 * t * t
                a = 1.0 - max(0, age - 34) / 9
                if y > 4:
                    continue
            c.shard(x, y, size * (1.6 if age >= 24 else 1.0), th + ph, a, rim=(246, 70, 190))
    if 12 <= age < 24:  # ④ 붉은 핵
        g = min(1.0, (age - 12) / 8)
        cy = -19 + h
        c.glow(0, cy, 3 + 6 * g, 3 + 6 * g, (255, 40, 30), 0.7 + 0.3 * g)
        c.ellipse(0, cy, 1.0 + 1.6 * g, 1.0 + 1.6 * g, WHITE, 1.0)
        if age >= 19:
            c.star(0, cy, 4, 1.2, 8 + (age - 19) * 4, WHITE, 0.95, phase=age * 0.2, core=CREAM)
    if 24 <= age < 32:  # ⑤ 출현: 붉은 번개 테 + 섬광
        t = age - 24
        a = 1.0 - max(0, t - 3) / 5
        c.glow(0, -18, 20 + t * 2, 24 + t * 2, (200, 30, 40), 0.35 * a, "add")
        if t < 3:
            c.star(0, -18, 8, 1.4, 22 - t * 5, (255, 120, 90), 0.95 - t * 0.25, phase=0.2, squash=0.9, rnd=random.Random(5))
            c.glow(0, -18, 7, 9, CREAM, 0.6 - t * 0.2, "add")
        r2 = random.Random(77 + t // 2)
        for k in range(8):
            th = k * 2 * math.pi / 8 + r2.uniform(-0.25, 0.25)
            r_in = 16 + t * 1.8
            x0, y0 = math.cos(th) * r_in, -17 + math.sin(th) * r_in * 1.25
            x1, y1 = math.cos(th) * (r_in + 9 + r2.uniform(0, 7)), -17 + math.sin(th) * (r_in + 9) * 1.25
            pts = c.bolt_pts(x0, y0, x1, y1, 3, 0.4, r2)
            c.lightning(pts, 1.1, (120, 0, 20), (255, 70, 50), core=(255, 240, 210), alpha=0.9 * a)
    if 24 <= age < 43:  # ⑥ 불티
        t = age - 24
        r3 = random.Random(5)
        for k in range(12):
            x = r3.uniform(-28, 28) * (0.4 + t * 0.06)
            y = -r3.uniform(4, 36) + t * t * 0.1 - t * 1.2
            c.ellipse(x, y, 0.8, 0.8, (255, 150, 70) if k % 2 else PINK, 1.0 - max(0, age - 34) / 9, "add")
    return c


def transform_ground(age):
    """변신 땅 층 (44): 발밑의 검은 그림자와 분홍 고리 → 출현(24)에 흙먼지 고리가 퍼지고 돌이 튄다 (start6 착지)."""
    c = Canvas(150, 80, 75, 40)
    if age < 24:
        g = fp_ease(min(1.0, age / 20))
        c.ellipse(0, 0, 10 + 8 * g, 3.6 + 2.2 * g, (20, 4, 18), 0.6 * min(1.0, age / 4))
        c.ellipse(0, 0, 13 + 8 * g, 4.4 + 2.6 * g, (236, 60, 180), 0.7 * min(1.0, max(0, age - 2) / 4), width=1.2)
        c.glow(0, 0, 12 + 6 * g, 4 + 2 * g, (220, 40, 160), 0.35 * min(1.0, age / 6))
        if age >= 12:
            c.glow(0, 0, 9, 3, (255, 40, 30), 0.45 * min(1.0, (age - 12) / 8))
    else:
        t = age - 24
        a = 1.0 - max(0, t - 9) / 10
        rx, ry = 14 + 22 * fp_ease(min(1.0, t / 12)), 5 + 7 * fp_ease(min(1.0, t / 12))
        c.ellipse(0, 0, rx, ry, (60, 40, 30), 0.25 * a)
        r2 = random.Random(31)
        for k in range(14):
            th = k * 2 * math.pi / 14 + r2.uniform(-0.2, 0.2)
            x, y = math.cos(th) * rx, math.sin(th) * ry
            c.blob(x, y - 2, 4.2 + r2.uniform(0, 2) + t * 0.15, 2.8 + t * 0.08, fp.DUST, seed=k * 7 + t // 3, alpha=0.85 * a, scale=0.5)
        if t < 10:
            c.glow(0, 0, 18, 5, (255, 60, 30), 0.55 * (1 - t / 10), "base")
        for k in range(7):
            th = k * 2 * math.pi / 7 + 0.4
            rr = 10 + min(t, 8) * 2.6
            hop = max(0.0, 7 * math.sin(math.pi * min(t, 8) / 8))
            c.rock(math.cos(th) * rr * 1.25, math.sin(th) * rr * 0.4 - hop, 2.4, 1.0 - max(0, t - 14) / 5, rnd=random.Random(k),
                   hot=HOT_RIM if t < 6 else None, **ROCK_FIRE)
    return c


# ── 시험 표지 ──────────────────────────────────────────────────────────────────────────────────────────────
def marker():
    """기준점 확인용 초록 십자 (effect 62)."""
    c = Canvas(21, 21, 10, 10)
    c.stroke([(-8, 0), (8, 0)], 1, (0, 255, 0), 1.0)
    c.stroke([(0, -8), (0, 8)], 1, (0, 255, 0), 1.0)
    return c
