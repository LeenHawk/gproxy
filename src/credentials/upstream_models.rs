//! On-demand "pull models from the upstream" for a provider: walk enabled
//! credentials, ensure each secret is fresh, send a `list_models` request
//! through the channel (same proxy + TLS identity its traffic uses), and parse
//! the upstream's native model list into `(id, display_name)` rows.
//! Admin-triggered, infrequent — mirrors [`super::usage`].

use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use serde::Serialize;
use serde_json::Value;

use crate::app::AppState;
use crate::channel::{Channel, ChannelError, Disposition, PrepareCtx};
use crate::health::CredAdmit;
use crate::health::config::breaker_config;
use crate::http::client::UpstreamClient;
use crate::pipeline::context::Candidate;
use crate::pipeline::health_hooks;
use crate::protocol::{Operation, OperationKey, Provider};
use crate::util::time::unix_now;

/// One model offered by the upstream.
#[derive(Debug, Clone, Serialize)]
pub struct UpstreamModel {
    pub id: String,
    pub display_name: Option<String>,
}

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
    let provider = state
        .persistence
        .get_provider(provider_id)
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

    // Walk enabled credentials — the pull authenticates to the upstream, and a
    // stale/dead first credential must not prevent later healthy credentials
    // from serving the admin request.
    let credentials = state
        .persistence
        .list_credentials(provider_id)
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
            breaker_cfg: cfg.clone(),
        };
        match fetch_models_for_credential(state, &channel, family, &cand).await {
            CredentialPull::Success(models) => return Ok(models),
            CredentialPull::Next(err) => last_err = Some(err),
            CredentialPull::Stop(err) => return Err(err),
        }
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
    Stop(ModelsError),
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
        .refresh
        .ensure_fresh(
            state,
            channel,
            &cand.credential,
            &cand.provider,
            opened,
            false,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            health_hooks::record_attempt(state, cand, &Disposition::AuthDead, None);
            return CredentialPull::Next(ModelsError::Channel(e));
        }
    };
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
            health_hooks::record_attempt(state, cand, &Disposition::Success, None);
            CredentialPull::Success(models)
        }
        Ok(ModelPullResult::Failure {
            status,
            disposition: Disposition::AuthDead,
        }) => {
            match state
                .refresh
                .ensure_fresh(
                    state,
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
                    health_hooks::record_attempt(state, cand, &Disposition::AuthDead, None);
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
            health_hooks::record_attempt(state, cand, &Disposition::Success, None);
            CredentialPull::Success(models)
        }
        Ok(ModelPullResult::Failure {
            status,
            disposition,
        }) => {
            health_hooks::record_attempt(state, cand, &disposition, None);
            let err = ModelsError::Status(status.as_u16());
            if disposition.should_failover() {
                CredentialPull::Next(err)
            } else {
                CredentialPull::Stop(err)
            }
        }
        Err(ModelsError::Channel(ChannelError::InvalidCredential(e))) => {
            health_hooks::record_attempt(state, cand, &Disposition::AuthDead, None);
            CredentialPull::Next(ModelsError::Channel(ChannelError::InvalidCredential(e)))
        }
        Err(err @ (ModelsError::Upstream(_) | ModelsError::Decrypt(_))) => {
            health_hooks::record_attempt(state, cand, &Disposition::Transient, None);
            CredentialPull::Next(err)
        }
        Err(err) => CredentialPull::Next(err),
    }
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
    let target = crate::protocol::request_target(
        OperationKey::provider(Operation::ListModels, family),
        "",
        false,
    );
    let headers = http::HeaderMap::new();

    let mut attempt = 0;
    loop {
        attempt += 1;
        // Re-prepare each attempt (`into_http` consumes the request); cheap.
        let prepared = channel.prepare(PrepareCtx {
            secret,
            provider_settings: settings,
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
                    enable_magic_cache: false,
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

/// Parse an upstream native model-list response into `(id, display_name)` rows.
/// openai/claude → `data[]` (`id`); gemini → `models[]` (`name`, `models/` stripped).
fn parse_models(family: Provider, body: &[u8]) -> Vec<UpstreamModel> {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let key = match family {
        Provider::Gemini => "models",
        _ => "data",
    };
    let Some(arr) = v.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            let id = match family {
                Provider::Gemini => m
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|s| s.strip_prefix("models/").unwrap_or(s).to_owned()),
                _ => m.get("id").and_then(Value::as_str).map(str::to_owned),
            }?;
            let display_name = match family {
                Provider::Gemini => m.get("displayName"),
                Provider::Claude => m.get("display_name"),
                Provider::OpenAi => None,
            }
            .and_then(Value::as_str)
            .map(str::to_owned);
            Some(UpstreamModel { id, display_name })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_openai_and_gemini() {
        let oa = br#"{"object":"list","data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#;
        let ids: Vec<_> = parse_models(Provider::OpenAi, oa)
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, ["gpt-4o", "gpt-4o-mini"]);

        let gm = br#"{"models":[{"name":"models/gemini-1.5-pro","displayName":"Gemini 1.5 Pro"}]}"#;
        let g = parse_models(Provider::Gemini, gm);
        assert_eq!(g[0].id, "gemini-1.5-pro");
        assert_eq!(g[0].display_name.as_deref(), Some("Gemini 1.5 Pro"));
    }

    #[cfg(all(
        not(target_arch = "wasm32"),
        feature = "cache-memory",
        feature = "persist-file",
        feature = "channel-openai"
    ))]
    mod fetch {
        use super::*;

        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        use http::header::AUTHORIZATION;

        use crate::app::AppState;
        use crate::app::snapshot::ControlPlaneSnapshot;
        use crate::config::{
            CacheConfig, DEFAULT_MAX_ATTEMPTS, DEFAULT_MAX_IN_FLIGHT, PersistenceConfig,
            RuntimeConfig, UpstreamConfig,
        };
        use crate::http::client::ClientError;

        const BUNDLE: &str = r#"{
          "schema_version": 1,
          "providers": [
            { "id": 1, "name": "oai", "channel": "openai", "label": null,
              "settings_json": { "base_url": "http://fake.local" },
              "credential_strategy": "round_robin", "proxy_url": null,
              "tls_fingerprint": null, "enabled": true }
          ],
          "credentials": [
            { "id": 1, "provider_id": 1, "label": "bad",
              "secret_json": { "api_key": "bad-key" }, "enabled": true },
            { "id": 2, "provider_id": 1, "label": "good",
              "secret_json": { "api_key": "good-key" }, "enabled": true }
          ]
        }"#;

        struct Seen {
            uri: String,
            authorization: Option<String>,
        }

        struct SequencedUpstream {
            statuses: Vec<StatusCode>,
            seen: Mutex<Vec<Seen>>,
            calls: AtomicUsize,
        }

        #[async_trait::async_trait]
        impl UpstreamClient for SequencedUpstream {
            async fn send(
                &self,
                req: http::Request<Bytes>,
            ) -> Result<http::Response<Bytes>, ClientError> {
                let authorization = req
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                self.seen.lock().unwrap().push(Seen {
                    uri: req.uri().to_string(),
                    authorization,
                });
                let i = self.calls.fetch_add(1, Ordering::SeqCst);
                let status = self
                    .statuses
                    .get(i)
                    .or_else(|| self.statuses.last())
                    .copied()
                    .unwrap_or(StatusCode::OK);
                let body = if status.is_success() {
                    Bytes::from_static(br#"{"object":"list","data":[{"id":"gpt-good"}]}"#)
                } else {
                    Bytes::from_static(br#"{"error":"bad credential"}"#)
                };
                Ok(http::Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(body)
                    .expect("response"))
            }
        }

        impl SequencedUpstream {
            fn new(statuses: Vec<StatusCode>) -> Self {
                Self {
                    statuses,
                    seen: Mutex::new(Vec::new()),
                    calls: AtomicUsize::new(0),
                }
            }
        }

        async fn state_with(upstream: Arc<SequencedUpstream>) -> (AppState, tempfile::TempDir) {
            let dir = tempfile::tempdir().expect("tempdir");
            let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
                crate::store::persistence::FilePersistence::open(dir.path().to_path_buf())
                    .await
                    .expect("file persistence"),
            );
            crate::app::import::import_bundle(
                persistence.as_ref(),
                &crate::crypto::NoopCipher,
                BUNDLE,
            )
            .await
            .expect("import");
            let snapshot = ControlPlaneSnapshot::build(persistence.as_ref(), 1)
                .await
                .expect("snapshot");
            let config = Arc::new(RuntimeConfig {
                host: "127.0.0.1".into(),
                port: 0,
                cache: CacheConfig::Memory,
                persistence: PersistenceConfig::File {
                    data_dir: dir.path().to_path_buf(),
                },
                upstream: UpstreamConfig::from_proxy_url(None),
                instance_id: 0,
                max_attempts: DEFAULT_MAX_ATTEMPTS,
                max_in_flight: DEFAULT_MAX_IN_FLIGHT,
                trusted_proxies: Vec::new(),
                update_channel: "releases".to_string(),
                update_data_dir: dir.path().to_path_buf(),
                cors_origins: Vec::new(),
            });
            let cache: Arc<dyn crate::store::cache::CacheBackend> =
                Arc::new(crate::store::cache::MemoryCache::new());
            let upstream_client: Arc<dyn UpstreamClient> = upstream;
            let state = AppState::new(
                config,
                cache,
                persistence,
                upstream_client,
                Arc::new(arc_swap::ArcSwap::from_pointee(snapshot)),
                Arc::new(crate::channel::registry::ChannelRegistry::with_builtin()),
                Arc::new(crate::crypto::NoopCipher),
            );
            (state, dir)
        }

        #[tokio::test]
        async fn fetch_models_tries_next_credential_after_auth_dead() {
            let upstream = Arc::new(SequencedUpstream::new(vec![
                StatusCode::UNAUTHORIZED,
                StatusCode::OK,
            ]));
            let (state, _dir) = state_with(Arc::clone(&upstream)).await;

            let models = fetch_models(&state, 1).await.expect("model pull");
            assert_eq!(
                models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
                ["gpt-good"]
            );

            let seen = upstream.seen.lock().unwrap();
            assert_eq!(seen.len(), 2, "first auth-dead credential then next");
            assert_eq!(seen[0].uri, "http://fake.local/v1/models");
            assert_eq!(
                seen.iter()
                    .map(|s| s.authorization.as_deref())
                    .collect::<Vec<_>>(),
                [Some("Bearer bad-key"), Some("Bearer good-key")]
            );
        }
    }
}
