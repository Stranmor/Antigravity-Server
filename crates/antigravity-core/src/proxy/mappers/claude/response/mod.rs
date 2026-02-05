// Claude non-streaming response transformation (Gemini → Claude)
// Corresponds to NonStreamingProcessor

mod grounding;
mod part_processing;
#[cfg(test)]
mod tests;
mod tool_remap;

use super::models::*;
use super::token_scaling::to_claude_usage;
use grounding::{decode_signature, format_grounding_text};
use part_processing::PartProcessingContext;

/// Non-streaming response processor
pub struct NonStreamingProcessor {
    content_blocks: Vec<ContentBlock>,
    text_builder: String,
    thinking_builder: String,
    thinking_signature: Option<String>,
    trailing_signature: Option<String>,
    pub has_tool_call: bool,
    pub scaling_enabled: bool,
    pub context_limit: u32,
    pub session_id: Option<String>,
    pub model_name: String,
}

impl NonStreamingProcessor {
    pub fn new(session_id: Option<String>, model_name: String) -> Self {
        Self {
            content_blocks: Vec::new(),
            text_builder: String::new(),
            thinking_builder: String::new(),
            thinking_signature: None,
            trailing_signature: None,
            has_tool_call: false,
            scaling_enabled: false,
            context_limit: 1_048_576,
            session_id,
            model_name,
        }
    }

    pub fn process(
        &mut self,
        gemini_response: &GeminiResponse,
        scaling_enabled: bool,
        context_limit: u32,
    ) -> ClaudeResponse {
        self.scaling_enabled = scaling_enabled;
        self.context_limit = context_limit;
        let empty_parts = vec![];
        let parts = gemini_response
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|candidate| candidate.content.as_ref())
            .map(|content| &content.parts)
            .unwrap_or(&empty_parts);

        for part in parts {
            self.process_part(part);
        }

        if let Some(candidate) = gemini_response.candidates.as_ref().and_then(|c| c.first()) {
            if let Some(grounding) = &candidate.grounding_metadata {
                self.process_grounding(grounding);
            }
        }

        self.flush_thinking();
        self.flush_text();

        if let Some(signature) = self.trailing_signature.take() {
            self.content_blocks.push(ContentBlock::Thinking {
                thinking: String::new(),
                signature: Some(signature),
                cache_control: None,
            });
        }

        self.build_response(gemini_response)
    }

    fn process_part(&mut self, part: &GeminiPart) {
        let signature = part.thought_signature.as_ref().map(|sig| decode_signature(sig));

        if let Some(sig) = &signature {
            if let Some(s_id) = &self.session_id {
                crate::proxy::SignatureCache::global()
                    .cache_session_signature(s_id, sig.to_string());
                crate::proxy::SignatureCache::global()
                    .cache_thinking_family(sig.to_string(), self.model_name.clone());
                tracing::debug!(
                    "[Claude-Response] Cached signature (len: {}) for session: {}",
                    sig.len(),
                    s_id
                );
            }
        }

        let mut ctx = PartProcessingContext {
            content_blocks: &mut self.content_blocks,
            text_builder: &mut self.text_builder,
            thinking_builder: &mut self.thinking_builder,
            thinking_signature: &mut self.thinking_signature,
            trailing_signature: &mut self.trailing_signature,
            has_tool_call: &mut self.has_tool_call,
        };

        if let Some(fc) = &part.function_call {
            ctx.process_function_call(fc, signature);
            return;
        }

        if let Some(text) = &part.text {
            ctx.process_text(text, part.thought.unwrap_or(false), signature);
        }

        if let Some(img) = &part.inline_data {
            ctx.process_inline_data(img);
        }
    }

    fn process_grounding(&mut self, grounding: &GroundingMetadata) {
        let grounding_text = format_grounding_text(grounding);
        if !grounding_text.is_empty() {
            self.flush_thinking();
            self.flush_text();
            self.text_builder.push_str(&grounding_text);
            self.flush_text();
        }
    }

    fn flush_text(&mut self) {
        let mut ctx = PartProcessingContext {
            content_blocks: &mut self.content_blocks,
            text_builder: &mut self.text_builder,
            thinking_builder: &mut self.thinking_builder,
            thinking_signature: &mut self.thinking_signature,
            trailing_signature: &mut self.trailing_signature,
            has_tool_call: &mut self.has_tool_call,
        };
        ctx.flush_text_internal();
    }

    fn flush_thinking(&mut self) {
        let mut ctx = PartProcessingContext {
            content_blocks: &mut self.content_blocks,
            text_builder: &mut self.text_builder,
            thinking_builder: &mut self.thinking_builder,
            thinking_signature: &mut self.thinking_signature,
            trailing_signature: &mut self.trailing_signature,
            has_tool_call: &mut self.has_tool_call,
        };
        ctx.flush_thinking_internal();
    }

    fn build_response(&self, gemini_response: &GeminiResponse) -> ClaudeResponse {
        let finish_reason = gemini_response
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|candidate| candidate.finish_reason.as_deref());

        let stop_reason = if self.has_tool_call {
            "tool_use"
        } else if finish_reason == Some("MAX_TOKENS") {
            "max_tokens"
        } else {
            "end_turn"
        };

        let usage = gemini_response
            .usage_metadata
            .as_ref()
            .map(|u| to_claude_usage(u, self.scaling_enabled, self.context_limit))
            .unwrap_or(Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                server_tool_use: None,
            });

        ClaudeResponse {
            id: gemini_response.response_id.clone().unwrap_or_else(|| {
                format!("msg_{}", crate::proxy::common::random_id::generate_random_id())
            }),
            type_: "message".to_string(),
            role: "assistant".to_string(),
            model: gemini_response.model_version.clone().unwrap_or_default(),
            content: self.content_blocks.clone(),
            stop_reason: stop_reason.to_string(),
            stop_sequence: None,
            usage,
        }
    }
}

/// Transform Gemini response to Claude response (public interface)
pub fn transform_response(
    gemini_response: &GeminiResponse,
    scaling_enabled: bool,
    context_limit: u32,
    session_id: Option<String>,
    model_name: String,
) -> Result<ClaudeResponse, String> {
    let mut processor = NonStreamingProcessor::new(session_id, model_name);
    Ok(processor.process(gemini_response, scaling_enabled, context_limit))
}
