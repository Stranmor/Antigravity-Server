//! Tool specifications for ZAI vision tools.

use serde_json::{json, Value};

/// Returns the list of available vision tool specifications.
pub fn tool_specs() -> Vec<Value> {
    vec![
        json!({
            "name": "ui_to_artifact",
            "description": "Convert UI screenshots into artifacts (code/prompt/spec/description).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "image_source": { "type": "string", "description": "Local file path or remote URL to the image" },
                    "output_type": { "type": "string", "enum": ["code","prompt","spec","description"] },
                    "prompt": { "type": "string" }
                },
                "required": ["image_source","output_type","prompt"]
            }
        }),
        json!({
            "name": "extract_text_from_screenshot",
            "description": "Extract text/code from screenshots (OCR-like).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "image_source": { "type": "string" },
                    "prompt": { "type": "string" },
                    "language_hint": { "type": "string" }
                },
                "required": ["image_source","prompt"]
            }
        }),
        json!({
            "name": "diagnose_error_screenshot",
            "description": "Diagnose error screenshots (stack traces, logs, runtime errors).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "image_source": { "type": "string" },
                    "prompt": { "type": "string" },
                    "context": { "type": "string" }
                },
                "required": ["image_source","prompt"]
            }
        }),
        json!({
            "name": "understand_technical_diagram",
            "description": "Analyze architecture/flow/UML/ER diagrams.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "image_source": { "type": "string" },
                    "prompt": { "type": "string" },
                    "diagram_type": { "type": "string" }
                },
                "required": ["image_source","prompt"]
            }
        }),
        json!({
            "name": "analyze_data_visualization",
            "description": "Analyze charts/dashboards to extract insights and trends.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "image_source": { "type": "string" },
                    "prompt": { "type": "string" },
                    "analysis_focus": { "type": "string" }
                },
                "required": ["image_source","prompt"]
            }
        }),
        json!({
            "name": "ui_diff_check",
            "description": "Compare two UI screenshots and report visual differences.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "expected_image_source": { "type": "string" },
                    "actual_image_source": { "type": "string" },
                    "prompt": { "type": "string" }
                },
                "required": ["expected_image_source","actual_image_source","prompt"]
            }
        }),
        json!({
            "name": "analyze_image",
            "description": "General-purpose image analysis.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "image_source": { "type": "string" },
                    "prompt": { "type": "string" }
                },
                "required": ["image_source","prompt"]
            }
        }),
        json!({
            "name": "analyze_video",
            "description": "Analyze video content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "video_source": { "type": "string" },
                    "prompt": { "type": "string" }
                },
                "required": ["video_source","prompt"]
            }
        }),
    ]
}
