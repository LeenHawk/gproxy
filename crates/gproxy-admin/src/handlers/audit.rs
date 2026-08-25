use bytes::Bytes;
use http::request::Parts;
use http::{Response, StatusCode};

use crate::dto::AuditEventDto;
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn list(state: &impl State, parts: &Parts) -> Result<Response<Bytes>, AdminError> {
    let query = util::query(parts);
    let limit = util::value(&query, "limit")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| AdminError::BadRequest("limit must be an integer".into()))
        })
        .transpose()?
        .unwrap_or(100)
        .clamp(1, 500);
    let values = state
        .store()
        .audit_events(limit)
        .await?
        .into_iter()
        .map(|value| AuditEventDto {
            id: value.id,
            actor_admin_id: value.event.actor_admin_id,
            action: value.event.action,
            target_kind: value.event.target_kind,
            target_id: value.event.target_id,
            at: value.event.at,
            details: value.event.details,
        })
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &values)
}
