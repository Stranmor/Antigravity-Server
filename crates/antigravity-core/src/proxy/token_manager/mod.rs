use crate::modules::{config, oauth, quota};
use crate::proxy::active_request_guard::ActiveRequestGuard;
use crate::proxy::rate_limit::RateLimitTracker;
use crate::proxy::routing_config::SmartRoutingConfig;
use crate::proxy::AdaptiveLimitManager;
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

mod file_utils;
mod proxy_token;

use file_utils::{atomic_write_json, calculate_max_quota_percentage, truncate_reason};
pub use proxy_token::{AccountTier, ProxyToken};

const SESSION_FAILURE_THRESHOLD: u32 = 3;
const STICKY_UNBIND_RATE_LIMIT_SECONDS: u64 = 15;

/// Manages OAuth tokens for multiple accounts with smart routing and session affinity.
///
/// Key responsibilities:
/// - Load/reload accounts from disk
/// - Smart token selection with least-connections algorithm
/// - Per-account concurrency limiting (prevents thundering herd)
/// - Rate limit tracking per account
/// - Session-to-account binding for cache optimization
/// - AIMD predictive rate limiting integration
pub struct TokenManager {
    tokens: Arc<DashMap<String, ProxyToken>>,
    data_dir: PathBuf,
    rate_limit_tracker: Arc<RateLimitTracker>,
    routing_config: Arc<tokio::sync::RwLock<SmartRoutingConfig>>,
    session_accounts: Arc<DashMap<String, String>>,
    adaptive_limits: Arc<tokio::sync::RwLock<Option<Arc<AdaptiveLimitManager>>>>,
    preferred_account_id: Arc<tokio::sync::RwLock<Option<String>>>,
    health_scores: Arc<DashMap<String, f32>>,
    runtime_protected_models: Arc<DashMap<String, HashSet<String>>>,
    active_requests: Arc<DashMap<String, AtomicU32>>,
    session_failures: Arc<DashMap<String, AtomicU32>>,
    file_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl TokenManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
            data_dir,
            rate_limit_tracker: Arc::new(RateLimitTracker::new()),
            routing_config: Arc::new(tokio::sync::RwLock::new(SmartRoutingConfig::default())),
            session_accounts: Arc::new(DashMap::new()),
            adaptive_limits: Arc::new(tokio::sync::RwLock::new(None)),
            preferred_account_id: Arc::new(tokio::sync::RwLock::new(None)),
            health_scores: Arc::new(DashMap::new()),
            runtime_protected_models: Arc::new(DashMap::new()),
            active_requests: Arc::new(DashMap::new()),
            session_failures: Arc::new(DashMap::new()),
            file_locks: Arc::new(DashMap::new()),
        }
    }

    pub async fn set_routing_config(&self, config: SmartRoutingConfig) {
        let mut guard = self.routing_config.write().await;
        *guard = config;
    }

    pub fn increment_active_requests(&self, email: &str) -> u32 {
        self.active_requests
            .entry(email.to_string())
            .or_insert_with(|| AtomicU32::new(0))
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }

    pub fn decrement_active_requests(&self, email: &str) {
        if let Some(counter) = self.active_requests.get(email) {
            let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                if v > 0 {
                    Some(v - 1)
                } else {
                    None
                }
            });
        }
    }

    pub fn get_active_requests(&self, email: &str) -> u32 {
        self.active_requests
            .get(email)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Check if model is protected for account (combines disk and runtime state)
    pub fn is_model_protected(&self, account_id: &str, model: &str) -> bool {
        if let Some(runtime) = self.runtime_protected_models.get(account_id) {
            if runtime.contains(model) {
                return true;
            }
        }
        if let Some(token) = self.tokens.get(account_id) {
            return token.protected_models.contains(model);
        }
        false
    }

    pub fn record_session_failure(&self, session_id: &str) -> u32 {
        self.session_failures
            .entry(session_id.to_string())
            .or_insert_with(|| AtomicU32::new(0))
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }

    pub fn clear_session_failures(&self, session_id: &str) {
        self.session_failures.remove(session_id);
    }

    pub fn get_session_failures(&self, session_id: &str) -> u32 {
        self.session_failures
            .get(session_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    fn get_file_lock(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.file_locks
            .entry(account_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Inject AIMD tracker for predictive rate limiting
    pub async fn set_adaptive_limits(&self, tracker: Arc<AdaptiveLimitManager>) {
        let mut guard = self.adaptive_limits.write().await;
        *guard = Some(tracker);
    }

    pub fn start_auto_cleanup(&self) {
        let tracker = self.rate_limit_tracker.clone();
        let session_failures = self.session_failures.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let cleaned = tracker.cleanup_expired();
                if cleaned > 0 {
                    tracing::info!(
                        "🧹 Auto-cleanup: Removed {} expired rate limit record(s)",
                        cleaned
                    );
                }
                // Cleanup stale session failures (retain only non-zero counts)
                let before = session_failures.len();
                session_failures.retain(|_, v| v.load(Ordering::Relaxed) > 0);
                let cleaned_sessions = before - session_failures.len();
                if cleaned_sessions > 0 {
                    tracing::debug!(
                        "🧹 Cleaned {} stale session failure record(s)",
                        cleaned_sessions
                    );
                }
            }
        });
        tracing::info!("✅ Rate limit auto-cleanup task started (interval: 60s)");
    }

    /// Start periodic account sync task (reloads accounts from disk every 60s)
    /// This ensures accounts added/modified externally are picked up automatically.
    pub fn start_auto_account_sync(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            // Skip first tick (accounts already loaded at startup)
            interval.tick().await;

            loop {
                interval.tick().await;
                match manager.reload_all_accounts().await {
                    Ok(count) => {
                        tracing::debug!("🔄 Auto-sync: Reloaded {} account(s) from disk", count);
                    }
                    Err(e) => {
                        tracing::warn!("⚠️ Auto-sync: Failed to reload accounts: {}", e);
                    }
                }
            }
        });
        tracing::info!("✅ Account auto-sync task started (interval: 60s)");
    }

    /// 从主应用账号目录加载所有账号
    pub async fn load_accounts(&self) -> Result<usize, String> {
        let accounts_dir = self.data_dir.join("accounts");

        if !accounts_dir.exists() {
            return Err(format!("账号目录不存在: {:?}", accounts_dir));
        }

        // Stage 1: Load all accounts into temporary storage first
        let mut new_tokens: Vec<(String, ProxyToken)> = Vec::new();

        let mut entries = tokio::fs::read_dir(&accounts_dir)
            .await
            .map_err(|e| format!("读取账号目录失败: {}", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("读取目录项失败: {}", e))?
        {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            match self.load_single_account(&path).await {
                Ok(Some(token)) => {
                    let account_id = token.account_id.clone();
                    new_tokens.push((account_id, token));
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::debug!("加载账号失败 {:?}: {}", path, e);
                }
            }
        }

        // Stage 2: Atomic swap - collect old keys, remove stale, insert new
        let old_keys: Vec<String> = self.tokens.iter().map(|e| e.key().clone()).collect();
        let new_keys: std::collections::HashSet<String> =
            new_tokens.iter().map(|(k, _)| k.clone()).collect();

        // Remove accounts no longer on disk
        for old_key in &old_keys {
            if !new_keys.contains(old_key) {
                self.tokens.remove(old_key);
            }
        }

        // Insert/update accounts from disk only if disk token is newer (atomic check-and-set)
        let count = new_tokens.len();
        for (account_id, disk_token) in new_tokens {
            self.tokens
                .entry(account_id)
                .and_modify(|existing| {
                    if disk_token.timestamp > existing.timestamp {
                        *existing = disk_token.clone();
                    }
                })
                .or_insert(disk_token);
        }

        Ok(count)
    }

    pub async fn reload_account(&self, account_id: &str) -> Result<(), String> {
        let path = self
            .data_dir
            .join("accounts")
            .join(format!("{}.json", account_id));
        if !path.exists() {
            return Err(format!("账号文件不存在: {:?}", path));
        }

        match self.load_single_account(&path).await {
            Ok(Some(token)) => {
                self.tokens.insert(account_id.to_string(), token);
                Ok(())
            }
            Ok(None) => Err("账号加载失败".to_string()),
            Err(e) => Err(format!("同步账号失败: {}", e)),
        }
    }

    pub async fn reload_all_accounts(&self) -> Result<usize, String> {
        self.load_accounts().await
    }

    /// 加载单个账号
    async fn load_single_account(&self, path: &PathBuf) -> Result<Option<ProxyToken>, String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("读取文件失败: {}", e))?;

        let account: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| format!("解析 JSON 失败: {}", e))?;

        if account
            .get("disabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            tracing::debug!(
                "Skipping disabled account file: {:?} (email={})",
                path,
                account
                    .get("email")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>")
            );
            return Ok(None);
        }

        // 检查主动禁用状态
        if account
            .get("proxy_disabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            tracing::debug!(
                "Skipping proxy-disabled account file: {:?} (email={})",
                path,
                account
                    .get("email")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>")
            );
            return Ok(None);
        }

        let account_id = account["id"].as_str().ok_or("缺少 id 字段")?.to_string();

        let email = account["email"]
            .as_str()
            .ok_or("缺少 email 字段")?
            .to_string();

        let token_obj = account["token"].as_object().ok_or("缺少 token 字段")?;

        let access_token = token_obj["access_token"]
            .as_str()
            .ok_or("缺少 access_token")?
            .to_string();

        let refresh_token = token_obj["refresh_token"]
            .as_str()
            .ok_or("缺少 refresh_token")?
            .to_string();

        let expires_in = token_obj["expires_in"].as_i64().ok_or("缺少 expires_in")?;

        let timestamp = token_obj["expiry_timestamp"]
            .as_i64()
            .ok_or("缺少 expiry_timestamp")?;

        // project_id 是可选的
        let project_id = token_obj
            .get("project_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 【新增】提取订阅等级 (subscription_tier 为 "FREE" | "PRO" | "ULTRA")
        let subscription_tier = account
            .get("quota")
            .and_then(|q| q.get("subscription_tier"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // [FIX #563] 提取最大剩余配额百分比用于优先级排序
        let remaining_quota = account
            .get("quota")
            .and_then(calculate_max_quota_percentage);

        // [FIX #621] 提取受保护模型列表 (quota exhausted models)
        // Also auto-populate from quota data - models with 0% should be protected
        let mut protected_models: HashSet<String> = account
            .get("protected_models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        // [FIX] Auto-add models with 0% quota to protected_models
        if let Some(quota) = account.get("quota") {
            if let Some(models) = quota.get("models").and_then(|m| m.as_array()) {
                for model in models {
                    if let (Some(name), Some(percentage)) = (
                        model.get("name").and_then(|n| n.as_str()),
                        model.get("percentage").and_then(|p| p.as_i64()),
                    ) {
                        if percentage == 0 && !protected_models.contains(name) {
                            protected_models.insert(name.to_string());
                            tracing::debug!(
                                "🛡️ Auto-protected model {} for account (quota=0%)",
                                name
                            );
                        }
                    }
                }
            }
        }

        if !protected_models.is_empty() {
            tracing::info!(
                "📋 Account has {} protected models: {:?}",
                protected_models.len(),
                protected_models
            );
        }

        let health_score = self
            .health_scores
            .get(&account_id)
            .map(|v| *v)
            .unwrap_or(1.0);

        if subscription_tier
            .as_ref()
            .is_some_and(|t| t.contains("ultra-business"))
        {
            tracing::info!(
                "🚀 Loaded Business-Ultra account: {} (tier={})",
                email,
                subscription_tier.as_deref().unwrap_or("?")
            );
        }

        Ok(Some(ProxyToken {
            account_id,
            access_token,
            refresh_token,
            expires_in,
            timestamp,
            email,
            account_path: path.clone(),
            project_id,
            subscription_tier,
            remaining_quota,
            protected_models,
            health_score,
        }))
    }

    /// 获取当前可用的 Token（支持粘性会话与智能调度）
    /// 参数 `quota_group` 用于区分 "claude" vs "gemini" 组
    /// 参数 `force_rotate` 为 true 时将忽略锁定，强制切换账号
    /// 参数 `session_id` 用于跨请求维持会话粘性
    /// 参数 `target_model` 目标模型名称（用于配额保护检查）
    /// 参数 `exclude_accounts` 已尝试过的账号列表（用于避免重复选择失败账号）
    pub async fn get_token(
        &self,
        quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str,
    ) -> Result<(String, String, String, ActiveRequestGuard), String> {
        self.get_token_with_exclusions(quota_group, force_rotate, session_id, target_model, None)
            .await
    }

    /// Extended version of get_token that accepts a set of accounts to exclude from selection
    pub async fn get_token_with_exclusions(
        &self,
        quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str,
        exclude_accounts: Option<&std::collections::HashSet<String>>,
    ) -> Result<(String, String, String, ActiveRequestGuard), String> {
        let timeout_duration = std::time::Duration::from_secs(5);
        match tokio::time::timeout(
            timeout_duration,
            self.get_token_internal(
                quota_group,
                force_rotate,
                session_id,
                target_model,
                exclude_accounts,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(
                "Token acquisition timeout (5s) - system too busy or deadlock detected".to_string(),
            ),
        }
    }

    /// 检查是否有可用账号（用于预检）
    /// Added for upstream API compatibility
    pub async fn has_available_account(&self, quota_group: &str, _target_model: &str) -> bool {
        let tokens_snapshot: Vec<ProxyToken> =
            self.tokens.iter().map(|e| e.value().clone()).collect();

        if tokens_snapshot.is_empty() {
            return false;
        }

        // Check if any account is available (not rate limited)
        for token in &tokens_snapshot {
            if !self.is_rate_limited(&token.email) {
                return true;
            }
        }

        // Log for debugging
        tracing::debug!("No available accounts for quota_group={}", quota_group);
        false
    }

    /// 通过 email 获取指定账号的 Token（用于预热等需要指定账号的场景）
    /// Added for upstream API compatibility
    pub async fn get_token_by_email(
        &self,
        email: &str,
    ) -> Result<(String, String, String), String> {
        // Find account by email
        let token = self
            .tokens
            .iter()
            .find(|entry| entry.value().email == email)
            .map(|entry| entry.value().clone());

        let mut token = match token {
            Some(t) => t,
            None => return Err(format!("Account not found: {}", email)),
        };

        // Check if token needs refresh
        let now = chrono::Utc::now().timestamp();
        if now >= token.timestamp - 300 {
            match crate::modules::oauth::refresh_access_token(&token.refresh_token).await {
                Ok(token_response) => {
                    token.access_token = token_response.access_token.clone();
                    token.expires_in = token_response.expires_in;
                    token.timestamp = now + token_response.expires_in;

                    // Update in-memory
                    if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                        entry.access_token = token.access_token.clone();
                        entry.expires_in = token.expires_in;
                        entry.timestamp = token.timestamp;
                    }

                    // Persist to disk
                    let _ = self
                        .save_refreshed_token(&token.account_id, &token_response)
                        .await;
                }
                Err(e) => {
                    return Err(format!("Token refresh failed for {}: {}", email, e));
                }
            }
        }

        let project_id = token.project_id.clone().unwrap_or_default();
        Ok((token.access_token, project_id, token.email))
    }

    /// 内部实现：获取 Token 的核心逻辑
    async fn get_token_internal(
        &self,
        _quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str,
        exclude_accounts: Option<&std::collections::HashSet<String>>,
    ) -> Result<(String, String, String, ActiveRequestGuard), String> {
        let mut tokens_snapshot: Vec<ProxyToken> =
            self.tokens.iter().map(|e| e.value().clone()).collect();
        let total = tokens_snapshot.len();

        if total == 0 {
            return Err("Token pool is empty".to_string());
        }

        // ===== 【优化】根据订阅等级和剩余配额排序 =====
        // [FIX #563] 优先级: ULTRA-BUSINESS > ULTRA > PRO > FREE, 同tier内优先高配额账号
        // 理由: ULTRA/PRO 重置快，优先消耗；FREE 重置慢，用于兜底
        //       高配额账号优先使用，避免低配额账号被用光
        tokens_snapshot.sort_by(|a, b| {
            // [FIX] Use ProxyToken::tier_priority() method for consistent tier ordering
            // Priority: 0=ultra-business, 1=ultra, 2=pro, 3=free, 4=unknown
            // First: compare by subscription tier
            let tier_cmp = a.tier_priority().cmp(&b.tier_priority());

            if tier_cmp != std::cmp::Ordering::Equal {
                return tier_cmp;
            }

            // [FIX #563] Second: compare by remaining quota percentage (higher is better)
            // Accounts with unknown/zero percentage go last within their tier
            let quota_a = a.remaining_quota.unwrap_or(0);
            let quota_b = b.remaining_quota.unwrap_or(0);
            let quota_cmp = quota_b.cmp(&quota_a);

            if quota_cmp != std::cmp::Ordering::Equal {
                return quota_cmp;
            }

            // [NEW v4.0.4] Third: compare by health score (higher is better)
            b.health_score
                .partial_cmp(&a.health_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 0. Load smart routing configuration
        let routing = self.routing_config.read().await.clone();

        // [FIX #621] Load quota protection config
        let quota_protection_enabled = config::load_config()
            .map(|cfg| cfg.quota_protection.enabled)
            .unwrap_or(false);

        // Normalize target model name to standard ID for quota protection check
        let normalized_target =
            crate::proxy::common::model_mapping::normalize_to_standard_id(target_model)
                .unwrap_or_else(|| target_model.to_string());

        // [ARCHITECTURE FIX] Pre-filter tokens_snapshot to exclude accounts with 0% quota for target model
        // This is the SINGLE place where quota protection is enforced - all selection paths will only see eligible accounts
        if quota_protection_enabled {
            let original_count = tokens_snapshot.len();
            tokens_snapshot.retain(|t| !self.is_model_protected(&t.account_id, &normalized_target));
            let filtered_count = original_count - tokens_snapshot.len();
            if filtered_count > 0 {
                tracing::debug!(
                    "🛡️ Quota protection: filtered out {} accounts with 0% quota for {}",
                    filtered_count,
                    normalized_target
                );
            }
        }

        // ===== [FIX #820] Fixed Account Mode: prefer specified account =====
        let preferred_id = self.preferred_account_id.read().await.clone();
        if let Some(ref pref_id) = preferred_id {
            if let Some(preferred_token) = tokens_snapshot.iter().find(|t| &t.account_id == pref_id)
            {
                let is_rate_limited =
                    self.is_rate_limited_for_model(&preferred_token.email, &normalized_target);
                let is_quota_protected = quota_protection_enabled
                    && self.is_model_protected(&preferred_token.account_id, &normalized_target);

                if !is_rate_limited && !is_quota_protected {
                    tracing::info!(
                        "🔒 [FIX #820] Using preferred account: {} (fixed mode)",
                        preferred_token.email
                    );

                    let mut token = preferred_token.clone();

                    let now = chrono::Utc::now().timestamp();
                    if now >= token.timestamp - 300 {
                        tracing::debug!(
                            "Preferred account {} token expiring, refreshing...",
                            token.email
                        );
                        match crate::modules::oauth::refresh_access_token(&token.refresh_token)
                            .await
                        {
                            Ok(token_response) => {
                                token.access_token = token_response.access_token.clone();
                                token.expires_in = token_response.expires_in;
                                token.timestamp = now + token_response.expires_in;

                                if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                                    entry.access_token = token.access_token.clone();
                                    entry.expires_in = token.expires_in;
                                    entry.timestamp = token.timestamp;
                                }
                                let _ = self
                                    .save_refreshed_token(&token.account_id, &token_response)
                                    .await;
                            }
                            Err(e) => {
                                tracing::warn!("Preferred account token refresh failed: {}", e);
                            }
                        }
                    }

                    let project_id = if let Some(pid) = &token.project_id {
                        pid.clone()
                    } else {
                        match crate::proxy::project_resolver::fetch_project_id(&token.access_token)
                            .await
                        {
                            Ok(pid) => {
                                if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                                    entry.project_id = Some(pid.clone());
                                }
                                let _ = self.save_project_id(&token.account_id, &pid).await;
                                pid
                            }
                            Err(_) => "bamboo-precept-lgxtn".to_string(),
                        }
                    };

                    if let Some(guard) = ActiveRequestGuard::try_new(
                        Arc::clone(&self.active_requests),
                        token.email.clone(),
                        routing.max_concurrent_per_account,
                    ) {
                        return Ok((token.access_token, project_id, token.email, guard));
                    }
                    tracing::debug!(
                        "Preferred account {} at max concurrency, falling back",
                        token.email
                    );
                } else if is_rate_limited {
                    tracing::warn!(
                        "🔒 [FIX #820] Preferred account {} is rate-limited, falling back to round-robin",
                        preferred_token.email
                    );
                } else {
                    tracing::warn!(
                        "🔒 [FIX #621] Preferred account {} is quota-protected for model {}, falling back to round-robin",
                        preferred_token.email, normalized_target
                    );
                }
            } else {
                tracing::warn!(
                    "🔒 [FIX #820] Preferred account {} not found in pool, falling back to round-robin",
                    pref_id
                );
            }
        }
        // ===== [END FIX #820] =====

        let mut attempted: HashSet<String> = exclude_accounts.cloned().unwrap_or_default();
        let mut last_error: Option<String> = None;

        // 获取 AIMD tracker 引用 (避免在循环中多次获取锁)
        let aimd = self.adaptive_limits.read().await.clone();

        for attempt in 0..total {
            let rotate = force_rotate || attempt > 0;

            let mut target_token: Option<ProxyToken> = None;
            let mut active_guard: Option<ActiveRequestGuard> = None;

            // Check if session has too many consecutive failures - force unbind
            if let Some(sid) = session_id {
                let failures = self.get_session_failures(sid);
                if failures >= SESSION_FAILURE_THRESHOLD {
                    if let Some(bound_id) = self.session_accounts.get(sid).map(|v| v.clone()) {
                        self.session_accounts.remove(sid);
                        self.clear_session_failures(sid);
                        tracing::warn!(
                            "Session {} unbound from {} after {} consecutive failures",
                            sid,
                            bound_id,
                            failures
                        );
                    }
                }
            }

            // === ULTRA-TIER PRIORITY: Check ultra accounts BEFORE sticky session ===
            // If an ultra/ultra-business account is available, use it even if session is sticky to pro
            if target_token.is_none() && !rotate {
                let mut ultra_candidates: Vec<(&ProxyToken, u8, u32)> = Vec::new();

                for candidate in &tokens_snapshot {
                    if !candidate.is_ultra_tier() {
                        continue;
                    }

                    if attempted.contains(&candidate.email) {
                        continue;
                    }

                    if self.is_rate_limited_for_model(&candidate.email, &normalized_target) {
                        continue;
                    }

                    if quota_protection_enabled
                        && self.is_model_protected(&candidate.account_id, &normalized_target)
                    {
                        continue;
                    }

                    if let Some(aimd) = &aimd {
                        if aimd.usage_ratio(&candidate.email) > 1.2 {
                            continue;
                        }
                    }

                    let active = self.get_active_requests(&candidate.email);
                    let tier = candidate.tier_priority();
                    ultra_candidates.push((candidate, tier, active));
                }

                if !ultra_candidates.is_empty() {
                    // Sort by: 1) tier priority (lower=better), 2) active requests (lower=better)
                    ultra_candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

                    for (candidate, _tier, _active) in ultra_candidates {
                        if let Some(guard) = ActiveRequestGuard::try_new(
                            Arc::clone(&self.active_requests),
                            candidate.email.clone(),
                            routing.max_concurrent_per_account,
                        ) {
                            tracing::debug!(
                                "Ultra Priority: Selected {} ({:?}) over sticky session",
                                candidate.email,
                                candidate.account_tier()
                            );
                            target_token = Some(candidate.clone());
                            active_guard = Some(guard);
                            break;
                        }
                    }
                }
            }
            // === END ULTRA-TIER PRIORITY ===

            // Sticky session handling (FALLBACK: only if no ultra account was selected)
            if target_token.is_none() {
                if let Some(sid) = session_id {
                    if !rotate && routing.enable_session_affinity {
                        if let Some(bound_id) = self.session_accounts.get(sid).map(|v| v.clone()) {
                            let reset_sec = self
                                .rate_limit_tracker
                                .get_remaining_wait_for_model(&bound_id, &normalized_target);

                            if reset_sec > 0 {
                                if reset_sec > STICKY_UNBIND_RATE_LIMIT_SECONDS {
                                    self.session_accounts.remove(sid);
                                    tracing::warn!(
                                        "Sticky Session: {} rate-limited ({}s), unbinding session {}",
                                        bound_id,
                                        reset_sec,
                                        sid
                                    );
                                } else {
                                    tracing::debug!(
                                        "Sticky Session: {} rate-limited ({}s), migrating this request only",
                                        bound_id, reset_sec
                                    );
                                }
                            } else if !attempted.contains(&bound_id) {
                                let is_quota_protected = quota_protection_enabled
                                    && tokens_snapshot
                                        .iter()
                                        .find(|t| t.email == bound_id)
                                        .is_some_and(|t| {
                                            self.is_model_protected(
                                                &t.account_id,
                                                &normalized_target,
                                            )
                                        });

                                if is_quota_protected {
                                    tracing::debug!(
                                        "Sticky Session: {} is quota-protected for {}, unbinding",
                                        bound_id,
                                        normalized_target
                                    );
                                    self.session_accounts.remove(sid);
                                } else if let Some(found) =
                                    tokens_snapshot.iter().find(|t| t.email == bound_id)
                                {
                                    if let Some(guard) = ActiveRequestGuard::try_new(
                                        Arc::clone(&self.active_requests),
                                        found.email.clone(),
                                        routing.max_concurrent_per_account,
                                    ) {
                                        tracing::debug!(
                                            "Sticky Session: Reusing {} for session {}",
                                            found.email,
                                            sid
                                        );
                                        target_token = Some(found.clone());
                                        active_guard = Some(guard);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Mode B (60s global lock) REMOVED: conflicts with Smart Routing distribution
            // Session affinity (Mode A) handles per-session consistency instead

            if target_token.is_none() {
                // Collect eligible candidates with tier and active count
                let mut scored_candidates: Vec<(&ProxyToken, u8, u32)> = Vec::new();

                for candidate in &tokens_snapshot {
                    if attempted.contains(&candidate.email) {
                        continue;
                    }

                    if self.is_rate_limited_for_model(&candidate.email, &normalized_target) {
                        continue;
                    }

                    if quota_protection_enabled
                        && self.is_model_protected(&candidate.account_id, &normalized_target)
                    {
                        continue;
                    }

                    if let Some(aimd) = &aimd {
                        if aimd.usage_ratio(&candidate.email) > 1.2 {
                            continue;
                        }
                    }

                    let active = self.get_active_requests(&candidate.email);
                    let tier = candidate.tier_priority();
                    scored_candidates.push((candidate, tier, active));
                }

                // Sort by: 1) tier priority (lower=better), 2) active requests (lower=better)
                scored_candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

                // Try to reserve slot atomically for each candidate in order
                for (candidate, _tier, _active) in scored_candidates {
                    if let Some(guard) = ActiveRequestGuard::try_new(
                        Arc::clone(&self.active_requests),
                        candidate.email.clone(),
                        routing.max_concurrent_per_account,
                    ) {
                        target_token = Some(candidate.clone());
                        active_guard = Some(guard);
                        break;
                    }
                }
            }

            let mut token = match target_token {
                Some(t) => t,
                None => {
                    // 乐观重置策略: 双层防护机制
                    // 当所有账号都无法选择时,可能是时序竞争导致的状态不同步

                    // 计算最短等待时间
                    let min_wait = tokens_snapshot
                        .iter()
                        .filter_map(|t| self.rate_limit_tracker.get_reset_seconds(&t.email))
                        .min();

                    // Layer 1: 如果最短等待时间 <= 2秒,执行缓冲延迟
                    if let Some(wait_sec) = min_wait {
                        if wait_sec <= 2 {
                            tracing::warn!(
                                "All accounts rate-limited but shortest wait is {}s. Applying 500ms buffer for state sync...",
                                wait_sec
                            );

                            // 缓冲延迟 500ms
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                            // Retry selection with atomic slot reservation
                            let mut found_token: Option<ProxyToken> = None;
                            for t in tokens_snapshot.iter() {
                                if attempted.contains(&t.email) {
                                    continue;
                                }
                                if self.is_rate_limited_for_model(&t.email, &normalized_target) {
                                    continue;
                                }
                                if let Some(guard) = ActiveRequestGuard::try_new(
                                    Arc::clone(&self.active_requests),
                                    t.email.clone(),
                                    routing.max_concurrent_per_account,
                                ) {
                                    tracing::info!(
                                        "✅ Buffer delay successful! Found available account: {}",
                                        t.email
                                    );
                                    active_guard = Some(guard);
                                    found_token = Some(t.clone());
                                    break;
                                }
                            }

                            if let Some(t) = found_token {
                                t
                            } else {
                                // Layer 2: 缓冲后仍无可用账号,执行乐观重置
                                tracing::warn!(
                                    "Buffer delay failed. Executing optimistic reset for all {} accounts...",
                                    tokens_snapshot.len()
                                );

                                // 清除所有限流记录
                                self.rate_limit_tracker.clear_all();

                                // Retry with atomic slot reservation after reset
                                let mut reset_found: Option<ProxyToken> = None;
                                for t in tokens_snapshot.iter() {
                                    if attempted.contains(&t.email) {
                                        continue;
                                    }
                                    if let Some(guard) = ActiveRequestGuard::try_new(
                                        Arc::clone(&self.active_requests),
                                        t.email.clone(),
                                        routing.max_concurrent_per_account,
                                    ) {
                                        tracing::info!(
                                            "✅ Optimistic reset successful! Using account: {}",
                                            t.email
                                        );
                                        active_guard = Some(guard);
                                        reset_found = Some(t.clone());
                                        break;
                                    }
                                }

                                if let Some(t) = reset_found {
                                    t
                                } else {
                                    // 所有策略都失败,返回错误
                                    return Err(
                                        "All accounts failed after optimistic reset. Please check account health.".to_string()
                                    );
                                }
                            }
                        } else {
                            // 等待时间 > 2秒,正常返回错误
                            return Err(format!(
                                "All accounts are currently limited. Please wait {}s.",
                                wait_sec
                            ));
                        }
                    } else {
                        // [FIX] No rate-limit records but all accounts busy (max_concurrent)
                        // Wait and retry instead of immediate failure
                        tracing::warn!(
                            "All {} accounts at max concurrency. Waiting 500ms for availability...",
                            tokens_snapshot.len()
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                        // Find any available account after wait with atomic slot reservation
                        let mut wait_found: Option<ProxyToken> = None;
                        for t in tokens_snapshot.iter() {
                            if attempted.contains(&t.email) {
                                continue;
                            }
                            if self.is_rate_limited_for_model(&t.email, &normalized_target) {
                                continue;
                            }
                            if quota_protection_enabled
                                && self.is_model_protected(&t.account_id, &normalized_target)
                            {
                                continue;
                            }
                            if let Some(guard) = ActiveRequestGuard::try_new(
                                Arc::clone(&self.active_requests),
                                t.email.clone(),
                                routing.max_concurrent_per_account,
                            ) {
                                tracing::info!(
                                    "✅ Found available account after wait: {}",
                                    t.email
                                );
                                active_guard = Some(guard);
                                wait_found = Some(t.clone());
                                break;
                            }
                        }

                        if let Some(t) = wait_found {
                            t
                        } else {
                            return Err(
                                "All accounts at maximum capacity. Please retry later.".to_string()
                            );
                        }
                    }
                }
            };

            // Ensure session is always bound to the selected account (by email)
            // This covers all selection paths: rotation, fallback, optimistic reset
            if let Some(sid) = session_id {
                if routing.enable_session_affinity {
                    let current_binding = self.session_accounts.get(sid).map(|v| v.clone());
                    if current_binding.as_ref() != Some(&token.email) {
                        self.session_accounts
                            .insert(sid.to_string(), token.email.clone());
                        if current_binding.is_some() {
                            tracing::info!(
                                "Sticky Session: Rebound session {} from {} to {} (cache continuity)",
                                sid,
                                current_binding.unwrap_or_default(),
                                token.email
                            );
                        }
                    }
                }
            }

            // 3. 检查 token 是否过期（提前5分钟刷新）
            let now = chrono::Utc::now().timestamp();
            if now >= token.timestamp - 300 {
                tracing::debug!("账号 {} 的 token 即将过期，正在刷新...", token.email);

                // 调用 OAuth 刷新 token
                match oauth::refresh_access_token(&token.refresh_token).await {
                    Ok(token_response) => {
                        tracing::debug!("Token 刷新成功！");

                        // 更新本地内存对象供后续使用
                        token.access_token = token_response.access_token.clone();
                        token.expires_in = token_response.expires_in;
                        token.timestamp = now + token_response.expires_in;

                        // 同步更新跨线程共享的 DashMap
                        if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                            entry.access_token = token.access_token.clone();
                            entry.expires_in = token.expires_in;
                            entry.timestamp = token.timestamp;
                        }

                        // 同步落盘（避免重启后继续使用过期 timestamp 导致频繁刷新）
                        if let Err(e) = self
                            .save_refreshed_token(&token.account_id, &token_response)
                            .await
                        {
                            tracing::debug!("保存刷新后的 token 失败 ({}): {}", token.email, e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Token 刷新失败 ({}): {}，尝试下一个账号", token.email, e);
                        if e.contains("\"invalid_grant\"") || e.contains("invalid_grant") {
                            tracing::error!(
                                "Disabling account due to invalid_grant ({}): refresh_token likely revoked/expired",
                                token.email
                            );
                            let _ = self
                                .disable_account(
                                    &token.account_id,
                                    &format!("invalid_grant: {}", e),
                                )
                                .await;
                            self.tokens.remove(&token.account_id);
                        }
                        // Avoid leaking account emails to API clients; details are still in logs.
                        last_error = Some(format!("Token refresh failed: {}", e));
                        attempted.insert(token.email.clone());
                        continue;
                    }
                }
            }

            // 4. 确保有 project_id
            let project_id = if let Some(pid) = &token.project_id {
                pid.clone()
            } else {
                tracing::debug!("账号 {} 缺少 project_id，尝试获取...", token.email);
                match crate::proxy::project_resolver::fetch_project_id(&token.access_token).await {
                    Ok(pid) => {
                        if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                            entry.project_id = Some(pid.clone());
                        }
                        let _ = self.save_project_id(&token.account_id, &pid).await;
                        pid
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch project_id for {}: {}", token.email, e);
                        last_error = Some(format!(
                            "Failed to fetch project_id for {}: {}",
                            token.email, e
                        ));
                        attempted.insert(token.email.clone());
                        continue;
                    }
                }
            };

            let guard = match active_guard {
                Some(g) => g,
                None => {
                    // All selection paths should create a guard. If we reach here,
                    // it means token was selected but guard wasn't created - try once more.
                    match ActiveRequestGuard::try_new(
                        Arc::clone(&self.active_requests),
                        token.email.clone(),
                        routing.max_concurrent_per_account,
                    ) {
                        Some(g) => g,
                        None => {
                            tracing::warn!(
                                "Account {} at capacity after selection. Retrying with next account.",
                                token.email
                            );
                            attempted.insert(token.email.clone());
                            continue;
                        }
                    }
                }
            };

            return Ok((token.access_token, project_id, token.email, guard));
        }

        Err(last_error.unwrap_or_else(|| "All accounts failed".to_string()))
    }

    async fn disable_account(&self, account_id: &str, reason: &str) -> Result<(), String> {
        let path = if let Some(entry) = self.tokens.get(account_id) {
            entry.account_path.clone()
        } else {
            self.data_dir
                .join("accounts")
                .join(format!("{}.json", account_id))
        };

        let lock = self.get_file_lock(account_id);
        let _guard = lock.lock().await;

        let content_str = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("读取文件失败: {}", e))?;
        let mut content: serde_json::Value =
            serde_json::from_str(&content_str).map_err(|e| format!("解析 JSON 失败: {}", e))?;

        let now = chrono::Utc::now().timestamp();
        content["disabled"] = serde_json::Value::Bool(true);
        content["disabled_at"] = serde_json::Value::Number(now.into());
        content["disabled_reason"] = serde_json::Value::String(truncate_reason(reason, 800));

        atomic_write_json(&path, &content).await?;

        tracing::warn!("Account disabled: {} ({:?})", account_id, path);
        Ok(())
    }

    /// 保存 project_id 到账号文件
    async fn save_project_id(&self, account_id: &str, project_id: &str) -> Result<(), String> {
        let entry = self.tokens.get(account_id).ok_or("账号不存在")?;
        let path = entry.account_path.clone();
        drop(entry);

        let lock = self.get_file_lock(account_id);
        let _guard = lock.lock().await;

        let content_str = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("读取文件失败: {}", e))?;
        let mut content: serde_json::Value =
            serde_json::from_str(&content_str).map_err(|e| format!("解析 JSON 失败: {}", e))?;

        content["token"]["project_id"] = serde_json::Value::String(project_id.to_string());

        atomic_write_json(&path, &content).await?;

        tracing::debug!("已保存 project_id 到账号 {}", account_id);
        Ok(())
    }

    /// 保存刷新后的 token 到账号文件
    async fn save_refreshed_token(
        &self,
        account_id: &str,
        token_response: &oauth::TokenResponse,
    ) -> Result<(), String> {
        let entry = self.tokens.get(account_id).ok_or("账号不存在")?;
        let path = entry.account_path.clone();
        drop(entry);

        let lock = self.get_file_lock(account_id);
        let _guard = lock.lock().await;

        let content_str = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("读取文件失败: {}", e))?;
        let mut content: serde_json::Value =
            serde_json::from_str(&content_str).map_err(|e| format!("解析 JSON 失败: {}", e))?;

        let now = chrono::Utc::now().timestamp();

        content["token"]["access_token"] =
            serde_json::Value::String(token_response.access_token.clone());
        content["token"]["expires_in"] =
            serde_json::Value::Number(token_response.expires_in.into());
        content["token"]["expiry_timestamp"] =
            serde_json::Value::Number((now + token_response.expires_in).into());

        atomic_write_json(&path, &content).await?;

        tracing::debug!("已保存刷新后的 token 到账号 {}", account_id);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    // ===== 限流管理方法 =====

    /// 标记账号限流(从外部调用,通常在 handler 中)
    /// Backwards-compatible 4-argument version (model defaults to None)
    pub fn mark_rate_limited(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        error_body: &str,
    ) {
        self.mark_rate_limited_with_model(account_id, status, retry_after_header, error_body, None);
    }

    /// 标记账号限流 with model parameter
    pub fn mark_rate_limited_with_model(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        error_body: &str,
        model: Option<String>,
    ) {
        self.rate_limit_tracker.parse_from_error(
            account_id,
            status,
            retry_after_header,
            error_body,
            model,
        );
    }

    /// 检查账号是否在限流中
    pub fn is_rate_limited(&self, account_id: &str) -> bool {
        self.rate_limit_tracker.is_rate_limited(account_id)
    }

    /// Check if account is rate-limited for specific model (checks both levels)
    pub fn is_rate_limited_for_model(&self, account_id: &str, model: &str) -> bool {
        self.rate_limit_tracker
            .is_rate_limited_for_model(account_id, model)
    }

    pub fn rate_limit_tracker(&self) -> &RateLimitTracker {
        &self.rate_limit_tracker
    }

    /// 获取距离限流重置还有多少秒
    #[allow(dead_code)]
    pub fn get_rate_limit_reset_seconds(&self, account_id: &str) -> Option<u64> {
        self.rate_limit_tracker.get_reset_seconds(account_id)
    }

    /// 清除过期的限流记录
    #[allow(dead_code)]
    pub fn cleanup_expired_rate_limits(&self) -> usize {
        self.rate_limit_tracker.cleanup_expired()
    }

    /// 清除指定账号的限流记录
    pub fn clear_rate_limit(&self, account_id: &str) -> bool {
        self.rate_limit_tracker.clear(account_id)
    }

    pub fn clear_all_rate_limits(&self) {
        self.rate_limit_tracker.clear_all();
    }

    /// 标记账号请求成功，重置连续失败计数
    ///
    /// 在请求成功完成后调用，将该账号的失败计数归零，
    /// 下次失败时从最短的锁定时间开始（智能限流）。
    pub fn mark_account_success(&self, account_id: &str) {
        self.rate_limit_tracker.mark_success(account_id);
    }

    /// 从账号文件获取配额刷新时间
    ///
    /// 返回该账号最近的配额刷新时间字符串（ISO 8601 格式）
    /// Optimized: uses in-memory token lookup instead of O(N) disk scan
    pub async fn get_quota_reset_time(&self, email: &str) -> Option<String> {
        let account_path = self
            .tokens
            .iter()
            .find(|entry| entry.value().email == email)
            .map(|entry| entry.value().account_path.clone())?;

        let content = tokio::fs::read_to_string(&account_path).await.ok()?;
        let account: serde_json::Value = serde_json::from_str(&content).ok()?;

        let models = account
            .get("quota")
            .and_then(|q| q.get("models"))
            .and_then(|m| m.as_array())?;

        models
            .iter()
            .filter_map(|model| model.get("reset_time").and_then(|r| r.as_str()))
            .filter(|s| !s.is_empty())
            .min()
            .map(|s| s.to_string())
    }

    /// 使用配额刷新时间精确锁定账号
    ///
    /// 当 API 返回 429 但没有 quotaResetDelay 时,尝试使用账号的配额刷新时间
    ///
    /// # 参数
    /// - `model`: 可选的模型名称,用于模型级别限流
    pub async fn set_precise_lockout(
        &self,
        email: &str,
        reason: crate::proxy::rate_limit::RateLimitReason,
        model: Option<String>,
    ) -> bool {
        if let Some(reset_time_str) = self.get_quota_reset_time(email).await {
            tracing::info!("找到账号 {} 的配额刷新时间: {}", email, reset_time_str);
            self.rate_limit_tracker
                .set_lockout_until_iso(email, &reset_time_str, reason, model)
        } else {
            tracing::debug!("未找到账号 {} 的配额刷新时间,将使用默认退避策略", email);
            false
        }
    }

    /// 实时刷新配额并精确锁定账号
    ///
    /// 当 429 发生时调用此方法:
    /// 1. 实时调用配额刷新 API 获取最新的 reset_time
    /// 2. 使用最新的 reset_time 精确锁定账号
    /// 3. 如果获取失败,返回 false 让调用方使用回退策略
    ///
    /// # 参数
    /// - `model`: 可选的模型名称,用于模型级别限流
    pub async fn fetch_and_lock_with_realtime_quota(
        &self,
        email: &str,
        reason: crate::proxy::rate_limit::RateLimitReason,
        model: Option<String>,
    ) -> bool {
        // 1. 从 tokens 中获取该账号的 access_token
        let access_token = {
            let mut found_token: Option<String> = None;
            for entry in self.tokens.iter() {
                if entry.value().email == email {
                    found_token = Some(entry.value().access_token.clone());
                    break;
                }
            }
            found_token
        };

        let access_token = match access_token {
            Some(t) => t,
            None => {
                tracing::warn!("无法找到账号 {} 的 access_token,无法实时刷新配额", email);
                return false;
            }
        };

        // 2. 调用配额刷新 API
        tracing::info!("账号 {} 正在实时刷新配额...", email);
        match quota::fetch_quota(&access_token, email).await {
            Ok((quota_data, _project_id)) => {
                // 3. 从最新配额中提取 reset_time
                let earliest_reset = quota_data
                    .models
                    .iter()
                    .filter_map(|m| {
                        if !m.reset_time.is_empty() {
                            Some(m.reset_time.as_str())
                        } else {
                            None
                        }
                    })
                    .min();

                if let Some(reset_time_str) = earliest_reset {
                    tracing::info!(
                        "账号 {} 实时配额刷新成功,reset_time: {}",
                        email,
                        reset_time_str
                    );
                    self.rate_limit_tracker.set_lockout_until_iso(
                        email,
                        reset_time_str,
                        reason,
                        model,
                    )
                } else {
                    tracing::warn!("账号 {} 配额刷新成功但未找到 reset_time", email);
                    false
                }
            }
            Err(e) => {
                tracing::warn!("账号 {} 实时配额刷新失败: {:?}", email, e);
                false
            }
        }
    }

    /// 标记账号限流(异步版本,支持实时配额刷新)
    ///
    /// 三级降级策略:
    /// 1. 优先: API 返回 quotaResetDelay → 直接使用
    /// 2. 次优: 实时刷新配额 → 获取最新 reset_time
    /// 3. 保底: 使用本地缓存配额 → 读取账号文件
    /// 4. 兜底: 指数退避策略 → 默认锁定时间
    ///
    /// # 参数
    /// - `model`: 可选的模型名称,用于模型级别限流。传入实际使用的模型可以避免不同模型配额互相影响
    pub async fn mark_rate_limited_async(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        error_body: &str,
        model: Option<&str>,
    ) {
        let reason = self.rate_limit_tracker.parse_rate_limit_reason(error_body);
        let raw_model = model.unwrap_or("unknown");

        // Normalize model to match get_token() check (prevents key mismatch)
        let model_str = crate::proxy::common::model_mapping::normalize_to_standard_id(raw_model)
            .unwrap_or_else(|| raw_model.to_string());

        // [FIX] ModelCapacityExhausted = temporary GPU overload, NOT quota exhaustion
        // Don't lock the account - handler will retry with exponential backoff
        if reason == crate::proxy::rate_limit::RateLimitReason::ModelCapacityExhausted {
            tracing::debug!(
                "⚡ {}:{} ModelCapacityExhausted - NOT locking, handler will retry",
                account_id,
                model_str
            );
            return; // Exit early - no lockout
        }

        // Immediately set temporary lockout BEFORE any async operations (race condition fix)
        let immediate_lockout = std::time::Duration::from_secs(15);
        self.rate_limit_tracker.set_model_lockout(
            account_id,
            &model_str,
            std::time::SystemTime::now() + immediate_lockout,
            reason,
        );
        tracing::debug!(
            "🔒 {}:{} immediate 15s lockout (pending precise time)",
            account_id,
            model_str
        );

        // Check if API returned explicit retry time
        let has_explicit_retry_time =
            retry_after_header.is_some() || error_body.contains("quotaResetDelay");

        if has_explicit_retry_time {
            if let Some(info) = self.rate_limit_tracker.parse_from_error(
                account_id,
                status,
                retry_after_header,
                error_body,
                Some(model_str.clone()),
            ) {
                self.rate_limit_tracker.set_model_lockout(
                    account_id,
                    &model_str,
                    info.reset_time,
                    reason,
                );
            }
            return;
        }

        match reason {
            crate::proxy::rate_limit::RateLimitReason::QuotaExhausted => {
                // Store in runtime_protected_models (persists across account reloads)
                self.runtime_protected_models
                    .entry(account_id.to_string())
                    .or_default()
                    .insert(model_str.clone());
                tracing::warn!(
                    "🛡️ {}:{} added to runtime_protected_models (quota exhausted)",
                    account_id,
                    model_str
                );

                let lockout = std::time::Duration::from_secs(600);
                self.rate_limit_tracker.set_model_lockout(
                    account_id,
                    &model_str,
                    std::time::SystemTime::now() + lockout,
                    reason,
                );
                tracing::info!(
                    "⏳ {}:{} QUOTA_EXHAUSTED, 10min fallback lock (fetching precise time)",
                    account_id,
                    model_str
                );
            }
            _ => {
                let lockout_secs = self
                    .rate_limit_tracker
                    .set_adaptive_model_lockout(account_id, &model_str);
                tracing::debug!(
                    "⚡ {}:{} adaptive lockout: {}s",
                    account_id,
                    model_str,
                    lockout_secs
                );
            }
        }

        if self
            .fetch_and_lock_with_realtime_quota(account_id, reason, Some(model_str.clone()))
            .await
        {
            tracing::info!(
                "{}:{} locked with precise reset time",
                account_id,
                model_str
            );
            return;
        }

        // Fallback: try local cache
        if self
            .set_precise_lockout(account_id, reason, model.map(|s| s.to_string()))
            .await
        {
            tracing::info!("{}:{} locked with cached reset time", account_id, model_str);
            return;
        }

        // All failed — keep the temporary lock set above
        tracing::warn!(
            "{}:{} no precise reset time available, using temporary lock",
            account_id,
            model_str
        );
    }

    // ===== Smart Routing Configuration Methods =====

    pub async fn get_routing_config(&self) -> SmartRoutingConfig {
        self.routing_config.read().await.clone()
    }

    pub async fn update_routing_config(&self, new_config: SmartRoutingConfig) {
        let mut config = self.routing_config.write().await;
        *config = new_config;
        tracing::debug!("Smart routing configuration updated: {:?}", *config);
    }

    /// 清除特定会话的粘性映射
    #[allow(dead_code)]
    pub fn clear_session_binding(&self, session_id: &str) {
        self.session_accounts.remove(session_id);
    }

    /// 清除所有会话的粘性映射
    pub fn clear_all_sessions(&self) {
        self.session_accounts.clear();
    }

    // ===== [FIX #820] Fixed Account Mode =====

    pub async fn set_preferred_account(&self, account_id: Option<String>) {
        let mut preferred = self.preferred_account_id.write().await;
        if let Some(ref id) = account_id {
            tracing::info!("🔒 [FIX #820] Fixed account mode enabled: {}", id);
        } else {
            tracing::info!("🔄 [FIX #820] Round-robin mode enabled (no preferred account)");
        }
        *preferred = account_id;
    }

    pub async fn get_preferred_account(&self) -> Option<String> {
        self.preferred_account_id.read().await.clone()
    }

    // ===== [NEW v4.0.4] Health Score Tracking =====

    /// Record request success, increase health score
    pub fn record_success(&self, account_id: &str) {
        self.health_scores
            .entry(account_id.to_string())
            .and_modify(|s| *s = (*s + 0.05).min(1.0))
            .or_insert(1.0);
        tracing::debug!("📈 Health score increased for account {}", account_id);
    }

    /// Record request failure, decrease health score
    pub fn record_failure(&self, account_id: &str) {
        self.health_scores
            .entry(account_id.to_string())
            .and_modify(|s| *s = (*s - 0.2).max(0.0))
            .or_insert(0.8);
        tracing::warn!("📉 Health score decreased for account {}", account_id);
    }
}

#[cfg(test)]
mod tests;
