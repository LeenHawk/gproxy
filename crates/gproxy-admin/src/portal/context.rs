use bytes::Bytes;
use http::{Response, StatusCode};

use super::{PortalIdentity, recent_requests_enabled};
use crate::dto::PortalContextDto;
use crate::{AdminError, State, response};

pub(super) async fn get(
    state: &impl State,
    identity: &PortalIdentity,
) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    response::json(
        StatusCode::OK,
        &PortalContextDto {
            user_name: identity.user_name.clone(),
            key_prefix: identity.key_prefix.clone(),
            key_label: identity.key_label.clone(),
            expires_at: identity.expires_at,
            recent_requests_enabled: recent_requests_enabled(&snapshot.settings),
        },
    )
}
