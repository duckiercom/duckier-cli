use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use super::ApiClient;
use crate::storage;

/// A VPN server location entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    #[serde(rename = "country-iso-code", default)]
    pub country_code: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub pro: bool,
    #[serde(default)]
    pub locations: Vec<Server>,
}

/// Feature flags from the backend app_config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeatureFlags {
    #[serde(rename = "isLoginRequired", default)]
    pub is_login_required: bool,
    #[serde(rename = "isFreeModeEnabled", default = "default_true")]
    pub is_free_mode_enabled: bool,
}

fn default_true() -> bool {
    true
}

/// The full app config response.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AppConfig {
    pub servers: Vec<Server>,
    pub features: FeatureFlags,
    pub user: Option<Value>,
}

/// Fetch the app config (server list, feature flags, user info).
/// Uses a 1-hour cache stored via storage::save_cached_appconfig.
pub async fn fetch_app_config(client: &ApiClient, force: bool) -> Result<AppConfig> {
    if !force {
        if let Some(cached) = storage::load_cached_appconfig() {
            if let Ok(config) = parse_app_config(&cached) {
                debug!("Using cached app config");
                return Ok(config);
            }
        }
    }

    let resp = client
        .get("/api/appconfig.json?lang=en")
        .await
        .context("failed to fetch app configuration")?;

    if resp.get("data").is_none() {
        return Err(anyhow!("missing data in appconfig response"));
    }

    storage::save_cached_appconfig(&resp).context("failed to cache app configuration")?;
    debug!("Fetched and cached app config");

    parse_app_config(&resp)
}

fn parse_app_config(resp: &Value) -> Result<AppConfig> {
    let data = resp
        .get("data")
        .ok_or_else(|| anyhow!("missing data in appconfig response"))?;

    let servers: Vec<Server> = data
        .get("server")
        .and_then(|s| serde_json::from_value(s.clone()).ok())
        .unwrap_or_default();

    let features: FeatureFlags = data
        .get("app_config")
        .and_then(|c| serde_json::from_value(c.clone()).ok())
        .unwrap_or_default();

    let user = data.get("user").cloned();

    Ok(AppConfig {
        servers,
        features,
        user,
    })
}
