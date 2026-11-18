#!/bin/bash
# hero2 가디언 밸런스 실측 (2026-10-08): 한 마리와 싸우는 probe 런 + 창 분석.
#   usage: tools_re/guardian_fight.sh <fs-root> <tag> <corr 0|1> <frames> <OK cadence> [extra probe args]
#   env SLOTKEYS = 슬롯 고르는 키 (기본 슬롯 2 = "320:OK,420:OK,470:RIGHT,520:OK"; 슬롯 1 = "320:OK,420:OK,520:OK")
# 숲 세이브(맵 100, 플레이어 (15,17) 가 오른쪽을 본다): f=538 에 다른 일곱 마리를 눕히고(--h2-mob-hurt, ≈1,300 프레임 뒤 되살아난다)
# f=550 에 (15,20) 의 한 마리를 앞 칸 (16,17) 로 옮긴 뒤 OK 를 cadence 마다. 로그 = target/fight_<tag>.log, 분석 = guardian_fight_parse.py.
cd "$(dirname "$0")/.."
FS=$1; TAG=$2; CORR=$3; FR=$4; CAD=$5; shift 5
KEYS=${SLOTKEYS:-"320:OK,420:OK,470:RIGHT,520:OK"}
for f in $(seq 560 $CAD $((FR-100))); do KEYS="$KEYS,$f:OK"; done
HURT=""
for t in 22:6 14:8 19:10 10:15 22:15 6:20 22:23; do HURT="$HURT --h2-mob-hurt 538:$t:30000"; done
./target/release/wie_probe.exe --zip android_app/app/src/main/assets/hero2_ktf.zip --fs $FS --w 176 --h 220 --ms 20 \
  --frames $FR --keys $KEYS --gauges 2 --h2-mob-place 550:15:20:16:17 --h2-guardian-word 9:$CORR $HURT "$@" \
  > target/fight_$TAG.log 2>&1
python tools_re/guardian_fight_parse.py $TAG
