#!/usr/bin/env python3
"""영웅서기2 KTF 원본 zip → 앱/프로브가 먹는 hero2_ktf.zip 재패키징.

원본 = KTF 배포본 덤프(예: "영웅서기2_1.zip"): __adf__, __class__, __env__,
01035D0B.jar, big/middle/small.icon, P/(추가다운로드분: m/m 맵 111장, snd, hl2List …).

wie_ktf::from_archive 가 요구하는 모양은 { __adf__, <AID>.jar, [P/**] } 뿐이다
(P/ 접두는 wie 가 벗겨서 게스트 FS 에 넣는다). __class__/__env__/아이콘은
KTF 단말 플랫폼 메타라 불필요.

    python tools_re/make_hero2_asset.py <원본.zip> [출력.zip]

출력 기본값 = android_app/app/src/main/assets/hero2_ktf.zip (gitignore 대상).

★ 2026-09-21 — 텍스트 표 패치 (#13 5단계, CARD → ACCOUNT). jar 안의 `t/gm.tdf` 를 다시 쓴다:
  * 행 11 `CARD` → `ACCOUNT`, 행 19 `카드` → `계정창고` — 필드 메뉴의 여섯째 행(비포커스 영문 /
    포커스 한글, HERO2_GUEST_RE.md §2.7.1). 라이브 `byte[]` 교체가 아니라 에셋 패치인 이유: 게임은
    메뉴를 열 때마다 표를 다시 읽고 닫을 때 null 로 만든다(`bw.a(boolean)`), 그래서 런타임 교체는
    열 때마다 되풀이돼야 했다. 표 포맷(`[1B count][2B BE len][EUC-KR]`)이 가변 길이라 7B 도 그냥 들어간다.
  * 행 78 `창고에넣기`, 행 79 `가방에넣기` — **덧붙인** 행. 게임은 상수 인덱스로만 읽으므로 뒤에 붙는
    행은 아무도 안 읽고, 호스트가 `h2_gm_row_handle(78/79)` 로 팝업 라벨(`PopupLabelRef`)에 쓴다.
  * ★ 2026-09-26 — 행 82~89 = 창고·초기화의 **안내 문구**(장착 장비 거절 · 넣었습니다 · 초기화됨 …).
    호스트 토스트 대신 게임 자신의 메시지 상자 `ba` 에 담긴다. GM_APPEND 주석 참조.
  이 도구가 재생성한 zip 만이 정본이다 — 원본 덤프는 건드리지 않는다. `client.bin1120` 은 바이트 그대로
  (probe 의 빌드 식별 md5 `08a53cc4…` 불변).

★ 2026-09-23 — 경갑/중갑 버그수정 (#19). jar 안의 `itm/05`(투구) `itm/08`(장갑) `itm/forshop` 에서
  **`body[23]` 세 바이트**를 뒤집는다. 레코드 길이도 개수도 안 바뀌므로 `u16 BE 길이`도 `forshop` 의
  `u8 recLen` 도 그대로다. 어느 아이템이고 왜인지는 ARMOUR_FIXES 주석 + `HERO2_ITEM_DB.md` §6.

★ 2026-09-23 — 야수진화제 판매 (#22 의 절반). `itm/22` 의 백·흑 가격을 2,000 → 50,000 으로 올리고
  둘을 `itm/forshop` 에 **새 레코드로 넣는다**(게임은 이미 진화제를 알고 UI 어휘도 다 갖고 있다 —
  없던 것은 상점에 놓이지 않았다는 것 하나였다). PRICE_FIXES + SHOP_ADDS 주석 참조.
  ★ 2026-09-24 — **적도 판다**(사용자 지시 *"상점에 적도 팔아야해"* · *"적은 70000원이야"*):
  `itm/22[2]` 2,000 → 70,000 + forshop `0x1602`. 셋이 전역 인덱스 순으로 나란히 놓인다.

★ 2026-09-23 — 망각의 구 (#22 의 나머지 절반). `itm/21`(마법구)에 **레코드 둘을 새로 만들고**
  상점에도 올린다. ⛔ **게임은 이 아이템으로 아무것도 하지 않는다** — 초기화는 전부 호스트가 한다
  (`emulator::AccountState::start_reset`). 그래서 에셋만 먼저 넣으면 살 수는 있는데 쓸 수 없는
  아이템이 되고, 호스트 쪽과 같은 커밋에서만 의미가 있다. ITM_ADDS 주석 참조.

★ 2026-09-25 — 부활의 구. `itm/21[5]` 를 같은 방식으로 새로 만들고 상점 기타에 올린다(5,000원),
  그리고 `t/itmtp.tdf` 에 **행 64 「부활의 구 사용」** 을 덧붙인다(부활 뒤 호스트가 게임 자신의 안내
  `n.a([BII)V` 로 띄우는 문구). 부활 자체는 전부 호스트다(`emulator::Revive`) — 망각의 구와 같이
  에셋과 호스트는 한 커밋이어야 한다. ITM_ADDS · ITMTP_APPEND 주석 참조.

★ 2026-10-01 — 의상 아바타 A4. `t/itmtp.tdf` 행 65 「이안 아바타 획득」 · 66 「율리케 아바타 획득」(회차 클리어
  보상의 안내 — `emulator::ClearHook`) + 의상 카탈로그 `hero/costume.txt` 에 보조손 칸(offhand)과 직업별 이름 줄
  (여섯째 필드 = 직업). 호스트와 한 커밋.

★ 2026-10-01 — 의상 아바타 A5. 의상 셋 더(`costumes/202_claire` 기본 지급 · `203_pool` HELL · `204_halloween` HELL 난이도
  NORMAL 이상) + `t/itmtp.tdf` 행 67 「수영장 아바타 획득」 · 68 「할로윈 아바타 획득」. 호스트(`avatar_state::CLEAR_REWARDS`)와 한 커밋.
  ★ 2026-10-02 HELL 의 둘이 조건을 바꿨다(할로윈 = 난이도 무관 · 수영장 파티 = NORMAL 이상) — 호스트 표만, 에셋은 그대로.

★ 2026-09-25 — **전 주인의 세이브를 싣지 않는다.** 사용자: *"새로설치한사람들은 모두 빈세이브로 시작해야해.
  세이브는 내로컬에서 내가 원할떄만 넣어줘야해"*. 원본은 실제 폰의 덤프라 `P/` 에 그 폰 주인의 세 슬롯
  (`n1~n3` 가방 · `g1~g3` 캐릭터)이 들어 있고, 에뮬레이터는 폰에 파일이 없으면 아카이브 쪽을 읽는다
  (`FilesystemOverlay`) — 그래서 누가 새로 깔든 그 캐릭터 셋으로 시작했다. DUMP_PRIVATE 는 에셋에서 빠지고,
  **이 PC 의 `dist/test_saves/<AID>/fs/` 에만** 풀린다(`dist/` 는 gitignore). 헤드리스 기준선과 숲 전투 시험대는
  이제 `--fs dist/test_saves` 로 그 세이브를 싣는다. ⛔ `cf`(설정 + 추가데이터 설치 표시)와 `hl2List`(추가다운로드
  목록)는 남는다 — `cf` 를 빼면 게임이 데이터 다운로드 화면에서 「연결중」으로 멈춘다(DUMP_PRIVATE 주석).

★ 2026-09-26 — **장비 드랍률을 등급별로** (사용자: *"레어 이상등급의 아이템 드랍률을 올리고싶어"* → *"일반 10%, 레어 4%,
  에픽 3%, 영웅2% 로 변경해줘"*). `enm/enm` 의 장비 슬롯 폭 = 그 등급의 폭(GRADE_WIDTH). 전에는 원작 폭(5 · 0.5 · 0.1 ·
  0.05 %)의 ×2 였다. 어느 적이 어느 장비를 떨구는지(레벨 규칙)는 그대로고 확률만 바뀐다.
  그리고 **버서커 「근력단련」 설명문** `t/sk0.tdf[17]` — 효과를 스킬 데미지 증가로 바꾼 게스트 패치의 글자 절반(SK0_REPLACE).

★ 2026-09-27 — **나이트 「인내오라」 → 「신의가호」 · 로그 「민첩태세」 = 무적** (사용자: *"로그의 민첩태세를 일정시간 무적으로
  바꿔줘. 그리고 인내오라는 신의가호 라는 버프스킬로 변경하고 …"*). 효과는 게스트 패치(`guest_anchors::{blessing_aura_rework,
  agility_stance_invuln, CombatPatch}`)이고 여기는 그 글자 절반이다 — `t/sk1.tdf[8][9]` 이름·설명, `t/sk2.tdf[3]` 설명(SK1/SK2_REPLACE).

★ 2026-09-29 — **로그 「맹독감염」 = 버프 · 「독 연마」 = 중독 피해와 버프 시간** (사용자: *"맹독감염의 효과를 버프기로 바꾸고
  기본공격시 중독을 거는 효과로 …"*). 효과는 게스트 패치(`guest_anchors::VenomPatch`)이고 여기는 그 글자 절반이다 —
  `t/sk2.tdf[7]` 맹독감염 설명 · `[15]` 독 연마 설명(SK2_REPLACE).

★ 2026-09-26 — **잡은 보스를 다시 잡을 수 없게** (사용자: *"보스를 잡고나서 보스방으로 다시 들어가려고하면 보스방
  클리어 이후의 대화창이 떠야하는데 … 또 보스전에 들어갈 수있는 버그"*). 맵 파일 `P/m/m/86.m.gz`·`91.m.gz` 의 **이벤트
  레코드**를 고친다(gzip 을 풀고 다시 싼다; 다른 맵은 바이트 그대로). 형식·선택 규칙·근거는 MAP_EVENT_FIXES 주석 +
  `HERO2_GUEST_RE.md` §8.22.

★ 2026-09-30 — **의상 시트** (HERO2_AVATAR_PLAN.md A1, 사용자 결정 「새 도트」). `costumes/<번호>_<이름>/` 의 PNG 를
  `tools_re/costume_sheets.py` 가 반전 지원 시트(무압축 IDAT)로 싸서 jar 에 **새 멤버**로 더한다 — `hero/1|2|3/<번호>`.
  배포본 멤버는 그대로다. 호스트는 부팅 때 jar 목차를 읽어 거기 있는 번호만 의상으로 받으므로(`hero2java::AvatarSheets`)
  에셋을 재생성하지 않은 기계는 그 의상을 거절할 뿐 죽지 않는다. COSTUME_DATE 주석 참조.
"""

import io
import os
import struct
import sys
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import map_monsters  # noqa: E402  — 어느 맵에 어느 적이 나오는가 (드랍의 「출처」 판정, 2026-09-28)
import venom_effect
import costume_sheets  # noqa: E402  — 의상 도트 PNG → 게임 파츠 시트 (2026-09-30)
import costume_geometry
import hero_cape  # noqa: E402  — 이안의 낡은 옷의 망토: 직업 애니메이션의 망토 스프라이트 (2026-10-01)
import knight_blessing  # noqa: E402  — 신의가호 중 나이트의 둔기강타 · 검기 그림 (2026-10-01)
import knight_iaido  # noqa: E402  — 나이트 「발도」(부활오라 자리) 그림·아이콘 + 원형 검기 (2026-10-03)
import rogue_ilpung  # noqa: E402  — 로그 「일풍격」(a.p[15], 시퀀스 19) 그림·아이콘 (2026-10-05)
import rogue_slaughter  # noqa: E402  — 로그 「살상연희」(번개난타 자리 a.p[10], 시퀀스 14 · 베기 20) 그림·아이콘 (2026-10-06)
import berserker_taunt  # noqa: E402  — 버서커 「도발」(a.p[15], 시퀀스 19 = 보라색 사자후) 그림·아이콘 (2026-10-07)
import awakenings  # noqa: E402
import rogue_amsal  # noqa: E402  — 로그 패시브 「암살」(a.p[17]) 아이콘: img/gmico 로그 띠 칸 12 (2026-10-09 (11))
import guardian_gear  # noqa: E402
import skill_gear  # noqa: E402  — 신규 아이템 2차: 스킬 특화 장비 39종 · 1차 가디언 장비의 옵션 문구 (2026-10-14)
import raid_gate  # noqa: E402  — 신전 보스 레이드: 귀환문 변형 신전(맵 112) · 단계 맵 113..121(2026-10-17) · 붉은 불꽃 obj 65..68 (2026-10-16)
import elixir_icon  # noqa: E402  — 엘릭서 자기 아이콘: img/glb 17번 띠 칸 34 (2026-10-16, ItemIconPatch 가 그 칸을 그린다)

# ★★ 2026-10-14 신규 아이템 2차 (HERO2_NEW_ITEM_DRAFT.md): 2차 38종(★★ 2026-10-15 E2 삭제) + ★★ 번개의 호흡 = 39 · 조합서 35(itm/23 92..126) ·
#   가디언 칸 16 · 17 · 장비 옵션 문구(가디언 28 + 2차) · itmtp 75 · 76. 효과가 다 들어가기 전까지는 플래그 `--skill-gear` 뒤에 있었다 —
#   ★★ 2026-10-15 H1~H3 로 효과가 전부 들어가 **기본으로 켠다**. `--no-skill-gear`(또는 환경 HERO2_SKILL_GEAR=0)면 예전 에셋(바이트 그대로).
SKILL_GEAR = "--no-skill-gear" not in sys.argv[1:] and os.environ.get("HERO2_SKILL_GEAR") != "0"
import demon_look  # noqa: E402  — 「악마화」 D2: 악의의 외형 — 몸통 시트 300 · 악마 시퀀스 · bsk 문구 · bico 아이콘 (2026-10-13)  — 「가디언」 모드 신규 장비 22종: 무기 시트 10 + 수치표 (2026-10-08)  — 각성기 셋: 버서커 「격분」(시퀀스 20·21·22) · 나이트 「천공의검」(25) · 로그 「개화」(21·22) 그림·아이콘 (2026-10-07 (2))
import demon_skin  # noqa: E402  -- 2026-10-16: the demon versions of six beast skills (bsk rows 26..37, bico 13..18)

# ── 망토 (2026-10-01, 사용자 「이안의 낡은 옷에 망토도 같이 포함」 — `hero_cape.py` 머리말) ─────────────────────
# 직업 애니메이션 셋(`hero/hero0..2`)의 모든 포즈가 몸통 파트의 프레임 5 · 6 · 7(망토) · 8(스카프)을 그린다. 그림은
# 이안의 낡은 옷(`costumes/200_ian/body_5..8.png`)에만 있고, 나머지 몸통 시트는 그 넷이 1×1 빈 칸 — ⛔ 게임은 배열 밖
# 프레임을 그리려 하면 던지므로 **jar 의 모든 `hero/1/*` 시트가 9 프레임**이어야 한다(아래 검사가 확인한다).
CAPE_ANIMS = ("hero/hero0", "hero/hero1", "hero/hero2")
BODY_DIR = "hero/1/"

# ── 의상 시트 (2026-09-30, HERO2_AVATAR_PLAN.md §8.1 · A1 「새 도트」) ─────────────────────────
# `costumes/<번호>_<이름>/` 의 PNG 를 `costume_sheets.costume_members()` 가 게임 시트로 싸고, jar 에 **새 멤버**로 더한다
# (`hero/1/<번호>.mph` + `_0.mpd` · 팔 hero/2 · 다리 hero/3). 배포본 멤버는 한 바이트도 안 바뀐다 — 번호가 배포본과
# 겹치면 거절한다(로더는 번호를 경로에 붙이므로 같은 번호 = 같은 파일). 호스트는 jar 의 목차를 부팅 때 읽어 거기 있는
# 번호만 의상으로 받는다(`hero2java::AvatarSheets`) — 그래서 여기 의상을 더해도 코드 표를 고칠 일이 없다.
# 날짜를 고정해 둔다: 같은 PNG 면 같은 jar 바이트(에셋 재생성이 결정적이어야 [heap] 기준선 비교가 된다).
COSTUME_DATE = (2026, 9, 30, 0, 0, 0)

KEEP_TOP = {"__adf__"}
JAR_SUFFIX = ".jar"
# The dump owner's three save slots (module doc, 2026-09-25): NOT shipped — a fresh install
# must start with empty slots. They are unpacked to TEST_SAVES instead, for the headless probe
# (`--fs dist/test_saves`).
# ⛔ `P/cf` STAYS. It is the options file, and it is also how the game knows the add-on data
# (`P/m`, `P/snd`, `hl2List`) is already on the phone: without it the game writes its own
# default `cf` (`5f 20 …`) and goes to 「게임 데이터 다운로드를 시작합니다」 → DOWNLOADING 0 % /
# 연결중 forever (the server is gone). Every phone that has the data carries `?? 32` in byte 1
# (the dump `3f 32`, the S9 `9f 32`); a fresh game writes `20` — the 0x12 bits are that state.
DUMP_PRIVATE = ("P/n1", "P/n2", "P/n3", "P/g1", "P/g2", "P/g3")
TEST_SAVES = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "dist", "test_saves")

# `t/gm.tdf` rewrites: index -> new EUC-KR text. Appended rows follow the last shipped
# row (78 rows shipped, so the first appended row is 78).
GM_TDF = "t/gm.tdf"
GM_REPLACE = {11: "ACCOUNT", 19: "계정창고"}
# 78·79 = #13 의 이동 행 · 80·81 = #22 의 망각의 구 행. 게임은 상수 인덱스로만 읽으므로
# 뒤에 붙는 행은 아무도 안 읽고, 호스트가 `h2_gm_row_handle` 로 팝업 라벨에 쓴다.
# ★ 「사용하기」(행 30)를 쓰지 않는 이유: 이 행은 되돌릴 수 없는 일을 하므로 무엇을
#   하는지 라벨이 말해야 한다. 오발을 막는 가장 싼 방법이고, 게임 자신의 예/아니오
#   상자를 짓는 것보다 확실하다.
# ★ 82~89 (2026-09-26, 사용자: *"장착중인 장비는 계정창고에 들어가면 안돼 … 안내팝업은 항상 기존 게임
#   팝업으로 만들어줘. 내가 직접그린 안내토스트문구가 뜨는게아니라"*): 창고·초기화의 **안내 문구**. 호스트가
#   이 행의 `byte[]` 를 게임 자신의 메시지 상자 `new ba(창, 문구)` 에 넘기고(`JavaCall::MessageBoxNew`),
#   게임이 자기 글꼴·자기 상자로 그린다 — 게임이 인벤에서 gm[55] 「장착중인 아이템은 버릴수 없습니다.」를
#   띄우는 것과 같은 길이다(`q.a(BB)V` @0x146a22). 말투는 게임 자신의 문구를 따랐다: 82 는
#   `shop.tdf[25]` 「장착중인 장비는 판매할 수 없습니다.」, 83·84 는 `shop.tdf[20/21]` 「구입/판매하였습니다.」.
#   「빈공간이 없습니다.」는 새로 넣지 않는다 — 게임의 것(`itmtp.tdf[7]`)을 그대로 쓴다.
GM_APPEND = [
    "창고에넣기", "가방에넣기", "스탯초기화", "스킬초기화",
    "장착중인 장비는 창고에 넣을 수 없습니다.",  # 82
    "창고에 넣었습니다.",  # 83
    "가방에 넣었습니다.",  # 84
    "스탯이 초기화되었습니다.",  # 85
    "스킬이 초기화되었습니다.",  # 86
    "되돌릴 포인트가 없습니다.",  # 87
    "초기화할 수 없습니다.",  # 88
    "계정창고를 사용할 수 없습니다.",  # 89
    # ★★ 2026-10-16 엘릭서: 자기 「사용하기」 행(30 과 같은 글자, 다른 행 — 팝업 리더가 호스트 행을 게임 행과 가른다) · 가득 찼을 때의 상자
    "사용하기",  # 90 = Hero2Java::GM_ROW_USE_ELIXIR
    "HP와 SP가 가득합니다.",  # 91 = GM_ROW_NOTE_VITALS_FULL
]
GM_APPEND += ["윈터하츠", "언더시티", "실바나르", "노르덴항", "신드라프", "서리골짜기마을", "방문한 마을이 없습니다."]
GM_SHIPPED_ROWS = 78

# ── `t/com.tdf` — 타이틀 캐러셀에서 네트워크창고가 빠진다 (2026-09-24 사용자 지시) ──
# 사용자: *"메인 메뉴내에서 네트워크 창고라는 메뉴를 없애줘"*. 캐러셀의 라벨은
# `com.tdf[커서]` 직접 조회(`t.a(II)V` @0x149ab4)이고, 정지 수(7→6)와 OK 점프 테이블
# (4·5 를 한 칸씩 당김)은 게스트 패치 `guest_anchors::title_menu_no_network` 가 한다 —
# **이 두 행이 그 패치의 라벨 절반이다**: 정지 4 가 정보이용료 핸들러를, 5 가 게임종료
# 핸들러를 물려받으므로 라벨도 같이 내려온다. [6] 게임종료는 그대로 남는다(6칸 캐러셀이
# 도달하지 못하고, 다른 독자도 없다 — com 표 상수 독자 39곳 전수 조사: #5 는 정보이용료
# 화면 `bh` 의 헤더뿐이고 그 인덱스는 게스트 패치가 #4 로 옮긴다. HERO2_GUEST_RE.md §8.12).
COM_TDF = "t/com.tdf"
COM_SHIPPED_ROWS = 56
# index -> (배포본 텍스트, 새 텍스트). 현 값을 확인하고 쓴다 — #19 의 규율 그대로.
COM_REPLACE = {4: ("네트워크창고", "정보이용료"), 5: ("정보이용료", "게임종료")}
# ★ 2026-10-08 가디언 (HERO2_GUARDIAN_PLAN.md §2.7, guest_anchors::GuestBuild::guardian_labels): the restart notice `n.o()V`
#   now draws com[7 + min(ag.i, 2)] (was com[ag.i > 0 ? 9 : 8]) → [7] 하드코어 모드로 · [8] 헬 모드로 · [9] 가디언 모드로; the slot
#   screen draws com[10 + mode] with mode up to 3 → [13] GUARDIAN (the game's font is upper-case only, like HARDCORE); the
#   정보이용료 fee text that was [13] moves to a NEW row [56] and its one reader `bh.a(II)V` follows (`movs r2,#0xd → #0x38`).
#   Readers of 7 · 8 · 9 by constant: none but n.o()V; of 13: none but bh.a (the 39 com readers walked 2026-10-08).
#   ⛔ Asset and code ship together — see the halfword table's note.
COM_GUARDIAN = {7: ("", "하드코어 모드로"), 8: ("하드코어 모드로", "헬 모드로"), 9: ("헬 모드로", "가디언 모드로")}
COM_FEE_ROW = 13  # moves to COM_FEE_NEW_ROW, replaced by the mode label
COM_FEE_NEW_ROW = 56
COM_MODE3_LABEL = "GUARDIAN"

# ── `t/itmtp.tdf` — 부활의 구 안내 한 줄 (2026-09-25) ────────────────────────────
# 이 표(`cf.c:Li;` @0x19466c)는 부팅 때 한 번 읽혀 **실행 내내 살아 있는** 유일한 텍스트 표다 —
# gm 표는 메뉴를 열 때마다 새로 만들어지고 닫으면 null 이 된다(`bw.a(boolean)`). 부활은 필드에서
# 일어나므로 안내 문구의 `byte[]` 가 필드에서 살아 있어야 하고, 그래서 여기에 덧붙인다.
# 게임은 이 표를 상수 인덱스로만 읽는다: 타입 이름 0..34 · 등급 35+ · 라벨 39..58 · 세트 59..63
# (`58 + 세트` 가 최대 63) — 64 는 아무도 안 읽는다. 호스트가 `h2_revive_text` 로 행 64 의 핸들을
# 집어 `n.a([BII)V` 에 준다(`빈공간이 없습니다.` 가 행 7 로 가는 길과 같다).
# ⚠ 안내 그림은 96×16 이다 — 글자폭 1 + 9·음절 + 6·ASCII 가 96 을 넘으면 잘린다(이 문구는 52).
# ★ 2026-10-01 의상 아바타 A4: 행 65 · 66 = 회차 클리어 보상의 안내(NORMAL 이안 · HARDCORE 율리케 — 호스트의
# `avatar_state::CLEAR_REWARDS` 가 이 행 번호를 쓴다; 76 · 85 px). ⛔ 행을 더하거나 옮기면 그 표도 같이.
ITMTP_TDF = "t/itmtp.tdf"
ITMTP_SHIPPED_ROWS = 64
# 64 부활의 구 · 65/66 = A4 회차 클리어 보상 안내(이안 · 율리케) · ★ 2026-10-01 67/68 = HELL 클리어 보상(수영장 파티 · 할로윈 —
# 「수영장 파티 아바타 획득」은 109 px 라 96 px 안내 그림에 안 들어가서 「수영장」). 순서 = `avatar_state::CLEAR_REWARDS` 의 notice_row.
# ★ 2026-10-06: 69 「오딘 아바타 획득」 — 의상 205 할로윈 파티(HELL 클리어 · 난이도 HARD 이상, avatar_state::CLEAR_REWARDS).
# ★ 2026-10-08: 70 「제복 아바타 획득」 — 의상 206 제복(가디언 클리어 · 난이도 EASY 이상, avatar_state::CLEAR_REWARDS).
# ★ 2026-10-09: 71 「진홍 아바타 획득」(207 진홍의 검사 · 가디언 EASY 이상) · 72 「해머 아바타 획득」(208 순백의 해머 · 가디언 NORMAL
#   이상) · 73 「엘프 아바타 획득」(209 달빛 엘프 · 가디언 HARD 이상) · 74 「사신 아바타 획득」(210 백은의 사신 · HELL HARD 이상) — 사용자가
#   준 그림 넷, 이름은 Claude 가 정했다(costumes/README.md).
ITMTP_APPEND = [
    "부활의 구 사용", "이안 아바타 획득", "율리케 아바타 획득", "수영장 아바타 획득", "할로윈 아바타 획득", "오딘 아바타 획득", "제복 아바타 획득",
    "진홍 아바타 획득", "해머 아바타 획득", "엘프 아바타 획득", "사신 아바타 획득",
]
# ★★ 2026-10-14 75 「옵션>」 · 76 「<정보」 — 상세창의 장비 옵션 페이지 안내(AccessoryPatch::GEAR_HINT_OPEN_ROW / GEAR_HINT_BACK_ROW,
#   정보창 오른쪽 끝에 맞춰 그린다). 「▶」 는 게임 글꼴에 없다(ASCII 대문자 · 기호와 한글 음절뿐).
if SKILL_GEAR:
    ITMTP_APPEND = ITMTP_APPEND + ["옵션>", "<정보"]
# ★★ 2026-10-18 77 「재료가방이 가득 참」 — 줍기 거절 안내의 재료가방 판(사용자: 인벤토리에 빈 칸이 있는데 「빈공간이 없습니다.」가
#   떠서 안 주워진다 = 조합서 · 조합 재료(23 · 30 · 31 · 32)는 재료가방으로 가고 그 가방이 찼다). 게임의 거절 안내 `movs r2,#7`
#   @0x112600 을 호스트가 OK 순간에 이 행으로 바꾼다(`guest_anchors::BagFullRowPatch`, 85 px). 호스트는 행의 바이트를 확인하고 쓴다 —
#   SKILL_GEAR 가 꺼진 에셋에서는 이 줄이 75 에 앉아 77 이 없으므로 호스트는 늘 행 7 을 쓴다(안전한 쪽).
ITMTP_APPEND = ITMTP_APPEND + ["재료가방이 가득 참"]
assert all(1 + sum(9 if ord(c) > 0x7F else 6 for c in t) <= 96 for t in ITMTP_APPEND), "an itmtp notice wider than the 96-px image"

# ── `t/shop.tdf` — 일괄판매의 확인 상자 세 줄 (2026-10-16) ─────────────────────────────────────────
# 사용자: *"에픽 이상을 선택하면 높은 가치의 아이템 등급이 선택되었습니다. 정말 판매하시겠습니까? 팝업창이 뜨고"*. 호스트가 게임의
# 예/아니오 상자(`d` kind 10)를 판매 창에 띄울 때 그 줄들이다 — `guest_anchors::BulkSell::SHOP_ROWS_ASK` 26 · 27 · 28. 이 표(`v.a:Li;`)는
# 상점이 열려 있는 동안만 산다(`v.a()Lv;` 가 싣고 `v.c()V` 가 비운다) — 상자도 그때만 뜬다. 게임은 0..25 를 상수로만 읽는다.
# ⚠ 상자는 117 px 이고 줄을 접지 않는다: 한 줄 = 1 + 9·음절 + 6·ASCII ≤ 105.
SHOP_TDF = "t/shop.tdf"
SHOP_SHIPPED_ROWS = 26
# 29 = 그 등급에 팔 것이 없을 때의 안내 — 게임 자신의 23 「판매할 아이템이 없습니다.」(103 px)는 메시지 상자(`ba`, 101 px 에서 접는다)에서
# 마지막 음절 하나가 둘째 줄로 떨어진다. 같은 말을 게임의 줄바꿈 `;` 로 두 줄로(`guest_anchors::BulkSell::SHOP_ROW_NONE`).
SHOP_APPEND = ["높은 가치의 아이템", "등급이 선택되었습니다.", "정말 판매하시겠습니까?", "판매할 아이템이;없습니다."]
assert all(1 + sum(9 if ord(c) > 0x7F else 6 for c in t) <= 105 for t in SHOP_APPEND[:3]), "a confirm line wider than the 117-px box"
assert all(1 + sum(9 if ord(c) > 0x7F else 6 for c in line) <= 101 for line in SHOP_APPEND[3].split(";")), "a notice line wider than the box wraps"


def patch_shop(rows):
    """`t/shop.tdf` 뒤에 SHOP_APPEND 를 붙인다. 배포본인지 행 수와 21 · 23 으로 확인한다."""
    if len(rows) != SHOP_SHIPPED_ROWS:
        raise SystemExit(f"{SHOP_TDF}: {len(rows)} rows, expected {SHOP_SHIPPED_ROWS} (KTF 1.0.7)")
    if rows[21].decode("euc-kr") != "판매하였습니다." or rows[23].decode("euc-kr") != "판매할 아이템이 없습니다.":
        raise SystemExit(f"{SHOP_TDF}: rows 21/23 are not the shipped 판매하였습니다./판매할 아이템이 없습니다.")
    return list(rows) + [t.encode("euc-kr") for t in SHOP_APPEND]

# ── `hero/h.p` 팔레트 레코드 덧붙이기 — 의상 아바타 A1 (2026-09-30) ─────────────────────────
# 플레이어 파츠 시트(`hero/1..3/<nn>`)는 장비 레코드의 `bu.a`(팔레트 번호)로 이 파일의 레코드 하나를
# 골라 PLTE 색을 치환한다(`cf.a(Lam;SLjava/lang/String;)V` → `am.a(II)V`, HERO2_GUEST_RE.md §8.35.2).
# 형식 = `[u16 BE len][u8 n][n × (RR GG BB 00 from)(RR GG BB 00 to)]` × 189 (배포본, 걷기가 EOF 에 정확히 착지).
# 호스트의 의상 오버라이드(`guest_anchors::AvatarPatch`)는 (스프라이트, 팔레트) 쌍을 넣으므로, **도트 0장으로**
# 「기존 파츠 + 새 팔레트」 의상이 된다 — 여기 덧붙인 레코드가 그 팔레트다. 레코드 번호 = 189 + 순서이고
# 호스트는 `AvatarPatch::PALETTE_COUNT`(= HP_SHIPPED_RECORDS + len(HP_APPEND)) 아래만 쓴다: ⛔ **둘을 같이 늘린다**.
# 첫 의상 「이안의 낡은 옷」 = 로그 맨몸 시트(몸통 102 · 팔 100 · 다리 102)가 이미 NPC 이안의 색(상의 #3f3f3f·#5f5f3f·#1f1f3f,
# 카키 부츠)이라, 허리띠를 진홍(#9f1f3f)으로 · 다리의 붉은 장식을 검정으로 · 부츠를 카키로 맞추는 두 레코드만 더한다.
HP = "hero/h.p"
HP_SHIPPED_RECORDS = 189
HP_APPEND = [
    # 189: 몸통 바탕 `hero/1/102` — 붉은 허리 장식 #9f3f3f → 이안의 진홍 허리띠 #9f1f3f
    ("이안의 낡은 옷 몸통", [(0x9F3F3F, 0x9F1F3F)]),
    # 190: 다리 바탕 `hero/3/102` — 붉은 무릎 장식 → 검정 바지, 갈색 → 카키 부츠
    ("이안의 낡은 옷 다리", [(0x7F1F3F, 0x3F3F3F), (0x9F3F3F, 0x5F5F3F), (0xBF7F3F, 0x9F9F7F)]),
]


def hp_records(blob, name=HP):
    """`hero/h.p` as a list of `[(from_rgb, to_rgb), ...]` per record; the walk must land on EOF."""
    out, p = [], 0
    while p < len(blob):
        if p + 3 > len(blob):
            raise SystemExit(f"{name}: truncated record header at {p}")
        ln = blob[p] << 8 | blob[p + 1]
        n = blob[p + 2]
        if ln != 1 + 8 * n or p + 2 + ln > len(blob):
            raise SystemExit(f"{name}: record {len(out)} at {p} has len {ln} for {n} pair(s)")
        body = blob[p + 3 : p + 2 + ln]
        pairs = []
        for k in range(n):
            a, b = body[8 * k : 8 * k + 4], body[8 * k + 4 : 8 * k + 8]
            if a[3] != 0 or b[3] != 0:
                raise SystemExit(f"{name}: record {len(out)} pair {k} is not RR GG BB 00")
            pairs.append((int.from_bytes(a[:3], "big"), int.from_bytes(b[:3], "big")))
        out.append(pairs)
        p += 2 + ln
    return out


def bgr(rgb):
    """★ 2026-10-08: an `h.p` colour word is B·G·R·0 on disk (`am.a(II)V` reads it little-endian as 0x00RRGGBB), so this
    tool's record ints — the three bytes big-endian, byte-identical for the shipped 189 — hold BB GG RR. An RGB value
    goes through this before it is written. (HP_APPEND 189 · 190 were written as RGB until today: no effect in game.)"""
    return ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | (rgb >> 16)


def hp_encode(records):
    out = bytearray()
    for pairs in records:
        body = bytearray([len(pairs)])
        for a, b in pairs:
            body += a.to_bytes(3, "big") + b"\0" + b.to_bytes(3, "big") + b"\0"
        out += len(body).to_bytes(2, "big") + body
    return bytes(out)


def hp_appended(extra=()):
    """The records appended after the shipped 189, in order: HP_APPEND (RGB → file order), then `extra` (already in
    file order — `guardian_palettes`). The host's `AvatarPatch::PALETTE_COUNT` bounds what it writes."""
    out = []
    for _label, pairs in HP_APPEND:
        for a, b in pairs:
            for c in (a, b):
                if not 0 <= c <= 0xFFFFFF:
                    raise SystemExit(f"{HP}: colour {c:#x} out of range")
        out.append([(bgr(a), bgr(b)) for a, b in pairs])
    for pairs in extra:
        out.append(list(pairs))
    return out


def patch_hp(blob, extra=()):
    recs = hp_records(blob)
    if len(recs) != HP_SHIPPED_RECORDS:
        raise SystemExit(f"{HP}: {len(recs)} records, expected the shipped {HP_SHIPPED_RECORDS}")
    recs += hp_appended(extra)
    new = hp_encode(recs)
    assert new[: len(blob)] == blob, "h.p: the shipped records are appended to, never rewritten"
    return new

# ── `t/sk0.tdf` — 버서커 「근력단련」의 설명문 (2026-09-26) ─────────────────────────
# 사용자: *"콤보스킬이 sp를 소모하지 않으면서 버서커의 근력단련스킬이 리뉴얼이 필요할거같아"* →
# *"스킬데미지 증가로 바꿀수도있어?"* → *"근력단력 스킬데미지 증가를 레벨당 3%로 바꿔줘"* · *"설명도 간단하게
# "스킬공격력 증가"로 변경해줘"*. 원래 효과(기본공격 1타 SP −7/레벨)는 #16 뒤로 **획득량**을 깎는
# 벌칙이 됐다. 새 효과는 게스트 패치 둘이다 — `guest_anchors::strength_training_sp_off`(SP 항 × 0) +
# `SkillPowerPatch` 의 버서커 블록(`a.p[12]` 레벨당 스킬 데미지 +3 %). **이 행이 그 패치의 글자 절반이다.**
# 표는 이름·설명 쌍(짝수 = 이름, 홀수 = 설명)이고 [16] 근력단련 · [17] 그 설명(= sk0 스킬 8, a.p[4 + 8]).
# 마크업(`{…@` 강조색 · `$…@` 다른 색)은 쓰지 않는다 — 사용자가 준 문구 그대로이고, 나이트 패시브(「체력단련 —
# 최대 HP 증가」 · 「오라수련 — 오라 효과 증가」)도 평문이다.
SK0_TDF = "t/sk0.tdf"
SK0_SHIPPED_ROWS = 22
# index -> (배포본 텍스트, 새 텍스트). 현 값을 확인하고 쓴다 — COM_REPLACE 와 같은 규율.
SK0_REPLACE = {
    17: ("콤보 공격시 $SP@ 소모량 감소", "스킬공격력 증가"),
    # ★ 2026-10-07 (사용자: *"분노태세를 on off토글형 스킬로 … 1lv기준 받는피해 20%증가, 주는피해 20증가 … 5lv 기준 받는피해 10%
    #   증가, 주는피해 30%증가"*): [7] = sk0 스킬 3 = `a.p[7]` 분노태세의 설명 — 배포본(공격력 ×2 · 방어도 ×3/5, 시간제)을 끄고 켜는
    #   태세로(`guest_anchors::BerserkerPatch` · `rage_shipped_off`). 배포본의 `{…@`(강조) · `$…@`(다른 색) 꼴을 그대로 — 좋은 쪽 강조,
    #   나쁜 쪽 다른 색. 한 줄 78 px(한글 9 · 공백·쉼표 6, 펜 1 부터): 67 · 73 · 73.
    7: ("{공격력@이 {상승@하고,$방어도@가 $하락@한다.", "켜고 끄는 태세;{주는 피해@ 증가,;$받는 피해@ 증가."),
}
# ★ 2026-10-07 「도발」: 버서커의 8번째 액티브 = `a.p[15]`(guest_anchors::BerserkerPatch) — 상세 창 `cj` 는 `2·(15 − 4)` = 행
#   **22 · 23** 을 이름 · 설명으로 읽는다(스킬 창의 칸 그리기도 같은 인덱스 11). 이름 = 사용자, 설명 = Claude(세 줄 76 px 이하).
SK0_APPEND = [
    "도발",
    "맵의 모든 적을;도발해 끌어온다.;{적이 약해진다@",
    # ★ 2026-10-07 (2) 각성기 「격분」: 버서커의 `a.p[16]` → 상세 창 `cj` = `2·(16 − 4)` = 행 **24 · 25**. 이름 = 사용자, 설명 = Claude
    #   (세 줄 78 px 이하: 69 · 75 · 57).
    "격분",
    "타격으로 분노를;모아 폭발시킨다.;{맵 전체 피해@",
]

# ── `t/sk1.tdf` · `t/sk2.tdf` — 신의가호 · 민첩태세 (2026-09-27) ──────────────────────
# 나이트 [8][9] = sk1 스킬 4 = `a.p[3 + 4]` 인내오라 → 「신의가호」: 버프가 켜져 있는 동안 검기 3방향 · 둔기강타 ×2 ·
# 방패치기 앞 2줄 6칸 ×1.5, 원래의 HP·SP 최대량 증가는 없앴다. 로그 [3] = sk2 스킬 1 = `a.p[5 + 1]` 민첩태세의 설명 —
# 민첩 상승을 없애고 무적(Lv1 2초 → Lv5 4초)으로 바꿨다. 로그 설명문은 사용자에게 보인 문구 그대로(평문, SK0 와 같은 규율) —
# 줄바꿈 `;` 하나만 더했다(없으면 「무적 상 / 태가 된다.」 로 갈라진다).
# 신의가호 설명 = 사용자 문구 그대로 — 셋째 판(같은 날 밤 *"여전히 신의가호 스킬 설명이 박스를 넘어서고 있어. 문구를 그냥 "일정시간
# 공격스킬이 강화됩니다" 로 변경해줘"*; 둘째 판 「일정시간 검기 둔기강타 방패치기 스킬이 강화된다.」 는 네 줄이라 기기에서 상자 밑으로
# 넘쳤다) + 줄바꿈 `;` 둘. `;` 는 상세 화면의 줄바꿈이다(배포본 [3] 「검기를 날려 피해를 준다.;{도검 필요@」). 상세 화면은 한 줄 ≈ 7음절 +
# 공백 둘에서 **단어 중간이라도** 자른다(첫 판 「검기 둔기강타 방패치기」 가 「방 / 패치기」 로 갈라졌다, 헤드리스 실측) → 단어 경계에서
# 세 줄: 일정시간 / 공격스킬이 / 강화됩니다. ⛔ **상자에 들어가는 설명은 세 줄까지**다(배포본 설명은 전부 두 줄 이하).
SK1_TDF = "t/sk1.tdf"
SK1_SHIPPED_ROWS = 24
# ★ 2026-10-03: [12·13] = sk1 스킬 6 = `a.p[9]` 부활오라(네트워크용 — 시전 case 도, 여는 스크립트도 없고 게임이 스킬 창에서
#   숨겼다)의 자리가 「발도」다(`guest_anchors::CombatPatch::IAIDO`, HERO2_GUEST_RE.md §8.45). 한 줄 ≈ 7음절, `;` 가 줄바꿈.
# ★ 2026-10-05: [10·11] = sk1 스킬 5 = `a.p[8]` 방어의지 → 「철괴」(사용자 이름) — 2~4초 모든 타격 막기 대신 받는 피해 감소
#   (Lv1 10 % … Lv5 20 %, 오라수련 레벨당 +1 %), 20~40초 + 오라수련 레벨당 4초, 쿨 60초, 공격해도 안 풀림
#   (`guest_anchors::GuestBuild::iron_wall_rework` + `CombatPatch::IRON_AT`). 신의가호와 같은 세 줄 꼴.
SK1_REPLACE = {
    # ★ 2026-10-12: 치유오라 · 철괴가 지력에 비례한다(guest_anchors::CombatPatch::{heal_amount, IRON_CUT_PER_INT}) — 줄 끝에 「(지력 비례)」
    #   (상세 창 한 줄 78 px: 「{HP@를 회복한다.」 69 · 「일정시간 받는」 60 · 「피해가 감소」 51 · 「(지력 비례)」 54).
    1: ("{HP@를 일정량 회복한다.", "{HP@를 회복한다.;(지력 비례)"),
    8: ("인내오라", "신의가호"),
    9: ("일정시간 {HP@,$SP@의 최대량을 증가시킨다.", "공격스킬 강화;명중률 +20%P;일정시간 유지"),
    10: ("방어의지", "철괴"),
    11: ("일정시간 극단적인 {방어 태세@를 취한다.", "일정시간 받는;피해가 감소;(지력 비례)"),
    12: ("부활오라", "발도"),
    13: ("파티원을 부활시킨다.(네트워크용)", "검을 뽑아 주위의;적을 모두 벤다."),
}
SK2_TDF = "t/sk2.tdf"
SK2_SHIPPED_ROWS = 20
# 2026-09-29: [7] = sk2 스킬 3 = `a.p[5 + 3]` 맹독감염의 설명 — 적 하나를 찔러 중독시키던 스킬이 버프(기본공격이 중독을 건다)가
# 됐다(`guest_anchors::VenomPatch`). [15] = sk2 스킬 7 = `a.p[5 + 7]` 독 연마의 설명 — 배포본에서는 아무 코드도 그 레벨을
# 읽지 않았다(「중독 시간 증가」는 빈 약속이었다, 헤드리스 A/B), 이제 중독 피해와 버프 시간을 올린다. 배포본 문구의 `{중독@`
# 강조를 그대로 두고, 줄은 단어 경계에서 `;` 로 나눴다(한 줄 ≈ 7음절, 세 줄까지 — 신의가호 절 참조).
# ★ 2026-10-06 (사용자: *"독연마 패시브 스킬 효과를 맹독 액티브 스킬에 바로 추가 … 독연마 스킬을 동체시력 1레벨 자동 획득 패시브
#   스킬로 변경 … 맵 내의 투명한 몬스터를 확인 할 수있는 스킬"* → 이름은 「다른 이름」(배포본 [12] 동체시력 = 회피율이 따로 있다) =
#   「심안」): [14·15] = sk2 스킬 7 = `a.p[12]` 독 연마 → 「심안」(로그는 Lv1 로 시작 — `hero2java::h2_born_skill_repair`, 상한 1 —
#   `IlpungPatch::SIGHT_SLOT`, 효과 = `guest_anchors::RoguePatch` 의 반투명 그리기). 독 연마의 효과는 맹독감염이 늘 최고 단계로 갖는다
#   (`VenomPatch::buff_frames_mastered`) — [7] 의 문구는 그대로.
#   [10·11] = sk2 스킬 5 = `a.p[10]` 번개난타 → 「살상연희」(사용자: *"번개난타를 … 살상연희 스킬로 바꾸고싶어"* — 주변 5×5 의 가까운
#   적 뒤로 순간이동하며 벤다, `guest_anchors::RoguePatch`). 쓸 수 있는 무기는 배포본 규칙 그대로(주무기 + 보조단검; 로그의 주무기 칸은
#   도검·단검 — 사용자가 물은 「도검,단검 조합」도 된다) → 게임 자신의 이름 「보조단검」으로 적었다.
#   한 줄 78 px(상세 창 `cj.a(II)V` 의 `ac.a(…, 78, 1, 글)`, 한글 9 · ASCII/공백 6, 펜 1 부터): 58 · 76 · 67.
SK2_REPLACE = {
    3: ("일정 시간 {민첩@을 상승시킨다.", "일정시간 무적;상태가 된다."),
    7: ("적에게 {중독@을 건다.", "일정시간;기본공격이;{중독@을 건다."),
    10: ("번개난타", "살상연희"),
    11: ("번개처럼 연속 공격.;{쌍수 무기@ 필요.", "주변 적 뒤로;순간이동해 벤다.;{보조단검@ 필요."),
    14: ("독 연마", "심안"),
    15: ("{중독@ 시간 증가.", "{투명한 몬스터@를;볼 수 있다."),
}

# ★ 2026-10-02 스킬을 바꾸는 장신구: 벽력일섬(일격 → 일섬) · 극사의 목걸이(급소가격 → 극사)를 끼면 메뉴의 스킬 표
#   (`bw.c`, 메뉴를 열 때 이 파일에서 만든다)의 행 둘을 호스트가 이 덧붙인 행으로 가리킨다(`guest_anchors::AccessoryPatch::
#   SKILL_NAME_SWAPS`, `emulator::Accessory::poll_names`). 끼지 않으면 배포본 행 그대로다. 행 수가 늘지만
#   게임은 스킬 수를 `a.j`(정적) 로 세므로 표의 꼬리 행은 아무도 읽지 않는다 — gm.tdf 의 78·79 와 같다. 한 줄 ≈ 7음절, `;` 가 줄바꿈.
# ★ 2026-10-05 일풍격: 로그의 7번째 액티브 = `a.p[15]`(guest_anchors::IlpungPatch) 이고 상세 창 `cj` 는 `2·(인덱스 − 콤보 수)` =
#   행 **20 · 21** 을 이름 · 설명으로 읽는다 — 그 자리를 일풍격이 갖고, 일섬 · 극사는 22·23 / 24·25 로 한 칸씩 밀렸다
#   (SKILL_NAME_SWAPS 도 같이). 표의 꼬리 행이 처음으로 「읽히는」 행이 됐다.
SK2_APPEND = [
    "일풍격",
    "가까운 적에게;이동해 회전하며;단검을 던진다.",
    # ★ 2026-10-07 (2) 각성기 「개화」: 로그의 `a.p[16]` → 상세 창 `cj` = `2·(16 − 5)` = 행 **22 · 23** — 일섬 · 극사는 24..27 로
    #   한 칸 더 밀렸다(`AccessoryPatch::SKILL_NAME_SWAPS` 도 같이). 설명 = Claude (51 · 69 · 57 px).
    "개화",
    "꽃을 피우고;주변 적을 30번;베어 넘긴다.",
    # ★★ 2026-10-09 (11) 로그 패시브 「암살」(사용자: *"상대방에게 가하는 첫번째 공격이 100% 방어 무시 추가 피해를 입히는 패시브 … 5lv까지
    #   찍으면 20%부터 100%까지"*): 로그의 `a.p[17]` → 상세 창 `cj` = `2·(17 − 5)` = 행 **24 · 25** — 일섬 · 극사는 26..29 로 한 칸 더
    #   밀렸다(`AccessoryPatch::SKILL_NAME_SWAPS` 도 같이). 이름 = 사용자, 설명 = Claude (48 · 42 · 75 px — 한 줄 78 px).
    "암살",
    "첫 공격 시;{방어 무시@;추가 피해를 준다",
    "일섬",
    "앞으로 5칸 돌진;지나간 적을;모두 벤다.",
    "극사",
    "가장 가까운 적;뒤로 순간이동;급소를 찌른다.",
    # ★★ 2026-10-17 「벽력일섬」(사용자: *"번개의 호흡과 벽력일섬 아이템 2개를 장착하면 일섬이 벽력일섬으로 변경되며 기존 일섬 데미지 +
    #   번개데미지(번개 데미지는 일섬 데미지의 1.5배)로"*): 벽력일섬 + 번개의 호흡을 같이 끼면 일격 칸이 이 행 **30 · 31** 을 가리킨다
    #   (`AccessoryPatch::SKILL_NAME_SWAPS` 의 `ILSEOM_BOLT_NAMES`). 이름 = 사용자, 설명 = Claude (한 줄 78 px: 73 · 61 · 76).
    "벽력일섬",
    "5칸 돌진해 베고;{번개@가 내리쳐;1.5배 추가 피해",
]

# 스킬 표 셋 = (배포본 행 수, 바꿀 행, 덧붙일 행). 이름·설명 쌍(짝수 = 이름, 홀수 = 설명)이고 배포본 행은 제자리다.
# ★ 2026-10-07 (2) 각성기 「천공의검」: 나이트의 `a.p[15]` → 상세 창 `cj` = `2·(15 − 3)` = 행 **24 · 25** (배포본 24 행 바로 뒤).
#   이름 = 사용자, 설명 = Claude (60 · 75 · 48 px).
SK1_APPEND = [
    "천공의검",
    "공간을 세 갈래로;갈라 깨뜨리고;폭발시킨다.",  # ★ 2026-10-08: three prongs in turn · the break · the blast (five hits)
]

SKILL_TEXT = {
    SK0_TDF: (SK0_SHIPPED_ROWS, SK0_REPLACE, SK0_APPEND),
    SK1_TDF: (SK1_SHIPPED_ROWS, SK1_REPLACE, SK1_APPEND),
    SK2_TDF: (SK2_SHIPPED_ROWS, SK2_REPLACE, SK2_APPEND),
}

# ── `itm/NN` 방어구의 경갑/중갑 한 바이트 (#19) ──────────────────────────────
# 레코드 = [u16 BE len][u32 LE price][u8 nameLen][name EUC-KR][body], 파일 = 그 배열
# (헤더 없음, EOF 에 정확히 착지). 방어구 body 는 28 B 이고 `body[23]` = 0 경갑 · 1 중갑
# (HERO2_ITEM_DB.md §1, `t/itmtp.tdf[56][57]`, 거절 문구 `t/gm.tdf[59]` "중갑 착용 불가").
#
# 게스트 게이트는 `cc.a(BB)V` @0x13e288 (HERO2_GUEST_RE.md §8.1): 경갑은 누구나 · 중갑은
# 나이트 무조건 · 버서커는 「중장비」 스킬 · **로그는 영구 불가**. 그래서 이 한 바이트가
# 잘못 박힌 방어구는 로그에게 영영 못 쓰는 장비가 된다 — 그것이 고치는 이유다.
#
# 고치는 셋 (2026-09-23 사용자 결정. 후보 넷과 근거는 HERO2_ITEM_DB.md §6.2):
#   장갑[50] 망령의 손길  — 갑옷·장갑·신발 51개 평행 라인 중 **유일한** 어긋남. 같은 라인의
#     갑옷 팬텀레더아머와 신발 망령의그림자는 경갑인데 장갑만 중갑이라, 로그의 최종 장비
#     라인에서 장갑만 빠진다. 구조로 증명된 유일한 건.
#   투구[3]  청동투구     — 「청동」 8개 중 혼자 경갑(나머지 7은 전부 중갑). 재질 낱말이다.
#   투구[10] 바람의 모자  — 세트 5개 중 유일한 경중 혼합. 짝인 갑옷[24] 바람의가죽갑옷은 경갑.
# ⛔ 방패 28/28 중갑은 **설계**이므로 건드리지 않는다(§6.3) — 예외가 하나도 없다는 것이 곧
#    의도이고, 나이트에게만 「방패수련」이 있는 것과 앞뒤가 맞는다.
ARMOUR_CLASS_OFF = 23
LIGHT, HEAVY = 0, 1
# (itm 타입, 인덱스) -> (이름, 배포본의 값, 새 값). 이름과 현 값을 **둘 다** 확인하고 쓴다.
ARMOUR_FIXES = {
    (8, 50): ("망령의 손길", HEAVY, LIGHT),
    (5, 3): ("청동투구", LIGHT, HEAVY),
    (5, 10): ("바람의 모자", HEAVY, LIGHT),
}
# 상점 목록은 DB 의 재직렬화다: [u8 recLen][u16 BE 전역인덱스][그 itm 레코드와 바이트 동일].
# 전역 인덱스 = (타입 << 8) | 인덱스 — 배포본 134 레코드 전부에서 성립(실측 2026-09-23).
# 고치는 셋 중 청동투구가 여기에도 있으므로 같은 바이트를 양쪽에서 뒤집는다.
ITM_SHOP = "itm/forshop"
SHOP_RECORDS = 134

# ── 망각의 구 — 없던 아이템 둘을 만든다 (#22, 2026-09-23 사용자 결정) ────────
# 사용자 지시: *"망각의 구 스탯, 망각의 구 스킬 로 2개 추가하면돼 야수진화제 흑, 백 처럼"* ·
# *"아이템구매 → 기타 항목에 추가"* · 각각 5,000원.
#
# ⛔ **새 타입 번호가 없다.** `t/itmtp.tdf` 는 `[0..34]` 가 아이템 타입이고 **`[35]` 부터
#    등급 이름**(일반/레어/에픽/영웅)이라 35번을 만들면 등급 표와 충돌한다. `[7]` 도 못 쓴다 —
#    「빈공간이 없습니다」라는 UI 문구이고 그래서 `itm/07` 이 아예 없다(이 일치가 타입표 해석의
#    교차검증이다). 그래서 **기존 타입 21 마법구를 빌린다**: 재료가 아니고(재료가방으로 안 간다)
#    회복 순환 `bq.j` 밖이고 카드도 아니고 호스트의 타입별 플레이트도 없고, 무엇보다 **게스트의
#    마법구 처리가 가방에서 도달 불가**다(팝업이 kind 3 · 2행 `[상세보기 버리기]`, 사용하기 없음 —
#    probe 실측 2026-09-23). 상세 = `guest_anchors::Hero2Java::ITEM_TYPE_ORB`.
#
# 레코드 모양은 계열 D(소모품)다: `[u32 LE price][u8 nameLen][name][u8 descLen][desc]` —
# `itm/21` 의 기존 셋(하급/중급/상급 마력구)과 같은 꼴.
#
# ★ 2026-09-25 — 부활의 구 (사용자: *"부활의 서라는 아이템도 상점에 추가할수있나? 죽으면 해당
#   아이템을 사용해서 제자리에서 부활하는거야"* → 결정 *5,000원 · HP·SP 전부 · 매번 묻기 · 파란 구슬로
#   하고 부활의 구 로 이름을 바꾸자*). 같은 마법구 타입을 빌리는 이유도 같다 — 게임이 가방에서 이
#   타입에 아무 동작도 안 하므로(사용하기 없음) 쓰러질 때 호스트가 묻는 것 말고는 이 아이템을 쓰는
#   길이 없다. 드랍 표에는 타입 21 슬롯이 하나도 없으므로(NO_DROP_TYPES, verify_enm 이 잠근다) 이 구는 떨어지지 않는다.
#
# ★ 2026-10-02 — 스킬을 바꾸는 장신구 6종 (HERO2_ACCESSORY_PLAN.md). 사용자 결정: *"드랍 조건 : 목걸이 및 반지
#   드랍테이블에 추가, 요구레벨 1, 등급 : 영웅, 능력치:없음"* + 이름·종류·설명은 사용자가 준 그대로. 레코드는 계열 C
#   (`itm/11` 목걸이 · `itm/12` 반지)인데 **설명을 싣는다** — `[u32 price][nameLen][name][descLen][desc]` 뒤에
#   ACCESSORY_TAIL(레벨 · 등급 · 세트 · 0x64 · 능력치 범위 30 B)이 온다. 게임의 기본 파서 `ch.a([BIZZ)I` 가 설명을
#   `ch.k` 에 담고 남은 자리부터 `z` 가 읽으므로(HERO2_ACCESSORY_PLAN.md §1.1·§1.3) 설명이 있어도 같은 아이템이다.
#   ⛔ 정보창이 장비의 설명을 안 그리므로 그것은 호스트가 보탠다. ⛔ 한 번 배포한 행은 영원히 둔다(§1.1).
#   가격 = 같은 종류 영웅 장신구의 가격(상점에는 안 올린다 — 판매가만 뜻이 있다, 가격의 ¼).
#   효과는 호스트(`guest_anchors::AccessoryPatch`)가 장착 칸(슬롯 2)의 `(타입, id)` 를 보고 건다.
# ★ 2026-10-03 — 요구레벨 1 → **70** (사용자: *"70으로 바꿔야해"* · 이미 받은 것은 무시). 레벨은 body[1] = 꼬리 첫 바이트다(계열 C, `HERO2_ITEM_DB.md` §1).
#   HELL 보너스 칸 8 도 레벨 규칙을 따른다 — HELL 레벨 + DROP_LEVEL_UP 이 70 에 못 미치는 적(원래 LV 12 이하, 7 마리)은 그 칸이 비었다(plan_hell_slots).
ACCESSORY_LEVEL = 70
ACCESSORY_TAIL = bytes([ACCESSORY_LEVEL, 3, 0, 0x64]) + bytes(30)  # LV 70 · 영웅 · 세트 없음 · 0x64(배포본 34 개 전부) · 능력치 없음
# ★ 2026-10-03 — 사용자: *"신규장신구 문구수정과 스탯을 추가할거야 … 바바리안의 의지 : 폭풍강타 업그레이드, 방어 +15 / 광룡의 외침 :
#   사자후 쿨타임 -50%, sp소모 -50%, 의지 +20 / 크루세이더 반지(이름도 수정해야함) : 방패치기 업그레이드, 체력 + 20 / 야수조련 목걸이 :
#   야수스킬 범위, 데미지 100% 상승, 지력 +20 / 벽력일섬 : 일격 업그레이드, 방어 +15 / 극사의 목걸이 : 급소가격 업그레이드, 민첩 + 20"*.
#   능력치는 계열 C 의 꼬리에 **min = max** 로 앉힌다(장신구 파서 `z.a([BIZZ)I` 가 쌍마다 `h.a(min, max)` 를 굴리므로 같은 값이면 고정):
#   body[5..14] = 다섯 쌍 **힘·체력·민첩·의지·지력**(`HERO2_ITEM_DB.md` §1, `itmtp[45..49]`) · [15,16] 공격 · [17,18] 방어 · [19..26] HP · [27..34] SP.
#   설명은 호스트 DESC 조각이 게임 글꼴로 그린다 — ⛔ 게임 ASCII 에 **소문자가 없다**(`ftd/eng5`: `sp` 는 빈 칸) → 「SP소모」; `% + - ,` 는 있다.
#   ★ 줄바꿈은 게임 글 상자 `ac.a(Graphics,IIII[BIII)V` 의 폭 규칙뿐이다(줄바꿈 문자 없음): 펜 1 에서 시작해 ASCII 6 · 한글 9 씩 나아가고
#   `펜 + 폭 > W`(W = `AccessoryPatch::DESC_WIDTH` **94**) 인 글자에서 줄을 바꾼다 — 그 글자가 공백이면 버린다(probe 실측). 그래서 긴 문구는
#   **쉼표 뒤 공백 수**로 줄 끝을 맞춘다(공백은 그 줄에 남고 다음 낱말이 넘쳐 새 줄로): 「야수스킬 범위,」+4 · 「SP소모-50%,」+5 · 「일격 업그레이드,」+3.
#   「사자후 쿨타임 -50%,」(97 px) 는 94 를 넘어 「쿨타임-50%」 로 붙였다. 폭을 바꾸면 이 공백 수를 다시 센다(검증 = probe 상세보기 덤프).
#   ⛔ 이름·가격·굴린 능력치는 **레코드마다 세이브에 실린다**(`ch.a()[B` · `z.a()[B`) — 이미 가진 여섯(두 폰 슬롯 1, 10-03 `save_put`)은
#   옛 이름 「크루세이더」·능력치 0 그대로이고 새로 생긴 레코드부터 새 값이다(효과는 `(타입, id)` 로 걸리므로 그대로 듣는다).
def accessory_tail(*, str_=0, con=0, dex=0, wis=0, int_=0, atk=0, def_=0, level=ACCESSORY_LEVEL, hp=0, sp=0):
    """계열 C 꼬리(body[1..34], 34 B): LV `level` · 영웅 · 세트 없음 · 0x64 · 능력치 쌍은 (v, v) 로 고정 (★ 2026-10-08 가디언: HP · SP 쌍도)."""
    for v in (str_, con, dex, wis, int_, atk, def_):
        if not 0 <= v <= 127:
            raise SystemExit(f"accessory stat {v} does not fit a signed byte")
    pairs = bytes(x for v in (str_, con, dex, wis, int_, atk, def_) for x in (v, v))
    tail = bytes([level, 3, 0, 0x64]) + pairs + struct.pack("<IIII", hp, hp, sp, sp)  # HP · SP 쌍(u32 × 4)
    assert len(tail) == len(ACCESSORY_TAIL) == 34
    return tail


ITM_ADDS = [
    (28, 1, "비행서", 2000, "방문한 마을을 선택하여 귀환", b""),
    (21, 3, "망각의 구 스탯", 5000, "스탯을 초기화", b""),
    (21, 4, "망각의 구 스킬", 5000, "스킬을 초기화", b""),
    (21, 5, "부활의 구", 5000, "사망시 제자리에서 부활", b""),
    (21, 6, "대장장이의 보증서", 500000, "제련 실패시 1장으로 장비 파괴 방지. 기존 제련도 유지.", b""),
    # ★★ 2026-10-16 엘릭서 (사용자: *"상점에 엘릭서 아이템을 추가해줘. HP:100%, SP : 100% 회복해주는 아이템이고 가격은 1200원이야"*) — 같은 타입
    #   21 을 빌린다(게임은 가방에서 이 타입에 아무 동작도 없다): 호스트가 팝업에 「사용하기」(gm 90) 행을 붙이고 HP·SP 를 최대로(JavaWrite::CharRefill),
    #   하나를 먹는다(emulator::AccountState::start_reset — 망각의 구와 같은 길). 가득 차 있으면 gm 91 상자, 아이템은 그대로. 판매가는 ¼ = 300.
    (21, 7, "엘릭서", 1200, "HP와 SP를 모두 회복", b""),
    # ★★ 2026-10-20 상점주인의 혼 (사용자: *"영구 아이템이야(한번사면 사용해도 안없어지고 계속 이용가능) … 사용하기 누르면 기존 상점 npc한테
    #   말한것처럼 아이템 구매, 아이템 판매 선택지 뜨고 … 이름으로 만들면되고 가격은 5백만 골드야"*) — 같은 타입 21: 호스트가 팝업에
    #   「사용하기」(gm 90) 행을 붙이고, 상점 재고가 비어 있으면 게임의 `z.a()` 로 채운 뒤 메뉴를 닫고 상점 이벤트 (4, 0) 을 보낸다
    #   (emulator::AccountState::start_shop). 소모 없음. 판매가 = 게임 규칙 ¼ = 1,250,000.
    #   설명의 공백 수 = 줄 끝(폭 규칙 94 px: 「휴대용 상점.」 58 · 「사용해도 사라지지」 79 · 「않는다.」 — desc_wrap 으로 확인).
    (21, 8, "상점주인의 혼", 5000000, "휴대용 상점.     사용해도 사라지지  않는다.", b""),
    (11, 17, "바바리안의 의지", 2592000, "폭풍강타 업그레이드, 방어 +15", accessory_tail(def_=15)),
    (11, 18, "야수조련 목걸이", 2592000, "야수스킬 범위,    데미지 100% 상승, 지력 +20", accessory_tail(int_=20)),
    (11, 19, "극사의 목걸이", 2592000, "급소가격 업그레이드, 민첩 +20", accessory_tail(dex=20)),
    # ★ 2026-10-12 (사용자: *"사자후 쿨타임 -50%, 사자후 SP소모량 -50% 로 수정"*) — 「사자후 SP소모량 -50%」(102 px)는 한 줄(94 px)에
    #   안 들어가서 네 줄로 끊었다: 「사자후 쿨타임 -50%」 · 「사자후 SP소모량」 · 「-50%」 · 「의지 +20」 (공백 수 = 줄 끝, 아래 GWANGRYONG_DESC_LINES 가 잠근다).
    (12, 17, "광룡의 외침", 1996500, "사자후 쿨타임 -50% 사자후 SP소모량    -50%            의지 +20", accessory_tail(wis=20)),
    (12, 18, "크루세이더 반지", 1996500, "방패치기 업그레이드, 체력 +20", accessory_tail(con=20)),
    (12, 19, "벽력일섬", 1996500, "일격 업그레이드,   방어 +15", accessory_tail(def_=15)),
]

# ★★ 2026-10-12 인장 「상급」 (사용자: *"가디언 노말 이상부터 떨어지는 인장류의 상급 을 추가할꺼야. 기존 인장과 로직은 같고 15~20 스탯
#   랜덤 부여하면돼"*). 배포본의 인장은 id 0 「… 하급」(+0~+4) · 1 「… 중급」(+2~+7) · 2 이름에 등급이 없는 「힘의인장」(+5~+10) — 그래서
#   「상급」은 비어 있던 이름이고, 새 id 3 이 그것이다(이름 꼴은 같은 타입의 하급·중급을 따른다: 체력만 띄어쓰기 없음).
#   수치는 코드가 정한다 — 연금술사 「장비 능력 부여」 `x.c()V` 가 id 로 `h.a(2·id + id/2, 3·id + 4)` 를 굴리는데 id 3 이면 7~13 이라,
#   `guest_anchors::GuestBuild::seal_top_tier` 가 id ≥ 3 을 15~20 으로, 비용(`x.b()V`, 하급 1,000 · 중급 5,000 · 「인장」 10,000)을
#   20,000 으로 한다. 가격 = 하급·중급·「인장」 1,000 · 2,000 · 3,000 의 다음 칸(판매가 ¼ = 1,000). 드랍 = 가디언 패스 칸 15(GUARDIAN_SEAL_WIDTH).
SEAL_TOP_ID = 3
SEAL_TOP_NAMES = {13: ("힘", "힘의인장 상급"), 14: ("체력", "체력의인장상급"), 15: ("민첩", "민첩의인장 상급"), 16: ("의지", "의지의인장 상급"), 17: ("지력", "지력의인장 상급")}
SEAL_TOP_ADDS = [(t, SEAL_TOP_ID, name, 4000, f"무기에 {stat} +15 ~ +20", b"") for t, (stat, name) in sorted(SEAL_TOP_NAMES.items())]
SEAL_TOP_ITEMS = frozenset((a[0], a[1]) for a in SEAL_TOP_ADDS)
ITM_ADDS = ITM_ADDS + SEAL_TOP_ADDS

# ★★ 2026-10-08 「가디언」 모드 신규 장비 22종 (HERO2_GUARDIAN_PLAN.md Q6 · Q7, 수치·이름·그림 = tools_re/guardian_gear.py) — 무기 10
#   (새 시트 `hero/6/<종류><번호>`, GUARDIAN_WEAPON_DIR) · 방어구 9(종결 시트 + 새 `h.p` 팔레트 = 레코드의 `body[17..20]`: 그 i32 는
#   「조합 기반」이 아니라 **팔레트 번호**다 — 독자가 파서 · 직렬화기 · 파츠 로더 `cf.a(I)V` 여섯뿐, −1 = 없음) · 장신구 3. 등급 영웅 ·
#   세트 0 · 명예 0 · 설명 없음(장비 레코드는 길이로 판별된다). 드랍 = HELL 블록의 가디언 패스 칸 9 · 10(GUARDIAN_SLOTS) 에서만 —
#   plan_drops · HELL 칸에서 뺀다(GUARDIAN_ITEMS). ★ 「개성 옵션」13 은 호스트 블록 `GearPatch`(2026-10-08 밤) — 레코드는 수치까지, 장신구 셋만 설명 문구.
#   ⛔ 이름·가격·굴린 능력치는 레코드마다 세이브에 실린다(ITM_ADDS 의 규율 그대로).
GUARDIAN_WEAPON_DIR = guardian_gear.WEAPON_DIR


def _gear_tail(level, grade, set_id, slots, opt, honor, sprite, palette, refine, element, heavy, *power):
    """계열 A(무기, 33 B) / B(방어구, 27 B) 꼬리 = body[1:] — decode_hero2_items.py 의 배치 그대로."""
    tail = bytes([level, grade, set_id, slots]) + bytes(x for _ in range(5) for x in opt) + bytes([honor, sprite])
    tail += struct.pack("<i", palette) + bytes([refine, element, heavy])
    return tail + b"".join(struct.pack("<I", p) for p in power)


def _gear_desc(t, idx):
    """★★ 2026-10-14 장비의 옵션 문구 — 상세창 둘째 페이지(AccessoryPatch GEAR_AT)가 그린다. 게임의 파서는 장신구와 같이 이름 뒤
    [설명 길이][설명] 을 읽는다(`ch.a([BIZZ)I`; 설명은 세이브에 안 실린다 → 이미 가진 장비도 새 문구). 플래그가 꺼져 있으면 설명 없음."""
    if not SKILL_GEAR:
        return ""
    return _desc_lines(*skill_gear.GUARDIAN_GEAR_LINES[(t, idx)], cap=GEAR_DESC_LINES)


def _guardian_adds():
    adds = []
    for t, idx, name, _fn, atk_min, atk_max, crit, opt, slots, price in guardian_gear.WEAPONS:
        tail = _gear_tail(guardian_gear.WEAPON_LEVEL, 3, 0, slots, opt, 0, idx, -1, guardian_gear.WEAPON_REFINE, 0, 0, atk_max, atk_min) + bytes([crit, 0])
        assert len(tail) == 33
        adds.append((t, idx, name, price, _gear_desc(t, idx), tail))
    for t, idx, name, level, defence, refine, heavy, sprite, opt, slots, price, pal, _colour in guardian_gear.ARMOUR:
        tail = _gear_tail(level, 3, 0, slots, opt, 0, sprite, HP_SHIPPED_RECORDS + len(HP_APPEND) + pal, refine, 0, heavy, defence)
        assert len(tail) == 27
        adds.append((t, idx, name, price, _gear_desc(t, idx), tail))
    for t, idx, name, level, s, c, d, w, i, df, hp, sp, price in guardian_gear.ACCESSORIES:
        adds.append((t, idx, name, price, GUARDIAN_ACCESSORY_DESC[(t, idx)], accessory_tail(str_=s, con=c, dex=d, wis=w, int_=i, def_=df, level=level, hp=hp, sp=sp)))
    return adds


# ★ 2026-10-08 밤 「개성 옵션」(HERO2_GUARDIAN_DECISIONS.md C1, 호스트 블록 `GearPatch` — ⚠ 수치는 Claude 초안): 장신구 셋의 설명 문구.
#   무기 열 자루의 옵션은 정보창이 그리지 않는다 — 호스트 DESC 조각(`h2_acc_desc_block`)이 타입 11/12 레코드의 설명만 그린다(무기 정보창은
#   게임이 수치를 그리는 자리). 줄바꿈은 위 ITM_ADDS 의 폭 규칙(94 px, ASCII 6 · 한글 9)뿐 — 쉼표마다 끊기게 짧은 낱말로.
DESC_WIDTH = 94  # AccessoryPatch::DESC_WIDTH
DESC_LINES = 4  # AccessoryPatch::DESC_LINES — the box holds four 14-px lines under LV (rows 101..152)
GEAR_DESC_LINES = 3  # AccessoryPatch::GEAR_DESC_LINES — the equipment option page (★ 2026-10-14)


def desc_wrap(text, width=DESC_WIDTH):
    """The game's text box as it wraps (`ac.a(Graphics,IIII[BIII)V`): pen from 1, ASCII 6 · Hangul 9, a glyph with `pen + advance >
    width` starts the next line, and a space that overflows is dropped."""
    lines, cur, pen = [], "", 1
    for ch in text:
        w = 6 if ord(ch) < 128 else 9
        if pen + w > width:
            lines.append(cur)
            cur, pen = "", 1
            if ch == " ":
                continue
        cur += ch
        pen += w
    lines.append(cur)
    return lines


def _desc_lines(*lines, width=DESC_WIDTH, cap=DESC_LINES):
    """줄 하나하나를 그대로 그리게 하는 설명 문구: 줄마다 뒤에 공백을 「들어가는 만큼 + 1」 붙인다 — 마지막 공백이 넘쳐 줄이 바뀌고
    (넘친 공백은 버려진다) 다음 줄이 새 줄 맨 앞에서 시작한다. 폭 규칙은 위 ITM_ADDS 의 것(펜 1 · ASCII 6 · 한글 9 · `펜 + 폭 > W`)."""
    assert len(lines) <= cap, f"설명이 {len(lines)} 줄 — 정보창은 {cap} 줄까지"
    out = []
    for k, line in enumerate(lines):
        pen = 1 + sum(6 if ord(c) < 128 else 9 for c in line)
        assert pen <= width + 1, f"설명 한 줄이 {pen - 1} px — {width} px 를 넘는다: {line!r}"
        out.append(line + (" " * ((width - pen) // 6 + 1) if k + 1 < len(lines) else ""))
    text = "".join(out)
    assert [l.rstrip() for l in desc_wrap(text, width)] == list(lines), f"줄이 의도와 다르게 갈린다: {desc_wrap(text, width)}"
    return text


# ★ 2026-10-12: 광룡의 외침의 네 줄 (ITM_ADDS 의 공백이 줄 끝을 맞춘다).
GWANGRYONG_DESC_LINES = ["사자후 쿨타임 -50%", "사자후 SP소모량", "-50%", "의지 +20"]
assert [l.rstrip() for l in desc_wrap(next(a[4] for a in ITM_ADDS if a[:2] == (12, 17)))] == GWANGRYONG_DESC_LINES


GUARDIAN_ACCESSORY_DESC = {
    # ★ 2026-10-09 (3) 사용자: 용의 심장에 「적 처치시 1% 확률로 1분간 받는 피해 10% 감소 버프」 · 현자의 인장에 「적 타격 시 1% 확률로 모든
    #   스킬 쿨타임 초기화」 — 개성 옵션 둘. 줄마다 끊어 쓴다(그 전 판은 「H|P +250」 처럼 낱말 가운데서 갈라졌다).
    #   ★ 같은 날: 정보창은 네 줄까지 — 설명이 있는 장신구는 게임의 능력치 줄을 지우고(AccessoryPatch::DESC_DY) 설명이 능력치까지
    #   말한다. 그래서 두 장신구의 능력치를 한 줄에 들게 모았다(guardian_gear.ACCESSORIES: 체력 30 · 의지 30).
    (11, 20): _desc_lines("받는 피해 -5%,", "처치 시 1%로 1분간", "받는 피해 -10%,", "체력+30, HP+250"),
    (12, 20): _desc_lines("스킬 SP소모 -15%,", "타격 시 1%로 모든", "스킬 쿨타임 초기화,", "의지+30, SP+150"),
    (12, 21): _desc_lines("최대 HP +10%,", "힘+10, 지력+15,", "방어+10, HP+100,", "SP+100"),  # ★ 2026-10-09 사용자: 처치 EXP +10% → 최대 HP +10%
}


GUARDIAN_ADDS = _guardian_adds()
GUARDIAN_ITEMS = frozenset((a[0], a[1]) for a in GUARDIAN_ADDS)
assert len(GUARDIAN_ADDS) == 31 and len(GUARDIAN_ITEMS) == 31  # ★ 2026-10-09: 무기 10 · 방어구 18 · 장신구 3

# ── ★★ 2026-10-14 신규 아이템 2차 — 스킬 특화 장비 39종 (HERO2_NEW_ITEM_DRAFT.md §3 · §6, 표 = tools_re/skill_gear.py) ──────────────
# 능력치는 「바탕」 가디언 장비 그대로(무기 공격 · 방어구 방어 · 장신구 능력치 — 초안 §6: 수치가 아니라 옵션이 다르다) · LV 85 · 영웅 ·
#   이름 · 그림 · 옵션 문구만 새로. 무기 11 = 새 시트 `hero/6/<종류><번호>`(SKILL_WEAPON_DIR, 단검은 `hero/7` 도) · 방어구 23 = 바탕의
#   시트 + 새 `h.p` 팔레트(가디언 팔레트 뒤, skill_gear.armour() 순서) · 장신구 5 = 바탕의 꼬리 + 설명(옵션 + 능력치 한 줄).
#   효과는 호스트 블록(GearPatch 와 새 조각들) — 이 레코드는 이름 · 수치 · 그림 · 문구까지.
SKILL_WEAPON_DIR = skill_gear.WEAPON_DIR


def _skill_gear_adds():
    adds = []
    for code, t, idx, name, (_bt, _bi, _bn, _fn, atk_min, atk_max, crit, opt, slots, price) in skill_gear.weapons():
        tail = _gear_tail(guardian_gear.WEAPON_LEVEL, 3, 0, slots, opt, 0, idx, -1, guardian_gear.WEAPON_REFINE, 0, 0, atk_max, atk_min) + bytes([crit, 0])
        adds.append((t, idx, name, price, _desc_lines(*skill_gear.lines_of(code), cap=GEAR_DESC_LINES), tail))
    first_pal = HP_SHIPPED_RECORDS + len(HP_APPEND) + len(guardian_gear.ARMOUR)
    for k, (code, t, idx, name, base, _colour) in enumerate(skill_gear.armour()):
        _bt, _bi, _bn, level, defence, refine, heavy, sprite, opt, slots, price, _pal, _c = base
        tail = _gear_tail(level, 3, 0, slots, opt, 0, sprite, first_pal + k, refine, 0, heavy, defence)
        adds.append((t, idx, name, price, _desc_lines(*skill_gear.lines_of(code), cap=GEAR_DESC_LINES), tail))
    for code, t, idx, name, (_bt, _bi, _bn, level, s, c, d, w, i, df, hp, sp, price) in skill_gear.accessories():
        tail = accessory_tail(str_=s, con=c, dex=d, wis=w, int_=i, def_=df, level=level, hp=hp, sp=sp)
        adds.append((t, idx, name, price, _desc_lines(*skill_gear.lines_of(code)), tail))
    return adds


SKILL_GEAR_ADDS = _skill_gear_adds() if SKILL_GEAR else []
SKILL_GEAR_ITEMS = frozenset((a[0], a[1]) for a in SKILL_GEAR_ADDS)
assert not SKILL_GEAR or (len(SKILL_GEAR_ADDS) == 60 and len(SKILL_GEAR_ITEMS) == 60)  # ★★ 2026-10-14 39 + 번개의 호흡 (3/39) · ★★ 2026-10-15 − E2 휘두르기 도끼 · + E8 스프링 부츠 (9/62) · ★★ 2026-10-16 + 악의 무기 다섯 · ★★ 2026-10-17 + 발도 여섯
# the slots the guardian pass rolls (after HELL's 5..9 — ★ 2026-10-09 HELL got slot 9, the recipes, so the guardian's moved one up):
#   10 guardian gear · 11 guardian accessory (GATE) · 12 종결 gear · 13 종결 accessory · 14 the guardian recipes (GUARDIAN_RECIPES) ·
#   ★ 2026-10-12 15 인장 상급 (GATE = 환경설정 난이도 NORMAL+, 사용자 「가디언 노말 이상」) — 적 인덱스 % 5 가 종류(힘·체력·민첩·의지·지력)를 고른다
# ★★ 2026-10-14 (--skill-gear) 16 = 2차 장비 한 타입(적 인덱스 % 11, 그 타입의 2차 전부) · 17 = 2차 조합서 92..127 — 초안 §6.2,
#   사용자 「제안값」(1.5 % · 2 %). 블록의 가디언 패스는 배열 끝까지 걷는다(HellDropPatch) — 게스트 코드 변경 없음.
SKILL_SLOTS = (16, 17) if SKILL_GEAR else ()
GUARDIAN_SLOTS = (10, 11, 12, 13, 14, 15) + SKILL_SLOTS
SKILL_GEAR_WIDTH = 492  # 1.5 % (가디언 장비 칸 10 과 같게)
SKILL_RECIPE_WIDTH = 656  # 2 % (가디언 조합서 칸 14 와 같게)
SKILL_GEAR_TYPES = (0, 1, 2, 3, 4, 5, 6, 8, 9, 11, 12)
GUARDIAN_SEAL_WIDTH = 328  # 1 % (사용자 결정 2026-10-12)
GUARDIAN_GEAR_WIDTH = 492  # 1.5 %
GUARDIAN_ACC_WIDTH = 164  # 0.5 % (난이도 NORMAL+ 게이트)
GUARDIAN_FINAL_WIDTH = 984  # 3 %  (HELL 종결 장비 따라잡기)
GUARDIAN_FINAL_ACC_WIDTH = 328  # 1 %
GUARDIAN_GEAR_TYPES = (0, 1, 2, 3, 4, 5, 6, 8, 9, 10)  # 적 인덱스 % 10 이 칸 10 의 타입을 고른다 — 열아홉 장비가 전부 출처를 갖는다
GUARDIAN_LEVEL_DOWN = 20  # 칸 10 · 11 의 레벨 띠 아래쪽 (가디언 LV 105 의 적도 LV 88 장비를 떨군다); 위쪽은 DROP_LEVEL_UP 그대로

# ── ★★ 2026-10-09 가디언 조합서 31 (사용자: *"재료가방창도 127칸으로 만들꺼고. 추가로 신규아이템 조합서들을 구성할꺼야.
#    종결급 템들을 재료로 조합할 수 있는거야. 확장 이전 종결템들도 그런식으로 조합했었으니까 비슷한느낌으로 만들면 될거같아"* →
#    물어서 「가디언 31종 전부」 · 조합서 출처 「HELL + 가디언 드랍」) ──────────────────────────────────────────────────────
# 원작의 조합서(`itm/23`, 61 개)는 계열 D 레코드다: `[u32 price][이름 = 결과물 이름][u8 설명 길이][설명 = 재료 이름, 「;」 줄바꿈]`
#   뒤에 8 B 꼬리 = (결과 타입, id) + 재료 셋 (타입, id) — 없는 칸 ff ff(`aj.c()V` 는 앞에서부터 센다). 재료는 하나씩, 조합서도
#   한 장 쓰이고(`aj.d()V` @0x116d90) 값은 1,000 원 고정(`aj.c()V` 의 `0xfa << 2`). HERO2_GUEST_RE.md §8.60.
# 종결 조합의 원형 = 라그나로크(LV 70 영웅) ← 뇌인(LV 57 영웅 — 같은 종류 한 단계 아래) + 오리하르콘 + 용맹의 고리(에픽 반지).
#   그래서 가디언 31 = **같은 부위 · 같은 경/중갑의 확장 이전 종결 장비** + 최상급 재료(오리하르콘 · 엘리멘티움 · 아만타이트, 이름이
#   맞으면 마법의 얼음 · 바람피리 · 강화가죽 · 불사조 깃털 · 죄인의 저주) + **종결 장신구 하나**(영웅 목걸이/반지 14..16 — HELL 칸 6 ·
#   가디언 칸 13 이 떨군다). 장신구 셋은 같은 종류의 종결 장신구 + 다른 종결 장신구 + 재료. ⚠ 짝은 Claude 가 정했다(확인 대기).
#   ⛔ 종결 장비를 재료로 쓰므로 「입은 사본을 먹는」 원작 버그를 같이 고쳤다(guest_anchors::craft_worn_skip).
# id = 61.. GUARDIAN_ADDS 의 순서 그대로(무기 10 · 투구 4 · 갑옷 4 · 장갑 4 · 신발 4 · 방패 2 · 목걸이 1 · 반지 2). 가격 0(원작 그대로).
RECIPE_FIRST_NEW = 61
RECIPE_NAMES = {  # 재료의 아이템 DB 이름 — 빌드가 실제 itm 과 대조한다(verify)
    (0, 31): "라그나로크", (0, 32): "팬텀 블레이드", (1, 27): "어둠의광마도끼", (2, 27): "풍마의 분노검",
    (3, 31): "주작의 부리", (3, 32): "팬텀 퀘이드", (4, 31): "팬텀스타", (4, 32): "파괴마의 등뼈",
    (5, 25): "광속의 헤드기어", (5, 26): "팬텀 헬름", (5, 27): "파괴마의 머리",
    (6, 49): "열화의 흉갑", (6, 50): "팬텀레더아머", (8, 49): "열화의 손길", (8, 50): "망령의 손길",
    (9, 49): "열화의 발자국", (9, 50): "망령의 그림자", (10, 26): "정의의 수호자", (10, 27): "아에기스",
    (11, 14): "선지자의 유산", (11, 15): "칠요의 인", (11, 16): "마수의 발톱",
    (12, 14): "칠요의 반지", (12, 15): "순교자의 결의", (12, 16): "흑옥촉",
    (30, 3): "아만타이트", (30, 4): "엘리멘티움", (30, 5): "오리하르콘",
    (31, 1): "현자의 책", (31, 4): "죄인의 저주", (31, 5): "축복의 시",
    (32, 0): "불사조 깃털", (32, 3): "바람피리", (32, 4): "강화가죽", (32, 5): "마법의 얼음",
}
GUARDIAN_RECIPES = [  # (결과, 재료1 = 종결 장비, 재료2 = 재료, 재료3 = 종결 장신구)
    ((0, 33), (0, 32), (30, 5), (11, 14)),  # 오딘의 대검 ← 팬텀 블레이드 · 오리하르콘 · 선지자의 유산
    ((0, 34), (0, 31), (30, 5), (12, 15)),  # 디미트리의 참수검 ← 라그나로크 · 오리하르콘 · 순교자의 결의
    ((1, 28), (1, 27), (30, 5), (11, 15)),  # 고대수호물의 도끼 ← 어둠의광마도끼 · 오리하르콘 · 칠요의 인
    ((1, 29), (1, 27), (30, 4), (11, 16)),  # 제스의 수련도끼 ← 어둠의광마도끼 · 엘리멘티움 · 마수의 발톱
    ((2, 28), (2, 27), (30, 5), (12, 14)),  # 이안의 검 ← 풍마의 분노검 · 오리하르콘 · 칠요의 반지
    ((2, 29), (2, 27), (30, 4), (12, 15)),  # 파멸의 수호검 ← 풍마의 분노검 · 엘리멘티움 · 순교자의 결의
    ((3, 33), (3, 31), (30, 5), (11, 16)),  # 터줏대감의 송곳니 ← 주작의 부리 · 오리하르콘 · 마수의 발톱
    ((3, 34), (3, 32), (30, 5), (12, 16)),  # 응보의 단검 ← 팬텀 퀘이드 · 오리하르콘 · 흑옥촉
    ((4, 33), (4, 32), (30, 5), (11, 14)),  # 심판의 철퇴 ← 파괴마의 등뼈 · 오리하르콘 · 선지자의 유산
    ((4, 34), (4, 31), (30, 5), (12, 14)),  # 보복의 망치 ← 팬텀스타 · 오리하르콘 · 칠요의 반지
    ((5, 28), (5, 27), (30, 3), (11, 15)),  # 용암군주의 투구(중) ← 파괴마의 머리(중) · 아만타이트 · 칠요의 인
    ((5, 29), (5, 26), (30, 4), (12, 16)),  # 그림자 사냥꾼의 두건(경) ← 팬텀 헬름(경) · 엘리멘티움 · 흑옥촉
    ((5, 30), (5, 25), (32, 5), (12, 14)),  # 빙해왕의 투구(중) ← 광속의 헤드기어(중) · 마법의 얼음 · 칠요의 반지
    ((5, 31), (5, 26), (32, 3), (11, 14)),  # 바람술사의 머리띠(경) ← 팬텀 헬름(경) · 바람피리 · 선지자의 유산
    ((6, 51), (6, 49), (30, 5), (11, 15)),  # 불굴의 흉갑(중) ← 열화의 흉갑(중) · 오리하르콘 · 칠요의 인
    ((6, 52), (6, 50), (32, 4), (12, 16)),  # 밤까마귀 가죽갑옷(경) ← 팬텀레더아머(경) · 강화가죽 · 흑옥촉
    ((6, 53), (6, 49), (30, 3), (12, 15)),  # 심연의 판금갑옷(중) ← 열화의 흉갑(중) · 아만타이트 · 순교자의 결의
    ((6, 54), (6, 50), (32, 5), (11, 16)),  # 서리늑대 가죽옷(경) ← 팬텀레더아머(경) · 마법의 얼음 · 마수의 발톱
    ((8, 51), (8, 49), (30, 5), (12, 14)),  # 거인의 건틀릿(중) ← 열화의 손길(중) · 오리하르콘 · 칠요의 반지
    ((8, 52), (8, 50), (31, 4), (11, 16)),  # 독사의 장갑(경) ← 망령의 손길(경) · 죄인의 저주 · 마수의 발톱
    ((8, 53), (8, 49), (30, 3), (12, 15)),  # 파쇄자의 손갑(중) ← 열화의 손길(중) · 아만타이트 · 순교자의 결의
    ((8, 54), (8, 50), (32, 3), (12, 16)),  # 질풍의 손목보호대(경) ← 망령의 손길(경) · 바람피리 · 흑옥촉
    ((9, 51), (9, 49), (30, 3), (11, 14)),  # 산맥의 철각(중) ← 열화의 발자국(중) · 아만타이트 · 선지자의 유산
    ((9, 52), (9, 50), (30, 4), (12, 16)),  # 그림자 걸음(경) ← 망령의 그림자(경) · 엘리멘티움 · 흑옥촉
    ((9, 53), (9, 49), (30, 5), (11, 15)),  # 진군의 철화(중) ← 열화의 발자국(중) · 오리하르콘 · 칠요의 인
    ((9, 54), (9, 50), (32, 0), (12, 14)),  # 비호의 장화(경) ← 망령의 그림자(경) · 불사조 깃털 · 칠요의 반지
    ((10, 28), (10, 27), (30, 3), (12, 15)),  # 성채의 방패 ← 아에기스 · 아만타이트 · 순교자의 결의
    ((10, 29), (10, 26), (30, 5), (11, 14)),  # 격돌의 방패 ← 정의의 수호자 · 오리하르콘 · 선지자의 유산
    ((11, 20), (11, 15), (11, 16), (32, 0)),  # 용의 심장 ← 칠요의 인 · 마수의 발톱 · 불사조 깃털
    ((12, 20), (12, 16), (11, 14), (31, 1)),  # 현자의 인장 ← 흑옥촉 · 선지자의 유산 · 현자의 책
    ((12, 21), (12, 14), (12, 15), (31, 5)),  # 생명의 서약 ← 칠요의 반지 · 순교자의 결의 · 축복의 시
]


def _guardian_recipe_adds():
    """`itm/23` 61.. — one recipe a guardian item, in GUARDIAN_ADDS' order: name = the result's, description = the materials'
    names one a line (`;`, the shipped recipes' own form), tail = result + three materials. Each line must fit the info
    box (DESC_WIDTH, the game's 94-px rule)."""
    names = {(a[0], a[1]): a[2] for a in GUARDIAN_ADDS}
    assert [r[0] for r in GUARDIAN_RECIPES] == [(a[0], a[1]) for a in GUARDIAN_ADDS], "one recipe per guardian item, in order"
    adds = []
    for k, (out, *mats) in enumerate(GUARDIAN_RECIPES):
        assert len(set(mats)) == 3, f"{names[out]}: a material twice — the craft consumes by (type, id) one at a time"
        lines = [RECIPE_NAMES[m] for m in mats]
        for line in lines:
            assert 1 + sum(6 if ord(c) < 128 else 9 for c in line) <= DESC_WIDTH, f"recipe {names[out]}: {line!r} is wider than the box"
        tail = bytes([out[0], out[1]] + [b for m in mats for b in m])
        adds.append((RECIPE_TYPE, RECIPE_FIRST_NEW + k, names[out], 0, ";".join(lines), tail))
    return adds


# ★★ 2026-10-14 2차 조합서 — `itm/23` 92..127 (36 장): 결과 ← 그 아이템이 능력치를 빌린 가디언 장비 + 최상급 재료 + 종결 장신구
#   (장신구는 빌린 가디언 장신구 + 다른 종결 장신구 + 재료 — skill_gear.RECIPES, ⚠ 짝은 Claude 초안). 조합서 id 는 부호 있는 바이트
#   (`ch.h:B`)라 127 에서 끝난다 → F1 · F2 · E7 은 조합서 없이 가디언 드랍(칸 16)만(사용자 2026-10-14).
SKILL_RECIPE_FIRST = RECIPE_FIRST_NEW + len(GUARDIAN_RECIPES)
ITEM_ID_MAX = 127  # ch.h:B


def _skill_recipe_adds():
    names = {**RECIPE_NAMES, **{(a[0], a[1]): a[2] for a in GUARDIAN_ADDS}, **{(a[0], a[1]): a[2] for a in SKILL_GEAR_ADDS}}
    base = {code: b for code, _t, _i, _n, b, _l in skill_gear.ITEMS}
    adds = []
    for code, t, idx, name, _b, _l in skill_gear.ITEMS:
        if code in skill_gear.NO_RECIPE:
            continue
        mats = (base[code], *skill_gear.RECIPES[code])
        assert len(set(mats)) == 3, f"{name}: a material twice — the craft consumes by (type, id) one at a time"
        lines = [names[m] for m in mats]
        for line in lines:
            assert 1 + sum(6 if ord(c) < 128 else 9 for c in line) <= DESC_WIDTH, f"recipe {name}: {line!r} is wider than the box"
        rid = SKILL_RECIPE_FIRST + len(adds)
        assert rid <= ITEM_ID_MAX, f"recipe {name}: id {rid} past {ITEM_ID_MAX} (ch.h:B)"
        adds.append((RECIPE_TYPE, rid, name, 0, ";".join(lines), bytes([t, idx] + [b for m in mats for b in m])))
    return adds

# ── 야수진화제를 상점에 올린다 (#22 의 절반, 2026-09-23 사용자 결정 · 2026-09-24 적 추가) ──
# 사용자 지시: *"야수진화제도 흑 백 모두 50000원에 팔꺼야"*(09-23) · *"상점에 적도 팔아야해"* +
# *"적은 70000원이야"*(09-24). 게임은 이미 이 아이템을 알고 (`itm/22` = 백·흑·적) UI 어휘도 다
# 갖고 있다(`t/bm.tdf`: 진화제·진화제를 선택하세요 …). 없던 것은 **상점에 놓이지 않았다**는 것
# 하나다 — `itm/forshop` 에 type 22 가 없었다.
#
# ⚠ `price` 는 구매가와 판매가를 **같이** 움직일 수 있다(판매가가 구매가의 함수라면).
#    이미 진화제를 들고 있는 플레이어가 비싸게 팔 수 있게 된다 — 사용자가 알고 고른 값이다.
# ★ 적(`itm/22[2]`, 「상급 진화용」)은 백·흑보다 비싸다 — 70,000. 이것도 사용자가 준 숫자다.
# ★ 2026-10-05 (user: *"상점에서 기타항목으로 현재 재련석을 500원에 구매할 수있어야하는데 …"* → *"판매가 200원으로 바꿔"*):
#   제련석 `itm/20[0]` 1,000 → 500 + forshop `0x1400`(방패 뒤 = 기타 구간 맨 앞). 판매가는 가격의 ¼ 이 아니라
#   `guest_anchors::ShopSellPatch::PRICES` 의 200 이다(이미 가진 1,000 짜리 레코드도 같은 200).
#   ⛔ 2026-10-03 의 「상점에서 재련석 1개에 500원씩으로 팔아줘」 는 이것이었다 — 그때는 판매가를 500 으로 올렸다.
PRICE_FIXES = {
    (20, 0): ("제련석", 1000, 500),
    (22, 0): ("야수진화제 백", 2000, 50000),
    (22, 1): ("야수진화제 흑", 2000, 50000),
    (22, 2): ("야수진화제 적", 2000, 70000),
}
# forshop 에 새로 놓을 것. 레코드는 그 itm 레코드를 **그대로** 복사하고 앞에 전역 인덱스를
# 붙인다(= 이 파일이 하는 일의 전부다 — 134/134 가 그렇게 돼 있다, HERO2_ITEM_DB.md §6.5).
# 파일은 전역 인덱스 오름차순이므로 22 는 방패(10)와 포션(24) 사이에 들어간다.
# ★ 2026-09-25: 부활의 구 `0x1505` — 망각의 구 둘 바로 뒤, 진화제 앞(기타 구간).
# ★ 2026-10-05: 제련석 `0x1400` — 방패(0x0a08) 바로 뒤, 망각의 구 앞 = 기타 구간 맨 앞.
SHOP_ADDS = [(28, 1), (22, 0), (22, 1), (22, 2), (21, 3), (21, 4), (21, 5), (21, 6), (21, 7), (21, 8), (20, 0)]  # ★★ 2026-10-16 (21, 7) 엘릭서 · 2026-10-20 (21, 8) 상점주인의 혼
# ★★ 2026-10-16 (user: *"판매위치 지금 위치 말고 포도주 아래에 넣어줘"*): records placed right after another instead of by
# global index. The game reads `itm/forshop` front to back and appends each non-equipment record to the 기타 list in file
# order (`ch.a()[Ljava/util/Vector;` @0x140a90), so file order IS the list order — 엘릭서 0x1507 goes after 포도주 0x1a00.
SHOP_PLACE_AFTER = {(21, 7): (26, 0), (21, 8): (21, 7)}  # ★★ 2026-10-20 상점주인의 혼 0x1508 right after the elixir (포도주 → 엘릭서 → 상점주인의 혼 → 치료제 …)

# ── 드랍 표 `enm/enm` (#20 · #21, 2026-09-24 사용자 결정) ──────────────────────
# 사용자 지시: *"드랍안되는 아이템이 많아서 드랍되도록 변경"* · *"장비드랍률 상승"*, 그리고
# 2026-09-24 의 결정 둘: **#20 = 모드 창 풀기 + 자동 배치** · **#21 = 장비 ×2**.
# ★ 2026-09-26: *"레어 이상등급의 아이템 드랍률을 올리고싶어"* → *"일반 10%, 레어 4%, 에픽 3%, 영웅2%"* —
#   장비 슬롯의 폭이 **등급 표**(GRADE_WIDTH)가 됐고 #21 의 ×2 는 그 일반 칸(10 %)으로 흡수됐다.
#
# 표의 모양 = `tools_re/decode_drops.py` 머리말(과 HERO2_GUEST_RE.md §8.9): 적마다 body[33] 개의
# 슬롯 `(u8 타입, u8 id 최소, u8 id 최대, u16 LE 누적)`, 처치 루틴 `bd.q()V` 가 롤 1..32767
# (**고정 분모**)을 `누적[k-1] < 롤 < 누적[k]` 인 첫 슬롯에 넣는다. 그래서 슬롯 **폭**
# (= 누적의 차)이 곧 확률이고, 폭을 두 배로 하면 그 슬롯이 두 배로 나온다.
#
# ⛔⛔ **모드 창은 배포본 그대로다 (2026-09-25).** `bd.q()V` 는 NORMAL 에서 슬롯 0·1·2 만, HARDCORE/HELL
#    에서 2·3·4 만 굴린다 — 그리고 3·4 는 적마다 **그 모드의 레벨**(`2·lv/5 + 40` / `+ 60`)에 맞춘 장비다.
#    2026-09-24 의 코드 패치(`drop_window_all`, 다섯 슬롯 전부)는 그것을 NORMAL 에 풀어서 LV 2 거미가 LV 36
#    장갑을 10 % 로 떨구게 만들었고 삭제됐다. 그래서 이 플랜은 **창마다 따로** 놓는다: NORMAL 이 굴리는
#    슬롯(0·1·2)에는 그 적 레벨 + DROP_LEVEL_UP 을 넘는 장비가 없고, NORMAL 적의 최고 레벨(LV 45) 이하 장비는
#    전부 NORMAL 에서 나오며, 그 위(LV 46+)는 HARDCORE/HELL 슬롯(3·4)에서 나온다.
#
# 순서 (결정적 — 같은 입력이면 같은 표):
#   ⓪ 레벨 규칙 위반 자르기: NORMAL 창 장비 슬롯의 id 범위에서 적 레벨 + DROP_LEVEL_UP 을 넘는 것을 뺀다
#      (배포본에서는 스켈레톤[29] 슬롯 0 의 반지 0-16 → 0-8 하나).
#   ① 인장 하급·중급: 인장 슬롯 `2-2`(상급만) → `0-2`. 셋이 한 슬롯을 나눈다.
#   ② 조합서 58·59·60(아만타이트·엘리멘티움·오리하르콘): `…-57` 로 끝나는 조합서 슬롯을 `…-60` 으로.
#   ③ (없앴다, 2026-09-27) 마력구 하급·중급·상급 — 09-24 판은 귀환서 슬롯 셋(수배범·독수리·제국정예병 슬롯 2)을
#      돌려써 ≈1 % 로 떨궜다. 그런데 **네트워크 전용 아이템**이다(사용자: *"하급마력구와 상급마력구 등은 네트워크
#      전용아이템이여서 드랍되면 안돼"*) — 배포본이 판매·조합·드랍 어디에도 두지 않은 이유가 그것이었다. 그 셋은
#      배포본의 귀환서 0-0 으로 돌아갔고, NO_DROP_TYPES 가 다시 들어오지 못하게 잠근다.
#   ④⁰ 지정 배치 (DROP_ASSIGN, 2026-09-28): 사용자가 고른 칸을 먼저 바꾼다 — 바뀌는 칸의 원래 아이템이 그 창의
#      **다른 배치된 적**에게서도 나올 때만, 새 아이템이 레벨 규칙을 지킬 때만. ④ 가 그 결과를 보고 나머지를 채운다.
#   ④ 창마다(NORMAL = 슬롯 0·1, HARD = 3·4) 그 창에서 안 나오는 장비: (A) 같은 타입 슬롯의 id 범위를
#      넓힌다 — 범위 ≤ 4 · 같은 등급 · 레벨 폭 ≤ 10, (B) ≤ 6 · ≤ 16 으로 다시, (C) 그래도 남으면 그 창에서
#      **모든 아이템이 다른 슬롯에서도 나오는** 장비 슬롯 하나를 그 아이템으로 돌려쓴다. 어느 단계든 그
#      슬롯의 모드 레벨(NORMAL 은 적 레벨, 3·4 는 HARD/HELL 레벨)에서 레벨 규칙을 지켜야 한다 — 원작도 H
#      슬롯에는 HARD 레벨에 맞춘 장비를 넣었다(갈색곰 LV20 → HARD LV48 의 H4 = LV48 갑옷).
#   ⑤ 등급별 폭 (2026-09-26 — #21 의 「장비 ×2」를 흡수했다): 장비 슬롯(무기·방어구·방패·목걸이·반지)
#      마다 폭 = 그 슬롯이 담은 아이템 등급의 폭(GRADE_WIDTH, 섞인 슬롯은 평균). 재료·퀘스트·소모품
#      슬롯은 그대로. 적 하나의 합이 32767 을 넘으면 **장비 폭만** 비례로 줄여 맞춘다.
# ⛔⛔ 「나온다」는 **맵에 배치된 적**에게서 나온다는 뜻이다 (2026-09-28). 신관(#23)·신관병(#24)은 드랍 게이트가
#    있지만 111장 어느 맵의 몬스터 목록에도 없다(원본부터 — `map_monsters.py`). 09-24~27 판은 그 둘을 출처로 세어서
#    신관만 떨구던 팬텀스타·파괴마의 등뼈(HARD)와 붉은사자의흉갑(NORMAL)을 「이미 나온다」로 두었고, 그 탓에 원작의
#    무쇠가죽돼지 슬롯 0(붉은사자의흉갑)을 블러디 체인으로 덮었다. 이제 ④ 의 적용 범위·후보와 verify_enm 이 배치된 적만
#    센다 — 배치 없는 적의 칸도 ⑤ 의 등급 폭은 받는다(아무도 굴리지 않는 칸이다).
# ⛔ 스펠카드 11·12·29 는 건드리지 않는다: 11·12 는 처치 루틴이 **일부러** 건너뛰고(id 로 비교),
#    29 는 적 인덱스 90~92(보스, 드랍 게이트 0)에서만 나오는 번호다 — 표가 아니라 코드가 정한 것이다.
# ⛔ 오브 6 개는 조합서 52~57 로 만들어진다(그 조합서는 떨어진다) — 구멍이 아니다.
ENM = "enm/enm"
ENM_RECORDS = 102
ENM_SLOTS = 5
ENM_SLOT_OFF = 34  # body offset of the first slot (body[33] = slot count)
GEAR_TYPES = frozenset({0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12})
SLOT_TYPE_REMAP = {29: 23}  # bd.q()V @0x12836a — a slot typed 29 drops a 조합서 (23)
DROP_ROLL_MAX = 0x7FFF  # h.a(1, 0x7fff)
SEAL_TYPES = frozenset(range(13, 18))  # 힘·체력·민첩·의지·지력의 인장


def guardian_seal_type(idx):
    """★ 2026-10-12: 가디언 칸 15 의 인장 종류 — 적 인덱스 % 5 → 13 힘 · 14 체력 · 15 민첩 · 16 의지 · 17 지력."""
    return min(SEAL_TYPES) + idx % len(SEAL_TYPES)


RECIPE_TYPE, RECIPE_HI_FROM, RECIPE_HI_TO = 23, 57, 60
# ⛔ 어느 슬롯도 이 타입을 떨구면 안 된다 (2026-09-27) — verify_enm 이 잠근다. 타입 21 마법구:
#   0·1·2 하급·중급·상급 마력구 = 네트워크 전용(③) · 3·4 망각의 구 · 5 부활의 구 = 호스트의 상점 아이템(ITM_ADDS).
#   배포본 표에도 타입 21 슬롯은 없다.
NO_DROP_TYPES = frozenset({21})
# ★★ 등급별 슬롯 폭 = 그 등급 장비가 **한 슬롯에서** 나올 확률 (2026-09-26 사용자 결정: *"일반 10%, 레어 4%,
# 에픽 3%, 영웅2% 로 변경해줘"*). 롤이 양쪽 엄격이라 폭 w 인 슬롯은 w − 1 개 값에 맞는다 — 그래서
# P = (w − 1) / 32767 = 9.995 % · 4.001 % · 3.000 % · 1.999 %. 일반 3276 은 #21 의 「장비 ×2」(원작 1638 × 2)
# 그대로이고, #21 은 이 표에 흡수됐다(⑤). 원작이 등급에 준 폭은 1638 · 163 · 32 · 16(= 5 · 0.5 · 0.1 ·
# 0.05 %, 배포본 440 슬롯에서 읽은 값)이었고 09-24~25 판은 그 ×2 였다 — 레어 이상만 크게 오른다.
# ④(C) 로 돌려쓴 슬롯도 이 폭을 받는다: 옛 폭을 그대로 두면 10 % 일반 슬롯이 영웅 하나를 10 % 로 떨군다.
# ★ 2026-10-02 (사용자: *"하드코어와 헬모드 드랍률이 전체적으로 너무 높다 … 하드코어 레어 3% 에픽 1%, 영웅 0.5% …
#   헬은 레어 4%, 에픽 1.5%, 영웅 1% … 장신구는 하드코어 0.5%, 헬 1%"*) — 폭이 **슬롯이 굴려지는 모드**를 따른다:
#   NORMAL 이 굴리는 슬롯 0·1·2 = GRADE_WIDTH 그대로(슬롯 2 는 HARDCORE/HELL 도 굴리지만 담는 것이 NORMAL 레벨 장비다 —
#   「종결템」이 아니다) · HARDCORE/HELL 만 굴리는 슬롯 3·4 = HARD_GRADE_WIDTH, 그 칸의 목걸이·반지는 등급과 무관하게
#   HARD_ACCESSORY_WIDTH · HELL 보너스 칸 5·6 = HELL_GRADE_WIDTH · HELL_ACCESSORY_WIDTH(아래 「HELL 보너스 칸」).
#   1 % = 328(위 표와 같은 단위). 일반 등급은 사용자가 안 바꿨다(10 %).
# ⛔ 같은 날 저녁부터 이 두 등급 표는 **쓰이지 않는다** — 아래 티어 표(NORMAL_TIER_WIDTH · HARD_TIER_WIDTH)가 대신한다. 기록용으로 둔다.
GRADE_WIDTH = {0: 3276, 1: 1312, 2: 984, 3: 656}
HARD_GRADE_WIDTH = {0: 3276, 1: 984, 2: 328, 3: 164}
ACCESSORY_TYPES = frozenset({11, 12})  # 목걸이 · 반지
HARD_ACCESSORY_WIDTH = 164
# ★★★ 2026-10-02 저녁 — 드랍 **티어** (사용자: *"종결아이템급은 드랍률을 좀 낮추고싶어. 하드코어에서는 종결아이템이 거의 드랍이
#   안되는 수준으로하고 헬모드에서는 1%확률로 … 중간에 거쳐가는 붉은 사자세트 같은 중간급 아이템들은 드랍률을 올려서"* ·
#   *"장신구도 … 하드코어에서는 종결급 장신구는 거의 안나오는 느낌 … 중간급 장신구는 잘나와야해 … 신규 6종 장신구는 매우 낮춰야해
#   0.1%수준"*). 폭이 등급이 아니라 **티어**를 따른다 — `tier_of(타입, 레벨, 등급, 세트)`:
#     종결 = 영웅 LV 69+ 장비(팬텀·열화·망령·파괴마 세트 · 라그나로크 · 주작의 부리 · 어둠의광마도끼 · 풍마의 분노검 · 광속의 헤드기어 ·
#            작렬의 방패 · 정의의 수호자 · 아에기스, 20) + **영웅 장신구 전부**(14..16 여섯 + 신규 17..19 여섯) + 팬텀S 세트
#     상급 = 그 밖의 영웅(버서커 · 암살자 · 악마 세트 등 LV 47~60) · 레어/에픽 LV 55+ · 베르S 세트 — **오늘 낮에 정한 수치 그대로**
#     중간 = 레어/에픽 LV 54 이하 · 바람S/테러S/사자S 세트 · 레어/에픽 장신구(0..13) — **×2**
#     일반 = 등급 0 — 그대로 10 %
#   사용자 결정(2026-10-02 저녁, 물어서): 종결 범위 = 영웅 LV69+ 장비 + 영웅 장신구 · HARD 종결 칸 **0.1 %** · HELL 종결 장비 **1 %**(보너스 칸 7) ·
#   HELL 종결 장신구(14..16) **0.3 %** · 신규 6종 = HELL + 환경설정 난이도 HARD 에서만 **0.1 %**(보너스 칸 8, GATE 비트) · 중간 ×2.
#   ★ 2026-10-03: 신규 6종의 난이도 조건은 **NORMAL 이상**이 됐다(사용자: *"헬 노말난이도부터"*) — 코드 상수 `GATE_DIFFICULTY` 뿐, 에셋 불변.
#   세트로 티어를 정하는 이유: 베르디슈(LV 50~56)·붉은사자(LV 35~42)가 레벨 경계(54)에 걸쳐 있어 한 세트가 두 티어로 갈라진다.
#   폭 단위 = 그대로(1 % = 328, P = (w − 1)/32767): 0.1 % = 34 · 0.3 % = 99.
# ★★ 2026-10-02 밤 — 낮춰 둔 폭을 조금씩 올림 (사용자: *"드랍테이블 낮춰둔거 전체적으로 조금씩 올리자"*, 수치는 Claude 가 정함 = 대략 ×1.5):
#     HARDCORE/HELL 창(슬롯 3·4): 상급 레어 3 → **4 %** · 에픽 1 → **1.5 %** · 영웅 0.5 → **0.75 %** · 중간 에픽 2 → **3 %** · 종결 0.1 → **0.15 %**
#     HELL 보너스 칸: 상급 레어 4 → **5 %** · 에픽 1.5 → **2 %** · 영웅 1 → **1.5 %** · 종결 장비 1 → **1.5 %** · 종결 장신구 0.3 → **0.5 %** ·
#                    신규 6종 0.1 → **0.15 %**
#   낮춘 적 없는 것(일반 10 % · NORMAL 창 전부 · HARD 창 중간 레어 6 %)은 그대로. 폭: 0.15 % = 50 · 0.5 % = 164 · 0.75 % = 246 · 1.5 % = 492 · 5 % = 1640.
TIER_FINAL, TIER_UPPER, TIER_MID, TIER_COMMON = "final", "upper", "mid", "common"
TIER_GUARDIAN = "guardian"  # ★ 2026-10-08: 영웅 LV GUARDIAN_TIER_LEVEL+ — the guardian pass's slots 9 · 10 only
TIER_NAME = {TIER_GUARDIAN: "가디언", TIER_FINAL: "종결", TIER_UPPER: "상급", TIER_MID: "중간", TIER_COMMON: "일반"}
GUARDIAN_TIER_LEVEL = 85
FINAL_LEVEL = 69  # 영웅 장비는 이 레벨부터 종결
UPPER_LEVEL = 55  # 레어·에픽은 이 레벨부터 상급 (그 아래 = 중간)
SET_TIER = {1: TIER_MID, 2: TIER_MID, 3: TIER_MID, 4: TIER_UPPER, 5: TIER_FINAL}  # 바람S · 테러S · 사자S · 베르S · 팬텀S (itmtp[59..63])
# 창마다 (티어, 등급) → 폭. NORMAL 창(슬롯 0·1·2) · HARDCORE/HELL 창(슬롯 3·4). 등급은 중간·상급 안에서 레어/에픽을 가른다.
NORMAL_TIER_WIDTH = {
    (TIER_COMMON, 0): 3276,  # 10 %
    (TIER_MID, 1): 2624, (TIER_MID, 2): 1968,  # 8 · 6 % (= 낮 수치 ×2)
    (TIER_UPPER, 1): 1312, (TIER_UPPER, 2): 984, (TIER_UPPER, 3): 656,  # 4 · 3 · 2 % (낮 수치 그대로 — 레벨 규칙상 LV 50 까지)
    (TIER_FINAL, 3): 50,  # 0.15 % — NORMAL 창에는 레벨 규칙상 올 수 없다(표를 닫으려고 둔다)
}
HARD_TIER_WIDTH = {
    (TIER_COMMON, 0): 3276,  # 10 %
    (TIER_MID, 1): 1968, (TIER_MID, 2): 984,  # 6 · 3 % (에픽 2 → 3 %, 밤)
    (TIER_UPPER, 1): 1312, (TIER_UPPER, 2): 492, (TIER_UPPER, 3): 246,  # 4 · 1.5 · 0.75 % (저녁 3 · 1 · 0.5 % 에서, 밤)
    (TIER_FINAL, 3): 50,  # 0.15 % — 「거의 안 나온다」(저녁 0.1 % 에서, 밤)
}
WINDOW_TIER_WIDTH = {"N": NORMAL_TIER_WIDTH, "H": HARD_TIER_WIDTH}
# 세트 조각의 출처 보강 (④ᵇ, 「이것저것 많이 껴볼 수 있게」): 중간 세트 조각은 NORMAL 창에서 적어도 이만큼의 배치된 적에게서,
# LV 35+ 조각은 HARDCORE 창에서도 적어도 이만큼 — 같은 타입·같은 등급 슬롯의 id 범위를 넓혀서(④ 의 A/B 규칙, 레벨 규칙 그대로).
# 못 채우면 로그에 남기고 그대로 둔다(규칙을 깨지 않는다).
SET_SOURCES = {"N": 3, "H": 2}
SET_SOURCES_HARD_LEVEL = 35


def tier_of(t, level, grade, set_id=0):
    """The drop tier of a gear item (see the 드랍 티어 note above)."""
    if set_id in SET_TIER:
        return SET_TIER[set_id]
    if grade == 3 and level >= GUARDIAN_TIER_LEVEL:
        return TIER_GUARDIAN
    if t in ACCESSORY_TYPES:
        return TIER_FINAL if grade == 3 else TIER_MID
    if grade == 3:
        return TIER_FINAL if level >= FINAL_LEVEL else TIER_UPPER
    if grade == 0:
        return TIER_COMMON
    return TIER_UPPER if level >= UPPER_LEVEL else TIER_MID
# ★ 2026-10-02 스킬을 바꾸는 장신구 6종(ITM_ADDS 의 목걸이·반지 17..19)은 **HELL 보너스 칸 8 에서만** 떨어진다(「헬 하드 모드
#   이상에서만 드랍」 = HELL + 환경설정 난이도 HARD, 같은 날 저녁 — 처음엔 칸 6 이었다; ★ 2026-10-03 부터 난이도 NORMAL 이상). #20 의 「모든 장비가 어느 창에서든 나온다」
#   규칙(plan_drops ④ · verify_enm 의 lost_n/lost_h)에서 뺀다 — 빼지 않으면 LV 1 이라 NORMAL 창에 「안 나오는 장비」로 자동
#   배치된다(실측: 적 6마리의 슬롯 0, 각 2 %). verify_enm 이 슬롯 0..4 에 이 아이템이 있으면 거절한다.
#   (★ 2026-10-03 부터 LV 70 — 그래도 뺀다: 이 6종의 출처는 칸 8 하나라는 결정이 레벨과 무관하다.)
HELL_ONLY_ITEMS = frozenset((a[0], a[1]) for a in ITM_ADDS if a[0] in ACCESSORY_TYPES)
# ★ 2026-10-08: the guardian 22 are appended after that census (their accessories are the guardian pass's, not slot 8's);
#   OUT_OF_WINDOWS = everything no NORMAL/HARD window and no HELL slot may place.
ITM_ADDS = ITM_ADDS + GUARDIAN_ADDS + SKILL_GEAR_ADDS
# ★ 2026-10-09: …and their recipes (`itm/23` 61..91) — not gear, so no window rule touches them; HELL slot 9 and guardian slot
#   14 are their only source (RECIPE_SLOT_IDS).
GUARDIAN_RECIPE_ADDS = _guardian_recipe_adds()
ITM_ADDS = ITM_ADDS + GUARDIAN_RECIPE_ADDS
RECIPE_SLOT_IDS = (RECIPE_FIRST_NEW, RECIPE_FIRST_NEW + len(GUARDIAN_RECIPE_ADDS) - 1)
assert RECIPE_SLOT_IDS == (61, 91)
SKILL_RECIPE_ADDS = _skill_recipe_adds() if SKILL_GEAR else []
ITM_ADDS = ITM_ADDS + SKILL_RECIPE_ADDS
SKILL_RECIPE_IDS = (SKILL_RECIPE_FIRST, SKILL_RECIPE_FIRST + len(SKILL_RECIPE_ADDS) - 1)
assert not SKILL_GEAR or SKILL_RECIPE_IDS == (92, 126)  # ★★ 2026-10-15 35 장 (E2 휘두르기 도끼 삭제 — 127 이 빈다)
# ★ 2026-10-09 (3): every accessory description fits the info box — four lines (it clears the game's stat rows and says the stats).
for _t, _i, _name, _price, _desc, _tail in ITM_ADDS:
    if _t in ACCESSORY_TYPES and _desc:
        assert len(desc_wrap(_desc)) <= DESC_LINES, f"{_name}: 설명이 {len(desc_wrap(_desc))} 줄 {desc_wrap(_desc)}"
OUT_OF_WINDOWS = HELL_ONLY_ITEMS | GUARDIAN_ITEMS | SEAL_TOP_ITEMS | SKILL_GEAR_ITEMS  # ★ 2026-10-12: 인장 상급은 가디언 칸 15 에서만 · ★ 2026-10-14 2차는 칸 16 에서만
# ★ 레벨 규칙 (2026-09-25, 사용자: "2랩 3랩 몬스터를 잡았는데 40랩제 아이템들이 쏟아지고있어 … 각 몬스터에
# 레벨에 맞는 드랍테이블이 올바르게 되었는지 점검해야해"). 슬롯이 굴려지는 **그 모드의 적 레벨**보다 장비가
# DROP_LEVEL_UP 을 넘게 높으면 안 된다(배포본 NORMAL 슬롯 0·1 의 최대가 +5 다 — 반지 하나 빼고). 새로 넣는
# 장비는 DROP_LEVEL_DOWN 보다 낮지도 않게 먼저 찾는다.
DROP_LEVEL_UP = 5
DROP_LEVEL_DOWN = 15
# `bd.q()V` 의 배포본 창: NORMAL 은 0·1·2, HARDCORE/HELL 은 2·3·4. 슬롯 2 는 모든 모드가 굴린다.
NORMAL_SLOTS = (0, 1, 2)
HARD_WINDOW = (2, 3, 4)
HARD_ONLY_SLOTS = (3, 4)
# ④⁰ 지정 배치 — {(적 인덱스, 슬롯): (배포본의 (타입, lo, hi), 새 (타입, lo, hi), 이유)}. 배포본 값을 확인하고 바꾼다.
# ★ 2026-09-28 사용자 결정 「B」: 팬텀스타·파괴마의 등뼈(둔기 31·32, LV 70·73 영웅)는 원본부터 **배치 없는 신관**의
#   HARD/HELL 칸에만 있었다(원작에서도 몬스터로는 못 얻었다). 계획기에 맡기면 유령기사의 요르닐 칸을 넓혀 셋이 2 % 를
#   나눠 가지므로(요르닐 2 → 0.67 %), 전용 칸을 준다: 심해의 망령(해저터널 4구역, HARD 51 / HELL 71) 슬롯 4 — 원래의
#   전뇌가드의다리(둔기 29)는 왕도마뱀·천둥새의 같은 칸에서 계속 나온다. 영웅 폭 656 을 둘이 나눠 각 ≈1 %.
#   (★ 2026-10-02 부터 슬롯 3·4 는 HARD_GRADE_WIDTH — 영웅 0.5 % 를 둘이 나눠 각 ≈0.25 %. ★ 같은 날 저녁 드랍 티어: 둘은 종결이라
#   HARD 칸 0.1 % 를 나눠 각 0.05 % — 「거의 안 나오는」 쪽. HELL 에서는 보너스 칸 7 이 1 % 로 따로 준다.)
DROP_ASSIGN = {
    (20, 4): ((4, 29, 29), (4, 31, 32), "팬텀스타·파괴마의 등뼈 — 신관(배치 없음)만 떨구던 둘의 전용 칸"),
}

# ── ★ 2026-10-02 HELL 보너스 칸 (사용자: "헬 장비드랍이 하드코어보다 조금 더 좋아야해" ·
#    "hell모드에서 하드코어보다 장신구도 잘 나와야해") ────────────────────────────────────
# HARDCORE 와 HELL 은 처치 루틴의 **같은 창(2·3·4)을 같은 롤로** 굴린다(`bd.q()V` 의 분기는 `ag.i > 0` 하나 —
# HERO2_GUEST_RE.md §8.9 끝 「HARDCORE 와 HELL」). HELL 몬스터만 20 레벨 높아서, HELL 에서는 제 레벨(몬스터 LV −10
# 이상) 장비가 처치당 0.55 % 였다(HARDCORE 8.29 %). 그래서 드랍 게이트가 있는 적마다 **슬롯 둘을 덧붙인다**(슬롯 수
# 5 → 7, 레코드 +10 B):
#   슬롯 5 = HELL 레벨 장비 — HELL 몬스터 레벨 −HELL_LEVEL_DOWN .. +DROP_LEVEL_UP 안의, 같은 등급 · 연속 id 한 묶음.
#            타입은 그 적의 HARDCORE 칸(슬롯 3, 다음 4)의 타입을 먼저(같은 적이 같은 계열을 HELL 레벨로), 등급은
#            레어 → 에픽 → 영웅 → 일반 순(HELL 레벨에는 일반 등급 장비가 없다), 그다음 평균 레벨이 몬스터에 가까운 것 ·
#            긴 묶음.
#   슬롯 6 = 영웅 장신구 — 목걸이/반지 14..16 (LV 60 / 55, 게임의 최고 장신구; ★ 같은 날 14..19 — 아래). 그 적의 슬롯 4 가 장신구면 그 타입,
#            아니면 인덱스 짝수 목걸이 · 홀수 반지.
# 처치 루틴은 슬롯 0..4 만 읽으므로 NORMAL · HARDCORE 는 한 바이트도 안 바뀐다. 이 둘은 호스트 블록
# (`guest_anchors::HellDropPatch`)이 HELL 에서만 처치 루틴 **뒤에 한 번 더** 굴린다 — 롤 1..32767, 슬롯 5 = 0 < 롤 < 누적5,
# 슬롯 6 = 누적5 < 롤 < 누적6 (**자기 축**, 0 부터 — 슬롯 0..4 의 누적과 무관). HELL = HARDCORE 드랍 전부 + 이 롤.
# 폭(첫 판): 장비 9.000 %(등급과 무관 — HELL 레벨에는 레어 이상뿐이라 등급 폭이면 HARDCORE 보다 덜 나온다),
# 장신구 HELL_ACCESSORY_WIDTH = 영웅 폭 1.999 %. → 처치당 HELL 레벨 장비 ≈ HARDCORE 의 1.2 배, 장신구 ≈ 6 배
# (`decode_drops.py --bands`). 블록이 없는 빌드(배포본·`--h2-shipped`)는 슬롯 5·6 을 아무도 안 읽는다.
# ★ 2026-10-02 (같은 날) 스킬을 바꾸는 장신구 6종 — 사용자: *"목걸이 및 반지 드랍테이블에 추가"*. 슬롯 6 이 바로 HELL 의
#   목걸이/반지 표라 그 범위를 14..16 → **14..19** 로 넓히고(목걸이 17..19 = 바바리안의 의지 · 야수조련 목걸이 · 극사의 목걸이,
#   반지 17..19 = 광룡의 외침 · 크루세이더 · 벽력일섬 — ITM_ADDS) 폭을 **두 배**(656 → 1312)로 한다 → 아이템 하나의 확률은
#   전과 같다(656/3 = 1312/6 = 0.667 %/처치, 그 적의 종류 안에서). 새 셋도 등급 영웅이라 「한 등급 묶음」 검사가 그대로 선다.
# ★★ 2026-10-02 (같은 날, 위 GRADE_WIDTH 주석의 사용자 결정) — 폭을 사용자 수치로: 슬롯 5 = **등급별** HELL_GRADE_WIDTH
#   (레어 4 · 에픽 1.5 · 영웅 1 % — 위의 9 % 고정은 Claude 가 정한 값이었다), 슬롯 6 = HELL_ACCESSORY_WIDTH **1 %**(14..19 여섯이
#   나눈다 → 하나 0.167 %/처치). HARDCORE 와 같이 굴리는 슬롯 3·4 는 HARD_GRADE_WIDTH(레어 3 · 에픽 1 · 영웅 0.5 %) ·
#   장신구 0.5 % — HELL 은 그것에 이 롤을 더한다.
# ★★★ 2026-10-02 저녁 — 드랍 티어 (위 GRADE_WIDTH 주석의 「드랍 티어」): 보너스 칸이 **넷**이 됐다(n 5 → 9, 레코드 +20 B).
#   슬롯 5 = 상급 장비 — HELL 레벨 띠(몬스터 LV −10..+5) 안의 같은 등급 · 연속 id 한 묶음, **종결 티어는 뺀다**. 폭 = HELL_GRADE_WIDTH
#            (레어 4 · 에픽 1.5 · 영웅 1 % — 낮 수치 그대로).
#   슬롯 6 = 종결 장신구 14..16(영웅 목걸이/반지) — HELL_ACCESSORY_WIDTH **99 = 0.3 %**(낮 1 % 에서; HARD 는 0.1 %).
#   슬롯 7 = **종결 장비** — 띠 안의 영웅 LV 69+ 한 묶음, HELL_FINAL_WIDTH **328 = 1 %**(사용자: *"헬모드에서는 1%확률로"*). 띠에 종결이
#            없는 적(HELL LV 63 이하 = 원래 LV 9 이하)은 **타입 7(없음) · 폭 0** — 처치 루틴의 「없음」 타입 그대로, 블록도 그 칸을 건너뛴다.
#   슬롯 8 = **신규 6종** 17..19 — 타입 바이트에 HELL_GATE(0x80) 를 세운다: 호스트 블록이 **환경설정 난이도 NORMAL 이상일 때만**
#            굴린다(`HellDropPatch::GATE` / `gate_open`, 게이트 워드는 블록 안; ★ 2026-10-03 전엔 HARD 일 때만 — 사용자: *"헬 노말난이도부터"*).
#            폭 HELL_NEW_SIX_WIDTH **34 = 0.1 %**(사용자: *"0.1%수준"*). 게이트의 난이도는 코드 상수라 에셋은 그대로다.
#            ★ 2026-10-03 — 6종이 LV 70 이 되면서 칸 8 도 **레벨 규칙**(HELL 레벨 + DROP_LEVEL_UP ≥ 70)을 따른다(사용자: *"7마리에서 빼"*).
#            못 미치는 적(원래 LV 12 이하 = HELL LV 64 이하, 7 마리)은 칸 8 = **타입 7 + GATE · 0-0 · 폭 0** — 칸 수(9)와 게이트 비트는
#            그대로 두고 롤만 못 받는다(엄격한 두 경계라 폭 0 칸은 고를 수 없다 — `HellDropPatch::pick_slot`). 코드 변경 없음.
#   슬롯 6·8 의 타입(목걸이/반지)은 같다 — 그 적의 슬롯 4 가 장신구면 그 타입, 아니면 인덱스 짝수 목걸이 · 홀수 반지.
#   ⛔ 블록이 없는 빌드(배포본·`--h2-shipped`)와 7 칸 에셋 위의 새 블록은 전과 같이 동작한다(블록이 배열 끝까지 걷는다).
#   ★ 2026-10-09 — 슬롯 9 = **가디언 조합서** 61..91(GUARDIAN_RECIPES) — HELL 은 가디언보다 낮게(사용자가 고른 「HELL + 가디언 드랍」):
#            HELL_RECIPE_WIDTH 0.5 %, 31 장이 균등히 나눈다. 레벨 규칙은 장비의 것이라 이 칸에는 없다(원작 조합서 칸도 레벨과 무관 —
#            LV 3 도적이 0..20 을 떨군다). 블록의 HELL 패스가 칸 9 까지 걷는다(`HellDropPatch::LAST_HELL_SLOT` 8 → 9) — 가디언 칸은
#            한 칸씩 밀려 10..14(GUARDIAN_SLOTS), 그 끝 14 가 가디언 쪽 조합서 칸(GUARDIAN_RECIPE_WIDTH 2 %).
HELL_SLOTS = (5, 6, 7, 8, 9)
HELL_RECIPE_WIDTH = 164  # 슬롯 9 — 0.5 % (31 장 균등 = 장당 0.016 %)
GUARDIAN_RECIPE_WIDTH = 656  # 가디언 칸 14 — 2 % (장당 0.065 %; 가디언 모드는 HELL 패스도 굴리므로 합 2.5 %)
# ★★ 2026-10-02 밤 — 조금씩 올림(위 「낮춰 둔 폭을 조금씩 올림」): 레어 4 → 5 · 에픽 1.5 → 2 · 영웅 1 → 1.5 % · 종결 장신구 0.3 → 0.5 % ·
#   종결 장비 1 → 1.5 % · 신규 6종 0.1 → 0.15 %. 위 주석들의 옛 수치는 그때의 기록이다.
HELL_GRADE_WIDTH = {0: 3276, 1: 1640, 2: 656, 3: 492}  # 슬롯 5 (상급) 레어 5 · 에픽 2 · 영웅 1.5 % — 일반 = 쓰이지 않는다(HELL 레벨에 일반 장비가 없다)
HELL_ACCESSORY_WIDTH = 164  # 슬롯 6 — 0.5 %
HELL_FINAL_WIDTH = 492  # 슬롯 7 — 1.5 %
HELL_NEW_SIX_WIDTH = 50  # 슬롯 8 — 0.15 %
HELL_GATE = 0x80  # = guest_anchors::HellDropPatch::GATE
HELL_ACCESSORY_TYPES = (11, 12)  # 목걸이 · 반지
HELL_ACCESSORY_IDS = (14, 16)
HELL_NEW_SIX_IDS = (17, 19)
HELL_LEVEL_DOWN = 10
HELL_GRADE_ORDER = (1, 2, 3, 0)  # 레어 → 에픽 → 영웅 → 일반
SLOT_TYPE_NONE = 7  # bd.q()V @0x1283d6 — 「없음」(itm/07 은 없다)


def tdf_decode(blob, name=GM_TDF):
    count, i, rows = blob[0], 1, []
    for _ in range(count):
        (n,) = struct.unpack(">H", blob[i : i + 2])
        rows.append(blob[i + 2 : i + 2 + n])
        i += 2 + n
    if i != len(blob):
        raise SystemExit(f"{name}: {len(blob) - i} B of slack — not the format this tool knows")
    return rows


def tdf_encode(rows, name=GM_TDF):
    if len(rows) > 255:
        raise SystemExit(f"{name}: {len(rows)} rows do not fit the 1-byte count")
    out = bytearray([len(rows)])
    for r in rows:
        out += struct.pack(">H", len(r)) + r
    return bytes(out)


def patch_gm(rows):
    if len(rows) != GM_SHIPPED_ROWS:
        raise SystemExit(f"{GM_TDF}: {len(rows)} rows, expected {GM_SHIPPED_ROWS} (KTF 1.0.7)")
    if rows[11] != b"CARD" or rows[19] != "카드".encode("euc-kr"):
        raise SystemExit(f"{GM_TDF}: rows 11/19 are not CARD/카드 — is this the shipped table?")
    rows = list(rows)
    for k, text in GM_REPLACE.items():
        rows[k] = text.encode("euc-kr")
    rows += [t.encode("euc-kr") for t in GM_APPEND]
    return rows


def patch_com(rows):
    if len(rows) != COM_SHIPPED_ROWS:
        raise SystemExit(f"{COM_TDF}: {len(rows)} rows, expected {COM_SHIPPED_ROWS} (KTF 1.0.7)")
    rows = list(rows)
    for k, (old, new) in list(COM_REPLACE.items()) + list(COM_GUARDIAN.items()):
        if rows[k] != old.encode("euc-kr"):
            raise SystemExit(f"{COM_TDF}: row {k} is not {old!r} — is this the shipped table?")
        rows[k] = new.encode("euc-kr")
    # ★ 2026-10-08 가디언: the fee text [13] → [56], [13] ← GUARDIAN.
    fee = rows[COM_FEE_ROW]
    if not fee.startswith("본 게임은".encode("euc-kr")):
        raise SystemExit(f"{COM_TDF}: row {COM_FEE_ROW} is not the 정보이용료 fee text — is this the shipped table?")
    if len(rows) != COM_FEE_NEW_ROW:
        raise SystemExit(f"{COM_TDF}: the fee text would land at row {len(rows)}, the code expects {COM_FEE_NEW_ROW}")
    rows[COM_FEE_ROW] = COM_MODE3_LABEL.encode("euc-kr")
    rows.append(fee)
    return rows


def patch_skill_text(name, rows):
    """스킬 표 `name` 의 SKILL_TEXT 행을 바꾼다. 행 수와 바꿀 행의 배포본 값을 확인한다 — sk0 은 [16] 근력단련 이름도."""
    shipped, replace, append = SKILL_TEXT[name]
    if len(rows) != shipped:
        raise SystemExit(f"{name}: {len(rows)} rows, expected {shipped} (KTF 1.0.7)")
    if name == SK0_TDF and rows[16] != "근력단련".encode("euc-kr"):
        raise SystemExit(f"{SK0_TDF}: row 16 is not 근력단련 — is this the shipped table?")
    rows = list(rows)
    for k, (old, new) in replace.items():
        if rows[k] != old.encode("euc-kr"):
            raise SystemExit(f"{name}: row {k} is not {old!r} — is this the shipped table?")
        rows[k] = new.encode("euc-kr")
    return rows + [t.encode("euc-kr") for t in append]


def patch_itmtp(rows):
    """`t/itmtp.tdf` 뒤에 ITMTP_APPEND 를 붙인다. 배포본인지 두 행으로 확인한다."""
    if len(rows) != ITMTP_SHIPPED_ROWS:
        raise SystemExit(f"{ITMTP_TDF}: {len(rows)} rows, expected {ITMTP_SHIPPED_ROWS} (KTF 1.0.7)")
    if rows[7] != "빈공간이 없습니다.".encode("euc-kr") or rows[21] != "마법구".encode("euc-kr"):
        raise SystemExit(f"{ITMTP_TDF}: rows 7/21 are not the shipped 빈공간이 없습니다./마법구")
    return list(rows) + [t.encode("euc-kr") for t in ITMTP_APPEND]


def itm_records(name, blob):
    """[u16 BE len][payload] * N, EOF 에 정확히 착지. 오프셋을 함께 돌려준다."""
    out, i = [], 0
    while i + 2 <= len(blob):
        (n,) = struct.unpack_from(">H", blob, i)
        if i + 2 + n > len(blob):
            raise SystemExit(f"{name}: record at {i} overruns EOF (len={n})")
        out.append((i + 2, blob[i + 2 : i + 2 + n]))
        i += 2 + n
    if i != len(blob):
        raise SystemExit(f"{name}: {len(blob) - i} B of slack after the last record")
    return out


def armour_sites(name, blob):
    """(키, body[23] 의 파일 내 절대 오프셋, 레코드가 말하는 이름) 목록.

    ⛔ 걷기와 쓰기를 나눠 둔 것은 일부러다 — 패치한 결과를 **같은 걷기로** 다시 읽어
    확인하기 위해서다. 쓰는 쪽이 현재 값을 검사하므로, 걷는 쪽이 검사하면 확인 패스가
    「이미 고쳐진 값」에 걸려 죽는다.
    """
    sites = []
    if name == ITM_SHOP:
        # [u8 recLen][u16 BE 전역인덱스][itm 레코드와 바이트 동일한 payload]
        i, seen = 0, 0
        while i < len(blob):
            n = blob[i]
            rec = blob[i + 1 : i + 1 + n]
            if len(rec) != n:
                raise SystemExit(f"{ITM_SHOP}: record at {i} overruns EOF (len={n})")
            (gidx,) = struct.unpack_from(">H", rec, 0)
            key = (gidx >> 8, gidx & 0xFF)
            if key in ARMOUR_FIXES:
                sites.append(_site(key, i + 3, rec[2:]))
            i += 1 + n
            seen += 1
        # 배포본이거나(134) 이 도구가 SHOP_ADDS 를 넣은 뒤(136)여야 한다 — 확인 패스가
        # 패치된 목록으로 같은 걷기를 다시 부른다.
        if seen not in (SHOP_RECORDS, SHOP_RECORDS + len(SHOP_ADDS)):
            raise SystemExit(f"{ITM_SHOP}: {seen} records, expected {SHOP_RECORDS} (KTF 1.0.7)")
        return sites

    tid = int(name[len("itm/") :])
    recs = itm_records(name, blob)
    for i in sorted(k for t, k in ARMOUR_FIXES if t == tid):
        if i >= len(recs):
            raise SystemExit(f"{name}: only {len(recs)} records, need index {i}")
        base, payload = recs[i]
        sites.append(_site((tid, i), base, payload))
    return sites


def _site(key, base, payload):
    """payload = [u32 LE price][u8 nameLen][name][body] — body[23] 은 여기다."""
    n = payload[4]
    return key, base + 5 + n + ARMOUR_CLASS_OFF, payload[5 : 5 + n].decode("euc-kr", "replace")


def patch_armour(name, blob):
    """해당 레코드들의 body[23] 을 제자리에서 뒤집는다. 이름과 현 값을 둘 다 확인한다."""
    out = bytearray(blob)
    sites = armour_sites(name, blob)
    for key, off, got in sites:
        expect, old, new = ARMOUR_FIXES[key]
        if got != expect:
            raise SystemExit(f"{name}: {key} is {got!r}, expected {expect!r} — wrong archive?")
        if out[off] != old:
            raise SystemExit(f"{name}: {key} {expect} body[23]={out[off]}, expected {old}")
        out[off] = new
    return bytes(out), len(sites)


def itm_encode(records):
    out = bytearray()
    for rec in records:
        out += struct.pack(">H", len(rec)) + rec
    return bytes(out)


def append_records(name, blob):
    """`itm/NN` 뒤에 ITM_ADDS 의 레코드를 붙인다. 인덱스는 반드시 이어져야 한다."""
    tid = int(name[len("itm/") :])
    adds = [a for a in ITM_ADDS if a[0] == tid]
    if not adds:
        return blob, 0
    recs = [bytes(r) for _off, r in itm_records(name, blob)]
    for _t, idx, label, price, desc, tail in sorted(adds, key=lambda a: a[1]):
        if idx != len(recs):
            raise SystemExit(f"{name}: next index is {len(recs)}, ITM_ADDS wants {idx} ({label})")
        nm, ds = label.encode("euc-kr"), desc.encode("euc-kr")
        if len(nm) > 255 or len(ds) > 255:
            raise SystemExit(f"{name}[{idx}]: {label!r} does not fit a 1-byte length")
        recs.append(struct.pack("<I", price) + bytes([len(nm)]) + nm + bytes([len(ds)]) + ds + tail)
    return itm_encode(recs), len(adds)


def patch_price(name, blob):
    """`itm/NN` 의 `u32 LE price` 를 바꾼다. 이름과 현재 가격을 둘 다 확인한다."""
    tid = int(name[len("itm/") :])
    recs = [bytearray(r) for _off, r in itm_records(name, blob)]
    done = 0
    for (t, i), (expect, old, new) in PRICE_FIXES.items():
        if t != tid:
            continue
        if i >= len(recs):
            raise SystemExit(f"{name}: only {len(recs)} records, need index {i}")
        rec = recs[i]
        n = rec[4]
        got = rec[5 : 5 + n].decode("euc-kr", "replace")
        (price,) = struct.unpack_from("<I", rec, 0)
        if got != expect or price != old:
            raise SystemExit(f"{name}[{i}]: {got!r} @{price}, expected {expect!r} @{old}")
        struct.pack_into("<I", rec, 0, new)
        done += 1
    return itm_encode(bytes(r) for r in recs), done


def add_to_shop(blob, source_records):
    """`itm/forshop` 에 새 레코드를 넣는다. 파일은 전역 인덱스 오름차순으로 유지한다.

    레코드 = [u8 recLen][u16 BE 전역인덱스][그 itm 레코드와 바이트 동일한 payload].
    """
    rows, i = [], 0
    while i < len(blob):
        n = blob[i]
        rec = blob[i + 1 : i + 1 + n]
        if len(rec) != n:
            raise SystemExit(f"{ITM_SHOP}: record at {i} overruns EOF (len={n})")
        (gidx,) = struct.unpack_from(">H", rec, 0)
        rows.append((gidx, rec[2:]))
        i += 1 + n
    if len(rows) != SHOP_RECORDS:
        raise SystemExit(f"{ITM_SHOP}: {len(rows)} records, expected {SHOP_RECORDS} (KTF 1.0.7)")
    if [g for g, _ in rows] != sorted(g for g, _ in rows):
        raise SystemExit(f"{ITM_SHOP}: not sorted by global index — the insert would break the order")

    have = {g for g, _ in rows}
    for t, idx in SHOP_ADDS:
        gidx = (t << 8) | idx
        if gidx in have:
            raise SystemExit(f"{ITM_SHOP}: 0x{gidx:04x} is already on sale — is this the shipped list?")
        payload = source_records[(t, idx)]
        if 2 + len(payload) > 255:
            raise SystemExit(f"{ITM_SHOP}: 0x{gidx:04x} record is {2 + len(payload)} B, over the 1-byte length")
        rows.append((gidx, payload))
    rows.sort(key=lambda r: r[0])
    for (t, idx), (at, aidx) in SHOP_PLACE_AFTER.items():
        moved = [r for r in rows if r[0] == (t << 8) | idx]
        rows = [r for r in rows if r[0] != (t << 8) | idx]
        anchor = [k for k, r in enumerate(rows) if r[0] == (at << 8) | aidx]
        if len(moved) != 1 or len(anchor) != 1:
            raise SystemExit(f"{ITM_SHOP}: cannot place 0x{(t << 8) | idx:04x} after 0x{(at << 8) | aidx:04x}")
        rows.insert(anchor[0] + 1, moved[0])

    out = bytearray()
    for gidx, payload in rows:
        rec = struct.pack(">H", gidx) + payload
        out += bytes([len(rec)]) + rec
    return bytes(out), len(SHOP_ADDS)


def enm_records(blob):
    """`enm/enm` → [(index, name, body_start, body)]. EOF 에 정확히 착지해야 한다.

    레코드 = [u16 BE 길이][버전 4B][u8 nameLen][name EUC-KR][body], body = 36 + 5n.
    """
    out, i = [], 0
    while i < len(blob):
        (n,) = struct.unpack_from(">H", blob, i)
        rec = blob[i + 2 : i + 2 + n]
        if len(rec) != n:
            raise SystemExit(f"{ENM}: record {len(out)} overruns EOF")
        nl = rec[4]
        body_start = i + 2 + 5 + nl
        body = blob[body_start : i + 2 + n]
        cnt = body[33]
        if len(body) != 36 + 5 * cnt:
            raise SystemExit(f"{ENM}: record {len(out)} body {len(body)} B but n={cnt}")
        out.append((len(out), rec[5 : 5 + nl].decode("euc-kr", "replace"), body_start, body))
        i += 2 + n
    if len(out) != ENM_RECORDS:
        raise SystemExit(f"{ENM}: {len(out)} records, expected {ENM_RECORDS} (KTF 1.0.7)")
    return out


def item_levels(itm_blobs):
    """({(type, id): (요구레벨, 등급, 세트 id)} for gear, {every (type, id)}) from the itm files.
    (★ 2026-10-02 저녁: the set id is the third — `tier_of` reads it; the callers index [0] · [1].)"""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from decode_hero2_items import accessory_body, decode_accessory, decode_equipment, equipment_body, records, split_head  # noqa: E402

    gear, exists = {}, set()
    for name, blob in itm_blobs.items():
        tid = int(name[len("itm/") :])
        for idx, rec in enumerate(records(blob)):
            exists.add((tid, idx))
            _price, _nm, body = split_head(rec)
            if tid in (0, 1, 2, 3, 4) and equipment_body(body, True) is not None:  # ★ 2026-10-14: 옵션 문구(설명) 있는 장비도
                d = decode_equipment(body, weapon=True)
            elif tid in (5, 6, 8, 9, 10) and equipment_body(body, False) is not None:
                d = decode_equipment(body, weapon=False)
            elif tid in (11, 12) and accessory_body(body) is not None:  # ★ 2026-10-02: 설명 있는 장신구도
                d = decode_accessory(body)
            else:
                continue
            gear[(tid, idx)] = (d["level"], d["grade"], d.get("setId", 0))
    return gear, exists


def gear_tier(gear, key):
    """`tier_of` of the item `key` = (type, id) in the `item_levels` table."""
    lv, grade, set_id = gear[key]
    return tier_of(key[0], lv, grade, set_id)


def _mode_level(level, mode):
    """`cq.a()V`: 0 NORMAL = 그대로, 1 HARDCORE / 2 HELL = 2·lv/5 + 20·(모드>1 ? 2 : 1) + 20 (상한 99); ★ 2026-10-08 3 GUARDIAN =
    HELL 60..80 을 70..105 로 늘인 것 — `guest_anchors::GuardianPatch` 의 래퍼."""
    if mode == 0:
        return level
    hell = min(level * 2 // 5 + 20 * (2 if mode > 1 else 1) + 20, 99)
    return min(70 + (max(hell - 60, 0) * 448 >> 8), 105) if mode >= 3 else hell  # ★ 저녁: 60..80 → 70..105


def _slot_refs(level, k):
    """The enemy levels slot k is rolled at (`bd.q()V`'s SHIPPED window, 2026-09-25).

    NORMAL rolls slots 0·1·2 at the file level; HARDCORE / HELL roll 2·3·4 at the raised
    `cq.a()V` level. Slot 2 rolls in every mode, so it must suit NORMAL — the lowest.
    """
    if k in HARD_ONLY_SLOTS:
        return (_mode_level(level, 1), _mode_level(level, 2))
    return (level,)


def plan_drops(enemies, gear, exists, placed):
    """The new slot table, `{enemy index: [[type byte, lo, hi, width], ...]}`, and a log.

    Pure: the same records and items always give the same table (sorted iteration, ties
    broken by enemy index then slot). See the DROP PLAN notes above for the steps.
    `placed` = the enemies some map spawns (`map_monsters.placed`): only they count as a
    source, and only their slots are candidates — an enemy no map lists never drops anything.
    """
    eff = lambda t: SLOT_TYPE_REMAP.get(t, t)  # noqa: E731
    gear = {key: v for key, v in gear.items() if key not in OUT_OF_WINDOWS}  # HELL 칸 8 · 가디언 칸 전용 — 이 창들의 몫이 아니다
    by_idx = {i: (name, body) for i, name, _s, body in enemies}
    table = {}
    for idx, _name, _start, body in enemies:
        gate = struct.unpack_from("<h", body, 15)[0]
        if gate == 0 or body[33] != ENM_SLOTS:
            continue
        rows, prev = [], 0
        for k in range(ENM_SLOTS):
            o = ENM_SLOT_OFF + 5 * k
            cum = struct.unpack_from("<h", body, o + 3)[0]
            rows.append([body[o], body[o + 1], body[o + 2], cum - prev])
            prev = cum
        table[idx] = rows
    log = []
    level = lambda idx: by_idx[idx][1][4]  # noqa: E731
    refs = lambda idx, k: _slot_refs(level(idx), k)  # noqa: E731
    dist = lambda idx, k, lv: min(abs(lv - r) for r in refs(idx, k))  # noqa: E731
    ceiling = lambda idx, k: max(refs(idx, k)) + DROP_LEVEL_UP  # noqa: E731

    def fits(idx, k, lv, down=DROP_LEVEL_DOWN):
        return any(r - down <= lv <= r + DROP_LEVEL_UP for r in refs(idx, k))

    def coverage(slots):
        c = {}
        for idx, rows in table.items():
            if idx not in placed:
                continue  # 어느 맵에도 없는 적의 칸은 아무도 굴리지 않는다 (2026-09-28)
            for k, (t, lo, hi, w) in enumerate(rows):
                if k in slots and w >= 2:  # a width-1 slot can never be hit: both comparisons are strict
                    for i in range(lo, hi + 1):
                        c[(eff(t), i)] = c.get((eff(t), i), 0) + 1
        return c

    # ⓪ 레벨 규칙 위반을 잘라낸다 — NORMAL 창의 장비 슬롯에 적 레벨 + DROP_LEVEL_UP 을 넘는 id 가 있으면
    #    그 범위를 규칙을 지키는 가장 긴 연속 구간으로 줄인다(배포본에서는 스켈레톤[29] 의 반지 0-16 하나:
    #    LV 22 적이 LV 55 반지를 떨군다).
    for idx in sorted(table):
        for k in NORMAL_SLOTS:
            r = table[idx][k]
            t = eff(r[0])
            if t not in GEAR_TYPES:
                continue
            top = ceiling(idx, k)
            ok = [j for j in range(r[1], r[2] + 1) if gear[(t, j)][0] <= top]
            if len(ok) == r[2] - r[1] + 1:
                continue
            best, run = None, []
            for j in range(r[1], r[2] + 2):
                if j <= r[2] and gear[(t, j)][0] <= top:
                    run.append(j)
                    continue
                if run and (best is None or len(run) > len(best)):
                    best = run
                run = []
            if best is None:
                raise SystemExit(f"{ENM}: [{idx}] slot {k} t{t} {r[1]}-{r[2]} has no item at or under LV {top}")
            log.append(f"자름   [{idx:3}] {by_idx[idx][0]} 슬롯{k}: t{r[0]} {r[1]}-{r[2]} -> {best[0]}-{best[-1]}  (LV{level(idx)} 적에 LV{top} 초과 제외)")
            r[1], r[2] = best[0], best[-1]
    # ① 인장 하급·중급
    for idx in sorted(table):
        for k, r in enumerate(table[idx]):
            if eff(r[0]) in SEAL_TYPES and (r[1], r[2]) == (2, 2):
                r[1] = 0
                log.append(f"인장   [{idx:3}] {by_idx[idx][0]} 슬롯{k}: t{r[0]} 2-2 -> 0-2")
    # ② 조합서 58..60
    for idx in sorted(table):
        for k, r in enumerate(table[idx]):
            if eff(r[0]) == RECIPE_TYPE and r[2] == RECIPE_HI_FROM:
                r[2] = RECIPE_HI_TO
                log.append(f"조합서 [{idx:3}] {by_idx[idx][0]} 슬롯{k}: {r[1]}-{RECIPE_HI_FROM} -> {r[1]}-{RECIPE_HI_TO}")
    # ③ 마력구 — 없앴다(네트워크 전용, 2026-09-27; NO_DROP_TYPES)
    # ④⁰ 지정 배치 (DROP_ASSIGN) — 배포본 값 · 레벨 규칙 · 밀려나는 아이템의 다른 출처를 확인하고 바꾼다.
    for (idx, k), (was, new, why) in sorted(DROP_ASSIGN.items()):
        if idx not in table or idx not in placed:
            raise SystemExit(f"{ENM}: DROP_ASSIGN [{idx}] is not a drop-gated enemy some map spawns")
        r = table[idx][k]
        if (eff(r[0]), r[1], r[2]) != was:
            raise SystemExit(f"{ENM}: DROP_ASSIGN [{idx}] {by_idx[idx][0]} slot {k} is t{r[0]} {r[1]}-{r[2]}, expected t{was[0]} {was[1]}-{was[2]} — wrong archive?")
        t, lo, hi = new
        for j in range(lo, hi + 1):
            lv = gear[(t, j)][0]
            if not fits(idx, k, lv) or lv > ceiling(idx, k):
                raise SystemExit(f"{ENM}: DROP_ASSIGN [{idx}] slot {k}: {t}:{j} LV{lv} breaks the level rule (enemy {refs(idx, k)})")
        # 이 칸이 굴려지는 창마다(0·1·2 = NORMAL, 2·3·4 = HARDCORE/HELL) 밀려나는 아이템이 다른 배치된 적에게서도 나와야 한다.
        for window in [w for w in (NORMAL_SLOTS, HARD_WINDOW) if k in w]:
            cov = coverage(window)
            for j in range(r[1], r[2] + 1):
                if cov.get((eff(r[0]), j), 0) < 2:
                    raise SystemExit(f"{ENM}: DROP_ASSIGN [{idx}] slot {k}: {eff(r[0])}:{j} would stop dropping in window {window} — pick another slot")
        log.append(f"지정   [{idx:3}] {by_idx[idx][0]} 슬롯{k}: t{r[0]} {r[1]}-{r[2]} -> t{t} {lo}-{hi}  ({why}, 적 {'/'.join(map(str, refs(idx, k)))})")
        table[idx][k] = [t, lo, hi, r[3]]
    # ④ 장비 — 창마다 따로. NORMAL 창(0·1)은 NORMAL 적의 최고 레벨 이하 장비 전부, HARD 창(3·4)은 그 위.
    top_normal = max(level(idx) for idx in table)
    need = {
        "N": sorted(key for key, (lv, _g, _s) in gear.items() if lv <= top_normal),
        "H": sorted(key for key, (lv, _g, _s) in gear.items() if lv > top_normal),
    }
    left = []
    for cls, window, gear_slots in (("N", NORMAL_SLOTS, (0, 1)), ("H", HARD_WINDOW, HARD_ONLY_SLOTS)):
        cov = coverage(window)
        missing = [key for key in need[cls] if key not in cov]
        for maxlen, spread in ((4, 10), (6, 16)):
            for key in list(missing):
                t, i = key
                lv, grade, _set = gear[key]
                best = None
                for idx in sorted(table):
                    if idx not in placed:
                        continue
                    for k in gear_slots:
                        r = table[idx][k]
                        if eff(r[0]) != t or r[3] < 2 or not fits(idx, k, lv):
                            continue
                        lo, hi = min(r[1], i), max(r[2], i)
                        ids = range(lo, hi + 1)
                        if len(ids) > maxlen or any((t, j) not in gear for j in ids):
                            continue
                        if any(gear[(t, j)][1] != grade for j in ids):
                            continue
                        lvs = [gear[(t, j)][0] for j in ids]
                        if max(lvs) - min(lvs) > spread or max(lvs) > ceiling(idx, k):
                            continue
                        score = (dist(idx, k, lv) + 2 * ((hi - lo) - (r[2] - r[1])), idx, k)
                        if best is None or score < best[0]:
                            best = (score, idx, k, lo, hi)
                if best is not None:
                    _s, idx, k, lo, hi = best
                    r = table[idx][k]
                    log.append(f"넓힘{cls}  [{idx:3}] {by_idx[idx][0]} 슬롯{k}: t{t} {r[1]}-{r[2]} -> {lo}-{hi}  (+{t}:{i} LV{lv}, 적 {'/'.join(map(str, refs(idx, k)))})")
                    r[1], r[2] = lo, hi
                    missing.remove(key)
        cov = coverage(window)
        used = set()
        for key in list(missing):
            t, i = key
            lv, grade, _set = gear[key]
            best = None
            # A slot near the item's level first; failing that, any slot whose enemy is not
            # BELOW the item by more than the rule allows (a low item from a strong enemy is
            # fine — a strong item from a weak one is the bug this plan exists to prevent).
            for down in (DROP_LEVEL_DOWN, 99):
                for idx in sorted(table):
                    if idx not in placed:
                        continue
                    for k in gear_slots:
                        r = table[idx][k]
                        rt = eff(r[0])
                        if rt not in GEAR_TYPES or (idx, k) in used or r[3] < 2 or not fits(idx, k, lv, down):
                            continue
                        if any(cov.get((rt, j), 0) < 2 for j in range(r[1], r[2] + 1)):
                            continue
                        # Prefer a slot of the same TYPE and the same GRADE: the enemy's drop
                        # profile then keeps its shape (a rare slot stays a rare slot).
                        sg = gear.get((rt, r[1]), (0, grade))[1]
                        score = (dist(idx, k, lv) + (0 if rt == t else 5) + 3 * abs(sg - grade), idx, k)
                        if best is None or score < best[0]:
                            best = (score, idx, k)
                if best is not None:
                    break
            if best is None:
                left.append((cls, key))
                continue
            _s, idx, k = best
            r = table[idx][k]
            for j in range(r[1], r[2] + 1):
                cov[(eff(r[0]), j)] -= 1
            width = tier_width(gear, t, i, i, k)
            log.append(f"돌려씀{cls} [{idx:3}] {by_idx[idx][0]} 슬롯{k}: t{r[0]} {r[1]}-{r[2]} {r[3]} -> t{t} {i}-{i} {width}  (LV{lv} 등급{grade} {TIER_NAME[gear_tier(gear, key)]}, 적 {'/'.join(map(str, refs(idx, k)))})")
            table[idx][k] = [t, i, i, width]
            cov[(t, i)] = cov.get((t, i), 0) + 1
            used.add((idx, k))
    # ④ᵇ 세트 조각의 출처 보강 (2026-10-02 저녁, 사용자: *"이것저것 많이 껴볼수 있게"*): 중간 티어 세트 조각마다 그 창에서
    #    떨구는 **배치된 적**이 SET_SOURCES 보다 적으면, ④ 의 (A)/(B) 규칙 그대로 같은 타입·같은 등급 슬롯의 id 범위를 넓혀
    #    출처를 더한다 — 이미 그 조각을 떨구는 적은 건너뛴다. 못 채우면 로그에 남긴다(규칙은 안 깬다).
    for cls, window, gear_slots, min_level in (("N", NORMAL_SLOTS, (0, 1), 0), ("H", HARD_WINDOW, HARD_ONLY_SLOTS, SET_SOURCES_HARD_LEVEL)):
        pieces = sorted(key for key, (lv, _g, set_id) in gear.items() if SET_TIER.get(set_id) == TIER_MID and lv >= min_level and (cls == "H" or lv <= top_normal))
        for key in pieces:
            t, i = key
            lv, grade, _set = gear[key]
            while True:
                cov = coverage(window)
                have = cov.get(key, 0)
                if have >= SET_SOURCES[cls]:
                    break
                best = None
                for maxlen, spread in ((4, 10), (6, 16)):
                    for idx in sorted(table):
                        if idx not in placed:
                            continue
                        for k in gear_slots:
                            r = table[idx][k]
                            if eff(r[0]) != t or r[3] < 2 or not fits(idx, k, lv) or r[1] <= i <= r[2]:
                                continue
                            lo, hi = min(r[1], i), max(r[2], i)
                            ids = range(lo, hi + 1)
                            if len(ids) > maxlen or any((t, j) not in gear for j in ids):
                                continue
                            if any(gear[(t, j)][1] != grade for j in ids):
                                continue
                            lvs = [gear[(t, j)][0] for j in ids]
                            if max(lvs) - min(lvs) > spread or max(lvs) > ceiling(idx, k):
                                continue
                            score = (dist(idx, k, lv) + 2 * ((hi - lo) - (r[2] - r[1])), idx, k)
                            if best is None or score < best[0]:
                                best = (score, idx, k, lo, hi)
                    if best is not None:
                        break
                if best is None:
                    log.append(f"세트{cls}   t{t}:{i} LV{lv} — 출처 {have}/{SET_SOURCES[cls]}, 더 넓힐 슬롯이 없다")
                    break
                _s, idx, k, lo, hi = best
                r = table[idx][k]
                log.append(f"세트{cls}  [{idx:3}] {by_idx[idx][0]} 슬롯{k}: t{t} {r[1]}-{r[2]} -> {lo}-{hi}  (+{t}:{i} LV{lv} 출처 {have + 1}/{SET_SOURCES[cls]}, 적 {'/'.join(map(str, refs(idx, k)))})")
                r[1], r[2] = lo, hi
    # ④ᶜ 종결 분리 (2026-10-02 저녁): 한 슬롯의 id 는 **균등**하게 뽑히므로(`h.a(lo, hi)`), 종결 티어가 중간·상급과 한 칸에 있으면
    #    폭의 평균을 줘도 종결 조각이 제 몫(0.1 %)보다 훨씬 자주 나온다(배포본 반지 0-16 · 목걸이 9-16 = 영웅 14..16 이 0.22 %).
    #    그래서 종결이 섞인 창 슬롯은 종결 id 를 떼어 낸다 — 떨어져 나가는 id 마다 그 창의 **다른 배치된 적**이 있어야 한다
    #    (없으면 거절: 그 조각이 그 창에서 사라진다). 남는 범위는 종결이 아닌 쪽의 가장 긴 연속 구간이다.
    for window, slots in ((NORMAL_SLOTS, (0, 1, 2)), (HARD_WINDOW, HARD_ONLY_SLOTS)):
        cov = coverage(window)
        for idx in sorted(table):
            if idx not in placed:
                continue
            for k in slots:
                r = table[idx][k]
                t = eff(r[0])
                if t not in GEAR_TYPES or r[3] < 2:
                    continue
                tiers = [gear_tier(gear, (t, j)) == TIER_FINAL for j in range(r[1], r[2] + 1)]
                if all(tiers) or not any(tiers):
                    continue
                best, run = None, []
                for j, is_final in zip(range(r[1], r[2] + 2), tiers + [True]):
                    if not is_final:
                        run.append(j)
                        continue
                    if run and (best is None or len(run) > len(best)):
                        best = run
                    run = []
                dropped = [j for j in range(r[1], r[2] + 1) if j not in best]
                short = [j for j in dropped if cov.get((t, j), 0) < 2]
                if short:
                    raise SystemExit(f"{ENM}: [{idx}] slot {k} t{t} {r[1]}-{r[2]} mixes 종결 with other tiers and {t}:{short} has no other source in window {window}")
                for j in dropped:
                    cov[(t, j)] -= 1
                log.append(f"분리{'N' if window is NORMAL_SLOTS else 'H'}  [{idx:3}] {by_idx[idx][0]} 슬롯{k}: t{t} {r[1]}-{r[2]} -> {best[0]}-{best[-1]}  (종결 {t}:{dropped} 을 뗀다)")
                r[1], r[2] = best[0], best[-1]
    # ⑤ 티어별 폭 (2026-10-02 저녁 — 09-26 의 등급 폭을 대신한다; #21 의 ×2 는 일반 칸에 그대로), 합이 32767 을 넘으면 장비 폭만 줄인다
    by_tier = {}
    for idx in sorted(table):
        rows = table[idx]
        for k, r in enumerate(rows):
            if eff(r[0]) in GEAR_TYPES and r[3] >= 2:  # < 2 = a slot the roll never reaches: the shipped game's off switch
                w = tier_width(gear, eff(r[0]), r[1], r[2], k)
                if w is None:
                    raise SystemExit(f"{ENM}: [{idx}] slot {k} t{r[0]} {r[1]}-{r[2]} names gear the item DB could not grade")
                tiers = {gear_tier(gear, (eff(r[0]), j)) for j in range(r[1], r[2] + 1)}
                key = tiers.pop() if len(tiers) == 1 else "섞임"
                by_tier[key] = by_tier.get(key, 0) + 1
                r[3] = w
        total = sum(r[3] for r in rows)
        if total > DROP_ROLL_MAX:
            gear_sum = sum(r[3] for r in rows if eff(r[0]) in GEAR_TYPES)
            room = DROP_ROLL_MAX - (total - gear_sum)
            for r in rows:
                if eff(r[0]) in GEAR_TYPES:
                    r[3] = r[3] * room // gear_sum
            log.append(f"맞춤   [{idx:3}] {by_idx[idx][0]}: 합 {total} > {DROP_ROLL_MAX} — 장비 폭 x{room / gear_sum:.3f}")
    shares = " · ".join(f"{TIER_NAME[g]} {by_tier[g]}" for g in TIER_NAME if g in by_tier)
    log.append(f"티어폭 장비 슬롯 {sum(by_tier.values())}개: {shares} · 섞임 {by_tier.get('섞임', 0)} (폭 NORMAL {NORMAL_TIER_WIDTH} · HARD 칸 {HARD_TIER_WIDTH})")
    return table, left, log


GRADE_NAME = {0: "일반", 1: "레어", 2: "에픽", 3: "영웅"}  # t/itmtp.tdf[35..38]


def item_width(gear, key, window):
    """The width one item gets in window "N" (slots 0·1·2) or "H" (slots 3·4): its tier's — and the grade's inside
    the tier (WINDOW_TIER_WIDTH). A pairing the table lacks (a 종결 영웅 in NORMAL …) falls to the tier's first row."""
    table = WINDOW_TIER_WIDTH[window]
    tier, grade = gear_tier(gear, key), gear[key][1]
    if (tier, grade) in table:
        return table[(tier, grade)]
    return next(w for (tr, _g), w in table.items() if tr == tier)


def tier_width(gear, t, lo, hi, k):
    """The width gear slot `k` holding `t lo..hi` gets (★ 2026-10-02 저녁 — the tier table instead of the grade
    table): the mean of its items' widths in the window that rolls the slot ("N" for the NORMAL slots 0·1·2, "H" for
    the HARDCORE/HELL-only 3·4), so a one-tier slot gets exactly that width and each item of a mixed slot keeps
    its share. `None` when an id is not graded gear (the caller refuses the table)."""
    window = "H" if k in HARD_ONLY_SLOTS else "N"
    ws = [item_width(gear, (t, j), window) for j in range(lo, hi + 1) if (t, j) in gear]
    if len(ws) != hi - lo + 1:
        return None
    return round(sum(ws) / len(ws))


def patch_enm(blob, itm_blobs, placed):
    """`enm/enm` 의 슬롯 바이트만 제자리에서 다시 쓴다. 레코드 길이·다른 바이트는 그대로."""
    enemies = enm_records(blob)
    gear, exists = item_levels(itm_blobs)
    table, missing, log = plan_drops(enemies, gear, exists, placed)
    if missing:
        raise SystemExit(f"{ENM}: {len(missing)} gear item(s) could not be placed by the level rule: {missing[:8]}…")
    out = bytearray(blob)
    for idx, _name, start, _body in enemies:
        if idx not in table:
            continue
        cum = 0
        for k, (t, lo, hi, w) in enumerate(table[idx]):
            cum += w
            if not 0 <= cum <= DROP_ROLL_MAX:
                raise SystemExit(f"{ENM}: [{idx}] slot {k} cumulative {cum} out of range")
            o = start + ENM_SLOT_OFF + 5 * k
            out[o : o + 5] = bytes([t, lo, hi]) + struct.pack("<h", cum)
    return bytes(out), table, log


def verify_enm(old, new, itm_blobs, placed):
    """The patched table parses, only slot bytes moved, cums rise and fit, every slot names
    real items, NO slot a mode rolls holds gear above that mode's enemy level + the rule,
    every gear item drops in the window it belongs to (NORMAL: all a NORMAL enemy can
    carry — up to the strongest NORMAL enemy's level; HARDCORE/HELL: everything above that)
    FROM AN ENEMY SOME MAP SPAWNS (`placed` — 2026-09-28: the drop-gated 신관 is on no map),
    every DROP_ASSIGN slot holds what it names,
    every reachable gear slot has its grade's width in the mode that rolls it (grade_width — no more than it on an
    enemy whose table ⑤ had to squeeze under 32767), and no slot at all names a
    NO_DROP_TYPES item (the network-only 마력구, the host's shop orbs)."""
    a, b = enm_records(old), enm_records(new)
    gear, exists = item_levels(itm_blobs)
    gear = {key: v for key, v in gear.items() if key not in OUT_OF_WINDOWS}  # HELL 칸 8 · 가디언 칸 전용 (아래에서 따로 잠근다)
    eff = lambda t: SLOT_TYPE_REMAP.get(t, t)  # noqa: E731
    covered = {"N": set(), "H": set()}
    top_normal = 0
    for (ia, _na, sa, ba), (ib, _nb, sb, bb) in zip(a, b):
        assert (ia, sa, len(ba)) == (ib, sb, len(bb)), f"{ENM}: record {ia} moved"
        n = ba[33]
        assert ba[:ENM_SLOT_OFF] == bb[:ENM_SLOT_OFF], f"{ENM}: [{ia}] a non-slot byte changed"
        assert ba[ENM_SLOT_OFF + 5 * n :] == bb[ENM_SLOT_OFF + 5 * n :], f"{ENM}: [{ia}] a byte after the slots changed"
        if struct.unpack_from("<h", bb, 15)[0] == 0:
            assert ba == bb, f"{ENM}: [{ia}] a no-drop enemy was changed"
            continue
        top_normal = max(top_normal, bb[4])
        prev = 0
        widths = []
        for k in range(n):
            o = ENM_SLOT_OFF + 5 * k
            t, lo, hi = bb[o], bb[o + 1], bb[o + 2]
            cum = struct.unpack_from("<h", bb, o + 3)[0]
            assert prev <= cum <= DROP_ROLL_MAX, f"{ENM}: [{ia}] slot {k} cumulative {cum} after {prev}"
            assert lo <= hi and all((eff(t), j) in exists for j in range(lo, hi + 1)), f"{ENM}: [{ia}] slot {k} t{t} {lo}-{hi} names a missing item"
            assert eff(t) not in NO_DROP_TYPES, f"{ENM}: [{ia}] slot {k} t{t} {lo}-{hi} drops a NO_DROP_TYPES item"
            if k < ENM_SLOTS:
                assert not any((eff(t), j) in OUT_OF_WINDOWS for j in range(lo, hi + 1)), f"{ENM}: [{ia}] slot {k} t{t} {lo}-{hi} drops a HELL-only / guardian item"
            widths.append((k, eff(t), lo, hi, cum - prev))
            if cum - prev >= 2:
                ids = [(eff(t), j) for j in range(lo, hi + 1)]
                top = max(_slot_refs(bb[4], k)) + DROP_LEVEL_UP
                for key in ids:
                    if key in gear:
                        assert gear[key][0] <= top, f"{ENM}: [{ia}] slot {k} drops {key} (LV {gear[key][0]}) above LV {top}"
                if ia in placed:  # 맵에 없는 적의 칸은 아무도 굴리지 않는다 — 출처로 세지 않는다
                    if k in NORMAL_SLOTS:
                        covered["N"].update(ids)
                    if k in HARD_WINDOW:
                        covered["H"].update(ids)
            prev = cum
        # 등급별 폭 — reachable gear slots only (< 2 is the shipped game's off switch, left alone).
        rows = [(k, t, lo, hi, w) for (k, t, lo, hi, w) in widths if t in GEAR_TYPES and w >= 2]
        expect = {k: tier_width(gear, t, lo, hi, k) for (k, t, lo, hi, _w) in rows}
        squeezed = sum(r[4] for r in widths) - sum(r[4] for r in rows) + sum(expect.values()) > DROP_ROLL_MAX
        for k, t, lo, hi, w in rows:
            ok = w <= expect[k] if squeezed else w == expect[k]
            assert ok, f"{ENM}: [{ia}] slot {k} t{t} {lo}-{hi} width {w}, its tier width is {expect[k]}"
            # ④ᶜ — a map-placed enemy's slot never mixes 종결 with another tier (the ids are drawn uniformly).
            if ia in placed:
                finals = {gear_tier(gear, (t, j)) == TIER_FINAL for j in range(lo, hi + 1) if (t, j) in gear}
                assert len(finals) == 1, f"{ENM}: [{ia}] slot {k} t{t} {lo}-{hi} mixes 종결 with other tiers"
    lost_n = sorted(key for key, (lv, _g, _s) in gear.items() if lv <= top_normal and key not in covered["N"])
    lost_h = sorted(key for key, (lv, _g, _s) in gear.items() if lv > top_normal and key not in covered["H"])
    assert not lost_n, f"{ENM}: gear a NORMAL enemy could carry never drops in NORMAL (from a map-placed enemy): {lost_n[:8]}"
    assert not lost_h, f"{ENM}: high gear never drops in HARDCORE/HELL (from a map-placed enemy): {lost_h[:8]}"
    for (idx, k), (_was, (t, lo, hi), _why) in DROP_ASSIGN.items():
        body = b[idx][3]
        o = ENM_SLOT_OFF + 5 * k
        assert (eff(body[o]), body[o + 1], body[o + 2]) == (t, lo, hi), f"{ENM}: DROP_ASSIGN [{idx}] slot {k} did not land"


def _hell_runs(gear, t, lo_lv, hi_lv, tiers):
    """Maximal runs of consecutive ids of type `t` whose items all lie in `lo_lv..hi_lv`, share one grade and
    belong to one of `tiers` (★ 2026-10-02 저녁: slot 5 takes the 상급 runs, slot 7 the 종결 ones)."""
    runs, cur = [], []
    for i in sorted(i for (tt, i) in gear if tt == t):
        lv, g, _set = gear[(t, i)]
        if not lo_lv <= lv <= hi_lv or gear_tier(gear, (t, i)) not in tiers:
            if cur:
                runs.append(cur)
            cur = []
            continue
        if cur and i == cur[-1] + 1 and gear[(t, cur[0])][1] == g:
            cur.append(i)
            continue
        if cur:
            runs.append(cur)
        cur = [i]
    if cur:
        runs.append(cur)
    return runs


def _hell_best_run(gear, main_types, prefer, lo_lv, hi_lv, lv, tiers):
    """The run slot 5 (상급) or 7 (종결) takes: the enemy's HARD-slot types first, then 레어 → 에픽 → 영웅, the mean
    level nearest the enemy's, the longest, then type and id for a total order. `None` when no run fits."""
    best = None
    for t in main_types:
        for run in _hell_runs(gear, t, lo_lv, hi_lv, tiers):
            grade = gear[(t, run[0])][1]
            mean = sum(gear[(t, i)][0] for i in run) / len(run)
            score = (prefer.index(t) if t in prefer else len(prefer), HELL_GRADE_ORDER.index(grade), abs(mean - lv), -len(run), t, run[0])
            if best is None or score < best[0]:
                best = (score, t, run[0], run[-1], grade)
    return best


def plan_hell_slots(enemies, gear):
    """`{enemy index: [(type, lo, hi, width) slot 5, (…) 6, (…) 7, (…) 8]}` for every drop-gated five-slot record, and
    a log. Pure: the same records and items always give the same slots (see the HELL 보너스 칸 notes above).
    ★ 2026-10-02 저녁: slot 5 = a 상급 run · 6 = the 종결 accessories 14..16 · 7 = a 종결 run, or (7, 0, 0, 0) when the
    HELL band holds none · 8 = the new six 17..19 behind HELL_GATE (환경설정 난이도 NORMAL+ since 2026-10-03; HARD only before),
    or (7 | HELL_GATE, 0, 0, 0) when the six (LV 70 since 2026-10-03) sit above the HELL level + DROP_LEVEL_UP."""
    eff = lambda t: SLOT_TYPE_REMAP.get(t, t)  # noqa: E731
    main_types = sorted(t for t in GEAR_TYPES if t not in HELL_ACCESSORY_TYPES)
    plan, log = {}, []
    for idx, name, _start, body in enemies:
        gate = struct.unpack_from("<h", body, 15)[0]
        if gate == 0 or body[33] != ENM_SLOTS:
            continue
        lv = _mode_level(body[4], 2)
        lo_lv, hi_lv = lv - HELL_LEVEL_DOWN, lv + DROP_LEVEL_UP
        prefer = []
        for k in HARD_ONLY_SLOTS:
            t = eff(body[ENM_SLOT_OFF + 5 * k])
            if t in main_types and t not in prefer:
                prefer.append(t)
        upper = _hell_best_run(gear, main_types, prefer, lo_lv, hi_lv, lv, (TIER_UPPER,))
        if upper is None:
            raise SystemExit(f"{ENM}: [{idx}] {name}: no 상급 gear between LV {lo_lv} and {hi_lv} for its HELL slot 5")
        _s, t, lo, hi, grade = upper
        final = _hell_best_run(gear, main_types, prefer, lo_lv, hi_lv, lv, (TIER_FINAL,))
        s4 = eff(body[ENM_SLOT_OFF + 5 * 4])
        acc = s4 if s4 in HELL_ACCESSORY_TYPES else HELL_ACCESSORY_TYPES[idx % 2]
        rows = [(t, lo, hi, HELL_GRADE_WIDTH[grade]), (acc, HELL_ACCESSORY_IDS[0], HELL_ACCESSORY_IDS[1], HELL_ACCESSORY_WIDTH)]
        if final is None:
            rows.append((SLOT_TYPE_NONE, 0, 0, 0))
            final_text = "슬롯7 없음(띠에 종결 장비 없음)"
        else:
            _s, ft, flo, fhi, _fg = final
            rows.append((ft, flo, fhi, HELL_FINAL_WIDTH))
            flvs = [gear[(ft, i)][0] for i in range(flo, fhi + 1)]
            final_text = f"슬롯7 t{ft} {flo}-{fhi} LV{min(flvs)}-{max(flvs)} 종결 {HELL_FINAL_WIDTH}"
        six_lv = max(gear[(acc, i)][0] for i in range(HELL_NEW_SIX_IDS[0], HELL_NEW_SIX_IDS[1] + 1))
        if six_lv <= hi_lv:
            rows.append((acc | HELL_GATE, HELL_NEW_SIX_IDS[0], HELL_NEW_SIX_IDS[1], HELL_NEW_SIX_WIDTH))
            six_text = f"슬롯8 t{acc}+GATE {HELL_NEW_SIX_IDS[0]}-{HELL_NEW_SIX_IDS[1]} 신규 {HELL_NEW_SIX_WIDTH}"
        else:
            rows.append((SLOT_TYPE_NONE | HELL_GATE, 0, 0, 0))
            six_text = f"슬롯8 없음(신규 LV{six_lv} > LV{hi_lv})"
        # ★ 2026-10-09: 칸 9 = 가디언 조합서 61..91 — 모든 게이트 적, 레벨과 무관(HELL_RECIPE_WIDTH).
        rows.append((RECIPE_TYPE, RECIPE_SLOT_IDS[0], RECIPE_SLOT_IDS[1], HELL_RECIPE_WIDTH))
        # ★ 2026-10-08 가디언 패스 (★ 2026-10-09 칸 10..14, 자기 축): 10 = 가디언 장비 한 타입(적 인덱스 % 10) · 11 = 가디언 장신구(GATE) ·
        #   12 = 칸 7 의 종결 묶음을 폭 두 배로 · 13 = 종결 장신구 14..16 · 14 = 가디언 조합서. 레벨 띠 = 가디언 레벨 −GUARDIAN_LEVEL_DOWN .. +DROP_LEVEL_UP.
        lvg = _mode_level(body[4], 3)
        g_lo, g_hi = lvg - GUARDIAN_LEVEL_DOWN, lvg + DROP_LEVEL_UP
        gt = GUARDIAN_GEAR_TYPES[idx % len(GUARDIAN_GEAR_TYPES)]
        gids = sorted(i for (tt, i) in GUARDIAN_ITEMS if tt == gt)
        assert gids and gids == list(range(gids[0], gids[-1] + 1)), f"guardian gear of type {gt} is not one run: {gids}"
        if all(g_lo <= gear[(gt, i)][0] <= g_hi for i in gids):
            rows.append((gt, gids[0], gids[-1], GUARDIAN_GEAR_WIDTH))
            g9_text = f"슬롯9 t{gt} {gids[0]}-{gids[-1]} 가디언 {GUARDIAN_GEAR_WIDTH}"
        else:
            rows.append((SLOT_TYPE_NONE, 0, 0, 0))
            g9_text = f"슬롯9 없음(가디언 장비가 LV{g_lo}-{g_hi} 밖)"
        gacc, (galo, gahi) = (11, (20, 20)) if idx % 2 == 0 else (12, (20, 21))
        if all(g_lo <= gear[(gacc, i)][0] <= g_hi for i in range(galo, gahi + 1)):
            rows.append((gacc | HELL_GATE, galo, gahi, GUARDIAN_ACC_WIDTH))
            g10_text = f"슬롯10 t{gacc}+GATE {galo}-{gahi} 가디언 장신구 {GUARDIAN_ACC_WIDTH}"
        else:
            rows.append((SLOT_TYPE_NONE | HELL_GATE, 0, 0, 0))
            g10_text = f"슬롯10 없음(가디언 장신구가 LV{g_lo}-{g_hi} 밖)"
        if final is None:
            rows.append((SLOT_TYPE_NONE, 0, 0, 0))
        else:
            rows.append((ft, flo, fhi, GUARDIAN_FINAL_WIDTH))
        rows.append((acc, HELL_ACCESSORY_IDS[0], HELL_ACCESSORY_IDS[1], GUARDIAN_FINAL_ACC_WIDTH))
        rows.append((RECIPE_TYPE, RECIPE_SLOT_IDS[0], RECIPE_SLOT_IDS[1], GUARDIAN_RECIPE_WIDTH))
        # ★ 2026-10-12: 칸 15 = 인장 상급 — 종류 = 적 인덱스 % 5, 난이도 NORMAL+ 게이트, 레벨과 무관(장비가 아니다).
        rows.append((guardian_seal_type(idx) | HELL_GATE, SEAL_TOP_ID, SEAL_TOP_ID, GUARDIAN_SEAL_WIDTH))
        s16_text = ""
        if SKILL_GEAR:
            # ★★ 2026-10-14 칸 16 = 2차 장비(적 인덱스 % 11 의 타입 전부, 가디언 칸 10 과 같은 레벨 띠) · 17 = 2차 조합서(레벨과 무관).
            st = SKILL_GEAR_TYPES[idx % len(SKILL_GEAR_TYPES)]
            sids = sorted(i for (tt, i) in SKILL_GEAR_ITEMS if tt == st)
            assert sids and sids == list(range(sids[0], sids[-1] + 1)), f"2nd-gen gear of type {st} is not one run: {sids}"
            if all(g_lo <= gear[(st, i)][0] <= g_hi for i in sids):
                rows.append((st, sids[0], sids[-1], SKILL_GEAR_WIDTH))
                s16_text = f" · 슬롯16 t{st} {sids[0]}-{sids[-1]} 2차 {SKILL_GEAR_WIDTH}"
            else:
                rows.append((SLOT_TYPE_NONE, 0, 0, 0))
                s16_text = f" · 슬롯16 없음(2차가 LV{g_lo}-{g_hi} 밖)"
            rows.append((RECIPE_TYPE, SKILL_RECIPE_IDS[0], SKILL_RECIPE_IDS[1], SKILL_RECIPE_WIDTH))
            s16_text += f" · 슬롯17 2차 조합서 {SKILL_RECIPE_IDS[0]}-{SKILL_RECIPE_IDS[1]} {SKILL_RECIPE_WIDTH}"
        assert len(rows) == len(HELL_SLOTS) + len(GUARDIAN_SLOTS), f"[{idx}] {name}: {len(rows)} bonus rows"
        plan[idx] = rows
        lvs = [gear[(t, i)][0] for i in range(lo, hi + 1)]
        log.append(
            f"HELL   [{idx:3}] {name} HELL LV{lv}: 슬롯5 t{t} {lo}-{hi} LV{min(lvs)}-{max(lvs)} {GRADE_NAME[grade]} {HELL_GRADE_WIDTH[grade]} · "
            f"슬롯6 t{acc} {HELL_ACCESSORY_IDS[0]}-{HELL_ACCESSORY_IDS[1]} 종결 장신구 {HELL_ACCESSORY_WIDTH} · {final_text} · {six_text} · "
            f"슬롯9 조합서 {RECIPE_SLOT_IDS[0]}-{RECIPE_SLOT_IDS[1]} {HELL_RECIPE_WIDTH} · "
            f"가디언 LV{lvg}: {g9_text} · {g10_text} · 슬롯12 종결 {GUARDIAN_FINAL_WIDTH if final is not None else '없음'} · 슬롯13 종결 장신구 {GUARDIAN_FINAL_ACC_WIDTH} · "
            f"슬롯14 조합서 {GUARDIAN_RECIPE_WIDTH} · 슬롯15 t{guardian_seal_type(idx)}+GATE 인장 상급 {GUARDIAN_SEAL_WIDTH}{s16_text}"
        )
    return plan, log


def add_hell_slots(blob, itm_blobs):
    """`enm/enm` with the HELL slots (HELL_SLOTS — four since 2026-10-02 저녁) appended to every drop-gated five-slot
    record: the record grows by 5 B a slot — its u16 length, the slot count body[33] 5 → 9, the slots between slot 4
    and the trailing `cq.g` · `cq.l`. Every other record is copied byte for byte."""
    enemies = enm_records(blob)
    gear, _exists = item_levels(itm_blobs)
    plan, log = plan_hell_slots(enemies, gear)
    out, i = bytearray(), 0
    cut = ENM_SLOT_OFF + 5 * ENM_SLOTS
    for idx, _name, start, body in enemies:
        (n,) = struct.unpack_from(">H", blob, i)
        assert i + 2 + n == start + len(body), f"{ENM}: [{idx}] does not end at its body"
        head = blob[i + 2 : start]
        if idx in plan:
            cum, extra = 0, b""
            for n_row, (t, lo, hi, w) in enumerate(plan[idx]):
                if n_row == len(HELL_SLOTS):
                    cum = 0  # ★ 2026-10-08: the guardian pass rolls on its own axis (HellDropPatch::LAST_HELL_SLOT)
                cum += w
                extra += bytes([t, lo, hi]) + struct.pack("<h", cum)
            body = body[:33] + bytes([ENM_SLOTS + len(HELL_SLOTS) + len(GUARDIAN_SLOTS)]) + body[34:cut] + extra + body[cut:]
        rec = head + body
        out += struct.pack(">H", len(rec)) + rec
        i += 2 + n
    return bytes(out), plan, log


def verify_hell_slots(before, after, itm_blobs, placed):
    """Every record parses and keeps its index and name; a drop-gated five-slot record became nine slots with its first
    five and its tail byte for byte; every other record is unchanged; on their own axis from 0: slot 5 is one 상급 run
    (one grade, inside the HELL band) at its grade's HELL_GRADE_WIDTH · slot 6 the 종결 accessories 14..16 at
    HELL_ACCESSORY_WIDTH · slot 7 one 종결 run at HELL_FINAL_WIDTH, or type 7 at width 0 · slot 8 the new six 17..19 at
    HELL_NEW_SIX_WIDTH with the HELL_GATE bit; no NO_DROP_TYPES; and every 종결 gear item drops from some map-placed
    enemy's slot 7. Returns how many records got the slots."""
    a, b = enm_records(before), enm_records(after)
    gear, exists = item_levels(itm_blobs)
    assert len(a) == len(b) == ENM_RECORDS, f"{ENM}: record count"
    cut = ENM_SLOT_OFF + 5 * ENM_SLOTS
    added = 0
    final_seen = set()
    skill_seen = set()
    for (ia, na, _sa, ba), (ib, nb, _sb, bb) in zip(a, b):
        assert (ia, na) == (ib, nb), f"{ENM}: record {ia} moved"
        if struct.unpack_from("<h", ba, 15)[0] == 0 or ba[33] != ENM_SLOTS:
            assert ba == bb, f"{ENM}: [{ia}] has no HELL slots but changed"
            continue
        assert bb[33] == ENM_SLOTS + len(HELL_SLOTS) + len(GUARDIAN_SLOTS), f"{ENM}: [{ia}] slot count {bb[33]}"
        assert ba[:33] == bb[:33] and ba[34:cut] == bb[34:cut], f"{ENM}: [{ia}] a shipped-window byte changed"
        assert ba[cut:] == bb[cut + 5 * (len(HELL_SLOTS) + len(GUARDIAN_SLOTS)) :], f"{ENM}: [{ia}] the bytes after the slots changed"
        lv = _mode_level(ba[4], 2)
        prev = 0
        acc = None
        for k in HELL_SLOTS:
            o = ENM_SLOT_OFF + 5 * k
            t, lo, hi = bb[o], bb[o + 1], bb[o + 2]
            cum = struct.unpack_from("<h", bb, o + 3)[0]
            w = cum - prev
            prev = cum
            gated, t = bool(t & HELL_GATE), t & ~HELL_GATE
            assert gated == (k == HELL_SLOTS[3]), f"{ENM}: [{ia}] HELL slot {k} gate bit {gated}"
            if k == HELL_SLOTS[2] and t == SLOT_TYPE_NONE:
                assert (lo, hi, w) == (0, 0, 0), f"{ENM}: [{ia}] HELL slot 7 'nothing' must be 0-0 at width 0"
                assert not any(gear_tier(gear, key) == TIER_FINAL and lv - HELL_LEVEL_DOWN <= gear[key][0] <= lv + DROP_LEVEL_UP for key in gear if key[0] not in HELL_ACCESSORY_TYPES), f"{ENM}: [{ia}] HELL slot 7 is empty though a 종결 piece fits LV {lv}"
                continue
            if k == HELL_SLOTS[3] and t == SLOT_TYPE_NONE:
                # ★ 2026-10-03: the new six (LV 70) above this enemy's HELL ceiling — empty, and only then
                assert (lo, hi, w) == (0, 0, 0), f"{ENM}: [{ia}] HELL slot 8 'nothing' must be 0-0 at width 0"
                assert all(gear[key][0] > lv + DROP_LEVEL_UP for key in HELL_ONLY_ITEMS), f"{ENM}: [{ia}] HELL slot 8 is empty though the new six fit LV {lv}"
                continue
            assert w >= 2 and 0 < cum <= DROP_ROLL_MAX, f"{ENM}: [{ia}] HELL slot {k} cumulative {cum} after {prev - w}"
            assert t not in NO_DROP_TYPES, f"{ENM}: [{ia}] HELL slot {k} drops a NO_DROP_TYPES item"
            if k == HELL_SLOTS[4]:
                # ★ 2026-10-09: the guardian recipes, every gated enemy, no level rule (they are not gear)
                assert (t, (lo, hi), w) == (RECIPE_TYPE, RECIPE_SLOT_IDS, HELL_RECIPE_WIDTH), f"{ENM}: [{ia}] HELL slot 9 t{t} {lo}-{hi} width {w} is not the guardian recipes"
                assert all((t, j) in exists for j in range(lo, hi + 1)), f"{ENM}: [{ia}] HELL slot 9 names a recipe the DB lacks"
                continue
            ids = [(t, j) for j in range(lo, hi + 1)]
            assert lo <= hi and all(key in exists and key in gear for key in ids), f"{ENM}: [{ia}] HELL slot {k} t{t} {lo}-{hi} is not gear"
            assert len({gear[key][1] for key in ids}) == 1, f"{ENM}: [{ia}] HELL slot {k} mixes grades"
            tiers = {gear_tier(gear, key) for key in ids}
            if k == HELL_SLOTS[0]:
                assert t not in HELL_ACCESSORY_TYPES and tiers == {TIER_UPPER} and w == HELL_GRADE_WIDTH[gear[ids[0]][1]], f"{ENM}: [{ia}] HELL slot 5 t{t} {lo}-{hi} width {w} tiers {tiers}"
            elif k == HELL_SLOTS[2]:
                assert t not in HELL_ACCESSORY_TYPES and tiers == {TIER_FINAL} and w == HELL_FINAL_WIDTH, f"{ENM}: [{ia}] HELL slot 7 t{t} {lo}-{hi} width {w} tiers {tiers}"
                if ia in placed:
                    final_seen.update(ids)
            else:
                want_ids, want_w = (HELL_ACCESSORY_IDS, HELL_ACCESSORY_WIDTH) if k == HELL_SLOTS[1] else (HELL_NEW_SIX_IDS, HELL_NEW_SIX_WIDTH)
                assert t in HELL_ACCESSORY_TYPES and (lo, hi) == want_ids and w == want_w, f"{ENM}: [{ia}] HELL slot {k} t{t} {lo}-{hi} width {w}"
                assert tiers == {TIER_FINAL}, f"{ENM}: [{ia}] HELL slot {k} is not 종결"
                assert acc in (None, t), f"{ENM}: [{ia}] HELL slots 6 and 8 name different accessory types"
                acc = t
                if k == HELL_SLOTS[3]:
                    assert all(key in HELL_ONLY_ITEMS for key in ids), f"{ENM}: [{ia}] HELL slot 8 is not the new six"
            # ★ 2026-10-03: slot 8 too — the new six are LV 70 now (they were LV 1 and exempt)
            assert all(gear[key][0] <= lv + DROP_LEVEL_UP for key in ids), f"{ENM}: [{ia}] HELL slot {k} above LV {lv + DROP_LEVEL_UP}"
            if k in (HELL_SLOTS[0], HELL_SLOTS[2]):
                assert all(gear[key][0] >= lv - HELL_LEVEL_DOWN for key in ids), f"{ENM}: [{ia}] HELL slot {k} below LV {lv - HELL_LEVEL_DOWN}"
        # ★ 2026-10-08 the guardian pass: its own axis from 0; the level band is the GUARDIAN level's.
        lvg = _mode_level(ba[4], 3)
        prev = 0
        s7 = bb[ENM_SLOT_OFF + 5 * HELL_SLOTS[2] : ENM_SLOT_OFF + 5 * HELL_SLOTS[2] + 3]
        for k in GUARDIAN_SLOTS:
            o = ENM_SLOT_OFF + 5 * k
            t, lo, hi = bb[o], bb[o + 1], bb[o + 2]
            cum = struct.unpack_from("<h", bb, o + 3)[0]
            w = cum - prev
            prev = cum
            gated, t = bool(t & HELL_GATE), t & ~HELL_GATE
            assert gated == (k in (GUARDIAN_SLOTS[1], GUARDIAN_SLOTS[5])), f"{ENM}: [{ia}] guardian slot {k} gate bit {gated}"
            if t == SLOT_TYPE_NONE:
                assert (lo, hi, w) == (0, 0, 0), f"{ENM}: [{ia}] guardian slot {k} 'nothing' must be 0-0 at width 0"
                if k == GUARDIAN_SLOTS[0]:
                    gt = GUARDIAN_GEAR_TYPES[ia % len(GUARDIAN_GEAR_TYPES)]
                    assert not all(lvg - GUARDIAN_LEVEL_DOWN <= gear[key][0] <= lvg + DROP_LEVEL_UP for key in GUARDIAN_ITEMS if key[0] == gt), f"{ENM}: [{ia}] guardian slot 9 is empty though type {gt} fits LV {lvg}"
                elif k == GUARDIAN_SLOTS[2]:
                    assert s7[0] == SLOT_TYPE_NONE, f"{ENM}: [{ia}] guardian slot 11 is empty though slot 7 holds a 종결 run"
                elif SKILL_SLOTS and k == SKILL_SLOTS[0]:
                    st = SKILL_GEAR_TYPES[ia % len(SKILL_GEAR_TYPES)]
                    assert not all(lvg - GUARDIAN_LEVEL_DOWN <= gear[key][0] <= lvg + DROP_LEVEL_UP for key in SKILL_GEAR_ITEMS if key[0] == st), f"{ENM}: [{ia}] guardian slot 16 is empty though type {st} fits LV {lvg}"
                else:
                    assert not (SKILL_SLOTS and k == SKILL_SLOTS[1]), f"{ENM}: [{ia}] guardian slot 17 is empty"
                continue
            assert w >= 2 and 0 < cum <= DROP_ROLL_MAX, f"{ENM}: [{ia}] guardian slot {k} cumulative {cum} after {prev - w}"
            if SKILL_SLOTS and k == SKILL_SLOTS[1]:
                # ★★ 2026-10-14 the 2nd-gen recipes, every gated enemy, no level rule
                assert (t, (lo, hi), w) == (RECIPE_TYPE, SKILL_RECIPE_IDS, SKILL_RECIPE_WIDTH), f"{ENM}: [{ia}] guardian slot 17 t{t} {lo}-{hi} width {w} is not the 2nd-gen recipes"
                assert all((t, j) in exists for j in range(lo, hi + 1)), f"{ENM}: [{ia}] guardian slot 17 names a recipe the DB lacks"
                continue
            if SKILL_SLOTS and k == SKILL_SLOTS[0]:
                ids = [(t, j) for j in range(lo, hi + 1)]
                assert lo <= hi and all(key in exists and key in gear for key in ids), f"{ENM}: [{ia}] guardian slot 16 t{t} {lo}-{hi} is not gear"
                assert all(key in SKILL_GEAR_ITEMS for key in ids) and t == SKILL_GEAR_TYPES[ia % len(SKILL_GEAR_TYPES)] and w == SKILL_GEAR_WIDTH, f"{ENM}: [{ia}] guardian slot 16 t{t} {lo}-{hi} width {w}"
                assert sorted(ids) == sorted(key for key in SKILL_GEAR_ITEMS if key[0] == t), f"{ENM}: [{ia}] guardian slot 16 is not all of type {t}'s 2nd gen"
                assert {gear_tier(gear, key) for key in ids} == {TIER_GUARDIAN}, f"{ENM}: [{ia}] guardian slot 16 tier"
                assert all(lvg - GUARDIAN_LEVEL_DOWN <= gear[key][0] <= lvg + DROP_LEVEL_UP for key in ids), f"{ENM}: [{ia}] guardian slot 16 outside LV {lvg - GUARDIAN_LEVEL_DOWN}-{lvg + DROP_LEVEL_UP}"
                if ia in placed:
                    skill_seen.update(ids)
                continue
            if k == GUARDIAN_SLOTS[4]:
                assert (t, (lo, hi), w) == (RECIPE_TYPE, RECIPE_SLOT_IDS, GUARDIAN_RECIPE_WIDTH), f"{ENM}: [{ia}] guardian slot 14 t{t} {lo}-{hi} width {w} is not the guardian recipes"
                continue
            if k == GUARDIAN_SLOTS[5]:
                # ★ 2026-10-12: 인장 상급 (id SEAL_TOP_ID) of the enemy's type, 1 %
                assert (t, lo, hi, w) == (guardian_seal_type(ia), SEAL_TOP_ID, SEAL_TOP_ID, GUARDIAN_SEAL_WIDTH), f"{ENM}: [{ia}] guardian slot 15 t{t} {lo}-{hi} width {w} is not 인장 상급"
                assert (t, lo) in exists, f"{ENM}: [{ia}] guardian slot 15 names a seal the DB lacks"
                continue
            ids = [(t, j) for j in range(lo, hi + 1)]
            assert lo <= hi and all(key in exists and key in gear for key in ids), f"{ENM}: [{ia}] guardian slot {k} t{t} {lo}-{hi} is not gear"
            tiers = {gear_tier(gear, key) for key in ids}
            if k == GUARDIAN_SLOTS[0]:
                assert all(key in GUARDIAN_ITEMS for key in ids) and t == GUARDIAN_GEAR_TYPES[ia % len(GUARDIAN_GEAR_TYPES)] and w == GUARDIAN_GEAR_WIDTH and tiers == {TIER_GUARDIAN}, f"{ENM}: [{ia}] guardian slot 9 t{t} {lo}-{hi} width {w}"
                assert all(lvg - GUARDIAN_LEVEL_DOWN <= gear[key][0] <= lvg + DROP_LEVEL_UP for key in ids), f"{ENM}: [{ia}] guardian slot 9 outside LV {lvg - GUARDIAN_LEVEL_DOWN}-{lvg + DROP_LEVEL_UP}"
            elif k == GUARDIAN_SLOTS[1]:
                assert all(key in GUARDIAN_ITEMS for key in ids) and t in HELL_ACCESSORY_TYPES and w == GUARDIAN_ACC_WIDTH and tiers == {TIER_GUARDIAN}, f"{ENM}: [{ia}] guardian slot 10 t{t} {lo}-{hi} width {w}"
                assert all(gear[key][0] <= lvg + DROP_LEVEL_UP for key in ids), f"{ENM}: [{ia}] guardian slot 10 above LV {lvg + DROP_LEVEL_UP}"
            elif k == GUARDIAN_SLOTS[2]:
                assert (t, lo, hi) == (s7[0], s7[1], s7[2]) and w == GUARDIAN_FINAL_WIDTH and tiers == {TIER_FINAL}, f"{ENM}: [{ia}] guardian slot 11 t{t} {lo}-{hi} width {w} is not slot 7's run at {GUARDIAN_FINAL_WIDTH}"
            else:
                assert t in HELL_ACCESSORY_TYPES and (lo, hi) == HELL_ACCESSORY_IDS and w == GUARDIAN_FINAL_ACC_WIDTH and tiers == {TIER_FINAL}, f"{ENM}: [{ia}] guardian slot 12 t{t} {lo}-{hi} width {w}"
        added += 1
    final_all = {key for key in gear if gear_tier(gear, key) == TIER_FINAL and key[0] not in HELL_ACCESSORY_TYPES}
    lost = sorted(final_all - final_seen)
    assert not lost, f"{ENM}: 종결 gear no map-placed enemy's HELL slot 7 drops: {lost}"
    lost2 = sorted(SKILL_GEAR_ITEMS - skill_seen)
    assert not lost2, f"{ENM}: 2nd-gen gear no map-placed enemy's guardian slot 16 drops: {lost2}"
    return added


def _shop_rows(blob):
    rows, i = [], 0
    while i < len(blob):
        n = blob[i]
        rec = blob[i + 1 : i + 1 + n]
        if len(rec) != n:
            raise SystemExit(f"{ITM_SHOP}: record at {i} overruns EOF (len={n})")
        rows.append((struct.unpack_from(">H", rec, 0)[0], rec[2:]))
        i += 1 + n
    return rows


# ── 보스 문 이벤트 (2026-09-26) ─────────────────────────────────────────────────
# A map's EVENT section (read by `bf.a([BI)I` @0x12af98) opens with the trigger groups:
#   [u8 nA] { [u8 m] [m × 31-byte record] } × nA,  then the strings [u8 n] {[u8 len][EUC-KR]},
#   then the scripts [u8 n] {[u8 m] [m × 3-byte command]}.
# A door tile names a group; `bb.a(BB)Z` @0x1223f8 runs the FIRST record of it whose type
# (rec[2]) matches and whose conditions pass (`bb.a([B)Z` @0x1224e0):
#   rec[0..1] class / min level · rec[3..8] three (quest, state) pairs — `ag.r` 2-bit states,
#   script op 0x11 · rec[9..14] three (event flag, 1) pairs — `ag.q` bits, script op 0x12, the
#   ones a boss's kill script sets · rec[30] the script index (255 = none).
# 29, 41 and 56 close their doors at entry (the entry script sets the flag the first record tests).
# 86 (해저터널 3구역 → 102 「소문의 검사」) closes only when quest 0x20 is turned in: its prompt record
# wants 0x20 == 1 and 클레멘스 (68) moves it to 2 — so between the kill (`12 42 00`) and the report
# the door reopens and the boss stands again at full HP (102's own kill-flag record walks a second
# kill out without the dialogue: the window was known). 91 (도적굴 3구역 → 101 도적두목, `12 86 00`)
# never closes: its kill-flag record routes to the prompt, also after 볼프강's report. So this is a
# behaviour change, not a restoration (probe, original maps + `--h2-shipped 1`): both doors now
# close at the kill — 86 on a line added for it (「굳이 다시 들어갈 필요는 없지.」), 91 on the map's
# own "no reason" line, the text that door shows outside its quest.
# Recheck: `python tools_re/decode_map_events.py <zip> --quest 0x20,0x08 --event 0x42,0x86`.
def _event_record(quest=(), event=(), script=0):
    """A 31-byte trigger record: type 3 (step on), no class/level gate, up to three
    (quest, state) and three (event flag, 1) conditions — `(0, 1)` is the game's "none"."""
    r = bytearray(31)
    r[2] = 3
    for i in range(3):
        r[3 + 2 * i : 5 + 2 * i] = bytes((0, 1))
        r[9 + 2 * i : 11 + 2 * i] = bytes((0, 1))
    for i, (f, v) in enumerate(quest):
        r[3 + 2 * i : 5 + 2 * i] = bytes((f, v))
    for i, (f, v) in enumerate(event):
        r[9 + 2 * i : 11 + 2 * i] = bytes((f, v))
    r[30] = script
    return bytes(r)


MAP_EVENT_FIXES = {
    # map: offset of section A, the shipped records it must find (group, record, bytes), the edit
    "P/m/m/86.m.gz": {
        "at": 1873,
        "expect": [
            (0, 0, _event_record(quest=((0x20, 1),), script=0)),  # quest 0x20 running -> the boss prompt
            (0, 1, _event_record(script=2)),  # otherwise -> 「지금은 별로 들어가고 싶지 않아.」
        ],
        # the door's shipped lines (section B) and script count (section C), checked before the edit
        "expect_lines": [
            "}보스룸으로 이동하면 저장할 수 없습니다.@",
            "}이동합니까?@",
            "|[클레르]@ 지금은 별로 들어가고 싶지 않아.",
        ],
        "expect_scripts": 3,
        # The killed boss gets a line of its own (user 2026-09-26: 「굳이 다시 들어갈 필요는 없지.」),
        # appended as line 3 + script 3 = [say 3, end]. The shipped 「지금은 별로…」 (script 2) stays for
        # the door before the quest is taken, where 「다시」 would not fit — nobody has been inside yet.
        "add_line": "|[클레르]@ 굳이 다시 들어갈 필요는 없지.",
        # ahead of record 0: event flag 0x42 (the 102 kill) -> script 3, that line
        "insert": (0, 0, _event_record(event=((0x42, 1),), script=3)),
    },
    "P/m/m/91.m.gz": {
        "at": 3334,
        "expect": [
            (0, 0, _event_record(event=((0x86, 1),), script=3)),  # the 101 kill -> the prompt (!)
            (0, 1, _event_record(quest=((0x08, 1),), script=3)),  # quest 0x08 running -> the prompt
            (0, 2, _event_record(script=2)),  # otherwise -> 「지금은 별로 들어갈 이유가 없어.」
        ],
        # record 0 (the 101 kill) -> script 2 instead of the prompt 3
        "script": (0, 0, 3, 2),
    },
    # ★ 2026-10-03 발도 (user: *"방어의지 스킬을 얻었다는 대화창에서 방어의지와 발도를 얻었다고 말해줘"*): the quest
    # 「터줏대감」 (재야의 고수) teaches the knight 방어의지 by script 9 — `1a 08` (a.h(B)V: a.p[8] -1 -> 0) and the line
    # 「액티브 스킬 - 방어의지를 배웠습니다!」. 발도 lives in the 부활오라 slot a.p[9] (HERO2_GUEST_RE.md §8.45), so the same
    # script opens it too (`1a 09`, right after the 방어의지 unlock) and the line names both. The host's unlock poll
    # (`emulator::IaidoUnlock`) still covers knights who did the quest before this build. Lines can be long — the
    # dialog wraps (the longest shipped line is 140 bytes).
    "P/m/m/77.m.gz": {
        "at": 1080,
        "anchor_text": "재야의 고수",
        "expect": [],
        # ★ 2026-10-05: 방어의지 → 철괴 (SK1_REPLACE [10]) — the line names the new skill. ★ Same day 일풍격: the
        # rogue's script 10 teaches 번개난타 (`1a 0a`, line 21) — 일풍격 (a.p[15], `guest_anchors::IlpungPatch`) comes
        # with it, so that line names both. ★ 2026-10-06: 번개난타's slot is 「살상연희」 now (SK2_REPLACE [10]). The unlock itself is the HOST's (`emulator::IlpungUnlock`: 번개난타 open →
        # a.p[15] -1 -> 0), not a `1a 0f` here: a build whose patch did not install has a 15-slot a.p, and the script's
        # write would throw there.
        "replace_line": [
            (22, "}액티브 스킬 - {방어의지}를 배웠습니다!", "}액티브 스킬 - {철괴}와 {발도}를 배웠습니다!"),
            (21, "}액티브 스킬 - {번개난타}를 배웠습니다!", "}액티브 스킬 - {살상연희}와 {일풍격}을 배웠습니다!"),
            # ★ 2026-10-07 도발: the berserker's script 8 teaches 사자후 (`1a 0a`, line 20) — 도발 (a.p[15]) opens with it,
            # host-side (`emulator::IlpungUnlock` → `hero2java::H2_SLOT16_UNLOCKS`), for the same reason as 일풍격.
            (20, "}액티브 스킬 - {사자후}를 배웠습니다!", "}액티브 스킬 - {사자후}와 {도발}을 배웠습니다!"),
        ],
        # script 9 as shipped: the two greetings, 1a 08 (방어의지), wait, say 22, the item/gold/quest/exp rewards, end
        "expect_script": (
            9,
            [(0x05, 4, 6), (0x0F, 18, 0), (0x05, 4, 0), (0x0F, 19, 0), (0x1A, 8, 0), (0x25, 0, 0), (0x0F, 22, 0), (0x31, 22, 2), (0x16, 1, 0), (0x0F, 25, 0), (0x18, 58, 152), (0x0F, 26, 0), (0x25, 0, 0), (0x11, 36, 1), (0x12, 86, 0), (0x0F, 23, 0), (0x19, 97, 168), (0x0F, 24, 0), (0x2D, 0, 0)],
        ),
        "insert_cmd": (9, 5, (0x1A, 9, 0)),  # after `1a 08 00`: a.p[9] (발도) -1 -> 0
    },
    # ★ 2026-10-06 「심안」 (user: *"독연마 스킬을 … 1레벨 자동 획득 패시브 스킬로 변경"*): the quest 「무사의 장갑」 (보라두건의 무사,
    # 카디프숲 북부) taught the rogue 독 연마 — script 6: `1a 0c` (a.p[12] -1 -> 0) and line 15 「패시브 스킬 - {독 연마}를
    # 배웠습니다!」. That slot is 「심안」 now and every rogue holds it at 1 from the start (`hero2java::h2_born_skill_repair`),
    # 독 연마's effect is 맹독감염's — so the rogue's reward is the quest itself (completion · EXP), as the knight's and the
    # berserker's scripts 4·5 keep their own lessons. The three commands go (the unlock, the line, its wait); line 15 stays
    # in section B, unread.
    "P/m/m/98.m.gz": {
        "at": 3443,
        "anchor_text": "보라두건의 무사",
        "expect": [],
        "expect_script": (
            6,
            [(0x05, 0, 4), (0x0F, 12, 0), (0x05, 0, 0), (0x31, 8, 5), (0x16, 0xFF, 0), (0x0F, 13, 0), (0x25, 0, 0), (0x1A, 12, 0), (0x0F, 15, 0), (0x25, 0, 0), (0x11, 0x32, 1), (0x12, 0x83, 0), (0x0F, 17, 0), (0x19, 3, 0xE8), (0x0F, 19, 0), (0x2D, 0, 0)],
        ),
        "delete_cmds": (6, 7, [(0x1A, 12, 0), (0x0F, 15, 0), (0x25, 0, 0)]),
    },
}


def _map_groups(d, at):
    """Section A at `at`: [(group start, m, [record offsets])], and where it ends."""
    p = at
    n = d[p]
    p += 1
    groups = []
    for _ in range(n):
        m = d[p]
        groups.append((p, m, [p + 1 + 31 * k for k in range(m)]))
        p += 1 + 31 * m
    return groups, p


def _map_lines(d, p):
    """Sections B and C at `p` (where A ends): (lines, offset of the script count, scripts, end of C)."""
    lines, q = [], p + 1
    for _ in range(d[p]):
        n = d[q]
        lines.append(bytes(d[q + 1 : q + 1 + n]).decode("cp949"))
        q += 1 + n
    c_at, scripts = q, []
    q += 1
    for _ in range(d[c_at]):
        scripts.append(q)
        q += 1 + 3 * d[q]
    return lines, c_at, scripts, q


def patch_map(name, gz):
    """One MAP_EVENT_FIXES entry, verified byte for byte before anything is written."""
    import gzip

    fix = MAP_EVENT_FIXES[name]
    d = bytearray(gzip.decompress(gz))
    groups, end = _map_groups(d, fix["at"])
    for g, r, want in fix["expect"]:
        assert len(want) == 31, f"{name}: bad expect g{g} r{r}"
        got = bytes(d[groups[g][2][r] : groups[g][2][r] + 31])
        assert got == want, f"{name}: g{g} r{r} is not the shipped record: {got.hex()} != {want.hex()}"
    # The strings follow section A: the map's own text must be where the scripts expect it.
    anchor = fix.get("anchor_text", "보스룸")
    assert anchor.encode("euc-kr") in bytes(d[end : end + 400]), f"{name}: no 「{anchor}」 text after section A"
    if "replace_line" in fix or "insert_cmd" in fix or "delete_cmds" in fix:
        # ★ 2026-10-03: a line's TEXT and a script's COMMANDS — sections B and C are rebuilt whole from
        # their parsed form (A before them keeps its offsets; what follows C is read sequentially by the
        # loader, as `add_line` already relies on). cp949 both ways so every untouched line round-trips.
        lines, c_at, scripts, c_end = _map_lines(d, end)
        cmds = [[tuple(d[s + 1 + 3 * k : s + 4 + 3 * k]) for k in range(d[s])] for s in scripts]
        if "expect_script" in fix:
            si, want = fix["expect_script"]
            assert cmds[si] == [tuple(c) for c in want], f"{name}: script {si} is not the shipped one: {cmds[si]}"
        if "replace_line" in fix:
            edits = fix["replace_line"]
            for i, old, new in edits if isinstance(edits, list) else [edits]:
                assert lines[i] == old, f"{name}: line {i} is not {old!r}: {lines[i]!r}"
                assert 0 < len(new.encode("cp949")) < 256, f"{name}: new line {i} is {len(new.encode('cp949'))} bytes"
                lines[i] = new
        if "delete_cmds" in fix:
            # ★ 2026-10-06: a run of commands out of one script, each checked against what it must be.
            si, k, want = fix["delete_cmds"]
            want = [tuple(c) for c in want]
            assert cmds[si][k : k + len(want)] == want, f"{name}: script {si} at {k} is {cmds[si][k : k + len(want)]}, not {want}"
            del cmds[si][k : k + len(want)]
        if "insert_cmd" in fix:
            si, k, cmd = fix["insert_cmd"]
            assert len(cmd) == 3 and len(cmds[si]) < 255 and k <= len(cmds[si]), f"{name}: bad insert_cmd"
            cmds[si].insert(k, tuple(cmd))
        b = bytes((len(lines),)) + b"".join(bytes((len(e),)) + e for e in (l.encode("cp949") for l in lines))
        c = bytes((len(cmds),)) + b"".join(bytes((len(s),)) + b"".join(bytes(cm) for cm in s) for s in cmds)
        d[end:c_end] = b + c
    if "add_line" in fix:
        # B and C lie after A, so they are edited first and A's offsets stay valid; within them C
        # (the later one) goes first for the same reason.
        lines, c_at, scripts, c_end = _map_lines(d, end)
        assert lines == fix["expect_lines"], f"{name}: shipped lines differ: {lines}"
        assert len(scripts) == fix["expect_scripts"], f"{name}: {len(scripts)} scripts, expected {fix['expect_scripts']}"
        enc = fix["add_line"].encode("euc-kr")
        assert 0 < len(enc) < 256, f"{name}: new line is {len(enc)} bytes"
        new_line, new_script = len(lines), len(scripts)
        assert fix["insert"][2][30] == new_script, f"{name}: the inserted record must run script {new_script}"
        d[c_end:c_end] = bytes((2, 0x0F, new_line, 0x00, 0x2D, 0x00, 0x00))  # say new_line · end
        d[c_at] = new_script + 1
        d[c_at:c_at] = bytes((len(enc),)) + enc
        d[end] = new_line + 1
    if "insert" in fix:
        g, r, rec = fix["insert"]
        assert len(rec) == 31, f"{name}: bad insert record"
        start, m, recs = groups[g]
        d[start] = m + 1
        d[recs[r] : recs[r]] = rec
    if "script" in fix:
        g, r, old, new = fix["script"]
        off = groups[g][2][r] + 30
        assert d[off] == old, f"{name}: g{g} r{r} script is {d[off]}, expected {old}"
        d[off] = new
    return gzip.compress(bytes(d), mtime=0)


def _grid(v):
    """The shipped palettes' 8-bit channels sit on 00 1f 3f 5f 7f 9f bf df ff — snap to it."""
    return max(0, min(255, int(round((v + 1) / 32.0)) * 32 - 1)) if v > 0 else 0


def _recolour(rgb, colour):
    """★ 2026-10-09 가디언 방어구의 색 — 아이템마다 따로(세트처럼 보이지 않게, 사용자): `colour` = (본색 색상°, 본색 채도, 명도 배율,
    강조 색상°, 강조 채도) — 밝고 옅은 곳(하이라이트)은 강조색, 나머지는 본색으로. 외곽선(아주 어두운 색)은 그대로.
    (2026-10-08 판은 중갑 = 보라+금 · 경갑 = 강청+은 두 가지뿐이었다.)"""
    import colorsys

    main_h, main_s, vscale, acc_h, acc_s = colour
    r, g, b = (rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF
    h, s, v = colorsys.rgb_to_hsv(r / 255.0, g / 255.0, b / 255.0)
    if v < 0.26:
        return rgb  # outline / shadow
    if v > 0.85 and s < 0.5:
        h2, s2, v2 = acc_h / 360.0, acc_s, v  # the highlights take the accent
    else:
        h2, s2, v2 = main_h / 360.0, main_s if main_s == 0 else min(max(s, main_s * 0.6), max(main_s, 0.05)), min(v * vscale, 1.0)
    r2, g2, b2 = colorsys.hsv_to_rgb(h2, s2, v2)
    return (_grid(int(r2 * 255)) << 16) | (_grid(int(g2 * 255)) << 8) | _grid(int(b2 * 255))


def _armour_palette(zin, t, idx, sprite, colour):
    """Every colour the base sheet `sprite` of type `t` uses, mapped by `_recolour` — pairs in the FILE's byte order (`bgr`)."""
    d = {5: 0, 6: 1, 8: 2, 9: 3, 10: 7}[t]
    stem = f"{sprite:02d}" if t != 10 else f"10{sprite:02d}"
    sheet = costume_sheets.sheet_decode(zin.read(f"hero/{d}/{stem}.mph"), zin.read(f"hero/{d}/{stem}_0.mpd"), stem)
    used = sorted({v for fr in sheet.frames for row in fr.pixels for v in row if v})
    pairs = []
    for i in used:
        rgb = (sheet.palette[i][0] << 16) | (sheet.palette[i][1] << 8) | sheet.palette[i][2]
        to = _recolour(rgb, colour)
        if to != rgb and (bgr(rgb), bgr(to)) not in pairs:
            pairs.append((bgr(rgb), bgr(to)))
    assert 1 <= len(pairs) <= 255, f"armour palette {t}/{idx}: {len(pairs)} pairs"
    return pairs


def guardian_palettes(zin):
    """One `h.p` record per guardian armour piece, in PALETTE-NUMBER order (the ARMOUR tuple's 12th field — the first nine
    keep the numbers they shipped with, ★ 2026-10-09's nine take 9..17), then (★ 2026-10-14, --skill-gear) one per 2nd-gen
    armour piece in skill_gear.armour() order: every colour its base sheet uses, mapped by `_recolour` with the piece's own
    colour — pairs in the FILE's byte order (`bgr`). Pure: the same jar always gives the same records."""
    out = []
    pieces = sorted(guardian_gear.ARMOUR, key=lambda a: a[11])
    assert [a[11] for a in pieces] == list(range(len(pieces))), "guardian armour palette numbers are 0..n-1, each once"
    for t, _idx, _name, _lv, _df, _rf, _heavy, sprite, _opt, _slots, _price, _pal, colour in pieces:
        out.append(_armour_palette(zin, t, _idx, sprite, colour))
    if SKILL_GEAR:
        for _code, t, idx, _name, base, colour in skill_gear.armour():
            out.append(_armour_palette(zin, t, idx, base[7], colour))
    return out


def patch_jar(jar_bytes, placed):
    """Rebuild the jar entry by entry, replacing `t/gm.tdf` and the item bytes.
    `placed` = the enemies some map spawns (the drop plan's notion of a real source)."""
    zin = zipfile.ZipFile(io.BytesIO(jar_bytes))
    armour_files = {f"itm/{t:02d}" for t, _ in ARMOUR_FIXES}
    price_files = {f"itm/{t:02d}" for t, _ in PRICE_FIXES}
    new_files = {f"itm/{a[0]:02d}" for a in ITM_ADDS}
    add_files = {f"itm/{t:02d}" for t, _ in SHOP_ADDS}
    itm_files = armour_files | price_files | new_files
    missing = (itm_files | add_files | set(SKILL_TEXT) | {GM_TDF, COM_TDF, ITMTP_TDF, SHOP_TDF, ITM_SHOP, ENM, HP, demon_look.BSK_TDF}) - set(zin.namelist())
    if missing:
        raise SystemExit(f"jar is missing {sorted(missing)} — wrong archive?")

    # 상점에 넣을 레코드는 **가격을 고친 뒤의 것**이어야 한다 — 그래야 forshop 사본이
    # 계속 itm 본체와 바이트 동일이고, 산 것과 주운 것이 같은 아이템으로 남는다.
    patched_itm = {}
    for name in sorted(itm_files):
        data = zin.read(name)
        if name in armour_files:
            data, _ = patch_armour(name, data)
        if name in price_files:
            data, _ = patch_price(name, data)
        if name in new_files:
            data, _ = append_records(name, data)
        patched_itm[name] = data
    # The drop plan reads every item file AFTER this tool's own item edits, so a record
    # it names is one the shipped game (plus our additions) actually has.
    all_itm = {}
    for name in zin.namelist():
        tail = name[len("itm/") :] if name.startswith("itm/") else ""
        if tail.isdigit():
            all_itm[name] = patched_itm.get(name, zin.read(name))
    enm_old = zin.read(ENM)
    enm_new, drop_table, drop_log = patch_enm(enm_old, all_itm, placed)
    verify_enm(enm_old, enm_new, all_itm, placed)
    # ★ 2026-10-02: the HELL bonus slots (HELL 보너스 칸 주석) — onto the five-slot table once it is final and verified.
    enm_five = enm_new
    enm_new, _hell_plan, hell_log = add_hell_slots(enm_five, all_itm)
    hell_records = verify_hell_slots(enm_five, enm_new, all_itm, placed)
    assert hell_records == len(hell_log), f"{ENM}: {hell_records} records got HELL slots, the plan has {len(hell_log)}"
    drop_log = drop_log + hell_log

    source_records = {}
    for t, idx in SHOP_ADDS:
        name = f"itm/{t:02d}"
        blob = patched_itm.get(name, zin.read(name))
        recs = itm_records(name, blob)
        if idx >= len(recs):
            raise SystemExit(f"{name}: only {len(recs)} records, need index {idx}")
        source_records[(t, idx)] = recs[idx][1]

    # ★ 2026-10-01 망토: every shipped body sheet with the cape's four frames as 1×1 blanks (CAPE_ANIMS 주석).
    padded_body = {}
    for name in zin.namelist():
        if name.startswith(BODY_DIR) and name.endswith(".mph") and name[:-4] + "_0.mpd" in zin.namelist():
            mpd_name = name[:-4] + "_0.mpd"
            sheet = costume_sheets.sheet_decode(zin.read(name), zin.read(mpd_name), name)
            padded_body[name], padded_body[mpd_name] = costume_sheets.sheet_encode(costume_sheets.pad_sheet("body", sheet))
        elif name.startswith("hero/3/") and name.endswith(".mph") and name[:-4] + "_0.mpd" in zin.namelist():
            mpd_name = name[:-4] + "_0.mpd"
            sheet = costume_sheets.sheet_decode(zin.read(name), zin.read(mpd_name), name)
            padded_body[name], padded_body[mpd_name] = costume_sheets.sheet_encode(costume_geometry.duplicate(sheet))
    caped = {}
    # ★ 2026-10-01 신의가호 이펙트: the knight's effect sheet + nine boss pictures, and the frame sizes the
    # two new sequences place them by (`knight_blessing.py` 머리말).
    kb_names = (knight_blessing.SHEET + ".mph", knight_blessing.SHEET + "_0.mpd")
    # ★ 2026-10-03 발도: knight_iaido appends its fourteen rings AFTER knight_blessing's nine (48..61) and
    # redraws the knight's sequence 13 / appends 21 · 22 after knight_blessing's 19 · 20 — in that order.
    kb_sheet = dict(zip(kb_names, knight_iaido.patch_sheet(*knight_blessing.patch_sheet(zin.read(kb_names[0]), zin.read(kb_names[1]), zin))))
    kb_sizes = knight_iaido.sheet_sizes_after(knight_blessing.sheet_sizes_after(zin))
    icon_names = (knight_iaido.ICON_SHEET + ".mph", knight_iaido.ICON_SHEET + "_0.mpd")
    # ★ 2026-10-05 일풍격: the rogue strip (frame 2) widened to eleven cells, the icon in 6 and 10 — after the knight's.
    # ★ 2026-10-06 살상연희: its icon in the rogue strip's cell 5 (번개난타's) — after 일풍격's widening.
    # ★ 2026-10-07 도발: the berserker strip (frame 0) widened to twelve cells, the icon in 7 and 11 — after the rest.
    # ★ 2026-10-07 (2) 각성기: the three strips widened once more (버서커 13 · 나이트 13 · 로그 12) — after everything, in
    # the nearest existing colours.
    # ★★ 2026-10-09 (11) 「암살」: the rogue strip widened to 13 cells, its icon in 12 (17 − 5 — the detail window's) — last.
    icon_sheet = dict(zip(icon_names, rogue_amsal.patch_icon(*awakenings.patch_icon(*berserker_taunt.patch_icon(*rogue_slaughter.patch_icon(*rogue_ilpung.patch_icon(*knight_iaido.patch_icon(zin.read(icon_names[0]), zin.read(icon_names[1])))))))))
    # ★ 2026-10-07 도발: the berserker's effect sheet + the twelve purple copies of 사자후's pictures (46..57).
    # ★ 2026-10-07 (2) 격분: + twelve red copies (58..69) and two blood drops (70 · 71).
    # ★★ 2026-10-13 「악마화」 D2: the beast icon strip's frame 9 (대지분노) → 악의 (tools/gen_h2_atlas.py re-bakes icons.bin from it).
    bico_names = (demon_look.ICON_SHEET + ".mph", demon_look.ICON_SHEET + "_0.mpd")
    bico_sheet = dict(zip(bico_names, demon_look.patch_icon(zin.read(bico_names[0]), zin.read(bico_names[1]))))
    # ★★ 2026-10-16 엘릭서: the item-type strip (img/glb frame 17) gets its own cell 34 after the shipped 34.
    glb_names = (elixir_icon.SHEET + ".mph", elixir_icon.SHEET + "_0.mpd")
    glb_sheet = dict(zip(glb_names, elixir_icon.patch_glb(zin.read(glb_names[0]), zin.read(glb_names[1]))))
    bsk_fx_names = (berserker_taunt.SHEET + ".mph", berserker_taunt.SHEET + "_0.mpd")
    bsk_fx = dict(zip(bsk_fx_names, awakenings.patch_bsk_sheet(*berserker_taunt.patch_sheet(zin.read(bsk_fx_names[0]), zin.read(bsk_fx_names[1])))))
    rogue_fx_names = (rogue_ilpung.SHEET + ".mph", rogue_ilpung.SHEET + "_0.mpd")
    # ★ 2026-10-06 살상연희: its sixteen cuts and the sparkle (61..77) after 일풍격's eighteen (43..60).
    # ★ 2026-10-07 (2) 개화: + three petals and the flower's heart (78..81).
    rogue_fx = dict(zip(rogue_fx_names, awakenings.patch_rg_sheet(*rogue_slaughter.patch_sheet(*rogue_ilpung.patch_sheet(zin.read(rogue_fx_names[0]), zin.read(rogue_fx_names[1]))))))

    buf = io.BytesIO()
    armour = added = added_records = 0
    costume_added = {}  # jar name -> bytes (COSTUME_DATE 주석)
    with zipfile.ZipFile(buf, "w") as zout:
        # ★ 2026-10-16 신전 보스 레이드: obj 시트 끝에 붉은 불꽃 네 프레임(65..68) — 원본 프레임 바이트는 그대로 (raid_gate.obj_with_raid_flame).
        raid_obj = raid_gate.obj_with_raid_flame(zin.read("m/obj.mph"), zin.read("m/obj_0.mpd"))
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename == GM_TDF:
                data = tdf_encode(patch_gm(tdf_decode(data)))
            elif info.filename == demon_look.BSK_TDF:
                data = tdf_encode(demon_look.patch_bsk(tdf_decode(data, demon_look.BSK_TDF)), demon_look.BSK_TDF)
            elif info.filename == "t/bsm.tdf":
                rows = tdf_decode(data, "t/bsm.tdf")
                assert len(rows) == 15
                rows.append("보증서를 사용하여 장비를 보존했습니다.".encode("euc-kr"))
                data = tdf_encode(rows, "t/bsm.tdf")
            elif info.filename == "m/obj.mph":
                data = raid_obj[0]
            elif info.filename == "m/obj_0.mpd":
                data = raid_obj[1]
            elif info.filename == COM_TDF:
                data = tdf_encode(patch_com(tdf_decode(data, COM_TDF)), COM_TDF)
            elif info.filename == ITMTP_TDF:
                data = tdf_encode(patch_itmtp(tdf_decode(data, ITMTP_TDF)), ITMTP_TDF)
            elif info.filename == SHOP_TDF:
                data = tdf_encode(patch_shop(tdf_decode(data, SHOP_TDF)), SHOP_TDF)
            elif info.filename in SKILL_TEXT:
                data = tdf_encode(patch_skill_text(info.filename, tdf_decode(data, info.filename)), info.filename)
            elif info.filename in CAPE_ANIMS:
                if info.filename == "hero/hero2":
                    data = venom_effect.patch(data)
                if info.filename == knight_blessing.ANIM:
                    data = knight_blessing.patch_anim(data, kb_sizes)  # before the cape: the jump poses get one too
                    data = knight_iaido.patch_anim(data, kb_sizes)  # 발도 (sequence 13) · the ring waves (21 · 22)
                data, caped[info.filename] = hero_cape.patch(data)
                if info.filename == knight_iaido.ANIM:
                    # AFTER the cape: 천공의검 copies 발도's (caped) poses — sequence 25; its flying slash = sequence 26
                    data = awakenings.patch_knt_anim(data, kb_sizes)
                if info.filename == rogue_ilpung.ANIM:
                    # AFTER the cape: the spin moves each direction's (caped) body into the other slots
                    data = rogue_ilpung.patch_anim(data)  # 일풍격 = sequence 19 (18 = an empty filler)
                    data = rogue_slaughter.patch_anim(data)  # 살상연희 = sequence 14 (25 frames) · its cuts = sequence 20
                    data = awakenings.patch_rg_anim(data)  # ★ (2) 개화 = sequence 21 (25 frames) · the flower = sequence 22
                if info.filename == berserker_taunt.ANIM:
                    # AFTER the cape: 도발 copies 사자후's (caped) poses
                    data = berserker_taunt.patch_anim(data)  # 도발 = sequence 19 (18 = an empty filler)
                    data = awakenings.patch_bsk_anim(data)  # ★ (2) 격분 = sequences 20 · 21 · the blood aura 22
                data = costume_geometry.patch_animation(data)
                # ★★ 2026-10-13 「악마화」 D2 — LAST: 18 demon copies of the body sequences + the transform (copies of the directional-leg stand)
                data = demon_look.patch_anim(data, int(info.filename[-1]))
                if info.filename == knight_iaido.ANIM:
                    # ★★ 2026-10-17 발도 특화 아이템 — after the demon copies (no number moves): 발도 with the radius 4 · 5 ring
                    #   (sequences 54 · 55, copies of the finished 13) and 신의가호's crescents of those radii (56 · 57)
                    data = knight_iaido.patch_reach_anim(data, kb_sizes)
            elif info.filename in bico_sheet:
                data = bico_sheet[info.filename]
            elif info.filename in glb_sheet:
                data = glb_sheet[info.filename]
            elif info.filename in rogue_fx:
                data = rogue_fx[info.filename]
            elif info.filename in bsk_fx:
                data = bsk_fx[info.filename]
            elif info.filename in kb_sheet:
                data = kb_sheet[info.filename]
            elif info.filename in icon_sheet:
                data = icon_sheet[info.filename]
            elif info.filename in padded_body:
                data = padded_body[info.filename]
            elif info.filename == HP:
                data = patch_hp(data, guardian_palettes(zin))
            elif info.filename in itm_files:
                if info.filename in armour_files:
                    _d, n = patch_armour(info.filename, data)
                    armour += n
                data = patched_itm[info.filename]
                added_records += sum(1 for a in ITM_ADDS if f"itm/{a[0]:02d}" == info.filename)
            elif info.filename == ITM_SHOP:
                data, n = patch_armour(info.filename, data)
                armour += n
                data, added = add_to_shop(data, source_records)
            elif info.filename == ENM:
                data = enm_new
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.compress_type = info.compress_type
            out.external_attr = info.external_attr
            zout.writestr(out, data)
        # The costume sheets — NEW members after every shipped one (COSTUME_DATE 주석).
        shipped_names = set(zin.namelist())
        for cdir in costume_sheets.list_costumes():
            try:
                members, _notes = costume_sheets.costume_members(cdir)
            except costume_sheets.SheetError as e:
                raise SystemExit(f"costume {cdir}:\n{e}")
            for name, data in sorted(members.items()):
                if name in shipped_names or name in costume_added:
                    raise SystemExit(f"{name}: the jar already has it — a costume number must not shadow a shipped sheet or another costume")
                out = zipfile.ZipInfo(name, date_time=COSTUME_DATE)
                out.compress_type = zipfile.ZIP_DEFLATED
                out.external_attr = 0o100644 << 16
                zout.writestr(out, data)
                costume_added[name] = data
        # ★ 2026-10-09 층 치마(207 코트 자락 · 210 트인 치마): 드레스 × 다리를 가진 다른 의상마다 `hero/3/<드레스 × 1000 + 신발>`
        #   = 아랫층 + 그 신발의 다리 + 윗층 — 드레스를 입고 신발 아바타를 바꾸면 그 다리가 보인다(avatar_state::skirt_over_shoes).
        try:
            skirt_combos = costume_sheets.skirt_combo_members(costume_sheets.list_costumes())
        except costume_sheets.SheetError as e:
            raise SystemExit(f"costume skirt combinations:\n{e}")
        for name, data in sorted(skirt_combos.items()):
            if name in shipped_names or name in costume_added:
                raise SystemExit(f"{name}: the jar already has it — a skirt combination must not shadow another sheet")
            out = zipfile.ZipInfo(name, date_time=COSTUME_DATE)
            out.compress_type = zipfile.ZIP_DEFLATED
            out.external_attr = 0o100644 << 16
            zout.writestr(out, data)
            costume_added[name] = data
        # ★ 2026-10-08 가디언 무기 10: `guardian/weapons/<종류><번호>/` 의 PNG → `hero/6/<종류><번호>` 새 멤버 (의상 시트와 같은 포장).
        # ★ 2026-10-09 단검(타입 3)은 보조손 시트 `hero/7/<종류><번호>` 도 — 로그가 둘째 단검으로 끼면 파츠 로더 케이스 7 이 그것을 싣는다
        #   (없으면 장비창 「적용 → 예」에서 워커가 죽었다; guardian_gear.OFFHAND_OF_TYPE). 아래 검사가 jar 의 모든 단검 시트 쌍을 잰다.
        # ★★ 2026-10-14 (--skill-gear) + 2차 무기 11: `guardian/skill_weapons/<종류><번호>/` (tools_re/skill_gear.py) — 같은 포장.
        weapon_dirs = [(GUARDIAN_WEAPON_DIR, t, nn, wname, "guardian_gear.py") for t, nn, wname, *_rest in guardian_gear.WEAPONS]
        if SKILL_GEAR:
            weapon_dirs += [(SKILL_WEAPON_DIR, t, nn, wname, "skill_gear.py") for _c, t, nn, wname, _b in skill_gear.weapons()]
        for wroot, t, nn, wname, tool in weapon_dirs:
            stem = guardian_gear.weapon_stem(t, nn)
            wdir = os.path.join(wroot, stem)
            if not os.path.isdir(wdir):
                raise SystemExit(f"{wdir}: no sheet for {wname} — run tools_re/{tool} first")
            sheets = [(6, guardian_gear.PART_OF_TYPE[t])] + ([(7, guardian_gear.OFFHAND_OF_TYPE[t])] if t in guardian_gear.OFFHAND_OF_TYPE else [])
            for d, part in sheets:
                try:
                    sheet, _notes = costume_sheets.load_part(wdir, part)
                except costume_sheets.SheetError as e:
                    raise SystemExit(f"guardian weapon {stem} ({wname}) {part}:\n{e}\n— run tools_re/guardian_gear.py (it adds a missing part)")
                mph, mpd = costume_sheets.sheet_encode(sheet)
                back = costume_sheets.sheet_decode(mph, mpd, stem)
                assert [f.pixels for f in back.frames] == [f.pixels for f in sheet.frames]
                for name, blob in ((f"hero/{d}/{stem}.mph", mph), (f"hero/{d}/{stem}_0.mpd", mpd)):
                    if name in shipped_names or name in costume_added:
                        raise SystemExit(f"{name}: the jar already has it — a guardian weapon number must not shadow a shipped sheet")
                    out = zipfile.ZipInfo(name, date_time=COSTUME_DATE)
                    out.compress_type = zipfile.ZIP_DEFLATED
                    out.external_attr = 0o100644 << 16
                    zout.writestr(out, blob)
                    costume_added[name] = blob
        # ★★ 2026-10-13 「악마화」 D2: the demon body sheet hero/1/300 (9 blank human frames + the demon frames; demon_look.py).
        # ★ 2026-10-14 밤 (4): 75 demon frames, compressed IDAT from frame demon_look.COMPRESSED_FROM (never flipped — checked below).
        try:
            demon_members = demon_look.members()
        except costume_sheets.SheetError as e:
            raise SystemExit(f"demon look:\n{e}")
        for name, data in demon_members.items():
            if name in shipped_names or name in costume_added:
                raise SystemExit(f"{name}: the jar already has it — the demon sheet must not shadow a shipped sheet or a costume")
            out = zipfile.ZipInfo(name, date_time=COSTUME_DATE)
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
            costume_added[name] = data
        # ★ A3: the costume catalog (names · bags · descriptions, EUC-KR) — STORED, because the host
        #   reads it out of the jar without an inflater (`hero2java::h2_zip_stored`).
        try:
            catalog = costume_sheets.catalog_bytes(costume_sheets.list_costumes())
        except costume_sheets.SheetError as e:
            raise SystemExit(f"costume catalog:\n{e}")
        if catalog is not None:
            name = costume_sheets.CATALOG_MEMBER
            if name in shipped_names:
                raise SystemExit(f"{name}: the jar already has it")
            out = zipfile.ZipInfo(name, date_time=COSTUME_DATE)
            out.compress_type = zipfile.ZIP_STORED
            out.external_attr = 0o100644 << 16
            zout.writestr(out, catalog)
            costume_added[name] = catalog
    if armour < len(ARMOUR_FIXES):
        raise SystemExit(f"armour: wrote {armour} byte(s), expected at least {len(ARMOUR_FIXES)}")
    if added != len(SHOP_ADDS):
        raise SystemExit(f"shop: added {added} record(s), expected {len(SHOP_ADDS)}")
    # Read it back: the patched tables must parse and say what we wrote.
    check = zipfile.ZipFile(io.BytesIO(buf.getvalue()))
    rows = tdf_decode(check.read(GM_TDF))
    assert rows[11] == b"ACCOUNT" and rows[19] == "계정창고".encode("euc-kr"), "gm.tdf verify"
    assert len(rows) == GM_SHIPPED_ROWS + len(GM_APPEND), "gm.tdf row count"
    # The host names these rows by NUMBER (`Hero2Java::GM_ROW_*`), so the order is part of the
    # contract: a row inserted mid-list would put another message in every box after it.
    assert [r.decode("euc-kr") for r in rows[GM_SHIPPED_ROWS:]] == GM_APPEND, "gm.tdf appended rows"
    rows = tdf_decode(check.read(COM_TDF), COM_TDF)
    assert len(rows) == COM_SHIPPED_ROWS + 1, "com.tdf row count"  # ★ 2026-10-08: + the fee text at [56]
    for k, (_old, new) in list(COM_REPLACE.items()) + list(COM_GUARDIAN.items()):
        assert rows[k] == new.encode("euc-kr"), f"com.tdf row {k} verify"
    assert rows[COM_FEE_ROW] == COM_MODE3_LABEL.encode("euc-kr") and rows[COM_FEE_NEW_ROW].startswith("본 게임은".encode("euc-kr")), "com.tdf guardian rows"
    rows = tdf_decode(check.read(ITMTP_TDF), ITMTP_TDF)
    assert len(rows) == ITMTP_SHIPPED_ROWS + len(ITMTP_APPEND), "itmtp.tdf row count"
    assert [r.decode("euc-kr") for r in rows[ITMTP_SHIPPED_ROWS:]] == ITMTP_APPEND, "itmtp.tdf appended rows"
    rows = tdf_decode(check.read(SHOP_TDF), SHOP_TDF)
    assert len(rows) == SHOP_SHIPPED_ROWS + len(SHOP_APPEND), "shop.tdf row count"
    assert [r.decode("euc-kr") for r in rows[SHOP_SHIPPED_ROWS:]] == SHOP_APPEND, "shop.tdf appended rows"
    for name, (shipped, replace, append) in SKILL_TEXT.items():
        rows = tdf_decode(check.read(name), name)
        assert len(rows) == shipped + len(append), f"{name} row count"
        for k, (_old, new) in replace.items():
            assert rows[k] == new.encode("euc-kr"), f"{name} row {k} verify"
        assert [r.decode("euc-kr") for r in rows[shipped:]] == append, f"{name} appended rows"
    for name in sorted(armour_files | {ITM_SHOP}):
        blob = check.read(name)
        for key, off, _got in armour_sites(name, blob):
            assert blob[off] == ARMOUR_FIXES[key][2], f"{name} {key} armour verify"
    for (t, i), (_expect, _old, new_price) in PRICE_FIXES.items():
        rec = itm_records(f"itm/{t:02d}", check.read(f"itm/{t:02d}"))[i][1]
        assert struct.unpack_from("<I", rec, 0)[0] == new_price, f"itm/{t:02d}[{i}] price verify"
    for t, idx, label, price, desc, tail in ITM_ADDS:
        name = f"itm/{t:02d}"
        recs = itm_records(name, check.read(name))
        assert idx < len(recs), f"{name}[{idx}] was not appended"
        rec = recs[idx][1]
        n = rec[4]
        assert rec[5 : 5 + n].decode("euc-kr") == label, f"{name}[{idx}] name verify"
        assert struct.unpack_from("<I", rec, 0)[0] == price, f"{name}[{idx}] price verify"
        ds = desc.encode("euc-kr")
        assert rec[5 + n] == len(ds) and rec[6 + n : 6 + n + len(ds)] == ds, f"{name}[{idx}] description verify"
        assert rec[6 + n + len(ds) :] == tail, f"{name}[{idx}] body verify"
    # ★ 2026-10-09: every guardian recipe names live records — the result its own name, each material the name its line says.
    db_name = {}
    for t, idx, *_ in GUARDIAN_RECIPE_ADDS:
        for key in (GUARDIAN_RECIPES[idx - RECIPE_FIRST_NEW][0], *GUARDIAN_RECIPES[idx - RECIPE_FIRST_NEW][1:]):
            if key not in db_name:
                recs = itm_records(f"itm/{key[0]:02d}", check.read(f"itm/{key[0]:02d}"))
                assert key[1] < len(recs), f"recipe {idx}: {key} is not in the item DB"
                rec = recs[key[1]][1]
                db_name[key] = rec[5 : 5 + rec[4]].decode("euc-kr")
        out, *mats = GUARDIAN_RECIPES[idx - RECIPE_FIRST_NEW]
        assert all(db_name[m] == RECIPE_NAMES[m] for m in mats), f"recipe {idx}: a material line is not its record's name {[(db_name[m], RECIPE_NAMES[m]) for m in mats]}"
        assert db_name[out] == GUARDIAN_RECIPE_ADDS[idx - RECIPE_FIRST_NEW][2], f"recipe {idx}: not named after its result"
    shop_order = [g for g, _ in _shop_rows(check.read(ITM_SHOP))]
    shop = {g: p for g, p in _shop_rows(check.read(ITM_SHOP))}
    assert len(shop) == SHOP_RECORDS + len(SHOP_ADDS), "forshop row count"
    for (t, idx), (at, aidx) in SHOP_PLACE_AFTER.items():  # ★★ 2026-10-16 엘릭서 right below 포도주
        k = shop_order.index((at << 8) | aidx)
        assert shop_order[k + 1] == (t << 8) | idx, f"forshop: 0x{(t << 8) | idx:04x} is not right after 0x{(at << 8) | aidx:04x}"
    rest = [g for g in shop_order if (g >> 8, g & 0xFF) not in SHOP_PLACE_AFTER]
    assert rest == sorted(rest), "forshop: the other records keep the global-index order"
    glb = costume_sheets.sheet_decode(check.read(glb_names[0]), check.read(glb_names[1]), glb_names[0]).frames[elixir_icon.STRIP_FRAME]
    own = elixir_icon.SHIPPED_CELLS * elixir_icon.CELL
    assert (glb.w, glb.h) == (elixir_icon.CELL, own + elixir_icon.CELL * len(elixir_icon.OWN_CELLS)), "img/glb strip with the own cells"
    assert glb.pixels[:own] == costume_sheets.sheet_decode(zin.read(glb_names[0]), zin.read(glb_names[1]), "shipped glb").frames[elixir_icon.STRIP_FRAME].pixels, "img/glb shipped cells"
    for k, (_t, _i, name, _p) in enumerate(elixir_icon.OWN_CELLS):  # ★ 2026-10-21: every own cell (엘릭서 · 상점주인의 혼 · 비행서) is drawn
        cell = glb.pixels[own + k * elixir_icon.CELL:own + (k + 1) * elixir_icon.CELL]
        assert any(v for row in cell for v in row), f"img/glb: the {name} cell is drawn"
    for t, idx in SHOP_ADDS:
        gidx = (t << 8) | idx
        assert gidx in shop, f"forshop is missing 0x{gidx:04x}"
        assert shop[gidx] == itm_records(f"itm/{t:02d}", check.read(f"itm/{t:02d}"))[idx][1], \
            f"forshop 0x{gidx:04x} is not byte-identical to its itm record"
    assert check.read(ENM) == enm_new, "enm/enm verify"
    # The costume palettes: the shipped 189 untouched, ours appended in HP_APPEND's order — the host
    # names them by number (`AvatarPatch::PALETTE_COUNT` bounds what it will write).
    hp = hp_records(check.read(HP))
    assert len(hp) == HP_SHIPPED_RECORDS + len(HP_APPEND) + len(guardian_gear.ARMOUR) + (len(skill_gear.armour()) if SKILL_GEAR else 0), "h.p record count"
    assert hp[:HP_SHIPPED_RECORDS] == hp_records(zin.read(HP)), "h.p shipped records verify"
    assert hp[HP_SHIPPED_RECORDS:] == hp_appended(guardian_palettes(zin)), "h.p appended records verify"
    # ★ 2026-10-08: every guardian armour record names its palette (body[17..20]) and a shipped sheet (body[16]).
    # ★ 2026-10-14: the record's description (the option lines) sits before the tail — strip it (body[0] = its length).
    armour_pals = [(t, idx, sprite, HP_SHIPPED_RECORDS + len(HP_APPEND) + pal) for t, idx, _n, _l, _d, _r, _h, sprite, _o, _s, _p, pal, _c in guardian_gear.ARMOUR]
    if SKILL_GEAR:
        first = HP_SHIPPED_RECORDS + len(HP_APPEND) + len(guardian_gear.ARMOUR)
        armour_pals += [(t, idx, base[7], first + k) for k, (_c, t, idx, _n, base, _col) in enumerate(skill_gear.armour())]
    for t, idx, sprite, pal_no in armour_pals:
        rec = itm_records(f"itm/{t:02d}", check.read(f"itm/{t:02d}"))[idx][1]
        body = rec[5 + rec[4] :]
        body = bytes([0]) + body[1 + body[0] :]
        assert len(body) == 28 and body[16] == sprite and struct.unpack_from("<i", body, 17)[0] == pal_no, f"armour {t}/{idx}"
        d = {5: 0, 6: 1, 8: 2, 9: 3, 10: 7}[t]
        assert f"hero/{d}/{sprite:02d}.mph" in check.namelist() or f"hero/{d}/10{sprite:02d}.mph" in check.namelist(), f"armour {t}/{idx}: base sheet"
    # ★★ 2026-10-14 (--skill-gear): every 2nd-gen recipe makes its item from live records, and every 2nd-gen weapon has its sheet.
    for _t, idx, rname, _price, desc, tail in SKILL_RECIPE_ADDS:
        keys = [(tail[k], tail[k + 1]) for k in (0, 2, 4, 6)]
        got = []
        for key in keys:
            recs = itm_records(f"itm/{key[0]:02d}", check.read(f"itm/{key[0]:02d}"))
            assert key[1] < len(recs), f"2nd recipe {idx}: {key} is not in the item DB"
            got.append(recs[key[1]][1][5 : 5 + recs[key[1]][1][4]].decode("euc-kr"))
        assert got[0] == rname and got[1:] == desc.split(";"), f"2nd recipe {idx}: {got} vs {rname} / {desc}"
    for _c, t, nn, wname, _b in (skill_gear.weapons() if SKILL_GEAR else []):
        assert f"hero/6/{guardian_gear.weapon_stem(t, nn)}.mph" in check.namelist(), f"{wname}: no weapon sheet"
    # The costume sheets read back as written and decode as the game will (the flip-sheet form).
    for name, data in costume_added.items():
        assert check.read(name) == data, f"{name} verify"
    if costume_sheets.CATALOG_MEMBER in costume_added:
        assert check.getinfo(costume_sheets.CATALOG_MEMBER).compress_type == zipfile.ZIP_STORED, "costume catalog must be stored"
    for name in costume_added:
        if name.endswith(".mph"):
            costume_sheets.sheet_decode(check.read(name), check.read(name[:-4] + "_0.mpd"), name,
                                        compressed_from=demon_look.COMPRESSED_FROM if name == demon_look.BODY_MEMBERS[0] else None)
    # ★ 2026-10-09: every dagger sprite (`hero/6/3NN`, the item sheets — costume sheets have four digits) has its second-dagger
    # sheet `hero/7/3NN` — a rogue wears daggers in the off-hand slot too and the parts loader's case 7 loads that path; a
    # missing one killed the worker at the EQUIPMENT window's 「적용 → 예」 (가디언 단검 333 · 334, guardian_gear.OFFHAND_OF_TYPE).
    jar_names = set(check.namelist())
    daggers = sorted(s for s in (n[len("hero/6/") : -4] for n in jar_names if n.startswith("hero/6/3") and n.endswith(".mph")) if len(s) == 3 and s.isdigit())
    lone = [s for s in daggers if f"hero/7/{s}.mph" not in jar_names or f"hero/7/{s}_0.mpd" not in jar_names]
    assert daggers and not lone, f"dagger sheets without a second-dagger sheet hero/7/<n>: {lone}"
    # ★ 망토: the three class animations carry it, and EVERY body sheet in the jar has the frames it
    # draws (a frame past a sheet's end throws in the game's drawer — CAPE_ANIMS 주석).
    assert sorted(caped) == sorted(CAPE_ANIMS) and all(caped.values()), f"cape: {caped}"
    want = len(costume_sheets.PARTS["body"]["sizes"])
    bodies = [n for n in check.namelist() if n.startswith(BODY_DIR) and n.endswith(".mph")]
    assert len(bodies) >= sum(n.startswith(BODY_DIR) and n.endswith('.mph') for n in padded_body), "body sheets lost"
    for name in bodies:
        demon_sheet = name == demon_look.BODY_MEMBERS[0]
        blank_sheet = name == demon_look.BLANK_MEMBERS[0]  # ★ 2026-10-15 HD: the blank body sheet 301 (the same frame count, 1×1 each)
        frames = costume_sheets.sheet_decode(check.read(name), check.read(name[:-4] + "_0.mpd"), name,
                                             compressed_from=demon_look.COMPRESSED_FROM if demon_sheet else None).frames
        if demon_sheet or blank_sheet:
            assert len(frames) == demon_look.BODY_BASE + demon_look.FRAMES, f"{name}: {len(frames)} frames — the demon sheet"
            if blank_sheet:
                assert all(f.w == 1 and f.h == 1 for f in frames), f"{name}: the blank sheet's frames are 1×1"
            continue
        assert len(frames) == want, f"{name}: {len(frames)} frames — the cape needs {want}"
    for name in check.namelist():
        if name.startswith("hero/3/") and name.endswith(".mph"):
            frames = costume_sheets.sheet_decode(check.read(name), check.read(name[:-4] + "_0.mpd"), name).frames
            assert len(frames) == costume_geometry.STRIDE * costume_geometry.BANKS, f"{name}: missing directional leg frames"
    for name in CAPE_ANIMS:
        _, anim_parts, sequences = venom_effect.decode(check.read(name))
        for sequence in sequences:
            for frame in sequence:
                for sprites in frame:
                    for part, index, _, _ in sprites:
                        if part & 127 == 3 and index != 255:
                            assert 11 <= index < 55, f"{name}: invalid directional leg frame {index}"
        # ★ 2026-10-10: a mirrored sprite is drawn from the copy the part loader makes ONLY for the frames the part's
        # flip list (the header's first list, cf.b[part]) names — an unnamed one is a null image the drawer skips, i.e.
        # that part vanishes in that pose. The shipped game has none; vC 98 had 225..300 (every mirrored leg pose).
        for p, (flip_list, _second) in enumerate(anim_parts):
            unlisted = costume_geometry.flipped_frames(sequences, p) - set(flip_list)
            assert not unlisted, f"{name}: part {p} is drawn mirrored from frames {sorted(unlisted)} its flip list lacks — invisible in those poses"
            assert len(flip_list) < 128, f"{name}: part {p} flip list of {len(flip_list)} — the loader reads its length as a signed byte"
    # ★ 신의가호 이펙트: the knight's two new sequences keep the shipped frame counts of the ones they stand
    # in for (the action's length and the wave's frame both come from those), and every effect picture the
    # knight's animation names is in its effect sheet.
    _h, _p, kseqs = venom_effect.decode(check.read(knight_blessing.ANIM))
    # ★ 2026-10-03: + 발도 (sequence 13, six frames — the action's length follows the sheet) and the two ring
    # waves (21 · 22, at least the wave's life + 1 frames — the painter hands `cb.d` over as the frame).
    # ★ 2026-10-07 (2): + 천공의검 (sequence 25) and its flying slash (26, life + 1 frames). ★ 2026-10-08: 25 is SKY_FRAMES (18; ★ (3) 27) long —
    #   LONGER than sequence 19 (six, what cf.a()V would put in a.n[15]): the cast wrapper stores SKY_FRAMES into a.n[15] itself
    #   (CombatPatch::frames_slot), so the sheet must have a pose for every one of those frames.
    assert len(kseqs) == awakenings.SKY_WAVE_SEQ + 1 + demon_look.MAPPED_COUNT + 1 + len(demon_look.CAST_SEQS) + knight_iaido.REACH_SEQS, f"{knight_blessing.ANIM}: {len(kseqs)} sequences"  # ★ D2: + 18 demon copies + the transform · ★★ 2026-10-17 + 발도's radius 4 · 5
    assert knight_iaido.REACH_SEQ == awakenings.SKY_WAVE_SEQ + 1 + demon_look.MAPPED_COUNT + 1 + len(demon_look.CAST_SEQS), "★★ 발도's big rings right after the demon copies (= CombatPatch::IAIDO_REACH_SEQ)"
    assert all(len(kseqs[knight_iaido.REACH_SEQ + k]) == knight_iaido.IAIDO_FRAMES for k in range(len(knight_iaido.REACH_RADII))), "★★ the big-ring 발도 = six frames"
    assert len(kseqs[awakenings.SKY_SEQ]) == awakenings.SKY_FRAMES >= len(kseqs[19]), "천공의검 = SKY_FRAMES poses (at least sequence 19's)"
    assert len(kseqs[awakenings.SKY_WAVE_SEQ]) > awakenings.SKY_WAVE_LIFE, "the slash flies SKY_WAVE_LIFE tiles"
    assert len(kseqs[knight_iaido.IAIDO_WAVE_SEQ]) > knight_iaido.BLESSED_WAVE_LIFE, "발도 slash frames"
    assert len(kseqs[knight_iaido.CHARGE_SEQ]) == len(kseqs[knight_iaido.BASH_SRC_SEQ]), "방패 돌진 frame count = 방패치기's"
    assert len(kseqs[knight_blessing.SLAM_SEQ]) == len(kseqs[knight_blessing.BLUDGEON_SEQ]), "jump slam frame count"
    assert len(kseqs[knight_blessing.FLAME_SEQ]) == len(kseqs[knight_blessing.WAVE_SEQ]), "flame frame count"
    assert len(kseqs[knight_iaido.IAIDO_SEQ]) == knight_iaido.IAIDO_FRAMES, "발도 frame count"
    assert len(kseqs[knight_iaido.WAVE_RING_SEQ]) > knight_iaido.WAVE_LIFE and len(kseqs[knight_iaido.BLESSED_WAVE_RING_SEQ]) > knight_iaido.BLESSED_WAVE_LIFE, "ring frames"
    kfx = costume_sheets.sheet_decode(check.read(kb_names[0]), check.read(kb_names[1]), kb_names[0], compressed_from=knight_iaido.REACH_AT).frames
    used = {f for s in kseqs for fr in s for d in fr for p, f, _x, _y in d if p & 0x7F == knight_blessing.EFFECT_PART}
    assert max(used) < len(kfx), f"{knight_blessing.SHEET}: frame {max(used)} drawn, {len(kfx)} in the sheet"
    icon = costume_sheets.sheet_decode(check.read(icon_names[0]), check.read(icon_names[1]), icon_names[0]).frames[knight_iaido.ICON_STRIP]
    assert (icon.w, icon.h) == (awakenings.icon_cells_after()[1] * knight_iaido.ICON, knight_iaido.ICON), "the knight's icon strip (13 cells since 천공의검)"
    # ★ 2026-10-05 일풍격: sequence 19 (= kind 15 + 4; its frame count is the action's length a.n[15]), the rogue's
    # eleven-cell icon strip, and every effect picture the rogue's animation names is in hero/4/2.
    _h, _p, rseqs = venom_effect.decode(check.read(rogue_ilpung.ANIM))
    assert len(rseqs[rogue_ilpung.ILPUNG_SEQ]) == rogue_ilpung.ILPUNG_FRAMES, "일풍격 sequence"
    # ★ 2026-10-06 살상연희: sequence 14 = 25 frames (its a.n[10] — guest_anchors::RoguePatch::SEQ_FRAMES) and the cuts
    # (sequence 20, frame for frame) — the last sequence.
    # ★ 2026-10-07 (2): + 개화 (sequence 21 = 25 frames, a.n[16] = sequence 20's) and the flower (22, ten frames).
    assert len(rseqs) == awakenings.FLOWER_SEQ + 1 + demon_look.MAPPED_COUNT + 1 + len(demon_look.CAST_SEQS), f"{rogue_ilpung.ANIM}: {len(rseqs)} sequences"  # ★ D2
    assert len(rseqs[rogue_slaughter.SLAUGHTER_SEQ]) == len(rseqs[rogue_slaughter.SLASH_SEQ]) == rogue_slaughter.FRAMES, "살상연희 sequences"
    assert len(rseqs[awakenings.BLOOM_SEQ]) == rogue_slaughter.FRAMES and len(rseqs[awakenings.FLOWER_SEQ]) == awakenings.FLOWER_FRAMES, "개화 sequences"
    rfx = costume_sheets.sheet_decode(check.read(rogue_fx_names[0]), check.read(rogue_fx_names[1]), rogue_fx_names[0]).frames
    used = {f for s in rseqs for fr in s for d in fr for p, f, _x, _y in d if p & 0x7F == rogue_ilpung.EFFECT_PART}
    assert max(used) < len(rfx), f"{rogue_ilpung.SHEET}: frame {max(used)} drawn, {len(rfx)} in the sheet"
    # ★★ 2026-10-13 「악마화」 D2: every demon copy names body frames the demon sheet has (and nothing else of the body), each class
    # animation ends with MAPPED_COUNT copies + the transform, the beast skill text and the icon are the demon's.
    dsheet = costume_sheets.sheet_decode(check.read(demon_look.BODY_MEMBERS[0]), check.read(demon_look.BODY_MEMBERS[1]), demon_look.BODY_STEM,
                                         compressed_from=demon_look.COMPRESSED_FROM)
    assert len(dsheet.frames) == demon_look.BODY_BASE + demon_look.FRAMES, "the demon body sheet"
    for cls in range(3):
        _h, _p, dseqs = venom_effect.decode(check.read(f"hero/hero{cls}"))
        # ★ 2026-10-14 밤 (4): the demon frames are compressed IDAT — the game rewrites PNG bytes only for the frames a part's
        # flip list names (am.a([BIIII)V), so the body flip list must stay inside the human slots 0..COMPRESSED_FROM−1.
        assert all(f < demon_look.COMPRESSED_FROM for f in _p[demon_look.BODY_DIR][0]), \
            f"hero/hero{cls}: the body flip list {_p[demon_look.BODY_DIR][0]} names a compressed demon frame"
        assert len(dseqs) == demon_look.SHIPPED_SEQS[cls] + demon_look.MAPPED_COUNT + 1 + len(demon_look.CAST_SEQS) + (knight_iaido.REACH_SEQS if cls == 1 else 0), f"hero/hero{cls}: {len(dseqs)} sequences"  # ★★ 2026-10-17 + the knight's 발도 radius 4 · 5 after the demon ones
        for i_m, s in enumerate(demon_look.MAPPED[cls]):
            d = dseqs[demon_look.SHIPPED_SEQS[cls] + i_m]
            assert len(d) == len(dseqs[s]), f"hero/hero{cls}: demon copy of {s}"
            for fr in d:
                for sprites in fr:
                    body = [sp for sp in sprites if sp[0] & 0x7F != 4]
                    assert all(sp[0] & 0x7f not in (6, 7) for sp in body), f"hero/hero{cls}: demon sprite {body}"
        assert len(dseqs[demon_look.transform_seq_no(cls)]) == demon_look.TRANSFORM_FRAMES, f"hero/hero{cls}: the transform sequence"
    _bsk_rows = tdf_decode(check.read(demon_look.BSK_TDF), demon_look.BSK_TDF)
    assert _bsk_rows[18] == "악의".encode("euc-kr"), "bsk row 18 is 악의"
    for _k, (_old, _new) in demon_look.BSK_REPLACE.items():  # ★ D3: rows 7 · 21 carry the toggles' line
        assert _bsk_rows[_k] == _new.encode("euc-kr"), f"bsk row {_k}"
    dicon = costume_sheets.sheet_decode(check.read(demon_look.ICON_SHEET + ".mph"), check.read(demon_look.ICON_SHEET + "_0.mpd"), demon_look.ICON_SHEET).frames
    assert len(dicon) == demon_look.ICON_FRAMES_BUILT and any(v for row in dicon[demon_look.ICON_AT].pixels for v in row), "the bico frame 9 icon"
    # ★★ 2026-10-16: the 악의 versions — 12 rows after the shipped 26 and six icons after the shipped 13, in demon_skin.SKIN's order
    assert len(_bsk_rows) == demon_look.BSK_ROWS and _bsk_rows[demon_look.BSK_SHIPPED_ROWS:] == demon_skin.bsk_append_rows(), "bsk rows 26..37"
    assert all(any(v for row in f.pixels for v in row) for f in dicon[demon_look.ICON_FRAMES:]), "the bico frames 13..18"
    ricon = costume_sheets.sheet_decode(check.read(icon_names[0]), check.read(icon_names[1]), icon_names[0]).frames[rogue_ilpung.ICON_STRIP]
    assert (ricon.w, ricon.h) == (rogue_amsal.CELLS * rogue_ilpung.ICON, rogue_ilpung.ICON), "the rogue's icon strip (13 cells since 암살)"
    # ★ 2026-10-07 도발: sequence 19 (= kind 15 + 4, a.n[15]) after an 18 — twenty sequences, so the berserker's a.n
    # (`cf.a()V`: new byte[S − 3]) has its index 15 — every effect picture it names in hero/4/0, the twelve-cell strip.
    _h, _p, bseqs = venom_effect.decode(check.read(berserker_taunt.ANIM))
    # ★ 2026-10-07 (2): + 격분 (20 · 21, eight frames each — a.n[16] = sequence 20's) and the blood aura (22).
    assert len(bseqs) == awakenings.FURY_AURA_SEQ + 1 + demon_look.MAPPED_COUNT + 1 + len(demon_look.CAST_SEQS), f"{berserker_taunt.ANIM}: {len(bseqs)} sequences"  # ★ D2
    assert len(bseqs[berserker_taunt.TAUNT_SEQ]) == berserker_taunt.ROAR_FRAMES, "도발 sequence"
    assert len(bseqs[awakenings.FURY_SEQ]) == len(bseqs[awakenings.FURY_EXPLODE_SEQ]) == awakenings.FURY_FRAMES, "격분 sequences"
    bfx = costume_sheets.sheet_decode(check.read(bsk_fx_names[0]), check.read(bsk_fx_names[1]), bsk_fx_names[0]).frames
    used = {f for s in bseqs for fr in s for d in fr for p, f, _x, _y in d if p & 0x7F == berserker_taunt.EFFECT_PART}
    assert max(used) < len(bfx), f"{berserker_taunt.SHEET}: frame {max(used)} drawn, {len(bfx)} in the sheet"
    bicon = costume_sheets.sheet_decode(check.read(icon_names[0]), check.read(icon_names[1]), icon_names[0]).frames[berserker_taunt.ICON_STRIP]
    assert (bicon.w, bicon.h) == (awakenings.icon_cells_after()[0] * berserker_taunt.ICON, berserker_taunt.ICON), "the berserker's icon strip (13 cells since 격분)"
    return buf.getvalue(), armour, added, added_records, drop_log, sorted(costume_added)


def main() -> int:
    # A cp949 console cannot encode every character the plan prints (「—」 in the 맞춤 line);
    # the asset is written before the plan is printed, so a crash there reads as a failed run.
    sys.stdout.reconfigure(errors="replace")
    # `--plan`: also print every drop-table edit (the #20/#21 plan), one line each.
    show_plan = "--plan" in sys.argv[1:]
    argv = [a for a in sys.argv if a not in ("--plan", "--skill-gear", "--no-skill-gear")]  # read at import (SKILL_GEAR); --skill-gear is the default now
    if len(argv) < 2:
        print(__doc__)
        return 2
    src = argv[1]
    out = (
        argv[2]
        if len(argv) > 2
        else os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "android_app", "app", "src", "main", "assets", "hero2_ktf.zip",
        )
    )

    zin = zipfile.ZipFile(src)
    names = zin.namelist()
    jar = [n for n in names if n.endswith(JAR_SUFFIX) and "/" not in n]
    if "__adf__" not in names or not jar:
        print(f"원본 모양이 아니다: __adf__={'__adf__' in names} jar={jar}")
        return 1

    missing_private = [n for n in DUMP_PRIVATE if n not in names]
    if missing_private:
        print(f"원본에 {missing_private} 가 없다 — KTF 1.0.7 덤프가 아닌가?")
        return 1
    aid = next(
        (line.split(":", 1)[1].strip() for line in zin.read("__adf__").decode("euc-kr", "replace").splitlines() if line.startswith("AID:")),
        None,
    )
    if not aid:
        print("__adf__ 에 AID 가 없다")
        return 1

    # 드랍의 「출처」 = 어느 맵이 배치하는 적 (원본 덤프의 맵 — MAP_EVENT_FIXES 는 몬스터 구간 뒤의 이벤트만 고친다).
    placed = map_monsters.placed(map_monsters.load(zin, ENM_RECORDS))

    os.makedirs(os.path.dirname(out), exist_ok=True)
    maps_fixed = 0
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zout:
        for n in names:
            if n in DUMP_PRIVATE:
                continue  # the dump owner's saves / options — never shipped (module doc)
            if n in KEEP_TOP or n in jar or n.startswith("P/"):
                data = zin.read(n)
                if n in jar:
                    data, armour, added, added_records, drop_log, costume_names = patch_jar(data, placed)
                elif n in MAP_EVENT_FIXES:
                    data = patch_map(n, data)
                    maps_fixed += 1
                zout.writestr(n, data)
        # ★ 2026-10-16 신전 보스 레이드: 새 맵 — 112 (귀환문 변형 신전 = 15 사본; 호스트가 가디언 HARD 클리어 캐릭터에게 `/m/m/15.m.gz`
        #   대신 내준다, wie_android::raid_gate) · ★ 2026-10-17 단계 맵 113..121 (스토리 보스방 아홉의 사본; 옛 111 은 없다). 원본은 그대로.
        for n, data in sorted(raid_gate.map_members(zin).items()):
            if n in names:
                raise SystemExit(f"{n}: the dump already has it")
            out_info = zipfile.ZipInfo(n, date_time=COSTUME_DATE)
            out_info.compress_type = zipfile.ZIP_DEFLATED
            out_info.external_attr = 0o100644 << 16
            zout.writestr(out_info, data)
    if maps_fixed != len(MAP_EVENT_FIXES):
        raise SystemExit(f"map events: fixed {maps_fixed} map(s), expected {len(MAP_EVENT_FIXES)}")
    with zipfile.ZipFile(out) as check:
        leaked = [n for n in DUMP_PRIVATE if n in check.namelist()]
    assert not leaked, f"{out}: still carries {leaked}"

    # The probe's copy: <TEST_SAVES>/<AID>/fs/<name>, the layout `--fs` reads (a device pull).
    fs_dir = os.path.join(TEST_SAVES, aid, "fs")
    os.makedirs(fs_dir, exist_ok=True)
    for n in DUMP_PRIVATE:
        with open(os.path.join(fs_dir, n[len("P/") :]), "wb") as f:
            f.write(zin.read(n))

    size = os.path.getsize(out)
    kept = sum(1 for n in names if (n in KEEP_TOP or n in jar or n.startswith("P/")) and n not in DUMP_PRIVATE)
    print(
        f"wrote {out} ({size:,} B, {kept} members; jar={jar[0]}; "
        f"{GM_TDF}: rows {list(GM_REPLACE)} rewritten, {len(GM_APPEND)} appended; "
        f"{ITMTP_TDF}: {len(ITMTP_APPEND)} appended; "
        f"armour: {armour} byte(s) for {len(ARMOUR_FIXES)} item(s); "
        f"shop: {added} record(s) added, {len(PRICE_FIXES)} price(s) raised; "
        f"new items: {added_records}; "
        f"{ENM}: {len(drop_log)} drop edit(s), gear slot widths by tier (NORMAL {NORMAL_TIER_WIDTH} · HARD {HARD_TIER_WIDTH}), "
        f"HELL slots on {sum(1 for line in drop_log if line.startswith('HELL'))} enemies (상급 by grade {HELL_GRADE_WIDTH} · 종결 장신구 {HELL_ACCESSORY_WIDTH} · 종결 장비 {HELL_FINAL_WIDTH} · 신규 6종 {HELL_NEW_SIX_WIDTH} behind the 난이도 gate, NORMAL+); "
        f"skill text: {', '.join(f'{n} {list(r)}' for n, (_s, r, _a) in SKILL_TEXT.items())} rewritten, {SK0_TDF} +{len(SK0_APPEND)} · {SK2_TDF} +{len(SK2_APPEND)} appended; "
        f"boss doors: {maps_fixed} map(s); "
        f"{'2차 스킬 장비 ' + str(len(SKILL_GEAR_ADDS)) + ' · 조합서 ' + str(len(SKILL_RECIPE_ADDS)) + ' · 가디언 칸 16 · 17; ' if SKILL_GEAR else ''}"
        f"costume sheets: {', '.join(n[:-4] for n in costume_names if n.endswith('.mph')) or 'none'}"
        f"{' + ' + costume_sheets.CATALOG_MEMBER if costume_sheets.CATALOG_MEMBER in costume_names else ''})"
    )
    print(f"dump saves NOT shipped: {', '.join(n[2:] for n in DUMP_PRIVATE)} -> probe copy in {fs_dir} (use --fs {TEST_SAVES})")
    if show_plan:
        for line in drop_log:
            print("  " + line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
