//! Assembling a handle.
//!
//! Everything that changes behaviour is explicit. There is no implicit
//! plaintext fallback for secrets, no implicit cache and no implicit schema
//! migration beyond the one this builder is told to run. What the builder does
//! supply are the defaults a single-instance host would otherwise write out by
//! hand: a process-local cache, the channels compiled into this build, a
//! pooled outbound client and Store-backed observation.

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use arc_swap::ArcSwap;
use gproxy_cache::Cache;
use gproxy_channel::{BaseChannel, ChannelRegistry};
use gproxy_client::ClientPool;
use gproxy_core::{
    AesGcmCodec, Core, FetchPolicy, Observer, PlaintextCodec, PublicationUrl, SecretCodec,
};
use gproxy_file::Operator;
use gproxy_seaorm::{BatchConnectionTrait, SchemaSyncConnectionTrait};
use gproxy_store::{Store, entity::config::setting};

use crate::{
    SdkError, SdkResult,
    handle::{Gproxy, Inner},
    resolve::RoutingTable,
    sync::{self, DEFAULT_POLL_INTERVAL, SyncMode},
};

/// How long a login started on one instance stays resumable on any instance.
/// Both are cache TTLs, so a slow person loses the session rather than the
/// credential: nothing is persisted until the exchange succeeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginTtl {
    /// Browser redirect flows: the window between `authorize` and the callback.
    pub authorization_code: Duration,
    /// Device flows: long enough for a person to type a code on another device.
    pub device_code: Duration,
}

impl Default for LoginTtl {
    fn default() -> Self {
        Self {
            authorization_code: Duration::from_secs(600),
            device_code: Duration::from_secs(900),
        }
    }
}

/// Assembly of a [`Gproxy`] over one database connection.
pub struct GproxyBuilder<C> {
    connection: C,
    options: Options,
}

struct Options {
    cache: Option<Arc<dyn Cache>>,
    codec: Option<Arc<dyn SecretCodec>>,
    channels: Vec<Arc<dyn BaseChannel>>,
    default_channels: bool,
    clients: Option<ClientPool>,
    files: Option<Operator>,
    publication_url: Option<Arc<dyn PublicationUrl>>,
    fetch_policy: Option<Arc<dyn FetchPolicy>>,
    observer: Option<Arc<dyn Observer>>,
    instance_id: Option<String>,
    sync_schema: bool,
    initial_reload: bool,
    sync_mode: SyncMode,
    poll_interval: Duration,
    login_ttl: LoginTtl,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cache: None,
            codec: None,
            channels: Vec::new(),
            default_channels: true,
            clients: None,
            files: None,
            publication_url: None,
            fetch_policy: None,
            observer: None,
            instance_id: None,
            sync_schema: true,
            initial_reload: true,
            sync_mode: SyncMode::default(),
            poll_interval: DEFAULT_POLL_INTERVAL,
            login_ttl: LoginTtl::default(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl GproxyBuilder<sea_orm::DatabaseConnection> {
    /// A SQLite file, created if it is not there yet.
    ///
    /// One connection, deliberately: SQLite has a single writer, and a pool
    /// larger than one turns a serialized write into a `database is locked`
    /// race without making any read faster.
    pub async fn sqlite(path: impl AsRef<str>) -> SdkResult<Self> {
        Self::open(&format!("sqlite://{}?mode=rwc", path.as_ref())).await
    }

    /// A private in-memory SQLite database that lives as long as the handle.
    /// The single connection is what keeps it alive; a second one would see an
    /// empty database.
    pub async fn sqlite_memory() -> SdkResult<Self> {
        Self::open("sqlite::memory:").await
    }

    /// Any URL SeaORM understands, with the driver features this build enabled:
    /// `postgres://…` needs `postgres`, `mysql://…` needs `mysql`.
    pub async fn database_url(url: impl Into<String>) -> SdkResult<Self> {
        let mut options = sea_orm::ConnectOptions::new(url.into());
        options.sqlx_logging(false);
        Ok(Self::connection(sea_orm::Database::connect(options).await?))
    }

    async fn open(url: &str) -> SdkResult<Self> {
        let mut options = sea_orm::ConnectOptions::new(url.to_owned());
        // min = max = 1 is not only SQLite's single writer: it is what keeps a
        // `sqlite::memory:` database alive, because the database exists only
        // as long as its connection does.
        options
            .min_connections(1)
            .max_connections(1)
            .sqlx_logging(false);
        Ok(Self::connection(sea_orm::Database::connect(options).await?))
    }
}

impl<C> GproxyBuilder<C> {
    /// An already-open connection: a pool the host also uses for its own
    /// tables, a libSQL client, a Cloudflare D1 binding.
    pub fn connection(connection: C) -> Self {
        Self {
            connection,
            options: Options::default(),
        }
    }

    /// Shared transient state and the invalidation transport. Defaults to a
    /// process-local `MemoryCache` natively and to `StoreCache` on wasm32,
    /// which is correct for one instance; several instances need a cache they
    /// actually share (Redis natively, `StoreCache` over D1/libSQL at the edge).
    pub fn cache(mut self, cache: Arc<dyn Cache>) -> Self {
        self.options.cache = Some(cache);
        self
    }

    /// Seal credential secrets with AES-256-GCM under this master key.
    pub fn master_key(mut self, master_key: [u8; 32]) -> Self {
        self.options.codec = Some(Arc::new(AesGcmCodec::new(master_key)));
        self
    }

    /// Store credential secrets unencrypted. Never a fallback: a host that
    /// wants this has to say so, which is why there is no default codec.
    pub fn plaintext_secrets(mut self) -> Self {
        self.options.codec = Some(Arc::new(PlaintextCodec));
        self
    }

    /// Register one more channel. A channel registered here wins over the
    /// compiled-in one with the same id.
    pub fn channel(mut self, channel: Arc<dyn BaseChannel>) -> Self {
        self.options.channels.push(channel);
        self
    }

    /// Register nothing but the channels this builder was given explicitly.
    pub fn without_default_channels(mut self) -> Self {
        self.options.default_channels = false;
        self
    }

    /// The outbound client cache. Defaults to `ClientPool::default()`.
    pub fn client_pool(mut self, clients: ClientPool) -> Self {
        self.options.clients = Some(clients);
        self
    }

    /// Object storage for published bodies and downloaded vocabularies.
    pub fn file_storage(mut self, files: Operator) -> Self {
        self.options.files = Some(files);
        self
    }

    /// The host's link builder for `PublicationKind::Url`.
    pub fn publication_url(mut self, builder: Arc<dyn PublicationUrl>) -> Self {
        self.options.publication_url = Some(builder);
        self
    }

    /// Which URLs core may fetch on a caller's behalf. Defaults to
    /// `DefaultFetchPolicy`: http/https to public addresses only.
    pub fn fetch_policy(mut self, policy: Arc<dyn FetchPolicy>) -> Self {
        self.options.fetch_policy = Some(policy);
        self
    }

    /// Replace Store persistence of usage and captures with the host's own.
    pub fn observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.options.observer = Some(observer);
        self
    }

    /// A stable identity for this process, recorded with continuations that
    /// hold a live upstream connection. Random when not supplied.
    pub fn instance_id(mut self, instance_id: impl Into<String>) -> Self {
        self.options.instance_id = Some(instance_id.into());
        self
    }

    /// Create or incrementally synchronize the entity schema during `build`.
    /// On by default; turn it off where migrations are someone else's job or
    /// where more than one process would race to write DDL.
    pub fn sync_schema(mut self, sync: bool) -> Self {
        self.options.sync_schema = sync;
        self
    }

    /// Load the first configuration snapshot during `build`. On by default;
    /// off leaves the handle serving an empty snapshot until `reload`.
    pub fn initial_reload(mut self, reload: bool) -> Self {
        self.options.initial_reload = reload;
        self
    }

    pub fn sync_mode(mut self, mode: SyncMode) -> Self {
        self.options.sync_mode = mode;
        self
    }

    /// How often the background loop compares the durable revision. This is
    /// the backstop behind notifications, not the primary path, so seconds of
    /// lag here cost nothing when the cache is healthy.
    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.options.poll_interval = interval;
        self
    }

    pub fn login_ttl(mut self, ttl: LoginTtl) -> Self {
        self.options.login_ttl = ttl;
        self
    }
}

impl<C> GproxyBuilder<C>
where
    C: BatchConnectionTrait + SchemaSyncConnectionTrait + Send + Sync + 'static,
{
    /// Assemble, synchronizing the schema first unless that was turned off.
    pub async fn build(self) -> SdkResult<Gproxy<C>> {
        let Self {
            connection,
            options,
        } = self;
        let store = Arc::new(Store::new(connection));
        if options.sync_schema {
            store.sync().await?;
        }
        options.assemble(store).await
    }
}

impl<C> GproxyBuilder<C>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    /// Assemble over a connection that cannot synchronize schema on its own —
    /// a D1 binding whose DDL is planned by Wrangler, a replica. The schema
    /// must already match; nothing here checks it.
    pub async fn build_unsynced(self) -> SdkResult<Gproxy<C>> {
        let Self {
            connection,
            options,
        } = self;
        options.assemble(Arc::new(Store::new(connection))).await
    }
}

impl Options {
    async fn assemble<C>(self, store: Arc<Store<C>>) -> SdkResult<Gproxy<C>>
    where
        C: BatchConnectionTrait + Send + Sync + 'static,
    {
        let Self {
            cache,
            codec,
            channels,
            default_channels,
            clients,
            files,
            publication_url,
            fetch_policy,
            observer,
            instance_id,
            sync_schema: _,
            initial_reload,
            sync_mode,
            poll_interval,
            login_ttl,
        } = self;

        // Everything that can be rejected without touching the database is
        // rejected first: a refused build leaves no trace of itself.
        let codec = codec.ok_or_else(|| {
            SdkError::invalid("secret codec required: call master_key() or plaintext_secrets()")
        })?;
        let channels = registry(channels, default_channels)?;

        // The settings row is a precondition, not a convenience: it carries
        // the revision every management write bumps and the execution limits
        // core derives its codec bounds from. Creating it changes nothing that
        // already exists.
        store
            .settings()
            .update(setting::ActiveModel::default())
            .await?;

        let cache = match cache {
            Some(cache) => cache,
            None => default_cache(&store)?,
        };

        let mut core = Core::builder(store.clone())
            .cache(cache.clone())
            .secret_codec(codec)
            .channels(channels)
            .file_storage(files);
        if let Some(clients) = clients {
            core = core.client_pool(clients);
        }
        if let Some(observer) = observer {
            core = core.observer(observer);
        }
        if let Some(publication_url) = publication_url {
            core = core.publication_url(publication_url);
        }
        if let Some(fetch_policy) = fetch_policy {
            core = core.fetch_policy(fetch_policy);
        }
        if let Some(instance_id) = instance_id {
            core = core.instance_id(instance_id);
        }
        let core = core.build()?;

        let instance = core.instance_id().clone();
        let inner = Arc::new(Inner {
            core,
            store,
            cache,
            routing: ArcSwap::from_pointee(RoutingTable::default()),
            rotation: Default::default(),
            reload_lock: tokio::sync::Mutex::new(()),
            sync: OnceLock::new(),
            login_ttl,
            instance,
        });

        // Subscribe before the first reload, so a write landing between the
        // two is delivered rather than waited out by the revision poll.
        let handle = sync::prepare(&inner.cache, sync_mode).await;
        let _ = inner.sync.set(handle);
        if initial_reload {
            inner.reload().await?;
        }
        sync::spawn(&inner, sync_mode, poll_interval);
        Ok(Gproxy(inner))
    }
}

/// Explicit channels first: a host that registers its own `codex` replaces the
/// compiled-in one instead of colliding with it.
fn registry(explicit: Vec<Arc<dyn BaseChannel>>, defaults: bool) -> SdkResult<ChannelRegistry> {
    let mut registry = ChannelRegistry::new();
    for channel in explicit {
        registry
            .register(channel)
            .map_err(|error| SdkError::conflict(error.to_string()))?;
    }
    if defaults {
        for channel in default_channels() {
            if registry.get(channel.id()).is_none() {
                registry
                    .register(channel)
                    .map_err(|error| SdkError::conflict(error.to_string()))?;
            }
        }
    }
    Ok(registry)
}

/// The channels this build compiled in, one per Cargo feature.
fn default_channels() -> Vec<Arc<dyn BaseChannel>> {
    // A build with no channel feature yields an empty list, which is
    // legitimate: the host registers its own.
    vec![
        #[cfg(feature = "custom")]
        (Arc::new(gproxy_channel::channels::custom::Custom) as Arc<dyn BaseChannel>),
        #[cfg(feature = "codex")]
        (Arc::new(gproxy_channel::channels::codex::Codex) as Arc<dyn BaseChannel>),
        #[cfg(feature = "claudecode")]
        (Arc::new(gproxy_channel::channels::claudecode::Claudecode) as Arc<dyn BaseChannel>),
        #[cfg(feature = "claudeweb")]
        (Arc::new(gproxy_channel::channels::claudeweb::ClaudeWeb::new()) as Arc<dyn BaseChannel>),
    ]
}

/// One instance in one process shares state through memory; an isolate that
/// shares nothing with the next one shares it through the database instead.
#[cfg(not(target_arch = "wasm32"))]
fn default_cache<C>(_store: &Arc<Store<C>>) -> SdkResult<Arc<dyn Cache>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    #[cfg(feature = "memory")]
    {
        Ok(Arc::new(gproxy_cache::MemoryCache::default()))
    }
    #[cfg(not(feature = "memory"))]
    {
        Err(SdkError::invalid(
            "a cache is required: enable the `memory` feature or call cache()",
        ))
    }
}

#[cfg(target_arch = "wasm32")]
fn default_cache<C>(store: &Arc<Store<C>>) -> SdkResult<Arc<dyn Cache>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    Ok(Arc::new(gproxy_store::StoreCache::new(store.clone())))
}
