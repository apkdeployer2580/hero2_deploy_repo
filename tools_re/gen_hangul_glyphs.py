#!/usr/bin/env python3
"""Generate the tiny Hangul bitmap set the overlay needs, as Rust source.

WHY THIS EXISTS
---------------
`overlay::draw_glyph` is a 5x7 ASCII bitmap font, so every host-drawn string has
been ASCII — which is why the cash-buff status lines said "EXP X2 6D 23H" instead
of naming the item.  The player asked for the real Korean names (2026-08-06), and
a full Hangul font is neither shippable nor necessary: the host draws a CLOSED,
KNOWN set of Korean words, so only the syllables in that set need glyphs.

The output is a Rust source file of const bitmaps, not a runtime asset: it is ~40
lines of data, it must never fail to load, and keeping it in the crate means the
standalone selftests can `include!` it like everything else.

Re-run after adding a word:
    python tools_re/gen_hangul_glyphs.py
and check the ASCII-art preview it prints before committing the result.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# Every Korean word the overlay draws. Keep this list as the single source of
# truth; the syllable set is derived from it so a new word cannot forget a glyph.
WORDS = [
    "경험의비석",  # cash item, EXP x2
    "경험의보주",  # cash item, EXP x4
    "초월의문장",  # cash item, equip level requirement -5
    "일",  # time units, from buffs::fmt_time_left
    "시간",
    "분",
    "저장완료",  # quick-save toast (player 2026-08-08: "Saved! 말고 저장완료")
    "저장실패",  # ...and its two failure twins, so the set cannot end up half-Korean
    "저장불가",
    # ★ 2026-09-01: the HOST quickbar plate. The bar is one plate wider than the game's
    # own (crate::QUICK_SLOTS vs GUEST_QUICK_SLOTS) and that plate cannot be pressed
    # until the host can bind it, so a tap says so instead of doing nothing in silence.
    "사용불가",
    # ★ 59차: the settings-row toasts. 57차 shipped the HEAT SAVER toast without
    # adding its syllables, so it drew as ": ()" on the device — a missing glyph is
    # silently BLANK (`overlay::draw_hangul` returns early), which is exactly the
    # failure this list exists to prevent. Add the word here BEFORE writing a toast.
    "발열절감",  # HEAT SAVER row (57차)
    "재시작후적용",  # ...and its "applies at next start" tail
    "켬",  # on / off, shared by both toggles
    "끔",
    "게임패드",  # PAD HUD row (59차)
    "자동",
    "항상",
    # ★ 2026-09-17 (퀵슬롯·터치 계획 S0~S3, HERO2_QUICKSLOT_TOUCH_PLAN.md): the `#`
    # key left the field HUD (its seat collided with plates 6·7), so a long press on
    # `0` swaps the recovery item and SAYS so; and the 퀵슬롯등록 / 해제 flow toasts.
    "회복아이템교체",  # long-press `0` -> `#`
    "퀵슬롯등록",  # popup row + toast
    "퀵슬롯해제",  # popup row + toast
    "완료",  # ...등록완료 / 해제완료
    "등록안됨",  # 해제 on a skill no plate holds
    "등록할퀵슬롯선택",  # picking hint
    # ★ S7 (2026-09-17 저녁): a potion plate pressed with none of that type in the bag.
    "회복아이템없음",
    # ★ hero3 82차 port (2026-09-17 저녁): the layout editor's HIDE / SHOW toasts.
    "퀵슬롯먼저선택",  # HIDE with nothing selected
    "퀵슬롯하나는남겨야함",  # HIDE on the last visible plate
    "퀵슬롯숨김",
    "숨긴퀵슬롯없음",  # SHOW with nothing hidden
    "퀵슬롯전체복원",
    # ★ 2026-09-18: the gear panel itself went Korean, to match hero3 (user: "상단
    # 톱니바퀴로 설정하는 부분 hero3랑 다른거같아"). Every row is a TITLE line over a
    # STATE line, so both halves of every row belong here.
    "설정",  # the panel's title band
    "화면배치",  # LAYOUT row
    "가로전용",  # ...its "landscape immersive only" state, shared with 조작 방식
    "사용자배치",  # ...and "the player has moved something"
    "편집",
    "진단정보",  # DIAG row
    "세이브복원",  # ...its second line: the row is also the way to the save vault
    "조작방식",  # TOUCH TYPE row
    "십자키",
    "자유스틱",
    "효과음크기",  # SFX level row (hero3 2026-08-27)
    "기본",  # ...the shipped level, i.e. "unchanged"
    "패드표시",  # PAD HUD row — 자동 / 항상 켬 / 끔 are already above
    "빠른저장",  # QUICK SAVE row
    "지금",
    # ★ 2026-09-20 (잔여 작업 #10): the worker-death notice. When the wie-emu thread
    # dies the UI thread and the audio thread live on, so the player sees a frozen game
    # with the music still playing and no explanation — these lines ARE the explanation,
    # and they stay up for the rest of the process (`overlay::draw_fatal_notice`).
    "오류로중단됨",
    "앱을다시실행하세요",
    "세이브백업있음",  # ...only when the vault actually holds a generation
    # ★ #13 step 7 (2026-09-21): the ACCOUNT warehouse moves — the popup ROWS themselves
    # are the game's (gm.tdf rows 78/79, drawn by its own font); these are the host's
    # refusals when the other side has no room, and the confirmations.
    "창고가득참",
    "가방가득참",
    "창고에넣음",
    "가방에넣음",
    # ★ #22 (2026-09-23): the 망각의 구's host toasts. The popup ROWS themselves are the
    # game's (gm.tdf 80/81); these are what the worker says after — or instead of — a reset.
    "스탯초기화됨",
    "스킬초기화됨",
    "초기화실패",
    "되돌릴것없음",
    # ★ 2026-09-23 (기본공격 버그): the repair toast. The skill reset used to zero the
    # leading combo stages of `a.p`, which are 「기본공격」 and not skills; a save made
    # while that was live comes back with stage 1 unlearned, and the worker puts it back.
    "기본공격복구",
]

# 12x12 from Gulim at 12 px, chosen by rendering 험/경/월 in every candidate and
# reading the ASCII art: at 10 px every font loses strokes ("험" becomes mush), and
# Gulim — a screen face with hinting built for exactly this size — stays legible at
# 12 where Malgun still blurs. Bigger cells were legible too but the player asked
# for SMALLER text, and 12 is the smallest that survives.
W = H = 12
FONT_CANDIDATES = [
    (r"C:\Windows\Fonts\gulim.ttc", 12),
    (r"C:\Windows\Fonts\malgun.ttf", 12),
    (r"C:\Windows\Fonts\batang.ttc", 12),
]
OUT = Path(__file__).resolve().parent.parent / "wie_android" / "src" / "hangul.rs"


def load_font():
    for path, size in FONT_CANDIDATES:
        if Path(path).exists():
            return ImageFont.truetype(path, size), Path(path).name, size
    sys.exit("no Korean font found; edit FONT_CANDIDATES")


def render(font, ch):
    """1-bit W x H bitmap of `ch`, centred in the cell.

    Rendered DIRECTLY at the target size rather than oversampled-and-downscaled:
    a hinted screen face puts its strokes on the pixel grid on purpose, and
    resampling averages that back into grey which then thresholds to mush.
    """
    img = Image.new("L", (W * 2, H * 2), 0)
    ImageDraw.Draw(img).text((0, 0), ch, font=font, fill=255)
    bbox = img.getbbox()
    if bbox:
        img = img.crop(bbox)
    cell = Image.new("L", (W, H), 0)
    cell.paste(img, (max((W - img.width) // 2, 0), max((H - img.height) // 2, 0)))
    rows = []
    for y in range(H):
        bits = 0
        for x in range(W):
            if cell.getpixel((x, y)) >= 110:
                bits |= 1 << (W - 1 - x)
        rows.append(bits)
    return rows


def main():
    font, font_name, font_size = load_font()
    syllables = sorted({ch for w in WORDS for ch in w})
    glyphs = [(ch, render(font, ch)) for ch in syllables]

    for ch, rows in glyphs:
        print(f"--- {ch} ---")
        for bits in rows:
            print("".join("#" if bits & (1 << (W - 1 - x)) else "." for x in range(W)))

    lines = [
        "//! Hangul glyphs for the CLOSED set of Korean words the overlay draws.",
        "//!",
        "//! GENERATED by `tools_re/gen_hangul_glyphs.py` — do not hand-edit; add the word",
        f"//! there and re-run. Source font: {font_name} at {font_size} px, 4x oversampled",
        "//! and thresholded (a direct 10 px render loses whole strokes, and Hangul is all",
        "//! strokes).",
        "//!",
        "//! Only the syllables actually used are here, which is what makes shipping Korean",
        "//! text possible at all: `overlay::draw_glyph` is a 5x7 ASCII font, and a real",
        "//! Hangul font is ~11 172 syllables.",
        "",
        f"/// Glyph cell size. Taller than the {5}x{7} ASCII cell, so mixed text is",
        "/// BOTTOM-aligned by the caller.",
        f"pub const HANGUL_W: u32 = {W};",
        f"pub const HANGUL_H: u32 = {H};",
        "",
        "/// Bitmap rows, MSB = leftmost pixel of the cell.",
        f"const GLYPHS: [(char, [u16; {H}]); {len(glyphs)}] = [",
    ]
    for ch, rows in glyphs:
        body = ", ".join(f"0b{bits:0{W}b}" for bits in rows)
        lines.append(f"    ('{ch}', [{body}]),")
    lines += [
        "];",
        "",
        "/// Rows for `ch`, or `None` when it is not one of the syllables we ship.",
        f"pub fn glyph(ch: char) -> Option<&'static [u16; {H}]> {{",
        "    GLYPHS.iter().find(|(c, _)| *c == ch).map(|(_, rows)| rows)",
        "}",
        "",
        "/// Whether every Hangul character in `s` has a glyph — the check the selftest",
        "/// uses so a new word can never ship with missing syllables.",
        "///",
        "/// `allow(dead_code)`: its only caller is `buffs_selftest.rs`, which `include!`s",
        "/// this file rather than linking the crate, so the crate build cannot see the use.",
        "#[allow(dead_code)]",
        "pub fn covers(s: &str) -> bool {",
        "    s.chars().filter(|c| !c.is_ascii()).all(|c| glyph(c).is_some())",
        "}",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"\nwrote {OUT} ({len(glyphs)} glyphs)")


if __name__ == "__main__":
    main()
