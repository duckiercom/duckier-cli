use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use tracing::debug;

use crate::brand;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthData {
    #[serde(default)]
    pub auth_token: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub is_ephemeral: bool,
    #[serde(default)]
    pub user: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WireGuardData {
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub preshared_key: String,
}

pub(crate) fn config_dir() -> PathBuf {
    if let Some(dir) = dirs::config_dir() {
        return dir.join(brand::CONFIG_DIR);
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(".config").join(brand::CONFIG_DIR);
    }

    PathBuf::from(".").join(format!(".{}", brand::CONFIG_DIR))
}

fn ensure_dir() -> std::io::Result<()> {
    let dir = config_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
        debug!("Created config directory: {}", dir.display());
    }
    #[cfg(unix)]
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;

    // Also create cache subdir
    let cache = dir.join("cache");
    if !cache.exists() {
        std::fs::create_dir_all(&cache)?;
    }
    #[cfg(unix)]
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o700))?;

    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(filename: &str) -> T {
    let path = config_dir().join(filename);
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

fn write_json<T: Serialize>(filename: &str, data: &T) -> Result<()> {
    ensure_dir().with_context(|| format!("failed to create config directory for {}", filename))?;
    let path = config_dir().join(filename);
    let json = serde_json::to_string_pretty(data)
        .with_context(|| format!("failed to serialize {}", path.display()))?;

    #[cfg(unix)]
    {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("failed to write {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", path.display()))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to secure {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    debug!("Wrote {}", path.display());
    Ok(())
}

// ── Auth ──

pub fn load_auth() -> AuthData {
    read_json("auth.json")
}

pub fn save_auth(auth: &AuthData) -> Result<()> {
    write_json("auth.json", auth)
}

pub fn has_auth() -> bool {
    let auth = load_auth();
    !auth.auth_token.is_empty()
}

/// Forget the account and its WireGuard keys. The keys are registered on the
/// backend under the user, so they are useless once the account is gone.
pub fn clear_credentials() -> Result<()> {
    save_auth(&AuthData::default())?;
    let wg_path = config_dir().join("wireguard.json");
    if wg_path.exists() {
        std::fs::remove_file(&wg_path)
            .with_context(|| format!("failed to remove {}", wg_path.display()))?;
    }
    Ok(())
}

// ── WireGuard keys ──

pub fn load_wireguard() -> Option<WireGuardData> {
    let data: WireGuardData = read_json("wireguard.json");
    if data.public_key.is_empty() {
        None
    } else {
        Some(data)
    }
}

pub fn save_wireguard(wg: &WireGuardData) -> Result<()> {
    write_json("wireguard.json", wg)
}

// ── App config cache ──

pub fn load_cached_appconfig() -> Option<serde_json::Value> {
    let path = config_dir().join("cache/appconfig.json");
    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = std::time::SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    // 1-hour TTL
    if age > std::time::Duration::from_secs(3600) {
        return None;
    }
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn save_cached_appconfig(config: &serde_json::Value) -> Result<()> {
    ensure_dir().context("failed to create app config cache directory")?;
    let path = config_dir().join("cache/appconfig.json");
    let json =
        serde_json::to_string(config).context("failed to serialize cached app configuration")?;
    std::fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
