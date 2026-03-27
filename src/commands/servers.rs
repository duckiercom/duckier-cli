use anyhow::{Context, Result};
use std::collections::BTreeMap;

use crate::api::client::ApiClient;
use crate::api::config::{fetch_app_config, Server};
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

#[derive(Debug, Clone)]
struct ServerGroup {
    country_code: String,
    city: String,
    count: usize,
    pro: bool,
}

fn group_servers(servers: &[Server]) -> Vec<ServerGroup> {
    let mut map: BTreeMap<(String, String), (usize, bool)> = BTreeMap::new();

    for s in servers {
        if s.locations.is_empty() {
            let key = (s.country_code.to_uppercase(), s.city.clone());
            let entry = map.entry(key).or_insert((0, s.pro));
            entry.0 += 1;
            entry.1 = entry.1 || s.pro;
        } else {
            for loc in &s.locations {
                let key = (loc.country_code.to_uppercase(), loc.city.clone());
                let entry = map.entry(key).or_insert((0, loc.pro));
                entry.0 += 1;
                entry.1 = entry.1 || loc.pro;
            }
        }
    }

    map.into_iter()
        .map(|((cc, city), (count, pro))| ServerGroup {
            country_code: cc,
            city,
            count,
            pro,
        })
        .collect()
}

pub async fn run(country: Option<String>, out: &Output) -> Result<i32> {
    ensure_onboarded()?;

    let api = ApiClient::new();
    let app_config = fetch_app_config(&api, false).context("failed to load server list")?;

    let mut groups = group_servers(&app_config.servers);

    if let Some(ref cc) = country {
        let cc_upper = cc.to_uppercase();
        groups.retain(|g| g.country_code == cc_upper);
    }

    if groups.is_empty() {
        out.error("No servers found");
        return Ok(1);
    }

    if out.is_json() {
        let json_list: Vec<serde_json::Value> = groups
            .iter()
            .map(|g| {
                serde_json::json!({
                    "country": g.country_code,
                    "city": g.city,
                    "servers": g.count,
                    "pro": g.pro,
                })
            })
            .collect();
        out.print_json(&json_list);
    } else {
        for g in &groups {
            let pro_tag = if g.pro { "  PRO" } else { "" };
            out.println(&format!(
                "  {:<4}{:<18}({} servers){}",
                g.country_code, g.city, g.count, pro_tag
            ));
        }
    }

    Ok(0)
}
