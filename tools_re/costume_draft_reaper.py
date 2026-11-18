#!/usr/bin/env python3
"""의상 210 「백은의 사신」 초안 생성기 (2026-10-09) — HELL 모드 클리어 보상, 난이도 HARD 이상(진행도 95 %).

사용자(2026-10-09): *"추가로 4번쨰는 헬모드 하드 클리어했을떄야"* — 그림 = 은빛 긴 머리 · 노란 꽃술의 푸른 꽃 장식 · 붉은 눈 ·
청록 보석 목걸이 · 금장식과 초록 보석의 하얀 상의 · 맨배 · 금 사슬 허리띠와 푸른 띠 · 푸른 소매(금 테) · 앞이 트인 회색 긴 치마
(푸른 단) · 맨발 · 검은 기운이 서린 거대한 검은 낫. 이름은 Claude 가 정했다(확인 대기).

    python tools_re/costume_draft_reaper.py            # costumes/210_reaper/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_reaper.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_reaper.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다** (`costumes/README.md`). 그림 도구 = `costume_draft_kit.py`.

아이템:
  투구 백은의 사신-머리     head_0..2    은빛 긴 머리(긴 머리 틀) · 한쪽 귀 위의 푸른 꽃 · 붉은 눈
  갑옷 백은의 사신-드레스   body · arm · ★ skirtunder · skirtover (층 치마, 2026-10-09)
                                                 몸통 = 드러난 어깨 · 청록 목걸이 · 하얀 상의(금 테 · 초록 보석) · 맨배 ·
                                                 금 사슬 허리띠(푸른 띠) · 어깨 조각 없음(맨어깨) · 팔 = 아래팔은 푸른 소매(금 테 둘),
                                                 위팔은 맨살 · 치마 = 회색 긴 치마 — 앞 · 옆은 다리 뒤 양쪽으로(트인 안쪽 = 푸른 단),
                                                 뒤에서는 다리를 덮는다(밑단 푸른 단) · 앞의 푸른 띠 한 줄. 다리는 신발 칸의 것
  신발 백은의 사신-하얀 스타킹  leg_0..10  ★ 2026-10-09 (금 발찌에서 바꿈) 은빛 속옷 · 맨 허벅지 한 줄 · 레이스 단 · 발끝까지 하얀 스타킹
  무기 버서커 백은의 사신-거대한 낫   weapon_berserker_0..6   검은 자루 + 끝에서 직각으로 뻗어 휘어 내려오는 검은 날(안쪽 날 = 회색 빛, 붉은 기운)
  무기 나이트 백은의 사신-검은 낫     weapon_knight_0..4
  무기 로그   백은의 사신-작은 낫     weapon_rogue_0..4 (4 = 1×1 빈 칸) · 보조손 로그 같은 이름 offhand_rogue_0..3 (짧은 자루의 낫)
나이트 방패는 없다(그림에 방패가 없다).
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_kit as kit  # noqa: E402
import costume_draft_lib as lib  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

OUT = kit.out_dir("210_reaper")

SK = kit.SKIN
SL, SM, SD = H("#ffdfbf"), H("#ff9f7f"), H("#df7f7f")
W = H("#ffffff")
G0, G1, G2 = H("#9f7f1f"), H("#dfbf3f"), H("#ffff7f")
GREEN = H("#3fdf9f")
BL = [H("#1f1f5f"), H("#1f3f9f"), H("#3f5fbf"), H("#3f5fbf"), H("#3f7fdf"), H("#7fbfff")]  # 푸른 천 (밝기 0..5)
GR0, GR1, GR2, GR3, GR4 = H("#3f3f5f"), H("#7f7f9f"), H("#9f9fbf"), H("#bfbfdf"), H("#dfdfff")  # 회색 치마 (외곽 → 빛)

# ── 투구: 은빛 긴 머리 + 푸른 꽃 ──
HAIR = {"O": H("#5f5f7f"), "D": H("#9f9fbf"), "M": H("#bfbfdf"), "L": H("#dfdfff"), "H": H("#ffffff")}
FLOWER = {"B": H("#3f5fdf"), "n": H("#1f3f9f"), "P": H("#7fbfff"), "Y": H("#dfdf3f")}
front = list(kit.LONG_FRONT)
front[2] = "..nBOLHHLLMOO..."
front[3] = ".nPYBHLLLLMMDO.."
front[4] = ".BYPnLLLLMMMDO.."
front[5] = ".OBnLLLMMMMMDDO."
side = list(kit.LONG_SIDE)
side[2] = "...OOLHHLnBO..."
side[3] = "..OLHHLLnPYBO.."
side[4] = "..OLHLLLBYPnDO."
side[5] = ".OLLLLLMMBnMDO."
back = list(kit.LONG_BACK)
back[2] = "...OOLLHHLLnBO.."
back[3] = "..OLLHHHLLnPYBO."
back[4] = "..OLHHLLLLBYPnO."
back[5] = ".OLLLLLLMMMBnDO."
heads = kit.head_from_art((back, side, front), HAIR, H("#bf1f3f"), FLOWER)
lib.write_part(OUT, "head", heads)

# ── 갑옷: 몸통 — 청록 목걸이 · 하얀 상의(금 테 · 초록 보석) · 맨배 · 금 사슬 허리띠와 푸른 띠 ──
TORSO = [
    ".mSeSm.",
    "GWWeWWG",
    "mSSSSSm",
    "mSStSSm",
    "gGBGgGg",
]
TKEY = {".": None, "S": SL, "m": SM, "t": SD, "e": GREEN, "W": W, "G": G1, "g": G0, "B": BL[4]}
lib.write_part(OUT, "body", kit.body(TORSO, TKEY))


# ── 갑옷: 팔 — 아래팔 = 푸른 소매(손목 · 위끝 금 테), 위팔 = 맨살 ──
def arm_paint(k, x, y, lv, hand, d, reach):
    if hand or reach == 0:
        return SK[lv]
    cut = max(2, round(reach * 0.55))
    if d == 1 or d == cut:
        return G1 if lv >= 3 else G0
    if d < cut:
        return BL[lv]
    return SK[lv]


lib.write_part(OUT, "arm", kit.arms(arm_paint))


# ── 신발: 하얀 스타킹 ──
# ★ 2026-10-09 (사용자: *"금 발찌 아바타도 하얀 스타킹으로 바꾸는게 좋을것같아"*): 맨다리 · 맨발 · 금 발찌 → 은빛 속옷 한 줄 ·
# 맨 허벅지 한 줄 · 레이스 단 한 줄 · 발끝까지 하얀 스타킹(발바닥 = 한 단 그늘). 드레스를 입으면 허리의 금 사슬이 속옷 줄을 덮는다.
WS = [H("#7f7f9f"), H("#9f9fbf"), H("#bfbfdf"), H("#dfdfff"), W]  # 하얀 스타킹 (밝기 0 외곽 .. 4)
UNDIE = [H("#5f5f7f"), H("#9f9fbf"), H("#9f9fbf"), H("#bfbfdf"), H("#dfdfff")]


def leg_paint(info, x, y, lv):
    if y == info.top:
        return UNDIE[lv]
    if y == info.top + 1:
        return [SK[0], SK[2], SK[3], SK[4], SK[5]][lv]  # 스타킹 위 맨 허벅지 한 줄
    if y == info.top + 2:
        return WS[0] if lv == 0 else (W if x % 2 else WS[3])  # 레이스 단
    if y >= info.top + 4 and y == info.bottom[x]:
        return WS[max(0, lv - 1)]  # 발바닥
    return WS[lv]


legs = kit.legs(leg_paint)
lib.write_part(OUT, "leg", legs)


# ── 갑옷: 치마 — 회색 긴 치마, 앞 · 옆은 다리 뒤 양쪽 · 뒤에서는 덮는다 ──
# ★ 2026-10-09 층 치마(costume_sheets SKIRT_LAYERS — 사용자: 앞이나 뒤를 덮지 않는 드레스는 신발 아바타를 바꾸면 보여야 한다): 다리를
# 그리지 않는다. 아랫층(앞 · 옆) = 다리 뒤의 치마(트인 안쪽 = 푸른 단 — 이 세트 다리의 옆 칸) · 윗층 = 허리의 금 사슬 · 앞의 푸른 띠 ·
# 뒤에서 본 치마(다리를 덮는다). 다리는 신발 칸이 정하고 빌드가 아랫층 + 다리 + 윗층을 한 장으로 합친다 — 이 세트의 다리와
# 합친 것이 첫 판의 한 장짜리 치마와 같은 그림이다.
def reaper_skirt_layers():
    unders, overs = [], []
    for k, fr in enumerate(kit.leg_base.frames):
        info = kit.LegInfo(k, fr)
        leg = legs[k]
        rows = kit.aline(info, hem=0, waist_half=2.5, flare=1.0)
        last = max(rows)
        under, over = Canvas(fr.w, fr.h), Canvas(fr.w, fr.h)

        def cloth(x, y):
            x0, x1 = rows[y]
            if x in (x0, x1):
                return GR0
            if y == last:
                return BL[4] if x % 2 else BL[2]  # 푸른 단
            if not info.back and leg.get(x, y) is None and (leg.get(x - 1, y) is not None or leg.get(x + 1, y) is not None):
                return BL[4]  # 트인 안쪽 = 푸른 단
            u = (x - x0) / max(1, x1 - x0)
            return GR4 if u < 0.15 else GR3 if u < 0.4 else GR2 if u < 0.75 else GR1

        for y, (x0, x1) in rows.items():
            for x in range(x0, x1 + 1):
                if y == info.top:
                    over.put(x, y, G1 if x % 2 else G0)  # 허리의 금 사슬 — 다리 위
                elif info.back:
                    over.put(x, y, cloth(x, y))  # 뒤에서는 다리를 덮는다
                else:
                    under.put(x, y, cloth(x, y))  # 앞 · 옆은 다리 뒤
        if not info.back:
            # 앞에 늘어진 푸른 띠 한 줄 — 허리 가운데 조금 왼쪽에서 무릎께까지(치마 위 · 다리 위)
            mid = (info.ext[info.top][0] + info.ext[info.top][1]) // 2 - 1
            for y in range(info.top + 1, info.top + max(3, (last - info.top) * 2 // 3)):
                if (y in rows and rows[y][0] <= mid <= rows[y][1]) or leg.get(mid, y) is not None:
                    over.put(mid, y, BL[2] if y % 2 else BL[4])
        unders.append(under)
        overs.append(over)
    return unders, overs


kit.write_skirt_layers(OUT, *reaper_skirt_layers())

# ── 무기: 검은 낫 ──
SH = (H("#9f9f9f"), H("#5f5f5f"), H("#1f1f1f"))  # 자루: 빛 · 몸 · 외곽 (새까만 낫은 어두운 맵에서 사라진다 — 회색으로)
BLADE_GLINT, BLADE_HI, BLADE_BODY, BLADE_DARK, BLADE_RED, BLADE_OUT = H("#ffffff"), H("#9f9f9f"), H("#5f5f5f"), H("#3f3f3f"), H("#7f1f3f"), H("#1f1f1f")


def arc_pts(ax, ay, ux, uy, qx, qy, R, theta=1.9, n=10):
    """자루 끝 (ax, ay) 에서 q 쪽으로 직각으로 나가 −u(손잡이 쪽)로 휘어 내려오는 사분원 남짓."""
    cx, cy = ax - ux * R, ay - uy * R
    return [(cx + R * (qx * math.sin(theta * i / n) + ux * math.cos(theta * i / n)), cy + R * (qy * math.sin(theta * i / n) + uy * math.cos(theta * i / n))) for i in range(n + 1)]


def scythe(part, k, size):
    """size: 'big'(버서커) · 'mid'(나이트) · 'small'(로그 — 짧은 자루의 낫)."""
    w, h, gx, gy, tx, ty, ux, uy, px, py, L = kit.frame_axis(part, k)
    R = {"big": 5.2, "mid": 3.6, "small": 3.0}[size]
    bw = {"big": 2.2, "mid": 1.6, "small": 1.25}[size]
    sw = {"big": 1.0, "mid": 0.8, "small": 0.7}[size]
    if min(w, h) <= 3:
        shaft = kit.ribbon(w, h, [(gx - ux * 0.5, gy - uy * 0.5), (tx - ux * 2.0, ty - uy * 2.0)], [0.75, 0.75])
        blade = kit.ribbon(w, h, [(tx - ux * 3.5, ty - uy * 3.5), (tx + ux * 0.3, ty + uy * 0.3)], [1.5, 0.6])
        sgn = 1
    else:
        # 날이 프레임 밖으로 나가면 가장자리에 눌려 실처럼 가늘어진다 — 날 전체가 프레임 안에 드는 가장 큰 반지름을 찾는다
        # (붙는 자리 · 방향 · 반지름을 다 해 보고: 큰 날 먼저, 같으면 끝에 가까운 쪽).
        lo, hi = (0.35, 0.6) if size == "small" else (0.6, 1.0)
        best = None
        for i in range(13):
            da = L * (hi - (hi - lo) * i / 12)
            ax, ay = gx + ux * da, gy + uy * da
            for sgn in (1, -1):
                r = R
                while r >= 1.8:
                    pts = arc_pts(ax, ay, ux, uy, px * sgn, py * sgn, r)
                    hw = min(bw, r * 0.42)
                    margin = max(0.5, hw * 0.6)
                    inside = sum(1 for x, y in pts if margin <= x <= w - margin and margin <= y <= h - margin) / len(pts)
                    score = (inside >= 0.999) * 100 + r * 2 + da / L + inside
                    if best is None or score > best[0]:
                        best = (score, da, sgn, pts, hw)
                    if inside >= 0.999:
                        break
                    r -= 0.3
        _s, da, sgn, pts, hw = best
        ax, ay = gx + ux * da, gy + uy * da
        shaft = kit.ribbon(w, h, kit.clamp_pts(w, h, [(gx - ux * 1.5, gy - uy * 1.5), (ax + ux * 0.7, ay + uy * 0.7)], 0.5), [sw, sw])
        blade = kit.ribbon(w, h, kit.clamp_pts(w, h, pts, 0.4), [hw * (1 - (i / 10) ** 2.4) + 0.4 * (i / 10) ** 2.4 for i in range(11)])  # 끝 가까이까지 넓게

    def blade_fill(t, side):
        e = side * sgn  # > 0 = 안쪽(날)
        if e > 0.45:
            return BLADE_HI
        if 0.2 < t < 0.65 and e > -0.3:
            return BLADE_RED
        return BLADE_BODY if e > -0.4 else BLADE_DARK

    def blade_edge(t, side, lit):
        return BLADE_GLINT if side * sgn > 0 and t > 0.25 else BLADE_OUT

    return kit.compose(
        w,
        h,
        [
            kit.Layer(shaft, lambda t, s: SH[0] if s < -0.3 else SH[1], lambda t, s, lit: SH[0] if lit else SH[2]),
            kit.Layer(blade, blade_fill, blade_edge),
        ],
    )


lib.write_part(OUT, "weapon_berserker", [scythe("weapon_berserker", k, "big") for k in range(7)])
lib.write_part(OUT, "weapon_knight", [scythe("weapon_knight", k, "mid") for k in range(5)])
lib.write_part(OUT, "weapon_rogue", [scythe("weapon_rogue", k, "small") for k in range(4)] + [lib.BLANK])
lib.write_part(OUT, "offhand_rogue", [scythe("offhand_rogue", k, "small") for k in range(4)])

lib.write_text(
    OUT,
    """# 의상 210 — 「백은의 사신」 (Claude 초안 2026-10-09, tools_re/costume_draft_reaper.py) — HELL 모드 클리어 보상 (난이도 HARD 이상으로
# 그 회차 진행도의 95 % 이상, avatar_state::CLEAR_REWARDS · 가디언 클리어도 HELL HARD 클리어로 친다). 사용자가 준 그림 4: 은빛 긴 머리와
# 푸른 꽃 · 금장식 상의 · 푸른 소매 · 트인 회색 긴 치마 · 맨발 · 거대한 검은 낫. 이름은 Claude 가 정했다(확인 대기). 나이트 방패는 없다.
# ★ 2026-10-09 (사용자): 신발 = 금 발찌 → 하얀 스타킹. 드레스의 치마는 층(skirtunder · skirtover) — 다른 신발을 신으면 그 다리가 보인다.
set                   = 백은의 사신
head                  = 백은의 사신-머리
head.desc             = 푸른 꽃을 꽂은 은발
armor                 = 백은의 사신-드레스
armor.desc            = 금장식 상의와 회색 치마
shoes                 = 백은의 사신-하얀 스타킹
shoes.desc            = 발끝까지 감싼 흰 스타킹
weapon.berserker      = 백은의 사신-거대한 낫
weapon.berserker.desc = 검은 기운이 서린 큰 낫
weapon.knight         = 백은의 사신-검은 낫
weapon.knight.desc    = 칠흑의 날을 지닌 낫
weapon.rogue          = 백은의 사신-작은 낫
weapon.rogue.desc     = 오른손에 쥐는 작은 낫
offhand.rogue         = 백은의 사신-작은 낫
offhand.rogue.desc    = 왼손에 쥐는 작은 낫
""",
)
