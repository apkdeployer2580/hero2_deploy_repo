#!/usr/bin/env python3
"""의상 200 「이안의 낡은 옷」 초안 생성기 (2026-09-30) — 배포본 시트의 실루엣을 그대로 두고 이안의 색으로 다시 칠한다.

    python tools_re/costume_draft_ian.py            # costumes/200_ian/ 에 PNG 27 장 (이미 있으면 거절)
    python tools_re/costume_draft_ian.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다.** 이 스크립트는 첫 초안을 만든 기록일 뿐이다 — 사용자가 PNG 를 고친 뒤에 다시 돌리면 그 수정을
덮어쓴다(그래서 기본은 거절). 사용자 결정(2026-09-30): 「Claude 초안 → 사용자 수정」 · 몸통+팔+다리 전부 덮는다.

바탕(실루엣은 한 픽셀도 안 바꾼다 — HERO2_AVATAR_PLAN.md §8.1 「가장 안전한 제작법」):
  몸통  프레임 2 = 손으로 그린 7x5 (황갈 칼라 · 남색 재킷 · 진홍 허리띠) · 0·1 = 천 갑옷 hero/1/05 의 어깨 조각 → 황갈 망토 · 3·4 = 빈 칸
  팔    hero/2/100(맨팔) — 손·아래팔 = 글러브 시트 hero/2/00 의 파란 영역 → 갈색 가죽 장갑 · 어깨 끝 → 황갈 망토 · 나머지 → 남색 소매
  다리  hero/3/102(로그 바지) — 맨 윗줄 → 진홍 허리띠 · 다음 두 줄 → 검정/남색 바지 · 정강이 → 황갈 각반 · 열마다 맨 아래 → 적갈 부츠
색 = NPC 이안(npc/npc 레코드 0 = npc/2body·3arm·4leg/00)의 팔레트 + 명암 한두 단계.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "200_ian")
if os.path.isdir(OUT) and any(f.endswith(".png") for f in os.listdir(OUT)) and "--force" not in sys.argv:
    raise SystemExit(f"{OUT}: PNG 가 이미 있다 — 손으로 고친 것일 수 있다. 덮어쓰려면 --force")


def H(s):
    return (int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16))


# ── Ian palette ──────────────────────────────────────────────────────────────
K = H("#000000")  # outline
N = H("#1f1f3f")  # navy
Q = H("#3f3f5f")  # navy mid
S = H("#5f5f7f")  # slate highlight
O = H("#5f5f3f")  # olive (khaki shadow)
E = H("#9f9f7f")  # khaki mid
T = H("#bf9f7f")  # tan khaki
L = H("#dfbfbf")  # light trim highlight
R = H("#9f1f3f")  # crimson sash
C = H("#bf1f3f")  # crimson light
M = H("#5f1f3f")  # maroon
P = H("#3f1f3f")  # boot dark
B = H("#9f5f3f")  # glove brown
W = H("#bf7f3f")  # glove highlight
X = H("#7f3f3f")  # glove shadow

jar = cs.open_jar(cs.DEFAULT_ZIP)


def remap(frame, palette, region):
    """region(x, y) -> {source rgb: new rgb}."""
    rows = []
    for y, row in enumerate(frame.pixels):
        out = []
        for x, v in enumerate(row):
            if not v:
                out.append(None)
                continue
            src = palette[v]
            table = region(x, y)
            if src not in table:
                raise SystemExit(f"no mapping for #{src[0]:02x}{src[1]:02x}{src[2]:02x} at ({x},{y})")
            out.append(table[src])
        rows.append(out)
    return (frame.w, frame.h, rows)


def write_part(part, frames_rgb):
    # ⛔ Index 0 is the TRANSPARENT entry (#ffffff): an opaque white gets an index of its own
    # (2026-10-01 — the gear draft's white pixels fell onto index 0 and turned see-through; no
    # colour of this draft is white, but the same code must not bite the next one).
    opaque = []
    for _w, _h, rows in frames_rgb:
        for row in rows:
            for c in row:
                if c is not None and c not in opaque:
                    opaque.append(c)
    palette = [cs.TRANSPARENT_RGB] + opaque
    os.makedirs(OUT, exist_ok=True)
    for k, (w, h, rows) in enumerate(frames_rgb):
        px = [[0 if c is None else 1 + opaque.index(c) for c in row] for row in rows]
        cs.png_write_indexed(os.path.join(OUT, f"{part}_{k}.png"), w, h, palette, px)
    print(part, len(frames_rgb), "frames,", len(palette), "colours")


def m(**kw):
    return {H("#" + k[1:]): v for k, v in kw.items()}


# ── body ─────────────────────────────────────────────────────────────────────
TORSO = [
    ".ETLTE.",
    "OTLNLTO",
    "NQQNQSN",
    "KNQNQQK",
    ".MRCRM.",
]
KEY = {".": None, "K": K, "N": N, "Q": Q, "S": S, "O": O, "E": E, "T": T, "L": L, "R": R, "C": C, "M": M}
cloth = cs.shipped_sheet(jar, 1, 5)
MANTLE = m(x000000=K, x1f1f3f=N, x3f1f3f=N, x3f3f3f=O, x5f5f7f=E, x7f7f7f=T, xbfbfbf=L)
body = []
for k in range(5):
    if k == 2:
        body.append((7, 5, [[KEY[ch] for ch in r] for r in TORSO]))
    elif k in (0, 1):
        body.append(remap(cloth.frames[k], cloth.palette, lambda x, y: MANTLE))
    else:
        body.append((1, 1, [[None]]))
write_part("body", body)

# ── arms: glove region = brown leather, the shoulder end = khaki mantle, the rest = dark sleeve ──
# hero/2/100 skin: c #5f1f3f < a #7f3f3f < b #bf5f3f < e #bf7f3f < d #ff9f7f < f #ffdfbf
SLEEVE = m(x5f1f3f=K, x7f3f3f=N, xbf5f3f=Q, xbf7f3f=Q, xff9f7f=S, xffdfbf=S)
GLOVE = m(x5f1f3f=P, x7f3f3f=X, xbf5f3f=B, xbf7f3f=B, xff9f7f=W, xffdfbf=W)
SHOULDER = m(x5f1f3f=N, x7f3f3f=O, xbf5f3f=E, xbf7f3f=E, xff9f7f=T, xffdfbf=L)
arm_base = cs.shipped_sheet(jar, 2, 100)
glove = cs.shipped_sheet(jar, 2, 0)
blue = {i for i, c in enumerate(glove.palette) if i and c[2] > c[0] + 20}


def bfs(frame, seeds):
    d, q = {s: 0 for s in seeds}, list(seeds)
    while q:
        x, y = q.pop(0)
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < frame.w and 0 <= ny < frame.h and frame.pixels[ny][nx] and (nx, ny) not in d:
                d[(nx, ny)] = d[(x, y)] + 1
                q.append((nx, ny))
    return d


arms = []
for k, fr in enumerate(arm_base.frames):
    gl = {(x, y) for y in range(fr.h) for x in range(fr.w) if fr.pixels[y][x] and glove.frames[k].pixels[y][x] in blue}
    far = bfs(fr, gl) if gl else {}
    reach = max(far.values()) if far else 0

    def region(x, y, gl=gl, far=far, reach=reach):
        if (x, y) in gl:
            return GLOVE
        if not gl:  # no hand in view (frame 6): mantle on top, sleeve below
            return SHOULDER if y <= 1 else SLEEVE
        return SHOULDER if far.get((x, y), 0) >= max(3, reach - 1) else SLEEVE

    arms.append(remap(fr, arm_base.palette, region))
write_part("arm", arms)

# ── legs: top row = sash, next rows = black trousers, shins = khaki wraps, lowest pixel per column = boot ──
# hero/3/102: a #1f1f3f · b #3f3f3f · c #7f1f3f · d #5f5f3f · e #7f7f3f · f #bf7f3f · g #9f3f3f ·
#             h #ff9f7f · i #ffdfbf · j #9f9f7f · k #df7f7f
SASH = m(x1f1f3f=M, x3f3f3f=M, x7f1f3f=M, x5f5f3f=R, x7f7f3f=R, xbf7f3f=C, x9f3f3f=R, xff9f7f=C, xffdfbf=C, x9f9f7f=C, xdf7f7f=C)
TROUSERS = m(x1f1f3f=K, x3f3f3f=N, x7f1f3f=N, x5f5f3f=Q, x7f7f3f=Q, xbf7f3f=S, x9f3f3f=N, xff9f7f=S, xffdfbf=S, x9f9f7f=S, xdf7f7f=S)
WRAPS = m(x1f1f3f=N, x3f3f3f=O, x7f1f3f=O, x5f5f3f=E, x7f7f3f=E, xbf7f3f=T, x9f3f3f=O, xff9f7f=T, xffdfbf=L, x9f9f7f=T, xdf7f7f=T)
BOOT = m(x1f1f3f=K, x3f3f3f=P, x7f1f3f=P, x5f5f3f=M, x7f7f3f=M, xbf7f3f=X, x9f3f3f=M, xff9f7f=X, xffdfbf=X, x9f9f7f=X, xdf7f7f=X)
leg_base = cs.shipped_sheet(jar, 3, 102)
legs = []
for k, fr in enumerate(leg_base.frames):
    top = min(y for y in range(fr.h) if any(fr.pixels[y]))
    bottom = {x: max((y for y in range(fr.h) if fr.pixels[y][x]), default=-1) for x in range(fr.w)}

    def region(x, y, top=top, bottom=bottom):
        if y == top:
            return SASH
        if y >= top + 5 and y >= bottom[x]:
            return BOOT
        if y >= top + 3:
            return WRAPS
        return TROUSERS

    legs.append(remap(fr, leg_base.palette, region))
write_part("leg", legs)
