
//! `GET /api/auth/me` and `GET /api/frameio/status` — the small authenticated reads the admin
//! shell makes on every load.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::json;

use crate::auth::{Admin, SessionExt};
use crate::state::AppState;
use sqlx::Row;

/// `REFRESH_SKEW_SECONDS` from `portal.frameio.connection`.
const REFRESH_SKEW_SECONDS: i64 = 300;

pub async fn me(admin: Admin) -> Response {
    Json(json!({ "id": admin.id.to_string(), "email": admin.email })).into_response()
}

pub async fn frameio_status(State(state): State<AppState>, admin: Admin) -> Response {
    type Row = (Option<String>, Option<DateTime<Utc>>, Option<String>, Option<String>);
    let row = sqlx::query_as::<_, Row>(
        "SELECT adobe_email, token_expires_at, access_token_encrypted, refresh_token_encrypted
           FROM frameio_connections WHERE user_id = $1 LIMIT 1",
    )
    .bind(admin.id)
    .fetch_optional(&state.db)
    .await;

    // A connection whose tokens this instance can't decrypt (TOKEN_ENCRYPTION_KEY changed, or
    // written under a different key derivation) is as good as none: every Frame.io call would
    // fail. Report it as not connected — deliberately unlike Python, which only checks the row
    // exists — so Settings offers "Connect" instead of claiming a connection that doesn't work.
    let cipher = crate::crypto::TokenCipher::new(&state.settings.token_encryption_key);
    let usable = |t: &Option<String>| t.as_deref().is_some_and(|v| cipher.decrypt(v).is_ok());
    let row = row.map(|r| {
        r.filter(|(_, _, access, refresh)| {
            let ok = usable(access) || usable(refresh);
            if !ok {
                tracing::warn!("frameio.status.undecryptable_tokens user_id={}", admin.id);
            }
            ok
        })
    });
    let body = match row {
        Ok(None) => json!({
            "connected": false, "adobe_email": null, "expires_at": null,
            "needs_refresh": null, "scopes": null,
        }),
        Ok(Some((adobe_email, token_expires_at, _, _))) => {
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

/// `GET /api/settings/security` — TOTP flag + the account's registered passkeys.
pub async fn security_status(State(state): State<AppState>, admin: Admin) -> Response {
    let totp_enabled: Option<bool> =
        sqlx::query_scalar("SELECT totp_enabled FROM users WHERE id = $1")
            .bind(admin.id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let rows = sqlx::query(
        "SELECT id, name, created_at, last_used_at FROM webauthn_credentials
          WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(admin.id)
    .fetch_all(&state.db)
    .await;
    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("security.status_failed: {e}");
            return crate::error::detail(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
            );
        }
    };
    let passkeys: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<uuid::Uuid, _>("id").to_string(),
                "name": r.get::<Option<String>, _>("name"),
                "created_at": crate::pyfmt::iso(&r.get::<DateTime<Utc>, _>("created_at")),
                "last_used_at": r.get::<Option<DateTime<Utc>>, _>("last_used_at").map(|d| crate::pyfmt::iso(&d)),
            })
        })
        .collect();
    Json(json!({ "totp_enabled": totp_enabled.unwrap_or(false), "passkeys": passkeys }))
        .into_response()
}

/// `POST /api/auth/logout` — clear the session (`session.clear()`), then `{"ok": true}`.
pub async fn logout(SessionExt(session): SessionExt) -> Response {
    {
        let mut s = session.lock().unwrap();
        s.accessed = true;
        s.data.clear();
        s.modified = true;
    }
    Json(json!({ "ok": true })).into_response()
}
