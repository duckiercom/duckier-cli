mod api;
mod brand;
mod commands;
mod crypto;
mod grpc;
mod http_client;
mod output;
mod storage;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;

/// Duckier VPN — headless CLI client
#[derive(Parser)]
#[command(name = "duckier-cli", version, about)]
struct Cli {
    /// Output as JSON (for scripting)
    #[arg(long, global = true)]
    json: bool,

    /// gRPC daemon address
    #[arg(long, global = true, default_value = brand::GRPC_ADDRESS)]
    daemon_addr: String,

    /// Enable verbose logging
    #[cfg(debug_assertions)]
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to a VPN server
    Connect {
        /// Country code (e.g. US, DE, NL)
        #[arg(long)]
        country: Option<String>,
        /// City name (e.g. "New York", "Frankfurt")
        #[arg(long)]
        city: Option<String>,
    },
    /// Disconnect active VPN
    Disconnect,
    /// Show connection and daemon status
    Status,
    /// List available VPN servers
    Servers {
        /// Filter by country code
        #[arg(long)]
        country: Option<String>,
    },
    /// Link to a real Duckier account (connection code)
    Login,
    /// Log out and clear credentials
    Logout,
    /// Show account info and subscription status
    Account,
    /// Manage network kill switch
    Killswitch {
        #[command(subcommand)]
        action: KillSwitchAction,
    },
    /// Daemon operations
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Check for and apply CLI updates
    Update,
    /// Uninstall the CLI and daemon
    Uninstall,
}

#[derive(Subcommand)]
enum KillSwitchAction {
    /// Enable kill switch
    Enable {
        /// Allow LAN traffic
        #[arg(long)]
        allow_lan: bool,
    },
    /// Disable kill switch
    Disable,
    /// Show kill switch status
    Status,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Check daemon health
    Health,
    /// Show daemon PID
    Pid,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    #[cfg(debug_assertions)]
    let max_level = if cli.verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::WARN
    };
    #[cfg(not(debug_assertions))]
    let max_level = LevelFilter::WARN;

    tracing_subscriber::fmt().with_max_level(max_level).init();

    let out = output::Output::new(cli.json);
    let result = run(cli, &out).await;

    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            out.error(&format!("{:#}", e));
            std::process::exit(1);
        }
    }
}

async fn run(cli: Cli, out: &output::Output) -> Result<i32> {
    match cli.command {
        Commands::Connect { country, city } => {
            commands::connect::run(&cli.daemon_addr, country, city, out).await
        }
        Commands::Disconnect => commands::disconnect::run(&cli.daemon_addr, out).await,
        Commands::Status => commands::status::run(&cli.daemon_addr, out).await,
        Commands::Servers { country } => commands::servers::run(country, out).await,
        Commands::Login => commands::login::run(out).await,
        Commands::Logout => commands::logout::run(out).await,
        Commands::Account => commands::account::run(out).await,
        Commands::Killswitch { action } => match action {
            KillSwitchAction::Enable { allow_lan } => {
                commands::killswitch::enable(&cli.daemon_addr, allow_lan, out).await
            }
            KillSwitchAction::Disable => commands::killswitch::disable(&cli.daemon_addr, out).await,
            KillSwitchAction::Status => commands::killswitch::status(&cli.daemon_addr, out).await,
        },
        Commands::Daemon { action } => match action {
            DaemonAction::Health => commands::daemon::health(&cli.daemon_addr, out).await,
            DaemonAction::Pid => commands::daemon::pid(&cli.daemon_addr, out).await,
        },
        Commands::Update => commands::update::run(out).await,
        Commands::Uninstall => commands::uninstall::run(out).await,
    }
}
