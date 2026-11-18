#!/usr/bin/env python3
"""「악마화」 이펙트 리뉴얼 — 초안 모의 그림 (2026-10-13, Claude 초안 · 코드 미착수).

사용자: *"악의 이펙트를 리뉴얼할꺼야. dist/악의 이펙트 폴더내에 참고자료들은 전부 넣어놨어. … 용암불과 첫 변신 이펙트는 내가
지정해놨어. 나머지 항목중 너가 스킬에 어울리는거로 뽑아서 초안 뽑아줘봐."* — 참고 = `dist/악의 이펙트/`(DNF 데몬 변신 영상
스크린샷 51 장: `start1..6` = 변신(사용자 지정) · `용암불1..2` = 용암불(사용자 지정) · 나머지 43 장은 이 파일의 `REFS` 가 스킬에
배정한다).

이 도구는 **게임 해상도(176 px 폭 · 타일 16 px · 악마 30×36)의 모의 그림**을 만든다 — 호스트 블록이 그릴 수 있는 것(게임
Graphics 의 선 · 사각 · 반투명 · 시트 프레임)만 쓰도록 모양을 잡았고, 각 패널 오른쪽에 그 스킬에 배정한 참고 스크린샷을 같은
높이로 붙인다(⛔ 규율: 그림은 레퍼런스 옆에 놓고 판정할 것 — §8.55). 지금 설치된 이펙트(선 · 사각 프리미티브, D3~D5)와
무엇이 달라지는지는 패널 머리글에 적혀 있다.

    python tools_re/demon_fx_draft.py [--out docs/skill/demon_fx_draft] [--zoom 3] [--refs "dist/악의 이펙트"]

산출물 = `<out>/<skill>.png` 8 장 + `<out>/sheet.png`(전부). 배경 = 숲 시험대의 probe 캡처(`docs/skill/demon_charge_2026-10-13.png`)
한 조각 · 악마 = `costumes/demon_300/demon_<k>.png`(D2 초안 도트 그대로) · 몬스터 = 같은 캡처의 반란전투병 조각.
"""

import argparse
import math
import os
import random
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEMON_DIR = os.path.join(ROOT, "costumes", "demon_300")
BG_SRC = os.path.join(ROOT, "docs", "skill", "demon_charge_2026-10-13.png")
FONT = os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf")

W, H = 176, 160  # 모의 창 (게임 px) — 세로 160 이면 13×13 멸화가 거의 꽉 찬다
FX, FY = 88, 108  # 악마의 발바닥 (화면 중앙 아래)
TILE = 16

# 악마 프레임 번호 (demon_look.POSES × 4 방향 — README.txt) · 방향 0 뒤 1 앞 2 왼 3 오른
BACK, FRONT, LEFT, RIGHT = 0, 1, 2, 3
POSES = ["stand", "walk0", "walk1", "walk2", "walk3", "claw_a", "claw_b", "sweep_low", "sweep_high", "slam_up", "slam_down", "roar"]

# ── 참고 스크린샷 배정 (이 파일이 정본 — 사용자 확인 대기) ───────────────────────────────────────────────────
REFS = {
    "transform": ["start1.jpg", "start3.jpg", "start4.jpg", "start6.jpg"],  # ★ 사용자 지정
    "lava": ["용암불1.jpg", "용암불2.jpg"],  # ★ 사용자 지정
    "claw": ["Screenshot_20261013_203958_YouTube.jpg", "Screenshot_20261013_204001_YouTube.jpg",
             "Screenshot_20261013_204003_YouTube.jpg", "Screenshot_20261013_204048_YouTube.jpg"],
    "charge": ["Screenshot_20261013_204355_YouTube.jpg", "Screenshot_20261013_204356_YouTube.jpg",
               "Screenshot_20261013_204357_YouTube.jpg", "Screenshot_20261013_204359_YouTube.jpg"],
    "follow": ["Screenshot_20261013_204411_YouTube.jpg", "Screenshot_20261013_204404_YouTube.jpg",
               "Screenshot_20261013_204435_YouTube.jpg", "Screenshot_20261013_204425_YouTube.jpg"],
    "blade": ["Screenshot_20261013_204406_YouTube.jpg", "Screenshot_20261013_204432_YouTube.jpg",
              "Screenshot_20261013_204407_YouTube.jpg"],
    "judge": ["Screenshot_20261013_204437_YouTube.jpg", "Screenshot_20261013_204409_YouTube.jpg",
              "Screenshot_20261013_204446_YouTube.jpg"],
    "fire": ["Screenshot_20261013_204145_YouTube.jpg", "Screenshot_20261013_204121_YouTube.jpg",
             "Screenshot_20261013_204128_YouTube.jpg", "Screenshot_20261013_204113_YouTube.jpg"],
}

TITLES = {
    "transform": "변신 (악의 시전) — 사용자 지정 start1~6: 검은 실루엣 + 자홍 불꽃 날개 + 붉은 기운 + 검은 조각 → 악마 착지 흙먼지 고리",
    "claw": "할퀴기 3 콤보 — 1단 붉은 초승달 ×2 · 2단 노란 큰 호 쓸어올리기 · 3단 세 줄 긁어내리기 + 흙먼지",
    "charge": "돌진 — 머리의 붉은 불꽃 갈기 · 발 뒤 흙먼지 + 돌 · 잔상 2 · 닿으면 흰 섬광",
    "follow": "추종 — 게이지 = 보라 기운 알갱이 ↑ · 100 % = 보라 반구 + 자홍 불꽃 · 끝 = 검은 연기 폭발 + 섬광 + 어둠 가시 + 보라 반구 확산",
    "blade": "지옥칼날 — 바닥의 검붉은 인장(회전하는 검은 소용돌이 셋) · 몬스터 칸에서 솟는 보라 수정 칼날 + 노란 불씨",
    "judge": "어둠심판 — 선딜 = 머리 위 보라 구체(커진다) · 판정 = 하늘에서 떨어지는 보라 번개 + 가시 폭발",
    "fire": "멸화 — 선딜 = 가슴의 붉은 핵 + 주황 고리 · 폭발 = 흰 섬광 → 4 겹 불덩이 + 치솟는 불혀 + 검은 연기 → 연기",
    "lava": "용암불 — 사용자 지정 용암불1~2: 칸마다 붉은 용암 웅덩이 + 흔들리는 불혀 + 떠오르는 불씨, 5 초에 걸쳐 식는다",
}

# 색 (게임 Graphics 는 24 bit — 팔레트 제약 없음; 시트 프레임으로 가면 ≤ 15 색)
BLACK = (15, 7, 15)
MAGENTA = (236, 72, 210)
PINK = (255, 170, 240)
WHITE = (255, 255, 255)
RED = (220, 30, 40)
RED_HOT = (255, 60, 30)
ORANGE = (255, 130, 30)
AMBER = (255, 200, 60)
YELLOW_PALE = (255, 240, 170)
SAND = (232, 210, 165)
SAND_RIM = (170, 140, 100)
ROCK = (110, 85, 55)
PURPLE = (130, 50, 220)
PURPLE_LT = (200, 120, 255)
PURPLE_DK = (60, 10, 90)
SMOKE = (30, 18, 36)
LAVA = (200, 40, 10)
LAVA_LT = (255, 120, 30)
LAVA_CRACK = (255, 230, 120)


# ── 그리기 도구 (게임 px · 정수) ───────────────────────────────────────────────────────────────────────────────
class Scene:
    def __init__(self, bg, w=W, h=H):
        self.w, self.h = w, h
        self.im = bg.copy().convert("RGBA")

    # 한 겹: fn(draw, layer) 가 RGBA 레이어에 그리고 알파 합성 (겹 안에서는 덮어쓰기 · 겹끼리는 블렌드)
    def paint(self, fn, alpha=255):
        layer = Image.new("RGBA", (self.w, self.h), (0, 0, 0, 0))
        fn(ImageDraw.Draw(layer), layer)
        if alpha != 255:
            a = layer.getchannel("A").point(lambda v: v * alpha // 255)
            layer.putalpha(a)
        self.im = Image.alpha_composite(self.im, layer)

    def mask(self, fn, color, alpha):
        m = Image.new("L", (self.w, self.h), 0)
        fn(ImageDraw.Draw(m))
        layer = Image.new("RGBA", (self.w, self.h), color + (0,))
        layer.putalpha(m.point(lambda v: v * alpha // 255))
        self.im = Image.alpha_composite(self.im, layer)

    def sprite(self, spr, x, y, alpha=255):
        if alpha != 255:
            spr = spr.copy()
            spr.putalpha(spr.getchannel("A").point(lambda v: v * alpha // 255))
        self.im.alpha_composite(spr, (int(x), int(y)))

    # ── 모양 ──
    def ellipse(self, cx, cy, rx, ry, color, alpha=255, width=0):
        def f(d):
            d.ellipse([cx - rx, cy - ry, cx + rx, cy + ry], fill=255 if width == 0 else None, outline=255 if width else None, width=width)
        self.mask(f, color, alpha)

    def crescent(self, cx, cy, rx, ry, dx, dy, t_in, t_out, color, alpha=255):
        """타원과 (−방향으로) 밀린 타원의 차 = 초승달 띠. 방향 (dx, dy) 쪽이 볼록. t_in < t_out = 띠의 안·바깥 변위."""
        def f(d):
            sx, sy = cx - dx * t_in, cy - dy * t_in
            d.ellipse([sx - rx, sy - ry, sx + rx, sy + ry], fill=255)
            sx, sy = cx - dx * t_out, cy - dy * t_out
            d.ellipse([sx - rx, sy - ry, sx + rx, sy + ry], fill=0)
        self.mask(f, color, alpha)

    def slash(self, cx, cy, rx, ry, dx, dy, thick, outer, mid, edge, alpha=255):
        """DNF 식 참격: 바깥 진한 색 → 가운데 밝은 색 → 오목한 쪽 흰 날 1 px."""
        self.crescent(cx, cy, rx, ry, dx, dy, 0, thick, outer, alpha)
        self.crescent(cx, cy, rx, ry, dx, dy, 1, thick - 2, mid, alpha)
        self.crescent(cx, cy, rx, ry, dx, dy, thick - 1, thick, edge, alpha)

    def star(self, cx, cy, n, r_in, r_out, color, alpha=255, phase=0.0, squash=1.0, seed=None):
        rnd = random.Random(seed)
        pts = []
        for i in range(n * 2):
            a = phase + math.pi * i / n
            r = r_out if i % 2 == 0 else r_in
            if seed is not None and i % 2 == 0:
                r *= 0.75 + 0.5 * rnd.random()
            pts.append((cx + math.cos(a) * r, cy + math.sin(a) * r * squash))
        self.mask(lambda d: d.polygon(pts, fill=255), color, alpha)

    def taper(self, x0, y0, x1, y1, w0, w1, color, alpha=255):
        dx, dy = x1 - x0, y1 - y0
        n = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / n, dx / n
        pts = [(x0 + nx * w0, y0 + ny * w0), (x0 - nx * w0, y0 - ny * w0), (x1 - nx * w1, y1 - ny * w1), (x1 + nx * w1, y1 + ny * w1)]
        self.mask(lambda d: d.polygon(pts, fill=255), color, alpha)

    def flame(self, x, y, h, w, lean, outer, inner, alpha=255):
        """불혀: 밑변 w · 높이 h · 끝이 lean 만큼 기운다. 바깥 색 + 안쪽 밝은 색(60 %)."""
        def tongue(hh, ww, col, a):
            pts = [(x - ww / 2, y), (x - ww / 2 - 1, y - hh * 0.35), (x + lean * 0.5, y - hh * 0.7), (x + lean, y - hh),
                   (x + lean * 0.6 + ww * 0.3, y - hh * 0.6), (x + ww / 2 + 1, y - hh * 0.3), (x + ww / 2, y)]
            self.mask(lambda d: d.polygon(pts, fill=255), col, a)
        tongue(h, w, outer, alpha)
        tongue(h * 0.6, w * 0.5, inner, alpha)

    def puff(self, cx, cy, r, color, alpha, seed=0, rim=None):
        rnd = random.Random(seed)
        blobs = []
        for i in range(4):
            ox, oy = rnd.uniform(-r * 0.6, r * 0.6), rnd.uniform(-r * 0.35, r * 0.25)
            rr = r * rnd.uniform(0.55, 0.9)
            blobs.append((cx + ox, cy + oy, rr))
        def f(d):
            for (x, y, rr) in blobs:
                d.ellipse([x - rr, y - rr * 0.8, x + rr, y + rr * 0.8], fill=255)
        if rim is not None:
            def g(d):
                for (x, y, rr) in blobs:
                    d.ellipse([x - rr - 1, y - rr * 0.8 - 1, x + rr + 1, y + rr * 0.8 + 1], fill=255)
            self.mask(g, rim, alpha)
        self.mask(f, color, alpha)

    def rocks(self, pts, color, size=2):
        def f(d, _):
            for (x, y) in pts:
                d.rectangle([x, y, x + size, y + size - 1], fill=color + (255,))
                d.point((x, y), fill=(min(color[0] + 50, 255), min(color[1] + 40, 255), min(color[2] + 30, 255), 255))
        self.paint(f)

    def bolt(self, x0, y0, x1, y1, seed, core=WHITE, glow=PURPLE_LT, alpha=255, segs=5, jitter=6):
        rnd = random.Random(seed)
        pts = [(x0, y0)]
        for i in range(1, segs):
            t = i / segs
            pts.append((x0 + (x1 - x0) * t + rnd.uniform(-jitter, jitter), y0 + (y1 - y0) * t + rnd.uniform(-2, 2)))
        pts.append((x1, y1))
        self.paint(lambda d, _: d.line(pts, fill=glow + (255,), width=4), alpha * 55 // 100)
        self.paint(lambda d, _: d.line(pts, fill=core + (255,), width=2), alpha)
        # 곁가지 하나
        bx, by = pts[len(pts) // 2]
        self.paint(lambda d, _: d.line([(bx, by), (bx + rnd.choice([-1, 1]) * 9, by + 7)], fill=glow + (255,), width=1), alpha * 70 // 100)

    def motes(self, cx, cy, n, color, seed, spread=22, lift=0, size=2):
        rnd = random.Random(seed)
        def f(d, _):
            for i in range(n):
                x = cx + rnd.uniform(-spread, spread)
                y = cy - rnd.uniform(0, 40) - lift
                d.rectangle([x, y, x + size - 1, y + size - 1], fill=color + (255,))
        self.paint(f)

    def ring(self, cx, cy, rx, ry, color, alpha=255, width=1):
        self.ellipse(cx, cy, rx, ry, color, alpha, width=width)


# ── 재료 ───────────────────────────────────────────────────────────────────────────────────────────────────
def load_demon(pose, view):
    import demon_look as dl  # ★ 2026-10-14 밤 (4): 자세 표는 demon_look 하나 (옛 이름은 LEGACY)
    k = dl.frame_of(pose, view) - dl.BODY_BASE
    im = Image.open(os.path.join(DEMON_DIR, f"demon_{k}.png"))
    pal = im.getpalette()
    rgba = Image.new("RGBA", im.size, (0, 0, 0, 0))
    px = im.load()
    out = rgba.load()
    for y in range(im.height):
        for x in range(im.width):
            i = px[x, y]
            if i != 0:
                out[x, y] = (pal[i * 3], pal[i * 3 + 1], pal[i * 3 + 2], 255)
    return rgba


def silhouette(spr, fill, edge, eyes=None):
    """검은 실루엣 + 1 px 테 (변신 중의 「그림자」 모습). eyes = [(x, y), …] 붉은 눈."""
    w, h = spr.size
    a = spr.getchannel("A")
    out = Image.new("RGBA", (w + 2, h + 2), (0, 0, 0, 0))
    o = out.load()
    src = a.load()
    on = lambda x, y: 0 <= x < w and 0 <= y < h and src[x, y] > 0
    for y in range(-1, h + 1):
        for x in range(-1, w + 1):
            if on(x, y):
                o[x + 1, y + 1] = fill + (255,)
            elif any(on(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                o[x + 1, y + 1] = edge + (255,)
    if eyes:
        for (x, y) in eyes:
            o[x + 1, y + 1] = (255, 40, 40, 255)
    return out


def demon_at(scene, pose, view, x=FX, y=FY, alpha=255):
    spr = load_demon(pose, view)
    scene.sprite(spr, x - 15, y - 34, alpha)
    return spr


def background():
    """숲 시험대의 풀 조각 둘을 타일링 — 모의 창의 가운데가 풀이어야 흙먼지 · 용암이 보인다(캡처 그대로면 모래밭 위)."""
    im = Image.open(BG_SRC).convert("RGB")
    a = im.crop((952, 152, 984, 184))  # 순수 풀 32×32 둘 (초록 우세 96 %)
    b = im.crop((1104, 32, 1136, 64))
    bg = Image.new("RGB", (W, H))
    from PIL import ImageOps
    for ty in range(0, H, 32):
        for tx in range(0, W, 32):
            t = a if (tx // 32 + ty // 32) % 2 == 0 else b
            if (tx // 32) % 2:
                t = ImageOps.mirror(t)  # 거울 타일링 — 이음매를 숨긴다
            if (ty // 32) % 2:
                t = ImageOps.flip(t)
            bg.paste(t, (tx, ty))
    sand = im.crop((96, 262, 200, 350)).convert("RGBA")  # 모래밭 한 조각 — 타원 마스크로 귀퉁이에
    m = Image.new("L", sand.size, 0)
    ImageDraw.Draw(m).ellipse([4, 6, sand.width - 4, sand.height - 6], fill=255)
    sand.putalpha(m)
    bg.paste(sand, (W - 96, H - 70), sand)
    return bg


def key_green(rgba):
    """초록이 우세한 픽셀(풀)을 투명으로 — 캡처에서 오려 낸 조각의 배경을 떼어 낸다."""
    out = rgba.copy()
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = px[x, y]
            if g > r * 1.12 and g > b * 1.12 and g > 40:
                px[x, y] = (0, 0, 0, 0)
    return out


def monster():
    im = Image.open(BG_SRC).convert("RGB")
    return key_green(im.crop((164, 161, 190, 196)).convert("RGBA"))


def human():
    im = Image.open(os.path.join(ROOT, "docs", "skill", "demon_look_2026-10-13.png")).convert("RGB")
    return key_green(im.crop((28, 95, 64, 147)).convert("RGBA"))


# ── 효과 ───────────────────────────────────────────────────────────────────────────────────────────────────
def fx_transform(bg):
    """start1~6: 실루엣 + 자홍 날개 + 붉은 기운 + 조각 → 섬광 → 악마 + 흙먼지 고리 (24 틱 = 1.2 s 중 8 장)."""
    frames = []
    stand = load_demon("stand", FRONT)
    sil = silhouette(stand, BLACK, MAGENTA)
    sil_eyes = silhouette(stand, BLACK, MAGENTA, eyes=[(11, 7), (17, 7)])

    def wings(s, t, rx):
        # 발치에서 양옆으로 퍼지는 톱니 불꽃 날개 (start2~4 의 자홍 톱니)
        def f(d, _):
            rnd = random.Random(t)
            for sgn in (-1, 1):
                pts = [(FX + sgn * 3, FY - 16)]
                n = 7
                for i in range(n):
                    u = (i + 0.5) / n
                    tall = (16 * (1.0 - u * 0.7) + rnd.uniform(0, 6)) * (1 if i % 2 else 0.55)
                    pts.append((FX + sgn * (3 + rx * u), FY - 1 - tall))
                    pts.append((FX + sgn * (3 + rx * (u + 0.5 / n)), FY - 1 - tall * 0.3))
                pts.append((FX + sgn * (3 + rx), FY + 1))
                pts.append((FX + sgn * 3, FY + 3))
                d.polygon(pts, fill=(70, 10, 70, 255))  # 검은 기운 덩어리
                inner = [(x + (FX - x) * 0.25, y + (FY - 2 - y) * 0.35) for (x, y) in pts]
                d.polygon(inner, fill=(150, 30, 140, 255))  # 속의 자홍
                d.line(pts[:-1], fill=MAGENTA + (255,), width=1)  # 톱니 테
                for (x, y) in pts[1:-2:2]:
                    d.point((x, y - 1), fill=PINK + (255,))
        s.paint(f, 210)

    def streaks(s, seed, n, length):
        rnd = random.Random(seed)
        for i in range(n):
            a = rnd.uniform(-1.25, -0.35) * math.pi / 1.0 + rnd.choice([0, 0.4])
            a = rnd.uniform(math.radians(200), math.radians(340))
            x0, y0 = FX + rnd.uniform(-6, 6), FY - rnd.uniform(10, 26)
            L = length * rnd.uniform(0.7, 1.2)
            x1, y1 = x0 + math.cos(a) * L, y0 + math.sin(a) * L
            s.taper(x0, y0, x1, y1, 1.6, 0.3, RED_HOT, 230)
            s.taper(x0, y0, x1, y1, 0.7, 0.2, (255, 210, 120), 230)

    def chunks(s, seed, n, lift):
        rnd = random.Random(seed)
        def f(d, _):
            for i in range(n):
                x = FX + rnd.uniform(-22, 22)
                y = FY - 18 - rnd.uniform(0, 26) - lift
                sz = rnd.choice([2, 3, 3, 4])
                d.rectangle([x, y, x + sz, y + sz], fill=BLACK + (255,))
                d.point((x + sz, y), fill=PINK + (255,))
        s.paint(f)

    def core(s, r, bright):
        s.ellipse(FX, FY - 20, r + 3, r + 3, MAGENTA, 90)
        s.ellipse(FX, FY - 20, r, r, PINK, 200)
        s.ellipse(FX, FY - 20, max(r - 2, 1), max(r - 2, 1), WHITE if bright else PINK, 255)

    def ground_ring(s, rx, ry, a, rock_r, seed=7):
        s.ellipse(FX, FY, rx, ry, SAND, a)
        s.ring(FX, FY, rx, ry, (160, 120, 70), a + 40)
        rnd = random.Random(seed)
        pts = [(FX + math.cos(t) * rock_r * rnd.uniform(0.8, 1.2), FY + math.sin(t) * rock_r * 0.45 * rnd.uniform(0.8, 1.2))
               for t in [i * math.pi / 3 + 0.3 for i in range(6)]]
        s.rocks(pts, ROCK, 3)

    # t0 사람
    s = Scene(bg); s.sprite(human(), FX - 18, FY - 46); frames.append(("t0 사람", s))
    # t3 허리 고리 + 어두워짐
    s = Scene(bg); s.sprite(human(), FX - 18, FY - 46); s.sprite(sil, FX - 16, FY - 35, 150)
    s.ring(FX, FY - 16, 20, 5, MAGENTA, 230, width=2); s.ring(FX, FY - 16, 18, 4, PINK, 200)
    frames.append(("t3 허리 고리", s))
    # t6 실루엣 + 날개 + 기운
    s = Scene(bg); wings(s, 6, 16); s.sprite(sil, FX - 16, FY - 35); streaks(s, 1, 2, 16); chunks(s, 1, 3, 0)
    frames.append(("t6 실루엣·날개", s))
    # t9
    s = Scene(bg); wings(s, 9, 24); s.sprite(sil, FX - 16, FY - 35); streaks(s, 2, 3, 20); chunks(s, 2, 4, 6); core(s, 2, False)
    frames.append(("t9 기운·조각", s))
    # t12 눈 · 핵
    s = Scene(bg); wings(s, 12, 30); s.sprite(sil_eyes, FX - 16, FY - 35); streaks(s, 3, 4, 24); chunks(s, 3, 5, 12); core(s, 4, True)
    frames.append(("t12 붉은 눈·핵", s))
    # t15 섬광
    s = Scene(bg); wings(s, 15, 26); s.sprite(sil_eyes, FX - 16, FY - 35); chunks(s, 4, 6, 20)
    s.ellipse(FX, FY - 18, 22, 26, PINK, 140); s.ellipse(FX, FY - 18, 12, 16, WHITE, 200); s.star(FX, FY - 18, 8, 8, 30, WHITE, 150, seed=3)
    frames.append(("t15 섬광", s))
    # t18 악마 착지 + 흙먼지 고리
    s = Scene(bg); ground_ring(s, 26, 8, 110, 20); s.ellipse(FX, FY - 1, 14, 4, RED_HOT, 120); demon_at(s, "stand", FRONT)
    s.puff(FX - 24, FY - 2, 7, SAND, 190, 1, rim=SAND_RIM); s.puff(FX + 24, FY - 2, 7, SAND, 190, 2, rim=SAND_RIM)
    frames.append(("t18 악마·흙먼지", s))
    # t22 가라앉음
    s = Scene(bg); ground_ring(s, 30, 9, 50, 28, seed=9); demon_at(s, "stand", FRONT)
    frames.append(("t22 가라앉음", s))
    return frames


def fx_claw(bg):
    frames = []
    mob = monster()
    def with_mob(s, x, y, alpha=255):
        s.sprite(mob, x, y, alpha)

    # 1단-1: 붉은 초승달 (앞쪽으로 볼록)
    s = Scene(bg); with_mob(s, FX + 18, FY - 40); demon_at(s, "claw_a", RIGHT)
    s.slash(FX + 22, FY - 20, 19, 24, 1, 0, 8, RED, (255, 120, 110), (255, 240, 240))
    s.motes(FX + 20, FY - 10, 4, BLACK, 1, spread=16)
    frames.append(("1단 ① 초승달", s))
    # 1단-2: 거꾸로 + 앞 것 잔상 → X
    s = Scene(bg); with_mob(s, FX + 18, FY - 40); demon_at(s, "claw_b", RIGHT)
    s.slash(FX + 22, FY - 20, 19, 24, 1, 0, 8, RED, (255, 120, 110), (255, 240, 240), alpha=90)
    s.slash(FX + 26, FY - 20, 19, 24, -1, 0, 8, RED, (255, 120, 110), (255, 240, 240))
    s.star(FX + 26, FY - 22, 6, 3, 9, WHITE, 230, seed=2)
    frames.append(("1단 ② 교차", s))
    # 2단-1: 노란 큰 호 (아래에서)
    s = Scene(bg); with_mob(s, FX + 18, FY - 40); demon_at(s, "sweep_low", RIGHT)
    s.slash(FX + 24, FY - 10, 32, 14, 0.6, 0.8, 8, ORANGE, AMBER, YELLOW_PALE)
    frames.append(("2단 ① 큰 호", s))
    # 2단-2: 위로 쓸어올림 (잔상 + 불똥)
    s = Scene(bg); with_mob(s, FX + 18, FY - 40); demon_at(s, "sweep_high", RIGHT)
    s.slash(FX + 24, FY - 10, 32, 14, 0.6, 0.8, 8, ORANGE, AMBER, YELLOW_PALE, alpha=80)
    s.slash(FX + 26, FY - 32, 36, 16, 0.7, -0.7, 9, ORANGE, AMBER, YELLOW_PALE)
    s.motes(FX + 30, FY - 24, 5, AMBER, 4, spread=16, size=2)
    s.star(FX + 26, FY - 26, 6, 2, 8, WHITE, 200, seed=5)
    frames.append(("2단 ② 쓸어올림", s))
    # 3단-1: 세 줄 긁어내리기 (위에서)
    s = Scene(bg); with_mob(s, FX + 18, FY - 40); with_mob(s, FX + 40, FY - 26); demon_at(s, "slam_up", RIGHT)
    for i in range(3):
        x = FX + 12 + i * 12
        s.taper(x + 6, FY - 56, x, FY - 22, 2.2, 0.8, ORANGE, 230)
        s.taper(x + 6, FY - 56, x, FY - 22, 1.2, 0.4, YELLOW_PALE, 240)
    frames.append(("3단 ① 세 줄", s))
    # 3단-2: 바닥 도달 + 흙먼지 + 돌 + 섬광
    s = Scene(bg); with_mob(s, FX + 18, FY - 40); with_mob(s, FX + 40, FY - 26); demon_at(s, "slam_down", RIGHT)
    for i in range(3):
        x = FX + 12 + i * 12
        s.taper(x + 7, FY - 50, x, FY + 2, 2.6, 1.0, ORANGE, 200)
        s.taper(x + 7, FY - 50, x, FY + 2, 1.4, 0.5, YELLOW_PALE, 220)
    for i, (dx, r) in enumerate(((8, 9), (24, 11), (40, 9))):
        s.puff(FX + dx, FY + 1, r, SAND, 190, i + 10, rim=SAND_RIM)
    s.rocks([(FX + 6, FY - 6), (FX + 30, FY - 9), (FX + 46, FY - 3), (FX + 18, FY + 4)], ROCK, 2)
    s.star(FX + 26, FY - 8, 8, 3, 12, WHITE, 220, seed=8, squash=0.6)
    frames.append(("3단 ② 흙먼지", s))
    return frames


def fx_charge(bg):
    frames = []
    mob = monster()
    stand = load_demon("stand", RIGHT)
    ghost = silhouette(stand, (40, 20, 60), (90, 40, 120))

    def mane(s, seed, n, L):
        """머리를 감싼 붉은 불꽃이 뒤로 흐른다(204355) — 머리 위 불덩이 + 뒤로 눕힌 불혀 n 개."""
        rnd = random.Random(seed)
        hx, hy = FX - 1, FY - 30
        s.ellipse(hx, hy, 8, 7, RED_HOT, 200); s.ellipse(hx - 1, hy - 1, 5, 4, ORANGE, 230)
        for i in range(n):
            y0 = hy + rnd.uniform(-5, 5)
            pts = [(hx - 2, y0 - 4), (hx - 2, y0 + 4), (hx - L * 0.5, y0 + 2 + rnd.uniform(-3, 3)),
                   (hx - L * rnd.uniform(0.9, 1.3), y0 - 3 + rnd.uniform(-4, 4)), (hx - L * 0.55, y0 - 3 + rnd.uniform(-3, 2))]
            s.mask(lambda d: d.polygon(pts, fill=255), RED_HOT, 210)
            pts2 = [(hx - 2, y0 - 2), (hx - 2, y0 + 2), (hx - L * 0.4, y0 + 1), (hx - L * 0.7, y0 - 1), (hx - L * 0.35, y0 - 2)]
            s.mask(lambda d: d.polygon(pts2, fill=255), (255, 190, 80), 220)

    def dust(s, n, seed):
        for i in range(n):
            s.puff(FX - 14 - i * 11, FY - 1 + (i % 2) * 2, 6 + i * 2, SAND, 200 - i * 40, seed + i, rim=SAND_RIM)

    def ghosts(s, n):
        for i in range(n):
            s.sprite(ghost, FX - 16 - 9 * (i + 1), FY - 35, 90 - i * 40)

    s = Scene(bg); ghosts(s, 1); dust(s, 1, 1); demon_at(s, "walk0", RIGHT); mane(s, 1, 2, 16)
    frames.append(("t2 불꽃 갈기", s))
    s = Scene(bg); ghosts(s, 2); dust(s, 2, 2); s.rocks([(FX - 24, FY - 6), (FX - 34, FY + 1)], ROCK, 2); demon_at(s, "walk1", RIGHT); mane(s, 2, 3, 22)
    frames.append(("t4 흙먼지·돌", s))
    s = Scene(bg); ghosts(s, 2); dust(s, 3, 3); s.rocks([(FX - 28, FY - 8), (FX - 40, FY), (FX - 20, FY + 3)], ROCK, 2)
    s.sprite(mob, FX + 16, FY - 40); demon_at(s, "walk2", RIGHT); mane(s, 3, 3, 24)
    s.star(FX + 30, FY - 22, 8, 4, 14, YELLOW_PALE, 230, seed=4); s.star(FX + 30, FY - 22, 8, 2, 6, WHITE, 255, seed=4)
    frames.append(("t6 닿음 섬광", s))
    s = Scene(bg); ghosts(s, 2); dust(s, 3, 5); s.sprite(mob, FX + 32, FY - 38); demon_at(s, "walk3", RIGHT); mane(s, 4, 3, 24)
    s.taper(FX + 24, FY - 20, FX + 44, FY - 20, 1.5, 0.3, WHITE, 150); s.taper(FX + 22, FY - 14, FX + 40, FY - 14, 1.2, 0.3, WHITE, 120)
    s.star(FX + 34, FY - 22, 8, 3, 9, YELLOW_PALE, 120, seed=4)
    frames.append(("t8 한 칸 밀림", s))
    return frames


def fx_follow(bg):
    frames = []
    mob = monster()

    def aura_ground(s, a):
        s.ellipse(FX, FY, 16, 5, PURPLE, a)

    s = Scene(bg); aura_ground(s, 50); demon_at(s, "stand", RIGHT); s.motes(FX, FY, 3, PURPLE_LT, 1)
    frames.append(("게이지 25 %", s))
    s = Scene(bg); aura_ground(s, 80); demon_at(s, "stand", RIGHT); s.motes(FX, FY, 6, PURPLE_LT, 2)
    s.flame(FX - 10, FY, 7, 4, -2, MAGENTA, PINK, 200); s.flame(FX + 10, FY, 6, 4, 2, MAGENTA, PINK, 200)
    frames.append(("게이지 60 %", s))
    s = Scene(bg); aura_ground(s, 110)
    s.ellipse(FX, FY - 18, 24, 28, PURPLE, 60); demon_at(s, "stand", RIGHT)
    s.ring(FX, FY - 18, 24, 28, PURPLE_LT, 190); s.ring(FX, FY - 18, 22, 26, (240, 200, 255), 70)
    for (x, h, lean) in ((FX - 12, 11, -3), (FX - 4, 8, -1), (FX + 6, 12, 2), (FX + 13, 8, 3)):
        s.flame(x, FY + 1, h, 4, lean, MAGENTA, PINK, 210)
    s.motes(FX, FY, 8, PINK, 3, spread=20)
    frames.append(("100 % 보라 반구", s))
    # 끝 = 어둠 폭발
    s = Scene(bg); s.sprite(mob, FX + 34, FY - 44); s.sprite(mob, FX - 52, FY - 20); demon_at(s, "roar", RIGHT)
    for (dx, dy, r, sd) in ((-10, -14, 12, 1), (12, -22, 14, 2), (0, -32, 10, 3), (6, -6, 11, 4)):
        s.puff(FX + dx, FY + dy, r, SMOKE, 200, sd)
    s.star(FX, FY - 20, 8, 7, 20, YELLOW_PALE, 230, seed=11); s.star(FX, FY - 20, 8, 3, 10, WHITE, 255, seed=11)
    frames.append(("끝 ① 검은 연기·섬광", s))
    s = Scene(bg); s.sprite(mob, FX + 34, FY - 44); s.sprite(mob, FX - 52, FY - 20)
    s.star(FX, FY - 18, 12, 10, 46, PURPLE_DK, 230, seed=12, squash=0.8); s.star(FX, FY - 18, 12, 9, 44, (110, 40, 170), 140, seed=12, squash=0.8)
    for (dx, dy, r, sd) in ((-16, -14, 20, 5), (16, -26, 22, 6), (0, -40, 16, 7), (8, -4, 18, 8)):
        s.puff(FX + dx, FY + dy, r, SMOKE, 170, sd)
    demon_at(s, "roar", RIGHT)
    s.ring(FX, FY - 14, 60, 40, PURPLE_LT, 150, width=2); s.ellipse(FX, FY - 14, 60, 40, PURPLE, 40)
    s.star(FX + 48, FY - 30, 6, 3, 9, PURPLE_LT, 230, seed=13); s.star(FX - 38, FY - 6, 6, 3, 9, PURPLE_LT, 230, seed=14)
    frames.append(("끝 ② 어둠 가시·반구", s))
    s = Scene(bg); s.sprite(mob, FX + 34, FY - 44); s.sprite(mob, FX - 52, FY - 20)
    for (dx, dy, r, sd) in ((-20, -30, 22, 9), (18, -44, 24, 10), (0, -58, 18, 11)):
        s.puff(FX + dx, FY + dy, r, (70, 50, 90), 90, sd)
    demon_at(s, "stand", RIGHT)
    s.ring(FX, FY - 14, 96, 64, PURPLE_LT, 90, width=2); s.ellipse(FX, FY - 14, 96, 64, PURPLE, 25)
    s.motes(FX, FY - 20, 6, PURPLE_LT, 15, spread=50)
    frames.append(("끝 ③ 반구 확산", s))
    return frames


def fx_blade(bg):
    frames = []
    mob = monster()
    mobs = [(FX + 28, FY - 20), (FX - 40, FY + 6)]

    def sigil(s, phase):
        rx, ry = 56, 32  # ±3 칸 = 7 칸 = 112 px · 9/16 눕힘
        s.ellipse(FX, FY, rx, ry, (110, 0, 16), 80)
        s.ring(FX, FY, rx, ry, (210, 40, 40), 160)
        s.ring(FX, FY, rx - 6, ry - 4, (170, 20, 30), 90)
        # 회전하는 검은 소용돌이 셋 (호 세 개)
        def f(d, _):
            for i in range(3):
                a0 = phase + i * 120
                d.arc([FX - 40, FY - 23, FX + 40, FY + 23], a0, a0 + 70, fill=(8, 4, 12, 255), width=3)
                d.arc([FX - 24, FY - 14, FX + 24, FY + 14], -a0 + 30, -a0 + 90, fill=(8, 4, 12, 255), width=2)
        s.paint(f, 160)
        # 테의 룬 자국
        def g(d, _):
            for i in range(8):
                a = math.radians(i * 45 + phase * 0.5)
                x, y = FX + math.cos(a) * (rx - 2), FY + math.sin(a) * (ry - 1)
                d.rectangle([x - 1, y - 1, x + 1, y], fill=(240, 90, 80, 255))
        s.paint(g)

    def spike(s, x, y, h, w=6):
        pts = [(x - w / 2, y), (x - w / 6, y - h * 0.55), (x + 1, y - h), (x + w / 3, y - h * 0.5), (x + w / 2, y)]
        s.mask(lambda d: d.polygon(pts, fill=255), PURPLE_DK, 255)
        pts2 = [(x - w / 4, y), (x - 0.5, y - h * 0.6), (x + 0.5, y - h + 1), (x + w / 6, y - h * 0.5), (x + w / 4, y)]
        s.mask(lambda d: d.polygon(pts2, fill=255), (160, 80, 230), 255)
        s.paint(lambda d, _: d.line([(x + 1, y - h), (x + w / 3, y - h * 0.5)], fill=PURPLE_LT + (255,), width=1))

    def tuft(s, x, y, h):
        s.flame(x, y + 1, h, 4, 1, ORANGE, AMBER, 220)

    def mobs_at(s):
        for (x, y) in mobs:
            s.sprite(mob, x - 15, y - 40)

    s = Scene(bg); sigil(s, 0); mobs_at(s); demon_at(s, "stand", RIGHT)
    for (x, y) in mobs:
        spike(s, x - 4, y + 2, 9, 8); spike(s, x + 6, y + 3, 6, 6)
    frames.append(("t0 인장·솟음", s))
    s = Scene(bg); sigil(s, 40); mobs_at(s); demon_at(s, "stand", RIGHT)
    for (x, y) in mobs:
        tuft(s, x - 4, y + 2, 7); tuft(s, x + 7, y + 3, 5); spike(s, x - 4, y + 2, 24, 10); spike(s, x + 7, y + 3, 16, 7); spike(s, x - 13, y + 1, 13, 6)
        s.star(x, y - 20, 6, 3, 9, PURPLE_LT, 230, seed=21)
    frames.append(("t2 칼날·불씨", s))
    s = Scene(bg); sigil(s, 80); mobs_at(s); demon_at(s, "stand", RIGHT)
    for (x, y) in mobs:
        tuft(s, x - 4, y + 2, 11); tuft(s, x + 7, y + 3, 8); spike(s, x - 4, y + 2, 11, 10); spike(s, x + 7, y + 3, 7, 7)
    frames.append(("t4 가라앉음", s))
    return frames


def fx_judge(bg):
    frames = []
    mob = monster()
    mobs = [(FX + 34, FY - 14), (FX - 44, FY + 10)]

    def zone(s, a):
        s.ring(FX, FY, 72, 40, PURPLE_LT, a, width=1)  # ±4 칸 = 9 칸 = 144 px
        def f(d, _):
            for i in range(12):
                t = math.radians(i * 30)
                d.point((FX + math.cos(t) * 70, FY + math.sin(t) * 39), fill=(240, 200, 255, 255))
        s.paint(f, a)

    def orb(s, r, spokes, phase):
        cx, cy = FX + 2, FY - 48
        s.ellipse(cx, cy, r + 4, r + 4, PURPLE, 70)
        s.ellipse(cx, cy, r, r, PURPLE_LT, 230)
        s.ellipse(cx, cy, max(r - 3, 1), max(r - 3, 1), (245, 225, 255), 255)
        def f(d, _):
            for i in range(spokes):
                t = phase + i * 2 * math.pi / spokes
                d.line([(cx + math.cos(t) * (r + 1), cy + math.sin(t) * (r + 1)), (cx + math.cos(t) * (r + 7), cy + math.sin(t) * (r + 7))], fill=PURPLE_LT + (255,), width=1)
        s.paint(f, 200)

    def mobs_at(s):
        for (x, y) in mobs:
            s.sprite(mob, x - 15, y - 40)

    s = Scene(bg); zone(s, 70); mobs_at(s); demon_at(s, "slam_up", RIGHT); orb(s, 3, 4, 0.0)
    frames.append(("선딜 t5 구체", s))
    s = Scene(bg); zone(s, 110); mobs_at(s); demon_at(s, "slam_up", RIGHT); orb(s, 7, 8, 0.4); s.motes(FX, FY - 30, 4, PURPLE_LT, 31, spread=14)
    frames.append(("선딜 t15 커진다", s))
    s = Scene(bg); zone(s, 110); mobs_at(s); demon_at(s, "stand", RIGHT)
    (x, y) = mobs[0]
    s.bolt(x + 12, 0, x, y - 20, 3)
    s.star(x, y - 18, 8, 5, 17, PURPLE_LT, 230, seed=32); s.star(x, y - 18, 8, 3, 9, (240, 215, 255), 255, seed=32); s.ellipse(x, y - 18, 3, 3, WHITE, 255)
    s.ellipse(x, y, 10, 4, PURPLE, 110)
    (x2, y2) = mobs[1]
    s.bolt(x2 - 8, 0, x2, y2 - 20, 5, alpha=120); s.ellipse(x2, y2, 10, 4, PURPLE, 70)
    frames.append(("판정 t20 번개", s))
    s = Scene(bg); zone(s, 110); mobs_at(s); demon_at(s, "stand", RIGHT)
    (x, y) = mobs[0]
    s.paint(lambda d, _: d.line([(x + 10, 0), (x + 4, y - 40), (x, y - 20)], fill=PURPLE_LT + (255,), width=1), 90)
    s.star(x, y - 18, 8, 4, 16, PURPLE_LT, 110, seed=33); s.motes(x, y - 10, 4, PURPLE_LT, 34, spread=10, size=1)
    s.ellipse(x, y, 10, 4, PURPLE, 60)
    frames.append(("t22 잔광", s))
    return frames


def fx_fire(bg):
    frames = []
    mob = monster()
    mobs = [(FX + 46, FY - 30), (FX - 50, FY + 4), (FX + 30, FY + 22)]

    def mobs_at(s, hit=False):
        for (x, y) in mobs:
            s.sprite(mob, x - 15, y - 40)
            if hit:
                s.star(x, y - 18, 6, 3, 9, WHITE, 230, seed=x)

    def core(s, r, ring_r):
        s.ellipse(FX + 1, FY - 20, r, r, RED_HOT, 255); s.ellipse(FX + 1, FY - 20, max(r - 2, 1), max(r - 2, 1), (255, 200, 120), 255)
        s.ring(FX + 1, FY - 20, ring_r, ring_r * 0.8, ORANGE, 160)
        s.ellipse(FX + 1, FY - 20, ring_r, ring_r * 0.8, RED_HOT, 40)

    s = Scene(bg); mobs_at(s); demon_at(s, "roar", RIGHT); core(s, 3, 10)
    frames.append(("선딜 t2 붉은 핵", s))
    s = Scene(bg); mobs_at(s); s.ellipse(FX, FY, 16, 5, RED_HOT, 90); demon_at(s, "roar", RIGHT); core(s, 5, 17); core(s, 5, 9)
    for i in range(3):
        s.taper(FX - 6 + i * 6, FY - 16, FX - 10 + i * 8, FY - 34 - i * 3, 1.2, 0.3, RED_HOT, 200)
    frames.append(("선딜 t8 고리", s))
    s = Scene(bg); mobs_at(s); demon_at(s, "roar", RIGHT)
    s.ellipse(FX, FY - 20, 24, 24, (255, 250, 230), 220); s.ring(FX, FY - 20, 32, 30, RED_HOT, 200, width=2)
    frames.append(("t10 섬광", s))

    def fireball(s, R, tongues, seed, smoke_n, base_rx):
        cy = FY - 16
        s.ellipse(FX, FY, base_rx, base_rx * 0.3, ORANGE, 90)
        def blob(r, col, a, sd, ry=0.92):
            rnd2 = random.Random(sd)
            def f(d):
                d.ellipse([FX - r, cy - r * ry, FX + r, cy + r * ry], fill=255)
                for i in range(7):
                    t = rnd2.uniform(0, math.pi * 2)
                    rr = r * rnd2.uniform(0.22, 0.4)
                    x, y = FX + math.cos(t) * r * 0.9, cy + math.sin(t) * r * ry * 0.9
                    d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=255)
            s.mask(f, col, a)
        blob(R, (120, 20, 10), 180, seed + 1)
        blob(R * 0.76, RED, 235, seed + 2)
        blob(R * 0.52, ORANGE, 255, seed + 3)
        blob(R * 0.26, (255, 230, 120), 255, seed + 4)
        rnd = random.Random(seed)
        for i in range(tongues):
            x = FX + rnd.uniform(-R * 0.7, R * 0.7)
            h = rnd.uniform(R * 0.6, R * 1.1)
            s.flame(x, cy - R * 0.4, h, rnd.uniform(6, 12), rnd.uniform(-6, 6), RED, ORANGE, 230)
        for i in range(smoke_n):
            t = rnd.uniform(0, math.pi * 2)
            s.puff(FX + math.cos(t) * R * 1.05, cy + math.sin(t) * R * 0.9, rnd.uniform(4, 8), (40, 20, 20), 200, seed * 10 + i)

    s = Scene(bg); mobs_at(s, hit=True); fireball(s, 40, 5, 1, 6, 60); demon_at(s, "roar", RIGHT, alpha=160)
    frames.append(("t12 불덩이", s))
    s = Scene(bg); mobs_at(s, hit=True); fireball(s, 58, 7, 2, 9, 84)
    rnd = random.Random(5)
    s.rocks([(FX + rnd.uniform(-70, 70), FY - 30 - rnd.uniform(0, 60)) for _ in range(8)], (40, 20, 20), 3)
    frames.append(("t16 치솟음", s))
    s = Scene(bg); mobs_at(s); demon_at(s, "stand", RIGHT)
    for (dx, dy, r, sd) in ((-30, -40, 24, 1), (26, -56, 26, 2), (0, -76, 20, 3), (-8, -26, 18, 4)):
        s.puff(FX + dx, FY + dy, r, (70, 35, 35), 110, sd)
    s.motes(FX, FY - 40, 8, (255, 200, 80), 41, spread=60, size=1)
    frames.append(("t22 연기", s))
    return frames


def fx_lava(bg):
    frames = []

    def lava_cell(s, cx, cy, age, seed):
        """age 0..1 (5 초). 웅덩이 + 불혀 + 불씨 — 식을수록 어둡고 낮아진다."""
        rnd = random.Random(seed)
        a = int(200 - 130 * age)
        s.ellipse(cx, cy + 3, 9, 5, (120, 15, 5), a)  # 식은 테 (칸을 거의 채운다 — 이웃과 이어져 「불타는 땅」)
        s.ellipse(cx, cy + 3, 7, 3, LAVA, a)
        s.ellipse(cx + 1, cy + 3, 4, 2, (245, 80, 20), a)
        for i in range(4):
            x, y = cx + rnd.uniform(-6, 6), cy + 2 + rnd.uniform(-3, 3)
            s.paint(lambda d, _: d.rectangle([x, y, x + 1, y + 1], fill=LAVA_LT + (255,)), a)
        s.paint(lambda d, _: d.line([(cx - 5, cy + 1), (cx - 1, cy + 4), (cx + 2, cy + 2), (cx + 6, cy + 4)], fill=LAVA_CRACK + (255,), width=1), int(a * 0.9))
        n = 2 if age < 0.5 else 1
        for i in range(n):
            x = cx + rnd.uniform(-5, 5)
            h = (4 + rnd.uniform(0, 5)) * (1.0 - 0.55 * age)
            s.flame(x, cy + 3, h, 2 + rnd.uniform(0, 1), rnd.uniform(-2, 2), LAVA_LT, AMBER, int(200 - 120 * age))
        for i in range(3 if age < 0.7 else 1):
            x, y = cx + rnd.uniform(-7, 7), cy - 6 - rnd.uniform(0, 14)
            s.paint(lambda d, _: d.point((x, y), fill=(255, 220, 100, 255)), int(230 - 150 * age))

    # 지옥불 링 = 캐릭터 둘레 반지름 2 칸의 마름모 12 칸 (실제 칸은 링 스포너의 셈 — 여기선 모양만)
    ring = []  # 5×5 둘레에서 모서리를 뺀 12 칸 (지옥불 링 12 — 실제 칸은 링 스포너의 셈)
    for dx in range(-2, 3):
        for dy in range(-2, 3):
            if max(abs(dx), abs(dy)) == 2 and abs(dx) + abs(dy) < 4:
                ring.append((dx, dy))
    for (label, age) in (("t0 지옥불 → 용암", 0.0), ("t50 (2.5 s)", 0.5), ("t95 식는다", 0.95)):
        s = Scene(bg)
        for i, (dx, dy) in enumerate(ring):
            lava_cell(s, FX + dx * TILE, FY - 8 + dy * TILE, age, i + int(age * 100))
        demon_at(s, "stand", RIGHT)
        frames.append((label, s))
    # 한 칸 4 단계 흔들림 (같은 칸의 불혀가 틱마다 다르게)
    for k in range(2):
        s = Scene(bg)
        for i, (dx, dy) in enumerate(ring):
            lava_cell(s, FX + dx * TILE, FY - 8 + dy * TILE, 0.1, i * 7 + k + 100)
        demon_at(s, "stand", RIGHT)
        frames.append((f"흔들림 {k + 1}", s))
    return frames


FX_FNS = {
    "transform": fx_transform, "claw": fx_claw, "charge": fx_charge, "follow": fx_follow,
    "blade": fx_blade, "judge": fx_judge, "fire": fx_fire, "lava": fx_lava,
}
ORDER = ["transform", "claw", "charge", "follow", "blade", "judge", "fire", "lava"]


# ── 패널 조립 ──────────────────────────────────────────────────────────────────────────────────────────────
def panel(name, frames, refs_dir, zoom):
    font = ImageFont.truetype(FONT, 16)
    small = ImageFont.truetype(FONT, 16)
    gap = 4
    fw, fh = W * zoom, H * zoom
    head = 24
    label_h = 20
    strip_w = len(frames) * (fw + gap) + gap
    # 참고 썸네일 — 프레임 높이에 맞춰 세로로 두 줄 (최대 4 장)
    refs = []
    for f in REFS.get(name, []):
        p = os.path.join(refs_dir, f)
        if os.path.exists(p):
            im = Image.open(p).convert("RGB")
            refs.append((f, im))
    ref_h = (fh + label_h) // 2 - gap
    ref_tiles = []
    for f, im in refs:
        im = im.copy(); im.thumbnail((ref_h * 2, ref_h))
        ref_tiles.append((f, im))
    ref_w = max([t.width for _, t in ref_tiles] + [0])
    cols = (len(ref_tiles) + 1) // 2
    ref_block_w = (ref_w + gap) * cols if ref_tiles else 0
    total_w = strip_w + ref_block_w + gap * 2
    total_h = head + label_h + fh + gap * 2
    out = Image.new("RGB", (total_w, total_h), (28, 28, 32))
    d = ImageDraw.Draw(out)
    d.text((gap, 4), TITLES[name], fill=(255, 230, 120), font=font)
    x = gap
    for (label, scene) in frames:
        d.text((x, head), label, fill=(220, 220, 220), font=small)
        out.paste(scene.im.convert("RGB").resize((fw, fh), Image.NEAREST), (x, head + label_h))
        x += fw + gap
    x += gap
    for i, (f, t) in enumerate(ref_tiles):
        cx = x + (i // 2) * (ref_w + gap)
        cy = head + label_h + (i % 2) * (ref_h + gap)
        out.paste(t, (cx, cy))
        d.text((cx + 2, cy + 2), "참고 " + f.replace("Screenshot_20261013_", "").replace("_YouTube.jpg", "").replace(".jpg", ""), fill=(255, 255, 0), font=small)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(ROOT, "docs", "skill", "demon_fx_draft"))
    ap.add_argument("--zoom", type=int, default=3)
    ap.add_argument("--refs", default=os.path.join(ROOT, "dist", "악의 이펙트"))
    ap.add_argument("--only", default="")
    ap.add_argument("--split", action="store_true", help="프레임마다 한 장씩도 쓴다 (<out>/<skill>_<i>.png, zoom 4)")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    bg = background()
    panels = []
    for name in ORDER:
        if args.only and name not in args.only.split(","):
            continue
        frames = FX_FNS[name](bg)
        p = panel(name, frames, args.refs, args.zoom)
        p.save(os.path.join(args.out, f"{name}.png"))
        if args.split:
            for i, (label, sc) in enumerate(frames):
                sc.im.convert("RGB").resize((W * 4, H * 4), Image.NEAREST).save(os.path.join(args.out, f"{name}_{i}.png"))
        panels.append(p)
        print(f"{name}: {len(frames)} frames -> {name}.png ({p.width}x{p.height})")
    if not args.only:
        tw = max(p.width for p in panels)
        th = sum(p.height + 8 for p in panels)
        sheet = Image.new("RGB", (tw, th), (20, 20, 24))
        y = 0
        for p in panels:
            sheet.paste(p, (0, y)); y += p.height + 8
        sheet.save(os.path.join(args.out, "sheet.png"))
        print(f"sheet.png {sheet.width}x{sheet.height}")


if __name__ == "__main__":
    main()
