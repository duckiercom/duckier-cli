use anyhow::{Context, Result};
use tracing::debug;

use crate::api::auth::{get_connection_code, login_by_session, refresh_connection_code};
use crate::api::client::ApiClient;
use crate::brand;
use crate::output::Output;
use crate::storage::has_auth;

fn ensure_onboarded() -> Result<()> {
    if has_auth() {
        return Ok(());
    }
    let api = ApiClient::new();
    crate::api::auth::onboard(&api).context("failed to create ephemeral account")?;
    Ok(())
}

pub async fn run(out: &Output) -> Result<i32> {
    ensure_onboarded()?;

    // Check if already logged in with a real account
    let auth = crate::storage::load_auth();
    if !auth.is_ephemeral && !auth.email.is_empty() {
        if out.is_json() {
            out.print_json(&serde_json::json!({
                "status": "already_logged_in",
                "email": auth.email,
            }));
        } else {
            out.success(&format!("Already logged in as {}", auth.email));
        }
        return Ok(0);
    }

    let api = ApiClient::new();

    // Get connection code — returns (code, session)
    let (code, session) = get_connection_code(&api).context("failed to request connection code")?;

    if out.is_json() {
        out.print_json(&serde_json::json!({
            "status": "waiting",
            "code": code,
            "url": brand::CONNECT_URL,
        }));
    } else {
        out.println(&format!("Connection code: {}", code));
        out.println("");
        out.println(&format!(
            "Enter this code at {} to link your account.",
            brand::CONNECT_URL
        ));
        out.println("Waiting for confirmation...");
    }

    // Poll until the user links their account (bounded wait with gentle backoff).
    let started_at = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(300);
    let mut poll_interval = std::time::Duration::from_secs(3);

    loop {
        if started_at.elapsed() > max_wait {
            out.error(&format!(
                "Login timed out after 5 minutes. Run `{} login` again.",
                brand::BINARY_NAME
            ));
            return Ok(1);
        }

        tokio::time::sleep(poll_interval).await;
        if poll_interval < std::time::Duration::from_secs(10) {
            poll_interval += std::time::Duration::from_secs(1);
        }

        match refresh_connection_code(&api, &session)
            .context("failed while polling for login confirmation")?
        {
            Some(session_id) => {
                debug!("Got session ID, completing login...");

                let auth = login_by_session(&api, &session_id)
                    .context("failed to complete login after confirmation")?;

                let email = &auth.email;
                let plan = auth
                    .user
                    .as_ref()
                    .and_then(|u| u.get("subscription"))
                    .and_then(|s| s.get("plan"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("free");

                if out.is_json() {
                    out.print_json(&serde_json::json!({
                        "status": "ok",
                        "email": email,
                        "plan": plan,
                    }));
                } else {
                    out.success(&format!("Logged in as {} ({})", email, plan));
                }
                return Ok(0);
            }
            None => {
                // Still waiting, continue polling
                debug!("Connection code not yet linked, polling...");
            }
        }
    }
}
