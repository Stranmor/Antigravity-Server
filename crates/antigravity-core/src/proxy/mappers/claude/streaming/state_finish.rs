use bytes::Bytes;
use serde_json::json;

use super::state::{BlockType, StreamingState};
use crate::proxy::mappers::claude::models::*;
use crate::proxy::mappers::claude::token_scaling::to_claude_usage;

impl StreamingState {
    pub fn emit_finish(
        &mut self,
        finish_reason: Option<&str>,
        usage_metadata: Option<&UsageMetadata>,
    ) -> Vec<Bytes> {
        let mut chunks = Vec::new();

        let was_inside_block = self.block_type != BlockType::None;
        let prev_block_type = self.block_type;

        chunks.extend(self.end_block());

        if let Some(signature) = self.trailing_signature.take() {
            tracing::info!(
                "[Streaming] Captured trailing signature (len: {}), caching for session.",
                signature.len()
            );
            self.signatures.store(Some(signature));
        }

        chunks.extend(self.emit_grounding_block());

        let stop_reason =
            self.determine_stop_reason(finish_reason, was_inside_block, prev_block_type);

        let usage = usage_metadata
            .map(|u| to_claude_usage(u, self.scaling_enabled, self.context_limit))
            .unwrap_or(Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                server_tool_use: None,
            });

        if let (Some(estimated), Some(um)) = (self.estimated_tokens, usage_metadata) {
            let actual =
                um.prompt_token_count.unwrap_or(0) + um.candidates_token_count.unwrap_or(0);
            if actual > 0 {
                crate::proxy::mappers::estimation_calibrator::get_calibrator()
                    .record(estimated, actual);
            }
        }

        chunks.push(self.emit(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": stop_reason, "stop_sequence": null },
                "usage": usage
            }),
        ));

        if !self.message_stop_sent {
            chunks.push(Bytes::from(
                "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            ));
            self.message_stop_sent = true;
        }

        chunks
    }

    fn determine_stop_reason(
        &self,
        finish_reason: Option<&str>,
        was_inside_block: bool,
        prev_block_type: BlockType,
    ) -> &'static str {
        if self.used_tool {
            "tool_use"
        } else if finish_reason == Some("MAX_TOKENS") {
            "max_tokens"
        } else if finish_reason.is_none() && was_inside_block {
            tracing::warn!(
                "[Truncation Detected] Stream ended without finish_reason while inside {:?} block",
                prev_block_type
            );
            crate::proxy::prometheus::record_truncation();
            "max_tokens"
        } else {
            "end_turn"
        }
    }

    fn emit_grounding_block(&mut self) -> Vec<Bytes> {
        let mut chunks = Vec::new();

        if self.web_search_query.is_none() && self.grounding_chunks.is_none() {
            return chunks;
        }

        let mut grounding_text = String::new();

        if let Some(query) = &self.web_search_query {
            if !query.is_empty() {
                grounding_text.push_str("\n\n---\n**🔍 Searched for:** ");
                grounding_text.push_str(query);
            }
        }

        if let Some(grounding) = &self.grounding_chunks {
            let mut links = Vec::new();
            for (i, chunk) in grounding.iter().enumerate() {
                if let Some(web) = chunk.get("web") {
                    let title = web
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Web Source");
                    let uri = web.get("uri").and_then(|v| v.as_str()).unwrap_or("#");
                    links.push(format!("[{}] [{}]({})", i + 1, title, uri));
                }
            }

            if !links.is_empty() {
                grounding_text.push_str("\n\n**🌐 Source Citations:**\n");
                grounding_text.push_str(&links.join("\n"));
            }
        }

        if !grounding_text.is_empty() {
            chunks.push(self.emit(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": self.block_index,
                    "content_block": { "type": "text", "text": "" }
                }),
            ));
            chunks.push(self.emit_delta("text_delta", json!({ "text": grounding_text })));
            chunks.push(self.emit(
                "content_block_stop",
                json!({ "type": "content_block_stop", "index": self.block_index }),
            ));
            self.block_index += 1;
        }

        chunks
    }
}
