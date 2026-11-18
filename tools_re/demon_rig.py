#!/usr/bin/env python3
"""악마 도트의 뼈대 — 예상도에서 오린 자세(`demon_draft.src`)를 관절로 굽혀 사이 동작을 만든다.

사용자(2026-10-14): *"초안을 토대로 연결동작들을 유추해서 … 기본공격시 몸을 숙이는듯한 동작 … 전부 자세하게"*.
예상도에는 할퀴기 세 단의 「친 순간」 한 장씩만 있다. 웅크림 · 뛰어듦 · 내려침 · 회복은 그 한 장을 관절로 굽혀 만든다
(2D 스켈레톤 애니메이션 — 그림을 새로 그리지 않고 초안 픽셀을 뼈마다 돌려 옮긴다).

  * 뼈대 = 자세마다 관절 15 개(`SKELETONS`, 캔버스 56 × 50 · 발바닥 (28, 46) 좌표 — `python demon_rig.py zoom` 의 격자로 읽었다).
  * 픽셀 → 뼈 = 가장 가까운 뼈 선분(뼈 굵기만큼 빼고). 털 · 뿔은 몸통 · 머리에 붙는다.
  * 자세 = 관절의 새 자리. 뼈마다 (쉼 A→B) → (새 A'→B') 닮음 변환으로 픽셀을 옮긴다(뒤 → 앞 순서로 칠해 앞 팔이 몸 위).
  * 그림 = 4 배 확대(EPX 두 번) 위에서 역방향으로 옮기고 4×4 다수결로 줄인다(rotsprite 꼴 — 돌린 선이 들쭉날쭉하지 않다).
    관절 둘레는 이웃 뼈의 픽셀도 받아 틈이 안 생긴다. 팔이 비켜 드러난 몸통 구멍은 이웃 색으로 메운다.

명령: `python demon_rig.py zoom <out_dir>` (원본 자세 + 관절 점) · `python demon_rig.py demo <out.png>`.
"""
import argparse
import math
import os
import sys
from collections import Counter

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

S = 4  # 확대 배율 (EPX 두 번)

# ── 뼈대 ─────────────────────────────────────────────────────────────────────────────────────────────
# 관절: hip · neck · crown(뿔 끝) · sh/el/hand (n = 앞쪽 팔, f = 뒤쪽 팔) · hp/kn/ft (n = 앞쪽 다리, f = 뒤쪽 다리).
# 옆모습은 오른쪽을 본다(+x = 앞). 앞모습은 n = 화면 오른쪽 팔 · 다리, f = 화면 왼쪽.
SKELETONS = {
    "side_lunge": dict(hip=(34, 38), neck=(39, 29), crown=(36, 20),
                       sh_n=(41, 31), el_n=(47, 32), hand_n=(54, 32),
                       sh_f=(32, 28), el_f=(28, 27), hand_f=(23, 29),
                       hp_n=(37, 39), kn_n=(39, 42), ft_n=(38, 46),
                       hp_f=(31, 39), kn_f=(27, 40), ft_f=(23, 42)),
    "side_crouch": dict(hip=(30, 35), neck=(39, 29), crown=(44, 17),
                        sh_n=(41, 32), el_n=(45, 36), hand_n=(50, 41),
                        sh_f=(37, 30), el_f=(39, 33), hand_f=(41, 35),
                        hp_n=(33, 37), kn_n=(33, 41), ft_n=(33, 45),
                        hp_f=(27, 36), kn_f=(22, 40), ft_f=(16, 43)),
    "side_crouch2": dict(hip=(29, 37), neck=(36, 29), crown=(41, 19),
                         sh_n=(41, 30), el_n=(44, 33), hand_n=(45, 37),
                         sh_f=(30, 25), el_f=(24, 22), hand_f=(17, 24),
                         hp_n=(32, 38), kn_n=(33, 42), ft_n=(34, 46),
                         hp_f=(26, 38), kn_f=(19, 40), ft_f=(13, 43)),
    "front_stand": dict(hip=(28, 34), neck=(28, 21), crown=(29, 11),
                        sh_n=(35, 23), el_n=(37, 25), hand_n=(39, 29),
                        sh_f=(21, 23), el_f=(19, 26), hand_f=(15, 29),
                        hp_n=(33, 35), kn_n=(34, 40), ft_n=(35, 46),
                        hp_f=(23, 35), kn_f=(22, 40), ft_f=(21, 46)),
    "front_fist": dict(hip=(29, 38), neck=(30, 27), crown=(30, 14),
                       sh_n=(35, 27), el_n=(38, 25), hand_n=(43, 22),
                       sh_f=(24, 29), el_f=(21, 32), hand_f=(17, 36),
                       hp_n=(33, 39), kn_n=(34, 43), ft_n=(35, 46),
                       hp_f=(25, 39), kn_f=(24, 43), ft_f=(23, 46)),
    "front_roar": dict(hip=(28, 36), neck=(28, 24), crown=(28, 15),
                       sh_n=(35, 28), el_n=(40, 30), hand_n=(44, 28),
                       sh_f=(21, 28), el_f=(16, 29), hand_f=(12, 28),
                       hp_n=(33, 37), kn_n=(35, 41), ft_n=(36, 46),
                       hp_f=(24, 37), kn_f=(22, 41), ft_f=(21, 46)),
    # ★ 2026-10-14 (5): 스킬 시전 동작의 정점 둘 — 예상도 08 지옥칼날(앞팔을 곧게 뻗은 낮은 자세) · 11 용암불(두 손을 땅에 댄 웅크림)
    "side_reach": dict(hip=(27, 33), neck=(33, 24), crown=(39, 14),
                       sh_n=(38, 24), el_n=(45, 22), hand_n=(52, 22),
                       sh_f=(27, 27), el_f=(21, 31), hand_f=(17, 34),
                       hp_n=(31, 35), kn_n=(36, 38), ft_n=(35, 46),
                       hp_f=(24, 35), kn_f=(20, 39), ft_f=(14, 44)),
    "side_lava": dict(hip=(25, 39), neck=(31, 31), crown=(35, 23),
                      sh_n=(35, 33), el_n=(40, 35), hand_n=(45, 36),
                      sh_f=(25, 31), el_f=(19, 32), hand_f=(16, 33),
                      hp_n=(29, 41), kn_n=(32, 42), ft_n=(33, 46),
                      hp_f=(22, 41), kn_f=(17, 42), ft_f=(13, 45)),
}

# (이름, 관절 A, 관절 B, 굵기, 칠하는 순서) — 순서가 클수록 위. 옆모습: 뒤쪽 팔 · 다리가 몸 뒤, 앞쪽이 몸 앞.
SIDE_BONES = [
    ("arm_f_up", "sh_f", "el_f", 2.0, 0), ("arm_f_lo", "el_f", "hand_f", 2.4, 0),
    ("leg_f_up", "hp_f", "kn_f", 2.6, 1), ("leg_f_lo", "kn_f", "ft_f", 2.4, 1),
    ("torso", "hip", "neck", 5.5, 2),
    ("leg_n_up", "hp_n", "kn_n", 2.6, 3), ("leg_n_lo", "kn_n", "ft_n", 2.4, 3),
    ("head", "neck", "crown", 4.0, 4),
    ("arm_n_up", "sh_n", "el_n", 2.0, 5), ("arm_n_lo", "el_n", "hand_n", 2.4, 5),
]
FRONT_BONES = [
    ("leg_f_up", "hp_f", "kn_f", 2.6, 1), ("leg_f_lo", "kn_f", "ft_f", 2.4, 1),
    ("leg_n_up", "hp_n", "kn_n", 2.6, 1), ("leg_n_lo", "kn_n", "ft_n", 2.4, 1),
    ("torso", "hip", "neck", 6.0, 2),
    ("head", "neck", "crown", 4.5, 3),
    ("arm_f_up", "sh_f", "el_f", 2.0, 5), ("arm_f_lo", "el_f", "hand_f", 2.4, 5),
    ("arm_n_up", "sh_n", "el_n", 2.0, 5), ("arm_n_lo", "el_n", "hand_n", 2.4, 5),
]
JOINT_R = 2.2  # 관절 둘레: 이웃 뼈의 픽셀도 받는 반지름


def bones_of(name):
    return FRONT_BONES if name.startswith("front_") else SIDE_BONES


def _seg_dist(px, py, a, b):
    ax, ay = a
    bx, by = b
    vx, vy = bx - ax, by - ay
    L = vx * vx + vy * vy
    t = np.clip(((px - ax) * vx + (py - ay) * vy) / max(L, 1e-9), 0, 1)
    return np.hypot(px - (ax + t * vx), py - (ay + t * vy))


# ── 확대 · 축소 ──────────────────────────────────────────────────────────────────────────────────────
def _epx(a):
    """EPX(scale2x) — 같은 색 대각선을 매끄럽게 두 배로."""
    H, W = a.shape[:2]
    key = a.astype(np.int64)
    key = (key[..., 0] << 24) | (key[..., 1] << 16) | (key[..., 2] << 8) | key[..., 3]
    key[a[..., 3] == 0] = 0
    p = np.pad(key, 1, mode="edge")
    A, B, C, D = p[:-2, 1:-1], p[1:-1, 2:], p[1:-1, :-2], p[2:, 1:-1]  # 위 · 오른 · 왼 · 아래
    pa = np.pad(a, ((1, 1), (1, 1), (0, 0)), mode="edge")
    cA, cB, cC, cD = pa[:-2, 1:-1], pa[1:-1, 2:], pa[1:-1, :-2], pa[2:, 1:-1]
    out = np.zeros((H * 2, W * 2, 4), a.dtype)
    q1 = np.where(((C == A) & (C != D) & (A != B))[..., None], cA, a)
    q2 = np.where(((A == B) & (A != C) & (B != D))[..., None], cB, a)
    q3 = np.where(((D == C) & (D != B) & (C != A))[..., None], cC, a)
    q4 = np.where(((B == D) & (B != A) & (D != C))[..., None], cD, a)
    out[0::2, 0::2], out[0::2, 1::2], out[1::2, 0::2], out[1::2, 1::2] = q1, q2, q3, q4
    return out


def upscale(a):
    up = a
    for _ in range(int(math.log2(S))):
        up = _epx(up)
    return up


def downscale(big, thresh=7):
    """S×S 다수결: 불투명이 thresh 이상이면 그 칸 = 가장 많은 색."""
    H, W = big.shape[0] // S, big.shape[1] // S
    out = np.zeros((H, W, 4), np.uint8)
    blk = big.reshape(H, S, W, S, 4).transpose(0, 2, 1, 3, 4).reshape(H, W, S * S, 4)
    op = blk[..., 3] > 0
    cnt = op.sum(axis=2)
    ys, xs = np.nonzero(cnt >= thresh)
    for y, x in zip(ys, xs):
        cols = [tuple(c) for c in blk[y, x][op[y, x]][:, :3]]
        out[y, x, :3] = Counter(cols).most_common(1)[0][0]
        out[y, x, 3] = 255
    return out


def fill_holes(a, max_hole=40):
    """테두리에서 닿지 않는 투명 칸(팔이 비켜 드러난 몸통)을 이웃 색으로 메운다."""
    op = a[..., 3] > 0
    H, W = op.shape
    seen = np.zeros_like(op)
    stack = [(y, x) for y in range(H) for x in (0, W - 1) if not op[y, x]] + [(y, x) for x in range(W) for y in (0, H - 1) if not op[y, x]]
    for y, x in stack:
        seen[y, x] = True
    while stack:
        y, x = stack.pop()
        for yy, xx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
            if 0 <= yy < H and 0 <= xx < W and not op[yy, xx] and not seen[yy, xx]:
                seen[yy, xx] = True
                stack.append((yy, xx))
    holes = ~op & ~seen
    if holes.sum() > max_hole:
        holes[:] = False
    out = a.copy()
    todo = list(zip(*np.nonzero(holes)))
    for _ in range(8):
        left = []
        for y, x in todo:
            nb = [tuple(out[yy, xx, :3]) for yy in range(y - 1, y + 2) for xx in range(x - 1, x + 2)
                  if 0 <= yy < H and 0 <= xx < W and out[yy, xx, 3] > 0 and (yy, xx) != (y, x)]
            if len(nb) >= 3:
                out[y, x, :3] = Counter(nb).most_common(1)[0][0]
                out[y, x, 3] = 255
            else:
                left.append((y, x))
        if not left:
            break
        todo = left
    return out


# ── 자세 그리기 ──────────────────────────────────────────────────────────────────────────────────────
class Rig:
    """한 원본 자세의 뼈대: 확대 그림 · 픽셀의 뼈 · 관절."""

    def __init__(self, name, art=None):
        import demon_draft as dd
        self.name = name
        self.art = dd.src(name) if art is None else art
        # ★ HD (2026-10-15): the art may be the 3× canvas — the joints · bone radii · the joint neighbourhood scale with it
        self.k = max(1, self.art.shape[1] // 56)
        self.rest = {k: np.array(v, float) * self.k for k, v in SKELETONS[name].items()}
        self.bones = [(n, a, b, r * self.k, z) for (n, a, b, r, z) in bones_of(name)]
        self.up = upscale(self.art)
        H, W = self.up.shape[:2]
        yy, xx = np.mgrid[0:H, 0:W]
        px, py = (xx + 0.5) / S, (yy + 0.5) / S
        d = np.stack([_seg_dist(px, py, self.rest[a], self.rest[b]) - r for (_n, a, b, r, _z) in self.bones])
        self.owner = np.argmin(d, axis=0)
        self.opaque = self.up[..., 3] > 0
        self.CH, self.CW = self.art.shape[:2]

    def joints(self):
        return {k: v.copy() for k, v in self.rest.items()}

    def _transform(self, a, b, A, B):
        v, w = b - a, B - A
        k = float(np.clip(np.hypot(*w) / max(np.hypot(*v), 1e-6), 0.8, 1.25))
        th = math.atan2(w[1], w[0]) - math.atan2(v[1], v[0])
        return a, A, k, th

    def render(self, J, fill=True, z=None):
        """관절 J 의 그림 (RGBA, 원본 캔버스 크기). `z` = {뼈 이름: 칠하는 순서} — 그 자세에서만 바꾼다
        (예: 할퀴기 III 의 내려치는 뒤쪽 팔은 몸 앞으로 넘어온다)."""
        z = z or {}
        H, W = self.up.shape[:2]
        yy, xx = np.mgrid[0:H, 0:W]
        qx, qy = (xx + 0.5) / S, (yy + 0.5) / S
        out = np.zeros_like(self.up)
        joint_of = {}
        for i, (_n, a, b, _r, _z) in enumerate(self.bones):
            for j, (_m, c, d, _s, _y) in enumerate(self.bones):
                if i != j:
                    shared = {a, b} & {c, d}
                    if shared:
                        joint_of[(i, j)] = self.rest[shared.pop()]
        order = sorted(range(len(self.bones)), key=lambda i: z.get(self.bones[i][0], self.bones[i][4]))
        for i in order:
            _n, a, b, _r, _z = self.bones[i]
            a0, A, k, th = self._transform(self.rest[a], self.rest[b], np.array(J[a], float), np.array(J[b], float))
            c, s = math.cos(-th), math.sin(-th)
            dx, dy = qx - A[0], qy - A[1]
            sx = a0[0] + (c * dx - s * dy) / k
            sy = a0[1] + (s * dx + c * dy) / k
            ix, iy = np.floor(sx * S).astype(int), np.floor(sy * S).astype(int)
            ok = (ix >= 0) & (ix < W) & (iy >= 0) & (iy < H)
            ixc, iyc = np.clip(ix, 0, W - 1), np.clip(iy, 0, H - 1)
            own = self.owner[iyc, ixc]
            take = ok & self.opaque[iyc, ixc] & (own == i)
            for (p, q), jt in joint_of.items():
                if p == i:
                    near = np.hypot(sx - jt[0], sy - jt[1]) <= JOINT_R * self.k
                    take |= ok & self.opaque[iyc, ixc] & (own == q) & near
            out[take] = self.up[iyc[take], ixc[take]]
        small = downscale(out)
        return fill_holes(small, max_hole=40 * self.k * self.k) if fill else small


# ── 관절 조작 ────────────────────────────────────────────────────────────────────────────────────────
UPPER = ["neck", "crown", "sh_n", "el_n", "hand_n", "sh_f", "el_f", "hand_f"]
ARM = {"n": ["el_n", "hand_n"], "f": ["el_f", "hand_f"]}


def _cross(u, v):
    return float(u[0] * v[1] - u[1] * v[0])


def _rest_bend(rig, root, mid, end):
    """쉼 자세에서 가운데 관절이 굽은 쪽 (+1 / −1) — `_ik` 의 bend 와 같은 뜻."""
    return 1 if _cross(rig.rest[end] - rig.rest[root], rig.rest[mid] - rig.rest[root]) >= 0 else -1


def _rot(p, c, deg):
    t = math.radians(deg)
    dx, dy = p[0] - c[0], p[1] - c[1]
    return np.array([c[0] + math.cos(t) * dx - math.sin(t) * dy, c[1] + math.sin(t) * dx + math.cos(t) * dy])


def lean(J, deg, pivot="hip"):
    """윗몸을 엉덩이 둘레로 deg 만큼 (화면 시계 방향 = 오른쪽을 보는 옆모습의 앞으로 숙임)."""
    c = J[pivot].copy()
    for k in UPPER:
        J[k] = _rot(J[k], c, deg)
    return J


def move(J, names, dx, dy):
    for k in names:
        J[k] = J[k] + np.array([dx, dy], float)
    return J


def _ik(root, target, l1, l2, bend):
    """두 마디 IK — 팔꿈치 · 무릎 자리 (bend = +1 / −1: 굽는 쪽)."""
    d = target - root
    L = float(np.hypot(*d))
    L = min(max(L, abs(l1 - l2) + 1e-3), l1 + l2 - 1e-3)
    u = d / max(np.hypot(*d), 1e-9)
    a = (l1 * l1 - l2 * l2 + L * L) / (2 * L)
    h = math.sqrt(max(l1 * l1 - a * a, 0))
    n = np.array([-u[1], u[0]]) * bend
    end = root + u * L
    return root + u * a + n * h, end


def reach(rig, J, side, hand, bend=1):
    """팔 side('n'|'f')의 손을 어깨 상대 hand=(dx, dy) 로 — 팔꿈치는 IK."""
    sh, el, hd = f"sh_{side}", f"el_{side}", f"hand_{side}"
    l1 = float(np.hypot(*(rig.rest[el] - rig.rest[sh])))
    l2 = float(np.hypot(*(rig.rest[hd] - rig.rest[el])))
    J[el], J[hd] = _ik(J[sh], J[sh] + np.array(hand, float), l1, l2, bend)
    return J


def crouch(rig, J, dy, knee_out=1):
    """엉덩이 · 윗몸을 dy 내리고 무릎을 굽힌다(발은 그대로)."""
    move(J, ["hip", "hp_n", "hp_f"] + UPPER, 0, dy)
    for s in ("n", "f"):
        hp, kn, ft = f"hp_{s}", f"kn_{s}", f"ft_{s}"
        l1 = float(np.hypot(*(rig.rest[kn] - rig.rest[hp])))
        l2 = float(np.hypot(*(rig.rest[ft] - rig.rest[kn])))
        bend = _rest_bend(rig, hp, kn, ft) * (1 if knee_out else -1)
        J[kn], _ = _ik(J[hp], J[ft], l1, l2, bend)
    return J


def step(rig, J, side, dx, lift):
    """다리 side 의 발을 dx 옮기고 lift 만큼 든다 (무릎 IK)."""
    hp, kn, ft = f"hp_{side}", f"kn_{side}", f"ft_{side}"
    J[ft] = rig.rest[ft] + np.array([dx, -lift], float)
    l1 = float(np.hypot(*(rig.rest[kn] - rig.rest[hp])))
    l2 = float(np.hypot(*(rig.rest[ft] - rig.rest[kn])))
    J[kn], J[ft] = _ik(J[hp], J[ft], l1, l2, _rest_bend(rig, hp, kn, ft))
    return J


def lift_front(J, side, lift):
    """앞 · 뒷모습의 다리 들기 — 무릎이 카메라 쪽으로 오므로 옆으로 굽히지 않고 정강이를 위로 줄인다."""
    J[f"ft_{side}"] = J[f"ft_{side}"] + np.array([0, -lift], float)
    J[f"kn_{side}"] = J[f"kn_{side}"] + np.array([0, -lift * 0.5], float)
    return J


# ── 도구 ─────────────────────────────────────────────────────────────────────────────────────────────
def overlay(a, J, rig, z=10):
    """그림 + 관절 · 뼈 (확인용)."""
    H, W = a.shape[:2]
    bg = Image.new("RGB", (W * z, H * z), (60, 95, 55))
    im = Image.fromarray(a, "RGBA").resize((W * z, H * z), Image.NEAREST)
    bg.paste(im, (0, 0), im)
    d = ImageDraw.Draw(bg)
    for (_n, a_, b_, _r, zz) in rig.bones:
        p, q = J[a_], J[b_]
        col = (255, 255, 0) if zz >= 4 else (0, 255, 255) if zz >= 2 else (255, 0, 255)
        d.line([((p[0]) * z, (p[1]) * z), ((q[0]) * z, (q[1]) * z)], fill=col, width=2)
    for k, p in J.items():
        d.ellipse([p[0] * z - 4, p[1] * z - 4, p[0] * z + 4, p[1] * z + 4], outline=(255, 255, 255))
    return bg


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("zoom")
    p.add_argument("out_dir")
    a = ap.parse_args()
    if a.cmd == "zoom":
        os.makedirs(a.out_dir, exist_ok=True)
        for name in SKELETONS:
            rig = Rig(name)
            J = rig.joints()
            overlay(rig.render(J), J, rig).save(os.path.join(a.out_dir, f"rig_{name}.png"))
            print(name)


if __name__ == "__main__":
    main()
