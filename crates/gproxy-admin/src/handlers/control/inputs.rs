use gproxy_store::records::{
    AliasInput, ExposedModelInput, ProviderInput, ProviderModelInput, RouteInput, RouteMemberInput,
};

use crate::dto::{
    AliasWriteRequest, ModelAliasWriteRequest, ProviderModelWriteRequest, ProviderWriteRequest,
    RouteMemberWriteRequest, RouteWriteRequest,
};
use crate::{AdminError, State};

pub(super) fn provider(
    state: &impl State,
    request: ProviderWriteRequest,
) -> Result<ProviderInput, AdminError> {
    if request.name.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "provider name must not be blank".into(),
        ));
    }
    if !matches!(
        request.credential_strategy.as_str(),
        "round_robin" | "sticky"
    ) {
        return Err(AdminError::BadRequest(
            "credential_strategy must be round_robin or sticky".into(),
        ));
    }
    if !state
        .channel_catalogue()
        .iter()
        .any(|channel| channel.id == request.channel)
    {
        return Err(AdminError::BadRequest("unknown runtime channel".into()));
    }
    if let Some(fingerprint) = &request.tls_fingerprint {
        fingerprint
            .validate()
            .map_err(|message| AdminError::BadRequest(message.into()))?;
    }
    Ok(ProviderInput {
        name: request.name,
        label: request.label,
        settings: state.normalize_provider_settings(&request.channel, &request.settings)?,
        channel: request.channel,
        credential_strategy: request.credential_strategy,
        proxy_url: request.proxy_url,
        tls_fingerprint: request
            .tls_fingerprint
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| AdminError::BadRequest(error.to_string()))?,
        enabled: request.enabled,
    })
}

pub(super) fn route(request: RouteWriteRequest) -> Result<RouteInput, AdminError> {
    if request.name.trim().is_empty() || request.max_attempts == 0 {
        return Err(AdminError::BadRequest(
            "route name must not be blank and max_attempts must be positive".into(),
        ));
    }
    Ok(RouteInput {
        name: request.name,
        max_attempts: request.max_attempts,
        enabled: request.enabled,
    })
}

pub(super) fn route_member(
    request: RouteMemberWriteRequest,
) -> Result<RouteMemberInput, AdminError> {
    if request.upstream_model.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "upstream_model must not be blank".into(),
        ));
    }
    if request.weight == 0 {
        return Err(AdminError::BadRequest(
            "route member weight must be positive".into(),
        ));
    }
    Ok(RouteMemberInput {
        route_id: request.route_id,
        provider_id: request.provider_id,
        credential_id: request.credential_id,
        upstream_model: request.upstream_model,
        tier: request.tier,
        weight: request.weight,
        enabled: request.enabled,
    })
}

pub(super) fn alias(request: AliasWriteRequest) -> Result<AliasInput, AdminError> {
    if request.alias.trim().is_empty() || request.target.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "alias and target must not be blank".into(),
        ));
    }
    Ok(AliasInput {
        alias: request.alias,
        target: request.target,
        provider_id: request.provider_id,
        priority: request.priority,
        enabled: request.enabled,
    })
}

pub(super) fn model_alias(
    request: ModelAliasWriteRequest,
) -> Result<ExposedModelInput, AdminError> {
    if request.name.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "model alias name must not be blank".into(),
        ));
    }
    Ok(ExposedModelInput {
        name: request.name,
        route_id: request.route_id,
        enabled: request.enabled,
    })
}

pub(super) fn provider_model(
    request: ProviderModelWriteRequest,
) -> Result<ProviderModelInput, AdminError> {
    if request.model_id.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "provider model id must not be blank".into(),
        ));
    }
    if [request.context_window, request.max_output_tokens]
        .into_iter()
        .flatten()
        .any(|value| value <= 0)
    {
        return Err(AdminError::BadRequest(
            "model token limits must be positive".into(),
        ));
    }
    gproxy_store::records::parse_model_variants(request.variants.as_ref())
        .map_err(AdminError::BadRequest)?;
    Ok(ProviderModelInput {
        provider_id: request.provider_id,
        model_id: request.model_id.trim().to_owned(),
        display_name: request
            .display_name
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        variants: request.variants,
        context_window: request.context_window,
        max_output_tokens: request.max_output_tokens,
        thinking_supported: request.thinking_supported,
        thinking_adaptive_supported: request.thinking_adaptive_supported,
        thinking_enabled_supported: request.thinking_enabled_supported,
        enabled: request.enabled,
    })
}
