use crate::modules::{config, oauth, quota};
// 移除冗余的顶层导入，因为这些在代码中已由 full path 或局部导入处理
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::proxy::rate_limit::RateLimitTracker;
use crate::proxy::AdaptiveLimitManager;

// ============================================================================
// Smart Routing Configuration (replaces SchedulingMode enum)
// ============================================================================

/// Unified smart routing configuration.
/// Replaces the old 3-mode system (CacheFirst/Balance/PerformanceFirst) with
/// a single algorithm that maximizes cache hits while preventing thundering herd.
#[derive(Debug, Clone)]
pub struct SmartRoutingConfig {
    /// Maximum concurrent requests per account (prevents thundering herd)
    /// Default: 3
    pub max_concurrent_per_account: u32,
    /// AIMD usage ratio threshold for pre-emptive queueing
    /// When ratio > threshold, wait instead of switching accounts
    /// Default: 0.8
    pub preemptive_throttle_ratio: f32,
    /// Minimum delay (ms) before retrying same account after soft throttle
    /// Default: 100
    pub throttle_delay_ms: u64,
    /// Enable session affinity (sticky sessions for cache optimization)
    /// Default: true
    pub enable_session_affinity: bool,
}

impl Default for SmartRoutingConfig {
    fn default() -> Self {
        Self {
            max_concurrent_per_account: 3,
            preemptive_throttle_ratio: 0.8,
            throttle_delay_ms: 100,
            enable_session_affinity: true,
        }
    }
}

/// Token representing an authenticated account with OAuth credentials.
///
/// Contains access/refresh tokens, account metadata, and quota information
/// for routing requests to the appropriate Google/Anthropic backend.
#[derive(Debug, Clone)]
pub struct ProxyToken {
    pub account_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub timestamp: i64,
    pub email: String,
    pub account_path: PathBuf, // 账号文件路径，用于更新
    pub project_id: Option<String>,
    pub subscription_tier: Option<String>, // "FREE" | "PRO" | "ULTRA"
    pub remaining_quota: Option<i32>, // [FIX #563] Remaining quota percentage for priority sorting
    pub protected_models: HashSet<String>, // [FIX #621] Models with exhausted quota (0%)
    pub health_score: f32,            // [NEW v4.0.4] Health score (0.0 - 1.0)
}

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
    current_index: Arc<AtomicUsize>,
    last_used_account: Arc<tokio::sync::Mutex<Option<(String, std::time::Instant)>>>,
    data_dir: PathBuf,
    rate_limit_tracker: Arc<RateLimitTracker>,
    routing_config: Arc<tokio::sync::RwLock<SmartRoutingConfig>>,
    session_accounts: Arc<DashMap<String, String>>,
    adaptive_limits: Arc<tokio::sync::RwLock<Option<Arc<AdaptiveLimitManager>>>>,
    preferred_account_id: Arc<tokio::sync::RwLock<Option<String>>>,
    health_scores: Arc<DashMap<String, f32>>,
    active_requests: Arc<DashMap<String, AtomicU32>>,
    session_failures: Arc<DashMap<String, AtomicU32>>,
}

impl TokenManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
            current_index: Arc::new(AtomicUsize::new(0)),
            last_used_account: Arc::new(tokio::sync::Mutex::new(None)),
            data_dir,
            rate_limit_tracker: Arc::new(RateLimitTracker::new()),
            routing_config: Arc::new(tokio::sync::RwLock::new(SmartRoutingConfig::default())),
            session_accounts: Arc::new(DashMap::new()),
            adaptive_limits: Arc::new(tokio::sync::RwLock::new(None)),
            preferred_account_id: Arc::new(tokio::sync::RwLock::new(None)),
            health_scores: Arc::new(DashMap::new()),
            active_requests: Arc::new(DashMap::new()),
            session_failures: Arc::new(DashMap::new()),
        }
    }

    pub async fn set_routing_config(&self, config: SmartRoutingConfig) {
        let mut guard = self.routing_config.write().await;
        *guard = config;
    }

    pub fn increment_active_requests(&self, account_id: &str) -> u32 {
        self.active_requests
            .entry(account_id.to_string())
            .or_insert_with(|| AtomicU32::new(0))
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }

    pub fn decrement_active_requests(&self, account_id: &str) {
        if let Some(counter) = self.active_requests.get(account_id) {
            let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                if v > 0 {
                    Some(v - 1)
                } else {
                    None
                }
            });
        }
    }

    pub fn get_active_requests(&self, account_id: &str) -> u32 {
        self.active_requests
            .get(account_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
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

        // Reload should reflect current on-disk state (accounts can be added/removed/disabled).
        self.tokens.clear();
        self.current_index.store(0, Ordering::SeqCst);
        {
            let mut last_used = self.last_used_account.lock().await;
            *last_used = None;
        }

        let entries =
            std::fs::read_dir(&accounts_dir).map_err(|e| format!("读取账号目录失败: {}", e))?;

        let mut count = 0;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // 尝试加载账号
            match self.load_single_account(&path).await {
                Ok(Some(token)) => {
                    let account_id = token.account_id.clone();
                    self.tokens.insert(account_id, token);
                    count += 1;
                }
                Ok(None) => {
                    // 跳过无效账号
                }
                Err(e) => {
                    tracing::debug!("加载账号失败 {:?}: {}", path, e);
                }
            }
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
                // [v4.0.5] Auto-clear rate limit when reloading account
                self.clear_rate_limit(account_id);
                Ok(())
            }
            Ok(None) => Err("账号加载失败".to_string()),
            Err(e) => Err(format!("同步账号失败: {}", e)),
        }
    }

    pub async fn reload_all_accounts(&self) -> Result<usize, String> {
        let count = self.load_accounts().await?;
        // [v4.0.5] Auto-clear all rate limits when reloading all accounts
        self.clear_all_rate_limits();
        Ok(count)
    }

    /// 加载单个账号
    async fn load_single_account(&self, path: &PathBuf) -> Result<Option<ProxyToken>, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?;

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
            .and_then(Self::calculate_max_quota_percentage);

        // [FIX #621] 提取受保护模型列表 (quota exhausted models)
        let protected_models: HashSet<String> = account
            .get("protected_models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let health_score = self
            .health_scores
            .get(&account_id)
            .map(|v| *v)
            .unwrap_or(1.0);

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

    fn calculate_max_quota_percentage(quota: &serde_json::Value) -> Option<i32> {
        let models = quota.get("models")?.as_array()?;
        let mut max_percentage = 0;
        let mut has_data = false;

        for model in models {
            if let Some(pct) = model.get("percentage").and_then(|v| v.as_i64()) {
                let pct_i32 = pct as i32;
                if pct_i32 > max_percentage {
                    max_percentage = pct_i32;
                }
                has_data = true;
            }
        }

        if has_data {
            Some(max_percentage)
        } else {
            None
        }
    }

    /// 获取当前可用的 Token（支持粘性会话与智能调度）
    /// 参数 `quota_group` 用于区分 "claude" vs "gemini" 组
    /// 参数 `force_rotate` 为 true 时将忽略锁定，强制切换账号
    /// 参数 `session_id` 用于跨请求维持会话粘性
    /// 参数 `target_model` 目标模型名称（用于配额保护检查）
    pub async fn get_token(
        &self,
        quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str, // [FIX #621] Now used for quota protection
    ) -> Result<(String, String, String), String> {
        // 【优化 Issue #284】添加 5 秒超时，防止死锁
        let timeout_duration = std::time::Duration::from_secs(5);
        match tokio::time::timeout(
            timeout_duration,
            self.get_token_internal(quota_group, force_rotate, session_id, target_model),
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
            if !self.is_rate_limited(&token.account_id) {
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
        quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str, // [FIX #621] Used for quota protection
    ) -> Result<(String, String, String), String> {
        let mut tokens_snapshot: Vec<ProxyToken> =
            self.tokens.iter().map(|e| e.value().clone()).collect();
        let total = tokens_snapshot.len();
        if total == 0 {
            return Err("Token pool is empty".to_string());
        }

        // ===== 【优化】根据订阅等级和剩余配额排序 =====
        // [FIX #563] 优先级: ULTRA > PRO > FREE, 同tier内优先高配额账号
        // 理由: ULTRA/PRO 重置快，优先消耗；FREE 重置慢，用于兜底
        //       高配额账号优先使用，避免低配额账号被用光
        tokens_snapshot.sort_by(|a, b| {
            let tier_priority = |tier: &Option<String>| match tier.as_deref() {
                Some("ULTRA") => 0,
                Some("PRO") => 1,
                Some("FREE") => 2,
                _ => 3,
            };

            // First: compare by subscription tier
            let tier_cmp =
                tier_priority(&a.subscription_tier).cmp(&tier_priority(&b.subscription_tier));

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

        // ===== [FIX #820] Fixed Account Mode: prefer specified account =====
        let preferred_id = self.preferred_account_id.read().await.clone();
        if let Some(ref pref_id) = preferred_id {
            if let Some(preferred_token) = tokens_snapshot.iter().find(|t| &t.account_id == pref_id)
            {
                let is_rate_limited =
                    self.is_rate_limited_for_model(&preferred_token.account_id, &normalized_target);
                // [FIX #621] Check if model is quota-protected
                let is_quota_protected = quota_protection_enabled
                    && preferred_token
                        .protected_models
                        .contains(&normalized_target);

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

                    return Ok((token.access_token, project_id, token.email));
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

        // 【优化 Issue #284】将锁操作移到循环外，避免重复获取锁
        // 预先获取 last_used_account 的快照，避免在循环中多次加锁
        // 【FIX TOCTOU】保存原始快照用于 Compare-And-Swap 验证
        let last_used_account_snapshot = if quota_group != "image_gen" {
            let last_used = self.last_used_account.lock().await;
            last_used.clone()
        } else {
            None
        };
        // Clone for loop usage (immutable reference)
        let last_used_account_id = last_used_account_snapshot.clone();

        let mut attempted: HashSet<String> = HashSet::new();
        let mut last_error: Option<String> = None;
        let mut need_update_last_used: Option<(String, std::time::Instant)> = None;

        // 获取 AIMD tracker 引用 (避免在循环中多次获取锁)
        let aimd = self.adaptive_limits.read().await.clone();

        for attempt in 0..total {
            let rotate = force_rotate || attempt > 0;

            // ===== 【核心】粘性会话与智能调度逻辑 =====
            let mut target_token: Option<ProxyToken> = None;

            // Check if session has too many consecutive failures - force unbind
            if let Some(sid) = session_id {
                let failures = self.get_session_failures(sid);
                if failures >= 3 {
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

            // 模式 A: 粘性会话处理 (CacheFirst 或 Balance 且有 session_id)
            if let Some(sid) = session_id {
                if !rotate && routing.enable_session_affinity {
                    // 1. 检查会话是否已绑定账号
                    if let Some(bound_id) = self.session_accounts.get(sid).map(|v| v.clone()) {
                        // 2. 检查绑定的账号是否限流 (使用精准的剩余时间接口)
                        let reset_sec = self.rate_limit_tracker.get_remaining_wait(&bound_id);
                        if reset_sec > 0 {
                            // Long lockout (>5min) = QUOTA_EXHAUSTED, unbind session
                            // Short lockout = temporary rate limit, keep session for cache affinity
                            if reset_sec > 300 {
                                self.session_accounts.remove(sid);
                                tracing::warn!(
                                    "Session {} unbound from {} (quota exhausted, {}s lockout)",
                                    sid,
                                    bound_id,
                                    reset_sec
                                );
                            } else {
                                tracing::debug!(
                                    "Session {} bound account {} rate-limited ({}s). Migrating THIS request only.",
                                    sid, bound_id, reset_sec
                                );
                            }
                            // target_token stays None, falls through to least-connections
                        } else if !attempted.contains(&bound_id) {
                            // 3. 账号可用且未被标记为尝试失败
                            // [FIX #621] Also check quota protection
                            let is_quota_protected = quota_protection_enabled
                                && tokens_snapshot
                                    .iter()
                                    .find(|t| t.account_id == bound_id)
                                    .is_some_and(|t| {
                                        t.protected_models.contains(&normalized_target)
                                    });

                            if is_quota_protected {
                                tracing::debug!(
                                    "Sticky Session: Bound account {} is quota-protected for {}, unbinding",
                                    bound_id, normalized_target
                                );
                                self.session_accounts.remove(sid);
                            } else if let Some(found) =
                                tokens_snapshot.iter().find(|t| t.account_id == bound_id)
                            {
                                tracing::debug!(
                                    "Sticky Session: Successfully reusing bound account {} for session {}",
                                    found.email,
                                    sid
                                );
                                target_token = Some(found.clone());
                            }
                        }
                    }
                }
            }

            // 模式 B: 原子化 60s 全局锁定 (针对无 session_id 情况的默认保护)
            if target_token.is_none() && !rotate && quota_group != "image_gen" {
                // 【优化】使用预先获取的快照，不再在循环内加锁
                if let Some((account_id, last_time)) = &last_used_account_id {
                    if last_time.elapsed().as_secs() < 60 && !attempted.contains(account_id) {
                        // [FIX] Check rate limit BEFORE reusing account in 60s window
                        if self.is_rate_limited_for_model(account_id, &normalized_target) {
                            tracing::debug!(
                                "60s Window: Last account {} is rate-limited for {}, skipping",
                                account_id,
                                normalized_target
                            );
                        } else if let Some(found) =
                            tokens_snapshot.iter().find(|t| &t.account_id == account_id)
                        {
                            // [FIX #621] Check quota protection before reusing
                            let is_quota_protected = quota_protection_enabled
                                && found.protected_models.contains(&normalized_target);

                            if !is_quota_protected {
                                tracing::debug!(
                                    "60s Window: Force reusing last account: {}",
                                    found.email
                                );
                                target_token = Some(found.clone());
                            } else {
                                tracing::debug!(
                                    "60s Window: Last account {} is quota-protected for {}, skipping",
                                    found.email, normalized_target
                                );
                            }
                        }
                    }
                }

                // Select account using least-connections algorithm (prevents thundering herd)
                if target_token.is_none() {
                    let mut best_candidate: Option<&ProxyToken> = None;
                    let mut min_active = u32::MAX;

                    for candidate in &tokens_snapshot {
                        if attempted.contains(&candidate.account_id) {
                            continue;
                        }

                        if self.is_rate_limited_for_model(&candidate.account_id, &normalized_target)
                        {
                            continue;
                        }

                        if quota_protection_enabled
                            && candidate.protected_models.contains(&normalized_target)
                        {
                            continue;
                        }

                        // Check concurrency limit
                        let active = self.get_active_requests(&candidate.account_id);
                        if active >= routing.max_concurrent_per_account {
                            tracing::debug!(
                                "Account {} at concurrency limit ({}/{}), skipping",
                                candidate.email,
                                active,
                                routing.max_concurrent_per_account
                            );
                            continue;
                        }

                        // AIMD predictive check
                        if let Some(aimd) = &aimd {
                            if aimd.usage_ratio(&candidate.account_id) > 1.2 {
                                tracing::debug!(
                                    "AIMD: Skipping account {} (usage ratio > 1.2)",
                                    candidate.email
                                );
                                continue;
                            }
                        }

                        // Least-connections selection
                        if active < min_active {
                            min_active = active;
                            best_candidate = Some(candidate);
                        }
                    }

                    if let Some(candidate) = best_candidate {
                        target_token = Some(candidate.clone());
                        need_update_last_used =
                            Some((candidate.account_id.clone(), std::time::Instant::now()));

                        if let Some(sid) = session_id {
                            if routing.enable_session_affinity {
                                self.session_accounts
                                    .insert(sid.to_string(), candidate.account_id.clone());
                                tracing::debug!(
                                    "Sticky Session: Bound new account {} to session {} (active: {})",
                                    candidate.email,
                                    sid,
                                    min_active
                                );
                            }
                        }
                    }
                }
            } else if target_token.is_none() {
                // Force rotation or no session affinity - use least-connections
                let mut best_candidate: Option<&ProxyToken> = None;
                let mut min_active = u32::MAX;

                for candidate in &tokens_snapshot {
                    if attempted.contains(&candidate.account_id) {
                        continue;
                    }

                    if self.is_rate_limited_for_model(&candidate.account_id, &normalized_target) {
                        continue;
                    }

                    if quota_protection_enabled
                        && candidate.protected_models.contains(&normalized_target)
                    {
                        continue;
                    }

                    let active = self.get_active_requests(&candidate.account_id);
                    if active >= routing.max_concurrent_per_account {
                        continue;
                    }

                    if let Some(aimd) = &aimd {
                        if aimd.usage_ratio(&candidate.account_id) > 1.2 {
                            continue;
                        }
                    }

                    if active < min_active {
                        min_active = active;
                        best_candidate = Some(candidate);
                    }
                }

                if let Some(candidate) = best_candidate {
                    target_token = Some(candidate.clone());
                    if rotate {
                        tracing::debug!(
                            "Force Rotation: Switched to account {} (active: {})",
                            candidate.email,
                            min_active
                        );
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
                        .filter_map(|t| self.rate_limit_tracker.get_reset_seconds(&t.account_id))
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

                            // 重新尝试选择账号
                            let retry_token = tokens_snapshot.iter().find(|t| {
                                !attempted.contains(&t.account_id)
                                    && !self.is_rate_limited(&t.account_id)
                            });

                            if let Some(t) = retry_token {
                                tracing::info!(
                                    "✅ Buffer delay successful! Found available account: {}",
                                    t.email
                                );
                                t.clone()
                            } else {
                                // Layer 2: 缓冲后仍无可用账号,执行乐观重置
                                tracing::warn!(
                                    "Buffer delay failed. Executing optimistic reset for all {} accounts...",
                                    tokens_snapshot.len()
                                );

                                // 清除所有限流记录
                                self.rate_limit_tracker.clear_all();

                                // 再次尝试选择账号
                                let final_token = tokens_snapshot
                                    .iter()
                                    .find(|t| !attempted.contains(&t.account_id));

                                if let Some(t) = final_token {
                                    tracing::info!(
                                        "✅ Optimistic reset successful! Using account: {}",
                                        t.email
                                    );
                                    t.clone()
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
                        // 无限流记录但仍无可用账号,可能是其他问题
                        return Err("All accounts failed or unhealthy.".to_string());
                    }
                }
            };

            // Ensure session is always bound to the selected account
            // This covers all selection paths: rotation, fallback, optimistic reset
            if let Some(sid) = session_id {
                if routing.enable_session_affinity {
                    let current_binding = self.session_accounts.get(sid).map(|v| v.clone());
                    if current_binding.as_ref() != Some(&token.account_id) {
                        self.session_accounts
                            .insert(sid.to_string(), token.account_id.clone());
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
                        attempted.insert(token.account_id.clone());

                        // 【优化】标记需要清除锁定，避免在循环内加锁
                        if quota_group != "image_gen"
                            && matches!(&last_used_account_id, Some((id, _)) if id == &token.account_id)
                        {
                            need_update_last_used =
                                Some((String::new(), std::time::Instant::now()));
                            // 空字符串表示需要清除
                        }
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
                        attempted.insert(token.account_id.clone());

                        // 【优化】标记需要清除锁定，避免在循环内加锁
                        if quota_group != "image_gen"
                            && matches!(&last_used_account_id, Some((id, _)) if id == &token.account_id)
                        {
                            need_update_last_used =
                                Some((String::new(), std::time::Instant::now()));
                            // 空字符串表示需要清除
                        }
                        continue;
                    }
                }
            };

            // 【优化】在成功返回前，统一更新 last_used_account（如果需要）
            if let Some((new_account_id, new_time)) = need_update_last_used {
                if quota_group != "image_gen" {
                    let mut last_used = self.last_used_account.lock().await;
                    if new_account_id.is_empty() {
                        // 空字符串表示需要清除锁定
                        *last_used = None;
                    } else {
                        *last_used = Some((new_account_id, new_time));
                    }
                }
            }

            return Ok((token.access_token, project_id, token.email));
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

        let mut content: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {}", e))?,
        )
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;

        let now = chrono::Utc::now().timestamp();
        content["disabled"] = serde_json::Value::Bool(true);
        content["disabled_at"] = serde_json::Value::Number(now.into());
        content["disabled_reason"] = serde_json::Value::String(truncate_reason(reason, 800));

        std::fs::write(&path, serde_json::to_string_pretty(&content).unwrap())
            .map_err(|e| format!("写入文件失败: {}", e))?;

        tracing::warn!("Account disabled: {} ({:?})", account_id, path);
        Ok(())
    }

    /// 保存 project_id 到账号文件
    async fn save_project_id(&self, account_id: &str, project_id: &str) -> Result<(), String> {
        let entry = self.tokens.get(account_id).ok_or("账号不存在")?;

        let path = &entry.account_path;

        let mut content: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?,
        )
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;

        content["token"]["project_id"] = serde_json::Value::String(project_id.to_string());

        std::fs::write(path, serde_json::to_string_pretty(&content).unwrap())
            .map_err(|e| format!("写入文件失败: {}", e))?;

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

        let path = &entry.account_path;

        let mut content: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?,
        )
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;

        let now = chrono::Utc::now().timestamp();

        content["token"]["access_token"] =
            serde_json::Value::String(token_response.access_token.clone());
        content["token"]["expires_in"] =
            serde_json::Value::Number(token_response.expires_in.into());
        content["token"]["expiry_timestamp"] =
            serde_json::Value::Number((now + token_response.expires_in).into());

        std::fs::write(path, serde_json::to_string_pretty(&content).unwrap())
            .map_err(|e| format!("写入文件失败: {}", e))?;

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
    pub fn get_quota_reset_time(&self, email: &str) -> Option<String> {
        // 尝试从账号文件读取配额信息
        let accounts_dir = self.data_dir.join("accounts");

        // 遍历账号文件查找对应的 email
        if let Ok(entries) = std::fs::read_dir(&accounts_dir) {
            for entry in entries.flatten() {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(account) = serde_json::from_str::<serde_json::Value>(&content) {
                        // 检查 email 是否匹配
                        if account.get("email").and_then(|e| e.as_str()) == Some(email) {
                            // 获取 quota.models 中最早的 reset_time
                            if let Some(models) = account
                                .get("quota")
                                .and_then(|q| q.get("models"))
                                .and_then(|m| m.as_array())
                            {
                                // 找到最早的 reset_time（最保守的锁定策略）
                                let mut earliest_reset: Option<&str> = None;
                                for model in models {
                                    if let Some(reset_time) =
                                        model.get("reset_time").and_then(|r| r.as_str())
                                    {
                                        if !reset_time.is_empty()
                                            && earliest_reset
                                                .as_ref()
                                                .is_none_or(|earliest| reset_time < *earliest)
                                        {
                                            earliest_reset = Some(reset_time);
                                        }
                                    }
                                }
                                if let Some(reset) = earliest_reset {
                                    return Some(reset.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// 使用配额刷新时间精确锁定账号
    ///
    /// 当 API 返回 429 但没有 quotaResetDelay 时,尝试使用账号的配额刷新时间
    ///
    /// # 参数
    /// - `model`: 可选的模型名称,用于模型级别限流
    pub fn set_precise_lockout(
        &self,
        email: &str,
        reason: crate::proxy::rate_limit::RateLimitReason,
        model: Option<String>,
    ) -> bool {
        if let Some(reset_time_str) = self.get_quota_reset_time(email) {
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
        // Parse error reason FIRST to choose strategy
        let reason = self.rate_limit_tracker.parse_rate_limit_reason(error_body);
        let model_str = model.unwrap_or("unknown");

        // Check if API returned explicit retry time
        let has_explicit_retry_time =
            retry_after_header.is_some() || error_body.contains("quotaResetDelay");

        if has_explicit_retry_time {
            // API returned precise time — use it directly, set per-model lock
            if let Some(info) = self.rate_limit_tracker.parse_from_error(
                account_id,
                status,
                retry_after_header,
                error_body,
                model.map(|s| s.to_string()),
            ) {
                // Also set model-specific lock
                self.rate_limit_tracker.set_model_lockout(
                    account_id,
                    model_str,
                    info.reset_time,
                    reason,
                );
            }
            return;
        }

        // No explicit time — strategy depends on error type
        match reason {
            crate::proxy::rate_limit::RateLimitReason::QuotaExhausted => {
                // QUOTA_EXHAUSTED: permanent block until quota refresh
                // Dynamically add model to protected_models in memory
                if let Some(mut token) = self.tokens.get_mut(account_id) {
                    if !token.protected_models.contains(model_str) {
                        token.protected_models.insert(model_str.to_string());
                        tracing::warn!(
                            "🛡️ {}:{} added to protected_models (quota exhausted)",
                            account_id,
                            model_str
                        );
                    }
                }

                // Temporary lockout while fetching precise reset time
                let lockout = std::time::Duration::from_secs(600);
                self.rate_limit_tracker.set_model_lockout(
                    account_id,
                    model_str,
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
                // RATE_LIMIT_EXCEEDED or other: adaptive short lockout (5→15→30→60s)
                let lockout_secs = self
                    .rate_limit_tracker
                    .set_adaptive_model_lockout(account_id, model_str);
                tracing::debug!(
                    "⚡ {}:{} adaptive lockout: {}s",
                    account_id,
                    model_str,
                    lockout_secs
                );
            }
        }

        // Try to get precise reset time async
        if self
            .fetch_and_lock_with_realtime_quota(account_id, reason, model.map(|s| s.to_string()))
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
        if self.set_precise_lockout(account_id, reason, model.map(|s| s.to_string())) {
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

fn truncate_reason(reason: &str, max_len: usize) -> String {
    if reason.chars().count() <= max_len {
        return reason.to_string();
    }
    let mut s: String = reason.chars().take(max_len).collect();
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_manager() -> TokenManager {
        TokenManager::new(PathBuf::from("/tmp/test_antigravity"))
    }

    #[test]
    fn test_new_manager_is_empty() {
        let manager = create_test_manager();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_rate_limit_integration() {
        let manager = create_test_manager();
        let account_id = "test_account_123";

        assert!(!manager.is_rate_limited(account_id));

        manager.mark_rate_limited(account_id, 429, Some("60"), "");
        assert!(manager.is_rate_limited(account_id));

        manager.mark_account_success(account_id);
        assert!(!manager.is_rate_limited(account_id));
    }

    #[test]
    fn test_rate_limit_with_model() {
        let manager = create_test_manager();
        let account_id = "test_account_456";

        manager.mark_rate_limited_with_model(
            account_id,
            429,
            Some("30"),
            "",
            Some("gemini-pro".to_string()),
        );

        assert!(manager.is_rate_limited(account_id));
        let info = manager.rate_limit_tracker().get(account_id);
        assert!(info.is_some());
        assert_eq!(info.unwrap().model, Some("gemini-pro".to_string()));
    }

    #[tokio::test]
    async fn test_preferred_account_mode() {
        let manager = create_test_manager();

        assert!(manager.get_preferred_account().await.is_none());

        manager
            .set_preferred_account(Some("fixed_account".to_string()))
            .await;
        assert_eq!(
            manager.get_preferred_account().await,
            Some("fixed_account".to_string())
        );

        manager.set_preferred_account(None).await;
        assert!(manager.get_preferred_account().await.is_none());
    }

    #[tokio::test]
    async fn test_routing_config_update() {
        let manager = create_test_manager();

        let initial = manager.get_routing_config().await;
        assert!(initial.enable_session_affinity);
        assert_eq!(initial.max_concurrent_per_account, 3);

        let new_config = SmartRoutingConfig {
            enable_session_affinity: false,
            max_concurrent_per_account: 5,
            ..Default::default()
        };
        manager.update_routing_config(new_config).await;

        let updated = manager.get_routing_config().await;
        assert!(!updated.enable_session_affinity);
        assert_eq!(updated.max_concurrent_per_account, 5);
    }

    #[test]
    fn test_active_requests_increment_decrement() {
        let manager = create_test_manager();

        assert_eq!(manager.get_active_requests("account_a"), 0);

        let count = manager.increment_active_requests("account_a");
        assert_eq!(count, 1);
        assert_eq!(manager.get_active_requests("account_a"), 1);

        let count = manager.increment_active_requests("account_a");
        assert_eq!(count, 2);

        manager.decrement_active_requests("account_a");
        assert_eq!(manager.get_active_requests("account_a"), 1);

        manager.decrement_active_requests("account_a");
        assert_eq!(manager.get_active_requests("account_a"), 0);
    }

    #[test]
    fn test_active_requests_underflow_protection() {
        let manager = create_test_manager();

        manager.decrement_active_requests("nonexistent");
        assert_eq!(manager.get_active_requests("nonexistent"), 0);

        manager.increment_active_requests("account_b");
        manager.decrement_active_requests("account_b");
        manager.decrement_active_requests("account_b");
        assert_eq!(manager.get_active_requests("account_b"), 0);
    }

    #[test]
    fn test_session_bindings() {
        let manager = create_test_manager();

        manager
            .session_accounts
            .insert("session_1".to_string(), "account_a".to_string());
        manager
            .session_accounts
            .insert("session_2".to_string(), "account_b".to_string());

        assert_eq!(manager.session_accounts.len(), 2);

        manager.clear_session_binding("session_1");
        assert_eq!(manager.session_accounts.len(), 1);

        manager.clear_all_sessions();
        assert_eq!(manager.session_accounts.len(), 0);
    }

    #[test]
    fn test_truncate_reason() {
        assert_eq!(truncate_reason("short", 10), "short");
        assert_eq!(
            truncate_reason("this is a very long reason", 10),
            "this is a …"
        );
        assert_eq!(truncate_reason("exact10chr", 10), "exact10chr");
    }

    #[tokio::test]
    async fn test_adaptive_limits_injection() {
        let manager = create_test_manager();

        {
            let guard = manager.adaptive_limits.read().await;
            assert!(guard.is_none());
        }

        let limits = Arc::new(AdaptiveLimitManager::new(0.8, Default::default()));
        manager.set_adaptive_limits(limits).await;

        {
            let guard = manager.adaptive_limits.read().await;
            assert!(guard.is_some());
        }
    }
}
