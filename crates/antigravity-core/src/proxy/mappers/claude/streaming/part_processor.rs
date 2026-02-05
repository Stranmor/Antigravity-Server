use bytes::Bytes;
use serde_json::json;

use super::tool_remapping::remap_function_call_args;
use super::BlockType;
use super::StreamingState;
use crate::proxy::mappers::claude::models::*;
use crate::proxy::SignatureCache;

pub struct PartProcessor<'a> {
    state: &'a mut StreamingState,
}

impl<'a> PartProcessor<'a> {
    pub fn new(state: &'a mut StreamingState) -> Self {
        Self { state }
    }

    fn emit_trailing_signature(&mut self) -> Vec<Bytes> {
        let mut chunks = Vec::new();
        chunks.extend(self.state.end_block());
        if let Some(trailing_sig) = self.state.take_trailing_signature() {
            chunks.push(self.state.emit(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": self.state.current_block_index(),
                    "content_block": { "type": "thinking", "thinking": "" }
                }),
            ));
            chunks.push(self.state.emit_delta("thinking_delta", json!({ "thinking": "" })));
            chunks.push(
                self.state.emit_delta("signature_delta", json!({ "signature": trailing_sig })),
            );
            chunks.extend(self.state.end_block());
        }
        chunks
    }

    pub fn process(&mut self, part: &GeminiPart) -> Vec<Bytes> {
        let mut chunks = Vec::new();
        let signature = part.thought_signature.as_ref().map(|sig: &String| {
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(sig) {
                Ok(decoded_bytes) => match String::from_utf8(decoded_bytes) {
                    Ok(decoded_str) => {
                        tracing::debug!(
                            "[Streaming] Decoded base64 signature (len {} -> {})",
                            sig.len(),
                            decoded_str.len()
                        );
                        decoded_str
                    },
                    Err(_) => sig.clone(),
                },
                Err(_) => sig.clone(),
            }
        });

        if let Some(fc) = &part.function_call {
            if self.state.has_trailing_signature() {
                chunks.extend(self.emit_trailing_signature());
            }

            chunks.extend(self.process_function_call(fc, signature));
            return chunks;
        }

        if let Some(text) = &part.text {
            if part.thought.unwrap_or(false) {
                chunks.extend(self.process_thinking(text, signature));
            } else {
                chunks.extend(self.process_text(text, signature));
            }
        }

        if let Some(img) = &part.inline_data {
            let mime_type = &img.mime_type;
            let data = &img.data;
            if !data.is_empty() {
                let markdown_img = format!("![image](data:{};base64,{})", mime_type, data);
                chunks.extend(self.process_text(&markdown_img, None));
            }
        }

        chunks
    }

    fn process_thinking(&mut self, text: &str, signature: Option<String>) -> Vec<Bytes> {
        let mut chunks = Vec::new();

        if self.state.has_trailing_signature() {
            chunks.extend(self.emit_trailing_signature());
        }

        if self.state.current_block_type() != BlockType::Thinking {
            chunks.extend(
                self.state.start_block(
                    BlockType::Thinking,
                    json!({ "type": "thinking", "thinking": "" }),
                ),
            );
        }

        if !text.is_empty() {
            chunks.push(self.state.emit_delta("thinking_delta", json!({ "thinking": text })));
            self.state.accumulate_thinking(text);
        }

        if let Some(ref sig) = signature {
            if let Some(model) = self.state.model_name.clone() {
                SignatureCache::global().cache_thinking_family(sig.clone(), model.clone());

                let accumulated = self.state.get_accumulated_thinking();
                if !accumulated.is_empty() {
                    SignatureCache::global().cache_content_signature(
                        &accumulated,
                        sig.clone(),
                        model,
                    );
                }
            }
            if let Some(session_id) = &self.state.session_id {
                SignatureCache::global().cache_session_signature(session_id, sig.clone());
            }
            tracing::debug!("[Claude-SSE] Captured thought_signature (len={})", sig.len());
        }

        self.state.store_signature(signature);
        chunks
    }

    fn process_text(&mut self, text: &str, signature: Option<String>) -> Vec<Bytes> {
        let mut chunks = Vec::new();

        if text.is_empty() {
            if signature.is_some() {
                self.state.set_trailing_signature(signature);
            }
            return chunks;
        }

        if self.state.has_trailing_signature() {
            chunks.extend(self.emit_trailing_signature());
        }

        if signature.is_some() {
            self.state.store_signature(signature);

            chunks.extend(
                self.state.start_block(BlockType::Text, json!({ "type": "text", "text": "" })),
            );
            chunks.push(self.state.emit_delta("text_delta", json!({ "text": text })));
            chunks.extend(self.state.end_block());

            return chunks;
        }

        if text.contains("<mcp__") || self.state.in_mcp_xml {
            self.state.in_mcp_xml = true;
            self.state.mcp_xml_buffer.push_str(text);

            if self.state.mcp_xml_buffer.contains("</mcp__")
                && self.state.mcp_xml_buffer.contains('>')
            {
                let buffer = self.state.mcp_xml_buffer.clone();
                if let Some(start_idx) = buffer.find("<mcp__") {
                    if let Some(tag_end_idx) = buffer[start_idx..].find('>') {
                        let actual_tag_end = start_idx + tag_end_idx;
                        let tool_name = &buffer[start_idx + 1..actual_tag_end];
                        let end_tag = format!("</{}>", tool_name);

                        if let Some(close_idx) = buffer.find(&end_tag) {
                            let input_str = &buffer[actual_tag_end + 1..close_idx];
                            let input_json: serde_json::Value =
                                serde_json::from_str(input_str.trim())
                                    .unwrap_or_else(|_| json!({ "input": input_str.trim() }));

                            let fc = FunctionCall {
                                name: tool_name.to_string(),
                                args: Some(input_json),
                                id: Some(format!("{}-xml", tool_name)),
                            };

                            let tool_chunks = self.process_function_call(&fc, None);

                            self.state.mcp_xml_buffer.clear();
                            self.state.in_mcp_xml = false;

                            if start_idx > 0 {
                                let prefix_text = &buffer[..start_idx];
                                if self.state.current_block_type() != BlockType::Text {
                                    chunks.extend(self.state.start_block(
                                        BlockType::Text,
                                        json!({ "type": "text", "text": "" }),
                                    ));
                                }
                                chunks.push(
                                    self.state
                                        .emit_delta("text_delta", json!({ "text": prefix_text })),
                                );
                            }

                            chunks.extend(tool_chunks);

                            let suffix = &buffer[close_idx + end_tag.len()..];
                            if !suffix.is_empty() {
                                chunks.extend(self.process_text(suffix, None));
                            }

                            return chunks;
                        }
                    }
                }
            }
            return vec![];
        }

        if self.state.current_block_type() != BlockType::Text {
            chunks.extend(
                self.state.start_block(BlockType::Text, json!({ "type": "text", "text": "" })),
            );
        }

        chunks.push(self.state.emit_delta("text_delta", json!({ "text": text })));

        chunks
    }

    fn process_function_call(
        &mut self,
        fc: &FunctionCall,
        signature: Option<String>,
    ) -> Vec<Bytes> {
        let mut chunks = Vec::new();

        self.state.mark_tool_used();

        let tool_id = fc.id.clone().unwrap_or_else(|| {
            format!("{}-{}", fc.name, crate::proxy::common::random_id::generate_random_id())
        });

        let mut tool_name = fc.name.clone();
        if tool_name.to_lowercase() == "search" {
            tool_name = "grep".to_string();
            tracing::debug!("[Streaming] Normalizing tool name: Search → grep");
        }

        let mut tool_use = json!({
            "type": "tool_use",
            "id": tool_id,
            "name": tool_name,
            "input": {}
        });

        if let Some(ref sig) = signature {
            tool_use["signature"] = json!(sig);

            SignatureCache::global().cache_tool_signature(&tool_id, sig.clone());

            if let Some(session_id) = &self.state.session_id {
                SignatureCache::global().cache_session_signature(session_id, sig.clone());
            }

            tracing::debug!(
                "[Claude-SSE] Captured thought_signature for function call (length: {})",
                sig.len()
            );
        }

        chunks.extend(self.state.start_block(BlockType::Function, tool_use));

        if let Some(args) = &fc.args {
            let mut remapped_args = args.clone();

            let tool_name_title = fc.name.clone();
            let mut final_tool_name = tool_name_title;
            if final_tool_name.to_lowercase() == "search" {
                final_tool_name = "Grep".to_string();
            }
            remap_function_call_args(&final_tool_name, &mut remapped_args);

            let json_str =
                serde_json::to_string(&remapped_args).unwrap_or_else(|_| "{}".to_string());
            chunks.push(
                self.state.emit_delta("input_json_delta", json!({ "partial_json": json_str })),
            );
        }

        chunks.extend(self.state.end_block());

        chunks
    }
}
