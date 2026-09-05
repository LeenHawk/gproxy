use crate::query::runtime;
use crate::records::{CredentialQuotaCycleRecord, QuotaCycleCloseReason, QuotaCycleStatus};
use crate::{Store, StoreError};

impl Store {
    pub async fn close_credential_quota_cycle(
        &self,
        id: i64,
        reason: QuotaCycleCloseReason,
        closed_at: i64,
    ) -> Result<Option<CredentialQuotaCycleRecord>, StoreError> {
        for _ in 0..8 {
            let Some(mut cycle) = self.credential_quota_cycle(id).await? else {
                return Ok(None);
            };
            if cycle.status == QuotaCycleStatus::Closed {
                return Ok(Some(cycle));
            }
            let cutoff = closed_at * 1000;
            if cutoff < cycle.accounting_start_ms || closed_at < cycle.last_observed_at {
                return Err(StoreError::InvalidData {
                    field: "closed_at",
                    message: "must not precede the cycle or its last observation".into(),
                });
            }
            let expected = cycle.version;
            cycle.version += 1;
            cycle.accounting_end_ms = Some(cutoff);
            cycle.status = QuotaCycleStatus::Closed;
            cycle.close_reason = Some(reason);
            if self
                .backend()
                .execute(runtime::update_tracked_cycle(&cycle, expected)?)
                .await?
                .affected_rows
                == 1
            {
                return self.credential_quota_cycle(id).await;
            }
        }
        Err(StoreError::Database(
            "credential cycle close remained contended".into(),
        ))
    }
}
