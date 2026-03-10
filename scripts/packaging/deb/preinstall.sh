#!/usr/bin/env bash
set -eu

mkdir -p /var/log/duckier

# Stop existing daemon if running
if which systemctl &> /dev/null; then
    if systemctl status duckiervpn-daemon &> /dev/null; then
        systemctl stop duckiervpn-daemon.service || true
        systemctl disable duckiervpn-daemon.service || true
    fi
    # Also check old daemon name
    if systemctl status duckier-daemon &> /dev/null; then
        systemctl stop duckier-daemon.service || true
        systemctl disable duckier-daemon.service || true
    fi
fi
