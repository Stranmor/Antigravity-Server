// Random ID helpers.

pub fn generate_random_id() -> String {
    use rand::Rng;
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

/// Infer quota group from model name.
// NOTE: Deprecated. Use mappers::request_config::resolve_request_config instead.
#[deprecated(note = "Use mappers::request_config::resolve_request_config instead.")]
pub fn _deprecated_infer_quota_group(model: &str) -> String {
    antigravity_types::ModelFamily::from_model_name(model).as_str().to_string()
}

#[cfg(test)]
mod tests {
    use super::generate_random_id;

    #[test]
    fn generate_random_id_has_expected_length() {
        let id = generate_random_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
