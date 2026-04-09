use std::path::Path;
use std::sync::Arc;

use clap::{CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use gproxy_api::admin::config_toml::GproxyToml;
use gproxy_sdk::provider::engine::GproxyEngineBuilder;
use gproxy_server::{AppStateBuilder, GlobalConfig};
use gproxy_storage::{SeaOrmStorage, StorageWriteEvent};

mod web;
mod workers;

#[derive(Parser)]
#[command(name = "gproxy", about = "High-performance LLM proxy server")]
struct Cli {
    /// Listen host
    #[arg(long, env = "GPROXY_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Listen port
    #[arg(long, env = "GPROXY_PORT", default_value_t = 8787)]
    port: u16,

    /// Bootstrap admin username on first start
    #[arg(long, env = "GPROXY_ADMIN_USER", default_value = "admin")]
    admin_user: String,

    /// Bootstrap admin password on first start (generated randomly if not set)
    #[arg(long, env = "GPROXY_ADMIN_PASSWORD")]
    admin_password: Option<String>,

    /// Bootstrap admin API key on first start (generated randomly if not set)
    #[arg(long, env = "GPROXY_ADMIN_API_KEY")]
    admin_api_key: Option<String>,

    /// Database connection string (default: sqlite in data_dir)
    #[arg(long, env = "GPROXY_DSN")]
    dsn: Option<String>,

    /// Path to TOML config file for initial seeding
    #[arg(long, env = "GPROXY_CONFIG", default_value = "gproxy.toml")]
    config: String,

    /// Data directory
    #[arg(long, env = "GPROXY_DATA_DIR", default_value = "./data")]
    data_dir: String,

    /// HTTP proxy for upstream requests
    #[arg(long, env = "GPROXY_PROXY")]
    proxy: Option<String>,

    /// TLS fingerprint emulation
    #[arg(long, env = "GPROXY_SPOOF", default_value = "chrome_136")]
    spoof_emulation: String,

    /// Database encryption secret key (XChaCha20Poly1305).
    /// When set, credentials, passwords, and API keys are encrypted at rest.
    #[arg(long, env = "DATABASE_SECRET_KEY")]
    database_secret_key: Option<String>,
}

fn is_explicit(matches: &clap::ArgMatches, id: &str) -> bool {
    matches
        .value_source(id)
        .is_some_and(|source| source != ValueSource::DefaultValue)
}

fn log_generated_bootstrap_admin(
    admin_user: &str,
    outcome: &gproxy_api::bootstrap::BootstrapAdminOutcome,
) {
    if let Some(password) = outcome.generated_password.as_deref() {
        tracing::info!(
            admin_user = %admin_user,
            admin_password = %password,
            "generated bootstrap admin password (save this!)"
        );
    }
    if let Some(api_key) = outcome.generated_api_key.as_deref() {
        tracing::info!(
            admin_user = %admin_user,
            admin_api_key = %api_key,
            "generated bootstrap admin API key (save this!)"
        );
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // 2. Parse CLI
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    let admin_user_explicit = is_explicit(&matches, "admin_user");
    let admin_password_explicit = is_explicit(&matches, "admin_password");
    let admin_api_key_explicit = is_explicit(&matches, "admin_api_key");
    let has_admin_override =
        admin_user_explicit || admin_password_explicit || admin_api_key_explicit;

    // 3. Resolve DSN
    let mut dsn = cli.dsn.clone().unwrap_or_else(|| {
        let db_path = Path::new(&cli.data_dir).join("gproxy.db");
        format!("sqlite://{}?mode=rwc", db_path.display())
    });
    let mut active_data_dir = cli.data_dir.clone();

    // 4. Ensure data directory exists
    std::fs::create_dir_all(&cli.data_dir)?;

    // 5. Connect database + sync schema
    tracing::info!(dsn = %dsn, "connecting to database");
    let storage = SeaOrmStorage::connect(&dsn, cli.database_secret_key.as_deref()).await?;
    storage.sync().await?;
    let mut storage = Arc::new(storage);
    tracing::info!("database schema synced");

    let mut persisted_settings_exist = storage.get_global_settings().await?.is_some();
    if !is_explicit(&matches, "dsn")
        && !is_explicit(&matches, "data_dir")
        && let Some(settings) = storage.get_global_settings().await?
        && !settings.dsn.is_empty()
        && settings.dsn != dsn
    {
        if !settings.data_dir.is_empty() {
            std::fs::create_dir_all(&settings.data_dir)?;
        }
        tracing::info!(from = %dsn, to = %settings.dsn, "reconnecting to configured database");
        let reconnected = storage.reconnect(&settings.dsn).await?;
        reconnected.sync().await?;
        dsn = settings.dsn;
        active_data_dir = settings.data_dir;
        storage = Arc::new(reconnected);
        persisted_settings_exist = storage.get_global_settings().await?.is_some();
    }

    let config = GlobalConfig {
        host: cli.host.clone(),
        port: cli.port,
        proxy: cli.proxy.clone(),
        spoof_emulation: cli.spoof_emulation.clone(),
        update_source: "github".to_string(),
        enable_usage: true,
        enable_upstream_log: false,
        enable_upstream_log_body: false,
        enable_downstream_log: false,
        enable_downstream_log_body: false,
        dsn: dsn.clone(),
        data_dir: active_data_dir.clone(),
    };

    // 9. Create usage channel (sender goes into AppState, receiver to worker)
    let (usage_tx, usage_rx) = tokio::sync::mpsc::channel::<gproxy_storage::UsageWrite>(1024);
    let (mut worker_set, _shutdown_rx) = workers::WorkerSet::new();

    // 10. Build empty engine + AppState
    let engine = GproxyEngineBuilder::new()
        .configure_clients(config.proxy.as_deref(), Some(&config.spoof_emulation))
        .build();

    let app_builder = AppStateBuilder::new()
        .engine(engine)
        .storage(storage.clone())
        .config(config)
        .usage_tx(usage_tx);

    let state = Arc::new(app_builder.build());

    // 11. Start usage sink worker (after AppState, so it reads fresh storage)
    worker_set.register(workers::usage_sink::spawn_with_receiver(
        state.clone(),
        usage_rx,
        worker_set.subscribe(),
    ));

    // 10. Bootstrap: load from DB or seed from TOML / defaults
    let has_data = persisted_settings_exist;

    if has_data {
        tracing::info!("loading state from database");
        let counts = gproxy_api::bootstrap::reload_from_db(&state)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        tracing::info!(
            providers = counts.providers,
            users = counts.users,
            keys = counts.keys,
            models = counts.models,
            "bootstrap from database complete"
        );
        if has_admin_override {
            let outcome = gproxy_api::bootstrap::reconcile_bootstrap_admin(
                &state,
                &gproxy_api::bootstrap::BootstrapAdmin {
                    username: cli.admin_user.clone(),
                    password: cli.admin_password.clone(),
                    api_key: cli.admin_api_key.clone(),
                },
                false,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            log_generated_bootstrap_admin(&cli.admin_user, &outcome);
        }
    } else {
        let toml_path = Path::new(&cli.config);
        if toml_path.exists() {
            tracing::info!(path = %toml_path.display(), "seeding from TOML config");
            let toml_str = std::fs::read_to_string(toml_path)?;
            let toml_config: GproxyToml = toml::from_str(&toml_str)?;
            let should_bootstrap_admin = has_admin_override
                || !gproxy_api::bootstrap::config_has_enabled_admin_with_key(&toml_config);
            let bootstrap_admin =
                should_bootstrap_admin.then(|| gproxy_api::bootstrap::BootstrapAdmin {
                    username: cli.admin_user.clone(),
                    password: cli.admin_password.clone(),
                    api_key: cli.admin_api_key.clone(),
                });
            let outcome = gproxy_api::bootstrap::seed_from_toml_with_bootstrap(
                &state,
                &toml_str,
                bootstrap_admin.as_ref(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            log_generated_bootstrap_admin(&cli.admin_user, &outcome);
            tracing::info!("TOML seed complete, data persisted to database");
        } else {
            tracing::info!("no existing data or config file, creating defaults");
            let outcome = gproxy_api::bootstrap::seed_defaults(
                &state,
                &gproxy_api::bootstrap::BootstrapAdmin {
                    username: cli.admin_user.clone(),
                    password: cli.admin_password.clone(),
                    api_key: cli.admin_api_key.clone(),
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            log_generated_bootstrap_admin(&cli.admin_user, &outcome);
        }
    }

    let mut config = state.config().as_ref().clone();
    let mut persist_global_settings = false;

    if is_explicit(&matches, "host") {
        config.host = cli.host.clone();
        persist_global_settings = true;
    }
    if is_explicit(&matches, "port") {
        config.port = cli.port;
        persist_global_settings = true;
    }
    if is_explicit(&matches, "proxy") {
        config.proxy = cli.proxy.clone();
        persist_global_settings = true;
    }
    if is_explicit(&matches, "spoof_emulation") {
        config.spoof_emulation = cli.spoof_emulation.clone();
        persist_global_settings = true;
    }
    if is_explicit(&matches, "data_dir") {
        config.data_dir = cli.data_dir.clone();
        config.dsn = dsn.clone();
        persist_global_settings = true;
    }
    if is_explicit(&matches, "dsn") {
        config.dsn = dsn.clone();
        persist_global_settings = true;
    }

    if persist_global_settings {
        state.replace_config(config.clone());
        state
            .storage()
            .apply_write_event(StorageWriteEvent::UpsertGlobalSettings(
                gproxy_storage::GlobalSettingsWrite {
                    host: config.host.clone(),
                    port: config.port,
                    proxy: config.proxy.clone(),
                    spoof_emulation: config.spoof_emulation.clone(),
                    update_source: config.update_source.clone(),
                    enable_usage: config.enable_usage,
                    enable_upstream_log: config.enable_upstream_log,
                    enable_upstream_log_body: config.enable_upstream_log_body,
                    enable_downstream_log: config.enable_downstream_log,
                    enable_downstream_log_body: config.enable_downstream_log_body,
                    dsn: config.dsn.clone(),
                    data_dir: config.data_dir.clone(),
                },
            ))
            .await?;
    }

    // 13. Start remaining background workers
    worker_set.register(workers::quota_reconciler::spawn(
        state.clone(),
        worker_set.subscribe(),
    ));
    worker_set.register(workers::rate_limit_gc::spawn(
        state.clone(),
        worker_set.subscribe(),
    ));
    let health_rx = state.engine().store().subscribe();
    worker_set.register(workers::health_broadcaster::spawn(
        health_rx,
        state.clone(),
        worker_set.subscribe(),
    ));
    tracing::info!("background workers started");

    // 14. Build router and start server
    let app = gproxy_api::api_router(state).merge(web::router());
    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "gproxy listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // 12. Graceful shutdown: drain workers
    tracing::info!("shutting down background workers...");
    worker_set.shutdown().await;

    tracing::info!("gproxy shut down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl+c");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("received ctrl+c"),
        () = terminate => tracing::info!("received SIGTERM"),
    }
}
