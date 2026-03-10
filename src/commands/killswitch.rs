use anyhow::{Context, Result};
use tracing::debug;

use crate::brand;
use crate::grpc::client::GrpcClient;
use crate::grpc::vpn::KillSwitchEnableRequest;
use crate::output::Output;

pub async fn enable(daemon_addr: &str, allow_lan: bool, out: &Output) -> Result<i32> {
    let grpc = GrpcClient::connect(daemon_addr).await.with_context(|| {
        format!(
            "cannot reach VPN daemon at {} — is the daemon running?",
            daemon_addr
        )
    })?;
    let mut client = grpc.client();

    let request = grpc.make_request(KillSwitchEnableRequest {
        allow_lan,
        extra_api_domains: brand::ks_api_domains(),
    });

    match client.kill_switch_enable(request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(resp)) = stream.message().await {
                debug!(
                    "KS enable: command={} success={}",
                    resp.command, resp.success
                );
                if resp.error {
                    out.error(&resp.message);
                    return Ok(1);
                }
                if resp.success {
                    let lan_note = if allow_lan { " (LAN allowed)" } else { "" };
                    out.success(&format!("Kill switch enabled{}", lan_note));
                    return Ok(0);
                }
            }
            out.success("Kill switch enabled");
            Ok(0)
        }
        Err(e) => {
            out.error(&format!("gRPC error: {}", e));
            Ok(1)
        }
    }
}

pub async fn disable(daemon_addr: &str, out: &Output) -> Result<i32> {
    let grpc = GrpcClient::connect(daemon_addr).await.with_context(|| {
        format!(
            "cannot reach VPN daemon at {} — is the daemon running?",
            daemon_addr
        )
    })?;
    let mut client = grpc.client();
    let request = grpc.make_request(());

    match client.kill_switch_disable(request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(resp)) = stream.message().await {
                debug!(
                    "KS disable: command={} success={}",
                    resp.command, resp.success
                );
                if resp.error {
                    out.error(&resp.message);
                    return Ok(1);
                }
                if resp.success {
                    out.success("Kill switch disabled");
                    return Ok(0);
                }
            }
            out.success("Kill switch disabled");
            Ok(0)
        }
        Err(e) => {
            out.error(&format!("gRPC error: {}", e));
            Ok(1)
        }
    }
}

pub async fn status(daemon_addr: &str, out: &Output) -> Result<i32> {
    let grpc = GrpcClient::connect(daemon_addr).await.with_context(|| {
        format!(
            "cannot reach VPN daemon at {} — is the daemon running?",
            daemon_addr
        )
    })?;
    let mut client = grpc.client();
    let request = grpc.make_request(());

    match client.kill_switch_status(request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(resp)) = stream.message().await {
                debug!("KS status: payload={}", resp.payload);
                let enabled = match serde_json::from_str::<serde_json::Value>(&resp.payload) {
                    Ok(parsed) => parsed.get("enabled") == Some(&serde_json::Value::Bool(true)),
                    Err(e) => {
                        debug!("Failed to parse kill switch status payload: {}", e);
                        false
                    }
                };

                if out.is_json() {
                    out.print_json(&serde_json::json!({
                        "killswitch": {
                            "enabled": enabled,
                            "payload": resp.payload,
                        }
                    }));
                } else {
                    out.info("Kill Switch", if enabled { "enabled" } else { "disabled" });
                }
                return Ok(0);
            }
            // No response — assume disabled
            if out.is_json() {
                out.print_json(&serde_json::json!({
                    "killswitch": { "enabled": false }
                }));
            } else {
                out.info("Kill Switch", "disabled");
            }
            Ok(0)
        }
        Err(e) => {
            out.error(&format!("gRPC error: {}", e));
            Ok(1)
        }
    }
}
