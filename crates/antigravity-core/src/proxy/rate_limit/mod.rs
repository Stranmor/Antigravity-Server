mod parser;
mod types;

pub use types::{RateLimitInfo, RateLimitKey, RateLimitReason};

use dashmap::DashMap;
use std::time::{Duration, SystemTime};

const FAILURE_COUNT_EXPIRY_SECONDS: u64 = 3600;

fn duration_to_secs_ceil(d: Duration) -> u64 {
    let secs = d.as_secs();
    if d.subsec_nanos() > 0 {
        secs + 1
    } else {
        secs
    }
}

pub struct RateLimitTracker {
    limits: DashMap<RateLimitKey, RateLimitInfo>,
    failure_counts: DashMap<RateLimitKey, (u32, SystemTime)>,
}

impl RateLimitTracker {
    pub fn new() -> Self {
        Self {
            limits: DashMap::new(),
            failure_counts: DashMap::new(),
        }
    }

    /// 获取账号剩余的等待时间(秒)
    pub fn get_remaining_wait(&self, account_id: &str) -> u64 {
        let key = RateLimitKey::account(account_id);
        if let Some(info) = self.limits.get(&key) {
            let now = SystemTime::now();
            if info.reset_time > now {
                let duration = info
                    .reset_time
                    .duration_since(now)
                    .unwrap_or(Duration::from_secs(0));
                return duration_to_secs_ceil(duration);
            }
        }
        0
    }

    /// 标记账号请求成功，重置连续失败计数
    ///
    /// 当账号成功完成请求后调用此方法，将其失败计数归零，
    /// 这样下次失败时会从最短的锁定时间（60秒）开始。
    pub fn mark_success(&self, account_id: &str) {
        let key = RateLimitKey::account(account_id);
        if self.failure_counts.remove(&key).is_some() {
            tracing::debug!("账号 {} 请求成功，已重置失败计数", account_id);
        }
        // 同时清除限流记录（如果有）
        self.limits.remove(&key);
    }

    /// Set adaptive temporary lockout based on consecutive failure count.
    /// Returns the lockout duration in seconds.
    ///
    /// Progression: 5s → 15s → 30s → 60s (max)
    /// Resets on success (via mark_success)
    pub fn set_adaptive_temporary_lockout(&self, account_id: &str) -> u64 {
        let now = SystemTime::now();
        let key = RateLimitKey::account(account_id);

        let failure_count = {
            let mut entry = self.failure_counts.entry(key.clone()).or_insert((0, now));

            // Check expiry (1 hour)
            let elapsed = now
                .duration_since(entry.1)
                .unwrap_or(Duration::from_secs(0))
                .as_secs();
            if elapsed > FAILURE_COUNT_EXPIRY_SECONDS {
                *entry = (0, now);
            }

            entry.0 += 1;
            entry.1 = now;
            entry.0
        };

        let lockout_secs = match failure_count {
            1 => 5,
            2 => 15,
            3 => 30,
            _ => 60,
        };

        let info = RateLimitInfo {
            reset_time: now + Duration::from_secs(lockout_secs),
            retry_after_sec: lockout_secs,
            detected_at: now,
            reason: RateLimitReason::Unknown,
            model: None,
        };

        self.limits.insert(key, info);

        tracing::debug!(
            "⚡ Account {} adaptive lockout: {}s (attempt #{})",
            account_id,
            lockout_secs,
            failure_count
        );

        lockout_secs
    }

    /// 精确锁定账号到指定时间点
    ///
    /// 使用账号配额中的 reset_time 来精确锁定账号,
    /// 这比指数退避更加精准。
    ///
    /// # 参数
    /// - `model`: 可选的模型名称,用于模型级别限流。None 表示账号级别限流
    pub fn set_lockout_until(
        &self,
        account_id: &str,
        reset_time: SystemTime,
        reason: RateLimitReason,
        model: Option<String>,
    ) {
        let now = SystemTime::now();
        let retry_sec = reset_time
            .duration_since(now)
            .map(|d| d.as_secs())
            .unwrap_or(60); // 如果时间已过,使用默认 60 秒

        let info = RateLimitInfo {
            reset_time,
            retry_after_sec: retry_sec,
            detected_at: now,
            reason,
            model: model.clone(),
        };

        // Type-safe key construction via RateLimitKey
        let key = RateLimitKey::from_optional_model(account_id, model.as_deref());
        self.limits.insert(key, info);

        if let Some(m) = &model {
            tracing::info!(
                "账号 {} 的模型 {} 已精确锁定到配额刷新时间,剩余 {} 秒",
                account_id,
                m,
                retry_sec
            );
        } else {
            tracing::info!(
                "账号 {} 已精确锁定到配额刷新时间,剩余 {} 秒",
                account_id,
                retry_sec
            );
        }
    }

    /// 使用 ISO 8601 时间字符串精确锁定账号
    ///
    /// 解析类似 "2026-01-08T17:00:00Z" 格式的时间字符串
    ///
    /// # 参数
    /// - `model`: 可选的模型名称,用于模型级别限流
    pub fn set_lockout_until_iso(
        &self,
        account_id: &str,
        reset_time_str: &str,
        reason: RateLimitReason,
        model: Option<String>,
    ) -> bool {
        // 尝试解析 ISO 8601 格式
        match chrono::DateTime::parse_from_rfc3339(reset_time_str) {
            Ok(dt) => {
                let ts = dt.timestamp();
                if ts < 0 {
                    tracing::warn!("配额刷新时间 '{}' 在 1970 之前，忽略", reset_time_str);
                    return false;
                }
                let reset_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(ts as u64);
                self.set_lockout_until(account_id, reset_time, reason, model);
                true
            }
            Err(e) => {
                tracing::warn!(
                    "无法解析配额刷新时间 '{}': {},将使用默认退避策略",
                    reset_time_str,
                    e
                );
                false
            }
        }
    }

    /// 从错误响应解析限流信息
    ///
    /// # Arguments
    /// * `account_id` - 账号 ID
    /// * `status` - HTTP 状态码
    /// * `retry_after_header` - Retry-After header 值
    /// * `body` - 错误响应 body
    pub fn parse_from_error(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        body: &str,
        model: Option<String>,
    ) -> Option<RateLimitInfo> {
        // 支持 429 (限流) 以及 500/503/529 (后端故障软避让)
        if status != 429 && status != 500 && status != 503 && status != 529 {
            return None;
        }

        // 1. 解析限流原因类型
        let reason = if status == 429 {
            tracing::warn!("Google 429 Error Body: {}", body);
            self.parse_rate_limit_reason(body)
        } else {
            RateLimitReason::ServerError
        };

        // [FIX] ModelCapacityExhausted: НЕ блокируем аккаунт вообще!
        // Это временная перегрузка GPU, handler должен просто сделать retry с задержкой
        if reason == RateLimitReason::ModelCapacityExhausted {
            tracing::debug!(
                "MODEL_CAPACITY_EXHAUSTED для {}: НЕ блокируем, handler сделает retry",
                account_id
            );
            // Возвращаем None — аккаунт остаётся доступным для retry
            return None;
        }

        let mut retry_after_sec = None;

        // 2. 从 Retry-After header 提取
        if let Some(retry_after) = retry_after_header {
            if let Ok(seconds) = retry_after.parse::<u64>() {
                retry_after_sec = Some(seconds);
            }
        }

        // 3. 从错误消息提取 (优先尝试 JSON 解析，再试正则)
        if retry_after_sec.is_none() {
            retry_after_sec = parser::parse_retry_time_from_body(body);
        }

        // 4. 处理默认值与软避让逻辑（根据限流类型设置不同默认值）
        let retry_sec = match retry_after_sec {
            Some(s) => {
                // 引入 PR #28 的安全缓冲区：最小 2 秒，防止极高频无效重试
                if s < 2 {
                    2
                } else {
                    s
                }
            }
            None => {
                // 获取连续失败次数，用于指数退避（带自动过期逻辑）
                let failure_count = {
                    let now = SystemTime::now();
                    let key = RateLimitKey::from_optional_model(account_id, model.as_deref());
                    let mut entry = self.failure_counts.entry(key).or_insert((0, now));
                    // 检查是否超过过期时间，如果是则重置计数
                    let elapsed = now
                        .duration_since(entry.1)
                        .unwrap_or(Duration::from_secs(0))
                        .as_secs();
                    if elapsed > FAILURE_COUNT_EXPIRY_SECONDS {
                        tracing::debug!(
                            "账号 {} 失败计数已过期（{}秒），重置为 0",
                            account_id,
                            elapsed
                        );
                        *entry = (0, now);
                    }
                    entry.0 += 1;
                    entry.1 = now;
                    entry.0
                };

                match reason {
                    RateLimitReason::QuotaExhausted => {
                        // [智能限流] 根据连续失败次数动态调整锁定时间
                        // 第1次: 60s, 第2次: 5min, 第3次: 30min, 第4次+: 2h
                        let lockout = match failure_count {
                            1 => {
                                tracing::warn!(
                                    "检测到配额耗尽 (QUOTA_EXHAUSTED)，第1次失败，锁定 60秒"
                                );
                                60
                            }
                            2 => {
                                tracing::warn!(
                                    "检测到配额耗尽 (QUOTA_EXHAUSTED)，第2次连续失败，锁定 5分钟"
                                );
                                300
                            }
                            3 => {
                                tracing::warn!(
                                    "检测到配额耗尽 (QUOTA_EXHAUSTED)，第3次连续失败，锁定 30分钟"
                                );
                                1800
                            }
                            _ => {
                                tracing::warn!(
                                    "检测到配额耗尽 (QUOTA_EXHAUSTED)，第{}次连续失败，锁定 2小时",
                                    failure_count
                                );
                                7200
                            }
                        };
                        lockout
                    }
                    RateLimitReason::RateLimitExceeded => {
                        // 🔧 [FIX] 速率限制：降低默认值从 30秒 → 5秒
                        // 原因: 时间解析器修复后,多数情况会解析成功,不会走到这里
                        // 即使解析失败,5秒也足够应对瞬时限流
                        tracing::debug!("检测到速率限制 (RATE_LIMIT_EXCEEDED)，使用默认值 5秒");
                        5
                    }
                    RateLimitReason::ModelCapacityExhausted => {
                        // Unreachable: early return at line 215 handles this case
                        unreachable!("ModelCapacityExhausted should be handled by early return")
                    }
                    RateLimitReason::ServerError => {
                        // 服务器错误：执行"软避让"，默认锁定 20 秒
                        tracing::warn!("检测到 5xx 错误 ({}), 执行 20s 软避让...", status);
                        20
                    }
                    RateLimitReason::Unknown => {
                        // 未知原因：使用中等默认值（60秒）
                        tracing::debug!("无法解析 429 限流原因, 使用默认值 60秒");
                        60
                    }
                }
            }
        };

        let info = RateLimitInfo {
            reset_time: SystemTime::now() + Duration::from_secs(retry_sec),
            retry_after_sec: retry_sec,
            detected_at: SystemTime::now(),
            reason,
            model: model.clone(),
        };

        // Type-safe key construction via RateLimitKey
        let key = RateLimitKey::from_optional_model(account_id, model.as_deref());
        self.limits.insert(key, info.clone());

        tracing::warn!(
            "账号 {} [{}] 限流类型: {:?}, 重置延时: {}秒",
            account_id,
            status,
            reason,
            retry_sec
        );

        Some(info)
    }

    /// 解析限流原因类型
    pub fn parse_rate_limit_reason(&self, body: &str) -> RateLimitReason {
        // 尝试从 JSON 中提取 reason 字段
        let trimmed = body.trim();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(reason_str) = json
                    .get("error")
                    .and_then(|e| e.get("details"))
                    .and_then(|d| d.as_array())
                    .and_then(|a| a.first())
                    .and_then(|o| o.get("reason"))
                    .and_then(|v| v.as_str())
                {
                    return match reason_str {
                        "QUOTA_EXHAUSTED" => RateLimitReason::QuotaExhausted,
                        "RATE_LIMIT_EXCEEDED" => RateLimitReason::RateLimitExceeded,
                        "MODEL_CAPACITY_EXHAUSTED" => RateLimitReason::ModelCapacityExhausted,
                        _ => RateLimitReason::Unknown,
                    };
                }
                // [NEW] 尝试从 message 字段进行文本匹配（防止 missed reason）
                if let Some(msg) = json
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|v| v.as_str())
                {
                    let msg_lower = msg.to_lowercase();
                    if msg_lower.contains("per minute") || msg_lower.contains("rate limit") {
                        return RateLimitReason::RateLimitExceeded;
                    }
                }
            }
        }

        // 如果无法从 JSON 解析，尝试从消息文本判断
        let body_lower = body.to_lowercase();
        // [FIX] 优先判断分钟级限制，避免将 TPM 误判为 Quota
        if body_lower.contains("per minute")
            || body_lower.contains("rate limit")
            || body_lower.contains("too many requests")
        {
            RateLimitReason::RateLimitExceeded
        } else if body_lower.contains("exhausted") || body_lower.contains("quota") {
            RateLimitReason::QuotaExhausted
        } else {
            RateLimitReason::Unknown
        }
    }

    pub fn get(&self, account_id: &str) -> Option<RateLimitInfo> {
        let key = RateLimitKey::account(account_id);
        self.limits.get(&key).map(|r| r.clone())
    }

    pub fn get_for_model(&self, account_id: &str, model: &str) -> Option<RateLimitInfo> {
        let key = RateLimitKey::model(account_id, model);
        self.limits.get(&key).map(|r| r.clone())
    }

    /// 检查账号是否仍在限流中
    pub fn is_rate_limited(&self, account_id: &str) -> bool {
        if let Some(info) = self.get(account_id) {
            info.reset_time > SystemTime::now()
        } else {
            false
        }
    }

    /// Check if account is rate-limited for specific model.
    /// Checks both account-level AND model-specific limits.
    pub fn is_rate_limited_for_model(&self, account_id: &str, model: &str) -> bool {
        let now = SystemTime::now();

        // Check account-level limit
        let account_key = RateLimitKey::account(account_id);
        if let Some(info) = self.limits.get(&account_key) {
            if info.reset_time > now {
                return true;
            }
        }

        // Check model-specific limit
        let model_key = RateLimitKey::model(account_id, model);
        if let Some(info) = self.limits.get(&model_key) {
            if info.reset_time > now {
                return true;
            }
        }

        false
    }

    pub fn get_remaining_wait_for_model(&self, account_id: &str, model: &str) -> u64 {
        let now = SystemTime::now();
        let mut max_wait: u64 = 0;

        let account_key = RateLimitKey::account(account_id);
        if let Some(info) = self.limits.get(&account_key) {
            if info.reset_time > now {
                let duration = info
                    .reset_time
                    .duration_since(now)
                    .unwrap_or(Duration::from_secs(0));
                max_wait = max_wait.max(duration_to_secs_ceil(duration));
            }
        }

        let model_key = RateLimitKey::model(account_id, model);
        if let Some(info) = self.limits.get(&model_key) {
            if info.reset_time > now {
                let duration = info
                    .reset_time
                    .duration_since(now)
                    .unwrap_or(Duration::from_secs(0));
                max_wait = max_wait.max(duration_to_secs_ceil(duration));
            }
        }

        max_wait
    }

    /// Set lockout for specific account:model pair
    pub fn set_model_lockout(
        &self,
        account_id: &str,
        model: &str,
        reset_time: SystemTime,
        reason: RateLimitReason,
    ) {
        let now = SystemTime::now();
        let retry_sec = reset_time
            .duration_since(now)
            .map(|d| d.as_secs())
            .unwrap_or(60);

        let key = RateLimitKey::model(account_id, model);
        let info = RateLimitInfo {
            reset_time,
            retry_after_sec: retry_sec,
            detected_at: now,
            reason,
            model: Some(model.to_string()),
        };

        self.limits.insert(key, info);
        tracing::info!(
            "🔒 Account {}:{} locked for {}s ({:?})",
            account_id,
            model,
            retry_sec,
            reason
        );
    }

    /// Adaptive temporary lockout for specific model.
    /// Returns lockout duration. Progression: 5s → 15s → 30s → 60s
    pub fn set_adaptive_model_lockout(&self, account_id: &str, model: &str) -> u64 {
        let now = SystemTime::now();
        let key = RateLimitKey::model(account_id, model);

        let failure_count = {
            let mut entry = self.failure_counts.entry(key.clone()).or_insert((0, now));

            let elapsed = now
                .duration_since(entry.1)
                .unwrap_or(Duration::from_secs(0))
                .as_secs();
            if elapsed > FAILURE_COUNT_EXPIRY_SECONDS {
                *entry = (0, now);
            }

            entry.0 += 1;
            entry.1 = now;
            entry.0
        };

        let lockout_secs = match failure_count {
            1 => 5,
            2 => 15,
            3 => 30,
            _ => 60,
        };

        let info = RateLimitInfo {
            reset_time: now + Duration::from_secs(lockout_secs),
            retry_after_sec: lockout_secs,
            detected_at: now,
            reason: RateLimitReason::RateLimitExceeded,
            model: Some(model.to_string()),
        };

        self.limits.insert(key, info);

        tracing::debug!(
            "⚡ {}:{} adaptive lockout: {}s (attempt #{})",
            account_id,
            model,
            lockout_secs,
            failure_count
        );

        lockout_secs
    }

    /// Clear model-specific failure count on success
    pub fn mark_model_success(&self, account_id: &str, model: &str) {
        let key = RateLimitKey::model(account_id, model);
        if self.failure_counts.remove(&key).is_some() {
            tracing::debug!("{}:{} success, reset failure count", account_id, model);
        }
        self.limits.remove(&key);
    }

    /// 获取距离限流重置还有多少秒
    pub fn get_reset_seconds(&self, account_id: &str) -> Option<u64> {
        if let Some(info) = self.get(account_id) {
            info.reset_time
                .duration_since(SystemTime::now())
                .ok()
                .map(|d| d.as_secs())
        } else {
            None
        }
    }

    /// 清除过期的限流记录
    #[allow(dead_code)]
    pub fn cleanup_expired(&self) -> usize {
        let now = SystemTime::now();
        let mut count = 0;

        self.limits.retain(|_k, v| {
            if v.reset_time <= now {
                count += 1;
                false
            } else {
                true
            }
        });

        if count > 0 {
            tracing::debug!("清除了 {} 个过期的限流记录", count);
        }

        count
    }

    /// 清除指定账号的限流记录
    pub fn clear(&self, account_id: &str) -> bool {
        let key = RateLimitKey::account(account_id);
        self.limits.remove(&key).is_some()
    }

    /// 清除所有限流记录 (乐观重置策略)
    ///
    /// 用于乐观重置机制,当所有账号都被限流但等待时间很短时,
    /// 清除所有限流记录以解决时序竞争条件
    pub fn clear_all(&self) {
        let count = self.limits.len();
        self.limits.clear();
        tracing::warn!(
            "🔄 Optimistic reset: Cleared all {} rate limit record(s)",
            count
        );
    }
}

impl Default for RateLimitTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_retry_time_minutes_seconds() {
        let body = "Rate limit exceeded. Try again in 2m 30s";
        let time = parser::parse_retry_time_from_body(body);
        assert_eq!(time, Some(150));
    }

    #[test]
    fn test_parse_google_json_delay() {
        let body = r#"{
            "error": {
                "details": [
                    {
                        "metadata": {
                            "quotaResetDelay": "42s"
                        }
                    }
                ]
            }
        }"#;
        let time = parser::parse_retry_time_from_body(body);
        assert_eq!(time, Some(42));
    }

    #[test]
    fn test_parse_retry_after_ignore_case() {
        let body = "Quota limit hit. Retry After 99 Seconds";
        let time = parser::parse_retry_time_from_body(body);
        assert_eq!(time, Some(99));
    }

    #[test]
    fn test_get_remaining_wait() {
        let tracker = RateLimitTracker::new();
        tracker.parse_from_error("acc1", 429, Some("30"), "", None);
        let wait = tracker.get_remaining_wait("acc1");
        assert!(wait > 25 && wait <= 30);
    }

    #[test]
    fn test_safety_buffer() {
        let tracker = RateLimitTracker::new();
        // 如果 API 返回 1s，我们强制设为 2s
        tracker.parse_from_error("acc1", 429, Some("1"), "", None);
        let wait = tracker.get_remaining_wait("acc1");
        // Due to time passing, it might be 1 or 2
        assert!((1..=2).contains(&wait));
    }

    #[test]
    fn test_tpm_exhausted_is_rate_limit_exceeded() {
        let tracker = RateLimitTracker::new();
        // 模拟真实世界的 TPM 错误，同时包含 "Resource exhausted" 和 "per minute"
        let body = "Resource has been exhausted (e.g. check quota). Quota limit 'Tokens per minute' exceeded.";
        let reason = tracker.parse_rate_limit_reason(body);
        // 应该被识别为 RateLimitExceeded，而不是 QuotaExhausted
        assert_eq!(reason, RateLimitReason::RateLimitExceeded);
    }

    #[test]
    fn test_mark_success_clears_rate_limit() {
        let tracker = RateLimitTracker::new();
        tracker.parse_from_error("acc1", 429, Some("60"), "", None);
        assert!(tracker.is_rate_limited("acc1"));
        tracker.mark_success("acc1");
        assert!(!tracker.is_rate_limited("acc1"));
    }

    #[test]
    fn test_set_lockout_until_iso() {
        let tracker = RateLimitTracker::new();
        let future = chrono::Utc::now() + chrono::Duration::seconds(120);
        let iso_str = future.to_rfc3339();
        let result =
            tracker.set_lockout_until_iso("acc1", &iso_str, RateLimitReason::QuotaExhausted, None);
        assert!(result);
        assert!(tracker.is_rate_limited("acc1"));
        let remaining = tracker.get_remaining_wait("acc1");
        assert!((115..=125).contains(&remaining));
    }

    #[test]
    fn test_parse_duration_string_variants() {
        assert_eq!(parser::parse_duration_string("1h30m"), Some(5400));
        assert_eq!(parser::parse_duration_string("2h1m1s"), Some(7261));
        assert_eq!(parser::parse_duration_string("5m"), Some(300));
        assert_eq!(parser::parse_duration_string("30s"), Some(30));
        assert_eq!(parser::parse_duration_string("1h"), Some(3600));
    }

    #[test]
    fn test_cleanup_expired_removes_old_records() {
        let tracker = RateLimitTracker::new();
        let past = SystemTime::now() - Duration::from_secs(10);
        tracker.limits.insert(
            RateLimitKey::Account("expired".to_string()),
            RateLimitInfo {
                reset_time: past,
                retry_after_sec: 60,
                detected_at: past,
                reason: RateLimitReason::Unknown,
                model: None,
            },
        );
        let future = SystemTime::now() + Duration::from_secs(60);
        tracker.limits.insert(
            RateLimitKey::Account("active".to_string()),
            RateLimitInfo {
                reset_time: future,
                retry_after_sec: 60,
                detected_at: SystemTime::now(),
                reason: RateLimitReason::Unknown,
                model: None,
            },
        );
        let cleaned = tracker.cleanup_expired();
        assert_eq!(cleaned, 1);
        assert!(!tracker
            .limits
            .contains_key(&RateLimitKey::account("expired")));
        assert!(tracker
            .limits
            .contains_key(&RateLimitKey::account("active")));
    }

    #[test]
    fn test_clear_all_removes_everything() {
        let tracker = RateLimitTracker::new();
        tracker.parse_from_error("acc1", 429, Some("60"), "", None);
        tracker.parse_from_error("acc2", 429, Some("60"), "", None);
        assert!(tracker.is_rate_limited("acc1"));
        assert!(tracker.is_rate_limited("acc2"));
        tracker.clear_all();
        assert!(!tracker.is_rate_limited("acc1"));
        assert!(!tracker.is_rate_limited("acc2"));
    }

    #[test]
    fn test_model_level_rate_limit() {
        let tracker = RateLimitTracker::new();
        tracker.parse_from_error("acc1", 429, Some("60"), "", Some("gemini-pro".to_string()));
        assert!(tracker.is_rate_limited_for_model("acc1", "gemini-pro"));
        let info = tracker
            .get_for_model("acc1", "gemini-pro")
            .expect("should have rate limit");
        assert_eq!(info.model, Some("gemini-pro".to_string()));
    }
}
