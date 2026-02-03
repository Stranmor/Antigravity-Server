mod monolith;
mod part_processor;
mod signature_manager;
mod tool_remapping;

pub use monolith::*;
pub use part_processor::PartProcessor;
pub use signature_manager::SignatureManager;
pub use tool_remapping::remap_function_call_args;
