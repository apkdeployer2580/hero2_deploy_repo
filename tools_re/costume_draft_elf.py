#!/usr/bin/env python3
"""의상 209 「달빛 엘프」 초안 생성기 (2026-10-09) — 가디언 모드(4회차) 클리어 보상, 난이도 HARD 이상(진행도 95 %).

사용자(2026-10-09): *"세번째는 가디언 하드 클리어했을떄야"* — 그림 = 바닥까지 오는 금빛 물결 머리 · 뾰족한 엘프 귀 · 이마의 청록
보석관 · 푸른 눈 · 어깨를 드러낸 별 무늬 청록 망토 · 하얀 드레스(금 장식 · 보랏빛 보석 · 밑단의 청록 별) · 청록 구두 · 남색 자루
끝에 금빛 햇살과 보랏빛 구슬을 품은 초승달 지팡이. 이름은 Claude 가 정했다(확인 대기).

    python tools_re/costume_draft_elf.py            # costumes/209_elf/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_elf.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_elf.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다** (`costumes/README.md`). 그림 도구 = `costume_draft_kit.py`.

아이템:
  투구 달빛 엘프-머리       head_0..2    금빛 물결 머리(긴 머리 틀) · 양옆으로 솟은 엘프 귀 · 청록 보석과 금 받침 · 푸른 눈
  갑옷 달빛 엘프-드레스     body · arm · ★ skirt   몸통 = 드러난 어깨 · 하얀 몸판(보랏빛 보석 · 금 허리) · 양옆 청록 망토 ·
                                                 어깨 조각 = 청록 망토(금 별) · 팔 = 맨팔(어깨 쪽은 망토 · 손목 금 팔찌) ·
                                                 치마 = 바닥까지 오는 하얀 드레스, 양옆은 청록 망토 자락 · 밑단 금 테 · 청록 별 —
                                                 뒤에서는 망토가 덮는다(밑단만 하얀 드레스)
  신발 달빛 엘프-구두       leg_0..10    하얀 스타킹 · 청록 구두 (드레스를 입으면 치마가 덮는다)
  무기 버서커 달빛 엘프-달의 대지팡이  weapon_berserker_0..6   남색 자루 + 초승달 머리(보랏빛 구슬 · 금빛 햇살)
  무기 나이트 달빛 엘프-초승달 지팡이  weapon_knight_0..4
  무기 로그   달빛 엘프-달의 완드      weapon_rogue_0..4 (4 = 1×1 빈 칸) · 보조손 로그 달빛 엘프-별의 완드 = 금빛 별
나이트 방패는 없다(그림에 방패가 없다).
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_kit as kit  # noqa: E402
import costume_draft_lib as lib  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

OUT = kit.out_dir("209_elf")

SK = kit.SKIN
SL, SM = H("#ffdfbf"), H("#ff9f7f")
W, Wl, Wm, Wd, WO = H("#ffffff"), H("#dfdfff"), H("#dfdfdf"), H("#bfbfdf"), H("#7f7f9f")  # 하얀 드레스 (흰색 · 그늘 · 외곽)
T0, T1, T2, T3, T4 = H("#1f3f5f"), H("#1f5f7f"), H("#3f7f9f"), H("#5f9f9f"), H("#7fbfbf")  # 청록 망토 (외곽 → 빛)
G0, G1, G2 = H("#9f7f1f"), H("#dfbf3f"), H("#ffff7f")
GEM = H("#7f3fbf")
TEAL_GEM, TEAL_GEM2 = H("#3fdfdf"), H("#1f9f9f")

# ── 투구: 금빛 물결 머리 · 엘프 귀 · 청록 보석관 ──
HAIR = {"O": H("#9f7f3f"), "D": H("#bf9f5f"), "M": H("#dfbf7f"), "L": H("#ffdf9f"), "H": H("#ffffbf")}
EXTRA = {"g": G0, "G": G1, "T": TEAL_GEM, "t": TEAL_GEM2, "k": H("#9f3f3f"), "f": H("#ff9f7f"), "j": H("#ffdfbf")}  # k f j = 엘프 귀(얼굴 글자는 맨머리 자리 전용)
FRONT = [
    "................",
    ".....OOOOOO.....",
    "...OOLHHLLMOO...",
    "..OLHHLLLLMMDO..",
    "..OLHLLLLMMMDO..",
    ".OLLLLLMMMMMDDO.",
    "kOLMLLGTTGMDMDOk",
    "fjMMDMgttgMDMDjf",
    ".kLMDO89OO9ODMk.",
    ".OMLObb88bbOLDO.",
    ".OLM76caac67MDO.",
    ".OML798aa897DMO.",
    ".OLMM798897MMDO.",
    ".OMLDO7777OMDLO.",
]
SIDE = [
    "...............",
    ".....OOOOO.....",
    "...OOLHHLMOO...",
    "..OLHHLLLLMMO..",
    "..OLHLLLLMMMDO.",
    ".OLLLLLMMMkMDO.",
    ".OGTGMLMMfMMDDO",
    ".OMDMMDMfjMMDDO",
    ".OM8OMDfjjMMDDO",
    "..78bbOfLMMMDDO",
    "..7ac6ODMLMDDDO",
    "..7aa89OMMLDDDO",
    "...7899OLMDMDO.",
    "....777ODLDMDO.",
]
BACK = [
    "................",
    ".....OOOOOO.....",
    "...OOLLHHLLOO...",
    "..OLLHHHLLLMMO..",
    "..OLHHLLLLMMDO..",
    ".OLLLLLLMMMMDDO.",
    "kOLMLLMMMMMMDDOk",
    "fjLMLMMMMMMDMDjf",
    ".kMMLMMDMMMDMDk.",
    ".OMLMMDMLMDMDDO.",
    ".OMDLMDMMDLMDDO.",
    ".ODMMDLMDMMDLDO.",
    ".ODLMDDMLDMDDDO.",
    "..ODDODDODDDOO..",
]
heads = kit.head_from_art((BACK, SIDE, FRONT), HAIR, H("#3f7fdf"), EXTRA)
lib.write_part(OUT, "head", heads)

# ── 갑옷: 몸통 — 드러난 어깨 · 하얀 몸판 · 보랏빛 보석 · 금 허리 · 양옆 청록 망토 ──
TORSO = [
    "tmSSSmT",
    "tWWEWWT",
    "tWwWwWT",
    "tWWwWWT",
    ".GgGgG.",
]
TKEY = {".": None, "t": T2, "T": T1, "S": SL, "m": SM, "W": W, "w": Wd, "E": GEM, "G": G1, "g": G0}
pieces = kit.shoulders([T4, T3, T2, T1], T0)
pieces[0].put(4, 2, G2)  # 망토의 금 별
pieces[1].put(3, 2, G2)
lib.write_part(OUT, "body", kit.body(TORSO, TKEY, pieces))


# ── 갑옷: 팔 — 맨팔 · 어깨 쪽 = 청록 망토 · 손목 = 금 팔찌 ──
def arm_paint(k, x, y, lv, hand, d, reach):
    if hand:
        return SK[lv]
    if reach == 0 or d >= reach - 1:
        return [T0, T1, T2, T2, T3, T4][lv]
    if d == 1:
        return G1 if lv >= 3 else G0
    return SK[lv]


lib.write_part(OUT, "arm", kit.arms(arm_paint))

# ── 신발: 하얀 스타킹 · 청록 구두 ──
STOCKING = [WO, Wd, Wd, Wm, W]


def leg_paint(info, x, y, lv):
    if y >= info.top + 4 and y >= info.bottom[x] - 1:
        return [T0, T0, T1, T2, T3][lv]
    return STOCKING[lv]


legs = kit.legs(leg_paint)
lib.write_part(OUT, "leg", legs)


# ── 갑옷: 치마 — 하얀 드레스 + 양옆 청록 망토 자락 (뒤 = 망토) ──
def gown_paint(info, x, y, u, v, edge, last):
    side = u < 0.24 or u > 0.76
    if info.back:
        if last:
            return Wd if x % 2 else Wm  # 망토 밑으로 보이는 드레스 밑단
        if edge:
            return T0
        star = (x * 3 + y * 5) % 11 == 0 and 0.15 < v < 0.9
        return G2 if star else (T3 if u < 0.3 else T2 if u < 0.7 else T1)
    if edge:
        return T0 if side else WO
    if side:
        if last:
            return T1
        star = (x * 3 + y * 5) % 9 == 0 and v > 0.2
        return G2 if star else (T3 if u < 0.24 else T1)
    if last:
        return G1 if x % 2 else G0  # 하얀 치마의 금 밑단
    if v > 0.55 and (x * 7 + y * 3) % 6 == 0:
        return T2  # 밑단 쪽 청록 별
    return W if u < 0.5 else Wl if u < 0.65 else Wd


skirt = [cv for _info, _rows, cv in kit.skirts(gown_paint, waist_half=2.5, flare=1.0)]
lib.write_part(OUT, "skirt", skirt)

# ── 무기: 초승달 지팡이 · 별의 완드 ──
SHAFT = (H("#5f7fdf"), H("#3f5fbf"), H("#1f1f5f"))  # 빛 · 몸 · 외곽
MOON = (H("#ffffff"), H("#dfdfff"), H("#bfbfdf"), H("#5f5f9f"))  # 빛 · 몸 · 그늘 · 외곽
ORB = (H("#bf9fff"), H("#5f3f9f"), H("#3f1f5f"), H("#1f1f3f"))
RAY = (H("#ffff7f"), H("#ffdf3f"), H("#9f7f1f"))


def disc(w, h, cx, cy, r):
    return {(x, y): (0.0, 0.0) for y in range(h) for x in range(w) if math.hypot(x + 0.5 - cx, y + 0.5 - cy) <= r}


def moon_head(w, h, cx, cy, R):
    """초승달(바깥 원 − 위 오른쪽으로 비킨 안쪽 원) · 그 품의 구슬 · 바깥쪽 금빛 햇살 셋. → 층 넷."""
    outer = disc(w, h, cx, cy, R)
    cut = disc(w, h, cx + R * 0.42, cy - R * 0.42, R * 0.78)
    crescent = {p: (0.0, 0.0) for p in outer if p not in cut}
    orb = disc(w, h, cx + R * 0.32, cy - R * 0.32, max(0.9, R * 0.45))
    rays = {}
    for ang in (150, 200, 250):
        a = math.radians(ang)
        for t in (R + 0.6, R + 1.4):
            x, y = cx + math.cos(a) * t, cy + math.sin(a) * t
            if 0 <= x < w and 0 <= y < h:
                rays[(int(x), int(y))] = (0.0, 0.0)
    return crescent, orb, rays


def staff(part, k, size):
    w, h, gx, gy, tx, ty, ux, uy, px, py, L = kit.frame_axis(part, k)
    R = {"big": 3.6, "mid": 2.6, "small": 2.0}[size]
    if min(w, h) <= 3:
        shaft = kit.ribbon(w, h, [(gx - ux * 0.5, gy - uy * 0.5), (tx - ux * 2.5, ty - uy * 2.5)], [0.75, 0.75])
        crescent = kit.ribbon(w, h, [(tx - ux * 3.0, ty - uy * 3.0), (tx + ux * 0.3, ty + uy * 0.3)], [1.5, 1.5])
        orb = kit.ribbon(w, h, [(tx - ux * 1.6, ty - uy * 1.6), (tx - ux * 1.5, ty - uy * 1.5)], [1.0, 1.0])
        rays = {}
    else:
        cx, cy = kit.clamp_pts(w, h, [(tx - ux * R * 0.8, ty - uy * R * 0.8)], R + 0.2)[0]
        shaft = kit.ribbon(w, h, kit.clamp_pts(w, h, [(gx - ux * 1.5, gy - uy * 1.5), (cx, cy)], 0.5), [0.8, 0.8])
        crescent, orb, rays = moon_head(w, h, cx, cy, R)
    return kit.compose(
        w,
        h,
        [
            kit.Layer(shaft, lambda t, s: SHAFT[0] if s < -0.3 else SHAFT[1], lambda t, s, lit: SHAFT[0] if lit else SHAFT[2]),
            kit.Layer(rays, lambda t, s: RAY[1], lambda t, s, lit: RAY[0] if lit else RAY[1]),
            kit.Layer(crescent, lambda t, s: MOON[1], lambda t, s, lit: MOON[0] if lit else MOON[3]),
            kit.Layer(orb, lambda t, s: ORB[1], lambda t, s, lit: ORB[0] if lit else ORB[2]),
        ],
    )


STAR = [
    "..Y..",
    ".YyY.",
    "YyWyY",
    ".YyY.",
    "Y...Y",
]


def star_wand(part, k):
    """별의 완드(로그 왼손): 짧은 남색 자루 끝에 금빛 별 5×5."""
    w, h, gx, gy, tx, ty, ux, uy, px, py, L = kit.frame_axis(part, k)
    cx, cy = kit.clamp_pts(w, h, [(tx - ux * 2.2, ty - uy * 2.2)], 2.5)[0]
    cv = kit.compose(w, h, [kit.Layer(kit.ribbon(w, h, kit.clamp_pts(w, h, [(gx - ux, gy - uy), (cx, cy)], 0.5), [0.75, 0.75]), lambda t, s: SHAFT[1], lambda t, s, lit: SHAFT[0] if lit else SHAFT[2])])
    cv.stamp(Canvas.from_art(STAR, {"Y": RAY[1], "y": RAY[0], "W": W}), int(round(cx - 2.5)), int(round(cy - 2.5)))
    return cv


lib.write_part(OUT, "weapon_berserker", [staff("weapon_berserker", k, "big") for k in range(7)])
lib.write_part(OUT, "weapon_knight", [staff("weapon_knight", k, "mid") for k in range(5)])
lib.write_part(OUT, "weapon_rogue", [staff("weapon_rogue", k, "small") for k in range(4)] + [lib.BLANK])
lib.write_part(OUT, "offhand_rogue", [star_wand("offhand_rogue", k) for k in range(4)])

lib.write_text(
    OUT,
    """# 의상 209 — 「달빛 엘프」 (Claude 초안 2026-10-09, tools_re/costume_draft_elf.py) — 가디언 모드 클리어 보상 (난이도 HARD 이상으로
# 그 회차 진행도의 95 % 이상, avatar_state::CLEAR_REWARDS). 사용자가 준 그림 3: 금빛 물결 머리와 엘프 귀 · 청록 보석관 · 별 무늬 청록
# 망토와 하얀 드레스 · 초승달 지팡이. 이름은 Claude 가 정했다(확인 대기). 나이트 방패는 없다(그림에 없다).
set                   = 달빛 엘프
head                  = 달빛 엘프-머리
head.desc             = 보석관을 쓴 엘프 금발
armor                 = 달빛 엘프-드레스
armor.desc            = 별을 수놓은 망토 드레스
shoes                 = 달빛 엘프-구두
shoes.desc            = 하얀 스타킹과 청록 구두
weapon.berserker      = 달빛 엘프-달의 대지팡이
weapon.berserker.desc = 초승달을 얹은 큰 지팡이
weapon.knight         = 달빛 엘프-초승달 지팡이
weapon.knight.desc    = 보랏빛 구슬의 지팡이
weapon.rogue          = 달빛 엘프-달의 완드
weapon.rogue.desc     = 오른손에 쥐는 달 완드
offhand.rogue         = 달빛 엘프-별의 완드
offhand.rogue.desc    = 왼손에 쥐는 별 완드
""",
)
