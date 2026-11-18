import sys

# ⚠ hero3 KTF scratch (2026-09-04: the path was hardcoded `d:/tmp/client.bin`, a file
# that no longer exists on any machine). Every offset below is the HERO3 `client.bin`
# build's; on hero2 they mean nothing — see guest_anchors.rs.
if len(sys.argv) < 2:
    sys.exit("usage: %s <client.bin>" % sys.argv[0])
data = open(sys.argv[1], "rb").read()
def kr(fo,n=90):
    b=data[fo:fo+n]; e=b.find(b"\x00")
    if e>=0:b=b[:e]
    try:return b.decode("euc-kr")
    except Exception as ex:return "<%s>"%ex
slots={"0x48c/PHONE-auth":0x0a9600,"0x490/SMS":0x0a9618,"0x494/payload":0x0a962c,
       "0x498":0x0a9638,"0x49c/savefile":0x0a9650,
       "WAIT@0xaa880":0x0aa880,"cashshop@0xaa8d0":0x0aa8d0,"warehouse-full@0xa9fe9":0x0a9fe9,
       "idle5min@0xab1c0":0x0ab1c0,"conn-closed@0xaa837":0x0aa837,"conn-fail@0xaa807":0x0aa807,
       "money-down@0xaa751":0x0aa751,"orb@0xaa8bc":0x0aa8bc,"save-slot@0xaa910":0x0aa910}
out=[]
for k,fo in slots.items():
    out.append("%-24s fo=0x%06x  %s"%(k,fo,kr(fo)))
open(sys.argv[2] if len(sys.argv) > 2 else "kr_clean.txt", "w", encoding="utf-8").write("\n".join(out))
print("ok")
