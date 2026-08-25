use bytes::Bytes;
use gproxy_store::records::{UsageAggregateQuery, UsageGroupBy};
use http::request::Parts;
use http::{Response, StatusCode};

use super::PortalIdentity;
use crate::dto::{PortalUsageDto, PortalUsageQueryDto};
use crate::{AdminError, State, handlers, response};

pub(super) async fn get(
    state: &impl State,
    identity: &PortalIdentity,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query =
        serde_urlencoded::from_str::<PortalUsageQueryDto>(parts.uri.query().unwrap_or_default())
            .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    let (from, to) = handlers::observability::range(query.from, query.to)?;
    let record = state
        .store()
        .usage_aggregate(&UsageAggregateQuery {
            from,
            to,
            group_by: UsageGroupBy::UserKey,
            user_key_id: Some(identity.user_key_id),
            user_id: None,
            provider_id: None,
            model: None,
        })
        .await?
        .into_iter()
        .next();
    response::json(
        StatusCode::OK,
        &PortalUsageDto {
            from,
            to,
            requests: record.as_ref().map_or(0, |record| record.requests),
            input_tokens: record.as_ref().map_or(0, |record| record.input_tokens),
            output_tokens: record.as_ref().map_or(0, |record| record.output_tokens),
            cached_input_tokens: record
                .as_ref()
                .map_or(0, |record| record.cached_input_tokens),
            cost: record.map_or_else(|| "0".into(), |record| record.cost.normalize().to_string()),
        },
    )
}
