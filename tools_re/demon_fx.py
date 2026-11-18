#!/usr/bin/env python3
"""「악마화」 이펙트 라이브러리 (2026-10-14) — `docs/skill/astra_demon/ingame` 예상도 → 게임 해상도 스프라이트 + 프레임표.

사용자: *"docs\\skill\\astra_demon 안에 이펙트들을 모아놨어. 인게임 구현이펙트는 ingame 에 넣어놨어 해당 폴더 분석해서 악의 관련
이펙트 전수 변경해줘."* — 예상도 12 장면(변신 출현 · 할퀴기 1~3 단 · 돌진 · 추종 100 % · 추종 폭발 · 지옥칼날 · 어둠심판 · 멸화 ·
용암불 연소 · 냉각)과 보드 4 장(`boards/01~04`)의 단계 설명을 게임 해상도(176 폭 · 타일 16 · 악마 30×36)의 픽셀 그림으로 다시 그린다.

이 파일이 만드는 것 = **`wie_android/src/demon_fx.bin`**(H2FX 이펙트 라이브러리 — 형식은 `vendor/wie/wie_backend/src/canvas/fx.rs`):
아틀라스 PNG + 스프라이트(사각 · 기준점) + 이펙트(프레임 = 부품 `(sprite, dx, dy, alpha, flags)` 목록). 호스트가 부팅 때 캔버스에
등록하고(`wie_backend::canvas::set_fx_library`), 게스트의 데몬 블록 그림 조각이 `setAlpha(FX_TAG | 1)` 뒤 `drawLine(x, y, x + 효과,
y + 나이)` 한 번으로 「효과 N 의 나이 t 프레임을 (x, y) 에」 그린다. 효과 번호는 `EFFECTS` 의 순서이고 `DemonPatch::FX_*` 와 한 계약이다
(`check` 가 이름 · 길이를 대조하고, 러스트 테스트가 같은 일을 한다).

기준점(앵커) = 효과가 붙는 개체의 「발바닥」 = 개체 world px + `DemonPatch::FEET`(8, 10) + 카메라 — 악마 30×36 은 그 점에서
(−15, −34) 에 그려진다. 용암 칸만 칸의 왼쪽 위(타일 × 16 + 카메라)가 기준점이다. 층: GROUND = 땅 그리기(모든 개체 밑) · TOP = HUD
페인터 앞(모든 개체 위).

    python tools_re/demon_fx.py build            # → wie_android/src/demon_fx.bin
    python tools_re/demon_fx.py preview [--out docs/skill/demon_fx] [--zoom 3]
    python tools_re/demon_fx.py check            # bin 의 효과 이름 · 프레임 수
    python tools_re/demon_fx.py compare --out <dir>    # 예상도(176 폭) | 라이브러리 — 같은 배치의 12 장면
    python tools_re/demon_fx.py timeline --out <png>   # 효과마다 몇 프레임을 실제 게임 배경 위에
    python tools_re/demon_fx.py footprint        # 효과마다 그림이 닿는 범위(px · 칸) — 판정 재정의의 기준

2 차 (2026-10-14 밤): 그림 함수는 전부 `demon_fx_art.py`(그림판 `fxpaint.py` — 4 배 슈퍼샘플링 · 노이즈 세기 필드 · 색 띠 ·
결정 조각)에 있다. 이 파일은 계약(효과 번호 · 길이) · 묶기 · 읽기 · 대조 도구만 갖는다. 사용자: *"먼저 이펙트 퀄리티를 훨씬 올리고
그 뒤에 판정범위를 이펙트에 맞게 재정의"* — 그림은 예상도 크기를 따르고 판정보다 클 수 있다(`footprint` 표).
"""

import argparse
import hashlib
import io
import os
import struct
import sys

import numpy as np
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools_re"))
OUT_BIN = os.path.join(ROOT, "wie_android", "src", "demon_fx.bin")
DEMON_DIR = os.path.join(ROOT, "costumes", "demon_300")
# ★★ 2026-10-15 HD (사용자: *"이펙트와 외형 전부 업스케일링하고 자연스럽게"*): 스프라이트마다 예상도 해상도(게임 3×)의 쌍(twin)을
# 싣는다 — `fxpaint.set_ss(3)` 로 그림판을 3× 에 두고 `Canvas.sprites()`(1×) 와 `sprites_hd()`(3×) 를 한 그리기에서 받는다; 오린 그림은
# `costumes/astra_fx_hd`(`astra_cut.py build`) 의 3× 원본. 악마의 몸도 효과(`demon_body`, 115 프레임 = `costumes/demon_300_hd`)로 실린다 —
# HD 모드의 게스트 DRAW 조각이 빈 시트 301 위에서 그 효과를 발행한다(`DemonPatch::FX_BODY`). bin 은 H2FX v2 (fx.rs 머리말):
# 1× 아틀라스 + 효과 뒤에 8 비트 색인 아틀라스(L PNG) + 쌍마다 팔레트(≤ 255 색 + 투명).
HD = 3
DEMON_DIR_HD = os.path.join(ROOT, "costumes", "demon_300_hd")
HD_ATLAS_W = 4096
MOCKUPS = os.path.join(ROOT, "docs", "skill", "astra_demon", "ingame")

# 악마 프레임 (demon_look.POSES × 4 방향): 방향 0 뒤(위) 1 앞(아래) 2 왼 3 오른 — facing − 1 과 같다
# ★ 2026-10-14 밤 (4): 자세 표는 demon_look 하나 — 옛 이름(claw_b 등)은 demon_look.LEGACY 가 지금 이름으로 바꾼다
import demon_look as _dl  # noqa: E402
import ilseom_fx as _il  # noqa: E402  ★ 2026-10-14 일섬 「번개의 호흡」 노란 번개 — EFFECTS 의 뒤
import demon_weapon_fx as _dw  # noqa: E402  ★★ 2026-10-16 악의 무기 — 사방 할퀴기 · 큰 할퀴기
import lava_wide_fx as _lw  # noqa: E402  ★★ 2026-10-18 야수조련 목걸이의 용암 지대 (고리 조각 × 2), EFFECTS 의 맨 뒤

POSES = _dl.POSES
UP, DOWN, LEFT, RIGHT = 0, 1, 2, 3
DIR_NAMES = ["up", "down", "left", "right"]
DEMON_OFF = (-28, -46)  # 악마 PNG 캔버스(56 × 50, `demon_draft.CW · CH`)의 왼쪽 위 (발바닥 기준) — 옛 30 × 36 은 (−15, −34)


FLIP = 1
ADD = 2
X2 = 4  # ★★ 2026-10-18 the sprite at twice its size about its anchor (`wie_backend::canvas::FX_X2`)
RADIAL = 64  # ★★ 2026-10-21 lava on the doubled ring (`wie_backend::canvas::FX_LAVA_RADIAL`)

# ── 효과 번호 (= DemonPatch::FX_* — 순서가 계약) ────────────────────────────────────────────────────────────
TRANSFORM_LEN = 44
CLAW_LEN = 6
CHARGE_LEN = 8
FOLLOW_LEN = 16
DOME_LEN = 16
BURST_LEN = 20
BURST_MOB_LEN = 12
SEAL_LEN = 32
BLADE_LEN = 16
ORB_LEN = 20
BOLT_LEN = 4
FIRE_CORE_LEN = 10
FIRE_BLAST_LEN = 24
FIRE_MOB_LEN = 12
SCORCH_LEN = 64
LAVA_LEN = 100
# ★★ 2026-10-22 악의 = 클레르 자신 + 등 뒤의 검은 날개: `demon_body` 는 날개 16 프레임(방향 4 × 날갯짓 4, `malice_wings.py`) — 악마 몸 115 장은 끝
WING_FRAMES = 16
# ★★ 2026-10-22 (4) 악의 중 발밑의 검정 · 회색 오라 — EFFECTS 의 맨 뒤(`malice_wings.aura_frames`, FXG 조각이 그린다)
AURA_LEN = 16

EFFECTS = (
    [("transform", TRANSFORM_LEN), ("transform_ground", TRANSFORM_LEN)]
    + [(f"claw{c}_{d}", CLAW_LEN) for c in range(4) for d in DIR_NAMES]
    + [(f"charge_ground_{d}", CHARGE_LEN) for d in DIR_NAMES]
    + [(f"charge_top_{d}", CHARGE_LEN) for d in DIR_NAMES]
    + [(f"follow_build{k}", FOLLOW_LEN) for k in range(4)]
    + [("follow_dome", DOME_LEN), ("burst", BURST_LEN), ("burst_mob", BURST_MOB_LEN), ("seal", SEAL_LEN)]
    + [(f"blade_rise{k}", BLADE_LEN) for k in range(2)]
    + [("judge_orb", ORB_LEN)]
    + [(f"bolt{k}", BOLT_LEN) for k in range(4)]
    + [("fire_core", FIRE_CORE_LEN), ("fire_ground", FIRE_CORE_LEN), ("fire_blast", FIRE_BLAST_LEN), ("fire_mob", FIRE_MOB_LEN), ("fire_scorch", SCORCH_LEN)]
    + [(f"lava{v}", LAVA_LEN) for v in range(16)]
    + [("marker", 1)]
    + [(f"lava_top{v}", LAVA_LEN) for v in range(16)]
    + _il.effect_names()
    + [("demon_body", WING_FRAMES)]  # ★★ 2026-10-22 the WINGS (facing × flap — `malice_wings.py`; `DemonPatch::FX_BODY`), the demon body before
    + [("fire_back", FIRE_BLAST_LEN)]  # ★★ 2026-10-15 5 차: 멸화 폭발의 뒤 반쪽 (땅 층 · fire_blast 와 같은 그리기 나이, `fire_fx.py`)
    + _dw.effect_names()  # ★★ 2026-10-16 악의 무기: claw{c}_spin (M2) · claw{c}_{d}_big (M3) — `demon_weapon_fx.py`
    + _lw.effect_names()  # ★★ 2026-10-18 야수조련 목걸이: lava_w{k} · lava_wtop{k} — `lava_wide_fx.py`
    + [("malice_aura", AURA_LEN)]  # ★★ 2026-10-22 (4) 악의 발밑 오라 (`DemonPatch::FX_AURA`)
)
FX_ID = {name: i for i, (name, _) in enumerate(EFFECTS)}
# 러스트 계약 (DemonPatch::FX_*) — check 가 이 값들을 본다
CONTRACT = {
    "FX_TRANSFORM": FX_ID["transform"], "FX_TRANSFORM_GROUND": FX_ID["transform_ground"], "FX_CLAW": FX_ID["claw0_up"],
    "FX_CHARGE_GROUND": FX_ID["charge_ground_up"], "FX_CHARGE_TOP": FX_ID["charge_top_up"], "FX_FOLLOW_BUILD": FX_ID["follow_build0"],
    "FX_FOLLOW_DOME": FX_ID["follow_dome"], "FX_BURST": FX_ID["burst"], "FX_BURST_MOB": FX_ID["burst_mob"], "FX_SEAL": FX_ID["seal"],
    "FX_BLADE_RISE": FX_ID["blade_rise0"], "FX_JUDGE_ORB": FX_ID["judge_orb"], "FX_BOLT": FX_ID["bolt0"], "FX_FIRE_CORE": FX_ID["fire_core"],
    "FX_FIRE_GROUND": FX_ID["fire_ground"], "FX_FIRE_BLAST": FX_ID["fire_blast"], "FX_FIRE_MOB": FX_ID["fire_mob"],
    "FX_FIRE_SCORCH": FX_ID["fire_scorch"], "FX_LAVA": FX_ID["lava0"], "FX_MARKER": FX_ID["marker"],
    "FX_LAVA_TOP": FX_ID["lava_top0"],
    "FX_ILSTREAK": FX_ID["ilstreak_up_1m"], "FX_ILFLASH": FX_ID["ilflash"], "FX_ILSLASH": FX_ID["ilslash0"], "FX_ILBOLT": FX_ID["ilbolt0"],
    "FX_BODY": FX_ID["demon_body"],
    "FX_FIRE_BACK": FX_ID["fire_back"],
    "FX_CLAW_SPIN": FX_ID["claw0_spin"], "FX_CLAW_BIG": FX_ID["claw0_up_big"],
    "FX_LAVA_WIDE": FX_ID["lava_w0"], "FX_LAVA_WIDE_TOP": FX_ID["lava_wtop0"],
    "FX_AURA": FX_ID["malice_aura"],
    "FX_COUNT": len(EFFECTS),
}
CONTRACT_U32 = {"FX_BODY_LEN": WING_FRAMES, "FX_AURA_LEN": AURA_LEN}


# ── 라이브러리 묶음 ─────────────────────────────────────────────────────────────────────────────────────────
class Library:
    def __init__(self, hd=False):
        self.sprites = []  # (arr uint8 RGBA, ax, ay)
        self.twins = {}  # ★ HD: sprite index → (arr uint8 RGBA at scale, ax, ay, scale)
        self.qtwins = {}  # ★ 이미 양자화된 쌍 (다른 bin 에서 그대로 옮긴 것): sprite index → (idx HxW uint8, pal n×4, ax, ay, scale)
        self.index = {}
        self.effects = []  # (name, [ [ (sprite, dx, dy, alpha, flags) ] ])
        self.hd = hd

    def sprite(self, item, twin=None):
        arr, ax, ay = item
        h = hashlib.sha1(arr.tobytes() + struct.pack("<iiII", int(ax), int(ay), arr.shape[1], arr.shape[0]))
        if twin is not None:
            h.update(twin[0].tobytes() + struct.pack("<iiIIi", int(twin[1]), int(twin[2]), twin[0].shape[1], twin[0].shape[0], int(twin[3])))
        key = h.hexdigest()
        if key not in self.index:
            self.index[key] = len(self.sprites)
            self.sprites.append((arr, int(ax), int(ay)))
            if twin is not None:
                self.twins[len(self.sprites) - 1] = (twin[0], int(twin[1]), int(twin[2]), int(twin[3]))
        return self.index[key]

    def sprite_q(self, item, qtwin=None):
        """스프라이트 + 이미 양자화된 쌍(색인 · 팔레트 그대로 — 다시 양자화하지 않는다; `fire_fx.replace` · `lava_fx` 처럼 다른 효과를
        바이트 그대로 옮길 때)."""
        arr, ax, ay = item
        h = hashlib.sha1(arr.tobytes() + struct.pack("<iiII", int(ax), int(ay), arr.shape[1], arr.shape[0]))
        if qtwin is not None:
            idx, pal, tax, tay, scale = qtwin
            h.update(b"q" + idx.tobytes() + pal.tobytes() + struct.pack("<iiIIi", int(tax), int(tay), idx.shape[1], idx.shape[0], int(scale)))
        key = h.hexdigest()
        if key not in self.index:
            self.index[key] = len(self.sprites)
            self.sprites.append((arr, int(ax), int(ay)))
            if qtwin is not None:
                self.qtwins[len(self.sprites) - 1] = qtwin
        return self.index[key]

    def parts_of(self, frame, dx=0, dy=0, flags=0, alpha=255):
        """Frame 하나 → 부품 (보통, 더하기). ★ HD: 그림판이 SS > 1 이면 쌍(`sprites_hd`)도 받는다."""
        parts = []
        base, add = frame.sprites()
        twins = frame.sprites_hd() if self.hd and getattr(frame, "ss", 1) > 1 and hasattr(frame, "sprites_hd") else [None, None]
        if base is not None:
            parts.append((self.sprite(base, twins[0]), dx, dy, alpha, flags))
        if add is not None:
            parts.append((self.sprite(add, twins[1]), dx, dy, alpha, flags | ADD))
        return parts

    def add_effect(self, name, frames):
        assert name == EFFECTS[len(self.effects)][0], f"effect order: {name} at {len(self.effects)}"
        assert len(frames) == EFFECTS[len(self.effects)][1], f"{name}: {len(frames)} frames"
        self.effects.append((name, frames))

    def pack(self):
        """선반 채우기 — (atlas RGBA, [(x, y, w, h, ax, ay)])."""
        W = 1024
        order = sorted(range(len(self.sprites)), key=lambda i: -self.sprites[i][0].shape[0])
        pos = [None] * len(self.sprites)
        x = y = shelf = 0
        for i in order:
            h, w = self.sprites[i][0].shape[:2]
            if x + w > W:
                x, y, shelf = 0, y + shelf + 1, 0
            pos[i] = (x, y)
            x += w + 1
            shelf = max(shelf, h)
        H = y + shelf + 1
        atlas = np.zeros((H, W, 4), np.uint8)
        rects = []
        for i, (arr, ax, ay) in enumerate(self.sprites):
            px, py = pos[i]
            h, w = arr.shape[:2]
            atlas[py:py + h, px:px + w] = arr
            rects.append((px, py, w, h, ax, ay))
        return atlas, rects

    def pack_hd(self):
        """★ HD: 쌍마다 ≤ 255 색으로 양자화(FASTOCTREE, 알파 포함) → 색인 아틀라스(L) + [(sprite, scale, idx HxW, pal n×4, ax, ay)]."""
        items = []
        for s in sorted(set(self.twins) | set(self.qtwins)):
            if s in self.qtwins:
                idx, pal, ax, ay, scale = self.qtwins[s]
                items.append((s, scale, idx, pal, ax, ay))
                continue
            arr, ax, ay, scale = self.twins[s]
            q = quantize_twin(arr)
            if q is None:
                continue
            idx, pal = q
            items.append((s, scale, idx, pal, ax, ay))
        W = HD_ATLAS_W
        order = sorted(range(len(items)), key=lambda i: -items[i][2].shape[0])
        pos = [None] * len(items)
        x = y = shelf = 0
        for i in order:
            h, w = items[i][2].shape[:2]
            if x + w > W:
                x, y, shelf = 0, y + shelf + 1, 0
            pos[i] = (x, y)
            x += w + 1
            shelf = max(shelf, h)
        H = y + shelf + 1
        atlas = np.zeros((H, W), np.uint8)
        rects = []
        for i, (s, scale, idx, pal, ax, ay) in enumerate(items):
            px, py = pos[i]
            h, w = idx.shape[:2]
            atlas[py:py + h, px:px + w] = idx
            rects.append((s, scale, px, py, w, h, ax, ay, pal))
        return atlas, rects

    def write(self, path):
        atlas, rects = self.pack()
        buf = io.BytesIO()
        Image.fromarray(atlas, "RGBA").save(buf, "PNG", optimize=True)
        png = buf.getvalue()
        version = 2 if (self.twins or self.qtwins) else 1
        out = bytearray(b"H2FX")
        out += struct.pack("<II", version, len(png))
        out += png
        out += struct.pack("<I", len(rects))
        for (x, y, w, h, ax, ay) in rects:
            out += struct.pack("<HHHHhh", x, y, w, h, ax, ay)
        out += struct.pack("<I", len(self.effects))
        for name, frames in self.effects:
            nb = name.encode("ascii")
            out += struct.pack("<B", len(nb)) + nb
            out += struct.pack("<I", len(frames))
            for parts in frames:
                out += struct.pack("<H", len(parts))
                for (s, dx, dy, a, f) in parts:
                    out += struct.pack("<HhhBB", s, int(dx), int(dy), int(max(0, min(255, a))), f)
        hd_shape = None
        if version == 2:
            hd_atlas, hd_rects = self.pack_hd()
            hd_shape = hd_atlas.shape
            buf = io.BytesIO()
            Image.fromarray(hd_atlas, "L").save(buf, "PNG", optimize=True)
            hd_png = buf.getvalue()
            out += struct.pack("<I", len(hd_png))
            out += hd_png
            out += struct.pack("<I", len(hd_rects))
            for (s, scale, x, y, w, h, ax, ay, pal) in hd_rects:
                assert 1 <= len(pal) <= 255 and w < 65536 and h < 65536, (s, len(pal), w, h)
                out += struct.pack("<HBBHHHHhh", s, scale, len(pal), x, y, w, h, ax, ay)
                out += pal.astype(np.uint8).tobytes()
        with open(path, "wb") as fh:
            fh.write(out)
        return len(out), atlas.shape, len(rects), hd_shape, len(self.twins) + len(self.qtwins)


def quantize_twin(arr):
    """RGBA → (색인 HxW uint8 — 0 = 투명 · 1..n = 팔레트, 팔레트 n×4 RGBA) 또는 None (빈 그림). FASTOCTREE 는 알파를 함께 양자화한다."""
    opaque = arr[..., 3] > 0
    if not opaque.any():
        return None
    im = Image.fromarray(np.ascontiguousarray(arr), "RGBA")
    q = im.quantize(colors=255, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE)
    idx = np.asarray(q, np.uint16)
    try:
        pal = np.array(q.getpalette(rawmode="RGBA"), np.uint8).reshape(-1, 4)
    except TypeError:  # older Pillow: RGB palette, the alpha from the image's own
        pal3 = np.array(q.getpalette(), np.uint8).reshape(-1, 3)
        pal = np.concatenate([pal3, np.full((len(pal3), 1), 255, np.uint8)], axis=1)
    n = int(idx.max()) + 1
    pal = pal[:n]
    out = np.where(opaque, idx + 1, 0).astype(np.uint8)
    return out, pal


# ── 재료 ───────────────────────────────────────────────────────────────────────────────────────────────────
_DEMON = {}


def demon(pose, view):
    key = (pose, view)
    if key not in _DEMON:
        k = _dl.frame_of(pose, view) - _dl.BODY_BASE
        im = Image.open(os.path.join(DEMON_DIR, f"demon_{k}.png")).convert("RGBA")
        _DEMON[key] = im
    return _DEMON[key]


def demon_hd_path(k):
    return os.path.join(DEMON_DIR_HD, f"demon_{k}.png")


def demon_any(pose, view):
    """★ HD: (그림, 배율) — HD 프레임(`costumes/demon_300_hd`)이 있으면 그것(3×), 없으면 1×."""
    k = _dl.frame_of(pose, view) - _dl.BODY_BASE
    key = ("hd", k)
    if key not in _DEMON:
        path = demon_hd_path(k)
        if os.path.exists(path):
            im = Image.open(path).convert("RGBA")
            _DEMON[key] = (im, im.width // 56)
        else:
            _DEMON[key] = (demon(pose, view), 1)
    return _DEMON[key]


def _bbox(arr):
    ys, xs = np.nonzero(arr[..., 3])
    return int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1


# ── 묶기 ───────────────────────────────────────────────────────────────────────────────────────────────────
def build(hd=True):
    """그림 함수(`demon_fx_art`) → 라이브러리. 왼쪽 할퀴기는 오른쪽 그림의 거울(FLIP), 용암 한 칸은 [0] = 땅 층 · [1:] = 위 층.
    ★ HD: 그림판을 3× 에 두고(`fxpaint.set_ss`) 오린 그림은 3× 원본으로(`demon_fx_cut.HD`) — 스프라이트마다 쌍이 실린다."""
    import demon_fx_art as art
    import demon_fx_cut as cx
    import fxpaint as fp
    if hd:
        fp.set_ss(HD)
        cx.HD = HD
    lib = Library(hd=hd)

    def B(name, n, fn):
        """B 경로: 그 이름으로 들인 프레임 시트(`astra_frames.py import`)가 있으면 그 프레임, 없으면 A(fn)."""
        return [lib.parts_of(cx.imported(name, a, n) or fn(a)) for a in range(n)]
    lib.add_effect("transform", B("transform", TRANSFORM_LEN, cx.transform_top))
    lib.add_effect("transform_ground", B("transform_ground", TRANSFORM_LEN, cx.transform_ground))
    for c in range(4):
        right = [lib.parts_of(cx.claw_frame(c, RIGHT, f)) for f in range(CLAW_LEN)]
        for d in range(4):
            if d == LEFT:
                frames = [[(sp, dx, dy, a, fl | FLIP) for (sp, dx, dy, a, fl) in parts] for parts in right]
            elif d == RIGHT:
                frames = right
            else:
                frames = [lib.parts_of(cx.claw_frame(c, d, f)) for f in range(CLAW_LEN)]
            lib.add_effect(f"claw{c}_{DIR_NAMES[d]}", frames)
    # ★★ 2026-10-16 6 차: 돌진 잔상은 오린 그림이 아니라 악마 자신의 달리기 HD 프레임으로 (`charge_fx.py` — 위 변이 잘리지 않는다)
    import charge_fx as chf
    for d in range(4):
        lib.add_effect(f"charge_ground_{DIR_NAMES[d]}", B(f"charge_ground_{DIR_NAMES[d]}", CHARGE_LEN, lambda f, d=d: chf.charge_ground(d, f)))
    for d in range(4):
        lib.add_effect(f"charge_top_{DIR_NAMES[d]}", B(f"charge_top_{DIR_NAMES[d]}", CHARGE_LEN, lambda f, d=d: chf.charge_top(d, f)))
    for k in range(4):
        lib.add_effect(f"follow_build{k}", B(f"follow_build{k}", FOLLOW_LEN, lambda f, k=k: art.follow_build(k, f)))
    lib.add_effect("follow_dome", B("follow_dome", DOME_LEN, cx.follow_dome))
    lib.add_effect("burst", B("burst", BURST_LEN, cx.burst))
    lib.add_effect("burst_mob", B("burst_mob", BURST_MOB_LEN, art.burst_mob))
    # ★★ 2026-10-16 6 차: 지옥칼날의 인장 · 칼날은 오린 그림이 아니라 절차 그림 (`blade_fx.py` — 지옥에서 칼날이 휘몰아치는 인장)
    import blade_fx as bf
    lib.add_effect("seal", B("seal", SEAL_LEN, bf.seal))
    for k in range(2):
        lib.add_effect(f"blade_rise{k}", B(f"blade_rise{k}", BLADE_LEN, lambda f, k=k: bf.blade_rise(k, f)))
    lib.add_effect("judge_orb", B("judge_orb", ORB_LEN, art.judge_orb))
    for k in range(4):
        lib.add_effect(f"bolt{k}", B(f"bolt{k}", BOLT_LEN, lambda a, k=k: cx.bolt(k, a)))
    # ★★ 2026-10-15 5 차: 멸화는 오린 그림(검은 결정 · 바위 · 금 무늬가 섞였다)이 아니라 3D 입자 화염 (`fire_fx.py`)
    import fire_fx as ff

    def FF(name, n):
        return [lib.parts_of(im) if (im := cx.imported(name, a, n)) else ff.parts(lib, name, a) for a in range(n)]
    lib.add_effect("fire_core", FF("fire_core", FIRE_CORE_LEN))
    lib.add_effect("fire_ground", FF("fire_ground", FIRE_CORE_LEN))
    lib.add_effect("fire_blast", FF("fire_blast", FIRE_BLAST_LEN))
    lib.add_effect("fire_mob", FF("fire_mob", FIRE_MOB_LEN))
    lib.add_effect("fire_scorch", FF("fire_scorch", SCORCH_LEN))
    import lava_fx  # ★★ 2026-10-15 4 차: 초안 11 의 지대를 고리 12 칸의 조각으로 (`lava_fx.py` — 옛 `art.lava_effect` 는 칸마다 사각 타일)
    lava_fx.add_lava_effects(lib, "ground")
    lib.add_effect("marker", [lib.parts_of(art.marker())])
    lava_fx.add_lava_effects(lib, "top")
    _il.add_effects(lib)  # ★ 2026-10-14 일섬 「번개의 호흡」 — ilseom_fx.py (`extend` = 옛 스프라이트 바이트 그대로 붙이기)
    import malice_wings
    lib.add_effect("demon_body", malice_wings.wing_frames(lib))  # ★★ 2026-10-22 the wings (the demon body is gone)
    lib.add_effect("fire_back", FF("fire_back", FIRE_BLAST_LEN))
    _dw.add_effects(lib)  # ★★ 2026-10-16 악의 무기 — 할퀴기에서 만든다
    _lw.add_effects(lib)  # ★★ 2026-10-18 야수조련 목걸이 — 용암 조각에서 만든다
    lib.add_effect("malice_aura", malice_wings.aura_frames(lib))  # ★★ 2026-10-22 (4)
    assert len(lib.effects) == len(EFFECTS)
    return malice_wings.replace_library(lib)  # the four cuts from `costumes/malice_wings` in place of the drawn ones


# ── 읽기 (check · preview) ─────────────────────────────────────────────────────────────────────────────────
def read_bin(path):
    return read_bin_hd(path)[:2]


def read_bin_hd(path):
    """(sprites [(RGBA, ax, ay)], effects [(name, frames)], twins {sprite: (RGBA, ax, ay, scale)}) — v1 은 twins 가 빈다."""
    with open(path, "rb") as stream:
        b = stream.read()
    assert b[:4] == b"H2FX"
    ver, plen = struct.unpack_from("<II", b, 4)
    assert ver in (1, 2), ver
    at = 12
    atlas = Image.open(io.BytesIO(b[at:at + plen])).convert("RGBA")
    at += plen
    (n,) = struct.unpack_from("<I", b, at)
    at += 4
    sprites = []
    A = np.asarray(atlas)
    for _ in range(n):
        x, y, w, h, ax, ay = struct.unpack_from("<HHHHhh", b, at)
        at += 12
        sprites.append((A[y:y + h, x:x + w], ax, ay))
    (ne,) = struct.unpack_from("<I", b, at)
    at += 4
    effects = []
    for _ in range(ne):
        nl = b[at]
        name = b[at + 1:at + 1 + nl].decode()
        at += 1 + nl
        (nf,) = struct.unpack_from("<I", b, at)
        at += 4
        frames = []
        for _ in range(nf):
            (np_,) = struct.unpack_from("<H", b, at)
            at += 2
            parts = []
            for _ in range(np_):
                parts.append(struct.unpack_from("<HhhBB", b, at))
                at += 8
            frames.append(parts)
        effects.append((name, frames))
    twins = {}
    if ver == 2:
        (hlen,) = struct.unpack_from("<I", b, at)
        at += 4
        hd_atlas = np.asarray(Image.open(io.BytesIO(b[at:at + hlen])).convert("L")) if hlen else None
        at += hlen
        (nh,) = struct.unpack_from("<I", b, at)
        at += 4
        for _ in range(nh):
            s, scale, ncol, x, y, w, h, ax, ay = struct.unpack_from("<HBBHHHHhh", b, at)
            at += 16
            pal = np.frombuffer(b[at:at + 4 * ncol], np.uint8).reshape(ncol, 4)
            at += 4 * ncol
            idx = hd_atlas[y:y + h, x:x + w]
            full = np.concatenate([np.zeros((1, 4), np.uint8), pal], axis=0)
            twins[s] = (full[idx], ax, ay, scale)
    assert at == len(b)
    return sprites, effects, twins


def _draw_lava_radial(canvas, arr, ax, ay, x, y, dx, dy, alpha, fl):
    """The necklace's lava: stretch the 1× piece from the beast's tile edge out to the radius-4 ring."""
    H, W = canvas.shape[:2]
    h, w = arr.shape[:2]
    rdx, pdx = dx // 1000, dx % 1000 - 500
    rdy, pdy = dy // 1000, dy % 1000 - 500
    flip = fl & FLIP
    corners = []
    for sx in (0.0, float(w)):
        for sy in (0.0, float(h)):
            relx = (ax - sx) if flip else (sx - ax)
            rely = sy - ay
            wx, wy = rdx * 16 + pdx + relx, rdy * 16 + pdy + rely
            cx, cy = wx - 8.0, wy - 8.0
            r = (cx * cx + cy * cy) ** 0.5
            s = 1.0 if r <= 8.0 else (r * 2.0 - 8.0) / r
            corners.append((x + 8.0 + cx * s - rdx * 16, y + 8.0 + cy * s - rdy * 16))
    xs = [c[0] for c in corners]
    ys = [c[1] for c in corners]
    x0, x1 = max(0, int(min(xs))), min(W, int(max(xs)) + 1)
    y0, y1 = max(0, int(min(ys))), min(H, int(max(ys)) + 1)
    if x0 >= x1 or y0 >= y1:
        return
    yy, xx = np.mgrid[y0:y1, x0:x1]
    wx = xx + 0.5 - x + rdx * 16
    wy = yy + 0.5 - y + rdy * 16
    cx, cy = wx - 8.0, wy - 8.0
    rp = np.maximum(np.hypot(cx, cy), 1e-3)
    s = np.where(rp <= 8.0, 1.0, (rp + 8.0) * 0.5 / rp)
    relx = 8.0 + cx * s - rdx * 16 - pdx
    rely = 8.0 + cy * s - rdy * 16 - pdy
    px = (ax - relx) if flip else (ax + relx)
    py = ay + rely
    ix = np.floor(px).astype(np.int32)
    iy = np.floor(py).astype(np.int32)
    ok = (ix >= 0) & (iy >= 0) & (ix < w) & (iy < h)
    if not ok.any():
        return
    sub = np.zeros((y1 - y0, x1 - x0, 4), np.float32)
    sub[ok] = arr[iy[ok], ix[ok]]
    sa = sub[..., 3:4] / 255.0 * (alpha / 255.0)
    if fl & ADD:
        canvas[y0:y1, x0:x1] = np.minimum(255, canvas[y0:y1, x0:x1] + sub[..., :3] * sa)
    else:
        canvas[y0:y1, x0:x1] = sub[..., :3] * sa + canvas[y0:y1, x0:x1] * (1 - sa)


def draw_effect(canvas, sprites, effects, eid, age, x, y, galpha=255):
    """캔버스(H×W×3 float)에 효과 하나 — vendor 캔버스와 같은 규칙."""
    if eid < 0 or eid >= len(effects):
        return
    frames = effects[eid][1]
    if age < 0 or age >= len(frames):
        return
    H, W = canvas.shape[:2]
    for (s, dx, dy, a, fl) in frames[age]:
        arr, ax, ay = sprites[s]
        a = a * galpha // 255
        h, w = arr.shape[:2]
        if fl & RADIAL:
            _draw_lava_radial(canvas, arr, ax, ay, x, y, dx, dy, a, fl)
            continue
        src = arr[:, ::-1] if fl & FLIP else arr
        m = 2 if fl & X2 else 1
        ox = x + dx - m * (w - 1 - ax if fl & FLIP else ax)
        oy = y + dy - m * ay
        if m > 1:
            src = np.repeat(np.repeat(src, m, axis=0), m, axis=1)
            h, w = h * m, w * m
        x0, y0 = max(0, ox), max(0, oy)
        x1, y1 = min(W, ox + w), min(H, oy + h)
        if x0 >= x1 or y0 >= y1:
            continue
        sub = src[y0 - oy:y1 - oy, x0 - ox:x1 - ox].astype(np.float32)
        sa = sub[..., 3:4] / 255.0 * (a / 255.0)
        if fl & ADD:
            canvas[y0:y1, x0:x1] = np.minimum(255, canvas[y0:y1, x0:x1] + sub[..., :3] * sa)
        else:
            canvas[y0:y1, x0:x1] = sub[..., :3] * sa + canvas[y0:y1, x0:x1] * (1 - sa)


def check(path=OUT_BIN):
    sprites, effects, twins = read_bin_hd(path)
    assert len(effects) == len(EFFECTS), f"{len(effects)} effects, want {len(EFFECTS)}"
    for i, ((name, frames), (want, n)) in enumerate(zip(effects, EFFECTS)):
        assert name == want and len(frames) == n, f"effect {i}: {name}/{len(frames)} vs {want}/{n}"
    hd_px = sum(t[0].shape[0] * t[0].shape[1] for t in twins.values())
    print(f"ok: {len(effects)} effects · {len(sprites)} sprites · {len(twins)} HD twins ({hd_px:,} px) · {os.path.getsize(path):,} B")
    for k, v in CONTRACT.items():
        print(f"  {k} = {v}")
    for k, v in CONTRACT_U32.items():
        print(f"  {k} = {v}")


# ── 미리보기 ───────────────────────────────────────────────────────────────────────────────────────────────
def _background(w=176, h=150):
    return real_background()[:h, :w].astype(np.float32)


def _monster():
    return real_monster()


def _paste(canvas, img, x, y, alpha=1.0):
    a = np.asarray(img.convert("RGBA"), np.float32)
    h, w = a.shape[:2]
    H, W = canvas.shape[:2]
    x0, y0, x1, y1 = max(0, x), max(0, y), min(W, x + w), min(H, y + h)
    if x0 >= x1 or y0 >= y1:
        return
    sub = a[y0 - y:y1 - y, x0 - x:x1 - x]
    sa = sub[..., 3:4] / 255.0 * alpha
    canvas[y0:y1, x0:x1] = sub[..., :3] * sa + canvas[y0:y1, x0:x1] * (1 - sa)


def preview(out, zoom):
    sprites, effects = read_bin(OUT_BIN)
    os.makedirs(out, exist_ok=True)
    FXp, FYp = 88, 104
    mob = _monster()
    E = FX_ID
    scenes = []

    def scene(title, ground=(), top=(), pose=("stand", DOWN), mobs=(), lava=(), demon_on=True):
        c = _background()
        for (tx, ty, age, v) in lava:
            draw_effect(c, sprites, effects, E["lava0"] + v, age, tx, ty)
        for (eid, age, x, y) in ground:
            draw_effect(c, sprites, effects, eid, age, x, y)
        items = [(my, "mob", mx) for (mx, my) in mobs]
        if demon_on:
            items.append((FYp, "demon", FXp))
        for (yy, kind, xx) in sorted(items):
            if kind == "mob":
                _paste(c, mob[0], xx - mob[1][0], yy - mob[1][1])
            else:
                _paste(c, demon(*pose), xx + DEMON_OFF[0], yy + DEMON_OFF[1])
        for (tx, ty, age, v) in lava:
            draw_effect(c, sprites, effects, E["lava_top0"] + v, age, tx, ty)
        for (eid, age, x, y) in top:
            draw_effect(c, sprites, effects, eid, age, x, y)
        scenes.append((title, Image.fromarray(np.clip(c, 0, 255).astype(np.uint8))))

    R = RIGHT
    m1 = (FXp + 30, FYp - 4)
    scene("01 변신 출현", ground=[(E["transform_ground"], 26, FXp, FYp)], top=[(E["transform"], 26, FXp, FYp)], mobs=[(40, 70), (140, 50)])
    scene("변신 중 (사람 위)", ground=[(E["transform_ground"], 14, FXp, FYp)], top=[(E["transform"], 14, FXp, FYp)], demon_on=False)
    scene("02 할퀴기 1단", top=[(E["claw1_right"], 0, FXp, FYp)], pose=("c0_hitA", R), mobs=[m1])
    scene("03 할퀴기 2단", top=[(E["claw2_right"], 0, FXp, FYp)], pose=("c1_hit", R), mobs=[m1])
    scene("04 할퀴기 3단", top=[(E["claw3_right"], 0, FXp, FYp)], pose=("c2_hit", R), mobs=[m1])
    scene("할퀴기 1단 (아래)", top=[(E["claw1_down"], 0, FXp, FYp)], pose=("c0_hitA", DOWN))
    scene("05 돌진", ground=[(E["charge_ground_right"], 3, FXp, FYp)], top=[(E["charge_top_right"], 1, FXp, FYp)], pose=("walk1", R), mobs=[(FXp + 22, FYp)])
    scene("06 추종 100%", top=[(E["follow_dome"], 5, FXp, FYp)], mobs=[(30, 60), (150, 80)])
    scene("추종 게이지 75%", ground=[(E["follow_build3"], 5, FXp, FYp)])
    scene("07 추종 폭발", top=[(E["burst"], 4, FXp, FYp), (E["burst_mob"], 3, 150, 80)], pose=("roar", DOWN), mobs=[(150, 80)])
    blades = [(40, 70), (140, 60), (150, 100)]
    scene("08 지옥칼날", ground=[(E["seal"], 9, FXp, FYp)] + [(E["blade_rise0"] + (i & 1), 6 + i * 2, x, y) for i, (x, y) in enumerate(blades)], mobs=blades)
    scene("어둠심판 선딜", top=[(E["judge_orb"], 15, FXp, FYp)])
    scene("09 어둠심판", top=[(E["bolt0"], 0, 130, 90)], mobs=[(130, 90), (40, 60)])
    scene("멸화 선딜", ground=[(E["fire_ground"], 6, FXp, FYp)], top=[(E["fire_core"], 6, FXp, FYp)], pose=("roar", DOWN))
    scene("10 멸화", ground=[(E["fire_scorch"], 2, FXp, FYp)], top=[(E["fire_blast"], 6, FXp, FYp), (E["fire_mob"], 3, 150, 60)], mobs=[(150, 60), (30, 70)])
    ring = [(FXp // 16 + dx, FYp // 16 + dy) for dx, dy in [(-2, 0), (-2, -1), (-1, -2), (0, -2), (1, -2), (2, -1), (2, 0), (2, 1), (1, 2), (0, 2), (-1, 2), (-2, 1), (-1, 0), (1, 0), (0, 1), (0, -1), (-1, 1), (1, 1), (-1, -1), (1, -1)]]
    lava = lambda age: [(tx * 16 - 8, ty * 16 - 6, age, (tx & 3) + 4 * (ty & 3)) for (tx, ty) in ring]
    scene("11 용암불 연소", lava=lava(30))
    scene("12 용암불 냉각", lava=lava(85))
    scene("멸화 잔해", ground=[(E["fire_scorch"], 20, FXp, FYp)], top=[(E["fire_blast"], 16, FXp, FYp)])

    font = None
    try:
        from PIL import ImageFont
        font = ImageFont.truetype(os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf"), 16)
    except Exception:
        pass
    cols = 4
    tw, th = 176 * zoom, 150 * zoom
    rows = (len(scenes) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (tw + 8) + 8, rows * (th + 30) + 8), (20, 20, 26))
    d = ImageDraw.Draw(sheet)
    for i, (title, im) in enumerate(scenes):
        x, y = 8 + (i % cols) * (tw + 8), 8 + (i // cols) * (th + 30)
        sheet.paste(im.resize((tw, th), Image.NEAREST), (x, y + 22))
        d.text((x, y), title, fill=(230, 230, 230), font=font)
        im.resize((tw, th), Image.NEAREST).save(os.path.join(out, f"scene_{i:02d}.png"))
    sheet.save(os.path.join(out, "sheet.png"))
    print(f"preview: {len(scenes)} scenes → {out}/sheet.png")


# ── 예상도 대조 (2 차) ─────────────────────────────────────────────────────────────────────────────────────
APPLIED = os.path.join(ROOT, "docs", "skill", "astra_demon", "applied")
MOCK_FILES = ["01_transform_claw.png", "02_charge_dark.png", "03_judgment_fire_lava.png"]
MOCK_BOXES = [(22, 60, 548, 660), (573, 60, 1098, 660), (22, 719, 548, 1319), (573, 719, 1098, 1319)]


def real_background():
    """실제 게임 캡처(applied/ 의 숲 장면 11 장)의 중앙값 = 이펙트 · 몬스터 없는 화면 + 가운데 겹친 캐릭터 자리를 풀 조각으로."""
    import glob
    fs = [f for f in sorted(glob.glob(os.path.join(APPLIED, "[01]*_*.png"))) if "charge" not in f]
    A = np.stack([np.asarray(Image.open(f).convert("RGB")) for f in fs])
    m = np.median(A, axis=0).astype(np.uint8)
    tiles = [m[68:92, 24:48].copy(), m[72:96, 116:140].copy(), m[152:176, 24:48].copy(), m[148:172, 104:128].copy()]
    k = 0
    for y in range(72, 146, 24):
        for x in range(52, 104, 24):
            t = tiles[k % 4]
            k += 1
            if (x // 24) % 2:
                t = t[:, ::-1]
            if (y // 24) % 2:
                t = t[::-1]
            h, w = min(24, 146 - y), min(24, 104 - x)
            m[y:y + h, x:x + w] = t[:h, :w]
    return m


def real_monster():
    """반란전투병 한 마리 (게임 해상도, 캡처 08 에서 배경을 빼서) → (RGBA, 발 기준점)."""
    bg = None
    import glob
    fs = [f for f in sorted(glob.glob(os.path.join(APPLIED, "[01]*_*.png"))) if "charge" not in f]
    bg = np.median(np.stack([np.asarray(Image.open(f).convert("RGB")) for f in fs]), axis=0).astype(int)
    a = np.asarray(Image.open(os.path.join(APPLIED, "08_judge_777.png")).convert("RGB")).astype(int)
    x0, y0, x1, y1 = 106, 46, 131, 77
    sub = a[y0:y1, x0:x1]
    d = np.abs(sub - bg[y0:y1, x0:x1]).sum(axis=2) > 40
    for r in range(d.shape[0]):  # 줄마다 처음 ~ 끝 사이를 메운다 (몸 안 배경과 같은 색 픽셀)
        xs = np.nonzero(d[r])[0]
        if len(xs):
            d[r, xs[0]:xs[-1] + 1] = True
    rgba = np.dstack([sub, d * 255]).astype(np.uint8)
    return Image.fromarray(rgba, "RGBA"), (118 - x0, 75 - y0)


def mockup_panel(i):
    """예상도 12 장면 중 i 번(0..11)을 게임 폭 176 으로 줄인 그림."""
    m = Image.open(os.path.join(ROOT, "docs", "skill", "astra_demon", "ingame", MOCK_FILES[i // 4])).convert("RGB").crop(MOCK_BOXES[i % 4])
    w, h = m.size
    return m.resize((176, round(h * 176 / w)), Image.LANCZOS)


def _scene_tiles(feet, dxs):
    """발이 칸 가운데인 화면에서 (dx, dy) 칸들의 용암 (칸 왼쪽 위, 월드 정렬 변형)."""
    fx, fy = feet
    wx, wy = 20, 28  # 악마 칸의 월드 좌표 (변형 번호만 정한다)
    out = []
    for dx, dy in dxs:
        tx, ty = wx + dx, wy + dy
        out.append((fx - 8 + dx * 16, fy - 10 + dy * 16, (tx & 3) + 4 * (ty & 3)))
    return out


def compare_scenes():
    """예상도 12 장면과 같은 배치 — 악마 · 몬스터의 발바닥 = `astra_cut.FEET` · `MOBS`(초안에서 잰 자리)."""
    import astra_cut as ac
    E = FX_ID

    def mobs(i):
        return [((x0 + x1) // 2, y1 - 2) for (x0, y0, x1, y1) in ac.MOBS[i]]
    F = ac.FEET
    lava_field = [(dx, dy) for dy in range(-2, 3) for dx in range(-4, 5) if (dx / 4.6) ** 2 + (dy / 2.2) ** 2 <= 1.0]
    S = []
    S.append(dict(t="변신 출현", m=0, demon=(*F[0], "stand", DOWN), mobs=mobs(0),
                  ground=[(E["transform_ground"], 26, *F[0])], top=[(E["transform"], 26, *F[0])]))
    for k, (pose, cut) in enumerate((("c0_hitA", 1), ("c1_hit", 2), ("c2_hit", 3))):
        i = 1 + k
        S.append(dict(t=f"할퀴기 {k + 1}단", m=i, demon=(*F[i], pose, RIGHT), mobs=mobs(i), top=[(E[f"claw{cut}_right"], 0, *F[i])]))
    S.append(dict(t="돌진", m=4, demon=(*F[4], "walk1", RIGHT), mobs=mobs(4),
                  ground=[(E["charge_ground_right"], 0, *F[4])], top=[(E["charge_top_right"], 0, *F[4])]))
    S.append(dict(t="추종 100%", m=5, demon=(*F[5], "stand", DOWN), mobs=mobs(5), top=[(E["follow_dome"], 0, *F[5])]))
    S.append(dict(t="추종 폭발", m=6, demon=(*F[6], "roar", DOWN), mobs=mobs(6),
                  top=[(E["burst"], 5, *F[6])] + [(E["burst_mob"], 3, x, y) for (x, y) in mobs(6)]))
    blades = mobs(7)
    S.append(dict(t="지옥칼날", m=7, demon=(*F[7], "stand", DOWN), mobs=blades,
                  ground=[(E["seal"], 0, *F[7])] + [(E["blade_rise0"] + (i & 1), 6, x, y) for i, (x, y) in enumerate(blades)]))
    S.append(dict(t="어둠심판", m=8, demon=(*F[8], "roar", RIGHT), mobs=mobs(8), top=[(E["bolt0"], 1, 118, 125)]))
    S.append(dict(t="멸화", m=9, demon=(*F[9], "roar", DOWN), mobs=mobs(9),
                  ground=[(E["fire_scorch"], 6, *F[9])], top=[(E["fire_blast"], 6, *F[9])] + [(E["fire_mob"], 4, x, y) for (x, y) in mobs(9)]))
    S.append(dict(t="용암불 연소", m=10, demon=(*F[10], "stand", RIGHT), mobs=mobs(10), lava=(_scene_tiles(F[10], lava_field), 30)))
    S.append(dict(t="용암불 냉각", m=11, demon=(*F[11], "stand", RIGHT), mobs=mobs(11), lava=(_scene_tiles(F[11], lava_field), 85)))
    return S


def render_scene(sc, sprites, effects, bg, mob):
    c = bg.astype(np.float32).copy()
    E = FX_ID
    mob_img, (mx0, my0) = mob
    lava = sc.get("lava")
    if lava:
        tiles, age = lava
        for (x, y, v) in tiles:
            draw_effect(c, sprites, effects, E["lava0"] + v, age, x, y)
    for (eid, age, x, y) in sc.get("ground", []):
        draw_effect(c, sprites, effects, eid, age, x, y)
    items = [(y, 0, x, None) for (x, y) in sc.get("mobs", [])]
    if sc.get("demon"):
        x, y, pose, view = sc["demon"]
        items.append((y, 1, x, (pose, view)))
    for (y, kind, x, pv) in sorted(items):
        if kind == 0:
            _paste(c, mob_img, x - mx0, y - my0)
        else:
            _paste(c, demon(*pv), x + DEMON_OFF[0], y + DEMON_OFF[1])
    if lava:
        tiles, age = lava
        for (x, y, v) in tiles:
            draw_effect(c, sprites, effects, E["lava_top0"] + v, age, x, y)
    for (eid, age, x, y) in sc.get("top", []):
        draw_effect(c, sprites, effects, eid, age, x, y)
    return Image.fromarray(np.clip(c, 0, 255).astype(np.uint8))


def compare(out, zoom, path=OUT_BIN, only=None):
    """예상도(176 폭으로 줄임) | 새 라이브러리로 그린 같은 배치 — 장면마다 나란히, 그리고 한 장에 전부."""
    sprites, effects = read_bin(path)
    os.makedirs(out, exist_ok=True)
    bg = real_background()
    mob = real_monster()
    from PIL import ImageFont
    try:
        font = ImageFont.truetype(os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf"), 16)
    except Exception:
        font = None
    pairs = []
    for i, sc in enumerate(compare_scenes()):
        if only is not None and i not in only:
            continue
        mk = mockup_panel(sc["m"])
        im = render_scene(sc, sprites, effects, bg, mob).crop((0, 0, 176, mk.height))
        pair = Image.new("RGB", (176 * 2 + 4, mk.height), (20, 20, 26))
        pair.paste(mk, (0, 0))
        pair.paste(im, (180, 0))
        pair = pair.resize((pair.width * zoom, pair.height * zoom), Image.NEAREST)
        pair.save(os.path.join(out, f"cmp_{i:02d}.png"))
        pairs.append((sc["t"], pair))
    cols = 2
    pw, ph = pairs[0][1].size
    rows = (len(pairs) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (pw + 10) + 10, rows * (ph + 30) + 10), (20, 20, 26))
    d = ImageDraw.Draw(sheet)
    for k, (t, p) in enumerate(pairs):
        x, y = 10 + (k % cols) * (pw + 10), 10 + (k // cols) * (ph + 30)
        d.text((x, y), f"{t}  — 왼쪽 예상도 / 오른쪽 라이브러리", fill=(230, 230, 230), font=font)
        sheet.paste(p, (x, y + 22))
    sheet.save(os.path.join(out, "compare.png"))
    print(f"compare: {len(pairs)} scenes → {out}/compare.png")


FOOTPRINT_GROUPS = [
    ("변신 (위 층)", ["transform"]), ("변신 (땅 층)", ["transform_ground"]),
    ("할퀴기 1단 첫 손 (오른쪽)", ["claw0_right"]), ("할퀴기 1단 X (오른쪽)", ["claw1_right"]), ("할퀴기 2단 (오른쪽)", ["claw2_right"]),
    ("할퀴기 3단 (오른쪽)", ["claw3_right"]), ("할퀴기 1단 X (아래)", ["claw1_down"]), ("할퀴기 1단 X (위)", ["claw1_up"]),
    ("돌진 (오른쪽, 두 층)", ["charge_ground_right", "charge_top_right"]), ("추종 게이지 75 %", ["follow_build3"]),
    ("추종 100 % 반구", ["follow_dome"]), ("추종 폭발", ["burst"]), ("추종 폭발 — 맞은 몬스터", ["burst_mob"]),
    ("지옥칼날 인장", ["seal"]), ("지옥칼날 낫날 (몬스터)", ["blade_rise0", "blade_rise1"]), ("어둠심판 선딜 구체", ["judge_orb"]),
    ("어둠심판 낙뢰 착탄", ["bolt0", "bolt1", "bolt2", "bolt3"]), ("멸화 선딜", ["fire_core", "fire_ground"]),
    ("멸화 폭발 (두 층)", ["fire_blast", "fire_back", "fire_scorch"]), ("멸화 — 맞은 몬스터", ["fire_mob"]),
    ("용암 한 칸 (두 층)", ["lava5", "lava_top5"]),
]


def footprint(path=OUT_BIN):
    """효과마다 모든 프레임의 그림이 닿는 상자 (기준점 = 발바닥 · 용암은 칸 왼쪽 위) — 판정 범위를 그림에 맞출 때의 기준.
    「전체」= 옅은 빛 · 튀는 조각까지 (알파 40+), 「몸통」= 덮기 그림의 진한 부분만 (알파 160+, 더하기 빛 제외)."""
    sprites, effects = read_bin(path)
    by = {name: frames for name, frames in effects}
    rows = []
    for title, names in FOOTPRINT_GROUPS:
        boxes = []
        for thr, base_only in ((40, False), (160, True)):
            x0 = y0 = 10 ** 9
            x1 = y1 = -10 ** 9
            for n in names:
                for parts in by[n]:
                    for (s_, dx, dy, a, fl) in parts:
                        if base_only and fl & ADD:
                            continue
                        arr, ax, ay = sprites[s_]
                        h, w = arr.shape[:2]
                        lx = dx - (w - 1 - ax if fl & FLIP else ax)
                        ly = dy - ay
                        ys, xs = np.nonzero(arr[..., 3].astype(int) * a // 255 > thr)
                        if len(xs) == 0:
                            continue
                        if fl & FLIP:
                            xs = w - 1 - xs
                        x0, x1 = min(x0, lx + xs.min()), max(x1, lx + xs.max() + 1)
                        y0, y1 = min(y0, ly + ys.min()), max(y1, ly + ys.max() + 1)
            boxes.append((x0, y0, x1, y1))
        rows.append((title, boxes))
    print("| 효과 | 전체 x · y 범위 (px) | 전체 폭 × 높이 (칸) | 몸통 x · y 범위 (px) | 몸통 폭 × 높이 (칸) |")
    print("|---|---|---|---|---|")
    for t, ((a0, b0, a1, b1), (c0, d0, c1, d1)) in rows:
        print(f"| {t} | {a0}..{a1} · {b0}..{b1} | {(a1 - a0) / 16:.1f} × {(b1 - b0) / 16:.1f} | {c0}..{c1} · {d0}..{d1} | "
              f"{(c1 - c0) / 16:.1f} × {(d1 - d0) / 16:.1f} |")
    return rows


TIMELINE = [  # (제목, [(층, 효과, 나이 줄)], 악마 자세, 몬스터 자리)
    ("변신", [("g", "transform_ground", (14, 22, 24, 25, 26, 30, 36, 42)), ("t", "transform", (14, 22, 24, 25, 26, 30, 36, 42))], ("stand", DOWN), ()),
    ("할퀴기 1단 첫 손", [("t", "claw0_right", (0, 1, 2, 3, 4, 5))], ("c0_hitA", RIGHT), ((116, 0),)),
    ("할퀴기 1단 X", [("t", "claw1_right", (0, 1, 2, 3, 4, 5))], ("c0_hitB", RIGHT), ((116, 0),)),
    ("할퀴기 2단", [("t", "claw2_right", (0, 1, 2, 3, 4, 5))], ("c1_hit", RIGHT), ((116, 0),)),
    ("할퀴기 3단", [("t", "claw3_right", (0, 1, 2, 3, 4, 5))], ("c2_hit", RIGHT), ((116, 0),)),
    ("할퀴기 1단 X (아래)", [("t", "claw1_down", (0, 1, 2, 3, 4, 5))], ("c0_hitB", DOWN), ()),
    ("할퀴기 2단 (위)", [("t", "claw2_up", (0, 1, 2, 3, 4, 5))], ("c1_hit", UP), ()),
    ("돌진", [("g", "charge_ground_right", (0, 1, 2, 3, 4, 5, 6, 7)), ("t", "charge_top_right", (0, 1, 2, 3, 4, 5, 6, 7))], ("walk1", RIGHT), ()),
    ("추종 100%", [("t", "follow_dome", (0, 2, 4, 6, 8, 10, 12, 14))], ("stand", DOWN), ()),
    ("추종 폭발", [("t", "burst", (0, 1, 2, 3, 5, 9, 12, 15, 18))], ("roar", DOWN), ()),
    ("지옥칼날 인장", [("g", "seal", (0, 4, 8, 12, 16, 20, 24, 28))], ("stand", DOWN), ()),
    ("지옥칼날 낫날", [("g", "blade_rise0", (0, 1, 2, 3, 6, 10, 12, 14))], None, ((0, 0),)),
    ("어둠심판 낙뢰 (변형 0 · 2)", [("t", "bolt0", (0, 1, 2, 3, -1, -1, -1, -1)), ("t", "bolt2", (-1, -1, -1, -1, 0, 1, 2, 3))], None, ((0, 0),)),
    ("멸화", [("g", "fire_scorch", (0, 1, 2, 3, 6, 10, 14, 18, 22, 30)), ("g", "fire_back", (0, 1, 2, 3, 6, 10, 14, 18, 22, 23)),
             ("t", "fire_blast", (0, 1, 2, 3, 6, 10, 14, 18, 22, 23))], ("roar", DOWN), ()),
]


def timeline(out, zoom):
    """효과마다 몇 프레임을 실제 게임 배경 위에 나란히 (애니메이션 흐름 확인)."""
    sprites, effects = read_bin(OUT_BIN)
    bg = real_background()
    mob = real_monster()
    W, H, fx, fy = 176, 150, 88, 118
    rows = []
    for title, layers, pose, mobs in TIMELINE:
        n = max(len(lay[2]) for lay in layers)
        cells = []
        for i in range(n):
            c = bg[:H, :W].astype(np.float32).copy()
            for (lay, name, ages) in layers:
                if lay == "g" and i < len(ages):
                    draw_effect(c, sprites, effects, FX_ID[name], ages[i], fx, fy)
            for (dx, dy) in mobs:
                if pose is None:
                    _paste(c, mob[0], fx + dx - mob[1][0], fy + dy - mob[1][1])
                else:
                    _paste(c, mob[0], fx + 44 - mob[1][0], fy - 2 - mob[1][1])
            if pose is not None:
                _paste(c, demon(*pose), fx + DEMON_OFF[0], fy + DEMON_OFF[1])
            for (lay, name, ages) in layers:
                if lay == "t" and i < len(ages):
                    draw_effect(c, sprites, effects, FX_ID[name], ages[i], fx, fy)
            cells.append(np.clip(c, 0, 255).astype(np.uint8))
        rows.append((title, np.concatenate(cells, axis=1)))
    wmax = max(r.shape[1] for _, r in rows)
    sheet = Image.new("RGB", (wmax * zoom + 20, sum(H * zoom + 30 for _ in rows) + 10), (20, 20, 26))
    from PIL import ImageFont
    try:
        font = ImageFont.truetype(os.path.join(ROOT, "vendor", "wie", "fonts", "neodgm.ttf"), 16)
    except Exception:
        font = None
    d = ImageDraw.Draw(sheet)
    y = 10
    for title, r in rows:
        d.text((10, y), title, fill=(230, 230, 230), font=font)
        im = Image.fromarray(r)
        sheet.paste(im.resize((im.width * zoom, im.height * zoom), Image.NEAREST), (10, y + 22))
        y += H * zoom + 30
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    sheet.save(out)
    print(f"timeline → {out}")


def strip(names, out, zoom):
    sprites, effects = read_bin(OUT_BIN)
    rows = []
    for name in names:
        eid = FX_ID[name]
        n = len(effects[eid][1])
        W, H = 150, 150
        cells = []
        for age in range(n):
            c = np.zeros((H, W, 3), np.float32)
            c[...] = (46, 82, 40)
            c[::8, :] = (52, 90, 46)
            draw_effect(c, sprites, effects, eid, age, W // 2, H - 40)
            c[H - 40, :] += 40
            c[:, W // 2] += 40
            cells.append(np.clip(c, 0, 255).astype(np.uint8))
        rows.append(np.concatenate(cells, axis=1))
    w = max(r.shape[1] for r in rows)
    img = np.zeros((sum(r.shape[0] + 4 for r in rows), w, 3), np.uint8)
    y = 0
    for r in rows:
        img[y:y + r.shape[0], :r.shape[1]] = r
        y += r.shape[0] + 4
    Image.fromarray(img).resize((img.shape[1] * zoom, img.shape[0] * zoom), Image.NEAREST).save(out)
    print(f"strip → {out}")


# ★ 2026-10-14 밤 (4): 동작 미리보기 — 게임 박자(20 fps, 한 그리기 = 한 게임 프레임)로 서기 → 걷기 → 할퀴기 세 단 → 서기.
#   악마 자세 · 몸 움직임은 시퀀스와 같은 규칙(`demon_look.pose_for` · `motion`), 할퀴기 이펙트는 타격 프레임에서 시작해 6 그리기
#   (호스트 CLAW 신호 + 그리기 나이 — 다음 단으로 넘어가도 이어진다). 단 길이는 직업의 사람 값(나이트 4 · 4 · 4 · 로그 4 · 5 · 3).
CLAW_FX = {(4, 1): "claw0", (4, 3): "claw1", (5, 2): "claw2", (6, 2): "claw3"}  # (시퀀스, 타격 프레임) → 할퀴기 이펙트
STAGE_LENGTHS = {"knight": (4, 4, 4), "rogue": (4, 5, 3)}


def motion_frames(view, lengths=(4, 4, 4), cls=1):
    """[(자세, (dx, dy), [(이펙트, 나이)])] — 한 게임 프레임씩."""
    steps = [("stand", None, 0)] * 6 + [(None, 2, f) for f in range(8)]
    for k, n in enumerate(lengths):
        steps += [(None, 4 + k, f) for f in range(n)]
    steps += [("stand", None, 0)] * 10
    out, live = [], []
    for pose, seq, f in steps:
        if pose is None:
            pose = _dl.pose_for(cls, seq, f)
            hit = CLAW_FX.get((seq, f))
            if hit:
                live.append([f"{hit}_{DIR_NAMES[view]}", 0])
        fx = [(name, age) for name, age in live if age < CLAW_LEN]
        out.append((pose, _dl.motion(pose, view), fx))
        for e in live:
            e[1] += 1
    return out


def motion(out_dir, zoom=3, job="knight"):
    """방향 넷 × GIF 느리게(160 ms) + 옆 · 앞은 게임 속도(50 ms) GIF + 할퀴기 구간 PNG 띠 → out_dir."""
    sprites, effects = read_bin(OUT_BIN)
    bg = real_background()
    mob = real_monster()
    W, H, fx, fy = 176, 150, 88, 100
    ahead = {RIGHT: (34, 0), LEFT: (-34, 0), DOWN: (0, 30), UP: (0, -30)}
    os.makedirs(out_dir, exist_ok=True)
    strips = []
    for view in (RIGHT, DOWN, LEFT, UP):
        frames = []
        for pose, (mx, my), fxs in motion_frames(view, STAGE_LENGTHS[job]):
            c = bg[:H, :W].astype(np.float32).copy()
            ax, ay = ahead[view]
            items = [(fy + ay, "mob"), (fy + my, "demon")]
            for _y, what in sorted(items):
                if what == "mob":
                    _paste(c, mob[0], fx + ax - mob[1][0], fy + ay - mob[1][1])
                else:
                    _paste(c, demon(pose, view), fx + DEMON_OFF[0] + mx, fy + DEMON_OFF[1] + my)
            for name, age in fxs:
                draw_effect(c, sprites, effects, FX_ID[name], age, fx, fy)
            frames.append(Image.fromarray(np.clip(c, 0, 255).astype(np.uint8)).resize((W * zoom, H * zoom), Image.NEAREST))
        tag = DIR_NAMES[view]
        if view in (RIGHT, DOWN):  # 게임 속도는 옆 · 앞만 (왼쪽 = 거울 · 뒷모습은 느린 판으로 충분)
            frames[0].save(os.path.join(out_dir, f"motion_{tag}.gif"), save_all=True, append_images=frames[1:], duration=50, loop=0)
        frames[0].save(os.path.join(out_dir, f"motion_{tag}_slow.gif"), save_all=True, append_images=frames[1:], duration=160, loop=0)
        strips.append(frames)
    # 띠: 할퀴기 구간만(서기 · 걷기 뒤) 방향마다 한 줄
    first = 6 + 8
    n = sum(STAGE_LENGTHS[job]) + 2
    cw, ch = W * zoom // 2, H * zoom // 2
    sheet = Image.new("RGB", (n * (cw + 4), len(strips) * (ch + 4)), (20, 20, 26))
    for r, frames in enumerate(strips):
        for i in range(n):
            sheet.paste(frames[first + i].resize((cw, ch), Image.NEAREST), (i * (cw + 4), r * (ch + 4)))
    sheet.save(os.path.join(out_dir, "motion_claw_strip.png"))
    print(f"motion → {out_dir} ({job} {STAGE_LENGTHS[job]})")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("build")
    p.add_argument("--out", default=OUT_BIN)
    p.add_argument("--no-hd", action="store_true", help="1x only (no twins; the old v1 library)")
    p = sub.add_parser("preview")
    p.add_argument("--out", default=os.path.join(ROOT, "docs", "skill", "demon_fx"))
    p.add_argument("--zoom", type=int, default=3)
    p = sub.add_parser("check")
    p.add_argument("--path", default=OUT_BIN)
    p = sub.add_parser("timeline")
    p.add_argument("--out", required=True)
    p.add_argument("--zoom", type=int, default=2)
    p = sub.add_parser("footprint")
    p.add_argument("--path", default=OUT_BIN)
    p = sub.add_parser("compare")
    p.add_argument("--out", required=True)
    p.add_argument("--zoom", type=int, default=2)
    p.add_argument("--path", default=OUT_BIN)
    p.add_argument("--only", type=int, nargs="*")
    p = sub.add_parser("motion")
    p.add_argument("--out", required=True)
    p.add_argument("--zoom", type=int, default=3)
    p.add_argument("--job", choices=sorted(STAGE_LENGTHS), default="knight")
    p = sub.add_parser("strip")
    p.add_argument("names", nargs="+")
    p.add_argument("--out", required=True)
    p.add_argument("--zoom", type=int, default=2)
    a = ap.parse_args()
    if a.cmd == "build":
        lib = build(hd=not a.no_hd)
        size, shape, n, hd_shape, twins = lib.write(a.out)
        print(f"{a.out}: {size:,} B · atlas {shape[1]}×{shape[0]} · {n} sprites · {len(lib.effects)} effects · HD twins {twins}"
              + (f" (atlas {hd_shape[1]}×{hd_shape[0]})" if hd_shape else ""))
        check(a.out)
    elif a.cmd == "preview":
        preview(a.out, a.zoom)
    elif a.cmd == "timeline":
        timeline(a.out, a.zoom)
    elif a.cmd == "footprint":
        footprint(a.path)
    elif a.cmd == "compare":
        compare(a.out, a.zoom, a.path, set(a.only) if a.only else None)
    elif a.cmd == "strip":
        strip(a.names, a.out, a.zoom)
    elif a.cmd == "motion":
        motion(a.out, a.zoom, a.job)
    else:
        check(a.path)


if __name__ == "__main__":
    main()
