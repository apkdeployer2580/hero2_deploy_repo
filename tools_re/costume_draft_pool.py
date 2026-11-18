#!/usr/bin/env python3
"""의상 203 「수영장 파티」 초안 생성기 (2026-10-01) — HELL 클리어(난이도 EASY 이상) 보상.

사용자(2026-10-01): *"헬모드 클리어 조건(난이도 easy 이상)으로 비키니 클레르도 만들꺼야. 이름은 수영장 파티 - 수영모, 수영장
파티 - 비키니 상의, 수영장 파티- 비키니 하의 로 3개 만들꺼고, 무기는 튜브 형태로 만들꺼야. 대검은 큰 튜브, 도검은 파란 풍선으로
만든 검 모양, 방패는 풍선으로 만든 방패 모양, 단검은 큰오리인형, 작은 오리인형 으로 만들꺼야. 무기류 도트는 너가 최대한 잘
찍어보고 맘에안들면 그냥 제거할꺼야."* + *"클레르 수영장파티 머리아바타는 수영모 말고 선글라스로 해줘. 일반 클레르머리에
선글라스가 올라가있는 형태야."*

    python tools_re/costume_draft_pool.py            # costumes/203_pool/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_pool.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_pool.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다.** 이 스크립트는 첫 초안의 기록이다(`costumes/README.md`).

아이템 (이름은 「수영장 파티-…」 — 이름칸 114 px 에 「수영장 파티 - 비키니 상의」(121 px)가 안 들어가서 하이픈 옆 공백을 뺐다):
  투구 수영장 파티-선글라스   head_0..2   맨머리 hero/0/100(클레르의 금발) + 머리 위에 걸친 선글라스(검은 알 · 하늘빛 반사 · 분홍 테)
  갑옷 수영장 파티-비키니 상의 body_2 · arm_0..10
                                       몸통 = 맨살 + 분홍 물방울 비키니 상의 · 팔 = 맨팔 hero/2/100 그대로
  신발 수영장 파티-비키니 하의 leg_0..10   로그 바지 hero/3/102 의 실루엣 = 맨다리 + 허리 두 줄 분홍 비키니 하의 + 하늘색 슬리퍼
  무기 버서커 수영장 파티-큰 튜브        weapon_berserker_0..6   빨강·하양 줄무늬 튜브 — 손잡이에서 칼끝까지가 고리 하나
  무기 나이트 수영장 파티-풍선 검        weapon_knight_0..4      파란 풍선 마디 셋(칼날) + 코등이 풍선 둘 + 손잡이 풍선
  무기 로그   수영장 파티-큰오리인형     weapon_rogue_0..4       노란 오리인형(8×8) — 4 번은 배포본처럼 1×1 빈 칸
  보조손 나이트 수영장 파티-풍선 방패    offhand_knight_0..2     방패 모양으로 묶은 색 풍선(앞 · 옆 · 뒤)
  보조손 로그   수영장 파티-작은오리인형 offhand_rogue_0..3      작은 노란 오리인형(6×6)
무기 프레임은 크기와 손잡이 자리가 고정이라 모양은 `costume_draft_lib.GRIPS` 의 축을 따라 그린다.
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
import costume_sheets as cs  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "203_pool")
lib.refuse_overwrite(OUT, ["head", "body", "arm", "leg", "weapon_berserker", "weapon_knight", "weapon_rogue", "offhand_knight", "offhand_rogue"])

jar = cs.open_jar(cs.DEFAULT_ZIP)

W = H("#ffffff")
# 피부 (맨팔 hero/2/100 · 맨머리)
S0 = H("#5f1f3f")
SO = H("#7f3f3f")
SD = H("#bf5f3f")
SE = H("#bf7f3f")
SM = H("#ff9f7f")
SL = H("#ffdfbf")
# 분홍 비키니
PO = H("#7f1f3f")
PD = H("#df1f7f")
PB = H("#ff5f9f")
PL = H("#ffbfdf")
# 선글라스
GK = H("#1f1f3f")
GS = H("#9fdfff")

# ── 투구: 맨머리 + 머리 위에 걸친 선글라스 ─────────────────────────────────────
bare = cs.shipped_sheet(jar, 0, 100)
heads = [Canvas.from_frame(fr, bare.palette) for fr in bare.frames]
# 앞(2): 이마 위 머리칼에 알 둘(3×2, 하늘빛 반사 한 점) + 분홍 다리 · 옆(1): 앞쪽 알 하나 + 뒤로 가는 다리 · 뒤(0): 귀 위의
# 다리 끝만(알은 머리에 가린다).
front = heads[2]
for x0 in (4, 9):
    for dx in range(3):
        for dy in range(2):
            front.put(x0 + dx, 4 + dy, GK)
    front.put(x0, 4, GS)
for x in (7, 8):
    front.put(x, 4, PD)
for x, y in ((3, 4), (12, 4), (2, 5), (13, 5)):
    front.put(x, y, PB)
side = heads[1]
for dx in range(3):
    for dy in range(2):
        side.put(2 + dx, 4 + dy, GK)
side.put(2, 4, GS)
for x in range(5, 10):
    side.put(x, 4, PB)
side.put(10, 5, PD)
back = heads[0]  # 뒤에서는 안경알이 머리에 가린다 — 귀 위의 분홍 다리 끝만
for x, y in ((2, 6), (13, 6)):
    back.put(x, y, PB)
lib.write_part(OUT, "head", heads)

# ── 갑옷: 몸통 — 맨살 + 분홍 물방울 비키니 상의 (모든 방향이 이 한 장) ───────────────
KEY = {"o": SO, "s": SM, "L": SL, "d": SD, "q": PO, "p": PD, "P": PB, "h": PL, "W": W}
BIKINI = [
    ".sLLLs.",
    "qPWsPWq",
    "qpPsPpq",
    "osLLLdo",
    ".sLLLd.",
]
lib.write_part(OUT, "body", [lib.BLANK, lib.BLANK, Canvas.from_art(BIKINI, KEY), lib.BLANK, lib.BLANK])

# ── 갑옷: 팔 = 맨팔 그대로 ─────────────────────────────────────────────────────
arm_base = cs.shipped_sheet(jar, 2, 100)
lib.write_part(OUT, "arm", [Canvas.from_frame(fr, arm_base.palette) for fr in arm_base.frames])

# ── 신발: 맨다리 + 비키니 하의 + 슬리퍼 (로그 바지 hero/3/102 의 실루엣) ─────────────
# 바지의 짙은 천(#3f3f3f 등)이 많아서 그대로 내리면 다리가 갈색이 된다 — 팔(맨팔)과 같은 밝기가 되게 한 단씩 올렸다.
SKIN = {
    H("#1f1f3f"): SO,
    H("#3f3f3f"): SD,
    H("#7f1f3f"): SD,
    H("#9f3f3f"): SE,
    H("#5f5f3f"): SM,
    H("#7f7f3f"): SM,
    H("#bf7f3f"): SM,
    H("#df7f7f"): SM,
    H("#ff9f7f"): SL,
    H("#9f9f7f"): SL,
    H("#ffdfbf"): SL,
}
SLIPPER = H("#3f9fdf")
leg_base = cs.shipped_sheet(jar, 3, 102)
legs = []
for fr in leg_base.frames:
    rows = lib.rgb_rows(fr, leg_base.palette)
    w, h = fr.w, fr.h
    top = min(y for y in range(h) if any(rows[y]))
    bottom = {x: max((y for y in range(h) if rows[y][x] is not None), default=-1) for x in range(w)}
    cv = Canvas(w, h)
    for y, row in enumerate(rows):
        for x, c in enumerate(row):
            if c is None:
                continue
            if y <= top + 1:
                cv.put(x, y, PO if c == H("#1f1f3f") else (PL if lib.lum(c) > 150 else PB if lib.lum(c) > 80 else PD))
            elif y >= top + 7 and bottom[x] >= h - 3 and y == bottom[x]:
                cv.put(x, y, SLIPPER)
            else:
                cv.put(x, y, SKIN[c])
    legs.append(cv)
lib.write_part(OUT, "leg", legs)

# ── 무기 ────────────────────────────────────────────────────────────────────────


def fit_b(w, h, cx, cy, ux, uy, a, cap):
    """장축 a 의 타원이 프레임 안에 드는 가장 큰 단축(≤ cap)."""
    b = cap
    while b > 0.6:
        ok = True
        for i in range(48):
            th = 2 * math.pi * i / 48
            u, v = a * math.cos(th), b * math.sin(th)
            x, y = cx + u * ux - v * uy, cy + u * uy + v * ux
            if not (-0.2 <= x <= w + 0.2 and -0.2 <= y <= h + 0.2):
                ok = False
                break
        if ok:
            return b
        b -= 0.1
    return 0.6


RED = [H("#ff9f9f"), H("#ff5f5f"), H("#ff3f3f"), H("#df1f3f"), H("#9f1f3f")]
WHITE = [W, W, H("#dfdfff"), H("#bfbfdf"), H("#7f7f9f")]
RED_O, WHITE_O = H("#5f1f3f"), H("#3f3f5f")
L3 = (-0.45, -0.6, 0.66)  # 3D 빛: 왼쪽 위 + 앞


# 튜브의 타원 — 프레임마다 (중심, 장축 방향, 장축 반지름, 단축 반지름). 칼날 축을 그대로 쓰면 긴 프레임(10×23 · 25×10 ·
# 24×7)에서 타원이 대각선에 갇혀 실처럼 가늘어진다 — 프레임을 채우는 타원을 손으로 정했다. 손(GRIPS 의 손잡이)은 언제나
# 고리의 한쪽 끝 둘레 위에 온다. 3×7 · 3×18 = 옆에서 본 튜브(구멍 없음).
TUBES = [
    ((5.0, 11.8), (0.2, 0.98), 11.1, 4.7),
    ((12.4, 5.2), (-0.976, 0.22), 12.1, 4.6),
    ((9.0, 9.0), (-0.707, 0.707), 9.2, 6.6),
    ((12.0, 3.5), (0.992, -0.12), 11.8, 3.3),
    ((12.0, 3.5), (0.992, 0.12), 11.8, 3.3),
    ((1.5, 3.5), (0.0, 1.0), 3.45, 1.5),
    ((1.5, 9.0), (0.0, 1.0), 8.95, 1.5),
]


def tube_frame(k):
    """튜브 = 프레임을 채우는 타원 고리(TUBES). 굵은 고리(두께 ≥ 2.5 px)라야 가운데에 명암이 남는다 — 외곽선은 바깥 둘레에만
    (구멍 둘레는 그늘), 줄무늬 여섯(빨강·하양), 명암은 원환의 단면 법선과 왼쪽 위 빛. 옆에서 본 튜브는 길이를 따라 띠 여섯."""
    (w, h), _g, _t = lib.axis("weapon_berserker", k)
    (cx, cy), (ux, uy), a, b = TUBES[k]
    ux, uy = lib._unit(ux, uy)
    tw = max(2.0, min(b * 0.62, 4.2))
    a_in, b_in = a - tw, b - tw
    hole = b_in >= 0.6
    am, bm = ((a + a_in) / 2, (b + b_in) / 2) if hole else (a, b)
    cv = Canvas(w, h)
    ring, outer = {}, set()
    for y in range(h):
        for x in range(w):
            px, py = x + 0.5 - cx, y + 0.5 - cy
            u = px * ux + py * uy
            v = -px * uy + py * ux
            if (u / a) ** 2 + (v / b) ** 2 > 1.0:
                continue
            outer.add((x, y))
            if hole and (u / a_in) ** 2 + (v / b_in) ** 2 <= 1.0:
                continue
            ring[(x, y)] = (u, v)

    def stripe(u, v):
        if not hole:
            return int((u / a + 1) * 3) % 6
        phi = math.atan2(v / bm, u / am)
        return int((phi + math.pi) / (2 * math.pi) * 6 + 0.25) % 6

    for (x, y), (u, v) in ring.items():
        if hole:
            q = math.hypot(u / am, v / bm) - 1.0
            hw = ((a - a_in) / 2) / am
            off = max(-1.0, min(1.0, q / hw))
            nx, ny = lib._unit((u / (am * am)) * ux - (v / (bm * bm)) * uy, (u / (am * am)) * uy + (v / (bm * bm)) * ux)
        else:
            off = max(-1.0, min(1.0, v / b))  # 옆에서 본 튜브: 단축이 단면
            nx, ny = -uy, ux
        z = math.sqrt(max(0.0, 1 - off * off))
        bright = nx * off * L3[0] + ny * off * L3[1] + z * L3[2]
        ramp = RED if stripe(u, v) % 2 == 0 else WHITE
        t = (0.95 - bright) / 1.3
        cv.put(x, y, ramp[max(0, min(4, int(t * 5)))])
    for (x, y), (u, v) in ring.items():
        if any((x + dx, y + dy) not in outer for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
            cv.put(x, y, RED_O if stripe(u, v) % 2 == 0 else WHITE_O)
    return cv


lib.write_part(OUT, "weapon_berserker", [tube_frame(k) for k in range(7)])

BLUE = [H("#dfffff"), H("#9fdfff"), H("#5fbfff"), H("#3f9fff"), H("#3f7fdf")]
BLUE_O = H("#1f3f9f")


def balloon_sword(k):
    (w, h), (gx, gy), (tx, ty) = lib.axis("weapon_knight", k)
    ux, uy = lib._unit(tx - gx, ty - gy)
    px, py = -uy, ux
    L = math.hypot(tx - gx, ty - gy)
    wide = min(w, h) > 3
    br = 1.9 if wide else 1.5
    s = lib.Shape()
    s.ellipse(gx + ux * 0.3, gy + uy * 0.3, ux, uy, 1.6, 1.2 if wide else 1.5, "hilt")  # 손잡이 풍선
    if wide:
        for sgn in (-1, 1):
            s.circle(gx + ux * 2.3 + px * sgn * 1.7, gy + uy * 2.3 + py * sgn * 1.7, 1.45, "guard")
    else:
        s.ellipse(gx + ux * 2.3, gy + uy * 2.3, ux, uy, 1.1, 1.5, "guard")
    s0, s1 = 3.6, L + 0.7
    n = 3 if L > 10 else 2
    seg = (s1 - s0) / n
    for i in range(n):
        c = s0 + (i + 0.5) * seg
        s.ellipse(gx + ux * c, gy + uy * c, ux, uy, seg / 2 + 0.05, br, "blade")
    return lib.render_shape(w, h, s, lambda tag: BLUE, lambda tag: BLUE_O, lambda tag: W)


lib.write_part(OUT, "weapon_knight", [balloon_sword(k) for k in range(5)])

# 노란 오리인형 — 왼쪽을 본다(오른쪽으로 가는 칼날 프레임에서는 뒤집는다).
DUCK_KEY = {"o": H("#9f5f1f"), "Y": H("#ffdf3f"), "y": H("#dfbf3f"), "L": H("#ffff9f"), "e": H("#1f1f3f"), "w": W, "B": H("#ff7f1f"), "b": H("#df5f1f")}
BIG_DUCK = [
    "..ooo...",
    ".oLYYo..",
    ".oYweo..",
    "oBBYYo.o",
    "obbYYoyo",
    ".ooLYYYo",
    ".oYYYyyo",
    "..oooo..",
]
SMALL_DUCK = [
    ".ooo..",
    "oLeo..",
    "BBYo.o",
    ".oYYYo",
    ".oyyyo",
    "..ooo.",
]


def duck_frame(part, k, art):
    (w, h), (gx, gy), (tx, ty) = lib.axis(part, k)
    sprite = Canvas.from_art(art, DUCK_KEY)
    if tx > gx:  # 칼날이 오른쪽으로 가면 오리도 오른쪽을 본다
        sprite = sprite.flipped_x()
    ux, uy = lib._unit(tx - gx, ty - gy)
    r = max(sprite.w, sprite.h) / 2
    cx, cy = gx + ux * r, gy + uy * r  # 손이 오리의 꽁무니를 쥔다
    ox = int(round(cx - sprite.w / 2))
    oy = int(round(cy - sprite.h / 2))
    ox = max(0, min(w - sprite.w, ox))
    oy = max(0, min(h - sprite.h, oy))
    cv = Canvas(w, h)
    cv.stamp(sprite, ox, oy)
    return cv


lib.write_part(OUT, "weapon_rogue", [duck_frame("weapon_rogue", k, BIG_DUCK) for k in range(4)] + [lib.BLANK])
lib.write_part(OUT, "offhand_rogue", [duck_frame("offhand_rogue", k, SMALL_DUCK) for k in range(4)])

# 풍선 방패 — 앞(15×17)은 색 풍선 여섯을 방패 모양으로, 옆(7×17)은 그 세로 단면, 뒤(15×17)는 뒷면(어둡게) + 손잡이 끈.
SHIELD_KEY = {
    "o": H("#3f1f5f"),
    "Y": H("#ffdf3f"),
    "y": H("#dfaf1f"),
    "P": H("#ff5f9f"),
    "p": H("#df1f7f"),
    "B": H("#5f9fff"),
    "b": H("#3f5fdf"),
    "G": H("#5fdf5f"),
    "g": H("#3f9f3f"),
    "w": W,
    "k": H("#1f1f3f"),
    "n": H("#5f5f7f"),
}
SHIELD_FRONT = [
    "...............",
    "...ooo.ooo.ooo.",
    "..oPwPoYwYoBwBo",
    "..oPPpoYYyoBBbo",
    "..oPppoYyyoBbbo",
    "...oooGooooooo.",
    "...oBwBoGwGoo..",
    "...oBBboGGgPo..",
    "....obbbogPPo..",
    "....oooYwYoo...",
    ".....oYYYyo....",
    "......oyyo.....",
    ".......oo......",
    "...............",
    "...............",
    "...............",
    "...............",
]
SHIELD_SIDE = [
    ".......",
    "..oo...",
    ".oPwo..",
    ".oPpo..",
    ".oopo..",
    ".oGwo..",
    ".oGgo..",
    ".oogo..",
    ".oBwo..",
    ".oBbo..",
    "..obo..",
    "..oYo..",
    "...o...",
    ".......",
    ".......",
    ".......",
    ".......",
]


def darker(art):
    """뒷면 = 같은 풍선을 한 단 어둡게 + 가운데 손잡이 끈."""
    dark = {"P": "p", "Y": "y", "B": "b", "G": "g", "w": "n"}
    out = [list("".join(dark.get(ch, ch) for ch in row)) for row in art]
    for y in range(4, 9):
        out[y][7] = "k"
    out[6][6] = out[6][8] = "k"
    return ["".join(r) for r in out]


shield_front = Canvas.from_art(SHIELD_FRONT, SHIELD_KEY)
shield_back = Canvas.from_art(darker(SHIELD_FRONT), SHIELD_KEY).flipped_x()
lib.write_part(OUT, "offhand_knight", [shield_front, Canvas.from_art(SHIELD_SIDE, SHIELD_KEY), shield_back])

lib.write_text(
    OUT,
    """# 의상 203 — 「수영장 파티」 (Claude 초안 2026-10-01, tools_re/costume_draft_pool.py) — HELL 클리어(난이도 EASY 이상) 보상.
# 이름 = 「수영장 파티-…」: 「수영장 파티 - 비키니 상의」는 121 px 라 이름칸(114 px)에 안 들어간다 — 하이픈 옆 공백을 뺐다.
set                 = 수영장 파티
head                = 수영장 파티-선글라스
head.desc           = 머리 위에 얹은 선글라스
armor               = 수영장 파티-비키니 상의
armor.desc          = 분홍 물방울 비키니 상의
shoes               = 수영장 파티-비키니 하의
shoes.desc          = 분홍 비키니와 슬리퍼
weapon.berserker    = 수영장 파티-큰 튜브
weapon.berserker.desc = 대검처럼 휘두르는 튜브
weapon.knight       = 수영장 파티-풍선 검
weapon.knight.desc  = 파란 풍선으로 만든 검
weapon.rogue        = 수영장 파티-큰오리인형
weapon.rogue.desc   = 말랑말랑한 큰 오리인형
offhand.knight      = 수영장 파티-풍선 방패
offhand.knight.desc = 풍선으로 만든 방패
offhand.rogue       = 수영장 파티-작은오리인형
offhand.rogue.desc  = 말랑말랑한 작은 오리인형
""",
)
