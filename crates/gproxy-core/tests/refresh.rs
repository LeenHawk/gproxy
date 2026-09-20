mod support;
use support::*;

use gproxy_core::{CoreError, CredentialStatus, RefreshMode, SecretCodec, keys};
use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_store::operations::credentials::CredentialRefresh as RefreshRow;
use http::StatusCode;
use serde_json::json;

async fn stored_version(h: &Harness, id: &str) -> (i64, CredentialStatus, Option<String>) {
    let row = h
        .core
        .store()
        .credentials()
        .get_many(&[id.to_owned()])
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    (row.version, row.status, row.status_reason)
}

#[tokio::test]
async fn force_refresh_rotates_the_secret_persists_by_cas_and_publishes() {
    let h = harness(full(), "round_robin").await;
    let mut subscription = h
        .core
        .cache()
        .subscribe(keys::INVALIDATION_TOPIC)
        .await
        .unwrap();
    // The first notification is always the resync marker.
    assert!(matches!(
        subscription.recv().await.unwrap(),
        gproxy_cache::Notification::ResyncRequired
    ));
    h.channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(RefreshReply::Rotated {
            api_key: "ka2",
            expires_at_ms: Some(9_999_999_999_000),
        });
    let summary = h
        .core
        .refresh_credential("p", "a", RefreshMode::Force)
        .await
        .unwrap();
    assert_eq!(summary.version, 1);
    assert_eq!(summary.status, CredentialStatus::Active);
    assert_eq!(summary.expires_at_ms, Some(9_999_999_999_000));
    assert_eq!(stored_version(&h, "a").await.0, 1);
    let live = h.core.snapshot().credentials["a"].state.load();
    assert_eq!(live.version, 1);
    assert_eq!(live.secret["api_key"], "ka2");
    let calls = h.channel.refresh_calls.lock().unwrap().clone();
    assert_eq!(calls, vec![("a".to_owned(), 0)]);
    let gproxy_cache::Notification::Message(payload) = subscription.recv().await.unwrap() else {
        panic!("a CredentialChanged notification is published");
    };
    let event: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(event["kind"], "credential_changed");
    assert_eq!(event["credential_id"], "a");
    assert_eq!(event["version"], 1);

    // The next attempt uses the rotated material.
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": 1}))]);
    let ctx = h.context("r1", 1, None);
    let one = std::sync::Arc::new(gproxy_core::RequestContext {
        target: gproxy_core::ExecutionTarget {
            provider: ctx.target.provider.clone(),
            upstream_model: ctx.target.upstream_model.clone(),
            credentials: vec![ctx.target.credentials[0].clone()],
        },
        request_id: ctx.request_id.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: None,
        operation: ctx.operation,
        budgets: Vec::new(),
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    });
    let execution = h
        .core
        .stream_generate_content(one, request("{}"))
        .await
        .unwrap();
    read(execution.into_parts().0.body).await;
    assert!(h.client.seen.lines()[0].contains("auth=Bearer ka2"));
}

#[tokio::test]
async fn if_needed_skips_fresh_material_and_a_peer_version_satisfies_it() {
    let h = harness(full(), "round_robin").await;
    // No expiry: nothing to do, the channel is never asked.
    let summary = h
        .core
        .refresh_credential("p", "a", RefreshMode::IfNeeded)
        .await
        .unwrap();
    assert_eq!(summary.version, 0);
    assert!(h.channel.refresh_calls.lock().unwrap().is_empty());

    // A peer rotated the row directly in the Store; the lease is free, the
    // authoritative read publishes the peer's version without a channel call.
    h.core
        .store()
        .credentials()
        .refresh_many(vec![RefreshRow {
            id: "b".into(),
            expected_version: 0,
            secret: gproxy_core::PlaintextCodec
                .seal("b", &json!({"api_key": "kb-peer"}))
                .unwrap(),
            expires_at_ms: Some(9_999_999_999_000),
        }])
        .await
        .unwrap();
    let summary = h
        .core
        .refresh_credential("p", "b", RefreshMode::Force)
        .await
        .unwrap();
    assert_eq!(summary.version, 1, "the peer's newer version stands in");
    assert!(h.channel.refresh_calls.lock().unwrap().is_empty());
    assert_eq!(
        h.core.snapshot().credentials["b"].state.load().secret["api_key"],
        "kb-peer"
    );

    // Wrong provider is an invalid target, not a refresh.
    let error = h
        .core
        .refresh_credential("claude", "a", RefreshMode::Force)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::InvalidTarget(_)), "{error}");
}

#[tokio::test]
async fn a_definitive_rejection_marks_the_credential_dead_and_selection_reports_it() {
    let h = harness(full(), "round_robin").await;
    h.channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(RefreshReply::Rejected("invalid_grant"));
    let error = h
        .core
        .refresh_credential("p", "a", RefreshMode::Force)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::CredentialDead { credential_id, reason }
            if credential_id == "a" && reason.as_deref() == Some("invalid_grant")),
        "{error}"
    );
    assert_eq!(
        stored_version(&h, "a").await,
        (1, CredentialStatus::Dead, Some("invalid_grant".into()))
    );
    let live = h.core.snapshot().credentials["a"].state.load();
    assert_eq!(live.status, CredentialStatus::Dead);
    // Dead is final: Force does not ask the channel again.
    let again = h
        .core
        .refresh_credential("p", "a", RefreshMode::Force)
        .await
        .unwrap_err();
    assert!(matches!(again, CoreError::CredentialDead { .. }));
    assert_eq!(h.channel.refresh_calls.lock().unwrap().len(), 1);

    // A transient failure changes nothing durable.
    h.channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(RefreshReply::Failed);
    let error = h
        .core
        .refresh_credential("p", "b", RefreshMode::Force)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Channel(_)), "{error}");
    assert_eq!(stored_version(&h, "b").await.0, 0);
    // The lease was released: a second refresh proceeds immediately.
    h.channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(RefreshReply::Rotated {
            api_key: "kb2",
            expires_at_ms: None,
        });
    assert_eq!(
        h.core
            .refresh_credential("p", "b", RefreshMode::Force)
            .await
            .unwrap()
            .version,
        1
    );
}

#[tokio::test]
async fn unauthorized_upstream_forces_one_refresh_and_retries_the_same_credential() {
    let h = harness(full(), "sticky").await;
    h.channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(RefreshReply::Rotated {
            api_key: "ka-new",
            expires_at_ms: None,
        });
    h.script(vec![
        json_reply(StatusCode::UNAUTHORIZED, json!({"error": "expired"})),
        json_reply(StatusCode::OK, json!({"ok": 1})),
    ]);
    let ctx = h.context("r1", 3, Some("s1"));
    let one = std::sync::Arc::new(gproxy_core::RequestContext {
        target: gproxy_core::ExecutionTarget {
            provider: ctx.target.provider.clone(),
            upstream_model: ctx.target.upstream_model.clone(),
            credentials: vec![ctx.target.credentials[0].clone()],
        },
        request_id: ctx.request_id.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: ctx.session.clone(),
        operation: OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAi,
        },
        budgets: Vec::new(),
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    });
    let execution = h
        .core
        .stream_generate_content(one, request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    read(response.body).await;
    assert_eq!(
        completion.await.unwrap().state,
        gproxy_core::UsageState::Completed
    );
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].contains("auth=Bearer ka"), "{}", seen[0]);
    assert!(seen[1].contains("auth=Bearer ka-new"), "{}", seen[1]);
    assert_eq!(h.channel.refresh_calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn material_near_expiry_is_refreshed_before_the_attempt_pins_it() {
    let h = harness(full(), "round_robin").await;
    // Expire credential a in the Store and publish that version.
    h.core
        .store()
        .credentials()
        .refresh_many(vec![RefreshRow {
            id: "a".into(),
            expected_version: 0,
            secret: gproxy_core::PlaintextCodec
                .seal("a", &json!({"api_key": "ka-old"}))
                .unwrap(),
            expires_at_ms: Some(1),
        }])
        .await
        .unwrap();
    h.core.reload_credentials(&["a".into()]).await.unwrap();
    h.channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(RefreshReply::Rotated {
            api_key: "ka-fresh",
            expires_at_ms: None,
        });
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": 1}))]);
    let ctx = h.context("r1", 1, None);
    let one = std::sync::Arc::new(gproxy_core::RequestContext {
        target: gproxy_core::ExecutionTarget {
            provider: ctx.target.provider.clone(),
            upstream_model: ctx.target.upstream_model.clone(),
            credentials: vec![ctx.target.credentials[0].clone()],
        },
        request_id: ctx.request_id.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: None,
        operation: ctx.operation,
        budgets: Vec::new(),
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    });
    let execution = h
        .core
        .stream_generate_content(one, request("{}"))
        .await
        .unwrap();
    read(execution.into_parts().0.body).await;
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].contains("auth=Bearer ka-fresh"), "{}", seen[0]);
    assert_eq!(stored_version(&h, "a").await.0, 2);
}
