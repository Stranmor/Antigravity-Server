//! SSOP (Shell/Search Output Parser) - Detects embedded JSON commands in text output
//!
//! When the model outputs shell commands as JSON text instead of proper function calls,
//! this module detects and converts them to proper tool call events.

use bytes::Bytes;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Result of SSOP detection - contains events to emit
pub struct SsopDetectionResult {
    pub events: Vec<Bytes>,
}

/// Detect embedded shell commands in full_content and generate appropriate events
pub fn detect_and_emit_ssop_events(full_content: &str) -> SsopDetectionResult {
    let mut events = Vec::new();

    let mut detected_cmd_val = None;
    let mut detected_cmd_type = "unknown";

    // Find all potential JSON start/end indices
    let chars: Vec<char> = full_content.chars().collect();
    let mut depth = 0;
    let mut start_idx = 0;

    // Scan for top-level JSON objects
    for (i, c) in chars.iter().enumerate() {
        if *c == '{' {
            if depth == 0 {
                start_idx = i;
            }
            depth += 1;
        } else if *c == '}' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                // Found a potential JSON object block [start_idx..=i]
                let json_str: String = chars[start_idx..=i].iter().collect();
                if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                    if let Some(cmd_val) = val.get("command") {
                        // Case 1: "command": ["shell", ...] or ["ls", ...]
                        if let Some(arr) = cmd_val.as_array() {
                            if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                                if matches!(
                                    first,
                                    "shell" | "powershell" | "cmd" | "ls" | "git" | "echo"
                                ) {
                                    detected_cmd_type = "shell";
                                    detected_cmd_val = Some(cmd_val.clone());
                                }
                            }
                        }
                        // Case 2: "command": "shell" (String) and "args": { "command": "..." }
                        else if let Some(cmd_str) = cmd_val.as_str() {
                            if cmd_str == "shell" || cmd_str == "local_shell" {
                                if let Some(args) = val
                                    .get("args")
                                    .or(val.get("arguments"))
                                    .or(val.get("params"))
                                {
                                    if let Some(inner_cmd) = args
                                        .get("command")
                                        .or(args.get("code"))
                                        .or(args.get("argument"))
                                    {
                                        if let Some(inner_cmd_str) = inner_cmd.as_str() {
                                            detected_cmd_type = "shell";
                                            detected_cmd_val = Some(json!([inner_cmd_str]));
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Fallback for malformed JSON (e.g. unescaped quotes)
                    if let Some(result) = try_recover_malformed_json(&json_str) {
                        detected_cmd_type = "shell";
                        detected_cmd_val = Some(result);
                    }
                }
            }
        }
    }

    if let Some(cmd_val) = detected_cmd_val {
        if detected_cmd_type == "shell" {
            let shell_events = generate_shell_events(&cmd_val);
            events.extend(shell_events);
        }
    }

    SsopDetectionResult { events }
}

/// Try to recover command from malformed JSON
fn try_recover_malformed_json(json_str: &str) -> Option<Value> {
    if (json_str.contains("\"command\": \"shell\"")
        || json_str.contains("\"command\": \"local_shell\""))
        && (json_str.contains("\"argument\":") || json_str.contains("\"code\":"))
    {
        let keys = ["\"argument\":", "\"code\":", "\"command\":"];
        for key in keys {
            if let Some(pos) = json_str.find(key) {
                let slice_start = pos + key.len();
                if let Some(slice_after_key) = json_str.get(slice_start..) {
                    if let Some(quote_idx) = slice_after_key.find('"') {
                        let val_start_abs = slice_start + quote_idx + 1;
                        if let Some(last_quote_idx) = json_str.rfind('"') {
                            if last_quote_idx > val_start_abs {
                                if let Some(raw_cmd) = json_str.get(val_start_abs..last_quote_idx) {
                                    tracing::debug!(
                                        "SSOP: Recovered malformed JSON command: {}",
                                        raw_cmd
                                    );
                                    return Some(json!([raw_cmd]));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Generate shell call events from detected command value
fn generate_shell_events(cmd_val: &Value) -> Vec<Bytes> {
    let mut events = Vec::new();

    let mut hasher = DefaultHasher::new();
    "ssop_shell_call".hash(&mut hasher);
    serde_json::to_string(cmd_val)
        .unwrap_or_default()
        .hash(&mut hasher);
    let call_id = format!("call_{:x}", hasher.finish());

    let mut cmd_vec: Vec<String> = cmd_val
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    // Strip "shell" or "local_shell" label if present
    if !cmd_vec.is_empty() && (cmd_vec[0] == "shell" || cmd_vec[0] == "local_shell") {
        cmd_vec.remove(0);
    }

    let final_cmd_vec = build_final_command(cmd_vec);

    tracing::debug!(
        "SSOP: Detected Shell Command in Text, Injecting Event: {:?}",
        final_cmd_vec
    );

    // Emit added event
    let item_added_ev = json!({
        "type": "response.output_item.added",
        "item": {
            "type": "local_shell_call",
            "status": "in_progress",
            "call_id": &call_id,
            "action": {
                "type": "exec",
                "command": final_cmd_vec
            }
        }
    });
    events.push(Bytes::from(format!(
        "data: {}\n\n",
        serde_json::to_string(&item_added_ev).unwrap_or_default()
    )));

    // Emit done event
    let item_done_ev = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "local_shell_call",
            "status": "in_progress",
            "call_id": &call_id,
            "action": {
                "type": "exec",
                "command": final_cmd_vec
            }
        }
    });
    events.push(Bytes::from(format!(
        "data: {}\n\n",
        serde_json::to_string(&item_done_ev).unwrap_or_default()
    )));

    events
}

/// Build final command vector with proper shell wrapping
fn build_final_command(cmd_vec: Vec<String>) -> Vec<String> {
    if cmd_vec.is_empty() {
        return vec![
            "powershell".to_string(),
            "-Command".to_string(),
            "echo 'Empty command'".to_string(),
        ];
    }

    if matches!(
        cmd_vec[0].as_str(),
        "powershell" | "cmd" | "git" | "python" | "node"
    ) {
        return cmd_vec;
    }

    // Wrap generic commands in powershell with EncodedCommand
    let raw_cmd = cmd_vec.join(" ");
    let joined = format!("& {{ {} }} | Out-String", raw_cmd);
    let utf16: Vec<u16> = joined.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for c in utf16 {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    vec!["powershell".to_string(), "-EncodedCommand".to_string(), b64]
}
