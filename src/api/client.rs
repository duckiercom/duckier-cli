use anyhow::{anyhow, Context, Result};
use hyper::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde_json::Value;
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

    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-app-id", HeaderValue::from_static(brand::APP_ID));

        let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        if let Ok(header_value) = HeaderValue::from_str(&req_id.to_string()) {
            headers.insert("x-request-id", header_value);
        }

        let host = get_hostname();
        if let Ok(header_value) = HeaderValue::from_str(&host) {
            headers.insert("x-device", header_value);
        }

        headers.insert("x-os", HeaderValue::from_static(detect_os()));

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
        if let Ok(header_value) = HeaderValue::from_str(&effective_id) {
            headers.insert("x-id", header_value);
        }

        let username = get_username();
        if let Ok(header_value) = HeaderValue::from_str(&username) {
            headers.insert("x-name", header_value);
        }

        if !auth.auth_token.is_empty() {
            if let Ok(header_value) = HeaderValue::from_str(&auth.auth_token) {
                headers.insert("x-auth-token", header_value);
            }
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

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        debug!("POST {}", url);

        let resp = self
            .client
            .post_json(&url, self.build_headers(), body)
            .await
            .with_context(|| format!("failed to send POST {}", url))?;

        let status = resp.status();
        let text = resp
            .text()
            .with_context(|| format!("failed to read response body from POST {}", url))?;

        if !status.is_success() {
            if let Ok(error_body) = serde_json::from_str::<Value>(&text) {
                if error_body.get("error").is_some() {
                    return Err(anyhow!(
                        "API error ({}): {}",
                        status,
                        error_body["error"].as_str().unwrap_or("unknown")
                    ));
                }
            }
            return Err(anyhow!("HTTP {} — {}", status, text));
        }

        let json: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse JSON response from POST {}", url))?;
        Ok(json)
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET {}", url);

        let resp = self
            .client
            .get(&url, self.build_headers())
            .await
            .with_context(|| format!("failed to send GET {}", url))?;

        let status = resp.status();
        let text = resp
            .text()
            .with_context(|| format!("failed to read response body from GET {}", url))?;

        if !status.is_success() {
            return Err(anyhow!("HTTP {} — {}", status, text));
        }

        let json: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse JSON response from GET {}", url))?;
        Ok(json)
    }
}
