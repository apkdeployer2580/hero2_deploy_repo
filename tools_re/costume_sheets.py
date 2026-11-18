#!/usr/bin/env python3
"""영웅서기2 플레이어 파츠 시트 — 의상 도트 PNG ⇄ 게임 시트(`.mph` + `_0.mpd`).

의상 아바타(HERO2_AVATAR_PLAN.md §8.1 · §8.2)의 새 도트를 게임이 싣는 시트로 싸는 도구. 게임은 「의상」을
모른다 — 파츠 로더 `cf.a(I)V` 가 머리 `hero/0` · 몸통 `hero/1` · 팔 `hero/2` · 다리 `hero/3` · 무기 `hero/6` ·
보조손 `hero/7` 시트를 번호로 싣고(HERO2_GUEST_RE.md §8.35), 호스트의 오버라이드(`guest_anchors::AvatarPatch`)가
그 번호(무기·보조손은 경로 종류까지)만 바꾼다. 그래서 의상 하나 = 이 도구가 만든 시트 몇 장이다(A3b 2026-10-01:
머리·무기(직업별 셋)·보조손(나이트 방패 · 로그 둘째 단검)까지 최대 9 장).

    python tools_re/costume_sheets.py verify                          배포본 시트 재포장 = 바이트 동일 (도구의 첫 검사)
    python tools_re/costume_sheets.py unpack <dir> body:102 arm:100 leg:102 head:05 weapon_knight:207 offhand_knight:1008
                                                                      배포본 시트 → 편집용 PNG (새 의상의 바탕; 무기·보조손은 jar 이름 그대로)
    python tools_re/costume_sheets.py check <costume_dir>             규격 검사만
    python tools_re/costume_sheets.py pack <costume_dir> [out_dir]    PNG → hero/<0·1·2·3>/<번호> · hero/<6·7>/<종류><번호> .mph + _0.mpd
    python tools_re/costume_sheets.py preview <out.png> <look> [<look> …] [--seqs 0,2 | --all] [--frames 0] [--classes 0,1,2] [--scale N] [--fx] [--tint]
                                                                      세 직업 동작 미리보기 (에뮬레이터 없이) — 모습을 나란히
    공통: --zip <hero2_ktf.zip> (기본 = 앱 에셋)

의상 소스 = `costumes/<번호>_<이름>/` 안의 `body_0..4.png` · `arm_0..10.png` · `leg_0..10.png` · `head_0..2.png` ·
`weapon_<berserker|knight|rogue>_<k>.png` · `offhand_<knight|rogue>_<k>.png` (README 참조, 크기표 = PARTS).
미리보기의 <look> 은 의상 디렉터리 또는 배포본 번호 `body:102,arm:100,leg:102`.
`make_hero2_asset.py` 는 `costume_members()` 로 이 디렉터리들을 jar 멤버로 만든다.

FORMAT (2026-09-30, 배포본 372 시트 · 2,539 프레임 전수 — `verify` 가 매번 다시 증명한다)
-----------------------------------------------------------------------------------------
    .mph = [u32 BE 헤드][u32 BE 프레임 수 N]
           N × [u16 BE .mpd 파일 번호 = 0][u32 BE .mpd 안 오프셋][u8 플래그][u8 플래그]
           ★ 2026-10-14 정정: 옛 서술 「[u32 0][u16 오프셋]」 은 같은 바이트의 잘못된 읽기였다 — 게임(`am.a(I)[B` · `am.b(I)[B` ·
           `am.a(I)I` @0x1198a0 · 0x119a10 · 0x119f78)은 앞 u16 으로 `<경로>_<번호>.mpd` 를 고르고 +2 의 u32 를 오프셋으로,
           「다음 항목의 오프셋(같은 파일이면) 또는 파일 끝」 까지를 프레임으로 읽는다 → 시트에 64 KB 한도는 없다.
           진짜 한도는 시퀀스 쪽이다: 그리는 쪽이 프레임 번호를 부호 있는 바이트로 읽으므로(−1 = 건너뜀) 쓸 수 있는 프레임은 0..127.
           [PLTE][tRNS]
    .mpd = 프레임마다 PNG 청크(IHDR … IDAT)를 시그니처·IEND 없이 이어 붙인 것 (dump_mpd.py 머리말)

★ 헤드 0x0c000000(bit 26 = 반전 지원, `am.d()V` @0x1192d8 → `am.b:Z`) 인 시트 335 개는 예외 없이:
  * IDAT = zlib 헤더 `78 01` + **무압축(stored) deflate 블록 하나** + Adler-32, 모든 행의 필터 바이트 0.
    게임이 좌우반전 사본을 **PNG 바이트 안에서** 만들기 때문이다 — `am.a([BIIII)V` @0x11a3a0 가 IDAT 청크
    시작 + 15(길이 4 · 타입 4 · zlib 2 · stored 헤더 5)부터 행을 읽고, 필터 바이트가 0 이 아닌 행이 있으면
    반전을 포기하며, 픽셀을 뒤집은 뒤 Adler-32(`am.b([BII)V`·`am.a()J`)와 CRC(`am.a([BII)V`·`am.b()I`)를
    다시 계산해 `createImage` 한다. ⛔ 압축된 IDAT 로 싸면 반전 사본이 깨진다 — 보통의 PNG 저장기가 내는
    것이 바로 그것이라 이 도구가 필요하다. (헤드 0x08000000 시트 36 개는 전부 압축 `78 9c` = 반전 안 하는 시트.)
  * 비트 깊이 = 팔레트 크기로 정해진다: 4 색 이하 2 · 16 색 이하 4 · 그 이상 8. 한 시트의 프레임은 전부 같고,
    행 끝의 남는 비트는 0.
  * tRNS = 한 바이트 `00` — 팔레트 인덱스 0 만 투명(배포본 79 파츠 시트 전부 #ffffff).
  * 프레임 플래그 = `98 00`. 0x40 = 그 프레임에 bKGD 청크, 둘째 바이트 0x02 = gAMA 청크(hero/4 이펙트 · 일부 적
    시트뿐) — 의상 파트 61 시트는 전부 `98 00` 이고 앞 u32 는 전 시트 0 이다. 0x80·0x10·0x08·0x20 의 뜻은 미해독.

★ 그리는 쪽(`n.a([BIBB[Ljava/lang/Object;ZBBII)B` @0x163374): 직업 시트 `hero/hero<직업>` 의 스프라이트
  `(p | 0x80 반전, f, dx, dy)` 를 목록 순서대로(먼저 = 아래) `Graphics.drawImage(img, x+dx, y+dy, LEFT)` —
  좌상단 기준이고, 반전 사본도 **같은 좌상단**에 그린다. 그래서 프레임 크기는 인덱스마다 고정이다(PARTS 표 —
  크기를 바꾸면 반전 사본이 옆으로 밀린다). `preview` 는 이 규칙 그대로 합성한다.
"""

import io
import os
import struct
import sys
import zipfile
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import venom_effect  # noqa: E402  — 직업 애니메이션 시트(hero/hero0..2) 디코더

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_ZIP = os.path.join(ROOT, "android_app", "app", "src", "main", "assets", "hero2_ktf.zip")
COSTUMES = os.path.join(ROOT, "costumes")

SIG = b"\x89PNG\r\n\x1a\n"
HEAD_FLIP = 0x0C000000  # bit 26 = 반전 사본 지원 → IDAT 무압축 (머리말)
FRAME_LEAD = b"\0\0\0\0"
FRAME_FLAGS = b"\x98\x00"
TRNS = b"\x00"  # 인덱스 0 투명
TRANSPARENT_RGB = (0xFF, 0xFF, 0xFF)  # 배포본의 인덱스 0 색
GRID = {0x00, 0x1F, 0x3F, 0x5F, 0x7F, 0x9F, 0xBF, 0xDF, 0xFF}  # 배포본 파츠 색 채널의 거의 전부 (0x6f 3 · 0xef 4 예외)

# 의상 파트 = 파츠 디렉터리 셋. 크기는 인덱스마다 고정(배포본 24 · 16 · 21 시트 전수, `verify` 가 다시 잰다).
# blank = 1×1 빈 칸(인덱스 0)이 허용되는 프레임 — 갑옷의 덧댄 조각이라 천 옷은 비워도 된다(로그 맨몸 hero/1/102).
PARTS = {
    # ★ 2026-10-01 (사용자: 「이안의 낡은 옷에 망토도 같이 포함」): 프레임 5..8 = **망토** 셋(바람에 날리는 모양 a · b · c)과
    # **스카프** 하나 — 보스 이안·「소문의 검사」의 망토(`enm/5effect/11_0` 11 · 12 · 13 · 10, 그 크기 그대로). 직업 애니메이션이
    # 모든 동작의 몸통 옆에 이 넷을 그린다(`hero_cape.py`). 배포본 몸통 시트는 5 프레임뿐이라 `make_hero2_asset.py` 가 1×1 빈 칸
    # 넷을 덧붙인다 — 그래야 망토 없는 장비·의상에서도 게임이 프레임 5..8 을 읽다가 죽지 않는다(인덱스가 배열 밖이면 던진다).
    # 의상 폴더에 `body_5..8.png` 가 없으면 빈 칸(`optional`). `shipped` = 배포본 시트의 프레임 수.
    "body": {
        "dir": 1,
        "sizes": [(7, 6), (8, 7), (7, 5), (5, 6), (8, 7), (26, 14), (24, 14), (26, 14), (9, 3)],
        "blank": {0, 1, 3, 4, 5, 6, 7, 8},
        "optional": {5, 6, 7, 8},
        "shipped": 5,
    },
    "arm": {
        "dir": 2,
        "sizes": [(4, 6), (5, 7), (4, 7), (6, 8), (6, 8), (10, 7), (4, 5), (6, 10), (4, 6), (8, 5), (11, 5)],
        "blank": set(),
    },
    "leg": {
        "dir": 3,
        "sizes": [(11, 12), (12, 12), (10, 12), (10, 12), (10, 12), (10, 12), (14, 11), (10, 12), (14, 11), (12, 12), (13, 11)],
        "blank": set(),
    },
}
# ★ A3b (2026-10-01, 사용자 「투구·옷·신발·무기 모두」): 머리 · 무기 · 보조손. 크기 = 배포본 시트 전수(머리 18 · 무기 90 ·
# 보조손 31, HERO2_AVATAR_PLAN.md §8.2). 무기·보조손은 직업마다 다르다 — 직업 애니메이션(`hero/hero<직업>`)이 자기 무기
# 종류의 프레임 묶음을 쓰기 때문이다. 로더가 무기·보조손 경로를 `종류 + 번호`(`hero/6/<type><nn>`)로 만들고 호스트가 그
# 종류를 직업으로 정하므로(`guest_anchors::AvatarPatch::path_type`), 의상 시트 이름 = `hero/<dir>/<type><번호>`.
#   class = 이 파트를 그리는 직업(0 버서커 · 1 나이트 · 2 로그) · type = 의상 시트의 경로 종류(path_type 과 같아야 한다)
#   types = 같은 크기표를 쓰는 배포본 종류(verify 가 전부 잰다) · alt = 배포본에 있는 다른 크기(그 프레임만)
# 버서커의 보조손은 없다 — `hero/hero0` 에 파트 7 이 없어 방패를 들어도 그리지 않는다.
PARTS.update(
    {
        "head": {"dir": 0, "sizes": [(16, 14), (15, 14), (16, 14)], "blank": set()},
        "weapon_berserker": {
            "dir": 6,
            "class": 0,
            "type": 0,
            "types": (0, 1),
            "sizes": [(10, 23), (25, 10), (18, 18), (24, 7), (24, 7), (3, 7), (3, 18)],
            "alt": {0: [(10, 24)]},  # hero/6/015
            "blank": set(),
        },
        "weapon_knight": {"dir": 6, "class": 1, "type": 2, "types": (2, 4), "sizes": [(9, 14), (9, 14), (14, 9), (14, 9), (3, 16)], "blank": set()},
        # 로그 단검의 5번째 프레임은 배포본 16 시트 전부 1×1 투명 — 직업 시트가 0..3 만 쓴다.
        "weapon_rogue": {"dir": 6, "class": 2, "type": 3, "types": (3,), "sizes": [(9, 14), (9, 14), (14, 9), (14, 9), (1, 1)], "blank": {4}},
        "offhand_knight": {"dir": 7, "class": 1, "type": 10, "types": (10,), "sizes": [(15, 17), (7, 17), (15, 17)], "blank": set()},
        "offhand_rogue": {
            "dir": 7,
            "class": 2,
            "type": 3,
            "types": (3,),
            "sizes": [(7, 10), (7, 10), (10, 7), (10, 7)],
            "alt": {3: [(11, 7)]},  # 배포본 8 시트
            "blank": set(),
        },
    }
)
# ★ 2026-10-01 (율리케의 웨딩드레스): 드레스의 **치마** — 다리 시트(`hero/3`)를 세트 번호 + 1000 으로 한 장 더(`hero/3/1201`).
# 갑옷 칸 아이템이 치마를 가지면 그 아이템을 입는 동안 다리는 신발 칸이 아니라 치마다(`avatar_state::SKIRT_SHEET_OFFSET`):
# 치마 밑에 신발 칸의 바지가 그려지면 안 되고, 바닥까지 오는 드레스는 신발을 가린다. 크기표는 다리와 같다.
PARTS["skirt"] = {"dir": 3, "sizes": PARTS["leg"]["sizes"], "blank": set(), "offset": 1000}
# ★ 2026-10-09 (사용자: *"드레스류 아이템 입었을떄 신발아바타를 바꿔줘 아무런 변경이없은데 … 앞이나 뒤를 덮지 않는 아바타라면
# 신발아바타를바꿨을떄 변경이 되어야 더 자연스러워"*): 다리가 보이는 드레스(207 코트 자락 · 210 트인 치마)의 치마는 다리와
# **따로** 그린다 — 아랫층 `skirtunder_*`(다리 뒤) · 윗층 `skirtover_*`(다리 앞), 다리와 같은 크기표(층이 없는 프레임은 1×1 빈 칸).
# 다리는 신발 칸이 정한다: `costume_members` 가 `hero/3/<번호 + 1000>` = 아랫층 + 그 세트의 다리 + 윗층(신발 아바타가 없을 때),
# `skirt_combo_members` 가 다리를 가진 다른 의상마다 `hero/3/<번호 × 1000 + 신발 번호>` = 아랫층 + 그 신발의 다리 + 윗층을
# 만든다(`avatar_state::skirt_over_shoes`). 앞뒤를 다 덮는 드레스(201 · 209)는 지금처럼 한 장(`skirt_*`) — 신발을 가린다.
SKIRT_LAYERS = ("skirtunder", "skirtover")
for _layer in SKIRT_LAYERS:
    PARTS[_layer] = {"dir": 3, "sizes": PARTS["leg"]["sizes"], "blank": set(range(len(PARTS["leg"]["sizes"]))), "layer": True}
SKIRT_OVER_SHOES = 1000  # hero/3/<드레스 × 1000 + 신발> — avatar_state::SKIRT_OVER_SHOES 와 같은 값
PART_BY_DIR = {v["dir"]: k for k, v in PARTS.items() if "type" not in v and "offset" not in v and "layer" not in v}
CLASS_KEYS = ["berserker", "knight", "rogue"]


def part_stem(part, n):
    """로더가 여는 파일 이름 — 머리·몸통·팔·다리 = `(n < 10 ? "0" : "") + n`, 무기·보조손은 그 앞에 경로 종류,
    치마는 번호 + 1000(`hero/3/1201`)."""
    s = f"{n + PARTS[part].get('offset', 0):02d}"
    t = PARTS[part].get("type")
    return s if t is None else f"{t}{s}"


def part_size_ok(part, k, size, blank_pixels=None):
    """프레임 k 의 크기가 이 파트에 맞는가 — 표의 크기 · 배포본의 다른 크기(alt) · 빈 칸(1×1)."""
    spec = PARTS[part]
    if size == spec["sizes"][k] or size in spec.get("alt", {}).get(k, ()):
        return True
    return k in spec["blank"] and size == (1, 1) and (blank_pixels is None or blank_pixels == [[0]])

# 새 의상 번호 = 200 번대(HERO2_AVATAR_PLAN.md §8.1). 배포본 번호와 겹치면 안 된다 — 로더가 번호를 경로에 붙이므로
# 같은 번호는 같은 파일이다.
COSTUME_MIN, COSTUME_MAX = 200, 999


class SheetError(Exception):
    pass


# ── PNG 청크 ─────────────────────────────────────────────────────────────────


def chunk(typ, data):
    return len(data).to_bytes(4, "big") + typ + data + zlib.crc32(typ + data).to_bytes(4, "big")


def walk_chunks(buf, pos=0, end=None, where="?"):
    """[(type, data)] from `pos` to `end`; every CRC is checked."""
    end = len(buf) if end is None else end
    out = []
    while pos < end:
        if pos + 12 > end:
            raise SheetError(f"{where}: truncated chunk header at {pos}")
        n = int.from_bytes(buf[pos : pos + 4], "big")
        typ = bytes(buf[pos + 4 : pos + 8])
        if pos + 12 + n > end:
            raise SheetError(f"{where}: chunk {typ!r} at {pos} runs past the end")
        data = bytes(buf[pos + 8 : pos + 8 + n])
        if zlib.crc32(typ + data) != int.from_bytes(buf[pos + 8 + n : pos + 12 + n], "big"):
            raise SheetError(f"{where}: bad CRC on {typ!r} at {pos}")
        out.append((typ, data))
        pos += 12 + n
    return out


def unfilter(raw, w, h, bits_per_px, where):
    """PNG 필터를 푼 행들 (바이트열)."""
    stride = (w * bits_per_px + 7) // 8
    bpp = max(1, bits_per_px // 8)
    rows, prev, p = [], bytearray(stride), 0
    if len(raw) < h * (stride + 1):
        raise SheetError(f"{where}: image data is {len(raw)} B, need {h * (stride + 1)}")
    for _ in range(h):
        ft = raw[p]
        line = bytearray(raw[p + 1 : p + 1 + stride])
        p += 1 + stride
        if ft == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif ft == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 255
        elif ft == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
        elif ft == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        elif ft != 0:
            raise SheetError(f"{where}: unknown filter type {ft}")
        rows.append(bytes(line))
        prev = line
    return rows


def unpack_bits(row, w, depth):
    if depth == 8:
        return list(row[:w])
    per = 8 // depth
    mask = (1 << depth) - 1
    return [(row[x // per] >> (8 - depth * (x % per + 1))) & mask for x in range(w)]


def pack_bits(px, depth):
    if depth == 8:
        return bytes(px)
    per = 8 // depth
    out = bytearray((len(px) + per - 1) // per)
    for x, v in enumerate(px):
        out[x // per] |= v << (8 - depth * (x % per + 1))
    return bytes(out)


def depth_for(colours):
    """배포본 규칙 (머리말): 팔레트 크기 → 비트 깊이."""
    return 2 if colours <= 4 else 4 if colours <= 16 else 8


def stored_idat(raw):
    """zlib `78 01` + stored 블록 하나 + Adler-32 — 게임의 반전 코드가 +15 에서 행을 읽는 모양."""
    if len(raw) > 0xFFFF:
        raise SheetError(f"frame data {len(raw)} B does not fit one stored block")
    n = len(raw)
    return b"\x78\x01\x01" + struct.pack("<HH", n, n ^ 0xFFFF) + raw + zlib.adler32(raw).to_bytes(4, "big")


# ── 시트 ─────────────────────────────────────────────────────────────────────


class Frame:
    def __init__(self, w, h, pixels, flags=FRAME_FLAGS, lead=FRAME_LEAD, extra=()):
        self.w, self.h, self.pixels = w, h, pixels  # pixels = 행 목록, 값 = 팔레트 인덱스
        self.flags, self.lead, self.extra = flags, lead, list(extra)  # extra = IHDR 과 IDAT 사이 청크 (배포본 일부)


class Sheet:
    def __init__(self, palette, frames, head=HEAD_FLIP, trns=TRNS):
        self.palette = palette  # [(r, g, b)] — 인덱스 0 = 투명
        self.frames, self.head, self.trns = frames, head, trns


def sheet_decode(mph, mpd, where="sheet", compressed_from=None):
    """`compressed_from`: 이 번호부터의 프레임은 반전 시트여도 압축 IDAT 를 받는다 — 게임은 반전 목록에 든 프레임만
    PNG 바이트 안에서 뒤집으므로(`am.a([BIIII)V`), 반전되지 않는 프레임은 압축이어도 된다(악마 시트 hero/1/300 의 악마 프레임)."""
    if len(mph) < 8:
        raise SheetError(f"{where}: .mph is {len(mph)} B")
    head, n = struct.unpack(">II", mph[:8])
    table = mph[8 : 8 + 8 * n]
    if len(table) != 8 * n:
        raise SheetError(f"{where}: frame table cut short")
    tail = walk_chunks(mph, 8 + 8 * n, where=where + ".mph")
    kinds = [t for t, _ in tail]
    if kinds != [b"PLTE", b"tRNS"]:
        raise SheetError(f"{where}: .mph tail is {kinds}, expected PLTE + tRNS")
    plte, trns = tail[0][1], tail[1][1]
    if len(plte) % 3:
        raise SheetError(f"{where}: PLTE is {len(plte)} B")
    palette = [tuple(plte[i : i + 3]) for i in range(0, len(plte), 3)]
    # ★ 2026-10-14: 항목 = [u16 .mpd 파일 번호][u32 오프셋][플래그 2] — 게임의 `am.a(I)[B` 가 앞 u16 으로 `<경로>_<번호>.mpd` 를,
    # `am.b(I)[B` · `am.a(I)I` 가 +2 의 u32 로 오프셋을 읽는다(배포본은 전부 파일 0 · 64 KB 안이라 옛 서술 「u32 0 + u16」과 같은 바이트).
    for k in range(n):
        if table[8 * k : 8 * k + 2] != b"\0\0":
            raise SheetError(f"{where}: frame {k} lives in .mpd file {int.from_bytes(table[8 * k : 8 * k + 2], 'big')} — only file 0 is read here")
    offs = [int.from_bytes(table[8 * k + 2 : 8 * k + 6], "big") for k in range(n)] + [len(mpd)]
    frames = []
    for k in range(n):
        if not offs[k] <= offs[k + 1]:
            raise SheetError(f"{where}: frame {k} offsets run backwards")
        ch = walk_chunks(mpd, offs[k], offs[k + 1], where=f"{where} frame {k}")
        if not ch or ch[0][0] != b"IHDR":
            raise SheetError(f"{where} frame {k}: does not start at IHDR")
        w, h, depth, ctype, comp, filt, lace = struct.unpack(">IIBBBBB", ch[0][1])
        if (ctype, comp, filt, lace) != (3, 0, 0, 0):
            raise SheetError(f"{where} frame {k}: IHDR {ctype, comp, filt, lace} is not plain indexed")
        idat = [d for t, d in ch if t == b"IDAT"]
        extra = [(t, d) for t, d in ch[1:] if t != b"IDAT"]
        if len(idat) != 1 or ch[-1][0] != b"IDAT":
            raise SheetError(f"{where} frame {k}: want one IDAT, last")
        if head == HEAD_FLIP and idat[0][:2] != b"\x78\x01" and (compressed_from is None or k < compressed_from):
            raise SheetError(f"{where} frame {k}: flip sheet with a compressed IDAT")
        rows = unfilter(zlib.decompress(idat[0]), w, h, depth, f"{where} frame {k}")
        pixels = [unpack_bits(r, w, depth) for r in rows]
        frames.append(Frame(w, h, pixels, flags=table[8 * k + 6 : 8 * k + 8], extra=extra))
    return Sheet(palette, frames, head=head, trns=trns)


def sheet_encode(sheet, any_head=False, compress_from=None):
    """(mph, mpd) — 반전 지원 시트의 정규형 (머리말). `any_head`: 반전 없는 시트(`img/gmico` 0x08000000)도
    같은 꼴(무압축 IDAT — 유효한 zlib 스트림이라 어느 인플레이터든 푼다)로 싼다 — 2026-10-03 발도 아이콘.
    `compress_from`: 이 번호부터의 프레임은 압축 IDAT(zlib 9) — 반전 목록에 절대 들지 않는 프레임만(★ 2026-10-14 악마 시트;
    `sheet_decode(compressed_from=…)` 가 짝). ★ 오프셋은 u32(항목 +2) — 게임에 64 KB 한도는 없다(`sheet_decode` 주석)."""
    if sheet.head != HEAD_FLIP and not any_head:
        raise SheetError("only flip sheets (head 0x0c000000) are encoded — the costume parts are all flip sheets")
    colours = len(sheet.palette)
    if not 1 <= colours <= 256:
        raise SheetError(f"palette has {colours} colours")
    depth = depth_for(colours)
    mpd = bytearray()
    table = bytearray()
    for k, fr in enumerate(sheet.frames):
        if fr.lead != FRAME_LEAD:
            raise SheetError(f"frame {k}: lead {fr.lead.hex()} — only .mpd file 0 is written")
        table += b"\0\0" + len(mpd).to_bytes(4, "big") + fr.flags
        raw = bytearray()
        for row in fr.pixels:
            if len(row) != fr.w or max(row, default=0) >= colours:
                raise SheetError(f"frame {k}: a row is not {fr.w} indices below {colours}")
            raw += b"\0" + pack_bits(row, depth)
        mpd += chunk(b"IHDR", struct.pack(">IIBBBBB", fr.w, fr.h, depth, 3, 0, 0, 0))
        for t, d in fr.extra:
            mpd += chunk(t, d)
        packed = zlib.compress(bytes(raw), 9) if compress_from is not None and k >= compress_from else stored_idat(bytes(raw))
        mpd += chunk(b"IDAT", packed)
    plte = b"".join(bytes(c) for c in sheet.palette)
    mph = struct.pack(">II", sheet.head, len(sheet.frames)) + bytes(table) + chunk(b"PLTE", plte) + chunk(b"tRNS", sheet.trns)
    return bytes(mph), bytes(mpd)


def pad_sheet(part, sheet):
    """배포본 시트 → 이 파트의 프레임 수까지 1×1 빈 칸을 덧붙인 시트(몸통의 망토 프레임 5..8). 이미 그만큼 있으면 그대로.
    빈 칸은 의상 PNG 의 빈 칸과 같은 프레임(`load_part` 의 "blank")이라 둘이 같은 바이트로 싸인다."""
    want = len(PARTS[part]["sizes"])
    if len(sheet.frames) >= want:
        return sheet
    if len(sheet.frames) != PARTS[part].get("shipped", want):
        raise SheetError(f"{part}: {len(sheet.frames)} 프레임 — 배포본은 {PARTS[part].get('shipped', want)}")
    frames = list(sheet.frames) + [Frame(1, 1, [[0]]) for _ in range(want - len(sheet.frames))]
    return Sheet(sheet.palette, frames, head=sheet.head, trns=sheet.trns)


def sheet_rgba(sheet):
    """프레임마다 (w, h, [[(r,g,b,a)]]) — tRNS 로 알파."""
    alpha = list(sheet.trns) + [255] * (len(sheet.palette) - len(sheet.trns))
    out = []
    for fr in sheet.frames:
        out.append((fr.w, fr.h, [[sheet.palette[i] + (alpha[i],) for i in row] for row in fr.pixels]))
    return out


# ── 편집용 PNG 읽기·쓰기 ─────────────────────────────────────────────────────


def png_write_indexed(path, w, h, palette, pixels, trns=TRNS):
    depth = depth_for(len(palette))
    raw = b"".join(b"\0" + pack_bits(row, depth) for row in pixels)
    data = (
        SIG
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, depth, 3, 0, 0, 0))
        + chunk(b"PLTE", b"".join(bytes(c) for c in palette))
        + chunk(b"tRNS", trns)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(data)


def png_write_rgba(path, w, h, px):
    """px = 행 목록 of (r, g, b, a) — 미리보기용."""
    raw = b"".join(b"\0" + bytes(c for p in row for c in p) for row in px)
    data = SIG + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(data)


class Picture:
    """편집기에서 온 PNG 한 장. kind = 'indexed' (palette/alpha/pixels) 또는 'rgba' (pixels = (r,g,b,a))."""

    def __init__(self, path):
        self.path = path
        buf = open(path, "rb").read()
        if buf[:8] != SIG:
            raise SheetError(f"{path}: not a PNG")
        ch = walk_chunks(buf, 8, where=path)
        if not ch or ch[0][0] != b"IHDR":
            raise SheetError(f"{path}: no IHDR")
        w, h, depth, ctype, comp, filt, lace = struct.unpack(">IIBBBBB", ch[0][1])
        if lace:
            raise SheetError(f"{path}: interlaced PNG — 비인터레이스로 저장할 것")
        if comp or filt:
            raise SheetError(f"{path}: unknown compression/filter method")
        get = {}
        for t, d in ch:
            get.setdefault(t, d)
        idat = b"".join(d for t, d in ch if t == b"IDAT")
        chans = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(ctype)
        if chans is None or (ctype != 3 and depth != 8) or depth not in (1, 2, 4, 8):
            raise SheetError(f"{path}: colour type {ctype} depth {depth} — 인덱스 PNG 또는 8 비트 RGB/RGBA 로 저장할 것")
        rows = unfilter(zlib.decompress(idat), w, h, depth * chans, path)
        self.w, self.h = w, h
        trns = get.get(b"tRNS")
        if ctype == 3:
            plte = get.get(b"PLTE")
            if plte is None:
                raise SheetError(f"{path}: indexed PNG without PLTE")
            self.kind = "indexed"
            self.plte = plte
            self.palette = [tuple(plte[i : i + 3]) for i in range(0, len(plte), 3)]
            # tRNS 가 없으면 인덱스 0 이 투명 — 게임의 약속이자 dump_mpd.py 가 뽑은 템플릿의 모양.
            self.alpha = list(trns) + [255] * (len(self.palette) - len(trns)) if trns is not None else [0] + [255] * (len(self.palette) - 1)
            self.pixels = [unpack_bits(r, w, depth) for r in rows]
            for y, row in enumerate(self.pixels):
                for x, v in enumerate(row):
                    if v >= len(self.palette):
                        raise SheetError(f"{path}: pixel ({x},{y}) index {v} is past the palette")
        else:
            self.kind = "rgba"
            px = []
            key = None
            if ctype == 2 and trns is not None and len(trns) == 6:
                key = (trns[1], trns[3], trns[5])
            for r in rows:
                line = []
                for x in range(w):
                    s = r[x * chans : (x + 1) * chans]
                    if ctype == 0:
                        line.append((s[0], s[0], s[0], 255))
                    elif ctype == 4:
                        line.append((s[0], s[0], s[0], s[1]))
                    elif ctype == 2:
                        rgb = (s[0], s[1], s[2])
                        # 알파 없는 RGB = #ffffff 가 투명 (배포본의 투명색; 그림판처럼 투명을 못 쓰는 편집기용).
                        transparent = rgb == key if key is not None else rgb == TRANSPARENT_RGB
                        line.append(rgb + (0 if transparent else 255,))
                    else:
                        line.append((s[0], s[1], s[2], s[3]))
                px.append(line)
            self.pixels = px

    def rgba(self, x, y):
        if self.kind == "indexed":
            i = self.pixels[y][x]
            return self.palette[i] + (self.alpha[i],)
        return self.pixels[y][x]


# ── 의상 디렉터리 ────────────────────────────────────────────────────────────


def costume_number(path):
    base = os.path.basename(os.path.normpath(path))
    head = base.split("_", 1)[0]
    if not head.isdigit():
        raise SheetError(f"{path}: 디렉터리 이름이 <번호>_<이름> 이 아니다")
    n = int(head)
    if not COSTUME_MIN <= n <= COSTUME_MAX:
        raise SheetError(f"{path}: 번호 {n} — 새 의상은 {COSTUME_MIN}..{COSTUME_MAX} (배포본 번호와 겹치면 안 된다)")
    return n


def load_part(costume_dir, part):
    """(Sheet, notes) — 규격 위반은 SheetError 하나에 전부 모아 던진다."""
    spec = PARTS[part]
    errors, notes, pics = [], [], []
    present = sorted(f for f in os.listdir(costume_dir) if f.startswith(part + "_") and f.endswith(".png"))
    want = [f"{part}_{k}.png" for k in range(len(spec["sizes"]))]
    for f in present:
        if f not in want:
            errors.append(f"{f}: {part} 는 프레임 0..{len(want) - 1} 뿐이다")
    for k, f in enumerate(want):
        path = os.path.join(costume_dir, f)
        if not os.path.exists(path) and k in spec.get("optional", ()):
            pics.append("blank")  # 망토 프레임 없음 = 1×1 빈 칸
            continue
        if not os.path.exists(path):
            errors.append(f"{f}: 없음")
            pics.append(None)
            continue
        try:
            pic = Picture(path)
        except SheetError as e:
            errors.append(str(e))
            pics.append(None)
            continue
        size = (pic.w, pic.h)
        if not part_size_ok(part, k, size):
            shapes = [spec["sizes"][k]] + list(spec.get("alt", {}).get(k, ()))
            allowed = " 또는 ".join(f"{w}x{h}" for w, h in shapes) + (" 또는 1x1(빈 칸)" if k in spec["blank"] else "")
            errors.append(f"{f}: {pic.w}x{pic.h} — 이 프레임은 {allowed} 여야 한다 (위치는 직업 시트가 정한다)")
        pics.append(pic)
    if errors:
        raise SheetError("\n".join(errors))

    # 팔레트: 전 프레임이 같은 PLTE 의 인덱스 PNG 면 그대로(순서까지) — 배포본 재포장이 바이트 동일한 이유이고,
    # 바탕 시트의 색 번호를 지키므로 h.p 색 변형 레코드도 그대로 맞는다. 아니면 쓰인 색으로 새로 짓는다.
    real = [p for p in pics if p != "blank"]
    keep = all(p.kind == "indexed" and p.plte == real[0].plte for p in real) and all(
        p.alpha[0] == 0 and all(a == 255 for a in p.alpha[1:]) for p in real
    )
    blank = Frame(1, 1, [[0]])
    if keep:
        palette = list(real[0].palette)
        frames = [blank if p == "blank" else Frame(p.w, p.h, [list(r) for r in p.pixels]) for p in pics]
    else:
        palette, index = [TRANSPARENT_RGB], {}
        frames = []
        for k, p in enumerate(pics):
            if p == "blank":
                frames.append(blank)
                continue
            rows = []
            for y in range(p.h):
                row = []
                for x in range(p.w):
                    r, g, b, a = p.rgba(x, y)
                    if a == 0:
                        row.append(0)
                        continue
                    if a != 255:
                        errors.append(f"{part}_{k}.png: ({x},{y}) 반투명(alpha {a}) — 게임 시트는 투명/불투명 둘뿐이다")
                        row.append(0)
                        continue
                    c = (r, g, b)
                    if c not in index:
                        index[c] = len(palette)
                        palette.append(c)
                    row.append(index[c])
                rows.append(row)
            frames.append(Frame(p.w, p.h, rows))
        if errors:
            raise SheetError("\n".join(errors[:20]) + (f"\n… 외 {len(errors) - 20} 건" if len(errors) > 20 else ""))
        if len(palette) > 256:
            raise SheetError(f"{part}: 색이 {len(palette) - 1} 개 — 255 개까지")
        if TRANSPARENT_RGB in palette[1:]:
            notes.append(f"{part}: 불투명한 #ffffff 가 있다 — 투명색(인덱스 0)과 같은 색이라 h.p 색 변형이 둘을 같이 바꾼다")
    for k, fr in enumerate(frames):
        if k in spec["blank"] and (fr.w, fr.h) == (1, 1):
            if fr.pixels[0][0] != 0:
                notes.append(f"{part}_{k}.png: 1x1 빈 칸인데 불투명 — 점 하나가 그려진다")
        elif all(v == 0 for row in fr.pixels for v in row):
            notes.append(f"{part}_{k}.png: 전부 투명 — 이 조각이 안 보인다")
    used = {v for fr in frames for row in fr.pixels for v in row}
    off = sorted({palette[i] for i in used if i and any(c not in GRID for c in palette[i])})
    if off:
        notes.append(f"{part}: 배포본 색 격자(00 1f 3f … ff) 밖의 색 {len(off)} 개 — " + " ".join("#%02x%02x%02x" % c for c in off[:8]))
    colours = len(palette)
    notes.append(f"{part}: {len(frames)} 프레임 · 팔레트 {colours} 색({'원본 순서 유지' if keep else '새로 지음'}) · {depth_for(colours)} 비트")
    return Sheet(palette, frames), notes


def costume_parts(costume_dir):
    """의상이 가진 파트 — PARTS 중 파일이 하나라도 있는 것 (사용자 결정 2026-09-30 밤: 장비 전부 — 머리·몸통·팔·다리·
    무기(직업별)·보조손(나이트 방패 · 로그 둘째 단검)). 파일 이름 = `<파트>_<프레임>.png`, 파트 = PARTS 의 키
    (`weapon_knight_0.png` …) — 파트 이름이 다른 파트 이름의 접두가 되지 않게 지었다."""
    names = os.listdir(costume_dir)
    return [p for p in PARTS if any(f.startswith(p + "_") and f[len(p) + 1 : -4].isdigit() and f.endswith(".png") for f in names)]


def packed_members(d, stem, sheet, where):
    """{`hero/<d>/<stem>.mph` · `_0.mpd`: 바이트} — 싼 것을 다시 풀어 같은 픽셀인지(인코더의 종단 검사)."""
    mph, mpd = sheet_encode(sheet)
    back = sheet_decode(mph, mpd, where)
    assert [f.pixels for f in back.frames] == [f.pixels for f in sheet.frames] and back.palette == sheet.palette
    return {f"hero/{d}/{stem}.mph": mph, f"hero/{d}/{stem}_0.mpd": mpd}


def skirt_layers(costume_dir, parts):
    """(아랫층, 윗층) 시트 — 층 치마가 없으면 None. 한쪽만 있어도 된다(없는 쪽 = 빈 층)."""
    have = [p for p in SKIRT_LAYERS if p in parts]
    if not have:
        return None
    return tuple(load_part(costume_dir, p)[0] if p in have else None for p in SKIRT_LAYERS)


def skirt_compose(under, legs, over, where="skirt"):
    """다리 시트 한 장 = 아랫층 → 다리 → 윗층(불투명한 점이 위를 덮는다). 프레임 크기 = 다리의 것; 층의 1×1 빈 칸은
    그 프레임에 그 층이 없다는 뜻. 팔레트는 쓰인 색으로 새로 짓는다(0 = 투명)."""
    palette, index, frames = [TRANSPARENT_RGB], {}, []
    for k, lf in enumerate(legs.frames):
        rows = [[None] * lf.w for _ in range(lf.h)]
        for sheet in (under, legs, over):
            if sheet is None:
                continue
            fr = sheet.frames[k]
            if sheet is not legs and (fr.w, fr.h) == (1, 1) and fr.pixels[0][0] == 0:
                continue
            if (fr.w, fr.h) != (lf.w, lf.h):
                raise SheetError(f"{where} frame {k}: 층 {fr.w}x{fr.h} — 다리는 {lf.w}x{lf.h}")
            for y, row in enumerate(fr.pixels):
                for x, v in enumerate(row):
                    if v:
                        rows[y][x] = sheet.palette[v]
        px = []
        for row in rows:
            line = []
            for c in row:
                if c is not None and c not in index:
                    index[c] = len(palette)
                    palette.append(c)
                line.append(0 if c is None else index[c])
            px.append(line)
        frames.append(Frame(lf.w, lf.h, px))
    if len(palette) > 256:
        raise SheetError(f"{where}: 색이 {len(palette) - 1} 개 — 255 개까지")
    return Sheet(palette, frames)


def costume_members(costume_dir):
    """{jar 멤버 이름: 바이트} — make_hero2_asset.py 가 jar 에 더한다. ★ 2026-10-09 층 치마(`skirtunder_*` · `skirtover_*`)는
    그대로 싸지 않고 그 세트의 다리와 합쳐 `hero/3/<번호 + 1000>`(신발 아바타가 없을 때의 치마) 한 장이 된다 — 다른 신발과의
    조합은 `skirt_combo_members`."""
    import costume_geometry as geometry
    n = costume_number(costume_dir)
    parts = costume_parts(costume_dir)
    if not parts:
        raise SheetError(f"{costume_dir}: 파트 PNG(body_ · arm_ · leg_ · head_ · weapon_<직업>_ · offhand_<직업>_) 가 하나도 없다")
    layered = any(PARTS[p].get("layer") for p in parts)
    if layered and "skirt" in parts:
        raise SheetError(f"{costume_dir}: 치마가 한 장(skirt_*)과 층(skirtunder_* · skirtover_*) 둘 다 있다 — 하나만")
    if layered and "leg" not in parts:
        raise SheetError(f"{costume_dir}: 층 치마(skirtunder_* · skirtover_*)는 그 세트의 다리(leg_*)가 있어야 한다 — 신발 아바타가 없을 때 그 다리 위에 입는다")
    out, notes, sheets = {}, [], {}
    for part in parts:
        sheet, nt = load_part(costume_dir, part)
        if part == "leg":
            sheet = geometry.footwear(sheet, n)
        notes += nt
        sheets[part] = sheet
        if PARTS[part].get("layer"):
            continue
        if part == "skirt":
            base = geometry.footwear(load_part(costume_dir, "leg")[0], n) if "leg" in parts else sheet
            sheet = geometry.garment(base, gown=sheet)
        elif part == "leg":
            sheet = geometry.short_skirt(sheet) if n == 208 else geometry.duplicate(sheet)
        out.update(packed_members(PARTS[part]["dir"], part_stem(part, n), sheet, f"{part} {n}"))
    if layered:
        merged = geometry.garment(sheets["leg"], sheets.get("skirtunder"), sheets.get("skirtover"))
        out.update(packed_members(PARTS["skirt"]["dir"], part_stem("skirt", n), merged, f"skirt {n}"))
        notes.append(f"skirt: 층 치마 + 이 세트의 다리 = hero/3/{part_stem('skirt', n)} (팔레트 {len(merged.palette)} 색)")
    return out, notes


def skirt_combo_members(dirs):
    """{jar 멤버 이름: 바이트} — 층 치마를 가진 드레스마다, 다리(`leg_*`)를 가진 **다른** 의상마다 `hero/3/<드레스 × 1000 +
    신발>` = 아랫층 + 그 신발의 다리 + 윗층. 드레스 자기 세트의 신발은 `hero/3/<번호 + 1000>`(costume_members)가 맡는다."""
    import costume_geometry as geometry
    found = [(costume_number(d), d, costume_parts(d)) for d in sorted(dirs, key=costume_number)]
    shoes = {n: geometry.footwear(load_part(d, "leg")[0], n) for n, d, parts in found if "leg" in parts}
    if 208 in shoes:
        shoes[208] = geometry.short_skirt(shoes[208])
    out = {}
    for n, d, parts in found:
        layers = skirt_layers(d, parts)
        if layers is None:
            continue
        for s, legs in sorted(shoes.items()):
            if s == n:
                continue
            sheet = geometry.garment(legs, layers[0], layers[1])
            out.update(packed_members(PARTS["skirt"]["dir"], str(n * SKIRT_OVER_SHOES + s), sheet, f"skirt {n} over shoes {s}"))
    return out


def list_costumes(root=COSTUMES):
    if not os.path.isdir(root):
        return []
    return sorted(
        os.path.join(root, d) for d in os.listdir(root) if os.path.isdir(os.path.join(root, d)) and d.split("_", 1)[0].isdigit()
    )


# ── 카탈로그 — 세트와 아이템의 이름·설명 (A3 2026-09-30 · ★ A3c 2026-10-01) ─────────────────────
#
# ★ A3c (사용자 2026-10-01: 「투구,갑옷,신발,무기 … 세트를 획득하면 이안의 머리, 이안의 낡은 옷, 이안의 낡은 신발, 이안의
# 낡은 대검 등 획득하지만, 각각 따로따로 아바타 장착」): 의상 폴더 하나 = **세트** 하나 = 아바타 **아이템** 최대 넷 — 장비창
# 아바타 칸마다 하나: 투구(head = 머리 PNG) · 갑옷(armor = 몸통·팔) · 신발(shoes = 다리) · 무기(weapon = 무기·보조손).
# 의상 디렉터리마다 `costume.txt`(UTF-8, `키 = 값`)를 두면 `make_hero2_asset.py` 가 전부 모아 jar 의 `hero/costume.txt`
# 한 멤버로 넣는다 — EUC-KR, 한 줄 = 아이템 하나 `<번호>\t<칸>\t<세트>\t<이름>\t<설명>`(칸 = head · armor · shoes · weapon),
# **무압축**(호스트가 inflate 없이 읽는다: `hero2java::{H2_COSTUME_CATALOG, h2_zip_stored, h2_costume_catalog}`).
# 장비창의 아바타 옷장(A3c)이 그 이름으로 아이템을 그린다 — 게임 글꼴(ftd/kor · ftd/eng5)로.
#
#   set        = 빙해의 검사            세트 이름 (필수)
#   head       = 이안의 머리            투구 칸 아이템의 이름 — 머리 PNG 가 있으면 필수, 없으면 쓰면 안 된다
#   head.desc  = …                      그 설명 (없어도 된다)
#   armor / armor.desc · shoes / shoes.desc · weapon / weapon.desc · offhand / offhand.desc   (같은 규칙)
#
# ★ A4 (2026-10-01, 사용자: 「직업별로 이름을 다르게 해서 얻어야하고 … 대검, 도검, 단검 … 이안의 낡은 단검, 이안의 보조
# 단검」 · 「나이트는 방패까지, 로그는 보조단검까지」): 칸이 다섯 — 보조손(offhand = 나이트 방패 · 로그 둘째 단검)이 무기에서
# 떨어졌다. 그리고 키에 직업을 붙이면 그 직업에게만 보이는 이름이다(`<칸>.<직업>` · `<칸>.<직업>.desc`, 직업 = berserker ·
# knight · rogue) — 없으면 `<칸>` 이 모든 직업의 이름. 아이템은 같은 하나(`(번호, 칸)`)이고 캐릭터의 직업은 평생 같으므로,
# 캐릭터는 제 직업의 이름만 본다. jar 의 줄 = 직업마다 다르면 직업마다 한 줄(여섯째 필드 = 직업 번호 0 · 1 · 2), 모든
# 직업이 같은 이름·설명이면 한 줄(다섯 필드).

CATALOG_MEMBER = "hero/costume.txt"
# 아바타 칸 — 호스트의 `avatar_state::SLOT_KEYS` · `hero2java::H2_AV_SLOT_KEYS` 와 같은 순서·같은 낱말.
SLOT_KEYS = ("head", "armor", "shoes", "weapon", "offhand")
# 칸마다 그 아이템이 그리는 파트(PARTS 의 키) — 호스트의 `SLOT_PARTS`(0 · 1 2 · 3 · 6 · 7).
SLOT_FILE_PARTS = {
    "head": ("head",),
    "armor": ("body", "arm", "skirt") + SKIRT_LAYERS,
    "shoes": ("leg",),
    "weapon": ("weapon_berserker", "weapon_knight", "weapon_rogue"),
    "offhand": ("offhand_knight", "offhand_rogue"),
}
assert tuple(SLOT_FILE_PARTS) == SLOT_KEYS and sorted(p for ps in SLOT_FILE_PARTS.values() for p in ps) == sorted(PARTS)
# 직업 — 게임의 번호(`ag.a:B` 0 버서커 · 1 나이트 · 2 로그)와 키의 낱말.
CLASS_KEYS = ("berserker", "knight", "rogue")
CATALOG_KEYS = (
    ("set",)
    + tuple(k for s in SLOT_KEYS for k in (s, f"{s}.desc"))
    + tuple(k for s in SLOT_KEYS for c in CLASS_KEYS for k in (f"{s}.{c}", f"{s}.{c}.desc"))
)
# 폭 한도(픽셀, 게임 글꼴 `ac.a([B)I` = 1 + 한글 9 · ASCII 6): 아이템 이름과 설명은 장비창 이름칸 한 줄(패널 120 − 여백
# 6)에 **온전히** 보여야 한다(계획서 §3). 옷장 목록 행은 폭에 맞게 자른다.
TEXT_MAX_PX = 114


def game_text_width(text):
    """게임 글꼴로 그린 폭 — 한글 음절 9 · ASCII 6 · 앞 1 (빈 글은 0)."""
    w = sum(9 if ord(ch) > 0x7F else 6 for ch in text)
    return w + 1 if w else 0


def check_game_text(text, where):
    """게임 글꼴이 그리는 글자만인가 — 한글 음절(KS X 1001 의 2,350 자, EUC-KR B0A1..C8FE) · 공백 · ASCII 의 대문자·숫자·
    기호. ⛔ `ftd/eng5` 에 소문자가 없다(95 글리프 중 a..z 26 개가 빈칸, 2026-09-30 전수) — 소문자는 안 그려진다."""
    bad = set()
    for ch in text:
        o = ord(ch)
        if 0x20 <= o < 0x7F:
            if "a" <= ch <= "z":
                bad.add(ch)
            continue
        try:
            b = ch.encode("euc-kr")
        except UnicodeEncodeError:
            bad.add(ch)
            continue
        if not (len(b) == 2 and 0xB0 <= b[0] <= 0xC8 and 0xA1 <= b[1] <= 0xFE):
            bad.add(ch)
    if bad:
        raise SheetError(f"{where}: 게임 글꼴에 없는 글자 {''.join(sorted(bad))!r} — 한글 음절 · 대문자 · 숫자 · 기호만 그려진다")


def costume_slots(costume_dir):
    """이 의상이 그림을 가진 아바타 칸 — SLOT_KEYS 순서."""
    parts = set(costume_parts(costume_dir))
    return [s for s in SLOT_KEYS if parts.intersection(SLOT_FILE_PARTS[s])]


def slot_classes(slot, parts):
    """`slot` 칸의 그림이 있는 직업(0 · 1 · 2)의 집합 — `parts` = 이 의상이 가진 파트. 직업이 정해지지 않은 파트(머리 ·
    몸통 · 팔 · 다리 · 치마)는 모든 직업, 무기·보조손 파트는 그 파트의 직업."""
    out = set()
    for p in SLOT_FILE_PARTS[slot]:
        if p in parts:
            c = PARTS[p].get("class")
            out |= {0, 1, 2} if c is None else {c}
    return out


def costume_info(costume_dir):
    """`costume.txt` → {"set": 세트 이름, "items": [(칸, 직업 또는 None, 이름, 설명)] (SLOT_KEYS 순서, 칸 안은 직업 순)}.
    없거나 규칙 위반이면 SheetError. 규칙(★ A4 — 직업마다): 칸의 그림이 있는 직업은 이름이 **있어야** 하고(`<칸>.<직업>`, 없으면
    `<칸>`), 그림이 없는 직업의 키(`<칸>.<직업>` · `.desc`)는 **없어야** 하며, 그림이 아예 없는 칸은 이름을 쓰면 안 된다 —
    그림 없는 아이템은 옷장에 「착용불가」로만 보이고, 이름 없는 그림은 옷장에 「아바타 <번호>」로 보인다. 세 직업 모두 그림이
    있고 이름·설명이 같으면 한 줄(직업 None), 아니면 직업마다 한 줄."""
    path = os.path.join(costume_dir, "costume.txt")
    if not os.path.exists(path):
        raise SheetError(f"{path}: 없음 — 세트 이름(set)과 칸마다 아이템 이름(head · armor · shoes · weapon · offhand)을 적는 파일 (costumes/README.md)")
    info = {}
    with open(path, encoding="utf-8-sig") as f:
        lines = f.read().splitlines()
    for ln, line in enumerate(lines, 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise SheetError(f"{path}:{ln}: `키 = 값` 이 아니다")
        k, v = (t.strip() for t in line.split("=", 1))
        if k in ("name", "bag", "desc"):
            raise SheetError(f"{path}:{ln}: `{k}` 는 A3 의 형식이다 — A3c(2026-10-01)부터 세트는 `set`, 아이템은 칸마다 `head` · `armor` · `shoes` · `weapon` · `offhand` (+ `.desc`, 직업별 `<칸>.<berserker|knight|rogue>`)")
        if k not in CATALOG_KEYS:
            raise SheetError(f"{path}:{ln}: 모르는 키 {k!r} — {' · '.join(CATALOG_KEYS)}")
        if k in info:
            raise SheetError(f"{path}:{ln}: {k} 가 두 번")
        info[k] = v
    if not info.get("set"):
        raise SheetError(f"{path}: set 이 없다")
    parts = set(costume_parts(costume_dir))
    items = []
    for s in SLOT_KEYS:
        have = slot_classes(s, parts)
        generic, generic_desc = info.get(s, ""), info.get(f"{s}.desc", "")
        keys = (s, f"{s}.desc") + tuple(f"{s}.{c}{x}" for c in CLASS_KEYS for x in ("", ".desc"))
        if not have:
            used = [k for k in keys if info.get(k)]
            if used:
                raise SheetError(f"{path}: {s} 칸의 이름이 있는데 그림({' · '.join(p + '_*.png' for p in SLOT_FILE_PARTS[s])})이 없다 ({' · '.join(used)})")
            continue
        per = {}
        for c, ck in enumerate(CLASS_KEYS):
            if c not in have:
                stray = [k for k in (f"{s}.{ck}", f"{s}.{ck}.desc") if info.get(k)]
                if stray:
                    raise SheetError(f"{path}: {' · '.join(stray)} — {ck} 의 {s} 그림({' · '.join(p + '_*.png' for p in SLOT_FILE_PARTS[s] if PARTS[p].get('class') == c)})이 없다")
                continue
            name = info.get(f"{s}.{ck}", "") or generic
            desc = info.get(f"{s}.{ck}.desc", "") or generic_desc
            if not name:
                raise SheetError(f"{path}: {s} 칸의 {ck} 그림이 있는데 이름({s}.{ck} = … 또는 {s} = …)이 없다")
            per[c] = (name, desc)
        if len(per) == 3 and len(set(per.values())) == 1:
            items.append((s, None) + per[0])
        else:
            items += [(s, c) + per[c] for c in sorted(per)]
    for k, v in info.items():
        check_game_text(v, f"{path} {k}")
        w = game_text_width(v)
        if w > TEXT_MAX_PX:
            raise SheetError(f"{path} {k}: {w} px — 장비창 이름칸에 온전히 들어가려면 {TEXT_MAX_PX} px 까지 (한글 9 · 영문/숫자/공백 6)")
    return {"set": info["set"], "items": items}


def catalog_bytes(dirs):
    """`hero/costume.txt` 의 바이트 — 의상 디렉터리 번호 순, 그 안은 칸 순. 의상이 없으면 None.
    ⛔ 폴더 하나 = 세트 하나 = 번호 하나: 두 폴더가 같은 번호면 거절한다(파트가 달라 jar 멤버는 안 겹쳐도 카탈로그에
    같은 `(번호, 칸)` 줄이 둘 생기거나 두 세트가 소리 없이 한 세트로 섞인다 — 호스트는 먼저 나온 줄만 읽는다)."""
    if not dirs:
        return None
    by_number = {}
    for d in dirs:
        by_number.setdefault(costume_number(d), []).append(d)
    dup = {n: ds for n, ds in by_number.items() if len(ds) > 1}
    if dup:
        raise SheetError("같은 번호의 의상 폴더: " + " · ".join(f"{n} = {', '.join(sorted(os.path.basename(x) for x in ds))}" for n, ds in sorted(dup.items())) + " — 폴더 하나 = 세트 하나 = 번호 하나")
    lines = ["# hero2 costume catalog - number, slot, set, item name, description[, class 0|1|2] (tools_re/costume_sheets.py)".encode("ascii")]
    for d in sorted(dirs, key=costume_number):
        info = costume_info(d)
        for slot, cls, name, desc in info["items"]:
            fields = [str(costume_number(d)), slot, info["set"], name, desc]
            if cls is not None:
                fields.append(str(cls))
            lines.append("\t".join(fields).encode("euc-kr"))
    return b"\n".join(lines) + b"\n"


# ── 아카이브 ─────────────────────────────────────────────────────────────────


def open_jar(zip_path):
    z = zipfile.ZipFile(zip_path)
    jars = [n for n in z.namelist() if n.endswith(".jar") and "/" not in n]
    if not jars:
        raise SheetError(f"{zip_path}: jar 가 없다")
    return zipfile.ZipFile(io.BytesIO(z.read(jars[0])))


def shipped_sheet(jar, d, n):
    """배포본 시트 — n 은 번호(정수, 0 을 채움) 또는 jar 의 이름 그대로(문자열: 무기 `205` · 방패 `1003`)."""
    base = f"hero/{d}/{n:02d}" if isinstance(n, int) else f"hero/{d}/{n}"
    sheet = sheet_decode(jar.read(base + ".mph"), jar.read(base + "_0.mpd"), base)
    if d == 3 and int(n) < COSTUME_MIN and len(sheet.frames) == 55:
        sheet.frames = sheet.frames[:11]
    return sheet


def shipped_stems(jar, part):
    """이 파트의 크기표를 쓰는 배포본 시트 이름들 — 무기·보조손은 `types` 의 종류만."""
    spec = PARTS[part]
    d = spec["dir"]
    stems = sorted(n[len(f"hero/{d}/") : -4] for n in jar.namelist() if n.startswith(f"hero/{d}/") and n.endswith(".mph"))
    if "types" not in spec:
        return stems
    return [s for s in stems if any(
        s.startswith(str(t)) and s[len(str(t)):].isdigit()
        and (len(s[len(str(t)):]) == 2 or (len(s[len(str(t)):]) == 3 and int(s[len(str(t)):]) >= COSTUME_MIN))
        for t in spec["types"])]


# ── verify ───────────────────────────────────────────────────────────────────


def cmd_verify(zip_path):
    """배포본 전 시트: 반전 시트는 픽셀에서 다시 싸서, 나머지는 청크를 다시 이어서 바이트 동일인지.
    그리고 의상 파트(hero/1·2·3)는 편집용 PNG 로 풀었다가 pack 으로 다시 싸서 바이트 동일인지."""
    import tempfile

    outer = zipfile.ZipFile(zip_path)
    srcs = [outer] + [zipfile.ZipFile(io.BytesIO(outer.read(n))) for n in outer.namelist() if n.endswith(".jar")]
    flip = reframed = frames = 0
    for src in srcs:
        names = set(src.namelist())
        for name in sorted(names):
            if not name.endswith(".mph"):
                continue
            mph = src.read(name)
            mpd_name = name[:-4] + "_0.mpd"
            if mpd_name not in names:
                continue
            mpd = src.read(mpd_name)
            head = struct.unpack(">I", mph[:4])[0]
            if head == HEAD_FLIP:
                sheet = sheet_decode(mph, mpd, name)
                got = sheet_encode(sheet)
                if got != (mph, mpd):
                    raise SheetError(f"{name}: re-encode differs (mph {got[0] == mph}, mpd {got[1] == mpd})")
                flip += 1
                frames += len(sheet.frames)
            else:
                # 압축 시트는 픽셀에서 같은 압축을 재현할 수 없다 — 컨테이너만: 청크를 다시 이어 붙여 같은가.
                n = struct.unpack(">I", mph[4:8])[0]
                offs = [int.from_bytes(mph[8 + 8 * k + 4 : 8 + 8 * k + 6], "big") for k in range(n)] + [len(mpd)]
                again = b"".join(chunk(t, d) for k in range(n) for t, d in walk_chunks(mpd, offs[k], offs[k + 1], where=name))
                tail = b"".join(chunk(t, d) for t, d in walk_chunks(mph, 8 + 8 * n, where=name))
                if again != mpd or mph[: 8 + 8 * n] + tail != mph:
                    raise SheetError(f"{name}: re-frame differs")
                reframed += 1
                frames += n
    print(f"verify: {flip} flip sheet(s) re-encoded from pixels + {reframed} other sheet(s) re-framed = {frames} frame(s), all byte-identical")

    jar = open_jar(zip_path)
    parts = 0
    for part, spec in PARTS.items():
        if "offset" in spec or "layer" in spec:
            continue  # 치마 = 다리 시트를 다른 번호로 — 배포본에는 없고 다리가 이미 잰다
        d = spec["dir"]
        for stem in shipped_stems(jar, part):
            base = f"hero/{d}/{stem}"
            sheet = sheet_decode(jar.read(base + '.mph'), jar.read(base + '_0.mpd'), base)
            shipped_n = spec.get("shipped", len(spec["sizes"]))
            for k, fr in enumerate(sheet.frames):
                counts = {shipped_n, len(spec["sizes"])}
                if part == "leg":
                    counts.add(55)
                source_index = k % 11 if part == "leg" else k
                ok = len(sheet.frames) in counts and part_size_ok(part, source_index, (fr.w, fr.h), fr.pixels)
                if not ok or fr.flags != FRAME_FLAGS or fr.extra:
                    raise SheetError(f"{base} frame {k}: {fr.w}x{fr.h} {fr.flags.hex()} breaks the {part} table")
            if sheet.palette[0] != TRANSPARENT_RGB or sheet.trns != TRNS:
                raise SheetError(f"{base}: index 0 is not the #ffffff transparent")
            with tempfile.TemporaryDirectory() as tmp:
                cdir = os.path.join(tmp, f"{COSTUME_MIN}_{part}_{stem}")
                os.makedirs(cdir)
                for k, fr in enumerate(sheet.frames[:len(spec["sizes"])]):
                    png_write_indexed(os.path.join(cdir, f"{part}_{k}.png"), fr.w, fr.h, sheet.palette, fr.pixels)
                source, _notes = load_part(cdir, part)
                if part == "leg" and len(sheet.frames) == 55:
                    for bank in range(1, 5):
                        for k, fr in enumerate(sheet.frames[bank * 11:(bank + 1) * 11]):
                            png_write_indexed(os.path.join(cdir, f"{part}_{k}.png"), fr.w, fr.h, sheet.palette, fr.pixels)
                        extra, _ = load_part(cdir, part)
                        source.frames.extend(extra.frames)
                members = packed_members(d, part_stem(part, COSTUME_MIN), source, base)
            got = f"hero/{d}/{part_stem(part, COSTUME_MIN)}"
            # ★ 몸통: 배포본 5 프레임 + 망토 빈 칸 넷 = make_hero2_asset.py 가 jar 에 넣는 그 시트(`pad_sheet`).
            want = sheet_encode(pad_sheet(part, sheet)) if shipped_n != len(spec["sizes"]) else (jar.read(base + ".mph"), jar.read(base + "_0.mpd"))
            if (members[got + ".mph"], members[got + "_0.mpd"]) != want:
                raise SheetError(f"{base}: unpack → pack round trip differs")
            parts += 1
    print(f"verify: {parts} costume-part sheet(s) (hero/0·1·2·3·6·7) — size table holds, PNG → pack round trip byte-identical")
    return 0


# ── unpack / check / pack ────────────────────────────────────────────────────


def cmd_unpack(zip_path, out_dir, specs):
    jar = open_jar(zip_path)
    os.makedirs(out_dir, exist_ok=True)
    for spec in specs:
        part, _, num = spec.partition(":")
        if part not in PARTS or not num.isdigit():
            raise SheetError(f"{spec}: <파트>:<번호> — body:102 · head:05 · weapon_knight:205 · offhand_knight:1003 (무기·보조손은 jar 이름 그대로)")
        key = num if "type" in PARTS[part] else int(num)
        sheet = shipped_sheet(jar, PARTS[part]["dir"], key)
        for k, fr in enumerate(sheet.frames):
            png_write_indexed(os.path.join(out_dir, f"{part}_{k}.png"), fr.w, fr.h, sheet.palette, fr.pixels)
        print(f"  {part} ← hero/{PARTS[part]['dir']}/{key if isinstance(key, str) else f'{key:02d}'}: {len(sheet.frames)} PNG, 팔레트 {len(sheet.palette)} 색")
    print(f"unpacked into {out_dir}")
    return 0


def cmd_check(costume_dir):
    members, notes = costume_members(costume_dir)
    info = costume_info(costume_dir)
    for line in notes:
        print("  " + line)
    for name, data in sorted(members.items()):
        print(f"  {name}  {len(data)} B")
    print(f"  catalog: 세트 [{info['set']}] — 줄 {len(info['items'])}")
    for slot, cls, name, desc in info["items"]:
        who = "모든 직업" if cls is None else CLASS_KEYS[cls]
        print(f"    {slot:7} {who:9} {name} — {desc or '(설명 없음)'}")
    print(f"{costume_dir}: OK — {len(members) // 2} sheet(s), 번호 {costume_number(costume_dir)}")
    return 0


def cmd_pack(costume_dir, out_dir):
    members, notes = costume_members(costume_dir)
    for line in notes:
        print("  " + line)
    for name, data in sorted(members.items()):
        path = os.path.join(out_dir, *name.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)
        print(f"  wrote {path} ({len(data)} B)")
    return 0


# ── preview — 게임의 합성을 그대로 (머리말 「그리는 쪽」) ───────────────────────

CLASS_NAMES = ["berserker", "knight", "rogue"]
# 미리보기의 머리·무기·보조손: 맨머리 + 직업의 대표 무기(t/itmtp.tdf: 0 양손대검 · 2 도검 · 3 단검) + 방패 / 둘째 단검.
PREVIEW_GEAR = {
    0: {0: "hero/0/100", 6: "hero/6/000"},
    1: {0: "hero/0/100", 6: "hero/6/200", 7: "hero/7/1000"},
    2: {0: "hero/0/100", 6: "hero/6/300", 7: "hero/7/300"},
}
DIR_NAMES = ["back", "front", "left", "right"]  # g.u 1..4
BG = (0x1B, 0x1D, 0x23, 255)


def load_look(jar, spec, zip_path):
    """{(part dir, 직업 또는 None): rgba frames} — 의상 디렉터리(싼 결과를 다시 풀어서 = 게임이 받을 그대로) 또는
    body:n,arm:n,leg:n,head:n,weapon_knight:205,…(배포본). 직업이 붙은 파트(무기·보조손)는 그 직업의 행에만 쓰인다."""
    out = {}
    if os.path.isdir(spec):
        members, _ = costume_members(spec)
        n = costume_number(spec)
        for part in costume_parts(spec):
            # 층 치마 = 그 세트의 다리와 합친 한 장(`hero/3/<번호 + 1000>`) — 드레스를 입은 모습이다.
            shown = "skirt" if PARTS[part].get("layer") else part
            d, stem = PARTS[shown]["dir"], part_stem(shown, n)
            out[(d, PARTS[shown].get("class"))] = sheet_rgba(sheet_decode(members[f"hero/{d}/{stem}.mph"], members[f"hero/{d}/{stem}_0.mpd"], spec))
        label = os.path.basename(os.path.normpath(spec))
    else:
        for item in spec.split(","):
            part, _, num = item.partition(":")
            if part not in PARTS or not num.isdigit():
                raise SheetError(f"{spec}: 의상 디렉터리 또는 body:<n>,arm:<n>,leg:<n>,head:<n>,weapon_<직업>:<jar 이름>")
            d = PARTS[part]["dir"]
            key = num if "type" in PARTS[part] else int(num)
            sheet = shipped_sheet(jar, d, key)
            if d == 3:
                import costume_geometry as geometry
                sheet = geometry.duplicate(sheet)
            out[(d, PARTS[part].get("class"))] = sheet_rgba(sheet)
        label = spec
    return label, out


def sheet_by_path(jar, base):
    import costume_geometry as geometry
    sheet = sheet_decode(jar.read(base + ".mph"), jar.read(base + "_0.mpd"), base)
    if base.startswith("hero/3/"):
        sheet = geometry.duplicate(sheet)
    return sheet_rgba(sheet)


# --tint: 파트마다 한 색(명암은 원래 밝기) — 어느 픽셀이 어느 파트의 몇 번 프레임인지 보는 해부도.
TINT = {0: (0xC8, 0xC8, 0xC8), 1: (0xE0, 0x40, 0x40), 2: (0x40, 0xD0, 0x60), 3: (0x40, 0x80, 0xF0), 6: (0x70, 0x70, 0x70), 7: (0x90, 0x70, 0x40)}


def compose(sprites, table, skip_parts, tint=False, flip_lists=None):
    """[(x, y, w, h, px)] 를 그릴 순서대로 — 좌표는 캐릭터 원점 기준. ★ 2026-10-10 `flip_lists`(애니메이션 머리의 파트별
    첫 목록)를 주면 게임처럼 그 목록에 없는 프레임의 반전 조각은 그리지 않는다 — 파츠 로더가 반전 사본을 그 목록의 프레임에만
    만들고 그리는 쪽은 null 그림을 건너뛴다(vC 98 의 「이동 중 잠깐 투명」을 미리보기가 못 본 이유)."""
    out = []
    for p_byte, f, dx, dy in sprites:
        p = p_byte & 0x7F
        if p in skip_parts or p not in table or f >= len(table[p]):
            continue
        if flip_lists is not None and p_byte & 0x80 and (p >= len(flip_lists) or f not in flip_lists[p]):
            continue
        w, h, px = table[p][f]
        if p_byte & 0x80:
            px = [row[::-1] for row in px]
        if tint and p in TINT:
            t = TINT[p]
            px = [[(t[0] * (r + g + b) // 765, t[1] * (r + g + b) // 765, t[2] * (r + g + b) // 765, a) for r, g, b, a in row] for row in px]
        out.append((dx - 256 if dx > 127 else dx, dy - 256 if dy > 127 else dy, w, h, px))
    return out


def cmd_preview(zip_path, out_png, looks, seq_list, scale, with_fx, tint, frame_list=None, class_list=(0, 1, 2)):
    """행 = (직업, 시퀀스, 방향), 열 묶음 = 모습(왼쪽부터 인수 순서), 묶음 안 = 그 시퀀스의 프레임."""
    import costume_geometry as geometry
    jar = open_jar(zip_path)
    looks = [load_look(jar, s, zip_path) for s in looks]
    skip = {5} if with_fx else {4, 5}
    rows = []  # (class, [cells per look]) — cells = composed sprite lists
    for c in class_list:
        _h, anim_parts, seqs = venom_effect.decode(geometry.patch_animation(jar.read(f"hero/hero{c}")))
        flip_lists = [set(first) for first, _second in anim_parts]
        base = {p: sheet_by_path(jar, b) for p, b in PREVIEW_GEAR[c].items()}
        if with_fx:
            base[4] = sheet_by_path(jar, f"hero/4/{c}")
        tables = []
        for _label, look in looks:
            table = dict(base)
            table.update({d: fr for (d, cl), fr in look.items() if cl is None or cl == c})
            tables.append(table)
        for s in seq_list if seq_list is not None else range(len(seqs)):
            if s >= len(seqs):
                continue
            for d in range(4):
                frames = [fr for i, fr in enumerate(seqs[s]) if frame_list is None or i in frame_list]
                rows.append((c, [[compose(fr[d], t, skip, tint, flip_lists) for fr in frames] for t in tables]))
    # 한 칸 = 전 칸 합집합 상자 (원점이 칸마다 같아야 프레임·모습을 넘기며 비교할 수 있다).
    x0 = y0 = 10**9
    x1 = y1 = -(10**9)
    for _c, groups in rows:
        for cells in groups:
            for cell in cells:
                for x, y, w, h, _px in cell:
                    x0, y0, x1, y1 = min(x0, x), min(y0, y), max(x1, x + w), max(y1, y + h)
    cw, chh = x1 - x0 + 2, y1 - y0 + 2
    per_look = max(len(cells) for _c, groups in rows for cells in groups)
    pad, gap, band_h = 2, 6, 3
    W = (len(looks) * (per_look * (cw + pad) + gap) + pad) * scale
    H = (len(rows) * (chh + pad) + len(class_list) * (band_h + pad) + pad) * scale
    img = [[BG] * W for _ in range(H)]
    oy, last = pad, None
    for c, groups in rows:
        if c != last:
            # 직업이 바뀌는 곳 = 얇은 띠 (버서커 · 나이트 · 로그 순).
            band = [(0x3A, 0x3F, 0x4C, 255), (0x4C, 0x3A, 0x3F, 255), (0x3A, 0x4C, 0x3F, 255)][c]
            for yy in range(oy * scale, (oy + band_h) * scale):
                img[yy] = [band] * W
            oy += band_h + pad
            last = c
        for gi, cells in enumerate(groups):
            gx = pad + gi * (per_look * (cw + pad) + gap)
            for ci, cell in enumerate(cells):
                cx = gx + ci * (cw + pad)
                for x, y, w, h, px in cell:
                    for yy in range(h):
                        for xx in range(w):
                            p = px[yy][xx]
                            if p[3] == 0:
                                continue
                            X = (cx + 1 + x - x0 + xx) * scale
                            Y = (oy + 1 + y - y0 + yy) * scale
                            for sy in range(scale):
                                img[Y + sy][X : X + scale] = [p] * scale
        oy += chh + pad
    png_write_rgba(out_png, W, H, img)
    seqs_txt = "all" if seq_list is None else ",".join(map(str, seq_list))
    print(f"preview: {out_png} {W}x{H} — rows = class × seq [{seqs_txt}] × dir (back front left right), columns = " + " | ".join(l for l, _ in looks))
    return 0


# ── main ─────────────────────────────────────────────────────────────────────


def main():
    sys.stdout.reconfigure(errors="replace")
    args = sys.argv[1:]
    zip_path = DEFAULT_ZIP
    if "--zip" in args:
        k = args.index("--zip")
        zip_path = args[k + 1]
        del args[k : k + 2]
    flags = {a for a in args if a in ("--all", "--fx", "--tint")}
    scale = 4
    if "--scale" in args:
        k = args.index("--scale")
        scale = int(args[k + 1])
        del args[k : k + 2]
    seq_list = [0, 2]  # 무기 든 대기 · 걷기 — 옷이 가장 잘 보이는 둘
    if "--seqs" in args:
        k = args.index("--seqs")
        seq_list = [int(s) for s in args[k + 1].split(",")]
        del args[k : k + 2]
    if "--all" in flags:
        seq_list = None
    class_list = (0, 1, 2)
    if "--classes" in args:
        k = args.index("--classes")
        class_list = tuple(int(s) for s in args[k + 1].split(","))
        del args[k : k + 2]
    frame_list = None
    if "--frames" in args:
        k = args.index("--frames")
        frame_list = [int(s) for s in args[k + 1].split(",")]
        del args[k : k + 2]
    args = [a for a in args if a not in flags]
    if not args:
        print(__doc__)
        return 2
    cmd, rest = args[0], args[1:]
    try:
        if cmd == "verify" and not rest:
            return cmd_verify(zip_path)
        if cmd == "unpack" and len(rest) >= 2:
            return cmd_unpack(zip_path, rest[0], rest[1:])
        if cmd == "check" and len(rest) == 1:
            return cmd_check(rest[0])
        if cmd == "pack" and len(rest) in (1, 2):
            return cmd_pack(rest[0], rest[1] if len(rest) == 2 else os.path.join(rest[0], "out"))
        if cmd == "preview" and len(rest) >= 2:
            return cmd_preview(zip_path, rest[0], rest[1:], seq_list, scale, "--fx" in flags, "--tint" in flags, frame_list, class_list)
    except SheetError as e:
        print(f"costume_sheets: {e}")
        return 1
    print(__doc__)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
