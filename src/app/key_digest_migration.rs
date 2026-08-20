//! One-time user-key digest normalization after the storage cipher is ready.

use std::collections::HashMap;

use serde_json::Value;

use crate::crypto::SecretCipher;
use crate::crypto::envelope::is_envelope;
use crate::store::persistence::PersistenceBackend;
use crate::util::api_key::KEY_DIGEST_VERSION;

const LEGACY_DIGEST_VERSION: i64 = 1;

/// Rewrite legacy full-key digests to hashes of the prefix-free key payload.
///
/// Migration 27 adds the per-row version marker. This application phase runs
/// after the cipher is constructed because encrypted key material cannot be
/// normalized by portable SQL. All conflicts are detected before the first
/// write, and per-row versioning makes an interrupted run safely resumable.
pub async fn normalize_user_key_digests(
    persistence: &dyn PersistenceBackend,
    cipher: &dyn SecretCipher,
) -> anyhow::Result<usize> {
    let keys = persistence.list_all_user_keys().await?;
    let mut owners: HashMap<String, i64> = HashMap::with_capacity(keys.len());
    let mut pending = Vec::new();

    for key in keys {
        let digest = match key.api_key_digest_version {
            KEY_DIGEST_VERSION => key.api_key_digest.clone(),
            LEGACY_DIGEST_VERSION => {
                let plain = plaintext_key(&key.api_key_ciphertext, cipher)
                    .map_err(|error| anyhow::anyhow!("decrypt user key {}: {error}", key.id))?;
                crate::util::api_key::key_digest(&plain)
            }
            version => anyhow::bail!(
                "user key {} has unsupported api_key_digest_version {version}",
                key.id
            ),
        };

        if let Some(existing_id) = owners.insert(digest.clone(), key.id)
            && existing_id != key.id
        {
            anyhow::bail!(
                "user keys {existing_id} and {} normalize to the same key payload",
                key.id
            );
        }
        if key.api_key_digest_version == LEGACY_DIGEST_VERSION {
            pending.push((key.id, digest));
        }
    }

    for (id, digest) in &pending {
        persistence
            .update_user_key_digest(*id, digest, KEY_DIGEST_VERSION)
            .await?;
    }
    Ok(pending.len())
}

fn plaintext_key(stored: &str, cipher: &dyn SecretCipher) -> anyhow::Result<String> {
    let value = match serde_json::from_str::<Value>(stored) {
        Ok(value) if is_envelope(&value) => cipher.open(&value)?,
        _ => Value::String(stored.to_owned()),
    };
    match value {
        Value::String(key) => Ok(key),
        other => anyhow::bail!("decrypted user key is not a string: {other}"),
    }
}
