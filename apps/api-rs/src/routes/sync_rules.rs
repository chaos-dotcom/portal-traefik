
//! `GET /api/sync-rules` — the sync-rule list, mirroring `portal.routes.sync_rules`.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::Row;

use crate::auth::Admin;
use crate::state::AppState;

pub async fn list_rules(State(state): State<AppState>, _: Admin) -> Response {
    let rows = sqlx::query(
        "SELECT id, name, source_config, destination_path, conflict_policy, path_template,
                enabled, frameio_webhook_id, created_at
           FROM sync_rules ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await;
    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("sync_rules.list_failed: {e}");
            return crate::error::detail(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
            );
        }
    };
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<uuid::Uuid, _>("id").to_string(),
                "name": r.get::<String, _>("name"),
                "source": r.get::<Value, _>("source_config"),
                "destination_path": r.get::<String, _>("destination_path"),
                "conflict_policy": r.get::<String, _>("conflict_policy"),
                "path_template": r.get::<Option<String>, _>("path_template"),
                "enabled": r.get::<bool, _>("enabled"),
                "subscription_active": r.get::<Option<String>, _>("frameio_webhook_id").is_some(),
                "created_at": crate::pyfmt::iso(&r.get::<DateTime<Utc>, _>("created_at")),
            })
        })
        .collect();
    Json(Value::Array(out)).into_response()
}
