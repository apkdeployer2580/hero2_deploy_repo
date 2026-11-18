# hero3 8대 기능 → hero2 이식 전수 조사

2026-08-22. 사용자 요구 8개 항목(퀵슬롯 / 게임 HUD / 게임패드 / 오프라인 공용창고 / 캐시샵 /
프레임 최적화 / 발열 최적화 / 컷신 버튼 hidden 정책)에 대해 **hero3 구현을 전수 조사하고
hero2 적용 가능성을 판정한 결과**. 세션별로 차례차례 구현하기 위한 실행 근거 문서다.

조사 방법: hero3 트리 정독 + git 히스토리 발굴 + hero2 트리 대조 + **S9 실기기 실측**
(SM-G960N / Android 10 / `com.hero2remake.app` 0.1.0 debug) + **`wie_probe` 헤드리스 실측**
+ `client.bin1120` 정적 RE + 에셋 전 코퍼스 해독. 저장소 무수정.

정본 계획서는 `HERO2_REMAKE_PLAN.md` 이고, 이 문서가 그 §5·§7·§9 를 대체·확장한다.
**이 조사로 계획서의 서술 6건이 뒤집혔다** — §11 에 목록이 있다.

---

## 0. 결론 요약

| # | 기능 | hero2 현황 | 판정 | 규모 | 순번 |
|---|---|---|---|---|---|
| 1 | 퀵슬롯 | 코드 100% 존재(시드와 0 diff), 게이트로 정지. **키표가 hero3 값이라 틀리다** | `after-deeper-re` | XL | 3 |
| 2 | 게임 HUD | 엔진·배치 존재(hero5 클러스터 **완료**), **내용이 전부 None** | `after-char-anchor` | XL | 1 |
| 3 | 게임패드 | 매핑·L3·R3 **전부 이식돼 있고 동작**. 레이블만 hero3 의미론 | **`now`** | M | 2 |
| 4 | 오프라인 공용창고 | 게스트 UI 완비, 훅 지점 확정. **hero3 offline.rs 와 층이 다르다** | `after-deeper-re` | XL | 3 |
| 5 | 캐시샵 | **hero2 원본에 없다** (전 코퍼스 실증) | `needs-hero2-content-re` | XL | 8 |
| 6 | 프레임 최적화 | 6~11 fps. **병목이 hero3 와 완전히 다르다** | **`now`** | M | 2 |
| 7 | 발열 최적화 | HEAT SAVER 동작. **동일프레임 스킵은 영구 정지** | **`now`** | M | 4 |
| 8 | 컷신 hidden 정책 | 규칙 구현돼 있으나 **반대로 켜져 있다** | `after-char-anchor` | M | 5 |

★ 판정의 핵심은 표가 아니라 **§2 의 단일 근본원인**과 **§3 의 성능 반전**이다. 이 둘이
순번을 결정한다.

---

## 1. hero2 에 물리적으로 없는 코드 — 커밋 `289c9fd` 델타

hero2 는 hero3 `262dd63`(2026-08-19)의 포크다. **hero2 에 없는 hero3 변경은 정확히 커밋
`289c9fd`(2026-08-21, "오프라인 네트워크, 패드최적화 작업 진행중") 하나**다. 그 이전 커밋
(57차 발열 `4742313`, 58차 게임패드 `c3046f2`, 59차 패드HUD `1c335b8`)은 **전부 시드에 포함**돼 있다.

| 파일 | 줄 | 역할 | hero2 관련성 |
|---|---|---|---|
| `offline.rs` | +894 (신규) | 인앱 사설 서버 엔진 (프레임 단위 요청/응답, 소켓 없음) | **설계는 재사용, 층이 다름** (§6) |
| `vault.rs` | +488 (신규) | 공용창고 저장소 (슬롯 간 공유 보관함) | 저장 모델 재사용 가능 |
| `savevault.rs` | +305 (신규) | 세이브 금고 | hero2 는 DB 가 아니라 파일 → **재설계** |
| `cash_catalog.rs` | +137 (신규) | 캐시샵 카탈로그 | **이식 대상 아님** (§7) |
| `gamefont.rs` + `gamefont_data.rs` | +1,417 (신규) | 게임 폰트 렌더러 + 글리프 데이터 | 설정 한글화용, 선택 사항 |
| `network.rs` | +364 | `State::Local` 추가 | hero2 는 `Fresh/Connecting/Connected/Dead` 뿐 |
| `emulator.rs` | +413 | 오프라인 배관 + 계측 | 부분 이식 |
| `overlay.rs` | +647/−236 | 창고/캐시샵 UI + 컷신 히스테리시스 + 패드 조건 | 부분 이식 |
| `lib.rs` | +232 | JNI 확장 | 부분 |
| `MainActivity.java` | +115 | JNI 릴레이 확장 | 부분 |
| `items.rs` | +274 | 캐시버프 + `cutscene_hold` 히스테리시스 | **히스테리시스는 §8 에서 필요** |
| `backup.rs` `database.rs` `server_prompt.rs` `hero3_link.rs` `config.rs` `platform.rs` `gamepad.rs` `hangul.rs` | +288 합계 | 지원 | 선별 |

★ **파일 md5 비교는 하지 마라.** hero2 워킹트리는 **CRLF**, hero3 git blob 은 **LF** 다.
그래서 같은 내용도 md5 가 전부 다르게 나온다. 두 트리 비교는 반드시 `tr -d '\r'` 정규화 후에.
정규화 실측: `quickpage.rs` `items.rs` `placement.rs` `guest_anchors.rs` `guest_build.rs`
`atlas.rs` `ui_icons.rs` `padhud.rs` `app.rs` = **시드와 0 줄 차이**. `emulator.rs` = **6 줄**
(정확히 `set_ktf()` 제거 + 5줄 주석). `overlay.rs` `config.rs` `lib.rs` = 커밋 `7a92d2d` 가 수정.

---

## 2. ★★ 단일 근본원인 — 모드 고착이 필드 HUD 전체를 삼킨다  ✅ **해소 2026-09-09**

> ★★★ **이 절은 이제 기록이다.** hero2 자체 앵커가 확보되어 `emulator.rs` 가
> `guest_build::set_hero2_ktf()` 를 부르고, `resolve_char` 가 성공하고, 호스트 모드가
> 실제로 `Field` 로 간다. 정본 = **`HERO2_GUEST_RE.md`**.
> 아래 §2.1~2.4 의 사슬 분석은 옳았고, 열린 경로는 §2.4 의 두 경로 중 **①(앵커 리버싱)** 이다.
> ⚠ 단 §2.3 의 부작용 서술 두 건은 이번에 **코드로 닫혔다**: `can_save_here` 의 fail-OPEN 이
> SAVE 버튼을 상시 켜는 문제는 `placement::shown` 이 세이브 체인 없는 빌드에서 그 컨트롤을
> 숨기는 것으로, 미니맵은 같은 방식으로 처리했다.


**8개 항목 중 4개(1·2·3-일부·8)가 하나의 자물쇠에 묶여 있다.**

### 2.1 사슬 (소스 + 기기로 양쪽 확정)

```
emulator.rs:1108-1113 가 guest_build::set_ktf() 를 부르지 않는다   ← hero2 의 제1규율, 의도된 것
  └ guest_build.rs:62-69  kind() = None
     └ items.rs:254-255   resolve_char() = char_anchor()? → 즉시 None
        └ char_base_raw() 도 같은 앵커를 요구 → None
           └ state / window / selector 는 전부 CHAR 상대 오프셋이다
              (char_ptr + guest_build::state_off() / + WINDOW_ID_OFF / + SELECTOR_FLAG_OFF)
                 └ in_field_state=false, cutscene_open=false, script_choice_open=false,
                   map_window_open=false, shell_state=false, can_save_here=fail-OPEN(true),
                   read_stats=None, scan_quickbar=[], minimap=None
                    └ app.rs:1865-1869  match (field,cutscene) { … _ => Menu }  → 영구 Menu
                       └ placement.rs:184-190  ids_for(Menu) = &MENU_IDS
                          ├ 1 퀵슬롯: Q1~Q5 좌석이 화면에 없다
                          ├ 2 게임 HUD: STICK / MAP 패널 / STATS 가 화면에 없다
                          ├ 3 게임패드: L3 빠른저장이 can_save 체인에서 죽는다
                          └ 8 컷신: Cutscene 이 절대 발화하지 않는다
```

### 2.2 기기 실측 증거

필드에서 걸어다니는 중에도 게이지가 매번 이것을 찍는다:

```
main::emulator: area gate: can_save=None
main::emulator: quickbar probe: char=None field=false window=None state=None
                selector=None in_game=false types=[0, 0, 0, 0, 0] minimap=None
```

그리고 SET → DIAG INFO 가 `mode Menu` 를 보고한다.

**레이아웃 에디터가 이것을 눈으로 증명한다.** hero2 의 SET → LAYOUT EDIT 는 완전히 동작하고
툴바 `GAME | MENU | FLIP | RESET | CANCEL | SAVE` 로 두 배열을 각각 보여준다:

| 배열 | 좌석 |
|---|---|
| **GAME(필드)** | `STICK`(좌하 플로팅 조이스틱) · `Q1 Q2 Q3 Q4 Q5` · `PAGE` · `7` · `CLR` · `OK` · **`MAP` 대형 패널(우상단)** · 상단 STATS 영역 |
| **MENU** | `1 3 # 7 9` 숫자 키패드 · `DPAD` 대형 존 · `CLR` · `OK` · 중앙 팝업 영역 |

→ **인게임에서 관찰되는 HUD(`1 3 # 7 9` + 디패드 삼각형 + CLR + OK)는 MENU 배열과 정확히 일치한다.**
즉 hero2 는 플레이 중에도 MENU 레이아웃을 쓰고 있고, **GAME 레이아웃은 한 번도 화면에 뜨지 않는다.**

### 2.3 그래서 실제로 무엇이 잘못 보이는가

* 컷신·대화 중에 hero3 가 "절대 띄우지 말라"고 명시한 것들(OK 디스크 · CLR · `1/3/7/9/#` 블록 ·
  4방향 화살표 · 기어/키패드 토글)이 **전부 대화창 위에 떠 있다.**
* 반대로 컷신용 반투명 `#` 스킵과 `tap_anywhere_key`(화면 전체가 OK)는 **영원히 발동하지 않는다.**
* `can_save_here`(`items.rs:1592-1596`)는 **fail-OPEN** 이라 앵커가 없어도 `true` → SAVE 버튼이
  상시 켜져 있지만 세이브 체인은 `guest_code_addresses_valid()=false` 로 막혀 있다.
  → **누르면 조용히 아무 일도 안 하는 버튼**이 이미 노출돼 있다.
* `stats_hud_on` 의 Menu 분기는 `stats.level > 0` 을 요구하는데 `read_stats` 가 None 을 돌려
  기본값(level 0)이므로 게이지 블록은 안 뜬다 — **우연히** 안전하다.

### 2.4 잠금을 여는 두 경로

**(A) CHAR 앵커 (본선)** — §9 참조. 이번 조사로 난제에서 **한 번의 probe 실험**으로 축소됐다.

**(B) 프레임버퍼 판별 (우회로, 게스트 RE 0줄)** — §8.3 참조. 컷신은 이미 실측으로 열렸다.

이 둘은 배타적이지 않다. (B)로 8번을 먼저 살리고, (A)로 나머지를 여는 것이 최단 경로다.

---

## 3. ★★ 성능 반전 — hero2 의 병목은 hero3 와 완전히 다르다

hero3 의 최적화 역사는 전부 **게스트 명령 처리량**(프레임당 2.46M)을 겨눈 것이다:
blit HLE, `binary_patches.toml` md5 엔트리, 45ms 타이머 함정. **hero2 에서는 그 축이 전부 무효다.**

### 3.1 기기 실측 (S9, 필드 걷기 45~110 사이클, `emu` 게이지 + `top -H` + `dumpsys battery`)

| 조건 | 온도 | guest fps | emu loop | tick 평균 | budget-capped | in-tick | wie-emu CPU |
|---|---|---|---|---|---|---|---|
| 타이틀/메뉴 | — | 11.5 | 53 f/s | 9.03 ms | 363/1831 (20%) | 49% | — |
| 필드, HEAT SAVER OFF (차가움) | 32.5→34.0℃ | **9.2** | 28 f/s | 23.65 ms | 300/1111 (27%) | 82% | 110% |
| 필드, HEAT SAVER ON (실적용) | 35.5→36.3℃ | **5.9** | 12 f/s | 80.81 ms | 1726/1812 (95%) | 99% | 120% |
| 필드, HEAT SAVER OFF (뜨거움, 대조) | 36.8℃ | **5.6** | 11 f/s | 89.67 ms | 1800/1800 (100%) | 100% | 100% |

**⚠ 자기정정 1**: 초기 측정에서 "HEAT SAVER 를 켜면 악화된다"고 판단했으나 **틀렸다.**
`ToggleHeatSaver`(`app.rs:1262-1276`)는 파일만 쓰고 토스트로 `재시작 후 적용`을 알린다 —
`render_div` 는 `app.rs:356` 에서 **부팅 시 한 번만** 읽는다. 첫 A/B 는 재시작이 없어 무효였다.
재시작을 포함해 다시 측정하고, **같은 뜨거운 온도에서 OFF 대조**를 붙인 결과가 위 표다:
같은 조건에서 ON 5.9 fps vs OFF 5.6 fps — **HEAT SAVER 는 원인이 아니고, 오히려 미세하게 낫다.**
실제 패턴은 **세션 경과/온도에 따른 열화**(9.2 → 5.6 fps, capped 27% → 100%)이며 원인 미확정이다.

**⚠ 자기정정 2**: "176→1080p 업스케일이 hero3(240)보다 배율이 커서 더 비싸다"는 가설도 **틀렸다.**
확대는 `App::redraw`(`app.rs:755-780`)에서 CPU 로 일어나는데, 목적지 행마다 `sy = dy*lh/g.h` 를
구해 **소스 행이 바뀔 때만** gather 하고 같은 소스 행이면 `copy_within` 순수 memcpy 다.
비용 = 목적지 픽셀 수(화면이 정하므로 hero2·hero3 동일) + 고유 소스 행 수(hero2 219 < hero3 240).
**hero2 의 확대는 오히려 미세하게 싸다.** 배율은 비용식에 들어가지 않는다.

### 3.2 진짜 병목 — 호스트↔게스트 경계 (`wie_probe --profile`, 1400프레임)

```
[prof] 341000 samples, 1061 distinct leaf PCs
[prof]  59.45%  pc=0x00000008  (ARM SVC 벡터)
[prof]  36.90%  pc=0x7f000000  (RUN_FUNCTION_LR, 호스트 복귀 센티넬)
[prof]   0.18%  pc=0x001689ea  ← 이하 전부 AOT-Java 런타임 헬퍼 클러스터
[prof]   0.08%  pc=0x0016889a
[prof]   0.08%  pc=0x00168a10
[prof]   0.05%  pc=0x00166a7c … (상위 30개 전부 0.18% 미만)
```

* **샘플의 96.35% 가 호스트↔게스트 경계 주소**다. 게스트 코드는 어느 루틴도 0.18% 를 넘지 않는다.
* 상위 30개는 전부 게스트 `0x166928~0x168a10` = **AOT-Java 런타임 헬퍼 클러스터**
  (getfield `0x1680a4`, 배열 load `0x1689e4`, 배열 store `0x168a40`, new `0x167ba0`,
  `<init>` `0x16922c` — §9.4 의 정적 RE 가 독립적으로 특정한 바로 그 함수들).
* 결정적 비율: **명령 27.09M / 엔진 배치 338,280 = 왕복 1회당 게스트 명령 80개.**
  hero2 는 명령 80개마다 호스트 경계를 넘는다.
* 기기 게이지가 이것을 확증한다: `emu svc/s by category [init 403 wipic 0 stdlib 0 hook 7]`
  — **`wipic 0`.** hero2 의 게스트→호스트 트래픽은 WIPI-C 가 아니라 **전부 KTF Java 인터페이스**다.
  (분류: 1=init, 3=wipic, 5=stdlib, 8+=바이너리패치 훅 — `wie_util/src/lib.rs:70-72`.)

### 3.3 그 경계가 왜 비싼가 — 캐시가 없다

`get_java_method`(`vendor/wie/wie_ktf/src/runtime/java/interface.rs:108-136`)는 **호출마다**:

1. `KtfJvmSupport::read_name(core, ptr_fullname)` → `JavaFullName::from_ptr`
   (`jvm_support/name.rs:17-27`) — 게스트 메모리에서 NUL 종단 문자열을 읽고,
   `String::from_utf8`, `split('+')`, **`String` 3개 할당**.
2. `vtable.find_method(&name, &descriptor)` — **문자열 선형 비교 탐색**.
3. `find_java_method`(`interface.rs:141-156`) — 부모 클래스 체인을 `#[async_recursion]` 으로 재귀,
   같은 문자열 비교 반복.

**메모 캐시가 없다.** AOT-Java 게임에서 모든 필드 접근·메서드 디스패치가 이 경로를 탄다.

### 3.4 ★ 두 번째 근본원인 — 게스트 화면이 16bpp 가 아니다

`platform.rs:282` 의 `if image.bytes_per_pixel() == 2` 게이트가 **세 가지 빠른 경로를 동시에** 켠다.
hero2 의 화면 이미지는 **4bpp ARGB** 이므로 세 개 전부 꺼진다:

| 꺼지는 것 | 위치 | 대가 | 실측 확인 |
|---|---|---|---|
| 동일 프레임 스킵 (57차) | `platform.rs:282-289` | 정적 화면에서도 매 프레임 전량 페인트 | 기기: `emu paint: sent 5/s, skipped 0/s identical` — 타이틀 정적 화면에서도 0 |
| RGB565→ARGB 빠른 변환 | `platform.rs:294-301` | `image.colors()` 폴백 = **픽셀당 임시 객체**. 452×219 에서 프레임마다 ~1.6 MB 할당·폐기 | 주석이 "never-taken today"(hero3 기준)라고 단정 |
| 16bpp 블리터 빠른 경로 | `canvas.rs:547` | `src.bpp==2 && dst.bpp==2` 를 요구 | `[blit] calls=0` |

독립 확증: 컷신 판별 조사가 프로브 덤프 프레임의 **79색 중 75색이 565 라운드트립에 실패**한다고
실측했다 — 프레임버퍼가 565 가 아니기 때문이다.

### 3.5 그래서 hero3 최적화의 hero2 유효성

| hero3 최적화 | hero2 유효성 |
|---|---|
| 45ms 타이머 함정 수정 (이벤트 펌프 `clamp(1,16)` + 워커 `next_wakeup_ms` 상한) | **이미 시드에 상속됨.** `wie_midp/.../event_queue.rs:323-338` + `emulator.rs:2120-2141` 실측 확인. hero2 는 이 함정을 물려받지 않았다 |
| blit HLE / `binary_patches.toml` md5 엔트리 신설 (계획서 Phase 3-9) | **폐기.** §3.2 프로파일이 핫 게스트 루틴의 부재를 증명했다. `[blit] calls=0`, `blit hle true` 인데 매칭 0 |
| HEAT SAVER 반해상도 | **유효하고 hero2 에서 비중이 더 크다** (§4.7) |
| 동일 프레임 스킵 | **영구 정지** — 4bpp 게이트 (§3.4) |
| 라이트 신스 (oxisynth 코러스/리버브 off) | **유효.** hero2 사운드 32개가 전부 SMAF(`MMMD`)이므로 같은 신스를 탄다 |
| **(hero2 신규)** KTF Java 인터페이스 메모 캐시 | ★ hero2 전용 최대 카드 (§3.3) |
| **(hero2 신규)** 4bpp 게이트 해소 | ★ 세 경로를 한 번에 되찾는다 (§3.4) |

### 3.6 부수 — 로그 스팸 (게스트 RE 0, 리스크 0)

* `logcat -c` 후 10초: wie 태그 **6,586줄**, 그중 `stub java_check_type` **6,630건 ≈ 663줄/초**.
  출처 = `interface.rs:354` `tracing::warn!("stub java_check_type({:#x}, {:#x}, {:#x})")` —
  게스트 호출마다 hex 3개 포맷 + Android 로그 소켓 write.
* 타이틀 구간에서는 `Graphics::setAlpha`(`wie_wipi_java/.../lcdui/graphics.rs:183`) 초당 ~14회,
  창고 진입 시 `jb_unk7`/`jb_unk8`(`interface.rs:261/267`) 폭주.
* 이 양이 **logcat 링버퍼를 넘치게 해 `emu throughput` 요약 줄을 밀어냈다** — 측정을 방해할 정도다.
* A/B (`setprop log.tag.wie S`, 같은 걷기 부하):

| | wie-emu | AudioTrack | android_main | 기기 전체 %sys |
|---|---|---|---|---|
| 로그 정상 | 110% | 20.6% | 13.7% | **65%** |
| 로그 침묵 | 100% | 6.8% | 3.4% | **17%** |

  → 커널시간이 **65% → 17%**. 다만 `wie-emu` 는 침묵 후에도 ~100% 포화이므로 **로그는 발열의
  큰 조각이지만 프레임률의 1차 병목은 아니다.** (한계: 태그를 침묵시키면 요약 줄도 사라져
  이 조건의 fps 는 측정 못 했다 — 스텁만 `trace!` 로 내린 빌드로 A/B 해야 한다.)

### 3.7 반증된 가설 (기록용)

* **게스트 힙 first-fit 워크 열화** — 반증. `wie_probe` 3000프레임에서 `list_walk_steps` 가
  f=600 에 152,377 로 **평탄화**되고 이후 전혀 늘지 않는다.
* **업스케일 배율 비용** — 반증 (§3.1 자기정정 2).
* **HEAT SAVER 역효과** — 반증 (§3.1 자기정정 1).

---

## 4. 기능별 상세

### 4.1 퀵슬롯 — `after-deeper-re` / XL / 순번 3

> ✅ **호스트 측 1단계 완료 (2026-09-01).** 아래 §4.1 본문의 키표·페이지 서술은 **틀렸다** —
> 이 블록이 정본이다.
>
> **게임의 사실** (KTF 1.0.7 아카이브 직접 디코딩): 스킬 슬롯 `1 3 7 9` **4개**,
> `0` = 회복아이템 **전용키**, `#` = 회복아이템 **교체**(페이지가 아니다).
> 게임의 등록 행은 스킬 4개뿐이고(`t/gm.tdf[34..37]`), **아이템 컨텍스트 메뉴에는 등록 행이
> 없다**(`[28] 버리기 [29] 장비장착 [30] 사용하기 [31] 상세보기`).
>
> **설계 결정 (사용자, 2026-09-01)**: 바는 **5칸**으로 간다. 5번째는 **호스트 소유**이고,
> hero3 처럼 포션을 아무 칸에나 넣는 것이 목표다. `#` 는 **게스트 키로 유지**하고 퀵슬롯 전체
> 페이지는 **전용 컨트롤**(`CtrlId::QuickPage`)이 맡는다.
>
> 반영된 것: `QUICK_SLOTS = 5` / `GUEST_QUICK_SLOTS = 4` 분리(바 폭과 게스트 폭이 다르다는
> 사실을 타입이 아니라 상수 두 개로 못박음), `quickslot_hotkey` = `1 3 7 9` + 슬롯4는 `None`,
> `is_host_slot`/`quickslot_inert` 신설, `CtrlId::Heal`(`0`)·`CtrlId::HealSwap`(`#`) 신설
> (액션 디스크 옆 키 행 `overlay::disc_row_seat`), `scan_quickbar` 가 호스트 칸까지 패딩해
> **바 폭의 단일 근원**이 됨, 비-이머시브 퀵바 루프의 상한 없음 수정(§7② 적대적 검증),
> `gamepad.rs` 기본표 수정(패드 연결 시 **오늘 이미** 오작동하던 사본), 5번째 플레이트
> 비활성 렌더 + `사용불가` 토스트.
>
> **앵커(#12) 뒤로 남은 것**: 바인딩 읽기(`scan_quickbar` ← CHAR), 스킬 페이지 복원(hero2
> `skill_bind` 또는 4배열 오프셋), **포션을 아무 칸에나**(= 아래 「측정 하나」).
>
> ★ **설계를 가르는 측정 하나**: `0`/`#` 가 퀵바 배열의 **5번째 칸**을 쓰는가?
> 쓴다면 게스트가 이미 5칸이고 호스트가 그 칸에 아이템 ref 를 쓰는 것으로 끝난다.
> 안 쓴다면 별도의 "선택된 회복아이템" 변수가 있다는 뜻이고(`#` 가 그걸 바꾸니 **반드시
> 존재한다**), 포션 슬롯 = `선택 변수에 쓰기 + 0 키 전송` 이 된다. 후자면 **아이템 슬롯은
> 페이지 복원 시 게스트 쓰기가 0** 이다.

**hero3 구조.** 5칸 액션 퀵바 플레이트 + 수량 배지 + 쿨다운 라디얼 + **호스트 소유 페이지**
(`quickpage.rs` 788줄) + 게임 팝업 리라벨 "퀵슬롯등록". 게스트 결합: 퀵바 4배열
(`QB_TYPE +0x252` / `QB_ITEMREF +0x257` / `QB_SKILLREF +0x25c` / `QB_SKILLIDX +0x26c`),
인벤토리 그리드 이중간접(`+0x9e28 → +8 → tab*0x3c4 + slot*0x3c → +0x3ba`),
스킬 시트 id(`+0xd1+i`), 팝업 핸들 배열, `hud_bottom_patch`(게임 자체 하단 HUD 의 퀵바 셀을
지우는 Thumb 분기 1개).

**hero2 현황.** `quickpage.rs`(788줄, 인라인 `mod tests` 16개) · `items.rs`(`scan_quickbar`:416,
`slot_writes`:557, `skill_slot_writes`:586, `popup_view`:868, `hud_bottom_patch`:1481) ·
`overlay.rs` 그리기 7함수(`quick_buttons`:1542, `draw_cooldown_overlay`:1815,
`draw_quickbar`:1851, `draw_skill_buttons`:1978, `draw_pick_highlight`:2060) ·
`placement.rs`(`CtrlId::Quick(u8)`:81, `QuickPage`:111, 직렬화 `q0..q4`/`qpage`, 라벨 `Q1..Q5`)
— **전부 시드와 0 줄 차이로 존재하고, 게이트로 전부 정지.**

**★ 지금 당장 고쳐야 하는 것 — 키표가 틀리다.**

`placement.rs:80-83` 의 독스트링이 직접 말한다:

```rust
/// Field quickbar plates, seats 0..=4 — hotkeys 1 / 3 / 9 / 0 / #.
Quick(u8),
/// Menu-navigation keys, seats 0..=4 — 1 / 3 / 7 / 9 / #.
MenuKey(u8),
/// Secondary-weapon attack (key 7), field only.
Secondary,
/// The minimap panel (tapping it presses `*`), field only.
Minimap,
```

그리고 `lib.rs:131-141 quickslot_hotkey` = `0→NUM1, 1→NUM3, 2→NUM9, 3→NUM0, 4→HASH` —
**`guest_build` 참조가 0개인 게이트 없는 순수 함수**다.

| | hero3 (코드) | hero2 (기기 실측) |
|---|---|---|
| 필드 퀵슬롯 키 | `1 3 9 0 #` | **`1 3 7 9 0`** |
| 페이지 | **호스트 소유**(`quickpage.rs` 가 게스트 슬롯 바인딩을 RAM 에 되쓴다) | **게임 자체 `#` 키** |
| 보조무기 | 키 `7` 전용 좌석 | 없음 — `7` 이 퀵슬롯이다. ★ **장비 슬롯도 없다** (`t/shop.tdf`: 무기/방어구(방패 포함)/기타). → `CtrlId::Secondary` 2026-09-01 삭제 |
| 지도 | 미니맵 탭 = `*` | `*` 개념 미확인 |

→ 결과 두 가지:

1. **버그 예정.** (✅ 2026-09-01 해소 — 위 블록 참조. 아래는 당시 서술이고 `#` 를 "게임
   페이지" 로 부른 것은 오독이다: `#` 는 회복아이템 교체다.) 모드 게이트를 열어 필드
   레이아웃이 뜨는 순간 `Quick(4)` 가 `#` 를 보내
   슬롯을 쓰는 대신 **게임 페이지를 넘긴다**. 그리고 hero2 의 3번째 슬롯 `7` 은 `Secondary`
   좌석으로만 닿는다. `quickslot_hotkey` 는 게이트가 없어 **CHAR 개방 순간 조용히 틀린 키를 쏜다.**
2. ★ **hero3 의 `quickpage.rs`(788줄)는 hero2 에 불필요하다.** hero3 가 호스트 페이지를 만든
   이유는 게스트 슬롯이 5개뿐이라 **게스트 메모리에 바인딩을 되써야** 했기 때문(= 게스트 쓰기
   경로 = RE 선행)이다. hero2 는 게임이 이미 `#` 페이지를 갖고 있으므로 **호스트는 `#` 키를
   보내면 끝이고 게스트 쓰기가 0** 이다. → 1번은 "quickpage 이식"이 아니라 **"quickpage 폐기 +
   `#` 전송"** 이 정답이고, **게스트 RE 선행조건이 하나 줄어든다.**

**hero2 에 물리적으로 없는 것.** `app.rs` +33줄(패드로 퀵슬롯 등록: `slot_for_hotkey()` 신규 함수
+ `if diverted` 뒤의 `else if` 분기 — 삽입 위치는 `app.rs:1113`/`:1114` 사이),
`gamepad.rs` 실질 4줄(hero2 현재 `L1=NUM9, R1=NUM0, L2=HASH, R2=NUM7` → 289c9fd 는
`L1=NUM7, R1=NUM9, R2=NUM0`).

**아트.** `icons.bin` md5 `964176ff929799b234bcc18463a6166a`, 82,998 B = **hero3 시드 blob 과
바이트 동일** = hero3 아트("H3IC", item_bm 29 + s4..s10 7시트). hero2 스프라이트는 `mph`/`mpd`
포맷이다. ✅ **디코더는 2026-09-01 에 생겼다** (`tools_re/dump_mpd.py` — `.mpd` 는 시그니처를
뗀 PNG 청크 스트림, `.mph` 는 프레임표+공유 `PLTE`; 아카이브 2,539 프레임 전부 파싱).
남은 것은 **아틀라스 생성기**이고, 소스 아트를 꺼내는 문제는 더 이상 아니다.

**함정.** `landscape_selftest.rs`(67 tests)에 `QuickPage`/`QuickSave` 참조가 **0개** —
손으로 쓴 미러가 원본과 표류한 상태(CLAUDE.md 가 경고한 그 함정). `items_selftest.rs`(44)도
손 미러이고 `quickbar`/`QB_` 참조 35곳. 새 테스트는 `include!` 방식만.

### 4.2 게임 HUD — `after-char-anchor` / XL / 순번 1

**hero3 구조.** `overlay.rs`(217KB) — 게이지(`draw_stats`) · 퀵바 플레이트 · 미니맵 패널 ·
컨트롤 레이어 · 설정 모달 · 레이아웃 에디터 · 토스트 · 패드 배지, `placement.rs`(43KB) 좌표 모델,
`minimap_render.rs` 체인, `ui_icons.rs`/`atlas.rs`/`icons.bin`, 289c9fd 신규 `gamefont.rs`(+data).

**★ hero2 는 배치를 이미 hero5 식으로 갈아치웠다.** 커밋 `7a92d2d`(2026-08-19,
"화면 상수 176×220 + hero5식 우측 클러스터 HUD 기본 배치")가:

* `CONTENT_W/H = 176/220`(`lib.rs:284-285`), `LANDSCAPE_BASE_H = 220`(`lib.rs:303`),
  `LOGICAL_W_MIN = 176`(`lib.rs:305`), `BASE_H_CHOICES = [160,190,220]`(`config.rs:542`).
* `POPUP_SAFE_LOGICAL_W = LOGICAL_W_MAX`(`lib.rs:313`) — hero3 팝업 렌더러의 8bit 절단 버그
  전용이던 496 캡 해제.
* **`overlay.rs:515-554 ring_slots` 를 hero3 의 호(arc) 배치 → hero5 식 우측 세로 클러스터
  (최대 2열, 미니맵 아래 ~ 액션디스크 위)로 전면 교체.** `Quick(i)` 는
  `ring_seat(i+1)`(`placement.rs:583`)이므로 이것이 퀵슬롯 플레이트의 좌표를 만드는 코드다.
* 회귀 테스트 `overlay.rs:4245+ mod hero5_layout_tests` — 클러스터 불변식(≤2열·무겹침·디스크
  위·미니맵 아래·엄지 크기)을 2개 창에서 검증 + `HUD_MOCK_OUT` 목업 생성(기기 불필요).

→ **계획서 §4 의 "스킬 버튼 세로 칼럼 … 세로 배치 프리셋" 작업 항목은 이미 완료됐다.**
제가 기기 레이아웃 에디터에서 본 `Q5 Q2 / Q4 Q1 / Q3 CLR` 2열 클러스터 + 우상단 MAP 패널 +
우하단 대형 OK 디스크 + 좌하단 STICK 이 정확히 이것이다.

**남은 것은 배치가 아니라 내용이고, 전부 앵커 게이트다.** `read_stats`=None(게이지),
`minimap`=None(미니맵 렌더 체인), `scan_quickbar`=[](플레이트 아이콘), 쿨다운 라디얼.
그리고 hero5 스펙의 `map / shop / save` 텍스트 버튼 줄과 초상화 슬롯은 미착수.

**hero2 고유 차이.** 게임이 **자체 하단 HUD 를 그린다**(HP 적/MP 청 캡슐바 + `1·3·7·9·0`
퀵슬롯 아이콘 + 수량 배지 + `#` 페이지 위젯 + 좌우 해골 장식). 호스트 HUD 는 결국 이걸
대체하는 것이므로 `hud_bottom_patch`(게임 HUD 를 지우는 게스트 코드패치)의 hero2 대응이
없으면 **이중 표시**가 된다. 그리고 `draw_stats` 가 게임 HUD 박스를 **`176 x 24`** 로 가정하는데
(`overlay.rs:2613-2614`) 이건 hero3 실측값이고 hero2 화면폭이 우연히 176 인 것이다.
잔여 hero3 상수: `overlay.rs:2265 SHELL_ART_W` 주석이 아직 "// 240",
`mask_game_hud` 세로 경로가 `288/320`·`48/240`·`28/320` 비율.

**해상도.** 기기 DIAG 실측: `render 2220x1080 (heat saver div 1), logical 452x219, immersive true`.
**게스트 논리 해상도가 176×220 이 아니라 452×219 다.** 산출식은 `init_logical`
(`lib.rs:357-372`): `w = 220 × 2220/1080 = 452`(짝수 마스크), `h = 452 × 1080/2220 = 219`
(문서화된 ±1 정수 반올림). 게스트가 이걸 그대로 소화하므로 **가로 이머시브 전략이 기기에서도 성립**한다.
(다만 `init_logical` 의 폴백 주석 "keeps the original 240x320" 은 hero3 잔존 서술 — 실제 폴백은 176/220.)

### 4.3 게임패드 — **`now`** / M / 순번 2

**hero2 에 전부 이식돼 있고 동작한다.** 58차(`c3046f2`)·59차(`1c335b8`)가 시드에 포함됐다.
기기 DIAG 전문:

```
게임패드 (위치 = 모든 패드 공통)
  면버튼 아래 (Xbox A) = 확인 / 공격        면버튼 오른쪽 (Xbox B) = 취소 / 메뉴
  면버튼 왼쪽 (Xbox X) = 퀵슬롯 1           면버튼 위 (Xbox Y) = 퀵슬롯 2
  왼쪽 어깨 위 (L1/LB) = 퀵슬롯 3           오른쪽 어깨 위 (R1/RB) = 퀵슬롯 4
  왼쪽 트리거 (L2/LT) = 퀵슬롯 5 / 등록(#)  오른쪽 트리거 (R2/RT) = 보조무기 (7)
  가운데 왼쪽 (SELECT) = 지도 (*)           오른스틱 누르기 (R3) = 퀵바 페이지
  ↑ 2026-09-01 이전 로그. 현재 기본표는 L1=퀵슬롯3(7) R1=퀵슬롯4(9)
    L2=회복 아이템(0) R2=회복 아이템 교체(#). R3=퀵바 페이지는 그대로.
    5번째 플레이트는 호스트 슬롯이라 키가 없고, 따라서 패드로 닿지 않는다.
  왼스틱 누르기 (L3) = 빠른저장             십자키 / 왼스틱 = 이동 / 커서
메뉴에서는 같은 버튼이 그 창의 키가 된다 (게임 자신의 키 라우팅).
```

**하지만 레이블이 hero3 의미론이다.** hero2 에서 `7` 은 퀵슬롯이고(보조무기가 아니다), `*` 지도
개념은 미확인이고, `#` 는 **회복아이템 교체**다(페이지가 아니다 — 이 서술은 §2② 가 뒤집었다).
→ §4.1 의 키표 재정의와 **같은 작업**이고 ✅ **2026-09-01 에 완료됐다.**

**죽어 있는 하위 항목 2개.**
* `L3 빠른저장` — `can_save` 체인이 CHAR 앵커를 요구. 기기 `area gate: can_save=None`.
* `R3 퀵바 페이지` — `items::quickbar_identities` 가 CHAR 앵커를 요구. **여전히 그렇다.**
  ⛔ 괄호 안의 옛 결론("그냥 `#` 를 보내면 된다")은 **틀렸다**: `#` 는 페이지가 아니라
  회복아이템 교체다. 페이지는 호스트가 소유하며 앵커가 필요하다.

**PAD HUD 행이 안 보이는 이유.** 기기 DIAG: `pad hud Auto (now TOUCH), pad seen false`.
59차의 PAD HUD 설정행과 위치 배지는 **패드가 한 번 감지된 뒤에** 나타난다. 버그가 아니다.

**즉시 가능한 작업(RE 0).** 키 의미표 재도출은 `wie_probe` 로 필드에서 숫자키를 하나씩 넣고
게임 자체 HUD 반응을 관찰하면 답이 나온다 — CHAR 앵커 불필요. `overlay.rs:176 MENU_KEYS`
(`1/3/7/9/#`)에 hero2 용 `0` 을 추가해야 하는지도 같은 실험으로 판정된다.

### 4.4 오프라인 공용창고 — `after-deeper-re` / XL / 순번 3

#### 훅 지점 — 기기 실측으로 확정

타이틀 캐러셀 `이어하기 / 환경설정 / 도움말 / **네트워크창고** / 정보이용료 / 게임종료` →
`네트워크창고` → **슬롯 선택 UI**(제목만 교체: `SLOT 1 / NORMAL / 로그 LV 48 / 달성도 45%`
— 로컬 세이브 메타를 읽어 표시하므로 창고 플로우는 **캐릭터(슬롯) 스코프**) →
`네트워크 접속시에는 통화료가 부과됩니다. 접속하시겠습니까? 예/아니오` → `예` →

```
W wie: wie_wipi_java::classes::org::kwis::msf::io::network: stub org.kwis.msf.io.Network::connect()
W wie: wie_wipi_java::classes::org::kwis::msf::io::network: stub org.kwis.msf.io.Network::disconnect()
E wie: java_runtime::classes::java::lang::thread: Uncaught exception in thread 708735333:
E wie: net.wie.WieError: Fatal error: jump native address is null
E wie: 	at bv.run()V
E wie: 	at java/lang/Thread.run()V
```

→ 화면 결과: **`서버 장애 중입니다. 잠시후 다시 이용해 주시길 바랍니다.`**

★ **게스트가 실패를 우아하게 처리한다.** 앱·UI 정상, 워커 스레드만 죽는다.
계획서 §7 Phase 4-11 의 "무한로딩 위험" 우려는 **해당 없음**으로 정정.

#### ★★ hero3 `offline.rs` 와 층이 다르다

| | hero3 | hero2 |
|---|---|---|
| 게스트가 말하는 것 | WIPI-C 넷 슬롯 30/31/32/33 (`wie_ktf/.../wipi_c/method_table.rs:329-371`) | **WIPI-Java `org.kwis.msf.io.Socket`** |
| 호스트 훅 | `network.rs` `State::Local` → `AndroidNetwork::{connect,send,recv}` = **바이트스트림 층** | 그보다 한 층 위 **자바 클래스 층** |
| 엔디안 | u32 **LE** 프레이밍 | Java `DataOutputStream` = **빅엔디안** |
| 프로토콜 표 | 한빛 hero3 서버용 `CAT_*`/`OP_*` | **완전 미지** |

**누락 클래스는 정확히 3개** (게스트가 참조하는 라이브러리 클래스 41개 중):
`org/kwis/msf/io/Socket`(게스트 `0x173761`), `org/kwis/msf/io/URL`(`0x173ECE`),
`org/kwis/msf/core/ProgramExitException`(`0x175264`).
`vendor/wie` 전체에 `Socket` 도 `URL` 도 **한 줄도 없다** —
`wie_wipi_java/src/classes/org/kwis/msf/io/` 에는 `network.rs` 와
`scheme_not_found_exception.rs` 둘뿐이다. 그리고 `Network::connect()` 는 `Ok(-1)` 스텁이다.

게스트가 말하는 API 표면(문자열 풀 실측): `Socket.find(String)` static factory,
`getInputStream()/getOutputStream()`, `close()`, 그 위에
`DataInputStream`/`DataOutputStream`/`ByteArrayInputStream`/`ByteArrayOutputStream`
(전부 등록돼 있음) + `readInt/writeInt/readShort/writeShort/readByte/writeByte/readFully/available/flush/toByteArray`.
게스트 자체 클래스에 `Lorg/kwis/msf/io/Socket; a` 필드(소켓 핸들)가 있다.
`MC_` 문자열은 바이너리에 **0건**.

**★ hero2 에는 바이트스트림 백엔드가 이미 있다.** `network.rs` 의
`AndroidNetwork::{socket,connect,send,recv}`(`:509/523/638/733`) + `REDIRECT_ENABLED`(:65) +
`connect_resolved`(:204) + TLS(`tls.rs`,`tls_buf.rs`)가 다 있고, `State` 만
`Fresh/Connecting/Connected/Dead` 로 `Local` 이 없다.

#### 2단 훅 제안

1. **(호스트 전용, 게스트 RE 불필요 — 지금 바로 가능)**
   `vendor/wie/wie_wipi_java/src/classes/org/kwis/msf/io/` 에 `Socket` 과 `URL` 을 구현하고
   `lib.rs::get_protos()` 를 35 → 37 로 늘린다. 그 뒤를 hero2 가 이미 가진 `AndroidNetwork` 에 얹으면
   **hero2 도 hero3 와 같은 바이트스트림 seam 으로 내려온다.**
2. 그 다음 `State::Local` 을 이식하면 hero3 `offline.rs` 의 **설계**(프레임 단위 요청/응답 엔진,
   소켓 없음)는 재사용 가능하다. **프로토콜 표는 재사용 불가** — hero2 패킷은 1단계를 넣은 뒤
   `Socket` 어댑터에서 로그로 뽑아야 한다(hero3 의 `NETTRACE` 가 했던 일).

★ **이 작업은 `guest_anchors` 게이트와 무관하다.** 순수 호스트 측 자바 클래스 등록이므로
**CHAR 앵커보다 먼저 착수할 수 있다.**

#### 필요한 응답의 종류는 이미 다 안다

`t/nt.tdf` 24행을 전부 해독했다: 서버 접속 중 / 사용자 인증 / 접속 끊김 / 서버 처리 중 /
캐릭터 업데이트 중 / 게임 로딩중 / 서버 처리 완료 / **카드가 없습니다** / 네트워크 / 싱글 /
아이템이 없습니다 / **내리기** / **올리기** / 버리기 / 서버 처리 실패 / 돈이 부족합니다 /
금액을 설정하세요 / 빈공간이 없습니다 / **카드를 전송하시겠습니까** / 서버 장애 중입니다 / 공지 사항.
**구매·가격·결제 동사가 한 개도 없다.**
창고 UI 탭 구성은 `t/gm.tdf[60..66]`: `네트워크창고 / 돈 / 아이템 / 카드 / 올리기 / 내리기`.

→ **3탭(돈·아이템·카드) × 2동작(올리기·내리기) + 카드 전송 + 공지사항.** 프로토콜을 RE 하기
전에 로컬 응답기가 흉내내야 하는 의미론이 완전히 열거된다.

#### hero2 세이브는 DB 가 아니라 파일이다

hero2 세이브는 WIPI DB 가 아니라 `org.kwis.msp.io.File` → `AndroidFilesystem` 의
`g1`/`g2`/`g3` 다. → hero3 `database.rs` 훅과 `vault::slot_has_save`(DB 디렉터리 스캔)는
hero2 에서 **구조적으로 발화하지 않는다.** `savevault.rs` 는 재설계 대상.

#### 미결 위험

* `bv.run()` 널 점프의 1차 원인이 (a) `Socket` 클래스 미등록인지, (b) `Object.wait/notify` 등의
  `MethodAccessFlags::NATIVE` 불일치인지, (c) `jb_unk4/5/7/8` 스텁의 `Ok(0)` 인지 **미구별**.
  발생지 4곳(`interface.rs:162/277/301/311`)이 **완전히 같은 문자열**이라 로그로는 못 가른다.
  → 제 기기 로그가 단서를 준다: 크래시 직전 순서가 `jb_unk8/jb_unk7`(둘 다 `Ok(0)`) →
  `Network::connect()` → `disconnect()` → 널 점프. (c) 또는 (a) 를 시사한다.
* 게스트의 `([B)[B`(fo 0x716e8) / `(BB)[B`(fo 0x7157c) 변환이 암호인지 미확인.
  `P/g1~g3` 와 `P/n1~n3` 이 둘 다 6바이트 주기 구조를 보이므로 프레임도 암호화됐을 가능성.

### 4.5 캐시샵 — **hero2 원본에 없다** / 순번 8

#### 정적 실증 (confirmed)

* 게임 전체 텍스트 코퍼스(에셋 zip 1137 블롭 / 압축해제 2,626,356 B, 맵 111장 gz 전개 포함)를
  **5개 인코딩**(cp949/euc-kr/utf-8/utf-16le/utf-16be)으로 전수 검색.
  `캐시`·`캐쉬`·`결제`·`현금`·`과금`·`아이템샵`·`아이템몰`·`프리미엄`·`쿠폰`·`코인`·`포인트샵`·
  `구입하기`·`유료` 히트 **0**. (히트 34건은 전부 정보이용료/통화료/상점(맵이름)/할머니/선물/
  마력 충전 스펠카드의 오탐.)
* `t/*.tdf` 포맷 **완전 해독**: `[u8 count][u16 BE 길이][cp949 본문]×N`. 20개 파일 전부
  count 일치 + `end==filesize` 로 100% 커버, 총 **799 문자열**. 못 읽은 문자열이 남을 여지가 없다.
* `client.bin1120` 안 한글 문자열 **0개**(cp949 2바이트 런 스캔 210건은 전부 Thumb 코드 오탐).
  Java String 상수풀(게스트 `0x181392~0x181c20`, `[u16le 문자수][UTF-16LE]`) 안 한글은 **딱 10개**
  이고 전부 전투/인벤 UI(`아이템 소지 공간이`, `아이템이 부족합니다.`, `방어`, `공격`, `비용 `…).
  **가격·구매·결제 문자열이 하나도 없다.**
* **대조군 검증** — 같은 개발사·같은 시리즈인 hero3(캐시샵 실존)은 클라이언트에 문자열을 남긴다:
  `binary.mod` 안 `캐시`@0x5af6c, `유료`@0x5a3d4, `구매`@0x5a3e0, 평문 전개
  `본 게임은 … 유료 아이템 구매, 캐릭터 복구 이용시 …`, `과금중 에러 발생. 아이템 구입이
  취소되었습니다.`, `캐시 아이템 정보 요청중입니다.`, `실제 현금`, `구입하기`.
  → **캐시샵은 문자열 없이 존재할 수 없다**는 것이 같은 개발사 산출물로 확증된다. hero2 는 0건.
* 인게임 상점은 전부 **게임머니 상점**이다. `t/shop.tdf`(26행): `아이템 구매 / 아이템 판매 /
  무기·방어구·기타 / 구입하시겠습니까? / 돈이 부족합니다.` 화폐 단위는 `t/com.tdf[20] 소지금` +
  `t/itmtp.tdf[58] 가격`. 야수관리사(`bm.tdf`)·대장장이(`bsm.tdf`)·연금술사(`rf.tdf`)도 전부 `비용`.
  상점 맵 5곳(01 중앙 / 66 실바나르 / 69 노르덴항 / 73 신드라프 / 77 서리골짜기) 전부 일반 NPC 상점.

#### 기기 실증 — 진입점 전수 확인

| 진입점 | 내용 | 캐시샵? |
|---|---|---|
| 타이틀 캐러셀 | 이어하기 / 환경설정 / 도움말 / 네트워크창고 / 정보이용료 / 게임종료 | 없음 |
| `정보이용료` | `본 게임은 네트웍 데이터 사용량에 따라 기본통신료외 1패킷(512BYTE)당 **0원**의 추가 정보이용료가 부과됩니다.` | **고지문일 뿐** |
| CLR 메뉴 | 캐릭터 / INVENTORY / EQUIPMENT / SKILL / BEAST / QUEST / CARD / SYSTEM | 없음 |
| `SYSTEM` | 세이브 / 도움말 / 옵션 / 종료하기 | 없음 |
| `CARD` | 15칸 전투 카드 덱 + 보유수. 설명 예: `마력 충전 / 다음턴의 마력구 위력 상승` | **전투 카드.** "충전"은 마나 |

#### 과금 메커니즘의 정체

상수풀에 `BillSocket`(fo 0x08177c), `BillSocket://`(0x081b12), `TestBillSocket`(0x081982),
`TestBillSocket://`(0x081b3e), 테스트 MIN `01611111111`(0x081aa2).
쓰이는 곳은 캐시샵이 아니라 **체험판→정식판 1회 전환**이다 — `t/try.tdf` 12행:
`정식판 등록 / 정식판으로 전환하시 / 겠습니까? / (정보이용료 3000원) / 이후 무료로 게임을 /
즐길수 있습니다. / 정식판 인증 중입니다. / 체험판`. `__adf__` 와 정확히 일치(`Fee:3000`).

→ **과금 축은 (a) 3000원 정식판 1회 (b) 통신사 통화료. 인게임 소액결제 축이 없다.**

#### 함의

* hero3 `cash_catalog.rs` + `offline.rs` 캐시샵 서빙 경로는 **이식 대상이 아니다.** 게스트에
  그것을 소비할 UI·문자열·프로토콜이 없으므로 설계상 불가능하다.
* **계획서 §7 Phase 4-12 의 "캐시샵/공용창고" 는 "공용창고"만 남기고 개정해야 한다.**
* 넣고 싶다면 원본 재현이 아니라 **신규 기능 추가**이고, 게스트 문자열이 없으므로 호스트 HUD 에서
  그려야 한다. 원본 충실성 규율상 **사용자 확인이 선행돼야 한다.**

#### 부수 — hero2 서버 주소 확정

상수풀 실측: `://211.239.150.70:30002`(fo 0x081588, 게스트 0x181588),
`211.239.150.70`(0x08199e), `30000`(0x0819cc). 스킴은 HTTP 가 아니라 WIPI `BillSocket://`.
네트워크 API 표면은 `org/kwis/msf/io/{Socket,Network,URL}` 뿐 — HTTP 클라이언트도, 도메인도,
다른 IP 도 없다(정규식 전수: `http/https/socket://` 0건, 다른 IPv4 리터럴 0건).
→ **CLAUDE.md "남은 정리" 의 `network.rs` 리다이렉트 표(hanbit IP) 교체 대상이 이 값이다.**
오프라인 전용 빌드(커밋 `0108620`)의 "서버 주소를 굽지 않았음을 증명" 규율에는 영향 없다 —
이 주소는 **게스트 바이너리 안**에 있고 호스트 코드가 아니다.

#### help.tdf 의 광고문 주의

`help.tdf[14]` 는 `네트워크로 다른 플레이어와 대전 … 4인 대전 … 길드에 가입 … 길드전 …
장터를 통해 다른 플레이어와 아이템을 거래합니다` 라고 서술한다. 그러나 그 기능들의 UI 문자열
(대전/길드/장터)은 `nt.tdf` 24행에도, 상수풀에도, **어디에도 없다.** `__adf__` 의
"Semi Network" 는 실제 구현이 **창고 + 공지사항뿐**인 광고문에 가깝다.

### 4.6 프레임 최적화 — **`now`** / M / 순번 2

§3 이 본문이다. 요약하면:

* **hero3 의 축(게스트 명령 처리량)은 hero2 에서 무효.** blit HLE / `binary_patches.toml`
  hero2 엔트리 신설(계획서 Phase 3-9)은 **폐기**. 프로파일이 핫 게스트 루틴의 부재를 증명했다.
* **hero2 의 축은 호스트↔게스트 경계**(샘플 96.35%, 왕복당 명령 80개)와
  **4bpp 게이트**(세 빠른 경로 동시 정지)다.
* 45ms 페이싱 함정 수정은 이미 상속됨.
* 계측 인프라의 한계를 알고 시작할 것:
  * `wie_probe` 는 **프레임 페이싱을 원리적으로 측정 못 한다**(1틱/프레임 + 가상 시계,
    `main.rs:717,726-727,757`; `ProbePlatform::now` = 프레임 카운터).
  * `wie_probe` 는 **게스트 fps 와 동일프레임 스킵률을 셀 수 없다** — `stats::add_frame()`
    호출지가 저장소 전체에서 `platform.rs:267` 단 하나이고 `ProbeScreen::paint` 는 해시도
    카운트도 안 한다. → 이 두 지표는 **기기 logcat 전용**이다.
  * `wie_probe` 는 `frame_timing_take()`/`svc_mix_take()` 를 **출력하지 않는다**(데이터는 수집됨).
    → 계획서 §7 Phase0-4("hero2 타이머 주기 실측")는 probe 3줄 추가 또는 `RUST_LOG` 트레이스로만.
  * `wie_probe` 는 **clap 을 쓰지 않는다**(수동 `while` 파서, `main.rs:571-635`). `--help` 가 없고
    **오타난 플래그는 조용히 무시된다**(`_ => {}`). 기본값이 전부 hero3
    (`zip=d:/tmp/hero3_fixed.zip`, `w=240 h=320`)이므로 `--zip --w --h --out` 은 항상 명시.
  * `--profile 1` 의 `fo = pc - 0x100000`(`main.rs:1362`)이 **hero2 매핑과 정확히 일치**하므로
    프로파일 결과를 `client.bin1120` 오프셋으로 그대로 디스어셈에 넣을 수 있다.
* ⚠ **`wie_probe` 는 hero2 를 부팅하면서도 `GuestBuild::Ktf` 를 세팅한다**(`main.rs:698`).
  실행 로그 실측: `[probe] loaded (KTF) char_anchor=*0x1b3084 (fallback 0x1b7b6c)` — **hero3 값**이다.
  `--verify-offsets`/`--char-dump`/`--find-fb`/`--map-*` 는 무의미한 숫자를 내고,
  **`--poke`/`--host-save`/`--host-levelup` 은 hero3 주소로 실제 write/call 을 발사한다.**
  → hero2 에서 이 세 플래그는 **절대 금지**. 읽기 옵션만 쓸 것.
* 아직 공짜로 남은 카드 하나: **해시 없는 제네릭 컴파일러 라이브러리 훅 6개**가 hero2 에
  설치되는지 여부. 붙으면 memcpy/memset 이 호스트로 넘어간다. 런 한 번으로 판정된다.

### 4.7 발열 최적화 — **`now`** / M / 순번 4

**hero2 시드에 이미 있다** (57차 `4742313`). 기기 SET 모달에 `HEAT SAVER` 행이 있고 동작한다.

**HEAT SAVER 의 실제 동작.** 게스트 프레임버퍼를 반으로 줄이는 것이 **아니다** — 호스트 서피스만
줄인다. `render_div = if read_heat_saver {2} else {1}`(`app.rs:356`) →
`win_w/win_h = 물리크기 / render_div`(`app.rs:1718-1719`) → `Surface::resize` →
`ANativeWindow_setBuffersGeometry` → 컴포지터 하드웨어 업스케일. 그리고
`init_logical(win_w, win_h, …)` 은 **비율만** 쓰므로 양축을 반으로 줄여도 **게스트 프레임버퍼
크기는 불변**이다. → **"176×220 → 88×110" 은 일어나지 않는다.** 깨지는 것은 오버레이 선명도뿐이고
`config.rs:401-404` 가 그 대가를 명시한다.
기기 확인: `resized: 2220x1080 -> render 1110x540`.

**★ hero2 에서 HEAT SAVER 의 비중이 hero3 보다 크다.** 게스트 해석 비용이 사실상 사라지면
남는 전력 소비는 (1) 목적지 픽셀 트래픽(게임 블릿 2.4M px + softbuffer present 스위즐 2.4M px
= 프레임당 ~29MB), (2) 오버레이의 픽셀당 sqrt/나눗셈 AA 블렌딩, (3) 무음에서도 코어의 ~13%를
먹는 oxisynth 리버브/코러스뿐이다. **이 셋 전부가 HEAT SAVER 한 스위치로 1/4~라이트가 된다.**
→ 7번의 1순위는 새 최적화가 아니라 **HEAT SAVER 를 기본 ON 후보로 올려 실측하는 것**이다.

**동일 프레임 스킵은 영구 정지.** §3.4 — 4bpp 게이트. 기기 실측 `skipped 0/s`(정적 타이틀에서도 0).
이걸 되살리는 것이 7번의 최대 단일 항목이고 **게스트 RE 불필요**다.

**라이트 신스.** `platform.rs:50-58` → `lite = read_audio_lite || read_heat_saver` →
`audio_sink.rs:228-256 build_synth(lite)` 가 `chorus_active/reverb_active = !lite`,
`interpolation = Linear`. hero2 사운드 32개가 전부 SMAF(`MMMD` 매직, `P/snd/02..53`)이므로
`parse_smaf` → `midi_note_on/off` → oxisynth 를 타고 **hero2 에도 그대로 해당**한다.
단 오디오 상수는 hero3 튜닝값이다: `SYNTH_GAIN=1.0` 의 근거가 hero3 스코어 피크,
`SFX_TRIM=0.4` 의 근거가 hero3 효과음 14개 실측(`audio_mix.rs:42-61`).
hero2 의 32개 SMAF 피크는 미측정 → 클리핑/밸런스 **재측정 대상**(성능이 아니라 품질 항목).

**⚠ adb A/B 를 막는 것.** `config.rs` 의 sdcard 탈출구 경로 **7곳**(:322 audio_lite, :345 profile,
:373 pad_hud, :408 heat_saver, :437 gamepad, :473 blit_hle, :515 frame_timer_cap)이
`/sdcard/Android/data/**com.hero3remake.app**/files/...` 로 하드코딩돼 있다.
`applicationId` 는 `com.hero2remake.app`(`build.gradle.kts:64`)이므로 **adb 우회가 안 되고,
hero3 가 함께 설치된 기기에서는 hero3 의 설정을 읽는다.** 이 기계의 S9 가 정확히 그 상태다.
(1차 경로는 앱 내부 dir 이라 인앱 토글은 정상 동작한다 — 기기 확인:
`/data/data/com.hero2remake.app/files/wie_heat_saver.txt`.)

### 4.8 컷신 상태 게임 버튼 hidden 정책 — `after-char-anchor` / M / 순번 5

#### hero3 의 판별 = 게스트 메모리 1바이트

호스트 휴리스틱이 **아니다**. `cutscene_open`(`items.rs:652-655`) = `window_id(read) == Some(WIN_SCRIPT)`,
`WIN_SCRIPT = 4`, `window_id`(`items.rs:605-608`) = `rd_u8(read, resolve_char() + 0xa0c0)`.
즉 **CHAR+0xa0c0 의 int8 == 4**. 픽셀 분석·키 이력·타이머 전부 아니다.

그 바이트의 정체(`items.rs:74-82` 주석): 윈도우 매니저의 current window id, 유일한 writer 는
`openWindow`(게스트 0x12c6a4, store at 0x12ca7c). id 표: **3=창없음(필드), 4=script/컷신,
7·17·18=확인창, 8=맵, 14=세이브, 15·16=인벤/장비.**

두 번째 축: `script_choice_open`(`items.rs:679-687`) = `window_id==4 && rd_u8(CHAR+0xa0dc)==1`.
발견 경로가 문서화돼 있다 — `items::flag_window_changes`(CHAR 43KB 전체 변경 감시)로 기기
3전환 포착: 대화열림 `0xa0c0 3→4` + `0xa0dc 1→0`, 선택지등장 `0xa0dc 0→1` + 행테이블
`0xa16c..0xa187` 채워짐, 선택지닫힘 `0xa0c0 4→3`.

#### 오분류 계보 — `app.rs:499` 주석은 버그 기록이 아니라 진단표다

`git log -S "misclassified as the SCRIPT window"` → 커밋 **577bcd8** 하나(2026-08-05, 진단 계측).
`mode=Cutscene` 항목은 "만약 이 값이 나오면 메뉴가 SCRIPT 창으로 오분류된 것"이라는 **가설 분기**이며
실제 관측 기록은 없다.

실제 오분류는 다른 것이었다(`git log -S "Cutscene" -- overlay.rs`, 6커밋):
54차(`19fa6a7`)가 창 id 4 에서 화살표를 통째로 없애 **게임 자체 퀘스트 선택지를 못 고르게 됐다**
(플레이어: "선택지를 못 골랐다"). 원인은 **창 id 4 가 스토리 내레이션과 NPC 대화를 동시에 뜻하기
때문**(기기 확정: 안내원 퀘스트 대화가 프롤로그와 똑같이 `win=4 state=4`).
즉 오분류는 "메뉴→컷신"이 아니라 **"대화→컷신"** 이었고, 해법은 네 번째 Mode 가 아니라
별도 bool `choice` 한 개였다(55차-C `a7b46ba`).

#### Cutscene 모드가 정확히 무엇을 숨기고 남기는가 (hero2 줄번호, 로직 동일)

**숨김** ① OK 액션 디스크(`overlay.rs:1113`) ② CLR 취소(`:1134`) ③ `1/3/7/9/#` 메뉴키 블록
④ 방향 화살표 — 단 `choice` 면 부활(`:1209-1213`) ⑤ 퀵바 플레이트(`:1544` 빈 Vec)
⑥ 기어+키패드 토글(`:1324`) ⑦ 좌상단 게이지 블록(`app.rs:543-550 stats_hud_on`)
⑧ 조이스틱 존 자체(`:1295`) ⑨ 필드 전용 세컨더리(7)·미니맵 패널.

**남김** **반투명 `#` 스킵 하나**(`:1240-1248`, CANCEL 좌석 재사용, `Face::Skip`) +
**화면 전체가 OK**(`:2795 tap_anywhere_key(mode) = (mode==Cutscene).then_some(KeyCode::OK)` —
rect 가 아니라 히트테스트 체인 마지막 fallback).

사용자 요청 원문(`overlay.rs:1234-1239`, 2026-08-16):
> 컷신 일때는 화살표 혹은 조이스틱과 확인버튼이 노출되면 안돼고 #이 조금 더 투명해져야해

`placement.rs:184-190 ids_for`: `Mode::Menu | Mode::Cutscene => &MENU_IDS` — 저장 배열이
field[]/menu[] 두 개뿐이라 Cutscene 에 세 번째 배열을 주면 **아무도 쓰지 않는 死데이터**가 된다.
장애물 집합도 같은 이유로 `!is_field(mode)` 로 묶었다(`placement.rs:665-668`).

**hero2 에 없는 것**: 289c9fd 의 **연속표본 히스테리시스**
(`cutscene_hold(gone, script_open, exit_polls)` + `CUTSCENE_EXIT_POLLS=3`,
`choice_hold` + `CHOICE_CONFIRM_POLLS=2`). 이유: 대화 페이지 넘김이 `0xa0c0` 을 한두 폴 동안
3으로 떨어뜨려 mode 가 Cutscene→Menu→Cutscene 로 튀며 버튼이 깜빡였다(플레이어 리포트 2종).
`git log -S cutscene_hold` = **289c9fd 단 하나** → hero2 시드에 없다.

#### ★★ hero2 는 게스트 RE 0줄로 컷신을 가려낼 수 있다 (probe 실측)

hero2 의 스크립트/대화 창은 **프레임버퍼 한 행이 단색으로 가득 차는 가로 괘선**을 만든다.

> **판별식(색·해상도 무관)**: 프레임버퍼 **하단 절반**의 어떤 행 y 에서, 그 행의 최빈색이
> 폭의 **90% 이상**을 차지하고 그 색이 검정이 아니면 → 스크립트 창.

실측 **62프레임 × 4해상도에서 오탐 0, 미탐 0**. 스크립트 창 프레임은 3~6개 행이 걸리고
(176×220 → y=181/182/183; 440×220 → 181/182/183 + 214/215/216; 400×240 → 201/202/203 + 234…;
240×320 → 254/255/256), **그 외 전부 0개** — 타이틀·스플래시·슬롯선택·필드·선택지패널·CLR 메뉴·
STATUS·맵창(`*`)·퀵슬롯 페이지(`#`)·아이템사용(`1`).

**hero2 만의 이점**: 176×220 에서 대화가 열리면 게임 자체 하단 HUD 가 **통째로 사라지고**
전폭 금테 텍스트박스 + 우하단 `# SKIP` 로 교체된다. 반면 **선택지 패널은 화면 중앙 모달이고
하단 HUD 가 그대로 남는다.** → hero3 가 `0xa0dc` 로 겨우 갈랐던 "컷신 vs 선택지"를
**hero2 는 화면 자체가 구분해 준다.** 선택지는 화살표와 OK 가 필요한 상태이므로
`Mode::Menu` 로 두는 것이 정답이고, **hero3 의 `choice` 특례가 hero2 에서는 아예 필요 없다.**

신호는 깜빡이지 않는다: 5프레임 간격 덤프 620→850 에서 0(620) → 173/176(625..725 연속 14샘플)
→ 0(730 이후). hero3 가 히스테리시스를 붙여야 했던 페이지넘김 blip 이 프레임버퍼에는 없다
(박스가 매 프레임 다시 그려지므로).

**구현 규율 3개 (모두 실측 근거)**
1. **고정 행 오프셋 금지.** 220/240 높이에서는 `y = h-38` 이었지만 240×320 에서는 y=254
   (`h-38`=282 이 아니다) — 세로가 긴 화면에서는 게임이 하단 HUD 를 유지한 채 대화박스를
   그 위에 그린다. 반드시 "하단 절반 전체 스캔".
2. **절대 색상 상수 하드코딩 금지.** 프로브 덤프의 금색은 (159,127,63)이지만 이건
   `image.colors()` 산출물이고, 앱의 `platform.rs:189-202 EXPAND5/EXPAND6` 는 5비트 19 → **156**
   을 낸다. 실제로 덤프 79색 중 75색이 565 라운드트립에 실패하고 `(v<<3)|7` 패턴에 맞는다
   (= §3.4 의 4bpp 문제와 같은 원인). 그래서 판별식은 색을 특정하지 않고 최빈색 점유율만 쓴다.
3. **히스테리시스는 그대로 붙여라.** 실측에서 깜빡임이 없었지만 그건 신드라프 상점 대화 한
   장면의 표본이다. hero3 의 289c9fd 가 "벽시계 데드라인 + field 기반 fast cancel"이 실패한
   이유까지 기록해 뒀다 — **cancel 을 없애고 연속 폴 카운트만** 쓸 것.

**훅 지점.** 프레임은 워커 `platform.rs:313 UserEvent::Paint(data)`(0xAARRGGBB `Vec<u32>`)로
UI 스레드에 오고 `app.rs:1801-1810` 에서 `self.last_frame` 에 들어간다. 치수는
`crate::logical()`(`app.rs:678`). 비용 = 하단 절반 스캔 = 440×220 기준 약 48K 비교/프레임
(행별 조기이탈 가능). 판정 결과를 `self.mode = Cutscene` 로 넣으면 `relayout()` 이 이미
mode 변화를 처리한다(`app.rs:1886-1900`).

#### ★ hero2 정책 충돌 2건 — hero3 정책을 그대로 옮기면 안 된다

1. **`#`**: hero3 는 컷신에서 반투명 `#` 스킵 디스크를 남긴다. 그런데 hero2 의 `#` 는
   **게임 자체 하단 HUD 의 퀵슬롯 페이지 키**다. 컷신에서 `#` 가 실제로 스킵인지 미확인 —
   (다만 §4.8 의 176×220 실측에서 대화 중 우하단에 `# SKIP` 표기가 관찰됐다 → 스킵일 가능성이
   높지만 확정 아님.) 스킵이 아니면 `Face::Skip` 디스크는 무의미하거나 유해하다.
2. **`CLR`**: hero3 정책은 컷신에서 CLR 을 지운다(`overlay.rs:1134`). 그런데 계획서 §9.1 이
   실측한 hero2 동작은 **"대화창이 떠 있으면 첫 CLR 이 그걸 닫는다"** 다.
   → 정책을 그대로 옮기면 **hero2 에서 유일하게 검증된 대화창 닫기 키를 없앤다.**

#### 미검증 위험 3개

(a) 진짜 스토리 컷신(박스 없는 전체화면 연출)이면 판별식이 미탐한다 — 표본이 NPC 대화 1종뿐.
(b) 전투 중 HUD/박스 거동 미확인.
(c) 상점·네트워크창고 UI 가 전폭 단색 괘선을 그리면 오탐한다.

---

## 5. 의존 그래프

```
                  ┌──────────────────────────────────────────────────┐
                  │ ★ CHAR 앵커 + 3필드                              │
                  │   state(u32) / window(u8) / selector(u8)         │
                  │   = items::resolve_char() 가 요구하는 것          │
                  └────────────────────┬─────────────────────────────┘
                                       │ 없으면 전부 None → mode=Menu 고착
        ┌──────────────┬───────────────┼───────────────┬──────────────┐
        ▼              ▼               ▼               ▼              ▼
   8 컷신정책      2 게임HUD 내용    1 퀵슬롯 표시   3 L3 빠른저장   2 미니맵
   (window==4)     read_stats       scan_quickbar   can_save       minimap_render
        │              │               │
        │(우회로 있음)   │               ▼
        │              │        좌석↔키 표 hero2화 ← ✅ 완료 2026-09-01
        │              │        (5칸 바, `#`=회복아이템 교체, 페이지는 호스트)
        │              ▼               │
        │        아이콘 아틀라스        └─→ 3 R3 퀵바페이지는 앵커가 필요하다
        │        (mph/mpd 디코더 ✅ 2026-09-01)
        │
        └─→ ★ 프레임버퍼 판별식 (실측 완료, 게스트 RE 0줄) — 8번을 앵커 없이 살린다

앵커와 무관 (지금 바로 가능):
   6 프레임 최적화 ─┬─ KTF Java 인터페이스 메모 캐시 (§3.3)  ← hero2 최대 카드
                    ├─ 4bpp 게이트 해소 (§3.4)              ← 세 경로 동시 회복
                    └─ 스텁 로그 강등 (§3.6)                ← %sys 65→17% 실측
   7 발열 최적화 ───┬─ HEAT SAVER 기본 ON 후보 실측
                    ├─ 동일프레임 스킵 부활 (= 4bpp 게이트와 같은 작업)
                    └─ config.rs sdcard 경로 7곳 hero2 화
   4 오프라인 창고 ─┬─ wie_wipi_java 에 Socket/URL 구현      ← 앵커 무관, 지금 가능
                    └─ 그 다음 State::Local + 프로토콜 RE
   5 캐시샵 ────────  원본에 없음. 신규 제작 여부는 제품 결정 → 최후순위
```

---

## 6. 권장 실행 순서 — 세션 단위

순번의 근거: **(a) 사용자가 눈으로 확인할 수 있는 결과가 나오는가, (b) 선행조건이 없는가,
(c) 나중에 하면 재작업이 생기는가.** 세 기준이 충돌할 때는 (b) → (c) → (a) 로 우선했다.

### S1 — 계측·성능 기반 (선행조건 0, 즉시 성과)

목표: §3 의 세 카드를 실행하고 A/B 로 수치를 남긴다.
1. 스텁 로그 강등 (`java_check_type` / `jb_unk*` / `setAlpha` → `trace!` 또는 샘플링).
   **요약 게이지는 `warn!` 로 남긴다** — 안 그러면 측정 수단을 잃는다(§3.6 의 한계 그대로).
2. **4bpp 게이트 해소** — hero2 화면 이미지가 왜 4bpp 인지 찾고(WIPI-Java lcdui 경로),
   565 로 만들 수 있으면 세 경로(스킵·빠른 변환·블리터)가 한 번에 돌아온다.
   못 만들면 4bpp 용 해시 + 빠른 변환 경로를 추가한다.
3. `config.rs` sdcard 경로 7곳을 `com.hero2remake.app` 으로.
산출물: fps A/B 표, `emu paint: skipped` 가 0 이 아닌 로그, 발열 A/B.
후퇴선: 2번이 막히면 1·3 만으로도 세션이 성립한다.

### S2 — CHAR 앵커 (§9. 실패 판정 기준이 명확한 실험 3개)

목표: `field=true` 를 로그에 띄우고 **필드 HUD 를 화면에 등장시킨다.**
산출물: `quickbar probe: char=0x… field=true` + STICK/Q1~Q5/MAP 패널이 보이는 스크린샷.
후퇴선: 실패하면 그 세션 안에서 S3 로 전환한다(§9 의 각 단계에 실패 판정이 붙어 있다).

### S3 — 퀵슬롯 키표 + 게임패드 레이블 (RE 불필요)

⛔ **이 절의 원래 목표 3개가 전부 틀렸다.** "`1 3 7 9 0`" 은 `0` 을 슬롯으로 오해한 것이고,
"`QuickPage` 를 `#` 전송으로" 는 `#` 를 페이지로 오해한 것이며(§2②), "`quickpage.rs` 폐기" 는
사용자 결정으로 뒤집혔다 — 페이지는 **호스트가 소유**한다.

✅ **2026-09-01 완료** (순서표 #3, S2 를 기다리지 않고 먼저 했다 — 앵커를 여는 순간 틀린 키가
나가므로 순서상 앞서야 한다). 실제로 한 것: `QUICK_SLOTS = 5` / `GUEST_QUICK_SLOTS = 4` 분리,
`quickslot_hotkey` = `1 3 7 9`(+슬롯4 호스트), `CtrlId::Heal`/`HealSwap` 신설, `QuickPage` 유지,
패드 기본표·레이블 hero2 화. **`CtrlId::Secondary` 는 재정의가 아니라 삭제**했다 — hero2 에
보조무기는 키도 장비 슬롯도 없다(`t/shop.tdf[4..16]`). 좌석 기하만 `overlay::disc_side_button`
으로 남아 회복 아이템 `0` 이 그 자리를 물려받았다. `Minimap` 좌석 재정의는 **보류**.
산출물(기기 확인)은 **필드 HUD 가 떠야 가능하므로 S2 이후**다.

### S4 — 컷신 정책 (프레임버퍼 판별식)

목표: §4.8 의 판별식 구현 + 히스테리시스 + hero2 정책 충돌 2건 해소(`#`/`CLR`).
**S2 가 성공했으면 앵커 경로로, 실패했으면 프레임버퍼 경로로** — 어느 쪽이든 이 세션은 성립한다.
산출물: 대화창에서 버튼이 접히고 반투명 스킵만 남는 것을 기기에서 확인.
선행: §4.8 "미검증 위험 3개"를 probe 로 먼저 닫는다.

### S5 — 게임 HUD 내용 (앵커 진도순)

STATS 게이지 → 퀵바 아이콘(아틀라스 신규) → 쿨다운 → 미니맵. 배치는 이미 완료(`7a92d2d`).
`hud_bottom_patch` hero2 대응이 없으면 게임 HUD 와 이중 표시되므로 그 RE 가 이 세션에 포함된다.

### S6 — 발열 (HEAT SAVER 기본값 결정)

S1 의 4bpp 작업이 끝난 뒤여야 의미가 있다(스킵이 살아나면 발열 그림이 바뀐다).
hero2 SMAF 32개 피크 측정 → `SYNTH_GAIN`/`SFX_TRIM` 재튜닝(품질 항목).

### S7 — 오프라인 공용창고

1단계(`Socket`/`URL` 구현)는 **앵커 무관이라 언제든 가능**하다. 다만 프로토콜 RE 가 XL 이고
게스트가 실패를 우아하게 처리하므로(§4.4) 급하지 않다. 1단계만 먼저 넣어 `NETTRACE` 로
패킷을 뽑아 두면 이후 세션이 쉬워진다.

### S8 — 캐시샵

**원본에 없으므로 "구현"이 아니라 "신규 제작 여부" 결정이 먼저다.** 사용자 확인 필요.

### 반영하지 않은 반대 의견

* "1번 퀵슬롯을 S1 로" — 필드 HUD 가 화면에 없으면 눌러서 확인할 수 없다. S2 뒤로 미뤘다.
* "4번을 S1 로 (앵커 무관이니까)" — 맞지만 프로토콜 RE 가 XL 이고 게스트가 우아하게 실패하므로
  체감 이득이 가장 낮다. 1단계만 S7 에 두었다.
* "S2(앵커)를 1순위로" — §9 로 난이도가 크게 내려갔지만 **여전히 실패 가능한 유일한 세션**이다.
  실패해도 잃지 않도록 S1(선행조건 0)을 앞에 두었다.

---

## 7. hero2 는 WIPI-C 가 아니다 — AOT 컴파일된 Java MIDlet

계획서 §1.2 는 `client.bin1120` 을 "Thumb ARM, **WIPI-C**"라고, "자바 클래스 없음"이라고 적었다.
**전자는 오기다.** (CLAUDE.md 의 "진짜 자바 MIDlet(ARM 컴파일)" 이 정확한 서술이다.)

근거:
* `01035D0B.jar` 는 878 멤버 중 **`.class` 0개** (실측). `rpg/` 디렉터리는 비어 있고 루트에
  `client.bin1120` 하나뿐. 그런데 `__adf__` 에 `MClass:rpg.RPGHero2` 가 있다.
* `client.bin1120` fo 463367(게스트 `0x171207`)에 `rpg/RPGHero2` 문자열이 있고, 그 앞
  463151~463310 이 **게스트 자체 클래스명 풀**이다 — `au av aw ay az ba bb … bu bv bw bx by bz
  ca cb …`(난독화 2글자 이름 40여 개) + `[Laa;` `[Lch;` `[Lbi;` 배열 디스크립터.
  **`bv` = fo 463251 = 게스트 `0x171193`.**
* fo 0x071000~0x0747a0 에 JavaFullName 문자열이 평문으로 있다
  (`java/lang/Thread`@0x7477e, `(Lorg/kwis/msp/lcdui/Graphics;IIII[BIII)V+a`@0x712c3 …).
  이름 레코드 포맷 = `tag(1B) + descriptor + '+' + name + NUL`(`jvm_support/name.rs:19-27`).
* **Java 클래스 테이블 = fo 0x092238..0x093144, stride 0x24**(`JavaClassDescriptor` 36 B).
  **108 클래스 / 878 필드정의 / 3725 메서드정의**의 이름·디스크립터·오프셋·본문주소가
  이미지 안에 그대로 있다.
* 런타임 헬퍼 클러스터 = fo 0x67000~0x6E200. §3.2 의 프로파일 상위 30개가 정확히 이 구간이다.

**함의가 크다.** hero3 의 "C 구조체 CHAR + `CHAR+imm`" 모델이 hero2 에 그대로 적용되지 않는다.
hero2 의 **모든** 필드 접근이 `*(obj+0) + off + 4` 이중역참조다
(`jvm_support/class_instance.rs` `field_address = raw.ptr_fields + offset + 4`;
static 필드 저장 주소 = 필드정의 + 12, `field.rs`). 즉 hero3 의 `minimap_render.rs` 체인 오프셋
패턴이 오히려 hero2 의 **기본형**이다.

`guest_anchors.rs` 의 hero2 항목은 hero3 처럼 단일 베이스가 아니라 **두 층**으로 표현해야 한다:
**(a) 이미지 상수 앵커** = static 필드 슬롯의 게스트 주소, **(b) 런타임 체인** = `*(slot)` →
`*(obj)+off+4`. 3축 게이트는 그대로 유효하되 **"코드 패치" 축의 비중은 낮다**
(패치 대상이 AOT Java 본문이라 (class,member) 심볼 해석 없이 손대면 안 된다).

---

## 8. hero2 RE 좌표계 — 확정값

| 항목 | 값 | 근거 |
|---|---|---|
| 게스트 주소 | 파일오프셋 + `0x100000` | 기존 확정. `wie_probe --profile` 의 `fo = pc - 0x100000` 과 일치 |
| 이미지 범위 | `0x100000..0x196434` | 파일 614,356 B + bss 1120 B |
| **RELOC_DELTA** | **`0xbf10`** | 저장 포인터 W → fo = W + 0xbf10. NUL종단 문자열 시작점 312개 히스토그램에서 **310/312**, 차순위 후보 전부 97/312 |
| **정적 베이스 `sl`** | **게스트 `0x195CA4`** (fo 0x95CA4) | 두 함수에서 독립 확인: fo 0x0359d8 lit 0x000602c2 + `add sl,pc`(pc=0x1359e2); fo 0x013240 lit 0x00082a5a + `add sl,pc`(pc=0x11324a) |
| GOT 성격 테이블 | fo 0x95CA4..0x95FD0 (0x32c B) | sl 직후 |
| bss | 게스트 `0x195FD4..0x196434` (1120 B) | 파일명 `client.bin1120` → `wie_ktf/src/adf.rs parse_bss_size` |
| Java 클래스 테이블 | fo 0x092238..0x093144, stride 0x24 | 108 클래스 |
| Java 문자열 상수풀 | 게스트 `0x181392~0x181c20` (`[u16le 문자수][UTF-16LE]`, ~107 엔트리) | 리소스 경로 · PNG 청크명 · 서버주소 · BillSocket 전부 여기 |
| 심볼릭 상수 풀 | fo 0x7b1a8 대역 + fo 0x81c00..0x81d40, 레코드 `(u16 class_idx, u16 member_idx)` | GOT 슬롯 sl+0xd0/0xd4/0xd8/0xdc 가 이 레코드들을 가리킨다 |
| 서버 | `211.239.150.70:30002` / `:30000`, 스킴 `BillSocket://` | 상수풀 fo 0x081588 / 0x08199e / 0x0819cc / 0x08177c |
| `t/*.tdf` 포맷 | `[u8 count][u16 BE 길이][cp949 본문]×N` | 20파일 전부 `end==filesize` 로 100% 커버, 799 문자열 |
| hero3 대조본 | `client.bin64000` 735,760 B, `dist/hero3-v0.1.0-vc11.apk` → `hero3_fixed.zip` → `0103EFD4.jar` | 현재 assets 의 `hero3_fixed.zip` 은 LGT(`binary.mod`)라 KTF 대조는 apk 에서 꺼내야 한다 |

⚠ **hero3 도구를 그대로 쓰면 조용히 틀린 답을 낸다.** `tools_re/h3dis*.py`·`find*.py`·
`scan_off.py`·`solve_base.py`·`verify_delta.py` 는 `d:\tmp\client.bin` 하드코딩 +
hero3 상수(`RELOC_DELTA 0xeec`, `SL_FO 0xb2c40`)다. `h3dis*` 계열은 3상수만 바꾸면 쓸 수 있지만
`annotate()` 의 `[sl,#imm]` 해석은 의미가 다르다(hero2 는 `ldr rX,[pc]; add rX,sl; ldr rX,[rX]`
= GOT 1단 역참조이므로 한 번 더 따라가야 한다). `lgtdis.py`/`lgt_statics_xref.py` 는
ELF/절대주소 전용이라 **hero2 부적합**. `scan_off.py` 의 `CHAR+imm` 관용구 스캔도 부적합
(hero2 필드접근은 imm 이 아니라 (class,member) 심볼릭 ref 를 헬퍼에 넘긴다).

---

## 9. CHAR 앵커 확보 계획 — 난제에서 실험으로  ✅ **완료 2026-09-09**

> ★★★ **닫힌 항목.** 결과 정본 = **`HERO2_GUEST_RE.md`**. 이 절은 착수 계획으로서 대부분
> 옳았고, 어긋난 지점은 `HERO2_NEXT_SESSION.md` 「#11·#12 완료 기록」에 정리돼 있다:
> P0-a 는 불필요했고, §9.1 의 후보 체인은 진입점만 맞았고(능력치는 `bn` 배열이 아니라 클래스
> `a` 의 평문 int 필드), 진짜 장벽은 주소가 아니라 **인스턴스 레이아웃 규칙**이었다.
> ⛔ 그리고 **§9.3 의 상태워드 값 서술은 반증됐다** — 아래 §9.3 의 ⛔ 블록 참조.


계획서 §9.2 는 "값검색 실패 / 이동차분 실패"를 기록했다. **왜 실패했는지가 이제 설명된다**:
hero2 의 캐릭터 레코드는 평문 정수 구조체가 아니라 **Java 객체 + `short[]`/`byte[]` 배열들**이고,
필드 하나를 읽는 데도 이중역참조를 거친다. 인접 u16/u32 쌍 검색이 (709,780) 을 못 찾은 건
두 값이 애초에 한 구조체 안 인접 워드가 아니기 때문일 가능성이 크다.

**그리고 계획서 §9.3-① (STATUS 화면 문자열 역추적)은 실측 음성으로 폐기다.**
`client.bin1120` 안에 EUC-KR `힘/민첩/체력/의지/지력` **0건**, ASCII
`CRITICAL/DODGE/ATK/EXP/STATUS/INVENTORY/EQUIPMENT/SKILL/BEAST/QUEST/CARD/SYSTEM` **전부 0건**,
파일명 `.tdf/mph/mpd/hl2List/client` **전부 0건**. 라벨은 **에셋 안**에 있다 —
`t/gm.tdf[5..12]` = STATUS/INVENTORY/…/SYSTEM, `[13..20]` = 그 한글판, `[76]` = 경험치;
**`t/hero.tdf[18..22]` = 힘/민첩/체력/의지/지력**, `[0..3]` = 클레르/버서커/나이트/로그.

**대신 훨씬 좋은 조건이 확보됐다.** hero3 는 심볼 0개의 C 바이너리를 크래시 덤프로 뚫었지만
hero2 는 클래스 108개·필드 878개·메서드 3725개의 이름·오프셋·본문주소가 이미지에 그대로 있다.
즉 "CHAR 앵커를 찾는다"가 **"어느 Java 객체가 캐릭터인지 확인한다"** 로 축소된다.

### 9.1 1순위 후보 체인

```
클래스 ag 의 static  La; a   (슬롯 게스트 0x193b5c)
   └→ 클래스 a  (fields_size 0xf4, 인스턴스필드 61개, 메서드 116개)
        └→ +0xd8  Lbn; a
             └→ 클래스 bn (fields_size 0x50)
                  ├ [S a @+0x14   ← 수치 배열 후보 (cur)
                  ├ [S b @+0x18   ← 수치 배열 후보 (max)
                  ├ S c @+0x0, S d @+0x20, I a @+0x24, I b @+0x28, B a @+0x2c …
                  └ [B i/j/k/l/m @+0x4/0x8/0xc/0x10/0x1c
```

주소식: 인스턴스 필드 = `*(obj+0) + off + 4`, static 필드 저장 = 필드정의 + 12.
static 슬롯 320개 중 **11개는 이미지 리터럴 풀에서 직접 참조된다**(예: lit@fo 0xe080 →
슬롯 fo 0x93b7c = `ag.a:Lbq;`) → 재배치 후 고정 게스트주소로 읽고 쓸 수 있는 진짜 앵커가 있다.

**세이브 매니저는 클래스 `ag` 다.** 직렬화/역직렬화 6+1함수의 정확한 주소:
`ag.a(B)[B` @게스트 0x112be5(**슬롯번호 인자**), `ag.b()[B` @0x113235, `ag.a([B)V` @0x1135c1,
`ag.c()[B` @0x113939, `ag.b([B)V` @0x113c65, `ag.a()[B`(static) @0x114c95, `ag.c([B)V` @0x114d7d.
(메서드 body 주소는 **Thumb 비트가 켜진 홀수값** — 디스어셈은 `&~1` 로.)

### 9.2 실행 계획 — 각 단계에 실패 판정이 붙어 있다

**P0-a (1시간, 기기 불필요) — 재배치 이미지로 델타 산수를 없앤다.**
`cargo build --release -p wie_ktf_dump` 후 `wie_ktf_dump <hero2_ktf.zip> out.bin`.
(`vendor/wie/wie_ktf_dump` 는 `core.load(...)` 후 `run_function(IMAGE_BASE+1,[bss_size])` 로
게스트 자기재배치를 돌린 뒤 이미지 전체를 읽는다 → 모든 포인터가 절대 게스트주소가 된다.)
성공판정: out.bin 의 fo 0x92ebc 워드가 `0x171207`(= `rpg/RPGHero2` 의 게스트주소)로 바뀌어 있다.
실패판정: 그 워드가 `0x652f7` 그대로면 자기재배치가 그 영역을 안 건드린다는 뜻 → 검증된
`RELOC_DELTA 0xbf10` 을 계속 쓴다(손실 없음).

**P0-b (30분, 기기 불필요) — CHAR 체인 런타임 실증.**
```
wie_probe --zip android_app/app/src/main/assets/hero2_ktf.zip --w 176 --h 220 --ms 20 \
  --frames 1000 --keys 320:OK,420:OK,520:OK,620:OK,720:OK,780:CLR,810:CLR,880:OK \
  --dump 960 --dump-mem 0x193a00:0x200,0x1945a0:0x100,0x195ca4:0x790 --out dumpA
```
성공판정: `mem_00960_0x193a00.bin` 의 오프셋 0x15c(= 게스트 0x193b5c)가 힙 포인터
(`0x40000000..0x50000000`)다. 그 P 에 대해 `*(*(P)+4+0xd8)` = `bn` 인스턴스,
`*(*(bn)+4+0x14)` = `[S a` 핸들.
실패판정: 0 이면 static 저장이 "필드정의+12"가 아니라 GOT→bss 라는 뜻 → `--dump-mem
0x195fd4:0x460`(bss 전체)로 갈아타고 sl+0x24/0x44 가 가리키는 fo 0x960f0·0x961b0 을 본다.

**P0-c (30분) — 14개 실측값을 배열에서 찾는다.**
번들 세이브의 STATUS 실측값(LV 48, EXP 195/112128, HP 709/780, SP 390/390, ATK 138, DEF 115,
CRIT 43%, DODGE 35%, 힘 56, 민첩 118, 체력 17, 의지 17, 지력 10)을 `[S a`/`[S b` 배열에서 찾는다.
성공판정: 한 배열 안에 `(56,118,17,17,10)` 이 연속으로 있거나 `(709,390)` 이 같은 배열의 같은
인덱스대에 있다 → **CHAR 앵커 확정, §9.2 종료.**
실패판정: 없으면 후보를 `a` 인스턴스의 61개 필드로 넓히고, 그래도 없으면 `bf`(0x54)·`cq`(0x60)·
`bd`(0x48)·`n`(0x40)을 같은 방법으로.

**P1-a (2~3시간) — 세이브 인코딩 = `ag` 직렬화 6함수 역어셈.**
통계로는 여기까지 왔다: 자기상관 lag6 = g1 18.6% / g2 20.6% / g3 20.2%(lag12·18·24 는 배음,
그 외 lag 는 0.0~1.0%), 잔여류별 최빈 바이트가 세 파일 모두 순환열 **`10 05 47 18 3b 2b`**
(g1·g2 위상 0, g3 위상 4). 이걸로 XOR 하면 0바이트가 21~29% 로 늘지만 **두 번째 반복 6바이트
블록**이 남는다 → 단일 XOR 키가 아니라 평문에 반복 6바이트 레코드가 있는 구조(또는 48비트
주기 비트팩)다. 공통 헤더: 오프셋 0..1 = `00 06`, 7..9 = `d5 00 2d`. 길이 889/447/960
(g3 = 160×6 정확히). → **통계는 여기서 막히고 역어셈이 정답 경로다.**

**P1-b (2시간) — 심볼릭 상수 풀 해독 = hero2 RE 의 마스터키.**
fo 0x7b1a8 / 0x81c00.. 의 `(u16 class_idx, u16 member_idx)` → 클래스디스크립터/필드정의 인덱스
테이블을 찾는다. 단서: fo 0x82000~0x82300 에 stride 0x2c 로 외부 클래스명 포인터
(0x822a8=`org/kwis/msp/lcdui/Card`, 0x822d4=`java/lang/Thread`); 클래스디스크립터의 작은
`ptr_parent_class` 값(0x51/0x165/0x173/0x189)이 같은 인덱스 공간(fo 0x7b1b8 첫 u16 = 0x0173 과 일치).
성공하면 **모든 메서드가 필드명까지 심볼화된다.**

**P1-c (1시간) — 게임 자체 하단 HUD 렌더러 특정 = CHAR 앵커의 독립 2차 증거.**
클래스 메타에서 디스크립터가 `(Lorg/kwis/msp/lcdui/Graphics;…)V` 인 메서드를 전부 뽑고,
`fillRect` 계열 헬퍼를 두 번(적/청) 부르는 함수를 찾는다. 성공판정: HP 바 폭 계산식
`cur*W/max` 가 보이고 그 cur/max 가 어느 필드/배열 인덱스인지 읽힌다.
(이 함수는 §4.2 의 `hud_bottom_patch` hero2 대응 지점과 같은 것이다 — 두 과제가 한 번에 풀린다.)

**P2 (30분) — 이동차분 재시도, 조건을 바꿔서.**
전역 32MB 스캔이 아니라 **P0-b 에서 얻은 객체 포인터 주변 0x100 바이트만** 두 프레임 차분.
실패하면 좌표가 `[S`/`[B` 안에 있다는 뜻 → 배열 핸들을 따라 반복.

### 9.3 병행 가능한 대안 — 상태워드 전이 지문  ⛔ **값 서술 반증 (2026-09-09)**

> ⛔ **아래의 "2=부트/로고, 3=타이틀·슬롯 셸, 4=필드, 5=셀렉터 … hero2 도 같은 값" 은
> 틀렸다.** 실측(probe 로 부팅→타이틀→슬롯→필드→창을 몰고 프레임과 같이 읽음):
> hero2 의 상태워드는 **CHAR 상대가 아니라 절대 자바 static `0x193aec`** 이고 값은
> **0=로고 · 2=타이틀 · 1=슬롯선택 · 3=필드 · 5=창** 이다. `4` 는 관측되지 않았다.
> `items.rs:610-622` 주석의 "same values as KTF" 는 근거 없는 주장이었다.
> 방법론(전이 지문으로 찾는다)은 옳았고 실제로 그 방법으로 찾았다.


앵커 체인이 막히면: 상태워드는 `2=부트/로고, 3=타이틀·슬롯 셸, 4=필드, 5=셀렉터` 이고
`items.rs:610-622` 주석이 **hero2 도 같은 값**이라고 명시한다("same values as KTF, so the
`== 3` test itself ports unchanged"). 즉 찾는 것은 **이산·반복·양방향 전이**다:
`타이틀(3) → 이어하기 → 인게임(4)` 과 `인게임(4) → 종료 → 타이틀(3)`.
이 전이는 기기·probe 양쪽에서 몇 초마다 무한 반복 가능하고, `3→4` 와 `4→3` 양쪽으로 교차
필터링하면 잡음이 거의 남지 않는다 — 이동차분보다 신호 대 잡음이 훨씬 좋다.

도구: hero3 가 CHAR 오프셋을 캘 때 쓴 온디바이스 감시자 `items::flag_window_changes`
(`items.rs:1162`, 바이트 단위 변화 추적 + "N 샘플 연속 변하면 카운터로 보고 뮤트" 필터)가
hero2 에 상속돼 있으나 첫 줄이 `resolve_char(read)?` 라 죽어 있다.
→ **절대 주소 범위를 받는 변형**(`flag_range_changes(read, base, len)`)을 추가하면
게이트를 열지 않고(= hero3 주소가 hero2 메모리를 건드릴 위험 0) 리스트 힙 앞 6 MB 를 훑을 수 있다.

---

## 10. 실측 미해결 질문

| # | 질문 | 실험 |
|---|---|---|
| Q1 | hero2 화면 이미지는 왜 4bpp 인가, 565 로 바꿀 수 있는가 | WIPI-Java lcdui 화면 버퍼 생성 지점 추적. §3.4 의 세 경로가 걸려 있다 |
| Q2 | 세션 경과에 따른 9.2→5.6 fps 열화의 원인 | 힙 워크는 반증됐다(§3.7). 냉각 후 재측정 + `svc_mix` 시계열. 로그 강등 빌드로 A/B |
| Q3 | 컷신 판별식이 진짜 스토리 컷신(박스 없는 연출)을 미탐하는가 | 새 게임으로 프롤로그 진입 후 5프레임 간격 덤프 전량 판정 |
| Q4 | 상점·창고·전투 화면이 전폭 단색 괘선을 그려 오탐하는가 | 각 화면에 probe 로 진입해 같은 판정 |
| Q5 | hero2 컷신에서 `#` 가 스킵인가, CLR 이 대화창을 닫는가 | 기기. §4.8 정책 충돌 2건의 판정 근거 |
| Q6 | hero2 가 200ms 이상 타이머를 거는 화면이 있는가 | `RUST_LOG=wie_wipi_c=debug` 로 `MC_knlSetTimer` 히스토그램. 0건이면 hero2 는 WIPI 타이머가 아니라 자바 스레드 sleep 으로 프레임을 돈다는 뜻 |
| Q7 | 제네릭(해시 없는) 컴파일러 라이브러리 훅 6개가 hero2 에 붙는가 | `RUST_LOG=wie_core_arm=warn` 5프레임 런에서 `Installed N hooks` 확인 |
| Q8 | 몰입 프레임버퍼 폭이 게스트 비용을 올리는가 | `--w 176/352/452 --h 220` 3회 A/B, `[work]` 의 `M/frame` 비교. 크게 늘면 `base_h` 기본값을 내릴 근거 |
| Q9 | `bv.run()` 널 점프의 1차 원인 3가지 중 어느 것인가 | §4.4 미결 위험 |
| Q10 | hero2 SMAF 32개의 피크 진폭 | `SYNTH_GAIN`/`SFX_TRIM` 재튜닝 근거 |

---

## 11. 이 조사로 뒤집힌 기존 서술

| 문서 | 기존 서술 | 정정 |
|---|---|---|
| 계획서 §1.2 | `client.bin1120` = "Thumb ARM, **WIPI-C**", "자바 클래스 없음" | **AOT 컴파일된 Java MIDlet**. jar 에 `.class` 는 0개지만 클래스 메타(108/878/3725)가 바이너리 안에 있다 (§7) |
| 계획서 §7 Phase 3-9 | 핫 블리터를 찾아 `binary_patches.toml` hero2 md5 엔트리 신설 | **폐기.** 프로파일이 핫 게스트 루틴의 부재를 증명 (§3.2) |
| 계획서 §7 Phase 4-11 | 게스트가 접속 실패를 어떻게 다루는지 확인 — **무한로딩 위험** | **해당 없음.** `서버 장애 중입니다.` 로 우아하게 실패 (§4.4) |
| 계획서 §7 Phase 4-12 | "**캐시샵**/공용창고의 저장소는 앱 로컬 파일" | **캐시샵 삭제.** hero2 원본에 없다 (§4.5) |
| 계획서 §9.1 | "캐시샵도 같은 방식으로 게임 안에 이미 있는지 먼저 확인할 것" | **확인 완료: 없다** (§4.5) |
| 계획서 §9.3-① | STATUS 화면 문자열을 역추적해 CHAR 앵커로 | **실측 음성.** 라벨이 바이너리에 없다(에셋 tdf 에 있다). §9 의 클래스 메타 경로로 대체 |
| 계획서 §4 | "스킬 버튼 세로 칼럼 → 세로 배치 프리셋" 작업 항목 | **이미 완료** (커밋 `7a92d2d`, §4.2) |
| 계획서 §3 | `LOGICAL_W_MIN=240` 이 176 을 막는다 / `POPUP_SAFE_LOGICAL_W=496` 캡 | **둘 다 해소됨** (`7a92d2d`) |
| CLAUDE.md | `network.rs` 리다이렉트 표(hanbit IP) — "hero2 서버 주소 확인 후 교체" | **확인 완료: `211.239.150.70:30002`/`:30000`, 스킴 `BillSocket://`** (§4.5) |
| — | (신규) `wie_probe` 안전 규율 | hero2 에서도 `GuestBuild::Ktf`(hero3 앵커)를 세팅한다. **`--poke`/`--host-save`/`--host-levelup` 금지** (§4.6) |
| — | (신규) 트리 비교 규율 | hero2 는 CRLF, hero3 blob 은 LF. **md5 비교 무의미, `tr -d '\r'` 정규화 필수** (§1) |

---

## 12. 기기 재현 절차

```bash
ADB=/c/Users/<user>/AppData/Local/Android/Sdk/platform-tools/adb.exe
$ADB install -r -d android_app/app/build/outputs/apk/debug/app-debug.apk
$ADB logcat -G 64M      # ★ 스텁 로그가 링버퍼를 넘쳐 요약 줄을 밀어낸다
$ADB shell monkey -p com.hero2remake.app -c android.intent.category.LAUNCHER 1
```

좌표(2220×1080 가로): `OK=2020,827` `CLR=2131,670` `기어=1227,46` `123=1301,46`
디패드 `상=355,605 하=355,949 좌=186,766 우=533,766`
퀵 `1=132,184 3=344,184 #=239,291 7=132,396 9=344,396`
SET 행 `LAYOUT=1110,363 ACCOUNT=1110,481 DIAG=1110,599 TOUCH=1110,717 HEAT=1110,835`
에디터 툴바 `GAME=733,48 MENU=920,48 FLIP=1109,48 RESET=1295,48 CANCEL=1483,48 SAVE=1642,48`

* 인게임: `OK` ×3 (각 3초 간격) 후 ~15초 로딩
* CLR 메뉴 → SYSTEM: `CLR` → 디패드 하 ×7 → `OK`
* CARD: `CLR` → 하 ×6 → `OK`
* 네트워크창고: 타이틀에서 `OK` → 우 ×3 → `OK` → `OK`(예)
* 정보이용료: 타이틀에서 `OK` → 우 ×4
* 성능: `$ADB logcat -d | grep -E "emu throughput|emu loop|emu paint|quickbar probe|area gate"`
  — 게이지는 `frame % 1800 == 900` 마다이므로 **60초 이상** 부하를 준 뒤 읽는다
* HEAT SAVER A/B: **인앱 토글은 재시작 후 적용된다.** 파일로 쓰고 `am force-stop` 할 것 —
  `$ADB shell "run-as com.hero2remake.app sh -c 'echo on > /data/data/com.hero2remake.app/files/wie_heat_saver.txt'"`
  적용 확인은 logcat `resized: 2220x1080 -> render 1110x540`

헤드리스:
```bash
./target/release/wie_probe.exe --zip android_app/app/src/main/assets/hero2_ktf.zip \
  --w 452 --h 220 --ms 20 --frames 3000 --keys 320:OK,420:OK,520:OK,620:OK,720:OK \
  --profile 1 --out <scratch>
```
`--ms` 필수(가상 시계 없으면 검은 화면). 기본값이 전부 hero3 이므로 `--zip --w --h --out` 명시.
오타난 플래그는 조용히 무시된다.
