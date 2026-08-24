mod boundary;
mod metrics;
mod read;
mod row;
mod state;

use crate::query::runtime;
use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaCycleCloseReason, QuotaCycleStatus,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn observe_credential_quota_cycle(
        &self,
        observation: &CredentialQuotaObservation,
    ) -> Result<CredentialQuotaCycleRecord, StoreError> {
        state::validate(observation)?;
        let mut continuation_start = None;
        const RETRIES: usize = 8;
        for _ in 0..RETRIES {
            let mut input = observation.clone();
            if let Some(start) = continuation_start {
                input.period_start = Some(start);
            }
            let open = self
                .open_credential_quota_cycle(input.credential_id, &input.window_key)
                .await?;
            if let Some(open) = open.as_ref() {
                state::preserve_cycle_bounds(open, &mut input);
                state::validate(&input)?;
            }
            match open {
                Some(open) if state::stale_open(&open, &input) => return Ok(open),
                Some(open) if state::crossed_boundary(&open, &input) => {
                    let Some(boundary) = boundary::resolve(&open, &input) else {
                        state::retain_cycle_boundary(&open, &mut input);
                        state::merge_same_second(&open, &mut input);
                        let metrics = metrics::collect(self, &input).await?;
                        let result = self
                            .backend()
                            .execute(runtime::update_credential_quota_cycle(
                                &open,
                                &input,
                                state::update_coverage(&open, &input),
                                &metrics,
                            )?)
                            .await?;
                        if result.affected_rows == 1 {
                            return self.require_credential_quota_cycle(open.id).await;
                        }
                        continue;
                    };
                    let metrics = metrics::collect_range(
                        self,
                        open.credential_id,
                        open.period_start,
                        boundary.at,
                    )
                    .await?;
                    let result = self
                        .backend()
                        .execute(runtime::close_credential_quota_cycle(
                            &open,
                            boundary.at,
                            boundary.source,
                            boundary.confidence,
                            QuotaCycleCloseReason::BoundaryCrossed,
                            Some(input.observed_at),
                            &metrics,
                        )?)
                        .await;
                    match result {
                        Ok(result) if result.affected_rows == 1 => {
                            continuation_start = Some(
                                observation
                                    .period_start
                                    .filter(|start| *start >= boundary.at)
                                    .unwrap_or(boundary.at),
                            );
                            continue;
                        }
                        Ok(_) => continue,
                        Err(error) => return Err(error),
                    }
                }
                Some(open) => {
                    state::merge_same_second(&open, &mut input);
                    let metrics = metrics::collect(self, &input).await?;
                    let result = self
                        .backend()
                        .execute(runtime::update_credential_quota_cycle(
                            &open,
                            &input,
                            state::update_coverage(&open, &input),
                            &metrics,
                        )?)
                        .await?;
                    if result.affected_rows == 1 {
                        return self.require_credential_quota_cycle(open.id).await;
                    }
                }
                None => {
                    let latest = self
                        .latest_credential_quota_cycle(input.credential_id, &input.window_key)
                        .await?;
                    if let Some(latest) = latest.as_ref() {
                        state::continue_after_natural_close(latest, &mut input);
                    }
                    if let Some(latest) = latest.as_ref()
                        && latest.status == QuotaCycleStatus::Closed
                        && state::stale_after_close(latest, &input)
                    {
                        return Ok(latest.clone());
                    }
                    let metrics = metrics::collect(self, &input).await?;
                    match self
                        .backend()
                        .execute(runtime::insert_credential_quota_cycle(
                            &input,
                            state::new_coverage(latest.as_ref(), &input),
                            &metrics,
                        )?)
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
        const RETRIES: usize = 8;
        for _ in 0..RETRIES {
            let Some(cycle) = self.credential_quota_cycle(id).await? else {
                return Ok(None);
            };
            if cycle.status == QuotaCycleStatus::Closed {
                return Ok(Some(cycle));
            }
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
            let metrics =
                metrics::collect_range(self, cycle.credential_id, cycle.period_start, closed_at)
                    .await?;
            let result = self
                .backend()
                .execute(runtime::close_credential_quota_cycle(
                    &cycle,
                    closed_at,
                    cycle.boundary_source,
                    cycle.boundary_confidence,
                    reason,
                    None,
                    &metrics,
                )?)
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

fn unique_conflict(error: &StoreError) -> bool {
    matches!(error, StoreError::Database(message) if message.to_ascii_lowercase().contains("unique"))
}
