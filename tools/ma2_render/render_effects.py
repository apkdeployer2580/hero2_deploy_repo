"""Render hero2's effects before/after for listening (`ma2_render sfx`, every mode asked for).

    python tools/ma2_render/render_effects.py <archive.zip> <out dir> [--exe <ma2_render>] [--modes old,ship]

Writes `<nn>_<name>_<mode>.wav` per effect and `all_<mode>.wav` (every effect in a row, 0.5 s
apart, announced by nothing — the order is the list below), plus a table of what changed.
The output is game audio — keep it out of the repository (dist/ is gitignored).
"""
import io
import os
import subprocess
import sys
import wave
import zipfile

# number -> (file name part, where the game plays it)
EFFECTS = {
    "00": "door",
    "40": "critical_hit",
    "41": "miss",
    "43": "element_attack",
    "45": "berserker_shout",
    "46": "knight_aura",
    "47": "knight_sword_wave",
    "49": "rogue_flurry",
    "51": "damage_a",
    "52": "damage_b",
    "53": "basic_hit",
}
LABEL = {"old": "A_before", "decode": "A2_decode_only", "sinc": "B_after_no_air", "ship": "C_after_app"}


def main():
    args = sys.argv[1:]
    exe = os.path.join("tools", "ma2_render", "target", "release", "ma2_render.exe")
    modes = ["old", "ship"]
    for flag in ("--exe", "--modes"):
        if flag in args:
            at = args.index(flag)
            value = args[at + 1]
            del args[at:at + 2]
            if flag == "--exe":
                exe = value
            else:
                modes = value.split(",")
    if len(args) != 2:
        sys.exit(__doc__)
    src, out = args
    os.makedirs(out, exist_ok=True)
    raw = os.path.join(out, "_raw")
    os.makedirs(raw, exist_ok=True)

    z = zipfile.ZipFile(src)
    jar = zipfile.ZipFile(io.BytesIO(z.read("01035D0B.jar")))
    for number in EFFECTS:
        data = jar.read("snd/00") if number == "00" else z.read(f"P/snd/{number}")
        with open(os.path.join(raw, number), "wb") as f:
            f.write(data)

    for mode in modes:
        joined = None
        for number, name in EFFECTS.items():
            path = os.path.join(out, f"{number}_{name}_{LABEL[mode]}.wav")
            line = subprocess.run([exe, "sfx", os.path.join(raw, number), path, "--mode", mode], capture_output=True, text=True, encoding="utf-8")
            if line.returncode != 0:
                sys.exit(line.stderr)
            print(line.stdout.strip().replace(out + os.sep, ""))
            with wave.open(path, "rb") as w:
                params = w.getparams()
                frames = w.readframes(w.getnframes())
            if joined is None:
                joined = wave.open(os.path.join(out, f"all_{LABEL[mode]}.wav"), "wb")
                joined.setparams(params)
            joined.writeframes(frames + b"\0" * (params.framerate // 2 * params.nchannels * params.sampwidth))
        joined.close()
    for number in EFFECTS:
        os.remove(os.path.join(raw, number))
    os.rmdir(raw)


if __name__ == "__main__":
    main()
