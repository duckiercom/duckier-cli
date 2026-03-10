use anyhow::{Context, Result};
use tracing::debug;

use crate::grpc::client::GrpcClient;
use crate::output::Output;

pub async fn run(daemon_addr: &str, out: &Output) -> Result<i32> {
    let grpc = GrpcClient::connect(daemon_addr).await.with_context(|| {
        format!(
            "cannot reach VPN daemon at {} — is the daemon running?",
            daemon_addr
        )
    })?;
    let mut client = grpc.client();
    let request = grpc.make_request(());

    match client.wireguard_stop(request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(resp)) = stream.message().await {
                debug!("Stream: command={} success={}", resp.command, resp.success);
                if resp.error {
                    out.error(&resp.message);
                    return Ok(1);
                }
                if resp.success {
                    out.success("Disconnected");
                    return Ok(0);
                }
            }
            // Stream ended — treat as success (already disconnected)
            out.success("Disconnected");
            Ok(0)
        }
        Err(e) => {
            out.error(&format!("gRPC error: {}", e));
            Ok(1)
        }
    }
}
