# Duckier CLI

Command-line client for [Duckier](https://duckier.com). A single, statically-linked Rust binary that runs headless on Linux, macOS, and Windows.

The CLI communicates with the Duckier daemon over gRPC for tunnel management and with the backend API over HTTPS for authentication, server discovery, and key exchange.

## Installation

### macOS

```bash
# Install from .pkg (recommended)
sudo installer -pkg duckier-cli-mac-aarch64.pkg -target /

# Or extract from tarball
tar xzf duckier-cli-mac-aarch64.tar.gz
sudo mv duckier-cli /usr/local/bin/
```

### Linux

```bash
# Debian / Ubuntu
sudo dpkg -i duckier-cli_2.0.13_amd64.deb

# RPM-based
sudo rpm -i duckier-cli-2.0.13-1.x86_64.rpm

# Arch
sudo pacman -U duckier-cli-2.0.13-1-x86_64.pkg.tar.zst
```

## Quick Start

```bash
duckier-cli connect                         # Connect to the fastest server
duckier-cli connect --country DE            # Connect to a German server
duckier-cli status                          # Show connection status
duckier-cli disconnect                      # Disconnect
```

No account required — an anonymous session is created automatically on first use. To unlock Pro servers, link your Duckier account:

```bash
duckier-cli login                           # Displays a connection code
# Enter the code at https://duckier.com/connect
```

## Commands

| Command | Description |
|---------|-------------|
| `connect [--country CC] [--city NAME]` | Connect to a VPN server |
| `disconnect` | Disconnect the active tunnel |
| `status` | Show connection and daemon status |
| `servers [--country CC]` | List available servers |
| `login` | Link to a Duckier account via connection code |
| `logout` | Log out and clear stored credentials |
| `account` | Show account and subscription info |
| `killswitch enable [--allow-lan]` | Enable the network kill switch |
| `killswitch disable` | Disable the kill switch |
| `killswitch status` | Show kill switch status |
| `daemon health` | Check daemon health |
| `daemon pid` | Show daemon process ID |
| `update` | Check for and apply CLI updates |

### Global Flags

| Flag | Description |
|------|-------------|
| `--json` | Machine-readable JSON output |
| `--daemon-addr ADDR` | Override the gRPC daemon address |

## Architecture

The CLI is a thin frontend. All tunnel operations are delegated to the **Duckier daemon**, a privileged system service that manages WireGuard interfaces, routing, and the kill switch. Communication between the CLI and daemon uses gRPC with streaming responses.

```
 duckier-cli                    duckier daemon
┌─────────────┐    gRPC     ┌──────────────────┐
│  commands/*  │───────────▶│  WireGuard / KS   │
│  api client  │            │  system routes    │
└──────┬──────┘            └──────────────────┘
       │
       │  HTTPS
       ▼
┌─────────────┐
│  Duckier API │
│  auth, keys  │
│  servers     │
└─────────────┘
```

### Reconnect Handling

The CLI sends a `cli_mode` flag with every connection request. When the daemon's built-in reconnect (cached config, exponential backoff) is exhausted, it invokes `duckier-cli connect` as a subprocess to fetch fresh server configuration from the API and re-establish the tunnel — no persistent gRPC stream required.

## Building

### Requirements

- Rust 1.70+
- `protoc` (Protocol Buffers compiler)

### Development

```bash
cargo check                                 # Type-check
cargo build --release                       # Release binary → target/release/duckier-cli
cargo run -- status                         # Run from source
```

### Packaging

```bash
./scripts/build-mac.sh [--sign]             # macOS: .pkg + .tar.gz per architecture
./scripts/build-linux.sh [--format deb|rpm|arch|all]  # Linux: via Docker
./scripts/download-daemon.sh <platform>     # Download daemon binary only
```

Build artifacts are written to `dist/mac/` or `dist/linux/`.

## Configuration

All branding, URLs, and constants live in `config.toml` and are embedded at compile time. Change this file to white-label the CLI for a different VPN brand.

### Storage

Credentials and cached data are stored in `~/.config/duckier/`:

| File | Contents |
|------|----------|
| `auth.json` | Auth token, device ID, account info |
| `wireguard.json` | WireGuard keypair (tunnel address comes from the backend per connect) |
| `cache/appconfig.json` | Server list and feature flags (1 h TTL) |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `2` | Authentication required |
| `3` | Connection failed |

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE) (`GPL-3.0-only`).

Additional attribution files:
- [COPYRIGHT](COPYRIGHT)
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)

To regenerate third-party notices after dependency changes:

```bash
./scripts/generate-third-party-notices.sh
```
