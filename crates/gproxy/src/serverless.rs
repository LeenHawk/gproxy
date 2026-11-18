//! Native host for function runtimes. A small JS entry owns the public request;
//! this process binds loopback and keeps the normal axum routes and streaming.
//! All durable state and the shared cache live in PostgreSQL, never in /tmp.

use std::{sync::Arc, time::Duration};

use axum::{extract::State, middleware::Next, response::Response};
use gproxy_app::{App, AppPublicationUrl, config::StoreBackendConfig};
use gproxy_sdk::{GproxyBuilder, SyncMode};
use gproxy_seaorm::PostgresSchemaConnection;
use gproxy_store::{Store, StoreCache};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, TransactionTrait};

use crate::{Error, Result, Settings};

type Application = Arc<App<PostgresSchemaConnection>>;

pub async fn run(mut settings: Settings) -> Result<()> {
    let StoreBackendConfig::Url { dsn } = &settings.config.store else {
        return Err(Error::other(
            "serverless deployment requires a PostgreSQL database",
        ));
    };
    if !dsn.starts_with("postgres://") && !dsn.starts_with("postgresql://") {
        return Err(Error::other(
            "serverless deployment requires a PostgreSQL database",
        ));
    }
    let password = settings
        .admin
        .password
        .as_deref()
        .ok_or_else(|| Error::other("set GPROXY_ADMIN_PASSWORD before deploying"))?;
    // Optional, as for the CLI. Rotation is not: a function runtime cannot
    // guarantee the single running instance a rotation needs.
    let key = settings.config.master_key.resolve()?;
    if key.is_none() {
        tracing::warn!(
            "{} Set GPROXY_MASTER_KEY to 32 bytes as 64 hex characters or base64 before adding \
             upstream credentials.",
            crate::rotate::PLAINTEXT_SECRETS
        );
    }
    // The process is private to its JS entry. The entry overwrites forwarded
    // headers from the platform's trusted client IP and the public request URL.
    settings.config.host = "127.0.0.1".into();
    settings.config.port = 0;
    settings.config.cache = gproxy_app::config::CacheBackendConfig::Store;
    settings.config.trusted_proxies = vec!["127.0.0.1".into()];
    settings.config.file_storage = None;
    // Optional fonts use writable temporary storage; instance data stays in the database.
    settings.config.data_dir = Some(std::env::temp_dir().join("gproxy").to_string_lossy().into());

    let mut options = ConnectOptions::new(dsn.clone());
    options
        .min_connections(0)
        .max_connections(4)
        .connect_timeout(Duration::from_secs(10))
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(30))
        .sqlx_logging(false);
    let connection = Database::connect(options).await?;
    // Serialize cold starts, including first-admin creation. The transaction
    // owns only the advisory lock; schema/data operations use the other pool
    // connections. A terminated function releases the lock on disconnect.
    let lock = connection.begin().await?;
    lock.execute_unprepared("SET LOCAL lock_timeout = '20s'")
        .await?;
    lock.execute_unprepared("SELECT pg_advisory_xact_lock(1735422578, 1)")
        .await?;
    // Keep platform migration ledgers out of the application's schema census.
    connection
        .execute_unprepared("CREATE SCHEMA IF NOT EXISTS gproxy")
        .await?;
    let connection = PostgresSchemaConnection::new(connection, "gproxy")?;
    let store = Arc::new(Store::new(connection.clone()));
    crate::instance::prepare(&store, &settings.config).await?;
    let publications = AppPublicationUrl::from_config(&settings.config);
    let builder = GproxyBuilder::connection(connection.clone())
        .sync_schema(false)
        .initial_reload(false)
        .sync_mode(SyncMode::Manual);
    let builder = match key {
        Some(key) => builder.master_key(key),
        None => builder.plaintext_secrets(),
    };
    let gateway = builder
        .cache(Arc::new(StoreCache::new(store)))
        .publication_url(Arc::new(publications))
        .build()
        .await?;
    let app = Arc::new(App::new(gateway, settings.config));
    let data = app.data();
    gproxy_app::Operations::new(app.gproxy(), &data, app.config())
        .users()
        .bootstrap_admin(&settings.admin.user, Some(password))
        .await?;
    app.reload_all().await?;
    app.start_sync(SyncMode::Manual, gproxy_app::DEFAULT_POLL_INTERVAL)
        .await;
    lock.commit().await?;

    let router = gproxy_host_axum::router(gproxy_host_axum::HostState::new(app.clone())).layer(
        axum::middleware::from_fn_with_state(app.clone(), synchronize),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| Error::io("binding the serverless listener", error))?;
    let address = listener
        .local_addr()
        .map_err(|error| Error::io("reading the serverless listener", error))?;
    // A machine-readable readiness line, emitted only after schema and auth
    // initialization. Never print a generated password or API key to logs.
    println!("GPROXY_READY http://{address}");
    let result = gproxy_host_axum::serve::serve(listener, router, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await;
    app.shutdown();
    result.map_err(|error| Error::io("serving serverless requests", error))
}

async fn synchronize(
    State(app): State<Application>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    // A suspended function cannot keep background polling alive. Catch up
    // before admitting traffic after a warm invocation resumes.
    if let Err(error) = app.gproxy().tick().await {
        tracing::warn!(%error, "configuration synchronization failed");
    }
    if let Err(error) = app.tick().await {
        tracing::warn!(%error, "identity synchronization failed");
    }
    next.run(request).await
}
