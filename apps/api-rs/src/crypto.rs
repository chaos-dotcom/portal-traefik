
//! AES-GCM encryption for secrets at rest — port of `portal.lib.crypto.TokenCipher`.
//! Output is base64url(nonce || ciphertext) with a 12-byte nonce, matching the Python format
//! exactly, so either implementation can decrypt what the other wrote.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::alphabet;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::engine::DecodePaddingMode;
use base64::Engine;
use sha2::{Digest, Sha256};

const NONCE_BYTES: usize = 12;

/// `binascii.a2b_base64` as `base64.urlsafe_b64decode` calls it: the standard alphabet, padding
/// required, stray trailing bits tolerated.
const PY_B64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_allow_trailing_bits(true)
        .with_decode_padding_mode(DecodePaddingMode::RequireCanonical),
);

/// Python's `base64.urlsafe_b64decode(s)`: map `-_` to `+/`, drop anything outside the base64
/// alphabet (non-strict decoding ignores it), then decode with padding required.
fn python_urlsafe_b64decode(s: &str) -> Option<Vec<u8>> {
    let filtered: String = s
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
        .collect();
    PY_B64.decode(filtered).ok()
}

/// Derive the 32-byte AES key exactly as Python's `_derive_key`: a key that urlsafe-base64
/// decodes (padding required) to 32 bytes is used as is; anything else is SHA-256'd. So a
/// `secrets.token_urlsafe(32)` key — 43 characters, no padding — is hashed, as in Python.
fn derive_key(key_material: &str) -> [u8; 32] {
    if let Some(raw) = python_urlsafe_b64decode(key_material)
        && raw.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw);
        return key;
    }
    Sha256::digest(key_material.as_bytes()).into()
}

/// The key Rust builds derived before they matched Python: they also accepted *unpadded*
/// base64, so for a 43-character key they used the decoded bytes where Python hashes. Kept only
/// to re-encrypt what those builds wrote (see `rekey_legacy_values`). None when it's the same key.
fn legacy_rust_key(key_material: &str) -> Option<[u8; 32]> {
    for engine in [&URL_SAFE, &URL_SAFE_NO_PAD, &STANDARD_NO_PAD] {
        if let Ok(decoded) = engine.decode(key_material)
            && decoded.len() == 32
        {
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            return (key != derive_key(key_material)).then_some(key);
        }
    }
    None
}

pub struct TokenCipher {
    cipher: Aes256Gcm,
}

impl TokenCipher {
    pub fn new(key_material: &str) -> Self {
        Self::from_key(&derive_key(key_material))
    }

    fn from_key(key: &[u8; 32]) -> Self {
        Self { cipher: Aes256Gcm::new_from_slice(key).expect("aes key") }
    }

    pub fn encrypt(&self, plaintext: &str) -> String {
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes).expect("os random");
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .expect("aes encrypt");
        let mut out = nonce_bytes.to_vec();
        out.extend_from_slice(&ciphertext);
        URL_SAFE.encode(out)
    }

    pub fn decrypt(&self, token: &str) -> Result<String, CryptoError> {
        let raw = URL_SAFE
            .decode(token)
            .or_else(|_| URL_SAFE_NO_PAD.decode(token))
            .map_err(|_| CryptoError)?;
        if raw.len() < NONCE_BYTES {
            return Err(CryptoError);
        }
        let (nonce, ciphertext) = raw.split_at(NONCE_BYTES);
        let nonce = Nonce::try_from(nonce).map_err(|_| CryptoError)?;
        let plain = self
            .cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| CryptoError)?;
        String::from_utf8(plain).map_err(|_| CryptoError)
    }
}

#[derive(Debug)]
pub struct CryptoError;
impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "decryption failed")
    }
}
impl std::error::Error for CryptoError {}

/// Columns holding a `TokenCipher` ciphertext: (table, column).
const ENCRYPTED_COLUMNS: [(&str, &str); 5] = [
    ("public.users", "totp_secret_encrypted"),
    ("public.app_settings", "smtp_password_encrypted"),
    ("public.app_settings", "frameio_client_secret_encrypted"),
    ("public.frameio_connections", "access_token_encrypted"),
    ("public.frameio_connections", "refresh_token_encrypted"),
];

/// Re-encrypt, under the Python-compatible key, every value an earlier Rust build wrote with its
/// mismatched key (see `legacy_rust_key`), so both implementations can read everything again.
/// Values that already decrypt — or decrypt under neither key — are left alone. Returns the count.
pub async fn rekey_legacy_values(conn: &mut sqlx::PgConnection, key_material: &str) -> anyhow::Result<usize> {
    let Some(legacy) = legacy_rust_key(key_material) else { return Ok(0) };
    let (current, legacy) = (TokenCipher::new(key_material), TokenCipher::from_key(&legacy));
    let mut rekeyed = 0;
    for (table, column) in ENCRYPTED_COLUMNS {
        // AssertSqlSafe: table and column names come from the constant list above.
        let select = format!("SELECT id::text, {column} FROM {table} WHERE {column} IS NOT NULL");
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(select)).fetch_all(&mut *conn).await?;
        for (id, value) in rows {
            if current.decrypt(&value).is_ok() {
                continue;
            }
            let Ok(plain) = legacy.decrypt(&value) else { continue };
            let update = format!("UPDATE {table} SET {column} = $1 WHERE id = $2::uuid");
            sqlx::query(sqlx::AssertSqlSafe(update))
                .bind(current.encrypt(&plain))
                .bind(&id)
                .execute(&mut *conn)
                .await?;
            rekeyed += 1;
        }
    }
    Ok(rekeyed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ciphertexts of "frameio-token-é" from Python's portal.lib.crypto.TokenCipher.
    const PY_VECTORS: [(&str, &str); 5] = [
        // secrets.token_urlsafe(32): 43 chars, no padding — Python hashes it (production's case).
        ("Zm9vYmFyLWJhei1xdXV4X3Rlc3Qta2V5LW5vLXBhZGQ", "b_aAzKT_phSBiMC0smiz7LsLMHZuoAm0t2YPZIBG5rzFXNZUa2H5bcvv7iI="),
        // Padded urlsafe base64 of 32 bytes — used as is.
        ("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "5GMlGjnTVmZMiurirxHJx9BVSMzEti1tlSzX-0WY7Gj1MdiXlqOF_tptvXc="),
        // Padded standard-alphabet base64 — Python's decoder accepts + and / too.
        ("yMnKy8zNzs/Q0dLT1NXW19jZ2tvc3d7f4OHi4+Tl5uc=", "kHFp7tz2Uz6KUVGkLZE7UffljpXsQ0sZsBtb3Xm01VuubHBbI_ZhIJDNmyc="),
        // Not base64 at all — hashed.
        ("change-me-token-encryption-key", "RbdasIQch35Nz_CTUDaT0YNd95gQo8VVJ9C2FPBkwXgHFev1yebD5uWHWBg="),
        // A stray newline is ignored by Python's non-strict decoder.
        ("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\n", "4oLpkNfDV6wC7b3p3weo80OmWBnYHMYxBZkWU8_Xr_B9EKQeXL6dCT4Gqbs="),
    ];

    #[test]
    fn decrypts_what_python_encrypted() {
        for (key, ciphertext) in PY_VECTORS {
            assert_eq!(TokenCipher::new(key).decrypt(ciphertext).as_deref().ok(), Some("frameio-token-é"), "key {key:?}");
        }
    }

    #[test]
    fn round_trips() {
        let c = TokenCipher::new("Zm9vYmFyLWJhei1xdXV4X3Rlc3Qta2V5LW5vLXBhZGQ");
        assert_eq!(c.decrypt(&c.encrypt("secret")).unwrap(), "secret");
    }

    #[test]
    fn legacy_key_only_where_the_old_derivation_differed() {
        let unpadded = "Zm9vYmFyLWJhei1xdXV4X3Rlc3Qta2V5LW5vLXBhZGQ";
        let legacy = TokenCipher::from_key(&legacy_rust_key(unpadded).expect("differs for unpadded keys"));
        let written_by_old_build = legacy.encrypt("frameio-token-é");
        assert!(TokenCipher::new(unpadded).decrypt(&written_by_old_build).is_err());
        assert_eq!(legacy.decrypt(&written_by_old_build).unwrap(), "frameio-token-é");
        // Padded and non-base64 keys derived the same before and after.
        assert!(legacy_rust_key("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=").is_none());
        assert!(legacy_rust_key("change-me-token-encryption-key").is_none());
    }
}
