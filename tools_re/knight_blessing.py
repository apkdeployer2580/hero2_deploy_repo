#!/usr/bin/env python3
"""신의가호 중 나이트의 둔기강타 · 검기 그림 (2026-10-01).

사용자(2026-10-01): *"신의가호를 사용한 상태에서 변경되는 나이트의 검기,둔기강타의 스킬 이펙트를 수정할꺼야. 검기는 율리케 보스가
던지는 검기를 붉은색으로 바꾼 형태 … 둔기강타는 이안이 사용하는 점프했다 내려오면서 붉은 눈송이들이 흩날리는 듯한 스킬이 있는데
이걸로 변경할꺼고 색상은 마찬 푸른색 계열"* → 물어서: 검기 = **3방향 그대로**, 그림만 율리케의 불기둥을 붉게 · 둔기강타 = **이안의
점프 내려찍기를 파랗게**, 타격 = **8방향 직선 2칸씩**(그것은 코드 — `guest_anchors::CombatPatch` 의 링).

── 보스의 그림 (HERO2_GUEST_RE.md §8.38) ─────────────────────────────────────────────────────────────
* 이안(#89)의 점프 내려찍기 = `enm/spr/14` 시퀀스 5: 웅크림 → 30 px 도약 → 착지에 폭발(`enm/5effect/11_0` 프레임 6, 24×24) →
  붉은 바람개비(프레임 7, 18×21 — 「붉은 눈송이」)가 고리로 퍼진다.
* 율리케(#95)의 십자 불기둥 한 칸 = `enm/spr/15` 시퀀스 7: 불혀 한 쌍(`enm/5effect/12_0` 프레임 11 + 그 반전) → 불기둥(12 + 반전)과
  연기(14) → 연기(13)와 불티(15). 율리케의 시트에는 검기 그림이 없다 — 「던지는 검기」는 이 불기둥이다.

── 무엇을 더하나 ───────────────────────────────────────────────────────────────────────────────
* 나이트의 이펙트 시트 `hero/4/1`(배포본 39 프레임) 끝에 그림 9 장(39..47) — 보스 픽셀 그대로, 색만 바꿔서(`BLUE` · `RED`).
  반전은 미리 뒤집은 그림으로 싣는다(게임의 반전 목록 = 애니메이션의 파트 4 첫 목록은 건드리지 않는다).
  팔레트는 시트 하나에 하나라 새 색을 뒤에 붙인다(배포본 18 색 → 8 비트 그대로; 옛 프레임의 인덱스는 그대로라 그림도 그대로).
* 직업 애니메이션 `hero/hero1`(배포본 19 시퀀스) 끝에 시퀀스 둘:
  19 = 신의가호 둔기강타 — **6 프레임**(둔기강타 9 와 같은 수: 동작 길이 `a.n[5]` 는 시퀀스 9 의 프레임 수다, `cf.a()V`).
       시퀀스 9 의 포즈(이펙트 뺀 것)를 띄웠다 내리고, 착지(프레임 4 = 게임이 맞히는 프레임 `bx.a()Z` case 5)에 가까운 8 칸,
       프레임 5 에 먼 8 칸 — 타격 범위 16 칸 위에 폭발, 그 사이로 바람개비가 퍼진다.
  20 = 신의가호 검기 한 칸 — **3 프레임**(검기 17 과 같은 수: 그리는 쪽이 `cb.d` 를 프레임으로 그대로 넘긴다), 네 방향 같은 그림
       (불기둥은 방향이 없다 — 대각선 파동도 범위 밖 목록을 안 읽는다).
* 그리기 = `guest_anchors::CombatPatch` — 시퀀스 맵(`bx.a(B)B`)이 신의가호 중 둔기강타에 19 를, 검기 페인터(`ak.b(II)V`)가 신의가호
  주인의 파동에 20 을 그린다. 신의가호가 없으면 배포본 그림 그대로.
* 망토(`hero_cape.py`)는 이 뒤에 — 그래야 점프 포즈에도 망토가 달린다.

    python tools_re/knight_blessing.py preview <out.png> [zip]   # 시퀀스 19 · 20 을 네 방향으로 (몸 파츠는 배포본)
"""

import copy
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402
import venom_effect  # noqa: E402

ANIM = "hero/hero1"
SHEET = "hero/4/1"
SHIPPED_SEQS = 19
SHIPPED_FRAMES = 39
BLUDGEON_SEQ, WAVE_SEQ = 9, 17  # 둔기강타 (kind 5 + 4) · 검기의 파동
SLAM_SEQ, FLAME_SEQ = 19, 20  # = CombatPatch::BLESSED_SLAM_SEQ · BLESSED_WAVE_SEQ
EFFECT_PART = 4
IAN = "enm/5effect/11_0"
ULRIKE = "enm/5effect/12_0"

# 이안의 붉은 폭발 · 바람개비 → 푸른 계열 (나이트 자신의 파란 이펙트 #3f9fff 를 가운데에).
BLUE = {
    (0xFF, 0x00, 0x00): (0x2F, 0x6F, 0xFF),
    (0x9F, 0x3F, 0x3F): (0x1F, 0x3F, 0x9F),
    (0xFF, 0x9F, 0x3F): (0x3F, 0x9F, 0xFF),
    (0xFF, 0xBF, 0x3F): (0x7F, 0xCF, 0xFF),
    (0xFF, 0xFF, 0x7F): (0xBF, 0xEF, 0xFF),
    (0xFF, 0xFF, 0xFF): (0xFF, 0xFF, 0xFF),
}
# 율리케의 주황 불 → 붉은 불 (흰 심지는 그대로, 연기는 잿빛 붉게).
RED = {
    (0xFF, 0xDF, 0xBF): (0xFF, 0xB7, 0xA7),
    (0xFF, 0xFF, 0xFF): (0xFF, 0xFF, 0xFF),
    (0xFF, 0xBF, 0x3F): (0xFF, 0x3F, 0x3F),
    (0xFF, 0x00, 0x00): (0xB0, 0x00, 0x00),
    (0x7F, 0x7F, 0x7F): (0x5F, 0x2F, 0x2F),
    (0xBF, 0xBF, 0xBF): (0x9F, 0x5F, 0x5F),
    (0xDF, 0xDF, 0xBF): (0xDF, 0xAF, 0xAF),
    (0xBF, 0x1F, 0x3F): (0x8F, 0x0F, 0x1F),
}

# 새 그림 39..47 = (보스 시트, 프레임, 좌우 반전, 색).
BLAST, SWIRL, TONGUE, TONGUE_M, COLUMN, COLUMN_M, SMOKE, PUFF, EMBER = range(SHIPPED_FRAMES, SHIPPED_FRAMES + 9)
SOURCES = (
    (IAN, 6, False, BLUE),
    (IAN, 7, False, BLUE),
    (ULRIKE, 11, False, RED),
    (ULRIKE, 11, True, RED),
    (ULRIKE, 12, False, RED),
    (ULRIKE, 12, True, RED),
    (ULRIKE, 13, False, RED),
    (ULRIKE, 14, False, RED),
    (ULRIKE, 15, False, RED),
)
NO_EXTRA_FLAGS = b"\x88\x20"  # 이 시트에서 IHDR · IDAT 만 있는 프레임의 플래그 (배포본 1 · 20 · 36 · 38)

# 점프 내려찍기: (시퀀스 9 의 프레임, 더 띄우는 px) — 웅크림 · 솟음 · 꼭대기(9 의 공중 포즈 −28 위에 −6) · 내려옴 · 착지 · 충격파.
SLAM_POSES = ((0, 0), (1, -16), (3, -6), (3, 12), (4, 0), (5, 0))
LAND = 4  # 게임이 맞히는 프레임 (`bx.a()Z` case 5: g.w == 4)
TILE = 16
# 8 방향 = CombatPatch::ring_tiles 의 순서 (위 · 아래 · 왼 · 오른 · 대각 넷).
RING8 = ((0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1))
BLAST_LIFT = -6  # 폭발 가운데 = 칸 가운데보다 6 px 위 (서 있는 몬스터의 몸통). 그리는 원점 = 칸 가운데 (`ad.b` 8 · `ad.c` 9 − 1)
SWIRL_RADIUS = {LAND: 12, LAND + 1: 26}

# 불기둥 한 칸 = 율리케 시퀀스 7 의 프레임 0 · 1 · 2 그대로 (그림 번호만 새 것).
FLAME = (
    ((TONGUE, -8, -36), (TONGUE_M, 0, -36)),
    ((PUFF, -12, -13), (PUFF, 3, -13), (COLUMN, -3, -50), (COLUMN_M, 0, -50), (PUFF, -17, -5), (PUFF, -5, -2), (PUFF, 7, -5)),
    ((SMOKE, 0, -13), (SMOKE, -13, -9), (SMOKE, 3, -9), (SMOKE, -10, -13), (SMOKE, -5, -6),
     (EMBER, -2, -21), (EMBER, 1, -28), (EMBER, -3, -38), (EMBER, 0, -52)),
)


def _s8(v):
    return v - 256 if v > 127 else v


def _sprite(frame, x, y):
    if not (-128 <= x <= 127 and -128 <= y <= 127):
        raise ValueError(f"effect {frame} at ({x},{y}) does not fit a signed byte")
    return [EFFECT_PART, frame, x & 0xFF, y & 0xFF]


def _fx(frame_no, sheet_sizes):
    """(위쪽 = 몸 밑에 그릴 것, 아래쪽 = 몸 위에 그릴 것) — 착지 프레임과 그 다음 프레임에만."""
    if frame_no not in SWIRL_RADIUS:
        return [], []
    under, over = [], []
    # ★ 2026-10-03: the blessed 둔기강타 reaches THREE tiles out (CombatPatch::star_tiles) — the second
    # frame carries the far two rings' blasts.
    reaches = (1,) if frame_no == LAND else (2, 3)
    bw, bh = sheet_sizes[BLAST]
    sw, sh = sheet_sizes[SWIRL]
    for reach in reaches:
        for dx, dy in RING8:
            cx, cy = TILE * reach * dx, TILE * reach * dy + BLAST_LIFT
            (under if dy < 0 else over).append(_sprite(BLAST, cx - bw // 2, cy - bh // 2))
    r = SWIRL_RADIUS[frame_no]
    for dx, dy in RING8:
        n = math.hypot(dx, dy)
        cx, cy = round(r * dx / n), round(r * dy / n) + BLAST_LIFT
        (under if dy < 0 else over).append(_sprite(SWIRL, cx - sw // 2, cy - sh // 2))
    return under, over


def patch_anim(blob, sheet_sizes):
    """`hero/hero1` + 시퀀스 19 (점프 내려찍기) · 20 (불기둥 한 칸). 이미 있으면 ValueError."""
    header, parts, seqs = venom_effect.decode(blob)
    if len(seqs) != SHIPPED_SEQS or len(seqs[BLUDGEON_SEQ]) != 6 or len(seqs[WAVE_SEQ]) != 3:
        raise ValueError(f"{ANIM}: {len(seqs)} sequences — expected the shipped knight animation ({SHIPPED_SEQS})")
    slam = []
    for f, (src, lift) in enumerate(SLAM_POSES):
        under, over = _fx(f, sheet_sizes)
        directions = []
        for sprites in seqs[BLUDGEON_SEQ][src]:
            pose = [s for s in sprites if s[0] & 0x7F != EFFECT_PART]  # 둔기강타 자신의 섬광 · 금빛 입자는 뺀다
            if any(not -128 <= _s8(y) + lift <= 127 for _p, _f, _x, y in pose):
                raise ValueError(f"slam frame {f}: a lifted sprite leaves the signed byte")
            body = [[p, fr, x, (_s8(y) + lift) & 0xFF] for p, fr, x, y in pose]
            directions.append(copy.deepcopy(under) + body + copy.deepcopy(over))
        slam.append(directions)
    flame = [[[_sprite(fr, x, y) for fr, x, y in frame] for _d in range(4)] for frame in FLAME]
    if len(slam) != len(seqs[BLUDGEON_SEQ]) or len(flame) != len(seqs[WAVE_SEQ]):
        raise AssertionError("the new sequences must keep the shipped frame counts")
    seqs.append(slam)
    seqs.append(flame)
    out = venom_effect.encode(header, parts, seqs)
    _h, _p, again = venom_effect.decode(out)
    assert again[:SHIPPED_SEQS] == venom_effect.decode(blob)[2] and len(again) == FLAME_SEQ + 1, "read back"
    return out


def boss_frames(jar):
    """[(w, h, [[(r, g, b) or None]])] — 새 그림 39..47, 색을 바꾼 뒤."""
    sheets = {}
    out = []
    for base, k, mirror, colours in SOURCES:
        if base not in sheets:
            sheets[base] = cs.sheet_decode(jar.read(base + ".mph"), jar.read(base + "_0.mpd"), base)
        sh = sheets[base]
        fr = sh.frames[k]
        rows = []
        for row in fr.pixels:
            line = []
            for v in row:
                if v == 0:
                    line.append(None)
                    continue
                rgb = tuple(sh.palette[v])
                if rgb not in colours:
                    raise ValueError(f"{base} frame {k}: colour #{rgb[0]:02x}{rgb[1]:02x}{rgb[2]:02x} has no mapping")
                line.append(colours[rgb])
            rows.append(line[::-1] if mirror else line)
        out.append((fr.w, fr.h, rows))
    return out


def patch_sheet(mph, mpd, jar):
    """(mph, mpd) of `hero/4/1` + 그림 39..47. 이미 있으면 ValueError."""
    sheet = cs.sheet_decode(mph, mpd, SHEET)
    if len(sheet.frames) != SHIPPED_FRAMES:
        raise ValueError(f"{SHEET}: {len(sheet.frames)} frames — expected the shipped {SHIPPED_FRAMES}")
    palette = list(sheet.palette)
    index = {}
    for i, rgb in enumerate(palette):
        if i:  # 0 = 투명 (#ffffff) — 진짜 흰색은 0 이 아닌 칸에
            index.setdefault(tuple(rgb), i)
    frames = list(sheet.frames)
    for w, h, rows in boss_frames(jar):
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
        frames.append(cs.Frame(w, h, px, flags=NO_EXTRA_FLAGS))
    if len(palette) > 256:
        raise ValueError(f"{SHEET}: {len(palette)} colours")
    out = cs.Sheet(palette, frames, head=sheet.head, trns=sheet.trns)
    new_mph, new_mpd = cs.sheet_encode(out)
    back = cs.sheet_decode(new_mph, new_mpd, SHEET)
    assert [(f.w, f.h, f.pixels) for f in back.frames[:SHIPPED_FRAMES]] == [(f.w, f.h, f.pixels) for f in sheet.frames], "old frames moved"
    assert back.palette[: len(sheet.palette)] == sheet.palette, "old colours moved"
    return new_mph, new_mpd


def sheet_sizes_after(jar):
    """프레임 번호 → (w, h), 새 그림 포함 (애니메이션의 좌표가 그림 크기에서 나온다)."""
    sheet = cs.sheet_decode(jar.read(SHEET + ".mph"), jar.read(SHEET + "_0.mpd"), SHEET)
    sizes = {k: (f.w, f.h) for k, f in enumerate(sheet.frames)}
    for k, (w, h, _rows) in enumerate(boss_frames(jar)):
        sizes[SHIPPED_FRAMES + k] = (w, h)
    return sizes


def cmd_preview(out_png, zip_path):
    from PIL import Image, ImageDraw

    jar = cs.open_jar(zip_path)
    mph, mpd = patch_sheet(jar.read(SHEET + ".mph"), jar.read(SHEET + "_0.mpd"), jar)
    fx = cs.sheet_rgba(cs.sheet_decode(mph, mpd, SHEET))
    _h, _p, seqs = venom_effect.decode(patch_anim(jar.read(ANIM), sheet_sizes_after(jar)))
    table = {p: cs.sheet_by_path(jar, b) for p, b in cs.PREVIEW_GEAR[1].items()}
    for part in (1, 2, 3):  # 맨몸 — 몸통 · 팔 · 다리 100 (뛰어오르는 몸이 보이게)
        table[part] = cs.sheet_by_path(jar, f"hero/{part}/100")
    table[EFFECT_PART] = fx
    scale, cw, ch, ox, oy = 4, 112, 112, 56, 72
    rows = [(SLAM_SEQ, f) for f in range(len(seqs[SLAM_SEQ]))] + [(FLAME_SEQ, f) for f in range(len(seqs[FLAME_SEQ]))]
    img = Image.new("RGB", (4 * cw * scale, len(rows) * ch * scale), (27, 29, 35))
    dr = ImageDraw.Draw(img)
    for r, (s, f) in enumerate(rows):
        for d in range(4):
            # the tile grid: the knight's tile and the sixteen it hits
            for gx in range(-2, 3):
                for gy in range(-2, 3):
                    x0 = (d * cw + ox + gx * TILE - TILE // 2) * scale
                    y0 = (r * ch + oy + gy * TILE - TILE // 2) * scale
                    hit = (gx, gy) != (0, 0) and (gx == 0 or gy == 0 or abs(gx) == abs(gy))
                    dr.rectangle([x0, y0, x0 + TILE * scale - 1, y0 + TILE * scale - 1], outline=(70, 90, 120) if hit and s == SLAM_SEQ else (45, 48, 56))
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
