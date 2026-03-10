Name:           duckier-cli
Version:        __VERSION__
Release:        1
Summary:        DuckierVPN CLI — headless VPN client with daemon
License:        GPL-3.0-only
URL:            https://duckier.com

%description
Command-line interface and daemon for DuckierVPN. Connects via WireGuard.
Includes the VPN daemon service for headless/server deployments.

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/systemd/system
install -m 755 %{_sourcedir}/duckier-cli %{buildroot}/usr/bin/duckier-cli
install -m 755 %{_sourcedir}/duckiervpn-daemon %{buildroot}/usr/bin/duckiervpn-daemon
install -m 644 %{_sourcedir}/duckiervpn-daemon.service %{buildroot}/usr/lib/systemd/system/duckiervpn-daemon.service

%pre
mkdir -p /var/log/duckier
if which systemctl &> /dev/null; then
    systemctl stop duckiervpn-daemon.service 2>/dev/null || true
    systemctl disable duckiervpn-daemon.service 2>/dev/null || true
fi

%post
if [ -f /usr/lib/systemd/system/duckiervpn-daemon.service ]; then
    systemctl daemon-reload
    systemctl enable duckiervpn-daemon.service
    systemctl start duckiervpn-daemon.service
fi

%preun
# Shared daemon coexistence: only stop daemon if desktop app is NOT installed
if ! rpm -q duckier &>/dev/null; then
    if which systemctl &> /dev/null; then
        systemctl stop duckiervpn-daemon.service 2>/dev/null || true
        systemctl disable duckiervpn-daemon.service 2>/dev/null || true
    fi
    killall duckiervpn-daemon 2>/dev/null || true
else
    echo "Desktop app (duckier) is installed — leaving daemon running."
fi

%postun
# Only reload systemd if desktop app is NOT installed
if ! rpm -q duckier &>/dev/null; then
    systemctl daemon-reload 2>/dev/null || true
fi

%files
%attr(755, root, root) /usr/bin/duckier-cli
%attr(755, root, root) /usr/bin/duckiervpn-daemon
%attr(644, root, root) /usr/lib/systemd/system/duckiervpn-daemon.service
