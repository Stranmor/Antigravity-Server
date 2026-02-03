use crate::proxy::mappers::claude::ClaudeRequest;

const BACKGROUND_MODEL_LITE: &str = "gemini-2.5-flash-lite";
const BACKGROUND_MODEL_STANDARD: &str = "gemini-2.5-flash";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundTaskType {
    TitleGeneration,
    SimpleSummary,
    ContextCompression,
    PromptSuggestion,
    SystemMessage,
    EnvironmentProbe,
}

const TITLE_KEYWORDS: &[&str] = &[
    "write a 5-10 word title",
    "Please write a 5-10 word title",
    "Respond with the title",
    "Generate a title for",
    "Create a brief title",
    "title for the conversation",
    "conversation title",
    "generate title",
    "give conversation a title",
];

const SUMMARY_KEYWORDS: &[&str] = &[
    "Summarize this coding conversation",
    "Summarize the conversation",
    "Concise summary",
    "in under 50 characters",
    "compress the context",
    "Provide a concise summary",
    "condense the previous messages",
    "shorten the conversation history",
    "extract key points from",
];

const SUGGESTION_KEYWORDS: &[&str] = &[
    "prompt suggestion generator",
    "suggest next prompts",
    "what should I ask next",
    "generate follow-up questions",
    "recommend next steps",
    "possible next actions",
];

const SYSTEM_KEYWORDS: &[&str] = &["Warmup", "<system-reminder>", "This is a system message"];

const PROBE_KEYWORDS: &[&str] = &[
    "check current directory",
    "list available tools",
    "verify environment",
    "test connection",
];

pub fn detect_background_task_type(request: &ClaudeRequest) -> Option<BackgroundTaskType> {
    let last_user_msg = extract_last_user_message_for_detection(request)?;
    let preview = last_user_msg.chars().take(500).collect::<String>();

    if last_user_msg.len() > 800 {
        return None;
    }

    if matches_keywords(&preview, SYSTEM_KEYWORDS) {
        return Some(BackgroundTaskType::SystemMessage);
    }

    if matches_keywords(&preview, TITLE_KEYWORDS) {
        return Some(BackgroundTaskType::TitleGeneration);
    }

    if matches_keywords(&preview, SUMMARY_KEYWORDS) {
        if preview.contains("in under 50 characters") {
            return Some(BackgroundTaskType::SimpleSummary);
        }
        return Some(BackgroundTaskType::ContextCompression);
    }

    if matches_keywords(&preview, SUGGESTION_KEYWORDS) {
        return Some(BackgroundTaskType::PromptSuggestion);
    }

    if matches_keywords(&preview, PROBE_KEYWORDS) {
        return Some(BackgroundTaskType::EnvironmentProbe);
    }

    None
}

fn matches_keywords(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

fn extract_last_user_message_for_detection(request: &ClaudeRequest) -> Option<String> {
    request
        .messages
        .iter()
        .rev()
        .filter(|m| m.role == "user")
        .find_map(|m| {
            let content = match &m.content {
                crate::proxy::mappers::claude::models::MessageContent::String(s) => s.to_string(),
                crate::proxy::mappers::claude::models::MessageContent::Array(arr) => arr
                    .iter()
                    .filter_map(|block| match block {
                        crate::proxy::mappers::claude::models::ContentBlock::Text { text } => {
                            Some(text.as_str())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            };

            if content.trim().is_empty()
                || content.starts_with("Warmup")
                || content.contains("<system-reminder>")
            {
                None
            } else {
                Some(content)
            }
        })
}

pub fn select_background_model(task_type: BackgroundTaskType) -> &'static str {
    match task_type {
        BackgroundTaskType::TitleGeneration => BACKGROUND_MODEL_LITE,
        BackgroundTaskType::SimpleSummary => BACKGROUND_MODEL_LITE,
        BackgroundTaskType::SystemMessage => BACKGROUND_MODEL_LITE,
        BackgroundTaskType::PromptSuggestion => BACKGROUND_MODEL_LITE,
        BackgroundTaskType::EnvironmentProbe => BACKGROUND_MODEL_LITE,
        BackgroundTaskType::ContextCompression => BACKGROUND_MODEL_STANDARD,
    }
}
