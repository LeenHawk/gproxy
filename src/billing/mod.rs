//! §17 billing: request_id-idempotent usage records + rollups; failed
//! attempts are audit-only (`upstream_requests`), never billed.

pub mod pending;
pub mod price;

use rust_decimal::Decimal;

use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{UpstreamRequest, UpstreamRequestInput, UsageInput};
use crate::usage::{Ended, NormalizedUsage, UsageSource};

/// Everything needed to settle one successful (possibly interrupted) request.
pub struct UsageRecord<'a> {
    pub request_id: &'a str,
    /// Unix seconds.
    pub at: i64,
    pub route_name: Option<&'a str>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: &'a str,
    pub kind: &'a str,
    pub model: Option<&'a str>,
    pub usage: &'a NormalizedUsage,
    pub cost: Decimal,
    /// §15.3: upstream latency (ms) of the settled attempt; 0 when unmeasured.
    pub latency_ms: i64,
    pub source: UsageSource,
    pub ended: Ended,
}

/// One failed failover attempt: audit-only, never billed.
pub struct FailureRecord<'a> {
    pub request_id: &'a str,
    /// Unix seconds.
    pub at: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub url: &'a str,
    pub method: &'a str,
    pub status: i64,
    pub latency_ms: i64,
    pub error: &'a str,
}

fn tok(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// Record a settled request: append the usage row (idempotent by
/// `request_id`) and, on FIRST insert only, accumulate hour+day rollups.
/// Returns `false` when this `request_id` was already settled (no-op).
pub async fn record_success(
    db: &dyn PersistenceBackend,
    rec: UsageRecord<'_>,
) -> anyhow::Result<bool> {
    let input = UsageInput {
        request_id: rec.request_id.to_owned(),
        at: rec.at,
        route_name: rec.route_name.map(str::to_owned),
        provider_id: rec.provider_id,
        credential_id: rec.credential_id,
        org_id: rec.org_id,
        team_id: rec.team_id,
        user_id: rec.user_id,
        user_key_id: rec.user_key_id,
        operation: rec.operation.to_owned(),
        kind: rec.kind.to_owned(),
        model: rec.model.map(str::to_owned),
        input_tokens: tok(rec.usage.input),
        output_tokens: tok(rec.usage.output),
        cache_read_tokens: tok(rec.usage.cache_read),
        // NormalizedUsage carries the summed cache-creation total; the 5m/1h
        // split is not preserved post-normalization — record under 5m.
        cache_creation_5m_tokens: tok(rec.usage.cache_creation),
        cache_creation_1h_tokens: 0,
        cost: rec.cost,
        latency_ms: rec.latency_ms,
        usage_source: rec.source.as_str().to_owned(),
        ended: rec.ended.as_str().to_owned(),
    };
    if db.append_usage(input).await?.is_none() {
        return Ok(false); // duplicate settle — no rollup
    }

    for (granularity, bucket) in [("hour", 3600i64), ("day", 86_400i64)] {
        db.add_usage_rollup(crate::store::persistence::records::UsageRollupInput {
            granularity: granularity.to_owned(),
            bucket_start: rec.at - rec.at.rem_euclid(bucket),
            provider_id: rec.provider_id,
            org_id: rec.org_id,
            team_id: rec.team_id,
            user_id: rec.user_id,
            route_name: rec.route_name.map(str::to_owned),
            model: rec.model.map(str::to_owned),
            requests: 1,
            input_tokens: tok(rec.usage.input),
            output_tokens: tok(rec.usage.output),
            cost: rec.cost,
        })
        .await?;
    }
    Ok(true)
}

/// Record a failed failover attempt as an `upstream_requests` audit row.
pub async fn record_failure(
    db: &dyn PersistenceBackend,
    rec: FailureRecord<'_>,
) -> anyhow::Result<UpstreamRequest> {
    db.append_upstream_request(UpstreamRequestInput {
        request_id: rec.request_id.to_owned(),
        at: rec.at,
        provider_id: rec.provider_id,
        credential_id: rec.credential_id,
        url: rec.url.to_owned(),
        method: rec.method.to_owned(),
        status: rec.status,
        latency_ms: rec.latency_ms,
        headers_json: None,
        body: Some(rec.error.to_owned()),
        response_body: None,
    })
    .await
}

#[cfg(all(test, feature = "persist-file", not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::store::persistence::FilePersistence;

    #[tokio::test]
    async fn record_success_is_idempotent_by_request_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = FilePersistence::open(dir.path().to_path_buf())
            .await
            .expect("open");

        let usage = NormalizedUsage {
            input: 1500,
            output: 100,
            ..Default::default()
        };
        let rec = || UsageRecord {
            request_id: "req-1",
            at: 7200,
            route_name: Some("default"),
            provider_id: Some(1),
            credential_id: Some(1),
            org_id: None,
            team_id: None,
            user_id: Some(9),
            user_key_id: None,
            operation: "messages",
            kind: "claude_messages",
            model: Some("claude-x"),
            usage: &usage,
            cost: "0.0045".parse().unwrap(),
            latency_ms: 123,
            source: UsageSource::Upstream,
            ended: Ended::Complete,
        };

        assert!(record_success(&db, rec()).await.expect("first"));
        assert!(!record_success(&db, rec()).await.expect("second"));

        let rows = db.list_usages(10).await.expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].usage_source, "upstream");
        assert_eq!(rows[0].ended, "complete");

        // Rollups counted exactly once per granularity.
        for gran in ["hour", "day"] {
            let rollups = db
                .list_usage_rollups(gran, 0, i64::MAX, None)
                .await
                .expect("rollups");
            assert_eq!(rollups.len(), 1, "{gran}");
            assert_eq!(rollups[0].requests, 1, "{gran}");
            assert_eq!(rollups[0].input_tokens, 1500, "{gran}");
        }
    }
}
