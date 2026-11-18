#!/usr/bin/env python3
"""세 직업의 실전 회전 실측 (2026-10-14) — 버프를 켜고, 공격 스킬 넷을 퀵바에 앉혀 OK 와 섞어 누른 20 초 동안 한 마리에게 준 피해.
skill_gear_balance.py 의 스킬 회전(쿨 · SP · 시전 시간 · 배율)을 같은 캐릭터로 대조하려는 도구다.

    python tools_re/class_rotation_probe.py <work dir> [--plan name ...] [--frames 1000]

세이브 = class_dps_probe.py 와 같은 LV 90 캐릭터(그 도구가 work dir 에 만든 것을 다시 만든다). 숲에서 아래 칸 (15,18) 쪽의 한 마리만 남기고
나머지는 f=550 에 눕힌다(≈1,300 프레임 뒤 되살아나므로 창은 그 전). 남은 한 마리 HP = 90,000.

키 규칙: OK 는 짝수 프레임마다, 스킬 키(1 · 3 · 7 · 9)는 홀수 프레임에 돌아가며 — 쿨 중이면 게임이 무시하고, 콤보 중이면 다음 단으로 잇는다.
측정 = 그 한 마리의 HP 감소(초당) · 시전 수(퀵바 카운터가 다시 올라간 횟수) · SP.
"""

import argparse
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import class_dps_probe as CD  # noqa: E402

START_WIN = 720
CYCLE = 24  # 스킬 한 번 + 콤보의 한 순환(probe 프레임)

# 계획: (이름, 직업 인덱스(CD.CLASSES), 스킬 레벨 [(a.p 인덱스, 레벨)], 버프 단계 [(퀵바 칸, 값)], 공격 단계 [(퀵바 칸, 값)], 야수 [(칸, 종류, 레벨)])
PLANS = {
    # 지속 회전 — 각성기는 빼고(40 초 쿨이라 20 초 창에 한 번 들어가면 그것이 지배한다) 따로 잰다
    "버서커": (0, [(4, 5), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (15, 5)],
            [(0, 4), (1, 5), (2, 8)], [(0, 6), (1, 1), (2, 7)], []),
    "버서커-격분": (0, [(4, 5), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (15, 5), (16, 5)],
              [(0, 4), (1, 5), (2, 8), (3, 9)], [(0, 6), (1, 1), (2, 7)], []),
    "버서커-기본": (0, [], [], [], []),
    "나이트": (1, [(3, 5), (4, 5), (6, 5), (7, 5), (8, 5), (9, 5)],
            [(0, 5), (1, 6)], [(0, 2), (1, 4), (2, 7)], []),
    "나이트-천공": (1, [(4, 5), (7, 5), (15, 5)], [(0, 5)], [(0, 8)], []),
    "나이트-기본": (1, [], [], [], []),
    "로그": (2, [(5, 5), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (15, 5)],
           [(0, 4)], [(0, 5), (1, 6), (2, 7), (3, 1)], []),
    "로그-개화": (2, [(8, 5), (10, 5), (16, 5)], [(0, 4)], [(0, 8)], []),
    "로그-기본": (2, [], [], [], []),
    # 야수 = 나이트 캐릭터, 야수 링(지옥불 8) · 강공격(학살화염 11) Lv5 만 + OK
    "야수": (1, [], [], [(2, 12), (3, 13)], [(2, 8, 5), (3, 11, 5)]),
}
KEYS = {0: "1", 1: "3", 2: "7", 3: "9"}


def mob_tiles(log, frame):
    pat = re.compile(r"bd\((\d+),(\d+)\)@\(\d+,\d+\) t(\d+) w(-?\d+) (\d+)/(\d+)")
    for line in open(log, encoding="utf-8", errors="replace"):
        if line.startswith(f"[h2-mobs] f={frame} "):
            return [(int(a), int(b)) for a, b, *_ in pat.findall(line)]
    return []


def run(work, name, frames):
    ci, levels, buffs, attacks, beast = PLANS[name]
    cname, src, slot, extra, slotkeys = CD.CLASSES[ci]
    fs = CD.build(work, cname, src, slot, extra)
    dry = os.path.join(work, f"rot_{name}_dry.log")
    with open(dry, "w", encoding="utf-8") as out:
        subprocess.run([CD.PROBE, "--zip", CD.ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", "550", "--keys", slotkeys,
                        "--gauges", "548"], stdout=out, stderr=subprocess.STDOUT, check=False)
    tiles = mob_tiles(dry, 548)
    keep = min(tiles, key=lambda t: abs(t[0] - 15) + abs(t[1] - 18))
    args = [CD.PROBE, "--zip", CD.ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", str(START_WIN + frames + 20), "--gauges", "1",
            "--h2-mob-hp", "556:90000"]
    for t in tiles:
        if t != keep:
            args += ["--h2-mob-hurt", f"550:{t[0]}:{t[1]}:99999"]
    for i, lv in levels:
        args += ["--h2-skill-set", f"552:{i}:{lv}"]
    for s, ty, lv in beast:
        args += ["--h2-beast-skill", f"552:{s}:{ty}:{lv}"]
    keys = slotkeys + f",{CD.TURN}:DOWN"
    f = 570
    for g, v in buffs:
        args += ["--h2-quickbar", f"{f - 4}:{g}:{v}"]
        keys += f",{f}:{KEYS[g]}"
        f += 30
    for g, v in attacks:
        args += ["--h2-quickbar", f"{START_WIN - 20}:{g}:{v}"]
    seat = [KEYS[g] for g, _ in attacks]
    # 순환: 한 칸 CYCLE 프레임 = 스킬 키 세 번(0 · 1 · 2) 뒤 OK 를 2 프레임마다 — 스킬을 고르고 콤보를 섞는 손. 스킬이 없으면 OK 만.
    k = 0
    for t0 in range(START_WIN - 10, START_WIN + frames, CYCLE):
        start = 0
        if seat:
            for d in (0, 1, 2):
                keys += f",{t0 + d}:{seat[k % len(seat)]}"
            k += 1
            start = 6
        for d in range(start, CYCLE, 2):
            keys += f",{t0 + d}:OK"
    args += ["--keys", keys]
    log = os.path.join(work, f"rot_{name}.log")
    with open(log, "w", encoding="utf-8") as out:
        subprocess.run(args, stdout=out, stderr=subprocess.STDOUT, check=False)
    return parse(log, keep, frames)


def parse(log, keep, frames):
    pat = re.compile(r"bd\((\d+),(\d+)\)@\(\d+,\d+\) t(\d+) w(-?\d+) (\d+)/(\d+)")
    hp_prev = None
    dmg = 0
    hits = 0
    sp = []
    cds = []
    stats = None
    for line in open(log, encoding="utf-8", errors="replace"):
        if line.startswith("[h2-stats]") and stats is None:
            m = re.search(r" atk=(-?\d+) def=(-?\d+) crit=(\d+).*hpmax=(\d+) spmax=(\d+)", line)
            if m:
                stats = tuple(int(x) for x in m.groups())
        m = re.match(r"\[h2-mobs\] f=(\d+) ", line)
        if m:
            f = int(m.group(1))
            if START_WIN <= f <= START_WIN + frames:
                hps = [int(h) for a, b, t, w, h, mx in pat.findall(line) if int(h) >= 20000]  # --h2-mob-hp 는 최대 HP 는 그대로 둔다
                cur = sum(hps)
                if hp_prev is not None and cur < hp_prev:
                    dmg += hp_prev - cur
                    hits += 1
                hp_prev = cur
        g = re.match(r"\[gauges\] f=(\d+) .*sp=Some\((\d+)\)", line)
        if g and START_WIN <= int(g.group(1)) <= START_WIN + frames and int(g.group(1)) % 100 == 0:
            sp.append(int(g.group(2)))
        q = re.match(r"\[quickbar\] f=(\d+) .*cd=Some\(\[([^\]]*)\]\)", line)
        if q and START_WIN <= int(q.group(1)) <= START_WIN + frames:
            cds.append([int(x) for x in q.group(2).replace(" ", "").split(",") if x])
    casts = [0, 0, 0, 0]
    for a, b in zip(cds, cds[1:]):
        for i in range(min(len(a), len(b), 4)):
            if b[i] > a[i] + 1:
                casts[i] += 1
    secs = frames * 0.02
    return dict(stats=stats, dps=dmg / secs, hits=hits, casts=casts, sp=sp[:6])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("--plan", nargs="*", default=list(PLANS))
    ap.add_argument("--frames", type=int, default=1000)
    ap.add_argument("--spam", action="store_true", help="스킬 하나씩 (SPAM · BEAST_SPAM)")
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    os.makedirs(a.work, exist_ok=True)
    if a.spam:
        spam_all(a.work)
        return
    for name in a.plan:
        r = run(a.work, name, a.frames)
        print(f"{name:8s} atk/def/crit/hp/sp={r['stats']} dps={r['dps']:.0f} hits={r['hits']} casts={r['casts']} sp={r['sp']}")



# ── 스킬 하나씩: 그 키만 2 프레임마다 20 초 — 시전당 피해 · 시전 간격 · 시전 시간(g.t == 3 인 프레임) · SP 한 번 ─────────────────
#   의지를 300 으로 올린 세이브(SP 가 막지 않게 — 의지는 천공의검 말고는 피해에 안 든다)
SPAM = {
    # 직업 인덱스: [(이름, 퀵바 값, a.p 인덱스)]
    0: [("휘두르기", 1, 4), ("폭풍강타", 6, 9), ("사자후", 7, 10)],
    1: [("검기", 2, 4), ("방패치기", 4, 6), ("발도", 7, 9), ("천공의검", 8, 15)],
    2: [("일격", 1, 5), ("급소가격", 3, 7), ("단검난무", 5, 9), ("살상연희", 6, 10), ("일풍격", 7, 15), ("개화", 8, 16)],
}
BEAST_SPAM = [("빛의파동", 7), ("지옥불", 8), ("암살자", 3), ("심판번개", 10), ("학살화염", 11), ("암흑폭발", 12)]


def spam(work, ci, label, value, levels, beast=None, wis=300, frames=1000, buffs=(), ok=False):
    cname, src, slot, extra, slotkeys = CD.CLASSES[ci]
    ex = list(extra)
    i = ex.index("--stats")
    st = ex[i + 1].split(",")
    st[3] = str(wis)
    ex[i + 1] = ",".join(st)
    fs = CD.build(work, f"{cname}_w{wis}", src, slot, ex)
    dry = os.path.join(work, f"spam_{cname}_dry.log")
    if not os.path.exists(dry):
        with open(dry, "w", encoding="utf-8") as out:
            subprocess.run([CD.PROBE, "--zip", CD.ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", "550", "--keys", slotkeys,
                            "--gauges", "548"], stdout=out, stderr=subprocess.STDOUT, check=False)
    tiles = mob_tiles(dry, 548)
    keep = min(tiles, key=lambda t: abs(t[0] - 15) + abs(t[1] - 18))
    args = [CD.PROBE, "--zip", CD.ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", str(START_WIN + frames + 20), "--gauges", "1",
            "--h2-mob-hp", "556:90000", "--h2-quickbar", f"{START_WIN - 20}:0:{value}"]
    for t in tiles:
        if t != keep:
            args += ["--h2-mob-hurt", f"550:{t[0]}:{t[1]}:99999"]
    for idx, lv in levels:
        args += ["--h2-skill-set", f"552:{idx}:{lv}"]
    if beast:
        args += ["--h2-beast-skill", f"552:0:{beast[0]}:{beast[1]}"]
    keys = slotkeys + f",{CD.TURN}:DOWN"
    f = 580
    for g, v in buffs:  # 버프는 칸 3 에 앉혀 한 번씩 (창 앞)
        args += ["--h2-quickbar", f"{f - 4}:3:{v}"]
        keys += f",{f}:9"
        f += 30
    for fr in range(START_WIN - 10, START_WIN + frames, 2):
        keys += f",{fr}:{'OK' if ok else '1'}"
    args += ["--keys", keys]
    log = os.path.join(work, f"spam_{cname}_{label}.log")
    with open(log, "w", encoding="utf-8") as out:
        subprocess.run(args, stdout=out, stderr=subprocess.STDOUT, check=False)
    return spam_parse(log, frames)


def spam_parse(log, frames):
    pat = re.compile(r"bd\((\d+),(\d+)\)@\(\d+,\d+\) t(\d+) w(-?\d+) (\d+)/(\d+)")
    prev_hp, prev_cd, prev_sp = None, None, None
    dmg, casts, cast_frames, sp_costs, t3 = 0, [], 0, [], 0
    acting = False
    for line in open(log, encoding="utf-8", errors="replace"):
        m = re.match(r"\[h2-mobs\] f=(\d+) ", line)
        if m and START_WIN <= int(m.group(1)) <= START_WIN + frames:
            cur = sum(int(h) for a, b, t, w, h, mx in pat.findall(line) if int(h) >= 1000)
            if prev_hp is not None and cur < prev_hp:
                dmg += prev_hp - cur
            prev_hp = cur
        q = re.match(r"\[quickbar\] f=(\d+) .*cd=Some\(\[([^\]]*)\]\)", line)
        if q and START_WIN <= int(q.group(1)) <= START_WIN + frames:
            c0 = int(q.group(2).split(",")[0])
            if prev_cd is not None and c0 > prev_cd + 1:
                casts.append((int(q.group(1)), c0))
            prev_cd = c0
        g = re.match(r"\[gauges\] f=(\d+) .*sp=Some\((\d+)\)", line)
        if g and START_WIN <= int(g.group(1)) <= START_WIN + frames:
            sp = int(g.group(2))
            if prev_sp is not None and sp < prev_sp - 3:
                sp_costs.append(prev_sp - sp)
            prev_sp = sp
        w = re.match(r"\[h2-walk\] f=(\d+) t=(-?\d+)", line)
        if w and START_WIN <= int(w.group(1)) <= START_WIN + frames:
            if int(w.group(2)) == 3:
                t3 += 1
    secs = frames * 0.02
    gaps = [b[0] - a[0] for a, b in zip(casts, casts[1:])]
    return dict(dmg=dmg, casts=len(casts), per_cast=dmg / len(casts) if casts else 0.0, gap_s=(sum(gaps) / len(gaps) * 0.02) if gaps else None,
                cd_start=casts[0][1] if casts else None, act_s=(t3 * 0.02 / len(casts)) if casts else None, sp=sorted(sp_costs)[len(sp_costs) // 2] if sp_costs else 0, dps=dmg / secs)


def spam_all(work):
    out = []
    for ci, skills in SPAM.items():
        lv_all = [(idx, 5) for _, _, idx in skills]
        for label, value, idx in skills:
            r = spam(work, ci, label, value, lv_all)
            out.append((CD.CLASSES[ci][0], label, r))
            print(f"{CD.CLASSES[ci][0]} {label:6s} casts={r['casts']:3d} per_cast={r['per_cast']:8.0f} gap={r['gap_s']} cd_start={r['cd_start']} act_s={r['act_s']} sp={r['sp']} dps={r['dps']:.0f}", flush=True)
    for label, ty in BEAST_SPAM:
        r = spam(work, 1, "야수" + label, 10, [], beast=(ty, 5))
        out.append(("야수", label, r))
        print(f"야수 {label:6s} casts={r['casts']:3d} per_cast={r['per_cast']:8.0f} gap={r['gap_s']} cd_start={r['cd_start']} sp={r['sp']} dps={r['dps']:.0f}", flush=True)
    return out


if __name__ == "__main__":
    main()
