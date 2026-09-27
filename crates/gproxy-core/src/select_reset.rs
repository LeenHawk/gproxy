//! Reset ordering uses observed quota readings, never a quota query on the request path.
//! Cache only the latest readings, independent of the requested model/operation; a
//! cold cache is rebuilt from the credentials' open cycles with observed boundaries.

use std::{collections::HashMap, sync::Arc, time::Duration};

use gproxy_channel::channel::{
    QuotaAllowance, QuotaEntry, QuotaScope, QuotaSubject, QuotaValue, QuotaWindow,
};
use gproxy_protocol::Operation;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::limits::credential_cycle::CycleBoundary;
use serde::{Deserialize, Serialize};

use crate::{Core, CoreError, CoreResult, CredentialBlocks, CredentialData, keys};

#[derive(Serialize, Deserialize)]
struct Observation {
    id: String,
    observed_at_ms: i64,
    source_id: String,
    scope: QuotaScope,
    resets_at_ms: Option<i64>,
    kind: String,
    unlimited: bool,
}

impl Observation {
    /// The persisted fields of the observation, in the shape the channel's
    /// classification rule reads.
    fn entry(&self) -> QuotaEntry {
        let allowance = QuotaAllowance {
            period_end_ms: self.resets_at_ms,
            unlimited: self.unlimited.then_some(true),
            ..QuotaAllowance::default()
        };
        QuotaEntry {
            id: self.id.clone(),
            source_id: self.source_id.clone(),
            label: None,
            subject: QuotaSubject::Unknown,
            model_scope: self.scope.clone(),
            value: match self.kind.as_str() {
                "budget" => QuotaValue::Budget(allowance),
                "rate_limit" => QuotaValue::RateLimit(allowance),
                _ => QuotaValue::Window(allowance),
            },
        }
    }
}

/// Refresh an existing projection after observations are persisted. CAS
/// merges concurrent responses without losing a different quota window.
pub(crate) async fn update_reset_observations(
    cache: &dyn gproxy_cache::Cache,
    credential_id: &str,
    entries: &[QuotaEntry],
    now_ms: i64,
) -> CoreResult<()> {
    let key = keys::credential_reset_observations(credential_id);
    for _ in 0..3 {
        let Some(cached) = cache.get(&key).await? else {
            return Ok(());
        };
        let Ok(mut rows) = serde_json::from_slice::<Vec<Observation>>(&cached.value) else {
            cache.delete(&key).await?;
            return Ok(());
        };
        for entry in entries {
            if rows
                .iter()
                .any(|r| r.id == entry.id && r.observed_at_ms > now_ms)
            {
                continue;
            }
            rows.retain(|r| r.id != entry.id);
            let (kind, allowance) = match &entry.value {
                QuotaValue::Window(a) => ("window", Some(a)),
                QuotaValue::Budget(a) => ("budget", Some(a)),
                QuotaValue::RateLimit(a) => ("rate_limit", Some(a)),
                QuotaValue::Balance(_) => ("balance", None),
                QuotaValue::Breakdown(_) => ("breakdown", None),
            };
            rows.push(Observation {
                id: entry.id.clone(),
                observed_at_ms: now_ms,
                source_id: entry.source_id.clone(),
                scope: entry.model_scope.clone(),
                resets_at_ms: allowance.and_then(|a| a.period_end_ms),
                kind: kind.into(),
                unlimited: allowance.is_some_and(|a| a.unlimited == Some(true)),
            });
        }
        let outcome = cache
            .compare_exchange(
                &key,
                Some(cached.version),
                Some(gproxy_cache::Replacement {
                    value: serde_json::to_vec(&rows)
                        .map_err(|e| CoreError::Rewrite(e.to_string()))?,
                    ttl: Duration::from_secs(30),
                }),
            )
            .await?;
        if matches!(outcome, gproxy_cache::CasOutcome::Applied(_)) {
            return Ok(());
        }
    }
    // Contended projections are rebuilt from the durable observations.
    cache.delete(&key).await?;
    Ok(())
}

impl<C: BatchConnectionTrait> Core<C> {
    pub(crate) async fn credential_reset_times(
        &self,
        eligible: &[(Arc<CredentialData>, CredentialBlocks)],
        model: Option<&str>,
        operation: Operation,
        now_ms: i64,
    ) -> CoreResult<HashMap<String, i64>> {
        let mut observations: HashMap<String, Vec<Observation>> = HashMap::new();
        let mut missing = Vec::new();
        for (credential, _) in eligible {
            let cached = self
                .cache
                .get(&keys::credential_reset_observations(&credential.id))
                .await?
                .and_then(|entry| serde_json::from_slice(&entry.value).ok());
            match cached {
                Some(rows) => {
                    observations.insert(credential.id.clone(), rows);
                }
                None => {
                    missing.push(credential.id.clone());
                }
            }
        }
        if !missing.is_empty() {
            // A cold projection is rebuilt from the open cycles, not the
            // observation log: one indexed read per credential set, where the
            // log would be a scan of its whole history.
            let rows = self.store().credential_cycles().open_of(&missing).await?;
            for id in &missing {
                observations.insert(id.clone(), Vec::new());
            }
            for row in rows {
                // Only boundaries the upstream reported order by reset. A
                // local one is a guess, and a cycle goes back to local when
                // the upstream stops reporting its reset.
                if row.boundary != CycleBoundary::Observed {
                    continue;
                }
                let Some(source_id) = row.dimension_id else {
                    continue;
                };
                let scope = serde_json::from_value(row.scope).unwrap_or(QuotaScope::Unknown);
                observations
                    .entry(row.credential_id)
                    .or_default()
                    .push(Observation {
                        id: row.window_id,
                        observed_at_ms: row.sample_at_ms.unwrap_or(row.starts_at_ms),
                        source_id,
                        scope,
                        resets_at_ms: row.ends_at_ms,
                        kind: "window".into(),
                        unlimited: false,
                    });
            }
            for id in missing {
                self.cache
                    .put(
                        &keys::credential_reset_observations(&id),
                        serde_json::to_vec(&observations[&id])
                            .map_err(|e| CoreError::Rewrite(e.to_string()))?,
                        Duration::from_secs(30),
                    )
                    .await?;
            }
        }
        let data = self.snapshot();
        let mut resets = HashMap::new();
        for (credential, _) in eligible {
            let quota_model = crate::quota::quota_model(&data, credential);
            for row in &observations[&credential.id] {
                if row.unlimited || !matches!(row.kind.as_str(), "window" | "budget") {
                    continue;
                }
                let Some(reset) = row.resets_at_ms.filter(|r| *r > now_ms) else {
                    continue;
                };
                let Some(dimension) = crate::quota::classify(quota_model, credential, &row.entry())
                else {
                    continue;
                };
                if matches!(dimension.window, QuotaWindow::Total)
                    || dimension
                        .operations
                        .as_ref()
                        .is_some_and(|ops| !ops.contains(&operation))
                {
                    continue;
                }
                let scope = match &dimension.scope {
                    QuotaScope::Unknown => &row.scope,
                    scope => scope,
                };
                if !matches!(scope, QuotaScope::All)
                    && !model.is_some_and(|model| scope.matches(model))
                {
                    continue;
                }
                // A scoped observation can narrow a broadly declared dimension.
                if !matches!(row.scope, QuotaScope::All | QuotaScope::Unknown)
                    && !model.is_some_and(|model| row.scope.matches(model))
                {
                    continue;
                }
                resets
                    .entry(credential.id.clone())
                    .and_modify(|end: &mut i64| *end = (*end).min(reset))
                    .or_insert(reset);
            }
        }
        Ok(resets)
    }
}
