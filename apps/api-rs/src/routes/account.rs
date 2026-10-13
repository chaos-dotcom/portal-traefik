
//! `GET /api/auth/me` and `GET /api/frameio/status` — the small authenticated reads the admin
//! shell makes on every load.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::json;

use crate::auth::Admin;
use crate::state::AppState;

/// `REFRESH_SKEW_SECONDS` from `portal.frameio.connection`.
const REFRESH_SKEW_SECONDS: i64 = 300;

pub async fn me(admin: Admin) -> Response {
    Json(json!({ "id": admin.id.to_string(), "email": admin.email })).into_response()
}

pub async fn frameio_status(State(state): State<AppState>, admin: Admin) -> Response {
    let row = sqlx::query_as::<_, (Option<String>, Option<DateTime<Utc>>)>(
        "SELECT adobe_email, token_expires_at FROM frameio_connections WHERE user_id = $1 LIMIT 1",
    )
    .bind(admin.id)
    .fetch_optional(&state.db)
    .await;

    let body = match row {
        Ok(None) => json!({
            "connected": false, "adobe_email": null, "expires_at": null,
            "needs_refresh": null, "scopes": null,
        }),
        Ok(Some((adobe_email, token_expires_at))) => {
            let needs_refresh = match token_expires_at {
                None => true,
                Some(exp) => (exp - Utc::now()).num_seconds() <= REFRESH_SKEW_SECONDS,
            };
            json!({
                "connected": true,
                "adobe_email": adobe_email,
                "expires_at": token_expires_at.map(|d| crate::pyfmt::iso(&d)),
                "needs_refresh": needs_refresh,
                "scopes": state.settings.frameio_scopes,
            })
        }
        Err(e) => {
            tracing::error!("frameio.status_failed: {e}");
            return crate::error::detail(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
            );
        }
    };
    Json(body).into_response()
}
