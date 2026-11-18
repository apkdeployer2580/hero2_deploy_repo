#!/usr/bin/env python3
"""★★ 2026-10-16 악의 무기 (HERO2_DEMON_WEAPON_DRAFT.md) — 할퀴기의 새 효과 두 가족을 `wie_android/src/demon_fx.bin` 뒤에 붙인다.

* `claw{c}_spin` (M2 피바람 도끼 — 할퀴기 1·2 단이 사방을 할퀸다): 같은 나이의 `claw{c}_up/down/left/right` 부품을 한 프레임에 —
  새 스프라이트 없이 네 방향 그림이 한꺼번에 (판정 = 네 방향 창의 합, `DemonPatch::SPIN_STAGES`).
* `claw{c}_{d}_big` (M3 타락한 성검 — 신의가호 중 할퀴기 범위 +1 칸): `claw{c}_{d}` 의 스프라이트 · HD 쌍 · 부품 위치를 기준점을 중심으로
  `SCALE` 배 (= `DemonPatch::CLAW_BIG_SCALE` / 100) — 1× 는 미리곱한 알파로 LANCZOS, HD 쌍은 같은 방법으로 키운 뒤 그 쌍의 팔레트로
  다시 색인(가장 가까운 색 · 투명 포함)이라 HD 그림의 색은 그대로다.

다른 효과는 1× 스프라이트 · HD 쌍의 색인 · 팔레트가 바이트 그대로 옮겨진다(`fire_fx.read_raw` → `Library.sprite_q`, `fx_swap.replace` 의 꼴).
효과 번호 · 이름 · 길이는 `demon_fx.EFFECTS` 의 계약(뒤에 붙음 — `DemonPatch::FX_CLAW_SPIN` · `FX_CLAW_BIG` · `FX_COUNT`).

    python tools_re/demon_weapon_fx.py              # bin 뒤에 붙이기 (이미 붙어 있으면 다시 만든다)
    python tools_re/demon_weapon_fx.py preview      # docs/skill/demon_weapons/ — 사방 · 큰 할퀴기 장면 시트
"""

import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCALE = 1.35
CLAW_LEN = 6  # = demon_fx.CLAW_LEN (checked at build)
DIRS = ("up", "down", "left", "right")
# ★★ 2026-10-21 야수조련 목걸이. `wie_backend::canvas::FX_FROM_CHEST` (bit 7): a scale-2 draw grows about the chest,
# 20px above the feet, so the swing still leaves the body. The tile-stretch flag is not set.
FROM_CHEST = 128
# bits 4..5 — `wie_backend::canvas::FX_CLAW_UP` / `FX_CLAW_DOWN`. Side claws stay 0 (they grow about the chest).
DIR_FLAG = {"up": 16, "down": 32, "left": 0, "right": 0}


def effect_names():
    return [(f"claw{c}_spin", CLAW_LEN) for c in range(4)] + [(f"claw{c}_{d}_big", CLAW_LEN) for c in range(4) for d in DIRS]


def _resize_premul(rgba, nw, nh):
    """Straight RGBA (float 0..1) → premultiplied LANCZOS → straight RGBA (float)."""
    a = rgba[..., 3:4]
    pm = np.concatenate([rgba[..., :3] * a, a], -1)
    chans = [np.asarray(Image.fromarray(pm[..., k].astype(np.float32), "F").resize((nw, nh), Image.LANCZOS)) for k in range(4)]
    out = np.clip(np.stack(chans, -1), 0.0, 1.0)
    al = out[..., 3:4]
    rgb = np.where(al > 1e-4, out[..., :3] / np.maximum(al, 1e-4), 0.0)
    return np.concatenate([np.clip(rgb, 0, 1), al], -1)


def scale_sprite(arr, ax, ay, s=SCALE):
    """1× sprite (uint8 RGBA, anchor) → the same about its anchor × s."""
    h, w = arr.shape[:2]
    nw, nh = max(1, round(w * s)), max(1, round(h * s))
    out = _resize_premul(arr.astype(np.float32) / 255.0, nw, nh)
    o = np.round(out * 255).astype(np.uint8)
    o[o[..., 3] < 2] = 0
    return o, int(round(ax * s)), int(round(ay * s))


def scale_twin(qtwin, s=SCALE):
    """HD twin (idx HxW, pal n×4 for indices 1..=n, ax, ay, scale) → the same × s, re-indexed to its own palette (0 = transparent)."""
    idx, pal, ax, ay, scale = qtwin
    # the palette holds indices 1..=n (index 0 = transparent, not stored)
    p = np.concatenate([np.zeros((1, 4), np.float32), pal.astype(np.float32) / 255.0], 0)
    rgba = p[idx]
    h, w = idx.shape
    nw, nh = max(1, round(w * s)), max(1, round(h * s))
    out = _resize_premul(rgba, nw, nh)
    pm_out = np.concatenate([out[..., :3] * out[..., 3:4], out[..., 3:4]], -1).reshape(-1, 4)
    pm_pal = np.concatenate([p[:, :3] * p[:, 3:4], p[:, 3:4]], -1)
    best = np.zeros(pm_out.shape[0], np.uint8)
    for i in range(0, pm_out.shape[0], 8192):
        d = ((pm_out[i:i + 8192, None, :] - pm_pal[None, :, :]) ** 2).sum(-1)
        best[i:i + 8192] = d.argmin(1).astype(np.uint8)
    return best.reshape(nh, nw), pal.copy(), int(round(ax * s)), int(round(ay * s)), scale


def add_effects(lib):
    """The twenty after `fire_back`, from the claws already in `lib` (a full `demon_fx.build` or the kept bin of `build`)."""
    by = {name: frames for name, frames in lib.effects}
    big = {}

    def bigger(s):
        if s not in big:
            arr, ax, ay = lib.sprites[s]
            if s in lib.qtwins:
                big[s] = lib.sprite_q(scale_sprite(arr, ax, ay), scale_twin(lib.qtwins[s]))
            elif s in lib.twins:
                tarr, tax, tay, tscale = lib.twins[s]
                h, w = tarr.shape[:2]
                t = _resize_premul(tarr.astype(np.float32) / 255.0, max(1, round(w * SCALE)), max(1, round(h * SCALE)))
                t8 = np.round(t * 255).astype(np.uint8)
                t8[t8[..., 3] < 2] = 0
                big[s] = lib.sprite(scale_sprite(arr, ax, ay), (t8, int(round(tax * SCALE)), int(round(tay * SCALE)), tscale))
            else:
                big[s] = lib.sprite(scale_sprite(arr, ax, ay))
        return big[s]

    for name, n in effect_names():
        assert n == CLAW_LEN
        if name.endswith("_spin"):
            c = name[len("claw"):-len("_spin")]
            frames = [[(s, dx, dy, a, (fl & ~0x38) | FROM_CHEST | DIR_FLAG[d])
                       for d in DIRS for (s, dx, dy, a, fl) in by[f"claw{c}_{d}"][f]] for f in range(CLAW_LEN)]
        else:
            plain = name[: -len("_big")]
            dflag = next(v for k, v in DIR_FLAG.items() if f"_{k}" in plain)
            frames = [[(bigger(s), int(round(dx * SCALE)), int(round(dy * SCALE)), a, (fl & ~0x38) | FROM_CHEST | dflag) for (s, dx, dy, a, fl) in by[plain][f]] for f in range(CLAW_LEN)]
        lib.add_effect(name, frames)


def build(src=None, out=None):
    """The bin's other effects byte for byte (`fire_fx.read_raw` → `Library.sprite_q`), ours appended (or made again)."""
    import demon_fx
    import fire_fx
    assert demon_fx.CLAW_LEN == CLAW_LEN
    src = src or demon_fx.OUT_BIN
    out = out or demon_fx.OUT_BIN
    sprites, effects, qtwins = fire_fx.read_raw(src)
    names = [n for n, _ in effects]
    ours = [n for n, _ in effect_names()]
    base = [n for n, _ in demon_fx.EFFECTS if n not in ours]
    missing = [n for n in base if n not in names]
    assert not missing, f"bin 에 없는 효과: {missing}"
    lib = demon_fx.Library(hd=True)
    remap = {}
    by = dict(effects)

    def keep(s):
        if s not in remap:
            arr, ax, ay = sprites[s]
            remap[s] = lib.sprite_q((np.ascontiguousarray(arr), ax, ay), qtwins.get(s))
        return remap[s]

    # ★★ 2026-10-18 the families after ours (lava_wide_fx) are kept at their place too
    for name, _ in demon_fx.EFFECTS:
        if name in ours:
            if name == ours[0]:
                add_effects(lib)
            continue
        dflag = next((v for k, v in DIR_FLAG.items() if f"_{k}" in name), 0) if name.startswith("claw") else 0
        lib.add_effect(name, [[(keep(s), dx, dy, a, ((fl & ~0x38) | FROM_CHEST | dflag) if name.startswith("claw") else fl) for (s, dx, dy, a, fl) in parts] for parts in by[name]])
    size, shape, n, hd_shape, twins = lib.write(out)
    print(f"{out}: {size:,} B · atlas {shape[1]}×{shape[0]} · {n} sprites · {len(lib.effects)} effects · HD twins {twins}"
          + (f" (atlas {hd_shape[1]}×{hd_shape[0]})" if hd_shape else "") + f" · appended: {len(ours)} (spin 4 · big 16, × {SCALE})")


def preview(out=None):
    import fx_swap
    out = out or os.path.join(ROOT, "docs", "skill", "demon_weapons")
    FX, FY = 88, 104
    seq = []
    for c, t in ((0, "1단 첫 손"), (1, "1단 X"), (2, "2단")):
        for age in (1, 3):
            seq.append((f"사방 할퀴기 {t} · {age}", [("t", f"claw{c}_spin", age, FX, FY)]))
    for c, t in ((1, "1단 X"), (2, "2단"), (3, "3단")):
        for age in (1, 3):
            seq.append((f"할퀴기 {t} · {age}", [("t", f"claw{c}_right", age, FX, FY)]))
            seq.append((f"큰 할퀴기 {t} · {age}", [("t", f"claw{c}_right_big", age, FX, FY)]))
    fx_swap.preview_scenes(out, "demon_weapon_claws", seq, demon_pose=("c0_hitA", 3), cols=4)


if __name__ == "__main__":
    sys.stdout.reconfigure(errors="replace")
    if len(sys.argv) > 1 and sys.argv[1] == "preview":
        preview(sys.argv[2] if len(sys.argv) > 2 else None)
    else:
        build()
