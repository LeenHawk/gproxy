use crate::dto::{
    AliasDto, CredentialDto, CredentialHealthDto, ModelAliasDto, ProviderDto, RouteDto,
    RouteMemberDto,
};
pub(super) fn provider(value: &gproxy_store::records::ProviderRecord) -> ProviderDto {
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
        settings: value.settings.clone(),
        credential_strategy: value.credential_strategy.clone(),
        proxy_url: value.proxy_url.clone(),
        tls_fingerprint,
        invalid_tls_fingerprint,
        tls_fingerprint_error,
        enabled: value.enabled,
    }
}

pub(super) fn credential(
    value: &gproxy_store::records::CredentialAdminRecord,
    health: Option<&gproxy_store::records::CredentialHealthRecord>,
) -> CredentialDto {
    let (tls_fingerprint, invalid_tls_fingerprint, tls_fingerprint_error) =
        match value.tls_fingerprint.clone() {
            Some(raw) => match serde_json::from_value(raw.clone()) {
                Ok(fingerprint) => (Some(fingerprint), None, None),
                Err(error) => (None, Some(raw), Some(error.to_string())),
            },
            None => (None, None, None),
        };
    let state = health.map(|health| match health.state {
        gproxy_store::records::CredentialHealthState::Healthy => CredentialHealthDto::Healthy,
        gproxy_store::records::CredentialHealthState::Degraded => CredentialHealthDto::Degraded,
        gproxy_store::records::CredentialHealthState::Dead => CredentialHealthDto::Dead,
    });
    CredentialDto {
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
            state.unwrap_or(CredentialHealthDto::Unknown)
        } else {
            CredentialHealthDto::Disabled
        },
        health_observed_at: health.map(|health| health.observed_at),
        health_response_status: health.and_then(|health| health.response_status),
        health_detail: health.and_then(|health| health.detail.clone()),
    }
}

pub(super) fn route(value: &gproxy_store::records::RouteRecord) -> RouteDto {
    RouteDto {
        id: value.id,
        name: value.name.clone(),
        max_attempts: value.max_attempts,
        enabled: value.enabled,
    }
}

pub(super) fn route_member(value: &gproxy_store::records::RouteMemberRecord) -> RouteMemberDto {
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

pub(super) fn alias(value: &gproxy_store::records::AliasRecord) -> AliasDto {
    AliasDto {
        id: value.id,
        alias: value.alias.clone(),
        target: value.target.clone(),
        provider_id: value.provider_id,
        priority: value.priority,
        enabled: value.enabled,
    }
}

pub(super) fn model_alias(value: &gproxy_store::records::ExposedModelRecord) -> ModelAliasDto {
    ModelAliasDto {
        id: value.id,
        name: value.name.clone(),
        route_id: value.route_id,
        enabled: value.enabled,
    }
}
