#!/usr/bin/env python3
"""의상 208 「순백의 해머」 초안 생성기 (2026-10-09) — 가디언 모드(4회차) 클리어 보상, 난이도 NORMAL 이상(진행도 95 %).

사용자(2026-10-09): *"두번쨰 스크린샷은 가디언 노말 클리어했을떄"* — 그림 = 무릎까지 오는 백금발 · 짙은 리본과 노란 매듭의
하얀 보닛 · 보라 눈 · 어깨가 드러난 회색 블라우스(부푼 짧은 소매) · 짙은 허리띠 · 회색 주름 치마 · 맨발 · 몸보다 큰 망치(검은 쇠
자루 · 크림빛 머리 · 주황 장식). 이름은 Claude 가 정했다(확인 대기).

    python tools_re/costume_draft_hammer.py            # costumes/208_hammer/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_hammer.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_hammer.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다** (`costumes/README.md`). 그림 도구 = `costume_draft_kit.py`.

아이템:
  투구 순백의 해머-보닛      head_0..2    하얀 보닛(짙은 리본 띠 · 노란 매듭 · 주름 테 · 뒤로 늘어진 리본 끈) + 백금빛 긴 머리 · 보라 눈
  갑옷 순백의 해머-블라우스  body · arm   몸통 = 드러난 어깨 · 주름 테 · 회색 블라우스 · 짙은 허리띠 · 어깨 조각 = 부푼 짧은 소매 ·
                                          팔 = 맨팔(어깨 쪽 한두 칸은 부푼 소매)
  신발 순백의 해머-주름 치마 leg_0..10    ★ 2026-10-09 허리부터 허벅지 중간까지 일곱 줄 = 회색 주름 치마(엉덩이 · 다리 사이를 덮고
                                          아래로 벌어짐, 주름 끝이 번갈아 접힌 밑단) · 그 밑 맨다리(한 줄 그늘) · 맨발
  무기 버서커 순백의 해머-거대 망치   weapon_berserker_0..6   검은 쇠자루 + 크림빛 망치 머리(가운데 주황 띠) + 은 자루끝
  무기 나이트 순백의 해머-쇠망치     weapon_knight_0..4
  무기 로그   순백의 해머-작은 망치   weapon_rogue_0..4 (4 = 1×1 빈 칸) · 보조손 로그 같은 이름 offhand_rogue_0..3
나이트 방패는 없다(그림에 방패가 없다).
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_kit as kit  # noqa: E402
import costume_draft_lib as lib  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

OUT = kit.out_dir("208_hammer")

SK = kit.SKIN
SL, SM, SD = H("#ffdfbf"), H("#ff9f7f"), H("#df7f7f")
# 회색 블라우스 · 치마 (그림의 회색에 보랏빛 한 방울)
B0 = H("#5f5f7f")
B1 = H("#7f7f9f")
B2 = H("#9f9fbf")
B3 = H("#bfbfdf")
B4 = H("#dfdfff")
BELT = H("#3f3f5f")

# ── 투구: 하얀 보닛 + 백금빛 긴 머리 ──
HAIR = {"O": H("#7f7f5f"), "D": H("#bfbf9f"), "M": H("#dfdfbf"), "L": H("#ffffdf"), "H": H("#ffffff")}
CAP = {"K": H("#7f7f9f"), "W": H("#ffffff"), "w": H("#dfdfdf"), "v": H("#bfbfdf"), "R": H("#5f1f3f"), "r": H("#7f3f5f"), "Y": H("#ffdf3f"), "y": H("#bf9f1f")}
FRONT = [
    "....KKKKKKKK....",
    "..KKwWWWWWWwKK..",
    ".KwWWWWWWWWWWvK.",
    ".KWWWWWWWWWWwvK.",
    "YKwWWWWWWWWwwvvK",
    "yRRRRrRRRRRRRRRK",
    "YyvwvwvwvwvwvwvK",
    ".OLMMDMMDMMDMDO.",
    ".OLMDO89OO9ODMO.",
    ".OLMObb88bbOMDO.",
    ".OLM76caac67MDO.",
    ".OLM798aa897MDO.",
    ".OLMM798897MDDO.",
    ".OMMDO7777ODDDO.",
]
SIDE = [
    ".....KKKKKK....",
    "...KKwWWWWWKK..",
    "..KwWWWWWWWWvK.",
    ".KwWWWWWWWWwvKY",
    ".KWWWWWWWWwwvyY",
    "KRRRRRRRRRRRRRy",
    "KvwvwvwvwvwvwvY",
    ".OMDMMDMMMMMDDO",
    ".OM8OMDMMMMMDDO",
    "..78bbOMMMMMDDO",
    "..7ac6ODMMMDDDO",
    "..7aa89OMMMDDDO",
    "...7899OMMDDDO.",
    "....777ODDDDDO.",
]
BACK = [
    "....KKKKKKKK....",
    "..KKwWWWWWWwKK..",
    ".KwWWWWWWWWWWvK.",
    ".KWWWWWWWWWWwvKY",
    "KwWWWWWWWWWwwvyY",
    "KRRRRRRRRRRRRRRy",
    "KvwvwvwRRwvwvwvK",
    ".OLMLMMRrMMDMDO.",
    ".OMMLMMrRMMDMDO.",
    ".OMMMMDRrMDMDDO.",
    ".OMDMMDrRDMMDDO.",
    ".ODMMDMMDMMDDDO.",
    ".ODDMDDMDDMDDDO.",
    "..ODDODDODDDOO..",
]
heads = kit.head_from_art((BACK, SIDE, FRONT), HAIR, H("#7f3fbf"), CAP)
lib.write_part(OUT, "head", heads)

# ── 갑옷: 몸통 — 드러난 어깨 · 주름 테 · 회색 블라우스 · 짙은 허리띠 ──
TORSO = [
    ".mSSSm.",
    "fFfFfFf",
    "gBBbBBg",
    "gBbBbBg",
    ".kkkkk.",
]
TKEY = {".": None, "S": SL, "m": SM, "f": B3, "F": B4, "B": B3, "b": B2, "g": B1, "k": BELT}
pieces = kit.shoulders([B4, B3, B2, B1], B0)  # 부푼 짧은 소매
lib.write_part(OUT, "body", kit.body(TORSO, TKEY, pieces))


# ── 갑옷: 팔 — 맨팔, 어깨 쪽 한두 칸은 부푼 소매 ──
def arm_paint(k, x, y, lv, hand, d, reach):
    if not hand and (reach == 0 or d >= reach - 1):
        return [B0, B1, B2, B2, B3, B4][lv]
    return SK[lv]


lib.write_part(OUT, "arm", kit.arms(arm_paint))

# ── 신발: 주름 치마 · 맨다리 · 맨발 ──


# ★ 2026-10-09 (사용자: *"순백의 해머 주름치마가 치마처럼 안보이여 왜냐면 다리 사이 도트를 가리지 않아서 바지를 벗은것처럼 보여"*):
# 첫 판의 치마는 맨 위 세 줄뿐이라 그 아래 엉덩이 · 허벅지 사이(다리가 갈라지는 4~6 줄)가 맨살이었다. 이제 허리부터 허벅지 중간
# (PLEAT_ROWS 줄 — 다리 프레임 11~12 줄 중, 그림의 무릎 위 치마)까지 A 라인으로 덮는다: 엉덩이 줄(HIP_ROWS)은 다리 실루엣을 전부,
# 그 아래는 허리 가운데에서 한 줄마다 조금씩 넓어지는 폭 — 걸음에 벌어진 다리는 그 밑으로 비스듬히 나온다. 치마 바로 아래 한 줄의
# 다리는 한 단 그늘(치마의 그림자).
PLEAT_ROWS = 7
HIP_ROWS = 5


def pleat_rows(info):
    """{y: (x0, x1)} — 허리(맨 윗줄)부터 PLEAT_ROWS 줄."""
    top = info.top
    x0w, x1w = info.ext[top]
    cx, half = (x0w + x1w) / 2, (x1w - x0w) / 2 + 0.5
    rows = {}
    for y in range(top, min(info.h, top + PLEAT_ROWS)):
        d = y - top
        reach = half + min(d, 2) * 0.5 + max(0, d - 2) * 0.6
        x0, x1 = round(cx - reach), round(cx + reach)
        if d < HIP_ROWS and y in info.ext:
            x0, x1 = min(x0, info.ext[y][0]), max(x1, info.ext[y][1])
        rows[y] = (max(0, x0), min(info.w - 1, x1))
    return rows


def pleated_legs():
    out = []
    for k, fr in enumerate(kit.leg_base.frames):
        info = kit.LegInfo(k, fr)
        rows = pleat_rows(info)
        hem = max(rows)
        cv = Canvas(fr.w, fr.h)
        for y in range(fr.h):
            for x in range(fr.w):
                lv = info.level(x, y)
                if lv is None:
                    continue
                if y >= info.top + 4 and y == info.bottom[x]:
                    c = SK[1] if lv < 2 else SK[3]  # 맨발 바닥
                else:
                    c = kit.bare_leg(info, x, y, lv)
                if y == hem + 1 and c in SK[2:]:
                    c = SK[SK.index(c) - 1]  # 치마 밑 그늘
                cv.put(x, y, c)
        # 치마 = 허리띠 한 줄 + 주름(두 칸마다 밝게 · 어둡게, 왼쪽이 빛) + 밑단(주름 끝이 번갈아 접힌다) — 외곽은 짙게.
        for y, (x0, x1) in rows.items():
            d = y - info.top
            for x in range(x0, x1 + 1):
                u = (x - x0) / max(1, x1 - x0)
                lit = (x - x0) % 2 == 0
                if x in (x0, x1):
                    c = B0
                elif d == 0:
                    c = BELT
                elif y == hem:
                    c = B1 if lit else B0
                elif u > 0.7:
                    c = B2 if lit else B1  # 오른쪽 = 그늘
                elif d <= 2:
                    c = B4 if lit and u < 0.35 else B3 if lit else B2
                else:
                    c = B3 if lit else B2
                cv.put(x, y, c)
        out.append(cv)
    return out


lib.write_part(OUT, "leg", pleated_legs())

# ── 무기: 망치 ──
IRON = (H("#5f5f5f"), H("#3f3f3f"), H("#1f1f1f"))  # 밝은 쪽 · 몸 · 외곽
BONE = (H("#ffffdf"), H("#dfdfbf"), H("#bfbf9f"), H("#5f5f3f"))  # 빛 · 몸 · 그늘 · 외곽
CORE = (H("#ffbf3f"), H("#ff7f1f"), H("#9f3f1f"))
CAPM = (H("#dfdfdf"), H("#9f9f9f"), H("#3f3f3f"))


def hammer(part, k, size):
    """size: 'big'(버서커) · 'mid'(나이트) · 'small'(로그)."""
    w, h, gx, gy, tx, ty, ux, uy, px, py, L = kit.frame_axis(part, k)
    hl = {"big": 4.4, "mid": 2.6, "small": 1.8}[size]  # 머리의 가로 반길이(자루에 직각)
    ht = {"big": 2.7, "mid": 1.7, "small": 1.25}[size]  # 머리의 세로 반두께(자루 방향)
    hs = {"big": 1.15, "mid": 0.85, "small": 0.7}[size]
    if min(w, h) <= 3:
        shaft = kit.ribbon(w, h, [(gx - ux * 0.5, gy - uy * 0.5), (tx - ux * 2.0, ty - uy * 2.0)], [0.75, 0.75])
        head = kit.ribbon(w, h, [(tx - ux * 3.2, ty - uy * 3.2), (tx + ux * 0.3, ty + uy * 0.3)], [1.5, 1.5])
        core = kit.ribbon(w, h, [(tx - ux * 1.6, ty - uy * 1.6), (tx - ux * 1.5, ty - uy * 1.5)], [1.5, 1.5])
        cap = {}
    else:
        # 머리는 언제나 끝에(망치 길이가 자세마다 같게) — 자루에 직각인 양쪽 길이를 프레임 안까지만(한쪽이 짧아질 수 있다).
        cx, cy = kit.clamp_pts(w, h, [(tx - ux * ht * 0.9, ty - uy * ht * 0.9)], ht * 0.8)[0]

        def reach(sgn):
            r = 0.0
            while r < hl and 0.5 <= cx + px * sgn * (r + 0.25) <= w - 0.5 and 0.5 <= cy + py * sgn * (r + 0.25) <= h - 0.5:
                r += 0.25
            return r

        a, b = reach(-1), reach(1)
        # 끝이 프레임 모서리라 머리가 반도 안 들면(나이트 · 로그) 자루 쪽으로 물린다 — 망치가 조금 짧아지는 대신 머리가 온전하다.
        back = 0.0
        while min(a, b) < hl * 0.6 and back < L * 0.35:
            back += 0.5
            cx, cy = kit.clamp_pts(w, h, [(tx - ux * (ht * 0.9 + back), ty - uy * (ht * 0.9 + back))], ht * 0.8)[0]
            a, b = reach(-1), reach(1)
        shaft = kit.ribbon(w, h, kit.clamp_pts(w, h, [(gx - ux * 1.4, gy - uy * 1.4), (cx, cy)], 0.5), [hs, hs])
        head = kit.ribbon(w, h, [(cx - px * a, cy - py * a), (cx + px * b, cy + py * b)], [ht, ht])
        m = (b - a) / 2
        core = kit.ribbon(w, h, [(cx + px * (m - (a + b) * 0.22), cy + py * (m - (a + b) * 0.22)), (cx + px * (m + (a + b) * 0.22), cy + py * (m + (a + b) * 0.22))], [ht * 0.45, ht * 0.45])
        cap = kit.ribbon(w, h, kit.clamp_pts(w, h, [(gx - ux * 1.9, gy - uy * 1.9), (gx - ux * 1.6, gy - uy * 1.6)], 0.6), [hs + 0.35, hs + 0.35])

    return kit.compose(
        w,
        h,
        [
            kit.Layer(shaft, lambda t, s: IRON[0] if s < -0.3 else IRON[1], lambda t, s, lit: IRON[0] if lit else IRON[2]),
            kit.Layer(cap, lambda t, s: CAPM[0] if s < 0 else CAPM[1], lambda t, s, lit: CAPM[1] if lit else CAPM[2]),
            kit.Layer(head, lambda t, s: BONE[0] if s < -0.45 else BONE[1] if s < 0.45 else BONE[2], lambda t, s, lit: BONE[0] if lit else BONE[3]),
            kit.Layer(core, lambda t, s: CORE[0] if s < 0 else CORE[1], lambda t, s, lit: CORE[1] if lit else CORE[2]),
        ],
    )


lib.write_part(OUT, "weapon_berserker", [hammer("weapon_berserker", k, "big") for k in range(7)])
lib.write_part(OUT, "weapon_knight", [hammer("weapon_knight", k, "mid") for k in range(5)])
lib.write_part(OUT, "weapon_rogue", [hammer("weapon_rogue", k, "small") for k in range(4)] + [lib.BLANK])
lib.write_part(OUT, "offhand_rogue", [hammer("offhand_rogue", k, "small") for k in range(4)])

lib.write_text(
    OUT,
    """# 의상 208 — 「순백의 해머」 (Claude 초안 2026-10-09, tools_re/costume_draft_hammer.py) — 가디언 모드 클리어 보상 (난이도 NORMAL
# 이상으로 그 회차 진행도의 95 % 이상, avatar_state::CLEAR_REWARDS). 사용자가 준 그림 2: 하얀 보닛 · 백금빛 긴 머리 · 회색 블라우스와
# 주름 치마 · 맨발 · 거대한 망치. 이름은 Claude 가 정했다(확인 대기). 나이트 방패는 없다(그림에 없다).
set                   = 순백의 해머
head                  = 순백의 해머-보닛
head.desc             = 리본을 맨 하얀 보닛
armor                 = 순백의 해머-블라우스
armor.desc            = 어깨를 드러낸 블라우스
shoes                 = 순백의 해머-주름 치마
shoes.desc            = 회색 주름 치마와 맨발
weapon.berserker      = 순백의 해머-거대 망치
weapon.berserker.desc = 몸집보다 큰 쇠망치
weapon.knight         = 순백의 해머-쇠망치
weapon.knight.desc    = 한 손으로 쥐는 쇠망치
weapon.rogue          = 순백의 해머-작은 망치
weapon.rogue.desc     = 오른손에 쥐는 작은 망치
offhand.rogue         = 순백의 해머-작은 망치
offhand.rogue.desc    = 왼손에 쥐는 작은 망치
""",
)
