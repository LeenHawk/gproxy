//! Usage lists and aggregates over structured usage records.

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

    /// Totals over every record the filters match, unless a scan cap is requested.
    pub async fn summary(&self, query: UsageQuery) -> SdkResult<UsageSummaryDto> {
        Ok(self.aggregate(query, None, None).await?.0)
    }

    pub async fn group(&self, query: UsageGroupQuery) -> SdkResult<Vec<UsageGroupDto>> {
        Ok(self
            .aggregate(query.filter, Some(query.group_by), None)
            .await?
            .1)
    }

    pub async fn trend(&self, query: UsageTrendQuery) -> SdkResult<Vec<UsageTrendPointDto>> {
        Ok(self
            .aggregate(query.filter, None, Some(query.bucket_ms))
            .await?
            .2)
    }

    /// Summary, optional groups and trend from one paged scan. All three
    /// describe the same rows even while new requests are being settled.
    pub async fn aggregate(
        &self,
        query: UsageQuery,
        group_by: Option<UsageGroupBy>,
        bucket_ms: Option<i64>,
    ) -> SdkResult<(UsageSummaryDto, Vec<UsageGroupDto>, Vec<UsageTrendPointDto>)> {
        let (from_ms, width, count) = if let Some(width) = bucket_ms {
            let from = query
                .from_ms
                .ok_or_else(|| SdkError::invalid("a trend needs fromMs"))?;
            let to = query
                .to_ms
                .ok_or_else(|| SdkError::invalid("a trend needs toMs"))?;
            if width <= 0 {
                return Err(SdkError::invalid("bucketMs must be positive"));
            }
            let span = to
                .checked_sub(from)
                .filter(|span| *span > 0)
                .ok_or_else(|| SdkError::invalid("toMs must be after fromMs"))?;
            let count = (span - 1) / width + 1;
            if count > MAX_TREND_BUCKETS {
                return Err(SdkError::invalid(format!(
                    "a {width}ms bucket over this range is {count} buckets, more than the {MAX_TREND_BUCKETS} allowed"
                )));
            }
            (from, width, count as usize)
        } else {
            (0, 1, 0)
        };
        let mut totals = Totals::default();
        let mut groups: BTreeMap<Option<String>, Totals> = BTreeMap::new();
        let mut buckets = vec![Totals::default(); count];
        let scan = self
            .scan(&Filters::from(&query), cap(query.max_scan_rows), |row| {
                totals.add(row);
                if let Some(group_by) = group_by {
                    groups.entry(key_of(row, group_by)).or_default().add(row);
                }
                if bucket_ms.is_some() {
                    buckets[((row.started_at_ms - from_ms) / width) as usize].add(row);
                }
            })
            .await?;
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_by(|a, b| {
            b.1.cost
                .cmp(&a.1.cost)
                .then(b.1.requests.cmp(&a.1.requests))
                .then(a.0.cmp(&b.0))
        });
        let groups = groups
            .into_iter()
            .map(|(key, totals)| UsageGroupDto {
                key,
                summary: totals.summary(&scan),
            })
            .collect();
        let trend = buckets
            .into_iter()
            .enumerate()
            .map(|(index, totals)| {
                let start_ms = from_ms + index as i64 * width;
                UsageTrendPointDto {
                    start_ms,
                    end_ms: start_ms.saturating_add(width).min(query.to_ms.unwrap()),
                    summary: totals.summary(&scan),
                }
            })
            .collect();
        Ok((totals.summary(&scan), groups, trend))
    }

    /// Read the rows the column filters match, oldest first, handing each to
    /// `visit`, and optionally stop at `cap` matching upstream rows.
    async fn scan(
        &self,
        filters: &Filters<'_>,
        cap: Option<u64>,
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

/// Only explicitly requested row budgets limit aggregation.
fn cap(requested: Option<u64>) -> Option<u64> {
    requested.map(|cap| cap.clamp(1, MAX_SCAN_ROWS))
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
