//! Turning an [`AppConfig`] into a running instance.
//!
//! This is the one place that translates configuration into backends: a
//! connection, a cache, object storage, the secret codec, and then the sdk
//! handle and the [`App`] over it. Everything it decides, it decides from the
//! `AppConfig` it was given — there is no second configuration source below
//! this line.
//!
//! # The order matters
//!
//! 1. resolve paths against `data_dir` and create the directories;
//! 2. open the connection;
//! 3. synchronize the schema and make sure the settings row exists;
//! 4. **rotate the master key, if asked** — before the handle exists, because
//!    the handle's codec is fixed at assembly and the rotation decides which key
//!    that is;
//! 5. assemble the handle with the key that is now in force;
//! 6. load the first snapshot ([`App::reload_all`]);
//! 7. start synchronization ([`App::start_sync`]), which is what keeps both
//!    snapshots in step with the other instances afterwards.
//!
//! Step 4 is why this is a function rather than a builder chain: which codec the
//! handle gets depends on work done against the same database a moment earlier.

use std::{path::PathBuf, sync::Arc};

use gproxy_app::{
    App, AppConfig, AppPublicationUrl,
    config::{CacheBackendConfig, FileStorageConfig, StoreBackendConfig},
};
use gproxy_sdk::{Cache, Gproxy, GproxyBuilder, SyncMode};
use gproxy_store::{Store, entity::config::setting};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::{Error, Result, Settings, rotate};

pub type Connection = DatabaseConnection;
pub type Handle = Gproxy<Connection>;

/// One assembled instance.
pub struct Instance {
    pub app: Arc<App<Connection>>,
    /// The revision the first snapshot was loaded at, for the startup log line.
    pub revision: i64,
    /// What the master-key configuration actually did, so the caller can warn
    /// about plaintext storage and about a rotation that now needs promoting.
    pub secrets: rotate::Outcome,
}

// Manual, and deliberately thin: `App`'s own `Debug` is
// `finish_non_exhaustive`, and the secrets outcome holds key material that must
// never reach a log line or a test failure message.
impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("revision", &self.revision)
            .field("plaintext_secrets", &self.secrets.is_plaintext())
            .field("rotated", &self.secrets.rotated)
            .finish_non_exhaustive()
    }
}

/// How an instance is assembled for a particular command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenOptions {
    /// Poll and subscribe for configuration changes made by other instances —
    /// for both snapshots: the engine's, through the sdk's loops, and
    /// identity's, through [`App::start_sync`]. Only a server wants this; a
    /// one-shot command would be spawning tasks it immediately shuts down
    /// again, and refreshes explicitly instead.
    pub sync_mode: SyncMode,
}

impl OpenOptions {
    /// A long-running server: keep in step with the other instances.
    pub fn serving() -> Self {
        Self {
            sync_mode: SyncMode::Background,
        }
    }

    /// A one-shot command: read, write, exit.
    pub fn management() -> Self {
        Self {
            sync_mode: SyncMode::Manual,
        }
    }
}

/// Create or incrementally synchronize the schema, and nothing else.
///
/// Deliberately does not assemble a handle: a deployment that runs this as its
/// migration step wants one writer touching DDL and no instance loading a
/// snapshot out of a half-migrated database.
pub async fn migrate(config: &AppConfig) -> Result<()> {
    let store = Store::new(connect(config).await?);
    let report = store.sync().await?;
    // The settings row is a precondition for every write that follows, and
    // creating it here is what lets `serve` start against this database without
    // doing any DDL of its own.
    store
        .settings()
        .update(setting::ActiveModel::default())
        .await?;
    for warning in &report.warnings {
        tracing::warn!(warning, "schema synchronization");
    }
    tracing::info!(warnings = report.warnings.len(), "schema is up to date");
    Ok(())
}

/// Assemble the engine and the product layer over one database.
pub async fn open(settings: &Settings, options: OpenOptions) -> Result<Instance> {
    let config = &settings.config;
    let connection = connect(config).await?;

    // One `Store` over a clone of the connection, for the two things that have
    // to happen before a handle can exist. `DatabaseConnection` is a handle to
    // the same pool, so this is not a second connection.
    let store = Store::new(connection.clone());
    store.sync().await?;
    store
        .settings()
        .update(setting::ActiveModel::default())
        .await?;

    let secrets = rotate::run(&store, &config.master_key).await?;

    let mut builder = GproxyBuilder::connection(connection)
        // Already done above, together with the settings row, so the builder
        // must not repeat the DDL.
        .sync_schema(false)
        .sync_mode(options.sync_mode)
        // The app loads the first snapshot itself through `reload_all`, which
        // refreshes both layers from one revision. Letting the builder reload
        // as well would read the configuration twice for no gain.
        .initial_reload(false)
        .cache(cache(config).await?);
    builder = match secrets.active_key {
        Some(key) => builder.master_key(key),
        None => builder.plaintext_secrets(),
    };
    if let Some(files) = file_storage(config)? {
        builder = builder.file_storage(files);
    }
    if let Some(instance_id) = settings.instance_id.as_deref() {
        builder = builder.instance_id(instance_id);
    }
    // The publication builder names this host's own download route, so a
    // `PublicationKind::Url` resolves to `{public_base_url}/publications/{id}`
    // — the route `gproxy-host-axum` actually mounts.
    let publications = AppPublicationUrl::from_config(config);
    let publishable = publications.is_configured();
    builder = builder.publication_url(Arc::new(publications));

    let gproxy = builder.build().await?;
    let app = Arc::new(App::new(gproxy, config.clone()));
    let revision = app.reload_all().await?;
    // After the first load, never before: a fresh subscription opens with
    // `ResyncRequired`, and that load is the resync. From here the instance
    // keeps itself in step with whoever else writes to this database — and,
    // in `Background`, with its own `/admin/api` writes between polls.
    app.start_sync(options.sync_mode, gproxy_app::DEFAULT_POLL_INTERVAL)
        .await;

    if !publishable {
        tracing::debug!(
            "no public base URL is configured; an upstream that needs a fetchable link will be \
             given the bytes inline instead"
        );
    }

    Ok(Instance {
        app,
        revision,
        secrets,
    })
}

/// Open the connection the configuration names, creating a SQLite file and its
/// directory when that is what it names.
///
/// The connection is opened here rather than through `GproxyBuilder::sqlite`
/// because the rotation step needs a `Store` over it *before* a handle exists,
/// and the builder does not give its connection back.
///
/// # A database this build does not own is refused here
///
/// Every command that touches a database comes through this function, so this
/// is where a v3 file is recognised and refused — see [`crate::v3::detect`].
/// Putting it in a command would mean `serve` was safe and `migrate` was not,
/// and `migrate` is the one that runs the DDL. The check is `SELECT`-only and
/// happens before any caller can reach `Store::sync`.
pub async fn connect(config: &AppConfig) -> Result<Connection> {
    let connection = open_connection(config).await?;
    if let crate::v3::detect::Verdict::Legacy { tells } =
        crate::v3::detect::inspect(&connection).await
    {
        return Err(crate::v3::detect::refuse(&describe(config), &tells));
    }
    Ok(connection)
}

/// What the refusal calls the database, so the message names the file an
/// operator has to move rather than the flag that found it.
fn describe(config: &AppConfig) -> String {
    match &config.store {
        StoreBackendConfig::Sqlite { path } => resolve(config, path).display().to_string(),
        // Never the DSN itself: it carries a password.
        StoreBackendConfig::Url { .. } => "the configured database".to_owned(),
        StoreBackendConfig::D1 { binding } => format!("the D1 binding `{binding}`"),
        StoreBackendConfig::Libsql { .. } => "the configured libSQL database".to_owned(),
    }
}

async fn open_connection(config: &AppConfig) -> Result<Connection> {
    match &config.store {
        StoreBackendConfig::Sqlite { path } => {
            let path = resolve(config, path);
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)
                    .map_err(|error| Error::io(format!("creating {}", parent.display()), error))?;
            }
            let mut options =
                ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.to_string_lossy()));
            // min = max = 1 is not tuning: SQLite has one writer, and a larger
            // pool turns a serialized write into a `database is locked` race
            // without making any read faster. It is also what the sdk's own
            // `sqlite()` does, so both paths behave identically.
            options
                .min_connections(1)
                .max_connections(1)
                .sqlx_logging(false);
            Ok(Database::connect(options).await?)
        }
        StoreBackendConfig::Url { dsn } => {
            let mut options = ConnectOptions::new(dsn.clone());
            options.sqlx_logging(false);
            Ok(Database::connect(options).await?)
        }
        // Both exist for the edge host, which is handed its binding by the
        // runtime rather than opening anything.
        StoreBackendConfig::D1 { .. } => Err(Error::other(
            "a D1 binding exists only inside a Cloudflare Worker; the native binary cannot open one",
        )),
        StoreBackendConfig::Libsql { .. } => Err(Error::other(
            "this build does not open libSQL; use a `postgres://`, `mysql://` or SQLite store",
        )),
    }
}

/// Shared transient state: TTL entries, rate-limit counters, concurrency
/// permits and the invalidation topic.
async fn cache(config: &AppConfig) -> Result<Arc<dyn Cache>> {
    match &config.cache {
        CacheBackendConfig::Memory => {
            #[cfg(feature = "memory")]
            {
                Ok(Arc::new(gproxy_cache::MemoryCache::default()))
            }
            #[cfg(not(feature = "memory"))]
            {
                Err(Error::other(
                    "this build has no in-process cache; rebuild with the `memory` feature or \
                     configure Redis",
                ))
            }
        }
        CacheBackendConfig::Redis { url, namespace } => {
            #[cfg(feature = "redis")]
            {
                // Connecting at startup is deliberate. Admission refuses a
                // request whose rate limit it cannot count, so an instance that
                // cannot reach its cache must fail here rather than accept
                // traffic it will reject one request at a time.
                let options = gproxy_cache::RedisOptions::new(
                    namespace.as_deref().unwrap_or(DEFAULT_CACHE_NAMESPACE),
                );
                let cache = gproxy_cache::RedisCache::connect(url.as_str(), options)
                    .await
                    .map_err(|error| Error::other(format!("cache: {error}")))?;
                Ok(Arc::new(cache))
            }
            #[cfg(not(feature = "redis"))]
            {
                let _ = (url, namespace);
                Err(Error::other(
                    "this build has no Redis cache; rebuild with the `redis` feature",
                ))
            }
        }
        CacheBackendConfig::Store => Err(Error::other(
            "the database-backed cache exists for the edge host, where there is no Redis and no \
             process to share memory with; a native instance uses `memory` or `redis`",
        )),
    }
}

/// The Redis key prefix when the configuration does not name one. Distinct
/// deployments sharing one Redis must name distinct namespaces; this default is
/// only correct for a single deployment.
#[cfg(feature = "redis")]
const DEFAULT_CACHE_NAMESPACE: &str = "gproxy";

/// Object storage for published bodies and downloaded vocabularies. `None`
/// leaves both disabled, and core refuses those operations rather than
/// inventing a location.
fn file_storage(config: &AppConfig) -> Result<Option<gproxy_file::Operator>> {
    let Some(storage) = config.file_storage.as_ref() else {
        return Ok(None);
    };
    match storage {
        FileStorageConfig::Fs { root } => {
            #[cfg(feature = "fs")]
            {
                let root = resolve(config, root);
                std::fs::create_dir_all(&root)
                    .map_err(|error| Error::io(format!("creating {}", root.display()), error))?;
                Ok(Some(
                    gproxy_file::filesystem(&root.to_string_lossy())
                        .map_err(|error| Error::other(format!("file storage: {error}")))?,
                ))
            }
            #[cfg(not(feature = "fs"))]
            {
                let _ = root;
                Err(Error::other(
                    "this build has no filesystem storage; rebuild with the `fs` feature",
                ))
            }
        }
        FileStorageConfig::S3 {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            root,
        } => {
            #[cfg(feature = "s3")]
            {
                let mut builder = gproxy_file::S3::default().bucket(bucket);
                if let Some(region) = region {
                    builder = builder.region(region);
                }
                if let Some(endpoint) = endpoint {
                    builder = builder.endpoint(endpoint);
                }
                if let Some(key) = access_key_id {
                    builder = builder.access_key_id(key);
                }
                if let Some(secret) = secret_access_key {
                    builder = builder.secret_access_key(secret);
                }
                if let Some(root) = root {
                    builder = builder.root(root);
                }
                Ok(Some(gproxy_file::s3(builder).map_err(|error| {
                    Error::other(format!("file storage: {error}"))
                })?))
            }
            #[cfg(not(feature = "s3"))]
            {
                let _ = (
                    bucket,
                    region,
                    endpoint,
                    access_key_id,
                    secret_access_key,
                    root,
                );
                Err(Error::other(
                    "this build has no S3 storage; rebuild with the `s3` feature",
                ))
            }
        }
    }
}

/// A relative path, joined to `data_dir`. An absolute path is left alone, so an
/// operator can put the database somewhere other than the state directory
/// without giving up the state directory.
fn resolve(config: &AppConfig, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        return path;
    }
    match config.data_dir.as_deref() {
        Some(root) => PathBuf::from(root).join(path),
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AppConfig {
        AppConfig {
            data_dir: Some("/var/lib/gproxy".into()),
            ..AppConfig::default()
        }
    }

    #[test]
    fn a_relative_path_lands_in_the_data_directory() {
        assert_eq!(
            resolve(&config(), "gproxy.db"),
            PathBuf::from("/var/lib/gproxy/gproxy.db")
        );
    }

    #[test]
    fn an_absolute_path_is_left_where_it_was_put() {
        assert_eq!(
            resolve(&config(), "/srv/db/gproxy.db"),
            PathBuf::from("/srv/db/gproxy.db")
        );
    }

    #[tokio::test]
    async fn the_edge_only_backends_are_refused_with_an_explanation() {
        for store in [
            StoreBackendConfig::D1 {
                binding: "DB".into(),
            },
            StoreBackendConfig::Libsql {
                url: "libsql://x".into(),
                token: None,
            },
        ] {
            let config = AppConfig {
                store,
                ..AppConfig::default()
            };
            let error = connect(&config).await.unwrap_err();
            assert!(matches!(error, Error::Other(_)), "{error}");
        }
    }

    #[tokio::test]
    async fn the_database_backed_cache_is_refused_natively() {
        let config = AppConfig {
            cache: CacheBackendConfig::Store,
            ..AppConfig::default()
        };
        assert!(cache(&config).await.is_err());
    }

    #[test]
    fn no_file_storage_configured_is_not_an_error() {
        assert!(file_storage(&AppConfig::default()).unwrap().is_none());
    }

    #[test]
    fn local_file_storage_is_created_under_the_data_directory() {
        let temporary = tempfile::tempdir().unwrap();
        let config = AppConfig {
            data_dir: Some(temporary.path().to_string_lossy().into_owned()),
            file_storage: Some(FileStorageConfig::Fs {
                root: "files".into(),
            }),
            ..AppConfig::default()
        };
        assert!(file_storage(&config).unwrap().is_some());
        assert!(temporary.path().join("files").is_dir());
    }
}
