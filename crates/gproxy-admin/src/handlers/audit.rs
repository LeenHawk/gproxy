use bytes::Bytes;
use http::request::Parts;
use http::{Response, StatusCode};

use crate::dto::AuditEventDto;
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(crate) async fn record(
    state: &impl State,
    actor_user_id: i64,
    client_ip: Option<&str>,
    event: crate::route::AuditDescriptor,
) -> Result<(), AdminError> {
    state
        .store()
        .record_audit_event(&gproxy_store::records::AuditEventInput {
            actor_user_id,
            action: event.action,
            target_kind: event.target_kind,
            target_id: event.target_id,
            at: crate::auth::now()?,
            client_ip: client_ip.map(str::to_owned),
            details: None,
        })
        .await?;
    Ok(())
}

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
            actor_user_id: value.event.actor_user_id,
            action: value.event.action,
            target_kind: value.event.target_kind,
            target_id: value.event.target_id,
            at: value.event.at,
            client_ip: value.event.client_ip,
            details: value.event.details,
        })
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &values)
}
