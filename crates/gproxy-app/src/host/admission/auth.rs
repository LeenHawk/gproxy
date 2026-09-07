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
        if let Ok(identity) = authenticate_headers(host, &request.headers) {
            return Ok(identity);
        }
        let token = api_key(&request.headers).ok_or(CoreError::Unauthorized)?;
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let identity = host
            .services
            .store
            .oauth_access_identity(&digest, unix_now())
            .await
            .map_err(|error| CoreError::Store(gproxy_core::error::StoreError(error.to_string())))?
            .ok_or(CoreError::Unauthorized)?;
        Ok(CallerIdentity {
            oauth_access_digest: Some(digest),
            user_id: identity.user_id,
            user_key_id: identity.user_key_id,
            org_id: identity.organization_id,
            team_id: identity.team_id,
        })
    })
}

pub(crate) fn authenticate_headers(
    host: &AppHost,
    headers: &http::HeaderMap,
) -> Result<CallerIdentity, CoreError> {
    let key = api_key(headers).ok_or(CoreError::Unauthorized)?;
    let identity = crate::control::user_key_digests(key)
        .find_map(|(version, digest)| host.services.control.key_identity(version, &digest))
        .filter(|identity| identity.expires_at.is_none_or(|expiry| expiry > unix_now()))
        .ok_or(CoreError::Unauthorized)?;
    Ok(identity.caller)
}

pub(crate) fn authorize(
    snapshot: &gproxy_store::records::ControlSnapshot,
    identity: &CallerIdentity,
    operation: Option<OperationKey>,
    plan: &Plan,
) -> Result<Plan, CoreError> {
    let mut plan = plan.clone();
    plan.targets
        .retain(|target| provider_permitted(snapshot, identity, operation, target.provider.id));
    if plan.targets.is_empty() {
        return Err(CoreError::Forbidden("permission denied".into()));
    }
    Ok(plan)
}

pub(crate) fn catalogue_permitted(
    snapshot: &gproxy_store::records::ControlSnapshot,
    identity: &CallerIdentity,
    provider: i64,
    oauth: bool,
) -> bool {
    [
        gproxy_protocol::OperationGroup::GenerateContent,
        gproxy_protocol::OperationGroup::Models,
    ]
    .into_iter()
    .filter(|group| !oauth || *group == gproxy_protocol::OperationGroup::GenerateContent)
    .any(|group| group_permitted(snapshot, identity, Some(group.id()), provider))
}

pub(crate) fn provider_permitted(
    snapshot: &gproxy_store::records::ControlSnapshot,
    identity: &CallerIdentity,
    operation: Option<OperationKey>,
    provider: i64,
) -> bool {
    if operation
        .is_some_and(|key| key.operation().group() == gproxy_protocol::OperationGroup::Models)
    {
        return catalogue_permitted(
            snapshot,
            identity,
            provider,
            identity.oauth_access_digest.is_some(),
        );
    }
    group_permitted(
        snapshot,
        identity,
        operation.map(|key| key.operation().group().id()),
        provider,
    )
}

fn group_permitted(
    snapshot: &gproxy_store::records::ControlSnapshot,
    identity: &CallerIdentity,
    group: Option<&str>,
    provider: i64,
) -> bool {
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
            return false;
        }
        allowed = true;
    }
    allowed
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

pub(in crate::host) fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_secs() as i64
}

fn api_key(headers: &http::HeaderMap) -> Option<&str> {
    headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| headers.get("x-api-key")?.to_str().ok())
        .or_else(|| headers.get("x-goog-api-key")?.to_str().ok())
        .filter(|value| !value.is_empty())
}

pub(super) async fn oauth_admission(
    host: &AppHost,
    identity: &CallerIdentity,
    operation: Option<OperationKey>,
) -> Result<Option<CallerIdentity>, CoreError> {
    let Some(digest) = identity.oauth_access_digest.as_ref() else {
        if host.services.control.is_oauth_key(identity.user_key_id) {
            return Err(CoreError::Unauthorized);
        }
        return Ok(None);
    };
    let current = host
        .services
        .store
        .oauth_access_identity(digest, unix_now())
        .await
        .map_err(|error| CoreError::Store(gproxy_core::error::StoreError(error.to_string())))?
        .filter(|current| {
            current.user_key_id == identity.user_key_id && current.user_id == identity.user_id
        })
        .ok_or(CoreError::Unauthorized)?;
    let group = operation.map(|key| key.operation().group());
    if current.client_id != gproxy_channel_api::CODEX_OAUTH_CLIENT_ID
        && (current.scopes != gproxy_channel_api::GPROXY_OAUTH_SCOPE
            || !matches!(
                group,
                Some(
                    gproxy_protocol::OperationGroup::Models
                        | gproxy_protocol::OperationGroup::GenerateContent
                        | gproxy_protocol::OperationGroup::CountTokens
                        | gproxy_protocol::OperationGroup::Compact
                )
            ))
    {
        return Err(CoreError::Forbidden(
            "OAuth grant only permits model access".into(),
        ));
    }
    Ok(Some(CallerIdentity {
        oauth_access_digest: identity.oauth_access_digest,
        user_id: current.user_id,
        user_key_id: current.user_key_id,
        org_id: current.organization_id,
        team_id: current.team_id,
    }))
}
