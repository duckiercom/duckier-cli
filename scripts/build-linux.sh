#!/bin/bash
# Build Duckier CLI + Daemon for Linux using Docker
# Cross-compiles for both x86_64 and aarch64 using Alpine + musl.
#
# Usage:
#   ./scripts/build-linux.sh [--format deb|rpm|arch|all]
#
# Artifacts land in dist/linux/{deb,rpm,arch,generic}/
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(dirname "$SCRIPT_DIR")"

# Parse --format flag
FORMAT="all"
while [[ $# -gt 0 ]]; do
    case $1 in
        --format)
            FORMAT="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--format deb|rpm|arch|all]"
            exit 1
            ;;
    esac
done

echo "============================================"
echo " Duckier CLI — Linux build (x86_64 + aarch64)"
echo " Format: $FORMAT"
echo "============================================"

# ── Preflight ──
if ! command -v docker &> /dev/null; then
    echo "Error: Docker is not installed or not in PATH"
    exit 1
fi
if ! docker info &> /dev/null; then
    echo "Error: Docker daemon is not running"
    exit 1
fi

export DOCKER_BUILDKIT=1

VERSION=$(grep '^version' "$CLI_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')

# ── 1. Download daemon binaries (both architectures) ──
echo ""
echo "[1/4] Downloading Linux daemon binaries..."
"$SCRIPT_DIR/download-daemon.sh" linux-x64
mv "$CLI_DIR/binaries/duckiervpn-daemon" "$CLI_DIR/binaries/duckiervpn-daemon-amd64"

"$SCRIPT_DIR/download-daemon.sh" linux-arm64
mv "$CLI_DIR/binaries/duckiervpn-daemon" "$CLI_DIR/binaries/duckiervpn-daemon-arm64"

echo "  Binaries ready:"
ls -lh "$CLI_DIR/binaries/"

# ── 2. Copy vpn.proto for Docker context ──
# Proto file lives outside this project — copy it in temporarily
PROTO_CANDIDATES=(
    "$CLI_DIR/vpn.proto"
    "$CLI_DIR/../vpn_mobile/vpn.proto"
    "$CLI_DIR/../vpn.proto"
)
PROTO_FOUND=false
for candidate in "${PROTO_CANDIDATES[@]}"; do
    if [ -f "$candidate" ]; then
        if [ "$candidate" != "$CLI_DIR/vpn.proto" ]; then
            cp "$candidate" "$CLI_DIR/vpn.proto"
        fi
        PROTO_FOUND=true
        break
    fi
done

if [ "$PROTO_FOUND" = false ]; then
    echo "Error: vpn.proto not found. Searched:"
    for c in "${PROTO_CANDIDATES[@]}"; do echo "  $c"; done
    exit 1
fi

# ── 3. Build Docker image ──
echo ""
echo "[2/4] Building Docker image (Alpine + musl cross-compilation)..."
docker build \
    --platform linux/amd64 \
    -f "$CLI_DIR/Dockerfile.linux" \
    -t duckier-cli-linux-builder \
    "$CLI_DIR"

# Clean up temp files from Docker context
rm -rf "$CLI_DIR/binaries"

# ── 4. Extract artifacts ──
echo ""
echo "[3/4] Extracting build artifacts..."

DIST="$CLI_DIR/dist/linux"
rm -rf "$DIST"
mkdir -p "$DIST"

CONTAINER_ID=$(docker create --platform linux/amd64 duckier-cli-linux-builder)

case $FORMAT in
    deb)
        mkdir -p "$DIST/deb"
        docker cp "$CONTAINER_ID:/output/deb/." "$DIST/deb/" 2>/dev/null || true
        ;;
    rpm)
        mkdir -p "$DIST/rpm"
        docker cp "$CONTAINER_ID:/output/rpm/." "$DIST/rpm/" 2>/dev/null || true
        ;;
    arch)
        mkdir -p "$DIST/generic"
        docker cp "$CONTAINER_ID:/output/duckier-cli-linux-x86_64.tar.gz" "$DIST/generic/" 2>/dev/null || true
        docker cp "$CONTAINER_ID:/output/duckier-cli-linux-aarch64.tar.gz" "$DIST/generic/" 2>/dev/null || true
        ;;
    all)
        mkdir -p "$DIST/deb" "$DIST/rpm" "$DIST/generic"
        docker cp "$CONTAINER_ID:/output/deb/." "$DIST/deb/" 2>/dev/null || true
        docker cp "$CONTAINER_ID:/output/rpm/." "$DIST/rpm/" 2>/dev/null || true
        docker cp "$CONTAINER_ID:/output/duckier-cli-linux-x86_64.tar.gz" "$DIST/generic/" 2>/dev/null || true
        docker cp "$CONTAINER_ID:/output/duckier-cli-linux-aarch64.tar.gz" "$DIST/generic/" 2>/dev/null || true
        ;;
esac

docker rm "$CONTAINER_ID" > /dev/null

# ── 5. Build Arch packages (if requested) ──
if [ "$FORMAT" = "arch" ] || [ "$FORMAT" = "all" ]; then
    for TAR_ARCH in x86_64 aarch64; do
        TARBALL="$DIST/generic/duckier-cli-linux-${TAR_ARCH}.tar.gz"
        if [ -f "$TARBALL" ]; then
            echo ""
            echo "[3b/4] Building Arch package (${TAR_ARCH})..."
            mkdir -p "$DIST/arch"

            docker run --rm --platform linux/amd64 \
                --security-opt seccomp=unconfined \
                --privileged \
                -v "$DIST/generic:/build/src:ro" \
                -v "$CLI_DIR/scripts/packaging/arch:/build/pkg:ro" \
                -v "$DIST/arch:/output" \
                archlinux:latest bash -c "
                    # Disable pacman sandboxing (fails under QEMU emulation)
                    sed -i 's/^#*\s*DisableSandbox.*/DisableSandbox/' /etc/pacman.conf
                    grep -q '^DisableSandbox' /etc/pacman.conf || echo 'DisableSandbox' >> /etc/pacman.conf
                    pacman -Sy --noconfirm base-devel debugedit fakeroot
                    useradd -m builder
                    mkdir -p /tmp/build
                    cp /build/pkg/PKGBUILD /tmp/build/
                    sed -i 's/^pkgver=.*/pkgver=${VERSION}/' /tmp/build/PKGBUILD
                    cp /build/pkg/duckier-cli.install /tmp/build/
                    cp /build/src/duckier-cli-linux-${TAR_ARCH}.tar.gz /tmp/build/
                    # Override architecture for cross-arch packaging
                    sed -i \"s/CARCH=.*/CARCH='${TAR_ARCH}'/\" /etc/makepkg.conf
                    chown -R builder:builder /tmp/build
                    su builder -c 'cd /tmp/build && makepkg -d --skipchecksums --nocheck'
                    cp /tmp/build/*.pkg.tar.zst /output/
                "
        else
            echo ""
            echo "Warning: Cannot build Arch package for ${TAR_ARCH} — tarball not found"
        fi
    done
fi

# ── Done ──
echo ""
echo "[4/4] Build complete!"
echo ""
echo "Artifacts in $DIST:"
for dir in deb rpm arch generic; do
    if [ -d "$DIST/$dir" ]; then
        echo ""
        echo "  $dir/"
        ls -lh "$DIST/$dir/" 2>/dev/null | tail -n +2 || true
    fi
done
echo ""
echo "============================================"
