//! Configuration: CLI/env-only bootstrap (no config file).

use std::net::SocketAddr;
use std::path::PathBuf;

/// CLI input type only — used by `clap` for `--persistence`.
///
/// The `clap::ValueEnum` derive is native-only (clap is not a wasm dep); the
/// enum itself stays shared so `PersistenceConfig::from_parts` compiles on both
/// targets.
#[cfg_attr(not(target_arch = "wasm32"), derive(clap::ValueEnum))]
#[cfg_attr(not(target_arch = "wasm32"), value(rename_all = "lowercase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceKind {
    /// Local disk — single-instance only.
    File,
    /// SeaORM-backed database — supports multi-instance.
    Db,
}

/// Validated cache configuration. Illegal states (e.g. Redis without URL)
/// cannot be constructed.
#[derive(Clone)]
pub enum CacheConfig {
    Memory,
    Redis { url: String },
    Libsql { url: String },
    Upstash { url: String },
}

impl std::fmt::Debug for CacheConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheConfig::Memory => write!(f, "Memory"),
            CacheConfig::Redis { .. } => write!(f, "Redis {{ url: <redacted> }}"),
            CacheConfig::Libsql { .. } => write!(f, "Libsql {{ url: <redacted> }}"),
            CacheConfig::Upstash { .. } => write!(f, "Upstash {{ url: <redacted> }}"),
        }
    }
}

impl CacheConfig {
    pub fn from_url(redis_url: Option<String>) -> Self {
        match redis_url {
            Some(url) => Self::Redis { url },
            None => Self::Memory,
        }
    }
}

/// Validated persistence configuration. `Db` variant always carries a DSN.
#[derive(Clone)]
pub enum PersistenceConfig {
    File { data_dir: PathBuf },
    Db { dsn: String },
}

impl std::fmt::Debug for PersistenceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceConfig::File { data_dir } => {
                write!(f, "File {{ data_dir: {data_dir:?} }}")
            }
            PersistenceConfig::Db { .. } => write!(f, "Db {{ dsn: <redacted> }}"),
        }
    }
}

impl PersistenceConfig {
    pub fn from_parts(
        kind: PersistenceKind,
        data_dir: PathBuf,
        dsn: Option<String>,
    ) -> anyhow::Result<Self> {
        match kind {
            PersistenceKind::File => Ok(Self::File { data_dir }),
            PersistenceKind::Db => Ok(Self::Db {
                dsn: match dsn {
                    Some(d) => d,
                    // No DSN given → default to a SQLite file named `gproxy.db`
                    // inside the data dir. Same name/path v1 used, so a v2 binary
                    // dropped in place keeps writing to `<data_dir>/gproxy.db`
                    // (the legacy file is migrated in-place first; see
                    // `app::migrate_v1`). `mode=rwc` creates it if absent.
                    None => {
                        let path = std::path::absolute(data_dir.join("gproxy.db"))
                            .map_err(|e| anyhow::anyhow!("resolve default db path: {e}"))?;
                        format!("sqlite://{}?mode=rwc", path.display())
                    }
                },
            }),
        }
    }
}

/// Outbound upstream transport configuration.
#[derive(Clone)]
pub struct UpstreamConfig {
    /// Native-only proxy for upstream provider requests. Redacted in `Debug`
    /// because proxy URLs may carry credentials.
    pub proxy_url: Option<String>,
}

impl std::fmt::Debug for UpstreamConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.proxy_url {
            Some(_) => write!(f, "UpstreamConfig {{ proxy_url: <redacted> }}"),
            None => write!(f, "UpstreamConfig {{ proxy_url: None }}"),
        }
    }
}

impl UpstreamConfig {
    pub fn from_proxy_url(proxy_url: Option<String>) -> Self {
        let proxy_url = proxy_url.and_then(|url| {
            let trimmed = url.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        Self { proxy_url }
    }
}

/// Immutable runtime snapshot built from CLI args / environment variables.
///
/// Wrapped in [`Arc`](std::sync::Arc) for cheap sharing across handlers.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Bind host. IPv6 addresses must use bracket notation (e.g. `[::1]`)
    /// because [`bind_addr`](Self::bind_addr) parses `host:port` as a
    /// [`SocketAddr`].
    pub host: String,
    pub port: u16,
    pub cache: CacheConfig,
    pub persistence: PersistenceConfig,
    pub upstream: UpstreamConfig,
    /// Numeric identifier for this instance. Numeric (not a name) so the
    /// database can partition / shard per-instance rows by it later.
    pub instance_id: u64,
    /// §6.4 per-request failover budget: the loop stops after this many
    /// candidate ATTEMPTS even if more candidates remain (returns the last
    /// error). Bounds pathological fan-out on a large unhealthy pool. The
    /// AuthDead forced-refresh retry does NOT count against this (same logical
    /// candidate). Default [`DEFAULT_MAX_ATTEMPTS`].
    pub max_attempts: u32,
    /// §16.2 overload protection: max concurrent in-flight gateway requests
    /// before load-shedding to 503. Bounds memory/latency under a traffic spike
    /// or a slow upstream. Default [`DEFAULT_MAX_IN_FLIGHT`].
    pub max_in_flight: usize,
    /// Reverse proxies whose forwarding headers (`x-forwarded-for` /
    /// `x-real-ip`) are honored for client-IP resolution, in ADDITION to
    /// loopback (always trusted). A connection from any other peer has its
    /// forwarding headers ignored — the peer IS the client.
    pub trusted_proxies: Vec<std::net::IpAddr>,
    /// §19 self-update: GitHub `owner/repo` hosting the signed manifest +
    /// release artifacts. `None` = self-update disabled (admin check/apply → 409).
    pub update_repo: Option<String>,
    /// §19.3 release channel tracked by admin self-update.
    ///
    /// Stored as `String` ("releases" | "staging") rather than
    /// `crate::selfupdate::Channel` because the `selfupdate` module is
    /// `#[cfg(not(target_arch = "wasm32"))]` in `lib.rs` — the module is
    /// entirely absent under `wasm32-unknown-unknown`, so the enum type is not
    /// visible there.  The handler (Task 2) will parse this into `Channel`
    /// on the native path.  Valid values: "releases" (default), "staging".
    pub update_channel: String,
    /// Directory under which self-update stages downloads (`<dir>/.update`, §19.5).
    /// Sourced from `--data-dir` (always set; default `./data`) so the `db`
    /// persistence backend also has a writable staging dir.
    pub update_data_dir: std::path::PathBuf,
    /// B2 cross-origin admin: exact allowed Origins (e.g. `https://console.example.com`)
    /// for the admin API CORS layer + CSRF allow-list. Empty (default) = same-origin
    /// only (no CORS headers, SameSite=Lax cookie). Non-empty enables credentialed
    /// CORS for these origins and switches the session cookie to SameSite=None; Secure.
    pub cors_origins: Vec<String>,
}

/// Default per-request failover attempt cap (`GPROXY_MAX_ATTEMPTS`).
pub const DEFAULT_MAX_ATTEMPTS: u32 = 6;

/// Default max concurrent in-flight gateway requests (`GPROXY_MAX_IN_FLIGHT`).
pub const DEFAULT_MAX_IN_FLIGHT: usize = 1024;

/// Max accepted request-body size, enforced on BOTH surfaces: native via the
/// gateway `DefaultBodyLimit` layer, edge via an explicit check in
/// `http::edge` (content-length pre-check + post-read length check) → 413.
pub const MAX_BODY_BYTES: usize = 50 * 1024 * 1024;

/// Upstream transport bounds (§16.2 slow-upstream guard — without them a dead
/// or deliberately slow upstream holds a gateway concurrency slot forever).
/// TCP/TLS connect cap.
pub const UPSTREAM_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Per-read idle cap: bounds silent stalls (header wait, dead streams) while
/// leaving actively-streaming responses uncapped in total duration.
pub const UPSTREAM_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
/// Total cap for NON-streaming upstream calls (connect → full body buffered).
/// Streaming is bounded by the read timeout only — long active streams are
/// legitimate.
pub const UPSTREAM_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

impl RuntimeConfig {
    /// Resolve the `host:port` bind address.
    pub fn bind_addr(&self) -> anyhow::Result<SocketAddr> {
        let addr = format!("{}:{}", self.host, self.port);
        addr.parse()
            .map_err(|e| anyhow::anyhow!("invalid bind address {addr}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_cfg() -> RuntimeConfig {
        RuntimeConfig {
            host: "127.0.0.1".to_string(),
            port: 8787,
            cache: CacheConfig::Memory,
            persistence: PersistenceConfig::File {
                data_dir: PathBuf::from("./data"),
            },
            upstream: UpstreamConfig::from_proxy_url(None),
            instance_id: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            trusted_proxies: Vec::new(),
            update_repo: None,
            update_channel: "releases".to_string(),
            update_data_dir: PathBuf::from("./data"),
            cors_origins: Vec::new(),
        }
    }

    #[test]
    fn bind_addr_parses() {
        let addr = file_cfg().bind_addr().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8787");
    }

    #[test]
    fn persistence_db_without_dsn_defaults_to_data_dir_sqlite() {
        // db backend with no DSN now derives `<data_dir>/gproxy.db` (the v1 path)
        // instead of erroring — the drop-in default (see `app::migrate_v1`).
        let cfg = PersistenceConfig::from_parts(PersistenceKind::Db, PathBuf::from("./data"), None)
            .unwrap();
        match cfg {
            PersistenceConfig::Db { dsn } => {
                assert!(dsn.starts_with("sqlite://"), "got {dsn}");
                assert!(dsn.contains("gproxy.db"), "got {dsn}");
                assert!(dsn.contains("mode=rwc"), "got {dsn}");
            }
            other => panic!("expected Db, got {other:?}"),
        }
    }

    #[test]
    fn persistence_db_with_dsn_is_ok() {
        PersistenceConfig::from_parts(
            PersistenceKind::Db,
            PathBuf::from("./data"),
            Some("sqlite://test.db".to_string()),
        )
        .unwrap();
    }

    #[test]
    fn persistence_file_is_ok() {
        PersistenceConfig::from_parts(PersistenceKind::File, PathBuf::from("./data"), None)
            .unwrap();
    }

    #[test]
    fn cache_from_url_none_is_memory() {
        assert!(matches!(CacheConfig::from_url(None), CacheConfig::Memory));
    }

    #[test]
    fn cache_from_url_some_is_redis() {
        let cfg = CacheConfig::from_url(Some("redis://127.0.0.1".to_string()));
        assert!(matches!(cfg, CacheConfig::Redis { .. }));
    }

    #[test]
    fn upstream_proxy_url_blank_is_none() {
        let cfg = UpstreamConfig::from_proxy_url(Some("  ".to_string()));
        assert!(cfg.proxy_url.is_none());
    }

    #[test]
    fn upstream_proxy_url_is_trimmed() {
        let cfg = UpstreamConfig::from_proxy_url(Some(" http://127.0.0.1:7890 ".to_string()));
        assert_eq!(cfg.proxy_url.as_deref(), Some("http://127.0.0.1:7890"));
    }
}
