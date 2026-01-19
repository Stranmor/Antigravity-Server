# antigravity-client

Rust SDK for Antigravity Manager API with auto-discovery and retry logic.

## Usage

```rust
use antigravity_client::{AntigravityClient, ChatRequest, ChatMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Auto-discover local Antigravity server
    let client = AntigravityClient::auto_discover().await?;

    // Simple chat request
    let request = ChatRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        messages: vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
            },
        ],
        max_tokens: Some(1024),
        temperature: None,
        stream: None,
    };

    let response = client.chat(request).await?;
    println!("{:?}", response);

    Ok(())
}
```

## Features

- **Auto-discovery**: Finds local Antigravity server via env vars or default ports
- **Retry with backoff**: Automatic retry on 429/5xx with exponential backoff
- **Streaming**: SSE streaming support for real-time responses
- **Type-safe**: Fully typed request/response models
