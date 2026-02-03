//! Configuration synchronization methods for AppState

use super::{current_timestamp_ms, get_instance_id, AppState};

impl AppState {
    pub async fn get_syncable_mapping(&self) -> antigravity_types::SyncableMapping {
        use antigravity_types::MappingEntry;

        let mapping = self.inner.custom_mapping.read().await;
        let timestamps = self.inner.mapping_timestamps.read().await;

        let entries = mapping
            .iter()
            .map(|(k, v)| {
                let ts = timestamps
                    .get(k)
                    .copied()
                    .unwrap_or_else(current_timestamp_ms);
                (k.clone(), MappingEntry::with_timestamp(v.clone(), ts))
            })
            .collect();

        antigravity_types::SyncableMapping {
            entries,
            instance_id: Some(get_instance_id()),
        }
    }

    pub async fn sync_with_remote(
        &self,
        remote: &antigravity_types::SyncableMapping,
    ) -> (usize, antigravity_types::SyncableMapping) {
        use antigravity_types::MappingEntry;

        let (mapping_to_persist, inbound, diff) = {
            let mut mapping = self.inner.custom_mapping.write().await;
            let mut timestamps = self.inner.mapping_timestamps.write().await;

            let local_entries: std::collections::HashMap<_, _> = mapping
                .iter()
                .map(|(k, v)| {
                    let ts = timestamps.get(k).copied().unwrap_or(0);
                    (k.clone(), MappingEntry::with_timestamp(v.clone(), ts))
                })
                .collect();
            let local_mapping = antigravity_types::SyncableMapping {
                entries: local_entries,
                instance_id: Some(get_instance_id()),
            };

            let diff = local_mapping.diff_newer_than(remote);

            let mut updated = 0;
            for (key, remote_entry) in &remote.entries {
                let local_ts = timestamps.get(key).copied().unwrap_or(0);
                if remote_entry.updated_at > local_ts {
                    mapping.insert(key.clone(), remote_entry.target.clone());
                    timestamps.insert(key.clone(), remote_entry.updated_at);
                    updated += 1;
                }
            }

            let persist = if updated > 0 {
                Some(mapping.clone())
            } else {
                None
            };

            (persist, updated, diff)
        };

        if let Some(ref map) = mapping_to_persist {
            tracing::info!(
                "Merged {} mapping entries from remote (instance: {:?})",
                inbound,
                remote.instance_id
            );
            if let Err(e) = self.persist_mapping_to_config(map).await {
                tracing::error!("Failed to persist mapping to config: {}", e);
            }
        }

        (inbound, diff)
    }

    pub async fn merge_remote_mapping(&self, remote: &antigravity_types::SyncableMapping) -> usize {
        let mapping_to_persist = {
            let mut mapping = self.inner.custom_mapping.write().await;
            let mut timestamps = self.inner.mapping_timestamps.write().await;

            let mut updated = 0;

            for (key, remote_entry) in &remote.entries {
                let local_ts = timestamps.get(key).copied().unwrap_or(0);

                if remote_entry.updated_at > local_ts {
                    mapping.insert(key.clone(), remote_entry.target.clone());
                    timestamps.insert(key.clone(), remote_entry.updated_at);
                    updated += 1;
                }
            }

            if updated > 0 {
                tracing::info!(
                    "Merged {} mapping entries from remote (instance: {:?})",
                    updated,
                    remote.instance_id
                );
                Some((mapping.clone(), updated))
            } else {
                None
            }
        };

        if let Some((mapping, updated)) = mapping_to_persist {
            if let Err(e) = self.persist_mapping_to_config(&mapping).await {
                tracing::error!("Failed to persist mapping to config: {}", e);
            }
            updated
        } else {
            0
        }
    }

    pub(crate) async fn persist_mapping_to_config(
        &self,
        mapping: &std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        use antigravity_core::modules::config as core_config;

        let mapping_clone = mapping.clone();
        tokio::task::spawn_blocking(move || {
            core_config::update_config(|config| {
                config.proxy.custom_mapping = mapping_clone;
            })
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {}", e))??;

        let mut proxy_config = self.inner.proxy_config.write().await;
        proxy_config.custom_mapping = mapping.clone();

        Ok(())
    }
}
