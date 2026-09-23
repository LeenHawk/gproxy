#![cfg(not(target_arch = "wasm32"))]

//! Operator limits on upstream credentials: `quotas` rows owned by a
//! credential or provider become Counted dimensions, charged before the
//! exchange (requests) or after settlement (USD cost), blocking the
//! credential until the window ends, reset by hand, and invisible to caller
//! budgets.
mod support;
use support::*;

use gproxy_channel::channel::{QuotaMetric, QuotaScope, QuotaTracking, QuotaWindow};
use gproxy_core::{BlockSource, BudgetOwner, CoreError, ExecutionTarget, RequestContext};
use gproxy_store::entity::{
    config::setting,
    limits::{counted_window, quota, quota_window},
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

fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}

/// One limit row. `metric` is `requests` (unit `count`) or `cost` (unit
/// `USD`); `owner` is `credential:<id>` or `provider:<id>`.
fn limit_row(
    id: &str,
    owner: (&str, &str),
    window_key: &str,
    metric: &str,
    period: &str,
    limit: &str,
    model: Option<&str>,
) -> quota::ActiveModel {
    quota::ActiveModel {
        id: Set(id.into()),
        owner_kind: Set(owner.0.into()),
        owner_id: Set(owner.1.into()),
        window_key: Set(window_key.into()),
        metric: Set(metric.into()),
        unit: Set(if metric == "cost" { "USD" } else { "count" }.into()),
        limit_value: Set(money(limit)),
        period: Set(period.into()),
        model_pattern: Set(model.map(Into::into)),
        ..Default::default()
    }
}

fn budget_row(id: &str, owner: BudgetOwner, period: &str, limit: &str) -> quota::ActiveModel {
    quota::ActiveModel {
        id: Set(id.into()),
        owner_kind: Set(owner.kind),
        owner_id: Set(owner.id),
        window_key: Set(format!("{id}-key")),
        metric: Set("cost".into()),
        unit: Set("USD".into()),
        limit_value: Set(money(limit)),
        period: Set(period.into()),
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

async fn seed_quotas(h: &Harness, rows: Vec<quota::ActiveModel>) {
    h.core.store().quotas().create_many(rows).await.unwrap();
    bump(h).await;
}

/// A request on provider `p` for `model`, restricted to `credential` when
/// given, charging `owners`.
fn context(
    h: &Harness,
    credential: Option<&str>,
    id: &str,
    model: &str,
    owners: Vec<BudgetOwner>,
) -> Arc<RequestContext> {
    let ctx = h.context(id, 2, None);
    let target = h.target("p");
    Arc::new(RequestContext {
        target: ExecutionTarget {
            requested_model: None,
            upstream_model: Some(model.into()),
            credentials: target
                .credentials
                .iter()
                .filter(|c| credential.is_none_or(|id| c.id == id))
                .cloned()
                .collect(),
            ..target
        },
        budgets: owners,
        ..(*ctx).clone()
    })
}

fn usage_reply() -> Reply {
    json_reply(
        StatusCode::OK,
        json!({"ok": 1, "usage": {"input_tokens": 4, "output_tokens": 2}}),
    )
}

/// Run one request to completion (so post-usage metering has happened).
async fn run(h: &Harness, ctx: Arc<RequestContext>) -> gproxy_core::UsageReport {
    h.script(vec![usage_reply()]);
    let execution = h
        .core
        .stream_generate_content(ctx, request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    completion.await.unwrap()
}

async fn refused(h: &Harness, ctx: Arc<RequestContext>) {
    let sent = h.client.seen.lines().len();
    let error = h
        .core
        .stream_generate_content(ctx, request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NoUsableCredential), "{error}");
    assert_eq!(h.client.seen.lines().len(), sent, "nothing was sent");
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

async fn cached_blocks(h: &Harness, credential_id: &str) -> gproxy_core::CredentialBlocks {
    h.core
        .cache()
        .get(&gproxy_core::keys::credential_blocks("p", credential_id))
        .await
        .unwrap()
        .map(|entry| serde_json::from_slice(&entry.value).unwrap())
        .unwrap_or_default()
}

async fn counted(h: &Harness, credential_id: &str) -> Vec<counted_window::Model> {
    h.core
        .store()
        .counted_windows()
        .query(
            counted_window::Entity::find()
                .filter(counted_window::Column::CredentialId.eq(credential_id)),
        )
        .await
        .unwrap()
}

fn credential(h: &Harness, id: &str) -> Arc<gproxy_core::CredentialData> {
    h.core.snapshot().credentials[id].clone()
}

#[tokio::test]
async fn a_provider_request_limit_blocks_each_credential_after_its_share() {
    let h = harness(full(), "sticky").await;
    seed_quotas(
        &h,
        vec![limit_row(
            "l",
            ("provider", "p"),
            "w",
            "requests",
            "5h",
            "2",
            None,
        )],
    )
    .await;
    // Both credentials of the provider carry the synthetic dimension.
    for id in ["a", "b"] {
        let c = credential(&h, id);
        assert_eq!(c.quota.len(), 1, "{id}");
        assert_eq!(c.quota[0].id, "limit:l");
        assert_eq!(c.quota[0].metric, QuotaMetric::Requests);
        assert_eq!(c.quota[0].tracking, QuotaTracking::Counted);
        assert_eq!(c.quota[0].scope, QuotaScope::All);
        assert_eq!(
            c.quota[0].window,
            QuotaWindow::Fixed {
                seconds: 5 * 3600,
                anchor_at_ms: Some(0)
            }
        );
        assert_eq!(c.quota[0].limit, Some(Decimal::from(2)));
        assert_eq!(c.limits.len(), 1);
    }
    assert_eq!(h.core.snapshot().credential_limits.len(), 1);
    assert!(
        h.core.snapshot().budgets.is_empty(),
        "a limit is not a budget"
    );
    run(&h, context(&h, Some("a"), "r1", "gpt-x", vec![])).await;
    run(&h, context(&h, Some("a"), "r2", "gpt-x", vec![])).await;
    // The second request landed on the limit: `a` is blocked until the
    // window ends.
    let rows = blocks_for(&h, "a").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source["kind"], "counted");
    assert_eq!(rows[0].source["dimension"], "limit:l");
    let window = counted(&h, "a").await.remove(0);
    assert_eq!((window.used, window.limit), (2, 2));
    assert_eq!(rows[0].until_ms, window.window_end_ms);
    assert_eq!(window.window_end_ms - window.window_start_ms, 5 * HOUR);
    refused(&h, context(&h, Some("a"), "r3", "gpt-x", vec![])).await;
    // The whole provider: `b` has its own two.
    run(&h, context(&h, None, "r4", "gpt-x", vec![])).await;
    assert!(h.client.seen.lines()[2].contains("auth=Bearer kb"));
    run(&h, context(&h, None, "r5", "gpt-x", vec![])).await;
    refused(&h, context(&h, None, "r6", "gpt-x", vec![])).await;
    let status = h
        .core
        .credential_limit_status("b", wall_ms())
        .await
        .unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].quota_id, "l");
    assert_eq!(status[0].owner, BudgetOwner::new("provider", "p"));
    assert_eq!(
        (status[0].metric.as_str(), status[0].unit.as_str()),
        ("requests", "count")
    );
    assert_eq!(
        (status[0].used, status[0].limit),
        (Decimal::TWO, Decimal::TWO)
    );
    assert_eq!(status[0].window_start_ms, window.window_start_ms);
    assert_eq!(status[0].window_end_ms, Some(window.window_end_ms));
}

#[tokio::test]
async fn a_credential_cost_limit_blocks_once_settled_cost_reaches_it() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_quotas(
        &h,
        vec![
            limit_row(
                "a-usd",
                ("credential", "a"),
                "w",
                "cost",
                "1d",
                "0.0002",
                None,
            ),
            limit_row(
                "b-usd",
                ("credential", "b"),
                "w",
                "cost",
                "1d",
                "0.0001",
                None,
            ),
        ],
    )
    .await;
    assert_eq!(credential(&h, "a").quota[0].metric, QuotaMetric::Cost);
    let report = run(&h, context(&h, Some("a"), "r1", "gpt-x", vec![])).await;
    assert_eq!(report.cost.unwrap().amount, dec("0.0001"));
    let window = counted(&h, "a").await.remove(0);
    assert_eq!((window.used, window.limit), (100_000, 200_000), "USD atoms");
    assert!(blocks_for(&h, "a").await.is_empty(), "under the limit");
    let status = h
        .core
        .credential_limit_status("a", wall_ms())
        .await
        .unwrap();
    assert_eq!(
        (status[0].used, status[0].limit),
        (dec("0.0001"), dec("0.0002"))
    );
    assert_eq!(
        (status[0].metric.as_str(), status[0].unit.as_str()),
        ("cost", "USD")
    );
    run(&h, context(&h, Some("a"), "r2", "gpt-x", vec![])).await;
    let rows = blocks_for(&h, "a").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source["dimension"], "limit:a-usd");
    refused(&h, context(&h, Some("a"), "r3", "gpt-x", vec![])).await;
    // An unpriced model charges nothing: `b` stays open however often it
    // is used.
    for id in ["r4", "r5"] {
        let report = run(&h, context(&h, Some("b"), id, "claude-x", vec![])).await;
        assert!(report.cost.is_none());
    }
    assert!(counted(&h, "b").await.is_empty());
    assert!(blocks_for(&h, "b").await.is_empty());
    assert_eq!(
        h.core
            .credential_limit_status("b", wall_ms())
            .await
            .unwrap()[0]
            .used,
        Decimal::ZERO
    );
}

#[tokio::test]
async fn a_credential_row_overrides_the_provider_row_with_the_same_window_key() {
    let h = harness(full(), "sticky").await;
    seed_quotas(
        &h,
        vec![
            limit_row("pw", ("provider", "p"), "w", "requests", "1d", "5", None),
            limit_row(
                "po",
                ("provider", "p"),
                "other",
                "requests",
                "1d",
                "9",
                None,
            ),
            limit_row("cw", ("credential", "a"), "w", "requests", "1d", "1", None),
        ],
    )
    .await;
    let ids = |id: &str| {
        credential(&h, id)
            .quota
            .iter()
            .map(|d| (d.id.clone(), d.limit.unwrap()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids("a"),
        [
            ("limit:cw".to_owned(), Decimal::ONE),
            ("limit:po".to_owned(), Decimal::from(9))
        ]
    );
    assert_eq!(
        ids("b"),
        [
            ("limit:po".to_owned(), Decimal::from(9)),
            ("limit:pw".to_owned(), Decimal::from(5))
        ]
    );
    run(&h, context(&h, Some("a"), "r1", "gpt-x", vec![])).await;
    refused(&h, context(&h, Some("a"), "r2", "gpt-x", vec![])).await;
    run(&h, context(&h, Some("b"), "r3", "gpt-x", vec![])).await;
    run(&h, context(&h, Some("b"), "r4", "gpt-x", vec![])).await;
    assert!(blocks_for(&h, "b").await.is_empty());
}

#[tokio::test]
async fn total_never_rolls_over_and_months_roll_at_utc_boundaries() {
    let h = harness(full(), "sticky").await;
    seed_quotas(
        &h,
        vec![
            limit_row(
                "t",
                ("credential", "a"),
                "t",
                "requests",
                "total",
                "100",
                None,
            ),
            limit_row("m", ("credential", "a"), "m", "requests", "1m", "100", None),
        ],
    )
    .await;
    assert_eq!(
        credential(&h, "a").quota[0].window,
        QuotaWindow::CalendarMonth
    );
    assert_eq!(credential(&h, "a").quota[1].window, QuotaWindow::Total);
    run(&h, context(&h, Some("a"), "r1", "gpt-x", vec![])).await;
    let now = wall_ms();
    let status = h.core.credential_limit_status("a", now).await.unwrap();
    assert_eq!(status.len(), 2);
    let month = &status[0];
    assert_eq!(month.quota_id, "m");
    assert_eq!(month.used, Decimal::ONE);
    let month_end = month.window_end_ms.unwrap();
    assert!(month.window_start_ms <= now && now < month_end);
    let total = &status[1];
    assert_eq!(total.quota_id, "t");
    assert_eq!(total.used, Decimal::ONE);
    assert_eq!((total.window_start_ms, total.window_end_ms), (0, None));
    // Two years on: the month window is a new, empty one; the permanent
    // window still holds the request.
    let later = now + 730 * 24 * HOUR;
    let status = h.core.credential_limit_status("a", later).await.unwrap();
    assert_eq!(status[0].used, Decimal::ZERO);
    assert!(status[0].window_start_ms >= month_end);
    assert_eq!(status[1].used, Decimal::ONE);
    // Just past this month's end as well.
    let status = h
        .core
        .credential_limit_status("a", month_end)
        .await
        .unwrap();
    assert_eq!(status[0].used, Decimal::ZERO);
    assert_eq!(status[0].window_start_ms, month_end);
}

#[tokio::test]
async fn reset_clears_the_block_and_starts_a_fresh_window() {
    let h = harness(full(), "sticky").await;
    seed_quotas(
        &h,
        vec![
            limit_row("l", ("credential", "a"), "w", "requests", "5h", "1", None),
            limit_row(
                "t",
                ("credential", "a"),
                "t",
                "requests",
                "total",
                "1",
                None,
            ),
        ],
    )
    .await;
    run(&h, context(&h, Some("a"), "r1", "gpt-x", vec![])).await;
    assert_eq!(blocks_for(&h, "a").await.len(), 2);
    assert_eq!(cached_blocks(&h, "a").await.blocks.len(), 2);
    refused(&h, context(&h, Some("a"), "r2", "gpt-x", vec![])).await;
    let now = wall_ms();
    assert_eq!(
        h.core.reset_credential_limit("l", now).await.unwrap(),
        ["a"]
    );
    // Only the reset limit's block and window are gone.
    let rows = blocks_for(&h, "a").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source["dimension"], "limit:t");
    let cached = cached_blocks(&h, "a").await;
    assert_eq!(cached.blocks.len(), 1);
    assert!(matches!(
        &cached.blocks[0].source,
        BlockSource::Counted { dimension } if dimension == "limit:t"
    ));
    let windows = counted(&h, "a").await;
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].dimension, "limit:t");
    let quota = h
        .core
        .store()
        .quotas()
        .get_many(&["l".into()])
        .await
        .unwrap();
    assert_eq!(quota[0].as_ref().unwrap().anchor_at_ms, Some(now));
    let status = h.core.credential_limit_status("a", now).await.unwrap();
    assert_eq!(status[0].used, Decimal::ZERO);
    assert_eq!(status[1].used, Decimal::ONE);
    // Still refused: the permanent limit is spent until its own reset.
    refused(&h, context(&h, Some("a"), "r3", "gpt-x", vec![])).await;
    h.core.reset_credential_limit("t", now).await.unwrap();
    assert!(blocks_for(&h, "a").await.is_empty());
    assert!(cached_blocks(&h, "a").await.blocks.is_empty());
    run(&h, context(&h, Some("a"), "r3", "gpt-x", vec![])).await;
    assert_eq!(h.client.seen.lines().len(), 2);
    // The permanent window restarted: the count is fresh at once, and once
    // the snapshot is reloaded its start is the reset and fixed windows
    // align to the new anchor.
    let status = h.core.credential_limit_status("a", now + 1).await.unwrap();
    assert_eq!(status[1].used, Decimal::ONE);
    assert_eq!(status[1].window_start_ms, 0, "old anchor until reload");
    bump(&h).await;
    let status = h.core.credential_limit_status("a", now + 1).await.unwrap();
    assert_eq!(status[1].used, Decimal::ONE);
    assert_eq!(status[1].window_start_ms, now);
    assert_eq!(
        credential(&h, "a").quota[0].window,
        QuotaWindow::Fixed {
            seconds: 5 * 3600,
            anchor_at_ms: Some(now)
        }
    );
    assert!(h.core.reset_credential_limit("nope", now).await.is_err());
}

#[tokio::test]
async fn model_scoped_limits_ignore_other_models() {
    let h = harness(full(), "sticky").await;
    seed_quotas(
        &h,
        vec![
            limit_row(
                "claude",
                ("credential", "a"),
                "claude",
                "requests",
                "1d",
                "1",
                Some("claude-*"),
            ),
            limit_row(
                "exact",
                ("credential", "b"),
                "exact",
                "requests",
                "1d",
                "1",
                Some("gpt-x"),
            ),
        ],
    )
    .await;
    // A glob stays a charge-time filter on an `All` dimension; an exact
    // name is the dimension's scope.
    assert_eq!(credential(&h, "a").quota[0].scope, QuotaScope::All);
    assert_eq!(
        credential(&h, "b").quota[0].scope,
        QuotaScope::Models(vec!["gpt-x".into()])
    );
    run(&h, context(&h, Some("a"), "r1", "gpt-x", vec![])).await;
    run(&h, context(&h, Some("a"), "r2", "gpt-x", vec![])).await;
    assert!(counted(&h, "a").await.is_empty(), "never charged");
    run(&h, context(&h, Some("a"), "r3", "claude-3", vec![])).await;
    refused(&h, context(&h, Some("a"), "r4", "claude-3", vec![])).await;
    // The block names the charged model, not every model.
    let rows = blocks_for(&h, "a").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].scope, json!({"models": ["claude-3"]}));
    run(&h, context(&h, Some("a"), "r5", "gpt-x", vec![])).await;
    // Another model under the glob shares the window and is refused too.
    refused(&h, context(&h, Some("a"), "r6", "claude-4", vec![])).await;
    assert_eq!(blocks_for(&h, "a").await.len(), 2);
    let status = h
        .core
        .credential_limit_status("a", wall_ms())
        .await
        .unwrap();
    assert_eq!(status[0].model_pattern.as_deref(), Some("claude-*"));
    assert_eq!(status[0].used, Decimal::ONE);
    // The exact limit on `b`.
    run(&h, context(&h, Some("b"), "r7", "gpt-x-mini", vec![])).await;
    run(&h, context(&h, Some("b"), "r8", "gpt-x", vec![])).await;
    refused(&h, context(&h, Some("b"), "r9", "gpt-x", vec![])).await;
    run(&h, context(&h, Some("b"), "r10", "gpt-x-mini", vec![])).await;
}

#[tokio::test]
async fn budgets_and_limits_do_not_see_each_other() {
    let h = harness(full(), "sticky").await;
    seed_pricing(&h).await;
    seed_quotas(
        &h,
        vec![
            budget_row("b", BudgetOwner::new("user", "u"), "1d", "100"),
            limit_row("l", ("credential", "a"), "w", "cost", "1d", "0.0001", None),
        ],
    )
    .await;
    let snapshot = h.core.snapshot();
    assert_eq!(snapshot.budgets.len(), 1);
    assert_eq!(snapshot.budgets[0].quota.id, "b");
    assert_eq!(snapshot.credential_limits.len(), 1);
    assert_eq!(snapshot.credential_limits[0].quota.id, "l");
    assert!(credential(&h, "b").quota.is_empty());
    let user = vec![BudgetOwner::new("user", "u")];
    run(&h, context(&h, Some("a"), "r1", "gpt-x", user.clone())).await;
    // The budget settled the cost; the limit counted it; neither opened
    // anything for the other.
    let windows = h
        .core
        .store()
        .quota_windows()
        .query(quota_window::Entity::find())
        .await
        .unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].quota_id, "b");
    assert_eq!(windows[0].used, money("0.0001"));
    let counted = counted(&h, "a").await;
    assert_eq!(counted.len(), 1);
    assert_eq!(counted[0].dimension, "limit:l");
    assert_eq!(counted[0].used, 100_000);
    // `a` is spent; the same budgeted user goes on through `b`.
    refused(&h, context(&h, Some("a"), "r2", "gpt-x", user.clone())).await;
    run(&h, context(&h, None, "r3", "gpt-x", user.clone())).await;
    assert!(h.client.seen.lines()[1].contains("auth=Bearer kb"));
    assert_eq!(
        h.core
            .store()
            .quota_windows()
            .query(quota_window::Entity::find())
            .await
            .unwrap()[0]
            .used,
        money("0.0002")
    );
    let status = h.core.budget_status(&user, 1).await.unwrap();
    assert_eq!(status.len(), 1);
    assert!(h.core.reset_budget("l", 1).await.is_err());
    assert!(h.core.reset_credential_limit("b", 1).await.is_err());
    assert!(
        h.core
            .credential_limit_status("b", 1)
            .await
            .unwrap()
            .is_empty()
    );
}
