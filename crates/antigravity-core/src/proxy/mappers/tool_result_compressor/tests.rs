use super::*;

#[test]
fn test_truncate_text() {
    let text = "a".repeat(300_000);
    let result = truncate_text_safe(&text, 200_000);
    assert!(result.len() < 210_000);
    assert!(result.contains("[truncated"));
    assert!(result.contains("100000 chars]"));
}

#[test]
fn test_truncate_text_no_truncation() {
    let text = "short text";
    let result = truncate_text_safe(text, 1000);
    assert_eq!(result, text);
}

#[test]
fn test_compact_browser_snapshot() {
    let snapshot = format!("page snapshot: {}", "ref=abc ".repeat(10_000));
    let result = compact_tool_result_text(&snapshot, 16_000);

    assert!(result.len() <= 16_500);
    assert!(result.contains("[HEAD]"));
    assert!(result.contains("[TAIL]"));
    assert!(result.contains("page snapshot summarized"));
}

#[test]
fn test_compact_saved_output_notice() {
    let text = r#"result (150000 characters) exceeds maximum allowed tokens. Output has been saved to /tmp/output.txt
Format: JSON array with schema
Please read the file locally."#;

    let result = compact_tool_result_text(text, 500);
    println!("Result: {}", result);
    assert!(result.contains("150000 characters") || result.contains("150,000 characters"));
    assert!(result.contains("/tmp/output.txt"));
    assert!(result.contains("[tool_result omitted") || result.len() <= 500);
}

#[test]
fn test_sanitize_tool_result_blocks() {
    let mut blocks = vec![
        serde_json::json!({
            "type": "text",
            "text": "a".repeat(100_000)
        }),
        serde_json::json!({
            "type": "text",
            "text": "b".repeat(150_000)
        }),
    ];

    sanitize_tool_result_blocks(&mut blocks);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["text"].as_str().unwrap().len(), 100_000);
    assert!(blocks[1]["text"].as_str().unwrap().len() < 110_000);
}

#[test]
fn test_sanitize_removes_base64_image() {
    let mut blocks = vec![
        serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "data": "a".repeat(2_000_000)
            }
        }),
        serde_json::json!({
            "type": "text",
            "text": "some text"
        }),
    ];

    sanitize_tool_result_blocks(&mut blocks);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["type"], "text");
    assert_eq!(blocks[0]["text"], "some text");
    assert!(blocks[1]["text"].as_str().unwrap().contains("[image omitted: 1953KB exceeds limit"));
}

#[test]
fn test_sanitize_keeps_small_base64_image() {
    let mut blocks = vec![
        serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
            }
        }),
        serde_json::json!({
            "type": "text",
            "text": "some text"
        }),
    ];

    sanitize_tool_result_blocks(&mut blocks);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["type"], "image");
    assert_eq!(blocks[1]["type"], "text");
}

#[test]
fn test_is_oversized_base64_image() {
    let small_image = serde_json::json!({
        "type": "image",
        "source": {
            "type": "base64",
            "data": "abc123"
        }
    });
    assert!(is_oversized_base64_image(&small_image).is_none());

    let huge_image = serde_json::json!({
        "type": "image",
        "source": {
            "type": "base64",
            "data": "a".repeat(2_000_000)
        }
    });
    assert_eq!(is_oversized_base64_image(&huge_image), Some(2_000_000));

    let text_block = serde_json::json!({
        "type": "text",
        "text": "hello"
    });
    assert!(is_oversized_base64_image(&text_block).is_none());
}
