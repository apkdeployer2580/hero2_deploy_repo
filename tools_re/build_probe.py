#!/usr/bin/env python3
"""세팅(빌드)별 핵심 수치 실측 (2026-10-20) — `build_balance_survey.py`(모델 v3)의 계수를 probe 로 잰다.

    python tools_re/build_probe.py <work dir> [--only 이름 ...] [--frames 1000]

class_rotation_probe.spam 과 같은 벤치(LV 90 · 의지 300 · 숲 맵 100 의 LV 21~22 몬스터, 한 마리 HP 90,000)에 아이템 효과를
`--h2-gear-wear`(GearPatch 효과 워드만 — 능력치는 벤치 그대로) · 장신구를 `--h2-acc` 로 걸고 스킬 하나를 2 프레임마다 20 초 누른다.
결과 = 시전당 피해 · 시전 간격 · SP · DPS, 범위 시험은 첫 시전에 맞은 몬스터 수. 악의 · 격분 · 반격은 창 안의 몬스터 HP 감소 합.
⚠ 능력치를 안 바꾸므로 「없음 → 세트」의 **배율**이 검증 대상이다(절대값은 벤치 캐릭터의 것).
"""

import argparse
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import class_dps_probe as CD  # noqa: E402
import class_rotation_probe as CR  # noqa: E402

PROBE = os.environ.get("WIE_PROBE", CD.PROBE)  # 별도 target 디렉터리의 probe 를 쓸 때

WIN = 720
MOB_RE = re.compile(r"bd\((\d+),(\d+)\)@\(\d+,\d+\) t(\d+) w(-?\d+) (\d+)/(\d+)")
KEYS = CR.KEYS

# 직업 인덱스 → (퀵바 값 → a.p 인덱스) : 버서커 v+3 · 나이트 v+2 · 로그 v+4
AP_OF = {0: lambda v: v + 3, 1: lambda v: v + 2, 2: lambda v: v + 4}


def bench(work, ci, stats=None, lv=90, wis=300):
    cname, src, slot, extra, slotkeys = CD.CLASSES[ci]
    ex = list(extra)
    i = ex.index("--stats")
    st = (stats or ex[i + 1]).split(",")
    st[3] = str(wis)
    ex[i + 1] = ",".join(st)
    tag = f"{cname}_w{wis}_{st[0]}_{st[4]}_L{lv}"
    dst = os.path.join(work, tag, "01035D0B", "fs")
    if not os.path.exists(dst):
        cmd = [sys.executable, "-I", CD.H2SAVE, "edit", os.path.join(CD.ROOT, src), dst, str(slot), "--level", str(lv), "--hp", "99999", "--sp", "99999"] + ex
        subprocess.run(cmd, check=True, capture_output=True)
    fs = os.path.join(work, tag)
    dry = os.path.join(work, f"dry_{tag}.log")
    if not os.path.exists(dry):
        with open(dry, "w", encoding="utf-8") as out:
            subprocess.run([PROBE, "--zip", CD.ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", "550", "--keys", slotkeys, "--gauges", "548"],
                           stdout=out, stderr=subprocess.STDOUT, check=False)
    return fs, slotkeys, CR.mob_tiles(dry, 548)


def run_case(work, name, ci, value=None, levels=(), gear=None, acc=None, acc_word=None, buffs=(), beast=None, ok=False, keep_all=False,
             frames=1000, stats=None, extra=(), key_at=None, win=WIN, mob_hp=90000, skip_cast=False, single=False, seats=(), beasts=(), wis=300):
    """하나의 probe 런. value = 칸 0 에 앉힐 퀵바 값(None 이면 안 앉힘) · levels = [(a.p 인덱스, 레벨)] · gear = 'T.I+T.I' ·
    acc = ACTIVE 코드 · buffs = [(퀵바 값)] (칸 3 에 차례로 앉혀 한 번씩) · beast = (슬롯, 종류, 레벨) · key_at = [(프레임, 키)] 추가 키."""
    fs, slotkeys, tiles = bench(work, ci, stats, wis=wis)
    keep = min(tiles, key=lambda t: abs(t[0] - 15) + abs(t[1] - 18))
    args = [PROBE, "--zip", CD.ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", str(win + frames + 20), "--gauges", "1",
            "--h2-mob-hp", f"556:{mob_hp}"]
    if not keep_all:
        for t in tiles:
            if t != keep:
                args += ["--h2-mob-hurt", f"550:{t[0]}:{t[1]}:99999"]
    for idx, lv in levels:
        args += ["--h2-skill-set", f"552:{idx}:{lv}"]
    if beast:
        args += ["--h2-beast-skill", f"552:{beast[0]}:{beast[1]}:{beast[2]}"]
    for slot, ty, lv in beasts:  # 야수 네 칸 전부
        args += ["--h2-beast-skill", f"552:{slot}:{ty}:{lv}"]
    for g_, v_ in seats:  # 퀵바 칸에 값 (f=690)
        args += ["--h2-quickbar", f"690:{g_}:{v_}"]
    if gear is not None:
        args += ["--h2-gear-wear", f"600:{gear}"]
    if acc is not None:
        args += ["--h2-acc", f"600:{acc}"]
    if acc_word:
        args += ["--h2-acc-word", f"600:{acc_word}"]
    args += list(extra)
    keys = slotkeys + f",{CD.TURN}:DOWN"
    f = 580
    for v in buffs:
        args += ["--h2-quickbar", f"{f - 4}:3:{v}"]
        keys += f",{f}:9"
        f += 30
    if value is not None:
        args += ["--h2-quickbar", f"{min(win - 20, 700)}:0:{value}"]
    for fr, k in (key_at or []):
        keys += f",{fr}:{k}"
    if single:  # 한 번만 — 일섬처럼 자리를 옮기는 스킬 (창 = frames, 누름은 창 안에서)
        keys += f",{win + 2}:1"
    elif not skip_cast:
        for fr in range(win - 10, win + frames, 2):
            keys += f",{fr}:{'OK' if ok else '1'}"
    args += ["--keys", keys]
    log = os.path.join(work, f"case_{name}.log")
    with open(log, "w", encoding="utf-8") as out:
        subprocess.run(args, stdout=out, stderr=subprocess.STDOUT, check=False)
    return parse(log, frames, win, mob_hp)


def parse(log, frames, win, mob_hp):
    prev = None
    dmg, hits = 0, 0
    first_targets = None
    casts, prev_cd, sp_costs, prev_sp = [], None, [], None
    stats = None
    per_mob_first = {}
    for line in open(log, encoding="utf-8", errors="replace"):
        if line.startswith("[h2-stats]") and stats is None:
            m = re.search(r" atk=(-?\d+) def=(-?\d+) crit=(\d+).*hpmax=(\d+) spmax=(\d+)", line)
            if m:
                stats = tuple(int(x) for x in m.groups())
        m = re.match(r"\[h2-mobs\] f=(\d+) ", line)
        if m and win <= int(m.group(1)) <= win + frames:
            cur = {}
            for k, (a, b, t, w, h, mx) in enumerate(MOB_RE.findall(line)):
                if int(mx) >= 1000 or int(h) >= 1000:
                    cur[k] = int(h)
            if prev is not None:
                drop = 0
                for k, v in cur.items():
                    if k in prev and v < prev[k]:
                        drop += prev[k] - v
                        if first_targets is None:
                            per_mob_first[k] = per_mob_first.get(k, 0) + prev[k] - v
                if drop:
                    dmg += drop
                    hits += 1
                    if first_targets is None and per_mob_first and len(casts) >= 1:
                        pass
            prev = cur
        q = re.match(r"\[quickbar\] f=(\d+) .*cd=Some\(\[([^\]]*)\]\)", line)
        if q and win <= int(q.group(1)) <= win + frames:
            c0 = int(q.group(2).split(",")[0])
            if prev_cd is not None and c0 > prev_cd + 1:
                casts.append((int(q.group(1)), c0))
                if first_targets is None and len(casts) == 2:
                    first_targets = dict(per_mob_first)
            prev_cd = c0
        g = re.match(r"\[gauges\] f=(\d+) .*sp=Some\((\d+)\)", line)
        if g and win <= int(g.group(1)) <= win + frames:
            sp = int(g.group(2))
            if prev_sp is not None and sp < prev_sp - 3:
                sp_costs.append(prev_sp - sp)
            prev_sp = sp
    secs = frames * 0.02
    gaps = [b[0] - a[0] for a, b in zip(casts, casts[1:])]
    if first_targets is None:
        first_targets = per_mob_first
    return dict(stats=stats, dmg=dmg, hits=hits, casts=len(casts), per_cast=dmg / len(casts) if casts else 0.0,
                gap_s=(sum(gaps) / len(gaps) * 0.02) if gaps else None, cd_start=casts[0][1] if casts else None,
                sp=sorted(sp_costs)[len(sp_costs) // 2] if sp_costs else 0, dps=dmg / secs, targets=len(first_targets), first=first_targets)


# ── 사례 표 ───────────────────────────────────────────────────────────────────────────────────────────────────────────
K_IAIDO = [(9, 5), (7, 5)]
K_ALL = [(3, 5), (4, 5), (5, 5), (6, 5), (7, 5), (8, 5), (9, 5), (15, 5)]
R_ALL = [(5, 5), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (15, 5), (16, 5), (17, 0)]  # 암살 0: HP 90,000 > 최대라 매 타가 「첫 타」로 ×2 가 된다
B_ALL = [(4, 5), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (11, 3), (12, 3), (14, 3), (15, 5), (16, 5)]
DEMON_STATS = "10,95,8,96,96"  # 체1의1지1 나이트 (bench_dmn)
VIT_STATS = "10,233,8,12,57"  # 체력 위주 + 2 레벨당 지력 1 (LV 90)
DEMON4 = [(0, 9, 5), (1, 3, 5), (2, 8, 5), (3, 10, 5)]  # 악의 · 지옥칼날 · 용암불 · 어둠심판
FOLLOW4 = [(0, 9, 5), (1, 2, 5), (2, 8, 5), (3, 10, 5)]  # 추종 대신
SEATS4 = [(0, 10), (1, 11), (2, 12), (3, 13)]

CASES = {
    # 나이트 — 발도
    "K_iaido_0": dict(ci=1, value=7, levels=K_ALL),
    "K_iaido_J123": dict(ci=1, value=7, levels=K_ALL, gear="2.33+8.60+5.37"),
    "K_iaido_J456": dict(ci=1, value=7, levels=K_ALL, gear="2.34+8.61+6.61"),
    "K_iaido_dmg5": dict(ci=1, value=7, levels=K_ALL, gear="2.33+8.60+5.37+6.63+9.64"),
    "K_iaido_ho_0": dict(ci=1, value=7, levels=K_ALL, buffs=(5,)),
    "K_iaido_ho_J123": dict(ci=1, value=7, levels=K_ALL, buffs=(5,), gear="2.33+8.60+5.37"),
    "K_iaido_reach": dict(ci=1, value=7, levels=K_ALL, gear="2.33+8.60", keep_all=True),
    "K_iaido_reach0": dict(ci=1, value=7, levels=K_ALL, keep_all=True),
    # 나이트 — 검기 · 방패치기 · 둔기강타
    "K_wave_0": dict(ci=1, value=2, levels=K_ALL),
    "K_wave_K": dict(ci=1, value=2, levels=K_ALL, gear="8.59+5.38+6.62+9.63"),
    "K_wave_ho_K": dict(ci=1, value=2, levels=K_ALL, buffs=(5,), gear="8.59+5.38+6.62+9.63"),
    "K_bash_0": dict(ci=1, value=4, levels=K_ALL),
    "K_bash_K": dict(ci=1, value=4, levels=K_ALL, gear="2.29+10.29+5.39+6.64+9.65+8.62"),
    "K_bash_ho_K": dict(ci=1, value=4, levels=K_ALL, buffs=(5,), gear="2.29+10.29+5.39+6.64+9.65+8.62"),
    "K_blunt_0": dict(ci=1, value=3, levels=K_ALL, extra=("--h2-weapon-type", "560:4")),
    "K_blunt_E": dict(ci=1, value=3, levels=K_ALL, extra=("--h2-weapon-type", "560:4"), gear="4.35+9.62+8.53+4.36"),
    "K_blunt_reach": dict(ci=1, value=3, levels=K_ALL, extra=("--h2-weapon-type", "560:4"), gear="4.35+9.62", keep_all=True),
    "K_blunt_reach0": dict(ci=1, value=3, levels=K_ALL, extra=("--h2-weapon-type", "560:4"), keep_all=True),
    "K_basic": dict(ci=1, levels=K_ALL, ok=True),
    # 로그 — 일섬 · 극사 · 살상연희 · 일풍격
    "R_ilgyeok_0": dict(ci=2, value=1, levels=R_ALL),
    "R_ilseom_0": dict(ci=2, value=1, levels=R_ALL, acc=5, single=True, frames=120),
    "R_ilseom_set": dict(ci=2, value=1, levels=R_ALL, acc=5, acc_word="1940:1", gear="12.19+3.39+9.52", single=True, frames=120),
    "R_ilseom_shoes": dict(ci=2, value=1, levels=R_ALL, acc=5, gear="9.52", single=True, frames=120),
    "R_ilseom_I1": dict(ci=2, value=1, levels=R_ALL, acc=5, acc_word="1940:1", gear="12.19+3.39", single=True, frames=120),
    "R_geuksa_0": dict(ci=2, value=3, levels=R_ALL),
    "R_geuksa_acc": dict(ci=2, value=3, levels=R_ALL, acc=6),
    "R_sl_0": dict(ci=2, value=6, levels=R_ALL),
    "R_sl_D": dict(ci=2, value=6, levels=R_ALL, gear="3.35+5.36+6.59+8.58+9.59"),
    "R_sl_D8": dict(ci=2, value=6, levels=R_ALL, gear="3.35+5.36+6.59+8.58+9.59+12.24"),
    "R_sl_D1": dict(ci=2, value=6, levels=R_ALL, gear="3.35"),
    "R_sl_D2": dict(ci=2, value=6, levels=R_ALL, gear="3.36"),
    "R_sl_Darmor": dict(ci=2, value=6, levels=R_ALL, gear="5.36+6.59+8.58+9.59"),
    "R_sl_D6": dict(ci=2, value=6, levels=R_ALL, gear="8.58"),
    "R_ilpung_0": dict(ci=2, value=7, levels=R_ALL),
    "R_ilpung_E": dict(ci=2, value=7, levels=R_ALL, gear="9.60+3.38"),
    "R_basic": dict(ci=2, levels=R_ALL, ok=True),
    "R_basic_venom": dict(ci=2, levels=R_ALL, ok=True, buffs=(4,)),
    # 버서커 — 폭풍강타 · 격분 · 반격
    "B_storm_0": dict(ci=0, value=6, levels=B_ALL),
    "B_storm_G": dict(ci=0, value=6, levels=B_ALL, gear="1.29+8.53"),
    "B_basic": dict(ci=0, levels=B_ALL, ok=True),
    "B_basic_rage": dict(ci=0, levels=B_ALL, ok=True, buffs=(4,)),
    "B_fury": dict(ci=0, levels=B_ALL, ok=True, buffs=(9,), extra=("--h2-fury-gauge", "705:100")),
    "B_fury_A5": dict(ci=0, levels=B_ALL, ok=True, buffs=(9,), extra=("--h2-fury-gauge", "705:100"), gear="0.35"),
    "B_counter": dict(ci=0, levels=B_ALL, buffs=(5,), skip_cast=True),
    "B_counter_G": dict(ci=0, levels=B_ALL, buffs=(5,), skip_cast=True, gear="0.36+8.55"),
    # 악의 (체1의1지1 나이트) — 할퀴기 OK 연타
    "D_claw_0": dict(ci=1, stats=DEMON_STATS, levels=K_ALL, beast=(0, 9, 5), value=10, key_at=[(720, "1")], ok=True, win=950, frames=650),
    "D_claw_C": dict(ci=1, stats=DEMON_STATS, levels=K_ALL, beast=(0, 9, 5), value=10, key_at=[(720, "1")], ok=True, win=950, frames=650, gear="8.57+6.58"),
    "D_basic_0": dict(ci=1, stats=DEMON_STATS, levels=K_ALL, ok=True),
    # ⛔ 변신(STATE 1, ≈ 60 게임 틱 = 180 probe 프레임) 중의 야수 키는 배포본 스킬로 나간다 — 토글은 STATE 2 뒤(f ≥ 1050)에 누른다
    # ★ 2026-10-20 사용자: 「악의 + 토글형 + 기본공격 DPS 가 낮을 리 없다」 — 체력 위주 · 2 레벨당 지력 1 나이트(LV 90: 힘 10 · 체 233 · 민 8 · 의 12 · 지 57)
    #   야수 = 악의(0) · 지옥칼날(1 = 암살자 자리) · 용암불(2 = 지옥불 자리) · 어둠심판(3 = 심판번개 자리) · 퀵바 10..13 · 키 1/3/7/9 · 악의 뒤 토글 둘 + 용암불 → OK 13 초
    "V_basic": dict(ci=1, stats=VIT_STATS, levels=K_ALL, ok=True),
    "V_claw": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1")], ok=True, win=1120, frames=650, mob_hp=400000),
    "V_blade": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1"), (1050, "3")], ok=True, win=1120, frames=650, mob_hp=400000),
    "V_blade_judge": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9")], ok=True, win=1120, frames=650, mob_hp=400000),
    "V_all": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9"), (1090, "7")], ok=True, win=1120, frames=650, mob_hp=400000),
    "V_all_gear": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9"), (1090, "7")], ok=True, win=1120, frames=650, mob_hp=400000, gear="8.57+6.58"),
    "V_all_gear_x2": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9"), (1090, "7")], ok=True, win=1120, frames=650, mob_hp=400000, gear="8.57+6.58", acc=3),
    "V_follow_all_gear_x2": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=FOLLOW4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9"), (1090, "7")], ok=True, win=1120, frames=650, mob_hp=400000, gear="8.57+6.58", acc=3),
    "V_all_gear_x2_w12": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9"), (1090, "7")], ok=True, win=1120, frames=650, mob_hp=400000, gear="8.57+6.58", acc=3, wis=12),
    "V_follow_all_gear_x2_w12": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=FOLLOW4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9"), (1090, "7")], ok=True, win=1120, frames=650, mob_hp=400000, gear="8.57+6.58", acc=3, wis=12),
    "V_toggles_only": dict(ci=1, stats=VIT_STATS, levels=K_ALL, beasts=DEMON4, seats=SEATS4, key_at=[(720, "1"), (1050, "3"), (1070, "9"), (1090, "7")], skip_cast=True, win=1120, frames=650, mob_hp=400000, acc=3),
    # 야수 지옥불 · 암살자 (나이트 벤치) · 야수조련
    "Be_ring": dict(ci=1, value=10, beast=(0, 8, 5)),
    "Be_ring_x2": dict(ci=1, value=10, beast=(0, 8, 5), acc=3),
    "Be_ass": dict(ci=1, value=10, beast=(0, 3, 5)),
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("--only", nargs="*", default=None)
    ap.add_argument("--frames", type=int, default=1000)
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    os.makedirs(a.work, exist_ok=True)
    names = a.only or list(CASES)
    for name in names:
        c = dict(CASES[name])
        if "frames" not in c:
            c["frames"] = a.frames
        r = run_case(a.work, name, **c)
        print(f"{name:16s} atk/def/crit/hp/sp={r['stats']} casts={r['casts']:3d} per_cast={r['per_cast']:8.0f} gap={r['gap_s']} cd0={r['cd_start']} "
              f"sp={r['sp']} dps={r['dps']:.0f} hits={r['hits']} targets={r['targets']} first={r['first']}", flush=True)


if __name__ == "__main__":
    main()
