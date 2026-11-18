# hero2 퀵슬롯 7칸 · 「퀵슬롯등록」 드랍박스 · 터치 전수 전환 — 계획서 (2026-09-17)

> 사용자 요청: *"hero3 분석해서 퀵슬롯과 터치모드전수전환 작업을 hero2에도 진행할거야. hero3처럼
> 스킬퀵슬롯이 7개있고 스킬을 선택했을때 1,3,7,9 번호등록 드랍박스가 아닌 퀵슬롯등록 드랍박스가
> 나와야해. 전수 조사 후 hero2에 어떻게 적용할지 plan가지고와."*
>
> 이 문서는 **조사 결과 + 설계 + 실행 순서**다 (계획 시점에는 코드를 바꾸지 않았고, 사용자 승인 뒤
> 같은 날 S0~S6 을 실행했다 — 바로 아래 「실행 기록」). 예시 이미지는 `docs/quickslot_touch_plan/`
> — 생성기 `mock_hud.py`·증거 스크립트 `hero2_chain.py`/`rd_static_array.py` 포함.

---

## ✅ 실행 기록 (2026-09-17, 같은 날) — S0~S6 완료, ★ 기기 확인 통과 (같은 날 저녁, S25)

★★ **기기 확인 (SM-S938N, Android 16, 릴리스 서명 업데이트 — 세이브 유지):** ① 7칸 + hero2
아이콘(빈 칸은 희미한 링) ② 스킬 → `퀵슬롯등록` → 토스트 `등록할퀵슬롯선택` → 플레이트 5 탭 →
등록(`popup: binding plate 4 through guest slot 0`, 스킬창 배지 `1`) ③ `퀵슬롯해제`
(`quickbar byte … cleared`, 배지·플레이트 소멸) ④ 재시작 뒤 플레이트 5 탭 = 재좌석
(`plate 4 (value 3) seated in guest slot 0: 2 write(s), ok=true`), 밀려난 일격 탭 = 슬롯 1 에
재좌석 ⑤ `0` 롱프레스 → 토스트 `회복아이템교체` ⑥ 필드 메뉴·SKILL·탭 헤더·팝업 전부 탭
(`menu tap: … -> focus / OK / tab RIGHT`) ⑦ 패닉 0, 필드 15~16 fps · tick 2.8~4.1 ms.
**기기에서 잡힌 것 셋** (전부 같은 날 수정): SAVE·MAP 이 필드에 그려짐(`placement::shown` 우회
경로) · NPC 대화 상자가 26행 잘라내기에 반으로 잘림(상태 4 = 대화, 스크롤로 해결) ·
hero3 오프셋 진단 `autoloot flag unlocated … @+0x2d7` 가 5 초마다 로그 — 요약은
`HERO2_NEXT_SESSION.md` ▶ 블록.

| 단계 | 결과 | 검증 |
|---|---|---|
| **S0** `#` 겹침 | `CtrlId::HealSwap` 삭제. `0` 버튼 = **탭(뗄 때) `0` · 롱프레스 450 ms `#`** (`app::HEAL_HOLD_MS`, 토스트 `회복아이템교체`). `#` 는 풀아웃 키패드·패드 `R2` 에 남는다 | `overlay.rs` `ring_layout_tests::no_field_control_touches_a_quickbar_plate` (원 기준, 창 8종 — Linux `cargo test`) + 파이썬 재계산 12개 창 겹침 0 |
| **S1** 퀵바 읽기 | `ag.j`/`ag.a`/클래스/`bn.j` → `IconRef::Skill(class, v-1)` · `IconRef::Beast(id)` · 게스트 쿨타임 `(left, total)` | probe `[quickbar]` 게이지: `[1,10,5,2]` · 등록 직후 `cd=[..,35,..]` = `ay.a[0]` |
| **S1** 아틀라스 | `tools/gen_h2_atlas.py` → `icons.bin` 19,084 B (bico 13 + gmico 7/7/6), `atlas.rs` `include_bytes!` 복귀 | 미리보기 PNG |
| **S2** 팝업 재작성·커밋 | **`wie_android/src/hero2java.rs` 신설**(의존성 0, `guest_build`·`wie_probe` 양쪽 `include!`): 체인 워크 `bw.a → s\|af → d`, 라벨 검사, 재작성/복원 write 생산자, **repaint 플래그 `j.e`**(없으면 화면이 안 바뀐다 — 실측) | probe `--h2-relabel` / `--h2-commit`: 게임이 **자기 글꼴로 4행 팝업**을 그리고, 커밋 후 `ag.j=[-1,10,1,2]`·`cd=[-1,-1,35,-1]`(중복제거·쿨타임까지 게임 코드) |
| **S2** 쓰기 게이트 | `JavaWrite {WidgetCursor, PopupLabel, PopupRows, Quickbar, Cooldown}` + `java_write_ok` (Hero2Ktf 만) + 워커 단일 관문 `emulator::java_write` (`java write:` 로그) · probe `--poke` hero2 개방 | `anchors_selftest` 잠금 갱신 (`popup_commit == SyntheticOk`, 새 표 핀) |
| **S3** 창 모델 | `quickpage.rs` 전면 재작성: 정체성 = 바이트 값, `Window`(LRU·미확정 좌석), `capture`(무쓰기 재좌석), `reserve_for_bind`, `activate`, `flip_to`, 주인 `(세이브슬롯, 클래스)` + `arc` 주차 | `quickpage_selftest` 11 · 세이브 슬롯 byte `ag.c:B` 실측 (SLOT 2 로드 → 1) |
| **S4** 라우터 1 | `h2_tap`: 필드 메뉴 8행 · 팝업 행(kind 3/6/7/8) · 스킬창 칸/탭 헤더 · 1탭 포커스(+repaint)/2탭 OK · 패널 밖 None · 소유 창 Swallow. `InputEvent::MenuTap{fx,fy}`(fb 좌표) | probe `--h2-tap` 시퀀스: 메뉴→SKILL→탭 RIGHT→칸→팝업→재작성→행→OK, 프레임 확인 |
| **S5** 라우터 2 | 인벤 7×4 · STATUS 3×2(포인트 있을 때만) · 장비 7칸 · 야수 2열 · 야수 목록 · SYSTEM(중앙 리스트) · HELP 페이저 · 예/아니오(kind 2) · 행 없는 창(상세·메시지) = OK | probe: 인벤 칸→팝업→상세, SYSTEM→도움말→HELP |
| **S6** 패드 | 배지가 **좌석**을 따라간다(`QuickSlot.guest`), picking 중 숫자키 = 그 슬롯에 앉은 플레이트에 등록(`PopupBindGuest`) | 컴파일 |
| **S7** 포션 플레이트 (저녁) | `Entry::Item(type)` — 좌석 없음, 발동 = `bq.c`+`bq.a` 쓰기(`JavaWrite::HealSelect`, `#` 가 남기는 두 바이트) + `0`. 인벤 kind-6 팝업에 **행 2개 추가**(3→5, 버킷 여유에 `gm[34]/[35]` 핸들 = `JavaWrite::PopupLabelRef`) → `퀵슬롯등록`(플레이트 선택) / `퀵슬롯해제`; 커밋은 호스트 전용(복원 → CLR). 아이콘 = `img/glb#17` 34칸 시트(`IconRef::ItemType`), 스택 수 = 가방 `(type,0)` 합계. 회복템 없으면 토스트 `회복아이템없음` | probe `--h2-relabel`(5행 팝업을 게임이 그림) · `--h2-tap`(행 4 포커스→OK, 팝업 닫힘) · `--h2-heal 1250:26`(포도주 87→86, HP 777→780) · **S9 실기**: 하이포션 등록→플레이트 아이콘+99→발동 141→140→139 · 포도주 등록(2 write)→86 · 해제 → 파일 `i25` · 패닉/리포트 0 · 셀프테스트 quickpage 12 · hero2java 17 · anchors 34 · probe 28 |
| **S7-b** 치료제·귀환서 플레이트 (2026-09-18 오후, ⑧) | `0` 밖의 27/28 은 **드라이브**가 게임의 인벤 팝업 `사용하기` 를 대신 누른다 — `hero2java::h2_use_step`(순수) + `emulator.rs::step_use_drive`(페인트 홀드·키다운 드랍·HUD 폴 정지). 폴마다 체인(`ptr_class`)을 다시 걷고 모양이 맞을 때만 키 하나. 귀환서는 게임의 kind-11 `이동하시겠습니까?` 에서 **핸드오프**(라우터가 kind 10/11 도 kind 2 기하로 탭). `bindable` 에 27/28, 팝업 워커가 kind 7 도 받는다. 정본 = `HERO2_GUEST_RE.md` §2.6 | probe `--h2-use 640:28`: 646 핸드오프(스크린샷) · 아니오 → 707 DONE(12 그대로) · 예 → 683 DONE + 워프(12→11) · `--h2-use 640:26`(kind 6 대역) 87→86 · `--h2-relabel` 로 kind 7 팝업 5행. hero2java 20 · 디버그 APK 빌드 OK · **기기 미확인** |

**하지 않은 것**: ~~HIDE/SHOW 편집기~~(✅ 같은 날 밤, S9 — `HERO2_NEXT_SESSION.md` ▶ 블록 ③) · ~~QUEST/CARD 창~~(✅ 같은 날 저녁) · ~~셸(타이틀/슬롯) 터치~~(✅ 같은 날 저녁) · ~~포션 플레이트(S7)~~(✅ 같은 날 저녁, S9) · ~~91차 STICK~~(✅ 같은 날 밤, S9 — M6 통째로: 79차-B 숫자 블록 제거 + 91차/91차-B STICK, `HERO2_NEXT_SESSION.md` ▶ 블록 ⑤) · ~~기기 확인~~(✅ 같은 날 저녁, S25).

★ **셸 터치 (2026-09-17 저녁)**: `h2_window_chain` 의 뿌리가 둘 — 상태 5 는 `bw.a`, 상태 1/2 는
`t.a`(타이틀 싱글턴). 타이틀·슬롯 화면은 **화살표 상자 하나**(안 = OK, 양옆 = LEFT/RIGHT — 상자가
곧 커서라 쓰기 없음), OPTION 은 행 밴드(포커스 행 = RIGHT 로 값 순환, 다른 행 = 커서 쓰기),
HELP 는 `ax.j` 중심으로 다시 잡았다(셸에는 패널이 없다). probe `--h2-tap` 으로 타이틀→슬롯→로드,
OPTION, HELP, 새로하기→확인 팝업까지, S25 에서 타이틀·슬롯을 확인. 셀프테스트
`a_tap_on_the_shell_is_routed_from_the_title_singleton`.

⚠ **새로 배운 함정** (둘 다 코드 주석에 박았다): ① **게스트 힙 주소는 런마다 다르다** — 같은 키 스크립트로 `bw.a` 가 `0x484348b0 / 0x4843c208 / 0x48449618`. 미리 계산한 `--poke` 는 남의 객체를 친다 → 그래서 `hero2java.rs` 를 probe 에도 `include!` 하고 `--h2-relabel/--h2-commit/--h2-tap` 이 프레임에서 걷는다. ② **창은 `j.e`(+0x20) 가 설 때만 다시 그린다** — 메모리를 바꿔도 플래그를 안 세우면 화면은 열 때 프레임 그대로(실측: rows=4 인데 6행이 보였다). 커서 쓰기·재작성 뒤 체인 전체(메뉴·창·팝업)에 1 을 쓴다.

---

## 0. 한 줄 결론

**hero3 의 두 축(79차 창 모델 · 66차 터치 라우터)은 hero2 에 그대로 이식 가능하고, 조건은 hero3 보다
좋다.** hero2 의 퀵바·팝업·창 커서가 전부 **이름 붙은 자바 필드**라 픽셀 스캔이 필요 없고, 게임의
스킬 팝업 라벨이 **길이 10 의 EUC-KR `byte[]`** 라 `1번에 등록`(10 B) 을 `퀵슬롯등록`(10 B) 으로
제자리에서 덮어쓸 수 있다. 게스트 쓰기는 **바이트 몇 개**(커서 · 라벨 · 배열 길이 · 퀵바 4 B)뿐이고
등록 자체는 게임 자신의 `ag.a(slot, idx, beast)` 가 한다 — hero3 와 같은 「커서 쓰고 OK 합성」 패턴.

★★ **이번 세션이 확정한 것** (HERO2_GUEST_RE.md §6 #1 「퀵바 배열」이 닫혔다):

| | 값 | 근거 |
|---|---|---|
| **퀵바** | `ag.j:[B` @ **`0x193bac`**, **길이 4** | SKT 역컴파일 `u.j = new byte[4]` + probe f=1200 실측 `[1, 10, 5, 2]` = 게임 HUD 4칸(일격·피의계약·단검난무·민첩태세)과 일치, 스킬창 배지 `1/9/7` 일치 |
| 값 의미 | `-1` 빈칸 · `1..7` = ACTIVE 탭의 n번째 스킬 · `10+k` = 야수의 k번째 스킬 | `m.a(3,row)` → `u.a.a(slot, h+1, false)` / `bm` → `u.a.a(slot, h, true)` (`+10`) |
| 쿨타임 | `ag.a:[I` @`0x193bbc` (int[4], `-1`=준비, 매 프레임 `ag.i()` 가 감소) · 최대값 = 클래스 static `ay.a:[S` @`0x193d6c` = `[35,200,70,180,80,80]` | SKT `u.a[4]`, `ac.a[]` |
| 등록 함수 | `ag.a(IIZ)V` @`0x111f44` — 같은 값이 든 칸을 `-1` 로 지우고(중복 금지) `j[slot]=v`, 쿨타임 세팅 | SKT `u.a(int,int,boolean)` |
| **팝업(드랍박스)** | 클래스 **`d`** (SKT `ba`): `+0x28 kind:B`(3=스킬) `+0x2c labels:[[B` `+0x30 height:I` + 상속 `+0x8 h`(커서) `+0xc i`(행수) | probe f=1820 체인 `bw.a(0x19422c) → .b(+4) = s(스킬창) → .b(+4) = d`, labels len 6, height 102 = 6+16·6 |
| 라벨 | `[0] 포인트투자(10B) [1] 스킬상세정보(12B) [2] 1번에 등록(10B) [3] 3번에 [4] 7번에 [5] 9번에` — tdf 표 `bw.a:Li;` 의 **원소 그 자체**(공유) | `31 b9f8 bfa1 20 b5ee b7cf` 실측 |
| 팝업 기하 | 상자 **74 × (6+16n)**, 중앙 `(ax.j - 37, ax.k - h/2)`, 행 `y+6+16i`, 텍스트 가운데 정렬, 선택행 **흰색** 나머지 **회색** | 176×220: (51,59) / 452×245: (189,71) 실측 = 식 그대로 |
| 화면 기하 | `ax.g`=폭 `ax.i`=**전체 fb 높이** `ax.j`=cx `ax.k`=cy @`0x194b4c/5c/6c/7c` | 176×220 → 176/220/88/110, 452×245 → 452/245/226/122 |
| 스킬창 | 클래스 **`s`**: `+0x2c b:Z` 탭(true=PASSIVE, false=ACTIVE) · `h` 커서 · `i` 행수(6) · `+0x30 d` 열 분할(4) · 칸 = `(x0+5+col·57+15, y0+12+row·19+10, 38, 13)` | SKT `m.java` 페인터 + 실측 프레임 일치 |
| 야수 스킬창 | 클래스 **`af`** — 같은 팝업, 등록값 `10+h` | SKT `bm.java` |
| 아이콘 | 캐릭터 스킬 = `img/gmico` 프레임 `ag.a:B`(클래스 0..2, @`0x193a6c`) 의 12×12 스트립 셀 `v-1` · 야수 스킬 = `img/bico[bn.j[k]]` | 스킬창·HUD 픽셀 일치 (`gmico_2` = 일격·민첩태세·급소가격·맹독감염·단검난무·번개난타) |

⛔ **그리고 현재 코드에 결함 하나가 발견됐다** (§4.7): 8좌석 원호(2026-09-10)의 마지막 두 좌석이
`#` 버튼(`disc_row_seat(1)`, 2026-09-01) **위에 앉는다** — 플레이트 6 은 96 px, 7 은 18 px 겹침,
S9·S25·16:9 전부. 기기 확인 전에 잡혔으니 S0 에서 먼저 고친다.

---

## 1. 조사 범위와 방법

| 원천 | 무엇을 봤나 |
|---|---|
| hero3 = `d:/tmp/wie_app` (HEAD `10c2bc5`, 시드 `262dd63` 이후 **145 커밋**) | `CLAUDE.md` 4,179줄 · `PLAY_FEEDBACK_FIXES.md` 66차~101차 · `TOUCH_MENU_FEASIBILITY.md` · `QUICKSLOT_EXPAND_FEASIBILITY.md` · 코드 `menutouch.rs`(2,528) `quickpage.rs`(2,914) `items.rs`(popup·menu 절) `emulator.rs`(popup/menu_tap arm) `app.rs`(터치 체인) |
| hero2 저장소 | `HERO2_NEXT_SESSION.md` `HERO2_GUEST_RE.md` `CLAUDE.md` · `lib.rs` `items.rs` `guest_build.rs` `guest_anchors.rs` `overlay.rs` `app.rs` `quickpage.rs`(hero3 시드 버전 833줄) |
| **SKT 역컴파일 소스** `C:\Users\<user>\OneDrive\바탕 화면\영서2 사운드 (50%).jar.src` (97 클래스) | `g`(창 기반) `ba`(팝업) `m`(스킬창) `bm`(야수스킬창) `ck`(필드메뉴) `u`(전역) `b`(캐릭터) `bd`(캔버스 키핸들러) `bf`(인벤) `e`(tdf) — **게임 로직은 KTF 와 같은 소스**라 페인터 상수·등록 경로를 코드로 읽었다 |
| probe 실측 (재빌드한 `wie_probe`, hero2 KTF 1.0.7) | ① 176×220 f=1200 필드 + statics/힙 덤프 ② 176×220 필드→메뉴→SKILL→ACTIVE 탭→팝업 (f=1330/1480/1690/1730/1770/1820 + 덤프) ③ **452×245**(S9 가로 논리) 같은 시퀀스 — 팝업·패널 원점이 식대로 움직이는지 |
| 도구 | `tools_re/dump_hero2_java.py`(클래스·필드·메서드 표) · `hero2_heap.py` · `dump_mpd.py`(아이콘) · `dump_tdf.py` · 새 스크래치 `chain.py`(창 체인 워커) `rd_arr.py`(정적 배열) |

probe 키 스크립트 (재현용):
```
--w 176 --h 220 --ms 20 --frames 1850
--keys 320:OK,420:OK,520:OK,1300:CLR,1340:DOWN,1380:DOWN,1420:DOWN,1460:OK,1500:DOWN,1540:DOWN,1580:DOWN,1620:DOWN,1660:DOWN,1700:RIGHT,1740:UP,1780:OK
--dump 1330,1480,1690,1730,1770,1820 --dump-mem 0x100000:0x96434,0x48000000:0x8000000
```
(1300 CLR=메뉴 · DOWN×3=SKILL · OK=스킬창(PASSIVE) · DOWN×5=탭 헤더로 · RIGHT=ACTIVE · UP=목록 ·
OK=팝업. `docs/quickslot_touch_plan/00_probe_sequence_176x220.png` 가 그 6장이다.)

⚠ 이 세션에 밟은 함정: `dump_tdf.py <out.txt>` 의 첫 인수는 **출력 파일**이다. 에셋 경로를 첫
인수로 주면 `io.open(out,"w")` 가 **에셋 zip 을 0바이트로 만든다** — 실제로 그랬고
`make_hero2_asset.py "dist/영웅서기2-1.zip"` 으로 재생성해 md5 `08a53cc4…`·148 멤버·게이지 실측까지
동일함을 확인했다. `dump_mpd.py <out_dir>` 도 같은 모양이다.

---

## 2. hero3 에서 가져올 것 — 메커니즘 8개

| # | hero3 (회차) | 핵심 | hero2 에서의 형태 |
|---|---|---|---|
| M1 | **터치 = 커서 쓰기 + OK 합성** (66차) | 게스트는 좌표를 못 본다. 탭 → 게임 자신의 커서 바이트 쓰기(키 0회) → 같은 칸 2탭 = OK 하나. `items::MenuTapAction::{Focus,Activate}` | 동일. 커서 = 모든 창의 공용 기반 `j.h`(+0x8), 범위 = `j.i`(+0xc). **픽셀 감지기 대부분 불필요** — 원점은 `bw.a/b`, 화면 중심은 `ax.j/k` 가 준다 |
| M2 | **순수 기하 모듈** `menutouch.rs` (의존성 0, `include!` 셀프테스트 39건) | `Grid{x0,y0,w,h,pitch,cols,rows,pad}::hit` — 빈틈은 가까운 칸이 갖고 격자 밖은 `None`, 절대 클램프하지 않고 **거절** | `wie_android/src/hero2touch.rs` 신설, 같은 `Grid` + 창별 표(메뉴 8행 · 스킬 4+2 · 인벤 7×4 · 팝업 n행 · 확인창 2버튼) |
| M3 | **팝업 재작성** `items::popup_view/relabel/commit` (2026-08-06~) | 게임 라벨 버퍼에 `퀵슬롯등록` 을 **매 프레임 idempotent** 로 쓰고 행수를 줄인다; 커밋 = 행수 복원 + 선택 바이트 + 합성 OK (`PopupCommit::SyntheticOk`) | 동일 패턴. 라벨은 `byte[]`(길이 필드 있음, NUL 없음) — 10 B 정확히 맞는다. 행수 = `d.i` + **배열 길이 필드** + `d.a:I`(높이) 셋을 같이 줄인다 (§4.2) |
| M4 | **창(window) 모델** `quickpage.rs` (79차) | `PLATES=7`, `GUEST_SLOTS=5`, `Window`(플레이트→게스트 슬롯 + LRU), `reserve`/`activate`, `PENDING_POLLS`(미확정 좌석), `own/arc`(캐릭터별 행 주차), HIDE/SHOW 마스크(82차) | `PLATES=7`, **`GUEST_SLOTS=4`**. 정체성 = **바이트 값 하나**(hero3 의 `(cat,cell)`+클래스 스탬프가 필요 없다 — 클래스는 세이브 안에서 불변, 야수 스킬은 게임이 `ag.h()` 로 스스로 무효화) |
| M5 | **드랍박스 행 탭** (79차-C) | 상자를 프레임에서 73px 검은 런으로 찾는다(`popup_box`), 1탭 = 선택 바이트, 2탭 = `PopupConfirm`, 크롬 탭은 삼킴 | 상자 위치는 **계산**(`ax.j-37`, `ax.k-h/2`) — 스캔 없음. 나머지 동일 |
| M6 | **숫자 키패드 제거** (79차-B) · **CLR 좌석** (80차-H `cancel_seat`) · **STICK** (91차) | 상설 `1 3 7 9 #` 블록 없음, 풀아웃 키패드가 12키 전부 보유 | `cancel_seat` 는 09-10 에 이미 이식. `0`/`#` 행이 남아 있고 그것이 §4.7 의 겹침 |
| M7 | **패드** (`PadAction::Quick(n)`, 81차 `menu_digit`) | 패드 버튼은 게스트 키가 아니라 **플레이트**를 가리킨다; 창이 열리면 예전 숫자로 | 동일. `gamepad.rs` 기본표 `L1=7 R1=9 L2=0 R2=#` 는 이미 hero2 값 |
| M8 | **셸 터치** (80차-C/D/F/K/L/N) | 타이틀 카루셀·슬롯 목록·OPTION·HELP·확인창을 픽셀 감지기로 | hero2 셸도 `j` 자손(HERO2_GUEST_RE §5.2: `y`·`aw`·`bz`·`t`·`v`·`ck` 싱글턴) — 커서/행수는 메모리로, 기하만 SKT 페인터에서 읽어 실측 확인. **후순위**(§5 S5) |

hero3 가 픽셀에 의존해야 했던 이유 셋이 hero2 에는 없다: ① 팝업 x/y 가 스택에만 있었다 → hero2 는
`ax.j/k` 정적 + `d.a:I` ② 「가방인가 목록인가」를 메모리로 못 갈랐다 → hero2 는 자식 창의
**`ptr_class`** 가 곧 화면 종류 ③ 패널 원점을 해상도별 표로 들었다 → hero2 는 `bw.a/b` 를 읽는다.

---

## 3. hero2 조건 — 확정값과 대응표

### 3.1 SKT ↔ KTF 클래스 대응 (이번 세션 확정, 필드 모양·정적 값으로 대조)

| 역할 | SKT (역컴파일) | **KTF (client.bin1120)** | 확인 근거 |
|---|---|---|---|
| 창 기반 (커서 `h`, 행수 `i`, 부모 `a`, 자식 `b`) | `g` | **`j`** (fields_size 0x28) | 필드 12개 동일 배열 |
| 팝업/드랍박스 | `ba` | **`d`** (`B a`, `[[B b`, `I a`) | f=1820 grandchild 의 `ptr_class` = `d` |
| 필드 메뉴 | `ck` | **`bw`** (`bw.a:I`=패널 x, `bw.b:I`=패널 y, `bw.a:Lbw;` 싱글턴 `0x19422c`, `bw.a:Z` 스킬포인트 有) | HERO2_GUEST_RE §5 + 452×245 에서 (166,64) 실측 |
| 스킬창 | `m` | **`s`** (`a:Z` +0x28 헤더 포커스, `b:Z` +0x2c 탭, `d:B` +0x30 열 분할, `a:La;` +0x34) · statics `s.a:B`(행수) `s.b:B` `s.c:B` | f=1820 child = `s`, `s.b=0`(ACTIVE), `s.i=6`, `s.d=4` |
| 야수 스킬창 | `bm` | **`af`** (`a:Lbn;`, `a:B`, `i:[B`) | 필드 모양 |
| 전역 (퀵바·상태워드·캐릭터) | `u` | **`ag`** | `ag.a:I`=상태 `ag.a:La;`=캐릭터 `ag.j:[B`=퀵바 `ag.a:[I`=쿨타임 `ag.a:B`=클래스(=2) `ag.f:I`=돈(=276037) |
| 캐릭터 추상/클래스 3종 | `b` / `ac`·`an`·`ce` | **`a`** / **`ay`·`bl`·`bx`** (각 static `[S a` = 스킬 쿨타임표) | `ay.a` = `[35,200,70,180,80,80]`, 나머지 둘은 미초기화(null) |
| 야수 | `cg` | **`bn`** (`bn.j:[B` = 야수 스킬 슬롯→전역 야수스킬 id, `[2,3,255,255]`) | HERO2_GUEST_RE §2.2 + `bico[2]` 가 HUD 3번 칸과 일치 |
| tdf 표 | `e` | **`i`** (`[Object a`, `S a`) | `bw.a:Li;` = `/t/gm` |
| 캔버스 기반 (화면 기하) | `bw` | **`ax`** (`g`=폭 `i`=높이 `j`=cx `k`=cy @`0x194b4c/5c/6c/7c`) | 두 해상도에서 값 확인 |
| 인벤창 / 인벤 데이터 | `bf` / `ci` | 미확정 / **`bq`** | S5 에서 |

### 3.2 퀵바와 등록 경로 (SKT `u.java`·`m.java`·`bd.java` 를 KTF 이름으로)

```
필드 키:  bd.a(int,int)  '1'→a(0) '3'→a(1) '7'→a(2) '9'→a(3)   '0'→u.a.r() 회복  '#'→u.a.b() 교체  '*'→맵
          a(slot): v=ag.j[slot]; v<0 → 무시; ag.a[slot]>0(쿨) → 무시; v>=10 → 야수스킬 (v%10); else ag.c(v) 시전
등록:     s(스킬창).a(3,row) → ag.a.a(row-2(투자행 있을 때)/row-1, s.h+1, false)
          af(야수창).a(3,row) → ag.a.a(row-2/row-1, af.h, true)   (값 = h+10)
          ag.a(IIZ): 같은 값 든 칸 -1 로 → j[slot]=v → 정적 a(idx,beast): a[slot] = 쿨타임 최대값
세이브:   ag.b():[B 가 p[32] q[32] r[3] 뒤에 j[4] 를 그대로 쓴다 → 게임이 4칸을 스스로 지속
새 게임:  x(): j[0]=1
```
→ hero3 와 결정적으로 다른 점: **아이템은 퀵바에 없다.** `0`/`#` 는 캐릭터의 `t:B`(선택 회복템)로
돈다. 「포션도 아무 슬롯에나」는 여전히 호스트 신규 기능(§5 S7)이고 이 계획의 본체(스킬)와 독립이다.

### 3.3 팝업 라벨은 tdf 표의 원소다 — 그래서 재작성이 싸고, 그래서 매 프레임이어야 한다

`d.b:[[B`(len 6) 의 원소 `[2]` 핸들 `0x484395f8` 은 **`bw.a:Li;`(gm 표) 의 `a[34]` 그 객체**다
(`e.a(int...)` 가 표 원소 참조를 그대로 배열에 넣는다). 즉 10바이트를 덮으면 그 세션 동안 표가
바뀐 채로 남는다 — 다음 팝업은 이미 `퀵슬롯등록` 이다. 단 필드 메뉴를 닫으면 `bw.a(boolean)` 가
`ck.a = null` 로 표를 버리고 다음에 `new i("/t/gm")` 으로 **다시 읽는다**. 그래서 hero3 처럼
「열려 있는 동안 매 프레임 idempotent 검사 → 다르면 쓰기」가 맞다(팝업 없을 때 비용 = 읽기 3회).

### 3.4 ★ 배열 길이를 줄일 때의 할당자 제약 (설계를 정한 사실)

KTF 자바 배열의 필드 블록 = `count·esz + 4(길이) + 4(헤더)` 이고 `Allocator::free` 는 **해제 시점의
길이 필드로 크기를 다시 계산**해 버킷 클래스(4/8/16/32/64/…)를 고른다. 원래 6행 `[[B` = 32 B →
32-버킷. 줄인 길이가 **같은 버킷**에 머물러야 팝업이 닫힐 때(GC) 올바로 해제된다:

| 원래 행수 | 줄인 행수 | 블록 | 버킷 | 판정 |
|---|---|---|---|---|
| 6 | 3 | 20 B | 32 | ✅ |
| 6 | 4 | 24 B | 32 | ✅ |
| 5 | 2 | 16 B | **16** | ❌ 다른 클래스 → `free` 가 "not a slot of this class, leaking" 로그 + 32 B 슬롯 누수/회 |
| 5 | 3 | 20 B | 32 | ✅ |

→ **줄인 행수는 3 이상**이어야 한다. 5행 팝업(투자 불가 스킬: `스킬상세정보/1/3/7/9`)을 `스킬상세정보/
퀵슬롯등록` 2행으로 만들 수 없다. 그래서 §4.2 는 **`퀵슬롯해제`** 행을 셋째 행으로 둔다(6행→4행,
5행→3행 — 둘 다 32-버킷). 대안 = vendored `bucket.rs::free` 를 주소로 클래스를 찾게 고치기(§7 ②).
hero3 의 커밋 직전 행수 복원(`popup_count_restore`)은 그대로 가져온다 — 정상 경로에서는 원래
길이로 해제된다.

---

## 4. 설계 — hero2 판

### 4.1 퀵슬롯 창 모델 (7 플레이트 ↔ 게스트 4칸)

```
Entry  = Empty | Skill(v: 1..7) | Beast(v: 10..)            ← ag.j 의 바이트 값이 곧 정체성
Window = plate -> guest slot(0..3) + LRU + 미확정(PENDING_POLLS) 좌석
Pages  = [ [Entry; 7] ; 2 ]  + hidden mask(82차) + owner
```
| 동작 | 게스트 쓰기 | 비고 |
|---|---|---|
| 플레이트 탭(필드), 상주 | **0** — 그 슬롯의 핫키 `1/3/7/9` 전송 (`InputEvent::QuickActivate{plate}`) | hero3 §6 규칙: UI 가 키를 직접 보내면 남의 슬롯을 누른다 |
| 플레이트 탭(필드), 비상주 | 빈 칸 우선·없으면 LRU 로 좌석 배정 → `ag.j[g]=v`(1 B) + 같은 값 든 다른 칸 `-1`(중복 금지, 게임 규칙) + `ag.a[g]=-1`(준비) → 핫키 | 게임이 스스로 만들 수 있는 상태만 남긴다(등록 직후 쿨이 끝난 상태). 세이브가 끼어들어도 정합 |
| 등록(팝업) | §4.2 — 게임이 쓴다 | |
| 해제 | `ag.j[g]=-1` + 저장소에서 제거 | `퀵슬롯해제` 행 또는 플레이트 롱프레스 |
| 캡처(폴링) | `ag.j` 4 B 읽기 → 좌석에 앉은 플레이트의 Entry 갱신, 미확정 좌석은 `PENDING_POLLS` 동안 대기 | hero3 79차-F 의 「낡은 읽기」 규칙 포함 |
| 페이지 플립 | 창 비우고 새 페이지 워밍(비상주 4개까지 raw write) | 14 바인딩 |
| 저장 | `wie_quickpages.txt`(기존 포맷 유지) — `own <slot> <identity>`; identity = **세이브 슬롯 + 클래스** | 세이브 슬롯 정적은 SKT `u.c:B`(`ca.java:36` 이 슬롯 커서를 씀) → KTF `ag.c:B` @`0x193a9c` 후보, S3 에서 실측 |
| 셸↔월드 엣지 | 같은 주인 → 저장소가 이긴다(플레이트 0.. 순으로 워밍) · 다른 주인/없음 → 게스트 4칸을 플레이트 0..3 으로 채택(=지금 화면과 동일) | 79차 §4 그대로 |

★ hero3 대비 **없어지는 것**: 클래스 스탬프 검증(`class_of`), `(tab<<5)|slot` 아이템 좌표, `skill_bind`
게스트 콜, 파티 전환 무효화 처리. hero2 는 값 하나가 정체성이고 게임이 야수 스킬을 스스로 지운다
(`ag.h()`: `>=10` 전부 `-1`) — 캡처가 그것을 그대로 따라가면 된다.

★ **비상주 활성화를 raw write 로 하는 이유**: 게임의 `ag.a(IIZ)` 는 AOT 자바 인스턴스 메서드라
`this`(`ag.a:Lag;` @`0x193c6c`) + JVM 컨텍스트가 필요하고 내부에서 가상호출(`u.a.a(idx-1)`)을 한다.
호출 가능하겠지만 검증 비용이 크고, 그 함수가 하는 일은 결국 위 2바이트 + 쿨타임 세팅이다.
hero3 도 아이템은 raw write 였다. (§7 ③ 사용자 결정)

### 4.2 드랍박스 재작성 — 「퀵슬롯등록 / 퀵슬롯해제」

| | 게임 원본 | 재작성 |
|---|---|---|
| 투자 가능 스킬 (6행) | 포인트투자 · 스킬상세정보 · 1번에 등록 · 3번에 · 7번에 · 9번에 | 포인트투자 · 스킬상세정보 · **퀵슬롯등록** · **퀵슬롯해제** (4행) |
| 투자 불가 스킬 (5행) | 스킬상세정보 · 1번에 · 3번에 · 7번에 · 9번에 | 스킬상세정보 · **퀵슬롯등록** · **퀵슬롯해제** (3행) |
| 야수 스킬창 (`af`) | 같은 두 모양 | 같게 |
| 1~2행 팝업 (미습득/패시브) | 포인트투자 (· 스킬상세정보) | **손대지 않는다** — 등록 행이 없다 |

쓰기 (팝업이 살아 있는 매 프레임, 이미 우리 것이면 0회):
1. `labels[2]`(6행) / `labels[1]`(5행) 의 10 B ← `퀵슬롯등록` = `c4fc bdbd b7d4 b5ee b7cf`
2. 그 다음 원소의 10 B ← `퀵슬롯해제` = `c4fc bdbd b7d4 c7d8 c1a6` (둘 다 정확히 10 B — 길이 필드 불변)
3. `[[B` 길이 필드 6→4 / 5→3 · `d.i`(+0xc) 같은 값 · `d.a:I`(+0x30) = `6+16·n`

게이트 (전부 통과해야 「우리 팝업」): 상태워드 `0x193aec == 5` ∧ `bw.a` non-null ∧ `bw.b`(child) 의
`ptr_class ∈ {s, af}` ∧ 그 `.b`(grandchild) 의 `ptr_class == d` ∧ `d.kind == 3` ∧ `d.i ∈ {6,5}`(원본) 또는
`{4,3}`(우리 것) ∧ **라벨 바이트 일치**(`1번에 등록` 또는 `퀵슬롯등록`) — hero3 `popup_view` 의 8관문과 같은 성격.
이 관문은 `s.b == false`(ACTIVE 탭) 도 함께 본다(PASSIVE 탭의 2행 팝업은 등록 행이 없다).

커밋 (플레이트 p 를 골랐을 때, 워커):
1. 관문 재검사(팝업이 아직 그 팝업인가, 커서가 같은 스킬인가) — 실패면 무동작
2. `Pages::reserve(p)` → 게스트 슬롯 g (빈 칸 우선 · LRU)
3. **복원**: 길이 필드·`d.i`·`d.a:I` 를 원래(6/5행)로 — 합성 OK 가 다음 틱에 처리되는 사이 게임의 커서
   클램프·페인터가 우리 행수를 보지 않게 (hero3 `popup_count_restore` 와 같은 틱 문제)
4. `d.h` ← `base + g` (`base` = 2(6행) / 1(5행)) — 「g번째 N번에 등록」 행
5. `Keydown(OK)` + `Keyup(OK)` → `d.a(int,int)` → `parent.a(3, h)` → `ag.a(g, idx, beast)` → 팝업 닫힘
6. 좌석 g 를 「미확정」으로 두고 캡처가 `ag.j[g]` 에서 값을 읽으면 확정

`퀵슬롯해제` 커밋: 이 스킬 값이 `ag.j` 에 있으면 그 칸 `-1`(raw) + 저장소의 그 플레이트 비움 + 팝업은
`CLR` 합성으로 닫는다(`d.a(3,-1)` = 취소 경로). 없으면 토스트 「등록되지 않은 스킬」.

⛔ 하지 않을 것: `d.h` 에 `d.i` 이상을 **줄인 상태로** 쓰기(페인터가 `this.b[this.h]` 를 그려
ArrayIndexOutOfBounds) — 그래서 3번 복원이 4번보다 먼저다. `LABEL_KEY1` 검사 없이 tag/count 만으로
팝업을 믿기(hero3 가 그 실수를 문서로 남겼다).

### 4.3 UI 흐름 (기존 이벤트 재사용 — `lib.rs` 의 `Popup/PickingSlot/PopupConfirm/PopupBindSlot/PopupCancelPick` 그대로)

1. 팝업 행 탭 → `hero2touch::popup_row(fb_w, fb_h, d.a, d.i, x, y)` → 다른 행이면 `PopupSelectRow{row}`
   (워커가 `d.h` 쓰기, 범위는 라이브 `d.i` 로 거절) · 같은 행이면 `PopupConfirm`
2. 워커 `PopupConfirm`: 재읽기 → 행이 `퀵슬롯등록` 이면 `picking=true` + `PickingSlot(true)` · `퀵슬롯해제`
   면 즉시 처리 · 나머지 행은 합성 OK 그대로(포인트투자/상세정보는 게임 것)
3. picking 동안: 플레이트 7개 금색 링(`draw_pick_highlight` 이미 있음), 페이지 버튼 허용, 플레이트 탭 →
   `PopupBindSlot{slot}` → §4.2 커밋 · CLR → `PopupCancelPick` · 팝업이 사라지면(게임 쪽 닫힘) 자동 해제
4. 등록 뒤 첫 캡처가 아이콘을 붙인다(§4.6)

### 4.4 터치 라우터 (터치 전수 전환)

원칙은 hero3 66차-B 그대로: **1탭 = 포커스(커서 바이트 하나) · 같은 칸 2탭 = OK · 패널을 소유한 창의
빗나간 탭은 삼킨다 · 범위 밖은 클램프가 아니라 거절 · 되돌릴 수 없는 탭은 교차검증**.

라우팅은 **창 체인**으로 한다 — 픽셀이 아니라 `ptr_class` 가 화면을 말한다:
```
root = bw.a(0x19422c)           // null 이면 필드 메뉴 계열이 아님 → 다른 루트(§5 S5)
node = root; while node.b != null: node = node.b     // 가장 안쪽 창(포커스 소유자)
match class_of(node):
  bw → MENU 8행 (y = bw.b+12+12i, x = bw.a+2..+118, 커서 bw.h, 행수 bw.i)
  s  → SKILL 칸 (§3.1 식, 커서 s.h, 분할 s.d, 탭 헤더 = LEFT/RIGHT 합성)
  d  → 팝업 행 (kind 3: n행 / kind 2: 예·아니오 2버튼 42×15 @ (ax.j-44, y)·(ax.j+2, y), 커서 h)
  af → 야수 스킬창 (칸 기하는 S5 실측)
  기타 → S5 에서 창별로 추가 (인벤 7×4: `bf` 생성자 `super(parent,28,4,7)` = 28칸·열 이동 7, 장비, STATUS, SYSTEM …)
```
게이트: 상태워드 == 5 · CHAR resolve · 체인의 모든 포인터가 자바 힙 범위 · 각 노드의 `ptr_class` 가
표에 있는 클래스 · 최심 노드가 `d` 면 **팝업만** 답한다(모달 — 뒤 창의 커서를 옮기면 안 된다).
관측 한 줄: `menu tap: (dx,dy) -> <class> row N (wrote 0x… no key) | REFUSED state=… chain=…` (66차-C 의
「한 줄로 답한다」 규칙).

패널 밖 탭: hero3 는 패널 소유 창에서 `Swallow`. hero2 도 같게 — CLR 디스크가 있으므로 「밖 = 닫기」로
만들지 않는다(대화 선택지·확인창에서 오탭이 취소가 된다).

⚠ **가로 이머시브에서 좌표계**: 창은 **전체 fb**(`ax.i` = 245) 기준으로 중앙 정렬되므로(실측: 패널 y
64 = (245-117)/2) 논리 219행 화면에서는 13 px 아래에 그려진다. 라우터는 **fb 좌표**로 계산하고
표시 좌표→fb 좌표 변환은 앱이 이미 하는 것(월드도 같은 fb)을 쓴다. 팝업 y 도 `ax.k` 기준.

### 4.5 쓰기 게이트 — 좁게, 표로

`guest_code_addresses_valid()` 는 그대로 **false**. 새 축 하나를 만든다:
```rust
/// hero2: 자바 데이터 필드에 대한 호스트 쓰기 — 코드 패치·게스트 콜이 아니다.
pub enum JavaWrite { WidgetCursor, PopupLabel, PopupRows, Quickbar, Cooldown }
fn java_writes_ok(w: JavaWrite) -> bool   // Hero2Ktf 만 true, 항목별
```
쓰기 함수는 전부 `items.rs`(또는 새 `hero2quick.rs`)의 **주소 유도 + 검증 + (addr, bytes)** 반환 순수
함수이고, 실제 `write_memory` 는 워커 한 곳. 각 쓰기 직전에 **읽고 비교**한다(read-verify-then-write —
`featurephone-port:guest-re` 스킬의 규율): 라벨은 원문 바이트, 커서는 `< d.i`, 퀵바는 `-1|1..7|10..`.

probe: `KNOWN_BUILDS` 의 hero2 항목에 `writes: JavaData` 를 더해 **`--poke` 를 hero2 에 연다**
(거부 사유였던 「hero3 주소를 hero2 에 쏜다」는 호출자가 hero2 주소를 주는 `--poke` 에는 해당하지
않는다 — `--host-save/--host-levelup/--find-fb…` 는 계속 거부). 이것이 §6 의 헤드리스 A/B 를 만든다.

### 4.6 HUD — 아이콘 · 쿨타임 · 배지

* **아틀라스**: `img/gmico` 프레임 0..2 (클래스별 12×12 ×7/7/6) + `img/bico` 0..12 → `tools/gen_h2_atlas.py`
  로 `icons.bin` 재생성(`atlas.rs` 의 7바이트 스텁을 되돌린다; 포맷은 hero3 `H3IC` 그대로, 인덱스만
  hero2). 색키 = 좌상단 픽셀(`dump_mpd.py --key` 규칙).
* `IconRef::Skill(sheet, cell)` 을 `Skill(class, v-1)` / `Beast(bn.j[k])` 로 재해석 — `scan_quickbar` 가
  `ag.j` 4 B + `ag.a:B` + `bn.j` 를 읽어 낸다. 비상주 플레이트는 저장소의 Entry 로 그린다.
* **쿨타임**: 게임 자신의 `ag.a[g]`(남은 틱) / `ay.a[v-1]`(최대, 클래스 static — `ag.a:La;` 객체의
  `ptr_class` 가 가리키는 클래스의 static) → 호스트 fill. hero3 의 `wie_cooldowns.txt`·SP 관찰이 필요
  없다. 비상주 플레이트는 쿨 표시 없음(게임에 없는 상태).
* 게임의 HUD 띠(안 보이는 26행)는 그대로 둔다 — 아무것도 마스킹하지 않는다(2026-09-10 정책).

### 4.7 ⛔ 선행 결함 — 플레이트 6·7 이 `#` 위에 앉는다

`overlay.rs` 의 현재 기하(`RING_SEATS=8`, `RING_A1=195°`, `disc_row_seat(1)` = `#`)를 파이썬으로 재계산
(`docs/quickslot_touch_plan/mock_hud.py`, 식은 `ring_slots` 를 그대로 옮겼다):

| 창 | 플레이트 변 | 플레이트 6 중심 | 플레이트 7 중심 | `#` 좌석 | 겹침 |
|---|---|---|---|---|---|
| S9 2220×1080 | 86 | (1611, 848) | (1624, 976) | (1518..1680, 792..954) | **6: 96 px · 7: 18 px** |
| S25 2340×1080 | 86 | (1731, 848) | (1744, 976) | (1638..1800, 792..954) | 같음 |
| 16:9 1920×1080 | 86 | (1311, 848) | (1324, 976) | (1218..1380, 792..954) | 같음 |

원인: `0`/`#` 행은 **6좌석 링**(플레이트 5개, 호 끝 161°) 시절인 2026-09-01 에 「액션 디스크 옆」에
앉혔고, 2026-09-10 의 8좌석 링은 호 끝이 195°(디스크 좌하)라 정확히 그 자리에서 끝난다. 터치 체인은
플레이트(`quick_hit`)가 `layout.buttons`(`#`)보다 **먼저**라 `#` 의 탭 영역 대부분을 플레이트 6 이
가져간다. `landscape_selftest` 의 `cancel_disc_never_touches_a_quickbar_plate` 는 CLR 만 본다.
호를 줄여도(RING_A1 182°) 플레이트가 79 px 로 줄면서 겹침은 남는다(계산됨).

**제안 A (권고)**: `#` 를 필드 HUD 에서 걷어낸다 — 풀아웃 키패드(12키 보유)에 남기고 **`0` 롱프레스 =
`#`**. 79차-B 가 hero3 에서 숫자 블록을 걷어낸 논리 그대로(도달 불가 상태 0). `0` 은 좌석 그대로 —
플레이트 6·7 과 `0` 의 원거리는 170·186 px > 반지름 합 124 라 겹치지 않는다. 컨트롤 하나가 줄어
`placement` 표·`FIELD_IDS`·저장 레이아웃 키 하나가 준다(미지 id 는 `CtrlId::parse → None` 으로 안전).
**제안 B**: `#` 유지 + 플레이트 크기 축소/호 재배치 — 어느 각도로 돌려도 같은 자리라 불가. 
**제안 C**: S7(포션 플레이트)이 `0`/`#` 를 통째로 대체 — 장기.
셀프테스트로 못 박는다: `no_field_control_touches_a_quickbar_plate`(창 매트릭스 × 컨트롤 전부, 원 기준).

---

## 5. 실행 순서 (세션 단위 · 각 단계 독립 검증)

| # | 작업 | 산출물 | 검증 | 위험/의존 |
|---|---|---|---|---|
| **S0** ½일 | §4.7 `#` 겹침 수정(제안 A) + 셀프테스트 | `overlay.rs` `placement.rs` `app.rs`(롱프레스) `landscape_selftest.rs` | 셀프테스트 + S9 눈 확인 | 사용자 결정 ① |
| **S1** 1일 | **퀵바 읽기**: `scan_quickbar` hero2 분기 = `ag.j`+`ag.a:B`+`bn.j` → 아이콘·쿨타임. 아틀라스 생성기. `HERO2_GUEST_RE.md` §6 #1 닫기 | `items.rs` `atlas.rs` `tools/gen_h2_atlas.py` `guest_anchors.rs`(Hero2Ktf `quickbar: Some(Hero2Quickbar{..})`) | probe `--gauges` 확장(`[quickbar] j=[1,10,5,2] cd=[-1,..]`) · S9 에서 4 플레이트 아이콘 = 게임 HUD 와 같은가 | 읽기 전용, 위험 0 |
| **S2** 1~2일 | **쓰기 게이트 + 팝업 재작성 + 커밋**: §4.5 표 · §4.2 · probe `--poke` 개방 | `guest_build.rs` `items.rs`(popup_* hero2 판) `emulator.rs`(관문 재검사·복원·합성 OK) `wie_probe` | ★ 헤드리스 A/B: 위 키 스크립트로 팝업을 띄운 뒤 (a) 라벨·길이 poke → 덤프 프레임에 4행 상자 (b) 커서 poke + OK → `ag.j` 차분(예: `[1,10,5,2]` → `[1,10,5,1]`) (c) 야수창 동일 · S9 실기 | 3.4 의 버킷 제약 준수. hero2 첫 게스트 쓰기 — 좁은 표로 |
| **S3** 1일 | **창 모델**: `quickpage.rs` 를 hero3 79차 판으로(값 정체성 · GUEST 4 · LRU · PENDING · own/arc · HIDE) + `QuickActivate` + 저장 | `quickpage.rs` `quickpage_selftest.rs` `emulator.rs` `app.rs` `config.rs` | 셀프테스트(hero3 33건을 hero2 정체성으로) · probe: 비상주 플레이트 활성화 = `ag.j` 차분 + 핫키 → 시전(`[canvas]` 변화) · S9 | 세이브 슬롯 정적(`ag.c:B` 후보) 실측 선행 |
| **S4** 1일 | **터치 라우터 1단계**: `hero2touch.rs`(Grid + 창 체인 워커) — 필드 메뉴 8행 · 팝업 행 · 스킬창 칸/탭 | `hero2touch.rs` `hero2touch_selftest.rs`(include!) `items.rs::menu_state/menu_tap_action` hero2 판 `emulator.rs::menu_tap` `app.rs::menu_tap_hit` | 셀프테스트(실측 좌표 픽스처 176×220·452×245) · probe: 커서 poke → 다음 프레임 하이라이트 이동(픽셀 diff) · S9 로그 한 줄 | `bw.a` 루트만. 다른 루트는 무동작 |
| **S5** 1~2일 | **터치 라우터 2단계**: 확인창(kind 2) · 인벤 7×4(`bf`) · 장비 · STATUS · SYSTEM · 야수창 · 셸(타이틀·슬롯·옵션: 싱글턴 `y/aw/bz/t/v/ck`) · 대화 선택지 | 같은 파일들 + 창별 기하 표 | 창마다 probe 진입 스크립트 + 프레임 픽스처 | SKT 페인터에서 상수를 읽고 **실측으로 확인**한 창만 연다 |
| **S6** ½일 | 패드: `PadAction::Quick(n)` + `menu_digit` + picking 중 `PopupBindSlot` | `gamepad.rs` | 패드 셀프테스트 | |
| **S7** 별도 | 포션 플레이트(호스트 신규): 플레이트에 회복템 등록 = `a.t`(선택) 쓰기 + `0` 전송; 인벤 팝업(`사용하기/상세보기/버리기`)에 `퀵슬롯등록` 행 추가 | | | 사용자 결정 ⑤ |
| 문서 | 각 단계 끝에 `HERO2_GUEST_RE.md`(새 앵커) · `CLAUDE.md`(규율) · 이 문서(완료 기록) 갱신 | | | |

순서 근거: S1 은 읽기만이라 **오늘 기기에서 보이는 첫 변화**(아이콘)이고 S2·S3 의 캡처가 그 위에
선다. S2 가 S3 보다 앞인 이유 = 등록이 있어야 창 모델을 채울 것이 생긴다. S4 는 S2 의 팝업 행
탭(`PopupSelectRow`)을 포함하므로 S2 뒤.

---

## 6. 검증 계획

* **헤드리스(기기 불필요)** — 전부 `.\build_probe.ps1` 로 빌드한 `wie_probe` + 위 키 스크립트:
  1. 팝업 상태 기록: `--dump-mem 0x100000:0x96434,0x48000000:0x8000000` 후 `chain.py`(이 세션 스크래치 →
     `tools_re/hero2_chain.py` 로 승격) 가 `bw.a → s → d` 와 라벨을 찍는다.
  2. 재작성 A/B: `--poke 1800:<labels[2] bytes addr>:c4fcbdbdb7d4b5eeb7cf,1800:<len addr>:04000000,…` →
     `--dump 1820` 프레임에서 상자 높이 70·4행·`퀵슬롯등록` 흰 행 확인.
  3. 커밋 A/B: `--poke 1800:<d.h>:03` + `1801:OK` → `--dump-mem` 에서 `ag.j` 가 `[1,10,5,2]` → `[1,10,1,2]`
     (슬롯 2 ← 일격, 중복이던 슬롯 0 은 `-1`… 게임 규칙대로) 인지.
  4. 라우터: 커서 poke 뒤 프레임 diff 로 하이라이트 이동(66차 재실측 방식).
* **셀프테스트**(윈도우에서 도는 것만 — CLAUDE.md 「셀프테스트」 절): `hero2touch_selftest.rs`,
  `quickpage_selftest.rs`, `landscape_selftest.rs`(겹침 불변식), `anchors_selftest.rs`(새 앵커가 hero3 주소를
  열지 않음 — `hero2_opens_no_hero3_guest_address` 유지).
* **기기(S9 `<S9_SERIAL>`)**: 단계마다 logcat 한 줄(`quickbar:` · `popup:` · `menu tap:`)과
  스크린샷. `USE AFTER FREE` 리포트 0 유지(우리 배열 길이 쓰기가 GC 와 상호작용하므로 S2 뒤 특히).

---

## 7. 사용자 결정이 필요한 것

1. **`#` 처리** (§4.7): 제안 A(`0` 롱프레스 + 풀아웃 키패드, 권고) / B / C.
2. **팝업 셋째 행**: `퀵슬롯해제` 추가(권고, 할당자 제약을 자연스럽게 만족) vs vendored `bucket.rs` 수정으로
   2행 허용(코어 이탈 +1). 라벨 문구는 10 B 짜리 아무 것이나 가능(`퀵슬롯해제` / `등록 해제`+공백 등).
3. **비상주 플레이트 활성화**: raw write 2 B(권고) vs 게임의 `ag.a(IIZ)` 호출(검증 비용 큼).
4. **패널 밖 탭**: 삼킴(hero3 규칙, 권고) vs CLR.
5. **포션 플레이트(S7)**: 이번 범위에 넣을지. `0`/`#` 의 운명(1번)과 묶여 있다.
6. **저장 파일 정체성 키**: (세이브 슬롯, 클래스) 로 갈지, 캐릭터 이름까지 넣을지(이름 문자열 위치 미확정).

---

## 8. 부록

### 8.1 이미지 (`docs/quickslot_touch_plan/`)
| 파일 | 내용 |
|---|---|
| `00_probe_sequence_176x220.png` | 실측 6장: 필드메뉴 → SKILL PASSIVE → 헤더 → ACTIVE → 커서 → 팝업 |
| `01_field_icons.png` | S9 가로 필드 + 현재 기하 그대로의 7 플레이트(아이콘은 S1 결과) — **`#` 겹침 표시** |
| `02_skill_popup_now.png` | 지금의 게임 팝업 6행 (제안 A 기하) |
| `03_skill_popup_proposed.png` | 재작성 4행 (`퀵슬롯등록` 선택) — 상자 위치·크기·색은 게임 규칙, 글꼴만 굴림 대체 |
| `04_picking.png` | picking: 플레이트 금색 링 + 안내 |
| `05a/b/c_router_*.png` | 필드 메뉴 8행 · 스킬창 칸/탭 · 팝업 행의 히트 박스(실측 프레임 위) |
| `mock_hud.py` | 생성기 — `overlay.rs` 기하의 파이썬 사본 (겹침 계산에도 씀) |

### 8.2 정적 슬롯 요약 (게스트 주소, 전부 이번 세션 실측)
```
0x193aec ag.a:I   상태워드 (3=필드 5=창)        0x193b5c ag.a:La;  캐릭터
0x193a6c ag.a:B   클래스 0..2 (=2 로그)          0x193c0c ag.f:I    돈 (=276037)
0x193bac ag.j:[B  퀵바 [4]  = [1,10,5,2]         0x193bbc ag.a:[I   쿨타임 [4] = [-1,-1,-1,-1]
0x193d6c ay.a:[S  클래스2 쿨타임표 [35,200,70,180,80,80]   (bl 0x1940cc / bx 0x19425c 는 미초기화)
0x19422c bw.a:Lbw; 필드메뉴 싱글턴               0x1941ec/0x1941fc bw.a/b:I 패널 원점 (28,51)@176x220 (166,64)@452x245
0x19420c bw.a:Li;  gm 표 (팝업 라벨의 주인)       0x19423c bw.a:Z 스킬포인트 有  0x19424c bw.a:B
0x194b4c ax.g 폭 · 0x194b5c ax.i 높이(전체 fb) · 0x194b6c ax.j cx · 0x194b7c ax.k cy
0x19443c/4c/5c s.a/b/c:B  스킬창 statics (6/4/2)
인스턴스: j.a +0 부모 · j.b +4 자식 · j.h +8 커서 · j.i +0xc 행수 · d.kind +0x28 · d.labels +0x2c · d.height +0x30
          s.a:Z +0x28 · s.b:Z(탭) +0x2c · s.d:B +0x30 · s.a:La; +0x34
배열:     handle → *(h)=pf → len=*(pf+4) → 원소 pf+8 ;  필드 블록 alloc = len·esz + 8 (버킷 4/8/16/32/64/128/256/512)
```

### 8.3 hero3 파일 좌표 (이식 원본)
* 창 모델: `wie_app/wie_android/src/quickpage.rs` 1~160 (헤더·상수) · `Window` 438~670 · `Pages` 681~1450
* 팝업: `items.rs` 1400~1770 (`PopupView`·`popup_view`·`popup_relabel`·`popup_guest_row`·`popup_list`·`popup_select_write`·`popup_count_restore`) · `emulator.rs` 1578~1720 (arm 4개) · 1990~2035 (`popup_commit`) · 4344~4400 (매 프레임 재작성)
* 라우터: `menutouch.rs` 1~200 (`Grid`·표) · `items.rs` 4640~4960 (`MenuState`·`MenuTapAction`·`menu_tap_action`) · `emulator.rs::menu_tap` 1202~ · `app.rs` 터치 체인 3780~3880
* 기록: `CLAUDE.md` 1604~2040 (66차~80차) · 2271~2384 (79차) · `PLAY_FEEDBACK_FIXES.md` 3592~3830 (79차·79차-C)
