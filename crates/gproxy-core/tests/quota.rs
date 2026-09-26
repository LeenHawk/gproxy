#![cfg(not(target_arch = "wasm32"))]

mod support;
use support::*;

use gproxy_channel::channel::{
    QuotaAllowance, QuotaEntry, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use gproxy_core::{
    BlockSource, CoreError, ExecutionTarget, RequestContext, SecretCodec, UsageState,
};
use gproxy_store::entity::{
    config::setting,
    limits::credential_quota_cycle,
    upstream::{credential, provider},
};
use http::StatusCode;
use sea_orm::EntityTrait;
use sea_orm::Set;
use serde_json::json;
use std::sync::Arc;

/// Provider `q` with `q1` (counted dimensions from metadata) and `q2` (none).
async fn seed_quota_provider(h: &Harness, q1_quota: serde_json::Value) {
    let store = h.core.store();
    store
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set("q".into()),
            name: Set("q".into()),
            channel: Set("test".into()),
            base_url: Set(Some("https://q.example".into())),
            config: Set(json!({"credential_strategy": "sticky"})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let cred = |id: &str, metadata: serde_json::Value| credential::ActiveModel {
        id: Set(id.into()),
        provider_id: Set("q".into()),
        user_id: Set(Some("u".into())),
        auth_kind: Set("api_key".into()),
        secret: Set(gproxy_core::PlaintextCodec
            .seal(id, &json!({"api_key": format!("k-{id}")}))
            .unwrap()),
        metadata: Set(metadata),
        ..Default::default()
    };
    store
        .credentials()
        .create_many(vec![cred("q1", q1_quota), cred("q2", json!({}))])
        .await
        .unwrap();
    let revision = h.core.snapshot().revision.0 + 1;
    store
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(i64::try_from(revision).unwrap()),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
}

fn only(h: &Harness, credential_id: &str, request_id: &str, attempts: u32) -> Arc<RequestContext> {
    let ctx = h.context_for("q", KEY, request_id, attempts, None);
    let target = h.target("q");
    Arc::new(RequestContext {
        target: ExecutionTarget {
            requested_model: None,
            credentials: target
                .credentials
                .iter()
                .filter(|c| c.id == credential_id)
                .cloned()
                .collect(),
            ..target
        },
        request_id: ctx.request_id.clone(),
        attribution: ctx.attribution.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: None,
        operation: ctx.operation,
        budgets: Vec::new(),
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    })
}

async fn blocks_for(
    h: &Harness,
    credential_id: &str,
) -> Vec<gproxy_store::entity::limits::credential_block::Model> {
    h.core
        .store()
        .load_control_data()
        .await
        .unwrap()
        .credential_blocks
        .into_iter()
        .filter(|row| row.credential_id == credential_id)
        .collect()
}

#[tokio::test]
async fn counted_requests_block_the_credential_when_the_window_fills() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "hourly", "metric": "requests", "window_seconds": 3600, "limit": 2}]}),
    )
    .await;
    assert_eq!(h.core.snapshot().credentials["q1"].quota.len(), 1);
    h.script(vec![
        json_reply(StatusCode::OK, json!({"ok": 1})),
        json_reply(StatusCode::OK, json!({"ok": 2})),
    ]);
    for id in ["r1", "r2"] {
        let execution = h
            .core
            .stream_generate_content(only(&h, "q1", id, 1), request("{}"))
            .await
            .unwrap();
        read(execution.into_parts().0.body).await;
    }
    // The second request landed on the limit: the window is now blocked.
    let rows = blocks_for(&h, "q1").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source["kind"], "counted");
    assert_eq!(rows[0].source["dimension"], "hourly");
    let error = h
        .core
        .stream_generate_content(only(&h, "q1", "r3", 1), request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NoUsableCredential), "{error}");
    assert_eq!(h.client.seen.lines().len(), 2, "nothing was sent for r3");

    // With the whole provider allowed, the unlimited credential takes over.
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": 3}))]);
    let execution = h
        .core
        .stream_generate_content(h.context_for("q", KEY, "r4", 2, None), request("{}"))
        .await
        .unwrap();
    read(execution.into_parts().0.body).await;
    assert!(h.client.seen.lines()[2].contains("auth=Bearer k-q2"));
}

#[tokio::test]
async fn counted_tokens_are_charged_after_usage_settles() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "daily_tokens", "metric": "tokens", "window_seconds": 86400, "limit": 10}]}),
    )
    .await;
    let usage = json!({"ok": 1, "usage": {"input_tokens": 4, "output_tokens": 2}});
    h.script(vec![
        json_reply(StatusCode::OK, usage.clone()),
        json_reply(StatusCode::OK, usage),
    ]);
    for id in ["r1", "r2"] {
        let execution = h
            .core
            .stream_generate_content(only(&h, "q1", id, 1), request("{}"))
            .await
            .unwrap();
        let (response, completion) = execution.into_parts();
        read(response.body).await;
        assert_eq!(completion.await.unwrap().state, UsageState::Completed);
    }
    // 6 + 6 tokens against a limit of 10: the second charge tripped the block.
    let rows = blocks_for(&h, "q1").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].source["dimension"], "daily_tokens");
    let error = h
        .core
        .stream_generate_content(only(&h, "q1", "r3", 1), request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NoUsableCredential), "{error}");
}

#[tokio::test]
async fn exhaustion_reported_in_headers_blocks_the_dimension_until_its_reset() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "primary", "metric": "requests", "window_seconds": 18000, "limit": null, "tracking": "reported"}]}),
    )
    .await;
    let reset = 9_999_999_999_000i64;
    h.script(vec![
        (
            StatusCode::TOO_MANY_REQUESTS,
            vec![("x-test-quota", "primary=0;reset=9999999999000")],
            vec![],
        ),
        json_reply(StatusCode::OK, json!({"ok": 1})),
    ]);
    let execution = h
        .core
        .stream_generate_content(h.context_for("q", KEY, "r1", 2, None), request("{}"))
        .await
        .unwrap();
    let (response, _) = execution.into_parts();
    assert_eq!(
        response.status,
        StatusCode::OK,
        "failed over after exhaustion"
    );
    let seen = h.client.seen.lines();
    assert!(seen[0].contains("auth=Bearer k-q1") && seen[1].contains("auth=Bearer k-q2"));
    let rows = blocks_for(&h, "q1").await;
    assert_eq!(
        rows.len(),
        1,
        "exhaustion replaces the generic rate-limit block"
    );
    assert_eq!(rows[0].source["kind"], "quota_exhausted");
    assert_eq!(rows[0].source["dimension"], "primary");
    assert_eq!(rows[0].until_ms, reset);
    let cycle_id = rows[0].source["cycle_id"].as_str().unwrap().to_owned();
    let cycles = h
        .core
        .store()
        .credential_quota_cycles()
        .get_many(&[cycle_id])
        .await
        .unwrap();
    let cycle = cycles[0].as_ref().unwrap();
    assert_eq!(cycle.credential_id, "q1");
    assert_eq!(cycle.resets_at_ms, Some(reset));
    assert_eq!(cycle.snapshot["remaining"], "0");
    let cached: gproxy_core::CredentialBlocks = serde_json::from_slice(
        &h.core
            .cache()
            .get(&gproxy_core::keys::credential_blocks("q", "q1"))
            .await
            .unwrap()
            .unwrap()
            .value,
    )
    .unwrap();
    assert!(matches!(
        &cached.blocks[0].source,
        BlockSource::QuotaExhausted { dimension, .. } if dimension == "primary"
    ));
}

#[tokio::test]
async fn the_channel_rule_places_an_observation_on_its_dimension() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "primary", "metric": "requests", "window_seconds": 18000, "limit": null, "tracking": "reported"}]}),
    )
    .await;
    h.script(vec![
        (
            StatusCode::TOO_MANY_REQUESTS,
            vec![("x-test-quota", "primary@7d_oi=0;reset=9999999999000")],
            vec![],
        ),
        json_reply(StatusCode::OK, json!({"ok": 1})),
    ]);
    let execution = h
        .core
        .stream_generate_content(h.context_for("q", KEY, "r1", 2, None), request("{}"))
        .await
        .unwrap();
    let (response, _) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let rows = blocks_for(&h, "q1").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].source["kind"], "quota_exhausted");
    assert_eq!(
        rows[0].source["dimension"], "primary",
        "not the raw source id"
    );
}

#[tokio::test]
async fn quota_query_persists_cycles_and_blocks_only_on_exhausted_known_dimensions() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "secondary", "metric": "requests", "window_seconds": 604800, "tracking": "reported"}]}),
    )
    .await;
    let entry = |id: &str, remaining: i64| QuotaEntry {
        id: id.into(),
        source_id: id.into(),
        label: None,
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Window(QuotaAllowance {
            remaining: Some(remaining.into()),
            limit: Some(100.into()),
            ..Default::default()
        }),
    };
    h.channel
        .quota_snapshots
        .lock()
        .unwrap()
        .push_back(QuotaSnapshot {
            observed_at_ms: 1,
            entries: vec![
                entry("secondary", 0),
                entry("unknown", 0),
                entry("other", 5),
            ],
        });
    let snapshot = h.core.query_credential_quota("q", "q1").await.unwrap();
    assert_eq!(snapshot.entries.len(), 3);
    assert!(
        snapshot.observed_at_ms > 1_700_000_000_000,
        "host stamps receipt"
    );
    let all = h
        .core
        .store()
        .credential_quota_cycles()
        .query(credential_quota_cycle::Entity::find())
        .await
        .unwrap();
    assert_eq!(all.len(), 3, "every entry is recorded");
    assert!(
        all.iter()
            .all(|row| row.observed_at_ms == snapshot.observed_at_ms)
    );
    let rows = blocks_for(&h, "q1").await;
    assert_eq!(
        rows.len(),
        1,
        "only the declared exhausted dimension blocks"
    );
    assert_eq!(rows[0].source["dimension"], "secondary");
    assert!(
        rows[0].until_ms > 0 && rows[0].until_ms - rows[0].observed_at_ms <= 604_800_000,
        "no upstream reset: one window from now"
    );
    let error = h.core.query_credential_quota("p", "q1").await.unwrap_err();
    assert!(matches!(error, CoreError::InvalidTarget(_)));
}

#[tokio::test]
async fn quota_query_refreshes_expired_material_before_querying() {
    let h = harness(full(), "sticky").await;
    h.core
        .store()
        .credentials()
        .refresh_many(vec![
            gproxy_store::operations::credentials::CredentialRefresh {
                id: "a".into(),
                expected_version: 0,
                secret: gproxy_core::PlaintextCodec
                    .seal("a", &json!({"api_key": "old"}))
                    .unwrap(),
                expires_at_ms: Some(1),
            },
        ])
        .await
        .unwrap();
    h.core.reload_credentials(&["a".into()]).await.unwrap();
    h.channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(RefreshReply::Rotated {
            api_key: "fresh",
            expires_at_ms: Some(9_999_999_999_000),
        });
    h.channel
        .quota_snapshots
        .lock()
        .unwrap()
        .push_back(QuotaSnapshot {
            observed_at_ms: 1,
            entries: vec![],
        });
    h.core.query_credential_quota("p", "a").await.unwrap();
    assert_eq!(*h.channel.quota_versions.lock().unwrap(), vec![2]);
    assert_eq!(h.channel.refresh_calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn quota_query_refreshes_unknown_expiry_on_401_and_retries_only_once() {
    for (status, failures, expected_queries, expected_refreshes) in [
        (StatusCode::UNAUTHORIZED, 1, 2, 1),
        (StatusCode::UNAUTHORIZED, 2, 2, 1),
        (StatusCode::FORBIDDEN, 1, 1, 0),
    ] {
        let h = harness(full(), "sticky").await;
        h.channel
            .refreshes
            .lock()
            .unwrap()
            .push_back(RefreshReply::Rotated {
                api_key: "fresh",
                expires_at_ms: None,
            });
        for _ in 0..failures {
            h.channel.quota_errors.lock().unwrap().push_back(
                gproxy_channel::ChannelError::UpstreamResponse {
                    status,
                    body: Default::default(),
                },
            );
        }
        h.channel
            .quota_snapshots
            .lock()
            .unwrap()
            .push_back(QuotaSnapshot {
                observed_at_ms: 1,
                entries: vec![],
            });
        let result = h.core.query_credential_quota("p", "a").await;
        assert_eq!(
            result.is_ok(),
            status == StatusCode::UNAUTHORIZED && failures == 1
        );
        let versions = h.channel.quota_versions.lock().unwrap();
        assert_eq!(versions.len(), expected_queries);
        assert_eq!(versions[0], 0);
        if expected_queries == 2 {
            assert_eq!(versions[1], 1);
        }
        assert_eq!(
            h.channel.refresh_calls.lock().unwrap().len(),
            expected_refreshes
        );
    }
}

#[tokio::test]
async fn positive_probe_clears_only_the_recovered_exhaustion_block() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(&h, json!({"quota": [
        {"id": "primary", "metric": "requests", "window_seconds": 18000, "tracking": "reported"},
        {"id": "secondary", "metric": "requests", "window_seconds": 604800, "tracking": "reported"}
    ]})).await;
    let entry = |id: &str, remaining: Option<i64>| QuotaEntry {
        id: id.into(),
        source_id: id.into(),
        label: None,
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Window(QuotaAllowance {
            remaining: remaining.map(Into::into),
            ..Default::default()
        }),
    };
    for entries in [
        vec![entry("primary", Some(0)), entry("secondary", Some(0))],
        vec![entry("primary", None)],
    ] {
        h.channel
            .quota_snapshots
            .lock()
            .unwrap()
            .push_back(QuotaSnapshot {
                observed_at_ms: 0,
                entries,
            });
        h.core.query_credential_quota("q", "q1").await.unwrap();
        assert_eq!(
            blocks_for(&h, "q1").await.len(),
            2,
            "unknown is not recovery"
        );
    }
    h.channel
        .quota_snapshots
        .lock()
        .unwrap()
        .push_back(QuotaSnapshot {
            observed_at_ms: 0,
            entries: vec![entry("primary", Some(100))],
        });
    h.core.query_credential_quota("q", "q1").await.unwrap();
    let rows = blocks_for(&h, "q1").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source["dimension"], "secondary");
    let cached = h
        .core
        .cache()
        .get(&gproxy_core::keys::credential_blocks("q", "q1"))
        .await
        .unwrap()
        .unwrap();
    let blocks: gproxy_core::CredentialBlocks = serde_json::from_slice(&cached.value).unwrap();
    assert_eq!(blocks.blocks.len(), 1);
    assert!(
        matches!(&blocks.blocks[0].source, BlockSource::QuotaExhausted { dimension, .. } if dimension == "secondary")
    );
}

#[tokio::test]
async fn breakdown_is_persisted_without_becoming_a_quota_block() {
    let h = harness(full(), "sticky").await;
    h.channel
        .quota_snapshots
        .lock()
        .unwrap()
        .push_back(QuotaSnapshot {
            observed_at_ms: 0,
            entries: vec![QuotaEntry {
                id: "seven_day_breakdown".into(),
                source_id: "seven_day_breakdown".into(),
                label: None,
                subject: QuotaSubject::Account,
                model_scope: QuotaScope::All,
                value: QuotaValue::Breakdown(vec![gproxy_channel::channel::QuotaBreakdownRow {
                    key: "claude_code".into(),
                    label: Some("Claude Code".into()),
                    percent: 100.into(),
                }]),
            }],
        });
    h.core.query_credential_quota("p", "a").await.unwrap();
    let rows = h
        .core
        .store()
        .credential_quota_cycles()
        .query(credential_quota_cycle::Entity::find())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].snapshot["kind"], "breakdown");
    assert_eq!(rows[0].snapshot["breakdown"][0]["percent"], "100");
    assert_eq!(rows[0].resets_at_ms, None);
    assert!(blocks_for(&h, "a").await.is_empty());
}

async fn cycle_rows(h: &Harness, credential_id: &str) -> Vec<credential_quota_cycle::Model> {
    let mut rows: Vec<_> = h
        .core
        .store()
        .credential_quota_cycles()
        .query(credential_quota_cycle::Entity::find())
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.credential_id == credential_id)
        .collect();
    rows.sort_by_key(|row| row.observed_at_ms);
    rows
}

/// One answer from `q1` carrying `x-test-quota: <quota>`.
async fn answer_with_quota(h: &Harness, request_id: &str, quota: &'static str) {
    h.script(vec![(
        StatusCode::OK,
        vec![
            ("content-type", "application/json"),
            ("x-test-quota", quota),
        ],
        vec![gproxy_protocol::connection::Bytes::from_static(b"{}")],
    )]);
    let execution = h
        .core
        .stream_generate_content(only(h, "q1", request_id, 1), request("{}"))
        .await
        .unwrap();
    read(execution.into_parts().0.body).await;
}

/// Pretend the persisted readings of `credential_id` are one heartbeat old.
async fn age_quota_observations(h: &Harness, credential_id: &str) {
    let key = gproxy_core::keys::credential_quota_observations(credential_id);
    let cached = h.core.cache().get(&key).await.unwrap().unwrap();
    let mut rows: Vec<serde_json::Value> = serde_json::from_slice(&cached.value).unwrap();
    for row in &mut rows {
        let at = row["observed_at_ms"].as_i64().unwrap();
        row["observed_at_ms"] = json!(at - 15 * 60 * 1000);
    }
    h.core
        .cache()
        .put(
            &key,
            serde_json::to_vec(&rows).unwrap(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
}

fn now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

#[tokio::test]
async fn unchanged_header_observations_persist_once_until_they_change_or_the_heartbeat() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "primary", "metric": "requests", "window_seconds": 18000, "tracking": "reported"}]}),
    )
    .await;
    for id in ["r1", "r2", "r3"] {
        answer_with_quota(&h, id, "primary=5;reset=9999999999000").await;
    }
    assert_eq!(cycle_rows(&h, "q1").await.len(), 1, "identical readings");

    answer_with_quota(&h, "r4", "primary=4;reset=9999999999000").await;
    assert_eq!(cycle_rows(&h, "q1").await.len(), 2, "remaining changed");

    // Drift inside five minutes is the same period; past it, a new one.
    answer_with_quota(&h, "r5", "primary=4;reset=9999999999060").await;
    assert_eq!(cycle_rows(&h, "q1").await.len(), 2, "reset drift");
    answer_with_quota(&h, "r6", "primary=4;reset=10000000600000").await;
    let rows = cycle_rows(&h, "q1").await;
    assert_eq!(rows.len(), 3, "reset moved");
    assert_eq!(rows[2].resets_at_ms, Some(10_000_000_600_000));

    age_quota_observations(&h, "q1").await;
    answer_with_quota(&h, "r7", "primary=4;reset=10000000600000").await;
    assert_eq!(cycle_rows(&h, "q1").await.len(), 4, "heartbeat");

    // A cold projection writes rather than guessing.
    h.core
        .cache()
        .delete(&gproxy_core::keys::credential_quota_observations("q1"))
        .await
        .unwrap();
    answer_with_quota(&h, "r8", "primary=4;reset=10000000600000").await;
    assert_eq!(cycle_rows(&h, "q1").await.len(), 5, "cache miss");
    assert!(blocks_for(&h, "q1").await.is_empty());
}

#[tokio::test]
async fn an_unused_window_with_a_floating_reset_persists_once() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "five_hour", "metric": "requests", "window_seconds": 18000, "tracking": "reported"}]}),
    )
    .await;
    let unused = |reset: i64| QuotaSnapshot {
        observed_at_ms: 0,
        entries: vec![QuotaEntry {
            id: "five_hour".into(),
            source_id: "five_hour".into(),
            label: None,
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Window(QuotaAllowance {
                used_percent: Some(0.into()),
                period_end_ms: Some(reset),
                ..Default::default()
            }),
        }],
    };
    let window = 18_000_000;
    let minutes = 60_000;
    // Eight minutes apart, but each is "now plus the window" within tolerance.
    for drift in [-4 * minutes, 4 * minutes] {
        h.channel
            .quota_snapshots
            .lock()
            .unwrap()
            .push_back(unused(now() + window + drift));
        h.core.query_credential_quota("q", "q1").await.unwrap();
    }
    assert_eq!(cycle_rows(&h, "q1").await.len(), 1);
}

#[tokio::test]
async fn exhausted_observations_always_persist_and_block() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "primary", "metric": "requests", "window_seconds": 18000, "tracking": "reported"}]}),
    )
    .await;
    for id in ["r1", "r2"] {
        h.script(vec![(
            StatusCode::TOO_MANY_REQUESTS,
            vec![("x-test-quota", "primary=0;reset=9999999999000")],
            vec![],
        )]);
        let _ = h
            .core
            .stream_generate_content(only(&h, "q1", id, 1), request("{}"))
            .await;
        // The block from the first answer keeps the credential out of the
        // second request; clear it so the same reading is observed again.
        if id == "r1" {
            let rows = cycle_rows(&h, "q1").await;
            let blocks = blocks_for(&h, "q1").await;
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].source["cycle_id"], rows[0].id.as_str());
            h.core
                .store()
                .credential_blocks()
                .delete_many(&[blocks[0].id.clone()])
                .await
                .unwrap();
            h.core
                .cache()
                .delete(&gproxy_core::keys::credential_blocks("q", "q1"))
                .await
                .unwrap();
        }
    }
    let rows = cycle_rows(&h, "q1").await;
    assert_eq!(rows.len(), 2, "exhaustion is always written");
    let blocks = blocks_for(&h, "q1").await;
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].source["kind"], "quota_exhausted");
    assert!(
        rows.iter()
            .any(|row| blocks[0].source["cycle_id"] == row.id.as_str()),
        "the block names a persisted row"
    );
}

#[tokio::test]
async fn readings_move_declared_cycles_and_undeclared_ones_never_open_one() {
    let h = harness(full(), "sticky").await;
    seed_quota_provider(
        &h,
        json!({"quota": [{"id": "primary", "metric": "requests", "window_seconds": 18000, "tracking": "reported"}]}),
    )
    .await;
    let reading = |id: &str, percent: i64, reset: i64| QuotaEntry {
        id: id.into(),
        source_id: id.into(),
        label: None,
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Window(QuotaAllowance {
            used_percent: Some(percent.into()),
            period_end_ms: Some(reset),
            ..Default::default()
        }),
    };
    let reset = now() + 2 * 60 * 60 * 1000;
    h.channel
        .quota_snapshots
        .lock()
        .unwrap()
        .push_back(QuotaSnapshot {
            observed_at_ms: 0,
            entries: vec![reading("primary", 10, reset), reading("extra", 50, reset)],
        });
    h.core.query_credential_quota("q", "q1").await.unwrap();
    let cycles = h
        .core
        .store()
        .credential_cycles()
        .recent("q1", 10)
        .await
        .unwrap();
    assert_eq!(cycles.len(), 1, "{cycles:?}");
    assert_eq!(cycles[0].window_id, "primary");
    assert_eq!(cycles[0].ends_at_ms, Some(reset));
    assert_eq!(
        cycles[0].boundary,
        gproxy_store::entity::limits::credential_cycle::CycleBoundary::Observed
    );
    let rows = cycle_rows(&h, "q1").await;
    assert_eq!(rows.len(), 2);
    for row in rows {
        if row.snapshot["id"] == "primary" {
            assert_eq!(
                row.credential_cycle_id.as_deref(),
                Some(cycles[0].id.as_str())
            );
            assert_eq!(row.cycle_cost_usd, Some(gproxy_store::FixedDecimal::ZERO));
        } else {
            assert_eq!(row.credential_cycle_id, None);
            assert_eq!(row.cycle_cost_usd, None);
        }
    }
}
