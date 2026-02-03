mod client;
mod media;
mod specs;

use serde_json::{json, Value};

use crate::proxy::config::UpstreamProxyConfig;
use crate::proxy::ZaiConfig;

pub use specs::tool_specs;

use client::{build_client, vision_chat_completion};
use media::{image_source_to_content, video_source_to_content};

pub async fn call_tool(
    zai: &ZaiConfig,
    upstream_proxy: UpstreamProxyConfig,
    timeout_secs: u64,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let api_key = zai.api_key.trim();
    if api_key.is_empty() {
        return Err("z.ai api_key is missing".to_string());
    }

    let client = build_client(upstream_proxy, timeout_secs)?;

    let tool_result = match tool_name {
        "ui_to_artifact" => {
            let image_source = arguments
                .get("image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing image_source")?;
            let output_type = arguments
                .get("output_type")
                .and_then(|v| v.as_str())
                .ok_or("Missing output_type")?;
            let prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?;

            let system_prompt = match output_type {
                "code" => {
                    "You are a frontend engineer. Generate clean, accessible, responsive frontend code from the UI screenshot."
                }
                "prompt" => "You generate precise prompts to recreate UI screenshots.",
                "spec" => {
                    "You are a design systems architect. Produce a detailed UI specification from the screenshot."
                }
                "description" => {
                    "You describe UI screenshots clearly and completely in natural language."
                }
                _ => return Err("Invalid output_type".to_string()),
            };

            let image = image_source_to_content(image_source, 5)?;
            vision_chat_completion(&client, api_key, system_prompt, vec![image], prompt).await?
        }
        "extract_text_from_screenshot" => {
            let image_source = arguments
                .get("image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing image_source")?;
            let mut prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?
                .to_string();
            if let Some(lang) = arguments.get("language_hint").and_then(|v| v.as_str()) {
                if !lang.trim().is_empty() {
                    prompt.push_str(&format!("\n\nLanguage hint: {}", lang.trim()));
                }
            }
            let image = image_source_to_content(image_source, 5)?;
            let system_prompt = "Extract text from the screenshot accurately. Preserve code formatting. If unsure, say what is uncertain.";
            vision_chat_completion(&client, api_key, system_prompt, vec![image], &prompt).await?
        }
        "diagnose_error_screenshot" => {
            let image_source = arguments
                .get("image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing image_source")?;
            let mut prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?
                .to_string();
            if let Some(ctx) = arguments.get("context").and_then(|v| v.as_str()) {
                if !ctx.trim().is_empty() {
                    prompt.push_str(&format!("\n\nContext: {}", ctx.trim()));
                }
            }
            let image = image_source_to_content(image_source, 5)?;
            let system_prompt = "Diagnose the error shown in the screenshot. Identify root cause, propose fixes and verification steps.";
            vision_chat_completion(&client, api_key, system_prompt, vec![image], &prompt).await?
        }
        "understand_technical_diagram" => {
            let image_source = arguments
                .get("image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing image_source")?;
            let mut prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?
                .to_string();
            if let Some(diagram_type) = arguments.get("diagram_type").and_then(|v| v.as_str()) {
                if !diagram_type.trim().is_empty() {
                    prompt.push_str(&format!("\n\nDiagram type: {}", diagram_type.trim()));
                }
            }
            let image = image_source_to_content(image_source, 5)?;
            let system_prompt = "Explain the technical diagram. Describe components, relationships, data flows, and key assumptions.";
            vision_chat_completion(&client, api_key, system_prompt, vec![image], &prompt).await?
        }
        "analyze_data_visualization" => {
            let image_source = arguments
                .get("image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing image_source")?;
            let mut prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?
                .to_string();
            if let Some(focus) = arguments.get("analysis_focus").and_then(|v| v.as_str()) {
                if !focus.trim().is_empty() {
                    prompt.push_str(&format!("\n\nFocus: {}", focus.trim()));
                }
            }
            let image = image_source_to_content(image_source, 5)?;
            let system_prompt = "Analyze the chart/dashboard and extract insights, trends, anomalies, and recommendations.";
            vision_chat_completion(&client, api_key, system_prompt, vec![image], &prompt).await?
        }
        "ui_diff_check" => {
            let expected = arguments
                .get("expected_image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing expected_image_source")?;
            let actual = arguments
                .get("actual_image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing actual_image_source")?;
            let prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?;

            let expected_img = image_source_to_content(expected, 5)?;
            let actual_img = image_source_to_content(actual, 5)?;
            let system_prompt = "Compare the two UI screenshots and report differences grouped by severity. Include actionable fix suggestions.";
            vision_chat_completion(
                &client,
                api_key,
                system_prompt,
                vec![expected_img, actual_img],
                prompt,
            )
            .await?
        }
        "analyze_image" => {
            let image_source = arguments
                .get("image_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing image_source")?;
            let prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?;
            let image = image_source_to_content(image_source, 5)?;
            let system_prompt = "Analyze the image. Be precise and include relevant details.";
            vision_chat_completion(&client, api_key, system_prompt, vec![image], prompt).await?
        }
        "analyze_video" => {
            let video_source = arguments
                .get("video_source")
                .and_then(|v| v.as_str())
                .ok_or("Missing video_source")?;
            let prompt = arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing prompt")?;
            let video = video_source_to_content(video_source, 8)?;
            let system_prompt = "Analyze the video content according to the user's request.";
            vision_chat_completion(&client, api_key, system_prompt, vec![video], prompt).await?
        }
        _ => return Err("Unknown tool".to_string()),
    };

    Ok(json!({
        "content": [
            { "type": "text", "text": tool_result }
        ]
    }))
}
