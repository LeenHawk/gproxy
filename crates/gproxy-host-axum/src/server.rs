use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use axum::extract::DefaultBodyLimit;

pub(crate) const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;

#[derive(Clone)]
pub struct HostConfig {
    max_in_flight: usize,
    instance_id: u64,
    trusted_proxies: Arc<[std::net::IpAddr]>,
    cors_origins: Arc<[String]>,
}

impl HostConfig {
    pub fn from_config(config: &gproxy_app::Config) -> Self {
        Self {
            max_in_flight: config.max_in_flight(),
            instance_id: config.instance_id(),
            trusted_proxies: config.trusted_proxies().into(),
            cors_origins: config.cors_origins().into(),
        }
    }
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            max_in_flight: 1024,
            instance_id: 0,
            trusted_proxies: Arc::new([]),
            cors_origins: Arc::new([]),
        }
    }
}

#[derive(Clone)]
pub(crate) struct HostState {
    pub app: gproxy_app::AppHandle,
    pub semaphore: Arc<tokio::sync::Semaphore>,
    pub trusted_proxies: Arc<[std::net::IpAddr]>,
    pub cors_origins: Arc<[String]>,
    pub uploads: Arc<UploadState>,
    instance_id: u64,
    request_prefix: u64,
    request_counter: Arc<AtomicU64>,
}

impl HostState {
    fn new(app: gproxy_app::AppHandle, config: HostConfig) -> Result<Self, HostError> {
        let mut prefix = [0_u8; 8];
        getrandom::fill(&mut prefix).map_err(|_| HostError::Randomness)?;
        Ok(Self {
            app,
            semaphore: Arc::new(tokio::sync::Semaphore::new(config.max_in_flight)),
            trusted_proxies: config.trusted_proxies,
            cors_origins: config.cors_origins,
            uploads: Arc::new(UploadState::default()),
            instance_id: config.instance_id,
            request_prefix: u64::from_be_bytes(prefix),
            request_counter: Arc::new(AtomicU64::new(1)),
        })
    }

    pub(crate) fn request_id(&self) -> String {
        let sequence = self.request_counter.fetch_add(1, Ordering::Relaxed);
        format!(
            "{}-{:016x}-{sequence:016x}",
            self.instance_id, self.request_prefix
        )
    }
}

#[derive(Default)]
pub(crate) struct UploadState {
    in_flight: std::sync::atomic::AtomicUsize,
    changed: tokio::sync::Notify,
}

impl UploadState {
    pub(crate) async fn acquire(self: &Arc<Self>, limit: usize) -> Option<UploadPermit> {
        if limit == 0 {
            return None;
        }
        loop {
            let current = self.in_flight.load(Ordering::Acquire);
            if current < limit
                && self
                    .in_flight
                    .compare_exchange_weak(
                        current,
                        current + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
            {
                return Some(UploadPermit(Arc::clone(self)));
            }
            self.changed.notified().await;
        }
    }
}

pub(crate) struct UploadPermit(Arc<UploadState>);

impl Drop for UploadPermit {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::AcqRel);
        self.0.changed.notify_one();
    }
}

pub struct AxumServer {
    address: SocketAddr,
    app: gproxy_app::AppHandle,
    task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl AxumServer {
    pub async fn bind(app: gproxy_app::AppHandle, address: SocketAddr) -> Result<Self, HostError> {
        Self::bind_with_config(app, address, HostConfig::default()).await
    }

    pub async fn bind_with_config(
        app: gproxy_app::AppHandle,
        address: SocketAddr,
        config: HostConfig,
    ) -> Result<Self, HostError> {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(HostError::Io)?;
        let address = listener.local_addr().map_err(HostError::Io)?;
        let shutdown = app.clone();
        let state = HostState::new(app.clone(), config)?;
        let router = Router::new()
            .fallback(crate::ingress::handle)
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(state);
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move { shutdown.wait_shutdown().await })
            .await
        });
        Ok(Self { address, app, task })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub async fn shutdown(self) -> Result<(), HostError> {
        self.app.shutdown();
        self.task
            .await
            .map_err(HostError::Join)?
            .map_err(HostError::Io)
    }

    pub async fn wait(self) -> Result<(), HostError> {
        self.task
            .await
            .map_err(HostError::Join)?
            .map_err(HostError::Io)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("listener: {0}")]
    Io(#[source] std::io::Error),
    #[error("listener task: {0}")]
    Join(#[source] tokio::task::JoinError),
    #[error("secure request-id randomness unavailable")]
    Randomness,
}
