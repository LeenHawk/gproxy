//! Envelope encryption for stored secrets (architecture-design §14.1).
//!
//! Secrets (`credentials.secret_json`, `user_keys.api_key_ciphertext`) are
//! stored as envelopes: a per-secret random DEK encrypts the plaintext with
//! XChaCha20-Poly1305, and the DEK is wrapped by a KEK from
//! `GPROXY_MASTER_KEY` ([`kms::LocalKms`]). Snapshots carry sealed values;
//! the pipeline decrypts at use (per failover attempt, µs-scale).
//!
//! Single KEK only for now — rotation is deferred (M9+ export/import or
//! batch re-seal); `kek_id` in the envelope is the rotation marker.
//! Compiled unconditionally: secrets exist on edge too.

pub mod envelope;
pub mod kms;
pub mod password;

use std::sync::{Arc, Once};

use serde_json::Value;

use envelope::is_envelope;
use kms::{Kms, LocalKms};

/// Object-safe cipher for stored secrets.
pub trait SecretCipher: Send + Sync {
    /// Seal a plaintext JSON value (identity in keyless mode).
    fn seal(&self, plain: &Value) -> anyhow::Result<Value>;
    /// Open a stored value. A non-envelope value is returned as-is (legacy
    /// plaintext compatibility).
    fn open(&self, stored: &Value) -> anyhow::Result<Value>;
}

/// Real envelope cipher over a [`Kms`].
pub struct EnvelopeCipher {
    kms: Box<dyn Kms>,
}

impl EnvelopeCipher {
    pub fn new(kms: Box<dyn Kms>) -> Self {
        Self { kms }
    }
}

impl SecretCipher for EnvelopeCipher {
    fn seal(&self, plain: &Value) -> anyhow::Result<Value> {
        envelope::seal_with(self.kms.as_ref(), plain)
    }

    fn open(&self, stored: &Value) -> anyhow::Result<Value> {
        if !is_envelope(stored) {
            return Ok(stored.clone());
        }
        envelope::open_with(self.kms.as_ref(), stored)
    }
}

/// Keyless mode: secrets stay plaintext (warns once on first seal). Refuses
/// to open an envelope — returning a sealed blob as if it were plaintext
/// would silently hand ciphertext to upstream calls after the master key
/// was removed.
pub struct NoopCipher;

static NOOP_WARN: Once = Once::new();

impl SecretCipher for NoopCipher {
    fn seal(&self, plain: &Value) -> anyhow::Result<Value> {
        NOOP_WARN.call_once(|| {
            tracing::warn!(
                "GPROXY_MASTER_KEY not set — secrets are stored in PLAINTEXT; \
                 set a base64 32-byte master key to enable envelope encryption"
            );
        });
        Ok(plain.clone())
    }

    fn open(&self, stored: &Value) -> anyhow::Result<Value> {
        if is_envelope(stored) {
            anyhow::bail!("sealed secret but no master key configured (set GPROXY_MASTER_KEY)");
        }
        Ok(stored.clone())
    }
}

/// Build the process-wide cipher. `Some(key)` → [`EnvelopeCipher`] over
/// [`LocalKms`] (malformed key → `Err`, callers should hard-fail boot);
/// `None` → [`NoopCipher`].
pub fn cipher_from_master_key(master_b64: Option<&str>) -> anyhow::Result<Arc<dyn SecretCipher>> {
    match master_b64 {
        Some(s) => Ok(Arc::new(EnvelopeCipher::new(Box::new(
            LocalKms::from_master_key_b64(s)?,
        )))),
        None => Ok(Arc::new(NoopCipher)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use serde_json::json;

    fn cipher(key_byte: u8) -> Arc<dyn SecretCipher> {
        cipher_from_master_key(Some(&B64.encode([key_byte; 32]))).unwrap()
    }

    #[test]
    fn seal_open_roundtrip_with_envelope_shape() {
        let c = cipher(7);
        let plain = json!({"api_key": "sk-test-123", "region": "us", "n": 1});
        let sealed = c.seal(&plain).unwrap();
        assert!(is_envelope(&sealed));
        let obj = sealed.as_object().unwrap();
        assert!(obj["kek_id"].as_str().unwrap().starts_with("local-"));
        for k in ["wrapped_dek", "nonce", "ciphertext"] {
            B64.decode(obj[k].as_str().unwrap()).unwrap();
        }
        assert_eq!(c.open(&sealed).unwrap(), plain);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let c = cipher(7);
        let mut sealed = c.seal(&json!({"k": "v"})).unwrap();
        let mut ct = B64.decode(sealed["ciphertext"].as_str().unwrap()).unwrap();
        ct[0] ^= 0x01;
        sealed["ciphertext"] = Value::String(B64.encode(ct));
        assert!(c.open(&sealed).is_err());
    }

    #[test]
    fn wrong_kek_error_names_both_kek_ids() {
        let sealed = cipher(1).seal(&json!({"k": "v"})).unwrap();
        let sealing_kek = sealed["kek_id"].as_str().unwrap().to_string();
        let opening_kek = LocalKms::from_master_key_b64(&B64.encode([2u8; 32]))
            .unwrap()
            .kek_id()
            .to_string();
        let err = cipher(2).open(&sealed).unwrap_err().to_string();
        assert!(err.contains(&sealing_kek), "missing sealing kek in: {err}");
        assert!(err.contains(&opening_kek), "missing opening kek in: {err}");
    }

    #[test]
    fn passthrough_and_noop_envelope_guard() {
        let plain = json!({"api_key": "bare-plaintext"});
        let env_cipher = cipher(7);
        assert_eq!(env_cipher.open(&plain).unwrap(), plain);
        assert_eq!(NoopCipher.open(&plain).unwrap(), plain);
        assert_eq!(NoopCipher.seal(&plain).unwrap(), plain);
        let sealed = env_cipher.seal(&plain).unwrap();
        let err = NoopCipher.open(&sealed).unwrap_err().to_string();
        assert!(err.contains("no master key"), "unexpected error: {err}");
    }
}
