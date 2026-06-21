//! `/user/usage` and `/user/usage-rollups` — portal read-only endpoints (§F7a).
//!
//! Both endpoints scope results to the authenticated session user:
//! - `user_id` is **forced** from `SessionUser.id` injected by `require_session`.
//! - `MyUsageQuery` deliberately omits a `user_id` field so a client passing
//!   `?user_id=X` in the query string will have it silently ignored (serde unknown
//!   field is dropped by default).

use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::admin::session::SessionUser;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::UsageQuery as StoreUsageQuery;
use crate::store::persistence::records::{Usage, UsageRollup};

fn internal(e: anyhow::Error) -> ApiError {
    ApiError::Internal(e.to_string())
}

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

/// Query parameters for `GET /user/usage`. No `user_id` field — it is forced
/// from the session and cannot be supplied by the caller.
#[derive(Debug, Clone, Deserialize)]
pub struct MyUsageQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
}

/// `GET /user/usage` — keyset-paginated usage rows for the authenticated user.
/// `user_id` is forced from the session; any `?user_id=` query parameter is
/// ignored (not deserialized).
pub async fn usage(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
    Query(q): Query<MyUsageQuery>,
) -> Result<Json<Vec<Usage>>, ApiError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let query = StoreUsageQuery {
        user_id: Some(u.id), // forced from session — never from request
        at_from: q.at_from,
        at_to: q.at_to,
        route_name: q.route_name,
        model: q.model,
        before_id: q.before_id,
        limit,
        ..Default::default() // provider_id stays None
    };
    Ok(Json(
        state
            .persistence
            .query_usages(&query)
            .await
            .map_err(internal)?,
    ))
}

/// `GET /user/usage-rollups?granularity=hour|day|week|month&from=&to=`
/// Returns rollup buckets that belong to the authenticated user only.
#[derive(Debug, Clone, Deserialize)]
pub struct MyRollupQuery {
    pub granularity: String,
    pub from: i64,
    pub to: i64,
}

pub async fn rollups(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
    Query(q): Query<MyRollupQuery>,
) -> Result<Json<Vec<UsageRollup>>, ApiError> {
    if !matches!(q.granularity.as_str(), "hour" | "day" | "week" | "month") {
        return Err(ApiError::BadRequest(
            "granularity must be one of hour|day|week|month".into(),
        ));
    }
    Ok(Json(
        state
            .persistence
            .list_usage_rollups(&q.granularity, q.from, q.to, Some(u.id))
            .await
            .map_err(internal)?,
    ))
}
