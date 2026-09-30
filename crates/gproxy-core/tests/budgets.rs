#![cfg(not(target_arch = "wasm32"))]

//! Caller budgets: pricing at settlement, pre-attempt rejection, lazy
//! windows, calendar and permanent periods, manual reset and model scoping.
mod support;
use support::*;

use gproxy_core::{BudgetOwner, CoreError, RequestContext, UsageState};
use gproxy_store::entity::{
    config::setting,
    identity::api_key,
    limits::{quota, quota_settlement, quota_window},
    pricing::{price_rate, price_rule, price_unit::PriceUnit},
};
use http::StatusCode;
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;
use std::sync::Arc;

const HOUR: i64 = 3_600_000;

fn wall_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

fn money(s: &str) -> gproxy_store::FixedDecimal {
    s.parse().unwrap()
}

fn user(id: &str) -> BudgetOwner {
    BudgetOwner::new("user", id)
}

fn owner(kind: &str, id: &str) -> BudgetOwner {
    BudgetOwner::new(kind, id)
}

/// One `cost`/USD budget row. `limit` in USD; `model` scopes it.
fn budget_row(
    id: &str,
    owner: BudgetOwner,
    period: &str,
    limit: &str,
    model: Option<&str>,
) -> quota::ActiveModel {
    quota::ActiveModel {
        id: Set(id.into()),
        owner_kind: Set(owner.kind),
        owner_id: Set(owner.id),
        window_key: Set(format!("{id}-key")),
        metric: Set("cost".into()),
        unit: Set("USD".into()),
        limit_value: Set(money(limit)),
        period: Set(period.into()),
        model_pattern: Set(model.map(Into::into)),
        ..Default::default()
    }
}

async fn bump(h: &Harness) {
    let revision = h.core.snapshot().revision.0 + 1;
    h.core
        .store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(i64::try_from(revision).unwrap()),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
}

/// Provider `p` prices `gpt-*` at 10 USD/M input and 30 USD/M output, so
/// the scripted usage (4 in, 2 out) costs 0.0001 USD per request.
async fn seed_pricing(h: &Harness) {
    let store = h.core.store();
    store
        .price_rules()
        .create_many(vec![price_rule::ActiveModel {
            id: Set("gpt".into()),
            provider_id: Set(Some("p".into())),
            model_pattern: Set("gpt-*".into()),
            currency: Set("USD".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    let rate = |id: &str, metric: &str, value: &str| price_rate::ActiveModel {
        id: Set(id.into()),
        price_rule_id: Set("gpt".into()),
        metric: Set(metric.into()),
        unit: Set(PriceUnit::Token),
        unit_quantity: Set(money("1000000")),
        value: Set(money(value)),
        ..Default::default()
    };
    store
        .price_rates()
        .create_many(vec![
            rate("in", "input_tokens", "10"),
            rate("out", "output_tokens", "30"),
        ])
        .await
        .unwrap();
}

async fn seed_budgets(h: &Harness, rows: Vec<quota::ActiveModel>) {
    h.core.store().quotas().create_many(rows).await.unwrap();
    bump(h).await;
}

fn context(h: &Harness, id: &str, owners: Vec<BudgetOwner>, model: &str) -> Arc<RequestContext> {
    let ctx = h.context(id, 1, None);
    let mut target = h.target("p");
    target.upstream_model = Some(model.into());
    Arc::new(RequestContext {
        target,
        request_id: ctx.request_id.clone(),
        attribution: ctx.attribution.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: None,
        operation: ctx.operation,
        budgets: owners,
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    })
}

fn usage_reply() -> Reply {
    json_reply(
        StatusCode::OK,
        json!({"ok": 1, "usage": {"input_tokens": 4, "output_tokens": 2}}),
    )
}

async fn run(h: &Harness, ctx: Arc<RequestContext>) -> gproxy_core::UsageReport {
    let execution = h
        .core
        .stream_generate_content(ctx, request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    completion.await.unwrap()
}

async fn windows(h: &Harness, quota_id: &str) -> Vec<quota_window::Model> {
    h.core
        .store()
        .quota_windows()
        .query(quota_window::Entity::find().filter(quota_window::Column::QuotaId.eq(quota_id)))
        .await
        .unwrap()
}

#[tokio::test]
async fn priced_requests_settle_into_the_window_and_the_observer_sees_the_cost() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_budgets(&h, vec![budget_row("b", user("u"), "1d", "0.00015", None)]).await;
    assert_eq!(h.core.snapshot().budgets.len(), 1);
    h.script(vec![usage_reply(), usage_reply()]);
    let report = run(&h, context(&h, "r1", vec![user("u")], "gpt-x")).await;
    assert_eq!(report.state, UsageState::Completed);
    let cost = report.cost.as_ref().expect("priced");
    assert_eq!(cost.amount, Decimal::from_str_exact("0.0001").unwrap());
    assert_eq!(cost.currency, "USD");
    assert_eq!(report.exchanges[0].cost, report.cost);
    assert!(
        !report.exchanges[0]
            .usage
            .dimensions
            .contains_key("unpriced")
    );
    let rows = windows(&h, "b").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].used, money("0.0001"));
    assert_eq!(rows[0].ends_at_ms, Some(rows[0].starts_at_ms + 24 * HOUR));
    assert_eq!(rows[0].quota_snapshot["window_key"], "b-key");
    // Under the limit still: the second request runs and adds up.
    run(&h, context(&h, "r2", vec![user("u")], "gpt-x")).await;
    assert_eq!(windows(&h, "b").await[0].used, money("0.0002"));
    let settlements = h
        .core
        .store()
        .quota_settlements()
        .query(quota_settlement::Entity::find())
        .await
        .unwrap();
    assert_eq!(settlements.len(), 2);
    // A request naming no owner is not budgeted at all.
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r3", vec![], "gpt-x")).await;
    assert_eq!(windows(&h, "b").await[0].used, money("0.0002"));
}

#[tokio::test]
async fn an_exhausted_budget_rejects_before_any_upstream_call() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_budgets(&h, vec![budget_row("b", user("u"), "5h", "0.0001", None)]).await;
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r1", vec![user("u")], "gpt-x")).await;
    // used == limit: spent.
    let error = h
        .core
        .stream_generate_content(context(&h, "r2", vec![user("u")], "gpt-x"), request("{}"))
        .await
        .unwrap_err();
    let CoreError::BudgetExhausted {
        quota_id,
        window_key,
        resets_at_ms,
    } = &error
    else {
        panic!("{error}")
    };
    assert_eq!((quota_id.as_str(), window_key.as_str()), ("b", "b-key"));
    assert_eq!(*resets_at_ms, windows(&h, "b").await[0].ends_at_ms);
    assert_eq!(h.client.seen.lines().len(), 1, "nothing was sent for r2");
    let reports = h.observer.reports.lock().unwrap().clone();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[1].request_id, "r2");
    assert_eq!(reports[1].state, UsageState::Failed);
    assert!(reports[1].exchanges.is_empty());
    assert!(
        h.observer
            .log
            .lines()
            .iter()
            .any(|l| l == "trace r2 budget-rejected b b-key"),
        "{:?}",
        h.observer.log.lines()
    );
    // The same owner is fine on another model the budget does not cover only
    // when the budget is scoped; this one is not.
    let error = h
        .core
        .stream_generate_content(
            context(&h, "r3", vec![user("u")], "claude-x"),
            request("{}"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::BudgetExhausted { .. }));
}

#[tokio::test]
async fn an_unpriced_model_settles_zero_and_is_flagged() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_budgets(&h, vec![budget_row("b", user("u"), "1d", "1", None)]).await;
    h.script(vec![usage_reply()]);
    let report = run(&h, context(&h, "r1", vec![user("u")], "claude-x")).await;
    assert!(report.cost.is_none());
    assert!(report.exchanges[0].cost.is_none());
    assert_eq!(
        report.exchanges[0]
            .usage
            .dimensions
            .get("unpriced")
            .map(String::as_str),
        Some("true")
    );
    let rows = windows(&h, "b").await;
    assert_eq!(rows[0].used, money("0"));
    let settled = h
        .core
        .store()
        .quota_settlements()
        .get_many(&[(rows[0].id.clone(), "r1".into())])
        .await
        .unwrap();
    assert_eq!(settled[0].as_ref().unwrap().amount, money("0"));
}

#[tokio::test]
async fn fixed_windows_roll_over_and_keep_history() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_budgets(&h, vec![budget_row("b", user("u"), "5h", "1", None)]).await;
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r1", vec![user("u")], "gpt-x")).await;
    let first = windows(&h, "b").await.remove(0);
    assert_eq!(first.used, money("0.0001"));
    let later = first.ends_at_ms.unwrap() + HOUR;
    let status = h.core.budget_status(&[user("u")], later).await.unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].used, Decimal::ZERO);
    assert_eq!(status[0].starts_at_ms, first.ends_at_ms.unwrap());
    assert_eq!(
        status[0].ends_at_ms,
        Some(first.ends_at_ms.unwrap() + 5 * HOUR)
    );
    assert_eq!(status[0].resets_at_ms, status[0].ends_at_ms);
    assert_eq!(status[0].limit, Decimal::ONE);
    assert_eq!(status[0].window_key, "b-key");
    let rows = windows(&h, "b").await;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter().find(|w| w.id == first.id).unwrap().used,
        money("0.0001")
    );
    // Status within the first window still reports the first window.
    let inside = h
        .core
        .budget_status(&[user("u")], first.starts_at_ms + 1)
        .await
        .unwrap();
    assert_eq!(inside[0].window_id, first.id);
    assert_eq!(windows(&h, "b").await.len(), 2, "no third window");
}

#[tokio::test]
async fn calendar_months_and_permanent_windows() {
    let h = harness(full(), "sticky").await;
    seed_budgets(
        &h,
        vec![
            budget_row("m", user("u"), "1m", "1", None),
            budget_row("t", user("u"), "total", "1", None),
        ],
    )
    .await;
    // 2026-09-19T10:00:00Z
    let september = 1_789_812_000_000;
    let status = h.core.budget_status(&[user("u")], september).await.unwrap();
    assert_eq!(status.len(), 2);
    let month = status.iter().find(|s| s.quota_id == "m").unwrap();
    assert_eq!(
        month.starts_at_ms, 1_788_220_800_000,
        "2026-09-01T00:00:00Z"
    );
    assert_eq!(
        month.ends_at_ms,
        Some(1_790_812_800_000),
        "2026-10-01T00:00:00Z"
    );
    let total = status.iter().find(|s| s.quota_id == "t").unwrap();
    assert_eq!(total.starts_at_ms, 0);
    assert_eq!(total.ends_at_ms, None);
    assert_eq!(total.resets_at_ms, None);
    // 2026-10-02T00:00:00Z: a new month window, the same permanent one.
    let october = 1_790_899_200_000;
    let status = h.core.budget_status(&[user("u")], october).await.unwrap();
    let month2 = status.iter().find(|s| s.quota_id == "m").unwrap();
    assert_eq!(month2.starts_at_ms, 1_790_812_800_000);
    assert_ne!(month2.window_id, month.window_id);
    assert_eq!(
        status.iter().find(|s| s.quota_id == "t").unwrap().window_id,
        total.window_id
    );
    assert_eq!(windows(&h, "m").await.len(), 2);
    assert_eq!(windows(&h, "t").await.len(), 1);
}

#[tokio::test]
async fn reset_closes_the_window_and_opens_a_fresh_one() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_budgets(
        &h,
        vec![
            budget_row("b", user("u"), "5h", "0.0001", None),
            budget_row("t", user("u"), "total", "0.0001", None),
        ],
    )
    .await;
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r1", vec![user("u")], "gpt-x")).await;
    let error = h
        .core
        .stream_generate_content(context(&h, "r2", vec![user("u")], "gpt-x"), request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::BudgetExhausted { .. }));
    let old = windows(&h, "b").await.remove(0);
    // A reset a moment ago: the execution path reads the wall clock, so the
    // reopened window must already contain it.
    let now = wall_ms() - 10;
    assert!(now > old.starts_at_ms);
    let status = h.core.reset_budget("b", now).await.unwrap();
    assert_eq!(status.used, Decimal::ZERO);
    assert_eq!(status.starts_at_ms, now);
    assert_eq!(status.ends_at_ms, Some(now + 5 * HOUR));
    let rows = windows(&h, "b").await;
    assert_eq!(rows.len(), 2);
    let closed = rows.iter().find(|w| w.id == old.id).unwrap();
    assert_eq!(closed.ends_at_ms, Some(now), "closed at the reset");
    assert_eq!(closed.used, money("0.0001"), "history kept");
    let quota = h
        .core
        .store()
        .quotas()
        .get_many(&["b".into()])
        .await
        .unwrap();
    assert_eq!(quota[0].as_ref().unwrap().anchor_at_ms, Some(now));
    let current = h.core.budget_status(&[user("u")], now + 1).await.unwrap();
    assert_eq!(
        current
            .iter()
            .find(|s| s.quota_id == "b")
            .unwrap()
            .window_id,
        status.window_id
    );
    // The permanent budget is still spent until its own reset.
    let error = h
        .core
        .stream_generate_content(context(&h, "r3", vec![user("u")], "gpt-x"), request("{}"))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::BudgetExhausted { quota_id, resets_at_ms: None, .. } if quota_id == "t")
    );
    let reset = h.core.reset_budget("t", now).await.unwrap();
    assert_eq!((reset.starts_at_ms, reset.ends_at_ms), (now, None));
    assert_eq!(windows(&h, "t").await.len(), 2);
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r3", vec![user("u")], "gpt-x")).await;
    assert_eq!(h.client.seen.lines().len(), 2);
    assert!(h.core.reset_budget("nope", now).await.is_err());
}

#[tokio::test]
async fn model_scoped_budgets_ignore_other_models() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_budgets(
        &h,
        vec![budget_row(
            "claude-only",
            user("u"),
            "1d",
            "0",
            Some("claude-*"),
        )],
    )
    .await;
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r1", vec![user("u")], "gpt-x")).await;
    assert!(windows(&h, "claude-only").await.is_empty(), "never opened");
    let error = h
        .core
        .stream_generate_content(
            context(&h, "r2", vec![user("u")], "claude-3"),
            request("{}"),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::BudgetExhausted { quota_id, .. } if quota_id == "claude-only")
    );
    // Status lists the scoped budget whatever the model.
    let status = h.core.budget_status(&[user("u")], 1).await.unwrap();
    assert_eq!(status[0].model_pattern.as_deref(), Some("claude-*"));
}

#[tokio::test]
async fn every_owner_must_have_room() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    h.core
        .store()
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("k".into()),
            user_id: Set("u".into()),
            name: Set("k".into()),
            key_hash: Set("h".into()),
            prefix: Set("sk".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    seed_budgets(
        &h,
        vec![
            budget_row("user", user("u"), "1d", "100", None),
            budget_row("key", owner("api_key", "k"), "1d", "0", None),
        ],
    )
    .await;
    let both = vec![user("u"), owner("api_key", "k")];
    let error = h
        .core
        .stream_generate_content(context(&h, "r1", both.clone(), "gpt-x"), request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(&error, CoreError::BudgetExhausted { quota_id, .. } if quota_id == "key"));
    assert!(h.client.seen.lines().is_empty());
    // The user alone is fine, and only the user's window is charged.
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r2", vec![user("u")], "gpt-x")).await;
    assert_eq!(windows(&h, "user").await[0].used, money("0.0001"));
    assert_eq!(windows(&h, "key").await[0].used, money("0"));
    let status = h.core.budget_status(&both, 1).await.unwrap();
    assert_eq!(
        status
            .iter()
            .map(|s| s.quota_id.as_str())
            .collect::<Vec<_>>(),
        ["key", "user"]
    );
}

/// Owner kinds are whatever the host says they are: a request charges the
/// whole chain it names, an exhausted budget anywhere in the chain rejects
/// it, and settlement writes one row per owner in the chain.
#[tokio::test]
async fn host_defined_owner_chains_charge_every_level() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_budgets(
        &h,
        vec![
            budget_row("k1", owner("api_key", "k1"), "1d", "100", None),
            budget_row("k2", owner("api_key", "k2"), "1d", "100", None),
            budget_row("u", user("u"), "1d", "100", None),
            budget_row("t", owner("team", "t"), "1d", "100", None),
            budget_row("o", owner("org", "o"), "1d", "0", None),
        ],
    )
    .await;
    let team_key = vec![
        owner("api_key", "k2"),
        user("u"),
        owner("team", "t"),
        owner("org", "o"),
    ];
    let error = h
        .core
        .stream_generate_content(context(&h, "r1", team_key.clone(), "gpt-x"), request("{}"))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::BudgetExhausted { quota_id, .. } if quota_id == "o"),
        "{error:?}"
    );
    assert!(h.client.seen.lines().is_empty());
    // The same user through a personal key is not under the org's budget.
    let personal_key = vec![owner("api_key", "k1"), user("u")];
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r2", personal_key, "gpt-x")).await;
    assert_eq!(windows(&h, "k1").await[0].used, money("0.0001"));
    assert_eq!(windows(&h, "u").await[0].used, money("0.0001"));
    assert!(windows(&h, "k2").await[0].used.decimal().is_zero());
    assert!(windows(&h, "t").await[0].used.decimal().is_zero());
    // Lift the org budget: the team key charges all four levels at once.
    h.core
        .store()
        .quotas()
        .update_many(vec![quota::ActiveModel {
            id: Set("o".into()),
            limit_value: Set(money("100")),
            ..Default::default()
        }])
        .await
        .unwrap();
    bump(&h).await;
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r3", team_key, "gpt-x")).await;
    for (quota_id, used) in [
        ("k1", "0.0001"),
        ("k2", "0.0001"),
        ("u", "0.0002"),
        ("t", "0.0001"),
        ("o", "0.0001"),
    ] {
        assert_eq!(
            windows(&h, quota_id).await[0].used,
            money(used),
            "{quota_id}"
        );
    }
    let settled_r3 = h
        .core
        .store()
        .quota_settlements()
        .query(
            quota_settlement::Entity::find().filter(quota_settlement::Column::RequestId.eq("r3")),
        )
        .await
        .unwrap();
    assert_eq!(settled_r3.len(), 4, "one settlement per owner in the chain");
    // Status reports the owner as the host named it.
    let status = h.core.budget_status(&[owner("org", "o")], 1).await.unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].owner, owner("org", "o"));
    assert_eq!(status[0].owner.to_string(), "org:o");
}

#[tokio::test]
async fn the_caller_service_view_renders_budget_windows() {
    use gproxy_channel::channel::{CallerRole, ServiceView};
    use gproxy_core::ServiceRequest;
    use gproxy_protocol::{HttpBody, WireRequest, connection::Bytes};
    let h = harness(full(), "sticky").await;
    h.channel
        .expose_services
        .store(true, std::sync::atomic::Ordering::Relaxed);
    seed_pricing(&h).await;
    seed_budgets(&h, vec![budget_row("b", user("u"), "5h", "0.0004", None)]).await;
    h.script(vec![usage_reply()]);
    run(&h, context(&h, "r1", vec![user("u")], "gpt-x")).await;
    let call = |budgets: Vec<BudgetOwner>| {
        h.core.call_service(ServiceRequest {
            cancellation: tokio_util::sync::CancellationToken::new(),
            scope: "tenant".into(),
            user_id: Some("u".into()),
            caller: CallerRole::Member,
            view: ServiceView::Caller,
            target: h.target("p"),
            budgets,
            request: WireRequest {
                method: http::Method::GET,
                path: "/usage".into(),
                query: None,
                headers: http::HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            },
        })
    };
    let body: serde_json::Value =
        serde_json::from_str(&read(call(vec![user("u")]).await.unwrap().body).await).unwrap();
    let window = windows(&h, "b").await.remove(0);
    assert_eq!(body["windows"].as_array().unwrap().len(), 1);
    assert_eq!(body["windows"][0]["key"], "b-key");
    assert_eq!(body["windows"][0]["used_percent"], 25.0);
    assert_eq!(body["windows"][0]["period_start_ms"], window.starts_at_ms);
    assert_eq!(
        body["windows"][0]["reset_at_ms"],
        window.ends_at_ms.unwrap()
    );
    let body: serde_json::Value =
        serde_json::from_str(&read(call(Vec::new()).await.unwrap().body).await).unwrap();
    assert!(
        body["windows"].as_array().unwrap().is_empty(),
        "no owners, no windows"
    );
}
