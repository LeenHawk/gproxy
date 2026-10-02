//! Key-paged reads of structured usage records, independent of logs.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Store, StoreError, entity::usage::usage_record};
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

/// Maximum row budget for explicitly bounded reads, including credential spend.
/// Unbounded usage aggregates do not apply this limit.
pub const MAX_SCAN_ROWS: u64 = 50_000;

/// How many rows one round trip reads. Small enough that a scan never holds a
/// whole range at once.
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
/// and optionally stop after `cap` rows. Without a cap, read the full range.
///
/// Paging is by key, not by offset: an offset would re-read everything it
/// skipped on every chunk, and would repeat or drop rows if a request settled
/// while the scan ran.
async fn scan_inner<C: BatchConnectionTrait>(
    store: &Store<C>,
    condition: Condition,
    order: ScanOrder,
    cap: Option<u64>,
    mut visit: impl FnMut(&UsageRecord),
) -> Result<ScanOutcome, StoreError> {
    use usage_record::Column as Col;
    let repository = store.usage_records();
    let mut cursor: Option<(i64, String)> = None;
    let mut budget = cap.map(|cap| cap.max(1));
    let mut outcome = ScanOutcome::default();
    'chunks: loop {
        // One row past the budget, so the last chunk can tell "that was
        // everything" from "there is more and the cap stopped us".
        let limit = budget.map_or(SCAN_CHUNK, |left| SCAN_CHUNK.min(left.saturating_add(1)));
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
            if budget == Some(0) {
                outcome.truncated = true;
                break 'chunks;
            }
            cursor = Some((row.started_at_ms, row.request_id.clone()));
            visit(row);
            if let Some(left) = &mut budget {
                *left -= 1;
            }
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

/// One physical upstream usage row.
pub type UsageRecord = usage_record::Model;

pub async fn scan<C: BatchConnectionTrait>(
    store: &Store<C>,
    condition: Condition,
    order: ScanOrder,
    cap: Option<u64>,
    visit: impl FnMut(&UsageRecord),
) -> Result<ScanOutcome, StoreError> {
    scan_inner(store, condition, order, cap, visit).await
}

/// USD spent by this credential's own calls. Filtering occurs in SQL before
/// the bounded scan, so unrelated traffic cannot consume the scan budget.
pub async fn credential_usd_cost<C: BatchConnectionTrait>(
    store: &Store<C>,
    credential_id: &str,
    from_ms: i64,
    to_ms: i64,
    models: impl Fn(Option<&str>) -> bool,
    max_rows: u64,
) -> Result<(Decimal, bool), StoreError> {
    use usage_record::Column as Col;
    let mut total = Decimal::ZERO;
    let outcome = scan_inner(
        store,
        usage_condition(Some(from_ms), Some(to_ms)).add(Col::CredentialId.eq(credential_id)),
        ScanOrder::Oldest,
        Some(max_rows.clamp(1, MAX_SCAN_ROWS)),
        |row| {
            if models((!row.model.is_empty()).then_some(row.model.as_str()))
                && let Some(cost) = row.cost
            {
                total += cost.decimal();
            }
        },
    )
    .await?;
    Ok((total, outcome.truncated))
}
