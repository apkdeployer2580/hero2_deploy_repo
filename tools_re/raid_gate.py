# hero2 (KTF 1.0.7) — 신전 「보스 레이드」: 귀환문 변형 신전(맵 112) · 단계 맵 113..121(스토리 보스 아홉, 2026-10-17) · 붉은 불꽃(obj 65..68).
#   (아래 2026-10-16 절의 「맵 111」은 첫 판 — 단일 율리케 방은 2026-10-17 절의 단계 맵으로 바뀌었다.)
#
# ── 사용자 결정 (2026-10-16) ───────────────────────────────────────────────────────
# *"윈터하츠의 신전 … 입구에서 좌측에 기둥2개 우측에 기둥 2개 … 우측 위에있는 기둥에 새로운 컨텐츠 … 귀환장소 지정 기둥으로
# 외관을 변경하고 그 입구에서 OK누르면 보스레이드가 바로 시작"* → *"외형이 바뀌는 기준은 가디언 모드 하드난이도를 클리어하면
# 그떄만 … 그대로 3칸이고 오른쪽 벽쪽 길 한줄 막혀도돼 … 외관만 바꿀꺼고 귀환장소용 아니야. 가운데 있는 입구쪽 포탈색상도
# 바꾸꺼야 … 기존 보스방중 하나 복제 … 보스의 레벨은 110 고정이고 HP와 공격력은 기존 가디언 모드 하드난이도보다 1.5배 …
# 보상은 골드야. 보스 한마리 잡을때마다 100만 골드 … 출구타일을 가운데 아래쪽에 길 하나 … 포기하시겠습니까? … 재도전은 언제든"*.
#
# ── 무엇이 어디에 (전부 맵 데이터 — 코드 패치는 보스의 능력치뿐, `guest_anchors::GuardianPatch` 의 레이드 워드) ─────────
# * 맵 112 = 신전(15)의 사본 + 우측 위 기둥 (20..22, 17..21) 을 윈터하츠 중앙 다운타운(00)의 귀환문 그대로 — 타일 t00 의
#   `07 07 07 / 0a 05 0a / 13 05 13 / 07 0b 07`, 처마 obj 35·36·37(층 1), 불꽃 `f` 객체(by 0 = obj 65..68, 보라 → 붉은 색) —
#   + 그 아홉 칸이 말걸기 그룹 11 → 스크립트 22(안내 · 「보스레이드를 시작할까요?」 · 효과음 · 맵 111 (9,12)).
#   게임은 이 파일을 모른다: 호스트가 가디언 HARD 클리어 영수증을 가진 캐릭터일 때만 `/m/m/15.m.gz` 읽기에 이 바이트를
#   내준다(`wie_android::raid_gate`) — 맵 번호 `bf.b` 는 그대로 15 라 세이브 · 귀환 지점 · 퀘스트는 아무것도 모른다.
# * 맵 111 = 신전지하 G구역(108, 율리케)의 사본: 이벤트를 전부 갈아 끼운다 — 처치(자동, 그룹 0) → 「보스 레이드 성공!」 ·
#   골드 100 만(`18 v` 가 부호 있는 16 비트라 32,767 × 30 + 16,990 = 31 명령; `ag.g(I)V` 는 999,999,999 에서 자른다) ·
#   맵 15 (21,21) — 보스 문 원본처럼 처치 순간 나간다(떨어진 드랍은 원본 보스방과 같이 못 줍는다) · 출구 = 바닥 가운데 (9,16) 의
#   문(밟기 그룹 1, obj 12) → 「포기하시겠습니까?」 → 예 = 맵 15 (21,21). 번호 111 ≥ 101 이라 게임 자신이 저장 · 귀환서를 막는다.
# * obj 65..68 = obj 21..24(귀환문의 보라 불꽃)의 색상 회전 사본 — 팔레트 끝에 새 색을 더하고 원본 프레임 바이트는 그대로.
#
# ── ★★ 2026-10-17 사용자 지시: 보스를 스토리 순서대로 하나씩 ─────────────────────────────────────────
# *"보스레이드의 보스가 기존 보스 순차로 … 스토리에서 진행되는 보스 순서대로 … 처음에 도적보스 잡고 그뒤에 다음 보스 … 보스를
# 죽인뒤에 좌측 템나오는 부분에 다음 보스로 이동 중...5(폰트는 게임폰트 그대로) … 5,4,3,2,1 … 1이후 다음보스로 … 마지막에 보스
# 끝나고 난 뒤에 최종 클리어한 보스만큼 보상 … 이지 100만 · 노말 200만 · 하드 300만"*. 그래서 맵 111(율리케 하나)은 없어지고
# 단계 맵 아홉 = **113..121** — 보스방 101 · 103 · 104 · 102 · 107 · 105 · 106 · 108 · 109 의 사본(그 방에 들어가는 문 스크립트가
# 진행시키는 퀘스트 0x08 · 0x10 · 0x16 · 0x20 · 0x21 · 0x25 · 0x26 · 0x2f · 0x2f 순 = 레벨 11 · 17 · 25 · 29 · 34 · 43 · 45 · 46 · 50 순).
# 각 단계 맵의 이벤트:
#   그룹 0 (자동 · 처치 — 그 방의 보스가 `cq.l` 로 고르는 그룹 그대로) → 스크립트 0 = 이벤트 플래그 RAID_FLAG 켬.
#     대사도 워프도 없다 — 호스트(`emulator::BossRaid` · `hero2java::H2BossRaid`)가 플래그를 보고 5 초를 세며
#     좌측 안내를 띄우고, 다 세면 그룹 2 를 게임 자신의 `bb.a(B)V` 로 돌린다. 그 사이 플레이어는 걷고 드랍을 줍는다.
#   그룹 1 (밟기 · 바닥 가운데 아래의 문, obj 12) → 스크립트 1 = 「포기하시겠습니까?」 → 예 = 플래그 끔 · 신전 (21,21).
#   그룹 2 (자동, 어느 칸에도 없다 — 호스트만 부른다) → 스크립트 2 = 플래그 끔 · 다음 단계 맵의 입장 지점.
# ★★ 2026-10-17 (2) 바퀴 (*"한바퀴 다 돌면 그 다음 바퀴부터는 난이도가 상승 … 총 10회까지 … 공격력 5%, HP30% 상승"*): 마지막 단계의
#   그룹 2 도 신전이 아니라 첫 단계(113 (13,14))로 간다 — 바퀴 수 · 능력치 · 바퀴별 골드는 호스트가 센다(맵은 바퀴를 모른다).
#   레이드를 나가는 길은 문(포기)과 죽음뿐이고, 마지막 단계의 「보스 레이드 성공!」 대사는 없어졌다(카운트다운을 멈춘다).
# 보상(골드)은 맵 데이터가 아니다 — 호스트가 레이드를 떠나 신전에 닿을 때 처치한 보스 수만큼 `ag.g(I)V` 로 준다(난이도별 금액).
# 맵 번호 ≥ 101 이라 게임 자신이 저장 · 귀환서를 막고, 레이드 맵의 보스 능력치는 `GuardianPatch` 의 레이드 워드(첫 맵 · 맵 수)가 고른다.
# RAID_FLAG = 0xF0 — 배포본 맵의 이벤트 플래그는 0..145 만 쓴다(스크립트 `12 f y` 는 f 를 부호 없이 읽는다, 0x124492 `lsrs`).
#
# ⛔ 규율: 맵은 `hero2_map.parse → 고침 → build` 로만 (배포본 111 장이 바이트 동일로 왕복하는 그 형식) · 원본 맵 15 · 108 ·
#   obj 시트의 바이트는 손대지 않는다(변형은 새 멤버) · 고치기 전에 원본이 기대한 모양인지 전부 assert.
import colorsys
import struct
import sys
import zlib

import costume_sheets as cs
import hero2_map as hm

# 맵 번호 — 게임이 `/m/m/<n>.m.gz` 를 번호 문자열로 연다(`cf.n()V`); 배포본은 0..110. `ag.h:B` 는 부호 있는 바이트라 127 까지.
RAID_FIRST = 113  # 단계 맵 113..121 (= `guest_anchors::GuardianPatch::RAID_MAP` · `RAID_COUNT`)
TEMPLE = 15
TEMPLE_VARIANT = 112
TEMPLE_RETURN = (21, 21)  # 귀환문의 디딤돌(0b) — 레이드를 끝내거나 포기하면 여기
GATE_X, GATE_Y = 20, 17  # 변형되는 3×5 칸의 왼쪽 위 (귀환문 사본의 ff 줄부터)
GATE_GROUP = 11  # 신전의 그룹 0..10 다음
RAID_FLAG = 0xF0  # 「이 단계의 보스를 잡았다」 — 처치 스크립트가 켜고 다음 단계 · 신전으로 가는 스크립트가 끈다 (= `hero2java::H2_RAID_FLAG`)
# 스토리 순서: (원본 보스방, 그 방의 몬스터 종류 · 배치, 입장 지점 = 원본 문 스크립트의 `3a x y`, 보스 이름(검사용))
STAGES = (
    (101, [90], [(90, 9, 11)], (13, 14), "도적 두목"),
    (103, [93], [(93, 9, 7)], (9, 12), "리이드가 당주"),
    (104, [89], [(89, 9, 7)], (9, 12), "이안"),
    (102, [91], [(91, 9, 8)], (10, 13), "소문의 검사"),
    (107, [96, 98, 99], [(96, 9, 6), (98, 7, 10), (99, 11, 10)], (9, 12), "고대수호물"),
    (105, [94], [(94, 9, 7)], (9, 12), "하인츠"),
    (106, [92], [(92, 9, 7)], (9, 12), "디미트리"),
    (108, [95], [(95, 9, 7)], (9, 12), "율리케"),
    (109, [97, 100, 101], [(97, 9, 7), (100, 5, 10), (101, 13, 10)], (9, 13), "오딘"),
)
RAID_COUNT = len(STAGES)
FLAME_SRC = (21, 22, 23, 24)  # 귀환문 불꽃 (by 0 of maps 00 · 34 · 64)
OBJ_SHIPPED = 65
FLAME_FIRST = OBJ_SHIPPED  # 새 프레임 65..68
FLAME_HUE_SHIFT = 110 / 360  # 보라(240..270°) → 붉은 주황(350..20°)
DOOR_OBJ = 12
CORNICE = (35, 36, 37)
# 로드 표 명령 종류 (`bf.b(BBBB)V` 점프표 @0x170088 — 항목 = 표 + 부호 있는 오프셋):
LOAD_FX_ANIM = 0x04  # 0x12b6ce: f 객체 n 의 애니메이션 ← by[x] — 신전에는 없다(이 파일이 f 를 하나 두므로 assert)
LOAD_EMOTE = 0x0D  # 0x12b854: `bf.b(BB)V` = NPC n 머리 위 감정표현 x (`w`, img/emoti — 8 「!」 · 9 「?」, ≤ 0 = 지움)

# 윈터하츠 중앙 다운타운(00) (6..8, 18..22) 의 귀환문 — 신전과 같은 타일셋 t00.
GATE_TILES = (
    (0xFF, 0xFF, 0xFF),
    (0x07, 0x07, 0x07),
    (0x0A, 0x05, 0x0A),
    (0x13, 0x05, 0x13),
    (0x07, 0x0B, 0x07),
)
# 신전(15) 의 그 자리, 바뀌기 전 (우측 위 기둥 + 벽 쪽 바닥 한 줄).
PILLAR_TILES = (
    (0xFF, 0xFF, 0x05),
    (0x1D, 0x1C, 0x05),
    (0x1D, 0x1C, 0x10),
    (0x07, 0x07, 0x10),
    (0x08, 0x08, 0x05),
)
PILLAR_ATTR = (
    (0, 0, 0),
    (-128, -128, 0),
    (-128, -128, 0),
    (-128, -128, 0),
    (0, 0, 0),
)


def _gate_attr():
    g = -(GATE_GROUP + 1)
    return (
        (0, 0, 0),
        (g, g, g),
        (g, g, g),
        (g, g, g),
        (-128, 0, -128),
    )


def cp949(s):
    b = s.encode("cp949")
    if not 0 < len(b) < 256:
        raise ValueError(f"line {s!r} is {len(b)} B")
    return b


# ── obj 시트: 붉은 불꽃 네 프레임 덧붙이기 ─────────────────────────────────────────


def _shift_hue(rgb):
    r, g, b = (c / 255 for c in rgb)
    h, s, v = colorsys.rgb_to_hsv(r, g, b)
    if s < 0.05:
        return tuple(rgb)  # 회색은 그대로 (불꽃의 검은 심 · 테두리)
    h = (h + FLAME_HUE_SHIFT) % 1.0
    r, g, b = colorsys.hsv_to_rgb(h, s, v)
    return (round(r * 255), round(g * 255), round(b * 255))


def obj_with_raid_flame(mph, mpd):
    """`m/obj.mph` · `m/obj_0.mpd` → 같은 바이트 + 프레임 65..68(21..24 의 색상 회전) · 팔레트 끝에 새 색.
    원본 프레임의 바이트 · 오프셋은 그대로다(표 뒤에 항목 넷, mpd 뒤에 프레임 넷)."""
    head, n = struct.unpack(">II", mph[:8])
    if n != OBJ_SHIPPED:
        raise ValueError(f"m/obj.mph has {n} frames, expected {OBJ_SHIPPED} (KTF 1.0.7)")
    sheet = cs.sheet_decode(mph, mpd, "m/obj")
    table = mph[8 : 8 + 8 * n]
    tail = cs.walk_chunks(mph, 8 + 8 * n, where="m/obj.mph")
    plte = bytearray(tail[0][1])
    trns = tail[1][1]
    palette = list(sheet.palette)
    remap = {}
    for k in FLAME_SRC:
        for row in sheet.frames[k].pixels:
            for idx in row:
                if idx in remap:
                    continue
                new = _shift_hue(palette[idx])
                if new == tuple(palette[idx]):
                    remap[idx] = idx
                    continue
                if new in palette:
                    remap[idx] = palette.index(new)
                else:
                    palette.append(new)
                    remap[idx] = len(palette) - 1
    if len(palette) > 256:
        raise ValueError(f"palette would hold {len(palette)} colours")
    new_mpd = bytearray(mpd)
    new_table = bytearray(table)
    for k in FLAME_SRC:
        fr = sheet.frames[k]
        flags = table[8 * k + 6 : 8 * k + 8]
        raw = bytearray()
        for row in fr.pixels:
            raw += b"\0" + bytes(remap[i] for i in row)
        new_table += b"\0\0" + len(new_mpd).to_bytes(4, "big") + flags
        new_mpd += cs.chunk(b"IHDR", struct.pack(">IIBBBBB", fr.w, fr.h, 8, 3, 0, 0, 0))
        for t, d in fr.extra:
            new_mpd += cs.chunk(t, d)
        new_mpd += cs.chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    plte = b"".join(bytes(c) for c in palette)
    new_mph = struct.pack(">II", head, n + len(FLAME_SRC)) + bytes(new_table) + cs.chunk(b"PLTE", plte) + cs.chunk(b"tRNS", trns)
    # 되읽기: 원본 프레임 전부 같은 픽셀, 새 넷은 색만 바뀐 같은 모양.
    back = cs.sheet_decode(new_mph, bytes(new_mpd), "m/obj+raid")
    if [f.pixels for f in back.frames[:n]] != [f.pixels for f in sheet.frames]:
        raise ValueError("the shipped obj frames changed")
    for j, k in enumerate(FLAME_SRC):
        want = [[remap[i] for i in row] for row in sheet.frames[k].pixels]
        if back.frames[n + j].pixels != want:
            raise ValueError(f"flame frame {n + j} did not round-trip")
    return new_mph, bytes(new_mpd)


# ── 맵 112: 귀환문 변형 신전 ─────────────────────────────────────────────────────────


def temple_variant(d15):
    m = hm.parse(d15)
    if m["name"] != "신전".encode("cp949") or (m["w"], m["h"]) != (30, 30) or m["tileset"] != 0:
        raise ValueError("map 15 is not the shipped 신전")
    if list(m["f2"]) != [2, 12, 35, 37, 47] or m["by"] or m["objs"][3]:
        raise ValueError("map 15's object lists are not the shipped ones")
    if len(m["groups"]) != GATE_GROUP or len(m["scripts"]) != 22:
        raise ValueError(f"map 15 has {len(m['groups'])} groups · {len(m['scripts'])} scripts — not the shipped file")
    for dy, (trow, arow) in enumerate(zip(PILLAR_TILES, PILLAR_ATTR)):
        for dx in range(3):
            x, y = GATE_X + dx, GATE_Y + dy
            if hm.tile(m, x, y) != trow[dx] or hm.attr(m, x, y) != arow[dx]:
                raise ValueError(f"map 15 ({x},{y}) is {hm.tile(m, x, y):#04x}/{hm.attr(m, x, y)} — not the pillar this expects")
    # 1 — 타일 · 속성: 귀환문 (3 × 5)
    for dy, (trow, arow) in enumerate(zip(GATE_TILES, _gate_attr())):
        for dx in range(3):
            hm.set_tile(m, GATE_X + dx, GATE_Y + dy, trow[dx])
            hm.set_attr(m, GATE_X + dx, GATE_Y + dy, arow[dx])
    # 2 — 처마 셋 (층 1): 기둥의 35 · 37 두 장을 35 · 36 · 37 로
    px = GATE_X * 16 + 8
    py = (GATE_Y + 1) * 16
    layer = m["objs"][1]
    for want in ((35, px, py), (37, px + 16, py)):
        if want not in layer:
            raise ValueError(f"map 15 layer 1 has no {want}")
    layer.remove((37, px + 16, py))
    layer.append((36, px + 16, py))
    layer.append((37, px + 32, py))
    # 3 — 불꽃: by 0 = 새 obj 65..68, f 객체 하나 (귀환문 00 의 (0, 120, 352) = 칸 (7, 22) 와 같은 자리 규칙)
    m["by"] = [[(FLAME_FIRST + j, 0, 0) for j in range(len(FLAME_SRC))]]
    m["by_raw"] = 1
    m["f2"] = bytes(sorted(set(m["f2"]) | {36} | {FLAME_FIRST + j for j in range(len(FLAME_SRC))}))
    # ⛔ `f` 레코드의 첫 바이트는 by 번호가 아니라 **f2 의 한 프레임 번호**다 — 로더(0x12a6ce)가 `bf.a(B)B` 로 f2 안의 위치를 찾아
    #   그 위치를 by 번호로 쓴다(없으면 −1 → `bf.a:[Lby;`[−1] = ArrayIndexOutOfBounds, 로드 중 죽는다). 배포본 셋(00 · 34 · 64)은
    #   f2[0] = 0 이고 레코드 바이트도 0 이라 「0 = by 0」처럼 보였을 뿐이다. 여기서는 f2[0](= 2) 을 적는다 → 위치 0 = by 0.
    m["objs"][3] = [(m["f2"][0], (GATE_X + 1) * 16 + 8, (GATE_Y + 4) * 16)]
    # ⛔⛔ 2026-10-18 정정: 로드 표(구간 E)의 `0d n x y` 는 **NPC n 머리 위의 퀘스트 표시**다(점프표 13 → 0x12b854 → `bf.b(BB)V(x, n)`
    #   → `w(−6, x)`, 9 = 「?」 · 8 = 「!」 — 신전 NPC 0 = 율리케 · 1..3 = 신관). 첫 판은 이것을 f 객체 애니메이션(실은 종류 0x04)으로
    #   읽고 걷어내서, 게이트가 켜진 캐릭터(가디언 HARD 클리어)는 신전 NPC 의 「?」·「!」를 하나도 못 봤다(사용자 보고). 그대로 둔다 —
    #   로드 표는 배포본과 바이트 동일이어야 하고, 이 파일이 더하는 f 객체를 건드리는 0x04 명령은 신전에 없어야 한다.
    if any(c[0] == LOAD_FX_ANIM for lst in m["load_lists"] for c in lst):
        raise ValueError("map 15's load table animates an f object — the flame added here would be the one it picks")
    npcs = len(m["npcs"])
    if not any(c[0] == LOAD_EMOTE for lst in m["load_lists"] for c in lst) or any(
        c[0] == LOAD_EMOTE and c[1] >= npcs for lst in m["load_lists"] for c in lst
    ):
        raise ValueError("map 15's load table quest marks are not the shipped ones")
    # 4 — 말걸기 그룹 11 → 스크립트 22
    s = len(m["scripts"])
    m["groups"].append([hm.record(2, s)])
    l0 = len(m["lines"])
    m["lines"].append(cp949("}보스 레이드(보스 LV 110). 보스룸으로 이동하면 저장할 수 없습니다.@"))
    m["lines"].append(cp949("}보스레이드를 시작할까요?@"))
    # 예 → 플래그 끔(남아 있던 것이 첫 단계를 「처치됨」으로 읽히지 않게) · 효과음 · 첫 단계 맵의 입장 지점
    ex, ey = STAGES[0][3]
    go = [(0x12, RAID_FLAG, 1), (0x14, 0, 0), (0x39, RAID_FIRST, 0), (0x3A, ex, ey), (0x2D, 0, 0)]
    m["scripts"].append([(0x0F, l0, 0), (0x10, l0 + 1, len(go))] + go)  # 아니오 = 끝까지 건너뜀(배포본 문들처럼 `2d` 포함)
    out = hm.build(m)
    back, orig = hm.parse(out), hm.parse(d15)
    # NPC 배치 · 로드 표(숨김 · 보임 · 퀘스트 표시) · 기존 트리거 · 스크립트는 배포본 그대로여야 한다 — 변형은 귀환문뿐
    for key in ("npc_kinds", "npcs", "load_entries", "load_lists"):
        if back[key] != orig[key]:
            raise ValueError(f"map 112's {key} differs from map 15's")
    if back["groups"][:GATE_GROUP] != orig["groups"] or back["scripts"][: len(orig["scripts"])] != orig["scripts"]:
        raise ValueError("map 112 changed one of map 15's triggers or scripts")
    return out


# ── 단계 맵 113..121: 스토리 보스방의 사본 ─────────────────────────────────────────────


def exit_tile(m):
    """바닥 맨 아래 줄 바로 밑의 벽 한 칸, 가운데(x 9)에 가장 가까운 것 — 출구 문 자리. 위 칸은 걸을 수 있는 바닥이어야 한다."""
    floor_rows = [y for y in range(m["h"]) if any(hm.attr(m, x, y) == 0 for x in range(m["w"]))]
    y = max(floor_rows)
    xs = [x for x in range(m["w"]) if hm.attr(m, x, y) == 0 and y + 1 < m["h"] and hm.attr(m, x, y + 1) == -128]
    if not xs:
        raise ValueError(f"{m['name'].decode('cp949')}: no wall under the bottom floor row {y}")
    x = min(xs, key=lambda v: (abs(v - 9), v))
    return x, y + 1


def stage_map(k):
    return RAID_FIRST + k


def raid_stage(k, src):
    """단계 k(0..)의 맵 = 원본 보스방 `src` 의 사본: 타일 · 몬스터는 그대로, 이벤트 · 컷신 · 로드 표를 갈아 끼운다."""
    room, kinds, spawns, entry, boss = STAGES[k]
    m = hm.parse(src)
    if (m["w"], m["h"]) != (20, 20) or m["kinds"] != kinds or m["spawns"] != spawns:
        raise ValueError(f"map {room} is not the shipped boss room of {boss}: {m['kinds']} {m['spawns']}")
    if m["npcs"] or m["f2"] or any(m["objs"]) or m["by"]:
        raise ValueError(f"map {room}'s NPC/object sections are not the shipped empty ones")
    if len(m["groups"]) != 1 or any(r[2] != 0 for r in m["groups"][0]):
        raise ValueError(f"map {room}'s trigger groups are not the shipped kill group")
    if hm.attr(m, *entry) != 0:
        raise ValueError(f"map {room}'s entry {entry} is not floor")
    if any(hm.attr(m, x, y) not in (0, -128) for y in range(m["h"]) for x in range(m["w"])):
        raise ValueError(f"map {room} has a tile trigger the copy would keep")
    ex, ey = exit_tile(m)
    m["name"] = cp949(f"보스 레이드 {k + 1}/{len(STAGES)}")
    # 출구 문: 밟기 그룹 1 (막힌 칸 — 부딪히면 돈다) + 문 그림
    hm.set_attr(m, ex, ey, -2)
    m["objs"][0] = [(DOOR_OBJ, ex * 16 + 8, ey * 16)]
    m["f2"] = bytes([DOOR_OBJ])
    # 그룹 0 = 처치(자동, 조건 없음 — 원본의 「이미 잡았으면 조용히 내보냄」 레코드는 버린다) · 1 = 출구 문 · 2 = 호스트가 부르는 진행 ·
    # ★★ 2026-10-17 (3) 3 = 호스트가 부르는 귀환(묻지 않음) — 레이드에서 쓰러지면(사용자: *"레이드 도중 죽으면 밖으로 나가지면서 이전까지
    # 쌓아뒀던 보상을 받아야해"*) 호스트가 이 그룹을 돌리고 캐릭터를 일으킨다(`hero2java::H2_RAID_HOME_GROUP`)
    m["groups"] = [[hm.record(0, 0)], [hm.record(3, 1)], [hm.record(0, 2)], [hm.record(0, 3)]]
    m["lines"] = [cp949("}포기하시겠습니까?@")]
    home = [(0x12, RAID_FLAG, 1), (0x14, 0, 0), (0x39, TEMPLE, 0), (0x3A, TEMPLE_RETURN[0], TEMPLE_RETURN[1]), (0x2D, 0, 0)]
    # ★★ 2026-10-17 (2) 바퀴: 마지막 단계의 진행도 다음 단계 — 첫 단계로 (바퀴는 호스트가 센다). 레이드를 나가는 길은 문과 쓰러짐.
    nk = (k + 1) % len(STAGES)
    nx, ny = STAGES[nk][3]
    advance = [(0x12, RAID_FLAG, 1), (0x14, 0, 0), (0x39, stage_map(nk), 0), (0x3A, nx, ny), (0x2D, 0, 0)]
    kill = [(0x12, RAID_FLAG, 0), (0x2D, 0, 0)]
    m["scripts"] = [kill, [(0x10, 0, len(home))] + home, advance, list(home)]
    m["cuts"] = []
    m["load_entries"] = []
    m["load_lists"] = []
    out = hm.build(m)
    hm.parse(out)
    return out


def map_members(zin):
    """원본 zip → {새 멤버 이름: gzip 바이트} — 단계 맵 아홉 · 귀환문 변형 신전."""
    import gzip

    out = {}
    for k, stage in enumerate(STAGES):
        src = gzip.decompress(zin.read(hm.map_name(stage[0])))
        out[hm.map_name(stage_map(k))] = gzip.compress(raid_stage(k, src), mtime=0)
    d15 = gzip.decompress(zin.read(hm.map_name(TEMPLE)))
    out[hm.map_name(TEMPLE_VARIANT)] = gzip.compress(temple_variant(d15), mtime=0)
    return out


def verify(zpath):
    """생성된 에셋: 단계 맵이 차례로 서로를 가리키고 마지막은 첫 단계로(바퀴), 문은 신전으로, 변형 신전은 첫 단계로; obj 시트 69 프레임;
    원본 그대로."""
    import gzip
    import io
    import zipfile

    z = zipfile.ZipFile(zpath)
    jar = zipfile.ZipFile(io.BytesIO(z.read([n for n in z.namelist() if n.endswith(".jar")][0])))
    sheet = cs.sheet_decode(jar.read("m/obj.mph"), jar.read("m/obj_0.mpd"), "m/obj")
    assert len(sheet.frames) == OBJ_SHIPPED + len(FLAME_SRC), f"obj frames {len(sheet.frames)}"
    v = hm.parse(gzip.decompress(z.read(hm.map_name(TEMPLE_VARIANT))))
    assert any(c[0] == 0x39 and c[1] == RAID_FIRST for c in v["scripts"][-1]), "the temple variant does not warp to stage 1"
    assert max(v["f2"]) < len(sheet.frames) and max(v["by"][0])[0] < len(sheet.frames), "the variant names an obj frame the sheet lacks"
    assert all(f in v["f2"] for f, _, _ in v["by"][0]) and v["objs"][3][0][0] == v["f2"][0], "the flame's frames must be in f2 and its record byte f2[0]"
    assert hm.map_name(111) not in z.namelist(), "the old single raid room 111 is still in the asset"
    for k in range(len(STAGES)):
        r = hm.parse(gzip.decompress(z.read(hm.map_name(stage_map(k)))))
        want = stage_map((k + 1) % len(STAGES))
        assert [c[1] for c in r["scripts"][2] if c[0] == 0x39] == [want], f"stage {k + 1} does not advance to {want}"
        assert [(c[1], c[2]) for c in r["scripts"][2] if c[0] == 0x3A] == [STAGES[(k + 1) % len(STAGES)][3]], f"stage {k + 1} lands off the next entry"
        assert not any(c[0] == 0x0F for c in r["scripts"][0]), f"stage {k + 1}'s kill holds a dialog (it would stop the countdown)"
        assert [c[1] for c in r["scripts"][1] if c[0] == 0x39] == [TEMPLE], f"stage {k + 1}'s door does not go home"
        assert len(r["groups"]) == 4 and (r["groups"][3][0][2], r["groups"][3][0][30]) == (0, 3), f"stage {k + 1} has no home group (3, auto → script 3)"
        assert [c[1] for c in r["scripts"][3] if c[0] == 0x39] == [TEMPLE] and not any(c[0] in (0x0F, 0x10) for c in r["scripts"][3]), f"stage {k + 1}'s home script asks or does not go home"
        assert (0x12, RAID_FLAG, 0) in r["scripts"][0], f"stage {k + 1}'s kill does not raise the flag"
        assert r["kinds"] == STAGES[k][1], f"stage {k + 1} lost its boss"
    o15 = hm.parse(gzip.decompress(z.read(hm.map_name(TEMPLE))))
    assert len(o15["groups"]) == GATE_GROUP and not o15["by"], "the shipped 신전 changed"
    return True


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    print("ok" if verify(sys.argv[1]) else "bad")
