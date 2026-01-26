use axum::{extract::Request, middleware::Next, response::Response};

pub async fn service_status_middleware(request: Request, next: Next) -> Response {
    next.run(request).await
}
