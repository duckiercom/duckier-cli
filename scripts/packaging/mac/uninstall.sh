#!/usr/bin/env bash
# Uninstall Duckier CLI + Daemon from macOS
#
# Usage: sudo ./uninstall.sh
set -e

if [ "$(id -u)" -ne 0 ]; then
    echo "Error: This script must be run as root (sudo)."
    exit 1
fi

echo "Uninstalling Duckier CLI + Daemon..."

# ── Shared daemon coexistence ──
# The daemon binary is shared with the desktop app (com.duckier.app).
# Only stop/remove the daemon if the desktop app is NOT installed.
DESKTOP_INSTALLED=false
if [ -d "/Applications/Duckier.app" ]; then
    DESKTOP_INSTALLED=true
fi

if [ "$DESKTOP_INSTALLED" = false ]; then
    # No desktop app — safe to stop and remove daemon
    if launchctl list com.duckier.vpn.daemon &>/dev/null; then
        echo "  Stopping daemon..."
        launchctl bootout system/com.duckier.vpn.daemon 2>/dev/null || true
    fi
    killall duckiervpn-daemon 2>/dev/null || true
    rm -f /usr/local/bin/duckiervpn-daemon
    rm -f /Library/LaunchDaemons/com.duckier.vpn.daemon.plist
    echo "  Removed daemon binary and launchd plist"
else
    echo "  Desktop app (Duckier.app) is installed — leaving daemon running."
    echo "  The desktop app will continue managing the daemon."
fi

# ── Always remove CLI binary ──
rm -f /usr/local/bin/duckier-cli
echo "  Removed CLI binary"

echo ""
echo "Uninstall complete."
echo "User config remains at ~/.config/duckier/ (remove manually if desired)"
