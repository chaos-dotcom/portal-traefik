
//! Admin authentication: the `require_admin` extractor and the password login, mirroring
//! `portal.auth.session` and `portal.routes.auth.login`.

use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::detail;
use crate::session::SessionHandle;
use crate::state::AppState;

/// Fixed bcrypt hash used when the email doesn't match an account, so login pays one verify
/// either way (constant-time account enumeration guard, mirroring `verify_dummy`).
const DUMMY_HASH: &str = "$2b$12$EJusDFY5AD5/1C0Xe5Fk9.IQPOlhkjmNkDD.uHSVZSg1k77I.mWnq";

pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

fn session_handle(parts: &Parts) -> SessionHandle {
    parts
        .extensions
        .get::<SessionHandle>()
        .cloned()
        .expect("session middleware must run before auth extractors")
}

pub fn session_user_id(parts: &Parts) -> Option<String> {
    let handle = session_handle(parts);
    let mut s = handle.lock().unwrap();
    s.get("user_id")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

#[derive(Clone)]
pub struct SessionExt(pub SessionHandle);

impl<S> axum::extract::FromRequestParts<S> for SessionExt
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(SessionExt(session_handle(parts)))
    }
}

#[allow(dead_code)] // the uuid is used once authenticated bodies land
pub struct Admin(pub Uuid);

impl FromRequestParts<AppState> for Admin {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(uid) = session_user_id(parts) else {
            return Err(detail(StatusCode::UNAUTHORIZED, "Authentication required"));
        };
        let Ok(id) = Uuid::parse_str(&uid) else {
            return Err(detail(StatusCode::UNAUTHORIZED, "Authentication required"));
        };
        let row = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1 AND is_active")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| detail(StatusCode::UNAUTHORIZED, "Authentication required"))?;
        match row {
            Some(id) => Ok(Admin(id)),
            None => Err(detail(StatusCode::UNAUTHORIZED, "Authentication required")),
        }
    }
}

pub fn invalid_credentials() -> Response {
    detail(StatusCode::UNAUTHORIZED, "Invalid email or password")
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    SessionExt(session): SessionExt,
    axum::extract::Json(body): axum::extract::Json<LoginRequest>,
) -> Response {
    let row = sqlx::query_as::<_, (Uuid, String, bool, bool)>(
        "SELECT id, password_hash, is_active, totp_enabled FROM users WHERE email = $1",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await;

    let Ok(Some((id, password_hash, is_active, totp_enabled))) = row else {
        verify_password(&body.password, DUMMY_HASH);
        return invalid_credentials();
    };
    if !is_active {
        verify_password(&body.password, DUMMY_HASH);
        return invalid_credentials();
    }
    if !verify_password(&body.password, &password_hash) {
        return invalid_credentials();
    }
    if totp_enabled {
        let mut s = session.lock().unwrap();
        s.set("totp_pending_uid", json!(id.to_string()));
        s.set(
            "totp_pending_ts",
            json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)),
        );
        s.set("totp_pending_attempts", json!(0));
        return (
            StatusCode::OK,
            axum::Json(json!({ "totp_required": true, "id": null, "email": null })),
        )
            .into_response();
    }
    {
        let mut s = session.lock().unwrap();
        s.set("user_id", json!(id.to_string()));
    }
    (
        StatusCode::OK,
        axum::Json(json!({ "totp_required": false, "id": id.to_string(), "email": body.email })),
    )
        .into_response()
}
