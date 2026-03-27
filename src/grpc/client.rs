use anyhow::{anyhow, Context, Result};
use std::time::Duration;
use tonic::transport::Channel;
use tracing::debug;

use super::vpn::vpn_client::VpnClient;
use crate::brand;

pub struct GrpcClient {
    channel: Channel,
    auth_token: String,
}

impl GrpcClient {
    pub async fn connect(addr: &str) -> Result<Self> {
        enforce_safe_daemon_addr(addr)?;

        match Self::try_connect(addr, Duration::from_secs(5)).await {
            Ok(client) => Ok(client),
            Err(first_err) => {
                #[cfg(target_os = "windows")]
                if is_daemon_no_service() {
                    debug!("Daemon not reachable and DaemonNoService=1, spawning elevated daemon");
                    if let Err(e) = spawn_daemon_elevated() {
                        debug!("Failed to spawn elevated daemon: {}", e);
                        return Err(first_err);
                    }
                    // Shorter timeout for retries — daemon should come up fast once spawned
                    for attempt in 1..=10 {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        debug!("Retry attempt {} connecting to daemon", attempt);
                        if let Ok(client) = Self::try_connect(addr, Duration::from_secs(2)).await {
                            return Ok(client);
                        }
                    }
                }

                Err(first_err)
            }
        }
    }

    async fn try_connect(addr: &str, connect_timeout: Duration) -> Result<Self> {
        let auth_token = read_auth_token();

        let channel = Channel::from_shared(addr.to_string())
            .with_context(|| format!("invalid daemon address {}", addr))?
            .connect_timeout(connect_timeout)
            .timeout(Duration::from_secs(30))
            .connect()
            .await
            .with_context(|| format!("failed to connect to gRPC daemon at {}", addr))?;

        debug!("gRPC connected to {}", addr);
        Ok(Self {
            channel,
            auth_token,
        })
    }

    pub fn client(&self) -> VpnClient<Channel> {
        VpnClient::new(self.channel.clone())
    }

    pub fn make_request<T>(&self, inner: T) -> tonic::Request<T> {
        make_request(inner, &self.auth_token)
    }
}

fn enforce_safe_daemon_addr(addr: &str) -> Result<()> {
    if is_safe_local_addr(addr) {
        return Ok(());
    }

    // Explicit escape hatch for advanced/debug scenarios.
    if std::env::var("DUCKIER_ALLOW_REMOTE_DAEMON").ok().as_deref() == Some("1") {
        return Ok(());
    }

    Err(anyhow!(
        "Refusing non-local daemon address '{}'. Use a loopback address or set DUCKIER_ALLOW_REMOTE_DAEMON=1 to override.",
        addr
    ))
}

fn is_safe_local_addr(addr: &str) -> bool {
    addr.starts_with("http://127.0.0.1:")
        || addr.starts_with("http://localhost:")
        || addr.starts_with("http://[::1]:")
}

fn make_request<T>(inner: T, auth_token: &str) -> tonic::Request<T> {
    let mut request = tonic::Request::new(inner);
    if !auth_token.is_empty() {
        if let Ok(value) = auth_token.parse() {
            request.metadata_mut().insert("authorization", value);
        }
    }
    request
}

fn read_auth_token() -> String {
    let path = auth_token_path();
    match std::fs::read_to_string(&path) {
        Ok(token) => {
            let trimmed = token.trim().to_string();
            if !trimmed.is_empty() {
                debug!("Auth token loaded from {}", path.display());
            }
            trimmed
        }
        Err(_) => {
            debug!(
                "No auth token at {} (daemon may not be running)",
                path.display()
            );
            String::new()
        }
    }
}

fn auth_token_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        let program_data =
            std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        std::path::PathBuf::from(program_data)
            .join(brand::PRODUCT_NAME)
            .join("run")
            .join("auth_token")
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::path::PathBuf::from(format!("/var/run/{}/auth_token", brand::DAEMON_NAME))
    }
}

/// Check if the Windows registry key `HKLM\SOFTWARE\{PRODUCT_NAME}\DaemonNoService` is set to 1.
#[cfg(target_os = "windows")]
fn is_daemon_no_service() -> bool {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let subkey = format!(r"SOFTWARE\{}", brand::PRODUCT_NAME);
    let Ok(key) = hklm.open_subkey(&subkey) else {
        return false;
    };
    let Ok(value): Result<u32, _> = key.get_value("DaemonNoService") else {
        return false;
    };
    value == 1
}

/// Spawn the daemon process with elevated (UAC) privileges using ShellExecuteW("runas").
#[cfg(target_os = "windows")]
fn spawn_daemon_elevated() -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let daemon_path = find_daemon_exe()?;
    debug!("Spawning elevated daemon: {}", daemon_path.display());

    let operation: Vec<u16> = std::ffi::OsStr::new("runas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let file: Vec<u16> = daemon_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let params: Vec<u16> = vec![0u16];

    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
        )
    };

    // ShellExecuteW returns > 32 on success
    if (result as usize) <= 32 {
        return Err(anyhow!(
            "ShellExecuteW(runas) failed with code {}",
            result as usize
        ));
    }

    Ok(())
}

/// Locate the daemon executable. Check next to the current exe first,
/// then fall back to the standard Program Files install path.
#[cfg(target_os = "windows")]
fn find_daemon_exe() -> Result<std::path::PathBuf> {
    let daemon_name = format!("{}vpn-daemon.exe", brand::DAEMON_NAME);

    // Check next to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(&daemon_name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // Fall back to standard install location
    let program_files =
        std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    let candidate = std::path::PathBuf::from(program_files)
        .join(brand::PRODUCT_NAME)
        .join(&daemon_name);
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(anyhow!("Daemon executable '{}' not found", daemon_name))
}
