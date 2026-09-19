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
            credentials: target
                .credentials
                .iter()
                .filter(|c| c.id == credential_id)
                .cloned()
                .collect(),
            ..target
        },
        request_id: ctx.request_id.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: None,
        operation: ctx.operation,
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
    let all = h
        .core
        .store()
        .credential_quota_cycles()
        .query(credential_quota_cycle::Entity::find())
        .await
        .unwrap();
    assert_eq!(all.len(), 3, "every entry is recorded");
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
