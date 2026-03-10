use anyhow::Result;
use tracing::debug;

use crate::grpc::client::GrpcClient;
use crate::output::Output;

pub async fn run(daemon_addr: &str, out: &Output) -> Result<i32> {
    let grpc = match GrpcClient::connect(daemon_addr).await {
        Ok(g) => g,
        Err(_) => {
            if out.is_json() {
                out.print_json(&serde_json::json!({
                    "daemon": { "running": false },
                    "vpn": { "status": "unknown", "protocol": "WireGuard" },
                    "killswitch": { "enabled": false }
                }));
            } else {
                out.info("Daemon", "not running");
                out.info("VPN", "unknown");
                out.info("Kill Switch", "unknown");
            }
            return Ok(1);
        }
    };

    let mut client = grpc.client();

    // Get daemon PID
    let pid_request = grpc.make_request(());
    let pid_val = match client.system_pid(pid_request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            let mut pid = String::from("unknown");
            while let Ok(Some(resp)) = stream.message().await {
                debug!("PID response: payload={}", resp.payload);
                if resp.success {
                    pid = resp.payload;
                    break;
                }
            }
            pid
        }
        Err(_) => "unknown".to_string(),
    };

    // Get WireGuard status
    let status_request = grpc.make_request(());
    let (vpn_status, vpn_connected) = match client.wireguard_status(status_request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            let mut status = String::from("Disconnected");
            let mut connected = false;
            while let Ok(Some(resp)) = stream.message().await {
                debug!(
                    "WG status: command={} payload={}",
                    resp.command, resp.payload
                );
                if resp.command == "wireguard-connected" {
                    status = "Connected".to_string();
                    connected = true;
                } else if resp.success {
                    match serde_json::from_str::<serde_json::Value>(&resp.payload) {
                        Ok(parsed) => {
                            if parsed.get("running") == Some(&serde_json::Value::Bool(true)) {
                                status = "Connected".to_string();
                                connected = true;
                            }
                        }
                        Err(e) => {
                            debug!("Failed to parse WG status payload: {}", e);
                        }
                    }
                }
                break;
            }
            (status, connected)
        }
        Err(_) => ("Disconnected".to_string(), false),
    };

    // Get kill switch status
    let ks_request = grpc.make_request(());
    let ks_enabled = match client.kill_switch_status(ks_request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            let mut enabled = false;
            while let Ok(Some(resp)) = stream.message().await {
                debug!("KS status: payload={}", resp.payload);
                match serde_json::from_str::<serde_json::Value>(&resp.payload) {
                    Ok(parsed) => {
                        enabled = parsed.get("enabled") == Some(&serde_json::Value::Bool(true));
                    }
                    Err(e) => {
                        debug!("Failed to parse kill switch status payload: {}", e);
                    }
                }
                break;
            }
            enabled
        }
        Err(_) => false,
    };

    if out.is_json() {
        out.print_json(&serde_json::json!({
            "daemon": {
                "running": true,
                "pid": pid_val,
            },
            "vpn": {
                "status": if vpn_connected { "connected" } else { "disconnected" },
                "protocol": "WireGuard",
            },
            "killswitch": {
                "enabled": ks_enabled,
            }
        }));
    } else {
        out.info("Daemon", &format!("running (pid {})", pid_val));
        out.info("Protocol", "WireGuard");
        out.info("Status", &vpn_status);
        out.info(
            "Kill Switch",
            if ks_enabled { "enabled" } else { "disabled" },
        );
    }

    Ok(0)
}
