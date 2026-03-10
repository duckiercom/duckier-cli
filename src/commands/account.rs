use crate::brand;
use crate::output::Output;
use crate::storage::load_auth;
use anyhow::Result;

pub async fn run(out: &Output) -> Result<i32> {
    let auth = load_auth();

    if auth.auth_token.is_empty() {
        if out.is_json() {
            out.print_json(&serde_json::json!({
                "status": "unauthenticated",
                "message": "Not logged in"
            }));
        } else {
            out.error(&format!(
                "Not logged in. Run `{} login` to link your account.",
                brand::BINARY_NAME
            ));
        }
        return Ok(2);
    }

    let email = &auth.email;
    let ephemeral = auth.is_ephemeral;

    // Extract subscription info from user data
    let plan = auth
        .user
        .as_ref()
        .and_then(|u| u.get("subscription"))
        .and_then(|s| s.get("plan"))
        .and_then(|p| p.as_str())
        .unwrap_or("free");

    let active = auth
        .user
        .as_ref()
        .and_then(|u| u.get("subscription"))
        .and_then(|s| s.get("active"))
        .and_then(|a| a.as_bool())
        .unwrap_or(false);

    let expires = auth
        .user
        .as_ref()
        .and_then(|u| u.get("subscription"))
        .and_then(|s| s.get("expires_at"))
        .and_then(|e| e.as_str())
        .unwrap_or("n/a");

    if out.is_json() {
        let mut json = serde_json::json!({ "ephemeral": ephemeral });
        if !ephemeral {
            json["email"] = serde_json::json!(email);
            json["subscription"] = serde_json::json!({
                "plan": plan,
                "active": active,
                "expires_at": expires,
            });
        }
        out.print_json(&json);
    } else {
        out.info("Account", if ephemeral { "ephemeral" } else { "linked" });
        if !ephemeral {
            out.info("Email", email);
            out.info("Plan", plan);
            out.info("Subscription", if active { "active" } else { "inactive" });
            if active && expires != "n/a" {
                out.info("Expires", expires);
            }
        }
    }

    Ok(0)
}
