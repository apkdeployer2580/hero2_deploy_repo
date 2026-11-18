#!/bin/bash
# usage: tools/s9_field.sh — drive the RUNNING app on S9 to the slot-1 field, each step verified by screenshot
# (title → 이어하기 → SLOT 1 → field HP bar). Exit 1 = not on the field. See HERO2_NEXT_SESSION.md 「UI 스레드 −60 %」.
ADB="$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe"; S="${ANDROID_SERIAL:?set ANDROID_SERIAL to the phone serial}"
T="$(cygpath -m "$TEMP")"
TU="$(cygpath -u "$TEMP")"
"$ADB" -s $S shell svc power stayon true
step() {
  for try in 1 2 3 4 5; do
    "$ADB" -s $S exec-out screencap -p > $TU/step.png
    if python -c "
from PIL import Image; im=Image.open('$T/step.png').convert('RGB'); import sys
sys.exit(0 if ($3) else 1)"; then return 0; fi
    "$ADB" -s $S shell input tap $1 $2; sleep 3
  done
  echo "step failed: $3"; return 1
}
step 2009 877 "im.getpixel((1110,786))[0] > 200 and im.getpixel((955,784))[0] < 100" || exit 1
step 1110 785 "im.getpixel((1110,527))[0] > 150" || exit 1
"$ADB" -s $S shell input tap 1110 527; sleep 15
"$ADB" -s $S exec-out screencap -p > $TU/field.png
python -c "
from PIL import Image; im=Image.open('$T/field.png').convert('RGB'); p=im.getpixel((150,33)); print('field HP px', p)
import sys; sys.exit(0 if p[0]>150 and p[2]<120 else 1)" || { echo "NOT ON FIELD"; exit 1; }
echo ON FIELD
