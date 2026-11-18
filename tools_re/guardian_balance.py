#!/usr/bin/env python3
"""「가디언」 모드 전수 밸런스 모델 (2026-10-08 저녁, 사용자: *"새로운 템과 가디언 모드 몬스터를 포함한 가디언 모드에서의 모든 밸런스에 대한
전수 검증"*). 게임의 식(디스어셈블 §8.11 · §8.26 · §8.54 · 피격 루틴 `a.a(Lbd;SB)V` · 템플릿 `cq.a()V`)에 에셋 값을 넣어 적 전부 ×
직업 원형 × 장비 세트 × 레벨 띠의 표를 낸다. ⚠ 식의 보정 상수(CALIB)는 probe `[h2-stats]` 실측으로 맞춘다 — 아래 CALIB 주석.

    python tools_re/guardian_balance.py <hero2_ktf.zip> [--md out.md]

식 (확정 · 근거):
  템플릿(HELL)  L = min(2·L0/5 + 60, 99) · HP = (HP0−10)·2·L/L0 · ATK = (ATK0−4)·2·L/L0 · 민첩 e = (e0−4)·2·L/L0 · DEF = (DEF0+1)·L/L0
                 (`cq.a()V`, half16 = ÷2 — HERO2_GUARDIAN_PLAN.md §2.2)
  가디언 래퍼   L_g = min(70 + (L−60)·448>>8, 105) · HP ×384>>8 (★ 드랍 게이트 0 = 보스·이벤트 적은 ×307>>8 — 2026-10-08 밤 A1) (≤32,767) ·
                 ATK/DEF ×307>>8 · e 그대로 (GuardianPatch)
  개체          bd.n = L_g + k · HP += 20k · ATK += 3k, k = 0..2 (평균 k 1 로 센다)
  플레이어 타격 d = atk − DEF_m (★ 실측: 전부 뺀다, ½ 아님; 무기 min..max 가 ±3 의 흔들림) → 스킬 배율(직업 ×1.5 포함, --h2-skill 실측값)
                 → 치명 ×1.5 (★ 실측 175/117 · 196/131; 두 번 굴림: crit% · clamp(20 + L_seen − LV, 8, 50))
                 명중 = 100 − clamp(a.k + L_seen + e/5, 8, 90), a.k = 10 − LV − AGI'/10 − INT'/10 (§2.3, 실측 −74 · −87)
  몬스터 타격   ATK_m × U(0.9, 1.1) − DEF_p (최소 3) · 치명 clamp(20 + L_seen − LV, 8, 50) % 는 ×2.5 (실측 1437/575) · 회피 clamp(a.m − L_seen − e/10, 8, 90)
                 a.m = LV + 10 + AGI'/3 (+6 로그) (실측 100 · 151)
  L_seen       가디언: 60 + (bd.n − 70)·146>>8 (보정 ON) — 그 밖 bd.n (★ 실측: seen 220 이면 2/11 타 명중 · 몬스터 치명 58 %)
  HP           (VIT' + LV)·12 (실측 12,948 = (999+80)·12) · 나이트 체력단련 Lv 당 +1 (만렙 17 로 센다)
  SP           (WIS' + LV)·6 · EXP next = L³ − L² + 80L (실측 512,000) · 처치 EXP = bd.n·17·clamp(20 + bd.n − LV, 0, 28)/20 (실측 1,900 = LV 86 → 80)
                 ★ 2026-10-08 밤 A3: 모드 3 은 그 값 ×384>>8 (= ×1.5, 호스트가 bd.q()V 의 a.d(I)V 호출지에서; 퀘스트 EXP 는 그대로) — 표 3·4 에 반영
  공격 a.h     나이트 STR'·7/10 (실측 106 = 152·0.7) · 로그 (STR' + AGI')/2 (실측 127 · +25/+25) · 버서커 STR'(+패시브, 미실측) ·
                 무기 = (min+max)/2 × 종류 보너스(도검·둔기 1.1 · 단검 1.2) + 제련×1.5
  방어          Σ(방어구 방어 + 제련×1.5) + 나이트 STR'/3 (실측) · 로그 (STR' + AGI')/4 (실측 +13/+12) · 버서커 STR'/3 (가정) + 장신구
  연타          로그 5단 콤보의 단계 배율(실측 1.0 · 1.1 · 1.15 · 2.0 · 2.2 → 평균 1.45) · 나이트 3단 전부 1.0 (실측) · 버서커 1.0 (가정)
  박자          기본공격 한 타 = 나이트 26 · 로그 12 · 버서커 20(가정) 프레임 · 몬스터 공격 한 번 = 45 프레임 (붙어 있을 때, 실측) · 20 fps

★ 2026-10-09 (3) — 사용자: 확정된 개성 옵션 31 로 「전체 밸런스 다시 전수 검사 · 직업간(의·지·체 1 씩 올린 야수나이트 포함) · 가디언 노말(환경설정
난이도 NORMAL)은 좀 어렵게 · 이지는 신규템을 다 맞췄다는 가정에서 큰 어려움 없게」:
  장비 능력치   ⛔ 고침 둘 — 랜덤 옵션은 `h.a(0, slots)` 개(평균 slots/2)가 다섯 능력치 중 하나씩(HERO2_ITEM_DB §1 body[4]) → 능력치마다
                 기대값 (slots/2)/5 × 범위 평균 (전에는 다섯 전부 더했다 = ×5~×10 과대) · 제련은 갓 주운 0 (전에는 최대 제련을 가정) ·
                 장신구 몸통은 길이붙은 설명 뒤부터(가디언 셋은 설명이 있어 전에는 엉뚱한 바이트를 능력치로 읽었다) + 장신구 HP/SP 고정값
  개성 옵션     guest_anchors.rs 의 `GearItem` 표를 그대로 읽는다(parse_gear_items): 기본공격/스킬 피해 + · 받는 피해 × · 반사 · 치명/회피 +p ·
                 최대 HP/SP % · 처치 회복 · 기본공격 흡혈 · 야수 쿨 × — 용의 심장 버프(처치 1 %)와 현자의 쿨 초기화(타격 1 %)는 기대값이 작아 뺐다
  난이도        emulator::Difficulty = hero2java::DIFFICULTY_PCT — 몬스터 최대 HP · HP · 공격력 ×1 / ×1.5 / ×2 (short 32,767 에서 자름), 방어는 그대로
  흡혈          `<job>.a(BILbd;)I` 의 답(치명 전) × n/256 이 기본공격 단계마다 — 그래서 명중한 기본공격의 비치명 피해에 건다
  반사          맞은 피해(받는 피해 × 뒤) × n/256 가 그 몬스터에게
  야수 피해     링(빛의파동·지옥불·대지분노) `(bn.d + 30 + 지력·1.5)·(lv+3)/3` · 중타격(심판번개·학살화염·암흑폭발) `(bn.d·5/2 + 20 + 지력·1.5)·(lv+2)/3`
                 (§8.49 · bn.d = 야수 레벨 = 플레이어 레벨로 센다) → `bd.a(IBB)V` 가 템플릿 방어를 빼고 속성 ×/10(×1 로 센다) · 쿨(틱) 링 (14−lv)·10 ·
                 중 (21−lv)·10 · SP 링 (14−lv)·8 · 중 144−5·lv · 한 마리에 링 1 타 · 중 2 타(부모 + 흩뿌림 하나로 센다) · 야수조련 목걸이 ×2 ·
                 야수 스킬은 플레이어 동작을 쓰지 않는다(야수가 친다)
  스킬 회전     주력 스킬을 쿨마다(probe `--h2-skill` 의 a(I)S 틱 · 배율), 시전 한 번 = 기본공격 한 타의 시간, 남는 시간은 기본공격 —
                 SP 는 따로 「SP 가 버티는 초」로 보인다(자연 회복·콤보 SP 획득은 안 센다 = 보수적)

★ 2026-10-10 — 사용자: *"가디언 모드가 신규아이템말고 기존 종결아이템들을 제련했을때도 깰수있는지 직업별 밸런스 조사해줘"* → §7 (HERO2_GUEST_RE.md §8.62):
  제련 성공     `ck.d()V` @0x15d470: h.a(1, 100) ≤ (상한 − 현재)·100 // 상한 + 30 · 실패 = 장비 파괴 + 즉시 자동 저장(t/bsm.tdf[0]) · 성공은 저장 안 함
  제련 비용     `ck.e()V` @0x15d800: 현재 c → c+1 에 제련석 c+1 개 + 골드 (c+1)·100 (실패해도 낸다)
  제련 효과     무기 공격 최소·최대 + ⌊1.5·c⌋ (`bo.a()S`/`bo.b()S` → CombatPatch 무기 보너스가 그 합에 곱한다) · 방어구 방어 + ⌊1.5·c⌋ (`bu.a()I` —
                 `a.m()V` 가 투구·갑옷·장갑·신발, 기사 방패는 `bx.c()V`) · 상한 body[21]: 무기 50 · 중갑 갑옷·방패 25 · 경갑 갑옷 20 · 투구·장갑·신발 0
  기댓값        한 사본으로 +n = Π 성공 % · 실패하면 그 사본이 사라져 다음 사본은 0 부터 → 사본 = 1 ÷ 그 확률 · 제련석·골드 = 사본 하나의 기대 비용 ÷ 그 확률
  로그          전투 공격 `a.a()I` = a.h + 주무기 굴림(`a.a(B)S` 종류 4) — 보조 단검은 종류 3 타격만(장비창의 a.g = 두 단검 평균은 표시용) → 주무기만 센다
"""

import argparse
import io
import json
import math
import os
import struct
import sys
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import decode_drops as DD  # noqa: E402
import guardian_gear as GG  # noqa: E402
import map_monsters as MM  # noqa: E402
from decode_hero2_items import decode_all  # noqa: E402

# ── CALIB — probe `[h2-stats]` 로 맞춘 상수 (2026-10-08 저녁 실측, 아래 run 기록) ──
CALIB = {
    # a.h: (힘' 계수, 민첩' 계수) — 나이트 bx.c ×7/10 (실측 106 @ 152) · 로그 (힘'+민첩')/2 (실측 127 @ 91+165, +50 힘 → +25, +50 민첩 → +25) ·
    # 버서커 bl.c STR'·(10 + a.p[14])/10 (패시브 0, 미실측 — 슬롯에 버서커 세이브가 없다)
    # ★ 2026-10-09 (3) 버서커 실측(S25 백업 슬롯 1 LV60, `[h2-stats]`): 힘 128 → a.h 166 = 힘 × (10 + a.p[14] 3)/10 — 그 패시브를 다 찍은
    #   LV 70+ 로 ×1.5 를 쓴다(전에는 ×1.0). 같은 런의 HP 1,596 = (73 + 60)·12 · SP 450 = (15 + 60)·6 · 치명 11 · 회피 바탕 75 — 식 그대로
    "str_term": {0: (1.5, 0.0), 1: (0.85, 0.0), 2: (0.5, 0.5)},  # ★ 2026-10-20 나이트 0.7 → 0.85 (bx.c()V 17/20)
    # 방어에 더하는 (힘', 민첩') 몫 — 나이트 힘'/3 (실측 +17 @ +50) · 로그 (힘'+민첩')/4 (실측 +13 · +12 @ +50) · 버서커 힘'/3 (가정)
    "def_str": {0: (1 / 3, 0.0), 1: (1 / 3, 0.0), 2: (0.25, 0.25)},
    "hp_mul": {0: 12, 1: 17, 2: 12},  # (체력' + LV) × 이것 — 실측 12 (나이트 LV 80 체력 999 → 12,948); 나이트 체력단련 Lv5 → 17
    "crit_base": {0: (8, 1 / 5), 1: (8, 1 / 5), 2: (14, 1 / 3)},  # 치명 % = a + 민첩'·b (실측 14 @ 32 · 69 @ 165 · 85 @ 215)
    "am_extra": {0: 0, 1: 0, 2: 6},  # 회피 바탕 a.m = LV + 10 + 민첩'/3 + 이것 (실측 100 @ LV 80·32 · 151 @ LV 80·165)
    "crit_mul": 1.5,  # 플레이어 치명 배율 (실측 175..180 / 117.7 · 195..199 / 131)
    "mon_crit_mul": 2.5,  # 몬스터 치명 배율 (실측 1437..1515 / 575..606)
    "combo_avg": {0: 1.0, 1: 1.0, 2: 1.45},  # 기본공격 한 타의 평균 배율 (콤보 단계) — 로그 실측 161·176·186·310·350
    "attack_frames": {0: 20, 1: 26, 2: 12},  # 기본공격 한 타에 드는 프레임 (실측 나이트 26 · 로그 12 · 버서커 가정)
    "monster_attack_frames": 45,  # 붙은 몬스터의 공격 간격 (실측 15..20 회 / 700..900 프레임)
}
BASE_STATS = {0: (13, 12, 9, 11, 10), 1: (10, 11, 8, 12, 12), 2: (11, 9, 13, 10, 10)}  # 힘·체력·민첩·의지·지력 (§8.6)
CLASS_NAME = {0: "버서커", 1: "나이트", 2: "로그"}
WEAPON_BONUS = {0: 1.0, 1: 1.0, 2: 1.1, 3: 1.2, 4: 1.1}  # CombatPatch 무기 게터 (§8.26)
JOB_SKILL = 1.5  # SkillPowerPatch
KNIGHT_INT_MUL = 179  # ★★ 2026-10-17 CombatPatch::KNIGHT_INT_MUL (>> 9) — 나이트 공격 스킬의 공격력 + 지력' · 179/512

# 직업 원형: 레벨 L 의 3·(L−1) 포인트 배분 비율 (힘·체력·민첩·의지·지력)
ARCHETYPE = {0: (0.7, 0.2, 0.0, 0.1, 0.0), 1: (0.6, 0.3, 0.1, 0.0, 0.0), 2: (0.3, 0.1, 0.6, 0.0, 0.0)}
# 주력 스킬: (이름, d 배율 식(lv=5)) — §8.11 + 상향분 (나이트 검기 +30 % · 발도 ×(lv+5)(lv+3)/12)
MAIN_SKILL = {0: ("가르기 Lv5", lambda d: d + d / 3), 1: ("검기 Lv5", lambda d: d * 3 * 1.301), 2: ("단검난무 Lv5", lambda d: d * 11 / 3)}

# 장비 세트: (이름, {슬롯: (type, id)}) — HELL 종결 vs 가디언
SETS = {
    "HELL 종결": {
        0: {"w": (0, 32), "h": (5, 27), "a": (6, 49), "g": (8, 49), "s": (9, 49), "acc": (12, 14)},
        1: {"w": (2, 27), "h": (5, 27), "a": (6, 49), "g": (8, 49), "s": (9, 49), "sh": (10, 27), "acc": (12, 14)},
        2: {"w": (3, 32), "w2": (3, 32), "h": (5, 26), "a": (6, 50), "g": (8, 50), "s": (9, 50), "acc": (11, 16)},
    },
    "가디언": {
        0: {"w": (0, 34), "h": (5, 28), "a": (6, 51), "g": (8, 51), "s": (9, 51), "acc": (12, 20)},
        1: {"w": (2, 29), "h": (5, 28), "a": (6, 51), "g": (8, 51), "s": (9, 51), "sh": (10, 28), "acc": (12, 20)},
        2: {"w": (3, 34), "w2": (3, 34), "h": (5, 29), "a": (6, 52), "g": (8, 52), "s": (9, 52), "acc": (11, 20)},
    },
}
BANDS = [(70, "70 대 (초반)"), (80, "80 대 (중반)"), (90, "90 대 (후반)"), (99, "99 만렙 (후반 띠에서)")]


def band_range(lv):
    """띠의 시작 레벨 → 그 띠의 가디언 몬스터 레벨 범위."""
    return (70, 79) if lv == 70 else (80, 89) if lv == 80 else (90, 105)


def clamp(v, lo, hi):
    return max(lo, min(hi, v))


def hell_template(e):
    L0, hp0, atk0, e0, def0 = e["L0"], e["hp0"], e["atk0"], e["e0"], e["def0"]
    L = min(L0 * 2 // 5 + 60, 99)
    return dict(L=L, hp=(hp0 - 10) * 2 * L // L0, atk=(atk0 - 4) * 2 * L // L0, e=(e0 - 4) * 2 * L // L0, defence=(def0 + 1) * L // L0)


HP_MUL, BOSS_HP_MUL, ATK_MUL, KILL_EXP_MUL = 384, 307, 307, 384  # GuardianPatch::{HP_MUL, BOSS_HP_MUL, ATK_MUL, KILL_EXP_MUL} (/256)


def hp_mul_for(gate):
    """★ 2026-10-08 밤 (A1, 사용자 「보스 ×1.2」): 드랍 게이트 0 = 보스·이벤트 적 → BOSS_HP_MUL, 그 밖 HP_MUL."""
    return BOSS_HP_MUL if gate == 0 else HP_MUL


def guardian_template(h, gate=300):
    Lg = min(70 + ((max(h["L"] - 60, 0) * 448) >> 8), 105)
    return dict(L=Lg, hp=(h["hp"] * hp_mul_for(gate)) >> 8, atk=(h["atk"] * ATK_MUL) >> 8, e=h["e"], defence=(h["defence"] * ATK_MUL) >> 8)


def kill_exp(L, lv):
    """처치 EXP: 게임 식 × KILL_EXP_MUL>>8 (★ 2026-10-08 밤 A3 — 모드 3 의 bd.q()V 호출지에서 호스트가 곱한다; 퀘스트 EXP 는 그대로)."""
    return ((L * 17 * clamp(20 + L - lv, 0, 28) // 20) * KILL_EXP_MUL) >> 8


def seen_level(L, correct=True):
    return 60 + (((L - 70) * 146) >> 8) if correct and L >= 70 else L


EVADE_TRUE = True  # ★ 2026-10-09 (4) GuardianPatch::EVADE_TRUE — the player's evade formula alone sees the true guardian level


def enemies(src):
    blob = DD.load_member(src, "enm/enm")
    out, i = [], 0
    while i < len(blob):
        (ln,) = struct.unpack_from(">H", blob, i)
        rec = blob[i + 2 : i + 2 + ln]
        i += 2 + ln
        nl = rec[4]
        body = rec[5 + nl :]
        out.append(dict(index=len(out), name=rec[5 : 5 + nl].decode("euc-kr", "replace"), L0=body[4], hp0=struct.unpack_from("<h", body, 5)[0], atk0=struct.unpack_from("<h", body, 7)[0], e0=body[9], gate=struct.unpack_from("<h", body, 15)[0], def0=body[17]))
    with zipfile.ZipFile(src) as z:
        maps = MM.load(z, len(out))
    for e in out:
        e["maps"] = [(mi, mp["name"]) for mi, mp in sorted(maps.items()) if e["index"] in mp["kinds"]]
    return out


REFINE = 0  # ★ 2026-10-09 (3): 갓 주운 장비 — 제련 0 (전에는 refineMax × 1.5 를 더했다). ★ 2026-10-10: §7 은 칸마다 `refine` 으로 준다


def acc_body(r):
    """장신구 몸통 — 길이붙은 설명 뒤부터(오프셋은 설명이 빈 레코드 기준)."""
    b = bytes.fromhex(r["bodyHex"])
    return bytes([0]) + b[1 + b[0]:]


def gear_stats(items, cls, slots, refine=None):
    """(무기 평균 공격, 방어 합, 능력치 보너스 5, HP 고정, SP 고정). 랜덤 옵션 = 기대값 (slots/2)/5 × 범위 평균(능력치마다).
    ★ 2026-10-10: `refine` = {칸: 제련 단계} — 아이템의 상한 body[21] 에서 자르고, 무기는 최소·최대에 ⌊1.5·c⌋ 를 더한 뒤 무기 보너스를 곱한다."""
    atk = 0.0
    defence = 0.0
    bonus = [0.0] * 5
    hp = sp = 0
    refine = refine or {}
    for slot, key in slots.items():
        r = items[key]
        lv = min(refine.get(slot, REFINE), r.get("refineMax") or 0)
        if slot.startswith("w"):
            share = 1.0 if slot == "w" else 0.0  # 둘째 단검은 기본공격에 안 더한다(스킬 둘만 합산) — ★ 2026-10-10: `a.a()I` 는 주무기만(종류 4)
            atk += share * ((r["power"] + r["powerMin"]) / 2 + (3 * lv) // 2) * WEAPON_BONUS[key[0]]
        elif slot == "acc":
            b = acc_body(r)
            for k in range(5):
                bonus[k] += (b[5 + 2 * k] + b[6 + 2 * k]) / 2
            defence += (b[17] + b[18]) / 2
            hp += struct.unpack_from("<I", b, 19)[0]
            sp += struct.unpack_from("<I", b, 27)[0]
            continue
        else:
            defence += r["power"] + (3 * lv) // 2
        n = (r.get("slots") or 0) / 2
        for k, (lo, hi) in enumerate(r.get("optionRanges") or []):
            bonus[k] += n / 5 * (lo + hi) / 2
    return atk, defence, bonus, hp, sp


def player(cls, level, items, set_name, slots=None, archetype=None, opts=None, refine=None):
    """플레이어 원형. `slots` 가 없으면 SETS[set_name][cls]; `archetype` 이 없으면 ARCHETYPE[cls]; `opts` = gear_options() (없으면 옵션 없음);
    `refine` = {칸: 제련 단계} (★ 2026-10-10, 없으면 0)."""
    base = BASE_STATS[cls]
    pts = 3 * (level - 1)
    arch = archetype or ARCHETYPE[cls]
    stats = [base[k] + int(pts * arch[k]) for k in range(5)]
    watk, gdef, bonus, hp_flat, sp_flat = gear_stats(items, cls, slots or SETS[set_name][cls], refine)
    o = opts or NO_OPTS
    eff = [stats[k] + bonus[k] for k in range(5)]  # 힘'·체력'·민첩'·의지'·지력'
    cs, ca = CALIB["str_term"][cls]
    ah = eff[0] * cs + eff[2] * ca
    atk = ah + watk
    # ★★ 2026-10-17 나이트 공격 스킬 × 지력 (CombatPatch::knight_int_bonus — 지력' · 179/512 = 힘 계수의 절반): 스킬의 공격력에만
    skill_atk = atk + (max(eff[4], 0) * KNIGHT_INT_MUL / 512 if cls == 1 else 0)
    ds, da = CALIB["def_str"][cls]
    defence = gdef + eff[0] * ds + eff[2] * da
    hp = ((eff[1] + level) * CALIB["hp_mul"][cls] + hp_flat) * (1 + o["max_hp"])
    sp = ((eff[3] + level) * 6 + sp_flat) * (1 + o["max_sp"])
    cb, cm = CALIB["crit_base"][cls]
    crit = cb + eff[2] * cm + o["crit"]
    ak = 10 - level - eff[2] / 10 - eff[4] / 10  # a.k = 10 − LV − AGI'/10 − INT'/10
    am = level + 10 + eff[2] / 3 + CALIB["am_extra"][cls] + o["evade"]
    return dict(cls=cls, level=level, stats=stats, eff=eff, atk=atk, skill_atk=skill_atk, defence=defence, hp=hp, sp=sp, crit=clamp(crit, 8, 90), ak=ak, am=am, set=set_name, opts=o)


def fight(p, g, correct=True):
    """플레이어 원형 p 가 가디언 템플릿 g 의 개체(k=1)를 상대로: 명중·치명·회피·타격·피격·처치/사망 타수·EXP."""
    L = g["L"] + 1
    hp, atk, e, defence = g["hp"] + 20, g["atk"] + 3, g["e"], g["defence"]
    Ls = seen_level(L, correct)
    miss = clamp(p["ak"] + Ls + e / 5, 8, 90)
    hit = (100 - miss) / 100
    crit2 = clamp(20 + Ls - p["level"], 8, 50) / 100
    crit = p["crit"] / 100 * crit2
    d = max(p["atk"] - defence, 0)
    basic = d * CALIB["combo_avg"][p["cls"]] * (1 + crit * (CALIB["crit_mul"] - 1)) * hit
    skill = MAIN_SKILL[p["cls"]][1](max(p["skill_atk"] - defence, 0)) * JOB_SKILL * (1 + crit * (CALIB["crit_mul"] - 1)) * hit
    taken = max(atk - p["defence"], 3)
    mcrit = clamp(20 + Ls - p["level"], 8, 50) / 100
    evade = clamp(p["am"] - (L if EVADE_TRUE else Ls) - e / 10, 8, 90) / 100  # (4) the evade site sees the true level
    taken_avg = taken * (1 + mcrit * (CALIB["mon_crit_mul"] - 1)) * (1 - evade)
    exp = kill_exp(L, p["level"])
    hits_basic = hp / basic if basic else float("inf")
    kill_s = hits_basic * CALIB["attack_frames"][p["cls"]] / 20
    hits_to_die = p["hp"] / taken_avg
    die_s = hits_to_die * CALIB["monster_attack_frames"] / 20
    return dict(L=L, Ls=Ls, hp=hp, atk=atk, defence=defence, miss=miss, crit=crit * 100, basic=basic, skill=skill, hits_basic=hits_basic, hits_skill=hp / skill if skill else float("inf"), taken=taken, mcrit=mcrit * 100, evade=evade * 100, taken_avg=taken_avg, hits_to_die=hits_to_die, exp=exp, kill_s=kill_s, die_s=die_s, margin=die_s / kill_s if kill_s else float("inf"))


MEASURED = """## 0. 실측 근거 (probe, 2026-10-08 밤 — 식의 상수는 전부 여기서 나왔다)

시험대 = 숲 슬롯 2 나이트(`h2save.py edit … --level 70/80/90 --stats 힘,999,민첩,12,12 --mode 3`, 체력 999 = 오래 살리기) · 슬롯 1 로그 LV 80 ·
`tools_re/guardian_fight.sh`(다른 일곱 마리를 `--h2-mob-hurt` 로 먼저 눕히고 한 마리를 `--h2-mob-place` 로 앞 칸에, OK 12 프레임마다) +
`tools_re/guardian_fight_parse.py`(한 마리뿐인 창 — 눕힌 것들은 ≈1,300 프레임 뒤 되살아난다). 상대 = 반란전투병 가디언 LV 86 · HP 4,018 · ATK 690 · DEF 27
(HELL 68 · 2,652 · 571 · 23 — `[h2-mobs]` 의 LV/atk, DEF 는 식).

| 무엇 | 실측 | 식 |
|---|---|---|
| 플레이어 기본공격 | 나이트 atk 145 → 115..121(평균 117.7) · atk 158 → 128..134 · 로그 atk 188 → 161 | **d = atk − DEF_m** (½ 이 아니다) ± 무기 min..max |
| 플레이어 치명 | 175..180 / 117.7 · 195..199 / 131 | **×1.5** |
| 몬스터 타격 | DEF 104 에 513..659 (평균 589) · DEF 98 에 519..665 | **ATK × U(0.9, 1.1) − DEF_p** |
| 몬스터 치명 | 1,437..1,515 / 575..606 | **×2.5** |
| 레벨 보정(READ 조각) | seen 220(`--h2-guardian-word 8:2560`) → 11 번 휘둘러 2 타 명중 · 몬스터 치명 7/12 · seen 60(`8:0`) → 몬스터 치명 1/10 · 회피 5/15 · 기본값(seen 69) LV 80 → 몬스터 치명 1/13, 보정 OFF → 4/14 · LV 70 같은 창에서 명중 33 vs 24 | 빗나감 = clamp(a.k + seen + e/5, 8, 90) · 몬스터 치명 = clamp(20 + seen − LV, 8, 50) · 회피 = clamp(a.m − seen − e/10, 8, 90) |
| `[h2-stats]` 나이트 | LV 80 힘 152 민첩 31: a.h 106 · DEF 104 · 치명 14 · a.k −74 · a.m 100 · HP 12,948(체력 999) · SP 552 | a.h = 힘'·7/10 · DEF = 장비 + 힘'/3 · 치명 = 8 + 민첩'/5 · a.k = 10 − LV − 민첩'/10 − 지력'/10 · a.m = LV + 10 + 민첩'/3 · HP = (체력' + LV)·12 · SP = (의지' + LV)·6 |
| `[h2-stats]` 로그 | LV 80 힘 82(+9) 민첩 155(+10): a.h 127 · DEF 135 · 치명 69 · a.m 151 — 힘 +50 → a.h +25 · DEF +13 — 민첩 +50 → a.h +25 · DEF +12 · 치명 +16 | a.h = (힘' + 민첩')/2 · DEF = 장비 + (힘' + 민첩')/4 · 치명 = 14 + 민첩'/3 · a.m = LV + 16 + 민첩'/3 |
| 로그 콤보 | 한 타 161 · 176 · 186 · 310 · 350(5 단) · 나이트 3 단 전부 118 | 로그 평균 ×1.45 · 나이트 ×1.0 |
| 스킬 배율 `--h2-skill`(Lv5, 직업 ×1.5 포함) | 나이트 a.p[4] 검기 **×5.853** · [5] 둔기강타 ×7.804 · [6] 방패치기 ×2.25 · 나머지 ×1.5 — 로그 [5] 일격 ×2.0 · [9]·[10] 급소가격·단검난무 **×5.5** · SP 나이트 110/55/55/69/92/55/138 · 로그 129/44/34/44/96/96 | 모델의 주력 스킬 = 나이트 검기 5.853 · 로그 5.5 · 버서커 가르기 2.0(미실측) |
| 박자 | 나이트 한 타 ≈ 26 프레임 · 로그 ≈ 12 · 붙은 몬스터 공격 15..20 회 / 700..900 프레임 | 처치 s · 사망 s 의 환산 |
| EXP | LV 80 이 LV 86 처치 → +1,900 · 다음 레벨 512,000 | bd.n·17·clamp(20 + bd.n − LV, 0, 28)/20 · L³ − L² + 80L |
| HP 상한 | 세이브 HP 30,000 → 로드 때 최대 12,948 로 잘림 | HP 는 최대를 못 넘는다 |

⛔ **미실측**: 버서커(슬롯에 버서커 세이브가 없고 직업만 바꾸면 장비가 안 맞아 로드에서 죽는다) — a.h · DEF · 콤보 · 가르기 배율은 디스어셈블 값(§8.11 · §8.26) ·
가디언 장비를 실제로 입힌 수치(레코드 값에서 계산) · 보스의 특수기(피격 루틴 밖 — 모델은 일반 타격만).

"""


def next_exp(L):
    return L ** 3 - L ** 2 + 80 * L


# ── ★ 2026-10-09 (3): 개성 옵션 · 난이도 · 야수나이트 ────────────────────────────────────────────────────────────────

DIFFICULTY = [("EASY", 1.0), ("NORMAL", 1.5), ("HARD", 2.0)]  # hero2java::DIFFICULTY_PCT
STAGES = {0: 4, 1: 3, 2: 5}  # 기본공격 단계 (a.i:[B)
NO_OPTS = dict(taken=1.0, reflect=0.0, sp=1.0, crit=0, evade=0, venom=0.0, heal_hp=0.0, heal_sp=0.0, dmg={}, lifesteal=0.0, beast_cd=1.0, max_hp=0.0, max_sp=0.0, beast_x2=False)


def parse_gear_items(path):
    """guest_anchors.rs 의 GearPatch::ITEMS → {(type, id): [(효과, 인수…)]} (앱이 쓰는 그 표 그대로)."""
    import re
    text = open(path, encoding="utf-8").read()
    out = {}
    for m in re.finditer(r'GearItem \{ item_type: (\d+), id: (\d+), text: "[^"]*", effects: &\[([^\]]*)\]', text):
        effs = [(name, tuple(int(x) for x in args.split(",") if x.strip())) for name, args in re.findall(r"(\w+)\(([^)]*)\)", m.group(3))]
        out[(int(m.group(1)), int(m.group(2)))] = effs
    assert len(out) == 31, f"GearPatch::ITEMS 가 {len(out)} 개"
    return out


def gear_options(gear_table, keys, cls):
    """GearPatch::effects_for 의 파이썬 판(분수로): 착용한 (type, id) 들 + 직업."""
    o = dict(NO_OPTS)
    o["dmg"] = {}
    for key in keys:
        if key == (11, 18):
            o["beast_x2"] = True  # 야수조련 목걸이 (AccessoryPatch — 야수 공격 ×2)
        for name, a in gear_table.get(key, []):
            if name == "Taken":
                o["taken"] *= a[0] / 256
            elif name == "Reflect":
                o["reflect"] += a[0] / 256
            elif name == "SpCost":
                o["sp"] *= a[0] / 256
            elif name == "Crit":
                o["crit"] += a[0]
            elif name == "Evade":
                o["evade"] += a[0]
            elif name == "Venom":
                o["venom"] += a[0] / 256
            elif name == "HealHp":
                o["heal_hp"] += a[0] / 256
            elif name == "HealSp":
                o["heal_sp"] += a[0] / 256
            elif name == "Dmg" and a[0] == cls:
                for k in range(a[1], a[2] + 1):
                    o["dmg"][k] = o["dmg"].get(k, 0) + a[3] / 256
            elif name == "Basic":
                for k in range(STAGES[cls]):
                    o["dmg"][k] = o["dmg"].get(k, 0) + a[0] / 256
            elif name == "Lifesteal":
                o["lifesteal"] += a[0] / 256
            elif name == "BeastCd":
                o["beast_cd"] *= a[0] / 256
            elif name == "MaxHp":
                o["max_hp"] += a[0] / 256
            elif name == "MaxSp":
                o["max_sp"] += a[0] / 256
    return o


# 주력 스킬 회전 (probe `--h2-skill` — 배율 = a(BILbd;)I(1000)/1000 은 직업 ×1.5 포함 · 쿨 = a(I)S 틱 · SP = a(I)I, Lv5 기준):
#   (이름, 종류 kind, 배율, 쿨 틱, SP)
ROTATION = {
    # 버서커 = S25 백업 슬롯 1 LV60 실측(그 세이브의 스킬 레벨 — Lv5 가 아닐 수 있다) · 쿨 = bl.a 표 ÷2 · SP 는 미실측(40 · 폭풍강타 60 으로 센다)
    0: [("폭풍강타", 9, 3.270, 60, 60), ("휘두르기", 5, 1.907, 30, 40), ("찍기", 4, 1.868, 40, 40), ("가르기", 6, 1.839, 60, 40)],
    1: [("검기", 4, 5.853, 25, 55), ("둔기강타", 5, 7.804, 35, 55)],  # 나이트 Lv5 (§0 실측)
    2: [("단검난무", 9, 5.5, 40, 96), ("일격", 5, 1.75, 17, 44)],  # 로그 (슬롯 1 실측)
}
# 야수 공격 스킬 Lv5: (이름, 링?, 쿨 틱, SP, 한 마리에 맞는 수)
BEAST_SKILLS = [("링(지옥불)", True, (14 - 5) * 10, (14 - 5) * 8, 1), ("중타격(학살화염)", False, (21 - 5) * 10, 144 - 5 * 5, 2)]


def beast_hit(p, ring, defence):
    INT = p["eff"][4]
    lv, bd = 5, p["level"]
    d = (bd + 30 + INT * 1.5) * (lv + 3) / 3 if ring else (bd * 5 / 2 + 20 + INT * 1.5) * (lv + 2) / 3
    d = max(d - defence, 0)
    return d * (2 if p["opts"]["beast_x2"] else 1)


SP_REGEN = 0.05  # 최대 SP 의 5 %/초 — probe 숲 나이트 LV26: 싸우는 동안 SP 228 에서 1 초에 +11 (자연 회복 + 콤보 획득)


# 상시 버프(쿨 60 초 · 만렙 지속 75 초 → 늘 켜 둔 것으로): 버서커 분노태세 Lv5 = 주는 피해 ×1.30 · 받는 피해 ×1.10 (§8.52) ·
# 나이트 신의가호 = 검기 7.8 → 12.0 (공격력 대비, §8.45 ⑫) · 로그 맹독감염 Lv5 + 독연마 = 기본공격이 건 중독이 0.5 초마다
# 공격력 × (28·5 + 20·3)/100 = 2.0 배 (★ 2026-10-09 (4) 사용자: 300 → 200 %) — 방어 없이(`bd.a(IB)V`), × (1 + 중독 옵션)
BUFF = {0: dict(dmg=1.30, taken=1.10), 1: dict(skill={4: 12.0 / 7.8}), 2: dict(poison=2.0 * 20 / 10)}  # ★ (4) 맹독 300 → 200 %


def rotation_dps(p, g, beast=False):
    """지속 회전: 주력 스킬(과 야수 스킬)을 쿨마다 쓰고 싶지만 SP 가 SP_REGEN 만큼만 들어오므로 그 비율로 줄인다.
    → (DPS, 스킬 가동률 0..1, 기본공격 시간 몫, 비치명 기본 한 타 피해(명중 포함))."""
    cls = p["cls"]
    o = p["opts"]
    L = g["L"] + 1
    Ls = seen_level(L)
    hit = (100 - clamp(p["ak"] + Ls + g["e"] / 5, 8, 90)) / 100
    crit = p["crit"] / 100 * clamp(20 + Ls - p["level"], 8, 50) / 100
    critx = 1 + crit * (CALIB["crit_mul"] - 1)
    d = max(p["atk"] - g["defence"], 0)
    swing = CALIB["attack_frames"][cls] / 20
    buff = {} if beast else BUFF[cls]
    bx = buff.get("dmg", 1.0)
    plain = d * CALIB["combo_avg"][cls] * (1 + o["dmg"].get(0, 0)) * hit * bx
    basic_dps = plain * critx / swing
    want = []  # (dps if fully on, time share, SP/s if fully on)
    free = 1.0
    for name, kind, mul, cd, sp in ROTATION[cls]:
        per = max(p["skill_atk"] - g["defence"], 0) * mul * buff.get("skill", {}).get(kind, 1.0) * bx * (1 + o["dmg"].get(kind, 0)) * critx * hit
        share = min(swing / (cd / 20), free)
        want.append((per / swing * share, share, sp * o["sp"] * share / swing))
        free -= share
    beasts = []
    if beast:
        for name, ring, cd, sp, hits in BEAST_SKILLS:
            rate = 1 / (cd * o["beast_cd"] / 20)
            beasts.append((beast_hit(p, ring, g["defence"]) * hits * rate, sp * o["sp"] * rate))
    sp_want = sum(w[2] for w in want) + sum(b[1] for b in beasts)
    scale = min(1.0, SP_REGEN * p["sp"] / sp_want) if sp_want > 0 else 1.0
    used = sum(w[1] for w in want) * scale
    dps = sum(w[0] for w in want) * scale + basic_dps * (1 - used) + sum(b[0] for b in beasts) * scale
    if buff.get("poison") and (1 - used) > 0:
        dps += p["atk"] * buff["poison"] * (1 + o["venom"])  # 중독은 기본공격이 걸어 두면 5 초 — 기본공격을 섞는 한 늘 걸려 있다
    return dps, scale, 1 - used, plain


def duel(p, g, diff, beast=False):
    """한 마리(k=1)와의 결투, 난이도 배율 diff: 처치 s(지속 회전) · 사망 s · 여유 · 처치당 HP 손실 %."""
    o = p["opts"]
    L = g["L"] + 1
    Ls = seen_level(L)
    hp = (g["hp"] + 20) * diff  # ★★ 2026-10-16 monster HP is an int (GuestBuild::monster_hp_int) — no 32,767
    atk = min((g["atk"] + 3) * diff, 32767)
    dps, scale, basic_share, plain = rotation_dps(p, g, beast)
    taken = max(atk - p["defence"], 3) * o["taken"] * ({} if beast else BUFF[p["cls"]]).get("taken", 1.0)
    mcrit = clamp(20 + Ls - p["level"], 8, 50) / 100
    evade = clamp(p["am"] - (L if EVADE_TRUE else Ls) - g["e"] / 10, 8, 90) / 100
    mon_rate = 20 / CALIB["monster_attack_frames"]
    taken_dps = taken * (1 + mcrit * (CALIB["mon_crit_mul"] - 1)) * (1 - evade) * mon_rate
    reflect_dps = taken * o["reflect"] * (1 - evade) * mon_rate
    swing = CALIB["attack_frames"][p["cls"]] / 20
    leech = plain * o["lifesteal"] / swing * basic_share
    kill_s = hp / (dps + reflect_dps) if dps + reflect_dps > 0 else float("inf")
    net = taken_dps - leech
    die_s = p["hp"] / net if net > 0 else float("inf")
    loss = max(net * kill_s - o["heal_hp"] * p["hp"], 0) / p["hp"] * 100
    return dict(kill_s=kill_s, die_s=die_s, margin=die_s / kill_s if kill_s else float("inf"), loss=loss, sp_s=scale * 100, dps=dps, taken_dps=taken_dps, leech=leech)


# 가디언 장비 한 벌(「신규템을 다 맞췄다」) — 직업마다 옵션이 맞는 것, 로그는 경갑만 · 버서커는 중갑에 「중장비」를 찍은 것으로
LOADOUTS = {
    "버서커": (0, None, {"w": (0, 33), "h": (5, 28), "a": (6, 53), "g": (8, 52), "s": (9, 54), "acc": (11, 20)}),
    "나이트": (1, None, {"w": (2, 28), "sh": (10, 28), "h": (5, 28), "a": (6, 53), "g": (8, 52), "s": (9, 54), "acc": (11, 20)}),
    "로그": (2, None, {"w": (3, 34), "w2": (3, 33), "h": (5, 29), "a": (6, 52), "g": (8, 52), "s": (9, 54), "acc": (11, 20)}),
    # 의지·지력·체력을 레벨마다 1 씩 (사용자) — 힘 0. 야수 쿨 −20 % 신발 · SP 줄이는 투구 · SP 회복 장갑 · 야수조련 목걸이(야수 ×2)
    "야수나이트": (1, (0.0, 1 / 3, 0.0, 1 / 3, 1 / 3), {"w": (2, 28), "sh": (10, 28), "h": (5, 31), "a": (6, 53), "g": (8, 54), "s": (9, 51), "acc": (11, 18)}),
    "야수나이트(현자)": (1, (0.0, 1 / 3, 0.0, 1 / 3, 1 / 3), {"w": (2, 28), "sh": (10, 28), "h": (5, 31), "a": (6, 53), "g": (8, 54), "s": (9, 51), "acc": (12, 20)}),
}
HELL_LOADOUT = {name: (cls, arch, SETS["HELL 종결"][cls]) for name, (cls, arch, _) in LOADOUTS.items()}


def judge(m, loss):
    if m < 1:
        return "❌ 진다"
    if m < 2 or loss > 60:
        return "어려움"
    if m < 4 or loss > 30:
        return "조금 어려움"
    if m < 10:
        return "보통"
    return "쉬움"


def section6(items, gear_table, rows):
    out = ["## ★ 6. 확정 옵션 · 난이도 · 직업간 (2026-10-09 (3)) — 이 문서의 결론\n"]
    out.append("가디언 모드(모드 3) · 적 = 맵에 배치된 일반 적(보스 제외, §1) · 개체 k=1 · 장비 = **가디언 한 벌**(갓 주운 것: 제련 0 · 랜덤 옵션 기대값) + 확정 개성 옵션 31 "
               "(`GearPatch::ITEMS`) + 상시 버프(버서커 분노태세 · 나이트 신의가호 · 로그 맹독감염 — 쿨 60 초 · 지속 75 초라 늘 켜 둔다). 난이도 = 환경설정 「난이도」(몬스터 HP · 공격 ×1 / ×1.5 / ×2). "
               "스킬은 SP 가 들어오는 만큼만(최대 SP 의 5 %/초 — 가동률 열). 처치 s = 주력 스킬 회전(쿨마다 · 남는 시간 기본공격, "
               "야수나이트 = 기본공격 + 야수 링·중타격을 쿨마다) · 사망 s = HP ÷ (받는 피해/초 − 흡혈/초) · 여유 = 사망 s ÷ 처치 s · "
               "처치당 HP = 한 마리 잡는 동안 잃는 HP(처치 회복 뺌) ÷ 최대 HP. 판정: 여유 < 1 ❌ · < 2 또는 처치당 > 60 % 어려움 · < 4 또는 > 30 % 조금 어려움 · < 10 보통 · 그 위 쉬움.\n")
    out.append("| 장비 한 벌 | 직업 | 무기 · 투구 · 갑옷 · 장갑 · 신발 · 방패/보조 · 장신구 |\n|---|---|---|")
    for name, (cls, arch, slots) in LOADOUTS.items():
        out.append(f"| 가디언 | {name} | " + " · ".join(f"{items[v]['name']}" for v in slots.values()) + " |")
    out.append("")
    summ = []
    for blo, label in BANDS:
        lo, hi = band_range(blo)
        band = [(e, h, g) for e, h, g in rows if lo <= g["L"] <= hi and e["gate"] != 0]
        bosses = [(e, h, g) for e, h, g in rows if lo <= g["L"] <= hi and e["gate"] == 0]
        out.append(f"### {label} — 가디언 LV {lo}..{hi}, 일반 적 {len(band)} 종 · 플레이어 LV {blo}\n")
        out.append("| 직업 | 장비 | 난이도 | ATK/DEF/HP/SP | 처치 s 평균 | 사망 s 평균 | 여유 평균 | 여유 최소 (적) | 처치당 HP % 평균/최대 | 스킬 가동률 % | 판정 | 보스 여유 최소 |\n|---|---|---|---|---|---|---|---|---|---|---|---|")
        for name, (cls, arch, slots) in LOADOUTS.items():
            beast = name.startswith("야수")
            for set_name, sl, dname, diff in [("가디언+옵션", slots, "EASY", 1.0), ("가디언+옵션", slots, "NORMAL", 1.5), ("HELL 종결", HELL_LOADOUT[name][2], "NORMAL", 1.5), ("가디언+옵션", slots, "HARD", 2.0)]:
                opts = gear_options(gear_table, list(sl.values()), cls) if set_name.startswith("가디언") else gear_options({}, list(sl.values()), cls)
                p = player(cls, blo, items, None, slots=sl, archetype=arch, opts=opts)
                ds = [(duel(p, g, diff, beast), e["name"]) for e, h, g in band]
                if not ds:
                    continue
                inf = lambda v: v if v != float("inf") else 9999
                ks = sum(inf(d["kill_s"]) for d, _ in ds) / len(ds)
                dd = sum(inf(d["die_s"]) for d, _ in ds) / len(ds)
                mm = sum(min(d["margin"], 999) for d, _ in ds) / len(ds)
                wd, wn = min(ds, key=lambda x: x[0]["margin"])
                la = sum(d["loss"] for d, _ in ds) / len(ds)
                lm = max(d["loss"] for d, _ in ds)
                sp_s = sum(d["sp_s"] for d, _ in ds) / len(ds)
                bm = min((duel(p, g, diff, beast)["margin"], e["name"]) for e, h, g in bosses) if bosses else None
                verdict = judge(wd["margin"], la)
                f = lambda v: "∞" if v >= 999 else f"{v:.0f}" if v >= 10 else f"{v:.1f}"
                out.append(f"| {name} | {set_name} | {dname} | {p['atk']:.0f}/{p['defence']:.0f}/{p['hp']:.0f}/{p['sp']:.0f} | {f(ks)} | {f(dd)} | ×{f(mm)} | ×{f(wd['margin'])} ({wn}) | {la:.0f}/{min(lm, 999):.0f} | {f(sp_s)} | {verdict} | "
                           + (f"×{f(bm[0])} ({bm[1]})" if bm else "—") + " |")
                summ.append((label, name, set_name, dname, mm, wd["margin"], la, verdict, ks))
        out.append("")
    out.append("### 6b. 한눈에 — 띠 × 직업, 가디언 한 벌 (EASY → NORMAL), 여유 최소 · 처치 s · 판정\n")
    out.append("| 띠 | 직업 | EASY 여유 최소 · 처치 s | EASY 판정 | NORMAL 여유 최소 · 처치 s | NORMAL 판정 |\n|---|---|---|---|---|---|")
    for blo, label in BANDS:
        for name in LOADOUTS:
            e = next((x for x in summ if x[0] == label and x[1] == name and x[2] == "가디언+옵션" and x[3] == "EASY"), None)
            n = next((x for x in summ if x[0] == label and x[1] == name and x[2] == "가디언+옵션" and x[3] == "NORMAL"), None)
            if e and n:
                out.append(f"| {label} | {name} | ×{min(e[5], 999):.1f} · {e[8]:.0f} s | {e[7]} | ×{min(n[5], 999):.1f} · {n[8]:.0f} s | {n[7]} |")
    out.append("")
    return out


# ── ★ 2026-10-10: 기존 종결 + 제련 (§7) ──────────────────────────────────────────────────────────────────────────────

def refine_step(c, cap):
    """`ck.d()V` @0x15d470: 현재 c 에서 한 단계 성공 확률 — h.a(1, 100) ≤ (cap − c)·100 // cap + 30."""
    return min(100, (cap - c) * 100 // cap + 30) / 100


def refine_cost(n, cap):
    """사본 하나로 +n: (닿을 확률, 필요한 사본, 제련석, 수수료 골드) 기댓값. 실패 = 그 사본 파괴(+ 자동 저장)라 다음 사본은 0 부터 —
    사본마다 같은 분포라 총비용 = 사본 하나의 기대 비용 ÷ 닿을 확률(Wald). 한 단계 비용(`ck.e()V`) = 제련석 c+1 · 골드 (c+1)·100(실패해도 낸다)."""
    q, stones, fee = 1.0, 0.0, 0.0
    for c in range(n):
        stones += q * (c + 1)
        fee += q * (c + 1) * 100
        q *= refine_step(c, cap)
    return q, 1 / q, stones / q, fee / q


# 시나리오 = (이름, 규칙): 정수 = 그 단계 · "free" = 성공 100 % 인 단계까지(사본을 잃지 않는다) · ("copies", k) = 사본 기댓값 k 이하인 가장 높은
# 단계 · "max" = 상한. 상한 50 → 0 · 16 · 25 · 30 · 50 / 25 → 0 · 8 · 15 · 18 · 25 / 20 → 0 · 7 · 12 · 15 · 20
REFINE_RULES = [("+0", 0), ("무손실", "free"), ("사본 ~3", ("copies", 3.0)), ("사본 ~10", ("copies", 11.0)), ("최대", "max")]
REFINE_SLOTS = ("w", "a", "sh")  # 모델이 세는 제련 칸 — 로그의 보조 단검(w2)은 종류 3 타격에만 쓰여 모델 밖(§7a)
REFINE_TABLE_LEVELS = {50: (16, 20, 22, 24, 25, 26, 28, 30, 32, 35, 40, 45, 50), 25: (8, 10, 12, 14, 15, 16, 18, 20, 22, 25), 20: (7, 8, 10, 12, 14, 15, 16, 18, 20)}
EXISTING = ("버서커", "나이트", "로그", "야수나이트")  # LOADOUTS 의 직업 원형 — 야수나이트(현자)는 장신구만 다른 같은 원형이라 뺀다
HERO_ACCS = [(11, 14), (11, 15), (11, 16), (12, 14), (12, 15), (12, 16)]  # 원작 영웅 장신구(LV 55..60) = 종결 장신구(make_hero2_asset.tier_of)
SHORT = {"❌ 진다": "❌", "어려움": "어려움", "조금 어려움": "조금 어려움", "보통": "보통", "쉬움": "쉬움"}


def refine_level(cap, rule):
    if cap <= 0:
        return 0
    if rule == "max":
        return cap
    if isinstance(rule, int):
        return min(rule, cap)
    best = 0
    for n in range(cap + 1):
        q, copies, _, _ = refine_cost(n, cap)
        if (q >= 1.0) if rule == "free" else (copies <= rule[1]):
            best = n
    return best


def refine_plan(items, slots, rule):
    """{칸: 단계} — 제련할 수 있고(상한 > 0) 모델이 세는 칸만."""
    return {s: refine_level(items[slots[s]].get("refineMax") or 0, rule) for s in REFINE_SLOTS if s in slots and (items[slots[s]].get("refineMax") or 0) > 0}


def drop_rates(src, items, keys, mode=3):
    """(type, id) → (배치된 적 평균 처치당 확률, 가장 잘 떨구는 적의 확률, 그 적 이름) — decode_drops 의 표 그대로(모드 3 = 가디언: HELL 의 창 ·
    보너스 칸 + 가디언 칸, 난이도 NORMAL+ 칸 포함)."""
    ens = DD.parse_enm(DD.load_member(src, "enm/enm"))
    with zipfile.ZipFile(src) as z:
        placed = MM.placed(MM.load(z, len(ens)))
    tot = {k: 0.0 for k in keys}
    best = {k: (0.0, "") for k in keys}
    n = 0
    for e in ens:
        if e["index"] not in placed:
            continue
        odds = DD.table_odds(e, mode)
        if not odds:
            continue
        n += 1
        per = {}
        for (_k, t, lo, hi, p) in odds:
            ids = [i for i in range(lo, hi + 1) if (t, i) in items]
            for i in ids:
                per[(t, i)] = per.get((t, i), 0.0) + p / len(ids)
        for k in keys:
            v = per.get(k, 0.0)
            tot[k] += v
            if v > best[k][0]:
                best[k] = (v, e["name"])
    return {k: (tot[k] / max(n, 1), best[k][0], best[k][1]) for k in keys}


def band_fight(items, rows, name, slots, blo, diff, opts, refine=None):
    """§6 과 같은 결투를 그 띠의 일반 적 전부와: 여유 최소 · 판정 · 1:1 로 지는(여유 < 1) 적 수 · 여유 < 2 인 적 수 · 보스 여유 최소."""
    cls, arch, _ = LOADOUTS[name]
    lo, hi = band_range(blo)
    band = [(e, g) for e, h, g in rows if lo <= g["L"] <= hi and e["gate"] != 0]
    bosses = [(e, g) for e, h, g in rows if lo <= g["L"] <= hi and e["gate"] == 0]
    beast = name.startswith("야수")
    p = player(cls, blo, items, None, slots=slots, archetype=arch, opts=opts, refine=refine)
    ds = [(duel(p, g, diff, beast), e["name"]) for e, g in band]
    w, wn = min(ds, key=lambda x: x[0]["margin"])
    loss = sum(d["loss"] for d, _ in ds) / len(ds)
    kill = sum(min(d["kill_s"], 9999) for d, _ in ds) / len(ds)
    boss = min(((duel(p, g, diff, beast)["margin"], e["name"]) for e, g in bosses), default=None)
    return dict(p=p, worst=w["margin"], worst_name=wn, loss=loss, kill=kill, n=len(ds), lethal=sum(1 for d, _ in ds if d["margin"] < 1),
                hard=sum(1 for d, _ in ds if d["margin"] < 2), verdict=judge(w["margin"], loss), boss=boss)


def existing_slots(items, rows, name):
    """원작 종결 한 벌(SETS["HELL 종결"] = §2) + 원작 영웅 장신구 여섯 중 +0 · EASY/NORMAL · 네 띠의 여유 최소(10 에서 자름) 합이 가장 큰 것."""
    cls = LOADOUTS[name][0]
    best = None
    for acc in HERO_ACCS:
        slots = dict(SETS["HELL 종결"][cls])
        slots["acc"] = acc
        opts = gear_options({}, list(slots.values()), cls)
        score = sum(min(band_fight(items, rows, name, slots, blo, diff, opts)["worst"], 10) for blo, _ in BANDS for diff in (1.0, 1.5))
        if best is None or score > best[0]:
            best = (score, slots)
    return best[1]


def margin_text(m):
    return "×∞" if m >= 999 else f"×{m:.0f}" if m >= 10 else f"×{m:.2f}"


def fight_cell(r):
    return f"{margin_text(r['worst'])} {SHORT[r['verdict']]}" + (f" ({r['lethal']}/{r['n']})" if r["lethal"] else "")


def big(v):
    return "0" if v < 0.05 else f"{v:.1e}" if v >= 1e7 else f"{v:,.0f}" if v >= 10 else f"{v:.1f}"


def section7(items, gear_table, rows, src):
    out = ["## ★ 7. 기존 종결 + 제련 — 가디언 아이템 없이 깰 수 있나 (2026-10-10)\n"]
    out.append("사용자: *\"가디언 모드가 신규아이템말고 기존 종결아이템들을 제련했을때도 깰수있는지 직업별 밸런스 조사해줘\"*. §6 과 같은 결투(식 · 상시 버프 · "
               "스킬 회전 · SP · 난이도 · 판정 전부 그대로)에서 **장비만** 원작 종결 한 벌(§2 의 「HELL 종결」 + 직업마다 가장 나은 원작 영웅 장신구)과 그 제련으로 "
               "바꾼다 — 원작 아이템에는 개성 옵션(`GearPatch`)이 없다. 칸 = **여유 최소**(그 띠의 일반 적 중 가장 위험한 적과 1:1 — §6 의 「여유 최소」) · "
               "판정 · (1:1 로 지는 = 여유 < 1 인 적 종류 / 그 띠의 일반 적 종류). 가디언 +0 = §6 의 가디언 한 벌 + 옵션(같은 표의 값).\n")
    out.append("모델 밖(모든 장비에 똑같이): 포션 · 부활의 구 · 각성기 · 로그 「암살」 · 보스 특수기 · 세트 효과(`a.m()V` — 다섯 칸이 같은 세트면 능력치 전부 +2) · "
               "기사 방패 막기(`a.a(Lbd;SB)V` — 확률 a.q 는 LV · a.p[11] 에 따라 8..90 %, 막아도 피해 − 방패 방어(40..50 + 제련)라 작다) · 랜덤 옵션은 기댓값.\n")
    # 7a — 규칙
    out.append("### 7a. 제련 규칙 — 게임 코드 (HERO2_GUEST_RE.md §8.62)\n")
    out.append("| | |\n|---|---|")
    out.append("| 성공 | `ck.d()V` @0x15d470: `h.a(1, 100) ≤ (상한 − 현재)·100 // 상한 + 30` — 상한의 ≈70 % 까지는 100 %, 마지막 한 단계는 ≈ 30 % |")
    out.append("| 실패 | **장비 파괴**(`bq.a(BI)V` 로 가방에서 지운다) + **즉시 자동 저장**(`ag.l()V`; `t/bsm.tdf[0]` 「제련에 실패하면 장비가 파괴되며, 자동으로 세이브됩니다.」) — "
               "다시 불러와 되돌릴 수 없다. 성공은 저장하지 않는다 |")
    out.append(f"| 비용 | `ck.e()V` @0x15d800: 현재 c → c+1 에 **제련석 c+1 개 + 골드 (c+1)·100** — 성공·실패 모두 낸다. 제련석 = 상점 {items[(20, 0)]['price']} 원 · "
               "처치마다 clamp(20 + 몹LV − 내LV, 8, 40) % |")
    out.append("| 효과 | 무기 공격 최소·최대 **+⌊1.5·c⌋** (`bo.a()S`/`bo.b()S` — `CombatPatch` 의 무기 보너스 ×1.1 · ×1.2 는 그 합에 곱한다) · 방어구 방어 **+⌊1.5·c⌋** "
               "(`bu.a()I` — `a.m()V` 가 투구·갑옷·장갑·신발을, 기사 방패는 `bx.c()V`) |")
    out.append("| 상한 | body[21]: 종결 무기 **50** · 중갑 갑옷 · 방패 **25** · 경갑 갑옷 **20** · 투구 · 장갑 · 신발 · 장신구 **0**(제련 불가). 가디언 장비도 같다 |")
    out.append("| 로그 | 전투 공격 `a.a()I` = a.h + **주무기** 굴림(`a.a(B)S` 종류 4) — 보조 단검은 종류 3 타격만, 장비창의 공격 `a.g` = 두 단검 평균(`ay.c()V`)은 표시용. 모델은 주무기만 센다 |")
    out.append("")
    # 7b — 사본 하나로 +n
    out.append("### 7b. 한 사본으로 +n — 확률 · 필요한 사본 · 비용 (기댓값)\n")
    out.append("사본 = 깨진 것 포함 필요한 같은 아이템의 수 · 제련석 · 수수료 = 그 사본들에 쓰는 전부 · 골드 합 = 수수료 + 제련석을 전부 상점에서 산 값.\n")
    out.append("| 상한 | +n | 마지막 단계 성공 % | 한 사본이 닿을 % | 사본 | 제련석 | 수수료 골드 | 골드 합 |\n|---|---|---|---|---|---|---|---|")
    stone_price = items[(20, 0)]["price"]
    for cap, levels in REFINE_TABLE_LEVELS.items():
        for n in levels:
            q, copies, stones, fee = refine_cost(n, cap)
            out.append(f"| {cap} | +{n} | {refine_step(n - 1, cap) * 100:.0f} | {q * 100:.3g} | {big(copies)} | {big(stones)} | {big(fee)} | {big(fee + stones * stone_price)} |")
    out.append("")
    # 7c — 장비와 시나리오
    out.append("### 7c. 기존 종결 한 벌 · 제련 시나리오 · 모으는 비용\n")
    loadouts = {name: existing_slots(items, rows, name) for name in EXISTING}
    out.append("| 직업 | 무기 · 투구 · 갑옷 · 장갑 · 신발 · 방패/보조 · 장신구 (제련 상한) |\n|---|---|")
    for name, slots in loadouts.items():
        out.append(f"| {name} | " + " · ".join(f"{items[k]['name']}" + (f"({items[k]['refineMax']})" if items[k].get("refineMax") else "") for k in slots.values()) + " |")
    out.append("")
    out.append("시나리오 = 상한마다의 단계(" + " · ".join(f"**{sn}** " + "/".join(f"+{refine_level(cap, rule)}" for cap in (50, 25, 20)) for sn, rule in REFINE_RULES)
               + " — 무기 50 / 중갑·방패 25 / 경갑 20). 모두 모델이 세는 칸(무기 · 갑옷 · 방패)의 합: **더 모을 사본** = 지금 입은 한 벌 말고 제련하다 깨질 몫까지 "
               "더 필요한 같은 아이템 · 제련석 · 골드 합(수수료 + 제련석을 전부 상점에서) · 사본 처치 = 그 사본을 가디언 모드에서 모으는 처치 수 — 칸마다 가장 잘 떨구는 "
               "적만 잡을 때 / 맵에 배치된 적 평균으로 · 비용 처치 = 제련석과 수수료를 처치로 버는 수(80 대 띠 평균: 처치 하나 = 제련석 clamp(20 + 몹LV − 80, 8, 40) % "
               "+ 골드 65 % × (몹LV × 4 + 20) — `bd.q()V` · decode_drops 머리말, 골드는 제련석 값으로 환산).\n")
    keys = sorted({k for slots in loadouts.values() for s, k in slots.items() if s in REFINE_SLOTS})
    rates = drop_rates(src, items, keys, 3)
    lo80, hi80 = band_range(80)
    band80 = [g for e, h, g in rows if lo80 <= g["L"] <= hi80 and e["gate"] != 0]
    per_kill = sum(clamp(20 + g["L"] - 80, 8, 40) / 100 + 0.65 * (g["L"] * 4 + 20) / stone_price for g in band80) / len(band80)
    out.append("| 직업 | 시나리오 | 무기 · 갑옷 · 방패 | LV 80 ATK / DEF / HP | 더 모을 사본 | 제련석 | 골드 합 | 사본 처치 (최적 / 평균) | 비용 처치 |\n|---|---|---|---|---|---|---|---|---|")
    for name, slots in loadouts.items():
        cls = LOADOUTS[name][0]
        opts = gear_options({}, list(slots.values()), cls)
        for sn, rule in REFINE_RULES:
            plan = refine_plan(items, slots, rule)
            p = player(cls, 80, items, None, slots=slots, archetype=LOADOUTS[name][1], opts=opts, refine=plan)
            extra = stones = fee = kb = ka = 0.0
            for s, lv in plan.items():
                k = slots[s]
                _, c, st, fe = refine_cost(lv, items[k]["refineMax"])
                extra += c - 1
                stones += st
                fee += fe
                avg, best, _ = rates[k]
                kb += (c - 1) / best if best else float("inf")
                ka += (c - 1) / avg if avg else float("inf")
            lv_txt = " · ".join(f"+{plan.get(s, 0)}" if s in slots else "—" for s in REFINE_SLOTS)
            out.append(f"| {name} | {sn} | {lv_txt} | {p['atk']:.0f} / {p['defence']:.0f} / {p['hp']:.0f} | {big(extra)}" + (" (사실상 불가)" if extra >= 1e4 else "")
                       + f" | {big(stones)} | {big(fee + stones * stone_price)} | {big(kb)} / {big(ka)} | {big((stones + fee / stone_price) / per_kill)} |")
    out.append("")
    out.append(f"80 대 띠의 처치 하나 ≈ 제련석 {per_kill:.2f} 개 몫(제련석 확률 + 골드).\n")
    out.append("가디언 모드 처치당 확률 (제련 칸): " + " · ".join(f"{items[k]['name']} {rates[k][0] * 100:.3f} % (최적 {rates[k][2]} {rates[k][1] * 100:.2f} %)" for k in keys) + "\n")
    # 7d — 띠 × 직업 × 난이도
    out.append("### 7d. 띠 × 직업 × 난이도 — 여유 최소 · 판정 (1:1 로 지는 적 / 일반 적)\n")
    summary = {}
    for blo, label in BANDS:
        lo, hi = band_range(blo)
        out.append(f"#### {label} — 가디언 LV {lo}..{hi} · 플레이어 LV {blo}\n")
        out.append("| 직업 | 난이도 | " + " | ".join(f"기존 종결 {sn}" for sn, _ in REFINE_RULES) + " | 가디언 +0 (§6) |\n|---|---|" + "---|" * (len(REFINE_RULES) + 1))
        for name, slots in loadouts.items():
            cls = LOADOUTS[name][0]
            opts = gear_options({}, list(slots.values()), cls)
            gslots = LOADOUTS[name][2]
            gopts = gear_options(gear_table, list(gslots.values()), cls)
            for dname, diff in DIFFICULTY:
                cells = []
                for sn, rule in REFINE_RULES:
                    r = band_fight(items, rows, name, slots, blo, diff, opts, refine_plan(items, slots, rule))
                    summary[(blo, name, dname, sn)] = r
                    cells.append(fight_cell(r))
                g = band_fight(items, rows, name, gslots, blo, diff, gopts)
                summary[(blo, name, dname, "가디언")] = g
                out.append(f"| {name} | {dname} | " + " | ".join(cells) + f" | {fight_cell(g)} |")
        out.append("")
    # 7e — 한눈에
    out.append("### 7e. 한눈에 — 「사본 ~3」(현실적인 제련)에서 띠마다 판정 · 「최대」에서 달라지는 것\n")
    out.append("| 직업 | 난이도 | 70 대 | 80 대 | 90 대 | 99 만렙 | 최대 제련이면 |\n|---|---|---|---|---|---|---|")
    for name in loadouts:
        for dname, _ in DIFFICULTY:
            cells, changed = [], []
            for blo, label in BANDS:
                r = summary[(blo, name, dname, "사본 ~3")]
                cells.append(fight_cell(r))
                m = summary[(blo, name, dname, "최대")]
                if SHORT[m["verdict"]] != SHORT[r["verdict"]]:
                    changed.append(f"{label.split()[0]} → {SHORT[m['verdict']]}")
            out.append(f"| {name} | {dname} | " + " | ".join(cells) + " | " + (" · ".join(changed) or "같다") + " |")
    out.append("")
    # 7f — 보스
    out.append("### 7f. 보스 (드랍 게이트 0) — 여유 최소 · EASY / NORMAL\n")
    out.append("§6 의 「보스 여유 최소」와 같은 값 — 이 모델의 보스는 장비와 무관하게 낮게 나오므로(일반 타격만 · 포션 · 각성기 · 특수기 없음) "
               "가디언 +0 열과 나란히 볼 것. 부활의 구(5,000 원) = 그 자리에서 HP · SP 가득 → ⌈1/여유⌉ − 1 개면 쓰러지며 이어서 잡는다(보스 회복은 무시). "
               "⚠ 보스 피격 루틴의 치명(과 무기 특성 「투지」)은 원작 버그로 **× ⌊치명 % / 10⌋** 였고(HERO2_GUEST_RE.md §8.63) 2026-10-10 「2」로 "
               "**× max(⌊치명 % / 10⌋, 무기 치명 배율 a.k ÷ 10)** 가 됐다(`BossCritPatch`) — 이 모델은 ×1.5 로 센다: 치명 20 % 미만이면 무기 배율 ×1.3..2.6 이라 "
               "모델과 비슷하고, 치명 60..90 % 인 로그는 ×6..9 라 보스 여유가 표보다 높다.\n")
    out.append("| 띠 | 직업 | 난이도 | 기존 +0 | 기존 사본 ~3 | 기존 최대 | 가디언 +0 | 부활의 구 (기존 사본 ~3 / 가디언 +0) | 보스 |\n|---|---|---|---|---|---|---|---|---|")
    orbs = lambda m: max(math.ceil(1 / m) - 1, 0) if m > 0 else "∞"
    for blo, label in BANDS:
        for name in loadouts:
            for dname, _ in DIFFICULTY[:2]:
                rs = [summary[(blo, name, dname, sn)] for sn in ("+0", "사본 ~3", "최대", "가디언")]
                if rs[0]["boss"] is None:
                    continue
                out.append(f"| {label} | {name} | {dname} | " + " | ".join(margin_text(r["boss"][0]) for r in rs)
                           + f" | {orbs(rs[1]['boss'][0])} / {orbs(rs[3]['boss'][0])} | {rs[1]['boss'][1]} |")
    out.append("")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source")
    ap.add_argument("--md", default="HERO2_GUARDIAN_BALANCE.md")
    ap.add_argument("--no-correct", action="store_true")
    ap.add_argument("--anchors", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "wie_android", "src", "guest_anchors.rs"))
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    items = {(r["typeId"], r["index"]): r for r in decode_all(a.source)}
    ens = [e for e in enemies(a.source) if e["maps"]]
    rows = []
    for e in ens:
        h = hell_template(e)
        g = guardian_template(h, e["gate"])
        rows.append((e, h, g))
    rows.sort(key=lambda r: (r[2]["L"], r[0]["index"]))
    out = []
    out.append("# 가디언 모드 전수 밸런스 (모델 · 2026-10-08, ★ 2026-10-09 (3) 확정 옵션·난이도·야수나이트 · ★ 2026-10-10 기존 종결 + 제련 §7) — 생성 `tools_re/guardian_balance.py`\n")
    out.append("식과 보정 상수는 스크립트 머리말. 적 = 맵에 배치된 전부(가디언 레벨 순). 개체 = 스폰 보너스 k=1 (LV +1 · HP +20 · ATK +3). 보정 OFF 표 = `--no-correct`.\n")
    gear_table = parse_gear_items(a.anchors)
    out.extend(section6(items, gear_table, rows))
    out.extend(section7(items, gear_table, rows, a.source))
    out.append(MEASURED)
    out.append("## 1. 몬스터 — HELL → 가디언\n")
    out.append("| # | 이름 | 맵 | HELL LV/HP/ATK/DEF | **가디언 LV/HP/ATK/DEF** | 보정이 보는 LV | 민첩 e |\n|---|---|---|---|---|---|---|")
    for e, h, g in rows:
        maps = " · ".join(f"{mi} {nm}" for mi, nm in e["maps"][:2]) + (" …" if len(e["maps"]) > 2 else "")
        cap = ""  # ★★ 2026-10-16: no HP cap any more (monster HP is an int)
        out.append(f"| {e['index']} | {e['name']} | {maps} | {h['L']}/{h['hp']}/{h['atk']}/{h['defence']} | **{g['L']}/{g['hp']}{cap}/{g['atk']}/{g['defence']}** | {seen_level(g['L'])} | {g['e']} |")
    out.append("")
    out.append("## 2. 직업 원형 (3·(LV−1) 포인트 배분: 버서커 힘 70/체력 20/의지 10 · 나이트 힘 60/체력 30/민첩 10 · 로그 힘 30/체력 10/민첩 60)\n")
    out.append("| 직업 | LV | 세트 | 힘'/체력'/민첩' | ATK | DEF | HP | 치명 % |\n|---|---|---|---|---|---|---|---|")
    players = {}
    for cls in (0, 1, 2):
        for lv, _ in BANDS:
            for set_name in SETS:
                p = player(cls, lv, items, set_name)
                players[(cls, lv, set_name)] = p
                out.append(f"| {CLASS_NAME[cls]} | {lv} | {set_name} | {p['eff'][0]:.0f}/{p['eff'][1]:.0f}/{p['eff'][2]:.0f} | {p['atk']:.0f} | {p['defence']:.0f} | {p['hp']:.0f} | {p['crit']:.0f} |")
    out.append("")
    out.append("## 3. 띠별 전투 — 원형 × 세트 × 그 띠의 적 (처치 타수 = HP ÷ 평균 피해, 명중·치명 포함 · 사망 타수 = HP ÷ 평균 피격, 회피·치명 포함)\n")
    out.append("보스(드랍 게이트 0 = 스토리 보스·이벤트 적)는 표 끝에 따로, 띠 평균에서 뺀다. 여유 = 사망까지 걸리는 초 ÷ 기본공격만으로 처치하는 초 (스킬이면 2~4 배 짧다).\n")
    summary = []
    for lv, label in BANDS:
        lo, hi = band_range(lv)
        band = [(e, h, g) for e, h, g in rows if lo <= g["L"] <= hi]
        band.sort(key=lambda r: (r[0]["gate"] == 0, r[2]["L"], r[0]["index"]))
        out.append(f"### {label} — 가디언 LV {lo}..{hi}, 적 {len(band)} 종 (보스 {sum(1 for e, _, _ in band if e['gate'] == 0)})\n")
        for cls in (0, 1, 2):
            for set_name in SETS:
                p = players[(cls, lv, set_name)]
                out.append(f"**{CLASS_NAME[cls]} LV {lv} · {set_name}** (ATK {p['atk']:.0f} · DEF {p['defence']:.0f} · HP {p['hp']:.0f})\n")
                out.append("| 적 | LV(보정) | HP | 빗나감 % | 치명 % | 기본 타수 | " + MAIN_SKILL[cls][0] + " 타수 | 처치 s | 피격 | 몹 치명 % | 회피 % | 사망까지 피격 | 사망 s | 여유 | EXP/처치 |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
                worst = []
                for e, h, g in band:
                    f = fight(p, g, not a.no_correct)
                    boss = e["gate"] == 0
                    if not boss:
                        worst.append((f["hits_to_die"], f["hits_skill"], f["margin"], e["name"]))
                    nm = f"**{e['name']} (보스)**" if boss else e["name"]
                    out.append(f"| {nm} | {f['L']}({f['Ls']}) | {f['hp']} | {f['miss']:.0f} | {f['crit']:.0f} | {f['hits_basic']:.1f} | {f['hits_skill']:.1f} | {f['kill_s']:.0f} | {f['taken']:.0f} | {f['mcrit']:.0f} | {f['evade']:.0f} | {f['hits_to_die']:.1f} | {f['die_s']:.0f} | ×{f['margin']:.1f} | {f['exp']} |")
                if worst:
                    wmin = min(worst, key=lambda w: w[2])
                    avg_die = sum(w for w, _, _, _ in worst) / len(worst)
                    avg_skill = sum(s for _, s, _, _ in worst) / len(worst)
                    avg_m = sum(m for _, _, m, _ in worst) / len(worst)
                    out.append(f"\n→ 이 띠 평균(보스 제외): 사망까지 피격 {avg_die:.1f} · 주력 스킬 처치 {avg_skill:.1f} 타 · 여유 ×{avg_m:.1f} (최소 ×{wmin[2]:.1f} {wmin[3]})\n")
                    summary.append((label, CLASS_NAME[cls], set_name, p, avg_die, avg_skill, avg_m, wmin, sum(1 for w in worst if w[2] < 1.0), len(worst)))
    out.append("## 3b. 요약 — 띠 × 직업 × 세트 (보스 제외)\n")
    out.append("| 띠 | 직업 | 세트 | ATK/DEF/HP | 사망까지 피격 | 주력 스킬 처치 타 | 여유 평균 | 여유 최소 | 여유 < 1 인 적 |\n|---|---|---|---|---|---|---|---|---|")
    for label, cn, sn, p, avg_die, avg_skill, avg_m, wmin, nbad, n in summary:
        out.append(f"| {label} | {cn} | {sn} | {p['atk']:.0f}/{p['defence']:.0f}/{p['hp']:.0f} | {avg_die:.1f} | {avg_skill:.1f} | ×{avg_m:.1f} | ×{wmin[2]:.1f} {wmin[3]} | {nbad}/{n} |")
    out.append("")
    out.append("## 4. 경험치 — 레벨당 처치 수 (그 띠의 적 평균 EXP)\n")
    out.append("| LV | 다음 레벨 EXP | 70 대 적 처치/레벨 | 80 대 | 90 대+ |\n|---|---|---|---|---|")
    for lv in (70, 75, 80, 85, 90, 95, 98):
        cells = []
        for blo, _ in BANDS[:3]:
            bhi = band_range(blo)[1]
            band = [g for _, _, g in rows if blo <= g["L"] <= bhi]
            ex = [kill_exp(g["L"] + 1, lv) for g in band]
            cells.append(f"{next_exp(lv) / (sum(ex) / len(ex)):.0f}" if ex and sum(ex) else "-")
        out.append(f"| {lv} | {next_exp(lv):,} | " + " | ".join(cells) + " |")
    out.append("")
    out.append("## 5. 아이템 — 가디언 22 vs HELL 종결\n")
    out.append("| 가디언 | LV | 수치 | 비교 종결 | 수치 | 비 |\n|---|---|---|---|---|---|")
    pairs = [((0, 33), (0, 32)), ((0, 34), (0, 32)), ((1, 28), (1, 27)), ((1, 29), (1, 27)), ((2, 28), (2, 27)), ((2, 29), (2, 27)), ((3, 33), (3, 32)), ((3, 34), (3, 32)), ((4, 33), (4, 32)), ((4, 34), (4, 32)),
             ((5, 28), (5, 27)), ((5, 29), (5, 26)), ((6, 51), (6, 49)), ((6, 52), (6, 50)), ((8, 51), (8, 49)), ((8, 52), (8, 50)), ((9, 51), (9, 49)), ((9, 52), (9, 50)), ((10, 28), (10, 27)),
             # ★ 2026-10-09: 부위·종류마다 하나씩 더 (방어구 18)
             ((5, 30), (5, 27)), ((5, 31), (5, 26)), ((6, 53), (6, 49)), ((6, 54), (6, 50)), ((8, 53), (8, 49)), ((8, 54), (8, 50)), ((9, 53), (9, 49)), ((9, 54), (9, 50)), ((10, 29), (10, 27))]
    for gk, fk in pairs:
        g, f = items[gk], items[fk]
        gv = f"{g['powerMin']}–{g['power']}" if g.get("powerMin") else f"{g['power']}"
        fv = f"{f['powerMin']}–{f['power']}" if f.get("powerMin") else f"{f['power']}"
        ratio = (g["power"] + (g.get("powerMin") or g["power"])) / (f["power"] + (f.get("powerMin") or f["power"]))
        out.append(f"| {g['name']} | {g['level']} | {gv} | {f['name']} | {fv} | ×{ratio:.2f} |")
    for key in [(11, 20), (12, 20), (12, 21)]:
        r = items[key]
        b = bytes.fromhex(r["bodyHex"])
        b = bytes([0]) + b[1 + b[0]:]  # ★ 2026-10-09: the body starts with the length-prefixed description — skip it (the offsets below assume an empty one)
        out.append(f"| {r['name']} | {r['level']} | 힘 {b[5]} 체력 {b[7]} 민첩 {b[9]} 의지 {b[11]} 지력 {b[13]} 방어 {b[17]} HP {struct.unpack_from('<I', b, 19)[0]} SP {struct.unpack_from('<I', b, 27)[0]} | — | — | — |")
    text = "\n".join(out) + "\n"
    with open(a.md, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print(f"wrote {a.md} ({len(rows)} enemies)")


if __name__ == "__main__":
    main()
