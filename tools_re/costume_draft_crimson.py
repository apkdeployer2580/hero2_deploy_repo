#!/usr/bin/env python3
"""의상 207 「진홍의 검사」 초안 생성기 (2026-10-09) — 가디언 모드(4회차) 클리어 보상, 난이도 EASY 이상.

사용자(2026-10-09): *"스킨을 새로 생성할꺼야. 첫번쨰 스크린샷은 가디언 모드 이지 클리어했을떄"* — 그림 = 붉은 긴 머리와 금관 ·
왼어깨의 해골 견갑 · 은 장식이 박힌 검은 롱코트(붉은 안감) · 검은 상의 · 검은 스타킹과 하이힐 · 길고 붉은 태도. 이름은 Claude 가
정했다(「빙해의 검사」 이안의 짝 — 확인 대기).

    python tools_re/costume_draft_crimson.py            # costumes/207_crimson/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_crimson.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_crimson.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다** (`costumes/README.md`). 그림 도구 = `costume_draft_kit.py`.

아이템:
  투구 진홍의 검사-머리      head_0..2    긴 머리 틀 + 진홍 머리칼 · 작은 금관 · 붉은 눈
  갑옷 진홍의 검사-코트      body · arm · ★ skirtunder · skirtover (층 치마, 2026-10-09 — 다리는 신발 칸의 것)
                                                  몸통 = 열린 검은 코트(붉은 안감 · 은 징) 사이로 검은 상의와 맨배 · 어깨 조각 =
                                                  검은 코트 어깨(은 징) + 해골 견갑 · 팔 = 검은 소매(손목은 붉은 안감) + 맨손 ·
                                                  치마 = 코트 자락: 앞 · 옆은 다리 뒤로 양쪽에 늘어지고(안쪽 테두리 = 붉은 안감)
                                                  뒤에서는 다리를 덮는다 — 밑단 아래로 스타킹과 힐
  신발 진홍의 검사-스타킹    leg_0..10    검은 속옷 · 맨 허벅지 한 줄 · 비치는 검은 스타킹 · 검은 하이힐
  무기 버서커 진홍의 검사-붉은 대도   weapon_berserker_0..6   길고 두꺼운 붉은 태도(노다치)
  무기 나이트 진홍의 검사-붉은 태도   weapon_knight_0..4      붉은 태도 — 금빛 코등이 · 갈색 손잡이
  무기 로그   진홍의 검사-붉은 단도   weapon_rogue_0..4 (4 = 1×1 빈 칸) · 보조손 로그 같은 이름 offhand_rogue_0..3
나이트 방패는 없다(그림에 방패가 없다) — 그 칸은 다른 세트나 장착 장비 모습이다.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_kit as kit  # noqa: E402
import costume_draft_lib as lib  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

OUT = kit.out_dir("207_crimson")

# ── 색 ──
KK = H("#000000")
C0 = H("#1f1f1f")  # 코트 그늘 · 검은 상의
C1 = H("#3f3f3f")  # 코트
C2 = H("#5f5f5f")  # 코트 밝게
C3 = H("#7f7f7f")
ST = H("#bfbfbf")  # 은 징
ST2 = H("#dfdfdf")
R0 = H("#5f001f")  # 붉은 안감 그늘
R1 = H("#9f1f1f")
R2 = H("#df3f3f")
G0 = H("#9f7f1f")  # 금
G1 = H("#dfbf3f")
G2 = H("#ffff7f")
SK = kit.SKIN  # 맨팔 피부 램프(0 외곽 .. 5 가장 밝게)
SL, SM, SD = H("#ffdfbf"), H("#ff9f7f"), H("#df7f7f")
STK = [H("#1f001f"), H("#3f1f3f"), H("#3f1f3f"), H("#5f3f5f"), H("#7f5f7f")]  # 비치는 검은 스타킹 (밝기 0..4)
HEEL = [KK, KK, C0, C1, C2]
PANTY = [KK, C0, C0, C1, C1]

# ── 투구: 진홍의 긴 머리 + 작은 금관 ──
HAIR = {"O": H("#5f001f"), "D": H("#9f001f"), "M": H("#df1f1f"), "L": H("#ff3f3f"), "H": H("#ff7f7f")}
CROWN = {"g": G0, "G": G1, "Y": G2}
back = list(kit.LONG_BACK)
back[0] = ".....Y.YY.Y....."
back[1] = ".....gGGGGg....."
side = list(kit.LONG_SIDE)
side[0] = "....Y.Y.Y......"
side[1] = "....gGYGgO....."
front = list(kit.LONG_FRONT)
front[0] = ".....Y.YY.Y....."
front[1] = ".....gGYYGg....."
heads = kit.head_from_art((back, side, front), HAIR, H("#bf3f1f"), CROWN)
lib.write_part(OUT, "head", heads)

# ── 갑옷: 몸통 — 열린 검은 코트 · 붉은 안감 · 검은 상의 · 맨배 ──
TORSO = [
    ".cCSCK.",
    "CRbSbRK",
    "wRSSSRw",
    "CRStSRK",
    ".KbbbK.",
]
TKEY = {".": None, "K": C0, "C": C1, "c": C2, "R": R1, "b": C0, "S": SL, "t": SD, "w": ST}
pieces = kit.shoulders([C2, C1, C0, C0], KK)
pieces[0].put(4, 2, ST)  # 어깨의 은 징 하나
for (x, y), c in {(2, 1): ST2, (3, 1): ST2, (4, 1): ST, (1, 2): ST, (2, 2): KK, (3, 2): ST, (4, 2): KK, (2, 3): C3, (3, 3): ST, (4, 3): C3}.items():
    pieces[1].put(x, y, c)  # 해골 견갑 — 흰 두개골 · 검은 눈구멍 · 회색 턱
lib.write_part(OUT, "body", kit.body(TORSO, TKEY, pieces))


# ── 갑옷: 팔 — 검은 소매 · 손목 = 붉은 안감 · 어깨 끝 = 은 징 · 맨손 ──
def arm_paint(k, x, y, lv, hand, d, reach):
    if hand:
        return SK[lv]
    if reach and d == 1:
        return R1 if lv < 4 else R2  # 소맷부리의 붉은 안감
    if reach and d >= reach and lv >= 3:
        return ST  # 어깨 끝 은 징
    return [KK, C0, C1, C1, C2, C3][lv]


lib.write_part(OUT, "arm", kit.arms(arm_paint))


# ── 신발: 검은 속옷 · 맨 허벅지 · 스타킹 · 하이힐 ──
def leg_paint(info, x, y, lv):
    if y == info.top:
        return PANTY[lv]
    if y == info.top + 1:
        return [SK[0], SK[2], SK[3], SK[4], SK[5]][lv]  # 스타킹 위 맨 허벅지 한 줄
    if y >= info.top + 4 and y >= info.bottom[x] - 1:
        return HEEL[lv]
    return STK[lv]


legs = kit.legs(leg_paint)
lib.write_part(OUT, "leg", legs)


# ── 갑옷: 치마 = 코트 자락 ──
# ★ 2026-10-09 층 치마(costume_sheets SKIRT_LAYERS — 사용자: 앞이나 뒤를 덮지 않는 드레스는 신발 아바타를 바꾸면 보여야 한다): 다리를
# 그리지 않는다. 앞 · 옆 = 다리 뒤로 늘어진 자락(아랫층 — 코트가 열린 자리 = 붉은 안감, 이 세트 다리의 옆 칸 · 다리 바로 뒤는 안감 그늘),
# 뒤 = 다리를 덮는 코트(윗층 — 가운데 트임 · 은 징 둘), 밑단 아래로 신발 칸의 다리. 빌드가 아랫층 + 다리 + 윗층을 한 장으로 합친다 —
# 이 세트의 다리와 합친 것이 첫 판의 한 장짜리 자락과 같은 그림이다.
def coat_skirt_layers():
    unders, overs = [], []
    for k, fr in enumerate(kit.leg_base.frames):
        info = kit.LegInfo(k, fr)
        leg = legs[k]
        rows = kit.aline(info, hem=2, waist_half=2.5, flare=0.7)
        hem = max(rows)
        under, over = Canvas(fr.w, fr.h), Canvas(fr.w, fr.h)
        layer = over if info.back else under
        for y, (x0, x1) in rows.items():
            for x in range(x0, x1 + 1):
                if x in (x0, x1) or y == hem:
                    c = KK
                elif not info.back and leg.get(x, y) is not None:
                    c = R0  # 다리 바로 뒤 = 안감 그늘 (이 세트의 다리가 가린다)
                elif not info.back and (leg.get(x - 1, y) is not None or leg.get(x + 1, y) is not None):
                    c = R1 if y < hem - 1 else R0  # 코트가 열린 자리 = 붉은 안감
                else:
                    u = (x - x0) / max(1, x1 - x0)
                    c = C2 if u < 0.15 else C1 if u < 0.45 else C0
                layer.put(x, y, c)
        # 뒤에서 본 코트: 가운데 트임 한 줄(밑단 쪽 절반)과 은 징 두 점
        if info.back:
            mid = (rows[info.top][0] + rows[info.top][1]) // 2
            for y in range(info.top + (hem - info.top) // 2, hem):
                if over.get(mid, y) is not None:
                    over.put(mid, y, C0)
            for y in (info.top + 1,):
                for x in (rows[y][0] + 1, rows[y][1] - 1):
                    if over.get(x, y) is not None:
                        over.put(x, y, ST)
        unders.append(under)
        overs.append(over)
    return unders, overs


kit.write_skirt_layers(OUT, *coat_skirt_layers())

# ── 무기: 붉은 태도 — 날(볼록한 쪽) = 밝은 줄 · 몸 · 등 = 어두운 외곽 (배포본 칼의 짜임), 금빛 코등이 · 감은 손잡이 ──
EDGE_HI = H("#ffbfbf")
BL_EDGE, BL_BODY, BL_SPINE, BL_OUT = H("#ff3f3f"), H("#df1f1f"), H("#9f1f1f"), H("#5f001f")
WRAP = [H("#bf7f3f"), H("#7f3f1f")]
GRIP_OUT = H("#3f1f1f")


def katana(part, k, size):
    """size: 'big'(버서커 노다치) · 'mid'(나이트) · 'small'(로그 단도)."""
    w, h, gx, gy, tx, ty, ux, uy, px, py, L = kit.frame_axis(part, k)
    hw0 = {"big": 1.75, "mid": 1.3, "small": 1.1}[size]
    hwg = {"big": 1.05, "mid": 0.9, "small": 0.8}[size]
    if min(w, h) <= 3:
        # 칼날을 옆에서 본 프레임(3 px 폭): 곧은 칼날 + 손잡이 쪽 코등이
        sgn = 1
        blade = kit.ribbon(w, h, [(gx + ux * 2.2, gy + uy * 2.2), (tx + ux * 0.4, ty + uy * 0.4)], [1.45, 0.9])
        grip = kit.ribbon(w, h, [(gx - ux * 0.6, gy - uy * 0.6), (gx + ux * 0.9, gy + uy * 0.9)], [1.0, 1.0])
        guard = kit.ribbon(w, h, [(gx + ux * 1.6, gy + uy * 1.6), (gx + ux * 1.7, gy + uy * 1.7)], [1.5, 1.5])
    else:
        sgn = kit.room_side(w, h, gx + ux * L * 0.6, gy + uy * L * 0.6, px, py, 3)
        bend = {"big": min(1.5, L / 13), "mid": min(1.0, L / 12), "small": 0.35}[size]
        pts = kit.clamp_pts(w, h, kit.curve(gx, gy, ux, uy, px * sgn, py * sgn, 2.2, L + 0.3, bend), 0.55)
        blade = kit.ribbon(w, h, pts, [hw0 - (hw0 - 0.45) * (i / 10) ** 1.4 for i in range(11)])
        grip = kit.ribbon(w, h, kit.clamp_pts(w, h, [(gx - ux * 1.6, gy - uy * 1.6), (gx + ux * 0.9, gy + uy * 0.9)], 0.6), [hwg, hwg])
        bar = 1.6 if size != "small" else 1.3
        guard = kit.ribbon(w, h, kit.clamp_pts(w, h, [(gx + ux * 1.6 + px * bar, gy + uy * 1.6 + py * bar), (gx + ux * 1.6 - px * bar, gy + uy * 1.6 - py * bar)], 0.6), [0.7, 0.7])

    def blade_fill(t, side):
        e = side * sgn
        if t > 0.94:
            return EDGE_HI
        return BL_EDGE if e > 0.35 else BL_BODY if e > -0.35 else BL_SPINE

    def blade_edge(t, side, lit):
        return EDGE_HI if side * sgn > 0 or t > 0.94 else BL_OUT

    return kit.compose(
        w,
        h,
        [
            kit.Layer(grip, lambda t, side: WRAP[int(t * 5) % 2], lambda t, side, lit: GRIP_OUT),
            kit.Layer(blade, blade_fill, blade_edge),
            kit.Layer(guard, lambda t, side: G1, lambda t, side, lit: G2 if lit else G0),
        ],
    )


lib.write_part(OUT, "weapon_berserker", [katana("weapon_berserker", k, "big") for k in range(7)])
lib.write_part(OUT, "weapon_knight", [katana("weapon_knight", k, "mid") for k in range(5)])
lib.write_part(OUT, "weapon_rogue", [katana("weapon_rogue", k, "small") for k in range(4)] + [lib.BLANK])
lib.write_part(OUT, "offhand_rogue", [katana("offhand_rogue", k, "small") for k in range(4)])

lib.write_text(
    OUT,
    """# 의상 207 — 「진홍의 검사」 (Claude 초안 2026-10-09, tools_re/costume_draft_crimson.py) — 가디언 모드 클리어 보상 (난이도 EASY 이상,
# avatar_state::CLEAR_REWARDS). 사용자가 준 그림 1: 붉은 긴 머리와 금관 · 해골 견갑 · 붉은 안감의 검은 롱코트 · 검은 스타킹 · 붉은 태도.
# 이름은 Claude 가 정했다(확인 대기). 나이트 방패는 없다(그림에 없다).
set                   = 진홍의 검사
head                  = 진홍의 검사-머리
head.desc             = 금관을 쓴 붉은 긴 머리
armor                 = 진홍의 검사-코트
armor.desc            = 붉은 안감의 검은 롱코트
shoes                 = 진홍의 검사-스타킹
shoes.desc            = 검은 스타킹과 하이힐
weapon.berserker      = 진홍의 검사-붉은 대도
weapon.berserker.desc = 핏빛으로 물든 큰 태도
weapon.knight         = 진홍의 검사-붉은 태도
weapon.knight.desc    = 핏빛으로 물든 긴 태도
weapon.rogue          = 진홍의 검사-붉은 단도
weapon.rogue.desc     = 오른손에 쥐는 붉은 단도
offhand.rogue         = 진홍의 검사-붉은 단도
offhand.rogue.desc    = 왼손에 쥐는 붉은 단도
""",
)
