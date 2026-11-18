import struct
import sys

# ⚠ hero3 KTF scratch (2026-09-04: the path was hardcoded `d:/tmp/client.bin`, a file
# that no longer exists on any machine). Every offset below is the HERO3 `client.bin`
# build's; on hero2 they mean nothing — see guest_anchors.rs.
if len(sys.argv) < 2:
    sys.exit("usage: %s <client.bin>" % sys.argv[0])
data = open(sys.argv[1], "rb").read()
SL_FO=0xb2c40
need=0x000a9994  # relocword pointing to the WAIT string
# 1) find data slots (in RWPI static area) holding this pointer value
print("data slots holding 0x%08x:"%need)
i=0
while True:
    j=data.find(struct.pack("<I",need),i)
    if j<0: break
    imm=j-SL_FO
    print(f"  fo=0x{j:06x}  sl+0x{imm:x} (imm {imm})")
    i=j+1
# 2) also print field-0x40 clears near dispatcher context: search stores of 0 to +0x40? (hard) -- skip
