use crate::error::ClientError;
use crate::types::*;
use futures::Stream;
use reqwest::Client;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::StreamExt;

pub struct AntigravityClient {
    client: Client,
    config: ClientConfig,
}

impl AntigravityClient {
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()?;
        Ok(Self { client, config })
    }

    pub async fn auto_discover() -> Result<Self, ClientError> {
        let candidates = discovery_candidates();
        for base_url in candidates {
            if let Ok(client) = Self::try_connect(&base_url).await {
                tracing::info!("Connected to Antigravity at {}", base_url);
                return Ok(client);
            }
        }
        Err(ClientError::ServerNotFound)
    }

    async fn try_connect(base_url: &str) -> Result<Self, ClientError> {
        let config = ClientConfig {
            base_url: base_url.to_string(),
            ..Default::default()
        };
        let client = Self::new(config)?;
        let resp = client
            .client
            .get(format!("{}/api/proxy/status", client.config.base_url))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map_err(|e| ClientError::Connection(e.to_string()))?;
        if resp.status().is_success() {
            Ok(client)
        } else {
            Err(ClientError::Connection(format!(
                "Health check failed: {}",
                resp.status()
            )))
        }
    }

    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ClientError> {
        self.chat_with_retry(request).await
    }

    async fn chat_with_retry(&self, request: ChatRequest) -> Result<ChatResponse, ClientError> {
        let mut attempts = 0;
        let mut delay = self.config.retry.base_delay_ms;

        loop {
            attempts += 1;
            match self.chat_once(&request).await {
                Ok(response) => return Ok(response),
                Err(ClientError::RateLimited { retry_after }) => {
                    if attempts > self.config.retry.max_retries {
                        return Err(ClientError::Timeout(attempts));
                    }
                    let wait = retry_after.unwrap_or(delay / 1000).max(1);
                    tracing::debug!("Rate limited, waiting {}s (attempt {})", wait, attempts);
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    delay = (delay * 2).min(self.config.retry.max_delay_ms);
                }
                Err(ClientError::ServerError { status, .. }) if status >= 500 => {
                    if attempts > self.config.retry.max_retries {
                        return Err(ClientError::Timeout(attempts));
                    }
                    tracing::debug!("Server error {}, retrying (attempt {})", status, attempts);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    delay = (delay * 2).min(self.config.retry.max_delay_ms);
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn chat_once(&self, request: &ChatRequest) -> Result<ChatResponse, ClientError> {
        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok());
            return Err(ClientError::RateLimited { retry_after });
        }

        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(ClientError::ServerError {
                status: status.as_u16(),
                message,
            });
        }

        resp.json()
            .await
            .map_err(|e| ClientError::InvalidResponse(e.to_string()))
    }

    pub async fn chat_stream(
        &self,
        mut request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, ClientError>> + Send>>, ClientError>
    {
        request.stream = Some(true);

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok());
            return Err(ClientError::RateLimited { retry_after });
        }

        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(ClientError::ServerError {
                status: status.as_u16(),
                message,
            });
        }

        let byte_stream = resp.bytes_stream();
        let stream = byte_stream.map(|chunk| {
            let bytes = chunk.map_err(|e| ClientError::Stream(e.to_string()))?;
            parse_sse_chunk(&bytes)
        });

        Ok(Box::pin(stream))
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
}

fn discovery_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(url) = std::env::var("ANTIGRAVITY_URL") {
        candidates.push(url);
    }
    if let Ok(port) = std::env::var("ANTIGRAVITY_PORT") {
        candidates.push(format!("http://127.0.0.1:{}", port));
    }
    candidates.push("http://127.0.0.1:8045".to_string());
    candidates.push("http://127.0.0.1:8046".to_string());
    candidates
}

fn parse_sse_chunk(bytes: &[u8]) -> Result<StreamChunk, ClientError> {
    let text = std::str::from_utf8(bytes).map_err(|e| ClientError::Stream(e.to_string()))?;

    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            let trimmed = data.trim();
            if trimmed.is_empty() || trimmed.starts_with(':') {
                continue;
            }
            return serde_json::from_str(trimmed)
                .map_err(|e| ClientError::Stream(format!("JSON parse error: {}", e)));
        }
    }

    Err(ClientError::Stream("No data in SSE chunk".to_string()))
}
