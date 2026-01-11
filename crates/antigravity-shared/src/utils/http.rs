use serde::{Deserialize, Serialize};
use validator::Validate;

/// Upstream proxy configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Validate)]
pub struct UpstreamProxyConfig {
    pub enabled: bool,
    #[validate(url)]
    pub url: String,
}
