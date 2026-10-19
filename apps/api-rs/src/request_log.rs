//! Per-request id + access log for the API — port of `portal.lib.logging.RequestIdMiddleware`.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// The container healthcheck probes this every 15s. Logged in full, healthy probes were about a
/// third of all log lines and pushed real activity out of Docker's size-capped log in ~1.5 days.
/// Only an unhealthy answer is worth a line.
pub const HEALTH_PATH: &str = "/api/health";

/// Honour an incoming `X-Request-Id` (or mint one), log the request, and echo the id back.
pub async fn request_id_middleware(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    let method = request.method().clone();
    // Nested under /api, so the router sees the stripped path; log the full one, like Python.
    let path = request
        .extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|u| u.path().to_string())
        .unwrap_or_else(|| format!("/api{}", request.uri().path()));
    let probe = path == HEALTH_PATH;
    if !probe {
        tracing::info!(target: "http", request_id = %request_id, method = %method, path = %path, "request.start");
    }
    let mut response = next.run(request).await;
    if let Ok(v) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", v);
    }
    let status = response.status().as_u16();
    if !probe || status >= 400 {
        tracing::info!(target: "http", request_id = %request_id, status, path = %path, "request.end");
    }
    response
}
