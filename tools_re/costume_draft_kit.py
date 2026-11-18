#!/usr/bin/env python3
"""의상 207..210 초안 생성기들이 같이 쓰는 그림 도구 (2026-10-09 — 가디언 · HELL 보상 넷).

사용자(2026-10-09)가 그림 넷을 주었다 — 가디언 EASY · NORMAL · HARD 클리어, HELL HARD 클리어의 보상:
  207 진홍의 검사  `costume_draft_crimson.py`   붉은 긴 머리 · 금관 · 검은 롱코트(붉은 안감 · 은 장식) · 붉은 태도
  208 순백의 해머  `costume_draft_hammer.py`    백금발 · 하얀 보닛(리본) · 어깨가 드러난 회색 블라우스 · 주름 치마 · 맨발 · 거대한 망치
  209 달빛 엘프    `costume_draft_elf.py`       금발 웨이브 · 엘프 귀 · 청록 보석관 · 별 무늬 청록 망토 + 하얀 드레스 · 초승달 지팡이
  210 백은의 사신  `costume_draft_reaper.py`    은발 · 푸른 꽃 · 금장식 상의 · 푸른 소매 · 금 사슬 허리띠 · 회색 긴 치마 · 검은 큰 낫

⛔ **PNG 가 정본이다** (`costumes/README.md`). 이 파일은 그 초안을 그린 도구다 — 바탕 실루엣은 배포본 시트 그대로:
  머리   긴 머리 틀 LONG_* — 맨머리 hero/0/100 의 얼굴 픽셀(눈 · 코 · 입 · 턱)을 그 자리 그대로 두고 머리칼만 새로 그렸다:
         앞 · 옆은 얼굴 양옆으로 프레임 맨 아래까지 내려오는 긴 머리, 뒤는 아래가 물결인 커튼
  몸통   2 = 손으로 그린 7×5 (모든 방향이 이 한 장) · 0 · 1 = 천 갑옷 hero/1/05 의 어깨 조각을 다시 칠함 · 3 · 4 = 빈 칸
  팔     hero/2/100 맨팔 — 손(장갑 시트 hero/2/0 의 푸른 칸)에서의 거리로 손 · 소매 · 어깨를 가른다
  다리   hero/3/102 로그 바지 — 맨 윗줄 · 열마다 맨 아래 두 줄(신)
  치마   다리 실루엣 둘레의 A 라인(율리케 201 의 기계) — 갑옷이 치마를 가지면 그 갑옷을 입는 동안 신발 칸 대신 다리가 된다
  무기   `costume_draft_lib.GRIPS` 의 (손잡이 → 끝) 축을 따라 모양을 그린다(프레임 크기 · 손 자리는 고정)

다리 프레임이 어느 방향에서 쓰이는가(직업 애니메이션 hero/hero0..2 전수, 2026-10-09): 9 = 앞 · 옆으로 서기(가장 많이),
10 · 4 · 5 · 1 = 뒤가 대부분, 6 · 7 · 8 = 옆 걸음, 0 · 2 · 3 = 앞 — 그래서 코트 · 치마의 뒷면은 BACK_LEGS 에만 그린다.
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_draft_lib as lib  # noqa: E402
import costume_sheets as cs  # noqa: E402
from costume_draft_lib import H, Canvas  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
jar = cs.open_jar(cs.DEFAULT_ZIP)

ALL_PARTS = ["head", "body", "arm", "leg", "skirt", "skirtunder", "skirtover", "weapon_berserker", "weapon_knight", "weapon_rogue", "offhand_knight", "offhand_rogue"]


def out_dir(default_name):
    args = [a for a in sys.argv[1:] if a != "--force"]
    out = args[0] if args else os.path.join(ROOT, "costumes", default_name)
    lib.refuse_overwrite(out, ALL_PARTS)
    return out


# ── 머리 ─────────────────────────────────────────────────────────────────────────
# 글자: O 머리 외곽 · D 그늘 · M 가운데 · L 밝게 · H 빛 — 얼굴은 맨머리 그대로(7 피부 외곽 · 8 · 9 · a 피부 · b 속눈썹 ·
# 6 흰자 · c 눈동자). 세트가 다른 글자(관 · 꽃 · 귀 …)를 덧그린다.
FACE = {"7": H("#9f3f3f"), "8": H("#ff9f7f"), "9": H("#df7f7f"), "a": H("#ffdfbf"), "b": H("#1f1f3f"), "6": H("#ffffff")}

LONG_BACK = [
    "................",
    ".....OOOOOO.....",
    "...OOLLHHLLOO...",
    "..OLLHHHLLLMMO..",
    "..OLHHLLLLMMDO..",
    ".OLLLLLLMMMMDDO.",
    ".OLMLLMMMMMMDDO.",
    ".OLMLMMMMMMDMDO.",
    ".OMMLMMDMMMDMDO.",
    ".OMMMMDMMMDMDDO.",
    ".OMDMMDMMDMMDDO.",
    ".ODMMDMMDMMDDDO.",
    ".ODDMDDMDDMDDDO.",
    "..ODDODDODDDOO..",
]
LONG_SIDE = [
    "...............",
    ".....OOOOO.....",
    "...OOLHHLMOO...",
    "..OLHHLLLLMMO..",
    "..OLHLLLLMMMDO.",
    ".OLLLLLMMMMMDO.",
    ".OLMLMMLMMMMDDO",
    ".OMDMMDMMMMMDDO",
    ".OM8OMDMMMMMDDO",
    "..78bbOMMMMMDDO",
    "..7ac6ODMMMDDDO",
    "..7aa89OMMMDDDO",
    "...7899OMMDDDO.",
    "....777ODDDDDO.",
]
LONG_FRONT = [
    "................",
    ".....OOOOOO.....",
    "...OOLHHLLMOO...",
    "..OLHHLLLLMMDO..",
    "..OLHLLLLMMMDO..",
    ".OLLLLLMMMMMDDO.",
    ".OLMLLMMLMMDMDO.",
    ".OMMDMMDMMDMDDO.",
    ".OLMDO89OO9ODMO.",
    ".OLMObb88bbOMDO.",
    ".OLM76caac67MDO.",
    ".OLM798aa897MDO.",
    ".OLMM798897MDDO.",
    ".OMMDO7777ODDDO.",
]


def check_face(arts):
    """긴 머리 틀이 맨머리의 얼굴을 한 픽셀도 옮기지 않았는가 — 얼굴 글자(7 8 9 a b 6 c)가 맨머리의 같은 자리에서 같은
    색이어야 한다(6 = 흰자/머리빛 공용 흰색이라 얼굴 안의 것만)."""
    bare = cs.shipped_sheet(jar, 0, 100)
    pal = bare.palette
    for k, art in enumerate(arts):
        fr = bare.frames[k]
        for y, line in enumerate(art):
            for x, ch in enumerate(line):
                if ch in "789abc6":
                    v = fr.pixels[y][x]
                    want = FACE.get(ch, H("#5f3f7f"))
                    if not v or pal[v] != want:
                        raise SystemExit(f"head frame {k}: ({x},{y}) {ch!r} is not the bare head's face pixel")


check_face([LONG_BACK, LONG_SIDE, LONG_FRONT])


def head_from_art(arts, hair, eye, extra=None):
    """arts = (뒤, 옆, 앞) 줄 목록 · hair = {O D M L H: rgb} · eye = 눈동자 · extra = 덧그린 글자 → rgb. 얼굴 글자는 맨머리의
    그 자리여야 한다(check_face) — 덧그린 장식이 얼굴을 밀어내지 않았는가."""
    check_face(arts)
    key = dict(FACE)
    key.update(hair)
    key["c"] = eye
    key.update(extra or {})
    return [Canvas.from_art(a, key) for a in arts]


# ── 몸통 0 · 1: 천 갑옷 hero/1/05 의 어깨 조각을 밝기대로 다시 칠한다 ───────────────
cloth = cs.shipped_sheet(jar, 1, 5)


def shoulders(ramp, outline):
    """ramp = 밝은 것부터 넷. 밝기 0 = 외곽."""
    out = []
    for k in (0, 1):
        fr = cloth.frames[k]
        cv = Canvas(fr.w, fr.h)
        for y, row in enumerate(fr.pixels):
            for x, v in enumerate(row):
                if not v:
                    continue
                lv = lib.lum(cloth.palette[v])
                cv.put(x, y, outline if lv < 0x28 else ramp[3] if lv < 0x50 else ramp[2] if lv < 0x70 else ramp[1] if lv < 0xA0 else ramp[0])
        out.append(cv)
    return out


def body(torso_art, key, shoulder_pieces=None):
    """몸통 다섯 — 2 = torso_art(7×5), 0 · 1 = shoulder_pieces(없으면 빈 칸), 3 · 4 = 빈 칸."""
    torso = Canvas.from_art(torso_art, key)
    if (torso.w, torso.h) != (7, 5):
        raise SystemExit("body 2 must be 7x5")
    s = shoulder_pieces or [lib.BLANK, lib.BLANK]
    return [s[0], s[1], torso, lib.BLANK, lib.BLANK]


# ── 팔 ───────────────────────────────────────────────────────────────────────────
arm_base = cs.shipped_sheet(jar, 2, 100)
glove = cs.shipped_sheet(jar, 2, 0)
GLOVE_BLUE = {i for i, c in enumerate(glove.palette) if i and c[2] > c[0] + 20}
# 맨팔의 피부 램프: 0 외곽 #5f1f3f · 1 #7f3f3f · 2 #bf5f3f · 3 #bf7f3f · 4 #ff9f7f · 5 #ffdfbf
ARM_LEVEL = {H("#5f1f3f"): 0, H("#7f3f3f"): 1, H("#bf5f3f"): 2, H("#bf7f3f"): 3, H("#ff9f7f"): 4, H("#ffdfbf"): 5}
SKIN = [H("#5f1f3f"), H("#7f3f3f"), H("#bf5f3f"), H("#bf7f3f"), H("#ff9f7f"), H("#ffdfbf")]


def bfs(frame, seeds):
    d, q = {s: 0 for s in seeds}, list(seeds)
    while q:
        x, y = q.pop(0)
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < frame.w and 0 <= ny < frame.h and frame.pixels[ny][nx] and (nx, ny) not in d:
                d[(nx, ny)] = d[(x, y)] + 1
                q.append((nx, ny))
    return d


def arms(paint):
    """paint(k, x, y, level 0..5, hand?, 손에서의 거리, 그 프레임의 최대 거리) → rgb. 손이 없는 프레임(팔뚝 조각)은 거리 0 ·
    최대 0 — 소매로 칠하면 된다."""
    out = []
    for k, fr in enumerate(arm_base.frames):
        hand = {(x, y) for y in range(fr.h) for x in range(fr.w) if fr.pixels[y][x] and glove.frames[k].pixels[y][x] in GLOVE_BLUE}
        far = bfs(fr, hand) if hand else {}
        reach = max(far.values()) if far else 0
        cv = Canvas(fr.w, fr.h)
        for y, row in enumerate(fr.pixels):
            for x, v in enumerate(row):
                if v:
                    cv.put(x, y, paint(k, x, y, ARM_LEVEL[arm_base.palette[v]], (x, y) in hand, far.get((x, y), 0), reach))
        out.append(cv)
    return out


# ── 다리 · 치마 ──────────────────────────────────────────────────────────────────
leg_base = cs.shipped_sheet(jar, 3, 102)
BACK_LEGS = {1, 4, 5, 10}


def leg_level(c):
    """로그 바지의 색 → 밝기 0(외곽) .. 4."""
    lv = lib.lum(c)
    return 0 if lv < 40 else 1 if lv < 70 else 2 if lv < 110 else 3 if lv < 165 else 4


class LegInfo:
    def __init__(self, k, fr):
        self.k, self.w, self.h = k, fr.w, fr.h
        self.rows = lib.rgb_rows(fr, leg_base.palette)
        self.top = min(y for y in range(fr.h) if any(self.rows[y]))
        self.bot = max(y for y in range(fr.h) if any(self.rows[y]))
        self.bottom = {x: max((y for y in range(fr.h) if self.rows[y][x] is not None), default=-1) for x in range(fr.w)}
        self.ext = {}
        for y in range(fr.h):
            xs = [x for x in range(fr.w) if self.rows[y][x] is not None]
            if xs:
                self.ext[y] = (min(xs), max(xs))
        self.back = k in BACK_LEGS

    def level(self, x, y):
        c = self.rows[y][x]
        return None if c is None else leg_level(c)


def legs(paint):
    """paint(info, x, y, level 0..4) → rgb (다리 실루엣의 칸마다)."""
    out = []
    for k, fr in enumerate(leg_base.frames):
        info = LegInfo(k, fr)
        cv = Canvas(fr.w, fr.h)
        for y in range(fr.h):
            for x in range(fr.w):
                lv = info.level(x, y)
                if lv is not None:
                    cv.put(x, y, paint(info, x, y, lv))
        out.append(cv)
    return out


def aline(info, hem=0, waist_half=2.5, flare=1.0):
    """율리케 201 의 A 라인: 허리 = 다리의 맨 윗줄(다리 가운데), 처음 몇 줄은 한 칸씩, 그 뒤 밑단까지 프레임 가장자리 쪽으로
    벌어진다 — 다리보다 좁아지지 않는다(걸음에 밑단이 같이 벌어진다). hem = 밑단을 바닥에서 몇 줄 위로. → {y: (x0, x1)}."""
    top, bottom = info.top, info.bot - hem
    cx = (info.ext[top][0] + info.ext[top][1]) / 2
    rows = {}
    for y in range(top, bottom + 1):
        t = (y - top) / max(1, bottom - top)
        reach = waist_half + min(y - top, 3) + flare * t * t * info.w / 2
        x0, x1 = round(cx - reach), round(cx + reach)
        if y in info.ext and y > top + 1:
            x0, x1 = min(x0, info.ext[y][0]), max(x1, info.ext[y][1])
        rows[y] = (max(0, x0), min(info.w - 1, x1))
    return rows


def write_skirt_layers(out, unders, overs):
    """★ 2026-10-09 층 치마 — 아랫층(다리 뒤) · 윗층(다리 앞) 을 `skirtunder_*` · `skirtover_*` 로 쓰고, 한 장짜리 `skirt_*`
    (옛 형식 — 다리까지 그린 치마)는 지운다(둘 다 있으면 costume_sheets 가 거절한다). 아무것도 없는 층 프레임 = 1×1 빈 칸.
    다리는 신발 칸이 그린다: 빌드가 층 + 다리를 한 장으로 합친다(costume_sheets.skirt_compose)."""
    def thin(frames):
        return [lib.BLANK if all(c is None for row in cv.px for c in row) else cv for cv in frames]

    for f in os.listdir(out) if os.path.isdir(out) else []:
        if f.startswith("skirt_") and f.endswith(".png"):
            os.remove(os.path.join(out, f))
    lib.write_part(out, "skirtunder", thin(unders))
    lib.write_part(out, "skirtover", thin(overs))


def skirts(paint, hem=0, waist_half=2.5, flare=1.0, round_hem=True):
    """치마(갑옷의 다리) 열하나 — paint(info, x, y, u 0..1 왼쪽→오른쪽, v 0..1 허리→밑단, edge?, last?) → rgb 또는 None
    (None = 그 칸은 다리 칸이면 다리 바탕 그대로가 아니라 투명: 치마가 다리를 대신하므로 보이고 싶은 다리는 paint 가 그린다)."""
    out = []
    for k, fr in enumerate(leg_base.frames):
        info = LegInfo(k, fr)
        rows = aline(info, hem, waist_half, flare)
        cv = Canvas(fr.w, fr.h)
        ys = sorted(rows)
        for y in ys:
            x0, x1 = rows[y]
            if y == ys[-1] and round_hem and x1 - x0 >= 5:
                x0, x1 = x0 + 1, x1 - 1
            span = max(1, x1 - x0)
            for x in range(x0, x1 + 1):
                c = paint(info, x, y, (x - x0) / span, (y - ys[0]) / max(1, ys[-1] - ys[0]), x in (x0, x1), y == ys[-1])
                if c is not None:
                    cv.put(x, y, c)
        out.append((info, rows, cv))
    return out


# ── 무기 ─────────────────────────────────────────────────────────────────────────


def frame_axis(part, k):
    (w, h), (gx, gy), (tx, ty) = lib.axis(part, k)
    ux, uy = lib._unit(tx - gx, ty - gy)
    return w, h, gx, gy, tx, ty, ux, uy, -uy, ux, math.hypot(tx - gx, ty - gy)


def room_side(w, h, cx, cy, px, py, reach):
    """법선 ±(px, py) 중 프레임 안에 더 오래 남는 쪽(+1 / −1)."""
    def room(sgn):
        n = 0
        for i in range(1, int(reach * 4) + 1):
            x, y = cx + px * sgn * i / 4, cy + py * sgn * i / 4
            if 0 <= x < w and 0 <= y < h:
                n += 1
            else:
                break
        return n
    return 1 if room(1) >= room(-1) else -1


# ── 리본: 중심선을 따라 폭이 변하는 띠 (칼날 · 자루 · 낫날) ─────────────────────────
# 원 사슬(lib.Shape)은 폭 2~3 px 짜리 칼날에서 외곽선이 칼날을 다 먹고 대각선에서 점선이 된다. 배포본 칼은 등(어두운 외곽) ·
# 몸 · 날(밝은 줄)이 또렷한 띠다 — 그래서 칸마다 중심선에서의 (길이 위치 t, 가로 위치 side −1..1)을 재서 칠한다.
N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))


def ribbon(w, h, pts, halfw):
    """pts = 중심선 [(x, y)] (2 개 이상), halfw = 점마다 반폭. → {(x, y): (t 0..1, side −1..1)} — side 의 부호 = 진행 방향의
    오른쪽(+) / 왼쪽(−). 끝은 둥글다(끝점까지의 거리)."""
    segs, total = [], 0.0
    for i in range(len(pts) - 1):
        L = math.hypot(pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1])
        segs.append((i, total, L))
        total += L
    total = total or 1.0
    hits = {}
    for y in range(h):
        for x in range(w):
            cx, cy = x + 0.5, y + 0.5
            best = None
            for i, s0, L in segs:
                ax, ay = pts[i]
                bx, by = pts[i + 1]
                vx, vy = bx - ax, by - ay
                tt = max(0.0, min(1.0, ((cx - ax) * vx + (cy - ay) * vy) / (L * L or 1.0)))
                qx, qy = ax + vx * tt, ay + vy * tt
                d = math.hypot(cx - qx, cy - qy)
                hw = halfw[i] + (halfw[i + 1] - halfw[i]) * tt
                if d <= hw and (best is None or d / hw < best[2]):
                    cross = vx * (cy - ay) - vy * (cx - ax)
                    best = ((s0 + tt * L) / total, (d / hw) * (1 if cross > 0 else -1), d / hw)
            if best:
                hits[(x, y)] = (best[0], best[1])
    return hits


class Layer:
    """hits = ribbon(…) 또는 {(x, y): (t, side)} · fill(t, side) → rgb · edge(t, side, lit?) → rgb 또는 None(fill 그대로)."""

    def __init__(self, hits, fill, edge=None):
        self.hits, self.fill, self.edge = hits, fill, edge


def compose(w, h, layers):
    """뒤의 층이 앞의 층을 덮는다. 외곽(4-이웃에 빈 칸) = 그 층의 edge — 바깥으로 열린 쪽이 빛(왼쪽 위)을 보면 lit."""
    owner = {}
    for li, layer in enumerate(layers):
        for p in layer.hits:
            owner[p] = li
    cv = Canvas(w, h)
    for (x, y), li in owner.items():
        t, side = layers[li].hits[(x, y)]
        c = layers[li].fill(t, side)
        out = [(dx, dy) for dx, dy in N4 if (x + dx, y + dy) not in owner]
        if out and layers[li].edge:
            ox, oy = sum(d[0] for d in out), sum(d[1] for d in out)
            lit = (ox or oy) and lib.facing(*lib._unit(ox, oy)) > 0.15
            e = layers[li].edge(t, side, bool(lit))
            if e is not None:
                c = e
        cv.put(x, y, c)
    return cv


def curve(gx, gy, ux, uy, px, py, start, end, bend, power=1.6, n=10):
    """축 위 start → end, 끝 쪽으로 갈수록(t^power) 법선 (px, py) 쪽으로 bend 만큼 휘는 중심선."""
    pts = []
    for i in range(n + 1):
        t = i / n
        d = start + (end - start) * t
        b = bend * t ** power
        pts.append((gx + ux * d + px * b, gy + uy * d + py * b))
    return pts


def clamp_pts(w, h, pts, m=0.6):
    return [(min(max(x, m), w - m), min(max(y, m), h - m)) for x, y in pts]


def bare_leg(info, x, y, lv):
    """로그 바지 실루엣 → 맨다리. 바지의 짙은 천(솔기 · 그늘)을 그대로 내리면 다리가 갈색 바지처럼 보인다(208 첫 판) — 실루엣의
    바깥 테두리만 짙게, 안쪽의 짙은 칸은 한 단 그늘로, 밝은 칸은 피부 빛으로."""
    rim = any(info.rows[y + dy][x + dx] is None if 0 <= x + dx < info.w and 0 <= y + dy < info.h else True for dx, dy in N4)
    if rim and lv <= 1:
        return SKIN[1]
    return [SKIN[3], SKIN[4], SKIN[4], SKIN[4], SKIN[5]][lv]
