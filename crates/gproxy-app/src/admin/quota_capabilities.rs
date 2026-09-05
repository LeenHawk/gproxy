use crate::AppHandle;
use gproxy_admin::{AdminError, dto::QuotaCapabilitiesDto};

pub(super) async fn read(
    app: &AppHandle,
    id: i64,
) -> Result<Option<QuotaCapabilitiesDto>, AdminError> {
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
    app.inner
        .core
        .quota_capabilities(&credential.channel, &secret)
        .map(|value| value.map(Into::into))
        .map_err(|error| AdminError::BadRequest(error.to_string()))
}
