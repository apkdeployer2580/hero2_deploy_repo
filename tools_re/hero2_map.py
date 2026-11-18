# hero2 (KTF 1.0.7) — 맵 파일(`P/m/m/NN.m.gz`) 전 구간 파서 · 빌더.
#
# ── 왜 이 모듈인가 (2026-10-16, 신전 보스 레이드) ──────────────────────────────────
# 지금까지의 맵 편집은 이벤트 구간(A·B·C)만 제자리에서 고쳤다(`make_hero2_asset.patch_map`). 신전 기둥을
# 귀환문으로 바꾸고(타일 · 속성 · 오브젝트 · 불꽃 · 트리거) 보스방을 복제해 레이드 맵을 만들려면 **구간 전부**를
# 읽고 다시 써야 한다. 이 모듈은 로더 `bf.a([B)V` @0x129a94 가 읽는 순서 그대로 한 장을 사전으로 풀고(`parse`)
# 같은 순서로 되감는다(`build`). 배포본 111 장 전부가 **바이트 동일**로 왕복한다(`verify`) — 그것이 형식을 맞게
# 읽었다는 유일한 증거다. 이벤트 구간 A 의 시작은 `decode_map_events.find_events` 의 탐색 결과와 111/111 일치한다.
#
# 형식 (정본 = HERO2_GUEST_RE.md §8.28 · §8.22 · decode_map_events.py 머리말):
#   [u8 타일셋 번호 → bf.c (`/m/t0N`)] [u8 n][이름 cp949] [u8 n][f1 n B] [u8 n][f2 n B = 이 맵이 쓰는 obj 프레임 목록]
#   [u8 W][u8 H] [타일 H×W: 값 & 0x7f 가 1 이상이면 타일셋 프레임 (값 − 1), 0·0x7f = 빈 칸] [속성 H×W: 부호 있는 바이트]
#   by   [s8 n]{ [u8 k][k × 3 B (obj 프레임, ?, ?)] }      — `f` 객체의 애니메이션 (n ≤ 0 이면 바이트 하나뿐)
#   bi×3 [u8 u]{ [u8 obj 프레임][i16 x][i16 y] }             — 오브젝트 세 층 (x = 칸 px + 8, y = 칸 px)
#   f    [u8 u]{ [u8 by 번호][i16 x][i16 y] }                — 애니메이션 객체 (귀환문의 불꽃)
#   몬스터 [u8 n][n × 전역 적 인덱스] [u8 m][m × (적 인덱스, x, y)]
#   NPC   [u8 n][n × npc 레코드 번호] [u8 m][m × (레코드, x, y)]
#   A  [u8 nA]{ [u8 m][m × 31 B] }   B  [u8 nB]{ [u8 len][cp949] }   C  [u8 nC]{ [u8 m][m × 3 B] }
#   D  [u8 n]{ [u8 m][m × 3 B] }   E  [u8 k]{ 4 B } [u8 L]{ [u8 j][j × 4 B] }   — 파일 끝
#
# 사용법:
#   python tools_re/hero2_map.py verify <zip>            # 전 맵 왕복 (바이트 동일이어야 exit 0)
#   python tools_re/hero2_map.py dump <zip> <맵>          # 한 장의 구간 요약
import gzip
import struct
import sys
import zipfile

MAP_PREFIX = "P/m/m/"


def map_name(n):
    return f"{MAP_PREFIX}{n:02d}.m.gz"


def read_map(zf, n):
    return gzip.decompress(zf.read(map_name(n)))


def parse(d):
    """맵 한 장(압축 푼 바이트) → 구간 사전. 끝이 파일 끝과 다르면 ValueError."""
    d = bytes(d)
    m = {}
    p = 0
    m["tileset"] = d[0]
    p = 1
    n = d[p]
    m["name"] = d[p + 1 : p + 1 + n]
    p += 1 + n
    n = d[p]
    m["f1"] = d[p + 1 : p + 1 + n]
    p += 1 + n
    n = d[p]
    m["f2"] = d[p + 1 : p + 1 + n]
    p += 1 + n
    w, h = d[p], d[p + 1]
    p += 2
    m["w"], m["h"] = w, h
    m["tiles"] = bytearray(d[p : p + w * h])
    p += w * h
    m["attr"] = bytearray(d[p : p + w * h])
    p += w * h
    n = d[p]
    p += 1
    m["by_raw"] = n
    by = []
    if n < 128:
        for _ in range(n):
            k = d[p]
            by.append([tuple(d[p + 1 + 3 * i : p + 4 + 3 * i]) for i in range(k)])
            p += 1 + 3 * k
    m["by"] = by
    objs = []
    for _ in range(4):
        u = d[p]
        p += 1
        lst = []
        for _i in range(u):
            a = d[p]
            x, y = struct.unpack_from("<hh", d, p + 1)
            lst.append((a, x, y))
            p += 5
        objs.append(lst)
    m["objs"] = objs  # [bi 층 0, bi 층 1, bi 층 2, f]
    n = d[p]
    m["kinds"] = list(d[p + 1 : p + 1 + n])
    p += 1 + n
    n = d[p]
    m["spawns"] = [tuple(d[p + 1 + 3 * i : p + 4 + 3 * i]) for i in range(n)]
    p += 1 + 3 * n
    n = d[p]
    m["npc_kinds"] = list(d[p + 1 : p + 1 + n])
    p += 1 + n
    n = d[p]
    m["npcs"] = [tuple(d[p + 1 + 3 * i : p + 4 + 3 * i]) for i in range(n)]
    p += 1 + 3 * n
    m["events_at"] = p
    n = d[p]
    p += 1
    groups = []
    for _ in range(n):
        k = d[p]
        p += 1
        groups.append([bytes(d[p + 31 * i : p + 31 * i + 31]) for i in range(k)])
        p += 31 * k
    m["groups"] = groups
    n = d[p]
    p += 1
    lines = []
    for _ in range(n):
        k = d[p]
        lines.append(bytes(d[p + 1 : p + 1 + k]))
        p += 1 + k
    m["lines"] = lines
    n = d[p]
    p += 1
    scripts = []
    for _ in range(n):
        k = d[p]
        scripts.append([tuple(d[p + 1 + 3 * i : p + 4 + 3 * i]) for i in range(k)])
        p += 1 + 3 * k
    m["scripts"] = scripts
    n = d[p]
    p += 1
    cuts = []
    for _ in range(n):
        k = d[p]
        cuts.append([tuple(d[p + 1 + 3 * i : p + 4 + 3 * i]) for i in range(k)])
        p += 1 + 3 * k
    m["cuts"] = cuts
    n = d[p]
    p += 1
    m["load_entries"] = [tuple(d[p + 4 * i : p + 4 * i + 4]) for i in range(n)]
    p += 4 * n
    n = d[p]
    p += 1
    lists = []
    for _ in range(n):
        k = d[p]
        lists.append([tuple(d[p + 1 + 4 * i : p + 5 + 4 * i]) for i in range(k)])
        p += 1 + 4 * k
    m["load_lists"] = lists
    if p != len(d):
        raise ValueError(f"map ends at {p}, file is {len(d)} B — format misread")
    return m


def _u8(v, what):
    if not 0 <= v <= 255:
        raise ValueError(f"{what} = {v} does not fit a byte")
    return bytes((v,))


def build(m):
    """`parse` 의 사전 → 바이트 (배포본은 바이트 동일로 되돌아온다)."""
    w, h = m["w"], m["h"]
    if len(m["tiles"]) != w * h or len(m["attr"]) != w * h:
        raise ValueError("tiles/attr are not W×H")
    o = bytearray(_u8(m["tileset"], "tileset"))
    o += _u8(len(m["name"]), "name") + bytes(m["name"])
    o += _u8(len(m["f1"]), "f1") + bytes(m["f1"])
    o += _u8(len(m["f2"]), "f2") + bytes(m["f2"])
    o += bytes((w, h)) + bytes(m["tiles"]) + bytes(m["attr"])
    if m["by"]:
        o += _u8(len(m["by"]), "by")
        for rec in m["by"]:
            o += _u8(len(rec), "by rec") + b"".join(bytes(t) for t in rec)
    else:
        o += _u8(m["by_raw"] if m["by_raw"] >= 128 else 0, "by")
    if len(m["objs"]) != 4:
        raise ValueError("objs must hold the three bi layers and the f list")
    for lst in m["objs"]:
        o += _u8(len(lst), "objs")
        for a, x, y in lst:
            o += _u8(a, "obj frame") + struct.pack("<hh", x, y)
    o += _u8(len(m["kinds"]), "kinds") + bytes(m["kinds"])
    o += _u8(len(m["spawns"]), "spawns") + b"".join(bytes(t) for t in m["spawns"])
    o += _u8(len(m["npc_kinds"]), "npc kinds") + bytes(m["npc_kinds"])
    o += _u8(len(m["npcs"]), "npcs") + b"".join(bytes(t) for t in m["npcs"])
    o += _u8(len(m["groups"]), "groups")
    for g in m["groups"]:
        o += _u8(len(g), "group")
        for r in g:
            if len(r) != 31:
                raise ValueError("a trigger record is not 31 B")
            o += bytes(r)
    o += _u8(len(m["lines"]), "lines")
    for line in m["lines"]:
        o += _u8(len(line), "line") + bytes(line)
    o += _u8(len(m["scripts"]), "scripts")
    for s in m["scripts"]:
        o += _u8(len(s), "script")
        for c in s:
            if len(c) != 3:
                raise ValueError("a script command is not 3 B")
            o += bytes(c)
    o += _u8(len(m["cuts"]), "cuts")
    for s in m["cuts"]:
        o += _u8(len(s), "cut") + b"".join(bytes(t) for t in s)
    o += _u8(len(m["load_entries"]), "load entries") + b"".join(bytes(t) for t in m["load_entries"])
    o += _u8(len(m["load_lists"]), "load lists")
    for lst in m["load_lists"]:
        o += _u8(len(lst), "load list") + b"".join(bytes(t) for t in lst)
    return bytes(o)


def pack(m):
    """`build` 를 gzip 으로 (mtime 0 — 같은 입력이면 같은 바이트)."""
    return gzip.compress(build(m), mtime=0)


def record(kind, script, quest=(), event=(), job=0, level=0):
    """31 B 트리거 레코드: 종류(0 자동 · 2 말걸기 · 3 밟기), 스크립트 번호, 조건 (퀘스트, 상태)×≤3 · (이벤트, 1)×≤3.
    `(0, 1)` 쌍이 게임의 「조건 없음」이다(decode_map_events.py)."""
    r = bytearray(31)
    r[0], r[1], r[2] = job, level, kind
    for i in range(3):
        r[3 + 2 * i : 5 + 2 * i] = bytes((0, 1))
        r[9 + 2 * i : 11 + 2 * i] = bytes((0, 1))
    for i, (q, s) in enumerate(quest):
        r[3 + 2 * i : 5 + 2 * i] = bytes((q, s))
    for i, (f, v) in enumerate(event):
        r[9 + 2 * i : 11 + 2 * i] = bytes((f, v))
    r[30] = script
    return bytes(r)


def tile(m, x, y):
    return m["tiles"][y * m["w"] + x]


def attr(m, x, y):
    v = m["attr"][y * m["w"] + x]
    return v - 256 if v >= 128 else v


def set_tile(m, x, y, v):
    m["tiles"][y * m["w"] + x] = v & 0xFF


def set_attr(m, x, y, v):
    m["attr"][y * m["w"] + x] = v & 0xFF


def load_all(zf):
    """{번호: 압축 푼 바이트} — 이름이 숫자인 멤버만."""
    out = {}
    for nm in zf.namelist():
        if nm.startswith(MAP_PREFIX) and nm.endswith(".m.gz"):
            stem = nm[len(MAP_PREFIX) : -len(".m.gz")]
            if stem.isdigit():
                out[int(stem)] = gzip.decompress(zf.read(nm))
    return out


def verify(zpath):
    zf = zipfile.ZipFile(zpath)
    maps = load_all(zf)
    bad = []
    for n in sorted(maps):
        d = maps[n]
        try:
            m = parse(d)
        except Exception as e:  # noqa: BLE001 — a misread is the finding
            bad.append((n, f"parse: {e}"))
            continue
        if build(m) != d:
            bad.append((n, "rebuild differs"))
    return len(maps), bad


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    cmd, zpath = sys.argv[1], sys.argv[2]
    if cmd == "verify":
        total, bad = verify(zpath)
        for n, why in bad:
            print(f"map {n}: {why}")
        print(f"{total} map(s), {len(bad)} bad")
        return 1 if bad else 0
    if cmd == "dump":
        m = parse(read_map(zipfile.ZipFile(zpath), int(sys.argv[3])))
        print(f"{m['name'].decode('cp949', 'replace')} tileset t{m['tileset']:02d} {m['w']}x{m['h']} f1={list(m['f1'])} f2={list(m['f2'])}")
        print(f"by {m['by']}")
        for i, lst in enumerate(m["objs"]):
            print(f"objs[{i}] ({len(lst)}): {lst}")
        print(f"kinds {m['kinds']} spawns {m['spawns']} npc_kinds {m['npc_kinds']} npcs {m['npcs']}")
        for gi, g in enumerate(m["groups"]):
            for ri, r in enumerate(g):
                print(f"g{gi} r{ri}: {r.hex(' ')}")
        for i, line in enumerate(m["lines"]):
            print(f"line {i}: {line.decode('cp949', 'replace')}")
        for i, s in enumerate(m["scripts"]):
            print(f"s{i}: {' · '.join(f'{a:02x} {b:02x} {c:02x}' for a, b, c in s)}")
        print(f"cuts {len(m['cuts'])} load_entries {m['load_entries']} load_lists {m['load_lists']}")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
