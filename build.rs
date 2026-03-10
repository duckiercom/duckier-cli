fn main() {
    // ── gRPC proto compilation ──
    let proto_path = "vpn.proto";
    let proto_dir = ".";

    tonic_build::configure()
        .build_server(false)
        .compile_protos(&[proto_path], &[proto_dir])
        .expect("Failed to compile vpn.proto");

    println!("cargo:rerun-if-changed={}", proto_path);

    // ── Embed branding from config.toml at compile time ──
    println!("cargo:rerun-if-changed=config.toml");

    let config_str = std::fs::read_to_string("config.toml").expect("Failed to read config.toml");
    let config: toml::Value = config_str.parse().expect("Failed to parse config.toml");

    // Branding
    let branding = &config["branding"];
    emit("BRAND_PRODUCT_NAME", branding, "product_name");
    emit("BRAND_BINARY_NAME", branding, "binary_name");
    emit("BRAND_DAEMON_NAME", branding, "daemon_name");

    // Backend
    let backend = &config["backend"];
    emit("BRAND_API_URL", backend, "api_url");
    emit("BRAND_FRONTEND_URL", backend, "frontend_url");
    emit("BRAND_APP_ID", backend, "app_id");
    emit("BRAND_EPHEMERAL_TLD", backend, "ephemeral_tld");
    emit("BRAND_CONNECT_URL", backend, "connect_url");
    emit("BRAND_UPDATE_URL", backend, "update_url");

    // Daemon
    let daemon = &config["daemon"];
    emit("BRAND_GRPC_ADDRESS", daemon, "grpc_address");

    // Kill switch
    let ks = &config["killswitch"];
    let domains: Vec<&str> = ks["api_domains"]
        .as_array()
        .expect("killswitch.api_domains must be an array")
        .iter()
        .map(|v| v.as_str().expect("domain must be a string"))
        .collect();
    println!("cargo:rustc-env=BRAND_KS_API_DOMAINS={}", domains.join(","));

    // Storage
    let storage = &config["storage"];
    emit("BRAND_CONFIG_DIR", storage, "config_dir");
}

fn emit(env_key: &str, table: &toml::Value, field: &str) {
    let val = table[field]
        .as_str()
        .unwrap_or_else(|| panic!("config.toml: missing {}", field));
    println!("cargo:rustc-env={}={}", env_key, val);
}
