//! Reading back usage and captured requests.
//!
//! Rows are written straight through `Store`, the way core's observer writes
//! them: the aggregates are the thing under test, and a test that produced its
//! fixtures by making real calls would only be testing the call path again.
//! The `metrics` documents below are the shape `gproxy_core::StoreObserver`
//! writes — normalized totals, the per-exchange breakdown, the settlement
//! state and the priced cost.

mod support;

use gproxy_sdk::{
    Gproxy,
    dto::{
        ListQuery, LogBodyEncoding, LogQuery, UsageGroupBy, UsageGroupQuery, UsageQuery,
        UsageRecordQuery, UsageTrendQuery,
    },
};
use gproxy_store::entity::usage::{capture_event, capture_record, usage_record};
use sea_orm::{DatabaseConnection, Set};
use serde_json::{Value, json};

type Handle = Gproxy<DatabaseConnection>;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// One upstream attempt inside a request: provider, tokens and priced cost.
type Exchange<'a> = (&'a str, u64, u64, Option<&'a str>);

/// One usage record. Defaults are the uninteresting case, so a test only
/// spells out the columns it is about.
struct Seed<'a> {
    request_id: &'a str,
    started_at_ms: i64,
    user_id: Option<&'a str>,
    api_key_id: Option<&'a str>,
    model: &'a str,
    operation: &'a str,
    /// input, output, cached input, reasoning.
    tokens: (u64, u64, u64, u64),
    /// The settled charge, which is both the indexed column and the document.
    cost: Option<&'a str>,
    exchanges: &'a [Exchange<'a>],
}

impl Default for Seed<'_> {
    fn default() -> Self {
        Self {
            request_id: "r-1",
            started_at_ms: 0,
            user_id: None,
            api_key_id: None,
            model: "m-1",
            operation: "generate_content",
            tokens: (0, 0, 0, 0),
            cost: None,
            exchanges: &[],
        }
    }
}

/// The `usage_json` shape core writes, at the top level and per exchange.
fn usage_json(tokens: (u64, u64, u64, u64)) -> Value {
    let (input, output, cached, reasoning) = tokens;
    json!({
        "tokens": {
            "input_tokens": input,
            "output_tokens": output,
            "cached_input_tokens": cached,
            "cache_creation_5m_tokens": null,
            "cache_creation_30m_tokens": null,
            "cache_creation_1h_tokens": null,
            "reasoning_tokens": reasoning,
        },
        "metrics": {},
        "dimensions": {},
        "actual_service_tier": null,
        "completeness": "complete",
        "attempts": [],
        "responses": [],
    })
}

fn money(amount: Option<&str>) -> Value {
    match amount {
        Some(amount) => json!({"amount": amount, "currency": "USD"}),
        None => Value::Null,
    }
}

async fn usage(gproxy: &Handle, seed: Seed<'_>) {
    let exchanges = json!(
        seed.exchanges
            .iter()
            .enumerate()
            .map(|(index, (provider, input, output, cost))| json!({
                "capture_id": format!("{}-x{index}", seed.request_id),
                "attempt_id": format!("{}-a{index}", seed.request_id),
                "attempt_ordinal": index,
                "provider_id": provider,
                "credential_id": format!("c-{provider}"),
                "model": seed.model,
                "usage": usage_json((*input, *output, 0, 0)),
                "cost": money(*cost),
            }))
            .collect::<Vec<_>>()
    );
    insert(gproxy, seed, exchanges).await;
}

/// One upstream attempt spelled out in full, for the tests that need a
/// credential, an upstream model or a currency the provider-keyed [`Exchange`]
/// tuple cannot say.
fn attempt(credential: &str, model: &str, cost: Option<(&str, &str)>) -> Value {
    json!({
        "provider_id": "p-1",
        "credential_id": credential,
        "model": model,
        "usage": usage_json((1, 1, 0, 0)),
        "cost": cost.map_or(Value::Null, |(amount, currency)| {
            json!({"amount": amount, "currency": currency})
        }),
    })
}

/// A record with the given `exchanges[]` document, verbatim.
async fn insert(gproxy: &Handle, seed: Seed<'_>, exchanges: Value) {
    let base = usage_record::ActiveModel {
        request_id: Set(seed.request_id.into()),
        user_id: Set(seed.user_id.map(str::to_owned)),
        api_key_id: Set(seed.api_key_id.map(str::to_owned)),
        model: Set(seed.model.into()),
        operation: Set(seed.operation.into()),
        side: Set(capture_record::CaptureSide::Downstream),
        downstream_request_id: Set(None),
        provider_id: Set(None),
        credential_id: Set(None),
        attempt_id: Set(None),
        attempt_ordinal: Set(None),
        input_tokens: Set(Some(seed.tokens.0 as i64)),
        output_tokens: Set(Some(seed.tokens.1 as i64)),
        cached_input_tokens: Set(Some(seed.tokens.2 as i64)),
        reasoning_tokens: Set(Some(seed.tokens.3 as i64)),
        state: Set(Some("settled".into())),
        completeness: Set(Some("complete".into())),
        metrics: Set(json!({})),
        cost: Set(seed.cost.map(|c| c.parse().unwrap())),
        started_at_ms: Set(seed.started_at_ms),
        ended_at_ms: Set(Some(seed.started_at_ms + 100)),
        ..Default::default()
    };
    let mut rows = vec![base.clone()];
    for (index, exchange) in exchanges.as_array().unwrap().iter().enumerate() {
        let text = |key: &str| exchange[key].as_str().map(str::to_owned);
        let token = |key: &str| exchange["usage"]["tokens"][key].as_i64();
        rows.push(usage_record::ActiveModel {
            request_id: Set(
                text("capture_id").unwrap_or_else(|| format!("{}-x{index}", seed.request_id))
            ),
            side: Set(capture_record::CaptureSide::Upstream),
            downstream_request_id: Set(Some(seed.request_id.into())),
            provider_id: Set(text("provider_id")),
            credential_id: Set(text("credential_id")),
            model: Set(text("model").unwrap_or_default()),
            attempt_id: Set(text("attempt_id")),
            attempt_ordinal: Set(exchange["attempt_ordinal"].as_i64()),
            input_tokens: Set(token("input_tokens")),
            output_tokens: Set(token("output_tokens")),
            cached_input_tokens: Set(token("cached_input_tokens")),
            reasoning_tokens: Set(token("reasoning_tokens")),
            cost: Set(exchange["cost"]["amount"]
                .as_str()
                .map(|s| s.parse().unwrap())),
            ..base.clone()
        });
    }
    gproxy
        .store()
        .usage_records()
        .create_many(rows)
        .await
        .unwrap();
}

/// Three records: two users, two models, two providers, one request that
/// reached no upstream at all.
async fn three_records(gproxy: &Handle) {
    usage(
        gproxy,
        Seed {
            request_id: "r-1",
            started_at_ms: 1_000,
            user_id: Some("u-1"),
            api_key_id: Some("k-1"),
            model: "m-1",
            tokens: (10, 20, 5, 3),
            cost: Some("0.5"),
            exchanges: &[("p-1", 10, 20, Some("0.5"))],
            ..Default::default()
        },
    )
    .await;
    usage(
        gproxy,
        Seed {
            request_id: "r-2",
            started_at_ms: 2_000,
            user_id: Some("u-1"),
            api_key_id: Some("k-2"),
            model: "m-2",
            tokens: (100, 200, 0, 0),
            cost: Some("1.25"),
            // A failover: two providers served one request, and each brought
            // its own tokens and its own share of the price.
            exchanges: &[("p-1", 40, 80, Some("0.25")), ("p-2", 60, 120, Some("1"))],
            ..Default::default()
        },
    )
    .await;
    usage(
        gproxy,
        Seed {
            request_id: "r-3",
            started_at_ms: 3_000,
            user_id: Some("u-2"),
            api_key_id: Some("k-1"),
            model: "m-1",
            operation: "create_embedding",
            tokens: (1, 2, 0, 0),
            ..Default::default()
        },
    )
    .await;
}

/// One captured exchange. Everything the query does not filter on is left at
/// the database's own defaults.
async fn capture(gproxy: &Handle, row: capture_record::ActiveModel) {
    gproxy
        .store()
        .capture_records()
        .create_many(vec![row])
        .await
        .unwrap();
}

fn downstream(id: &str, started_at_ms: i64) -> capture_record::ActiveModel {
    capture_record::ActiveModel {
        id: Set(id.into()),
        side: Set(capture_record::CaptureSide::Downstream),
        kind: Set(capture_record::CaptureKind::Http),
        started_at_ms: Set(started_at_ms),
        state: Set(capture_record::CaptureState::Completed),
        request_body_state: Set(capture_record::CaptureBodyState::NotCaptured),
        response_body_state: Set(capture_record::CaptureBodyState::NotCaptured),
        ..Default::default()
    }
}

fn upstream(id: &str, initiator: &str, started_at_ms: i64) -> capture_record::ActiveModel {
    capture_record::ActiveModel {
        id: Set(id.into()),
        initiator_request_id: Set(Some(initiator.into())),
        side: Set(capture_record::CaptureSide::Upstream),
        kind: Set(capture_record::CaptureKind::Http),
        started_at_ms: Set(started_at_ms),
        state: Set(capture_record::CaptureState::Completed),
        request_body_state: Set(capture_record::CaptureBodyState::NotCaptured),
        response_body_state: Set(capture_record::CaptureBodyState::NotCaptured),
        ..Default::default()
    }
}

async fn event(gproxy: &Handle, capture_id: &str, sequence: i64, payload: &[u8]) {
    gproxy
        .store()
        .capture_events()
        .create_many(vec![capture_event::ActiveModel {
            capture_id: Set(capture_id.into()),
            sequence: Set(sequence),
            direction: Set(capture_event::CaptureDirection::Response),
            kind: Set(capture_event::CaptureEventKind::Bytes),
            payload: Set(payload.to_vec()),
            observed_at_ms: Set(sequence),
            ..Default::default()
        }])
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Usage records
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_credential_cut_counts_only_that_credentials_attempts() {
    let gproxy = support::sdk().await;
    three_records(&gproxy).await;
    let query = gproxy.query();
    let usage = query.usage();
    let for_credential = |credential: &str| UsageQuery {
        credential_id: Some(credential.into()),
        ..Default::default()
    };

    // r-2 failed over from c-p-1 to c-p-2. Each credential is charged its own
    // attempt, never the request's 1.25 settled total.
    let first = usage.summary(for_credential("c-p-1")).await.unwrap();
    assert_eq!(first.requests, 2, "r-1 and the first attempt of r-2");
    assert_eq!(first.input_tokens, 10 + 40);
    assert_eq!(first.output_tokens, 20 + 80);
    assert_eq!(first.cost, "0.75");
    assert_eq!(first.currency.as_deref(), Some("USD"));
    assert_eq!(first.scanned, 2, "SQL filters out unrelated requests");
    let second = usage.summary(for_credential("c-p-2")).await.unwrap();
    assert_eq!(second.requests, 1);
    assert_eq!(second.input_tokens, 60);
    assert_eq!(second.cost, "1");
    let nobody = usage.summary(for_credential("c-none")).await.unwrap();
    assert_eq!(nobody.requests, 0);
    assert_eq!(nobody.cost, "0");

    // The provider filter is the same cut keyed differently.
    let provider = usage
        .summary(UsageQuery {
            provider_id: Some("p-2".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!((provider.requests, provider.cost.as_str()), (1, "1"));

    // Grouped by credential, the groups add back up to the whole summary.
    let whole = usage.summary(UsageQuery::default()).await.unwrap();
    let by_credential = usage
        .group(UsageGroupQuery {
            filter: UsageQuery::default(),
            group_by: UsageGroupBy::Credential,
        })
        .await
        .unwrap();
    assert_eq!(
        by_credential
            .iter()
            .map(|group| (group.key.as_deref(), group.summary.cost.as_str()))
            .collect::<Vec<_>>(),
        [(Some("c-p-2"), "1"), (Some("c-p-1"), "0.75"), (None, "0")]
    );
    let sum = |field: fn(&gproxy_sdk::dto::UsageSummaryDto) -> u64| {
        by_credential
            .iter()
            .map(|group| field(&group.summary))
            .sum::<u64>()
    };
    assert_eq!(sum(|s| s.input_tokens), whole.input_tokens);
    assert_eq!(sum(|s| s.output_tokens), whole.output_tokens);
    let cost: rust_decimal::Decimal = by_credential
        .iter()
        .map(|group| group.summary.cost.parse::<rust_decimal::Decimal>().unwrap())
        .sum();
    assert_eq!(cost.to_string(), whole.cost);

    // Under a credential filter every other cut applies the same rule: a
    // column group sees only the attempt's share, and a per-credential group
    // sees only the filtered credential.
    let by_user = usage
        .group(UsageGroupQuery {
            filter: for_credential("c-p-2"),
            group_by: UsageGroupBy::User,
        })
        .await
        .unwrap();
    assert_eq!(by_user.len(), 1);
    assert_eq!(by_user[0].key.as_deref(), Some("u-1"));
    assert_eq!(by_user[0].summary.cost, "1");
    assert_eq!(by_user[0].summary.input_tokens, 60);
    let filtered_by_credential = usage
        .group(UsageGroupQuery {
            filter: for_credential("c-p-1"),
            group_by: UsageGroupBy::Credential,
        })
        .await
        .unwrap();
    assert_eq!(filtered_by_credential.len(), 1, "no None group of r-3");
    assert_eq!(filtered_by_credential[0].summary.cost, "0.75");

    let trend = usage
        .trend(UsageTrendQuery {
            filter: UsageQuery {
                from_ms: Some(0),
                to_ms: Some(4_000),
                ..for_credential("c-p-1")
            },
            bucket_ms: 1_000,
        })
        .await
        .unwrap();
    assert_eq!(
        trend
            .iter()
            .map(|point| point.summary.cost.as_str())
            .collect::<Vec<_>>(),
        ["0", "0.5", "0.25", "0"]
    );

    // The record list keeps its order and paging under the scan, and returns
    // matching records whole.
    let records = usage
        .records(UsageRecordQuery {
            credential_id: Some("c-p-1".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        records
            .items
            .iter()
            .map(|item| item.request_id.as_str())
            .collect::<Vec<_>>(),
        ["r-2", "r-1"]
    );
    assert_eq!(records.total, 2);
    assert!(!records.truncated);
    assert_eq!(
        records.items[0].exchanges.len(),
        2,
        "the record, not a share"
    );
    let second_page = usage
        .records(UsageRecordQuery {
            credential_id: Some("c-p-1".into()),
            page: Some(2),
            page_size: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(second_page.total, 2);
    assert_eq!(second_page.offset, 1);
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(second_page.items[0].request_id, "r-1");
    let only_second = usage
        .records(UsageRecordQuery {
            credential_id: Some("c-p-2".into()),
            user_id: Some("u-1".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(only_second.total, 1);
    assert_eq!(only_second.items[0].request_id, "r-2");
}

#[tokio::test]
async fn a_credentials_usd_spend_is_summed_over_its_own_attempts_in_range() {
    use gproxy_channel::channel::QuotaScope;
    use gproxy_core::usage_scan::credential_usd_cost;
    use rust_decimal::Decimal;

    let gproxy = support::sdk().await;
    let rows: [(&str, i64, Value); 4] = [
        // Before the range: not counted however large.
        (
            "early",
            999,
            json!([attempt("c-x", "claude-sonnet-4", Some(("100", "USD")))]),
        ),
        // Another credential's attempt in the same request is not ours.
        (
            "a",
            1_000,
            json!([
                attempt("c-y", "claude-sonnet-4", Some(("2", "USD"))),
                attempt("c-x", "claude-sonnet-4", Some(("1.5", "USD"))),
            ]),
        ),
        // Unpriced attempts add no cost, while remaining part of the history.
        (
            "b",
            2_000,
            json!([
                attempt("c-x", "gpt-5", Some(("0.25", "USD"))),
                attempt("c-x", "claude-opus-4", None),
                attempt("c-x", "claude-sonnet-4", None),
            ]),
        ),
        // `to_ms` is exclusive.
        (
            "late",
            3_000,
            json!([attempt("c-x", "claude-sonnet-4", Some(("4", "USD")))]),
        ),
    ];
    for (request_id, started_at_ms, exchanges) in rows {
        insert(
            &gproxy,
            Seed {
                request_id,
                started_at_ms,
                ..Default::default()
            },
            exchanges,
        )
        .await;
    }
    let store = gproxy.store();
    let dec = |text: &str| text.parse::<Decimal>().unwrap();

    let all = credential_usd_cost(store, "c-x", 1_000, 3_000, |_| true, u64::MAX)
        .await
        .unwrap();
    assert_eq!(all, (dec("1.75"), false));

    let sonnet = QuotaScope::ModelPrefixes(vec!["claude-sonnet".into()]);
    let scoped = credential_usd_cost(
        store,
        "c-x",
        1_000,
        3_000,
        |model| model.is_some_and(|model| sonnet.matches(model)),
        u64::MAX,
    )
    .await
    .unwrap();
    assert_eq!(scoped, (dec("1.5"), false));

    // The cap is on rows read, oldest first, and reaching it is said.
    let capped = credential_usd_cost(store, "c-x", 1_000, 3_000, |_| true, 1)
        .await
        .unwrap();
    assert_eq!(capped, (dec("1.5"), true));
    let exact = credential_usd_cost(store, "c-x", 1_000, 3_000, |_| true, 4)
        .await
        .unwrap();
    assert_eq!(
        exact,
        (dec("1.75"), false),
        "a cap at the row count is not a cut"
    );

    // The SDK's credential cut agrees with core's sum on the same range.
    let summary = gproxy
        .query()
        .usage()
        .summary(UsageQuery {
            from_ms: Some(1_000),
            to_ms: Some(3_000),
            credential_id: Some("c-x".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(summary.requests, 2);
    assert_eq!(
        summary.currency.as_deref(),
        Some("USD"),
        "priced usage is always USD"
    );
}

#[tokio::test]
async fn records_page_newest_first_and_honour_every_filter() {
    let gproxy = support::sdk().await;
    three_records(&gproxy).await;
    let usage = gproxy.query();
    let usage = usage.usage();

    let first = usage
        .records(UsageRecordQuery {
            page_size: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(first.total, 3, "the count is of matches, not of the page");
    assert_eq!(
        first
            .items
            .iter()
            .map(|item| item.request_id.as_str())
            .collect::<Vec<_>>(),
        ["r-3", "r-2"],
        "newest first"
    );
    let second = usage
        .records(UsageRecordQuery {
            page: Some(2),
            page_size: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].request_id, "r-1");

    // The extracted fields, and the document they were extracted from.
    let record = &second.items[0];
    assert_eq!(record.tokens.input_tokens, Some(10));
    assert_eq!(record.tokens.reasoning_tokens, Some(3));
    assert_eq!(record.cost.as_deref(), Some("0.5"));
    assert_eq!(record.currency.as_deref(), Some("USD"));
    assert_eq!(record.state.as_deref(), Some("settled"));
    assert_eq!(record.exchanges.len(), 1);
    assert_eq!(record.exchanges[0].provider_id.as_deref(), Some("p-1"));
    assert!(record.metrics.get("exchanges").is_none());

    for (query, expected) in [
        (
            UsageRecordQuery {
                user_id: Some("u-1".into()),
                ..Default::default()
            },
            vec!["r-2", "r-1"],
        ),
        (
            UsageRecordQuery {
                api_key_id: Some("k-1".into()),
                ..Default::default()
            },
            vec!["r-3", "r-1"],
        ),
        (
            UsageRecordQuery {
                model: Some("m-2".into()),
                ..Default::default()
            },
            vec!["r-2"],
        ),
        (
            UsageRecordQuery {
                operation: Some("create_embedding".into()),
                ..Default::default()
            },
            vec!["r-3"],
        ),
        (
            UsageRecordQuery {
                request_id: Some("r-2".into()),
                ..Default::default()
            },
            vec!["r-2"],
        ),
        // Half-open: 1000 is inside the range and 3000 is not.
        (
            UsageRecordQuery {
                from_ms: Some(1_000),
                to_ms: Some(3_000),
                ..Default::default()
            },
            vec!["r-2", "r-1"],
        ),
        (
            UsageRecordQuery {
                user_id: Some("nobody".into()),
                ..Default::default()
            },
            vec![],
        ),
    ] {
        let page = usage.records(query).await.unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|item| item.request_id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[tokio::test]
async fn a_summary_adds_up_to_the_seeded_numbers() {
    let gproxy = support::sdk().await;
    three_records(&gproxy).await;

    let summary = gproxy
        .query()
        .usage()
        .summary(UsageQuery::default())
        .await
        .unwrap();
    assert_eq!(summary.requests, 3);
    assert_eq!(summary.input_tokens, 10 + 100 + 1);
    assert_eq!(summary.output_tokens, 20 + 200 + 2);
    assert_eq!(summary.cached_input_tokens, 5);
    assert_eq!(summary.reasoning_tokens, 3);
    assert_eq!(summary.cost, "1.75");
    assert_eq!(summary.currency.as_deref(), Some("USD"));
    assert!(!summary.truncated);
    assert_eq!(summary.scanned, 3);

    // The same filters as the record list, over the same rows.
    let one_user = gproxy
        .query()
        .usage()
        .summary(UsageQuery {
            user_id: Some("u-2".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(one_user.requests, 1);
    assert_eq!(one_user.input_tokens, 1);
    assert_eq!(
        one_user.cost, "0",
        "a request nobody priced contributes nothing rather than nothing at all"
    );
    assert_eq!(
        one_user.currency, None,
        "no record in this scan carried a currency"
    );
}

#[tokio::test]
async fn the_scan_cap_is_reported_rather_than_hidden() {
    let gproxy = support::sdk().await;
    three_records(&gproxy).await;
    let query = gproxy.query();
    let usage = query.usage();

    let capped = usage
        .summary(UsageQuery {
            max_scan_rows: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(capped.truncated, "the cap stopped the scan short");
    assert_eq!(capped.scanned, 2);
    assert_eq!(capped.requests, 2);
    // Ascending scan order: the two oldest records, so 10 + 100 input.
    assert_eq!(capped.input_tokens, 110);

    // A cap exactly at the row count is not a truncation: the scan reads one
    // row past its budget precisely so it can tell the two apart.
    let exact = usage
        .summary(UsageQuery {
            max_scan_rows: Some(3),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!exact.truncated);
    assert_eq!(exact.scanned, 3);
}

#[tokio::test]
async fn groups_cut_the_same_totals_by_column_and_by_provider() {
    let gproxy = support::sdk().await;
    three_records(&gproxy).await;
    let query = gproxy.query();
    let usage = query.usage();

    let by_user = usage
        .group(UsageGroupQuery {
            filter: UsageQuery::default(),
            group_by: UsageGroupBy::User,
        })
        .await
        .unwrap();
    // Ordered by cost descending.
    assert_eq!(
        by_user
            .iter()
            .map(|group| group.key.as_deref())
            .collect::<Vec<_>>(),
        [Some("u-1"), Some("u-2")]
    );
    assert_eq!(by_user[0].summary.requests, 2);
    assert_eq!(by_user[0].summary.input_tokens, 110);
    assert_eq!(by_user[0].summary.cost, "1.75");
    assert_eq!(by_user[1].summary.requests, 1);
    assert_eq!(by_user[1].summary.cost, "0");

    let by_model = usage
        .group(UsageGroupQuery {
            filter: UsageQuery::default(),
            group_by: UsageGroupBy::Model,
        })
        .await
        .unwrap();
    assert_eq!(
        by_model
            .iter()
            .map(|group| group.key.as_deref())
            .collect::<Vec<_>>(),
        [Some("m-2"), Some("m-1")]
    );
    assert_eq!(by_model[0].summary.cost, "1.25");
    assert_eq!(by_model[1].summary.requests, 2, "r-1 and r-3");
    assert_eq!(by_model[1].summary.input_tokens, 11);

    // Provider comes out of the per-exchange breakdown, so the failed-over
    // request contributes to both providers with each one's own numbers, and
    // the request that reached nobody lands under the empty key.
    let by_provider = usage
        .group(UsageGroupQuery {
            filter: UsageQuery::default(),
            group_by: UsageGroupBy::Provider,
        })
        .await
        .unwrap();
    assert_eq!(
        by_provider
            .iter()
            .map(|group| group.key.as_deref())
            .collect::<Vec<_>>(),
        [Some("p-2"), Some("p-1"), None]
    );
    assert_eq!(by_provider[0].summary.requests, 1);
    assert_eq!(by_provider[0].summary.cost, "1");
    assert_eq!(by_provider[1].summary.requests, 2);
    assert_eq!(by_provider[1].summary.input_tokens, 50);
    assert_eq!(by_provider[1].summary.output_tokens, 100);
    assert_eq!(by_provider[1].summary.cost, "0.75");
    assert_eq!(
        by_provider[2].summary.requests, 1,
        "r-3 reached no upstream"
    );
}

#[tokio::test]
async fn a_trend_keeps_its_empty_buckets() {
    let gproxy = support::sdk().await;
    usage(
        &gproxy,
        Seed {
            request_id: "r-1",
            started_at_ms: 1_000,
            tokens: (10, 20, 0, 0),
            cost: Some("0.5"),
            ..Default::default()
        },
    )
    .await;
    usage(
        &gproxy,
        Seed {
            request_id: "r-2",
            started_at_ms: 3_500,
            tokens: (1, 2, 0, 0),
            ..Default::default()
        },
    )
    .await;

    let points = gproxy
        .query()
        .usage()
        .trend(UsageTrendQuery {
            filter: UsageQuery {
                from_ms: Some(1_000),
                to_ms: Some(4_000),
                ..Default::default()
            },
            bucket_ms: 1_000,
        })
        .await
        .unwrap();
    assert_eq!(points.len(), 3);
    assert_eq!(
        points
            .iter()
            .map(|point| (point.start_ms, point.end_ms))
            .collect::<Vec<_>>(),
        [(1_000, 2_000), (2_000, 3_000), (3_000, 4_000)],
        "fixed width, aligned to fromMs"
    );
    assert_eq!(points[0].summary.requests, 1);
    assert_eq!(points[0].summary.input_tokens, 10);
    assert_eq!(
        points[1].summary.requests, 0,
        "an empty bucket is present with zeros, not missing"
    );
    assert_eq!(points[1].summary.cost, "0");
    assert_eq!(points[2].summary.requests, 1);
}

#[tokio::test]
async fn a_trend_refuses_a_range_it_cannot_draw() {
    let gproxy = support::sdk().await;
    let query = gproxy.query();
    let usage = query.usage();
    let range = UsageQuery {
        from_ms: Some(0),
        to_ms: Some(10_000),
        ..Default::default()
    };

    for (bucket_ms, filter) in [
        (0, range.clone()),
        (-1_000, range.clone()),
        // One bucket per millisecond over a day is far past the maximum.
        (
            1,
            UsageQuery {
                from_ms: Some(0),
                to_ms: Some(86_400_000),
                ..Default::default()
            },
        ),
        // A range that runs backwards.
        (
            1_000,
            UsageQuery {
                from_ms: Some(10_000),
                to_ms: Some(0),
                ..Default::default()
            },
        ),
        // No range at all.
        (1_000, UsageQuery::default()),
    ] {
        let error = usage
            .trend(UsageTrendQuery { filter, bucket_ms })
            .await
            .expect_err("the request cannot be answered");
        assert_eq!(error.status_code(), 400, "{error}");
    }
}

// ---------------------------------------------------------------------------
// Request logs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_log_cursor_does_not_loop_on_a_shared_millisecond() {
    let gproxy = support::sdk().await;
    capture(&gproxy, downstream("c-1", 1_000)).await;
    capture(&gproxy, downstream("c-2", 2_000)).await;
    // The same millisecond as c-2: a timestamp-only cursor would either
    // repeat this pair forever or skip one of them.
    capture(&gproxy, downstream("c-3", 2_000)).await;
    capture(&gproxy, downstream("c-4", 3_000)).await;
    // An upstream attempt is not a request and must never be listed.
    capture(&gproxy, upstream("u-1", "c-4", 3_010)).await;

    let query = gproxy.query();
    let logs = query.logs();
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = None;
    let mut cursor_id = None;
    for _ in 0..5 {
        let page = logs
            .list(LogQuery {
                limit: Some(2),
                cursor,
                cursor_id: cursor_id.clone(),
                ..Default::default()
            })
            .await
            .unwrap();
        seen.extend(page.items.iter().map(|item| item.request_id.clone()));
        cursor = page.next_cursor;
        cursor_id = page.next_cursor_id;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        seen,
        ["c-4", "c-3", "c-2", "c-1"],
        "every downstream request exactly once, newest first"
    );
    assert!(cursor.is_none(), "the walk reached the end of the list");
}

#[tokio::test]
async fn log_filters_narrow_the_downstream_list() {
    let gproxy = support::sdk().await;
    let mut hit = downstream("c-1", 1_000);
    hit.user_id = Set(Some("u-1".into()));
    hit.provider_id = Set(Some("p-1".into()));
    hit.model = Set(Some("m-1".into()));
    hit.response_status = Set(Some(200));
    capture(&gproxy, hit).await;
    let mut miss = downstream("c-2", 2_000);
    miss.user_id = Set(Some("u-2".into()));
    miss.response_status = Set(Some(429));
    capture(&gproxy, miss).await;

    let query = gproxy.query();
    let logs = query.logs();
    for (query, expected) in [
        (
            LogQuery {
                user_id: Some("u-1".into()),
                ..Default::default()
            },
            vec!["c-1"],
        ),
        (
            LogQuery {
                status: Some(429),
                ..Default::default()
            },
            vec!["c-2"],
        ),
        (
            LogQuery {
                provider_id: Some("p-1".into()),
                ..Default::default()
            },
            vec!["c-1"],
        ),
        (
            LogQuery {
                request_id: Some("c-2".into()),
                ..Default::default()
            },
            vec!["c-2"],
        ),
        (
            LogQuery {
                from_ms: Some(2_000),
                ..Default::default()
            },
            vec!["c-2"],
        ),
    ] {
        let page = logs.list(query).await.unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|item| item.request_id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[tokio::test]
async fn a_detail_resolves_the_upstream_attempts_and_their_events() {
    let gproxy = support::sdk().await;
    let mut request = downstream("d-1", 1_000);
    // A captured body that was genuinely empty.
    request.request_body = Set(Some(Vec::new()));
    request.request_body_state = Set(capture_record::CaptureBodyState::Complete);
    capture(&gproxy, request).await;

    let mut first = upstream("x-1", "d-1", 1_010);
    first.provider_id = Set(Some("p-1".into()));
    first.response_body = Set(Some(br#"{"ok":true}"#.to_vec()));
    first.response_body_state = Set(capture_record::CaptureBodyState::Complete);
    capture(&gproxy, first).await;

    let mut second = upstream("x-2", "d-1", 1_020);
    second.provider_id = Set(Some("p-2".into()));
    // Not text: it must come back as base64 rather than as lossy nonsense.
    second.response_body = Set(Some(vec![0xff, 0xfe, 0x00, 0x01]));
    second.response_body_state = Set(capture_record::CaptureBodyState::Complete);
    capture(&gproxy, second).await;

    event(&gproxy, "x-1", 1, b"data: one\n\n").await;
    event(&gproxy, "x-1", 2, b"data: two\n\n").await;
    // An event on a record this request did not reach.
    capture(&gproxy, upstream("x-3", "other", 1_030)).await;
    event(&gproxy, "x-3", 1, b"not ours").await;

    usage(
        &gproxy,
        Seed {
            request_id: "d-1",
            started_at_ms: 1_000,
            tokens: (7, 9, 0, 0),
            cost: Some("0.25"),
            ..Default::default()
        },
    )
    .await;

    let detail = gproxy.query().logs().detail("d-1").await.unwrap();
    assert_eq!(detail.downstream.id, "d-1");
    assert_eq!(detail.downstream.side, "downstream");
    assert_eq!(
        detail
            .upstream
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["x-1", "x-2"],
        "upstream start time order"
    );
    assert_eq!(
        detail
            .events
            .iter()
            .map(|e| (e.capture_id.as_str(), e.sequence))
            .collect::<Vec<_>>(),
        [("x-1", 1), ("x-1", 2)],
        "only the events of this request's own records"
    );
    assert_eq!(detail.events[0].payload.content, "data: one\n\n");
    assert!(!detail.events_truncated);

    let usage = detail.usage.expect("the request settled");
    assert_eq!(usage.tokens.input_tokens, Some(7));
    assert_eq!(usage.cost.as_deref(), Some("0.25"));

    // Text stays text, bytes become base64.
    let text = detail.upstream.iter().find(|row| row.id == "x-1").unwrap();
    assert_eq!(text.response_body.encoding, LogBodyEncoding::Utf8);
    assert_eq!(text.response_body.content, r#"{"ok":true}"#);
    assert_eq!(text.response_body.bytes, 11);
    assert!(!text.response_body.truncated);
    let binary = detail.upstream.iter().find(|row| row.id == "x-2").unwrap();
    assert_eq!(binary.response_body.encoding, LogBodyEncoding::Base64);
    assert_eq!(binary.response_body.content, "//4AAQ==");

    assert_eq!(
        gproxy
            .query()
            .logs()
            .detail("nothing")
            .await
            .expect_err("no such request")
            .status_code(),
        404
    );
}

#[tokio::test]
async fn an_uncaptured_body_is_not_an_empty_one() {
    let gproxy = support::sdk().await;
    let mut request = downstream("d-1", 1_000);
    // Captured, and empty.
    request.request_body = Set(Some(Vec::new()));
    request.request_body_state = Set(capture_record::CaptureBodyState::Complete);
    // Never captured. The column is empty for the same reason it is empty
    // above, so only the state can tell the two apart.
    request.response_body_state = Set(capture_record::CaptureBodyState::NotCaptured);
    capture(&gproxy, request).await;

    let detail = gproxy.query().logs().detail("d-1").await.unwrap();
    assert_eq!(detail.downstream.request_body.state, "complete");
    assert_eq!(detail.downstream.request_body.bytes, 0);
    assert_eq!(detail.downstream.request_body.content, "");
    assert_eq!(detail.downstream.response_body.state, "not_captured");
    assert_eq!(detail.downstream.response_body.bytes, 0);
    assert_eq!(detail.downstream.response_body.content, "");
    assert!(!detail.downstream.response_body.truncated);
}

// ---------------------------------------------------------------------------
// Quota windows
// ---------------------------------------------------------------------------

#[tokio::test]
async fn windows_carry_their_quota_and_their_settlements() {
    use gproxy_sdk::dto::{QuotaWindowQuery, QuotaWrite};
    use gproxy_store::entity::limits::{quota_settlement, quota_window};

    let gproxy = support::sdk().await;
    gproxy
        .manage()
        .quotas()
        .create(QuotaWrite {
            id: Some("q-1".into()),
            owner_kind: "user".into(),
            owner_id: "u-1".into(),
            metric: "cost".into(),
            unit: "USD".into(),
            limit_value: "10".into(),
            period: "total".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let now = 1_700_000_000_000_i64;
    gproxy
        .store()
        .quota_windows()
        .create_many(vec![
            quota_window::ActiveModel {
                id: Set("w-open".into()),
                quota_id: Set("q-1".into()),
                starts_at_ms: Set(now - 1_000),
                ends_at_ms: Set(None),
                used: Set("2.5".parse().unwrap()),
                quota_snapshot: Set(json!({"limit_value": "10"})),
            },
            quota_window::ActiveModel {
                id: Set("w-closed".into()),
                quota_id: Set("q-1".into()),
                starts_at_ms: Set(now - 10_000),
                ends_at_ms: Set(Some(now - 5_000)),
                used: Set("1".parse().unwrap()),
                quota_snapshot: Set(json!({})),
            },
            // A window whose quota row is gone: a historical reference, not a
            // foreign key, so it must still be listed.
            quota_window::ActiveModel {
                id: Set("w-orphan".into()),
                quota_id: Set("q-gone".into()),
                starts_at_ms: Set(now - 2_000),
                ends_at_ms: Set(None),
                used: Set("3".parse().unwrap()),
                quota_snapshot: Set(json!({})),
            },
        ])
        .await
        .unwrap();
    gproxy
        .store()
        .quota_settlements()
        .create_many(vec![quota_settlement::ActiveModel {
            window_id: Set("w-open".into()),
            request_id: Set("r-1".into()),
            amount: Set("2.5".parse().unwrap()),
            settled_at_ms: Set(now - 500),
            receipt: Set(vec![0; 32]),
        }])
        .await
        .unwrap();

    let query = gproxy.query();
    let quota = query.quota();
    let all = quota.windows(QuotaWindowQuery::default()).await.unwrap();
    assert_eq!(all.total, 3);
    assert_eq!(
        all.items
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["w-open", "w-orphan", "w-closed"],
        "newest first"
    );
    let open = &all.items[0];
    assert_eq!(open.used, "2.5");
    assert_eq!(open.limit_value.as_deref(), Some("10"));
    assert_eq!(open.owner_kind.as_deref(), Some("user"));
    assert_eq!(open.period.as_deref(), Some("total"));
    assert!(open.active);
    assert_eq!(
        all.items[1].limit_value, None,
        "a window whose quota is gone keeps only its snapshot"
    );
    assert!(!all.items[2].active, "a closed window is not active");

    let active = quota
        .windows(QuotaWindowQuery {
            quota_id: Some("q-1".into()),
            active_only: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        active
            .items
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["w-open"]
    );

    let by_owner = quota
        .windows(QuotaWindowQuery {
            owner_kind: Some("user".into()),
            owner_id: Some("u-1".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(by_owner.total, 2, "both windows of that owner's quota");
    let nobody = quota
        .windows(QuotaWindowQuery {
            owner_kind: Some("team".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(nobody.total, 0, "an owner that owns nothing is empty");

    let settlements = quota
        .settlements("w-open", ListQuery::default())
        .await
        .unwrap();
    assert_eq!(settlements.total, 1);
    assert_eq!(settlements.items[0].request_id, "r-1");
    assert_eq!(settlements.items[0].amount, "2.5");
}

#[tokio::test]
async fn counted_windows_and_cycles_read_the_meter_and_its_history() {
    use gproxy_sdk::dto::QuotaObservationQuery;
    use gproxy_store::entity::limits::{
        counted_window, credential_block,
        credential_cycle::{self, CycleBoundary, CycleOpening},
        credential_quota_cycle,
    };

    let gproxy = support::sdk().await;
    let now = 1_700_000_000_000_i64;
    // A block is the one row here with a real foreign key to its credential.
    support::seed::provider(&gproxy, "p-1", "test", &[]).await;
    support::seed::credential(&gproxy, "c-1", "p-1").await;
    gproxy
        .store()
        .counted_windows()
        .create_many(vec![
            counted_window::ActiveModel {
                credential_id: Set("c-1".into()),
                dimension: Set("limit:q-1".into()),
                window_start_ms: Set(now - 1_000),
                window_end_ms: Set(now + 1_000),
                used: Set(42),
                limit: Set(100),
            },
            // A window that has already rolled over.
            counted_window::ActiveModel {
                credential_id: Set("c-1".into()),
                dimension: Set("limit:q-1".into()),
                window_start_ms: Set(now - 10_000),
                window_end_ms: Set(now - 5_000),
                used: Set(7),
                limit: Set(100),
            },
        ])
        .await
        .unwrap();
    // One open 5h cycle with a reading, twelve closed 5h cycles, and one
    // closed weekly cycle that a total cap would have crowded out.
    let fixed = |text: &str| text.parse::<gproxy_store::FixedDecimal>().unwrap();
    let cycle = |id: String, window: &str, starts: i64, closed: Option<i64>| {
        credential_cycle::ActiveModel {
            id: Set(id),
            credential_id: Set("c-1".into()),
            closed_at_ms: Set(closed),
            open_key: Set(closed
                .is_none()
                .then(|| credential_cycle::open_key("c-1", window))),
            window_id: Set(window.into()),
            dimension_id: Set(Some(window.into())),
            scope: Set(json!("all")),
            starts_at_ms: Set(starts),
            ends_at_ms: Set(Some(starts + 18_000_000)),
            boundary: Set(CycleBoundary::Observed),
            opened_by: Set(CycleOpening::Rollover),
            cost_usd: Set(fixed("1")),
            ..Default::default()
        }
    };
    let mut cycles = vec![credential_cycle::ActiveModel {
        cost_usd: Set(fixed("2.5")),
        sample_used_percent: Set(Some(fixed("40"))),
        sample_used: Set(None),
        sample_limit: Set(None),
        sample_cost_usd: Set(Some(fixed("2"))),
        sample_at_ms: Set(Some(now - 50)),
        ..cycle("open-5h".into(), "5h", now - 1_000, None)
    }];
    for index in 0..12_i64 {
        let starts = now - 1_000 - (index + 1) * 18_000_000;
        cycles.push(cycle(
            format!("closed-5h-{index:02}"),
            "5h",
            starts,
            Some(starts + 18_000_000),
        ));
    }
    cycles.push(credential_cycle::ActiveModel {
        sample_used_percent: Set(Some(fixed("0"))),
        sample_cost_usd: Set(Some(fixed("1"))),
        sample_at_ms: Set(Some(now - 900_000_000)),
        ..cycle(
            "closed-7d".into(),
            "7d",
            now - 1_000_000_000,
            Some(now - 800_000_000),
        )
    });
    gproxy
        .store()
        .credential_cycles()
        .create_many(cycles)
        .await
        .unwrap();
    let observation = |id: &str, at: i64| credential_quota_cycle::ActiveModel {
        id: Set(id.into()),
        credential_id: Set("c-1".into()),
        scope: Set(json!("all")),
        snapshot: Set(json!({"entries": []})),
        observed_at_ms: Set(at),
        credential_cycle_id: Set(Some("open-5h".into())),
        cycle_cost_usd: Set(Some(fixed("0.75"))),
        ..Default::default()
    };
    gproxy
        .store()
        .credential_quota_cycles()
        .create_many(vec![
            observation("ob-old", now - 5_000),
            observation("ob-1", now - 100),
            observation("ob-2", now - 50),
        ])
        .await
        .unwrap();
    gproxy
        .store()
        .credential_blocks()
        .create_many(vec![
            credential_block::ActiveModel {
                id: Set("b-live".into()),
                credential_id: Set("c-1".into()),
                scope: Set(json!("all")),
                until_ms: Set(i64::MAX),
                source: Set(json!({"kind": "rate_limited"})),
                observed_at_ms: Set(now),
                ..Default::default()
            },
            credential_block::ActiveModel {
                id: Set("b-expired".into()),
                credential_id: Set("c-1".into()),
                scope: Set(json!("all")),
                until_ms: Set(1),
                source: Set(json!({"kind": "rate_limited"})),
                observed_at_ms: Set(0),
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    let query = gproxy.query();
    let quota = query.quota();
    let windows = quota.counted_windows("c-1", now).await.unwrap();
    assert_eq!(windows.len(), 1, "only the window covering that instant");
    assert_eq!(windows[0].dimension, "limit:q-1");
    assert_eq!(windows[0].used, 42);
    assert_eq!(windows[0].limit, 100);

    let cycles = quota.credential_cycles("c-1").await.unwrap();
    let ids = cycles
        .cycles
        .iter()
        .map(|cycle| cycle.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids.len(),
        1 + 10 + 1,
        "open, ten closed 5h, the one closed 7d"
    );
    assert_eq!(ids[0], "open-5h");
    assert_eq!(ids[1], "closed-5h-00", "closed newest first");
    assert_eq!(ids[10], "closed-5h-09");
    assert_eq!(
        ids[11], "closed-7d",
        "a busy window does not crowd out another"
    );
    let open = &cycles.cycles[0];
    assert_eq!(open.closed_at_ms, None);
    assert_eq!(open.cost_usd, "2.5");
    assert_eq!(open.opened_by, "rollover");
    assert_eq!(open.boundary, "observed");
    let sample = open.sample.as_ref().unwrap();
    assert_eq!(sample.used_percent.as_deref(), Some("40"));
    assert_eq!(sample.cost_usd.as_deref(), Some("2"));
    assert_eq!(sample.at_ms, now - 50);
    assert_eq!(
        open.estimated_allowance_usd.as_deref(),
        Some("5"),
        "the sample's own cost over its own percent, not the live cost"
    );
    assert!(cycles.cycles[1].sample.is_none());
    assert_eq!(cycles.cycles[1].estimated_allowance_usd, None);
    assert_eq!(
        cycles.cycles[11].estimated_allowance_usd, None,
        "a zero percent estimates nothing"
    );

    // By default the listing starts at the open cycle's start.
    let observations = quota
        .credential_observations(
            "c-1",
            QuotaObservationQuery {
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(observations.total, 2);
    assert_eq!(observations.items[0].id, "ob-2", "newest first");
    assert_eq!(observations.items[0].cycle_id.as_deref(), Some("open-5h"));
    assert_eq!(
        observations.items[0].cycle_cost_usd.as_deref(),
        Some("0.75")
    );
    let ranged = quota
        .credential_observations(
            "c-1",
            QuotaObservationQuery {
                since_ms: Some(now - 10_000),
                until_ms: Some(now - 50),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        ranged
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["ob-1", "ob-old"]
    );
    assert_eq!(
        cycles
            .blocks
            .iter()
            .map(|block| block.id.as_str())
            .collect::<Vec<_>>(),
        ["b-live"],
        "an expired block is not keeping anything out of selection"
    );
}

#[tokio::test]
async fn log_details_return_full_large_text_binary_and_event_payloads() {
    use base64::Engine;
    let gproxy = support::sdk().await;
    let text = "界".repeat(40_000);
    let binary = vec![0xff; 130_001];
    let mut row = downstream("full-payload", 100);
    row.request_body = Set(Some(text.as_bytes().to_vec()));
    row.request_body_state = Set(capture_record::CaptureBodyState::Complete);
    row.response_body = Set(Some(binary.clone()));
    row.response_body_state = Set(capture_record::CaptureBodyState::Complete);
    capture(&gproxy, row).await;
    event(&gproxy, "full-payload", 0, text.as_bytes()).await;
    let detail = gproxy.query().logs().detail("full-payload").await.unwrap();
    assert_eq!(detail.downstream.request_body.content, text);
    assert_eq!(detail.downstream.request_body.bytes, text.len() as u64);
    assert!(!detail.downstream.request_body.truncated);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(detail.downstream.response_body.content)
            .unwrap(),
        binary
    );
    assert_eq!(detail.events[0].payload.content, text);
    let standalone = gproxy.query().logs().capture("full-payload").await.unwrap();
    assert_eq!(standalone.record.request_body.content, text);
    assert_eq!(standalone.events[0].payload.content, text);
}

#[tokio::test]
async fn cache_write_periods_survive_summary_groups_and_trend_while_metadata_ops_are_excluded() {
    let gproxy = support::sdk().await;
    let store = gproxy.store();
    let mut rows = Vec::new();
    for (id, op) in [
        ("infer-a", "generate_content"),
        ("infer-b", "generate_content"),
        ("models", "list_models"),
        ("count", "count_tokens"),
        ("file", "retrieve_file"),
        ("video", "retrieve_video"),
        ("conversation", "create_conversation"),
    ] {
        rows.push(usage_record::ActiveModel {
            request_id: Set(id.into()),
            user_id: Set(Some("u".into())),
            model: Set("m".into()),
            operation: Set(op.into()),
            started_at_ms: Set(100),
            input_tokens: Set(Some(10)),
            output_tokens: Set(Some(20)),
            cached_input_tokens: Set(Some(50)),
            cache_creation_5m_tokens: Set(Some(11)),
            cache_creation_30m_tokens: Set(Some(22)),
            cache_creation_1h_tokens: Set(Some(33)),
            metrics: Set(json!({})),
            ..Default::default()
        });
    }
    let upstream: Vec<_> = rows
        .iter()
        .map(|row| usage_record::ActiveModel {
            request_id: Set(format!("{}-up", row.request_id.clone().unwrap())),
            downstream_request_id: Set(Some(row.request_id.clone().unwrap())),
            side: Set(capture_record::CaptureSide::Upstream),
            provider_id: Set(Some("p".into())),
            ..row.clone()
        })
        .collect();
    rows.extend(upstream);
    store.usage_records().create_many(rows).await.unwrap();
    let query = gproxy.query();
    let usage = query.usage();
    let summary = usage.summary(UsageQuery::default()).await.unwrap();
    assert_eq!(summary.requests, 2);
    assert_eq!(summary.cached_input_tokens, 100);
    assert_eq!(summary.cache_creation_5m_tokens, 22);
    assert_eq!(summary.cache_creation_30m_tokens, 44);
    assert_eq!(summary.cache_creation_1h_tokens, 66);
    assert_eq!(summary.cache_creation_tokens, 132);
    for group_by in [UsageGroupBy::Model, UsageGroupBy::Provider] {
        let groups = usage
            .group(UsageGroupQuery {
                filter: UsageQuery::default(),
                group_by,
            })
            .await
            .unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].summary.cache_creation_5m_tokens, 22);
        assert_eq!(groups[0].summary.cache_creation_30m_tokens, 44);
        assert_eq!(groups[0].summary.cache_creation_1h_tokens, 66);
    }
    let trend = usage
        .trend(UsageTrendQuery {
            filter: UsageQuery {
                from_ms: Some(0),
                to_ms: Some(200),
                ..Default::default()
            },
            bucket_ms: 200,
        })
        .await
        .unwrap();
    assert_eq!(trend[0].summary.requests, 2);
    assert_eq!(trend[0].summary.cache_creation_tokens, 132);
    assert_eq!(trend[0].summary.cache_creation_30m_tokens, 44);
    let records = usage.records(UsageRecordQuery::default()).await.unwrap();
    assert_eq!(records.total, 2);
    assert_eq!(records.items[0].tokens.cache_creation_1h_tokens, Some(33));
    let excluded = usage
        .summary(UsageQuery {
            operation: Some("list_models".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(excluded.requests, 0);
}

#[tokio::test]
async fn media_tool_and_custom_quantities_survive_filters_groups_and_trends() {
    let gproxy = support::sdk().await;
    usage(
        &gproxy,
        Seed {
            request_id: "tools",
            started_at_ms: 100,
            exchanges: &[("p-1", 1, 2, Some("0.1")), ("p-2", 3, 4, Some("0.2"))],
            ..Default::default()
        },
    )
    .await;
    for (id, searches, seconds) in [
        ("tools", "5", "1.75"),
        ("tools-x0", "2", "0.5"),
        ("tools-x1", "3", "1.25"),
    ] {
        gproxy.store().usage_records().update_many(vec![usage_record::ActiveModel {
            request_id: Set(id.into()), web_searches: Set(Some(searches.parse().unwrap())),
            audio_seconds: Set(Some(seconds.parse().unwrap())),
            metrics: Set(json!({"metrics":{"vendor_units":"0.000000000001"},"dimensions":{"tool_name":"web"}})),
            ..Default::default()
        }]).await.unwrap();
    }
    let filter = UsageQuery {
        provider_id: Some("p-1".into()),
        from_ms: Some(0),
        to_ms: Some(200),
        ..Default::default()
    };
    let query = gproxy.query();
    let usage = query.usage();
    let all = usage.summary(UsageQuery::default()).await.unwrap();
    assert_eq!(all.requests, 1);
    assert_eq!(all.quantities["web_searches"], "5");
    let share = usage.summary(filter.clone()).await.unwrap();
    assert_eq!(share.quantities["web_searches"], "2");
    assert_eq!(share.quantities["audio_seconds"], "0.5");
    assert_eq!(share.quantities["vendor_units"], "0.000000000001");
    let groups = usage
        .group(UsageGroupQuery {
            filter: filter.clone(),
            group_by: UsageGroupBy::Provider,
        })
        .await
        .unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].summary.quantities, share.quantities);
    let trend = usage
        .trend(UsageTrendQuery {
            filter,
            bucket_ms: 200,
        })
        .await
        .unwrap();
    assert_eq!(trend[0].summary.quantities, share.quantities);
    let records = usage.records(UsageRecordQuery::default()).await.unwrap();
    assert_eq!(records.total, 1);
    assert_eq!(records.items[0].exchanges.len(), 2);
    assert_eq!(
        records.items[0].exchanges[0].quantities["web_searches"],
        "2"
    );
}

#[tokio::test]
async fn upstream_association_is_optional_and_independent_of_downstream_retention() {
    let gproxy = support::sdk().await;
    let mut linked = upstream("up", "down", 110);
    linked.provider_id = Set(Some("provider".into()));
    capture(&gproxy, linked).await;
    let mut independent = upstream("independent", "", 120);
    independent.initiator_request_id = Set(None);
    capture(&gproxy, independent).await;
    let query = gproxy.query();
    let logs = query.logs();
    assert_eq!(
        logs.upstream(LogQuery::default())
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    capture(&gproxy, downstream("down", 100)).await;
    assert_eq!(logs.detail("down").await.unwrap().upstream[0].id, "up");
    assert_eq!(logs.detail("down").await.unwrap().upstream.len(), 1);
    let filtered = logs
        .list(LogQuery {
            provider_id: Some("provider".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(filtered.items.len(), 1);
    assert_eq!(filtered.items[0].request_id, "down");
    usage(
        &gproxy,
        Seed {
            request_id: "down",
            ..Default::default()
        },
    )
    .await;
    gproxy
        .store()
        .capture_records()
        .delete_many(&["down".into()])
        .await
        .unwrap();
    let upstream = gproxy
        .store()
        .capture_records()
        .get_many(&["up".into()])
        .await
        .unwrap();
    assert_eq!(
        upstream[0]
            .as_ref()
            .unwrap()
            .initiator_request_id
            .as_deref(),
        Some("down")
    );
    assert_eq!(
        gproxy
            .query()
            .usage()
            .records(UsageRecordQuery::default())
            .await
            .unwrap()
            .total,
        1
    );
}
