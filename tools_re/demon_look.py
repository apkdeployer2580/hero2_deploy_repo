#!/usr/bin/env python3
"""「악마화」 D2 — 악의의 외형: 악마 도트 · 몸통 시트 300 · 악마 시퀀스 · 변신 시퀀스 · bsk 문구 · bico 아이콘 (2026-10-13).

사용자(2026-10-13, 참고 그림 둘 = hero5 악마 외형): *"대지분노 → 악의 … 1 초 괴성 · 공중 변신 … 아바타·무기 무관 완전 악마 외형"*,
§6 ⑬ **a** = Claude 초안 도트 → 사용자 수정. ★ 2026-10-13 밤: 사용자가 참고를 줬다(`dist/악의 이펙트/` — DNF 악마 변신 영상
`start6` · `204357` · `204359`) + *"악마의 외형도 참고용과 같게 변경해줘"* → 둘째 판 = **강철빛 파란 피부 · 길고 검은 머리칼(앞은 얼굴 둘레 +
어깨로 두 가닥 · 옆은 등 쪽 갈기 · 뒤는 등을 타고 내려오는 꼬리) · 짙은 회색 뿔 · 붉은 손 + 발톱 · 어깨의 검은 털 가시 · 가슴의 흰 문양 ·
검은 허리 천 + 회색 띠**(첫 판 = 보라빛 검은 털 · 금 발톱 · 금 어깨). 코드 쪽 = `guest_anchors::DemonPatch`(D2 조각 DRAW · TRANSFORM) ·
`hero2java::h2_demon_*` · `emulator::Demon`(LOOK 규약) — 정본 `HERO2_GUEST_RE.md` §8.69.

2026-10-14 ASTRA 적용: 넓어진 어깨·팔·가슴, 숙인 옆모습, 남청색 피부, 검은 갈기, 금빛 발톱·가슴 문양,
붉은 눈. 51프레임·30×36·15색 계약은 유지한다.
★ 2026-10-14 밤 (3·4): 3 차 = 예상도 픽셀(풀컬러 255 색 · 56×50) · 4 차 = 자세 18 × 4 방향 + 쓰러짐 3 = **75 장** — 할퀴기 단마다
네 자세(STAGE_POSES — 그 단의 게임 프레임 0..3, 타격 프레임 = HIT_FRAMES)와 몸의 이동(MOTION), 악마 프레임은 압축 IDAT(COMPRESSED_FROM).

── 기계 (왜 이렇게 되는가) ──────────────────────────────────────────────────────────────────────────
* 플레이어의 모든 그림은 정적 드로어 `n.a(BBBII)V(seq, g.w, dir, x, y)` 하나를 지난다(서기 `a.a(IIB)V` · 걷기 `a.e(II)V` ·
  동작 `a.a(II)V` → `a.a(BBII)V`, 쓰러짐 `a.d(II)V` 까지). 호스트는 그 정의 word 0 를 감싸 LOOK 중 **시퀀스 번호만** 악마 시퀀스로
  바꾼다 — 프레임 번호 `g.w` 는 게임 것이므로 악마 시퀀스는 사람 시퀀스와 **프레임 수가 같다**(사람 시퀀스의 사본에 몸 조각만 바꿔
  끼운 것; 파트 4 이펙트는 그대로 남긴다).
* 악마의 몸은 **몸통 시트 `hero/1/300` 의 프레임 9..** 에 전부 들어 있다(0..8 = 사람 몸통 자리 · 망토 = 1×1 빈 칸). 악마 중 호스트가
  몸통 파트만 300 으로 바꿔 싣고(`AvatarPatch` 워드), 악마 시퀀스는 파트 1 의 그 프레임만 가리킨다 — 머리 · 팔 · 다리 · 무기 ·
  보조손 파트는 악마 시퀀스에 **없으므로** 무엇이 실려 있든 안 그려진다(아바타 · 무기 무관).
* ⛔ 반전 플래그(0x80)를 안 쓴다 — 왼쪽 · 오른쪽을 따로 그린다. 반전 사본은 파트의 반전 목록에 든 프레임만 만들어지고 그 목록은
  모든 몸통 시트에 걸리므로(목록에 든 프레임이 없는 시트는 로더가 던진다) 목록을 늘리면 jar 의 몸통 시트 전부를 늘려야 한다.
* ⛔ 악마 시퀀스는 몸통 시트 300 이 실려 있을 때만 그려져야 한다 — 사람 시트(9 프레임)에서 프레임 9+ 는 「배열 밖 = 던진다」.
  그 보장은 호스트의 LOOK 규약(`emulator::Demon`): 300 이 실린 것을 확인한 뒤 LOOK 1, 사람 몸통을 다시 싣기 **전에** LOOK 0.
* 변신(STATE 1) = 기본공격 0 단을 빌린 20 프레임 동작(코드가 `a.n[0]` 를 20 으로) → 그 동안 시퀀스 4 를 **변신 시퀀스**로 바꾼다 —
  사람 그림 그대로 웅크림 → 도약 → 공중 회전 → 착지(몸통 시트가 바뀌는 동안이라 악마 프레임을 쓰지 않는다).

── 계약 (코드와 같아야 한다 — `DemonPatch::{COSTUME, BODY_BASE, SHIPPED_SEQS, MAPPED_COUNT, STAGE0_SEQ}`) ────
* 의상 번호 300 · 몸통 시트 프레임 = 9 + k (k = 0..FRAMES−1) · 자세 18 × 방향 4(뒤 · 앞 · 왼 · 오른) + 쓰러짐 3(앞만) = 75 프레임
  (★ 2026-10-14 밤 (4) — 할퀴기 단마다 네 자세: 예상도의 「친 순간」 + 뼈대로 굽힌 웅크림 · 뛰어듦 · 내려침 · 회복, `demon_draft.RIG_POSES`).
* ★ 악마 프레임(9..)은 **압축 IDAT** 다(`COMPRESSED_FROM`) — 게임이 PNG 바이트를 직접 고치는 것은 반전 목록에 든 프레임뿐이고(사람
  자리 0..8), 악마 프레임은 반전을 안 쓴다. 무압축이면 75 장이 16 비트 오프셋 한도(64 KB)를 넘는다.
* 직업마다 배포본(이 파이프라인의 앞 패치 뒤) 시퀀스 수 S = 23 · 27 · 23, 몸 시퀀스 MAPPED 18 개씩 → 악마 시퀀스 = S + MAPPED 의
  자리, 변신 시퀀스 = S + 18. 호스트가 그 표(64 B, 0xff = 그대로)를 블록에 쓴다.
* 패치 순서: `make_hero2_asset.py` 의 애니메이션 체인 **맨 끝**(costume_geometry 뒤 — 변신 시퀀스가 방향 다리 번호를 복사한다).

    python tools_re/demon_look.py draft [--force]        # costumes/demon_300/demon_<k>.png (Claude 초안 — PNG 가 정본)
    python tools_re/demon_look.py preview <out.png> [zip] # 자세 × 방향 격자 + 아이콘 + 변신 · 할퀴기 시퀀스의 합성
    python tools_re/demon_look.py check [zip]             # 만든 시트 · 시퀀스 · 문구의 종단 검사 (make_hero2_asset 도 돈다)
"""

import copy
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
import costume_sheets as cs  # noqa: E402
import venom_effect  # noqa: E402
import demon_skin  # noqa: E402
from costume_draft_lib import H, Canvas, Shape, render_shape  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC_DIR = os.path.join(ROOT, "costumes", "demon_300")  # 「demon_300」: 앞이 숫자가 아니라 costume_sheets.list_costumes 가 안 집는다
COSTUME = 300
BODY_DIR = 1
BODY_STEM = str(COSTUME)
BODY_MEMBERS = (f"hero/{BODY_DIR}/{BODY_STEM}.mph", f"hero/{BODY_DIR}/{BODY_STEM}_0.mpd")
# ★★ 2026-10-15 HD: the BLANK body sheet 301 — the same frame count, every frame 1×1 transparent; the host loads it under HD mode
# (`DemonPatch::COSTUME_HD`) so the game's drawer draws no body and the host composites the HD body (`demon_fx` `demon_body`).
COSTUME_HD = 301
BLANK_MEMBERS = (f"hero/{BODY_DIR}/{COSTUME_HD}.mph", f"hero/{BODY_DIR}/{COSTUME_HD}_0.mpd")
BODY_BASE = 9  # 사람 몸통 0..4 + 망토 5..8 (1×1 빈 칸)
FW, FH = 30, 36  # 2 차 초안 그림(`render_pose`)의 크기 (발바닥 = 34 행)
CX, GROUND = 15.0, 34.0
FEET_DY = 2  # 사람 발바닥(원점 아래 2 px)에 악마 발바닥을 맞춘다
# ★ 2026-10-14 밤 3 차: PNG 정본은 예상도에서 오린 악마(`demon_draft.py`) — 캔버스 56 × 50, 발바닥 (28, 46). 시트에는 그림이 있는
#   상자만 들어가고, 그 상자의 자리가 프레임마다의 (dx, dy) 다(`frame_offsets`). 옛 30 × 36 PNG 도 그대로 읽힌다(발바닥 (15, 34)).
CANVAS_FEET = {(30, 36): (15, 34), (56, 50): (28, 46)}

BACK, FRONT, LEFT, RIGHT = 0, 1, 2, 3  # 시퀀스의 방향 목록 순서 (g.u 1..4)
# ★ 2026-10-14 밤 (4): 할퀴기 세 단 = 단마다 네 자세(c<단>_*) — 사람 시퀀스의 프레임 f 가 그 단의 f 번째 자세(넘으면 마지막)이고,
#   타격 프레임(`DemonPatch::HIT_FRAMES` = 0 단 f1 · f3 · 1 단 f2 · 2 단 f2)이 「친」 자세에 온다(`pose_for`).
POSES = ["stand", "walk0", "walk1", "walk2", "walk3",
         "c0_wind", "c0_hitA", "c0_windB", "c0_hitB",
         "c1_coil", "c1_dash", "c1_hit", "c1_follow",
         "c2_coil", "c2_leap", "c2_hit", "c2_impact",
         "roar"]
FALLS = ["fall0", "fall1", "fall2"]  # 쓰러짐은 게임이 방향 1(앞)로만 그린다
# ★★ 2026-10-14 (5): 서기 숨쉬기 · 스킬 시전 동작 · 돌진 달리기 — 쓰러짐 뒤(프레임 84..123)에 덧붙인다(앞 75 장의 번호는 그대로).
#   그림 = `demon_draft.RIG_POSES`, 쓰는 곳 = `CAST_SEQS`(DemonPatch::POSE_* — DRAW 조각이 서기 · 걷기 대신 고른다).
EXTRA_POSES = ["idle", "run0", "run1", "blade_wind", "blade_thrust", "judge_raise", "judge_up", "fire_gather", "lava_rise", "lava_slam"]
FRAMES = len(POSES) * 4 + len(FALLS) + len(EXTRA_POSES) * 4  # 115
# ⛔ 그리는 쪽(`n.a([BIBB[Ljava/lang/Object;ZBBII)B` 0x1634a4)은 시퀀스의 프레임 번호를 **부호 있는 바이트**로 읽는다(−1 = 건너뜀) —
#   몸통 시트에서 쓸 수 있는 프레임은 0..127. 시트 크기(.mpd)에는 한도가 없다(오프셋 u32 — costume_sheets 머리말).
FRAME_LIMIT = 128
COMPRESSED_FROM = 9  # = BODY_BASE: 이 프레임부터 압축 IDAT (반전 목록 밖 — make_hero2_asset 이 확인한다)
# 옛 자세 이름(3 차까지 · `demon_fx` 장면 · 2 차 절차 도트 `render_pose`) → 지금 이름
LEGACY = {"claw_a": "c0_wind", "claw_b": "c0_hitA", "sweep_low": "c1_coil", "sweep_high": "c1_hit", "slam_up": "c2_leap", "slam_down": "c2_hit"}
_PROCEDURAL = {"c0_wind": "claw_a", "c0_hitA": "claw_b", "c0_windB": "claw_a", "c0_hitB": "claw_b",
               "c1_coil": "sweep_low", "c1_dash": "sweep_low", "c1_hit": "sweep_high", "c1_follow": "sweep_high",
               "c2_coil": "slam_up", "c2_leap": "slam_up", "c2_hit": "slam_down", "c2_impact": "slam_down"}
# 할퀴기 단 → 네 자세 · 타격 프레임 (DemonPatch::HIT_FRAMES 와 같아야 한다 — test_demon_look 이 guest_anchors.rs 와 대조)
STAGE_POSES = {4: ["c0_wind", "c0_hitA", "c0_windB", "c0_hitB"],
               5: ["c1_coil", "c1_dash", "c1_hit", "c1_follow"],
               6: ["c2_coil", "c2_leap", "c2_hit", "c2_impact"]}
HIT_FRAMES = {4: (1, 3), 5: (2,), 6: (2,)}
# 자세마다 몸의 움직임 (앞으로 px, 아래로 px) — 시퀀스 조각의 (dx, dy)에 더한다(그림을 늘리지 않고 뛰어듦 · 도약을 준다).
# 앞 = 바라보는 쪽: 오른 +x · 왼 −x · 앞모습(아래) +y · 뒷모습(위) −y.
MOTION = {"c0_wind": (-1, 0), "c0_hitA": (2, 0), "c0_windB": (1, 0), "c0_hitB": (2, 0),
          "c1_coil": (-2, 1), "c1_dash": (2, 0), "c1_hit": (3, 0), "c1_follow": (3, 0),
          "c2_coil": (0, 1), "c2_leap": (1, -5), "c2_hit": (3, 1), "c2_impact": (3, 1)}

SHIPPED_SEQS = {0: 23, 1: 27, 2: 23}  # berserker_taunt · awakenings / knight_iaido · awakenings / rogue_* · awakenings 뒤
FALL_SEQ = {0: 17, 1: 18, 2: 16}  # 정적 a.m:[B = [17, 18, 16] (a.<clinit>)
STAGE0_SEQ = 4  # 기본공격 0 단 = 종류 0 → 시퀀스 4 (변신이 빌리는 자리)
TRANSFORM_FRAMES = 20  # = DemonPatch::TRANSFORM_FRAMES
# 몸 시퀀스(머리 · 몸통 · 팔 · 다리가 있는 것) — 직업마다 18 개. 코드의 DemonPatch::MAPPED 와 글자 그대로 같아야 한다.
MAPPED = {
    0: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 17, 19, 20, 21],
    1: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 18, 19, 23, 25],
    2: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 19, 21],
}
MAPPED_COUNT = 18

BSK_TDF = "t/bsk.tdf"
BSK_SHIPPED_ROWS = 26
# ★★ 2026-10-16: the 악의 versions of the six skills it changes — name + description rows appended after the shipped 26
# (`demon_skin.SKIN`; the host exchanges them in while 악의 runs, `DemonPatch::SKIN_KINDS`)
BSK_ROWS = BSK_SHIPPED_ROWS + 2 * len(demon_skin.SKIN)
BSK_REPLACE = {
    18: ("대지분노", "악의"),
    19: ("{지진@을 일으켜 적을 공격", "잠시동안 {야수@에게;모든 주도권을;넘긴다."),
    # ★ D3 (2026-10-13): 악의 중에는 암살자 → 지옥칼날(7×7 토글) · 심판번개 → 어둠심판(9×9 토글) — 이름은 그대로(악의 밖에선 옛 스킬), 설명에 한 줄
    7: ("}검은 기운@으로 범위내의 적을 공격", "}검은 기운@으로 범위내의 적을 공격;{악의@ 시 }지옥칼날@"),
    21: ("{번개@를 내리쳐 적을 공격", "{번개@를 내리쳐 적을 공격;{악의@ 시 }어둠심판@"),
    # ★ D4 (2026-10-13): 악의 중에는 피의계약 → 추종(1 분 게이지 버프 · 끝에 어둠 폭발) · 암흑폭발 → 돌진(5 초 · 3×3 밀치기)
    5: ("주인공 {HP@를 감소시켜 $SP@를 회복", "주인공 {HP@를 감소시켜 $SP@를 회복;{악의@ 시 }추종@"),
    25: ("}암흑@을 폭발시켜 적을 공격", "}암흑@을 폭발시켜 적을 공격;{악의@ 시 }돌진@"),
    # ★ D5 (2026-10-13): 악의 중에는 지옥불 → 용암불(터진 칸이 5 초 용암) · 학살화염 → 멸화(0.5 초 뒤 13×13 폭발)
    # ★★ 2026-10-13 밤 (사용자): 셋째 줄은 전부 「악의 시 <스킬>」 만(초 · 크기 · 토글 같은 꼬리 없음) · 악의 설명 = 「잠시동안 야수에게 모든 주도권을 넘긴다.」
    17: ("{지옥의 불@을 소환하여 적을 공격", "{지옥의 불@을 소환하여 적을 공격;{악의@ 시 }용암불@"),
    23: ("{화염@을 일으켜 적을 공격", "{화염@을 일으켜 적을 공격;{악의@ 시 }멸화@"),
}
ICON_SHEET = "img/bico"
ICON = 12
ICON_FRAMES = 13
ICON_AT = 9  # 전역 야수 스킬 id 9 = 대지분노 → 악의
ICON_FRAMES_BUILT = ICON_FRAMES + len(demon_skin.SKIN)  # ★★ 2026-10-16: + the six 악의 skill icons (frames 13..18)
assert demon_skin.ROW_FIRST == BSK_SHIPPED_ROWS and demon_skin.ICON_FIRST == ICON_FRAMES

# ── 색 (15 + 투명 = 16 → 4 비트 시트) — ★ 2026-10-13 밤: 참고 그림(`dist/악의 이펙트/start6 · 204357 · 204359`)대로 ──
# 강철빛 파란 피부 · 길고 검은 머리칼(어깨 · 등으로 흐른다) · 짙은 회색 뿔 · 붉은 손발톱 · 어깨의 검은 털 · 가슴의 흰 문양 ·
# 검은 허리 천. (첫 판 = 보라빛 검은 털 + 금 발톱 · 금 어깨 — 사용자: "악마의 외형도 참고용과 같게 변경해줘")
SKIN = [H("#91a6d5"), H("#536ba6"), H("#304576"), H("#192640")]
HAIR = [H("#393544"), H("#211d2b"), H("#100e19")]
HORN = [H("#9a9ab0"), H("#5c5c74"), H("#33334a")]
CLAW = [H("#ffe0a0"), H("#d99945"), H("#b52635")]
OUT = H("#0d0a18")
EYE, EYE_GLOW = CLAW[2], CLAW[2]
TOOTH = H("#ffe6ba")
BELT = HORN[1]
RAMPS = {"skin": SKIN, "hair": HAIR, "horn": HORN, "claw": CLAW}
OUTLINES = {"skin": OUT, "hair": OUT, "horn": OUT, "claw": CLAW[2]}  # 발톱은 가늘어 어두운 빨강 테 (검은 테면 다 먹힌다)


def frame_of(pose, view):
    """몸통 시트 프레임 번호 (9 + k). 쓰러짐은 방향 무관. 옛 이름(`LEGACY`)도 받는다."""
    pose = LEGACY.get(pose, pose)
    if pose in FALLS:
        return BODY_BASE + len(POSES) * 4 + FALLS.index(pose)
    if pose in EXTRA_POSES:
        return BODY_BASE + len(POSES) * 4 + len(FALLS) + EXTRA_POSES.index(pose) * 4 + view
    return BODY_BASE + POSES.index(pose) * 4 + view


assert BODY_BASE + FRAMES <= FRAME_LIMIT, "a demon frame past 127 — the drawer reads the frame as a signed byte"


# ── 자세 — 앞모습 골격 (x 오른쪽 · y 아래, 원점 = 캔버스 왼쪽 위) ───────────────────────────────────
def _front(**kw):
    """앞모습 기본 골격 + 덮어쓰기. 좌 = 보는 사람의 왼쪽."""
    j = dict(
        head=(15, 9), head_r=4.4, horn=[((11.5, 6), (8, 1)), ((18.5, 6), (22, 1))],
        eyes=[(13, 7.5), (17, 7.5)], mouth=None,
        torso=(15, 18.5), torso_dir=(0, 1), torso_ab=(7.8, 8.5), hips=(15, 25.5), hips_ab=(4.2, 5.0),
        sho=[(6.8, 14), (23.2, 14)], sho_r=3.5,
        elb=[(4.8, 20), (25.2, 20)], hand=[(6, 26), (24, 26)], claw=["down", "down"],
        knee=[(11, 30), (19, 30)], foot=[(11, 33.3), (19, 33.3)], hip_pt=[(12, 26), (18, 26)],
        arm_front=[True, True], body_dy=0, face=True,
    )
    j.update(kw)
    return j


def _side(**kw):
    """옆모습(왼쪽을 본다) 기본 골격. near = 보는 사람 쪽 팔·다리 (앞에), far = 뒤."""
    j = dict(
        head=(9.5, 9.5), head_r=4.3, snout=(5.8, 10.3), horn=[((11, 6), (17, 1.2)), ((8.5, 6.4), (13.5, 1))],
        eyes=[(8.2, 6.6)], mouth=None,
        torso=(15, 18.5), torso_dir=(0.25, 1), torso_ab=(7.8, 6.4), hips=(16, 25.5), hips_ab=(4.0, 4.2),
        sho=[(13.5, 13.5), (17.5, 14.2)], sho_r=3.0,
        elb=[(12.5, 20), (18.5, 20.5)], hand=[(12, 26), (18.6, 26.3)], claw=["down", "down"],
        knee=[(12.5, 30), (17.5, 30)], foot=[(11.5, 33.3), (18.5, 33.3)], hip_pt=[(14, 26), (16.5, 26)],
        arm_front=[True, False], body_dy=0, face=True,
    )
    j.update(kw)
    return j


def _shift(pts, dx, dy):
    return [(x + dx, y + dy) for x, y in pts]


def pose_joints(pose, view):
    """자세 · 방향(BACK · FRONT · LEFT) → 골격. RIGHT 는 LEFT 의 반전(캔버스에서)."""
    pose = _PROCEDURAL.get(pose, pose)
    side = view == LEFT
    s = _side if side else _front
    if pose == "stand":
        return s()
    if pose.startswith("walk"):
        k = int(pose[4:])
        sw = [1.0, 0.0, -1.0, 0.0][k]
        bob = -1 if k % 2 else 0
        if side:
            return s(
                foot=[(11.5 - 4 * sw, 33.3), (18.5 + 4 * sw, 33.3)],
                knee=[(12.5 - 2.5 * sw, 30 - abs(sw)), (17.5 + 2.5 * sw, 30 - abs(sw))],
                hand=[(12 + 3 * sw, 26), (18.6 - 3 * sw, 26.3)], elb=[(12.5 + 1.5 * sw, 20), (18.5 - 1.5 * sw, 20.5)],
                body_dy=bob,
            )
        return s(
            foot=[(11, 33.3 - 1.5 * max(0, sw)), (19, 33.3 - 1.5 * max(0, -sw))],
            knee=[(11, 30 - 1.5 * max(0, sw)), (19, 30 - 1.5 * max(0, -sw))],
            hand=[(6, 26 + 1.5 * sw), (24, 26 - 1.5 * sw)], body_dy=bob,
        )
    if pose == "claw_a":  # 오른팔(보는 사람의 오른쪽)이 가슴 앞을 가로지른다
        if side:
            return s(elb=[(8.5, 16), (18.5, 20.5)], hand=[(3, 19), (18.6, 26.3)], claw=["left", "down"])
        return s(elb=[(4.8, 20), (19.5, 15)], hand=[(6, 26), (8.5, 17)], claw=["down", "left"])
    if pose == "claw_b":
        if side:
            return s(elb=[(8, 13), (18.5, 20.5)], hand=[(2.6, 15), (18.6, 26.3)], claw=["left", "down"])
        return s(elb=[(10.5, 15), (25.2, 20)], hand=[(21.5, 17), (24, 26)], claw=["right", "down"])
    if pose == "sweep_low":
        if side:
            return s(elb=[(8, 23), (18.5, 24)], hand=[(4, 29), (19, 30)], claw=["left", "down"], body_dy=1.5)
        return s(elb=[(5.5, 23), (24.5, 23)], hand=[(9, 29), (21, 29)], claw=["down", "down"], body_dy=1.5)
    if pose == "sweep_high":
        if side:
            return s(elb=[(8, 12), (18.5, 17)], hand=[(4, 5.5), (19, 20)], claw=["up", "down"], body_dy=-1)
        return s(elb=[(4, 12), (26, 12)], hand=[(7, 6), (23, 6)], claw=["up", "up"], body_dy=-1)
    if pose == "slam_up":
        if side:
            return s(elb=[(10.5, 8), (17, 9)], hand=[(9.5, 1.5), (13, 2)], claw=["up", "up"], body_dy=-1.5)
        return s(elb=[(8, 8), (22, 8)], hand=[(12, 1.5), (18, 1.5)], claw=["up", "up"], body_dy=-1.5)
    if pose == "slam_down":
        if side:
            return s(
                elb=[(6.5, 26), (17, 27)], hand=[(3, 32), (6, 33)], claw=["down", "down"], body_dy=3,
                knee=[(10, 29.5), (19, 29.5)], foot=[(11.5, 33.3), (18.5, 33.3)],
            )
        return s(
            elb=[(5.5, 26), (24.5, 26)], hand=[(8, 32), (22, 32)], claw=["down", "down"], body_dy=3,
            knee=[(9, 29.5), (21, 29.5)], foot=[(11, 33.3), (19, 33.3)],
        )
    if pose == "roar":
        if side:
            return s(head=(11, 6.5), snout=(6.4, 6.2), elb=[(6, 18), (18.5, 19)], hand=[(2, 11), (23, 13)], claw=["up", "right"], mouth=(7, 9.2), body_dy=-0.5)
        return s(head=(15, 6.5), elb=[(5.5, 18), (24.5, 18)], hand=[(2, 12), (28, 12)], claw=["left", "right"], mouth=(15, 10.5), body_dy=-0.5)
    if pose == "fall0":  # 비틀거림 (앞모습)
        return _front(head=(18, 8), torso=(17, 19), torso_dir=(0.17, 0.98), hips=(16, 25.5), elb=[(5, 17), (26, 22)], hand=[(2.5, 14), (27.5, 28)], claw=["left", "down"], knee=[(10, 29.5), (20, 30)])
    if pose == "fall1":  # 넘어지는 중 — 몸이 기운다
        return _front(
            head=(8, 15), head_r=5, horn=[((5, 12), (1.5, 9)), ((10, 11), (9, 6.5))], eyes=[(6.5, 15.5), (9.5, 14)],
            torso=(16, 22), torso_dir=(0.76, 0.65), torso_ab=(7.5, 6.5), hips=(23, 27), hips_ab=(3.5, 4.5),
            sho=[(10, 19), (14, 11)], elb=[(4, 22), (18, 8)], hand=[(2, 27), (22, 4)], claw=["down", "up"],
            hip_pt=[(22, 28), (25, 27)], knee=[(24, 31), (28, 30)], foot=[(22, 33.3), (28, 33.3)], face=True,
        )
    if pose == "fall2":  # 누움 — 왼쪽으로 머리
        return _front(
            head=(5.5, 27), head_r=5, horn=[((2.5, 24), (0.6, 20.5)), ((5, 22.5), (4.5, 19))], eyes=[], face=False,
            torso=(16, 29), torso_dir=(1, 0), torso_ab=(7.8, 4.6), hips=(23, 29.5), hips_ab=(3.4, 3.8),
            sho=[(11, 25), (13, 32)], sho_r=2.8, elb=[(8, 22.5), (12, 34)], hand=[(4, 21.5), (9, 33.5)], claw=["left", "left"],
            hip_pt=[(24, 28), (24, 31)], knee=[(27.5, 27.5), (27.5, 31.5)], foot=[(29, 27.5), (29, 31.5)], arm_front=[True, True],
        )
    raise ValueError(pose)


def _unit(dx, dy):
    n = math.hypot(dx, dy) or 1.0
    return dx / n, dy / n


def _claw_shapes(sh, hand, direction, r=0.8, length=3.4):
    """손에서 뻗는 붉은 발톱 셋 — direction 쪽으로 부채꼴."""
    base = {"down": (0, 1), "up": (0, -1), "left": (-1, 0), "right": (1, 0)}[direction]
    for ang in (-28, 0, 28):
        a = math.radians(ang)
        dx = base[0] * math.cos(a) - base[1] * math.sin(a)
        dy = base[0] * math.sin(a) + base[1] * math.cos(a)
        hx, hy = hand
        sh.capsule(hx + dx * 1.4, hy + dy * 1.4, hx + dx * length, hy + dy * length, r, "claw")


def render_pose(pose, view):
    """자세 · 방향 → Canvas (W×H). RIGHT = LEFT 반전. 층마다 따로 그려 겹쳐 올린다(층 사이 외곽선이 조각을 가른다).

    ★ 2026-10-13 밤 참고대로: 파란 피부(skin) · 머리칼(hair — 앞에선 머리 뒤 + 어깨로 내려오는 두 가닥, 옆에선 등 쪽으로
    흐르는 갈기, 뒤에선 등을 덮는 큰 덩어리) · 어깨 검은 털 뭉치 · 붉은 손(발톱 색) + 발톱 · 가슴 흰 문양(앞) · 검은 허리 천 + 띠.
    """
    if view == RIGHT:
        return render_pose(pose, LEFT).flipped_x()
    j = pose_joints(pose, view)
    dy = j["body_dy"]
    cv = Canvas(FW, FH)
    ramp_for = lambda t: RAMPS[t or "skin"]  # noqa: E731
    outline_for = lambda t: OUTLINES[t or "skin"]  # noqa: E731

    def layer(build):
        sh = Shape()
        build(sh)
        cv.stamp(render_shape(FW, FH, sh, ramp_for, outline_for), 0, 0)

    def arm(sh, i):
        (sx, sy), (ex, ey), (hx, hy) = j["sho"][i], j["elb"][i], j["hand"][i]
        sh.capsule(sx, sy + dy, ex, ey, 3.1, "skin")
        sh.capsule(ex, ey, hx, hy, 2.7, "skin")
        sh.circle(hx, hy, 2.4, "skin")
        _claw_shapes(sh, (hx, hy), j["claw"][i])

    def leg(sh, i):
        (hx, hy), (kx, ky), (fx, fy) = j["hip_pt"][i], j["knee"][i], j["foot"][i]
        sh.capsule(hx, hy + dy, kx, ky, 3.2, "skin")
        sh.capsule(kx, ky, fx, fy, 2.4, "skin")
        sh.ellipse(fx, fy + 0.6, 1, 0, 3.0, 1.7, "skin")
        sh.capsule(fx - 1.5, fy + 1.4, fx - 1.5, fy + 2.3, 0.6, "claw")
        sh.capsule(fx + 1.5, fy + 1.4, fx + 1.5, fy + 2.3, 0.6, "claw")

    back_view = view == BACK
    side_view = view == LEFT
    far_arms = [i for i, f in enumerate(j["arm_front"]) if not f or back_view]
    near_arms = [i for i, f in enumerate(j["arm_front"]) if f and not back_view]
    hx0, hy0 = j["head"]
    tx0, ty0 = j["torso"]
    ddx, ddy = _unit(tx0 - hx0, ty0 - hy0)  # 머리 → 몸통 (머리칼이 흐르는 쪽)

    def hair_back(sh):
        """머리 뒤의 머리칼 덩어리 — 앞모습은 머리 둘레 + 어깨로 내려오는 두 가닥, 옆모습은 등 쪽 갈기."""
        hx, hy = hx0, hy0 + dy
        if side_view:
            sh.capsule(hx + 1, hy - 2.5, hx + 7.5, hy + 4.5, 3.4, "hair")
            sh.capsule(hx + 7, hy + 4, hx + 9.5, hy + 12.5, 2.6, "hair")
            sh.capsule(hx + 9, hy + 12, hx + 9.5, hy + 18, 1.7, "hair")
            for ox, oy in ((8, 6), (10, 10), (11, 14)):
                sh.capsule(hx + ox - 3, hy + oy - 3, hx + ox + 2, hy + oy, 1.3, "hair")
        else:
            sh.ellipse(hx + ddx * 2.5, hy + ddy * 2.5, ddx, ddy, 7.4, 6.6, "hair")
            if not back_view:
                px, py = -ddy, ddx  # 몸의 가로 방향
                for sgn in (-1, 1):
                    ax, ay = hx + px * sgn * 6.0 + ddx * 2, hy + py * sgn * 6.0 + ddy * 2
                    bx, by = hx + px * sgn * 7.0 + ddx * 12, hy + py * sgn * 7.0 + ddy * 12
                    sh.capsule(ax, ay, bx, by, 1.9, "hair")

    def hair_mane_back(sh):
        """뒷모습: 등을 덮는 큰 머리칼 (머리 · 몸통 위에)."""
        hx, hy = hx0, hy0 + dy
        sh.ellipse(hx, hy, 0, 1, 5.6, 5.2, "hair")  # 뒤통수
        sh.ellipse(hx + ddx * 6, hy + ddy * 6, ddx, ddy, 5.6, 6.0, "hair")  # 어깨 사이
        sh.capsule(hx + ddx * 8, hy + ddy * 8, hx + ddx * 15.5, hy + ddy * 15.5, 3.2, "hair")  # 등을 타고 내려오는 꼬리
        sh.capsule(hx + ddx * 15, hy + ddy * 15, hx + ddx * 19, hy + ddy * 19, 1.9, "hair")

    def hair_top(sh):
        """정수리의 앞머리 — 뿔 사이."""
        hx, hy = hx0, hy0 + dy
        if side_view:
            sh.ellipse(hx + 0.5, hy - 3.6, 1, 0, 4.6, 2.1, "hair")
        else:
            sh.ellipse(hx - ddx * 3.8, hy - ddy * 3.8, -ddy, ddx, 5.0, 2.2, "hair")

    # 뒤 팔 · 뒤 다리 → 엉덩이 · 몸통 → 허리 천 → 앞 다리 → 머리칼(뒤) → 앞 팔 → 어깨 털 → 머리 → 앞머리 → 뿔
    for i in far_arms:
        layer(lambda sh, i=i: arm(sh, i))
    layer(lambda sh: leg(sh, 1 if view == LEFT else 0))

    def trunk(sh):
        hx, hy = j["hips"]
        sh.ellipse(hx, hy + dy, 0, 1, j["hips_ab"][0], j["hips_ab"][1], "skin")
        tx, ty = j["torso"]
        ux, uy = _unit(*j["torso_dir"])
        sh.ellipse(tx, ty + dy, ux, uy, j["torso_ab"][0], j["torso_ab"][1], "skin")

    layer(trunk)

    def cloth(sh):
        hx, hy = j["hips"]
        a, b = j["hips_ab"]
        sh.ellipse(hx, hy + dy + 0.6, 0, 1, a + 0.5, max(b - 1.2, 1.6), "hair")

    layer(cloth)
    # 허리 띠 — 천의 윗줄에 회색 한 줄
    hx, hy = j["hips"]
    a, b = j["hips_ab"]
    by = int(round(hy + dy + 0.6 - max(b - 1.2, 1.6) + 1))
    for k in range(-int(a) + 1, int(a)):
        if cv.opaque(int(hx) + k, by):
            cv.put(int(hx) + k, by, BELT)
    layer(lambda sh: leg(sh, 0 if view == LEFT else 1))
    if not back_view:
        layer(hair_back)
    for i in near_arms:
        layer(lambda sh, i=i: arm(sh, i))

    def shoulders(sh):
        for k, (x, y) in enumerate(j["sho"]):
            if view == LEFT and k == 1:
                continue
            r = j["sho_r"] if k == 0 or view != LEFT else j["sho_r"] - 0.4
            sh.circle(x, y + dy, r, "hair")
            out = -1 if (x < hx0 or (side_view and k == 0)) else 1
            sh.capsule(x, y + dy - 1, x + out * 2.2, y + dy - 4.2, 0.9, "hair")  # 검은 털 가시
            sh.capsule(x + out * 1.5, y + dy, x + out * 3.6, y + dy - 2.4, 0.8, "hair")

    layer(shoulders)

    def head(sh):
        hx, hy = j["head"]
        sh.circle(hx, hy + dy, j["head_r"], "skin")
        if "snout" in j:
            sx, sy = j["snout"]
            sh.ellipse(sx, sy + dy, 1, 0, 3.2, 2.3, "skin")

    layer(head)
    if back_view:
        layer(hair_mane_back)
    else:
        layer(hair_top)
    layer(lambda sh: [sh.capsule(a[0], a[1] + dy, b[0], b[1] + dy, 1.25, "horn") for a, b in j["horn"]])
    if j["face"] and not back_view:
        for ex, ey in j["eyes"]:
            cv.put(int(ex), int(ey + dy), EYE)
            cv.put(int(ex) + (1 if view != LEFT else -1), int(ey + dy), EYE_GLOW)
        if j["mouth"]:
            mx, my = j["mouth"]
            for k in range(-2, 3):
                cv.put(int(mx) + k, int(my + dy), OUT if k % 2 else TOOTH)
            cv.put(int(mx) - 1, int(my + dy) + 1, TOOTH)
            cv.put(int(mx) + 1, int(my + dy) + 1, TOOTH)
    # 가슴의 흰 문양 (앞모습 · 서 있을 때만 — 몸통 위쪽 가운데)
    if view == FRONT and j["face"] and pose not in ("fall1", "fall2"):
        tx, ty = j["torso"]
        cx, cy = int(round(tx)), int(round(ty + dy - 2.5))
        for (ox, oy) in ((0, 0), (0, -1), (0, 1), (0, 2), (-2, -1), (2, -1), (-3, -2), (3, -2)):
            if cv.opaque(cx + ox, cy + oy):
                cv.put(cx + ox, cy + oy, TOOTH)
        for (ox, oy) in ((-1, -1), (1, -1), (-1, 1), (1, 1)):
            if cv.opaque(cx + ox, cy + oy):
                cv.put(cx + ox, cy + oy, SKIN[0])
    return cv


def draft_frames():
    """75 장 — POSES × 4 방향 + FALLS (앞)."""
    out = []
    for pose in POSES:
        for view in (BACK, FRONT, LEFT, RIGHT):
            out.append(render_pose(pose, view))
    for pose in FALLS:
        out.append(render_pose(pose, FRONT))
    for pose in EXTRA_POSES:  # ★ (5): 옛 절차 도트에는 그 자세가 없다 — 서기 · 포효로 자리만
        for view in (BACK, FRONT, LEFT, RIGHT):
            out.append(render_pose("stand" if pose in ("idle", "run0", "run1") else "roar", view))
    assert len(out) == FRAMES
    return out


# ── PNG 정본 · 시트 ───────────────────────────────────────────────────────────────────────────────
def cmd_draft(force=False):
    if not force:
        lib.refuse_overwrite(SRC_DIR, ["demon"])
    frames = draft_frames()
    lib.write_part(SRC_DIR, "demon", frames)
    with open(os.path.join(SRC_DIR, "README.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(__doc__.split("── 기계")[0])
        f.write("\n프레임 번호 k (demon_<k>.png) = 자세 × 4 + 방향(0 뒤 · 1 앞 · 2 왼 · 3 오른), 쓰러짐 48..50 (앞만):\n")
        for i, p in enumerate(POSES):
            f.write(f"  {4 * i:2d}..{4 * i + 3:2d} {p}\n")
        for i, p in enumerate(FALLS):
            f.write(f"  {len(POSES) * 4 + i:2d}     {p}\n")
        f.write("몸통 시트 hero/1/300 의 프레임 = 9 + k. 크기는 자유(지금 30×36) · 발바닥 = 원점 아래 2 px 에 맞춰 놓인다.\n")
    return 0


def load_frames():
    """costumes/demon_300/demon_<k>.png → [Frame] + 팔레트 (없으면 초안을 그 자리에서 쓴다 — 에셋 빌드는 PNG 를 요구한다)."""
    want = [os.path.join(SRC_DIR, f"demon_{k}.png") for k in range(FRAMES)]
    missing = [p for p in want if not os.path.exists(p)]
    if missing:
        raise cs.SheetError(f"{SRC_DIR}: {len(missing)} 장 없음 (예: {os.path.basename(missing[0])}) — `python tools_re/demon_look.py draft` 먼저")
    palette, index, frames = [cs.TRANSPARENT_RGB], {}, []
    for k, path in enumerate(want):
        pic = cs.Picture(path)
        x0, y0, x1, y1 = _opaque_box(pic, k)
        rows = []
        for y in range(y0, y1):
            row = []
            for x in range(x0, x1):
                r, g, b, a = pic.rgba(x, y)
                if a == 0:
                    row.append(0)
                    continue
                if a != 255:
                    raise cs.SheetError(f"demon_{k}.png: ({x},{y}) 반투명 — 게임 시트는 투명/불투명 둘뿐이다")
                c = (r, g, b)
                if c not in index:
                    index[c] = len(palette)
                    palette.append(c)
                row.append(index[c])
            rows.append(row)
        frames.append(cs.Frame(x1 - x0, y1 - y0, rows))
    if len(palette) > 256:
        raise cs.SheetError(f"악마 시트: 색 {len(palette) - 1} 개 — 255 까지")
    return palette, frames


def _opaque_box(pic, k):
    xs, ys = [], []
    for y in range(pic.h):
        for x in range(pic.w):
            if pic.rgba(x, y)[3]:
                xs.append(x)
                ys.append(y)
    if not xs:
        raise cs.SheetError(f"demon_{k}.png: 전부 투명")
    if (pic.w, pic.h) not in CANVAS_FEET:
        raise cs.SheetError(f"demon_{k}.png: {pic.w}×{pic.h} — 캔버스는 {sorted(CANVAS_FEET)} 중 하나")
    return min(xs), min(ys), max(xs) + 1, max(ys) + 1


_OFFSETS = []


def frame_offsets():
    """프레임 k 마다 (dx, dy) — 그림 상자의 왼쪽 위가 사람 원점에서 떨어진 자리 (옛 30 × 36 이면 늘 (−15, −34))."""
    if not _OFFSETS:
        for k in range(FRAMES):
            pic = cs.Picture(os.path.join(SRC_DIR, f"demon_{k}.png"))
            x0, y0, _, _ = _opaque_box(pic, k)
            fx, fy = CANVAS_FEET[(pic.w, pic.h)]
            _OFFSETS.append((x0 - fx, y0 + FEET_DY - 2 - fy))
    return _OFFSETS


def body_sheet():
    """몸통 시트 300 = 사람 자리 9 장(1×1 빈 칸) + 악마 75 장."""
    palette, frames = load_frames()
    blank = cs.Frame(1, 1, [[0]])
    return cs.Sheet(palette, [blank] * BODY_BASE + frames)


def last_frame_start(mph):
    """프레임 표의 마지막 오프셋 (u32, 항목 +2 — ★ 2026-10-14: 게임에 64 KB 한도는 없다, costume_sheets 머리말)."""
    n = int.from_bytes(mph[4:8], "big")
    return int.from_bytes(mph[8 + 8 * (n - 1) + 2 : 8 + 8 * (n - 1) + 6], "big")


def members():
    """{jar 멤버: 바이트} — hero/1/300 (반전 지원 시트 꼴; 사람 자리 0..8 은 무압축, 악마 프레임은 압축 IDAT) + ★ HD hero/1/301 (빈 시트)."""
    sheet = body_sheet()
    mph, mpd = cs.sheet_encode(sheet, compress_from=COMPRESSED_FROM)
    back = cs.sheet_decode(mph, mpd, BODY_STEM, compressed_from=COMPRESSED_FROM)
    assert len(back.frames) == BODY_BASE + FRAMES and [f.pixels for f in back.frames] == [f.pixels for f in sheet.frames]
    blank = cs.Frame(1, 1, [[0]])
    bsheet = cs.Sheet([cs.TRANSPARENT_RGB], [blank] * (BODY_BASE + FRAMES))
    bmph, bmpd = cs.sheet_encode(bsheet)
    bback = cs.sheet_decode(bmph, bmpd, str(COSTUME_HD))
    assert len(bback.frames) == BODY_BASE + FRAMES and all(f.w == 1 and f.h == 1 for f in bback.frames)
    return {BODY_MEMBERS[0]: mph, BODY_MEMBERS[1]: mpd, BLANK_MEMBERS[0]: bmph, BLANK_MEMBERS[1]: bmpd}


# ── 애니메이션 — 악마 시퀀스 18 + 변신 1 ──────────────────────────────────────────────────────────
def pose_for(cls, seq, f):
    """사람 시퀀스 seq 의 프레임 f 에 서는 악마 자세. 할퀴기 세 단(4 · 5 · 6)은 단의 네 자세를 차례로(넘는 프레임은 마지막 자세 —
    직업마다 단 길이가 다르다: 나이트 4 · 4 · 4 · 로그 4 · 5 · 3). 타격 프레임이 「친」 자세에 온다(`HIT_FRAMES`)."""
    if seq in (0, 1):
        return "stand"
    if seq in (2, 3):
        return f"walk{f % 4}"
    if seq in STAGE_POSES:
        poses = STAGE_POSES[seq]
        return poses[min(f, len(poses) - 1)]
    if seq == FALL_SEQ[cls]:
        return "fall0" if f < 3 else ("fall1" if f < 7 else "fall2")
    if cls == 1 and seq == 12:  # 철괴 자세의 서기
        return "stand"
    return "roar"


def motion(pose, view):
    """자세의 몸 움직임 (dx, dy) — `MOTION` 의 (앞, 아래)를 방향 view 의 화면 좌표로."""
    ahead, down = MOTION.get(pose, (0, 0))
    return {RIGHT: (ahead, down), LEFT: (-ahead, down), FRONT: (0, down + ahead), BACK: (0, down - ahead)}[view]


def demon_sprite(pose, view):
    """(파트 1, 프레임, dx, dy) — 발바닥을 사람 발바닥(원점 + 2)에 + 그 자세의 움직임."""
    dx, dy = frame_offsets()[frame_of(pose, view) - BODY_BASE]
    mx, my = motion(pose, view) if pose not in FALLS else (0, 0)
    return [BODY_DIR, frame_of(pose, view), (dx + mx) & 0xFF, (dy + my) & 0xFF]


CLAW_SEQS = (4, 5, 6)  # 기본공격 세 단 = 할퀴기 (★ 2026-10-14: 그 그림은 호스트 이펙트 라이브러리 — 사람의 무기 궤적은 뺀다)

# ★★ 2026-10-22 (사용자: *"기존 공격 모션에서 무기가 사라졌으니 할퀴는 기본공격 모션이 여러 프레임으로 새로 생성해야"*): 할퀴기 세 단은 그 직업
#   자신의 공격 프레임(시퀀스 4 · 5 · 6 — 머리 · 몸통 · 팔 · 다리의 자리가 방향마다 맞다)을 무기 없이 다시 엮은 동작이다. 단마다
#   [(원본 시퀀스, 원본 프레임, 앞으로 px, 아래로 px)] — 길이 = 그 직업의 단 길이(`a.n`: 버서커 5 · 5 · 5 / 나이트 4 · 4 · 4 / 로그 4 · 5 · 3,
#   게임의 `g.w` 가 돌리므로 같아야 한다). 타격 프레임(`HIT_FRAMES` 0 단 1 · 3 / 1 단 2 / 2 단 2)에 팔을 뻗은 프레임 + 뛰어듦이 오고 그 앞은
#   당김(한 걸음 물러섬 · 웅크림), 2 단은 도약(−4) → 내리찍음(+1). 「앞」은 바라보는 쪽(오른 +x · 왼 −x · 앞모습 +y · 뒷모습 −y).
#   참격 그림(붉은 발톱 자국)은 호스트 FXT 가 그 타격 프레임에 그린다(`malice_wings.claw_frames`).
CLAW_PLAN = {
    0: [[(4, 0, -1, 0), (4, 2, 3, 0), (5, 0, 1, 0), (5, 2, 3, 0), (5, 3, 3, 0)],
        [(6, 0, -1, 1), (6, 1, 0, 0), (6, 2, 4, 0), (6, 3, 4, 0), (6, 4, 4, 0)],
        [(4, 0, 0, 1), (4, 1, 1, -4), (4, 3, 3, 1), (4, 4, 3, 1), (4, 4, 3, 1)]],
    1: [[(4, 0, -1, 0), (4, 2, 3, 0), (5, 1, 1, 0), (5, 2, 3, 0)],
        [(6, 0, -1, 1), (4, 1, 0, 0), (5, 2, 4, 0), (5, 3, 4, 0)],
        [(6, 0, 0, 1), (6, 1, 1, -4), (6, 2, 3, 1), (6, 3, 3, 1)]],
    2: [[(4, 0, -1, 0), (4, 2, 3, 0), (5, 0, 1, 0), (5, 2, 3, 0)],
        [(6, 0, -1, 1), (6, 1, 0, 0), (5, 2, 4, 0), (5, 3, 4, 0), (5, 4, 4, 0)],
        [(6, 0, 0, 1), (6, 1, 1, -4), (6, 2, 3, 1)]],
}
STAGE_LENS = {0: (5, 5, 5), 1: (4, 4, 4), 2: (4, 5, 3)}  # the shipped stage sequences' frame counts (= a.n[0..3])


def claw_copy(cls, stage, seqs):
    """할퀴기 단 `stage`(0..2)의 시퀀스 — `CLAW_PLAN` 의 원본 프레임(무기 · 보조손 · 이펙트 뺌)에 뛰어듦 (앞, 아래)를 더한 것.
    `seqs` = 그 직업의 배포본 시퀀스(원본 4 · 5 · 6 을 읽는다)."""
    plan = CLAW_PLAN[cls][stage]
    if len(plan) != len(seqs[4 + stage]):
        raise ValueError(f"class {cls} stage {stage}: the plan has {len(plan)} frames, the game's stage {len(seqs[4 + stage])}")
    out = []
    for src_seq, src_f, ahead, down in plan:
        directions = []
        for view in range(4):
            mx, my = {RIGHT: (ahead, down), LEFT: (-ahead, down), FRONT: (0, down + ahead), BACK: (0, down - ahead)}[view]
            sprites = []
            for sp in seqs[src_seq][src_f][view]:
                if sp[0] & 0x7F in (4, 6, 7):
                    continue
                dx = sp[2] - 256 if sp[2] > 127 else sp[2]
                dy = sp[3] - 256 if sp[3] > 127 else sp[3]
                sprites.append([sp[0], sp[1], (dx + mx) & 0xFF, (dy + my) & 0xFF])
            directions.append(sprites)
        out.append(directions)
    return out


def demon_copy(cls, seq, frames):
    """★★ 2026-10-22 악의 = 클레르 자신 + 날개: 사람 시퀀스의 사본에서 무기(6) · 보조손(7)만 뺀다 — 머리 · 몸통 · 팔 · 다리는
    입은 장비 · 아바타의 파츠 그대로(날개는 호스트 효과 `demon_body`). 할퀴기 단(CLAW_SEQS)은 이펙트(4)도 뺀다 — 실제 사본은 `claw_copy`."""
    return [[[list(sp) for sp in sprites
              if sp[0] & 0x7f not in ({4, 6, 7} if seq in CLAW_SEQS else {6, 7})]
             for sprites in directions] for directions in frames]


TRANSFORM_HEIGHT = [2, 2, 2, 2, -3, -8, -13, -18, -22, -25, -26, -26, -26, -26, -26, -26, -20, -13, -6, 0]
TRANSFORM_SPIN = {10: FRONT, 11: LEFT, 12: BACK, 13: RIGHT, 14: FRONT, 15: LEFT}


def transform_seq(stand):
    """변신(STATE 1) 20 프레임 = 서기 시퀀스 0 프레임 0 의 사람 그림: 웅크림 → 도약 → 공중 회전(네 방향을 돈다) → 착지."""
    assert len(TRANSFORM_HEIGHT) == TRANSFORM_FRAMES
    base = stand[0]
    out = []
    for k, h in enumerate(TRANSFORM_HEIGHT):
        directions = []
        for d in range(4):
            src = base[TRANSFORM_SPIN.get(k, d)]
            sprites = []
            for s in src:
                if s[0] & 0x7f in (6, 7):
                    continue
                y = s[3] - 256 if s[3] > 127 else s[3]
                sprites.append([s[0], s[1], s[2], max(-128, min(127, y + h)) & 0xFF])
            directions.append(sprites)
        out.append(directions)
    return out


# ★★ 2026-10-14 (5): 자세 시퀀스 — 변신 시퀀스 바로 뒤에 이 순서로(= `DemonPatch::POSE_*`, 시퀀스 = 변신 + 1 + 종류). 프레임 f = 그
#   스킬의 f 번째 틱(DRAW 조각이 그 틱부터 센다 — 사람 시퀀스와 무관하므로 길이는 자유). 정점 = 예상도(지옥칼날 08 · 어둠심판 09 ·
#   멸화 10 · 용암불 11 · 돌진 05), 사이 = 뼈대(`demon_draft.RIG_POSES`). 길이(앞 여섯) = `DemonPatch::POSE_LENS`(test_demon_look 대조) ·
#   돌진 = TICKS & 7 · 숨쉬기 = (TICKS >> 3) & 3.
CAST_SEQS = [
    ("blade", ["blade_wind"] * 4 + ["blade_thrust"] * 9 + ["blade_wind"] * 3),          # 지옥칼날 켬: 당겨 → 찌름 → 거둠
    ("judge", ["judge_raise"] * 4 + ["judge_up"] * 16 + ["c2_hit"] * 4),               # 어둠심판 켬: 들어 → 주먹 듦(선딜 20) → 내리침
    ("fire", ["fire_gather"] * 10 + ["roar"] * 14),                                     # 멸화: 모음(선딜 10) → 포효(폭발)
    ("lava", ["lava_rise"] * 5 + ["lava_slam"] * 11),                                   # 용암불: 들어 → 땅을 침
    ("follow", ["c1_coil"] * 6 + ["roar"] * 12),                                        # 추종 켬: 웅크려 → 포효
    ("burst", ["c2_coil"] * 3 + ["c2_impact"] * 13),                                    # 추종 끝 폭발: 웅크려 → 내리침
    ("run", ["run0", "run0", "run1", "run1"] * 2),                                       # 돌진 (TICKS & 7) — 두 자세 교대(c1_dash 의 앞모습은 두 팔을 든다)
    ("idle", ["stand", "stand", "idle", "idle"]),                                        # 서기 숨쉬기 ((TICKS >> 3) & 3)
]
POSE_LENS = [len(f) for _n, f in CAST_SEQS[:6]]
# 자세 시퀀스의 몸 움직임 (앞, 아래) — 할퀴기의 MOTION 과 따로(빌린 할퀴기 자세가 제자리에서 그려지게).
POSE_MOTION = {"blade_thrust": (2, 0), "lava_slam": (1, 1), "c2_hit": (0, 1), "c2_impact": (0, 1)}


def pose_sprite(pose, view):
    """자세 시퀀스의 조각 — `demon_sprite` 와 같되 움직임은 `POSE_MOTION`."""
    dx, dy = frame_offsets()[frame_of(pose, view) - BODY_BASE]
    ahead, down = POSE_MOTION.get(pose, (0, 0))
    mx, my = {RIGHT: (ahead, down), LEFT: (-ahead, down), FRONT: (0, down + ahead), BACK: (0, down - ahead)}[view]
    return [BODY_DIR, frame_of(pose, view), (dx + mx) & 0xFF, (dy + my) & 0xFF]


def pose_seq(frames):
    """자세 시퀀스 하나 — 프레임마다 방향 넷(뒤 · 앞 · 왼 · 오른)에 악마 조각 하나."""
    return [[[pose_sprite(pose, d)] for d in (BACK, FRONT, LEFT, RIGHT)] for pose in frames]


def patch_anim(blob, cls):
    """`hero/hero<cls>`(이 파이프라인의 다른 패치 전부 뒤) + 악마 시퀀스 18(MAPPED 순서) + 변신 1 + ★ (5) 자세 8(CAST_SEQS)."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != SHIPPED_SEQS[cls]:
        raise ValueError(f"hero/hero{cls}: {len(seqs)} sequences — expected {SHIPPED_SEQS[cls]} (every other animation patch first)")
    for s in MAPPED[cls]:
        used = {sp[0] & 0x7F for fr in seqs[s] for d in fr for sp in d}
        if not used & {0, 1, 2, 3}:
            raise ValueError(f"hero/hero{cls}: sequence {s} has no body part — MAPPED is wrong")
        seqs.append(claw_copy(cls, s - 4, seqs) if s in CLAW_SEQS else demon_copy(cls, s, seqs[s]))
    seqs.append(transform_seq(seqs[0]))
    for _name, frames in CAST_SEQS:
        source = seqs[2 if _name == "run" else (4 if _name in ("blade", "lava", "burst") else 0)]
        seqs.append([[ [list(sp) for sp in sprites if sp[0] & 0x7f not in (4, 6, 7)]
                       for sprites in source[min(len(source) - 1, f * len(source) // len(frames))]]
                     for f in range(len(frames))])
    out = venom_effect.encode(header, parts, seqs)
    _h, again_parts, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[: SHIPPED_SEQS[cls]] == before, "read back: the shipped sequences moved"
    assert len(again) == SHIPPED_SEQS[cls] + MAPPED_COUNT + 1 + len(CAST_SEQS)
    for k, (_name, frames) in enumerate(CAST_SEQS):
        d = again[pose_seq_no(cls, k)]
        assert len(d) == len(frames)
        for fr in d:
            assert len(fr) == 4 and all(all(sp[0] & 0x7f not in (4, 6, 7) for sp in sprites) for sprites in fr)
    for i, s in enumerate(MAPPED[cls]):
        d = again[SHIPPED_SEQS[cls] + i]
        assert len(d) == len(before[s]), f"sequence {s}: the demon copy has {len(d)} frames, the human {len(before[s])}"
        for fr in d:
            for sprites in fr:
                body = [sp for sp in sprites if sp[0] & 0x7F != 4]
                assert all(sp[0] & 0x7f not in (6, 7) for sp in body), body
                for sp in sprites:
                    if sp[0] & 0x80:
                        assert sp[1] in again_parts[sp[0] & 0x7F][0], f"sequence {s}: a mirrored sprite the flip list lacks {sp}"
    t = again[transform_seq_no(cls)]
    assert len(t) == TRANSFORM_FRAMES
    for fr in t:
        for sprites in fr:
            for sp in sprites:
                if sp[0] & 0x7F == BODY_DIR:
                    assert sp[1] < BODY_BASE, "the transform uses the human body frames only"
                if sp[0] & 0x80:
                    assert sp[1] in again_parts[sp[0] & 0x7F][0], "a mirrored sprite the flip list lacks"
    return out


def demon_seq(cls, seq):
    """사람 시퀀스 → 악마 시퀀스 번호 (없으면 None) — 호스트 표와 같은 규칙."""
    if seq in MAPPED[cls]:
        return SHIPPED_SEQS[cls] + MAPPED[cls].index(seq)
    return None


def transform_seq_no(cls):
    return SHIPPED_SEQS[cls] + MAPPED_COUNT


def pose_seq_no(cls, kind):
    """자세 시퀀스 번호 = 변신 + 1 + 종류 (`DemonPatch::pose_seq`)."""
    return transform_seq_no(cls) + 1 + kind


# ── 문구 · 아이콘 ─────────────────────────────────────────────────────────────────────────────────
def patch_bsk(rows):
    """`t/bsk.tdf` [18] 대지분노 → 악의 · [19] 설명. 배포본인지 확인하고 쓴다. ★★ 2026-10-16: + the 악의 versions' rows (26..37)."""
    if len(rows) != BSK_SHIPPED_ROWS:
        raise SystemExit(f"{BSK_TDF}: {len(rows)} rows, expected {BSK_SHIPPED_ROWS} (KTF 1.0.7)")
    rows = list(rows)
    for k, (old, new) in BSK_REPLACE.items():
        if rows[k] != old.encode("euc-kr"):
            raise SystemExit(f"{BSK_TDF}: row {k} is not {old!r} — is this the shipped table?")
        rows[k] = new.encode("euc-kr")
    return rows + demon_skin.bsk_append_rows()


ICON_ART = [
    "..K........K",
    ".KbK......Kb",
    "..KbKHHHHKbK",
    "..HKPPPPPPKH",
    ".HKPpPPPPpPKH"[:12],
    "HKPPRRPPRRPKH"[:12],
    "HKPPrRPPrRPKH"[:12],
    "HKPPPPDDPPPKH"[:12],
    ".HKPPDPPDPKH"[:12],
    ".HKPGPGPGPKH"[:12],
    "..HKPPPPPKH.",
    "...HKKKKKH..",
]
# ★ 2026-10-14 밤 3 차: 예상도 속 악마의 얼굴색 (밝은 파랑 피부 · 붉은 눈 · 검은 갈기) — 시트에 있는 가장 가까운 색으로 들어간다
ICON_COLOURS = {"K": H("#14122a"), "b": H("#5a4c46"), "P": H("#4e5cba"), "p": H("#8292e2"), "R": H("#ec4030"), "r": H("#ff965a"),
                "D": H("#2c3478"), "G": TOOTH, "H": H("#282024")}


def _nearest(palette, rgb):
    best, bi = None, 0
    for i, c in enumerate(palette):
        if i == 0:
            continue
        d = sum((a - b) ** 2 for a, b in zip(c, rgb))
        if best is None or d < best:
            best, bi = d, i
    return bi


def patch_icon(mph, mpd):
    """`img/bico` 프레임 9(대지분노) ← 악의 아이콘, 있는 색 가운데 가장 가까운 색으로(팔레트는 안 건드린다)."""
    sheet = cs.sheet_decode(mph, mpd, ICON_SHEET)
    if len(sheet.frames) != ICON_FRAMES:
        raise ValueError(f"{ICON_SHEET}: {len(sheet.frames)} frames — expected {ICON_FRAMES}")
    old = sheet.frames[ICON_AT]
    if (old.w, old.h) != (ICON, ICON):
        raise ValueError(f"{ICON_SHEET} frame {ICON_AT}: {old.w}×{old.h}")
    px = [[0] * ICON for _ in range(ICON)]
    for y, row in enumerate(ICON_ART):
        assert len(row) == ICON
        for x, ch in enumerate(row):
            px[y][x] = 0 if ch == "." else _nearest(sheet.palette, ICON_COLOURS[ch])
    frames = list(sheet.frames)
    frames[ICON_AT] = cs.Frame(ICON, ICON, px, flags=old.flags, lead=old.lead, extra=old.extra)
    # ★★ 2026-10-16: the six 악의 skill icons after the shipped thirteen (the game makes `cf.i` from every frame, `cf.r()V`)
    for j in range(len(demon_skin.SKIN)):
        frames.append(cs.Frame(ICON, ICON, demon_skin.icon_pixels(sheet.palette, j), flags=old.flags, lead=old.lead, extra=old.extra))
    out = cs.Sheet(sheet.palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out, any_head=True)
    back = cs.sheet_decode(new_mph, new_mpd, ICON_SHEET)
    for k, (a, b) in enumerate(zip(back.frames, sheet.frames)):
        if k != ICON_AT:
            assert (a.w, a.h, a.pixels) == (b.w, b.h, b.pixels), f"{ICON_SHEET} frame {k} moved"
    assert back.palette == sheet.palette
    assert len(back.frames) == ICON_FRAMES_BUILT and all((f.w, f.h) == (ICON, ICON) for f in back.frames[ICON_FRAMES:])
    return new_mph, new_mpd


# ── 검사 · 미리보기 ───────────────────────────────────────────────────────────────────────────────
def cmd_check(zip_path):
    jar = cs.open_jar(zip_path)
    m = members()
    sheet = cs.sheet_decode(m[BODY_MEMBERS[0]], m[BODY_MEMBERS[1]], BODY_STEM, compressed_from=COMPRESSED_FROM)
    last = last_frame_start(m[BODY_MEMBERS[0]])
    print(f"{BODY_MEMBERS[0]}: {len(sheet.frames)} frames (signed-byte limit {FRAME_LIMIT}) · {len(sheet.palette)} colours · "
          f"{len(m[BODY_MEMBERS[1]])} B mpd · last frame at {last}")
    for cls in range(3):
        try:
            blob = jar.read(f"hero/hero{cls}")
        except KeyError:
            continue
        _h, _p, seqs = venom_effect.decode(blob)
        print(f"hero/hero{cls}: {len(seqs)} sequences in the jar (shipped-before-this {SHIPPED_SEQS[cls]}, MAPPED {len(MAPPED[cls])}, "
              f"transform 1, poses {len(CAST_SEQS)})")
    print("bsk:", {k: v[1] for k, v in BSK_REPLACE.items()})
    return 0


def cmd_preview(out_png, zip_path):
    from PIL import Image, ImageDraw

    pics = [Image.open(os.path.join(SRC_DIR, f"demon_{k}.png")).convert("RGBA") for k in range(FRAMES)] if os.path.isdir(SRC_DIR) else None
    if pics is None:
        pics = []
        for cv in draft_frames():
            im = Image.new("RGBA", (cv.w, cv.h))
            for y in range(cv.h):
                for x in range(cv.w):
                    if cv.px[y][x] is not None:
                        im.putpixel((x, y), tuple(cv.px[y][x]) + (255,))
            pics.append(im)
    W, H = pics[0].size
    scale = 4
    cols, rows = 4, len(POSES) + 1
    img = Image.new("RGB", (cols * (W + 2) * scale + 160, rows * (H + 2) * scale), (27, 29, 35))
    dr = ImageDraw.Draw(img)
    for i, pic in enumerate(pics):
        r, c = (i // 4, i % 4) if i < len(POSES) * 4 else (len(POSES), i - len(POSES) * 4)
        img.paste(pic.resize((W * scale, H * scale), Image.NEAREST), (c * (W + 2) * scale, r * (H + 2) * scale), pic.resize((W * scale, H * scale), Image.NEAREST))
    for i, p in enumerate(POSES + ["fall"]):
        dr.text((cols * (W + 2) * scale + 6, i * (H + 2) * scale + 4), p, fill=(255, 255, 0))
    if os.path.exists(zip_path):
        jar = cs.open_jar(zip_path)
        imph, impd = patch_icon(jar.read(ICON_SHEET + ".mph"), jar.read(ICON_SHEET + "_0.mpd"))
        icon = cs.sheet_rgba(cs.sheet_decode(imph, impd, ICON_SHEET))[ICON_AT]
        for y in range(ICON):
            for x in range(ICON):
                p = icon[2][y][x]
                if p[3]:
                    X, Y = cols * (W + 2) * scale + 60 + x * 6, 4 + y * 6
                    dr.rectangle([X, Y, X + 5, Y + 5], fill=p[:3])
    img.save(out_png)
    print(out_png, img.size)
    return 0


def main():
    args = sys.argv[1:]
    if args and args[0] == "draft":
        return cmd_draft(force="--force" in args)
    if args and args[0] == "preview" and len(args) in (2, 3):
        return cmd_preview(args[1], args[2] if len(args) == 3 else cs.DEFAULT_ZIP)
    if args and args[0] == "check":
        return cmd_check(args[1] if len(args) == 2 else cs.DEFAULT_ZIP)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
