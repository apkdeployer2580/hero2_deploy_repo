"""Direction-aware garment projection and material boundaries for hero parts."""

import costume_sheets as cs
import venom_effect

STRIDE = 11
BANKS = 5
SKIN = [(95, 47, 47), (155, 91, 71), (207, 139, 111), (239, 175, 143), (255, 215, 183)]
# The white tights' own ramp (the PNGs' lavender whites, darkest = outline) — what a white stocking is made of.
WHITE = [(95, 95, 127), (127, 127, 159), (159, 159, 191), (191, 191, 223), (223, 223, 255), (255, 255, 255)]
# ★ 2026-10-10 (사용자): 「스타킹류와 구두류 아바타에 팬티가 보여야」 · 「하얀스타킹 … 흰색스타킹에 살색이 비치는듯한 색상(여기에
# 팬티쪽에 팬티가 비치는거야)」. Per set:
#   shoe     = (dark, light, rows) — painted up each foot from its sole
#   stocking = ('white', skin) — the PNG's white-tights shading in the WHITE ramp with `skin` of the leg under it showing
#              through (vC 98 had it the other way round: skin with a quarter of white — it read as bare legs)
#            | ('dark', fabric, amount) — skin under `amount` of a dark sheer fabric (vC 98's black stockings, kept)
#   panty    = (base, string) — a thong (`panty_cells`): the waist string in `string`, the piece down the middle in
#              `base`; under a white stocking it shows through the stocking (`PANTY_UNDER_WHITE` of white over it)
MATERIALS = {
    200: dict(shoe=((49, 39, 35), (139, 111, 79), 4)),
    201: dict(shoe=((119, 99, 95), (239, 227, 215), 2), stocking=('white', .30), panty=((167, 119, 215), (143, 95, 199))),
    207: dict(shoe=((31, 23, 31), (79, 55, 63), 2), stocking=('dark', (35, 27, 35), .58), panty=((27, 19, 27), (143, 31, 47))),
    209: dict(shoe=((23, 63, 63), (63, 159, 143), 2), stocking=('white', .30), panty=((63, 175, 167), (39, 143, 135))),
    # ★ 2026-10-10 (3) (사용자: *"백은의 사신 하얀스타킹이 흰스타킹과 흰 구두로 … 구두와 팬티, 스타킹 색상을 다르게해야해서, 스타킹만
    # 좀더 살색을 섞은 색으로 하고 구두와 팬티를 흰색으로"*): white heels and a white thong, the stocking alone warmer (45 % skin).
    210: dict(shoe=((143, 143, 175), (247, 247, 255), 2), stocking=('white', .45), panty=((251, 251, 255), (223, 223, 239))),
}
PANTY_UNDER_WHITE = .25
# The view each bank is drawn for (`patch_animation`: bank = view + 1; bank 0 is no view's — the front's drawing).
BANK_VIEW = ('front', 'back', 'front', 'side', 'side')
# ★ 2026-10-10 (2) (사용자: *"팬티가 너무 커 약간 t팬티처럼 되어야해"* — the first cut covered the hips from the band to the
# crotch): a thong. The string runs across the whole hip one row below the waistline; below it only a piece down the
# middle to the crotch, this many pixels either side of the middle row by row (the last one holds to the crotch): in
# front a little triangle, behind the thin strip of the T, from the side the gusset's narrow line.
THONG_HALF = {'front': (1.5, 1.0, 0.5), 'back': (0.5,), 'side': (1.0, 0.5)}


def mix(a, b, amount):
    return tuple(round(x * (1 - amount) + y * amount) for x, y in zip(a, b))


def from_rows(rows_list):
    palette, index, frames = [cs.TRANSPARENT_RGB], {}, []
    for rows in rows_list:
        pixels = []
        for row in rows:
            line = []
            for color in row:
                if color is not None and color not in index:
                    index[color] = len(palette)
                    palette.append(color)
                line.append(0 if color is None else index[color])
            pixels.append(line)
        frames.append(cs.Frame(len(rows[0]), len(rows), pixels))
    if len(palette) > 256:
        raise cs.SheetError('garment materials exceed the 255-color budget')
    return cs.Sheet(palette, frames)


def rows_of(sheet, k, w, h):
    rows = [[None] * w for _ in range(h)]
    if sheet is not None:
        f = sheet.frames[k if k < len(sheet.frames) else k % STRIDE]
        for y, row in enumerate(f.pixels):
            for x, v in enumerate(row):
                if v:
                    rows[y][x] = sheet.palette[v]
    return rows


def ramp(colors, t):
    """`colors` read as an even ramp at 0 <= t <= 1."""
    t = min(1.0, max(0.0, t)) * (len(colors) - 1)
    i = min(int(t), len(colors) - 2)
    return mix(colors[i], colors[i + 1], t - i)


def runs(row):
    """[(x0, x1)] — the row's opaque runs, left to right."""
    out, x0 = [], None
    for x, c in enumerate(row + [None]):
        if c is not None and x0 is None:
            x0 = x
        elif c is None and x0 is not None:
            out.append((x0, x - 1))
            x0 = None
    return out


def panty_cells(rows, top, view):
    """({(x, y)}, string row) — the thong in one leg frame: the string across the whole row below the waistline, then
    down the middle (`THONG_HALF[view]` either side of the parting) to the crotch — the row above the first one where
    the legs part (or four rows below the waistline when they never part, the closed-leg frames)."""
    split = next((y for y in range(top + 1, len(rows)) if len(runs(rows[y])) >= 2), None)
    if split is not None:
        crotch = split - 1
        (_, a), (b, _) = runs(rows[split])[:2]
        cx = (a + b) / 2
    else:
        crotch = min(top + 4, len(rows) - 1)
        line = runs(rows[min(top + 2, crotch)])
        cx = (line[0][0] + line[-1][1]) / 2 if line else 0
    string = min(top + 1, crotch)
    cells = {(x, string) for x in range(len(rows[string])) if rows[string][x] is not None}
    halves = THONG_HALF[view]
    for i, y in enumerate(range(string + 1, crotch + 1)):
        half = halves[min(i, len(halves) - 1)]
        cells.update((x, y) for x in range(len(rows[y])) if rows[y][x] is not None and abs(x - cx) <= half)
    return cells, string


def dress_legs(rows, material, number, view):
    """One leg frame in the set's materials (MATERIALS) for `view` — the PNG gives the silhouette and, for white
    tights, the shading; shoes, stocking and panty are laid on it here."""
    occupied = [(x, y) for y, row in enumerate(rows) for x, c in enumerate(row) if c is not None]
    if not occupied:
        return rows
    w = len(rows[0])
    top, bottom = min(y for x, y in occupied), max(y for x, y in occupied)
    waist = [x for x, y in occupied if y == top]
    center = (min(waist) + max(waist)) / 2
    feet = {side: max((y for x, y in occupied if (x > center) == side and y > top + 3), default=bottom)
            for side in (False, True)}
    shoe, stocking, panty = material.get('shoe'), material.get('stocking'), material.get('panty')
    pants, string = panty_cells(rows, top, view) if panty else (set(), None)
    out = [row[:] for row in rows]
    for x, y in occupied:
        c = rows[y][x]
        edge = any(nx < 0 or nx >= w or rows[y][nx] is None for nx in (x - 1, x + 1))
        level = 1 if edge else (4 if x < w / 2 else 3)
        skin = SKIN[level]
        # the PNG's brightness: the white tights' outline (95, 95, 127) → 0, white → 1
        t = (0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2] - 98.6) / (255 - 98.6)
        white = stocking and stocking[0] == 'white'
        if white:
            fabric = ramp(WHITE, t)
            dressed = mix(fabric, ramp(SKIN, t), stocking[1])
        elif stocking:
            dressed = mix(skin, stocking[1], stocking[2])
        else:
            dressed = skin
        if shoe and y >= max(top + 5, feet[x > center] - shoe[2] + 1):
            out[y][x] = shoe[0] if edge or y == feet[x > center] else shoe[1]
        elif number == 200:
            out[y][x] = c
        elif (x, y) in pants:
            base, band = panty
            cloth = band if y == string else base
            cloth = mix(cloth, (0, 0, 0), .35) if edge else (mix(cloth, (255, 255, 255), .12) if x < w / 2 else cloth)
            out[y][x] = mix(cloth, fabric, PANTY_UNDER_WHITE) if white else cloth
        else:
            out[y][x] = dressed
    return out


def footwear(sheet, number):
    """The set's leg PNG dressed in its materials — ★ 2026-10-10 a whole directional sheet (55 frames), because the
    panty is cut per view (BANK_VIEW); sets without materials come back as they are."""
    if number not in MATERIALS:
        return sheet
    material = MATERIALS[number]
    result = []
    for bank in range(BANKS):
        for k, f in enumerate(sheet.frames[:STRIDE]):
            result.append(dress_legs(rows_of(sheet, k, f.w, f.h), material, number, BANK_VIEW[bank]))
    return from_rows(result)


def duplicate(sheet):
    if len(sheet.frames) == STRIDE * BANKS:
        return sheet
    if len(sheet.frames) != STRIDE:
        raise cs.SheetError(f'legs require {STRIDE} source frames, got {len(sheet.frames)}')
    return cs.Sheet(sheet.palette, sheet.frames * BANKS, head=sheet.head, trns=sheet.trns)


def garment(legs, under=None, over=None, gown=None):
    result = []
    for bank in range(BANKS):
        for k, f in enumerate(legs.frames[:STRIDE]):
            body = rows_of(legs, bank * STRIDE + k, f.w, f.h)
            below = rows_of(under, k, f.w, f.h)
            above = rows_of(over, k, f.w, f.h)
            cloth = rows_of(gown, k, f.w, f.h) if gown else [
                [a or b for a, b in zip(ar, br)] for ar, br in zip(above, below)]
            active = [y for y, row in enumerate(cloth) if any(c is not None for c in row)]
            if not active:
                result.append(body)
                continue
            top, hem = min(active), max(active)
            waist = [x for x, c in enumerate(cloth[top]) if c is not None]
            center = (min(waist) + max(waist)) / 2
            if bank == 0:
                rows = cloth if gown else [[a or b or u for a, b, u in zip(ar, br, ur)]
                                           for ar, br, ur in zip(above, body, below)]
            elif bank in (1, 2):
                rows = [[c or b if gown or bank == 1 or y <= top else b or c
                         for c, b in zip(cr, br)] for y, (cr, br) in enumerate(zip(cloth, body))]
                if bank == 2 and not gown:
                    for y in range(top + 1, hem + 1):
                        for x, c in enumerate(above[y]):
                            if c and sum(v is not None for v in above[y]) <= 3:
                                rows[y][x] = c
            else:
                rows = [[None] * f.w for _ in range(f.h)] if gown else [row[:] for row in body]
                facing_left = bank == 3
                for y in active:
                    xs = [x for x, c in enumerate(cloth[y]) if c is not None]
                    width = max(xs) - min(xs) + 1
                    half = max(2, round(width * .32))
                    sway = (k % 3 - 1) * (y - top) / max(1, hem - top) * .65
                    lo = max(0, round(center - half + sway))
                    hi = min(f.w - 1, round(center + half + sway))
                    for x in range(lo, hi + 1):
                        u = (x - lo) / max(1, hi - lo)
                        sx = min(xs) + round(u * (width - 1))
                        c = cloth[y][sx]
                        if c is None:
                            continue
                        front_edge = x <= lo + 1 if facing_left else x >= hi - 1
                        if not gown and front_edge and y > top + 2:
                            rows[y][x] = rows[y][x] or c
                        else:
                            rows[y][x] = mix(c, (31, 27, 35), .15 if front_edge else .04)
            result.append(rows)
    return from_rows(result)


def short_skirt(sheet):
    legs, cloth = [], []
    for k, f in enumerate(sheet.frames[:STRIDE]):
        rows = rows_of(sheet, k, f.w, f.h)
        skin_colors = {(95, 31, 63), (127, 63, 63), (191, 95, 63), (191, 127, 63), (255, 159, 127), (255, 223, 191)}
        fabric = [[c if c not in skin_colors else None for c in row] for row in rows]
        legs.append([[None if fc is not None else c for c, fc in zip(row, fr)] for row, fr in zip(rows, fabric)])
        cloth.append(fabric)
    projected = garment(from_rows(legs), gown=from_rows(cloth))
    result = []
    for k, f in enumerate(projected.frames):
        rows = rows_of(projected, k, f.w, f.h)
        hem = max(y for y, row in enumerate(cloth[k % STRIDE]) if any(c is not None for c in row))
        for y in range(hem + 1, f.h):
            rows[y] = legs[k % STRIDE][y]
        result.append(rows)
    return from_rows(result)


def flipped_frames(seqs, part):
    """Every frame `part` is drawn mirrored with (`p | 0x80`) — what the part's flip list must name."""
    return {s[1] for seq in seqs for frame in seq for sprites in frame for s in sprites
            if s[0] & 127 == part and s[0] & 128 and s[1] != 255}


def patch_animation(blob):
    """Leg sprites → the bank of the view they face. ★ 2026-10-10: and the leg part's flip list follows them — the
    part loader `cf.a(I)V` makes a mirrored copy `cf.a[part + 8][f]` only of the frames in that list (`cf.b[part]`,
    the header's first list) and the drawer skips a null image, so a flipped frame the list does not name is simply
    not drawn. The first version moved the frames to 11..54 and left the list at the shipped 0..10: every mirrored
    leg pose (walking up/down every fourth frame, standing facing up, every pose facing right) had no legs."""
    header, parts, seqs = venom_effect.decode(blob)
    for seq in seqs:
        for frame in seq:
            for direction, sprites in enumerate(frame):
                view = direction
                head = next((s for s in sprites if s[0] & 127 == 0 and s[1] in (0, 1, 2)), None)
                if head is not None:
                    view = {0: 0, 2: 1}.get(head[1], 3 if head[0] & 128 else 2)
                for sprite in sprites:
                    p, f, x, y = sprite
                    if p & 127 != 3 or f == 255:
                        continue
                    bank = view + 1
                    if view >= 2 and p & 128:
                        bank = 7 - bank
                    sprite[1] = f % STRIDE + bank * STRIDE
    parts[3] = (bytes(sorted(flipped_frames(seqs, 3))), parts[3][1])
    return venom_effect.encode(header, parts, seqs)
