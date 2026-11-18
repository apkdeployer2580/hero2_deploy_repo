#!/usr/bin/env python3
"""★★ 2026-10-16 — 「악의」 악마 몸의 HD 쌍을 3× → 6× 로 (사용자: *"악의 발동했을때 캐릭터 외형 더 업스케일링 해주고"*).

악마 몸(`demon_body`, 효과 146 · 115 프레임)의 쌍은 예상도 해상도 = 게임 3× 였다. 예상도 자체가 게임 화면 × 3 의 도트라(패널 526 × 600 =
176 × 220 의 2.99 배 — `demon_fx.MOCK_BOXES`) 원본에서 더 뽑을 해상도는 없다. 그런데 창은 게임 px 당 S25 ≈ 4.9 · S9 ≈ 6.5 창 px 이라
3× 쌍을 최근접으로 1.6 ~ 2.2 배 늘리면 3× 도트 하나가 창 px 1 · 2 개로 들쭉날쭉 그려지고 외곽선 계단이 그대로 보였다.

그래서 3× 쌍에 **픽셀아트 확대(xBR 꼴)** 를 두 번 걸어(12×) 2×2 상자 평균으로 6× 쌍을 만든다(`smooth6`): 외곽 · 경계의 계단을
가장자리 방향을 따라 매끄럽게 잇고(가중 거리 wd_red < wd_blue 인 모서리만 · 사각 모서리와 1 px 선은 남긴다), 섞인 색은 반투명 가장자리까지
premultiplied 로 섞는다. 한 번(6×)보다 곡선이 둥글고, 눈 · 발톱 같은 작은 점은 흐려지지 않는다(2026-10-16 비교 — 3× · xBR 6× · 이것). 새 쌍의 기준점 =
옛 기준점 × 2(쌍 px), 배율 6 — 게임 좌표의 자리 · 크기는 그대로다(`fx.rs` `compose_hd` 가 쌍마다 배율을 읽는다). 창 px 보다 촘촘한 쌍
(6× 를 4.9 창에)은 `compose_hd` 가 창 px 마다 2×2 를 평균한다(같은 날 함께 바꿈 — 3× 쌍은 S25 · S9 에서 촘촘하지 않으니 그대로).

`replace(src, out)`: bin 의 다른 모든 효과 · 스프라이트 · 쌍은 **바이트 그대로**(`fire_fx.read_raw` → `Library.sprite_q`), `demon_body`
프레임이 쓰는 스프라이트의 쌍만 6× 로 다시 양자화한다(1× 스프라이트도 그대로). 원천 = 지금 bin 의 3× 쌍(색인 + 팔레트) — 이미 6× 인
쌍은 그대로 둔다(두 번 돌려도 같다).

    python tools_re/demon_upscale.py replace [src.bin [out.bin]]    # 기본 = wie_android/src/demon_fx.bin 제자리
    python tools_re/demon_upscale.py preview <out.png> [k,k,..] [창배율]   # 3× 쌍 ↔ 6× 쌍, 창 해상도에서
"""

import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "wie_android", "src", "demon_fx.bin")
BODY = "demon_body"
FROM_SCALE = 3
TO_SCALE = 6
# xBR 의 「같은 색」 문턱 (가중 YUV + 알파 거리, 0..1 색 기준 — 48·ΔY 가 이 값이면 밝기 차 ≈ 2 %)
EQ = 1.0


# ── xBR 2× (Hyllian 의 lv2 규칙, numpy) ───────────────────────────────────────────────────────────────────
def _yuva(p):
    """premultiplied RGBA(0..1) → 거리용 (48Y, 7U, 6V, 48A)."""
    r, g, b, a = p[..., 0], p[..., 1], p[..., 2], p[..., 3]
    y = 0.299 * r + 0.587 * g + 0.114 * b
    u = -0.169 * r - 0.331 * g + 0.5 * b
    v = 0.5 * r - 0.419 * g - 0.081 * b
    return np.stack([48 * y, 7 * u, 6 * v, 48 * a], -1)


def _corner(P):
    """오른쪽 아래 모서리(E3) 하나 — (px 색, E3 · E2 · E1 섞음 비율). P = premultiplied RGBA, 가장자리 2 px 덧댄 것."""
    H_, W_ = P.shape[0] - 4, P.shape[1] - 4
    Y = _yuva(P)

    def at(dy, dx):
        return slice(2 + dy, 2 + dy + H_), slice(2 + dx, 2 + dx + W_)

    def d(a, b):
        return np.abs(Y[at(*a)] - Y[at(*b)]).sum(-1)

    def neq(a, b):
        return d(a, b) > EQ

    def eq(a, b):
        return ~neq(a, b)

    E, B, C, D, F, G, H, I = (0, 0), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)
    F4, I4, H5, I5 = (0, 2), (1, 2), (2, 0), (2, 1)
    wd_red = d(E, C) + d(E, G) + d(I, F4) + d(I, H5) + 4 * d(H, F)
    wd_blue = d(H, D) + d(H, I5) + d(F, I4) + d(F, B) + 4 * d(E, I)
    irlv1 = neq(E, F) & neq(E, H)
    irlv2 = ((neq(F, B) & neq(F, C)) | (neq(H, D) & neq(H, G)) | (eq(E, I) & ((neq(F, F4) & neq(F, I4)) | (neq(H, H5) & neq(H, I5))))
             | eq(E, G) | eq(E, C))
    edr = (wd_red < wd_blue) & irlv1 & irlv2
    dfg, dhc = d(F, G), d(H, C)
    left = edr & (2 * dfg <= dhc) & neq(E, G) & neq(D, G)
    up = edr & (dfg >= 2 * dhc) & neq(E, C) & neq(B, C)
    px = np.where((d(E, F) <= d(E, H))[..., None], P[at(*F)], P[at(*H)])
    w3 = np.where(left & up, 5 / 6, np.where(left | up, 0.75, 0.5)) * edr
    w2 = np.where(left, 0.25, 0.0)
    w1 = np.where(up, 0.25, 0.0)
    return px, w3, w2, w1


def xbr2x(rgba):
    """uint8 RGBA (H×W) → uint8 RGBA (2H×2W). 네 모서리를 뒤집기로(같은 규칙 · 거울) 차례로 섞는다."""
    a = rgba.astype(np.float32) / 255.0
    P = np.concatenate([a[..., :3] * a[..., 3:4], a[..., 3:4]], -1)  # premultiplied
    h, w = P.shape[:2]
    sub = [P.copy() for _ in range(4)]  # 0 왼위 · 1 오른위 · 2 왼아래 · 3 오른아래
    # (y 뒤집기, x 뒤집기) → 그 거울에서의 E3 · E2 · E1 이 원래의 어느 칸인가
    for fy, fx, q3, q2, q1 in ((0, 0, 3, 2, 1), (0, 1, 2, 3, 0), (1, 0, 1, 0, 3), (1, 1, 0, 1, 2)):
        Q = P[::-1] if fy else P
        Q = Q[:, ::-1] if fx else Q
        Qp = np.pad(Q, ((2, 2), (2, 2), (0, 0)), mode="edge")
        px, w3, w2, w1 = _corner(Qp)

        def back(x):
            x = x[::-1] if fy else x
            return x[:, ::-1] if fx else x

        px, w3, w2, w1 = back(px), back(w3), back(w2), back(w1)
        for q, wq in ((q3, w3), (q2, w2), (q1, w1)):
            sub[q] = sub[q] + (px - sub[q]) * wq[..., None]
    out = np.zeros((2 * h, 2 * w, 4), np.float32)
    out[0::2, 0::2], out[0::2, 1::2], out[1::2, 0::2], out[1::2, 1::2] = sub
    al = out[..., 3:4]
    col = np.where(al > 1e-4, out[..., :3] / np.maximum(al, 1e-4), 0.0)
    res = np.concatenate([np.clip(col, 0, 1), np.clip(al, 0, 1)], -1)
    res = np.round(res * 255).astype(np.uint8)
    res[res[..., 3] < 8] = 0  # 거의 투명한 섞임은 버린다 (쌍 상자가 쓸데없이 커지지 않게)
    return res


def box_half(rgba):
    """uint8 RGBA → 가로 · 세로 ½ (premultiplied 2×2 평균)."""
    f = rgba.astype(np.float32) / 255.0
    p = np.concatenate([f[..., :3] * f[..., 3:4], f[..., 3:4]], -1)
    h, w = p.shape[0] // 2 * 2, p.shape[1] // 2 * 2
    p = p[:h, :w]
    p = (p[0::2, 0::2] + p[0::2, 1::2] + p[1::2, 0::2] + p[1::2, 1::2]) / 4
    al = p[..., 3:4]
    col = np.where(al > 1e-4, p[..., :3] / np.maximum(al, 1e-4), 0.0)
    res = np.round(np.clip(np.concatenate([col, al], -1), 0, 1) * 255).astype(np.uint8)
    res[res[..., 3] < 8] = 0
    return res


def smooth6(rgba3):
    """3× 쌍(RGBA) → 6× (xBR 2× 두 번 → ½ 상자 평균)."""
    return box_half(xbr2x(xbr2x(rgba3)))


# ── bin ───────────────────────────────────────────────────────────────────────────────────────────────────
def dequantize(idx, pal):
    """쌍 색인(0 = 투명 · i = pal[i − 1]) → uint8 RGBA."""
    full = np.concatenate([np.zeros((1, 4), np.uint8), pal.astype(np.uint8)], 0)
    return full[idx]


def upscale_twin(qtwin):
    """(idx, pal, ax, ay, 3) → (idx, pal, 2ax, 2ay, 6) — 투명 가장자리는 잘라 상자를 줄인다(기준점이 따라 움직인다)."""
    import demon_fx
    idx, pal, ax, ay, scale = qtwin
    assert scale == FROM_SCALE, scale
    big = smooth6(dequantize(idx, pal))
    ys, xs = np.nonzero(big[..., 3])
    y0, y1, x0, x1 = ys.min(), ys.max() + 1, xs.min(), xs.max() + 1
    big = np.ascontiguousarray(big[y0:y1, x0:x1])
    q = demon_fx.quantize_twin(big)
    assert q is not None
    nidx, npal = q
    return nidx, npal, 2 * int(ax) - int(x0), 2 * int(ay) - int(y0), TO_SCALE


def body_sprites(effects):
    by = dict(effects)
    return sorted({s for parts in by[BODY] for (s, *_r) in parts})


def replace(src=BIN, out=BIN):
    import demon_fx
    import fire_fx
    sprites, effects, qtwins = fire_fx.read_raw(src)
    want = [n for n, _ in demon_fx.EFFECTS]
    assert [n for n, _ in effects] == want, "bin 의 효과 목록이 EFFECTS 와 다르다"
    body = set(body_sprites(effects))
    new = {}
    done = kept = 0
    for s in sorted(body):
        t = qtwins.get(s)
        if t is None:
            continue
        if t[4] == TO_SCALE:
            kept += 1
            continue
        new[s] = upscale_twin(t)
        done += 1
    lib = demon_fx.Library(hd=True)
    remap = {}

    def keep(parts):
        out_parts = []
        for (s, dx, dy, a, fl) in parts:
            if s not in remap:
                arr, ax, ay = sprites[s]
                remap[s] = lib.sprite_q((np.ascontiguousarray(arr), ax, ay), new.get(s, qtwins.get(s)))
            out_parts.append((remap[s], dx, dy, a, fl))
        return out_parts

    for name, frames in effects:
        lib.add_effect(name, [keep(p) for p in frames])
    size, shape, n, hd_shape, twins = lib.write(out)
    print(f"{out}: {size:,} B · {n} sprites · HD twins {twins}" + (f" (atlas {hd_shape[1]}×{hd_shape[0]})" if hd_shape else "")
          + f" · {BODY}: {done} twin(s) {FROM_SCALE}× → {TO_SCALE}×, {kept} already {TO_SCALE}×")
    return done


# ── 미리보기 ──────────────────────────────────────────────────────────────────────────────────────────────
def render(rgba, k, win, bg=(52, 80, 40)):
    """쌍(배율 k)을 창(게임 px 당 win 창 px)에 — `compose_hd` 와 같은 표본: 촘촘하면 2×2 평균, 아니면 최근접."""
    h, w = rgba.shape[:2]
    W, H = int(round(w / k * win)), int(round(h / k * win))
    t = rgba.astype(np.float32) / 255
    subs = [(0.5, 0.5)] if k <= win else [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
    acc = np.zeros((H, W, 4), np.float32)
    for sx, sy in subs:
        xs = np.clip(np.floor((np.arange(W) + sx) / win * k).astype(int), 0, w - 1)
        ys = np.clip(np.floor((np.arange(H) + sy) / win * k).astype(int), 0, h - 1)
        s = t[ys][:, xs]
        acc[..., :3] += s[..., :3] * s[..., 3:4]
        acc[..., 3] += s[..., 3]
    acc /= len(subs)
    o = acc[..., :3] + (np.array(bg, np.float32) / 255) * (1 - acc[..., 3:4])
    return (np.clip(o, 0, 1) * 255).astype(np.uint8)


def preview(out_png, frames=(1, 40, 66, 90), win=4.909, src=BIN):
    import fire_fx
    sprites, effects, qtwins = fire_fx.read_raw(src)
    by = dict(effects)
    tiles = []
    for k in frames:
        s = by[BODY][k][0][0]
        t = qtwins[s]
        a3 = dequantize(t[0], t[1]) if t[4] == FROM_SCALE else None
        if a3 is None:
            continue
        a6 = dequantize(*upscale_twin(t)[:2])
        tiles.append((render(a3, FROM_SCALE, win), render(a6, TO_SCALE, win)))
    th = max(max(a.shape[0], b.shape[0]) for a, b in tiles)
    tw = sum(a.shape[1] + b.shape[1] + 24 for a, b in tiles)
    sheet = np.full((th + 8, tw, 3), 24, np.uint8)
    x = 0
    for a, b in tiles:
        sheet[4:4 + a.shape[0], x:x + a.shape[1]] = a
        x += a.shape[1] + 4
        sheet[4:4 + b.shape[0], x:x + b.shape[1]] = b
        x += b.shape[1] + 20
    Image.fromarray(sheet).save(out_png)
    print(f"preview → {out_png} (위: 각 쌍 = 3× ↔ 6×, 창 배율 {win})")


if __name__ == "__main__":
    a = sys.argv[1:]
    if a and a[0] == "replace":
        replace(*(a[1:3] if len(a) > 1 else []))
    elif a and a[0] == "preview" and len(a) > 1:
        ks = tuple(int(v) for v in a[2].split(",")) if len(a) > 2 else (1, 40, 66, 90)
        preview(a[1], ks, float(a[3]) if len(a) > 3 else 4.909)
    else:
        print(__doc__)
