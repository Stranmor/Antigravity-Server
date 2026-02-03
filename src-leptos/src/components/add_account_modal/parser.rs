//! Token parsing utilities

pub fn parse_refresh_tokens(input: &str) -> Vec<String> {
    let input = input.trim();
    let mut tokens = Vec::new();

    if input.starts_with('[') && input.ends_with(']') {
        if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(input) {
            for item in parsed {
                if let Some(token) = item
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .filter(|t| t.starts_with("1//"))
                {
                    tokens.push(token.to_string());
                } else if let Some(token) = item.as_str().filter(|t| t.starts_with("1//")) {
                    tokens.push(token.to_string());
                }
            }
            if !tokens.is_empty() {
                return tokens;
            }
        }
    }

    for line in input.lines() {
        for word in line.split_whitespace() {
            let word = word
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '_' && c != '-');
            if word.starts_with("1//") {
                tokens.push(word.to_string());
            }
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
}
