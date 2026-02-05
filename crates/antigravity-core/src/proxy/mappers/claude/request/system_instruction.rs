//! System instruction building.

use super::super::models::SystemPrompt;
use serde_json::{json, Value};

pub fn build_system_instruction(
    system: &Option<SystemPrompt>,
    _model_name: &str,
    has_mcp_tools: bool,
) -> Option<Value> {
    let mut parts = Vec::new();

    // [NEW] Antigravity identity instruction (simplified version)
    let antigravity_identity = "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding.\n\
    You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.\n\
    **Absolute paths only**\n\
    **Proactiveness**";

    // [HYBRID] Check if user already provided Antigravity identity
    let mut user_has_antigravity = false;
    if let Some(sys) = system {
        match sys {
            SystemPrompt::String(text) => {
                if text.contains("You are Antigravity") {
                    user_has_antigravity = true;
                }
            },
            SystemPrompt::Array(blocks) => {
                for block in blocks {
                    if block.block_type == "text" && block.text.contains("You are Antigravity") {
                        user_has_antigravity = true;
                        break;
                    }
                }
            },
        }
    }

    // If user didn't provide Antigravity identity, inject it
    if !user_has_antigravity {
        parts.push(json!({"text": antigravity_identity}));
    }

    // Add user system prompt
    if let Some(sys) = system {
        match sys {
            SystemPrompt::String(text) => {
                // [FIX] Filter OpenCode default prompt, but preserve user custom instructions (Instructions from: ...)
                if text.contains("You are an interactive CLI tool") {
                    // Extract user custom instructions part
                    if let Some(idx) = text.find("Instructions from:") {
                        let custom_part = &text[idx..];
                        tracing::info!(
                            "[Claude-Request] Extracted custom instructions (len: {}), filtered default prompt",
                            custom_part.len()
                        );
                        parts.push(json!({"text": custom_part}));
                    } else {
                        tracing::info!(
                            "[Claude-Request] Filtering out OpenCode default system instruction (len: {})",
                            text.len()
                        );
                    }
                } else {
                    parts.push(json!({"text": text}));
                }
            },
            SystemPrompt::Array(blocks) => {
                for block in blocks {
                    if block.block_type == "text" {
                        // [FIX] Filter OpenCode default prompt, but preserve user custom instructions
                        if block.text.contains("You are an interactive CLI tool") {
                            if let Some(idx) = block.text.find("Instructions from:") {
                                let custom_part = &block.text[idx..];
                                tracing::info!(
                                    "[Claude-Request] Extracted custom instructions from block (len: {})",
                                    custom_part.len()
                                );
                                parts.push(json!({"text": custom_part}));
                            } else {
                                tracing::info!(
                                    "[Claude-Request] Filtering out OpenCode default system block (len: {})",
                                    block.text.len()
                                );
                            }
                        } else {
                            parts.push(json!({"text": block.text}));
                        }
                    }
                }
            },
        }
    }

    // [NEW] MCP XML Bridge: If mcp__ prefixed tools exist, inject dedicated call protocol
    // This effectively avoids parsing instability issues in partial MCP chains under standard tool_use protocol
    if has_mcp_tools {
        let mcp_xml_prompt = "\n\
        ==== MCP XML Tool Call Protocol (Workaround) ====\n\
        When you need to call MCP tools with `mcp__` prefix:\n\
        1) Prefer XML format call: output `<mcp__tool_name>{\"arg\":\"value\"}</mcp__tool_name>`.\n\
        2) Must directly output XML block, no markdown wrap needed, content as JSON format parameters.\n\
        3) This method has higher connectivity and fault tolerance, suitable for large result return scenarios.\n\
        ===========================================";
        parts.push(json!({"text": mcp_xml_prompt}));
    }

    // If user didn't provide any system prompt, add end marker
    if !user_has_antigravity {
        parts.push(json!({"text": "\n--- [SYSTEM_PROMPT_END] ---"}));
    }

    Some(json!({
        "role": "user",
        "parts": parts
    }))
}
