
//! First-boot seeding — the port of `portal.seed`. Idempotent. Run from the entrypoint
//! (`portal-api seed`) before the API starts, so the supervised workers never race an empty
//! users table.

use sqlx::{PgPool, Row};

use crate::config::Settings;
use crate::services::runtime_config::encrypt_secret;

/// Create the first admin from `ADMIN_EMAIL`/`ADMIN_PASSWORD`. Does nothing if any user exists.
pub async fn seed_admin(pool: &PgPool, settings: &Settings) -> anyhow::Result<()> {
    let email = &settings.admin_email;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        tracing::info!("seed: {count} user(s) already present, skipping");
        return Ok(());
    }
    let hash = bcrypt::hash(&settings.admin_password, bcrypt::DEFAULT_COST)?;
    // is_active has no DB default (SQLAlchemy sets it at the model level); set it here.
    sqlx::query("INSERT INTO users (email, password_hash, is_active) VALUES ($1, $2, true)")
        .bind(email)
        .bind(&hash)
        .execute(pool)
        .await?;
    tracing::info!("seed: created admin {email}");
    Ok(())
}

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

fn env_flag(key: &str, default: bool) -> bool {
    match std::env::var(key).map(|v| v.trim().to_ascii_lowercase()) {
        Ok(v) if ["1", "true", "t", "yes", "y", "on"].contains(&v.as_str()) => true,
        Ok(v) if ["0", "false", "f", "no", "n", "off"].contains(&v.as_str()) => false,
        _ => default,
    }
}

/// One-time import of legacy `.env` email/Frame.io values into the app_settings row — port of
/// `bootstrap_config`. Runs once (guarded by `config_seeded`), so an instance that used to carry
/// SMTP/Frame.io credentials in `.env` keeps them when config moved to the database.
pub async fn seed_config(pool: &PgPool, settings: &Settings) -> anyhow::Result<()> {
    let row = match sqlx::query("SELECT id, config_seeded, smtp_host, frameio_client_id FROM app_settings LIMIT 1")
        .fetch_optional(pool)
        .await?
    {
        Some(r) => r,
        None => {
            sqlx::query("INSERT INTO app_settings (config_seeded) VALUES (false) RETURNING id, config_seeded, smtp_host, frameio_client_id")
                .fetch_one(pool)
                .await?
        }
    };
    if row.get::<bool, _>("config_seeded") {
        return Ok(());
    }
    let id: uuid::Uuid = row.get("id");
    let none_if_empty = |v: String| if v.is_empty() { None } else { Some(v) };

    let smtp_host = env("SMTP_HOST");
    if !smtp_host.is_empty() && row.get::<Option<String>, _>("smtp_host").is_none() {
        let port: i32 = env("SMTP_PORT").trim().parse().unwrap_or(587);
        let password = env("SMTP_PASSWORD");
        let password_enc = (!password.is_empty()).then(|| encrypt_secret(settings, &password));
        sqlx::query(
            "UPDATE app_settings SET smtp_host=$1, smtp_port=$2, smtp_username=$3,
                    smtp_password_encrypted=COALESCE($4, smtp_password_encrypted), smtp_from=$5,
                    smtp_use_tls=$6, smtp_starttls=$7, notify_email=$8, updated_at=now()
              WHERE id=$9",
        )
        .bind(&smtp_host)
        .bind(port)
        .bind(none_if_empty(env("SMTP_USERNAME")))
        .bind(password_enc)
        .bind(none_if_empty(env("SMTP_FROM")))
        .bind(env_flag("SMTP_USE_TLS", false))
        .bind(env_flag("SMTP_STARTTLS", true))
        .bind(none_if_empty(env("NOTIFY_EMAIL")))
        .bind(id)
        .execute(pool)
        .await?;
    }

    let client_id = env("FRAMEIO_CLIENT_ID");
    if !client_id.is_empty() && row.get::<Option<String>, _>("frameio_client_id").is_none() {
        let secret = env("FRAMEIO_CLIENT_SECRET");
        let secret_enc = (!secret.is_empty()).then(|| encrypt_secret(settings, &secret));
        sqlx::query(
            "UPDATE app_settings SET frameio_client_id=$1,
                    frameio_client_secret_encrypted=COALESCE($2, frameio_client_secret_encrypted),
                    updated_at=now()
              WHERE id=$3",
        )
        .bind(&client_id)
        .bind(secret_enc)
        .bind(id)
        .execute(pool)
        .await?;
    }

    sqlx::query("UPDATE app_settings SET config_seeded=true, updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await?;
    tracing::info!("runtime_config.bootstrapped");
    Ok(())
}
