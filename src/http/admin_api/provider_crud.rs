//! Provider CRUD responses that differ from the shared persistence records.

use std::collections::HashMap;

use http::Method;

use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::Provider;

use super::{Request, Resp, internal, parse_i64, segments};

#[derive(serde::Serialize)]
struct ProviderView {
    #[serde(flatten)]
    provider: Provider,
    credential_count: usize,
}

pub(super) async fn dispatch(state: &AppState, parts: &Request) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "providers"]) => Some(list(state, parts).await),
        (&Method::GET, ["admin", "providers", id]) => Some(get(state, parts, id).await),
        (&Method::DELETE, ["admin", "providers", id]) => Some(delete(state, parts, id).await),
        _ => None,
    }
}

async fn list(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let (providers, credentials) = futures_util::try_join!(
        state.persistence.list_providers(),
        state.persistence.list_all_credentials(),
    )
    .map_err(internal)?;
    let mut counts = HashMap::new();
    for credential in credentials {
        *counts.entry(credential.provider_id).or_insert(0) += 1;
    }
    let providers = providers
        .into_iter()
        .map(|provider| ProviderView {
            credential_count: counts.get(&provider.id).copied().unwrap_or(0),
            provider,
        })
        .collect::<Vec<_>>();
    Resp::json(200, &providers)
}

async fn get(state: &AppState, parts: &Request, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let provider = state
        .persistence
        .get_provider(id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::NotFound("not found".into()))?;
    Resp::json(200, &provider)
}

async fn delete(state: &AppState, parts: &Request, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    if state
        .persistence
        .delete_provider(id)
        .await
        .map_err(internal)?
    {
        invalidate(state).await;
        Ok(Resp::no_content())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}
