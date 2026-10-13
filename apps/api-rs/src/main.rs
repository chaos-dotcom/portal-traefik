
mod auth;
mod config;
mod error;
mod routes;
mod session;
mod pyfmt;
mod state;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use config::Settings;
use state::AppState;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let settings = Settings::from_env();
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&settings.database_url)
        .await?;
    let redis = redis::aio::ConnectionManager::new(
        redis::Client::open(settings.redis_url.clone())?,
    )
    .await?;

    let addr = format!("{}:{}", settings.host, settings.port);
    let state = AppState {
        settings: Arc::new(settings),
        db,
        redis,
    };

    // The API and the static SPA share one origin, so the browser's relative `fetch("/api/…")`
    // calls carry the session cookie with no proxy in between.
    let api = Router::<AppState>::new()
        .route("/health", get(routes::health::health))
        .route("/auth/login", post(auth::login))
        .route("/auth/me", get(routes::account::me))
        .route("/download-links", get(routes::download_links::list_links))
        .route("/public/links/{token}", get(routes::public::resolve_link))
        .route(
            "/dashboard/transfers",
            get(routes::dashboard::transfers),
        )
        .route("/upload-links", get(routes::upload_links::list_links))
        .route("/upload-links/extension-preset", get(routes::upload_links::extension_preset))
        .route("/branding/logo", get(routes::branding::get_logo))
        .route("/privacy/subject", get(routes::guarded::privacy_subject))
        .route("/dashboard/history", get(routes::dashboard::history))
        .route("/sync-rules", get(routes::sync_rules::list_rules))
        .route("/settings/security", get(routes::account::security_status))
        .route("/frameio/status", get(routes::account::frameio_status))
        .fallback(error::not_found);

    // SvelteKit's static build; unknown non-API paths fall back to index.html for client routing.
    let web_dir = std::env::var("WEB_DIR").unwrap_or_else(|_| "web/build".to_string());
    let static_service = ServeDir::new(&web_dir)
        .not_found_service(ServeFile::new(format!("{web_dir}/index.html")));

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            session::session_middleware,
        ))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr} (web dir: {web_dir})");
    axum::serve(listener, app).await?;
    Ok(())
}
