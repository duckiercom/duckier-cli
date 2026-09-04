use anyhow::{Context, Result};
use tracing::debug;

use crate::api::client::{is_unauthorized, ApiClient};
use crate::api::config::{fetch_app_config, Server};
use crate::api::vpn::{get_wireguard_config, register_wireguard_keys, WgRemoteConfig};
use crate::brand;
use crate::crypto::generate_wireguard_keys;
use crate::grpc::client::GrpcClient;
use crate::grpc::vpn::{WireguardPeer, WireguardStartRequest};
use crate::output::Output;
use crate::storage::{load_wireguard, save_wireguard, WireGuardData};

/// Flatten servers and their nested locations into a single list, then pick one.
fn select_server(
    servers: &[Server],
    country: &Option<String>,
    city: &Option<String>,
) -> Option<Server> {
    let mut flat: Vec<&Server> = Vec::new();
    for s in servers {
        if s.locations.is_empty() {
            flat.push(s);
        } else {
            for loc in &s.locations {
                flat.push(loc);
            }
        }
    }

    if let Some(cc) = country {
        let cc_upper = cc.to_uppercase();
        flat.retain(|s| s.country_code.eq_ignore_ascii_case(&cc_upper));
    }

    if let Some(c) = city {
        let c_lower = c.to_lowercase();
        flat.retain(|s| s.city.to_lowercase().contains(&c_lower));
    }

    if flat.is_empty() {
        return None;
    }

    let idx = rand::Rng::gen_range(&mut rand::thread_rng(), 0..flat.len());
    Some(flat[idx].clone())
}

pub async fn run(
    daemon_addr: &str,
    country: Option<String>,
    city: Option<String>,
    out: &Output,
) -> Result<i32> {
    // 1. Ensure onboarded
    crate::api::auth::ensure_onboarded()?;

    // 2. Fetch app config
    let api = ApiClient::new();
    let app_config = fetch_app_config(&api, false).context("failed to fetch app configuration")?;

    // 3. Select server
    let server = match select_server(&app_config.servers, &country, &city) {
        Some(s) => s,
        None => {
            out.error("No servers match the given filters");
            return Ok(3);
        }
    };

    // 4. Check PRO requirement
    if server.pro {
        let auth = crate::storage::load_auth();
        let is_pro = auth
            .user
            .as_ref()
            .and_then(|u| u.get("subscription"))
            .and_then(|s| s.get("active"))
            .and_then(|a| a.as_bool())
            .unwrap_or(false);
        if !is_pro {
            out.error(&format!(
                "Server {}:{} requires a PRO subscription. Run `{} login` to upgrade.",
                server.country_code,
                server.city,
                brand::BINARY_NAME
            ));
            return Ok(1);
        }
    }

    // 5. Register keys and fetch the tunnel config. A stored token can be
    // dead (31-day backend TTL) while auth.json still looks fine, so on a 401
    // drop the credentials, re-onboard ephemeral accounts and try once more.
    let (wg_keys, wg_config) = match prepare_tunnel(&api, &server, out) {
        Ok(v) => v,
        Err(e) if is_unauthorized(&e) => {
            if !crate::api::auth::recover_from_unauthorized(&api)? {
                out.error(&format!(
                    "Your session has expired. Run `{} login` to link your account again.",
                    brand::BINARY_NAME
                ));
                return Ok(2);
            }
            prepare_tunnel(&api, &server, out)?
        }
        Err(e) => return Err(e),
    };

    // 7. Build gRPC request
    let request = WireguardStartRequest {
        cli_mode: Some(crate::grpc::vpn::CliModeConfig {
            enabled: true,
            ..Default::default()
        }),
        interface_name: String::new(),
        private_key: wg_keys.private_key,
        address: wg_config.interface.address,
        listen_port: 0,
        mtu: 0,
        dns: wg_config.interface.dns,
        peers: vec![WireguardPeer {
            public_key: wg_config.peer.public_key,
            preshared_key: wg_config.peer.preshared_key,
            endpoint: wg_config.peer.endpoint.clone(),
            allowed_ips: wg_config.peer.allowed_ips,
            persistent_keepalive: wg_config.peer.keep_alive,
        }],
        resource_path: String::new(),
    };

    // 8. Connect to daemon and start WireGuard
    let grpc = GrpcClient::connect(daemon_addr).await.with_context(|| {
        format!(
            "cannot reach VPN daemon at {} — is the daemon running?",
            daemon_addr
        )
    })?;
    let mut client = grpc.client();
    let grpc_request = grpc.make_request(request);

    match client.wireguard_start(grpc_request).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            while let Ok(Some(resp)) = stream.message().await {
                debug!("Stream: command={} error={}", resp.command, resp.error);
                if resp.error {
                    out.error(&resp.message);
                    return Ok(3);
                }
                if resp.command == "wireguard-connected" || resp.command == "connected" {
                    if out.is_json() {
                        out.print_json(&serde_json::json!({
                            "status": "connected",
                            "protocol": "WireGuard",
                            "country": server.country_code,
                            "city": server.city,
                        }));
                    } else {
                        out.success(&format!(
                            "Connected to {} — {}",
                            server.city, server.country_code
                        ));
                    }
                    return Ok(0);
                }
            }
            out.error("Connection stream ended without confirmation");
            Ok(3)
        }
        Err(e) => {
            out.error(&format!("gRPC error: {}", e));
            Ok(3)
        }
    }
}

/// Load or register WireGuard keys, then fetch the config for `server`.
fn prepare_tunnel(
    api: &ApiClient,
    server: &Server,
    out: &Output,
) -> Result<(WireGuardData, WgRemoteConfig)> {
    let wg_keys = match load_wireguard() {
        Some(keys) => {
            debug!("Using existing WireGuard keys");
            keys
        }
        None => {
            out.println("Generating WireGuard keys...");
            let kp = generate_wireguard_keys();
            register_wireguard_keys(api, &kp.public_key, &kp.preshared_key)
                .context("failed to register WireGuard keys")?;
            let data = WireGuardData {
                private_key: kp.private_key,
                public_key: kp.public_key,
                preshared_key: kp.preshared_key,
            };
            save_wireguard(&data).context("failed to persist WireGuard keys")?;
            data
        }
    };

    let wg_config =
        get_wireguard_config(api, &wg_keys.public_key, &server.country_code, &server.city)
            .context("failed to fetch WireGuard tunnel configuration")?;

    Ok((wg_keys, wg_config))
}
