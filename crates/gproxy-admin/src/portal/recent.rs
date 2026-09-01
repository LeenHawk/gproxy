use std::collections::BTreeMap;

use bytes::Bytes;
use http::request::Parts;
use http::{Response, StatusCode};

use super::{PortalIdentity, recent_requests_enabled};
use crate::dto::{PortalRecentQueryDto, PortalRecentRequestDto};
use crate::{AdminError, State, response};

pub(super) async fn get(
    state: &impl State,
    identity: &PortalIdentity,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query =
        serde_urlencoded::from_str::<PortalRecentQueryDto>(parts.uri.query().unwrap_or_default())
            .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AdminError::BadRequest(
            "limit must be between 1 and 50".into(),
        ));
    }
    let snapshot = state.store().control_snapshot().await?;
    if !recent_requests_enabled(&snapshot.settings) {
        return Err(AdminError::Forbidden);
    }
    let providers = snapshot
        .providers
        .iter()
        .map(|provider| (provider.id, provider.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let recent = match identity.user_key_id {
        Some(key) => state.store().recent_usage_for_key(key, limit).await?,
        None => {
            state
                .store()
                .recent_usage_for_user(identity.user_id, limit)
                .await?
        }
    };
    let values = recent
        .into_iter()
        .map(|record| PortalRecentRequestDto {
            request_id: record.request_id,
            at: record.at,
            provider_name: providers.get(&record.provider_id).cloned(),
            operation: record.operation,
            upstream_model: record.upstream_model,
            input_tokens: record.input_tokens,
            output_tokens: record.output_tokens,
            cached_input_tokens: record.cached_input_tokens,
            cost: record.cost.normalize().to_string(),
            usage_source: record.usage_source,
            ended: record.ended,
            latency_ms: record.latency_ms,
        })
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &values)
}
