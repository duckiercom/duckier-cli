use anyhow::{anyhow, bail, Context, Result};
use serde_json::json;
use tracing::debug;

use super::ApiClient;
use crate::storage::{self, AuthData};

/// Make sure some account exists locally, creating an ephemeral one if not.
/// Only checks for a stored token; whether the backend still accepts it is
/// discovered on first use, see `recover_from_unauthorized`.
pub fn ensure_onboarded() -> Result<()> {
    if storage::has_auth() {
        return Ok(());
    }
    debug!("No auth found, auto-onboarding...");
    crate::api::auth::onboard(&ApiClient::new()).context("failed to create ephemeral account")?;
    Ok(())
}

/// The backend rejected our token (expired, revoked, or the account is gone).
/// Drops the stale credentials and the WireGuard keys registered under them,
/// then re-onboards ephemeral accounts so the caller can retry. Returns false
/// for linked accounts: those need the user to run `login` again.
pub fn recover_from_unauthorized(client: &ApiClient) -> Result<bool> {
    let auth = storage::load_auth();
    let was_ephemeral = auth.is_ephemeral
        || auth.email.is_empty()
        || auth.email.contains(crate::brand::EPHEMERAL_TLD);
    debug!("Auth token rejected by backend, clearing local credentials");
    storage::clear_credentials().context("failed to clear stale credentials")?;
    if !was_ephemeral {
        return Ok(false);
    }
    onboard(client).context("failed to re-create ephemeral account")?;
    Ok(true)
}

/// Create an ephemeral (anonymous) account via /api/onboarding.
/// Returns the populated AuthData on success.
pub fn onboard(client: &ApiClient) -> Result<AuthData> {
    let body = json!({
        "deviceId": client.device_id(),
        "deviceOs": client.os_string(),
        "deviceTime": chrono_millis(),
        "version": client.version(),
    });

    let resp = client
        .post("/api/onboarding", &body)
        .context("failed to onboard device")?;

    if resp.get("onboardingCompleted").and_then(|v| v.as_bool()) != Some(true) {
        bail!("onboarding was not completed by the server");
    }

    let auth_token = resp["auth_token"]
        .as_str()
        .ok_or_else(|| anyhow!("missing auth_token in onboarding response"))?
        .to_string();

    let user = resp.get("user").cloned();
    let email = user
        .as_ref()
        .and_then(|u| u["email"].as_str())
        .unwrap_or("")
        .to_string();

    let auth = AuthData {
        auth_token,
        email,
        device_id: client.raw_device_id(),
        device_name: client.device_name(),
        is_ephemeral: true,
        user,
    };

    storage::save_auth(&auth).context("failed to persist onboarded auth state")?;
    debug!("Onboarding complete, ephemeral account created");
    Ok(auth)
}

/// Request a connection code for device linking.
/// Returns (code, session) that the user shows on another device.
pub fn get_connection_code(client: &ApiClient) -> Result<(String, String)> {
    let body = json!({
        "deviceId": client.device_id(),
        "deviceName": client.device_name(),
        "deviceOs": client.os_string(),
    });

    let resp = client
        .post("/api/device/connectcode", &body)
        .context("failed to request connection code")?;

    let code = match &resp["code"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return Err(anyhow!("missing code in connectcode response")),
    };
    let session = resp["session"]
        .as_str()
        .ok_or_else(|| anyhow!("missing session in connectcode response"))?
        .to_string();

    debug!("Got connection code from backend");
    Ok((code, session))
}

/// Poll for whether the user has linked their account on the web.
/// Returns Some(session_id) when the account is linked, None if still pending.
pub fn refresh_connection_code(client: &ApiClient, session: &str) -> Result<Option<String>> {
    let body = json!({
        "session": session,
        "deviceId": client.device_id(),
        "deviceName": client.device_name(),
        "deviceOs": client.os_string(),
    });

    let resp = client
        .post("/api/device/connectcode/refresh", &body)
        .with_context(|| format!("failed to refresh connection code session {}", session))?;

    // When the user has linked, the response contains a sessionId
    if let Some(session_id) = resp.get("sessionId").and_then(|v| v.as_str()) {
        debug!("Connection code linked, sessionId received");
        return Ok(Some(session_id.to_string()));
    }

    // Still pending
    Ok(None)
}

/// After the connection code is linked, log in using the session ID.
/// Saves auth data to storage and returns it.
pub fn login_by_session(client: &ApiClient, session_id: &str) -> Result<AuthData> {
    let body = json!({
        "sessionId": session_id,
        "deviceId": client.device_id(),
        "deviceOs": client.os_string(),
    });

    let resp = client
        .post("/api/device/login", &body)
        .with_context(|| format!("failed to log in with session {}", session_id))?;

    if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
        bail!("login failed: {}", err);
    }

    let auth_token = resp["auth_token"]
        .as_str()
        .ok_or_else(|| anyhow!("missing auth_token in login response"))?
        .to_string();

    let user = resp.get("user").cloned();
    let email = user
        .as_ref()
        .and_then(|u| u["email"].as_str())
        .unwrap_or("")
        .to_string();

    let auth = AuthData {
        auth_token,
        email,
        device_id: client.raw_device_id(),
        device_name: client.device_name(),
        is_ephemeral: false,
        user,
    };

    storage::save_auth(&auth).context("failed to persist linked account auth state")?;
    debug!("Logged in via session, account linked");
    Ok(auth)
}

/// Log out the current device from the backend and clear local credentials.
pub fn logout(client: &ApiClient) -> Result<()> {
    // Notify the backend so the session is invalidated server-side
    let resp = client.post("/api/device/logout", &serde_json::json!({}));
    debug!("Remote logout response: {:?}", resp);

    // Clear local state regardless of whether the remote call succeeded
    // (the user wants to log out locally even if the server is unreachable)
    storage::clear_credentials().context("failed to clear local auth state")?;

    // Remove cached app config
    let cache_path = storage::config_dir().join("cache/appconfig.json");
    if cache_path.exists() {
        std::fs::remove_file(&cache_path)
            .with_context(|| format!("failed to remove {}", cache_path.display()))?;
    }

    debug!("Local credentials cleared");
    Ok(())
}

/// Current time in milliseconds (matches JS `+new Date()`).
fn chrono_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
