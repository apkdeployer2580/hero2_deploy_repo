"""Rewrite the item-text files inside the shipped game asset, preserving everything else.

  KTF: assets/hero3_fixed.zip -> 0103EFD4.jar -> dat/{i16,i18}_dat
  LGT: assets/hero3_fixed.zip -> 0002865E.jar -> res/dat/{i16,i18}_dat

Both archives are rebuilt entry-by-entry, copying every other member's DECOMPRESSED
bytes and its original compress_type, so nothing but the named files change. The result
is verified by re-reading it out of the rebuilt archives and re-parsing it.

★ 2026-08-14 — TWO members are patched now, not one:
  * `i18_dat` (cat 0x12, consumables) — cash-item renames, `rename_cash_items.py`
  * `i16_dat` (cat 0x10, orbs)        — slot-advisory removal, `strip_orb_slot_text.py`
Each has its own `build`/`verify` pair and each is checked independently, so a failure
in one names the file it came from.

★ The member path is LOCATED, not hardcoded (2026-08-11): the two carrier builds ship
the same file under different prefixes (`dat/` vs `res/dat/`), and the jar is named
after the carrier's AID.

★ 2026-08-13 — REBUILT ON THE CORRECT RECORD MODEL, WITH AN `.orig` FALLBACK. The old
pipeline (via `decode_items.py`) shipped an i18_dat whose length prefixes were stale
from record 0x11 on (CASH_SHOP.md §(4)); `rename_cash_items.py` now refuses any input
that does not parse byte-exact, and THIS tool answers the refusal: when the zip's
current i18_dat is the damaged one and `<zip>.orig` exists (the backup this very tool
makes on --apply), the rebuild restarts from the .orig — so one `--apply` run both
repairs the framing and applies the current rename table.

Usage: python patch_asset.py <zip> [--apply]
Without --apply it writes <zip>.new and only reports.
"""
import io
import os
import shutil
import sys
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dump_item_db as DB            # noqa: E402  (the correct record model)
import rename_cash_items as RCI      # noqa: E402
import strip_orb_slot_text as ORB    # noqa: E402

# basename -> the module that rewrites it (each exposes build(bytes) and verify(a, b))
TARGETS = {
    'i18_dat': RCI,   # cat 0x12 consumables — cash-item renames
    'i16_dat': ORB,   # cat 0x10 orbs        — "…에 결합." advisory removal
}
# The member whose framing decides whether the shipped asset is pristine. `i18_dat` is
# the one the old broken record model damaged, so it is the canary for the .orig fallback.
CANARY_NAME = 'i18_dat'


def find_target(jar_bytes: bytes, base: str) -> str:
    """The one member with this basename, whatever directory it sits in."""
    names = [n for n in zipfile.ZipFile(io.BytesIO(jar_bytes)).namelist()
             if n.rsplit('/', 1)[-1] == base]
    if len(names) != 1:
        raise SystemExit(f'expected exactly one {base} in the jar, got {names}')
    return names[0]


def rebuild(zbytes: bytes, members: dict) -> bytes:
    """Rebuild a zip, replacing each `members[name]` through its transform."""
    src = zipfile.ZipFile(io.BytesIO(zbytes))
    out = io.BytesIO()
    seen = set()
    with zipfile.ZipFile(out, 'w') as dst:
        for info in src.infolist():
            data = src.read(info.filename)
            if info.filename in members:
                data = members[info.filename](data)
                seen.add(info.filename)
            # Keep each entry's own storage method (some assets are STORED).
            zi = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            zi.compress_type = info.compress_type
            zi.external_attr = info.external_attr
            zi.internal_attr = info.internal_attr
            zi.create_system = info.create_system
            dst.writestr(zi, data)
    missing = set(members) - seen
    if missing:
        raise SystemExit(f'{sorted(missing)} not found')
    return out.getvalue()


def member_of(zbytes: bytes, base: str) -> bytes:
    """The bytes of <zip> -> <jar> -> res?/dat/<base>."""
    z = zipfile.ZipFile(io.BytesIO(zbytes))
    jars = [n for n in z.namelist() if n.lower().endswith('.jar')]
    if len(jars) != 1:
        raise SystemExit(f'expected exactly one jar, got {jars}')
    jb = z.read(jars[0])
    return zipfile.ZipFile(io.BytesIO(jb)).read(find_target(jb, base))


def parses_byte_exact(blob: bytes) -> bool:
    recs = list(DB.records(blob))
    return bool(recs) and recs[-1][1] + 2 + len(recs[-1][2]) == len(blob)


def backup_paths(zpath: str) -> list:
    """Where a pristine copy of `zpath` may live, best first.

    `android_app/asset_backup/` is the documented home for pristine assets (CLAUDE.md)
    and is the ONLY correct one: anything left beside the zip inside `assets/` is packed
    into the APK verbatim, which cost ~1.76 MB of dead weight until 2026-08-14. The
    sibling path stays as a fallback so an older tree still repairs itself.
    """
    name = os.path.basename(zpath) + '.orig'
    here = os.path.dirname(os.path.abspath(zpath))
    # …/android_app/app/src/main/assets -> …/android_app/asset_backup
    android_app = os.path.abspath(os.path.join(here, '..', '..', '..', '..'))
    return [os.path.join(android_app, 'asset_backup', name), os.path.join(here, name)]


def pristine_path(zpath: str):
    """The first existing backup that parses cleanly, or None."""
    for p in backup_paths(zpath):
        if os.path.exists(p) and parses_byte_exact(member_of(open(p, 'rb').read(), CANARY_NAME)):
            return p
    return None


def main() -> None:
    zpath = sys.argv[1]
    apply = '--apply' in sys.argv
    zbytes = open(zpath, 'rb').read()

    # ── source selection ──
    # The tables must start from a PRISTINE item DB, so a known-good backup ALWAYS wins:
    # rebuilding from the already-patched asset happens to be idempotent for today's
    # tables, but it silently would not be for any table that edits a field it also
    # matches on. Falling back to the shipped zip only covers a tree with no backup yet.
    bak = pristine_path(zpath)
    if bak is not None:
        src_bytes = open(bak, 'rb').read()
        print(f'{zpath}: rebuilding from the pristine backup {bak}')
    elif parses_byte_exact(member_of(zbytes, CANARY_NAME)):
        src_bytes = zbytes
        print(f'{zpath}: no pristine backup found — using the shipped asset as the source')
    else:
        raise SystemExit(
            f'{zpath}: {CANARY_NAME} has the stale-length-prefix damage and no pristine '
            f'backup exists to rebuild from (looked in {", ".join(backup_paths(zpath))}) '
            '— restore a pristine asset first')

    z = zipfile.ZipFile(io.BytesIO(src_bytes))
    jars = [n for n in z.namelist() if n.lower().endswith('.jar')]
    jar = jars[0]
    print(f'{zpath}: {len(z.namelist())} entries, game jar = {jar}')

    def patch_jar(jb: bytes) -> bytes:
        paths = {find_target(jb, base): mod for base, mod in TARGETS.items()}
        for path, mod in paths.items():
            print(f'  item text member = {path}  ({mod.__name__})')
        newjar = rebuild(jb, {p: m.build for p, m in paths.items()})
        jz_b, jz_a = zipfile.ZipFile(io.BytesIO(jb)), zipfile.ZipFile(io.BytesIO(newjar))
        # Round-trip check per member: byte-exact parse and only the intended rows changed.
        for path, mod in paths.items():
            before, after = jz_b.read(path), jz_a.read(path)
            mod.verify(before, after)
            print(f'  {path}: {len(before)} -> {len(after)} bytes')
        # Every OTHER member of the jar must be byte-identical.
        assert jz_b.namelist() == jz_a.namelist(), 'jar member list changed'
        for n in jz_b.namelist():
            if n not in paths and jz_b.read(n) != jz_a.read(n):
                raise SystemExit(f'jar member {n} changed unexpectedly')
        print(f'  jar: {len(jz_a.namelist())} members, all others byte-identical')
        return newjar

    newzip = rebuild(src_bytes, {jar: patch_jar})
    z2 = zipfile.ZipFile(io.BytesIO(newzip))
    assert z.namelist() == z2.namelist(), 'zip member list changed'
    for n in z.namelist():
        if n != jar and z.read(n) != z2.read(n):
            raise SystemExit(f'zip member {n} changed unexpectedly')
    print(f'  zip: {len(z2.namelist())} entries, all others byte-identical')

    if apply:
        # Only ever back up a PRISTINE asset — backing up an already-patched one would
        # poison the fallback path above. `backup_paths()[0]` is asset_backup/, which is
        # outside `assets/` and so is not packed into the APK.
        if pristine_path(zpath) is None and parses_byte_exact(member_of(zbytes, CANARY_NAME)):
            bak = backup_paths(zpath)[0]
            os.makedirs(os.path.dirname(bak), exist_ok=True)
            shutil.copy2(zpath, bak)
            print(f'  backup -> {bak}')
        open(zpath, 'wb').write(newzip)
        print(f'  APPLIED: {zpath}  {len(zbytes)} -> {len(newzip)} bytes')
    else:
        # NOT beside the zip: `assets/` is packed verbatim into the APK.
        out = os.path.join(os.path.dirname(backup_paths(zpath)[0]),
                           os.path.basename(zpath) + '.new')
        os.makedirs(os.path.dirname(out), exist_ok=True)
        open(out, 'wb').write(newzip)
        print(f'  dry run -> {out}  {len(zbytes)} -> {len(newzip)} bytes')


if __name__ == '__main__':
    main()
