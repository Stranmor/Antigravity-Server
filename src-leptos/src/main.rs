//! Antigravity Manager - Leptos Frontend
//!
//! This is the Leptos-based frontend that runs inside Tauri WebView.
//! All business logic is handled by the existing Tauri backend via IPC.

use antigravity_leptos::app::App;
use leptos::prelude::*;

fn main() {
    // Initialize panic hook for better error messages
    console_error_panic_hook::set_once();

    // Initialize logging
    console_log::init_with_level(log::Level::Debug).expect("Failed to init logger");

    log::info!("Antigravity Manager (Leptos) starting...");

    // Mount the app
    mount_to_body(App);
}
