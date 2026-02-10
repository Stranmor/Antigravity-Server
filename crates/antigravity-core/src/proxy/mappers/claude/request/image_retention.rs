use serde_json::Value;

const DEFAULT_IMAGE_RETENTION_TURNS: usize = 5;
const IMAGE_PLACEHOLDER: &str = "[Image was provided in this message]";

pub fn strip_old_images(contents: &mut Value) {
    let contents_arr = match contents.as_array_mut() {
        Some(arr) => arr,
        None => return,
    };

    let user_indices: Vec<usize> = contents_arr
        .iter()
        .enumerate()
        .filter(|(_, c)| c.get("role").and_then(|r| r.as_str()) == Some("user"))
        .map(|(i, _)| i)
        .collect();

    let total_user = user_indices.len();
    if total_user <= DEFAULT_IMAGE_RETENTION_TURNS {
        return;
    }

    let cutoff = total_user.saturating_sub(DEFAULT_IMAGE_RETENTION_TURNS);
    let indices_to_strip: Vec<usize> = user_indices.into_iter().take(cutoff).collect();

    let mut stripped_count: usize = 0;
    for idx in indices_to_strip {
        if let Some(msg) = contents_arr.get_mut(idx) {
            stripped_count = stripped_count.saturating_add(replace_inline_data_in_parts(msg));
        }
    }

    if stripped_count > 0 {
        tracing::info!(
            "[Image-Retention] Stripped {} images from old user messages (keeping last {} turns)",
            stripped_count,
            DEFAULT_IMAGE_RETENTION_TURNS
        );
    }
}

fn replace_inline_data_in_parts(msg: &mut Value) -> usize {
    let parts = match msg.get_mut("parts").and_then(|p| p.as_array_mut()) {
        Some(p) => p,
        None => return 0,
    };

    let mut replaced: usize = 0;
    let mut i: usize = 0;
    while i < parts.len() {
        if parts[i].get("inlineData").is_some() {
            parts[i] = serde_json::json!({"text": IMAGE_PLACEHOLDER});
            replaced = replaced.saturating_add(1);
        }
        i = i.saturating_add(1);
    }
    replaced
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_stripping_when_few_user_messages() {
        let mut contents = json!([
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "abc"}}]},
            {"role": "model", "parts": [{"text": "I see an image"}]},
            {"role": "user", "parts": [{"text": "hello"}]},
        ]);
        strip_old_images(&mut contents);
        assert!(contents[0]["parts"][0].get("inlineData").is_some());
    }

    #[test]
    fn strips_images_from_old_user_messages() {
        let mut contents = json!([
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img1"}}]},
            {"role": "model", "parts": [{"text": "response 1"}]},
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img2"}}]},
            {"role": "model", "parts": [{"text": "response 2"}]},
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img3"}}]},
            {"role": "model", "parts": [{"text": "response 3"}]},
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img4"}}]},
            {"role": "model", "parts": [{"text": "response 4"}]},
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img5"}}]},
            {"role": "model", "parts": [{"text": "response 5"}]},
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img6"}}]},
            {"role": "model", "parts": [{"text": "response 6"}]},
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img7"}}]},
        ]);
        strip_old_images(&mut contents);

        assert_eq!(contents[0]["parts"][0]["text"].as_str().unwrap(), IMAGE_PLACEHOLDER);
        assert_eq!(contents[2]["parts"][0]["text"].as_str().unwrap(), IMAGE_PLACEHOLDER);

        assert!(contents[8]["parts"][0].get("inlineData").is_some());
        assert!(contents[10]["parts"][0].get("inlineData").is_some());
        assert!(contents[12]["parts"][0].get("inlineData").is_some());
    }

    #[test]
    fn preserves_text_parts_in_stripped_messages() {
        let mut contents = json!([
            {"role": "user", "parts": [
                {"text": "Look at this image"},
                {"inlineData": {"mimeType": "image/png", "data": "img1"}}
            ]},
            {"role": "model", "parts": [{"text": "response"}]},
            {"role": "user", "parts": [{"text": "msg2"}]},
            {"role": "model", "parts": [{"text": "resp2"}]},
            {"role": "user", "parts": [{"text": "msg3"}]},
            {"role": "model", "parts": [{"text": "resp3"}]},
            {"role": "user", "parts": [{"text": "msg4"}]},
            {"role": "model", "parts": [{"text": "resp4"}]},
            {"role": "user", "parts": [{"text": "msg5"}]},
            {"role": "model", "parts": [{"text": "resp5"}]},
            {"role": "user", "parts": [{"text": "msg6"}]},
        ]);
        strip_old_images(&mut contents);

        assert_eq!(contents[0]["parts"][0]["text"].as_str().unwrap(), "Look at this image");
        assert_eq!(contents[0]["parts"][1]["text"].as_str().unwrap(), IMAGE_PLACEHOLDER);
    }

    #[test]
    fn does_not_touch_model_messages() {
        let mut contents = json!([
            {"role": "user", "parts": [{"inlineData": {"mimeType": "image/png", "data": "img1"}}]},
            {"role": "model", "parts": [{"inlineData": {"mimeType": "image/png", "data": "model_img"}}]},
            {"role": "user", "parts": [{"text": "2"}]},
            {"role": "model", "parts": [{"text": "r2"}]},
            {"role": "user", "parts": [{"text": "3"}]},
            {"role": "model", "parts": [{"text": "r3"}]},
            {"role": "user", "parts": [{"text": "4"}]},
            {"role": "model", "parts": [{"text": "r4"}]},
            {"role": "user", "parts": [{"text": "5"}]},
            {"role": "model", "parts": [{"text": "r5"}]},
            {"role": "user", "parts": [{"text": "6"}]},
        ]);
        strip_old_images(&mut contents);

        assert!(contents[1]["parts"][0].get("inlineData").is_some());
    }

    #[test]
    fn handles_empty_contents() {
        let mut contents = json!([]);
        strip_old_images(&mut contents);
        assert_eq!(contents.as_array().unwrap().len(), 0);
    }

    #[test]
    fn handles_non_array_contents() {
        let mut contents = json!(null);
        strip_old_images(&mut contents);
        assert!(contents.is_null());
    }
}
