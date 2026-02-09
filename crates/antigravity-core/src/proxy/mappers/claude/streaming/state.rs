//! Streaming state management for Claude API SSE responses.
//!
//! This module handles the state machine for converting Gemini streaming
//! responses into Claude-compatible SSE events.

use crate::proxy::mappers::claude::streaming::signature_manager::SignatureManager;
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
    /// Accumulated thinking content for content-based signature caching.
    accumulated_thinking: String,
    /// Whether thinking was received in the response stream.
    pub(super) thinking_received: bool,
    /// Whether thinking was requested for this request.
    pub thinking_requested: bool,
    /// Whether a finish reason was received from upstream.
    received_finish_reason: bool,
    /// Whether the stream errored (network/decoding error from upstream).
    /// When true, emit_force_stop skips generating error/termination events
    /// because the error path in build_combined_stream already emitted them.
    pub stream_errored: bool,
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
            model_name: None,
            session_id: None,
            scaling_enabled: false,
            context_limit: 1_048_576,
            mcp_xml_buffer: String::new(),
            in_mcp_xml: false,
            estimated_tokens: None,
            accumulated_thinking: String::new(),
            thinking_received: false,
            thinking_requested: false,
            received_finish_reason: false,
            stream_errored: false,
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

    /// Marks that thinking content was received in the response.
    pub fn mark_thinking_received(&mut self) {
        self.thinking_received = true;
    }

    /// Sets whether thinking was requested for this request.
    pub fn set_thinking_requested(&mut self, requested: bool) {
        self.thinking_requested = requested;
    }

    /// Returns whether thinking was received.
    pub fn has_thinking_received(&self) -> bool {
        self.thinking_received
    }

    /// Marks that a finish reason was received from upstream.
    pub fn mark_finish_reason_received(&mut self) {
        self.received_finish_reason = true;
    }

    /// Returns whether a finish reason was received.
    pub fn has_finish_reason(&self) -> bool {
        self.received_finish_reason
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

    /// Sets block type to Thinking for synthetic signature blocks.
    pub fn set_block_type_thinking(&mut self) {
        self.block_type = BlockType::Thinking;
    }

    /// Accumulates thinking content for signature caching.
    pub fn accumulate_thinking(&mut self, text: &str) {
        const MAX_THINKING_SIZE: usize = 2 * 1024 * 1024; // 2MB
        if self.accumulated_thinking.len() < MAX_THINKING_SIZE {
            let remaining = MAX_THINKING_SIZE.saturating_sub(self.accumulated_thinking.len());
            if text.len() <= remaining {
                self.accumulated_thinking.push_str(text);
            } else {
                let end = text.floor_char_boundary(remaining);
                self.accumulated_thinking.push_str(&text[..end]);
            }
        }
    }

    /// Returns the accumulated thinking content and clears the buffer.
    pub fn get_accumulated_thinking(&mut self) -> String {
        std::mem::take(&mut self.accumulated_thinking)
    }
}
