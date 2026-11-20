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

<!-- release-section -->
## 최신 릴리스 (APK)

**[최신 APK 받기 → Releases](https://github.com/apkdeployer2580/hero2_deploy_repo/releases/latest)**

| 판 | 파일 | versionCode |
|---|---|---|
| 릴리스 | `hero2_ktf1.07_0.163.0.apk` | 163 |

**이미 설치해서 하던 분 — 세이브를 그대로 이어 하려면**

* 앱을 **지우지 말고** 받은 APK 를 그대로 설치하세요. 업데이트로 덮어 설치되고 세이브가 그대로 남습니다.
* ⚠ 앱을 지우면 세이브도 함께 지워지고 되살릴 수 없습니다.
* 「앱이 설치되지 않았습니다」가 뜨면 서명이 다른 APK(예: 이 소스로 직접 빌드한 APK)가 깔려 있는 것입니다. 게임의 환경설정 맨 아래 「세이브내보내기」로 세이브를 파일로 빼 두고, 그 앱을 지운 뒤 새 APK 를 설치해서 「세이브불러오기」로 넣으세요.
* 공식 APK 의 서명 인증서 SHA-256: `f6ec47cb4717874fe1d16e197763401a67c99e06ed6537b68cff04d46d38e922`
