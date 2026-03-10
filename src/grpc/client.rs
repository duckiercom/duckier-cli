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
        let auth_token = read_auth_token();

        let channel = Channel::from_shared(addr.to_string())
            .with_context(|| format!("invalid daemon address {}", addr))?
            .connect_timeout(Duration::from_secs(5))
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
