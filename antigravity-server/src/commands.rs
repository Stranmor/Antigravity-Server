use anyhow::Result;
use colored::Colorize;

use antigravity_core::modules::{account, config as core_config};

use crate::cli::{AccountCommands, ConfigCommands};

mod account_commands_impl {
    pub use crate::account_commands::*;
}
mod warmup_commands_impl {
    pub use crate::warmup_commands::*;
}
mod config_commands_impl {
    pub use crate::config_commands::*;
}

pub async fn handle_account_command(cmd: AccountCommands) -> Result<()> {
    match cmd {
        AccountCommands::List { json } => account_commands_impl::list_accounts(json).await,
        AccountCommands::Add { token, file } => {
            account_commands_impl::add_account(token, file).await
        }
        AccountCommands::Remove { identifier } => {
            account_commands_impl::remove_account(&identifier).await
        }
        AccountCommands::Toggle {
            identifier,
            enable,
            disable,
        } => account_commands_impl::toggle_account(&identifier, enable, disable).await,
        AccountCommands::Refresh { identifier } => {
            account_commands_impl::refresh_quota(&identifier).await
        }
    }
}

pub async fn handle_config_command(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Show { json } => config_commands_impl::show_config(json),
        ConfigCommands::Get { key } => config_commands_impl::get_config_value(&key),
        ConfigCommands::Set { key, value } => config_commands_impl::set_config_value(&key, &value),
    }
}

pub async fn handle_warmup(all: bool, email: Option<String>) -> Result<()> {
    if all {
        warmup_commands_impl::warmup_all().await
    } else if let Some(email) = email {
        warmup_commands_impl::warmup_account(&email).await
    } else {
        anyhow::bail!("Specify --all or provide an email address");
    }
}

pub async fn handle_status() -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let active = accounts
        .iter()
        .filter(|a| !a.disabled && !a.proxy_disabled)
        .count();

    println!("{}", "Antigravity Server Status".cyan().bold());
    println!("  Accounts: {} total, {} active", accounts.len(), active);
    println!("  Version: {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

pub async fn handle_generate_key() -> Result<()> {
    use rand::Rng;

    let key: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let api_key = format!("sk-{}", key);

    core_config::update_config(|config| {
        config.proxy.api_key = api_key.clone();
    })
    .map_err(|e| anyhow::anyhow!(e))?;

    println!("{} New API key generated: {}", "✓".green(), api_key);
    Ok(())
}
