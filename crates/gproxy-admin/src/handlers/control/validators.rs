use gproxy_store::records::{AliasInput, ExposedModelInput, RouteMemberInput};

use crate::{AdminError, State};

pub(crate) fn credential_settings(
    request: &crate::dto::CredentialWriteRequest,
) -> Result<(), AdminError> {
    if request.kind.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "credential kind must not be blank".into(),
        ));
    }
    if request.weight == 0 {
        return Err(AdminError::BadRequest(
            "credential weight must be positive".into(),
        ));
    }
    if let Some(fingerprint) = &request.tls_fingerprint {
        fingerprint
            .validate()
            .map_err(|message| AdminError::BadRequest(message.into()))?;
    }
    Ok(())
}

pub(crate) async fn provider(state: &impl State, id: i64) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if snapshot.providers.iter().any(|provider| provider.id == id) {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown provider_id".into()))
    }
}

pub(crate) async fn route_member(
    state: &impl State,
    input: &RouteMemberInput,
) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if !snapshot
        .routes
        .iter()
        .any(|route| route.id == input.route_id)
    {
        return Err(AdminError::BadRequest("unknown route_id".into()));
    }
    if !snapshot
        .providers
        .iter()
        .any(|provider| provider.id == input.provider_id)
    {
        return Err(AdminError::BadRequest("unknown provider_id".into()));
    }
    if let Some(credential_id) = input.credential_id {
        let credentials = state.store().admin_credentials().await?;
        if !credentials.iter().any(|credential| {
            credential.id == credential_id && credential.provider_id == input.provider_id
        }) {
            return Err(AdminError::BadRequest(
                "credential_id does not belong to provider_id".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) async fn alias(state: &impl State, input: &AliasInput) -> Result<(), AdminError> {
    if let Some(provider_id) = input.provider_id {
        provider(state, provider_id).await?;
    }
    Ok(())
}

pub(crate) async fn model_alias(
    state: &impl State,
    input: &ExposedModelInput,
) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if snapshot
        .routes
        .iter()
        .any(|route| route.id == input.route_id)
    {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown route_id".into()))
    }
}

pub(crate) async fn price_rule(
    state: &impl State,
    provider_id: Option<i64>,
) -> Result<(), AdminError> {
    if let Some(provider_id) = provider_id {
        provider(state, provider_id).await?;
    }
    Ok(())
}

pub(crate) async fn price_rate(state: &impl State, rule_id: i64) -> Result<(), AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if snapshot.price_rules.iter().any(|rule| rule.id == rule_id) {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown rule_id".into()))
    }
}
