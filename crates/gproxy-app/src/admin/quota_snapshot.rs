use crate::AppHandle;
use gproxy_admin::{
    AdminError,
    dto::{ConnectivityScopeDto, ConnectivityTestRequest},
};
use gproxy_channel_api::{QuotaSnapshot, QuotaSource, QuotaSourceState};
use gproxy_core::ProviderRef;

pub(super) async fn sources(
    app: &AppHandle,
    id: i64,
) -> Result<(ProviderRef, u64, Vec<QuotaSource>), AdminError> {
    let services = &app.inner.host.services;
    let credential = services
        .store
        .credential(id)
        .await?
        .ok_or(AdminError::NotFound)?;
    let secret = services
        .cipher
        .open(&credential.envelope)
        .map_err(|error| AdminError::Internal(error.to_string()))?;
    let (provider, _) = super::connectivity::target::resolve(
        app,
        &ConnectivityTestRequest {
            scope: ConnectivityScopeDto::Credential,
            provider_id: None,
            credential_id: Some(id),
            proxy_url: None,
        },
    )?;
    let sources = app
        .inner
        .core
        .quota_sources(&credential.channel, &secret, &provider.settings)
        .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    let control = services.control.current();
    let current = control
        .credentials
        .iter()
        .find(|record| record.id == id)
        .ok_or(AdminError::NotFound)?;
    if current.version != credential.version {
        let latest = services.store.control_snapshot().await?;
        let stored_meta = latest
            .credentials
            .iter()
            .find(|record| record.id == id)
            .ok_or(AdminError::NotFound)?;
        let mut expected_meta = current.clone();
        expected_meta.version = stored_meta.version;
        let old_provider = control
            .providers
            .iter()
            .find(|value| value.id == credential.provider_id)
            .ok_or(AdminError::NotFound)?;
        let new_provider = latest
            .providers
            .iter()
            .find(|value| value.id == credential.provider_id)
            .ok_or(AdminError::NotFound)?;
        if expected_meta != *stored_meta
            || old_provider.channel != new_provider.channel
            || old_provider.settings != new_provider.settings
            || old_provider.proxy_url != new_provider.proxy_url
            || old_provider.tls_fingerprint != new_provider.tls_fingerprint
        {
            return Err(AdminError::Conflict(
                "credential configuration changed; retry quota query".into(),
            ));
        }
    }
    if services
        .control
        .cached_credential(id)
        .is_some_and(|record| record.version != credential.version)
    {
        services.control.forget_credential(id);
    }
    Ok((provider, credential.version, sources))
}

pub(super) async fn read(app: &AppHandle, id: i64) -> Result<QuotaSnapshot, AdminError> {
    let (_, _, capabilities) = sources(app, id).await?;
    let store = &app.inner.host.services.store;
    let mut saved = store.credential_quota_snapshot(id).await?;
    let mut sources = Vec::new();
    for capability in capabilities {
        let previous = saved
            .sources
            .iter()
            .position(|state| state.capability.id == capability.id);
        let mut state = previous
            .map(|index| saved.sources.remove(index))
            .unwrap_or_else(|| QuotaSourceState {
                capability: capability.clone(),
                attempted_at_ms: None,
                observed_at_ms: None,
                error: None,
                reset_credits: None,
            });
        state.capability = capability;
        state.observed_at_ms = saved
            .entries
            .iter()
            .filter(|entry| entry.source_id == state.capability.id)
            .map(|entry| entry.observed_at_ms)
            .chain(state.observed_at_ms)
            .max();
        sources.push(state);
    }
    saved.entries.retain(|entry| {
        sources
            .iter()
            .any(|source| source.capability.id == entry.source_id)
    });
    Ok(QuotaSnapshot {
        sources,
        entries: saved.entries,
    })
}
