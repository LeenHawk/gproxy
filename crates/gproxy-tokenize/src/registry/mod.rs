mod access;
mod asset;
mod load;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytes::Bytes;
use dashmap::DashMap;
use tokenizers::Tokenizer;

#[cfg(test)]
pub(crate) use asset::bytes as bundled_bytes;
pub(crate) use asset::{BUNDLED_NAMES, BUNDLED_PRIMARY};
pub use load::MAX_TOKENIZER_BYTES;

#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RegistryError(String);

impl RegistryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

pub trait TokenizerStore: Send + Sync {
    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>, RegistryError>>;
    fn get<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, RegistryError>>;
    fn put<'a>(
        &'a self,
        name: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), RegistryError>>;
    fn quarantine<'a>(
        &'a self,
        name: &'a str,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<(), RegistryError>> {
        let _ = (name, reason);
        Box::pin(async { Ok(()) })
    }
}

pub trait TokenizerClient: Send + Sync {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VocabSource {
    BuiltinTiktoken,
    Bundled,
    Downloaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabInfo {
    pub name: String,
    pub source: VocabSource,
    pub loaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadRequestStatus {
    Scheduled,
    AlreadyInFlight,
    NegativeCached,
    NoRuntime,
}

pub struct TokenizerRegistry {
    pub(super) store: Arc<dyn TokenizerStore>,
    pub(super) upstream: Arc<dyn TokenizerClient>,
    pub(super) download_enabled: AtomicBool,
    pub(super) loaded: Arc<DashMap<String, Arc<Tokenizer>>>,
    pub(super) inflight: Arc<DashMap<String, ()>>,
    pub(super) negative: Arc<DashMap<String, ()>>,
}

impl TokenizerRegistry {
    pub fn new(store: Arc<dyn TokenizerStore>, upstream: Arc<dyn TokenizerClient>) -> Self {
        Self {
            store,
            upstream,
            download_enabled: AtomicBool::new(false),
            loaded: Arc::new(DashMap::new()),
            inflight: Arc::new(DashMap::new()),
            negative: Arc::new(DashMap::new()),
        }
    }

    pub fn set_download_enabled(&self, enabled: bool) {
        self.download_enabled.store(enabled, Ordering::Relaxed);
        if enabled {
            self.negative.clear();
        }
    }

    pub fn download_enabled(&self) -> bool {
        self.download_enabled.load(Ordering::Relaxed)
    }

    pub fn evict(&self, name: &str) {
        self.loaded.remove(name);
        self.negative.remove(name);
    }
}

pub(super) fn info(name: &str, source: VocabSource, loaded: bool) -> VocabInfo {
    VocabInfo {
        name: name.to_owned(),
        source,
        loaded,
    }
}
