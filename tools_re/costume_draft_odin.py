#!/usr/bin/env python3
"""의상 205 「할로윈 파티」 초안 생성기 (2026-10-06) — 최종보스 오딘을 본뜬 가면과 무기.

사용자(2026-10-06): *"아바타를 추가할꺼야. 할로윈 파티- 오딘 가면, 할로윈 파티- 오딘 송곳니(대검), 할로윈 파티- 미니 오딘주먹(단검)
오른손, 왼손, 할로윈 파티- 오딘의 손톱(도검)로 추가할꺼고. 최종보스 오딘을 참고해서 새로 구성하면돼."* → 얻는 법(물어서):
**HELL 클리어 · 난이도 HARD 이상**(`avatar_state::CLEAR_REWARDS`).

    python tools_re/costume_draft_odin.py            # costumes/205_halloween_party/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_odin.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_odin.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다.** 이 스크립트는 첫 초안의 기록이다(`costumes/README.md`).

오딘 = `enm/1head/13_0`(8 조각, `enm/spr/24` 가 좌우로 겹쳐 거대한 머리 하나를 만든다): 붉은 비늘(#ff5f3f · #df3f3f · #9f3f3f ·
#5f3f3f, 외곽 #3f1f3f) · 바깥으로 뻗었다 위로 휘는 줄무늬 뿔 둘(#dfbfbf · #bf9f7f · #7f5f7f · #5f5f3f) · 이마의 푸른 핏줄
(#3f9fbf · #1f5f7f) · 노란 눈 · 턱의 주황 엄니(#ffbf3f · #ff5f3f, 조각 3). 팔도 주먹도 없다 — 손톱 · 주먹은 그 색과 결로 새로 그렸다.

아이템 (이름 = 「할로윈 파티-…」: 하이픈 옆 공백을 뺀 수영장 파티 꼴 — 이름칸 114 px):
  투구 할로윈 파티-오딘 가면        head_0..2       뒤 · 옆 · 앞 — 붉은 비늘 얼굴 · 줄무늬 뿔 둘 · 노란 눈 · 푸른 핏줄 · 주황 엄니
  무기 버서커 할로윈 파티-오딘 송곳니   weapon_berserker_0..6   주황 엄니 칼날 + 붉은 비늘 코등이 + 검은 자루
  무기 나이트 할로윈 파티-오딘의 손톱   weapon_knight_0..4      뿔빛 갈고리 손톱 칼날 + 붉은 비늘 코등이
  무기 로그   할로윈 파티-미니오딘주먹   weapon_rogue_0..4 (4 = 1×1 빈 칸)   붉은 비늘 주먹 + 뿔빛 손톱 셋
  보조손 로그 할로윈 파티-미니오딘주먹   offhand_rogue_0..3      (왼손 — 같은 그림을 그 프레임 축으로)
갑옷 · 신발 · 나이트 방패는 없다(사용자 목록 그대로) — 그 칸은 다른 세트의 아바타나 장착 장비 모습이다.
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "205_halloween_party")
lib.refuse_overwrite(OUT, ["head", "weapon_berserker", "weapon_knight", "weapon_rogue", "offhand_rogue"])

# 오딘의 색 (enm/1head/13_0 에서 그대로)
OL = H("#3f1f3f")  # 외곽
BK = H("#000000")
R0 = H("#ff5f3f")  # 비늘 밝게
R1 = H("#df3f3f")
R2 = H("#9f3f3f")
R3 = H("#5f3f3f")  # 비늘 그늘
SCALE = [R0, R1, R2, R3]
HN0 = H("#dfbfbf")  # 뿔
HN1 = H("#bf9f7f")
HN2 = H("#7f5f7f")
HN3 = H("#5f5f3f")
HORN = [HN0, HN1, HN2, HN3]
BV0 = H("#3f9fbf")  # 핏줄
BV1 = H("#1f5f7f")
TU0 = H("#ffdf7f")  # 엄니 (조각 3 의 #ffbf3f → #ff5f3f, 빛 한 칸 더)
TU1 = H("#ffbf3f")
TU2 = H("#ff9f3f")
TU3 = H("#ff5f3f")
TUSK = [TU0, TU1, TU2, TU3, R2]
EYE = H("#ffff7f")
EYE2 = H("#ffbf3f")

KEY = {
    "o": OL, "K": BK,
    "R": R0, "r": R1, "d": R2, "p": R3,
    "H": HN0, "h": HN1, "m": HN2, "n": HN3,
    "B": BV0, "b": BV1,
    "T": TU1, "t": TU3,
    "Y": EYE, "y": EYE2,
}

# ── 투구: 오딘 가면 (맨머리와 같은 프레임: 뒤 16×14 · 옆 15×14(왼쪽을 본다) · 앞 16×14) ─────────────────
# 뿔 = 관자놀이에서 바깥으로 뻗었다가 위로 휜다(오딘 그대로), 밝은 H · 그늘 n 이 번갈아 줄무늬. 얼굴 = 붉은 비늘, 이마에 푸른 핏줄 V,
# 비스듬한 노란 눈, 콧등, 아래턱 양쪽에 주황 엄니가 아래로.
MASK_FRONT = [
    "H..............H",
    "hH............Hh",
    "nh....oooo....hn",
    "Hn..ooRRRrdo..nH",
    "hHnoRRBRRBrdonHh",
    ".nhoRRrBBrrdohn.",
    "..onRrrrrrrdpno.",
    "...oKYyrrdyYKo..",
    "...odrrRRrrddo..",
    "...ordRRrrdpdo..",
    "...oTpdrrdpdTo..",
    "...oTKKKKKKKto..",
    "....tKoKKoKKt...",
    ".....oo..oo.....",
]
MASK_SIDE = [
    "..........H....",
    ".........hH....",
    "....oooo..nh...",
    "...oRRRrdohHn..",
    "..oRRRBRrdnhH..",
    "..oRRrBrrdmn...",
    ".orRrrrrrdpo...",
    "oKyYrrrrddpo...",
    "odrRRrrddpo....",
    "orrdRrrdddo....",
    "oTdrrdddpo.....",
    "oTKKKKdpo......",
    ".tKoKKoo.......",
    "..oo...........",
]
MASK_BACK = [
    "H..............H",
    "hH............Hh",
    "nh....oooo....hn",
    "Hn..oorrrrdo..nH",
    "hHnordrrrrddonHh",
    ".nhordrRrrddohn.",
    "..onrrrrrrddpno.",
    "...odrrRrrddpo..",
    "...odrrrrrddpo..",
    "...oddrrrdddpo..",
    "...opddrrddppo..",
    "....opddddppo...",
    ".....opppppo....",
    "......oooo......",
]
head = [Canvas.from_art(a, KEY) for a in (MASK_BACK, MASK_SIDE, MASK_FRONT)]
lib.write_part(OUT, "head", head)


# ── 무기 ─────────────────────────────────────────────────────────────────────────


def _frame(part, k):
    (w, h), (gx, gy), (tx, ty) = lib.axis(part, k)
    ux, uy = lib._unit(tx - gx, ty - gy)
    return w, h, gx, gy, tx, ty, ux, uy, -uy, ux


def _inside(w, h, x, y, r):
    """원 중심을 프레임 안으로 — 반지름의 0.7 은 남긴다(휜 칼날이 프레임 밖으로 나가 끊겨 보이지 않게)."""
    m = max(0.5, r * 0.7)
    return min(max(x, m), w - m), min(max(y, m), h - m)


def _blade(s, w, h, gx, gy, ux, uy, px, py, start, L, r0, r1, bend, tag, hook=False):
    """손잡이에서 `start` 부터 끝(`L`)까지 반지름 r0 → r1 로 가늘어지는 원들의 줄. bend = 바깥으로 휘는 정도
    (hook = 끝으로 갈수록 휜다 — 갈고리, 아니면 가운데가 가장 휜다 — 엄니)."""
    n = max(4, int((L - start) * 2))
    for i in range(n + 1):
        t = i / n
        d = start + t * (L - start)
        b = bend * (t * t if hook else math.sin(t * math.pi))
        r = r0 + (r1 - r0) * t
        x, y = _inside(w, h, gx + ux * d + px * b, gy + uy * d + py * b, r)
        s.circle(x, y, r, tag)


def fang(part, k):
    """오딘 송곳니(버서커): 손잡이 쪽 검은 자루 · 붉은 비늘 코등이 · 끝으로 가늘어지며 살짝 휜 주황 엄니 칼날."""
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        # 칼날을 옆에서 본 프레임(3 px 폭): 축 위의 엄니 한 줄 + 코등이 한 점
        s.capsule(gx, gy, tx, ty, 1.5, "tusk")
        s.circle(*_inside(w, h, gx + ux * 1.2, gy + uy * 1.2, 1.5), 1.5, "guard")
    else:
        flat = min(w, h) <= 8
        s.capsule(*_inside(w, h, gx - ux * 1.2, gy - uy * 1.2, 1.1), *_inside(w, h, gx + ux * 1.0, gy + uy * 1.0, 1.1), 1.1, "grip")
        s.circle(*_inside(w, h, gx + ux * 2.0, gy + uy * 2.0, 2.0), 2.0, "guard")
        _blade(s, w, h, gx, gy, ux, uy, px, py, 2.6, L, 2.1, 0.55, 0.6 if flat else min(1.5, L / 8), "tusk")
    ramps = {"tusk": TUSK, "guard": SCALE, "grip": [R3, BK, BK]}
    return lib.render_shape(
        w, h, s,
        lambda tag: ramps[tag],
        lambda tag: OL,
        lambda tag: TU0 if tag == "tusk" else None,
        lambda tag: TU2 if tag == "tusk" else (R2 if tag == "guard" else None),
    )


def claw(part, k):
    """오딘의 손톱(나이트): 붉은 비늘 코등이 + 오딘 뿔빛의 갈고리 손톱 — 끝으로 가늘어지며 한쪽으로 크게 휜다."""
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.5, "claw")
        s.circle(*_inside(w, h, gx + ux * 1.0, gy + uy * 1.0, 1.5), 1.5, "guard")
    else:
        s.capsule(*_inside(w, h, gx - ux * 0.8, gy - uy * 0.8, 1.0), *_inside(w, h, gx + ux * 0.6, gy + uy * 0.6, 1.0), 1.0, "grip")
        s.circle(*_inside(w, h, gx + ux * 1.6, gy + uy * 1.6, 1.8), 1.8, "guard")
        _blade(s, w, h, gx, gy, ux, uy, px, py, 2.2, L, 1.85, 0.5, min(2.4, L / 5), "claw", hook=True)
    ramps = {"claw": HORN, "guard": SCALE, "grip": [R3, BK, BK]}
    return lib.render_shape(
        w, h, s,
        lambda tag: ramps[tag],
        lambda tag: OL,
        lambda tag: HN0 if tag == "claw" else None,
        lambda tag: HN1 if tag == "claw" else (R2 if tag == "guard" else None),
    )


def fist(part, k):
    """미니 오딘주먹(로그 · 오른손 · 왼손): 손잡이 자리에 붉은 비늘 주먹, 끝 쪽으로 부채꼴로 벌어진 뿔빛 손톱 셋."""
    w, h, gx, gy, tx, ty, ux, uy, px, py = _frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    cx, cy = _inside(w, h, gx + ux * 0.8, gy + uy * 0.8, 2.5)
    s.circle(cx, cy, 2.5, "fist")
    reach = max(2.5, L - 0.8)
    for sgn in (-1, 0, 1):
        a = (cx + ux * 2.0 + px * sgn * 1.2, cy + uy * 2.0 + py * sgn * 1.2)
        # 바깥 둘은 끝에서 더 벌어진다 — 손가락 셋이 아니라 갈퀴로 읽히게
        b = _inside(w, h, cx + ux * reach + px * sgn * 2.0, cy + uy * reach + py * sgn * 2.0, 0.6)
        s.capsule(a[0], a[1], b[0], b[1], 0.6 if sgn else 0.75, "nail")
    ramps = {"fist": SCALE, "nail": HORN}
    return lib.render_shape(
        w, h, s,
        lambda tag: ramps[tag],
        lambda tag: OL,
        lambda tag: R0 if tag == "fist" else None,
        lambda tag: HN1 if tag == "nail" else None,
    )


lib.write_part(OUT, "weapon_berserker", [fang("weapon_berserker", k) for k in range(7)])
lib.write_part(OUT, "weapon_knight", [claw("weapon_knight", k) for k in range(5)])
lib.write_part(OUT, "weapon_rogue", [fist("weapon_rogue", k) for k in range(4)] + [lib.BLANK])
lib.write_part(OUT, "offhand_rogue", [fist("offhand_rogue", k) for k in range(4)])

lib.write_text(
    OUT,
    """# 의상 205 — 「할로윈 파티」 (Claude 초안 2026-10-06, tools_re/costume_draft_odin.py) — 최종보스 오딘을 본뜬 가면과 무기.
# HELL 클리어 보상 — 난이도 HARD 이상으로 그 회차 진행도(완료한 퀘스트)의 95 % 이상 (avatar_state::CLEAR_REWARDS).
# 이름 = 「할로윈 파티-…」: 이름칸 114 px — 「미니 오딘주먹」은 공백을 빼야 들어간다(할로윈 파티-미니오딘주먹 = 112 px).
# 갑옷 · 신발 · 나이트 방패는 없다(사용자 목록 그대로).
set                   = 할로윈 파티
head                  = 할로윈 파티-오딘 가면
head.desc             = 오딘의 얼굴을 본뜬 가면
weapon.berserker      = 할로윈 파티-오딘 송곳니
weapon.berserker.desc = 오딘의 엄니로 만든 대검
weapon.knight         = 할로윈 파티-오딘의 손톱
weapon.knight.desc    = 오딘의 손톱을 본뜬 도검
weapon.rogue          = 할로윈 파티-미니오딘주먹
weapon.rogue.desc     = 오른손에 끼는 오딘 주먹
offhand.rogue         = 할로윈 파티-미니오딘주먹
offhand.rogue.desc    = 왼손에 끼는 오딘 주먹
""",
)
