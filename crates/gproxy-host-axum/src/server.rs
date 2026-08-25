use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use axum::extract::DefaultBodyLimit;

pub(crate) const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;
const MAX_IN_FLIGHT: usize = 256;

#[derive(Clone)]
pub(crate) struct HostState {
    pub app: gproxy_app::AppHandle,
    pub semaphore: Arc<tokio::sync::Semaphore>,
    request_prefix: u64,
    request_counter: Arc<AtomicU64>,
}

impl HostState {
    fn new(app: gproxy_app::AppHandle) -> Result<Self, HostError> {
        let mut prefix = [0_u8; 8];
        getrandom::fill(&mut prefix).map_err(|_| HostError::Randomness)?;
        Ok(Self {
            app,
            semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_IN_FLIGHT)),
            request_prefix: u64::from_be_bytes(prefix),
            request_counter: Arc::new(AtomicU64::new(1)),
        })
    }

    pub(crate) fn request_id(&self) -> String {
        let sequence = self.request_counter.fetch_add(1, Ordering::Relaxed);
        format!("{:016x}-{sequence:016x}", self.request_prefix)
    }
}

pub struct AxumServer {
    address: SocketAddr,
    app: gproxy_app::AppHandle,
    task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl AxumServer {
    pub async fn bind(app: gproxy_app::AppHandle, address: SocketAddr) -> Result<Self, HostError> {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(HostError::Io)?;
        let address = listener.local_addr().map_err(HostError::Io)?;
        let shutdown = app.clone();
        let state = HostState::new(app.clone())?;
        let router = Router::new()
            .fallback(crate::ingress::handle)
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(state);
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
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
