use gproxy_admin::AdminError;
use gproxy_admin::dto::{
    ConnectivityScopeDto, ConnectivityTestRequest, QuotaResetOutcomeDto, QuotaResetResponse,
};

use crate::AppHandle;

pub(super) async fn run(
    app: &AppHandle,
    credential_id: i64,
) -> Result<QuotaResetResponse, AdminError> {
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
        .quota_reset(
            &provider.channel.clone(),
            &provider,
            gproxy_core::CredentialId(credential_id),
        )
        .await
        .map_err(reset_error)?;
    Ok(QuotaResetResponse {
        outcome: match result.outcome {
            gproxy_core::QuotaResetOutcome::Reset => QuotaResetOutcomeDto::Reset,
            gproxy_core::QuotaResetOutcome::NothingToReset => QuotaResetOutcomeDto::NothingToReset,
            gproxy_core::QuotaResetOutcome::NoCredit => QuotaResetOutcomeDto::NoCredit,
            gproxy_core::QuotaResetOutcome::AlreadyRedeemed => {
                QuotaResetOutcomeDto::AlreadyRedeemed
            }
        },
        windows_reset: result.windows_reset,
    })
}

fn reset_error(error: gproxy_core::CoreError) -> AdminError {
    match error {
        gproxy_core::CoreError::Unsupported => {
            AdminError::BadRequest("channel exposes no quota reset endpoint".into())
        }
        error => AdminError::BadRequest(format!("quota reset failed: {error}")),
    }
}
