//! Usage: the record list, and the three aggregates a console draws from it.
//!
//! Only the list is a database query, and only while it filters on columns.
//! `usage_records.metrics` is a JSON document, so no backend this crate
//! supports can sum a token count or cut a total by provider or credential;
//! the aggregates read the matching rows and fold them here. That is bounded
//! on purpose — see [`MAX_SCAN_ROWS`] — and a read that hit its bound says so
//! instead of returning a smaller number as if it were the whole truth. The
//! scan itself is core's ([`gproxy_core::usage_scan`]), shared with the
//! engine's own reads of a credential's spend.
//!
//! Cost is the one number that does not come out of the document: it has its
//! own indexed column, written once at settlement. Only a cut at the attempt —
//! the per-provider and per-credential groupings, and the provider and
//! credential filters — reads the priced amounts inside `metrics`, because
//! that is the only place the breakdown exists. [`UsageQuery`] spells out the
//! rule every aggregate applies under those filters.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use gproxy_core::usage_scan::{self, ScanOrder, ScanOutcome};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::usage::usage_record;
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder};

use super::{MAX_SCAN_ROWS, MAX_TREND_BUCKETS, bounds, filter};
use crate::{
    SdkError, SdkResult,
    dto::{
        Page, UsageExchangeDto, UsageGroupBy, UsageGroupDto, UsageGroupQuery, UsageQuery,
        UsageRecordDto, UsageRecordPage, UsageRecordQuery, UsageSummaryDto, UsageTokensDto,
        UsageTrendPointDto, UsageTrendQuery,
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
    /// A provider or credential filter is not a column, so with one set the
    /// page is cut from a bounded newest-first scan instead of a database
    /// page, in the same order; [`UsageRecordPage`] says what `total` and
    /// `truncated` then mean.
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
        if filters.cut.is_open() {
            // The repository appends the primary key ascending to whatever
            // order it is given, which is exactly the tie-break wanted here.
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
            let page = Page::convert(page, UsageRecordDto::from);
            return Ok(UsageRecordPage {
                items: page.items,
                total: page.total,
                offset: page.offset,
                limit: page.limit,
                truncated: false,
            });
        }

        let window = offset..offset.saturating_add(limit);
        let mut total = 0u64;
        let mut items = Vec::new();
        let scan = usage_scan::scan(
            &self.inner.store,
            filters.condition(),
            ScanOrder::Newest,
            MAX_SCAN_ROWS,
            |row| {
                if !filters.cut.matches(row) {
                    return;
                }
                if window.contains(&total) {
                    items.push(UsageRecordDto::from(row.clone()));
                }
                total += 1;
            },
        )
        .await?;
        Ok(UsageRecordPage {
            items,
            total,
            offset,
            limit,
            truncated: scan.truncated,
        })
    }

    /// Totals over every record the filters match, up to the scan cap.
    pub async fn summary(&self, query: UsageQuery) -> SdkResult<UsageSummaryDto> {
        let filters = Filters::from(&query);
        let mut totals = Totals::default();
        let scan = self
            .scan(&filters, cap(query.max_scan_rows), |row| {
                let share = filters.cut.share(row);
                totals.add_share(row, &share);
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
            .scan(
                &filters,
                cap(query.filter.max_scan_rows),
                |row| match group_by {
                    UsageGroupBy::Provider => {
                        group_by_attempt(&mut groups, row, filters.cut, |exchange| {
                            exchange.provider_id.as_deref()
                        })
                    }
                    UsageGroupBy::Credential => {
                        group_by_attempt(&mut groups, row, filters.cut, |exchange| {
                            exchange.credential_id.as_deref()
                        })
                    }
                    _ => {
                        let share = filters.cut.share(row);
                        if share.matches() {
                            groups
                                .entry(key_of(row, group_by))
                                .or_default()
                                .add_share(row, &share);
                        }
                    }
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
                    let share = filters.cut.share(row);
                    bucket.add_share(row, &share);
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
    /// `visit`, and stop at `cap` rows. The attempt filters are the visitor's
    /// business: they cannot be pushed into the query.
    async fn scan(
        &self,
        filters: &Filters<'_>,
        cap: u64,
        visit: impl FnMut(&usage_record::Model),
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

/// The filters both the list and the aggregates understand. Every column
/// filter is an exact match on an indexed column; there is no substring
/// search here, because a usage table is not browsed by name. The attempt
/// filters in `cut` are exact too, but are applied to each row after it is
/// read.
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
            condition = condition.add(C::RequestId.eq(request_id));
        }
        condition
    }
}

/// The filters that name an upstream attempt rather than a record: they are
/// matched against each entry of `metrics.exchanges[]`. See [`UsageQuery`] for
/// the counting rule they impose.
#[derive(Clone, Copy)]
struct AttemptCut<'a> {
    provider_id: Option<&'a str>,
    credential_id: Option<&'a str>,
}

impl AttemptCut<'_> {
    /// No attempt filter: every record counts whole.
    fn is_open(&self) -> bool {
        self.provider_id.is_none() && self.credential_id.is_none()
    }

    fn admits(&self, exchange: &UsageExchangeDto) -> bool {
        let is = |wanted: Option<&str>, actual: &Option<String>| {
            wanted.is_none_or(|wanted| actual.as_deref() == Some(wanted))
        };
        is(self.provider_id, &exchange.provider_id)
            && is(self.credential_id, &exchange.credential_id)
    }

    /// Whether a record belongs in a filtered list at all.
    fn matches(&self, row: &usage_record::Model) -> bool {
        self.share(row).matches()
    }

    /// What one record contributes under this cut.
    fn share(&self, row: &usage_record::Model) -> Share {
        if self.is_open() {
            return Share::Whole;
        }
        Share::Attempts(
            attempts(row)
                .filter(|exchange| self.admits(exchange))
                .collect(),
        )
    }
}

/// A record's contribution to a total.
enum Share {
    /// The whole record: its settled cost and top-level tokens.
    Whole,
    /// Only these attempts, each with its own tokens and price. Empty means
    /// the record does not match.
    Attempts(Vec<UsageExchangeDto>),
}

impl Share {
    fn matches(&self) -> bool {
        match self {
            Self::Whole => true,
            Self::Attempts(attempts) => !attempts.is_empty(),
        }
    }
}

/// Every attempt one record names, in the order they ran.
fn attempts(row: &usage_record::Model) -> impl Iterator<Item = UsageExchangeDto> + '_ {
    usage_scan::exchanges(&row.metrics)
        .iter()
        .map(UsageExchangeDto::read)
}

/// The grouping column's value on one record. None means the record carried
/// none, which is a group of its own rather than a reason to drop the record.
fn key_of(row: &usage_record::Model, group_by: UsageGroupBy) -> Option<String> {
    match group_by {
        UsageGroupBy::User => row.user_id.clone(),
        UsageGroupBy::ApiKey => row.api_key_id.clone(),

        UsageGroupBy::Model => Some(row.model.clone()),
        UsageGroupBy::Operation => Some(row.operation.clone()),
        // Handled by `group_by_attempt`: neither is a column.
        UsageGroupBy::Provider | UsageGroupBy::Credential => None,
    }
}

/// A record's contribution to a per-attempt cut: by provider or by
/// credential, whichever `key` reads.
///
/// The breakdown lives inside `metrics`, one entry per upstream attempt that
/// produced usage, so a request that failed over from one provider to another
/// contributes to both — each with that attempt's own tokens and price, never
/// with the request's totals counted twice. `requests` counts the record once
/// per distinct key, not once per attempt. A record with no breakdown at all
/// (nothing reached an upstream) lands under the `None` key with its own
/// totals, because dropping it would make the groups stop summing to the
/// summary — unless an attempt filter is set, in which case such a record
/// matched nothing and the summary left it out as well.
fn group_by_attempt(
    groups: &mut BTreeMap<Option<String>, Totals>,
    row: &usage_record::Model,
    cut: AttemptCut<'_>,
    key: fn(&UsageExchangeDto) -> Option<&str>,
) {
    let mut attempts = attempts(row).peekable();
    if attempts.peek().is_none() {
        if cut.is_open() {
            groups.entry(None).or_default().add(row);
        }
        return;
    }
    let mut seen: BTreeSet<Option<String>> = BTreeSet::new();
    for exchange in attempts.filter(|exchange| cut.admits(exchange)) {
        let key = key(&exchange).map(str::to_owned);
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
    cache_creation_5m_tokens: u64,
    cache_creation_30m_tokens: u64,
    cache_creation_1h_tokens: u64,
    reasoning_tokens: u64,
    cost: Decimal,
    currency: Currency,
}

impl Totals {
    /// One record under an attempt cut: the whole of it, or one request with
    /// only the matching attempts' tokens and prices. A record that matched
    /// nothing adds nothing, not even a request.
    fn add_share(&mut self, row: &usage_record::Model, share: &Share) {
        match share {
            Share::Whole => self.add(row),
            Share::Attempts(attempts) if attempts.is_empty() => {}
            Share::Attempts(attempts) => {
                self.requests += 1;
                for exchange in attempts {
                    self.add_tokens(&exchange.tokens);
                    self.add_cost(exchange.cost.as_deref(), exchange.currency.as_deref());
                }
            }
        }
    }

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

    fn summary(self, scan: &ScanOutcome) -> UsageSummaryDto {
        UsageSummaryDto {
            requests: self.requests,
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
