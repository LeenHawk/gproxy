//! Cross-instance config invalidation (§7.2, M8). Two halves:
//! [`broadcast`] announces a change (version stamp + pub/sub message, both
//! targets); [`spawn_invalidation_listener`] is the native push receiver
//! (redis + tokio::spawn). Edge has no listener — its `subscribe` is a no-op —
//! so it polls the version stamp instead (`http::edge::refresh_snapshot_if_stale`).

use crate::store::cache::{CONFIG_VERSION_KEY, CacheBackend, INVALIDATE_CHANNEL};

/// Announce a control-plane change to every other instance (§7.2): bump the
/// shared config-version stamp (edge isolates poll it), then fire the pub/sub
/// channel (native listeners reload at once; no-op on the REST cache
/// backends). `payload` is the usual hint (`config` / `cred:{id}`).
pub async fn broadcast(cache: &dyn CacheBackend, payload: &[u8]) {
    if cache.incr(CONFIG_VERSION_KEY, 1, None).await.is_err() {
        tracing::warn!(
            "config-version stamp bump failed; edge isolates may serve stale config \
             until the next successful broadcast"
        );
    }
    cache.publish(INVALIDATE_CHANNEL, payload).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_invalidation_listener(state: crate::app::AppState) {
    tokio::spawn(async move {
        let reloader = state.clone();
        state
            .cache
            .subscribe(
                INVALIDATE_CHANNEL,
                Box::new(move |_payload| {
                    let st = reloader.clone();
                    tokio::spawn(async move {
                        if let Err(e) = st.reload_snapshot().await {
                            tracing::warn!(error = %e, "snapshot reload after invalidation failed");
                        }
                    });
                }),
            )
            .await;
        // `subscribe` reconnects with backoff internally and only returns if it
        // gives up entirely (it currently never does on the redis backend; the
        // memory/edge no-op returns immediately).
        tracing::warn!("invalidation listener stopped");
    });
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;

    use crate::app::AppState;
    use crate::app::snapshot::ControlPlaneSnapshot;
    use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
    use crate::store::cache::{CacheBackend, MemoryCache};
    use crate::store::persistence::{FilePersistence, PersistenceBackend};

    /// §7.2: every broadcast must bump the shared config-version stamp the
    /// edge isolates poll (incr-by-0 reads the counter).
    #[tokio::test]
    async fn broadcast_bumps_config_version() {
        let cache = MemoryCache::new();
        super::broadcast(&cache, b"config").await;
        super::broadcast(&cache, b"cred:1").await;
        assert_eq!(
            cache
                .incr(crate::store::cache::CONFIG_VERSION_KEY, 0, None)
                .await,
            Ok(2)
        );
    }

    /// Spawning the listener over a memory cache must not panic and must leave
    /// the process healthy. Memory `subscribe` is a no-op, so the listener task
    /// just returns; this confirms the wiring compiles and runs end to end.
    #[tokio::test]
    async fn spawn_listener_memory_is_noop_and_healthy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let persistence: Arc<dyn PersistenceBackend> = Arc::new(
            FilePersistence::open(dir.path().to_path_buf())
                .await
                .expect("file persistence"),
        );
        let config = Arc::new(RuntimeConfig {
            host: "127.0.0.1".into(),
            port: 0,
            cache: CacheConfig::Memory,
            persistence: PersistenceConfig::File {
                data_dir: dir.path().to_path_buf(),
            },
            upstream: UpstreamConfig::from_proxy_url(None),
            instance_id: 0,
            max_attempts: crate::config::DEFAULT_MAX_ATTEMPTS,
            max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
            trusted_proxies: Vec::new(),
            update_repo: None,
            update_channel: "releases".to_string(),
            update_data_dir: dir.path().to_path_buf(),
            cors_origins: Vec::new(),
        });
        let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCache::new());
        let snapshot = Arc::new(ArcSwap::from_pointee(ControlPlaneSnapshot::empty(1)));
        let channels = Arc::new(crate::channel::registry::ChannelRegistry::with_builtin());
        let cipher = crate::crypto::cipher_from_master_key(Some(&B64.encode([7u8; 32]))).unwrap();
        let state = AppState::new(
            config,
            cache,
            persistence,
            Arc::new(NoopUpstream),
            snapshot,
            channels,
            cipher,
        );

        super::spawn_invalidation_listener(state.clone());
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        // No panic; snapshot pointer still readable.
        assert_eq!(state.cp().version, 1);
    }

    struct NoopUpstream;
    #[async_trait::async_trait]
    impl crate::http::client::UpstreamClient for NoopUpstream {
        async fn send(
            &self,
            _req: http::Request<bytes::Bytes>,
        ) -> Result<http::Response<bytes::Bytes>, crate::http::client::ClientError> {
            Err(crate::http::client::ClientError::Transport("noop".into()))
        }
    }
}
