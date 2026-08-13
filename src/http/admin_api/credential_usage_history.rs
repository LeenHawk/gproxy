//! Permanent, provider-independent credential usage history endpoints.

use bytes::Bytes;
use http::Method;
use serde::{Deserialize, Serialize};

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::credentials::history::summarize_daily;
use crate::store::persistence::{CredentialQuotaCycleQuery, CredentialUsageDailyQuery};
use crate::util::time::unix_now;

use super::{Request, Resp, internal, parse_i64, query, segments};

#[derive(Debug, Default, Deserialize)]
struct CycleFilter {
    credential_id: Option<i64>,
    provider_id: Option<i64>,
    channel: Option<String>,
    window_key: Option<String>,
    status: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
    before_id: Option<i64>,
    limit: Option<u64>,
}

#[derive(Serialize)]
struct CycleDetail {
    cycle: crate::store::persistence::records::CredentialQuotaCycle,
    by_model: Vec<crate::store::persistence::records::CredentialQuotaCycleModel>,
}

pub(super) async fn dispatch(
    state: &AppState,
    parts: &Request,
    _body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let response = match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "credentials", id, "usage-summary"]) => {
            credential_summary(state, parts, id).await
        }
        (&Method::GET, ["admin", "credential-quota-cycles"]) => list_cycles(state, parts).await,
        (&Method::GET, ["admin", "credential-quota-cycles", id]) => {
            cycle_detail(state, parts, id).await
        }
        _ => return None,
    };
    Some(response)
}

async fn credential_summary(state: &AppState, parts: &Request, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let credential_id = parse_i64(id)?;
    if state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(internal)?
        .is_none()
    {
        return Err(ApiError::NotFound("credential not found".into()));
    }
    let rows = state
        .persistence
        .query_credential_usage_daily(&CredentialUsageDailyQuery {
            credential_id: Some(credential_id),
            ..Default::default()
        })
        .await
        .map_err(internal)?;
    Resp::json(200, &summarize_daily(&rows, unix_now()))
}

async fn list_cycles(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let filter: CycleFilter = query(parts)?;
    let rows = state
        .persistence
        .query_credential_quota_cycles(&CredentialQuotaCycleQuery {
            credential_id: filter.credential_id,
            provider_id: filter.provider_id,
            channel: filter.channel,
            window_key: filter.window_key,
            status: filter.status,
            from: filter.from,
            to: filter.to,
            before_id: filter.before_id,
            limit: filter.limit.unwrap_or(100).min(1000),
        })
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn cycle_detail(state: &AppState, parts: &Request, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let cycle = state
        .persistence
        .get_credential_quota_cycle(id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::NotFound("credential quota cycle not found".into()))?;
    let by_model = state
        .persistence
        .list_credential_quota_cycle_models(id)
        .await
        .map_err(internal)?;
    Resp::json(200, &CycleDetail { cycle, by_model })
}
