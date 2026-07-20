//! Session-scoped usage handlers.

use serde::Deserialize;

use crate::admin::guard::guard_session;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::UsageQuery as StoreUsageQuery;

use super::pagination;
use super::{Request, Resp, internal, query};

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

/// Deliberately omits `user_id`; the session is its only trusted source.
#[derive(Debug, Deserialize)]
struct MyUsageQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MyRollupQuery {
    pub granularity: String,
    pub from: i64,
    pub to: i64,
}

pub(super) async fn user_usage(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    let user = guard_session(state, parts).await?;
    let q: MyUsageQuery = query(parts)?;
    let page = pagination::parse(
        q.page.as_deref(),
        q.page_size.as_deref(),
        q.before_id.is_some(),
    )?;
    let store_q = StoreUsageQuery {
        user_id: Some(user.id),
        at_from: q.at_from,
        at_to: q.at_to,
        route_name: q.route_name,
        model: q.model,
        before_id: q.before_id,
        limit: q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT),
        ..Default::default()
    };
    if let Some(page) = page {
        let result = state
            .persistence
            .query_usages_page(&store_q, &page.store)
            .await
            .map_err(internal)?;
        Resp::json(200, &page.response(result))
    } else {
        let rows = state
            .persistence
            .query_usages(&store_q)
            .await
            .map_err(internal)?;
        Resp::json(200, &rows)
    }
}

pub(super) async fn user_usage_rollups(
    state: &AppState,
    parts: &Request,
) -> Result<Resp, ApiError> {
    let user = guard_session(state, parts).await?;
    let q: MyRollupQuery = query(parts)?;
    if !matches!(q.granularity.as_str(), "hour" | "day" | "week" | "month") {
        return Err(ApiError::BadRequest(
            "granularity must be one of hour|day|week|month".into(),
        ));
    }
    let rows = state
        .persistence
        .list_usage_rollups(&q.granularity, q.from, q.to, Some(user.id))
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}
