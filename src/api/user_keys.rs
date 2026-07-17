//! User-key DTOs.
//!
//! Keys are generated server-side and stored encrypted. Authenticated key-list
//! responses include the decrypted key so the console can display it directly;
//! writes still never accept caller-supplied key material.

use crate::crypto::SecretCipher;
use crate::crypto::envelope::is_envelope;
use crate::store::persistence::records::UserKey;

/// Read-side user-key shape. `key_prefix` is retained for API compatibility;
/// the console displays the complete `api_key`.
#[derive(serde::Serialize)]
pub struct UserKeyView {
    pub id: i64,
    pub user_id: i64,
    pub label: Option<String>,
    pub enabled: bool,
    /// First 8 chars of the digest, retained for older API clients.
    pub key_prefix: String,
    /// Decrypted API key.
    pub api_key: String,
}

impl UserKeyView {
    /// Build a view when the plaintext is already available (for example,
    /// immediately after generating a key).
    pub fn from_plain(k: UserKey, api_key: String) -> Self {
        let key_prefix: String = k.api_key_digest.chars().take(8).collect();
        UserKeyView {
            id: k.id,
            user_id: k.user_id,
            label: k.label,
            enabled: k.enabled,
            key_prefix,
            api_key,
        }
    }

    /// Recover a stored plaintext key and build its API view. Keyless stores
    /// contain the bare string; encrypted stores contain a serialized envelope.
    pub fn from_stored(k: UserKey, cipher: &dyn SecretCipher) -> anyhow::Result<Self> {
        let stored = match serde_json::from_str(&k.api_key_ciphertext) {
            Ok(value) if is_envelope(&value) => value,
            _ => serde_json::Value::String(k.api_key_ciphertext.clone()),
        };
        let api_key = match cipher.open(&stored)? {
            serde_json::Value::String(value) => value,
            other => anyhow::bail!("decrypted user_key {} is not a string: {other}", k.id),
        };
        Ok(Self::from_plain(k, api_key))
    }
}

fn default_true() -> bool {
    true
}

/// Write-side user-key shape. `id = None` creates (the key material is
/// GENERATED server-side), `Some(id)` updates label/enabled
/// (key material is immutable — rotate by create + delete). `api_key` is kept
/// in the shape only so a caller supplying one gets an explicit 400 instead of
/// a silent ignore; external key material uses the separate import or native
/// first-run bootstrap paths.
#[derive(serde::Deserialize)]
pub struct UserKeyUpsert {
    #[serde(default)]
    pub id: Option<i64>,
    /// User id — taken from the path; ignored if present in the body.
    #[serde(default)]
    pub user_id: i64,
    /// Rejected if present (400) — keys are server-generated.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use serde_json::Value;

    use super::*;
    use crate::crypto::{NoopCipher, cipher_from_master_key};

    fn stored_key(api_key_ciphertext: String) -> UserKey {
        UserKey {
            id: 7,
            user_id: 3,
            api_key_ciphertext,
            api_key_digest: "1234567890abcdef".into(),
            label: Some("test".into()),
            enabled: true,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn stored_plaintext_and_encrypted_keys_are_revealed() {
        let bare = "sk-existing-key";
        let plain = UserKeyView::from_stored(stored_key(bare.into()), &NoopCipher).unwrap();
        assert_eq!(plain.api_key, bare);

        let master_key = B64.encode([42_u8; 32]);
        let cipher = cipher_from_master_key(Some(&master_key)).unwrap();
        let envelope = cipher.seal(&Value::String(bare.into())).unwrap();
        let encrypted = UserKeyView::from_stored(
            stored_key(serde_json::to_string(&envelope).unwrap()),
            cipher.as_ref(),
        )
        .unwrap();
        assert_eq!(encrypted.api_key, bare);
        assert_eq!(encrypted.key_prefix, "12345678");
    }
}
