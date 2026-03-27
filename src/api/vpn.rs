use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;

use super::ApiClient;

/// Result of registering WireGuard keys with the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgRegistration {
    pub ip: String,
    pub name: String,
}

/// Remote WireGuard configuration returned by the backend (desktop/web format).
#[derive(Debug, Clone)]
pub struct WgRemoteConfig {
    pub interface: WgInterface,
    pub peer: WgPeer,
}

#[derive(Debug, Clone)]
pub struct WgInterface {
    pub address: String,
    pub dns: String,
}

#[derive(Debug, Clone)]
pub struct WgPeer {
    pub public_key: String,
    pub preshared_key: String,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub keep_alive: u32,
}

/// Register a WireGuard key pair with the backend.
/// POST /api/wireguard/create
pub fn register_wireguard_keys(
    client: &ApiClient,
    pubk: &str,
    pshk: &str,
) -> Result<WgRegistration> {
    let body = json!({
        "deviceId": client.device_id(),
        "deviceOs": client.os_string(),
        "pubk": pubk,
        "pshk": pshk,
    });

    let resp = client
        .post("/api/wireguard/create", &body)
        .context("failed to register WireGuard keys")?;

    let config = resp
        .get("config")
        .ok_or_else(|| anyhow!("missing config in wireguard/create response"))?;

    let ip = config["ip"]
        .as_str()
        .ok_or_else(|| anyhow!("missing ip in wireguard config"))?
        .to_string();
    let name = config["name"]
        .as_str()
        .ok_or_else(|| anyhow!("missing name in wireguard config"))?
        .to_string();

    debug!("WireGuard keys registered: ip={}, name={}", ip, name);
    Ok(WgRegistration { ip, name })
}

/// Fetch a WireGuard tunnel configuration for a given server.
/// POST /api/wireguard/get
/// Sends raw=1 to get structured JSON (desktop format).
pub fn get_wireguard_config(
    client: &ApiClient,
    pubk: &str,
    country: &str,
    city: &str,
) -> Result<WgRemoteConfig> {
    let mut body = json!({
        "deviceId": client.device_id(),
        "deviceOs": client.os_string(),
        "pubk": pubk,
        "raw": "1",
    });

    if !country.is_empty() {
        body["country"] = serde_json::Value::String(country.to_string());
    }
    if !city.is_empty() {
        body["city"] = serde_json::Value::String(city.to_string());
    }

    let resp = client
        .post("/api/wireguard/get", &body)
        .context("failed to fetch WireGuard tunnel configuration")?;

    // Desktop format: { interface: { address, dns }, peer: { ... } }
    let iface = resp
        .get("interface")
        .ok_or_else(|| anyhow!("server did not return interface config"))?;
    let peer = resp
        .get("peer")
        .ok_or_else(|| anyhow!("server did not return peer config"))?;

    let address = iface["address"].as_str().unwrap_or_default().to_string();
    let dns = iface["dns"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            // DNS can be an array
            iface["dns"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|entry| entry.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
        })
        .unwrap_or_default();

    // allowed_ips can be a string "0.0.0.0/0, ::/0" or an array
    let allowed_ips = if let Some(arr) = peer["allowed_ips"].as_array() {
        arr.iter()
            .filter_map(|entry| entry.as_str())
            .map(|ip| ip.to_string())
            .collect()
    } else if let Some(s) = peer["allowed_ips"].as_str() {
        s.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        vec!["0.0.0.0/0".to_string(), "::/0".to_string()]
    };

    let config = WgRemoteConfig {
        interface: WgInterface { address, dns },
        peer: WgPeer {
            public_key: peer["public_key"].as_str().unwrap_or_default().to_string(),
            preshared_key: peer["preshared_key"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            endpoint: peer["endpoint"].as_str().unwrap_or_default().to_string(),
            allowed_ips,
            keep_alive: peer["keep_alive"].as_u64().unwrap_or(25) as u32,
        },
    };

    debug!("Got WireGuard config: endpoint={}", config.peer.endpoint);
    Ok(config)
}
