"""hero2 퀵슬롯·터치 계획 예시 이미지 생성기 (계획서용 목업).

wie_probe 가 452x245 로 뽑은 실제 게임 프레임(상단 219행 = 기기에 보이는 부분) 위에
hero2 앱의 `overlay.rs` 기하(action_disc / ring_slots / cancel_seat / disc_side_button /
joy_home_builtin / quickpage_seat)를 파이썬으로 다시 계산해 S9 가로(2220x1080)에 합성한다.
렌더러가 아니라 **배치 검증용 목업**이다 — 글꼴·아이콘 블렌딩은 앱과 다르다.

usage: python mock_hud.py <wide_dump_dir> <spr_dir> <out_dir>
"""
import math
import os
import sys

from PIL import Image, ImageDraw, ImageFont

WIN_W, WIN_H = 2220, 1080          # Galaxy S9 landscape
LW, LH, FB_H = 452, 219, 245       # logical shown / guest framebuffer (219 + 26)
PLATES = 7
RING_SEATS = 1 + PLATES
RING_A0, RING_A1 = 66.0, 195.0
RING_STEP_MAX = (RING_A1 - RING_A0) / 5.0

# overlay.rs colours (0xAARRGGBB -> RGB)
BORDER = (0x70, 0x80, 0xA0)
LABEL = (255, 255, 255)
QB_SKILL = (0x2A, 0x4A, 0x8A)
QB_EMPTY = (0x2A, 0x2E, 0x3A)
QB_INERT = (0x18, 0x1A, 0x22)
PICK = (0xE0, 0x9A, 0x3C)
DISC_OK = (0xE0, 0x9A, 0x3C)
DISC_CLR = (0x8A, 0x3A, 0x3A)
DISC_KEY = (0x2C, 0x30, 0x40)
BAR_TRACK = (0x1C, 0x21, 0x30)
BAR_HP = (0xE0, 0x3A, 0x3A)
BAR_SP = (0x3A, 0x84, 0xE6)
BAR_EXP = (0xF0, 0xC8, 0x28)

FONT_KO = "C:/Windows/Fonts/malgun.ttf"
FONT_KO_BOLD = "C:/Windows/Fonts/malgunbd.ttf"
FONT_GAME = "C:/Windows/Fonts/gulim.ttc"   # 11px 굴림 ≈ 게임 비트맵 글꼴 느낌
FONT_SYM = "C:/Windows/Fonts/seguisym.ttf"


def pad_of(w, h):
    return max(min(w, h) // 110, 3)


def action_centre(w, h, pad):
    nominal = min(max(h // 6, 56), 190)
    return (w - (pad * 3 + nominal), h - (pad * 3 + nominal))


def action_disc(w, h, pad):
    cx, cy = action_centre(w, h, pad)
    r = min(max(h // 8, 48), 140)
    return (cx - r, cy - r, r * 2, r * 2)


def disc_side_button(w, h):
    pad = pad_of(w, h)
    ax, ay, aw, ah = action_disc(w, h, pad)
    d = max(aw * 3 // 5, 56)
    return (ax - (pad * 2 + d), (ay + ah // 2) - d // 2, d, d)


def ring_slots(w, h, n, pad):
    ax, ay, aw, ah = action_disc(w, h, pad)
    ar = aw / 2.0
    acx, acy = action_centre(w, h, pad)
    clear = acx - disc_side_button(w, h)[0]
    step = min((RING_A1 - RING_A0) / (n - 1), RING_STEP_MAX) if n > 1 else 0.0

    def try_side(side):
        need = side * 1.485 / (2.0 * math.sin(math.radians(step) / 2.0)) if step > 0 else 0.0
        radius = max(need, ar + side * 0.75, clear + side / 2.0 + pad * 2)
        seats = []
        for i in range(n):
            ang = math.radians(RING_A0 + step * i)
            cx = acx + radius * math.cos(ang)
            cy = acy - radius * math.sin(ang)
            x0, y0 = cx - side / 2.0, cy - side / 2.0
            if x0 < 0 or y0 < 0 or x0 + side > w or y0 + side > h:
                return None
            seats.append((int(x0), int(y0), int(side), int(side)))
        return seats

    base = float(min(max(h // 11, 40), 120))
    s = try_side(base)
    if s:
        return s
    lo, hi, best = 0.0, base, None
    for _ in range(14):
        mid = (lo + hi) / 2
        s = try_side(mid)
        if s:
            best, lo = s, mid
        else:
            hi = mid
    return best


def ring_seat(w, h, pad, i):
    return ring_slots(w, h, RING_SEATS, pad)[i]


def cancel_seat(w, h, pad):
    d = max(disc_side_button(w, h)[2], ring_seat(w, h, pad, 0)[2])
    r = d / 2.0
    qx, qy, qw, qh = ring_seat(w, h, pad, 1)
    qcx, qcy = qx + qw / 2.0, qy + qh / 2.0
    need = r + min(qw, qh) / 2.0 + pad * 2
    cx = max(w - pad * 2 - r, r)
    dx = abs(cx - qcx)
    dy = math.sqrt(max(need * need - dx * dx, 0.0))
    cy = min(max(qcy + dy, r), h - r)
    return (int(cx - r), int(cy - r), int(d), int(d))


def joy_home_builtin(w, h):
    r = min(max(h // 12, 34), 96)
    pad = pad_of(w, h)
    return (pad * 5 + r, h - (pad * 5 + r), r)


def stick_seat(w, h):
    # 91차 STICK 정사각형: 한 변 (3*cell + 2*pad)*2, 좌하단 pad*4 인셋 — 근사
    pad = pad_of(w, h)
    cell = min(max(h // 5, 56), 190) // 2
    side = (3 * cell + 2 * pad) * 2
    side = min(side, 624)
    return (pad * 4, h - pad * 4 - side, side, side)


def page_seat(w, h, pad):
    ring = ring_slots(w, h, RING_SEATS, pad)
    base = max(ring[1:], key=lambda r: r[1])
    side = base[2]
    jx, _, jr = joy_home_builtin(w, h)
    x = base[0] - side - pad * 2
    y = base[1] + side // 2
    return (x, y, side, side)


def disc(cv, rect, fill, alpha=170, border=BORDER, width=None):
    x, y, w, h = rect
    layer = Image.new("RGBA", cv.im.size, (0, 0, 0, 0))
    ld = ImageDraw.Draw(layer)
    ld.ellipse([x, y, x + w, y + h], fill=fill + (alpha,), outline=border + (230,), width=width or max(int(w) // 40, 3))
    cv.im.alpha_composite(layer)


def ring(cv, rect, colour, width, alpha=255, inset=0):
    x, y, w, h = rect
    layer = Image.new("RGBA", cv.im.size, (0, 0, 0, 0))
    ld = ImageDraw.Draw(layer)
    ld.ellipse([x + inset, y + inset, x + w - inset, y + h - inset], outline=colour + (alpha,), width=int(width))
    cv.im.alpha_composite(layer)


class Canvas:
    def __init__(self, im):
        self.im = im
        self.d = ImageDraw.Draw(im, "RGBA")


def text_centered(cv, rect, s, font, fill=LABEL):
    x, y, w, h = rect
    bb = cv.d.textbbox((0, 0), s, font=font)
    tw, th = bb[2] - bb[0], bb[3] - bb[1]
    cv.d.text((x + (w - tw) / 2 - bb[0], y + (h - th) / 2 - bb[1]), s, font=font, fill=fill)


def load_frame(path):
    """452x245 프레임 -> 상단 219행만 -> 2220x1080."""
    im = Image.open(path).convert("RGBA")
    im = im.crop((0, 0, LW, LH))
    return im.resize((WIN_W, WIN_H), Image.NEAREST)


def icon_sheet(spr_dir, name, idx, cells=None, cell=12, vertical=False):
    """gmico_<class> 가로 스트립 / bico_<n> 낱장에서 12x12 아이콘 하나."""
    im = Image.open(os.path.join(spr_dir, name)).convert("RGBA")
    if cells is None:
        return im
    if vertical:
        return im.crop((0, idx * cell, cell, idx * cell + cell))
    return im.crop((idx * cell, 0, idx * cell + cell, cell))


def key_alpha(icon):
    """모서리 색을 컬러키로 알파 처리 (dump_mpd.py --key 와 같은 규칙)."""
    px = icon.load()
    w, h = icon.size
    key = px[0, 0][:3]
    out = Image.new("RGBA", icon.size)
    op = out.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            op[x, y] = (r, g, b, 0 if (r, g, b) == key else 255)
    return out


def draw_plate(cv, rect, icon=None, hotkey=None, kind="skill", inert=False, pressed=False, cooldown=None):
    x, y, w, h = rect
    fill = QB_INERT if inert else (QB_SKILL if icon is not None else QB_EMPTY)
    disc(cv, rect, fill, alpha=190)
    if icon is not None:
        side = int(w * 0.62)
        ic = icon.resize((side, side), Image.NEAREST)
        cv.im.alpha_composite(ic, (x + (w - side) // 2, y + (h - side) // 2))
    if hotkey:
        f = ImageFont.truetype(FONT_KO_BOLD, max(w // 4, 14))
        cv.d.text((x + w * 0.70, y + h * 0.62), hotkey, font=f, fill=(255, 220, 120, 235))
    if cooldown is not None:
        layer = Image.new("RGBA", cv.im.size, (0, 0, 0, 0))
        ld = ImageDraw.Draw(layer)
        ld.pieslice([x, y, x + w, y + h], start=-90, end=-90 + 360 * cooldown, fill=(0, 0, 0, 140))
        cv.im.alpha_composite(layer)


def draw_gauges(cv, lv=48, hp=(777, 780), sp=(390, 390), exp=(195, 112128)):
    pad = pad_of(WIN_W, WIN_H)
    x0, y0 = pad * 3, pad * 3
    f = ImageFont.truetype(FONT_KO_BOLD, 30)
    fs = ImageFont.truetype(FONT_KO, 22)
    cv.d.text((x0, y0), f"LV {lv}", font=f, fill=LABEL)
    y = y0 + 44
    bw, bh, gap = 420, 22, 12
    for frac, col, name, val in (
        (hp[0] / hp[1], BAR_HP, "HP", f"{hp[0]} / {hp[1]}"),
        (sp[0] / sp[1], BAR_SP, "SP", f"{sp[0]} / {sp[1]}"),
        (exp[0] / exp[1], BAR_EXP, "EXP", f"{exp[0]} / {exp[1]}"),
    ):
        cv.d.rounded_rectangle([x0, y, x0 + bw, y + bh], radius=bh // 2, fill=BAR_TRACK + (220,))
        cv.d.rounded_rectangle([x0, y, x0 + max(int(bw * frac), bh), y + bh], radius=bh // 2, fill=col + (255,))
        cv.d.text((x0 + bw + 14, y - 3), f"{name}  {val}", font=fs, fill=LABEL)
        y += bh + gap


def draw_hud(cv, spr_dir, quickbar, mode="field", picking=False, pressed=None, with_page=True, swap_btn=True, overlap_note=False):
    """quickbar = [(icon or None, hotkey or None, inert), ...] x7 (plate 0..6)."""
    pad = pad_of(WIN_W, WIN_H)
    ok = action_disc(WIN_W, WIN_H, pad)
    heal = disc_side_button(WIN_W, WIN_H)
    swap = (heal[0] - pad * 2 - heal[2], heal[1], heal[2], heal[3])
    clr = cancel_seat(WIN_W, WIN_H, pad)
    seats = ring_slots(WIN_W, WIN_H, RING_SEATS, pad)
    fbig = ImageFont.truetype(FONT_KO_BOLD, 54)
    fmid = ImageFont.truetype(FONT_KO_BOLD, 40)

    # stick (좌하단)
    sx, sy, ss, _ = stick_seat(WIN_W, WIN_H)
    jx, jy, jr = joy_home_builtin(WIN_W, WIN_H)
    if mode == "field":
        ring(cv, (sx, sy, ss, ss), BORDER, 4, alpha=120)
        disc(cv, (jx - jr, jy - jr, jr * 2, jr * 2), DISC_KEY, alpha=120)
    else:
        # 메뉴: 육각형 화살표 넷 (91차) 근사
        cx, cy = sx + ss // 2, sy + ss // 2
        arm = ss * 0.34
        for ang, ch in ((90, "▲"), (270, "▼"), (180, "◀"), (0, "▶")):
            ex = cx + math.cos(math.radians(ang)) * arm
            ey = cy - math.sin(math.radians(ang)) * arm
            r = ss * 0.15
            disc(cv, (ex - r, ey - r, r * 2, r * 2), DISC_KEY, alpha=150)
            text_centered(cv, (ex - r, ey - r, r * 2, r * 2), ch, fmid)

    # OK / 0 / # / CLR
    disc(cv, ok, DISC_OK, alpha=200)
    text_centered(cv, ok, "OK", fbig)
    disc(cv, heal, DISC_KEY, alpha=170)
    text_centered(cv, heal, "0", fmid)
    if swap_btn:
        disc(cv, swap, DISC_KEY, alpha=170)
        text_centered(cv, swap, "#", fmid)
    else:
        # 제안 A: `#` 는 풀아웃 키패드 + `0` 롱프레스로. `0` 밑에 작은 힌트만.
        fh = ImageFont.truetype(FONT_KO, 20)
        cv.d.text((heal[0] + 10, heal[1] + heal[3] - 30), "길게 = #", font=fh, fill=(255, 220, 120, 220))
    disc(cv, clr, DISC_CLR, alpha=190)
    text_centered(cv, clr, "CLR", ImageFont.truetype(FONT_KO_BOLD, 34))

    # plates: seats 1..7
    for i in range(PLATES):
        icon, hotkey, inert = quickbar[i]
        draw_plate(cv, seats[1 + i], icon=icon, hotkey=hotkey, inert=inert, pressed=(pressed == i))
    if picking:
        for i in range(PLATES):
            x, y, w, h = seats[1 + i]
            ring(cv, (x, y, w, h), PICK, max(w // 14, 3))
            ring(cv, (x, y, w, h), PICK, max(w // 28, 2), alpha=150, inset=int(w * 0.13))
    if overlap_note and swap_btn:
        fo = ImageFont.truetype(FONT_KO_BOLD, 30)
        x, y, w, h = swap
        ring(cv, (x - 8, y - 8, w + 16, h + 16), (255, 60, 60), 6)
        cv.d.text((x - 250, y + h + 12), "[겹침] 플레이트 6·7 이 # 위에 앉는다 (현재 코드)", font=fo, fill=(255, 90, 90, 255))

    # page + gear
    if with_page:
        pg = page_seat(WIN_W, WIN_H, pad)
        disc(cv, pg, DISC_KEY, alpha=150)
        text_centered(cv, pg, "⇄", ImageFont.truetype(FONT_SYM, 44))
    gear = (WIN_W - pad * 2 - 90, pad * 2, 90, 90)
    disc(cv, gear, DISC_KEY, alpha=120)
    text_centered(cv, gear, "⚙", ImageFont.truetype(FONT_SYM, 56))
    return {"ok": ok, "clr": clr, "seats": seats, "heal": heal, "swap": swap}


def caption(im, lines, h=150):
    out = Image.new("RGBA", (im.width, im.height + h), (18, 18, 24, 255))
    out.paste(im, (0, 0))
    d = ImageDraw.Draw(out)
    f = ImageFont.truetype(FONT_KO_BOLD, 30)
    f2 = ImageFont.truetype(FONT_KO, 25)
    y = im.height + 12
    for i, ln in enumerate(lines):
        d.text((24, y), ln, font=f if i == 0 else f2, fill=(255, 255, 255, 255) if i == 0 else (200, 205, 215, 255))
        y += 40 if i == 0 else 32
    return out


def popup_after(frame_wide, rows, sel, font_px=11):
    """게임 자신의 팝업 상자(74 x (6+16n), 중앙)를 우리가 재작성한 행 수로 다시 그린 452x245 프레임.

    원본 6행 팝업을 지우고(그 자리에는 뒤의 스킬창이 있어야 하므로 f=1770 프레임을 밑그림으로
    쓴다) 새 상자를 게임 규칙 그대로 그린다: 몸통 검정, 테두리 g.a[0]=#E6D2C0, 행 피치 16,
    선택 행 흰색·나머지 회색(#7F7F7F), 텍스트는 화면 중앙(bw.i) 기준 가운데 정렬.
    """
    im = frame_wide.copy()
    d = ImageDraw.Draw(im)
    n = len(rows)
    bh = 6 + 16 * n
    bx, by = (LW - 74) // 2, (FB_H - bh) // 2
    d.rectangle([bx, by, bx + 74, by + bh], fill=(0, 0, 0, 255), outline=(0xE6, 0xD2, 0xC0, 255))
    f = ImageFont.truetype(FONT_GAME, font_px)
    for i, s in enumerate(rows):
        col = (255, 255, 255, 255) if i == sel else (0x7F, 0x7F, 0x7F, 255)
        bb = d.textbbox((0, 0), s, font=f)
        tw = bb[2] - bb[0]
        d.text((LW // 2 - tw // 2 - bb[0], by + 6 + 16 * i + 2), s, font=f, fill=col)
    return im


def main():
    dump, spr, out = sys.argv[1], sys.argv[2], sys.argv[3]
    os.makedirs(out, exist_ok=True)

    # icons: ag.j = [1, 10, 5, 2]  (class 2 = 로그, gmico frame 2 = 6 icons; beast slot0 -> bn.j[0]=2 -> bico_2)
    def gm(i):
        return key_alpha(icon_sheet(spr, "img_gmico_2.png", i, cells=6))

    def bico(i):
        return key_alpha(Image.open(os.path.join(spr, f"img_bico_{i}.png")).convert("RGBA"))

    qb_now = [(gm(0), "1", False), (bico(2), "3", False), (gm(4), "7", False), (gm(1), "9", False),
              (None, None, False), (None, None, False), (None, None, False)]
    # 창 모델: 플레이트 4·5 에 급소가격·맹독감염이 등록돼 있고(호스트 저장소), 게스트 4칸 밖(비상주)
    qb_win = [(gm(0), "1", False), (bico(2), "3", False), (gm(4), "7", False), (gm(1), "9", False),
              (gm(2), None, False), (gm(3), None, False), (None, None, False)]

    # 1. 필드 — 퀵바 배열이 살아난 뒤 (아이콘 = ag.j 를 gmico/bico 로 해석)
    cv = Canvas(load_frame(os.path.join(dump, "frame_01200_452x245.png")))
    draw_gauges(cv)
    draw_hud(cv, spr, qb_now, mode="field", overlap_note=True)
    f = ImageFont.truetype(FONT_KO, 26)
    caption(cv.im, [
        "① 필드(현재 기하 그대로) — 퀵바 배열 `ag.j` 가 해석되면 플레이트 1~4 에 게임 아이콘이 붙는다 (일격 · 피의계약(야수) · 단검난무 · 민첩태세)",
        "  플레이트 5~7 = 호스트 칸(창 모델). [결함] 현재 코드의 8좌석 원호는 `#`(disc_row_seat 1) 위에서 끝난다: 플레이트 6 은 96 px, 7 은 18 px 겹침 (S9·S25·16:9 전부).",
        "  기하 = overlay.rs 의 action_disc / ring_slots / cancel_seat / disc_side_button 을 파이썬으로 재계산 (S9 2220x1080). 게임 HUD 는 안 보이는 26행에.",
    ]).convert("RGB").save(os.path.join(out, "01_field_icons.png"))

    # 2. 스킬창 + 게임 원래 팝업 (지금)
    cv = Canvas(load_frame(os.path.join(dump, "frame_01820_452x245.png")))
    draw_hud(cv, spr, qb_now, mode="menu", with_page=False, swap_btn=False)
    caption(cv.im, [
        "② 지금 — SKILL 창에서 스킬을 고르면 게임 자신의 드랍박스: 포인트투자 / 스킬상세정보 / 1번에 등록 / 3번에 등록 / 7번에 등록 / 9번에 등록",
        "  (wie_probe 452x245 실측 f=1820. 팝업 = 클래스 d, kind 3, 74 x (6+16n) 중앙, 행 피치 16, 선택 행만 흰색)",
    ]).convert("RGB").save(os.path.join(out, "02_skill_popup_now.png"))

    # 3. 재작성된 팝업 (제안)
    base = Image.open(os.path.join(dump, "frame_01770_452x245.png")).convert("RGBA")
    after = popup_after(base, ["포인트투자", "스킬상세정보", "퀵슬롯등록", "퀵슬롯해제"], sel=2)
    cv = Canvas(after.crop((0, 0, LW, LH)).resize((WIN_W, WIN_H), Image.NEAREST))
    draw_hud(cv, spr, qb_win, mode="menu", with_page=False, swap_btn=False)
    caption(cv.im, [
        "③ 제안 — 같은 팝업을 호스트가 게스트 메모리에서 재작성: 포인트투자 / 스킬상세정보 / 퀵슬롯등록 / 퀵슬롯해제 (6행→4행, 5행→3행)",
        "  라벨 byte[] 10 B 를 제자리에서 덮어쓰고(‘1번에 등록’ 10 B = ‘퀵슬롯등록’ 10 B) 배열 길이·d.i·상자 높이를 줄인다. 그리기는 게임이 한다.",
        "  ※ 목업: 글꼴은 굴림 11px 로 대체했다(게임 글꼴은 ftd/kor 자모 조합). 상자 위치·크기·색은 게임 규칙 그대로.",
    ]).convert("RGB").save(os.path.join(out, "03_skill_popup_proposed.png"))

    # 4. 플레이트 선택 (picking)
    cv = Canvas(after.crop((0, 0, LW, LH)).resize((WIN_W, WIN_H), Image.NEAREST))
    g = draw_hud(cv, spr, qb_win, mode="menu", picking=True, with_page=True, swap_btn=False)
    # 안내 + 화살표
    f = ImageFont.truetype(FONT_KO_BOLD, 40)
    msg = "등록할 퀵슬롯을 누르세요 (CLR = 취소)"
    bb = cv.d.textbbox((0, 0), msg, font=f)
    tw = bb[2] - bb[0]
    cv.d.rounded_rectangle([WIN_W // 2 - tw // 2 - 30, 40, WIN_W // 2 + tw // 2 + 30, 110], radius=20, fill=(0, 0, 0, 170))
    cv.d.text((WIN_W // 2 - tw // 2, 52), msg, font=f, fill=(255, 220, 120, 255))
    x, y, w, h = g["seats"][1 + 6]
    sx = int((LW // 2 + 37) * WIN_W / LW)
    sy = int(((FB_H - 70) // 2 + 6 + 16 * 2 + 8) * WIN_H / LH)
    cv.d.line([(sx, sy), (x - 6, y + h // 2)], fill=PICK + (255,), width=6)
    cv.d.ellipse([x - 16, y + h // 2 - 10, x + 4, y + h // 2 + 10], fill=PICK + (255,))
    caption(cv.im, [
        "④ 퀵슬롯등록을 고르면 — 팝업은 그대로 두고 플레이트 7개에 금색 링(draw_pick_highlight). 플레이트를 누르면 워커가:",
        "  ⓐ 그 플레이트에 게스트 슬롯(0..3)을 배정(빈 칸 우선·LRU) ⓑ 팝업 행수/배열길이를 원래대로 복원 ⓒ d.h(커서) = 그 슬롯의 ‘N번에 등록’ 행 ⓓ 합성 OK.  ②~④ 는 제안 A(`#` 제거) 기하.",
        "  → 게임 자신의 ag.a(slot, idx, beast) 가 등록·중복제거·쿨타임을 처리한다. 호스트 쓰기는 커서 1바이트 + 복원뿐.",
    ]).convert("RGB").save(os.path.join(out, "04_picking.png"))

    # 5. 터치 라우터 기하 (필드 메뉴 · 스킬창 · 팝업)
    def annotate(frame_png, boxes, title_lines, scale=4):
        im = Image.open(os.path.join(dump, frame_png)).convert("RGBA").crop((0, 0, LW, LH))
        im = im.resize((LW * scale, LH * scale), Image.NEAREST)
        layer = Image.new("RGBA", im.size, (0, 0, 0, 0))
        ld = ImageDraw.Draw(layer)
        fl = ImageFont.truetype(FONT_KO_BOLD, 22)
        for (x, y, w, h, col, label) in boxes:
            ld.rectangle([x * scale, y * scale, (x + w) * scale, (y + h) * scale], fill=col + (70,), outline=col + (255,), width=2)
            if label:
                ld.text((x * scale + 4, y * scale + 2), label, font=fl, fill=(255, 255, 255, 255))
        im.alpha_composite(layer)
        return caption(im, title_lines, h=200)

    # 필드 메뉴: bw.a=166, bw.b=64 (452x245 실측), 행 y = bw.b + 12 + 12*i, 높이 10, x = 바닥 폭(bw.a+2 .. +118)
    bx, by = 166, 64
    boxes = []
    for i, name in enumerate(["STATUS", "INVENTORY", "EQUIPMENT", "SKILL", "BEAST", "QUEST", "CARD", "SYSTEM"]):
        boxes.append((bx + 2, by + 12 + 12 * i - 1, 116, 12, (255, 200, 60) if i == 3 else (80, 180, 255), f"row {i}"))
    annotate("frame_01330_452x245.png", boxes, [
        "⑤-a 필드 메뉴 탭 라우터 — 패널 원점은 게임이 준다: (bw.a, bw.b) = (166, 64) @452x245",
        "  행 = y0 + 12 + 12·i, 높이 10, 행수 = bw.i = 8 (커서 = bw.h). 상수는 피치 하나뿐이고 원점은 매번 읽는다.",
        "  1탭 = j.h(커서) 한 바이트 쓰기(키 0회) · 같은 행 2탭 = 합성 OK · 패널 밖 = CLR (hero3 66차-B 규칙 그대로)",
        "  게이트: 상태워드 0x193aec == 5 && bw.a singleton non-null && bw.b(child) == null (자식 창이 없을 때만 메뉴 행)",
    ]).convert("RGB").save(os.path.join(out, "05a_router_menu.png"))

    # 스킬창: 패널 (166,64) 120x117 — 좌열/우열 칸(52x13 / 38x13 @ ACTIVE), 행 피치 19, 탭 헤더 y+112
    px0, py0 = 166, 64
    boxes = []
    for i in range(4):
        boxes.append((px0 + 5 + 15, py0 + 12 + i * 19 + 10, 38, 13, (80, 180, 255), f"h={i}"))
    for i in range(2):
        boxes.append((px0 + 5 + 57 + 15, py0 + 12 + i * 19 + 10, 38, 13, (80, 180, 255), f"h={4 + i}"))
    boxes.append((px0 + 8, py0 + 112 - 4, 62, 12, (255, 120, 200), "탭 ◀ ▶ (s.b 토글)"))
    annotate("frame_01770_452x245.png", boxes, [
        "⑤-b SKILL 창 — 칸 위치는 페인터 상수: x = 5 + col·57 + 15, y = 12 + row·19 + 10 (ACTIVE 탭)",
        "  커서 = s.h, 행수 = s.i = 6, 열 분할 = s.d = 4 (h<4 좌열, h>=4 우열). 탭 = s.b (true=PASSIVE, false=ACTIVE).",
        "  1탭 = s.h 쓰기 · 2탭 = OK(팝업 열림) · 탭 헤더 좌/우 = LEFT/RIGHT 합성(s.a 가 false 면 먼저 DOWN 으로 헤더 포커스) — 2단계 대상",
        "  ※ 회색(미습득) 칸의 2탭은 게임이 상세창을 연다(팝업 아님) — 게임 규칙에 맡긴다",
    ]).convert("RGB").save(os.path.join(out, "05b_router_skill.png"))

    # 팝업 행: (189, 71) 74x102, 행 y = 71+6+16i
    boxes = [(189, 71 + 6 + 16 * i, 74, 16, (255, 200, 60) if i == 0 else (80, 220, 120), f"row {i}") for i in range(6)]
    annotate("frame_01820_452x245.png", boxes, [
        "⑤-c 드랍박스(d) 행 — x = (fb_w - 74)/2, y = (fb_h - (6+16n))/2, 행 피치 16",
        "  픽셀 스캔 없이 게스트 fb 크기와 d.a:I(상자 높이)·d.i(행수)에서 계산한다 (hero3 의 popup_box 프레임 스캔이 필요 없다).",
        "  1탭 = d.h 쓰기(범위 d.i 로 거절) · 같은 행 2탭 = OK(→ 퀵슬롯등록이면 picking 진입) · 상자 밖 탭 = CLR(닫기) · 상자 크롬 = 삼킴",
    ]).convert("RGB").save(os.path.join(out, "05c_router_popup.png"))
    print("wrote 6 images to", out)


if __name__ == "__main__":
    main()
