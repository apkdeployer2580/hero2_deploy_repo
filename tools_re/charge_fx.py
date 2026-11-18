#!/usr/bin/env python3
"""「악마화」 돌진 이펙트 6 차 (2026-10-16) — 오린 잔상 대신 악마 자신의 달리기 그림으로 만든 잔상.

사용자: *"돌진 이펙트가 뭔가 이미지가 위쪽이 직선으로 잘린듯한 느낌이 들어 좀 더 자연스럽게 잔상을 업스케일링해줘."*

옛 땅 층(3 차 · `demon_fx_cut.charge_ground`, 왼쪽 · 오른쪽)은 예상도 05 에서 오린 남청 잔상 한 장(`costumes/astra_fx_hd/charge_trail.png`)
을 일렁였다 — 오린 상자의 윗변이 그대로 보여 「위가 직선으로 잘린」 그림이 됐다(위 · 아래 방향은 2 차의 절차 그림이라 괜찮았다).
이 파일은 네 방향 모두 **악마 자신의 달리기 HD 프레임**(`costumes/demon_300_hd` run0 · run1, `demon_fx.demon_any`)으로 잔상을 만든다:

* `charge_ground_<dir>` (땅 층 · 모든 개체 밑 · 8 반복 — FXG 가 `TICKS & 7`): 뒤로 셋(14 · 28 · 42 px) — 남청 → 자홍으로 물들인 몸 그림,
  멀수록 옅고(0.7 · 0.5 · 0.3) 노이즈로 부서지며(멀수록 더), **위 · 아래 가장자리는 그라데이션으로 사라진다**(직선 없음) · 바닥의 붉은
  속도선 셋(끝이 가늘어지는 띠 + 더하기 빛) · 발 뒤로 낮게 흐르는 흙먼지 · 구르는 돌.
* `charge_top_<dir>` (위 층 · 8 반복): 2 차 절차 그림 그대로 — 머리 · 등에서 뒤로 흐르는 불꽃 갈기 · 그을음 줄 · 앞쪽 충돌 섬광 · 불티
  (`demon_fx_art.charge_top` — 네 방향 전부 절차 그림이라 잘린 데가 없다) + 어깨에서 뒤로 끌리는 붉은 속도선 둘.

판정은 그대로(`DemonPatch::CHARGE_HALF` 2 = 5 × 5 칸).

    python tools_re/charge_fx.py replace [--src wie_android/src/demon_fx.bin] [--out …]   # 다른 효과는 바이트 그대로, 돌진만 새로
    python tools_re/charge_fx.py preview --out docs/skill/demon_charge_hd                   # 3× 시트 + GIF (실제 게임 배경 · 악마 HD 몸)
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
import demon_fx_art as art  # noqa: E402
from fx_swap import smooth  # noqa: E402

UP, DOWN, LEFT, RIGHT = 0, 1, 2, 3
DIR_NAMES = ["up", "down", "left", "right"]
CHARGE_LEN = 8  # = demon_fx.CHARGE_LEN (FXG · FXT: TICKS & 7)
BACK = {UP: (0, 1), DOWN: (0, -1), LEFT: (1, 0), RIGHT: (-1, 0)}  # 뒤 = 진행의 반대 (= demon_fx_art.BACK)
GHOSTS = [(16.0, 0.95, 0.14), (32.0, 0.78, 0.26), (50.0, 0.56, 0.4)]  # (뒤로 px, 알파, 부서짐) — 첫 잔상은 몸 뒤에서 반쯤 보인다
NAVY = (104, 96, 236)
MAGENTA = (240, 78, 140)
STREAK = (200, 26, 36)
STREAK_HOT = (255, 110, 90)


def _ghost(c, img, scale, x, y, alpha, tint, k, seed, dissolve, fade_top=0.28, fade_bottom=0.10, seed_shift=0.0):
    """악마 그림(PIL RGBA, `scale` px / 게임 px)의 잔상을 (x, y) — 기준점 상대 왼쪽 위 — 에: 물들이고(tint, k) · 노이즈로 부수고
    (dissolve 0..1 — 클수록 더 부서진다) · 위 fade_top · 아래 fade_bottom 몫을 그라데이션으로 지운다(잘린 변이 없다)."""
    a = np.asarray(img.convert("RGBA"), np.float32)
    s = c.ss
    assert s % scale == 0, (s, scale)
    if s // scale > 1:
        a = a.repeat(s // scale, axis=0).repeat(s // scale, axis=1)
    h, w = a.shape[:2]
    X0, Y0 = int(round((x + c.ax) * s)), int(round((y + c.ay) * s))
    sx0, sy0 = max(0, -X0), max(0, -Y0)
    X0c, Y0c = max(0, X0), max(0, Y0)
    X1, Y1 = min(c.w * s, X0 + w), min(c.h * s, Y0 + h)
    if X0c >= X1 or Y0c >= Y1:
        return
    sub = a[sy0:sy0 + (Y1 - Y0c), sx0:sx0 + (X1 - X0c)]
    body = sub[..., 3] / 255.0
    # 그림의 불투명 상자 안에서 위 → 아래 0..1
    ys, xs = np.nonzero(a[..., 3])
    if len(ys) == 0:
        return
    top, bot = ys.min(), ys.max() + 1
    yy = (np.arange(sy0, sy0 + sub.shape[0], dtype=np.float32) - top) / max(bot - top - 1, 1)
    grad = smooth(0.0, fade_top, yy) * smooth(1.0, 1.0 - fade_bottom, yy)
    gx = (np.arange(sx0, sx0 + sub.shape[1], dtype=np.float32) + X0) / s
    gy = (np.arange(sy0, sy0 + sub.shape[0], dtype=np.float32) + Y0) / s
    n = fp.fbm(gx[None, :] * 0.16 + seed * 3.7 + seed_shift, gy[:, None] * 0.16 - seed * 1.9, seed, 3)
    keep = smooth(dissolve - 0.22, dissolve + 0.22, n)  # 노이즈가 문턱을 넘는 곳만 — 멀수록(dissolve ↑) 덜 남는다
    lum = sub[..., :3].mean(axis=2, keepdims=True) / 255.0
    rgb = sub[..., :3] * (1 - k) + np.array(tint, np.float32) * (0.45 + lum) * k
    c.over(X0c, Y0c, rgb, body * grad[:, None] * keep * alpha)


def _run_img(f, d):
    import demon_fx as dfx
    return dfx.demon_any("run%d" % ((f // 2) % 2), d)


def charge_ground(d, f):
    """돌진 땅 층 (8 반복): 악마 자신의 달리기 잔상 셋 · 바닥 속도선 · 흙먼지 · 돌."""
    import demon_fx as dfx
    c = Canvas(176, 150, 88, 100)
    bx, by = BACK[d]
    side = d in (LEFT, RIGHT)
    off = dfx.DEMON_OFF
    img, sc = _run_img(f, d)
    for k, (dist, a, dis) in enumerate(reversed(GHOSTS)):  # 먼 것부터 (가까운 것이 위에)
        dd = dist * (1.0 if side else 0.72) + 2 * ((f + k) % 2)
        x, y = off[0] + bx * dd, off[1] + by * dd
        t = 0.35 + 0.65 * (dist / 40.0)
        tint = tuple(int(NAVY[i] + (MAGENTA[i] - NAVY[i]) * (1 - t)) for i in range(3))
        # 뒤로 조금 번진 사본 (움직임의 번짐) + 본 잔상
        _ghost(c, img, sc, x + bx * 4, y + by * 3, a * 0.5, tint, 0.9, seed=40 + k, dissolve=dis + 0.1, fade_top=0.3, seed_shift=f * 0.7)
        _ghost(c, img, sc, x, y, a, tint, 0.8, seed=10 + k, dissolve=dis, fade_top=0.22, seed_shift=f * 0.7)
    # 바닥 속도선 (몸 뒤로) — 끝이 가늘어지고 더하기 빛을 품는다
    for k in range(3):
        sidek = (k - 1) * 5
        px, py = -by * sidek, bx * sidek
        L = 28 + 9 * ((f + k * 3) % 4)
        x0, y0 = px + bx * 6, py + by * 6 * (1.0 if side else 0.7) - 2
        pts = [(x0, y0), (x0 + bx * L * 0.5, y0 + by * L * 0.5), (x0 + bx * L, y0 + by * L)]
        c.taper(pts, 2.4, 0.3, STREAK, 0.6)
        c.taper(pts, 1.1, 0.2, STREAK_HOT, 0.45, "add")
    # 흙먼지 — 발 뒤로 낮게, 프레임마다 흘러간다
    for j in range(4):
        t = ((f + j * 2) % 8) / 8
        dist = 6 + t * 30
        c.blob(bx * dist + (-by) * ((j % 2) * 4 - 2), by * dist * 0.7 + 1 - t * 2, 4.5 + t * 6, 2.6 + t * 2.4, fp.DUST, seed=50 + j * 7 + f,
               alpha=0.85 * (1 - t) ** 0.8, scale=0.5)
    rnd = random.Random(4000 + d * 10)
    for j in range(3):
        t = ((f + j * 3) % 8) / 8
        x = bx * (8 + t * 26) + rnd.uniform(-4, 4)
        y = by * (8 + t * 26) * 0.7 - 2 - math.sin(t * math.pi) * 6
        c.rock(x, y, 2.0, 1.0 - t, rnd=random.Random(j), base=(46, 38, 36), hi=(100, 88, 80))
    return c


def charge_top(d, f):
    """돌진 위 층 (8 반복): 2 차의 불꽃 갈기 · 그을음 · 섬광 · 불티 그대로 + 어깨에서 뒤로 끌리는 붉은 속도선 둘."""
    c = art.charge_top(d, f)
    bx, by = BACK[d]
    side = d in (LEFT, RIGHT)
    for i, (sx, sy) in enumerate(((-6, -30), (6, -26)) if not side else ((-bx * 4, -31), (-bx * 2, -24))):
        L = (34 if side else 22) + 6 * ((f + i * 2) % 4)
        wob = [(sx + bx * L * t + (-by) * math.sin(f * 1.3 + t * 6 + i) * 2.0 * t, sy + by * L * t + bx * math.sin(f * 1.3 + t * 6 + i) * 2.0 * t)
               for t in (0, 0.25, 0.5, 0.75, 1.0)]
        c.taper(wob, 1.6, 0.2, STREAK, 0.55)
        c.taper(wob, 0.8, 0.15, STREAK_HOT, 0.45, "add")
    return c


EFFECTS = {}
for _d in range(4):
    EFFECTS[f"charge_ground_{DIR_NAMES[_d]}"] = (lambda f, d=_d: charge_ground(d, f))
    EFFECTS[f"charge_top_{DIR_NAMES[_d]}"] = (lambda f, d=_d: charge_top(d, f))


def preview(out, src=None, k=3):
    import fx_swap
    FX, FY = 88, 104
    seq = []
    for d in (RIGHT, DOWN, UP, LEFT):
        for a in range(CHARGE_LEN):
            seq.append((f"돌진 {DIR_NAMES[d]} {a}", [("g", f"charge_ground_{DIR_NAMES[d]}", a, FX, FY), ("t", f"charge_top_{DIR_NAMES[d]}", a, FX, FY)]))
    fx_swap.preview_scenes(out, "charge", seq, src=src, k=k, demon_pose=("run0", RIGHT), mobs=[], duration=50,
                           pick=[0, 1, 2, 3, 8, 10, 12, 14, 16, 18, 24, 26])


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
        fx_swap.replace(a.src or demon_fx.OUT_BIN, a.out or demon_fx.OUT_BIN, EFFECTS)
    else:
        preview(a.out, a.src, a.k)


if __name__ == "__main__":
    main()
