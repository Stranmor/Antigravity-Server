use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod ssh_client;
use ssh_client::SshClient;

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
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .context("setting default subscriber failed")?;

    let cli = Cli::parse();
    let ssh_client_factory = SshClient::new().await?;

    match cli.command {
        // Changed to cli.command to avoid lifetime issue
        Commands::Exec { host, command } => {
            let parts: Vec<&str> = host.split('@').collect();
            let (user, remote_host) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                anyhow::bail!("Host must be in format user@host");
            };

            info!("Connecting to {}@{}", user, remote_host);
            let mut session = ssh_client_factory
                .connect(user, remote_host.to_string())
                .await?; // Cloned remote_host

            let cmd_str = command.join(" ");
            info!("Executing command {:?} on host {}", cmd_str, remote_host);
            let output = session.exec_command(&cmd_str).await?;
            println!("{}", output);
            info!("Command executed successfully on {}@{}", user, remote_host);
            session.close().await?;
        }
        Commands::Upload {
            host,
            local_path,
            remote_path,
        } => {
            let parts: Vec<&str> = host.split('@').collect();
            let (user, remote_host) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                anyhow::bail!("Host must be in format user@host");
            };

            info!("Connecting to {}@{}", user, remote_host);
            let mut session = ssh_client_factory
                .connect(user, remote_host.to_string())
                .await?; // Cloned remote_host

            info!(
                "Uploading {:?} to {}:{}",
                local_path, remote_host, remote_path
            );
            // Temporarily commented out as upload_file is not yet implemented in ssh_client.rs
            // session
            //     .upload_file(remote_host, local_path, remote_path)
            //     .await?;
            // info!("File uploaded successfully to {}:{}", remote_host, remote_path);
            anyhow::bail!("Upload command is not yet implemented.");
            // session.close().await?; // Unreachable
        }
        Commands::Download {
            host,
            remote_path,
            local_path,
        } => {
            let parts: Vec<&str> = host.split('@').collect();
            let (user, remote_host) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                anyhow::bail!("Host must be in format user@host");
            };

            info!("Connecting to {}@{}", user, remote_host);
            let mut session = ssh_client_factory
                .connect(user, remote_host.to_string())
                .await?; // Cloned remote_host

            info!(
                "Downloading {}:{} to {:?}",
                remote_host, remote_path, local_path
            );
            // Temporarily commented out as download_file is not yet implemented in ssh_client.rs
            // session
            //     .download_file(remote_host, remote_path, local_path)
            //     .await?;
            // info!("File downloaded successfully from {}:{}", remote_host, remote_path);
            anyhow::bail!("Download command is not yet implemented.");
            // session.close().await?; // Unreachable
        }
    }

    Ok(())
}
