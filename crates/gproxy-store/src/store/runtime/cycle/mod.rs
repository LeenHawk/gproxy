mod boundary;
mod close;
mod metrics;
mod models;
mod persist;
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
                // A header-borne observation may omit the display label the
                // probe recorded; absence is not removal.
                if input.label.is_none() {
                    input.label = open.label.clone();
                }
            }
            match open {
                Some(open) if state::stale_open(&open, &input) => return Ok(open),
                Some(open) if state::crossed_boundary(&open, &input) => {
                    let Some(boundary) = boundary::resolve(&open, &input) else {
                        state::retain_cycle_boundary(&open, &mut input);
                        state::merge_same_second(&open, &mut input);
                        let metrics = metrics::collect(self, &input).await?;
                        let statement = runtime::update_credential_quota_cycle(
                            &open,
                            &input,
                            state::update_coverage(&open, &input),
                            &metrics.totals,
                        )?;
                        let result = persist::known(
                            self,
                            statement,
                            open.id,
                            open.version.saturating_add(1),
                            &metrics,
                        )
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
                    let statement = runtime::close_credential_quota_cycle(
                        &open,
                        boundary.at,
                        boundary.source,
                        boundary.confidence,
                        QuotaCycleCloseReason::BoundaryCrossed,
                        Some(input.observed_at),
                        &metrics.totals,
                    )?;
                    let result = persist::known(
                        self,
                        statement,
                        open.id,
                        open.version.saturating_add(1),
                        &metrics,
                    )
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
                    let statement = runtime::update_credential_quota_cycle(
                        &open,
                        &input,
                        state::update_coverage(&open, &input),
                        &metrics.totals,
                    )?;
                    let result = persist::known(
                        self,
                        statement,
                        open.id,
                        open.version.saturating_add(1),
                        &metrics,
                    )
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
                    let statement = runtime::insert_credential_quota_cycle(
                        &input,
                        state::new_coverage(latest.as_ref(), &input),
                        &metrics.totals,
                    )?;
                    match persist::insert(self, statement, &input, &metrics).await {
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
}

fn unique_conflict(error: &StoreError) -> bool {
    matches!(error, StoreError::Database(message) if message.to_ascii_lowercase().contains("unique"))
}
