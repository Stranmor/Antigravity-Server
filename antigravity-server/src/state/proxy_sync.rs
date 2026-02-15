//! Proxy assignment synchronization methods for AppState.

use antigravity_types::{ProxyAssignment, SyncableProxyAssignments};

use super::{current_timestamp_ms, get_instance_id, AppState};
use crate::api::proxy_health::validate_proxy_url;

fn filter_valid_assignments(source: &SyncableProxyAssignments) -> SyncableProxyAssignments {
    let entries = source
        .entries
        .iter()
        .filter(|(_, entry)| match &entry.proxy_url {
            None => true,
            Some(url) => {
                if validate_proxy_url(url).is_err() {
                    tracing::warn!("Rejected invalid proxy URL from remote sync: {url}");
                    false
                } else {
                    true
                }
            },
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    SyncableProxyAssignments { entries, instance_id: source.instance_id.clone() }
}

impl AppState {
    pub async fn update_proxy_assignment(&self, email: &str, proxy_url: Option<String>) {
        let mut assignments = self.inner.proxy_assignments.write().await;
        let now = current_timestamp_ms();
        let ts =
            assignments.entries.get(email).map_or(now, |e| now.max(e.updated_at.saturating_add(1)));
        drop(
            assignments
                .entries
                .insert(email.to_string(), ProxyAssignment { proxy_url, updated_at: ts }),
        );
    }

    pub async fn get_syncable_proxy_assignments(&self) -> SyncableProxyAssignments {
        let assignments = self.inner.proxy_assignments.read().await;
        SyncableProxyAssignments {
            entries: assignments.entries.clone(),
            instance_id: Some(get_instance_id()),
        }
    }

    pub async fn sync_proxy_with_remote(
        &self,
        remote: &SyncableProxyAssignments,
    ) -> (usize, SyncableProxyAssignments) {
        let filtered = filter_valid_assignments(remote);

        let (inbound, diff, snapshot) = {
            let mut assignments = self.inner.proxy_assignments.write().await;
            let diff = assignments.diff_newer_than(&filtered);
            let inbound = assignments.merge_lww(&filtered);
            let snapshot = assignments.clone();
            (inbound, diff, snapshot)
        }; // Lock dropped here — I/O below runs without holding write lock

        if inbound > 0 {
            self.apply_proxy_assignments_to_accounts(&snapshot).await;
        }

        (inbound, diff)
    }

    pub async fn merge_remote_proxy_assignments(&self, remote: &SyncableProxyAssignments) -> usize {
        let filtered = filter_valid_assignments(remote);

        let (updated, snapshot) = {
            let mut assignments = self.inner.proxy_assignments.write().await;
            let updated = assignments.merge_lww(&filtered);
            let snapshot = assignments.clone();
            (updated, snapshot)
        }; // Lock dropped here

        if updated > 0 {
            tracing::info!(
                "Merged {} proxy assignment entries from remote (instance: {:?})",
                updated,
                remote.instance_id
            );
            self.apply_proxy_assignments_to_accounts(&snapshot).await;
        }

        updated
    }

    pub async fn hydrate_proxy_assignments(&self) {
        let accounts = match self.list_accounts().await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Failed to hydrate proxy assignments: {e}");
                return;
            },
        };

        let mut assignments = self.inner.proxy_assignments.write().await;
        let now = current_timestamp_ms();

        for account in &accounts {
            if account.proxy_url.is_some() {
                assignments.entries.entry(account.email.clone()).or_insert(ProxyAssignment {
                    proxy_url: account.proxy_url.clone(),
                    updated_at: now,
                });
            }
        }

        tracing::info!("Hydrated {} proxy assignments from accounts", assignments.entries.len());
    }

    async fn apply_proxy_assignments_to_accounts(&self, assignments: &SyncableProxyAssignments) {
        let accounts = match self.list_accounts().await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to list accounts for proxy sync: {e}");
                return;
            },
        };

        for account in &accounts {
            if let Some(entry) = assignments.entries.get(&account.email) {
                let new_proxy = entry.proxy_url.as_deref();
                let current_proxy = account.proxy_url.as_deref();

                if new_proxy != current_proxy {
                    if let Some(repo) = self.repository() {
                        if let Err(e) = repo.update_proxy_url(&account.id, new_proxy).await {
                            tracing::warn!(
                                "Failed to sync proxy_url for {} to DB: {e}",
                                account.email
                            );
                        }
                    }

                    let acc_id = account.id.clone();
                    let purl = new_proxy.map(String::from);
                    if let Err(e) = tokio::task::spawn_blocking(move || {
                        let mut acc = antigravity_core::modules::account::load_account(&acc_id)?;
                        acc.proxy_url = purl;
                        antigravity_core::modules::account::save_account(&acc)
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("spawn_blocking panicked: {e}")))
                    {
                        tracing::warn!(
                            "Failed to sync proxy_url for {} to JSON: {e}",
                            account.email
                        );
                    }
                }
            }
        }

        drop(self.reload_accounts().await);
    }
}
