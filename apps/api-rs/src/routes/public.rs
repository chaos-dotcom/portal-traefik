
//! Public (unauthenticated) upload-link endpoints — `portal.routes.public`.
//!
//! Token resolution is ported (the uploader page's first call). The uploader session lifecycle
//! (start/file/complete/heartbeat/abandon) is not yet.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::Row;

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

/// Python truthiness for the `a or b` chains: empty string and null both fall through.
fn or_else(a: Option<String>, b: Option<String>) -> Option<String> {
    match a {
        Some(s) if !s.is_empty() => Some(s),
        _ => match b {
            Some(s) if !s.is_empty() => Some(s),
            _ => None,
        },
    }
}

pub async fn resolve_link(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let row = sqlx::query(
        "SELECT l.brand_display_name, l.brand_subtitle, l.expires_at, l.revoked_at,
                l.password_hash, l.max_file_size, l.allowed_extensions,
                l.uploader_fields_required, l.verify_email,
                d.display_name AS destination_name, d.subtitle AS destination_subtitle
           FROM upload_links l JOIN destinations d ON d.id = l.destination_id
          WHERE l.token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await;

    let row = match row {
        Ok(None) => return crate::error::detail(StatusCode::NOT_FOUND, "This upload link does not exist"),
        Ok(Some(r)) => r,
        Err(e) => {
            tracing::error!("public.resolve_link failed: {e}");
            return crate::error::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };

    let app = sqlx::query(
        "SELECT logo_png, logo_updated_at, brand_display_name, brand_accent_color FROM app_settings LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let (app_logo, app_name, app_accent) = match app {
        Some(r) => {
            let has_logo: Option<Vec<u8>> = r.get("logo_png");
            let updated: Option<DateTime<Utc>> = r.get("logo_updated_at");
            let logo = if has_logo.is_some() {
                updated.map(|d| {
                    format!("{}/api/branding/logo?v={}", state.settings.base_url.trim_end_matches('/'), d.timestamp())
                })
            } else {
                None
            };
            (
                logo,
                r.get::<Option<String>, _>("brand_display_name"),
                r.get::<Option<String>, _>("brand_accent_color"),
            )
        }
        None => (None, None, None),
    };

    let branding_name = or_else(
        row.get::<Option<String>, _>("brand_display_name"),
        row.get::<Option<String>, _>("destination_name"),
    );
    let branding_subtitle = or_else(
        row.get::<Option<String>, _>("brand_subtitle"),
        row.get::<Option<String>, _>("destination_subtitle"),
    );
    let display_name = or_else(branding_name, app_name).unwrap_or_else(|| "Upload".to_string());

    let fields: Option<Value> = row.get("uploader_fields_required");
    let fields = fields.unwrap_or_else(|| json!({}));
    let flag = |k: &str| fields.get(k).and_then(|v| v.as_bool()).unwrap_or(false);

    let body = json!({
        "state": link_state(row.get("revoked_at"), row.get("expires_at")),
        "display_name": display_name,
        "subtitle": branding_subtitle,
        "logo_url": app_logo,
        "accent_color": app_accent,
        "password_required": row.get::<Option<String>, _>("password_hash").is_some(),
        "max_file_size": row.get::<Option<i64>, _>("max_file_size"),
        "allowed_extensions": row.get::<Option<Value>, _>("allowed_extensions").unwrap_or(Value::Null),
        "uploader_fields_required": {
            "name": flag("name"),
            "email": flag("email"),
            "message": flag("message"),
        },
        "verify_email": row.get::<bool, _>("verify_email"),
    });
    Json(body).into_response()
}
