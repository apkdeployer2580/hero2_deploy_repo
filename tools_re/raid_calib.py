#!/usr/bin/env python3
"""보스 레이드 — 보스의 공격 박자 보정 (2026-10-17). `raid_balance.py --calib` 가 읽는 JSON 을 probe 로그에서 만든다.

    python tools_re/raid_calib.py <out.json> <log stage1> … <log stage9>

로그 = 단계마다 한 probe 런(단계 k = 0..8):
    h2save.py edit <temple85 fs> <calK fs> 2 --map 113+k --x ex --y ey --stats 10,999,8,96,96 --hp 99999 --sp 5000   (raid_gate.STAGES 의 입장 칸)
    wie_probe --fs <calK> --keys 320:OK,420:OK,470:RIGHT,520:OK --frames 3200 --gauges 1 --h2-difficulty 0:1 --h2-guardian-word 16:1
서 있는 LV 85 나이트(체력 999 · HP 13,008 · 회피 바탕 98 → 보스 앞 회피 8 %)를 레이드 보스가 친다. `--h2-guardian-word 16:1` 이 RAID_MUL 을
1/256 로 낮춰 한 타가 최소 피해 3 이 되므로 3,200 프레임 동안 쓰러지지 않는다 — 피해 크기는 따로 맞췄다(실측 단계 1 NORMAL: 한 타 1,781..2,033 ·
치명 4,652 = ATK 2,006 × U(0.9, 1.1) − DEF 57 · ×2.5, guardian_balance 의 식 그대로). 여기서 세는 것은 **맞는 간격**뿐이다:
    배율 = 45(모델이 쓰는 일반 몬스터의 공격 간격, probe 프레임) ÷ (첫 피격부터 끝까지의 프레임 ÷ 피격 수) ÷ (1 − 0.08)
곁 몬스터(고대수호물의 수호물 둘 · 오딘의 두 손)와 특수기의 피격도 같이 센다. ⚠ 플레이어가 서 있는 동안의 박자 — 맞아서 움찔하는 보스는 덜 친다(위쪽 한계).
⛔ 오딘(단계 9)은 입장 칸 (9,13) 에 서 있는 상대를 치지 않고(3,200 프레임 피격 0) 손의 피해에는 최소 3 이 없어 1/256 런에서 안 보인다 →
   단계 9 만 `--x 9 --y 11`(몸 바로 아래) + 키 `UP` 30 프레임마다(밀려나면 다시 붙는다) + `--h2-guardian-word 16:64`(×¼, 쓰러질 때까지 센다).
   쓰러지면(H2RaidAction::Escape) 창은 그 프레임에서 끝난다.
"""

import json
import re
import sys

sys.path.insert(0, __import__("os").path.dirname(__import__("os").path.abspath(__file__)))
import raid_gate as RG  # noqa: E402

MODEL_FRAMES = 45  # guardian_balance.CALIB["monster_attack_frames"]
EVADE = 0.08  # 보정 나이트의 회피(바탕 98 − 110 − e/10 < 8 → 8 %)


def parse(path):
    prev = None
    drops = []
    last = 0
    for line in open(path, encoding="utf-8", errors="replace"):
        fell = re.match(r"\[h2-raid\] f=(\d+) .* fell", line)
        if fell:  # out of the raid (H2RaidAction::Escape) — the window ends at the fall
            last = int(fell.group(1))
            break
        m = re.match(r"\[gauges\] f=(\d+) .*hp=Some\((\d+)\)/Some\((\d+)\)", line)
        if not m:
            continue
        f, hp = int(m.group(1)), int(m.group(2))
        last = f
        if prev is not None and hp < prev:
            drops.append((f, prev - hp))
        prev = hp
    return drops, last


def main():
    out_path, logs = sys.argv[1], sys.argv[2:]
    doc = {"runs": []}
    for k, log in enumerate(logs):
        drops, last = parse(log)
        if len(drops) < 3:
            print(f"stage {k + 1}: only {len(drops)} hit(s) — skipped")
            continue
        frames = last - drops[0][0]
        per = frames / len(drops)
        factor = MODEL_FRAMES / per / (1 - EVADE)
        doc[str(k + 1)] = round(factor, 3)
        doc["runs"].append(dict(stage=k + 1, boss=RG.STAGES[k][4], frames=frames, hits=len(drops), per=round(per, 1), factor=round(factor, 3)))
        print(f"stage {k + 1} {RG.STAGES[k][4]}: {len(drops)} hits / {frames} frames = every {per:.1f} → ×{factor:.2f}")
    json.dump(doc, open(out_path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
