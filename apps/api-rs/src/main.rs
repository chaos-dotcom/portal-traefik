
mod config;
mod error;
mod routes;
mod state;

use std::sync::Arc;

use axum::{routing::get, Router};
use config::Settings;
use state::AppState;
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

    let app = Router::new()
        .route("/api/health", get(routes::health::health))
        .fallback(error::not_found)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
