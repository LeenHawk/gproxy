mod admission;
mod bindings;
mod credentials;
mod sinks;

use std::sync::Arc;
use std::time::Duration;

use gproxy_channel_api::{BindingStore, BoxFuture, CallerIdentity, UsageView};
use gproxy_core::{Host, Plan, ProviderRef, RequestCtx, Spawner};

use crate::cache::InProcessCache;
use crate::control::SnapshotControl;
use crate::secrets::EnvelopeCipher;

pub(crate) struct Services {
    pub store: gproxy_store::Store,
    pub cache: InProcessCache,
    pub cipher: EnvelopeCipher,
    pub control: SnapshotControl,
    pub transport: gproxy_upstream::Transport,
    #[cfg(not(target_arch = "wasm32"))]
    pub spawner: TokioSpawner,
}

#[derive(Clone)]
pub(crate) struct AppHost {
    pub services: Arc<Services>,
}

impl Host for AppHost {
    type Credentials = Self;
    type Cache = InProcessCache;
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
        admission::finish(self, request_id, settlement)
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
    ) -> Box<dyn UsageView + 'a> {
        Box::new(sinks::AppUsageView::new(
            self.services.store.clone(),
            identity.user_id,
            provider.id,
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
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct TokioSpawner;

#[cfg(not(target_arch = "wasm32"))]
impl Spawner for TokioSpawner {
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(task);
    }
}
