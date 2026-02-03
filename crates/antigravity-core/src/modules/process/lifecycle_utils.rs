//! Shared utilities for process lifecycle management.

use std::process::Command;
use std::thread;
use std::time::Duration;

use super::detection::is_antigravity_running;
use super::pid_collection::get_antigravity_pids;

pub(crate) fn is_helper_by_name_or_args(name: &str, args_str: &str) -> bool {
    args_str.contains("--type=")
        || name.contains("helper")
        || name.contains("plugin")
        || name.contains("renderer")
        || name.contains("gpu")
        || name.contains("crashpad")
        || name.contains("utility")
        || name.contains("audio")
        || name.contains("sandbox")
        || name.contains("language_server")
}

pub(crate) fn wait_for_graceful_exit(timeout_secs: u64) -> Result<(), String> {
    let graceful_timeout = (timeout_secs * 7) / 10;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(graceful_timeout) {
        if !is_antigravity_running() {
            crate::modules::logger::log_info("All Antigravity processes closed gracefully");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    Ok(())
}

pub(crate) fn force_kill_remaining() -> Result<(), String> {
    if is_antigravity_running() {
        let remaining_pids = get_antigravity_pids();
        if !remaining_pids.is_empty() {
            crate::modules::logger::log_warn(&format!(
                "Graceful close timeout, force killing {} remaining processes (SIGKILL)",
                remaining_pids.len()
            ));
            for pid in &remaining_pids {
                let output = Command::new("kill").args(["-9", &pid.to_string()]).output();
                if let Ok(result) = output {
                    if !result.status.success() {
                        let error = String::from_utf8_lossy(&result.stderr);
                        if !error.contains("No such process") {
                            crate::modules::logger::log_error(&format!(
                                "SIGKILL process {} failed: {}",
                                pid, error
                            ));
                        }
                    }
                }
            }
            thread::sleep(Duration::from_secs(1));
        }

        if !is_antigravity_running() {
            crate::modules::logger::log_info("All processes exited after force cleanup");
        }
    } else {
        crate::modules::logger::log_info("All processes exited after SIGTERM");
    }
    Ok(())
}
