use crate::Shared;
use crate::cache::InProcessCache;
use crate::control::SnapshotControl;
use crate::host::{AppHost, Services};
use crate::lifecycle::AppInner;
use crate::{App, AppError, AppHandle, Config};
use gproxy_channel_api::{Channel, ChannelRegistry};
#[cfg(not(target_arch = "wasm32"))]
use sha2::{Digest, Sha256};

impl App {
    pub async fn start(config: Config) -> Result<AppHandle, AppError> {
        #[cfg(not(target_arch = "wasm32"))]
        std::fs::create_dir_all(config.data_dir())
            .map_err(|error| AppError::Bootstrap(error.to_string()))?;
        let store = gproxy_store::Store::open(config.backend_config()).await?;
        let cipher = crate::key_rotation::prepare(&store, config.secret_keys()).await?;
        let channels = channels()?;
        #[cfg(not(target_arch = "wasm32"))]
        seed_first_run(&store, &channels, config.native()).await?;
        let runtime = crate::control::RuntimeOverrides::from_config(&config);
        let control = SnapshotControl::new(store.clone(), runtime).await?;
        let cache = InProcessCache::default();
        #[cfg(not(target_arch = "wasm32"))]
        let transport =
            gproxy_upstream::Transport::with_system_proxy(control.settings().inherit_system_proxy);
        #[cfg(target_arch = "wasm32")]
        let transport = gproxy_upstream::Transport::default();
        #[cfg(not(target_arch = "wasm32"))]
        let tokenizers = crate::host::tokenizers::build(
            store.clone(),
            transport.clone(),
            control.settings().enable_tokenizer_download,
        );
        let services = Shared::new(Services {
            store,
            cache,
            cipher,
            control,
            transport,
            health_sequence: std::sync::atomic::AtomicU64::new(0),
            #[cfg(not(target_arch = "wasm32"))]
            tokenizers,
            #[cfg(not(target_arch = "wasm32"))]
            spawner: crate::host::TokioSpawner,
            #[cfg(not(target_arch = "wasm32"))]
            continuations: Default::default(),
        });
        let host = AppHost { services };
        let core = gproxy_core::Core::new(host.clone(), channels)?;
        crate::cleanup::schedule(&host);
        #[cfg(not(target_arch = "wasm32"))]
        let shutdown = tokio::sync::watch::channel(false).0;
        #[cfg(target_arch = "wasm32")]
        let shutdown = std::sync::atomic::AtomicBool::new(false);
        Ok(AppHandle {
            inner: Shared::new(AppInner {
                core,
                host,
                shutdown,
            }),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn seed_first_run(
    store: &gproxy_store::Store,
    channels: &ChannelRegistry,
    options: &crate::config::NativeOptions,
) -> Result<(), AppError> {
    if store.has_admin_accounts().await? {
        return Ok(());
    }
    let Some(password) = options.admin_password.as_deref() else {
        if options.bootstrap_admin_api_key.is_some() || !options.bootstrap_channels.is_empty() {
            return Err(AppError::Bootstrap(
                "bootstrap API key and channels require GPROXY_ADMIN_PASSWORD on a fresh store"
                    .into(),
            ));
        }
        return Ok(());
    };
    if options
        .bootstrap_admin_api_key
        .as_deref()
        .is_some_and(|key| key.trim().is_empty())
    {
        return Err(AppError::Bootstrap(
            "GPROXY_BOOTSTRAP_ADMIN_API_KEY must not be blank".into(),
        ));
    }
    if let Some(channel) = options
        .bootstrap_channels
        .iter()
        .find(|channel| channels.get(channel).is_none())
    {
        return Err(AppError::Bootstrap(format!(
            "unknown bootstrap channel: {channel}"
        )));
    }
    let admin_id = gproxy_admin::seed_first_admin(store, &options.admin_user, password)
        .await
        .map_err(|error| AppError::Bootstrap(error.to_string()))?
        .ok_or_else(|| AppError::Bootstrap("administrator was created concurrently".into()))?;
    if let Some(api_key) = options.bootstrap_admin_api_key.as_deref() {
        store
            .create_admin_api_key(&Sha256::digest(api_key.as_bytes()), admin_id, unix_now())
            .await?;
    }
    for channel in &options.bootstrap_channels {
        store
            .insert_provider(&gproxy_store::records::ProviderInput {
                name: channel.clone(),
                label: None,
                channel: channel.clone(),
                settings: serde_json::json!({}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: true,
            })
            .await?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn channels() -> Result<ChannelRegistry, gproxy_channel_api::registry::DuplicateChannel> {
    let channels = vec![
        Box::new(gproxy_channels::OpenAiChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::AntigravityChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::ClaudeApiChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::ClaudeCodeChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::GeminiCliChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::ClineChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::CloudflareAiGatewayChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::CodexChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::CopilotCliChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::CustomChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::DashScopeChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::DeepSeekChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::GroqChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::GrokBuildChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::KiroChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::KimiChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::NvidiaChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::OpenCodeChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::OpenRouterChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::AiStudioChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::AzureChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::AwsBedrockChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::VertexChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::VertexExpressChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::WorkBuddyChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::XaiChannel) as Box<dyn Channel>,
        Box::new(gproxy_channels::VercelChannel) as Box<dyn Channel>,
    ];
    #[cfg(not(target_arch = "wasm32"))]
    let channels = {
        let mut channels = channels;
        channels.push(Box::new(gproxy_channels::ClaudeWebChannel));
        channels
    };
    ChannelRegistry::new(channels)
}
