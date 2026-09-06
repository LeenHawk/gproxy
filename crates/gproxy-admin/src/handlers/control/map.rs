use crate::dto::{
    AliasDto, CredentialDto, CredentialHealthDto, CredentialModelHealthDto, ModelAliasDto,
    ProviderDto, ProviderModelDto, RouteDto, RouteMemberDto,
};
pub(in crate::handlers) fn provider(value: &gproxy_store::records::ProviderRecord) -> ProviderDto {
    let mut settings = value.settings.clone();
    let traffic_policy = gproxy_channel_api::TrafficPolicyConfig::remove_from(&mut settings)
        .ok()
        .flatten()
        .map(Into::into);
    let (tls_fingerprint, invalid_tls_fingerprint, tls_fingerprint_error) =
        match value.tls_fingerprint.clone() {
            Some(raw) => match serde_json::from_value(raw.clone()) {
                Ok(fingerprint) => (Some(fingerprint), None, None),
                Err(error) => (None, Some(raw), Some(error.to_string())),
            },
            None => (None, None, None),
        };
    ProviderDto {
        id: value.id,
        name: value.name.clone(),
        label: value.label.clone(),
        channel: value.channel.clone(),
        settings,
        traffic_policy,
        credential_strategy: value.credential_strategy.clone(),
        proxy_url: value.proxy_url.clone(),
        tls_fingerprint,
        invalid_tls_fingerprint,
        tls_fingerprint_error,
        enabled: value.enabled,
    }
}

pub(in crate::handlers) fn credential(
    value: &gproxy_store::records::CredentialAdminRecord,
    health: &[gproxy_store::records::CredentialHealthRecord],
) -> CredentialDto {
    let (tls_fingerprint, invalid_tls_fingerprint, tls_fingerprint_error) =
        match value.tls_fingerprint.clone() {
            Some(raw) => match serde_json::from_value(raw.clone()) {
                Ok(fingerprint) => (Some(fingerprint), None, None),
                Err(error) => (None, Some(raw), Some(error.to_string())),
            },
            None => (None, None, None),
        };
    let current = health
        .iter()
        .filter(|health| health.credential_version == value.version)
        .collect::<Vec<_>>();
    let summary = current.iter().copied().max_by_key(|health| {
        (
            health_rank(health.state),
            health.observed_at,
            health.version,
        )
    });
    let model_health = current
        .iter()
        .map(|health| CredentialModelHealthDto {
            model: health.model.clone(),
            health: health_dto(health.state),
            observed_at: health.observed_at,
            response_status: health.response_status,
            detail: health.detail.clone(),
        })
        .collect();
    CredentialDto {
        quota_capabilities: None,
        id: value.id,
        provider_id: value.provider_id,
        label: value.label.clone(),
        kind: value.kind.clone(),
        version: value.version,
        enabled: value.enabled,
        weight: value.weight,
        rpm_limit: value.rpm_limit,
        tpm_limit: value.tpm_limit,
        proxy_url: value.proxy_url.clone(),
        tls_fingerprint,
        invalid_tls_fingerprint,
        tls_fingerprint_error,
        health: if value.enabled {
            summary
                .map(|health| health_dto(health.state))
                .unwrap_or(CredentialHealthDto::Unknown)
        } else {
            CredentialHealthDto::Disabled
        },
        health_observed_at: summary.map(|health| health.observed_at),
        health_response_status: summary.and_then(|health| health.response_status),
        health_detail: summary.and_then(|health| health.detail.clone()),
        model_health,
    }
}

fn health_dto(state: gproxy_store::records::CredentialHealthState) -> CredentialHealthDto {
    match state {
        gproxy_store::records::CredentialHealthState::Healthy => CredentialHealthDto::Healthy,
        gproxy_store::records::CredentialHealthState::Degraded => CredentialHealthDto::Degraded,
        gproxy_store::records::CredentialHealthState::Dead => CredentialHealthDto::Dead,
    }
}

fn health_rank(state: gproxy_store::records::CredentialHealthState) -> u8 {
    match state {
        gproxy_store::records::CredentialHealthState::Healthy => 0,
        gproxy_store::records::CredentialHealthState::Degraded => 1,
        gproxy_store::records::CredentialHealthState::Dead => 2,
    }
}

pub(in crate::handlers) fn route(value: &gproxy_store::records::RouteRecord) -> RouteDto {
    RouteDto {
        id: value.id,
        name: value.name.clone(),
        max_attempts: value.max_attempts,
        enabled: value.enabled,
    }
}

pub(in crate::handlers) fn route_member(
    value: &gproxy_store::records::RouteMemberRecord,
) -> RouteMemberDto {
    RouteMemberDto {
        id: value.id,
        route_id: value.route_id,
        provider_id: value.provider_id,
        credential_id: value.credential_id,
        upstream_model: value.upstream_model.clone(),
        tier: value.tier,
        weight: value.weight,
        enabled: value.enabled,
    }
}

pub(in crate::handlers) fn alias(value: &gproxy_store::records::AliasRecord) -> AliasDto {
    AliasDto {
        id: value.id,
        alias: value.alias.clone(),
        target: value.target.clone(),
        provider_id: value.provider_id,
        priority: value.priority,
        enabled: value.enabled,
    }
}

pub(in crate::handlers) fn model_alias(
    value: &gproxy_store::records::ExposedModelRecord,
) -> ModelAliasDto {
    ModelAliasDto {
        id: value.id,
        name: value.name.clone(),
        route_id: value.route_id,
        enabled: value.enabled,
    }
}

pub(in crate::handlers) fn provider_model(
    value: &gproxy_store::records::ProviderModelRecord,
) -> ProviderModelDto {
    ProviderModelDto {
        id: value.id,
        provider_id: value.provider_id,
        model_id: value.model_id.clone(),
        display_name: value.display_name.clone(),
        variants: value.variants.clone(),
        context_window: value.context_window,
        max_output_tokens: value.max_output_tokens,
        thinking_supported: value.thinking_supported,
        thinking_adaptive_supported: value.thinking_adaptive_supported,
        thinking_enabled_supported: value.thinking_enabled_supported,
        metadata: value.metadata.clone().into(),
        enabled: value.enabled,
    }
}
