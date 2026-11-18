#!/usr/bin/env python3
"""의상 초안 생성기들이 같이 쓰는 그림 도구 (2026-10-01 — 클레르 202 · 수영장 파티 203 · 할로윈 204).

`costume_draft_claire.py` · `costume_draft_pool.py` · `costume_draft_halloween.py` 가 쓴다. 그 앞의 초안(이안 · 율리케)은
저마다 같은 `write_part` 를 들고 있었다 — 여기 하나로 모았다(그 셋은 손대지 않는다).

⛔ **PNG 가 정본이다.** 생성기는 첫 초안의 기록이고, 이 파일은 그 그림 도구다(`costumes/README.md`).

내용:
  * `write_part` — 프레임 목록을 인덱스 PNG 로(불투명 색은 1 번부터: 0 번은 투명 칸이고 그 색이 #ffffff 라 진짜 흰색이
    0 번에 앉으면 뚫린다 — README ⛔) + 다시 읽어 투명이 정확히 그 자리인지 검사.
  * `Canvas` — 한 프레임. `from_art` = 글자 그림(한 글자 = 한 픽셀, '.' = 투명).
  * 무기 모양 — 파츠 시트의 무기 프레임은 **크기와 손잡이 자리가 고정**이다(직업 애니메이션이 프레임의 좌상단을 손 옆에
    놓는다). 그래서 모양은 프레임마다 `GRIPS` 의 (손잡이 → 끝) 축을 따라 그린다. 손잡이 = 직업 애니메이션 전 동작에서 그
    프레임 위에 손(맨팔 `hero/2/100` 의 장갑 자리)이 놓인 점들의 가운데(2026-10-01 실측), 끝 = 배포본 칼날의 반대쪽 끝.
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import costume_sheets as cs  # noqa: E402


def H(s):
    return (int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16))


def rgb_rows(frame, palette):
    return [[None if v == 0 else palette[v] for v in row] for row in frame.pixels]


def lum(c):
    return (2 * c[0] + 5 * c[1] + c[2]) // 8


def refuse_overwrite(out, parts):
    """이 생성기가 만드는 파트의 PNG 가 이미 있으면 거절(손으로 고친 것일 수 있다) — `--force` 가 아니면."""
    mine = [f for f in (os.listdir(out) if os.path.isdir(out) else []) if f.endswith(".png") and any(f.startswith(p + "_") for p in parts)]
    if mine and "--force" not in sys.argv:
        raise SystemExit(f"{out}: {len(mine)} 개의 PNG 가 이미 있다 — 손으로 고친 것일 수 있다. 덮어쓰려면 --force")


def write_part(out, part, frames):
    """frames = [(w, h, rows of rgb-or-None)] 또는 Canvas. 불투명 색은 1 번부터, 다시 읽어 투명을 검사한다."""
    frames = [f.rows() if isinstance(f, Canvas) else f for f in frames]
    opaque = []
    for _w, _h, rows in frames:
        for row in rows:
            for c in row:
                if c is not None and c not in opaque:
                    opaque.append(c)
    if len(opaque) > 255:
        raise SystemExit(f"{part}: {len(opaque)} colours — 255 까지")
    palette = [cs.TRANSPARENT_RGB] + opaque
    os.makedirs(out, exist_ok=True)
    for k, (w, h, rows) in enumerate(frames):
        if len(rows) != h or any(len(r) != w for r in rows):
            raise SystemExit(f"{part}_{k}: rows are not {w}x{h}")
        px = [[0 if c is None else 1 + opaque.index(c) for c in row] for row in rows]
        path = os.path.join(out, f"{part}_{k}.png")
        cs.png_write_indexed(path, w, h, palette, px)
        back = cs.Picture(path)
        holes = [(x, y) for y in range(h) for x in range(w) if (back.rgba(x, y)[3] == 0) != (rows[y][x] is None)]
        if holes:
            raise SystemExit(f"{path}: {len(holes)} pixel(s) with the wrong transparency, e.g. {holes[:5]}")
    print(f"{part}: {len(frames)} frames, {len(opaque)} colours")


def write_text(out, text):
    with open(os.path.join(out, "costume.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print("costume.txt")


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [[None] * w for _ in range(h)]

    @classmethod
    def from_art(cls, art, key):
        """art = 줄 목록(같은 길이), key = {글자: rgb 또는 None}. '.' 은 언제나 투명."""
        w, h = len(art[0]), len(art)
        c = cls(w, h)
        for y, line in enumerate(art):
            if len(line) != w:
                raise SystemExit(f"art row {y} is {len(line)} wide, not {w}: {line!r}")
            for x, ch in enumerate(line):
                c.px[y][x] = None if ch == "." else key[ch]
        return c

    @classmethod
    def from_frame(cls, frame, palette):
        c = cls(frame.w, frame.h)
        c.px = rgb_rows(frame, palette)
        return c

    def get(self, x, y):
        return self.px[y][x] if 0 <= x < self.w and 0 <= y < self.h else None

    def put(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = c

    def opaque(self, x, y):
        return self.get(x, y) is not None

    def stamp(self, other, ox, oy):
        """다른 캔버스를 (ox, oy) 에 — 투명 칸은 건너뛴다."""
        for y in range(other.h):
            for x in range(other.w):
                c = other.px[y][x]
                if c is not None:
                    self.put(ox + x, oy + y, c)

    def flipped_x(self):
        c = Canvas(self.w, self.h)
        c.px = [row[::-1] for row in self.px]
        return c

    def flipped_y(self):
        c = Canvas(self.w, self.h)
        c.px = [list(row) for row in self.px[::-1]]
        return c

    def rows(self):
        return (self.w, self.h, [list(r) for r in self.px])


BLANK = (1, 1, [[None]])

# ── 무기 프레임의 축 ─────────────────────────────────────────────────────────────
# (크기, 손잡이, 끝) — 손잡이 = 그 프레임 위에 손이 놓인 점들의 가운데(직업 애니메이션 전수, 2026-10-01), 끝 = 배포본
# 칼날의 반대쪽 끝(버서커 hero/6/000 · 나이트 207 · 로그 301 · 로그 둘째 단검 hero/7/301).
GRIPS = {
    "weapon_berserker": [
        ((10, 23), (3, 2), (8, 21)),
        ((25, 10), (22, 3), (1, 8)),
        ((18, 18), (15, 2), (2, 15)),
        ((24, 7), (1, 4), (23, 0)),
        ((24, 7), (1, 2), (23, 6)),
        ((3, 7), (1, 0), (1, 6)),
        ((3, 18), (1, 14), (1, 0)),
    ],
    "weapon_knight": [
        ((9, 14), (7, 12), (0, 0)),
        ((9, 14), (1, 1), (8, 13)),
        ((14, 9), (12, 1), (0, 8)),
        ((14, 9), (1, 7), (13, 0)),
        ((3, 16), (1, 0), (1, 15)),
    ],
    "weapon_rogue": [
        ((9, 14), (7, 12), (2, 4)),
        ((9, 14), (1, 1), (6, 9)),
        ((14, 9), (12, 1), (4, 6)),
        ((14, 9), (1, 7), (9, 2)),
    ],
    "offhand_rogue": [
        ((7, 10), (5, 8), (0, 0)),
        ((7, 10), (1, 1), (6, 9)),
        ((10, 7), (9, 1), (0, 6)),
        ((10, 7), (0, 5), (9, 0)),
    ],
}

LIGHT = (-0.6, -0.8)  # 빛이 오는 쪽 = 왼쪽 위 (배포본 칼날·방패의 밝은 쪽)


def _unit(dx, dy):
    n = math.hypot(dx, dy) or 1.0
    return dx / n, dy / n


def facing(nx, ny):
    """법선이 빛 쪽을 보는 정도 (−1 .. 1)."""
    return nx * LIGHT[0] + ny * LIGHT[1]


def _shade(nx, ny, ramp, bias=0.0):
    """법선 (nx, ny) 과 빛의 내적 → 램프(밝은 것부터)의 한 칸."""
    d = facing(nx, ny)
    t = (1.0 - (d + bias)) / 2.0  # 0 = 가장 밝게, 1 = 가장 어둡게
    i = int(round(t * (len(ramp) - 1)))
    return ramp[max(0, min(len(ramp) - 1, i))]


def outline_shape(cv, inside, colour):
    """inside(x, y) 인 칸 중 4-이웃에 바깥이 있는 칸 = 외곽선 색."""
    edge = []
    for y in range(cv.h):
        for x in range(cv.w):
            if inside(x, y) and any(not inside(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                edge.append((x, y))
    for x, y in edge:
        cv.put(x, y, colour)


class Shape:
    """원 · 캡슐 · 타원의 합 — 픽셀 중심이 어느 조각 안에 드는지, 그 조각의 법선은 어디로 향하는지."""

    def __init__(self):
        self.parts = []  # (kind, data, tag)

    def circle(self, cx, cy, r, tag=None):
        self.parts.append(("c", (cx, cy, r), tag))
        return self

    def capsule(self, ax, ay, bx, by, r, tag=None):
        self.parts.append(("s", (ax, ay, bx, by, r), tag))
        return self

    def ellipse(self, cx, cy, ux, uy, a, b, tag=None):
        """중심 (cx, cy), 장축 방향 (ux, uy)(단위), 반지름 a(장축) · b(단축)."""
        self.parts.append(("e", (cx, cy, ux, uy, a, b), tag))
        return self

    def hit(self, px, py):
        """(깊이, 법선 x, 법선 y, tag) — 가장 깊이 든 조각, 아니면 None. 깊이 = 0(가장자리) .. 1(가운데)."""
        best = None
        for kind, d, tag in self.parts:
            if kind == "c":
                cx, cy, r = d
                dx, dy = px - cx, py - cy
                dist = math.hypot(dx, dy)
                depth = 1 - dist / r
                n = _unit(dx, dy) if dist > 1e-6 else (0.0, 0.0)
                s = dist / r
            elif kind == "s":
                ax, ay, bx, by, r = d
                vx, vy = bx - ax, by - ay
                L2 = vx * vx + vy * vy or 1.0
                t = max(0.0, min(1.0, ((px - ax) * vx + (py - ay) * vy) / L2))
                qx, qy = ax + t * vx, ay + t * vy
                dx, dy = px - qx, py - qy
                dist = math.hypot(dx, dy)
                depth = 1 - dist / r
                n = _unit(dx, dy) if dist > 1e-6 else (0.0, 0.0)
                s = dist / r
            else:
                cx, cy, ux, uy, a, b = d
                dx, dy = px - cx, py - cy
                u = dx * ux + dy * uy
                v = -dx * uy + dy * ux
                s = math.hypot(u / a, v / b)
                depth = 1 - s
                # 타원의 법선: (u/a², v/b²) 를 다시 화면 축으로
                gu, gv = u / (a * a), v / (b * b)
                n = _unit(gu * ux - gv * uy, gu * uy + gv * ux) if s > 1e-6 else (0.0, 0.0)
            if s <= 1.0 and (best is None or depth > best[0]):
                best = (depth, n[0], n[1], tag)
        return best


def render_shape(w, h, shape, ramp_for, outline_for, highlight_for=None, lit_outline_for=None):
    """shape 를 w×h 에 — 칸마다 ramp_for(tag)(밝은 것부터)로 명암, 외곽은 outline_for(tag), 가운데 가까운 빛 쪽 한 점은
    highlight_for(tag)(있으면). lit_outline_for(tag) 가 있으면 빛을 보는 쪽 외곽은 그 색(배포본 칼날처럼 — 폭 2~3 px 짜리
    모양이 외곽선에 다 먹히지 않게: 어두운 외곽은 그늘 쪽에만)."""
    cv = Canvas(w, h)
    hits = {}
    for y in range(h):
        for x in range(w):
            hb = shape.hit(x + 0.5, y + 0.5)
            if hb is not None:
                hits[(x, y)] = hb
    for (x, y), (depth, nx, ny, tag) in hits.items():
        ramp = ramp_for(tag)
        # 깊은 곳(가운데)은 법선이 약하다 — 깊이가 클수록 중간 밝기 쪽으로.
        k = max(0.0, min(1.0, 1.0 - depth))
        cv.put(x, y, _shade(nx * k, ny * k, ramp))
    inside = lambda x, y: (x, y) in hits  # noqa: E731
    for (x, y), (_d, nx, ny, tag) in hits.items():
        out = [(dx, dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)) if not inside(x + dx, y + dy)]
        if out:
            o = outline_for(tag)
            if lit_outline_for is not None:
                # 바깥으로 열린 쪽의 평균 방향이 빛을 보면 밝은 외곽
                ox, oy = sum(d[0] for d in out), sum(d[1] for d in out)
                if facing(*_unit(ox, oy)) > 0.15 if (ox or oy) else facing(nx, ny) > 0.15:
                    o = lit_outline_for(tag)
            if o is not None:
                cv.put(x, y, o)
    if highlight_for is not None:
        for (x, y), (depth, nx, ny, tag) in hits.items():
            hl = highlight_for(tag)
            if hl is None:
                continue
            # 빛 쪽 가장자리 바로 안쪽: 법선이 빛을 향하고(내적 > .55) 외곽이 아닌 칸
            on_edge = any(not inside(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
            if not on_edge and facing(nx, ny) > 0.55 and 0.15 < depth < 0.6:
                cv.put(x, y, hl)
    return cv


def axis(part, k):
    (w, h), g, t = GRIPS[part][k]
    return (w, h), (g[0] + 0.5, g[1] + 0.5), (t[0] + 0.5, t[1] + 0.5)
