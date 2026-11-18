#!/usr/bin/env python3
"""버서커의 새 스킬 「도발」 — 그림과 애니메이션 (2026-10-07).

사용자(2026-10-07): *"버서커의 추가스킬로 도발이라는 액티브 스킬을 만들고싶어. 도발은 사자후처럼 모든 맵에 몬스터를 나에게
도발하는거고 5lv짜리 스킬이야 …"* → 물어서: 쿨 8초 · 지속 10초 · SP = 사자후의 ¼ · 사자후를 배우면 함께 열림. 수치·도발 자체는
코드(`guest_anchors::BerserkerPatch`), 이 파일은 그림뿐이다. 그림은 사용자가 정하지 않았다 — 「사자후처럼」에서 사자후의 외침
동작을 그대로 쓰고 이펙트만 보라색으로 물들였다(Claude 의 해석, 확인 대기).

── 자리 (HERO2_GUEST_RE.md §8.52) ───────────────────────────────────────────────────────────────────
* 버서커 `a.p` 15칸(콤보 4 + 스킬 11)은 빈 곳이 없다 → 도발 = **`a.p[15]`**(일풍격이 이미 모든 직업의 배열을 16칸으로) = 종류 15.
  그리는 시퀀스 = `bl.a(B)B`(9 → 4 · 10 → 9 · 그 밖 그대로) + 4 = **19**. 동작 길이 `a.n[15]` 는 `cf.a()V` 가 시퀀스에서 만든다 —
  버서커는 `a.n = new byte[S − 3]` 를 만들고 `a.n[i] ← 시퀀스 i + 4 의 프레임 수`(i < S − 4) 뒤 `a.n[10..13] ← a.n[9..12]`,
  `a.n[9] ← a.n[4]` 로 옮긴다. 그래서 `a.n[15]` 가 있으려면 시퀀스가 **20** 개(0..19)여야 하고, 시퀀스 18(= 종류 14, 아무도 안
  쓴다)은 빈 한 프레임으로 먼저 덧붙인다. 배포본 18 시퀀스 뒤.
* 시퀀스 19 = 사자후(종류 10 → 시퀀스 13, 11 프레임 — 외침 소리 프레임 1 · 도발 프레임 9 가 코드의 `TAUNT_SOUND_FRAME` ·
  `TAUNT_FRAME`) 의 포즈 그대로(망토 포함 — hero_cape 뒤), 이펙트(파트 4) 그림 29..40 만 보라색 사본 46..57 로.
* 이펙트 시트 `hero/4/0` + 12 장(46..57). 반전으로 쓰이는 원본(애니메이션 머리의 파트 4 첫 목록에 든 것)은 사본도 그 목록에 넣는다
  — 0x80 플래그는 그 목록에 든 프레임만 뒤집힌다.
* 아이콘 = `img/gmico` 프레임 0(버서커 띠 84×12 = 7칸)을 144×12(12칸)로 넓혀 칸 7(스킬 창의 8번째 칸 · 퀵슬롯의 값 8 − 1)과
  칸 11(상세 창 `cj` = `15 − 4` · 버프 HUD) 둘에 같은 그림. 8..10 은 비운다.

    python tools_re/berserker_taunt.py preview <out.png> [zip]   # 시퀀스 19 를 네 방향으로 + 아이콘
"""

import copy
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402
import venom_effect  # noqa: E402

ANIM = "hero/hero0"
SHEET = "hero/4/0"
ICON_SHEET = "img/gmico"
EFFECT_PART = 4
SEQS_BEFORE = 18  # 배포본 (hero_cape 는 시퀀스 수를 안 바꾼다)
FILLER_SEQ = 18  # = 종류 14 — 아무도 안 쓴다
TAUNT_SEQ = 19  # = guest_anchors::BerserkerPatch::TAUNT_KIND (15) + 4
ROAR_SEQ = 13  # 사자후 = 종류 10 → bl.a(B)B 9 → + 4
ROAR_FRAMES = 11
FRAMES_BEFORE = 46  # hero/4/0 배포본
FX_FROM, FX_TO = 29, 40  # 사자후가 쓰는 이펙트 그림 (외침 · 불꽃 줄 · 폭발)
FX_NEW = FRAMES_BEFORE  # 사본의 첫 번호


def tint(rgb):
    """사자후의 붉은·노란 외침 → 보라. 밝기를 지키고(층이 보이게), 회색·흰색은 옅은 라벤더."""
    r, g, b = rgb
    lum = 0.3 * r + 0.59 * g + 0.11 * b
    if max(rgb) - min(rgb) < 24:  # 회색 · 흰색
        return (min(255, round(lum * 0.92 + 12)), min(255, round(lum * 0.78)), min(255, round(lum * 0.98 + 6)))
    return (min(255, round(lum * 0.85 + 34)), min(255, round(lum * 0.30)), min(255, round(lum * 1.02 + 48)))


def fx_map():
    """원본 이펙트 그림 → 보라 사본."""
    return {f: FX_NEW + (f - FX_FROM) for f in range(FX_FROM, FX_TO + 1)}


def patch_anim(blob):
    """`hero/hero0`(hero_cape 뒤 = 18 시퀀스) + 시퀀스 18(빈 한 프레임) · 19(도발 = 사자후의 외침, 보라 이펙트)."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != SEQS_BEFORE or len(seqs[ROAR_SEQ]) != ROAR_FRAMES:
        raise ValueError(f"{ANIM}: {len(seqs)} sequences, 사자후 {len(seqs[ROAR_SEQ]) if len(seqs) > ROAR_SEQ else '-'} frames — expected the berserker animation ({SEQS_BEFORE}, {ROAR_FRAMES})")
    remap = fx_map()
    flips, loaded = parts[EFFECT_PART]
    parts[EFFECT_PART] = (flips + bytes(remap[f] for f in remap if f in flips and remap[f] not in flips), loaded)
    seqs.append([[[], [], [], []]])  # 18: 종류 14 — 아무도 안 쓴다
    taunt = copy.deepcopy(seqs[ROAR_SEQ])
    for fr in taunt:
        for sprites in fr:
            for s in sprites:
                if s[0] & 0x7F == EFFECT_PART and s[1] in remap:
                    s[1] = remap[s[1]]
    seqs.append(taunt)
    out = venom_effect.encode(header, parts, seqs)
    _h, again_parts, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[:SEQS_BEFORE] == before, "read back: the shipped sequences moved"
    assert len(again) == TAUNT_SEQ + 1 and len(again[TAUNT_SEQ]) == ROAR_FRAMES
    used = {s[1] for fr in again[TAUNT_SEQ] for d in fr for s in d if s[0] & 0x7F == EFFECT_PART}
    assert used and all(f >= FX_NEW for f in used), "every effect of 도발 is a tinted copy"
    flipped = {s[1] for fr in again[TAUNT_SEQ] for d in fr for s in d if s[0] == EFFECT_PART | 0x80}
    assert flipped <= set(again_parts[EFFECT_PART][0]), "a flipped copy is in the flip list"
    return out


def patch_sheet(mph, mpd):
    """(mph, mpd) of `hero/4/0` + the 12 tinted copies (46..57) of 사자후's effect pictures."""
    sheet = cs.sheet_decode(mph, mpd, SHEET)
    if len(sheet.frames) != FRAMES_BEFORE:
        raise ValueError(f"{SHEET}: {len(sheet.frames)} frames — expected the shipped {FRAMES_BEFORE}")
    palette = list(sheet.palette)
    index = {}
    for i, rgb in enumerate(palette):
        if i:  # 0 = 투명
            index.setdefault(tuple(rgb), i)
    out_frames = list(sheet.frames)
    for f in range(FX_FROM, FX_TO + 1):
        src = sheet.frames[f]
        px = []
        for row in src.pixels:
            line = []
            for v in row:
                if v == 0:
                    line.append(0)
                    continue
                rgb = tint(palette[v])
                if rgb not in index:
                    index[rgb] = len(palette)
                    palette.append(rgb)
                line.append(index[rgb])
            px.append(line)
        out_frames.append(cs.Frame(src.w, src.h, px, flags=src.flags, lead=src.lead, extra=src.extra))
    if len(palette) > 256:
        raise ValueError(f"{SHEET}: {len(palette)} colours")
    out = cs.Sheet(palette, out_frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out)
    back = cs.sheet_decode(new_mph, new_mpd, SHEET)
    assert [(f.w, f.h, f.pixels) for f in back.frames[:FRAMES_BEFORE]] == [(f.w, f.h, f.pixels) for f in sheet.frames], "old frames moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    assert len(back.frames) == FX_NEW + (FX_TO - FX_FROM + 1)
    return new_mph, new_mpd


# ── 아이콘: img/gmico 프레임 0 (버서커 띠) — 칸 7 · 11 ──────────────────────────────────────────────
ICON_STRIP, ICON = 0, 12
ICON_CELLS_BEFORE, ICON_CELLS = 7, 12
ICON_AT = (7, 11)  # 스킬 창 = 칸 7 (`s.a(II)V` 오른 열 넷째) · 상세 창·버프 HUD = 칸 11 (`cj.a = 15 − 4`)
WHITE, PALE, GOLD = (0xFF, 0xFF, 0xFF), (0xF0, 0xD8, 0xFF), (0xFF, 0xD0, 0x40)


def icon_pixels():
    """12×12: 진홍 → 검붉은 바탕 위, 네 방향에서 가운데로 몰려드는 흰 화살촉 넷 + 가운데 금빛 표적(끌어당김)."""
    c = 5.5
    rows = []
    # 바탕은 세 단계(진홍 → 검붉은) — `img/gmico` 의 팔레트는 다른 직업의 칸까지 247 색이라 9 색밖에 안 남는다
    shades = ((0xB0, 0x30, 0x60), (0x78, 0x1C, 0x40), (0x40, 0x0A, 0x22))
    for y in range(ICON):
        row = []
        for x in range(ICON):
            k = ((x - c) ** 2 + (y - c) ** 2) ** 0.5 / 7.5
            row.append(shades[0 if k < 0.45 else 1 if k < 0.8 else 2])
        rows.append(row)
    heads = {}
    # 네 화살촉: 바깥 줄이 넓고(옅은 라벤더 테) 가운데 쪽으로 좁아진다(흰 심) — 위·아래·왼·오른에서 가운데를 가리킨다
    for depth, (lo, hi) in enumerate(((3, 8), (4, 7), (5, 6))):
        for t in range(lo, hi + 1):
            rgb = PALE if depth == 0 and t in (lo, hi) else WHITE
            for x, y in ((t, depth), (t, 11 - depth), (depth, t), (11 - depth, t)):
                heads[(x, y)] = rgb
    # 가운데 표적
    for xy in ((5, 5), (6, 5), (5, 6), (6, 6)):
        heads[xy] = GOLD
    for xy in ((4, 4), (7, 4), (4, 7), (7, 7)):
        heads[xy] = (0xFF, 0x80, 0x30)
    for (x, y), rgb in heads.items():
        rows[y][x] = rgb
    return rows


def patch_icon(mph, mpd):
    """(mph, mpd) of `img/gmico` (the other classes' cells already in) with the berserker strip widened to 12 cells and
    도발 in cells 7 and 11. 반전 없는 시트(head 0x08000000)라 `sheet_encode(any_head=True)`."""
    sheet = cs.sheet_decode(mph, mpd, ICON_SHEET)
    strip = sheet.frames[ICON_STRIP]
    if (strip.w, strip.h) != (ICON_CELLS_BEFORE * ICON, ICON):
        raise ValueError(f"{ICON_SHEET} frame {ICON_STRIP}: {strip.w}×{strip.h} — expected the berserker's seven cells")
    palette = list(sheet.palette)
    index = {}
    for i, rgb in enumerate(palette):
        if i:
            index.setdefault(tuple(rgb), i)
    width = ICON_CELLS * ICON
    pixels = [list(row) + [0] * (width - strip.w) for row in strip.pixels]
    for cell in ICON_AT:
        for y, row in enumerate(icon_pixels()):
            for x, rgb in enumerate(row):
                if rgb not in index:
                    index[rgb] = len(palette)
                    palette.append(rgb)
                pixels[y][ICON * cell + x] = index[rgb]
    if len(palette) > 256:
        raise ValueError(f"{ICON_SHEET}: {len(palette)} colours (the icon brings {len(palette) - len(sheet.palette)})")
    frames = list(sheet.frames)
    frames[ICON_STRIP] = cs.Frame(width, strip.h, pixels, flags=strip.flags, lead=strip.lead, extra=strip.extra)
    out = cs.Sheet(palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out, any_head=True)
    back = cs.sheet_decode(new_mph, new_mpd, ICON_SHEET)
    for k, (a, b) in enumerate(zip(back.frames, sheet.frames)):
        if k != ICON_STRIP:
            assert (a.w, a.h, a.pixels, a.flags) == (b.w, b.h, b.pixels, b.flags), f"{ICON_SHEET} frame {k} moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    for y in range(ICON):
        assert back.frames[ICON_STRIP].pixels[y][: strip.w] == strip.pixels[y], "the seven shipped cells moved"
    assert back.frames[ICON_STRIP].w == width
    return new_mph, new_mpd


def cmd_preview(out_png, zip_path):
    from PIL import Image, ImageDraw

    import hero_cape

    jar = cs.open_jar(zip_path)
    mph, mpd = patch_sheet(jar.read(SHEET + ".mph"), jar.read(SHEET + "_0.mpd"))
    fx = cs.sheet_rgba(cs.sheet_decode(mph, mpd, SHEET))
    blob = jar.read(ANIM)
    if len(venom_effect.decode(blob)[2]) == SEQS_BEFORE:
        try:
            blob, _n = hero_cape.patch(blob)
        except ValueError:
            pass
        blob = patch_anim(blob)
    _h, _p, seqs = venom_effect.decode(blob)
    table = {p: cs.sheet_by_path(jar, b) for p, b in cs.PREVIEW_GEAR[0].items()}
    for part in (1, 2, 3):
        table[part] = cs.sheet_by_path(jar, f"hero/{part}/100")
    table[EFFECT_PART] = fx
    scale, cw, ch, ox, oy = 3, 96, 96, 48, 60
    rows = [(ROAR_SEQ, f) for f in range(len(seqs[ROAR_SEQ]))] + [(TAUNT_SEQ, f) for f in range(len(seqs[TAUNT_SEQ]))]
    half = len(rows) // 2
    img = Image.new("RGB", (2 * 4 * cw * scale + 14 * ICON * 8, half * ch * scale), (27, 29, 35))
    dr = ImageDraw.Draw(img)
    for r, (s, f) in enumerate(rows):
        col, rr = divmod(r, half)
        for d in range(4):
            bx = (col * 4 + d) * cw
            for x, y, w, h, px in cs.compose(seqs[s][f][d], table, set()):
                for yy in range(h):
                    for xx in range(w):
                        p = px[yy][xx]
                        if p[3] == 0:
                            continue
                        X, Y = (bx + ox + x + xx) * scale, (rr * ch + oy + y + yy) * scale
                        if 0 <= X < img.width and 0 <= Y < img.height:
                            dr.rectangle([X, Y, X + scale - 1, Y + scale - 1], fill=p[:3])
        dr.text(((col * 4) * cw * scale + 4, rr * ch * scale + 4), f"seq {s} f {f}", fill=(255, 255, 0))
    icon_mph, icon_mpd = patch_icon(jar.read(ICON_SHEET + ".mph"), jar.read(ICON_SHEET + "_0.mpd"))
    strip = cs.sheet_rgba(cs.sheet_decode(icon_mph, icon_mpd, ICON_SHEET))[ICON_STRIP]
    for y in range(ICON):
        for x in range(strip[0]):
            p = strip[2][y][x]
            X, Y = 2 * 4 * cw * scale + 8 + x * 8, 8 + y * 8
            dr.rectangle([X, Y, X + 7, Y + 7], fill=p[:3])
    img.save(out_png)
    print(out_png, img.size)
    return 0


def main():
    args = sys.argv[1:]
    if args and args[0] == "preview" and len(args) in (2, 3):
        return cmd_preview(args[1], args[2] if len(args) == 3 else cs.DEFAULT_ZIP)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
