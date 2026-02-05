//! Streaming state management for Claude API SSE responses.
//!
//! This module handles the state machine for converting Gemini streaming
//! responses into Claude-compatible SSE events.

use crate::proxy::mappers::claude::models::Usage;
use crate::proxy::mappers::claude::streaming::signature_manager::SignatureManager;
use crate::proxy::mappers::claude::token_scaling::to_claude_usage;
use bytes::Bytes;
use serde_json::json;

/// Types of content blocks in a streaming response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    /// No active block.
    None,
    /// Text content block.
    Text,
    /// Thinking/reasoning block.
    Thinking,
    /// Function/tool call block.
    Function,
}

/// State machine for streaming Claude-compatible SSE responses.
///
/// Tracks the current block type, indices, and manages the conversion
/// of Gemini streaming responses to Claude SSE format.
pub struct StreamingState {
    /// Current block type being streamed.
    pub(super) block_type: BlockType,
    /// Current block index in the response.
    pub block_index: usize,
    /// Whether message_start event has been sent.
    pub message_start_sent: bool,
    /// Whether message_stop event has been sent.
    pub message_stop_sent: bool,
    /// Whether a tool was used in this response.
    pub(super) used_tool: bool,
    /// Manager for thinking block signatures.
    pub(super) signatures: SignatureManager,
    /// Trailing signature to emit at block end.
    pub(super) trailing_signature: Option<String>,
    /// Web search query if grounding is used.
    pub web_search_query: Option<String>,
    /// Grounding chunks from web search.
    pub grounding_chunks: Option<Vec<serde_json::Value>>,
    /// Count of parse errors for error recovery.
    #[allow(dead_code)]
    pub(super) parse_error_count: usize,
    /// Last valid block type before error.
    #[allow(dead_code)]
    pub(super) last_valid_state: Option<BlockType>,
    /// Model name from response.
    pub model_name: Option<String>,
    /// Session ID for request tracking.
    pub session_id: Option<String>,
    /// Whether token scaling is enabled.
    pub scaling_enabled: bool,
    /// Context limit for token scaling.
    pub context_limit: u32,
    /// Buffer for MCP XML content.
    pub mcp_xml_buffer: String,
    /// Whether currently inside MCP XML block.
    pub in_mcp_xml: bool,
    /// Estimated token count for the response.
    pub estimated_tokens: Option<u32>,
}

impl Default for StreamingState {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingState {
    /// Creates a new streaming state with default values.
    pub fn new() -> Self {
        Self {
            block_type: BlockType::None,
            block_index: 0,
            message_start_sent: false,
            message_stop_sent: false,
            used_tool: false,
            signatures: SignatureManager::new(),
            trailing_signature: None,
            web_search_query: None,
            grounding_chunks: None,
            parse_error_count: 0,
            last_valid_state: None,
            model_name: None,
            session_id: None,
            scaling_enabled: false,
            context_limit: 1_048_576,
            mcp_xml_buffer: String::new(),
            in_mcp_xml: false,
            estimated_tokens: None,
        }
    }

    /// Emits an SSE event with the given type and data.
    pub fn emit(&self, event_type: &str, data: serde_json::Value) -> Bytes {
        let sse = format!(
            "event: {}\ndata: {}\n\n",
            event_type,
            serde_json::to_string(&data).unwrap_or_default()
        );
        Bytes::from(sse)
    }

    /// Emits the message_start event with usage information.
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

    /// Starts a new content block, closing any existing block first.
    pub fn start_block(
        &mut self,
        block_type: BlockType,
        content_block: serde_json::Value,
    ) -> Vec<Bytes> {
        let mut chunks = Vec::new();
        if self.block_type != BlockType::None {
            chunks.extend(self.end_block());
        }

        chunks.push(self.emit(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": self.block_index,
                "content_block": content_block
            }),
        ));

        self.block_type = block_type;
        chunks
    }

    /// Ends the current content block.
    pub fn end_block(&mut self) -> Vec<Bytes> {
        if self.block_type == BlockType::None {
            return vec![];
        }

        let mut chunks = Vec::new();

        if self.block_type == BlockType::Thinking && self.signatures.has_pending() {
            if let Some(signature) = self.signatures.consume() {
                chunks.push(self.emit_delta("signature_delta", json!({ "signature": signature })));
            }
        }

        chunks.push(self.emit(
            "content_block_stop",
            json!({
                "type": "content_block_stop",
                "index": self.block_index
            }),
        ));

        self.block_index += 1;
        self.block_type = BlockType::None;

        chunks
    }

    /// Emits a delta event for incremental content updates.
    pub fn emit_delta(&self, delta_type: &str, delta_content: serde_json::Value) -> Bytes {
        let mut delta = json!({ "type": delta_type });
        if let serde_json::Value::Object(map) = delta_content {
            for (k, v) in map {
                delta[k] = v;
            }
        }

        self.emit(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": self.block_index,
                "delta": delta
            }),
        )
    }

    /// Marks that a tool was used in this response.
    pub fn mark_tool_used(&mut self) {
        self.used_tool = true;
    }

    /// Returns the current block type.
    pub fn current_block_type(&self) -> BlockType {
        self.block_type
    }

    /// Returns the current block index.
    pub fn current_block_index(&self) -> usize {
        self.block_index
    }

    /// Stores a signature for later emission.
    pub fn store_signature(&mut self, signature: Option<String>) {
        self.signatures.store(signature);
    }

    /// Sets the trailing signature.
    pub fn set_trailing_signature(&mut self, signature: Option<String>) {
        self.trailing_signature = signature;
    }

    /// Checks if there is a trailing signature.
    pub fn has_trailing_signature(&self) -> bool {
        self.trailing_signature.is_some()
    }

    /// Takes and returns the trailing signature.
    pub fn take_trailing_signature(&mut self) -> Option<String> {
        self.trailing_signature.take()
    }

    /// Handles SSE parse errors with graceful recovery.
    #[allow(dead_code)]
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

    /// Resets the error state after recovery.
    #[allow(dead_code)]
    pub fn reset_error_state(&mut self) {
        self.parse_error_count = 0;
        self.last_valid_state = None;
    }

    /// Returns the current error count.
    #[allow(dead_code)]
    pub fn get_error_count(&self) -> usize {
        self.parse_error_count
    }
}
