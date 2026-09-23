//! Usage: the record list, and the three aggregates a console draws from it.
//!
//! Only the list is a database query. `usage_records.metrics` is a JSON
//! document, so no backend this crate supports can sum a token count or cut a
//! total by provider; the aggregates read the matching rows and fold them
//! here. That is bounded on purpose — see [`MAX_SCAN_ROWS`] — and an
//! aggregation that hit its bound says so instead of returning a smaller
//! number as if it were the whole truth.
//!
//! Cost is the one number that does not come out of the document: it has its
//! own indexed column, written once at settlement. Only the per-provider cut
//! reads the priced amounts inside `metrics`, because that is the only place
//! the breakdown exists.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::usage::usage_record;
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use super::{MAX_SCAN_ROWS, MAX_TREND_BUCKETS, SCAN_CHUNK, bounds, filter};
use crate::{
    SdkError, SdkResult,
    dto::{
        Page, UsageExchangeDto, UsageGroupBy, UsageGroupDto, UsageGroupQuery, UsageQuery,
        UsageRecordDto, UsageRecordQuery, UsageSummaryDto, UsageTokensDto, UsageTrendPointDto,
        UsageTrendQuery,
    },
    handle::Inner,
};

pub struct Usage<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

impl<'a, C> Usage<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Usage<'_, C> {
    /// One page of records, newest first. The order is
    /// `(started_at_ms DESC, request_id ASC)`: the timestamp is what a person
    /// reads by and the id is what keeps two records of the same millisecond
    /// from swapping places between two pages.
    pub async fn records(&self, query: UsageRecordQuery) -> SdkResult<Page<UsageRecordDto>> {
        let filters = Filters {
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            user_id: filter(&query.user_id),
            api_key_id: filter(&query.api_key_id),
            model: filter(&query.model),
            operation: filter(&query.operation),
            request_id: filter(&query.request_id),
        };
        let (offset, limit) = bounds(query.page, query.page_size);
        // The repository appends the primary key ascending to whatever order
        // it is given, which is exactly the tie-break wanted here.
        let page = self
            .inner
            .store
            .usage_records()
            .page(
                usage_record::Entity::find()
                    .filter(filters.condition())
                    .order_by_desc(usage_record::Column::StartedAtMs),
                offset,
                limit,
            )
            .await?;
        Ok(Page::convert(page, UsageRecordDto::from))
    }

    /// Totals over every record the filters match, up to the scan cap.
    pub async fn summary(&self, query: UsageQuery) -> SdkResult<UsageSummaryDto> {
        let filters = Filters::from(&query);
        let mut totals = Totals::default();
        let scan = self
            .scan(&filters, cap(query.max_scan_rows), |row| totals.add(row))
            .await?;
        Ok(totals.summary(&scan))
    }

    /// The same totals, cut by one column. Groups are ordered by cost and then
    /// by request count, both descending, with the key as the final tie-break,
    /// so the answer is stable between two identical calls.
    pub async fn group(&self, query: UsageGroupQuery) -> SdkResult<Vec<UsageGroupDto>> {
        let filters = Filters::from(&query.filter);
        let mut groups: BTreeMap<Option<String>, Totals> = BTreeMap::new();
        let group_by = query.group_by;
        let scan = self
            .scan(
                &filters,
                cap(query.filter.max_scan_rows),
                |row| match group_by {
                    UsageGroupBy::Provider => group_by_provider(&mut groups, row),
                    _ => groups.entry(key_of(row, group_by)).or_default().add(row),
                },
            )
            .await?;

        let mut entries: Vec<(Option<String>, Totals)> = groups.into_iter().collect();
        entries.sort_by(|a, b| {
            b.1.cost
                .cmp(&a.1.cost)
                .then(b.1.requests.cmp(&a.1.requests))
                .then(a.0.cmp(&b.0))
        });
        Ok(entries
            .into_iter()
            .map(|(key, totals)| UsageGroupDto {
                key,
                summary: totals.summary(&scan),
            })
            .collect())
    }

    /// Fixed-width buckets aligned to `from_ms`, every one of them present.
    /// A bucket nothing landed in is zeros rather than a gap: a chart must not
    /// have to guess whether a missing point is "no traffic" or "no data".
    pub async fn trend(&self, query: UsageTrendQuery) -> SdkResult<Vec<UsageTrendPointDto>> {
        let from_ms = query
            .filter
            .from_ms
            .ok_or_else(|| SdkError::invalid("a trend needs fromMs"))?;
        let to_ms = query
            .filter
            .to_ms
            .ok_or_else(|| SdkError::invalid("a trend needs toMs"))?;
        let bucket_ms = query.bucket_ms;
        if bucket_ms <= 0 {
            return Err(SdkError::invalid("bucketMs must be positive"));
        }
        let span = to_ms
            .checked_sub(from_ms)
            .filter(|span| *span > 0)
            .ok_or_else(|| SdkError::invalid("toMs must be after fromMs"))?;
        // Both operands are positive, so this is the ceiling division that
        // `i64::div_ceil` would be if it were stable.
        let count = (span - 1) / bucket_ms + 1;
        if count > MAX_TREND_BUCKETS {
            return Err(SdkError::invalid(format!(
                "a {bucket_ms}ms bucket over this range is {count} buckets, more than the {MAX_TREND_BUCKETS} allowed"
            )));
        }

        let filters = Filters::from(&query.filter);
        let mut buckets = vec![Totals::default(); count as usize];
        let scan = self
            .scan(&filters, cap(query.filter.max_scan_rows), |row| {
                // The filter is half-open on the same bounds, so the index is
                // always inside the vector; `get_mut` rather than an index is
                // the cheap way of saying so without a panic path.
                let index = (row.started_at_ms - from_ms) / bucket_ms;
                if let Ok(index) = usize::try_from(index)
                    && let Some(bucket) = buckets.get_mut(index)
                {
                    bucket.add(row);
                }
            })
            .await?;
        Ok(buckets
            .into_iter()
            .enumerate()
            .map(|(index, totals)| {
                let start_ms = from_ms + index as i64 * bucket_ms;
                UsageTrendPointDto {
                    start_ms,
                    end_ms: start_ms + bucket_ms,
                    summary: totals.summary(&scan),
                }
            })
            .collect())
    }

    /// Read the matching records in ascending key order, handing each one to
    /// `visit`, and stop at `cap` rows.
    ///
    /// Paging is by key, not by offset: the cursor is the `(started_at_ms,
    /// request_id)` of the last row visited, which is unique because ids are.
    /// An offset would re-read everything it skipped on every chunk, and would
    /// repeat or drop rows if a request settled while the scan was running.
    async fn scan(
        &self,
        filters: &Filters<'_>,
        cap: u64,
        mut visit: impl FnMut(&usage_record::Model),
    ) -> SdkResult<Scan> {
        let repository = self.inner.store.usage_records();
        let mut cursor: Option<(i64, String)> = None;
        let mut budget = cap;
        let mut scan = Scan {
            scanned: 0,
            truncated: false,
        };
        'chunks: loop {
            // One row past the budget, so the last chunk can tell "that was
            // everything" from "there is more and the cap stopped us".
            let limit = SCAN_CHUNK.min(budget + 1);
            let mut select = usage_record::Entity::find().filter(filters.condition());
            if let Some((started_at_ms, request_id)) = &cursor {
                select = select.filter(after(*started_at_ms, request_id));
            }
            let rows = repository
                .query(
                    select
                        .order_by_asc(usage_record::Column::StartedAtMs)
                        .order_by_asc(usage_record::Column::RequestId)
                        .limit(limit),
                )
                .await?;
            let fetched = rows.len() as u64;
            for row in &rows {
                if budget == 0 {
                    scan.truncated = true;
                    break 'chunks;
                }
                cursor = Some((row.started_at_ms, row.request_id.clone()));
                visit(row);
                budget -= 1;
                scan.scanned += 1;
            }
            if fetched < limit {
                break;
            }
        }
        Ok(scan)
    }
}

/// How far one scan actually got.
struct Scan {
    scanned: u64,
    truncated: bool,
}

/// The caller's row budget, clamped: absent means the module's cap, and no
/// caller may raise it past that.
fn cap(requested: Option<u64>) -> u64 {
    requested.unwrap_or(MAX_SCAN_ROWS).clamp(1, MAX_SCAN_ROWS)
}

/// Rows strictly after `(started_at_ms, request_id)` in the scan order.
fn after(started_at_ms: i64, request_id: &str) -> Condition {
    Condition::any()
        .add(usage_record::Column::StartedAtMs.gt(started_at_ms))
        .add(
            Condition::all()
                .add(usage_record::Column::StartedAtMs.eq(started_at_ms))
                .add(usage_record::Column::RequestId.gt(request_id)),
        )
}

/// The filters both the list and the aggregates understand. Every one of them
/// is an exact match on an indexed column; there is no substring search here,
/// because a usage table is not browsed by name.
struct Filters<'a> {
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    user_id: Option<&'a str>,
    api_key_id: Option<&'a str>,
    model: Option<&'a str>,
    operation: Option<&'a str>,
    request_id: Option<&'a str>,
}

impl<'a> From<&'a UsageQuery> for Filters<'a> {
    fn from(query: &'a UsageQuery) -> Self {
        Self {
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            user_id: filter(&query.user_id),
            api_key_id: filter(&query.api_key_id),
            model: filter(&query.model),
            operation: filter(&query.operation),
            request_id: None,
        }
    }
}

impl Filters<'_> {
    /// Half-open on time: `from_ms` is included and `to_ms` is not, so two
    /// adjacent ranges cover every record exactly once.
    fn condition(&self) -> Condition {
        use usage_record::Column as C;
        let mut condition = Condition::all();
        if let Some(from_ms) = self.from_ms {
            condition = condition.add(C::StartedAtMs.gte(from_ms));
        }
        if let Some(to_ms) = self.to_ms {
            condition = condition.add(C::StartedAtMs.lt(to_ms));
        }
        if let Some(user_id) = self.user_id {
            condition = condition.add(C::UserId.eq(user_id));
        }
        if let Some(api_key_id) = self.api_key_id {
            condition = condition.add(C::ApiKeyId.eq(api_key_id));
        }
        if let Some(model) = self.model {
            condition = condition.add(C::Model.eq(model));
        }
        if let Some(operation) = self.operation {
            condition = condition.add(C::Operation.eq(operation));
        }
        if let Some(request_id) = self.request_id {
            condition = condition.add(C::RequestId.eq(request_id));
        }
        condition
    }
}

/// The grouping column's value on one record. None means the record carried
/// none, which is a group of its own rather than a reason to drop the record.
fn key_of(row: &usage_record::Model, group_by: UsageGroupBy) -> Option<String> {
    match group_by {
        UsageGroupBy::User => row.user_id.clone(),
        UsageGroupBy::ApiKey => row.api_key_id.clone(),

        UsageGroupBy::Model => Some(row.model.clone()),
        UsageGroupBy::Operation => Some(row.operation.clone()),
        // Handled by `group_by_provider`: a provider is not a column.
        UsageGroupBy::Provider => None,
    }
}

/// A record's contribution to the per-provider cut.
///
/// The breakdown lives inside `metrics`, one entry per upstream attempt that
/// produced usage, so a request that failed over from one provider to another
/// contributes to both — each with that attempt's own tokens and price, never
/// with the request's totals counted twice. `requests` counts the record once
/// per distinct provider, not once per attempt. A record with no breakdown at
/// all (nothing reached an upstream) lands under the `None` key with its own
/// totals, because dropping it would make the groups stop summing to the
/// summary.
fn group_by_provider(groups: &mut BTreeMap<Option<String>, Totals>, row: &usage_record::Model) {
    let exchanges = row
        .metrics
        .get("exchanges")
        .and_then(serde_json::Value::as_array)
        .filter(|items| !items.is_empty());
    let Some(exchanges) = exchanges else {
        groups.entry(None).or_default().add(row);
        return;
    };
    let mut seen: BTreeSet<Option<String>> = BTreeSet::new();
    for exchange in exchanges {
        let exchange = UsageExchangeDto::read(exchange);
        let key = exchange.provider_id;
        let first = seen.insert(key.clone());
        let totals = groups.entry(key).or_default();
        if first {
            totals.requests += 1;
        }
        totals.add_tokens(&exchange.tokens);
        totals.add_cost(exchange.cost.as_deref(), exchange.currency.as_deref());
    }
}

/// One accumulator, shared by the summary, every group and every bucket.
#[derive(Clone, Default)]
struct Totals {
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,
    cache_creation_tokens: u64,
    reasoning_tokens: u64,
    cost: Decimal,
    currency: Currency,
}

impl Totals {
    /// One whole record: its tokens, its settled cost and one request.
    fn add(&mut self, row: &usage_record::Model) {
        self.requests += 1;
        self.add_tokens(&UsageTokensDto::read(Some(&row.metrics)));
        let (_, currency) = crate::dto::money(row.metrics.get("cost"));
        if let Some(cost) = row.cost {
            self.cost += cost.decimal();
        }
        self.observe_currency(currency.as_deref());
    }

    fn add_tokens(&mut self, tokens: &UsageTokensDto) {
        let add = |total: &mut u64, value: Option<u64>| {
            *total = total.saturating_add(value.unwrap_or(0));
        };
        add(&mut self.input_tokens, tokens.input_tokens);
        add(&mut self.output_tokens, tokens.output_tokens);
        add(&mut self.cached_input_tokens, tokens.cached_input_tokens);
        add(
            &mut self.cache_creation_tokens,
            tokens.cache_creation_5m_tokens,
        );
        add(
            &mut self.cache_creation_tokens,
            tokens.cache_creation_30m_tokens,
        );
        add(
            &mut self.cache_creation_tokens,
            tokens.cache_creation_1h_tokens,
        );
        add(&mut self.reasoning_tokens, tokens.reasoning_tokens);
    }

    /// A priced amount from the document rather than from the column. An
    /// amount that will not parse is dropped: an unreadable price must not
    /// silently become zero in a total that looks exact.
    fn add_cost(&mut self, amount: Option<&str>, currency: Option<&str>) {
        if let Some(amount) = amount.and_then(|amount| Decimal::from_str_exact(amount).ok()) {
            self.cost += amount;
        }
        self.observe_currency(currency);
    }

    fn observe_currency(&mut self, currency: Option<&str>) {
        let Some(currency) = currency.filter(|currency| !currency.is_empty()) else {
            return;
        };
        self.currency = match std::mem::take(&mut self.currency) {
            Currency::Unknown => Currency::One(currency.to_owned()),
            Currency::One(seen) if seen == currency => Currency::One(seen),
            _ => Currency::Mixed,
        };
    }

    fn summary(self, scan: &Scan) -> UsageSummaryDto {
        UsageSummaryDto {
            requests: self.requests,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_input_tokens: self.cached_input_tokens,
            cache_creation_tokens: self.cache_creation_tokens,
            reasoning_tokens: self.reasoning_tokens,
            cost: self.cost.normalize().to_string(),
            currency: match self.currency {
                Currency::One(currency) => Some(currency),
                Currency::Unknown | Currency::Mixed => None,
            },
            truncated: scan.truncated,
            scanned: scan.scanned,
        }
    }
}

/// What the scanned records agreed the cost was denominated in. Two different
/// currencies in one total are not a total, so the answer is then `None`
/// rather than whichever one happened to be seen first.
#[derive(Clone, Default)]
enum Currency {
    #[default]
    Unknown,
    One(String),
    Mixed,
}
