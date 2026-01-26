// Gemini mapper 模块
// 负责 v1internal 包装/解包

pub mod collector;
pub mod models;
pub mod wrapper;

pub use collector::collect_stream_to_json;
pub use wrapper::*;
