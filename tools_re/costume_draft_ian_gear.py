#!/usr/bin/env python3
"""의상 200 「이안의 낡은 옷」 머리·무기·보조손 초안 생성기 (2026-10-01, A3b) — 배포본 시트의 실루엣을 그대로 두고
이안의 색으로 다시 칠한다 (몸통·팔·다리 = `costume_draft_ian.py`).

    python tools_re/costume_draft_ian_gear.py            # costumes/200_ian/ 에 head_ · weapon_<직업>_ · offhand_<직업>_ PNG
    python tools_re/costume_draft_ian_gear.py --force    # 덮어쓰기 — ⛔ 손으로 고친 PNG 가 사라진다

⛔ **PNG 가 정본이다.** 이 스크립트는 첫 초안의 기록이다 — 사용자가 고친 뒤에 다시 돌리면 그 수정을 덮어쓴다(그래서
기본은 거절, 몸통·팔·다리 PNG 는 건드리지 않는다). 사용자 결정(2026-09-30 밤): 아바타 = 장비 전부 · 이안의 머리·무기도
Claude 초안 → 사용자 수정. 보조손(나이트 방패 · 로그 둘째 단검)은 무기 세트로 같이 그렸다.

바탕(실루엣은 한 픽셀도 안 바꾼다 — HERO2_AVATAR_PLAN.md §8.1 「가장 안전한 제작법」, 크기표 §8.2):
  머리          hero/0/100(맨머리, 금발의 주인공) — 금발 → 이안의 은회색 머리(명암은 NPC 이안 npc/1head/00 의 모브 그림자),
                눈 → 초록(얼굴 3), 피부는 그대로
  무기 버서커   hero/6/000(양손대검) · 나이트 hero/6/207(도검) · 로그 hero/6/301(단검)
  보조손 나이트 hero/7/1008(십자 방패) · 로그 hero/7/301(둘째 단검)
  무기 색 = 「빙해의 검사」 — 청록 칼날 → 차가운 은빛 라벤더(외곽은 남색), 손잡이 → 진홍(이안의 허리띠) · 코등이 → 카키
  방패 색 = 남색 판 · 진홍 십자 · 카키 테
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--force"]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "costumes", "200_ian")
PARTS = ["head", "weapon_berserker", "weapon_knight", "weapon_rogue", "offhand_knight", "offhand_rogue"]
mine = [f for f in (os.listdir(OUT) if os.path.isdir(OUT) else []) if f.endswith(".png") and any(f.startswith(p + "_") for p in PARTS)]
if mine and "--force" not in sys.argv:
    raise SystemExit(f"{OUT}: {len(mine)} 개의 머리·무기·보조손 PNG 가 이미 있다 — 손으로 고친 것일 수 있다. 덮어쓰려면 --force")


def H(s):
    return (int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16))


def table(**kw):
    return {H("#" + k[1:]): H(v) for k, v in kw.items()}


jar = cs.open_jar(cs.DEFAULT_ZIP)


def remap(frame, palette, colours, where):
    rows = []
    for y, row in enumerate(frame.pixels):
        out = []
        for x, v in enumerate(row):
            if not v:
                out.append(None)
                continue
            src = palette[v]
            if src not in colours:
                raise SystemExit(f"{where}: no mapping for #{src[0]:02x}{src[1]:02x}{src[2]:02x} at ({x},{y})")
            out.append(colours[src])
        rows.append(out)
    return (frame.w, frame.h, rows)


def write_part(part, frames_rgb):
    # ⛔ Index 0 is the TRANSPARENT entry and its colour is #ffffff: an opaque white must get an index
    # of its own. (2026-10-01: `c not in palette` found white at index 0 and painted the head's hair
    # shine and eye whites — and every blade highlight — see-through; the user saw the eyes change
    # colour with the background.) The shipped sheets do the same: two #ffffff entries.
    opaque = []
    for _w, _h, rows in frames_rgb:
        for row in rows:
            for c in row:
                if c is not None and c not in opaque:
                    opaque.append(c)
    palette = [cs.TRANSPARENT_RGB] + opaque
    os.makedirs(OUT, exist_ok=True)
    for k, (w, h, rows) in enumerate(frames_rgb):
        px = [[0 if c is None else 1 + opaque.index(c) for c in row] for row in rows]
        path = os.path.join(OUT, f"{part}_{k}.png")
        cs.png_write_indexed(path, w, h, palette, px)
        # Read it back: transparent exactly where the source was, never elsewhere.
        pic = cs.Picture(path)
        holes = [(x, y) for y in range(h) for x in range(w) if (pic.rgba(x, y)[3] == 0) != (rows[y][x] is None)]
        if holes:
            raise SystemExit(f"{path}: {len(holes)} pixel(s) with the wrong transparency, e.g. {holes[:5]}")
    print(part, len(frames_rgb), "frames,", len(opaque), "colours")


def from_shipped(part, stem, colours):
    d = cs.PARTS[part]["dir"]
    sheet = cs.shipped_sheet(jar, d, stem)
    write_part(part, [remap(fr, sheet.palette, colours, f"hero/{d}/{stem}") for fr in sheet.frames])


# ── 머리: 금발 램프 → 은회색 (NPC 이안의 #dfbfbf · #bfbfbf · #9e9996 · #7f5f7f · #3f3f3f 계열) ──
HEAD = table(
    xffff7f="#dfdfdf",  # 가장 밝은 머리칼
    xffbf3f="#bfbfbf",
    xdf9f3f="#9f9f9f",
    xbf7f3f="#7f5f7f",  # 모브 그림자 (NPC 이안)
    x7f3f3f="#3f3f3f",  # 머리 외곽
    x5f3f7f="#3f5f3f",  # 눈동자 → 초록 (얼굴 3)
    x1f1f3f="#1f3f3f",
    xffffff="#ffffff",  # 머리의 흰 빛 · 흰자
    x9f3f3f="#9f3f3f",  # 피부 외곽 · 입
    xff9f7f="#ff9f7f",
    xdf7f7f="#df7f7f",
    xffdfbf="#ffdfbf",
)
from_shipped("head", 100, HEAD)

# ── 무기: 청록 칼날 → 은빛 라벤더, 손잡이 → 진홍, 코등이 → 카키 ──
BLADE = table(
    x000000="#000000",
    x1f1f3f="#1f1f3f",
    x1f3f3f="#1f1f3f",
    x1f5f7f="#3f3f5f",  # 칼날 외곽 → 남색
    x3f7f7f="#5f5f7f",
    x5f7f7f="#7f7f9f",
    x9fbfbf="#9f9fbf",
    xbfdfff="#bfbfdf",
    xdfffff="#dfdfff",
    xffffff="#ffffff",
    xbf9fbf="#bf9fbf",
    x7f1f3f="#9f1f3f",
    xbf5f3f="#9f1f3f",  # 손잡이 감개 → 진홍 (이안의 허리띠)
    x7f3f3f="#5f1f3f",
    xffff7f="#dfbfbf",
    xdfbf3f="#9f9f7f",  # 코등이 → 카키
)
from_shipped("weapon_berserker", "000", BLADE)
from_shipped("weapon_knight", "207", BLADE)
from_shipped("weapon_rogue", "301", BLADE)
from_shipped("offhand_rogue", "301", BLADE)

# ── 방패: 푸른 판 → 남색, 분홍 십자 → 진홍, 회색 테 → 카키 ──
SHIELD = table(
    x000000="#000000",
    x1f1f3f="#1f1f3f",
    x1f3f3f="#1f1f3f",
    x3f5f7f="#1f1f3f",
    x5f7fbf="#3f3f5f",
    x7f9fbf="#5f5f7f",
    x9fbfbf="#7f7f9f",
    xbfdfff="#9f9fbf",
    xffffff="#dfbfbf",
    xdf1f7f="#bf1f3f",
    x9f1f7f="#9f1f3f",
    x7f1f3f="#5f1f3f",
    x3f3f3f="#5f5f3f",
    x7f7f7f="#9f9f7f",
    x5f5f7f="#7f7f5f",
)
from_shipped("offhand_knight", "1008", SHIELD)
