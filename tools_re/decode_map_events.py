# hero2 (KTF 1.0.7) — 맵 이벤트 구간 디코더: 트리거 레코드 · 대사 · 스크립트.
#
# ── 왜 이 도구인가 (2026-09-26, 보스 문 재확인) ─────────────────────────────────
# 「잡은 보스를 또 잡을 수 있다」 를 처음엔 문 레코드만 보고 원작 데이터 누락이라 적었다. 사용자가
# 「원본에서도 재입장이 됐다고?」 로 되물었고, 퀘스트를 **주고 받는 쪽**까지 전 맵에서 찾자 답이 바뀌었다:
# 맵 86 의 문은 퀘스트 0x20 이 「진행(1)」일 때만 열리고, 그 퀘스트는 노르덴항 여관(68)의 클레멘스에게
# 보고해야 「완료(2)」가 된다 — 원본은 **보고 순간** 닫히고, 처치~보고 사이에만 다시 열린다.
# 문 하나가 아니라 그 퀘스트를 읽고 쓰는 곳 전부를 봐야 한다. 이 도구가 그것을 한 줄로 한다.
#
# 형식 (`bf.a([BI)I` @0x12af98 가 맵 로드 때 읽는다; 정본 = HERO2_GUEST_RE.md §8.22):
#   A  [u8 nA] { [u8 m] [m × 31 B 레코드] } × nA      트리거 그룹 — 타일 속성(하위 6비트 = 그룹+1)이 그룹을 고른다
#   B  [u8 nB] { [u8 len] [cp949] } × nB              대사
#   C  [u8 nC] { [u8 m] [m × 3 B 명령] } × nC          스크립트
# 레코드: [0] 직업 [1] 최소 레벨 [2] 종류 · [3..8] (퀘스트, 상태) × 3 · [9..14] (이벤트 플래그, 1) × 3 ·
#         [15..29] 아이템 조건 등 · [30] 스크립트 번호(255 = 없음). 그룹 안에서 **앞에서부터 첫 일치**가 돈다.
#   (0, 1) 쌍은 「조건 없음」. 퀘스트 상태 = `ag.r` 의 2비트(`ag.a(I)B`), 이벤트 = `ag.q` 의 비트(`ag.a(I)Z`, LSB 부터).
# 종류 = `bb.a(BB)Z`(종류, 그룹+1) 를 부르는 쪽: 3 = 밟기(`bb.b()Z`, 선 타일) · 2 = 말 걸기(`bb.a()Z`, 앞 타일) ·
#   0 = 자동 — 몬스터 처치(`bd.q()V` 가 죽은 몬스터 정의 `cq.l`(+0x5c) > 0 이면 그 그룹 = 보스방의 처치 대사)
#   또는 캐릭터가 선 타일(`bb.a(La;)Z`, `a.i(B)V`·`n.a(II)V` 가 부른다 = 도착 컷신).
# 명령(아는 것만 이름을 붙인다): 0f n 대사 · 10 n k 질문(아니오면 k 개 건너뜀) ·
#   11 q y 퀘스트 — y 0 → 진행(1, `ag.d`) · 1 → 완료(2, `ag.e`) · 2 → 실패 ·
#   12 f y 이벤트 — y 0 → 켬(`ag.b`) · 1 → 끔 · 39 맵 · 3a x y 워프 · 2d 끝.
#
# ⚠ 구간 A 의 시작은 로더를 끝까지 풀지 않고 **검증된 파싱**으로 찾는다: 그리드 뒤의 모든 오프셋에서
#   A→B→C 를 읽어 레코드 값 범위·대사 디코딩·명령 코드·스크립트 번호가 전부 맞는 가장 앞 후보(nA > 0).
#   로더로 확인된 두 맵(86 = 1873, 91 = 3270)과 일치한다. 더미 맵(21·22·23·32·45·51·57·61·63)은 구간이 없다.
#   C 뒤 = D 컷신 표(명령 13 n 이 부르는 것) + E **로드 표**(★ 2026-10-15, 102 맵 전부 파일 끝까지 정확히 맞는다):
#   D  [u8 n] { [u8 m] [m × 3 B 명령] } × n
#   E  [u8 k] { [u8 목록] [u8 이벤트] × 3 } × k   [u8 L] { [u8 j] [j × 4 B 명령] } × L
#   E 는 **맵을 불러올 때 한 번만** 돈다(`bf.a([BI)I` ← `bf.a([B)V` 하나): 앞에서부터 이벤트 셋(0 = 조건 없음)이
#   전부 켜진 첫 항목의 목록을 `bf.b(BBBB)V`(종류 · 인자 셋)로 실행 — 종류 0b n = NPC n 숨김(`ab.e ← 0`, 서 있던 자리도
#   비운다) · 09 n x y = NPC n 보임 + 그 자리 · 0d n x = NPC n 머리 위 퀘스트 표시 x(8 「!」 · 9 「?」 — 점프표
#   @0x170088 의 13 → `bf.b(BB)V`; f 객체 애니메이션은 04) · 0e = 목록 끝. NPC 번호 = 맵 NPC 구간의 배치 순서.
#   ⛔ 그래서 같은 맵에 서 있는 동안 스크립트가 켠 이벤트는 다음 로드까지 NPC 를 바꾸지 않는다 — 맵 18 의 도적
#   시체는 조사(0x11) 뒤에도 맵을 떠날 때까지 길을 막는다(HERO2_GUEST_RE.md §8.64 끝).
#
# 사용법:
#   python tools_re/decode_map_events.py <원본.zip|hero2_ktf.zip> 86 102 68       # 그 맵들을 풀어서
#   python tools_re/decode_map_events.py <zip> --quest 0x20 --event 0x42           # 전 맵에서 읽는 곳 · 쓰는 곳
import argparse
import gzip
import sys
import zipfile


def load_maps(zpath):
    z = zipfile.ZipFile(zpath)
    out = {}
    for n in z.namelist():
        if n.startswith("P/m/m/") and n.endswith(".m.gz"):
            out[int(n.rsplit("/", 1)[1].split(".")[0])] = gzip.decompress(z.read(n))
    return out


def header(d):
    """(name, W, H, end of the two W×H grids)."""
    p = 2 + d[1]
    name = d[2:p].decode("cp949", "replace")
    p += 1 + d[p]
    p += 1 + d[p]
    w, h = d[p], d[p + 1]
    return name, w, h, p + 2 + 2 * w * h


def parse_events(d, p, strict=True):
    """Sections A, B, C at `p`, or None if any value is out of the format's range.
    `strict` also demands every record's script number to exist — map 72 (신드라프) is the one map
    that fails it (two records name scripts 11 and 12 of an 11-script table), hence the second pass."""
    try:
        n_a = d[p]
        q = p + 1
        groups = []
        for _ in range(n_a):
            m = d[q]
            q += 1
            if not 1 <= m <= 40:
                return None
            recs = []
            for _ in range(m):
                r = bytes(d[q : q + 31])
                if len(r) < 31 or r[2] > 12:
                    return None
                if any(r[4 + 2 * i] > 3 or r[10 + 2 * i] > 1 for i in range(3)):
                    return None
                recs.append(r)
                q += 31
            groups.append(recs)
        strs = []
        for _ in range(d[q]):
            q += 1
            n = d[q]
            s = bytes(d[q + 1 : q + 1 + n])
            if n == 0 or len(s) < n:
                return None
            strs.append(s.decode("cp949"))
            q += n
        q += 1
        scripts = []
        for _ in range(d[q]):
            q += 1
            m = d[q]
            if m == 0:
                return None
            cmds = []
            for k in range(m):
                c = tuple(d[q + 1 + 3 * k : q + 4 + 3 * k])
                if len(c) < 3 or c[0] > 0x3F:
                    return None
                cmds.append(c)
            scripts.append(cmds)
            q += 3 * m
        q += 1
        if strict and any(r[30] != 0xFF and r[30] >= len(scripts) for g in groups for r in g):
            return None
        if not groups:
            return None
        return {"at": p, "end": q, "groups": groups, "strs": strs, "scripts": scripts}
    except (IndexError, UnicodeDecodeError):
        return None


def parse_load_table(d, q):
    """Sections D and E after C at `q` → (cutscenes, entries, lists), or None unless they end exactly at EOF."""
    try:
        cuts = []
        for _ in range(d[q]):
            q += 1
            m = d[q]
            cuts.append([tuple(d[q + 1 + 3 * k : q + 4 + 3 * k]) for k in range(m)])
            q += 3 * m
        q += 1
        entries = [tuple(d[q + 1 + 4 * i : q + 5 + 4 * i]) for i in range(d[q])]
        q += 1 + 4 * len(entries)
        lists = []
        for _ in range(d[q]):
            q += 1
            j = d[q]
            lists.append([tuple(d[q + 1 + 4 * i : q + 5 + 4 * i]) for i in range(j)])
            q += 4 * j
        q += 1
    except IndexError:
        return None
    return (cuts, entries, lists) if q == len(d) else None


def npc_placements(d):
    """The NPC section (map_monsters.py's walk, then `[u8 n][n × rec] [u8 m][m × (rec, x, y)]`) → [(rec, x, y)]."""
    p = header(d)[3]
    n = d[p]
    p += 1
    if n < 128:
        for _ in range(n):
            p += 1 + 3 * d[p]
    for _ in range(4):
        p += 1 + 5 * d[p]
    p += 1 + d[p]
    p += 1 + 3 * d[p]
    p += 1 + d[p]
    return [tuple(d[p + 1 + 3 * i : p + 4 + 3 * i]) for i in range(d[p])]


def fmt_load_cmd(c):
    kind, a, x, y = c
    if kind == 0x0B:
        return f"NPC{a}숨김"
    if kind == 0x09:
        return f"NPC{a}보임({x},{y})"
    if kind == 0x0D:  # bf.b(BB)V — NPC 머리 위 감정표현 (img/emoti 8 「!」 · 9 「?」, 2026-10-18)
        return f"NPC{a}표시{ {8: '!', 9: '?'}.get(x, x) }"
    return f"{kind:02x} {a:02x} {x:02x} {y:02x}"


def load_lines(d, ev):
    """Section E as [(its three events, a printable line)] — first match wins at map load."""
    load = parse_load_table(d, ev["end"])
    if load is None:
        return [((), "(로드 표: 형식이 안 맞는다)")]
    _, entries, lists = load
    out = []
    for li, (lst, *evs) in enumerate(entries):
        cond = " ".join(f"이벤트{e:#04x}" for e in evs if e) or "조건 없음"
        body = " · ".join(fmt_load_cmd(c) for c in lists[lst] if c[0] != 0x0E) if lst < len(lists) else "⛔ 그런 목록 없음"
        out.append((tuple(evs), f"로드{li}: {cond} → 목록{lst} [{body}]"))
    return out


def find_events(d):
    name, w, h, grids_end = header(d)
    for strict in (True, False):
        for p in range(grids_end, len(d)):
            ev = parse_events(d, p, strict)
            if ev:
                return name, ev
    return name, None


def conditions(r):
    quests = [(r[3 + 2 * i], r[4 + 2 * i]) for i in range(3) if r[3 + 2 * i]]
    events = [(r[9 + 2 * i], r[10 + 2 * i]) for i in range(3) if r[9 + 2 * i]]
    return quests, events


KIND = {0: "자동", 2: "말걸기", 3: "밟기"}
QUEST_Y = {0: "진행", 1: "완료", 2: "실패"}


def fmt_record(r):
    quests, events = conditions(r)
    s = KIND.get(r[2], f"종류{r[2]}")
    if r[0] or r[1]:
        s += f" 직업{r[0]} LV>={r[1]}"
    s += "".join(f" 퀘스트{a:#04x}=={b}" for a, b in quests)
    s += "".join(f" 이벤트{a:#04x}" + ("" if b else "=0") for a, b in events)
    if any(r[15:30]):
        s += " 기타[" + r[15:30].rstrip(b"\0").hex(" ") + "]"
    return s + (" → 없음" if r[30] == 0xFF else f" → s{r[30]}")


def fmt_record_in(ev, r):
    s = fmt_record(r)
    return s + " ⛔(그런 스크립트 없음)" if r[30] != 0xFF and r[30] >= len(ev["scripts"]) else s


def fmt_script(ev, i, width=60):
    out = []
    for op, a, b in ev["scripts"][i]:
        if op == 0x0F:
            t = ev["strs"][a] if a < len(ev["strs"]) else "?"
            out.append(f"대사[{a}]«{t[:width]}»")
        elif op == 0x10:
            out.append(f"질문[{a}](아니오면 {b}개 건너뜀)")
        elif op == 0x11:
            out.append(f"퀘스트{a:#04x}←{QUEST_Y.get(b, b)}")
        elif op == 0x12:
            out.append(f"이벤트{a:#04x}" + ("켬" if b == 0 else "끔"))
        elif op == 0x39:
            out.append(f"맵{a}")
        elif op == 0x3A:
            out.append(f"({a},{b})")
        elif op == 0x2D:
            out.append("끝")
        else:
            out.append(f"{op:02x} {a:02x} {b:02x}")
    return " · ".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("zip")
    ap.add_argument("maps", nargs="*", type=int)
    ap.add_argument("--quest", default="", help="콤마 목록 — 이 퀘스트를 읽는 레코드 · 쓰는 스크립트")
    ap.add_argument("--event", default="", help="콤마 목록 — 이 이벤트 플래그를 읽는 레코드 · 쓰는 스크립트")
    a = ap.parse_args()
    maps = load_maps(a.zip)
    want_q = {int(x, 0) for x in a.quest.split(",") if x}
    want_e = {int(x, 0) for x in a.event.split(",") if x}

    for mid in sorted(a.maps or maps):
        name, ev = find_events(maps[mid])
        if not ev:
            if a.maps:
                print(f"== 맵 {mid} {name}: 이벤트 구간 없음")
            continue
        if want_q or want_e:
            hits = []
            for gi, grp in enumerate(ev["groups"]):
                for ri, r in enumerate(grp):
                    quests, events = conditions(r)
                    if any(q in want_q for q, _ in quests) or any(e in want_e for e, _ in events):
                        hits.append(f"  읽음 g{gi} r{ri}: {fmt_record_in(ev, r)}")
            for si, cmds in enumerate(ev["scripts"]):
                if any((op == 0x11 and x in want_q) or (op == 0x12 and x in want_e) for op, x, _ in cmds):
                    hits.append(f"  씀   s{si}: {fmt_script(ev, si)}")
            for evs, line in load_lines(maps[mid], ev):
                if any(e in want_e for e in evs if e):
                    hits.append(f"  읽음 {line}")
            if hits:
                print(f"== 맵 {mid} {name}")
                print("\n".join(hits))
            continue
        print(f"== 맵 {mid} {name}  (구간 A @{ev['at']})")
        for gi, grp in enumerate(ev["groups"]):
            for ri, r in enumerate(grp):
                print(f"  g{gi} r{ri}: {fmt_record_in(ev, r)}")
        for si in range(len(ev["scripts"])):
            print(f"  s{si}: {fmt_script(ev, si)}")
        npcs = npc_placements(maps[mid])
        if npcs:
            print("  NPC: " + " · ".join(f"{i}=rec{r}({x},{y})" for i, (r, x, y) in enumerate(npcs)))
        for _, line in load_lines(maps[mid], ev):
            print(f"  {line}")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
