#!/usr/bin/env python3
"""상세 월드맵 원화를 게임용 2560×1440 RGB565로 변환한다.

기본 원화는 docs/worldmap/worldmap_detailed.png이며 Lanczos로 출력 크기를 맞춘다.
이전 도트 원화는 --source docs/worldmap_base.png 로 지정하면 기존 xBR 2× 경로를 쓴다.
다른 상세 원화는 --source <png> --detailed 로 지정한다.

    python tools_re/worldmap_upscale.py
    python tools_re/worldmap_upscale.py --preview out.png   # 2560 그림의 가운데 640×360 을 4배로 (전/후 비교)

파일 형식 = `[u32 w][u32 h]` + w×h × RGB565 LE (전과 같다).
"""

import os
import argparse
import struct
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from demon_upscale import box_half, xbr2x  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASE = os.path.join(ROOT, "docs", "worldmap_base.png")
DETAILED = os.path.join(ROOT, "docs", "worldmap", "worldmap_detailed.png")
OUT = os.path.join(ROOT, "wie_android", "src", "worldmap.rgb565")


def to_rgb565(rgb):
    r = (rgb[..., 0].astype(np.uint16) >> 3) << 11
    g = (rgb[..., 1].astype(np.uint16) >> 2) << 5
    b = rgb[..., 2].astype(np.uint16) >> 3
    return (r | g | b).astype("<u2")


def upscale(base_rgb):
    rgba = np.concatenate([base_rgb, np.full(base_rgb.shape[:2] + (1,), 255, np.uint8)], -1)
    out = box_half(xbr2x(xbr2x(rgba)))
    return out[..., :3]


def build(base=DETAILED, out=OUT, detailed=False):
    im = Image.open(base).convert("RGB")
    rgb = np.asarray(im)
    detailed = detailed or os.path.normcase(os.path.abspath(base)) == os.path.normcase(DETAILED)
    up = np.asarray(im.resize((2560, 1440), Image.Resampling.LANCZOS)) if detailed else upscale(rgb)
    h, w = up.shape[:2]
    with open(out, "wb") as f:
        f.write(struct.pack("<II", w, h))
        f.write(to_rgb565(up).tobytes())
    print(f"{base} {im.size[0]}x{im.size[1]} -> {out} {w}x{h} ({os.path.getsize(out):,} B)")


def preview(out_png, src=OUT):
    with open(src, "rb") as f:
        w, h = struct.unpack("<II", f.read(8))
        px = np.frombuffer(f.read(), "<u2").reshape(h, w)
    r = ((px >> 11) & 31) * 255 // 31
    g = ((px >> 5) & 63) * 255 // 63
    b = (px & 31) * 255 // 31
    rgb = np.stack([r, g, b], -1).astype(np.uint8)
    x0, y0 = w // 2 - 320, h // 2 - 180
    crop = Image.fromarray(rgb[y0 : y0 + 360, x0 : x0 + 640])
    crop.resize((2560, 1440), Image.NEAREST).save(out_png)
    print(out_png)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", default=DETAILED)
    parser.add_argument("--output", default=OUT)
    parser.add_argument("--detailed", action="store_true")
    parser.add_argument("--preview")
    args = parser.parse_args()
    if args.preview:
        preview(args.preview, args.output)
    else:
        build(args.source, args.output, args.detailed)
