//! Explicit assembly of an engine. Every dependency that shapes behaviour
//! (cache, observer, secret codec) is required; nothing defaults to a no-op.

use crate::{Core, CoreData, Observer, SecretCodec};
use arc_swap::ArcSwap;
use gproxy_cache::Cache;
use gproxy_channel::{BaseChannel, ChannelRegistry, RegistryError};
use gproxy_store::Store;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("a cache is required")]
    MissingCache,
    #[error("an observer is required; observe nothing explicitly if that is intended")]
    MissingObserver,
    #[error("a secret codec is required; PlaintextCodec must be chosen explicitly")]
    MissingSecretCodec,
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

pub struct CoreBuilder<C> {
    store: Arc<Store<C>>,
    cache: Option<Arc<dyn Cache>>,
    observer: Option<Arc<dyn Observer>>,
    codec: Option<Arc<dyn SecretCodec>>,
    channels: ChannelRegistry,
    clients: Option<gproxy_client::ClientPool>,
    files: Option<gproxy_file::Operator>,
    data: Option<Arc<CoreData>>,
}

impl<C> CoreBuilder<C> {
    pub fn new(store: Arc<Store<C>>) -> Self {
        Self {
            store,
            cache: None,
            observer: None,
            codec: None,
            channels: ChannelRegistry::new(),
            clients: None,
            files: None,
            data: None,
        }
    }
    pub fn cache(mut self, cache: Arc<dyn Cache>) -> Self {
        self.cache = Some(cache);
        self
    }
    pub fn observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = Some(observer);
        self
    }
    pub fn secret_codec(mut self, codec: Arc<dyn SecretCodec>) -> Self {
        self.codec = Some(codec);
        self
    }
    /// Register one channel implementation; duplicate IDs are rejected at build.
    pub fn channel(mut self, channel: Arc<dyn BaseChannel>) -> Result<Self, RegistryError> {
        self.channels.register(channel)?;
        Ok(self)
    }
    pub fn channels(mut self, channels: ChannelRegistry) -> Self {
        self.channels = channels;
        self
    }
    /// Outbound client cache. Defaults to `ClientPool::default()`.
    pub fn client_pool(mut self, clients: gproxy_client::ClientPool) -> Self {
        self.clients = Some(clients);
        self
    }
    /// Object storage for bodies core publishes itself (generated images,
    /// downloaded video output). Without it, publishing or reading a locally
    /// stored body fails with `Unsupported` before any side effect; upstream
    /// file reads by ID still work.
    pub fn file_storage(mut self, operator: Option<gproxy_file::Operator>) -> Self {
        self.files = operator;
        self
    }
    /// Initial snapshot, e.g. one assembled before construction. Defaults to an
    /// empty snapshot at revision 0 until `load_data` runs.
    pub fn snapshot(mut self, data: Arc<CoreData>) -> Self {
        self.data = Some(data);
        self
    }

    pub fn build(self) -> Result<Core<C>, BuildError> {
        Ok(Core {
            store: self.store,
            cache: self.cache.ok_or(BuildError::MissingCache)?,
            observer: self.observer.ok_or(BuildError::MissingObserver)?,
            codec: self.codec.ok_or(BuildError::MissingSecretCodec)?,
            channels: Arc::new(self.channels),
            clients: self.clients.unwrap_or_default(),
            files: self.files,
            data: ArcSwap::from(self.data.unwrap_or_default()),
        })
    }
}
