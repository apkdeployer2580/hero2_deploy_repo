import struct
import sys

# ⚠ hero3 KTF scratch (2026-09-04: the path was hardcoded `d:/tmp/client.bin`, a file
# that no longer exists on any machine). Every offset below is the HERO3 `client.bin`
# build's; on hero2 they mean nothing — see guest_anchors.rs.
if len(sys.argv) < 2:
    sys.exit("usage: %s <client.bin>" % sys.argv[0])
data = open(sys.argv[1], "rb").read()
SL_FO = 0xb2c40  # RWPI static base file offset (guest 0x1b2c40)
def rdw(fo): return struct.unpack_from("<I", data, fo)[0]
def read_kr(fo, maxlen=64):
    b = data[fo:fo+maxlen]
    end = b.find(b"\x00")
    if end>=0: b=b[:end]
    try: s=b.decode("euc-kr")
    except: s=repr(b)
    return s
for imm in [0x18,0x16c,0x480,0x484,0x488,0x48c,0x490,0x494,0x498,0x49c,0x494,0x494]:
    slot_fo = SL_FO+imm
    w = rdw(slot_fo)
    tgt = (w + 0xeec) & 0xffffffff
    tgt_fo = tgt  # already a file offset per address model
    note=""
    # target may be a string (data) or an object; try string decode
    s = read_kr(tgt_fo)
    print(f"sl+0x{imm:03x}  word=0x{w:08x}  tgt_fo=0x{tgt_fo:06x}  str={s!r}")
