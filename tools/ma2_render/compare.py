"""Hold the shipping MA-2 parser's reading of every song against an independent one.

    python tools/ma2_render/compare.py <dump dir> <tools dir>

<dump dir>  = the `.ma2.txt` files `ma2_render dump --out` wrote (the Rust parser).
<tools dir> = the 2026-09-30 audit's `hps.py` / `voices.py` (dist/bgm_audit/tools), whose
              reading was checked note for note against the app's own `smaf_player`.

Compared per song, as multisets: every key-on and key-off (time, channel, note), every
program change with the bank in force, volume, expression and pan, every voice (bank,
program, bytes) in order, and the song's length. Exit 1 on any difference.
"""
import os
import sys
from collections import defaultdict


def rust_reading(path):
    notes, ctl, voices, length = [], [], [], None
    for line in open(path, encoding="utf-8"):
        f = line.split()
        if f[0] == "len":
            length = int(f[1])
        elif f[0] == "voice":
            voices.append((int(f[1]), int(f[2]), f[3]))
        elif f[0] in ("on", "off"):
            notes.append((f[0], *map(int, f[1:])))
        elif f[0] == "prog":
            ctl.append(("prog", *map(int, f[1:])))
        elif f[0] in ("vol", "expr", "pan"):
            ctl.append((f[0], *map(int, f[1:])))
    return sorted(notes), sorted(ctl), voices, length


def python_reading(hps, voices_mod, data):
    tracks = hps.parse_file(data)
    issues = []
    notes_raw, ctl_raw = hps.midi_events(tracks, issues)
    notes = []
    for on, off, ch, n, _prog in notes_raw:
        notes.append(("on", on, ch, n))
        notes.append(("off", off, ch, n))
    bank = defaultdict(int)
    ctl = []
    for t, ch, kind, value in ctl_raw:
        if kind == "bank":
            bank[ch] = value
        elif kind == "prog":
            ctl.append(("prog", t, ch, bank[ch], value))
        elif kind in ("vol", "pan"):
            ctl.append((kind, t, ch, value & 0x7F))
        elif kind in ("expr", "expr_s"):
            ctl.append(("expr", t, ch, value & 0x7F))
    voices = []
    for tr in tracks:
        if "Mtsu" in tr["sub"]:
            pays, bad = voices_mod.parse_setup(tr["sub"]["Mtsu"])
            if bad:
                issues.append(f"setup parse stopped at {bad}")
            for p in pays:
                voices.append((p[3], p[4], p[5:17].hex()))
    length = max(tr.get("end_ms") or 0 for tr in tracks)
    return sorted(notes), sorted(ctl), voices, length, issues


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    dump_dir, tools_dir = sys.argv[1], sys.argv[2]
    sys.path.insert(0, tools_dir)
    import hps  # noqa: E402
    import voices as voices_mod  # noqa: E402

    files = hps.load_files()
    by_number = {name.split("/")[-1]: data for name, data in files.items()}
    bad = 0
    total = defaultdict(int)
    for dump in sorted(os.listdir(dump_dir)):
        if not dump.endswith(".ma2.txt"):
            continue
        song = dump[: -len(".ma2.txt")]
        r_notes, r_ctl, r_voices, r_len = rust_reading(os.path.join(dump_dir, dump))
        p_notes, p_ctl, p_voices, p_len, issues = python_reading(hps, voices_mod, by_number[song])
        diffs = []
        if r_notes != p_notes:
            diffs.append(f"notes differ ({len(r_notes)} vs {len(p_notes)} key events)")
        if r_ctl != p_ctl:
            only_r = sorted(set(r_ctl) - set(p_ctl))[:5]
            only_p = sorted(set(p_ctl) - set(r_ctl))[:5]
            diffs.append(f"controls differ: rust-only {only_r} python-only {only_p}")
        if r_voices != p_voices:
            diffs.append("voices differ")
        if r_len != p_len:
            diffs.append(f"length {r_len} vs {p_len}")
        if issues:
            diffs.append(f"python issues {issues[:3]}")
        total["songs"] += 1
        total["key events"] += len(r_notes)
        total["controls"] += len(r_ctl)
        total["voices"] += len(r_voices)
        if diffs:
            bad += 1
            print(f"{song}: DIFFERENT — " + "; ".join(diffs))
        else:
            print(f"{song}: same — {len(r_notes) // 2} notes, {len(r_ctl)} controls, {len(r_voices)} voices, {r_len} ms")
    print(dict(total), "songs that differ:", bad)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
