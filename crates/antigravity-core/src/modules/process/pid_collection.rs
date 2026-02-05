//! PID collection for Antigravity processes.
//!
//! Collects PIDs of all Antigravity processes including main and helper processes.

use sysinfo::System;

use super::detection::{get_current_exe_path, is_helper_process};

#[cfg(target_os = "linux")]
/// Get current process and all direct relatives (ancestors + descendants) PID set.
pub(crate) fn get_self_family_pids(system: &System) -> std::collections::HashSet<u32> {
    let current_pid = std::process::id();
    let mut family_pids = std::collections::HashSet::new();
    family_pids.insert(current_pid);

    // Find all ancestors (prevents killing launcher)
    let mut next_pid = current_pid;
    for _ in 0..10 {
        let pid_val = sysinfo::Pid::from_u32(next_pid);
        if let Some(process) = system.process(pid_val) {
            if let Some(parent) = process.parent() {
                let parent_id = parent.as_u32();
                if !family_pids.insert(parent_id) {
                    break;
                }
                next_pid = parent_id;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Find all descendants via BFS
    let mut adj: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for (pid, process) in system.processes() {
        if let Some(parent) = process.parent() {
            adj.entry(parent.as_u32()).or_default().push(pid.as_u32());
        }
    }

    let mut queue = std::collections::VecDeque::new();
    queue.push_back(current_pid);

    while let Some(pid) = queue.pop_front() {
        if let Some(children) = adj.get(&pid) {
            for &child in children {
                if family_pids.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    family_pids
}

/// Get all Antigravity process PIDs (including main and helper processes).
pub(crate) fn get_antigravity_pids() -> Vec<u32> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);

    #[cfg(target_os = "linux")]
    let family_pids = get_self_family_pids(&system);

    let mut pids = Vec::new();
    let current_pid = std::process::id();
    let current_exe = get_current_exe_path();

    let manual_path = crate::modules::config::load_config()
        .ok()
        .and_then(|c| c.antigravity_executable)
        .and_then(|p| std::path::PathBuf::from(p).canonicalize().ok());

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

        let _name = process.name().to_string_lossy().to_lowercase();

        #[cfg(target_os = "linux")]
        {
            if family_pids.contains(&pid_u32) {
                continue;
            }
            if _name.contains("tools") {
                continue;
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            if pid_u32 == current_pid {
                continue;
            }
        }

        // Check manual path match
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
                            let is_helper_by_name = _name.contains("helper")
                                || _name.contains("plugin")
                                || _name.contains("renderer")
                                || _name.contains("gpu")
                                || _name.contains("crashpad")
                                || _name.contains("utility")
                                || _name.contains("audio")
                                || _name.contains("sandbox");
                            if !is_helper_by_args && !is_helper_by_name {
                                pids.push(pid_u32);
                                continue;
                            }
                        }
                    }
                }

                #[cfg(not(target_os = "macos"))]
                if m_path == &p_path {
                    pids.push(pid_u32);
                    continue;
                }
            }
        }

        let exe_path = process.exe().and_then(|p| p.to_str()).unwrap_or("").to_lowercase();

        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = is_helper_process(&_name, &args_str, &exe_path);

        #[cfg(target_os = "macos")]
        {
            if exe_path.contains("antigravity.app") && !is_helper {
                pids.push(pid_u32);
            }
        }

        #[cfg(target_os = "windows")]
        {
            let name = process.name().to_string_lossy().to_lowercase();
            if name == "antigravity.exe" && !is_helper {
                pids.push(pid_u32);
            }
        }

        #[cfg(target_os = "linux")]
        {
            let name = process.name().to_string_lossy().to_lowercase();
            if (name == "antigravity" || exe_path.contains("/antigravity"))
                && !name.contains("tools")
                && !is_helper
            {
                pids.push(pid_u32);
            }
        }
    }

    if !pids.is_empty() {
        crate::modules::logger::log_info(&format!(
            "Found {} Antigravity processes: {:?}",
            pids.len(),
            pids
        ));
    }

    pids
}
