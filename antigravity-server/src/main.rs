//! Antigravity Server - Headless Daemon
//!
//! A pure Rust HTTP server that:
//! - Runs the proxy logic (account rotation, API forwarding) on /v1/*
//! - Serves the Leptos WebUI as static files
//! - Provides a REST API for CLI and UI control on /api/*
//!
//! Access via: http://localhost:8045

// LINT OVERRIDE: Server binary uses arithmetic for timestamps, metrics, and config.
// Values are bounded by system limits. See AGENTS.md section 2.0.2.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "server binary: bounded arithmetic on system-limited values"
)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI application uses stdout/stderr for user output"
)]
#![allow(
    clippy::clone_on_ref_ptr,
    reason = "Arc::clone() vs .clone() is stylistic, both are correct"
)]

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod api;
mod cli;
mod commands;
mod config_sync;
mod router;
mod scheduler;
mod server_utils;
mod state;

#[cfg(test)]
mod test_helpers;

mod account_commands;
mod config_commands;
mod warmup_commands;

use antigravity_core::modules::account_pg::PostgresAccountRepository;
use antigravity_core::modules::repository::AccountRepository;
use antigravity_core::proxy::SignatureCache;
use cli::{Cli, Commands};
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.log_level.as_str() {
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Some(Commands::Account(cmd)) => commands::handle_account_command(cmd).await,
        Some(Commands::Config(cmd)) => commands::handle_config_command(cmd).await,
        Some(Commands::Warmup { all, email }) => commands::handle_warmup(all, email).await,
        Some(Commands::Status) => commands::handle_status().await,
        Some(Commands::GenerateKey) => commands::handle_generate_key().await,
        Some(Commands::Serve { port }) => run_server(port).await,
        None => run_server(cli.port).await,
    }
}

async fn run_server(port: u16) -> Result<()> {
    info!("🚀 Antigravity Server starting on port {}...", port);

    let _ = antigravity_core::proxy::prometheus::init_metrics();
    info!("📊 Prometheus metrics initialized");

    let data_dir = antigravity_core::modules::account::get_data_dir()
        .map_err(|e| anyhow::anyhow!("Failed to get data directory: {}", e))?;
    let initial_app_config = match antigravity_core::modules::config::load_config() {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!("⚠️ Failed to load config, using defaults: {}", e);
            Default::default()
        },
    };
    let initial_proxy_config = initial_app_config.proxy;

    let token_manager = Arc::new(antigravity_core::proxy::TokenManager::new(data_dir.clone()));

    let repository: Option<Arc<dyn AccountRepository>> = match std::env::var("DATABASE_URL") {
        Ok(database_url) => {
            info!("🗄️ Connecting to PostgreSQL...");
            match PostgresAccountRepository::connect(&database_url).await {
                Ok(repo) => {
                    info!("✅ PostgreSQL connected");
                    if let Err(e) = repo.run_migrations().await {
                        tracing::warn!(
                            "⚠️ Database migration issue: {}. Continuing with existing schema.",
                            e
                        );
                    } else {
                        info!("✅ Database migrations applied");
                    }
                    if let Err(e) =
                        antigravity_core::modules::json_migration::migrate_json_to_postgres(&repo)
                            .await
                    {
                        tracing::warn!("⚠️ JSON migration skipped or failed: {}", e);
                    }
                    SignatureCache::global().set_db_pool(repo.pool().clone());
                    Some(Arc::new(repo) as Arc<dyn AccountRepository>)
                },
                Err(e) => {
                    tracing::error!(
                        "❌ PostgreSQL connection failed: {}. Falling back to JSON storage.",
                        e
                    );
                    None
                },
            }
        },
        Err(_) => {
            info!("ℹ️ DATABASE_URL not set, using JSON file storage");
            None
        },
    };

    if let Some(ref repo) = repository {
        token_manager.set_repository(Arc::clone(repo)).await;
    }

    match token_manager.load_accounts().await {
        Ok(count) => {
            tracing::info!("📊 Loaded {} accounts into token manager", count);
        },
        Err(e) => {
            tracing::warn!("⚠️ Could not load accounts into token manager: {}", e);
        },
    }

    token_manager.start_auto_cleanup();
    token_manager.start_auto_account_sync();

    let monitor = if let Some(ref repo) = repository {
        Arc::new(antigravity_core::proxy::ProxyMonitor::with_db(
            Arc::new(antigravity_core::proxy::monitor::NoopEventBus),
            Arc::clone(repo),
            token_manager.tokens_ref().clone(),
        ))
    } else {
        Arc::new(antigravity_core::proxy::ProxyMonitor::new())
    };

    let state = AppState::new_with_components(
        token_manager.clone(),
        monitor.clone(),
        initial_proxy_config.clone(),
        repository,
    )
    .await?;

    info!("✅ Application state initialized");
    info!("📊 {} accounts loaded", state.get_account_count().await);

    scheduler::start(state.clone());
    scheduler::start_quota_refresh(state.clone());

    if let Ok(remote_url) = std::env::var("ANTIGRAVITY_SYNC_REMOTE") {
        config_sync::start_auto_config_sync(Arc::new(state.clone()), remote_url);
    }

    let listener = server_utils::create_listener(port, &initial_proxy_config).await?;
    let local_addr = listener.local_addr()?;
    state.set_bound_port(local_addr.port());

    let app = router::build_router(state).await;

    info!("🌐 Server listening on http://{}", local_addr);
    info!("📊 WebUI available at http://localhost:{}/", local_addr.port());
    info!("🔌 API available at http://localhost:{}/api/", local_addr.port());
    info!("🔀 Proxy endpoints at http://localhost:{}/v1/", local_addr.port());

    axum::serve(listener, app).with_graceful_shutdown(server_utils::shutdown_signal()).await?;

    info!("👋 Server shutdown complete");
    Ok(())
}
