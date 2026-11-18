#!/usr/bin/env python3
"""밸런스 지렛대 what-if (2026-10-20) — build_balance_survey 의 모델 위에 상수 하나씩 바꿔 핵심 세팅 열 개를 다시 센다.

    python tools_re/balance_whatif.py
"""
import sys, io, os, json
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")
sys.path.insert(0, r"C:\hero2NativeAPP\tools_re"); sys.argv = ["x"]
import build_balance_survey as S, guardian_balance as GB, demon_balance as DB, skill_gear_balance as SG, raid_balance as RB
src = r"C:\hero2NativeAPP\android_app\app\src\main\assets\hero2_ktf.zip"
items = {(r["typeId"], r["index"]): r for r in GB.decode_all(src)}
gear = S.parse_gear(os.path.join(S.ROOT, "wie_android", "src", "guest_anchors.rs"))
ens = [e for e in GB.enemies(src) if e["maps"]]
allrows = [(e, GB.hell_template(e), GB.guardian_template(GB.hell_template(e), e["gate"])) for e in ens]
lo, hi = GB.band_range(90)
rows = [(e, h, gt) for e, h, gt in allrows if lo <= gt["L"] <= hi and e["gate"] != 0]
bosses = RB.raid_bosses(src)
calib = {k: v for k, v in json.load(open(r"C:\hero2NativeAPP\tools_re\raid_calib.json", encoding="utf-8")).items() if k.isdigit()}
PICK = ["나이트 · 1차 가디언", "나이트 · 발도 쌍월", "나이트 · 검기 (", "야수나이트 · 체5/6 지1/6 · 데몬 + 기본공격", "야수나이트 · 체1의1지1", "로그 · 1차", "로그 · 살상연희 (", "버서커 · 1차", "버서커 · 격분 A", "버서커 · 폭풍강타 (제스"]
def sel():
    out = []
    for name, cls, slots, arch, demon in S.SETTINGS:
        if any(name.startswith(k) for k in PICK):
            out.append((name, cls, slots, arch, demon))
    return out
def run(label):
    print(f"\n== {label}")
    for name, cls, slots, arch, demon in sel():
        p, ne = S.make_player(items, gear, cls, slots, archetype=arch)
        s1 = S.evaluate(p, ne, rows, 1.5, False, demon=demon)
        s2 = S.evaluate(p, ne, rows, 1.5, True, demon=demon)
        s1h = S.evaluate(p, ne, rows, 3.0, False, demon=demon)
        rs = [S.raid_stage(p, ne, st, 150, 1, calib, demon) for st in bosses]
        lap = S.raid_last_lap(p, ne, bosses, 150, calib, demon, S.STAGE_CAP_S)
        print(f"  {name[:34]:34s} 단일 {s1['dps']:6,.0f} | 무리 {s2['kill']:4.1f}s 여유 {min(s2['worst']['margin'],99):5.2f} | HARD 1:1 여유 {min(s1h['worst']['margin'],99):5.2f} | 레이드N 합 {sum(r['kill_s'] for r in rs):5,.0f}s 최소여유 {min(min(r['margin'] for r in rs),99):5.2f} 바퀴 {lap}")
run("지금")
S.RAID_HP_MUL = 5; S.RAID_EVADE_RETAIN = 0.5; run("⑩ 레이드 HP ×5 + 회피 ½"); S.RAID_HP_MUL = 10; S.RAID_EVADE_RETAIN = 0.25
RB.RAID_ATK_PCT = 12; run("⑪ 레이드 ATK 18 % → 12 % (HP ×10 그대로)"); RB.RAID_ATK_PCT = 18
SG.KNIGHT_SKILL_SP_MUL = 0.5; GB.CALIB["str_term"][1] = (0.85, 0.0); DB.RECOMMENDED["claw_int"] = 7.0; S.BLADE_SP, S.JUDGE_SP = 0.06, 0.04; S.DEMON_CD_S = 15.0; S.RAID_HP_MUL = 5; S.RAID_EVADE_RETAIN = 0.5
run("⑫ 나이트 ①② + 악의 ③④⑤ + 레이드 ⑩ 전부")
