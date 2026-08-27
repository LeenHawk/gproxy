use serde_json::Value;

use crate::backend::{QueryResult, Row};
use crate::query::control;
use crate::records::{
    CredentialEnvelope, SecretInventory, SecretKeyFingerprint, SettingInput, StoredSecret,
};
use crate::{Store, StoreError};

pub const SECRET_KEY_FINGERPRINT: &str = "secret_key_fingerprint";

impl Store {
    pub async fn secret_inventory(&self) -> Result<SecretInventory, StoreError> {
        let mut results = self
            .backend()
            .batch(vec![
                control::select_secret_fingerprint(SECRET_KEY_FINGERPRINT)?,
                control::select_credential_secrets()?,
                control::select_user_key_secrets()?,
            ])
            .await?
            .into_iter();
        Ok(SecretInventory {
            fingerprint: parse_fingerprint(next(&mut results)?)?,
            credentials: parse_secrets(next(&mut results)?)?,
            user_keys: parse_secrets(next(&mut results)?)?,
        })
    }

    pub async fn replace_secret_inventory(
        &self,
        credentials: &[StoredSecret],
        user_keys: &[StoredSecret],
        fingerprint: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut statements = credentials
            .iter()
            .map(|secret| control::update_secret("credentials", secret.id, &secret.envelope))
            .chain(
                user_keys
                    .iter()
                    .map(|secret| control::update_secret("user_keys", secret.id, &secret.envelope)),
            )
            .collect::<Result<Vec<_>, _>>()?;
        statements.push(control::insert_setting(&SettingInput {
            key: SECRET_KEY_FINGERPRINT.into(),
            value: fingerprint.map_or(Value::Null, |value| Value::String(value.into())),
        })?);
        self.backend().batch(statements).await?;
        Ok(())
    }
}

fn parse_fingerprint(mut result: QueryResult) -> Result<SecretKeyFingerprint, StoreError> {
    let Some(row) = result.rows.pop() else {
        return Ok(SecretKeyFingerprint::Missing);
    };
    match serde_json::from_str(row.text("value_json")?).map_err(invalid_fingerprint)? {
        Value::Null => Ok(SecretKeyFingerprint::Plaintext),
        Value::String(value) => Ok(SecretKeyFingerprint::Sealed(value)),
        _ => Err(invalid_fingerprint("expected string or null")),
    }
}

fn parse_secrets(result: QueryResult) -> Result<Vec<StoredSecret>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(StoredSecret {
                id: row.i64("id")?,
                envelope: parse_envelope(&row)?,
            })
        })
        .collect()
}

fn parse_envelope(row: &Row) -> Result<CredentialEnvelope, StoreError> {
    Ok(CredentialEnvelope {
        ciphertext: row.blob("ciphertext")?.to_vec(),
        wrapped_key: row.blob("wrapped_key")?.to_vec(),
        payload_nonce: row.blob("payload_nonce")?.to_vec(),
        key_nonce: row.blob("key_nonce")?.to_vec(),
    })
}

fn next(results: &mut impl Iterator<Item = QueryResult>) -> Result<QueryResult, StoreError> {
    results
        .next()
        .ok_or_else(|| StoreError::Database("secret inventory result missing".into()))
}

fn invalid_fingerprint(error: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData {
        field: "secret key fingerprint",
        message: error.to_string(),
    }
}
