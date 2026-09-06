use bytes::Bytes;
use http::{Response, StatusCode, request::Parts};
use serde::Deserialize;

use crate::auth::{now, verify_same_origin};
use crate::dto::{OAuthSessionDto, OAuthSessionPageDto};
use crate::{AdminError, PortalIdentity, State, response};

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ListQuery {
    active_only: bool,
    limit: u64,
    offset: u64,
}

impl Default for ListQuery {
    fn default() -> Self {
        Self {
            active_only: true,
            limit: 20,
            offset: 0,
        }
    }
}

pub(super) async fn list(
    state: &impl State,
    identity: &PortalIdentity,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query: ListQuery = serde_urlencoded::from_str(parts.uri.query().unwrap_or_default())
        .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    if !(1..=100).contains(&query.limit) || query.offset > i64::MAX as u64 {
        return Err(AdminError::BadRequest("invalid pagination".into()));
    }
    let page = state
        .store()
        .oauth_sessions(
            identity.user_id,
            now()?,
            query.active_only,
            query.limit,
            query.offset,
        )
        .await?;
    response::json(
        StatusCode::OK,
        &OAuthSessionPageDto {
            total_logins: page.total_logins,
            active_sessions: page.active_sessions,
            total: page.total,
            sessions: page
                .sessions
                .into_iter()
                .map(|row| OAuthSessionDto {
                    id: row.id,
                    client_id: row.client_id,
                    client_name: row.client_name,
                    logged_in_at: row.logged_in_at,
                    last_refreshed_at: row.last_refreshed_at,
                    refresh_count: row.refresh_count,
                    refresh_expires_at: row.refresh_expires_at,
                    revoked_at: row.revoked_at,
                    active: row.active,
                })
                .collect(),
        },
    )
}

pub(super) async fn revoke(
    state: &impl State,
    identity: &PortalIdentity,
    parts: &Parts,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    if !parts.headers.contains_key(http::header::ORIGIN) {
        return Err(AdminError::Forbidden);
    }
    verify_same_origin(parts)?;
    if !state
        .store()
        .revoke_owned_oauth_session(identity.user_id, id, now()?)
        .await?
    {
        return Err(AdminError::NotFound);
    }
    state.reload().await?;
    Ok(response::empty(StatusCode::NO_CONTENT))
}
