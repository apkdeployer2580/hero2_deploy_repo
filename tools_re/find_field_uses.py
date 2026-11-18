# hero2 (KTF 1.0.7) — 어느 메서드가 이 필드를 읽고 쓰는가 (전 필드 상호참조).
#
# ── 왜 이 도구인가 (2026-09-23) ──────────────────────────────────────────────
# `HERO2_GUEST_RE.md` §8.5 의 교훈은 「데이터 **바이트**를 읽는 자리는 못 찾는다」였다 —
# 배열 원소는 레지스터 오프셋으로 읽히고 필드 접근은 헬퍼 뒤에 숨기 때문이다. 그런데 그
# 「헬퍼 뒤」가 사실은 찾을 수 있는 자리다: hero2 의 AOT 코드는 필드를 **늦게 해석**하고,
# 접근마다
#     ldr rD, [pc, #imm]   ; 셀
#     ... (몇 명령)
#     bl  <헬퍼>
# 를 낸다. 셀 워드의 **하위 16비트가 필드 정의를 가리키는 홀수 인덱스**다(hero2_dis.py 머리말):
#
#   셀 주소      = sl + (부호 있는 리터럴),  sl = 0x195ca4 (RWPI 정적 베이스, 전 메서드 공통)
#   인스턴스 필드  정의 = 0x18367c + ((w & 0xffff) >> 1) * 0x10   (읽기 0x1680a4 쓰기 0x168004)
#   static 필드    정의 = 0x193860 + ((w & 0xffff) >> 1) * 0x10   (읽기 0x167e84 쓰기 0x16845c)
#
# ⛔ **리터럴 자체는 셀 주소가 아니다** — `ldr` 가 읽는 워드는 sl 기준 **오프셋**(대개 음수)이고
#    `add rD, sl` 이 더해서야 셀이 된다. 실측: lit@0x15df38 = -88856 → sl+lit = 0x18018c.
#
# ⛔ **셀을 「그 값을 가진 워드」로 찾으면 안 된다** — 이미지 전체에서 홀수 하위16이 유효한
#    정의를 가리키는 워드가 12,790 개 나온다(대부분 그냥 데이터다, 실측). 그래서 이 도구는
#    반대로 간다: **코드에서 `ldr rD,[pc,#imm]` 를 전수**하고 그 자리의 워드를 셀로 풀어 본 뒤,
#    **뒤따르는 첫 `bl` 이 네 헬퍼 중 하나일 때만** 접근으로 인정한다. 헬퍼가 증거다.
#    (상위 16비트도 무엇인가를 나른다 — 같은 클래스의 필드들이 같은 값을 갖는 것이 관측된다 —
#    그러나 판정에 쓰지 않는다. 헬퍼만으로 충분하고, 모르는 것에 기대지 않는다.)
#
# ⚠ 셀은 런타임에 정의 포인터로 덮어써진다. 이 도구는 `client.bin1120`(재배치 전 디스크
#   이미지)을 읽으므로 언제나 미해석 값을 본다 — 살아 있는 힙이 아니라 이미지를 볼 것.
# ⚠ 배열 필드는 접근지가 「배열 참조를 집는 곳」이지 원소를 읽는 곳이 아니다.
#
# 사용법:
#   python tools_re/find_field_uses.py <client.bin1120|hero2_ktf.zip> a.d --desc S
#   python tools_re/find_field_uses.py <...> a.d --desc S --writes
#   python tools_re/find_field_uses.py <...> ag.j --static
#   python tools_re/find_field_uses.py <...> --all
import argparse
import bisect
import os
import struct
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from dump_hero2_java import Image, IMAGE_BASE, load_image, parse  # noqa: E402

INST_TABLE = 0x18367C
STATIC_TABLE = 0x193860
SL = 0x195CA4  # RWPI 정적 베이스 — 모든 메서드가 `add sl, pc` 로 같은 값을 만든다
HELPERS = {
    0x1680A4: ("inst", "get"),
    0x168004: ("inst", "put"),
    0x167E84: ("static", "get"),
    0x16845C: ("static", "put"),
}


def method_index(classes):
    """본문 시작 주소 → 'cls.name(desc)' 의 정렬된 표."""
    entries = []
    for c in classes:
        for m in c["methods"]:
            if m["own"] and m["body_guest"] is not None:
                entries.append((m["body_guest"], f"{c['name']}.{m['name']}{m['desc']}"))
    entries.sort()
    return [e[0] for e in entries], [e[1] for e in entries]


def scan(img, classes):
    """(클래스, 이름, desc, static) -> [(주소, 'get'|'put', 소유 메서드)]"""
    d = img.d
    defs = {}
    for c in classes:
        for f in c["fields"]:
            defs[f["def_guest"]] = (c["name"], f["name"], f["desc"])
    starts, names = method_index(classes)
    uses = defaultdict(list)
    for fo in range(0, len(d) - 1, 2):
        (ins,) = struct.unpack_from("<H", d, fo)
        if (ins & 0xF800) != 0x4800:  # LDR rD, [PC, #imm8*4]
            continue
        lit_fo = ((fo + IMAGE_BASE + 4) & ~3) + (ins & 0xFF) * 4 - IMAGE_BASE
        if not (0 <= lit_fo <= len(d) - 4):
            continue
        (lit,) = struct.unpack_from("<i", d, lit_fo)
        cell_fo = SL + lit - IMAGE_BASE
        if not (0 <= cell_fo <= len(d) - 4):
            continue
        (w,) = struct.unpack_from("<I", d, cell_fo)
        lo = w & 0xFFFF
        if not (lo & 1):
            continue
        # 뒤따르는 첫 bl 이 헬퍼인가 — 이것이 「이 워드는 셀이다」의 증거다.
        helper = None
        for k in range(1, 9):
            j = fo + 2 * k
            if j + 3 >= len(d):
                break
            w0, w1 = struct.unpack_from("<HH", d, j)
            if (w0 & 0xF800) == 0xF000 and (w1 & 0xF800) == 0xF800:
                off = ((w0 & 0x7FF) << 12) | ((w1 & 0x7FF) << 1)
                if off & (1 << 22):
                    off -= 1 << 23
                helper = HELPERS.get(fo + IMAGE_BASE + 2 * k + 4 + off)
                break
        if helper is None:
            continue
        kind, op = helper
        base = STATIC_TABLE if kind == "static" else INST_TABLE
        got = defs.get(base + (lo >> 1) * 0x10)
        if got is None:
            continue
        i = bisect.bisect_right(starts, fo + IMAGE_BASE) - 1
        uses[(got[0], got[1], got[2], kind == "static")].append(
            (fo + IMAGE_BASE, op, names[i] if i >= 0 else "?")
        )
    return uses


def report(key, sites, writes_only):
    cname, fname, desc, static = key
    if writes_only:
        sites = [s for s in sites if s[1] == "put"]
        if not sites:
            return
    print("%s.%s:%s  %s  %d site(s)"
          % (cname, fname, desc, "static" if static else "inst", len(sites)))
    bym = defaultdict(list)
    for addr, op, owner in sites:
        bym[owner].append((addr, op))
    for owner in sorted(bym, key=lambda o: bym[o][0][0]):
        ops = bym[owner]
        print("    %-46s %2d [%s]  %s"
              % (owner, len(ops), ",".join(sorted({o for _, o in ops})),
                 " ".join("0x%06x" % a for a, _ in ops[:10])))


def main():
    # ⚠ 이 기계의 콘솔은 cp949 라 그냥 쓰면 UnicodeEncodeError 로 죽는다.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("field", nargs="?", help="클래스.필드 — 예 a.d")
    ap.add_argument("--desc", help="같은 이름이 여럿이면 디스크립터로 가른다 — 예 S")
    ap.add_argument("--static", action="store_true")
    ap.add_argument("--writes", action="store_true", help="쓰는 곳만")
    ap.add_argument("--all", action="store_true", help="접근지가 있는 필드 전부")
    args = ap.parse_args()

    img = Image(load_image(args.image), relocated=False)
    classes = parse(img)
    uses = scan(img, classes)
    print("[xref] %d field(s) with at least one access site" % len(uses))

    if args.all:
        for key in sorted(uses, key=lambda k: (k[0] or "", k[1] or "", k[2] or "")):
            report(key, uses[key], args.writes)
        return 0
    if not args.field:
        ap.error("field or --all")
    cname, _, fname = args.field.rpartition(".")
    keys = [k for k in uses
            if k[0] == cname and k[1] == fname
            and (args.desc is None or k[2] == args.desc)
            and k[3] == args.static]
    if not keys:
        print("  no access site for %s (static=%s desc=%s)" % (args.field, args.static, args.desc))
        return 1
    for k in keys:
        report(k, uses[k], args.writes)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
