//! MIGRATE-V1 (remove in 2.1): decrypt secrets stored by the legacy v1 backend.
//!
//! v1 (`crates/gproxy-storage/src/seaorm/crypto.rs`) sealed `credentials.secret_json`
//! and `user_keys.api_key_ciphertext` with XChaCha20-Poly1305 under an Argon2id
//! key derived from the `DATABASE_SECRET_KEY` env. This module reproduces only
//! the *decrypt* half so the migration can recover plaintext, which the v2
//! `import_bundle` path then re-seals under `GPROXY_MASTER_KEY`.
//!
//! When `DATABASE_SECRET_KEY` is unset, v1 stored everything in plaintext — the
//! decrypt fns then short-circuit (no prefix / no marker = return as-is).

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde_json::Value;

/// v1 string-envelope prefix: `enc:v2:<nonce_b64url>:<ct_b64url>`.
const STRING_PREFIX: &str = "enc:v2:";
/// v1 JSON-envelope marker field + version.
const JSON_MARKER_FIELD: &str = "$gproxy_enc";
const JSON_VERSION: &str = "v2";
/// Fixed Argon2 salt (domain separator, not secret) — must match v1 verbatim.
const ARGON2_SALT: &[u8] = b"gproxy-db-enc-v2";
const NONCE_LEN: usize = 24;

/// v1 secret cipher (decrypt-only). `None` cipher = v1 ran keyless (plaintext).
pub struct V1Cipher {
    cipher: Option<XChaCha20Poly1305>,
}

impl V1Cipher {
    /// Build from the `DATABASE_SECRET_KEY` env (v1's key name). Absent/blank =
    /// keyless mode: secrets were stored as plaintext, so decryption is a no-op.
    pub fn from_env() -> Self {
        match std::env::var("DATABASE_SECRET_KEY") {
            Ok(s) if !s.trim().is_empty() => Self {
                cipher: Some(Self::derive(s.trim())),
            },
            _ => Self { cipher: None },
        }
    }

    /// Argon2id(secret, salt) → 32-byte key → XChaCha20-Poly1305 (matches v1).
    fn derive(secret: &str) -> XChaCha20Poly1305 {
        let mut okm = [0u8; 32];
        let params = Params::new(19 * 1024, 2, 1, Some(32)).expect("valid argon2 params");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(secret.as_bytes(), ARGON2_SALT, &mut okm)
            .expect("argon2 key derivation");
        XChaCha20Poly1305::new(&Key::from(okm))
    }

    fn decrypt_bytes(&self, nonce: &[u8], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
        let cipher = self.cipher.as_ref().ok_or_else(|| {
            anyhow::anyhow!("v1 secret is encrypted but DATABASE_SECRET_KEY is unset")
        })?;
        anyhow::ensure!(nonce.len() == NONCE_LEN, "v1 envelope: bad nonce length");
        let nonce = XNonce::try_from(nonce)
            .map_err(|_| anyhow::anyhow!("v1 envelope: bad nonce length"))?;
        cipher.decrypt(&nonce, ciphertext).map_err(|_| {
            anyhow::anyhow!("v1 secret decryption failed (wrong DATABASE_SECRET_KEY?)")
        })
    }

    /// Decrypt a v1 string envelope. No `enc:v2:` prefix → already plaintext.
    pub fn decrypt_string(&self, raw: &str) -> anyhow::Result<String> {
        let Some(rest) = raw.strip_prefix(STRING_PREFIX) else {
            return Ok(raw.to_string());
        };
        let (nonce_b64, ct_b64) = rest
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("malformed v1 string envelope"))?;
        let nonce = URL_SAFE_NO_PAD.decode(nonce_b64)?;
        let ciphertext = URL_SAFE_NO_PAD.decode(ct_b64)?;
        let plaintext = self.decrypt_bytes(&nonce, &ciphertext)?;
        Ok(String::from_utf8(plaintext)?)
    }

    /// Decrypt a v1 JSON envelope `{"$gproxy_enc":"v2","nonce":..,"ciphertext":..}`.
    /// A value without the marker is already plaintext and returned unchanged.
    pub fn decrypt_json(&self, value: Value) -> anyhow::Result<Value> {
        let Some(object) = value.as_object() else {
            return Ok(value);
        };
        let Some(marker) = object.get(JSON_MARKER_FIELD) else {
            return Ok(value);
        };
        anyhow::ensure!(
            marker.as_str() == Some(JSON_VERSION),
            "unsupported v1 json envelope version"
        );
        let nonce = object
            .get("nonce")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("malformed v1 json envelope: nonce"))?;
        let ciphertext = object
            .get("ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("malformed v1 json envelope: ciphertext"))?;
        let nonce = URL_SAFE_NO_PAD.decode(nonce)?;
        let ciphertext = URL_SAFE_NO_PAD.decode(ciphertext)?;
        let plaintext = self.decrypt_bytes(&nonce, &ciphertext)?;
        Ok(serde_json::from_slice(&plaintext)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chacha20poly1305::aead::Aead;
    use serde_json::json;

    // Re-encrypt helper mirroring v1's seal, to exercise the decrypt path.
    fn seal_string(key: &str, plaintext: &str) -> String {
        let cipher = V1Cipher::derive(key);
        let nonce = [7u8; NONCE_LEN];
        let xnonce = XNonce::try_from(&nonce[..]).unwrap();
        let ct = cipher.encrypt(&xnonce, plaintext.as_bytes()).unwrap();
        format!(
            "{STRING_PREFIX}{}:{}",
            URL_SAFE_NO_PAD.encode(nonce),
            URL_SAFE_NO_PAD.encode(ct)
        )
    }

    #[test]
    fn plaintext_passthrough_when_keyless() {
        let c = V1Cipher { cipher: None };
        assert_eq!(c.decrypt_string("sk-bare-key").unwrap(), "sk-bare-key");
        let v = json!({"api_key": "sk-x"});
        assert_eq!(c.decrypt_json(v.clone()).unwrap(), v);
    }

    #[test]
    fn decrypts_v1_string_envelope() {
        let c = V1Cipher {
            cipher: Some(V1Cipher::derive("topsecret")),
        };
        let sealed = seal_string("topsecret", "sk-ant-oat01-abc");
        assert_eq!(c.decrypt_string(&sealed).unwrap(), "sk-ant-oat01-abc");
    }

    #[test]
    fn encrypted_without_key_errors() {
        let c = V1Cipher { cipher: None };
        let sealed = seal_string("topsecret", "secret");
        assert!(c.decrypt_string(&sealed).is_err());
    }
}
