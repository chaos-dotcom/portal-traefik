
mod auth;
mod config;
mod crypto;
mod error;
mod routes;
mod services;
mod seed;
mod frameio;
mod session;
mod spa;
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

    // `portal-api seed` — one-shot admin seed, run once by the container entrypoint.
    if std::env::args().nth(1).as_deref() == Some("seed") {
        seed::seed_admin(&db).await?;
        return Ok(());
    }
    let redis = redis::aio::ConnectionManager::new(
        redis::Client::open(settings.redis_url.clone())?,
    )
    .await?;

    let addr = format!("{}:{}", settings.host, settings.port);
    let settings = Arc::new(settings);
    let frameio = Arc::new(frameio::FrameioClient::new(db.clone(), settings.clone()));
    let state = AppState {
        settings,
        db,
        redis,
        frameio,
    };

    // The API and the static SPA share one origin, so the browser's relative `fetch("/api/…")`
    // calls carry the session cookie with no proxy in between.
    let api = Router::<AppState>::new()
        .route("/health", get(routes::health::health))
        .route("/auth/login", post(auth::login))
        .route("/auth/me", get(routes::account::me))
        .route("/download-links", get(routes::download_links::list_links).post(routes::download_links::create_link))
        .route("/download-links/{link_id}", get(routes::download_links::get_link).patch(routes::download_links::update_link).delete(routes::download_links::delete_link))
        .route("/download-links/{link_id}/revoke", post(routes::download_links::revoke_link))
        .route("/download-links/{link_id}/events", get(routes::download_links::list_events))
        .route("/download-links/{link_id}/stats", get(routes::download_links::stats))
        .route("/public/links/{token}", get(routes::public::resolve_link))
        .route(
            "/dashboard/transfers",
            get(routes::dashboard::transfers),
        )
        .route("/upload-links/extension-preset", get(routes::upload_links::extension_preset))
        .route("/upload-links", get(routes::upload_links::list_links).post(routes::upload_links::create_link))
        .route("/upload-links/{link_id}", get(routes::upload_links::get_link).patch(routes::upload_links::update_link).delete(routes::upload_links::delete_link))
        .route("/upload-links/{link_id}/revoke", post(routes::upload_links::revoke_link))
        .route("/upload-links/{link_id}/stats", get(routes::upload_links::stats))
        .route("/branding/logo", get(routes::branding::get_logo))
        .route("/privacy/subject", get(routes::guarded::privacy_subject))
        .route("/dashboard/history", get(routes::dashboard::history))
        .route("/destinations", get(routes::destinations::list_destinations).post(routes::destinations::create_destination))
        .route(
            "/destinations/{id}",
            get(routes::destinations::get_destination)
                .patch(routes::destinations::update_destination)
                .delete(routes::destinations::delete_destination),
        )
        .route("/sync-rules", get(routes::sync_rules::list_rules))
        .route("/activity", get(routes::activity::list_activity))
        .route("/activity/actions", get(routes::activity::list_actions))
        .route("/activity/summary", get(routes::activity::summary))
        .route("/settings/security", get(routes::account::security_status))
        .route("/settings/email", get(routes::settings::email_status))
        .route("/settings/frameio-config", get(routes::settings::frameio_config_status))
        .route("/frameio/status", get(routes::account::frameio_status))
        .route("/frameio/oauth/connect", get(routes::frameio::connect))
        .route("/frameio/oauth/callback", get(routes::frameio::callback))
        .route("/frameio/disconnect", post(routes::frameio::disconnect))
        .route("/frameio/accounts", get(routes::frameio::list_accounts))
        .route("/frameio/workspaces", get(routes::frameio::list_workspaces))
        .route("/frameio/projects", get(routes::frameio::list_projects).post(routes::frameio::create_project))
        .route("/frameio/projects/{project_id}", axum::routing::delete(routes::frameio::delete_project))
        .route("/frameio/folders", get(routes::frameio::list_folders).post(routes::frameio::create_folder))
        .route("/frameio/folders/{folder_id}", axum::routing::delete(routes::frameio::delete_folder))
        .route("/frameio/folders/{folder_id}/move", axum::routing::patch(routes::frameio::move_folder))
        .route("/frameio/folders/{folder_id}/copy", post(routes::frameio::copy_folder))
        .route("/frameio/files", get(routes::frameio::list_files))
        .route("/frameio/files/{file_id}", axum::routing::delete(routes::frameio::delete_file))
        .route("/frameio/files/{file_id}/download-url", get(routes::frameio::file_download_url))
        .route("/frameio/files/{file_id}/move", axum::routing::patch(routes::frameio::move_file))
        .route("/frameio/files/{file_id}/copy", post(routes::frameio::copy_file))
        .route("/frameio/shares", post(routes::frameio::create_share))
        .fallback(error::not_found);

    // SvelteKit's static build. Unknown non-API paths serve index.html for client-side routing.
    // NOTE: `not_found_service` would force a 404 status on the shell; `fallback` serves it 200.
    let web_dir = std::env::var("WEB_DIR").unwrap_or_else(|_| "web/build".to_string());
    let static_service =
        ServeDir::new(&web_dir).fallback(ServeFile::new(format!("{web_dir}/index.html")));

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
