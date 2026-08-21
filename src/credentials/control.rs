//! Generic driver for credential-scoped upstream account/control operations.

use std::sync::Arc;

use crate::app::AppState;
use crate::channel::{
    Channel, ChannelError, CredentialControlOperation, CredentialControlResponse, Disposition,
};
use crate::http::client::UpstreamClient;
use crate::store::persistence::records::{Credential, Provider};

pub struct RawControlResponse {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: bytes::Bytes,
    pub disposition: Disposition,
}

pub struct RawControlStreamResponse {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: crate::http::client::RespStream,
    pub disposition: Disposition,
}

pub async fn execute_raw_streaming(
    state: &AppState,
    credential_id: i64,
    operation: CredentialControlOperation,
) -> Result<RawControlStreamResponse, ControlError> {
    let credential = state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::CredentialNotFound)?;
    let provider = state
        .persistence
        .get_provider(credential.provider_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| ControlError::UnknownChannel(provider.channel.clone()))?;
    let opened = state
        .cipher
        .open(&credential.secret_json)
        .map_err(|error| ControlError::Decrypt(error.to_string()))?;
    let secret = state
        .ensure_fresh_credential(&channel, &credential, &provider, opened, false)
        .await?;
    let request = channel
        .prepare_credential_control_request(&operation, &secret, &provider.settings_json)?
        .ok_or_else(|| ControlError::Unsupported(operation.name()))?;
    let client = state
        .upstream_client_for_credential(&channel, &credential, &provider)
        .map_err(|error| ControlError::Upstream(error.to_string()))?;
    let (status, headers, body) = client
        .send_streaming(request)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?;
    let disposition = channel.classify(status, &headers, &bytes::Bytes::new());
    record(state, &provider, &credential, &disposition);
    Ok(RawControlStreamResponse {
        status,
        headers,
        body,
        disposition,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn open_raw_websocket(
    state: &AppState,
    credential_id: i64,
    operation: CredentialControlOperation,
) -> Result<Box<dyn crate::http::client::ConduitSocket>, ControlError> {
    let credential = state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::CredentialNotFound)?;
    let provider = state
        .persistence
        .get_provider(credential.provider_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| ControlError::UnknownChannel(provider.channel.clone()))?;
    let opened = state
        .cipher
        .open(&credential.secret_json)
        .map_err(|error| ControlError::Decrypt(error.to_string()))?;
    let secret = state
        .ensure_fresh_credential(&channel, &credential, &provider, opened, false)
        .await?;
    let request = channel
        .prepare_credential_control_request(&operation, &secret, &provider.settings_json)?
        .ok_or_else(|| ControlError::Unsupported(operation.name()))?;
    let client = state
        .upstream_client_for_credential(&channel, &credential, &provider)
        .map_err(|error| ControlError::Upstream(error.to_string()))?;
    client
        .open_websocket(request)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("credential not found")]
    CredentialNotFound,
    #[error("provider not found")]
    ProviderNotFound,
    #[error("unknown channel: {0}")]
    UnknownChannel(String),
    #[error("channel does not support credential operation `{0}`")]
    Unsupported(&'static str),
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error("decrypt secret: {0}")]
    Decrypt(String),
    #[error("upstream credential control request failed: {0}")]
    Upstream(String),
    #[error("credential control endpoint returned HTTP {0}")]
    Status(u16),
}

pub async fn execute(
    state: &AppState,
    credential_id: i64,
    operation: CredentialControlOperation,
) -> Result<CredentialControlResponse, ControlError> {
    let credential = state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::CredentialNotFound)?;
    let provider = state
        .persistence
        .get_provider(credential.provider_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| ControlError::UnknownChannel(provider.channel.clone()))?;
    let opened = match state.cipher.open(&credential.secret_json) {
        Ok(opened) => opened,
        Err(error) => {
            record(state, &provider, &credential, &Disposition::AuthDead);
            return Err(ControlError::Decrypt(error.to_string()));
        }
    };
    let mut secret = match state
        .ensure_fresh_credential(&channel, &credential, &provider, opened, false)
        .await
    {
        Ok(secret) => secret,
        Err(error) => {
            record(
                state,
                &provider,
                &credential,
                &refresh_failure_disposition(&error),
            );
            return Err(ControlError::Channel(error));
        }
    };
    let client = match state.upstream_client_for_credential(&channel, &credential, &provider) {
        Ok(client) => client,
        Err(error) => {
            record(state, &provider, &credential, &Disposition::Transient);
            return Err(ControlError::Upstream(format!(
                "resolve credential control client: {error}"
            )));
        }
    };
    let audit = crate::credentials::usage::audit_sequence(state, &credential, operation.name());
    let client = audit.wrap_client(client);

    let mut result = send(&channel, &secret, &provider, &operation, &client).await;
    if result
        .as_ref()
        .is_err_and(|failure| failure.disposition == Some(Disposition::AuthDead))
    {
        match state
            .ensure_fresh_credential(&channel, &credential, &provider, secret, true)
            .await
        {
            Ok(fresh) => {
                secret = fresh;
                result = send(&channel, &secret, &provider, &operation, &client).await;
            }
            Err(error) => {
                let message = error.to_string();
                audit.persist(Some(&message)).await;
                record(
                    state,
                    &provider,
                    &credential,
                    &refresh_failure_disposition(&error),
                );
                return Err(ControlError::Channel(error));
            }
        }
    }
    let error = result
        .as_ref()
        .err()
        .map(|failure| failure.error.to_string());
    audit.persist(error.as_deref()).await;
    finish(state, &provider, &credential, result)
}

/// Execute one control request against a concrete credential while preserving
/// the upstream wire response for the public Codex PAT service surface.
pub async fn execute_raw(
    state: &AppState,
    credential_id: i64,
    operation: CredentialControlOperation,
) -> Result<RawControlResponse, ControlError> {
    let credential = state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::CredentialNotFound)?;
    let provider = state
        .persistence
        .get_provider(credential.provider_id)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?
        .ok_or(ControlError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| ControlError::UnknownChannel(provider.channel.clone()))?;
    let opened = state
        .cipher
        .open(&credential.secret_json)
        .map_err(|error| ControlError::Decrypt(error.to_string()))?;
    let mut secret = state
        .ensure_fresh_credential(&channel, &credential, &provider, opened, false)
        .await?;
    let client = state
        .upstream_client_for_credential(&channel, &credential, &provider)
        .map_err(|error| ControlError::Upstream(error.to_string()))?;
    let audit = crate::credentials::usage::audit_sequence(state, &credential, operation.name());
    let client = audit.wrap_client(client);

    let mut result = send_raw(&channel, &secret, &provider, &operation, &client).await;
    if result
        .as_ref()
        .is_ok_and(|response| response.disposition == Disposition::AuthDead)
    {
        secret = state
            .ensure_fresh_credential(&channel, &credential, &provider, secret, true)
            .await?;
        result = send_raw(&channel, &secret, &provider, &operation, &client).await;
    }
    match result {
        Ok(response) => {
            audit.persist(None).await;
            record(state, &provider, &credential, &response.disposition);
            Ok(response)
        }
        Err(error) => {
            audit.persist(Some(&error.to_string())).await;
            record(state, &provider, &credential, &Disposition::Transient);
            Err(error)
        }
    }
}

async fn send_raw(
    channel: &Arc<dyn Channel>,
    secret: &serde_json::Value,
    provider: &Provider,
    operation: &CredentialControlOperation,
    client: &Arc<dyn UpstreamClient>,
) -> Result<RawControlResponse, ControlError> {
    let request = channel
        .prepare_credential_control_request(operation, secret, &provider.settings_json)?
        .ok_or_else(|| ControlError::Unsupported(operation.name()))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| ControlError::Upstream(error.to_string()))?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body();
    let disposition = channel.classify(status, &headers, &body);
    Ok(RawControlResponse {
        status,
        headers,
        body,
        disposition,
    })
}

struct Failure {
    error: ControlError,
    disposition: Option<Disposition>,
}

async fn send(
    channel: &Arc<dyn Channel>,
    secret: &serde_json::Value,
    provider: &Provider,
    operation: &CredentialControlOperation,
    client: &Arc<dyn UpstreamClient>,
) -> Result<CredentialControlResponse, Failure> {
    let request = channel
        .prepare_credential_control_request(operation, secret, &provider.settings_json)
        .map_err(|error| Failure {
            disposition: matches!(error, ChannelError::InvalidCredential(_))
                .then_some(Disposition::AuthDead),
            error: ControlError::Channel(error),
        })?
        .ok_or_else(|| Failure {
            disposition: None,
            error: ControlError::Unsupported(operation.name()),
        })?;
    let response = client.send(request).await.map_err(|error| Failure {
        disposition: Some(Disposition::Transient),
        error: ControlError::Upstream(error.to_string()),
    })?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body();
    let disposition = channel.classify(status, &headers, &body);
    if disposition != Disposition::Success {
        return Err(Failure {
            disposition: Some(disposition),
            error: ControlError::Status(status.as_u16()),
        });
    }
    channel
        .parse_credential_control_response(operation, status, &headers, &body)
        .ok_or(Failure {
            disposition: Some(Disposition::Transient),
            error: ControlError::Status(status.as_u16()),
        })
}

fn finish(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    result: Result<CredentialControlResponse, Failure>,
) -> Result<CredentialControlResponse, ControlError> {
    match result {
        Ok(value) => {
            record(state, provider, credential, &Disposition::Success);
            Ok(value)
        }
        Err(failure) => {
            if let Some(disposition) = &failure.disposition {
                record(state, provider, credential, disposition);
            }
            Err(failure.error)
        }
    }
}

fn refresh_failure_disposition(error: &ChannelError) -> Disposition {
    if matches!(error, ChannelError::Transient(_)) {
        Disposition::Transient
    } else {
        Disposition::AuthDead
    }
}

fn record(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    disposition: &Disposition,
) {
    crate::pipeline::health_hooks::record_credential_attempt(
        state,
        None,
        provider,
        credential,
        disposition,
    );
}
