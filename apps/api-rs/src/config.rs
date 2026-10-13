
//! Runtime configuration, mirroring `portal.lib.config.Settings` for the fields the HTTP
//! service needs. Values come from the same environment variables the Python service reads.

#[derive(Clone, Debug)]
pub struct Settings {
    pub database_url: String,
    pub redis_url: String,
    pub host: String,
    pub port: u16,
    /// Signing key for the session cookie (`SESSION_SECRET`).
    pub session_secret: String,
    /// `SESSION_COOKIE_SECURE` — adds `secure` to the Set-Cookie flags (default true).
    pub session_cookie_secure: bool,
}

impl Settings {
    pub fn from_env() -> Self {
        // Python's DSN carries a SQLAlchemy driver suffix (`+asyncpg`); sqlx wants the bare
        // scheme. Translate rather than require the operator to run two different values.
        let raw_db = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://portal@127.0.0.1:5432/portal".to_string());
        let database_url = raw_db.replace("postgresql+asyncpg://", "postgresql://");
        let session_cookie_secure = std::env::var("SESSION_COOKIE_SECURE")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(true);
        Self {
            database_url,
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_string()),
            host: std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8000),
            session_secret: std::env::var("SESSION_SECRET")
                .unwrap_or_else(|_| "change-me-session-secret".to_string()),
            session_cookie_secure,
        }
    }
}
