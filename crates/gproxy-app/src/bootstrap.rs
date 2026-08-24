use std::sync::Arc;

use gproxy_channel_api::{Channel, ChannelRegistry};
use gproxy_core::CacheBackend;

use crate::cache::InProcessCache;
use crate::control::SnapshotControl;
use crate::host::{AppHost, Services};
use crate::lifecycle::AppInner;
use crate::secrets::EnvelopeCipher;
use crate::{App, AppError, AppHandle, Config};

impl App {
    pub async fn start(config: Config) -> Result<AppHandle, AppError> {
        #[cfg(not(target_arch = "wasm32"))]
        std::fs::create_dir_all(config.data_dir())
            .map_err(|error| AppError::Bootstrap(error.to_string()))?;
        let store = gproxy_store::Store::open(config.backend_config()).await?;
        let control = SnapshotControl::new(store.clone()).await?;
        let cache = InProcessCache::default();
        seed_quota_cache(&store, &control, &cache).await?;
        let services = Arc::new(Services {
            store,
            cache,
            cipher: EnvelopeCipher::new(*config.master_key()),
            control,
            transport: gproxy_upstream::Transport::default(),
            #[cfg(not(target_arch = "wasm32"))]
            spawner: crate::host::TokioSpawner,
        });
        let host = AppHost { services };
        let core = gproxy_core::Core::new(host.clone(), channels()?)?;
        #[cfg(not(target_arch = "wasm32"))]
        let shutdown = tokio::sync::watch::channel(false).0;
        #[cfg(target_arch = "wasm32")]
        let shutdown = std::sync::atomic::AtomicBool::new(false);
        Ok(AppHandle {
            inner: Arc::new(AppInner {
                core,
                host,
                shutdown,
            }),
        })
    }
}

fn channels() -> Result<ChannelRegistry, gproxy_channel_api::registry::DuplicateChannel> {
    ChannelRegistry::new([
        Box::new(gproxy_channels::OpenAiChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::ClaudeCodeChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::CodexChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::AiStudioChannel) as Box<dyn Channel>,
    ])
}

async fn seed_quota_cache(
    store: &gproxy_store::Store,
    control: &SnapshotControl,
    cache: &InProcessCache,
) -> Result<(), AppError> {
    let now = unix_now()?;
    let snapshot = control.current();
    for window in store.quota_windows().await? {
        let Some(quota) = snapshot
            .quotas
            .iter()
            .find(|quota| quota.id == window.quota_id)
        else {
            continue;
        };
        let end = window
            .window_start
            .saturating_add(i64::try_from(quota.window_seconds).unwrap_or(i64::MAX));
        let Some(ttl) = end.checked_sub(now).filter(|ttl| *ttl > 0) else {
            continue;
        };
        let used = i64::try_from(window.used_tokens)
            .map_err(|error| AppError::Cache(error.to_string()))?;
        cache
            .set(
                &format!("gproxy:quota:{}:{}", quota.id, window.window_start),
                used.to_be_bytes().to_vec(),
                Some(std::time::Duration::from_secs(ttl as u64)),
            )
            .await
            .map_err(|error| AppError::Cache(error.to_string()))?;
    }
    Ok(())
}

fn unix_now() -> Result<i64, AppError> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| AppError::Cache(error.to_string()))?
        .as_secs();
    i64::try_from(seconds).map_err(|error| AppError::Cache(error.to_string()))
}
