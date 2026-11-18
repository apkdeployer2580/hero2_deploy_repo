#!/usr/bin/env python3
"""의상 206 「제복」 초안 생성기 (2026-10-08) — 가디언 모드(4회차) 클리어 보상 (난이도 EASY 이상, avatar_state::CLEAR_REWARDS).

사용자(2026-10-08, Q9): *"가디언 이지 클리어 시 제복 아바타 획득하도록 할것 제복아바타는 스크린샷 참고"* — 스크린샷 = 적갈색 군복 상의
(금 단추 줄 · 금 견장 · 검은 허리띠) · 검은 챙 모자(붉은 띠) · 검은 바지 · 검은 장화 · 흰 장갑.

    python tools_re/costume_draft_uniform.py            # costumes/206_uniform/ 에 PNG (이미 있으면 거절)
    python tools_re/costume_draft_uniform.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다** (`costumes/README.md`). 바탕 실루엣은 배포본 시트 그대로(이안 의상 200 의 기계):
  머리  hero/0/100 맨머리 — 머리칼 영역을 모자로: 검정 챙모자 · 눈 바로 위 한 줄은 붉은 띠 · 금 모표 한 점
  몸통  프레임 2 = 손으로 그린 7×5(적갈 상의 · 가운데 금 단추 · 맨 아랫줄 검은 띠) · 0·1 = 천 갑옷 hero/1/05 어깨 조각 → 금 견장 · 3·4 빈 칸
  팔    hero/2/100 맨팔 — 손 = 흰 장갑 · 어깨 끝 = 금 견장 · 나머지 = 적갈 소매
  다리  hero/3/102 — 맨 윗줄 검은 띠 · 바지 검정/회색 · 열마다 맨 아래 두 줄 = 검은 장화
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
import costume_sheets as cs  # noqa: E402
from costume_draft_lib import H  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "206_uniform")
lib.refuse_overwrite(OUT, ["head", "body", "arm", "leg"])

jar = cs.open_jar(cs.DEFAULT_ZIP)

# ── 색 ──
K = H("#000000")  # outline
D1 = H("#1f1f1f")  # cap / boot black
D2 = H("#3f3f3f")  # black highlight
D3 = H("#5f5f5f")  # trousers highlight
C0 = H("#5f1f1f")  # coat outline
C1 = H("#7f1f1f")  # coat dark
C2 = H("#9f3f3f")  # coat
C3 = H("#bf5f3f")  # coat light
G0 = H("#7f5f1f")  # gold dark
G1 = H("#dfbf3f")  # gold
G2 = H("#ffdf7f")  # gold light
RB = H("#bf1f3f")  # cap band red
RB2 = H("#df3f3f")
W1 = H("#9f9f9f")  # glove shadow
W2 = H("#bfbfbf")  # glove
W3 = H("#dfdfdf")  # glove light
N = H("#1f1f3f")  # trousers navy-black


def m(**kw):
    return {H("#" + k[1:]): v for k, v in kw.items()}


def remap(frame, palette, region):
    rows = []
    for y, row in enumerate(frame.pixels):
        out = []
        for x, v in enumerate(row):
            if not v:
                out.append(None)
                continue
            src = palette[v]
            table = region(x, y)
            if src not in table:
                raise SystemExit(f"no mapping for #{src[0]:02x}{src[1]:02x}{src[2]:02x} at ({x},{y})")
            out.append(table[src])
        rows.append(out)
    return (frame.w, frame.h, rows)


# ── head: the bare head with a peaked cap over the hair ──
# hero/0/100: 1 #ffbf3f · 3 #bf7f3f · 4 #ffff7f · 5 #df9f3f · 6 #ffffff(hair shine) · 2 #7f3f3f (hair outline) = hair;
#             7 #9f3f3f · 8 #ff9f7f · 9 #df7f7f · a #ffdfbf = skin · b #1f1f3f eyes · c #5f3f7f.
HAIR = {H("#ffbf3f"), H("#bf7f3f"), H("#ffff7f"), H("#df9f3f"), H("#ffffff")}
HAIR_OUT = {H("#7f3f3f")}
LASH = H("#1f1f3f")
# ★ 2026-10-10 (사용자: *"제복모자에서 눈이 이상한데 눈 똑바로 그려줘"*): the first draft took every #ffffff for hair shine — the
# eyes' whites are the same #ffffff, so they went cap-black and only the irises were left; and it put the red band on
# the LOWEST hair row, which is the side locks by the chin, not above the eyes. Now the eye row decides: the band is
# the whole row just above the lashes (forehead included — the cap sits on the brow), the hair below it is black hair
# beside the face, and a white at or below the lashes is an eye white and stays.
bare = cs.shipped_sheet(jar, 0, 100)
heads = []
for k, fr in enumerate(bare.frames):
    cv = lib.Canvas.from_frame(fr, bare.palette)
    hair_rows = [y for y in range(fr.h) if any(cv.get(x, y) in HAIR for x in range(fr.w))]
    top_y = min(hair_rows) if hair_rows else 0
    lash_rows = [y for y in range(fr.h) if any(cv.get(x, y) == LASH for x in range(fr.w))]
    eye_y = min(lash_rows) if lash_rows else fr.h  # the back view has no eyes: all cap
    band_y = eye_y - 1
    band = [x for x in range(fr.w) if cv.get(x, band_y) is not None] if k != 0 and lash_rows else []
    for y in range(fr.h):
        for x in range(fr.w):
            c = cv.get(x, y)
            if c is None:
                continue
            if y == band_y and band:
                cv.put(x, y, K if x in (band[0], band[-1]) else (RB2 if (x + y) % 3 == 0 else RB))
            elif c == H("#ffffff") and y >= eye_y:
                continue  # an eye white
            elif c in HAIR:
                cv.put(x, y, D2 if y <= top_y + 1 else D1)  # the crown's top catches the light
            elif c in HAIR_OUT:
                cv.put(x, y, K)
    # the badge: one gold pixel on the front of the crown
    if k == 2:
        cv.put(fr.w // 2, top_y + 2, G1)
    if k == 1:
        cv.put(5, top_y + 2, G1)
    heads.append(cv)
lib.write_part(OUT, "head", heads)

# ── body ──
TORSO = [
    ".GCGCG.",
    "GCCGCCG",
    "KCCGCCK",
    "KCCGCCK",
    ".KKKKK.",
]
KEY = {".": None, "K": K, "C": C2, "G": G1}
cloth = cs.shipped_sheet(jar, 1, 5)
EPAULETTE = m(x000000=K, x1f1f3f=G0, x3f1f3f=G0, x3f3f3f=G0, x5f5f7f=G1, x7f7f7f=G1, xbfbfbf=G2)
body = []
for k in range(5):
    if k == 2:
        body.append((7, 5, [[KEY[ch] for ch in r] for r in TORSO]))
    elif k in (0, 1):
        body.append(remap(cloth.frames[k], cloth.palette, lambda x, y: EPAULETTE))
    else:
        body.append((1, 1, [[None]]))
lib.write_part(OUT, "body", body)

# ── arms: hand = white glove · shoulder end = gold epaulette · sleeve = coat ──
SLEEVE = m(x5f1f3f=C0, x7f3f3f=C1, xbf5f3f=C2, xbf7f3f=C2, xff9f7f=C3, xffdfbf=C3)
GLOVE = m(x5f1f3f=D2, x7f3f3f=W1, xbf5f3f=W2, xbf7f3f=W2, xff9f7f=W3, xffdfbf=W3)
SHOULDER = m(x5f1f3f=G0, x7f3f3f=G0, xbf5f3f=G1, xbf7f3f=G1, xff9f7f=G2, xffdfbf=G2)
arm_base = cs.shipped_sheet(jar, 2, 100)
glove = cs.shipped_sheet(jar, 2, 0)
blue = {i for i, c in enumerate(glove.palette) if i and c[2] > c[0] + 20}


def bfs(frame, seeds):
    d, q = {s: 0 for s in seeds}, list(seeds)
    while q:
        x, y = q.pop(0)
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < frame.w and 0 <= ny < frame.h and frame.pixels[ny][nx] and (nx, ny) not in d:
                d[(nx, ny)] = d[(x, y)] + 1
                q.append((nx, ny))
    return d


arms = []
for k, fr in enumerate(arm_base.frames):
    gl = {(x, y) for y in range(fr.h) for x in range(fr.w) if fr.pixels[y][x] and glove.frames[k].pixels[y][x] in blue}
    far = bfs(fr, gl) if gl else {}
    reach = max(far.values()) if far else 0

    def region(x, y, gl=gl, far=far, reach=reach):
        if (x, y) in gl:
            return GLOVE
        if not gl:
            return SHOULDER if y <= 1 else SLEEVE
        return SHOULDER if far.get((x, y), 0) >= max(3, reach - 1) else SLEEVE

    arms.append(remap(fr, arm_base.palette, region))
lib.write_part(OUT, "arm", arms)

# ── legs: belt · trousers · boots ──
BELT = m(x1f1f3f=K, x3f3f3f=K, x7f1f3f=K, x5f5f3f=D1, x7f7f3f=D1, xbf7f3f=G1, x9f3f3f=D1, xff9f7f=G1, xffdfbf=G1, x9f9f7f=G1, xdf7f7f=G1)
TROUSERS = m(x1f1f3f=K, x3f3f3f=N, x7f1f3f=N, x5f5f3f=D2, x7f7f3f=D2, xbf7f3f=D3, x9f3f3f=N, xff9f7f=D3, xffdfbf=D3, x9f9f7f=D3, xdf7f7f=D3)
BOOT = m(x1f1f3f=K, x3f3f3f=K, x7f1f3f=K, x5f5f3f=D1, x7f7f3f=D1, xbf7f3f=D2, x9f3f3f=D1, xff9f7f=D2, xffdfbf=D2, x9f9f7f=D2, xdf7f7f=D2)
leg_base = cs.shipped_sheet(jar, 3, 102)
legs = []
for k, fr in enumerate(leg_base.frames):
    top = min(y for y in range(fr.h) if any(fr.pixels[y]))
    bottom = {x: max((y for y in range(fr.h) if fr.pixels[y][x]), default=-1) for x in range(fr.w)}

    def region(x, y, top=top, bottom=bottom):
        if y == top:
            return BELT
        if y >= top + 4 and y >= bottom[x] - 2:
            return BOOT
        return TROUSERS

    legs.append(remap(fr, leg_base.palette, region))
lib.write_part(OUT, "leg", legs)

lib.write_text(
    OUT,
    """# 의상 206 — 「제복」 (Claude 초안 2026-10-08, tools_re/costume_draft_uniform.py) — 가디언 모드 클리어 보상 (난이도 EASY 이상,
# avatar_state::CLEAR_REWARDS). 적갈 군복 상의 · 금 견장과 단추 · 검은 챙모자(붉은 띠) · 검은 바지와 장화 · 흰 장갑.
set         = 제복
head        = 제복 모자
head.desc   = 붉은 띠의 검은 챙모자
armor       = 제복 상의
armor.desc  = 금 견장의 적갈색 군복
shoes       = 제복 바지
shoes.desc  = 검은 바지와 장화
""",
)
