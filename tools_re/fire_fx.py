#!/usr/bin/env python3
"""「악마화」 멸화 화염 5 차 (2026-10-15) — 오린 그림 대신 3D 입자 화염.

사용자: *"현재 악의 사용후 학살화염 사용했을때 뭔가 2d이미지가 나타났다 사라지는 느낌이라 좀 어색한데 이거 좀더 업스케일링 하자,
3d스럽게 실제로 내 캐릭터에서 화염이 터지는듯한 이펙트가 됐으면 좋겠고 화염이펙트 끝쪽에 이상한 디자인들이 나오고있는데 이런거 다
삭제해주고 실제 화염관련 이펙트만 나왔으면 좋겠어."*

옛 그림(3 차 · `demon_fx_cut.fire_blast`)은 예상도 10 장면에서 오린 **한 장**을 키웠다 · 일렁였다 · 노이즈로 부쉈다 — 그래서 「2D 그림이
나타났다 사라지는」 느낌이었고, 오린 그림에는 불 말고도 검은 결정 · 튀는 바위 · HUD 조각 · 땅에 떨어진 검 아이템 · 분홍 흙 얼룩이 같이
들어 있었다(`costumes/astra_fx_hd/fire.png`). 땅 층은 3 초 동안 빛나는 금이 간 그을음 타원을 남겼다.

이 파일은 불을 **3D 공간의 입자**로 계산한다 — 세계 좌표 (x 옆, d 깊이 = 화면 아래쪽이 앞, z 높이), 화면 = (x, d·K − z). 폭발은
악마의 가슴(z = CHEST)에서 터지는 불덩이 · 땅을 따라 번지는 불 고리 · 바깥으로 쏘는 불줄기 · 고리에서 솟는 불혀 · 불티 · 식어 가는 연기
뿐이고, 입자마다 온도(색) · 밀도(노이즈로 뭉게뭉게) 를 남겨 밀도 필드를 색 띠로 칠한다(뜨거운 속은 흰빛 · 노랑 → 식은 바깥은 붉은
테 · 그을린 연기; 차가운 부분만 위왼쪽 빛으로 음영 → 둥근 부피). 깊이가 앞(d > 0)인 입자는 위 층(`fire_blast` — 모든 개체 위),
뒤인 입자는 땅 층(`fire_back` — 모든 개체 밑)에 들어가서 **악마가 불 속에 서 있다**: 뒤쪽 불은 몸에 가리고 앞쪽 불은 몸을 덮는다.
두 층은 같은 그리기 나이로 그려진다(★ 새 효과 `fire_back` = FXG 가 FXT 의 나이를 미리 계산해 그린다 — `DemonPatch::FX_FIRE_BACK`).

효과 (번호 · 길이 = `demon_fx.EFFECTS` 계약):
* `fire_core` (위 층, 선딜 10 틱) · `fire_ground` (땅 층, 선딜 10 틱) — 발밑에서 불씨가 감겨 올라 가슴으로 모이고 발밑에 작은 불 고리가
  붙는다(앞 반쪽 = 위 층 · 뒤 반쪽 = 땅 층);
* `fire_blast` (위 층, 24 그리기) · `fire_back` (땅 층, 같은 24 그리기) — 폭발의 앞 · 뒤 반쪽 + 앞 층의 섬광 · 땅 층의 땅 빛;
* `fire_mob` (위 층, 12 그리기) — 맞은 몬스터를 감싸는 불;
* `fire_scorch` (땅 층, 64 틱) — 폭발 뒤 땅에 남은 작은 불 · 불티 · 옅은 연기 (그을음 · 금 무늬 없음).

    python tools_re/fire_fx.py replace [--src wie_android/src/demon_fx.bin] [--out …]   # 다른 효과는 바이트 그대로 두고 불만 새로
    python tools_re/fire_fx.py preview --out docs/skill/demon_fire_hd                    # 3× 시트 + GIF (실제 게임 배경 · 악마 HD 몸)
"""

import argparse
import functools
import math
import os
import random
import struct
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fxpaint as fp  # noqa: E402
from fxpaint import Canvas  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

K = 0.55  # 땅의 눌림 — 세계 깊이 d → 화면 y (게임의 3/4 시점; 판정 타원 6 × 4 칸보다 납작하게 그린다)
CHEST = 20.0  # 악마 가슴 높이 (발바닥에서, 게임 px) — 폭발이 터지는 자리
RING_R = 74.0  # 땅을 따라 번지는 불 고리의 끝 반지름 (옆 ±74 px → 판정 FIRE_WINDOW 6 칸 = 96 px 안, 「발자국 + 1 칸」)
BLAST_LEN = 24  # = demon_fx.FIRE_BLAST_LEN = DemonPatch::FX_FIRE_BLAST_LEN (그리기 나이)
WINDUP_LEN = 10  # = demon_fx.FIRE_CORE_LEN = DemonPatch::FIRE_WINDUP (틱)
MOB_LEN = 12  # = DemonPatch::FX_FIRE_MOB_LEN (그리기 나이)
TAIL_LEN = 64  # = DemonPatch::FX_SCORCH_LEN (틱)
TICKS_PER_DRAW = 1.3  # 게임의 그리기 한 번 ≈ 1.3 틱 (§8.73 실측) — 꼬리(틱)가 폭발(그리기) 뒤에 이어지게 맞출 때만 쓴다

# ── 색 ─────────────────────────────────────────────────────────────────────────────────────────────────────
# 온도 → 색 (식은 연기 → 그을린 붉은 테 → 빨강 → 주황 → 노랑 → 흰빛)
RAMP_T = np.array([0.0, 0.08, 0.17, 0.28, 0.40, 0.53, 0.66, 0.80, 0.93, 1.10], np.float32)
RAMP_C = np.array([(46, 40, 40), (62, 34, 30), (112, 26, 18), (176, 34, 16), (226, 66, 18), (250, 116, 26), (255, 164, 44),
                   (255, 214, 104), (255, 244, 196), (255, 255, 240)], np.float32)


def ramp(T):
    T = np.clip(T, 0, RAMP_T[-1])
    return np.stack([np.interp(T, RAMP_T, RAMP_C[:, k]) for k in range(3)], axis=-1).astype(np.float32)


def smooth(e0, e1, x):
    t = np.clip((np.asarray(x, np.float32) - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def ease(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


# ── 노이즈 질감 (이음매 없는 256² — 입자마다 자리를 달리해 표본) ─────────────────────────────────────────────────
NT = 256
_TEX = []


def _textures():
    if not _TEX:
        i = np.arange(NT, dtype=np.float32)
        X, Y = np.meshgrid(i / 32.0, i / 32.0)
        for seed in (7, 19, 41, 83):
            n = fp.fbm(X, Y, seed, octaves=5, gain=0.55, period=8)
            n = (n - n.mean()) / (n.std() + 1e-6)
            _TEX.append(np.clip(0.5 + 0.2 * n, 0, 1).astype(np.float32))
    return _TEX


def samp(tex, u, v):
    """이음매 없는 질감의 겹선형 표본 (u, v = 텍셀)."""
    u = np.asarray(u, np.float32)
    v = np.asarray(v, np.float32)
    x0 = np.floor(u)
    y0 = np.floor(v)
    fx = u - x0
    fy = v - y0
    x0 = x0.astype(np.int64) & (NT - 1)
    y0 = y0.astype(np.int64) & (NT - 1)
    x1 = (x0 + 1) & (NT - 1)
    y1 = (y0 + 1) & (NT - 1)
    a = tex[y0, x0] * (1 - fx) + tex[y0, x1] * fx
    b = tex[y1, x0] * (1 - fx) + tex[y1, x1] * fx
    return a * (1 - fy) + b * fy


def _resample(A, X, Y):
    """배열 A 를 (X, Y)(배열 픽셀, 실수)에서 겹선형으로 — 밖은 0."""
    H, W = A.shape
    x0 = np.floor(X).astype(np.int64)
    y0 = np.floor(Y).astype(np.int64)
    fx = (X - x0).astype(np.float32)
    fy = (Y - y0).astype(np.float32)
    P = np.pad(A, 1)

    def at(yy, xx):
        return P[np.clip(yy + 1, 0, H + 1), np.clip(xx + 1, 0, W + 1)]
    return (at(y0, x0) * (1 - fx) + at(y0, x0 + 1) * fx) * (1 - fy) + (at(y0 + 1, x0) * (1 - fx) + at(y0 + 1, x0 + 1) * fx) * fy


# ── 입자 ───────────────────────────────────────────────────────────────────────────────────────────────────
class Puff:
    """불 · 연기 덩어리 하나 — 세계 좌표의 닫힌 식 궤적 (끌림 k 로 느려지는 처음 속도 + 부력/중력), 반지름이 자라고 온도가 식는다.
    kind: "ball" 둥근 덩어리 · "tongue" 위로 길쭉한 불혀 · "jet" 속도 방향으로 늘어난 불줄기 · "ember" 불티 (더하기 빛)."""

    __slots__ = ("p", "v", "k", "born", "life", "r0", "rg", "T0", "tau", "buoy", "seed", "kind", "stretch", "amp", "fade_in")

    def __init__(self, p, v, born, life, r0, rg, T0, tau, k=0.25, buoy=0.0, seed=0, kind="ball", stretch=1.0, amp=1.0, fade_in=0.6):
        self.p, self.v, self.born, self.life = p, v, born, life
        self.r0, self.rg, self.T0, self.tau = r0, rg, T0, tau
        self.k, self.buoy, self.seed, self.kind, self.stretch, self.amp, self.fade_in = k, buoy, seed, kind, stretch, amp, fade_in

    def at(self, t):
        """나이 t 의 (x, d, z, 화면 속도 (vx, vy), 반지름, 온도, 세기) — 태어나기 전 · 죽은 뒤는 None."""
        a = t - self.born
        if a < 0 or a > self.life:
            return None
        k = self.k
        e = (1 - math.exp(-k * a)) / k if k > 0 else a
        ev = math.exp(-k * a)
        x = self.p[0] + self.v[0] * e
        d = self.p[1] + self.v[1] * e
        z = self.p[2] + self.v[2] * e + 0.5 * self.buoy * a * a
        vx, vd, vz = self.v[0] * ev, self.v[1] * ev, self.v[2] * ev + self.buoy * a
        r = self.r0 + self.rg * (1 - math.exp(-a / 2.6)) + 0.25 * a
        if self.kind == "tongue":  # 불혀는 타들어 가며 낮아진다 (커지며 옅어지지 않는다)
            r = (self.r0 + self.rg * (1 - math.exp(-a / 1.6))) * (1 - 0.5 * float(smooth(0.45, 1.0, a / self.life)))
        T = self.T0 * math.exp(-a / self.tau)
        w = min(1.0, (a + 0.3) / self.fade_in) if self.fade_in > 0 else 1.0
        u = a / self.life
        u0 = 0.45 if self.kind == "ball" else 0.6
        if u > u0:
            w *= max(0.0, 1 - (u - u0) / (1 - u0)) ** 1.3
        if self.kind != "tongue":
            w *= (self.r0 + 2.0) / (r + 2.0) * 1.6  # 커지며 옅어진다 (같은 양이 넓게 퍼진다)
        w *= self.amp
        if z < 0:  # 땅 밑으로는 안 간다 (땅에 닿으면 옆으로 퍼진 셈)
            z = 0.0
        return x, d, z, (vx, vd * K - vz), r, T, w


class Field:
    """그림판 하나의 밀도 D · 열 H(= 밀도 × 온도) · 불티 빛 E (SS 해상도)."""

    def __init__(self, c):
        self.c = c
        s = c.ss
        self.shape = (c.h * s, c.w * s)
        self.D = np.zeros(self.shape, np.float32)
        self.H = np.zeros(self.shape, np.float32)
        self.S = np.zeros(self.shape, np.float32)  # 밀도 × 덩어리 음영 (덩어리마다 위 왼쪽이 밝은 공 — 뭉게구름의 입체)
        self.E = np.zeros(self.shape + (3,), np.float32)
        tex = _textures()
        self.tex = tex

    def _win(self, x0, y0, x1, y1):
        c, s = self.c, self.c.ss
        sx0 = max(int(math.floor((x0 + c.ax) * s)), 0)
        sy0 = max(int(math.floor((y0 + c.ay) * s)), 0)
        sx1 = min(int(math.ceil((x1 + c.ax) * s)) + 1, c.w * s)
        sy1 = min(int(math.ceil((y1 + c.ay) * s)) + 1, c.h * s)
        if sx0 >= sx1 or sy0 >= sy1:
            return None
        X = (np.arange(sx0, sx1, dtype=np.float32) + 0.5) / s - c.ax
        Y = (np.arange(sy0, sy1, dtype=np.float32) + 0.5) / s - c.ay
        return sx0, sy0, sx1, sy1, X[None, :], Y[:, None]

    def splat(self, cx, cy, ra, rb, ux, uy, amp, temp, seed, age, taper=0.0, noise=1.0, nf=1.0, rise=1.6):
        """덩어리 하나: 가운데 (cx, cy) · 긴 축 (ux, uy) 방향 반지름 ra · 짧은 축 rb (게임 px). taper > 0 이면 긴 축의 + 쪽(끝)으로
        가늘어진다 (불혀). 노이즈는 덩어리에 붙어 움직이고(자리 = seed) 위로 흘러간다(rise · 나이)."""
        if amp <= 1e-3 or ra <= 0.2 or rb <= 0.2:
            return
        n_u = math.hypot(ux, uy) or 1.0
        ux, uy = ux / n_u, uy / n_u
        R = max(ra, rb) * 2.1
        win = self._win(cx - R, cy - R, cx + R, cy + R)
        if win is None:
            return
        sx0, sy0, sx1, sy1, X, Y = win
        dx = X - cx
        dy = Y - cy
        p = (dx * ux + dy * uy) / ra
        q = (-dx * uy + dy * ux) / rb
        if taper:
            q = q / np.clip(1 - taper * np.clip(p, -0.2, 1.2), 0.15, 2.0)
        g = np.exp(-(p * p + q * q) * 1.35)
        rnd = random.Random(seed)
        ox, oy = rnd.uniform(0, NT), rnd.uniform(0, NT)
        tex = self.tex[seed % 3]
        L = 46.0 * nf / max(ra, rb)
        n = samp(tex, dx * L + ox, dy * L * 0.85 + oy + age * rise * 9.0)
        m = np.clip(1 + noise * 2.2 * (n - 0.5), 0, 2.2)
        d = (amp * g * m).astype(np.float32)
        # 덩어리 하나를 공으로 보고 빛(위 왼쪽)을 받는 쪽을 밝게 — 노이즈 골은 조금 어둡게(주름)
        Rl = max(ra, rb)
        lam = np.clip(-(dx * LIGHT[0] + dy * LIGHT[1]) / Rl, -1.3, 1.3)
        sh = 0.74 + 0.30 * lam + 0.22 * (m - 1.0) * 0.5
        self.D[sy0:sy1, sx0:sx1] += d
        self.H[sy0:sy1, sx0:sx1] += d * temp
        self.S[sy0:sy1, sx0:sx1] += d * sh

    def ember(self, cx, cy, vx, vy, size, col, a):
        """불티 — 빛나는 작은 점이 속도 방향으로 끌린다 (더하기 빛)."""
        if a <= 0.01:
            return
        sp = math.hypot(vx, vy)
        L = min(2.6, 0.25 * sp) + size
        ux, uy = (vx / sp, vy / sp) if sp > 1e-3 else (0.0, -1.0)
        R = L + size * 2
        win = self._win(cx - R, cy - R, cx + R, cy + R)
        if win is None:
            return
        sx0, sy0, sx1, sy1, X, Y = win
        dx = X - cx
        dy = Y - cy
        p = dx * ux + dy * uy
        q = -dx * uy + dy * ux
        # 꼬리는 뒤로 (속도의 반대쪽)
        pp = np.where(p > 0, p / max(size, 0.4), p / L)
        g = np.exp(-(pp * pp + (q / max(size * 0.8, 0.35)) ** 2) * 1.6)
        self.E[sy0:sy1, sx0:sx1] += (g * a)[..., None] * np.asarray(col, np.float32)[None, None, :]

    def warp(self, t, amp=3.2, lift=1.4, fx=2.2, fy=0.9, seed=3):
        """불혀 핥기 — 세로로 늘어난 노이즈로 표본 자리를 흔든다 (윗가장자리가 위로 갈라진 혀가 된다)."""
        c, s = self.c, self.c.ss
        H, W = self.shape
        gx = (np.arange(W, dtype=np.float32) + 0.5) / s - c.ax
        gy = (np.arange(H, dtype=np.float32) + 0.5) / s - c.ay
        GX, GY = np.meshgrid(gx, gy)
        t1, t2 = self.tex[3], self.tex[(seed + 1) % 3]
        wx = (samp(t1, GX * fx * 3 + 11, GY * fy * 3 + t * 14) - 0.5) * 2 * amp
        wy = (samp(t2, GX * fx * 4 + 37, GY * fy * 2 + t * 18) - 0.5) * 2 * amp * 1.6 + lift
        # 땅 가까이는 덜 흔든다 (밑동은 제자리)
        k = smooth(4.0, -10.0, GY)
        PX = (GX + wx * k + c.ax) * s - 0.5
        PY = (GY + wy * k + c.ay) * s - 0.5
        self.D = _resample(self.D, PX, PY).astype(np.float32)
        self.H = _resample(self.H, PX, PY).astype(np.float32)
        self.S = _resample(self.S, PX, PY).astype(np.float32)

    def finish(self, kappa=1.9, bloom=0.5, bloom_r=5.0, shade=True, rim=0.45, glow_col=(255, 132, 44), hot_col=(255, 226, 150),
               relief=0.22):
        """밀도 · 열 → 그림판 (덮기 = 불 · 연기, 더하기 = 뜨거운 곳의 번짐 + 불티).
        색은 띠(게임 도트의 단계 명암): 값 V = 온도 + 덩어리 음영(빛 쪽 +, 그늘 쪽 −) 를 FIRE_BANDS 로 칠한다 — 뭉게뭉게한 덩어리마다
        밝은 윗면 · 어두운 아랫면이 띠로 갈라져 부피가 읽힌다. 식은 곳(온도 < 0.2)은 연기 색(음영만)으로 넘어간다."""
        c, s = self.c, self.c.ss
        D, H = self.D, self.H
        if D.max() > 1e-4:
            a_raw = 1 - np.exp(-kappa * D)
            T = H / np.maximum(D, 1e-5)
            sh = self.S / np.maximum(D, 1e-5) if shade else np.full_like(D, 0.74)
            lit = np.clip((sh - 0.74) / 0.3, -1.5, 1.5)
            edge = 1 - smooth(0.10, 0.6, a_raw)
            V = T + relief * lit * (1 - 0.5 * smooth(0.85, 1.05, T)) - rim * 0.35 * edge
            rgb, _ = fp.smooth_bands(V, FIRE_BANDS)
            smoke, _ = fp.smooth_bands(lit - 0.8 * edge, SMOKE_BANDS)
            ks = smooth(0.10, 0.24, T)[..., None]
            rgb = rgb * ks + smoke * (1 - ks)
            a = smooth(0.08, 0.5, a_raw) * (0.62 + 0.38 * smooth(0.06, 0.22, T))
            c.over(0, 0, np.clip(rgb, 0, 255), a)
            if bloom:
                Eh = D * smooth(0.45, 0.9, T)
                B = fp.box_blur(Eh, bloom_r * s)
                g = 1 - np.exp(-0.9 * B)
                hot = smooth(0.62, 0.95, fp.box_blur(T * (D > 0.05), bloom_r * s * 0.6))
                col = np.asarray(glow_col, np.float32)[None, None, :] * (1 - hot[..., None]) + np.asarray(hot_col, np.float32)[None, None, :] * hot[..., None]
                c.add += col * (g * bloom)[..., None]
        if self.E.max() > 1e-4:
            c.add += np.clip(self.E, 0, 600)


# 불의 띠 (값 V 의 문턱 → 색) — 맨 아래 띠가 그을린 붉은 테
FIRE_BANDS = [(-9.0, (70, 20, 22), 1.0), (0.17, (112, 24, 20), 1.0), (0.28, (168, 32, 18), 1.0), (0.39, (214, 58, 20), 1.0),
              (0.51, (244, 104, 26), 1.0), (0.64, (255, 152, 40), 1.0), (0.78, (255, 204, 86), 1.0), (0.92, (255, 240, 170), 1.0),
              (1.05, (255, 254, 236), 1.0)]
SMOKE_BANDS = [(-9.0, (40, 34, 36), 1.0), (-0.35, (64, 56, 56), 1.0), (0.3, (96, 86, 84), 1.0)]  # 그늘 · 중간 · 빛 받는 면


LIGHT = (-0.45, -0.89)  # 빛 = 위 왼쪽 (화면 방향 단위 벡터)


def _project(x, d, z):
    return x, d * K - z


def _split(d, soft=5.0):
    """깊이 d → 앞 층 몫 (0 = 뒤 · 1 = 앞) — 몸 둘레(±soft)는 섞인다."""
    return float(smooth(-soft, soft, d))


def _render(puffs, t, front_c, back_c, body_hole=0.0, noise=1.0):
    """입자 → 두 층의 필드 (앞 · 뒤). body_hole = 앞 층에서 몸 앞 불을 덜어 내는 몫 (악마가 불 속에서 보이게)."""
    F = Field(front_c) if front_c is not None else None
    B = Field(back_c) if back_c is not None else None
    for P in puffs:
        st = P.at(t)
        if st is None:
            continue
        x, d, z, (svx, svy), r, T, w = st
        sx, sy = _project(x, d, z)
        wf = _split(d)
        a = t - P.born
        if P.kind == "ember":
            col = ramp(np.array([min(1.0, T + 0.25)], np.float32))[0]
            al = w * 1.4
            for fld, ww in ((F, wf), (B, 1 - wf)):
                if fld is not None and ww > 0.02:
                    fld.ember(sx, sy, svx, svy, 0.55 + r * 0.25, col, al * ww)
            continue
        if P.kind == "tongue":
            lean = ((P.seed * 7919) % 100 / 100.0 - 0.5) * 0.3 + 0.08 * math.sin(a * 1.3 + P.seed)
            ra, rb, ux, uy, taper = r * P.stretch, r * 0.62, math.sin(lean), -math.cos(lean), 0.75
            sy -= ra * 0.45  # 밑동이 땅에 (가운데를 위로)
        elif P.kind == "jet":
            sp = math.hypot(svx, svy)
            ux, uy = (svx / sp, svy / sp) if sp > 1e-3 else (0.0, -1.0)
            ra, rb, taper = r * (1 + min(P.stretch, sp * 0.22)), r * 0.7, 0.35
        else:
            ra, rb, ux, uy, taper = r, r * 0.92, 0.0, -1.0, 0.0
        for fld, ww in ((F, wf), (B, 1 - wf)):
            if fld is None or ww < 0.02:
                continue
            amp = w * ww
            if fld is F and body_hole > 0:
                # 몸(발에서 위로 46 px · 옆 ±13 px) 바로 앞의 덩어리는 옅게 — 악마가 불 속에서 보이게
                inb = math.exp(-((sx / 15.0) ** 2) - (((sy + 22) / 26.0) ** 2))
                amp *= 1 - body_hole * inb * smooth(0, 10, d)
            fld.splat(sx, sy, ra, rb, ux, uy, amp, T, P.seed, a, taper=taper, noise=noise)
    return F, B


# ── 폭발 (24 그리기 — 위 층 · 땅 층이 같은 나이) ─────────────────────────────────────────────────────────────
_BLAST = []


def _sph(rnd, up=0.25, flat=0.75):
    """무작위 방향 (위로 치우치고 옆으로 납작)."""
    while True:
        x, y, z = rnd.gauss(0, 1), rnd.gauss(0, 1), rnd.gauss(0, 1)
        n = math.sqrt(x * x + y * y + z * z)
        if n > 1e-3:
            break
    x, y, z = x / n, y / n, z / n * flat + up
    n = math.sqrt(x * x + y * y + z * z)
    return x / n, y / n, z / n


RING_K = 0.26  # 고리의 끌림 — 8 그리기 뒤 RING_R 에 닿는다
RING_BORN = 0.9


def ring_front_time(r):
    """불 고리의 앞머리가 반지름 r 에 닿는 나이 (그리기)."""
    Rr = RING_R * 0.92
    u = (r - 6.0) * (1 - math.exp(-8 * RING_K)) / Rr
    if u <= 0:
        return RING_BORN
    return RING_BORN - math.log(max(1e-3, 1 - u)) / RING_K


def blast_puffs():
    if _BLAST:
        return _BLAST
    rnd = random.Random(20261015)
    P = []
    # ⓪ 가슴에서 터지는 첫 불덩이 — 하얗게 달아오른 작은 덩어리가 처음 몇 그리기 동안 부풀며 식는다
    for i in range(10):
        dx, dd, dz = _sph(rnd, up=0.2, flat=0.9)
        sp = rnd.uniform(1.0, 3.0)
        P.append(Puff((dx * 2, dd * 2, CHEST + dz * 2), (dx * sp, dd * sp, dz * sp), -0.3 + rnd.uniform(0, 0.3), rnd.uniform(5, 7),
                      r0=rnd.uniform(3.6, 5.0), rg=rnd.uniform(5, 7), T0=1.08, tau=4.5, k=0.35, buoy=0.1, seed=900 + i, amp=0.95,
                      fade_in=0.25))
    # ① 불덩이 — 처음 한 그리기 동안 터져 나와 바깥으로 구르며 솟는 덩어리 (속이 가장 뜨겁고 느리다)
    for i in range(56):
        dx, dd, dz = _sph(rnd, up=0.3, flat=0.8)
        sp = rnd.uniform(2.2, 7.4) * (1.0 if i % 3 else 1.15)
        born = rnd.uniform(0, 1.0)
        hot = 1 - sp / 9.0
        P.append(Puff((dx * 2, dd * 2, CHEST + dz * 2), (dx * sp, dd * sp, dz * sp * 0.9), born, rnd.uniform(13, 18),
                      r0=rnd.uniform(3.4, 5.0), rg=rnd.uniform(4.5, 7.5), T0=rnd.uniform(0.88, 0.96) + 0.12 * hot, tau=rnd.uniform(7.5, 10) + 3 * hot,
                      k=0.3, buoy=rnd.uniform(0.14, 0.26), seed=1000 + i, amp=rnd.uniform(0.55, 0.85)))
    # ② 땅을 따라 번지는 불 고리 — 발밑에서 바깥으로 구르는 덩어리
    n_ring = 40
    for i in range(n_ring):
        th = (i + rnd.uniform(0.1, 0.9)) * 2 * math.pi / n_ring
        R = RING_R * rnd.uniform(0.86, 1.0)
        sp = R * RING_K / (1 - math.exp(-RING_K * 8))
        P.append(Puff((math.cos(th) * 6, math.sin(th) * 6, 3.0), (math.cos(th) * sp, math.sin(th) * sp, 0.5), RING_BORN + rnd.uniform(-0.4, 0.4),
                      rnd.uniform(11, 15), r0=rnd.uniform(3.2, 4.4), rg=rnd.uniform(3.5, 5.5), T0=rnd.uniform(0.92, 1.0), tau=rnd.uniform(10, 13),
                      k=RING_K, buoy=rnd.uniform(0.03, 0.07), seed=2000 + i, amp=0.8))
    # ③ 바깥으로 쏘는 불줄기 — 처음 몇 그리기만 (터지는 힘)
    for i in range(14):
        dx, dd, dz = _sph(rnd, up=0.45, flat=0.7)
        sp = rnd.uniform(9, 13)
        P.append(Puff((dx * 4, dd * 4, CHEST + dz * 4), (dx * sp, dd * sp, dz * sp), rnd.uniform(0.0, 0.7), rnd.uniform(4, 6),
                      r0=rnd.uniform(2.0, 2.8), rg=1.5, T0=1.05, tau=4.0, k=0.36, buoy=0.0, seed=3000 + i, kind="jet", stretch=2.6,
                      amp=0.75, fade_in=0.35))
    # ④ 고리의 앞머리가 지나는 순간 그 자리에서 솟는 불혀 (가장자리일수록 크다 — 불의 벽)
    for i in range(52):
        th = rnd.uniform(0, 2 * math.pi)
        rr = rnd.uniform(0.45, 1.0) ** 0.7
        R = RING_R * 0.92 * rr
        born = ring_front_time(R) + rnd.uniform(-0.3, 0.5)
        big = 0.75 + 0.45 * rr
        P.append(Puff((math.cos(th) * R, math.sin(th) * R, 0.0), (rnd.uniform(-0.4, 0.4), rnd.uniform(-0.3, 0.3), rnd.uniform(1.4, 2.6)),
                      born, rnd.uniform(7, 12) * (0.7 + 0.4 * rr), r0=rnd.uniform(2.8, 4.0) * big, rg=rnd.uniform(1.0, 2.0),
                      T0=rnd.uniform(0.9, 1.02), tau=rnd.uniform(9, 12), k=0.12, buoy=0.03, seed=4000 + i, kind="tongue",
                      stretch=rnd.uniform(1.8, 2.6), amp=0.9, fade_in=0.9))
    # ⑤ 식어 가는 연기 — 불덩이 위로 늦게 솟는다 (옅게)
    for i in range(10):
        th = rnd.uniform(0, 2 * math.pi)
        rr = rnd.uniform(0, 20)
        P.append(Puff((math.cos(th) * rr, math.sin(th) * rr * 0.6, CHEST + rnd.uniform(18, 28)), (rnd.uniform(-0.5, 0.5), 0.0, rnd.uniform(0.8, 1.4)),
                      rnd.uniform(10, 14), rnd.uniform(9, 12), r0=rnd.uniform(4.5, 6.5), rg=rnd.uniform(4, 6), T0=0.16, tau=5.0, k=0.1,
                      buoy=0.05, seed=5000 + i, amp=0.3, fade_in=3.0))
    # ⑥ 불티 — 사방으로 튀어 떨어지는 빛점
    for i in range(40):
        dx, dd, dz = _sph(rnd, up=0.55, flat=0.8)
        sp = rnd.uniform(4, 10)
        P.append(Puff((dx * 6, dd * 6, CHEST + dz * 6), (dx * sp, dd * sp, dz * sp), rnd.uniform(1.0, 4.0), rnd.uniform(8, 16),
                      r0=rnd.uniform(0.5, 0.9), rg=0.0, T0=rnd.uniform(0.7, 0.95), tau=rnd.uniform(8, 14), k=0.2,
                      buoy=-rnd.uniform(0.10, 0.2), seed=6000 + i, kind="ember", amp=0.8, fade_in=0.3))
    _BLAST.extend(P)
    return _BLAST


BLAST_BOX = (-104, -128, 104, 60)  # 그림판 (발바닥 기준 게임 px)


def _canvas(box):
    x0, y0, x1, y1 = box
    return Canvas(x1 - x0, y1 - y0, -x0, -y0)


def blast_heat(t):
    """폭발 전체의 밝기 (섬광 · 땅 빛)."""
    return ease(min(1.0, (t + 0.6) / 1.4)) * math.exp(-max(0.0, t - 2.0) / 6.5)


@functools.lru_cache(maxsize=None)
def blast_layers(t):
    """나이 t (그리기) 의 (위 층, 땅 층) 그림판."""
    front, back = _canvas(BLAST_BOX), _canvas(BLAST_BOX)
    F, B = _render(blast_puffs(), t, front, back, body_hole=0.7 * smooth(1.5, 4.0, t))
    for fld in (F, B):
        fld.warp(t, amp=2.6 + 0.12 * t)
        fld.finish(bloom=0.34 * (0.35 + 0.65 * blast_heat(t)), bloom_r=4.0)
    h = blast_heat(t)
    # 섬광 — 가슴에서 (위 층, 처음 세 그리기)
    if t < 3:
        k = (1.0, 0.75, 0.35)[int(t)]
        front.glow(0, -CHEST, 6 + 6 * t, 6 + 5 * t, (255, 236, 190), 0.7 * k, "add")
        front.glow(0, -CHEST, 16 + 9 * t, 13 + 7 * t, (255, 120, 40), 0.4 * k, "add")
    # 땅 빛 — 땅 층 (불 밑의 풀이 붉게 달아오른다)
    R = 24 + 58 * ease(min(1.0, t / 7))
    back.glow(0, 2, R, R * K, (255, 108, 34), 0.42 * h, "add")
    back.glow(0, 2, R * 0.5, R * 0.5 * K, (255, 190, 90), 0.3 * h, "add")
    return front, back


def fire_blast(age):
    return blast_layers(age)[0]


def fire_back(age):
    return blast_layers(age)[1]


# ── 선딜 (10 틱 — 위 층 · 땅 층이 같은 틱 나이) ────────────────────────────────────────────────────────────────
_WIND = []


def windup_puffs():
    if _WIND:
        return _WIND
    rnd = random.Random(20261016)
    P = []
    # 발밑에서 감겨 올라 가슴으로 모이는 불씨 (나선)
    for i in range(14):
        P.append(("wisp", rnd.uniform(0, 2 * math.pi), rnd.uniform(26, 36), rnd.uniform(-1.5, 3.0), rnd.uniform(0.9, 1.25), 7000 + i))
    # 발밑의 작은 불 고리
    for i in range(18):
        P.append(("ring", (i + rnd.uniform(0.2, 0.8)) * 2 * math.pi / 18, rnd.uniform(17, 23), rnd.uniform(-1.0, 1.5), rnd.uniform(0.85, 1.2), 7100 + i))
    _WIND.extend(P)
    return _WIND


WIND_BOX = (-48, -64, 48, 28)


@functools.lru_cache(maxsize=None)
def windup_layers(age):
    front, back = _canvas(WIND_BOX), _canvas(WIND_BOX)
    F, B = Field(front), Field(back)
    g = ease(age / (WINDUP_LEN - 1))
    for kind, th0, R0, lag, sc, seed in windup_puffs():
        if kind == "wisp":
            u = (age - lag) / 8.5  # 0 → 1 = 땅에서 가슴까지
            if u < 0 or u > 1.08:
                continue
            for j in range(4):  # 꼬리 (앞이 가장 크다)
                uu = u - j * 0.06
                if uu < 0:
                    continue
                th = th0 + uu * 4.2
                uc = min(1.0, uu)
                R = R0 * (1 - uc) ** 1.15 + 2
                x, d, z = math.cos(th) * R, math.sin(th) * R, 2 + (CHEST - 2) * uc ** 0.9
                sx, sy = _project(x, d, z)
                wf = _split(d, 4.0)
                amp = 0.62 * sc * (1 - j * 0.24) * min(1.0, u * 4) * (1 - max(0.0, u - 0.92) / 0.16)
                r = (2.2 + 1.2 * (1 - uu)) * sc * (1 - j * 0.15)
                T = 0.62 + 0.3 * uu
                for fld, ww in ((F, wf), (B, 1 - wf)):
                    if ww > 0.02:
                        fld.splat(sx, sy, r * 1.5, r * 0.8, -math.sin(th), math.cos(th) * K - 0.6, amp * ww, T, seed + j, age, taper=0.3)
        else:
            on = smooth(1.5 + lag, 4.5 + lag, age)
            if on <= 0.01:
                continue
            x, d = math.cos(th0) * R0, math.sin(th0) * R0
            sx, sy = _project(x, d, 0.0)
            h = (2.6 + 4.2 * g) * sc * on
            wf = _split(d, 3.0)
            hole = 1 - 0.75 * math.exp(-((sx / 12.0) ** 2)) * smooth(0, 8, d)  # 몸 앞은 낮게 (몸이 보이게)
            for fld, ww in ((F, wf * hole), (B, 1 - wf)):
                if ww > 0.02:
                    fld.splat(sx, sy - h * 0.45, h, 2.2 * sc, 0.0, -1.0, 0.9 * ww * on, 0.7 + 0.12 * g, seed, age, taper=0.8)
    # 가슴의 불씨 — 커지며 하얗게
    core = 0.25 + 0.75 * g
    F.splat(0, -CHEST, 1.8 + 2.6 * g, 1.6 + 2.4 * g, 0, -1, 1.0 * core, 0.9 + 0.2 * g, 7300, age, noise=0.5)
    for fld in (F, B):
        fld.warp(age * 0.7, amp=1.4, lift=0.6)
        fld.finish(bloom=0.55, bloom_r=3.5, rim=0.35)
    front.glow(0, -CHEST, 5 + 9 * g, 5 + 8 * g, (255, 150, 60), 0.35 + 0.5 * g, "add")
    if age >= WINDUP_LEN - 2:
        front.glow(0, -CHEST, 4 + 3 * (age - 7), 4 + 3 * (age - 7), (255, 244, 210), 0.6, "add")
    back.glow(0, 2, 20 + 10 * g, (20 + 10 * g) * K, (255, 96, 30), 0.3 + 0.35 * g, "add")
    return front, back


def fire_core(age):
    return windup_layers(age)[0]


def fire_ground(age):
    return windup_layers(age)[1]


# ── 맞은 몬스터 (12 그리기, 위 층) ────────────────────────────────────────────────────────────────────────────
_MOB = []


def mob_puffs():
    if _MOB:
        return _MOB
    rnd = random.Random(20261017)
    P = []
    for i in range(14):  # 몸을 감싸는 불
        dx, dd, dz = _sph(rnd, up=0.5, flat=0.6)
        sp = rnd.uniform(1.0, 3.2)
        P.append(Puff((dx * 4, dd * 3, 9 + dz * 5), (dx * sp, dd * sp, dz * sp + 0.6), rnd.uniform(0.4, 2.2), rnd.uniform(6, 9.5),
                      r0=rnd.uniform(3.0, 4.4), rg=rnd.uniform(2.0, 3.5), T0=rnd.uniform(0.7, 0.8), tau=rnd.uniform(7.0, 9.0), k=0.3,
                      buoy=0.12, seed=8000 + i, amp=0.6))
    for i in range(12):  # 몸을 타고 오르는 불혀
        th = (i + rnd.uniform(0.2, 0.8)) * 2 * math.pi / 12
        R = rnd.uniform(4, 9)
        P.append(Puff((math.cos(th) * R, math.sin(th) * R, rnd.uniform(0, 8)), (0.0, 0.0, rnd.uniform(1.4, 2.4)), rnd.uniform(0.4, 2.5),
                      rnd.uniform(6, 9), r0=rnd.uniform(2.4, 3.4), rg=1.5, T0=rnd.uniform(0.78, 0.9), tau=7.0, k=0.1, seed=8100 + i,
                      kind="tongue", stretch=rnd.uniform(1.9, 2.5), amp=0.95, fade_in=1.0))
    for i in range(18):
        dx, dd, dz = _sph(rnd, up=0.7, flat=0.7)
        sp = rnd.uniform(2.5, 6)
        P.append(Puff((dx * 3, dd * 3, 10 + dz * 3), (dx * sp, dd * sp, dz * sp), rnd.uniform(0.5, 3.5), rnd.uniform(6, 10),
                      r0=0.8, rg=0.0, T0=0.9, tau=9, k=0.18, buoy=-0.18, seed=8200 + i, kind="ember"))
    _MOB.extend(P)
    return _MOB


MOB_BOX = (-34, -62, 34, 18)


def fire_mob(age):
    c = _canvas(MOB_BOX)
    F, _ = _render(mob_puffs(), age, c, None)
    F.warp(age, amp=1.6, lift=0.8)
    F.finish(bloom=0.3, bloom_r=3.0, rim=0.4)
    if age < 2:
        c.glow(0, -10, 8, 8, (255, 160, 80), 0.4 * (1 - age / 2), "add")
    return c


# ── 꼬리 (64 틱, 땅 층) — 땅에 남아 타들어 가는 작은 불 · 불티 · 옅은 연기 ─────────────────────────────────────────
_TAIL = []


def tail_fires():
    if _TAIL:
        return _TAIL
    rnd = random.Random(20261018)
    fires = []
    for i in range(11):
        while True:
            th = rnd.uniform(0, 2 * math.pi)
            rr = rnd.uniform(0.32, 0.95) * RING_R
            x, d = math.cos(th) * rr, math.sin(th) * rr
            if not (abs(x) < 18 and abs(d) < 12):
                break
        start = rnd.uniform(9, 15)
        end = rnd.uniform(36, 56)
        fires.append((x, d, rnd.uniform(0.75, 1.15), start, end, 9000 + i * 10))
    _TAIL.extend(fires)
    return _TAIL


TAIL_BOX = (-96, -70, 96, 54)


TAIL_FIRE_BOX = (-12, -46, 12, 7)  # 작은 불 하나의 그림판 (그 불의 밑동 기준)


def _tail_fire(age, sc, start, end, seed):
    """꼬리의 작은 불 하나 (밑동 = (0, 0)) — 혀 셋이 번갈아 솟고 · 옅은 연기 한 줄기 · 불티 셋 · 밑의 땅 빛. 없으면 None."""
    if age < start - 2 or age > end + 2:
        return None
    on = float(smooth(start - 2, start + 4, age) * (1 - smooth(end - 10, end + 2, age)))
    if on <= 0.01:
        return None
    c = _canvas(TAIL_FIRE_BOX)
    F = Field(c)
    life = (age - start) / max(1.0, end - start)
    h = (9 + 5 * sc) * (1 - 0.55 * max(0.0, life)) * on
    for j in range(3):
        ph = age * 0.55 + j * 2.1 + seed
        hh = h * (0.7 + 0.3 * math.sin(ph)) * (1.0, 0.75, 0.6)[j]
        ox = (j - 1) * 2.6 * sc
        F.splat(ox, -hh * 0.45, hh * 0.9, 2.4 * sc, 0.0, -1.0, 0.9 * on, 0.64 + 0.12 * (1 - life), seed + j, age * 0.6, taper=0.8)
    for j in range(2):  # 연기 한 줄기 (옅게, 위로)
        u = ((age * 0.045) + j * 0.5 + seed * 0.13) % 1.0
        F.splat(math.sin(u * 5 + seed) * 2.5, -10 - u * 24, 2.6 + u * 4, 2.4 + u * 4, 0, -1, 0.22 * on * math.sin(math.pi * u), 0.12,
                seed + 5 + j, age * 0.3, noise=1.2)
    rr = random.Random(seed)
    for j in range(3):  # 불티
        u = ((age / 14.0) + j / 3.0 + rr.random()) % 1.0
        ex = rr.uniform(-4, 4) + math.sin(u * 6 + j) * 2.5
        F.ember(ex, -4 - u * 30, 0.0, -2.0, 0.6, (255, 170, 70), on * math.sin(math.pi * u) * 1.2)
    F.warp(age * 0.6, amp=1.2, lift=0.4)
    F.finish(bloom=0.45, bloom_r=3.0, rim=0.35)
    c.glow(0, 1, 8, 4, (255, 100, 30), 0.32 * on * (1 - 0.5 * max(0.0, life)), "add")
    return c


def fire_scorch(age):
    """폭발 뒤의 땅 층 (틱 나이) — 고리 자리에 남아 타들어 가는 작은 불들. 불마다 따로 그린 조각 [(그림판, dx, dy)] — 한 장에 그리면
    스프라이트가 모든 불을 감싸는 큰 상자가 된다 (HD 쌍의 메모리)."""
    out = []
    for (x, d, sc, start, end, seed) in tail_fires():
        c = _tail_fire(age, sc, start, end, seed)
        if c is not None:
            sx, sy = _project(x, d, 0.0)
            out.append((c, int(round(sx)), int(round(sy))))
    return out


# ── 라이브러리 교체 ─────────────────────────────────────────────────────────────────────────────────────────
FIRE_EFFECTS = {"fire_core": fire_core, "fire_ground": fire_ground, "fire_blast": fire_blast, "fire_mob": fire_mob,
                "fire_scorch": fire_scorch, "fire_back": fire_back}


def pieces(name, age):
    """효과 name 의 나이 age → [(그림판, dx, dy)] (그림판 하나를 돌려주는 효과는 (c, 0, 0) 하나)."""
    r = FIRE_EFFECTS[name](age)
    return r if isinstance(r, list) else [(r, 0, 0)]


def parts(lib, name, age):
    """라이브러리 부품 (`demon_fx.Library.parts_of` 를 조각마다)."""
    out = []
    for (c, dx, dy) in pieces(name, age):
        out += lib.parts_of(c, dx, dy)
    return out


def read_raw(path):
    """bin → (sprites [(RGBA, ax, ay)], effects [(name, frames)], qtwins {sprite: (idx HxW uint8, pal n×4, ax, ay, scale)}) —
    쌍은 저장된 색인 · 팔레트 그대로 (다시 양자화하지 않는다 = 다른 효과의 HD 그림이 바이트 그대로 남는다)."""
    import io
    import demon_fx
    sprites, effects = demon_fx.read_bin(path)
    with open(path, "rb") as fh:
        b = fh.read()
    ver, plen = struct.unpack_from("<II", b, 4)
    qtwins = {}
    if ver != 2:
        return sprites, effects, qtwins
    at = 12 + plen
    (n,) = struct.unpack_from("<I", b, at)
    at += 4 + 12 * n
    (ne,) = struct.unpack_from("<I", b, at)
    at += 4
    for _ in range(ne):
        nl = b[at]
        at += 1 + nl
        (nf,) = struct.unpack_from("<I", b, at)
        at += 4
        for _ in range(nf):
            (np_,) = struct.unpack_from("<H", b, at)
            at += 2 + 8 * np_
    (hlen,) = struct.unpack_from("<I", b, at)
    at += 4
    hd_atlas = np.asarray(Image.open(io.BytesIO(b[at:at + hlen])).convert("L")) if hlen else None
    at += hlen
    (nh,) = struct.unpack_from("<I", b, at)
    at += 4
    for _ in range(nh):
        s, scale, ncol, x, y, w, h, ax, ay = struct.unpack_from("<HBBHHHHhh", b, at)
        at += 16
        pal = np.frombuffer(b[at:at + 4 * ncol], np.uint8).reshape(ncol, 4).copy()
        at += 4 * ncol
        qtwins[s] = (hd_atlas[y:y + h, x:x + w].copy(), pal, ax, ay, scale)
    assert at == len(b)
    return sprites, effects, qtwins


def replace(src, out):
    """지금 bin 의 다른 효과(1× 스프라이트 · HD 쌍의 색인 · 팔레트 그대로)는 두고 불 효과만 새로 + `fire_back` 을 계약 자리에."""
    import demon_fx
    fp.set_ss(demon_fx.HD)
    sprites, effects, qtwins = read_raw(src)
    names = [n for n, _ in effects]
    want = [n for n, _ in demon_fx.EFFECTS]
    missing = [n for n in want if n not in names]
    assert missing in ([], ["fire_back"]), f"bin 의 효과 목록이 EFFECTS 와 다르다: {missing}"
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
        if name in FIRE_EFFECTS:
            lib.add_effect(name, [parts(lib, name, a) for a in range(lens[name])])
        else:
            lib.add_effect(name, [keep(p) for p in by[name]])
    size, shape, n, hd_shape, twins = lib.write(out)
    print(f"{out}: {size:,} B · atlas {shape[1]}×{shape[0]} · {n} sprites · {len(lib.effects)} effects · HD twins {twins}"
          + (f" (atlas {hd_shape[1]}×{hd_shape[0]})" if hd_shape else ""))


# ── 미리보기 (3× — 앱의 HD 합성과 같은 규칙: 쌍을 최근접으로 · 덮기/더하기) ─────────────────────────────────────────────
def _blend_twin(canvas, twin, x, y, alpha=255, add=False):
    arr, tax, tay, s = twin
    h, w = arr.shape[:2]
    ox, oy = x - tax, y - tay
    H, W = canvas.shape[:2]
    x0, y0 = max(0, ox), max(0, oy)
    x1, y1 = min(W, ox + w), min(H, oy + h)
    if x0 >= x1 or y0 >= y1:
        return
    sub = arr[y0 - oy:y1 - oy, x0 - ox:x1 - ox].astype(np.float32)
    sa = sub[..., 3:4] / 255.0 * (alpha / 255.0)
    if add:
        canvas[y0:y1, x0:x1] = np.minimum(255, canvas[y0:y1, x0:x1] + sub[..., :3] * sa)
    else:
        canvas[y0:y1, x0:x1] = sub[..., :3] * sa + canvas[y0:y1, x0:x1] * (1 - sa)


def draw_hd(canvas, sprites, effects, twins, eid, age, x, y, k=3):
    """3× 캔버스에 효과 하나 (기준점 = 게임 px (x, y)) — 쌍이 있으면 쌍, 없으면 1× 를 k 배 최근접."""
    frames = effects[eid][1]
    if age < 0 or age >= len(frames):
        return
    for (s, dx, dy, a, fl) in frames[age]:
        if s in twins:
            tw = twins[s]
            if tw[3] != k:  # ★★ 2026-10-17 a twin at another scale (`bolt_fx` 의 2× 쌍) — nearest to k
                arr_, tax, tay, sc = tw
                big = np.asarray(Image.fromarray(arr_).resize((arr_.shape[1] * k // sc, arr_.shape[0] * k // sc), Image.NEAREST))
                tw = (big, tax * k // sc, tay * k // sc, k)
            if fl & 1:  # FLIP: the twin mirrored about its anchor
                tw = (tw[0][:, ::-1], tw[0].shape[1] - 1 - tw[1], tw[2], tw[3])
            _blend_twin(canvas, tw, (x + dx) * k, (y + dy) * k, a, bool(fl & 2))
        else:
            arr, ax, ay = sprites[s]
            big = np.asarray(Image.fromarray(arr).resize((arr.shape[1] * k, arr.shape[0] * k), Image.NEAREST))
            _blend_twin(canvas, (big, ax * k, ay * k, k), (x + dx) * k, (y + dy) * k, a, bool(fl & 2))


def preview(out, src=None, k=3):
    import demon_fx as d
    path = src or d.OUT_BIN
    sprites, effects, twins = d.read_bin_hd(path)
    E = {n: i for i, (n, _) in enumerate(effects)}
    assert "fire_back" in E, "fire_back 이 없는 bin — replace 먼저"
    os.makedirs(out, exist_ok=True)
    bg = d.real_background()[:150, :176]
    W, H = 176, 150
    FX, FY = 88, 104
    big_bg = np.asarray(Image.fromarray(bg).resize((W * k, H * k), Image.NEAREST)).astype(np.float32)
    mob_img, (mfx, mfy) = d.real_monster()
    mob_big = np.asarray(mob_img.resize((mob_img.width * k, mob_img.height * k), Image.NEAREST))
    mobs = [(FX + 58, FY - 26), (FX - 62, FY + 8), (FX + 30, FY + 34)]
    demon_im, ds = d.demon_any("roar", d.DOWN)
    dem = np.asarray(demon_im.convert("RGBA"))
    if ds != k:
        dem = np.asarray(demon_im.resize((demon_im.width * k // ds, demon_im.height * k // ds), Image.NEAREST))
    dem_feet = (-d.DEMON_OFF[0] * k, -d.DEMON_OFF[1] * k)

    def entities(c, mob_on=True):
        ents = [(FY, "demon")] + ([(my, ("mob", mx)) for (mx, my) in mobs] if mob_on else [])
        for _, e in sorted(ents, key=lambda q: q[0]):
            if e == "demon":
                _blend_twin(c, (dem, dem_feet[0], dem_feet[1], k), FX * k, FY * k)
            else:
                mx = e[1]
                my = [m[1] for m in mobs if m[0] == mx][0]
                _blend_twin(c, (mob_big, mfx * k, mfy * k, k), mx * k, my * k)

    seq = []  # (제목, [(층, 효과, 나이, x, y)])
    for a in range(WINDUP_LEN):
        seq.append((f"선딜 {a}", [("g", "fire_ground", a, FX, FY), ("t", "fire_core", a, FX, FY)]))
    n_draws = BLAST_LEN + int(math.ceil((TAIL_LEN - BLAST_LEN * TICKS_PER_DRAW) / TICKS_PER_DRAW)) + 1
    for i in range(n_draws):
        tick = int(round(i * TICKS_PER_DRAW))
        lay = [("g", "fire_scorch", tick, FX, FY)]
        if i < BLAST_LEN:
            lay += [("g", "fire_back", i, FX, FY), ("t", "fire_blast", i, FX, FY)]
            if i < MOB_LEN:
                lay += [("t", "fire_mob", i, mx, my) for (mx, my) in mobs]
        seq.append((f"폭발 {i}" if i < BLAST_LEN else f"꼬리 틱 {tick}", lay))
    frames = []
    for title, lay in seq:
        c = big_bg.copy()
        for (l, name, age, x, y) in lay:
            if l == "g":
                draw_hd(c, sprites, effects, twins, E[name], age, x, y, k)
        entities(c)
        for (l, name, age, x, y) in lay:
            if l == "t":
                draw_hd(c, sprites, effects, twins, E[name], age, x, y, k)
        frames.append((title, np.clip(c, 0, 255).astype(np.uint8)))
    ims = [Image.fromarray(f) for _, f in frames]
    durs = [50] * WINDUP_LEN + [65] * (len(ims) - WINDUP_LEN)
    ims[0].save(os.path.join(out, "fire.gif"), save_all=True, append_images=ims[1:], duration=durs, loop=0, disposal=1)
    # 시트: 선딜 2 장 + 폭발 0..23 의 고른 장 + 꼬리
    pick = [3, 9] + [WINDUP_LEN + i for i in (0, 1, 2, 3, 4, 6, 8, 10, 13, 16, 19, 22)] + [WINDUP_LEN + BLAST_LEN + 6, WINDUP_LEN + BLAST_LEN + 18]
    pick = [p for p in pick if p < len(frames)]
    cols = 4
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
    sheet.save(os.path.join(out, "fire_sheet.png"))
    print(f"preview → {out}/fire.gif · fire_sheet.png ({len(frames)} frames)")


def main():
    import demon_fx
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("replace")
    p.add_argument("--src", default=demon_fx.OUT_BIN)
    p.add_argument("--out", default=demon_fx.OUT_BIN)
    p = sub.add_parser("preview")
    p.add_argument("--out", required=True)
    p.add_argument("--src", default=None)
    a = ap.parse_args()
    if a.cmd == "replace":
        replace(a.src, a.out)
    else:
        preview(a.out, a.src)


if __name__ == "__main__":
    main()
