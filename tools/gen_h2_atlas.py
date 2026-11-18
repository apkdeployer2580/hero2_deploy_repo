#!/usr/bin/env python3
"""Pack hero2's quickbar icons into `wie_android/src/icons.bin` (the `H3IC` atlas).

WHAT GOES IN (2026-09-17, HERO2_QUICKSLOT_TOUCH_PLAN.md S1)
----------------------------------------------------------
The quickbar `ag.j:[B` holds a byte per slot: `1..7` = the n-th ACTIVE skill of the
character's class, `10+k` = the k-th skill of the current beast. The game draws them
from two sprites (SKT `bd.b` / `bd.c`, confirmed against the skill window's pixels):

  * `img/gmico`  frames 0..2  — one 12 px tall STRIP per class (버서커 · 나이트 · 로그),
                                cell `v-1` at x = 12*(v-1). 84/84/72 px wide = 7/7/6 cells.
                                (frame 3 is the equipment-slot column — not packed.)
  * `img/bico`   frames 0..12 — one 12x12 icon per GLOBAL beast skill id; the beast's
                                k-th skill maps through `bn.j[k]` to that id.
  * `img/glb`    frame 17     — the ITEM TYPE strip (S7, 2026-09-17 저녁): 12 x 408 =
                                34 cells, one per `t/itmtp.tdf` row except row 7 (a
                                message, not a type): SKT `bd.a(int,int,byte)` draws
                                cell `t` below 8 and `t - 1` from 8 on. Packed as a
                                FOURTH sheet so `atlas.rs::parse` stays untouched
                                (`Atlas::item_type`). Types 24/25/26 (포션·하이포션·
                                포도주) are the potion plates' icons.
  * sheet 4 (2026-09-21, #13 가방 5페이지): ONE cell — the 재료가방 icon (type 29 =
                                strip cell 28) rotated 90° to the LEFT (user: "가방아이콘은
                                재료가방을 90도 왼쪽으로 돌린 아이콘"), baked here rather
                                than rotated at draw time (the HUD layer cache would redo
                                it per pixel). `Atlas::bag` — the inventory page tabs.
  * sheet 5 (2026-09-29, 상태 HUD 초상): the character's HEADS — `hero/0/<n>` frame 2 (the
                                one facing the viewer; 0 = back, 1 = side), cells 0..=16 =
                                the 17 helmet sprites a helmet's `bu.l` names, cell 17 =
                                `hero/0/100`, the bare head. KEYED: palette index 0 is the
                                sprites' transparent colour (the `.mph` carries a one-entry
                                `tRNS` for it), so those pixels get alpha 0 and the portrait
                                ring shows through. `Atlas::head` — `hud_view.rs`'s ring.
  * sheet 6 (2026-10-09, 가디언 장비 개성 옵션): the HOST icons — art no game sheet has.
                                Cell 0 = 「용의 심장」's buff (the necklace's 1 % kill proc,
                                「1 분간 받는 피해 −10 %」): a crimson heart with two gold horns
                                and a gold slit-pupil dragon eye, drawn here (`DRAGON_HEART`,
                                a Claude draft — the user asked for a new 용의 심장 icon).
                                Keyed like the heads. `atlas::HOST_SHEET` · `HOST_DRAGON_HEART`.

Both are drawn OPAQUE by the game (`setClip(x, y, 12, 12)` + `drawImage`), so no colour
key is applied here — a keyed corner would punch holes in the fire icons.

FORMAT (unchanged from hero3's gen_h3_icons.py, so `atlas.rs::parse` is untouched)
  magic "H3IC"; item_count u16 LE; items[item_count]: w u8, h u8, px[w*h] u32 LE ARGB;
  sheet_count u8; per sheet: cell_count u8, cells[]: w u8, h u8, px[w*h] u32 LE.
  hero2 meaning: items = BEAST skill icons by global id; sheets = classes 0..2 + the
  item-type strip as sheet 3.

Usage:  python tools/gen_h2_atlas.py [--zip <hero2_ktf.zip>] [--preview <dir>]
"""
import io
import os
import struct
import sys
import zipfile

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools_re"))
from dump_mpd import frames  # noqa: E402

from PIL import Image  # noqa: E402

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
DEFAULT_ZIP = os.path.join(ROOT, "android_app", "app", "src", "main", "assets", "hero2_ktf.zip")
OUT = os.path.join(ROOT, "wie_android", "src", "icons.bin")
CELL = 12
CLASS_SHEETS = 3  # gmico frames 0..2; frame 3 = equipment column
BEAST_ICONS = 19  # bico frames 0..12 (bsk.tdf has 12 skills + 1) + ★★ 2026-10-16 the six 악의 versions 13..18 (demon_skin.SKIN)
ITEM_STRIP_FRAME = 17  # glb frame 17 = as.q, the inventory's item-type strip (vertical)
ITEM_STRIP_CELLS = 34
BAG_TYPE = 29  # 재료가방 (itm/29), drawn from strip cell `type - 1` = 28
HEAD_SPRITES = 17  # hero/0/00..16 — the helmets' part sprites (bu.l)
HEAD_BARE = 100  # hero/0/100 — no helmet (cf.a(I)V: movs r0,#0x64)
HEAD_FRAME = 2  # facing the viewer

# Sheet 6, cell 0: 「용의 심장」 (12 x 12, '.' = transparent).
DRAGON_HEART = [
    "g..........g",
    ".gKK....KKg.",
    ".KrRK..KRRK.",
    "KrrRRKKRRRDK",
    "KrRRGkkGRRDK",
    "KRRGGkkGGRDK",
    "KRRGGkkGGRDK",
    ".KRRGkkGRDK.",
    "..KRRGGRDK..",
    "...KRRRDK...",
    "....KRDK....",
    ".....KK.....",
]
DRAGON_COLOURS = {
    "K": (0x3A, 0x08, 0x08),  # outline
    "R": (0xC8, 0x1E, 0x1E),  # heart
    "r": (0xFF, 0x6A, 0x4A),  # its light side
    "D": (0x7A, 0x10, 0x10),  # its shadow
    "G": (0xF0, 0xC0, 0x40),  # the eye's gold
    "g": (0xD8, 0xA0, 0x30),  # the horns
    "k": (0x10, 0x00, 0x00),  # the slit pupil
}


def dragon_heart():
    rows = DRAGON_HEART
    if len(rows) != CELL or any(len(r) != CELL for r in rows):
        sys.exit("DRAGON_HEART is not 12 x 12")
    im = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    for y, row in enumerate(rows):
        for x, ch in enumerate(row):
            if ch != ".":
                im.putpixel((x, y), DRAGON_COLOURS[ch] + (255,))
    return im


def read_members(zip_path):
    outer = zipfile.ZipFile(zip_path)
    jar = [n for n in outer.namelist() if n.lower().endswith(".jar")][0]
    inner = zipfile.ZipFile(io.BytesIO(outer.read(jar)))
    return {n: inner.read(n) for n in inner.namelist() if n.startswith("img/") or n.startswith("hero/0/")}


def head_frame(members, n):
    """`hero/0/<n>` frame HEAD_FRAME as RGBA, palette index 0 -> alpha 0."""
    base = f"hero/0/{n:02d}" if n < 100 else f"hero/0/{n}"
    mph = members.get(f"{base}.mph")
    mpd = members.get(f"{base}_0.mpd")
    if mph is None or mpd is None:
        sys.exit(f"missing {base}.mph / {base}_0.mpd in the jar")
    fr = frames(mph, mpd)
    if len(fr) <= HEAD_FRAME:
        sys.exit(f"{base} has {len(fr)} frames, expected > {HEAD_FRAME}")
    pal = Image.open(io.BytesIO(fr[HEAD_FRAME]["png"]))
    if pal.mode != "P":
        sys.exit(f"{base} frame {HEAD_FRAME} is {pal.mode}, expected a palette image")
    idx = list(pal.getdata())
    rgba = pal.convert("RGBA")
    rgba.putdata([(r, g, b, 0 if i == 0 else 255) for (r, g, b, _a), i in zip(rgba.getdata(), idx)])
    return rgba


def pack_icon_keyed(im):
    """Like `pack_icon`, but keeps the alpha: 0 stays 0 (transparent), anything else is opaque."""
    w, h = im.size
    out = bytearray(struct.pack("<BB", w, h))
    for r, g, b, a in im.getdata():
        out += struct.pack("<I", (0 if a == 0 else 0xFF000000) | (r << 16) | (g << 8) | b)
    return bytes(out)


def sprite_frames(members, base):
    mph = members.get(f"{base}.mph")
    mpd = members.get(f"{base}_0.mpd")
    if mph is None or mpd is None:
        sys.exit(f"missing {base}.mph / {base}_0.mpd in the jar")
    return [Image.open(io.BytesIO(f["png"])).convert("RGBA") for f in frames(mph, mpd)]


def pack_icon(im):
    w, h = im.size
    out = bytearray(struct.pack("<BB", w, h))
    for r, g, b, _a in im.getdata():
        out += struct.pack("<I", 0xFF000000 | (r << 16) | (g << 8) | b)
    return bytes(out)


def main():
    args = sys.argv[1:]
    zip_path = DEFAULT_ZIP
    preview = None
    if "--zip" in args:
        k = args.index("--zip")
        zip_path = args[k + 1]
    if "--preview" in args:
        k = args.index("--preview")
        preview = args[k + 1]
    members = read_members(zip_path)

    gm = sprite_frames(members, "img/gmico")
    bi = sprite_frames(members, "img/bico")
    glb = sprite_frames(members, "img/glb")
    if len(gm) < CLASS_SHEETS or len(bi) < BEAST_ICONS or len(glb) <= ITEM_STRIP_FRAME:
        sys.exit(f"unexpected frame counts: gmico {len(gm)} bico {len(bi)} glb {len(glb)}")
    items = glb[ITEM_STRIP_FRAME]
    # ★★ 2026-10-16: the asset appends own-icon cells after the shipped 34 (tools_re/elixir_icon.py) — the atlas keeps the 34 type cells
    if items.size[0] != CELL or items.size[1] < CELL * ITEM_STRIP_CELLS or items.size[1] % CELL:
        sys.exit(f"glb frame {ITEM_STRIP_FRAME} is {items.size}, expected {CELL}x{CELL * ITEM_STRIP_CELLS} or longer")

    blob = bytearray(b"H3IC")
    beasts = [im for im in bi[:BEAST_ICONS]]
    blob += struct.pack("<H", len(beasts))
    for im in beasts:
        if im.size != (CELL, CELL):
            sys.exit(f"bico frame is {im.size}, expected {CELL}x{CELL}")
        blob += pack_icon(im)
    blob += struct.pack("<B", CLASS_SHEETS + 4)
    sheets = []
    for c in range(CLASS_SHEETS):
        strip = gm[c]
        if strip.height != CELL or strip.width % CELL != 0:
            sys.exit(f"gmico frame {c} is {strip.size}, expected a {CELL} px strip")
        cells = [strip.crop((i * CELL, 0, (i + 1) * CELL, CELL)) for i in range(strip.width // CELL)]
        sheets.append(cells)
        blob += struct.pack("<B", len(cells))
        for im in cells:
            blob += pack_icon(im)
    # Sheet 3: the item-type strip, top to bottom — ★★ 2026-10-17 with the asset's own-icon cells after the shipped 34
    # (tools_re/elixir_icon.py; ItemIconPatch::CELLS): an item plate names the 엘릭서 by its display type 35 = cell 34
    # (hero2java::H2_PLATE_ELIXIR, user: 「엘릭서 … 포션종류 취급」), so the plate draws the elixir's own icon.
    cells = [items.crop((0, i * CELL, CELL, (i + 1) * CELL)) for i in range(items.size[1] // CELL)]
    sheets.append(cells)
    blob += struct.pack("<B", len(cells))
    for im in cells:
        blob += pack_icon(im)
    # Sheet 4: the bag tab icon — the 재료가방 cell turned 90° counter-clockwise.
    bag = cells[BAG_TYPE - 1].rotate(90, expand=True)  # PIL rotates counter-clockwise
    sheets.append([bag])
    blob += struct.pack("<B", 1)
    blob += pack_icon(bag)
    # Sheet 5: the heads — 00..16 by helmet sprite, then the bare head.
    heads = [head_frame(members, n) for n in range(HEAD_SPRITES)] + [head_frame(members, HEAD_BARE)]
    sheets.append(heads)
    blob += struct.pack("<B", len(heads))
    for im in heads:
        blob += pack_icon_keyed(im)
    # Sheet 6: the host icons — 용의 심장's buff.
    host = [dragon_heart()]
    sheets.append(host)
    blob += struct.pack("<B", len(host))
    for im in host:
        blob += pack_icon_keyed(im)
    with open(OUT, "wb") as f:
        f.write(blob)
    print(f"wrote {OUT} ({len(blob)} B): {len(beasts)} beast icons, sheets {[len(s) for s in sheets]}")

    if preview:
        os.makedirs(preview, exist_ok=True)
        scale = 6
        pitch = 16  # the widest cell (the heads); smaller icons sit top-left in theirs
        w = max(len(beasts), max(len(s) for s in sheets)) * pitch * scale
        h = (1 + len(sheets)) * pitch * scale
        canvas = Image.new("RGBA", (w, h), (40, 40, 40, 255))
        for i, im in enumerate(beasts):
            canvas.alpha_composite(im.convert("RGBA").resize((im.width * scale, im.height * scale), Image.NEAREST), (i * pitch * scale, 0))
        for c, cells in enumerate(sheets):
            for i, im in enumerate(cells):
                big = im.convert("RGBA").resize((im.width * scale, im.height * scale), Image.NEAREST)
                canvas.alpha_composite(big, (i * pitch * scale, (c + 1) * pitch * scale))
        canvas.save(os.path.join(preview, "h2_atlas_preview.png"))
        print("preview ->", os.path.join(preview, "h2_atlas_preview.png"))


if __name__ == "__main__":
    main()
