#!/usr/bin/env python3
"""세 직업의 기본공격 박자 · 한 타 피해 실측 (2026-10-14) — skill_gear_balance.py 와 HERO2_GUARDIAN_BALANCE.md §6 의 직업 순위가
반대로 갈린 원인(기본공격 박자 가정)을 probe 로 판정한다.

    python tools_re/class_dps_probe.py <work dir> [--frames 1000]

세 세이브를 LV 90 · 모델의 능력치 배분(guardian_balance.ARCHETYPE)으로 만들어(h2save.py edit) 숲(맵 100, (15,17))에 세우고, 모든 몬스터
HP 를 30,000 으로 올린 뒤 아래 칸 (15,18) 의 몬스터를 향해 OK 를 두 박자로 누른다:
  every  = probe 프레임마다(20 ms — 게임 한 프레임은 50 ms 라 입력이 넘친다 = 실제로 낼 수 있는 가장 빠른 박자)
  every12 = 12 probe 프레임마다(= guardian_fight.sh 가 쓴 박자, §6 의 attack_frames 가 나온 곳)
창 안에서 몬스터 HP 가 줄어든 사건 = 한 타. 결과 = 초당 타수 · 한 타 평균 · 초당 피해, 그리고 [h2-stats] 의 atk.
장비는 세이브에 든 그대로(LV 26~60) — 비교는 「같은 캐릭터의 모델 예측 vs 실측」이다.
"""

import argparse
import os
import re
import statistics as st
import subprocess
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
ZIP = os.path.join(ROOT, "android_app", "app", "src", "main", "assets", "hero2_ktf.zip")
PROBE = os.path.join(ROOT, "target", "release", "wie_probe.exe")
H2SAVE = os.path.join(ROOT, "tools_re", "h2save.py")

# (이름, 원본 fs, 슬롯, h2save 추가 인수, 슬롯 고르는 키) — 능력치 = BASE_STATS + 3·89 × ARCHETYPE (guardian_balance)
CLASSES = [
    ("버서커", "dist/phone_backup/s25_20261003_1931/01035D0B/fs", 1, ["--map", "100", "--x", "15", "--y", "17", "--mode", "0", "--stats", "199,65,9,37,10"], "320:OK,420:OK,520:OK"),
    ("나이트", "dist/test_saves/01035D0B/fs", 2, ["--stats", "170,91,34,12,12"], "320:OK,420:OK,470:RIGHT,520:OK"),
    ("로그", "dist/test_saves/01035D0B/fs", 1, ["--map", "100", "--x", "15", "--y", "17", "--stats", "91,35,173,10,10"], "320:OK,420:OK,520:OK"),
]
START, TURN, OK_FROM = 560, 566, 580
MOB_RE = re.compile(r"bd\((\d+),(\d+)\)@\(\d+,\d+\) t(\d+) w(-?\d+) (\d+)/(\d+) LV(\d+) atk(\d+)")


def build(work, name, src, slot, extra):
    dst = os.path.join(work, name, "01035D0B", "fs")
    cmd = [sys.executable, "-I", H2SAVE, "edit", os.path.join(ROOT, src), dst, str(slot), "--level", "90", "--hp", "99999", "--sp", "99999"] + extra
    subprocess.run(cmd, check=True, capture_output=True)
    return os.path.join(work, name)


def run(fs, slotkeys, cadence, frames, log):
    keys = slotkeys + f",{TURN}:DOWN"
    for f in range(OK_FROM, OK_FROM + frames, cadence):
        keys += f",{f}:OK"
    cmd = [PROBE, "--zip", ZIP, "--fs", fs, "--w", "176", "--h", "220", "--ms", "20", "--frames", str(OK_FROM + frames + 20),
           "--keys", keys, "--gauges", "1", "--h2-mob-hp", f"{START}:30000"]
    with open(log, "w", encoding="utf-8") as out:
        subprocess.run(cmd, stdout=out, stderr=subprocess.STDOUT, check=False)


def parse(log, frames):
    hp = {}
    atk = None
    for line in open(log, encoding="utf-8", errors="replace"):
        if line.startswith("[h2-stats]") and atk is None:
            m = re.search(r" atk=(-?\d+) def=(-?\d+) crit=(\d+)", line)
            if m:
                atk = tuple(int(x) for x in m.groups())
        m = re.match(r"\[h2-mobs\] f=(\d+) ", line)
        if m:
            f = int(m.group(1))
            hp[f] = {(int(a), int(b), k): int(h) for k, (a, b, t, w, h, mx, lv, at) in enumerate(MOB_RE.findall(line))}
    fs = sorted(f for f in hp if OK_FROM <= f <= OK_FROM + frames)
    hits = []
    for a, b in zip(fs, fs[1:]):
        # 몬스터를 목록 순서(k)로 맞춘다 — 걸어도 같은 개체의 HP 가 줄어든 것만 센다
        ha = {k: v for (x, y, k), v in hp[a].items()}
        hb = {k: v for (x, y, k), v in hp[b].items()}
        for k, v in hb.items():
            if k in ha and v < ha[k] and ha[k] >= 20000:
                hits.append(ha[k] - v)
    secs = frames * 0.02
    return dict(atk=atk, hits=len(hits), rate=len(hits) / secs, mean=st.mean(hits) if hits else 0.0, dps=sum(hits) / secs, sample=sorted(hits)[:3] + sorted(hits)[-3:])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("--frames", type=int, default=1000)
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    os.makedirs(a.work, exist_ok=True)
    rows = []
    for name, src, slot, extra, keys in CLASSES:
        fs = build(a.work, name, src, slot, extra)
        for label, cad in (("every", 1), ("every12", 12)):
            log = os.path.join(a.work, f"{name}_{label}.log")
            run(fs, keys, cad, a.frames, log)
            r = parse(log, a.frames)
            rows.append((name, label, r))
            print(f"{name} {label:8s} atk/def/crit={r['atk']} hits={r['hits']} rate={r['rate']:.2f}/s mean={r['mean']:.0f} dps={r['dps']:.0f} sample={r['sample']}")
    return rows


if __name__ == "__main__":
    main()
