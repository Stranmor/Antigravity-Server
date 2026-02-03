//! HTTP API bindings for Leptos
//!
//! This module provides type-safe wrappers for calling the antigravity-server REST API.
//! Replaces Tauri IPC for the headless server architecture.

mod accounts;
mod config;
mod proxy;
mod system;

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

const API_BASE: &str = "/api";

/// Make a GET request to the API
pub async fn api_get<R: DeserializeOwned>(endpoint: &str) -> Result<R, String> {
    let url = format!("{}{}", API_BASE, endpoint);

    let opts = RequestInit::new();
    opts.set_method("GET");

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set headers: {:?}", e))?;

    let window = web_sys::window().ok_or("No window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Response is not a Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("JSON parse failed: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("JSON future failed: {:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Deserialize failed: {}", e))
}

/// Make a POST request to the API
pub async fn api_post<A: Serialize, R: DeserializeOwned>(
    endpoint: &str,
    body: &A,
) -> Result<R, String> {
    let url = format!("{}{}", API_BASE, endpoint);

    let body_str =
        serde_json::to_string(body).map_err(|e| format!("Failed to serialize body: {}", e))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set headers: {:?}", e))?;

    let window = web_sys::window().ok_or("No window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Response is not a Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("JSON parse failed: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("JSON future failed: {:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Deserialize failed: {}", e))
}

// Re-export command wrappers
pub mod commands {
    pub use super::accounts::*;
    pub use super::config::*;
    pub use super::proxy::*;
    pub use super::system::*;
}
