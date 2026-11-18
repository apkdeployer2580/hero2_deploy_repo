# hero2 (KTF 1.0.7) Thumb 디스어셈블러 — 자바 메타데이터로 주석을 단다.
#
# ── 왜 이 도구가 필요한가 (2026-09-18, 대화 선택지 RE) ─────────────────────────
# hero2 의 AOT 코드는 static 필드·메서드를 **이름으로 늦게 해석**한다. 함수마다
#   ldr r0, [pc, #imm] ; add r0, sl ; bl 0x167e84(읽기) / 0x16845c(쓰기) / 0x16916e(호출)
# 꼴이고, `sl + imm` 이 가리키는 **셀(cell)** 의 워드가 홀수면 미해석 참조다:
#   static 필드   : 정의 주소 = 0x193860 + ((w & 0xffff) >> 1) * 0x10   (슬롯 = +12; 읽기 0x167e84 쓰기 0x16845c)
#   인스턴스 필드 : 정의 주소 = 0x18367c + ((w & 0xffff) >> 1) * 0x10   (오프셋 = +12; 읽기 0x1680a4 쓰기 0x168004)
#   메서드        : 정의 주소 = 0x185644 + ((w & 0xffff) >> 1) * 0x1c   (호출 0x16916e/0x16917c/0x169160)
# 세 표의 인덱스 공간이 같아서 셀만 봐서는 겹친다 — 뒤따르는 bl 이 어느 헬퍼인지가 답이다.
# 런타임이 한 번 해석하면 셀에 정의 포인터를 되써 넣는다(0x167e84 본문 참조). 그래서
# `client.bin` 의 리터럴을 이 규칙으로 풀면 `bb.a(II)V` 가 "mode==3 이면 LEFT/RIGHT 가
# bb.d:Z 를 뒤집는다" 로 읽힌다 — 그것이 `HERO2_GUEST_RE.md` §3.3 의 근거다.
#
# 좌표계는 dump_hero2_java.py 와 같다: 게스트 = 파일오프셋 + 0x100000, 저장 포인터 W →
# 파일오프셋 W + 0xbf10. RWPI 정적 베이스 sl = 0x195ca4 (모든 메서드가 `add sl, pc` 로
# 같은 값을 만든다 — bb 클래스의 메서드 셋에서 실측).
#
# 사용법:
#   python tools_re/hero2_dis.py <hero2_ktf.zip|client.bin1120> 'bb.a(II)V'
#   python tools_re/hero2_dis.py <...> 0x1234e4 [len]
import bisect
import io
import struct
import sys
import zipfile

from capstone import CS_ARCH_ARM, CS_MODE_LITTLE_ENDIAN, CS_MODE_THUMB, Cs

sys.path.insert(0, __file__.rsplit("/", 1)[0] if "/" in __file__ else ".")
from dump_hero2_java import Image, load_image, parse  # noqa: E402

IMAGE = 0x100000
RELOC = 0xBF10
SL = 0x195CA4
STATIC_DEFS = 0x193860
INSTANCE_DEFS = 0x18367C
METHOD_DEFS = 0x185644


def load_bin(path):
    if path.lower().endswith(".zip"):
        z = zipfile.ZipFile(path)
        jar = [n for n in z.namelist() if n.endswith(".jar")][0]
        return zipfile.ZipFile(io.BytesIO(z.read(jar))).read("client.bin1120")
    return open(path, "rb").read()


def main():
    path, spec = sys.argv[1], sys.argv[2]
    length = int(sys.argv[3], 0) if len(sys.argv) > 3 else None
    D = load_bin(path)
    J = parse(Image(load_image(path), False))
    methods, by_body, statics, mdefs, fdefs, descs = [], {}, {}, {}, {}, {}
    for c in J:
        descs[c["desc_guest"]] = c["name"]
        descs[c["jc_ptr"] + IMAGE] = "JC:" + c["name"]
        for f in c["fields"]:
            if f["access"] & 8:
                statics[f["slot_guest"]] = f"{c['name']}.{f['name']}:{f['desc']}"
                fdefs[f["def_guest"]] = f"{c['name']}.{f['name']}:{f['desc']}@{f['slot_guest']:#x}"
            else:
                # instance field: the cell is used with the get/put-field helpers
                # (0x1680a4 / 0x168004); the offset is into the object's field block (+4).
                fdefs[f["def_guest"]] = f"{c['name']}.{f['name']}:{f['desc']} (inst +{f['offset_or_value']:#x})"
        for m in c["methods"]:
            mdefs[m["def_fo"] + IMAGE] = f"{c['name']}.{m['name']}{m['desc']}"
            if m["own"] and m["body_guest"]:
                methods.append((m["body_guest"] & ~1, f"{c['name']}.{m['name']}{m['desc']}"))
                by_body[m["body_guest"] & ~1] = f"{c['name']}.{m['name']}{m['desc']}"
    methods.sort()
    starts = [a for a, _ in methods]

    def owner(addr):
        i = bisect.bisect_right(starts, addr) - 1
        return methods[i][1] if i >= 0 else "?"

    def annotate_addr(g):
        if g in statics:
            return statics[g]
        if g in mdefs:
            return "mdef " + mdefs[g]
        if g in descs:
            return "class " + descs[g]
        if (g & ~1) in by_body:
            return "body " + by_body[g & ~1]
        return None

    md = Cs(CS_ARCH_ARM, CS_MODE_THUMB | CS_MODE_LITTLE_ENDIAN)

    def dis(start, n):
        for ins in md.disasm(D[start - IMAGE : start - IMAGE + n], start):
            note = ""
            if ins.mnemonic == "ldr" and "[pc" in ins.op_str:
                try:
                    imm = int(ins.op_str.split("#")[1].rstrip("]"), 0)
                except (IndexError, ValueError):
                    imm = 0
                lit = ((ins.address + 4) & ~3) + imm
                w = struct.unpack_from("<I", D, lit - IMAGE)[0]
                g = w + RELOC + IMAGE
                a = annotate_addr(g)
                note = f"; lit={w:#x}" + (f" -> {g:#x} {a}" if a else (f" -> {g:#x}" if IMAGE <= g < 0x200000 else ""))
                if w >= 0xFFF00000:
                    cell = (SL + w) & 0xFFFFFFFF
                    try:
                        cw = struct.unpack_from("<I", D, cell - IMAGE)[0]
                    except struct.error:
                        cw = 0
                    if cw & 1:
                        # The same index space is read by three helpers with three bases:
                        # static field (0x167e84/0x16845c), INSTANCE field (0x1680a4/0x168004)
                        # and method (0x16916e/0x16917c/0x169160). Show every table it lands in.
                        idx = (cw & 0xFFFF) >> 1
                        hits = []
                        if STATIC_DEFS + idx * 0x10 in fdefs:
                            hits.append("static " + fdefs[STATIC_DEFS + idx * 0x10])
                        if INSTANCE_DEFS + idx * 0x10 in fdefs:
                            hits.append("field " + fdefs[INSTANCE_DEFS + idx * 0x10])
                        if METHOD_DEFS + idx * 0x1C in mdefs:
                            hits.append("method " + mdefs[METHOD_DEFS + idx * 0x1C])
                        note = f"; cell {cell:#x} -> " + (" | ".join(hits) if hits else f"? {cw:#x}")
                    else:
                        note = f"; cell {cell:#x} -> resolved {cw:#x}"
            elif ins.mnemonic in ("bl", "blx"):
                try:
                    note = f"; -> {owner(int(ins.op_str.lstrip('#'), 0))}"
                except ValueError:
                    pass
            print(f"{ins.address:#x}: {ins.mnemonic:6} {ins.op_str:28} {note}")

    if "." in spec:
        cls, name = spec.split(".", 1)
        for a, n in [(a, n) for a, n in methods if n.startswith(cls + "." + name)]:
            i = starts.index(a)
            end = starts[i + 1] if i + 1 < len(starts) else a + 0x400
            print(f"===== {n} @ {a:#x} .. {end:#x}")
            dis(a, length or (end - a))
    else:
        dis(int(spec, 16) & ~1, length or 0x200)


if __name__ == "__main__":
    main()
