use crate::query::control;
use crate::records::{CredentialEnvelope, CredentialRecord};
use crate::{Store, StoreError};

impl Store {
    pub async fn credential(&self, id: i64) -> Result<Option<CredentialRecord>, StoreError> {
        let mut result = self
            .backend()
            .execute(control::load_credential(id)?)
            .await?;
        let Some(row) = result.rows.pop() else {
            return Ok(None);
        };
        Ok(Some(CredentialRecord {
            id: row.i64("id")?,
            provider_id: row.i64("provider_id")?,
            channel: row.text("channel")?.to_owned(),
            label: row.optional_text("label")?.map(str::to_owned),
            envelope: CredentialEnvelope {
                ciphertext: row.blob("ciphertext")?.to_vec(),
                wrapped_key: row.blob("wrapped_key")?.to_vec(),
                payload_nonce: row.blob("payload_nonce")?.to_vec(),
                key_nonce: row.blob("key_nonce")?.to_vec(),
            },
            version: to_u64(row.i64("version")?, "credential version")?,
            enabled: row.i64("enabled")? != 0,
        }))
    }

    pub async fn persist_credential_rotation(
        &self,
        id: i64,
        envelope: &CredentialEnvelope,
        version: u64,
    ) -> Result<(), StoreError> {
        let result = self
            .backend()
            .execute(control::compare_and_swap_credential(id, envelope, version)?)
            .await?;
        if result.affected_rows == 1 {
            Ok(())
        } else {
            Err(StoreError::VersionConflict)
        }
    }
}

fn to_u64(value: i64, field: &'static str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|error| StoreError::InvalidData {
        field,
        message: error.to_string(),
    })
}
