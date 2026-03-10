use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use x25519_dalek::PublicKey;

pub struct WgKeyPair {
    pub private_key: String,
    pub public_key: String,
    pub preshared_key: String,
}

/// Generate a WireGuard x25519 key pair + preshared key.
pub fn generate_wireguard_keys() -> WgKeyPair {
    // StaticSecret exposes bytes (EphemeralSecret doesn't).
    let static_secret = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let static_public = PublicKey::from(&static_secret);

    // Generate a random preshared key (32 bytes)
    let mut psk_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut OsRng, &mut psk_bytes);

    WgKeyPair {
        private_key: BASE64.encode(static_secret.to_bytes()),
        public_key: BASE64.encode(static_public.to_bytes()),
        preshared_key: BASE64.encode(psk_bytes),
    }
}

/// SHA256 hash of a string, returned as hex.
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a stable device ID from the platform's machine identifier.
pub fn device_id() -> String {
    let raw = platform_machine_id();
    sha256_hex(raw.trim())
}

#[cfg(target_os = "linux")]
fn platform_machine_id() -> String {
    std::fs::read_to_string("/etc/machine-id")
        .or_else(|_| std::fs::read_to_string("/var/lib/dbus/machine-id"))
        .unwrap_or_else(|_| fallback_id())
}

#[cfg(target_os = "macos")]
fn platform_machine_id() -> String {
    std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()
        .and_then(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .find(|l| l.contains("IOPlatformUUID"))
                .and_then(|l| l.split('"').nth(3))
                .map(|s| s.to_string())
        })
        .unwrap_or_else(fallback_id)
}

#[cfg(target_os = "windows")]
fn platform_machine_id() -> String {
    std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()
        .and_then(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .find(|l| l.contains("MachineGuid"))
                .and_then(|l| l.split_whitespace().last())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(fallback_id)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_machine_id() -> String {
    fallback_id()
}

fn fallback_id() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
