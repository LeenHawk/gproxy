use gproxy_admin::AdminError;
use gproxy_admin::dto::{
    ConnectivityScopeDto, ConnectivityTestRequest, QuotaProbeResponse, QuotaProbeWindowDto,
    QuotaResetCreditsDto,
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
    let result = app
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
        windows: result
            .observations
            .into_iter()
            .map(|observation| QuotaProbeWindowDto {
                window_key: observation.window_key,
                used_percent: observation.used_percent.map(|value| value.to_string()),
                period_end: observation.period_end,
            })
            .collect(),
        reset_credits: result.reset_credits.map(|credits| QuotaResetCreditsDto {
            available_count: credits.available_count,
            expires_at: credits.expires_at,
        }),
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
