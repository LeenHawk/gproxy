use gproxy_channel_api::{BoxFuture, CallerIdentity};
use gproxy_core::{CoreError, Plan, RequestCtx};
use gproxy_protocol::OperationKey;
use sha2::{Digest, Sha256};

use super::super::AppHost;

pub(in crate::host) fn authenticate<'a>(
    host: &'a AppHost,
    request: &'a RequestCtx,
) -> BoxFuture<'a, Result<CallerIdentity, CoreError>> {
    Box::pin(async move {
        let key = api_key(request).ok_or(CoreError::Unauthorized)?;
        let digest = Sha256::digest(key.as_bytes());
        let identity = host
            .services
            .control
            .key_identity(digest.as_slice())
            .filter(|identity| identity.expires_at.is_none_or(|expiry| expiry > unix_now()))
            .ok_or(CoreError::Unauthorized)?;
        Ok(identity.caller)
    })
}

pub(super) fn authorize(
    snapshot: &gproxy_store::records::ControlSnapshot,
    identity: &CallerIdentity,
    operation: Option<OperationKey>,
    plan: &Plan,
) -> Result<(), CoreError> {
    let group = operation.map(|key| key.operation.group().id());
    for provider in plan.targets.iter().map(|target| target.provider.id) {
        let applicable = snapshot.permissions.iter().filter(|permission| {
            subject_matches(&permission.subject_kind, permission.subject_id, identity)
                && permission.provider_id.is_none_or(|id| id == provider)
                && permission
                    .operation_group
                    .as_deref()
                    .is_none_or(|value| Some(value) == group)
        });
        let mut allowed = false;
        for permission in applicable {
            if !permission.allowed {
                return Err(CoreError::Forbidden("permission denied".into()));
            }
            allowed = true;
        }
        if !allowed {
            return Err(CoreError::Forbidden("permission denied".into()));
        }
    }
    Ok(())
}

pub(super) fn subject_matches(kind: &str, id: i64, identity: &CallerIdentity) -> bool {
    match kind {
        "user_key" => id == identity.user_key_id,
        "user" => id == identity.user_id,
        "organization" => Some(id) == identity.org_id,
        "team" => Some(id) == identity.team_id,
        _ => false,
    }
}

pub(super) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_secs() as i64
}

fn api_key(request: &RequestCtx) -> Option<&str> {
    request
        .headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| request.headers.get("x-api-key")?.to_str().ok())
        .or_else(|| request.headers.get("x-goog-api-key")?.to_str().ok())
        .filter(|value| !value.is_empty())
}
