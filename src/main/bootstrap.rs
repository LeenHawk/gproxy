use std::sync::Arc;

use gproxy::app::AppState;
use gproxy::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use gproxy::http::client::UpstreamClient;
use gproxy::store::cache::CacheBackend;
use gproxy::store::persistence::PersistenceBackend;

use super::cli::{Cli, Command};

pub(crate) async fn run(cli: Cli) -> anyhow::Result<()> {
    // Used by native first-run launchers so the same key can be passed through
    // an inherited environment variable and shown once without writing it to
    // disk. It is self-contained and must not initialize persistence.
    if matches!(&cli.command, Some(Command::GenerateKey)) {
        println!("{}", gproxy::util::rand::api_key());
        return Ok(());
    }

    // Self-update (§19): self-contained — needs only an HTTP client + data_dir,
    // so it runs before persistence/cache/server are built, then exits.
    if let Some(Command::Update { action, channel }) = &cli.command {
        return super::cli::run_update(
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
            None => {
                let PersistenceConfig::Db { dsn } = PersistenceConfig::from_parts(
                    cli.persistence,
                    cli.data_dir.clone(),
                    cli.dsn.clone(),
                )?;
                dsn
            }
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
    // Ensure the data dir exists before building persistence: the SQLite file
    // needs its parent dir to exist, and migration writes temp/backup files here.
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
        update_channel: match cli
            .update_channel
            .unwrap_or_else(gproxy::selfupdate::build_channel)
        {
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
    if cli.command.is_none() {
        let PersistenceConfig::Db { dsn } = &config.persistence;
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
        #[cfg(feature = "persist-db")]
        PersistenceConfig::Db { dsn } => {
            Arc::new(gproxy::store::persistence::DbPersistence::connect(dsn).await?)
        }
        #[cfg(not(feature = "persist-db"))]
        PersistenceConfig::Db { .. } => {
            anyhow::bail!("persistence backend `db` requires the `persist-db` feature")
        }
    };
    PersistenceBackend::health(persistence.as_ref()).await?;
    tracing::info!(
        "persistence backend: {} healthy",
        PersistenceBackend::kind(persistence.as_ref())
    );

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
            super::cli::write_secret_file(std::path::Path::new(&output), &json)?;
            tracing::warn!(
                "exported config to {output:?} — contains PLAINTEXT secrets (mode 0600); protect this file"
            );
            return Ok(());
        }
        None => {}
        Some(Command::GenerateKey) => unreachable!("generate-key is dispatched before persistence"),
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
        let empty = PersistenceBackend::list_providers(persistence.as_ref())
            .await?
            .is_empty()
            && PersistenceBackend::list_users(persistence.as_ref())
                .await?
                .is_empty();
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
    let bootstrap_admin_api_key = std::env::var("GPROXY_BOOTSTRAP_ADMIN_API_KEY").ok();
    let channels = gproxy::channel::registry::ChannelRegistry::with_builtin();
    gproxy::app::install_setup::ensure(
        persistence.as_ref(),
        cipher.as_ref(),
        &channels,
        &cli.admin_user,
        cli.admin_password.as_deref(),
        &cli.bootstrap_channels,
        bootstrap_admin_api_key.as_deref(),
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
    let channels = Arc::new(channels);

    let state = AppState::new(
        config,
        cache,
        persistence,
        upstream,
        snapshot,
        channels,
        cipher,
    );

    // Tokenizer registry (§6.3): vocab storage rides the persistence backend;
    // only the download toggle is seeded here from instance settings.
    #[cfg(feature = "count-local")]
    {
        let enabled = PersistenceBackend::list_instance_settings(state.persistence.as_ref())
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

    super::server_lifecycle::serve(state, bind).await
}
