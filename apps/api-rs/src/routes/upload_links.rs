
//! Admin upload-link list — `portal.routes.upload_links` (`GET /api/upload-links`).
//!
//! Only the list endpoint is ported here; create/update/delete land with the rest of the admin
//! CRUD. Field order and datetime formatting are part of the byte contract (see `pyfmt::iso`).

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::Row;

use crate::auth::Admin;

/// `MEDIA_EXTENSIONS_PRESET` from `portal.uploads.links`.
pub const MEDIA_EXTENSIONS_PRESET: [&str; 26] = [
    "r3d", "braw", "ari", "arx", "mxf", "mov", "mp4", "mts", "m4v", "avi", "prores", "dng",
    "cdng", "exr", "dpx", "tiff", "tif", "png", "jpg", "jpeg", "wav", "aif", "aiff", "mp3",
    "bwf", "zip",
];
use crate::error::detail;
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

pub async fn extension_preset(_: Admin) -> Response {
    Json(serde_json::json!(MEDIA_EXTENSIONS_PRESET)).into_response()
}

pub async fn list_links(State(state): State<AppState>, _: Admin) -> Response {
    let rows = sqlx::query(
        "SELECT l.id, l.token, l.expires_at, l.password_hash, l.max_file_size, l.verify_email,
                l.allowed_extensions, l.uploader_fields_required, l.revoked_at,
                l.brand_display_name, l.brand_subtitle, l.created_at, l.target_folder_id,
                l.target_folder_name, l.subfolder_template, l.destination_id,
                d.display_name AS destination_name, d.config AS destination_config
           FROM upload_links l
           JOIN destinations d ON d.id = l.destination_id
          ORDER BY l.created_at DESC",
    )
    .fetch_all(&state.db)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("upload_links.list_failed: {e}");
            return detail(
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
        let verify_email: bool = row.get("verify_email");
        let fields: Option<Value> = row.get("uploader_fields_required");
        let fields = fields.unwrap_or_else(|| json!({}));
        let flag = |k: &str| fields.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        let email = flag("email") || verify_email;
        let dest_config: Value = row.get("destination_config");
        let created_at: DateTime<Utc> = row.get("created_at");

        out.push(json!({
            "id": row.get::<uuid::Uuid, _>("id").to_string(),
            "token": token,
            "public_url": format!("{base}/u/{token}"),
            "state": link_state(revoked_at, expires_at),
            "destination_id": row.get::<uuid::Uuid, _>("destination_id").to_string(),
            "destination_name": row.get::<String, _>("destination_name"),
            "destination_path": dest_config.get("path").cloned().unwrap_or(Value::Null),
            "expires_at": expires_at.map(|d| crate::pyfmt::iso(&d)),
            "password_protected": row.get::<Option<String>, _>("password_hash").is_some(),
            "max_file_size": row.get::<Option<i64>, _>("max_file_size"),
            "allowed_extensions": row.get::<Option<Value>, _>("allowed_extensions").unwrap_or(Value::Null),
            "uploader_fields_required": {
                "name": flag("name"),
                "email": email,
                "message": flag("message"),
            },
            "verify_email": verify_email,
            "target_folder_id": row.get::<Option<String>, _>("target_folder_id"),
            "target_folder_name": row.get::<Option<String>, _>("target_folder_name"),
            "subfolder_template": row.get::<Option<String>, _>("subfolder_template"),
            "brand_display_name": row.get::<Option<String>, _>("brand_display_name"),
            "brand_subtitle": row.get::<Option<String>, _>("brand_subtitle"),
            "revoked_at": revoked_at.map(|d| crate::pyfmt::iso(&d)),
            "created_at": crate::pyfmt::iso(&created_at),
        }));
    }

    Json(Value::Array(out)).into_response()
}
