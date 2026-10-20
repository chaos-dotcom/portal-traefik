
//! Brand profiles — named brands (page name, accent colour, logo) a link can carry instead of the
//! app-wide brand, for work delivered under a client's or production's name.
//!
//! `GET/POST /api/brand-profiles`, `PATCH/DELETE /api/brand-profiles/{id}`,
//! `POST/DELETE /api/brand-profiles/{id}/logo`. The logo itself is public at
//! `/api/branding/logo?profile={id}` (see `routes::branding`).

use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::auth::Admin;
use crate::error::detail;
use crate::routes::security::{valid_hex_color, PNG_MAGIC};
use crate::state::AppState;

const COLUMNS: &str = "SELECT p.id, p.name, p.display_name, p.accent_color, p.logo_png IS NOT NULL AS has_logo,
        p.logo_updated_at, p.created_at,
        (SELECT count(*) FROM links l WHERE l.brand_profile_id = p.id) AS links
   FROM brand_profiles p";

fn render(row: &sqlx::postgres::PgRow) -> Value {
    let id: Uuid = row.get("id");
    let updated: Option<DateTime<Utc>> = row.get("logo_updated_at");
    let has_logo: bool = row.get("has_logo");
    json!({
        "id": id.to_string(),
        "name": row.get::<String, _>("name"),
        "display_name": row.get::<Option<String>, _>("display_name"),
        "accent_color": row.get::<Option<String>, _>("accent_color"),
        "has_logo": has_logo,
        "logo_url": if has_logo { updated.map(|d| format!("/api/branding/logo?profile={id}&v={}", d.timestamp())) } else { None },
        "links": row.get::<i64, _>("links"),
        "created_at": crate::pyfmt::iso(&row.get::<DateTime<Utc>, _>("created_at")),
    })
}

async fn fetch(state: &AppState, id: Uuid) -> Option<Value> {
    sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} WHERE p.id = $1")))
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|r| render(&r))
}

fn not_found() -> Response {
    detail(StatusCode::NOT_FOUND, "Brand profile not found")
}

/// Trimmed text or None; `Err` when a colour isn't a hex colour.
fn clean(body: &Map<String, Value>, key: &str) -> Result<Option<String>, Response> {
    let v = body.get(key).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if key == "accent_color"
        && let Some(c) = &v
        && !valid_hex_color(c)
    {
        return Err(detail(StatusCode::UNPROCESSABLE_ENTITY, "Accent colour must be a hex colour like #f59e0b"));
    }
    if v.as_ref().is_some_and(|s| s.chars().count() > 255) {
        return Err(detail(StatusCode::UNPROCESSABLE_ENTITY, "Keep it under 255 characters"));
    }
    Ok(v)
}

pub async fn list(State(state): State<AppState>, _: Admin) -> Response {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{COLUMNS} ORDER BY lower(p.name)")))
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    Json(Value::Array(rows.iter().map(render).collect())).into_response()
}

pub async fn create(State(state): State<AppState>, admin: Admin, Json(body): Json<Map<String, Value>>) -> Response {
    let name = match clean(&body, "name") {
        Ok(Some(n)) => n,
        Ok(None) => return detail(StatusCode::UNPROCESSABLE_ENTITY, "Give the brand a name"),
        Err(r) => return r,
    };
    let (display, accent) = match (clean(&body, "display_name"), clean(&body, "accent_color")) {
        (Ok(d), Ok(a)) => (d, a),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let id: Uuid = match sqlx::query_scalar(
        "INSERT INTO brand_profiles (name, display_name, accent_color) VALUES ($1,$2,$3) RETURNING id",
    )
    .bind(&name)
    .bind(display)
    .bind(accent)
    .fetch_one(&state.db)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("brands.create_failed: {e}");
            return detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };
    let _ = sqlx::query("INSERT INTO audit_log (user_id, action, detail) VALUES ($1,'brand_profile.created',$2)")
        .bind(admin.id).bind(json!({ "brand_profile_id": id.to_string(), "name": name })).execute(&state.db).await;
    match fetch(&state, id).await {
        Some(v) => (StatusCode::CREATED, Json(v)).into_response(),
        None => not_found(),
    }
}

pub async fn update(State(state): State<AppState>, _: Admin, Path(id): Path<Uuid>, Json(body): Json<Map<String, Value>>) -> Response {
    if fetch(&state, id).await.is_none() {
        return not_found();
    }
    for key in ["name", "display_name", "accent_color"] {
        if !body.contains_key(key) {
            continue;
        }
        let v = match clean(&body, key) {
            Ok(v) => v,
            Err(r) => return r,
        };
        if key == "name" && v.is_none() {
            return detail(StatusCode::UNPROCESSABLE_ENTITY, "Give the brand a name");
        }
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE brand_profiles SET {key} = $1, updated_at = now() WHERE id = $2")))
            .bind(v).bind(id).execute(&state.db).await;
    }
    match fetch(&state, id).await {
        Some(v) => Json(v).into_response(),
        None => not_found(),
    }
}

/// Links using the profile fall back to the app-wide brand (the foreign key sets them to NULL).
pub async fn delete(State(state): State<AppState>, admin: Admin, Path(id): Path<Uuid>) -> Response {
    if fetch(&state, id).await.is_none() {
        return not_found();
    }
    let _ = sqlx::query("DELETE FROM brand_profiles WHERE id = $1").bind(id).execute(&state.db).await;
    let _ = sqlx::query("INSERT INTO audit_log (user_id, action, detail) VALUES ($1,'brand_profile.deleted',$2)")
        .bind(admin.id).bind(json!({ "brand_profile_id": id.to_string() })).execute(&state.db).await;
    StatusCode::NO_CONTENT.into_response()
}

pub async fn upload_logo(State(state): State<AppState>, _: Admin, Path(id): Path<Uuid>, mut multipart: Multipart) -> Response {
    if fetch(&state, id).await.is_none() {
        return not_found();
    }
    let max_bytes = state.settings.branding_logo_max_bytes;
    let too_large = || detail(StatusCode::BAD_REQUEST, &format!("Logo exceeds the maximum size ({} KB)", max_bytes / 1024));
    let mut data: Option<Vec<u8>> = None;
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) if field.name() == Some("file") => {
                match field.bytes().await {
                    Ok(b) => data = Some(b.to_vec()),
                    Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => return too_large(),
                    Err(_) => {}
                }
                break;
            }
            Ok(Some(_)) => continue,
            Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => return too_large(),
            _ => break,
        }
    }
    let Some(data) = data else {
        return detail(StatusCode::UNPROCESSABLE_ENTITY, "Field required");
    };
    if data.len() > max_bytes {
        return too_large();
    }
    if !data.starts_with(&PNG_MAGIC) {
        return detail(StatusCode::BAD_REQUEST, "Logo must be a PNG file");
    }
    let _ = sqlx::query(
        "UPDATE brand_profiles SET logo_png = $1, logo_content_type = 'image/png', logo_updated_at = now(), updated_at = now() WHERE id = $2",
    )
    .bind(&data)
    .bind(id)
    .execute(&state.db)
    .await;
    match fetch(&state, id).await {
        Some(v) => Json(v).into_response(),
        None => not_found(),
    }
}

pub async fn delete_logo(State(state): State<AppState>, _: Admin, Path(id): Path<Uuid>) -> Response {
    if fetch(&state, id).await.is_none() {
        return not_found();
    }
    let _ = sqlx::query(
        "UPDATE brand_profiles SET logo_png = NULL, logo_content_type = NULL, logo_updated_at = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await;
    match fetch(&state, id).await {
        Some(v) => Json(v).into_response(),
        None => not_found(),
    }
}
