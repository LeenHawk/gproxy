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
    let mut settings = state.normalize_provider_settings(&request.channel, &request.settings)?;
    gproxy_channel_api::TrafficPolicyConfig::store(
        &mut settings,
        request.traffic_policy.map(Into::into),
    )
    .map_err(AdminError::BadRequest)?;
    Ok(ProviderInput {
        name: request.name,
        label: request.label,
        settings,
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
    validate_model_metadata(&request.metadata)?;
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
        metadata: request.metadata.into(),
        enabled: request.enabled,
    })
}

fn validate_model_metadata(metadata: &crate::dto::ModelMetadataDto) -> Result<(), AdminError> {
    for values in [
        metadata.input_modalities.as_deref(),
        metadata.output_modalities.as_deref(),
        metadata.supported_parameters.as_deref(),
        metadata.generation_methods.as_deref(),
        metadata.supported_actions.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let mut seen = std::collections::BTreeSet::new();
        if values
            .iter()
            .any(|value| value.trim().is_empty() || !seen.insert(value))
        {
            return Err(AdminError::BadRequest(
                "model metadata lists require unique non-blank values".into(),
            ));
        }
    }
    if metadata.reasoning_levels.as_deref().is_some_and(|levels| {
        let mut seen = std::collections::BTreeSet::new();
        levels.iter().any(|level| {
            !matches!(
                level.effort.as_str(),
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
            ) || !seen.insert(&level.effort)
        })
    }) {
        return Err(AdminError::BadRequest(
            "model reasoning levels are invalid".into(),
        ));
    }
    if metadata.service_tiers.as_deref().is_some_and(|tiers| {
        let mut seen = std::collections::BTreeSet::new();
        tiers.iter().any(|tier| {
            tier.id.trim().is_empty() || tier.name.trim().is_empty() || !seen.insert(&tier.id)
        })
    }) {
        return Err(AdminError::BadRequest(
            "model service tiers are invalid".into(),
        ));
    }
    for (value, allowed) in [
        (
            metadata.shell_type.as_deref(),
            &["unified_exec", "disabled"][..],
        ),
        (
            metadata.default_verbosity.as_deref(),
            &["low", "medium", "high"],
        ),
        (
            metadata.default_reasoning_summary.as_deref(),
            &["none", "auto", "concise", "detailed"],
        ),
        (metadata.apply_patch_tool_type.as_deref(), &["freeform"]),
        (
            metadata.web_search_tool_type.as_deref(),
            &["text", "text_and_image"],
        ),
        (metadata.truncation_mode.as_deref(), &["bytes", "tokens"]),
    ] {
        if value.is_some_and(|value| !allowed.contains(&value)) {
            return Err(AdminError::BadRequest(
                "model metadata contains an invalid enum value".into(),
            ));
        }
    }
    if metadata.truncation_mode.is_some() != metadata.truncation_limit.is_some()
        || metadata.truncation_limit.is_some_and(|value| value <= 0)
        || metadata
            .effective_context_window_percent
            .is_some_and(|value| !(1..=100).contains(&value))
    {
        return Err(AdminError::BadRequest(
            "model metadata limits are invalid".into(),
        ));
    }
    Ok(())
}
