#!/usr/bin/env bash
# Install Duckier CLI + Daemon on macOS
#
# Usage: sudo ./install.sh
#
# Installs:
#   /usr/local/bin/duckier-cli           — CLI binary
#   /usr/local/bin/duckiervpn-daemon    — VPN daemon (runs as root)
#   /Library/LaunchDaemons/com.duckier.vpn.daemon.plist — launchd service
set -e

if [ "$(id -u)" -ne 0 ]; then
    echo "Error: This script must be run as root (sudo)."
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing Duckier CLI + Daemon..."

# ── Stop existing daemon service ──
# Both CLI and desktop app share the same daemon (com.duckier.vpn.daemon).
if launchctl list com.duckier.vpn.daemon &>/dev/null; then
    echo "  Stopping existing daemon service..."
    launchctl bootout system/com.duckier.vpn.daemon 2>/dev/null || true
fi
killall duckiervpn-daemon 2>/dev/null || true

# ── Install binaries ──
install -m 755 "$SCRIPT_DIR/duckier-cli" /usr/local/bin/duckier-cli
install -m 755 "$SCRIPT_DIR/duckiervpn-daemon" /usr/local/bin/duckiervpn-daemon
echo "  Installed binaries to /usr/local/bin/"

# ── Install launchd plist ──
install -m 644 "$SCRIPT_DIR/com.duckier.vpn.daemon.plist" /Library/LaunchDaemons/
echo "  Installed launchd plist"

# ── Create log directory ──
mkdir -p /var/log/duckier
mkdir -p /usr/local/share/duckiervpn

# ── Load and start daemon ──
launchctl bootstrap system /Library/LaunchDaemons/com.duckier.vpn.daemon.plist
echo "  Daemon started"

echo ""
echo "Installation complete!"
echo "  CLI:    /usr/local/bin/duckier-cli"
echo "  Daemon: running (launchctl list com.duckier.vpn.daemon)"
echo ""
echo "To uninstall: duckier-cli uninstall"
