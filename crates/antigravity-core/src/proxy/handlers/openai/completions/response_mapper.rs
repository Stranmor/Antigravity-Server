// Response mapping for legacy completions format

use crate::proxy::mappers::openai::{OpenAIContent, OpenAIResponse};
use serde_json::{json, Value};

pub fn map_chat_to_legacy_response(chat_resp: &OpenAIResponse) -> Value {
    let choices = chat_resp
        .choices
        .iter()
        .map(|c| {
            json!({
                "text": match &c.message.content {
                    Some(OpenAIContent::String(s)) => s.clone(),
                    _ => String::new()
                },
                "index": c.index,
                "logprobs": null,
                "finish_reason": c.finish_reason
            })
        })
        .collect::<Vec<_>>();

    json!({
        "id": chat_resp.id,
        "object": "text_completion",
        "created": chat_resp.created,
        "model": chat_resp.model,
        "choices": choices
    })
}
