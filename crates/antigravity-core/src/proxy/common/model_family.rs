//! Model family classification for type-safe model dispatch.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFamily {
    Opus,
    Sonnet,
    Haiku,
    Flash,
    Pro,
    Unknown,
}

impl ModelFamily {
    pub fn from_model_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("opus") {
            Self::Opus
        } else if lower.contains("sonnet") {
            Self::Sonnet
        } else if lower.contains("haiku") {
            Self::Haiku
        } else if lower.contains("flash") {
            Self::Flash
        } else if lower.contains("pro") {
            Self::Pro
        } else {
            Self::Unknown
        }
    }

    #[inline]
    pub fn is_claude(self) -> bool {
        matches!(self, Self::Opus | Self::Sonnet | Self::Haiku)
    }

    #[inline]
    pub fn is_gemini(self) -> bool {
        matches!(self, Self::Flash | Self::Pro)
    }

    #[inline]
    pub fn is_premium(self) -> bool {
        matches!(self, Self::Opus | Self::Pro)
    }
}
