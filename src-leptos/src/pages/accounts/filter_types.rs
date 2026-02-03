#[derive(Clone, Copy, PartialEq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum FilterType {
    #[default]
    All,
    Pro,
    Ultra,
    Free,
    NeedsVerification,
}

pub fn quota_class(percent: i32) -> &'static str {
    match percent {
        0..=20 => "quota-fill--critical",
        21..=50 => "quota-fill--warning",
        _ => "quota-fill--good",
    }
}

pub fn is_pro_tier(tier: Option<&String>) -> bool {
    tier.is_some_and(|t| t.to_lowercase().contains("pro"))
}

pub fn is_ultra_tier(tier: Option<&String>) -> bool {
    tier.is_some_and(|t| t.to_lowercase().contains("ultra"))
}

pub fn needs_phone_verification(reason: Option<&String>) -> bool {
    reason.is_some_and(|r| r == "phone_verification_required")
}
