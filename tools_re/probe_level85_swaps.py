import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

import endgame_probe as probe


def fill_equipment(work, label, fs, keys, cls, count):
    tag = re.sub(r"[^0-9A-Za-z가-힣]+", "_", label)
    build_log = Path(work, f"build_{tag}.log").read_text(encoding="utf-8")
    present = sum(int(t) <= 12 for t in re.findall(r"\[h2-inv\]\s+\[\s*\d+\] type=\s*(\d+)", build_log))
    assert 0 < present <= count, present
    slot = probe.BASE[cls][1]
    staging = str(Path(work, "fill_" + tag))
    subprocess.run([sys.executable, "-I", probe.H2SAVE, "edit", str(Path(fs, "01035D0B/fs")),
                    str(Path(staging, "01035D0B/fs")), str(slot), "--mode", "0", "--hp", "99999", "--sp", "99999"],
                   check=True, capture_output=True)
    raw = str(Path(work, "filled_raw_" + tag))
    args = [probe.PROBE, "--zip", probe.ZIP, "--fs", staging, "--w", "176", "--h", "220",
            "--ms", "20", "--frames", "900", "--keys", keys, "--h2-bags127", "1",
            "--h2-inv", "780", "--h2-save", "800", "--fs-out", raw]
    if present < count:
        args += ["--h2-give", "600:" + ",".join(["0:0:1"] * (count - present))]
    log = Path(work, "fill_" + tag + ".log")
    probe.sh(args, log)
    actual = sum(int(t) <= 12 for t in re.findall(r"\[h2-inv\]\s+\[\s*\d+\] type=\s*(\d+)", log.read_text(encoding="utf-8")))
    assert actual == count, (actual, count)
    out = str(Path(work, "filled_" + tag))
    subprocess.run([sys.executable, "-I", probe.H2SAVE, "edit", str(Path(raw, "01035D0B/fs")),
                    str(Path(out, "01035D0B/fs")), str(slot), "--mode", "3", "--hp", "99999", "--sp", "99999"],
                   check=True, capture_output=True)
    return out


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    work = "target/audit85_swaps_current"
    Path(work).mkdir(parents=True, exist_ok=True)
    pinned_probe = str(Path(work, "wie_probe.exe").resolve())
    pinned_asset = str(Path(work, "hero2_ktf.zip").resolve())
    shutil.copyfile(probe.PROBE, pinned_probe)
    shutil.copyfile(probe.ZIP, pinned_asset)
    probe.PROBE, probe.ZIP = pinned_probe, pinned_asset
    source_paths = ["wie_android/src/guest_anchors.rs", "wie_android/src/hero2java.rs", "tools_re/skill_gear.py"]
    source_hashes = {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in source_paths}
    results = []
    for base_label, replacement, label in [
        ("로그 살상연희 D", (6, 59), "현재 살상연희 무희 47개"),
        ("로그 살상연희 D", (6, 60), "현재 살상연희 중력 47개"),
    ]:
        _, prefix, buffs, attacks, demon = next(p for p in probe.PLANS if p[0] == base_label)
        _, cls, base_slots = probe.setting(prefix)
        slots = dict(base_slots, a=replacement)
        fs, keys, _ = probe.build_save(work, label, cls, slots, probe.BASE[cls][3])
        fs = fill_equipment(work, label, fs, keys, cls, 47)
        one = probe.measure(work, label, fs, keys, cls, buffs, attacks, demon, False, 1000)
        pack = probe.measure(work, label, fs, keys, cls, buffs, attacks, demon, True, 1000)
        results.append({"baseline": base_label, "label": label, "slots": slots, "single": one, "pack": pack})
        document = {
            "probe_sha256": hashlib.sha256(Path(probe.PROBE).read_bytes()).hexdigest(),
            "asset_sha256": hashlib.sha256(Path(probe.ZIP).read_bytes()).hexdigest(),
            "source_sha256": source_hashes,
            "sources_unchanged_during_run": all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == h for p, h in source_hashes.items()),
            "frozen_inputs": {"probe": pinned_probe, "asset": pinned_asset},
            "frames": 1000, "trials": 1, "level": 99, "equipment_count": 47,
            "scenario": "guardian_forest_normal_fixed_rotation",
            "limitation": "장비 47개를 양쪽 모두 확인한 갑옷 교체 비교. 1회 측정으로 반복 변동 미검증. 레이드 및 이동 검증 아님.",
            "results": results,
        }
        Path("tools_re/level85_swaps_2026-10-21.json").write_text(json.dumps(document, ensure_ascii=False, indent=2), encoding="utf-8")
        print(label, json.dumps({"single": one, "pack": pack}, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
