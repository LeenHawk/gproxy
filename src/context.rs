use crate::cli::CliArgs;
use crate::config::AppConfig;
#[cfg(feature = "provider-aistudio")]
use crate::providers::aistudio::AIStudioStorage;
#[cfg(feature = "provider-antigravity")]
use crate::providers::antigravity::AntigravityStorage;
#[cfg(feature = "provider-claude")]
use crate::providers::claude::ClaudeStorage;
#[cfg(feature = "provider-claudecode")]
use crate::providers::claudecode::ClaudeCodeStorage;
#[cfg(feature = "provider-codex")]
use crate::providers::codex::CodexStorage;
#[cfg(feature = "provider-deepseek")]
use crate::providers::deepseek::DeepSeekStorage;
#[cfg(feature = "provider-geminicli")]
use crate::providers::geminicli::GeminiCliStorage;
#[cfg(feature = "provider-nvidia")]
use crate::providers::nvidia::NvidiaStorage;
#[cfg(feature = "provider-openai")]
use crate::providers::openai::OpenAIStorage;
#[cfg(feature = "provider-vertex")]
use crate::providers::vertex::VertexStorage;
#[cfg(feature = "provider-vertexexpress")]
use crate::providers::vertexexpress::VertexExpressStorage;
use crate::providers::credential_status::{
    CredentialStatus, CredentialStatusScheduler, ProviderKind, now_timestamp,
};
#[cfg(feature = "provider-claudecode")]
use crate::providers::claudecode::exchange_session_key;
use crate::usage::UsageStore;
use crate::storage::{ConfigStore, StorageConfig, StorageMode, StorageService, StorageSettings};
use anyhow::{Context, Result, anyhow};
use arc_swap::ArcSwap;
use axum::http::StatusCode;
use clap::Parser;
use std::future::Future;
use std::sync::{Arc, LazyLock};
use tokio::fs;
use tokio::sync::Mutex;
use tokio::sync::watch;
use tracing::info;
use wreq::{Client, Proxy};

fn build_http_client(config: &AppConfig) -> Result<Client> {
    let mut builder = Client::builder();
    if let Some(proxy) = config.app.proxy.as_ref() {
        let proxy =
            Proxy::all(proxy.as_str()).with_context(|| format!("invalid app.proxy {}", proxy))?;
        builder = builder.proxy(proxy);
    }
    builder.build().context("build wreq client")
}

static APP_CONTEXT: LazyLock<ArcSwap<AppContext>> = LazyLock::new(|| {
    let (reload_tx, _reload_rx) = watch::channel(0u64);
    ArcSwap::from_pointee(AppContext {
        app_config: ArcSwap::from_pointee(AppConfig::default()),
        http_client: ArcSwap::from_pointee(Client::new()),
        config_lock: Mutex::new(()),
        storage: StorageService::default(),
        usage_store: Arc::new(UsageStore::default()),
        reload_tx,
        status_scheduler: Arc::new(CredentialStatusScheduler::new()),
    })
});

pub struct AppContext {
    app_config: ArcSwap<AppConfig>,
    http_client: ArcSwap<Client>,
    config_lock: Mutex<()>,
    storage: StorageService,
    usage_store: Arc<UsageStore>,
    reload_tx: watch::Sender<u64>,
    status_scheduler: Arc<CredentialStatusScheduler>,
}

impl AppContext {
    pub async fn init() -> Result<()> {
        let (reload_tx, _reload_rx) = watch::channel(0u64);
        let storage_cli_args = CliArgs::parse();
        let storage_config = storage_cli_args.storage_config()?;
        let storage_service = StorageService::connect(&storage_config).await?;

        let local_path = storage_cli_args
            .file_path
            .clone()
            .unwrap_or_else(StorageSettings::default_file_path);
        let mut app_config = match &storage_service {
            #[cfg(feature = "storage-s3")]
            StorageService::S3(store) => {
                let remote_exists = store.object_exists().await?;
                if !remote_exists {
                    if fs::metadata(&local_path).await.is_ok() {
                        info!(
                            "s3 bootstrap: uploading config from {}",
                            local_path.display()
                        );
                        let contents = fs::read_to_string(&local_path).await?;
                        let mut config: AppConfig = toml::from_str(&contents)?;
                        if !matches!(config.storage.mode(), StorageMode::S3) {
                            return Err(anyhow!(
                                "local config storage mode is not s3: {}",
                                local_path.display()
                            ));
                        }
                        config.storage = storage_config.clone();
                        store.save_app_config(&config).await?;
                        store.flush_app_config().await?;
                        config
                    } else {
                        info!(
                            "s3 bootstrap: no local config found at {}, using defaults",
                            local_path.display()
                        );
                        store.load_app_config().await?
                    }
                } else {
                    info!("s3 bootstrap: remote config exists, skipping upload");
                    store.load_app_config().await?
                }
            }
            #[cfg(feature = "storage-db")]
            StorageService::Database(store) => {
                let has_config = store.app_config_exists().await?;
                if !has_config {
                    if fs::metadata(&local_path).await.is_ok() {
                        info!("db bootstrap: seeding config from {}", local_path.display());
                        let contents = fs::read_to_string(&local_path).await?;
                        let mut config: AppConfig = toml::from_str(&contents)?;
                        if !matches!(config.storage.mode(), StorageMode::Database) {
                            return Err(anyhow!(
                                "local config storage mode is not database: {}",
                                local_path.display()
                            ));
                        }
                        config.storage = storage_config.clone();
                        store.save_app_config(&config).await?;
                        store.flush_app_config().await?;
                        config
                    } else {
                        info!(
                            "db bootstrap: no local config found at {}, using defaults",
                            local_path.display()
                        );
                        store.load_app_config().await?
                    }
                } else {
                    store.load_app_config().await?
                }
            }
            _ => storage_service.load_app_config().await?,
        };
        let usage_storage_config =
            resolve_usage_storage_config(&storage_config, &storage_cli_args, &app_config);
        let usage_store = UsageStore::connect(&usage_storage_config).await?;
        app_config.storage = storage_config;
        let http_client = build_http_client(&app_config)?;
        let app_config = ArcSwap::from_pointee(app_config);
        let status_scheduler = Arc::new(CredentialStatusScheduler::new());
        APP_CONTEXT.store(Arc::new(AppContext {
            app_config,
            http_client: ArcSwap::from_pointee(http_client),
            config_lock: Mutex::new(()),
            storage: storage_service,
            usage_store: Arc::new(usage_store),
            reload_tx,
            status_scheduler: status_scheduler.clone(),
        }));
        status_scheduler.start(AppContext::get());
        #[cfg(feature = "provider-claudecode")]
        hydrate_claudecode_tokens(AppContext::get()).await;

        Ok(())
    }

    pub fn get() -> Arc<AppContext> {
        APP_CONTEXT.load_full()
    }

    pub fn reload_tx(&self) -> watch::Sender<u64> {
        self.reload_tx.clone()
    }

    pub fn get_config(&self) -> Arc<AppConfig> {
        self.app_config.load_full()
    }

    pub fn http_client(&self) -> Client {
        self.http_client.load_full().as_ref().clone()
    }

    pub async fn update_config<T, F>(&self, update: F) -> Result<T>
    where
        F: FnOnce(&mut AppConfig) -> T,
    {
        let _guard = self.config_lock.lock().await;
        let mut next = (*self.app_config.load_full()).clone();
        let out = update(&mut next);
        let http_client = build_http_client(&next)?;
        self.app_config.store(Arc::new(next.clone()));
        self.http_client.store(Arc::new(http_client));
        self.storage.save_app_config(&next).await?;
        Ok(out)
    }

    pub async fn update_config_async<T, F, Fut>(&self, update: F) -> Result<T>
    where
        F: FnOnce(&mut AppConfig) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let _guard = self.config_lock.lock().await;
        let mut next = (*self.app_config.load_full()).clone();
        let out = update(&mut next).await?;
        let http_client = build_http_client(&next)?;
        self.app_config.store(Arc::new(next.clone()));
        self.http_client.store(Arc::new(http_client));
        self.storage.save_app_config(&next).await?;
        Ok(out)
    }

    pub async fn flush_config(&self) -> Result<()> {
        self.storage.flush_app_config().await
    }

    pub async fn reload_config(&self) -> Result<AppConfig> {
        let config = self.storage.load_app_config().await?;
        let http_client = build_http_client(&config)?;
        self.app_config.store(Arc::new(config.clone()));
        self.http_client.store(Arc::new(http_client));
        Ok(config)
    }

    pub fn usage_store(&self) -> Arc<UsageStore> {
        self.usage_store.clone()
    }

    pub async fn schedule_usage_anchor(
        &self,
        provider: ProviderKind,
        view_name: &str,
        until: i64,
    ) {
        self.status_scheduler
            .schedule_usage_anchor(provider, view_name.to_string(), until)
            .await;
    }

    #[cfg(feature = "provider-openai")]
    pub fn openai(&self) -> OpenAIStorage<'_, StorageService> {
        OpenAIStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-codex")]
    pub fn codex(&self) -> CodexStorage<'_, StorageService> {
        CodexStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-claude")]
    pub fn claude(&self) -> ClaudeStorage<'_, StorageService> {
        ClaudeStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-claudecode")]
    pub fn claudecode(&self) -> ClaudeCodeStorage<'_, StorageService> {
        ClaudeCodeStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-aistudio")]
    pub fn aistudio(&self) -> AIStudioStorage<'_, StorageService> {
        AIStudioStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-vertex")]
    pub fn vertex(&self) -> VertexStorage<'_, StorageService> {
        VertexStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-vertexexpress")]
    pub fn vertexexpress(&self) -> VertexExpressStorage<'_, StorageService> {
        VertexExpressStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-geminicli")]
    pub fn geminicli(&self) -> GeminiCliStorage<'_, StorageService> {
        GeminiCliStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-antigravity")]
    pub fn antigravity(&self) -> AntigravityStorage<'_, StorageService> {
        AntigravityStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-nvidia")]
    pub fn nvidia(&self) -> NvidiaStorage<'_, StorageService> {
        NvidiaStorage::new(&self.storage)
    }

    #[cfg(feature = "provider-deepseek")]
    pub fn deepseek(&self) -> DeepSeekStorage<'_, StorageService> {
        DeepSeekStorage::new(&self.storage)
    }

    pub async fn update_credential_status_by_id<F>(
        &self,
        provider: ProviderKind,
        id: &str,
        model: &str,
        update: F,
    ) -> Result<(), StatusCode>
    where
        F: FnOnce(&CredentialStatus, i64) -> Option<CredentialStatus> + Send,
    {
        let now = now_timestamp();
        let mut next_status: Option<CredentialStatus> = None;
        let model = if model.trim().is_empty() {
            crate::providers::credential_status::DEFAULT_MODEL_KEY
        } else {
            model
        };
        match provider {
            ProviderKind::OpenAI => {
                self.openai()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::Claude => {
                self.claude()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::AIStudio => {
                self.aistudio()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::DeepSeek => {
                self.deepseek()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::Nvidia => {
                self.nvidia()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::VertexExpress => {
                self.vertexexpress()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::ClaudeCode => {
                self.claudecode()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::Codex => {
                self.codex()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::Vertex => {
                self.vertex()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::GeminiCli => {
                self.geminicli()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            ProviderKind::Antigravity => {
                self.antigravity()
                    .update_credential_by_id(id, |cred| {
                        let prev = cred.states.effective_status(model);
                        if let Some(status) = update(&prev, now) {
                            cred.states.update_status(model, status.clone());
                            next_status = Some(status);
                        }
                    })
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }

        if let Some(status) = next_status
            && let Some(until) = status.until() {
                self.status_scheduler
                    .schedule(provider, id.to_string(), model.to_string(), until)
                    .await;
            }

        Ok(())
    }
}

fn resolve_usage_storage_config(
    storage_config: &StorageConfig,
    cli_args: &CliArgs,
    app_config: &AppConfig,
) -> StorageConfig {
    let mut resolved = storage_config.clone();
    #[cfg(feature = "storage-file")]
    if matches!(resolved.mode, StorageMode::File) {
        if cli_args.file_data_dir.is_none()
            && let Some(StorageSettings::File {
                data_dir: config_data_dir,
                ..
            }) = app_config.storage.settings()
                && let Some(StorageSettings::File {
                    data_dir: resolved_data_dir,
                    ..
                }) = resolved.settings.as_mut()
                && config_data_dir.is_some()
            {
                *resolved_data_dir = config_data_dir.clone();
            }
        if let Some(StorageSettings::File {
            data_dir,
            ..
        }) = resolved.settings.as_mut()
            && data_dir.is_none() {
                *data_dir = StorageSettings::default_usage_data_dir();
            }
    }
    resolved
}

#[cfg(feature = "provider-claudecode")]
async fn hydrate_claudecode_tokens(ctx: Arc<AppContext>) {
    let storage = ctx.claudecode();
    let setting = match storage.get_config().await {
        Ok(setting) => setting,
        Err(_) => return,
    };
    let credentials = match storage.get_credentials().await {
        Ok(credentials) => credentials,
        Err(_) => return,
    };
    let now = now_timestamp();
    for (index, credential) in credentials.into_iter().enumerate() {
        if credential.session_key.trim().is_empty() {
            continue;
        }
        let has_tokens = !credential.refresh_token.trim().is_empty()
            && !credential.access_token.trim().is_empty();
        let expired = credential.expires_at > 0 && credential.expires_at <= now;
        if has_tokens && !expired {
            continue;
        }
        match exchange_session_key(ctx.as_ref(), &credential, &setting.base_url).await {
            Ok(tokens) => {
                let _ = storage
                    .update_credential(index, move |stored| {
                        stored.refresh_token = tokens.refresh_token;
                        stored.access_token = tokens.access_token;
                        stored.expires_at = tokens.expires_at;
                    })
                    .await;
            }
            Err(status) => {
                tracing::warn!(
                    "claudecode session_key exchange failed at startup: status={}",
                    status.as_u16()
                );
            }
        }
    }
}
