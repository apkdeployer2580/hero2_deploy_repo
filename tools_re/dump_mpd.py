#!/usr/bin/env python3
"""Decode 영웅서기2 sprite containers (`*.mph` + `*_N.mpd`) to PNG.

FORMAT (2026-09-01, verified against all 2,539 frames in the KTF 1.0.7 archive)
------------------------------------------------------------------------------
A `.mpd` is a **PNG chunk stream with the 8-byte signature stripped**. It starts
straight at `IHDR` and carries no `IEND`, so frames are packed back to back and one
frame ends where the next `IHDR` begins. Every frame seen is 8-bit palette
(`colour type 3`).

The matching `.mph` is the header: a frame table plus the chunks the frames share —
in practice the `PLTE`, and sometimes a `tRNS`. A frame that has no `PLTE` of its own
uses the one in the `.mph`.

So decoding is re-framing, not decompression: copy the frame's chunks verbatim, splice
in the shared `PLTE`, prepend the signature, append an `IEND`. The CRCs stay valid
because every chunk is copied whole.

⚠ TRANSPARENCY IS A COLOUR KEY, NOT `tRNS`. `img/sui.mph`'s `tRNS` names index 0, but
the title logo's background is index 17 = `rgb(1, 0, 0)` — a sentinel near-black that
is deliberately not pure black, the classic feature-phone trick. `--key` keys the
colour that occupies all four corners, which is what you want for extracting art.
Do not trust `tRNS` here without checking the corners.

    python tools_re/dump_mpd.py <out_dir>                      # every sprite
    python tools_re/dump_mpd.py <out_dir> img/sui img/logo     # just these
    python tools_re/dump_mpd.py <out_dir> img/sui --key        # corner colour -> alpha

The title logo is `img/sui` frame 3 (169x87). `img/logo` is NOT it — that is the
publisher mark.
"""

import io
import os
import sys
import zipfile

SIG = b'\x89PNG\r\n\x1a\n'
IHDR_START = b'\x00\x00\x00\rIHDR'
IEND = bytes.fromhex('0000000049454e44ae426082')
DEFAULT_ZIP = 'android_app/app/src/main/assets/hero2_ktf.zip'


def _chunks(buf, off):
    """Walk PNG chunks from `off`, yielding (type, whole chunk incl. length and CRC)."""
    i = off
    while i + 8 <= len(buf):
        ln = int.from_bytes(buf[i:i + 4], 'big')
        typ = buf[i + 4:i + 8]
        end = i + 8 + ln + 4
        if end > len(buf):
            return
        yield typ, buf[i:end]
        if typ == b'IEND':
            return
        i = end


def _find_chunk(buf, want):
    i = buf.find(want)
    if i < 4:
        return None
    ln = int.from_bytes(buf[i - 4:i], 'big')
    return buf[i - 4:i + 4 + ln + 4]


def frames(mph, mpd):
    """Every frame in `mpd`, each rebuilt as a standalone PNG."""
    shared_plte = _find_chunk(mph, b'PLTE') if mph else None
    starts, i = [], 0
    while True:
        i = mpd.find(IHDR_START, i)
        if i < 0:
            break
        starts.append(i)
        i += 1
    bounds = starts + [len(mpd)]
    out = []
    for k, s in enumerate(starts):
        got = list(_chunks(mpd[:bounds[k + 1]], s))
        types = [t for t, _ in got]
        png = SIG
        for typ, chunk in got:
            png += chunk
            if typ == b'IHDR' and b'PLTE' not in types and shared_plte:
                png += shared_plte
        if b'IEND' not in types:
            png += IEND
        out.append({
            'w': int.from_bytes(mpd[s + 8:s + 12], 'big'),
            'h': int.from_bytes(mpd[s + 12:s + 16], 'big'),
            'png': png,
        })
    return out


def key_corner(png_bytes):
    """PNG bytes -> RGBA with the corner colour keyed out. Needs Pillow."""
    from PIL import Image
    im = Image.open(io.BytesIO(png_bytes)).convert('RGBA')
    px = im.load()
    corners = {px[0, 0], px[im.width - 1, 0], px[0, im.height - 1], px[im.width - 1, im.height - 1]}
    if len(corners) != 1:
        return im  # not a flat key colour; leave it alone rather than guess
    kr, kg, kb, _ = corners.pop()
    data = [(r, g, b, 0) if (r, g, b) == (kr, kg, kb) else (r, g, b, a)
            for r, g, b, a in im.getdata()]
    im.putdata(data)
    return im


def sources(zip_path):
    outer = zipfile.ZipFile(zip_path)
    yield outer
    for n in outer.namelist():
        if n.lower().endswith('.jar'):
            yield zipfile.ZipFile(io.BytesIO(outer.read(n)))


def main():
    args = [a for a in sys.argv[1:]]
    do_key = '--key' in args
    if do_key:
        args.remove('--key')
    zip_path = DEFAULT_ZIP
    if '--zip' in args:
        k = args.index('--zip')
        zip_path = args[k + 1]
        del args[k:k + 2]
    if not args:
        sys.exit('usage: dump_mpd.py <out_dir> [name ...] [--key] [--zip <archive>]')
    out_dir, want = args[0], set(args[1:])
    os.makedirs(out_dir, exist_ok=True)

    total = 0
    for src in sources(zip_path):
        for name in src.namelist():
            if not name.endswith('.mph'):
                continue
            base = name[:-4]
            if want and base not in want:
                continue
            mph = src.read(name)
            for mpd_name in sorted(m for m in src.namelist()
                                   if m.startswith(base + '_') and m.endswith('.mpd')):
                for k, f in enumerate(frames(mph, src.read(mpd_name))):
                    stem = '%s_%d' % (base.replace('/', '_'), k)
                    path = os.path.join(out_dir, stem + '.png')
                    if do_key:
                        key_corner(f['png']).save(path)
                    else:
                        open(path, 'wb').write(f['png'])
                    print('  %3dx%-3d  %s' % (f['w'], f['h'], path))
                    total += 1
    print('%d frame(s) from %s' % (total, zip_path))


if __name__ == '__main__':
    main()
