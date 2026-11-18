#!/usr/bin/env python3
"""세팅(빌드)별 밸런스 전수 조사 — 모델 v3 (2026-10-20, 사용자: *"특정 셋팅을했을때(나이트 발도셋팅, 검기셋팅, 방패치기셋팅,
로그의 일섬셋팅, 살상연희 셋팅, 지력,체력 투자 후 악의 기본공격 셋팅, 버서커 격분셋팅, 폭풍강타 셋팅, 반격태세 셋팅 등) 밸런싱을
전수조사 … 맵핑능력(일반 몬스터잡는 능력), 보스레이드 능력"*).

    python tools_re/build_balance_survey.py <hero2_ktf.zip> [--md HERO2_BALANCE_SURVEY.md] [--probe <build_probe 출력>]

바탕 = `skill_gear_balance.py`(v2 회전 모델: 모든 액티브 · 버프 · 패시브 · 각성기 · 야수 · SP 살림 · probe 로 맞춘 계수)와
`guardian_balance.py`(적 · 템플릿 · 명중/치명/회피) · `demon_balance.py`(악의) · `raid_balance.py`(레이드 보스 · 바퀴 · 보정).
v3 가 더한 것:
  · 세팅 = 실제 아이템 (type, id) 일곱 칸 — 효과는 코드 표 `guest_anchors.rs` `GearPatch::ITEMS` 를 **직접** 읽는다(With 중첩 포함,
    2차 39 + 발도 6 + 검기/방패치기 K 9 + 악의 무기 5 + 1차 31). 모델 안의 아이템 표(NEW)는 더 이상 안 쓴다.
  · 나이트 공격 스킬 = 공격력 + 지력' × 179/512 (★ 10-17) · 발도 범위 아이템(반경 4 · 5 = 무리 더 맞음) · 쌍월 = SP 두 번 ·
    벽력일섬 세트 = 일섬 ×2.5(번개 1.5 배) · 극사 = 급소가격 ×3 · 둔기강타 범위 +50 % · 마지막 춤 3타 ×5 · 무모한 칼춤 처치 연쇄 ·
    악마의 손톱 기본공격 속도 · 도발자의 투구(반격태세 유지) · 중력 1 %/장비 · 시간도둑 · 흐름의 장화.
  · 가디언 HARD = 설정 200 % × 1.5 = **300 %**(★ 10-19 (31)) · 레이드 = `raid_balance` 의 보스 · 바퀴 · ATK 18 % + ★ `H2_RAID_HP_MUL`
    (트리의 값 — 10 이면 보스 HP ×10; 기록이 없어 그대로 반영하고 보고서가 표시한다).
  · 레이드 결투도 v2 회전(boss=True)으로 — 받는 피해는 `raid_calib.json` 의 단계별 박자 배율.
  · 캐릭터 = LV 99 · 스킬 전부 Lv5(각성기 Lv5) · 패시브 최대 · 야수 레이든 계열 Lv5(야수 LV = 99) · 제련 0 · 랜덤 옵션 기대값 · 인장 없음.
    「풀」 = 제련 최대 + 인장(장신구 밖 장비마다 두 능력치 +20).
모델 밖(v2 와 같다): 보스 특수기 · 이동 · 포션 · 부활 · 방패 막기(방어확률 옵션은 「방패 방어만큼 깎는 막기」로 근사) · 실제 몰림 ·
악의 무기 다섯의 할퀴기 변형(사방 · 맹독 · 범위) · 크루세이더/바바리안 채널.
"""

import argparse
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import demon_balance as DB  # noqa: E402
import guardian_balance as GB  # noqa: E402
import raid_balance as RB  # noqa: E402
import skill_gear_balance as SG  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEVEL = 99
BEAST_LV_ASSUMED = 99  # 야수 레벨 = 플레이어 레벨 (v2 가정 그대로)
HARD_PCT = 300  # ★ 2026-10-19 (31) hero2java::H2_GUARDIAN_HARD_PCT 150 × 설정 200 %
RAID_EVADE_RETAIN = int(re.search(r"RAID_EVADE_RETAIN_PERCENT: u8 = (\d+)", open(os.path.join(ROOT, "wie_android", "src", "guest_anchors.rs"), encoding="utf-8").read()).group(1)) / 100
GEUKSA_MUL = 4.3  # ★ probe 10-20: 급소가격 523 → 극사 2,244 (설계 ×3 — 실측대로)
KNIGHT_TREE_MUL = 1.2  # ★ probe 10-20: 현재 트리의 나이트 스킬(검기 · 방패치기 · 둔기강타 · 발도 · 신의가호)이 10-18 exe 대비 ×1.20 — 출처 미확인, 실측 보정
SEAL = 20
ACC_CODE = {(12, 17): 1, (11, 17): 2, (11, 18): 3, (12, 18): 4, (12, 19): 5, (11, 19): 6}
KIND_FIRST = {0: 4, 1: 3, 2: 5}  # 첫 직업 스킬의 a.p 인덱스 (KindCd 의 idx = kind − 이것)
BAG_ITEMS = SG.BAG_ITEMS

# ── GearPatch::ITEMS 파서 (With 중첩 · Range(상수) 포함) ──────────────────────────────────────────────────────────────


def parse_gear(path):
    text = open(path, encoding="utf-8").read()
    i = text.find("pub const ITEMS: [GearItem;")
    body = text[i:]
    out = {}
    for m in re.finditer(r'GearItem \{ item_type: (\d+), id: (\d+), text: "[^"]*", effects: &\[(.*?)\] \},?\s*(?://|\n)', body):
        effs = []
        s = m.group(3)
        for w in re.finditer(r"With\((\d+), (\d+), &\[(.*?)\]\)", s):
            inner = [(n, tuple(a.strip() for a in args.split(",") if a.strip())) for n, args in re.findall(r"(\w+)\(([^()]*)\)", w.group(3))]
            effs.append(("With", ((int(w.group(1)), int(w.group(2))), inner)))
        s = re.sub(r"With\(\d+, \d+, &\[.*?\]\)", "", s)
        effs += [(n, tuple(a.strip() for a in args.split(",") if a.strip())) for n, args in re.findall(r"(\w+)\(([^()]*)\)", s)]
        effs += [(n, ()) for n in re.findall(r"(?<![\w(])(StanceHold|ClawSpin|ClawWideBlessed|ClawVenom|DemonSlam)(?![\w(])", s)]
        out[(int(m.group(1)), int(m.group(2)))] = effs
    return out


def ival(a):
    return int(a) if re.fullmatch(r"-?\d+", a) else None


def effects(gear, keys, cls):
    """착용 (type,id) 들 → (GB opts, SG ne, 능력치 보너스 5, 기타)."""
    o = dict(GB.NO_OPTS)
    o["dmg"] = {}
    ne = dict(dmg={}, targets={}, cd_kind={}, sp_kind={})
    bonus = [0.0] * 5
    x = dict(guard=0, basic_spd=1.0, iaido_reach=0, blunt_reach=0.0, demon=dict(), acc=0)
    L = KIND_FIRST[cls]

    def add(name, a):
        v = [ival(t) for t in a]
        if name == "Taken":
            o["taken"] *= v[0] / 256
        elif name == "Reflect":
            o["reflect"] += v[0] / 256
        elif name == "SpCost":
            o["sp"] *= v[0] / 256
        elif name == "Crit":
            o["crit"] += v[0]
        elif name == "Evade":
            o["evade"] += v[0]
        elif name == "Venom":
            o["venom"] += v[0] / 256
        elif name == "HealHp":
            o["heal_hp"] += v[0] / 256
        elif name == "HealSp":
            o["heal_sp"] += v[0] / 256
        elif name == "Dmg":
            if v[0] == cls:
                for k in range(v[1], v[2] + 1):
                    o["dmg"][k] = o["dmg"].get(k, 0) + v[3] / 256
        elif name == "Basic":
            for k in range(GB.STAGES[cls]):
                o["dmg"][k] = o["dmg"].get(k, 0) + v[0] / 256
        elif name == "Lifesteal":
            o["lifesteal"] += v[0] / 256
        elif name == "BeastCd":
            o["beast_cd"] *= v[0] / 256
        elif name == "MaxHp":
            o["max_hp"] += v[0] / 256
        elif name == "MaxSp":
            o["max_sp"] += v[0] / 256
        elif name == "Ability":
            bonus[v[0]] += v[1]
        elif name == "Skills":
            ne["skill_dmg"] = ne.get("skill_dmg", 0) + v[0] / 256
        elif name == "AllCd":
            ne["cd_mul"] = ne.get("cd_mul", 1.0) * (1 - v[0] / 256)
        elif name == "KindCd":
            if v[0] == cls:
                k = v[1] + L
                ne["cd_kind"][k] = ne["cd_kind"].get(k, 1.0) * (1 - v[2] / 256)
        elif name == "KindSp":
            if v[0] == cls:
                ne["sp_kind"][v[1]] = ne["sp_kind"].get(v[1], 1.0) * (1 - v[2] / 256)
        elif name == "Counter":
            ne["counter_mul"] = ne.get("counter_mul", 0) + v[0] / 256
        elif name == "CounterLeech":
            ne["counter_heal"] = ne.get("counter_heal", 0) + v[0] / 256
        elif name == "CounterSplash":
            ne["counter_aoe"] = ne.get("counter_aoe", 0) + v[0] / 256
        elif name == "StanceTaken":
            ne["counter_taken_mul"] = ne.get("counter_taken_mul", 1.0) * v[0] / 256
        elif name == "TauntTaken":
            ne["taunt_taken_mul"] = ne.get("taunt_taken_mul", 1.0) * v[0] / 256
        elif name == "StanceHold":
            ne["counter_forever"] = 1
        elif name == "FuryStart":
            ne["fury_start"] = v[0] / 100
        elif name == "FuryDmg":
            ne["fury_dmg"] = ne.get("fury_dmg", 0) + v[0] / 256
        elif name == "FuryHit":
            ne["fury_hit"] = ne.get("fury_hit", 0) + v[0] / 20
        elif name == "FuryKill":
            ne["fury_kill"] = ne.get("fury_kill", 0) + v[0] / 20
        elif name == "FuryHurt":
            ne["fury_taken"] = ne.get("fury_taken", 0) + v[0] / 20
        elif name == "FuryTaken":
            ne["fury_taken_mul"] = ne.get("fury_taken_mul", 1.0) * v[0] / 256
        elif name == "FuryKeep":
            ne["fury_keep"] = 1
            ne["fury_keep_gauge"] = v[0] / 100
        elif name == "FuryCd":
            ne["fury_blast_cd"] = v[0] / 20
        elif name == "TimeThief":
            ne["cd_basic"] = (v[0], 1.0)
        elif name == "ComboSp":
            ne["sp_gain"] = ne.get("sp_gain", 0) + v[0] / 256
        elif name == "BasicFast":
            x["basic_spd"] *= 1 + v[0] / 100
        elif name == "Gravity":
            ne["gravity"] = ne.get("gravity", 0) + v[0] / 1000
        elif name == "IaidoReach":
            x["iaido_reach"] = min(2, x["iaido_reach"] + v[0])
        elif name == "BluntReach":
            x["blunt_reach"] += v[0] / 100
        elif name == "Wind":
            ne["wave_passive"] = 0.30
        elif name == "Range":
            t = a[0]
            if "LOBE" in t:
                ne["targets"][4] = ne["targets"].get(4, 0) + 1
            elif "DAGGER3" in t:
                ne["targets"][15] = ne["targets"].get(15, 0) + 1
            elif "WHIRL" in t:
                ne["ilpung_chain"] = 3
            elif "IAIDO_TWICE" in t:
                ne["iaido_double"] = 1
        elif name == "Guard":
            x["guard"] += v[0] / 100
        elif name in ("DemonDur", "DemonTaken", "DemonKill", "DemonKillHeal", "ClawBank", "FuryMaliceCd", "ToggleSp", "ClawPoisoned"):
            x["demon"][name] = v[0]
        elif name in ("ClawSpin", "ClawWideBlessed", "ClawVenom", "DemonSlam"):
            x["demon"][name] = 1
        # Walk · Drunk · DragonProc · ResetProc: 모델 밖

    for key in keys:
        if key == (11, 18):
            o["beast_x2"] = True
        if key in ACC_CODE:
            x["acc"] = ACC_CODE[key]
        for name, a in gear.get(key, []):
            if name == "With":
                cond, inner = a
                if cond in keys:
                    for n2, a2 in inner:
                        add(n2, a2)
            else:
                add(name, a)
    # 로그 살상연희 무기의 타수 (RoguePatch 의 gear 워드 — 코드 표 밖): 백 번째 칼날 +5 · 무모한 칼춤 +6 · 처치당 +2(최대 8)
    # ★ 2026-10-20 정정: 두 단검의 +타수는 **합산**이다(RoguePatch::gear_words 가 부위 합산) — probe 실게임 `[h2-slaughter]` 가
    #   한 마리 19 타(8 + 5 + 6) · 무리 27 타(+ 처치 2 × 4)를 센다. 옛 「큰 쪽 하나(14 타)」는 한 단검만 입힌 A/B(R_sl_D = 3.35 뿐)에서 온 오독이었다.
    for key in keys:
        if key == (3, 35):
            ne["sl_hits"] = ne.get("sl_hits", 0) + 5
        if key == (3, 36):
            ne["sl_hits"] = ne.get("sl_hits", 0) + 6
            ne["sl_chain"], ne["sl_chain_per"] = 8, 2
        if key == (12, 24):  # 마지막 춤: 3타 고정 · 타마다 ×5
            ne["sl_cap"], ne["sl_mul"] = 3, 5.0
    x["bolt_set"] = (12, 19) in keys and (3, 39) in keys
    return o, ne, bonus, x


# ── 플레이어 (GB.player + 능력치 보너스 · 인장 · SG 필드) ──────────────────────────────────────────────────────────────


def make_player(items, gear, cls, slots, archetype=None, refine=None, seal=None, off=(), level=LEVEL):
    keys = list(slots.values())
    o, ne, bonus_add, x = effects(gear, keys, cls)
    base = GB.BASE_STATS[cls]
    pts = 3 * (level - 1)
    arch = archetype or GB.ARCHETYPE[cls]
    stats = [base[k] + int(pts * arch[k]) for k in range(5)]
    watk, gdef, bonus, hp_flat, sp_flat = GB.gear_stats(items, cls, slots, refine)
    if seal:
        n_seal = sum(1 for s in slots if s != "acc")
        for k in seal:
            bonus[k] += SEAL * n_seal
    eff = [stats[k] + bonus[k] + bonus_add[k] for k in range(5)]
    cs, ca = GB.CALIB["str_term"][cls]
    atk = eff[0] * cs + eff[2] * ca + watk
    skill_atk = atk + (max(eff[4], 0) * GB.KNIGHT_INT_MUL / 512 if cls == 1 else 0)
    ds, da = GB.CALIB["def_str"][cls]
    defence = gdef + eff[0] * ds + eff[2] * da
    hp = ((eff[1] + level) * GB.CALIB["hp_mul"][cls] + hp_flat) * (1 + o["max_hp"])
    sp = ((eff[3] + level) * 6 + sp_flat) * (1 + o["max_sp"])
    cb, cm = GB.CALIB["crit_base"][cls]
    crit = cb + eff[2] * cm + o["crit"]
    ak = 10 - level - eff[2] / 10 - eff[4] / 10
    am = level + 10 + eff[2] / 3 + GB.CALIB["am_extra"][cls] + o["evade"]
    p = dict(cls=cls, level=level, stats=stats, eff=eff, atk=atk, skill_atk=skill_atk, defence=defence, hp=hp, sp=sp, crit=GB.clamp(crit, 8, 90), ak=ak, am=am, opts=o)
    wk = slots["w"]
    p["wmax"] = items[wk]["power"] * GB.WEAPON_BONUS[wk[0]]
    p["wmax2"] = sum(items[slots[s]]["power"] * GB.WEAPON_BONUS[slots[s][0]] for s in ("w", "w2") if s in slots)
    p["aj"] = p["wmax2"] * SG.ROGUE_AJ if cls == 2 else p["wmax"] / 5
    p["kept"] = keys
    p["wtype"] = wk[0]
    p["beast_x2"] = (11, 18) in keys
    p["off"] = set(off)
    p["x"] = x
    p["shield_def"] = items[slots["sh"]]["power"] if "sh" in slots else 0
    p["slots"] = slots
    return p, ne


# ── v2 키트 위에 v3 보정 (monkeypatch — rotation · fight 는 SG 의 것을 그대로 쓴다) ──────────────────────────────────

_kit2, _incoming2, _rotation2 = SG.kit, SG.incoming, SG.rotation


def kit3(p, m, ne, mul=1.0, spd=1.0, boss=False, pack=False):
    x = p.get("x", {})
    q = p
    if p["cls"] == 1 and abs(p.get("skill_atk", p["atk"]) - p["atk"]) > 1e-6:
        q = dict(p, atk=p["skill_atk"])
    k = _kit2(q, m, ne, mul, spd, boss, pack)
    if q is not p:  # 기본공격은 지력 몫 없이
        _, _, rel = SG.hitcrit(p, m)
        grav = 1 + SG.g(ne, "gravity") * BAG_ITEMS
        d = max(p["atk"] - m["defence"], 0)
        k["basic"] = d * SG.BASIC[1]["mul"] * (1 + p["opts"]["dmg"].get(0, 0) + ne["dmg"].get(0, 0)) * grav * rel
    alive = 1 if boss else SG.ALIVE
    for s in k["sk"]:
        n = s["name"]
        if p["cls"] == 1 and not s.get("beast") and n != "천공의검":
            s["one"] *= KNIGHT_TREE_MUL
            s["pack"] *= KNIGHT_TREE_MUL
        if n.startswith("발도"):
            R = x.get("iaido_reach", 0)
            if R and pack and not boss:
                s["pack"] = s["one"] * min(SG.N_PACK, alive * (1 + 0.6 * R))
            if ne.get("iaido_double"):
                s["sp"] *= 2  # ★ 10-17 쌍월: 0.5 초 안 재사용 — SP 는 두 번
        elif n == "일격" and x.get("acc") == 5:  # 일섬 — 5칸 돌진, 세트면 번개 ×1.5 더
            f = 2.5 if x.get("bolt_set") else 1.0
            s["one"] *= f
            s["pack"] = s["one"] * (min(2.0, alive) if pack and not boss else 1.0)
        elif n == "급소가격" and x.get("acc") == 6:  # 극사 — 최근접 적 뒤로 ×3 (★ probe 10-20 실측 ×4.3 — 뒤에서 치는 보너스까지)
            s["one"] *= GEUKSA_MUL
            s["pack"] *= GEUKSA_MUL
        elif n == "둔기강타" and x.get("blunt_reach"):
            if pack and not boss:
                s["pack"] = s["one"] * min(SG.N_PACK, alive * (1 + x["blunt_reach"]))
        elif n == "검기" and ne["targets"].get(4) and pack and not boss:
            s["pack"] = s["one"] * min(3.0 + ne["targets"][4], alive + ne["targets"][4])
    if ne.get("counter_forever"):
        k["up"] = [u for u in k["up"] if abs(u[1] - 75.0) > 1e-6]
    return k


def incoming3(p, m, taken_mul):
    inc, hits, one = _incoming2(p, m, taken_mul)
    g = p.get("x", {}).get("guard", 0)
    if g and p.get("shield_def"):
        inc *= 1 - g * min(1.0, p["shield_def"] / max(one, 1.0))
    r = m.get("rate_mul", 1.0)
    if m.get("evade_retain"):
        ev = GB.clamp(p["am"] - m["L"] - m["e"] / 10, 8, 90) / 100
        r *= (1 - ev * m["evade_retain"]) / (1 - ev)
    return inc * r, hits * r, one


def rotation3(p, m, ne, pack, mul=1.0, spd=1.0, boss=False):
    f = p.get("x", {}).get("basic_spd", 1.0)
    return _rotation2(p, m, ne, pack, mul, spd * f, boss) if f != 1.0 else _rotation2(p, m, ne, pack, mul, spd, boss)


SG.kit, SG.incoming, SG.rotation = kit3, incoming3, rotation3


# ── 세팅 표 ───────────────────────────────────────────────────────────────────────────────────────────────────────────
K = GB.LOADOUTS["나이트"][2]
R = GB.LOADOUTS["로그"][2]
B = GB.LOADOUTS["버서커"][2]
BEAST_ARCH = (0.0, 5 / 6, 0.0, 0.0, 1 / 6)  # ★ 사용자 10-20: 체력 위주 · 2 레벨당 지력 1
BEAST_ARCH_111 = (0.0, 1 / 3, 0.0, 1 / 3, 1 / 3)  # 체1 의1 지1 (10-13 벤치)


def s(base, **ch):
    d = dict(base)
    d.update(ch)
    return d


# (이름, 직업, 칸, 능력치 배분(None = 원형), 악의 세팅인가)
SETTINGS = [
    # 나이트
    ("나이트 · 1차 가디언 (이안의 검 · 성채)", 1, K, None, False),
    ("나이트 · 발도 범위·피해 (회천도 · 원월 · 검귀 · 검귀 흉갑 · 검귀 철화)", 1, s(K, w=(2, 33), g=(8, 60), h=(5, 37), a=(6, 63), s=(9, 64)), None, False),
    ("나이트 · 발도 SP·쿨 (찰나의 검 · 납도 장갑 · 납도 갑옷 · 검귀 투구 · 철화)", 1, s(K, w=(2, 34), g=(8, 61), a=(6, 61), h=(5, 37), s=(9, 64)), None, False),
    ("나이트 · 발도 쌍월 (쌍월 · 원월 · 검귀 · 흉갑 · 철화)", 1, s(K, w=(2, 30), g=(8, 60), h=(5, 37), a=(6, 63), s=(9, 64)), None, False),
    ("나이트 · 검기 (이안의 검 · 소드마스터 장갑 · 검성 투구 · 흉갑 · 철화)", 1, s(K, g=(8, 59), h=(5, 38), a=(6, 62), s=(9, 63)), None, False),
    ("나이트 · 검풍 (검풍 + 검기 넷)", 1, s(K, w=(2, 31), g=(8, 59), h=(5, 38), a=(6, 62), s=(9, 63)), None, False),
    ("나이트 · 방패치기 (수호검 · 격돌 · 성벽 투구 · 흉갑 · 철화 · 건틀릿)", 1, s(K, w=(2, 29), sh=(10, 29), h=(5, 39), a=(6, 64), s=(9, 65), g=(8, 62)), None, False),
    ("나이트 · 둔기강타 (곡괭이 · 파쇄자 손갑 · 스프링 부츠)", 1, s(K, w=(4, 35), g=(8, 53), s=(9, 62)), None, False),
    ("나이트 · 쿨감 B (시간도둑 · 흐름 · 집중의 관 · 시간의 외투 · 연환)", 1, s(K, acc=(12, 23), s=(9, 57), h=(5, 34), a=(6, 57), g=(8, 56)), None, False),
    # 야수나이트 = 체1의1지1 나이트 + 야수조련 목걸이. 데몬 계열(악의)이 본 세팅 — 악의 안 = 할퀴기 · 지옥칼날 · 용암불 · 멸화, 밖 = 나이트 스킬(지력 계수) + 용암불 · 멸화
    ("야수나이트 · 체5/6 지1/6 · 데몬 계열 (악의 + 토글 + 용암불 + 나이트 스킬 · 야수조련 목걸이)", 1, GB.LOADOUTS["야수나이트"][2], BEAST_ARCH, True),
    ("야수나이트 · 체5/6 지1/6 · 데몬 + 기본공격 세팅 (손톱 · 비늘 · 뿔 · 발굽 — 야수조련 유지)", 1, s(GB.LOADOUTS["야수나이트"][2], g=(8, 57), a=(6, 58), h=(5, 35), s=(9, 58)), BEAST_ARCH, True),
    ("야수나이트 · 체1의1지1 · 데몬 + 기본공격 세팅", 1, s(GB.LOADOUTS["야수나이트"][2], g=(8, 57), a=(6, 58), h=(5, 35), s=(9, 58)), BEAST_ARCH_111, True),
    ("야수나이트 · 체5/6 지1/6 · 레이든 계열 (지옥불 · 암살자 · 학살화염 — 악의 없음, 비교용)", 1, GB.LOADOUTS["야수나이트"][2], BEAST_ARCH, False),
    # 로그
    ("로그 · 1차 가디언 (응보 · 터줏대감)", 2, R, None, False),
    ("로그 · 일섬 (벽력일섬 · 번개의 호흡 · 응보 · 일격 +100 % 신발)", 2, s(R, acc=(12, 19), w=(3, 39), w2=(3, 34), s=(9, 52)), None, False),
    ("로그 · 극사 (극사의 목걸이 · 피의 연쇄)", 2, s(R, acc=(11, 19), w2=(3, 37)), None, False),
    ("로그 · 살상연희 (백 번째 칼날 · 무모한 칼춤 · 무희 · 칼춤 장갑 · 잔영)", 2, s(R, w=(3, 35), w2=(3, 36), h=(5, 36), a=(6, 59), g=(8, 58), s=(9, 59)), None, False),
    ("로그 · 살상연희 + 마지막 춤 (3타 ×5)", 2, s(R, w=(3, 35), w2=(3, 36), h=(5, 36), a=(6, 59), g=(8, 58), s=(9, 59), acc=(12, 24)), None, False),
    ("로그 · 일풍격 (회오리 송곳니 · 바람날개 신발)", 2, s(R, w=(3, 38), s=(9, 60)), None, False),
    ("로그 · 쿨감 B", 2, s(R, acc=(12, 23), s=(9, 57), h=(5, 34), a=(6, 57), g=(8, 56)), None, False),
    # 버서커
    ("버서커 · 1차 가디언 평타 (오딘 · 독사 · 비호)", 0, B, None, False),
    ("버서커 · 격분 A (멈추지 않는 대검 · 분노의 투구 · 상처의 갑주 · 발걸음 · 불씨)", 0, s(B, w=(0, 35), h=(5, 32), a=(6, 55), s=(9, 55), acc=(12, 22)), None, False),
    ("버서커 · 폭풍강타 (제스의 수련도끼 · 파쇄자 손갑)", 0, s(B, w=(1, 29), g=(8, 53)), None, False),
    ("버서커 · 폭풍강타 + 쿨감 B 넷", 0, s(B, w=(1, 29), g=(8, 53), acc=(12, 23), s=(9, 57), h=(5, 34), a=(6, 57)), None, False),
    ("버서커 · 반격태세 G (응징의 대검 · 도발자 · 가시 판금 · 되갚는 손 · 울림 · 서약)", 0, s(B, w=(0, 36), h=(5, 33), a=(6, 56), g=(8, 55), s=(9, 56), acc=(11, 21)), None, False),
    ("버서커 · 쿨감 B", 0, s(B, acc=(12, 23), s=(9, 57), h=(5, 34), a=(6, 57), g=(8, 56)), None, False),
    ("악의 버서커 · 올힘 (가디언 한 벌 · 광마의 대검 — 참고: 악의는 지력 빌드용)", 0, s(B, w=(0, 37)), (1.0, 0, 0, 0, 0), True),
]
BANDS = [("80 대 NORMAL", 80, 1.5), ("90 대 NORMAL", 90, 1.5), ("90 대 HARD(300 %)", 90, HARD_PCT / 100)]
RAID_DIFFS = RB.DIFFS
SEAL_PAIR = {0: (0, 1), 1: (0, 1), 2: (0, 2)}


# ── 악의 결투 (demon_balance 위에 C · 무리 · 레이드) ───────────────────────────────────────────────────────────────────


def demon_cfg(p):
    cfg = DB.recommended()
    x = p["x"]
    cfg["combo_frames"] = cfg["combo_frames"] / x.get("basic_spd", 1.0)
    cfg["duration_s"] = 60.0 + x["demon"].get("DemonDur", 0)
    cfg["taken"] = cfg["taken"] * (x["demon"]["DemonTaken"] / 256 if "DemonTaken" in x["demon"] else 1.0)
    return cfg


CLAW_CAL = 0.92  # ★ probe 10-20 (V_claw): 할퀴기만 816 DPS vs 블록 식 883 — OK 연타의 실제 박자
FOLLOW_CLAW_SPD = 1.2  # 추종 100 %: 할퀴기 5 프레임마다 하나 더
BLADE_SP, JUDGE_SP, BLADE_SP_MIN = 0.12, 0.08, 15.0  # 토글 SP = 최대 SP 의 %/초 (DemonPatch BLADE_SP_PCT 12 · JUDGE 8 · 최소 15/s)
DEMON_CD_S = 30.0


def toggle_fractions(p, want_blade, want_judge):
    """악의 60 s 동안 토글이 켜져 있는 몫 — SP 풀(최대 SP, 쉬는 30 s 에 자연 회복으로 찬다)을 어둠심판 → 지옥칼날 순으로 쓴다."""
    maxsp, regen = p["sp"], SG.SP_REGEN_PER_WIS * p["eff"][3]
    pool = min(maxsp, maxsp * 0.5 + regen * DEMON_CD_S)
    dur = 60.0
    fj = fb = 0.0
    if want_judge:
        drain = max(JUDGE_SP * maxsp, 1.0) - regen
        on = dur if drain <= 0 else min(dur, pool / drain)
        fj = on / dur
        pool = max(pool - max(drain, 0) * on, 0.0) if drain > 0 else pool
    if want_blade:
        drain = max(BLADE_SP * maxsp, BLADE_SP_MIN) + (JUDGE_SP * maxsp if want_judge and fj >= 1 else 0) - regen
        on = dur if drain <= 0 else min(dur, pool / drain)
        fb = on / dur
    return fb, fj


def hybrid(p, ne, g, m, pack, boss=False):
    """야수나이트(데몬 계열): 악의 가동률 u = 60 ÷ 90 안에서는 할퀴기 콤보 + 토글(지옥칼날 · 어둠심판 — SP 예산만큼) + 용암불(+ 멸화) + 추종(할퀴기 속도 ×1.2 ·
    흡혈 5 % · 끝의 폭발 3H), 밖에서는 그 직업의 스킬 회전(v2, 야수 없이) + 용암불 · 멸화. 야수 슬롯은 {추종|지옥칼날} × {어둠심판|멸화} 네 조합 중 단일 DPS 가 가장 큰 것.
    받는 피해는 악의 안 ×0.85(× 풀린 목줄) · 흡혈 = 장비 흡혈 × 할퀴기 (+ 추종 5 %). g = 템플릿(L·e·defence) · m = SG mob."""
    cfg = demon_cfg(p)
    u = min(cfg["duration_s"] / 90.0, 1.0)
    claw0, _ = DB.claw_combo_dps(p, g, cfg)
    claw0 *= CLAW_CAL
    t = DB.beast_tick_dps(p, g, cfg)
    x2 = 2 if p["opts"]["beast_x2"] else 1
    alive = 1.0 if boss or not pack else SG.ALIVE
    p_off = dict(p, off=set(p["off"]) | {"야수"})
    r = SG.fight(p_off, ne, m, pack, boss)
    best = None
    for follow in (False, True):
        for judge in (False, True):
            fb, fj = toggle_fractions(p, not follow, judge)
            claw = claw0 * (FOLLOW_CLAW_SPD if follow else 1.0)
            inside = claw * min(alive, 3.0) + (0.0 if follow else t["blade"] * fb * alive) + t["judge"] * fj * alive
            lava = t["lava"] / p["opts"]["beast_cd"]
            fire = (0.0 if judge else t["fire"] / p["opts"]["beast_cd"])
            beast_cd = lava + fire
            blast = (3 * t["H"] * x2 * min(alive, SG.N_PACK) / 90.0) if follow else 0.0  # 추종 끝의 어둠 폭발 — 60 s 에 한 번, 맵 전체
            inside += beast_cd * alive + blast / u
            outside = r["dps"] + beast_cd * alive
            inc_out, heal_out = r["inc"], r["heal"]
            inc_in = inc_out * cfg["taken"]
            heal_in = claw * min(alive, 3.0) * (p["opts"]["lifesteal"] + (0.05 if follow else 0.0))
            dps = u * inside + (1 - u) * outside
            inc = u * inc_in + (1 - u) * inc_out
            heal = u * heal_in + (1 - u) * heal_out
            count = 1 if boss or not pack else SG.N_PACK
            kill_s = count * m["hp"] / dps if dps > 0 else float("inf")
            net = inc - heal
            die_s = p["hp"] / net if net > 0 else float("inf")
            parts = {"악의 할퀴기": u * claw * min(alive, 3.0), "지옥칼날": 0.0 if follow else u * t["blade"] * fb * alive, "어둠심판": u * t["judge"] * fj * alive,
                     "용암불 · 멸화": beast_cd * alive, "추종 폭발": blast}
            for k, v in r["parts"].items():
                parts[k + " (악의 밖)"] = (1 - u) * v
            cand = dict(name="", dps=dps, kill_s=kill_s, die_s=die_s, margin=die_s / kill_s if kill_s else float("inf"), parts=parts, inc=inc, heal=heal, uptime=u,
                        slots=("추종" if follow else "지옥칼날") + " + " + ("어둠심판" if judge else "멸화") + f" (토글 {fb:.0%}/{fj:.0%})")
            if best is None or cand["dps"] > best["dps"]:
                best = cand
    return best


def demon_eval(p, ne, rows, diff, pack, boss=False):
    out = []
    for e, h, gt in rows:
        x = hybrid(p, ne, gt, SG.mob(gt, diff), pack, boss)
        x["name"] = e["name"]
        x["parts"]["〔" + x["slots"] + "〕"] = 0.0
        out.append(x)
    cap = lambda v: min(v, 9999)
    nn = len(out)
    parts = {}
    for x in out:
        for k, v in x["parts"].items():
            parts[k] = parts.get(k, 0.0) + v / nn
    return dict(kill=sum(cap(x["kill_s"]) for x in out) / nn, margin=sum(cap(x["margin"]) for x in out) / nn, worst=min(out, key=lambda x: x["margin"]),
                dps=sum(x["dps"] for x in out) / nn, parts=parts, rows=out, lose=sum(1 for x in out if x["margin"] < 1))


def demon_raid(p, ne, st, hp_pct, atk_pct, calib):
    b = st["mons"][0]
    g = dict(L=RB.RAID_LEVEL - 1, hp=b["hp"], atk=b["atk"], e=b["e"], defence=b["defence"])
    return hybrid(p, ne, g, raid_mob(st, hp_pct, atk_pct, calib), False, boss=True)


# ── 평가 ──────────────────────────────────────────────────────────────────────────────────────────────────────────────


def evaluate(p, ne, rows, diff, pack, boss=False, demon=False):
    if demon:
        return demon_eval(p, ne, rows, diff, pack, boss)
    r = SG.evaluate(p, ne, rows, diff, pack, boss)
    r["lose"] = sum(1 for x in r["rows"] if x["margin"] < 1)
    return r


def raid_mob(st, hp_pct, atk_pct, calib):
    b = st["mons"][0]
    return dict(L=RB.RAID_LEVEL, Ls=RB.SEEN, hp=b["hp"] * hp_pct / 100, atk=min(b["atk"] * atk_pct / 100, 32767), defence=b["defence"], e=b["e"],
                rate_mul=calib.get(str(st["stage"]), 1.0), evade_retain=RAID_EVADE_RETAIN)


def raid_want(d_pct, lap):
    hp, atk = RB.raid_want(d_pct, lap)
    return hp * (1 if hasattr(RB, "RAID_HP_MUL") else RAID_HP_MUL), atk  # raid_balance 가 ×10 을 알면 거기서 한 번만


def raid_stage(p, ne, st, d_pct, lap, calib, demon=False):
    hp_pct, atk_pct = raid_want(d_pct, lap)
    if demon:
        return demon_raid(p, ne, st, hp_pct, atk_pct, calib)
    r = SG.fight(p, ne, raid_mob(st, hp_pct, atk_pct, calib), False, boss=True)
    return r


STAGE_CAP_S = 300.0  # 「현실적으로 깬다」 = 단계마다 5 분 안 (여유 ≥ 1 이어도 처치가 수십 분이면 안 깬 것으로)


def raid_last_lap(p, ne, bosses, d_pct, calib, demon=False, cap=None):
    last = 0
    for lap in range(1, RB.LAP_STEPS + 2):
        rs = [raid_stage(p, ne, st, d_pct, lap, calib, demon) for st in bosses]
        if all(r["margin"] >= 1 and (cap is None or r["kill_s"] <= cap) for r in rs):
            last = lap
        else:
            break
    return last


def f(v):
    return SG.f(v)


def judge(m):
    return SG.judge(m)


def parts_text(parts, top=5):
    return SG.parts_text(parts, top)


def read_hero2java_const(name, default):
    try:
        t = open(os.path.join(ROOT, "wie_android", "src", "hero2java.rs"), encoding="utf-8").read()
        m = re.search(rf"pub const {name}: u32 = (\d+);", t)
        return int(m.group(1)) if m else default
    except OSError:
        return default


RAID_HP_MUL = read_hero2java_const("H2_RAID_HP_MUL", 1)
GUARD_HARD = read_hero2java_const("H2_GUARDIAN_HARD_PCT", 100)


# ── 같은 벤치의 직업 회전 실측 (class_rotation_probe, 2026-10-20 트리 probe · 실제 의지 · 20 초) ─────────────────────────────
ROTATION_BENCH = [
    ("버서커 회전 (휘두르기 · 폭풍강타 · 사자후 + 분노 · 반격 · 도발 + OK)", "1,454", "SP 780"),
    ("버서커 회전 + 격분", "2,634", "20 초 창 안에서 게이지 25 타 뒤 ×2"),
    ("나이트 회전 (검기 · 방패치기 · 발도 + 신의가호 · 철괴 + OK)", "1,536", "SP 612 · 검기는 SP 로 막힘"),
    ("나이트 천공의검 (한 번)", "1,322", "각성기 하나 + OK"),
    ("로그 회전 (일격 · 급소 · 단검난무 · 살상연희 + 맹독감염 + OK)", "2,902", "SP 600"),
    ("야수(레이든) 지옥불 · 학살화염 + OK (나이트 벤치)", "851", "야수 LV 47"),
    ("체력 나이트 악의 · 지옥칼날 · 어둠심판 · 용암불 + 손톱 · 비늘 · 야수조련 + OK (의지 12)", "2,599", "토글이 4 초 뒤 SP 로 꺼짐 · 13 초 창"),
    ("체력 나이트 악의 · 추종 · 어둠심판 · 용암불 + 손톱 · 비늘 · 야수조련 + OK (의지 12)", "3,113", "어둠심판은 8 %/s 라 버팀 · 13 초 창"),
]


# ── probe 검증 표 ──────────────────────────────────────────────────────────────────────────────────────────────────────

PROBE_ROWS = [
    # (라벨, 사례 A, 사례 B, 기대 배율 설명, 기대값(None = 표만))
    ("발도 범위·피해 셋 (회천도 +30 · 원월 +30 · 검귀 +50)", "K_iaido_0", "K_iaido_J123", "×2.10", 2.10),
    ("발도 피해 다섯 (+ 검귀 흉갑 70 · 철화 40)", "K_iaido_0", "K_iaido_dmg5", "×3.20", 3.20),
    ("발도 SP·쿨 셋 (찰나 · 납도 둘)", "K_iaido_0", "K_iaido_J456", "쿨 140 → 63 · SP ×0.45", None),
    ("신의가호 발도 + 범위·피해 셋", "K_iaido_ho_0", "K_iaido_ho_J123", "×2.10", 2.10),
    ("발도 반경 5 가 더 맞히는 수 (keep_all)", "K_iaido_reach0", "K_iaido_reach", "맞은 수", None),
    ("검기 K 넷 (장갑 30 · 투구 50 · 흉갑 70 · 철화 40)", "K_wave_0", "K_wave_K", "×2.90", 2.90),
    ("방패치기 여섯 (수호검 50 · 격돌 50 · 투구 50 · 흉갑 70 · 철화 40 · 건틀릿 30)", "K_bash_0", "K_bash_K", "×3.90", 3.90),
    ("둔기강타 곡괭이(범위 · 쿨 −20 %) · 파쇄자 +50(★ 2026-10-21, 전 +25) · 계약의 철퇴 +10", "K_blunt_0", "K_blunt_E", "×1.60 · 쿨 35 → 29 (10-20 probe 는 +25 기준 ×1.35)", 1.60),
    ("둔기강타 범위 +100 % 가 더 맞히는 수", "K_blunt_reach0", "K_blunt_reach", "맞은 수", None),
    ("일섬 → 벽력일섬 세트 (+30 · 신발 +100 · 번개 1.5 배 = ×5.75)", "R_ilseom_0", "R_ilseom_set", "×5.75", 5.75),
    ("일섬 + 벽력일섬 반지 + 번개의 호흡 (×1.3 × 2.5 번개, 신발 없이)", "R_ilseom_0", "R_ilseom_I1", "×3.25", 3.25),
    ("급소가격 → 극사 (설계 ×3 · 뒤에서)", "R_geuksa_0", "R_geuksa_acc", "×3 (실측 ×4.3 → 모델 4.3)", 4.3),
    ("살상연희 백 번째 칼날만 (+5 타 · +30 %)", "R_sl_0", "R_sl_D1", "×2.11", 13 / 8 * 1.3),
    ("살상연희 무모한 칼춤만 (+6 타)", "R_sl_0", "R_sl_D2", "×1.75", 14 / 8),
    ("살상연희 방어구 넷 (+20 +20 +40 +20)", "R_sl_0", "R_sl_Darmor", "×2.00", 2.0),
    ("살상연희 D 여섯 (타수 8 → 14 — 두 단검은 큰 쪽 · 피해 +130 %)", "R_sl_0", "R_sl_D", "×4.03", 14 / 8 * 2.3),
    ("살상연희 + 마지막 춤 (3타 ×5 × 2.3)", "R_sl_0", "R_sl_D8", "×4.31", 3 / 8 * 5 * 2.3),
    ("일풍격 바람날개 (+30 %)", "R_ilpung_0", "R_ilpung_E", "×1.30 (한 마리)", 1.30),
    ("폭풍강타 제스 +100 · 파쇄자 +50 (+150 %; ★ 2026-10-21 파쇄자 25 → 50)", "B_storm_0", "B_storm_G", "×2.50 (10-20 probe 는 +75 기준 ×1.75)", 2.50),
    ("기본공격 → 분노태세 Lv5", "B_basic", "B_basic_rage", "DPS ×1.20", 1.20),
    ("기본공격 → 격분 100 % (×2 · 속도 ×1.4)", "B_basic", "B_fury", "DPS ×2.8", 2.8),
    ("격분 + 멈추지 않는 대검 (+60 %)", "B_fury", "B_fury_A5", "DPS ×1.60", 1.60),
    ("반격 (응징 +80 · 되갚는 손 +30)", "B_counter", "B_counter_G", "DPS ×2.10", 2.10),
    ("악의 할퀴기 → 손톱 · 비늘 (+50 % · 속도 +20 %)", "D_claw_0", "D_claw_C", "DPS ×1.8", 1.8),
    ("악의 할퀴기 vs 같은 캐릭터의 기본공격 (체1의1지1)", "D_basic_0", "D_claw_0", "DPS ×10 안팎", None),
    ("로그 기본공격 → 맹독감염 중 (중독 300 %)", "R_basic", "R_basic_venom", "DPS +공격력 × 4.5/s", None),
    ("야수 지옥불 → 야수조련 (×2)", "Be_ring", "Be_ring_x2", "×2", 2.0),
    # ★ 10-20 (2) 사용자: 체력 위주 · 2 레벨당 지력 1 나이트(LV 90 힘 10 · 체 233 · 지 57 · 야수 LV 47)의 악의 — 한 마리 13 초
    ("체력 나이트 평타 → 악의 할퀴기만", "V_basic", "V_claw", "DPS", None),
    ("악의 할퀴기 → + 지옥칼날 토글 (의지 300 벤치 — SP 2,340 이 8 초 만에 바닥)", "V_claw", "V_blade", "DPS", None),
    ("+ 어둠심판 토글", "V_blade", "V_blade_judge", "DPS", None),
    ("+ 용암불", "V_blade_judge", "V_all", "DPS", None),
    ("+ 기본공격 세팅 (손톱 +20 % 속도 +20 % · 비늘 +30 %)", "V_all", "V_all_gear", "DPS", None),
    ("+ 야수조련 목걸이 (야수 몫 ×2)", "V_all_gear", "V_all_gear_x2", "DPS", None),
    ("지옥칼날 대신 추종 (할퀴기 속도 · 흡혈 · 폭발)", "V_all_gear_x2", "V_follow_all_gear_x2", "DPS", None),
    ("같은 세팅, 의지 12 (SP 612 — 지옥칼날 · 어둠심판이 4 초 만에 꺼진다)", "V_all_gear_x2", "V_all_gear_x2_w12", "DPS", None),
    ("추종 세팅, 의지 12 (어둠심판은 8 %/s 라 SP 가 버틴다)", "V_follow_all_gear_x2", "V_follow_all_gear_x2_w12", "DPS", None),
    ("토글 · 용암불만 (OK 안 누름)", "V_claw", "V_toggles_only", "DPS", None),
]


def read_probe(path):
    out = {}
    for line in open(path, encoding="utf-8", errors="replace"):
        m = re.match(r"(\S+)\s+atk/def/crit/hp/sp=.* casts=\s*(\d+) per_cast=\s*(\d+) gap=(\S+) cd0=(\S+) sp=(\d+) dps=(\d+) hits=(\d+) targets=(\d+)", line)
        if m:
            out[m.group(1)] = dict(casts=int(m.group(2)), per_cast=int(m.group(3)), gap=m.group(4), cd0=m.group(5), sp=int(m.group(6)), dps=int(m.group(7)),
                                   hits=int(m.group(8)), targets=int(m.group(9)))
    return out


# ── 보고서 ────────────────────────────────────────────────────────────────────────────────────────────────────────────


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source")
    ap.add_argument("--md", default="HERO2_BALANCE_SURVEY.md")
    ap.add_argument("--anchors", default=os.path.join(ROOT, "wie_android", "src", "guest_anchors.rs"))
    ap.add_argument("--calib", default=os.path.join(ROOT, "tools_re", "raid_calib.json"))
    ap.add_argument("--probe", default=None)
    ap.add_argument("--endgame", default=None, help="tools_re/endgame_probe.py 가 남긴 endgame.json — §8 만렙 실측 표")
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    items = {(r["typeId"], r["index"]): r for r in GB.decode_all(a.source)}
    gear = parse_gear(a.anchors)
    ens = [e for e in GB.enemies(a.source) if e["maps"]]
    allrows = [(e, GB.hell_template(e), GB.guardian_template(GB.hell_template(e), e["gate"])) for e in ens]
    bosses = RB.raid_bosses(a.source)
    calib_doc = json.load(open(a.calib, encoding="utf-8")) if os.path.exists(a.calib) else {}
    calib = {k: v for k, v in calib_doc.items() if k.isdigit()}
    probe = read_probe(a.probe) if a.probe and os.path.exists(a.probe) else {}

    players = []
    for name, cls, slots, arch, demon in SETTINGS:
        p, ne = make_player(items, gear, cls, slots, archetype=arch)
        pf, nef = make_player(items, gear, cls, slots, archetype=arch, refine=GB.refine_plan(items, slots, "max"), seal=SEAL_PAIR[cls] if arch is None else (4, 1))
        players.append((name, cls, slots, p, ne, pf, nef, demon))

    out = []
    out.append("# 세팅별 밸런스 전수 조사 — 매핑(일반 몬스터) · 띠 보스 · 보스 레이드 (2026-10-20) — 생성 `tools_re/build_balance_survey.py`\n")
    out.append("> 검증 범위: 아래 모델 표는 근사치이며 전 직업 균형의 증명이 아니다. 이전 실측의 HP 재초기화 오류를 수정한 새 로그만 §8에 허용한다. 이동 맵핑·실제 레이드·장기 회전은 별도 실측이 필요하다.\n")
    out.append(f"조건: 캐릭터 **LV {LEVEL}** · 스킬 전부 Lv5(각성기 Lv5) · 패시브 최대 · 야수 레이든 계열 Lv5(야수 LV = 플레이어) · 능력치 = 직업 원형(버서커 힘70/체20/의10 · "
               "나이트 힘60/체30/민10 · 로그 힘30/체10/민60; 야수나이트 = 체력 5/6 · 지력 1/6(사용자 10-20 「체력 위주 · 2 레벨당 지력 1」) · 야수조련 목걸이 · 데몬 계열 야수 = 악의 가동률 ⅔ 안에서 할퀴기 + 토글(지옥칼날 · 어둠심판은 SP 예산만큼) + 용암불·멸화 + 추종, 밖에서 나이트 스킬 회전 — 네 슬롯 조합 중 최선; 악의 버서커 = 올힘) · **제련 0 · 랜덤 옵션 기대값 · 인장 없음**(「풀」 열 = 제련 최대 + 인장 +20 둘). "
               "적 = 가디언 모드 띠(80 대 LV 80..89 · 90 대 LV 90..105)의 일반 적 전부 · 띠 보스 1:1 · 레이드 아홉 단계. 무리 = 6 마리(동시에 때리는 3.5 · 살아 있는 평균 3.5). "
               "여유 = 사망 s ÷ 처치 s(포션 · 부활 없이): ❌ < 1 · 어려움 < 2 · 조금 어려움 < 4 · 보통 < 10 · 쉬움. 식 · 계수 = v2 모델(probe 실측으로 맞춘 것) + v3 보정(머리말). "
               "아이템 효과는 코드 표 `GearPatch::ITEMS` 를 직접 읽는다.\n")
    out.append(f"⚠ **트리 상태**(2026-10-20 02:00, 미커밋 — vC 136 에 실려 S25·S9 설치됨): 가디언 HARD = 설정 200 % × {GUARD_HARD} % = **{HARD_PCT} %**(10-19 (31)) · "
               f"검기/방패치기 K 계열 9종 + 1차 방어구 상향(최대 HP +40 % · 민첩/의지 +50) + 원월 +30 %/검귀 +50 %(기록 없음 — 어느 세션 작업인지 미확인) · "
               f"**레이드 보스 HP 배율 `H2_RAID_HP_MUL` = {RAID_HP_MUL}**(10-17 레이드 문서는 ×1 — 기록 없는 변경, 코드 그대로 반영). 레이드 ATK 18 % · 바퀴 ×1.3 HP / ×1.05 ATK · 받는 피해 보정 = `raid_calib.json`.\n")

    # ── 1. 한눈에
    out.append("## 1. 한눈에 — 세팅 × 매핑(90 대 NORMAL · HARD) × 띠 보스 × 레이드\n")
    out.append("| 세팅 | ATK / 스킬ATK / DEF / HP / SP | 단일 DPS (90 N) | 무리 처치 s (90 N) | 무리 여유 최소 (90 N) · 지는 적 | 무리 여유 최소 (90 HARD) · 지는 적 | 띠 보스 처치 s · 여유 최소 (90 N) | 레이드 NORMAL 바퀴 1 최소 여유 (단계) · 아홉 단계 처치 합 s | 레이드 마지막 바퀴 EASY / NORMAL / HARD (여유 ≥ 1 · 단계 ≤ 5 분) | 풀 세팅 레이드 E / N / H · 처치 합 s |\n|---|---|---|---|---|---|---|---|---|---|")
    lo, hi = GB.band_range(90)
    rows90 = [(e, h, gt) for e, h, gt in allrows if lo <= gt["L"] <= hi and e["gate"] != 0]
    boss90 = [(e, h, gt) for e, h, gt in allrows if lo <= gt["L"] <= hi and e["gate"] == 0]
    summary = {}
    for name, cls, slots, p, ne, pf, nef, demon in players:
        s1 = evaluate(p, ne, rows90, 1.5, False, demon=demon)
        s2 = evaluate(p, ne, rows90, 1.5, True, demon=demon)
        s2h = evaluate(p, ne, rows90, HARD_PCT / 100, True, demon=demon)
        s3 = evaluate(p, ne, boss90, 1.5, False, boss=True, demon=demon)
        rs = [raid_stage(p, ne, st, 150, 1, calib, demon) for st in bosses]
        w = min(range(len(rs)), key=lambda i: rs[i]["margin"])
        laps = [raid_last_lap(p, ne, bosses, dp, calib, demon, STAGE_CAP_S) for _, dp in RAID_DIFFS]
        lapsf = [raid_last_lap(pf, nef, bosses, dp, calib, demon, STAGE_CAP_S) for _, dp in RAID_DIFFS]
        tot = sum(r["kill_s"] for r in rs)
        totf = sum(raid_stage(pf, nef, st, 150, 1, calib, demon)["kill_s"] for st in bosses)
        lt = lambda n: "11+" if n == RB.LAP_STEPS + 1 else (str(n) if n else "❌")
        summary[name] = dict(s1=s1, s2=s2, s2h=s2h, s3=s3, rs=rs, laps=laps, lapsf=lapsf, tot=tot, totf=totf)
        out.append(f"| {name} | {p['atk']:.0f} / {p['skill_atk']:.0f} / {p['defence']:.0f} / {p['hp']:.0f} / {p['sp']:.0f} | {s1['dps']:,.0f} | {f(s2['kill'])} | "
                   f"×{f(s2['worst']['margin'])} {judge(s2['worst']['margin'])} · {s2['lose']} | ×{f(s2h['worst']['margin'])} {judge(s2h['worst']['margin'])} · {s2h['lose']} | "
                   f"{f(s3['kill'])} · ×{f(s3['worst']['margin'])} | ×{f(rs[w]['margin'])} ({bosses[w]['stage']} {bosses[w]['boss']}) · {tot:,.0f} s | "
                   f"{lt(laps[0])} / {lt(laps[1])} / {lt(laps[2])} | {lt(lapsf[0])} / {lt(lapsf[1])} / {lt(lapsf[2])} · {totf:,.0f} s |")
    out.append("")

    # ── 2. 매핑 상세
    out.append("## 2. 매핑 — 띠 × 난이도마다 (단일 DPS · 원천 · 무리 처치 · 여유 평균/최소 · 지는 적)\n")
    for label, blo, diff in BANDS:
        lo, hi = GB.band_range(blo)
        rows = [(e, h, gt) for e, h, gt in allrows if lo <= gt["L"] <= hi and e["gate"] != 0]
        out.append(f"### {label} — 일반 적 {len(rows)} 종\n")
        out.append("| 세팅 | 단일 DPS | 단일 처치 s | 단일 여유 최소 | 무리 처치 s | 무리 여유 평균 · 최소 | 판정 | 지는 적 (수 · 가장 위험) | 원천 (단일) |\n|---|---|---|---|---|---|---|---|---|")
        for name, cls, slots, p, ne, pf, nef, demon in players:
            s1 = evaluate(p, ne, rows, diff, False, demon=demon)
            s2 = evaluate(p, ne, rows, diff, True, demon=demon)
            out.append(f"| {name} | {s1['dps']:,.0f} | {f(s1['kill'])} | ×{f(s1['worst']['margin'])} | {f(s2['kill'])} | ×{f(s2['margin'])} · ×{f(s2['worst']['margin'])} | "
                       f"{judge(s2['worst']['margin'])} | {s2['lose']} · {s2['worst']['name']} | {parts_text(s1['parts'] if s1['parts'] else s1['worst']['parts'])} |")
        out.append("")

    # ── 3. 레이드 상세
    out.append("## 3. 보스 레이드 — 단계 × 난이도 (바퀴 1 여유 · 처치 s) · 마지막으로 다 깨는 바퀴\n")
    out.append(f"보스 HP = 가디언 HARD 템플릿 × 1.5 × 설정(133 · 200 · 267 %) × **{RAID_HP_MUL}**(`H2_RAID_HP_MUL`) · ATK = 같은 식 × 18 % · 받는 피해 = 일반 타격 × 단계 보정(1b 표의 ×0.9..3.8). "
               "칸 = 여유 (처치 s). 단계마다 HP · SP 가 가득 차므로 독립.\n")
    for name, cls, slots, p, ne, pf, nef, demon in players:
        out.append(f"### {name}\n")
        out.append("| 단계 | EASY | NORMAL | HARD | 풀 세팅 NORMAL | 풀 세팅 HARD |\n|---|---|---|---|---|---|")
        for st in bosses:
            cells = []
            for pp, nn, dp in ((p, ne, 100), (p, ne, 150), (p, ne, 200), (pf, nef, 150), (pf, nef, 200)):
                r = raid_stage(pp, nn, st, dp, 1, calib, demon)
                cells.append(f"{RB.cell(r)} ({r['kill_s']:.0f} s)")
            out.append(f"| {st['stage']} {st['boss']} | " + " | ".join(cells) + " |")
        sm = summary[name]
        lt = lambda n: "11+" if n == RB.LAP_STEPS + 1 else (str(n) if n else "❌ 바퀴 1 도 못 깬다")
        out.append(f"\n마지막으로 아홉 단계를 다 깨는 바퀴: EASY {lt(sm['laps'][0])} · NORMAL {lt(sm['laps'][1])} · HARD {lt(sm['laps'][2])} / 풀 세팅 EASY {lt(sm['lapsf'][0])} · NORMAL {lt(sm['lapsf'][1])} · HARD {lt(sm['lapsf'][2])}\n")

    # ── 4. 직업 안 · 직업 간
    out.append("## 4. 직업 안 비교 — 1차 가디언 대비 (90 대 NORMAL)\n")
    out.append("| 세팅 | 단일 DPS (1차 대비) | 무리 처치 s (1차 대비) | 무리 여유 최소 (1차 → 이것) | 띠 보스 처치 s (1차 대비) | 레이드 NORMAL 바퀴 1 최소 여유 (1차 → 이것) |\n|---|---|---|---|---|---|")
    base_of = {}
    for name, cls, slots, p, ne, pf, nef, demon in players:
        if "1차 가디언" in name:
            base_of[cls] = summary[name]
    for name, cls, slots, p, ne, pf, nef, demon in players:
        b = base_of[cls]
        sm = summary[name]
        rw = min(sm["rs"], key=lambda r: r["margin"])["margin"]
        bw = min(b["rs"], key=lambda r: r["margin"])["margin"]
        out.append(f"| {name} | {sm['s1']['dps']:,.0f} (×{sm['s1']['dps'] / b['s1']['dps']:.2f}) | {f(sm['s2']['kill'])} (×{sm['s2']['kill'] / b['s2']['kill']:.2f}) | "
                   f"×{f(b['s2']['worst']['margin'])} → ×{f(sm['s2']['worst']['margin'])} | {f(sm['s3']['kill'])} (×{sm['s3']['kill'] / b['s3']['kill']:.2f}) | ×{f(bw)} → ×{f(rw)} |")
    out.append("")

    # ── 5. probe 검증
    if probe:
        out.append("## 5. probe 실측 — 세팅의 배율이 게임 안에서 그대로인가 (`tools_re/build_probe.py`, LV 90 벤치 · 효과 워드만 입힘)\n")
        out.append("| 항목 | 없음 | 세팅 | 실측 배율 | 기대 | 비고 |\n|---|---|---|---|---|---|")
        for label, ca, cb, exp, expv in PROBE_ROWS:
            A, Bv = probe.get(ca), probe.get(cb)
            if not A or not Bv:
                out.append(f"| {label} | — | — | (미측정) | {exp} | |")
                continue
            if "맞은 수" in exp:
                out.append(f"| {label} | {A['targets']} 마리 | {Bv['targets']} 마리 | — | {exp} | 첫 시전에 HP 가 줄어든 몬스터 수 — 숲 벤치의 이웃 몬스터가 둘뿐이라 범위 차이는 미판정(§8.92 · §8.82 의 칸 실측이 정본) |")
                continue
            if "DPS" in exp:
                ratio = Bv["dps"] / A["dps"] if A["dps"] else 0
                note = "" if expv is None or abs(ratio / expv - 1) <= 0.12 else "⚠ 기대와 다름"
                out.append(f"| {label} | DPS {A['dps']:,} | DPS {Bv['dps']:,} | ×{ratio:.2f} | {exp} | {note} |")
                continue
            if "쿨" in exp:
                out.append(f"| {label} | 쿨 {A['cd0']} · SP {A['sp']} · 간격 {A['gap']} s | 쿨 {Bv['cd0']} · SP {Bv['sp']} · 간격 {Bv['gap']} s | — | {exp} | |")
                continue
            ratio = Bv["per_cast"] / A["per_cast"] if A["per_cast"] else 0
            note = "" if expv is None or abs(ratio / expv - 1) <= 0.08 else "⚠ 기대와 다름"
            out.append(f"| {label} | {A['per_cast']:,}/시전 | {Bv['per_cast']:,}/시전 | ×{ratio:.2f} | {exp} | {note} |")
        out.append("")
        out.append("원 자료 (사례 이름 · 시전 수 · 시전당 피해 · 간격 · 쿨 · SP · DPS · 맞은 수):\n")
        out.append("| 사례 | casts | per_cast | gap s | cd0 | sp | dps | targets |\n|---|---|---|---|---|---|---|---|")
        for k, v in probe.items():
            out.append(f"| {k} | {v['casts']} | {v['per_cast']:,} | {v['gap']} | {v['cd0']} | {v['sp']} | {v['dps']:,} | {v['targets']} |")
        out.append("")

    # ── 6. 요약 (자동 집계)
    out.append("## 6. 요약 — 숫자가 말하는 것 (자동 집계, 90 대 NORMAL 기준)\n")
    names = [x[0] for x in players]
    by_cls = {}
    for name, cls, *_ in players:
        by_cls.setdefault(cls, []).append(name)
    lt = lambda n: "11+" if n == RB.LAP_STEPS + 1 else (str(n) if n else "0")
    for cls, label in ((1, "나이트 계열"), (2, "로그"), (0, "버서커")):
        ns = by_cls[cls]
        best_dps = max(ns, key=lambda n: summary[n]["s1"]["dps"])
        best_pack = min(ns, key=lambda n: summary[n]["s2"]["kill"])
        safest = max(ns, key=lambda n: min(summary[n]["s2"]["worst"]["margin"], 999))
        best_raid = min(ns, key=lambda n: summary[n]["tot"])
        out.append(f"- **{label}**: 단일 DPS 최고 = {best_dps} ({summary[best_dps]['s1']['dps']:,.0f}) · 무리 처치 최단 = {best_pack} ({f(summary[best_pack]['s2']['kill'])} s) · "
                   f"무리 생존 최고 = {safest} (여유 ×{f(summary[safest]['s2']['worst']['margin'])}) · 레이드 아홉 단계 최단 = {best_raid} ({summary[best_raid]['tot']:,.0f} s, "
                   f"NORMAL 바퀴 {lt(summary[best_raid]['laps'][1])})")
    losers = [n for n in names if summary[n]["s2"]["worst"]["margin"] < 1]
    out.append(f"- **90 대 NORMAL 무리에서 지는 적이 있는 세팅 ({len(losers)}/{len(names)})**: " + (" · ".join(f"{n} ({summary[n]['s2']['lose']} 종)" for n in losers) if losers else "없음"))
    hard_ok = [n for n in names if summary[n]["s2h"]["worst"]["margin"] >= 1]
    out.append("- **90 대 HARD(300 %) 무리를 버티는 세팅**: " + (" · ".join(hard_ok) if hard_ok else "없음 — 모든 세팅이 무리 안에서 쓰러진다(여유 최대 ×"
               + f"{max(min(summary[n]['s2h']['worst']['margin'], 999) for n in names):.2f})"))
    raid_n = sorted(names, key=lambda n: summary[n]["tot"])
    out.append("- **레이드 NORMAL 바퀴 1 아홉 단계 처치 합 (빠른 순)**: " + " · ".join(f"{n} {summary[n]['tot']:,.0f} s" for n in raid_n[:8]) + " …")
    raid_dead = [n for n in names if min(r["margin"] for r in summary[n]["rs"]) < 1]
    out.append("- **레이드 NORMAL 바퀴 1 에서 쓰러지는 세팅**: " + (" · ".join(raid_dead) if raid_dead else "없음"))
    out.append("")
    out.append("## 7. 같은 벤치의 직업 회전 실측 — LV 90 · 숲 몬스터 한 마리 · 20 초 (`tools_re/class_rotation_probe.py`, 실제 의지 · 2026-10-20 트리 probe)\n")
    out.append("| 세팅 (벤치) | DPS | 비고 |\n|---|---|---|")
    for row in ROTATION_BENCH:
        out.append("| " + " | ".join(row) + " |")
    out.append("")
    out.append("벤치의 야수는 세이브의 야수 그대로 **LV 47**(캐릭터만 LV 90 으로 올린 세이브)이라 야수 몫(토글 · 용암불 · 지옥불)은 LV 99 야수보다 작다. "
               "같은 조건에서 **체력 나이트의 악의 세팅(추종 · 어둠심판 · 용암불 · 손톱 · 비늘 · 야수조련) 3,113 ≥ 로그 회전 2,902 > 격분 버서커 2,634 > 나이트 회전 1,536** — "
               "사용자 지적대로 악의 세팅의 기본공격 + 토글 DPS 는 낮지 않다. 모델을 같은 벤치 조건에 놓으면 지옥칼날 세팅 2,671 / 추종 세팅 3,022 로 실측과 3~4 % 안이다. "
               "LV 99 · 가디언 장비 · 90 대 적의 모델(§1)에서 격차가 벌어지는 것은 버서커 쪽의 곱(오딘 · 독사 기본공격 +100 % × 분노 1.2 × 도발 1.2 × 격분)과 "
               "악의 쪽의 지력 몫(할퀴기 = 공격력 + 지력' × 4.73 — 체력 5/6 빌드는 지력' ≈ 60)이 LV 99 에서 서로 다르게 자라기 때문이고, 그 격차를 어디서 메울지가 조정 지점이다. "
               "⛔ 토글의 SP(지옥칼날 최대 SP 의 12 %/s · 어둠심판 8 %/s)는 의지를 안 찍은 빌드(SP ≈ 670)에서 지옥칼날을 4 초, 어둠심판을 17 초만 켜 준다 — 추종 + 어둠심판이 그 빌드의 답이다.\n")
    if a.endgame and os.path.exists(a.endgame):
        with open(a.endgame, encoding="utf-8") as source:
            benchmark = json.load(source)
        if not isinstance(benchmark, dict) or not benchmark.get("probe_sha256"):
            raise ValueError("Legacy benchmark is not valid balance evidence; rerun endgame_probe.py")
        out.append("## 8. 고정 위치 만렙 실측 — 이동 맵핑·레이드 순위로 해석하지 않음\n")
        out.append("준비 종료 시 HP/SP 회복 후 실제 적 공격을 복구한다. HP 재초기화·개체 교체는 무효 처리한다. 생존 중 DPS와 사망 후 0을 포함한 전체 창 DPS를 함께 표시한다.\n")
        out.append("| 세팅 | 단일 유효 | 생존 중 DPS | 전체 창 DPS | 단일 사망(s) | 무리 유효 | 무리 처치 | 무리 사망(s) |\n|---|---|---|---|---|---|---|---|")
        for label, r1, r2 in benchmark["results"]:
            dps = f"{r1['dps']:,.0f}" if r1.get("valid") else "무효"
            full = f"{r1['dmg'] / (benchmark['frames'] * .02):,.0f}" if r1.get("valid") else "무효"
            out.append(f"| {label} | {r1.get('valid', False)} | {dps} | {full} | {r1['died']} | {r2.get('valid', False)} | {r2['kills']}/{r2['targets']} | {r2['died']} |")
        out.append("\n단일 대상은 일반 몬스터의 HP를 늘린 시험이며 실제 레이드 보스가 아니다. 서로 다른 사망 시간과 첫 대상 전멸 시간이 있으므로 생존 중 DPS만으로 균형을 판정할 수 없다.\n")
    text = "\n".join(out) + "\n"
    open(a.md, "w", encoding="utf-8", newline="\n").write(text)
    print(f"wrote {a.md}")
    for name, cls, slots, p, ne, pf, nef, demon in players:
        sm = summary[name]
        print(f"{name[:40]:40s} dps={sm['s1']['dps']:8.0f} pack={sm['s2']['kill']:5.1f}s worst={min(sm['s2']['worst']['margin'],999):6.2f} hard={min(sm['s2h']['worst']['margin'],999):6.2f} "
              f"boss={sm['s3']['kill']:5.1f}s raidN1={min(min(r['margin'] for r in sm['rs']),999):6.2f} tot={sm['tot']:6.0f}s laps={sm['laps']} full={sm['lapsf']} totf={sm['totf']:6.0f}s")


if __name__ == "__main__":
    main()
