use anyhow::{Context, Result};
use tracing::debug;

use crate::grpc::client::GrpcClient;
use crate::output::Output;

pub async fn health(daemon_addr: &str, out: &Output) -> Result<i32> {
    let grpc = match GrpcClient::connect(daemon_addr).await {
        Ok(client) => client,
        Err(err) => {
            if out.is_json() {
                out.print_json(&serde_json::json!({
                    "daemon": { "healthy": false, "error": err.to_string() }
                }));
            } else {
                out.error(&format!("Daemon not reachable at {}: {}", daemon_addr, err));
            }
            return Ok(1);
        }
    };

    let mut client = grpc.client();
    let request = grpc.make_request(());

    match client.system_pong(request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(resp)) = stream.message().await {
                debug!("Pong: success={} payload={}", resp.success, resp.payload);
                if resp.success {
                    if out.is_json() {
                        out.print_json(&serde_json::json!({
                            "daemon": { "healthy": true, "address": daemon_addr }
                        }));
                    } else {
                        out.success("Daemon is healthy");
                    }
                    return Ok(0);
                }
            }
            if out.is_json() {
                out.print_json(&serde_json::json!({
                    "daemon": { "healthy": true, "address": daemon_addr }
                }));
            } else {
                out.success("Daemon is healthy");
            }
            Ok(0)
        }
        Err(err) => {
            if out.is_json() {
                out.print_json(&serde_json::json!({
                    "daemon": { "healthy": false, "error": err.to_string() }
                }));
            } else {
                out.error(&format!("Health check failed: {}", err));
            }
            Ok(1)
        }
    }
}

pub async fn pid(daemon_addr: &str, out: &Output) -> Result<i32> {
    let grpc = GrpcClient::connect(daemon_addr).await.with_context(|| {
        format!(
            "cannot reach VPN daemon at {} — is the daemon running?",
            daemon_addr
        )
    })?;

    let mut client = grpc.client();
    let request = grpc.make_request(());

    match client.system_pid(request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(resp)) = stream.message().await {
                debug!("PID: success={} payload={}", resp.success, resp.payload);
                if resp.success {
                    if out.is_json() {
                        out.print_json(&serde_json::json!({
                            "daemon": { "pid": resp.payload }
                        }));
                    } else {
                        out.println(&resp.payload);
                    }
                    return Ok(0);
                }
            }
            out.error("No PID response from daemon");
            Ok(1)
        }
        Err(err) => {
            out.error(&format!("gRPC error: {}", err));
            Ok(1)
        }
    }
}
