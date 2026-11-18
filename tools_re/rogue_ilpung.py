#!/usr/bin/env python3
"""로그의 새 스킬 「일풍격」 — 그림과 애니메이션 (2026-10-05).

사용자(2026-10-05): *"로그의 스킬을 한개 추가할꺼야. 일풍격 이라는 스킬을 추가할꺼야. 일풍격은 … 가장 가까운 적에게 순간이동하여
주변을 한바퀴 돌며 단검을 투척하는 스킬이야 … 단검 투척은 캐릭터 기준 8방향 2칸씩 데미지 … 단검을 투척하는 듯한 이펙트와 한바퀴
빙글 도는듯한 이펙트를 주고싶어."* · 물어서: 탐색 사방 20칸 · 이동할 곳이 없으면 시전 안 됨 · 수치 = 신의가호 발도와 같게(쿨·SP 포함) ·
16칸 안의 적은 여러 번 맞을 수 있다. 범위·수치·순간이동은 코드(`guest_anchors::IlpungPatch`).

── 자리 (HERO2_GUEST_RE.md §8.48) ───────────────────────────────────────────────────────────────────
* 로그의 `a.p` 15칸(콤보 5 + 스킬 10)은 빈 곳이 없다 → 일풍격 = **`a.p[15]`**(배열을 16칸으로 — `a.b()V`) = 종류 15.
  동작 = `hero/hero2` 시퀀스 **19**(= 종류 15 + 4 · 동작 길이 `a.n[15]` 도 이 시퀀스의 프레임 수에서 나온다 — `cf.a()V`).
  그래서 시퀀스 18(= 종류 14, 아무도 안 쓴다)을 빈 한 프레임으로 먼저 덧붙인다. 배포본 17 + 맹독감염(venom_effect) 17 = 18 시퀀스 뒤.
  ⛔ 시퀀스 15(발밑 물결 = 민첩태세 버프 그림)·16(쓰러짐)은 건드리지 않는다 — 종류 11 이 아니라 15 를 쓰는 이유다.
* 순서: venom_effect → hero_cape → **이것**. 회전 포즈는 다른 방향의 몸을 그 방향 칸에 그대로 옮기므로, 망토가 이미 붙은 포즈를
  옮겨야 망토가 몸을 따라간다(hero_cape 는 방향 칸마다 망토 자리를 정한다).

── 그림 ──────────────────────────────────────────────────────────────────────────────────────────────
* 프레임 10: f0 = 도착(서기 자세 + 발밑 연기) · f1..f8 = 한 바퀴(바라보는 쪽에서 시계 방향으로 8방향 하나씩 단검을 던진다 —
  몸 = 일격(시퀀스 9) 프레임 1·2 의 「양팔 벌림」을 그 방향의 칸에서 빌린다 · 회전 궤적 = 던지는 쪽에 은빛 호) · f9 = 마무리.
  타격은 코드가 f1..f8 에서 그 프레임의 방향 2칸을 친다(`IlpungPatch::THROWS`).
* 단검 = 8방향 그림 8장(은빛 칼날 · 금빛 코등이 · 붉은 손잡이) — 던진 프레임에서 8 px, 다음 20 px, 그다음 32 px(= 2칸) 날아간다.
* 회전 궤적 = 8방향 호 8장(흰 앞머리 → 하늘빛 꼬리, 눕힌 타원 위의 135°) — 위쪽 반은 몸 뒤, 아래쪽 반은 몸 앞.
* 도착 연기 = 눕힌 타원 둘(큰 것 · 작은 것) — 몸 뒤.
* 아이콘 = `img/gmico` 프레임 2(로그 띠 72×12 = 6칸)를 132×12(11칸)로 넓혀 칸 6(스킬 창의 7번째 칸)과 칸 10(상세 창 `cj` 는
  `인덱스 − 콤보 수` = 10 번 칸을 그린다) 둘에 같은 그림. 7..9 는 비운다.

    python tools_re/rogue_ilpung.py preview <out.png> [zip]   # 시퀀스 19 를 네 방향으로 + 아이콘
"""

import copy
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402
import venom_effect  # noqa: E402

ANIM = "hero/hero2"
SHEET = "hero/4/2"
ICON_SHEET = "img/gmico"
EFFECT_PART = 4
TILE = 16
SEQS_BEFORE = 18  # 배포본 17 + venom_effect 의 17
FILLER_SEQ = 18  # = 종류 14 — 아무도 안 쓴다
ILPUNG_SEQ = 19  # = guest_anchors::IlpungPatch::KIND (15) + 4
FRAMES_BEFORE = 43  # hero/4/2 배포본
STRIKE_SEQ, STRIKE_POSES = 9, (1, 2)  # 일격 — 양팔을 벌린 두 프레임
IDLE_SEQ = 0
# = IlpungPatch::{FRAMES, THROWS}
ILPUNG_FRAMES = 10
THROWS = 8
BODY_LIFT = -6  # 몸통 가운데 = 칸 가운데보다 6 px 위 (knight_blessing.BLAST_LIFT)
FEET_LIFT = 4
NO_EXTRA_FLAGS = b"\x88\x20"  # 이 시트에서도 IHDR · IDAT 만 있는 프레임의 플래그 (배포본 네 장 — knight_blessing 의 hero/4/1 과 같다)

# 8방향(화면 좌표, y 아래) — 북쪽부터 시계 방향. = IlpungPatch::DIR8
DIR8 = ((0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1))
# 애니메이션의 방향 칸(0 위 · 1 아래 · 2 왼 · 3 오른) → 한 바퀴의 첫 방향. = IlpungPatch::START8 (facing − 1 번째)
START8 = (0, 4, 6, 2)
# 던지는 방향 → 몸 자세를 빌릴 방향 칸
POSE_SLOT = (0, 3, 3, 1, 1, 2, 2, 0)
DAGGER_REACH = (8, 20, 32)  # 던진 프레임 · 다음 · 그다음 (px)

SILVER, STEEL, WHITE = (0xD8, 0xE0, 0xE8), (0x8F, 0x9F, 0xAF), (0xFF, 0xFF, 0xFF)
GOLD, HANDLE, DARK = (0xE8, 0xB8, 0x30), (0x9F, 0x2F, 0x1F), (0x3F, 0x1F, 0x1F)
SKY, PALE = (0x9F, 0xDF, 0xFF), (0xCF, 0xEF, 0xFF)
SMOKE, SMOKE_D = (0x6F, 0x5F, 0x7F), (0x3F, 0x37, 0x4F)


def _s8(v):
    return v - 256 if v > 127 else v


def _sprite(frame, x, y):
    if not (-128 <= x <= 127 and -128 <= y <= 127):
        raise ValueError(f"effect {frame} at ({x},{y}) does not fit a signed byte")
    return [EFFECT_PART, frame, x & 0xFF, y & 0xFF]


def _cropped(rows):
    """칠해진 곳만 남긴 (w, h, rows, (원래 가운데에서 잘린 왼쪽 위까지 x, y))."""
    ys = [y for y, row in enumerate(rows) if any(c is not None for c in row)]
    xs = [x for row in rows for x, c in enumerate(row) if c is not None]
    if not ys:
        raise ValueError("an empty picture")
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    return x1 - x0 + 1, y1 - y0 + 1, [row[x0 : x1 + 1] for row in rows[y0 : y1 + 1]], (x0, y0)


def _dagger(d):
    """방향 d 로 날아가는 단검 하나 — 13×13 판의 가운데 기준, 칼끝이 d 쪽."""
    n = 13
    c = (n - 1) / 2.0
    ux, uy = DIR8[d]
    m = math.hypot(ux, uy)
    ux, uy = ux / m, uy / m
    px, py = -uy, ux  # 칼날의 폭 방향
    rows = [[None] * n for _ in range(n)]

    def put(t, s, colour):
        x, y = round(c + ux * t + px * s), round(c + uy * t + py * s)
        if 0 <= x < n and 0 <= y < n:
            rows[y][x] = colour

    for k in range(-10, 13):
        t = k / 2.0
        if t > 4.5:
            continue
        if t >= 4.0:
            put(t, 0, WHITE)  # 칼끝
        elif t >= 0.0:
            put(t, 0, SILVER)
            put(t, 0.6 if t < 3 else 0, STEEL)  # 칼날의 어두운 날
        elif t >= -1.0:
            for s in (-1.5, -1.0, 0, 1.0, 1.5):
                put(t, s, GOLD)  # 코등이
        elif t >= -4.0:
            put(t, 0, HANDLE)
        else:
            put(t, 0, DARK)  # 자루 끝
    return rows


def _arc(d):
    """던지는 쪽 d 에 걸친 회전 궤적 — 눕힌 타원(가로 반지름 19 · 세로 14) 위 135°, 흰 앞머리 → 하늘빛 꼬리 (시계 방향으로 돈다)."""
    rx, ry, thick = 19.0, 14.0, 3.0
    w, h = 2 * int(rx) + 5, 2 * int(ry) + 5
    cx, cy = (w - 1) / 2.0, (h - 1) / 2.0
    lead = math.atan2(DIR8[d][1], DIR8[d][0])  # 화면 각(y 아래 = 양) — 앞머리
    span = math.radians(135)
    rows = [[None] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            dx, dy = (x - cx) / rx, (y - cy) / ry
            r = math.hypot(dx, dy)
            depth = (1.0 - r) * min(rx, ry)
            if not 0 <= depth < thick:
                continue
            a = math.atan2(dy, dx)
            back = (lead - a) % (2 * math.pi)  # 앞머리에서 거꾸로(반시계) 얼마나
            if back > span:
                continue
            u = back / span  # 0 앞머리 … 1 꼬리
            if u < 0.12:
                colour = WHITE
            elif u < 0.45:
                colour = PALE if depth < thick - 1 else WHITE
            elif u < 0.8:
                colour = SKY
            else:
                colour = SKY if (x + y) % 2 == 0 else None  # 꼬리는 성기게
            if colour is not None:
                rows[y][x] = colour
    return rows, (cx, cy)


def _puff(rx, ry):
    w, h = 2 * rx + 3, 2 * ry + 3
    cx, cy = (w - 1) / 2.0, (h - 1) / 2.0
    rows = [[None] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            r = math.hypot((x - cx) / rx, (y - cy) / ry)
            if 0.62 <= r <= 1.0:
                rows[y][x] = SMOKE if r > 0.82 or (x * 3 + y) % 4 else SMOKE_D
    return rows, (cx, cy)


def new_frames():
    """[(w, h, rows, (가운데 x, 가운데 y) — 잘린 그림 안의 기준점)]: 단검 8 · 궤적 8 · 연기 2."""
    out = []
    for d in range(8):
        w, h, rows, (x0, y0) = _cropped(_dagger(d))
        out.append((w, h, rows, (6 - x0, 6 - y0)))
    for d in range(8):
        rows, (cx, cy) = _arc(d)
        w, h, cut, (x0, y0) = _cropped(rows)
        out.append((w, h, cut, (cx - x0, cy - y0)))
    for rx, ry in ((15, 6), (11, 4)):
        rows, (cx, cy) = _puff(rx, ry)
        w, h, cut, (x0, y0) = _cropped(rows)
        out.append((w, h, cut, (cx - x0, cy - y0)))
    return out


DAGGER = tuple(range(FRAMES_BEFORE, FRAMES_BEFORE + 8))
ARC = tuple(range(FRAMES_BEFORE + 8, FRAMES_BEFORE + 16))
PUFF_BIG, PUFF_SMALL = FRAMES_BEFORE + 16, FRAMES_BEFORE + 17


def _at(frame_no, cx, cy, frames):
    """그림 frame_no 의 기준점을 (cx, cy) 에 놓는 스프라이트."""
    w, h, _rows, (ax, ay) = frames[frame_no - FRAMES_BEFORE]
    return _sprite(frame_no, round(cx - ax), round(cy - ay))


def _throw_dir(slot, i):
    return (START8[slot] + i) % 8


def frame_sprites(slot, f, seqs, frames):
    """시퀀스 19 의 프레임 f, 방향 칸 slot 의 스프라이트 목록."""
    under, over = [], []
    if f == 0:
        under.append(_at(PUFF_BIG, 0, FEET_LIFT, frames))
        body = [list(s) for s in seqs[IDLE_SEQ][0][slot]]
    elif f <= THROWS:
        i = f - 1
        d = _throw_dir(slot, i)
        pose = seqs[STRIKE_SEQ][STRIKE_POSES[i % 2]][POSE_SLOT[d]]
        body = [list(s) for s in pose if s[0] & 0x7F != EFFECT_PART]
        arc = _at(ARC[d], 0, BODY_LIFT, frames)
        (under if DIR8[d][1] < 0 else over).append(arc)
    else:
        under.append(_at(PUFF_SMALL, 0, FEET_LIFT, frames))
        body = [list(s) for s in seqs[IDLE_SEQ][0][slot]]
    # 날아가는 단검: 지금 던진 것 · 한 프레임 전 · 두 프레임 전
    for j, reach in enumerate(DAGGER_REACH):
        i = f - 1 - j
        if not 0 <= i < THROWS:
            continue
        d = _throw_dir(slot, i)
        ux, uy = DIR8[d]
        m = math.hypot(ux, uy)
        dagger = _at(DAGGER[d], reach * ux / m, reach * uy / m + BODY_LIFT, frames)
        (under if uy < 0 else over).append(dagger)
    return under + body + over


def patch_anim(blob):
    """`hero/hero2`(venom_effect · hero_cape 뒤 = 18 시퀀스) + 시퀀스 18(빈 한 프레임) · 19(일풍격 10 프레임)."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != SEQS_BEFORE or len(seqs[STRIKE_SEQ]) != 4 or len(seqs[IDLE_SEQ]) != 4:
        raise ValueError(f"{ANIM}: {len(seqs)} sequences — expected the rogue animation after venom_effect ({SEQS_BEFORE})")
    frames = new_frames()
    seqs.append([[[], [], [], []]])  # 18: 종류 14 — 아무도 안 쓴다
    spin = [[frame_sprites(slot, f, seqs, frames) for slot in range(4)] for f in range(ILPUNG_FRAMES)]
    for fr in spin:
        for sprites in fr:
            if len(sprites) > 255:
                raise ValueError("a pose would have more than 255 sprites")
    seqs.append(spin)
    out = venom_effect.encode(header, parts, seqs)
    _h, _p, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[:SEQS_BEFORE] == before, "read back: the shipped sequences moved"
    assert len(again) == ILPUNG_SEQ + 1 and len(again[ILPUNG_SEQ]) == ILPUNG_FRAMES
    return out


def patch_sheet(mph, mpd):
    """(mph, mpd) of `hero/4/2` + the 18 new pictures (43..60)."""
    sheet = cs.sheet_decode(mph, mpd, SHEET)
    if len(sheet.frames) != FRAMES_BEFORE:
        raise ValueError(f"{SHEET}: {len(sheet.frames)} frames — expected the shipped {FRAMES_BEFORE}")
    palette = list(sheet.palette)
    index = {}
    for i, rgb in enumerate(palette):
        if i:  # 0 = 투명 — 진짜 흰색은 0 이 아닌 칸에
            index.setdefault(tuple(rgb), i)
    out_frames = list(sheet.frames)
    for w, h, rows, _anchor in new_frames():
        px = []
        for row in rows:
            line = []
            for rgb in row:
                if rgb is None:
                    line.append(0)
                    continue
                if rgb not in index:
                    index[rgb] = len(palette)
                    palette.append(rgb)
                line.append(index[rgb])
            px.append(line)
        out_frames.append(cs.Frame(w, h, px, flags=NO_EXTRA_FLAGS))
    if len(palette) > 256:
        raise ValueError(f"{SHEET}: {len(palette)} colours")
    out = cs.Sheet(palette, out_frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out)
    back = cs.sheet_decode(new_mph, new_mpd, SHEET)
    assert [(f.w, f.h, f.pixels) for f in back.frames[:FRAMES_BEFORE]] == [(f.w, f.h, f.pixels) for f in sheet.frames], "old frames moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    return new_mph, new_mpd


# ── 아이콘: img/gmico 프레임 2 (로그 띠) — 칸 6 · 10 ────────────────────────────────────────────────
ICON_STRIP, ICON = 2, 12
ICON_CELLS_BEFORE, ICON_CELLS = 6, 11
ICON_AT = (6, 10)  # 스킬 창 = 칸 6 (`s.a(II)V` 오른 열 셋째) · 상세 창 = 칸 10 (`cj.a = 15 − 5`)
ICON_BG = (0x2F, 0x3F, 0x5F)


def icon_pixels():
    """12×12: 남색 바탕 위 바람개비처럼 네 방향으로 날아가는 단검 + 하얀 회전 호."""
    c = 5.5
    rows = []
    for y in range(ICON):
        row = []
        for x in range(ICON):
            k = min(1.0, math.hypot(x - c, y - c) / 7.5)  # 가운데 밝은 남색 → 가장자리 어두운 남색
            row.append(tuple(round(a + (b - a) * k) for a, b in zip((0x4F, 0x6F, 0xA7), (0x17, 0x1F, 0x37))))
        rows.append(row)
    for y in range(ICON):
        for x in range(ICON):
            r = math.hypot(x - c, y - c)
            a = math.atan2(y - c, x - c) % (math.pi / 2)  # 네 갈래 바람개비 호 — 각 갈래의 앞머리가 밝다
            if 3.4 <= r <= 4.9 and a < 1.1:
                rows[y][x] = WHITE if a < 0.35 else (PALE if a < 0.75 else SKY)
    blades = {
        (5, 0): WHITE, (5, 1): SILVER, (5, 2): SILVER, (6, 3): GOLD, (4, 3): GOLD,  # 위로
        (11, 5): WHITE, (10, 5): SILVER, (9, 5): SILVER, (8, 6): GOLD, (8, 4): GOLD,  # 오른쪽
        (6, 11): WHITE, (6, 10): SILVER, (6, 9): SILVER, (5, 8): GOLD, (7, 8): GOLD,  # 아래로
        (0, 6): WHITE, (1, 6): SILVER, (2, 6): SILVER, (3, 5): GOLD, (3, 7): GOLD,  # 왼쪽
        (5, 5): HANDLE, (6, 6): HANDLE, (5, 6): DARK, (6, 5): DARK,
    }
    for (x, y), rgb in blades.items():
        rows[y][x] = rgb
    return rows


def patch_icon(mph, mpd):
    """(mph, mpd) of `img/gmico` (나이트의 발도 칸이 이미 들어간 것이어도 된다) with the rogue strip widened to 11 cells and
    일풍격 in cells 6 and 10. 반전 없는 시트(head 0x08000000)라 `sheet_encode(any_head=True)`."""
    sheet = cs.sheet_decode(mph, mpd, ICON_SHEET)
    strip = sheet.frames[ICON_STRIP]
    if (strip.w, strip.h) != (ICON_CELLS_BEFORE * ICON, ICON):
        raise ValueError(f"{ICON_SHEET} frame {ICON_STRIP}: {strip.w}×{strip.h} — expected the rogue's six cells")
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
        raise ValueError(f"{ICON_SHEET}: {len(palette)} colours")
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
        assert back.frames[ICON_STRIP].pixels[y][: strip.w] == strip.pixels[y], "the six shipped cells moved"
    assert back.frames[ICON_STRIP].w == width
    return new_mph, new_mpd


def cmd_preview(out_png, zip_path):
    from PIL import Image, ImageDraw

    import hero_cape

    jar = cs.open_jar(zip_path)
    mph, mpd = patch_sheet(jar.read(SHEET + ".mph"), jar.read(SHEET + "_0.mpd"))
    fx = cs.sheet_rgba(cs.sheet_decode(mph, mpd, SHEET))
    blob = jar.read(ANIM)
    if len(venom_effect.decode(blob)[2]) == 17:
        blob = venom_effect.patch(blob)
    if len(venom_effect.decode(blob)[2]) == SEQS_BEFORE:
        try:
            blob, _n = hero_cape.patch(blob)
        except ValueError:
            pass
        blob = patch_anim(blob)
    _h, _p, seqs = venom_effect.decode(blob)
    table = {p: cs.sheet_by_path(jar, b) for p, b in cs.PREVIEW_GEAR[2].items()}
    for part in (1, 2, 3):
        table[part] = cs.sheet_by_path(jar, f"hero/{part}/100")
    table[EFFECT_PART] = fx
    scale, cw, ch, ox, oy = 3, 120, 112, 60, 64
    rows = [(ILPUNG_SEQ, f) for f in range(len(seqs[ILPUNG_SEQ]))]
    img = Image.new("RGB", (4 * cw * scale + 16 * ICON * scale, len(rows) * ch * scale), (27, 29, 35))
    dr = ImageDraw.Draw(img)
    for r, (s, f) in enumerate(rows):
        for d in range(4):
            for gx in range(-3, 4):
                for gy in range(-3, 4):
                    x0 = (d * cw + ox + gx * TILE - TILE // 2) * scale
                    y0 = (r * ch + oy + gy * TILE - TILE // 2) * scale
                    hit = (gx, gy) != (0, 0) and max(abs(gx), abs(gy)) <= 2 and (gx == 0 or gy == 0 or abs(gx) == abs(gy))
                    dr.rectangle([x0, y0, x0 + TILE * scale - 1, y0 + TILE * scale - 1], outline=(70, 90, 120) if hit else (45, 48, 56))
            if 1 <= f <= THROWS:
                dx, dy = DIR8[_throw_dir(d, f - 1)]
                for t in (1, 2):
                    x0 = (d * cw + ox + t * dx * TILE - TILE // 2) * scale
                    y0 = (r * ch + oy + t * dy * TILE - TILE // 2) * scale
                    dr.rectangle([x0, y0, x0 + TILE * scale - 1, y0 + TILE * scale - 1], outline=(200, 80, 80))
            for x, y, w, h, px in cs.compose(seqs[s][f][d], table, {5}):
                for yy in range(h):
                    for xx in range(w):
                        p = px[yy][xx]
                        if p[3] == 0:
                            continue
                        X, Y = (d * cw + ox + x + xx) * scale, (r * ch + oy + y + yy) * scale
                        if 0 <= X < img.width and 0 <= Y < img.height:
                            dr.rectangle([X, Y, X + scale - 1, Y + scale - 1], fill=p[:3])
        dr.text((4, r * ch * scale + 4), f"seq {s} f {f}", fill=(255, 255, 0))
    icon_mph, icon_mpd = patch_icon(jar.read(ICON_SHEET + ".mph"), jar.read(ICON_SHEET + "_0.mpd"))
    strip = cs.sheet_rgba(cs.sheet_decode(icon_mph, icon_mpd, ICON_SHEET))[ICON_STRIP]
    for y in range(ICON):
        for x in range(strip[0]):
            p = strip[2][y][x]
            X, Y = 4 * cw * scale + 8 + x * 8, 8 + y * 8
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
