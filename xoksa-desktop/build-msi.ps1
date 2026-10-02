<#
Builds the Windows MSI so that the binaries INSIDE it are byte-identical to the
ones that passed the shipping inspection.

WHY THIS SCRIPT EXISTS — do not replace it with plain `cargo tauri build`.

    Tauri's bundler patches a 3-byte marker into the desktop binary to record how
    it was packaged: `__TAURI_BUNDLE_TYPE_VAR_UNK` -> `..._MSI` for an MSI build.
    Those three ASCII characters are enough to flip Microsoft Defender's ML from
    clean to `Trojan:Win32/Wacatac.B!ml` — measured, same size, same sections,
    3 bytes different (docs/dev-prog/av-false-positive-case-study.md).

    `cargo tauri build` also rebuilds the exe with tauri-cli's own settings, which
    produces a different binary from the inspected `cargo build --release` one.

    So: build with cargo, let Tauri generate the WiX sources, then re-run WiX
    `light` with the UNPATCHED binary in place. `light` binds file contents at
    link time, so the MSI ends up carrying exactly what we inspected. The marker
    stays `UNK`, which only makes `tauri::app::bundle_type()` return None — an
    informational API this app never calls (no updater, no plugins).

The script FAILS if the MSI payload does not hash-match the binaries it built.

Usage:  pwsh -File build-msi.ps1            (from xoksa-desktop/)
Prereq: ../target/release/xoksa.exe must be the engine you intend to ship.
#>

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$repoRoot = (Resolve-Path "$PSScriptRoot\..").Path
$engine   = (Resolve-Path "$repoRoot\target\release\xoksa.exe").Path
$sidecar  = "$PSScriptRoot\binaries\xoksa-x86_64-pc-windows-msvc.exe"
$setupExe = "$repoRoot\xoksa-setup\target\release\xoksa-setup.exe"
$setupSc  = "$PSScriptRoot\binaries\xoksa-setup-x86_64-pc-windows-msvc.exe"
$deskExe  = "$PSScriptRoot\target\release\xoksa-desktop.exe"
$wixDir   = "$PSScriptRoot\target\release\wix\x64"
# The one canonical release folder for the MSI: the repo-root target\release (where
# the engine is), NOT the desktop crate's own target. Exactly one MSI lives here.
$outMsi   = "$repoRoot\target\release\XOKSA-2.9.5-x64.msi"
$rcedit   = "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\ElectronCommunity.rcedit_Microsoft.Winget.Source_8wekyb3d8bbwe\rcedit.exe"
$light    = "$env:LOCALAPPDATA\tauri\WixTools314\light.exe"

function Sha($p) { (Get-FileHash -Algorithm SHA256 $p).Hash.ToLower() }

# 1. Ship the engine that was inspected — the sidecar is easy to leave stale.
Copy-Item $engine $sidecar -Force
Write-Host "engine   : $(Sha $sidecar)"

# 1b. Build + stage the standalone settings app (xoksa-setup) as a second sidecar,
#     so the MSI installs xoksa-setup.exe beside the desktop and engine. Plain
#     cargo (no Tauri bundle-type marker), same clean-binary reasoning as step 2.
Push-Location ..\xoksa-setup
cargo build --release
if ($LASTEXITCODE -ne 0) { Pop-Location; throw 'xoksa-setup build failed' }
Pop-Location
Copy-Item $setupExe $setupSc -Force
$setupHash = Sha $setupSc
Write-Host "setup    : $setupHash"

# 2. Desktop binary: plain cargo, i.e. the build the 0/70 recipe was measured on.
cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

# 3. Descriptive FileDescription — tauri-build force-sets it to productName, so it
#    can only be applied post-build (see Cargo.toml's tauri-winres comment).
& $rcedit $deskExe --set-version-string FileDescription `
    'Stock analysis and news title triage desktop app using LLMs'
if ($LASTEXITCODE -ne 0) { throw 'rcedit failed' }

$deskHash = Sha $deskExe
$engHash  = Sha $sidecar
Write-Host "desktop  : $deskHash"

# 4. Keep a pristine copy: the next step patches the binary in place.
$pristine = "$env:TEMP\xoksa-desktop-pristine.exe"
Copy-Item $deskExe $pristine -Force

# 5. Let Tauri generate the WiX sources. Its own MSI is discarded — its payload
#    carries the MSI marker and is the one Defender flags.
cargo tauri bundle --bundles msi
if ($LASTEXITCODE -ne 0) { throw 'cargo tauri bundle failed' }

# 6. Restore the unpatched binary, and make sure every file main.wxs points at
#    still exists (Tauri stages the sidecar under %TEMP%, which may be cleaned).
Copy-Item $pristine $deskExe -Force
Select-String -Path "$wixDir\main.wxs" -Pattern 'Source="([^"]+)"' -AllMatches |
  ForEach-Object { $_.Matches } | ForEach-Object { $_.Groups[1].Value } |
  Where-Object { $_ -like '*xoksa.exe' -and -not (Test-Path $_) } |
  ForEach-Object { Copy-Item $engine $_ -Force }

# 7. Re-link. `light` reads file contents now, so the MSI gets the pristine ones.
& $light -nologo -ext WixUIExtension -ext WixUtilExtension -cultures:en-US `
    -loc "$wixDir\locale.wxl" "$wixDir\main.wixobj" -out $outMsi
if ($LASTEXITCODE -ne 0) { throw 'WiX light failed' }

# 8. Verify the payload, because the whole point is that it is unchanged.
$extract = "$env:TEMP\xoksa-msi-verify"
Remove-Item $extract -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $extract | Out-Null
Start-Process msiexec -ArgumentList '/a', (Resolve-Path $outMsi), '/qn', "TARGETDIR=$extract" -Wait

$inMsiDesk  = Get-ChildItem $extract -Recurse -Filter 'xoksa-desktop.exe' | Select-Object -First 1
$inMsiEng   = Get-ChildItem $extract -Recurse -Filter 'xoksa.exe'         | Select-Object -First 1
$inMsiSetup = Get-ChildItem $extract -Recurse -Filter 'xoksa-setup.exe'   | Select-Object -First 1
if (-not $inMsiDesk -or -not $inMsiEng -or -not $inMsiSetup) { throw 'MSI does not contain all three binaries (desktop / engine / settings app)' }
if ((Sha $inMsiDesk.FullName)  -ne $deskHash)  { throw "MSI desktop payload differs from the built binary — Tauri's bundle-type patch leaked in" }
if ((Sha $inMsiEng.FullName)   -ne $engHash)   { throw 'MSI engine payload differs from the inspected engine' }
if ((Sha $inMsiSetup.FullName) -ne $setupHash) { throw 'MSI settings-app payload differs from the built binary' }
Remove-Item $extract -Recurse -Force

# 9. Iron rule — exactly ONE MSI on disk: the freshly built, hash-verified clean
#    one. Tauri's own bundle MSI (step 5) carries the Defender-flagged marker and
#    is only "discarded" in comments; delete it, plus any older/stale MSIs, so a
#    wrong or AV-flagged package can never be shipped or tested by mistake.
$keep = (Resolve-Path $outMsi).Path
# Exactly ONE MSI on disk, in the single canonical release folder. Remove any MSI
# left in the desktop crate's build tree (Tauri's own marker bundle, byproducts)
# AND any other MSI in the release folder.
@("$PSScriptRoot\target\release", "$repoRoot\target\release") |
  ForEach-Object { Get-ChildItem $_ -Recurse -Filter '*.msi' -ErrorAction SilentlyContinue } |
  Where-Object { $_.FullName -ne $keep } |
  ForEach-Object { Write-Host "purge stale MSI: $($_.FullName)"; Remove-Item $_.FullName -Force }

# 10. Fail-closed single-artifact guarantee. After the purge, exactly ONE *.msi may
#     exist across the build trees, and it MUST be the canonical output. If two ever
#     coexist (a stale build elsewhere, a purge miss), STOP — otherwise the wrong
#     (incomplete) MSI could be shipped/evaluated while the real one is ignored.
$allMsi = @(
  @("$PSScriptRoot\target\release", "$repoRoot\target\release") |
    ForEach-Object { Get-ChildItem $_ -Recurse -Filter '*.msi' -ErrorAction SilentlyContinue } |
    ForEach-Object { $_.FullName }
)
if ($allMsi.Count -ne 1)  { throw "Expected exactly one MSI across the build trees; found $($allMsi.Count): $($allMsi -join '; ')" }
if ($allMsi[0] -ne $keep) { throw "The single MSI is not at the canonical path ${keep}: $($allMsi[0])" }
Write-Host "single MSI OK: $keep"

Write-Host ''
Write-Host "OK  $outMsi"
Write-Host "    msi     : $(Sha $outMsi)"
Write-Host "    desktop : $deskHash"
Write-Host "    engine  : $engHash"
Write-Host 'Scan all three on VirusTotal and record them in security-assessment.md C.4.'
