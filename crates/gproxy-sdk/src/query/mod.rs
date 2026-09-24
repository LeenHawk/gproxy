//! Reading back what happened: usage records, quota windows and credential
//! cycles, and captured request logs.
//!
//! Nothing here writes, and nothing here advances a revision. Three families,
//! one accessor each, all over the same durable rows the engine left behind.
//!
//! Two shapes of paging, because the two halves of this module page different
//! things. Usage records and quota windows use the offset [`Page`] every
//! management family uses: a console renders a pager over them and the row
//! counts are small. Request logs use a cursor, because they are an append-only
//! stream whose head moves while a person reads it, and an offset page would
//! silently repeat or skip rows as it did.
//!
//! [`Page`]: crate::Page

mod logs;
mod quota;
mod usage;

pub use logs::Logs;
pub use quota::QuotaQueries;
pub use usage::Usage;

use std::sync::Arc;

use crate::handle::Inner;

/// How far an aggregation may read before it stops and says so.
///
/// `usage_records.metrics` is a JSON document, so a sum over token counts or
/// a cut by provider cannot be pushed into the database: the rows are read and
/// folded here. That is fine for a month of one user's traffic and ruinous for
/// a year of a deployment's, so every aggregate carries a row budget. Reaching
/// it sets `truncated` on the answer rather than returning a smaller number as
/// if it were the whole truth.
pub const MAX_SCAN_ROWS: u64 = 50_000;

/// How many records one scan reads per round trip. Small enough that a scan
/// never holds the whole range in memory at once, large enough that the cap
/// above is a handful of queries rather than hundreds.
pub(crate) const SCAN_CHUNK: u64 = 1_000;

/// The most buckets a trend may produce. A one-minute bucket over a month is
/// already 43 200 points, which no chart draws; anything beyond this is a
/// caller that meant a different bucket width.
pub const MAX_TREND_BUCKETS: i64 = 5_000;

/// How many capture events one detail response carries, across the downstream
/// record and all of its upstream attempts.
pub const MAX_DETAIL_EVENTS: u64 = 2_000;

/// The read side of a handle, one accessor per family.
pub struct Query<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

impl<'a, C> Query<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }

    /// Usage records, and the aggregates over them.
    pub fn usage(&self) -> Usage<'a, C> {
        Usage::new(self.inner)
    }
    /// Budget and limit windows, their settlements, and what an upstream said
    /// about a credential's own quota.
    pub fn quota(&self) -> QuotaQueries<'a, C> {
        QuotaQueries::new(self.inner)
    }
    /// Captured downstream requests and their upstream attempts.
    pub fn logs(&self) -> Logs<'a, C> {
        Logs::new(self.inner)
    }
}

/// Page bounds with the clamps every family shares: a read is done by a
/// person, so there is no unbounded page.
pub(crate) fn bounds(page: Option<u64>, page_size: Option<u64>) -> (u64, u64) {
    let limit = page_size.unwrap_or(50).clamp(1, 500);
    let page = page.unwrap_or(1).max(1);
    (page.saturating_sub(1).saturating_mul(limit), limit)
}

/// A filter value that says something. A blank string filters nothing, which
/// is never what an empty form field meant.
pub(crate) fn filter(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}
