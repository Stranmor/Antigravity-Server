use crate::proxy::upstream::client::UpstreamClient;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, warn};

pub enum UpstreamResult {
    Success(reqwest::Response),
    ConnectionError(String),
}

pub async fn call_upstream_with_retry(
    upstream: Arc<UpstreamClient>,
    method: &str,
    access_token: &str,
    gemini_body: Value,
    query_string: Option<&str>,
    warp_proxy: Option<&str>,
    email: &str,
    attempt: usize,
    max_attempts: usize,
) -> UpstreamResult {
    let mut inner_retries = 0u8;
    loop {
        let gemini_body_clone = gemini_body.clone();
        match upstream
            .call_v1_internal_with_warp(
                method,
                access_token,
                gemini_body_clone,
                query_string,
                std::collections::HashMap::new(),
                warp_proxy,
            )
            .await
        {
            Ok(r) => {
                let status = r.status();
                if status.as_u16() == 503 && inner_retries < 5 {
                    inner_retries += 1;
                    let delay = 300 * (1u64 << inner_retries.min(3));
                    warn!(
                        "503 server overload on {}, inner retry {}/5 in {}ms",
                        email, inner_retries, delay
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                    continue;
                }
                return UpstreamResult::Success(r);
            },
            Err(e) => {
                debug!("OpenAI Request failed on attempt {}/{}: {}", attempt + 1, max_attempts, e);
                return UpstreamResult::ConnectionError(e);
            },
        }
    }
}
