mod accounting;
mod boundary;
mod close;
mod links;
mod metrics;
mod models;
mod observations;
mod read;
mod rejected;
mod row;
mod state;

use crate::query::runtime;
use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaCoverage, QuotaCycleCloseReason,
    QuotaCycleStatus,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn observe_credential_quota_cycle(
        &self,
        observation: &CredentialQuotaObservation,
    ) -> Result<CredentialQuotaCycleRecord, StoreError> {
        let raw = observation;
        let observation = &state::settle(raw)?;
        for _ in 0..8 {
            let mut input = observation.clone();
            let sample = state::sample(&input);
            let open = self
                .open_credential_quota_cycle(input.credential_id, &input.window_key)
                .await?;
            if let Some(mut open) = open {
                let previous_sample = open.tracking.sample;
                let same_sample = sample.started_at_ms == previous_sample.started_at_ms
                    && sample.received_at_ms == previous_sample.received_at_ms;
                if sample.received_at_ms < previous_sample.received_at_ms {
                    if self.record_rejected_quota(&mut open, raw).await? {
                        return Ok(open);
                    }
                    continue;
                }
                input.label = input.label.or_else(|| open.label.clone());
                input.period_start = input.period_start.or(open.period_start);
                input.period_end = input.period_end.or(open.period_end);
                if open.boundary_source == crate::records::QuotaBoundarySource::Upstream
                    && input.boundary_source == crate::records::QuotaBoundarySource::Inferred
                {
                    input.period_start = open.period_start;
                    input.period_end = open.period_end;
                }
                state::hold_boundary(&open, &mut input);
                let changed = state::changed(&open, &input);
                let decreased = state::decreased(&open, &input);
                if (changed || decreased || state::adjusted(&open, &input))
                    && (same_sample || sample.started_at_ms < previous_sample.received_at_ms)
                {
                    open.tracking.uncertain = true;
                    if open.tracking.pending_observation.is_none() {
                        open.tracking.pending_observation = Some(input.clone());
                    }
                    let expected = open.version;
                    open.version += 1;
                    if self
                        .backend()
                        .batch(vec![
                            runtime::update_tracked_cycle(&open, expected)?,
                            runtime::insert_cycle_observation(&open, raw, true)?,
                        ])
                        .await?[0]
                        .affected_rows
                        == 1
                    {
                        return Ok(open);
                    }
                    continue;
                }
                if same_sample {
                    return Ok(open);
                }
                if open
                    .tracking
                    .pending_observation
                    .as_ref()
                    .is_some_and(|pending| sample.started_at_ms < pending.sample.received_at_ms)
                {
                    if self.record_rejected_quota(&mut open, raw).await? {
                        return Ok(open);
                    }
                    continue;
                }
                if changed || decreased {
                    let (at, local) = boundary::transition(&open, &input, changed);
                    let expected = open.version;
                    open.version += 1;
                    open.status = QuotaCycleStatus::Closed;
                    open.accounting_end_ms = Some(at);
                    open.tracking.needs_rebuild = true;
                    open.close_reason = Some(if changed {
                        QuotaCycleCloseReason::BoundaryCrossed
                    } else {
                        QuotaCycleCloseReason::UsageDecreased
                    });
                    let next = new_cycle(&input, Some(at), local, Some(&open));
                    let result = self
                        .backend()
                        .batch(vec![
                            runtime::update_tracked_cycle(&open, expected)?,
                            runtime::insert_tracked_cycle(&next, Some(&open))?,
                            runtime::insert_cycle_observation(&next, raw, false)?,
                        ])
                        .await;
                    match result {
                        Ok(results)
                            if results[0].affected_rows == 1 && results[1].affected_rows == 1 =>
                        {
                            return self
                                .repair_and_read(input.credential_id, &input.window_key)
                                .await;
                        }
                        Ok(_) => continue,
                        Err(error) if constraint_conflict(&error) => continue,
                        Err(error) => return Err(error),
                    }
                }
                let expected = open.version;
                let adjusted = state::adjusted(&open, &input);
                let mut tracking = if adjusted {
                    state::tracking(&input, open.tracking.local_boundary)
                } else {
                    open.tracking.clone()
                };
                tracking.sample = sample;
                tracking.uncertain = false;
                tracking.pending_observation = None;
                let start = open.accounting_start_ms;
                let end = open
                    .accounting_end_ms
                    .or(input.period_end.map(|end| end * 1000));
                tracking.needs_rebuild |=
                    open.accounting_start_ms != start || open.accounting_end_ms != end;
                open.accounting_start_ms = start;
                open.accounting_end_ms = end;
                open.tracking = tracking;
                open.version += 1;
                open.period_start = input.period_start;
                open.period_end = input.period_end;
                open.label = input.label;
                open.last_observed_at = input.observed_at;
                open.upstream_used = input.upstream_used;
                open.upstream_limit = input.upstream_limit;
                open.used_percent = input.used_percent;
                if self
                    .backend()
                    .batch(vec![
                        runtime::update_tracked_cycle(&open, expected)?,
                        runtime::insert_cycle_observation(&open, raw, false)?,
                    ])
                    .await?[0]
                    .affected_rows
                    == 1
                {
                    if open.tracking.needs_rebuild {
                        return self
                            .repair_and_read(input.credential_id, &input.window_key)
                            .await;
                    }
                    return Ok(open);
                }
            } else {
                let latest = self
                    .latest_credential_quota_cycle(input.credential_id, &input.window_key)
                    .await?;
                if let Some(mut latest) = latest.clone() {
                    if latest.close_reason != Some(QuotaCycleCloseReason::ManualReset) {
                        state::hold_boundary(&latest, &mut input);
                    }
                    let cutoff = latest
                        .accounting_end_ms
                        .unwrap_or(latest.last_observed_at * 1000);
                    if input.period_end.is_some_and(|end| end * 1000 <= cutoff)
                        || sample.received_at_ms < cutoff
                        || sample.started_at_ms < latest.tracking.sample.started_at_ms
                    {
                        if self.record_rejected_quota(&mut latest, raw).await? {
                            return Ok(latest);
                        }
                        continue;
                    }
                    if latest.period_end == input.period_end
                        && !state::decreased(&latest, &input)
                        && !state::changed(&latest, &input)
                    {
                        if self.record_rejected_quota(&mut latest, raw).await? {
                            return Ok(latest);
                        }
                        continue;
                    }
                }
                let start = latest
                    .as_ref()
                    .and_then(|previous| previous.accounting_end_ms)
                    .map(|cutoff| {
                        input
                            .period_start
                            .map(|start| start * 1000)
                            .unwrap_or(cutoff)
                            .max(cutoff)
                    });
                let next = new_cycle(&input, start, false, latest.as_ref());
                match self
                    .backend()
                    .batch(vec![
                        runtime::insert_tracked_cycle(&next, latest.as_ref())?,
                        runtime::insert_cycle_observation(&next, raw, false)?,
                    ])
                    .await
                {
                    Ok(result) if result[0].affected_rows == 1 => {
                        return self
                            .repair_and_read(input.credential_id, &input.window_key)
                            .await;
                    }
                    Ok(_) => continue,
                    Err(error) if constraint_conflict(&error) => continue,
                    Err(error) => return Err(error),
                }
            }
        }
        Err(StoreError::Database(
            "credential cycle remained contended".into(),
        ))
    }

    async fn repair_and_read(
        &self,
        credential: i64,
        window: &str,
    ) -> Result<CredentialQuotaCycleRecord, StoreError> {
        self.repair_cycle_links(credential, window).await?;
        self.latest_credential_quota_cycle(credential, window)
            .await?
            .ok_or_else(|| StoreError::Database("quota cycle disappeared".into()))
    }
}

fn new_cycle(
    input: &CredentialQuotaObservation,
    start: Option<i64>,
    local: bool,
    previous: Option<&CredentialQuotaCycleRecord>,
) -> CredentialQuotaCycleRecord {
    let sample = state::sample(input);
    let start = start
        .or(input.period_start.map(|start| start * 1000))
        .unwrap_or(sample.received_at_ms);
    CredentialQuotaCycleRecord {
        id: 0,
        version: 1,
        credential_id: input.credential_id,
        window_key: input.window_key.clone(),
        label: input.label.clone(),
        period_start: input.period_start,
        period_end: input.period_end,
        boundary_source: input.boundary_source,
        boundary_confidence: input.boundary_confidence,
        status: QuotaCycleStatus::Open,
        close_reason: None,
        last_observed_at: input.observed_at,
        upstream_used: input.upstream_used,
        upstream_limit: input.upstream_limit,
        used_percent: input.used_percent,
        coverage: if input.scope == gproxy_core::QuotaScope::Unknown {
            QuotaCoverage::Unknown
        } else if previous.is_some_and(|previous| {
            previous.accounting_end_ms == Some(start)
                && previous.close_reason == Some(QuotaCycleCloseReason::BoundaryCrossed)
        }) {
            QuotaCoverage::FullPeriodLowerBound
        } else {
            QuotaCoverage::PartialLowerBound
        },
        metrics: serde_json::json!({}),
        models: Vec::new(),
        accounting_start_ms: start,
        accounting_end_ms: input
            .period_end
            .map(|end| end * 1000)
            .filter(|end| *end > start),
        tracking: state::tracking(input, local),
        estimate: None,
    }
}

fn constraint_conflict(error: &StoreError) -> bool {
    match error {
        StoreError::Database(message) => {
            let message = message.to_ascii_lowercase();
            message.contains("unique")
                || message.contains("duplicate")
                || message.contains("locked")
        }
        _ => false,
    }
}
