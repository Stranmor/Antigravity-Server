//! TOS Ban detection and permanent lockout for banned accounts.
//!
//! Detects Google Terms of Service violations and permanently locks out
//! banned accounts to prevent wasting requests on them.
//!
//! Known ban patterns:
//! - "Gemini has been disabled in this account for violation of Terms of Service"
//! - "Your access to Gemini Code Assist is suspended"
//! - "PERMISSION_DENIED" with "Terms of Service" mention
//! - "USER_DISABLED" status

/// Duration for TOS-banned account lockout (24 hours).
/// These accounts are effectively permanently banned by Google
/// so a very long lockout prevents wasting requests.
pub const TOS_BAN_LOCKOUT_SECS: u64 = 86400; // 24 hours

/// Patterns that indicate a TOS ban (account permanently disabled by Google).
const TOS_BAN_PATTERNS: &[&str] = &[
    "violation of Terms of Service",
    "Terms of Service violation",
    "access to Gemini Code Assist is suspended",
    "has been disabled in this account",
    "USER_DISABLED",
    "account has been suspended",
    "account is disabled",
    "account has been disabled",
    "TOS_VIOLATION",
    "ABUSE_DETECTED",
];

/// Patterns that indicate the account needs re-verification but is not banned.
/// These get a shorter lockout (handled by existing 1h lockout logic).
const VERIFICATION_PATTERNS: &[&str] = &[
    "verify your account",
    "CONSUMER_INVALID",
    "SERVICE_DISABLED",
    "Permission denied on resource project",
];

/// Check if an error response indicates a TOS ban.
///
/// TOS bans are different from SERVICE_DISABLED or CONSUMER_INVALID:
/// they indicate the account is permanently banned by Google and
/// will never recover without manual intervention.
pub fn is_tos_banned(status_code: u16, error_text: &str) -> bool {
    if status_code != 403 {
        return false;
    }
    TOS_BAN_PATTERNS.iter().any(|pattern| error_text.contains(pattern))
}

/// Check if an error indicates the account needs verification (not banned).
pub fn needs_verification(status_code: u16, error_text: &str) -> bool {
    if status_code != 403 {
        return false;
    }
    VERIFICATION_PATTERNS.iter().any(|pattern| error_text.contains(pattern))
}

/// Classify a 403 error into a specific subtype for appropriate handling.
#[derive(Debug, PartialEq)]
pub enum ForbiddenReason {
    /// Account is TOS-banned — lock out for 24 hours
    TosBanned,
    /// Account needs verification or has project issues — 1h lockout
    NeedsVerification,
    /// Other 403 — standard 30s model lockout
    Other,
}

/// Classify a 403 error response.
pub fn classify_403(error_text: &str) -> ForbiddenReason {
    if TOS_BAN_PATTERNS.iter().any(|p| error_text.contains(p)) {
        ForbiddenReason::TosBanned
    } else if VERIFICATION_PATTERNS.iter().any(|p| error_text.contains(p)) {
        ForbiddenReason::NeedsVerification
    } else {
        ForbiddenReason::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tos_ban_detected() {
        assert!(is_tos_banned(
            403,
            "Gemini has been disabled in this account for violation of Terms of Service"
        ));
        assert!(is_tos_banned(403, "Your access to Gemini Code Assist is suspended"));
        assert!(is_tos_banned(403, "USER_DISABLED: account is banned"));
        assert!(is_tos_banned(403, "TOS_VIOLATION detected"));
        assert!(is_tos_banned(403, "ABUSE_DETECTED for this account"));
    }

    #[test]
    fn test_verification_not_tos_ban() {
        assert!(!is_tos_banned(403, "SERVICE_DISABLED for project X"));
        assert!(!is_tos_banned(403, "CONSUMER_INVALID"));
        assert!(!is_tos_banned(403, "verify your account"));
    }

    #[test]
    fn test_non_403_not_tos_ban() {
        assert!(!is_tos_banned(429, "violation of Terms of Service"));
        assert!(!is_tos_banned(401, "USER_DISABLED"));
    }

    #[test]
    fn test_classify_403() {
        assert_eq!(classify_403("violation of Terms of Service"), ForbiddenReason::TosBanned);
        assert_eq!(
            classify_403("SERVICE_DISABLED for project X"),
            ForbiddenReason::NeedsVerification
        );
        assert_eq!(classify_403("some other 403 error"), ForbiddenReason::Other);
    }

    #[test]
    fn test_needs_verification() {
        assert!(needs_verification(403, "verify your account"));
        assert!(needs_verification(403, "CONSUMER_INVALID"));
        assert!(!needs_verification(403, "violation of Terms of Service"));
        assert!(!needs_verification(401, "CONSUMER_INVALID"));
    }
}
