//! On-demand "pull models from the upstream" for a provider: walk enabled
//! credentials, ensure each secret is fresh, send a `list_models` request
//! through the channel (same proxy + TLS identity its traffic uses), and parse
//! the upstream's native model list into `(id, display_name)` rows.
//! Admin-triggered, infrequent — mirrors [`super::usage`].

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use serde_json::Value;

mod parse;

pub use parse::UpstreamModel;
use parse::parse_models;

use crate::app::AppState;
use crate::channel::{Channel, ChannelError, Disposition, PrepareCtx};
use crate::health::CredAdmit;
use crate::health::config::breaker_config;
use crate::http::client::UpstreamClient;
use crate::pipeline::context::Candidate;
use crate::pipeline::health_hooks;
use crate::protocol::{Operation, OperationKey, Provider};
use crate::util::time::unix_now;

/// Why a model pull could not produce a list.
#[derive(Debug, thiserror::Error)]
pub enum ModelsError {
    #[error("provider not found")]
    ProviderNotFound,
    #[error("provider has no enabled credential")]
    NoCredential,
    #[error("provider has no available credential")]
    NoAvailableCredential,
    #[error("unknown channel: {0}")]
    UnknownChannel(String),
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error("decrypt secret: {0}")]
    Decrypt(String),
    #[error("upstream model request failed: {0}")]
    Upstream(String),
    #[error("upstream returned HTTP {0}")]
    Status(u16),
    #[error("{0}")]
    Internal(String),
}

/// Fetch the upstream model list for one provider.
pub async fn fetch_models(
    state: &AppState,
    provider_id: i64,
) -> Result<Vec<UpstreamModel>, ModelsError> {
    let provider = crate::store::persistence::PersistenceBackend::get_provider(
        state.persistence.as_ref(),
        provider_id,
    )
    .await
    .map_err(|e| ModelsError::Internal(e.to_string()))?
    .ok_or(ModelsError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| ModelsError::UnknownChannel(provider.channel.clone()))?;
    let family = channel.provider_family();

    // Channels with a bundled static catalogue (no upstream model-list endpoint,
    // e.g. vertexexpress) short-circuit — no credential / upstream call needed.
    if let Some(body) = channel.bundled_models() {
        return Ok(parse_models(family, &body));
    }

    // Walk every enabled credential serially. Account-specific catalogues may
    // differ, so successful results are unioned instead of returning the first.
    let credentials = crate::store::persistence::PersistenceBackend::list_credentials(
        state.persistence.as_ref(),
        provider_id,
    )
    .await
    .map_err(|e| ModelsError::Internal(e.to_string()))?
    .into_iter()
    .filter(|c| c.enabled)
    .collect::<Vec<_>>();
    if credentials.is_empty() {
        return Err(ModelsError::NoCredential);
    }

    let provider = Arc::new(provider);
    let cfg = breaker_config(&provider.settings_json);
    let now = unix_now();
    let mut last_err = None;
    let mut admitted = false;
    let mut succeeded = false;
    let mut models = Vec::new();
    let mut model_indexes = HashMap::new();
    for credential in credentials {
        if state.health.admit_credential(credential.id, &cfg, now) == CredAdmit::No {
            last_err.get_or_insert(ModelsError::NoAvailableCredential);
            continue;
        }
        admitted = true;
        let credential = Arc::new(credential);
        let cand = Candidate {
            provider: Arc::clone(&provider),
            credential: Arc::clone(&credential),
            upstream_model_id: String::new(),
            member_id: None,
        };
        match fetch_models_for_credential(state, &channel, family, &cand).await {
            CredentialPull::Success(pulled) => {
                succeeded = true;
                merge_models(&mut models, &mut model_indexes, pulled);
            }
            CredentialPull::Next(err) => last_err = Some(err),
        }
    }

    if succeeded {
        return Ok(models);
    }

    Err(last_err.unwrap_or(if admitted {
        ModelsError::Status(StatusCode::SERVICE_UNAVAILABLE.as_u16())
    } else {
        ModelsError::NoAvailableCredential
    }))
}

enum CredentialPull {
    Success(Vec<UpstreamModel>),
    Next(ModelsError),
}

fn merge_models(
    models: &mut Vec<UpstreamModel>,
    indexes: &mut HashMap<String, usize>,
    pulled: Vec<UpstreamModel>,
) {
    for model in pulled {
        if let Some(index) = indexes.get(&model.id).copied() {
            if models[index].display_name.is_none() && model.display_name.is_some() {
                models[index].display_name = model.display_name;
            }
            continue;
        }
        indexes.insert(model.id.clone(), models.len());
        models.push(model);
    }
}

async fn fetch_models_for_credential(
    state: &AppState,
    channel: &Arc<dyn Channel>,
    family: Provider,
    cand: &Candidate,
) -> CredentialPull {
    let opened = match state.cipher.open(&cand.credential.secret_json) {
        Ok(v) => v,
        Err(e) => return CredentialPull::Next(ModelsError::Decrypt(e.to_string())),
    };
    let mut secret = match state
        .ensure_fresh_credential(channel, &cand.credential, &cand.provider, opened, false)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_credential_attempt(state, cand, &Disposition::AuthDead);
            return CredentialPull::Next(ModelsError::Channel(e));
        }
    };
    if let Some(body) = channel.credential_models(&secret) {
        record_credential_attempt(state, cand, &Disposition::Success);
        return CredentialPull::Success(parse_models(family, &body));
    }
    let client =
        match super::usage::resolve_client(state, channel, &cand.credential, &cand.provider) {
            Ok(c) => c,
            Err(e) => return CredentialPull::Next(ModelsError::Upstream(e.to_string())),
        };

    match fetch_models_with(
        channel,
        family,
        &secret,
        &cand.provider.settings_json,
        &client,
    )
    .await
    {
        Ok(ModelPullResult::Success(models)) => {
            record_credential_attempt(state, cand, &Disposition::Success);
            CredentialPull::Success(models)
        }
        Ok(ModelPullResult::Failure {
            status,
            disposition: Disposition::AuthDead,
        }) => {
            match state
                .ensure_fresh_credential(
                    channel,
                    &cand.credential,
                    &cand.provider,
                    secret.clone(),
                    true,
                )
                .await
            {
                Ok(fresh) => {
                    secret = fresh;
                    finish_http_result(
                        state,
                        cand,
                        fetch_models_with(
                            channel,
                            family,
                            &secret,
                            &cand.provider.settings_json,
                            &client,
                        )
                        .await,
                    )
                }
                Err(e) => {
                    tracing::warn!(
                        credential_id = cand.credential.id,
                        error = %e,
                        "forced refresh after model-list AuthDead failed; skipping credential"
                    );
                    record_credential_attempt(state, cand, &Disposition::AuthDead);
                    CredentialPull::Next(ModelsError::Status(status.as_u16()))
                }
            }
        }
        result => finish_http_result(state, cand, result),
    }
}

fn finish_http_result(
    state: &AppState,
    cand: &Candidate,
    result: Result<ModelPullResult, ModelsError>,
) -> CredentialPull {
    match result {
        Ok(ModelPullResult::Success(models)) => {
            record_credential_attempt(state, cand, &Disposition::Success);
            CredentialPull::Success(models)
        }
        Ok(ModelPullResult::Failure {
            status,
            disposition,
        }) => {
            record_credential_attempt(state, cand, &disposition);
            let err = ModelsError::Status(status.as_u16());
            CredentialPull::Next(err)
        }
        Err(ModelsError::Channel(ChannelError::InvalidCredential(e))) => {
            record_credential_attempt(state, cand, &Disposition::AuthDead);
            CredentialPull::Next(ModelsError::Channel(ChannelError::InvalidCredential(e)))
        }
        Err(err @ (ModelsError::Upstream(_) | ModelsError::Decrypt(_))) => {
            record_credential_attempt(state, cand, &Disposition::Transient);
            CredentialPull::Next(err)
        }
        Err(err) => CredentialPull::Next(err),
    }
}

fn record_credential_attempt(state: &AppState, cand: &Candidate, disposition: &Disposition) {
    health_hooks::record_credential_attempt(state, &cand.provider, &cand.credential, disposition);
}

enum ModelPullResult {
    Success(Vec<UpstreamModel>),
    Failure {
        status: StatusCode,
        disposition: Disposition,
    },
}

/// Transport-injectable core: build the `list_models` request, send it, parse.
/// Transient throttling (`429`) / server errors are retried with backoff — the
/// gemini CLI does the same for its quota-derived model list, since Google
/// frequently 429s the `retrieveUserQuota` endpoint a single call rides.
async fn fetch_models_with(
    channel: &Arc<dyn Channel>,
    family: Provider,
    secret: &Value,
    settings: &Value,
    client: &Arc<dyn UpstreamClient>,
) -> Result<ModelPullResult, ModelsError> {
    let op = OperationKey::provider(Operation::ListModels, family);
    let target = crate::protocol::request_target(op, "", false);
    let headers = http::HeaderMap::new();

    let mut attempt = 0;
    loop {
        attempt += 1;
        // Re-prepare each attempt (`into_http` consumes the request); cheap.
        let prepared = channel.prepare(PrepareCtx {
            secret,
            provider_settings: settings,
            op,
            stream: false,
            upstream_model_id: "",
            method: http::Method::GET,
            path: &target.path,
            query: target.query.as_deref(),
            headers: &headers,
            body: Bytes::new(),
        })?;

        let resp = client
            .send(prepared.into_http())
            .await
            .map_err(|e| ModelsError::Upstream(e.to_string()))?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.into_body();

        if status.is_success() {
            // Channel response 整形 (same hook proxy traffic uses): lets a channel
            // reshape a non-standard model-list body (e.g. codex `{models}`→`{data}`,
            // vertex `publisherModels`→`models`) into its family's canonical shape
            // before `parse_models` reads it.
            let op = OperationKey::provider(Operation::ListModels, family);
            let body = channel.shape_response(
                body,
                &crate::channel::ShapeCtx {
                    op,
                    stream: false,
                    status,
                    settings,
                },
            );
            return Ok(ModelPullResult::Success(parse_models(family, &body)));
        }

        let disposition = channel.classify(status, &headers, &body);

        // Retry transient throttling (429) / server errors a few times before
        // surfacing — mirrors the gemini CLI's `retrieveUserQuota` retry.
        if (status.as_u16() == 429 || status.is_server_error()) && attempt < PULL_MAX_ATTEMPTS {
            pull_backoff(attempt).await;
            continue;
        }
        return Ok(ModelPullResult::Failure {
            status,
            disposition,
        });
    }
}

/// Max model-pull attempts (1 try + 2 retries) for transient 429/5xx.
const PULL_MAX_ATTEMPTS: u32 = 3;

/// Backoff between pull retries. The pull is admin-triggered + infrequent, so a
/// slightly longer delay than the CLI's 100ms is fine and gentler on the quota
/// endpoint. No-op on wasm (the pull is native-only; this only keeps it edge-safe).
#[cfg(not(target_arch = "wasm32"))]
async fn pull_backoff(attempt: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(400 * attempt as u64)).await;
}
#[cfg(target_arch = "wasm32")]
async fn pull_backoff(_attempt: u32) {}

#[cfg(test)]
mod tests;
