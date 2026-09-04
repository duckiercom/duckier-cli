use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::debug;

use crate::brand;
use crate::crypto::device_id;
use crate::http_client::HttpClient;
use crate::storage::load_auth;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn detect_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "LINUX"
    } else if cfg!(target_os = "macos") {
        "DARWIN"
    } else if cfg!(target_os = "windows") {
        "WINDOWS"
    } else {
        "UNKNOWN"
    }
}

fn get_hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

/// Non-2xx response from the backend. Kept as a typed error so callers can
/// react to the status (401 means the stored token is dead) instead of
/// string-matching the message.
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "API error ({}): {}", self.status, self.message)
    }
}

impl std::error::Error for ApiError {}

/// True when any error in the chain is a 401 from the backend.
pub fn is_unauthorized(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|e| matches!(e.downcast_ref::<ApiError>(), Some(api) if api.status == 401))
}

pub struct ApiClient {
    client: HttpClient,
    base_url: String,
}

impl ApiClient {
    pub fn new() -> Self {
        Self {
            client: HttpClient::new(),
            base_url: brand::API_URL.to_string(),
        }
    }

    fn build_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("x-app-id".to_string(), brand::APP_ID.to_string());

        let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        headers.insert("x-request-id".to_string(), req_id.to_string());

        let host = get_hostname();
        headers.insert("x-device".to_string(), host);

        headers.insert("x-os".to_string(), detect_os().to_string());

        let raw_id = device_id();
        let auth = load_auth();
        let is_ephemeral =
            auth.is_ephemeral || auth.email.is_empty() || auth.email.contains(brand::EPHEMERAL_TLD);

        let effective_id = if is_ephemeral && !raw_id.starts_with("e_") {
            format!("e_{}", raw_id)
        } else if !is_ephemeral && raw_id.contains("e_") {
            raw_id.replace("e_", "")
        } else {
            raw_id
        };
        headers.insert("x-id".to_string(), effective_id);

        let username = get_username();
        headers.insert("x-name".to_string(), username);

        if !auth.auth_token.is_empty() {
            headers.insert("x-auth-token".to_string(), auth.auth_token);
        }

        headers
    }

    pub fn device_id(&self) -> String {
        let raw_id = device_id();
        let auth = load_auth();
        let is_ephemeral =
            auth.is_ephemeral || auth.email.is_empty() || auth.email.contains(brand::EPHEMERAL_TLD);

        if is_ephemeral && !raw_id.starts_with("e_") {
            format!("e_{}", raw_id)
        } else if !is_ephemeral && raw_id.contains("e_") {
            raw_id.replace("e_", "")
        } else {
            raw_id
        }
    }

    pub fn raw_device_id(&self) -> String {
        device_id()
    }

    pub fn device_name(&self) -> String {
        get_hostname()
    }

    pub fn os_string(&self) -> &'static str {
        detect_os()
    }

    pub fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        debug!("POST {}", url);

        let resp = self
            .client
            .post_json(&url, &self.build_headers(), body)
            .with_context(|| format!("failed to send POST {}", url))?;

        let status = resp.status_code();
        let text = resp
            .text()
            .with_context(|| format!("failed to read response body from POST {}", url))?;

        if !resp.is_success() {
            return Err(Self::api_error(status, &text));
        }

        let json: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse JSON response from POST {}", url))?;
        Ok(json)
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET {}", url);

        let resp = self
            .client
            .get(&url, &self.build_headers())
            .with_context(|| format!("failed to send GET {}", url))?;

        let status = resp.status_code();
        let text = resp
            .text()
            .with_context(|| format!("failed to read response body from GET {}", url))?;

        if !resp.is_success() {
            return Err(Self::api_error(status, &text));
        }

        let json: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse JSON response from GET {}", url))?;
        Ok(json)
    }

    fn api_error(status: u16, text: &str) -> anyhow::Error {
        let message = serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|body| body["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| text.to_string());
        ApiError { status, message }.into()
    }
}
