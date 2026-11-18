#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""영웅서기2 드랍표 — Vercel 에 올리는 자기 완결형 HTML 한 장을 게임 데이터에서 만든다.

사용법:
  python tools_re/droptable/make_drop_site.py                        # 앱 에셋 → out/hero2-drop-table/index.html
  python tools_re/droptable/make_drop_site.py --zip <hero2_ktf.zip> --out <dir>

무엇을 읽나 (전부 앱이 싣는 에셋과 이 저장소의 소스 — 손으로 옮겨 적은 수치는 없다):
  * 아이템 DB `itm/NN`           — tools_re/decode_hero2_items.py (HERO2_ITEM_DB.md)
  * 적 표 `enm/enm` + 처치 루틴    — tools_re/decode_drops.py (HERO2_GUEST_RE.md §8.9)
  * 맵 111장 `P/m/m/*.m.gz`      — tools_re/map_monsters.py (맵 로더 `bf.a([B)V` @0x129a94 의 읽기 순서)
  * 상점 `itm/forshop` · 조합서 `itm/23` · 아이콘 `img/glb` 17번 프레임
  * 앱이 더하는 것 — wie_android/src/guest_anchors.rs 에서 읽는다(무기 보정 · 카드 드랍 제거).
    상수가 소스에서 움직였거나 이름이 바뀌면 여기서 멈춘다: 아무도 안 도는 빌드의 수치를
    인쇄하는 것보다 그 편이 낫다(hero5 tools_re/codex 와 같은 규율).
"""
import argparse
import base64
import datetime
import hashlib
import html
import io
import json
import os
import re
import struct
import sys
import zipfile
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLS_RE = os.path.dirname(HERE)
ROOT = os.path.dirname(TOOLS_RE)
sys.path.insert(0, TOOLS_RE)

import decode_drops as DD  # noqa: E402
import map_monsters  # noqa: E402  — 맵의 몬스터 구간 (에셋 계획기와 같은 코드)
from decode_hero2_items import TYPE_NAME, decode_all  # noqa: E402
from dump_mpd import frames  # noqa: E402

from PIL import Image  # noqa: E402

DEFAULT_ZIP = os.path.join(ROOT, "android_app", "app", "src", "main", "assets", "hero2_ktf.zip")
DEFAULT_OUT = os.path.join(ROOT, "out", "hero2-drop-table")
ANCHORS_RS = os.path.join(ROOT, "wie_android", "src", "guest_anchors.rs")
VERSION_PROPS = os.path.join(ROOT, "android_app", "version.properties")
TEMPLATE = os.path.join(HERE, "template.html")

MODES = ("노말", "하드코어", "헬", "가디언")  # 게임 모드 ag.i 0..3 (★ 2026-10-08 가디언)
# 페이지의 칸(template.html `modes`): 게임 모드 0..3 → 0 · 1 · 2 · 4, 난이도 게이트 칸은 헬 → 3 · 가디언 → 5.
MODE_COLUMN = (0, 1, 2, 4)
GATED_COLUMN = {2: 3, 3: 5}
COLUMNS = 6
GRADES = ("일반", "레어", "에픽", "영웅")
WEAPONS = (0, 1, 2, 3, 4)
ARMOUR = (5, 6, 8, 9)
SHIELD = 10
ACCESSORY = (11, 12)
EQUIP = WEAPONS + ARMOUR + (SHIELD,) + ACCESSORY
STAT_NAMES = ("힘", "체력", "민첩", "의지", "지력")  # body[5..14] 순서 (HERO2_ITEM_DB.md §1)

# 착용 규칙 — 장비 창 `cc.a(BB)V` @0x13de18: 칸마다 허용 타입을 점프표로 고르고(0x13de88),
# 무기 칸은 직업으로 갈린다(0x13df4e · 0x13df80). 방어구는 경갑/중갑 게이트 0x13e288(§8.1).
WEAR = {
    0: ["버서커"], 1: ["버서커"],
    2: ["나이트", "로그"], 3: ["로그"], 4: ["나이트"],
    10: ["나이트"],
    11: ["버서커", "나이트", "로그"], 12: ["버서커", "나이트", "로그"],
}

# 표에서 뺀 것 — 이유는 페이지 각주에 그대로 적는다.
EXCLUDED = {
    (21, 0), (21, 1), (21, 2),  # 마력구 하급·중급·상급 — 네트워크 전용 (2026-09-27 사용자, NO_DROP_TYPES)
    (29, 0),                    # 재료가방 — 가방 자체
}
EXCLUDED_TYPES = {34}           # 스펠카드 — 이 앱에서 카드 드랍이 없다(card_drop_off) · 카드 창도 없다

CATS = [  # (필터 키, 칩 이름, 타입들)
    ("t0", "양손대검", (0,)), ("t1", "양손도끼", (1,)), ("t2", "도검", (2,)), ("t3", "단검", (3,)),
    ("t4", "둔기", (4,)), ("t10", "방패", (10,)), ("t5", "투구", (5,)), ("t6", "갑옷", (6,)),
    ("t8", "장갑", (8,)), ("t9", "신발", (9,)), ("t11", "목걸이", (11,)), ("t12", "반지", (12,)),
    ("enh", "강화 재료", (13, 14, 15, 16, 17, 18, 19, 20)),
    ("craft", "조합서·조합 재료", (23, 30, 31, 32)),
    ("use", "소모품", (21, 22, 24, 25, 26, 27, 28)),
    ("quest", "퀘스트", (33,)),
]
CAT_OF = {t: k for k, _n, ts in CATS for t in ts}

# ★ 2026-10-23 (사용자): 장비가 아닌데도 페이지에 싣는 것 — 인장의 위 두 단계. id 2 = 이름 없는 「인장」(+5~+10, 하급·중급과 한 칸),
#   id 3 = 「상급」(+15~+20, 가디언 칸 15 — make_hero2_asset.py SEAL_TOP_ID). 게임에 「최상급」은 없다.
SEAL_TYPES = (13, 14, 15, 16, 17)  # 힘·체력·민첩·의지·지력의 인장
SEAL_SHOWN = frozenset((t, i) for t in SEAL_TYPES for i in (2, 3))

ITEM_STRIP_FRAME, ITEM_STRIP_CELLS, CELL = 17, 34, 12


# ── 입력 ──────────────────────────────────────────────────────────────────────

def load_asset(path):
    outer = zipfile.ZipFile(path)
    jar_name = next(n for n in outer.namelist() if n.endswith(".jar"))
    jar = zipfile.ZipFile(io.BytesIO(outer.read(jar_name)))
    return outer, jar


def app_version():
    for line in open(VERSION_PROPS, encoding="ascii"):
        m = re.match(r"\s*versionCode\s*=\s*(\d+)", line)
        if m:
            return int(m.group(1))
    raise SystemExit(f"{VERSION_PROPS}: no versionCode")


def rust_constants():
    """앱이 드랍·표시에 더하는 것을 러스트 소스에서 읽는다. 못 찾으면 멈춘다."""
    src = open(ANCHORS_RS, encoding="utf-8").read()
    m = re.search(r"weapon_extra:\s*\[\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\]", src)
    if not m:
        raise SystemExit("guest_anchors.rs: CombatPatch weapon_extra not found — the weapon boost moved; update this generator")
    extra = [int(x) for x in m.groups()]
    m = re.search(r"pub const WEAPON_SHIFT:\s*u8\s*=\s*(\d+);", src)
    if not m:
        raise SystemExit("guest_anchors.rs: CombatPatch::WEAPON_SHIFT not found")
    shift = int(m.group(1))
    # 카드 드랍 제거: bd.q()V 의 `cmp r0,#89` → `#100` (롤 1..100 이라 카드 분기에 영영 안 간다).
    if not re.search(r"\(0x0012_8482,\s*0x2859,\s*0x2864\)", src):
        raise SystemExit("guest_anchors.rs: card_drop_off (0x128482 89->100) not found — spell cards would drop again; model them before publishing")
    # 모드 창 패치는 2026-09-25 에 지워졌다 — 되살아났으면 NORMAL 창이 다섯 칸이 되니 표 전체가 틀린다.
    if re.search(r"fn\s+drop_window_all\s*\(", src):
        raise SystemExit("guest_anchors.rs: drop_window_all is back — NORMAL rolls all five slots; this generator models the shipped windows")
    # ★ 2026-10-02: HELL 보너스 롤 (HellDropPatch) — decode_drops.table_odds(…, 2) 가 슬롯 5·6 을 더한다. 앱에 그 블록이 없으면
    #   7 칸 표의 슬롯 5·6 은 아무도 굴리지 않으니, 페이지가 없는 드랍을 보여 주게 된다(build() 가 확인한다).
    hell_bonus = bool(re.search(r"fn\s+hell_drop_patch\s*\(", src))
    # ★ 2026-10-08: 가디언 패스 — HellDropPatch 의 두 패스(LAST_HELL_SLOT)와 모드 3 을 여는 guardian_patch 가 같이 있어야 칸 9.. 가 굴려진다.
    #   ★ 2026-10-09: the pass boundary must be the one decode_drops models (9 since the guardian recipes).
    last = re.search(r"pub const LAST_HELL_SLOT:\s*u8\s*=\s*(\d+);", src)
    guardian = bool(last) and int(last.group(1)) == DD.HELL_LAST_SLOT and bool(re.search(r"fn\s+guardian_patch\s*\(", src))
    return {"weapon_extra": extra, "weapon_shift": shift, "hell_bonus": hell_bonus, "guardian": guardian, "gear": gear_options(src)}


def gear_options(src):
    """★ 2026-10-09 「가디언」 장비의 개성 옵션 31 — guest_anchors.rs 의 `GearPatch::ITEMS` 표(아이템마다 `text` 와 효과)에서 읽어
    (타입, id) → 문구. 문구의 숫자가 효과와 맞는지는 Rust 셀프테스트(`the_gear_items_are_the_31_and_their_texts_match_their_effects`)가
    잠근다 — 여기서는 옮겨 적지 않는다. 못 찾으면 멈춘다."""
    m = re.search(r"pub const ITEMS: \[GearItem; (\d+)\]", src)
    if not m:
        raise SystemExit("guest_anchors.rs: GearPatch::ITEMS not found — the gear options moved; update this generator")
    items = re.findall(r'GearItem \{ item_type: (\d+), id: (\d+), text: "([^"]*)"', src)
    if len(items) != int(m.group(1)):
        raise SystemExit(f"guest_anchors.rs: GearPatch::ITEMS says {m.group(1)} rows, the page read {len(items)}")
    return {(int(t), int(i)): text for t, i, text in items}

def load_shop(jar):
    blob, out, i = jar.read("itm/forshop"), {}, 0
    while i < len(blob):
        n = blob[i]
        rec = blob[i + 1:i + 1 + n]
        (g,) = struct.unpack_from(">H", rec, 0)
        (price,) = struct.unpack_from("<I", rec, 2)
        out[(g >> 8, g & 0xFF)] = price
        i += 1 + n
    if i != len(blob):
        raise SystemExit("itm/forshop: did not land on EOF")
    return out


def load_icons(jar):
    fr = frames(jar.read("img/glb.mph"), jar.read("img/glb_0.mpd"))
    strip = Image.open(io.BytesIO(fr[ITEM_STRIP_FRAME]["png"])).convert("RGB")
    if strip.size[0] != CELL or strip.size[1] < CELL * ITEM_STRIP_CELLS or strip.size[1] % CELL:  # ★ 2026-10-16: own cells after the 34
        raise SystemExit(f"img/glb frame {ITEM_STRIP_FRAME} is {strip.size}, expected the 12x408 item-type strip")
    icons = {}
    for t in range(35):
        if t == 7:
            continue
        cell = t if t < 8 else t - 1  # 인벤이 그리는 규칙: 7 은 타입이 아니다 (tools/gen_h2_atlas.py)
        im = strip.crop((0, cell * CELL, CELL, (cell + 1) * CELL))
        buf = io.BytesIO()
        im.save(buf, "PNG", optimize=True)
        icons[t] = "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode("ascii")
    return icons


# ── 계산 ──────────────────────────────────────────────────────────────────────

def boosted(v, t, rc):
    """무기 공격력 표시값 = 앱의 게터 래퍼와 같은 식: v + (v·extra[type] >> shift)."""
    if t in WEAPONS:
        return v + ((v * rc["weapon_extra"][t]) >> rc["weapon_shift"])
    return v


def ranges_text(pairs, among=False):
    """[(min,max)×5] → ["힘·체력 +1~3", ...] — 같은 범위는 한 줄로 묶는다.
    among=True 는 무기·방어구: 그 능력치들 **중에서** 0~N개만 붙는다(aa.a([BIZZ)I 0x10d84e)."""
    groups = defaultdict(list)
    for name, (lo, hi) in zip(STAT_NAMES, pairs):
        if lo or hi:
            groups[(lo, hi)].append(name)
    out = []
    for (lo, hi), names in sorted(groups.items(), key=lambda kv: STAT_NAMES.index(kv[1][0])):
        span = f"+{lo}" if lo == hi else f"+{lo}~{hi}"
        who = "·".join(names) if len(names) < 5 else "전 능력치"
        out.append(f"{who}{' 중' if among else ''} {span}")
    return out


def pct(p):
    v = p * 100.0
    if v >= 10:
        return f"{v:.0f}%"
    if v >= 1:
        return f"{v:.1f}%"
    if v >= 0.1:
        return f"{v:.2f}%"
    if v < 0.0005:
        return "<0.001%"
    return f"{v:.3f}%"


def eun_neun(word):
    """받침이 있으면 「은」, 없으면 「는」 (한글이 아니면 「은(는)」)."""
    c = ord(word[-1]) if word else 0
    if 0xAC00 <= c <= 0xD7A3:
        return "은" if (c - 0xAC00) % 28 else "는"
    return "은(는)"


def mode_label(ms):
    ms = tuple(sorted(ms))
    return {(0,): "노말", (1,): "하드코어", (2,): "헬", (1, 2): "하드·헬", (0, 1, 2): "전 모드",
            (0, 1): "노말·하드", (0, 2): "노말·헬", (3,): "헬 · 난이도 NORMAL+", (2, 3): "헬 (일부 난이도 NORMAL+)"}[ms]


def build(zip_path):
    outer, jar = load_asset(zip_path)
    rc = rust_constants()
    if DD.SLOT_WINDOW != {0: (0, 1, 2), 1: (2, 3, 4), 2: (2, 3, 4), 3: (2, 3, 4)}:
        raise SystemExit("decode_drops.SLOT_WINDOW is not the shipped window — this page describes the shipped windows")

    enemies = DD.parse_enm(DD.load_member(zip_path, "enm/enm"))
    if any(DD.hell_odds(e) for e in enemies) and not rc["hell_bonus"]:
        raise SystemExit("enm/enm has HELL bonus slots but guest_anchors.rs has no hell_drop_patch — the page would show drops nobody rolls")
    if any(DD.guardian_odds(e) for e in enemies) and not rc["guardian"]:
        raise SystemExit("enm/enm has guardian slots but guest_anchors.rs has no guardian pass (LAST_HELL_SLOT == decode_drops.HELL_LAST_SLOT + guardian_patch) — the page would show drops nobody rolls")
    maps = map_monsters.load(outer, len(enemies))
    shop = load_shop(jar)
    icons = load_icons(jar)
    rows = decode_all(zip_path)
    items = {(r["typeId"], r["index"]): r for r in rows}

    where = defaultdict(list)                    # 적 인덱스 → [(맵 이름, 마릿수)]
    for idx in sorted(maps):
        mp = maps[idx]
        for g in mp["kinds"]:
            cnt = sum(1 for s in mp["spawns"] if s[0] == g)
            where[g].append([mp["name"], cnt])

    # 조합서: 본문 끝 8 바이트 = (결과 타입, id) + 재료 셋 (ff ff = 없음)
    recipes = {}
    for (t, i), r in items.items():
        if t != 23:
            continue
        body = bytes.fromhex(r["bodyHex"].replace(" ", ""))
        tail = body[1 + body[0]:]
        if len(tail) != 8:
            raise SystemExit(f"조합서[{i}] {r['name']}: tail {tail.hex()} is not 8 bytes")
        out = (tail[0], tail[1])
        mats = [(tail[k], tail[k + 1]) for k in (2, 4, 6) if tail[k] != 0xFF]
        if out not in items or any(m not in items for m in mats):
            raise SystemExit(f"조합서[{i}] {r['name']}: a pointer does not resolve")
        # 조합서 이름 = 결과물 이름 (띄어쓰기만 다를 수 있다: 「바람의 브랜디쉬」 → 바람의브랜디쉬)
        if items[out]["name"].replace(" ", "") != r["name"].replace(" ", ""):
            raise SystemExit(f"조합서[{i}]: named {r['name']!r} but makes {items[out]['name']!r}")
        recipes[i] = (out, mats)
    made_by = {out: (i, mats) for i, (out, mats) in recipes.items()}
    used_in = defaultdict(list)
    for i, (out, mats) in recipes.items():
        for mt in mats:
            used_in[mt].append(items[out]["name"])

    # ── 표 드랍: 처치마다 롤 하나, 모드가 창을 고르고, 칸 안의 id 는 균등 (bd.q()V) ──
    # ★ 2026-10-02 저녁: a HELL bonus slot behind the app's 난이도 gate (decode_drops.gated_slots — the new six
    #   accessories; NORMAL+ since 2026-10-03, HARD only before) is pseudo-mode 3 on the item rows
    #   ("헬 · 난이도 NORMAL+") and tagged on the mob rows (`dg`).
    probs = defaultdict(lambda: defaultdict(lambda: [0.0] * COLUMNS))   # item → mob → [노말, 하드, 헬, 헬·난이도 NORMAL+, 가디언, 가디언·난이도 NORMAL+]
    mob_rows = []
    for e in enemies:
        g = e["index"]
        placed = bool(where.get(g))
        drops = {m: defaultdict(float) for m in range(len(MODES))}
        gated_keys = set()
        for m in range(len(MODES)):
            gated_rows = DD.gated_slots(e, m) if m >= DD.HELL_MODE else []
            for row in DD.table_odds(e, m):
                (_k, t, lo, hi, p) = row
                if t == DD.TYPE_NONE or p <= 0:
                    continue
                share = p / (hi - lo + 1)
                gated = row in gated_rows
                for i in range(lo, hi + 1):
                    key = (t, i)
                    if key not in items:
                        raise SystemExit(f"{e['name']}[{g}] slot points at {key}, which is not in the item DB")
                    if key in EXCLUDED or t in EXCLUDED_TYPES:
                        raise SystemExit(f"{e['name']}[{g}] drops excluded item {key} {items[key]['name']} — the page would hide a real drop")
                    drops[m][key] += share
                    probs[key][g][GATED_COLUMN[m] if gated else MODE_COLUMN[m]] += share
                    if gated:
                        gated_keys.add(f"{t}:{i}")
        lv = [DD.mode_level(e["level"], m) for m in range(len(MODES))]
        mob_rows.append({
            "i": g, "n": e["name"], "lv": lv,
            "maps": where.get(g, []),
            "gate": e["gate"] != 0,
            "boss": e["gate"] == 0 and g >= 89,
            "placed": placed,
            "exp": [x * 17 for x in lv],
            "gold": [x * 4 + 20 for x in lv],
            "d": [[[f"{t}:{i}", round(p, 9)] for (t, i), p in sorted(drops[m].items(), key=lambda kv: -kv[1])] for m in range(len(MODES))],
            "dg": sorted(gated_keys),
        })

    dropping = [mr for mr in mob_rows if mr["gate"] and mr["placed"]]
    low_n = [mr for mr in dropping if mr["lv"][0] <= 24]
    high_n = [mr for mr in dropping if mr["lv"][0] > 24]

    # 표 밖 드랍 (bd.q()V 0x128412..0x1284de) — 몹 개체 레벨 `bd.n` 을 쓴다(하드·헬은 올라간 레벨)
    special = {
        (20, 0): [[[0, 1, 2, 4], "모든 몬스터", "8~40%", "clamp(20 + 몹 레벨 − 내 레벨, 8, 40) % — 나보다 강한 몹일수록 높다"]],
        (26, 0): [[[0, 1, 2, 4], "모든 몬스터", "5%", "물약 10 % 중 절반"]],
        (24, 0): [[[0], f"LV 24 이하 몬스터 {len(low_n)}종", "5%", "물약 10 % 중 절반 — 몹 LV 25 이상이면 하이포션이 대신 나온다"]],
        (25, 0): [[[0, 1, 2, 4], f"LV 25 이상 몬스터 (노말 {len(high_n)}종 · 하드·헬·가디언 전부)", "5%", "물약 10 % 중 절반"],
                  [[2, 4], "헬 · 가디언 모드의 모든 몬스터", "+6%", "헬 이상에서만 따로 굴린다"]],
    }

    # ── 아이템 행 ──
    out_items = []
    for (t, i), r in sorted(items.items()):
        if t in EXCLUDED_TYPES or (t, i) in EXCLUDED:
            continue
        row = {"k": f"{t}:{i}", "n": r["name"], "t": t, "tn": TYPE_NAME[t], "cat": CAT_OF.get(t, ""),
               "g": r.get("grade", -1) if t in EQUIP else -1}
        row["lv"] = r.get("level", 0) if t in EQUIP else 0
        st, op, wear, arm = [], [], [], ""
        if t in WEAPONS:
            lo, hi = boosted(r["powerMin"], t, rc), boosted(r["power"], t, rc)
            st.append(f"공격 {lo}~{hi}" if lo != hi else f"공격 {hi}")
            wear = WEAR[t]
        elif t in ARMOUR or t == SHIELD:
            st.append(f"방어 {r['power']}")
            arm = r.get("armourClass", "")
            if t == SHIELD:
                wear = WEAR[t]
            elif arm == "중갑":
                wear = ["나이트", "버서커*"]
            else:
                wear = ["버서커", "나이트", "로그"]
        elif t in ACCESSORY:
            for lab, key in (("공격", "atk"), ("방어", "def"), ("HP", "hp"), ("SP", "sp")):
                lo, hi = r[key]
                if lo or hi:
                    st.append(f"{lab} {lo}~{hi}" if lo != hi else f"{lab} {lo}")
            wear = WEAR[t]
        if t in EQUIP:
            if t in ACCESSORY:
                op = ranges_text(r["optionRanges"])
            else:
                rng = ranges_text(r["optionRanges"], among=True)
                if rng and r.get("slots", 0) > 0:
                    op = [f"랜덤 옵션 0~{r['slots']}개"] + rng
            if r.get("refineMax"):
                st.append(f"제련 최대 +{r['refineMax']}")
            if r.get("element") and r["element"] != "무속성":
                st.append(f"{r['element']} 속성")
            if r.get("setName") and r["setName"] != "-":
                row["set"] = r["setName"][:-1] + " 세트" if r["setName"].endswith("S") else r["setName"]
        row.update({"st": st, "op": op, "w": wear, "arm": arm})
        if t in ACCESSORY and r.get("desc"):
            row["desc"] = " ".join(r["desc"].split())
        if t in EQUIP and t not in ACCESSORY:
            # ★ 2026-10-09: 가디언 무기 · 방어구의 개성 옵션 — 호스트 블록(GearPatch)이 건다. 장신구는 설명 문구가 이미 말한다.
            # ★★ 2026-10-15: 문구 = GearPatch::ITEMS 의 것(효과와 숫자가 맞는지 Rust 셀프테스트가 잠근다), 그 표에 없는 장비(효과가
            #   다른 블록에 사는 것 — 무모한 칼춤 · 피의 연쇄 …)는 레코드 자신의 옵션 문구(`ch.k`, 게임의 옵션 페이지 글). 전에는
            #   GearPatch 에 없는 2차 장비의 옵션이 페이지에 아예 안 나왔다(사용자: 「피의 연쇄는 효과가 제대로 적혀있지않아」).
            text = rc["gear"].get((t, i)) or " ".join((r.get("desc") or "").split())
            if text:
                row["desc"] = text + " (장착 시)"
        if t not in EQUIP and r.get("desc"):
            row["desc"] = r["desc"].replace(";", " · ") if t != 23 else ""
            if t in SEAL_TYPES:  # 「+2 ~ +7」 / 「+0~+4」 — 레코드마다 띄어쓰기가 달라 한 꼴로
                row["desc"] = re.sub(r"\s*~\s*", "~", row["desc"])

        # 드랍: 몹마다 모드별 확률 → 같은 확률의 모드를 한 줄로
        d = []
        for g, ps in probs.get((t, i), {}).items():
            by_p = defaultdict(list)
            for m, p in enumerate(ps):
                if p > 0:
                    by_p[round(p, 12)].append(m)
            for p, ms in by_p.items():
                d.append([g, ms, p])
        d.sort(key=lambda x: (-x[2], x[1][0], mob_rows[x[0]]["lv"][0]))
        row["d"] = [[g, ms, pct(p), round(p, 9)] for g, ms, p in d]
        row["sp"] = special.get((t, i), [])
        src = []
        if (t, i) in shop:
            row["shop"] = shop[(t, i)]
            src.append("상점")
        if (t, i) in made_by:
            rid, mats = made_by[(t, i)]
            row["craft"] = {"recipe": items[(23, rid)]["name"], "mats": [items[m]["name"] for m in mats]}
            src.append("조합")
        if t == 23:
            out, mats = recipes[i]
            row["makes"] = {"n": items[out]["name"], "mats": [items[m]["name"] for m in mats]}
        if (t, i) in used_in:
            row["used"] = used_in[(t, i)]
        row["src"] = src
        out_items.append(row)

    # ── 자기 검사 — 틀린 페이지를 쓰기 전에 멈춘다 ──
    # ★★ 2026-10-15: 옵션 문구가 있는 장비(레코드에 `ch.k`)는 페이지에도 옵션 줄이 있어야 한다 — GearPatch 표에 없는 장비가 빈칸으로
    #   나가던 것(피의 연쇄)을 다시 막는다.
    blank = [it["n"] for it in out_items if it["t"] in EQUIP and it["t"] not in ACCESSORY and items[tuple(map(int, it["k"].split(":")))].get("desc") and not it.get("desc")]
    if blank:
        raise SystemExit(f"equipment whose option text the page dropped: {blank}")
    for mr in mob_rows:
        for m in range(len(MODES)):
            tot = sum(p for _k, p in mr["d"][m])
            if tot > 1.0 + 1e-9:
                raise SystemExit(f"{mr['n']}: {MODES[m]} table drops sum to {tot:.4f} > 1")
    gear = [it for it in out_items if it["t"] in EQUIP]
    no_source = [it["n"] for it in gear if not it["d"] and not it["src"]]
    if no_source:
        raise SystemExit(f"equipment with no source at all: {no_source[:8]} … — the level-rule plan says 0 (HERO2_DROP_GAPS.md)")
    if any(g for g in probs for mob in probs[g] if mob_rows[mob]["boss"]):
        raise SystemExit("a boss (drop gate 0) shows a table drop — gate handling is wrong")
    # 드랍표는 있는데 어느 맵의 몬스터 목록에도 없는 적만 떨구는 장비 — 각주가 이름을 댄다.
    orphans = [it["n"] for it in gear
               if it["d"] and not it["src"] and not any(mob_rows[e[0]]["placed"] for e in it["d"])]
    unplaced = [mr["n"] for mr in mob_rows if mr["gate"] and not mr["placed"]]
    seals = {it["k"]: it for it in out_items if tuple(map(int, it["k"].split(":"))) in SEAL_SHOWN}
    if len(seals) != len(SEAL_SHOWN):
        raise SystemExit(f"seals: the page wants {len(SEAL_SHOWN)}, the item DB has {sorted(seals)}")
    dry = [it["n"] for it in seals.values() if not any(mob_rows[e[0]]["placed"] for e in it["d"])]
    if dry:
        raise SystemExit(f"seals no placed monster drops: {dry} — the seal rows would say 「드랍 없음」")

    return {
        "items": out_items,
        "mobs": mob_rows,
        "icons": {str(t): u for t, u in icons.items()},
        "cats": [[k, n] for k, n, _ts in CATS],
        "meta": {
            "vc": app_version(),
            "date": datetime.date.today().isoformat(),
            "asset_md5": hashlib.md5(open(zip_path, "rb").read()).hexdigest(),
            "boost": rc["weapon_extra"],
            "orphans": orphans,
            "unplaced": unplaced,
            "counts": {
                "gear": [sum(1 for it in gear if it["g"] == k) for k in range(4)],
                "gear_total": len(gear),
                "dropping_items": sum(1 for it in out_items if it["d"] or it["sp"]),
                "mobs": len(dropping),
                "maps": sum(1 for mp in maps.values() if any(mob_rows[g]["gate"] for g in mp["kinds"])),
                "bosses": sum(1 for mr in mob_rows if mr["boss"] and mr["placed"]),
            },
        },
    }


def render(data):
    tpl = open(TEMPLATE, encoding="utf-8").read()
    page_data = {
        "items": [it for it in data["items"] if it["g"] >= 1 or tuple(map(int, it["k"].split(":"))) in SEAL_SHOWN],
        "mobs": [{"n": mob["n"], "placed": mob["placed"]} for mob in data["mobs"]],
    }
    blob = json.dumps(page_data, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
    if "{{DATA}}" not in tpl:
        raise SystemExit("template.html: placeholder {{DATA}} missing")
    tpl = tpl.replace("{{DATA}}", blob)
    if "{{" in tpl:
        raise SystemExit("template.html: an unfilled placeholder remains")
    return tpl


def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description="영웅서기2 드랍표 사이트 생성기")
    ap.add_argument("--zip", default=DEFAULT_ZIP)
    ap.add_argument("--out", default=DEFAULT_OUT)
    ap.add_argument("--json", help="데이터만 JSON 으로도 쓴다 (검토용)")
    a = ap.parse_args()
    data = build(a.zip)
    page = render(data)
    os.makedirs(a.out, exist_ok=True)
    path = os.path.join(a.out, "index.html")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(page)
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
    c = data["meta"]["counts"]
    print(f"wrote {path} ({len(page.encode('utf-8')):,} B) — vC {data['meta']['vc']}, "
          f"items {len(data['items'])} (gear {c['gear_total']} = {c['gear']}), dropping {c['dropping_items']}, "
          f"monsters {c['mobs']} on {c['maps']} maps, bosses {c['bosses']}")


if __name__ == "__main__":
    main()
