use crate::AppHandle;
use gproxy_admin::{AdminError, dto::QuotaCapabilitiesDto};

pub(super) async fn read(
    app: &AppHandle,
    id: i64,
) -> Result<Option<QuotaCapabilitiesDto>, AdminError> {
    let services = &app.inner.host.services;
    let Some(credential) = services.store.credential(id).await? else {
        return Ok(None);
    };
    let (_, _, sources) = super::quota_snapshot::sources(app, id).await?;
    let secret = services
        .cipher
        .open(&credential.envelope)
        .map_err(|error| AdminError::Internal(error.to_string()))?;
    let reset = app
        .inner
        .core
        .quota_capabilities(&credential.channel, &secret)
        .map_err(|error| AdminError::BadRequest(error.to_string()))?
        .is_some_and(|capability| capability.reset);
    Ok(Some(QuotaCapabilitiesDto {
        probe: sources.iter().any(|source| {
            source.mode == gproxy_channel_api::QuotaQueryMode::Probe
                && source.support == gproxy_channel_api::QuotaSupport::Ready
        }),
        reset,
    }))
}
