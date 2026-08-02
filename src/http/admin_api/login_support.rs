//! Small shared helpers for admin login-flow persistence and wire auditing.

use crate::api::error::ApiError;
use crate::app::AppState;
use crate::credentials::audit::UpstreamAuditSequence;
use crate::store::persistence::records::CredentialInput;

pub(super) fn audit_sequence<'a>(
    state: &'a AppState,
    channel: &str,
    provider_id: Option<i64>,
    request_id: Option<&str>,
) -> UpstreamAuditSequence<'a> {
    let settings = state.cp().log_settings.clone();
    UpstreamAuditSequence::for_login(
        settings.enable_upstream_log,
        state.persistence.as_ref(),
        channel,
        provider_id,
        request_id,
        settings.enable_upstream_log_body,
        settings.disable_log_redaction,
    )
}

pub(super) fn provider_settings(
    state: &AppState,
    provider_id: Option<i64>,
    channel: &str,
) -> Result<serde_json::Value, ApiError> {
    let Some(provider_id) = provider_id else {
        return Ok(serde_json::Value::Null);
    };
    let snapshot = state.cp();
    let provider = snapshot
        .providers_by_id
        .get(&provider_id)
        .ok_or_else(|| ApiError::NotFound("provider not found".into()))?;
    if provider.channel != channel {
        return Err(ApiError::BadRequest("provider channel mismatch".into()));
    }
    Ok(provider.settings_json.clone())
}

/// Seal-then-persist a login secret and invalidate the control-plane cache.
pub(super) async fn seal_create(
    state: &AppState,
    provider_id: i64,
    channel: &str,
    name: Option<String>,
    sealed: serde_json::Value,
) -> Result<crate::api::credentials::CredentialView, ApiError> {
    let cred = state
        .persistence
        .upsert_credential(CredentialInput {
            id: None,
            provider_id,
            name,
            kind: "oauth".into(),
            secret_json: sealed,
            weight: 100,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .map_err(|error| {
            let safe = crate::http::telemetry::redact_url_query(&error.to_string()).into_owned();
            tracing::warn!(
                channel,
                provider_id,
                operation = "create_credential",
                status = 0u16,
                error_kind = "persistence",
                error = %safe,
                "login flow credential creation failed"
            );
            ApiError::Internal(error.to_string())
        })?;
    crate::admin::invalidate(state).await;
    Ok(crate::api::credentials::CredentialView::from(cred))
}
