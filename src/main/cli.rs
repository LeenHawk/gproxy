use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use gproxy::config::PersistenceKind;
use gproxy::http::client::UpstreamClient;

#[derive(Parser)]
#[command(name = "gproxy", version, about = "GPROXY v2 LLM proxy")]
pub(crate) struct Cli {
    /// Bind host (IPv6 must use bracket notation, e.g. [::1]).
    #[arg(long, env = "GPROXY_HOST", default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Bind port.
    #[arg(long, env = "GPROXY_PORT", default_value_t = 8787)]
    pub(crate) port: u16,

    /// Persistence backend. `db` uses SeaORM and defaults to a single SQLite
    /// file at `<data_dir>/gproxy.db` (the v1 path), so a v2 binary dropped in
    /// place adopts and migrates an existing v1 database.
    #[arg(long, env = "GPROXY_PERSISTENCE", default_value = "db")]
    pub(crate) persistence: PersistenceKind,

    /// Data directory used by the default SQLite database, v1 migration, and
    /// self-update staging.
    #[arg(long, env = "GPROXY_DATA_DIR", default_value = "./data")]
    pub(crate) data_dir: PathBuf,

    /// Database connection string. Omit to use SQLite under `--data-dir`.
    #[arg(long, env = "GPROXY_DSN")]
    pub(crate) dsn: Option<String>,

    /// Redis URL for the shared cache backend (e.g. redis://127.0.0.1:6379).
    /// Omit to use the in-process memory cache.
    #[arg(long, env = "GPROXY_REDIS_URL")]
    pub(crate) redis_url: Option<String>,

    /// Optional native proxy URL for upstream provider requests.
    #[arg(long, env = "GPROXY_UPSTREAM_PROXY_URL")]
    pub(crate) upstream_proxy_url: Option<String>,

    /// Numeric identifier for this instance (used to partition per-instance
    /// rows in the database; set distinct values across a multi-node fleet).
    #[arg(long, env = "GPROXY_INSTANCE_ID", default_value_t = 0)]
    pub(crate) instance_id: u64,

    /// Per-request failover attempt cap: the loop stops after this many
    /// candidate attempts even if more remain (bounds fan-out on a large
    /// unhealthy pool). The AuthDead forced-refresh retry does not count.
    #[arg(long, env = "GPROXY_MAX_ATTEMPTS", default_value_t = gproxy::config::DEFAULT_MAX_ATTEMPTS)]
    pub(crate) max_attempts: u32,

    /// §16.2 overload protection: max concurrent in-flight gateway requests
    /// before load-shedding excess to 503. Bounds memory/latency under a spike.
    #[arg(long, env = "GPROXY_MAX_IN_FLIGHT", default_value_t = gproxy::config::DEFAULT_MAX_IN_FLIGHT)]
    pub(crate) max_in_flight: usize,

    /// Reverse proxies (IPs, repeatable / comma-separated) whose forwarding
    /// headers are trusted for client-IP resolution, in addition to loopback.
    /// Connections from any other peer have x-forwarded-for / x-real-ip ignored.
    #[arg(
        long = "trusted-proxy",
        env = "GPROXY_TRUSTED_PROXIES",
        value_delimiter = ','
    )]
    pub(crate) trusted_proxies: Vec<std::net::IpAddr>,

    /// Allowed cross-origin browser Origins for admin/API gateway requests
    /// (repeatable / comma-separated), e.g. https://app.example.com. Empty =
    /// same-origin only.
    #[arg(
        long = "cors-origin",
        env = "GPROXY_CORS_ORIGINS",
        value_delimiter = ','
    )]
    pub(crate) cors_origins: Vec<String>,

    /// §19.3 channel for admin-triggered self-update (`releases` or `staging`).
    ///
    /// Uses a DISTINCT env var (`GPROXY_UPDATE_CHANNEL_SERVE`) to avoid a clap
    /// collision with the `update` subcommand's `GPROXY_UPDATE_CHANNEL` env —
    /// both map different args but clap would error if the same env key appeared
    /// in two different arg definitions within the same parse call.
    #[arg(long, env = "GPROXY_UPDATE_CHANNEL_SERVE")]
    pub(crate) update_channel: Option<gproxy::selfupdate::Channel>,

    /// Admin username for the first-boot bootstrap / credential override (§14.2).
    #[arg(long, env = "GPROXY_ADMIN_USER", default_value = "admin")]
    pub(crate) admin_user: String,

    /// Admin password override (§14.2): when set, force-resets this admin every
    /// startup (host-level recovery). Prefer env over the CLI flag — the flag is
    /// visible in /proc/*/cmdline. Never logged.
    #[arg(long, env = "GPROXY_ADMIN_PASSWORD")]
    pub(crate) admin_password: Option<String>,

    /// Built-in channels to create as enabled providers during first-run setup.
    /// Repeat the flag or pass a comma-separated environment value.
    #[arg(
        long = "bootstrap-channel",
        env = "GPROXY_BOOTSTRAP_CHANNELS",
        value_delimiter = ','
    )]
    pub(crate) bootstrap_channels: Vec<String>,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(clap::Subcommand)]
pub(crate) enum Command {
    /// Generate a new CSPRNG-backed user API key and print it once.
    GenerateKey,
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

        /// Release channel: `releases` (semver) or `staging` (commit identity).
        #[arg(long, env = "GPROXY_UPDATE_CHANNEL", default_value = "releases")]
        channel: gproxy::selfupdate::Channel,
    },
}

#[derive(clap::Subcommand)]
pub(crate) enum UpdateAction {
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

/// Run the `update` subcommand (§19): build a proxy-aware HTTP client, then
/// check or apply on the configured channel. Self-contained; never starts the
/// server. `apply` may diverge (re-exec) or exit with the supervisor sentinel.
pub(crate) async fn run_update(
    channel: gproxy::selfupdate::Channel,
    data_dir: PathBuf,
    proxy_url: Option<String>,
    action: &UpdateAction,
) -> anyhow::Result<()> {
    #[cfg(not(feature = "upstream-wreq"))]
    {
        let _ = (channel, data_dir, proxy_url, action);
        anyhow::bail!("self-update requires the `upstream-wreq` feature");
    }
    #[cfg(feature = "upstream-wreq")]
    {
        let client: Arc<dyn UpstreamClient> = Arc::new(
            gproxy::http::client::WreqClient::with_proxy_url(proxy_url.as_deref())?,
        );
        let ctx = gproxy::selfupdate::UpdateContext {
            repo: gproxy::selfupdate::DEFAULT_REPO.to_string(),
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
pub(crate) fn write_secret_file(path: &std::path::Path, contents: &str) -> anyhow::Result<()> {
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
