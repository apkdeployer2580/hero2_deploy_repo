#!/usr/bin/env python3
"""보스 레이드 밸런스 (2026-10-17, 사용자: *"각 단계별로 특정 아이템셋팅들을 모두 맞추고 제련도 풀, 인장도 풀로 박은채로 클리어가 가능한지
보스레이드 난이도 밸런싱 점검"*). `guardian_balance.py` 의 결투 모델(probe 실측으로 맞춘 식 · 상수 그대로)에 레이드 보스를 넣는다.

    python tools_re/raid_balance.py <hero2_ktf.zip> [--md HERO2_RAID_BALANCE.md] [--calib raid_calib.json]

레이드 보스 (확정 — probe 실측 아홉과 HP · ATK 가 정수까지 같다):
  템플릿   guardian_balance.hell_template → guardian_template(드랍 게이트 0 = 보스 ×307>>8) → HP · ATK ×384>>8(GuardianPatch::RAID_MUL)
           · DEF = 가디언 DEF 그대로 · 민첩 e 그대로 · LV 110 평평 · 보정이 보는 LV = 60 + (110 − 70)·146>>8 = 82(회피 자리만 110)
  호스트   HP % = p × 1.3^n · ATK % = p × 1.05^n × 0.18, p = 설정 % × 4/3 = 133 · 200 · 267, n = min(바퀴 − 1, 10) (hero2java::h2_raid_want ·
           ★★ 2026-10-17 (4) ×0.18 = H2_RAID_ATK_PCT — 이 도구의 첫 판(×1.0)이 「풀 세팅으로도 바퀴 1 불가」를 낸 뒤 사용자가 공격력만 낮췄다)
  곁 몬스터 고대수호물의 파멸 · 응보의 수호물 · 오딘의 두 손 — 처치 판정은 보스 하나(ClearStage 가 나머지를 같이 눕힌다)라 처치 시간 = 보스 HP,
           받는 피해는 probe 보정(calib)이 곁 몬스터 · 특수기까지 담는다
  매 보스  처치 순간 HP · SP 가득(BossRefill) → 단계마다 독립된 1:1 결투. 쓰러지면 그때까지의 골드를 받고 신전으로(H2RaidAction::Escape)

풀 세팅 (플레이어):
  LV 99 · 직업 원형(guardian_balance.ARCHETYPE / LOADOUTS) · 장비 = 가디언 한 벌(LOADOUTS — 직업마다 옵션이 맞는 것) + 개성 옵션(GearPatch::ITEMS)
  제련 풀  무기 · 갑옷 · 방패를 상한까지(guardian_balance.refine_plan "max") — 투구 · 장갑 · 신발 · 장신구는 상한 0
  인장 풀  연금술사 「장비 능력 부여」의 대상 = `x.a:Lbu;`(장비 레코드 — 무기 · 방어구, 장신구 `z` 는 아님) · 한 장비에 능력치 둘까지 ·
           상급 인장 15..20 의 최대 → 장신구 밖 장비마다 고른 능력치 둘 +20 (랜덤 옵션은 덮어쓴다). 고르는 둘 = 직업마다 아래 탐색이 가장 많은
           (보스, 바퀴)를 깨는 쌍
  모델 밖  포션 · 부활의 구 · 각성기 · 로그 「암살」 · 세트 효과 · 기사 방패 막기 · 격분 · 반격 · 발도 범위 같은 특수 옵션(효과 표의 Dmg · Basic ·
           Skills · Taken · Reflect · SpCost · Crit · Evade · Venom · Heal · Lifesteal · BeastCd · MaxHp/Sp · AllCd 만 센다)
  ⚠ 보스 넷(도적 두목 · 이안 · 소문의 검사 · 디미트리)은 `ca`(순간이동) — 앞 칸에 늘 서지 않는다. 모델의 처치 시간은 붙어서 치는 것으로 센다.

시간: guardian_balance 의 초는 probe 프레임 ÷ 20 이라 게임 초의 2.5 배다(HERO2_NEW_ITEM_DRAFT §4 실측) — 여유(사망 s ÷ 처치 s)는 비라서 그대로,
이 문서의 「처치 s」는 ÷ 2.5 한 게임 초.
"""

import argparse
import itertools
import json
import os
import re
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import guardian_balance as GB  # noqa: E402
import raid_gate as RG  # noqa: E402
from decode_hero2_items import decode_all  # noqa: E402

RAID_MUL = 384  # GuardianPatch::RAID_MUL (/256)
RAID_LEVEL = 110  # GuardianPatch::RAID_LEVEL
LAP_STEPS = 10  # hero2java::H2_RAID_LAP_STEPS
LAP_HP, LAP_ATK = 130, 105  # H2_RAID_LAP_HP_PCT · H2_RAID_LAP_ATK_PCT
RAID_HP_MUL = 10  # ★ 2026-10-19/20 hero2java::H2_RAID_HP_MUL — 레이드 보스 HP ×10 (사용자 변경, 기록 없던 것)
RAID_ATK_PCT = 12  # ★ 2026-10-20 18 → 12 (밸런스 전수 조사 ①) · ★★ 2026-10-17 (4) hero2java::H2_RAID_ATK_PCT — 레이드 단계의 ATK 만 18 % (사용자: 공격력만 낮춤, HP 그대로)
GOLD_BASE = (200_000, 500_000, 1_000_000)
GOLD_STEP = (100_000, 200_000, 500_000)
DIFFS = (("EASY", 100), ("NORMAL", 150), ("HARD", 200))  # hero2java::DIFFICULTY_PCT
SEAL_MAX = 20  # 상급 인장 15..20 의 최대
SEAL_SLOTS = ("w", "w2", "h", "a", "g", "s", "sh")  # 장신구 밖 — 한 벌에 있는 것만
STAT_NAMES = ("힘", "체력", "민첩", "의지", "지력")
TIME_FIX = 2.5  # guardian_balance 의 초 ÷ 이것 = 게임 초
LEVEL = 99
CLASSES = ("버서커", "나이트", "로그", "야수나이트")


def pct_pow(p, step, n):
    den = 100 ** n
    return (p * step ** n + den // 2) // den


def raid_want(setting_pct, lap):
    """hero2java::h2_raid_want — (HP %, ATK %) at 바퀴 `lap`(1..) · ★★ (4) ATK 는 그 RAID_ATK_PCT % 만."""
    p = (setting_pct * 4 + 1) // 3
    n = min(lap - 1, LAP_STEPS)
    den = 100 ** n * 100
    return pct_pow(p, LAP_HP, n) * RAID_HP_MUL, (p * LAP_ATK ** n * RAID_ATK_PCT + den // 2) // den


def gold(d, lap):
    return GOLD_BASE[d] + GOLD_STEP[d] * min(lap - 1, LAP_STEPS)


def raid_bosses(src):
    """단계마다 [(종류, 이름, base HP, base ATK, DEF, e)] — base = 100 % (호스트 배율 전)."""
    ens = GB.enemies(src)
    out = []
    for k, (room, kinds, _spawns, _entry, boss) in enumerate(RG.STAGES):
        mons = []
        for kind in kinds:
            e = ens[kind]
            g = GB.guardian_template(GB.hell_template(e), e["gate"])
            mons.append(dict(kind=kind, name=e["name"], hp=(g["hp"] * RAID_MUL) >> 8, atk=(g["atk"] * RAID_MUL) >> 8, defence=g["defence"], e=g["e"]))
        out.append(dict(stage=k + 1, map=RG.stage_map(k), room=room, boss=boss, mons=mons))
    return out


# ── 개성 옵션 표 (GearPatch::ITEMS, 79 — 이 모델이 세는 효과만) ───────────────────────────────────────────────────────────

def parse_gear(path):
    text = open(path, encoding="utf-8").read()
    out = {}
    for m in re.finditer(r'GearItem \{ item_type: (\d+), id: (\d+), text: "[^"]*", effects: &\[(.*?)\] \}', text):
        effs = []
        for name, args in re.findall(r"(\w+)\(([^()]*)\)", m.group(3)):
            vals = []
            for x in args.split(","):
                x = x.strip()
                if re.fullmatch(r"-?\d+", x):
                    vals.append(int(x))
            effs.append((name, tuple(vals)))
        out[(int(m.group(1)), int(m.group(2)))] = effs
    return out


def gear_options(table, keys, cls):
    o = GB.gear_options(table, keys, cls)
    o["skills"] = 0.0
    o["cd"] = 1.0
    for key in keys:
        for name, a in table.get(key, []):
            if name == "Skills" and a:
                o["skills"] += a[0] / 256
            elif name == "AllCd" and a:
                o["cd"] *= 1 - a[0] / 256
    return o


# ── 플레이어: guardian_balance.player + 인장 ───────────────────────────────────────────────────────────────────────────

def player(cls, level, items, slots, archetype, opts, refine, seal):
    """GB.player 와 같은 식 · `seal` = (능력치 a, 능력치 b) 면 장신구 밖 장비마다 그 둘 +SEAL_MAX, 그 장비의 랜덤 옵션은 없다(덮어쓴다)."""
    base = GB.BASE_STATS[cls]
    pts = 3 * (level - 1)
    arch = archetype or GB.ARCHETYPE[cls]
    stats = [base[k] + int(pts * arch[k]) for k in range(5)]
    sealed = {s for s in slots if s in SEAL_SLOTS} if seal else set()
    plain = {s: k for s, k in slots.items() if s not in sealed}
    watk, gdef, bonus, hp_flat, sp_flat = GB.gear_stats(items, cls, plain, refine)
    # 봉인된 칸: 무기 · 방어 수치만(랜덤 옵션 0) + 인장
    for s in sealed:
        r = items[slots[s]]
        lv = min((refine or {}).get(s, GB.REFINE), r.get("refineMax") or 0)
        if s.startswith("w"):
            share = 1.0 if s == "w" else 0.0
            watk += share * ((r["power"] + r["powerMin"]) / 2 + (3 * lv) // 2) * GB.WEAPON_BONUS[slots[s][0]]
        else:
            gdef += r["power"] + (3 * lv) // 2
        for k in seal:
            bonus[k] += SEAL_MAX
    o = opts
    eff = [stats[k] + bonus[k] for k in range(5)]
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
    return dict(cls=cls, level=level, stats=stats, eff=eff, atk=atk, skill_atk=skill_atk, defence=defence, hp=hp, sp=sp, crit=GB.clamp(crit, 8, 90), ak=ak, am=am, opts=o)


# ── 결투: guardian_balance.rotation_dps / duel 의 레이드 판 ──────────────────────────────────────────────────────────────

SEEN = 60 + (((RAID_LEVEL - 70) * 146) >> 8)  # 레이드 맵의 읽기 조각이 보정한 LV (= 82)


def rotation_dps(p, m, beast):
    """GB.rotation_dps 와 같은 식 — 대상 레벨이 평평한 110(보정 82) · 「직업 스킬 피해」(Skills) · 쿨 −(AllCd) 를 더 센다."""
    cls = p["cls"]
    o = p["opts"]
    hit = (100 - GB.clamp(p["ak"] + SEEN + m["e"] / 5, 8, 90)) / 100
    crit = p["crit"] / 100 * GB.clamp(20 + SEEN - p["level"], 8, 50) / 100
    critx = 1 + crit * (GB.CALIB["crit_mul"] - 1)
    d = max(p["atk"] - m["defence"], 0)
    swing = GB.CALIB["attack_frames"][cls] / 20
    buff = {} if beast else GB.BUFF[cls]
    bx = buff.get("dmg", 1.0)
    plain = d * GB.CALIB["combo_avg"][cls] * (1 + o["dmg"].get(0, 0)) * hit * bx
    basic_dps = plain * critx / swing
    want, free = [], 1.0
    for name, kind, mul, cd, sp in GB.ROTATION[cls]:
        per = max(p["skill_atk"] - m["defence"], 0) * mul * buff.get("skill", {}).get(kind, 1.0) * bx * (1 + o["dmg"].get(kind, 0) + o["skills"]) * critx * hit
        share = min(swing / (cd * o["cd"] / 20), free)
        want.append((per / swing * share, share, sp * o["sp"] * share / swing))
        free -= share
    beasts = []
    if beast:
        for name, ring, cd, sp, hits in GB.BEAST_SKILLS:
            rate = 1 / (cd * o["beast_cd"] / 20)
            beasts.append((GB.beast_hit(p, ring, m["defence"]) * hits * rate, sp * o["sp"] * rate))
    sp_want = sum(w[2] for w in want) + sum(b[1] for b in beasts)
    scale = min(1.0, GB.SP_REGEN * p["sp"] / sp_want) if sp_want > 0 else 1.0
    used = sum(w[1] for w in want) * scale
    dps = sum(w[0] for w in want) * scale + basic_dps * (1 - used) + sum(b[0] for b in beasts) * scale
    if buff.get("poison") and (1 - used) > 0:
        dps += p["atk"] * buff["poison"] * (1 + o["venom"])
    return dps, scale, 1 - used, plain


def melee_taken_dps(p, m, atk_pct, beast):
    """한 마리의 일반 타격만(GB.duel 과 같은 식): (피격/초, 반사/초)."""
    o = p["opts"]
    atk = min(m["atk"] * atk_pct / 100, 32767)
    taken = max(atk - p["defence"], 3) * o["taken"] * ({} if beast else GB.BUFF[p["cls"]]).get("taken", 1.0)
    mcrit = GB.clamp(20 + SEEN - p["level"], 8, 50) / 100
    evade = GB.clamp(p["am"] - RAID_LEVEL - m["e"] / 10, 8, 90) / 100
    rate = 20 / GB.CALIB["monster_attack_frames"]
    return taken * (1 + mcrit * (GB.CALIB["mon_crit_mul"] - 1)) * (1 - evade) * rate, taken * o["reflect"] * (1 - evade) * rate


def duel(p, stage, hp_pct, atk_pct, beast, calib):
    """레이드 단계 하나: 보스 HP 를 깎는 시간 vs 받는 피해(보스 일반 타격 × 단계의 보정 배율 — 곁 몬스터 · 특수기 포함)."""
    boss = stage["mons"][0]
    o = p["opts"]
    hp = boss["hp"] * hp_pct / 100
    dps, scale, basic_share, plain = rotation_dps(p, boss, beast)
    taken, reflect = melee_taken_dps(p, boss, atk_pct, beast)
    k = calib.get(str(stage["stage"]), 1.0)
    taken *= k
    swing = GB.CALIB["attack_frames"][p["cls"]] / 20
    leech = plain * o["lifesteal"] / swing * basic_share
    kill_s = hp / (dps + reflect) if dps + reflect > 0 else float("inf")
    net = taken - leech
    die_s = p["hp"] / net if net > 0 else float("inf")
    return dict(kill_s=kill_s / TIME_FIX, die_s=die_s / TIME_FIX, margin=die_s / kill_s if kill_s else float("inf"), dps=dps, taken=taken, sp=scale)


# ── 풀 세팅 ────────────────────────────────────────────────────────────────────────────────────────────────────────────

def build(items, table, name, seal):
    cls, arch, slots = GB.LOADOUTS[name]
    opts = gear_options(table, list(slots.values()), cls)
    refine = GB.refine_plan(items, slots, "max")
    return player(cls, LEVEL, items, slots, arch, opts, refine, seal), slots, refine


def clears(p, bosses, beast, calib, d_pct, lap):
    return [duel(p, st, *raid_want(d_pct, lap), beast, calib) for st in bosses]


def best_seal(items, table, name, bosses, calib):
    """장신구 밖 장비마다 같은 능력치 둘 — NORMAL · 바퀴 1..11 에서 여유 ≥ 1 인 (보스, 바퀴) 가 가장 많은 쌍(같으면 여유 합)."""
    beast = name.startswith("야수")
    best = None
    for pair in itertools.combinations(range(5), 2):
        p, _, _ = build(items, table, name, pair)
        n = s = 0.0
        for lap in range(1, LAP_STEPS + 2):
            for r in clears(p, bosses, beast, calib, 150, lap):
                n += r["margin"] >= 1
                s += min(r["margin"], 10)
        if best is None or (n, s) > best[0]:
            best = ((n, s), pair)
    return best[1]


def cell(r):
    m = r["margin"]
    mark = "✅" if m >= 2 else "⚠" if m >= 1 else "❌"
    return f"{mark} ×{m:.2f}" if m < 10 else f"{mark} ×{m:.0f}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source")
    ap.add_argument("--md", default="HERO2_RAID_BALANCE.md")
    ap.add_argument("--calib", default=None, help="probe 보정 JSON {단계: 배율, 'runs': …} — 없으면 1.0(일반 타격만)")
    ap.add_argument("--anchors", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "wie_android", "src", "guest_anchors.rs"))
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    items = {(r["typeId"], r["index"]): r for r in decode_all(a.source)}
    table = parse_gear(a.anchors)
    bosses = raid_bosses(a.source)
    calib_doc = json.load(open(a.calib, encoding="utf-8")) if a.calib else {}
    calib = {k: v for k, v in calib_doc.items() if k.isdigit()}
    out = []
    out.append("# 보스 레이드 밸런스 — 풀 세팅(가디언 한 벌 · 제련 풀 · 인장 풀 · LV 99)으로 단계 · 바퀴 · 난이도마다 깨지나 (2026-10-17, ★ (4) 보스 ATK 18 % 적용 뒤) — 생성 `tools_re/raid_balance.py`\n")
    out.append("★★ (4) 이 문서의 첫 판(ATK 그대로)은 「버서커 · 나이트 · 야수나이트는 EASY 바퀴 1 도 1:1 불가 · 로그만 EASY 4 · NORMAL 1」이었다 → 사용자: "
               "*\"오케이 낮추자. 너 권장대로 공격력만 낮출게 HP는 그대로야\"* → 레이드 단계의 ATK 만 **18 %**(`hero2java::H2_RAID_ATK_PCT` — 풀 세팅 네 직업이 "
               "NORMAL 바퀴 1 을 다 깨는 가장 큰 값: 버서커 여유 ×1.21 · 19 % 면 ×0.92). 아래 표는 전부 그 뒤의 값.\n")
    out.append("식 · 가정 · 모델 밖은 스크립트 머리말. 결투 식과 보정 상수는 `guardian_balance.py`(probe 실측으로 맞춘 것) 그대로. "
               "**여유** = 쓰러질 때까지의 시간 ÷ 보스를 잡는 시간(포션 · 부활의 구 없이 1:1) — ❌ < 1(진다) · ⚠ 1..2(빠듯) · ✅ ≥ 2. "
               "보스를 잡을 때마다 HP · SP 가 가득 차므로(BossRefill) 단계마다 독립이고, 한 바퀴를 다 도는 조건 = 아홉 단계 전부 여유 ≥ 1.\n")
    # 1 — 보스
    out.append("## 1. 레이드 보스 — 바퀴 1 의 HP / ATK (설정 EASY · NORMAL · HARD = 133 · 200 · 267 %) · 바퀴 11(10회 상승 끝)\n")
    out.append("| 단계 | 보스 | DEF | 민첩 e | EASY HP / ATK | NORMAL HP / ATK | HARD HP / ATK | NORMAL 바퀴 11 HP / ATK | 받는 피해 보정 (probe) |\n|---|---|---|---|---|---|---|---|---|")
    for st in bosses:
        b = st["mons"][0]
        cells = []
        for _, dp in DIFFS:
            hp_p, atk_p = raid_want(dp, 1)
            cells.append(f"{b['hp'] * hp_p // 100:,} / {b['atk'] * atk_p // 100:,}")
        hp11, atk11 = raid_want(150, 11)
        extra = " · ".join(f"{m['name']}" for m in st["mons"][1:])
        k = calib.get(str(st["stage"]))
        out.append(f"| {st['stage']} ({st['map']}) | {b['name']}" + (f" + {extra}" if extra else "") + f" | {b['defence']} | {b['e']} | " + " | ".join(cells)
                   + f" | {b['hp'] * hp11 // 100:,} / {b['atk'] * atk11 // 100:,} | " + (f"×{k:.2f}" if k else "— (일반 타격만)") + " |")
    out.append("")
    out.append("바퀴 n 의 배율(NORMAL): " + " · ".join(f"{n} {raid_want(150, n)[0]}/{raid_want(150, n)[1]} %" for n in (1, 2, 3, 5, 8, 11)) + " (HP/ATK). "
               "골드/마리: " + " · ".join(f"{dn} {gold(i, 1) // 10_000}만 → 바퀴 11 {gold(i, 11) // 10_000}만" for i, (dn, _) in enumerate(DIFFS)) + ".\n")
    if calib_doc.get("runs"):
        out.append("### 1b. 받는 피해 보정 — 보스의 공격 박자, probe 실측 (`tools_re/raid_calib.py`)\n")
        out.append("모델의 받는 피해는 「붙은 몬스터가 45 프레임마다 한 번, ATK×U(0.9,1.1) − DEF · 치명 ×2.5」(guardian_balance 의 일반 몬스터 실측)다. "
                   "한 타의 크기는 레이드 보스도 이 식 그대로였고(단계 1 NORMAL 실측: 1,781..2,033 · 치명 4,652 = ATK 2,006 − DEF 57), **다른 것은 박자**다 — "
                   "보스는 연타(6 프레임 간격 2..3 타)와 특수기, 곁 몬스터(수호물 · 손)까지 쳐서 훨씬 자주 맞는다. 서 있는 LV 85 나이트(보스 피해를 1/256 로 낮춰 "
                   "3,200 프레임 버티게 한 런)에서 센 맞는 간격으로 모델의 공격 빈도를 곱한 것이 배율이고, 아래 표들의 받는 피해는 전부 그 배율을 곱했다. "
                   "⚠ 서 있는 상대의 박자라 위쪽 한계 — 맞아서 움찔하는 보스는 덜 친다.\n")
        out.append("| 단계 | 보스 | 측정 창 (프레임) | 맞은 횟수 | 맞는 간격 (프레임) | 모델 간격 | 배율 |\n|---|---|---|---|---|---|---|")
        for r in calib_doc["runs"]:
            out.append(f"| {r['stage']} | {r['boss']} | {r['frames']} | {r['hits']} | {r['per']} | 45 ÷ 0.92 | ×{r['factor']:.2f} |")
        out.append("")
    # 2 — 풀 세팅
    out.append("## 2. 풀 세팅 — LV 99 · 가디언 한 벌 + 개성 옵션 · 제련 풀 · 인장 풀\n")
    seals = {}
    builds = {}
    for name in CLASSES:
        seals[name] = best_seal(items, table, name, bosses, calib)
        builds[name] = build(items, table, name, seals[name])
    out.append("| 직업 | 장비 (제련) | 인장 (장비마다 +20 둘) | ATK / 스킬 ATK / DEF / HP / SP / 치명 % |\n|---|---|---|---|")
    for name in CLASSES:
        p, slots, refine = builds[name]
        gear = " · ".join(items[k]["name"] + (f" +{refine[s]}" if refine.get(s) else "") for s, k in slots.items())
        sealed = sum(1 for s in slots if s in SEAL_SLOTS)
        out.append(f"| {name} | {gear} | {STAT_NAMES[seals[name][0]]} · {STAT_NAMES[seals[name][1]]} ×{sealed} 장비 = 각 +{SEAL_MAX * sealed} | "
                   f"{p['atk']:.0f} / {p['skill_atk']:.0f} / {p['defence']:.0f} / {p['hp']:.0f} / {p['sp']:.0f} / {p['crit']:.0f} |")
    out.append("")
    # 3 — 한눈에
    out.append("## 3. 한눈에 — 아홉 단계를 다 깨는 마지막 바퀴 (여유 ≥ 1 · 1:1 · 포션 없이)\n")
    out.append("| 직업 | EASY | NORMAL | HARD | NORMAL 바퀴 1 에서 가장 어려운 단계 |\n|---|---|---|---|---|")
    full = {}
    for name in CLASSES:
        p = builds[name][0]
        beast = name.startswith("야수")
        cells = []
        for _, dp in DIFFS:
            last = 0
            for lap in range(1, LAP_STEPS + 2):
                rs = clears(p, bosses, beast, calib, dp, lap)
                if all(r["margin"] >= 1 for r in rs):
                    last = lap
                else:
                    break
            full[(name, dp)] = last
            cells.append("바퀴 11+ (끝까지)" if last == LAP_STEPS + 1 else f"바퀴 {last}" if last else "❌ 바퀴 1 도 못 깬다")
        rs = clears(p, bosses, beast, calib, 150, 1)
        w = min(range(len(rs)), key=lambda i: rs[i]["margin"])
        out.append(f"| {name} | " + " | ".join(cells) + f" | {bosses[w]['stage']} {bosses[w]['boss']} ×{rs[w]['margin']:.2f} |")
    out.append("")
    # 4 — 단계 × 바퀴
    out.append("## 4. 단계 × 바퀴 — 여유 (직업 · 난이도마다)\n")
    out.append("칸 = 여유 · 처치 s 는 게임 초(바퀴 1 열의 괄호).\n")
    laps = (1, 2, 3, 4, 5, 6, 8, 11)
    for name in CLASSES:
        p = builds[name][0]
        beast = name.startswith("야수")
        for dn, dp in DIFFS:
            out.append(f"### {name} · {dn}\n")
            out.append("| 단계 | " + " | ".join(f"바퀴 {n}" for n in laps) + " |\n|---|" + "---|" * len(laps))
            for i, st in enumerate(bosses):
                row = []
                for n in laps:
                    r = duel(p, st, *raid_want(dp, n), beast, calib)
                    row.append(cell(r) + (f" ({r['kill_s']:.0f} s)" if n == 1 else ""))
                out.append(f"| {st['stage']} {st['boss']} | " + " | ".join(row) + " |")
            out.append("")
    # 5 — 공격력을 얼마나 낮추면
    out.append("## 5. 공격력을 지금(18 %)에서 더 바꾼다면 — 바퀴 1 에서 아홉 단계 전부 여유 ≥ 1 / ≥ 2 가 되는 ATK 배율 (지금 = 1.00 = 원래의 0.18)\n")
    out.append("HP 는 그대로 두고 보스 ATK 에만 곱한다(레이드의 ATK = 가디언 HARD × 1.5 × 설정 × 4/3 × 0.18 — 그 전체에 곱하는 수; 1.00 = 지금). "
               "괄호 = 그때 가장 빠듯한 단계.\n")
    out.append("| 직업 | EASY ≥ 1 | EASY ≥ 2 | NORMAL ≥ 1 | NORMAL ≥ 2 | HARD ≥ 1 |\n|---|---|---|---|---|---|")

    def atk_scale_for(p, beast, dp, need):
        hp_p, atk_p = raid_want(dp, 1)
        for s100 in range(100, 0, -1):
            s = s100 / 100
            rs = [duel(p, st, hp_p, atk_p * s, beast, calib) for st in bosses]
            if all(r["margin"] >= need for r in rs):
                w = min(range(len(rs)), key=lambda i: rs[i]["margin"])
                return f"×{s:.2f} ({bosses[w]['boss']})"
        return "× < 0.01"
    for name in CLASSES:
        p = builds[name][0]
        beast = name.startswith("야수")
        out.append(f"| {name} | " + " | ".join(atk_scale_for(p, beast, dp, need) for dp, need in ((100, 1), (100, 2), (150, 1), (150, 2), (200, 1))) + " |")
    out.append("")
    text = "\n".join(out) + "\n"
    with open(a.md, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print(f"wrote {a.md}")
    for name in CLASSES:
        print(name, "seal", [STAT_NAMES[s] for s in seals[name]], {dn: full[(name, dp)] for dn, dp in DIFFS})


if __name__ == "__main__":
    main()
