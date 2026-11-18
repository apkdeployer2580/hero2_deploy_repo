import java.util.Properties

plugins {
    id("com.android.application")
}

// ── version ─────────────────────────────────────────────────────────────────
// Kept in ../version.properties rather than inline, so `build_android.ps1 -Release`
// can bump the code without editing this file (and so a merge conflict on it is a
// one-line conflict). Missing file -> 1/"0.1.0", i.e. a fresh clone still builds.
val versionProps = Properties().apply {
    val f = rootProject.file("version.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
val appVersionCode = (versionProps.getProperty("versionCode") ?: "1").trim().toInt()
val appVersionName = (versionProps.getProperty("versionName") ?: "0.1.0").trim()

// ── release signing ─────────────────────────────────────────────────────────
// An unsigned APK CANNOT BE INSTALLED, and `assembleRelease` produced exactly that
// until 2026-08-03 — which is why the only installable builds were debug ones,
// signed with the throwaway debug keystore Android generates per machine. A debug
// key is fine for your own phone and useless for handing the app to anyone: it is
// not stable across machines, and Play will not take it.
//
// The keystore and its passwords are SECRETS and are read from outside the repo, in
// this order (first complete set wins):
//
//   1. environment:  HERO2_KEYSTORE, HERO2_KEYSTORE_PASSWORD,
//                    HERO2_KEY_ALIAS, HERO2_KEY_PASSWORD
//   2. gradle properties (put them in %USERPROFILE%\.gradle\gradle.properties,
//      NOT in this project):  hero2.keystore, hero2.keystorePassword,
//                             hero2.keyAlias, hero2.keyPassword
//
// ★ 2026-09-01 — these were HERO3_ / hero3.* and that was not cosmetic: this project
// was reading ANOTHER app's signing key, so `assembleRelease` produced a
// com.hero2remake.app APK signed `CN=Hero3 WIE`. Caught before the first release
// (versionCode was still 1 and no release APK had ever been built), so nothing is
// orphaned. hero2's own alias `hero2` now lives in the same shared keystore file
// (one file, one alias per project — this machine's convention).
//
// With neither, the release build still succeeds but is UNSIGNED and the build
// prints how to fix it. Nothing here ever writes a secret into the build output.
fun secret(env: String, prop: String): String? =
    (System.getenv(env) ?: providers.gradleProperty(prop).orNull)?.takeIf { it.isNotBlank() }

val ksPath = secret("HERO2_KEYSTORE", "hero2.keystore")
val ksPass = secret("HERO2_KEYSTORE_PASSWORD", "hero2.keystorePassword")
val ksAlias = secret("HERO2_KEY_ALIAS", "hero2.keyAlias")
val ksKeyPass = secret("HERO2_KEY_PASSWORD", "hero2.keyPassword")
val ksFile = ksPath?.let { file(it) }
val canSign = ksFile != null && ksFile.exists() && ksPass != null && ksAlias != null && ksKeyPass != null

android {
    // ★ `namespace` (the JAVA package) and `applicationId` (the INSTALL identity) are
    // deliberately different since 2026-08-03.
    //
    // The Java class is `net.wie.emulator.MainActivity` and STAYS there: the Rust side
    // looks it up BY NAME through JNI (`orientation.rs`, `server_prompt.rs`), so moving
    // the class would break the orientation toggle and the server/account dialogs at
    // runtime — silently, since a JNI class lookup fails at call time, not at build.
    // Only the installed identity is renamed, which is all Play cares about.
    namespace = "net.wie.emulator"
    compileSdk = 36
    // ★ 16 KB 페이지 정렬은 이 NDK 로 이미 충족된다 — 올리지 말 것.
    // "NDK 25.1 은 4 KB 정렬을 방출하니 r27+ 가 필요하다" 는 이 빌드 경로에 대해
    // 틀렸다: 여기서는 clang 을 직접 부르지 않고 `cargo ndk` 를 부르며,
    // cargo-ndk 4.1.2 가 링크할 때 `-Wl,-z,max-page-size=16384` 를 이미 붙인다.
    // 산출물 실측 (2026-09-08):
    //   llvm-readelf -l jniLibs/arm64-v8a/libmain.so → LOAD 4개 전부 Align 0x4000
    //   zipalign -c -P 16 -v 4 app-{debug,release}.apk → lib/arm64-v8a/libmain.so (OK)
    // ⛔ 근거는 버전 번호가 아니라 산출물이다. 의심되면 위 두 명령을 다시 돌려라.
    ndkVersion = "25.1.8937393"

    defaultConfig {
        // ★ PERMANENT once uploaded to Play: it is the app's identity there and cannot
        // be changed afterwards. It is also the on-device data path
        // (/data/data/<applicationId>/), so changing it makes the app a DIFFERENT app —
        // a fresh install with no access to the previous one's saves.
        applicationId = "com.hero2remake.app"
        minSdk = 26        // AAudio (next-stage audio) needs API 26; S25 = API 36
        // 36 = Android 16. 사용자 결정 (2026-08-28, HERO2_NEXT_SESSION.md §1.2):
        // Play 등록은 안 하고 사이드로드 전용이지만 targetSdk 는 36 으로 올린다.
        // 36 의 동작 변경 중 이 앱에 실제로 닿는 것:
        //  · 16 KB 페이지 강제 — 이미 충족 (위 ndkVersion 주석의 실측).
        //  · edge-to-edge 강제 — 무해. MainActivity.hideSystemBars() 가 이미
        //    setDecorFitsSystemWindows(false) + systemBars hide 로 완전 몰입이다.
        //    (그리고 edge-to-edge 강제 자체는 targetSdk 35 부터 이미 적용된다.)
        //  · sw>=600dp 화면에서 screenOrientation 무시 — 폰(S9, sw≈360dp)은 해당 없음.
        //    태블릿/펼친 폴더블에서는 orientation.rs 의 세로/가로 고정이 무시될 수 있다.
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersionName
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    if (canSign) {
        signingConfigs {
            create("release") {
                storeFile = ksFile
                storePassword = ksPass
                keyAlias = ksAlias
                keyPassword = ksKeyPass
                // Both schemes: v1 is what API 26 verifies, v2 is what everything
                // since verifies and what Play expects.
                enableV1Signing = true
                enableV2Signing = true
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (canSign) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The Rust cdylib is produced by cargo-ndk into src/main/jniLibs; it is
    // already stripped, so let AGP keep it as-is.
    packaging {
        jniLibs {
            keepDebugSymbols += "**/*.so"
        }
    }
}

// games-activity 4.4.0 drags in the pre-1.8 split kotlin-stdlib-jdk7/jdk8
// (1.6.21) artifacts, whose classes were folded into the main kotlin-stdlib in
// 1.8+. That collides with appcompat 1.7.0's kotlin-stdlib 1.8.22 (duplicate
// class). Forcing the split artifacts to 1.8.22 (now empty stubs) resolves it.
configurations.all {
    resolutionStrategy {
        force(
            "org.jetbrains.kotlin:kotlin-stdlib-jdk7:1.8.22",
            "org.jetbrains.kotlin:kotlin-stdlib-jdk8:1.8.22",
        )
    }
}

// Say it plainly at the top of a release build rather than letting someone discover
// it when the APK will not install on a tester's phone.
if (gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }) {
    if (canSign) {
        logger.lifecycle("==> release signing: ${ksFile!!.absolutePath} (alias $ksAlias), versionCode $appVersionCode")
    } else {
        logger.warn(
            "==> WARNING: release build will be UNSIGNED and CANNOT BE INSTALLED.\n" +
                "    Set HERO2_KEYSTORE / HERO2_KEYSTORE_PASSWORD / HERO2_KEY_ALIAS / HERO2_KEY_PASSWORD,\n" +
                "    or hero2.keystore / hero2.keystorePassword / hero2.keyAlias / hero2.keyPassword in\n" +
                "    %USERPROFILE%\\.gradle\\gradle.properties. See build_android.ps1 -NewAlias." +
                (if (ksPath != null && ksFile?.exists() != true) "\n    (configured keystore not found: $ksPath)" else "")
        )
    }
}

dependencies {
    // Provides the com.google.androidgamesdk.GameActivity Java class whose
    // native methods are implemented inside libmain.so (android-activity 0.6.1
    // bundles the matching GameActivity 4.4.0 C++).
    implementation("androidx.games:games-activity:4.4.0")
    // GameActivity 4.4.0 extends androidx.appcompat.app.AppCompatActivity, so
    // appcompat (which brings androidx.core) must be on the classpath and the
    // activity theme must be a Theme.AppCompat descendant.
    implementation("androidx.appcompat:appcompat:1.7.0")
}
