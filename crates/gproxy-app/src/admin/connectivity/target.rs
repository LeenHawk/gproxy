use gproxy_admin::AdminError;
use gproxy_admin::dto::{
    ConnectivityProxySourceDto, ConnectivityScopeDto, ConnectivityTestRequest,
};
use gproxy_core::ProviderRef;

use crate::AppHandle;

pub(super) fn resolve(
    app: &AppHandle,
    request: &ConnectivityTestRequest,
) -> Result<(ProviderRef, ConnectivityProxySourceDto), AdminError> {
    let services = &app.inner.host.services;
    let settings = services.control.settings();
    if request.scope == ConnectivityScopeDto::Proxy {
        let proxy_url = request
            .proxy_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AdminError::BadRequest("proxy URL is required".into()))?;
        return Ok((
            ProviderRef {
                id: 0,
                name: "proxy connectivity probe".into(),
                channel: String::new(),
                settings: serde_json::json!({}),
                fingerprint: None,
                proxy_url: Some(proxy_url.trim().into()),
                traffic_blacklist: settings.traffic_blacklist.clone(),
            },
            ConnectivityProxySourceDto::Proxy,
        ));
    }
    if request.scope == ConnectivityScopeDto::Global {
        let source = fallback(settings.proxy.as_ref(), settings.inherit_system_proxy);
        return Ok((
            ProviderRef {
                id: 0,
                name: "global connectivity probe".into(),
                channel: String::new(),
                settings: serde_json::json!({}),
                fingerprint: None,
                proxy_url: settings.proxy,
                traffic_blacklist: settings.traffic_blacklist.clone(),
            },
            source,
        ));
    }
    let snapshot = services.control.current();
    let credential = request.credential_id.and_then(|id| {
        snapshot
            .credentials
            .iter()
            .find(|credential| credential.id == id)
    });
    if request.scope == ConnectivityScopeDto::Credential && credential.is_none() {
        return Err(AdminError::NotFound);
    }
    let provider_id = request
        .provider_id
        .or_else(|| credential.map(|value| value.provider_id))
        .ok_or(AdminError::NotFound)?;
    let provider = snapshot
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or(AdminError::NotFound)?;
    let credential_proxy = credential.and_then(|value| value.proxy_url.clone());
    let provider_proxy = provider.proxy_url.clone();
    let proxy_url = crate::control::settings::effective_proxy(
        credential_proxy.as_deref(),
        provider_proxy.as_deref(),
        settings.proxy.as_deref(),
    );
    let source = if credential_proxy.is_some() {
        ConnectivityProxySourceDto::Credential
    } else if provider_proxy.is_some() {
        ConnectivityProxySourceDto::Provider
    } else {
        fallback(settings.proxy.as_ref(), settings.inherit_system_proxy)
    };
    let fingerprint = settings
        .spoof_emulation
        .then(|| {
            credential
                .and_then(|value| value.tls_fingerprint.as_ref())
                .or(provider.tls_fingerprint.as_ref())
                .and_then(|value| crate::control::fingerprint::parse(Some(value)))
        })
        .flatten();
    Ok((
        ProviderRef {
            id: provider.id,
            name: provider.name.clone(),
            channel: provider.channel.clone(),
            settings: gproxy_channels::canonical_provider_settings(
                &provider.channel,
                &provider.settings,
            )
            .map_err(AdminError::BadRequest)?,
            fingerprint,
            proxy_url,
            traffic_blacklist: settings.traffic_blacklist.clone(),
        },
        source,
    ))
}

fn fallback(proxy: Option<&String>, system: bool) -> ConnectivityProxySourceDto {
    if proxy.is_some() {
        ConnectivityProxySourceDto::Global
    } else if system {
        ConnectivityProxySourceDto::System
    } else {
        ConnectivityProxySourceDto::Direct
    }
}
