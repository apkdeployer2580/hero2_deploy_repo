#!/usr/bin/env python3
"""의상 201 「율리케의 드레스코드」 초안 생성기 (2026-10-01) — 한국식 하얀 웨딩드레스 세트.

사용자(2026-10-01): *"율리케의 웨딩드레스라고 한국식 하얀 웨딩드레스를 도트로 새로 찍어서 보여줄꺼야. 율리케의 머리,
율리케의 웨딩드레스, 율리케의 구두. 3가지를 만들어줘."*

    python tools_re/costume_draft_ulrike.py            # costumes/201_ulrike/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_ulrike.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_ulrike.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다.** 이 스크립트는 첫 초안을 만든 기록일 뿐이다(`costume_draft_ian.py` 와 같은 규칙).

세 아이템 (칸 = `costumes/README.md`):
  투구 율리케의 머리     head_0..2   맨머리 hero/0/100 의 실루엣·얼굴 그대로, 머리칼 = NPC 율리케(npc/1head/02)의 짙은 보라 ·
                                    하얀 면사포(뒤 · 옆 · 앞 가장자리) + 작은 금빛 티아라
  갑옷 율리케의 웨딩드레스 body_0..4 · arm_0..10 · ★ skirt_0..10
                                    몸통 = 하얀 코르셋(목선 = 살빛, 허리 = 은빛 리본) · 어깨 조각 0·1 = 하얀 퍼프 · 3·4 빈 칸 ·
                                    팔 = 하얀 레이스 소매(손은 살빛) · 치마 = 다리 실루엣을 행마다 감싸 채운 바닥까지 오는 A 라인
                                    (주름 · 밑단 그림자). 치마는 다리 시트 `hero/3/1201` — 드레스를 입는 동안 신발 칸 대신
                                    다리가 된다(`avatar_state::SKIRT_SHEET_OFFSET`): 바닥까지 오는 드레스가 신발을 가린다
  신발 율리케의 구두     leg_0..10   로그 바지 hero/3/102 의 실루엣 = 하얀 스타킹 + 열마다 맨 아래 하얀 구두(드레스 없이 신을 때)

색 = 배포본 격자(00 1f 3f … ff) — 흰색 #ffffff 는 팔레트 0 번(투명)이 아닌 칸에(README ⛔).
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "201_ulrike")
if os.path.isdir(OUT) and any(f.endswith(".png") for f in os.listdir(OUT)) and "--force" not in sys.argv:
    raise SystemExit(f"{OUT}: PNG 가 이미 있다 — 손으로 고친 것일 수 있다. 덮어쓰려면 --force")


def H(s):
    return (int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16))


# ── palette ──────────────────────────────────────────────────────────────────
W = H("#ffffff")  # white (⛔ never index 0 — `write_part` puts opaque colours from 1)
P1 = H("#dfdfff")  # palest lavender
P2 = H("#bfbfdf")  # light shadow
P3 = H("#9f9fbf")  # mid shadow
P4 = H("#7f7f9f")  # deep shadow
OL = H("#5f5f7f")  # the dress's outline
GD = H("#dfbf3f")  # tiara gold
GL = H("#ffff7f")  # tiara light
SV = H("#bfbfbf")  # silver ribbon
# hair — NPC 율리케 (npc/1head/02) on the grid: dark purple
HA = H("#1f1f3f")
HB = H("#3f1f5f")
HC = H("#5f3f7f")
HD = H("#7f5f9f")
HE = H("#9f7fbf")
# skin (the player head's own)
SJ = H("#ffdfbf")
SH = H("#ff9f7f")
SI = H("#df7f7f")
SG = H("#9f3f3f")

jar = cs.open_jar(cs.DEFAULT_ZIP)


def write_part(part, frames_rgb):
    """frames_rgb = [(w, h, rows of rgb-or-None)]. Opaque colours from index 1; read back and checked."""
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
        path = os.path.join(OUT, f"{part}_{k}.png")
        cs.png_write_indexed(path, w, h, palette, px)
        back = cs.Picture(path)
        for y in range(h):
            for x in range(w):
                if (back.rgba(x, y)[3] == 0) != (rows[y][x] is None):
                    raise SystemExit(f"{path}: ({x},{y}) came back {'see-through' if rows[y][x] is not None else 'opaque'}")
    print(part, len(frames_rgb), "frames,", len(palette), "colours")


def rgb_rows(frame, palette):
    return [[None if v == 0 else palette[v] for v in row] for row in frame.pixels]


def lum(c):
    return (2 * c[0] + 5 * c[1] + c[2]) // 8


# ── 투구: 율리케의 머리 ────────────────────────────────────────────────────────
# hero/0/100 frames: 0 = back · 1 = side (face to the left) · 2 = front. The blond hair ramp
# (a ffbf3f · b 7f3f3f · c bf7f3f · d ffff7f · e df9f3f · f ffffff (hair shine)) → purple; the face
# (h ff9f7f · i df7f7f · j ffdfbf · k 1f1f3f · l 5f3f7f · g 9f3f3f; f = eye white INSIDE the face)
# stays. Then the veil is laid over: white on the crown, down the back and the side edges.
bare = cs.shipped_sheet(jar, 0, 100)
HAIR = {H("#7f3f3f"): HA, H("#bf7f3f"): HB, H("#df9f3f"): HC, H("#ffbf3f"): HD, H("#ffff7f"): HE}
FACE = {H("#ff9f7f"), H("#df7f7f"), H("#ffdfbf"), H("#1f1f3f"), H("#5f3f7f"), H("#9f3f3f")}


def head_frame(k):
    fr = bare.frames[k]
    rows = rgb_rows(fr, bare.palette)
    w, h = fr.w, fr.h
    # The face's box: rows and columns holding a face colour (front 8..13, side 8..13 left).
    face = {(x, y) for y in range(h) for x in range(w) if rows[y][x] in FACE}
    fy0 = min(y for _, y in face) if face else h
    out = [[None] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            c = rows[y][x]
            if c is None:
                continue
            if (x, y) in face:
                out[y][x] = c
            elif c == W:  # the hair's shine, or (in the face) an eye white
                inside = any((x + dx, y + dy) in face for dx in (-1, 0, 1) for dy in (-1, 0, 1))
                out[y][x] = W if inside and y >= fy0 else HE
            else:
                out[y][x] = HAIR.get(c, HC)
    # Row extents of the head's silhouette.
    ext = {}
    for y in range(h):
        xs = [x for x in range(w) if rows[y][x] is not None]
        if xs:
            ext[y] = (min(xs), max(xs))
    top = min(ext)

    def veil(x, y, shade):
        if rows[y][x] is not None and (x, y) not in face:
            out[y][x] = shade

    if k == 0:  # back: purple hair on the crown, the tiara's band, the veil draping from there down
        band = top + 3
        for y in ext:
            x0, x1 = ext[y]
            for x in range(x0, x1 + 1):
                if y < band:
                    continue  # the hair (already purple)
                if y == band:
                    out[y][x] = GD if (x - x0) % 2 else GL  # the tiara's band, seen from behind
                    if x in (x0, x1):
                        out[y][x] = HB
                    continue
                rim = x in (x0, x1)
                d = min(x - x0, x1 - x)
                fold = d >= 2 and (x - x0) % 3 == 0 and y > band + 1
                shade = OL if rim else (P2 if d == 1 else (P1 if fold else W))
                if y == max(ext) and not rim:
                    shade = P2 if (x % 2) else P1  # the veil's lace edge
                veil(x, y, shade)
    elif k == 1:  # side, face to the left: the veil on the crown and down the back (right)
        for y in ext:
            x0, x1 = ext[y]
            for x in range(x0, x1 + 1):
                back_side = x >= x1 - (3 if y >= top + 4 else 6)
                crown = y <= top + 1
                if not (back_side or crown):
                    continue
                shade = OL if x == x1 or y == top else (P2 if x == x1 - 1 else W)
                veil(x, y, shade)
        # tiara on the front of the crown
        tx = ext[top + 1][0] + 2
        for x, c in ((tx, GD), (tx + 1, GL), (tx + 2, GD)):
            if rows[top + 1][x] is not None:
                out[top + 1][x] = c
    else:  # front: the veil on the crown, framing both sides; tiara at the top centre
        for y in ext:
            x0, x1 = ext[y]
            for x in range(x0, x1 + 1):
                crown = y <= top + 1
                sides = y >= top + 4 and (x <= x0 + 1 or x >= x1 - 1)
                if not (crown or sides):
                    continue
                shade = OL if (x in (x0, x1) or y == top) else (P2 if (x in (x0 + 1, x1 - 1) and not crown) else W)
                veil(x, y, shade)
        cx = (ext[top + 2][0] + ext[top + 2][1]) // 2
        for x, c in ((cx - 2, GD), (cx - 1, GL), (cx, GL), (cx + 1, GD)):
            if rows[top + 2][x] is not None:
                out[top + 2][x] = c
        if rows[top + 1][cx] is not None:
            out[top + 1][cx] = GL
    return (w, h, out)


write_part("head", [head_frame(k) for k in range(3)])

# ── 갑옷: 율리케의 웨딩드레스 — 몸통 ────────────────────────────────────────────
# body 2 = the 7x5 bodice (every direction draws it): skin neckline · white corset · silver ribbon.
KEY = {".": None, "W": W, "1": P1, "2": P2, "3": P3, "4": P4, "O": OL, "j": SJ, "h": SH, "S": SV}
BODICE = [
    ".2jjj2.",
    "2WWhWW2",
    "OW1W1WO",
    "OWW1WWO",
    ".SSSSS.",
]
# 0 · 1 = the shoulder pieces (a cloth armour's, hero/1/05) → white puff sleeves.
cloth = cs.shipped_sheet(jar, 1, 5)


def puff(fr, pal):
    rows = rgb_rows(fr, pal)
    out = []
    for y, row in enumerate(rows):
        o = []
        for x, c in enumerate(row):
            if c is None:
                o.append(None)
                continue
            l = lum(c)
            o.append(OL if l < 0x30 else P3 if l < 0x60 else P2 if l < 0x90 else P1 if l < 0xc0 else W)
        out.append(o)
    return (fr.w, fr.h, out)


body = []
for k in range(5):
    if k == 2:
        body.append((7, 5, [[KEY[ch] for ch in r] for r in BODICE]))
    elif k in (0, 1):
        body.append(puff(cloth.frames[k], cloth.palette))
    else:
        body.append((1, 1, [[None]]))
write_part("body", body)

# ── 갑옷 — 팔: white lace sleeves, the hand (the glove sheet's blue region) bare ──
arm_base = cs.shipped_sheet(jar, 2, 100)
glove = cs.shipped_sheet(jar, 2, 0)
blue = {i for i, c in enumerate(glove.palette) if i and c[2] > c[0] + 20}
# hero/2/100 skin ramp: c 5f1f3f < a 7f3f3f < b bf5f3f < e bf7f3f < d ff9f7f < f ffdfbf
SLEEVE = {H("#5f1f3f"): OL, H("#7f3f3f"): P4, H("#bf5f3f"): P2, H("#bf7f3f"): P2, H("#ff9f7f"): P1, H("#ffdfbf"): W}
arms = []
for k, fr in enumerate(arm_base.frames):
    rows = rgb_rows(fr, arm_base.palette)
    hand = {(x, y) for y in range(fr.h) for x in range(fr.w) if fr.pixels[y][x] and glove.frames[k].pixels[y][x] in blue}
    out = [[None if c is None else (c if (x, y) in hand else SLEEVE[c]) for x, c in enumerate(row)] for y, row in enumerate(rows)]
    arms.append((fr.w, fr.h, out))
write_part("arm", arms)

# ── 갑옷 — 치마 (hero/3/1201): every row of the leg silhouette wrapped and filled to the floor ──
leg_base = cs.shipped_sheet(jar, 3, 102)


def skirt_frame(fr):
    """An A-line gown to the floor over this leg frame: the waist = the legs' top row (centred where
    the legs are), widening one pixel a side every two rows, never narrower than the legs (a stride
    swings the hem out with them), clipped to the frame (its size is fixed — §8.1)."""
    w, h = fr.w, fr.h
    ext = {}
    for y in range(h):
        xs = [x for x in range(w) if fr.pixels[y][x]]
        if xs:
            ext[y] = (min(xs), max(xs))
    top, bottom = min(ext), max(ext)
    # The waist: the bodice is 7 wide — start at 6 around the legs' own centre, then a bell: one
    # pixel a side for each of the first rows, the rest of the way to the frame's edge by the hem.
    cx = (ext[top][0] + ext[top][1]) / 2
    half = 2.5
    rows = {}
    for y in range(top, bottom + 1):
        t = (y - top) / max(1, bottom - top)
        reach = half + min(y - top, 3) + t * t * w / 2
        x0, x1 = round(cx - reach), round(cx + reach)
        if y in ext and y > top + 1:
            x0, x1 = min(x0, ext[y][0]), max(x1, ext[y][1])  # a stride swings the hem out with the legs
        rows[y] = (max(0, x0), min(w - 1, x1))
    out = [[None] * w for _ in range(h)]
    for y in range(top, bottom + 1):
        x0, x1 = rows[y]
        if y == bottom and x1 - x0 >= 5:
            x0, x1 = x0 + 1, x1 - 1  # a rounded hem: the corners fall away
        span = max(1, x1 - x0)
        for x in range(x0, x1 + 1):
            edge = x in (x0, x1)
            u = (x - x0) / span  # 0 = the lit left, 1 = the shaded right
            fold = y >= top + 3 and not edge and any(abs(x - (x0 + span * f)) < 0.5 for f in (0.33, 0.66))
            if y == top:
                c = OL if edge else SV  # the waist seam, a silver sash
            elif edge:
                c = OL
            elif y == bottom:
                c = P3 if x % 2 else P2  # the lace hem
            elif y == bottom - 1:
                c = P2 if u > 0.6 else P1
            elif fold:
                c = P2 if u > 0.5 else P1
            elif u > 0.75:
                c = P2
            elif u > 0.55:
                c = P1
            else:
                c = W
            out[y][x] = c
    return (w, h, out)


write_part("skirt", [skirt_frame(fr) for fr in leg_base.frames])

# ── 신발: 율리케의 구두 (hero/3/201) — white stockings, white heels at the bottom of each column ──
STOCKING = {H("#1f1f3f"): P4, H("#3f3f3f"): P3, H("#7f1f3f"): P3, H("#5f5f3f"): P2, H("#7f7f3f"): P2, H("#bf7f3f"): P1,
            H("#9f3f3f"): P3, H("#ff9f7f"): P1, H("#ffdfbf"): W, H("#9f9f7f"): P1, H("#df7f7f"): P1}
legs = []
for fr in leg_base.frames:
    rows = rgb_rows(fr, leg_base.palette)
    w, h = fr.w, fr.h
    bottom = {x: max((y for y in range(h) if rows[y][x] is not None), default=-1) for x in range(w)}
    out = []
    for y, row in enumerate(rows):
        o = []
        for x, c in enumerate(row):
            if c is None:
                o.append(None)
            elif y >= bottom[x] - 1:
                o.append(OL if y == bottom[x] else W)  # the shoe: a white heel over a dark sole
            else:
                o.append(STOCKING[c])
        out.append(o)
    legs.append((w, h, out))
write_part("leg", legs)

# ── costume.txt ──────────────────────────────────────────────────────────────
TXT = """# 의상 201 — 「율리케의 드레스코드」 (Claude 초안 2026-10-01, tools_re/costume_draft_ulrike.py)
set         = 율리케의 드레스코드
head        = 율리케의 머리
head.desc   = 하얀 면사포를 쓴 머리.
armor       = 율리케의 웨딩드레스
armor.desc  = 순백의 웨딩드레스.
shoes       = 율리케의 구두
shoes.desc  = 새하얀 하이힐.
"""
with open(os.path.join(OUT, "costume.txt"), "w", encoding="utf-8", newline="\n") as f:
    f.write(TXT)
print("costume.txt")
