# Advanced Configuration and Experimental Features

Antigravity v3.3.16 introduces `ExperimentalConfig`, a set of experimental feature toggles enabled by default, designed to enhance system robustness and compatibility. These configurations are located in `src-tauri/src/proxy/config.rs` and are currently not exposed to the UI.

## Feature List

### 1. Dual-Layer Signature Cache (Signature Cache)
*   **Configuration Item**: `enable_signature_cache`
*   **Default Value**: `true`
*   **Description**: When enabled, the system caches `ToolUse ID` and `Thought Signature` mappings.
*   **Purpose**: Addresses issues where some clients (e.g., Claude Desktop CLI, Cherry Studio) might lose historical Tool Call signatures in multi-turn conversations. When the upstream API returns a "Missing signature" error, the system can automatically recover from the cache, preventing conversation interruptions.

### 2. Tool Loop Automatic Recovery (Tool Loop Recovery)
*   **Configuration Item**: `enable_tool_loop_recovery`
*   **Default Value**: `true`
*   **Description**: When enabled, the system monitors conversation status in real-time to detect "deadlock" patterns.
*   **Trigger Condition**: Detects continuous `ToolUse` -> `ToolResult` loops where the `Assistant` message lacks a `Thinking` block (usually due to signature validation failure stripping).
*   **Behavior**: Automatically injects synthetic messages (`Assistant: Tool execution completed.` -> `User: Proceed.`) to break the infinite loop, forcing the model into the next round of thinking.

### 3. Cross-Model Compatibility Checks (Cross-Model Checks)
*   **Configuration Item**: `enable_cross_model_checks`
*   **Default Value**: `true`
*   **Description**: Prevents signature errors caused by switching between different model families (e.g., Claude -> Gemini) within the same session.
*   **Purpose**: When a signature in historical messages is detected to belong to an incompatible model family (e.g., `claude-3-5` vs `gemini-2.0`), the system automatically discards the old signature, preventing API request rejections.

## Custom Configuration

Currently, these configuration items can be adjusted by modifying the `default_true` default value in `src-tauri/src/proxy/config.rs`, or by waiting for future versions to integrate them into the "Settings -> Advanced" interface.

```rust
// src-tauri/src/proxy/config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentalConfig {
    #[serde(default = "default_true")]
    pub enable_signature_cache: bool,
    // ...
}
```
