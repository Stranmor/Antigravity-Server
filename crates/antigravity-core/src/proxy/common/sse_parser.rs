/// Parse a single SSE line into (key, value) pair.
///
/// SSE format: `key: value\n`
pub fn parse_sse_line(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let key = &line[..colon_pos];
    let value = line[colon_pos + 1..].trim_start();
    Some((key.to_string(), value.to_string()))
}
