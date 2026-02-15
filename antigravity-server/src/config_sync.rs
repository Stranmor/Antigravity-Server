use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};

use antigravity_core::proxy::common::client_builder::build_http_client;

use crate::state::AppState;

const SYNC_INTERVAL_SECS: u64 = 60;

pub fn start_auto_config_sync(state: Arc<AppState>, remote_url: String) {
    let url_for_log = remote_url.clone();

    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(SYNC_INTERVAL_SECS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            // Read current proxy config each iteration to respect hot-reload + enforce_proxy
            let upstream_proxy = state.inner.upstream_proxy.read().await.clone();
            let client = match build_http_client(Some(&upstream_proxy), 30) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("⚠️ Config sync: failed to build HTTP client: {e}");
                    continue;
                },
            };

            if let Err(e) = sync_once(&state, &client, &remote_url).await {
                tracing::warn!("⚠️ Config sync failed: {}", e);
            }
        }
    });
    tracing::info!(
        "✅ Config sync task started (remote: {}, interval: {}s)",
        url_for_log,
        SYNC_INTERVAL_SECS
    );
}

async fn sync_once(
    state: &AppState,
    client: &reqwest::Client,
    remote_url: &str,
) -> Result<(), String> {
    let base_url = remote_url.trim_end_matches('/');
    let url = format!("{base_url}/api/config/mapping");

    let api_key = state.inner.security_config.read().await.api_key.clone();

    let remote_mapping: antigravity_types::SyncableMapping = client
        .get(&url)
        .bearer_auth(&api_key)
        .send()
        .await
        .map_err(|e| format!("fetch failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("parse failed: {e}"))?;

    let (inbound_updated, diff) = state.sync_with_remote(&remote_mapping).await;
    let outbound_count = diff.len();

    if outbound_count > 0 {
        let push_body = serde_json::json!({
            "mapping": diff
        });

        let resp = client
            .post(&url)
            .bearer_auth(&api_key)
            .json(&push_body)
            .send()
            .await
            .map_err(|e| format!("push failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("push returned {}", resp.status()));
        }
    }

    if inbound_updated > 0 || outbound_count > 0 {
        tracing::info!(
            "🔄 Config sync: {} inbound, {} outbound entries",
            inbound_updated,
            outbound_count
        );
    } else {
        tracing::debug!("🔄 Config sync: no changes");
    }

    sync_proxy_assignments(state, client, base_url, &api_key).await?;

    Ok(())
}

async fn sync_proxy_assignments(
    state: &AppState,
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<(), String> {
    let url = format!("{base_url}/api/config/proxy-assignments");

    let remote: antigravity_types::SyncableProxyAssignments = client
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| format!("proxy-assignments fetch failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("proxy-assignments parse failed: {e}"))?;

    let (inbound, diff) = state.sync_proxy_with_remote(&remote).await;
    let outbound = diff.len();

    if outbound > 0 {
        let push_body = serde_json::json!({ "assignments": diff });

        let resp = client
            .post(&url)
            .bearer_auth(api_key)
            .json(&push_body)
            .send()
            .await
            .map_err(|e| format!("proxy-assignments push failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("proxy-assignments push returned {}", resp.status()));
        }
    }

    if inbound > 0 || outbound > 0 {
        tracing::info!("🔄 Proxy assignment sync: {} inbound, {} outbound", inbound, outbound);
    }

    Ok(())
}
