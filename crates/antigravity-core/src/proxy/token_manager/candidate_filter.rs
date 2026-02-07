use super::TokenManager;
use crate::proxy::AdaptiveLimitManager;
use std::collections::HashSet;
use std::sync::Arc;

use super::proxy_token::ProxyToken;

impl TokenManager {
    /// Unified eligibility check for candidate accounts.
    ///
    /// Checks (in order):
    /// 1. Already attempted in this request cycle
    /// 2. Rate-limited for the target model
    /// 3. Quota-protected for the target model (if `check_quota` is true)
    /// 4. AIMD usage ratio exceeds 1.2 threshold (if `check_aimd` is true)
    pub fn is_candidate_eligible(
        &self,
        candidate: &ProxyToken,
        model: &str,
        attempted: &HashSet<String>,
        quota_protection_enabled: bool,
        aimd: &Option<Arc<AdaptiveLimitManager>>,
        check_quota: bool,
        check_aimd: bool,
    ) -> bool {
        if attempted.contains(&candidate.email) {
            return false;
        }
        if self.is_rate_limited_for_model(&candidate.email, model) {
            return false;
        }
        if check_quota
            && quota_protection_enabled
            && self.is_model_protected(&candidate.account_id, model)
        {
            return false;
        }
        if check_aimd {
            if let Some(aimd) = aimd {
                if aimd.usage_ratio(&candidate.email) > 1.2 {
                    return false;
                }
            }
        }
        true
    }
}
