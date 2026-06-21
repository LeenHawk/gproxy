//! §15.3 metrics aggregate for the `file` backend — loads the relevant JSON
//! tables and aggregates in memory (dev-scale; the file backend is
//! single-instance and not the production hot path). Mirrors the `db` backend's
//! semantics: token/request totals from `hour` rollups, latency histogram over
//! measured (`> 0`) `usages.latency_ms`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::store::persistence::metrics::{LATENCY_BUCKETS_MS, MetricsAggregate, QuotaUsage};
use crate::store::persistence::records::{CredentialStatus, Quota, Usage, UsageRollup};

use super::table;

pub(crate) async fn aggregate(root: &Path) -> anyhow::Result<MetricsAggregate> {
    let mut out = MetricsAggregate::default();

    // token / request totals (hour rollups — a complete cover, no double count)
    let rollups = table::load::<UsageRollup>(&root.join("usage_rollups.json")).await?;
    for r in rollups.rows.iter().filter(|r| r.granularity == "hour") {
        out.requests_total += r.requests;
        out.input_tokens_total += r.input_tokens;
        out.output_tokens_total += r.output_tokens;
    }

    // upstream-latency histogram (cumulative buckets + sum + count)
    let usages = table::load::<Usage>(&root.join("usages.json")).await?;
    out.latency_buckets = vec![0; LATENCY_BUCKETS_MS.len()];
    for u in usages.rows.iter().filter(|u| u.latency_ms > 0) {
        out.latency_count += 1;
        out.latency_sum_ms += u.latency_ms;
        for (i, le) in LATENCY_BUCKETS_MS.iter().enumerate() {
            if u.latency_ms <= *le {
                out.latency_buckets[i] += 1;
            }
        }
    }

    // credential health counts by kind (BTreeMap → stable ordering)
    let statuses = table::load::<CredentialStatus>(&root.join("credential_statuses.json")).await?;
    let mut by_kind: BTreeMap<String, i64> = BTreeMap::new();
    for s in &statuses.rows {
        *by_kind.entry(s.health_kind.clone()).or_default() += 1;
    }
    out.credential_health = by_kind.into_iter().collect();

    // per-scope quota gauges
    let quotas = table::load::<Quota>(&root.join("quotas.json")).await?;
    out.quota = quotas
        .rows
        .iter()
        .map(|q| QuotaUsage {
            scope: q.scope.as_str().to_owned(),
            scope_id: q.scope_id,
            total: q.quota_total,
            used: q.cost_used,
        })
        .collect();

    Ok(out)
}
