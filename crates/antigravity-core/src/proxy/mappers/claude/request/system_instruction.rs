//! System instruction building.

use super::super::models::SystemPrompt;
use serde_json::{json, Value};

pub fn build_system_instruction(
    system: &Option<SystemPrompt>,
    _model_name: &str,
    has_mcp_tools: bool,
) -> Option<Value> {
    let mut parts = Vec::new();

    // [NEW] Antigravity 身份指令 (原始简化版)
    let antigravity_identity = "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding.\n\
    You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.\n\
    **Absolute paths only**\n\
    **Proactiveness**";

    // [HYBRID] 检查用户是否已提供 Antigravity 身份
    let mut user_has_antigravity = false;
    if let Some(sys) = system {
        match sys {
            SystemPrompt::String(text) => {
                if text.contains("You are Antigravity") {
                    user_has_antigravity = true;
                }
            }
            SystemPrompt::Array(blocks) => {
                for block in blocks {
                    if block.block_type == "text" && block.text.contains("You are Antigravity") {
                        user_has_antigravity = true;
                        break;
                    }
                }
            }
        }
    }

    // 如果用户没有提供 Antigravity 身份,则注入
    if !user_has_antigravity {
        parts.push(json!({"text": antigravity_identity}));
    }

    // 添加用户的系统提示词
    if let Some(sys) = system {
        match sys {
            SystemPrompt::String(text) => {
                // [FIX] 过滤 OpenCode 默认提示词，但保留用户自定义指令 (Instructions from: ...)
                if text.contains("You are an interactive CLI tool") {
                    // 提取用户自定义指令部分
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
            }
            SystemPrompt::Array(blocks) => {
                for block in blocks {
                    if block.block_type == "text" {
                        // [FIX] 过滤 OpenCode 默认提示词，但保留用户自定义指令
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
            }
        }
    }

    // [NEW] MCP XML Bridge: 如果存在 mcp__ 开头的工具，注入专用的调用协议
    // 这能有效规避部分 MCP 链路在标准的 tool_use 协议下解析不稳的问题
    if has_mcp_tools {
        let mcp_xml_prompt = "\n\
        ==== MCP XML 工具调用协议 (Workaround) ====\n\
        当你需要调用名称以 `mcp__` 开头的 MCP 工具时：\n\
        1) 优先尝试 XML 格式调用：输出 `<mcp__tool_name>{\"arg\":\"value\"}</mcp__tool_name>`。\n\
        2) 必须直接输出 XML 块，无需 markdown 包装，内容为 JSON 格式的入参。\n\
        3) 这种方式具有更高的连通性和容错性，适用于大型结果返回场景。\n\
        ===========================================";
        parts.push(json!({"text": mcp_xml_prompt}));
    }

    // 如果用户没有提供任何系统提示词,添加结束标记
    if !user_has_antigravity {
        parts.push(json!({"text": "\n--- [SYSTEM_PROMPT_END] ---"}));
    }

    Some(json!({
        "role": "user",
        "parts": parts
    }))
}
