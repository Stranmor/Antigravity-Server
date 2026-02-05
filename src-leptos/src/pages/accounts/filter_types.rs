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

pub(crate) fn needs_phone_verification(reason: Option<&String>) -> bool {
    reason.is_some_and(|r| r == "phone_verification_required")
}
