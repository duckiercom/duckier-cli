#!/bin/bash
# Build Duckier CLI + Daemon for Windows (cross-compiled from macOS)
#
# For native Windows builds with hardware-token signing (signtool + YubiKey),
# use build-windows-native.ps1 instead.
#
# Usage:
#   ./scripts/build-windows.sh [--sign]
#
# Prerequisites:
#   brew install nsis
#   cargo install cargo-xwin
#   rustup target add x86_64-pc-windows-msvc
#
# Optional signing (Azure Key Vault, cross-platform):
#   brew install jsign azure-cli openjdk@17
#   source scripts/sign-env.sh
#
# Artifacts land in dist/windows/
#   duckier-cli-windows-x64-setup.exe  — NSIS installer (CLI + daemon + service)
#   duckier-cli.exe                    — Standalone CLI binary (for desktop app bundling)
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(dirname "$SCRIPT_DIR")"

# Parse flags
SIGN=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --sign)
            SIGN=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--sign]"
            exit 1
            ;;
    esac
done

VERSION=$(grep '^version' "$CLI_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')

echo "============================================"
echo " Duckier CLI v$VERSION — Windows build (x86_64)"
echo "============================================"

# ── Preflight ──
if ! command -v cargo &> /dev/null; then
    echo "Error: Rust toolchain not found. Install from https://rustup.rs"
    exit 1
fi
if ! command -v cargo-xwin &> /dev/null && ! cargo xwin --version &> /dev/null; then
    echo "Error: cargo-xwin not found. Install: cargo install cargo-xwin"
    exit 1
fi
if ! command -v makensis &> /dev/null; then
    echo "Error: makensis not found. Install: brew install nsis"
    exit 1
fi
if ! command -v protoc &> /dev/null; then
    echo "Error: protoc not found. Install: brew install protobuf"
    exit 1
fi

# Ensure target is available
rustup target add x86_64-pc-windows-msvc 2>/dev/null || true

# ── 1. Download Windows daemon ──
echo ""
echo "[1/4] Downloading Windows daemon binary..."
"$SCRIPT_DIR/download-daemon.sh" win

DAEMON_BIN="$CLI_DIR/binaries/duckiervpn-daemon.exe"
if [ ! -f "$DAEMON_BIN" ]; then
    echo "Error: Daemon binary not found after download."
    ls -la "$CLI_DIR/binaries/" 2>/dev/null || echo "  (directory doesn't exist)"
    exit 1
fi

# ── 2. Cross-compile CLI for Windows ──
cd "$CLI_DIR"
echo ""
echo "[2/4] Building CLI x86_64 (Windows)..."
cargo xwin build --release --target x86_64-pc-windows-msvc

CLI_BIN="$CLI_DIR/target/x86_64-pc-windows-msvc/release/duckier-cli.exe"
if [ ! -f "$CLI_BIN" ]; then
    # cargo may produce the binary with the package name
    CLI_BIN="$CLI_DIR/target/x86_64-pc-windows-msvc/release/duckier.exe"
fi
if [ ! -f "$CLI_BIN" ]; then
    echo "Error: CLI binary not found after build."
    ls -la "$CLI_DIR/target/x86_64-pc-windows-msvc/release/"*.exe 2>/dev/null || true
    exit 1
fi

# ── 3. Code signing (optional, Azure Key Vault via jsign) ──
SIGN_SCRIPT="$SCRIPT_DIR/sign-windows.sh"
if [ "$SIGN" = true ]; then
    if [ -n "${AZURE_CLIENT_ID:-}" ] && [ -n "${AZURE_KEY_VAULT_URI:-}" ]; then
        echo ""
        echo "[3/4] Code signing (Azure Key Vault)..."
        for bin in "$CLI_BIN" "$DAEMON_BIN"; do
            "$SIGN_SCRIPT" "$bin"
        done
    else
        echo ""
        echo "[3/4] Skipping code signing (Azure Key Vault not configured)"
        echo "  Run: source scripts/sign-env.sh"
    fi
else
    echo ""
    echo "[3/4] Skipping code signing (use --sign to enable)"
fi

# ── 4. Build NSIS installer ──
echo ""
echo "[4/4] Building NSIS installer..."

DIST="$CLI_DIR/dist/windows"
STAGING="$DIST/staging"
rm -rf "$DIST"
mkdir -p "$STAGING"

# Copy standalone CLI binary to dist (for desktop app bundling)
cp "$CLI_BIN" "$DIST/duckier-cli.exe"
echo "  Standalone: $DIST/duckier-cli.exe"

# Stage files for NSIS installer
cp "$CLI_BIN" "$STAGING/duckier-cli.exe"
cp "$DAEMON_BIN" "$STAGING/duckiervpn-daemon.exe"
cp "$CLI_DIR/LICENSE" "$STAGING/"
cp "$CLI_DIR/THIRD_PARTY_NOTICES.md" "$STAGING/"

INSTALLER_NAME="duckier-cli-windows-x64-setup.exe"
NSI_FILE="$CLI_DIR/scripts/packaging/win/installer.nsi"

makensis \
    -DVERSION="$VERSION" \
    -DOUTFILE="$DIST/$INSTALLER_NAME" \
    -DSTAGING_DIR="$STAGING" \
    "$NSI_FILE"

# Sign the installer itself
if [ "$SIGN" = true ] && [ -n "${AZURE_CLIENT_ID:-}" ] && [ -n "${AZURE_KEY_VAULT_URI:-}" ]; then
    echo "  Signing installer..."
    "$SIGN_SCRIPT" "$DIST/$INSTALLER_NAME"
fi

# Clean up staging and downloaded daemon
rm -rf "$STAGING"
rm -rf "$CLI_DIR/binaries"

echo ""
echo "============================================"
echo " Build complete!"
echo ""
echo " Artifacts:"
ls -lh "$DIST/$INSTALLER_NAME"
ls -lh "$DIST/duckier-cli.exe"
echo ""
echo " Installer: Run the .exe installer on Windows (requires Administrator)"
echo " Standalone: duckier-cli.exe can be bundled into the desktop app"
echo "============================================"
