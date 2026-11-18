#!/usr/bin/env python3
"""로그 「살상연희」 — 번개난타 자리의 새 스킬, 그림과 애니메이션 (2026-10-06).

사용자(2026-10-06): *"번개난타를 바꿀수도 있어? 살상연희 스킬로 바꾸고싶어. 살상연희는 잠시동안 반투명 상태로 주변 5x5(캐릭터 위치
중앙 기준)타일 내의 가장 가까운 순서대로 적의 뒤로 순간이동하며 공격을 하는 스킬이고 총 8마리의 적을 공격할 수있어 … 이펙트는 반투명
잔상을 남기며 순간이동해서 적을 칼로 긋는 듯한"* (연출). 범위 · 순간이동 · 타격 · 무적 · 잔상 · 반투명은 코드(`guest_anchors::RoguePatch`
— 드라이브가 프레임 3h 에 옮기고 3h+1 에 친다, 그리기 래퍼가 몸을 반투명으로 · 잔상을 그 뒤에 · 이 시퀀스 20 을 그 위에 불투명으로).

── 자리 (HERO2_GUEST_RE.md §8.49) ───────────────────────────────────────────────────────────────────
* 살상연희 = 번개난타의 칸 `a.p[10]` = 종류 10 → 동작 = `hero/hero2` 시퀀스 **14**(= 종류 + 4). 길이 `a.n[10]` 은 이 시퀀스의
  프레임 수에서 나온다(`cf.a()V` — 로그·나이트는 `a.n = new byte[시퀀스 수 − 4]`, `a.n[i − 4] = 프레임 수`) → **25 프레임** =
  8 번 × 3 프레임(도착 · 벤다 · 뒤따름) + 마무리 1. 타수가 적으면 드라이브가 `g.w ← a.n[10] − 1` 로 마무리 프레임에 건너뛴다.
* 베는 그림만 = 시퀀스 **20**(= `RoguePatch::SLASH_SEQ`, 25 프레임 — 몸과 프레임마다 짝) — 그리기 래퍼가 반투명 몸 위에 불투명으로
  `n.a(BBBII)V(20, g.w, 방향, x, y)`. 시퀀스가 21 개가 되어 `a.n` 이 17 칸이 되지만 그 끝 칸은 아무도 안 읽는다(종류는 0..15).
* 순서: venom_effect → hero_cape → rogue_ilpung → **이것** (시퀀스 0..19 뒤에 20 을 덧붙이고, 14 는 그 자리에서 새로 쓴다 — 몸은
  망토가 이미 붙은 배포본 14 의 포즈를 빌린다).

── 그림 ──────────────────────────────────────────────────────────────────────────────────────────────
* 몸(시퀀스 14) = 배포본 번개난타(시퀀스 14)의 포즈를 효과(파트 4 — 번개)만 빼고: 도착 = f0(겨누기) · 벤다 = f1 · f3 · f6 을 돌아가며 ·
  뒤따름 = f2 · f4 · f7 · 마무리 = f7.
* 베기(시퀀스 20) = 바라보는 쪽 칸을 가로지르는 칼 자국 — 흰 심 · 옅은 하늘빛 날 · 보랏빛 테, 바라보는 쪽으로 살짝 휜 초승달 줄.
  번마다 기울기가 번갈아(＼ · ／) — 방향 넷 × 기울기 둘 × (벤다 · 뒤따름 = 옅어진 자국) = 16 장(미리 돌린 그림 — 반전 플래그 안 씀).
  도착 프레임에는 몸 둘레에 작은 반짝임 셋(순간이동의 번쩍임).
* 아이콘 = `img/gmico` 프레임 2(로그 띠, rogue_ilpung 뒤 11칸)의 칸 **5**(번개난타 — 스킬 창 · 상세 창 `cj` 의 `10 − 5` 둘 다).

    python tools_re/rogue_slaughter.py preview <out.png> [zip]   # 시퀀스 14 + 20 을 네 방향으로 + 아이콘
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402
import rogue_ilpung  # noqa: E402
import venom_effect  # noqa: E402

ANIM = "hero/hero2"
SHEET = "hero/4/2"
ICON_SHEET = "img/gmico"
EFFECT_PART = 4
TILE = 16
SEQS_BEFORE = 20  # 배포본 17 + venom_effect 의 17 + rogue_ilpung 의 18 · 19
SLAUGHTER_SEQ = 14  # = guest_anchors::RoguePatch::SEQ (종류 10 + 4)
SLASH_SEQ = 20  # = RoguePatch::SLASH_SEQ
# = RoguePatch::{SEQ_FRAMES, HOP_FRAMES, TOTALS[5]}
FRAMES = 25
HOP_FRAMES = 3
HOPS = 8
FRAMES_BEFORE = 61  # hero/4/2 — 배포본 43 + rogue_ilpung 18
SHIPPED_FRAMES = 8  # 배포본 시퀀스 14 (번개난타)
ARRIVE_POSE = 0
SLASH_POSES = (1, 3, 6)
FOLLOW_POSES = (2, 4, 7)
END_POSE = 7
BODY_LIFT = -6  # 몸통 가운데 = 칸 가운데보다 6 px 위 (rogue_ilpung 과 같다)
CUT_REACH = 13  # 칼 자국의 가운데 = 몸통 가운데에서 바라보는 쪽으로 13 px (옆 칸 가운데는 16)
CUT_LEN = 24
CUT_TILT = 38  # 바라보는 쪽에 직각인 줄에서 ± 이만큼 (도)
CUT_BULGE = 3.0
NO_EXTRA_FLAGS = rogue_ilpung.NO_EXTRA_FLAGS

# 방향 칸(0 위 · 1 아래 · 2 왼 · 3 오른) → 바라보는 쪽 단위 벡터 (화면 좌표, y 아래)
FACING = ((0, -1), (0, 1), (-1, 0), (1, 0))

WHITE, PALE, EDGE = (0xFF, 0xFF, 0xFF), (0xBF, 0xEF, 0xFF), (0x3F, 0xAF, 0xEF)
FADE, FADE_EDGE = (0x9F, 0xDF, 0xFF), (0x2F, 0x7F, 0xCF)
SPARK, SPARK_ARM = (0xFF, 0xFF, 0xFF), (0xBF, 0xE7, 0xFF)


def _cut(slot, tilt, fade):
    """방향 칸 slot 쪽을 가로지르는 칼 자국 — 판 가운데가 자국의 가운데. tilt = ±1(기울기), fade = 뒤따름의 옅어진 자국."""
    fx, fy = FACING[slot]
    # 바라보는 쪽에 직각인 줄을 tilt 쪽으로 CUT_TILT 만큼 돌린 방향
    a = math.atan2(fx, -fy) + tilt * math.radians(CUT_TILT)  # (fx, fy) 를 90° 돌린 (−fy, fx) 의 각 + 기울기
    ux, uy = math.cos(a), math.sin(a)
    n = 2 * (CUT_LEN // 2 + 4) + 1
    c = (n - 1) / 2.0
    rows = [[None] * n for _ in range(n)]
    max_hw = 0.7 if fade else 1.15
    bulge = CUT_BULGE * (0.7 if fade else 1.0)
    length = CUT_LEN * (1.08 if fade else 1.0)

    def at(t):
        # t ∈ [−1, 1]: 줄 위의 점 — 가운데가 바라보는 쪽으로 bulge 만큼 휜다
        return c + ux * t * length / 2 + fx * bulge * (1 - t * t), c + uy * t * length / 2 + fy * bulge * (1 - t * t)

    steps = 240
    for edge in (True, False):
        for k in range(steps + 1):
            t = -1 + 2 * k / steps
            x0, y0 = at(t)
            x1, y1 = at(min(1.0, t + 0.01))
            tx, ty = x1 - x0, y1 - y0
            m = math.hypot(tx, ty) or 1.0
            nx, ny = -ty / m, tx / m
            hw = max_hw * (1 - t * t) ** 0.6
            if fade and (k % 3 == 0 and abs(t) > 0.55):
                continue  # 옅어진 자국의 끝은 성기게
            span = hw + (0.8 if edge else 0.0)
            s = -span
            while s <= span + 1e-9:
                x, y = round(x0 + nx * s), round(y0 + ny * s)
                if 0 <= x < n and 0 <= y < n:
                    if edge:
                        if rows[y][x] is None:
                            rows[y][x] = FADE_EDGE if fade else EDGE
                    else:
                        core = abs(s) <= 0.45 * hw + 0.2
                        rows[y][x] = (FADE if fade else WHITE) if core else (FADE if fade else PALE)
                s += 0.25
    return rows, (c, c)


def _spark():
    """5×5 작은 반짝임 — 흰 가운데 · 하늘빛 네 갈래."""
    rows = [[None] * 5 for _ in range(5)]
    for i in range(5):
        rows[2][i] = SPARK_ARM
        rows[i][2] = SPARK_ARM
    rows[2][2] = SPARK
    rows[1][2] = rows[3][2] = rows[2][1] = rows[2][3] = SPARK
    return rows, (2.0, 2.0)


def new_frames():
    """[(w, h, rows, (기준점 x, y) — 잘린 그림 안)]: 칼 자국 16 (방향 칸 × 기울기 × 옅음) · 반짝임 1."""
    out = []
    for slot in range(4):
        for tilt in (-1, 1):
            for fade in (False, True):
                rows, (cx, cy) = _cut(slot, tilt, fade)
                w, h, cut, (x0, y0) = rogue_ilpung._cropped(rows)
                out.append((w, h, cut, (cx - x0, cy - y0)))
    rows, (cx, cy) = _spark()
    w, h, cut, (x0, y0) = rogue_ilpung._cropped(rows)
    out.append((w, h, cut, (cx - x0, cy - y0)))
    return out


def cut_frame(slot, tilt, fade):
    return FRAMES_BEFORE + 4 * slot + 2 * (0 if tilt < 0 else 1) + (1 if fade else 0)


SPARK_FRAME = FRAMES_BEFORE + 16
SPARK_AT = (((-10, -16), (9, -6), (-7, 1)), ((9, -17), (-10, -5), (7, 2)))  # 번마다 번갈아


def _at(frame_no, cx, cy, frames):
    w, h, _rows, (ax, ay) = frames[frame_no - FRAMES_BEFORE]
    return rogue_ilpung._sprite(frame_no, round(cx - ax), round(cy - ay))


def _body(pose):
    return [list(s) for s in pose if s[0] & 0x7F != EFFECT_PART]


def body_frames(shipped):
    """시퀀스 14 의 25 프레임 — 배포본 번개난타 포즈(효과 뺀)를 도착 · 벤다 · 뒤따름 × 8 + 마무리."""
    out = []
    for f in range(FRAMES):
        h, phase = divmod(f, HOP_FRAMES)
        if h >= HOPS:
            pose = shipped[END_POSE]
        elif phase == 0:
            pose = shipped[ARRIVE_POSE]
        elif phase == 1:
            pose = shipped[SLASH_POSES[h % len(SLASH_POSES)]]
        else:
            pose = shipped[FOLLOW_POSES[h % len(FOLLOW_POSES)]]
        out.append([_body(pose[slot]) for slot in range(4)])
    return out


def slash_frames(frames):
    """시퀀스 20 의 25 프레임 — 도착엔 반짝임 셋, 벤다엔 칼 자국, 뒤따름엔 옅어진 자국, 마무리는 빈 칸."""
    out = []
    for f in range(FRAMES):
        h, phase = divmod(f, HOP_FRAMES)
        fr = []
        for slot in range(4):
            sprites = []
            if h < HOPS:
                if phase == 0:
                    for x, y in SPARK_AT[h % 2]:
                        sprites.append(_at(SPARK_FRAME, x, y + 2, frames))
                else:
                    fx, fy = FACING[slot]
                    tilt = -1 if h % 2 == 0 else 1
                    sprites.append(_at(cut_frame(slot, tilt, phase == 2), fx * CUT_REACH, fy * CUT_REACH + BODY_LIFT, frames))
            fr.append(sprites)
        out.append(fr)
    return out


def patch_anim(blob):
    """`hero/hero2`(rogue_ilpung 뒤 = 20 시퀀스) → 시퀀스 14 = 살상연희 25 프레임 · 시퀀스 20 = 베기 25 프레임."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != SEQS_BEFORE or len(seqs[SLAUGHTER_SEQ]) != SHIPPED_FRAMES:
        raise ValueError(f"{ANIM}: {len(seqs)} sequences, sequence 14 {len(seqs[SLAUGHTER_SEQ]) if len(seqs) > 14 else '?'} frames"
                         f" — expected the rogue animation after rogue_ilpung ({SEQS_BEFORE}, the shipped 번개난타 {SHIPPED_FRAMES})")
    frames = new_frames()
    before = [list(s) for s in seqs]
    seqs[SLAUGHTER_SEQ] = body_frames(seqs[SLAUGHTER_SEQ])
    seqs.append(slash_frames(frames))
    for s in (SLAUGHTER_SEQ, SLASH_SEQ):
        for fr in seqs[s]:
            for sprites in fr:
                if len(sprites) > 255:
                    raise ValueError("a pose would have more than 255 sprites")
    out = venom_effect.encode(header, parts, seqs)
    _h, _p, again = venom_effect.decode(out)
    assert len(again) == SLASH_SEQ + 1, "read back: sequence count"
    assert all(again[i] == before[i] for i in range(SEQS_BEFORE) if i != SLAUGHTER_SEQ), "read back: another sequence moved"
    assert len(again[SLAUGHTER_SEQ]) == FRAMES and len(again[SLASH_SEQ]) == FRAMES
    flipped = {(p & 0x7F, f) for s in (SLAUGHTER_SEQ, SLASH_SEQ) for fr in again[s] for d in fr for p, f, _x, _y in d if p & 0x80}
    shipped_flipped = {(p & 0x7F, f) for fr in before[SLAUGHTER_SEQ] for d in fr for p, f, _x, _y in d if p & 0x80}
    assert flipped <= shipped_flipped, f"a flipped picture the class does not load: {sorted(flipped - shipped_flipped)}"
    return out


def patch_sheet(mph, mpd):
    """(mph, mpd) of `hero/4/2` (rogue_ilpung 뒤 61 장) + 칼 자국 16 · 반짝임 1 (61..77)."""
    sheet = cs.sheet_decode(mph, mpd, SHEET)
    if len(sheet.frames) != FRAMES_BEFORE:
        raise ValueError(f"{SHEET}: {len(sheet.frames)} frames — expected {FRAMES_BEFORE} (rogue_ilpung's)")
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


# ── 아이콘: img/gmico 프레임 2 (로그 띠, rogue_ilpung 뒤 11칸) — 칸 5 ─────────────────────────────────
ICON_STRIP, ICON = rogue_ilpung.ICON_STRIP, rogue_ilpung.ICON
ICON_CELLS = rogue_ilpung.ICON_CELLS
ICON_AT = 5  # 번개난타의 칸 — 스킬 창(종류 10 − 5) · 상세 창 cj(10 − 5) 둘 다


def icon_pixels():
    """12×12: 짙은 진홍 바탕 위 엇갈린 칼 자국 셋(흰 심 · 분홍 날) + 희미한 잔상 실루엣 둘."""
    c = 5.5
    rows = []
    for y in range(ICON):
        row = []
        for x in range(ICON):
            k = min(1.0, math.hypot(x - c, y - c) / 7.5)
            row.append(tuple(round(a + (b - a) * k) for a, b in zip((0x8F, 0x1F, 0x2F), (0x27, 0x07, 0x11))))
        rows.append(row)
    ghost = (0x5F, 0x2F, 0x4F)
    for x, y in ((2, 3), (2, 4), (3, 4), (2, 5), (1, 6), (3, 6), (8, 2), (8, 3), (9, 3), (8, 4), (7, 5), (9, 5)):
        rows[y][x] = ghost  # 잔상 둘 — 흐린 사람 모양
    pink, edge = (0xFF, 0xAF, 0xBF), (0xCF, 0x4F, 0x6F)
    for x0, y0, n in ((1, 9, 7), (4, 10, 7), (3, 6, 5)):
        for i in range(n):
            x, y = x0 + i, y0 - i
            if 0 <= x < ICON and 0 <= y < ICON:
                rows[y][x] = WHITE if 1 <= i < n - 1 else pink
                if 0 <= y + 1 < ICON and rows[y + 1][x] != WHITE:
                    rows[y + 1][x] = edge
    return rows


def patch_icon(mph, mpd):
    """(mph, mpd) of `img/gmico` (rogue_ilpung 뒤 — 로그 띠 11칸) with 살상연희 in cell 5."""
    sheet = cs.sheet_decode(mph, mpd, ICON_SHEET)
    strip = sheet.frames[ICON_STRIP]
    if (strip.w, strip.h) != (ICON_CELLS * ICON, ICON):
        raise ValueError(f"{ICON_SHEET} frame {ICON_STRIP}: {strip.w}×{strip.h} — expected rogue_ilpung's eleven cells")
    palette = list(sheet.palette)
    index = {}
    for i, rgb in enumerate(palette):
        if i:
            index.setdefault(tuple(rgb), i)
    pixels = [list(row) for row in strip.pixels]
    for y, row in enumerate(icon_pixels()):
        for x, rgb in enumerate(row):
            if rgb not in index:
                index[rgb] = len(palette)
                palette.append(rgb)
            pixels[y][ICON * ICON_AT + x] = index[rgb]
    if len(palette) > 256:
        raise ValueError(f"{ICON_SHEET}: {len(palette)} colours")
    frames = list(sheet.frames)
    frames[ICON_STRIP] = cs.Frame(strip.w, strip.h, pixels, flags=strip.flags, lead=strip.lead, extra=strip.extra)
    out = cs.Sheet(palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out, any_head=True)
    back = cs.sheet_decode(new_mph, new_mpd, ICON_SHEET)
    for k, (a, b) in enumerate(zip(back.frames, sheet.frames)):
        if k != ICON_STRIP:
            assert (a.w, a.h, a.pixels, a.flags) == (b.w, b.h, b.pixels, b.flags), f"{ICON_SHEET} frame {k} moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    for y in range(ICON):
        row, was = back.frames[ICON_STRIP].pixels[y], strip.pixels[y]
        assert row[: ICON * ICON_AT] == was[: ICON * ICON_AT] and row[ICON * (ICON_AT + 1):] == was[ICON * (ICON_AT + 1):], "another cell moved"
    return new_mph, new_mpd


def cmd_preview(out_png, zip_path):
    from PIL import Image, ImageDraw

    import hero_cape

    jar = cs.open_jar(zip_path)
    sheet_mph, sheet_mpd = jar.read(SHEET + ".mph"), jar.read(SHEET + "_0.mpd")
    if len(cs.sheet_decode(sheet_mph, sheet_mpd, SHEET).frames) == rogue_ilpung.FRAMES_BEFORE:
        sheet_mph, sheet_mpd = rogue_ilpung.patch_sheet(sheet_mph, sheet_mpd)
    if len(cs.sheet_decode(sheet_mph, sheet_mpd, SHEET).frames) == FRAMES_BEFORE:
        sheet_mph, sheet_mpd = patch_sheet(sheet_mph, sheet_mpd)
    fx = cs.sheet_rgba(cs.sheet_decode(sheet_mph, sheet_mpd, SHEET))
    blob = jar.read(ANIM)
    if len(venom_effect.decode(blob)[2]) == 17:
        blob = venom_effect.patch(blob)
    if len(venom_effect.decode(blob)[2]) == rogue_ilpung.SEQS_BEFORE:
        try:
            blob, _n = hero_cape.patch(blob)
        except ValueError:
            pass
        blob = rogue_ilpung.patch_anim(blob)
    shipped = venom_effect.decode(blob)[2]
    if len(shipped) == SEQS_BEFORE:
        blob = patch_anim(blob)
    _h, _p, seqs = venom_effect.decode(blob)
    table = {p: cs.sheet_by_path(jar, b) for p, b in cs.PREVIEW_GEAR[2].items()}
    for part in (1, 2, 3):
        table[part] = cs.sheet_by_path(jar, f"hero/{part}/100")
    table[EFFECT_PART] = fx
    scale, cw, ch, ox, oy = 3, 96, 80, 48, 50
    rows = list(range(FRAMES))
    cols = 4 + 1  # 네 방향 + 배포본 번개난타(아래 방향)
    img = Image.new("RGB", (cols * cw * scale + 16 * ICON * scale, len(rows) * ch * scale), (27, 29, 35))
    dr = ImageDraw.Draw(img)

    def blit(sprites, X0, Y0, alpha):
        for x, y, w, h, px in cs.compose(sprites, table, {5}):
            for yy in range(h):
                for xx in range(w):
                    p = px[yy][xx]
                    if p[3] == 0:
                        continue
                    X, Y = X0 + (x + xx) * scale, Y0 + (y + yy) * scale
                    if 0 <= X < img.width and 0 <= Y < img.height:
                        under = img.getpixel((X, Y))
                        col = tuple(round(u + (q - u) * alpha / 255) for u, q in zip(under, p[:3]))
                        dr.rectangle([X, Y, X + scale - 1, Y + scale - 1], fill=col)

    for r, f in enumerate(rows):
        for d in range(4):
            X0, Y0 = (d * cw + ox) * scale, (r * ch + oy) * scale
            for gx in (-1, 0, 1):
                for gy in (-1, 0, 1):
                    x0, y0 = X0 + (gx * TILE - TILE // 2) * scale, Y0 + (gy * TILE - TILE // 2) * scale
                    dr.rectangle([x0, y0, x0 + TILE * scale - 1, y0 + TILE * scale - 1], outline=(45, 48, 56))
            blit(seqs[SLAUGHTER_SEQ][f][d], X0, Y0, 140)  # 그리기 래퍼처럼: 몸은 반투명
            blit(seqs[SLASH_SEQ][f][d], X0, Y0, 255)  # 베기는 불투명으로 위에
        if f < len(shipped[SLAUGHTER_SEQ]):
            blit(shipped[SLAUGHTER_SEQ][f][1], (4 * cw + ox) * scale, (r * ch + oy) * scale, 255)
        dr.text((4, r * ch * scale + 4), f"f {f}", fill=(255, 255, 0))
    icon_mph, icon_mpd = jar.read(ICON_SHEET + ".mph"), jar.read(ICON_SHEET + "_0.mpd")
    if cs.sheet_decode(icon_mph, icon_mpd, ICON_SHEET).frames[ICON_STRIP].w != ICON_CELLS * ICON:
        import knight_iaido

        if cs.sheet_decode(icon_mph, icon_mpd, ICON_SHEET).frames[knight_iaido.ICON_STRIP].w != 7 * knight_iaido.ICON:
            icon_mph, icon_mpd = knight_iaido.patch_icon(icon_mph, icon_mpd)
        icon_mph, icon_mpd = rogue_ilpung.patch_icon(icon_mph, icon_mpd)
    icon_mph, icon_mpd = patch_icon(icon_mph, icon_mpd)
    strip = cs.sheet_rgba(cs.sheet_decode(icon_mph, icon_mpd, ICON_SHEET))[ICON_STRIP]
    for y in range(ICON):
        for x in range(strip[0]):
            p = strip[2][y][x]
            X, Y = cols * cw * scale + 8 + x * 4, 8 + y * 4
            dr.rectangle([X, Y, X + 3, Y + 3], fill=p[:3])
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
