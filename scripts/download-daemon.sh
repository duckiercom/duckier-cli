#!/bin/bash
# Download VPN daemon binary for the specified platform
#
# Usage:
#   ./scripts/download-daemon.sh <platform> [--no-verify]
#
# Platforms:
#   linux-x64       → duckiervpn-daemon (Linux x86_64)
#   linux-arm64     → duckiervpn-daemon (Linux arm64)
#   mac             → duckiervpn-daemon (macOS universal, split via lipo)
#   win             → duckiervpn-daemon.exe (Windows x86_64)
#
# Output goes to binaries/ directory
#
# SHA256 integrity verification is performed against the update server's
# /version/{path} endpoint for all platforms. Verification is mandatory by
# default — the script aborts if the hash cannot be obtained or does not match.
# Use --no-verify to skip (development only, NOT for release builds).
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(dirname "$SCRIPT_DIR")"
BINARIES_DIR="$CLI_DIR/binaries"

BASE_URL="https://update.duckier.com"

PLATFORM=""
SKIP_VERIFY=false
for arg in "$@"; do
    case "$arg" in
        --no-verify) SKIP_VERIFY=true ;;
        -*) echo "Unknown flag: $arg"; exit 1 ;;
        *) PLATFORM="$arg" ;;
    esac
done

if [ -z "$PLATFORM" ]; then
    echo "Usage: $0 <linux-x64|linux-arm64|mac|win> [--no-verify]"
    exit 1
fi

mkdir -p "$BINARIES_DIR"

# Compute SHA256 hash portably (macOS vs Linux)
compute_sha256() {
    local file="$1"
    if command -v shasum &> /dev/null; then
        shasum -a 256 "$file" | awk '{print $1}'
    elif command -v sha256sum &> /dev/null; then
        sha256sum "$file" | awk '{print $1}'
    else
        echo "Error: No SHA256 tool found (need shasum or sha256sum)" >&2
        exit 1
    fi
}

# Extract sha256 field from JSON without requiring jq
extract_sha256() {
    local json="$1"
    if command -v jq &> /dev/null; then
        echo "$json" | jq -r '.sha256'
    else
        echo "$json" | grep -o '"sha256" *: *"[^"]*"' | head -1 | sed 's/.*: *"//;s/"//'
    fi
}

# Verify SHA256 of a downloaded file against the update server.
# Fails hard if verification cannot be completed (fail-closed).
verify_sha256() {
    local file="$1"
    local version_path="$2"

    if [ "$SKIP_VERIFY" = true ]; then
        echo "  Skipping SHA256 verification (--no-verify)"
        return 0
    fi

    echo "  Verifying SHA256 via $BASE_URL/version/$version_path ..."
    local version_json
    version_json="$(curl -fsSL "$BASE_URL/version/$version_path")" || {
        echo "  ERROR: Could not fetch version info from $BASE_URL/version/$version_path"
        echo "  Cannot verify daemon integrity. Aborting."
        echo "  Use --no-verify to skip (development only)."
        rm -f "$file"
        exit 1
    }

    local expected
    expected="$(extract_sha256 "$version_json")"
    if [ -z "$expected" ] || [ "$expected" = "null" ]; then
        echo "  ERROR: Version endpoint returned no SHA256 hash."
        echo "  Cannot verify daemon integrity. Aborting."
        echo "  Use --no-verify to skip (development only)."
        rm -f "$file"
        exit 1
    fi

    local actual
    actual="$(compute_sha256 "$file")"

    if [ "$expected" != "$actual" ]; then
        echo "  ERROR: SHA256 mismatch!"
        echo "    Expected: $expected"
        echo "    Actual:   $actual"
        echo "  The downloaded file may be corrupted or tampered with."
        rm -f "$file"
        exit 1
    fi
    echo "  SHA256 verified: ${actual:0:16}..."
}

download() {
    local url="$1"
    local dest="$2"
    local version_path="$3"

    echo "  Downloading: $url"
    curl -fSL --progress-bar -o "$dest" "$url"

    verify_sha256 "$dest" "$version_path"

    chmod 755 "$dest"
}

case "$PLATFORM" in
    linux-x64)
        download "$BASE_URL/linux/daemon" "$BINARIES_DIR/duckiervpn-daemon" "linux/daemon"
        echo "  Saved: binaries/duckiervpn-daemon ($(ls -lh "$BINARIES_DIR/duckiervpn-daemon" | awk '{print $5}'))"
        ;;
    linux-arm64)
        download "$BASE_URL/linux/daemon/arm64" "$BINARIES_DIR/duckiervpn-daemon" "linux/daemon/arm64"
        echo "  Saved: binaries/duckiervpn-daemon ($(ls -lh "$BINARIES_DIR/duckiervpn-daemon" | awk '{print $5}'))"
        ;;
    mac)
        TEMP="$BINARIES_DIR/.daemon-universal"
        download "$BASE_URL/mac/daemon" "$TEMP" "mac/daemon"

        # Extract per-arch binaries from the universal fat binary
        for ARCH in arm64 x86_64; do
            DEST="$BINARIES_DIR/duckiervpn-daemon-$ARCH"
            if lipo -thin "$ARCH" "$TEMP" -output "$DEST" 2>/dev/null; then
                echo "  Extracted $ARCH: $(ls -lh "$DEST" | awk '{print $5}')"
            else
                # Single-arch binary — just copy
                cp "$TEMP" "$DEST"
                echo "  Copied (single-arch): $DEST"
            fi
            chmod 755 "$DEST"
        done
        rm -f "$TEMP"
        ;;
    win)
        download "$BASE_URL/win/daemon" "$BINARIES_DIR/duckiervpn-daemon.exe" "win/daemon"
        echo "  Saved: binaries/duckiervpn-daemon.exe ($(ls -lh "$BINARIES_DIR/duckiervpn-daemon.exe" | awk '{print $5}'))"
        ;;
    *)
        echo "Unknown platform: $PLATFORM"
        echo "Valid: linux-x64, linux-arm64, mac, win"
        exit 1
        ;;
esac

echo "Done."
