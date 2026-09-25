#![cfg(not(target_arch = "wasm32"))]
mod support;
use support::*;

use gproxy_channel::channel::{
    QuotaAllowance, QuotaEntry, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use gproxy_core::{RequestContext, keys};
use gproxy_store::entity::{
    config::setting,
    upstream::{credential, provider},
};
use http::StatusCode;
use sea_orm::Set;
use serde_json::json;
use std::sync::Arc;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn configured(strategy: &str, affinity: Option<bool>) -> Harness {
    let h = harness(full(), strategy).await;
    let mut config = json!({"credential_strategy": strategy});
    if let Some(value) = affinity {
        config["session_affinity"] = json!(value);
    }
    h.core
        .store()
        .providers()
        .update_many(vec![provider::ActiveModel {
            id: Set("p".into()),
            config: Set(config),
            ..Default::default()
        }])
        .await
        .unwrap();
    for id in ["a", "b"] {
        h.core
            .store()
            .credentials()
            .update_many(vec![credential::ActiveModel {
                id: Set(id.into()),
                metadata: Set(json!({"quota":[
                    {"id":"5h", "window_seconds":18000, "tracking":"reported"},
                    {"id":"1w", "window_seconds":604800, "tracking":"reported"}
                ]})),
                ..Default::default()
            }])
            .await
            .unwrap();
    }
    h.core
        .store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(h.core.snapshot().revision.0 as i64 + 1),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
    h
}

async fn observe(h: &Harness, credential: &str, entries: &[(&str, Option<i64>, i64)]) {
    h.channel
        .quota_snapshots
        .lock()
        .unwrap()
        .push_back(QuotaSnapshot {
            observed_at_ms: now(),
            entries: entries
                .iter()
                .map(|(id, end, remaining)| QuotaEntry {
                    id: (*id).into(),
                    source_id: (*id).into(),
                    label: None,
                    subject: QuotaSubject::Account,
                    model_scope: QuotaScope::All,
                    value: QuotaValue::Window(QuotaAllowance {
                        remaining: Some((*remaining).into()),
                        period_end_ms: *end,
                        ..Default::default()
                    }),
                })
                .collect(),
        });
    h.core
        .query_credential_quota("p", credential)
        .await
        .unwrap();
}

async fn run(h: &Harness, session: Option<&str>, allowed: Option<&str>) -> String {
    let mut context: RequestContext = (*h.context("r", 2, session)).clone();
    if let Some(id) = allowed {
        context.target.credentials.retain(|c| c.id == id);
    }
    run_context(h, context).await
}

async fn run_context(h: &Harness, context: RequestContext) -> String {
    h.script(vec![json_reply(StatusCode::OK, json!({"ok":true}))]);
    let execution = h
        .core
        .stream_generate_content(Arc::new(context), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    completion.await.unwrap();
    let line = h.client.seen.lines().pop().unwrap();
    if line.contains("auth=Bearer ka") {
        "a".into()
    } else {
        assert!(line.contains("auth=Bearer kb"), "{line}");
        "b".into()
    }
}

#[tokio::test]
async fn earliest_reset_compares_actual_times_across_windows_and_stays_in_allowed_pool() {
    let h = configured("earliest_reset", Some(false)).await;
    let time = now();
    observe(
        &h,
        "a",
        &[
            ("5h", Some(time + 7_200_000), 10),
            ("1w", Some(time + 600_000), 10),
        ],
    )
    .await;
    observe(&h, "b", &[("5h", Some(time + 3_600_000), 80)]).await;
    assert_eq!(run(&h, None, None).await, "a");
    assert_eq!(run(&h, None, None).await, "a");
    assert_eq!(run(&h, None, Some("b")).await, "b");
    // New observed exhaustion excludes a even though its reset is sooner.
    observe(&h, "a", &[("1w", Some(time + 600_000), 0)]).await;
    assert_eq!(run(&h, None, None).await, "b");
}

#[tokio::test]
async fn a_valid_pin_wins_over_new_reset_order_and_new_sessions_use_the_new_order() {
    let h = configured("earliest_reset", Some(true)).await;
    let time = now();
    observe(&h, "a", &[("5h", Some(time + 3_600_000), 10)]).await;
    observe(&h, "b", &[("5h", Some(time + 600_000), 10)]).await;
    assert_eq!(run(&h, Some("s"), None).await, "b");
    observe(&h, "a", &[("1w", Some(time + 300_000), 10)]).await;
    assert_eq!(run(&h, Some("s"), None).await, "b");
    assert_eq!(run(&h, Some("new"), None).await, "a");
}

#[tokio::test]
async fn tied_and_unknown_resets_rotate_and_expired_observations_do_not_win() {
    let h = configured("earliest_reset", None).await;
    assert_eq!(run(&h, None, None).await, "a");
    assert_eq!(run(&h, None, None).await, "b");
    let time = now();
    observe(&h, "a", &[("5h", Some(time - 1), 10)]).await;
    observe(&h, "b", &[("5h", Some(time + 600_000), 10)]).await;
    assert_eq!(run(&h, None, None).await, "b");
    observe(&h, "a", &[("1w", Some(time + 600_000), 10)]).await;
    assert_eq!(run(&h, None, None).await, "a");
    assert_eq!(run(&h, None, None).await, "b");
}

#[tokio::test]
async fn newer_unknown_reset_does_not_resurrect_an_old_reading() {
    let h = configured("earliest_reset", None).await;
    let time = now();
    observe(&h, "a", &[("5h", Some(time + 300_000), 10)]).await;
    observe(&h, "b", &[("5h", Some(time + 600_000), 10)]).await;
    assert_eq!(run(&h, None, None).await, "a");
    // Ensure a strictly later observation even with a millisecond clock.
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    observe(&h, "a", &[("5h", None, 10)]).await;
    assert_eq!(run(&h, None, None).await, "b");
    // A cache miss (as on another instance/restart) gives the same answer.
    h.core
        .cache()
        .delete(&keys::credential_reset_observations("a"))
        .await
        .unwrap();
    assert_eq!(run(&h, None, None).await, "b");
}

#[tokio::test]
async fn independent_affinity_switch_overrides_legacy_defaults() {
    for strategy in ["sticky", "round_robin_affinity"] {
        let h = configured(strategy, None).await;
        assert_eq!(run(&h, Some("s"), None).await, "a");
        assert_eq!(run(&h, Some("s"), None).await, "a");
        let h = configured(strategy, Some(false)).await;
        assert_eq!(run(&h, Some("s"), None).await, "a");
        assert_eq!(run(&h, Some("s"), None).await, "b");
    }
    let h = configured("round_robin", Some(true)).await;
    assert_eq!(run(&h, Some("s"), None).await, "a");
    assert_eq!(run(&h, Some("s"), None).await, "a");
}

#[tokio::test]
async fn reset_order_respects_dimension_model_and_operation_scopes() {
    use gproxy_channel::channel::QuotaWindow;
    use gproxy_protocol::Operation;
    let h = configured("earliest_reset", None).await;
    let time = now();
    observe(&h, "a", &[("5h", Some(time + 300_000), 10)]).await;
    observe(&h, "b", &[("5h", Some(time + 600_000), 10)]).await;
    for mode in ["model", "operation", "total", "no_model", "observed_scope"] {
        let mut ctx = (*h.context("r", 2, None)).clone();
        let a = ctx
            .target
            .credentials
            .iter_mut()
            .find(|c| c.id == "a")
            .unwrap();
        let dimension = &mut Arc::get_mut(a).unwrap().quota[0];
        match mode {
            "model" => dimension.scope = QuotaScope::Models(vec!["other-model".into()]),
            "operation" => dimension.operations = Some(vec![Operation::CountTokens]),
            "total" => dimension.window = QuotaWindow::Total,
            "no_model" => {
                dimension.scope = QuotaScope::Models(vec!["gpt-x".into()]);
                ctx.target.upstream_model = None;
            }
            // Unknown declarations may use a concrete scope from the observation.
            "observed_scope" => dimension.scope = QuotaScope::Unknown,
            _ => unreachable!(),
        }
        assert_eq!(
            run_context(&h, ctx).await,
            if mode == "observed_scope" { "a" } else { "b" },
            "{mode}"
        );
    }
}
