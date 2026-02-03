//! Close Antigravity process functionality.

use std::process::Command;

#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

use super::detection::is_antigravity_running;
use super::lifecycle_utils::{
    force_kill_remaining, is_helper_by_name_or_args, wait_for_graceful_exit,
};
use super::pid_collection::get_antigravity_pids;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub fn close_antigravity(timeout_secs: u64) -> Result<(), String> {
    crate::modules::logger::log_info("Closing Antigravity...");

    #[cfg(target_os = "windows")]
    {
        let pids = get_antigravity_pids();
        if !pids.is_empty() {
            crate::modules::logger::log_info(&format!(
                "Precisely closing {} identified processes on Windows...",
                pids.len()
            ));
            for pid in pids {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .creation_flags(0x08000000)
                    .output();
            }
            thread::sleep(Duration::from_millis(200));
        }
    }

    #[cfg(target_os = "macos")]
    {
        close_macos(timeout_secs)?;
    }

    #[cfg(target_os = "linux")]
    {
        close_linux(timeout_secs)?;
    }

    if is_antigravity_running() {
        return Err("Cannot close Antigravity process, please close manually".to_string());
    }

    crate::modules::logger::log_info("Antigravity closed successfully");
    Ok(())
}

#[cfg(target_os = "macos")]
fn close_macos(timeout_secs: u64) -> Result<(), String> {
    use sysinfo::System;

    let pids = get_antigravity_pids();
    if pids.is_empty() {
        crate::modules::logger::log_info("Antigravity not running, no need to close");
        return Ok(());
    }

    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let manual_path = crate::modules::config::load_config()
        .ok()
        .and_then(|c| c.antigravity_executable)
        .and_then(|p| std::path::PathBuf::from(p).canonicalize().ok());

    let main_pid = find_main_process_macos(&system, &pids, &manual_path);

    if let Some(pid) = main_pid {
        crate::modules::logger::log_info(&format!("Sending SIGTERM to main process PID: {}", pid));
        let output = Command::new("kill")
            .args(["-15", &pid.to_string()])
            .output();
        if let Ok(result) = output {
            if !result.status.success() {
                let error = String::from_utf8_lossy(&result.stderr);
                crate::modules::logger::log_warn(&format!(
                    "Main process SIGTERM failed: {}",
                    error
                ));
            }
        }
    } else {
        crate::modules::logger::log_warn("No main process identified, sending SIGTERM to all");
        for pid in &pids {
            let _ = Command::new("kill")
                .args(["-15", &pid.to_string()])
                .output();
        }
    }

    wait_for_graceful_exit(timeout_secs)?;
    force_kill_remaining()
}

#[cfg(target_os = "macos")]
fn find_main_process_macos(
    system: &sysinfo::System,
    pids: &[u32],
    manual_path: &Option<std::path::PathBuf>,
) -> Option<u32> {
    let mut main_pid = None;

    crate::modules::logger::log_info("Analyzing process list to identify main process:");
    for pid_u32 in pids {
        let pid = sysinfo::Pid::from_u32(*pid_u32);
        if let Some(process) = system.process(pid) {
            let name = process.name().to_string_lossy();
            let args = process.cmd();
            let args_str = args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<String>>()
                .join(" ");

            crate::modules::logger::log_info(&format!(
                " - PID: {} | Name: {} | Args: {}",
                pid_u32, name, args_str
            ));

            if let (Some(ref m_path), Some(p_exe)) = (manual_path, process.exe()) {
                if let Ok(p_path) = p_exe.canonicalize() {
                    let m_path_str = m_path.to_string_lossy();
                    let p_path_str = p_path.to_string_lossy();
                    if let (Some(m_idx), Some(p_idx)) =
                        (m_path_str.find(".app"), p_path_str.find(".app"))
                    {
                        if m_path_str[..m_idx + 4] == p_path_str[..p_idx + 4] {
                            let is_helper =
                                is_helper_by_name_or_args(&name.to_lowercase(), &args_str);
                            if !is_helper {
                                main_pid = Some(*pid_u32);
                                crate::modules::logger::log_info(
                                    "   => Identified as main process (manual path match)",
                                );
                                break;
                            }
                        }
                    }
                }
            }

            let is_helper = is_helper_by_name_or_args(&name.to_lowercase(), &args_str);
            if !is_helper && main_pid.is_none() {
                main_pid = Some(*pid_u32);
                crate::modules::logger::log_info(
                    "   => Identified as main process (feature analysis)",
                );
            } else if is_helper {
                crate::modules::logger::log_info("   => Identified as helper process");
            }
        }
    }

    main_pid
}

#[cfg(target_os = "linux")]
fn close_linux(timeout_secs: u64) -> Result<(), String> {
    use sysinfo::System;

    let pids = get_antigravity_pids();
    if pids.is_empty() {
        crate::modules::logger::log_info("No Antigravity processes found to close");
        return Ok(());
    }

    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let manual_path = crate::modules::config::load_config()
        .ok()
        .and_then(|c| c.antigravity_executable)
        .and_then(|p| std::path::PathBuf::from(p).canonicalize().ok());

    let main_pid = find_main_process_linux(&system, &pids, &manual_path);

    if let Some(pid) = main_pid {
        crate::modules::logger::log_info(&format!(
            "Attempting graceful close of main process {} (SIGTERM)",
            pid
        ));
        let _ = Command::new("kill")
            .args(["-15", &pid.to_string()])
            .output();
    } else {
        crate::modules::logger::log_warn(
            "No main Linux process identified, sending SIGTERM to all",
        );
        for pid in &pids {
            let _ = Command::new("kill")
                .args(["-15", &pid.to_string()])
                .output();
        }
    }

    wait_for_graceful_exit(timeout_secs)?;
    force_kill_remaining()
}

#[cfg(target_os = "linux")]
fn find_main_process_linux(
    system: &sysinfo::System,
    pids: &[u32],
    manual_path: &Option<std::path::PathBuf>,
) -> Option<u32> {
    let mut main_pid = None;

    crate::modules::logger::log_info("Analyzing Linux process list to identify main process:");
    for pid_u32 in pids {
        let pid = sysinfo::Pid::from_u32(*pid_u32);
        if let Some(process) = system.process(pid) {
            let name = process.name().to_string_lossy().to_lowercase();
            let args = process.cmd();
            let args_str = args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<String>>()
                .join(" ");

            crate::modules::logger::log_info(&format!(
                " - PID: {} | Name: {} | Args: {}",
                pid_u32, name, args_str
            ));

            if let (Some(ref m_path), Some(p_exe)) = (manual_path, process.exe()) {
                if let Ok(p_path) = p_exe.canonicalize() {
                    if &p_path == m_path {
                        let is_helper = is_helper_by_name_or_args(&name, &args_str);
                        if !is_helper {
                            main_pid = Some(*pid_u32);
                            crate::modules::logger::log_info(
                                "   => Identified as main process (manual path match)",
                            );
                            break;
                        }
                    }
                }
            }

            let is_helper = is_helper_by_name_or_args(&name, &args_str);
            if !is_helper && main_pid.is_none() {
                main_pid = Some(*pid_u32);
                crate::modules::logger::log_info(
                    "   => Identified as main process (feature analysis)",
                );
            } else if is_helper {
                crate::modules::logger::log_info("   => Identified as helper process");
            }
        }
    }

    main_pid
}
