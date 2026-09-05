use crate::AppHandle;
use gproxy_admin::{AdminError, dto::QuotaCapabilitiesDto};

pub(super) async fn read(
    app: &AppHandle,
    id: i64,
) -> Result<Option<QuotaCapabilitiesDto>, AdminError> {
    let snapshot = app.inner.host.services.control.current();
    let credential = snapshot
        .credentials
        .iter()
        .find(|value| value.id == id)
        .ok_or(AdminError::NotFound)?;
    let provider = snapshot
        .providers
        .iter()
        .find(|value| value.id == credential.provider_id)
        .ok_or(AdminError::NotFound)?;
    app.inner
        .core
        .quota_capabilities(&provider.channel, gproxy_core::CredentialId(id))
        .await
        .map(|value| value.map(Into::into))
        .map_err(|error| AdminError::BadRequest(error.to_string()))
}
