use anyhow::Result;
use colored::Colorize;

use antigravity_core::modules::account;

pub async fn warmup_account(email: &str) -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let mut acc = accounts
        .into_iter()
        .find(|a| a.email == email || a.id == email)
        .ok_or_else(|| anyhow::anyhow!("Account not found"))?;

    println!("{}", format!("Warming up {}...", acc.email).cyan());

    account::fetch_quota_with_retry(&mut acc)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    if let Some(quota) = acc.quota.clone() {
        account::update_account_quota_async(acc.id.clone(), quota)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    println!("{} Account {} warmed up", "✓".green(), acc.email.green());
    Ok(())
}

pub async fn warmup_all() -> Result<()> {
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
                if let Some(quota) = acc.quota.clone() {
                    let _ = account::update_account_quota_async(acc.id.clone(), quota).await;
                }
                println!("{}", "✓".green());
                success += 1;
            }
            Err(e) => println!("{} ({})", "✗".red(), e),
        }
    }

    println!("\n{}/{} accounts warmed up", success, total);
    Ok(())
}
