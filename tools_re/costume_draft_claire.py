#!/usr/bin/env python3
"""의상 202 「클레르의 잠옷」 초안 생성기 (2026-10-01) — 새 게임부터 누구나 가진 기본 아바타.

사용자(2026-10-01): *"아무 조건없이 맨처음부터 가지고있는(새게임 시작 시 부터 가지게 될)아바타로 클레르 머리, 클레르의 잠옷
상의, 클레르의 잠옷 바지 를 새로 만들어줘. 잠옷 상하의는 하늘색 셔츠로 파자마 셔츠와 같은 느낌으로 만들면돼."*

    python tools_re/costume_draft_claire.py            # costumes/202_claire/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_claire.py <dir>      # 다른 곳에 (미리보기 실험)
    python tools_re/costume_draft_claire.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다.** 이 스크립트는 첫 초안의 기록이다(`costumes/README.md`).

세 아이템 (클레르 = 주인공, `t/hero.tdf[0]`):
  투구 클레르의 머리      head_0..2   맨머리 hero/0/100 그대로 — 주인공 자신의 금발. 투구를 써도 머리칼이 보이게 하는 아바타
  갑옷 클레르의 잠옷 상의  body_2 · arm_0..10
                                     몸통 = 하늘색 파자마 셔츠(하얀 칼라 · 앞섶의 하얀 단추 셋) · 어깨 조각 0·1·3·4 = 빈 칸(맨몸처럼
                                     팔 끝이 어깨) · 팔 = 맨팔 hero/2/100 의 명암을 하늘색 소매로, 손목에 하얀 소맷단, 손은 맨손
  신발 클레르의 잠옷 바지  leg_0..10   로그 바지 hero/3/102 의 실루엣 = 하늘색 파자마 바지(허리 고무단 · 발목 하얀 단) + 맨발
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
import costume_sheets as cs  # noqa: E402
from costume_draft_lib import H  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "202_claire")
lib.refuse_overwrite(OUT, ["head", "body", "arm", "leg"])

# ── 하늘색 파자마 ─────────────────────────────────────────────────────────────
O = H("#1f3f7f")  # 외곽
D = H("#3f5f9f")  # 그림자
M = H("#5f9fdf")
B = H("#7fbfff")  # 바탕 하늘색
L = H("#9fdfff")
HI = H("#dfffff")
W = H("#ffffff")  # 칼라 · 단추 · 소맷단 (⛔ 팔레트 0 번이 아닌 칸 — write_part 가 1 번부터 놓는다)
# 피부 (맨팔 hero/2/100 · 맨머리의 것)
SO = H("#7f3f3f")
SD = H("#bf5f3f")
SM = H("#ff9f7f")
SL = H("#ffdfbf")

jar = cs.open_jar(cs.DEFAULT_ZIP)

# ── 투구: 클레르의 머리 = 맨머리 hero/0/100 그대로 ─────────────────────────────
bare = cs.shipped_sheet(jar, 0, 100)
lib.write_part(OUT, "head", [lib.Canvas.from_frame(fr, bare.palette) for fr in bare.frames])

# ── 갑옷: 몸통 — 7x5 하늘색 셔츠 (모든 방향이 이 한 장을 그린다) ───────────────────
KEY = {"O": O, "D": D, "M": M, "B": B, "L": L, "h": HI, "W": W, "j": SL, "s": SM}
SHIRT = [
    ".WWjWW.",
    "OLBhBMO",
    "OLBWBMO",
    "OLBhBDO",
    ".MBWMD.",
]
body = [lib.BLANK, lib.BLANK, lib.Canvas.from_art(SHIRT, KEY), lib.BLANK, lib.BLANK]
lib.write_part(OUT, "body", body)

# ── 갑옷: 팔 — 맨팔의 명암 → 하늘색 소매, 손(장갑 시트 hero/2/00 의 파란 자리)은 맨손, 손목 = 하얀 소맷단 ──
arm_base = cs.shipped_sheet(jar, 2, 100)
glove = cs.shipped_sheet(jar, 2, 0)
blue = {i for i, c in enumerate(glove.palette) if i and c[2] > c[0] + 20}
# hero/2/100: c 5f1f3f < a 7f3f3f < b bf5f3f < e bf7f3f < d ff9f7f < f ffdfbf
SLEEVE = {H("#5f1f3f"): O, H("#7f3f3f"): D, H("#bf5f3f"): M, H("#bf7f3f"): M, H("#ff9f7f"): B, H("#ffdfbf"): L}
arms = []
for k, fr in enumerate(arm_base.frames):
    rows = lib.rgb_rows(fr, arm_base.palette)
    hand = {(x, y) for y in range(fr.h) for x in range(fr.w) if fr.pixels[y][x] and glove.frames[k].pixels[y][x] in blue}
    cv = lib.Canvas(fr.w, fr.h)
    for y, row in enumerate(rows):
        for x, c in enumerate(row):
            if c is None:
                continue
            if (x, y) in hand:
                cv.put(x, y, c)
                continue
            cuff = any((x + dx, y + dy) in hand for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
            cv.put(x, y, W if cuff and c != H("#5f1f3f") else SLEEVE[c])
    arms.append(cv)
lib.write_part(OUT, "arm", arms)

# ── 신발: 잠옷 바지 — 로그 바지 hero/3/102 의 실루엣 ──────────────────────────────
# hero/3/102: a 1f1f3f · b 3f3f3f · c 7f1f3f · d 5f5f3f · e 7f7f3f · f bf7f3f · g 9f3f3f · h ff9f7f · i ffdfbf ·
#             j 9f9f7f · k df7f7f — 어두운 것부터 하늘색 램프로(맨살이던 정강이도 같은 천).
PANTS = {
    H("#1f1f3f"): O,
    H("#3f3f3f"): D,
    H("#7f1f3f"): D,
    H("#9f3f3f"): M,
    H("#5f5f3f"): M,
    H("#7f7f3f"): B,
    H("#bf7f3f"): B,
    H("#9f9f7f"): L,
    H("#df7f7f"): L,
    H("#ff9f7f"): L,
    H("#ffdfbf"): HI,
}
leg_base = cs.shipped_sheet(jar, 3, 102)
legs = []
for fr in leg_base.frames:
    rows = lib.rgb_rows(fr, leg_base.palette)
    w, h = fr.w, fr.h
    top = min(y for y in range(h) if any(rows[y]))
    bottom = {x: max((y for y in range(h) if rows[y][x] is not None), default=-1) for x in range(w)}
    cv = lib.Canvas(w, h)
    for y, row in enumerate(rows):
        for x, c in enumerate(row):
            if c is None:
                continue
            b = bottom[x]
            foot = y >= top + 7 and b >= h - 3  # 발: 프레임 아래쪽에 닿는 열의 맨 아래 두 칸
            if foot and y == b:
                cv.put(x, y, SO)
            elif foot and y == b - 1:
                cv.put(x, y, SM if c not in (H("#ffdfbf"), H("#ff9f7f")) else SL)
            elif foot and y == b - 2 and c != H("#1f1f3f"):
                cv.put(x, y, W)  # 발목의 하얀 단
            elif y == top and c != H("#1f1f3f"):
                cv.put(x, y, HI)  # 허리 고무단
            else:
                cv.put(x, y, PANTS[c])
    legs.append(cv)
lib.write_part(OUT, "leg", legs)

lib.write_text(
    OUT,
    """# 의상 202 — 「클레르의 잠옷」 (Claude 초안 2026-10-01, tools_re/costume_draft_claire.py)
# ★ 새 게임부터 누구나 가진 기본 아바타 (avatar_state::STARTER_SETS) — 조건 없이, 이미 시작한 캐릭터도 다음 불러오기부터.
set         = 클레르의 잠옷
head        = 클레르의 머리
head.desc   = 클레르 본래의 금발 머리
armor       = 클레르의 잠옷 상의
armor.desc  = 클레르가 입는 잠옷 셔츠
shoes       = 클레르의 잠옷 바지
shoes.desc  = 클레르가 입는 잠옷 바지
""",
)
