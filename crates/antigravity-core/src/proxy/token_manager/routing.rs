use super::TokenManager;
use crate::proxy::routing_config::SmartRoutingConfig;

impl TokenManager {
    pub async fn set_routing_config(&self, config: SmartRoutingConfig) {
        let mut guard = self.routing_config.write().await;
        *guard = config;
    }

    pub async fn get_routing_config(&self) -> SmartRoutingConfig {
        self.routing_config.read().await.clone()
    }

    pub async fn update_routing_config(&self, new_config: SmartRoutingConfig) {
        let mut config = self.routing_config.write().await;
        tracing::debug!("Smart routing configuration updated: {:?}", new_config);
        *config = new_config;
    }

    pub fn is_model_protected(&self, account_id: &str, model: &str) -> bool {
        if let Some(token) = self.tokens.get(account_id) {
            return token.protected_models.contains(model);
        }
        false
    }

    pub async fn set_preferred_account(&self, account_id: Option<String>) {
        let mut preferred = self.preferred_account_id.write().await;
        if let Some(ref id) = account_id {
            tracing::info!("Fixed account mode enabled: {}", id);
        } else {
            tracing::info!("Round-robin mode enabled (no preferred account)");
        }
        *preferred = account_id;
    }

    pub async fn get_preferred_account(&self) -> Option<String> {
        self.preferred_account_id.read().await.clone()
    }
}
