# Build Duckier CLI + Daemon for Windows — runs natively on a Windows host.
#
# Usage:
#   . .\scripts\packaging\win\sign-env.ps1        # configure signing first
#   .\scripts\build-windows-native.ps1 --Sign      # build + sign
#
# Prerequisites:
#   - Rust toolchain (rustup)
#   - protoc (Protocol Buffers compiler)
#   - NSIS (makensis.exe)
#   - Windows SDK (for signtool.exe, when signing)
#   - GlobalSign minidriver + YubiKey inserted (when signing)
#
# Artifacts land in dist\windows\
#   duckier-cli-windows-x64-setup.exe  -- NSIS installer (CLI + daemon + service)
#   duckier-cli.exe                    -- Standalone CLI binary (for desktop app bundling)

param(
    [switch]$Sign,
    [switch]$SkipDaemon,
    [switch]$NoVerify,
    [switch]$Debug
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$CliDir    = $ScriptDir  # scripts/ is directly under the CLI root
# Resolve to actual CLI root (parent of scripts/)
$CliDir    = (Resolve-Path "$ScriptDir\..").Path

# Read version from Cargo.toml
$cargoToml = Get-Content "$CliDir\Cargo.toml" -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    Write-Error "Could not read version from Cargo.toml"
    exit 1
}

Write-Host ""
Write-Host "============================================" -ForegroundColor Cyan
Write-Host " Duckier CLI v$Version - Windows Build (native)" -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan

# -- Signing status --
if ($Sign) {
    if ($env:WINDOWS_CERT_THUMBPRINT) {
        $short = $env:WINDOWS_CERT_THUMBPRINT.Substring(0, [Math]::Min(8, $env:WINDOWS_CERT_THUMBPRINT.Length))
        Write-Host "  Signing:  ENABLED (thumbprint: $short...)" -ForegroundColor Green
    } else {
        Write-Host "  Signing:  REQUESTED but WINDOWS_CERT_THUMBPRINT not set" -ForegroundColor Yellow
        Write-Host "  Run:  . .\scripts\packaging\win\sign-env.ps1" -ForegroundColor Yellow
    }
} else {
    Write-Host "  Signing:  DISABLED (use -Sign to enable)" -ForegroundColor DarkGray
}
Write-Host ""

# -- Check prerequisites --
$missing = @()
if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue))  { $missing += "cargo (Rust toolchain)" }
if (-not (Get-Command "protoc" -ErrorAction SilentlyContinue)) { $missing += "protoc (Protocol Buffers)" }

if ($missing.Count -gt 0) {
    Write-Error "Missing prerequisites: $($missing -join ', ')"
    exit 1
}

# Locate makensis.exe
$makensis = $null
if (Get-Command "makensis" -ErrorAction SilentlyContinue) {
    $makensis = (Get-Command "makensis").Source
} else {
    # Check common NSIS install locations
    $nsisLocations = @(
        "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
        "${env:ProgramFiles}\NSIS\makensis.exe",
        "C:\Program Files (x86)\NSIS\makensis.exe",
        "C:\Program Files\NSIS\makensis.exe"
    )
    foreach ($loc in $nsisLocations) {
        if (Test-Path $loc) {
            $makensis = $loc
            break
        }
    }
}
if (-not $makensis) {
    Write-Error "makensis.exe not found. Install NSIS from https://nsis.sourceforge.io"
    exit 1
}

$SignScript = "$CliDir\scripts\packaging\win\sign.ps1"

# -- 1. Download daemon binary --
if (-not $SkipDaemon) {
    Write-Host "[1/4] Downloading Windows daemon binary..." -ForegroundColor Cyan
    $dlArgs = @()
    if ($NoVerify) { $dlArgs += "-NoVerify" }
    & "$CliDir\scripts\packaging\win\download-daemon.ps1" @dlArgs
} else {
    Write-Host "[1/4] Skipping daemon download (-SkipDaemon)" -ForegroundColor DarkGray
}

$DaemonBin = "$CliDir\binaries\duckiervpn-daemon.exe"
if (-not (Test-Path $DaemonBin)) {
    Write-Error "Daemon binary not found at $DaemonBin"
    exit 1
}

# -- 2. Build CLI --
Write-Host ""
Write-Host "[2/4] Building CLI x86_64 (Windows)..." -ForegroundColor Cyan
Push-Location $CliDir
try {
    if ($Debug) {
        cargo build
    } else {
        cargo build --release
    }
} finally {
    Pop-Location
}

$profile = if ($Debug) { "debug" } else { "release" }
$CliBin = "$CliDir\target\$profile\duckier-cli.exe"
if (-not (Test-Path $CliBin)) {
    # Try package name fallback
    $CliBin = "$CliDir\target\$profile\duckier.exe"
}
if (-not (Test-Path $CliBin)) {
    Write-Error "CLI binary not found after build."
    Get-ChildItem "$CliDir\target\$profile\*.exe" -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  Found: $($_.Name)" }
    exit 1
}

# -- 3. Code signing --
Write-Host ""
if ($Sign -and $env:WINDOWS_CERT_THUMBPRINT) {
    Write-Host "[3/4] Code signing..." -ForegroundColor Cyan
    foreach ($bin in @($CliBin, $DaemonBin)) {
        & $SignScript $bin
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
} elseif ($Sign) {
    Write-Host "[3/4] Skipping code signing (WINDOWS_CERT_THUMBPRINT not set)" -ForegroundColor Yellow
} else {
    Write-Host "[3/4] Skipping code signing (use -Sign to enable)" -ForegroundColor DarkGray
}

# -- 4. Build NSIS installer --
Write-Host ""
Write-Host "[4/4] Building NSIS installer..." -ForegroundColor Cyan

$Dist    = "$CliDir\dist\windows"
$Staging = "$Dist\staging"
if (Test-Path $Dist) { Remove-Item -Recurse -Force $Dist }
New-Item -ItemType Directory -Path $Staging -Force | Out-Null

# Copy standalone CLI binary to dist (for desktop app bundling)
Copy-Item $CliBin "$Dist\duckier-cli.exe"
Write-Host "  Standalone: dist\windows\duckier-cli.exe"

# Stage files for NSIS installer
Copy-Item $CliBin "$Staging\duckier-cli.exe"
Copy-Item $DaemonBin "$Staging\duckiervpn-daemon.exe"
Copy-Item "$CliDir\LICENSE" "$Staging\"
Copy-Item "$CliDir\THIRD_PARTY_NOTICES.md" "$Staging\"

$InstallerName = "duckier-cli-windows-x64-setup.exe"
$NsiFile = "$CliDir\scripts\packaging\win\installer.nsi"

& $makensis `
    "-DVERSION=$Version" `
    "-DOUTFILE=$Dist\$InstallerName" `
    "-DSTAGING_DIR=$Staging" `
    $NsiFile

if ($LASTEXITCODE -ne 0) {
    Write-Error "makensis failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

# Sign the installer itself
if ($Sign -and $env:WINDOWS_CERT_THUMBPRINT) {
    Write-Host "  Signing installer..."
    & $SignScript "$Dist\$InstallerName"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

# Clean up staging and downloaded daemon
Remove-Item -Recurse -Force $Staging
Remove-Item -Recurse -Force "$CliDir\binaries"

# -- Output --
Write-Host ""
Write-Host "============================================" -ForegroundColor Cyan
Write-Host " Build complete!" -ForegroundColor Green
Write-Host "" -ForegroundColor Cyan
Write-Host " Artifacts:" -ForegroundColor Cyan

$installer = Get-Item "$Dist\$InstallerName"
$standalone = Get-Item "$Dist\duckier-cli.exe"
$installerSize = [math]::Round($installer.Length / 1MB, 1)
$standaloneSize = [math]::Round($standalone.Length / 1MB, 1)
Write-Host "   $InstallerName ($installerSize MB)"
Write-Host "   duckier-cli.exe ($standaloneSize MB)"

if ($Sign -and $env:WINDOWS_CERT_THUMBPRINT) {
    Write-Host ""
    Write-Host " Signature verification:" -ForegroundColor Cyan
    foreach ($file in @("$Dist\$InstallerName", "$Dist\duckier-cli.exe")) {
        $sig = Get-AuthenticodeSignature $file
        $name = Split-Path $file -Leaf
        Write-Host "   $name  Status: $($sig.Status)  Signer: $($sig.SignerCertificate.Subject)"
    }
}

Write-Host ""
Write-Host " Installer: Run the .exe installer on Windows (requires Administrator)"
Write-Host " Standalone: duckier-cli.exe can be bundled into the desktop app"
Write-Host "============================================" -ForegroundColor Cyan
