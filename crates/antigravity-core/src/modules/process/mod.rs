//! Process management for Antigravity.
//!
//! Provides detection, lifecycle control, and path discovery for Antigravity processes.

mod close;
mod detection;
mod info;
mod lifecycle_utils;
mod paths;
mod pid_collection;
mod start;

pub use close::close_antigravity;
pub use detection::is_antigravity_running;
pub use info::{
    get_args_from_running_process, get_path_from_running_process, get_user_data_dir_from_process,
};
pub use paths::get_antigravity_executable_path;
pub use start::start_antigravity;
