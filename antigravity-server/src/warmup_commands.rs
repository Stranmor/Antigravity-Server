use anyhow::Result;
use colored::Colorize;
use std::io::Write;

use antigravity_core::modules::account;

pub async fn warmup_account(email: &str) -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let acc = accounts
        .into_iter()
        .find(|a| a.email == email || a.id == email)
        .ok_or_else(|| anyhow::anyhow!("Account not found"))?;

    println!("{}", format!("Warming up {}...", acc.email).cyan());

    let result = account::fetch_quota_with_retry(&acc, None)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    account::update_account_quota_async(acc.id.clone(), result.quota)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    println!("{} Account {} warmed up", "✓".green(), acc.email.green());
    Ok(())
}

pub async fn warmup_all() -> Result<()> {
    let accounts = account::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let enabled: Vec<_> =
        accounts.into_iter().filter(|a| !a.disabled && !a.proxy_disabled).collect();

    let total = enabled.len();
    let mut success = 0;
    let mut failed = 0;

    for acc in enabled {
        print!("Warming up {}... ", acc.email);
        let _ = std::io::stdout().flush();
        match account::fetch_quota_with_retry(&acc, None).await {
            Ok(result) => {
                if let Err(e) =
                    account::update_account_quota_async(acc.id.clone(), result.quota).await
                {
                    eprintln!("{} (persist: {})", "✗".red(), e);
                    failed += 1;
                } else {
                    println!("{}", "✓".green());
                    success += 1;
                }
            },
            Err(e) => {
                println!("{} ({})", "✗".red(), e);
                failed += 1;
            },
        }
    }

    println!("\n{}/{} accounts warmed up ({} failed)", success, total, failed);
    Ok(())
}
