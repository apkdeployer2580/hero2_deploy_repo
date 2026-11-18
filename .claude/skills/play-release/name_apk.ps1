# Give the built APK its distribution name and drop a copy in dist\.
#
#   .\.claude\skills\play-release\name_apk.ps1           # release APK (S25, anyone else)
#   .\.claude\skills\play-release\name_apk.ps1 -DebugApk # debug-signed APK (S9 only)
#   .\.claude\skills\play-release\name_apk.ps1 -Aab      # Play bundle
#
# build_android.ps1 runs this by itself at the end of every build, so normally there is
# nothing to type. It exists as its own script for the same reason as hero3's: re-naming
# an existing build without rebuilding it.
#
# Naming rule (user, 2026-09-25: "apk뽑는 룰을 hero3처럼 해야해. 버전명같은것도 나와야하고")
# = hero3's rule (hero3NativeAPP .claude\skills\play-release\SKILL.md, section 1-b) with
# hero2's own label:
#
#     hero2_ktf1.07_0.<versionCode>.0.apk           release (signed with alias hero2)
#     hero2_ktf1.07_0.<versionCode>.0_debug.apk     debug-signed (-DebugApk)
#
# gradle always writes `app-release.apk` / `app-debug.apk`, so every build overwrites the
# last one and nothing in the filename says which build it is. Handing that file over is
# how "which version are you running?" becomes unanswerable.

[CmdletBinding()]
param(
    # Name the debug-signed APK instead (the S9's install is debug-signed; a release APK
    # cannot update it). The name carries `_debug` so it is never mistaken for a hand-out.
    # (Not `-Debug`: [CmdletBinding()] already owns that common parameter.)
    [switch]$DebugApk,
    # Build the Play bundle name instead of the APK.
    [switch]$Aab,
    # Write the copy somewhere other than dist\.
    [string]$OutDir,
    # Allow an unsigned artifact through. Only for inspecting a build you will not hand out.
    [switch]$AllowUnsigned
)

$ErrorActionPreference = 'Stop'

# -- the one fixed field -------------------------------------------------------
# The carrier build the app carries: KTF 1.0.7 (`__adf__` says `Ver:01.00.07`, the store
# text says "버전:1.0.7"), written the way hero3 writes its `lgt1.04`. A fixed string, not
# read from the asset at build time - change this line, and only this line, if it moves.
$CarrierLabel = 'ktf1.07'
# The release key's certificate: alias `hero2`, `CN=Hero2 WIE` (CLAUDE.md, signing table).
$ExpectedDigest = 'f6ec47cb4717874fe1d16e197763401a67c99e06ed6537b68cff04d46d38e922'

$Repo = Resolve-Path (Join-Path $PSScriptRoot '..\..\..')

# -- versionCode: read it, never assume it ------------------------------------
# `build_android.ps1 -Release` bumps it on every release build, so a number anyone
# remembered is already stale.
$VersionFile = Join-Path $Repo 'android_app\version.properties'
if (-not (Test-Path $VersionFile)) { throw "version.properties not found: $VersionFile" }
$versionCode = $null
$versionName = $null
foreach ($line in (Get-Content $VersionFile)) {
    if ($line -match '^\s*versionCode\s*=\s*(\d+)\s*$') { $versionCode = $Matches[1] }
    if ($line -match '^\s*versionName\s*=\s*(\S+)\s*$') { $versionName = $Matches[1] }
}
if (-not $versionCode) { throw "no versionCode in $VersionFile" }

if ($DebugApk -and $Aab) { throw '-DebugApk and -Aab do not go together (the Play bundle is release-only)' }
$ext = if ($Aab) { 'aab' } else { 'apk' }
$variant = if ($DebugApk) { 'debug' } else { 'release' }
$outDirBuild = if ($Aab) {
    Join-Path $Repo 'android_app\app\build\outputs\bundle\release'
} else {
    Join-Path $Repo "android_app\app\build\outputs\apk\$variant"
}
$src = Join-Path $outDirBuild "app-$variant.$ext"

if (-not (Test-Path $src)) {
    # build_android.ps1 reads SIGNEDNESS OFF THE FILENAME - `app-release.apk` is signed,
    # `app-release-unsigned.apk` is not. Renaming gradle's own output in place destroys
    # that signal and breaks a later -Install, which is why this script COPIES.
    $found = @(Get-ChildItem $outDirBuild -Filter "*.$ext" -File -ErrorAction SilentlyContinue)
    if ($found.Count -eq 1) {
        $src = $found[0].FullName
        Write-Warning "gradle's output was renamed in place to '$($found[0].Name)'."
        Write-Warning "Leave it as app-$variant.$ext -- build_android.ps1 reads signedness off that name."
    } elseif ($found.Count -gt 1) {
        throw "several .$ext files in $outDirBuild -- cannot tell which is this build. Clean it out and rebuild."
    } else {
        throw "no $variant $ext in $outDirBuild -- run .\build_android.ps1 first"
    }
}

# An `-unsigned` artifact installs on nothing and updates nothing.
if ((Split-Path $src -Leaf) -like '*-unsigned*' -and -not $AllowUnsigned) {
    throw 'that artifact is UNSIGNED -- do not hand it out (see SKILL.md)'
}

$suffix = if ($DebugApk) { '_debug' } else { '' }
$name = "hero2_${CarrierLabel}_0.$versionCode.0$suffix.$ext"

if (-not $OutDir) { $OutDir = Join-Path $Repo 'dist' }
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Force $OutDir | Out-Null }
$dest = Join-Path $OutDir $name

# Copy, not move: gradle's own output stays where the build script and -Install
# expect to find it.
Copy-Item $src $dest -Force

$size = (Get-Item $dest).Length
$md5 = (Get-FileHash $dest -Algorithm MD5).Hash.ToLower()

Write-Host ""
Write-Host "==> $name"
Write-Host "    $dest"
Write-Host "    versionCode $versionCode  (versionName $versionName)  version 0.$versionCode.0"
Write-Host "    $('{0:N0}' -f $size) bytes   md5 $md5"

# -- the fingerprint is the app's identity -------------------------------------
# Printed here rather than trusted from the build log, because this script is what
# produces the file that actually gets handed over. The debug APK is signed with this
# machine's throwaway debug key on purpose, so it is reported but not compared.
if (-not $Aab) {
    $sdk = if ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT }
           elseif ($env:ANDROID_HOME) { $env:ANDROID_HOME }
           else { "$env:LOCALAPPDATA\Android\Sdk" }
    $signer = Get-ChildItem (Join-Path $sdk 'build-tools') -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'apksigner.bat' } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
    if ($signer) {
        $certs = & $signer verify --print-certs $dest
        $digest = @($certs | Select-String 'SHA-256 digest')
        if ($digest) { Write-Host "    $($digest -join "`n    ")" }
        if ($DebugApk) {
            Write-Host "    (debug key - installs only over a debug-signed copy, i.e. the S9)"
        } elseif (($digest -join ' ') -match $ExpectedDigest) {
            Write-Host "    release key OK (alias hero2)"
        } else {
            Write-Warning "    this is NOT the hero2 release key ($ExpectedDigest) -- do not hand it out"
        }
    } else {
        Write-Warning "apksigner not found under $sdk\build-tools -- fingerprint NOT checked"
    }
}
Write-Host ""
