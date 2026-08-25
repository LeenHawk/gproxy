use crate::query::runtime;
use crate::records::{CredentialHealthInput, CredentialHealthRecord, CredentialHealthState};
use crate::{Store, StoreError};

impl Store {
    pub async fn record_credential_health(
        &self,
        input: &CredentialHealthInput,
    ) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::upsert_credential_health(input)?)
            .await?;
        Ok(())
    }

    pub async fn credential_health(&self) -> Result<Vec<CredentialHealthRecord>, StoreError> {
        self.backend()
            .execute(runtime::select_credential_health()?)
            .await?
            .rows
            .into_iter()
            .map(|row| {
                let state = row.text("state")?;
                Ok(CredentialHealthRecord {
                    credential_id: row.i64("credential_id")?,
                    credential_version: u64::try_from(row.i64("credential_version")?).map_err(
                        |error| StoreError::InvalidData {
                            field: "credential health version",
                            message: error.to_string(),
                        },
                    )?,
                    version: row.i64("version")?,
                    state: CredentialHealthState::from_name(state).ok_or_else(|| {
                        StoreError::InvalidData {
                            field: "credential health state",
                            message: format!("unknown state `{state}`"),
                        }
                    })?,
                    observed_at: row.i64("observed_at")?,
                    response_status: row
                        .optional_i64("response_status")?
                        .map(|value| {
                            u16::try_from(value).map_err(|error| StoreError::InvalidData {
                                field: "credential health response_status",
                                message: error.to_string(),
                            })
                        })
                        .transpose()?,
                    detail: row.optional_text("detail")?.map(str::to_owned),
                })
            })
            .collect()
    }

    pub async fn clear_credential_health(&self, credential_id: i64) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::delete_credential_health(credential_id)?)
            .await?;
        Ok(())
    }
}
