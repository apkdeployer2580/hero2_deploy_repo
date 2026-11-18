import struct
import sys

# ⚠ hero3 KTF scratch (2026-09-04: the path was hardcoded `d:/tmp/client.bin`, a file
# that no longer exists on any machine). Every offset below is the HERO3 `client.bin`
# build's; on hero2 they mean nothing — see guest_anchors.rs.
if len(sys.argv) < 2:
    sys.exit("usage: %s <client.bin>" % sys.argv[0])
data = open(sys.argv[1], "rb").read()
# find literal-pool words == 0x000009a8 (the sl offset for the WAIT string slot) in code area (< 0xb2c40)
val=struct.pack("<I",0x000009a8)
i=0
print("literal 0x9a8 occurrences (potential 'ldr rX,[pc]; add rX,sl' for WAIT string):")
while True:
    j=data.find(val,i)
    if j<0: break
    if j < 0xb2c40:  # code region
        print(f"  fo=0x{j:06x} guest=0x{j+0x100000:06x}")
    i=j+1
