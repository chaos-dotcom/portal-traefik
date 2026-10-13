
//! Public (unauthenticated) upload-link endpoints — `portal.routes.public`.
//!
//! Only the token-resolution 404 path is ported so far; the resolved-link body needs the
//! branding resolver and is tracked in RESULTS.md.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;

use crate::error::detail;
use crate::state::AppState;

pub async fn resolve_link(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let row = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM upload_links WHERE token = $1")
        .bind(&token)
        .fetch_optional(&state.db)
        .await;
    match row {
        Ok(None) => detail(StatusCode::NOT_FOUND, "This upload link does not exist"),
        Ok(Some(_)) => detail(
            StatusCode::NOT_IMPLEMENTED,
            "public link resolution not yet ported",
        ),
        Err(_) => detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
    }
}
