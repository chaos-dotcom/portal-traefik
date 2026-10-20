
//! Link presets — combinations of link options the admin saves from the composer and applies to
//! new links. `GET/POST /api/link-presets`, `PATCH/DELETE /api/link-presets/{id}`.
//!
//! `settings` is the composer's own shape (expiry, protection, file types, …); the server only
//! checks it's a small JSON object, since the composer is what reads it back.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::auth::Admin;
use crate::error::detail;
use crate::state::AppState;

const MAX_SETTINGS_BYTES: usize = 8 * 1024;

fn render(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("id").to_string(),
        "name": row.get::<String, _>("name"),
        "kind": row.get::<String, _>("kind"),
        "settings": row.get::<Value, _>("settings"),
        "created_at": crate::pyfmt::iso(&row.get::<DateTime<Utc>, _>("created_at")),
    })
}

async fn fetch(state: &AppState, id: Uuid) -> Option<Value> {
    sqlx::query("SELECT id, name, kind, settings, created_at FROM link_presets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|r| render(&r))
}

fn check_name(v: Option<&Value>) -> Result<String, Response> {
    let name = v.and_then(|v| v.as_str()).map(|s| s.trim().to_string()).unwrap_or_default();
    if name.is_empty() {
        return Err(detail(StatusCode::UNPROCESSABLE_ENTITY, "Give the preset a name"));
    }
    if name.chars().count() > 255 {
        return Err(detail(StatusCode::UNPROCESSABLE_ENTITY, "Keep the name under 255 characters"));
    }
    Ok(name)
}

fn check_settings(v: Option<&Value>) -> Result<Value, Response> {
    let settings = v.cloned().unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        return Err(detail(StatusCode::UNPROCESSABLE_ENTITY, "settings must be an object"));
    }
    if settings.to_string().len() > MAX_SETTINGS_BYTES {
        return Err(detail(StatusCode::UNPROCESSABLE_ENTITY, "That preset is too large"));
    }
    Ok(settings)
}

pub async fn list(State(state): State<AppState>, _: Admin) -> Response {
    let rows = sqlx::query("SELECT id, name, kind, settings, created_at FROM link_presets ORDER BY kind, lower(name)")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    Json(Value::Array(rows.iter().map(render).collect())).into_response()
}

pub async fn create(State(state): State<AppState>, _: Admin, Json(body): Json<Map<String, Value>>) -> Response {
    let name = match check_name(body.get("name")) {
        Ok(n) => n,
        Err(r) => return r,
    };
    let kind = body.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if kind != "upload" && kind != "download" {
        return detail(StatusCode::UNPROCESSABLE_ENTITY, "kind must be upload or download");
    }
    let settings = match check_settings(body.get("settings")) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let id: Uuid = match sqlx::query_scalar("INSERT INTO link_presets (name, kind, settings) VALUES ($1,$2,$3) RETURNING id")
        .bind(&name)
        .bind(kind)
        .bind(&settings)
        .fetch_one(&state.db)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("presets.create_failed: {e}");
            return detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };
    match fetch(&state, id).await {
        Some(v) => (StatusCode::CREATED, Json(v)).into_response(),
        None => detail(StatusCode::NOT_FOUND, "Preset not found"),
    }
}

pub async fn update(State(state): State<AppState>, _: Admin, Path(id): Path<Uuid>, Json(body): Json<Map<String, Value>>) -> Response {
    if fetch(&state, id).await.is_none() {
        return detail(StatusCode::NOT_FOUND, "Preset not found");
    }
    if body.contains_key("name") {
        let name = match check_name(body.get("name")) {
            Ok(n) => n,
            Err(r) => return r,
        };
        let _ = sqlx::query("UPDATE link_presets SET name = $1, updated_at = now() WHERE id = $2").bind(name).bind(id).execute(&state.db).await;
    }
    if body.contains_key("settings") {
        let settings = match check_settings(body.get("settings")) {
            Ok(s) => s,
            Err(r) => return r,
        };
        let _ = sqlx::query("UPDATE link_presets SET settings = $1, updated_at = now() WHERE id = $2").bind(settings).bind(id).execute(&state.db).await;
    }
    match fetch(&state, id).await {
        Some(v) => Json(v).into_response(),
        None => detail(StatusCode::NOT_FOUND, "Preset not found"),
    }
}

pub async fn delete(State(state): State<AppState>, _: Admin, Path(id): Path<Uuid>) -> Response {
    let res = sqlx::query("DELETE FROM link_presets WHERE id = $1").bind(id).execute(&state.db).await;
    match res.map(|r| r.rows_affected()) {
        Ok(0) => detail(StatusCode::NOT_FOUND, "Preset not found"),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("presets.delete_failed: {e}");
            detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        }
    }
}
