# hero2 (KTF 1.0.7) — 맵 파일의 몬스터 구간: 어느 맵에 어느 적이 몇 마리 나오는가.
#
# ── 왜 이 모듈인가 (2026-09-28) ─────────────────────────────────────────────────
# 드랍표 사이트(`droptable/make_drop_site.py`)를 만들며 맵 로더를 끝까지 읽었다. 그러자 드랍 게이트가 있는
# 적 중 둘 — 신관(#23)·신관병(#24) — 이 111장 **어느 맵의 몬스터 목록에도 없다**는 것이 드러났다(원본부터).
# #20 의 드랍 계획기(`make_hero2_asset.plan_drops`)는 그런 적도 「나오는 곳」으로 세어서, 신관만 떨구던
# 팬텀스타·파괴마의 등뼈와 (NORMAL 의) 붉은사자의흉갑을 「이미 나온다」로 두었다. 그래서 「배치됐는가」를
# 세 도구(계획기 · 사이트 · decode_drops)가 같은 코드로 묻게 여기로 뺐다.
#
# 형식 — 맵 로더 `bf.a([B)V` @0x129a94 가 읽는 순서 그대로 (2026-09-28 역어셈블, 탐색 없음):
#   [u8 번호] [u8 n][이름 cp949] [u8 n][타일셋] [u8 n][타일셋] [u8 W] [u8 H] [그리드 H×W] [그리드 H×W]
#   by   [s8 n]{ [u8 k][k × 3 B] }                      (n ≤ 0 이면 건너뛴다 — 0x12a09a 부호 비교)
#   bi×3 [u8 u]{ [u8][i16][i16] } · f [u8 u]{ … }       (5 B 레코드 넷 구간, 0x12a24a · 0x12a384 · 0x12a4a4 · 0x12a636)
#   몬스터 [u8 n][n × 전역 적 인덱스] [u8 m][m × (적 인덱스, x, y)]   (0x12a75e · 0x12a816; 인덱스 > 88 은 보스 경로 bf.a(BII)V)
# 111장 전부가 값 범위(적 인덱스 < 적 수, 배치 인덱스 ∈ 목록, 좌표 ∈ 맵) 검사를 통과한다 — 하나라도 어긋나면
# 형식을 잘못 읽은 것이므로 `load` 가 멈춘다.
import gzip

MAP_PREFIX = "P/m/m/"
MAPS = 111


def parse(d):
    """맵 한 장(압축 푼 바이트) → (이름, W, H, [적 인덱스], [(적 인덱스, x, y)])."""
    p = 1
    n = d[p]; p += 1
    name = d[p:p + n].decode("cp949", "replace").strip(); p += n
    n = d[p]; p += 1 + n
    n = d[p]; p += 1 + n
    w, h = d[p], d[p + 1]; p += 2
    p += 2 * w * h
    n = d[p]; p += 1
    if n < 128:
        for _ in range(n):
            k = d[p]; p += 1 + 3 * k
    for _ in range(4):
        u = d[p]; p += 1 + 5 * u
    n = d[p]; p += 1
    kinds = list(d[p:p + n]); p += n
    m = d[p]; p += 1
    spawns = [tuple(d[p + 3 * i:p + 3 * i + 3]) for i in range(m)]
    if p + 3 * m > len(d):
        raise ValueError("monster section overruns the map")
    return name, w, h, kinds, spawns


def load(zf, n_enemies):
    """`zf` = 맵이 든 zip(원본 덤프 · 앱 에셋 둘 다 `P/m/m/NN.m.gz`) → {맵 번호: {name, kinds, spawns}}."""
    maps = {}
    for nm in zf.namelist():
        if not (nm.startswith(MAP_PREFIX) and nm.endswith(".m.gz")):
            continue
        idx = int(nm.rsplit("/", 1)[1].split(".")[0])
        try:
            name, w, h, kinds, spawns = parse(gzip.decompress(zf.read(nm)))
        except (IndexError, ValueError) as e:
            raise SystemExit(f"map {idx}: {e} — map format misread")
        if any(k >= n_enemies for k in kinds) or len(set(kinds)) != len(kinds):
            raise SystemExit(f"map {idx}: monster kinds {kinds} out of range — map format misread")
        if any(g not in kinds or x >= w or y >= h for g, x, y in spawns):
            raise SystemExit(f"map {idx}: a spawn is outside the map or not in its kind list — map format misread")
        maps[idx] = {"name": name, "kinds": kinds, "spawns": spawns}
    # ★ 2026-10-16: the generated asset carries two more (111 보스 레이드 · 112 귀환문 변형 신전, tools_re/raid_gate.py) — the
    #   shipped 111 must all be there; extras are read like any map.
    if len(maps) < MAPS:
        raise SystemExit(f"{len(maps)} maps, expected at least {MAPS} (KTF 1.0.7)")
    return maps


def placed(maps):
    """어느 맵의 몬스터 목록에든 있는 적 인덱스 집합."""
    return {g for mp in maps.values() for g in mp["kinds"]}
