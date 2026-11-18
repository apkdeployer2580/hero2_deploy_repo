#!/usr/bin/env python3
"""Dump 영웅서기2 text tables (`t/*.tdf`) to a UTF-8 file.

WHY THIS EXISTS
---------------
Three design questions were settled on 2026-09-01 by reading these tables, and every
one of them had been answered WRONG from disassembly and guesswork first:

  * `t/help.tdf[8]` — the game's own control guide. "스킬: 1, 3, 7, 9 ; 회복 아이템: 0 ;
    회복 아이템 교체: #". This is what killed "`#` is the quickbar page key".
  * `t/gm.tdf[34..37]` — "1번에 등록" … "9번에 등록", and they sit in the SKILL context
    block. The ITEM block ([28..31] 버리기 / 장비장착 / 사용하기 / 상세보기) has no
    register row at all, which is how we know hero2 cannot bind an item to a slot.
  * `t/shop.tdf[4..16]` — 무기 / 방어구(투구·갑옷·장갑·신발·방패) / 기타. No 보조무기
    slot, which is what retired `CtrlId::Secondary`.

The archive holds 20 tables / 824 records in all (measured 2026-09-01; the "799" in
HERO2_NEXT_SESSION.md §2① is that count as it stood in an earlier pass). It is the
cheapest evidence in the project and it had no tool, so every session that wanted it
was re-deriving the container format first.

FORMAT (verified against every `.tdf` in the KTF 1.0.7 archive, no parse slack)
------------------------------------------------------------------------------
    [1 byte  record count]
    [2 bytes BE length][EUC-KR bytes]   x count

`;` inside a record is the game's own line break, not a separator we add.

⚠ NEVER print EUC-KR through this machine's console — a cp949 terminal mangles it and
you end up debugging the terminal. This writes a UTF-8 file; read the file.

    python tools_re/dump_tdf.py <out.txt>                       # every table
    python tools_re/dump_tdf.py <out.txt> gm.tdf help.tdf       # just these
    python tools_re/dump_tdf.py <out.txt> --zip dist/영웅서기2-1.zip
"""

import io
import sys
import zipfile

# The APK asset by default: it is the tree the emulator actually loads, so what this
# prints is what the running game reads. `--zip` takes the original KTF dump instead.
DEFAULT_ZIP = "android_app/app/src/main/assets/hero2_ktf.zip"


def sources(path):
    """The archive plus the jar nested inside it — `.tdf` files live in the jar."""
    outer = zipfile.ZipFile(path)
    yield outer
    for name in outer.namelist():
        if name.lower().endswith(".jar"):
            yield zipfile.ZipFile(io.BytesIO(outer.read(name)))


def decode(blob):
    """`(count, records, exact)`. `exact` = the records consumed the file byte for byte,
    which is the check that says the container format is right rather than plausible."""
    count, i, out = blob[0], 1, []
    for _ in range(count):
        if i + 2 > len(blob):
            break
        length = blob[i] << 8 | blob[i + 1]
        i += 2
        out.append(blob[i : i + length].decode("euc-kr", "replace"))
        i += length
    return count, out, i == len(blob)


def main():
    args = sys.argv[1:]
    zip_path = DEFAULT_ZIP
    if "--zip" in args:
        k = args.index("--zip")
        zip_path = args[k + 1]
        del args[k : k + 2]
    if not args:
        sys.exit(__doc__.strip().splitlines()[-3].strip())
    out_path, want = args[0], set(args[1:])

    with io.open(out_path, "w", encoding="utf-8") as f:
        seen = 0
        for src in sources(zip_path):
            for name in src.namelist():
                if not name.endswith(".tdf"):
                    continue
                if want and name.rsplit("/", 1)[-1] not in want:
                    continue
                blob = src.read(name)
                if not blob:
                    continue
                count, records, exact = decode(blob)
                f.write(
                    "==== %s  (%d B, count=%d, parsed=%d, exact=%s)\n"
                    % (name, len(blob), count, len(records), exact)
                )
                for i, rec in enumerate(records):
                    f.write("[%3d] %s\n" % (i, rec.replace("\n", " \\n ")))
                f.write("\n")
                seen += 1
    print("wrote %s (%d table(s) from %s)" % (out_path, seen, zip_path))


if __name__ == "__main__":
    main()
