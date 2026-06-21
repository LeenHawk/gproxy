//! §15.3 metrics aggregate for the `db` backend — backend-side SUM/COUNT/CASE
//! queries (never an in-memory counter). Token/request totals come from the
//! `hour` rollup granularity (a complete cover; summing both hour+day would
//! double-count); the latency histogram buckets `usages.latency_ms` (measured
//! rows only, `> 0`).

use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};

use crate::store::persistence::metrics::{LATENCY_BUCKETS_MS, MetricsAggregate, QuotaUsage};

pub async fn aggregate(conn: &DatabaseConnection) -> anyhow::Result<MetricsAggregate> {
    let backend = conn.get_database_backend();
    let stmt = |sql: String| Statement::from_string(backend, sql);

    // ── token / request totals (hour rollups) ──
    let mut out = MetricsAggregate::default();
    if let Some(row) = conn
        .query_one_raw(stmt(
            "SELECT COALESCE(SUM(requests),0) AS r, COALESCE(SUM(input_tokens),0) AS i, \
             COALESCE(SUM(output_tokens),0) AS o FROM usage_rollups WHERE granularity='hour'"
                .into(),
        ))
        .await?
    {
        out.requests_total = row.try_get::<i64>("", "r")?;
        out.input_tokens_total = row.try_get::<i64>("", "i")?;
        out.output_tokens_total = row.try_get::<i64>("", "o")?;
    }

    // ── upstream-latency histogram (cumulative buckets + sum + count) ──
    let buckets_sql: String = LATENCY_BUCKETS_MS
        .iter()
        .map(|le| format!("SUM(CASE WHEN latency_ms<={le} THEN 1 ELSE 0 END) AS b{le}"))
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(row) = conn
        .query_one_raw(stmt(format!(
            "SELECT {buckets_sql}, COALESCE(SUM(latency_ms),0) AS s, COUNT(*) AS c \
             FROM usages WHERE latency_ms > 0"
        )))
        .await?
    {
        out.latency_buckets = LATENCY_BUCKETS_MS
            .iter()
            .map(|le| row.try_get::<i64>("", &format!("b{le}")).unwrap_or(0))
            .collect();
        out.latency_sum_ms = row.try_get::<i64>("", "s").unwrap_or(0);
        out.latency_count = row.try_get::<i64>("", "c").unwrap_or(0);
    }

    // ── credential health counts by kind ──
    for row in conn
        .query_all_raw(stmt(
            "SELECT health_kind AS k, COUNT(*) AS n FROM credential_statuses GROUP BY health_kind"
                .into(),
        ))
        .await?
    {
        out.credential_health.push((
            row.try_get::<String>("", "k")?,
            row.try_get::<i64>("", "n")?,
        ));
    }

    // ── per-scope quota gauges ──
    for row in conn
        .query_all_raw(stmt(
            "SELECT scope AS s, scope_id AS sid, quota_total AS qt, cost_used AS cu FROM quotas"
                .into(),
        ))
        .await?
    {
        out.quota.push(QuotaUsage {
            scope: row.try_get::<String>("", "s")?,
            scope_id: row.try_get::<i64>("", "sid")?,
            total: row.try_get::<String>("", "qt")?.parse().unwrap_or_default(),
            used: row.try_get::<String>("", "cu")?.parse().unwrap_or_default(),
        });
    }

    Ok(out)
}
