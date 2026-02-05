// Part processing helpers for NonStreamingProcessor

use super::super::models::*;
use super::tool_remap::remap_function_call_args;

pub struct PartProcessingContext<'a> {
    pub content_blocks: &'a mut Vec<ContentBlock>,
    pub text_builder: &'a mut String,
    pub thinking_builder: &'a mut String,
    pub thinking_signature: &'a mut Option<String>,
    pub trailing_signature: &'a mut Option<String>,
    pub has_tool_call: &'a mut bool,
}

impl PartProcessingContext<'_> {
    pub fn process_function_call(&mut self, fc: &FunctionCall, signature: Option<String>) {
        self.flush_thinking();
        self.flush_text();

        if let Some(trailing_sig) = self.trailing_signature.take() {
            self.content_blocks.push(ContentBlock::Thinking {
                thinking: String::new(),
                signature: Some(trailing_sig),
                cache_control: None,
            });
        }

        *self.has_tool_call = true;

        let tool_id = fc.id.clone().unwrap_or_else(|| {
            format!("{}-{}", fc.name, crate::proxy::common::random_id::generate_random_id())
        });

        let mut tool_name = fc.name.clone();
        if tool_name.to_lowercase() == "search" {
            tool_name = "Grep".to_string();
        }

        let mut args = fc.args.clone().unwrap_or(serde_json::json!({}));
        remap_function_call_args(&tool_name, &mut args);

        let mut tool_use = ContentBlock::ToolUse {
            id: tool_id,
            name: tool_name,
            input: args,
            signature: None,
            cache_control: None,
        };

        if let ContentBlock::ToolUse { signature: sig, .. } = &mut tool_use {
            *sig = signature;
        }

        self.content_blocks.push(tool_use);
    }

    pub fn process_text(&mut self, text: &str, is_thought: bool, signature: Option<String>) {
        if is_thought {
            self.flush_text();

            if let Some(trailing_sig) = self.trailing_signature.take() {
                self.flush_thinking();
                self.content_blocks.push(ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: Some(trailing_sig),
                    cache_control: None,
                });
            }

            self.thinking_builder.push_str(text);
            if signature.is_some() {
                *self.thinking_signature = signature;
            }
        } else {
            if text.is_empty() {
                if signature.is_some() {
                    *self.trailing_signature = signature;
                }
                return;
            }

            self.flush_thinking();

            if let Some(trailing_sig) = self.trailing_signature.take() {
                self.flush_text();
                self.content_blocks.push(ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: Some(trailing_sig),
                    cache_control: None,
                });
            }

            self.text_builder.push_str(text);

            if let Some(sig) = signature {
                self.flush_text();
                self.content_blocks.push(ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: Some(sig),
                    cache_control: None,
                });
            }
        }
    }

    pub fn process_inline_data(&mut self, img: &InlineData) {
        self.flush_thinking();

        let mime_type = &img.mime_type;
        let data = &img.data;
        if !data.is_empty() {
            let markdown_img = format!("![image](data:{};base64,{})", mime_type, data);
            self.text_builder.push_str(&markdown_img);
            self.flush_text();
        }
    }

    fn flush_text(&mut self) {
        self.flush_text_internal();
    }

    pub fn flush_text_internal(&mut self) {
        if self.text_builder.is_empty() {
            return;
        }

        let mut current_text = self.text_builder.clone();
        self.text_builder.clear();

        loop {
            let result = super::grounding::parse_mcp_xml(&current_text);
            if !result.found {
                break;
            }

            if let Some(text) = result.text_before {
                self.content_blocks.push(ContentBlock::Text { text });
            }

            if let Some((tool_name, input_json)) = result.tool_use {
                self.content_blocks.push(ContentBlock::ToolUse {
                    id: format!("{}-xml", tool_name),
                    name: tool_name,
                    input: input_json,
                    signature: None,
                    cache_control: None,
                });
                *self.has_tool_call = true;
            }

            current_text = result.remaining;
        }

        if !current_text.is_empty() {
            self.content_blocks.push(ContentBlock::Text { text: current_text });
        }
    }

    fn flush_thinking(&mut self) {
        self.flush_thinking_internal();
    }

    pub fn flush_thinking_internal(&mut self) {
        if self.thinking_builder.is_empty() && self.thinking_signature.is_none() {
            return;
        }

        let thinking = self.thinking_builder.clone();
        let signature = self.thinking_signature.take();

        self.content_blocks.push(ContentBlock::Thinking {
            thinking,
            signature,
            cache_control: None,
        });
        self.thinking_builder.clear();
    }
}
