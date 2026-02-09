// CORS middleware
use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

/// Trusted origins for browser-based access to the management UI.
/// Programmatic clients (Cursor, Claude Code, OpenCode) don't use CORS.
const ALLOWED_ORIGINS: &[&str] =
    &["http://localhost:8045", "http://127.0.0.1:8045", "https://antigravity.quantumind.ru"];

/// create CORS layer
pub fn cors_layer() -> CorsLayer {
    let origins: Vec<HeaderValue> = ALLOWED_ORIGINS.iter().filter_map(|o| o.parse().ok()).collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::HEAD,
            Method::OPTIONS,
            Method::PATCH,
        ])
        .allow_headers(Any)
        .allow_credentials(false)
        .max_age(std::time::Duration::from_secs(3600))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_layer_creation() {
        let _layer = cors_layer();
        // Layer creation succeeded - type system ensures correctness
    }
}
