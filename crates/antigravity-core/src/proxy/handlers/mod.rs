// Handlers module - API endpoint handler
// core endpoint handler module

pub mod audio;
pub mod claude;
pub mod gemini;
pub mod mcp;
pub mod mcp_forward;
pub mod mcp_vision;
pub mod model_detect;
pub mod openai;
pub mod warmup;

#[cfg(test)]
mod retry_tests;
