// Autostart Commands
use tauri_plugin_autostart::ManagerExt;

use antigravity_core::modules::logger;

#[tauri::command]
pub async fn toggle_auto_launch(app: tauri::AppHandle, enable: bool) -> Result<(), String> {
    let manager = app.autolaunch();

    if enable {
        manager.enable().map_err(|e| e.to_string())?;
        logger::log_info("Autostart enabled");
    } else {
        manager.disable().map_err(|e| e.to_string())?;
        logger::log_info("Autostart disabled");
    }

    Ok(())
}

#[tauri::command]
pub async fn is_auto_launch_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    let manager = app.autolaunch();
    manager.is_enabled().map_err(|e| e.to_string())
}
