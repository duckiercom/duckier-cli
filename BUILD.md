# Build Guide

## Prerequisites

| Tool | Required For | Install |
|------|-------------|---------|
| Rust 1.70+ | All builds | [rustup.rs](https://rustup.rs) |
| `protoc` | All builds | `brew install protobuf` / `apt install protobuf-compiler` |
| Docker | Linux packages | [docker.com](https://docs.docker.com/get-docker/) |
| `lipo` | macOS (bundled with Xcode) | `xcode-select --install` |

The build requires `vpn.proto` in the project root (included in the repo).

## Development Build

```bash
cargo check                # Type-check only (fast)
cargo build                # Debug build
cargo build --release      # Release build → target/release/duckier-cli (~2.0 MB)
cargo run -- status        # Run from source
```

Enable debug logging:

```bash
cargo run -- -v status
```

## Release Builds

### macOS

Builds universal (aarch64 + x86_64) tarballs with the daemon binary included.

```bash
./scripts/build-mac.sh              # Unsigned build
./scripts/build-mac.sh --sign       # Code-signed (requires APPLE_SIGNING_IDENTITY)
```

**What it does:**
1. Downloads macOS daemon binary (universal fat binary, split via `lipo`)
2. Builds CLI for `aarch64-apple-darwin`
3. Builds CLI for `x86_64-apple-darwin`
4. Optionally code-signs all binaries
5. Creates per-arch tarballs with install/uninstall scripts + launchd plist

**Artifacts** in `dist/mac/`:

```
dist/mac/
  duckier-cli-mac-aarch64.tar.gz    # Apple Silicon
  duckier-cli-mac-x86_64.tar.gz     # Intel
  aarch64/
    duckier-cli, duckiervpn-daemon, install.sh, uninstall.sh, *.plist
  x86_64/
    duckier-cli, duckiervpn-daemon, install.sh, uninstall.sh, *.plist
```

**Install from tarball:**

```bash
tar xzf duckier-cli-mac-aarch64.tar.gz
sudo ./install.sh
```

**Code signing** requires `scripts/sign-env.sh` (copy from `scripts/sign-env.sh.example`):

```bash
cp scripts/sign-env.sh.example scripts/sign-env.sh
# Edit scripts/sign-env.sh with your credentials
./scripts/build-mac.sh --sign
```

**macOS signing prerequisites:**

| Tool | Purpose | Install |
|------|---------|---------|
| Xcode Command Line Tools | `codesign`, `productsign`, `notarytool`, `stapler` | `xcode-select --install` |
| Developer ID Application certificate | Code signing binaries | [Apple Developer Portal](https://developer.apple.com/account/resources/certificates) |
| Developer ID Installer certificate | Signing `.pkg` installers | Same portal |
| App Store Connect API key (`.p8`) | Notarization | [App Store Connect > Keys](https://appstoreconnect.apple.com/access/integrations/api) |

The signing environment variables are documented in `scripts/sign-env.sh.example`.

### Linux

Builds DEB, RPM, and Arch packages inside Docker (cross-compiled on any host OS).

```bash
./scripts/build-linux.sh                   # All formats
./scripts/build-linux.sh --format deb      # Debian/Ubuntu only
./scripts/build-linux.sh --format rpm      # Fedora/RHEL only
./scripts/build-linux.sh --format arch     # Arch Linux only
```

**What it does:**
1. Downloads Linux x86_64 daemon binary
2. Copies `vpn.proto` into Docker context
3. Builds CLI inside Docker (`ubuntu:24.04` + Rust stable)
4. Packages as DEB, RPM, and generic tarball
5. Optionally builds Arch `.pkg.tar.zst` (uses `archlinux:latest` container)

**Artifacts** in `dist/linux/`:

```
dist/linux/
  deb/duckier-cli_VERSION_amd64.deb
  rpm/duckier-cli-VERSION-1.x86_64.rpm
  arch/duckier-cli-VERSION-1-x86_64.pkg.tar.zst
  generic/duckier-cli-linux-x86_64.tar.gz
```

**Install:**

```bash
# Debian/Ubuntu
sudo dpkg -i duckier-cli_2.0.13_amd64.deb

# Fedora/RHEL
sudo rpm -i duckier-cli-2.0.13-1.x86_64.rpm

# Arch
sudo pacman -U duckier-cli-2.0.13-1-x86_64.pkg.tar.zst

# Generic
tar xzf duckier-cli-linux-x86_64.tar.gz -C /usr/local
sudo systemctl enable --now duckiervpn-daemon
```

### Windows

Two build options: cross-compile from macOS or build natively on Windows.

**Cross-compile from macOS** (Azure Key Vault signing):

```bash
./scripts/build-windows.sh              # Unsigned
source scripts/sign-env.sh              # Load Azure credentials
./scripts/build-windows.sh --sign       # Signed via Azure Key Vault
```

**Native build on Windows** (hardware-token signing via YubiKey):

```powershell
.\scripts\build-windows-native.ps1                    # Unsigned
. .\scripts\packaging\win\sign-env.ps1                # Load signing config
.\scripts\build-windows-native.ps1 -Sign              # Signed via signtool + YubiKey
```

**Artifacts** in `dist/windows/`:

```
dist/windows/
  duckier-cli-windows-x64-setup.exe    # NSIS installer (CLI + daemon + service)
  duckier-cli.exe                      # Standalone CLI binary
```

**Windows cross-compile signing prerequisites** (macOS/Linux host):

| Tool | Purpose | Install |
|------|---------|---------|
| `cargo-xwin` | Cross-compile to Windows MSVC | `cargo install cargo-xwin` |
| `nsis` | Build NSIS installer | `brew install nsis` |
| `jsign` | Sign PE binaries via Azure Key Vault | `brew install jsign` |
| `azure-cli` | Authenticate to Key Vault | `brew install azure-cli` |
| Java 17+ | Required by jsign | `brew install openjdk@17` |
| Azure Key Vault | Code signing certificate + HSM | [Azure Portal](https://portal.azure.com) |

**Windows native signing prerequisites** (Windows host):

| Tool | Purpose | Install |
|------|---------|---------|
| Rust toolchain | Build CLI | [rustup.rs](https://rustup.rs) |
| `protoc` | Compile `.proto` | [GitHub releases](https://github.com/protocolbuffers/protobuf/releases) or `winget install Google.Protobuf` |
| NSIS | Build installer | [nsis.sourceforge.io](https://nsis.sourceforge.io) |
| Windows SDK | `signtool.exe` | [Visual Studio Installer](https://visualstudio.microsoft.com/) (select "Windows SDK") |
| GlobalSign minidriver | Makes YubiKey cert visible to signtool | [GlobalSign support](https://support.globalsign.com) |
| YubiKey | Hardware token with code signing certificate | Physical device, inserted via USB |

The signing environment variables are documented in:
- `scripts/sign-env.sh.example` — Azure Key Vault (cross-platform)
- `scripts/packaging/win/sign-env.ps1.example` — signtool + YubiKey (native Windows)

## Daemon Binary

The daemon (`duckiervpn-daemon`) is a separate binary downloaded from the update server. It's not built from this repo — it's bundled into packages by the build scripts.

Download manually:

```bash
./scripts/download-daemon.sh linux-x64     # Linux x86_64
./scripts/download-daemon.sh linux-arm64   # Linux ARM64
./scripts/download-daemon.sh mac           # macOS (universal → split to arm64 + x86_64)
```

Binaries are saved to `binaries/` (gitignored, cleaned up after packaging).

## How build.rs Works

The build script does two things at compile time:

1. **Compiles `vpn.proto`** into Rust types via `tonic-build` (for gRPC communication with the daemon)
2. **Embeds `config.toml` values** as `BRAND_*` environment variables, so all branding, URLs, and constants are baked in at compile time with zero runtime config

## Packaging Details

All packages include:
- `duckier-cli` CLI binary → `/usr/bin/duckier-cli` (Linux) or `/usr/local/bin/duckier-cli` (macOS)
- `duckiervpn-daemon` binary → `/usr/bin/duckiervpn-daemon` (Linux) or `/usr/local/bin/duckiervpn-daemon` (macOS)
- System service auto-start:
  - **Linux**: systemd unit (`duckiervpn-daemon.service`)
  - **macOS**: launchd plist (`com.duckier.vpn.daemon.plist`)

### Release Binary Optimizations

From `Cargo.toml [profile.release]`:
- `panic = "abort"` — no unwinding overhead
- `codegen-units = 1` — better optimization
- `lto = true` — link-time optimization
- `opt-level = "z"` — optimize for size
- `strip = true` — strip debug symbols

## Third-Party Notices

Regenerate the compact third-party attribution list (project, crates, versions, licenses, authors):

```bash
./scripts/generate-third-party-notices.sh
```

This updates `THIRD_PARTY_NOTICES.md` using `cargo metadata --locked` and requires `jq`.
