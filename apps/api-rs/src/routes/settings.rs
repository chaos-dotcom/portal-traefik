
//! `GET /api/settings/email` and `GET /api/settings/frameio-config` — the read halves of the
//! Settings page (`portal.routes.settings` + `portal.services.runtime_config`).

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use sqlx::Row;

use crate::auth::Admin;
use crate::state::AppState;

pub async fn email_status(State(state): State<AppState>, _: Admin) -> Response {
    let row = sqlx::query(
        "SELECT smtp_host, smtp_port, smtp_username, smtp_password_encrypted, smtp_from,
                smtp_use_tls, smtp_starttls, notify_email
           FROM app_settings LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await;
    let row = match row {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("settings.email_status failed: {e}");
            return crate::error::detail(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };
    // Defaults mirror EmailConfig: port 587, starttls on, tls off when unset.
    let (host, port, user, from, use_tls, starttls, notify, has_password) = match &row {
        Some(r) => (
            r.get::<Option<String>, _>("smtp_host").unwrap_or_default(),
            r.get::<Option<i32>, _>("smtp_port").unwrap_or(587),
            r.get::<Option<String>, _>("smtp_username").unwrap_or_default(),
            r.get::<Option<String>, _>("smtp_from").unwrap_or_default(),
            r.get::<Option<bool>, _>("smtp_use_tls").unwrap_or(false),
            r.get::<Option<bool>, _>("smtp_starttls").unwrap_or(true),
            r.get::<Option<String>, _>("notify_email").unwrap_or_default(),
            r.get::<Option<String>, _>("smtp_password_encrypted").is_some(),
        ),
        None => (String::new(), 587, String::new(), String::new(), false, true, String::new(), false),
    };
    let configured = !host.is_empty() && !from.is_empty();
    let notify = if notify.is_empty() { state.settings.admin_email.clone() } else { notify };
    Json(json!({
        "configured": configured,
        "smtp_host": if host.is_empty() { serde_json::Value::Null } else { json!(host) },
        "smtp_port": port,
        "smtp_username": if user.is_empty() { serde_json::Value::Null } else { json!(user) },
        "smtp_from": if from.is_empty() { serde_json::Value::Null } else { json!(from) },
        "smtp_use_tls": use_tls,
        "smtp_starttls": starttls,
        "notify_email": notify,
        "has_password": has_password,
    }))
    .into_response()
}

pub async fn frameio_config_status(State(state): State<AppState>, _: Admin) -> Response {
    let row = sqlx::query(
        "SELECT frameio_client_id, frameio_client_secret_encrypted FROM app_settings LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await;
    let (client_id, has_secret) = match &row {
        Ok(Some(r)) => (
            r.get::<Option<String>, _>("frameio_client_id").unwrap_or_default(),
            r.get::<Option<String>, _>("frameio_client_secret_encrypted").is_some(),
        ),
        _ => (String::new(), false),
    };
    let configured = !client_id.is_empty() && has_secret;
    Json(json!({
        "configured": configured,
        "client_id": if client_id.is_empty() { serde_json::Value::Null } else { json!(client_id) },
        "has_secret": has_secret,
        "redirect_uri": state.settings.resolved_frameio_redirect_uri(),
    }))
    .into_response()
}
