use axum::{
    extract::Request,
    extract::State,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::net::IpAddr;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;

use super::rate_limiter;
use crate::proxy::{ProxyAuthMode, ProxySecurityConfig};

pub async fn auth_middleware(
    state: State<Arc<RwLock<ProxySecurityConfig>>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    auth_middleware_internal(state, request, next, false).await
}

pub async fn admin_auth_middleware(
    state: State<Arc<RwLock<ProxySecurityConfig>>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    auth_middleware_internal(state, request, next, true).await
}

fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn is_health_check(path: &str) -> bool {
    path == "/healthz" || path == "/api/health" || path == "/health"
}

async fn auth_middleware_internal(
    State(security): State<Arc<RwLock<ProxySecurityConfig>>>,
    request: Request,
    next: Next,
    force_strict: bool,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();
    let path = request.uri().path();

    let health = is_health_check(path);
    let log_path = path.to_string();

    if !path.contains("event_logging") && !health {
        tracing::info!("Request: {} {}", method, log_path);
    } else {
        tracing::trace!("Heartbeat/Health: {} {}", method, log_path);
    }

    if method == axum::http::Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let client_ip = extract_client_ip(&request);

    if force_strict {
        if let Some(ip) = client_ip {
            if rate_limiter::is_blocked(ip) {
                tracing::warn!("Blocked IP {} attempted access", ip);
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
        }
    }

    let security = security.read().await.clone();
    let effective_mode = security.effective_auth_mode();

    // force_strict=true: ALWAYS require auth (for admin endpoints)
    // force_strict=false: respect auth_mode setting
    if force_strict {
        // Admin endpoints: only skip auth for health checks
        if health {
            return Ok(next.run(request).await);
        }
        // Otherwise, ALWAYS require auth regardless of auth_mode
    } else {
        // Proxy endpoints: respect auth_mode
        if matches!(effective_mode, ProxyAuthMode::Off) {
            return Ok(next.run(request).await);
        }

        if matches!(effective_mode, ProxyAuthMode::AllExceptHealth) && health {
            return Ok(next.run(request).await);
        }
    }

    let api_key = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").or(Some(s)))
        .or_else(|| request.headers().get("x-api-key").and_then(|h| h.to_str().ok()))
        .or_else(|| request.headers().get("x-goog-api-key").and_then(|h| h.to_str().ok()));

    if security.api_key.is_empty() {
        if force_strict {
            tracing::error!("Admin auth is required but api_key is empty");
            return Err(StatusCode::UNAUTHORIZED);
        }
        tracing::error!("Proxy auth is enabled but api_key is empty; denying request");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let authorized = api_key.is_some_and(|k| constant_time_compare(k, &security.api_key));

    if authorized {
        if let Some(ip) = client_ip {
            rate_limiter::clear_failed_attempts(ip);
        }
        Ok(next.run(request).await)
    } else {
        if force_strict {
            if let Some(ip) = client_ip {
                rate_limiter::record_failed_attempt(ip);
            }
        }
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn extract_client_ip(request: &Request) -> Option<IpAddr> {
    request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            request
                .headers()
                .get("x-real-ip")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.trim().parse().ok())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare("abc", "abc"));
        assert!(!constant_time_compare("abc", "abd"));
        assert!(!constant_time_compare("ab", "abc"));
    }
}
