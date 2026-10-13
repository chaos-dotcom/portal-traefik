
//! `GET /api/download-links` — the download-link list, mirroring `portal.routes.download_links`.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::Row;

use crate::auth::Admin;
use crate::state::AppState;

fn link_state(revoked_at: Option<DateTime<Utc>>, expires_at: Option<DateTime<Utc>>) -> &'static str {
    if revoked_at.is_some() {
        return "revoked";
    }
    if let Some(exp) = expires_at {
        if exp <= Utc::now() {
            return "expired";
        }
    }
    "ok"
}

pub async fn list_links(State(state): State<AppState>, _: Admin) -> Response {
    let rows = sqlx::query(
        "SELECT id, token, source, expires_at, password_hash, max_downloads, downloads_count,
                viewer_fields_required, verify_email, allow_preview, brand_display_name,
                brand_subtitle, revoked_at, created_at
           FROM download_links ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("download_links.list_failed: {e}");
            return crate::error::detail(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
            );
        }
    };

    let base = state.settings.base_url.trim_end_matches('/').to_string();
    let mut out: Vec<Value> = Vec::with_capacity(rows.len());
    for row in rows {
        let token: String = row.get("token");
        let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
        let revoked_at: Option<DateTime<Utc>> = row.get("revoked_at");
        let created_at: DateTime<Utc> = row.get("created_at");
        let fields: Option<Value> = row.get("viewer_fields_required");
        let fields = fields.unwrap_or_else(|| json!({}));
        let verify_email: bool = row.get("verify_email");
        let flag = |k: &str| fields.get(k).and_then(|v| v.as_bool()).unwrap_or(false);

        out.push(json!({
            "id": row.get::<uuid::Uuid, _>("id").to_string(),
            "token": token,
            "public_url": format!("{base}/d/{token}"),
            "state": link_state(revoked_at, expires_at),
            "source": row.get::<Value, _>("source"),
            "expires_at": expires_at.map(|d| crate::pyfmt::iso(&d)),
            "password_protected": row.get::<Option<String>, _>("password_hash").is_some(),
            "max_downloads": row.get::<Option<i32>, _>("max_downloads"),
            "downloads_count": row.get::<i32, _>("downloads_count"),
            "viewer_fields_required": {
                "name": flag("name"),
                "email": flag("email") || verify_email,
            },
            "verify_email": verify_email,
            "allow_preview": row.get::<bool, _>("allow_preview"),
            "brand_display_name": row.get::<Option<String>, _>("brand_display_name"),
            "brand_subtitle": row.get::<Option<String>, _>("brand_subtitle"),
            "revoked_at": revoked_at.map(|d| crate::pyfmt::iso(&d)),
            "created_at": crate::pyfmt::iso(&created_at),
        }));
    }
    Json(Value::Array(out)).into_response()
}
