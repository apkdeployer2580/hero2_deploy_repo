"""Pull hero2's 18 songs (SMAF HPS) out of the game archive for tools/ma2_render.

    python tools/ma2_render/extract_songs.py <archive.zip> <out dir> [--effects]

<archive.zip> is either the original dump (dist/영웅서기2-1.zip) or the app asset
(android_app/app/src/main/assets/hero2_ktf.zip) — both hold `01035D0B.jar` with
snd/00·01 and P/snd/02..53 beside it. Only MTR (score) files are written, named
by their number (`01`, `02`, … `17`, `42`); the ATR effects are skipped — unless
`--effects` is given, which writes ONLY them (`00`, `40`, `41`, `43` … `53`) for
`ma2_render sfx`.
The output is game audio — keep it out of the repository (dist/ is gitignored).
"""
import io
import os
import sys
import zipfile


def is_score(data):
    """True if the MMMD file has a score track (MTR) chunk at its top level."""
    if data[:4] != b"MMMD":
        return False
    end = 8 + int.from_bytes(data[4:8], "big") - 2
    at = 8
    while at + 8 <= end:
        if data[at:at + 3] == b"MTR":
            return True
        at += 8 + int.from_bytes(data[at + 4:at + 8], "big")
    return False


def main():
    effects = "--effects" in sys.argv
    args = [a for a in sys.argv[1:] if a != "--effects"]
    if len(args) != 2:
        sys.exit(__doc__)
    src, out = args
    os.makedirs(out, exist_ok=True)
    z = zipfile.ZipFile(src)
    files = {}
    jar = zipfile.ZipFile(io.BytesIO(z.read("01035D0B.jar")))
    for name in ("snd/00", "snd/01"):
        files[name.split("/")[-1]] = jar.read(name)
    for info in z.infolist():
        if info.filename.startswith("P/snd/"):
            files[info.filename.split("/")[-1]] = z.read(info.filename)
    n = 0
    for name, data in sorted(files.items()):
        if is_score(data) == effects:
            continue
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)
        n += 1
    print(f"{n} {'effects' if effects else 'songs'} -> {out}")


if __name__ == "__main__":
    main()
