//! Model family detection — single point of truth for model name classification.

/// Represents which AI provider family a model belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelFamily {
    /// Google Gemini models (including flash variants)
    Gemini,
    /// Anthropic Claude models (via Vertex AI)
    Claude,
    /// Unknown model family
    Unknown,
}

impl ModelFamily {
    /// Determine model family from model name string.
    ///
    /// This is the SINGLE POINT OF TRUTH for model family detection.
    /// All code that needs to know "is this a Claude model?" or "is this a Gemini model?"
    /// MUST use this function instead of ad-hoc string matching.
    pub fn from_model_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("claude") {
            Self::Claude
        } else if lower.contains("gemini") || lower.contains("flash") {
            Self::Gemini
        } else {
            Self::Unknown
        }
    }

    /// Returns true if this is a Gemini family model.
    pub fn is_gemini(self) -> bool {
        self == Self::Gemini
    }

    /// Returns true if this is a Claude family model.
    pub fn is_claude(self) -> bool {
        self == Self::Claude
    }

    /// Returns the family name as a string (for signature cache keys etc.)
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::Claude => "claude",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ModelFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
