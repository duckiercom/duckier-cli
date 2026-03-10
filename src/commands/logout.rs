use crate::api::auth::logout;
use crate::api::client::ApiClient;
use crate::output::Output;
use crate::storage;
use anyhow::{Context, Result};

pub async fn run(out: &Output) -> Result<i32> {
    let auth = storage::load_auth();

    if auth.auth_token.is_empty() {
        if out.is_json() {
            out.print_json(&serde_json::json!({ "status": "not_logged_in" }));
        } else {
            out.error("Not logged in");
        }
        return Ok(1);
    }

    let email = auth.email.clone();
    let api = ApiClient::new();
    logout(&api).await.context("failed to log out")?;

    if out.is_json() {
        out.print_json(&serde_json::json!({
            "status": "ok",
            "email": email,
        }));
    } else {
        out.success("Logged out");
    }

    Ok(0)
}
