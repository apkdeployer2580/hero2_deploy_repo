#!/usr/bin/env python3
"""hero2 (영웅서기2 KTF 1.0.7) 아이템 DB 디코더.

입력 = 배포 에셋 zip (`hero2_ktf.zip`) 또는 jar 를 이미 푼 디렉터리.
출력 = UTF-8 표 / JSON.

포맷 (2026-08-28 유도. 35개 파일 전부 EOF 에 정확히 착지 = slack 0):

  jar 안 `itm/NN` 한 파일 = 아이템 타입 NN 의 레코드 배열. 파일 헤더 없음.
  NN 은 `t/itmtp.tdf` 의 인덱스와 **같다** — itmtp[7] 이 아이템 타입이 아니라
  "빈공간이 없습니다" 라는 UI 문구이고, 실제로 `itm/07` 은 **존재하지 않는다.**
  이 일치가 타입표 해석 전체의 교차검증이다.

  레코드:
    u16 BE  length      (이 필드 다음부터 레코드 끝까지의 바이트 수)
    u32 LE  price
    u8      nameLen
    bytes   name        (EUC-KR)
    ...     body        (타입 계열마다 다름 — 아래)

  ⚠ 길이는 BE, 가격과 그 뒤 정수는 LE 다. 섞여 있다. 실측이다.

필드 의미는 2026-08-31 에 SKT 역컴파일본의 **아이템 상세 렌더러**(g.java:300-400)로
검증했다 — 어느 바이트가 어느 라벨 옆에 그려지는지가 그대로 나온다.

계열 A — 무기 (itm/00~04, body 34바이트)
계열 B — 방어구 (itm/05,06,08,09,10, body 28바이트)
  두 계열은 body[0..23] 레이아웃을 공유한다.
    [0]     설명 길이 (장비는 항상 0 = 설명 없음).  cp.java: if (len==0) return
    [1]     요구 레벨 1~75          g.java:349 가 itmtp[39]="LV" 옆에 그린다
    [2]     등급 0~3                itmtp[35..38] = 일반/레어/에픽/영웅
    [3]     세트 ID 0~5             itmtp[59..63] = 바람S/테러S/사자S/베르S/팬텀S
    [4]     오브·결정 슬롯 수 0~2
    [5:15]  5쌍 (min,max) = 힘·체력·민첩·의지·지력 순
            (bi.java:204 가 i[인장typeId-13] 에 쓴다; g.java:392 가 itmtp[45+n] 로 그린다)
    [15]    ★ 요구 명예 0~10        g.java:354 가 hero.tdf[4+k] = 계급 이름으로 그린다
    [16]    스프라이트 인덱스
    [17:21] i32 LE 조합 기반 아이템 인덱스, -1 = 없음
    [21]    제련도 상한 (0,5,10…50)  g.java 가 itmtp[44]="제련도" 옆에 현재/상한을 그리고
            상한 < 1 이면 행을 숨긴다 → 투구·장갑·신발은 전부 0 = 제련 불가
    [22]    속성 0~5 = 무/물/불/땅/빛/암흑 (itmtp[50..55]; 결정 itm/19 이 부여)
    [23]    방어구: 0=경갑 1=중갑   (itmtp[56][57], gm.tdf[59] "중갑 착용 불가")
    [24:28] u32 LE  공격력 **최대**(무기) / 방어력(방어구)
  무기만:
    [28:32] u32 LE  공격력 **최소**   g.java:355 는 "최소-최대" 순으로 그린다
    [32]    미확정 (12~23)
    [33]    특성 — 오브가 부여 (itmatt.tdf = 일섬/집중/흡수/저항/투지/기절)

  ★ 위력 감쇠는 **무기에만** 걸린다: cf.java 의 두 줄이 전부이고, 방어구는
    am.a() = 방어 + 제련×1.5 로 명예 항이 없다.  honor_scale() 참조.

계열 C — 장신구 (itm/11 목걸이, itm/12 반지, body 35바이트)
  [0:15] 은 계열 A/B 와 같다. 그 뒤가 다르고, **요구 명예 바이트가 아예 없다**:
    [15],[16]  공격 (min,max) — 바이트 쌍
    [17],[18]  방어 (min,max) — 바이트 쌍
    [19:27]    HP  (min,max) — u32 쌍
    [27:35]    SP  (min,max) — u32 쌍
  (전부 g.java:370-388 의 리터럴 라벨. **[15] 를 명예로 읽으면 가짜 명예 아이템이 생긴다.**)

계열 D — 소모품/재료 (나머지): price 뒤가 u8 descLen + EUC-KR 설명.
  예) itm/20 제련석 1개, itm/13 힘의 인장 하급/중급/상급 (1000/2000/3000골드,
      "무기에 힘 +0~+4" / "+2~+7" / "+5~+10").

`itm/forshop` 은 별도 프레이밍: u8 recLen(= 뒤따르는 인덱스+body 길이), u16 BE
전역 아이템 인덱스, 그리고 해당 itm 파일의 body 와 바이트 동일한 body.
"""

import argparse
import io
import json
import os
import struct
import sys
import zipfile

GRADE = ["일반", "레어", "에픽", "영웅"]
SET = ["-", "바람S", "테러S", "사자S", "베르S", "팬텀S"]

# itmtp[50..55] — 결정(itm/19)이 부여한다.
ELEMENT = ["무속성", "물", "불", "땅", "빛", "암흑"]

# t/hero.tdf[4..17] + [28] — 명예 등급 사다리 15단계.
# [28] 총사령관 이 [26]빈 슬롯 / [27]슬롯 뒤에 붙어 있다 = 15번째 계급이 뒤늦게 추가됐다.
# (같은 파일 [0] 클레르 = 주인공, [1..3] 버서커/나이트/로그 = 전투 스타일, [25] = "명예" 라벨)
HONOR_RANK = [
    "수련생", "용병", "경보병", "경기병", "이십인장", "백인장", "견습기사",
    "준기사", "정규기사", "기사단원", "기사단장", "근위단원", "근위단장",
    "사령관", "총사령관",
]

# 파일 번호 == t/itmtp.tdf 인덱스.
TYPE_NAME = {
    0: "양손대검", 1: "양손도끼", 2: "도검", 3: "단검", 4: "둔기",
    5: "투구", 6: "갑옷", 8: "장갑", 9: "신발", 10: "방패",
    11: "목걸이", 12: "반지",
    13: "힘의 인장", 14: "체력의 인장", 15: "민첩의 인장",
    16: "의지의 인장", 17: "지력의 인장",
    18: "오브", 19: "결정", 20: "제련석", 21: "마법구", 22: "야수진화제",
    23: "조합서", 24: "포션", 25: "하이포션", 26: "포도주", 27: "치료제",
    28: "귀환서", 29: "조합서", 30: "광석", 31: "스크롤", 32: "재료",
    33: "퀘스트 아이템", 34: "스펠카드",
}

WEAPONS = {0, 1, 2, 3, 4}
ARMOUR = {5, 6, 8, 9, 10}
ACCESSORY = {11, 12}


def records(blob):
    """u16 BE 길이접두 레코드 배열. EOF 에 정확히 착지하지 않으면 예외."""
    out, i = [], 0
    while i + 2 <= len(blob):
        (ln,) = struct.unpack_from(">H", blob, i)
        body = blob[i + 2 : i + 2 + ln]
        if len(body) != ln:
            raise ValueError("record overruns EOF at offset %d (len=%d)" % (i, ln))
        out.append(body)
        i += 2 + ln
    if i != len(blob):
        raise ValueError("trailing %d byte(s) after last record" % (len(blob) - i))
    return out


def split_head(rec):
    (price,) = struct.unpack_from("<I", rec, 0)
    n = rec[4]
    return price, rec[5 : 5 + n].decode("euc-kr", "replace"), rec[5 + n :]


def honor_scale(value, req_honor, player_honor):
    """무기 위력 감쇠. cf.java:56-71 — 방어구/방패에는 적용되지 않는다."""
    if player_honor >= req_honor:
        return value
    return value * (10 - req_honor + player_honor) // 10


EQUIPMENT_BODY = {True: 34, False: 28}  # 무기(계열 A) · 방어구(계열 B) 의 설명 없는 body 길이


def equipment_body(body, weapon):
    """계열 A/B 의 body 를 설명 없는 꼴(34 / 28 B)로 — 아니면 None.

    ★ 2026-10-14 신규 아이템 2차: 가디언 장비 · 스킬 특화 장비는 옵션 문구를 설명으로 싣는다(상세창의 옵션 페이지 —
    `guest_anchors::AccessoryPatch` GEAR_AT). 게임의 파서는 장신구와 같이 이름 뒤 [설명 길이][설명] 을 읽고 남은 자리부터
    꼬리를 읽는다(`ch.a([BIZZ)I`) — 그래서 설명이 있으면 body[0] = n 이고 꼬리는 body[1 + n:] 이다. 배포본 장비는 전부 n = 0.
    """
    n = body[0] if body else 0
    if len(body) != EQUIPMENT_BODY[weapon] + n:
        return None
    return bytes([0]) + body[1 + n :]


def decode_equipment(body, weapon):
    """계열 A/B 공용. 장신구(계열 C)에는 쓰지 말 것 — [15] 가 명예가 아니다. 설명이 있으면 desc 로 준다."""
    n = body[0] if body else 0
    desc = body[1 : 1 + n].decode("euc-kr", "replace")
    body = equipment_body(body, weapon) or body
    level = body[1]  # body[0] 은 설명 길이 — 설명을 뗀 뒤의 꼴
    (base,) = struct.unpack_from("<i", body, 17)
    d = {
        "desc": desc,
        "descLen": n,
        "level": level,
        "grade": body[2],
        "gradeName": GRADE[body[2]] if body[2] < len(GRADE) else "?%d" % body[2],
        "setId": body[3],
        "setName": SET[body[3]] if body[3] < len(SET) else "?%d" % body[3],
        "slots": body[4],
        "optionRanges": [(body[5 + 2 * k], body[6 + 2 * k]) for k in range(5)],
        "reqHonor": body[15],
        "reqHonorRank": HONOR_RANK[body[15]] if body[15] < len(HONOR_RANK) else "?%d" % body[15],
        "sprite": body[16],
        "craftBase": None if base == -1 else base,
        "refineMax": body[21],
        "element": ELEMENT[body[22]] if body[22] < len(ELEMENT) else "?%d" % body[22],
        "power": int.from_bytes(body[24:28], "little"),  # 무기 공격 최대 / 방어구 방어
    }
    if weapon:
        d["powerMin"] = int.from_bytes(body[28:32], "little")
        d["unk32"] = body[32]
        d["attrib"] = body[33]  # 오브가 부여하는 특성 (itmatt.tdf)
        # ★ 감쇠는 무기에만 걸린다 (cf.java). 방어구/방패는 요구 명예를 표시만 한다.
        d["powerAtHonor0"] = honor_scale(d["power"], d["reqHonor"], 0)
    else:
        d["armourClass"] = "중갑" if body[23] else "경갑"
    return d


def accessory_body(body):
    """계열 C 의 body 를 설명 없는 35 B 꼴로 — 없으면 None.

    ★ 2026-10-02: 스킬을 바꾸는 장신구 6종(itm/11·12 의 17..19)은 설명을 싣는다. 게임의 기본 파서
    `ch.a([BIZZ)I` 가 이름 뒤 [설명 길이][설명] 을 읽고 남은 자리부터 `z` 가 레벨을 읽으므로, 설명이
    있으면 body[0] = n 이고 레벨은 body[1 + n] 이다. 배포본 34 개는 전부 n = 0 (35 B).
    """
    n = body[0] if body else 0
    if len(body) != 35 + n:
        return None
    return bytes([0]) + body[1 + n :]


def decode_accessory(body):
    """계열 C. 요구 명예 바이트가 없다 — [15] 는 공격 최소값이다. 설명이 있으면 desc 로 준다."""
    desc = body[1 : 1 + body[0]].decode("euc-kr", "replace")
    body = accessory_body(body)
    return {
        "desc": desc,
        "level": body[1],
        "grade": body[2],
        "gradeName": GRADE[body[2]] if body[2] < len(GRADE) else "?%d" % body[2],
        "optionRanges": [(body[5 + 2 * k], body[6 + 2 * k]) for k in range(5)],
        "atk": (body[15], body[16]),
        "def": (body[17], body[18]),
        "hp": (int.from_bytes(body[19:23], "little"), int.from_bytes(body[23:27], "little")),
        "sp": (int.from_bytes(body[27:31], "little"), int.from_bytes(body[31:35], "little")),
    }


def decode_consumable(body):
    n = body[0]
    return {"desc": body[1 : 1 + n].decode("euc-kr", "replace")}


def load_itm(source):
    """source = hero2_ktf.zip 경로, 또는 jar 를 이미 푼 디렉터리."""
    files = {}
    if os.path.isdir(source):
        d = os.path.join(source, "itm")
        for n in sorted(os.listdir(d)):
            with open(os.path.join(d, n), "rb") as f:
                files[n] = f.read()
        return files
    with zipfile.ZipFile(source) as outer:
        jar = next((n for n in outer.namelist() if n.endswith(".jar")), None)
        if jar is None:
            raise SystemExit("%s: no .jar member (is this the asset zip?)" % source)
        with zipfile.ZipFile(io.BytesIO(outer.read(jar))) as j:
            for n in j.namelist():
                if n.startswith("itm/") and not n.endswith("/"):
                    files[n[4:]] = j.read(n)
    return files


def decode_all(source):
    items = []
    for name, blob in sorted(load_itm(source).items()):
        if name == "forshop":
            continue
        try:
            tid = int(name)
        except ValueError:
            continue
        for idx, rec in enumerate(records(blob)):
            price, iname, body = split_head(rec)
            row = {
                "file": "itm/" + name,
                "typeId": tid,
                "typeName": TYPE_NAME.get(tid, "?%d" % tid),
                "index": idx,
                "name": iname,
                "price": price,
                "bodyLen": len(body),
                "bodyHex": body.hex(" "),
            }
            if tid in WEAPONS and equipment_body(body, True) is not None:
                row.update(decode_equipment(body, weapon=True))
            elif tid in ARMOUR and equipment_body(body, False) is not None:
                row.update(decode_equipment(body, weapon=False))
            elif tid in ACCESSORY and accessory_body(body) is not None:
                row.update(decode_accessory(body))
            else:
                try:
                    row.update(decode_consumable(body))
                except Exception:
                    pass
            items.append(row)
    return items


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("source", help="hero2_ktf.zip 또는 jar 를 푼 디렉터리")
    ap.add_argument("--json", action="store_true", help="JSON 으로 출력")
    ap.add_argument("--honor", action="store_true", help="요구 명예 > 0 만")
    args = ap.parse_args()

    out = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    items = decode_all(args.source)
    if args.honor:
        items = [i for i in items if i.get("reqHonor", 0) > 0]

    if args.json:
        json.dump(items, out, ensure_ascii=False, indent=1)
        out.write("\n")
        out.flush()
        return

    for i in items:
        if "reqHonor" in i:
            out.write(
                "%-10s LV%-3d %-4s %-6s 명예%-2d(%-5s) %-18s 가격%-9d 위력%-4s %s\n"
                % (i["typeName"], i["level"], i["gradeName"], i["setName"],
                   i["reqHonor"], i["reqHonorRank"], i["name"], i["price"],
                   i["power"],
                   "명예0->%d" % i["powerAtHonor0"] if "powerAtHonor0" in i
                   else "(방어구=감쇠없음)")
            )
        else:
            out.write("%-10s %-18s 가격%-9d %s\n"
                      % (i["typeName"], i["name"], i["price"],
                         i.get("desc", i.get("note", ""))))
    out.write("\n총 %d개\n" % len(items))
    out.flush()


if __name__ == "__main__":
    main()
