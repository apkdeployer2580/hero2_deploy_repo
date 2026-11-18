# 스크래치 → 계획서 증거용. 정적 슬롯이 가리키는 자바 배열을 probe 덤프에서 읽는다.
# usage: python rd_static_array.py <dump_dir> <frame> 0x193bac:1 0x193bbc:4   (슬롯:원소바이트)
import struct, sys, os
d = sys.argv[1]; frame = int(sys.argv[2])
chunks = []
for f in os.listdir(d):
    if f.startswith(f"mem_{frame:05d}_0x"):
        base = int(f.split("_0x")[1].split(".")[0], 16)
        chunks.append((base, open(os.path.join(d, f), "rb").read()))
def rd(a, n):
    for b, data in chunks:
        if b <= a and a + n <= b + len(data):
            return data[a-b:a-b+n]
    return None
def u32(a):
    b = rd(a, 4); return None if b is None else struct.unpack("<I", b)[0]
def arr(handle, esz, signed=False):
    pf = u32(handle); n = u32(pf + 4)
    raw = rd(pf + 8, n * esz)
    fmt = {1: "b" if signed else "B", 2: "h" if signed else "H", 4: "i" if signed else "I"}[esz]
    return pf, n, list(struct.unpack("<" + fmt * n, raw))
for spec in sys.argv[3:]:
    slot, esz = spec.split(":")
    slot = int(slot, 0); esz = int(esz)
    h = u32(slot)
    if h is None or h == 0:
        print(f"slot {slot:#x} -> {h}"); continue
    pf, n, vals = arr(h, esz, signed=True)
    print(f"slot {slot:#x} -> handle {h:#x} -> fields {pf:#x} len={n} {vals[:32]}")
