# antigravity-types

Core types, models, and error definitions for Antigravity Manager.

## Overview

This crate provides the foundational type system for the Antigravity ecosystem. All types are designed to be:

- **Serializable** via serde for API/IPC communication
- **Clone** for cheap sharing across async boundaries
- **PartialEq** for testing and comparison
- **Display** for logging and error messages

## Modules

### `error`

Typed error hierarchy with domain-specific error types:

- `AccountError` - Account-related errors (not found, disabled, token expired)
- `ProxyError` - Proxy operation errors (rate limited, upstream unavailable, circuit open)
- `ConfigError` - Configuration errors (not found, parse error, validation error)
- `TypedError` - Unified error type that wraps all domain errors

### `models`

Domain models for the application:

- `Account` - User account with tokens and quota
- `TokenData` - OAuth token information
- `QuotaData` - Model usage quota tracking
- `AppConfig` / `ProxyConfig` - Application configuration
- `ProxyStats` / `ProxyRequestLog` - Monitoring and logging

### `protocol`

API protocol type definitions:

- `openai` - OpenAI ChatCompletions API types
- `claude` - Anthropic Claude Messages API types
- `gemini` - Google Gemini GenerateContent API types

## Usage

```rust
use antigravity_types::{
    AccountError, ProxyError, TypedError,
    Account, TokenData, ProxyConfig,
};

// Create typed errors
let err = ProxyError::RateLimited {
    provider: "claude".to_string(),
    retry_after_secs: Some(60),
};

// Check error properties
if err.should_rotate_account() {
    // Handle account rotation
}

// Get HTTP status code
let status = err.http_status_code(); // 429
```

## Dependency Graph

```
                antigravity-types (this crate)
                       │
      ┌────────────────┼────────────────┐
      ▼                ▼                ▼
antigravity-proxy  antigravity-accounts  ...
      │                │
      └────────┬───────┘
               ▼
        antigravity-server
```

## License

CC-BY-NC-SA-4.0
