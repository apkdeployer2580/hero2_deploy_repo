#!/usr/bin/env python3
"""스킬 특화 장비(신규 아이템 2차) + 1차 직업간 전수 밸런스 모델 — v2 (2026-10-14, 사용자: *"패시브, 야수 스킬까지 전부 포함 다 포함해서 깊게 분석해줘."*)

    python tools_re/skill_gear_balance.py <hero2_ktf.zip> [--md HERO2_SKILL_GEAR_BALANCE.md]

적 · 템플릿 · 명중/치명/회피 · 받는 피해의 식은 `guardian_balance.py`(GB). 그 위에 **직업 스킬 전부 · 패시브 · 상시 버프 · 각성기 · 야수**를
넣는다. 근거는 셋이다 — ① 디스어셈블한 식(아래 표, 출처 = HERO2_GUEST_RE.md 와 2026-10-14 조사), ② **probe 실측**
(`tools_re/class_dps_probe.py` · `tools_re/class_rotation_probe.py --spam`), ③ 가정(무리의 몰림 · 광역 대상 수). 보고서 §0 이 모델을 실측과 대조한다.

── 확정 사실 (식 · 실측) ─────────────────────────────────────────────────────────────────────────────────────────────
  타격 경로      d = max(공격 − 몬스터 방어, 0) → 속성 → 직업 함수(스킬 ×1.5 SkillPower · 근력단련 +3 %/Lv · 나이트 extras · 신의가호)
                → 치명(두 번 굴림) → 최소 3. 「공격력 기준」 스킬(방패치기 · 발도 · 일풍격 · 개화 · 천공의검)은 공격 × 계수를 넘겨 방어를 한 번 뺀다.
  공격력 a.h     버서커 힘' × (10 + 힘단련 3)/10 = ×1.3 (실측 str_term 258 @ 힘 199) · 나이트 힘' × 0.7 · 로그 (힘' + 민첩')/2 · + 무기 굴림
  방어           버서커 힘'/4 · 나이트 방패 + 힘'/3 · 로그 (힘' + 민첩')/4 + 방어구
  HP             (체력' + LV) × 12 · 나이트 × (13 + 체력단련 5) = ×18
  SP             최대 (의지' + LV) × 6 · ★ 자연 회복 의지' × 1.25 / 초(실측) · 콤보 2 단부터 한 단에 a.j(무기 최대/5 · 로그 = 실측 맞춤)를 얻는다
  기본공격       ★ 실측: 버서커 2.0 타/초 × 단 보너스 1.1 · 나이트 2.3 타/초 · 로그 3.9 타/초 × 콤보 1.45 (쌍단검) — OK 박자와 무관
  쿨             ★ 실측: 시전 간격 ≈ 쿨 틱 / 16 + 0.15 초(쿨 시계 0.8)
  몬스터         ★ 실측: 붙은 몬스터는 0.96 초마다 공격 · 받는 피해 = 공격 × U(0.9,1.1) − 방어 · 치명 clamp(20 + L − LV, 8, 50) % ×2.5
  패시브         버서커 집중력 Lv3 빗나감 −9 · 근력단련 Lv3 스킬 +9 % · 힘단련 Lv3 ×1.3 · 나이트 체력단련 Lv5 HP ×18 · 오라수련 Lv5(가호 · 철괴 ·
                 치유오라) · 방패수련(막기 ≈ 8 % 바닥 × 방패 방어 — 거의 0) · 로그 동체시력 Lv3 회피 +6 · 직감 Lv3 치명 +4 · 암살 Lv5(첫 타 +100 %
                 방어 무시) · 심안 · 저항 · 급소공부(기절) 는 피해 · 생존 수치 밖
  야수           모든 직업이 갖는다 — 레이든 계열 마지막 형태(암살자 · 지옥불 · 대지분노 · 학살화염) Lv5 · 야수 레벨 = 플레이어 레벨 가정.
                 지옥불 = 링 위 몬스터마다 7 타 × ((LV + 30 + 2·지력')·4 − 방어) · 4.5 초 · SP 59 / 암살자 = 둘레 넷 1.62 틱(실측) × ((2.7·LV + 20 + 2·지력')·5
                 − 방어) · 4 초 · SP 45 / 학살화염 = ±4 칸에 흩뿌림 — 붙은 한 마리에는 안 닿는다(실측), 무리에서 한 마리 2 타 가정 · 8 초 · SP 95.
                 야수의 시전은 플레이어의 동작을 쓰지 않고 SP 만 나눠 쓴다. 실측(지력 12 · 야수 LV 46): 지옥불 2,748/시전 = 식 2,758
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import demon_balance as DB  # noqa: E402
import guardian_balance as GB  # noqa: E402

FPS = 20.0
CD_CLOCK = 0.8
N_PACK = 6
ALIVE = (N_PACK + 1) / 2
ATTACKERS = min(4.0, ALIVE)
MON_INTERVAL_S = 48 * 0.02  # ★ 실측 0.96 초 (GB 의 45 = probe 프레임을 20 fps 로 나눈 단위 실수)
MON_RATE = 1.0 / MON_INTERVAL_S
GB.CALIB["monster_attack_frames"] = MON_INTERVAL_S * FPS  # demon_balance 도 이 상수를 읽는다(이 프로세스 안에서만)
# ★ GB 의 상수 셋을 코드 값으로(조사 2026-10-14 — guardian_balance.py 는 그대로 둔다): 버서커 a.h ×1.3(힘단련 최대 3) · 버서커 방어 힘'/4 · 나이트 HP ×18
GB.CALIB["str_term"][0] = (1.3, 0.0)
GB.CALIB["def_str"][0] = (0.25, 0.0)
GB.CALIB["hp_mul"][1] = 18
SP_REGEN_PER_WIS = 1.25
BASIC = {0: dict(hits=2.0, mul=1.1, stages=4), 1: dict(hits=2.3, mul=1.0, stages=3), 2: dict(hits=3.9, mul=1.45, stages=5)}
ROGUE_AJ = 0.225  # 로그 a.j = 두 단검 최대 합 × 0.225 (실측 29 @ 60 + 69 — 식은 미확인)
BAG_ITEMS = 40 + 7
AWAKEN = 1.4
ASS_HITS = 1.62  # 야수 암살자: 한 번에 실측 1.62 틱 (식은 2 틱 — 4 방향 판정 · 틱 간격)
SKY_MEASURED = 101.0 / 130.8  # 천공의검: 식(다섯 타) 대비 실측 — 붙은 한 마리에 한 타가 안 닿았다(실측 17,262 @ 힘 170 · 지력 12 · 의지 12)
BEAST = True  # 모든 직업이 야수를 쓴다(레이든 계열)
# ── 1차 조정 손잡이 (착수 순서 0 — HERO2_NEW_ITEM_DRAFT.md §7) ─ 기본값 = 2026-10-14 사용자 결정 (게임 코드와 같다)
RAGE_DEALT = 1.20  # (c) 버서커 분노태세 Lv5 주는 피해 — BerserkerPatch::RAGE_DEALT_BASE 430 → (430 + 50)/400 (전 1.30)
RAGE_TAKEN = 1.10  # 분노태세 Lv5 받는 피해 (그대로)
VENOM_PER_S = 4.5  # 맹독감염 중 중독 = 공격력 × 이것 / 초 — 실측 3.0 은 틱 200 % 일 때, 300 % 롤백(VenomPatch 42 · 30)으로 ×1.5
KNIGHT_AJ_MUL = 1.0  # (b) 나이트 콤보 SP 획득 a.j 배율 (안 씀 — 후보였다)
KNIGHT_SKILL_SP_MUL = 0.5  # ★ 2026-10-20 0.75 → 0.5 (밸런스 전수 조사 ②) · (b) 나이트 공격 스킬 SP 배율 — CombatPatch::SP_CUT_KINDS (검기 · 둔기강타 · 방패치기 · 발도)
KNIGHT_SP_CUT_KINDS = (4, 5, 6, 9)


def period(cd_ticks, cd_mul=1.0, extra=0.0):
    """쿨 틱 → 시전 간격(초). 실측: 쿨 / 16 + 0.15 (extra = 평타가 쿨을 앞당기는 초당 몫)."""
    return cd_ticks * cd_mul / (FPS * 0.8 + extra * FPS) + 0.15


# ── 신규 아이템 2차 — 이 표가 초안의 수치다(HERO2_NEW_ITEM_DRAFT.md 와 같은 값) ─────────────────────────────────────
#   (코드, 가칭, 칸, 능력치를 빌릴 가디언 아이템, 효과)
NEW = {}


def new(code, name, slot, base, **eff):
    NEW[code] = dict(code=code, name=name, slot=slot, base=base, eff=eff)


# A 격분 무한 (버서커) — 늘리는 조각은 방어 칸(투구·신발·갑옷·장신구), 무기는 격분 중 피해
new("A1", "분노의 투구", "h", (5, 28), fury_hit=0.25)
new("A2", "광전사의 발걸음", "s", (9, 53), fury_hit=0.15)
new("A3", "상처의 갑주", "a", (6, 53), fury_kill=1.0, fury_taken=0.3, fury_taken_mul=0.85)
new("A4", "꺼지지 않는 불씨", "acc", (12, 20), fury_keep=1, fury_keep_gauge=0.5, fury_blast_cd=20.0)
new("A5", "멈추지 않는 도끼", "w", (1, 29), fury_start=0.5, fury_dmg=0.6)
new("A5'", "멈추지 않는 대검", "w", (0, 33), fury_start=0.5, fury_dmg=0.6)
# G 반격 (버서커)
new("G1", "응징의 대검", "w", (0, 33), counter_mul=0.8)
new("G2", "가시 판금", "a", (6, 53), counter_taken_mul=0.75)
new("G3", "도발자의 투구", "h", (5, 28), counter_forever=1, taunt_taken_mul=0.90)
new("G4", "되갚는 손", "g", (8, 51), counter_mul=0.3, counter_heal=0.06)
new("G5", "울림의 장화", "s", (9, 53), counter_aoe=0.25)
new("G6", "피의 서약", "acc", (11, 20), counter_heal=0.06)
# B 쿨감 스킬딜러 (공용) — 무기는 안 바꾼다
new("B1", "시간도둑", "acc", (12, 20), cd_basic=(10, 1.0))
new("B2", "흐름의 장화", "s", (9, 54), sp_gain=0.5)
new("B3", "집중의 관", "h", (5, 29), sp_mul=179 / 256)
new("B4", "시간의 외투", "a", (6, 52), cd_mul=0.85)
new("B5", "연환의 장갑", "g", (8, 52), skill_dmg=0.20)
# C 악의 (공용)
new("C1", "악마의 손톱", "g", (8, 52), claw_spd=0.2, claw_dmg=0.2)
new("C2", "지옥 발굽", "s", (9, 54), move=0.25)
new("C3", "검은 비늘", "a", (6, 52), claw_dmg=0.3)
new("C4", "뒤틀린 뿔", "h", (5, 29), malice_dur=15.0)
new("C5", "풀린 목줄", "acc", (11, 20), malice_kill=1.0)
# D 살상연희 (로그) — 무기만 타수, 방어구·장신구는 피해만 (사용자 2026-10-14)
new("D1", "백 번째 칼날", "w", (3, 34), sl_hits=5, dmg={10: 0.5})  # ★ 2026-10-14 사용자 「고쳐」: +5타 · 살상연희 +50 % (응보의 상위판)
new("D2", "무모한 칼춤", "w2", (3, 34), sl_hits=12, sl_noinv=1)
new("D3", "피의 연쇄", "w2", (3, 34), sl_hits=6, sl_chain=8, sl_chain_per=2)
new("D4", "무희의 두건", "h", (5, 29), sl_dmg=0.20)
new("D5", "무희의 옷", "a", (6, 52), sl_dmg=0.20)
new("D6", "칼춤 장갑", "g", (8, 52), sl_dmg=0.40)  # ★★ 2026-10-16 +20 → +40 %
new("D7", "잔영의 신", "s", (9, 54), sl_dmg=0.20)
new("D8", "마지막 춤", "acc", (12, 20), sl_cap=3, sl_mul=5.0)  # ★★ 2026-10-17 사용자: 3타 · 피해 5배 (옛 마지막 타 ×3)
# H 스킬을 바꾸는 무기 (사용자 2026-10-14 — 쌍월은 원안 그대로, 검풍 30 %)
new("H1", "쌍월", "w", (2, 29), iaido_double=1)
new("H2", "검풍", "w", (2, 28), wave_passive=0.30)
new("H3", "회오리 송곳니", "w", (3, 34), ilpung_chain=3)
# E 스킬별 특화 — 모델은 피해 · 대상 수만
new("E2", "휘두르기 도끼", "w", (1, 29), dmg={4: 1.0}, targets={4: 2})
new("E3", "둔기강타 둔기", "w", (4, 33), dmg={5: 1.0}, targets={5: 1})
new("E4", "검기 장갑", "g", (8, 51), dmg={4: 0.3}, sp_kind={4: 0.5})  # ★ 2026-10-14 사용자 「고쳐」: 검기 +30 % · 검기 SP −50 %
new("E7", "바람날개 신발", "s", (9, 54), dmg={15: 0.3}, targets={15: 1})  # ★★ 2026-10-17 이름 (옛 일풍격 신발)
# F 재미
new("F1", "중력의 갑주", "a", (6, 52), gravity=0.006)  # 경갑 (사용자 2026-10-14) · ★★ 2026-10-17 사용자: 0.4 → 1 %

MUL_KEYS = {"fury_taken_mul", "counter_taken_mul", "taunt_taken_mul", "sp_mul", "cd_mul", "sl_mul"}


def merge(codes):
    """새 아이템 효과 합 — 곱하는 것은 곱하고 나머지는 더한다. 같은 아이템 둘은 한 번."""
    out = dict(dmg={}, targets={}, cd_kind={}, sp_kind={})
    for c in dict.fromkeys(codes):
        for k, v in NEW[c]["eff"].items():
            if k in ("cd_kind", "sp_kind"):
                for kk, vv in v.items():
                    out[k][kk] = out[k].get(kk, 1.0) * vv
            elif k in ("dmg", "targets"):
                for kk, vv in v.items():
                    out[k][kk] = out[k].get(kk, 0) + vv
            elif k in MUL_KEYS:
                out[k] = out.get(k, 1.0) * v
            elif k == "cd_basic":
                out[k] = v
            elif k == "sl_cap":
                out[k] = min(out.get(k, v), v)
            else:
                out[k] = out.get(k, 0) + v
    return out


def g(ne, k, default=0.0):
    return ne.get(k, default)


# ── 1차 기준 장비 (GB.LOADOUTS + 직업마다 다른 1차 갈래 하나) ───────────────────────────────────────────────────────
FIRST = {
    "버서커 · 평타형(오딘 · 독사 · 비호 · 용의 심장)": (0, GB.LOADOUTS["버서커"][2]),
    "버서커 · 폭풍강타형(제스 · 파쇄자 · 비호 · 용의 심장)": (0, {"w": (1, 29), "h": (5, 28), "a": (6, 53), "g": (8, 53), "s": (9, 54), "acc": (11, 20)}),
    "나이트 · 검기형(이안 · 성채 · 독사 · 비호 · 용의 심장)": (1, GB.LOADOUTS["나이트"][2]),
    "나이트 · 방패치기형(수호검 · 격돌의 방패)": (1, {"w": (2, 29), "sh": (10, 29), "h": (5, 28), "a": (6, 53), "g": (8, 52), "s": (9, 54), "acc": (11, 20)}),
    "로그 · 살상연희형(응보 · 터줏대감 · 독사 · 비호)": (2, GB.LOADOUTS["로그"][2]),
    "로그 · 일격형(이안 대신 응보 + 그림자 걸음)": (2, {"w": (3, 34), "w2": (3, 33), "h": (5, 29), "a": (6, 52), "g": (8, 52), "s": (9, 52), "acc": (11, 20)}),
    "나이트 · 둔기형(심판의 철퇴 · 파쇄자의 손갑)": (1, {"w": (4, 33), "sh": (10, 28), "h": (5, 28), "a": (6, 53), "g": (8, 53), "s": (9, 54), "acc": (11, 20)}),
}

# ── 2차 빌드: 1차 기준(첫 갈래)에서 칸을 바꾼다 ──────────────────────────────────────────────────────────────────
BUILDS = [
    # (이름, 직업, 기준 1차 번호(FIRST 순서), {칸: 새 코드})
    ("A 격분 — 투구 · 신발 (싸우는 동안 무한의 최소)", 0, 0, {"h": "A1", "s": "A2"}),
    ("A 격분 — 투구 · 신발 · 도끼", 0, 0, {"h": "A1", "s": "A2", "w": "A5"}),
    ("A 격분 — 투구 · 신발 · 대검", 0, 0, {"h": "A1", "s": "A2", "w": "A5'"}),
    ("A 격분 — 투구 · 신발 · 갑옷 · 도끼", 0, 0, {"h": "A1", "s": "A2", "a": "A3", "w": "A5"}),
    ("A 격분 — 투구 · 신발 · 불씨", 0, 0, {"h": "A1", "s": "A2", "acc": "A4"}),
    ("A 격분 — 다섯", 0, 0, {"h": "A1", "s": "A2", "a": "A3", "w": "A5", "acc": "A4"}),
    ("G 반격 — 여섯", 0, 0, {"w": "G1", "a": "G2", "h": "G3", "g": "G4", "s": "G5", "acc": "G6"}),
    ("G 반격 — 대검 · 갑옷 · 장갑 · 목걸이", 0, 0, {"w": "G1", "a": "G2", "g": "G4", "acc": "G6"}),
    ("B 쿨감 다섯 · 버서커", 0, 0, {"acc": "B1", "s": "B2", "h": "B3", "a": "B4", "g": "B5"}),
    ("B 쿨감 다섯 · 나이트", 1, 2, {"acc": "B1", "s": "B2", "h": "B3", "a": "B4", "g": "B5"}),
    ("B 쿨감 다섯 · 로그", 2, 4, {"acc": "B1", "s": "B2", "h": "B3", "a": "B4", "g": "B5"}),
    ("B1 시간도둑 하나 · 버서커", 0, 0, {"acc": "B1"}),
    ("B1 시간도둑 하나 · 나이트", 1, 2, {"acc": "B1"}),
    ("B1 시간도둑 하나 · 로그", 2, 4, {"acc": "B1"}),
    ("H1 쌍월 (발도 두 번 · SP 한 번)", 1, 2, {"w": "H1"}),
    ("H1 쌍월 + B 넷", 1, 2, {"w": "H1", "acc": "B1", "a": "B4", "h": "B3", "s": "B2"}),
    ("H2 검풍 (검기 패시브 30 %)", 1, 2, {"w": "H2"}),
    ("H2 검풍 + B2 · B3", 1, 2, {"w": "H2", "s": "B2", "h": "B3"}),
    ("D 칼날 (주무기만)", 2, 4, {"w": "D1"}),
    ("D 칼날 + 무모한 칼춤 (무적 없음)", 2, 4, {"w": "D1", "w2": "D2"}),
    ("D 칼날 + 피의 연쇄", 2, 4, {"w": "D1", "w2": "D3"}),
    ("D 칼날 + 무모 + 피해 다섯", 2, 4, {"w": "D1", "w2": "D2", "h": "D4", "a": "D5", "g": "D6", "s": "D7", "acc": "D8"}),
    ("D 칼날 + 연쇄 + 피해 다섯", 2, 4, {"w": "D1", "w2": "D3", "h": "D4", "a": "D5", "g": "D6", "s": "D7", "acc": "D8"}),
    ("D 피해 다섯만 (응보 · 터줏대감 유지)", 2, 4, {"h": "D4", "a": "D5", "g": "D6", "s": "D7", "acc": "D8"}),
    ("H3 회오리 송곳니 (일풍격 연쇄 3)", 2, 4, {"w": "H3"}),
    ("E2 휘두르기 도끼 · 버서커 폭풍강타형", 0, 1, {"w": "E2"}),
    ("E3 둔기강타 둔기 · 나이트 둔기형", 1, 6, {"w": "E3"}),
    ("E4 검기 장갑 · 나이트 검기형", 1, 2, {"g": "E4"}),
    ("E7 바람날개 신발 · 로그", 2, 4, {"s": "E7"}),
    ("F1 중력의 갑주 · 버서커 (장비 47 = 가방 40 + 입은 7)", 0, 0, {"a": "F1"}),
    ("F1 중력의 갑주 · 나이트", 1, 2, {"a": "F1"}),
    ("F1 중력의 갑주 · 로그", 2, 4, {"a": "F1"}),
]


# ── 플레이어 · 끄기(패시브 · 버프 · 야수 하나씩 빼 보기) ─────────────────────────────────────────────────────────────
#   각 직업의 「끄기」 항목 — 보고서 §3 이 하나씩 빼서 단일 DPS · 무리 처치 · 여유가 얼마나 변하는지 보여 준다.
SWITCHES = {
    0: ["분노태세", "도발", "근력단련", "힘단련", "집중력", "반격태세", "사자후", "격분", "야수"],
    1: ["신의가호", "철괴", "치유오라", "체력단련", "천공의검", "야수"],
    2: ["맹독감염", "암살", "민첩태세", "직감", "동체시력", "살상연희", "일풍격", "개화", "야수"],
}


def make_player(items, gear, cls, slots, codes_by_slot, level, archetype=None, off=()):
    """1차 칸은 그 아이템 그대로(능력치 + 옵션), 새 칸은 base 의 능력치만 + 새 효과."""
    stat_slots = dict(slots)
    kept = [key for slot, key in slots.items() if slot not in codes_by_slot]
    for slot, code in codes_by_slot.items():
        stat_slots[slot] = NEW[code]["base"]
    o = GB.gear_options(gear, kept, cls)
    p = GB.player(cls, level, items, None, slots=stat_slots, archetype=archetype, opts=o)
    wk = stat_slots["w"]
    p["wmax"] = items[wk]["power"] * GB.WEAPON_BONUS[wk[0]]
    p["wmax2"] = sum(items[stat_slots[s]]["power"] * GB.WEAPON_BONUS[stat_slots[s][0]] for s in ("w", "w2") if s in stat_slots)
    p["aj"] = p["wmax2"] * ROGUE_AJ if cls == 2 else p["wmax"] / 5 * (KNIGHT_AJ_MUL if cls == 1 else 1.0)
    p["kept"] = kept
    p["wtype"] = wk[0]
    p["beast_x2"] = (11, 18) in kept
    p["off"] = set(off)
    if "힘단련" in p["off"]:
        p["atk"] -= 0.3 * p["eff"][0]
    if "체력단련" in p["off"]:
        p["hp"] *= 13 / 18
    if "직감" in p["off"]:
        p["crit"] = GB.clamp(p["crit"] - 4, 8, 90)
    if "동체시력" in p["off"]:
        p["am"] -= 6
    return p, merge(codes_by_slot.values())


def on(p, k):
    return k not in p["off"]


def mob(gt, diff):
    L = gt["L"] + 1
    return dict(L=L, Ls=GB.seen_level(L), hp=(gt["hp"] + 20) * diff, atk=min((gt["atk"] + 3) * diff, 32767), defence=gt["defence"], e=gt["e"])


def hitcrit(p, m):
    """(명중, 치명 배율, 실측 대비 배율). 실측 계수는 시험 몬스터(낮은 레벨 — 빗나감 바닥 8 %, 치명 굴림 바닥 8 %)에서
    나왔으므로 그 평균이 이미 들어 있다 → 계수에 곱하는 것은 「이 몬스터에서의 명중 · 치명 ÷ 시험 몬스터의 것」."""
    focus = 9 if p["cls"] == 0 and on(p, "집중력") else 0
    miss = GB.clamp(p["ak"] + m["Ls"] + m["e"] / 5 - focus, 8, 90)
    # ★ 플레이어 치명 둘째 굴림(0x127c56 · 보스 0x1164b2)은 가디언 보정 훅 밖이다 — 진짜 레벨을 본다(HERO2_GUEST_RE.md §8.54:
    #   「플레이어 치명 2차(좋아지는 쪽) → 안 건드린다」). v2 첫 판(과 guardian_balance.py)은 보정 레벨(Ls)로 셌다 — 8 % 바닥 착시.
    crit = p["crit"] / 100 * GB.clamp(20 + m["L"] - p["level"], 8, 50) / 100
    hit, cx = (100 - miss) / 100, 1 + crit * (GB.CALIB["crit_mul"] - 1)
    floor = 0.92 * (1 + p["crit"] / 100 * 0.08 * (GB.CALIB["crit_mul"] - 1))
    return hit, cx, hit * cx / floor


def int_eff(p):
    return p["eff"][4]


def incoming(p, m, taken_mul):
    """한 마리가 주는 초당 피해 · 초당 맞는 수 · 한 번의 피해."""
    taken = max(m["atk"] - p["defence"], 3) * taken_mul
    mcrit = GB.clamp(20 + m["Ls"] - p["level"], 8, 50) / 100
    evade = GB.clamp(p["am"] - m["L"] - m["e"] / 10, 8, 90) / 100
    rate = (1 - evade) * MON_RATE
    return taken * (1 + mcrit * (GB.CALIB["mon_crit_mul"] - 1)) * rate, rate, taken


# ── 직업 키트 ─────────────────────────────────────────────────────────────────────────────────────────────────────────
#   스킬 = dict(name, kind, one = 한 마리에게 한 번 시전의 피해, pack = 무리(살아 있는 ALIVE 마리)에 한 번 시전의 피해,
#              cd 틱, sp, cast 초 = 플레이어 동작 시간, beast = 야수(동작 안 씀), nuke = 넘침을 자른다)

def kit(p, m, ne, mul=1.0, spd=1.0, boss=False, pack=False):
    cls = p["cls"]
    o = p["opts"]
    _, _, rel = hitcrit(p, m)
    atk = p["atk"]
    DEF = m["defence"]
    grav = 1 + g(ne, "gravity") * BAG_ITEMS
    dm = lambda k: (1 + o["dmg"].get(k, 0) + ne["dmg"].get(k, 0) + (g(ne, "skill_dmg") if k >= BASIC[cls]["stages"] else 0)) * grav
    alive = 1 if boss else ALIVE
    maxsp = p["sp"]
    sk, up = [], []  # up = 유지 비용 (동작 초, 간격 초, SP)
    poison = assassin = 0.0
    if cls == 0:
        buff = (RAGE_DEALT if on(p, "분노태세") else 1.0) * (1.20 if on(p, "도발") else 1.0) * mul
        boost = 1.09 if on(p, "근력단련") else 1.0  # 근력단련 Lv3 (시험 세이브에도 Lv3 — 실측 계수는 이것을 포함)
        defe = DEF / 2 if p["wtype"] == 0 else DEF  # 가르기(대검)의 방어 반감 — 맞은 대상 10 초, 쿨 3 초라 늘 걸려 있다
        d = max(atk - defe, 0)
        spin = 1.5  # 휘두르기 · 폭풍강타: 정면 한 마리 2 타 · 둘레의 다른 몬스터는 평균 1.5 타
        if p["wtype"] == 1:
            per = 2.0 * boost * d * dm(4) * rel * buff
            sk.append(dict(name="휘두르기", kind=4, one=2 * per, pack=(2 + spin * (min(alive + ne["targets"].get(4, 0), 8) - 1)) * per, cd=40, sp=p["aj"], cast=0.41 / spd, speed=True))
        if p["wtype"] == 0:
            per = 2.0 * boost * d * dm(6) * rel * buff
            sk.append(dict(name="가르기", kind=6, one=per, pack=per * min(2.0, alive), cd=60, sp=max(p["wmax"] - 25, 0), cast=0.45))
        per = 3.0 * boost * d * dm(9) * rel * buff
        sk.append(dict(name="폭풍강타", kind=9, one=2 * per, pack=(2 + spin * (min(alive, 4) - 1)) * per, cd=60, sp=p["wmax"] * 1.5, cast=0.42 / spd, speed=True))
        if on(p, "사자후"):
            kwang = (12, 17) in p["kept"]
            pk = 0.0 if boss else GB.clamp(p["level"] + 50 - m["L"] + int_eff(p), 0, 100) / 100  # 진짜 레벨(보수적) · 보스는 즉사 없음
            fail = (4.0 if boss else 1.5) * boost * d * rel * buff
            one = pk * m["hp"] * 0.5 + (1 - pk) * fail  # 즉사 = 남은 HP(평균 ½ — 실측: 최대 HP 를 돌려준다)
            pk_pack = N_PACK * (pk * m["hp"] * 0.75 + (1 - pk) * fail)  # 무리: 들어가며 쓴다 — 여섯 마리가 거의 가득(¾)
            sk.append(dict(name="사자후", kind=10, one=one, pack=pk_pack, cd=120 * (0.5 if kwang else 1), sp=maxsp * 0.8 * (0.5 if kwang else 1), cast=0.93, roar=pk))
        if on(p, "도발"):
            up.append((0.6, 10.0, maxsp * 0.2))  # 도발 다시 걸기(지속 10 초 · SP 사자후의 ¼ = 최대의 20 %)
        if on(p, "반격태세"):
            up.append((0.6, 75.0, maxsp / 6))
        basic = d * BASIC[0]["mul"] * dm(0) * rel * buff
        wave = None
    elif cls == 1:
        ho = on(p, "신의가호")
        d = max(atk - DEF, 0)
        if p["wtype"] == 2 and not g(ne, "wave_passive"):
            per = (max(4 / 3 * atk - DEF, 0) * 9.01 if ho else d * 7.88) * dm(4) * rel
            sk.append(dict(name="검기", kind=4, one=per, pack=per * min(3.0, alive), cd=25, sp=p["wmax"], cast=0.46))
        if p["wtype"] == 4:
            per = d * (15.6 if ho else 7.8) * dm(5) * rel
            sk.append(dict(name="둔기강타", kind=5, one=per, pack=per * alive, cd=35, sp=p["wmax"], cast=0.40))
        per = (max(7.81 * atk - 3.5 * DEF, 0) if ho else max(5.73 * atk - 2.25 * DEF, 0)) * dm(6) * rel
        sk.append(dict(name="방패치기", kind=6, one=per, pack=per * min(2.5 + ne["targets"].get(6, 0), alive), cd=45, sp=maxsp / 8, cast=0.41))
        ia = 2 if g(ne, "iaido_double") else 1
        per = max((26.7 if ho else 14.9) * atk - 2.7 * DEF, 0) * dm(9) * rel * ia  # ★ 실측: 신의가호 발도 26.7 · 없이 14.9 (공격 대비)
        sk.append(dict(name="발도" + (" ×2" if ia == 2 else ""), kind=9, one=per, pack=per * alive, cd=140, sp=maxsp / 4, cast=0.48 + 0.3 * (ia - 1)))
        if on(p, "천공의검"):
            s_, w_, i_ = p["eff"][0], p["eff"][3], p["eff"][4]
            f = 3 * (i_ / 8 + s_ / 12 + w_ / 8) + 2 * (i_ / 6 + s_ / 10 + w_ / 6)
            per = max(atk * f * AWAKEN * SKY_MEASURED - 4.5 * DEF, 0) * rel
            sk.append(dict(name="천공의검", kind=15, one=per, pack=per * alive, cd=800, sp=maxsp / 4, cast=3.0, nuke=True))
        if ho:
            up.append((0.45, 60.0, maxsp / 6))
        if on(p, "철괴"):
            up.append((0.25, 60.0, maxsp / 10))
        basic = d * BASIC[1]["mul"] * dm(0) * rel
        wave = None
        if g(ne, "wave_passive"):
            w1 = (max(4 / 3 * atk - DEF, 0) * 9.01 if ho else d * 7.88) * dm(4) * rel * g(ne, "wave_passive")
            wave = (w1, w1 * min(3.0, alive))
    else:
        d = max(atk - DEF, 0)
        sk.append(dict(name="일격", kind=5, one=4.62 * d * dm(5) * rel, pack=4.62 * d * dm(5) * rel, cd=17, sp=p["aj"], cast=0.27))
        sk.append(dict(name="급소가격", kind=7, one=2.67 * d * dm(7) * rel, pack=2.67 * d * dm(7) * rel, cd=35, sp=maxsp / 17, cast=0.41))
        sk.append(dict(name="단검난무", kind=9, one=28.0 * d * dm(9) * rel, pack=28.0 * d * dm(9) * rel, cd=40, sp=p["wmax2"] * 0.75, cast=0.48))
        if on(p, "살상연희"):
            hits = 8 + g(ne, "sl_hits")
            if g(ne, "sl_cap"):
                hits = min(hits, g(ne, "sl_cap"))  # ★★ 2026-10-17 마지막 춤: 3타 고정
            blow = 10.1 * d * (dm(10) + g(ne, "sl_dmg") * grav) * rel * g(ne, "sl_mul", 1.0)
            sk.append(dict(name="살상연희", kind=10, one=blow * hits, pack=blow * hits, cd=200, sp=p["wmax2"] * 0.75,
                           cast=0.36 * hits, slaughter=hits, blow=blow))
        if on(p, "일풍격"):
            per = max(17.97 * atk - 1.5 * DEF, 0) * dm(15) * rel
            sk.append(dict(name="일풍격", kind=15, one=per * 1.46, pack=per * min(alive * 1.2 + ne["targets"].get(15, 0), 8), cd=140, sp=maxsp / 4, cast=0.84, ilpung=per))
        if on(p, "개화"):
            bloom = max(2.1 * atk - DEF, 0) * 5.5 * (dm(10) + g(ne, "sl_dmg") * grav) * rel
            sk.append(dict(name="개화", kind=16, one=bloom * 30, pack=bloom * 30, cd=800, sp=maxsp / 4, cast=5.0, nuke=True))
        if on(p, "민첩태세"):
            up.append((0.5, 37.5, maxsp / 13))
        if on(p, "맹독감염"):
            up.append((0.4, 75.0, maxsp / 13))
            poison = atk * VENOM_PER_S * (1 + o["venom"])  # ★ 실측: 버프 중 중독 = 공격력 × 3 / 초 (방어 없음)
        if on(p, "암살"):
            # Lv5 첫 타 +100 % (방어 전 피해, 로그 직업 함수만 — 야수 타격엔 없다). 첫 타 = 단일은 기본공격(한 마리를 1 초 안에 잡아 스킬은 대개 쿨 중) ·
            # 무리는 반은 일풍격 한 칸(17.97 × 공격 — 무리에 일풍격으로 들어간다) · 반은 기본공격
            assassin = atk * (0.5 * 17.97 + 0.5 * 1.45 if pack and not boss and on(p, "일풍격") else 1.45)
        basic = d * BASIC[2]["mul"] * dm(0) * rel
        wave = None
    if BEAST and on(p, "야수"):
        L, I = p["level"], int_eff(p)
        x2 = 2 if p["beast_x2"] else 1
        ring = 7 * max((L + 30 + 2 * I) * 4 - DEF, 0) * x2
        sk.append(dict(name="야수 지옥불", kind=-8, one=ring, pack=ring * alive, cd=72, sp=59, cast=0.0, beast=True))
        ass = ASS_HITS * max((2.7 * L + 20 + 2 * I) * 5 - DEF, 0) * x2
        sk.append(dict(name="야수 암살자", kind=-3, one=ass, pack=ass * min(alive, 4) * 0.6, cd=64, sp=45, cast=0.0, beast=True))
        heavy = 2 * max((2.5 * L + 20 + 2 * I) * 3.5 - DEF, 0) * x2
        sk.append(dict(name="야수 학살화염", kind=-11, one=0.0, pack=heavy, cd=128, sp=95, cast=0.0, beast=True))
    return dict(sk=sk, up=up, basic=basic, wave=wave, poison=poison, assassin=assassin)


def rotation(p, m, ne, pack, mul=1.0, spd=1.0, boss=False):
    """지속 회전 — 동작 시간 · SP 예산 안에서 스킬을 「(피해 − 그 시간의 기본공격) ÷ (SP + 그 시간에 못 번 SP)」 순으로 채운다.
    반환 parts = 원천별 DPS(스킬 · 기본공격 · 중독 · 암살)."""
    cls = p["cls"]
    k = kit(p, m, ne, mul, spd, boss, pack)
    sk, basic = k["sk"], k["basic"]
    hit, _, _ = hitcrit(p, m)
    hits = BASIC[cls]["hits"] * spd  # ★ 실측 = 맞은 타(시험 몬스터의 빗나감 8 % 포함)
    stages_s = hits / (2 if cls == 2 else 1)
    gain = p["aj"] * (BASIC[cls]["stages"] - 1) / BASIC[cls]["stages"] * (1 + g(ne, "sp_gain"))
    regen = SP_REGEN_PER_WIS * p["eff"][3]
    sp_mul = p["opts"]["sp"] * ne.get("sp_mul", 1.0)
    cd_mul = ne.get("cd_mul", 1.0)
    cdb = ne.get("cd_basic")
    up_t = sum(c / T for c, T, s in k["up"])
    up_sp = sum(s / T for c, T, s in k["up"])
    n = 1 if boss else (N_PACK if pack else 1)
    ones = []
    for s in sk:
        v = s["pack"] if pack and not boss else s["one"]
        if s.get("nuke") or (pack and s.get("slaughter")):
            v = min(v, n * m["hp"])
        if pack and not boss and s.get("slaughter") and g(ne, "sl_chain"):
            kills = s["slaughter"] * s["blow"] / m["hp"]
            v = min(v + min(g(ne, "sl_chain"), g(ne, "sl_chain_per") * kills) * s["blow"], N_PACK * m["hp"])
        if pack and not boss and s.get("ilpung") and g(ne, "ilpung_chain"):
            q = min(1.0, s["ilpung"] * 1.2 / (0.5 * m["hp"]))
            v *= 1 + sum(q ** j for j in range(1, int(g(ne, "ilpung_chain")) + 1))
        ones.append(v)
    basic_rate = hits * basic
    basic_frac = 1.0 - up_t
    use, periods, inc = [0.0] * len(sk), [1.0] * len(sk), 0.0
    for _ in range(6):
        extra = (hits * basic_frac / 0.92 * hit / cdb[0] * cdb[1] * FPS) if cdb else 0.0  # 평타 10 회마다 1 초 → 초당 앞당기는 틱
        periods = []
        for s in sk:
            if s.get("beast"):
                periods.append(s["cd"] * p["opts"]["beast_cd"] / (FPS / 1.2 + extra))  # ★ 실측: 야수 간격 = 쿨 틱 × 0.06 초
            else:
                cdk = s["cd"] * cd_mul * ne.get("cd_kind", {}).get(s["kind"], 1.0)
                periods.append(max(cdk / (FPS * 0.8 + extra) + 0.15, s["cast"]))
        # SP 예산 = 동작 시간 전부를 평타로 쓸 때의 수입. 스킬의 비용에 「그 시전 시간에 못 번 콤보 SP」가 들어 있으므로
        # 수입을 basic_frac 으로 줄이면 같은 손실을 두 번 센다(첫 판이 그랬다 — 시전이 긴 살상연희가 부당하게 밀렸다).
        inc = stages_s * (1.0 - up_t) * gain + regen - up_sp
        kmul = lambda s: KNIGHT_SKILL_SP_MUL if cls == 1 and s["kind"] in KNIGHT_SP_CUT_KINDS else 1.0
        spc = [s["sp"] * (1.0 if s.get("beast") else kmul(s) * sp_mul * ne.get("sp_kind", {}).get(s["kind"], 1.0)) for s in sk]  # 야수 SP 는 장비 SP 옵션과 무관
        score = lambda i: (ones[i] - sk[i]["cast"] * basic_rate) / max(spc[i] + sk[i]["cast"] * stages_s * gain, 1e-6)
        budget, tleft = inc, 0.95 - up_t
        use = [0.0] * len(sk)
        for i in sorted(range(len(sk)), key=lambda i: -score(i)):
            if ones[i] - sk[i]["cast"] * basic_rate <= 0:
                continue
            cost = (spc[i] + sk[i]["cast"] * stages_s * gain) / periods[i]
            tcost = sk[i]["cast"] / periods[i]
            u = 1.0
            if cost > 0:
                u = min(u, max(budget, 0.0) / cost)
            if tcost > 0:
                u = min(u, max(tleft, 0.0) / tcost)
            use[i] = u
            budget -= u * cost
            tleft -= u * tcost
        basic_frac = 1 - up_t - sum(use[i] * sk[i]["cast"] / periods[i] for i in range(len(sk)))
    casts = sum(use[i] / periods[i] for i in range(len(sk)) if not sk[i].get("beast"))
    rate = hits
    if g(ne, "post_aspd"):
        rate *= 1 + g(ne, "post_aspd") * min(1.0, 3.0 * casts)
    basic_hits = rate * basic_frac * hit / 0.92
    parts = {}
    for i, s in enumerate(sk):
        parts[s["name"]] = use[i] * ones[i] / periods[i]
    parts["기본공격"] = basic_hits * basic
    if k["wave"]:
        parts["검풍(평타 검기)"] = basic_hits * (k["wave"][1] if pack and not boss else k["wave"][0])
    if g(ne, "next_basic"):
        parts["기본공격"] += min(casts, basic_hits) * basic * g(ne, "next_basic")
    if k["poison"]:
        parts["맹독(중독)"] = k["poison"] * (min(2.0, ALIVE) if pack and not boss else 1.0)
    dps0 = sum(parts.values())
    if k["assassin"]:
        r = min(k["assassin"] / m["hp"], 0.9)
        parts["암살(첫 타)"] = dps0 * r / (1 - r)  # 마리마다 한 번 — 처치 시간이 줄어든 만큼 더 자주
    dps = sum(parts.values())
    sl = next((i for i, s in enumerate(sk) if s.get("slaughter")), None)
    sl_frac = use[sl] * sk[sl]["cast"] / periods[sl] if sl is not None else 0.0
    speed_dps = parts["기본공격"] + sum(parts[s["name"]] for s in sk if s.get("speed"))
    return dict(dps=dps, parts=parts, basic_frac=basic_frac, basic_hits=basic_hits, casts=casts, use=use, periods=periods, sk=sk, basic=basic, sl_frac=sl_frac,
                speed_dps=speed_dps, up_t=up_t, regen=regen, combo_sp=stages_s * basic_frac * gain, up_sp=up_sp)


def berserker_taken(p, ne):
    t = (RAGE_TAKEN if on(p, "분노태세") else 1.0) * (0.80 if on(p, "도발") else 1.0)
    return t * p["opts"]["taken"] * ne.get("counter_taken_mul", 1.0) * ne.get("taunt_taken_mul", 1.0)


def counter_dps(p, m, ne, pack, mul, boss=False):
    """반격태세 Lv5(상시): 맞을 때마다 공격력 × 1.4 × (1 + 반격 +%) − 방어, × 분노 · 도발 · 격분 배율."""
    if not on(p, "반격태세"):
        return 0.0, 0.0
    _, _, rel = hitcrit(p, m)
    attackers = ATTACKERS if pack and not boss else 1.0
    _, hits, _ = incoming(p, m, berserker_taken(p, ne))
    buff = (RAGE_DEALT if on(p, "분노태세") else 1.0) * (1.20 if on(p, "도발") else 1.0) * mul
    one = max(p["atk"] * 1.4 * (1 + g(ne, "counter_mul")) - m["defence"], 0) * buff * rel
    spread = 1 + (g(ne, "counter_aoe") * (ALIVE - 1) if pack and not boss else 0)
    return one * hits * attackers * spread, one * hits * attackers


def fury_model(p, m, ne, pack, boss=False):
    """버서커 격분: 게이지 25 타(주는 타 · 격분 중 받는 타 4 %) · 100 % = 피해 ×2 · 속도 ×1.4 · 흡혈 10 % · 60 초 · 폭발 = 공격 × 14(방어 없음, 맵 전체)
    · 폭발 뒤 쿨 30 초(싸우는 동안 37.5 초)."""
    off = rotation(p, m, ne, pack, boss=boss)
    if not on(p, "격분"):
        return dict(dps=off["dps"], parts=dict(off["parts"]), on=0.0, up=0.0, f_full=0.0, mean_mul=1.0, full=off, off=off, t_ramp=0.0, ext=0.0)
    fdm = g(ne, "fury_dmg")
    full = rotation(p, m, ne, pack, mul=2.0 + fdm, spd=1.4, boss=boss)
    ramp = rotation(p, m, ne, pack, mul=1.5 + fdm, spd=1.2, boss=boss)
    attackers = ATTACKERS if pack and not boss else 1.0
    _, taken_hits, _ = incoming(p, m, berserker_taken(p, ne))
    taken_hits *= attackers
    hit_events = min(full["basic_hits"] + full["casts"] * 2 + taken_hits, FPS)
    nb = 1 if boss else (ALIVE if pack else 1)
    explode = nb * (14 * p["atk"] if boss else min(14 * p["atk"], m["hp"]))
    start = g(ne, "fury_start")
    t_ramp = 25 * (1 - start) / max(ramp["basic_hits"] + ramp["casts"] * 2 + taken_hits, 0.1)
    kill_rate = 0.0 if boss else full["dps"] / m["hp"]
    ext = g(ne, "fury_hit") * hit_events + g(ne, "fury_kill") * kill_rate + g(ne, "fury_taken") * taken_hits
    keep = g(ne, "fury_keep")
    blast_cd = (g(ne, "fury_blast_cd") if keep else 30.0) / 0.8
    on_t = float("inf") if ext >= 1.0 else 60.0 / (1.0 - ext)
    t_rekeep = t_ramp * (1 - g(ne, "fury_keep_gauge")) / max(1 - start, 0.01)
    off_t = 30.0 / 0.8
    if keep and on_t == float("inf"):
        f_ramp, f_off, blasts = min(t_rekeep / blast_cd, 1.0), 0.0, 1 / blast_cd
    elif keep:
        cyc = on_t + off_t
        nbl = 1 + int(max(on_t - t_ramp, 0) // blast_cd)
        f_ramp, f_off, blasts = min(t_ramp + (nbl - 1) * t_rekeep, on_t) / cyc, off_t / cyc, nbl / cyc
    elif on_t == float("inf"):
        f_ramp, f_off, blasts = 0.0, 0.0, 0.0
    else:
        cyc = on_t + off_t
        f_ramp, f_off, blasts = min(t_ramp, on_t) / cyc, off_t / cyc, 1 / cyc
    f_full = max(1.0 - f_ramp - f_off, 0.0)
    parts = {}
    for w, r in ((f_ramp, ramp), (f_full, full), (f_off, off)):
        for kk, v in r["parts"].items():
            parts[kk] = parts.get(kk, 0.0) + w * v
    parts["격분 폭발"] = blasts * explode
    dps = sum(parts.values())
    return dict(dps=dps, parts=parts, on=on_t, up=1 - f_off, f_full=f_full, mean_mul=1.5 * f_ramp + 2 * f_full + f_off, full=full, off=off, t_ramp=t_ramp, ext=ext)


def fight(p, ne, m, pack, boss=False):
    """한 적(또는 무리): DPS · 원천 · 처치 s · 사망 s · 여유."""
    cls = p["cls"]
    attackers = 1.0 if boss or not pack else ATTACKERS
    heal = 0.0
    if cls == 0:
        fm = fury_model(p, m, ne, pack, boss)
        parts = fm["parts"]
        cd_, cone = counter_dps(p, m, ne, pack, fm["mean_mul"], boss)
        if cd_:
            parts["반격"] = cd_
        tmul = berserker_taken(p, ne) * (1 - fm["up"] * (1 - ne.get("fury_taken_mul", 1.0)))
        heal += fm["full"]["speed_dps"] * 0.10 * fm["f_full"]  # 격분 100 %: 속도 동작의 타격 10 % 흡혈
        heal += cone * g(ne, "counter_heal")
        rot = fm["off"]
    else:
        fm = None
        rot = rotation(p, m, ne, pack, boss=boss)
        parts = dict(rot["parts"])
        tmul = p["opts"]["taken"]
        if cls == 1 and on(p, "철괴"):
            tmul *= 1 - min(0.5, 0.25 + 0.002 * int_eff(p))
        if cls == 2:
            inv = (4.0 / 37.5) if on(p, "민첩태세") else 0.0
            if not g(ne, "sl_noinv"):
                inv += rot["sl_frac"]
            tmul *= 1 - min(inv, 0.9)
    dps = sum(parts.values())
    inc, _, _ = incoming(p, m, tmul)
    inc *= attackers
    heal += parts.get("기본공격", 0.0) * p["opts"]["lifesteal"]
    count = 1 if boss or not pack else N_PACK
    total = m["hp"] * count
    kill_s = total / dps if dps > 0 else float("inf")
    heal += p["opts"]["heal_hp"] * p["hp"] * (count / kill_s)
    aura = 0.0
    if cls == 1 and on(p, "치유오라"):
        T = 30 / (FPS * 0.8) + 0.15
        cap = p["hp"] * 5 / 15 * (1 + int_eff(p) / 100) / T
        need = max(inc - heal, 0)
        aura = min(need / cap, 1.0) if cap > 0 else 0.0
        heal += aura * cap
        dps *= 1 - aura * 0.4 / T
        kill_s = total / dps
    net = inc - heal
    die_s = p["hp"] / net if net > 0 else float("inf")
    return dict(dps=dps, parts=parts, kill_s=kill_s, die_s=die_s, margin=die_s / kill_s, inc=inc, heal=heal, aura=aura, rot=rot, fm=fm)


def evaluate(p, ne, rows, diff, pack, boss=False):
    out = [fight(p, ne, mob(gt, diff), pack, boss) for e, h, gt in rows]
    for x, (e, h, gt) in zip(out, rows):
        x["name"] = e["name"]
    cap = lambda v: min(v, 9999)
    nn = len(out)
    parts = {}
    for x in out:
        for kk, v in x["parts"].items():
            parts[kk] = parts.get(kk, 0.0) + v / nn
    return dict(kill=sum(cap(x["kill_s"]) for x in out) / nn, margin=sum(cap(x["margin"]) for x in out) / nn, worst=min(out, key=lambda x: x["margin"]),
                dps=sum(x["dps"] for x in out) / nn, parts=parts, aura=sum(x["aura"] for x in out) / nn, rows=out)


# ── 보고서 ────────────────────────────────────────────────────────────────────────────────────────────────────────────

def judge(m):
    return "❌ 진다" if m < 1 else "어려움" if m < 2 else "조금 어려움" if m < 4 else "보통" if m < 10 else "쉬움"


def f(v):
    if v == float("inf") or v >= 999:
        return "∞"
    return f"{v:.0f}" if v >= 10 else f"{v:.1f}"


def pct(v):
    return f"{v:+.0%}" if abs(v) >= 0.005 else "±0"


def validation():
    """시험 캐릭터(class_dps_probe 의 LV 90 · 의지 300 · 숲의 낮은 레벨 몬스터 — 방어 6)에서 probe 실측 vs 모델.
    「맞춤」 = 그 실측에서 계수를 정했다(모델이 그것을 재현하는 것은 당연) · 「식」 = 디스어셈블한 식이 실측을 따로 맞힌다."""
    B, K, R, DEF = 371, 171, 202, 6
    db, dk, dr = B - DEF, K - DEF, R - DEF
    rows = [
        ("버서커 기본공격 DPS", 884, BASIC[0]["hits"] * BASIC[0]["mul"] * db, "박자 2.0 타/초 × 단 보너스 1.1 (class_dps_probe 실측 박자 — 다른 런)"),
        ("버서커 기본공격 + 분노태세", 1149, BASIC[0]["hits"] * BASIC[0]["mul"] * db * 1.30, "식 — 분노 Lv5 ×1.30 (실측 당시 값 — 2026-10-14 부터 ×1.20, 모델은 RAGE_DEALT)"),
        ("버서커 휘두르기 한 번", 1564, 2 * 2.0 * 1.09 * db, "식 — 두 타 × 2.0 × 근력단련 Lv3 1.09"),
        ("버서커 폭풍강타 한 번", 2402, 2 * 3.0 * 1.09 * db, "식 — 두 타 × 3.0 × 1.09"),
        ("버서커 사자후 한 번 (낮은 몬스터)", 440, 440, "식 — 즉사 굴림 100 % → 몬스터의 최대 HP(440)를 돌려준다"),
        ("나이트 검기 한 번", 1301, 7.88 * dk, "맞춤 (×7.88 d)"),
        ("나이트 검기 + 신의가호", 2001, 9.01 * (4 / 3 * K - DEF), "식 — (공격 × 4/3 − 방어) × 9.01"),
        ("나이트 방패치기 한 번", 980, 5.73 * K - 2.25 * DEF, "맞춤 (공격 × 5.73)"),
        ("나이트 발도 한 번", 2546, 14.9 * K - 2.7 * DEF, "맞춤 (공격 × 14.9)"),
        ("나이트 발도 + 신의가호", 4574, 26.7 * K - 2.7 * DEF, "맞춤 (공격 × 26.7 — 설계 17.97 × 1.5 = 27.0)"),
        ("로그 기본공격 DPS", 1117, BASIC[2]["hits"] * BASIC[2]["mul"] * dr, "박자 3.9 타/초 × 콤보 1.45"),
        ("로그 맹독감염 중 + 중독", 1721 - 1117, 3.0 * R, "맞춤 (공격 × 3 / 초 — 틱 200 % 때 실측, 300 % 롤백 뒤 VENOM_PER_S 4.5)"),
        ("로그 일격 한 번", 905, 4.62 * dr, "맞춤 (×4.62 d)"),
        ("로그 급소가격 한 번", 523, 2.67 * dr, "맞춤 (×2.67 d)"),
        ("로그 단검난무 한 번", 5591, 28.0 * dr, "식 다섯 타 × 5.5 = 27.5 · 실측 28.5"),
        ("로그 살상연희 한 번 (8 타)", 15844, 8 * 10.1 * dr, "맞춤 (한 타 10.1 d)"),
        ("로그 일풍격 한 번 (한 마리)", 5321, 1.46 * (17.97 * R - 1.5 * DEF), "식 한 칸 공격 × 17.97 · 한 마리에 1.46 칸(맞춤)"),
        ("로그 개화 (30 타)", 69765, 30 * (2.1 * R - DEF) * 5.5, "식 — 30 × (공격 × 2.1 − 방어) × 5.5"),
        ("나이트 천공의검", 17262, 17262, "맞춤 (식의 0.775 — 다섯 타 중 하나가 안 닿음)"),
        ("야수 지옥불 한 번 (LV 46 · 지력 12)", 2748, 7 * ((46 + 30 + 24) * 4 - DEF), "식 — 링 7 타 × ((LV + 30 + 2·지력)·4 − 방어)"),
        ("야수 암살자 한 번", 1354, ASS_HITS * ((2.7 * 46 + 20 + 24) * 5 - DEF), f"맞춤 ({ASS_HITS} 틱 — 식은 2 틱)"),
        ("야수 학살화염 (붙은 한 마리)", 0, 0, "식 — ±4 칸 흩뿌림이라 붙은 한 마리에 안 닿는다"),
    ]
    gaps = [("휘두르기", 40, 2.7), ("폭풍강타", 60, 3.9), ("사자후", 120, 7.5), ("검기", 25, 1.8), ("방패치기", 45, 3.0), ("발도", 140, 8.7),
            ("일격", 17, 1.32), ("급소가격", 35, 2.4), ("단검난무", 40, 2.7), ("일풍격", 140, 8.7)]
    return rows, gaps


def parts_text(parts, top=6):
    tot = sum(parts.values())
    if tot <= 0:
        return "-"
    items = sorted(((k, v) for k, v in parts.items() if v > 0.005 * tot), key=lambda kv: -kv[1])
    return " · ".join(f"{k} {v / tot:.0%}" for k, v in items[:top])


def memo_for(p, ne, rows, diff):
    m0 = mob(rows[len(rows) // 2][2], diff)
    if p["cls"] == 0:
        fm = fury_model(p, m0, ne, True)
        on_t = "∞" if fm["on"] == float("inf") else f"{fm['on']:.0f} s"
        cd_, _ = counter_dps(p, m0, ne, True, fm["mean_mul"])
        stand = N_PACK * m0["hp"] / cd_ if cd_ > 0 else float("inf")
        return f"격분 {on_t} · 100 % {fm['f_full']:.0%} · 가만히 서서 반격만으로 무리 처치 {f(stand)} s"
    r = rotation(p, m0, ne, True)
    bits = []
    for s_, u, T in zip(r["sk"], r["use"], r["periods"]):
        if s_.get("slaughter"):
            bits.append(f"살상연희 {s_['slaughter']:.0f} 타 {T:.1f} s 마다 {u:.0%}")
        if s_["name"].startswith("발도"):
            bits.append(f"{s_['name']} {T:.1f} s 마다 {u:.0%}")
    bits.append(f"평타 시간 {r['basic_frac']:.0%}")
    return " · ".join(bits)


def demon_table(items, gear, rows, diff, lv):
    """악의 나이트(체1의1지1): 결정값 vs C 조각 · B1 — demon_balance 의 결투 그대로."""
    slots = GB.LOADOUTS["나이트"][2]
    A111 = (0, 1 / 3, 0, 1 / 3, 1 / 3)
    cfg0 = DB.recommended()
    variants = []

    def run(label, swap, claw_spd=0.0, claw_dmg=0.0, dur=60.0, kill_ext=0.0, cd_basic=False):
        sl = dict(slots)
        kept = [v for k, v in sl.items() if k not in swap]
        for k, code in swap.items():
            sl[k] = NEW[code]["base"]
        o = GB.gear_options(gear, kept, 1)
        o["dmg"] = dict(o["dmg"])
        o["dmg"][0] = o["dmg"].get(0, 0) + claw_dmg
        p = GB.player(1, lv, items, None, slots=sl, archetype=A111, opts=o)
        cfg = dict(cfg0)
        cfg["combo_frames"] = cfg0["combo_frames"] / (1 + claw_spd)
        res = []
        for e, h, gt in rows:
            m = mob(gt, diff)
            claw, _ = DB.claw_combo_dps(p, gt, cfg)
            cd = 90.0
            if cd_basic:
                r = 3 * (1 + claw_spd) / 10 * 1.0
                cd = 90.0 * CD_CLOCK / (CD_CLOCK + r)
            kills = claw / m["hp"]
            on_t = dur if kill_ext * kills < 1 else float("inf")
            if kill_ext and on_t != float("inf"):
                on_t = dur / max(1 - kill_ext * kills, 0.05)
            upt = 1.0 if on_t == float("inf") or on_t >= cd else on_t / cd
            d = DB.demon_duel(p, gt, diff, cfg, upt, ["follow"], beast_cd=1.0, fourth="fire")
            res.append((d, upt))
        n = len(res)
        variants.append((label, sum(min(d["kill_s"], 9999) for d, _ in res) / n, sum(min(d["margin"], 9999) for d, _ in res) / n, min(d["margin"] for d, _ in res), sum(u for _, u in res) / n))

    run("결정값 (가디언 한 벌)", {})
    c1, c3 = NEW["C1"]["eff"], NEW["C3"]["eff"]
    spd, dmg = c1["claw_spd"], c1["claw_dmg"] + c3["claw_dmg"]
    run("C1 · C3 (독사 · 밤까마귀 자리)", {"g": "C1", "a": "C3"}, claw_spd=spd, claw_dmg=dmg)
    run("C 다섯 (+ 75 초 · 처치 +1 초)", {"g": "C1", "a": "C3", "h": "C4", "s": "C2", "acc": "C5"}, claw_spd=spd, claw_dmg=dmg, dur=75.0, kill_ext=1.0)
    run("C1 · C3 · C4 + B1 시간도둑", {"g": "C1", "a": "C3", "h": "C4", "acc": "B1"}, claw_spd=spd, claw_dmg=dmg, dur=75.0, cd_basic=True)
    return variants


def section_first(out, items, gear, rows, bosses, diff, blo, first_names, detail):
    out.append("### 1차 기준 (가디언 한 벌 · 모든 스킬 · 패시브 · 상시 버프 · 각성기 · 야수)\n")
    out.append("보스 = 이 띠의 보스·이벤트 적 " + str(len(bosses)) + " 종(" + " · ".join(e["name"] for e, h, gt in bosses) + ") 1:1 — 보스 특수기는 모델 밖\n")
    out.append("| 직업 · 장비 | ATK/DEF/HP/SP | 단일 DPS | 단일 처치 s | 단일 여유 평균 · 최소 | 무리 처치 s | 무리 여유 평균 · 최소 | 판정(무리 최소) | 보스 처치 s · 여유 최소 | 야수 몫 단일 · 무리 |\n|---|---|---|---|---|---|---|---|---|---|")
    base = {}
    for name in first_names:
        cls, slots = FIRST[name]
        p, ne = make_player(items, gear, cls, slots, {}, blo)
        s1 = evaluate(p, ne, rows, diff, False)
        s2 = evaluate(p, ne, rows, diff, True)
        s3 = evaluate(p, ne, bosses, diff, False, boss=True) if bosses else s1
        base[name] = (s1, s2, s3, p)
        bshare = lambda s: sum(v for k, v in s["parts"].items() if k.startswith("야수")) / max(sum(s["parts"].values()), 1e-9)
        out.append(f"| {name} | {p['atk']:.0f}/{p['defence']:.0f}/{p['hp']:.0f}/{p['sp']:.0f} | {s1['dps']:.0f} | {f(s1['kill'])} | ×{f(s1['margin'])} · ×{f(s1['worst']['margin'])} | "
                   f"{f(s2['kill'])} | ×{f(s2['margin'])} · ×{f(s2['worst']['margin'])} | {judge(s2['worst']['margin'])} | {f(s3['kill'])} · ×{f(s3['worst']['margin'])} | {bshare(s1):.0%} · {bshare(s2):.0%} |")
    out.append("")
    if not detail:
        return base
    out.append("### 원천별 DPS — 무엇이 피해를 내나 (띠 평균)\n")
    out.append("| 직업 · 장비 | 단일 | 무리 | 보스 |\n|---|---|---|---|")
    for name in first_names:
        s1, s2, s3, p = base[name]
        out.append(f"| {name} | {parts_text(s1['parts'])} | {parts_text(s2['parts'])} | {parts_text(s3['parts'])} |")
    out.append("")
    out.append("### SP 살림 (띠 가운데 적 · 단일 · 격분 꺼진 동안)\n")
    out.append("| 직업 · 장비 | 최대 SP | 자연 회복 /s | 콤보 획득 /s | 유지(버프) /s | 평타 시간 | 쓰는 스킬 (가동률) |\n|---|---|---|---|---|---|---|")
    for name in first_names:
        s1, s2, s3, p = base[name]
        m0 = mob(rows[len(rows) // 2][2], diff)
        r = rotation(p, m0, merge([]), False)
        used = " · ".join(f"{s['name']} {u:.0%}" for s, u in zip(r["sk"], r["use"]) if u > 0.005)
        out.append(f"| {name} | {p['sp']:.0f} | {r['regen']:.0f} | {r['combo_sp']:.0f} | {r['up_sp']:.0f} | {r['basic_frac']:.0%} | {used} |")
    out.append("")
    return base


def wis_archetype(cls, share=0.2):
    """포인트의 share 를 주 능력치(힘 · 로그는 민첩)에서 의지로 옮긴 배분."""
    a = list(GB.ARCHETYPE[cls])
    main = 2 if cls == 2 else 0
    a[main] -= share
    a[3] += share
    return tuple(a)


def section_switches(out, items, gear, rows, bosses, diff, blo):
    out.append("### 패시브 · 버프 · 각성기 · 야수를 하나씩 끄면 (직업마다 첫 1차 장비 · 띠 평균)\n")
    out.append("변화 = 끈 값 ÷ 다 켠 값 − 1. 처치 s 는 짧을수록 좋다(+ = 느려짐). 여유는 무리 최소(가장 위험한 적).\n")
    out.append("| 직업 | 끈 것 | 단일 DPS | 무리 처치 s | 보스 처치 s | 단일 여유 평균 | 무리 여유 최소 |\n|---|---|---|---|---|---|---|")
    seen = set()
    for name in FIRST:
        cls, slots = FIRST[name]
        if cls in seen:
            continue
        seen.add(cls)
        p0, ne = make_player(items, gear, cls, slots, {}, blo)
        a1, a2 = evaluate(p0, ne, rows, diff, False), evaluate(p0, ne, rows, diff, True)
        a3 = evaluate(p0, ne, bosses, diff, False, boss=True)
        out.append(f"| {GB.CLASS_NAME[cls]} | (다 켬) | {a1['dps']:.0f} | {f(a2['kill'])} | {f(a3['kill'])} | ×{f(a1['margin'])} | ×{f(a2['worst']['margin'])} |")
        for sw in SWITCHES[cls] + ["+의지 20 %p"]:
            if sw.startswith("+의지"):
                p, ne = make_player(items, gear, cls, slots, {}, blo, archetype=wis_archetype(cls))
                sw = "(반대로) 포인트 20 %p 를 " + ("민첩" if cls == 2 else "힘") + " → 의지"
            else:
                p, ne = make_player(items, gear, cls, slots, {}, blo, off=(sw,))
            b1, b2 = evaluate(p, ne, rows, diff, False), evaluate(p, ne, rows, diff, True)
            b3 = evaluate(p, ne, bosses, diff, False, boss=True)
            out.append(f"| | {sw} | {pct(b1['dps'] / a1['dps'] - 1)} | {pct(b2['kill'] / a2['kill'] - 1)} | {pct(b3['kill'] / a3['kill'] - 1)} | "
                       f"×{f(b1['margin'])} | ×{f(b2['worst']['margin'])} |")
    out.append("")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source")
    ap.add_argument("--md", default="HERO2_SKILL_GEAR_BALANCE.md")
    ap.add_argument("--anchors", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "wie_android", "src", "guest_anchors.rs"))
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    items = {(r["typeId"], r["index"]): r for r in GB.decode_all(a.source)}
    ens = [e for e in GB.enemies(a.source) if e["maps"]]
    allrows = []
    for e in ens:
        h = GB.hell_template(e)
        allrows.append((e, h, GB.guardian_template(h, e["gate"])))
    gear = GB.parse_gear_items(a.anchors)
    first_names = list(FIRST)
    out = ["# 직업 밸런스 전수 분석 (모든 스킬 · 패시브 · 야수) + 스킬 특화 장비(신규 2차) — 생성 `tools_re/skill_gear_balance.py`\n"]
    out.append("모델 v2 (2026-10-14) — 직업마다 **모든 액티브 · 상시 버프(유지 SP · 동작 포함) · 패시브 · 각성기 · 야수**를 한 회전에 넣고, 계수는 "
               "**probe 실측**(§0)과 디스어셈블한 식으로 정했다. 가디언 모드 · 띠의 일반 적 전부 · 개체 k=1 · 제련 0 · 랜덤 옵션 기대값 · "
               f"**단일** = 한 마리 1:1 · **무리** = {N_PACK} 마리를 붙여 놓고(동시에 때리는 수 {ATTACKERS:.1f} · 살아 있는 평균 {ALIVE}) 무리 전체 처치 · "
               "**보스** = 띠의 보스 1:1(특수기 빼고). 여유 = 사망 s ÷ 처치 s. 판정: < 1 ❌ · < 2 어려움 · < 4 조금 어려움 · < 10 보통 · 그 위 쉬움. "
               "SP = 자연 회복(의지' × 1.25/초, 실측) + 콤보 획득 − 버프 유지; 스킬은 SP · 동작 시간 안에서 「피해 ÷ 비용」 순으로 쓴다. "
               "⚠ 가정 = 무리의 몰림 · 광역 대상 수 · 사자후 즉사의 레벨(진짜 레벨 — 보정이 걸리면 더 세다) · 야수 레벨 = 플레이어 레벨 · 능력치 배분 = `guardian_balance.ARCHETYPE`(의지 0 %). "
               "절대 초보다 **같은 표 안의 비교**가 의미 있다.\n")
    vrows, gaps = validation()
    out.append("## 0. 모델 검증 — probe 실측 vs 모델 (시험 캐릭터 LV 90 · 의지 300 · 숲의 낮은 몬스터, `tools_re/class_rotation_probe.py --spam`)\n")
    out.append("| 항목 | 실측 | 모델 | 차 | 근거 |\n|---|---|---|---|---|")
    for lab, meas, model, why in vrows:
        diff_ = "—" if meas == 0 else f"{model / meas - 1:+.0%}"
        out.append(f"| {lab} | {meas:,.0f} | {model:,.0f} | {diff_} | {why} |")
    out.append("")
    out.append("시전 간격 (쿨 틱 → 초, 식 = 쿨 ÷ 16 + 0.15): " + " · ".join(f"{n} {t:.2f} vs {cd / 16 + 0.15:.2f}" for n, cd, t in gaps)
               + " · 야수 = 쿨 × 0.06 (지옥불 72 → 4.32 · 암살자 64 → 3.84 실측).\n")
    for blo, label in [(90, "90 대 (가디언 LV 90..105 · 플레이어 LV 90)"), (80, "80 대 (가디언 LV 80..89 · 플레이어 LV 80)")]:
        lo, hi = GB.band_range(blo)
        rows = [(e, h, gt) for e, h, gt in allrows if lo <= gt["L"] <= hi and e["gate"] != 0]
        bosses = [(e, h, gt) for e, h, gt in allrows if lo <= gt["L"] <= hi and e["gate"] == 0]
        for dname, diff in [("NORMAL", 1.5), ("EASY", 1.0)]:
            if blo == 80 and dname == "EASY":
                continue
            detail = blo == 90 and dname == "NORMAL"
            out.append(f"## {label} · 난이도 {dname} — 일반 적 {len(rows)} 종\n")
            base = section_first(out, items, gear, rows, bosses, diff, blo, first_names, detail)
            if detail:
                section_switches(out, items, gear, rows, bosses, diff, blo)
            out.append("### 2차 빌드 (기준 = 같은 직업의 1차 장비에서 칸만 바꿈)\n")
            out.append("| 빌드 | 바꾼 칸 | 단일 DPS (기준 대비) | 무리 처치 s (기준 대비) | 보스 처치 s (기준 대비) | 단일 여유 최소 | 무리 여유 평균 · 최소 | 판정(무리 최소) | 메모 |\n|---|---|---|---|---|---|---|---|---|")
            summary = {}
            for bname, cls, bi, swap in BUILDS:
                fname = first_names[bi]
                _, slots = FIRST[fname]
                p, ne = make_player(items, gear, cls, slots, swap, blo)
                s1 = evaluate(p, ne, rows, diff, False)
                s2 = evaluate(p, ne, rows, diff, True)
                s3 = evaluate(p, ne, bosses, diff, False, boss=True)
                b1, b2, b3, _ = base[fname]
                swaps = " · ".join(f"{NEW[c]['code']} {NEW[c]['name']}" for c in dict.fromkeys(swap.values()))
                summary.setdefault(cls, []).append((bname, s1["dps"], s2["kill"], s3["kill"], s2["worst"]["margin"]))
                out.append(f"| {bname} | {swaps} | {s1['dps']:.0f} (×{s1['dps'] / b1['dps']:.2f}) | {f(s2['kill'])} (×{s2['kill'] / b2['kill']:.2f}) | "
                           f"{f(s3['kill'])} (×{s3['kill'] / b3['kill']:.2f}) | ×{f(s1['worst']['margin'])} | ×{f(s2['margin'])} · ×{f(s2['worst']['margin'])} | "
                           f"{judge(s2['worst']['margin'])} | {memo_for(p, ne, rows, diff)} |")
            out.append("")
            if detail:
                out.append("### 직업간 — 1차 최고 vs 2차 최고\n")
                out.append("| 직업 | 1차 최고 단일 DPS | 2차 최고 단일 DPS (빌드) | 1차 최고 무리 처치 s | 2차 최고 무리 처치 s (빌드) | 1차 최고 보스 처치 s | 2차 최고 보스 처치 s (빌드) |\n|---|---|---|---|---|---|---|")
                for cls in (0, 1, 2):
                    fb = [base[nm] for nm in first_names if FIRST[nm][0] == cls]
                    bs = max(summary[cls], key=lambda x: x[1])
                    bp = min(summary[cls], key=lambda x: x[2])
                    bb = min(summary[cls], key=lambda x: x[3])
                    out.append(f"| {GB.CLASS_NAME[cls]} | {max(x[0]['dps'] for x in fb):.0f} | {bs[1]:.0f} ({bs[0]}) | {f(min(x[1]['kill'] for x in fb))} | {f(bp[2])} ({bp[0]}) | "
                               f"{f(min(x[2]['kill'] for x in fb))} | {f(bb[3])} ({bb[0]}) |")
                out.append("")
            if blo == 90:
                out.append("### 악의 (체1의1지1 나이트 · 추종 · 멸화 — demon_balance 결투, 단일)\n")
                out.append("| 장비 | 처치 s 평균 | 여유 평균 | 여유 최소 | 악의 가동률 |\n|---|---|---|---|---|")
                for label2, ks, mm, mw, upt in demon_table(items, gear, rows, diff, blo):
                    out.append(f"| {label2} | {f(ks)} | ×{f(mm)} | ×{f(mw)} | {upt:.0%} |")
                out.append("")
    open(a.md, "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
    print("\n".join(out))


if __name__ == "__main__":
    main()
