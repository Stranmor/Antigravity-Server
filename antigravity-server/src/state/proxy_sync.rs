//! Proxy assignment synchronization methods for AppState.

use antigravity_types::{ProxyAssignment, SyncableProxyAssignments};

use super::{current_timestamp_ms, get_instance_id, AppState};

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
        let mut assignments = self.inner.proxy_assignments.write().await;

        let diff = assignments.diff_newer_than(remote);
        let inbound = assignments.merge_lww(remote);

        if inbound > 0 {
            self.apply_proxy_assignments_to_accounts(&assignments).await;
        }

        (inbound, diff)
    }

    pub async fn merge_remote_proxy_assignments(&self, remote: &SyncableProxyAssignments) -> usize {
        let mut assignments = self.inner.proxy_assignments.write().await;
        let updated = assignments.merge_lww(remote);

        if updated > 0 {
            tracing::info!(
                "Merged {} proxy assignment entries from remote (instance: {:?})",
                updated,
                remote.instance_id
            );
            self.apply_proxy_assignments_to_accounts(&assignments).await;
        }

        updated
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
