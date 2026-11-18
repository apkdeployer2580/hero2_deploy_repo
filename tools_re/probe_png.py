"""wie_probe 의 raw RGBA 덤프 → PNG (+ 여러 장 몽타주).

usage:
    python probe_png.py <dump_dir>                  # 폴더 내 *.rgba 전부 변환
    python probe_png.py <dump_dir> --montage out.png
파일명에서 크기를 읽는다: frame_00400_240x320.rgba

`<dump_dir>` = wie_probe 의 `--out`. 2026-09-04 까지 기본값이 `d:/tmp/probe` 였는데
그건 이 기계 한 대의 경로였고 다른 데선 조용히 0장을 변환했다 — 이제 필수 인수다.
"""
import glob
import os
import re
import sys

from PIL import Image


def load(path):
    m = re.search(r'_(\d+)x(\d+)\.rgba$', path)
    if not m:
        return None
    w, h = int(m.group(1)), int(m.group(2))
    raw = open(path, 'rb').read()
    if len(raw) != w * h * 4:
        print(f'  size mismatch {path}: {len(raw)} != {w*h*4}')
        return None
    return Image.frombytes('RGBA', (w, h), raw)


def main(argv):
    if len(argv) < 2:
        sys.exit(__doc__)
    d = argv[1]
    files = sorted(glob.glob(os.path.join(d, '*.rgba')))
    if not files:
        sys.exit('no *.rgba in %r' % d)
    imgs = []
    for f in files:
        im = load(f)
        if im is None:
            continue
        out = f[:-5] + '.png'
        im.save(out)
        imgs.append((os.path.basename(out), im))
        print(f'{os.path.basename(out)}  {im.size}')
    if '--montage' in argv and imgs:
        dst = argv[argv.index('--montage') + 1]
        gap = 8
        W = sum(i.width for _, i in imgs) + gap * (len(imgs) + 1)
        H = max(i.height for _, i in imgs) + gap * 2
        sheet = Image.new('RGBA', (W, H), (28, 28, 36, 255))
        x = gap
        for _, im in imgs:
            sheet.alpha_composite(im, (x, gap))
            x += im.width + gap
        sheet.save(dst)
        print(f'montage -> {dst} {sheet.size}')


if __name__ == '__main__':
    main(sys.argv)
