use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};

use crate::state::AppState;

const SYNC_INTERVAL_SECS: u64 = 60;

pub fn start_auto_config_sync(state: Arc<AppState>, remote_url: String) {
    let url_for_log = remote_url.clone();

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("❌ Failed to create HTTP client for config sync: {}", e);
            return;
        }
    };

    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(SYNC_INTERVAL_SECS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

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
    let fetch_url = format!("{}/api/config/mapping", remote_url.trim_end_matches('/'));
    let push_url = format!("{}/api/config/mapping", remote_url.trim_end_matches('/'));

    let remote_mapping: antigravity_types::SyncableMapping = client
        .get(&fetch_url)
        .send()
        .await
        .map_err(|e| format!("fetch failed: {}", e))?
        .json()
        .await
        .map_err(|e| format!("parse failed: {}", e))?;

    let (inbound_updated, diff) = state.sync_with_remote(&remote_mapping).await;
    let outbound_count = diff.len();

    if outbound_count > 0 {
        let push_body = serde_json::json!({
            "mapping": diff
        });

        let resp = client
            .post(&push_url)
            .json(&push_body)
            .send()
            .await
            .map_err(|e| format!("push failed: {}", e))?;

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

    Ok(())
}
