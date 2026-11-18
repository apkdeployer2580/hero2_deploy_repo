#!/usr/bin/env python3
"""나이트의 새 스킬 「발도」 + 원형 검기 — 그림과 애니메이션 (2026-10-03).

사용자(2026-10-02/03): *"나이트의 스킬에 발도라는 스킬을 추가할꺼야. 스킬은 방어의지 밑에 생성하면되고, 스킬 아이콘은 스킬 이펙트와
비슷하게 동그란 원형모습 … 캐릭터 기준으로 둥그렇게 이펙트가 생기고 그 동그란 범위 내의 적에게 모두 데미지"* ·
*"검기의 이펙트를 발도처럼 둥그런 원형 검기가 날라가는 느낌으로 … 검기는 파란색+ 하늘색 느낌, 발도는 검은색 + 회색 느낌"* ·
신의가호 중 검기 = *"붉은 원형 검기"*. 범위·수치는 코드(`guest_anchors::CombatPatch` — 발도 36칸 디스크 · 검기 13/27칸).

── 자리 (HERO2_GUEST_RE.md §8.45) ───────────────────────────────────────────────────────────────────
* 발도 = `a.p[9]` = 나이트 7번째 액티브 = 「부활오라」(네트워크용, 시전 case 없음, 어느 스크립트도 안 연다, 게임이 스킬 창에서 숨김)의
  자리. 그래서 **그 자리의 것을 바꾼다**: 이름·설명 `t/sk1.tdf[12·13]`(`make_hero2_asset.SK1_REPLACE`) · 아이콘 `img/gmico` 프레임 1
  (나이트 띠 84×12)의 칸 6(x 72..83) · 동작 = `hero/hero1` 시퀀스 13(= kind 9 + 4, 배포본 4 프레임)을 **통째로 다시 그린다** — 동작
  길이 `a.n[9]` 는 시트의 프레임 수에서 나오므로(`cf.a()V`) 6 프레임이 되고, 타격은 코드가 `g.w == 3` 에서 한다(`IAIDO_HIT_FRAME`).
* 검기의 그림 = 시퀀스 21(파란 원형 검기, 4 프레임) · 22(붉은 원형 검기, 더 크게, 6 프레임) — 검기 페인터(`ak.b(II)V` 래퍼)가
  파동의 프레임 `cb.d` 를 그대로 프레임으로 넘기므로 **프레임 수 ≥ 수명(3 · 5) + 1**. 네 방향 목록은 같은 그림(원은 방향이 없다).
* 그림은 전부 나이트의 이펙트 시트 `hero/4/1` 끝에 덧붙인다 — `knight_blessing.py` 의 9 장(39..47) **뒤**(48..). 반전 없음.

── 그림 ──────────────────────────────────────────────────────────────────────────────────────────────
* 발도 고리(검정 + 회색) — ★ 2026-10-03 다시 그림(사용자: *"너무 원형인데 캐릭터 기준으로 살짝 타원형 … 원형이 캐릭터 기준으로
  눕혀진느낌 … 양옆끝은 얇은데 위아래는 살짝 두꺼워야하고 위쪽은 살짝 옅게 … 아래쪽은 짙고 두꺼운 느낌"*): 바닥에 눕힌 원 = 가로 112
  (타격 디스크 7칸) · 세로 78 의 타원을 **위로 세운 띠**로 그린다(`_band_ring`) — 각 열에서 타원 선 위로 띠 높이만큼 칠하므로 선이
  가로로 누운 위·아래는 두껍고 선이 세로로 선 양옆 끝은 저절로 가늘다. 앞쪽(아래) 띠 = 검정 심 + 짙은 회색 윗테 + 회색 바닥선, 최대
  9 px · 뒤쪽(위) 띠 = 회색 심 + 밝은 회색 윗테, 최대 5 px(옅고 얇게). 뒤쪽 반은 **몸 뒤**, 앞쪽 반은 **몸 앞**에 그려 캐릭터가 고리
  안에 선다(그림 둘: 앞 48..51 · 뒤 62..65). 프레임: 작게 열리는 앞쪽 호(f2) → 가득 찬 고리(f3 = 타격) → 얇아지는 고리(f4) → 끊어진
  잿빛 고리(f5). 고리 가운데 = 칸 가운데보다 4 px 아래(발밑).
* 방패 돌진의 바람(★ 2026-10-03, 사용자: *"크루세이더 착용시 방패치기가 너무 이펙트가 없는데 방패들고 앞으로 돌진하는듯한 … 바람이
  뒤로 지나가는 듯한 이펙트"*): 시퀀스 23 = 방패치기(시퀀스 10)의 여섯 프레임 그대로 + 바라보는 방향의 **반대쪽**에 가는 바람 줄
  (흰 심 · 옅은 하늘색 끝, 가로 그림 둘 · 세로 그림 둘 = 66..69) — 프레임마다 4 px 씩 뒤로 흘러가고 세 프레임마다 다른 무늬.
  장신구 블록의 채널이 서기·걷기를 이 시퀀스로 그린다(`AccessoryPatch::BASH_SEQ`); 평소의 방패치기(시퀀스 10)는 그대로.
  ★ 같은 날 다시(사용자: *"계속 시전하면서 걸어가고있는데 그냥 앞으로 방패를 꺼내놓고 앞으로 돌진하는듯해야해. 그리고 돌진하는
  방향쪽에도 이펙트 … 앞대각선 1칸쪽에 공기를 가르는듯한 이펙트 … 뒤쪽에 생기는 이펙트랑은 좀 다르게"*): 몸은 시전 동작을 돌리지
  않고 **방패를 내민 자세**(시퀀스 10 의 프레임 3·4 — 1 px 차이로 세 틱씩 번갈아 = 달리는 흔들림)로 고정. 앞쪽에는 **뱃머리 물결
  꼴의 굵은 호 둘**(흰 심 + 하늘색 테, 방패 앞에서 만나 두 앞대각선 칸으로 벌어진다 — 그림 70..77: 꼭짓점 자리 넷 × 무늬 둘),
  프레임마다 2 px 씩 뒤·바깥으로 밀리고 세 번째에 끊어져 흩어진다. 뒤쪽 바람 줄(가늘고 곧은 흰 줄)은 그대로.
* 검기 = 초승달(부메랑) — ★ 2026-10-03 다시(사용자: *"원이 날라가는게 아니야 … 부메랑같은 모양의 검기가 날라가야해"*, 레퍼런스 =
  날아가는 쪽으로 볼록한 굵은 초승달, 두 끝은 뒤로): `_crescent` = 바깥 원(반지름 R) 안이면서 날아가는 반대쪽으로 d 만큼 민 같은 원
  밖인 칸 — 볼록한 쪽이 비행 방향, 두 끝이 뒤를 향한다. 색 = 바깥 테 · 안쪽 테 · 속 · 가운데 띠(흰 광택). 방향 넷은 **미리 돌려서 네
  그림**(파트 4 의 반전 목록은 안 건드린다 — knight_blessing 과 같은 이유), 프레임은 작은 것(앞 두 프레임)·큰 것 둘을 돌려 쓴다.
  파랑(검기): R 7/d 4 · R 11/d 6, 파랑 #3f9fff 테 · 하늘 #9fdfff 속 · 흰 띠 // 빨강(신의가호 검기): R 10/6 · R 16/9, 빨강 · 연분홍 ·
  흰 띠 // 검정(신의가호 중 발도의 날아가는 참격, 시퀀스 24 = `CombatPatch::IAIDO_WAVE_SEQ`): R 12/7 · R 18/10, 밝은 회색 테 · 검정 속 ·
  짙은 회색 안테. 그림 24 장 = 52..75.
  ★ 2026-10-03 (2) 너비(사용자: *"검기가 너무 좁은 것 같은데 검기의 너비를 조금더 늘려줘 발도 … 와 검기 스킬의 검기 둘다"*): 원이 아니라
  **비행 직각 방향으로 `CRESCENT_SPAN`(1.4) 배 늘인 타원**의 초승달 — 가로 반지름 R · 세로 R × 1.4, 뒤로 d 민 같은 타원 밖. 두께(d)·테
  1 px 은 그대로이고(테·띠 판정은 타원 가장자리까지의 **거리**로 — 세로로 늘여도 테가 두꺼워지지 않는다) 두 끝만 더 멀리 벌어진다:
  파랑 큰 것 13×23 → 13×32 px · 빨강 20×33 → 20×46 · 검정 23×37 → 23×52. span 1.0 이면 전과 바이트 동일(셀프체크).
  ⛔ 시트 `hero/4/1` 은 16 비트 오프셋 한도가 가깝다(이 판 뒤 ≈ 61.3 / 65.5 KB) — 더 넓히려면 먼저 `preview` 가 아니라 바이트를 잴 것.

    python tools_re/knight_iaido.py preview <out.png> [zip]   # 시퀀스 13 · 21 · 22 를 네 방향으로 + 아이콘
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402
import knight_blessing as kb  # noqa: E402
import venom_effect  # noqa: E402

ANIM = kb.ANIM  # hero/hero1
SHEET = kb.SHEET  # hero/4/1
ICON_SHEET = "img/gmico"
EFFECT_PART = kb.EFFECT_PART
SEQS_BEFORE = kb.FLAME_SEQ + 1  # 21 after knight_blessing
FRAMES_BEFORE = kb.SHIPPED_FRAMES + len(kb.SOURCES)  # 48 after knight_blessing

# = guest_anchors::CombatPatch::{IAIDO_SEQ, IAIDO_FRAMES, IAIDO_HIT_FRAME, WAVE_RING_SEQ, BLESSED_WAVE_RING_SEQ, WAVE_LIFE, BLESSED_WAVE_LIFE}
IAIDO_SEQ, IAIDO_FRAMES, IAIDO_HIT = 13, 6, 3
WAVE_RING_SEQ, BLESSED_WAVE_RING_SEQ = 21, 22
BASH_SRC_SEQ, CHARGE_SEQ = 10, 23  # 방패치기의 동작 · 크루세이더 채널이 그리는 「방패 돌진」 (= AccessoryPatch::BASH_SEQ)
WAVE_LIFE, BLESSED_WAVE_LIFE = 3, 5
SWING_SEQ = 8  # 검기의 동작 — 발도의 몸 포즈를 여기서 빌린다
# 발도 6 프레임의 몸 포즈 = 시퀀스 8 의 프레임 (서기 · 준비 · 준비 · 휘두름(타격) · 마무리 · 마무리).
IAIDO_POSES = (0, 1, 3, 4, 5, 6)

# 새 그림 번호 (시트 48..): 발도 고리 앞쪽 반 넷 · 파란 고리 넷 · 붉은 고리 여섯 · 발도 고리 뒤쪽 반 넷.
IAIDO_RINGS = tuple(range(FRAMES_BEFORE, FRAMES_BEFORE + 4))
IAIDO_WAVE_SEQ = 24  # = CombatPatch::IAIDO_WAVE_SEQ — 신의가호 중 발도의 날아가는 참격
# 초승달 그림: 색 셋(파랑 · 빨강 · 검정) × 크기 둘(작은 · 큰) × 방향 넷(위 · 아래 · 왼 · 오른 = 애니메이션 목록 순서) = 24 장.
CRESCENT_COLOURS = ("blue", "red", "black")
CRESCENT_AT = FRAMES_BEFORE + 4
CRESCENT = {(c, size, d): CRESCENT_AT + 8 * ci + 4 * size + d for ci, c in enumerate(CRESCENT_COLOURS) for size in range(2) for d in range(4)}
IAIDO_FAR_RINGS = tuple(range(CRESCENT_AT + 24, CRESCENT_AT + 28))
WIND_H = (CRESCENT_AT + 28, CRESCENT_AT + 29)  # 가로 바람 줄 (왼·오른을 볼 때) — 무늬 둘
WIND_V = (CRESCENT_AT + 30, CRESCENT_AT + 31)  # 세로 바람 줄 (위·아래를 볼 때)
# 앞쪽에서 공기를 가르는 호: 그림 안에서 꼭짓점(두 호가 만나는 방패 앞)이 놓인 모서리 넷 × 무늬 둘(가득 · 흩어짐).
BOW_CORNERS = ("br", "tr", "bl", "tl")
BOW = {(v, c): CRESCENT_AT + 32 + 4 * v + k for v in range(2) for k, c in enumerate(BOW_CORNERS)}
# 방패를 내민 자세: 방패치기(시퀀스 10)의 프레임 — 여섯 틱에 4·4·4·3·3·3 (1 px 흔들림).
CHARGE_POSES = (4, 4, 4, 3, 3, 3)
RING_LIFT = -6  # 검기 고리 가운데 = 칸 가운데보다 6 px 위 (몸통) — knight_blessing.BLAST_LIFT
IAIDO_LIFT = 4  # 발도 고리(바닥에 눕힌 원)의 가운데 = 칸 가운데보다 4 px 아래 (발밑)
NO_EXTRA_FLAGS = kb.NO_EXTRA_FLAGS

BLACK, DARK, GREY, LIGHT, WHITE = (0, 0, 0), (0x3F, 0x3F, 0x3F), (0x7F, 0x7F, 0x7F), (0xBF, 0xBF, 0xBF), (0xFF, 0xFF, 0xFF)
BLUE, SKY = (0x3F, 0x9F, 0xFF), (0x9F, 0xDF, 0xFF)
PALE = (0xBF, 0xDF, 0xFF)  # 바람 줄의 끝
RED, PINK = (0xFF, 0x3F, 0x3F), (0xFF, 0xB7, 0xA7)


def _ellipse_ring(w, h, thickness, core, inner, outer, gaps=(), glint=None):
    """w×h 그림: 가운데 타원 고리. thickness px 두께 — 바깥 1 px = outer, 안쪽 1 px = inner, 사이 = core.
    gaps = 각도 구간(도) 목록을 비운다(끊어진 고리). glint = (각도, 길이) 흰 광택."""
    cx, cy = (w - 1) / 2.0, (h - 1) / 2.0
    rx, ry = (w - 1) / 2.0, (h - 1) / 2.0
    rows = []
    for y in range(h):
        line = []
        for x in range(w):
            dx, dy = (x - cx) / rx, (y - cy) / ry
            r = math.hypot(dx, dy)  # 1.0 = 바깥 가장자리
            depth = (1.0 - r) * min(rx, ry)  # 바깥에서 안으로 몇 px
            colour = None
            if 0 <= depth < thickness:
                ang = (math.degrees(math.atan2(dy, dx)) + 360.0) % 360.0
                if any(a <= ang < b for a, b in gaps):
                    colour = None
                elif depth < 1.0:
                    colour = outer
                elif depth >= thickness - 1.0:
                    colour = inner
                else:
                    colour = core
                if colour is not None and glint is not None:
                    g0, glen = glint
                    if (ang - g0) % 360.0 < glen and depth >= 1.0:
                        colour = WHITE
            line.append(colour)
        rows.append(line)
    return w, h, rows


def _band_ring(w, h, near_h, far_h, near, far, gaps=(), glint=None, tip=1.5):
    """바닥에 눕힌 w×h 타원을 **위로 세운 띠**로: 열 x 에서 타원의 아래 점(앞쪽) 위로 near_h·c, 위 점(뒤쪽) 위로 far_h·c 만큼
    칠한다(c = √(1−u²): 가운데 1 → 양옆 끝 0, 끝에서도 tip px 는 남는다). 그래서 위·아래는 두껍고 양옆 끝은 가늘다.
    near / far = (바닥선, 심, 윗테, 끝의 심) 색 · gaps = 비울 각도 구간(도; 0 = 오른쪽 · 90 = 아래(앞) · 270 = 위(뒤)) ·
    glint = (각도, 길이) 앞쪽 띠 가운데의 흰 광택 한 줄.
    돌려주는 것: (앞쪽 반, 뒤쪽 반) — 각각 (w, h, rows, (그림 안의 고리 가운데 x, y)), 칠해진 곳만 남긴 그림."""
    pad = int(math.ceil(far_h)) + 1  # 뒤쪽 띠는 타원 위로 솟는다
    height = h + pad
    cx, cy = (w - 1) / 2.0, pad + (h - 1) / 2.0
    rx, ry = (w - 1) / 2.0, (h - 1) / 2.0
    near_rows = [[None] * w for _ in range(height)]
    far_rows = [[None] * w for _ in range(height)]

    def gapped(ang):
        return any(a <= ang < b or a <= ang + 360.0 < b for a, b in gaps)

    for x in range(w):
        u = (x - cx) / rx
        if abs(u) > 1.0:
            continue
        c = math.sqrt(1.0 - u * u)
        for rows, base, top, colours, sign in ((near_rows, cy + ry * c, tip + (near_h - tip) * c, near, 1.0), (far_rows, cy - ry * c, tip + (far_h - tip) * c, far, -1.0)):
            ang = (math.degrees(math.atan2(sign * c, u)) + 360.0) % 360.0
            if gapped(ang):
                continue
            line, core, rim, tip_core = colours
            for y in range(height):
                d = base - y  # 바닥선에서 위로 몇 px
                if not (-0.5 <= d < top):
                    continue
                if d < 0.5:
                    colour = line
                elif d >= top - 1.0 and top >= 3.0:
                    colour = rim
                else:
                    colour = core if c >= 0.45 else tip_core
                if glint is not None and sign > 0 and top >= 5.0 and (ang - glint[0]) % 360.0 < glint[1] and abs(d - top * 0.55) < 0.5:
                    colour = WHITE
                rows[y][x] = colour
    centre = (int(round(cx)), int(round(cy)))
    return _cropped(near_rows, centre), _cropped(far_rows, centre)


def _cropped(rows, centre):
    """(w, h, rows, (가운데 x, y)) — 칠해진 곳만 남긴다(시트의 프레임 표는 16 비트 오프셋이고 이 시트는 넉넉하지 않다).
    아무것도 없으면 1×1 빈 그림."""
    ys = [y for y, row in enumerate(rows) if any(px is not None for px in row)]
    if not ys:
        return 1, 1, [[None]], (0, 0)
    xs = [x for row in rows for x, px in enumerate(row) if px is not None]
    x0, x1, y0, y1 = min(xs), max(xs), ys[0], ys[-1]
    return x1 - x0 + 1, y1 - y0 + 1, [row[x0 : x1 + 1] for row in rows[y0 : y1 + 1]], (centre[0] - x0, centre[1] - y0)


# 앞쪽(아래) 띠 = 짙고 두껍게 · 뒤쪽(위) 띠 = 옅고 얇게.  (바닥선, 심, 윗테, 끝의 심)
_NEAR_FULL, _FAR_FULL = (GREY, BLACK, DARK, DARK), (DARK, GREY, LIGHT, GREY)
_NEAR_FADE, _FAR_FADE = (GREY, DARK, GREY, GREY), (GREY, LIGHT, LIGHT, LIGHT)
_NEAR_ASH, _FAR_ASH = (LIGHT, GREY, LIGHT, LIGHT), (LIGHT, LIGHT, LIGHT, LIGHT)


def _iaido_bands():
    """[(앞쪽 반, 뒤쪽 반)] 넷 — 열리는 앞쪽 호 · 가득 찬 고리(타격) · 얇아지는 고리 · 끊어진 잿빛 고리."""
    return [
        _band_ring(64, 44, 5, 3, _NEAR_FULL, _FAR_FULL, gaps=((150.0, 400.0),), glint=(70.0, 40.0)),
        _band_ring(112, 78, 9, 5, _NEAR_FULL, _FAR_FULL, glint=(100.0, 45.0)),
        _band_ring(116, 82, 6, 3, _NEAR_FADE, _FAR_FADE, gaps=((60.0, 72.0), (238.0, 256.0)), glint=(110.0, 30.0)),
        _band_ring(118, 84, 3, 2, _NEAR_ASH, _FAR_ASH, gaps=((20.0, 50.0), (118.0, 150.0), (205.0, 250.0), (300.0, 335.0))),
    ]


def iaido_ring_frames():
    """[(w, h, rows)] 넷 — 발도 고리의 앞쪽(아래) 반: 몸 **앞**에 그린다."""
    return [near[:3] for near, _far in _iaido_bands()]


def iaido_far_frames():
    """[(w, h, rows)] 넷 — 발도 고리의 뒤쪽(위) 반: 몸 **뒤**에 그린다. 첫 프레임(열리는 호)은 앞쪽뿐이라 1×1 빈 그림."""
    return [far[:3] for _near, far in _iaido_bands()]


def iaido_centres():
    """{그림 번호: 그림 안의 고리 가운데 (x, y)} — 앞쪽·뒤쪽 반 둘 다."""
    out = {}
    for k, (near, far) in enumerate(_iaido_bands()):
        out[IAIDO_RINGS[k]] = near[3]
        out[IAIDO_FAR_RINGS[k]] = far[3]
    return out


# ── ★★ 2026-10-17 발도 특화 아이템 — 반경 4 · 5 의 고리와 초승달 ────────────────────────────────────────────────────────────
# 사용자: *"발도의 범위를 늘려주는 아이템"* (아이템 하나마다 반경 +1칸) · *"발도 고리에 맞춰서 키워지고 날라가는 초승달 모양 발도의 검기도
# 크기에 맞게 폭이 늘어나야해.(입히는 데미지폭과 이펙트 모두)"*. 판정 = `CombatPatch::iaido_tiles`(반경 R: |dx|,|dy| ≤ R · |dx|+|dy| ≤ R+1;
# 신의가호 중엔 앞 5칸 · 폭 2R−3 칸의 띠 = 날아가는 초승달). 그림:
# * 고리 = 지금 고리(가로 7칸 = 112 px)를 (2R+1)/7 배 — 가로가 판정의 칸 수와 같다(9 · 11칸) · 띠 높이 +1 px/칸. 시퀀스 54(R4) · 55(R5) =
#   다 만든 시퀀스 13(망토 · 다리까지 입은 것)의 사본에서 고리 그림만 바꾼 것 — 애니메이션 맨 끝(악마 사본 뒤)에 붙여 다른 번호가 안 밀린다.
# * 초승달 = 검정 초승달을 띠 폭(3 · 5 · 7칸)만큼 세로로 늘인 것(큰 것의 높이 ≈ 칸 × 16 px). 시퀀스 56(R4) · 57(R5).
# 그림 32 장 = 시트 92..123(번호 한도 127) · 압축 IDAT(`compress_from=REACH_AT` — 반전 목록에 안 든다).
REACH_RADII = (4, 5)
REACH_SEQ = 54  # = CombatPatch::IAIDO_REACH_SEQ (반경 4; 반경 5 = +1)
REACH_WAVE_SEQ = 56  # = CombatPatch::IAIDO_REACH_WAVE_SEQ
REACH_SEQS = 2 * len(REACH_RADII)
REACH_AT = FRAMES_BEFORE + 4 + 24 + 4 + 4 + 8  # 92 — new_frames() 뒤
REACH_PER = 16  # 반경 하나: 앞쪽 반 넷 · 뒤쪽 반 넷 · 초승달 작은 것 넷 · 큰 것 넷
# (r, d, span) 작은 것 · 큰 것 — 지금(반경 3)은 (12, 7, 1.4) · (18, 10, 1.4): 큰 것 높이 51 px ≈ 3칸
REACH_CRESCENT = {4: ((14, 8, 2.0), (20, 11, 2.0)), 5: ((15, 8, 2.55), (22, 12, 2.55))}


def _reach_bands(radius):
    """반경 radius 의 고리 넷 — `_iaido_bands` 를 (2R+1)/7 배."""
    f = (2 * radius + 1) / 7.0
    up = radius - 3

    def sz(w, h):
        return int(round(w * f)) // 2 * 2, int(round(h * f)) // 2 * 2

    return [
        _band_ring(*sz(64, 44), 5 + up, 3 + up, _NEAR_FULL, _FAR_FULL, gaps=((150.0, 400.0),), glint=(70.0, 40.0)),
        _band_ring(*sz(112, 78), 9 + up, 5 + up, _NEAR_FULL, _FAR_FULL, glint=(100.0, 45.0)),
        _band_ring(*sz(116, 82), 6 + up, 3 + up, _NEAR_FADE, _FAR_FADE, gaps=((60.0, 72.0), (238.0, 256.0)), glint=(110.0, 30.0)),
        _band_ring(*sz(118, 84), 3 + up, 2 + up, _NEAR_ASH, _FAR_ASH, gaps=((20.0, 50.0), (118.0, 150.0), (205.0, 250.0), (300.0, 335.0))),
    ]


def reach_near(radius, k):
    return REACH_AT + REACH_PER * REACH_RADII.index(radius) + k


def reach_far(radius, k):
    return reach_near(radius, 4 + k)


def reach_crescent(radius, size, d):
    return reach_near(radius, 8 + 4 * size + d)


def reach_frames():
    """[(w, h, rows)] 32 — 반경 4 · 5 마다 앞쪽 반 넷 · 뒤쪽 반 넷 · 초승달(작은 · 큰 × 방향 넷)."""
    out = []
    style = _CRESCENT_STYLE["black"][0]
    for radius in REACH_RADII:
        bands = _reach_bands(radius)
        out += [near[:3] for near, _far in bands]
        out += [far[:3] for _near, far in bands]
        for r, d, span in REACH_CRESCENT[radius]:
            base = _crescent_right(r, d, style, span)
            out += [_turned(base, direction) for direction in range(4)]
    assert len(out) == REACH_PER * len(REACH_RADII)
    return out


def reach_centres():
    """{그림 번호: 고리 가운데 (x, y)} — 반경 4 · 5 의 앞쪽·뒤쪽 반."""
    out = {}
    for radius in REACH_RADII:
        for k, (near, far) in enumerate(_reach_bands(radius)):
            out[reach_near(radius, k)] = near[3]
            out[reach_far(radius, k)] = far[3]
    return out


def patch_reach_anim(blob, sheet_sizes):
    """`hero/hero1` — 이 파이프라인의 다른 패치 전부 뒤(악마 사본까지 54 시퀀스) — + 시퀀스 54 · 55(반경 4 · 5 의 발도 = 시퀀스 13 의
    사본, 고리만 큰 것) · 56 · 57(반경 4 · 5 의 신의가호 초승달)."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != REACH_SEQ:
        raise ValueError(f"{ANIM}: {len(seqs)} sequences — expected {REACH_SEQ} (every other animation patch first, the demon copies included)")
    centres = reach_centres()
    for radius in REACH_RADII:
        remap = {IAIDO_RINGS[k]: reach_near(radius, k) for k in range(4)}
        remap.update({IAIDO_FAR_RINGS[k]: reach_far(radius, k) for k in range(4)})
        seq = []
        for frame in seqs[IAIDO_SEQ]:
            dirs = []
            for sprites in frame:
                out = []
                for s in sprites:
                    if s[0] & 0x7F == EFFECT_PART and s[1] in remap:
                        nf = remap[s[1]]
                        if sheet_sizes[nf] == (1, 1):
                            continue
                        ax, ay = centres[nf]
                        out.append(_sprite(nf, -ax, -ay + IAIDO_LIFT))
                    else:
                        out.append(list(s))
                dirs.append(out)
            seq.append(dirs)
        seqs.append(seq)
    for radius in REACH_RADII:
        seqs.append([[[_centred(reach_crescent(radius, 0 if f < 2 else 1, d), sheet_sizes)] for d in range(4)] for f in range(BLESSED_WAVE_LIFE + 1)])
    out = venom_effect.encode(header, parts, seqs)
    _h, _p, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[:REACH_SEQ] == before, "read back: the sequences before moved"
    assert len(again) == REACH_SEQ + REACH_SEQS and REACH_WAVE_SEQ == REACH_SEQ + len(REACH_RADII)
    for k in range(len(REACH_RADII)):
        assert len(again[REACH_SEQ + k]) == IAIDO_FRAMES, "발도 = six frames (a.n[9] stays the sheet's)"
        assert len(again[REACH_WAVE_SEQ + k]) == BLESSED_WAVE_LIFE + 1, "the crescent flies five tiles"
        rings = {s[1] for fr in again[REACH_SEQ + k] for d in fr for s in d if s[0] & 0x7F == EFFECT_PART}
        assert rings and all(reach_near(REACH_RADII[k], 0) <= f < reach_near(REACH_RADII[k], 8) for f in rings), f"the radius {REACH_RADII[k]} rings only: {rings}"
    return out


# (바깥 테, 안쪽 테, 속, 가운데 띠) · (R, d) 작은 것 · 큰 것
_CRESCENT_STYLE = {
    "blue": ((BLUE, BLUE, SKY, WHITE), ((7, 4), (11, 6))),
    "red": ((RED, RED, PINK, WHITE), ((10, 6), (16, 9))),
    "black": ((LIGHT, DARK, BLACK, DARK), ((12, 7), (18, 10))),
}
# 비행 직각 방향의 반지름 배율 — 초승달의 「너비」(두 끝 사이). 1.0 = 원(2026-10-03 첫 판) · ★ 1.4 = 같은 날 사용자 재지시(*"검기가 너무
# 좁은 것 같은데 … 너비를 조금더 늘려줘"*). 두께 d 와 테 1 px 은 이 값과 무관하다.
CRESCENT_SPAN = 1.4


def _ellipse_edge(dx, dy, rx, ry):
    """타원(반지름 rx · ry, 가운데 원점) 가장자리까지의 부호 있는 거리(px, 안쪽이 양수) — `(1 − ρ) / |∇ρ|`.
    rx == ry 면 정확히 `rx − hypot(dx, dy)`(옛 원 판정 — 그 식을 그대로 써서 비트 동일; 나눗셈 꼴은 경계 픽셀에서 1e-15 가 어긋난다)이고,
    타원에서도 가로·세로 축 위에서는 정확하다."""
    if rx == ry:
        return rx - math.hypot(dx, dy)
    rho = math.hypot(dx / rx, dy / ry)
    if rho < 1e-9:
        return min(rx, ry)
    grad = math.hypot(dx / (rx * rx), dy / (ry * ry)) / rho
    return (1.0 - rho) / grad


def _crescent_right(r, d, colours, span=CRESCENT_SPAN):
    """볼록한 쪽이 오른쪽인 초승달 (w, h, rows): 바깥 타원(가로 반지름 r · 세로 r × span, 가운데 (cx, cy)) 안 · 왼쪽으로 d 민 같은
    타원 밖. 테·띠는 타원 가장자리까지의 거리(px)로 판정하므로 span 이 커져도 테는 1 px, 두께는 d 그대로이고 두 끝만 멀어진다."""
    outer, inner_rim, fill, core = colours
    ry = r * span
    size_x, size_y = 2 * r + 3, int(math.ceil(2 * ry)) + 3
    cx, cy = r + 1.0, ry + 1.0
    rows = [[None] * size_x for _ in range(size_y)]
    for y in range(size_y):
        for x in range(size_x):
            d_out = _ellipse_edge(x - cx, y - cy, r, ry)  # 바깥 타원 가장자리에서 안으로
            d_in = -_ellipse_edge(x - (cx - d), y - cy, r, ry)  # 안쪽 타원 가장자리에서 밖으로
            if d_out < 0.0 or d_in < 0.0:
                continue
            thick = d_out + d_in
            if d_out < 1.0:
                colour = outer
            elif d_in < 1.0:
                colour = inner_rim
            elif thick >= 3.5 and abs(d_out - d_in) < 0.9:
                colour = core
            else:
                colour = fill
            rows[y][x] = colour
    return _cropped(rows, (0, 0))[:3]


def _turned(frame, direction):
    """오른쪽을 향한 초승달을 방향(0 위 · 1 아래 · 2 왼 · 3 오른)으로 돌린다."""
    w, h, rows = frame
    if direction == 3:
        return frame
    if direction == 2:
        return w, h, [list(reversed(row)) for row in rows]
    if direction == 0:  # 90° 반시계: 오른쪽 열이 맨 윗줄로
        return h, w, [[rows[j][w - 1 - i] for j in range(h)] for i in range(w)]
    return h, w, [[rows[h - 1 - j][i] for j in range(h)] for i in range(w)]  # 아래: 90° 시계


def crescent_frames():
    """[(w, h, rows)] 24 — CRESCENT 의 번호 순서(색 · 크기 · 방향)."""
    out = []
    for colour in CRESCENT_COLOURS:
        style, sizes = _CRESCENT_STYLE[colour]
        for r, d in sizes:
            base = _crescent_right(r, d, style)
            for direction in range(4):
                out.append(_turned(base, direction))
    return out


# 바람 줄: (줄의 자리, 시작, 끝) — 가로 그림 18×11. 세로 그림은 그것을 눕힌 것.
WIND_LEN, WIND_SPAN = 18, 11
_WIND_LINES = (
    ((1, 3, 14), (5, 0, 10), (9, 6, 17)),
    ((0, 7, 17), (3, 1, 9), (7, 4, 15), (10, 0, 6)),
)


def wind_frames():
    """[(w, h, rows)] 넷 — 가로 무늬 둘 · 세로 무늬 둘. 줄은 1 px, 가운데 흰색 · 양 끝 2 px 옅은 하늘색(좌우 대칭이라 왼·오른 어느
    쪽 뒤에 놓아도 같다)."""
    horiz = []
    for lines in _WIND_LINES:
        rows = [[None] * WIND_LEN for _ in range(WIND_SPAN)]
        for y, x0, x1 in lines:
            for x in range(x0, x1 + 1):
                rows[y][x] = PALE if x - x0 < 2 or x1 - x < 2 else WHITE
        horiz.append((WIND_LEN, WIND_SPAN, rows))
    vert = [(h, w, [[rows[x][y] for x in range(h)] for y in range(w)]) for w, h, rows in horiz]
    return horiz + vert


BOW_W, BOW_H = 13, 16


def _bow_base(variant):
    """꼭짓점이 오른쪽 아래 모서리인 호 하나(13×16): 꼭짓점에서 왼쪽 위로 휘어 올라가며 가늘어진다. 흰 심에 하늘색 테.
    variant 1 = 흩어지는 무늬(가늘고 군데군데 끊긴다)."""
    rows = [[None] * BOW_W for _ in range(BOW_H)]
    p0, p1, p2 = (BOW_W - 2.0, BOW_H - 2.0), (BOW_W - 2.5, 5.0), (1.0, 1.0)  # 꼭짓점 · 휘는 점(앞으로 볼록) · 꼬리
    steps = 80
    for i in range(steps + 1):
        t = i / steps
        if variant == 1 and int(t * 6) % 2 == 1:
            continue  # 끊긴 무늬
        x = (1 - t) ** 2 * p0[0] + 2 * t * (1 - t) * p1[0] + t * t * p2[0]
        y = (1 - t) ** 2 * p0[1] + 2 * t * (1 - t) * p1[1] + t * t * p2[1]
        radius = (1.6 if variant == 0 else 1.0) * (1.0 - 0.75 * t) + 0.35  # 꼭짓점은 굵게, 꼬리는 가늘게
        for yy in range(BOW_H):
            for xx in range(BOW_W):
                d = math.hypot(xx - x, yy - y)
                if d <= radius:
                    core = d <= radius - 0.9
                    if core:
                        rows[yy][xx] = WHITE
                    elif rows[yy][xx] is None:
                        rows[yy][xx] = SKY
    return rows


def bow_frames():
    """[(w, h, rows)] 여덟 — 무늬 둘 × 꼭짓점 모서리 넷(br · tr · bl · tl; 기본 그림을 뒤집은 것)."""
    out = []
    for variant in range(2):
        base = _bow_base(variant)
        for corner in BOW_CORNERS:
            rows = base if corner[0] == "b" else base[::-1]
            rows = [list(r) if corner[1] == "r" else list(r[::-1]) for r in rows]
            out.append((BOW_W, BOW_H, rows))
    return out


def new_frames():
    return iaido_ring_frames() + crescent_frames() + iaido_far_frames() + wind_frames() + bow_frames()


# 몸의 가운데(칸 가운데 기준) — 바람 줄을 여기에 맞춘다.
_BODY_X, _BODY_Y = -2, -14


def _wind_sprite(direction, frame):
    """방향(0 위 · 1 아래 · 2 왼 · 3 오른 = facing − 1)의 **반대쪽**에 놓인 바람 줄 하나. 프레임마다 4 px 씩 뒤로 흘러가고(셋이 한
    바퀴) 세 프레임마다 무늬가 바뀐다."""
    flow, pattern = 4 * (frame % 3), (frame // 3) % 2
    if direction == 0:  # 위를 본다: 발 아래로
        return _sprite(WIND_V[pattern], _BODY_X - WIND_SPAN // 2, -2 + flow)
    if direction == 1:  # 아래를 본다: 머리 위로
        return _sprite(WIND_V[pattern], _BODY_X - WIND_SPAN // 2, -27 - flow - WIND_LEN)
    if direction == 2:  # 왼쪽을 본다: 오른쪽으로
        return _sprite(WIND_H[pattern], 10 + flow, _BODY_Y - WIND_SPAN // 2)
    return _sprite(WIND_H[pattern], -14 - flow - WIND_LEN, _BODY_Y - WIND_SPAN // 2)  # 오른쪽을 본다: 왼쪽으로


def _bow_sprites(direction, frame):
    """돌진하는 방향 앞에서 공기를 가르는 호 둘 — 방패 앞에서 만나 두 앞대각선 칸으로 벌어진다(뱃머리 물결 꼴).
    세 프레임이 한 바퀴: 2 px 씩 뒤·바깥으로 밀리고, 세 번째는 흩어지는 무늬."""
    phase = frame % 3
    variant, slide = (1 if phase == 2 else 0), 2 * phase

    def arm(corner, ax, ay, sx, sy):
        # 그림의 꼭짓점 모서리(안쪽 1 px)를 (ax, ay) 에 놓고 (sx, sy) 쪽으로 slide 만큼 민다.
        cx = BOW_W - 2 if corner[1] == "r" else 1
        cy = BOW_H - 2 if corner[0] == "b" else 1
        return _sprite(BOW[(variant, corner)], ax - cx + sx * slide, ay - cy + sy * slide)

    if direction == 3:  # 오른쪽으로 돌진: 「>」 — 위 호는 왼쪽 위로, 아래 호는 왼쪽 아래로 벌어진다
        return [arm("br", 25, _BODY_Y - 2, -1, -1), arm("tr", 25, _BODY_Y + 2, -1, 1)]
    if direction == 2:  # 왼쪽: 「<」
        return [arm("bl", -29, _BODY_Y - 2, 1, -1), arm("tl", -29, _BODY_Y + 2, 1, 1)]
    if direction == 0:  # 위: 「^」
        return [arm("tr", _BODY_X - 2, -40, -1, 1), arm("tl", _BODY_X + 2, -40, 1, 1)]
    return [arm("br", _BODY_X - 2, 15, -1, -1), arm("bl", _BODY_X + 2, 15, 1, -1)]  # 아래: 「v」


def _sprite(frame, x, y):
    if not (-128 <= x <= 127 and -128 <= y <= 127):
        raise ValueError(f"effect {frame} at ({x},{y}) does not fit a signed byte")
    return [EFFECT_PART, frame, x & 0xFF, y & 0xFF]


def _centred(frame, sizes, lift=RING_LIFT):
    w, h = sizes[frame]
    return _sprite(frame, -(w // 2), -(h // 2) + lift)


def _iaido_sprite(frame, sizes):
    """발도 고리의 한 반쪽 — 그림 안의 고리 가운데를 칸 가운데 + IAIDO_LIFT 에. 빈 그림(1×1)은 None."""
    if sizes[frame] == (1, 1):
        return None
    ax, ay = iaido_centres()[frame]
    return _sprite(frame, -ax, -ay + IAIDO_LIFT)


def patch_anim(blob, sheet_sizes):
    """`hero/hero1`(knight_blessing 뒤 = 21 시퀀스) → 시퀀스 13 을 발도 6 프레임으로 다시 그리고 21 · 22 를 덧붙인다."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != SEQS_BEFORE or len(seqs[IAIDO_SEQ]) != 4 or len(seqs[SWING_SEQ]) != 7:
        raise ValueError(f"{ANIM}: {len(seqs)} sequences / 13 has {len(seqs[IAIDO_SEQ])} frames — expected knight_blessing's output (21, 4, 7)")
    iaido = []
    for f, src in enumerate(IAIDO_POSES):
        k = f - (IAIDO_HIT - 1)  # 0..3 = 열리는 호 · 타격 · 얇아짐 · 잿빛
        under, over = [], []
        if 0 <= k < len(IAIDO_RINGS):
            far, near = _iaido_sprite(IAIDO_FAR_RINGS[k], sheet_sizes), _iaido_sprite(IAIDO_RINGS[k], sheet_sizes)
            under = [far] if far is not None else []
            over = [near] if near is not None else []
        directions = []
        for sprites in seqs[SWING_SEQ][src]:
            body = [list(s) for s in sprites if s[0] & 0x7F != EFFECT_PART]  # 검기 자신의 섬광은 뺀다
            directions.append(under + body + over)  # 뒤쪽 반은 몸 뒤 · 앞쪽 반은 몸 앞 — 캐릭터가 고리 안에 선다
        iaido.append(directions)
    seqs[IAIDO_SEQ] = iaido
    # 날아가는 그림 셋 — 프레임 = 날아간 칸(cb.d): 앞 두 프레임은 작은 초승달, 그 뒤는 큰 것; 목록 = 방향(위 · 아래 · 왼 · 오른).
    def flying(colour, frames):
        return [[[_centred(CRESCENT[(colour, 0 if f < 2 else 1, d)], sheet_sizes)] for d in range(4)] for f in range(frames)]

    blue, red, black = flying("blue", WAVE_LIFE + 1), flying("red", BLESSED_WAVE_LIFE + 1), flying("black", BLESSED_WAVE_LIFE + 1)
    if len(iaido) != IAIDO_FRAMES or len(blue) < WAVE_LIFE + 1 or len(red) < BLESSED_WAVE_LIFE + 1 or len(black) < BLESSED_WAVE_LIFE + 1:
        raise AssertionError("frame counts: 발도 6 · the crescents at least the wave's life + 1")
    seqs.append(blue)
    seqs.append(red)
    # 방패 돌진 (시퀀스 23): 방패치기의 프레임 그대로 + 뒤로 흐르는 바람 — 채널의 동작 프레임은 `a.n[6]`(= 방패치기의 프레임 수)로
    # 돌므로 프레임 수가 같아야 한다.
    # 몸은 시전 동작을 돌리지 않는다 — 방패를 내민 자세(CHARGE_POSES)로 고정; 뒤에는 바람 줄, 앞에는 공기를 가르는 호 둘.
    if len(CHARGE_POSES) != len(seqs[BASH_SRC_SEQ]):
        raise AssertionError("방패 돌진: 자세 표의 길이 = 방패치기의 프레임 수")
    charge = [
        [[_wind_sprite(d, f)] + [list(sp) for sp in seqs[BASH_SRC_SEQ][pose][d]] + _bow_sprites(d, f) for d in range(4)]
        for f, pose in enumerate(CHARGE_POSES)
    ]
    if len(charge) != len(seqs[BASH_SRC_SEQ]) or any(len(fr) != 4 for fr in charge):
        raise AssertionError("방패 돌진: 방패치기와 같은 프레임 수 · 네 방향")
    seqs.append(charge)
    seqs.append(black)  # 24: 신의가호 중 발도의 날아가는 참격
    out = venom_effect.encode(header, parts, seqs)
    _h, _p, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[:IAIDO_SEQ] == before[:IAIDO_SEQ] and again[IAIDO_SEQ + 1 : SEQS_BEFORE] == before[IAIDO_SEQ + 1 : SEQS_BEFORE], "read back"
    assert len(again) == IAIDO_WAVE_SEQ + 1 and len(again[IAIDO_SEQ]) == IAIDO_FRAMES
    assert len(again[IAIDO_WAVE_SEQ]) == BLESSED_WAVE_LIFE + 1
    assert len(again[CHARGE_SEQ]) == len(before[BASH_SRC_SEQ]), "방패 돌진 = 방패치기의 프레임 수"
    return out


def patch_sheet(mph, mpd):
    """(mph, mpd) of `hero/4/1` AFTER knight_blessing (48 frames) + 발도's ring halves (48..51 · 76..79), the 24
    crescents (52..75), the four wind streaks (80..83) and the eight bow arcs (84..91)."""
    sheet = cs.sheet_decode(mph, mpd, SHEET)
    if len(sheet.frames) != FRAMES_BEFORE:
        raise ValueError(f"{SHEET}: {len(sheet.frames)} frames — expected knight_blessing's {FRAMES_BEFORE}")
    palette = list(sheet.palette)
    index = {}
    for i, rgb in enumerate(palette):
        if i:  # 0 = 투명 — 진짜 흰색은 0 이 아닌 칸에
            index.setdefault(tuple(rgb), i)
    frames = list(sheet.frames)
    assert REACH_AT == FRAMES_BEFORE + len(new_frames())
    for w, h, rows in new_frames() + reach_frames():  # ★★ 2026-10-17 + the radius 4 · 5 rings and crescents (92..123)
        px = []
        for row in rows:
            line = []
            for rgb in row:
                if rgb is None:
                    line.append(0)
                    continue
                if rgb not in index:
                    index[rgb] = len(palette)
                    palette.append(rgb)
                line.append(index[rgb])
            px.append(line)
        frames.append(cs.Frame(w, h, px, flags=NO_EXTRA_FLAGS))
    if len(palette) > 256:
        raise ValueError(f"{SHEET}: {len(palette)} colours")
    if len(frames) > 127:
        raise ValueError(f"{SHEET}: {len(frames)} frames — a sequence names a frame by a signed byte")
    out = cs.Sheet(palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out, compress_from=REACH_AT)  # ★★ 2026-10-17 the big rings: zlib (never in a flip list)
    back = cs.sheet_decode(new_mph, new_mpd, SHEET, compressed_from=REACH_AT)
    assert [(f.w, f.h, f.pixels) for f in back.frames[:FRAMES_BEFORE]] == [(f.w, f.h, f.pixels) for f in sheet.frames], "old frames moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    return new_mph, new_mpd


def sheet_sizes_after(sizes_before):
    """knight_blessing.sheet_sizes_after(jar) + 새 그림 넷·넷·여섯·넷."""
    sizes = dict(sizes_before)
    for k, (w, h, _rows) in enumerate(new_frames() + reach_frames()):
        sizes[FRAMES_BEFORE + k] = (w, h)
    return sizes


# ── 아이콘: img/gmico 프레임 1 (나이트 띠) 칸 6 ─────────────────────────────────────────────────────
ICON_STRIP, ICON_CELL, ICON = 1, 6, 12
ICON_BG = (0x5F, 0x5F, 0x5F)  # 회색 바탕 — 스킬 창의 검은 칸 위에서 검은 고리가 보이게 (첫 판 #1f1f1f 는 칸과 구별이 안 됐다)


def icon_pixels():
    """12×12 — 회색 바탕에 검정 심·밝은 회색 테의 고리(발도의 고리 그대로), 왼 위 흰 광택, 오른 아래로 빠지는 참격 꼬리."""
    rows = []
    cx = cy = 5.5
    for y in range(ICON):
        line = []
        for x in range(ICON):
            r = math.hypot(x - cx, y - cy)
            if 3.0 <= r < 3.9:
                line.append(DARK)
            elif 3.9 <= r < 5.6:
                line.append(LIGHT if r >= 5.0 else BLACK)
            else:
                line.append(ICON_BG)
        rows.append(line)
    for x, y in ((3, 2), (2, 3), (3, 3)):
        rows[y][x] = WHITE
    for x, y in ((8, 9), (9, 10), (10, 11), (9, 9), (10, 10)):
        rows[y][x] = BLACK if (x + y) % 2 else WHITE
    return rows


def patch_icon(mph, mpd):
    """(mph, mpd) of `img/gmico` with the knight strip's cell 6 (부활오라) redrawn as 발도. 반전 없는 시트
    (head 0x08000000)라 `sheet_encode(any_head=True)`; 다른 프레임·팔레트 칸은 그대로(읽어 되돌려 확인)."""
    sheet = cs.sheet_decode(mph, mpd, ICON_SHEET)
    strip = sheet.frames[ICON_STRIP]
    if (strip.w, strip.h) != (7 * ICON, ICON):
        raise ValueError(f"{ICON_SHEET} frame {ICON_STRIP}: {strip.w}×{strip.h} — expected the knight's seven cells")
    palette = list(sheet.palette)
    index = {}
    for i, rgb in enumerate(palette):
        if i:
            index.setdefault(tuple(rgb), i)
    pixels = [list(row) for row in strip.pixels]
    for y, row in enumerate(icon_pixels()):
        for x, rgb in enumerate(row):
            if rgb not in index:
                index[rgb] = len(palette)
                palette.append(rgb)
            pixels[y][ICON * ICON_CELL + x] = index[rgb]
    if len(palette) > 256:
        raise ValueError(f"{ICON_SHEET}: {len(palette)} colours")
    frames = list(sheet.frames)
    frames[ICON_STRIP] = cs.Frame(strip.w, strip.h, pixels, flags=strip.flags, lead=strip.lead, extra=strip.extra)
    out = cs.Sheet(palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out, any_head=True)
    back = cs.sheet_decode(new_mph, new_mpd, ICON_SHEET)
    for k, (a, b) in enumerate(zip(back.frames, sheet.frames)):
        if k != ICON_STRIP:
            assert (a.w, a.h, a.pixels, a.flags) == (b.w, b.h, b.pixels, b.flags), f"{ICON_SHEET} frame {k} moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    for y in range(ICON):
        assert back.frames[ICON_STRIP].pixels[y][: ICON * ICON_CELL] == strip.pixels[y][: ICON * ICON_CELL], "the other six cells moved"
    return new_mph, new_mpd


def cmd_preview(out_png, zip_path):
    from PIL import Image, ImageDraw

    jar = cs.open_jar(zip_path)
    kb_mph, kb_mpd = kb.patch_sheet(jar.read(SHEET + ".mph"), jar.read(SHEET + "_0.mpd"), jar)
    mph, mpd = patch_sheet(kb_mph, kb_mpd)
    fx = cs.sheet_rgba(cs.sheet_decode(mph, mpd, SHEET, compressed_from=REACH_AT))
    sizes = sheet_sizes_after(kb.sheet_sizes_after(jar))
    anim = kb.patch_anim(jar.read(ANIM), kb.sheet_sizes_after(jar))
    _h, _p, seqs = venom_effect.decode(patch_anim(anim, sizes))
    table = {p: cs.sheet_by_path(jar, b) for p, b in cs.PREVIEW_GEAR[1].items()}
    for part in (1, 2, 3):
        table[part] = cs.sheet_by_path(jar, f"hero/{part}/100")
    table[EFFECT_PART] = fx
    scale, cw, ch, ox, oy = 3, 136, 136, 68, 76
    rows = [(IAIDO_SEQ, f) for f in range(len(seqs[IAIDO_SEQ]))] + [(WAVE_RING_SEQ, f) for f in range(len(seqs[WAVE_RING_SEQ]))] + [(BLESSED_WAVE_RING_SEQ, f) for f in range(len(seqs[BLESSED_WAVE_RING_SEQ]))]
    rows += [(CHARGE_SEQ, f) for f in range(len(seqs[CHARGE_SEQ]))] + [(IAIDO_WAVE_SEQ, f) for f in range(len(seqs[IAIDO_WAVE_SEQ]))]
    img = Image.new("RGB", (4 * cw * scale + 16 * ICON * scale, len(rows) * ch * scale), (27, 29, 35))
    dr = ImageDraw.Draw(img)
    for r, (s, f) in enumerate(rows):
        for d in range(4):
            for gx in range(-3, 4):
                for gy in range(-3, 4):
                    x0 = (d * cw + ox + gx * kb.TILE - kb.TILE // 2) * scale
                    y0 = (r * ch + oy + gy * kb.TILE - kb.TILE // 2) * scale
                    hit = (gx, gy) != (0, 0) and abs(gx) + abs(gy) <= 4
                    dr.rectangle([x0, y0, x0 + kb.TILE * scale - 1, y0 + kb.TILE * scale - 1], outline=(70, 90, 120) if hit and s == IAIDO_SEQ else (45, 48, 56))
            for x, y, w, h, px in cs.compose(seqs[s][f][d], table, {5}):
                for yy in range(h):
                    for xx in range(w):
                        p = px[yy][xx]
                        if p[3] == 0:
                            continue
                        X, Y = (d * cw + ox + x + xx) * scale, (r * ch + oy + y + yy) * scale
                        if 0 <= X < img.width and 0 <= Y < img.height:
                            dr.rectangle([X, Y, X + scale - 1, Y + scale - 1], fill=p[:3])
        dr.text((4, r * ch * scale + 4), f"seq {s} f {f}", fill=(255, 255, 0))
    # the icon, big, at the right
    icon_mph, icon_mpd = patch_icon(jar.read(ICON_SHEET + ".mph"), jar.read(ICON_SHEET + "_0.mpd"))
    strip = cs.sheet_rgba(cs.sheet_decode(icon_mph, icon_mpd, ICON_SHEET))[ICON_STRIP]
    for y in range(ICON):
        for x in range(strip[0]):
            p = strip[2][y][x]
            X, Y = 4 * cw * scale + 8 + x * 8, 8 + y * 8
            dr.rectangle([X, Y, X + 7, Y + 7], fill=p[:3])
    img.save(out_png)
    print(out_png, img.size)
    return 0


def main():
    args = sys.argv[1:]
    if args and args[0] == "preview" and len(args) in (2, 3):
        return cmd_preview(args[1], args[2] if len(args) == 3 else cs.DEFAULT_ZIP)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
