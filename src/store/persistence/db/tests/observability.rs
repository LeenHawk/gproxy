//! Metrics, usage summaries, request logs, and audit log tests.

use super::*;
use crate::store::persistence::traits::UsagePersistence;

#[tokio::test]
async fn metrics_aggregate_sums_rollups_and_buckets_latency() {
    let db = mem().await;
    // Two settled requests with measured latency (60ms, 600ms) + an hour rollup.
    for (rid, lat) in [("r1", 60i64), ("r2", 600)] {
        db.append_usage(UsageInput {
            request_id: rid.to_owned(),
            at: 100,
            route_name: None,
            provider_id: None,
            credential_id: None,
            org_id: None,
            team_id: None,
            user_id: None,
            user_key_id: None,
            operation: "chat".into(),
            kind: "openai".into(),
            model: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_5m_tokens: 0,
            cache_creation_30m_tokens: 0,
            cache_creation_1h_tokens: 0,
            cost: rust_decimal::Decimal::ZERO,
            latency_ms: lat,
            usage_source: "upstream".into(),
            ended: "complete".into(),
        })
        .await
        .expect("usage");
    }
    db.add_usage_rollup(UsageRollupInput {
        granularity: "hour".into(),
        bucket_start: 0,
        provider_id: None,
        org_id: None,
        team_id: None,
        user_id: None,
        route_name: None,
        model: None,
        requests: 5,
        input_tokens: 1000,
        output_tokens: 400,
        cache_write_tokens: 0,
        cache_read_tokens: 0,
        cost: rust_decimal::Decimal::ZERO,
    })
    .await
    .expect("rollup");

    let m = db.metrics_aggregate().await.expect("aggregate");
    assert_eq!(m.requests_total, 5);
    assert_eq!(m.input_tokens_total, 1000);
    assert_eq!(m.output_tokens_total, 400);
    assert_eq!(m.latency_count, 2);
    assert_eq!(m.latency_sum_ms, 660);
    // buckets are [50,100,250,500,1000,...]: 60ms → first ≤100 bucket; 600ms → ≤1000.
    assert_eq!(m.latency_buckets[0], 0, "≤50ms");
    assert_eq!(m.latency_buckets[1], 1, "≤100ms");
    assert_eq!(m.latency_buckets[4], 2, "≤1000ms cumulative");
}

#[tokio::test]
async fn usage_summary_matches_filters_and_ignores_pagination() {
    let db = mem().await;
    for (rid, at, provider_id, model, input, output, cost) in [
        ("r1", 100, 1, "model-a", 10, 20, "0.001"),
        ("r2", 200, 1, "model-a", 30, 40, "0.002"),
        ("r3", 300, 2, "model-b", 50, 60, "0.004"),
    ] {
        db.append_usage(UsageInput {
            request_id: rid.to_owned(),
            at,
            route_name: Some("default".into()),
            provider_id: Some(provider_id),
            credential_id: None,
            org_id: None,
            team_id: None,
            user_id: Some(7),
            user_key_id: None,
            operation: "chat".into(),
            kind: "openai".into(),
            model: Some(model.into()),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: input + 1,
            cache_creation_5m_tokens: input + 2,
            cache_creation_30m_tokens: input + 3,
            cache_creation_1h_tokens: input + 4,
            cost: cost.parse().unwrap(),
            latency_ms: 0,
            usage_source: "upstream".into(),
            ended: "complete".into(),
        })
        .await
        .unwrap();
    }

    let summary = db
        .summarize_usages(&UsageQuery {
            at_from: Some(100),
            at_to: Some(250),
            provider_id: Some(1),
            user_id: Some(7),
            route_name: Some("default".into()),
            model: Some("model-a".into()),
            // Summary deliberately covers the full filtered result set.
            before_id: Some(2),
            limit: 1,
        })
        .await
        .unwrap();

    assert_eq!(summary.requests, 2);
    assert_eq!(summary.input_tokens, 40);
    assert_eq!(summary.output_tokens, 60);
    assert_eq!(summary.cache_read_tokens, 42);
    assert_eq!(summary.cache_creation_5m_tokens, 44);
    assert_eq!(summary.cache_creation_30m_tokens, 46);
    assert_eq!(summary.cache_creation_1h_tokens, 48);
    assert_eq!(summary.cost, "0.003".parse().unwrap());

    let page = db
        .query_usages_page(
            &UsageQuery {
                provider_id: Some(1),
                user_id: Some(7),
                model: Some("model-a".into()),
                ..Default::default()
            },
            &PageQuery {
                offset: 1,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].request_id, "r1");
}

#[tokio::test]
async fn downstream_logs_filter_by_time_and_usage_dimensions() {
    let db = mem().await;
    for (rid, at) in [("match", 200), ("other", 50)] {
        db.append_downstream_request(DownstreamRequestInput {
            request_id: rid.into(),
            at,
            method: "POST".into(),
            path: "/v1/messages".into(),
            query: None,
            status: 200,
            headers_json: None,
            body: None,
            response_body: None,
        })
        .await
        .unwrap();
        db.append_usage(UsageInput {
            request_id: rid.into(),
            at,
            route_name: Some(if rid == "match" { "chat" } else { "other" }.into()),
            provider_id: Some(if rid == "match" { 1 } else { 2 }),
            credential_id: None,
            org_id: None,
            team_id: None,
            user_id: Some(if rid == "match" { 7 } else { 8 }),
            user_key_id: None,
            operation: "chat".into(),
            kind: "openai".into(),
            model: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_5m_tokens: 0,
            cache_creation_30m_tokens: 0,
            cache_creation_1h_tokens: 0,
            cost: rust_decimal::Decimal::ZERO,
            latency_ms: 0,
            usage_source: "counted".into(),
            ended: "complete".into(),
        })
        .await
        .unwrap();
    }

    let rows = db
        .query_downstream_requests(&LogQuery {
            at_from: Some(100),
            provider_id: Some(1),
            user_id: Some(7),
            route_name: Some("chat".into()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].request_id, "match");

    let page = db
        .query_downstream_requests_page(
            &LogQuery {
                at_from: Some(100),
                user_id: Some(7),
                ..Default::default()
            },
            &PageQuery {
                offset: 0,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].request_id, "match");
}

#[tokio::test]
async fn audit_log_round_trip() {
    let db = mem().await;
    let delete = db
        .append_audit_log(AuditLogInput {
            actor_id: Some(7),
            actor_name: Some("admin".into()),
            action: "DELETE".into(),
            target: "/admin/credentials/5".into(),
            status: 204,
            source_ip: Some("203.0.113.9".into()),
        })
        .await
        .expect("append 1");
    db.append_audit_log(AuditLogInput {
        actor_id: None,
        actor_name: None,
        action: "login.fail".into(),
        target: "alice".into(),
        status: 401,
        source_ip: None,
    })
    .await
    .expect("append 2");

    let rows = db.list_audit_logs(10).await.expect("list");
    assert_eq!(rows.len(), 2);
    // Most-recent first (id desc): the login.fail row leads.
    assert_eq!(rows[0].action, "login.fail");
    assert_eq!(rows[0].target, "alice");
    assert_eq!(rows[0].actor_id, None);
    assert_eq!(rows[1].action, "DELETE");
    assert_eq!(rows[1].actor_name.as_deref(), Some("admin"));
    assert_eq!(rows[1].status, 204);
    assert!(rows[0].id > rows[1].id);

    // limit caps the result.
    assert_eq!(db.list_audit_logs(1).await.expect("list 1").len(), 1);
    let page = db
        .query_audit_logs_page(
            &AuditLogQuery::default(),
            &PageQuery {
                offset: 1,
                limit: 1,
            },
        )
        .await
        .expect("page");
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].action, "DELETE");

    let filtered = db
        .query_audit_logs_page(
            &AuditLogQuery {
                at_from: Some(delete.at),
                at_to: Some(delete.at),
                actor_id: Some(7),
                action: Some("LET".into()),
                target: Some("credentials".into()),
                status: Some(204),
                source_ip: Some("203.0.113.9".into()),
            },
            &PageQuery {
                offset: 0,
                limit: 10,
            },
        )
        .await
        .expect("filtered page");
    assert_eq!(filtered.total, 1);
    assert_eq!(filtered.items.len(), 1);
    assert_eq!(filtered.items[0].id, delete.id);
}
