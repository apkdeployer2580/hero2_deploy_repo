# hero2 (KTF 1.0.7) 의 AOT 자바 메타데이터를 통째로 꺼낸다.
#
# ── 왜 이 도구가 hero2 RE 의 마스터키인가 ────────────────────────────────────
# hero3 는 심볼 0 개의 C 바이너리였다. hero2 는 그렇지 않다 — `client.bin1120` 은
# AOT 컴파일된 자바 MIDlet 이고, 그래서 **클래스·필드·메서드의 이름과 디스크립터와
# 오프셋과 본문주소가 이미지 안에 표로 들어 있다**. 이 도구가 그 표를 읽는다.
# (근거·좌표계 = HERO2_FEATURE_PARITY.md §7·§8)
#
# ── 좌표계 ──────────────────────────────────────────────────────────────────
#   게스트주소 = 파일오프셋 + IMAGE_BASE(0x100000)
#   저장 포인터 W (재배치 전) → 파일오프셋 = W + RELOC_DELTA(0xbf10)
# `--relocated` 를 주면 `wie_ktf_dump` 산출물(재배치 후)로 보고 델타를 0 으로 둔다.
#
# ── 구조체 (vendor/wie 의 `wipi_types::ktf::java` 그대로) ────────────────────
#   JavaClassDescriptor  0x24 B  ptr_name, unk1, ptr_parent_class, ptr_methods,
#                                ptr_interfaces, ptr_fields, method_count:u16,
#                                fields_size:u16, access_flag:u16, unk6..8:u16
#   JavaFieldDefinition  0x10 B  access_flags, ptr_class, ptr_name, offset_or_value
#   JavaMethodDefinition 0x1c B  fn_body, ptr_class, fn_body_native_or_extab,
#                                ptr_name, extab_count:u16, unk3:u16,
#                                index_in_vtable:u16, access_flags:u16, unk6
#   ptr_methods / ptr_fields 는 **NULL 종단 포인터 배열**이다.
#   이름 레코드 = `tag(1B) + descriptor + '+' + name + NUL`
#
# ⚠ 메서드 본문 주소는 **Thumb 비트가 켜진 홀수값**이다 — 디스어셈은 `&~1`.
# ⚠ 인스턴스 필드 실주소 = `*(obj+0) + offset + 4`  (class_instance.rs)
#    static 필드 저장 주소 = 필드정의주소 + 12 (= offset_or_value 칸 자신)
#
# 사용법:
#   python tools_re/dump_hero2_java.py <hero2_ktf.zip|client.bin1120> [--json out.json]
#   python tools_re/dump_hero2_java.py <...> --grep hp        # 이름/디스크립터 검색
#   python tools_re/dump_hero2_java.py <...> --class a        # 한 클래스 상세
#   python tools_re/dump_hero2_java.py <...> --relocated dumped.bin

import argparse
import io
import json
import struct
import sys
import zipfile

IMAGE_BASE = 0x100000
RELOC_DELTA = 0xBF10
# 클래스 디스크립터 테이블 (HERO2_FEATURE_PARITY.md §8). 끝은 스캔으로 다시 확인한다.
CLASS_TABLE_FO = 0x092238
CLASS_TABLE_END_FO = 0x093144  # 마지막 레코드의 시작 오프셋 (포함) → 108 개
DESC_STRIDE = 0x24
# `JavaClass` = ptr_next + unk1 + ptr_descriptor + ptr_vtable + vtable_count:u16 + unk_flag:u16
JC_STRIDE = 0x14

ACC = [
    (0x0001, "public"),
    (0x0002, "private"),
    (0x0004, "protected"),
    (0x0008, "static"),
    (0x0010, "final"),
    (0x0020, "synchronized"),
    (0x0100, "native"),
    (0x0200, "interface"),
    (0x0400, "abstract"),
]


def load_image(path):
    """zip / jar / 생 바이너리 중 무엇을 줘도 client.bin* 을 돌려준다."""
    with open(path, "rb") as f:
        head = f.read(4)
        f.seek(0)
        raw = f.read()
    if head[:2] != b"PK":
        return raw
    zf = zipfile.ZipFile(io.BytesIO(raw))
    # 에셋 zip 은 jar 를 품고 있고, jar 는 client.bin<bss> 를 품고 있다.
    for name in zf.namelist():
        if name.lower().endswith(".jar"):
            inner = zipfile.ZipFile(io.BytesIO(zf.read(name)))
            for n2 in inner.namelist():
                if n2.rsplit("/", 1)[-1].startswith("client.bin"):
                    return inner.read(n2)
    for name in zf.namelist():
        if name.rsplit("/", 1)[-1].startswith("client.bin"):
            return zf.read(name)
    raise SystemExit(f"{path}: client.bin* 을 못 찾았다 (members={zf.namelist()[:8]})")


class Image:
    def __init__(self, data, relocated):
        self.d = data
        self.delta = 0 if relocated else RELOC_DELTA

    def fo(self, ptr):
        """저장 포인터 → 파일오프셋. 범위 밖이면 None."""
        if ptr == 0:
            return None
        if self.delta:
            off = ptr + self.delta
        else:
            off = ptr - IMAGE_BASE
        if 0 <= off < len(self.d):
            return off
        return None

    def guest(self, fo):
        return fo + IMAGE_BASE

    def u32(self, fo):
        if fo is None or fo + 4 > len(self.d):
            return None
        return struct.unpack_from("<I", self.d, fo)[0]

    def u16(self, fo):
        if fo is None or fo + 2 > len(self.d):
            return None
        return struct.unpack_from("<H", self.d, fo)[0]

    def cstr(self, fo, limit=512):
        if fo is None:
            return None
        end = self.d.find(b"\0", fo, fo + limit)
        if end < 0:
            return None
        return self.d[fo:end]

    def name_record(self, ptr):
        """`tag + descriptor + '+' + name + NUL` → (tag, name, descriptor)."""
        fo = self.fo(ptr)
        if fo is None:
            return None
        tag = self.d[fo]
        body = self.cstr(fo + 1)
        if body is None:
            return None
        try:
            body = body.decode("utf-8")
        except UnicodeDecodeError:
            body = body.decode("latin-1")
        if "+" in body:
            desc, _, nm = body.partition("+")
        else:
            desc, nm = body, ""
        return tag, nm, desc

    def ptr_table(self, ptr, cap=4096):
        """NULL 종단 포인터 배열."""
        out = []
        fo = self.fo(ptr)
        if fo is None:
            return out
        while len(out) < cap:
            v = self.u32(fo)
            if v is None or v == 0:
                break
            out.append(v)
            fo += 4
        return out


def flags(v):
    return " ".join(n for b, n in ACC if v & b)


def javaclass_map(img):
    """저장 포인터(JavaClass) → 디스크립터 파일오프셋.

    `JavaClass` 는 16 B 이고 +8 이 `ptr_descriptor` 다. 이미지를 훑어서 그 칸이
    디스크립터 표를 가리키는 레코드를 모으면, 클래스마다 **자기 JavaClass 주소**가
    생긴다 — `ptr_parent_class` 와 메서드의 `ptr_class` 가 그 주소로 말한다.
    """
    want = set(range(CLASS_TABLE_FO, CLASS_TABLE_END_FO + 1, DESC_STRIDE))
    cand = []
    for fo in range(0, len(img.d) - JC_STRIDE, 4):
        pd = struct.unpack_from("<I", img.d, fo + 8)[0]
        dfo = img.fo(pd)
        if dfo in want:
            cand.append((fo, dfo))
    # ⚠ 이 검사만으로는 오탐이 많다 (임의의 16 B 창이 우연히 표를 가리킨다). 진짜
    # `JavaClass` 들은 **stride 0x14 의 한 테이블**에 붙어 있으므로 (2026-09-09 실측:
    # fo 0x94be8 대역, 클래스 `a` = fo 0x95250) 가장 긴 등차 런만 남긴다. 오탐을 그냥
    # 두면 `desc_to_jc` 가 음수 포인터를 물고 필드 소유자 판정이 통째로 죽는다.
    runs, prev = [], None
    for fo, dfo in cand:
        if prev is not None and fo - prev == JC_STRIDE:
            runs[-1].append((fo, dfo))
        else:
            runs.append([(fo, dfo)])
        prev = fo
    best = max(runs, key=len) if runs else []
    lo, hi = (best[0][0], best[-1][0]) if best else (0, 0)
    out = {}
    for fo, dfo in cand:
        if not (lo <= fo <= hi):
            continue
        stored = fo - img.delta if img.delta else fo + IMAGE_BASE
        out[stored] = dfo
    return out


def parse(img):
    classes = []
    jc = javaclass_map(img)
    desc_to_jc = {}
    for stored, dfo in jc.items():
        desc_to_jc.setdefault(dfo, stored)
    fo = CLASS_TABLE_FO
    idx = 0
    while fo <= CLASS_TABLE_END_FO:
        (ptr_name, unk1, ptr_parent, ptr_methods, ptr_ifaces, ptr_fields) = struct.unpack_from("<6I", img.d, fo)
        method_count, fields_size, access_flag, unk6, unk7, unk8 = struct.unpack_from("<6H", img.d, fo + 24)
        nm = img.cstr(img.fo(ptr_name))
        name = nm.decode("latin-1") if nm else None
        cls = {
            "index": idx,
            "desc_fo": fo,
            "desc_guest": img.guest(fo),
            "name": name,
            "parent_ptr": ptr_parent,
            "fields_size": fields_size,
            "access": access_flag,
            "method_count": method_count,
            "unk": [unk1, unk6, unk7, unk8],
            # 이 클래스의 JavaClass 구조체 (메서드 소유 판정과 부모 해석의 키).
            "jc_ptr": desc_to_jc.get(fo),
            "parent_desc_fo": jc.get(ptr_parent),
            "fields": [],
            "methods": [],
        }
        for fptr in img.ptr_table(ptr_fields):
            ffo = img.fo(fptr)
            if ffo is None:
                continue
            acc, pcls, pname, offval = struct.unpack_from("<4I", img.d, ffo)
            # ⚠ 메서드 배열과 마찬가지로 필드 배열도 **상속 체인 전체**를 나른다.
            # `ptr_class` 로 가르지 않으면 부모/자식 필드가 같은 오프셋에서 겹쳐 보이고
            # 타입이 뒤섞인다 (2026-09-09 실측: 클래스 `a` 의 +0x78 이 `B` 로 읽혔다).
            if cls["jc_ptr"] is not None and pcls != cls["jc_ptr"]:
                continue
            rec = img.name_record(pname)
            cls["fields"].append(
                {
                    "def_fo": ffo,
                    "def_guest": img.guest(ffo),
                    "access": acc,
                    "name": rec[1] if rec else None,
                    "desc": rec[2] if rec else None,
                    # static 이면 이 칸 자신이 값 저장소다 (주소 = def+12)
                    "offset_or_value": offval,
                    "slot_guest": img.guest(ffo + 12),
                }
            )
        for mptr in img.ptr_table(ptr_methods):
            mfo = img.fo(mptr)
            if mfo is None:
                continue
            fn_body, pcls, native, pname = struct.unpack_from("<4I", img.d, mfo)
            extab, unk3, vidx, macc = struct.unpack_from("<4H", img.d, mfo + 16)
            rec = img.name_record(pname)
            # 같은 배열이 상속 체인의 메서드도 나른다 — 소유자를 JavaClass 로 가른다.
            own = cls["jc_ptr"] is not None and pcls == cls["jc_ptr"]
            cls["methods"].append(
                {
                    "def_fo": mfo,
                    "name": rec[1] if rec else None,
                    "desc": rec[2] if rec else None,
                    "access": macc,
                    "vtable_index": vidx,
                    # ⚠ Thumb 비트 포함. 디스어셈 대상은 body & ~1
                    "body_ptr": fn_body,
                    "body_fo": img.fo(fn_body & ~1),
                    "body_guest": (img.guest(img.fo(fn_body & ~1)) if img.fo(fn_body & ~1) is not None else None),
                    "ptr_class": pcls,
                    "own": own,
                }
            )
        classes.append(cls)
        fo += DESC_STRIDE
        idx += 1
    by_fo = {c["desc_fo"]: c for c in classes}
    for c in classes:
        pd = c["parent_desc_fo"]
        c["parent"] = by_fo[pd]["name"] if pd in by_fo else None
    return classes


def inherited_fields(classes, name):
    """`name` 의 인스턴스 필드를 **절대 오프셋**으로 펼친다.

    ★★ 2026-09-09 실측으로 확정된 레이아웃 규칙: 객체의 필드 블록은 **부모 필드가 먼저**
    오고 그 뒤에 자기 필드가 붙는다. 그래서

        절대 오프셋 = (조상들의 fields_size 합) + 선언된 offset_or_value

    이고, `fields_size` 는 **그 클래스 자기 몫**이다 (클래스 `a` = 0xf4 = 61 필드 x 4 B).
    이 규칙 없이 선언 오프셋을 그대로 쓰면 부모·자식 필드가 같은 오프셋에서 겹쳐 보이고
    타입이 전부 뒤섞인다 — 캐릭터 레코드에서 `[S` 필드가 정수로, `B` 필드가 힙 포인터로
    읽히던 것이 그 증상이었다. 규칙을 적용하면 배열 필드 5개가 전부 힙 포인터가 되고
    int 필드가 전부 int 값이 된다 (= 독립 검증).
    """
    by_name = {c["name"]: c for c in classes if c["name"]}
    # 루트부터 내려오는 조상 목록.
    chain, seen, cur = [], set(), name
    while cur and cur in by_name and cur not in seen:
        seen.add(cur)
        chain.append(by_name[cur])
        cur = by_name[cur]["parent"]
    chain.reverse()
    out, base = [], 0
    for c in chain:
        for f in c["fields"]:
            if not (f["access"] & 0x8):
                out.append(dict(f, owner=c["name"], abs_off=base + f["offset_or_value"]))
        base += c["fields_size"]
    out.sort(key=lambda f: f["abs_off"])
    return out


def object_size(classes, name):
    """객체 필드 블록의 총 바이트 (조상 포함)."""
    by_name = {c["name"]: c for c in classes if c["name"]}
    total, seen, cur = 0, set(), name
    while cur and cur in by_name and cur not in seen:
        seen.add(cur)
        total += by_name[cur]["fields_size"]
        cur = by_name[cur]["parent"]
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("--relocated", action="store_true", help="입력이 wie_ktf_dump 산출물(재배치 후)이다")
    ap.add_argument("--json")
    ap.add_argument("--grep", help="이름/디스크립터 부분일치 검색")
    ap.add_argument("--klass", "--class", dest="klass", help="이 클래스만 상세 출력")
    ap.add_argument("--fields-size", type=lambda s: int(s, 0), help="이 fields_size 를 가진 클래스만")
    args = ap.parse_args()

    data = load_image(args.image)
    img = Image(data, args.relocated)
    classes = parse(img)

    named = [c for c in classes if c["name"]]
    print(
        f"[hero2-java] {len(data)} B image · {len(classes)} descriptors · {len(named)} named · "
        f"{sum(len(c['fields']) for c in classes)} fields · {sum(len(c['methods']) for c in classes)} methods",
        file=sys.stderr,
    )

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump(classes, f, ensure_ascii=False, indent=1)
        print(f"[hero2-java] wrote {args.json}", file=sys.stderr)

    def show(c, detail):
        print(
            f"class {c['name']}  desc@{c['desc_guest']:#x}  fields_size={c['fields_size']:#x}  "
            f"nfields={len(c['fields'])}  nmethods={len(c['methods'])}  parent={c.get('parent')}  access={flags(c['access'])}"
        )
        if not detail:
            return
        for fl in c["fields"]:
            st = fl["access"] & 0x8
            where = f"slot@{fl['slot_guest']:#x}" if st else f"+{fl['offset_or_value']:#x}"
            print(f"    {'S' if st else ' '} {fl['desc'] or '?':<24} {fl['name'] or '?':<8} {where}")
        for m in c["methods"]:
            bg = f"{m['body_guest']:#x}" if m["body_guest"] else "-"
            print(f"    M {m['desc'] or '?':<44} {m['name'] or '?':<8} body={bg:>10} v={m['vtable_index']:<4} {flags(m['access'])}")

    if args.klass:
        for c in classes:
            if c["name"] == args.klass:
                show(c, True)
        return
    if args.grep:
        q = args.grep.lower()
        for c in classes:
            hits = [f for f in c["fields"] if q in (f["name"] or "").lower() or q in (f["desc"] or "").lower()]
            mhits = [m for m in c["methods"] if q in (m["name"] or "").lower() or q in (m["desc"] or "").lower()]
            if q in (c["name"] or "").lower() or hits or mhits:
                print(f"class {c['name']}  fields_size={c['fields_size']:#x}")
                for f in hits:
                    st = f["access"] & 0x8
                    where = f"slot@{f['slot_guest']:#x}" if st else f"+{f['offset_or_value']:#x}"
                    print(f"    {'S' if st else ' '} {f['desc']} {f['name']} {where}")
                for m in mhits:
                    bg = f"{m['body_guest']:#x}" if m["body_guest"] else "-"
                    print(f"    M {m['desc']} {m['name']} body={bg}")
        return
    for c in classes:
        if args.fields_size is not None and c["fields_size"] != args.fields_size:
            continue
        show(c, False)


if __name__ == "__main__":
    main()
