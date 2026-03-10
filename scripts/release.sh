#!/usr/bin/env bash
# Build and release Duckier CLI to the update server.
#
# Usage:
#   ./scripts/release.sh              # Build all platforms + upload
#   ./scripts/release.sh --upload-only # Upload pre-built artifacts only
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(dirname "$SCRIPT_DIR")"
cd "$CLI_DIR"

# Parse flags
UPLOAD_ONLY=false
SIGN_FLAG="--sign"
while [[ $# -gt 0 ]]; do
    case $1 in
        --upload-only)
            UPLOAD_ONLY=true
            shift
            ;;
        --no-sign)
            SIGN_FLAG=""
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--upload-only] [--no-sign]"
            exit 1
            ;;
    esac
done

version=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

echo "============================================"
echo " Duckier CLI v${version} — Release"
echo "============================================"

# ── Build ──
if [ "$UPLOAD_ONLY" = false ]; then
    echo ""
    echo "[1/4] Building macOS (aarch64 + x86_64)..."
    "$SCRIPT_DIR/build-mac.sh" $SIGN_FLAG

    echo ""
    echo "[2/4] Building Linux (deb + rpm + arch, x86_64 + aarch64)..."
    "$SCRIPT_DIR/build-linux.sh" --format all

    echo ""
    echo "[3/4] Building Windows (x86_64)..."
    "$SCRIPT_DIR/build-windows.sh" $SIGN_FLAG
else
    echo ""
    echo "Skipping builds (--upload-only)"
fi

# ── Upload ──
echo ""
echo "[4/4] Uploading to update server..."

# macOS — .pkg installers (include CLI + daemon + launchd service)
MAC_AARCH64="dist/mac/duckier-cli-mac-aarch64.pkg"
if [ -f "$MAC_AARCH64" ]; then
    release-uploader -f "$MAC_AARCH64" --os CLI_MAC --version "$version"
else
    echo "  Skipping macOS aarch64 upload (artifact not found: $MAC_AARCH64)"
fi

MAC_X86="dist/mac/duckier-cli-mac-x86_64.pkg"
if [ -f "$MAC_X86" ]; then
    release-uploader -f "$MAC_X86" --os CLI_MAC_X86 --version "$version"
else
    echo "  Skipping macOS x86_64 upload (artifact not found: $MAC_X86)"
fi

# Linux amd64 — .deb
DEB=$(ls dist/linux/deb/duckier-cli_*_amd64.deb 2>/dev/null | head -1 || true)
if [ -n "$DEB" ]; then
    release-uploader -f "$DEB" --os CLI_DEB_AMD64 --version "$version"
else
    echo "  Skipping Linux deb amd64 upload (no artifact found)"
fi

# Linux arm64 — .deb
DEB_ARM=$(ls dist/linux/deb/duckier-cli_*_arm64.deb 2>/dev/null | head -1 || true)
if [ -n "$DEB_ARM" ]; then
    release-uploader -f "$DEB_ARM" --os CLI_DEB_ARM64 --version "$version"
else
    echo "  Skipping Linux deb arm64 upload (no artifact found)"
fi

# Linux x86_64 — .rpm
RPM=$(ls dist/linux/rpm/duckier-cli-*.x86_64.rpm 2>/dev/null | head -1 || true)
if [ -n "$RPM" ]; then
    release-uploader -f "$RPM" --os CLI_RPM_X86_64 --version "$version"
else
    echo "  Skipping Linux rpm x86_64 upload (no artifact found)"
fi

# Linux aarch64 — .rpm
RPM_ARM=$(ls dist/linux/rpm/duckier-cli-*.aarch64.rpm 2>/dev/null | head -1 || true)
if [ -n "$RPM_ARM" ]; then
    release-uploader -f "$RPM_ARM" --os CLI_RPM_AARCH64 --version "$version"
else
    echo "  Skipping Linux rpm aarch64 upload (no artifact found)"
fi

# Linux x86_64 — Arch
ARCH=$(ls dist/linux/arch/duckier-cli-*-x86_64.pkg.tar.zst 2>/dev/null | head -1 || true)
if [ -n "$ARCH" ]; then
    release-uploader -f "$ARCH" --os CLI_ARCH_X86_64 --version "$version"
else
    echo "  Skipping Linux arch x86_64 upload (no artifact found)"
fi

# Linux aarch64 — Arch
ARCH_ARM=$(ls dist/linux/arch/duckier-cli-*-aarch64.pkg.tar.zst 2>/dev/null | head -1 || true)
if [ -n "$ARCH_ARM" ]; then
    release-uploader -f "$ARCH_ARM" --os CLI_ARCH_AARCH64 --version "$version"
else
    echo "  Skipping Linux arch aarch64 upload (no artifact found)"
fi

# Windows x64 — NSIS installer
WIN_EXE="dist/windows/duckier-cli-windows-x64-setup.exe"
if [ -f "$WIN_EXE" ]; then
    release-uploader -f "$WIN_EXE" --os CLI_WIN --version "$version"
else
    echo "  Skipping Windows installer upload (artifact not found: $WIN_EXE)"
fi

# Windows x64 — Standalone CLI binary (for desktop app bundling)
WIN_CLI="dist/windows/duckier-cli.exe"
if [ -f "$WIN_CLI" ]; then
    release-uploader -f "$WIN_CLI" --os CLI_WIN_STANDALONE --version "$version"
else
    echo "  Skipping Windows standalone CLI upload (artifact not found: $WIN_CLI)"
fi

echo ""
echo "============================================"
echo " Released Duckier CLI v${version}"
echo "============================================"
