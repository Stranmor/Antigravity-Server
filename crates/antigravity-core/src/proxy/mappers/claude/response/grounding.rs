// Grounding metadata processing (Web Search results)

use super::super::models::*;

/// Decode base64 signature to string
pub fn decode_signature(sig: &str) -> String {
    use base64::Engine;
    match base64::engine::general_purpose::STANDARD.decode(sig) {
        Ok(decoded_bytes) => match String::from_utf8(decoded_bytes) {
            Ok(decoded_str) => {
                tracing::debug!(
                    "[Response] Decoded base64 signature (len {} -> {})",
                    sig.len(),
                    decoded_str.len()
                );
                decoded_str
            }
            Err(_) => sig.to_string(),
        },
        Err(_) => sig.to_string(),
    }
}

/// Format grounding metadata as markdown text
pub fn format_grounding_text(grounding: &GroundingMetadata) -> String {
    let mut grounding_text = String::new();

    if let Some(queries) = &grounding.web_search_queries {
        if !queries.is_empty() {
            grounding_text.push_str("\n\n---\n**🔍 Searched for:** ");
            grounding_text.push_str(&queries.join(", "));
        }
    }

    if let Some(chunks) = &grounding.grounding_chunks {
        let mut links = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            if let Some(web) = &chunk.web {
                let title = web.title.as_deref().unwrap_or("Web Source");
                let uri = web.uri.as_deref().unwrap_or("#");
                links.push(format!("[{}] [{}]({})", i + 1, title, uri));
            }
        }

        if !links.is_empty() {
            grounding_text.push_str("\n\n**🌐 Source Citations:**\n");
            grounding_text.push_str(&links.join("\n"));
        }
    }

    grounding_text
}

/// MCP XML bridge result
pub struct McpXmlParseResult {
    pub text_before: Option<String>,
    pub tool_use: Option<(String, serde_json::Value)>,
    pub remaining: String,
    pub found: bool,
}

/// Parse MCP XML tags from text and extract tool calls
pub fn parse_mcp_xml(text: &str) -> McpXmlParseResult {
    if let Some(start_idx) = text.find("<mcp__") {
        if let Some(tag_end_idx) = text[start_idx..].find('>') {
            let actual_tag_end = start_idx + tag_end_idx;
            let tool_name = &text[start_idx + 1..actual_tag_end];
            let end_tag = format!("</{}>", tool_name);

            if let Some(close_idx) = text.find(&end_tag) {
                let text_before = if start_idx > 0 {
                    Some(text[..start_idx].to_string())
                } else {
                    None
                };

                let input_str = &text[actual_tag_end + 1..close_idx];
                let input_json: serde_json::Value = serde_json::from_str(input_str.trim())
                    .unwrap_or_else(|_| serde_json::json!({ "input": input_str.trim() }));

                let remaining = text[close_idx + end_tag.len()..].to_string();

                return McpXmlParseResult {
                    text_before,
                    tool_use: Some((tool_name.to_string(), input_json)),
                    remaining,
                    found: true,
                };
            }
        }
    }

    McpXmlParseResult {
        text_before: None,
        tool_use: None,
        remaining: text.to_string(),
        found: false,
    }
}
