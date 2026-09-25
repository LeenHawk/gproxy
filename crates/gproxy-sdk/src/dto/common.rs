//! Shapes every family shares: how a list is asked for, how a page comes
//! back, and how several writes travel in one request.

use serde::{Deserialize, Deserializer, Serialize};

/// One page request, with the filters the management families understand.
///
/// This is deliberately one bag rather than a struct per family: a console
/// builds it from query parameters, and a filter a family does not use is
/// ignored rather than rejected. `page` is 1-based; both fields have defaults
/// so an empty query is a valid first page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ListQuery {
    /// 1-based. Zero and absent both mean the first page.
    pub page: Option<u64>,
    /// Clamped to 1..=500; absent means 50.
    pub page_size: Option<u64>,
    /// Case-insensitive substring of the family's natural name column.
    pub search: Option<String>,
    pub provider_id: Option<String>,
    pub route_id: Option<String>,
    pub rule_set_id: Option<String>,
    pub price_rule_id: Option<String>,
    pub credential_id: Option<String>,
    pub model_id: Option<String>,
    pub owner_kind: Option<String>,
    pub owner_id: Option<String>,
    pub enabled: Option<bool>,
    /// Any of these `(ownerKind, ownerId)` pairs, for a host whose tenant
    /// spans several owners. Set by the host, never read off the wire; when
    /// non-empty it replaces `ownerKind`/`ownerId`.
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub owner_any: Vec<(String, String)>,
}

impl ListQuery {
    /// Offset and limit, with the clamps documented on the fields. A caller
    /// cannot ask for an unbounded page: a management list is read by a person.
    pub fn bounds(&self) -> (u64, u64) {
        let limit = self.page_size.unwrap_or(50).clamp(1, 500);
        let page = self.page.unwrap_or(1).max(1);
        (page.saturating_sub(1).saturating_mul(limit), limit)
    }
}

/// One page of results. `total` counts every row the filters match, not the
/// page, so a console can render a pager without a second request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

impl<T> Page<T> {
    /// A store page of rows as a page of DTOs. Counts and bounds are carried
    /// over unchanged; only the items are converted.
    pub fn convert<M>(page: gproxy_store::Page<M>, map: impl Fn(M) -> T) -> Self {
        Self {
            items: page.items.into_iter().map(map).collect(),
            total: page.total,
            offset: page.offset,
            limit: page.limit,
        }
    }
}

/// One step of a batch write. Every item of one batch lands in a single
/// revision commit: either all of them are durable or none is.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub enum BatchItem<W, P> {
    Create(W),
    Update(BatchPatch<P>),
    Delete(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct BatchPatch<P> {
    pub id: String,
    pub patch: P,
}

/// Deserializer for a patch field over a nullable column. Absent leaves the
/// column alone (`None`), an explicit `null` clears it (`Some(None)`), and a
/// value sets it (`Some(Some(v))`). Without this, serde collapses the first
/// two cases and a column could never be cleared.
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}
