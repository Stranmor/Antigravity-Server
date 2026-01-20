use serde_json;
use std::fs;
use std::path::PathBuf;

use crate::models::AppConfig;
use crate::modules::account::get_data_dir;

const CONFIG_FILE: &str = "gui_config.json";

/// 加载应用配置
///
/// 注意：此函数包含迁移逻辑，会自动将旧的映射字段迁移到 custom_mapping
pub fn load_config() -> Result<AppConfig, String> {
    let data_dir = get_data_dir()?;
    let config_path = data_dir.join(CONFIG_FILE);

    if !config_path.exists() {
        return Ok(AppConfig::new());
    }

    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("读取配置文件失败: {}", e))?;

    let mut v: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析配置文件失败: {}", e))?;

    let mut modified = false;

    // 迁移逻辑
    if let Some(proxy) = v.get_mut("proxy") {
        let mut custom_mapping = proxy
            .get("custom_mapping")
            .and_then(|m| m.as_object())
            .cloned()
            .unwrap_or_default();

        // 迁移 Anthropic 映射
        if let Some(anthropic) = proxy
            .get_mut("anthropic_mapping")
            .and_then(|m| m.as_object_mut())
        {
            for (k, v) in anthropic.iter() {
                if !k.ends_with("-series") && !custom_mapping.contains_key(k) {
                    custom_mapping.insert(k.clone(), v.clone());
                }
            }
            // 移除旧字段
            if let Some(obj) = proxy.as_object_mut() {
                obj.remove("anthropic_mapping");
            }
            modified = true;
        }

        // 迁移 OpenAI 映射
        if let Some(openai) = proxy
            .get_mut("openai_mapping")
            .and_then(|m| m.as_object_mut())
        {
            for (k, v) in openai.iter() {
                if !k.ends_with("-series") && !custom_mapping.contains_key(k) {
                    custom_mapping.insert(k.clone(), v.clone());
                }
            }
            // 移除旧字段
            if let Some(obj) = proxy.as_object_mut() {
                obj.remove("openai_mapping");
            }
            modified = true;
        }

        if modified {
            if let Some(obj) = proxy.as_object_mut() {
                obj.insert(
                    "custom_mapping".to_string(),
                    serde_json::Value::Object(custom_mapping),
                );
            }
        }
    }

    let config: AppConfig =
        serde_json::from_value(v).map_err(|e| format!("迁移后转换配置失败: {}", e))?;

    // 如果发生了迁移，自动保存一次以清理文件
    if modified {
        let _ = save_config(&config);
    }

    Ok(config)
}

/// 保存应用配置
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let config_path = data_dir.join(CONFIG_FILE);
    let temp_path = data_dir.join(format!("{}.tmp", CONFIG_FILE));

    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {}", e))?;

    // Atomic write
    fs::write(&temp_path, content).map_err(|e| format!("写入临时配置失败: {}", e))?;
    fs::rename(&temp_path, &config_path).map_err(|e| format!("保存配置失败: {}", e))
}

/// Update specific fields in the config.
pub fn update_config<F>(updater: F) -> Result<AppConfig, String>
where
    F: FnOnce(&mut AppConfig),
{
    let mut config = load_config()?;
    updater(&mut config);
    save_config(&config)?;
    Ok(config)
}

/// Get the data directory path.
pub fn get_data_directory() -> Result<PathBuf, String> {
    get_data_dir()
}
