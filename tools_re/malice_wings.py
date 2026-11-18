"""악의 = 클레르 자신의 모습(아바타 포함) + 등 뒤의 검은 날개 — 날개 효과와 네 할퀴기 참격 (`costumes/malice_wings`).

★★ 2026-10-22 (사용자: *"악마로 변신한 외형 모습은 완전히 버리고 … 클레르의 외형에 검은색 날개가 뒤에 붙어있는 모습 …
공중에 떴다가 날개가 생기고 무기가 사라진 클레르"*): 악마 몸 그림(`demon_body` 115 프레임 · `costumes/demon_300*`)은
더는 실리지 않는다. 게임은 악의 중에도 사람 시퀀스의 사본(무기 · 보조손 없음 — `demon_look.demon_copy`)을 자기 파츠
(머리 · 몸통 · 팔 · 다리 = 입은 장비 · 아바타)로 그리고, 호스트는 그 앞(뒷모습일 땐 뒤)에 날개를 합성한다.

* `demon_body` = **날개** `WING_FRAMES` 16 프레임 = 바라보는 쪽 4(뒤 · 앞 · 왼 · 오른 = g.u − 1) × 날갯짓 4 단계. DRAW 조각이
  `(방향 − 1) × 4 + ((TICKS >> 3) & 3)` 을 나이로 발행한다 — 앞 · 왼 · 오른은 몸을 그리기 전(몸이 덮는다), 뒷모습은 몸 뒤에
  (등이 보이니 날개가 몸 위). 한 프레임 = 날개 그림 하나, 기준점 = 어깨 사이의 뿌리, 조각의 (dx, dy) = 캐릭터 원점에서 어깨까지.
* `claw{c}_{dir}` — 네 참격(0 첫 베기 · 1 교차 · 2 쓸기 · 3 내리찍기). 왼쪽 = 오른쪽의 거울, 위 · 아래 = 끝이 그쪽을 보게 돌린 것.
  야수조련 목걸이의 큰 참격은 그림이 아니라 게임이 가슴 기준 1.6× 로 그린다(`FX_TAG | 2` · `FX_FROM_CHEST`).

    python tools_re/malice_wings.py            # demon_fx.bin 의 날개 · 참격만 다시 쓴다 (다른 효과는 바이트 그대로)
    python tools_re/malice_wings.py preview    # docs/skill/malice_wings_preview/ — 클레르(나이트 맨몸) 위에 날개 · 참격
"""

import os
from pathlib import Path

import numpy as np
from PIL import Image

import demon_fx as fx
import demon_look as dl
import fire_fx

ROOT = Path(__file__).resolve().parents[1]
ART = ROOT / "costumes/malice_wings"
PREVIEW = ROOT / "docs/skill/malice_wings_preview"
SCALE = 6  # the HD twin's scale (demon_upscale.TO_SCALE)
FROM_CHEST = 128
ALPHAS = (210, 255, 235, 165, 95, 40)
FLAPS = 4
WING_FRAMES = 4 * FLAPS
# the wing's height a flap phase (0 spread … 2 folded … back), about the root
FLAP_HEIGHT = (1.0, 0.93, 0.86, 0.93)
# guest px — the character is ≈ 30 px tall (head −26 … feet +4); the shoulders sit at −13
SPREAD = (42, 28)
PROFILE = (20, 28)  # one wing of the pair, seen from the side
# where the wing root (the anchor) goes, guest px from the character's origin, a view
ROOT_AT = {dl.BACK: (0, -13), dl.FRONT: (0, -13), dl.LEFT: (2, -13), dl.RIGHT: (-2, -13)}  # the side roots sit INSIDE her back
# guest px of each cut, tips pointing right. The slam is the tall crescent turned forward.
CLAW_SIZE = ((40, 22), (36, 26), (50, 28), (32, 42))
# where the thick end (the hand) sits, guest px from the character's origin
HAND = (6, -12)

# source sheet, measured: three wing groups and the profile pair split at the column dip;
# four claw groups, the last two split the same way.
WING_BOXES = ((19, 44, 609, 641), (646, 44, 1277, 641), (1311, 44, 1748, 641), (1748, 44, 2152, 641))
CLAW_BOXES = ((16, 14, 560, 712), (590, 14, 1100, 712), (1130, 14, 1725, 712), (1735, 14, 2164, 712))
# sheet order: 0 front pair, 1 back pair, 2 · 3 profile wings (hanging down — they read as a cape on a 30 px figure, unused).
# ★ 2026-10-22 (3): the side views take ONE WING OF THE BACK PAIR — the half whose feathers sweep up and away from her back:
# facing right, her back is on the left, so the pair's left wing (root = the joint at its right edge); facing left, the right
# wing. The first two versions used the profile pictures with the root on the wrong end, so the wing floated off her back (user).
WING_SOURCE = {dl.FRONT: 0, dl.BACK: 1}
SIDE_PAIR = 1
# the pair's joint, fractions of the pair's box — where the two wings meet
JOINT = (0.50, 0.56)
assert WING_FRAMES == fx.WING_FRAMES


def _crop(name, box):
    im = Image.open(ART / name).convert("RGBA").crop(box)
    return im.crop(im.getbbox())


def _pair(im, tw, th):
    """Source → (1× RGBA, 6× RGBA) sharing the same framing."""
    lo = im.resize((tw, th), Image.Resampling.LANCZOS)
    hi = im.resize((tw * SCALE, th * SCALE), Image.Resampling.LANCZOS)
    return np.asarray(lo), np.asarray(hi)


def _add(lib, lo, hi, ax, ay):
    return lib.sprite((lo, ax, ay), (hi, ax * SCALE, ay * SCALE, SCALE))


# ★ 2026-10-22 (2) (user: *"검은색 날개가 어두운 맵에서 잘 안보이는데 검은색이랑 회색을 섞어서"*): the source feathers are near
# black. The wing is re-shaded black → grey: its luminance stretched over its own 5..95 percentiles onto GREY_LO..GREY_HI (the
# feather shadows stay black, the vanes go grey), a faint cool tint, and a lighter rim along the silhouette (the feather tips and
# edges) so the shape reads on dark ground. `shade_wing` is what the preview and the bin both use.
GREY_LO, GREY_HI, RIM = 0.08, 0.52, 0.74
TINT = (0.96, 0.97, 1.05)
RIM_WIDTH = 21  # px of the source (≈ 600 px wide) — a thin edge once the wing is 42 px


def shade_wing(im):
    from PIL import ImageFilter
    a = np.asarray(im).astype(np.float32) / 255.0
    alpha = a[..., 3]
    lum = a[..., :3] @ np.array((0.299, 0.587, 0.114), np.float32)
    mask = alpha > 0.5
    lo, hi = (np.percentile(lum[mask], [5, 95]) if mask.any() else (0.0, 1.0))
    t = np.clip((lum - lo) / max(float(hi - lo), 1e-3), 0.0, 1.0)
    g = GREY_LO + (GREY_HI - GREY_LO) * t ** 0.85
    eroded = np.asarray(Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.MinFilter(RIM_WIDTH))).astype(np.float32) / 255.0
    rim = np.clip(alpha - eroded, 0.0, 1.0)
    g = g + (RIM - g) * rim * 0.9
    rgb = np.stack([np.clip(g * TINT[0], 0, 1), np.clip(g * TINT[1], 0, 1), np.clip(g * TINT[2], 0, 1)], -1)
    out = np.concatenate([rgb, alpha[..., None]], -1)
    return Image.fromarray((out * 255).round().astype(np.uint8), "RGBA")


# ★ 2026-10-22 (3) (user: *"날개 도트가 너무 좀 부자연스러운데 아바타처럼 좀더 자연스럽게 업스케일링"*): the wing is DRAWN AS PIXEL
# ART first — the shaded source brought down to its guest size, hard alpha, GREYS luminance levels and a 1 px dark outline like
# the character's own sheets — and its 6× twin is that art blown up the way the avatar is (xBR ×8 averaged to ×6,
# `demon_upscale.xbr2x`), not the photo resized. Before, the twin was a LANCZOS resize of the feather photo: soft and
# photographic next to the crisp sprite.
GREYS = 8
OUTLINE = 0.05
ART_SCALE = 3  # the art is drawn at 3× (the mock-ups' resolution) — the feathers survive; 1× is its box third


def pixel_art(im, tw, th):
    """Shaded source → (1× RGBA tw×th, its 6× twin). The art: the source at 3×, hard alpha, GREYS luminance levels, a 1 px (at
    3×) dark outline; the twin = that art through xBR twice and a ½ box (`demon_upscale.smooth6`, the demon body's route);
    the 1× = the art's premultiplied box third."""
    import demon_upscale as du
    W, H = tw * ART_SCALE, th * ART_SCALE
    inner = im.resize((max(1, W - 2), max(1, H - 2)), Image.Resampling.LANCZOS)
    lo = np.zeros((H, W, 4), np.float32)
    lo[1:1 + inner.height, 1:1 + inner.width] = np.asarray(inner).astype(np.float32) / 255.0
    opaque = lo[..., 3] >= 0.5
    lum = lo[..., :3] @ np.array((0.299, 0.587, 0.114), np.float32)
    lvl = np.clip(np.round(lum * (GREYS - 1)) / (GREYS - 1), 0.0, 1.0)
    pad = np.pad(opaque, 1)
    touching = pad[:-2, 1:-1] | pad[2:, 1:-1] | pad[1:-1, :-2] | pad[1:-1, 2:]
    outline = touching & ~opaque
    art = np.zeros((H, W, 4), np.float32)
    for c in range(3):
        art[..., c] = np.where(opaque, np.clip(lvl * TINT[c], 0, 1), np.where(outline, OUTLINE, 0.0))
    art[..., 3] = np.where(opaque | outline, 1.0, 0.0)
    art8 = (art * 255).round().astype(np.uint8)
    twin = du.smooth6(art8)
    pm = np.concatenate([art[..., :3] * art[..., 3:4], art[..., 3:4]], -1)
    small = Image.fromarray((pm * 255).round().astype(np.uint8), "RGBA").resize((tw, th), Image.Resampling.BOX)
    q = np.asarray(small).astype(np.float32) / 255.0
    al = q[..., 3:4]
    col = np.where(al > 1e-3, q[..., :3] / np.maximum(al, 1e-3), 0.0)
    one = (np.concatenate([np.clip(col, 0, 1), np.clip(al, 0, 1)], -1) * 255).round().astype(np.uint8)
    one[one[..., 3] < 8] = 0
    return one, twin


def side_wing(pair, view):
    """One wing of the back pair for a side view, and its root (the joint) as fractions of the crop."""
    w, h = pair.size
    jx = int(round(w * JOINT[0]))
    if view == dl.RIGHT:
        crop = pair.crop((0, 0, jx, h))  # the pair's left wing: root at its right edge
        return crop, (1.0, JOINT[1])
    crop = pair.crop((jx, 0, w, h))
    return crop, (0.0, JOINT[1])


def wing_frames(lib):
    """The `demon_body` effect: frame = view × FLAPS + phase, one wing part at the shoulder."""
    raw = [shade_wing(_crop("wings_source.png", b)) for b in WING_BOXES]
    frames = []
    for view in (dl.BACK, dl.FRONT, dl.LEFT, dl.RIGHT):
        if view in (dl.FRONT, dl.BACK):
            im, (fx_, fy_) = raw[WING_SOURCE[view]], (0.5, 0.5)
            tw, th0 = SPREAD
        else:
            im, (fx_, fy_) = side_wing(raw[SIDE_PAIR], view)
            tw, th0 = PROFILE
        rx, ry = ROOT_AT[view]
        for phase in range(FLAPS):
            th = max(4, int(round(th0 * FLAP_HEIGHT[phase])))
            lo, hi = pixel_art(im, tw, th)
            ax = min(tw - 1, int(round((tw - 2) * fx_)) + 1)
            ay = min(th - 1, int(round((th - 2) * fy_)) + 1)
            frames.append([(_add(lib, lo, hi, ax, ay), rx, ry, 255, 0)])
    assert len(frames) == WING_FRAMES
    return frames


# ★★ 2026-10-22 (4) (user: *"캐릭터 발밑에 검은색,회색 섞은 계열의 오오라"*): the AURA under her feet while 악의 runs — the ground
# layer (the FXG piece draws it first, under every entity, at her feet), AURA_LEN frames of a flattened dark pool with grey
# smoke wisps turning around its rim. Drawn procedurally at 6× (the twin) and boxed down to 1×; anchor = the pool's centre.
AURA_LEN = 16
AURA_SIZE = (40, 16)  # guest px
AURA_SHIFT = 1  # the FXG piece advances a frame every 2 ticks → 1.6 s a turn
assert AURA_LEN == fx.AURA_LEN


def aura_picture(k):
    """Frame k of AURA_LEN at 6× — RGBA float: a near-black pool, grey smoke wisps turning around its rim, tongues licking
    out past it."""
    W, H = AURA_SIZE[0] * SCALE, AURA_SIZE[1] * SCALE
    ys, xs = np.mgrid[0:H, 0:W].astype(np.float32)
    nx = (xs + 0.5 - W / 2) / (W / 2)
    ny = (ys + 0.5 - H / 2) / (H / 2)
    r = np.sqrt(nx * nx + ny * ny)
    th = np.arctan2(ny, nx)
    ph = 2 * np.pi * k / AURA_LEN
    core = np.clip((0.92 - r) / 0.45, 0, 1) ** 1.2 * 0.9
    rim = np.exp(-((r - 0.78) / 0.20) ** 2)
    w1 = 0.5 + 0.5 * np.sin(3 * th + ph + 2.2 * r)
    w2 = 0.5 + 0.5 * np.sin(5 * th - 2 * ph + 6.0 * r + 1.3)
    wisp = np.clip((w1 * 0.6 + w2 * 0.4 - 0.42) / 0.5, 0, 1) ** 1.1
    tongues = np.exp(-((r - 1.0) / 0.13) ** 2) * (0.5 + 0.5 * np.sin(7 * th + 1.5 * ph)) ** 2.5
    smoke = np.clip(rim * wisp * 1.3 + tongues * 1.1, 0, 1)
    grey = 0.03 + 0.72 * smoke
    alpha = np.clip(core * (1 - 0.35 * smoke) + smoke * 0.95, 0, 1) * (r < 1.18)
    rgb = np.stack([grey * TINT[0], grey * TINT[1], grey * TINT[2]], -1)
    return np.concatenate([np.clip(rgb, 0, 1), alpha[..., None]], -1)


def aura_frames(lib):
    frames = []
    for k in range(AURA_LEN):
        pic = aura_picture(k)
        hi = (pic * 255).round().astype(np.uint8)
        hi[hi[..., 3] < 8] = 0
        pm = np.concatenate([pic[..., :3] * pic[..., 3:4], pic[..., 3:4]], -1)
        small = Image.fromarray((pm * 255).round().astype(np.uint8), "RGBA").resize(AURA_SIZE, Image.Resampling.BOX)
        q = np.asarray(small).astype(np.float32) / 255.0
        al = q[..., 3:4]
        col = np.where(al > 1e-3, q[..., :3] / np.maximum(al, 1e-3), 0.0)
        lo = (np.concatenate([np.clip(col, 0, 1), np.clip(al, 0, 1)], -1) * 255).round().astype(np.uint8)
        lo[lo[..., 3] < 8] = 0
        ax, ay = AURA_SIZE[0] // 2, AURA_SIZE[1] // 2
        frames.append([(_add(lib, lo, hi, ax, ay), 0, 1, 255, 0)])  # a px below the feet point
    return frames


def _point_right(im, cut):
    """Tips point right. The slam's source crescent points up, so it turns a quarter."""
    if cut == 3:
        im = im.rotate(-90, resample=Image.Resampling.BICUBIC, expand=True)
    a = np.asarray(im)
    ys, xs = np.nonzero(a[..., 3] > 20)
    box = (int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1)
    return im.crop(box)


def _hand_anchor(lo):
    """The thick end lands on HAND. Tips are the right-hand side of the picture."""
    a = np.asarray(lo)
    h, w = a.shape[:2]
    hand = a[:, : max(1, w // 5), 3] > 40
    ys, xs = np.nonzero(hand)
    if len(xs) == 0:
        hx, hy = 0, h // 2
    else:
        hx, hy = int(xs.mean()), int(ys.mean())
    return hx - HAND[0], hy - HAND[1]


def claw_frames(lib):
    """`claw{c}_{dir}` → 6 ages of one slash (alpha only — one sprite a direction)."""
    out = {}
    for cut, box in enumerate(CLAW_BOXES):
        im = _point_right(_crop("claws_source.png", box), cut)
        tw, th = CLAW_SIZE[cut]
        lo, hi = _pair(im, tw, th)
        ax, ay = _hand_anchor(lo)
        sid = _add(lib, lo, hi, ax, ay)
        out[f"claw{cut}_right"] = [[(sid, 0, 0, alpha, FROM_CHEST)] for alpha in ALPHAS]
        out[f"claw{cut}_left"] = [[(sid, 0, 0, alpha, FROM_CHEST | fx.FLIP)] for alpha in ALPHAS]
        out[f"claw{cut}_up"] = _turned(lib, im, tw, th, "up")
        out[f"claw{cut}_down"] = _turned(lib, im, tw, th, "down")
    return out


def _turned(lib, im, tw, th, way):
    """`way` up: tips point up (the right slash turned CCW). down: CW. The hand stays on HAND, straight ahead is
    up or down so the hand's x is the body's."""
    deg = 90 if way == "up" else -90
    turned = im.rotate(deg, resample=Image.Resampling.BICUBIC, expand=True)
    a = np.asarray(turned)
    ys, xs = np.nonzero(a[..., 3] > 16)
    turned = turned.crop((int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1))
    lo, hi = _pair(turned, th, tw)  # the guest box swaps axes
    arr = np.asarray(lo)
    h, w = arr.shape[:2]
    band_h = max(1, h // 5)
    if way == "up":
        hand = arr[h - band_h:, :, 3] > 40  # the hand is the near (bottom) end
        ys, xs = np.nonzero(hand)
        hy = (int(ys.mean()) + h - band_h) if len(xs) else h - 1
    else:
        hand = arr[:band_h, :, 3] > 40
        ys, xs = np.nonzero(hand)
        hy = int(ys.mean()) if len(xs) else 0
    hx = int(xs.mean()) if len(xs) else w // 2
    sid = _add(lib, lo, hi, hx - 0, hy - HAND[1])
    return [[(sid, 0, 0, alpha, FROM_CHEST)] for alpha in ALPHAS]


def _spin(plain, name):
    """The 악의 weapons' four-sided cut: the four facings' parts together (no direction bits — the canvas draws each as it is)."""
    cut = name[len("claw"): -len("_spin")]
    frames = []
    for age in range(len(ALPHAS)):
        parts = []
        for direction in ("up", "down", "left", "right"):
            parts.extend(plain[f"claw{cut}_{direction}"][age])
        frames.append(parts)
    return frames


def _big(lib, plain, name):
    """The holy sword's larger cut: the same slash × 1.35 about its anchor. No direction bits (the runtime test wants
    `flags & 0x30 == 0` on every facing)."""
    import demon_weapon_fx as dw
    plain_name = name[: -len("_big")]
    frames = []
    made = {}
    for age_parts in plain[plain_name]:
        new = []
        for sid, dx, dy, alpha, fl in age_parts:
            if sid not in made:
                arr, ax, ay = lib.sprites[sid]
                bigger = dw.scale_sprite(arr, ax, ay)
                if sid in lib.twins:
                    tarr, tax, tay, tscale = lib.twins[sid]
                    h, w = tarr.shape[:2]
                    nw, nh = max(1, round(w * dw.SCALE)), max(1, round(h * dw.SCALE))
                    hi = np.asarray(Image.fromarray(tarr, "RGBA").resize((nw, nh), Image.Resampling.LANCZOS))
                    made[sid] = lib.sprite(bigger, (hi, int(round(tax * dw.SCALE)), int(round(tay * dw.SCALE)), tscale))
                else:
                    made[sid] = lib.sprite(bigger)
            new.append((made[sid], int(round(dx * dw.SCALE)), int(round(dy * dw.SCALE)), alpha, fl & ~0x30))
        frames.append(new)
    return frames


def replace_library(old):
    """`old` (a built or read-back library) with the wings and the four cuts (and the weapons' four-sided · bigger cuts made
    from them) in place of its own; everything else as it was."""
    lib = fx.Library(hd=True)
    claws = claw_frames(lib)
    wings = wing_frames(lib)

    def keep(s):
        if s in old.qtwins:
            return lib.sprite_q(old.sprites[s], old.qtwins[s])
        twin = old.twins.get(s) if hasattr(old, "twins") else None
        return lib.sprite(old.sprites[s], twin)

    for name, frames in old.effects:
        if name == "demon_body":
            lib.add_effect(name, wings)
        elif name in claws:
            lib.add_effect(name, claws[name])
        elif name.startswith("claw") and name.endswith("_spin"):
            lib.add_effect(name, _spin(claws, name))
        elif name.startswith("claw") and name.endswith("_big"):
            lib.add_effect(name, _big(lib, claws, name))
        elif name == "malice_aura":
            lib.add_effect(name, aura_frames(lib))
        else:
            lib.add_effect(name, [[(keep(s), dx, dy, a, fl) for (s, dx, dy, a, fl) in parts] for parts in frames])
    if "malice_aura" not in {n for n, _ in old.effects}:
        lib.add_effect("malice_aura", aura_frames(lib))  # ★ (4) appended — an older bin
    else:
        pass
    assert [n for n, _ in lib.effects] == [n for n, _ in fx.EFFECTS], [n for n, _ in lib.effects][-3:]
    return lib


def _paste(canvas, src, ax, ay, x, y):
    """`src` RGBA onto float RGB canvas. `(ax, ay)` of the source lands on `(x, y)`."""
    h, w = src.shape[:2]
    x0, y0 = int(round(x - ax)), int(round(y - ay))
    H, W = canvas.shape[:2]
    cx0, cy0 = max(0, x0), max(0, y0)
    cx1, cy1 = min(W, x0 + w), min(H, y0 + h)
    if cx0 >= cx1 or cy0 >= cy1:
        return
    sub = src[cy0 - y0: cy1 - y0, cx0 - x0: cx1 - x0].astype(np.float32)
    sa = sub[..., 3:4] / 255.0
    canvas[cy0:cy1, cx0:cx1] = sub[..., :3] * sa + canvas[cy0:cy1, cx0:cx1] * (1 - sa)


def _claire(cls, seq, frame, view):
    """The bare knight (body:102 arm:100 leg:102) of `seq` frame `frame` facing `view`, composed as the game does —
    (RGBA at 1×, origin x, origin y)."""
    import costume_sheets as cs
    import costume_geometry as geometry
    import venom_effect
    jar = cs.open_jar(cs.DEFAULT_ZIP)
    _h, anim_parts, seqs = venom_effect.decode(geometry.patch_animation(jar.read(f"hero/hero{cls}")))
    flip_lists = [set(first) for first, _second in anim_parts]
    table = {p: cs.sheet_by_path(jar, b) for p, b in cs.PREVIEW_GEAR[cls].items()}
    _label, look = cs.load_look(jar, "body:102,arm:100,leg:102", cs.DEFAULT_ZIP)
    table.update({d: fr for (d, cl), fr in look.items() if cl is None or cl == cls})
    sprites = [sp for sp in seqs[seq][frame][view] if sp[0] & 0x7f not in (4, 6, 7)]
    pieces = cs.compose(sprites, table, {4, 5}, False, flip_lists)
    x0 = min(x for x, _y, _w, _h, _px in pieces) - 2
    y0 = min(y for _x, y, _w, _h, _px in pieces) - 2
    x1 = max(x + w for x, _y, w, _h, _px in pieces) + 2
    y1 = max(y + h for _x, y, _w, h, _px in pieces) + 2
    out = np.zeros((y1 - y0, x1 - x0, 4), np.uint8)
    for x, y, w, h, px in pieces:
        for yy in range(h):
            for xx in range(w):
                p = px[yy][xx]
                if p[3]:
                    out[y - y0 + yy, x - x0 + xx] = p
    return out, -x0, -y0


def preview(out=None):
    """The knight standing in each facing with the wing, then facing right with the four cuts (plain and the necklace's
    1.6× about the chest). Everything at 6× — the twins."""
    out = Path(out or PREVIEW)
    out.mkdir(parents=True, exist_ok=True)
    lib = fx.Library(hd=True)
    wings = wing_frames(lib)
    claws = claw_frames(lib)
    bg = (24, 20, 28)  # dark ground — where the all-black wing vanished

    def tile(view, cut=None, chest=False, phase=0):
        body, ox, oy = _claire(1, 0, 0, view)
        body6 = np.asarray(Image.fromarray(body, "RGBA").resize((body.shape[1] * SCALE, body.shape[0] * SCALE), Image.Resampling.NEAREST))
        W, H = 110 * SCALE, 70 * SCALE
        canvas = np.zeros((H, W, 3), np.float32)
        canvas[:] = bg
        cx, cy = 50 * SCALE, 48 * SCALE  # the character's origin on the tile
        (sid, dx, dy, _a, _fl), = wings[view * FLAPS + phase]
        warr, wax, way, _s = lib.twins[sid]
        if view != dl.BACK:
            _paste(canvas, warr, wax, way, cx + dx * SCALE, cy + dy * SCALE)
        _paste(canvas, body6, ox * SCALE, oy * SCALE, cx, cy)
        if view == dl.BACK:
            _paste(canvas, warr, wax, way, cx + dx * SCALE, cy + dy * SCALE)
        if cut is not None:
            sid, dx, dy, alpha, _fl = claws[f"claw{cut}_right"][1][0]
            carr, cax, cay, _s = lib.twins[sid]
            s = 1.6 if chest else 1.0
            if s != 1.0:
                h, w = carr.shape[:2]
                carr = np.asarray(Image.fromarray(carr, "RGBA").resize((int(w * s), int(h * s)), Image.Resampling.BILINEAR))
            faded = carr.copy()
            faded[..., 3] = (faded[..., 3].astype(np.uint16) * alpha // 255).astype(np.uint8)
            shift = (1.0 - s) * -20 * SCALE if chest else 0.0  # the game grows about the chest
            _paste(canvas, faded, cax * s, cay * s, cx + dx * SCALE, cy + dy * SCALE + shift)
        gy = cy + 4 * SCALE
        canvas[gy: gy + 3, cx - 20 * SCALE: cx + 20 * SCALE] *= 0.55
        return Image.fromarray(np.clip(canvas, 0, 255).astype(np.uint8), "RGB")

    def row(tiles):
        sheet = Image.new("RGB", (sum(t.width for t in tiles), tiles[0].height), (20, 24, 18))
        x = 0
        for t in tiles:
            sheet.paste(t, (x, 0))
            x += t.width
        return sheet

    stands = row([tile(v) for v in (dl.BACK, dl.FRONT, dl.LEFT, dl.RIGHT)])
    flaps = row([tile(dl.RIGHT, phase=p) for p in range(FLAPS)])
    basic = row([tile(dl.RIGHT, cut=c) for c in range(4)])
    neck = row([tile(dl.RIGHT, cut=c, chest=True) for c in range(4)])
    stands.save(out / "wings_stand.png")
    flaps.save(out / "wings_flap.png")
    basic.save(out / "claw_basic.png")
    neck.save(out / "claw_necklace.png")
    print(stands.size, flaps.size, basic.size, neck.size)


def build():
    sprites, effects, qtwins = fire_fx.read_raw(fx.OUT_BIN)
    old = fx.Library(hd=True)
    old.sprites = sprites
    old.effects = effects
    old.qtwins = qtwins
    result = replace_library(old)
    tmp = fx.OUT_BIN + ".new"
    print(result.write(tmp))
    os.replace(tmp, fx.OUT_BIN)


if __name__ == "__main__":
    import sys
    if len(sys.argv) > 1 and sys.argv[1] == "preview":
        preview()
    else:
        build()
