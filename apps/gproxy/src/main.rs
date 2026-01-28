use std::error::Error;
use std::sync::{Arc, RwLock};

use clap::Parser;
mod admin;
mod cli;
use gproxy_core::{AuthProvider, Core, MemoryAuth, ProviderLookup};
use gproxy_provider_impl::build_registry;
mod snapshot;
use gproxy_storage::TrafficStorage;
use time::OffsetDateTime;
use tracing::info;

use crate::cli::{Cli, GlobalConfig};
use crate::admin::admin_router;

#[tokio::main]
async fn main() {
    init_tracing();
    if let Err(err) = run().await {
        eprintln!("gproxy failed: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cli = Cli::parse();
    let dsn = resolve_dsn(&cli.dsn)?;
    let storage = TrafficStorage::connect(&dsn).await?;
    info!(dsn = %dsn, "db connected");
    storage.sync().await?;

    let snapshot = storage.load_snapshot().await?;

    let config = if let Some(config_row) = snapshot.global_config.as_ref() {
        serde_json::from_value(config_row.config_json.clone())?
    } else {
        let config = GlobalConfig {
            host: cli.host.clone(),
            port: cli.port,
            admin_key: cli.admin_key.clone(),
            dsn: dsn.clone(),
            proxy: cli.proxy.clone(),
        };
        let config_json = serde_json::to_value(&config)?;
        storage
            .upsert_global_config(1, config_json, OffsetDateTime::now_utc())
            .await?;
        config
    };
    info!(
        host = %config.host,
        port = config.port,
        admin_key = %config.admin_key,
        dsn = %config.dsn,
        proxy = %config.proxy.as_deref().unwrap_or(""),
        "config loaded"
    );

    storage.ensure_admin_user(&config.admin_key).await?;
    info!("admin user ensured");

    let snapshot = storage.load_snapshot().await?;
    info!(
        providers = snapshot.providers.len(),
        credentials = snapshot.credentials.len(),
        disallow = snapshot.disallow.len(),
        users = snapshot.users.len(),
        api_keys = snapshot.api_keys.len(),
        "snapshot loaded"
    );
    let auth_snapshot = snapshot::build_auth_snapshot(&snapshot);
    let auth = Arc::new(MemoryAuth::new(auth_snapshot));
    let auth_provider: Arc<dyn AuthProvider> = auth.clone();

    let registry = Arc::new(build_registry());
    let pools = snapshot::build_provider_pools(&snapshot);
    for (name, pool) in &pools {
        let total = pool.credentials.len();
        let enabled = pool.credentials.iter().filter(|cred| cred.enabled).count();
        info!(provider = %name, credentials_total = total, credentials_enabled = enabled, "pool ready");
    }
    registry.apply_pools(pools);

    let bind = format!("{}:{}", config.host, config.port);
    let (bind_tx, bind_rx) = tokio::sync::watch::channel(bind);

    let proxy = Arc::new(RwLock::new(config.proxy.clone()));

    let lookup: ProviderLookup = {
        let registry = registry.clone();
        Arc::new(move |name| registry.get(name))
    };

    let core = Core::new(lookup, auth_provider, proxy.clone());
    let app = core
        .router()
        .merge(admin_router(
            config.admin_key.clone(),
            config.dsn.clone(),
            config.clone(),
            storage.clone(),
            bind_tx.clone(),
            proxy.clone(),
            registry.clone(),
            auth,
        ));

    serve_loop(app, bind_rx).await?;

    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("gproxy=info,sqlx=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

pub(crate) fn resolve_dsn(input: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    if !input.trim().is_empty() {
        return Ok(input.to_string());
    }

    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or("failed to resolve executable directory")?;
    let db_path = dir.join("gproxy.db");
    let db_path = db_path.to_string_lossy();
    let dsn = if db_path.starts_with('/') {
        let trimmed = db_path.trim_start_matches('/');
        format!("sqlite:///{}", trimmed)
    } else {
        format!("sqlite://{}", db_path)
    };
    Ok(dsn)
}

async fn serve_loop(
    app: axum::Router,
    bind_rx: tokio::sync::watch::Receiver<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut current = bind_rx.borrow().clone();
    loop {
        let listener = tokio::net::TcpListener::bind(&current).await?;
        info!(addr = %current, "listening");
        let mut shutdown_rx = bind_rx.clone();
        let shutdown_addr = current.clone();
        let shutdown = async move {
            loop {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
                if *shutdown_rx.borrow() != shutdown_addr {
                    break;
                }
            }
        };
        axum::serve(listener, app.clone())
            .with_graceful_shutdown(shutdown)
            .await?;

        let next = bind_rx.borrow().clone();
        if next == current {
            break;
        }
        current = next;
    }

    Ok(())
}
