# Build the wie_android APK (Rust cdylib via cargo-ndk -> Gradle GameActivity APK).
# Captures every host-toolchain quirk discovered while bringing this up on Windows.
#
#   .\build_android.ps1                 # debug APK, cargo `dev` profile (see -FastDebug)
#   .\build_android.ps1 -FastDebug      # debug-SIGNED APK with RELEASE native code -
#                                       #   for a phone whose install is debug-signed (S9):
#                                       #   same key, same save, none of the dev-profile tax
#   .\build_android.ps1 -Release        # release (LTO) + versionCode bump + signing
#   .\build_android.ps1 -Install        # also adb-install + launch on the device
#   .\build_android.ps1 -NewKeystore    # create the release keystore, once, then exit
#   .\build_android.ps1 -Release -NoBump # rebuild the SAME versionCode (re-sign / retry)
#
# Every build also copies its APK into dist\ with the version in the name
# (hero2_ktf1.07_0.<versionCode>.0[_debug].apk) - see section 3c and
# .claude\skills\play-release\SKILL.md. Hand out THAT file, not app-release.apk.
#
# ── RELEASE SIGNING (read this before handing the APK to anyone) ──────────────
# An unsigned APK cannot be installed. Debug builds are auto-signed with a
# per-machine throwaway key, which is fine for your own phone and useless for
# distribution. `-Release` signs with YOUR key, which the build reads from outside
# this repo:
#     HERO2_KEYSTORE, HERO2_KEYSTORE_PASSWORD, HERO2_KEY_ALIAS, HERO2_KEY_PASSWORD
# (or hero2.keystore / hero2.keystorePassword / hero2.keyAlias / hero2.keyPassword
# in %USERPROFILE%\.gradle\gradle.properties, which is the persistent option).
#
# ★ 2026-09-01: this project's alias is `hero2` (DN `CN=Hero2 WIE`) in the SHARED
#   keystore, alongside the owner's other projects.
#   Until then the build read the hero3.* keys, i.e. it signed com.hero2remake.app
#   with ANOTHER app's key. Use -NewAlias to add a project alias to an existing
#   keystore; -NewKeystore only creates a keystore that does not exist yet.
#
# ★ THE KEY IS THE APP'S IDENTITY. Android will only update an installed app with
#   an APK signed by the SAME key. Lose it and every existing install is a dead end:
#   users must uninstall to take a new build, and uninstalling deletes their saves
#   (app-private storage). Back the .jks up somewhere that survives this machine.
param(
    [switch]$Release,
    # ★★ DEBUG-SIGNED APK, RELEASE-SPEED NATIVE CODE (2026-09-18).
    #
    # A plain debug APK builds the Rust with cargo's `dev` profile, and this project's
    # `[profile.dev]` sets only `opt-level = 3` - so the build that ships in it still has
    # `overflow-checks` and `debug-assertions` ON, `lto` OFF and `codegen-units = 256`.
    # For an ARM interpreter whose hot loop spans four crates that is not a small
    # difference, and the phone with the debug install (S9) is exactly the one the player
    # reported lag and heat on.
    #
    # The obvious fix - install the release APK - is not available there: the S9's install
    # is DEBUG-SIGNED, so a release-signed APK cannot update it and Android would require
    # an uninstall, which takes the save with it. This switch is the way out: cargo builds
    # `--release`, gradle still assembles (and debug-signs) the debug APK. Same signature,
    # same save, release-speed native code.
    [switch]$FastDebug,
    [switch]$Install,
    [switch]$NewKeystore,
    # Add THIS project's alias to an already-existing keystore. -NewKeystore refuses
    # to touch a file that exists (it would orphan every install signed from it), so
    # before this switch a shared keystore could only gain an alias by hand.
    [switch]$NewAlias,
    # Which phone -Install targets. Overrides everything; $env:ANDROID_SERIAL is the
    # other way in. With two phones attached this script REFUSES to pick one (see the
    # install block), so this is how you say which.
    [string]$Serial,
    [switch]$NoBump,
    # Accepted and IGNORED. The app is offline-only (§0b), so there is no
    # destination to opt out of; kept so older invocations still parse.
    [switch]$LocalServer
)
$ErrorActionPreference = 'Stop'

# --- tool locations ---------------------------------------------------------
# ★ Each is a CANDIDATE LIST, not an absolute path (2026-08-05). These were three
# absolute paths into one user profile, so a second machine could not run this script
# at all — the SDK, the JDK and adb were all "not found" before the first line of work.
# $env:ANDROID_HOME wins; otherwise the standard per-user location. There used to be a
# third candidate spelling out one developer's home directory — redundant, since
# "$env:LOCALAPPDATA\Android\Sdk" IS that path on that machine (2026-09-04, 순서표 #6).
$Sdk = @(
    $env:ANDROID_HOME
    "$env:LOCALAPPDATA\Android\Sdk"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $Sdk) { throw 'Android SDK not found. Set $env:ANDROID_HOME.' }
$Ndk = "$Sdk\ndk\25.1.8937393"
$NdkBin = "$Ndk\toolchains\llvm\prebuilt\windows-x86_64\bin"
if (-not (Test-Path $Ndk)) { throw "NDK 25.1.8937393 not found under $Sdk\ndk (gradle's ndkVersion pins this exact revision)." }
$env:ANDROID_HOME = $Sdk
$env:ANDROID_NDK_HOME = $Ndk
# Android Studio's bundled JBR is a real JDK 17+ and is present wherever Studio is, so
# it is the fallback rather than a second download.
$env:JAVA_HOME = @(
    $env:JAVA_HOME
    'C:\Program Files\Microsoft\jdk-21.0.11.10-hotspot'
    'C:\Program Files\Android\Android Studio\jbr'
) | Where-Object { $_ -and (Test-Path (Join-Path $_ 'bin\java.exe')) } | Select-Object -First 1
if (-not $env:JAVA_HOME) { throw 'No JDK found. Set $env:JAVA_HOME.' }
$Adb = @(
    $env:HERO2_ADB
    $env:HERO3_ADB   # accepted for continuity with the sibling projects on this machine
    "$Sdk\platform-tools\adb.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($Install -and -not $Adb) { throw 'adb not found. Set $env:HERO2_ADB.' }

# --- dlltool shim -----------------------------------------------------------
# Building for android on this windows-gnu host pulls host build-deps
# (cc -> windows-sys, getrandom) whose raw-dylib import libs need `dlltool.exe`.
# The bundled GNU dlltool needs a separate `as` the toolchain doesn't ship, so
# shim the default name to llvm-dlltool (self-contained). target.<host>.rustflags
# does NOT reach build-deps, hence a PATH shim.
#
# ★ 2026-08-05: copying llvm-dlltool to `dlltool.exe` is no longer enough, on ANY
#   toolchain. rustc chooses the GNU flag set from that file name and one of those
#   flags (`--temp-prefix`) makes llvm-dlltool print its usage and exit 1 — no import
#   lib, and the build dies with a misleading "failed to add native library
#   ...dll_imports.lib: file not found". Newer rustup also stopped shipping
#   llvm-dlltool here at all (`self-contained` now has the GNU one, still without the
#   `as` it needs). So the shim is a real WRAPPER, built from `tools/dlltool_shim.rs`,
#   which drops the GNU-only flags and forwards. See that file for the measurement.
$Shim = Join-Path $env:TEMP 'wie-dlltool'
New-Item -ItemType Directory -Force $Shim | Out-Null
$RealDlltool = Join-Path $Shim 'real-llvm-dlltool.exe'
$llvmDlltool = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-dlltool.exe"
if (Test-Path $llvmDlltool) {
    Copy-Item $llvmDlltool $RealDlltool -Force
} elseif (Test-Path "$NdkBin\llvm-ar.exe") {
    # llvm-ar/llvm-lib/llvm-ranlib/llvm-dlltool are ONE multicall binary: it picks its
    # personality from argv[0], so a copy named llvm-dlltool IS llvm-dlltool. It loads
    # its DLLs from the NDK bin dir, which is why that goes on PATH below.
    Copy-Item "$NdkBin\llvm-ar.exe" $RealDlltool -Force
} else {
    throw "No llvm-dlltool available (looked in the rustup gnu toolchain and $NdkBin)."
}
& "$env:USERPROFILE\.cargo\bin\rustc.exe" -O --edition 2021 "$PSScriptRoot\tools\dlltool_shim.rs" -o (Join-Path $Shim 'dlltool.exe')
if ($LASTEXITCODE -ne 0) { throw "could not build the dlltool shim ($LASTEXITCODE)" }
$env:WIE_REAL_DLLTOOL = $RealDlltool
# $NdkBin goes LAST: it is there so the multicall llvm binary above can find its own
# DLLs, and putting it first would let the NDK's cross tools shadow host ones.
$env:PATH = "$Shim;$env:USERPROFILE\.cargo\bin;$env:JAVA_HOME\bin;$env:PATH;$NdkBin"

$JniLibs = "$PSScriptRoot\android_app\app\src\main\jniLibs"
$VersionFile = "$PSScriptRoot\android_app\version.properties"
$GameAsset = "$PSScriptRoot\android_app\app\src\main\assets\hero2_ktf.zip"

# --- The bundled game archive, checked BEFORE anything expensive -------------
# `android_app/app/src/main/assets/*.zip` is gitignored (it is the licensed game data), so a
# fresh clone has no assets directory at all. Gradle packages that happily, and the APK then
# builds, SIGNS, INSTALLS - and dies on the emulator's first line:
#
#     E/wie: main: bundled game asset hero2_ktf.zip not found; aborting
#
# ★ 2026-08-08: exactly that shipped to the player on a second machine. The build said
# "BUILD SUCCESSFUL" and apksigner verified it, so nothing upstream of the phone noticed. An
# APK that cannot start is worse than no APK, and this is the one input the repo cannot carry
# itself, so it is a hard failure rather than a warning.
if (-not (Test-Path $GameAsset)) {
    Write-Warning "The game archive is gitignored, so a clone does not come with it."
    Write-Warning "Copy hero2_ktf.zip to: $GameAsset"
    throw "missing bundled game archive: $GameAsset"
}
Write-Output ("==> game asset: hero2_ktf.zip ({0:N0} bytes)" -f (Get-Item $GameAsset).Length)

# --- -NewKeystore: create the release key, once -----------------------------
# Deliberately a separate switch and not something a build does implicitly: this
# generates a secret whose loss is unrecoverable (see the header). Default location
# is OUTSIDE the repo, under the user profile, so no clone or zip of this project
# can ever carry it.
# ── -NewAlias: add THIS project's key to a keystore that already exists ──────
# The machine convention is ONE keystore file with one alias per project
# (one alias per project). -NewKeystore
# cannot serve that: it hard-throws on an existing file, correctly, because creating a
# keystore over one that exists orphans every install signed from it. So adding an
# alias had to be done by hand with keytool, which is how hero2NativeAPP shipped a
# build.gradle.kts reading hero3's alias for four weeks. This is that path, scripted.
if ($NewAlias) {
    # PS 5.1: keytool writes progress to STDERR even when it succeeds, and with
    # $ErrorActionPreference='Stop' (set at the top of this script) that becomes a
    # TERMINATING error on exit code 0 - the same trap the dlltool section documents.
    # Every keytool call below checks $LASTEXITCODE itself, so 'Stop' buys nothing here.
    $ErrorActionPreference = 'Continue'
    $ks = if ($env:HERO2_KEYSTORE) { $env:HERO2_KEYSTORE } else { "$env:USERPROFILE\.android\keystores\hero2-release.jks" }
    if (-not (Test-Path $ks)) { throw "no such keystore: $ks (use -NewKeystore to create one)" }
    if (-not $env:HERO2_KEYSTORE_PASSWORD -or -not $env:HERO2_KEY_ALIAS -or -not $env:HERO2_KEY_PASSWORD) {
        throw @"
Set the three secrets first, then re-run:
  `$env:HERO2_KEYSTORE_PASSWORD = '<store password of the EXISTING keystore>'
  `$env:HERO2_KEY_ALIAS         = 'hero2'
  `$env:HERO2_KEY_PASSWORD      = '<key password>'   # may equal the store password
  .\build_android.ps1 -NewAlias
"@
    }
    # Refuse to touch an alias that exists: -genkeypair onto a live alias is how a
    # signing identity gets silently replaced.
    $existing = & "$env:JAVA_HOME\bin\keytool.exe" -list -keystore $ks -storetype PKCS12 -storepass $env:HERO2_KEYSTORE_PASSWORD 2>&1
    if ($LASTEXITCODE -ne 0) { throw "cannot open $ks - wrong store password?" }
    if ($existing -match "(?m)^$([regex]::Escape($env:HERO2_KEY_ALIAS)),") {
        throw "alias '$($env:HERO2_KEY_ALIAS)' already exists in $ks (refusing to replace a signing identity)"
    }
    # A keystore is a secret whose loss is unrecoverable, so snapshot before writing.
    $bak = "$ks.bak-before-$($env:HERO2_KEY_ALIAS)"
    Copy-Item $ks $bak -Force
    Write-Output "==> backed up: $bak"
    Write-Output "==> adding alias '$($env:HERO2_KEY_ALIAS)' to $ks"
    & "$env:JAVA_HOME\bin\keytool.exe" -genkeypair -v `
        -keystore $ks -storetype PKCS12 -storepass $env:HERO2_KEYSTORE_PASSWORD `
        -alias $env:HERO2_KEY_ALIAS -keypass $env:HERO2_KEY_PASSWORD `
        -keyalg RSA -keysize 4096 -validity 10000 `
        -dname "CN=Hero2 WIE, OU=side project, O=self, C=KR"
    if ($LASTEXITCODE -ne 0) { throw "keytool failed ($LASTEXITCODE)" }
    Write-Output ''
    Write-Output "==> DONE. Make it permanent in %USERPROFILE%\.gradle\gradle.properties:"
    Write-Output "      hero2.keystore=$($ks -replace '\\','/')"
    Write-Output "      hero2.keystorePassword=<store password>"
    Write-Output "      hero2.keyAlias=$env:HERO2_KEY_ALIAS"
    Write-Output "      hero2.keyPassword=<key password>"
    Write-Output ''
    Write-Output "==> AND BACK UP $ks somewhere off this machine."
    return
}

if ($NewKeystore) {
    # PS 5.1: keytool writes progress to STDERR even when it succeeds, and with
    # $ErrorActionPreference='Stop' (set at the top of this script) that becomes a
    # TERMINATING error on exit code 0 - the same trap the dlltool section documents.
    # Every keytool call below checks $LASTEXITCODE itself, so 'Stop' buys nothing here.
    $ErrorActionPreference = 'Continue'
    $ksDir = "$env:USERPROFILE\.android\keystores"
    $ks = if ($env:HERO2_KEYSTORE) { $env:HERO2_KEYSTORE } else { "$ksDir\hero2-release.jks" }
    if (Test-Path $ks) { throw "keystore already exists: $ks (refusing to overwrite - that would orphan every install; use -NewAlias to add an alias to it)" }
    if (-not $env:HERO2_KEYSTORE_PASSWORD -or -not $env:HERO2_KEY_ALIAS -or -not $env:HERO2_KEY_PASSWORD) {
        throw @"
Set the three secrets first, then re-run:
  `$env:HERO2_KEYSTORE_PASSWORD = '<store password>'
  `$env:HERO2_KEY_ALIAS         = 'hero2'
  `$env:HERO2_KEY_PASSWORD      = '<key password>'   # may equal the store password
  .\build_android.ps1 -NewKeystore
"@
    }
    New-Item -ItemType Directory -Force $ksDir | Out-Null
    Write-Output "==> creating release keystore: $ks"
    # PKCS12, not JKS: JKS is Java's proprietary legacy format and keytool nags to
    # migrate off it on every use. Android/apksigner read PKCS12 natively.
    # 10000 days ~= 27 years. Play rejects keys that expire before 2033-10-22, and a
    # key that expires is a key you cannot ship updates with afterwards.
    & "$env:JAVA_HOME\bin\keytool.exe" -genkeypair -v `
        -keystore $ks -storetype PKCS12 -storepass $env:HERO2_KEYSTORE_PASSWORD `
        -alias $env:HERO2_KEY_ALIAS -keypass $env:HERO2_KEY_PASSWORD `
        -keyalg RSA -keysize 4096 -validity 10000 `
        -dname "CN=Hero2 WIE, OU=side project, O=self, C=KR"
    if ($LASTEXITCODE -ne 0) { throw "keytool failed ($LASTEXITCODE)" }
    Write-Output ''
    Write-Output "==> DONE. Now make it permanent (this file is outside the repo):"
    Write-Output "    %USERPROFILE%\.gradle\gradle.properties"
    Write-Output "      hero2.keystore=$($ks -replace '\\','/')"
    Write-Output "      hero2.keystorePassword=<store password>"
    Write-Output "      hero2.keyAlias=$env:HERO2_KEY_ALIAS"
    Write-Output "      hero2.keyPassword=<key password>"
    Write-Output ''
    Write-Output "==> AND BACK UP $ks somewhere off this machine."
    Write-Output "    Without it you can never update an installed copy - users would have to"
    Write-Output "    uninstall, which deletes their saves."
    return
}

# --- 0a) versionCode bump ---------------------------------------------------
# Only on -Release, so debug builds and IDE syncs leave it alone. Android refuses an
# APK whose versionCode is not higher than the installed one, and Play refuses one
# that was already uploaded - so a hand-edited constant is a trap you hit at the
# worst moment (handing someone a build that "won't install").
if ($Release -and -not $NoBump) {
    if (-not (Test-Path $VersionFile)) { throw "missing $VersionFile" }
    $lines = [System.IO.File]::ReadAllLines($VersionFile)
    $codeLine = $lines | Where-Object { $_ -match '^\s*versionCode\s*=' }
    if (-not $codeLine) { throw "no versionCode= line in $VersionFile" }
    $cur = [int]($codeLine -replace '.*=\s*', '')
    $next = $cur + 1
    $vname = (($lines | Where-Object { $_ -match '^\s*versionName\s*=' }) -replace '.*=\s*', '')
    # ★ NOT `Set-Content -Encoding utf8`: on Windows PowerShell 5.1 that writes a BOM
    # and re-encodes the file through the console codepage, which mangles anything
    # non-ASCII in the comments (caught doing exactly that). Write the bytes back the
    # way they came in.
    [System.IO.File]::WriteAllLines(
        $VersionFile,
        ($lines -replace '^\s*versionCode\s*=.*', "versionCode=$next"),
        (New-Object System.Text.UTF8Encoding($false)))
    Write-Output "==> versionCode $cur -> $next  (versionName $vname)"
}

# --- 0b) OFFLINE-ONLY: no server is baked into this app ---------------------
#
# * 2026-08-20, player decision: this game ships with NO network features. The
# cash shop, the shared warehouse and everything else hero3 reached over a private
# server are to be implemented LOCALLY, so every piece of content works with the
# phone in airplane mode.
#
# What this block used to do was RESOLVE a private-server address (env, then
# %USERPROFILE%\.gradle\gradle.properties) and bake it into libmain.so through
# `option_env!`. Those properties still hold hero3's server on this machine, so a
# hero2 build silently inherited `<hero3-server>` - an address that has
# nothing to do with this game. So the variables are CLEARED, not merely left
# unset: `option_env!` reads the process environment and an inherited value is
# exactly the failure this script exists to make impossible.
#
# The same resolution still runs, but only to learn WHAT this machine could have
# baked. Those strings become the forbidden list section 2b proves absent from the
# built .so. Searching for a generic marker instead does not work: "tls://" is a
# plain literal in app.rs/network.rs and is in every build no matter what.
$GradleProps = "$env:USERPROFILE\.gradle\gradle.properties"
function Get-Hero3Prop([string]$name) {
    if (-not (Test-Path $GradleProps)) { return $null }
    $line = Get-Content $GradleProps | Where-Object { $_ -match "^\s*$([regex]::Escape($name))\s*=" } | Select-Object -Last 1
    if (-not $line) { return $null }
    $v = ($line -split '=', 2)[1].Trim()
    if ($v) { $v } else { $null }
}
$Forbidden = @(
    $env:HERO3_SERVER_HOST
    $env:HERO3_SERVER_PIN
    Get-Hero3Prop 'hero3.serverHost'
    Get-Hero3Prop 'hero3.serverPin'
) | Where-Object { $_ } | Select-Object -Unique
$env:HERO3_SERVER_HOST = $null
$env:HERO3_SERVER_PIN = $null
$env:HERO3_SERVER_PORT = $null
if ($LocalServer) { Write-Warning '==> -LocalServer is a no-op now: every build is offline.' }
if ($Forbidden) {
    Write-Output "==> offline build: nothing baked in. $($Forbidden.Count) configured destination(s) will be proven ABSENT from the .so."
} else {
    Write-Output '==> offline build: no server destination is configured anywhere, and none is baked in.'
}

# --- 1..2b) Rust cdylib -> strip -> prove the build is OFFLINE ---------------
# One retry loop, because the three steps are one question: "is the .so that
# reached jniLibs free of a baked server destination?"
#
# * (1) cargo-ndk builds the cdylib.
# * (2) strip, or a debug .so carries ~266 MB of debuginfo.
# * (2b) THE EVIDENCE, inverted for the offline build (2026-08-20). Clearing
#   HERO3_SERVER_* in section 0b is intent; this is proof. `option_env!` is
#   resolved by rustc and its value is NOT part of cargo's fingerprint, so a
#   tree built earlier WITH a destination rebuilds in 0.4 s and keeps the old
#   string. That is not hypothetical: the first offline build on this machine
#   did exactly that and this check caught it. `tls://` is what every baked
#   endpoint carries; the "127.0.0.1" fallback is always present (the `_ =>`
#   arm in `build_time_endpoint`) so its absence would prove nothing.
#
# On failure the fingerprint is the only plausible cause, so the loop cleans the
# crate and builds once more rather than making the caller do it. A second
# failure is a real problem and throws.
# ⚠ PS 5.1, and the reason this script could not be run by anything that captures
# stderr (measured 2026-09-04). With $ErrorActionPreference='Stop', a native exe
# writing to stderr becomes a TERMINATING error even on exit code 0 — and `cargo ndk`
# announces "Building arm64-v8a (aarch64-linux-android)" on stderr as ordinary
# progress. So the build died at its first successful step. build_probe.ps1 already
# documents and dodges this trap; the setup above keeps 'Stop' (its cmdlets want it),
# and everything from here on checks $LASTEXITCODE for itself.
$ErrorActionPreference = 'Continue'

$SoPath = Join-Path $JniLibs (Join-Path 'arm64-v8a' 'libmain.so')
for ($attempt = 1; ; $attempt++) {
    Write-Output "==> cargo-ndk build ($(if ($Release -or $FastDebug) {'release'} else {'debug'}))$(if ($FastDebug) {' [FastDebug: release native code in a debug-signed APK]'})"
    Push-Location "$PSScriptRoot\wie_android"
    try {
        $ndkArgs = @('-t', 'arm64-v8a', '--platform', '26', '-o', $JniLibs, 'build')
        if ($Release -or $FastDebug) { $ndkArgs += '--release' }
        & cargo ndk @ndkArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo ndk failed ($LASTEXITCODE)" }
    } finally { Pop-Location }

    Write-Output '==> strip libmain.so'
    & "$NdkBin\llvm-strip.exe" --strip-all $SoPath
    # Checked because 'Continue' above means a failure here would otherwise be a
    # warning on the way to shipping a 266 MB debuginfo .so.
    if ($LASTEXITCODE -ne 0) { throw "llvm-strip failed ($LASTEXITCODE)" }

    $ascii = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($SoPath))
    $found = @($Forbidden | Where-Object { $ascii.Contains($_) })
    if (-not $found) {
        Write-Output '==> verified in libmain.so: no baked server destination'
        break
    }
    if ($attempt -ge 2) {
        throw "libmain.so STILL contains $($found -join ', ') after a clean rebuild. Something is exporting HERO3_SERVER_HOST into this process - this app is offline-only and must not ship a destination."
    }
    Write-Warning "==> libmain.so carries $($found -join ', ') (stale cargo fingerprint - option_env! is not fingerprinted)."
    Write-Warning '    cleaning wie_android and rebuilding once.'
    Push-Location "$PSScriptRoot\wie_android"
    try { & cargo clean -p wie_android --target aarch64-linux-android } finally { Pop-Location }
}

# --- 3) Gradle APK ----------------------------------------------------------
$task = if ($Release) { ':app:assembleRelease' } else { ':app:assembleDebug' }
Write-Output "==> gradle $task"
Push-Location "$PSScriptRoot\android_app"
try {
    & .\gradlew.bat $task --no-daemon
    if ($LASTEXITCODE -ne 0) { throw "gradle failed ($LASTEXITCODE)" }
} finally { Pop-Location }

# ★ The release output is named `app-release.apk` when it is SIGNED and
# `app-release-unsigned.apk` when it is not, so the filename itself is the answer to
# "can anyone install this?". Take whichever exists and say which one it was.
$apk = if ($Release) {
    $signed = "$PSScriptRoot\android_app\app\build\outputs\apk\release\app-release.apk"
    $unsigned = "$PSScriptRoot\android_app\app\build\outputs\apk\release\app-release-unsigned.apk"
    if (Test-Path $signed) { $signed } else { $unsigned }
} else {
    "$PSScriptRoot\android_app\app\build\outputs\apk\debug\app-debug.apk"
}
Write-Output "==> APK: $apk"
Get-Item $apk | Select-Object Length, LastWriteTime

# --- 3b) prove it is signed, and with WHICH key -----------------------------
# Not decoration: the failure mode is silent (an APK that simply refuses to install
# on someone else's phone, or that Android rejects as an update because the key
# changed). The certificate digest below is the app's identity - it must be the SAME
# string on every build you ever hand out.
$bt = Get-ChildItem "$Sdk\build-tools" -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
if ($Release) {
    $apksigner = if ($bt) { Join-Path $bt.FullName 'apksigner.bat' } else { $null }
    if ($apksigner -and (Test-Path $apksigner)) {
        Write-Output '==> apksigner verify'
        & $apksigner verify --print-certs $apk 2>&1 |
            Select-String -Pattern 'SHA-256 digest|DOES NOT VERIFY|Signer #1 certificate DN' |
            ForEach-Object { "    $_" }
        if ($LASTEXITCODE -ne 0) {
            Write-Warning '    APK IS NOT SIGNED - it cannot be installed. See the header of this script.'
        }
    } else {
        Write-Warning "==> apksigner not found under $Sdk\build-tools; signature unverified"
    }
    if ($apk -like '*unsigned*') {
        Write-Warning '==> This APK is UNSIGNED. Do not hand it out; it will not install.'
    }
}

# --- 3c) the distribution copy ----------------------------------------------
# ★ 2026-09-25 (user: "apk뽑는 룰을 hero3처럼 해야해. 버전명같은것도 나와야하고"). gradle's
# `app-release.apk` / `app-debug.apk` carry no version, and every build overwrites them.
# Every build therefore also leaves a COPY in dist\ under hero3's naming rule:
#     dist\hero2_ktf1.07_0.<versionCode>.0.apk          (release - S25, anyone else)
#     dist\hero2_ktf1.07_0.<versionCode>.0_debug.apk    (debug-signed - the S9 only)
# The original stays where it is: this script reads signedness off its filename, and
# -Install below installs it. See .claude\skills\play-release\SKILL.md.
$nameApk = Join-Path $PSScriptRoot '.claude\skills\play-release\name_apk.ps1'
if (-not (Test-Path $nameApk)) {
    Write-Warning "==> dist copy skipped: $nameApk not found"
} elseif ($apk -like '*unsigned*') {
    Write-Warning '==> dist copy skipped: an unsigned APK is not handed out'
} else {
    try {
        if ($Release) { & $nameApk } else { & $nameApk -DebugApk }
    } catch {
        Write-Warning "==> dist copy failed: $_"
    }
}

# --- 4) optional install + launch ------------------------------------------
if ($Install) {
    Write-Output '==> adb install + launch'
    # Pick the target device.
    #
    # ⛔ WHY THIS REFUSES INSTEAD OF GUESSING (2026-09-04). Two phones are routinely
    # attached to this machine and the user's standing instruction is that ONE of them
    # is off limits. This block used to take `$ready[0]` — whichever `adb devices`
    # happened to list first — and, with no device ready, fell back to a HARDCODED
    # serial that was the off-limits phone. Installing a build onto the wrong handset
    # is not recoverable by re-running the command, so ambiguity is an error here.
    # Order:
    #   1. -Serial, then $env:ANDROID_SERIAL (adb reads that one itself)
    #   2. the only device in `device` state — the ordinary one-phone case
    #   3. refuse, printing a ready-to-paste line per candidate
    if ($Serial) { $env:ANDROID_SERIAL = $Serial }
    if (-not $env:ANDROID_SERIAL) {
        $ready = @(& $Adb devices -l | Select-String -Pattern '^(\S+)\s+device\s' | ForEach-Object {
            $line = $_.Line
            $model = if ($line -match 'model:(\S+)') { $Matches[1] } else { '?' }
            [pscustomobject]@{ Serial = $_.Matches.Groups[1].Value; Model = $model }
        })
        if ($ready.Count -eq 1) {
            $env:ANDROID_SERIAL = $ready[0].Serial
        } elseif ($ready.Count -gt 1) {
            # The candidate list goes INSIDE the throw, not to Write-Output before it:
            # a terminating error loses buffered pipeline output, so the one message
            # the caller actually needs would vanish exactly when it is needed.
            $lines = $ready | ForEach-Object { "      $($_.Model)  ->  .\build_android.ps1 -Install -Serial $($_.Serial)" }
            throw @"
$($ready.Count) devices ready - refusing to choose which phone gets the build. Name one:
$($lines -join "`n")
   (or set `$env:ANDROID_SERIAL once for the shell session)
"@
        } else {
            throw 'no device in `device` state (unauthorized? accept the USB debugging prompt).'
        }
    }
    Write-Output "    device $env:ANDROID_SERIAL"
    & $Adb install -r $apk
    if ($LASTEXITCODE -ne 0) { throw "adb install failed ($LASTEXITCODE) on $env:ANDROID_SERIAL" }
    & $Adb shell input keyevent KEYCODE_HOME
    # ★ Component DERIVED from the APK, not hardcoded. `applicationId` and the Java
    # package are different now (com.hero2remake.app vs net.wie.emulator), so
    # `am start -n <applicationId>/.MainActivity` — which is what this used to do —
    # names a class that does not exist. aapt2 reports what actually shipped.
    $badging = if ($bt) { & (Join-Path $bt.FullName 'aapt2.exe') dump badging $apk 2>$null } else { $null }
    $pkg = ($badging | Select-String -Pattern "^package: name='([^']+)'").Matches.Groups[1].Value
    $act = ($badging | Select-String -Pattern "^launchable-activity: name='([^']+)'").Matches.Groups[1].Value
    if ($pkg -and $act) {
        Write-Output "    launching $pkg/$act"
        & $Adb shell am start -n "$pkg/$act"
    } else {
        Write-Warning '    could not read the launch component from the APK; start it by hand'
    }
}
