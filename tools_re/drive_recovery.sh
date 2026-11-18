#!/bin/bash
# Drive Hero3 (wie_android) from cold start to the SLOT screen and press '#' (복구).
# The slot screen's bottom menu shows "#: 복구" = server-side slot backup/restore,
# the other half of Track C alongside 네트워크창고.
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

OK="229 1872"; HASH="962 2200"
tap() { run shell input tap $1; }

run logcat -c
run shell am force-stop net.wie.emulator
sleep 1
run shell monkey -p net.wie.emulator -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
sleep 6                       # boot + title

tap "$OK";   sleep 2          # PRESS ANY KEY -> main menu (게임시작)
tap "$OK";   sleep 3          # 게임시작 -> slot screen
run exec-out screencap -p > netcap/r1.png
tap "$HASH"; sleep 3          # '#' -> 복구
run exec-out screencap -p > netcap/r2.png
tap "$OK";   sleep 6          # confirm whatever dialog appeared
run exec-out screencap -p > netcap/r3.png

echo "=== NETTRACE ==="
run logcat -d | grep -a "NETTRACE" || echo "(none)"
echo "=== EMULATOR ERRORS ==="
run logcat -d | grep -a "emulator tick error" | head -3 || echo "(none)"
