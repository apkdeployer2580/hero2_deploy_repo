#!/usr/bin/env python3
"""hero2 (영웅서기2 KTF 1.0.7) 드랍 디코더 — `enm/enm` + 처치 루틴 `bd.q()V` 를 게임 그대로.

── 왜 이 도구인가 (2026-09-24, #20·#21) ──────────────────────────────────────
2026-09-23 의 #20 조사는 `enm/enm` 의 드랍 슬롯 440 개만 보고 「578 개 중 190 개가 안 나온다」
를 냈다. 그런데 처치 루틴 `bd.q()V` @0x128080 을 끝까지 읽으면 드랍은 **셋**이다:

  ① 표 드랍 — 한 번의 롤 `h.a(1, 0x7fff)` = 1 + |Random.nextInt()| % 32767 을 **고정 분모 32767**
     에 대 슬롯의 누적값 구간에 넣는다. ★ 그런데 **모드가 창을 고른다**: `ag.i:B`(0 NORMAL ·
     1 HARDCORE · 2 HELL, `ag.y()V` 가 1씩 올리고 2 에서 멈춘다) 가 0 이면 슬롯 **0·1·2**, 아니면
     **2·3·4** 만 본다. 슬롯 k 는 `cum[k-1] < roll < cum[k]`(양쪽 엄격, cum[-1] = 0).
     슬롯 타입 29 는 23(조합서)으로 바뀌고, 7 은 「없음」이다.
  ② 하드코딩 드랍 — 표와 **독립인** 롤들(전부 `h.a(1,100)`):
       65 %  골드 (몹레벨 × 4 + 20)
       clamp(20 + 몹레벨 − 내레벨, 8, 40) %  제련석 (20:0)
       10 %  절반 포도주 (26:0) · 절반 포션 (24:0) — 몹레벨 > 24 면 하이포션 (25:0)
       11 %  스펠카드 (34 : max(적인덱스/3 − 1, 0)) — 단 id 11·12 는 건너뛴다
       HELL 만 6 %  하이포션 (25:0)
  ③ 전부가 **`cq.f:S`(body[15..16]) != 0** 인 적에게만 일어난다 — 0 이면 표·골드·물약·카드 전부 없다.

그래서 「안 나온다」는 **모드별로** 물어야 하고, 제련석·포션·하이포션·포도주·스펠카드는
표에 없어도 나온다. `--gaps` 가 그 목록을 낸다.

── 레코드 = `cq.a([BBB)V` @0x144ee4 가 읽는 순서 ─────────────────────────────
  파일:  [u16 BE 길이][버전 4B "1.03"/"1.02"][u8 nameLen][name EUC-KR][body]
  body:  [0] cq.a:B  [1] cq.d:B  [2] cq.e:B  [3] cq.f:B  [4] ★ 레벨 cq.h:B
         [5..6] cq.a:S  [7..8] cq.b:S  [9] cq.e:S  [10] cq.i:B  [11..12] cq.g:S
         [13] cq.j:B  [14] cq.k:B  [15..16] ★ 드랍 게이트 cq.f:S  [17] cq.c:S
         [18..19] cq.d:S  [20..22] 플래그 셋(==1)  [23..32] 외형(cf.a([BIBBBB)V)
         [33] ★ 슬롯 수 n  [34 .. 34+5n) ★ 슬롯 (u8 타입, u8 id최소, u8 id최대, u16 LE 누적)
         그 뒤 cq.g:B · cq.l:B  →  body 길이 = 36 + 5n  (n = 0 이면 36, 5 면 61)
  u16 는 `h.a([BI)S` = (short)(b[i] | b[i+1] << 8) — LE, **부호 있음**(32767 이 천장).
  ★ `cq.b:B` = 이 레코드의 **파일 안 인덱스** — 맵 로더 `bf.a([B)V` 가 `cq.a(rec, 지역번호, 전역번호)`
    로 넘긴다. 스펠카드 id 가 이것에서 나온다.
  ★ HARDCORE/HELL 에서는 `cq.a()V` 가 레벨을 `2·lv/5 + 20·(모드 > 1 ? 2 : 1) + 20` (상한 99) 로 올린다
    — 물약 종류(> 24)와 제련석 확률이 그 레벨을 쓴다.

사용법:
  python tools_re/decode_drops.py <hero2_ktf.zip>                 # 적마다 모드별 표 드랍 확률
  python tools_re/decode_drops.py <hero2_ktf.zip> --gaps [out.md] # 모드별 「안 나오는」 아이템
  python tools_re/decode_drops.py <hero2_ktf.zip> --levels        # 적 레벨 대비 장비 레벨 점검 (위반 = exit 1)
  python tools_re/decode_drops.py <hero2_ktf.zip> --grades        # 등급별(일반/레어/에픽/영웅) 슬롯 확률과 처치당 평균
  python tools_re/decode_drops.py <hero2_ktf.zip> --bands         # 모드별 처치당 장비를 아이템 레벨 대역으로 + 그 모드 몬스터 레벨에 맞는 몫
  python tools_re/decode_drops.py <hero2_ktf.zip> --tiers         # 모드 × 티어(종결·상급·중간·일반·장신구)의 처치당 평균과 출처 수
  python tools_re/decode_drops.py <hero2_ktf.zip> --json out.json

★ 2026-10-02 (사용자: 「하드코어가 헬보다 템이 압도적으로 잘 나온다」): HARDCORE 와 HELL 은 **같은 창 2·3·4 를 같은
  롤로** 굴린다(`bd.q()V` 의 분기는 `ag.i > 0` 하나) — 처치당 장비는 한 숫자였다. 다른 것은 몬스터 레벨(HELL +20)뿐이라
  같은 아이템이 HELL 에서는 한참 아래 레벨이 된다. `--bands` 가 그 몫을 낸다(「맞는」 = 아이템 LV ≥ 그 모드 몬스터 LV − 10).
  → 같은 날 HELL 보너스 롤을 더했다(사용자: 「헬 장비드랍이 하드코어보다 조금 더 좋아야해」 · 「장신구도 잘 나와야해」):
  7 칸 레코드의 슬롯 5 · 6 을 HELL 에서만 처치 루틴 뒤에 한 번 더 굴린다(자기 축) — `table_odds(…, 2)` 가 그 몫을 더한다
  (`hell_odds`). 정본 = HERO2_GUEST_RE.md §8.9 끝 「HARDCORE 와 HELL」.
★ 2026-10-02 저녁 — 드랍 티어 (사용자: 「종결아이템급은 드랍률을 좀 낮추고 … 하드코어에서는 거의 안 나오고 헬에서는 1 %」 ·
  「붉은 사자세트 같은 중간급은 올려서」 · 「신규 6종 장신구는 0.1 % 수준」): 장비 슬롯의 폭이 등급이 아니라 **티어**(등급 + 레벨 + 세트,
  `make_hero2_asset.py` `tier_of`)를 따르고, 보너스 칸이 넷(5 상급 · 6 종결 장신구 · 7 종결 장비 · 8 신규 6종 — 타입 바이트의
  GATE 비트 = 앱의 환경설정 난이도 NORMAL 이상에서만 — ★ 2026-10-03 전엔 HARD 에서만)이 됐다. `--tiers` 가 모드 × 티어의 처치당 평균을 낸다.

★ 2026-09-26: 장비 슬롯의 폭이 등급 표가 됐다(`make_hero2_asset.py` GRADE_WIDTH — 일반 10 · 레어 4 · 에픽 3 · 영웅 2 %).
  `--grades` 가 그것을 확인한다: 슬롯 확률은 등급마다 한 값이어야 하고(섞인 슬롯은 따로 센다), 처치당 평균은 모드마다
  적 전체의 산술평균이다(사냥터에 따라 다르다 — 적마다의 값은 인수 없이 돌린 표에 있다).
"""
import argparse
import io
import json
import os
import struct
import sys
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from decode_hero2_items import TYPE_NAME, decode_all  # noqa: E402
import map_monsters  # noqa: E402

ROLL_MAX = 0x7FFF  # h.a(1, 0x7fff) — the roll is 1..32767
MODES = ("NORMAL", "HARDCORE", "HELL", "GUARDIAN")  # ★ 2026-10-08: 3 = 가디언
# The window (ag.i == 0 ? 0..2 : 2..4). ⛔ The app rolls it AS SHIPPED again (2026-09-25):
# #20's `drop_window_all` patch (every slot in every mode) handed NORMAL each enemy's
# HARDCORE/HELL gear — LV 36~73 items from LV 2~4 monsters — and was removed.
SHIPPED_WINDOW = {0: (0, 1, 2), 1: (2, 3, 4), 2: (2, 3, 4), 3: (2, 3, 4)}  # the wrapper runs the template as HELL
SLOT_WINDOW = SHIPPED_WINDOW
# ★ 2026-10-02: HELL's bonus roll (guest_anchors::HellDropPatch, make_hero2_asset.py HELL 보너스 칸) — after the shipped
# table drop, HELL rolls 1..32767 once more against slots 5 · 6 of a seven-slot record, ON THEIR OWN AXIS from 0
# (slot 5 = 0 < roll < cum5, slot 6 = cum5 < roll < cum6). Independent of the table roll, so its odds ADD.
HELL_MODE = 2
HELL_FIRST_SLOT = 5
# ★ 2026-10-08 가디언 (guest_anchors::GuardianPatch · HellDropPatch::LAST_HELL_SLOT): the HELL pass ends at slot 8; in mode 3
# the block rolls ONCE MORE over the slots after it (9..end, their own axis from 0). The template scale = HELL + 27 levels
# HP ×1.5, attack / defence ×1.2 — the drop windows are HELL's (the wrapper runs cq.a()V as HELL).
# ★ 2026-10-09: the HELL pass ends at slot 9 (the guardian recipes) — the guardian pass is 10..end.
GUARDIAN_MODE = 3
HELL_LAST_SLOT = 9
# ★ 2026-10-08 저녁 (user: 「lv 70언저리에서 70~80 … 90언저리에서 90~105」): the guardian level stretches HELL's 60..80 onto
# 70..105 — `70 + (hell − 60) × 448 / 256`, cap 105 (GuardianPatch::{HELL_BASE, LEVEL_BASE, LEVEL_MUL, LEVEL_CAP}).
GUARDIAN_HELL_BASE, GUARDIAN_LEVEL_BASE, GUARDIAN_LEVEL_MUL, GUARDIAN_LEVEL_CAP = 60, 70, 448, 105
# ★ 2026-10-02 저녁 (드랍 티어): the bonus slots run from 5 to the record's end (the asset's 5·6·7·8); a slot whose
# type byte carries GATE (0x80) rolls only while the app's 환경설정 난이도 is NORMAL or above (HellDropPatch::GATE /
# gate_open; HARD only until 2026-10-03) — the new six accessories. The reports count it as HELL (난이도 NORMAL+);
# `hell_odds(e, gate=False)` leaves it out.
GATE = 0x80
# The level rule `make_hero2_asset.py` plans by: gear may be at most this many levels above
# the level the slot is rolled at (that mode's enemy level). `--levels` checks it.
LEVEL_UP = 5
GEAR_TYPES = frozenset({0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12})
# Not drops by design: 하급·중급·상급 마력구 — network-only (user 2026-09-27; #20 had made them drop and was
# reverted) —, 망각의 구 ×2 (#22) and 부활의 구 (2026-09-25) — the host's shop items —, the 재료가방 record, and the
# three spell cards the kill routine cannot produce (11·12 skipped by id, 29 = boss indices).
NOT_DROPS = {(21, 0), (21, 1), (21, 2), (21, 3), (21, 4), (21, 5), (29, 0), (34, 11), (34, 12), (34, 29)}
# Made, not dropped: the six 오브 come out of recipes 52..57 (which do drop).
CRAFTED = {(18, i) for i in range(6)}
TYPE_REMAP = {29: 23}  # bd.q()V @0x12836a: 29 -> 23
TYPE_NONE = 7  # bd.q()V @0x1283d6: 7 is "no drop" (itm/07 does not exist)
SPELL_CARD, SPELL_SKIP = 34, (11, 12)
HARDCODED_ALWAYS = {(20, 0), (24, 0), (25, 0), (26, 0)}  # 제련석 · 포션 · 하이포션 · 포도주


def load_member(source, name):
    with zipfile.ZipFile(source) as outer:
        jar = next((n for n in outer.namelist() if n.endswith(".jar")), None)
        if jar is None:
            raise SystemExit("%s: no .jar member (is this the asset zip?)" % source)
        with zipfile.ZipFile(io.BytesIO(outer.read(jar))) as j:
            return j.read(name)


def s16(b, i):
    return struct.unpack_from("<h", b, i)[0]


def parse_enm(blob):
    """`enm/enm` → 레코드 목록. EOF 에 정확히 착지하지 않으면 예외."""
    out, i, idx = [], 0, 0
    while i < len(blob):
        (ln,) = struct.unpack_from(">H", blob, i)
        rec = blob[i + 2 : i + 2 + ln]
        if len(rec) != ln:
            raise ValueError("record %d overruns EOF" % idx)
        i += 2 + ln
        ver = rec[:4].decode("ascii", "replace")
        nl = rec[4]
        name = rec[5 : 5 + nl].decode("euc-kr", "replace")
        body = rec[5 + nl :]
        n = body[33]
        if len(body) != 36 + 5 * n:
            raise ValueError("record %d (%s): body %d B but n=%d wants %d" % (idx, name, len(body), n, 36 + 5 * n))
        slots = []
        for k in range(n):
            t, lo, hi = body[34 + 5 * k], body[35 + 5 * k], body[36 + 5 * k]
            slots.append((t, lo, hi, s16(body, 37 + 5 * k)))
        out.append({
            "index": idx,
            "ver": ver,
            "name": name,
            "level": body[4],
            "gate": s16(body, 15),
            "family": body[27],
            "slots": slots,
        })
        idx += 1
    return out


def mode_level(level, mode):
    """`cq.a()V`: the level the game actually fights with in that mode."""
    if mode == 0:
        return level
    hell = min(level * 2 // 5 + 20 * (2 if mode > 1 else 1) + 20, 99)
    if mode >= GUARDIAN_MODE:
        return min(GUARDIAN_LEVEL_BASE + (max(hell - GUARDIAN_HELL_BASE, 0) * GUARDIAN_LEVEL_MUL >> 8), GUARDIAN_LEVEL_CAP)
    return hell


def table_odds(enemy, mode):
    """[(slot k, type, lo, hi, p)] for the slots the mode's window reads, p = P(this slot)."""
    if enemy["gate"] == 0 or not enemy["slots"]:
        return []
    cum = [c for (_, _, _, c) in enemy["slots"]]
    out = []
    for k in SLOT_WINDOW[mode]:
        if k >= len(cum):
            continue
        lo_c = cum[k - 1] if k > 0 else 0
        hits = max(0, min(cum[k], ROLL_MAX + 1) - max(lo_c, 0) - 1)  # lo < roll < hi, roll in 1..32767
        if k == 0:
            hits = max(0, min(cum[0], ROLL_MAX + 1) - 1)
        t, lo, hi, _ = enemy["slots"][k]
        out.append((k, TYPE_REMAP.get(t, t), lo, hi, hits / ROLL_MAX))
    if mode >= HELL_MODE:
        out += hell_odds(enemy)
    if mode >= GUARDIAN_MODE:
        out += guardian_odds(enemy)
    return out


def hell_odds(enemy, gate=True):
    """[(slot k, type, lo, hi, p)] of HELL's bonus roll — the slots 5..HELL_LAST_SLOT on their own axis (every slot to
    the record's end until 2026-10-08) — or [] for a record without them. A GATE-typed slot (the new six, 난이도 NORMAL+
    only) counts when `gate`; the type comes back without the bit. A width-0 slot (type 7 "nothing") is left out."""
    return _pass_odds(enemy, HELL_FIRST_SLOT, HELL_LAST_SLOT + 1, gate)


def guardian_odds(enemy, gate=True):
    """★ 2026-10-08: the guardian pass — the slots after HELL_LAST_SLOT, their own axis — rolled once more in mode 3."""
    return _pass_odds(enemy, HELL_LAST_SLOT + 1, len(enemy["slots"]), gate)


def _pass_odds(enemy, first, end, gate):
    if enemy["gate"] == 0 or len(enemy["slots"]) <= first:
        return []
    out, prev = [], 0
    for k in range(first, min(end, len(enemy["slots"]))):
        t, lo, hi, cum = enemy["slots"][k]
        # both bounds strict: slot 5 = 1..cum5−1, slot 6 = cum5+1..cum6−1, …
        p = max(0, min(cum, ROLL_MAX + 1) - prev - 1) / ROLL_MAX
        prev = cum
        gated = bool(t & GATE)
        t &= ~GATE
        if p <= 0 or t == TYPE_NONE or (gated and not gate):
            continue
        out.append((k, TYPE_REMAP.get(t, t), lo, hi, p))
    return out


def gated_slots(enemy, mode=HELL_MODE):
    """The bonus slots that wait for the 난이도 gate (NORMAL+): [(slot k, type, lo, hi, p)] — HELL's, and in mode 3 the guardian pass's."""
    rows = [row for row in hell_odds(enemy, True) if row not in hell_odds(enemy, False)]
    if mode >= GUARDIAN_MODE:
        rows += [row for row in guardian_odds(enemy, True) if row not in guardian_odds(enemy, False)]
    return rows


def spell_card_of(index):
    q = index // 3
    card = q - 1 if q > 0 else 0
    return None if card in SPELL_SKIP else card


def coverage(enemies, mode, placed=None):
    """(타입, id) → 그 모드에서 그것을 떨어뜨리는 적 이름들.
    `placed` 가 주어지면 그 적(어느 맵이 배치하는 적 — map_monsters.placed)만 센다: 신관(#23)·신관병(#24)은
    드랍 게이트가 있지만 어느 맵에도 없어서, 세면 그 둘만 떨구는 아이템이 「나온다」로 잘못 잡힌다(2026-09-28)."""
    cov = {}
    for e in enemies:
        if e["gate"] == 0 or (placed is not None and e["index"] not in placed):
            continue
        for (_k, t, lo, hi, p) in table_odds(e, mode):
            if t == TYPE_NONE or p <= 0:
                continue
            for i in range(lo, hi + 1):
                cov.setdefault((t, i), set()).add(e["name"])
        card = spell_card_of(e["index"])
        if card is not None:
            cov.setdefault((SPELL_CARD, card), set()).add(e["name"])
        for key in HARDCODED_ALWAYS:
            if key == (25, 0) and mode < HELL_MODE and mode_level(e["level"], mode) <= 24:
                continue  # 하이포션 needs a level-25+ monster outside HELL's bonus roll
            if key == (24, 0) and mode_level(e["level"], mode) > 24:
                continue  # 포션 only from level <= 24
            cov.setdefault(key, set()).add(e["name"])
    return cov


def level_report(enemies, items):
    """Per mode, per enemy: the levels of the gear its window can drop, against the level it
    fights at. A slot item above that level + LEVEL_UP is a violation (exit 1)."""
    bad = 0
    for m in range(len(MODES)):
        print("## %s — 창 %s" % (MODES[m], SLOT_WINDOW[m]))
        for e in enemies:
            odds = table_odds(e, m)
            if not odds:
                continue
            lv = mode_level(e["level"], m)
            parts, worst = [], None
            # A slot's ceiling is the HIGHEST level it is rolled at: 3·4 roll in HARDCORE and
            # HELL alike, and the shipped rare slot 4 is tuned to HELL (make_hero2_asset.py
            # `_slot_refs` plans by the same rule).
            # HELL's bonus slots 5..8 roll at the HELL level only; ★ 2026-10-08 the guardian pass (9..) at the guardian level.
            ceiling = lambda k: (mode_level(e["level"], 3) if k > HELL_LAST_SLOT else mode_level(e["level"], 2) if k >= 3 else e["level"]) + LEVEL_UP  # noqa: E731
            for (k, t, lo, hi, p) in odds:
                if t not in GEAR_TYPES or p <= 0:
                    continue
                lvs = [items[(t, i)].get("level") for i in range(lo, hi + 1) if (t, i) in items]
                lvs = [x for x in lvs if x is not None]
                if not lvs:
                    continue
                top = max(lvs)
                worst = top if worst is None else max(worst, top)
                flag = " ⛔" if top > ceiling(k) else ""
                bad += 1 if flag else 0
                parts.append("%d:%s%d-%d LV%d-%d %.2f%%%s" % (k, TYPE_NAME.get(t, "?")[:2], lo, hi, min(lvs), top, p * 100, flag))
            print("  [%3d] %-12s LV%-3d 장비최고 LV%-3s | %s" % (e["index"], e["name"], lv, worst if worst is not None else "-", " | ".join(parts)))
    print("위반 %d" % bad)
    return 1 if bad else 0


LEVEL_BANDS = ((0, 39), (40, 49), (50, 59), (60, 99))
# 「맞는」 장비 = 아이템 레벨이 그 모드에서 싸우는 몬스터 레벨 − FIT_MARGIN 이상 (`--bands`).
FIT_MARGIN = 10


def band_report(enemies, items, placed):
    """Per mode, over every placed enemy that drops: the mean per-kill chance of gear by the
    item's level band, and of gear that fits the level the mode's monsters fight at. HARDCORE
    and HELL read one window; HELL adds its bonus roll (slots 5 · 6) on top."""
    for m in range(len(MODES)):
        per = [0.0] * len(LEVEL_BANDS)
        gear = fit = 0.0
        lvs = []
        for e in enemies:
            if e["index"] not in placed:
                continue
            odds = table_odds(e, m)
            if not odds:
                continue
            lv = mode_level(e["level"], m)
            lvs.append(lv)
            for (_k, t, lo, hi, p) in odds:
                if t not in GEAR_TYPES or p <= 0:
                    continue
                ids = [i for i in range(lo, hi + 1) if (t, i) in items and items[(t, i)].get("level") is not None]
                for i in ids:
                    il, share = items[(t, i)]["level"], p / len(ids)
                    gear += share
                    fit += share if il >= lv - FIT_MARGIN else 0.0
                    for b, (a, z) in enumerate(LEVEL_BANDS):
                        if a <= il <= z:
                            per[b] += share
        n = max(len(lvs), 1)
        print("## %-8s 몬스터 LV%d..%d (%d 적) 처치당 장비 %.2f %% = %s | 몬스터 LV−%d 이상 %.2f %% (%s 처치당 1개)" % (
            MODES[m], min(lvs), max(lvs), len(lvs), gear / n * 100,
            " · ".join("LV%d-%d %.2f %%" % (a, z, per[b] / n * 100) for b, (a, z) in enumerate(LEVEL_BANDS)),
            FIT_MARGIN, fit / n * 100, ("%.0f" % (n / fit)) if fit else "-"))
    return 0


GRADE_NAMES = ("일반", "레어", "에픽", "영웅")  # t/itmtp.tdf[35..38]


def grade_report(enemies, items):
    """Per gear slot: its probability by the grade it holds (a one-grade slot) — then, per
    mode, the mean per-kill chance of each grade over every enemy that drops at all."""
    by_grade, mixed = {}, 0
    for e in enemies:
        for m in (0, 1):
            for (k, t, lo, hi, p) in table_odds(e, m):
                if t not in GEAR_TYPES or p <= 0 or (m == 1 and k == 2):  # slot 2 is in both windows: count it once
                    continue
                gs = {items[(t, i)]["grade"] for i in range(lo, hi + 1) if (t, i) in items}
                if len(gs) == 1:
                    by_grade.setdefault(gs.pop(), set()).add(round(p * 100, 3))
                else:
                    mixed += 1
    print("## 슬롯 확률 (등급이 하나인 장비 슬롯)")
    for g in sorted(by_grade):
        print("  %s  %s %%" % (GRADE_NAMES[g], " · ".join("%.3f" % v for v in sorted(by_grade[g]))))
    print("  섞인 슬롯 %d개 (각 아이템이 제 등급 몫을 받는 평균 폭)" % mixed)
    for m in range(len(MODES)):
        per = [0.0] * 4
        n = 0
        for e in enemies:
            odds = table_odds(e, m)
            if not odds:
                continue
            n += 1
            for (_k, t, lo, hi, p) in odds:
                if t not in GEAR_TYPES or p <= 0:
                    continue
                for i in range(lo, hi + 1):
                    if (t, i) in items:
                        per[items[(t, i)]["grade"]] += p / (hi - lo + 1)
        per = [x / n for x in per]
        rare = sum(per[1:])
        print("## %-8s 처치당 평균 (%d 적): %s | 레어 이상 %.3f %% = %s 처치당 1개" % (
            MODES[m], n, " · ".join("%s %.3f %%" % (GRADE_NAMES[g], per[g] * 100) for g in range(4)), rare * 100,
            ("%.0f" % (1 / rare)) if rare else "-"))
    return 0


def tier_report(enemies, items, placed):
    """Per mode (HELL = 난이도 NORMAL+, the gated slot in), over every placed enemy that drops: the mean per-kill
    chance of each tier (`make_hero2_asset.tier_of`), the kills per item and how many enemies carry one."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from make_hero2_asset import TIER_NAME, tier_of  # noqa: E402

    tiers = list(TIER_NAME)
    tier = {key: tier_of(key[0], r["level"], r["grade"], r.get("setId", 0)) for key, r in items.items()}
    counts = {t: sum(1 for v in tier.values() if v == t) for t in tiers}
    for m in range(len(MODES)):
        per, carriers, n = {t: 0.0 for t in tiers}, {t: set() for t in tiers}, 0
        for e in enemies:
            if e["index"] not in placed:
                continue
            odds = table_odds(e, m)
            if not odds:
                continue
            n += 1
            for (_k, t, lo, hi, p) in odds:
                if t not in GEAR_TYPES or p <= 0:
                    continue
                ids = [i for i in range(lo, hi + 1) if (t, i) in items]
                for i in ids:
                    per[tier[(t, i)]] += p / len(ids)
                    carriers[tier[(t, i)]].add(e["index"])
        print("## %-8s (%d 적, 처치당 평균%s)" % (MODES[m], n, " — 난이도 NORMAL+ 칸 포함" if m >= HELL_MODE else ""))
        for t in tiers:
            v = per[t] / max(n, 1)
            print("   %-16s %3d종  %7.3f %%  %s  떨구는 적 %d" % (TIER_NAME[t], counts[t], v * 100, ("= %6.0f 처치당 1개" % (1 / v)) if v else "  (안 나옴)      ", len(carriers[t])))
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("source", help="hero2_ktf.zip")
    ap.add_argument("--gaps", nargs="?", const="-", help="모드별 「안 나오는」 아이템 (파일 또는 -)")
    ap.add_argument("--json", help="적 표를 JSON 으로")
    ap.add_argument("--levels", action="store_true", help="모드마다 적 레벨 대비 장비 레벨 (규칙 위반이 있으면 exit 1)")
    ap.add_argument("--grades", action="store_true", help="등급별 슬롯 확률과 모드별 처치당 평균")
    ap.add_argument("--bands", action="store_true", help="모드별 처치당 장비를 아이템 레벨 대역으로 + 몬스터 레벨에 맞는 몫")
    ap.add_argument("--tiers", action="store_true", help="모드 × 티어의 처치당 평균 (HELL 은 난이도 NORMAL+ 칸 포함)")
    a = ap.parse_args()
    # ⛔ EUC-KR names through a cp949 console come out as mojibake — always UTF-8 out.
    sys.stdout.reconfigure(encoding="utf-8")

    enemies = parse_enm(load_member(a.source, "enm/enm"))
    # 퀘스트는 스크립트가 준다 · NOT_DROPS / CRAFTED 는 머리의 주석
    items = [r for r in decode_all(a.source) if r["typeId"] != 33 and (r["typeId"], r["index"]) not in NOT_DROPS | CRAFTED]

    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump(enemies, f, ensure_ascii=False, indent=1)

    if a.levels:
        return level_report(enemies, {(r["typeId"], r["index"]): r for r in decode_all(a.source)})
    if a.grades:
        graded = {(r["typeId"], r["index"]): r for r in decode_all(a.source) if r["typeId"] in GEAR_TYPES and "grade" in r}
        return grade_report(enemies, graded)
    if a.bands:
        with zipfile.ZipFile(a.source) as z:
            placed = map_monsters.placed(map_monsters.load(z, len(enemies)))
        return band_report(enemies, {(r["typeId"], r["index"]): r for r in decode_all(a.source) if r["typeId"] in GEAR_TYPES}, placed)
    if a.tiers:
        with zipfile.ZipFile(a.source) as z:
            placed = map_monsters.placed(map_monsters.load(z, len(enemies)))
        graded = {(r["typeId"], r["index"]): r for r in decode_all(a.source) if r["typeId"] in GEAR_TYPES and "grade" in r}
        return tier_report(enemies, graded, placed)

    if a.gaps is None:
        lines = []
        for e in enemies:
            head = "[%3d] %-12s LV%-3d gate %-5d" % (e["index"], e["name"], e["level"], e["gate"])
            if e["gate"] == 0 or not e["slots"]:
                lines.append(head + "  (드랍 없음)")
                continue
            per = []
            for m in range(len(MODES)):
                odds = table_odds(e, m)
                tot = sum(p for (*_x, p) in odds)
                per.append("%s %4.1f%%" % (MODES[m][:4], tot * 100))
            gated = sum(p for (*_x, p) in gated_slots(e, len(MODES) - 1))
            if gated:
                per[-1] += " (난이도 NORMAL+ 칸 %.2f%% 포함)" % (gated * 100)
            card = spell_card_of(e["index"])
            lines.append(head + "  표 " + " / ".join(per) + "  카드 " + ("-" if card is None else str(card)))
        sys.stdout.write("\n".join(lines) + "\n")
        return 0

    # 「나온다」 = 어느 맵이 배치하는 적에게서 (2026-09-28 — 드랍표가 있어도 맵에 없는 적은 셈에서 뺀다)
    with zipfile.ZipFile(a.source) as z:
        placed = map_monsters.placed(map_monsters.load(z, len(enemies)))
    covs = [coverage(enemies, m, placed) for m in range(len(MODES))]
    out = []
    for m in range(len(MODES)):
        missing = [r for r in items if (r["typeId"], r["index"]) not in covs[m]]
        out.append("## %s — %d / %d 안 나옴" % (MODES[m], len(missing), len(items)))
        by_type = {}
        for r in missing:
            by_type.setdefault(r["typeId"], []).append(r)
        for t in sorted(by_type):
            rows = by_type[t]
            out.append("  %-8s %2d  %s" % (TYPE_NAME.get(t, "?"), len(rows), " ".join("[%d]" % r["index"] for r in rows)))
    text = "\n".join(out) + "\n"
    if a.gaps == "-":
        sys.stdout.write(text)
    else:
        with open(a.gaps, "w", encoding="utf-8") as f:
            f.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
