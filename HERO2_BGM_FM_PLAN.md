# 배경음 원본 재현 계획 — 효과음 뒤 곡 이어가기 + MA-2 FM 음색 합성 (2026-10-01 작성 · 같은 날 A~B3 헤드리스 완료, B4 기기 대기)

## 0. 진행 기록 (2026-10-01, 세션 hero2nativeapp-9b) — ▶ 다음 = B4 (기기에서 귀로)

| 단계 | 상태 | 무엇을 · 어떻게 증명했나 |
|---|---|---|
| **A** 효과음 뒤 이어가기 | ✅ 헤드리스 | `audio.rs` `effect_hold_ms` = `max(효과 + 700, 3000 + 500)`(`GUEST_REISSUE_MS` · `REISSUE_SLACK_MS`). probe 슬롯 1 에 `--h2-sfx 1000:0`: 효과음 21.04 s 뒤 **정지·재시작 0**, 21~28 s 500 ms 창마다 음 계속(전: 22.02 정지 → 24.06 처음부터). 단위 테스트 둘(재개 3 s 뒤 같은 곡 = 재시작 없음 · 진짜 정지는 늦게라도 온다) — 옛 식으로 되돌리면 실패하는 것 확인 |
| **B1** 해석기 | ✅ | `vendor/wie/wie_backend/src/system/ma2.rs` `parse_ma2` → `Ma2Song { voices, events(ms), len_ms, rhythm, exclusive_ignored }` — 엄격(모르는 것이면 `None` → GM 경로). 단위 테스트 6. **18 곡 전수 = 파이썬 독립 해석과 동일**: 키 이벤트 15,662 · 레벨/프로그램 5,777 · 음색 131 · 곡 길이 전부(`tools/ma2_render/compare.py`) |
| **B2** 엔진 | ✅ 1차 | `wie_android/src/ma2_fm.rs` `Ma2Fm`(의존성 0, 셀프테스트 `ma2_fm_selftest.rs` 8). 2 파트 × 16 채널 단음, OPL 식 엔벨로프(문서 시간표 · EGT · KSR), 8 파형, 피드백·변조 깊이 OPL 값, YMF825 LFO/깊이, 볼륨·익스프레션 = 40·log10. 오프라인 렌더 = `tools/ma2_render`(1 분 곡 0.3 s). **`FM_GAIN` 0.25 → 17 곡 평균 RMS 가 지금 GM 경로와 +0.1 dB**(곡별 ±9 dB 는 음색 차이) · 모든 곡 피크 ≤ −9.3 dBFS(리미터 무개입). 02: 본문 − 도입 **+5.4 dB**(GM −11.4 · 참고 FM +4.2 — 역전 해소) |
| **B3** 통합 | ✅ 헤드리스 | 싱크 트레이트 `fm_supported/fm_voices/fm_event/fm_all_off`(기본 = 안 함 → GM 그대로) · `Score.ma2` · `SmafPlayer::play_fm`(같은 시계·정지 규칙 `wait_for_event`) · 게이트 `fm_part` → `silence` 가 파트를 하드 컷 · **3 s 미만 MA-2 = 효과음 → 파트 1**(42 가 곡을 안 끊는다; GM 경로에선 전처럼 곡 취급) · 안드로이드 `FmMsg` 채널 → `Bus` 가 음악 게인으로 렌더 · FM 모드에선 3 s 조용한 SoundFont 합성기 렌더 생략 · 스위치 `wie_bgm_synth.txt` = `gm` 이면 옛 소리(기본 FM, 시작 때 한 번 읽음). 단위 테스트 4 · arm64 `check` 새 경고 0. probe `--audio-fm 1` / `--audio-wav <path>` 실게임: 타이틀→마을 = `fmoff 0` 뒤 `fmv 0` · 효과음 0 뒤 곡 계속 · **42 = 파트 1, 곡 끊김·재시작 0** · GM MIDI 이벤트 0 · md5 둘·`[heap]` 불변 |
| **B4** 기기 | ⏳ | **vC 57 로 S25·S9 설치됨(2026-10-01 01:58)** — 사용자 청취·기기 확인 대기. PC 청취용 = `dist\bgm_audit\*_C_app_fm_engine.ogg`(18 곡, A·B 옆). 되돌리기 = `/sdcard/Android/data/com.hero2remake.app/files/wie_bgm_synth.txt` 에 `gm`(릴리스 S25 도 `adb push` 로 닿는다) 후 앱 재시작 |
| **C** 효과음 (2026-10-02) | ✅ 헤드리스 | 효과음 11 개는 `ATR` = 4-bit ADPCM 8 kHz(46 만 4 kHz)이고 **`smaf_player` 가 다른 야마하 칩(ADPCM-B, 상위 니블 먼저)의 규칙으로 풀고 있었다** — 파형이 한쪽으로 쏠리고 끝에서 절벽(「툭」). `wie_backend/src/system/atr.rs`(하위 니블 먼저 · `230 230 230 230 307 409 512 614 >> 8`) + `wie_android/src/sfx_upsample.rs`(카이저 싱크 32 탭 · 끝 4 ms 페이드 · 에어 `SFX_AIR` 0.6). 들리는 크기 평균 −33.3 → −22.3 dB · 실게임 probe `wave … mean 75 last 563`(이전 −21,694 / −22,950) · wie_backend 59 · `sfx_upsample_selftest.rs` 11 · md5 둘 · `[heap]` 불변. 벤치 = `ma2_render sfx … --mode old|decode|sinc|ship`, PC 청취 = `dist\sfx_audit\`. 정본 = `HERO2_GUEST_RE.md` §8.43 |

**귀로 맞출 손잡이 (전부 `ma2_fm.rs` 상수)** — 근거 없는 추정 순: ① `KSL_DB_PER_OCT`(YMF825 순서 0/1.5/3/6) + `KSL_ZERO_HZ` 48.55(OPL ROM) — 캐리어에 KSL 2 인 멜로디(05·01 pc 58, 01 pc 72, 06·07 pc 77)가 참고 FM 보다 7~16 dB 작은 주원인(참고 엔진은 가운데 C 기준) ② 볼륨 법칙(`update_gain`, 40·log10 — 참고 엔진은 선형) ③ `MOD_UNITS`·피드백 스케일(OPL, 참고 엔진보다 FB 7 에서 약 7 배 세다) ④ `FM_GAIN`. 바꾼 뒤 `tools/ma2_render render` 로 1 초 안에 다시 들을 수 있다.

---


**사용자 결정 (2026-09-30):** *"1. 이어지게 해줘. 2. 음색은 너가 권장한대로 해줘. 문서만 업데이트하고 다음 세션에 이어서 진행할게"*
→ **A. 효과음이 나도 배경음이 끊기거나 처음부터 다시 시작하지 않는다** (hero3 방식) · **B. 곡 파일에 들어 있는 FM 음색 그대로 연주한다** (GM 사운드폰트 대체를 끝낸다).

근거가 된 조사 = `HERO2_GUEST_RE.md` **§8.37** (맵→곡 규칙 · 18 곡 전수 검증 · 악기별 균형표). 비교 음원 = `dist\bgm_audit\`
(곡마다 `_A_app_now` = 지금 앱 · `_B_file_fm_voices` = 파일 음색 근사 · `README.txt`), 조사 도구 = `dist\bgm_audit\tools\` (아래 §6).
⛔ 둘 다 gitignore 된 로컬 폴더다 — 게임 음원(한빛소프트 저작물)이라 저장소에 넣지 않는다.

순서: **A 먼저**(작고 독립적) → **B**(큰 작업, 단계별). 둘 다 런타임 코드이므로 ⛔ 착수 전에 `ListAgents` + 활성 hero2 세션에 알릴 것
(메모리 「여러 세션이 한 작업 트리 공유」). 손대는 파일: A = `vendor/wie/wie_backend/src/system/audio.rs` · `wie_probe/src/main.rs` /
B = 위 둘 + `wie_android/src/audio_sink.rs` · 새 파일들.

---

## 1. 확정된 사실 (다시 조사하지 말 것)

| | |
|---|---|
| 곡 파일 | 18 개 전부 SMAF **Handy Phone Standard**(MA-2): `MMMD` › `CNTI` › `MTR1..4`(format 0, 트랙당 4 채널 → 논리 채널 `트랙×4+ch` = 0..15), 틱 4 ms(`tbD = tbG = 0x02`). 효과음(00·40~53)은 `ATR`(ADPCM) — **B 와 무관** (★ 2026-10-02: 그 ADPCM 이 잘못 풀리고 있었다 — §0 「C」) |
| MTR 머리 | `format(1) seqtype(1) tbD(1) tbG(1) chstatus(2)` — chstatus 니블 = `KCS VS type2` (type 3 = rhythm). 드럼 채널은 **전부** type 3 이고 bank 0x80 을 건다 |
| 하위 청크 | `MspI`(`st:` u32 · `sp:` u32 — 18 곡 전부 st 0 · sp = EOS 오프셋) · `Mtsu`(트랙 1, 음색) · `Mtsq`(시퀀스) |
| 길이 부호 | 1 바이트 `< 0x80` 그대로 · 2 바이트 `((b0 & 0x7f) + 1) << 7 \| b1` (b1 은 전부 < 0x80) |
| 음 | 상태 `cc oo nnnn`(0x01..0xFE) + 게이트(길이 부호). **음 번호 = nnnn + 12·oo + 36 + 12·옥타브시프트**(nnnn 1 = C#, 12 = C; 0·13~15 는 안 나온다). 게이트 0 음 0 개 |
| 제어 (`00` + `cc tttttt`) | t 0x00 = +1 바이트 NOP · 0x01..0x0E 짧은 익스프레션(표 `00 00 1f 27 2f 37 3f 47 4f 57 5f 67 6f 77 7f`) · 0x11..0x1E 짧은 벤드 · 0x21..0x2E 짧은 모듈레이션 · +1 바이트: 0x30 프로그램 · 0x31 뱅크(bit7 = 드럼) · 0x32 옥타브시프트(0..4 / 0x81..0x84 = −1..−4) · 0x33 모듈레이션 · 0x34 벤드 · 0x36/0x3B 익스프레션 · 0x37 볼륨 · 0x3A 팬 |
| `FF` | `FF 00` NOP(길이만 진행 — 곡 끝 정렬용, 트랙마다 하나) · `FF F0 len … F7` 익스클루시브 — ⛔ **길이 바이트로 건너뛸 것.** F7 을 찾으면 안 된다: 02 의 `Mtsu` 음색 데이터 안에 `f7` 이 들어 있다(`14 62 f7 00 a0`) |
| 끝 | `00 00 00 00` (= `MspI sp`) |
| 쓰인 값 범위 (18 곡 전수) | 팬 64 만(20 회) · 볼륨 22..127 · 익스프레션 5..123 · 옥타브시프트 0/1/2/0x81/0x82 · 벤드·모듈레이션 이벤트 **0 회** · 익스클루시브는 14 번 트랙 3 의 `43 03 90 {ba\|ca} xx` 68 개뿐 |
| **음색 (`Mtsu`)** | `FF F0 len` + `43 03 <en> <bank> <pc>` + `g0 g1` + 연산자 5 B × 2 + `F7` (18 곡 131 개, 전부 2-op). `g0` = LFO(7-6) · FB(5-3) · ALG(2-0) · `g1` = 0x01 고정. 연산자: `MULT(7-4) VIB(3) EGT(2) SUS(1) KSR(0)` · `RR(7-4) DR(3-0)` · `AR(7-4) SL(3-0)` · `TL(7-2) KSL(1-0)` · `DVB(7-6) DAM(5-4) AM(3) WS(2-0)` — go-smaf `voice/vmafm.go` 와 같다 |
| EGT | **1 = 지속음**(키를 누르는 동안 SL 에서 유지), **0 = 감쇠음**(SL 에 닿으면 RR 로 계속 줄어든다). 근거: go-smaf `ToVM35` 의 `SR = EGT ? 0 : RR` · 드럼 음색(bank 0x80)이 전부 EGT 0 · 10 초 게이트를 받는 패드가 EGT 1. ⛔ 참고 구현 yamaha-smaf-player 는 이것을 **거꾸로** 읽는다(`egType = (sr != 0)`) |
| 쓰인 음색 값 | SUS 비트 **전부 0**(262 연산자) · WS 0~7 전부 쓰임(0 이 161) · LFO 1 이 119/131 · VIB/AM 는 소수(약 30 연산자) · ALG 0/1 만 |
| 드럼 | 드럼 채널 = bank 0x80 + 프로그램 = 드럼 음색 pc(= GM 드럼 키). 원본은 그 FM 음색을 **음의 높이로** 연주한다(지금 앱은 음 높이를 버리고 pc 를 GM 키로 쓴다) |
| 채널 | 같은 채널 음 겹침 ~~0 개~~ → ⛔ **1 개**(2026-10-01 정정: 01 타이틀 ch 2, 13,068 ms 에 음 84 가 음 76 의 키오프보다 **4 ms** 먼저 온다) — 채널당 단음이면 새 음이 채널을 다시 치고, 늦게 온 76 의 키오프는 **음 번호가 달라** 무시된다(엔진이 키오프를 음 번호로 확인). 그 밖의 7,830 음은 겹침 0 |
| 루프 | 게임은 곡을 `Player.play(clip, true)` 로 튼다 → 곡 끝(마지막 트랙 EOS)에서 처음으로. 곡 끝을 넘는 음 0 개 |

**게임의 사운드 관리자 `br`** (§8.37): 곡 캐시 `br.a:[Clip` · 효과음 캐시 `br.b:[Clip` · 지금 맵 곡 `br.a:Clip` · 스크립트 곡 `br.b:Clip` · 재생 중 `br.c:Clip` ·
재개 타이머 = static long(`br.a:J` 로 보임, `[sl+0x74]+0x14`).

| 경로 | 동작 |
|---|---|
| 효과음 `br.a(B)V` @0x1388c8 | `br.a(clip, false)` = **재생 중인 곡을 `Player.stop`** → 효과음 재생 → 타이머 ← **now + 3000** |
| 곡 `br.a(BZ)V` @0x13882c | 재생 → 타이머 ← now + 3000, **반복 안 하는 곡이면 +10000 더**(= 13 s 뒤 맵 곡 재개; 헬퍼 해석은 추정) |
| 매 프레임 `br.a()V` @0x1385a4 | 타이머가 지나면 `br.b`(스크립트 곡) 또는 `br.a`(맵 곡)를 `Player.play(…, true)`, 타이머 ← 0. **재생 중인 클립과 같으면 다시 틀지 않는다** |
| 효과음 칸 | 0 ← 40 · 1 ← 41 · **2 ← 42(MIDI!)** · 3 ← 43 (`cf.f()V`) · 4 ← 51/52/53 (`cf.a(B)V`) · 직업별 5·6·7·8·9 ← 45·46·47·53·49 (`cf.g()V`). 42 는 환경설정 창 `r.a(II)Z` 에서만 |

---

## 2. A — 효과음이 나도 곡이 이어진다

**지금 (probe 실측, 슬롯 1, 게임 자신의 `br.a(B)V(0)` 주입):** 21.04 s 효과음 → 22.02 s 곡 정지 → 2 s 무음 → **24.06 s 곡이 처음부터**.
원인: `Audio::play` 의 효과음 분기가 음악을 `now + 효과음 길이 + MUSIC_REJOIN_MS(700)` 까지만 붙잡는다(hero3 는 약 0.6 s 뒤에 곡을 다시 부른다).
hero2 는 **3 s 뒤** 다시 부르므로 그 전에 음악 태스크가 끝나 버리고, 다시 부른 곡은 새 재생 = 처음부터.

**고칠 곳:** `vendor/wie/wie_backend/src/system/audio.rs`, `Audio::play` 의 `} else if let Some((_, gate, _, _)) = live_music {` 분기.

```rust
// 게임이 곡을 다시 부르는 시점(hero2 = 효과음 시작 + 3000 ms, br.a(B)V 의 0xbb8)까지 붙잡는다.
gate.hold_until(now_ms + (score.duration_ms + MUSIC_REJOIN_MS).max(GUEST_REISSUE_MS + REISSUE_SLACK_MS));
```
* `GUEST_REISSUE_MS = 3000`, `REISSUE_SLACK_MS = 500`(게임의 타이머 검사는 50 ms 프레임마다 + 틱 지연). 다시 부르는 곡이 같으면 기존 `Arc::ptr_eq` 분기가
  `keep_going()` 하고 **다시 시작하지 않는다**(이미 있는 코드) — 이게 목표 동작이다.
* 연속 효과음은 `hold_until` 이 계속 늘린다(이미 그렇다). 다른 곡이 오면 기존대로 즉시 교체(`silence`).
* ⚠ 부작용: 효과음 직후 게임이 곡을 진짜로 멈추면(재개 없음) 최대 3.5 s 늦게 멈춘다. hero2 에서 그런 경로는 아직 못 봤다(맵 74 무음·게임오버는
  효과음 경로가 아니다). 이 vendored wie 는 hero2 앱 전용이다.
* ⛔ MIDI 효과음 42 는 이걸로 안 고쳐진다 — `Score::is_music` 이라 「다른 곡」 분기로 가서 곡을 끊는다. 두 MIDI 가 같은 신스 채널을 쓰므로 섞을 수도 없다.
  → **B 에서 해결**(재생마다 채널 묶음을 따로, §3.4). 환경설정 창에서만 나는 소리라 A 단계에서는 그대로 둔다.
* 테스트: 같은 파일의 `a_guest_stop_of_the_music_survives_an_effect_but_not_a_menu` 가 `MUSIC_REJOIN_MS` 로 hold 를 계산한다 — 새 식에 맞추고,
  「효과음 뒤 3 s 에 같은 곡을 다시 부르면 재시작하지 않는다」 테스트를 더한다(가상 시계는 같은 파일의 `NullPlatform`).
  실행 = CLAUDE.md 「셀프테스트」 절(`vendor\wie` 안에서 `cargo test -p wie_backend`, dlltool 셔임).

**probe 로 증명 (먼저 플래그를 옮길 것):** 조사 때 probe **사본**에 붙였던 두 가지를 `wie_probe` 에 정식 플래그로 옮긴다 — 원본 코드 =
`dist\bgm_audit\tools\probe_audio_log_snippet.rs.txt`.
* `--audio-log <path>` — `NoAudio` 가 가상 시계와 함께 싱크 이벤트를 파일로(`on/off/pc/cc/aso/wave/musvol`). ⛔ `audio_sink()` 가 여러 번 불려도 파일은 한 번만 연다(`OnceLock`).
* `--h2-sfx <f:n[,f:n]>` — 프레임 f 에 게임 자신의 `br.a(B)V(n)` 를 `call_java_method(JavaReceiver::Class("br"), "a", "(B)V", [Int(n)])` 로.
  ⚠ 칸이 비어 있으면 아무 일도 없다 — 마을(슬롯 1)에 실린 칸은 0~3 뿐이었다(칸 8 은 무반응). 효과음 0 = 크리티컬 246 ms.
* 판정(슬롯 1, `--keys 320:OK,420:OK,520:OK --h2-sfx 1000:0 --frames 1500`): 효과음 뒤 `aso` 없음 · 곡 시작의 `pc` 묶음이 다시 안 나옴 ·
  500 ms 창마다 note-on 이 끊기지 않음. 지금 코드에서는 22.0~24.0 s 가 비고 24.06 s 에 `pc` 묶음이 나온다.
* 감시선: md5 둘·`[heap]` 불변이어야 한다(오디오는 게스트를 안 바꾼다).

---

## 3. B — 곡 파일의 FM 음색으로 연주한다

### 3.1 모양

```
Audio::load_smaf(bytes)
  └ Score::parse ── MTR format 0 이면 ──► Ma2Song { voices, events(ms, 논리채널 0..15), len_ms }   (새, wie_backend)
                     아니면(=hero3 등)  ─► 지금 경로 (smaf_player → MIDI → oxisynth)
SmafPlayer::play ── 같은 타이밍 루프(STOP_POLL_MS·catch-up·repeat 그대로) ──► AudioSink 의 FM 메서드 (새)
AndroidAudioSink ── 채널 → 오디오 콜백 ──► Ma2Fm 엔진 (새, wie_android, 의존성 0) ── 음악 버스(볼륨·soft_limit) 로 섞임
```

* **해석기는 우리 것으로 새로 쓴다**(`wie_backend/src/system/ma2.rs` 가칭). `smaf_player` 는 HPS 에서 ① `Mtsu` 를 버리고(`FF F0` 을 모른다)
  ② 드럼 음 높이를 pc 로 바꿔 버리고 ③ 채널을 GM 채널로 접는다 — FM 에 필요한 정보가 셋 다 사라진다. 문법은 §1 표 그대로이고
  파이썬 판(`dist\bgm_audit\tools\hps.py`·`voices.py`)이 앱의 `smaf_player` 와 음 단위로 일치하는 것을 이미 확인했다 → Rust 로 옮기고 같은 값으로 대조.
* 싱크 트레이트 확장(기본 구현 = 안 함, `fm_supported() -> false` 면 지금 GM 경로로 떨어진다 → probe·다른 싱크 무영향):
  `fm_supported` · `fm_voices(slot, &[Ma2Voice])` · `fm_event(slot, ch, Ma2Event)` · `fm_all_off(slot)`(= `PlaybackGate::silence` 의 하드 컷).
  `slot` = 재생 묶음(0 = 곡, 1 = MIDI 효과음) — 채널이 겹치지 않으니 42 도 곡을 끊지 않고 섞을 수 있다(그때 `Score::is_music` 분류도 고친다).
* 드럼도 FM 이다(bank 0x80 음색을 그 음 높이로). GM 드럼 키 변환은 FM 경로에서 쓰지 않는다.

### 3.2 엔진 (`wie_android/src/ma2_fm.rs` 가칭 — 의존성 0 · edition 2021 · 루트 `ma2_fm_selftest.rs` 가 `include!`)

| | 기준 (MA-2 데이터시트는 없다 — OPL3 에서 시작하고 귀·렌더 비교로 맞춘다) |
|---|---|
| 보이스 | 논리 채널 16 개 = 보이스 16 개, **채널당 단음**(새 음 = 그 채널 재트리거; 릴리스 꼬리는 다음 음까지 울린다) |
| 알고리즘 | ALG 0 = op1 → op2 직렬 FM · ALG 1 = op1 + op2 병렬 |
| 주파수 | 음 번호 → Hz, 연산자 × MULT(0 = ½) |
| 파형 WS 0~7 | OPL3 8 종(사인 · 반파 · 절대값 사인 · 펄스 사인 · 교대 사인 · 낙타 사인 · 사각 · 유도 사각)으로 시작 |
| 피드백 FB | op1 자기 변조(OPL 식: 직전 두 출력 평균 × 2^(FB−?)) — 귀로 맞출 값 |
| 엔벨로프 | dB-선형. OPL3 시간표(KSR 0): 어택 0→100 % AR1 2826 · 2 1413 · 3 707 · 4 353 · 5 177 · 6 88 · 7 44 · 8 22 · 9 11 · 10 5.5 · 11 2.8 · 12 1.4 · 13 0.7 · 14 0.35 · 15 0 ms / 감쇠·릴리스 0→−96 dB DR·RR 1 39281 · 2 19640 · 3 9820 · 4 4910 · 5 2455 · 6 1228 · 7 614 · 8 307 · 9 153 · 10 77 · 11 38 · 12 19 · 13 9.6 · 14 4.8 · 15 2.4 ms. SL 3 dB 단위(15 = −93 dB). **EGT 1 = SL 에서 유지 · EGT 0 = SL 뒤 RR 로 계속 감쇠**. KSR 은 높은 음일수록 빠르게 |
| 레벨 | TL 0.75 dB/단계 · KSL · 채널 볼륨(0x37) × 익스프레션(0x3B/짧은) · 팬 0x3A(쓰인 값은 64 뿐) |
| LFO | g0 의 LFO(0..3) 속도 · VIB(DVB 깊이) · AM(DAM 깊이) |
| 벤드·모듈레이션 | 이벤트가 곡에 없다(0 회) — 받되 최소 구현 |
| `43 03 90 xx yy` | 14 번 트랙 3 의 실시간 쓰기 68 개(값이 천천히 오르내림 — 음색 스윕으로 보임). 뜻 미확인 → 첫 판은 무시하고 로그만 |
| 믹스 | 스테레오 출력 × 게인 → 음악 버스(지금 `SYNTH_GAIN`·`volume_gain`·`soft_limit` 자리). 게인은 18 곡 RMS 가 지금 GM 판과 효과음 대비 같은 균형이 되게 잰다 |

비용: 2-op × 16 보이스 × 48 kHz — oxisynth(리버브·코러스·4차 보간)보다 훨씬 싸다. hero2 곡이 전부 HPS 이므로 **GeneralUser.sf3 를 안 싣는 길**도 열린다
(S9 메모리 — CLAUDE.md 의 oxisynth 포크 설명 참고). 그건 FM 이 기기에서 확인된 뒤 따로 결정.

### 3.3 단계

1. **B1 해석기** — `Ma2Song` + 단위 테스트(합성 바이트: 길이 부호 2 바이트 · 짧은/긴 제어 · 옥타브시프트 음수 · `FF F0 len` 안의 `f7` · EOS · 트랙 길이 정렬).
   실제 18 곡 대조는 에셋이 있을 때만 도는 검사로(저장소에 음원을 넣지 않는다): 음 7,831 개 · 음색 131 개 · 트랙마다 EOS = `MspI sp` · 곡 길이(02 = 46,260 ms 등,
   `dist\bgm_audit\tools\render_all.log` 가 아니라 `hps.py` 출력과 대조).
2. **B2 엔진 + 오프라인 렌더** — 엔진을 probe(또는 작은 bin)에 붙여 곡을 WAV 로. `dist\bgm_audit` 의 B 렌더(참고 엔진 근사)와 스펙트로그램으로 비교
   (`tools\spec.py`), 특히 02 의 0~24 s(패드 지속 · 8 s 부터 250 ms 플럭 화음) · 05(짧은 음들이 릴리스로 이어지는 반주) · 09 · 17. 엔벨로프·레벨은 여기서 귀로 맞춘다.
3. **B3 통합** — 트레이트 메서드 · `Score` 분기 · `SmafPlayer` · `silence` 하드 컷 · 루프 · 볼륨 버스 · 42 를 묶음 1 로. 전환 스위치를 하나 둔다
   (예: `wie_bgm_synth.txt` = `fm` / `gm`, 기본 `fm`) — 기기에서 A/B 하고, 문제가 생기면 사용자가 바로 되돌릴 수 있게. 설정 창에 행을 만들지는 사용자에게 묻는다.
4. **B4 기기** — 맵 넘나들기(곡 교체 무음·겹침 없음), 전투(효과음 + A 의 이어가기), 게임오버·엔딩, 환경설정 42, S9 CPU(`top -H` AudioTrack 스레드), 효과음 대비 음량.
   ⛔ 사용자가 폰으로 직접 테스트한다 — 설치·탭 전에 물을 것(메모리).

### 3.4 위험

* **원본 칩 수치가 없다** — 엔벨로프 속도·레벨·LFO·파형 모양은 OPL3 에서 출발한 근사다. 판정은 사용자의 귀다(원곡 녹음은 못 찾았다).
* 참고 엔진(yamaha-smaf-player)의 수치를 그대로 믿지 말 것 — EGT 반전 말고도 엔벨로프가 선형 진폭·귀로 맞춘 시간이다.
* `Score::is_music` 은 지금 「효과음이냐 곡이냐」를 가르는 유일한 기준이다 — 42 를 효과음으로 돌릴 때 16(게임오버 8.7 s, 반복 안 함)을 효과음으로 잘못 넣지 말 것
  (구분 = 재생 묶음/길이; 16 은 곡을 끊어야 한다).

---

## 4. 완료 판정

* A: probe 오디오 로그에서 효과음 뒤 재시작 0 · 기기에서 전투 후 곡이 이어짐.
* B: 18 곡 해석 대조 통과 · 02 에서 8 s 부터의 플럭 화음 연타가 또렷이 들리고 도입 패드가 곡에서 가장 크지 않음(지금 앱의 역전 해소) · 기기 CPU 가
  지금 이하 · 사용자 청취 확인.

## 5. 문서

착수·완료 때 `HERO2_NEXT_SESSION.md` 맨 위 블록과 CLAUDE.md 블록을 갱신하고, 엔진 수치·EGT 판정 같은 확정값은 `HERO2_GUEST_RE.md` §8.37 에 덧붙인다.

## 6. 조사 도구 (`dist\bgm_audit\tools\`, 로컬 전용)

| 파일 | 용도 |
|---|---|
| `hps.py` | HPS 해석(앱의 `smaf_player` 규칙 그대로 + 엄격 검사). `python hps.py` = 18 곡 구조 점검 |
| `voices.py` · `gmnames.py` | `Mtsu` 음색 해독 · GM 이름표 |
| `smafdump/` | 앱과 같은 `smaf_player` 로 이벤트 덤프(`cargo build --release`) |
| `gmrender2/` | 앱의 GM 경로 오프라인 렌더(`parse_smaf` + oxisynth + GeneralUser.sf3, 같은 `SynthDescriptor`, 블록 렌더 = 실시간의 약 20 배) |
| `ysp_patch/` | 참고 FM 엔진 akustikrausch/yamaha-smaf-player `627f7ad` 에 대는 패치(EGT 수정 + `SMAF_ONLY_PC` 솔로) + MSVC 빌드 배치 |
| `render_all.py` · `solo_balance.py` · `spec.py` | A/B 렌더 · 악기별 균형 · 스펙트로그램(결과 `solo_balance.json`·`render_all.log`) |
| `probe_audio_log_snippet.rs.txt` | probe 사본에 붙였던 오디오 로그 싱크 + 효과음 주입(§2 에서 정식 플래그로 옮길 원본) |

⚠ 스크립트 안의 경로(스크래치 폴더·exe 위치)는 조사 당시 것이다 — 다시 쓸 때 맞출 것.
