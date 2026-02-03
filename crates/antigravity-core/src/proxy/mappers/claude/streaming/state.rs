use crate::proxy::mappers::claude::models::*;
use crate::proxy::mappers::claude::token_scaling::to_claude_usage;
use bytes::Bytes;
use serde_json::json;

use super::signature_manager::SignatureManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    None,
    Text,
    Thinking,
    Function,
}

pub struct StreamingState {
    pub(super) block_type: BlockType,
    pub block_index: usize,
    pub message_start_sent: bool,
    pub message_stop_sent: bool,
    pub(super) used_tool: bool,
    pub(super) signatures: SignatureManager,
    pub(super) trailing_signature: Option<String>,
    pub web_search_query: Option<String>,
    pub grounding_chunks: Option<Vec<serde_json::Value>>,
    #[allow(dead_code)]
    pub(super) parse_error_count: usize,
    #[allow(dead_code)]
    pub(super) last_valid_state: Option<BlockType>,
    pub model_name: Option<String>,
    pub session_id: Option<String>,
    pub scaling_enabled: bool,
    pub context_limit: u32,
    pub mcp_xml_buffer: String,
    pub in_mcp_xml: bool,
    pub estimated_tokens: Option<u32>,
}

impl Default for StreamingState {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingState {
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
            context_limit: 1_048_576, // Default to 1M
            mcp_xml_buffer: String::new(),
            in_mcp_xml: false,
            estimated_tokens: None,
        }
    }

    pub fn emit(&self, event_type: &str, data: serde_json::Value) -> Bytes {
        let sse = format!(
            "event: {}\ndata: {}\n\n",
            event_type,
            serde_json::to_string(&data).unwrap_or_default()
        );
        Bytes::from(sse)
    }

    pub fn emit_message_start(&mut self, raw_json: &serde_json::Value) -> Bytes {
        if self.message_start_sent {
            return Bytes::new();
        }

        // [FIX] Always include usage field - clients (e.g., OpenCode) expect message.usage to be an object
        // If usageMetadata is missing, use default values (0 tokens)
        let usage = raw_json
            .get("usageMetadata")
            .and_then(|u| serde_json::from_value::<UsageMetadata>(u.clone()).ok())
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

        // Capture model name for signature cache
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

    pub fn end_block(&mut self) -> Vec<Bytes> {
        if self.block_type == BlockType::None {
            return vec![];
        }

        let mut chunks = Vec::new();

        // Emit pending signature when Thinking block ends
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

    pub fn mark_tool_used(&mut self) {
        self.used_tool = true;
    }

    pub fn current_block_type(&self) -> BlockType {
        self.block_type
    }
    pub fn current_block_index(&self) -> usize {
        self.block_index
    }
    pub fn store_signature(&mut self, signature: Option<String>) {
        self.signatures.store(signature);
    }
    pub fn set_trailing_signature(&mut self, signature: Option<String>) {
        self.trailing_signature = signature;
    }
    pub fn has_trailing_signature(&self) -> bool {
        self.trailing_signature.is_some()
    }
    pub fn take_trailing_signature(&mut self) -> Option<String> {
        self.trailing_signature.take()
    }

    /// 处理 SSE 解析错误，实现优雅降级
    ///
    /// 当 SSE stream 中发生解析错误时:
    /// 1. 安全关闭当前 block
    /// 2. 递增错误计数器
    /// 3. 在 debug 模式下输出错误信息
    #[allow(dead_code)] // Prepared for future error recovery implementation
    pub fn handle_parse_error(&mut self, raw_data: &str) -> Vec<Bytes> {
        let mut chunks = Vec::new();

        self.parse_error_count += 1;

        tracing::warn!(
            "[SSE-Parser] Parse error #{} occurred. Raw data length: {} bytes",
            self.parse_error_count,
            raw_data.len()
        );

        // 安全关闭当前 block
        if self.block_type != BlockType::None {
            self.last_valid_state = Some(self.block_type);
            chunks.extend(self.end_block());
        }

        // Debug 模式下输出详细错误信息
        #[cfg(debug_assertions)]
        {
            let preview = if raw_data.len() > 100 {
                format!("{}...", &raw_data[..100])
            } else {
                raw_data.to_string()
            };
            tracing::debug!("[SSE-Parser] Failed chunk preview: {}", preview);
        }

        // 错误率过高时发出警告并尝试发送错误信号
        if self.parse_error_count > 3 {
            // 降低阈值,更早通知用户
            tracing::error!(
                "[SSE-Parser] High error rate detected ({} errors). Stream may be corrupted.",
                self.parse_error_count
            );

            // [FIX] Explicitly signal error to client to prevent UI freeze
            // Using "network_error" type to suggest network/proxy issues
            chunks.push(self.emit(
                "error",
                json!({
                    "type": "error",
                    "error": {
                        "type": "network_error",
                        "message": "网络连接不稳定,请检查您的网络或代理设置。",
                        "code": "stream_decode_error",
                        "details": {
                            "error_count": self.parse_error_count,
                            "suggestion": "请尝试: 1) 检查网络连接 2) 更换代理节点 3) 稍后重试"
                        }
                    }
                }),
            ));
        }

        chunks
    }

    /// 重置错误状态 (recovery 后调用)
    #[allow(dead_code)]
    pub fn reset_error_state(&mut self) {
        self.parse_error_count = 0;
        self.last_valid_state = None;
    }

    /// 获取错误计数 (用于监控)
    #[allow(dead_code)]
    pub fn get_error_count(&self) -> usize {
        self.parse_error_count
    }
}
