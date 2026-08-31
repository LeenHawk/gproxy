use gproxy_admin::AdminError;
use gproxy_admin::dto::{
    ConnectivityScopeDto, ConnectivityTestRequest, QuotaProbeResponse, QuotaProbeWindowDto,
};

use crate::AppHandle;

pub(super) async fn run(
    app: &AppHandle,
    credential_id: i64,
) -> Result<QuotaProbeResponse, AdminError> {
    let (provider, _) = super::connectivity::target::resolve(
        app,
        &ConnectivityTestRequest {
            scope: ConnectivityScopeDto::Credential,
            provider_id: None,
            credential_id: Some(credential_id),
            proxy_url: None,
        },
    )?;
    let observations = app
        .inner
        .core
        .quota_probe(
            &provider.channel.clone(),
            &provider,
            gproxy_core::CredentialId(credential_id),
        )
        .await
        .map_err(probe_error)?;
    Ok(QuotaProbeResponse {
        windows: observations
            .into_iter()
            .map(|observation| QuotaProbeWindowDto {
                window_key: observation.window_key,
                used_percent: observation.used_percent.map(|value| value.to_string()),
                period_end: observation.period_end,
            })
            .collect(),
    })
}

fn probe_error(error: gproxy_core::CoreError) -> AdminError {
    match error {
        gproxy_core::CoreError::Unsupported => {
            AdminError::BadRequest("channel exposes no usage endpoint".into())
        }
        error => AdminError::BadRequest(format!("quota probe failed: {error}")),
    }
}
