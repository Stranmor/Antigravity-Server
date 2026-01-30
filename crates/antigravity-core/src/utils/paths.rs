use std::fs;
use std::path::PathBuf;

const DATA_DIR: &str = ".antigravity_tools";

/// Get data directory path.
///
/// Priority:
/// 1. `ANTIGRAVITY_DATA_DIR` environment variable (for container deployments)
/// 2. `~/.antigravity_tools` (default for desktop usage)
pub fn get_data_dir() -> Result<PathBuf, String> {
    let data_dir = if let Ok(custom_dir) = std::env::var("ANTIGRAVITY_DATA_DIR") {
        std::path::PathBuf::from(custom_dir)
    } else {
        let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
        home.join(DATA_DIR)
    };

    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
    }

    Ok(data_dir)
}
