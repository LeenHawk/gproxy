#![cfg(not(target_arch = "wasm32"))]

mod support;
use support::*;

use gproxy_core::{
    CoreError, CredentialStatus, ExecutionTarget, RequestContext, SessionIdentity, SessionSource,
    UsageState,
};
use gproxy_store::entity::resource::{
    agent_assignment::{self, AssignmentReason, AssignmentState},
    agent_session::{self, AgentSessionState},
};
use http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde_json::json;
use std::sync::Arc;

async fn seed_session(h: &Harness, id: &str) {
    h.core
        .store()
        .agent_sessions()
        .create_many(vec![agent_session::ActiveModel {
            id: Set(id.into()),
            user_id: Set("u".into()),
            scope: Set("agent".into()),
            affinity_key: Set(id.into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
}

fn agent_context(
    h: &Harness,
    session_id: &str,
    request_id: &str,
    attempts: u32,
) -> Arc<RequestContext> {
    let ctx = h.context(request_id, attempts, None);
    Arc::new(RequestContext {
        target: ExecutionTarget {
            provider: ctx.target.provider.clone(),
            upstream_model: ctx.target.upstream_model.clone(),
            credentials: ctx.target.credentials.clone(),
        },
        request_id: ctx.request_id.clone(),
        attribution: ctx.attribution.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: Some(SessionIdentity {
            id: format!("affinity-{session_id}"),
            source: SessionSource::CodexThread,
            field: None,
            agent_session_id: Some(session_id.into()),
        }),
        operation: ctx.operation,
        budgets: Vec::new(),
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    })
}

async fn session_row(h: &Harness, id: &str) -> agent_session::Model {
    h.core
        .store()
        .agent_sessions()
        .get_many(&[id.to_owned()])
        .await
        .unwrap()
        .remove(0)
        .unwrap()
}

async fn assignments(h: &Harness, session_id: &str) -> Vec<agent_assignment::Model> {
    h.core
        .store()
        .agent_assignments()
        .query(
            agent_assignment::Entity::find()
                .filter(agent_assignment::Column::SessionId.eq(session_id))
                .order_by_asc(agent_assignment::Column::Generation),
        )
        .await
        .unwrap()
}

async fn run(h: &Harness, ctx: Arc<RequestContext>) -> Result<StatusCode, CoreError> {
    let execution = h.core.stream_generate_content(ctx, request("{}")).await?;
    let (response, completion) = execution.into_parts();
    let status = response.status;
    read(response.body).await;
    let _ = completion.await;
    Ok(status)
}

#[tokio::test]
async fn an_agent_session_binds_on_first_success_and_stays_bound_across_rotation() {
    let h = harness(full(), "round_robin").await;
    seed_session(&h, "s").await;
    h.script(vec![
        json_reply(StatusCode::OK, json!({"ok": 1})),
        json_reply(StatusCode::OK, json!({"ok": 2})),
        json_reply(StatusCode::OK, json!({"ok": 3})),
    ]);
    assert_eq!(
        run(&h, agent_context(&h, "s", "r1", 2)).await.unwrap(),
        StatusCode::OK
    );
    let row = session_row(&h, "s").await;
    assert_eq!(row.state, AgentSessionState::Ready);
    assert_eq!(row.active_generation, Some(1));
    assert_eq!(row.pending_assignment_id, None);
    let rows = assignments(&h, "s").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, AssignmentState::Active);
    assert_eq!(rows[0].reason, AssignmentReason::Initial);
    assert_eq!(rows[0].credential_id, "a");
    assert!(rows[0].activated_at_ms.is_some());

    // Round robin would move on; the session does not.
    assert_eq!(
        run(&h, agent_context(&h, "s", "r2", 2)).await.unwrap(),
        StatusCode::OK
    );
    assert_eq!(
        run(&h, agent_context(&h, "s", "r3", 2)).await.unwrap(),
        StatusCode::OK
    );
    let seen = h.client.seen.lines();
    assert!(
        seen.iter().all(|l| l.contains("auth=Bearer ka")),
        "{seen:?}"
    );
    assert_eq!(assignments(&h, "s").await.len(), 1, "no new generation");
    let trace = h.observer.log.lines();
    assert!(
        trace
            .iter()
            .any(|l| l.starts_with("trace r2-1 a Succeeded")),
        "{trace:?}"
    );
}

#[tokio::test]
async fn transient_trouble_uses_another_credential_without_moving_the_session() {
    let h = harness(full(), "round_robin").await;
    seed_session(&h, "s").await;
    h.script(vec![
        json_reply(StatusCode::OK, json!({"ok": 1})),
        (
            StatusCode::TOO_MANY_REQUESTS,
            vec![("retry-after", "60")],
            vec![],
        ),
        json_reply(StatusCode::OK, json!({"ok": 2})),
        json_reply(StatusCode::OK, json!({"ok": 3})),
    ]);
    run(&h, agent_context(&h, "s", "r1", 2)).await.unwrap();
    // a is rate limited on r2: b answers, but the binding stays on a.
    assert_eq!(
        run(&h, agent_context(&h, "s", "r2", 2)).await.unwrap(),
        StatusCode::OK
    );
    let seen = h.client.seen.lines();
    assert!(
        seen[1].contains("auth=Bearer ka") && seen[2].contains("auth=Bearer kb"),
        "{seen:?}"
    );
    let rows = assignments(&h, "s").await;
    assert_eq!(rows.len(), 1, "a rate limit is not exhaustion: {rows:?}");
    assert_eq!(rows[0].credential_id, "a");
    // While a is blocked, r3 runs unbound on b and still does not switch.
    assert_eq!(
        run(&h, agent_context(&h, "s", "r3", 2)).await.unwrap(),
        StatusCode::OK
    );
    assert!(h.client.seen.lines()[3].contains("auth=Bearer kb"));
    assert_eq!(assignments(&h, "s").await.len(), 1);
    assert_eq!(session_row(&h, "s").await.active_generation, Some(1));
}

#[tokio::test]
async fn a_durably_unusable_credential_switches_the_session_with_evidence() {
    let h = harness(full(), "round_robin").await;
    seed_session(&h, "s").await;
    h.script(vec![
        json_reply(StatusCode::OK, json!({"ok": 1})),
        json_reply(StatusCode::OK, json!({"ok": 2})),
        json_reply(StatusCode::OK, json!({"ok": 3})),
    ]);
    run(&h, agent_context(&h, "s", "r1", 2)).await.unwrap();
    // a dies: the next request reserves generation 2 on b and activates it.
    h.core
        .store()
        .credentials()
        .set_status_many(vec![
            gproxy_store::operations::credentials::CredentialStatusUpdate {
                id: "a".into(),
                expected_version: 0,
                status: CredentialStatus::Dead,
                reason: Some("revoked".into()),
            },
        ])
        .await
        .unwrap();
    h.core.reload_credentials(&["a".into()]).await.unwrap();
    assert_eq!(
        run(&h, agent_context(&h, "s", "r2", 2)).await.unwrap(),
        StatusCode::OK
    );
    assert!(h.client.seen.lines()[1].contains("auth=Bearer kb"));
    let rows = assignments(&h, "s").await;
    assert_eq!(rows.len(), 2, "{rows:#?}");
    assert_eq!(rows[0].state, AssignmentState::Replaced);
    assert!(rows[0].replaced_at_ms.is_some());
    // Generations are session revisions: activation of 1 moved the row to
    // version 2, so the switch reserved generation 3.
    assert_eq!(rows[1].generation, 3);
    assert_eq!(rows[1].previous_generation, Some(1));
    assert_eq!(rows[1].reason, AssignmentReason::QuotaExhausted);
    assert_eq!(rows[1].trigger_request_id.as_deref(), Some("r2"));
    assert_eq!(rows[1].credential_id, "b");
    assert_eq!(rows[1].state, AssignmentState::Active);
    let row = session_row(&h, "s").await;
    assert_eq!(row.active_generation, Some(3));
    assert_eq!(row.state, AgentSessionState::Ready);
    // And it stays on b afterwards.
    run(&h, agent_context(&h, "s", "r3", 2)).await.unwrap();
    assert!(h.client.seen.lines()[2].contains("auth=Bearer kb"));
    assert_eq!(assignments(&h, "s").await.len(), 2);
}

#[tokio::test]
async fn a_failed_preparation_blocks_the_session_and_the_next_request_reserves_again() {
    let h = harness(full(), "round_robin").await;
    seed_session(&h, "s").await;
    h.script(vec![
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 1})),
        json_reply(StatusCode::OK, json!({"ok": 1})),
    ]);
    // One attempt: the 502 is the answer and the reservation failed uncertain.
    assert_eq!(
        run(&h, agent_context(&h, "s", "r1", 1)).await.unwrap(),
        StatusCode::BAD_GATEWAY
    );
    let rows = assignments(&h, "s").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, AssignmentState::Uncertain);
    assert_eq!(rows[0].error.as_deref(), Some("502"));
    let row = session_row(&h, "s").await;
    assert_eq!(row.state, AgentSessionState::Blocked);
    assert_eq!(row.active_generation, None);
    // The next request reserves a new initial generation.
    assert_eq!(
        run(&h, agent_context(&h, "s", "r2", 1)).await.unwrap(),
        StatusCode::OK
    );
    let rows = assignments(&h, "s").await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        rows[1].generation, 3,
        "the failure bumped the session revision"
    );
    assert_eq!(rows[1].reason, AssignmentReason::Initial);
    assert_eq!(rows[1].state, AssignmentState::Active);
    assert_eq!(session_row(&h, "s").await.active_generation, Some(3));
    let report_state = h.observer.reports.lock().unwrap().last().unwrap().state;
    assert_eq!(report_state, UsageState::Completed);
}

#[tokio::test]
async fn an_unknown_or_closed_session_runs_unbound() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": 1}))]);
    // No row: ordinary selection, nothing written.
    assert_eq!(
        run(&h, agent_context(&h, "missing", "r1", 1))
            .await
            .unwrap(),
        StatusCode::OK
    );
    assert!(assignments(&h, "missing").await.is_empty());
}
