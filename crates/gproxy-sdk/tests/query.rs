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
use gproxy_store::entity::usage::{capture_event, capture_link, capture_record, usage_record};
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
    let mut metrics = usage_json(seed.tokens);
    metrics["state"] = json!("settled");
    metrics["cost"] = money(seed.cost);
    metrics["exchanges"] = json!(
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
    gproxy
        .store()
        .usage_records()
        .create_many(vec![usage_record::ActiveModel {
            request_id: Set(seed.request_id.into()),
            user_id: Set(seed.user_id.map(str::to_owned)),
            api_key_id: Set(seed.api_key_id.map(str::to_owned)),
            model: Set(seed.model.into()),
            operation: Set(seed.operation.into()),
            metrics: Set(metrics),
            cost: Set(seed.cost.map(|cost| cost.parse().unwrap())),
            started_at_ms: Set(seed.started_at_ms),
            ended_at_ms: Set(Some(seed.started_at_ms + 100)),
        }])
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
            operation: "count_tokens",
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

async fn link(gproxy: &Handle, downstream_id: &str, upstream_id: &str, sequence: i32) {
    gproxy
        .store()
        .capture_links()
        .create_many(vec![capture_link::ActiveModel {
            downstream_id: Set(downstream_id.into()),
            upstream_id: Set(upstream_id.into()),
            sequence: Set(sequence),
        }])
        .await
        .unwrap();
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
    assert!(record.metrics.get("exchanges").is_some());

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
                operation: Some("count_tokens".into()),
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

    // Deliberately linked out of id order, to prove the link sequence wins.
    link(&gproxy, "d-1", "x-2", 0).await;
    link(&gproxy, "d-1", "x-1", 1).await;
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
        ["x-2", "x-1"],
        "link order, not id order"
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
    use gproxy_store::entity::limits::{counted_window, credential_block, credential_quota_cycle};

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
    gproxy
        .store()
        .credential_quota_cycles()
        .create_many(vec![credential_quota_cycle::ActiveModel {
            id: Set("cy-1".into()),
            credential_id: Set("c-1".into()),
            scope: Set(json!("all")),
            snapshot: Set(json!({"entries": []})),
            observed_at_ms: Set(now - 100),
            ..Default::default()
        }])
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
    assert_eq!(cycles.cycles.len(), 1);
    assert_eq!(cycles.cycles[0].id, "cy-1");
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
