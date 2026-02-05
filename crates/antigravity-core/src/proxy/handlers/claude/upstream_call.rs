//! Upstream call preparation for Claude messages

use crate::proxy::mappers::claude::ClaudeRequest;
use std::collections::HashMap;

pub struct UpstreamCallConfig {
    pub method: &'static str,
    pub query: Option<&'static str>,
    pub extra_headers: HashMap<String, String>,
    pub client_wants_stream: bool,
    pub actual_stream: bool,
}

pub fn prepare_upstream_call(
    request: &ClaudeRequest,
    request_with_mapped: &ClaudeRequest,
    trace_id: &str,
) -> UpstreamCallConfig {
    let client_wants_stream = request.stream;
    let force_stream_internally = !client_wants_stream;
    let actual_stream = client_wants_stream || force_stream_internally;

    if force_stream_internally {
        tracing::info!(
            "[{}] 🔄 Auto-converting non-stream request to stream for better quota",
            trace_id
        );
    }

    let method = if actual_stream { "streamGenerateContent" } else { "generateContent" };
    let query = if actual_stream { Some("alt=sse") } else { None };

    let mut extra_headers = HashMap::new();
    if request_with_mapped.thinking.is_some() && request_with_mapped.tools.is_some() {
        extra_headers
            .insert("anthropic-beta".to_string(), "interleaved-thinking-2025-05-14".to_string());
        tracing::debug!("[{}] Added Beta Header: interleaved-thinking-2025-05-14", trace_id);
    }

    UpstreamCallConfig { method, query, extra_headers, client_wants_stream, actual_stream }
}
