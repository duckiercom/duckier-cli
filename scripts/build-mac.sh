#!/bin/bash
# Build Duckier CLI + Daemon for macOS (per-arch, uploaded separately)
#
# Usage:
#   ./scripts/build-mac.sh [--sign]
#
# Artifacts land in dist/mac/
#   duckier-cli-mac-aarch64.pkg      — Apple Silicon installer (signed if --sign)
#   duckier-cli-mac-x86_64.pkg       — Intel installer (signed if --sign)
#   duckier-cli-mac-aarch64.tar.gz   — Apple Silicon tarball (for non-interactive installs)
#   duckier-cli-mac-x86_64.tar.gz    — Intel tarball (for non-interactive installs)
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
BUNDLE_ID="com.duckier.vpn.cli"

echo "============================================"
echo " Duckier CLI v$VERSION — macOS build (with daemon)"
echo "============================================"

# ── Source signing environment ──
if [ "$SIGN" = true ]; then
    SIGN_ENV="$SCRIPT_DIR/sign-env.sh"
    if [ -f "$SIGN_ENV" ]; then
        source "$SIGN_ENV"
    else
        echo "Error: scripts/sign-env.sh not found."
        echo "Copy scripts/sign-env.sh.example to scripts/sign-env.sh and fill in your credentials."
        exit 1
    fi
fi

# ── Preflight ──
if ! command -v cargo &> /dev/null; then
    echo "Error: Rust toolchain not found. Install from https://rustup.rs"
    exit 1
fi
if ! command -v protoc &> /dev/null; then
    echo "Error: protoc not found. Install: brew install protobuf"
    exit 1
fi

# Ensure both targets are available
rustup target add aarch64-apple-darwin 2>/dev/null || true
rustup target add x86_64-apple-darwin 2>/dev/null || true

# ── 1. Download daemon binaries ──
echo ""
echo "[1/5] Downloading macOS daemon binaries..."
"$SCRIPT_DIR/download-daemon.sh" mac

DAEMON_ARM="$CLI_DIR/binaries/duckiervpn-daemon-arm64"
DAEMON_X64="$CLI_DIR/binaries/duckiervpn-daemon-x86_64"

if [ ! -f "$DAEMON_ARM" ] || [ ! -f "$DAEMON_X64" ]; then
    echo "Error: Daemon binaries not found after download."
    ls -la "$CLI_DIR/binaries/" 2>/dev/null || echo "  (directory doesn't exist)"
    exit 1
fi

# ── 2. Build CLI — aarch64 (Apple Silicon) ──
cd "$CLI_DIR"
echo ""
echo "[2/5] Building CLI aarch64 (Apple Silicon)..."
cargo build --release --target aarch64-apple-darwin

# ── 3. Build CLI — x86_64 (Intel) ──
echo ""
echo "[3/5] Building CLI x86_64 (Intel)..."
cargo build --release --target x86_64-apple-darwin

# ── 4. Code signing ──
PKG_DIR="$CLI_DIR/scripts/packaging/mac"
ENTITLEMENTS="$PKG_DIR/entitlements.plist"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if [ "$SIGN" = true ]; then
    SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-}"
    if [ -n "$SIGNING_IDENTITY" ]; then
        echo ""
        echo "[4/5] Code signing..."
        for bin in \
            target/aarch64-apple-darwin/release/duckier-cli \
            target/x86_64-apple-darwin/release/duckier-cli \
            "$DAEMON_ARM" \
            "$DAEMON_X64"; do
            echo "  Signing: $(basename "$bin")"
            codesign --force --timestamp \
                --sign "$SIGNING_IDENTITY" \
                --options runtime \
                --entitlements "$ENTITLEMENTS" \
                --keychain "$KEYCHAIN" \
                "$bin"
        done
    else
        echo "Warning: --sign requested but APPLE_SIGNING_IDENTITY not set"
    fi
else
    echo ""
    echo "[4/5] Skipping code signing (use --sign to enable)"
fi

# ── 5. Assemble dist (per-arch: .pkg + .tar.gz) ──
echo ""
echo "[5/5] Assembling distribution..."

DIST="$CLI_DIR/dist/mac"
rm -rf "$DIST"

for ARCH in aarch64 x86_64; do
    ARCH_DIR="$DIST/$ARCH"
    mkdir -p "$ARCH_DIR"

    if [ "$ARCH" = "aarch64" ]; then
        TARGET="aarch64-apple-darwin"
        DAEMON_BIN="$DAEMON_ARM"
    else
        TARGET="x86_64-apple-darwin"
        DAEMON_BIN="$DAEMON_X64"
    fi

    # Copy binaries
    cp "target/$TARGET/release/duckier-cli" "$ARCH_DIR/duckier-cli"
    cp "$DAEMON_BIN" "$ARCH_DIR/duckiervpn-daemon"
    chmod 755 "$ARCH_DIR/duckier-cli" "$ARCH_DIR/duckiervpn-daemon"

    # Copy install scripts + launchd plist (for tarball)
    cp "$PKG_DIR/com.duckier.vpn.daemon.plist" "$ARCH_DIR/"
    cp "$PKG_DIR/install.sh"   "$ARCH_DIR/"
    cp "$PKG_DIR/uninstall.sh" "$ARCH_DIR/"
    chmod 755 "$ARCH_DIR/install.sh" "$ARCH_DIR/uninstall.sh"

    # Create tarball
    tar -czf "$DIST/duckier-cli-mac-${ARCH}.tar.gz" -C "$ARCH_DIR" .
    echo "  $ARCH: duckier-cli-mac-${ARCH}.tar.gz"

    # ── Build .pkg installer ──
    echo "  $ARCH: Building .pkg installer..."

    PKG_STAGE="$DIST/pkg-stage-${ARCH}"
    PKG_COMPONENT="$DIST/${ARCH}-component.pkg"
    PKG_FINAL="$DIST/duckier-cli-mac-${ARCH}.pkg"

    # Stage install layout
    mkdir -p "$PKG_STAGE/usr/local/bin"
    mkdir -p "$PKG_STAGE/Library/LaunchDaemons"
    cp "$ARCH_DIR/duckier-cli"           "$PKG_STAGE/usr/local/bin/duckier-cli"
    cp "$ARCH_DIR/duckiervpn-daemon"    "$PKG_STAGE/usr/local/bin/duckiervpn-daemon"
    cp "$PKG_DIR/com.duckier.vpn.daemon.plist" "$PKG_STAGE/Library/LaunchDaemons/"
    chmod 755 "$PKG_STAGE/usr/local/bin/duckier-cli" "$PKG_STAGE/usr/local/bin/duckiervpn-daemon"
    chmod 644 "$PKG_STAGE/Library/LaunchDaemons/com.duckier.vpn.daemon.plist"

    # Create pkg scripts directory with pre/post install
    PKG_SCRIPTS="$DIST/pkg-scripts-${ARCH}"
    mkdir -p "$PKG_SCRIPTS"

    cat > "$PKG_SCRIPTS/preinstall" << 'PREINSTALL_EOF'
#!/usr/bin/env bash
set +e
LOG_DIR=/var/log/duckier
mkdir -p $LOG_DIR
chmod 755 $LOG_DIR
echo "Pre installation process started" >> $LOG_DIR/installer.log

# Stop existing daemon service if running (shared by CLI and desktop app)
if launchctl list com.duckier.vpn.daemon &>/dev/null; then
    launchctl bootout system/com.duckier.vpn.daemon 2>/dev/null || true
    sleep 2
    echo "Daemon stopped" >> $LOG_DIR/installer.log
fi

killall duckiervpn-daemon 2>/dev/null || true
exit 0
PREINSTALL_EOF

    cat > "$PKG_SCRIPTS/postinstall" << 'POSTINSTALL_EOF'
#!/usr/bin/env bash
LOG_DIR=/var/log/duckier
mkdir -p $LOG_DIR
mkdir -p /usr/local/share/duckiervpn
chmod 755 $LOG_DIR
echo "Post installation started at $(date)" >> $LOG_DIR/installer.log

# Load and start daemon
launchctl bootstrap system /Library/LaunchDaemons/com.duckier.vpn.daemon.plist 2>/dev/null || true
echo "Daemon started" >> $LOG_DIR/installer.log

echo "Installation complete at $(date)" >> $LOG_DIR/installer.log
exit 0
POSTINSTALL_EOF

    chmod 755 "$PKG_SCRIPTS/preinstall" "$PKG_SCRIPTS/postinstall"

    # pkgbuild — create component package
    pkgbuild \
        --root "$PKG_STAGE" \
        --identifier "$BUNDLE_ID" \
        --version "$VERSION" \
        --install-location "/" \
        --scripts "$PKG_SCRIPTS" \
        --min-os-version 10.15 \
        "$PKG_COMPONENT"

    # productbuild — create distribution package (installer UI)
    DIST_XML="$DIST/distribution-${ARCH}.xml"
    cat > "$DIST_XML" << DISTXML_EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>Duckier CLI</title>
    <organization>$BUNDLE_ID</organization>
    <domains enable_localSystem="true"/>
    <options customize="never" require-scripts="true" rootVolumeOnly="true"/>
    <volume-check>
        <allowed-os-versions>
            <os-version min="10.15"/>
        </allowed-os-versions>
    </volume-check>
    <choices-outline>
        <line choice="default">
            <line choice="$BUNDLE_ID"/>
        </line>
    </choices-outline>
    <choice id="default"/>
    <choice id="$BUNDLE_ID" visible="false">
        <pkg-ref id="$BUNDLE_ID"/>
    </choice>
    <pkg-ref id="$BUNDLE_ID" version="$VERSION" onConclusion="none">${ARCH}-component.pkg</pkg-ref>
</installer-gui-script>
DISTXML_EOF

    UNSIGNED_PKG="$DIST/duckier-cli-mac-${ARCH}-unsigned.pkg"
    productbuild \
        --distribution "$DIST_XML" \
        --package-path "$DIST" \
        "$UNSIGNED_PKG"

    # productsign — sign .pkg with installer identity
    INSTALLER_IDENTITY="${APPLE_INSTALLER_IDENTITY:-}"
    if [ "$SIGN" = true ] && [ -n "$INSTALLER_IDENTITY" ]; then
        echo "  $ARCH: Signing .pkg..."
        productsign --sign "$INSTALLER_IDENTITY" \
            --keychain "$KEYCHAIN" \
            "$UNSIGNED_PKG" "$PKG_FINAL"
        rm -f "$UNSIGNED_PKG"
    else
        mv "$UNSIGNED_PKG" "$PKG_FINAL"
    fi

    # Clean up intermediate files
    rm -rf "$PKG_STAGE" "$PKG_SCRIPTS" "$PKG_COMPONENT" "$DIST_XML"

    echo "  $ARCH: duckier-cli-mac-${ARCH}.pkg"
done

# ── Notarization ──
NOTARY_ARGS=""
if [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ] && [ -n "${APPLE_API_KEY_PATH:-}" ]; then
    NOTARY_ARGS="--key $APPLE_API_KEY_PATH --key-id $APPLE_API_KEY --issuer $APPLE_API_ISSUER"
fi

if [ "$SIGN" = true ] && [ -n "$NOTARY_ARGS" ]; then
    echo ""
    echo "Submitting .pkg installers for notarization..."

    SUBMISSION_IDS=()
    ARTIFACT_PATHS=()
    NOTARIZE_STATE="$DIST/.notarization.json"

    for ARCH in aarch64 x86_64; do
        PKG_FILE="$DIST/duckier-cli-mac-${ARCH}.pkg"
        echo "  Submitting: $(basename "$PKG_FILE")"
        OUTPUT=$(xcrun notarytool submit "$PKG_FILE" $NOTARY_ARGS 2>&1)
        SUB_ID=$(echo "$OUTPUT" | grep -o '[0-9a-f\-]\{36\}' | head -1)
        if [ -n "$SUB_ID" ]; then
            SUBMISSION_IDS+=("$SUB_ID")
            ARTIFACT_PATHS+=("$PKG_FILE")
            echo "    Submission ID: $SUB_ID"
        else
            echo "    ERROR: Failed to submit"
            echo "$OUTPUT"
        fi
    done

    if [ ${#SUBMISSION_IDS[@]} -eq 0 ]; then
        echo "No artifacts submitted for notarization."
    else
        # Save submission state for resume if interrupted
        echo "{" > "$NOTARIZE_STATE"
        echo "  \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"," >> "$NOTARIZE_STATE"
        echo "  \"version\": \"$VERSION\"," >> "$NOTARIZE_STATE"
        echo "  \"submissions\": [" >> "$NOTARIZE_STATE"
        for i in "${!SUBMISSION_IDS[@]}"; do
            COMMA=""
            if [ $i -gt 0 ]; then COMMA=","; fi
            cat >> "$NOTARIZE_STATE" << ENTRY
    ${COMMA}{
      "id": "${SUBMISSION_IDS[$i]}",
      "artifact": "${ARTIFACT_PATHS[$i]}"
    }
ENTRY
        done
        echo "  ]" >> "$NOTARIZE_STATE"
        echo "}" >> "$NOTARIZE_STATE"

        echo ""
        echo "Saved ${#SUBMISSION_IDS[@]} submission(s) to $NOTARIZE_STATE"
        echo "If interrupted, resume with: ./scripts/notarize-resume.sh"
        echo ""
        echo "Waiting for ${#SUBMISSION_IDS[@]} notarization(s)..."

        FAILED=0
        for i in "${!SUBMISSION_IDS[@]}"; do
            SUB_ID="${SUBMISSION_IDS[$i]}"
            ARTIFACT="${ARTIFACT_PATHS[$i]}"
            NAME=$(basename "$ARTIFACT")

            echo "  Waiting: $NAME ($SUB_ID)"
            if xcrun notarytool wait "$SUB_ID" $NOTARY_ARGS 2>&1 | grep -q "Accepted"; then
                echo "    Accepted. Stapling..."
                xcrun stapler staple "$ARTIFACT" 2>/dev/null || true
                echo "    Done: $NAME"
            else
                echo "    FAILED: $NAME"
                echo "    Run: xcrun notarytool log $SUB_ID $NOTARY_ARGS"
                FAILED=$((FAILED + 1))
            fi
        done

        if [ $FAILED -gt 0 ]; then
            echo ""
            echo "WARNING: $FAILED artifact(s) failed notarization."
        fi
    fi
elif [ "$SIGN" = true ]; then
    echo ""
    echo "Skipping notarization (APPLE_API_KEY or key file not available)"
fi

# Clean up per-arch staging dirs and downloaded daemon binaries
rm -rf "$DIST/aarch64" "$DIST/x86_64"
rm -rf "$CLI_DIR/binaries"

echo ""
echo "============================================"
echo " Build complete!"
echo ""
echo " Artifacts in $DIST:"
ls -lh "$DIST/"*.pkg "$DIST/"*.tar.gz 2>/dev/null
echo ""
echo " Install (.pkg): double-click or: sudo installer -pkg duckier-cli-mac-<arch>.pkg -target /"
echo " Install (.tar.gz): tar xzf duckier-cli-mac-<arch>.tar.gz && sudo ./install.sh"
echo "============================================"
