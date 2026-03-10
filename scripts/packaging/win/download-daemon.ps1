# Download VPN daemon binary for Windows with SHA256 verification.
#
# Usage:
#   .\scripts\packaging\win\download-daemon.ps1
#   .\scripts\packaging\win\download-daemon.ps1 -NoVerify   # dev only
#
# Output: binaries\duckiervpn-daemon.exe

param(
    [switch]$NoVerify
)

$ErrorActionPreference = "Stop"

$ScriptDir   = Split-Path -Parent $MyInvocation.MyCommand.Path
$CliDir      = (Resolve-Path "$ScriptDir\..\..\..").Path
$BinariesDir = "$CliDir\binaries"
$BaseUrl     = "https://update-vpn.duckier.com"

if (-not (Test-Path $BinariesDir)) {
    New-Item -ItemType Directory -Path $BinariesDir -Force | Out-Null
}

$DaemonUrl   = "$BaseUrl/win/daemon"
$VersionPath = "win/daemon"
$DestFile    = "$BinariesDir\duckiervpn-daemon.exe"

Write-Host "  Downloading: $DaemonUrl"
Invoke-WebRequest -Uri $DaemonUrl -OutFile $DestFile -UseBasicParsing

if (-not $NoVerify) {
    Write-Host "  Verifying SHA256 via $BaseUrl/version/$VersionPath ..."

    try {
        $versionJson = Invoke-RestMethod -Uri "$BaseUrl/version/$VersionPath" -UseBasicParsing
    } catch {
        Write-Error "Could not fetch version info from $BaseUrl/version/$VersionPath`nCannot verify daemon integrity. Aborting.`nUse -NoVerify to skip (development only)."
        Remove-Item -Force $DestFile -ErrorAction SilentlyContinue
        exit 1
    }

    $expected = $versionJson.sha256
    if (-not $expected) {
        Write-Error "Version endpoint returned no SHA256 hash.`nCannot verify daemon integrity. Aborting.`nUse -NoVerify to skip (development only)."
        Remove-Item -Force $DestFile -ErrorAction SilentlyContinue
        exit 1
    }

    $actual = (Get-FileHash -Path $DestFile -Algorithm SHA256).Hash.ToLower()

    if ($expected -ne $actual) {
        Write-Host "  ERROR: SHA256 mismatch!" -ForegroundColor Red
        Write-Host "    Expected: $expected"
        Write-Host "    Actual:   $actual"
        Write-Host "  The downloaded file may be corrupted or tampered with."
        Remove-Item -Force $DestFile -ErrorAction SilentlyContinue
        exit 1
    }
    Write-Host "  SHA256 verified: $($actual.Substring(0, 16))..."
} else {
    Write-Host "  Skipping SHA256 verification (-NoVerify)"
}

$size = [math]::Round((Get-Item $DestFile).Length / 1MB, 1)
Write-Host "  Saved: binaries\duckiervpn-daemon.exe ($size MB)"
