#!/usr/bin/env python3
"""신규 아이템 2차 — 스킬 특화 장비 39종 + 1차 가디언 장비의 옵션 문구 (HERO2_NEW_ITEM_DRAFT.md §3 · §6, 착수 1 · 2).

사용자(2026-10-14): *"신규아이템 구현작업시작하자"* — 초안 §3 의 39종을 가디언 드랍 + 조합서로. 결정:
  · 조합서는 아이템 id 가 부호 있는 바이트(`ch.h:B`)라 `itm/23` 이 127 에서 끝난다 → 2차 조합서 36장(92..127),
    **F1 · F2 · E7 은 조합서 없이 가디언 드랍으로만** (사용자 「F1 · F2 · E7」).
  · 옵션 문구는 상세창의 둘째 페이지(사용자 「페이지 넘김」) — 장비 레코드의 설명(`ch.k`)이 그 글이고
    `guest_anchors::AccessoryPatch` 의 옵션 페이지 조각이 그린다. 1차 가디언 장비 28종도 같은 길로 문구를 얻는다.
  · 드랍 = 가디언 패스 칸 16(2차 장비, 1.5 %) · 17(2차 조합서, 2 %) — 사용자 「제안값」.
  · ★★ 2026-10-15 E2 휘두르기 도끼(1/31) 삭제 — 사용자 *"휘두르기 관련 옵션 아이템은 다 없애도돼. 필요없어"* (원작 휘두르기가 이미
    둘레를 다 친다: 네 방향마다 옆 칸 100 % · 대각선 ⅔ · 두 칸 앞 ½). 그래서 2차 38종 + 번개의 호흡 = 39 · 조합서 35장(92..126).

이 모듈은 수치 · 문구 · 그림만 갖는다. 레코드 · 조합서 · 팔레트 · 드랍 칸은 make_hero2_asset.py 가 만든다
(`--skill-gear` 일 때만 — 효과가 다 들어가기 전까지 공유 에셋에 실리지 않게).

    python tools_re/skill_gear.py            # guardian/skill_weapons/<종류><번호>/ 의 무기 PNG (이미 있으면 거절)
    python tools_re/skill_gear.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다
    python tools_re/skill_gear.py --check    # 문구 폭 · 줄 수 · id · 조합서 검사만

⛔ 무기 PNG 가 정본이다(guardian_gear.py 와 같은 규율). 그림은 가디언 무기의 모양을 다른 색으로 그린 Claude 초안.
⛔ 게임 글꼴은 ASCII(대문자) 와 한글 음절뿐이다 — 「·」「→」「x」(소문자) 는 빈칸이 된다.
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
import guardian_gear as gg  # noqa: E402
from costume_draft_lib import H  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WEAPON_DIR = os.path.join(ROOT, "guardian", "skill_weapons")

DESC_WIDTH = 94  # AccessoryPatch::DESC_WIDTH — the game's text box: pen from 1, ASCII 6 · Hangul 9, wrap when pen + advance > 94
GEAR_DESC_LINES = 3  # AccessoryPatch::GEAR_DESC_LINES — the option page
ACC_DESC_LINES = 4  # AccessoryPatch::DESC_LINES — an accessory's description (its stats included)


def text_width(line):
    return 1 + sum(6 if ord(c) < 128 else 9 for c in line)


# ── 1차 가디언 장비 28종의 옵션 문구 (GearPatch::ITEMS 의 text 와 같은 뜻 — 줄마다, 게임 글꼴에 맞춰) ──────────────
GUARDIAN_GEAR_LINES = {
    (0, 33): ["기본공격 피해 +50%"],
    (0, 34): ["처치 시 HP 5% 회복"],
    (1, 28): ["받는 피해 -15%", "최대 HP +15%"],  # ★★ 2026-10-20 LV 85 상향평준화 (was 받는 피해 -8 %)
    (1, 29): ["폭풍강타 피해", "+100%"],  # ★ 2026-10-20 +50 → +100 % (밸런스 전수 조사 ④) — 한 줄 96 px > 94 라 두 줄
    (2, 28): ["검기 피해 +40%,", "일격, 일섬", "피해 +60%"],  # ★★ 2026-10-20 검기 +20 → +40 %  # ★★ 2026-10-15 사용자: 일격 · 일섬 둘 다 +60 %
    (2, 29): ["방패치기 피해 +50%", "급소가격, 극사", "피해 +20%"],
    (3, 33): ["맹독감염 중독", "피해 +50%"],
    (3, 34): ["살상연희 피해 +50%"],
    (4, 33): ["둔기강타 피해 +20%"],
    (4, 34): ["받는 피해 -10%", "받는 피해의 20%", "반사"],  # ★★ 2026-10-20 (was 반사 10 %)
    (5, 28): ["받는 피해 -15%"],
    (5, 29): ["치명 +10%P", "기본공격 피해 +20%"],  # ★★ 2026-10-20 (was 치명 +5 %p)
    (5, 30): ["최대 HP +15%", "받는 피해의 10%", "반사"],  # ★★ 2026-10-20 (was 반사 5 %)
    (5, 31): ["스킬 SP 소모 -10%", "스킬 쿨타임 -10%"],  # ★★ 2026-10-20 + 쿨타임
    (6, 51): ["최대 HP +40%", "받는 피해 -15%"],  # ★★ 2026-10-20 + 받는 피해
    (6, 52): ["회피 +5%P", "민첩 +50"],
    (6, 53): ["받는 피해 -25%", "최대 HP +20%"],
    (6, 54): ["최대 SP +10%", "의지 +50"],
    (8, 51): ["기본공격 피해 +35%", "받는 피해 -8%"],  # ★★ 2026-10-20 (was +15 %)
    (8, 52): ["기본공격 피해 +50%"],
    (8, 53): ["폭풍강타, 둔기강타", "피해 +50%"],
    (8, 54): ["처치시 SP 10% 회복", "스킬 SP 소모 -20%"],  # ★★ 2026-10-20 (was SP 5 %) — 「처치 시」 with the space is 99 px > 94
    (9, 51): ["야수 스킬 쿨타임", "-20%"],
    (9, 52): ["일격, 일섬 피해", "+100%"],
    (9, 53): ["기본공격 피해의", "15% HP 회복", "받는 피해 -8%"],  # ★★ 2026-10-20 (was 10 %)
    (9, 54): ["기본공격 피해의", "20% HP 회복"],
    (10, 28): ["최대 HP +20%", "방어확률 +10%P"],  # ★★ 2026-10-20 (was 최대 HP +5 %)
    (10, 29): ["방패치기 피해 +50%"],
}

# ── 2차 39종 (HERO2_NEW_ITEM_DRAFT.md §3 · §6.1) ─────────────────────────────────────────────────────────────────────
# (코드, 타입, id, 이름, 능력치를 빌릴 가디언 아이템 (타입, id), 옵션 줄)
# 무기 · 방어구 = 3 줄까지(옵션 페이지), 장신구 = 4 줄까지(빌린 장신구의 능력치 줄 포함).
_ACC_STATS = {(11, 20): "체력+30, HP+250", (12, 20): "의지+30, SP+150"}
ITEMS = [
    # A 격분 무한 (버서커)
    ("A5'", 0, 35, "멈추지 않는 대검", (0, 33), ["격분 시 게이지 50%", "격분 중 피해 +60%"]),
    ("A5", 1, 30, "멈추지 않는 도끼", (1, 29), ["격분 시 게이지 50%", "격분 중 피해 +60%"]),
    ("A1", 5, 32, "분노의 투구", (5, 28), ["격분 중 타격마다", "격분 +0.25초"]),
    ("A3", 6, 55, "상처의 갑주", (6, 53), ["격분 중 처치 +1초,", "맞으면 +0.3초,", "받는 피해 -15%"]),
    ("A2", 9, 55, "광전사의 발걸음", (9, 53), ["격분 중 타격마다", "격분 +0.15초"]),
    ("A4", 12, 22, "꺼지지 않는 불씨", (12, 20), ["폭발 뒤 격분 유지,", "게이지 90% 남음,", "폭발 쿨 10초,"]),
    # G 반격 무한 (버서커)
    ("G1", 0, 36, "응징의 대검", (0, 33), ["반격 피해 +80%"]),
    ("G3", 5, 33, "도발자의 투구", (5, 28), ["반격태세 유지,", "도발 중 받는 피해", "-10%"]),
    ("G2", 6, 56, "가시 판금", (6, 53), ["반격태세 중", "받는 피해 -25%"]),
    ("G4", 8, 55, "되갚는 손", (8, 51), ["반격 피해 +30%,", "반격 피해의 6%", "HP 회복"]),
    ("G5", 9, 56, "울림의 장화", (9, 53), ["반격이 주변 적에게", "25% 퍼진다"]),
    ("G6", 11, 21, "피의 서약", (11, 20), ["반격 피해의 6%", "HP 회복,"]),
    # B 쿨감 스킬딜러 (공용)
    ("B3", 5, 34, "집중의 관", (5, 29), ["스킬 SP 소모 -30%"]),
    ("B4", 6, 57, "시간의 외투", (6, 52), ["스킬 쿨타임 -15%", "(각성기 포함)"]),
    ("B5", 8, 56, "연환의 장갑", (8, 52), ["직업 스킬 피해", "+20%"]),
    ("B2", 9, 57, "흐름의 장화", (9, 54), ["기본공격 SP 획득", "+50%"]),
    ("B1", 12, 23, "시간도둑", (12, 20), ["기본공격 10회마다", "모든 스킬 쿨 -1초", "(각성기, 야수 포함)"]),
    # C 악의 (공용)
    ("C4", 5, 35, "뒤틀린 뿔", (5, 29), ["악의 지속 75초"]),
    # ★★ 2026-10-15 사용자: *"악의중 할퀴기 말고 기본공격 데미지 증가로"* · *"악의 시 공격속도 옵션도 기본 공격 속도, 악의 시 이동속도옵션도
    # 그냥 이동속도"* · *"풀린목줄 … 악의 시 받는피해 -10%도 추가"* — C1 · C2 · C3 는 늘 (악의 중 기본공격 = 할퀴기라 거기에도)
    ("C3", 6, 58, "검은 비늘", (6, 52), ["기본공격 피해 +30%"]),
    ("C1", 8, 57, "악마의 손톱", (8, 52), ["기본공격 속도 +20%", "기본공격 피해 +20%"]),
    ("C2", 9, 58, "지옥 발굽", (9, 54), ["이동속도 +25%"]),
    # ★★ 2026-10-17 사용자: *"풀린목줄도 처치시 현재 +1초고 최대 75초인데 이거 최대 제한 없애줘"*
    ("C5", 11, 22, "풀린 목줄", (11, 20), ["악의 중 받는 피해", "-10%, 처치마다", "+1초,"]),
    # D 살상연희 (로그) — 타수는 무기만, 방어구 · 장신구는 피해만
    # ★★ 2026-10-15 사용자: *"백번째 칼날은 살상연희 피해 +30%"* · *"피의연쇄 효과를 무모한 칼춤으로 옮기고 무모한 칼춤의 옵션을 극사 옵션으로"*
    #   → 「극사로 HP 가득한 적을 한 방에 처치 시 SP 5%」(쿨 초기화 없음) — 그 옵션은 피의 연쇄에 (옛 무모한 칼춤의 +12 · 무적 없음은 삭제)
    ("D1", 3, 35, "백 번째 칼날", (3, 34), ["살상연희 타수 +5,", "살상연희 피해 +30%"]),
    ("D2", 3, 36, "무모한 칼춤", (3, 34), ["살상연희 타수 +6,", "처치당 +2(최대8)"]),
    ("D3", 3, 37, "피의 연쇄", (3, 34), ["급소가격,극사 +80%", "HP 가득한 적 극사", "한 방 처치시 SP 5%"]),
    ("D4", 5, 36, "무희의 두건", (5, 29), ["살상연희 피해 +20%"]),
    ("D5", 6, 59, "무희의 옷", (6, 52), ["살상연희 피해 +20%"]),
    ("D6", 8, 58, "칼춤 장갑", (8, 52), ["살상연희 피해 +40%"]),  # ★★ 2026-10-16 +20 → +40 % (연환의 장갑과 겹침)
    ("D7", 9, 59, "잔영의 신", (9, 54), ["살상연희 피해 +20%"]),
    # ★★ 2026-10-17 사용자: *"마지막춤 옵션을 살상연희가 3타로(8->3) 줄어들고 데미지 5배 증가로"* — 무기의 +타수 · 처치 타수도 3 에서 멈춘다
    ("D8", 12, 24, "마지막 춤", (12, 20), ["살상연희 3타 고정,", "살상연희 피해 5배,"]),
    # H 스킬을 바꾸는 무기
    # ★★ 2026-10-17 사용자: *"쌍월도 아이템 설명 문구 수정해줘. 짧은 시간내에 발도 두번 사용가능 으로"*
    ("H1", 2, 30, "쌍월", (2, 29), ["짧은 시간내에", "발도 두번 사용가능"]),
    ("H2", 2, 31, "검풍", (2, 28), ["기본공격마다 검기", "피해 30%, 나이트"]),
    ("H3", 3, 38, "회오리 송곳니", (3, 34), ["일풍격으로 처치 시", "재시전 (최대 3번)"]),
    # E 스킬별 특화
    # ★★ 2026-10-15 사용자: *"둔기강타 둔기 … 이름을 협곡 지하 곡괭이로 … 데미지+100% 옵션을 둔기강타 범위 +50%, 둔기강타 쿨타임 -20%"*
    #   (「범위 24칸」 없앰) · *"검기 장갑의 이름을 소드마스터의 수련장갑"* — ⛔ 띄어 쓰면 96 px(한 줄 94) → 「소드마스터의수련장갑」
    ("E3", 4, 35, "협곡 지하 곡괭이", (4, 33), ["둔기강타", "범위 +50%,", "쿨타임 -20%"]),
    ("E4", 8, 59, "소드마스터의수련장갑", (8, 51), ["검기 피해 +30%,", "검기 SP -50%,", "사거리 +2칸"]),
    # ★★ 2026-10-17 사용자: *"일풍격 신발도 바람날개 신발로 이름 수정"* — ⛔ 이름은 레코드마다 세이브에 실린다(이미 가진 것은 옛 이름)
    ("E7", 9, 60, "바람날개 신발", (9, 54), ["일풍격 범위 3칸,", "일풍격 피해 +30%"]),
    # F 재미
    # ★★ 2026-10-17 사용자: *"중력의 갑주 옵션을 상향 … +0.4%가 아니라 1%"*
    ("F1", 6, 60, "중력의 갑주", (6, 52), ["장비 1개당", "주는 피해 +0.6%", "(가방 + 입은 장비)"]),
    ("F2", 9, 61, "술꾼의 장화", (9, 54), ["포션을 마시면", "10초간 이동속도", "+50%"]),
    # ★★ 2026-10-14 I 일섬 세트 (사용자: *"단검중에 번개의 호흡이라는 단검을 추가 … 옵션은 일섬 데미지 30%증가, 일섬 쿨타임 -20%, 일섬 sp소모량
    # -20% 감소야. 벽력일섬과 세트"* · *"번개 이펙트 조건이 … 1개 착용"*) — 옵션 셋은 벽력일섬(12/19)을 낄 때만 (`GearPatch::ITEMS` With) ·
    # 번개 연출 = `AccessoryPatch` BOLT_* (벽력일섬 + 이 단검 하나, 주손이든 보조손이든). 조합서 없음(id 127 이 끝) — 가디언 칸 16 의 단검 풀.
    ("I1", 3, 39, "번개의 호흡", (3, 34), ["일섬 피해 +30%,", "일섬 쿨타임 -20%,", "일섬 SP -20%"]),
    # ★★ 2026-10-15 사용자: *"스프링 부츠라는 중갑 신발 하나 만들고 둔기강타 범위 +50% 옵션"* — 곡괭이와 둘이면 +100 %(신의가호 8방향 3칸 → 6칸).
    #   바탕 = 산맥의 철각(9/51, 중갑) · 조합서 없음(사용자 「가디언 드랍만」) · 맨 뒤(팔레트 · 2차 방어구 순서가 안 밀리게)
    ("E8", 9, 62, "스프링 부츠", (9, 51), ["둔기강타 범위 +50%", "둔기강타 피해 +30%"]),
    # ★★ 2026-10-16 악의 무기 다섯 (HERO2_DEMON_WEAPON_DRAFT.md — user: *"짐승의박자로 결정하고 아이템 5개 추가로 진행하자"*): 이름 · 수치 ·
    #   문구 Claude 초안. 바탕 = 같은 종류의 가디언 무기(오딘 · 제스 · 이안 · 응보 · 심판). 조합서 없음(id 127 이 끝) — 가디언 칸 16 의 풀
    ("M1", 0, 37, "광마의 대검", (0, 33), ["할퀴기 격분 +4%,", "격분 중 타격마다", "악의 쿨 -0.1초"]),
    ("M2", 1, 31, "피바람 도끼", (1, 29), ["할퀴기 1, 2단이", "사방을 할퀸다,", "악의 중 처치 HP 3%"]),
    ("M3", 2, 32, "타락한 성검", (2, 28), ["신의가호 중 할퀴기", "범위 +1칸, 지옥칼날", "어둠심판 SP -40%"]),
    ("M4", 3, 40, "악몽의 독니", (3, 34), ["악의 중 할퀴기가", "맹독을 걸고,", "중독된 적에게 +25%"]),
    # ★★ 2026-10-17 사용자: *"계약의 철퇴 옵션 수정하자 악의중 둔기강타 사용가능, 둔기강타 데미지 10%증가로 … 문구도 수정"* (짐승의 박자 삭제)
    ("M5", 4, 36, "계약의 철퇴", (4, 33), ["악의 중 둔기강타", "사용 가능,", "둔기강타 피해 +10%"]),
    # ★★ 2026-10-17 발도 특화 여섯 (user: *"발도의 범위를 늘려주는 아이템과 데미지 증가, sp와 쿨타임을 감소시켜 주는 아이템류 두개로 나눠서
    #   구성하고싶고 도검류 2종류와 방어구 장갑 2개, 투구 1개(데미지)"* + 「갑옷 1개 추가 (SP·쿨)」 · 「보통」 — `GearPatch::ITEMS` J1..J6). 범위·피해 =
    #   회천도 · 원월의 장갑 · 검귀의 투구, SP·쿨 = 찰나의 검 · 납도의 장갑 · 납도의 갑옷. 이름 · 그림 · 색 Claude 초안. 바탕 = 중갑(나이트 장비).
    #   조합서 없음(id 127 이 끝) — 가디언 칸 16 의 풀. ⛔ 맨 뒤에(방어구 팔레트 번호가 순서를 따라간다 — 앞에 넣으면 이미 가진 아이템의 색이 바뀐다)
    ("J1", 2, 33, "회천도", (2, 29), ["발도 범위 +1칸,", "발도 피해 +30%"]),
    ("J2", 8, 60, "원월의 장갑", (8, 51), ["발도 범위 +1칸,", "발도 피해 +30%"]),
    ("J3", 5, 37, "검귀의 투구", (5, 28), ["발도 피해 +50%"]),
    ("J4", 2, 34, "찰나의 검", (2, 28), ["발도 SP -30%,", "발도 쿨타임 -30%"]),
    ("J5", 8, 61, "납도의 장갑", (8, 53), ["발도 SP -20%,", "발도 쿨타임 -20%"]),
    ("J6", 6, 61, "납도의 갑옷", (6, 51), ["발도 SP -20%,", "발도 쿨타임 -20%"]),
    ('K1', 5, 38, '검성의 투구', (5, 28), ['검기 피해 +50%']),
    ('K2', 5, 39, '성벽의 투구', (5, 28), ['방패치기 피해 +50%', '최대 HP +20%', '방어확률 +8%P']),
    ('K3', 6, 62, '검성의 흉갑', (6, 51), ['검기 피해 +70%']),
    ('K4', 6, 63, '검귀의 흉갑', (6, 51), ['발도 피해 +70%']),
    ('K5', 6, 64, '성벽의 흉갑', (6, 51), ['방패치기 피해 +70%', '최대 HP +30%', '방어확률 +12%P']),
    ('K6', 9, 63, '검성의 철화', (9, 51), ['검기 피해 +40%']),
    ('K7', 9, 64, '검귀의 철화', (9, 51), ['발도 피해 +40%']),
    ('K8', 9, 65, '성벽의 철화', (9, 51), ['방패치기 피해 +40%', '최대 HP +15%', '방어확률 +6%P']),
    ('K9', 8, 62, '성벽의 건틀릿', (8, 51), ['방패치기 피해 +30%', '최대 HP +10%', '방어확률 +4%P']),
]
ITEM_KEYS = {(t, i): code for code, t, i, *_ in ITEMS}
NO_RECIPE = ("F1", "F2", "E7", "I1", "E8", "M1", "M2", "M3", "M4", "M5", "J1", "J2", "J3", "J4", "J5", "J6", 'K1', 'K2', 'K3', 'K4', 'K5', 'K6', 'K7', 'K8', 'K9')  # 사용자 2026-10-14 — 조합서 id 는 127 에서 끝난다 (★★ I1 번개의 호흡도 — 같은 이유 · ★★ 2026-10-16 악의 무기 다섯도 · ★★ 2026-10-17 발도 여섯도)

# ── 조합서 — 결과 ← 빌린 가디언 장비 + 최상급 재료 + 종결 장신구 (장신구는 빌린 장신구 + 다른 종결 장신구 + 재료) ──────
#   재료: (30,3) 아만타이트 · (30,4) 엘리멘티움 · (30,5) 오리하르콘 · (31,1) 현자의 책 · (31,4) 죄인의 저주 · (31,5) 축복의 시 ·
#         (32,0) 불사조 깃털 · (32,3) 바람피리 · (32,5) 마법의 얼음
#   종결 장신구: (11,14) 선지자의 유산 · (11,15) 칠요의 인 · (11,16) 마수의 발톱 · (12,14) 칠요의 반지 · (12,15) 순교자의 결의 · (12,16) 흑옥촉
#   ⚠ 짝은 Claude 초안 (분노의 투구 · 백 번째 칼날 · 시간도둑은 초안 §6.3 의 예 그대로).
RECIPES = {
    "A5'": ((32, 0), (11, 15)), "A5": ((32, 0), (11, 16)), "A1": ((30, 5), (11, 15)), "A3": ((32, 0), (12, 14)),
    "A2": ((32, 0), (11, 16)), "A4": ((12, 14), (32, 0)),
    "G1": ((30, 3), (12, 15)), "G3": ((30, 3), (12, 15)), "G2": ((30, 3), (11, 15)), "G4": ((30, 3), (11, 16)),
    "G5": ((30, 3), (12, 14)), "G6": ((11, 16), (30, 3)),
    "B3": ((31, 1), (11, 14)), "B4": ((31, 1), (12, 15)), "B5": ((31, 1), (12, 14)), "B2": ((32, 3), (11, 14)),
    "B1": ((12, 15), (31, 1)),
    "C4": ((31, 4), (11, 16)), "C3": ((31, 4), (12, 16)), "C1": ((31, 4), (11, 15)), "C2": ((31, 4), (12, 15)),
    "C5": ((11, 15), (31, 4)),
    "D1": ((30, 5), (12, 16)), "D2": ((31, 4), (11, 16)), "D3": ((30, 4), (12, 15)), "D4": ((30, 4), (12, 16)),
    "D5": ((30, 4), (11, 16)), "D6": ((30, 4), (12, 15)), "D7": ((30, 4), (11, 15)), "D8": ((12, 16), (30, 4)),
    "H1": ((30, 5), (11, 14)), "H2": ((32, 3), (11, 15)), "H3": ((32, 3), (11, 14)),
    "E3": ((30, 3), (12, 14)), "E4": ((30, 5), (11, 14)),
}

# ── 그림 ──────────────────────────────────────────────────────────────────────────────────────────────────────────
EMBER = [H("#ffdf7f"), H("#ff7f3f"), H("#df3f1f"), H("#7f1f1f")]
WIND = [H("#dfffdf"), H("#9fdf9f"), H("#5f9f7f"), H("#3f5f3f")]
TEAL = [H("#bfffff"), H("#5fdfdf"), H("#3f9f9f"), H("#1f5f5f")]
VOLT = [H("#ffffdf"), H("#ffef5f"), H("#dfaf1f"), H("#7f5f1f")]  # ★★ 번개의 호흡: 흰 끝 · 노랑 · 금빛 · 호박
# ★★ 2026-10-16 악의 무기 — 악마의 색 (악마 도트의 파란 피부 · 보라 기운 · 지옥 불빛 · 독)
HELL_RED = [H("#ff9f7f"), H("#df3f3f"), H("#8f1f3f"), H("#3f0f1f")]
ABYSS = [H("#c89fff"), H("#8f4fdf"), H("#5f1f9f"), H("#2f0f4f")]
DEMON_SKIN = [H("#bfcfff"), H("#6f7fdf"), H("#3f3f9f"), H("#1f1f4f")]
VENOM_GREEN = [H("#efff9f"), H("#9fdf3f"), H("#4f9f3f"), H("#1f4f2f")]
TARNISH = [H("#dfbf7f"), H("#9f7f3f"), H("#5f3f1f")]
# ★★ 2026-10-17 발도 — 발도 고리의 검정 · 회색(회천도), 번쩍 뽑는 칼의 흰 빛 · 하늘색(찰나의 검)
MOONSTEEL = [H("#efefff"), H("#afafcf"), H("#6f6f8f"), H("#2f2f3f")]
FLASH = [H("#ffffff"), H("#bfefff"), H("#5fafdf"), H("#1f4f7f")]


def lightning_dagger(part, k):
    """★★ 2026-10-14 번개의 호흡 — 칼날이 번개처럼 두 번 꺾인 단검 (노란 몸 · 흰 날 끝 · 남색 날밑). Claude 초안."""
    w, h, gx, gy, tx, ty, ux, uy, px, py = gg._frame(part, k)
    L = math.hypot(tx - gx, ty - gy)
    s = lib.Shape()
    if min(w, h) <= 3:
        s.capsule(gx, gy, tx, ty, 1.4, "blade")
    else:
        gg._grip(s, w, h, gx, gy, ux, uy, 0.8, 0.8, 0.9)
        s.circle(*gg._inside(w, h, gx + ux * 1.6, gy + uy * 1.6, 1.3), 1.3, "guard")
        z = min(1.1, max(0.6, L / 9))  # 꺾임 폭
        pts = [(2.2, 0.0), (2.2 + (L - 2.2) * 0.38, z), (2.2 + (L - 2.2) * 0.66, -z), (L, 0.0)]
        rad = [1.5, 1.2, 0.95, 0.5]
        for (d0, b0), (d1, b1), r0, r1 in zip(pts, pts[1:], rad, rad[1:]):
            x0, y0 = gg._inside(w, h, gx + ux * d0 + px * b0, gy + uy * d0 + py * b0, r0)
            x1, y1 = gg._inside(w, h, gx + ux * d1 + px * b1, gy + uy * d1 + py * b1, r1)
            n = max(2, int(math.hypot(x1 - x0, y1 - y0) * 2))
            for i in range(n + 1):
                t = i / n
                s.circle(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, r0 + (r1 - r0) * t, "blade")
    return gg._render(w, h, s, {"blade": VOLT, "guard": gg.STEEL_BLUE, "grip": [gg.STEEL_BLUE[2], gg.BK, gg.BK]}, gg.OL_DARK, {"blade": VOLT[0]}, {"blade": VOLT[1], "guard": gg.STEEL_BLUE[1]})
# 무기: 코드 → (가디언 모양 함수, 덮어쓸 색 {태그: 램프, "_outline": 색})
WEAPON_ART = {
    "A5'": (gg.odin_greatsword, {"blade": EMBER, "guard": gg.OBSIDIAN, "grip": [gg.OBSIDIAN[1], gg.BK, gg.BK]}),
    "G1": (gg.dimitri_sword, {"blade": gg.IRON, "guard": gg.CRIMSON, "_outline": gg.OL_RED}),
    "A5": (gg.ulrike_axe, {"head": EMBER, "pommel": gg.OBSIDIAN, "haft": [gg.OBSIDIAN[1], gg.OBSIDIAN[2], gg.BK]}),
    "H1": (gg.doom_sword, {"blade": gg.SILVER_LAV, "guard": gg.STEEL_BLUE}),
    "H2": (gg.ian_sword, {"blade": WIND, "guard": gg.STEEL_BLUE[:3], "grip": [WIND[2], WIND[3], gg.BK]}),
    "D1": (gg.retribution_dagger, {"blade": gg.CRIMSON, "guard": gg.GOLD, "_outline": gg.OL_RED}),
    "D2": (gg.retribution_dagger, {"blade": gg.GOLD, "guard": gg.CRIMSON, "_outline": gg.OL_DARK}),
    "D3": (gg.boss_fang, {"fang": gg.CRIMSON, "root": gg.OBSIDIAN[:3]}),
    "H3": (gg.boss_fang, {"fang": TEAL, "root": gg.IRON[1:]}),
    "I1": (lightning_dagger, {}),  # ★★ 번개의 호흡: 자기 모양 · 자기 색 (RECOLOUR 없음)
    "E3": (gg.judgement_mace, {"head": gg.STEEL_BLUE, "spike": gg.SILVER_LAV, "haft": gg.IRON[1:]}),
    # ★★ 2026-10-16 악의 무기 — 가디언 모양을 악마 색으로 (Claude 초안)
    "M1": (gg.odin_greatsword, {"blade": HELL_RED, "guard": ABYSS, "grip": [ABYSS[2], gg.BK, gg.BK], "_outline": gg.OL_RED}),
    "M2": (gg.ulrike_axe, {"head": HELL_RED, "pommel": ABYSS, "haft": [ABYSS[2], gg.OBSIDIAN[2], gg.BK]}),
    "M3": (gg.ian_sword, {"blade": ABYSS, "guard": TARNISH, "grip": gg.IAN_RED}),
    "M4": (gg.boss_fang, {"fang": VENOM_GREEN, "root": ABYSS[1:]}),
    "M5": (gg.vengeance_hammer, {"head": DEMON_SKIN, "band": gg.CRIMSON, "haft": gg.OBSIDIAN[1:]}),
    # ★★ 2026-10-17 발도 도검 둘 (Claude 초안)
    "J1": (gg.doom_sword, {"blade": MOONSTEEL, "guard": gg.OBSIDIAN, "grip": [gg.OBSIDIAN[1], gg.BK, gg.BK]}),
    "J4": (gg.ian_sword, {"blade": FLASH, "guard": gg.STEEL_BLUE[:3], "grip": [FLASH[2], FLASH[3], gg.BK]}),
}
# 방어구: 코드 → 새 팔레트의 색 (본색°, 본색 채도, 명도 배율, 강조°, 강조 채도) — make_hero2_asset._recolour
ARMOUR_COLOUR = {
    "A1": (8, 0.8, 0.9, 40, 0.7), "G3": (220, 0.15, 0.85, 0, 0.7), "B3": (180, 0.5, 0.95, 50, 0.4),
    "C4": (280, 0.55, 0.6, 330, 0.6), "D4": (340, 0.6, 0.85, 45, 0.6),
    "A3": (355, 0.7, 0.7, 30, 0.6), "G2": (25, 0.35, 0.65, 15, 0.5), "B4": (225, 0.55, 0.7, 50, 0.6),
    "C3": (265, 0.35, 0.45, 290, 0.5), "D5": (330, 0.55, 0.8, 45, 0.6), "F1": (210, 0.1, 0.65, 260, 0.3),
    "G4": (35, 0.6, 0.8, 10, 0.6), "B5": (190, 0.6, 0.95, 60, 0.5), "C1": (275, 0.6, 0.6, 0, 0.7),
    "D6": (350, 0.65, 0.85, 45, 0.6), "E4": (205, 0.5, 1.0, 0, 0.0),
    "A2": (15, 0.8, 0.85, 45, 0.7), "G5": (40, 0.3, 0.8, 0, 0.6), "B2": (165, 0.45, 1.0, 60, 0.4),
    "C2": (0, 0.6, 0.5, 20, 0.8), "D7": (315, 0.55, 0.9, 50, 0.5), "E7": (120, 0.45, 0.95, 55, 0.4),
    "F2": (30, 0.55, 0.75, 50, 0.6),
    "E8": (150, 0.55, 0.9, 50, 0.6),  # ★★ 2026-10-15 스프링 부츠: 연두 · 금빛 강조
    # ★★ 2026-10-17 발도 방어구 넷: 범위·피해 = 검정 · 회색(발도 고리) / SP·쿨 = 강철 파랑 · 금빛 (Claude 초안)
    "J2": (240, 0.12, 0.55, 0, 0.0),
    "J3": (0, 0.0, 0.45, 350, 0.75),
    "J5": (205, 0.35, 0.9, 45, 0.55),
    "J6": (205, 0.35, 0.85, 45, 0.55),
    'K1': (205, 0.4, 0.65, 45, 0.7),
    'K2': (45, 0.2, 0.75, 210, 0.6),
    'K3': (205, 0.4, 0.65, 45, 0.7),
    'K4': (350, 0.25, 0.5, 30, 0.65),
    'K5': (45, 0.2, 0.75, 210, 0.6),
    'K6': (205, 0.4, 0.65, 45, 0.7),
    'K7': (350, 0.25, 0.5, 30, 0.65),
    'K8': (45, 0.2, 0.75, 210, 0.6),
    'K9': (45, 0.2, 0.75, 210, 0.6),
}


def weapon_stem(t, nn):
    return gg.weapon_stem(t, nn)


def weapons():
    """(코드, 타입, id, 이름, 바탕 가디언 무기 행) — 바탕 = guardian_gear.WEAPONS 의 그 줄(수치를 빌린다)."""
    base = {(w[0], w[1]): w for w in gg.WEAPONS}
    return [(code, t, i, name, base[b]) for code, t, i, name, b, _l in ITEMS if t <= 4]


def armour():
    """(코드, 타입, id, 이름, 바탕 가디언 방어구 행, 색) — 바탕 = guardian_gear.ARMOUR 의 그 줄(수치 · 시트를 빌린다)."""
    base = {(a[0], a[1]): a for a in gg.ARMOUR}
    return [(code, t, i, name, base[b], ARMOUR_COLOUR[code]) for code, t, i, name, b, _l in ITEMS if 5 <= t <= 10]


def accessories():
    base = {(a[0], a[1]): a for a in gg.ACCESSORIES}
    return [(code, t, i, name, base[b]) for code, t, i, name, b, _l in ITEMS if t >= 11]


def lines_of(code):
    """The description lines as the asset writes them — an accessory adds its borrowed stats as the last line."""
    for c, t, i, _name, b, lines in ITEMS:
        if c == code:
            return lines + [_ACC_STATS[b]] if t >= 11 else list(lines)
    raise KeyError(code)


def check():
    """문구 폭 · 줄 수 · id 연속 · 조합서 · 그림 표가 맞는지 — 어긋나면 SystemExit."""
    problems = []
    for key, lines in GUARDIAN_GEAR_LINES.items():
        if len(lines) > GEAR_DESC_LINES:
            problems.append(f"guardian {key}: {len(lines)} lines")
    for code, t, i, name, b, _lines in ITEMS:
        lines = lines_of(code)
        cap = ACC_DESC_LINES if t >= 11 else GEAR_DESC_LINES
        if len(lines) > cap:
            problems.append(f"{code} {name}: {len(lines)} lines > {cap}")
        if text_width(name) > DESC_WIDTH + 1:  # past 71 px the grade drops a line (GradeLinePatch) — one line is the cap
            problems.append(f"{code} {name}: name {text_width(name) - 1} px > {DESC_WIDTH}")
    for lines in list(GUARDIAN_GEAR_LINES.values()) + [lines_of(c) for c, *_ in ITEMS]:
        for line in lines:
            if text_width(line) > DESC_WIDTH + 1:
                problems.append(f"{line!r}: {text_width(line) - 1} px > {DESC_WIDTH}")
            if any(c.islower() or c in "·→×" for c in line):
                problems.append(f"{line!r}: a glyph the game's font lacks")
    by_type = {}
    for _code, t, i, *_ in ITEMS:
        by_type.setdefault(t, []).append(i)
    for t, ids in by_type.items():
        if sorted(ids) != list(range(min(ids), max(ids) + 1)):
            problems.append(f"type {t}: ids {sorted(ids)} are not one run")
    if len(ITEMS) != 60 or len(ITEM_KEYS) != 60:  # ★★ 2026-10-14 39 + 번개의 호흡 · ★★ 2026-10-15 − E2 · + E8 스프링 부츠 · ★★ 2026-10-16 + 악의 무기 다섯 · ★★ 2026-10-17 + 발도 여섯
        problems.append(f"{len(ITEMS)} items")
    want_recipes = [c for c, *_ in ITEMS if c not in NO_RECIPE]
    if sorted(RECIPES) != sorted(want_recipes):
        problems.append(f"recipes {sorted(set(RECIPES) ^ set(want_recipes))}")
    if len(RECIPES) != 35:
        problems.append(f"{len(RECIPES)} recipes (92..126 — ★★ 2026-10-15 E2's is gone)")
    if sorted(WEAPON_ART) != sorted(c for c, t, *_ in ITEMS if t <= 4):
        problems.append("WEAPON_ART does not cover the weapons")
    if sorted(ARMOUR_COLOUR) != sorted(c for c, t, *_ in ITEMS if 5 <= t <= 10):
        problems.append("ARMOUR_COLOUR does not cover the armour")
    if problems:
        raise SystemExit("skill_gear: " + "; ".join(problems))


def draw_all(out=WEAPON_DIR, force=False):
    """guardian_gear.draw_all 과 같은 규율 — 파트 PNG 가 이미 있으면 거절(`--force` 로 덮어쓴다)."""
    made = []
    for code, t, nn, name, _base in weapons():
        fn, colours = WEAPON_ART[code]
        d = os.path.join(out, weapon_stem(t, nn))
        parts = [gg.PART_OF_TYPE[t]] + ([gg.OFFHAND_OF_TYPE[t]] if t in gg.OFFHAND_OF_TYPE else [])
        for part in parts:
            if os.path.isdir(d) and any(f.startswith(part + "_") and f.endswith(".png") for f in os.listdir(d)) and not force:
                print(f"{d}: {part} PNG 가 이미 있다 — 손으로 고친 것일 수 있다 (--force 로 덮어쓴다)")
                continue
            gg.RECOLOUR = colours
            try:
                frames = [fn(part, k) for k in range(gg.FRAMES_OF_PART[part])]
            finally:
                gg.RECOLOUR = None
            if part == "weapon_rogue":
                frames.append(lib.BLANK)
            lib.write_part(d, part, frames)
            made.append(f"{d} {part}")
        if not os.path.exists(os.path.join(d, "item.txt")) or force:
            with open(os.path.join(d, "item.txt"), "w", encoding="utf-8", newline="\n") as f:
                f.write(f"{t}/{nn} {name} ({code}) — Claude 초안 2026-10-14 (tools_re/skill_gear.py, 가디언 모양 + 새 색). PNG 가 정본.\n")
    return made


if __name__ == "__main__":
    sys.stdout.reconfigure(errors="replace")
    check()
    if "--check" in sys.argv:
        print("skill_gear: ok —", len(ITEMS), "items ·", len(RECIPES), "recipes")
        sys.exit(0)
    args = [a for a in sys.argv[1:] if a != "--force"]
    for line in draw_all(args[0] if args else WEAPON_DIR, "--force" in sys.argv):
        print(line)
