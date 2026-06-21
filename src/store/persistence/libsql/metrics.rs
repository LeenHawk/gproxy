//! §15.3 metrics aggregate for the libSQL edge backend — backend-side
//! SUM/COUNT/CASE queries (never an in-memory counter), mirroring
//! `db/ops/metrics.rs`. Token/request totals come from `granularity='hour'`
//! rollups; the latency histogram buckets `usages.latency_ms > 0`.

use crate::store::libsql::LibsqlClient;
use crate::store::persistence::libsql::row::{col_decimal, col_i64, col_str};
use crate::store::persistence::libsql::util::{query, query_one};
use crate::store::persistence::metrics::{LATENCY_BUCKETS_MS, MetricsAggregate, QuotaUsage};

pub async fn aggregate(client: &LibsqlClient) -> anyhow::Result<MetricsAggregate> {
    let mut out = MetricsAggregate::default();

    // ── token / request totals (hour rollups) ──
    if let Some(row) = query_one(
        client,
        "SELECT COALESCE(SUM(requests),0), COALESCE(SUM(input_tokens),0), \
         COALESCE(SUM(output_tokens),0) FROM usage_rollups WHERE granularity='hour'",
        &[],
    )
    .await?
    {
        out.requests_total = col_i64(&row, 0)?;
        out.input_tokens_total = col_i64(&row, 1)?;
        out.output_tokens_total = col_i64(&row, 2)?;
    }

    // ── upstream-latency histogram (cumulative buckets + sum + count) ──
    let buckets_sql: String = LATENCY_BUCKETS_MS
        .iter()
        .map(|le| format!("SUM(CASE WHEN latency_ms<={le} THEN 1 ELSE 0 END)"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {buckets_sql}, COALESCE(SUM(latency_ms),0), COUNT(*) \
         FROM usages WHERE latency_ms > 0"
    );
    if let Some(row) = query_one(client, &sql, &[]).await? {
        let n = LATENCY_BUCKETS_MS.len();
        out.latency_buckets = (0..n).map(|i| col_i64(&row, i).unwrap_or(0)).collect();
        out.latency_sum_ms = col_i64(&row, n).unwrap_or(0);
        out.latency_count = col_i64(&row, n + 1).unwrap_or(0);
    }

    // ── credential health counts by kind ──
    for row in query(
        client,
        "SELECT health_kind, COUNT(*) FROM credential_statuses GROUP BY health_kind",
        &[],
    )
    .await?
    {
        out.credential_health
            .push((col_str(&row, 0)?, col_i64(&row, 1)?));
    }

    // ── per-scope quota gauges ──
    for row in query(
        client,
        "SELECT scope, scope_id, quota_total, cost_used FROM quotas",
        &[],
    )
    .await?
    {
        out.quota.push(QuotaUsage {
            scope: col_str(&row, 0)?,
            scope_id: col_i64(&row, 1)?,
            total: col_decimal(&row, 2).unwrap_or_default(),
            used: col_decimal(&row, 3).unwrap_or_default(),
        });
    }

    Ok(out)
}
