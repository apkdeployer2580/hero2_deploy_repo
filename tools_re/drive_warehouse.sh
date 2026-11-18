#!/bin/bash
# Drive Hero3 (wie_android) from cold start to 네트워크창고 and dump the NETTRACE.
#
# Overlay button centres for the S25 portrait layout (1080x2340), derived from
# wie_android/src/overlay.rs::layout and confirmed against a screencap.
set -e

# ⚠ 2026-09-04 (순서표 #6). This used to `cd /d/tmp` and hardcode one phone's serial.
# Both are gone: the working directory comes from the caller, and the phone from
# $ANDROID_SERIAL (adb honours it) or $1. The old serial was the S25, which is the
# handset this project must NOT install to.
#
# ⚠ AND THE TAPS ARE STILL HERO3's. Every coordinate below was measured on hero3's
# 1080x2340 portrait layout on that S25; none of it has been re-derived for hero2 or
# for the S9. Treat this as the recorded PROCEDURE, not a runnable script, until the
# coordinates are re-measured.
SERIAL="${ANDROID_SERIAL:-$1}"
if [ -z "$SERIAL" ]; then
    echo "usage: ANDROID_SERIAL=<serial> $0    (or: $0 <serial>)" >&2
    echo "       run it from the directory holding platform-tools/ and netcap/" >&2
    exit 2
fi
ADB="${ADB:-./platform-tools/adb.exe}"
A="$ADB -s $SERIAL"
run() { MSYS_NO_PATHCONV=1 $A "$@"; }

OK=(229 1872); RIGHT=(377 1872); LEFT=(76 1872)
tap() { run shell input tap "$1" "$2"; }

run logcat -c
run shell am force-stop net.wie.emulator
sleep 1
run shell monkey -p net.wie.emulator -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
sleep 6                       # boot + title

tap "${OK[@]}";    sleep 2    # PRESS ANY KEY -> main menu (게임시작)
tap "${RIGHT[@]}"; sleep 1    # -> 네트워크창고
tap "${OK[@]}";    sleep 2    # -> "네트워크 접속시 통화료..." dialog (예 선ected)
tap "${OK[@]}";    sleep 8    # 예 -> network bring-up + socket connect

run exec-out screencap -p > netcap/w1.png
echo "=== NETTRACE ==="
run logcat -d | grep -a "NETTRACE" || echo "(none)"
echo "=== EMULATOR ERRORS ==="
run logcat -d | grep -a "emulator tick error" | head -3 || echo "(none)"
