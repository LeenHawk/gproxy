use std::sync::Arc;

use crate::cache::InProcessCache;
use crate::control::SnapshotControl;
use crate::host::{AppHost, Services};
use crate::lifecycle::AppInner;
use crate::secrets::EnvelopeCipher;
use crate::{App, AppError, AppHandle, Config};
use gproxy_channel_api::{Channel, ChannelRegistry};

impl App {
    pub async fn start(config: Config) -> Result<AppHandle, AppError> {
        #[cfg(not(target_arch = "wasm32"))]
        std::fs::create_dir_all(config.data_dir())
            .map_err(|error| AppError::Bootstrap(error.to_string()))?;
        let store = gproxy_store::Store::open(config.backend_config()).await?;
        let control = SnapshotControl::new(store.clone()).await?;
        let cache = InProcessCache::default();
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
