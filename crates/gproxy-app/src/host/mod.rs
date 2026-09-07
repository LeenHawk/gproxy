mod admission;
mod bindings;
mod continuations;
mod credentials;
pub(crate) mod oauth;
mod sinks;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod tokenizers;
mod usage_view;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use std::time::Duration;

use gproxy_channel_api::{BindingStore, BoxFuture, CallerIdentity, UsageView};
use gproxy_core::{CredentialHealth, Host, Plan, ProviderRef, RequestCtx, Spawner};

use crate::Shared;
use crate::cache::AppCache;
use crate::control::SnapshotControl;

use crate::secrets::EnvelopeCipher;
#[cfg(test)]
pub(crate) use admission::authenticate_headers;
#[cfg(test)]
pub(crate) use admission::authorize;
pub(crate) use admission::{catalogue_permitted, provider_permitted};

pub(crate) struct Services {
    pub store: gproxy_store::Store,
    pub cache: AppCache,
    pub cipher: EnvelopeCipher,
    pub control: SnapshotControl,
    pub transport: gproxy_upstream::Transport,
    pub health_sequence: std::sync::atomic::AtomicU64,
    #[cfg(not(target_arch = "wasm32"))]
    pub tokenizers: Arc<gproxy_tokenize::TokenizerRegistry>,
    #[cfg(not(target_arch = "wasm32"))]
    pub spawner: TokioSpawner,
    #[cfg(not(target_arch = "wasm32"))]
    pub continuations: continuations::LocalContinuations,
}

#[derive(Clone)]
pub(crate) struct AppHost {
    pub services: Shared<Services>,
}

impl Host for AppHost {
    type Credentials = Self;
    type Cache = AppCache;
    type Transport = gproxy_upstream::Transport;
    type Usage = Self;
    type Capture = Self;

    fn begin_credential_usage<'a>(
        &'a self,
        request_id: &'a str,
        target: &'a gproxy_core::Target,
        started_at_ms: i64,
    ) -> BoxFuture<'a, Result<(), gproxy_core::CoreError>> {
        Box::pin(async move {
            self.services
                .store
                .begin_credential_usage(
                    request_id,
                    target.credential.0,
                    &target.upstream_model,
                    started_at_ms,
                )
                .await
                .map_err(|error| gproxy_core::CoreError::Internal(error.to_string()))
        })
    }

    fn credentials(&self) -> &Self::Credentials {
        self
    }

    fn cache(&self) -> &Self::Cache {
        &self.services.cache
    }

    fn transport(&self) -> &Self::Transport {
        &self.services.transport
    }

    fn usage(&self) -> &Self::Usage {
        self
    }

    fn capture(&self) -> &Self::Capture {
        self
    }

    fn authenticate<'a>(
        &'a self,
        request: &'a RequestCtx,
    ) -> BoxFuture<'a, Result<CallerIdentity, gproxy_core::CoreError>> {
        admission::authenticate(self, request)
    }

    fn admit<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        request: &'a RequestCtx,
        operation: Option<gproxy_protocol::OperationKey>,
        plan: &'a Plan,
    ) -> BoxFuture<'a, Result<Plan, gproxy_core::CoreError>> {
        admission::admit(self, identity, request, operation, plan)
    }

    fn finish_admission<'a>(
        &'a self,
        request_id: &'a str,
        settlement: Option<&'a gproxy_core::Settlement>,
    ) -> BoxFuture<'a, ()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let host = self.clone();
            let request_id = request_id.to_owned();
            let settlement = settlement.cloned();
            Box::pin(async move {
                let task = tokio::spawn(async move {
                    admission::finish(&host, &request_id, settlement.as_ref()).await;
                });
                if let Err(error) = task.await {
                    tracing::error!(error = %error, "quota reconciliation task failed");
                }
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            admission::finish(self, request_id, settlement)
        }
    }

    fn admit_credential<'a>(
        &'a self,
        request_id: &'a str,
        target: &'a gproxy_core::Target,
        body: &'a bytes::Bytes,
        settle: gproxy_protocol::SettleMode,
    ) -> BoxFuture<'a, Result<(), gproxy_core::CoreError>> {
        admission::admit_credential(self, request_id, target, body, settle)
    }

    fn admit_retry<'a>(
        &'a self,
        request_id: &'a str,
        target: &'a gproxy_core::Target,
        body: &'a bytes::Bytes,
        settle: gproxy_protocol::SettleMode,
    ) -> BoxFuture<'a, Result<(), gproxy_core::CoreError>> {
        Box::pin(admission::retry::admit(
            self, request_id, target, body, settle,
        ))
    }

    fn count_tokens<'a>(
        &'a self,
        model: &'a str,
        body: &'a bytes::Bytes,
        tokenizer_map: Option<&'a serde_json::Value>,
    ) -> BoxFuture<'a, Result<u64, gproxy_core::CoreError>> {
        let model = model.to_owned();
        let body = body.clone();
        let tokenizer_map = tokenizer_map.cloned();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let registry = self.services.tokenizers.clone();
            Box::pin(async move {
                tokio::task::spawn_blocking(move || {
                    gproxy_tokenize::count(&model, &body, tokenizer_map.as_ref(), &registry)
                })
                .await
                .map_err(|error| {
                    gproxy_core::CoreError::Internal(format!("tokenizer task failed: {error}"))
                })
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(async move {
                Ok(gproxy_tokenize::count(
                    &model,
                    &body,
                    tokenizer_map.as_ref(),
                    (),
                ))
            })
        }
    }

    fn record_credential_health<'a>(
        &'a self,
        credential: gproxy_channel_api::CredentialId,
        model: &'a str,
        credential_version: u64,
        health: CredentialHealth,
        response_status: Option<http::StatusCode>,
        detail: &'a str,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let state = match health {
                CredentialHealth::Healthy => gproxy_store::records::CredentialHealthState::Healthy,
                CredentialHealth::Degraded => {
                    gproxy_store::records::CredentialHealthState::Degraded
                }
                CredentialHealth::Dead => gproxy_store::records::CredentialHealthState::Dead,
            };
            let input = gproxy_store::records::CredentialHealthInput {
                credential_id: credential.0,
                model: model.to_owned(),
                credential_version,
                version: match health_version(&self.services.health_sequence) {
                    Some(version) => version,
                    None => return,
                },
                state,
                observed_at: admission::unix_now(),
                response_status: response_status.map(|status| status.as_u16()),
                detail: Some(detail.into()),
            };
            let unchanged = self
                .services
                .control
                .credential_health_state(credential, model)
                == Some((credential_version, state));
            // A transition must be durable and visible before failover continues.
            // Re-observing the same state only refreshes the row, so a native host
            // takes it off the request path; edge hosts have no spawner and stay inline.
            if unchanged
                && !self
                    .services
                    .control
                    .health_refresh_due(credential, model, input.observed_at)
            {
                return;
            }
            match self.spawner().filter(|_| unchanged) {
                Some(spawner) => {
                    let host = self.clone();
                    spawner.spawn(Box::pin(async move {
                        persist_credential_health(&host, &input).await;
                    }));
                }
                None => persist_credential_health(self, &input).await,
            }
        })
    }

    fn observe_credential_quota<'a>(
        &'a self,
        credential: gproxy_channel_api::CredentialId,
        observations: Vec<gproxy_channel_api::QuotaObservation>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let observed_at = admission::unix_now();
            for value in observations {
                // The channel reported only wire facts: an upstream-declared
                // period end is an upstream boundary; a start computed from
                // end minus window length is derived, not exact.
                let (source, confidence) = match (value.period_start, value.period_end) {
                    (Some(_), Some(_)) => (
                        gproxy_store::records::QuotaBoundarySource::Upstream,
                        gproxy_store::records::QuotaBoundaryConfidence::Derived,
                    ),
                    (None, Some(_)) => (
                        gproxy_store::records::QuotaBoundarySource::Upstream,
                        gproxy_store::records::QuotaBoundaryConfidence::Partial,
                    ),
                    _ => (
                        gproxy_store::records::QuotaBoundarySource::Unknown,
                        gproxy_store::records::QuotaBoundaryConfidence::Unknown,
                    ),
                };
                let observation = gproxy_store::records::CredentialQuotaObservation {
                    unit: value.unit,
                    reset_behavior: value.reset_behavior,
                    scope: value.scope,
                    sample: value
                        .sample
                        .expect("upstream quota carries its sampling interval"),
                    credential_id: credential.0,
                    window_key: value.window_key,
                    label: value.label,
                    period_start: value.period_start,
                    period_end: value.period_end,
                    boundary_source: source,
                    boundary_confidence: confidence,
                    observed_at: value
                        .sample
                        .map_or(observed_at, |sample| sample.received_at_ms / 1000),
                    upstream_used: value.upstream_used,
                    upstream_limit: value.upstream_limit,
                    used_percent: value.used_percent,
                };
                if let Err(error) = self
                    .services
                    .control
                    .observe_credential_quota_cycle(&observation)
                    .await
                {
                    tracing::warn!(error = %error, "credential quota observation failed");
                }
            }
        })
    }

    fn wait<'a>(&'a self, duration: Duration) -> BoxFuture<'a, ()> {
        #[cfg(not(target_arch = "wasm32"))]
        return Box::pin(tokio::time::sleep(duration));
        #[cfg(target_arch = "wasm32")]
        Box::pin(gloo_timers::future::TimeoutFuture::new(
            duration.as_millis().min(u128::from(u32::MAX)) as u32,
        ))
    }

    fn surface_usage<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        provider: &'a ProviderRef,
        credential: gproxy_channel_api::CredentialId,
    ) -> Box<dyn UsageView + 'a> {
        Box::new(usage_view::AppUsageView::new(
            self.clone(),
            identity.clone(),
            provider.id,
            credential,
        ))
    }

    fn spawner(&self) -> Option<&dyn Spawner> {
        #[cfg(not(target_arch = "wasm32"))]
        return Some(&self.services.spawner);
        #[cfg(target_arch = "wasm32")]
        None
    }

    fn bindings(&self) -> Option<&dyn BindingStore> {
        Some(self)
    }

    fn oauth(&self) -> Option<&dyn gproxy_channel_api::OAuthService> {
        Some(self)
    }

    fn continuations(&self) -> Option<&dyn gproxy_core::ContinuationStore> {
        #[cfg(not(target_arch = "wasm32"))]
        return Some(&self.services.continuations);
        #[cfg(target_arch = "wasm32")]
        None
    }
}

fn health_version(sequence: &std::sync::atomic::AtomicU64) -> Option<i64> {
    let elapsed = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()?;
    let millis = i64::try_from(elapsed.as_millis()).ok()?;
    let sequence = sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 1_000_000;
    Some(
        millis
            .saturating_mul(1_000_000)
            .saturating_add(sequence as i64),
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct TokioSpawner {
    settlements: Arc<tokio::sync::Semaphore>,
}

/// Queued settlements beyond the in-flight cap before the response path
/// starts waiting. Each one holds its request and response bodies, so this
/// is a memory bound as much as a queue bound. Streams reserve their slot
/// while still open, which is why the pool also covers every in-flight
/// request: a full house of streams must not starve buffered settlement.
#[cfg(not(target_arch = "wasm32"))]
const SETTLEMENT_BACKLOG: usize = 2048;

#[cfg(not(target_arch = "wasm32"))]
impl TokioSpawner {
    pub(crate) fn new(max_in_flight: usize) -> Self {
        Self {
            settlements: Arc::new(tokio::sync::Semaphore::new(
                max_in_flight + SETTLEMENT_BACKLOG,
            )),
        }
    }
}

async fn persist_credential_health(
    host: &AppHost,
    input: &gproxy_store::records::CredentialHealthInput,
) {
    if let Err(error) = host.services.store.record_credential_health(input).await {
        tracing::error!(error = %error, "credential health persistence failed");
    } else {
        host.services.control.observe_credential_health(input);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Spawner for TokioSpawner {
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(task);
    }

    fn reserve_settlement(&self) -> BoxFuture<'_, gproxy_core::SettlementPermit> {
        Box::pin(async move {
            let permit = self
                .settlements
                .clone()
                .acquire_owned()
                .await
                .expect("settlement semaphore is never closed");
            Box::new(permit) as gproxy_core::SettlementPermit
        })
    }
}
