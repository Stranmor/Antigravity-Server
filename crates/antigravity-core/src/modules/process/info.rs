//! Process info extraction for Antigravity.

use sysinfo::System;

use super::detection::get_current_exe_path;

fn get_process_info() -> (Option<std::path::PathBuf>, Option<Vec<String>>) {
    let mut system = System::new_all();
    system.refresh_all();

    let current_exe = get_current_exe_path();
    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        if let (Some(ref my_path), Some(p_exe)) = (&current_exe, process.exe()) {
            if let Ok(p_path) = p_exe.canonicalize() {
                if my_path == &p_path {
                    continue;
                }
            }
        }

        let name = process.name().to_string_lossy().to_lowercase();

        if let Some(exe) = process.exe() {
            let mut args = process.cmd().iter();
            let exe_path = args
                .next()
                .map_or(exe.to_string_lossy(), |arg| arg.to_string_lossy())
                .to_lowercase();

            let args =
                args.map(|arg| arg.to_string_lossy().to_lowercase()).collect::<Vec<String>>();

            let args_str = args.join(" ");

            let is_helper = args_str.contains("--type=")
                || args_str.contains("node-ipc")
                || args_str.contains("nodeipc")
                || args_str.contains("max-old-space-size")
                || args_str.contains("node_modules")
                || name.contains("helper")
                || name.contains("plugin")
                || name.contains("renderer")
                || name.contains("gpu")
                || name.contains("crashpad")
                || name.contains("utility")
                || name.contains("audio")
                || name.contains("sandbox")
                || exe_path.contains("crashpad");

            let path = Some(exe.to_path_buf());
            let args = Some(args);

            #[cfg(target_os = "macos")]
            {
                if exe_path.contains("antigravity.app")
                    && !is_helper
                    && !exe_path.contains("frameworks")
                {
                    if let Some(app_idx) = exe_path.find(".app") {
                        let app_path_str = &exe.to_string_lossy()[..app_idx + 4];
                        let path = Some(std::path::PathBuf::from(app_path_str));
                        return (path, args);
                    }
                    return (path, args);
                }
            }

            #[cfg(target_os = "windows")]
            {
                if name == "antigravity.exe" && !is_helper {
                    return (path, args);
                }
            }

            #[cfg(target_os = "linux")]
            {
                if (name == "antigravity" || exe_path.contains("/antigravity"))
                    && !name.contains("tools")
                    && !is_helper
                {
                    return (path, args);
                }
            }
        }
    }
    (None, None)
}

pub fn get_path_from_running_process() -> Option<std::path::PathBuf> {
    let (path, _) = get_process_info();
    path
}

pub fn get_args_from_running_process() -> Option<Vec<String>> {
    let (_, args) = get_process_info();
    args
}

pub fn get_user_data_dir_from_process() -> Option<std::path::PathBuf> {
    if let Ok(config) = crate::modules::config::load_config() {
        if let Some(args) = config.antigravity_args {
            if let Some(path) = extract_user_data_dir(&args) {
                return Some(path);
            }
        }
    }

    if let Some(args) = get_args_from_running_process() {
        if let Some(path) = extract_user_data_dir(&args) {
            return Some(path);
        }
    }

    None
}

fn extract_user_data_dir(args: &[String]) -> Option<std::path::PathBuf> {
    for i in 0..args.len() {
        if args[i] == "--user-data-dir" && i + 1 < args.len() {
            let path = std::path::PathBuf::from(&args[i + 1]);
            if path.exists() {
                return Some(path);
            }
        } else if args[i].starts_with("--user-data-dir=") {
            let parts: Vec<&str> = args[i].splitn(2, '=').collect();
            if parts.len() == 2 {
                let path = std::path::PathBuf::from(parts[1]);
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }
    None
}
