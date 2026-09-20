//! Explicit assembly of an engine. Every dependency that shapes behaviour
//! (cache, secret codec) is required. Observation defaults to Store persistence.

use crate::{Core, CoreData, Observer, PublicationUrl, SecretCodec};
use arc_swap::ArcSwap;
use gproxy_cache::Cache;
use gproxy_channel::{BaseChannel, ChannelRegistry, RegistryError};
use gproxy_store::Store;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("a cache is required")]
    MissingCache,
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
    publication_url: Option<Arc<dyn PublicationUrl>>,
    instance_id: Option<Arc<str>>,
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
            publication_url: None,
            instance_id: None,
            data: None,
        }
    }
    /// A stable identity for this process (hostname, pod name, isolate id).
    /// Random when not supplied, which is right for a single instance.
    pub fn instance_id(mut self, instance_id: impl Into<String>) -> Self {
        self.instance_id = Some(Arc::from(instance_id.into()));
        self
    }
    pub fn cache(mut self, cache: Arc<dyn Cache>) -> Self {
        self.cache = Some(cache);
        self
    }
    /// Replace built-in Store persistence with a host implementation.
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
    /// Link builder for `PublicationKind::Url`: the host mints the public URL
    /// for a body core stores and serves `Core::read_publication` on that
    /// route. Without it, URL publication (images `response_format: url`) is
    /// refused before any side effect; `Id` publication is unaffected.
    pub fn publication_url(mut self, builder: Arc<dyn PublicationUrl>) -> Self {
        self.publication_url = Some(builder);
        self
    }
    /// Initial snapshot, e.g. one assembled before construction. Defaults to an
    /// empty snapshot at revision 0 until `load_data` runs.
    pub fn snapshot(mut self, data: Arc<CoreData>) -> Self {
        self.data = Some(data);
        self
    }

    pub fn build(self) -> Result<Core<C>, BuildError>
    where
        C: gproxy_seaorm::BatchConnectionTrait + Send + Sync + 'static,
    {
        let observer = self
            .observer
            .unwrap_or_else(|| Arc::new(crate::StoreObserver::new(self.store.clone())));
        Ok(Core {
            store: self.store,
            cache: self.cache.ok_or(BuildError::MissingCache)?,
            observer,
            codec: self.codec.ok_or(BuildError::MissingSecretCodec)?,
            channels: Arc::new(self.channels),
            clients: self.clients.unwrap_or_default(),
            files: self.files,
            publication_url: self.publication_url,
            instance_id: self
                .instance_id
                .unwrap_or_else(|| Arc::from(crate::ids::random_id())),
            data: ArcSwap::from(self.data.unwrap_or_default()),
        })
    }
}
