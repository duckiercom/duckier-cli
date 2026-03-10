use anyhow::{Context, Result};
use std::io::Write;

use super::update::{detect_install_method, which_exists, InstallMethod};
use crate::brand;
use crate::output::Output;

/// Prompt the user for Y/N confirmation, defaulting to No (destructive action).
fn confirm(prompt: &str) -> bool {
    eprint!("{} [y/N] ", prompt);
    let _ = std::io::stderr().flush();
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    let trimmed = input.trim().to_lowercase();
    trimmed == "y" || trimmed == "yes"
}

pub async fn run(out: &Output) -> Result<i32> {
    if out.is_json() {
        out.print_json(&serde_json::json!({
            "status": "error",
            "message": "Uninstall requires interactive confirmation. Run without --json.",
        }));
        return Ok(1);
    }

    if !confirm(&format!(
        "Are you sure you want to uninstall {}?",
        brand::PRODUCT_NAME
    )) {
        out.println("Uninstall cancelled.");
        return Ok(0);
    }

    let method = detect_install_method().context("failed to detect how the CLI was installed")?;

    match method {
        InstallMethod::Homebrew => run_homebrew_uninstall(out),
        InstallMethod::Apt => run_apt_uninstall(out),
        InstallMethod::Rpm => run_rpm_uninstall(out),
        InstallMethod::Arch => run_arch_uninstall(out),
        InstallMethod::MacInstaller => run_mac_uninstall(out),
        InstallMethod::WindowsInstaller => run_windows_uninstall(out),
        InstallMethod::LinuxManual => run_linux_manual_uninstall(out),
    }
}

fn run_homebrew_uninstall(out: &Output) -> Result<i32> {
    out.println(&format!("Running: brew uninstall {}", brand::BINARY_NAME));

    let status = std::process::Command::new("brew")
        .args(["uninstall", brand::BINARY_NAME])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("brew uninstall exited with code {}", code));
            return Ok(1);
        }
        Err(e) => {
            out.error(&format!("Failed to run brew: {}", e));
            return Ok(1);
        }
    }

    // Daemon may have been installed separately via .pkg — clean up if present
    cleanup_mac_daemon(out);
    print_success(out);
    Ok(0)
}

fn run_apt_uninstall(out: &Output) -> Result<i32> {
    out.println(&format!(
        "Running: sudo apt remove -y {}",
        brand::BINARY_NAME
    ));

    let status = std::process::Command::new("sudo")
        .args(["apt", "remove", "-y", brand::BINARY_NAME])
        .status();

    match status {
        Ok(s) if s.success() => {
            print_success(out);
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("apt remove exited with code {}", code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run apt: {}", e));
            Ok(1)
        }
    }
}

fn run_rpm_uninstall(out: &Output) -> Result<i32> {
    let (mgr, args) = if which_exists("dnf") {
        ("dnf", vec!["dnf", "remove", "-y", brand::BINARY_NAME])
    } else {
        ("yum", vec!["yum", "remove", "-y", brand::BINARY_NAME])
    };

    out.println(&format!(
        "Running: sudo {} remove -y {}",
        mgr,
        brand::BINARY_NAME
    ));

    let status = std::process::Command::new("sudo").args(&args).status();

    match status {
        Ok(s) if s.success() => {
            print_success(out);
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("{} remove exited with code {}", mgr, code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run {}: {}", mgr, e));
            Ok(1)
        }
    }
}

fn run_arch_uninstall(out: &Output) -> Result<i32> {
    out.println("Running: sudo pacman -R --noconfirm duckier-cli");

    let status = std::process::Command::new("sudo")
        .args(["pacman", "-R", "--noconfirm", "duckier-cli"])
        .status();

    match status {
        Ok(s) if s.success() => {
            print_success(out);
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

fn run_mac_uninstall(out: &Output) -> Result<i32> {
    out.println("Uninstalling...");

    let script = format!(
        r#"set -e
DESKTOP_INSTALLED=false
if [ -d "/Applications/Duckier.app" ]; then
    DESKTOP_INSTALLED=true
fi
if [ "$DESKTOP_INSTALLED" = false ]; then
    if launchctl list com.duckier.vpn.daemon &>/dev/null; then
        launchctl bootout system/com.duckier.vpn.daemon 2>/dev/null || true
    fi
    killall duckiervpn-daemon 2>/dev/null || true
    rm -f /usr/local/bin/duckiervpn-daemon
    rm -f /Library/LaunchDaemons/com.duckier.vpn.daemon.plist
fi
rm -f /usr/local/bin/{cli}
"#,
        cli = brand::BINARY_NAME,
    );

    let status = std::process::Command::new("sudo")
        .args(["bash", "-c", &script])
        .status();

    match status {
        Ok(s) if s.success() => {
            print_success(out);
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("Uninstall failed (exit code {})", code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run uninstall: {}", e));
            Ok(1)
        }
    }
}

fn run_windows_uninstall(out: &Output) -> Result<i32> {
    out.println("To uninstall on Windows:");
    out.println("  1. Open Settings > Apps > Installed apps");
    out.println(&format!(
        "  2. Find \"{}\" and click Uninstall",
        brand::PRODUCT_NAME
    ));
    Ok(0)
}

fn run_linux_manual_uninstall(out: &Output) -> Result<i32> {
    out.println("Uninstalling...");

    let script = format!(
        r#"set -e
# Check for desktop app (any package manager)
DESKTOP_INSTALLED=false
if dpkg -l duckier 2>/dev/null | grep -q '^ii'; then
    DESKTOP_INSTALLED=true
elif rpm -q duckier &>/dev/null; then
    DESKTOP_INSTALLED=true
elif pacman -Q duckier &>/dev/null; then
    DESKTOP_INSTALLED=true
fi
if [ "$DESKTOP_INSTALLED" = false ]; then
    if which systemctl &>/dev/null; then
        systemctl stop duckiervpn-daemon.service 2>/dev/null || true
        systemctl disable duckiervpn-daemon.service 2>/dev/null || true
    fi
    killall duckiervpn-daemon 2>/dev/null || true
    rm -f /usr/local/bin/duckiervpn-daemon
    rm -f /etc/systemd/system/duckiervpn-daemon.service
    if which systemctl &>/dev/null; then
        systemctl daemon-reload 2>/dev/null || true
    fi
else
    echo "Desktop app is installed — leaving daemon running."
fi
rm -f /usr/local/bin/{cli}
"#,
        cli = brand::BINARY_NAME,
    );

    let status = std::process::Command::new("sudo")
        .args(["bash", "-c", &script])
        .status();

    match status {
        Ok(s) if s.success() => {
            print_success(out);
            Ok(0)
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            out.error(&format!("Uninstall failed (exit code {})", code));
            Ok(1)
        }
        Err(e) => {
            out.error(&format!("Failed to run uninstall: {}", e));
            Ok(1)
        }
    }
}

/// Clean up daemon on macOS if it exists and the desktop app is not installed.
fn cleanup_mac_daemon(out: &Output) {
    if !std::path::Path::new("/usr/local/bin/duckiervpn-daemon").exists() {
        return;
    }
    if std::path::Path::new("/Applications/Duckier.app").exists() {
        out.println("Desktop app is installed — leaving daemon running.");
        return;
    }

    let script = r#"
if launchctl list com.duckier.vpn.daemon &>/dev/null; then
    launchctl bootout system/com.duckier.vpn.daemon 2>/dev/null || true
fi
killall duckiervpn-daemon 2>/dev/null || true
rm -f /usr/local/bin/duckiervpn-daemon
rm -f /Library/LaunchDaemons/com.duckier.vpn.daemon.plist
"#;

    let _ = std::process::Command::new("sudo")
        .args(["bash", "-c", script])
        .status();
}

fn print_success(out: &Output) {
    out.println("");
    out.success(&format!("{} has been uninstalled.", brand::PRODUCT_NAME));
    out.println(&format!(
        "User config remains at ~/.config/{}/ (remove manually if desired)",
        brand::CONFIG_DIR
    ));
}
