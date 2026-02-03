use anyhow::Result;
use colored::Colorize;

use antigravity_core::modules::config as core_config;

pub fn show_config(json: bool) -> Result<()> {
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

pub fn get_config_value(key: &str) -> Result<()> {
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

pub fn set_config_value(key: &str, value: &str) -> Result<()> {
    match key {
        "proxy.port" => {
            value
                .parse::<u16>()
                .map_err(|_| anyhow::anyhow!("Invalid port number: {}", value))?;
        }
        "proxy.enable_logging" => {
            value
                .parse::<bool>()
                .map_err(|_| anyhow::anyhow!("Invalid boolean: {}", value))?;
        }
        "proxy.api_key" => {}
        _ => return Err(anyhow::anyhow!("Unknown config key: {}", key)),
    }

    core_config::update_config(|config| match key {
        "proxy.port" => config.proxy.port = value.parse().unwrap(),
        "proxy.api_key" => config.proxy.api_key = value.to_string(),
        "proxy.enable_logging" => config.proxy.enable_logging = value.parse().unwrap(),
        _ => {}
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
