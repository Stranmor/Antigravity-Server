#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum ViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum FilterType {
    #[default]
    All,
    Pro,
    Ultra,
    Free,
    NeedsVerification,
}

pub(crate) fn quota_class(percent: i32) -> &'static str {
    match percent {
        0..=20 => "quota-fill--critical",
        21..=50 => "quota-fill--warning",
        _ => "quota-fill--good",
    }
}

pub(crate) fn is_pro_tier(tier: Option<&String>) -> bool {
    tier.is_some_and(|t| t.to_lowercase().contains("pro"))
}

pub(crate) fn is_ultra_tier(tier: Option<&String>) -> bool {
    tier.is_some_and(|t| t.to_lowercase().contains("ultra"))
}

/// Format raw subscription tier ID into a human-readable label.
/// e.g. "g1-ultra-tier" → "Ultra", "g1-pro-tier" → "Pro", None → "Free"
pub(crate) fn format_tier_display(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("ultra") && lower.contains("business") {
        "Business".to_string()
    } else if lower.contains("ultra") {
        "Ultra".to_string()
    } else if lower.contains("pro") {
        "Pro".to_string()
    } else if lower.contains("free") || raw.is_empty() {
        "Free".to_string()
    } else {
        raw.to_string()
    }
}

pub(crate) fn needs_phone_verification(reason: Option<&String>) -> bool {
    reason.is_some_and(|r| r == "phone_verification_required")
}

/// Returns true if the account appears banned:
/// - TOS ban reason set, OR
/// - All model quotas are 0% with no reset times (won't recover)
pub(crate) fn is_account_banned(account: &crate::api_models::Account) -> bool {
    // Check TOS ban reason
    if account.proxy_disabled_reason.as_ref().is_some_and(|r| {
        r.contains("tos_ban") || r.contains("banned") || r.contains("USER_DISABLED")
    }) {
        return true;
    }

    // Check if all quotas are 0% without reset times
    if let Some(ref quota) = account.quota {
        let all_zero = quota.models.iter().all(|m| m.percentage == 0);
        let has_any_reset = quota.models.iter().any(|m| !m.reset_time.is_empty());
        if all_zero && !has_any_reset && !account.disabled && !quota.models.is_empty() {
            return true;
        }
    }

    false
}
