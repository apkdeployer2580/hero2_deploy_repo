#!/usr/bin/env python3
"""로그 패시브 「암살」 — 아이콘 (2026-10-09 (11)).

사용자(2026-10-09): *"로그의 패시브 스킬 암살을 추가할꺼야. 상대방에게 가하는 첫번째 공격이 100% 방어 무시 추가 피해를 입히는
패시브야 lv은 5lv까지 있고 5lv까지 찍으면 20%부터 100%까지 추가피해를 입히는거야."* 수치·동작은 코드
(`guest_anchors::IlpungPatch::AMSAL*` · `hero2java::h2_ilp_amsal`), 이 파일은 아이콘 하나뿐이다(그림은 Claude — 확인 대기).

* `a.p[17]` → 스킬 창의 PASSIVE 탭은 아이콘을 그리지 않는다(셀 그리기가 PASSIVE 에서 아이콘 전에 돌아간다, 0x148f44) —
  아이콘은 **상세 창 `cj`** 만 그린다: `n.b(BII)V(cj.a)` 로 `img/gmico` 프레임 2(로그 띠)의 칸 `17 − 5 = 12`.
* 그래서 로그 띠를 12 → **13 칸**으로 넓히고 칸 12 에 그린다(awakenings 뒤). 이름 · 설명 = `t/sk2.tdf` 행 24 · 25
  (`make_hero2_asset.py` `SK2_APPEND`).
* 패시브라 퀵슬롯에 오르지 않는다 — 값 칸(값 − 1)은 없다.

⛔ `img/gmico` 의 팔레트는 253/256 색 — 새 아이콘은 **있는 색 가운데 가장 가까운 색**으로 그린다(색을 더하지 않는다).

    python tools_re/rogue_amsal.py preview <out.png> [zip]
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import awakenings  # noqa: E402
import costume_sheets as cs  # noqa: E402

ICON_SHEET = awakenings.ICON_SHEET
ICON = awakenings.ICON
ROGUE_STRIP = 2
CELLS_BEFORE = awakenings.icon_cells_after()[ROGUE_STRIP]  # 12 (개화 뒤)
CELLS = CELLS_BEFORE + 1
CELL = 12  # = IlpungPatch::AMSAL_ICON (17 − 5)
assert CELL == CELLS - 1, "the new cell is the strip's last"


def icon_amsal():
    """보랏빛 어둠 바탕, 오른쪽 위에서 왼쪽 아래로 찌르는 은빛 단검(금빛 날밑 · 갈색 손잡이), 칼끝의 핏방울 둘."""
    rows = awakenings._icon_bg((0x46, 0x22, 0x5A), (0x0E, 0x06, 0x16))
    W, S, D = (0xFF, 0xFF, 0xFF), (0xC8, 0xD8, 0xF0), (0x70, 0x80, 0x98)
    G, H = (0xFF, 0xD0, 0x40), (0x80, 0x50, 0x20)
    R, RD = awakenings.BLOOD, awakenings.BLOOD_DARK
    pts = {}
    # the blade: the tip at the lower left (2, 9), the edge lit, the back in shadow
    for i, (x, y) in enumerate([(2, 9), (3, 8), (4, 7), (5, 6), (6, 5)]):
        pts[(x, y)] = W if i == 0 else S
        pts[(x + 1, y + 1)] = D
    pts[(3, 9)] = D
    # the guard across the blade, the grip, the pommel
    for (x, y) in ((6, 3), (7, 4), (8, 5), (9, 6)):
        pts[(x, y)] = G
    for (x, y) in ((8, 3), (9, 2), (10, 1)):
        pts[(x, y)] = H
    pts[(11, 0)] = G
    # the blood: a drop under the tip and one falling
    pts[(1, 10)] = R
    pts[(1, 11)] = RD
    pts[(3, 11)] = R
    for (x, y), rgb in pts.items():
        rows[y][x] = rgb
    return rows


def patch_icon(mph, mpd):
    """(mph, mpd) of `img/gmico` (awakenings 뒤) with the rogue strip widened to 13 cells and 암살 in cell 12, the nearest
    EXISTING colours (no colour added). Every other frame and every earlier cell byte-identical."""
    sheet = cs.sheet_decode(mph, mpd, ICON_SHEET)
    palette = list(sheet.palette)
    index = awakenings._palette_index(palette)
    frames = list(sheet.frames)
    strip = sheet.frames[ROGUE_STRIP]
    if (strip.w, strip.h) != (CELLS_BEFORE * ICON, ICON):
        raise ValueError(f"{ICON_SHEET} frame {ROGUE_STRIP}: {strip.w}×{strip.h} — expected {CELLS_BEFORE} cells")
    width = CELLS * ICON
    pixels = [list(row) + [0] * (width - strip.w) for row in strip.pixels]
    for y, row in enumerate(icon_amsal()):
        for x, rgb in enumerate(row):
            pixels[y][ICON * CELL + x] = awakenings._index_of(palette, index, rgb, room=False)
    frames[ROGUE_STRIP] = cs.Frame(width, strip.h, pixels, flags=strip.flags, lead=strip.lead, extra=strip.extra)
    assert len(palette) == len(sheet.palette), "no colour added"
    out = cs.Sheet(palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out, any_head=True)
    back = cs.sheet_decode(new_mph, new_mpd, ICON_SHEET)
    for k, (a, b) in enumerate(zip(back.frames, sheet.frames)):
        if k != ROGUE_STRIP:
            assert (a.w, a.h, a.pixels, a.flags) == (b.w, b.h, b.pixels, b.flags), f"{ICON_SHEET} frame {k} moved"
    assert back.palette == sheet.palette, "colours moved"
    for y in range(ICON):
        lo, hi = 0, CELLS_BEFORE * ICON
        assert back.frames[ROGUE_STRIP].pixels[y][lo:hi] == sheet.frames[ROGUE_STRIP].pixels[y][lo:hi], "the earlier cells moved"
    assert back.frames[ROGUE_STRIP].w == CELLS * ICON
    return new_mph, new_mpd


def cmd_preview(out_png, zip_path):
    import io
    import zipfile

    from PIL import Image

    z = zipfile.ZipFile(zip_path)
    jar = zipfile.ZipFile(io.BytesIO(z.read([n for n in z.namelist() if n.endswith(".jar")][0])))
    sheet = cs.sheet_decode(jar.read(ICON_SHEET + ".mph"), jar.read(ICON_SHEET + "_0.mpd"), ICON_SHEET)
    strip = sheet.frames[ROGUE_STRIP]
    scale = 8
    img = Image.new("RGB", (strip.w * scale, strip.h * scale), (27, 29, 35))
    for y in range(strip.h):
        for x in range(strip.w):
            i = strip.pixels[y][x]
            if i:
                rgb = tuple(sheet.palette[i])
                for dy in range(scale):
                    for dx in range(scale):
                        img.putpixel((x * scale + dx, y * scale + dy), rgb)
    img.save(out_png)
    print(f"wrote {out_png} ({strip.w // ICON} cells)")


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "preview":
        cmd_preview(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "android_app", "app", "src", "main", "assets", "hero2_ktf.zip"))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
