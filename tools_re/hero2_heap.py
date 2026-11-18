# hero2 런타임 힙 해독기 — 자바 메타데이터 + probe 메모리 덤프를 합친다.
#
# `dump_hero2_java.py` 가 이미지에서 꺼낸 클래스/필드 표를 들고, `wie_probe --dump-mem`
# 산출물을 주소공간으로 붙여, **살아 있는 객체를 이름 붙은 필드로** 읽는다.
#
#   객체 P (JavaClassInstance) :  *(P+0) = ptr_fields,  *(P+4) = ptr_class
#   인스턴스 필드 실주소       :  ptr_fields + offset + 4      (class_instance.rs)
#   배열 핸들도 같은 모양이고, 원소는 ptr_fields + 4 부터 (아래 `arr` 참조)
#
# 재배치 후 이미지 안의 포인터는 절대 게스트주소다. 이미지 영역을 덤프하지 않았어도
# 파일에서 `W + 0x10bf10` 으로 같은 값을 얻을 수 있다 (§8 의 RELOC_DELTA).
#
# 사용법:
#   python tools_re/hero2_heap.py <hero2_ktf.zip> <dump_dir> [--frame 1200]
#     --chain 0x193b5c                 # static 슬롯에서 체인 따라가기
#     --find 709,780,390               # u16/u32 값 동시검색 (같은 배열/객체 안)
#     --obj 0x4001234                  # 객체 하나를 필드 이름으로 펼치기

import argparse
import glob
import os
import re
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dump_hero2_java import (  # noqa: E402
    IMAGE_BASE,
    RELOC_DELTA,
    Image,
    inherited_fields,
    load_image,
    object_size,
    parse,
)

# 재배치 후 이미지 포인터 = 저장값 + 이 값.
RELOC_ABS = RELOC_DELTA + IMAGE_BASE


class Space:
    """probe 덤프들을 하나의 주소공간으로 본다."""

    def __init__(self):
        self.chunks = []  # (base, bytes)

    def add(self, base, data):
        self.chunks.append((base, data))
        self.chunks.sort()

    def read(self, addr, n):
        for base, d in self.chunks:
            if base <= addr and addr + n <= base + len(d):
                return d[addr - base : addr - base + n]
        return None

    def u32(self, addr):
        b = self.read(addr, 4)
        return None if b is None else struct.unpack("<I", b)[0]

    def u16(self, addr):
        b = self.read(addr, 2)
        return None if b is None else struct.unpack("<H", b)[0]

    def i16(self, addr):
        b = self.read(addr, 2)
        return None if b is None else struct.unpack("<h", b)[0]

    def u8(self, addr):
        b = self.read(addr, 1)
        return None if b is None else b[0]


def is_heap(p):
    return p is not None and 0x4000_0000 <= p < 0x5000_0000


class World:
    def __init__(self, zip_path, dump_dir, frame):
        self.img = Image(load_image(zip_path), False)
        self.classes = parse(self.img)
        self.by_name = {c["name"]: c for c in self.classes if c["name"]}
        # 디스크립터 게스트주소 → 클래스
        self.by_desc = {c["desc_guest"]: c for c in self.classes}
        self.sp = Space()
        pat = os.path.join(dump_dir, f"mem_{frame:05d}_*.bin")
        for f in sorted(glob.glob(pat)):
            m = re.search(r"_0x([0-9a-fA-F]+)\.bin$", f)
            base = int(m.group(1), 16)
            with open(f, "rb") as fh:
                self.sp.add(base, fh.read())
        if not self.sp.chunks:
            raise SystemExit(f"no dumps matched {pat}")
        # JavaClass(16B) 주소 → 클래스. ptr_descriptor 가 디스크립터 표를 가리키는 것을
        # 이미지·힙 양쪽에서 찾는다.
        self.class_at = {}
        for base, d in self.sp.chunks:
            for off in range(0, len(d) - 16, 4):
                pd = struct.unpack_from("<I", d, off + 8)[0]
                c = self.by_desc.get(pd)
                if c is not None:
                    self.class_at[base + off] = c
        # 이미지 안의 JavaClass 도 (덤프 없이) 파일에서 찾는다.
        fd = self.img.d
        for off in range(0, len(fd) - 16, 4):
            pd = struct.unpack_from("<I", fd, off + 8)[0]
            c = self.by_desc.get(pd + RELOC_ABS)
            if c is not None:
                self.class_at.setdefault(off + IMAGE_BASE, c)

    # ---- 객체 ----
    def klass_of(self, obj):
        pc = self.sp.u32(obj + 4)
        if pc is None:
            return None
        return self.class_at.get(pc)

    def fields_base(self, obj):
        return self.sp.u32(obj)

    def field_addr(self, obj, off):
        fb = self.fields_base(obj)
        return None if fb is None else fb + off + 4

    def show_obj(self, obj, depth=0, seen=None):
        seen = seen or set()
        c = self.klass_of(obj)
        pad = "  " * depth
        if c is None:
            print(f"{pad}{obj:#x}: (class unknown, ptr_class={self.sp.u32(obj+4)})")
            return
        print(f"{pad}{obj:#x}: class {c['name']} (extends {c.get('parent')}) objsize={object_size(self.classes, c['name']):#x} fields@{self.fields_base(obj):#x}")
        for fl in inherited_fields(self.classes, c["name"]):
            a = self.field_addr(obj, fl["abs_off"])
            d = fl["desc"] or "?"
            if d in ("B", "Z"):
                v = self.sp.u32(a)
            elif d == "S":
                v = self.sp.u32(a)
                if v is not None and v >= 0x8000:
                    v -= 0x10000
            else:
                v = self.sp.u32(a)
            vs = "?" if v is None else f"{v}" if not is_heap(v) else f"{v:#x}"
            extra = ""
            if d.startswith("[") and is_heap(v):
                n, els = self.arr(v, d[1:])
                extra = f"  len={n} {els[:16]}"
            elif d.startswith("L") and is_heap(v):
                k = self.klass_of(v)
                extra = f"  -> {k['name'] if k else '?'}"
            print(f"{pad}    +{fl['abs_off']:#05x} {d:<20} {fl['owner']}.{fl['name'] or '?':<3} = {vs}{extra}")

    # ---- 배열 ----
    def arr(self, handle, elem):
        """자바 배열 핸들 → (길이, 원소 리스트). 길이는 fields[0] 에 있다."""
        fb = self.sp.u32(handle)
        if fb is None:
            return None, []
        n = self.sp.u32(fb + 4)
        if n is None or n > 1 << 20:
            return n, []
        out = []
        if elem in ("B", "Z"):
            b = self.sp.read(fb + 8, n)
            out = list(b) if b else []
        elif elem in ("S", "C"):
            for i in range(n):
                v = self.sp.i16(fb + 8 + 2 * i)
                out.append(v)
        else:
            for i in range(n):
                out.append(self.sp.u32(fb + 8 + 4 * i))
        return n, out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("zip")
    ap.add_argument("dumps")
    ap.add_argument("--frame", type=int, default=1200)
    ap.add_argument("--chain", type=lambda s: int(s, 0), help="static 슬롯 게스트주소")
    ap.add_argument("--obj", type=lambda s: int(s, 0))
    ap.add_argument("--find", help="쉼표로 구분된 수치들 — 같은 창 안에 다 있는 곳을 찾는다")
    ap.add_argument("--window", type=lambda s: int(s, 0), default=0x80)
    args = ap.parse_args()

    w = World(args.zip, args.dumps, args.frame)
    print(
        f"[heap] {len(w.classes)} classes · {len(w.class_at)} JavaClass structs located · "
        f"{sum(len(d) for _, d in w.sp.chunks)} B of dumps",
        file=sys.stderr,
    )

    if args.chain is not None:
        p = w.sp.u32(args.chain)
        print(f"slot {args.chain:#x} -> {p:#x}" if p is not None else f"slot {args.chain:#x} not in dumps")
        if is_heap(p):
            w.show_obj(p)
    if args.obj is not None:
        w.show_obj(args.obj)
    if args.find:
        vals = [int(x, 0) for x in args.find.split(",")]
        hits = 0
        for base, d in w.sp.chunks:
            pos = {v: [] for v in vals}
            for v in vals:
                pat = struct.pack("<H", v & 0xFFFF)
                i = d.find(pat)
                while i >= 0:
                    pos[v].append(i)
                    i = d.find(pat, i + 1)
            # 첫 값의 각 위치 주변 창에 나머지가 다 있나
            for i in pos[vals[0]]:
                if all(any(abs(j - i) <= args.window for j in pos[v]) for v in vals[1:]):
                    print(f"  hit {base + i:#x}  " + " ".join(f"{v}@{base + min(pos[v], key=lambda j: abs(j - i)):#x}" for v in vals))
                    hits += 1
                    if hits > 200:
                        print("  ... (더 있음)")
                        return
        if not hits:
            print("  no window contained all values")


if __name__ == "__main__":
    main()
