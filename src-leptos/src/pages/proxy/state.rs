//! Proxy page state signals

use crate::api::commands;
use crate::api_models::{Protocol, ProxyAuthMode, ZaiDispatchMode};
use leptos::prelude::*;
use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct ProxyState {
    // UI state
    pub loading: RwSignal<bool>,
    pub copied: RwSignal<Option<String>>,
    pub selected_protocol: RwSignal<Protocol>,
    pub selected_model: RwSignal<String>,
    pub message: RwSignal<Option<(String, bool)>>,

    // Config state
    pub port: RwSignal<u16>,
    pub timeout: RwSignal<u32>,
    pub auto_start: RwSignal<bool>,
    pub allow_lan: RwSignal<bool>,
    pub auth_mode: RwSignal<ProxyAuthMode>,
    pub api_key: RwSignal<String>,
    pub enable_logging: RwSignal<bool>,

    // Model mapping state
    pub custom_mappings: RwSignal<HashMap<String, String>>,
    pub new_mapping_from: RwSignal<String>,
    pub new_mapping_to: RwSignal<String>,

    // Scheduling state
    pub scheduling_mode: RwSignal<String>,
    pub sticky_session_ttl: RwSignal<u32>,

    // Z.ai state
    pub zai_expanded: RwSignal<bool>,
    pub zai_enabled: RwSignal<bool>,
    pub zai_base_url: RwSignal<String>,
    pub zai_api_key: RwSignal<String>,
    pub zai_dispatch_mode: RwSignal<ZaiDispatchMode>,
    pub zai_model_mapping: RwSignal<HashMap<String, String>>,

    // Section expansion state
    pub routing_expanded: RwSignal<bool>,
    pub scheduling_expanded: RwSignal<bool>,

    // Test mapping state
    pub test_mapping_expanded: RwSignal<bool>,
    pub test_model_input: RwSignal<String>,
    pub test_result: RwSignal<Option<commands::ModelDetectResponse>>,
    pub test_loading: RwSignal<bool>,
}

impl ProxyState {
    pub(crate) fn new() -> Self {
        Self {
            loading: RwSignal::new(false),
            copied: RwSignal::new(None),
            selected_protocol: RwSignal::new(Protocol::OpenAI),
            selected_model: RwSignal::new("gemini-3-flash".to_string()),
            message: RwSignal::new(None),

            port: RwSignal::new(8045u16),
            timeout: RwSignal::new(120u32),
            auto_start: RwSignal::new(false),
            allow_lan: RwSignal::new(false),
            auth_mode: RwSignal::new(ProxyAuthMode::default()),
            api_key: RwSignal::new(String::new()),
            enable_logging: RwSignal::new(true),

            custom_mappings: RwSignal::new(HashMap::new()),
            new_mapping_from: RwSignal::new(String::new()),
            new_mapping_to: RwSignal::new(String::new()),

            scheduling_mode: RwSignal::new("balance".to_string()),
            sticky_session_ttl: RwSignal::new(3600u32),

            zai_expanded: RwSignal::new(false),
            zai_enabled: RwSignal::new(false),
            zai_base_url: RwSignal::new(String::new()),
            zai_api_key: RwSignal::new(String::new()),
            zai_dispatch_mode: RwSignal::new(ZaiDispatchMode::default()),
            zai_model_mapping: RwSignal::new(HashMap::new()),

            routing_expanded: RwSignal::new(true),
            scheduling_expanded: RwSignal::new(false),

            test_mapping_expanded: RwSignal::new(false),
            test_model_input: RwSignal::new(String::new()),
            test_result: RwSignal::new(None),
            test_loading: RwSignal::new(false),
        }
    }
}

impl Default for ProxyState {
    fn default() -> Self {
        Self::new()
    }
}
