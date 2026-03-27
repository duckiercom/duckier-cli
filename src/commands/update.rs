use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Write;

use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::debug;

use crate::brand;
use crate::http_client::HttpClient;
use crate::output::Output;

/// Detected installation method determines how updates are applied.
pub(crate) enum InstallMethod {
    /// macOS Homebrew — run `brew upgrade`
    Homebrew,
    /// Linux apt/deb package — run `sudo apt update && sudo apt upgrade`
    Apt,
    /// Linux RPM package — run `sudo dnf upgrade` or `sudo yum upgrade`
    Rpm,
    /// Linux Arch package — run `sudo pacman -Syu`
    Arch,
    /// macOS .pkg installer — download and run `sudo installer -pkg`
    MacInstaller,
    /// Windows .exe installer — download and run elevated
    WindowsInstaller,
    /// Unknown Linux package manager — can only report, not auto-update
    LinuxManual,
}

/// Simple semver tuple for comparison.
type SemVer = (u32, u32, u32);

/// Parse a "major.minor.patch" string into a tuple. Returns None for invalid input.
fn parse_semver(s: &str) -> Option<SemVer> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// Build the version-check path for this OS + architecture + install method.
fn version_path(method: &InstallMethod) -> &'static str {
    match method {
        InstallMethod::Homebrew | InstallMethod::MacInstaller => {
            if cfg!(target_arch = "aarch64") {
                "/version/cli/mac"
            } else {
                "/version/cli/mac/x86"
            }
        }
        InstallMethod::WindowsInstaller => "/version/cli/win",
        InstallMethod::Apt => {
            if cfg!(target_arch = "aarch64") {
                "/version/cli/linux/deb/arm64"
            } else {
                "/version/cli/linux/deb/amd64"
            }
        }
        InstallMethod::Rpm => {
            if cfg!(target_arch = "aarch64") {
                "/version/cli/linux/rpm/aarch64"
            } else {
                "/version/cli/linux/rpm/x86_64"
            }
        }
        InstallMethod::Arch => {
            if cfg!(target_arch = "aarch64") {
                "/version/cli/linux/arch/aarch64"
            } else {
                "/version/cli/linux/arch/x86_64"
            }
        }
        InstallMethod::LinuxManual => {
            // Best-effort: use deb/amd64 as the most common Linux variant
            if cfg!(target_arch = "aarch64") {
                "/version/cli/linux/deb/arm64"
            } else {
                "/version/cli/linux/deb/amd64"
            }
        }
    }
}

/// Detect how the CLI was installed by inspecting the binary path.
pub(crate) fn detect_install_method() -> Result<InstallMethod> {
    if cfg!(target_os = "windows") {
        return Ok(InstallMethod::WindowsInstaller);
    }

    let exe_path = std::env::current_exe().context("failed to locate current executable")?;
    let resolved = std::fs::canonicalize(&exe_path)
        .with_context(|| format!("failed to resolve {}", exe_path.display()))?;
    let resolved_str = resolved.to_string_lossy();

    if cfg!(target_os = "macos") {
        if resolved_str.contains("/Cellar/") || resolved_str.contains("/homebrew/") {
            return Ok(InstallMethod::Homebrew);
        }
        return Ok(InstallMethod::MacInstaller);
    }

    // Linux — check dpkg (Debian/Ubuntu), rpm (Fedora/RHEL), pacman (Arch)
    if is_pkg_managed("dpkg", &["-S", &resolved_str]) {
        return Ok(InstallMethod::Apt);
    }
    if is_pkg_managed("rpm", &["-qf", &resolved_str])
        && (which_exists("dnf") || which_exists("yum"))
    {
        return Ok(InstallMethod::Rpm);
    }
    if is_pkg_managed("pacman", &["-Qo", &resolved_str]) {
        return Ok(InstallMethod::Arch);
    }

    // No known package manager manages this binary
    Ok(InstallMethod::LinuxManual)
}

/// Check whether a file path is managed by a package manager.
fn is_pkg_managed(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Prompt the user for Y/N confirmation. Returns true if they accept.
fn confirm(prompt: &str) -> bool {
    eprint!("{} [Y/n] ", prompt);
    let _ = std::io::stderr().flush();
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    let trimmed = input.trim().to_lowercase();
    trimmed.is_empty() || trimmed == "y" || trimmed == "yes"
}

pub async fn run(out: &Output) -> Result<i32> {
    let current_version = env!("CARGO_PKG_VERSION");

    // Detect install method first — needed to build the correct version-check URL
    let method = detect_install_method()?;

    let version_url = format!("{}{}", brand::UPDATE_URL, version_path(&method));
    debug!("Checking for updates: {}", version_url);

    let client = HttpClient::new();
    let resp = match client.get(&version_url, &HashMap::new()) {
        Ok(r) => r,
        Err(e) => {
            debug!("Update check failed: {}", e);
            out.error(&format!("Could not reach update server: {}", e));
            return Ok(1);
        }
    };

    if !resp.is_success() {
        let status = resp.status_code();
        debug!("Update server returned {}", status);
        out.error(&format!("Update server returned HTTP {}", status));
        return Ok(1);
    }

    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => {
            debug!("Failed to parse version response: {}", e);
            out.error(&format!("Invalid response from update server: {}", e));
            return Ok(1);
        }
    };

    // Server returned an error payload
    if body.get("error").is_some() || body.get("errorCode").is_some() {
        debug!("Update server error payload: {:?}", body);
        out.error("Update server returned an error");
        return Ok(1);
    }

    let remote_version = match body["version"].as_str() {
        Some(v) if !v.is_empty() => v,
        _ => {
            debug!("No version in response: {:?}", body);
            out.error("Update server did not return a version");
            return Ok(1);
        }
    };

    if remote_version == current_version {
        if out.is_json() {
            out.print_json(&serde_json::json!({
                "status": "up_to_date",
                "version": current_version,
            }));
        } else {
            out.success(&format!("Already up to date (v{})", current_version));
        }
        return Ok(0);
    }

    // Parse and compare semver — only upgrade, never downgrade
    let current_sv = parse_semver(current_version);
    let remote_sv = parse_semver(remote_version);

    match (current_sv, remote_sv) {
        (Some(cur), Some(rem)) if rem <= cur => {
            if out.is_json() {
                out.print_json(&serde_json::json!({
                    "status": "up_to_date",
                    "version": current_version,
                }));
            } else {
                out.success(&format!("Already up to date (v{})", current_version));
            }
            return Ok(0);
        }
        (_, None) => {
            debug!(
                "Server returned invalid version string: {:?}",
                remote_version
            );
            out.error(&format!(
                "Update server returned invalid version: {}",
                remote_version
            ));
            return Ok(1);
        }
        _ => {}
    }

    let method_label = match &method {
        InstallMethod::Homebrew => "Homebrew",
        InstallMethod::Apt => "apt",
        InstallMethod::Rpm => "rpm",
        InstallMethod::Arch => "pacman",
        InstallMethod::MacInstaller => "installer",
        InstallMethod::WindowsInstaller => "installer",
        InstallMethod::LinuxManual => "manual",
    };

    // In JSON mode, report what's available without interactive prompt
    if out.is_json() {
        out.print_json(&serde_json::json!({
            "status": "update_available",
            "current_version": current_version,
            "latest_version": remote_version,
            "install_method": method_label,
        }));
        return Ok(0);
    }

    out.println(&format!(
        "Update available: v{} → v{} (installed via {})",
        current_version, remote_version, method_label
    ));

    // LinuxManual — we can't auto-update, just inform the user
    if matches!(method, InstallMethod::LinuxManual) {
        out.println("");
        out.println("Could not detect package manager. Please update manually using your");
        out.println("distribution's package manager or download the latest release.");
        return Ok(0);
    }

    if !confirm("Do you want to update now?") {
        out.println("Update cancelled.");
        return Ok(0);
    }

    match method {
        InstallMethod::Homebrew => run_homebrew_upgrade(remote_version, out),
        InstallMethod::Apt => run_apt_upgrade(remote_version, out),
        InstallMethod::Rpm => run_rpm_upgrade(remote_version, out),
        InstallMethod::Arch => run_arch_upgrade(remote_version, out),
        InstallMethod::MacInstaller => {
            download_and_run_installer(&client, &body, remote_version, out)
        }
        InstallMethod::WindowsInstaller => {
            download_and_run_installer(&client, &body, remote_version, out)
        }
        InstallMethod::LinuxManual => unreachable!(),
    }
}

/// Run `brew upgrade duckier-cli`.
fn run_homebrew_upgrade(remote_version: &str, out: &Output) -> Result<i32> {
    out.println("Running: brew upgrade duckier-cli");

    let status = std::process::Command::new("brew")
        .args(["upgrade", "duckier-cli"])
        .status();

    match status {
        Ok(s) if s.success() => {
            out.success(&format!("Updated to v{}", remote_version));
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("brew upgrade exited with code {}", code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run brew: {}", e));
            Ok(1)
        }
    }
}

/// Run `sudo apt update && sudo apt upgrade -y duckier-cli`.
fn run_apt_upgrade(remote_version: &str, out: &Output) -> Result<i32> {
    out.println("Running: sudo apt update && sudo apt upgrade -y duckier-cli");

    let update_status = std::process::Command::new("sudo")
        .args(["apt", "update"])
        .status();

    match update_status {
        Ok(s) if !s.success() => {
            out.error(&format!(
                "apt update exited with code {}",
                s.code().unwrap_or(1)
            ));
            return Ok(1);
        }
        Err(e) => {
            out.error(&format!("Failed to run apt update: {}", e));
            return Ok(1);
        }
        _ => {}
    }

    let upgrade_status = std::process::Command::new("sudo")
        .args(["apt", "upgrade", "-y", "duckier-cli"])
        .status();

    match upgrade_status {
        Ok(s) if s.success() => {
            out.success(&format!("Updated to v{}", remote_version));
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("apt upgrade exited with code {}", code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run apt upgrade: {}", e));
            Ok(1)
        }
    }
}

/// Run `sudo dnf upgrade -y duckier-cli` (falls back to yum).
fn run_rpm_upgrade(remote_version: &str, out: &Output) -> Result<i32> {
    // Prefer dnf, fall back to yum
    let (mgr, subcmd, args) = if which_exists("dnf") {
        (
            "dnf",
            "upgrade",
            vec!["dnf", "upgrade", "-y", "duckier-cli"],
        )
    } else {
        ("yum", "update", vec!["yum", "update", "-y", "duckier-cli"])
    };

    out.println(&format!("Running: sudo {} {} -y duckier-cli", mgr, subcmd));

    let status = std::process::Command::new("sudo").args(&args).status();

    match status {
        Ok(s) if s.success() => {
            out.success(&format!("Updated to v{}", remote_version));
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("{} exited with code {}", mgr, code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run {}: {}", mgr, e));
            Ok(1)
        }
    }
}

/// Run `sudo pacman -Syu --noconfirm duckier-cli`.
fn run_arch_upgrade(remote_version: &str, out: &Output) -> Result<i32> {
    out.println("Running: sudo pacman -Syu --noconfirm duckier-cli");

    let status = std::process::Command::new("sudo")
        .args(["pacman", "-Syu", "--noconfirm", "duckier-cli"])
        .status();

    match status {
        Ok(s) if s.success() => {
            out.success(&format!("Updated to v{}", remote_version));
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("pacman exited with code {}", code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run pacman: {}", e));
            Ok(1)
        }
    }
}

/// Check if a command exists on PATH (works even if `which` is not installed).
pub(crate) fn which_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Download the installer package, verify its checksum, and run it.
fn download_and_run_installer(
    client: &HttpClient,
    body: &Value,
    remote_version: &str,
    out: &Output,
) -> Result<i32> {
    let download_url = match body["url"].as_str() {
        Some(u) if !u.is_empty() => u,
        _ => {
            out.error("Update available but no download URL provided");
            return Ok(1);
        }
    };

    let expected_sha = match body["sha256"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => {
            out.error("Update server did not provide a checksum — aborting for safety");
            return Ok(1);
        }
    };

    out.println("Downloading update...");

    debug!("Downloading installer from {}", download_url);

    let dl_resp = client
        .get(download_url, &HashMap::new())
        .with_context(|| format!("failed to download installer from {}", download_url))?;
    if !dl_resp.is_success() {
        out.error(&format!(
            "Failed to download update (HTTP {})",
            dl_resp.status_code()
        ));
        return Ok(1);
    }

    let bytes = dl_resp.bytes();

    // Verify SHA256
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual_sha = hex::encode(hasher.finalize());
    if actual_sha != expected_sha {
        out.error("SHA256 checksum mismatch — aborting update");
        debug!("Expected: {}, got: {}", expected_sha, actual_sha);
        return Ok(1);
    }
    debug!("SHA256 verified: {}", actual_sha);

    // Write installer to temp file
    let tmp_name = if cfg!(target_os = "windows") {
        "duckier-cli-update.exe"
    } else {
        "duckier-cli-update.pkg"
    };
    let tmp_path = std::env::temp_dir().join(tmp_name);
    {
        let mut tmp_file = std::fs::File::create(&tmp_path)
            .with_context(|| format!("failed to create {}", tmp_path.display()))?;
        tmp_file
            .write_all(bytes)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        tmp_file
            .flush()
            .with_context(|| format!("failed to flush {}", tmp_path.display()))?;
    }

    // Run the installer
    if cfg!(target_os = "macos") {
        run_mac_installer(&tmp_path, remote_version, out)
    } else {
        run_windows_installer(&tmp_path, remote_version, out)
    }
}

/// Run a .pkg installer on macOS via `sudo installer -pkg`.
fn run_mac_installer(
    pkg_path: &std::path::Path,
    remote_version: &str,
    out: &Output,
) -> Result<i32> {
    out.println("Running installer...");

    let status = std::process::Command::new("sudo")
        .args(["installer", "-pkg"])
        .arg(pkg_path)
        .args(["-target", "/"])
        .status();

    let _ = std::fs::remove_file(pkg_path);

    match status {
        Ok(s) if s.success() => {
            out.success(&format!("Updated to v{}", remote_version));
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("Installer exited with code {}", code));
            out.println(&format!(
                "  Try running: sudo {} update",
                brand::BINARY_NAME
            ));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run installer: {}", e));
            Ok(1)
        }
    }
}

/// Run an .exe installer on Windows (elevated).
fn run_windows_installer(
    exe_path: &std::path::Path,
    remote_version: &str,
    out: &Output,
) -> Result<i32> {
    out.println("Running installer...");

    // On Windows, use the exe directly. The installer handles elevation via its manifest.
    let status = std::process::Command::new(exe_path).status();

    let _ = std::fs::remove_file(exe_path);

    match status {
        Ok(s) if s.success() => {
            out.success(&format!("Updated to v{}", remote_version));
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("Installer exited with code {}", code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run installer: {}", e));
            Ok(1)
        }
    }
}
