# KS X 1001 의 한글 음절 2,350 자를 유니코드 순서의 비트맵으로 — `wie_android/src/euc_kr.rs` 의
# `KSX1001_SYLLABLE_BITS` 를 다시 쓴다 (2026-10-20, 호스트 안내 문구를 게임 글꼴의 EUC-KR 로).
#
# 근거: KS X 1001 의 음절 배열은 사전순 = 유니코드 순이므로 「집합 안에서의 순위」가 곧 셀이다
# (`0xB0 + 순위 // 94`, `0xA1 + 순위 % 94`). 이 스크립트가 파이썬의 cp949 코덱으로 그 전제를 전수 검사한다
# (확장 완성형 8,822 자는 선두 바이트가 0xB0..0xC8 밖이라 걸러진다).
#
# 사용법: python tools_re/euc_kr_table.py            # euc_kr.rs 를 제자리에서 갱신
import re
import sys

SRC = "wie_android/src/euc_kr.rs"


def in_ksx1001(ch: str) -> bool:
    b = ch.encode("cp949")
    return 0xB0 <= b[0] <= 0xC8 and 0xA1 <= b[1] <= 0xFE


def main() -> int:
    bits = bytearray((11172 + 7) // 8)
    rank = 0
    for i in range(11172):
        ch = chr(0xAC00 + i)
        if not in_ksx1001(ch):
            continue
        bits[i >> 3] |= 1 << (i & 7)
        b = ch.encode("cp949")
        if (b[0], b[1]) != (0xB0 + rank // 94, 0xA1 + rank % 94):
            print(f"order broken at U+{0xAC00 + i:04X}: {b.hex()} != rank {rank}", file=sys.stderr)
            return 1
        rank += 1
    if rank != 2350:
        print(f"expected 2350 syllables, got {rank}", file=sys.stderr)
        return 1
    rows = []
    for i in range(0, len(bits), 24):
        rows.append("    " + ", ".join(f"0x{b:02x}" for b in bits[i : i + 24]) + ",")
    table = f"const KSX1001_SYLLABLE_BITS: [u8; {len(bits)}] = [\n" + "\n".join(rows) + "\n];"
    with open(SRC, "rb") as f:
        src = f.read().decode("utf-8")
    new, n = re.subn(r"const KSX1001_SYLLABLE_BITS: \[u8; \d+\] = \[\n.*?\n\];", lambda _m: table, src, count=1, flags=re.S)
    if n != 1:
        print("the table block was not found", file=sys.stderr)
        return 1
    if new != src:
        with open(SRC, "wb") as f:
            f.write(new.encode("utf-8"))
        print(f"{SRC}: table rewritten ({rank} syllables, {len(bits)} bytes)")
    else:
        print(f"{SRC}: table unchanged ({rank} syllables)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
