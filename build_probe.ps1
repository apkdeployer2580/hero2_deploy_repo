# Host build of wie_probe — the headless verification loop CLAUDE.md documents.
#
# ── WHY THIS EXISTS ──────────────────────────────────────────────────────────
# `cargo build --release -p wie_probe` does NOT work on a stock windows-gnu host:
# rustc materialises `raw-dylib` imports (parking_lot_core -> windows-targets) by
# shelling out to `dlltool`, which the bundled gnu toolchain does not surface on
# PATH. Measured 2026-08-28:
#
#     error: error calling dlltool 'dlltool.exe': program not found
#     error: could not compile `parking_lot_core` (lib)
#
# build_android.ps1 already solves this for the ANDROID build, but the shim it
# builds lives inside that script, so the host/probe build never saw it. This is
# the same block, hoisted so the headless loop works too. See tools/dlltool_shim.rs
# for why a renamed llvm-dlltool is not enough and `self-contained` is not either.
param([switch]$Run, [int]$Frames = 1200)

$ErrorActionPreference = 'Stop'

# ★ 2026-09-04: the rustup gnu toolchain STOPPED shipping llvm-dlltool.exe — its
# `lib/rustlib/x86_64-pc-windows-gnu/bin` now holds only rust-lld / rust-objcopy plus
# a `self-contained/dlltool.exe` (the GNU one, which needs an `as` the toolchain does
# not ship). So the lookup needs the same NDK fallback build_android.ps1 already has;
# without it this script threw before cargo was ever reached.
$Sdk = @(
    $env:ANDROID_HOME
    "$env:LOCALAPPDATA\Android\Sdk"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
$NdkBin = if ($Sdk) { "$Sdk\ndk\25.1.8937393\toolchains\llvm\prebuilt\windows-x86_64\bin" } else { $null }

$Shim = Join-Path $env:TEMP 'wie-dlltool'
New-Item -ItemType Directory -Force $Shim | Out-Null
$RealDlltool = Join-Path $Shim 'real-llvm-dlltool.exe'
$llvmDlltool = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-dlltool.exe"
if (Test-Path $llvmDlltool) {
    Copy-Item $llvmDlltool $RealDlltool -Force
} elseif ($NdkBin -and (Test-Path "$NdkBin\llvm-ar.exe")) {
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
# $NdkBin goes LAST so the multicall llvm binary can find its own DLLs without the
# NDK's cross tools shadowing host ones.
$env:PATH = "$Shim;$env:USERPROFILE\.cargo\bin;$env:PATH" + $(if ($NdkBin) { ";$NdkBin" } else { '' })

# ⚠ PS 5.1: with $ErrorActionPreference='Stop', a native exe writing to stderr becomes
# a TERMINATING error even on exit 0 — so `.\build_probe.ps1 | Tee-Object log.txt` died
# on cargo's ordinary dead_code warnings. Every native call below checks $LASTEXITCODE
# itself, so 'Stop' buys nothing here; drop it for the rest of the script.
$ErrorActionPreference = 'Continue'

& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release -p wie_probe
if ($LASTEXITCODE -ne 0) { throw "wie_probe build failed ($LASTEXITCODE)" }
Write-Output "==> target\release\wie_probe.exe"

if ($Run) {
    # READ-ONLY flags. Since 2026-09-04 the probe ENFORCES this itself: it identifies
    # the build by the md5 of the guest binary inside the jar, and hero2 has no anchors
    # in guest_anchors.rs, so --poke / --host-save / --host-levelup (and the offset
    # decoders) exit 3 instead of firing hero3 addresses into hero2 memory. --zip is
    # required and a misspelled flag exits 2, so this line cannot silently drift.
    # --w/--h stay explicit anyway: their defaults are hero3's 240x320.
    $Asset = "$PSScriptRoot\android_app\app\src\main\assets\hero2_ktf.zip"
    if (-not (Test-Path $Asset)) {
        throw "no game asset at $Asset — regenerate it with:`n  python tools_re/make_hero2_asset.py `"dist\영웅서기2-1.zip`""
    }
    # ★ 2026-09-25: the asset no longer carries the dump owner's saves (a fresh install
    # starts with empty slots — user decision). The baseline boots slot 1, so it loads them
    # from the LOCAL copy make_hero2_asset.py unpacks to dist\test_saves (gitignored).
    $Saves = "$PSScriptRoot\dist\test_saves"
    if (-not (Test-Path "$Saves\01035D0B\fs\g1")) {
        throw "no test saves at $Saves — regenerate the asset (it unpacks them):`n  python tools_re/make_hero2_asset.py `"dist\영웅서기2-1.zip`""
    }
    $Out = Join-Path $env:TEMP 'wie-probe-hero2'
    New-Item -ItemType Directory -Force $Out | Out-Null
    & "$PSScriptRoot\target\release\wie_probe.exe" `
        --zip $Asset --fs $Saves --w 176 --h 220 --ms 20 --frames $Frames `
        --keys 320:OK,420:OK,520:OK --out $Out --dump 560,$Frames
    Write-Output "==> frames in $Out"
}
