/// Protocol-specific retry backoff configuration.
pub struct RetryProfile {
    pub backoff_429_base_ms: u64,
    pub backoff_429_max_ms: u64,
    pub backoff_503_base_ms: u64,
    pub backoff_503_max_ms: u64,
    pub backoff_500_base_ms: u64,
    pub fixed_401_403_delay_ms: u64,
    pub signature_patterns: &'static [&'static str],
}

const OPENAI_SIGNATURE_PATTERNS: &[&str] = &[
    "Invalid `signature`",
    "thinking.signature",
    "thinking.thinking",
    "Corrupted thought signature",
];

const CLAUDE_SIGNATURE_PATTERNS: &[&str] = &[
    "Invalid `signature`",
    "Invalid signature",
    "thinking.signature",
    "thinking.thinking",
    "thinking.signature: Field required",
    "thinking.thinking: Field required",
    "INVALID_ARGUMENT",
    "Corrupted thought signature",
    "failed to deserialise",
    "thinking block",
    "Found `text`",
    "Found 'text'",
    "must be `thinking`",
    "must be 'thinking'",
];

impl RetryProfile {
    pub const fn openai() -> Self {
        Self {
            backoff_429_base_ms: 5000,
            backoff_429_max_ms: 30_000,
            backoff_503_base_ms: 10_000,
            backoff_503_max_ms: 60_000,
            backoff_500_base_ms: 3000,
            fixed_401_403_delay_ms: 200,
            signature_patterns: OPENAI_SIGNATURE_PATTERNS,
        }
    }

    pub const fn claude() -> Self {
        Self {
            backoff_429_base_ms: 1000,
            backoff_429_max_ms: 10_000,
            backoff_503_base_ms: 1000,
            backoff_503_max_ms: 8000,
            backoff_500_base_ms: 500,
            fixed_401_403_delay_ms: 100,
            signature_patterns: CLAUDE_SIGNATURE_PATTERNS,
        }
    }

    pub const fn gemini() -> Self {
        Self {
            backoff_429_base_ms: 5000,
            backoff_429_max_ms: 30_000,
            backoff_503_base_ms: 10_000,
            backoff_503_max_ms: 60_000,
            backoff_500_base_ms: 3000,
            fixed_401_403_delay_ms: 200,
            signature_patterns: OPENAI_SIGNATURE_PATTERNS,
        }
    }
}
