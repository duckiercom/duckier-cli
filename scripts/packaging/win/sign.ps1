# Sign a Windows executable using signtool.exe with a hardware token (YubiKey).
#
# Usage:
#   .\scripts\packaging\win\sign.ps1 <file-to-sign>
#
# The certificate lives in the Windows certificate store, installed via
# the GlobalSign minidriver. signtool finds it by thumbprint and routes
# the private-key operation to the YubiKey automatically.
#
# Required environment variables:
#   WINDOWS_CERT_THUMBPRINT  -- SHA-1 thumbprint of the code signing certificate
#
# Optional:
#   WINDOWS_TIMESTAMP_URL    -- RFC 3161 timestamp server (default: GlobalSign)
#   WINDOWS_APP_NAME         -- Description embedded in the signature
#   WINDOWS_CERT_STORE       -- Certificate store name (default: My)
#   SIGNTOOL_PATH            -- Full path to signtool.exe if not on PATH

param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$FilePath
)

$ErrorActionPreference = "Stop"

# -- Config from environment --
$Thumbprint   = $env:WINDOWS_CERT_THUMBPRINT
$TimestampUrl = if ($env:WINDOWS_TIMESTAMP_URL) { $env:WINDOWS_TIMESTAMP_URL } else { "http://rfc3161timestamp.globalsign.com/advanced" }
$AppName      = if ($env:WINDOWS_APP_NAME)      { $env:WINDOWS_APP_NAME }      else { "Duckier CLI" }
$CertStore    = if ($env:WINDOWS_CERT_STORE)     { $env:WINDOWS_CERT_STORE }     else { "My" }

# -- Skip if thumbprint not configured --
if (-not $Thumbprint) {
    Write-Host "SKIP: WINDOWS_CERT_THUMBPRINT not set, skipping signing for: $(Split-Path $FilePath -Leaf)"
    exit 0
}

# -- Locate signtool.exe --
if ($env:SIGNTOOL_PATH -and (Test-Path $env:SIGNTOOL_PATH)) {
    $SignTool = $env:SIGNTOOL_PATH
} else {
    # Try common Windows SDK locations (newest version first)
    $sdkPaths = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe",
        "${env:ProgramFiles}\Windows Kits\10\bin\*\x64\signtool.exe"
    )
    $SignTool = $null
    foreach ($pattern in $sdkPaths) {
        $found = Get-ChildItem -Path $pattern -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
        if ($found) {
            $SignTool = $found.FullName
            break
        }
    }

    if (-not $SignTool) {
        $SignTool = (Get-Command signtool.exe -ErrorAction SilentlyContinue).Source
    }

    if (-not $SignTool) {
        Write-Error "signtool.exe not found. Install Windows SDK or set SIGNTOOL_PATH."
        exit 1
    }
}

# -- Verify the certificate exists in the store --
$cert = Get-ChildItem -Path "Cert:\CurrentUser\$CertStore" -ErrorAction SilentlyContinue |
    Where-Object { $_.Thumbprint -eq $Thumbprint }

if (-not $cert) {
    $cert = Get-ChildItem -Path "Cert:\LocalMachine\$CertStore" -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $Thumbprint }
}

if (-not $cert) {
    Write-Error "Certificate with thumbprint $Thumbprint not found in store '$CertStore'. Is the YubiKey inserted and minidriver installed?"
    exit 1
}

Write-Host "Signing: $(Split-Path $FilePath -Leaf)"
Write-Host "  Cert:       $($cert.Subject)"
Write-Host "  Thumbprint: $Thumbprint"

# -- Sign with SHA-256 + RFC 3161 timestamp --
# Use an argument list so values containing spaces (app name, file path) are
# passed as single tokens to signtool.exe.
$signArgs = @(
    "sign",
    "/sha1", $Thumbprint,
    "/s",    $CertStore,
    "/d",    $AppName,
    "/fd",   "sha256",
    "/tr",   $TimestampUrl,
    "/td",   "sha256",
    "/v",    $FilePath
)
& $SignTool @signArgs

if ($LASTEXITCODE -ne 0) {
    Write-Error "signtool.exe failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

Write-Host "  Signed: $(Split-Path $FilePath -Leaf)"
