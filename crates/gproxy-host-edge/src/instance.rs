//! Assembling one isolate, and keeping it in step with the others.
//!
//! # An isolate is not a process
//!
//! The native binary assembles once, spawns a background task that subscribes
//! to invalidations and polls for revisions, and serves for days. None of that
//! is available here. A Worker isolate is created when there is traffic and
//! destroyed when there is not, it may serve one request or ten thousand, and
//! nothing runs in it between requests — a spawned loop is simply not polled.
//!
//! So synchronization is manual, and it is one line: [`Instance::tick`] at the
//! top of every request. The sdk says so in its own README, and [`SyncMode::Manual`]
//! is the builder switch that stops it from spawning a loop that would never
//! run. A `tick` is one read of `settings.config_revision`; when the revision
//! it finds is newer than the one this isolate published, it reloads both
//! snapshots. An isolate that has just been created has nothing to catch up on
//! and an isolate that has been alive for an hour catches up on the hour.
//!
//! # Why the assembly is cached at all
//!
//! Because the alternative is a full reload — every provider, credential,
//! route and price — on every request, and a Worker pays for that in wall
//! clock on the request that triggers it. The [`OnceLock`] holds it for the
//! isolate's whole life.
//!
//! Two requests arriving before the first assembly finishes will both assemble
//! (there is no async `OnceLock`, and a Worker has no threads to block). The
//! loser's instance is dropped, which costs one wasted cold start and nothing
//! else — no connection is held open, because D1 and libSQL are both
//! request-scoped HTTP.

use std::sync::{Arc, OnceLock};

use axum::Router;
use gproxy_app::{App, AppConfig, AppPublicationUrl, config::StoreBackendConfig};
use gproxy_sdk::{Gproxy, GproxyBuilder, SyncMode};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Store, StoreCache, entity::config::setting};
use worker::Env;

use crate::config;

/// Everything one isolate holds: the product layer, and the router over it.
///
/// The router is built once rather than per request. `Router::clone` is cheap
/// — the match tree is behind an `Arc` — which is what lets a `&Instance` hand
/// out a callable service without a lock.
pub struct Instance<C> {
    app: Arc<App<C>>,
    router: Router,
}

impl<C> Instance<C>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// One step of synchronization, at the top of a request.
    ///
    /// A failure is logged and swallowed on purpose. `tick` is a *catch-up*:
    /// if the revision cannot be read right now, this isolate serves the
    /// configuration it already has, which is a previous revision rather than
    /// a wrong one. Refusing the request instead would turn a momentary
    /// database hiccup into an outage for traffic that did not need the write
    /// that has not been seen yet.
    pub async fn tick(&self) {
        if let Err(error) = self.app.gproxy().tick().await {
            worker::console_warn!(
                "configuration sync failed, serving the loaded revision: {error}"
            );
        }
    }
}

/// The assembled backends, one variant per store a Worker can reach.
///
/// An enum rather than two statics because the two are alternatives, not
/// options: a deployment configures one store, and the isolate holds exactly
/// what that store produced. `App<C>` is generic over the connection, so the
/// concrete type has to be settled here and nowhere else.
pub enum Assembled {
    #[cfg(feature = "d1")]
    D1(Instance<gproxy_seaorm::D1Connection>),
    #[cfg(feature = "libsql")]
    Libsql(Instance<gproxy_seaorm::LibsqlConnection>),
}

impl Assembled {
    pub fn router(&self) -> Router {
        match self {
            #[cfg(feature = "d1")]
            Self::D1(instance) => instance.router(),
            #[cfg(feature = "libsql")]
            Self::Libsql(instance) => instance.router(),
        }
    }

    pub async fn tick(&self) {
        match self {
            #[cfg(feature = "d1")]
            Self::D1(instance) => instance.tick().await,
            #[cfg(feature = "libsql")]
            Self::Libsql(instance) => instance.tick().await,
        }
    }
}

static INSTANCE: OnceLock<Assembled> = OnceLock::new();

/// The isolate's instance, assembling it on the first request that needs one.
pub async fn instance(env: &Env) -> Result<&'static Assembled, worker::Error> {
    if let Some(assembled) = INSTANCE.get() {
        return Ok(assembled);
    }
    let assembled = assemble(env).await?;
    Ok(INSTANCE.get_or_init(|| assembled))
}

async fn assemble(env: &Env) -> Result<Assembled, worker::Error> {
    let config = config::resolve(
        binding(env, config::binding::CONFIG).as_deref(),
        &config::Secrets {
            master_key: binding(env, config::binding::MASTER_KEY),
            libsql_token: binding(env, config::binding::LIBSQL_TOKEN),
            s3_access_key_id: binding(env, config::binding::S3_ACCESS_KEY_ID),
            s3_secret_access_key: binding(env, config::binding::S3_SECRET_ACCESS_KEY),
        },
    )
    .map_err(|error| worker::Error::RustError(error.to_string()))?;

    if config.master_key.key == gproxy_app::config::MasterKey::None {
        worker::console_warn!(
            "no {} is set; upstream credential secrets are stored in plaintext",
            config::binding::MASTER_KEY
        );
    }

    match &config.store {
        #[cfg(feature = "d1")]
        StoreBackendConfig::D1 { binding: name } => {
            let value: wasm_bindgen::JsValue = js_sys::Reflect::get(
                env.as_ref(),
                &wasm_bindgen::JsValue::from_str(name.as_str()),
            )
            .map_err(|_| error(format!("the Worker has no binding named `{name}`")))?;
            if value.is_undefined() || value.is_null() {
                return Err(error(format!(
                    "the Worker has no binding named `{name}`; add a `[[d1_databases]]` entry \
                     with that binding to wrangler.toml"
                )));
            }
            let connection = gproxy_seaorm::D1Connection::from_binding(value)
                .map_err(|e| error(format!("the `{name}` binding is not a D1 database: {e}")))?;
            Ok(Assembled::D1(build(connection, config).await?))
        }
        #[cfg(feature = "libsql")]
        StoreBackendConfig::Libsql { url, token } => {
            // The database goes out over the same fetch transport as every
            // upstream call, which is the only HTTP a Worker has.
            let transport: Arc<dyn gproxy_seaorm::LibsqlTransport> =
                Arc::new(gproxy_client::Client::Fetch(
                    gproxy_client::FetchClient::new()
                        .map_err(|e| error(format!("fetch transport: {e}")))?,
                ));
            let connection = gproxy_seaorm::LibsqlConnection::new(transport, url, token.as_deref())
                .map_err(|e| error(format!("libSQL: {e}")))?;
            Ok(Assembled::Libsql(build(connection, config).await?))
        }
        other => Err(error(format!(
            "this build cannot open the configured store ({other:?}); rebuild with the matching \
             feature"
        ))),
    }
}

/// The part of assembly that does not depend on which store it is.
///
/// The order mirrors the native host's `instance::open`, minus the steps that
/// need a filesystem: there is no directory to create, no schema to
/// synchronize on the request path (a Worker must not run DDL under traffic —
/// `wrangler d1 migrations apply` does that before the deployment), and no
/// key rotation (a one-shot management operation, not something to do on the
/// first request of every cold start).
async fn build<C>(connection: C, config: AppConfig) -> Result<Instance<C>, worker::Error>
where
    C: BatchConnectionTrait + Clone + Send + Sync + 'static,
{
    // One `Store` over a clone of the connection, for the cache — which has to
    // exist before the handle does, because the handle takes it.
    let store = Arc::new(Store::new(connection.clone()));
    // The settings row is a precondition for every write, and for `tick`,
    // which reads `config_revision` off it. Creating it is idempotent.
    store
        .settings()
        .update(setting::ActiveModel::default())
        .await
        .map_err(|e| error(format!("the settings row could not be read: {e}")))?;

    let cache = cache(&config, store)?;

    let mut builder = GproxyBuilder::connection(connection)
        // There is no task to run a loop in. `tick` is the whole mechanism.
        .sync_mode(SyncMode::Manual)
        // `App::reload_all` below reads both layers from one revision; letting
        // the builder reload as well would read the configuration twice on
        // every cold start, which a Worker pays for in wall clock.
        .initial_reload(false)
        .cache(cache);
    builder = match config
        .master_key
        .resolve()
        .map_err(|e| error(e.to_string()))?
    {
        Some(key) => builder.master_key(key),
        None => builder.plaintext_secrets(),
    };
    if let Some(files) = file_storage(&config)? {
        builder = builder.file_storage(files);
    }
    let publications = AppPublicationUrl::from_config(&config);
    let publishable = publications.is_configured();
    builder = builder.publication_url(Arc::new(publications));

    let gproxy: Gproxy<C> = builder
        // Not `build`: that synchronizes the schema, and a Worker must not run
        // DDL under traffic. D1's schema is planned by
        // `wrangler d1 migrations apply` before the deployment, and libSQL's
        // by whoever owns the database — which is what `build_unsynced` is
        // documented for.
        .build_unsynced()
        .await
        .map_err(|e| error(format!("the instance could not be assembled: {e}")))?;
    let app = Arc::new(App::new(gproxy, config));
    let revision = app
        .reload_all()
        .await
        .map_err(|e| error(format!("the first configuration load failed: {e}")))?;

    if !publishable {
        worker::console_log!(
            "no public base URL is configured; an upstream that needs a fetchable link will be \
             given the bytes inline instead"
        );
    }
    worker::console_log!("gproxy isolate assembled at revision {revision}");

    let router = gproxy_host_axum::router(gproxy_host_axum::HostState::new(Arc::clone(&app)));
    Ok(Instance { app, router })
}

/// Shared transient state. `Memory` never reaches here — [`config::resolve`]
/// refuses it, because an isolate's memory is not shared with anything.
fn cache<C>(
    config: &AppConfig,
    store: Arc<Store<C>>,
) -> Result<Arc<dyn gproxy_sdk::Cache>, worker::Error>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    match &config.cache {
        gproxy_app::config::CacheBackendConfig::Store => Ok(Arc::new(StoreCache::new(store))),
        gproxy_app::config::CacheBackendConfig::Redis { .. } => Err(error(
            "this build has no Redis client; a Worker reaches Redis over HTTP and that backend is \
             not compiled in — use the database-backed cache",
        )),
        gproxy_app::config::CacheBackendConfig::Memory => Err(error(
            "an in-memory cache cannot be shared between Worker isolates",
        )),
    }
}

/// Object storage for published bodies and downloaded vocabularies. There is
/// no filesystem at the edge, so S3 — and therefore R2, which speaks it — is
/// the only one.
fn file_storage(config: &AppConfig) -> Result<Option<gproxy_file::Operator>, worker::Error> {
    let Some(storage) = config.file_storage.as_ref() else {
        return Ok(None);
    };
    match storage {
        gproxy_app::config::FileStorageConfig::S3 {
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
                Ok(Some(
                    gproxy_file::s3(builder).map_err(|e| error(format!("file storage: {e}")))?,
                ))
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
                Err(error(
                    "this build has no object storage; rebuild with the `s3` feature",
                ))
            }
        }
        gproxy_app::config::FileStorageConfig::Fs { .. } => Err(error(
            "a Worker has no filesystem; configure S3 or R2 storage instead",
        )),
    }
}

/// One binding, as a string. Reads a secret and a `[vars]` entry the same way,
/// which is what lets an operator move a value from one to the other without
/// changing anything here.
fn binding(env: &Env, name: &str) -> Option<String> {
    env.secret(name)
        .ok()
        .map(|secret| secret.to_string())
        .or_else(|| env.var(name).ok().map(|var| var.to_string()))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn error(message: impl Into<String>) -> worker::Error {
    worker::Error::RustError(message.into())
}
