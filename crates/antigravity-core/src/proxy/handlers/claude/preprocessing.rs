//! Message preprocessing and logging utilities

use crate::proxy::mappers::claude::models::{ContentBlock, MessageContent};
use crate::proxy::mappers::claude::ClaudeRequest;
use tracing::{debug, info};

pub fn extract_meaningful_message(request: &ClaudeRequest) -> String {
    let meaningful_msg = request.messages.iter().rev().filter(|m| m.role == "user").find_map(|m| {
        let content = match &m.content {
            MessageContent::String(s) => s.to_string(),
            MessageContent::Array(arr) => arr
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" "),
        };

        if content.trim().is_empty()
            || content.starts_with("Warmup")
            || content.contains("<system-reminder>")
        {
            None
        } else {
            Some(content)
        }
    });

    meaningful_msg.unwrap_or_else(|| {
        request
            .messages
            .last()
            .map(|m| match &m.content {
                MessageContent::String(s) => s.clone(),
                MessageContent::Array(_) => "[Complex/Tool Message]".to_string(),
            })
            .unwrap_or_else(|| "[No Messages]".to_string())
    })
}

pub fn log_request_info(trace_id: &str, request: &ClaudeRequest) {
    info!(
        "[{}] Claude Request | Model: {} | Stream: {} | Messages: {} | Tools: {}",
        trace_id,
        request.model,
        request.stream,
        request.messages.len(),
        request.tools.is_some()
    );
}

pub fn log_request_debug(trace_id: &str, request: &ClaudeRequest, latest_msg: &str) {
    debug!("========== [{}] CLAUDE REQUEST DEBUG START ==========", trace_id);
    debug!("[{}] Model: {}", trace_id, request.model);
    debug!("[{}] Stream: {}", trace_id, request.stream);
    debug!("[{}] Max Tokens: {:?}", trace_id, request.max_tokens);
    debug!("[{}] Temperature: {:?}", trace_id, request.temperature);
    debug!("[{}] Message Count: {}", trace_id, request.messages.len());
    debug!("[{}] Has Tools: {}", trace_id, request.tools.is_some());
    debug!("[{}] Has Thinking Config: {}", trace_id, request.thinking.is_some());
    debug!("[{}] Content Preview: {:.100}...", trace_id, latest_msg);

    for (idx, msg) in request.messages.iter().enumerate() {
        let content_preview = match &msg.content {
            MessageContent::String(s) => {
                let char_count = s.chars().count();
                if char_count > 200 {
                    let preview: String = s.chars().take(200).collect();
                    format!("{}... (total {} chars)", preview, char_count)
                } else {
                    s.clone()
                }
            },
            MessageContent::Array(arr) => {
                format!("[Array with {} blocks]", arr.len())
            },
        };
        debug!(
            "[{}] Message[{}] - Role: {}, Content: {}",
            trace_id, idx, msg.role, content_preview
        );
    }

    debug!(
        "[{}] Full Claude Request JSON: {}",
        trace_id,
        serde_json::to_string_pretty(request).unwrap_or_default()
    );
    debug!("========== [{}] CLAUDE REQUEST DEBUG END ==========", trace_id);
}
