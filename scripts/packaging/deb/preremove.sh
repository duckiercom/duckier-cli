#!/usr/bin/env bash
set -eu

# ── Shared daemon coexistence ──
# The daemon is shared with the desktop app (duckier package).
# Only stop/disable the daemon if the desktop app is NOT installed,
# otherwise the desktop app would lose its daemon too.
DESKTOP_INSTALLED=false
if dpkg -l duckier 2>/dev/null | grep -q '^ii'; then
    DESKTOP_INSTALLED=true
fi

if [ "$DESKTOP_INSTALLED" = false ]; then
    # No desktop app — safe to stop the daemon
    if which systemctl &> /dev/null; then
        systemctl stop duckiervpn-daemon.service 2>/dev/null || true
        systemctl disable duckiervpn-daemon.service 2>/dev/null || true
    fi
    killall duckiervpn-daemon 2>/dev/null || true
else
    echo "Desktop app (duckier) is installed — leaving daemon running."
fi
