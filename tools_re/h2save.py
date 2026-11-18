#!/usr/bin/env python3
"""hero2 (영웅서기2 KTF 1.0.7) 세이브 `/g<slot>` 해독·편집 — 게임 자신의 암호 그대로.

── 왜 이 도구인가 (2026-09-26, 사자후 크래시) ─────────────────────────────────
버서커 시험 세이브가 없었다(시험 세이브 셋 = 로그 LV48 · 나이트 LV26 · 나이트 LV47). 세이브
파일은 평문이 아니라서 직업 바이트 하나 바꾸는 것도 게임 코드를 읽어야 했다:

  `ag.g(B)V` @0x114110 (→ `/g<slot>`) 은 블롭 다섯을 **각자 다른 키로** 암호화해
  `[u16 BE 길이][암호문]` 으로 잇는다 (뒤에 0 네 바이트):
      [0] 스테이지   `bf.a()[B`   키 ag.n   = [맵, x, y, 방향, ★직업 ag.a:B]   (5 B)
      [1] 캐릭터     `a.b()[B`    키 ag.o   = [0] 레벨 … [27] a.h(열린 콤보 단수)
                                             [29..43] ★ a.p:[B 스킬 15칸 (-1 = 잠김)
      [2] `ag.b()[B`              키 ag.p   = ag.q[32] 이벤트 플래그 · ag.r[32] 퀘스트 상태 · ag.k[3]
                                             · ag.j[4] 퀵바 · [71] ★ ag.i:B 모드(0 NORMAL·1 HARDCORE·2 HELL)
                                             · [72] ★ ag.j:B 누적 달성도(0..100) · ag.b Vector …
      [3] 재료가방   `bq.a()[B`   키 ag.n
      [4] `ag.c()[B`              키 ag.o   = ★ 2026-10-15 야수 목록(Vector `ag.a` @0x193b6c): [수] 다음 야수마다
                                             [15][m0 k0 m1 k1 m2 k2 m3 k3 · bn.a:I(BE 4) · bn.d:S(BE 2) · bn.e:Z] —
                                             k = 스킬 레벨 4칸, 장착 번호는 캐릭터 블롭 [28] a.r:B
  암호 `bg.a([B[B)[B` @0x12e6bc:  out[i] = in[i] ^ key[(i+1) % 6]  (키 인덱스가 1 부터),
  그리고 끝에 체크섬 한 바이트 = 쓴 키 바이트들의 합 & 0xff (평문이 아니라 **키**의 합 —
  길이만의 함수다). 키 셋은 `ag.<clinit>` 의 상수:
      ag.n = 05 47 18 3b 2b 10 · ag.o = 0e 1a 48 0a 1c 62 · ag.p = 43 0c 23 44 5b 2b
  시험 세이브 셋의 블롭 15개가 전부 체크섬 일치, 로그 LV48 의 a.p 가 문서값
  `[1 2 1 1 0 | 2 2 1 0 5 -1 3 0 3 0]` 과 바이트 단위로 같다.

  ⛔ 직업 바이트는 캐릭터 블롭이 아니라 **스테이지 블롭 [4]** 다 — 로더(`ag.o()V`)가 그 static 으로
  팩토리 `a.a(BSS)La;` 를 부른다(0 버서커 `bl` · 1 나이트 `bx` · 2 로그 `ay`). 직업을 바꾸면 a.p 의
  배치도 그 직업 것으로 줘야 한다(콤보 단수 버서커 4 · 나이트 3 · 로그 5 + 직업 스킬).

사용법:
  python tools_re/h2save.py dump <g파일>
  python tools_re/h2save.py edit <원본 fs 폴더> <새 fs 폴더> <슬롯 1..5>
         [--class 0|1|2] [--skills 15개,콤마] [--combo N] [--map ID --x X --y Y]
         [--mode 0|1|2|3] [--achievement 0..100]
         [--level N] [--stats 힘,체력,민첩,의지,지력] [--points N] [--skill-points N] [--hp N] [--sp N]
  ★ 2026-10-02 (의상 소급 지급): --mode / --achievement 는 「이미 깬 회차」를 만든다 — 모드 1 이상 = NORMAL 클리어,
    모드 2 = HARDCORE 클리어, 모드 2 + 달성도 81 이상 = HELL 클리어(NORMAL·HARDCORE 가 더하는 달성도는 최대 80).
  ★ 2026-10-08 가디언: --mode 3 = 가디언(4회차) — HELL 클리어로만 닿는 모드(호스트 패치 guardian_mode_open 가 있는 빌드).
    가디언 세이브는 모드 3 을 담는 빌드(vC 89+)로만 열어야 한다 — 옛 APK 는 슬롯 화면에서 AIOOBE.
  예) 슬롯 2 숲 나이트 → 사자후(a.p[10]) 배운 버서커:
      python tools_re/h2save.py edit dist/test_saves/01035D0B/fs <scratch>/bz/01035D0B/fs 2 \\
             --class 0 --combo 4 --skills 1,1,1,1,1,1,1,1,1,1,3,0,0,0,0
      → probe `--fs <scratch>/bz` 로 싣는다. 새 폴더에는 원본 폴더의 나머지 파일이 그대로 복사된다.
"""
import argparse
import os
import shutil
import struct
import sys

KEY_N = [0x05, 0x47, 0x18, 0x3B, 0x2B, 0x10]
KEY_O = [0x0E, 0x1A, 0x48, 0x0A, 0x1C, 0x62]
KEY_P = [0x43, 0x0C, 0x23, 0x44, 0x5B, 0x2B]
G_KEYS = [KEY_N, KEY_O, KEY_P, KEY_N, KEY_O]
G_NAMES = ["stage bf.a()[B", "char a.b()[B", "ag.b()[B", "materials bq.a()[B", "ag.c()[B"]
CLASSES = {0: "버서커(bl)", 1: "나이트(bx)", 2: "로그(ay)"}
# ★ 2026-10-08 (가디언 밸런스 검증): 캐릭터 블롭 = `a.b()[B` @0x152af0 의 쓰기 순서 — [0] 레벨 a.a:B · [1..2] 스탯 포인트 a.c:S(BE)
# · [3..4] 스킬 포인트 a.d:S · [5..8] HP a.a:I(BE) · [9..12] SP a.b:I · [13..16] EXP a.c:I · [17..18] 힘 a.e:S · [19..20] 체력 a.f:S ·
# [21..22] 민첩 a.g:S · [23..24] 의지 a.h:S · [25..26] 지력 a.i:S · [27] a.h:B(콤보 단수) · [28] a.r:B · [29..] a.p. 최대 HP/SP/ATK/DEF 는
# 로드 때 a.m()V 가 다시 센다 — 기본 능력치와 레벨만 쓰면 된다.
CHAR_LEVEL, CHAR_STAT_PTS, CHAR_SKILL_PTS, CHAR_HP, CHAR_SP, CHAR_EXP, CHAR_STATS = 0, 1, 3, 5, 9, 13, 17
CHAR_COMBO_OPEN = 27  # a.h — how many combo stages are open
CHAR_SKILLS = 29      # a.p:[B, 15 bytes
G2_MODE = 32 + 32 + 3 + 4   # ag.i:B in blob [2]: after ag.q[32] · ag.r[32] · ag.k[3] · ag.j[4]
G2_ACHIEVEMENT = G2_MODE + 1  # ag.j:B — the 달성도 the round clears added (ag.m = 50 · 30 · 20, clamp 100)
MODES = {0: "NORMAL", 1: "HARDCORE", 2: "HELL", 3: "GUARDIAN"}  # ★ 2026-10-08: 3 = 가디언 (HERO2_GUARDIAN_PLAN.md)


def decrypt(enc, key):
    """`bg.a` inverted: returns (plain, checksum_ok)."""
    n = len(enc) - 1
    plain = bytes((enc[i] ^ key[(i + 1) % len(key)]) & 0xFF for i in range(n))
    cksum = sum(key[(i + 1) % len(key)] for i in range(n)) & 0xFF
    return plain, enc[-1] == cksum


def encrypt(plain, key):
    """`bg.a([B[B)[B` as the game runs it."""
    n = len(plain)
    body = bytes((plain[i] ^ key[(i + 1) % len(key)]) & 0xFF for i in range(n))
    return body + bytes([sum(key[(i + 1) % len(key)] for i in range(n)) & 0xFF])


def parse_g(data):
    """The five length-prefixed blobs of a `/g<slot>` file, and the trailing bytes."""
    p, blobs = 0, []
    for _ in G_KEYS:
        (n,) = struct.unpack_from(">H", data, p)
        p += 2
        blobs.append(data[p:p + n])
        p += n
    return blobs, data[p:]


def build_g(plains, rest):
    out = b""
    for k, pt in enumerate(plains):
        e = encrypt(bytes(pt), G_KEYS[k])
        out += struct.pack(">H", len(e)) + e
    return out + rest


def signed(b):
    return b - 256 if b >= 128 else b


def cmd_dump(path):
    data = open(path, "rb").read()
    blobs, rest = parse_g(data)
    plains = []
    for k, b in enumerate(blobs):
        pt, ok = decrypt(b, G_KEYS[k])
        plains.append(pt)
        print(f"[{k}] {G_NAMES[k]:20} {len(pt):4} B  checksum {'ok' if ok else 'BAD'}")
    st, ch = plains[0], plains[1]
    print(f"stage: map {st[0]} tile ({st[1]},{st[2]}) facing {st[3]} class {st[4]} {CLASSES.get(st[4], '?')}")
    if len(ch) >= CHAR_SKILLS + 15:
        u16 = lambda i: struct.unpack_from(">H", ch, i)[0]  # noqa: E731
        u32 = lambda i: struct.unpack_from(">I", ch, i)[0]  # noqa: E731
        print(f"char : level {ch[0]}  stat pts {u16(CHAR_STAT_PTS)}  skill pts {u16(CHAR_SKILL_PTS)}  HP {u32(CHAR_HP)}  SP {u32(CHAR_SP)}  EXP {u32(CHAR_EXP)}")
        print(f"       힘 {u16(CHAR_STATS)} 체력 {u16(CHAR_STATS + 2)} 민첩 {u16(CHAR_STATS + 4)} 의지 {u16(CHAR_STATS + 6)} 지력 {u16(CHAR_STATS + 8)}  a.h (open combo stages) {ch[CHAR_COMBO_OPEN]}")
        print(f"       a.p {[signed(x) for x in ch[CHAR_SKILLS:CHAR_SKILLS + 15]]}")
    g2 = plains[2]
    if len(g2) > G2_ACHIEVEMENT:
        mode, ach = g2[G2_MODE], g2[G2_ACHIEVEMENT]
        cleared = [MODES[m] for m in range(3) if (m == 0 and mode >= 1) or (m == 1 and mode >= 2) or (m == 2 and (mode >= 3 or (mode >= 2 and ach > 80)))]
        print(f"round: mode {mode} {MODES.get(mode, '?')}  달성도 {ach}  → cleared before {cleared}")
    b4 = plains[4]
    if len(b4) >= 1:  # ★ 2026-10-15: the owned beasts (ag.c()[B) — bn.k is each beast's 4 skill levels
        n, p = b4[0], 1
        eq = ch[CHAR_COMBO_OPEN + 1] if len(ch) > CHAR_COMBO_OPEN + 1 else None
        print(f"beasts: {n} owned  equipped index a.r = {eq}")
        for i in range(n):
            if p >= len(b4) or p + 1 + b4[p] > len(b4):
                print(f"       beast {i}: truncated")
                break
            rec = b4[p + 1:p + 1 + b4[p]]
            p += 1 + b4[p]
            if len(rec) >= 8:
                print(f"       beast {i}: m {[rec[2 * s] for s in range(4)]}  k {[signed(rec[2 * s + 1]) for s in range(4)]}")
    print(f"rest : {rest.hex()}  ({len(data)} B file)")


def cmd_edit(a):
    os.makedirs(a.dst, exist_ok=True)
    for f in os.listdir(a.src):
        p = os.path.join(a.src, f)
        if os.path.isfile(p):
            shutil.copy(p, os.path.join(a.dst, f))
    name = f"g{a.slot}"
    data = open(os.path.join(a.src, name), "rb").read()
    blobs, rest = parse_g(data)
    plains = []
    for k, b in enumerate(blobs):
        pt, ok = decrypt(b, G_KEYS[k])
        if not ok:
            sys.exit(f"{name}: blob {k} fails its checksum — not a save this tool understands")
        plains.append(bytearray(pt))
    st, ch = plains[0], plains[1]
    if a.cls is not None:
        st[4] = a.cls
    if a.map is not None:
        st[0] = a.map
    if a.x is not None:
        st[1] = a.x
    if a.y is not None:
        st[2] = a.y
    if a.combo is not None:
        ch[CHAR_COMBO_OPEN] = a.combo
    if a.level is not None:
        if not 1 <= a.level <= 99:
            sys.exit("--level wants 1..99")
        ch[CHAR_LEVEL] = a.level
    if a.stats is not None:
        vals = [int(v) for v in a.stats.split(",")]
        if len(vals) != 5 or not all(1 <= v <= 999 for v in vals):
            sys.exit("--stats wants 5 values 1..999 (힘,체력,민첩,의지,지력)")
        for k, v in enumerate(vals):
            struct.pack_into(">H", ch, CHAR_STATS + 2 * k, v)
    if a.points is not None:
        struct.pack_into(">H", ch, CHAR_STAT_PTS, a.points)
    if a.skill_points is not None:
        struct.pack_into(">H", ch, CHAR_SKILL_PTS, a.skill_points)
    if a.hp is not None:
        struct.pack_into(">I", ch, CHAR_HP, a.hp)
    if a.sp is not None:
        struct.pack_into(">I", ch, CHAR_SP, a.sp)
    if a.skills is not None:
        vals = [int(v) for v in a.skills.split(",")]
        if len(vals) != 15:
            sys.exit("--skills wants 15 values")
        ch[CHAR_SKILLS:CHAR_SKILLS + 15] = bytes(v & 0xFF for v in vals)
    g2 = plains[2]
    if a.mode is not None:
        if a.mode not in MODES:
            sys.exit("--mode wants 0, 1, 2 or 3 (3 = 가디언)")
        g2[G2_MODE] = a.mode
    if a.achievement is not None:
        if not 0 <= a.achievement <= 100:
            sys.exit("--achievement wants 0..100 (the game clamps it at 100)")
        g2[G2_ACHIEVEMENT] = a.achievement
    # ★ 2026-10-11: story state — event flag f = bit f&7 of ag.q[f>>3] (`ag.a(I)Z`), quest i = the two
    # bits (i&3)*2 of ag.r[i>>2] (`ag.a(I)B`: 0 none · 1 진행 · 2 완료). A scene a test needs to stand in.
    for spec in a.event or []:
        f, _, v = spec.partition("=")
        f, v = int(f, 0), int(v or "1", 0)
        if not 0 <= f < 256 or v not in (0, 1):
            sys.exit("--event wants F or F=0|1 (F 0..255)")
        g2[f >> 3] = (g2[f >> 3] & ~(1 << (f & 7)) | (v << (f & 7))) & 0xFF
    for spec in a.quest or []:
        q, _, s = spec.partition("=")
        q, s = int(q, 0), int(s, 0)
        if not 0 <= q < 128 or s not in (0, 1, 2, 3):
            sys.exit("--quest wants Q=S (Q 0..127, S 0..3)")
        sh = (q & 3) << 1
        g2[32 + (q >> 2)] = (g2[32 + (q >> 2)] & ~(3 << sh) | (s << sh)) & 0xFF
    # ★ 2026-10-20: the beasts' level bn.d — blob [4] = [n][len][m0 k0 … m3 k3][bn.a u32][bn.d u16][bn.e]… (the LV 99 backup's beast reads 0x0063)
    if a.beast_level is not None:
        if not 1 <= a.beast_level <= 99:
            sys.exit("--beast-level wants 1..99")
        b4 = plains[4]
        n, p = b4[0], 1
        for _ in range(n):
            ln = b4[p]
            if ln >= 12:
                struct.pack_into(">H", b4, p + 1 + 10, a.beast_level)
            p += 1 + ln
    out = build_g(plains, rest)
    open(os.path.join(a.dst, name), "wb").write(out)
    print(f"wrote {os.path.join(a.dst, name)} ({len(out)} B)")
    cmd_dump(os.path.join(a.dst, name))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("dump")
    d.add_argument("g")
    e = sub.add_parser("edit")
    e.add_argument("src")
    e.add_argument("dst")
    # 1..5 since 2026-09-27 (the host extends the game's tables — guest_anchors::SaveSlotsPatch).
    e.add_argument("slot", type=int, choices=[1, 2, 3, 4, 5])
    e.add_argument("--class", dest="cls", type=int, choices=[0, 1, 2])
    e.add_argument("--skills")
    e.add_argument("--combo", type=int)
    e.add_argument("--map", type=int)
    e.add_argument("--x", type=int)
    e.add_argument("--y", type=int)
    e.add_argument("--mode", type=int)
    e.add_argument("--achievement", type=int)
    e.add_argument("--level", type=int)
    e.add_argument("--stats")
    e.add_argument("--points", type=int)
    e.add_argument("--skill-points", type=int)
    e.add_argument("--hp", type=int)
    e.add_argument("--sp", type=int)
    e.add_argument("--event", action="append", help="event flag F or F=0|1 (repeatable, e.g. 0x10)")
    e.add_argument("--beast-level", type=int, help="every owned beast's level bn.d (1..99)")
    e.add_argument("--quest", action="append", help="quest state Q=S (repeatable, e.g. 0x0a=2)")
    a = ap.parse_args()
    if a.cmd == "dump":
        cmd_dump(a.g)
    else:
        cmd_edit(a)


if __name__ == "__main__":
    main()
