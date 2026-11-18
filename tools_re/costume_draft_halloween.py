#!/usr/bin/env python3
"""의상 204 「할로윈」 초안 생성기 (2026-10-01) — HELL 클리어(난이도 NORMAL 이상으로 진행도 95 %) 보상.

사용자(2026-10-01): *"헬모드 - 난이도 normal 이상으로 클리어하면 해골 모양 아바타를 줄꺼야. 할로윈 - 해골, 할로윈- 해골 상의,
할로윈 해골 하의에 무기는 뼈다귀 모양으로 만들꺼야 전부. 대검은 큰 뼈다귀, 도검은 중간 크기 뼈다귀, 방패는 갈비뼈모양 방패,
단검은 작은 뼈다귀, 보조단검도 작은 뼈댜귀."*

    python tools_re/costume_draft_halloween.py            # costumes/204_halloween/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_halloween.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_halloween.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다.** 이 스크립트는 첫 초안의 기록이다(`costumes/README.md`).

아이템 (이름은 「할로윈-…」 — 수영장 파티와 같은 모양):
  투구 할로윈-해골        head_0..2   해골 가면(뒤 · 옆 · 앞) — 상아빛 뼈, 검은 눈구멍에 주황 불빛, 이빨
  갑옷 할로윈-해골 상의   body_2 · arm_0..10
                                      몸통 = 검은 옷에 상아빛 갈비뼈와 복장뼈 · 팔 = 맨팔 hero/2/100 의 밝은 쪽 = 팔뼈 그림, 나머지 = 검은 옷,
                                      손(장갑 자리) = 뼈 손
  신발 할로윈-해골 하의   leg_0..10   로그 바지 hero/3/102 의 실루엣 = 검은 바지, 밝은 칸 = 다리뼈 그림, 발 = 뼈
  무기 버서커 할로윈-큰 뼈다귀     weapon_berserker_0..6
  무기 나이트 할로윈-중간 뼈다귀   weapon_knight_0..4
  무기 로그   할로윈-작은 뼈다귀   weapon_rogue_0..4 (4 = 1×1 빈 칸)
  보조손 나이트 할로윈-갈비뼈 방패 offhand_knight_0..2  갈비뼈 여섯 쌍 + 복장뼈(앞) · 옆 · 등뼈(뒤)
  보조손 로그   할로윈-작은 뼈다귀 offhand_rogue_0..3
뼈다귀는 `costume_draft_lib.GRIPS` 의 (손잡이 → 끝) 축을 따라 그린다 — 손이 한쪽 뼈마디 바로 안쪽 자루를 쥔다.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
import costume_sheets as cs  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "204_halloween")
lib.refuse_overwrite(OUT, ["head", "body", "arm", "leg", "weapon_berserker", "weapon_knight", "weapon_rogue", "offhand_knight", "offhand_rogue"])

jar = cs.open_jar(cs.DEFAULT_ZIP)

# 뼈 (밝은 것부터)
BW = H("#ffffff")
BL = H("#ffffdf")
BM = H("#dfdfbf")
BS = H("#bfbf9f")
BD = H("#9f9f7f")
BO = H("#3f3f3f")  # 뼈의 외곽
BONE = [BW, BL, BM, BS, BD]
# 검은 옷
K0 = H("#000000")
K1 = H("#1f1f3f")
K2 = H("#3f3f5f")
K3 = H("#5f5f7f")
# 눈구멍의 불빛
GL = H("#ff9f1f")

KEY = {"o": BO, "W": BW, "L": BL, "M": BM, "S": BS, "D": BD, "K": K0, "k": K1, "n": K2, "g": GL}

# ── 투구: 해골 (프레임 = 맨머리와 같은 크기 · 같은 자리: 뒤 16×14 · 옆 15×14(왼쪽을 본다) · 앞 16×14) ──
SKULL_BACK = [
    "................",
    "................",
    ".....oooooo.....",
    "....oLLLLLMo....",
    "...oLLLLLLLMo...",
    "...oLLLLLLLMo...",
    "...oLLLMLLLMo...",
    "...oLLLLMLMSo...",
    "...oLLLLLLMSo...",
    "...oMLLLLLMSo...",
    "...oMMLLLMSSo...",
    "....oMMMMSSo....",
    ".....oSSSSo.....",
    "......oooo......",
]
SKULL_SIDE = [
    "...............",
    "...............",
    ".....oooooo....",
    "....oWLLLLMo...",
    "...oLLLLLLLMo..",
    "..oLLLLLLLLMSo.",
    "..oLLLLLLLLMSo.",
    "..oKKKLLLLMMSo.",
    "..oKgKLLLMMSSo.",
    ".oLLLLLLLMMSo..",
    ".oKLMLLLMMSo...",
    "..oLKLKMMSo....",
    "..oLKLKMSo.....",
    "...ooooo.......",
]
SKULL_FRONT = [
    "................",
    "................",
    ".....oooooo.....",
    "....oWLLLLMo....",
    "...oLLLLLLLMo...",
    "...oLLLLLLLMo...",
    "...oLLLLLLMMo...",
    "...oKKKLLKKKo...",
    "...oKgKLLKgKo...",
    "...oLKKLLKKSo...",
    "...ooLLKKLMSo...",
    "....oMLLLMSo....",
    "....oLKLKLKo....",
    ".....oooooo.....",
]
lib.write_part(OUT, "head", [Canvas.from_art(a, KEY) for a in (SKULL_BACK, SKULL_SIDE, SKULL_FRONT)])

# ── 갑옷: 몸통 — 검은 옷에 갈비뼈 (모든 방향이 이 한 장) ─────────────────────────
RIBS = [
    ".LLkMS.",
    "KLMWMSK",
    "KkkWkkK",
    "KLMWMSK",
    ".kkMkk.",
]
lib.write_part(OUT, "body", [lib.BLANK, lib.BLANK, Canvas.from_art(RIBS, KEY), lib.BLANK, lib.BLANK])

# ── 갑옷: 팔 — 맨팔의 밝은 쪽 = 팔뼈 그림, 나머지 = 검은 옷, 손 = 뼈 손 ───────────────
arm_base = cs.shipped_sheet(jar, 2, 100)
glove = cs.shipped_sheet(jar, 2, 0)
blue = {i for i, c in enumerate(glove.palette) if i and c[2] > c[0] + 20}
# hero/2/100: c 5f1f3f < a 7f3f3f < b bf5f3f < e bf7f3f < d ff9f7f < f ffdfbf
SLEEVE = {H("#5f1f3f"): K0, H("#7f3f3f"): K1, H("#bf5f3f"): K2, H("#bf7f3f"): K2, H("#ff9f7f"): BM, H("#ffdfbf"): BL}
HAND = {H("#5f1f3f"): BO, H("#7f3f3f"): BD, H("#bf5f3f"): BS, H("#bf7f3f"): BS, H("#ff9f7f"): BM, H("#ffdfbf"): BL}
arms = []
for k, fr in enumerate(arm_base.frames):
    rows = lib.rgb_rows(fr, arm_base.palette)
    cv = Canvas(fr.w, fr.h)
    for y, row in enumerate(rows):
        for x, c in enumerate(row):
            if c is not None:
                hand = fr.pixels[y][x] and glove.frames[k].pixels[y][x] in blue
                cv.put(x, y, HAND[c] if hand else SLEEVE[c])
    arms.append(cv)
lib.write_part(OUT, "arm", arms)

# ── 신발: 검은 바지 + 다리뼈 그림 + 뼈 발 (로그 바지 hero/3/102 의 실루엣) ───────────
# hero/3/102: a 1f1f3f · b 3f3f3f · c 7f1f3f · d 5f5f3f · e 7f7f3f · f bf7f3f · g 9f3f3f · h ff9f7f · i ffdfbf ·
#             j 9f9f7f · k df7f7f — 짙은 천 = 검은 옷, 밝은 칸(정강이 · 무릎 · 넓적다리의 빛) = 뼈 그림.
PANTS = {
    H("#1f1f3f"): K0,
    H("#3f3f3f"): K1,
    H("#7f1f3f"): K1,
    H("#9f3f3f"): K2,
    H("#5f5f3f"): K2,
    H("#7f7f3f"): K3,
    H("#bf7f3f"): BS,
    H("#9f9f7f"): BS,
    H("#df7f7f"): BM,
    H("#ff9f7f"): BM,
    H("#ffdfbf"): BL,
}
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
            foot = y >= top + 7 and bottom[x] >= h - 3 and y >= bottom[x] - 1
            if foot:
                cv.put(x, y, BO if y == bottom[x] else BM)
            else:
                cv.put(x, y, PANTS[c])
    legs.append(cv)
lib.write_part(OUT, "leg", legs)

# ── 무기: 뼈다귀 ────────────────────────────────────────────────────────────────


def bone(part, k, shaft, knob, spread, back=1.0):
    """손잡이 → 끝 축의 뼈다귀: 자루(캡슐) + 양 끝 뼈마디 둘씩(서로 떨어진 두 혹이라야 「뼈다귀」로 읽힌다). 마디는 프레임
    안으로 당겨 둔다(잘리면 막대기가 된다). 폭 3 px 프레임(옆에서 본 칼날)은 마디를 축 위에 겹친다."""
    (w, h), (gx, gy), (tx, ty) = lib.axis(part, k)
    ux, uy = lib._unit(tx - gx, ty - gy)
    px, py = -uy, ux
    thin = min(w, h) <= 3
    if thin:
        shaft, knob, spread = 1.5, 1.6, 0.0
    a = (gx - ux * back, gy - uy * back)
    b = (tx - ux * (knob * 0.8), ty - uy * (knob * 0.8))

    def inside(cx, cy):
        m = max(0.5, knob - 0.35)
        return (min(max(cx, m), w - m), min(max(cy, m), h - m))

    s = lib.Shape().capsule(a[0], a[1], b[0], b[1], shaft, "shaft")
    for end, dirn in ((a, -1), (b, 1)):
        for sgn in (-1, 1) if spread else (0,):
            cx, cy = inside(end[0] + px * sgn * spread + ux * dirn * 0.3, end[1] + py * sgn * spread + uy * dirn * 0.3)
            s.circle(cx, cy, knob, "knob")
    return lib.render_shape(w, h, s, lambda tag: BONE, lambda tag: BO, lambda tag: BW, lambda tag: BS)


# (자루 반지름, 마디 반지름, 마디 간격) — 자루는 3 px 이상이라야 가운데에 밝은 칸이 남는다(배포본 칼날 = 어두운 가장자리 ·
# 밝은 심 · 중간 가장자리, 3 px).
lib.write_part(OUT, "weapon_berserker", [bone("weapon_berserker", k, 2.0, 2.4, 2.1, back=1.4) for k in range(7)])
lib.write_part(OUT, "weapon_knight", [bone("weapon_knight", k, 1.5, 1.85, 1.65, back=1.1) for k in range(5)])
lib.write_part(OUT, "weapon_rogue", [bone("weapon_rogue", k, 1.3, 1.6, 1.45, back=0.9) for k in range(4)] + [lib.BLANK])
lib.write_part(OUT, "offhand_rogue", [bone("offhand_rogue", k, 1.3, 1.6, 1.45, back=0.9) for k in range(4)])

# ── 보조손: 갈비뼈 방패 — 앞 = 복장뼈 + 휜 갈비뼈 여섯 쌍(사이는 검은 틈) · 옆 = 그 단면 · 뒤 = 등뼈 ──
SHIELD_FRONT = [
    "...............",
    "...ooooooooo...",
    "...oLLLWLLMo...",
    "...LkkLWMkkS...",
    "...oLLLWLMSo...",
    "...LkkLWMkkS...",
    "...oLLLWLMSo...",
    "...LkkLWMkkS...",
    "...oLLLWLMSo...",
    "....LkLWMkS....",
    "....oLLWMSo....",
    ".....okWko.....",
    "......ooo......",
    "...............",
    "...............",
    "...............",
    "...............",
]
SHIELD_SIDE = [
    ".......",
    ".oo....",
    ".oLMo..",
    ".kLSo..",
    ".oLMo..",
    ".kLSo..",
    ".oLMo..",
    ".kLSo..",
    ".oLMo..",
    ".kLSo..",
    "..LMo..",
    "..kSo..",
    "...o...",
    ".......",
    ".......",
    ".......",
    ".......",
]
SHIELD_BACK = [
    "...............",
    "...ooooooooo...",
    "...oSSMDMSSo...",
    "...SkkoMokkD...",
    "...oSSMDMSSo...",
    "...SkkoMokkD...",
    "...oSSMDMSSo...",
    "...SkkoMokkD...",
    "...oSSMDMSSo...",
    "....SkoMokD....",
    "....oSMDMSo....",
    ".....okMko.....",
    "......ooo......",
    "...............",
    "...............",
    "...............",
    "...............",
]
lib.write_part(OUT, "offhand_knight", [Canvas.from_art(a, KEY) for a in (SHIELD_FRONT, SHIELD_SIDE, SHIELD_BACK)])

lib.write_text(
    OUT,
    """# 의상 204 — 「할로윈」 (Claude 초안 2026-10-01, tools_re/costume_draft_halloween.py)
# HELL 클리어 보상 — 난이도 NORMAL 이상으로 그 회차 진행도(완료한 퀘스트)의 95 % 이상 (avatar_state::CLEAR_REWARDS).
set                 = 할로윈
head                = 할로윈-해골
head.desc           = 섬뜩하게 웃는 해골 가면
armor               = 할로윈-해골 상의
armor.desc          = 뼈가 그려진 검은 상의
shoes               = 할로윈-해골 하의
shoes.desc          = 뼈가 그려진 검은 하의
weapon.berserker    = 할로윈-큰 뼈다귀
weapon.berserker.desc = 대검만큼 커다란 뼈다귀
weapon.knight       = 할로윈-중간 뼈다귀
weapon.knight.desc  = 검처럼 휘두르는 뼈다귀
weapon.rogue        = 할로윈-작은 뼈다귀
weapon.rogue.desc   = 단검처럼 쥐는 뼈다귀
offhand.knight      = 할로윈-갈비뼈 방패
offhand.knight.desc = 갈비뼈로 엮은 방패
offhand.rogue       = 할로윈-작은 뼈다귀
offhand.rogue.desc  = 왼손에 쥐는 작은 뼈다귀
""",
)
