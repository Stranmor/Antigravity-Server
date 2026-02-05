//! Process detection logic for Antigravity.
//!
//! Contains functions to detect if Antigravity is running and identify helper processes.

use sysinfo::System;

/// Get the canonicalized path of the current executable.
pub(crate) fn get_current_exe_path() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.canonicalize().ok())
}

/// Check if a process is a helper/auxiliary process based on name and args.
pub(crate) fn is_helper_process(name: &str, args_str: &str, exe_path: &str) -> bool {
    args_str.contains("--type=")
        || name.contains("helper")
        || name.contains("plugin")
        || name.contains("renderer")
        || name.contains("gpu")
        || name.contains("crashpad")
        || name.contains("utility")
        || name.contains("audio")
        || name.contains("sandbox")
        || exe_path.contains("crashpad")
}

/// Check if Antigravity is currently running.
pub fn is_antigravity_running() -> bool {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let current_exe = get_current_exe_path();
    let current_pid = std::process::id();

    // Load manual config path once (outside loop for performance)
    let manual_path = crate::modules::config::load_config()
        .ok()
        .and_then(|c| c.antigravity_executable)
        .and_then(|p| std::path::PathBuf::from(p).canonicalize().ok());

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process.exe().and_then(|p| p.to_str()).unwrap_or("").to_lowercase();

        // Exclude self path (handles case where manager is mistaken for Antigravity on Linux)
        if let (Some(ref my_path), Some(p_exe)) = (&current_exe, process.exe()) {
            if let Ok(p_path) = p_exe.canonicalize() {
                if my_path == &p_path {
                    continue;
                }
            }
        }

        // Check manual path match first
        if let (Some(ref m_path), Some(p_exe)) = (&manual_path, process.exe()) {
            if let Ok(p_path) = p_exe.canonicalize() {
                #[cfg(target_os = "macos")]
                {
                    let m_path_str = m_path.to_string_lossy();
                    let p_path_str = p_path.to_string_lossy();
                    if let (Some(m_idx), Some(p_idx)) =
                        (m_path_str.find(".app"), p_path_str.find(".app"))
                    {
                        if m_path_str[..m_idx + 4] == p_path_str[..p_idx + 4] {
                            let args = process.cmd();
                            let is_helper_by_args =
                                args.iter().any(|arg| arg.to_string_lossy().contains("--type="));
                            let is_helper_by_name = name.contains("helper")
                                || name.contains("plugin")
                                || name.contains("renderer")
                                || name.contains("gpu")
                                || name.contains("crashpad")
                                || name.contains("utility")
                                || name.contains("audio")
                                || name.contains("sandbox");
                            if !is_helper_by_args && !is_helper_by_name {
                                return true;
                            }
                        }
                    }
                }

                #[cfg(not(target_os = "macos"))]
                if m_path == &p_path {
                    return true;
                }
            }
        }

        // Generic helper process exclusion logic
        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = is_helper_process(&name, &args_str, &exe_path);

        #[cfg(target_os = "macos")]
        {
            if exe_path.contains("antigravity.app") && !is_helper {
                return true;
            }
        }

        #[cfg(target_os = "windows")]
        {
            if name == "antigravity.exe" && !is_helper {
                return true;
            }
        }

        #[cfg(target_os = "linux")]
        {
            if (name.contains("antigravity") || exe_path.contains("/antigravity"))
                && !name.contains("tools")
                && !is_helper
            {
                return true;
            }
        }
    }

    false
}
