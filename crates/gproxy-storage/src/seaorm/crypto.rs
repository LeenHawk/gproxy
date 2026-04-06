use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use rand::Rng as _;
use serde_json::{Value as JsonValue, json};
use thiserror::Error;

const DATABASE_SECRET_KEY_ENV: &str = "DATABASE_SECRET_KEY";
const STRING_PREFIX: &str = "enc:v2:";
const JSON_MARKER_FIELD: &str = "$gproxy_enc";
const JSON_NONCE_FIELD: &str = "nonce";
const JSON_CIPHERTEXT_FIELD: &str = "ciphertext";
const JSON_VERSION: &str = "v2";
const NONCE_LEN: usize = 24;
/// Fixed salt for Argon2 key derivation — domain separator (not secret, just unique).
const ARGON2_SALT: &[u8] = b"gproxy-db-enc-v2";

#[derive(Clone)]
pub(crate) struct DatabaseCipher {
    cipher: XChaCha20Poly1305,
}

#[derive(Debug, Error)]
pub(crate) enum DatabaseCipherError {
    #[error("{DATABASE_SECRET_KEY_ENV} is empty")]
    EmptySecret,
    #[error("malformed encrypted string")]
    MalformedStringEnvelope,
    #[error("malformed encrypted json envelope")]
    MalformedJsonEnvelope,
    #[error("base64 decode failed: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("cipher operation failed")]
    Cipher,
    #[error("utf-8 decode failed: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("json decode failed: {0}")]
    Json(#[from] serde_json::Error),
}

impl DatabaseCipher {
    pub(crate) fn from_optional_secret(
        secret: Option<&str>,
    ) -> Result<Option<Self>, DatabaseCipherError> {
        match secret {
            Some(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err(DatabaseCipherError::EmptySecret);
                }
                Ok(Some(Self::from_secret(trimmed)))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn from_secret(secret: &str) -> Self {
        let mut okm = [0u8; 32];
        let params = Params::new(19 * 1024, 2, 1, Some(32)).expect("valid argon2 params");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(secret.as_bytes(), ARGON2_SALT, &mut okm)
            .expect("argon2 key derivation");
        let cipher = XChaCha20Poly1305::new((&okm).into());
        Self { cipher }
    }

    pub(crate) fn encrypt_string(&self, plaintext: &str) -> Result<String, DatabaseCipherError> {
        let (nonce, ciphertext) = self.encrypt_bytes(plaintext.as_bytes())?;
        Ok(format!(
            "{STRING_PREFIX}{}:{}",
            URL_SAFE_NO_PAD.encode(nonce),
            URL_SAFE_NO_PAD.encode(ciphertext)
        ))
    }

    pub(crate) fn decrypt_string(&self, raw: &str) -> Result<String, DatabaseCipherError> {
        let Some(rest) = raw.strip_prefix(STRING_PREFIX) else {
            return Ok(raw.to_string());
        };
        let (nonce_b64, ciphertext_b64) = rest
            .split_once(':')
            .ok_or(DatabaseCipherError::MalformedStringEnvelope)?;
        let nonce = URL_SAFE_NO_PAD.decode(nonce_b64)?;
        let ciphertext = URL_SAFE_NO_PAD.decode(ciphertext_b64)?;
        let plaintext = self.decrypt_bytes(&nonce, &ciphertext)?;
        Ok(String::from_utf8(plaintext)?)
    }

    pub(crate) fn encrypt_json(&self, value: &JsonValue) -> Result<JsonValue, DatabaseCipherError> {
        let plaintext = serde_json::to_vec(value)?;
        let (nonce, ciphertext) = self.encrypt_bytes(&plaintext)?;
        Ok(json!({
            JSON_MARKER_FIELD: JSON_VERSION,
            JSON_NONCE_FIELD: URL_SAFE_NO_PAD.encode(nonce),
            JSON_CIPHERTEXT_FIELD: URL_SAFE_NO_PAD.encode(ciphertext),
        }))
    }

    pub(crate) fn decrypt_json(&self, value: JsonValue) -> Result<JsonValue, DatabaseCipherError> {
        let Some(object) = value.as_object() else {
            return Ok(value);
        };
        let Some(marker) = object.get(JSON_MARKER_FIELD) else {
            return Ok(value);
        };
        if marker.as_str() != Some(JSON_VERSION) {
            return Err(DatabaseCipherError::MalformedJsonEnvelope);
        }
        let nonce = object
            .get(JSON_NONCE_FIELD)
            .and_then(|item| item.as_str())
            .ok_or(DatabaseCipherError::MalformedJsonEnvelope)?;
        let ciphertext = object
            .get(JSON_CIPHERTEXT_FIELD)
            .and_then(|item| item.as_str())
            .ok_or(DatabaseCipherError::MalformedJsonEnvelope)?;
        let nonce = URL_SAFE_NO_PAD.decode(nonce)?;
        let ciphertext = URL_SAFE_NO_PAD.decode(ciphertext)?;
        let plaintext = self.decrypt_bytes(&nonce, &ciphertext)?;
        Ok(serde_json::from_slice(&plaintext)?)
    }

    fn encrypt_bytes(
        &self,
        plaintext: &[u8],
    ) -> Result<([u8; NONCE_LEN], Vec<u8>), DatabaseCipherError> {
        let mut nonce = [0_u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher
            .encrypt(XNonce::from_slice(&nonce), plaintext)
            .map_err(|_| DatabaseCipherError::Cipher)?;
        Ok((nonce, ciphertext))
    }

    fn decrypt_bytes(
        &self,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, DatabaseCipherError> {
        if nonce.len() != NONCE_LEN {
            return Err(DatabaseCipherError::MalformedStringEnvelope);
        }
        self.cipher
            .decrypt(XNonce::from_slice(nonce), ciphertext)
            .map_err(|_| DatabaseCipherError::Cipher)
    }
}

#[cfg(test)]
mod tests {
    use super::DatabaseCipher;
    use serde_json::json;

    #[test]
    fn string_roundtrip_and_plaintext_passthrough() {
        let cipher = DatabaseCipher::from_secret("demo-secret");
        let encrypted = cipher.encrypt_string("hello").expect("encrypt");
        assert!(encrypted.starts_with("enc:v2:"));
        assert_eq!(cipher.decrypt_string(&encrypted).expect("decrypt"), "hello");
        assert_eq!(
            cipher.decrypt_string("plain-text").expect("passthrough"),
            "plain-text"
        );
    }

    #[test]
    fn json_roundtrip_and_plaintext_passthrough() {
        let cipher = DatabaseCipher::from_secret("demo-secret");
        let payload = json!({"api_key": "abc", "nested": {"x": 1}});
        let encrypted = cipher.encrypt_json(&payload).expect("encrypt json");
        assert_ne!(encrypted, payload);
        assert_eq!(
            cipher.decrypt_json(encrypted).expect("decrypt json"),
            payload
        );
        let plain = json!({"plain": true});
        assert_eq!(
            cipher
                .decrypt_json(plain.clone())
                .expect("json passthrough"),
            plain
        );
    }
}
