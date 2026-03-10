//! Compile-time branding constants from config.toml.
//! Change config.toml to white-label for a different VPN product.

pub const PRODUCT_NAME: &str = env!("BRAND_PRODUCT_NAME");
pub const BINARY_NAME: &str = env!("BRAND_BINARY_NAME");
pub const DAEMON_NAME: &str = env!("BRAND_DAEMON_NAME");

pub const API_URL: &str = env!("BRAND_API_URL");
#[allow(dead_code)]
pub const FRONTEND_URL: &str = env!("BRAND_FRONTEND_URL");
pub const APP_ID: &str = env!("BRAND_APP_ID");
pub const EPHEMERAL_TLD: &str = env!("BRAND_EPHEMERAL_TLD");
pub const CONNECT_URL: &str = env!("BRAND_CONNECT_URL");
pub const UPDATE_URL: &str = env!("BRAND_UPDATE_URL");

pub const GRPC_ADDRESS: &str = env!("BRAND_GRPC_ADDRESS");

pub const KS_API_DOMAINS_STR: &str = env!("BRAND_KS_API_DOMAINS");

pub const CONFIG_DIR: &str = env!("BRAND_CONFIG_DIR");

/// Parse the kill switch API domains (comma-separated at compile time).
pub fn ks_api_domains() -> Vec<String> {
    KS_API_DOMAINS_STR
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}
