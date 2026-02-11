//! CLI tool for remote VPS management via SSH.

#![allow(clippy::print_stdout, reason = "CLI tool outputs to stdout")]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod ssh_client;
#[allow(
    unused_imports,
    reason = "SshSession is used as a return type for SshClientFactory::connect"
)]
use ssh_client::{SshClientFactory, SshSession};

/// Shell-escapes a single argument for safe remote execution.
fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // If string contains no special characters, return as-is
    if s.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || b == b'-'
            || b == b'_'
            || b == b'.'
            || b == b'/'
            || b == b':'
            || b == b'='
            || b == b','
    }) {
        return s.to_string();
    }
    // Wrap in single quotes, escaping existing single quotes
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Parses a host string in format "user@host" into (user, host) tuple.
fn parse_host(host: &str) -> Result<(&str, &str)> {
    host.split_once('@').ok_or_else(|| anyhow::anyhow!("Host must be in format user@host"))
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Connects to a remote VPS and executes a command
    Exec {
        /// The target VPS host (e.g., user@host)
        host: String,
        /// The command to execute on the remote VPS
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Uploads a file to a remote VPS
    Upload {
        /// The target VPS host (e.g., user@host)
        host: String,
        /// Local path to the file to upload
        local_path: PathBuf,
        /// Remote path where the file will be uploaded
        remote_path: String,
    },
    /// Downloads a file from a remote VPS
    Download {
        /// The target VPS host (e.g., user@host)
        host: String,
        /// Remote path to the file to download
        remote_path: String,
        /// Local path where the file will be saved
        local_path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber)
        .context("setting default subscriber failed")?;

    let cli = Cli::parse();
    let ssh_client_factory = SshClientFactory::new();

    match cli.command {
        Commands::Exec { host, command } => {
            let (user, remote_host) = parse_host(&host)?;

            info!("Connecting to {user}@{remote_host}");
            let session = ssh_client_factory.connect(user, remote_host.to_string()).await?;

            let cmd_str = command.iter().map(|arg| shell_escape(arg)).collect::<Vec<_>>().join(" ");
            info!("Executing command {cmd_str:?} on host {remote_host}");
            let output = session.exec_command(&cmd_str).await?;
            println!("{output}");
            info!("Command executed successfully on {user}@{remote_host}");
            session.close().await?;
        },
        Commands::Upload { host, local_path, remote_path } => {
            let (user, remote_host) = parse_host(&host)?;

            info!("Connecting to {user}@{remote_host}");
            let session = ssh_client_factory.connect(user, remote_host.to_string()).await?;

            info!("Uploading {local_path:?} to {remote_host}:{remote_path}");
            session.upload_file(remote_host, &local_path, &remote_path).await?;
            info!("File uploaded successfully to {remote_host}:{remote_path}");
            session.close().await?;
        },
        Commands::Download { host, remote_path, local_path } => {
            let (user, remote_host) = parse_host(&host)?;

            info!("Connecting to {user}@{remote_host}");
            let session = ssh_client_factory.connect(user, remote_host.to_string()).await?;

            info!("Downloading {remote_host}:{remote_path} to {local_path:?}");
            session.download_file(remote_host, &remote_path, &local_path).await?;
            info!("File downloaded successfully from {remote_host}:{remote_path}");
            session.close().await?;
        },
    }

    Ok(())
}
