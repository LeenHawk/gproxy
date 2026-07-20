//! On-demand per-credential upstream usage fetch (§17). Resolves the
//! credential's pooled client (the SAME proxy + TLS identity its traffic uses),
//! ensures the OAuth secret is fresh, then runs the channel's
//! `prepare_usage_request` → `parse_usage`. Admin-triggered and infrequent — not
//! on the request hot path; the orchestration mirrors [`super::refresh`].

use std::sync::Arc;

use crate::app::AppState;
use crate::channel::{
    Channel, ChannelError, Disposition, RateLimitResetCreditConsumeResponse, UsageSnapshot,
};
use crate::http::client::UpstreamClient;
use crate::store::persistence::records::{Credential, Provider};

mod attempt;
#[cfg(test)]
mod tests;

use attempt::{consume_reset_credit_with, fetch_with, finish};

/// Why a usage fetch could not produce a snapshot.
#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("credential not found")]
    CredentialNotFound,
    #[error("provider not found")]
    ProviderNotFound,
    #[error("unknown channel: {0}")]
    UnknownChannel(String),
    #[error("channel exposes no usage endpoint")]
    Unsupported,
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error("decrypt secret: {0}")]
    Decrypt(String),
    #[error("upstream usage request failed: {0}")]
    Upstream(String),
    #[error("usage endpoint returned HTTP {0}")]
    Status(u16),
}

/// Fetch the live usage snapshot for one credential id.
pub async fn fetch_usage(
    state: &AppState,
    credential_id: i64,
) -> Result<UsageSnapshot, UsageError> {
    let credential = state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(|e| UsageError::Upstream(e.to_string()))?
        .ok_or(UsageError::CredentialNotFound)?;
    let provider = state
        .persistence
        .get_provider(credential.provider_id)
        .await
        .map_err(|e| UsageError::Upstream(e.to_string()))?
        .ok_or(UsageError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| UsageError::UnknownChannel(provider.channel.clone()))?;

    // Decrypt → ensure a fresh access token (the usage endpoints are bearer-auth,
    // so a stale token would just 401). `ensure_fresh` re-seals + persists any
    // rotation, exactly as the traffic path does.
    let opened = match state.cipher.open(&credential.secret_json) {
        Ok(opened) => opened,
        Err(e) => {
            record(state, &provider, &credential, &Disposition::AuthDead);
            return Err(UsageError::Decrypt(e.to_string()));
        }
    };
    let mut secret = match state
        .ensure_fresh_credential(&channel, &credential, &provider, opened, false)
        .await
    {
        Ok(secret) => secret,
        Err(e) => {
            record(state, &provider, &credential, &Disposition::AuthDead);
            return Err(UsageError::Channel(e));
        }
    };

    // None → the channel has no usage endpoint (api-key / vertex channels).
    let client = match resolve_client(state, &channel, &credential, &provider) {
        Ok(client) => client,
        Err(e) => {
            record(state, &provider, &credential, &Disposition::Transient);
            return Err(e);
        }
    };
    let mut result = fetch_with(&channel, &secret, &provider.settings_json, &client).await;
    if result
        .as_ref()
        .is_err_and(|failure| failure.disposition == Some(Disposition::AuthDead))
    {
        match state
            .ensure_fresh_credential(&channel, &credential, &provider, secret.clone(), true)
            .await
        {
            Ok(fresh) => {
                secret = fresh;
                result = fetch_with(&channel, &secret, &provider.settings_json, &client).await;
            }
            Err(e) => {
                record(state, &provider, &credential, &Disposition::AuthDead);
                return Err(UsageError::Channel(e));
            }
        }
    }
    finish(state, &provider, &credential, result)
}

/// Consume one earned upstream rate-limit reset credit for a credential.
pub async fn consume_rate_limit_reset_credit(
    state: &AppState,
    credential_id: i64,
    idempotency_key: &str,
) -> Result<RateLimitResetCreditConsumeResponse, UsageError> {
    if idempotency_key.trim().is_empty() {
        return Err(UsageError::Channel(ChannelError::Build(
            "idempotency_key must not be empty".into(),
        )));
    }

    let credential = state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(|e| UsageError::Upstream(e.to_string()))?
        .ok_or(UsageError::CredentialNotFound)?;
    let provider = state
        .persistence
        .get_provider(credential.provider_id)
        .await
        .map_err(|e| UsageError::Upstream(e.to_string()))?
        .ok_or(UsageError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| UsageError::UnknownChannel(provider.channel.clone()))?;

    let opened = match state.cipher.open(&credential.secret_json) {
        Ok(opened) => opened,
        Err(e) => {
            record(state, &provider, &credential, &Disposition::AuthDead);
            return Err(UsageError::Decrypt(e.to_string()));
        }
    };
    let mut secret = match state
        .ensure_fresh_credential(&channel, &credential, &provider, opened, false)
        .await
    {
        Ok(secret) => secret,
        Err(e) => {
            record(state, &provider, &credential, &Disposition::AuthDead);
            return Err(UsageError::Channel(e));
        }
    };

    let client = match resolve_client(state, &channel, &credential, &provider) {
        Ok(client) => client,
        Err(e) => {
            record(state, &provider, &credential, &Disposition::Transient);
            return Err(e);
        }
    };
    let mut result = consume_reset_credit_with(
        &channel,
        &secret,
        &provider.settings_json,
        &client,
        idempotency_key,
    )
    .await;
    if result
        .as_ref()
        .is_err_and(|failure| failure.disposition == Some(Disposition::AuthDead))
    {
        match state
            .ensure_fresh_credential(&channel, &credential, &provider, secret.clone(), true)
            .await
        {
            Ok(fresh) => {
                secret = fresh;
                result = consume_reset_credit_with(
                    &channel,
                    &secret,
                    &provider.settings_json,
                    &client,
                    idempotency_key,
                )
                .await;
            }
            Err(e) => {
                record(state, &provider, &credential, &Disposition::AuthDead);
                return Err(UsageError::Channel(e));
            }
        }
    }
    finish(state, &provider, &credential, result)
}

fn record(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    disposition: &Disposition,
) {
    crate::pipeline::health_hooks::record_credential_attempt(
        state,
        provider,
        credential,
        disposition,
    );
}

/// Resolve the pooled client for this credential: its effective proxy + TLS
/// fingerprint (DB override) else the channel's built-in emulation, mirroring
/// [`super::refresh`] and `failover::attempt`.
pub(crate) fn resolve_client(
    state: &AppState,
    channel: &Arc<dyn Channel>,
    credential: &Credential,
    provider: &Provider,
) -> Result<Arc<dyn UpstreamClient>, UsageError> {
    state
        .upstream_client_for_credential(channel, credential, provider)
        .map_err(|e| UsageError::Upstream(format!("resolve usage client: {e}")))
}
