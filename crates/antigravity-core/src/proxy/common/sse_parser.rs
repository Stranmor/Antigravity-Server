/// Parse a single SSE line into (key, value) pair.
///
/// SSE format: `key: value\n`
pub fn parse_sse_line(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let key = &line[..colon_pos];
    let value = line[colon_pos + 1..].trim_start();
    Some((key.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_data_line() {
        let result = parse_sse_line("data: hello world");
        assert_eq!(result, Some(("data".into(), "hello world".into())));
    }

    #[test]
    fn test_parse_event_line() {
        let result = parse_sse_line("event: message");
        assert_eq!(result, Some(("event".into(), "message".into())));
    }

    #[test]
    fn test_parse_no_colon() {
        assert_eq!(parse_sse_line("no colon here"), None);
    }

    #[test]
    fn test_parse_empty_string() {
        assert_eq!(parse_sse_line(""), None);
    }

    #[test]
    fn test_parse_colon_only() {
        let result = parse_sse_line(":");
        assert_eq!(result, Some(("".into(), "".into())));
    }

    #[test]
    fn test_parse_value_with_colon() {
        let result = parse_sse_line("data: hello: world");
        assert_eq!(result, Some(("data".into(), "hello: world".into())));
    }

    #[test]
    fn test_parse_no_space_after_colon() {
        let result = parse_sse_line("data:nospace");
        assert_eq!(result, Some(("data".into(), "nospace".into())));
    }

    #[test]
    fn test_parse_multiple_spaces() {
        let result = parse_sse_line("data:   lots of space");
        assert_eq!(result, Some(("data".into(), "lots of space".into())));
    }
}
