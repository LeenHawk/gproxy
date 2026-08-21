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
/// normalized by portable SQL. All conflicts are resolved before the first
/// write, and per-row versioning makes an interrupted run safely resumable.
///
/// `sk-X` and `at-X` are separate rows before the migration but one identity
/// after it, so a database holding both cannot normalize all of its keys. The
/// contested row is left unmigrated rather than aborting startup: it stops
/// authenticating (the snapshot skips it) while every other key keeps working.
pub async fn normalize_user_key_digests(
    persistence: &dyn PersistenceBackend,
    cipher: &dyn SecretCipher,
) -> anyhow::Result<usize> {
    let mut keys = persistence.list_all_user_keys().await?;
    // Deterministic resolution: the lowest id wins a contested payload.
    keys.sort_by_key(|key| key.id);

    let mut owners: HashMap<String, i64> = HashMap::with_capacity(keys.len());
    let mut legacy = Vec::new();

    for key in keys {
        match key.api_key_digest_version {
            // Already normalized: it owns its digest and can never be displaced.
            KEY_DIGEST_VERSION => {
                owners.insert(key.api_key_digest.clone(), key.id);
            }
            LEGACY_DIGEST_VERSION => {
                let plain = plaintext_key(&key.api_key_ciphertext, cipher)
                    .map_err(|error| anyhow::anyhow!("decrypt user key {}: {error}", key.id))?;
                legacy.push((key.id, crate::util::api_key::key_digest(&plain)));
            }
            version => anyhow::bail!(
                "user key {} has unsupported api_key_digest_version {version}",
                key.id
            ),
        }
    }

    let (pending, conflicts) = claim_digests(legacy, owners);
    for (id, existing_id) in conflicts {
        tracing::warn!(
            user_key = id,
            conflicts_with = existing_id,
            "user key normalizes to an already-claimed key payload; leaving it \
             unmigrated — it cannot authenticate until one of the two is removed"
        );
    }

    for (id, digest) in &pending {
        persistence
            .update_user_key_digest(*id, digest, KEY_DIGEST_VERSION)
            .await?;
    }
    Ok(pending.len())
}

/// Rows to write: `(user key id, normalized digest)`.
type PendingDigests = Vec<(i64, String)>;
/// Skipped rows: `(skipped key id, key id already owning that payload)`.
type DigestConflicts = Vec<(i64, i64)>;

/// Assign each legacy key its normalized digest, skipping any payload already
/// claimed. Returns the rows to write and the `(skipped, winner)` conflicts.
fn claim_digests(
    legacy: PendingDigests,
    mut owners: HashMap<String, i64>,
) -> (PendingDigests, DigestConflicts) {
    let mut pending = Vec::new();
    let mut conflicts = Vec::new();
    for (id, digest) in legacy {
        match owners.get(&digest) {
            Some(&existing_id) => conflicts.push((id, existing_id)),
            None => {
                owners.insert(digest.clone(), id);
                pending.push((id, digest));
            }
        }
    }
    (pending, conflicts)
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
