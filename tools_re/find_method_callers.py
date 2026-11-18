# hero2 (KTF 1.0.7) — 이 메서드를 누가 부르는가.
#
# `find_field_uses.py` 와 같은 사슬인데 표가 다르다. AOT 코드의 메서드 호출도 늦게 해석된다:
#     ldr rD, [pc, #imm]   ; 리터럴 = sl 기준 오프셋
#     add rD, sl           ; 셀 = sl + 리터럴,  sl = 0x195ca4
#     bl  0x16916e | 0x16917c | 0x169160
# 셀 워드의 하위 16비트가 메서드 정의를 가리킨다:
#     정의 = 0x185644 + ((w & 0xffff) >> 1) * 0x1c        (JavaMethodDefinition = 0x1c B)
#
# ⛔ 필드 표와 **인덱스 공간이 겹친다** — 뒤따르는 `bl` 이 어느 헬퍼인지가 유일한 판별이다.
#   그래서 여기서는 호출 헬퍼 셋만 인정한다.
#
# 사용법:
#   python tools_re/find_method_callers.py <client.bin1120|hero2_ktf.zip> 'bn.d()V'
#   python tools_re/find_method_callers.py <...> --all          # 호출지가 있는 메서드 전부
import argparse
import bisect
import os
import struct
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from dump_hero2_java import Image, IMAGE_BASE, load_image, parse  # noqa: E402

METHOD_TABLE = 0x185644
METHOD_STRIDE = 0x1C
SL = 0x195CA4
CALL_HELPERS = {0x16916E: "virtual", 0x16917C: "virtual", 0x169160: "direct"}


def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("method", nargs="?", help="예 'bn.d()V'")
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()

    img = Image(load_image(args.image), relocated=False)
    classes = parse(img)
    d = img.d

    # 정의 주소 -> 이름
    defs = {}
    starts = []
    for c in classes:
        for m in c["methods"]:
            if m["own"]:
                defs[img.guest(m["def_fo"])] = f"{c['name']}.{m['name']}{m['desc']}"
                if m["body_guest"] is not None:
                    starts.append((m["body_guest"], f"{c['name']}.{m['name']}{m['desc']}"))
    starts.sort()
    addrs = [a for a, _ in starts]
    names = [n for _, n in starts]

    calls = defaultdict(list)
    for fo in range(0, len(d) - 1, 2):
        (ins,) = struct.unpack_from("<H", d, fo)
        if (ins & 0xF800) != 0x4800:
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
        kind = None
        for k in range(1, 9):
            j = fo + 2 * k
            if j + 3 >= len(d):
                break
            w0, w1 = struct.unpack_from("<HH", d, j)
            if (w0 & 0xF800) == 0xF000 and (w1 & 0xF800) == 0xF800:
                off = ((w0 & 0x7FF) << 12) | ((w1 & 0x7FF) << 1)
                if off & (1 << 22):
                    off -= 1 << 23
                kind = CALL_HELPERS.get(fo + IMAGE_BASE + 2 * k + 4 + off)
                break
        if kind is None:
            continue
        callee = defs.get(METHOD_TABLE + (lo >> 1) * METHOD_STRIDE)
        if callee is None:
            continue
        i = bisect.bisect_right(addrs, fo + IMAGE_BASE) - 1
        calls[callee].append((fo + IMAGE_BASE, kind, names[i] if i >= 0 else "?"))

    print("[xref] %d method(s) with at least one call site" % len(calls))
    want = sorted(calls) if args.all else [args.method]
    for m in want:
        sites = calls.get(m)
        if not sites:
            print("  no call site for %s" % m)
            continue
        print("%s  %d site(s)" % (m, len(sites)))
        by = defaultdict(list)
        for addr, kind, owner in sites:
            by[owner].append((addr, kind))
        for owner in sorted(by, key=lambda o: by[o][0][0]):
            print("    %-46s %2d [%s]  %s"
                  % (owner, len(by[owner]), ",".join(sorted({k for _, k in by[owner]})),
                     " ".join("0x%06x" % a for a, _ in by[owner][:8])))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
