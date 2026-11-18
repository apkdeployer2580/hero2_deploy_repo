#!/usr/bin/env python3
"""각성기 셋 — 그림·애니메이션·아이콘 (2026-10-07 (2)).

사용자(2026-10-07): *"마지막 각성기를 추가할꺼야. 각성기는 70레벨이 넘으면 얻을 수 있고 버서커 : 격분 … 나이트 : 천공의검 … 로그 :
개화 …"*. 수치·동작은 코드(`guest_anchors::{BerserkerPatch, CombatPatch, RoguePatch}`), 이 파일은 그림뿐이다. 그림은 사용자가
정하지 않았다 — 「캐릭터 주변에 붉은 핏방울 이펙트」(격분) · 「공간을 가르는 검기 세 갈래」(천공의검 = 신의가호 검기의 붉은 초승달,
코드가 날린다) · 「바닥에 꽃 모양 이펙트 … 양손으로 단검을 위로 던지는 듯한 모션」(개화) 만 말했고 나머지는 Claude 의 해석(확인 대기).

── 버서커 격분 (`hero/hero0` · `hero/4/0`) ────────────────────────────────────────────────────────────
* `a.p[16]` = 종류 16 → 그리는 시퀀스 `bl.a(B)B` 그대로 + 4 = **20**(격분 시작, 8 프레임 = `a.n[16]` — `cf.a()V` 가 시퀀스 20 의
  프레임 수를 넣는다); 폭발은 호스트의 `bl.a(B)B` 래퍼가 16 → 17 로 답해 **21**(8 프레임); 버프 중 핏방울 = `bm`(33) 이펙트의
  시퀀스 **22**(8 프레임 루프, `VenomPatch` 의 이펙트 조각이 버서커면 22 를 고른다).
* 시퀀스 20 · 21 = 사자후(13)의 포즈(망토 포함 — hero_cape 뒤) + 붉게 물들인 사자후 이펙트 사본(21 은 두 배로 넓게 두 장씩) ·
  핏방울 두 장(70 · 71). 시트 `hero/4/0` 58.. (berserker_taunt 의 46..57 뒤).
* 아이콘 = `img/gmico` 프레임 0(도발 뒤 12칸)을 13칸으로: 칸 8(퀵슬롯 값 9 − 1) · 12(스킬 창 `kind − stages` = 16 − 4 · 상세 창 · 버프 HUD).

── 나이트 천공의검 (`hero/hero1`) ──────────────────────────────────────────────────────────────────────
* `a.p[15]` = 종류 15 → 그리는 시퀀스 = 전투 블록의 `bx.a(B)B` 래퍼가 21 로 답해 **25**(6 프레임 = `a.n[15]` = 시퀀스 19 의 6);
  = 발도의 포즈(`knight_iaido.IAIDO_POSES`) 고리 없이 — 세 갈래 참격은 코드가 `ak` 파동(붉은 초승달, 시퀀스 22)으로 날린다.
* 아이콘 = 프레임 1(7칸)을 13칸으로: 칸 7(값 8 − 1) · 12(15 − 3).

── 로그 개화 (`hero/hero2` · `hero/4/2`) ─────────────────────────────────────────────────────────────
* `a.p[16]` = 종류 16 → 시퀀스 = 로그 블록의 `ay.a(B)B` 래퍼가 17 로 답해 **21**(25 프레임 = `a.n[16]` = 시퀀스 20 의 25):
  0..9 선딜 = 서기 포즈 + 양손에서 위로 떠오르는 단검 둘(rogue_ilpung 의 단검 그림) · 10..24 = 살상연희(14)의 도약 포즈 그대로
  (베기 시퀀스 20 이 프레임 번호로 겹친다). 꽃 = 시퀀스 **22**(10 프레임: 땅 위에 꽃잎 고리가 반지름 10 → 73 px 로 피어난다, 마지막
  프레임이 남는다) — 그리기 래퍼가 시전 칸에 그린다. 시트 `hero/4/2` 78.. (rogue_slaughter 의 61..77 뒤).
* 아이콘 = 프레임 2(11칸)를 12칸으로: 칸 7(값 8 − 1) · 11(16 − 5).

⛔ `img/gmico` 의 팔레트는 253/256 색 — 새 아이콘은 **있는 색 가운데 가장 가까운 색**으로 그린다(색을 더하지 않는다).

    python tools_re/awakenings.py preview <out.png> [zip]
"""

import copy
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import berserker_taunt  # noqa: E402
import costume_sheets as cs  # noqa: E402
import knight_iaido  # noqa: E402
import rogue_ilpung  # noqa: E402
import rogue_slaughter  # noqa: E402
import venom_effect  # noqa: E402

EFFECT_PART = 4
ICON_SHEET = "img/gmico"
ICON = 12
NO_EXTRA_FLAGS = rogue_ilpung.NO_EXTRA_FLAGS

# ── 버서커 ──
BSK_ANIM = berserker_taunt.ANIM
BSK_SHEET = berserker_taunt.SHEET
BSK_SEQS_BEFORE = berserker_taunt.TAUNT_SEQ + 1  # 20 (18 filler · 19 도발)
FURY_SEQ, FURY_EXPLODE_SEQ, FURY_AURA_SEQ = 20, 21, 22  # = BerserkerPatch::{FURY_SEQ, FURY_EXPLODE_SEQ, FURY_AURA_SEQ}
FURY_FRAMES = 8  # = BerserkerPatch::FURY_FRAMES_DRAWN
AURA_FRAMES = 8
BSK_FRAMES_BEFORE = berserker_taunt.FX_NEW + (berserker_taunt.FX_TO - berserker_taunt.FX_FROM + 1)  # 58
RED_NEW = BSK_FRAMES_BEFORE  # 사자후 이펙트의 붉은 사본 58..69
DROP_BIG, DROP_SMALL = RED_NEW + 12, RED_NEW + 13  # 70 · 71
FURY_CROWN = DROP_SMALL + 1
FURY_BURST = FURY_CROWN + FURY_FRAMES
ROAR_SEQ, ROAR_FRAMES = berserker_taunt.ROAR_SEQ, berserker_taunt.ROAR_FRAMES
FX_FROM, FX_TO = berserker_taunt.FX_FROM, berserker_taunt.FX_TO

# ── 나이트 ──
KNT_ANIM = knight_iaido.ANIM
KNT_SEQS_BEFORE = knight_iaido.IAIDO_WAVE_SEQ + 1  # 25
SKY_SEQ = 25  # = CombatPatch::SKY_SEQ (the map answers 21)
SKY_FRAMES = 60  # = CombatPatch::SKY_FRAMES (★ 2026-10-08: the cast wrapper writes it into a.n[15]; sequence 19 would give 6; ★ (6) 25 → 60 = 3 s)
# ★ 2026-10-08 (user: *"3갈래를 차례대로 그으면서 … 마지막 폭발"*): the pose of each frame as an index into
# knight_iaido.IAIDO_POSES (0·1 the draw, 2·3·4 the swing) — the wind-up, one swing per prong (frames 2 · 5 · 8 =
# CombatPatch::SKY_SLASH_FRAMES), the stance held through the break and the blast. The tears, the cracks and the flash
# are not in the sheet: the host draws them on the character draw (guest_anchors::SkyDrawPatch).
# ★ 2026-10-08 (3) (user: *"3번째 찌른뒤에 0.3초 뒤 깨지고. 깨진뒤에 0.3초뒤 폭발"*): 18 → 27 frames — the break at 14, the
# blast at 20 and its seven draws; the stance holds from 11 to the end.
# ★ 2026-10-08 (4) (user: *"각 찌르기 별 간격을 조금 더 좁혀줘"*): the prongs 0.1 s apart (2 · 4 · 6) → 25 frames — the break
# at 12, the blast at 18; a swing is two frames (its start 2 and end 4), the stance from 8 on.
# ★ 2026-10-08 (6): 60 frames — the wind-up, a two-frame swing at each prong (4 · 10 · 16 = CombatPatch::SKY_SLASH_FRAMES), the stance
# through the break (24), the blast (36) and the tail.
SKY_POSES = (0, 1, 1, 1, 2, 4, 5, 5, 5, 5, 2, 4, 5, 5, 5, 5, 2, 4) + (5,) * 42
SKY_WAVE_SEQ, SKY_WAVE_LIFE = 26, 7  # = CombatPatch::{SKY_WAVE_SEQ, SKY_WAVE_LIFE}: the flying slashes, life + 1 frames

# ── 로그 ──
RG_ANIM = rogue_ilpung.ANIM
RG_SHEET = rogue_ilpung.SHEET
RG_SEQS_BEFORE = rogue_slaughter.SLASH_SEQ + 1  # 21
BLOOM_SEQ, FLOWER_SEQ = 21, 22  # = RoguePatch::{BLOOM_SEQ, FLOWER_SEQ}
BLOOM_FRAMES = rogue_slaughter.FRAMES  # 25 = a.n[16] (sequence 20's)
BLOOM_WINDUP = 10  # = RoguePatch::BLOOM_WINDUP
FLOWER_FRAMES = 10  # = RoguePatch::FLOWER_FRAMES
RG_FRAMES_BEFORE = rogue_slaughter.FRAMES_BEFORE + len(rogue_slaughter.new_frames())  # 78
PETAL_A, PETAL_B, PETAL_C, FLOWER_HEART = (RG_FRAMES_BEFORE + k for k in range(4))  # 78..81
FEET = 6  # 발밑 (칸 가운데보다 6 px 아래)
DAGGER_UP = rogue_ilpung.DAGGER[0]  # DIR8[0] = (0, −1): 위를 보는 단검

PINK, ROSE, LIGHT = (0xFF, 0x8C, 0xC8), (0xE0, 0x50, 0x98), (0xFF, 0xD8, 0xEC)
HEART, HEART_DARK = (0xFF, 0xE0, 0x50), (0xC8, 0x90, 0x20)
BLOOD, BLOOD_DARK, BLOOD_LIGHT = (0xD8, 0x18, 0x20), (0x80, 0x08, 0x10), (0xFF, 0x60, 0x60)


def _index_of(palette, index, rgb, room=True):
    """팔레트 칸: 있으면 그것, 없고 자리가 있으면 더하고, 자리가 없으면 가장 가까운 칸(0 은 투명이라 제외)."""
    rgb = tuple(rgb)
    if rgb in index:
        return index[rgb]
    if room and len(palette) < 256:
        index[rgb] = len(palette)
        palette.append(rgb)
        return index[rgb]
    best = min(range(1, len(palette)), key=lambda i: sum((a - b) ** 2 for a, b in zip(palette[i], rgb)))
    return best


def _palette_index(palette):
    index = {}
    for i, rgb in enumerate(palette):
        if i:
            index.setdefault(tuple(rgb), i)
    return index


# ═══ 버서커 격분 ═══════════════════════════════════════════════════════════════════════════════════

def tint_red(rgb):
    """사자후의 붉은·노란 외침 → 핏빛. 밝기를 지키고, 회색·흰색은 옅은 분홍."""
    r, g, b = rgb
    lum = 0.3 * r + 0.59 * g + 0.11 * b
    if max(rgb) - min(rgb) < 24:
        return (min(255, round(lum * 1.0 + 20)), min(255, round(lum * 0.62)), min(255, round(lum * 0.66)))
    return (min(255, round(lum * 0.95 + 60)), min(255, round(lum * 0.18)), min(255, round(lum * 0.22)))


def red_map():
    return {f: RED_NEW + (f - FX_FROM) for f in range(FX_FROM, FX_TO + 1)}


def _drop(big):
    """핏방울: 큰 것 3×5 · 작은 것 2×3 (None = 투명)."""
    if big:
        rows = [
            [None, BLOOD_LIGHT, None],
            [None, BLOOD, None],
            [BLOOD, BLOOD, BLOOD],
            [BLOOD_DARK, BLOOD, BLOOD_DARK],
            [None, BLOOD_DARK, None],
        ]
    else:
        rows = [[BLOOD_LIGHT, None], [BLOOD, BLOOD], [None, BLOOD_DARK]]
    return rows


def _bsk_sprite(frame, x, y):
    if not (-128 <= x <= 127 and -128 <= y <= 127):
        raise ValueError(f"effect {frame} at ({x},{y}) does not fit a signed byte")
    return [EFFECT_PART, frame, x & 0xFF, y & 0xFF]


def blood_drops(frame, dense=False):
    """프레임 f 의 핏방울: 몸 둘레 여섯 자리에서 떨어진다(루프 8 프레임), dense 면 열둘."""
    out = []
    seats = ((-14, -20), (-8, -6), (10, -18), (14, -2), (-4, -26), (6, -10))
    if dense:
        seats = seats + ((-18, -12), (18, -14), (0, -30), (-10, 2), (12, 6), (2, -16))
    for i, (x, y) in enumerate(seats):
        fall = ((frame * 3) + i * 5) % 24
        big = i % 2 == 0
        out.append(_bsk_sprite(DROP_BIG if big else DROP_SMALL, x, y + fall))
    return out


def _bsk_body(pose):
    return [list(s) for s in pose if s[0] & 0x7F != EFFECT_PART]


def fury_flare(f, burst=False):
    rows = [[None] * 49 for _ in range(57)]
    ramp = (BLOOD_DARK, BLOOD, BLOOD_LIGHT, (255, 218, 174))

    def line(x0, y0, x1, y1, colour):
        steps = max(abs(x1 - x0), abs(y1 - y0), 1)
        for step in range(steps + 1):
            x = round((x0 + (x1 - x0) * step / steps) * 0.5) + 24
            y = round((y0 + (y1 - y0) * step / steps) * 0.7) + 45
            if 0 <= x < 49 and 0 <= y < 57:
                rows[y][x] = colour

    radius = (14 + f * 4) if burst else (16 + min(f, 4) * 3)
    for i in range(40):
        if (i + f) % 7 < 2:
            continue
        a, b = i * math.tau / 40, (i + 1) * math.tau / 40
        x0, y0 = round(radius * math.cos(a)), round(radius * 0.3 * math.sin(a)) + 3
        x1, y1 = round(radius * math.cos(b)), round(radius * 0.3 * math.sin(b)) + 3
        line(x0, y0 + 1, x1, y1 + 1, ramp[0])
        line(x0, y0, x1, y1, ramp[2 if f < 5 else 1])
    for i in range(10):
        a = i * math.tau / 10 + 0.16
        if burst:
            r0, r1 = radius * 0.65, min(46, radius + 10)
            x0, y0 = round(math.cos(a) * r0), round(math.sin(a) * r0 * 0.8) - 16
            x1, y1 = round(math.cos(a) * r1), round(math.sin(a) * r1 * 0.8) - 16
        else:
            x0 = round(math.cos(a) * (18 + i % 3 * 3))
            y0 = round(math.sin(a) * 7) + 1
            x1, y1 = x0 + (3 if x0 > 0 else -3), y0 - (13 + (f * 7 + i * 11) % 35)
        for offset, colour in ((-1, ramp[0]), (1, ramp[1]), (0, ramp[2])):
            line(x0 + offset, y0, x1 + offset, y1, colour)
        if f < 5:
            line(x1, y1, x1, y1 + 3, ramp[3])
    return rows


def patch_bsk_anim(blob):
    """`hero/hero0`(berserker_taunt 뒤 = 20 시퀀스) + 20 격분 시작 · 21 폭발 · 22 핏방울 오라."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != BSK_SEQS_BEFORE or len(seqs[ROAR_SEQ]) != ROAR_FRAMES:
        raise ValueError(f"{BSK_ANIM}: {len(seqs)} sequences, 사자후 {len(seqs[ROAR_SEQ]) if len(seqs) > ROAR_SEQ else '-'} frames — expected berserker_taunt's output ({BSK_SEQS_BEFORE}, {ROAR_FRAMES})")
    remap = red_map()
    flips, loaded = parts[EFFECT_PART]
    parts[EFFECT_PART] = (flips + bytes(remap[f] for f in remap if f in flips and remap[f] not in flips), loaded)
    roar = seqs[ROAR_SEQ]
    # 20: 격분 시작 — 외침의 앞 여덟 포즈, 이펙트는 붉은 사본, 핏방울이 돌기 시작한다
    start = []
    for f in range(FURY_FRAMES):
        src = copy.deepcopy(roar[f])
        directions = []
        for sprites in src:
            body = []
            for s in sprites:
                if s[0] & 0x7F == EFFECT_PART and s[1] in remap:
                    s[1] = remap[s[1]]
                body.append(s)
            directions.append([_bsk_sprite(FURY_CROWN + f, -24, -45)] + body + blood_drops(f))
        start.append(directions)
    # 21: 폭발 — 외침의 뒤 여덟 포즈(터지는 쪽), 붉은 이펙트를 두 장씩 바깥으로 벌려 더 크게, 핏방울 빽빽이
    explode = []
    for f in range(FURY_FRAMES):
        src = copy.deepcopy(roar[ROAR_FRAMES - FURY_FRAMES + f])
        directions = []
        for sprites in src:
            body, fx = [], []
            for s in sprites:
                if s[0] & 0x7F == EFFECT_PART and s[1] in remap:
                    s[1] = remap[s[1]]
                    x, y = (s[2] - 256 if s[2] > 127 else s[2]), (s[3] - 256 if s[3] > 127 else s[3])
                    spread = 10 + 2 * f
                    for dx in (-spread, spread):
                        xx = max(-128, min(127, x + dx))
                        fx.append([s[0], s[1], xx & 0xFF, s[3]])
                    fx.append(s)
                else:
                    body.append(s)
            directions.append(fx + [_bsk_sprite(FURY_BURST + f, -24, -45)] + body + blood_drops(f, dense=True))
        explode.append(directions)
    # 22: 버프 중 핏방울 오라 — 몸은 bm 이 아니라 캐릭터 자신이 그리니 여기엔 방울뿐
    aura = [[blood_drops(f) for _ in range(4)] for f in range(AURA_FRAMES)]
    seqs.append(start)
    seqs.append(explode)
    seqs.append(aura)
    for s in (FURY_SEQ, FURY_EXPLODE_SEQ, FURY_AURA_SEQ):
        for fr in seqs[s]:
            for sprites in fr:
                if len(sprites) > 255:
                    raise ValueError("a pose would have more than 255 sprites")
    out = venom_effect.encode(header, parts, seqs)
    _h, again_parts, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[:BSK_SEQS_BEFORE] == before, "read back: the shipped sequences moved"
    assert len(again) == FURY_AURA_SEQ + 1
    assert len(again[FURY_SEQ]) == FURY_FRAMES and len(again[FURY_EXPLODE_SEQ]) == FURY_FRAMES and len(again[FURY_AURA_SEQ]) == AURA_FRAMES
    flipped = {s[1] for q in (FURY_SEQ, FURY_EXPLODE_SEQ) for fr in again[q] for d in fr for s in d if s[0] == EFFECT_PART | 0x80}
    assert flipped <= set(again_parts[EFFECT_PART][0]), "a flipped copy is in the flip list"
    return out


def patch_bsk_sheet(mph, mpd):
    """기존 58장 + 붉은 사본 12장 · 핏방울 2장 · 혈기 상승과 폭발 각 8장."""
    sheet = cs.sheet_decode(mph, mpd, BSK_SHEET)
    if len(sheet.frames) != BSK_FRAMES_BEFORE:
        raise ValueError(f"{BSK_SHEET}: {len(sheet.frames)} frames — expected {BSK_FRAMES_BEFORE} (berserker_taunt's)")
    palette = list(sheet.palette)
    index = _palette_index(palette)
    out_frames = list(sheet.frames)
    for f in range(FX_FROM, FX_TO + 1):
        src = sheet.frames[f]
        px = []
        for row in src.pixels:
            line = []
            for v in row:
                line.append(0 if v == 0 else _index_of(palette, index, tint_red(palette[v])))
            px.append(line)
        out_frames.append(cs.Frame(src.w, src.h, px, flags=src.flags, lead=src.lead, extra=src.extra))
    for big in (True, False):
        rows = _drop(big)
        px = [[0 if rgb is None else _index_of(palette, index, rgb) for rgb in row] for row in rows]
        out_frames.append(cs.Frame(len(rows[0]), len(rows), px, flags=NO_EXTRA_FLAGS))
    for burst in (False, True):
        for f in range(FURY_FRAMES):
            rows = fury_flare(f, burst)
            px = [[0 if rgb is None else _index_of(palette, index, rgb) for rgb in row] for row in rows]
            out_frames.append(cs.Frame(len(rows[0]), len(rows), px, flags=NO_EXTRA_FLAGS))
    if len(palette) > 256:
        raise ValueError(f"{BSK_SHEET}: {len(palette)} colours")
    out = cs.Sheet(palette, out_frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out)
    back = cs.sheet_decode(new_mph, new_mpd, BSK_SHEET)
    assert [(f.w, f.h, f.pixels) for f in back.frames[:BSK_FRAMES_BEFORE]] == [(f.w, f.h, f.pixels) for f in sheet.frames], "old frames moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    assert len(back.frames) == FURY_BURST + FURY_FRAMES
    return new_mph, new_mpd


# ═══ 나이트 천공의검 ═══════════════════════════════════════════════════════════════════════════════

def patch_knt_anim(blob, sheet_sizes):
    """`hero/hero1`(knight_iaido 뒤 = 25 시퀀스) + 25 천공의검 = 발도의 포즈를 SKY_POSES 대로 SKY_FRAMES(★ 2026-10-08 (4) 25) 프레임에 (고리 없이; 세 갈래 · 균열 ·
    폭발은 호스트가 그린다 — ★ 2026-10-08) + 26 = 10-07 의 날아가는 참격 그림(붉은 초승달, SKY_WAVE_LIFE + 1 프레임 — 이제 시전이
    파동을 안 만들어 안 쓰이지만 페인터의 분기가 남아 있어 그대로 둔다)."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != KNT_SEQS_BEFORE or len(seqs[knight_iaido.SWING_SEQ]) != 7:
        raise ValueError(f"{KNT_ANIM}: {len(seqs)} sequences — expected knight_iaido's output ({KNT_SEQS_BEFORE}, 검기 7)")
    poses = []
    for src in knight_iaido.IAIDO_POSES:
        directions = []
        for sprites in seqs[knight_iaido.SWING_SEQ][src]:
            directions.append([list(s) for s in sprites if s[0] & 0x7F != EFFECT_PART])
        poses.append(directions)
    sky = [[[list(s) for s in d] for d in poses[i]] for i in SKY_POSES]
    if len(sky) != SKY_FRAMES:
        raise AssertionError("천공의검: SKY_FRAMES frames")
    seqs.append(sky)
    # 26: the flying slash — the small red crescent for the first two tiles, the big one after (as knight_iaido's `flying`)
    slash = [[[knight_iaido._centred(knight_iaido.CRESCENT[("red", 0 if f < 2 else 1, d)], sheet_sizes)] for d in range(4)] for f in range(SKY_WAVE_LIFE + 1)]
    seqs.append(slash)
    out = venom_effect.encode(header, parts, seqs)
    _h, _p, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[:KNT_SEQS_BEFORE] == before, "read back"
    assert len(again) == SKY_WAVE_SEQ + 1 and len(again[SKY_SEQ]) == SKY_FRAMES and len(again[SKY_WAVE_SEQ]) == SKY_WAVE_LIFE + 1
    return out


# ═══ 로그 개화 ═══════════════════════════════════════════════════════════════════════════════════

def _petal(kind):
    """꽃잎 셋 (A 5×5 분홍 · B 3×3 장미 · C 3×3 옅은) 과 꽃심 7×7."""
    P, R, L, N = PINK, ROSE, LIGHT, None
    if kind == "A":
        return [[N, P, P, P, N], [P, L, L, P, P], [P, L, P, P, R], [P, P, P, R, R], [N, R, R, R, N]]
    if kind == "B":
        return [[R, R, N], [R, P, R], [N, R, R]]
    if kind == "C":
        return [[N, L, N], [L, P, L], [N, L, N]]
    H, D = HEART, HEART_DARK
    return [
        [N, N, P, P, P, N, N],
        [N, P, H, H, H, P, N],
        [P, H, H, L, H, H, P],
        [P, H, L, L, L, H, P],
        [P, H, H, L, H, H, P],
        [N, P, D, H, D, P, N],
        [N, N, P, P, P, N, N],
    ]


def _rg_frames():
    """[(frame no, rows)] 꽃잎 A · B · C · 꽃심."""
    return [(PETAL_A, _petal("A")), (PETAL_B, _petal("B")), (PETAL_C, _petal("C")), (FLOWER_HEART, _petal("H"))]


def _rg_sprite(frame, rows, cx, cy):
    """그림 frame 의 가운데를 (cx, cy) 에."""
    w, h = len(rows[0]), len(rows)
    return rogue_ilpung._sprite(frame, round(cx - (w - 1) / 2), round(cy - (h - 1) / 2))


def flower_sprites(f):
    """시퀀스 22 프레임 f: 반지름 10 + 7f px 의 꽃잎 고리(땅에 눕힌 타원 ×0.55) + 안쪽 고리 + 꽃심 — 마지막 프레임이 남는다."""
    pics = dict(_rg_frames())
    out = [_rg_sprite(FLOWER_HEART, pics[FLOWER_HEART], 0, FEET)]
    r = 10 + 7 * f
    n = 6 + 2 * f
    for i in range(n):
        a = i * 2 * math.pi / n + f * 0.3
        kind = (PETAL_A, PETAL_B, PETAL_C)[i % 3]
        out.append(_rg_sprite(kind, pics[kind], r * math.cos(a), FEET + 0.55 * r * math.sin(a)))
    if f >= 3:
        r2 = r * 0.5
        for i in range(n // 2):
            a = i * 4 * math.pi / n - f * 0.2
            kind = (PETAL_C, PETAL_B)[i % 2]
            out.append(_rg_sprite(kind, pics[kind], r2 * math.cos(a), FEET + 0.55 * r2 * math.sin(a)))
    return out


def patch_rg_anim(blob):
    """`hero/hero2`(rogue_slaughter 뒤 = 21 시퀀스) + 21 개화(선딜 10 + 살상연희의 도약 15) · 22 꽃(10 프레임)."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != RG_SEQS_BEFORE or len(seqs[rogue_slaughter.SLAUGHTER_SEQ]) != BLOOM_FRAMES:
        raise ValueError(f"{RG_ANIM}: {len(seqs)} sequences — expected rogue_slaughter's output ({RG_SEQS_BEFORE}, 살상연희 {BLOOM_FRAMES})")
    frames = rogue_ilpung.new_frames()
    bloom = []
    for f in range(BLOOM_FRAMES):
        if f < BLOOM_WINDUP:
            directions = []
            for slot in range(4):
                body = [list(s) for s in seqs[rogue_ilpung.IDLE_SEQ][0][slot] if s[0] & 0x7F != EFFECT_PART]
                lift = -12 - 3 * f
                daggers = [rogue_ilpung._at(DAGGER_UP, -7, lift, frames), rogue_ilpung._at(DAGGER_UP, 7, lift - 2, frames)]
                petals = []
                for side in (-1, 1):
                    for trail in range(3):
                        y = lift + 6 + trail * 6
                        x = side * (7 + trail * 2) + round(math.sin(f * 0.7 + trail) * 3)
                        kind = PETAL_C if trail == 0 else PETAL_B
                        petals.append(_rg_sprite(kind, _petal("C" if trail == 0 else "B"), x, y))
                directions.append(body + petals + daggers)
            bloom.append(directions)
        else:
            bloom.append(copy.deepcopy(seqs[rogue_slaughter.SLAUGHTER_SEQ][f]))
    flower = [[flower_sprites(f) for _ in range(4)] for f in range(FLOWER_FRAMES)]
    seqs.append(bloom)
    seqs.append(flower)
    for s in (BLOOM_SEQ, FLOWER_SEQ):
        for fr in seqs[s]:
            for sprites in fr:
                if len(sprites) > 255:
                    raise ValueError("a pose would have more than 255 sprites")
    out = venom_effect.encode(header, parts, seqs)
    _h, _p, again = venom_effect.decode(out)
    before = venom_effect.decode(blob)[2]
    assert again[:RG_SEQS_BEFORE] == before, "read back"
    assert len(again) == FLOWER_SEQ + 1 and len(again[BLOOM_SEQ]) == BLOOM_FRAMES and len(again[FLOWER_SEQ]) == FLOWER_FRAMES
    assert again[BLOOM_SEQ][BLOOM_WINDUP:] == before[rogue_slaughter.SLAUGHTER_SEQ][BLOOM_WINDUP:], "the hops are 살상연희's, frame for frame"
    return out


def patch_rg_sheet(mph, mpd):
    """(mph, mpd) of `hero/4/2` (rogue_slaughter 뒤 78 장) + 꽃잎 셋 · 꽃심 (78..81)."""
    sheet = cs.sheet_decode(mph, mpd, RG_SHEET)
    if len(sheet.frames) != RG_FRAMES_BEFORE:
        raise ValueError(f"{RG_SHEET}: {len(sheet.frames)} frames — expected {RG_FRAMES_BEFORE} (rogue_slaughter's)")
    palette = list(sheet.palette)
    index = _palette_index(palette)
    out_frames = list(sheet.frames)
    for _no, rows in _rg_frames():
        px = [[0 if rgb is None else _index_of(palette, index, rgb) for rgb in row] for row in rows]
        out_frames.append(cs.Frame(len(rows[0]), len(rows), px, flags=NO_EXTRA_FLAGS))
    if len(palette) > 256:
        raise ValueError(f"{RG_SHEET}: {len(palette)} colours")
    out = cs.Sheet(palette, out_frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out)
    back = cs.sheet_decode(new_mph, new_mpd, RG_SHEET)
    assert [(f.w, f.h, f.pixels) for f in back.frames[:RG_FRAMES_BEFORE]] == [(f.w, f.h, f.pixels) for f in sheet.frames], "old frames moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    assert len(back.frames) == FLOWER_HEART + 1
    return new_mph, new_mpd


# ═══ 아이콘 (img/gmico) ═══════════════════════════════════════════════════════════════════════════
# 띠: 0 버서커 12 → 13칸(8 · 12 격분) · 1 나이트 7 → 13칸(7 · 12 천공의검) · 2 로그 11 → 12칸(7 · 11 개화).
ICON_PLAN = (
    (0, berserker_taunt.ICON_CELLS, 13, (8, 12), "fury"),
    (1, 7, 13, (7, 12), "sky"),
    (2, rogue_ilpung.ICON_CELLS, 12, (7, 11), "bloom"),
)


def _icon_bg(a, b, c=5.5, span=7.5):
    rows = []
    for y in range(ICON):
        row = []
        for x in range(ICON):
            k = min(1.0, math.hypot(x - c, y - c) / span)
            row.append(tuple(round(p + (q - p) * k) for p, q in zip(a, b)))
        rows.append(row)
    return rows


def icon_fury():
    """검붉은 바탕, 가운데 큰 핏방울과 두 작은 방울, 아래 불꽃 혀."""
    rows = _icon_bg((0x7A, 0x12, 0x18), (0x28, 0x04, 0x08))
    W, L, B, D = (0xFF, 0xFF, 0xFF), BLOOD_LIGHT, BLOOD, BLOOD_DARK
    pts = {
        (5, 1): L, (6, 1): L, (5, 2): L, (6, 2): L, (4, 3): B, (5, 3): L, (6, 3): L, (7, 3): B,
        (4, 4): B, (5, 4): B, (6, 4): B, (7, 4): B, (3, 5): B, (4, 5): B, (5, 5): W, (6, 5): B, (7, 5): B, (8, 5): B,
        (3, 6): D, (4, 6): B, (5, 6): B, (6, 6): B, (7, 6): B, (8, 6): D, (4, 7): D, (5, 7): B, (6, 7): B, (7, 7): D,
        (5, 8): D, (6, 8): D,
        (1, 4): L, (1, 5): B, (1, 6): D, (10, 3): L, (10, 4): B, (10, 5): D,
        (2, 10): (0xFF, 0xA0, 0x30), (3, 9): (0xFF, 0xD0, 0x40), (4, 10): (0xFF, 0xA0, 0x30), (7, 10): (0xFF, 0xA0, 0x30), (8, 9): (0xFF, 0xD0, 0x40), (9, 10): (0xFF, 0xA0, 0x30),
        (3, 11): (0xE0, 0x60, 0x20), (8, 11): (0xE0, 0x60, 0x20),
    }
    for (x, y), rgb in pts.items():
        rows[y][x] = rgb
    return rows


def icon_sky():
    """짙은 하늘빛 바탕, 가운데 위로 선 검과 양옆으로 갈라지는 붉은 초승달 셋."""
    rows = _icon_bg((0x3C, 0x5A, 0xA8), (0x10, 0x1A, 0x40))
    W, S, G, R, RL = (0xFF, 0xFF, 0xFF), (0xC8, 0xD8, 0xF0), (0xFF, 0xD0, 0x40), (0xE0, 0x30, 0x30), (0xFF, 0x80, 0x70)
    pts = {(5, 0): W, (6, 0): W, (5, 1): S, (6, 1): W, (5, 2): S, (6, 2): W, (5, 3): S, (6, 3): W, (5, 4): S, (6, 4): W, (5, 5): S, (6, 5): W,
           (4, 6): G, (5, 6): G, (6, 6): G, (7, 6): G, (5, 7): (0x80, 0x50, 0x20), (6, 7): (0x80, 0x50, 0x20), (5, 8): (0x80, 0x50, 0x20), (6, 8): (0x80, 0x50, 0x20)}
    # three crescents: ahead (top) and the two front diagonals
    for (x, y) in ((3, 1), (2, 2), (2, 3), (8, 1), (9, 2), (9, 3)):
        pts[(x, y)] = R
    for (x, y) in ((3, 2), (8, 2)):
        pts[(x, y)] = RL
    for (x, y) in ((0, 5), (0, 6), (1, 7), (11, 5), (11, 6), (10, 7)):
        pts[(x, y)] = R
    for (x, y) in ((1, 6), (10, 6)):
        pts[(x, y)] = RL
    for (x, y), rgb in pts.items():
        rows[y][x] = rgb
    return rows


def icon_bloom():
    """어두운 풀빛 바탕 위 분홍 꽃 한 송이(여섯 잎 · 노란 심)와 위로 던진 단검 둘."""
    rows = _icon_bg((0x2A, 0x60, 0x34), (0x0C, 0x24, 0x14))
    P, R, L, H, S, W = PINK, ROSE, LIGHT, HEART, (0xC8, 0xD8, 0xF0), (0xFF, 0xFF, 0xFF)
    pts = {}
    c = (5.5, 7.0)
    for i in range(6):
        a = i * math.pi / 3
        for d in (2.2, 3.4):
            x, y = round(c[0] + d * math.cos(a)), round(c[1] + 0.8 * d * math.sin(a))
            if 0 <= x < ICON and 0 <= y < ICON:
                pts[(x, y)] = P if d > 3 else R
        x, y = round(c[0] + 4.4 * math.cos(a)), round(c[1] + 0.8 * 4.4 * math.sin(a))
        if 0 <= x < ICON and 0 <= y < ICON:
            pts[(x, y)] = L
    for (x, y) in ((5, 7), (6, 7), (5, 6), (6, 6)):
        pts[(x, y)] = H
    for (x, y) in ((2, 0), (2, 1), (2, 2), (9, 0), (9, 1), (9, 2)):
        pts[(x, y)] = S
    pts[(2, 0)] = pts[(9, 0)] = W
    for (x, y), rgb in pts.items():
        rows[y][x] = rgb
    return rows


ICON_PIXELS = {"fury": icon_fury, "sky": icon_sky, "bloom": icon_bloom}


def patch_icon(mph, mpd):
    """(mph, mpd) of `img/gmico` (berserker_taunt 뒤) with the three strips widened and the awakenings' cells drawn in the
    nearest EXISTING colours (the palette is 253/256). 반전 없는 시트라 `sheet_encode(any_head=True)`."""
    sheet = cs.sheet_decode(mph, mpd, ICON_SHEET)
    palette = list(sheet.palette)
    index = _palette_index(palette)
    frames = list(sheet.frames)
    for strip_no, cells_before, cells, at, name in ICON_PLAN:
        strip = sheet.frames[strip_no]
        if (strip.w, strip.h) != (cells_before * ICON, ICON):
            raise ValueError(f"{ICON_SHEET} frame {strip_no}: {strip.w}×{strip.h} — expected {cells_before} cells")
        width = cells * ICON
        pixels = [list(row) + [0] * (width - strip.w) for row in strip.pixels]
        art = ICON_PIXELS[name]()
        for cell in at:
            for y, row in enumerate(art):
                for x, rgb in enumerate(row):
                    pixels[y][ICON * cell + x] = _index_of(palette, index, rgb, room=False)
        frames[strip_no] = cs.Frame(width, strip.h, pixels, flags=strip.flags, lead=strip.lead, extra=strip.extra)
    assert len(palette) == len(sheet.palette), "no colour added"
    out = cs.Sheet(palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out, any_head=True)
    back = cs.sheet_decode(new_mph, new_mpd, ICON_SHEET)
    strips = {p[0] for p in ICON_PLAN}
    for k, (a, b) in enumerate(zip(back.frames, sheet.frames)):
        if k not in strips:
            assert (a.w, a.h, a.pixels, a.flags) == (b.w, b.h, b.pixels, b.flags), f"{ICON_SHEET} frame {k} moved"
    assert back.palette == sheet.palette, "colours moved"
    for strip_no, cells_before, cells, at, _name in ICON_PLAN:
        for y in range(ICON):
            for cell in range(cells_before):
                if cell in at:
                    continue  # an empty filler cell of the earlier widening (the berserker's 8, the rogue's 7), painted now
                lo, hi = cell * ICON, (cell + 1) * ICON
                assert back.frames[strip_no].pixels[y][lo:hi] == sheet.frames[strip_no].pixels[y][lo:hi], f"strip {strip_no}: shipped cell {cell} moved"
        assert back.frames[strip_no].w == cells * ICON
    return new_mph, new_mpd


def icon_cells_after():
    """띠마다 칸 수, 이 파일 뒤: {띠: 칸}."""
    return {strip_no: cells for strip_no, _b, cells, _at, _n in ICON_PLAN}


# ═══ preview ═════════════════════════════════════════════════════════════════════════════════════

def cmd_preview(out_png, zip_path):
    from PIL import Image, ImageDraw

    import hero_cape

    jar = cs.open_jar(zip_path)
    # 버서커
    bmph, bmpd = patch_bsk_sheet(*berserker_taunt.patch_sheet(jar.read(BSK_SHEET + ".mph"), jar.read(BSK_SHEET + "_0.mpd")))
    bfx = cs.sheet_rgba(cs.sheet_decode(bmph, bmpd, BSK_SHEET))
    blob = jar.read(BSK_ANIM)
    blob, _n = hero_cape.patch(blob)
    blob = patch_bsk_anim(berserker_taunt.patch_anim(blob))
    _h, _p, bseqs = venom_effect.decode(blob)
    # 로그
    rmph, rmpd = patch_rg_sheet(*rogue_slaughter.patch_sheet(*rogue_ilpung.patch_sheet(jar.read(RG_SHEET + ".mph"), jar.read(RG_SHEET + "_0.mpd"))))
    rfx = cs.sheet_rgba(cs.sheet_decode(rmph, rmpd, RG_SHEET))
    rblob = venom_effect.patch(jar.read(RG_ANIM))
    rblob, _n = hero_cape.patch(rblob)
    rblob = patch_rg_anim(rogue_slaughter.patch_anim(rogue_ilpung.patch_anim(rblob)))
    _h, _p, rseqs = venom_effect.decode(rblob)
    scale, cw, ch, ox, oy = 2, 160, 120, 80, 70
    rows = [("b", FURY_SEQ, f) for f in range(FURY_FRAMES)] + [("b", FURY_EXPLODE_SEQ, f) for f in range(FURY_FRAMES)] + [("b", FURY_AURA_SEQ, f) for f in range(AURA_FRAMES)]
    rows += [("r", BLOOM_SEQ, f) for f in range(0, BLOOM_FRAMES, 2)] + [("r", FLOWER_SEQ, f) for f in range(FLOWER_FRAMES)]
    img = Image.new("RGB", (4 * cw * scale + 16 * ICON * 4, len(rows) * ch * scale), (27, 29, 35))
    dr = ImageDraw.Draw(img)
    tables = {}
    for cls, fx in (("b", bfx), ("r", rfx)):
        table = {p: cs.sheet_by_path(jar, b) for p, b in cs.PREVIEW_GEAR[0 if cls == "b" else 2].items()}
        bare = {1: 100, 2: 100, 3: 100} if cls == "b" else {1: 102, 2: 100, 3: 102}  # the classes' bare body · arm · leg sheets
        for part in (1, 2, 3):
            table[part] = cs.sheet_by_path(jar, f"hero/{part}/{bare[part]}")
        table[EFFECT_PART] = fx
        tables[cls] = table
    for r, (cls, s, f) in enumerate(rows):
        seqs = bseqs if cls == "b" else rseqs
        for d in range(4):
            bx = d * cw
            for x, y, w, h, px in cs.compose(seqs[s][f][d], tables[cls], set()):
                for yy in range(h):
                    for xx in range(w):
                        p = px[yy][xx]
                        if p[3] == 0:
                            continue
                        X, Y = (bx + ox + x + xx) * scale, (r * ch + oy + y + yy) * scale
                        if 0 <= X < img.width and 0 <= Y < img.height:
                            dr.rectangle([X, Y, X + scale - 1, Y + scale - 1], fill=p[:3])
        dr.text((4, r * ch * scale + 4), f"{cls} seq {s} f {f}", fill=(255, 255, 0))
    imph, impd = patch_icon(*berserker_taunt.patch_icon(*rogue_slaughter.patch_icon(*rogue_ilpung.patch_icon(*knight_iaido.patch_icon(jar.read(ICON_SHEET + ".mph"), jar.read(ICON_SHEET + "_0.mpd"))))))
    strips = cs.sheet_rgba(cs.sheet_decode(imph, impd, ICON_SHEET))
    for k, strip in enumerate(strips[:3]):
        for y in range(ICON):
            for x in range(strip[0]):
                p = strip[2][y][x]
                X, Y = 4 * cw * scale + 8 + x * 4, 8 + k * (ICON * 4 + 8) + y * 4
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
