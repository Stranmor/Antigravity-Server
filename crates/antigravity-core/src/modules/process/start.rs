//! Start Antigravity process functionality.

use std::process::Command;

/// Start Antigravity application.
pub fn start_antigravity() -> Result<(), String> {
    crate::modules::logger::log_info("Starting Antigravity...");

    let config = crate::modules::config::load_config().ok();
    let manual_path = config
        .as_ref()
        .and_then(|c| c.antigravity_executable.clone());
    let args = config.and_then(|c| c.antigravity_args.clone());

    if let Some(path_str) = manual_path {
        return start_from_manual_path(path_str, args);
    }

    start_from_default_location(args)
}

fn start_from_manual_path(path_str: String, args: Option<Vec<String>>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let (path_str, path) = {
        let mut path_str = path_str;
        let mut path = std::path::PathBuf::from(&path_str);

        if let Some(app_idx) = path_str.find(".app") {
            let corrected_app = &path_str[..app_idx + 4];
            if corrected_app != path_str {
                crate::modules::logger::log_info(&format!(
                    "Detected macOS path inside .app, auto-correcting to: {}",
                    corrected_app
                ));
                path_str = corrected_app.to_string();
                path = std::path::PathBuf::from(&path_str);
            }
        }
        (path_str, path)
    };

    #[cfg(not(target_os = "macos"))]
    let path = std::path::PathBuf::from(&path_str);

    if !path.exists() {
        crate::modules::logger::log_warn(&format!(
            "Manual config path does not exist: {}, falling back to auto-detection",
            path_str
        ));
        return start_from_default_location(args);
    }

    crate::modules::logger::log_info(&format!("Starting from manual config path: {}", path_str));

    #[cfg(target_os = "macos")]
    {
        if path_str.ends_with(".app") || path.is_dir() {
            let mut cmd = Command::new("open");
            cmd.arg("-a").arg(&path_str);
            if let Some(ref args) = args {
                for arg in args {
                    cmd.arg(arg);
                }
            }
            cmd.spawn()
                .map_err(|e| format!("Start failed (open): {}", e))?;
        } else {
            let mut cmd = Command::new(&path_str);
            if let Some(ref args) = args {
                for arg in args {
                    cmd.arg(arg);
                }
            }
            cmd.spawn()
                .map_err(|e| format!("Start failed (direct): {}", e))?;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut cmd = Command::new(&path_str);
        if let Some(ref args) = args {
            for arg in args {
                cmd.arg(arg);
            }
        }
        cmd.spawn().map_err(|e| format!("Start failed: {}", e))?;
    }

    Ok(())
}

fn start_from_default_location(args: Option<Vec<String>>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.args(["-a", "Antigravity"]);
        if let Some(ref args) = args {
            for arg in args {
                cmd.arg(arg);
            }
        }
        let output = cmd
            .output()
            .map_err(|e| format!("Cannot execute open command: {}", e))?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Start failed (open exited with {}): {}",
                output.status, error
            ));
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "antigravity://"]);
        if let Some(ref args) = args {
            for arg in args {
                cmd.arg(arg);
            }
        }
        let result = cmd.spawn();
        if result.is_err() {
            return Err("Start failed, please open Antigravity manually".to_string());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("antigravity");
        if let Some(ref args) = args {
            for arg in args {
                cmd.arg(arg);
            }
        }
        cmd.spawn().map_err(|e| format!("Start failed: {}", e))?;
    }

    Ok(())
}
