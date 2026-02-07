use super::state::{BlockType, StreamingState};
use bytes::Bytes;
use serde_json::json;

use crate::proxy::mappers::claude::models::Usage;
use crate::proxy::mappers::claude::token_scaling::to_claude_usage;

impl StreamingState {
    pub fn emit_message_start(&mut self, raw_json: &serde_json::Value) -> Bytes {
        if self.message_start_sent {
            return Bytes::new();
        }

        let usage = raw_json
            .get("usageMetadata")
            .and_then(|u| {
                serde_json::from_value::<super::super::gemini_models::UsageMetadata>(u.clone()).ok()
            })
            .map(|u| to_claude_usage(&u, self.scaling_enabled, self.context_limit))
            .unwrap_or(Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                server_tool_use: None,
            });

        let message = json!({
            "id": raw_json.get("responseId")
                .and_then(|v| v.as_str())
                .unwrap_or("msg_unknown"),
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": raw_json.get("modelVersion")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "stop_reason": null,
            "stop_sequence": null,
            "usage": usage,
        });

        if let Some(m) = raw_json.get("modelVersion").and_then(|v| v.as_str()) {
            self.model_name = Some(m.to_string());
        }

        let result = self.emit(
            "message_start",
            json!({
                "type": "message_start",
                "message": message
            }),
        );

        self.message_start_sent = true;
        result
    }

    #[allow(
        dead_code,
        reason = "error recovery API, will be wired when stream corruption handling is enabled"
    )]
    pub fn handle_parse_error(&mut self, raw_data: &str) -> Vec<Bytes> {
        let mut chunks = Vec::new();

        self.parse_error_count += 1;

        tracing::warn!(
            "[SSE-Parser] Parse error #{} occurred. Raw data length: {} bytes",
            self.parse_error_count,
            raw_data.len()
        );

        if self.block_type != BlockType::None {
            self.last_valid_state = Some(self.block_type);
            chunks.extend(self.end_block());
        }

        #[cfg(debug_assertions)]
        {
            let preview = if raw_data.len() > 100 {
                format!("{}...", &raw_data[..100])
            } else {
                raw_data.to_string()
            };
            tracing::debug!("[SSE-Parser] Failed chunk preview: {}", preview);
        }

        if self.parse_error_count > 3 {
            tracing::error!(
                "[SSE-Parser] High error rate detected ({} errors). Stream may be corrupted.",
                self.parse_error_count
            );

            chunks.push(self.emit(
                "error",
                json!({
                    "type": "error",
                    "error": {
                        "type": "network_error",
                        "message": "Network connection unstable.",
                        "code": "stream_decode_error",
                        "details": {
                            "error_count": self.parse_error_count,
                            "suggestion": "Check network connection or retry."
                        }
                    }
                }),
            ));
        }

        chunks
    }

    #[allow(dead_code, reason = "error recovery API, paired with handle_parse_error")]
    pub fn reset_error_state(&mut self) {
        self.parse_error_count = 0;
        self.last_valid_state = None;
    }

    #[allow(dead_code, reason = "error recovery API, paired with handle_parse_error")]
    pub fn get_error_count(&self) -> usize {
        self.parse_error_count
    }
}
