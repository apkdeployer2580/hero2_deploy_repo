# hero2NativeAPP — 영웅서기2 안드로이드 리메이크

피처폰(WIPI) RPG **영웅서기2: 빙해의 검사** (KTF판 1.0.7)를 dlunch/wie 에뮬레이터
포크로 안드로이드에서 네이티브 구동하는 프로젝트.
[hero3NativeAPP](https://github.com/apkdeployer2580/hero3_deploy_repo) 의 검증된 스택
(wie 포크 + 터치 HUD 엔진 + 레이아웃 에디터)을 시드로 하고, 게임 HUD 는
**영웅서기5 의 인게임 HUD 배치**를 재현한다.

* 착수 설계서·로드맵: [`HERO2_REMAKE_PLAN.md`](HERO2_REMAKE_PLAN.md)

## 상태 (2026-08-18)

* ✅ 헤드리스 부트: 타이틀 → 인게임(맵/NPC 대화/사운드) 정상 (`wie_probe`, 176×220)
* ✅ 해상도 적응 확인: 440×220 가로 렌더 정상 → 가로 이머시브 HUD 전략 성립
* ⬜ HUD 없는 APK (Phase 1) → hero5식 HUD 스킨 (Phase 2) → hero2 게스트 RE (Phase 3)

## 빌드

게임 에셋(`android_app/app/src/main/assets/hero2_ktf.zip`)은 저장소에 들어 있다. Android SDK · NDK 25.1.8937393 · JDK 17+ · Rust(`cargo-ndk`)가 있는 Windows 에서:

```
.\build_android.ps1            # 디버그 APK → dist\hero2_ktf1.07_0.<versionCode>.0_debug.apk
```

릴리스 서명은 자기 키스토어로 한다(`build_android.ps1` 머리말의 `HERO2_KEYSTORE*`). 에셋을 원본 덤프에서 다시 만들려면 `python tools_re/make_hero2_asset.py <영웅서기2_KTF원본.zip>`.

헤드리스 실행(기기 불필요):

```
cargo build --release -p wie_probe
./target/release/wie_probe --zip android_app/app/src/main/assets/hero2_ktf.zip \
  --w 176 --h 220 --ms 20 --frames 1200 --keys 320:OK --out /tmp/h2 --dump 560
```
