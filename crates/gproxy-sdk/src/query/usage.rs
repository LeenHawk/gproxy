//! Usage lists and bounded aggregates over structured usage records.

use std::{collections::BTreeMap, sync::Arc};

use gproxy_core::usage_scan::{self, ScanOrder, ScanOutcome, UsageRecord};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::usage::{capture_link, usage_record};
use rust_decimal::Decimal;
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait,
};

use super::{MAX_SCAN_ROWS, MAX_TREND_BUCKETS, bounds, filter};
use crate::{
    SdkError, SdkResult,
    dto::{
        UsageGroupBy, UsageGroupDto, UsageGroupQuery, UsageQuery, UsageRecordDto, UsageRecordPage,
        UsageRecordQuery, UsageSummaryDto, UsageTokensDto, UsageTrendPointDto, UsageTrendQuery,
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
    ///
    /// Provider and credential filters use structured upstream columns in SQL.
    pub async fn records(&self, query: UsageRecordQuery) -> SdkResult<UsageRecordPage> {
        let filters = Filters {
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            user_id: filter(&query.user_id),
            api_key_id: filter(&query.api_key_id),
            model: filter(&query.model),
            operation: filter(&query.operation),
            request_id: filter(&query.request_id),
            cut: AttemptCut {
                provider_id: filter(&query.provider_id),
                credential_id: filter(&query.credential_id),
            },
        };
        let (offset, limit) = bounds(query.page, query.page_size);
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
        let items = page.items.into_iter().map(UsageRecordDto::from).collect();
        Ok(UsageRecordPage {
            items,
            total: page.total,
            offset: page.offset,
            limit: page.limit,
            truncated: false,
        })
    }

    /// Totals over every record the filters match, up to the scan cap.
    pub async fn summary(&self, query: UsageQuery) -> SdkResult<UsageSummaryDto> {
        let filters = Filters::from(&query);
        let mut totals = Totals::default();
        let scan = self
            .scan(&filters, cap(query.max_scan_rows), |row| {
                totals.add(row);
            })
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
            .scan(&filters, cap(query.filter.max_scan_rows), |row| {
                groups.entry(key_of(row, group_by)).or_default().add(row);
            })
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

    /// Read the rows the column filters match, oldest first, handing each to
    /// `visit`, and stop at `cap` matching downstream rows.
    async fn scan(
        &self,
        filters: &Filters<'_>,
        cap: u64,
        visit: impl FnMut(&UsageRecord),
    ) -> SdkResult<ScanOutcome> {
        Ok(usage_scan::scan(
            &self.inner.store,
            filters.condition(),
            ScanOrder::Oldest,
            cap,
            visit,
        )
        .await?)
    }
}

/// The caller's row budget, clamped: absent means the module's cap, and no
/// caller may raise it past that.
fn cap(requested: Option<u64>) -> u64 {
    requested.unwrap_or(MAX_SCAN_ROWS).clamp(1, MAX_SCAN_ROWS)
}

/// Exact SQL filters shared by lists and aggregates. Matching upstream calls
/// select the downstream rows before pagination; `cut` also selects each row's
/// contribution when folding upstream quantities.
struct Filters<'a> {
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    user_id: Option<&'a str>,
    api_key_id: Option<&'a str>,
    model: Option<&'a str>,
    operation: Option<&'a str>,
    request_id: Option<&'a str>,
    cut: AttemptCut<'a>,
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
            cut: AttemptCut {
                provider_id: filter(&query.provider_id),
                credential_id: filter(&query.credential_id),
            },
        }
    }
}

impl Filters<'_> {
    /// Half-open on time: `from_ms` is included and `to_ms` is not, so two
    /// adjacent ranges cover every record exactly once.
    fn condition(&self) -> Condition {
        use usage_record::Column as C;
        let mut condition = usage_scan::usage_condition(self.from_ms, self.to_ms);
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
            let ids = capture_link::Entity::find()
                .select_only()
                .column(capture_link::Column::UpstreamId)
                .filter(capture_link::Column::DownstreamId.eq(request_id))
                .into_query();
            condition = condition.add(
                Condition::any()
                    .add(C::RequestId.eq(request_id))
                    .add(C::RequestId.in_subquery(ids)),
            );
        }
        if let Some(id) = self.cut.provider_id {
            condition = condition.add(C::ProviderId.eq(id));
        }
        if let Some(id) = self.cut.credential_id {
            condition = condition.add(C::CredentialId.eq(id));
        }
        condition
    }
}

/// Filters on structured upstream usage. See [`UsageQuery`] for the counting rule.
#[derive(Clone, Copy)]
struct AttemptCut<'a> {
    provider_id: Option<&'a str>,
    credential_id: Option<&'a str>,
}

/// The grouping column's value on one record. None means the record carried
/// none, which is a group of its own rather than a reason to drop the record.
fn key_of(row: &UsageRecord, group_by: UsageGroupBy) -> Option<String> {
    match group_by {
        UsageGroupBy::User => row.user_id.clone(),
        UsageGroupBy::ApiKey => row.api_key_id.clone(),

        UsageGroupBy::Model => Some(row.model.clone()),
        UsageGroupBy::Operation => Some(row.operation.clone()),
        UsageGroupBy::Provider => row.provider_id.clone(),
        UsageGroupBy::Credential => row.credential_id.clone(),
    }
}

/// One accumulator, shared by the summary, every group and every bucket.
#[derive(Clone, Default)]
struct Totals {
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,
    cache_creation_5m_tokens: u64,
    cache_creation_30m_tokens: u64,
    cache_creation_1h_tokens: u64,
    reasoning_tokens: u64,
    cost: Decimal,
    priced: bool,
    quantities: BTreeMap<String, Decimal>,
}

impl Totals {
    /// One whole record: its tokens, its settled cost and one request.
    fn add(&mut self, row: &UsageRecord) {
        self.requests += 1;
        self.add_tokens(&UsageTokensDto::from_row(row));
        for (key, value) in row.quantities() {
            let sum = self.quantities.entry(key).or_default();
            *sum = sum.saturating_add(value);
        }
        if let Some(cost) = row.cost {
            self.cost += cost.decimal();
        }
        self.priced |= row.cost.is_some();
    }

    fn add_tokens(&mut self, tokens: &UsageTokensDto) {
        let add = |total: &mut u64, value: Option<u64>| {
            *total = total.saturating_add(value.unwrap_or(0));
        };
        add(&mut self.input_tokens, tokens.input_tokens);
        add(&mut self.output_tokens, tokens.output_tokens);
        add(&mut self.cached_input_tokens, tokens.cached_input_tokens);
        add(
            &mut self.cache_creation_5m_tokens,
            tokens.cache_creation_5m_tokens,
        );
        add(
            &mut self.cache_creation_30m_tokens,
            tokens.cache_creation_30m_tokens,
        );
        add(
            &mut self.cache_creation_1h_tokens,
            tokens.cache_creation_1h_tokens,
        );
        add(&mut self.reasoning_tokens, tokens.reasoning_tokens);
    }

    fn summary(self, scan: &ScanOutcome) -> UsageSummaryDto {
        UsageSummaryDto {
            requests: self.requests,
            quantities: self
                .quantities
                .into_iter()
                .map(|(k, v)| (k, v.normalize().to_string()))
                .collect(),
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_input_tokens: self.cached_input_tokens,
            cache_creation_tokens: self
                .cache_creation_5m_tokens
                .saturating_add(self.cache_creation_30m_tokens)
                .saturating_add(self.cache_creation_1h_tokens),
            cache_creation_5m_tokens: self.cache_creation_5m_tokens,
            cache_creation_30m_tokens: self.cache_creation_30m_tokens,
            cache_creation_1h_tokens: self.cache_creation_1h_tokens,
            reasoning_tokens: self.reasoning_tokens,
            cost: self.cost.normalize().to_string(),
            currency: self.priced.then(|| "USD".into()),
            truncated: scan.truncated,
            scanned: scan.scanned,
        }
    }
}
