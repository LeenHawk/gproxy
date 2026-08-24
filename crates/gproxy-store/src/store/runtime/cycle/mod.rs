mod read;
mod row;
mod state;

use crate::query::runtime;
use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaCycleCloseReason,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn observe_credential_quota_cycle(
        &self,
        input: &CredentialQuotaObservation,
    ) -> Result<CredentialQuotaCycleRecord, StoreError> {
        state::validate(input)?;
        const RETRIES: usize = 8;
        for _ in 0..RETRIES {
            let open = self
                .open_credential_quota_cycle(input.credential_id, &input.window_key)
                .await?;
            match open {
                Some(open) if state::crossed_boundary(&open, input) => {
                    let end = state::boundary(&open, input);
                    let result = self
                        .backend()
                        .batch(vec![
                            runtime::close_credential_quota_cycle(
                                open.id,
                                end,
                                QuotaCycleCloseReason::BoundaryCrossed,
                            )?,
                            runtime::insert_credential_quota_cycle(input)?,
                        ])
                        .await;
                    match result {
                        Ok(results) => {
                            let id = results
                                .get(1)
                                .and_then(|result| result.last_insert_id)
                                .ok_or_else(|| {
                                    StoreError::Database("cycle insert returned no id".into())
                                })?;
                            return self.require_credential_quota_cycle(id).await;
                        }
                        Err(error) if unique_conflict(&error) => continue,
                        Err(error) => return Err(error),
                    }
                }
                Some(open) => {
                    let result = self
                        .backend()
                        .execute(runtime::update_credential_quota_cycle(open.id, input)?)
                        .await?;
                    if result.affected_rows == 1 {
                        return self.require_credential_quota_cycle(open.id).await;
                    }
                }
                None => {
                    match self
                        .backend()
                        .execute(runtime::insert_credential_quota_cycle(input)?)
                        .await
                    {
                        Ok(result) => {
                            let id = result.last_insert_id.ok_or_else(|| {
                                StoreError::Database("cycle insert returned no id".into())
                            })?;
                            return self.require_credential_quota_cycle(id).await;
                        }
                        Err(error) if unique_conflict(&error) => continue,
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Err(StoreError::Database(
            "credential quota-cycle observation remained contended".into(),
        ))
    }

    pub async fn close_credential_quota_cycle(
        &self,
        id: i64,
        reason: QuotaCycleCloseReason,
        closed_at: i64,
    ) -> Result<Option<CredentialQuotaCycleRecord>, StoreError> {
        self.backend()
            .execute(runtime::close_credential_quota_cycle(
                id, closed_at, reason,
            )?)
            .await?;
        self.credential_quota_cycle(id).await
    }
}

fn unique_conflict(error: &StoreError) -> bool {
    matches!(error, StoreError::Database(message) if message.to_ascii_lowercase().contains("unique"))
}
