use axum::{
    extract::Request,
    extract::State,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tokio::sync::RwLock;

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

async fn auth_middleware_internal(
    State(security): State<Arc<RwLock<ProxySecurityConfig>>>,
    request: Request,
    next: Next,
    force_strict: bool,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let is_health_check =
        path == "/healthz" || path == "/api/health" || path == "/health" || path == "/api/status";

    if !path.contains("event_logging") && !is_health_check {
        tracing::info!("Request: {} {}", method, path);
    } else {
        tracing::trace!("Heartbeat/Health: {} {}", method, path);
    }

    if method == axum::http::Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let security = security.read().await.clone();
    let effective_mode = security.effective_auth_mode();

    if !force_strict {
        if matches!(effective_mode, ProxyAuthMode::Off) {
            return Ok(next.run(request).await);
        }

        if matches!(effective_mode, ProxyAuthMode::AllExceptHealth) && is_health_check {
            return Ok(next.run(request).await);
        }
    } else {
        if matches!(effective_mode, ProxyAuthMode::Off) {
            return Ok(next.run(request).await);
        }

        if is_health_check {
            return Ok(next.run(request).await);
        }
    }

    let api_key = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").or(Some(s)))
        .or_else(|| {
            request
                .headers()
                .get("x-api-key")
                .and_then(|h| h.to_str().ok())
        })
        .or_else(|| {
            request
                .headers()
                .get("x-goog-api-key")
                .and_then(|h| h.to_str().ok())
        });

    if security.api_key.is_empty() {
        if force_strict {
            tracing::error!("Admin auth is required but api_key is empty");
            return Err(StatusCode::UNAUTHORIZED);
        }
        tracing::error!("Proxy auth is enabled but api_key is empty; denying request");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let authorized = api_key.is_some_and(|k| k == security.api_key);

    if authorized {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_auth_placeholder() {}
}
