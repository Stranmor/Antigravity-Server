use anyhow::{Context, Result};
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};

use antigravity_core::modules::{account, oauth};
use antigravity_types::models::TokenData;

pub async fn list_accounts(json: bool) -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;

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

pub async fn add_account(token: Option<String>, file: Option<std::path::PathBuf>) -> Result<()> {
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
    let token_response =
        oauth::refresh_access_token(refresh_token).await.map_err(|e| anyhow::anyhow!(e))?;
    let user_info =
        oauth::get_user_info(&token_response.access_token).await.map_err(|e| anyhow::anyhow!(e))?;
    let token_data = TokenData::new(
        token_response.access_token,
        refresh_token.to_string(),
        token_response.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    );
    let acc =
        account::upsert_account(user_info.email.clone(), user_info.get_display_name(), token_data)
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

pub async fn remove_account(identifier: &str) -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let acc = accounts
        .iter()
        .find(|a| a.email == identifier || a.id == identifier)
        .context("Account not found")?;
    account::delete_account(&acc.id).map_err(|e| anyhow::anyhow!(e))?;
    println!("{} Account removed: {}", "✓".green(), acc.email.green());
    Ok(())
}

pub async fn toggle_account(identifier: &str, enable: bool, disable: bool) -> Result<()> {
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

pub async fn refresh_quota(identifier: &str) -> Result<()> {
    if identifier == "all" {
        return refresh_all_quotas().await;
    }
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let mut acc = accounts
        .into_iter()
        .find(|a| a.email == identifier || a.id == identifier)
        .context("Account not found")?;
    println!("{}", format!("Refreshing quota for {}...", acc.email).cyan());
    account::fetch_quota_with_retry(&mut acc).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
    account::update_account_quota_async(acc.id.clone(), acc.quota.clone().unwrap_or_default())
        .await
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
                if let Some(quota) = acc.quota.clone() {
                    let _ = account::update_account_quota_async(acc.id.clone(), quota).await;
                }
                println!("{}", "✓".green());
                success += 1;
            },
            Err(e) => {
                println!("{} ({})", "✗".red(), e);
                failed += 1;
            },
        }
    }
    println!("\n{}/{} accounts refreshed ({} failed)", success, total, failed);
    Ok(())
}
