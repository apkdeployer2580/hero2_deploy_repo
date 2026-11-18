#!/usr/bin/env python3
"""B 경로 (2026-10-14 밤) — 이미지 생성 도구(ASTRA)로 받은 「연속 프레임 시트」를 게임 해상도 프레임으로 들여온다.

사용자 결정 「A 먼저, B 오면 교체」: A = 예상도 한 장면에서 오린 정점 프레임을 움직인 것(`demon_fx_cut.py` · `demon_draft.py`).
B = 같은 그림체의 프레임 시트를 받아 그 프레임을 그대로 쓰는 것. 시트를 만드는 법(프롬프트 · 격자 · 배경색)은
`docs/skill/astra_demon/astra_frames/README.md`, 격자 템플릿은 `template` 명령이 그린다.

시트 규약 = 한 칸(cell) 안에 한 프레임, 칸은 왼쪽 위부터 줄 단위 · 배경은 **단색 #00FF00**(그라데이션 · 그림자 없음) · 칸마다
발바닥 기준선(template 의 빨간 십자) 자리에 기준점. 들여오기 = 칸 자르기 → 원본 격자 위상을 찾아 블록 대표색(게임 1 px = 시트 S px)
→ 단색 배경 빼기(알려진 배경이라 `astra_cut.matte` 와 같은 식이 정확해진다) → `costumes/astra_fx/frames/<이름>/<k>.png` + meta.json.

    python tools_re/astra_frames.py template effect <out.png> --cols 6 --rows 2 --cell 96 96 --anchor 48 72 --scale 3
    python tools_re/astra_frames.py template demon <out.png>             # 악마 51 칸 (자세 12 줄 × 방향 4 + 쓰러짐 3)
    python tools_re/astra_frames.py templates <dir>                       # 아래 SHEETS 표의 템플릿 전부 + demon.png
    python tools_re/astra_frames.py import <sheet.png> <이름>              # SHEETS 표의 규격이면 칸 · 기준점은 자동
    python tools_re/astra_frames.py import <sheet.png> <이름> --cols C --rows R --cell W H --anchor AX AY --frames N [--scale S]
    python tools_re/astra_frames.py import <sheet.png> demon --demon     # 악마 51 칸 → frames/demon/<k>.png (일부 칸만 그려도 된다)
    python tools_re/astra_frames.py list

이름 = 효과 이름(`demon_fx.EFFECTS` 의 것 — 예 `burst`, `fire_blast`, `seal`) 또는 할퀴기 원본 `claw0..claw3`(오른쪽만; 위 · 아래 ·
왼쪽은 `demon_fx_cut` 이 돌리고 뒤집는다) · `lava_cool` / `lava_hot`(64 × 64 반복 질감 한 칸) · `demon`.
들인 프레임 수가 계약 길이와 다르면 시간축으로 늘이거나 줄여(가장 가까운 프레임) 쓴다 — 효과 번호 · 길이 계약은 그대로다.
"""

import argparse
import json
import os
import sys

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FRAMES_DIR = os.environ.get("ASTRA_FRAMES_DIR") or os.path.join(ROOT, "costumes", "astra_fx", "frames")
KEY = (0, 255, 0)


# ── 템플릿 ─────────────────────────────────────────────────────────────────────────────────────────────────
def template(out, cols, rows, cell, anchor, scale, labels=None):
    """격자 템플릿 — 칸 테두리(회색) · 기준점 십자(빨강) · 게임 1 px 격자(옅게) · 칸 번호. 배경 = 단색 #00FF00."""
    cw, ch = cell
    W, H = cols * cw * scale, rows * ch * scale
    img = Image.new("RGB", (W, H), KEY)
    d = ImageDraw.Draw(img)
    for r in range(rows):
        for c in range(cols):
            x0, y0 = c * cw * scale, r * ch * scale
            for gx in range(0, cw, 8):
                d.line([(x0 + gx * scale, y0), (x0 + gx * scale, y0 + ch * scale - 1)], fill=(40, 225, 40))
            for gy in range(0, ch, 8):
                d.line([(x0, y0 + gy * scale), (x0 + cw * scale - 1, y0 + gy * scale)], fill=(40, 225, 40))
            d.rectangle([x0, y0, x0 + cw * scale - 1, y0 + ch * scale - 1], outline=(110, 110, 110))
            ax, ay = x0 + anchor[0] * scale, y0 + anchor[1] * scale
            d.line([(ax - 4 * scale, ay), (ax + 4 * scale, ay)], fill=(255, 0, 0), width=max(1, scale // 2))
            d.line([(ax, ay - 4 * scale), (ax, ay + 4 * scale)], fill=(255, 0, 0), width=max(1, scale // 2))
            k = r * cols + c
            d.text((x0 + 3, y0 + 2), labels[k] if labels and k < len(labels) else str(k), fill=(0, 90, 0))
    img.save(out)
    print(f"template → {out} ({W}×{H}, {cols}×{rows} 칸, 칸 {cw}×{ch} 게임 px × {scale})")


def demon_layout():
    import demon_draft as dd
    import demon_look as dl
    labels = []
    for pose in dl.POSES:
        for v in ("back", "front", "left", "right"):
            labels.append(f"{pose}/{v}")
    labels += [f"{p}/front" for p in dl.FALLS]
    return dict(cols=4, rows=len(dl.POSES) + 1, cell=(dd.CW, dd.CH), anchor=dd.FEET, labels=labels)


# 시트 규격 — (이름, 프레임 수, 칸 열 수, 칸 게임 px (w, h), 기준점 (x, y), 내용). 프레임 수가 계약 길이보다 적으면 시간축으로 늘인다.
SHEETS = [
    ("claw0", 6, 6, (104, 88), (12, 76), "할퀴기 1 단 첫 손 — 오른쪽을 보는 악마 앞을 아래-왼쪽에서 위-오른쪽으로 긋는 붉은 참격 한 줄(／)"),
    ("claw1", 6, 6, (104, 88), (12, 76), "할퀴기 1 단 둘째 손 — 첫 줄과 엇갈리는 둘째 참격(＼)으로 X, 끝의 흰 접촉 섬광 · 튀는 검은 조각"),
    ("claw2", 6, 6, (104, 96), (12, 84), "할퀴기 2 단 — 아래에서 위로 쓸어 올리는 금빛 초승달 · 흙먼지"),
    ("claw3", 6, 6, (112, 88), (12, 76), "할퀴기 3 단 — 앞 땅으로 내리꽂는 세 줄 금빛 참격 · 땅의 금 · 흙먼지 · 튀는 돌"),
    ("charge_ground_right", 8, 4, (120, 56), (100, 44), "돌진 땅 층(반복) — 오른쪽으로 달리는 악마 뒤의 남청 잔상 · 붉은 속도선 · 흙먼지 (악마 본체는 그리지 않는다)"),
    ("charge_top_right", 8, 4, (72, 48), (28, 44), "돌진 위 층(반복) — 머리 위를 긋는 붉은 호와 앞쪽 충돌 섬광"),
    ("follow_dome", 8, 4, (112, 96), (56, 84), "추종 100 %(반복) — 악마를 감싸는 반투명 보라 반구, 테의 빛 · 안쪽 검은 결정"),
    ("burst", 10, 5, (144, 112), (72, 96), "추종 폭발 — 섬광 → 검은 결정 가시 부채가 솟음 → 부서져 흩어짐"),
    ("burst_mob", 6, 6, (48, 56), (24, 48), "폭발에 맞은 몬스터 위의 분홍 섬광 · 작은 가시"),
    ("seal", 8, 4, (176, 56), (88, 32), "지옥칼날 바닥 인장(반복) — 붉은 마법진 · 룬, 빛이 고리를 따라 돈다 (가운데 악마 자리도 인장)"),
    ("blade_rise0", 8, 8, (40, 72), (8, 60), "지옥칼날 낫날 — 몬스터 오른쪽 땅에서 솟음(0..2) · 머묾(3..5) · 가라앉음(6..7)"),
    ("judge_orb", 10, 5, (64, 80), (32, 76), "어둠심판 선딜 — 머리 위에 검은 구체가 응축"),
    ("bolt0", 4, 4, (56, 144), (28, 136), "어둠심판 낙뢰 — 하늘에서 몬스터 발밑으로, 0 = 가장 밝음 → 3 = 사라짐"),
    ("fire_core", 10, 5, (96, 80), (48, 64), "멸화 선딜 위 층 — 악마 가슴의 붉은 핵이 커진다"),
    ("fire_ground", 10, 5, (96, 80), (48, 64), "멸화 선딜 땅 층 — 발밑의 붉은 빛 · 갈라짐"),
    ("fire_blast", 12, 4, (176, 136), (88, 120), "멸화 폭발 위 층 — 불기둥 무리 (가운데 악마 몸 자리는 비워 둔다; 악마는 이 층 밑에 그려진다)"),
    ("fire_mob", 6, 6, (48, 56), (24, 48), "멸화에 맞은 몬스터 위의 불꽃"),
    ("fire_scorch", 8, 4, (144, 56), (72, 28), "멸화 땅 층 — 그을린 땅 · 식는 금 · 잔불 (처음 몇 장은 악마 뒤의 불)"),
    ("transform", 12, 4, (112, 112), (56, 92), "변신 위 층 — 분홍 띠 · 그림자 · 모이는 조각 · 붉은 핵 → 출현(보라 결정 · 흙먼지)"),
    ("transform_ground", 12, 4, (112, 112), (56, 92), "변신 땅 층 — 발밑 그림자 · 분홍 고리 → 출현의 흙먼지 고리 · 튀는 돌"),
    ("lava_cool", 1, 1, (64, 64), (0, 0), "용암불 냉각 — 이음매 없이 반복되는 64 × 64 현무암 판 · 붉은 균열"),
    ("lava_hot", 1, 1, (64, 64), (0, 0), "용암불 연소 — 같은 판 무늬의 끓는 상태(주황 · 노랑 균열)"),
]


def sheet_spec(name):
    for row in SHEETS:
        if row[0] == name:
            return row
    raise KeyError(name)


def templates(out_dir, scale=3):
    os.makedirs(out_dir, exist_ok=True)
    for name, n, cols, cell, anchor, _ in SHEETS:
        rows = (n + cols - 1) // cols
        template(os.path.join(out_dir, f"{name}.png"), cols, rows, cell, anchor, scale)
    L = demon_layout()
    template(os.path.join(out_dir, "demon.png"), L["cols"], L["rows"], L["cell"], L["anchor"], scale, L["labels"])


# ── 들여오기 ───────────────────────────────────────────────────────────────────────────────────────────────
def _blocks(n, s):
    """시트 칸(n, float, H×W×3)을 게임 px 로 — 위상(±s/2) 가운데 블록 분산이 가장 작은 것, 블록 중앙값."""
    H, W = n.shape[:2]
    gw, gh = int(round(W / s)), int(round(H / s))
    best = None
    for oy in np.linspace(-s / 2, s / 2, 5):
        for ox in np.linspace(-s / 2, s / 2, 5):
            xs = [int(round(ox + k * s)) for k in range(gw + 1)]
            ys = [int(round(oy + k * s)) for k in range(gh + 1)]
            xs = [min(max(v, 0), W) for v in xs]
            ys = [min(max(v, 0), H) for v in ys]
            var = 0.0
            for a in range(gh):
                for b in range(gw):
                    blk = n[ys[a]:ys[a + 1], xs[b]:xs[b + 1]]
                    if blk.size:
                        var += float(((blk - blk.mean(axis=(0, 1))) ** 2).sum())
            if best is None or var < best[0]:
                best = (var, xs, ys)
    _, xs, ys = best
    out = np.zeros((gh, gw, 3), np.float32)
    for a in range(gh):
        for b in range(gw):
            blk = n[ys[a]:max(ys[a + 1], ys[a] + 1), xs[b]:max(xs[b + 1], xs[b] + 1)].reshape(-1, 3)
            out[a, b] = np.median(blk, axis=0)
    return out


def key_out(m, key=KEY, hard=False, floor=0.1):
    """알려진 단색 배경 빼기 → (색, 알파). hard = 불투명/투명 둘뿐(악마 시트)."""
    B = np.array(key, np.float32)
    up = np.where(m > B, (m - B) / np.maximum(255 - B, 1), 0)
    dn = np.where(m < B, (B - m) / np.maximum(B, 1), 0)
    a = np.maximum(up, dn).max(axis=2)
    if hard:
        dist = np.sqrt(((m - B) ** 2).sum(axis=2))
        a = (dist > 90).astype(np.float32)
        return m, a
    a = np.where(a < floor, 0, np.clip((a - floor) / (1 - floor), 0, 1))
    F = np.clip((m - (1 - a[..., None]) * B) / np.maximum(a[..., None], 1e-3), 0, 255)
    F[..., 1] = np.minimum(F[..., 1], np.maximum(F[..., 0], F[..., 2]) + 24)  # 초록 번짐 빼기
    return F, a


def import_sheet(path, name, cols, rows, cell, anchor, frames, scale=None, hard=False, labels=None):
    img = np.asarray(Image.open(path).convert("RGB")).astype(np.float32)
    H, W = img.shape[:2]
    cw_px, ch_px = W / cols, H / rows
    s = scale or cw_px / cell[0]
    out_dir = os.path.join(FRAMES_DIR, name)
    os.makedirs(out_dir, exist_ok=True)
    written = []
    for k in range(min(frames, cols * rows)):
        r, c = divmod(k, cols)
        sub = img[int(round(r * ch_px)):int(round((r + 1) * ch_px)), int(round(c * cw_px)):int(round((c + 1) * cw_px))]
        g = _blocks(sub, s)[:cell[1], :cell[0]]
        F, a = key_out(g, hard=hard)
        if not (a > 0).any():
            continue  # 빈 칸 = 그 프레임은 A 그대로
        rgba = np.dstack([F, a * 255]).astype(np.uint8)
        rgba[rgba[..., 3] == 0] = 0
        Image.fromarray(rgba, "RGBA").save(os.path.join(out_dir, f"{k}.png"))
        written.append(k)
    meta = {"frames": frames, "cell": list(cell), "anchor": list(anchor), "source": os.path.basename(path), "written": written}
    if labels:
        meta["labels"] = labels
    with open(os.path.join(out_dir, "meta.json"), "w", encoding="utf-8") as fh:
        json.dump(meta, fh, ensure_ascii=False, indent=1)
    print(f"{name}: {len(written)}/{frames} 프레임 → {out_dir}")


# ── 쓰는 쪽 ────────────────────────────────────────────────────────────────────────────────────────────────
_CACHE = {}


def meta(name):
    p = os.path.join(FRAMES_DIR, name, "meta.json")
    if not os.path.exists(p):
        return None
    with open(p, encoding="utf-8") as fh:
        return json.load(fh)


def frame(name, k):
    """들인 프레임 k → (RGBA uint8, 기준점) 또는 None (그 칸이 비었으면)."""
    m = meta(name)
    if m is None:
        return None
    p = os.path.join(FRAMES_DIR, name, f"{k}.png")
    if not os.path.exists(p):
        return None
    if p not in _CACHE:
        _CACHE[p] = np.asarray(Image.open(p).convert("RGBA"))
    return _CACHE[p], tuple(m["anchor"])


def frame_at(name, age, length):
    """계약 길이 length 의 나이 age → 들인 프레임 (시간축을 늘이거나 줄여서 가장 가까운 것)."""
    m = meta(name)
    if m is None or not m.get("written"):
        return None
    n = m["frames"]
    k = min(n - 1, int(age * n / length))
    return frame(name, k)


def available():
    if not os.path.isdir(FRAMES_DIR):
        return []
    return sorted(d for d in os.listdir(FRAMES_DIR) if meta(d))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("template")
    p.add_argument("kind", choices=["effect", "demon"])
    p.add_argument("out")
    p.add_argument("--cols", type=int, default=6)
    p.add_argument("--rows", type=int, default=2)
    p.add_argument("--cell", type=int, nargs=2, default=(96, 96))
    p.add_argument("--anchor", type=int, nargs=2, default=(48, 72))
    p.add_argument("--scale", type=int, default=3)
    p = sub.add_parser("templates")
    p.add_argument("out_dir")
    p.add_argument("--scale", type=int, default=3)
    p = sub.add_parser("import")
    p.add_argument("sheet")
    p.add_argument("name")
    p.add_argument("--cols", type=int)
    p.add_argument("--rows", type=int)
    p.add_argument("--cell", type=int, nargs=2)
    p.add_argument("--anchor", type=int, nargs=2)
    p.add_argument("--frames", type=int)
    p.add_argument("--scale", type=float)
    p.add_argument("--demon", action="store_true")
    sub.add_parser("list")
    a = ap.parse_args()
    if a.cmd == "template":
        if a.kind == "demon":
            L = demon_layout()
            template(a.out, L["cols"], L["rows"], L["cell"], L["anchor"], a.scale, L["labels"])
        else:
            template(a.out, a.cols, a.rows, tuple(a.cell), tuple(a.anchor), a.scale)
    elif a.cmd == "templates":
        templates(a.out_dir, a.scale)
    elif a.cmd == "import":
        if a.name in [r[0] for r in SHEETS] and a.cols is None:
            _, n, cols, cell, anchor, _ = sheet_spec(a.name)
            a.cols, a.rows, a.cell, a.anchor, a.frames = cols, (n + cols - 1) // cols, cell, anchor, n
        if a.demon:
            L = demon_layout()
            import_sheet(a.sheet, "demon", L["cols"], L["rows"], L["cell"], L["anchor"], len(L["labels"]), a.scale, hard=True,
                         labels=L["labels"])
        else:
            import_sheet(a.sheet, a.name, a.cols, a.rows, tuple(a.cell), tuple(a.anchor), a.frames, a.scale)
    else:
        for n in available():
            m = meta(n)
            print(f"{n}: {len(m['written'])}/{m['frames']} · 칸 {m['cell']} · 기준점 {m['anchor']} · {m['source']}")


if __name__ == "__main__":
    main()
