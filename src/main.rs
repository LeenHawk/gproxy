//! GPROXY v2 binary: parse CLI/env config, wire persistence + state + router, serve.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use gproxy::app::AppState;
use gproxy::config::{
    CacheConfig, PersistenceConfig, PersistenceKind, RuntimeConfig, UpstreamConfig,
};
use gproxy::http;
use gproxy::http::client::UpstreamClient;
use gproxy::store::cache::CacheBackend;
use gproxy::store::persistence::PersistenceBackend;

#[derive(Parser)]
#[command(name = "gproxy", version, about = "GPROXY v2 LLM proxy")]
struct Cli {
    /// Bind host (IPv6 must use bracket notation, e.g. [::1]).
    #[arg(long, env = "GPROXY_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Bind port.
    #[arg(long, env = "GPROXY_PORT", default_value_t = 8787)]
    port: u16,

    /// Persistence backend: `file` (local disk) or `db` (SeaORM). Defaults to
    /// `db` — a single SQLite file at `<data_dir>/gproxy.db` (the v1 path), so a
    /// v2 binary dropped in place adopts and migrates an existing v1 database.
    #[arg(long, env = "GPROXY_PERSISTENCE", default_value = "db")]
    persistence: PersistenceKind,

    /// Data directory used by the file persistence backend.
    #[arg(long, env = "GPROXY_DATA_DIR", default_value = "./data")]
    data_dir: PathBuf,

    /// Database connection string (required when --persistence=db).
    #[arg(long, env = "GPROXY_DSN")]
    dsn: Option<String>,

    /// Redis URL for the shared cache backend (e.g. redis://127.0.0.1:6379).
    /// Omit to use the in-process memory cache.
    #[arg(long, env = "GPROXY_REDIS_URL")]
    redis_url: Option<String>,

    /// Optional native proxy URL for upstream provider requests.
    #[arg(long, env = "GPROXY_UPSTREAM_PROXY_URL")]
    upstream_proxy_url: Option<String>,

    /// Numeric identifier for this instance (used to partition per-instance
    /// rows in the database; set distinct values across a multi-node fleet).
    #[arg(long, env = "GPROXY_INSTANCE_ID", default_value_t = 0)]
    instance_id: u64,

    /// Per-request failover attempt cap: the loop stops after this many
    /// candidate attempts even if more remain (bounds fan-out on a large
    /// unhealthy pool). The AuthDead forced-refresh retry does not count.
    #[arg(long, env = "GPROXY_MAX_ATTEMPTS", default_value_t = gproxy::config::DEFAULT_MAX_ATTEMPTS)]
    max_attempts: u32,

    /// §16.2 overload protection: max concurrent in-flight gateway requests
    /// before load-shedding excess to 503. Bounds memory/latency under a spike.
    #[arg(long, env = "GPROXY_MAX_IN_FLIGHT", default_value_t = gproxy::config::DEFAULT_MAX_IN_FLIGHT)]
    max_in_flight: usize,

    /// Reverse proxies (IPs, repeatable / comma-separated) whose forwarding
    /// headers are trusted for client-IP resolution, in addition to loopback.
    /// Connections from any other peer have x-forwarded-for / x-real-ip ignored.
    #[arg(
        long = "trusted-proxy",
        env = "GPROXY_TRUSTED_PROXIES",
        value_delimiter = ','
    )]
    trusted_proxies: Vec<std::net::IpAddr>,

    /// B2: allowed cross-origin admin console Origins (repeatable / comma-separated),
    /// e.g. https://console.example.com. Empty = same-origin only.
    #[arg(
        long = "cors-origin",
        env = "GPROXY_CORS_ORIGINS",
        value_delimiter = ','
    )]
    cors_origins: Vec<String>,

    /// §19 self-update: GitHub owner/repo for admin-triggered updates. Omit to
    /// disable self-update (admin check/apply will return 409).
    #[arg(long, env = "GPROXY_UPDATE_REPO")]
    update_repo: Option<String>,

    /// §19.3 channel for admin-triggered self-update (`releases` or `staging`).
    ///
    /// Uses a DISTINCT env var (`GPROXY_UPDATE_CHANNEL_SERVE`) to avoid a clap
    /// collision with the `update` subcommand's `GPROXY_UPDATE_CHANNEL` env —
    /// both map different args but clap would error if the same env key appeared
    /// in two different arg definitions within the same parse call.
    #[arg(long, env = "GPROXY_UPDATE_CHANNEL_SERVE", default_value = "releases")]
    update_channel: gproxy::selfupdate::Channel,

    /// Admin username for the first-boot bootstrap / credential override (§14.2).
    #[arg(long, env = "GPROXY_ADMIN_USER", default_value = "admin")]
    admin_user: String,

    /// Admin password override (§14.2): when set, force-resets this admin every
    /// startup (host-level recovery). Prefer env over the CLI flag — the flag is
    /// visible in /proc/*/cmdline. Never logged.
    #[arg(long, env = "GPROXY_ADMIN_PASSWORD")]
    admin_password: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Import a config bundle (JSON) into the persistence backend, then exit.
    Import {
        /// Path to the bundle file.
        #[arg(long = "in")]
        input: PathBuf,
    },
    /// Export all control-plane config (with PLAINTEXT secrets) to a bundle
    /// file that `import` consumes, then exit.
    Export {
        /// Path to write the bundle file.
        #[arg(long = "out")]
        output: PathBuf,
    },
    /// MIGRATE-V1 (remove in 2.1): import a legacy v1 SQLite database into a v2
    /// db backend, then exit. For explicit/offline migrations; the serve path
    /// auto-migrates a v1 db found at the configured location.
    #[cfg(feature = "migrate-v1")]
    MigrateV1 {
        /// Path to the v1 `gproxy.db` to read (read-only).
        #[arg(long = "from")]
        from: PathBuf,
        /// Target v2 db DSN. Defaults to the server's configured db.
        #[arg(long = "to")]
        to: Option<String>,
        /// Read and report counts without writing anything.
        #[arg(long = "dry-run", default_value_t = false)]
        dry_run: bool,
    },
    /// Self-update (§19): check the configured release channel for a new build,
    /// and optionally download + verify + swap the binary. Native-only.
    Update {
        #[command(subcommand)]
        action: UpdateAction,

        /// GitHub `owner/repo` whose Releases host the signed manifest +
        /// artifacts.
        #[arg(long, env = "GPROXY_UPDATE_REPO")]
        repo: String,

        /// Release channel: `releases` (semver) or `staging` (sha256).
        #[arg(long, env = "GPROXY_UPDATE_CHANNEL", default_value = "releases")]
        channel: gproxy::selfupdate::Channel,
    },
}

#[derive(clap::Subcommand)]
enum UpdateAction {
    /// Check the channel and report current/latest without changing anything.
    Check,
    /// Download + verify + swap the binary if an update is available.
    Apply {
        /// Restart model after a successful swap: `supervisor` (exit with a
        /// sentinel code for the orchestrator), `re-exec` (execv in place), or
        /// `none` (stage only).
        #[arg(long, env = "GPROXY_UPDATE_RESTART", default_value = "supervisor")]
        restart: gproxy::selfupdate::Restart,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    // Self-update (§19): self-contained — needs only an HTTP client + data_dir,
    // so it runs before persistence/cache/server are built, then exits.
    if let Some(Command::Update {
        action,
        repo,
        channel,
    }) = &cli.command
    {
        return run_update(
            repo.clone(),
            *channel,
            cli.data_dir.clone(),
            cli.upstream_proxy_url.clone(),
            action,
        )
        .await;
    }

    // MIGRATE-V1 (remove in 2.1): explicit migration subcommand — self-contained
    // and dispatched before the shared persistence is built (it manages its own
    // source/target db connections).
    #[cfg(feature = "migrate-v1")]
    if let Some(Command::MigrateV1 { from, to, dry_run }) = &cli.command {
        let master_key = std::env::var("GPROXY_MASTER_KEY").ok();
        let cipher = gproxy::crypto::cipher_from_master_key(master_key.as_deref())?;
        let to_dsn = match to {
            Some(d) => d.clone(),
            None => match PersistenceConfig::from_parts(
                cli.persistence,
                cli.data_dir.clone(),
                cli.dsn.clone(),
            )? {
                PersistenceConfig::Db { dsn } => dsn,
                PersistenceConfig::File { .. } => anyhow::bail!(
                    "migrate-v1 needs a db target: pass --to <dsn> or use --persistence db"
                ),
            },
        };
        let channels = gproxy::channel::registry::ChannelRegistry::with_builtin();
        let report =
            gproxy::app::migrate_v1::run_cli(from, &to_dsn, *dry_run, cipher.as_ref(), &channels)
                .await?;
        tracing::info!(?report, dry_run = *dry_run, "migrate-v1 finished");
        return Ok(());
    }

    let cache_cfg = CacheConfig::from_url(cli.redis_url);
    // Clone data_dir BEFORE from_parts moves it, so update_data_dir can also
    // refer to the same directory (self-update stages under <data_dir>/.update).
    let update_data_dir = cli.data_dir.clone();
    // Ensure the data dir exists before building persistence: the file backend
    // creates it on open, but the db backend's SQLite file needs its parent dir
    // to already exist (and the migration writes its temp/backup files here too).
    std::fs::create_dir_all(&cli.data_dir)?;
    let persistence_cfg = PersistenceConfig::from_parts(cli.persistence, cli.data_dir, cli.dsn)?;
    let upstream_cfg = UpstreamConfig::from_proxy_url(cli.upstream_proxy_url);

    let config = Arc::new(RuntimeConfig {
        host: cli.host,
        port: cli.port,
        cache: cache_cfg,
        persistence: persistence_cfg,
        upstream: upstream_cfg,
        instance_id: cli.instance_id,
        max_attempts: cli.max_attempts,
        max_in_flight: cli.max_in_flight,
        trusted_proxies: cli.trusted_proxies,
        update_repo: cli.update_repo,
        update_channel: match cli.update_channel {
            gproxy::selfupdate::Channel::Releases => "releases".to_string(),
            gproxy::selfupdate::Channel::Staging => "staging".to_string(),
        },
        update_data_dir,
        cors_origins: cli.cors_origins,
    });

    let bind = config.bind_addr()?;

    // Envelope cipher (§14.1): GPROXY_MASTER_KEY is env-only (§8-E — never a
    // CLI flag). Malformed key = hard boot error; absent key = plaintext mode.
    // Built before persistence because the v1→v2 migration hook needs it.
    let master_key = std::env::var("GPROXY_MASTER_KEY").ok();
    if master_key.is_none() {
        tracing::warn!("GPROXY_MASTER_KEY not set; secrets stored and read as plaintext");
    }
    let cipher = gproxy::crypto::cipher_from_master_key(master_key.as_deref())?;

    // MIGRATE-V1 (remove in 2.1): on the serve path, if the configured SQLite db
    // is a legacy v1 database, migrate it to v2 in place (backing the v1 file up)
    // BEFORE the v2 backend opens it. No-op on a fresh install or an existing v2.
    #[cfg(feature = "migrate-v1")]
    if cli.command.is_none()
        && let PersistenceConfig::Db { dsn } = &config.persistence
    {
        let channels = gproxy::channel::registry::ChannelRegistry::with_builtin();
        if let Some(report) =
            gproxy::app::migrate_v1::maybe_migrate_on_boot(dsn, cipher.as_ref(), &channels).await?
        {
            tracing::info!(?report, "v1 → v2 migration complete");
        }
    }

    // Persistence is built next — the import subcommand and first-boot hook
    // both need it before the (optional) cache backend is started.
    let persistence: Arc<dyn PersistenceBackend> = match &config.persistence {
        #[cfg(feature = "persist-file")]
        PersistenceConfig::File { data_dir } => {
            Arc::new(gproxy::store::persistence::FilePersistence::open(data_dir.clone()).await?)
        }
        #[cfg(not(feature = "persist-file"))]
        PersistenceConfig::File { .. } => {
            anyhow::bail!("persistence backend `file` requires the `persist-file` feature")
        }
        #[cfg(feature = "persist-db")]
        PersistenceConfig::Db { dsn } => {
            Arc::new(gproxy::store::persistence::DbPersistence::connect(dsn).await?)
        }
        #[cfg(not(feature = "persist-db"))]
        PersistenceConfig::Db { .. } => {
            anyhow::bail!("persistence backend `db` requires the `persist-db` feature")
        }
    };
    persistence.health().await?;
    tracing::info!("persistence backend: {} healthy", persistence.kind());

    // Config subcommands: import / export, then exit — no server started.
    match cli.command {
        Some(Command::Import { input }) => {
            let json = std::fs::read_to_string(&input)?;
            let stats =
                gproxy::app::import::import_bundle(persistence.as_ref(), cipher.as_ref(), &json)
                    .await?;
            tracing::info!(records = stats.records, "bundle imported");
            return Ok(());
        }
        Some(Command::Export { output }) => {
            let bundle =
                gproxy::app::export::export_bundle(persistence.as_ref(), cipher.as_ref()).await?;
            let json = serde_json::to_string_pretty(&bundle)?;
            write_secret_file(std::path::Path::new(&output), &json)?;
            tracing::warn!(
                "exported config to {output:?} — contains PLAINTEXT secrets (mode 0600); protect this file"
            );
            return Ok(());
        }
        None => {}
        // Handled by the early dispatch above (before persistence is built).
        Some(Command::Update { .. }) => unreachable!("update is dispatched before persistence"),
        #[cfg(feature = "migrate-v1")]
        Some(Command::MigrateV1 { .. }) => {
            unreachable!("migrate-v1 is dispatched before persistence")
        }
    }

    // First-boot hook: if GPROXY_IMPORT_FILE is set and the store is empty,
    // seed it from the bundle before building the snapshot.
    if let Ok(path) = std::env::var("GPROXY_IMPORT_FILE")
        && !path.is_empty()
    {
        let empty = persistence.list_providers().await?.is_empty()
            && persistence.list_users().await?.is_empty();
        if empty {
            let json = std::fs::read_to_string(&path)?;
            let stats =
                gproxy::app::import::import_bundle(persistence.as_ref(), cipher.as_ref(), &json)
                    .await?;
            tracing::info!(records = stats.records, path, "first-boot bundle imported");
        } else {
            tracing::info!(path, "GPROXY_IMPORT_FILE set but store not empty; skipped");
        }
    }

    // First-boot admin bootstrap (§14.2): runs after the import hook so an
    // imported admin pre-empts random creation. The override (if set) force-
    // resets the admin every startup. Only on the serve path — the import/
    // export subcommands have already returned above.
    gproxy::app::bootstrap::ensure_admin(
        persistence.as_ref(),
        &cli.admin_user,
        cli.admin_password.as_deref(),
    )
    .await?;

    let cache: Arc<dyn CacheBackend> = match &config.cache {
        #[cfg(feature = "cache-memory")]
        CacheConfig::Memory => {
            tracing::info!("cache backend: memory ready");
            Arc::new(gproxy::store::cache::MemoryCache::new())
        }
        #[cfg(not(feature = "cache-memory"))]
        CacheConfig::Memory => {
            anyhow::bail!("cache backend `memory` requires the `cache-memory` feature")
        }
        #[cfg(feature = "cache-redis")]
        CacheConfig::Redis { url } => {
            // Redis signals a multi-instance fleet; the file backend has no
            // cross-process coordination, so every instance would need its own
            // (divergent) data dir.
            if matches!(config.persistence, PersistenceConfig::File { .. }) {
                tracing::warn!(
                    "redis cache + file persistence: the file backend is \
                     single-instance — multi-node fleets must use --persistence=db"
                );
            }
            let c = gproxy::store::cache::RedisCache::connect(url).await?;
            c.health().await?;
            tracing::info!("cache backend: redis ready");
            Arc::new(c)
        }
        #[cfg(not(feature = "cache-redis"))]
        CacheConfig::Redis { .. } => {
            anyhow::bail!("cache backend `redis` requires the `cache-redis` feature")
        }
        CacheConfig::Libsql { .. } | CacheConfig::Upstash { .. } => {
            anyhow::bail!("edge-only cache backend cannot be used by native server")
        }
    };

    #[cfg(not(feature = "upstream-wreq"))]
    compile_error!("a native GPROXY binary requires the `upstream-wreq` feature");
    #[cfg(feature = "upstream-wreq")]
    let upstream: Arc<dyn UpstreamClient> = Arc::new(
        gproxy::http::client::WreqClient::with_proxy_url(config.upstream.proxy_url.as_deref())?,
    );
    #[cfg(feature = "upstream-wreq")]
    tracing::info!(
        "upstream transport: wreq ready{}",
        if config.upstream.proxy_url.is_some() {
            " with proxy"
        } else {
            ""
        }
    );

    let snapshot =
        gproxy::app::snapshot::ControlPlaneSnapshot::build(persistence.as_ref(), 1).await?;
    let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot));
    let channels = Arc::new(gproxy::channel::registry::ChannelRegistry::with_builtin());

    let state = AppState::new(
        config,
        cache,
        persistence,
        upstream,
        snapshot,
        channels,
        cipher,
    );

    // Tokenizer registry (§6.3): vocab storage rides the persistence backend
    // (file = raw files under data_dir/tokenizers, db = BLOB rows); only the
    // download toggle is seeded here from instance settings.
    #[cfg(feature = "count-local")]
    {
        let enabled = state
            .persistence
            .list_instance_settings()
            .await?
            .first()
            .is_some_and(|s| s.enable_tokenizer_download);
        state.tokenizers.set_download_enabled(enabled);
    }

    // Multi-instance: listen for cross-instance config invalidation (redis only;
    // memory cache is single-instance and its subscribe is a no-op).
    if matches!(state.config.cache, CacheConfig::Redis { .. }) {
        gproxy::app::invalidation::spawn_invalidation_listener(state.clone());
    }

    // §8-D: periodically purge usage/request-log rows past the retention window
    // (no-op until an operator sets `instance_settings.retention_days`).
    gproxy::app::retention::spawn_retention_task(state.clone());

    let app = http::server::router(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("GPROXY v2 listening on http://{bind}");
    // ConnectInfo carries the socket peer into handlers — the anchor the
    // trusted-proxy client-IP resolution verifies forwarding headers against.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

/// Run the `update` subcommand (§19): build a proxy-aware HTTP client, then
/// check or apply on the configured channel. Self-contained; never starts the
/// server. `apply` may diverge (re-exec) or exit with the supervisor sentinel.
async fn run_update(
    repo: String,
    channel: gproxy::selfupdate::Channel,
    data_dir: PathBuf,
    proxy_url: Option<String>,
    action: &UpdateAction,
) -> anyhow::Result<()> {
    #[cfg(not(feature = "upstream-wreq"))]
    {
        let _ = (repo, channel, data_dir, proxy_url, action);
        anyhow::bail!("self-update requires the `upstream-wreq` feature");
    }
    #[cfg(feature = "upstream-wreq")]
    {
        let client: Arc<dyn UpstreamClient> = Arc::new(
            gproxy::http::client::WreqClient::with_proxy_url(proxy_url.as_deref())?,
        );
        let ctx = gproxy::selfupdate::UpdateContext {
            repo,
            channel,
            data_dir,
            client,
        };
        match action {
            UpdateAction::Check => {
                let report = gproxy::selfupdate::check(&ctx).await?;
                println!(
                    "channel={channel:?} current={} latest={} available={}{}",
                    report.current,
                    report.latest,
                    report.available,
                    report
                        .notes_url
                        .as_deref()
                        .map(|u| format!(" notes={u}"))
                        .unwrap_or_default()
                );
                Ok(())
            }
            UpdateAction::Apply { restart } => {
                let version = gproxy::selfupdate::apply(&ctx, *restart).await?;
                tracing::info!(version, "update applied (no restart requested)");
                Ok(())
            }
        }
    }
}

/// Write `contents` to `path` owner-readable only (0600), via a same-directory
/// temp file + atomic rename — the plaintext-secret export must never be
/// world-readable, not even transiently, and never half-written.
fn write_secret_file(path: &std::path::Path, contents: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("export");
    let tmp = dir.join(format!(".{name}.tmp"));
    let mut opts = std::fs::OpenOptions::new();
    // create_new: refuse to write through a pre-existing (possibly symlinked,
    // possibly lax-permissioned) temp file from an interrupted run.
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let write = || -> std::io::Result<()> {
        let mut f = opts.open(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => tracing::warn!("failed to install SIGTERM handler: {e}"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
