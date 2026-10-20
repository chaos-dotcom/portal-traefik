
//! DB-backed runtime configuration — port of `portal.services.runtime_config` (email half).

use sqlx::{PgPool, Row};

use crate::config::Settings;
use crate::crypto::TokenCipher;

#[derive(Clone, Debug)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_from: String,
    pub smtp_use_tls: bool,
    pub smtp_starttls: bool,
    pub notify_email: String,
    pub admin_email: String,
}

impl EmailConfig {
    pub fn configured(&self) -> bool {
        !self.smtp_host.is_empty() && !self.smtp_from.is_empty()
    }
    pub fn resolved_notify_email(&self) -> String {
        if self.notify_email.is_empty() {
            self.admin_email.clone()
        } else {
            self.notify_email.clone()
        }
    }
}

fn decrypt(cipher: &TokenCipher, value: Option<&str>) -> String {
    match value {
        None => String::new(),
        Some(v) => cipher.decrypt(v).unwrap_or_default(),
    }
}

pub async fn get_email_config(db: &PgPool, settings: &Settings) -> EmailConfig {
    let row = sqlx::query(
        "SELECT smtp_host, smtp_port, smtp_username, smtp_password_encrypted, smtp_from,
                smtp_use_tls, smtp_starttls, notify_email FROM app_settings LIMIT 1",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let cipher = TokenCipher::new(&settings.token_encryption_key);
    match row {
        Some(r) => EmailConfig {
            smtp_host: r.get::<Option<String>, _>("smtp_host").unwrap_or_default(),
            smtp_port: r.get::<Option<i32>, _>("smtp_port").unwrap_or(587) as u16,
            smtp_username: r.get::<Option<String>, _>("smtp_username").unwrap_or_default(),
            smtp_password: decrypt(&cipher, r.get::<Option<String>, _>("smtp_password_encrypted").as_deref()),
            smtp_from: r.get::<Option<String>, _>("smtp_from").unwrap_or_default(),
            smtp_use_tls: r.get::<Option<bool>, _>("smtp_use_tls").unwrap_or(false),
            smtp_starttls: r.get::<Option<bool>, _>("smtp_starttls").unwrap_or(true),
            notify_email: r.get::<Option<String>, _>("notify_email").unwrap_or_default(),
            admin_email: settings.admin_email.clone(),
        },
        None => EmailConfig {
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_username: String::new(),
            smtp_password: String::new(),
            smtp_from: String::new(),
            smtp_use_tls: false,
            smtp_starttls: true,
            notify_email: String::new(),
            admin_email: settings.admin_email.clone(),
        },
    }
}

pub fn encrypt_secret(settings: &Settings, plaintext: &str) -> String {
    TokenCipher::new(&settings.token_encryption_key).encrypt(plaintext)
}

/// Branding for outgoing emails (name/accent/logo).
#[derive(Clone)]
pub struct EmailBrand {
    pub name: String,
    pub accent_color: String,
    pub logo_url: Option<String>,
}

pub async fn email_brand(db: &PgPool, settings: &Settings) -> EmailBrand {
    let row = sqlx::query(
        "SELECT logo_png, logo_updated_at, brand_display_name, brand_accent_color FROM app_settings LIMIT 1",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    match row {
        Some(r) => {
            let has_logo = r.get::<Option<Vec<u8>>, _>("logo_png").is_some();
            let updated: Option<chrono::DateTime<chrono::Utc>> = r.get("logo_updated_at");
            let logo_url = if has_logo {
                updated.map(|d| {
                    format!("{}/api/branding/logo?v={}", settings.base_url.trim_end_matches('/'), d.timestamp())
                })
            } else {
                None
            };
            EmailBrand {
                name: r.get::<Option<String>, _>("brand_display_name").filter(|s| !s.is_empty()).unwrap_or_else(|| "Portal".to_string()),
                accent_color: r.get::<Option<String>, _>("brand_accent_color").filter(|s| !s.is_empty()).unwrap_or_else(|| "#f59e0b".to_string()),
                logo_url,
            }
        }
        None => EmailBrand { name: "Portal".into(), accent_color: "#f59e0b".into(), logo_url: None },
    }
}

/// The brand a link's pages and emails show: its brand profile where set, with the app-wide brand
/// filling the name and accent the profile leaves empty. A profile's logo stands alone: a client
/// brand without a logo shows none rather than the studio's.
#[derive(Clone, Debug, Default)]
pub struct LinkBrand {
    pub name: Option<String>,
    pub accent: Option<String>,
    pub logo_url: Option<String>,
}

pub async fn link_brand(db: &PgPool, settings: &Settings, profile: Option<uuid::Uuid>) -> LinkBrand {
    let base = settings.base_url.trim_end_matches('/').to_string();
    let nonempty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
    let app = sqlx::query(
        "SELECT logo_png IS NOT NULL AS has_logo, logo_updated_at, brand_display_name, brand_accent_color
           FROM app_settings LIMIT 1",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let (app_name, app_accent, app_logo) = match &app {
        Some(r) => (
            nonempty(r.get("brand_display_name")),
            nonempty(r.get("brand_accent_color")),
            if r.get::<bool, _>("has_logo") {
                r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("logo_updated_at")
                    .map(|d| format!("{base}/api/branding/logo?v={}", d.timestamp()))
            } else {
                None
            },
        ),
        None => (None, None, None),
    };
    let Some(id) = profile else {
        return LinkBrand { name: app_name, accent: app_accent, logo_url: app_logo };
    };
    let row = sqlx::query(
        "SELECT display_name, accent_color, logo_png IS NOT NULL AS has_logo, logo_updated_at
           FROM brand_profiles WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    match row {
        Some(r) => LinkBrand {
            name: nonempty(r.get("display_name")).or(app_name),
            accent: nonempty(r.get("accent_color")).or(app_accent),
            logo_url: if r.get::<bool, _>("has_logo") {
                r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("logo_updated_at")
                    .map(|d| format!("{base}/api/branding/logo?profile={id}&v={}", d.timestamp()))
            } else {
                None
            },
        },
        None => LinkBrand { name: app_name, accent: app_accent, logo_url: app_logo },
    }
}

/// `email_brand`, but for mail about a specific link (its brand profile, if any).
pub async fn email_brand_for(db: &PgPool, settings: &Settings, profile: Option<uuid::Uuid>) -> EmailBrand {
    if profile.is_none() {
        return email_brand(db, settings).await;
    }
    let b = link_brand(db, settings, profile).await;
    EmailBrand {
        name: b.name.unwrap_or_else(|| "Portal".to_string()),
        accent_color: b.accent.unwrap_or_else(|| "#f59e0b".to_string()),
        logo_url: b.logo_url,
    }
}
