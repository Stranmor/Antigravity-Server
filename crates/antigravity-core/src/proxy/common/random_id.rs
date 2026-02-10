// toolfunction

pub fn generate_random_id() -> String {
    use rand::Rng;
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}

/// based onmodelnameinferfeaturetype
// note：thisfunctionalreadydeprecated，pleaseuse instead mappers::request_config::resolve_request_config
pub fn _deprecated_infer_quota_group(model: &str) -> String {
    antigravity_types::ModelFamily::from_model_name(model).as_str().to_string()
}
