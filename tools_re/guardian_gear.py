#!/usr/bin/env python3
"""「가디언」 모드 신규 장비 22종 — 무기 10 의 시트 초안 + 22 레코드의 수치표 (2026-10-08, HERO2_GUARDIAN_PLAN.md Q6 · Q7).

사용자(2026-10-08): *"무기를 5개가아닌 10개로, 장신구도 3종, 각 무기, 장신구 별로 개성있는 옵션을 줄것임.(총22종)"* ·
*"새 무기 디자인 전부 새로 뽑을것임. 기존것 어느정도 참고해도 무방. 조금더 종결 방어구 다운 디자인(터줏대감의 송곳니 단검은 송곳니
디자인 같은 식)"* → 보스를 본뜬 무기 열 자루(타입마다 둘) — **Claude 초안, 사용자 수정 대기**. ★ 2026-10-08 밤: 개성 옵션 13 은
레코드가 아니라 호스트 블록 `guest_anchors::GearPatch` 에 있다(사용자 「C 권장대로」 = HERO2_GUARDIAN_DECISIONS.md C1 초안) — 레코드는 수치·이름·그림까지.

    python tools_re/guardian_gear.py            # guardian/weapons/<종류><번호>/weapon_<직업>_<k>.png (이미 있으면 거절)
    python tools_re/guardian_gear.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다** — 의상 초안과 같은 규율(`costumes/README.md`). `make_hero2_asset.py` 가 이 폴더를 `hero/6/<종류><번호>`
시트로 싸서 jar 에 더하고(`GUARDIAN_WEAPON_DIR`), 레코드의 `body[16]` 스프라이트가 그 번호를 가리킨다. 프레임 규격 = 배포본 무기
시트(`costume_sheets.PARTS["weapon_<직업>"]`): 버서커 7 · 나이트 5 · 로그 4 + 빈 칸. 그림은 `costume_draft_lib` 의 손잡이 → 끝 축
(`GRIPS`) 위에 원·캡슐·타원으로 그린다(오딘 의상 205 와 같은 기계).

무기 (타입/번호 · 이름 · 본뜬 보스 · 생김새):
  0/33 오딘의 대검        오딘(97)        뿔빛 상아 날 넓게 · 붉은 비늘 코등이 · 검은 자루
  0/34 디미트리의 참수검   디미트리(92)    검푸른 강철 폭 넓은 날 · 날 끝 아래 홈 · 금 코등이
  1/28 고대수호물의 도끼   고대수호물(96)  돌·이끼빛 큰 도끼머리(한쪽) · 나무 자루
  1/29 율리케의 단두도끼   율리케(95)      진홍·금 양날 도끼 · 금 자루끝
  2/28 이안의 검          이안(89)        은빛 라벤더 가는 날 · 진홍 손잡이 · 카키 코등이(의상 200 의 「낡은 대검」 색)
  2/29 파멸의 수호검       파멸의수호물(98) 보랏빛 날에 노란 날끝 · 검은 코등이 (가디언 보라 #b050ff)
  3/33 터줏대감의 송곳니   터줏대감(96 지역) 뼈빛 갈고리 송곳니 · 갈색 뿌리(손잡이)
  3/34 응보의 단검        응보의수호물(99) 흑요석 날 · 붉은 빛 테두리
  4/33 심판의 철퇴        심판의 손(101)   금·청동 구슬 머리 + 가시 · 검은 자루
  4/34 보복의 망치        보복의 손(100)   검은 쇠 네모 머리 · 보라 띠 · 회색 자루
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
from costume_draft_lib import H  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WEAPON_DIR = os.path.join(ROOT, "guardian", "weapons")
# 타입 → 그리는 직업의 파트 이름 (costume_sheets.PARTS — 버서커 0·1, 나이트 2·4, 로그 3)
PART_OF_TYPE = {0: "weapon_berserker", 1: "weapon_berserker", 2: "weapon_knight", 3: "weapon_rogue", 4: "weapon_knight"}
FRAMES_OF_PART = {"weapon_berserker": 7, "weapon_knight": 5, "weapon_rogue": 4, "offhand_rogue": 4}
# ★ 2026-10-09 (사용자: *"가디언 로그 단검 크래시도 같이 고쳐야해"*): 로그는 단검을 **둘째 단검**(장비 슬롯 5 = 보조손)으로도 끼고, 그때
# 파츠 로더 `cf.a(I)V` 케이스 7 이 `hero/7/3<번호>` 를 싣는다 — 시트가 없으면 장비창 「적용 → 예」에서 NullPointerException 으로 워커가
# 죽었다(LV 90 시험대 재현). 배포본 단검 300..315 는 `hero/6` · `hero/7` 둘 다 있다. 그래서 단검(타입 3)은 같은 그림 함수로 보조손 프레임
# (`costume_sheets.PARTS["offhand_rogue"]` — 7×10 · 7×10 · 10×7 · 10×7)도 그리고, make_hero2_asset.py 가 `hero/7/<종류><번호>` 로 싼다.
OFFHAND_OF_TYPE = {3: "offhand_rogue"}

# ── 색 ───────────────────────────────────────────────────────────────────────────
BK = H("#000000")
OL_DARK = H("#1f1f3f")
OL_RED = H("#3f1f3f")
IVORY = [H("#ffffdf"), H("#dfbfbf"), H("#bf9f7f"), H("#7f5f7f")]
SCALE = [H("#ff5f3f"), H("#df3f3f"), H("#9f3f3f"), H("#5f3f3f")]
STEEL_BLUE = [H("#dfdfff"), H("#7f7fbf"), H("#3f3f7f"), H("#1f1f3f")]
GOLD = [H("#ffff7f"), H("#ffbf3f"), H("#df9f3f"), H("#7f5f1f")]
STONE = [H("#bfbfbf"), H("#9f9f7f"), H("#5f5f3f"), H("#3f3f3f")]
MOSS = [H("#9fbf5f"), H("#5f7f3f"), H("#3f5f1f")]
WOOD = [H("#bf7f3f"), H("#7f3f3f"), H("#5f1f3f")]
CRIMSON = [H("#ff5f5f"), H("#df1f3f"), H("#9f1f3f"), H("#5f1f3f")]
SILVER_LAV = [H("#ffffff"), H("#dfdfff"), H("#bfbfff"), H("#7f7fbf")]
IAN_RED = [H("#df3f5f"), H("#9f1f3f"), H("#5f1f3f")]
KHAKI = [H("#bfbf7f"), H("#7f7f3f"), H("#3f3f1f")]
PURPLE = [H("#df9fff"), H("#b050ff"), H("#7f3fbf"), H("#3f1f7f")]
YELLOW_EDGE = H("#ffff7f")
BONE = [H("#ffffdf"), H("#dfdfbf"), H("#9f9f7f"), H("#5f5f3f")]
ROOT_BROWN = [H("#9f5f3f"), H("#5f3f1f"), H("#3f1f1f")]
OBSIDIAN = [H("#7f7f7f"), H("#3f3f3f"), H("#1f1f1f"), BK]
EMBER = H("#ff3f3f")
BRONZE = [H("#ffdf7f"), H("#df9f3f"), H("#9f5f1f"), H("#5f3f1f")]
IRON = [H("#9f9f9f"), H("#5f5f5f"), H("#3f3f3f"), H("#1f1f1f")]
GREY = [H("#bfbfbf"), H("#7f7f7f"), H("#3f3f3f")]


def _frame(part, k):
    (w, h), (gx, gy), (tx, ty) = lib.axis(part, k)
    ux, uy = lib._unit(tx - gx, ty - gy)
    return w, h, gx, gy, tx, ty, ux, uy, -uy, ux


def _inside(w, h, x, y, r):
    m = max(0.5, r * 0.7)
    return min(max(x, m), w - m), min(max(y, m), h - m)


def _line(s, w, h, gx, gy, ux, uy, px, py, start, L, r0, r1, bend, tag, hook=False):
    """손잡이에서 start 부터 L 까지 반지름 r0 → r1 의 원 줄(휘는 정도 bend; hook = 끝으로 갈수록)."""
    n = max(4, int((L - start) * 2))
    for i in range(n + 1):
        t = i / n
        d = start + t * (L - start)
        b = bend * (t * t if hook else math.sin(t * math.pi))
        r = r0 + (r1 - r0) * t
        x, y = _inside(w, h, gx + ux * d + px * b, gy + uy * d + py * b, r)
        s.circle(x, y, r, tag)


def _grip(s, w, h, gx, gy, ux, uy, back, fwd, r, tag="grip"):
    s.capsule(*_inside(w, h, gx - ux * back, gy - uy * back, r), *_inside(w, h, gx + ux * fwd, gy + uy * fwd, r), r, tag)


# ★ 2026-10-14 skill_gear.py: the same shapes in other colours — {tag: ramp, "_outline": colour} replaces those ramps (a replaced
# tag's highlight / lit outline then come from its new ramp). None = the guardian weapons as drawn.
RECOLOUR = None


def _render(w, h, s, ramps, outline, highlight=None, lit=None):
    if RECOLOUR:
        swap = {t: r for t, r in RECOLOUR.items() if not t.startswith("_")}
        ramps = {**ramps, **swap}
        outline = RECOLOUR.get("_outline", outline)
        if highlight:
            highlight = {t: (ramps[t][0] if t in swap else c) for t, c in highlight.items()}
        if lit:
            lit = {t: (ramps[t][1] if t in swap else c) for t, c in lit.items()}
    return lib.render_shape(
        w, h, s,
        lambda tag: ramps[tag],
        lambda tag: outline,
        (lambda tag: highlight.get(tag)) if highlight else None,
        (lambda tag: lit.get(tag)) if lit else None,
    )


# ── 열 자루 ──────────────────────────────────────────────────────────────────────


def odin_greatsword(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.5, "blade")
        s.circle(*_inside(w, h, gx + ux * 1.4, gy + uy * 1.4, 1.5), 1.5, "guard")
    else:
        flat = min(w, h) <= 8
        _grip(s, w, h, gx, gy, ux, uy, 1.4, 1.0, 1.1)
        s.ellipse(*_inside(w, h, gx + ux * 2.0, gy + uy * 2.0, 1.4), px, py, 2.6 if not flat else 1.6, 1.3, "guard")
        _line(s, w, h, gx, gy, ux, uy, px, py, 2.8, L, 2.4 if not flat else 1.6, 1.0, 0.0, "blade")
    return _render(w, h, s, {"blade": IVORY, "guard": SCALE, "grip": [SCALE[3], BK, BK]}, OL_RED, {"blade": IVORY[0]}, {"blade": IVORY[1], "guard": SCALE[2]})


def dimitri_sword(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.5, "blade")
        s.circle(*_inside(w, h, gx + ux * 1.4, gy + uy * 1.4, 1.5), 1.5, "guard")
    else:
        flat = min(w, h) <= 8
        _grip(s, w, h, gx, gy, ux, uy, 1.4, 1.0, 1.1)
        s.ellipse(*_inside(w, h, gx + ux * 2.0, gy + uy * 2.0, 1.3), px, py, 2.4 if not flat else 1.5, 1.1, "guard")
        # 폭 넓은 날: 끝 쪽이 더 넓다가 비스듬히 끝난다 (참수검)
        _line(s, w, h, gx, gy, ux, uy, px, py, 2.8, L * 0.85, 2.0 if not flat else 1.4, 2.6 if not flat else 1.5, 0.0, "blade")
        s.circle(*_inside(w, h, gx + ux * L * 0.92 + px * 0.9, gy + uy * L * 0.92 + py * 0.9, 1.2), 1.2, "blade")
    return _render(w, h, s, {"blade": STEEL_BLUE, "guard": GOLD, "grip": [IRON[2], BK, BK]}, OL_DARK, {"blade": STEEL_BLUE[0]}, {"blade": STEEL_BLUE[1], "guard": GOLD[1]})


def ancient_axe(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.4, "haft")
        s.circle(*_inside(w, h, gx + ux * L * 0.78, gy + uy * L * 0.78, 1.5), 1.5, "head")
    else:
        flat = min(w, h) <= 8
        _grip(s, w, h, gx, gy, ux, uy, 1.4, L * 0.8, 1.0, "haft")
        hx, hy = gx + ux * L * 0.72, gy + uy * L * 0.72
        # 한쪽으로 뻗은 돌 머리: 축과 직각인 타원 + 그 위 이끼
        s.ellipse(*_inside(w, h, hx + px * 1.6, hy + py * 1.6, 2.4), px, py, 3.6 if not flat else 2.2, 2.4 if not flat else 1.6, "head")
        s.circle(*_inside(w, h, hx + px * 2.6 - ux * 0.6, hy + py * 2.6 - uy * 0.6, 1.0), 1.0, "moss")
    return _render(w, h, s, {"head": STONE, "moss": MOSS, "haft": WOOD}, OL_DARK, {"head": STONE[0]}, {"head": STONE[1]})


def ulrike_axe(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.4, "haft")
        s.circle(*_inside(w, h, gx + ux * L * 0.78, gy + uy * L * 0.78, 1.5), 1.5, "head")
    else:
        flat = min(w, h) <= 8
        _grip(s, w, h, gx, gy, ux, uy, 1.4, L * 0.82, 1.0, "haft")
        s.circle(*_inside(w, h, gx - ux * 1.2, gy - uy * 1.2, 1.2), 1.2, "pommel")
        hx, hy = gx + ux * L * 0.72, gy + uy * L * 0.72
        for sgn in (-1, 1):
            s.ellipse(*_inside(w, h, hx + px * sgn * 1.8, hy + py * sgn * 1.8, 2.2), px, py, 3.0 if not flat else 1.8, 2.0 if not flat else 1.3, "head")
        s.circle(*_inside(w, h, hx, hy, 1.1), 1.1, "pommel")
    return _render(w, h, s, {"head": CRIMSON, "pommel": GOLD, "haft": [GOLD[2], WOOD[1], WOOD[2]]}, OL_RED, {"head": CRIMSON[0]}, {"head": CRIMSON[1], "pommel": GOLD[1]})


def ian_sword(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.4, "blade")
        s.circle(*_inside(w, h, gx + ux * 1.2, gy + uy * 1.2, 1.4), 1.4, "guard")
    else:
        _grip(s, w, h, gx, gy, ux, uy, 1.0, 0.8, 1.0)
        s.ellipse(*_inside(w, h, gx + ux * 1.8, gy + uy * 1.8, 1.0), px, py, 2.0, 0.9, "guard")
        _line(s, w, h, gx, gy, ux, uy, px, py, 2.4, L, 1.5, 0.6, 0.0, "blade")
    return _render(w, h, s, {"blade": SILVER_LAV, "guard": KHAKI, "grip": IAN_RED}, OL_DARK, {"blade": SILVER_LAV[0]}, {"blade": SILVER_LAV[1]})


def doom_sword(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.4, "blade")
        s.circle(*_inside(w, h, gx + ux * 1.2, gy + uy * 1.2, 1.4), 1.4, "guard")
    else:
        _grip(s, w, h, gx, gy, ux, uy, 1.0, 0.8, 1.0)
        s.ellipse(*_inside(w, h, gx + ux * 1.8, gy + uy * 1.8, 1.0), px, py, 2.2, 1.0, "guard")
        _line(s, w, h, gx, gy, ux, uy, px, py, 2.4, L, 1.7, 0.8, 0.0, "blade")
        s.circle(*_inside(w, h, tx - ux * 0.3, ty - uy * 0.3, 0.8), 0.8, "edge")
    return _render(w, h, s, {"blade": PURPLE, "edge": [YELLOW_EDGE, GOLD[1]], "guard": OBSIDIAN, "grip": [OBSIDIAN[1], BK, BK]}, OL_DARK, {"blade": PURPLE[0]}, {"blade": PURPLE[1]})


def boss_fang(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.4, "fang")
    else:
        s.circle(*_inside(w, h, gx + ux * 0.6, gy + uy * 0.6, 1.6), 1.6, "root")
        _line(s, w, h, gx, gy, ux, uy, px, py, 1.6, L, 1.8, 0.5, min(2.2, L / 5), "fang", hook=True)
    return _render(w, h, s, {"fang": BONE, "root": ROOT_BROWN}, OL_RED, {"fang": BONE[0]}, {"fang": BONE[1]})


def retribution_dagger(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.4, "blade")
    else:
        _grip(s, w, h, gx, gy, ux, uy, 0.8, 0.8, 0.9)
        s.circle(*_inside(w, h, gx + ux * 1.6, gy + uy * 1.6, 1.3), 1.3, "guard")
        _line(s, w, h, gx, gy, ux, uy, px, py, 2.2, L, 1.5, 0.5, 0.0, "blade")
    return _render(w, h, s, {"blade": OBSIDIAN, "guard": CRIMSON, "grip": [IRON[2], BK, BK]}, EMBER, {"blade": GREY[0]}, {"blade": EMBER, "guard": CRIMSON[1]})


def judgement_mace(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.3, "haft")
        s.circle(*_inside(w, h, tx - ux * 1.2, ty - uy * 1.2, 1.5), 1.5, "head")
    else:
        _grip(s, w, h, gx, gy, ux, uy, 1.0, L * 0.7, 0.9, "haft")
        hx, hy = _inside(w, h, gx + ux * (L - 2.6), gy + uy * (L - 2.6), 2.6)
        s.circle(hx, hy, 2.6, "head")
        for a in (0.0, 0.8, 1.6, 2.4, 3.2, 4.0, 4.8, 5.6):
            ex, ey = math.cos(a), math.sin(a)
            s.capsule(hx + ex * 2.2, hy + ey * 2.2, *_inside(w, h, hx + ex * 3.6, hy + ey * 3.6, 0.5), 0.5, "spike")
    return _render(w, h, s, {"head": GOLD, "spike": BRONZE, "haft": [IRON[1], IRON[2], BK]}, OL_DARK, {"head": GOLD[0]}, {"head": GOLD[1]})


def vengeance_hammer(part, k):
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.3, "haft")
        s.circle(*_inside(w, h, tx - ux * 1.2, ty - uy * 1.2, 1.5), 1.5, "head")
    else:
        _grip(s, w, h, gx, gy, ux, uy, 1.0, L * 0.72, 0.9, "haft")
        hx, hy = gx + ux * (L - 2.4), gy + uy * (L - 2.4)
        s.ellipse(*_inside(w, h, hx, hy, 2.2), px, py, 3.4, 2.0, "head")
        s.ellipse(*_inside(w, h, hx, hy, 1.0), px, py, 3.0, 0.7, "band")
    return _render(w, h, s, {"head": IRON, "band": PURPLE, "haft": GREY}, OL_DARK, {"head": IRON[0]}, {"head": IRON[1], "band": PURPLE[0]})


# (타입, 번호, 이름, 그리기, 공격 최소, 공격 최대, 치명(body[32]), 옵션 범위 (min, max) 다섯 쌍 공통, 슬롯 수, 가격)
# 수치 = 그 타입 종결(팬텀 블레이드 239–256 · 어둠의광마도끼 218–251 · 풍마의 분노검 150–164 · 팬텀 퀘이드 81–88 · 파괴마의 등뼈
# 154–168) × 1.25 안팎 — 둘째 자루가 조금 더 세다. 치명은 종결과 같은 자리값(18~22) + 2.
WEAPONS = [
    (0, 33, "오딘의 대검", odin_greatsword, 300, 320, 20, (16, 26), 2, 2_917_600),
    (0, 34, "디미트리의 참수검", dimitri_sword, 310, 335, 22, (16, 26), 2, 3_112_100),
    (1, 28, "고대수호물의 도끼", ancient_axe, 275, 315, 22, (15, 25), 2, 2_463_800),
    (1, 29, "제스의 수련도끼", ulrike_axe, 285, 325, 24, (15, 25), 2, 2_628_000),  # 2026-10-09 사용자: 율리케의 단두도끼 → 제스의 수련도끼
    (2, 28, "이안의 검", ian_sword, 190, 205, 22, (9, 15), 1, 1_653_800),
    (2, 29, "파멸의 수호검", doom_sword, 195, 212, 24, (9, 15), 1, 1_764_000),
    (3, 33, "터줏대감의 송곳니", boss_fang, 100, 110, 24, (9, 13), 1, 1_167_000),
    (3, 34, "응보의 단검", retribution_dagger, 104, 114, 26, (9, 13), 1, 1_244_800),
    (4, 33, "심판의 철퇴", judgement_mace, 192, 210, 20, (10, 15), 1, 1_927_500),
    (4, 34, "보복의 망치", vengeance_hammer, 198, 216, 22, (10, 15), 1, 2_056_000),
]
WEAPON_LEVEL = 85  # 2026-10-08 저녁: 전부 LV 85 — 가디언 레벨 80 대 지역(HELL LV 66+ = 원래 LV 15+)부터 떨어진다
WEAPON_REFINE = 50

# 방어구 열여덟 (★ 2026-10-09 사용자: *"가디언 섀도우 가디언 이런식으로 세트템처럼 보이게 하고싶지않아서 … 다시 이름뽑고 개성옵션도
# 다시 추가해서 … 각각 종류, 부위별 1개씩 더 추가해서 총 18종 … 방패중에 방패치기 데미지 +50%증가 하나"* — 이름 · 색은 Claude 초안)
# — 기존 상위 시트(스프라이트) + 아이템마다 다른 새 팔레트(`hero/h.p` 레코드). 개성 옵션은 레코드가 아니라 호스트 블록
# `guest_anchors::GearPatch::ITEMS`. (타입, 번호, 이름, LV, 방어, 제련 상한, 중갑, 스프라이트 = 바탕 시트, 옵션 범위, 슬롯 수, 가격,
# 팔레트 번호 — make_hero2_asset.guardian_palettes 의 순서, 색 = (본색 색상°, 본색 채도, 명도 배율, 강조 색상°, 강조 채도))
ARMOUR = [
    (5, 28, "용암군주의 투구", 85, 46, 0, 1, 16, (6, 11), 2, 4_558_700, 0, (12, 0.75, 0.95, 45, 0.6)),  # 파괴마의 머리 hero/0/16
    (5, 29, "그림자 사냥꾼의 두건", 85, 44, 0, 0, 10, (6, 11), 2, 2_799_300, 1, (268, 0.25, 0.7, 0, 0.0)),  # 팬텀 헬름 hero/0/10
    (5, 30, "빙해왕의 투구", 85, 46, 0, 1, 15, (6, 11), 2, 4_558_700, 9, (190, 0.45, 1.0, 200, 0.15)),  # 광속의 헤드기어 hero/0/15
    (5, 31, "바람술사의 머리띠", 85, 44, 0, 0, 7, (6, 11), 2, 2_799_300, 10, (140, 0.55, 0.9, 55, 0.5)),  # 드래곤 헬름 hero/0/07
    (6, 51, "불굴의 흉갑", 85, 75, 25, 1, 20, (9, 15), 2, 7_294_000, 2, (215, 0.25, 0.95, 45, 0.6)),  # 열화의 흉갑 hero/1/20
    (6, 52, "밤까마귀 가죽갑옷", 85, 68, 20, 0, 11, (9, 15), 2, 4_862_700, 3, (250, 0.3, 0.6, 285, 0.55)),  # 팬텀레더아머 hero/1/11
    (6, 53, "심연의 판금갑옷", 85, 75, 25, 1, 18, (9, 15), 2, 7_294_000, 11, (230, 0.7, 0.75, 175, 0.6)),  # 악마의 흉갑 hero/1/18
    (6, 54, "서리늑대 가죽옷", 85, 68, 20, 0, 10, (9, 15), 2, 4_862_700, 12, (205, 0.2, 1.0, 0, 0.0)),  # 코르버스암살복 hero/1/10
    (8, 51, "거인의 건틀릿", 85, 41, 0, 1, 14, (6, 11), 1, 3_647_000, 4, (30, 0.6, 0.9, 18, 0.7)),  # 열화의 손길 hero/2/14
    (8, 52, "독사의 장갑", 85, 36, 0, 0, 8, (6, 11), 1, 2_431_300, 5, (105, 0.7, 0.8, 75, 0.7)),  # 망령의 손길 hero/2/08
    (8, 53, "파쇄자의 손갑", 85, 41, 0, 1, 13, (6, 11), 1, 3_647_000, 13, (355, 0.65, 0.8, 0, 0.0)),  # 악마의 손아귀 hero/2/13
    (8, 54, "질풍의 손목보호대", 85, 36, 0, 0, 6, (6, 11), 1, 2_431_300, 14, (200, 0.6, 1.0, 0, 0.0)),  # 암살자의건틀릿 hero/2/06
    (9, 51, "산맥의 철각", 85, 46, 0, 1, 17, (6, 11), 1, 5_835_200, 6, (28, 0.45, 0.8, 0, 0.0)),  # 열화의 발자국 hero/3/17
    (9, 52, "그림자 걸음", 85, 40, 0, 0, 9, (6, 11), 1, 3_890_100, 7, (0, 0.0, 0.55, 350, 0.65)),  # 망령의 그림자 hero/3/09
    (9, 53, "진군의 철화", 85, 46, 0, 1, 16, (6, 11), 1, 5_835_200, 15, (350, 0.7, 0.85, 45, 0.6)),  # 악마의 발굽 hero/3/16
    (9, 54, "비호의 장화", 85, 40, 0, 0, 4, (6, 11), 1, 3_890_100, 16, (30, 0.8, 0.95, 25, 0.4)),  # 암살자의 신발 hero/3/04
    (10, 28, "성채의 방패", 85, 49, 25, 1, 14, (6, 11), 1, 733_500, 8, (210, 0.3, 1.0, 45, 0.6)),  # 아에기스 hero/7/1014
    (10, 29, "격돌의 방패", 85, 49, 25, 1, 13, (6, 11), 1, 733_500, 17, (20, 0.7, 0.9, 0, 0.75)),  # 정의의 수호자 hero/7/1013
]

# 장신구 셋 — 계열 C 꼬리(힘·체력·민첩·의지·지력 쌍 고정 + HP/SP). (타입, 번호, 이름, LV, 힘, 체력, 민첩, 의지, 지력, 방어, HP, SP, 가격)
# ★ 2026-10-09: 이름에 「가디언」이 들어가지 않게(사용자) — Claude 초안. 개성 옵션 = GearPatch::ITEMS(받는 피해 −5 % · 스킬 SP −15 % ·
# 최대 HP +10 %). ★ 2026-10-09 (3): 정보창의 설명은 네 줄까지(AccessoryPatch::DESC_LINES — 설명이 있는 장신구는 게임의 능력치 줄을
# 지우고 설명이 능력치까지 말한다) — 용의 심장 힘 15 · 체력 15 → 체력 30, 현자의 인장 민첩 15 · 의지 15 → 의지 30(합계 그대로, 한 줄에 들게).
ACCESSORIES = [
    (11, 20, "용의 심장", 85, 0, 30, 0, 0, 0, 0, 250, 0, 3_240_000),
    (12, 20, "현자의 인장", 85, 0, 0, 0, 30, 0, 0, 0, 150, 2_495_600),
    (12, 21, "생명의 서약", 85, 10, 0, 0, 0, 15, 10, 100, 100, 2_495_600),
]


def weapon_stem(t, nn):
    return f"{t}{nn:02d}"


def draw_all(out=WEAPON_DIR, force=False):
    """파트마다 따로 — 그 파트의 PNG 가 이미 있으면 거절(손으로 고친 것일 수 있다, `--force` 로 덮어쓴다). 그래서 무기 PNG 가 있는
    폴더에도 없는 보조손만 더해 그릴 수 있다(2026-10-09)."""
    made = []
    for t, nn, name, fn, *_rest in WEAPONS:
        d = os.path.join(out, weapon_stem(t, nn))
        parts = [PART_OF_TYPE[t]] + ([OFFHAND_OF_TYPE[t]] if t in OFFHAND_OF_TYPE else [])
        for part in parts:
            if os.path.isdir(d) and any(f.startswith(part + "_") and f.endswith(".png") for f in os.listdir(d)) and not force:
                print(f"{d}: {part} PNG 가 이미 있다 — 손으로 고친 것일 수 있다 (--force 로 덮어쓴다)")
                continue
            frames = [fn(part, k) for k in range(FRAMES_OF_PART[part])]
            if part == "weapon_rogue":
                frames.append(lib.BLANK)
            lib.write_part(d, part, frames)
            made.append(f"{d} {part}")
        if not os.path.exists(os.path.join(d, "item.txt")) or force:
            with open(os.path.join(d, "item.txt"), "w", encoding="utf-8", newline="\n") as f:
                f.write(f"{t}/{nn} {name} — Claude 초안 2026-10-08 (tools_re/guardian_gear.py). PNG 가 정본.\n")
    return made


if __name__ == "__main__":
    sys.stdout.reconfigure(errors="replace")  # cp949 콘솔에서도 죽지 않게 (costume_sheets.py 와 같은 이유)
    args = [a for a in sys.argv[1:] if a != "--force"]
    draw_all(args[0] if args else WEAPON_DIR, "--force" in sys.argv)
