use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use wasm_bindgen::prelude::*;

use crate::app::AppState;
use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use crate::http::client::{FetchClient, UpstreamClient};
use crate::store::cache::{CacheBackend, LibsqlCache, UpstashCache};
use crate::store::persistence::{LibsqlPersistence, PersistenceBackend};

/// Process-global app state, populated once by [`init`].
static STATE: OnceLock<AppState> = OnceLock::new();

/// §7.2 edge snapshot freshness: minimum interval between config-version
/// polls. Within the window requests serve the current snapshot untouched.
const SNAPSHOT_POLL_INTERVAL_MS: u64 = 10_000;

/// Wall-clock millis of this isolate's last config-version poll.
static LAST_POLL_MS: AtomicU64 = AtomicU64::new(0);
/// Config version this isolate's snapshot was last built against.
static SEEN_CFG_VERSION: AtomicI64 = AtomicI64::new(0);

fn js_err(e: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{e:?}"))
}

pub(super) fn state() -> Option<&'static AppState> {
    STATE.get()
}

/// Initialise the edge runtime from host-supplied credentials.
///
/// Persistence is always libSQL/Turso (`turso_url` + `turso_token`). The cache
/// is Upstash Redis when both `upstash_url` and `upstash_token` are non-empty,
/// otherwise it falls back to the libSQL kv table. `master_key` unseals stored
/// secrets (absent → plaintext NoopCipher).
///
/// Must be called once before [`super::fetch`]. A second call is a no-op (the
/// first `AppState` wins).
#[wasm_bindgen]
pub async fn init(
    turso_url: String,
    turso_token: String,
    upstash_url: Option<String>,
    upstash_token: Option<String>,
    master_key: Option<String>,
    admin_user: String,
    admin_password: String,
) -> Result<(), JsValue> {
    if STATE.get().is_some() {
        return Ok(());
    }

    // `connect` also ensures the schema (CREATE TABLE IF NOT EXISTS), so an
    // empty edge-first Turso database is usable immediately.
    let persistence: Arc<dyn PersistenceBackend> = Arc::new(
        LibsqlPersistence::connect(turso_url.clone(), turso_token.clone())
            .await
            .map_err(js_err)?,
    );

    let (cache, cache_cfg): (Arc<dyn CacheBackend>, CacheConfig) =
        match (upstash_url, upstash_token) {
            (Some(u), Some(t)) if !u.is_empty() && !t.is_empty() => (
                Arc::new(UpstashCache::new(u.clone(), t)),
                CacheConfig::Upstash { url: u },
            ),
            _ => {
                let c = LibsqlCache::connect(turso_url.clone(), turso_token.clone())
                    .await
                    .map_err(js_err)?;
                (
                    Arc::new(c),
                    CacheConfig::Libsql {
                        url: turso_url.clone(),
                    },
                )
            }
        };

    let config = Arc::new(RuntimeConfig {
        host: "0.0.0.0".to_string(),
        port: 0,
        cache: cache_cfg,
        persistence: PersistenceConfig::Db { dsn: turso_url },
        upstream: UpstreamConfig::from_proxy_url(None),
        instance_id: 0,
        max_attempts: crate::config::DEFAULT_MAX_ATTEMPTS,
        max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_channel: "releases".to_string(),
        // Edge (wasm) never self-updates; PathBuf is required by the type but
        // the field is never read in an edge build.
        update_data_dir: std::path::PathBuf::from("./data"),
        cors_origins: Vec::new(),
    });

    let upstream: Arc<dyn UpstreamClient> = Arc::new(FetchClient::new());

    crate::app::bootstrap::ensure_admin(
        persistence.as_ref(),
        &admin_user,
        Some(admin_password.as_str()),
    )
    .await
    .map_err(js_err)?;

    // Build the control-plane snapshot from persistence (libSQL read ops). An
    // un-provisioned database yields an empty snapshot; provisioning via the
    // admin API or `import` (into the same Turso DB) makes routing live.
    let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(
        crate::app::snapshot::ControlPlaneSnapshot::build(persistence.as_ref(), 1)
            .await
            .map_err(js_err)?,
    ));
    let channels = Arc::new(crate::channel::registry::ChannelRegistry::with_builtin());

    // §7.2 baseline: remember the config version this snapshot was built at so
    // the first request doesn't trigger a spurious rebuild (incr-by-0 reads
    // the counter, creating it at 0 when absent). An unreadable stamp
    // baselines at 0 — the first successful poll then rebuilds once (safe
    // direction).
    SEEN_CFG_VERSION.store(
        cache
            .incr(crate::store::cache::CONFIG_VERSION_KEY, 0, None)
            .await
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    LAST_POLL_MS.store(js_sys::Date::now() as u64, Ordering::Relaxed);

    let _ = STATE.set(AppState::new(
        config,
        cache,
        persistence,
        upstream,
        snapshot,
        channels,
        // Host-supplied master key (base64) unseals stored secrets; absent →
        // NoopCipher (plaintext), for a plaintext-secret edge deployment.
        crate::crypto::cipher_from_master_key(master_key.as_deref()).map_err(js_err)?,
    ));
    Ok(())
}

/// Edge replacement for the native pub/sub invalidation listener (§7.2): at
/// most once per [`SNAPSHOT_POLL_INTERVAL_MS`], read the config-version stamp
/// [`broadcast`](crate::app::invalidation::broadcast) bumps on every mutation
/// and rebuild the snapshot when it moved. The poll slot is claimed before the
/// awaits so interleaved requests do not duplicate the work.
pub(super) async fn refresh_snapshot_if_stale(state: &AppState) {
    let now_ms = js_sys::Date::now() as u64;
    if now_ms.saturating_sub(LAST_POLL_MS.load(Ordering::Relaxed)) < SNAPSHOT_POLL_INTERVAL_MS {
        return;
    }
    LAST_POLL_MS.store(now_ms, Ordering::Relaxed);
    let version = match state
        .cache
        .incr(crate::store::cache::CONFIG_VERSION_KEY, 0, None)
        .await
    {
        Ok(v) => v,
        // Stamp unreadable: keep serving the current snapshot; the next poll
        // window retries.
        Err(_) => return,
    };
    if version == SEEN_CFG_VERSION.load(Ordering::Relaxed) {
        return;
    }
    match state.reload_snapshot().await {
        Ok(()) => {
            SEEN_CFG_VERSION.store(version, Ordering::Relaxed);
            tracing::info!(version, "edge snapshot refreshed");
        }
        Err(e) => tracing::warn!(error = %e, "edge snapshot refresh failed"),
    }
}
