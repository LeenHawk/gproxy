use std::sync::Arc;

use gproxy_core::{CacheBackend, Core, ExecOutcome, RequestCtx};

use crate::AppError;
use crate::host::AppHost;

pub struct App;

#[derive(Clone)]
pub struct AppHandle {
    pub(crate) inner: Arc<AppInner>,
}

pub(crate) struct AppInner {
    pub core: Core<AppHost>,
    pub host: AppHost,
    #[cfg(not(target_arch = "wasm32"))]
    pub shutdown: tokio::sync::watch::Sender<bool>,
    #[cfg(target_arch = "wasm32")]
    pub shutdown: std::sync::atomic::AtomicBool,
}

impl AppHandle {
    pub async fn execute(
        &self,
        request: RequestCtx,
    ) -> Result<ExecOutcome, gproxy_core::CoreError> {
        self.inner
            .core
            .execute(&self.inner.host.services.control, request)
            .await
    }

    pub async fn mutate(
        &self,
        mutation: crate::ControlMutation,
    ) -> Result<crate::MutationResult, AppError> {
        crate::control::apply(self, mutation).await
    }

    pub async fn reload(&self) -> Result<(), AppError> {
        self.inner.host.services.control.reload().await?;
        self.inner
            .host
            .services
            .cache
            .incr("gproxy:invalidate", 1, None)
            .await
            .map_err(|error| AppError::Cache(error.to_string()))?;
        Ok(())
    }

    pub fn shutdown(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.inner.shutdown.send_replace(true);
        #[cfg(target_arch = "wasm32")]
        self.inner
            .shutdown
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub async fn wait_shutdown(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut receiver = self.inner.shutdown.subscribe();
            while !*receiver.borrow_and_update() && receiver.changed().await.is_ok() {}
        }
        #[cfg(target_arch = "wasm32")]
        while !self
            .inner
            .shutdown
            .load(std::sync::atomic::Ordering::Acquire)
        {
            gloo_timers::future::TimeoutFuture::new(25).await;
        }
    }

    pub async fn admission_pending(&self, request_id: &str) -> Result<bool, AppError> {
        self.inner
            .host
            .services
            .cache
            .get(&format!("gproxy:admission:{request_id}"))
            .await
            .map(|value| value.is_some())
            .map_err(|error| AppError::Cache(error.to_string()))
    }

    pub async fn usage_by_request(
        &self,
        request_id: &str,
    ) -> Result<Option<gproxy_store::records::UsageRecord>, AppError> {
        Ok(self
            .inner
            .host
            .services
            .store
            .usage_by_request(request_id)
            .await?)
    }

    pub async fn quota_windows(
        &self,
    ) -> Result<Vec<gproxy_store::records::QuotaWindowRecord>, AppError> {
        Ok(self.inner.host.services.store.quota_windows().await?)
    }

    pub async fn observe_credential_quota_cycle(
        &self,
        observation: gproxy_store::records::CredentialQuotaObservation,
    ) -> Result<gproxy_store::records::CredentialQuotaCycleRecord, AppError> {
        Ok(self
            .inner
            .host
            .services
            .control
            .observe_credential_quota_cycle(&observation)
            .await?)
    }
}
