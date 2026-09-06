use crate::{AdminError, State};

pub(super) async fn user(state: &impl State, id: i64) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if snapshot.users.iter().any(|user| user.id == id) {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown user_id".into()))
    }
}

pub(super) async fn organization(state: &impl State, id: i64) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if snapshot
        .organizations
        .iter()
        .any(|organization| organization.id == id)
    {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown organization_id".into()))
    }
}

pub(super) async fn user_scopes(
    state: &impl State,
    input: &gproxy_store::records::UserInput,
) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if let Some(organization_id) = input.organization_id
        && !snapshot
            .organizations
            .iter()
            .any(|organization| organization.id == organization_id)
    {
        return Err(AdminError::BadRequest("unknown organization_id".into()));
    }
    if let Some(team_id) = input.team_id {
        let team = snapshot
            .teams
            .iter()
            .find(|team| team.id == team_id)
            .ok_or_else(|| AdminError::BadRequest("unknown team_id".into()))?;
        if input.organization_id != Some(team.organization_id) {
            return Err(AdminError::BadRequest(
                "team_id requires its matching organization_id".into(),
            ));
        }
    }
    Ok(())
}

pub(super) async fn subject(state: &impl State, kind: &str, id: i64) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    let exists = match kind {
        "credential" => snapshot.credentials.iter().any(|value| value.id == id),
        "user_key" => snapshot.user_keys.iter().any(|value| value.id == id),
        "user" => snapshot.users.iter().any(|value| value.id == id),
        "organization" => snapshot.organizations.iter().any(|value| value.id == id),
        "team" => snapshot.teams.iter().any(|value| value.id == id),
        _ => return Err(AdminError::BadRequest("unknown subject kind".into())),
    };
    if exists {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown subject_id".into()))
    }
}

pub(super) fn operation_group(value: Option<&str>) -> Result<(), AdminError> {
    let Some(value) = value else {
        return Ok(());
    };
    if gproxy_protocol::registered_operations().any(|operation| operation.group().id() == value) {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown operation_group".into()))
    }
}

pub(super) async fn permission(
    state: &impl State,
    input: &gproxy_store::records::PermissionInput,
) -> Result<(), AdminError> {
    subject(state, &input.subject_kind, input.subject_id).await?;
    operation_group(input.operation_group.as_deref())?;
    if let Some(provider_id) = input.provider_id {
        super::super::control::validators::provider(state, provider_id).await?;
    }
    Ok(())
}

pub(super) async fn rate_limit(
    state: &impl State,
    input: &gproxy_store::records::RateLimitInput,
) -> Result<(), AdminError> {
    subject(state, &input.subject_kind, input.subject_id).await
}

pub(super) async fn quota(
    state: &impl State,
    input: &gproxy_store::records::QuotaInput,
) -> Result<(), AdminError> {
    subject(state, &input.subject_kind, input.subject_id).await
}

pub(super) async fn quota_update(
    state: &impl State,
    id: i64,
    input: &gproxy_store::records::QuotaInput,
) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    let stored = snapshot
        .quotas
        .iter()
        .find(|quota| quota.id == id)
        .ok_or(AdminError::NotFound)?;
    if stored.subject_kind != input.subject_kind || stored.subject_id != input.subject_id {
        return Err(AdminError::BadRequest(
            "quota subject cannot change; create a new quota".into(),
        ));
    }
    quota(state, input).await
}

pub(super) async fn rate_limit_update(
    state: &impl State,
    id: i64,
    input: &gproxy_store::records::RateLimitInput,
) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    let stored = snapshot
        .rate_limits
        .iter()
        .find(|limit| limit.id == id)
        .ok_or(AdminError::NotFound)?;
    if stored.subject_kind != input.subject_kind
        || stored.subject_id != input.subject_id
        || stored.window_seconds != input.window_seconds
    {
        return Err(AdminError::BadRequest(
            "rate-limit subject and window cannot change; create a new limit".into(),
        ));
    }
    rate_limit(state, input).await
}
