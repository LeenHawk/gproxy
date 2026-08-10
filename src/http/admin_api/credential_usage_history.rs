//! Permanent, provider-independent credential usage history endpoints.

use std::collections::HashMap;

use bytes::Bytes;
use http::Method;
use serde::{Deserialize, Serialize};

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::credentials::history::{CredentialUsageSummaryView, summarize_daily};
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

#[derive(Serialize)]
struct CredentialComparisonRow {
    credential_id: i64,
    credential_label: String,
    provider_id: i64,
    channel: String,
    supports_upstream_usage: bool,
    #[serde(flatten)]
    usage: CredentialUsageSummaryView,
    current_windows: Vec<crate::store::persistence::records::CredentialQuotaCycle>,
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
        (&Method::GET, ["admin", "credential-usage-comparison"]) => comparison(state, parts).await,
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

async fn comparison(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let credentials = state
        .persistence
        .list_all_credentials()
        .await
        .map_err(internal)?;
    let providers = state.persistence.list_providers().await.map_err(internal)?;
    let provider_by_id: HashMap<_, _> = providers
        .into_iter()
        .map(|provider| (provider.id, provider))
        .collect();
    let catalog: HashMap<_, _> = state
        .channels
        .catalog()
        .into_iter()
        .map(|entry| (entry.metadata.id, entry.metadata.usage))
        .collect();
    let daily = state
        .persistence
        .query_credential_usage_daily(&CredentialUsageDailyQuery::default())
        .await
        .map_err(internal)?;
    let open_cycles = state
        .persistence
        .query_credential_quota_cycles(&CredentialQuotaCycleQuery {
            status: Some("open".into()),
            ..Default::default()
        })
        .await
        .map_err(internal)?;

    let mut daily_by_credential = HashMap::<i64, Vec<_>>::new();
    for row in daily {
        daily_by_credential
            .entry(row.credential_id)
            .or_default()
            .push(row);
    }
    let mut cycles_by_credential = HashMap::<i64, Vec<_>>::new();
    for cycle in open_cycles {
        cycles_by_credential
            .entry(cycle.credential_id)
            .or_default()
            .push(cycle);
    }

    let now = unix_now();
    let rows: Vec<_> = credentials
        .into_iter()
        .map(|credential| {
            let provider = provider_by_id.get(&credential.provider_id);
            let channel = provider.map(|p| p.channel.clone()).unwrap_or_default();
            CredentialComparisonRow {
                credential_id: credential.id,
                credential_label: credential
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("#{}", credential.id)),
                provider_id: credential.provider_id,
                supports_upstream_usage: catalog.get(&channel).copied().unwrap_or(false),
                channel,
                usage: summarize_daily(
                    daily_by_credential
                        .get(&credential.id)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                    now,
                ),
                current_windows: cycles_by_credential
                    .remove(&credential.id)
                    .unwrap_or_default(),
            }
        })
        .collect();
    Resp::json(200, &rows)
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
