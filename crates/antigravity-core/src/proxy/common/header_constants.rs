//! Standard HTTP header names used across proxy handlers.

/// Header containing the email of the account that served the request.
pub const X_ACCOUNT_EMAIL: &str = "X-Account-Email";
/// Header containing the model name after alias resolution.
pub const X_MAPPED_MODEL: &str = "X-Mapped-Model";
/// Header explaining why a model was mapped (alias, override, etc.).
pub const X_MAPPING_REASON: &str = "X-Mapping-Reason";
/// Header to force routing to a specific account by email.
pub const X_FORCE_ACCOUNT: &str = "X-Force-Account";
