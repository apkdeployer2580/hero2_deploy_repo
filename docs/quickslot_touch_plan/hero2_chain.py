# 스크래치 → 계획서 증거용. bw.a → 자식 창 → 팝업(d) 체인과 라벨 byte[] 를 probe 덤프에서 찍는다.
# usage: python hero2_chain.py <dump_dir> <frame> <hero2_ktf.zip>   (덤프 = --dump-mem 0x100000:0x96434,0x48000000:0x8000000)
# S2 에서 tools_re/hero2_chain.py 로 승격할 것.
import struct, sys, os
sys.path.insert(0, "tools_re")
from dump_hero2_java import load_image, parse
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
def u8(a):
    b = rd(a, 1); return None if b is None else b[0]
def i8(a):
    v = u8(a); return None if v is None else (v - 256 if v >= 128 else v)
# class name lookup by ptr_class: JavaClass struct -> ptr_descriptor -> name.
from dump_hero2_java import Image as _Img
img = parse(_Img(load_image(sys.argv[3]), False))
desc_by_addr = {}
for c in img:
    desc_by_addr[c["desc_guest"]] = c["name"]
def cls_name(obj):
    pc = u32(obj + 4)
    if pc is None: return f"?pc=None"
    pd = u32(pc + 8)  # JavaClass +8 = ptr_descriptor
    return desc_by_addr.get(pd, f"?pc={pc:#x} pd={pd}")
def fields(obj): return u32(obj)
def show_window(obj, label):
    pf = fields(obj)
    print(f"{label}: obj={obj:#x} class={cls_name(obj)} fields={pf:#x} "
          f"parent(+0)={u32(pf+4+0):#x} child(+4)={u32(pf+4+4):#x} "
          f"h(cursor)={i8(pf+4+8)} i(rows)={i8(pf+4+0xc)} j={i8(pf+4+0x10)} k={i8(pf+4+0x14)} l={i8(pf+4+0x18)} "
          f"d={u8(pf+4+0x1c)} e={u8(pf+4+0x20)} f={u8(pf+4+0x24)}")
    return pf
def bytes_arr(handle):
    pf = u32(handle); n = u32(pf+4); return rd(pf+8, n)
bw = u32(0x19422c)
print(f"bw.a singleton @0x19422c = {bw:#x}   bw.a:I={u32(0x1941ec)} bw.b:I={u32(0x1941fc)}  bw.a:Z={u8(0x19423c)} bw.a:B={u8(0x19424c)}")
if bw:
    pf = show_window(bw, "field menu (bw)")
    child = u32(pf+4+4)
    if child:
        pf2 = show_window(child, "  child")
        # skill window extras (s): +0x28 a:Z, +0x2c b:Z(tab), +0x30 d:B, +0x34 a:La;
        print(f"    s.a:Z(+0x28)={u8(pf2+4+0x28)} s.b:Z(tab,+0x2c)={u8(pf2+4+0x2c)} s.d:B(+0x30)={i8(pf2+4+0x30)} s.a:La;(+0x34)={u32(pf2+4+0x34):#x}")
        print(f"    s statics: a:B@0x19443c={i8(0x19443c)} b:B@0x19444c={i8(0x19444c)} c:B@0x19445c={i8(0x19445c)}")
        gchild = u32(pf2+4+4)
        if gchild:
            pf3 = show_window(gchild, "    grandchild")
            kind = i8(pf3+4+0x28); labels = u32(pf3+4+0x2c); height = u32(pf3+4+0x30)
            print(f"      popup d: kind(+0x28)={kind} labels(+0x2c)={labels:#x} height(+0x30)={height}")
            lpf = u32(labels); n = u32(lpf+4)
            print(f"      labels array: fields={lpf:#x} len={n}")
            for i in range(n):
                h = u32(lpf+8+4*i)
                b = bytes_arr(h)
                print(f"        [{i}] handle={h:#x} bytes={b.hex()} len={len(b)} text={b.decode('cp949', 'replace')!r}")
