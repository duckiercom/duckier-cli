#!/usr/bin/env bash
set -eu

# ── Shared daemon coexistence ──
# Only reload systemd if the desktop app is NOT installed.
# If the desktop app is still present, it owns the daemon service.
DESKTOP_INSTALLED=false
if dpkg -l duckier 2>/dev/null | grep -q '^ii'; then
    DESKTOP_INSTALLED=true
fi

if [ "$DESKTOP_INSTALLED" = false ]; then
    if which systemctl &> /dev/null; then
        systemctl daemon-reload || true
    fi
fi
