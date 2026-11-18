#!/usr/bin/env python3
"""월드맵 길 표(`worldmap_view.rs` 의 `WM_ROADS`)를 맵 데이터에서 다시 만들어 대조한다 (2026-10-20).

길 = 캐릭터가 **걸어서** 오가는 두 장소: 맵 이벤트 구간의 밟기 레코드(종류 3)가 부르는 스크립트의 `39 <맵>`
워프를 `wm_place_index` 로 장소에 접은 것. 컷신 워프(종류 0 — 보스 처치 뒤 신드라프 → 윈터하츠, 납골당 → 신전,
신전지하 ↔ 윈터하츠/언더시티)와 말 걸기(종류 2)만 있는 쌍은 길이 아니다. 한쪽 방향만 있는 쌍도 길이 아니다.

    python tools_re/worldmap_roads.py android_app/app/src/main/assets/hero2_ktf.zip        # 다르면 exit 1
    python tools_re/worldmap_roads.py <zip> --print                                        # 맵 쌍까지
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import decode_map_events as D  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RS = os.path.join(ROOT, "wie_android", "src", "worldmap_view.rs")


def read_view():
    src = open(RS, encoding="utf-8").read()
    names = re.findall(r'WmPlace \{ name: "([^"]+)"', src)
    consts = dict(re.findall(r"pub const (WM_\w+): usize = (\d+);", src))
    body = re.search(r"pub fn wm_place_index\(map: u32\) -> usize \{\s*match map \{(.*?)\n    \}", src, re.S).group(1)
    pidx = {}
    for line in body.strip().splitlines():
        line = line.strip().rstrip(",")
        if not line or line.startswith("_"):
            continue
        pat, target = line.split("=>")
        for t in pat.replace("|", " ").split():
            pidx[int(t)] = int(consts[target.strip()])
    roads_src = re.search(r"pub const WM_ROADS: &\[\(usize, usize\)\] = &\[(.*?)\];", src, re.S).group(1)
    roads = {tuple(sorted((int(consts[a]), int(consts[b])))) for a, b in re.findall(r"\((WM_\w+), (WM_\w+)\)", roads_src)}
    return names, pidx, roads


def walk_edges(zpath, pidx):
    """{(place a, place b)} with a walk warp in BOTH directions, plus the map pairs behind each."""
    maps = D.load_maps(zpath)
    directed = {}
    for mid in sorted(maps):
        _, ev = D.find_events(maps[mid])
        if not ev:
            continue
        step_scripts = {r[30] for grp in ev["groups"] for r in grp if r[2] == 3 and r[30] != 255}
        for si in step_scripts:
            for op, x, _ in ev["scripts"][si]:
                if op == 0x39:
                    pa, pb = pidx.get(mid, 0), pidx.get(x, 0)
                    if pa != pb:
                        directed.setdefault((pa, pb), set()).add((mid, x))
    both = {}
    for (a, b), pairs in directed.items():
        if (b, a) in directed and a < b:
            both[(a, b)] = pairs | directed[(b, a)]
    return both


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not args:
        print(__doc__)
        return 2
    names, pidx, roads = read_view()
    found = walk_edges(args[0], pidx)
    if "--print" in sys.argv:
        for (a, b), pairs in sorted(found.items()):
            print(f"{names[a]} — {names[b]}: " + " ".join(f"{m}->{t}" for m, t in sorted(pairs)))
    missing = sorted(set(found) - roads)
    extra = sorted(roads - set(found))
    for a, b in missing:
        print(f"WM_ROADS 에 없는 길: {names[a]} — {names[b]}")
    for a, b in extra:
        print(f"맵 데이터에 없는 길: {names[a]} — {names[b]}")
    print(f"길 {len(found)} · 표 {len(roads)} · {'일치' if not missing and not extra else '불일치'}")
    return 1 if missing or extra else 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.exit(main())
