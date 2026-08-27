use crate::query::runtime;
use crate::records::{CredentialQuotaCycleRecord, QuotaCycleCloseReason, QuotaCycleStatus};
use crate::{Store, StoreError};

use super::{metrics, persist};

impl Store {
    pub async fn close_credential_quota_cycle(
        &self,
        id: i64,
        reason: QuotaCycleCloseReason,
        closed_at: i64,
    ) -> Result<Option<CredentialQuotaCycleRecord>, StoreError> {
        const RETRIES: usize = 8;
        for _ in 0..RETRIES {
            let Some(cycle) = self.credential_quota_cycle(id).await? else {
                return Ok(None);
            };
            if cycle.status == QuotaCycleStatus::Closed {
                return Ok(Some(cycle));
            }
            validate_time(&cycle, closed_at)?;
            let metrics =
                metrics::collect_range(self, cycle.credential_id, cycle.period_start, closed_at)
                    .await?;
            let statement = runtime::close_credential_quota_cycle(
                &cycle,
                closed_at,
                cycle.boundary_source,
                cycle.boundary_confidence,
                reason,
                None,
                &metrics.totals,
            )?;
            let result = persist::known(
                self,
                statement,
                cycle.id,
                cycle.version.saturating_add(1),
                &metrics,
            )
            .await?;
            if result.affected_rows == 1 {
                return self.credential_quota_cycle(id).await;
            }
        }
        Err(StoreError::Database(
            "credential quota-cycle close remained contended".into(),
        ))
    }
}

fn validate_time(cycle: &CredentialQuotaCycleRecord, closed_at: i64) -> Result<(), StoreError> {
    if cycle.period_start.is_some_and(|start| closed_at < start) {
        return Err(StoreError::InvalidData {
            field: "closed_at",
            message: "must not precede period_start".into(),
        });
    }
    if closed_at < cycle.last_observed_at {
        return Err(StoreError::InvalidData {
            field: "closed_at",
            message: "must not precede last_observed_at".into(),
        });
    }
    Ok(())
}
