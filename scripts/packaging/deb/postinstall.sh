#!/usr/bin/env bash
set -eu

# Enable and start the systemd service
if [ -f "/usr/lib/systemd/system/duckiervpn-daemon.service" ]; then
    systemctl daemon-reload
    systemctl enable "/usr/lib/systemd/system/duckiervpn-daemon.service"
    systemctl start duckiervpn-daemon.service
fi
