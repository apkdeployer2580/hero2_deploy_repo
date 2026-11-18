#!/usr/bin/env python3
"""월드맵 바탕 그림 — 처음부터 도트로 그린다 (2026-10-20, 사용자 결정 A: *"배경디자인 자체를 아예 수정하라고"*).

hero5 의 월드맵(480×320 도트 섬 그림)처럼 **한 스타일의 픽셀아트**로 바다 · 대륙 · 모래 해안 · 풀 · 숲 · 산 · 눈 ·
강 · 길 · 지형지물을 전부 절차적으로 그린다. 자리는 `worldmap_view.rs` 의 `WM_PLACES`(천분율)와 같은 표(`PLACES`)
— 둘 중 하나를 옮기면 다른 쪽도. 길은 `WM_ROADS` 와 같은 쌍(`ROADS`)을 흙길로 그려서 팝업의 선과 그림의 길이
겹친다. 옛 AI 그림은 `docs/worldmap_base_ai.png` 에 참고로 남겨 둔다(쓰지 않는다).

    python tools_re/worldmap_draw.py                  # → docs/worldmap_base.png (1280×720)
    python tools_re/worldmap_draw.py --marks out.png  # + 장소 십자 표시
그 뒤 `python tools_re/worldmap_upscale.py` 가 xBR 2× 로 `worldmap.rgb565`(2560×1440) 를 만든다.
"""

import math
import os
import random
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "docs", "worldmap_base.png")
W, H = 1280, 720

PLACES = {
    "윈터하츠": (480, 520), "윈터하츠 남부1": (480, 620), "노르도스트 평원": (660, 550), "노르덴항": (860, 510),
    "카디프숲 북부": (480, 820), "카디프숲 남부": (460, 940), "실바나르 남부": (250, 800), "납골당": (145, 815),
    "실바나르": (250, 620), "서리골짜기 남부": (250, 470), "어둠동굴": (315, 480), "서리골짜기 북부": (200, 330),
    "서리골짜기": (110, 290), "예언자의 무덤": (120, 160), "멀락협곡": (710, 300), "신드라프": (880, 170),
    "협곡지하": (786, 317), "노르도스트 북부": (720, 440),
    "버려진 섬": (950, 805), "신전": (480, 320), "신전지하": (545, 270), "리이드 지하": (400, 440), "언더시티": (560, 440),
    "윈터하츠 남부2": (480, 720), "도적굴": (600, 740), "노르덴 남부": (800, 710), "해저터널": (880, 680),
    "지하연구소": (965, 925), "반란군 캠프": (380, 840),
}
ROADS = [
    ("윈터하츠", "윈터하츠 남부1"), ("윈터하츠", "신전"), ("윈터하츠", "리이드 지하"), ("윈터하츠", "언더시티"),
    ("윈터하츠 남부1", "노르도스트 평원"), ("윈터하츠 남부1", "윈터하츠 남부2"), ("노르도스트 평원", "노르도스트 북부"),
    ("노르도스트 북부", "멀락협곡"), ("노르도스트 북부", "협곡지하"), ("멀락협곡", "협곡지하"),
    ("노르도스트 평원", "도적굴"), ("노르도스트 평원", "노르덴 남부"), ("노르덴항", "노르덴 남부"),
    ("카디프숲 북부", "카디프숲 남부"), ("카디프숲 북부", "윈터하츠 남부2"), ("카디프숲 남부", "실바나르 남부"),
    ("실바나르 남부", "납골당"), ("실바나르 남부", "실바나르"), ("실바나르 남부", "반란군 캠프"),
    ("실바나르", "서리골짜기 남부"), ("서리골짜기 남부", "어둠동굴"), ("서리골짜기 남부", "서리골짜기 북부"),
    ("서리골짜기 북부", "서리골짜기"), ("서리골짜기", "예언자의 무덤"), ("멀락협곡", "신드라프"), ("멀락협곡", "언더시티"),
    ("버려진 섬", "해저터널"), ("버려진 섬", "지하연구소"), ("신전", "신전지하"), ("윈터하츠 남부2", "도적굴"),
    ("노르덴 남부", "해저터널"),
]
# 성 안끼리 · 바다를 건너는 길은 흙길로 안 그린다
INSIDE_CASTLE = {"윈터하츠", "신전", "신전지하", "리이드 지하", "언더시티"}
SEA_ROADS = {("버려진 섬", "해저터널"), ("버려진 섬", "지하연구소")}


def px(name):
    x, y = PLACES[name]
    return x * W // 1000, y * H // 1000


# ── 색 ─────────────────────────────────────────────────────────────────────────
SEA_DEEP = (22, 62, 140)
SEA = (34, 88, 176)
SEA_LIGHT = (62, 128, 208)
FOAM = (214, 236, 250)
SAND = (222, 202, 148)
SAND_DARK = (184, 160, 108)
GRASS = (104, 160, 72)
GRASS_2 = (118, 174, 82)
GRASS_DARK = (84, 138, 60)
FOREST_FLOOR = (66, 116, 52)
PINE = (34, 92, 50)
PINE_LIGHT = (62, 128, 70)
LEAF = (58, 132, 56)
LEAF_LIGHT = (96, 168, 78)
TRUNK = (96, 68, 40)
ROCK = (132, 122, 108)
ROCK_DARK = (88, 80, 70)
ROCK_LIGHT = (170, 160, 144)
HILL = (164, 134, 86)
HILL_DARK = (124, 98, 60)
SNOW = (236, 242, 250)
SNOW_SHADE = (190, 206, 226)
ROAD = (206, 180, 122)
ROAD_EDGE = (160, 134, 84)
WALL = (168, 164, 152)
WALL_DARK = (110, 106, 98)
WALL_LIGHT = (206, 202, 190)
ROOF = (196, 72, 44)
ROOF_LIGHT = (228, 110, 70)
ROOF_DARK = (140, 48, 30)
WOOD = (150, 110, 64)
WOOD_DARK = (96, 70, 40)
WATER_LAKE = (50, 110, 190)
DARK = (18, 16, 24)


# ── 바탕: 바다 · 땅 · 지대 ─────────────────────────────────────────────────────

def noise(shape, scale, seed):
    """부드러운 값 노이즈 0..1 (작은 난수를 키워 블러)."""
    rng = np.random.default_rng(seed)
    h, w = shape
    sh, sw = max(2, h // scale), max(2, w // scale)
    small = rng.random((sh, sw)).astype(np.float32)
    im = Image.fromarray((small * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC)
    return np.asarray(im).astype(np.float32) / 255.0


def ellipse_field(cx, cy, rx, ry):
    yy, xx = np.mgrid[0:H, 0:W]
    return 1.0 - (((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2)


def land_field():
    """대륙 = 타원 덩어리의 합 − 만(灣) + 저주파 노이즈. 0 보다 크면 땅."""
    f = np.full((H, W), -1.0, np.float32)
    blobs = [
        (600, 470, 380, 320),   # 본토
        (250, 620, 130, 200),   # 서쪽 (실바나르 · 카디프숲)
        (480, 760, 230, 130),   # 남쪽
        (960, 200, 220, 160),   # 북동 산맥
        (980, 420, 130, 150),   # 동쪽 (노르도스트 · 해저터널 해안)
        (160, 250, 120, 190),   # 북서 눈 곶 (서리골짜기 · 무덤)
    ]
    for b in blobs:
        f = np.maximum(f, ellipse_field(*b))
    bays = [
        (150, 420, 130, 90),    # 실바나르 앞 만
        (1060, 640, 110, 70),   # 항구 남쪽 만
        (640, 60, 120, 70),     # 북쪽 만
    ]
    for b in bays:
        f = np.minimum(f, -ellipse_field(*b))
    f += (noise((H, W), 40, 1) - 0.5) * 0.5 + (noise((H, W), 12, 2) - 0.5) * 0.18
    # 섬 둘 (버려진 섬 · 연구소 섬)
    ix, iy = px("버려진 섬")
    f = np.maximum(f, ellipse_field(ix, iy + 2, 62, 40) + (noise((H, W), 10, 3) - 0.5) * 0.3)
    lx, ly = px("지하연구소")
    f = np.maximum(f, ellipse_field(lx, ly + 2, 26, 15))
    return f


def zone(cx, cy, rx, ry, seed, wobble=0.35):
    return ellipse_field(cx, cy, rx, ry) + (noise((H, W), 16, seed) - 0.5) * wobble > 0


def paint_base():
    """바다 · 땅 · 모래 · 풀 · 지대 색을 numpy 로 칠한 RGB 배열과 마스크들."""
    f = land_field()
    land = f > 0
    img = np.zeros((H, W, 3), np.uint8)
    # 바다: 깊이에 따라 두 파랑 + 물결 점
    n1 = noise((H, W), 6, 11)
    n2 = noise((H, W), 24, 12)
    depth = np.clip(-f, 0, 0.6) / 0.6
    sea = np.array(SEA, np.float32)[None, None] * (1 - depth[..., None]) + np.array(SEA_DEEP, np.float32)[None, None] * depth[..., None]
    sea += (n2[..., None] - 0.5) * 24
    img[...] = np.clip(sea, 0, 255).astype(np.uint8)
    waves = (n1 > 0.78) & (depth < 0.5) & ~land
    img[waves] = SEA_LIGHT
    # 땅: 풀 두 색을 노이즈로 섞고
    g = noise((H, W), 9, 13)
    grass = np.where(g[..., None] > 0.55, np.array(GRASS_2, np.uint8), np.array(GRASS, np.uint8))
    grass = np.where((g[..., None] < 0.18), np.array(GRASS_DARK, np.uint8), grass)
    img[land] = grass[land]
    # 모래 해안: 땅이면서 바다에 가까운 띠
    shore = land & ~binary_erode(land, 3)
    img[shore] = SAND
    shore_in = binary_erode(land, 3) & ~binary_erode(land, 5)
    sand_mix = noise((H, W), 5, 14) > 0.5
    img[shore_in & sand_mix] = SAND_DARK
    # 거품선: 바다이면서 땅에 가까운 한 줄
    foam = ~land & binary_dilate(land, 2)
    img[foam] = FOAM
    foam2 = ~land & binary_dilate(land, 4) & ~binary_dilate(land, 2) & (noise((H, W), 4, 15) > 0.45)
    img[foam2] = SEA_LIGHT
    return img, land


def binary_erode(m, r):
    im = Image.fromarray(m.astype(np.uint8) * 255).filter(ImageFilter.MinFilter(2 * r + 1))
    return np.asarray(im) > 127


def binary_dilate(m, r):
    im = Image.fromarray(m.astype(np.uint8) * 255).filter(ImageFilter.MaxFilter(2 * r + 1))
    return np.asarray(im) > 127


# ── 스프라이트 ─────────────────────────────────────────────────────────────────

def pine(d, x, y, s=1.0):
    """소나무: 세 단 삼각 + 줄기. (x, y) = 밑동."""
    h = int(14 * s)
    w = int(10 * s)
    d.line([(x, y), (x, y - 3)], fill=TRUNK, width=2)
    for i, (yy, ww) in enumerate(((y - 3, w), (y - 3 - h // 3, int(w * 0.8)), (y - 3 - 2 * h // 3, int(w * 0.55)))):
        top = yy - h // 2
        d.polygon([(x, top), (x - ww // 2, yy), (x + ww // 2, yy)], fill=PINE)
        d.polygon([(x, top), (x - ww // 2, yy), (x, yy)], fill=PINE_LIGHT)
    d.point((x, y - 3 - 2 * h // 3 - h // 2), fill=PINE_LIGHT)


def tree(d, x, y, s=1.0):
    """활엽수: 둥근 잎 + 줄기."""
    r = int(6 * s)
    d.line([(x, y), (x, y - r)], fill=TRUNK, width=2)
    d.ellipse((x - r, y - 2 * r - 2, x + r, y - 2), fill=LEAF)
    d.ellipse((x - r + 1, y - 2 * r - 1, x + r - 3, y - r + 1), fill=LEAF_LIGHT)
    d.ellipse((x - r - 2, y - r - 3, x + 1, y - 1), fill=LEAF)


def bare_tree(d, x, y):
    d.line([(x, y), (x, y - 9)], fill=(90, 80, 72), width=2)
    d.line([(x, y - 6), (x - 4, y - 10)], fill=(90, 80, 72), width=1)
    d.line([(x, y - 4), (x + 4, y - 9)], fill=(90, 80, 72), width=1)


def mountain(d, x, y, w, h, snow=False, brown=True):
    """산 하나: 왼쪽 밝고 오른쪽 어두운 삼각, 눈 덮인 봉우리."""
    base = HILL if brown else ROCK
    dark = HILL_DARK if brown else ROCK_DARK
    d.polygon([(x, y - h), (x - w // 2, y), (x + w // 2, y)], fill=base)
    d.polygon([(x, y - h), (x, y), (x + w // 2, y)], fill=dark)
    d.polygon([(x, y - h), (x - w // 2, y), (x - w // 6, y)], fill=ROCK_LIGHT if not brown else (190, 160, 110))
    if snow:
        sh = h // 3
        d.polygon([(x, y - h), (x - w * sh // (2 * h), y - h + sh), (x + w * sh // (2 * h), y - h + sh)], fill=SNOW)
        d.polygon([(x, y - h), (x, y - h + sh), (x + w * sh // (2 * h), y - h + sh)], fill=SNOW_SHADE)


def house(d, x, y, w=14, h=9, roof=ROOF, wall=(214, 196, 160)):
    """집: 벽 + 지붕 + 문. (x, y) = 바닥 가운데."""
    d.rectangle((x - w // 2, y - h, x + w // 2, y), fill=wall, outline=WOOD_DARK)
    d.polygon([(x - w // 2 - 2, y - h), (x, y - h - h * 2 // 3), (x + w // 2 + 2, y - h)], fill=roof, outline=ROOF_DARK)
    d.polygon([(x - w // 2 - 2, y - h), (x, y - h - h * 2 // 3), (x, y - h)], fill=ROOF_LIGHT if roof == ROOF else roof)
    d.rectangle((x - 2, y - 4, x + 1, y), fill=WOOD_DARK)
    d.point((x + w // 4, y - h // 2), fill=(90, 120, 170))


def cave(d, x, y, s=1.0):
    """동굴 입구: 바위 둔덕 + 검은 아치. (x, y) = 입구 바닥."""
    rw, rh = int(34 * s), int(26 * s)
    d.ellipse((x - rw // 2, y - rh, x + rw // 2, y + rh // 4), fill=ROCK, outline=ROCK_DARK)
    d.ellipse((x - rw // 2 + 3, y - rh + 2, x + rw // 2 - 8, y - rh // 3), fill=ROCK_LIGHT)
    for _ in range(int(10 * s)):
        rx, ry = random.randint(-rw // 2 + 2, rw // 2 - 2), random.randint(-rh + 3, rh // 5)
        d.point((x + rx, y + ry), fill=ROCK_DARK)
    aw, ah = int(12 * s), int(12 * s)
    d.pieslice((x - aw // 2, y - ah, x + aw // 2, y + 1), 180, 360, fill=DARK, outline=ROCK_DARK)
    d.rectangle((x - aw // 2, y - ah // 2, x + aw // 2, y), fill=DARK)


def tomb(d, x, y):
    """예언자의 무덤: 눈 산 밑의 돌 문 (기둥 둘 + 상인방 + 어두운 문)."""
    d.rectangle((x - 14, y - 2, x + 14, y + 3), fill=ROCK_DARK)
    d.rectangle((x - 12, y - 20, x - 7, y), fill=ROCK_LIGHT, outline=ROCK_DARK)
    d.rectangle((x + 7, y - 20, x + 12, y), fill=ROCK_LIGHT, outline=ROCK_DARK)
    d.rectangle((x - 15, y - 24, x + 15, y - 19), fill=ROCK, outline=ROCK_DARK)
    d.rectangle((x - 6, y - 18, x + 6, y), fill=DARK)
    d.line([(x - 15, y - 25), (x + 15, y - 25)], fill=SNOW, width=2)


def crypt(d, x, y):
    """납골당: 돌 영묘 — 박공 지붕 + 철문 + 작은 십자."""
    d.rectangle((x - 12, y - 12, x + 12, y), fill=ROCK, outline=ROCK_DARK)
    d.polygon([(x - 14, y - 12), (x, y - 20), (x + 14, y - 12)], fill=ROCK_LIGHT, outline=ROCK_DARK)
    d.rectangle((x - 3, y - 9, x + 3, y), fill=DARK)
    d.line([(x, y - 24), (x, y - 20)], fill=ROCK_DARK, width=1)
    d.line([(x - 2, y - 23), (x + 2, y - 23)], fill=ROCK_DARK, width=1)


def tent(d, x, y, w, h, col, flip=False):
    tip = (x, y - h)
    l, r = (x - w // 2, y), (x + w // 2, y)
    d.polygon([tip, l, r], fill=col, outline=WOOD_DARK)
    d.polygon([tip, (x, y), r], fill=tuple(int(c * 0.78) for c in col))
    ex = x + (w // 6 if flip else -w // 6)
    d.polygon([(ex, y - h * 0.55), (ex - w * 0.12, y), (ex + w * 0.12, y)], fill=DARK)


def camp(d, x, y):
    d.ellipse((x - 34, y - 14, x + 34, y + 14), fill=(176, 150, 98), outline=SAND_DARK)
    for i in range(-5, 6):
        fx = x + i * 6
        d.line([(fx, y - 14 + abs(i)), (fx, y - 21 + abs(i))], fill=WOOD_DARK, width=2)
    tent(d, x - 17, y - 1, 20, 14, (150, 122, 84))
    tent(d, x + 17, y + 1, 22, 15, (132, 106, 72), flip=True)
    tent(d, x, y + 9, 18, 12, (160, 130, 92))
    d.ellipse((x - 5, y - 5, x + 5, y - 1), fill=(70, 50, 36))
    d.polygon([(x - 3, y - 3), (x, y - 11), (x + 3, y - 3)], fill=(236, 120, 36))
    d.polygon([(x - 1, y - 3), (x, y - 7), (x + 1, y - 3)], fill=(255, 220, 90))
    d.line([(x + 28, y - 22), (x + 28, y - 4)], fill=WOOD_DARK, width=2)
    d.polygon([(x + 29, y - 22), (x + 39, y - 18), (x + 29, y - 14)], fill=(190, 40, 40))


def docks(d, x, y):
    """항구: 바다로 나간 나무 부두 둘 + 배 한 척."""
    for (ox, oy, ln) in ((6, 8, 46), (-10, 20, 34)):
        d.rectangle((x + ox, y + oy, x + ox + ln, y + oy + 4), fill=WOOD, outline=WOOD_DARK)
        for k in range(x + ox + 4, x + ox + ln, 8):
            d.line([(k, y + oy + 4), (k, y + oy + 8)], fill=WOOD_DARK, width=1)
    bx, by = x + 40, y + 26
    d.polygon([(bx - 10, by), (bx + 10, by), (bx + 7, by + 5), (bx - 7, by + 5)], fill=WOOD_DARK)
    d.line([(bx, by), (bx, by - 14)], fill=WOOD_DARK, width=1)
    d.polygon([(bx + 1, by - 13), (bx + 9, by - 6), (bx + 1, by - 4)], fill=(240, 236, 220))


def tunnel(d, x, y):
    """해저터널: 해안의 돌 아치 + 물속으로 이어지는 어두운 관."""
    for i in range(6):
        d.ellipse((x + 10 + i * 6, y + 2 + i * 5, x + 22 + i * 6, y + 10 + i * 5), fill=(26, 60, 120))
    d.rectangle((x - 16, y - 2, x + 12, y + 7), fill=ROCK, outline=ROCK_DARK)
    d.pieslice((x - 14, y - 22, x + 10, y + 2), 180, 360, fill=WALL, outline=ROCK_DARK)
    d.rectangle((x - 14, y - 10, x + 10, y), fill=WALL)
    d.pieslice((x - 9, y - 17, x + 5, y - 3), 180, 360, fill=DARK)
    d.rectangle((x - 9, y - 10, x + 5, y), fill=DARK)
    for yy in range(y - 20, y + 2, 5):
        d.line([(x - 14, yy), (x + 10, yy)], fill=WALL_DARK, width=1)
    for i in range(3):
        d.arc((x + 14 + i * 9, y + 1 + i * 6, x + 30 + i * 9, y + 9 + i * 6), 200, 340, fill=FOAM, width=1)


def bunker(d, x, y):
    d.rectangle((x - 13, y - 11, x + 11, y + 1), fill=(176, 176, 170), outline=(80, 80, 78))
    d.rectangle((x - 13, y - 14, x + 11, y - 11), fill=(200, 200, 194), outline=(80, 80, 78))
    d.rectangle((x - 4, y - 6, x + 2, y + 1), fill=(50, 56, 70))
    d.rectangle((x + 5, y - 9, x + 9, y - 5), fill=(70, 74, 80))
    d.line([(x - 8, y - 14), (x - 8, y - 25)], fill=(60, 60, 64), width=1)
    d.line([(x - 12, y - 23), (x - 4, y - 23)], fill=(60, 60, 64), width=1)
    d.point((x - 8, y - 26), fill=(255, 80, 80))


def snow_hut(d, x, y):
    house(d, x, y, 12, 8, roof=(120, 96, 70), wall=(170, 150, 120))
    d.line([(x - 8, y - 8), (x, y - 13), (x + 8, y - 8)], fill=SNOW, width=2)


def temple(d, x, y):
    d.rectangle((x - 18, y + 3, x + 18, y + 7), fill=(190, 182, 160), outline=WALL_DARK)
    d.rectangle((x - 15, y - 9, x + 15, y + 3), fill=(206, 198, 176))
    for i in range(4):
        cx = x - 13 + i * 8
        d.rectangle((cx, y - 9, cx + 3, y + 3), fill=(236, 230, 210), outline=WALL_DARK)
    d.polygon([(x - 19, y - 9), (x, y - 19), (x + 19, y - 9)], fill=(176, 156, 124), outline=WALL_DARK)
    d.polygon([(x - 13, y - 10), (x, y - 16), (x + 13, y - 10)], fill=(220, 212, 190))
    d.rectangle((x - 2, y - 6, x + 2, y + 3), fill=DARK)


def cellar(d, x, y):
    d.rectangle((x - 12, y - 8, x + 12, y + 8), fill=(120, 112, 100), outline=WALL_DARK)
    for i in range(4):
        k = 1 - i * 0.22
        d.rectangle((x - 9 + i * 2, y - 6 + i * 3, x + 9 - i * 2, y - 3 + i * 3), fill=tuple(int(c * k) for c in (150, 140, 124)))
    d.rectangle((x - 3, y + 4, x + 3, y + 7), fill=DARK)


def sewer(d, x, y):
    d.rectangle((x - 13, y - 6, x + 13, y + 8), fill=(110, 104, 96), outline=WALL_DARK)
    d.pieslice((x - 9, y - 10, x + 9, y + 8), 180, 360, fill=DARK, outline=(60, 58, 54))
    d.rectangle((x - 9, y - 1, x + 9, y + 7), fill=DARK)
    for xx in range(x - 6, x + 7, 4):
        d.line([(xx, y - 6), (xx, y + 7)], fill=(84, 80, 76), width=1)
    d.line([(x - 8, y + 8), (x + 8, y + 8)], fill=(40, 70, 110), width=1)


def stair_down(d, x, y):
    """신전지하: 작은 돌 계단 구멍."""
    d.rectangle((x - 7, y - 5, x + 7, y + 5), fill=(120, 112, 100), outline=WALL_DARK)
    d.rectangle((x - 5, y - 3, x + 5, y + 3), fill=(80, 74, 66))
    d.rectangle((x - 3, y - 1, x + 3, y + 3), fill=DARK)


# 성곽 = 정문(윈터하츠 자리) 위로 선 타원 — 신전 · 신전지하 · 리이드 지하 · 언더시티가 전부 안에 든다.
CASTLE_RX, CASTLE_RY = 132, 128
CASTLE_UP = 70          # 성곽 중심은 윈터하츠 스터드(정문 안쪽)보다 이만큼 위


def castle_centre():
    x, y = px("윈터하츠")
    return x, y - CASTLE_UP


def castle(d, cx, cy):
    """윈터하츠: 큰 성곽 도시 — 두꺼운 타원 성벽(흉벽 · 탑 아홉) · 정문 · 돌바닥 · 십자 거리 · 거리를 따라 줄지어 선
    집 · 가운데 성채 · 위쪽 신전 광장. (cx, cy) = 성곽 중심."""
    rx, ry = CASTLE_RX, CASTLE_RY
    # 돌바닥: 바탕 + 어긋난 포석 격자
    d.ellipse((cx - rx, cy - ry, cx + rx, cy + ry), fill=(168, 160, 142))
    rng = random.Random(23)
    for yy in range(cy - ry, cy + ry, 4):
        off = 2 if (yy // 4) % 2 else 0
        for xx in range(cx - rx + off, cx + rx, 5):
            if ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 < 0.92:
                k = rng.choice((0.94, 1.0, 1.0, 1.05))
                d.rectangle((xx, yy, xx + 3, yy + 2), fill=tuple(int(c * k) for c in (176, 168, 150)))
    # 성벽: 바깥 어두운 선 · 돌 띠(밝음 → 어두움 안쪽으로) · 안쪽 그림자
    bands = ((0, WALL_DARK), (1, WALL_DARK), (2, WALL_LIGHT), (3, WALL), (4, WALL), (5, WALL), (6, WALL),
             (7, WALL_DARK), (8, (120, 114, 104)), (9, (140, 132, 118)))
    for k, col in bands:
        d.ellipse((cx - rx + k, cy - ry + k, cx + rx - k, cy + ry - k), outline=col)
    # 흉벽: 바깥 테를 따라 돌이 하나 걸러
    for i in range(0, 360, 5):
        a = math.radians(i)
        tx, ty = cx + (rx - 2) * math.cos(a), cy + (ry - 2) * math.sin(a)
        d.rectangle((tx - 1, ty - 2, tx + 1, ty), fill=WALL_LIGHT if (i // 5) % 2 == 0 else WALL_DARK)
    # 탑 아홉: 큰 모서리 넷 + 작은 다섯(좌우 측문 · 위 · 정문 양옆)
    for (a, r) in ((225, 9), (315, 9), (135, 9), (45, 9), (180, 7), (0, 7), (270, 7), (96, 6), (84, 6)):
        tx, ty = cx + (rx - 5) * math.cos(math.radians(a)), cy + (ry - 5) * math.sin(math.radians(a))
        d.rectangle((tx - r, ty - r * 2, tx + r, ty + r // 2), fill=WALL, outline=WALL_DARK)
        d.rectangle((tx - r + 2, ty - r * 2, tx - r + 4, ty + r // 2), fill=WALL_LIGHT)
        d.rectangle((tx + r - 4, ty - r * 2, tx + r - 2, ty + r // 2), fill=WALL_DARK)
        d.polygon([(tx - r - 1, ty - r * 2), (tx, ty - r * 3), (tx + r + 1, ty - r * 2)], fill=ROOF, outline=ROOF_DARK)
        d.polygon([(tx - r - 1, ty - r * 2), (tx, ty - r * 3), (tx, ty - r * 2)], fill=ROOF_LIGHT)
        d.point((tx, ty - r), fill=DARK)
    # 거리: 정문 ↔ 성채 ↔ 신전 세로 · 좌우 측문 가로 (돌보다 어두운 흙)
    gx, gy = cx, cy + ry
    tpx, tpy = px("신전")
    lx, ly = px("리이드 지하")
    ux, uy = px("언더시티")
    d.line([(gx, gy - 8), (gx, tpy + 14)], fill=(150, 134, 104), width=7)
    d.line([(lx + 14, ly), (ux - 14, uy)], fill=(150, 134, 104), width=6)
    d.line([(gx, gy - 8), (gx, tpy + 14)], fill=(190, 172, 136), width=3)
    d.line([(lx + 14, ly), (ux - 14, uy)], fill=(190, 172, 136), width=2)
    # 정문: 아래 성벽의 아치 + 양옆 돌기둥
    d.rectangle((gx - 12, gy - 18, gx + 12, gy + 2), fill=WALL, outline=WALL_DARK)
    d.rectangle((gx - 12, gy - 18, gx - 9, gy + 2), fill=WALL_LIGHT)
    d.pieslice((gx - 7, gy - 16, gx + 7, gy - 2), 180, 360, fill=DARK)
    d.rectangle((gx - 7, gy - 9, gx + 7, gy + 1), fill=DARK)
    d.line([(gx - 7, gy - 5), (gx + 7, gy - 5)], fill=(90, 80, 70), width=1)
    # 집: 거리를 따라 네 구역에 줄지어 — (x0, y, n, step, w)
    rows = [
        (lx + 24, ly - 14, 3, 20, 14), (ux - 30, ly - 14, 3, -20, 14),
        (lx + 24, ly + 26, 3, 20, 14), (ux - 30, ly + 26, 3, -20, 14),
        (gx - 22, gy - 34, 2, -20, 13), (gx + 22, gy - 34, 2, 20, 13),
        (gx - 22, gy - 56, 2, -20, 13), (gx + 22, gy - 56, 2, 20, 13),
        (gx - 24, tpy + 36, 2, -20, 13), (gx + 24, tpy + 36, 2, 20, 13),
    ]
    for (x0, y, n, step, w) in rows:
        for i in range(n):
            house(d, x0 + i * step, y, w + rng.randint(-1, 2), rng.randint(8, 10))
    # 가운데 성채: 기단 + 높은 탑 셋 + 깃발
    kx, ky = cx, cy + 10
    d.rectangle((kx - 30, ky - 4, kx + 30, ky + 10), fill=WALL, outline=WALL_DARK)
    for (ox, hh) in ((-18, 30), (0, 44), (18, 30)):
        tx = kx + ox
        d.rectangle((tx - 7, ky - hh, tx + 7, ky + 6), fill=WALL, outline=WALL_DARK)
        d.rectangle((tx - 5, ky - hh, tx - 3, ky + 6), fill=WALL_LIGHT)
        d.rectangle((tx + 4, ky - hh, tx + 6, ky + 6), fill=WALL_DARK)
        d.polygon([(tx - 9, ky - hh), (tx, ky - hh - 12), (tx + 9, ky - hh)], fill=ROOF, outline=ROOF_DARK)
        d.polygon([(tx - 9, ky - hh), (tx, ky - hh - 12), (tx, ky - hh)], fill=ROOF_LIGHT)
        d.point((tx, ky - hh + 8), fill=(90, 120, 170))
        d.point((tx, ky - hh + 16), fill=(90, 120, 170))
    d.line([(kx, ky - 56), (kx, ky - 66)], fill=WOOD_DARK, width=1)
    d.polygon([(kx + 1, ky - 66), (kx + 9, ky - 63), (kx + 1, ky - 60)], fill=(60, 90, 200))
    # 신전 광장: 밝은 돌 타원
    d.ellipse((tpx - 30, tpy - 22, tpx + 30, tpy + 14), fill=(196, 190, 172), outline=(150, 142, 126))


def scatter(d, mask, fn, n, seed, margin=0, min_dist=10):
    """`mask` 안에 스프라이트 n 개를 겹치지 않게 뿌린다. y 순으로 그려 겹침이 자연스럽다."""
    rng = random.Random(seed)
    ys, xs = np.nonzero(mask)
    if len(xs) == 0:
        return
    pts = []
    tries = 0
    while len(pts) < n and tries < n * 30:
        tries += 1
        i = rng.randrange(len(xs))
        x, y = int(xs[i]), int(ys[i])
        if x < margin or y < margin or x > W - margin or y > H - margin:
            continue
        if all((x - a) ** 2 + (y - b) ** 2 >= min_dist ** 2 for a, b in pts):
            pts.append((x, y))
    for x, y in sorted(pts, key=lambda p: p[1]):
        fn(d, x, y)


def road(d, a, b, seed):
    """흙길: 두 장소 사이를 조금 흔들리는 폴리라인으로, 어두운 테 + 밝은 속."""
    rng = random.Random(seed)
    (x0, y0), (x1, y1) = a, b
    n = max(4, int(math.hypot(x1 - x0, y1 - y0) / 28))
    dx, dy = x1 - x0, y1 - y0
    ln = math.hypot(dx, dy) or 1
    nx, ny = -dy / ln, dx / ln
    pts = []
    for i in range(n + 1):
        t = i / n
        wob = math.sin(t * math.pi) * rng.uniform(-9, 9)
        pts.append((x0 + dx * t + nx * wob, y0 + dy * t + ny * wob))
    d.line(pts, fill=ROAD_EDGE, width=5, joint="curve")
    d.line(pts, fill=ROAD, width=3, joint="curve")


def keep_out(mask, cx, cy, rx, ry):
    """스프라이트를 뿌리지 않을 자리(지형지물 주변)를 마스크에서 뺀다."""
    yy, xx = np.mgrid[0:H, 0:W]
    return mask & ~((((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2) < 1)


# ── 조립 ───────────────────────────────────────────────────────────────────────

def build(mark_out=None):
    random.seed(5)
    base, land = paint_base()
    img = Image.fromarray(base)
    d = ImageDraw.Draw(img)
    inland = binary_erode(land, 6)

    # 지대
    snow_zone = zone(150, 190, 150, 170, 31) & inland
    ne_mtn = zone(960, 190, 190, 140, 32) & inland
    mullak = zone(930, 300, 125, 85, 33, wobble=0.6) & inland
    hills = zone(800, 545, 125, 72, 34, wobble=0.7) & inland
    cardiff = zone(430, 560, 95, 135, 35) & inland
    valley_wood = zone(300, 330, 90, 70, 36) & inland
    west_wood = zone(260, 690, 60, 60, 37) & inland
    south_wood = zone(560, 640, 90, 60, 38) & inland
    east_wood = zone(1000, 330, 60, 50, 39) & inland
    arr = np.asarray(img).copy()
    arr[snow_zone] = SNOW
    sn = noise((H, W), 7, 41) > 0.6
    arr[snow_zone & sn] = SNOW_SHADE
    for z in (cardiff, valley_wood, west_wood, south_wood, east_wood):
        arr[z] = FOREST_FLOOR
    arr[hills] = HILL
    hn = noise((H, W), 6, 42) > 0.55
    arr[hills & hn] = HILL_DARK
    arr[mullak | ne_mtn] = (150, 126, 86)
    # 질감: 풀밭의 밝은 풀 포기 · 어두운 점, 산지의 바위 점, 눈밭의 푸른 그늘 점
    tex = noise((H, W), 3, 43)
    plain_land = inland & ~(snow_zone | ne_mtn | mullak | hills)
    arr[plain_land & (tex > 0.86)] = (138, 190, 96)
    arr[plain_land & (tex < 0.12)] = (78, 128, 56)
    brown = inland & (ne_mtn | mullak)
    arr[brown & (tex > 0.84)] = (176, 150, 108)
    arr[brown & (tex < 0.14)] = (118, 96, 62)
    arr[snow_zone & (tex < 0.10)] = (172, 190, 214)
    img = Image.fromarray(arr)
    d = ImageDraw.Draw(img)
    # 풀 포기 표시 (작은 v 자) — 평야 곳곳
    rng = random.Random(44)
    ys, xs = np.nonzero(plain_land & binary_erode(plain_land, 4))
    for _ in range(900):
        i = rng.randrange(len(xs))
        x, y = int(xs[i]), int(ys[i])
        col = (134, 186, 94) if rng.random() < 0.7 else (84, 136, 60)
        d.line([(x - 1, y - 1), (x, y), (x + 1, y - 1)], fill=col, width=1)

    # 호수 하나 (노르도스트 평원 북쪽)
    d.ellipse((880, 400, 940, 432), fill=WATER_LAKE, outline=SAND_DARK)
    d.ellipse((886, 404, 920, 420), fill=SEA_LIGHT)

    # 길 (성 안 · 바다 건너는 길 제외)
    for i, (a, b) in enumerate(ROADS):
        if (a, b) in SEA_ROADS or (a in INSIDE_CASTLE and b in INSIDE_CASTLE):
            continue
        pa, pb = px(a), px(b)
        # 성 정문에서 나오는 길은 정문 밑에서 시작
        if a == "윈터하츠":
            pa = (pa[0], pa[1] + 2)
        road(d, pa, pb, 100 + i)
    # 바다 건너는 길: 물속 점선
    for (a, b) in SEA_ROADS:
        (x0, y0), (x1, y1) = px(a), px(b)
        for t in np.linspace(0.15, 0.85, 7):
            x, y = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
            d.ellipse((x - 2, y - 1, x + 2, y + 1), fill=(26, 60, 120))

    # 지형지물 주변은 나무를 안 심는다
    keep = inland.copy()
    cxx, cyy = castle_centre()
    keep = keep_out(keep, cxx, cyy, CASTLE_RX + 24, CASTLE_RY + 24)
    for name, (rx, ry) in {"실바나르": (250, 620), "신드라프": (36, 26), "노르덴항": (60, 40),
                           "어둠동굴": (22, 18), "도적굴": (22, 18), "협곡지하": (22, 18), "반란군 캠프": (380, 900), "납골당": (145, 815),
                           "예언자의 무덤": (22, 18), "서리골짜기": (30, 22), "해저터널": (24, 18)}.items():
        x, y = px(name)
        keep = keep_out(keep, x, y, rx, ry)
    road_mask = np.zeros((H, W), bool)
    rm = Image.new("L", (W, H), 0)
    rd = ImageDraw.Draw(rm)
    for (a, b) in ROADS:
        if (a, b) in SEA_ROADS or (a in INSIDE_CASTLE and b in INSIDE_CASTLE):
            continue
        rd.line([px(a), px(b)], fill=255, width=14)
    road_mask = np.asarray(rm) > 0
    keep &= ~road_mask

    # 산: 북동 산맥(눈 덮인 큰 봉우리) · 멀락협곡(갈색 능선) · 북서 눈 산
    scatter(d, keep & ne_mtn, lambda d, x, y: mountain(d, x, y, random.randint(34, 50), random.randint(30, 46), snow=True, brown=False), 16, 51, min_dist=26)
    scatter(d, keep & mullak, lambda d, x, y: mountain(d, x, y, random.randint(28, 40), random.randint(22, 34), snow=False, brown=True), 14, 52, min_dist=22)
    scatter(d, keep & snow_zone, lambda d, x, y: mountain(d, x, y, random.randint(26, 40), random.randint(24, 40), snow=True, brown=False), 12, 53, min_dist=26)
    scatter(d, keep & hills, lambda d, x, y: mountain(d, x, y, random.randint(18, 26), random.randint(10, 16), snow=False, brown=True), 10, 54, min_dist=18)

    # 숲
    scatter(d, keep & cardiff, lambda d, x, y: pine(d, x, y, random.uniform(0.8, 1.2)), 110, 61, min_dist=9)
    scatter(d, keep & valley_wood, lambda d, x, y: pine(d, x, y, random.uniform(0.8, 1.1)), 50, 62, min_dist=9)
    scatter(d, keep & west_wood, lambda d, x, y: tree(d, x, y, random.uniform(0.8, 1.1)), 26, 63, min_dist=9)
    scatter(d, keep & south_wood, lambda d, x, y: tree(d, x, y, random.uniform(0.8, 1.2)), 50, 64, min_dist=9)
    scatter(d, keep & east_wood, lambda d, x, y: tree(d, x, y, 0.9), 22, 65, min_dist=9)
    # 풀밭에 드문드문 나무 · 바위
    plain = keep & ~(cardiff | valley_wood | west_wood | south_wood | east_wood | snow_zone | ne_mtn | mullak | hills)
    scatter(d, plain, lambda d, x, y: tree(d, x, y, random.uniform(0.7, 1.0)) if random.random() < 0.6 else pine(d, x, y, 0.9), 70, 66, min_dist=16)
    scatter(d, plain, lambda d, x, y: d.ellipse((x - 3, y - 2, x + 3, y + 1), fill=ROCK, outline=ROCK_DARK), 30, 67, min_dist=14)
    # 눈밭의 앙상한 나무
    scatter(d, keep & snow_zone, bare_tree, 18, 68, min_dist=12)

    # 지형지물
    castle(d, *castle_centre())
    temple(d, *px("신전"))
    stair_down(d, *px("신전지하"))
    cellar(d, *px("리이드 지하"))
    sewer(d, *px("언더시티"))
    vx, vy = px("실바나르")
    d.rectangle((vx - 30, vy + 6, vx - 6, vy + 18), fill=(176, 150, 84), outline=SAND_DARK)   # 밭
    for k in range(vx - 28, vx - 6, 4):
        d.line([(k, vy + 7), (k, vy + 17)], fill=(150, 124, 64), width=1)
    for (ox, oy, w) in ((-16, -8, 14), (2, -12, 16), (18, -2, 14), (-2, 6, 15), (20, 12, 13)):
        house(d, vx + ox, vy + oy, w, 9)
    sx, sy = px("신드라프")
    d.ellipse((sx - 32, sy - 12, sx + 32, sy + 16), fill=HILL, outline=HILL_DARK)
    for (ox, oy) in ((-14, 2), (4, -4), (16, 8), (-2, 12)):
        house(d, sx + ox, sy + oy, 12, 8, roof=(120, 96, 70), wall=(190, 180, 160))
    hx, hy = px("노르덴항")
    docks(d, hx, hy)
    for (ox, oy, w) in ((-22, -10, 14), (-6, -16, 16), (10, -8, 14), (-14, 4, 15), (4, 6, 14), (-26, 8, 12)):
        house(d, hx + ox, hy + oy, w, 10)
    d.rectangle((hx - 2, hy - 34, hx + 6, hy - 14), fill=WALL, outline=WALL_DARK)   # 등대 겸 탑
    d.polygon([(hx - 4, hy - 34), (hx + 2, hy - 42), (hx + 8, hy - 34)], fill=ROOF, outline=ROOF_DARK)
    cave(d, *px("어둠동굴"))
    cave(d, px("도적굴")[0], px("도적굴")[1], 0.9)
    cave(d, *px("협곡지하"))
    camp(d, *px("반란군 캠프"))
    crypt(d, *px("납골당"))
    tomb(d, px("예언자의 무덤")[0], px("예언자의 무덤")[1] + 8)
    fx, fy = px("서리골짜기")
    for (ox, oy) in ((-14, 4), (6, -6), (14, 10)):
        snow_hut(d, fx + ox, fy + oy)
    tunnel(d, *px("해저터널"))
    ix, iy = px("버려진 섬")
    for (ox, oy) in ((-26, -4), (18, -8), (6, 10)):
        bare_tree(d, ix + ox, iy + oy)
    d.polygon([(ix - 10, iy + 2), (ix + 4, iy - 8), (ix + 14, iy + 4)], fill=(118, 96, 68), outline=WOOD_DARK)
    bunker(d, px("지하연구소")[0], px("지하연구소")[1] + 2)

    img.save(OUT)
    print(f"-> {OUT} {W}x{H}")
    if mark_out:
        m = img.copy()
        md = ImageDraw.Draw(m)
        for name in PLACES:
            x, y = px(name)
            md.line([(x - 6, y), (x + 6, y)], fill=(255, 0, 255), width=1)
            md.line([(x, y - 6), (x, y + 6)], fill=(255, 0, 255), width=1)
        m.save(mark_out)
        print(mark_out)


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    out = sys.argv[sys.argv.index("--marks") + 1] if "--marks" in sys.argv else None
    build(out)
