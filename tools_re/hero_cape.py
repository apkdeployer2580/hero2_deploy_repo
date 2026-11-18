#!/usr/bin/env python3
"""망토 — 직업 애니메이션(`hero/hero0..2`)의 모든 포즈에 망토와 스카프를 그리게 한다 (2026-10-01).

사용자(2026-10-01): *"이안의 낡은 옷에 망토도 같이 포함되어야해. 망토는 소문의 검사 외형을 확인해보면 알 수있어."*

── 보스의 망토 (HERO2_GUEST_RE.md §8.38) ───────────────────────────────────────────────
보스 이안(#89)과 「소문의 검사」(#91)는 같은 망토를 두른다 — 몸통 파츠가 아니라 **이펙트 파트**
`enm/5effect/11_0` 의 프레임 11 · 12 · 13(바람에 날리는 붉은 망토 26×14 · 24×14 · 26×14, 서기·걷기 내내 번갈아)과
10(붉은 스카프 9×3). 보스 애니메이션 `enm/spr/14` 가 포즈마다 몸통 조각 옆에 그린다:

| 방향 | 몸통 → 망토 | 그리는 순서 | 몸통 → 스카프 |
|---|---|---|---|
| 0 뒤 | (0, −1) | 맨 끝 (몸 위) | — |
| 1 앞 | (−1, 0) | 맨 앞 (몸 밑) | (−1, 0), 머리 바로 뒤 |
| 2 왼쪽 | (0, −1) | 맨 앞 (몸 밑) | (−2, 0), 머리 바로 뒤 |
| 3 오른쪽 | (−3, 0) | 맨 끝 (몸 위) | — |

── 플레이어에게 ────────────────────────────────────────────────────────────────────
플레이어의 직업 애니메이션도 같은 뼈대다(가슴 조각 = 몸통 파트 1 의 프레임 2, 보스 몸통과 같은 왼쪽 위). 그래서 포즈마다
그 가슴 조각의 자리에서 위 표대로 **몸통 파트 1 의 프레임 5 · 6 · 7(망토 a · b · c, 애니메이션 프레임 번호 % 3)과 8(스카프)**
을 더한다. 그림은 몸통 시트에 있다 — 이안의 낡은 옷(`costumes/200_ian/body_5..8.png` = 보스의 그 픽셀)만 그리고, 다른 모든
몸통 시트는 1×1 빈 칸이다(`costume_sheets.pad_sheet`, `make_hero2_asset.py`). ⛔ 게임의 그리는 쪽
(`n.a([BIBB[Object;ZBBII)B` @0x163374)은 프레임 바이트가 −1 이거나 그림이 null 이면 건너뛰지만 **배열 밖 인덱스는 던진다** —
그래서 망토 없는 시트도 프레임 9 개여야 한다(빈 칸 = 그려지는 것 없음, 기준선 md5 그대로).

    python tools_re/hero_cape.py extract costumes/200_ian      # body_5..8.png ← enm/5effect/11_0 의 망토·스카프 (있으면 거절)
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402
import venom_effect  # noqa: E402

TORSO = (1, 2)  # 몸통 파트 1 의 프레임 2 — 모든 방향이 그리는 가슴 조각
CAPE_FRAMES = (5, 6, 7)  # 몸통 시트의 망토 a · b · c
SCARF_FRAME = 8  # 몸통 시트의 스카프
CAPE_AT = {0: (0, -1), 1: (-1, 0), 2: (0, -1), 3: (-3, 0)}
CAPE_UNDER = frozenset({1, 2})
SCARF_AT = {1: (-1, 0), 2: (-2, 0)}
# 보스 시트의 그림 — 이안의 낡은 옷이 받는 픽셀.
BOSS_SHEET = "enm/5effect/11_0"
BOSS_FRAMES = {5: 11, 6: 12, 7: 13, 8: 10}


def _s8(v):
    return v - 256 if v > 127 else v


def _first(sprites, want):
    return next(((_s8(dx), _s8(dy)) for p, f, dx, dy in sprites if want(p & 0x7F, f)), None)


def patch(blob):
    """(새 애니메이션, 망토를 단 포즈 수). 이미 망토가 있으면 ValueError.

    기준 = 가슴 조각. 웅크리기·찌르기처럼 가슴 조각을 안 그리는 공격 포즈(직업마다 30 여 개)는 **머리**에서 가슴 자리를
    짐작한다 — 그 직업의 서기 포즈(시퀀스 0 프레임 0)에서 잰 방향별 머리 → 가슴 거리로. 머리도 가슴도 없는 포즈(빈 포즈 ·
    이펙트만 있는 포즈)에는 달지 않는다."""
    header, parts, seqs = venom_effect.decode(blob)
    caped = 0
    ours = set(CAPE_FRAMES) | {SCARF_FRAME}
    head_to_torso = {}
    for d, sprites in enumerate(seqs[0][0]):
        h, t = _first(sprites, lambda p, f: p == 0), _first(sprites, lambda p, f: (p, f) == TORSO)
        if h is None or t is None:
            raise ValueError(f"the stance (sequence 0, frame 0) has no head or chest facing {d}")
        head_to_torso[d] = (t[0] - h[0], t[1] - h[1])
    for seq in seqs:
        for fi, frame in enumerate(seq):
            for d, sprites in enumerate(frame):
                if any((p & 0x7F) == TORSO[0] and f in ours for p, f, _dx, _dy in sprites):
                    raise ValueError("this animation already carries the cape")
                torso = _first(sprites, lambda p, f: (p, f) == TORSO)
                if torso is None:
                    head = _first(sprites, lambda p, f: p == 0)
                    if head is None:
                        continue
                    torso = (head[0] + head_to_torso[d][0], head[1] + head_to_torso[d][1])
                tx, ty = torso
                ox, oy = CAPE_AT[d]
                cape = [TORSO[0], CAPE_FRAMES[fi % len(CAPE_FRAMES)], (tx + ox) & 0xFF, (ty + oy) & 0xFF]
                if d in SCARF_AT:
                    sx, sy = SCARF_AT[d]
                    head = next((i for i, s in enumerate(sprites) if (s[0] & 0x7F) == 0), None)
                    at = len(sprites) if head is None else head + 1
                    sprites.insert(at, [TORSO[0], SCARF_FRAME, (tx + sx) & 0xFF, (ty + sy) & 0xFF])
                if d in CAPE_UNDER:
                    sprites.insert(0, cape)
                else:
                    sprites.append(cape)
                if len(sprites) > 255:
                    raise ValueError("a pose would have more than 255 sprites")
                caped += 1
    out = venom_effect.encode(header, parts, seqs)
    # Read it back: the same poses, each with its cape where the table puts it.
    _h, _p, again = venom_effect.decode(out)
    capes = sum(1 for s in again for f in s for spr in f if any((p & 0x7F) == TORSO[0] and fr in CAPE_FRAMES for p, fr, _x, _y in spr))
    assert capes == caped, (capes, caped)
    return out, caped


def cmd_extract(costume_dir):
    jar = cs.open_jar(cs.DEFAULT_ZIP)
    sheet = cs.sheet_decode(jar.read(BOSS_SHEET + ".mph"), jar.read(BOSS_SHEET + "_0.mpd"), BOSS_SHEET)
    sizes = cs.PARTS["body"]["sizes"]
    for k, src in BOSS_FRAMES.items():
        path = os.path.join(costume_dir, f"body_{k}.png")
        if os.path.exists(path):
            raise SystemExit(f"{path}: 이미 있다 — 손으로 고친 것일 수 있다")
        fr = sheet.frames[src]
        if (fr.w, fr.h) != sizes[k]:
            raise SystemExit(f"{BOSS_SHEET} frame {src}: {fr.w}x{fr.h}, the body table wants {sizes[k]}")
        used = sorted({v for row in fr.pixels for v in row if v})
        palette = [cs.TRANSPARENT_RGB] + [sheet.palette[i] for i in used]
        remap = {0: 0, **{i: 1 + n for n, i in enumerate(used)}}
        cs.png_write_indexed(path, fr.w, fr.h, palette, [[remap[v] for v in row] for row in fr.pixels])
        print(f"{path}: {fr.w}x{fr.h} ← {BOSS_SHEET} frame {src} ({len(used)} colours)")
    return 0


def main():
    args = sys.argv[1:]
    if len(args) == 2 and args[0] == "extract":
        return cmd_extract(args[1])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
