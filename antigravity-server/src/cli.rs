use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "antigravity",
    about = "Antigravity Server - Headless AI Gateway",
    version = env!("CARGO_PKG_VERSION"),
    author,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long, env = "ANTIGRAVITY_PORT", default_value = "8045")]
    pub port: u16,

    #[arg(short, long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Start the proxy server (default if no command specified)")]
    Serve {
        #[arg(short, long, env = "ANTIGRAVITY_PORT", default_value = "8045")]
        port: u16,
    },

    #[command(subcommand, about = "Manage Google accounts")]
    Account(AccountCommands),

    #[command(subcommand, about = "View and modify configuration")]
    Config(ConfigCommands),

    #[command(about = "Trigger model warmup for accounts")]
    Warmup {
        #[arg(long, help = "Warmup all enabled accounts")]
        all: bool,

        #[arg(help = "Email of specific account to warmup")]
        email: Option<String>,
    },

    #[command(about = "Show proxy status")]
    Status,

    #[command(about = "Generate a new API key")]
    GenerateKey,
}

#[derive(Subcommand)]
pub enum AccountCommands {
    #[command(about = "List all accounts with quota status")]
    List {
        #[arg(short, long, help = "Output as JSON")]
        json: bool,
    },

    #[command(about = "Add account from refresh token or JSON file")]
    Add {
        #[arg(long, conflicts_with = "file", help = "Google refresh token")]
        token: Option<String>,

        #[arg(
            short,
            long,
            conflicts_with = "token",
            help = "Path to account JSON file"
        )]
        file: Option<PathBuf>,
    },

    #[command(about = "Remove an account")]
    Remove {
        #[arg(help = "Email or account ID to remove")]
        identifier: String,
    },

    #[command(about = "Enable or disable an account for proxy")]
    Toggle {
        #[arg(help = "Email or account ID")]
        identifier: String,

        #[arg(long, help = "Enable the account")]
        enable: bool,

        #[arg(long, help = "Disable the account")]
        disable: bool,
    },

    #[command(about = "Refresh quota for an account")]
    Refresh {
        #[arg(help = "Email or account ID (or 'all' for all accounts)")]
        identifier: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    #[command(about = "Show current configuration")]
    Show {
        #[arg(short, long, help = "Output as JSON")]
        json: bool,
    },

    #[command(about = "Get a specific configuration value")]
    Get {
        #[arg(help = "Configuration key (e.g., 'proxy.api_key', 'proxy.port')")]
        key: String,
    },

    #[command(about = "Set a configuration value")]
    Set {
        #[arg(help = "Configuration key")]
        key: String,

        #[arg(help = "New value")]
        value: String,
    },
}
