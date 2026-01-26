use anyhow::{Context, Result};
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};

use antigravity_core::modules::{account, config as core_config, oauth};
use antigravity_shared::models::TokenData;

use crate::cli::{AccountCommands, ConfigCommands};

fn str_err<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!(e))
}

pub async fn handle_account_command(cmd: AccountCommands) -> Result<()> {
    match cmd {
        AccountCommands::List { json } => list_accounts(json).await,
        AccountCommands::Add { token, file } => add_account(token, file).await,
        AccountCommands::Remove { identifier } => remove_account(&identifier).await,
        AccountCommands::Toggle {
            identifier,
            enable,
            disable,
        } => toggle_account(&identifier, enable, disable).await,
        AccountCommands::Refresh { identifier } => refresh_quota(&identifier).await,
    }
}

pub async fn handle_config_command(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Show { json } => show_config(json),
        ConfigCommands::Get { key } => get_config_value(&key),
        ConfigCommands::Set { key, value } => set_config_value(&key, &value),
    }
}

pub async fn handle_warmup(all: bool, email: Option<String>) -> Result<()> {
    if all {
        warmup_all().await
    } else if let Some(email) = email {
        warmup_account(&email).await
    } else {
        anyhow::bail!("Specify --all or provide an email address");
    }
}

pub async fn handle_status() -> Result<()> {
    let accounts = str_err(account::list_accounts())?;
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

    str_err(core_config::update_config(|config| {
        config.proxy.api_key = api_key.clone();
    }))?;

    println!("{} New API key generated: {}", "✓".green(), api_key);
    Ok(())
}

async fn list_accounts(json: bool) -> Result<()> {
    let accounts = str_err(account::list_accounts())?;

    if json {
        println!("{}", serde_json::to_string_pretty(&accounts)?);
        return Ok(());
    }

    if accounts.is_empty() {
        println!("{}", "No accounts found.".yellow());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Email", "Name", "Gemini", "Claude", "Status"]);

    for acc in &accounts {
        let gemini = get_quota(acc, "gemini");
        let claude = get_quota(acc, "claude");
        let status = if acc.disabled || acc.proxy_disabled {
            Cell::new("Disabled").fg(Color::Red)
        } else {
            Cell::new("Active").fg(Color::Green)
        };

        table.add_row(vec![
            Cell::new(&acc.email),
            Cell::new(acc.name.as_deref().unwrap_or("-")),
            Cell::new(&gemini),
            Cell::new(&claude),
            status,
        ]);
    }

    println!("{table}");
    println!("\n{} accounts total", accounts.len());
    Ok(())
}

fn get_quota(acc: &antigravity_core::models::Account, model: &str) -> String {
    acc.quota
        .as_ref()
        .and_then(|q| {
            q.models
                .iter()
                .find(|m| m.name.to_lowercase().contains(model))
                .map(|m| format!("{}%", m.percentage))
        })
        .unwrap_or_else(|| "-".to_string())
}

async fn add_account(token: Option<String>, file: Option<std::path::PathBuf>) -> Result<()> {
    if let Some(token) = token {
        add_by_token(&token).await
    } else if let Some(path) = file {
        add_from_file(&path).await
    } else {
        anyhow::bail!("Specify --token or --file");
    }
}

async fn add_by_token(refresh_token: &str) -> Result<()> {
    println!("{}", "Validating refresh token...".cyan());

    let token_response = oauth::refresh_access_token(refresh_token)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let user_info = oauth::get_user_info(&token_response.access_token)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let token_data = TokenData::new(
        token_response.access_token,
        refresh_token.to_string(),
        token_response.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    );

    let acc = account::upsert_account(
        user_info.email.clone(),
        user_info.get_display_name(),
        token_data,
    )
    .map_err(|e| anyhow::anyhow!(e))?;

    println!("{} Account added: {}", "✓".green(), acc.email.green());
    Ok(())
}

async fn add_from_file(path: &std::path::Path) -> Result<()> {
    let content = std::fs::read_to_string(path).context("Failed to read file")?;
    let acc: antigravity_core::models::Account =
        serde_json::from_str(&content).context("Failed to parse account JSON")?;

    account::save_account(&acc).map_err(|e| anyhow::anyhow!(e))?;
    println!("{} Account imported: {}", "✓".green(), acc.email.green());
    Ok(())
}

async fn remove_account(identifier: &str) -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let acc = accounts
        .iter()
        .find(|a| a.email == identifier || a.id == identifier)
        .context("Account not found")?;

    account::delete_account(&acc.id).map_err(|e| anyhow::anyhow!(e))?;
    println!("{} Account removed: {}", "✓".green(), acc.email.green());
    Ok(())
}

async fn toggle_account(identifier: &str, enable: bool, disable: bool) -> Result<()> {
    if enable == disable {
        anyhow::bail!("Specify either --enable or --disable");
    }

    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let mut acc = accounts
        .into_iter()
        .find(|a| a.email == identifier || a.id == identifier)
        .context("Account not found")?;

    acc.proxy_disabled = disable;
    account::save_account(&acc).map_err(|e| anyhow::anyhow!(e))?;

    let status = if disable { "disabled" } else { "enabled" };
    println!("{} Account {} {}", "✓".green(), acc.email, status);
    Ok(())
}

async fn refresh_quota(identifier: &str) -> Result<()> {
    if identifier == "all" {
        return refresh_all_quotas().await;
    }

    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let mut acc = accounts
        .into_iter()
        .find(|a| a.email == identifier || a.id == identifier)
        .context("Account not found")?;

    println!(
        "{}",
        format!("Refreshing quota for {}...", acc.email).cyan()
    );

    account::fetch_quota_with_retry(&mut acc)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    account::update_account_quota(&acc.id, acc.quota.clone().unwrap_or_default())
        .map_err(|e| anyhow::anyhow!(e))?;
    println!("{} Quota refreshed for {}", "✓".green(), acc.email.green());
    Ok(())
}

async fn refresh_all_quotas() -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let total = accounts.len();
    let mut success = 0;
    let mut failed = 0;

    for mut acc in accounts {
        if acc.disabled {
            continue;
        }
        print!("Refreshing {}... ", acc.email);
        match account::fetch_quota_with_retry(&mut acc).await {
            Ok(_) => {
                // Use update_account_quota to properly populate protected_models
                if let Some(quota) = acc.quota.clone() {
                    let _ = account::update_account_quota(&acc.id, quota);
                }
                println!("{}", "✓".green());
                success += 1;
            }
            Err(e) => {
                println!("{} ({})", "✗".red(), e);
                failed += 1;
            }
        }
    }

    println!(
        "\n{}/{} accounts refreshed ({} failed)",
        success, total, failed
    );
    Ok(())
}

async fn warmup_account(email: &str) -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let mut acc = accounts
        .into_iter()
        .find(|a| a.email == email || a.id == email)
        .context("Account not found")?;

    println!("{}", format!("Warming up {}...", acc.email).cyan());

    account::fetch_quota_with_retry(&mut acc)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    account::save_account(&acc).map_err(|e| anyhow::anyhow!(e))?;
    println!("{} Account {} warmed up", "✓".green(), acc.email.green());
    Ok(())
}

async fn warmup_all() -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let enabled: Vec<_> = accounts
        .into_iter()
        .filter(|a| !a.disabled && !a.proxy_disabled)
        .collect();

    let total = enabled.len();
    let mut success = 0;

    for mut acc in enabled {
        print!("Warming up {}... ", acc.email);
        match account::fetch_quota_with_retry(&mut acc).await {
            Ok(_) => {
                let _ = account::save_account(&acc);
                println!("{}", "✓".green());
                success += 1;
            }
            Err(e) => println!("{} ({})", "✗".red(), e),
        }
    }

    println!("\n{}/{} accounts warmed up", success, total);
    Ok(())
}

fn show_config(json: bool) -> Result<()> {
    let config = core_config::load_config().map_err(|e| anyhow::anyhow!(e))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        println!("{}", "Proxy Configuration:".cyan().bold());
        println!("  Port: {}", config.proxy.port);
        println!("  API Key: {}", mask_key(&config.proxy.api_key));
        println!("  Logging: {}", config.proxy.enable_logging);
        println!("  Custom Mappings: {}", config.proxy.custom_mapping.len());
    }
    Ok(())
}

fn get_config_value(key: &str) -> Result<()> {
    let config = core_config::load_config().map_err(|e| anyhow::anyhow!(e))?;

    let value = match key {
        "proxy.port" => config.proxy.port.to_string(),
        "proxy.api_key" => config.proxy.api_key.clone(),
        "proxy.enable_logging" => config.proxy.enable_logging.to_string(),
        _ => anyhow::bail!("Unknown config key: {}", key),
    };

    println!("{}", value);
    Ok(())
}

fn set_config_value(key: &str, value: &str) -> Result<()> {
    core_config::update_config(|config| match key {
        "proxy.port" => {
            config.proxy.port = value.parse().expect("Invalid port number");
        }
        "proxy.api_key" => {
            config.proxy.api_key = value.to_string();
        }
        "proxy.enable_logging" => {
            config.proxy.enable_logging = value.parse().expect("Invalid boolean");
        }
        _ => panic!("Unknown config key: {}", key),
    })
    .map_err(|e| anyhow::anyhow!(e))?;

    println!("{} Config updated: {} = {}", "✓".green(), key, value);
    Ok(())
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "*".repeat(key.len());
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}
