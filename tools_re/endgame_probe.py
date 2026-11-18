#!/usr/bin/env python3
"""LV99 주요 세팅의 고정 위치 전투 측정.

    python tools_re/endgame_probe.py <work dir> --frames 1000 --json <results.json>

최대 패시브/콤보, 제련 없는 지정 장비, 가디언 숲의 NORMAL 몬스터를 사용한다.
준비 중 적 공격을 억제하고 시작 시 플레이어 HP/SP와 적 공격을 복구한다.
단일 대상은 준비 중 HP 400,000을 지정한 뒤 게임의 난이도 보정이 안정된 값을
측정한다. 무리는 원래 8마리다. 개체별 HP를 추적하며 교체/재초기화는 무효 처리한다.
사망 또는 최초 대상 전멸까지 측정하므로 DPS만으로 생존/맵핑 우열을 판정하지 않는다.
이동, 레이드 보스 패턴, 악의 재사용 대기시간 전체 주기는 별도 검증이 필요하다.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import build_balance_survey as BS  # noqa: E402
import class_dps_probe as CD  # noqa: E402
import class_rotation_probe as CR  # noqa: E402

PROBE = os.environ.get("WIE_PROBE", CD.PROBE)
ROOT = CD.ROOT
H2SAVE = CD.H2SAVE
ZIP = CD.ZIP
KEYS = CR.KEYS
EQUIP_SLOT = {"g": 0, "h": 1, "acc": 2, "w": 3, "a": 4, "sh": 5, "w2": 5, "s": 6}
MOB_RE = re.compile(r"bd\((\d+),(\d+)\)@\(\d+,\d+\) t(\d+) w(-?\d+) (\d+)/(\d+)")

# 직업 → (원본 fs, 슬롯, 고르는 키, LV 99 원형 능력치)
BASE = {
    0: ("dist/phone_backup/s25_20261003_1931/01035D0B/fs", 1, "320:OK,420:OK,520:OK", "219,71,9,40,10"),
    1: ("dist/test_saves/01035D0B/fs", 2, "320:OK,420:OK,470:RIGHT,520:OK", "186,99,37,12,12"),
    2: ("dist/test_saves/01035D0B/fs", 1, "320:OK,420:OK,520:OK", "99,38,189,10,10"),
}
BEAST_STATS = "10,256,8,12,61"  # 체 5/6 · 지 1/6 (사용자 빌드)
BEAST111_STATS = "10,109,8,110,110"
K_ALL = [(3, 5), (4, 5), (5, 5), (6, 5), (7, 5), (8, 5), (9, 5), (15, 5)]
R_ALL = [(5, 5), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (15, 5), (16, 5), (17, 5)]
B_ALL = [(4, 5), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (11, 3), (12, 3), (14, 3), (15, 5), (16, 5)]
LEVELS = {0: B_ALL, 1: K_ALL, 2: R_ALL}
BASE_SKILLS = {
    0: [1, 2, 3, 4, 5, 0, 5, 5, 5, 5, 5, 3, 3, 1, 3],
    1: [1, 2, 3, 5, 5, 5, 5, 5, 5, 5, 1, 3, 3, 3, 3],
    2: [1, 2, 3, 4, 5, 5, 5, 5, 5, 5, 5, 3, 1, 3, 3],
}
DEMON4 = [(0, 9, 5), (1, 2, 5), (2, 8, 5), (3, 10, 5)]  # 악의 · 추종 · 용암불 · 어둠심판
SEATS4 = [(0, 10), (1, 11), (2, 12), (3, 13)]

# 실측할 세팅: (라벨, SETTINGS 이름 머리, 버프 [퀵바 값], 공격 [퀵바 값], 악의?)
PLANS = [
    # 버서커: 대검 = 가르기(3) · 도끼 = 휘두르기(1) · 폭풍강타(6). 사자후(7)는 뺀다 — 400,000 HP 몬스터엔 즉사가 안 되고 SP 80 % 를 먹어 회전을 말린다
    ("버서커 1차", "버서커 · 1차 가디언", [4, 5, 8], [6, 3], False),
    ("버서커 격분 A", "버서커 · 격분 A", [4, 5, 8, 9], [6, 3], False),
    ("버서커 폭풍강타", "버서커 · 폭풍강타 (제스", [4, 5, 8], [6, 1], False),
    ("버서커 반격태세", "버서커 · 반격태세 G", [4, 5, 8], [6, 3], False),
    ("나이트 1차", "나이트 · 1차 가디언", [5, 6], [2, 4, 7], False),
    ("나이트 검기 K", "나이트 · 검기 (", [5, 6], [2, 4, 7], False),
    ("나이트 방패치기", "나이트 · 방패치기 (", [5, 6], [4, 2, 7], False),
    ("나이트 발도 범위", "나이트 · 발도 범위", [5, 6], [7, 2, 4], False),
    ("나이트 발도 SP쿨", "나이트 · 발도 SP", [5, 6], [7, 2, 4], False),
    ("나이트 발도 쌍월", "나이트 · 발도 쌍월", [5, 6], [7, 7, 2, 4], False),
    ("로그 1차", "로그 · 1차 가디언", [4], [6, 1, 3, 5], False),
    ("로그 살상연희 D", "로그 · 살상연희 (", [4], [6, 1, 3, 5], False),
    ("로그 일섬", "로그 · 일섬", [4], [1, 6, 3, 5], False),
    ("야수나이트 체5/6 악의+기본공격 세팅", "야수나이트 · 체5/6 지1/6 · 데몬 + 기본공격", [], [], True),
    ("야수나이트 체1의1지1 악의+기본공격 세팅", "야수나이트 · 체1의1지1 · 데몬 + 기본공격", [], [], True),
]


def setting(prefix):
    for name, cls, slots, arch, demon in BS.SETTINGS:
        if name.startswith(prefix):
            return name, cls, slots
    raise KeyError(prefix)


def sh(cmd, log):
    with open(log, "w", encoding="utf-8") as out:
        subprocess.run(cmd, stdout=out, stderr=subprocess.STDOUT, check=True)


def build_save(work, label, cls, slots, stats):
    """① 세이브 — 반환 = 내보낸 fs 루트."""
    src, slot, keys, _ = BASE[cls]
    tag = re.sub(r"[^0-9A-Za-z가-힣]+", "_", label)
    edited = os.path.join(work, "edit_" + tag, "01035D0B", "fs")
    # 장비는 NORMAL(모드 0)의 숲에서 입히고 저장한다 — 가디언 모드(LV 86 · 공격 1,035)에서는 저장 전에 쓰러진다. 모드 3 · 야수 LV 99 는 내보낸 뒤에
    extra = ["--level", "99", "--stats", stats, "--hp", "99999", "--sp", "99999", "--mode", "0", "--points", "0"]
    extra += ["--skills", ",".join(map(str, BASE_SKILLS[cls])), "--combo", str(BS.GB.STAGES[cls])]
    if slot == 1:
        extra += ["--map", "100", "--x", "15", "--y", "17"]
    subprocess.run([sys.executable, "-I", H2SAVE, "edit", os.path.join(ROOT, src), edited, str(slot)] + extra, check=True, capture_output=True)
    fs = os.path.join(work, "edit_" + tag)
    # 가방의 현재 개수
    dry = os.path.join(work, f"dry_{tag}.log")
    sh([PROBE, "--zip", ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", "620", "--keys", keys, "--h2-inv", "600"], dry)
    n = None
    for line in open(dry, encoding="utf-8", errors="replace"):
        m = re.match(r"\[h2-inv\] f=600 bag a .* items=(\d+)", line)
        if m:
            n = int(m.group(1))
    if n is None:
        raise RuntimeError(f"{label}: no inventory in the dry run ({dry})")
    give = ",".join(f"{t}:{i}:1" for s_, (t, i) in slots.items())
    wear = ",".join(f"{n + k}:{EQUIP_SLOT[s_]}" for k, s_ in enumerate(slots))
    out = os.path.join(work, "fs_" + tag)
    log = os.path.join(work, f"build_{tag}.log")
    raw = os.path.join(work, "raw_" + tag)
    sh([PROBE, "--zip", ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", "900", "--keys", keys, "--h2-bags127", "1",
        "--h2-give", f"600:{give}", "--h2-wear", f"760:{wear}", "--h2-inv", "780", "--h2-save", "800", "--fs-out", raw], log)
    worn = [l.strip() for l in open(log, encoding="utf-8", errors="replace") if "WORN" in l or l.startswith("[h2-wear]") or l.startswith("[h2-save]")]
    # 내보낸 세이브를 가디언 모드 · 야수 LV 99 로
    subprocess.run([sys.executable, "-I", H2SAVE, "edit", os.path.join(raw, "01035D0B", "fs"), os.path.join(out, "01035D0B", "fs"), str(slot),
                    "--mode", "3", "--beast-level", "99", "--hp", "99999", "--sp", "99999"], check=True, capture_output=True)
    return out, keys, worn


def measure(work, label, fs, keys, cls, buffs, attacks, demon, pack, frames):
    tag = re.sub(r"[^0-9A-Za-z가-힣]+", "_", label) + ("_pack" if pack else "_one")
    log = os.path.join(work, f"run_{tag}.log")
    dry = os.path.join(work, f"mobs_{tag}.log")
    sh([PROBE, "--zip", ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", "560", "--keys", keys, "--gauges", "548"], dry)
    tiles = CR.mob_tiles(dry, 548)
    win = 1120
    args = [PROBE, "--zip", ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", str(win + frames + 20), "--gauges", "1",
            "--h2-difficulty", "0:1", "--h2-benchmark", f"{win}:{win + frames}"]
    if not pack:
        keep = min(tiles, key=lambda t: abs(t[0] - 15) + abs(t[1] - 18)) if tiles else None
        for t in tiles:
            if t != keep:
                args += ["--h2-mob-hurt", f"550:{t[0]}:{t[1]}:200000000"]
        args += ["--h2-mob-hp", f"{win - 20}:400000"]
    for idx, lv in LEVELS[cls]:
        args += ["--h2-skill-set", f"552:{idx}:{lv}"]
    k = keys + f",{CD.TURN}:DOWN"
    if demon:
        for slot, ty, lv in DEMON4:
            args += ["--h2-beast-skill", f"552:{slot}:{ty}:{lv}"]
        for g_, v_ in SEATS4:
            args += ["--h2-quickbar", f"690:{g_}:{v_}"]
        k += ",720:1,1050:3,1070:9,1090:7"
        for fr in range(win, win + frames, 2):
            k += f",{fr}:OK"
        for fr in range(win + 80, win + frames, 90):
            k += f",{fr + 1}:7"
    else:
        f = 960
        for v in buffs:
            args += ["--h2-quickbar", f"{f - 4}:3:{v}"]
            k += f",{f}:9"
            f += 30
        for g_, v in enumerate(attacks[:4]):
            args += ["--h2-quickbar", f"{win - 20}:{g_}:{v}"]
        seat = [KEYS[g_] for g_ in range(len(attacks[:4]))]
        kk = 0
        for t0 in range(win, win + frames, CR.CYCLE):
            for d in (0, 1, 2):
                k += f",{t0 + d}:{seat[kk % len(seat)]}"
            kk += 1
            for d in range(6, CR.CYCLE, 2):
                k += f",{t0 + d}:OK"
    args += ["--keys", k]
    sh(args, log)
    return parse(log, win, frames, single=not pack)


def parse(log, win, frames, single=True):
    samples = {}
    actors = {}
    stats = None
    for line in Path(log).read_text(encoding="utf-8", errors="replace").splitlines():
        m = re.match(r"\[h2-bench\] f=(\d+) hp=(-?\d+) hpmax=(\d+) sp=(-?\d+) spmax=(\d+)", line)
        if m:
            f, hp, maximum, sp, spmax = map(int, m.groups())
            if win <= f <= win + frames:
                samples[f] = (hp, maximum, sp, spmax)
        m = re.match(r"\[h2-bench-mob\] f=(\d+) id=(\d+) template=(\d+) state=(\d+) hp=(-?\d+) max=(\d+)", line)
        if m:
            f, actor, template, state, hp, maximum = map(int, m.groups())
            actors.setdefault(f, {})[actor] = (template, state, hp, maximum)
        m = re.search(r"\[h2-stats\].* atk=(-?\d+) def=(-?\d+) crit=(\d+).*hpmax=(\d+) spmax=(\d+)", line)
        if m and not samples:
            stats = tuple(map(int, m.groups()))
    if len(samples) != frames + 1 or win not in samples or win + frames not in samples:
        raise ValueError("Incomplete benchmark telemetry; rebuild wie_probe and rerun")
    hp0, hpmax, _, _ = samples[win]
    if hp0 <= 0 or hp0 != hpmax:
        raise ValueError("Benchmark did not start at full HP")
    initial = {h: v for h, v in actors.get(win, {}).items() if v[1] < 6 and v[2] > 0}
    if not initial or (single and len(initial) != 1):
        raise ValueError(f"Unexpected initial target count: {len(initial)}")
    alive = set(initial)
    prev = initial
    damage = kills = 0
    died = clear_s = None
    resets = missing = 0
    hp_min = hp0
    for f in range(win + 1, win + frames + 1):
        hp_min = min(hp_min, samples[f][0])
        current = actors.get(f, {})
        for h in list(alive):
            old = prev.get(h)
            new = current.get(h)
            if new is None:
                missing += 1
                alive.remove(h)
                continue
            if old is None:
                continue
            if new[0] != old[0] or new[3] != old[3] or new[2] > old[2]:
                resets += 1
                alive.remove(h)
                continue
            damage += max(0, old[2] - max(0, new[2]))
            if new[1] >= 6 or new[2] <= 0:
                kills += 1
                alive.remove(h)
        if not alive and clear_s is None and kills == len(initial):
            clear_s = (f - win) * 0.02
        prev = current
        if samples[f][0] <= 0:
            died = (f - win) * 0.02
            break
        if clear_s is not None:
            break
    elapsed = died or clear_s or frames * 0.02
    return dict(stats=stats, dps=damage / elapsed, window_dps=damage / (frames * 0.02), dmg=damage, kills=kills,
                hp0=hp0, hp_min=hp_min, died=died, clear_s=clear_s,
                elapsed_s=elapsed, targets=len(initial), resets=resets, missing=missing,
                valid=resets == 0 and missing == 0,
                mobs=[(v[2], v[3]) for v in initial.values()], log=str(log))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("--only", nargs="*", default=None)
    ap.add_argument("--frames", type=int, default=1000)
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--json", default=None)
    a = ap.parse_args()
    if a.frames <= 0:
        ap.error("--frames must be positive")
    sys.stdout.reconfigure(encoding="utf-8")
    os.makedirs(a.work, exist_ok=True)
    results = []
    for label, prefix, buffs, attacks, demon in PLANS:
        if a.only and label not in a.only:
            continue
        name, cls, slots = setting(prefix)
        stats = BEAST_STATS if "체5/6" in label else BEAST111_STATS if "체1의1지1" in label else BASE[cls][3]
        tag = re.sub(r"[^0-9A-Za-z가-힣]+", "_", label)
        fs = os.path.join(a.work, "fs_" + tag)
        keys = BASE[cls][2]
        if not a.skip_build or not os.path.isdir(fs):
            fs, keys, worn = build_save(a.work, label, cls, slots, stats)
            print(f"[build] {label}: " + " | ".join(w[:60] for w in worn if w.startswith("[h2-wear]")), flush=True)
        r1 = measure(a.work, label, fs, keys, cls, buffs, attacks, demon, False, a.frames)
        r2 = measure(a.work, label, fs, keys, cls, buffs, attacks, demon, True, a.frames)
        results.append([label, r1, r2])
        if a.json:
            with open(a.json, "w", encoding="utf-8") as out:
                json.dump(dict(probe_sha256=hashlib.sha256(Path(PROBE).read_bytes()).hexdigest(),
                               frames=a.frames, trials_per_build=1, scenario="guardian_forest_normal_fixed_rotation",
                               results=results), out, ensure_ascii=False, indent=2)
        print(f"{label:28s} atk/def/crit/hp/sp={r1['stats']} | valid(single/pack)={r1['valid']}/{r2['valid']} | 단일 dps={r1['dps']:7,.0f} | 무리 dmg={r2['dmg']:8,} kills={r2['kills']} hp {r2['hp0']}→min {r2['hp_min']} died={r2['died']} mobs={r2['mobs'][:2] if r2['mobs'] else None}", flush=True)


if __name__ == "__main__":
    main()
