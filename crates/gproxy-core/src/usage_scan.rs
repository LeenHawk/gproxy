//! Bounded reads over `usage_records`, shared by core and the SDK's query side.
//!
//! `usage_records.metrics` is the JSON document [`StoreObserver`] writes, and
//! the per-attempt breakdown inside it (`exchanges[]`) is the only place a
//! provider, a credential or an upstream model is named. No backend can sum
//! inside that document, so anything cut by credential reads rows and folds
//! them in Rust. Every such read is key-paged and capped here, once, so the
//! engine and the SDK agree on what "bounded" means and on when a read says it
//! stopped early.
//!
//! Usage rows exist only while `observation.usage` (or settlement) is on for
//! the request's snapshot: a stretch of traffic served with it off left no
//! row, and nothing here can reconstruct what it cost.
//!
//! [`StoreObserver`]: crate::StoreObserver

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Store, StoreError, entity::usage::usage_record};
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde_json::Value;

/// How many rows one bounded read may visit before it stops and says so.
///
/// A month of one credential's traffic is well inside it; a year of a
/// deployment's is not, and reading that into one fold is what the cap is
/// for. Reaching it is reported as truncation, never as a smaller total.
pub const MAX_SCAN_ROWS: u64 = 50_000;

/// How many rows one round trip reads. Small enough that a scan never holds a
/// whole range at once, large enough that the cap is a handful of queries.
pub const SCAN_CHUNK: u64 = 1_000;

/// The order a scan visits rows in. Both tie-break on `request_id`
/// ascending, which is unique, so a key cursor never repeats or skips a row
/// that shares a millisecond with the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOrder {
    /// `(started_at_ms ASC, request_id ASC)`, for folds over a range.
    Oldest,
    /// `(started_at_ms DESC, request_id ASC)`, the order a record list is
    /// read in.
    Newest,
}

/// How far one scan got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanOutcome {
    /// Rows visited, whether or not the caller's fold kept them.
    pub scanned: u64,
    /// The cap stopped the scan with matching rows still unread.
    pub truncated: bool,
}

/// Read every row `condition` matches in `order`, handing each to `visit`,
/// and stop after `cap` rows (clamped to `1..=MAX_SCAN_ROWS`).
///
/// Paging is by key, not by offset: an offset would re-read everything it
/// skipped on every chunk, and would repeat or drop rows if a request settled
/// while the scan ran.
pub async fn scan<C: BatchConnectionTrait>(
    store: &Store<C>,
    condition: Condition,
    order: ScanOrder,
    cap: u64,
    mut visit: impl FnMut(&usage_record::Model),
) -> Result<ScanOutcome, StoreError> {
    use usage_record::Column as Col;
    let repository = store.usage_records();
    let mut cursor: Option<(i64, String)> = None;
    let mut budget = cap.clamp(1, MAX_SCAN_ROWS);
    let mut outcome = ScanOutcome::default();
    'chunks: loop {
        // One row past the budget, so the last chunk can tell "that was
        // everything" from "there is more and the cap stopped us".
        let limit = SCAN_CHUNK.min(budget + 1);
        let mut select = usage_record::Entity::find().filter(condition.clone());
        if let Some((started_at_ms, request_id)) = &cursor {
            select = select.filter(past(order, *started_at_ms, request_id));
        }
        select = match order {
            ScanOrder::Oldest => select.order_by_asc(Col::StartedAtMs),
            ScanOrder::Newest => select.order_by_desc(Col::StartedAtMs),
        };
        let rows = repository
            .query(select.order_by_asc(Col::RequestId).limit(limit))
            .await?;
        let fetched = rows.len() as u64;
        for row in &rows {
            if budget == 0 {
                outcome.truncated = true;
                break 'chunks;
            }
            cursor = Some((row.started_at_ms, row.request_id.clone()));
            visit(row);
            budget -= 1;
            outcome.scanned += 1;
        }
        if fetched < limit {
            break;
        }
    }
    Ok(outcome)
}

/// Rows strictly after `(started_at_ms, request_id)` in `order`.
fn past(order: ScanOrder, started_at_ms: i64, request_id: &str) -> Condition {
    use usage_record::Column as Col;
    let later = match order {
        ScanOrder::Oldest => Col::StartedAtMs.gt(started_at_ms),
        ScanOrder::Newest => Col::StartedAtMs.lt(started_at_ms),
    };
    Condition::any().add(later).add(
        Condition::all()
            .add(Col::StartedAtMs.eq(started_at_ms))
            .add(Col::RequestId.gt(request_id)),
    )
}

/// The rows every usage read starts from: operations that produce usage, and
/// `started_at_ms` in the half-open `[from_ms, to_ms)`, so two adjacent ranges
/// cover every record exactly once.
pub fn usage_condition(from_ms: Option<i64>, to_ms: Option<i64>) -> Condition {
    use usage_record::Column as Col;
    let mut condition =
        Condition::all().add(Col::Operation.is_in(
            gproxy_protocol::Operation::usage_operations().map(gproxy_protocol::Operation::id),
        ));
    if let Some(from_ms) = from_ms {
        condition = condition.add(Col::StartedAtMs.gte(from_ms));
    }
    if let Some(to_ms) = to_ms {
        condition = condition.add(Col::StartedAtMs.lt(to_ms));
    }
    condition
}

/// The per-attempt entries of one record's `metrics`, empty when nothing
/// reached an upstream.
pub fn exchanges(metrics: &Value) -> &[Value] {
    metrics
        .get("exchanges")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// A string field of one exchange entry.
pub fn exchange_text<'a>(exchange: &'a Value, field: &str) -> Option<&'a str> {
    exchange.get(field).and_then(Value::as_str)
}

/// USD spent by one credential on requests that started in
/// `[from_ms, to_ms)`, and whether the read was cut short.
///
/// Only the credential's own attempts count: a request that failed over from
/// another credential contributes just the exchange this credential served,
/// priced on its own, never the request's settled total. `models` narrows
/// further by the upstream model the attempt ran against — pass `|_| true` to
/// count everything, or `|model| model.is_some_and(|m| scope.matches(m))` for
/// a [`QuotaScope`], which then leaves out an attempt whose model was not
/// recorded. A cost priced in another currency is skipped rather than
/// converted, because a quota measured in dollars cannot be charged in euros.
///
/// The bound is on rows read, not rows kept: every usage record in the range
/// is visited to find this credential's attempts, so a busy deployment can
/// hit `max_rows` (clamped to [`MAX_SCAN_ROWS`]) over a short range. `true`
/// in the second position means the sum covers only the oldest `max_rows`
/// records of the range, and is therefore a lower bound.
///
/// Rows exist only where `observation.usage` was on — see the module note —
/// so this is what was recorded, which may be less than what was spent.
///
/// [`QuotaScope`]: gproxy_channel::channel::QuotaScope
pub async fn credential_usd_cost<C: BatchConnectionTrait>(
    store: &Store<C>,
    credential_id: &str,
    from_ms: i64,
    to_ms: i64,
    models: impl Fn(Option<&str>) -> bool,
    max_rows: u64,
) -> Result<(Decimal, bool), StoreError> {
    let mut total = Decimal::ZERO;
    let outcome = scan(
        store,
        usage_condition(Some(from_ms), Some(to_ms)),
        ScanOrder::Oldest,
        max_rows,
        |row| {
            for exchange in exchanges(&row.metrics) {
                if exchange_text(exchange, "credential_id") != Some(credential_id)
                    || !models(exchange_text(exchange, "model"))
                {
                    continue;
                }
                if let Some(amount) = usd_amount(exchange.get("cost")) {
                    total += amount;
                }
            }
        },
    )
    .await?;
    Ok((total, outcome.truncated))
}

/// `{"amount": "…", "currency": "USD"}` as the observer writes a priced cost,
/// or None for any other currency, an unpriced attempt or an unreadable
/// amount.
fn usd_amount(cost: Option<&Value>) -> Option<Decimal> {
    let cost = cost?;
    let currency = exchange_text(cost, "currency")?;
    if !currency.eq_ignore_ascii_case("USD") {
        return None;
    }
    Decimal::from_str_exact(exchange_text(cost, "amount")?).ok()
}
