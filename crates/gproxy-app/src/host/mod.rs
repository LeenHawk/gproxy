mod admission;
mod bindings;
mod continuations;
mod credentials;
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
pub(crate) use admission::{authenticate_headers, authorize};

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
    ) -> BoxFuture<'a, Result<(), gproxy_core::CoreError>> {
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
        target: &'a gproxy_core::Target,
        body: &'a bytes::Bytes,
    ) -> BoxFuture<'a, Result<(), gproxy_core::CoreError>> {
        admission::admit_credential(self, target, body)
    }

    fn record_credential_health<'a>(
        &'a self,
        credential: gproxy_channel_api::CredentialId,
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
            if let Err(error) = self.services.store.record_credential_health(&input).await {
                tracing::error!(error = %error, "credential health persistence failed");
            } else {
                self.services.control.observe_credential_health(&input);
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
pub(crate) struct TokioSpawner;

#[cfg(not(target_arch = "wasm32"))]
impl Spawner for TokioSpawner {
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(task);
    }
}
