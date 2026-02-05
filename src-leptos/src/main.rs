//! Antigravity Manager - Leptos Frontend
//!
//! This is the Leptos-based frontend that runs inside Tauri WebView.
//! All business logic is handled by the existing Tauri backend via IPC.

// Dependencies used in lib.rs submodules, acknowledged here for bin target
use antigravity_types as _;
use chrono as _;
use gloo_timers as _;
use leptos_router as _;
use serde as _;
use serde_json as _;
use serde_wasm_bindgen as _;
use wasm_bindgen as _;
use wasm_bindgen_futures as _;
use web_sys as _;

use antigravity_leptos::app::App;
use leptos::prelude::*;

fn main() {
    // Initialize panic hook for better error messages
    console_error_panic_hook::set_once();

    // Initialize logging (ignore error if already initialized)
    drop(console_log::init_with_level(log::Level::Debug));

    log::info!("Antigravity Manager (Leptos) starting...");

    // Mount the app
    mount_to_body(App);
}
