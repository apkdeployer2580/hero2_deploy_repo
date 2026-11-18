# 영웅서기2 리메이크 (hero2NativeAPP) — 착수 설계서

2026-08-18. 세 소스(hero2 원본 2종, hero3NativeAPP, Hero5NativeAPP)를 심층 분석한 결과와
그로부터 도출한 실행 계획. **요구사항: hero2를 hero3처럼 네이티브 앱으로 리메이크하되,
게임 HUD는 hero5의 HUD를 기본 베이스로 수용하고(불필요 기능은 제거 가능), UX는 hero3를 참고한다.**

> ★★ **2026-08-22: `HERO2_FEATURE_PARITY.md` 를 먼저 읽어라.**
> hero3 8대 기능(퀵슬롯 / 게임 HUD / 게임패드 / 오프라인 공용창고 / 캐시샵 / 프레임 최적화 /
> 발열 최적화 / 컷신 hidden 정책)을 전수 조사하고 hero2 적용 가능성을 판정한 문서다.
> 기기(S9)·`wie_probe`·정적 RE·에셋 전 코퍼스 실측을 담고 있고, **이 설계서의 서술 9건을
> 뒤집는다**(그 문서 §11 에 목록). §5 이식성 매트릭스와 §7 로드맵과 §9 RE 노트는
> 그 문서가 대체·확장한다. 아래 본문에는 정정 지점마다 `★ 정정` 표시를 달았다.

---

## 0. 결론 요약

| 결정 | 내용 | 근거 |
|---|---|---|
| 게스트 원본 | **KTF판 영웅서기2 1.0.7** (`client.bin1120`, WIPI-C 네이티브, 176×220) | wie_ktf 파이프라인에 그대로 맞는 유일한 판. SKT판(1.0.2)은 자바 MIDlet이라 네이티브 경로가 없다 |
| 에뮬레이터 코어 | **vendor/wie 포크 그대로** (`wie_ktf`) | 로더가 `client.bin1120`을 무수정으로 받는다(아래 §3). hero5의 rehost 방식은 해당 없음(hero5는 ELF .so, hero2는 WIPI-C) |
| HUD 구현 엔진 | **hero3의 오버레이 스택** (`overlay.rs`+`placement.rs`+`config.rs`+`padhud.rs`) | hero5 앱에는 **호스트 HUD 코드가 0줄**이다(아래 §2) — hero5 HUD는 "디자인 스펙"으로만 수용 가능하다 |
| HUD 디자인 | **hero5 인게임 HUD의 화면 배치를 재현** | 스크린샷 실측 스펙 §4 |
| 저장소 | 별도 저장소 `hero2NativeAPP` 신설, vendor 포크 공유 | §6 |

---

## 1. 원본 판정 — 확보된 파일 3종

### 1.1 영웅서기2 SKT판 (`2bab8335-____2.zip`)
* SKVM/SKTP 배포본 (`wipi.nate.com`), MIDlet `rpg.RPGHero2`, 벤더 (유)핸즈온모바일코리아.
* 클라이언트 표시 버전 **V.1.0.2** (`bw.class` 상수 풀), MIDlet-Version 1.0.0, 2006-11 빌드.
* **순수 자바 MIDlet** — 난독화 `.class` 만 있고 네이티브 바이너리가 없다.
* → wie의 J2ME/SKVM 경로(`wie_j2me`/`wie_skvm`) 대상이지 네이티브 리호스트 대상이 아니다.
  hero3에서 쌓은 자산(ARM 블리터 훅, guest_anchors, RE 도구)이 전부 무효.
  **채택하지 않는다.** (콘텐츠 대조용 참고 자료로만 보관 — 1.0.2 vs 1.0.7 차이 확인용.)

### 1.2 영웅서기2 KTF판 (`fc25b253-____2_1.zip`) ★ 채택
* `__adf__`: PID `PD004517`, AID `01035D0B`, **Ver 01.00.07 (=1.0.7)**, MClass `rpg.RPGHero2`,
  `DisplaySize:176*220`, "Semi Network", 정보이용료 3000원.
* `01035D0B.jar` 안에 **`client.bin1120`** (614,356 B, Thumb ARM) + 기본 에셋.
  * ★ **정정 (2026-08-22)**: 여기 원래 "WIPI-C" 라고 적었던 것은 **오기다**. hero2 는
    **KTF 가 AOT 컴파일한 Java MIDlet** 이다. jar 에 `.class` 는 정말 0개지만, 바이너리 안에
    **Java 클래스 테이블(fo 0x092238.., stride 0x24, 108 클래스 / 878 필드 / 3725 메서드)** 과
    JavaFullName 문자열이 평문으로 들어 있다. 게스트 클래스명은 난독화 2글자(`bv`, `ag`, `bn` …).
    실측상 게스트→호스트 트래픽이 **`wipic 0`, 전부 KTF Java 인터페이스**다.
    → hero3 의 "C 구조체 CHAR + `CHAR+imm`" 모델이 hero2 에 그대로 적용되지 않는다.
    상세는 `HERO2_FEATURE_PARITY.md` §7·§8·§9.
* `P/` = 게임이 첫 실행 때 받는 추가다운로드(194KB 상당)가 이미 받아진 상태:
  맵 `m/m/*.m.gz` 111장, 사운드 `snd/*`, `hl2List`, `n1/n2/n3`, `g1/g2/g3`, `cf`.
* 에셋 포맷: 스프라이트 `mph/mpd`, 텍스트 `t/*.tdf`(길이접두 EUC-KR — "클레르/버서커/나이트/로그"
  직업명이 바로 디코드됨), 아이템 `itm/`, `itmatt.tdf`/`itmtp.tdf`.
* SKT판(1.0.2)보다 나중 빌드이기도 하다.

### 1.3 영웅서기3 LGT (`6fef42b1-____3_______1_0_4_LGT.zip`)
* **파일명의 "1.0.4"는 오기다.** `app_info` = `Ver:01.00.07`, `binary.mod` md5 =
  `6a352944bb43c0789ca9f4b6f0702f4b` — 현재 APK가 싣는 바로 그 빌드(01.00.07).
* 이 클라우드 클론에는 gitignore 때문에 `hero3_fixed.zip`이 없었는데, 이 첨부로
  hero3 원본이 확보됐다(참고·대조용). 새 정보는 없음.

---

## 2. ★ 핵심 발견 — "hero5의 HUD"는 코드가 아니라 디자인이다

Hero5NativeAPP(`github.com/apkdeployer2580/hero5_deploy_repo`)를 전수 분석한 결과:

* **hero5 앱에는 호스트 HUD가 존재하지 않는다.** 앱 측 UI 표면 전체가
  `MainActivity.java` 70줄 + `hl5_android/src/lib.rs` 827줄(부트·블릿·터치 전달)이 전부다.
  버튼·디패드·퀵슬롯·미니맵·설정·레이아웃 에디터 전부 0개.
* 이유는 구조적이다: **영웅서기5는 2011년부터 터치 게임**이다. HP/MP바, 미니맵, 스킬 버튼,
  가상 조이스틱을 전부 `libHeroesLore5.so`(게스트)가 자기 프레임버퍼에 그리고, 호스트는
  터치 좌표를 `nativeTouchHandleEvent`로 넣어줄 뿐이다.
* 반면 hero2는 hero3와 같은 **WIPI 키패드 게임**이라 터치 개념 자체가 없다. 즉 hero3가
  그랬듯 가상 컨트롤 레이어를 호스트가 만들어야 하고, 그 유일한 실전 코드베이스는
  **hero3의 `wie_android`** (overlay.rs 4,264줄 + 지원 모듈)다.
* 또 hero5의 코어는 wie가 아니라 ELF `.so`를 libc/JNI 경계에서 리호스트하는 별개 물건
  (`hl5_core`, 혈통은 ChronosSwordNativeAPP)이다. hero2(WIPI-C)에는 해당 없음.

**따라서 "hero5 HUD 수용"의 올바른 실행형은:
hero3의 HUD 엔진(순수 호스트, 게임 결합 0)으로 hero5 인게임 HUD의 화면 배치·조작감을 재현하는 것.**
hero5에서 가져올 코드는 0줄이고, 가져올 것은 §4의 디자인 스펙과 §7의 방법론이다.

---

## 3. 부트 경로 — hero2 KTF는 wie에 "거의 그대로" 물린다

hero3NativeAPP 부트 파이프라인 분석 결과 (핵심만):

* 캐리어 감지는 파일 존재 검사다: `__adf__` 있으면 KTF (`wie_ktf/src/emulator.rs:73`),
  `binary.mod` 있으면 LGT. hero2 KTF 원본은 `__adf__`를 갖고 있다.
* `__adf__` 파서는 `AID:`/`PID:`/`MClass:`만 읽고, 아카이브에서 `<AID>.jar` 멤버를 연다.
  hero2의 `01035D0B.jar`가 그대로 맞는다.
* client.bin 탐색은 이름이 `client.bin`으로 **시작**하기만 하면 된다(두 경로 모두 prefix 매치).
  `parse_bss_size("client.bin1120") == 1120` 케이스는 **wie에 유닛테스트로 이미 존재한다**
  (`wie_ktf/src/adf.rs:88-106`). RWPI 정적 베이스는 게스트 엔트리 스텁이 스스로 계산하므로
  에뮬레이터가 알 필요 없다.
* 저장소/DB 경로는 ADF의 AID/PID로 키가 잡히므로 `0002865E` 하드코딩 없음(전수 grep 확인).
* **필요한 유일한 가공 = 에셋 재패키징.** `wie_android`는 `from_archive`만 부르므로
  외부 zip `{ __adf__, 01035D0B.jar, P/** }` 형태여야 한다(멤버의 선행 `P/`는 wie가 벗겨서
  게스트 FS에 넣는다). → **완료: `hero2_ktf.zip` 886,851 B 생성** (원본 zip이 이미 이 모양이라
  아이콘/`__class__`/`__env__`만 빼고 그대로 재압축).
* 바이너리 패치(`vendor/wie/data/binary_patches.toml`)는 md5 키라 hero2에는 아무것도 안 붙는다.
  → 처음엔 **블리터 가속 없음**(정확성 무관, 성능 과제). hero3의 blit565/blend565/blitlut565
  사례처럼 hero2의 핫 블리터를 프로파일로 찾아 해시 엔트리를 새로 판다.
* 176×220 해상도: wie는 `Screen` 트레이트가 보고하는 값으로 프레임버퍼를 만들므로 구조적으로
  임의 해상도 지원. 걸리는 것은 전부 `wie_android`의 상수들이다 —
  `CONTENT_W/H=240/320`, `LOGICAL_W_MIN=240`(176을 막는다), `POPUP_SAFE_LOGICAL_W=496`
  (hero3 팝업 렌더러의 8bit 절단 버그 전용 — hero2에선 무의미, 재도출 전까지 제거),
  `BASE_H_CHOICES`. hero2가 176×220 외 해상도를 소화하는지는 **wie_probe로 실측**이 먼저다
  (hero3도 문서 결론이 실측으로 뒤집힌 전례가 있다 — LANDSCAPE_FEASIBILITY.md).
  * ✅ **이 상수 작업은 커밋 `7a92d2d`(2026-08-19)로 전부 끝났다.** `CONTENT_W/H=176/220`,
    `LANDSCAPE_BASE_H=220`, `LOGICAL_W_MIN=176`, `BASE_H_CHOICES=[160,190,220]`,
    `POPUP_SAFE_LOGICAL_W=LOGICAL_W_MAX`(캡 해제).
  * ★ **기기 실측 (2026-08-22, S9)**: DIAG INFO = `render 2220x1080, logical 452x219,
    immersive true`. 즉 게스트는 **176×220 이 아니라 452×219** 로 돌고 있고 그걸 그대로 소화한다.
    산출식은 `init_logical`(`lib.rs:357-372`): `w = 220×2220/1080 = 452`(짝수 마스크),
    `h = 452×1080/2220 = 219`(문서화된 ±1 정수 반올림). → **가로 이머시브 전략 성립 확정.**
  * ★ 그런데 **프레임버퍼가 16bpp 가 아니라 4bpp** 라서 `platform.rs:282` 의
    `bytes_per_pixel()==2` 게이트가 **세 가지 빠른 경로를 동시에 끈다**: 동일프레임 스킵(기기
    실측 `skipped 0/s`), RGB565→ARGB 빠른 변환(→ `image.colors()` 폴백 = 프레임마다 ~1.6MB
    임시 할당), 16bpp 블리터 빠른 경로(`canvas.rs:547`). `HERO2_FEATURE_PARITY.md` §3.4.

---

## 4. HUD 디자인 스펙 — hero5 인게임 HUD (스크린샷 실측, 400×240 기준)

`Hero5NativeAPP/docs/shots/hl5-08-gameplay-hud.png` 등에서 추출한 배치. hero2 HUD의 목표 화면:

```
┌────────────────────────────────────────────────────┐
│ [초상화+Lv] HP▓▓▓▓ 152        map | shop | save    │ ← 좌상: 초상+수치바 / 상단: 텍스트 버튼 줄
│            MP▓▓▓ 100                    ┌────────┐ │
│                                         │ 미니맵 │ │ ← 우상: 선묘 미니맵
│                                         └────────┘ │
│                                             [스킬] │
│                                             [스킬] │ ← 우측 세로 스킬 칼럼(쿨다운 라디얼)
│   (게임 화면)                               [스킬] │
│                                             [DASH] │
│  ◉ 조이스틱                        [아이템]  ┌───┐ │
│ (플로팅 디스크)        [퀵슬롯 ↔]  [수량5]  │ ◎ │ │ ← 우하: 대형 공격/OK 디스크
└────────────────────────────────────────────┴───┴──┘   하단 중앙: 아이템 퀵슬롯+페이지 화살표
```

구성요소 → hero3 오버레이의 대응물 (전부 이미 존재, 재배치+스킨 작업):

| hero5 HUD 요소 | hero3 오버레이 대응 | 작업 |
|---|---|---|
| 초상화+Lv+HP/MP 수치바 (좌상) | `draw_stats` HP/SP/EXP 캡슐바 | 배치 이동, 초상화 슬롯 추가, 수치 표기 |
| `map / shop / save` 텍스트 버튼 줄 (상단) | `MenuKey` 플레이트 + `QuickSave` | 텍스트 버튼 스타일로 스킨, 키 바인딩은 hero2 실측 후 |
| 미니맵 (우상) | `Minimap` 패널 + `minimap_render` | 패널 위치만 이동. **렌더 체인은 hero2 RE 후** (§5) |
| 스킬 버튼 세로 칼럼 + 쿨다운 | `Quick(0..4)` 플레이트 + `draw_cooldown_overlay` | ★ **세로 배치 프리셋은 완료** (커밋 `7a92d2d` 가 `overlay.rs:515-554 ring_slots` 를 hero5식 우측 2열 클러스터로 교체 + `hero5_layout_tests` 회귀). 남은 것은 hero2 스킬 슬롯 RE 후 아이콘 |
| DASH | 예비 버튼 (hero2에 대시 개념 있는지 실측 후) | 없으면 제거 (요구사항: 불필요 기능 제거 가능) |
| 대형 공격/OK 디스크 (우하) | `Ok` 디스크 | 그대로, 스킨만 |
| 아이템 퀵슬롯+수량 배지+페이지 화살표 (하단 중앙) | `Quick`+`QuickPage`+수량 배지 | 그대로, 배치만. ★ 아이템 칸은 **호스트 소유**다 — hero2 게임에는 아이템→슬롯 등록이 없다 (2026-09-01) |
| 플로팅 조이스틱 (좌하) | `Stick` (터치타입 A) | 그대로 — hero5와 같은 개념이 이미 있다 |

hero5 HUD에 **없어서** hero3 UX에서 가져올 것(요구사항 "UX는 hero3 참고"):
설정 기어+모달, **레이아웃 에디터**(placement.rs — 창 비율 좌표+빌트인 % 크기 모델 그대로),
당겨쓰는 12키 키패드(WIPI 게임엔 필수), 게임패드 위치 배지(padhud), 토스트, HEAT SAVER,
Cutscene/Menu/Field 모드 전환, 한글 토스트(hangul.rs 재생성 규율 포함).

hero5에서 추가로 배울 UX 규칙: 기본 배치를 hero5처럼 잡되, hero2는 탭-투-무브가 아니라
키패드 게임이므로 hero5의 "첫 탭=하이라이트, 둘째 탭=실행" 규칙은 해당 없음.

---

## 5. 이식성 매트릭스 — hero3 `wie_android` 기준

hero3 앱 34파일을 게스트 결합도로 전수 분류한 결과:

**그대로 복사 (게임 지식 0):** `placement.rs`, `overlay.rs`(그리기·기하·설정·에디터 전부),
`padhud.rs`, `gamepad.rs`, `hangul.rs`(단어표만 교체), `ui_icons.rs`, `config.rs`(20개 txt),
`endpoint.rs`, `tls.rs`/`tls_buf.rs`, `audio_sink/mix.rs`, `platform.rs`, `database.rs`,
`filesystem.rs`, `orientation.rs`, `server_prompt.rs`, `vibration.rs`, `app.rs` 이벤트 배관.
오버레이는 `Stats`/`QuickSlot`/`Minimap` 같은 **호스트 구조체만 소비**하므로(게스트 메모리 직독 없음)
HUD 스킨 작업이 게스트 RE와 완전히 분리된다.

**전면 재작성 (hero3 RE 산출물, 약 5,900줄):** `items.rs`(CHAR/퀵바/아이템DB 오프셋 244개),
`guest_anchors.rs` 값 전부, `minimap_render.rs`의 체인 오프셋(알고리즘은 이식),
`save.rs`/`backup.rs`/`buffs.rs`/`quickpage.rs`의 주소·스키마, `emulator.rs`의 guest_build 호출부 38곳,
`network.rs`의 리다이렉트 표(hero2가 하드코딩한 서버 주소로 교체), `icons.bin`(hero2 아트로 재추출).

**유지할 아키텍처 규율:** `guest_build`/`guest_anchors`의 **3축 게이트**(데이터 앵커 / 항목별
코드 패치 / `guest_code_addresses_valid()` 캐치올)를 첫날부터 채택. hero3에서 세 번의 오판을
막아준 구조다. 통짜 boolean 금지.

---

## 6. 저장소 전략

**별도 저장소 `hero2NativeAPP` 신설** (이 저장소에 섞지 않는다):

* `vendor/wie`·`vendor/arm32_cpu`·`vendor/softbuffer`·`vendor/oxisynth` 포크는 그대로 가져간다.
  포크가 이미 담고 있는 게임 무관 수정(OOM 예외 싱글턴, `call_guest_function_out`,
  `JvmThreadGuard`, `MC_knlUnsetTimer`, `select_record_ktf` 0바이트 수정, 오디오 250ms 유예,
  LGT net 범위 디스패치)이 전부 load-bearing이다. 이상적으로는 포크를 한 번 발행해서
  두 앱이 공유(두 사본 표류 방지).
* `wie_probe` 통째 복사 — 개발 루프의 중심. `wie_android`는 hero3 전용 7모듈만 비우고 복사.
* `build_android.ps1` 복사(에셋 하드게이트·bake 검증·서명 파일명 신호 유지),
  `applicationId`는 새로("com.hero2remake.app" 등 — Play 정체성·세이브 경로라 영구),
  Java `namespace net.wie.emulator`는 JNI 조회가 걸려 있으니 유지.
* 셀프테스트: **전부 `include!` 방식으로만** (items/landscape 미러 표류 함정 재생산 금지).
  리눅스 호스트에서는 `cargo test`가 그냥 돌므로 미러 파일 자체가 불필요.
* 서버: hero2는 "Semi Network" — 온라인 표면이 hero3(창고/복구/상점)보다 훨씬 작을 것.
  `protocol.py`+`server.py`(프레이밍·리스너·JSONL·hot-reload `replies.json`)만 복사하고
  `hero3_protocol.py` 125KB는 버린다. 프레이밍 상수(u32 길이, 1KB 수신버퍼)도 hero3 실측값이라
  hero2 바이너리에서 재검증. 같은 Lightsail 박스에 포트 하나 추가(관례: hero3 8091, hero4 8092,
  chronos 8094, hero5 8095 → **hero2 = 8090 또는 8096**).

---

## 7. 로드맵

**Phase 0 — 헤드리스 부트 (지금, 기기 불필요)**
1. ✅ 에셋 재패키징 (`hero2_ktf.zip`).
2. ✅ **부트 성공 + 인게임 진입 확인** (2026-08-18, 이 저장소의 wie_probe 그대로):
   * `wie_probe --zip hero2_ktf.zip --w 176 --h 220 --ms 20` → Hands-On 로고 →
     타이틀("영웅서기2 빙해의검사 V.1.0.7 PRESS ANY KEY") → OK 연타로 **인게임**
     (신드라프 상점 맵, 캐릭터 스프라이트, NPC 대화 "장비 능력 보여/아이템 조합",
     게임 자체 하단 HUD)까지 렌더. 사운드 로드/재생도 시작됨.
   * 발견·수정한 버그 1건: **rustjava Thread 종료 패닉**. hero2는 hero3("Clet")와 달리
     진짜 자바 MIDlet(ARM 컴파일)이라 단명 워커 스레드를 만들었다 끝낸다. run() 리턴 시
     게스트가 Thread 인스턴스를 스스로 해제하는데, `ThreadStartProxy`가
     `detach_thread()` **후에** `alive=false`를 쓰다 `NoSuchFieldError` → 예외 문자열
     생성이 등록 해제된 스레드에서 `jvm.rs:167` unwrap 패닉. → rustjava(62cf0c6)를
     `vendor/rustjava`로 벤더링하고 정리 순서를 detach 앞으로 옮김 + put_field 실패 관용.
     (§7 리스크 R3이 예견한 바로 그 유형 — probe가 몇 분 만에 표면화시켰다.)
   * `[work]` 실측: 부트 구간 ~4.1M instr, 이후 프레임당 ~0.03M — hero3(2.46M/frame)의
     **1~2% 수준**. 176×220 구세대 게임이라 성능 과제가 거의 없을 가능성.
   * blit HLE 게이트는 예상대로 매칭 0 (`calls=0`) — 필요해지면 hero2 md5 엔트리 신설.
3. ✅ **해상도 적응성 확인** (176×220 / 240×220 / 352×220 / 440×220 동일 시나리오 덤프):
   hero2는 **해상도 적응형**이다 — 맵은 넓어진 만큼 시야가 늘고, 안내 바("신드라프 상점")는
   전폭으로 늘어나며, 게임 하단 HUD는 중앙 정렬 + 측면 장식으로 그려진다. 손상·크래시 없음.
   → **hero5식 가로(예: 440×220, 2:1) 이머시브 HUD 전략 성립.** hero3의
   `LANDSCAPE_BASE_H` 접근을 220 기준으로 옮기면 된다. 메뉴/팝업 화면별 확인은 남음.
4. 프레임 페이싱 확인: hero2의 타이머 주기 실측 (hero3은 20ms 타이머가 45ms로 돌고 있었다 —
   같은 함정 1일차에 점검).
   * ✅ **45ms 함정 수정은 이미 시드에 상속돼 있다** (2026-08-22 확인): 이벤트 펌프
     `vendor/wie/wie_midp/.../net/wie/event_queue.rs:323-338` 이 `(due-now).clamp(1,16)` 만 자고,
     워커 루프 `emulator.rs:2120-2141` 이 `FRAME_PACE - elapsed` 를 `next_wakeup_ms()` 로
     상한 건다. **hero2 는 이 함정을 물려받지 않았다.**
   * ★ 대신 **다른 것이 나왔다** (기기 실측): hero2 는 S9 에서 **6~11 fps** 이고,
     `guest set 0/s timers` — 게스트가 WIPI 타이머를 **하나도 걸지 않는다**(자바 스레드
     기반이므로). `wie-emu` 스레드가 100~120% 로 포화된 상태에서 게스트 명령은 0.6~1.0 M/s 뿐이고
     tick 하나가 24~90 ms 다. 병목은 §7 Phase 3-9 정정 항목 참조.
   * ⚠ 계측 한계: `wie_probe` 는 **프레임 페이싱을 원리적으로 측정 못 한다**(1틱/프레임 +
     가상 시계). **게스트 fps 와 동일프레임 스킵률도 못 센다**(`stats::add_frame()` 호출지가
     `platform.rs:267` 하나뿐). → 이 지표들은 **기기 logcat 전용**이다.

**Phase 1 — HUD 없는 APK (첫 기기 빌드)**
5. hero3 전용 7모듈(items/guest_anchors/save/backup/buffs/quickpage/minimap_render) 뺀
   `wie_android` 사본으로 APK. 키패드+OK/CLR만으로 플레이 가능한 상태가 데이 원 목표.
   (앱 아이콘/라벨/applicationId 신규.)

**Phase 2 — hero5 스킨의 HUD**
6. §4 스펙대로 기본 레이아웃 프리셋 작성(placement 기본값 교체) + 오버레이 스킨
   (초상화 슬롯, 텍스트 버튼 줄, 스킬 세로 칼럼). 이 단계는 게스트 RE 불필요 —
   버튼이 누르는 건 아직 그냥 WIPI 키다.
   * ✅ **기본 배치 완료** (커밋 `7a92d2d`, 2026-08-19): 화면 상수 176×220,
     `LOGICAL_W_MIN=176`, `POPUP_SAFE_LOGICAL_W` 캡 해제, `ring_slots` → hero5식 우측 2열 클러스터.
   * ★ **그런데 그 배치가 화면에 뜨지 않는다** (2026-08-22 기기 실측). `kind()=None` →
     `field=false` → 모드가 **영구 Menu** → placement 가 항상 MENU 배열을 쓴다.
     플레이 중 보이는 HUD(`1 3 # 7 9` + 디패드)는 **MENU 레이아웃**이고 GAME 레이아웃
     (STICK + Q1~Q5 + MAP 패널)은 한 번도 안 뜬다. 1·2·3·8번이 이 자물쇠를 공유한다 —
     `HERO2_FEATURE_PARITY.md` §2.
   * ★ 남은 배치 작업: 초상화 슬롯, `map/shop/save` 텍스트 버튼 줄, 아이템 퀵슬롯 하단 중앙.
   * ✅ **좌석↔키 표 — 2026-09-01 수정 완료.** 위 서술("hero2 실제 `1/3/7/9/0` + `#`=페이지")
     **자체가 틀렸다**: 스킬 슬롯은 `1 3 7 9` 4개, `0` 은 회복아이템 전용키, `#` 는 회복아이템
     교체다. 그리고 `quickpage.rs` 는 "불필요"가 아니라 **필요**하다 — hero2 에 페이지 개념이
     없으므로 페이지는 호스트가 만들어야 한다(사용자 결정). 현재:
     `QUICK_SLOTS = 5`(바) / `GUEST_QUICK_SLOTS = 4`(게스트), 5번째는 호스트 칸.

**Phase 3 — 게스트 RE (hero3 방법론 그대로)**
7. CHAR 앵커부터 (읽기 전용 전부가 여기 매달린다: 게이지→퀵바→상태워드→미니맵 순).
   도구: `tools_re/h3dis.py` 계열을 hero2용으로 (`client.bin1120`, 게스트주소 = fo + 0x100000),
   `lgt_statics_xref.py`의 KTF 버전, probe `--poke`/`--verify-offsets` A/B.
8. HP/MP 수치바·스킬 쿨다운·미니맵·퀵세이브를 RE 진도에 맞춰 HUD에 연결.
   각 항목은 `guest_anchors` 3축 게이트로 개별 개방.
9. ~~성능: probe `--profile`로 핫 블리터 찾아 `binary_patches.toml`에 hero2 md5 엔트리 신설~~
   → ★ **정정 (2026-08-22): 폐기.** `--profile` 을 실제로 돌렸고 **핫 블리터가 없다**.
   샘플 341,000개 중 **96.35% 가 호스트↔게스트 경계**(ARM SVC 벡터 59.45% + `RUN_FUNCTION_LR`
   36.90%)이고 게스트 루틴은 어느 것도 **0.18% 를 넘지 않는다**. 상위 30개는 전부 AOT-Java
   런타임 헬퍼(getfield/배열/new). 명령 27.09M / 엔진 배치 338,280 = **왕복당 명령 80개**.
   → hero2 의 최적화 축은 블리터가 아니라 **KTF Java 인터페이스**(`get_java_method` 의
   무캐시 문자열 해석)와 **4bpp 프레임버퍼 게이트**다. `HERO2_FEATURE_PARITY.md` §3.

**Phase 4 — 오프라인 로컬 컨텐츠 (2026-08-20 확정: 서버는 만들지 않는다)**

★ 사용자 결정: **네트워크 기능은 넣지 않는다.** 캐시샵·공용창고를 포함해 hero3 이
서버로 처리하던 것을 전부 **로컬로** 구현한다 — 비행기모드에서도 모든 컨텐츠가 된다.
이건 Phase 4 를 미루는 게 아니라 *다른 것으로 대체*하는 것이다.

10. 빌드에서 서버 제거. ✅ `build_android.ps1` §0b 가 `HERO3_SERVER_*` 를 **지우고**,
    §2b 가 빌드된 `libmain.so` 에 `tls://` 가 있으면 **실패**한다(증거 역전). 이 기계의
    `gradle.properties` 에 hero3 서버가 남아 있어 hero2 APK 가 `<hero3-server>` 를
    물려받고 있었다 — 실측으로 확인하고 막았다.
11. `network.rs`/`tls.rs`/`endpoint.rs`/`server_prompt.rs`/`hero3_link.rs`/`tls_buf.rs`
    제거 또는 로컬 응답기로 대체.
    * ★ **정정 (2026-08-22): "무한로딩 위험" 은 해당 없다.** 기기 실측 — 네트워크창고에서
      `예` 를 누르면 `org.kwis.msf.io.Network::connect()` 스텁(`Ok(-1)`) → `disconnect()` →
      `net.wie.WieError: jump native address is null at bv.run()V` 로 게스트 워커만 죽고,
      화면은 **`서버 장애 중입니다. 잠시후 다시 이용해 주시길 바랍니다.`** 로 우아하게 실패한다.
      앱·UI 정상.
    * ★ **훅 지점이 hero3 와 층이 다르다.** hero3 `offline.rs` 는 WIPI-**C** 넷 슬롯 아래의
      바이트스트림(`network.rs State::Local`)에 물리지만, hero2 는 그 위 **WIPI-Java 클래스 층**
      (`org.kwis.msf.io.Socket`)을 탄다. `vendor/wie` 에 `Socket`·`URL` 클래스가 **아예 없다**.
      2단 계획은 `HERO2_FEATURE_PARITY.md` §4.4. **이 작업은 CHAR 앵커와 무관해 먼저 착수 가능.**
    * ★ hero2 서버 주소 확정: **`211.239.150.70:30002` / `:30000`, 스킴 `BillSocket://`**
      (게스트 문자열 풀 fo 0x081588 등). hanbit IP 표의 교체 대상이 이 값이다.
12. ~~캐시샵/~~**공용창고**의 **저장소**는 앱 로컬 파일(호스트 측), 게이트는 `guest_anchors`.
    공용창고 = 슬롯 간 공유 보관함이므로 캐릭터 레코드 밖에 두는 게 맞고, 그래서
    게스트 인벤토리 쓰기 경로(Phase 3)가 선행 조건이다.
    * ★ **정정 (2026-08-22): 캐시샵은 삭제.** hero2 원본에 **캐시샵이 없다** — 에셋 전 코퍼스
      (1137 블롭 / 2.6 MB, 5개 인코딩)에서 캐시·결제·유료 계열 히트 0, `t/*.tdf` 799 문자열
      전수 해독, 상수풀 한글 10개 전부 전투/인벤 UI. `정보이용료` 는 데이터 통신료 고지문이고
      `CARD` 는 전투 카드 덱이다. 대조군: 같은 개발사 hero3 는 클라이언트에 `캐시`/`유료 아이템
      구매`/`실제 현금` 문자열을 남긴다. hero2 는 0건. 상세 `HERO2_FEATURE_PARITY.md` §4.5.
    * ★ hero2 창고에 필요한 응답 의미론은 **RE 전에 이미 다 안다**: `t/nt.tdf` 24행 +
      `t/gm.tdf[60..66]` = 3탭(돈·아이템·카드) × 2동작(올리기·내리기) + 카드 전송 + 공지사항.
    * ★ hero2 세이브는 WIPI DB 가 아니라 `org.kwis.msp.io.File` → `g1`/`g2`/`g3` 파일이다 →
      hero3 `database.rs` 훅과 `vault::slot_has_save`(DB 스캔)는 hero2 에서 발화하지 않는다.

**리스크 상위 3개**
* R1: hero2의 WIPI API 표면이 hero3와 달라 `MC_*` 스텁 구현량이 클 수 있다 → Phase 0-2에서 즉시 측정됨.
* R2: 176×220 비적응(해상도 하드코딩) → 세로 고정 + 스케일 표시로 후퇴 가능(§3 상수 재설계).
* R3: KTF 자바 부트스트랩(`rpg.RPGHero2` 로딩)이 hero3와 다른 JVM 표면을 요구 → probe 부트 로그로 즉시 판별.

---

## 8. 이번 세션에서 물리적으로 확보된 것

* `hero2_ktf.zip` (스크래치패드) — wie_ktf `from_archive` 형태로 재패키징 완료.
* hero2 KTF/SKT 원본, hero3 LGT 01.00.07 원본 zip 3종 (업로드 디렉터리).
* Hero5NativeAPP 클론 및 HUD 부재 확정 + 스크린샷 기반 디자인 스펙(§4).
* hero3 `wie_android` 34파일 결합도 전수 분류(§5).

---

## 9. 게스트 RE 실측 노트 (2026-08-20, 기기+프로브)

### 9.1 확정된 것

* **게스트 메모리 맵** (`wie_core_arm::core`): 이미지 `0x100000..0x1A0000` (그 위는 매핑
  없음 — `read_memory` 가 범위를 넘으면 **빈 Vec 을 돌려준다**. 0 바이트 읽힘 = 실패지
  0 채움이 아니다). 리스트 힙 `0x40000000..0x48000000`, 버킷 힙(작은 할당)
  `0x48000000..0x50000000`. 실사용은 리스트 힙 앞 6 MB 안에 사실상 전부 들어간다
  (32 MB 를 떠도 non-zero 가 늘지 않는다). 프레임버퍼는 `0x40320000..0x40360000` 대역.
* **`CLR` = 게임 메뉴 키**. 대화창이 떠 있으면 첫 CLR 이 그걸 닫고, 필드에서 CLR 이
  캐릭터/INVENTORY/EQUIPMENT/SKILL/BEAST/QUEST/CARD/SYSTEM 메뉴를 연다.
  프로브 재현: `--keys 320:OK,420:OK,520:OK,620:OK,720:OK,780:CLR,810:CLR,880:OK`
  → 프레임 960 에 STATUS 화면.
* **번들 세이브의 실측값** (STATUS 화면, 위 시퀀스로 언제든 재현):
  클레르/수련생/로그, LV 48, EXP 195/112128, HP 709/780, SP 390/390,
  ATK 138, DEF 115, CRITICAL 43%, DODGE 35%, 힘 56, 민첩 118, 체력 17, 의지 17, 지력 10.
* **세이브 슬롯 = `P/g1`·`P/g2`·`P/g3`** (889/447/960 B). 셋 다 `00 06` 으로 시작하고
  오프셋 7~9 가 `d5 00 2d` 로 공통이다. 내용은 평문이 아니다 — 6바이트 주기의 반복
  패턴(`5b 2b 43 0c 23 44`…)이 보이는 걸로 봐서 비트팩 스트림이거나 마스킹돼 있다.
  `P/cf` (10 B) = 설정, `P/hl2List` (94 B) = 추가다운로드 목록.

* ★ **hero2 에는 자체 `네트워크창고` 가 있다** (2026-08-20, 기기 실측). 슬롯 선택
  화면과 같은 UI 를 제목만 바꿔 쓴다. 그리고 그 화면에서 게스트 워커 스레드가 죽는다:
  `net.wie.WieError: Fatal error: jump native address is null / at bv.run()V`.
  앱은 살아 있고 게스트 스레드만 죽는다. → **Phase 4(로컬 공용창고)의 훅 지점이
  바로 여기다.** 서버를 세우는 대신 `bv.run()` 이 부르는 것을 로컬 응답기로 대체하면 된다.
  * ★ **2026-08-22 정밀화**: `bv` 는 게스트 클래스명(`client.bin1120` fo 463251 = 게스트
    `0x171193`)이고 층은 **WIPI-Java `org.kwis.msf.io.Socket`** 이다. 누락 클래스는 정확히 3개
    (`Socket` `0x173761` / `URL` `0x173ECE` / `msf/core/ProgramExitException` `0x175264`).
    실패 화면은 `서버 장애 중입니다.` — **무한로딩 아님.** `HERO2_FEATURE_PARITY.md` §4.4.
  * ★ **캐시샵 확인 완료: 없다.** (같은 문서 §4.5)
* 게임 자체 하단 HUD = HP(적)/MP(청) 바 + 1·3·7·9·0 아이템/스킬 퀵슬롯 + 수량 배지
  + `#` 페이지 키. hero5식 호스트 HUD 는 결국 이걸 대체하는 것이므로, 필요한 데이터는
  정확히 이 HUD 가 읽는 것과 같다 — CHAR 앵커가 선행 조건인 이유.

### 9.2 CHAR 앵커 — 아직 못 찾았다 (실패한 방법과 이유)

* **값 검색 실패.** 위 14개 수치 중 어느 것도 힙/이미지에 인접한 u16/u32 쌍으로
  존재하지 않는다. `(709,780)`·`(390,390)`·`(195,112128)` 를 u16/u32 × LE/BE 전수로
  찾았으나 리스트 힙 32 MB, 버킷 힙 16 MB, 이미지 640 KB 어디에도 없다. 개별 값은
  존재하지만(709 가 14곳 등) 서로 64 B 안에 모이는 조합이 하나도 없다.
  → 레코드가 **평문 정수 배열이 아니다**. 세이브 파일이 비트팩/마스킹된 것과
  같은 처리를 RAM 에서도 하고 있을 가능성이 높다. (또는 ATK/DEF/CRIT/DODGE 처럼
  표시값 다수가 파생값이고 저장되는 건 기초 스탯뿐일 수도 있다.)
* **이동 차분 실패.** UP→DOWN 왕복(정지구간 안정 + 왕복 복귀) 필터로 3.1 M 워드에서
  6개까지 좁혔으나 전부 `0x400fff00..0x400fffff` — `0x40100000` 의 할당 블록 헤더
  (`ef be ad de` 매직) 바로 아래라 스택/스크래치다. 176(화면폭) 같은 값이 섞여 있다.
  LEFT 단조 이동은 두 번째 버스트가 벽에 막혀 3점 단조가 성립하지 않았다.

### 9.3 다음에 시도할 것 → ★ **`HERO2_FEATURE_PARITY.md` §9 가 이 절을 대체한다**

2026-08-22 조사로 §9.2 의 실패가 **설명됐고** 공략선이 완전히 바뀌었다. 요지만:

* **왜 실패했는가**: hero2 의 캐릭터 레코드는 평문 정수 구조체가 아니라 **Java 객체 +
  `short[]`/`byte[]` 배열들**이다. 필드 하나를 읽는 데도 `*(obj+0)+off+4` 이중역참조를 거치므로
  인접 u16/u32 쌍 검색이 (709,780) 을 못 찾은 것은 당연하다.
* ~~1. STATUS 화면 문자열 역추적~~ → **실측 음성으로 폐기.** `client.bin1120` 안에 EUC-KR
  `힘/민첩/체력/의지/지력` **0건**, ASCII `CRITICAL/DODGE/ATK/EXP/STATUS/…` **전부 0건**.
  라벨은 에셋 안에 있다 — `t/hero.tdf[18..22]` = 힘/민첩/체력/의지/지력,
  `t/gm.tdf[5..12]` = STATUS/INVENTORY/…/SYSTEM.
* **대신 훨씬 좋은 조건**: 클래스 108개·필드 878개·메서드 3725개의 이름·오프셋·본문주소가
  이미지 안에 있다. "CHAR 앵커를 찾는다"가 **"어느 Java 객체가 캐릭터인지 확인한다"** 로 축소된다.
  후보 체인은 이미 좁혀졌다: `ag` static (슬롯 게스트 `0x193b5c`) → `a` → `+0xd8` → `bn` →
  `[S a@+0x14`/`[S b@+0x18`. 세이브 매니저는 클래스 `ag` 이고 직렬화 6함수 주소도 확보됐다.
* **좌표계 확정**: `RELOC_DELTA = 0xbf10`, 정적 베이스 `sl = 게스트 0x195CA4`,
  Java 문자열 상수풀 = 게스트 `0x181392~0x181c20`.
* 실행 계획(P0-a/b/c, P1-a/b/c, P2)은 각 단계에 **성공·실패 판정 기준**이 붙어 있다 →
  `HERO2_FEATURE_PARITY.md` §9.2.
* 2·3번(이동차분 재시도 / 세이브 인코딩)은 살아 있으나 **조건이 바뀌었다**: 전역 32MB 스캔이
  아니라 P0-b 에서 얻은 객체 포인터 주변만 차분하고, 세이브는 통계가 아니라 `ag` 역어셈으로 깬다
  (통계로는 6바이트 주기와 순환열 `10 05 47 18 3b 2b` 까지 갔고 단일 XOR 키가 아님이 밝혀졌다).

⚠ **`wie_probe` 안전 규율**: probe 는 hero2 를 부팅하면서도 `GuestBuild::Ktf`(= **hero3 앵커**)를
세팅한다(`main.rs:698`, 로그 실측 `char_anchor=*0x1b3084`). `--verify-offsets`/`--char-dump`/
`--find-fb`/`--map-*` 는 무의미한 숫자를 내고, **`--poke`/`--host-save`/`--host-levelup` 은
hero3 주소로 실제 write/call 을 발사한다 — hero2 에서 절대 쓰지 말 것.**
