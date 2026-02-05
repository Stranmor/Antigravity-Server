//! Antigravity Manager - Leptos Frontend Library

// Leptos reactive patterns require these
#![allow(
    clippy::clone_on_copy,
    reason = "Leptos signals are Copy but .clone() is idiomatic for clarity in closures"
)]
#![allow(clippy::unit_arg, reason = "Leptos callbacks often pass () explicitly")]

// Used only in bin target (main.rs), acknowledged here for lib crate
use console_error_panic_hook as _;
use console_log as _;

pub(crate) mod api;
pub(crate) mod api_models;
pub mod app;
pub(crate) mod components;
pub(crate) mod formatters;
pub(crate) mod pages;
