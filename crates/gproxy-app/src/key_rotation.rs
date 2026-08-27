use sha2::{Digest as _, Sha256};

use crate::AppError;
use crate::config::{MasterKeyConfig, RotationTarget};
use crate::secrets::EnvelopeCipher;
use gproxy_store::records::{MasterKeyFingerprint, StoredSecret};

pub(crate) async fn prepare(
    store: &gproxy_store::Store,
    keys: &MasterKeyConfig,
) -> Result<EnvelopeCipher, AppError> {
    let inventory = store.secret_inventory().await?;
    let current_fingerprint = fingerprint(keys.current.as_ref());
    require_mode(&inventory.fingerprint, current_fingerprint.as_deref())?;
    let current = EnvelopeCipher::new(keys.current);
    let opened_credentials = open_all(&current, &inventory.credentials, false)?;
    let opened_user_keys = open_all(&current, &inventory.user_keys, true)?;

    if !keys.rotate {
        if !matches!(&keys.next, RotationTarget::Unset) {
            tracing::warn!(
                "GPROXY_MASTER_KEY_NEXT is set but GPROXY_MASTER_KEY_ROTATE is off; rotation was not attempted"
            );
        }
        if matches!(inventory.fingerprint, MasterKeyFingerprint::Missing) {
            store
                .replace_secret_inventory(&[], &[], current_fingerprint.as_deref())
                .await?;
        }
        if keys.current.is_none() {
            tracing::warn!("secrets are stored in plaintext because GPROXY_MASTER_KEY is unset");
        }
        return Ok(current);
    }

    let next_key = match &keys.next {
        RotationTarget::Unset => {
            return Err(AppError::Bootstrap(
                "GPROXY_MASTER_KEY_ROTATE is on but GPROXY_MASTER_KEY_NEXT is unset".into(),
            ));
        }
        RotationTarget::Plaintext => None,
        RotationTarget::Key(key) => Some(*key),
    };
    let next = EnvelopeCipher::new(next_key);
    let credentials = reseal_all(&next, &inventory.credentials, opened_credentials, false)?;
    let user_keys = reseal_all(&next, &inventory.user_keys, opened_user_keys, true)?;
    let next_fingerprint = fingerprint(next_key.as_ref());
    store
        .replace_secret_inventory(&credentials, &user_keys, next_fingerprint.as_deref())
        .await?;
    if next_key.is_some() {
        tracing::warn!(
            "secret-key rotation completed; copy GPROXY_MASTER_KEY_NEXT to GPROXY_MASTER_KEY, then clear GPROXY_MASTER_KEY_NEXT and GPROXY_MASTER_KEY_ROTATE"
        );
    } else {
        tracing::warn!(
            "secret-key rotation to plaintext completed; unset GPROXY_MASTER_KEY, then clear GPROXY_MASTER_KEY_NEXT and GPROXY_MASTER_KEY_ROTATE"
        );
    }
    Ok(next)
}

fn require_mode(stored: &MasterKeyFingerprint, supplied: Option<&str>) -> Result<(), AppError> {
    match (stored, supplied) {
        (MasterKeyFingerprint::Missing, _) | (MasterKeyFingerprint::Plaintext, None) => Ok(()),
        (MasterKeyFingerprint::Sealed(required), Some(actual)) if required == actual => Ok(()),
        (MasterKeyFingerprint::Sealed(required), _) => Err(AppError::Encryption(format!(
            "store requires secret key fingerprint {required}"
        ))),
        (MasterKeyFingerprint::Plaintext, Some(_)) => Err(AppError::Encryption(
            "store requires plaintext mode, but GPROXY_MASTER_KEY is set".into(),
        )),
    }
}

fn open_all(
    cipher: &EnvelopeCipher,
    secrets: &[StoredSecret],
    user_key: bool,
) -> Result<Vec<serde_json::Value>, AppError> {
    secrets
        .iter()
        .map(|secret| {
            if user_key {
                cipher.open_user_key(&secret.envelope)
            } else {
                cipher.open(&secret.envelope)
            }
        })
        .collect()
}

fn reseal_all(
    cipher: &EnvelopeCipher,
    stored: &[StoredSecret],
    values: Vec<serde_json::Value>,
    user_key: bool,
) -> Result<Vec<StoredSecret>, AppError> {
    stored
        .iter()
        .zip(values)
        .map(|(stored, value)| {
            let envelope = if user_key {
                cipher.seal_user_key(&value)?
            } else {
                cipher.seal(&value)?
            };
            Ok(StoredSecret {
                id: stored.id,
                envelope,
            })
        })
        .collect()
}

pub(crate) fn fingerprint(key: Option<&[u8; 32]>) -> Option<String> {
    key.map(|key| {
        let digest = Sha256::digest(key);
        let mut output = String::with_capacity(71);
        output.push_str("sha256:");
        for byte in digest {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("write to string");
        }
        output
    })
}
