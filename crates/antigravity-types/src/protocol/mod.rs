//! Protocol definitions for API providers.
//!
//! This module contains type definitions for various LLM API protocols:
//! - OpenAI (ChatCompletions API)
//! - Anthropic (Claude Messages API)
//! - Google Gemini (GenerateContent API)
//!
//! These are placeholder modules - full protocol types will be added as needed.

pub mod claude;
pub mod gemini;
pub mod openai;

// Re-export common protocol enums
pub use claude::ClaudeRole;
pub use openai::OpenAIRole;
