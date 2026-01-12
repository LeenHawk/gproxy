use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectOptions, ConnectionTrait, Database,
    DatabaseConnection, EntityTrait, QueryFilter, Schema, Set, TransactionTrait,
};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{self, Duration, Instant};
use url::Url;

use crate::config::{AppConfig, AppSection};
use crate::providers::ProvidersConfig;
use crate::providers::{
    aistudio::storage::database as aistudio_db, antigravity::storage::database as antigravity_db,
    claude::storage::database as claude_db, claudecode::storage::database as claudecode_db,
    codex::storage::database as codex_db, deepseek::storage::database as deepseek_db,
    geminicli::storage::database as geminicli_db, nvidia::storage::database as nvidia_db,
    openai::storage::database as openai_db, vertex::storage::database as vertex_db,
    vertexexpress::storage::database as vertexexpress_db,
};
use crate::usage::database as usage_db;
use crate::storage::{ConfigStore, StorageSettings};

mod app;

pub struct DatabaseStorage {
    connection: DatabaseConnection,
    tx: mpsc::Sender<WriteRequest>,
    config: ArcSwap<AppConfig>,
    update_lock: Mutex<()>,
    debounce: Duration,
}

impl DatabaseStorage {
    pub async fn connect(settings: &StorageSettings) -> Result<Self> {
        let StorageSettings::Database {
            uri,
            max_connections,
            min_connections,
            connect_timeout_secs,
            schema,
            ssl_mode: _,
            debounce_secs,
        } = settings
        else {
            return Err(anyhow!("storage settings mismatch for database"));
        };

        let mut options = ConnectOptions::new(uri.to_string());
        if let Some(max_connections) = max_connections {
            options.max_connections(*max_connections);
        }
        if let Some(min_connections) = min_connections {
            options.min_connections(*min_connections);
        }
        if let Some(connect_timeout_secs) = connect_timeout_secs {
            options.connect_timeout(Duration::from_secs(*connect_timeout_secs));
        }
        if let Some(schema) = schema {
            options.set_schema_search_path(schema.clone());
        }

        let connection = Database::connect(options).await?;
        init_schema(&connection).await?;

        let debounce = Duration::from_secs(*debounce_secs);
        let (tx, rx) = mpsc::channel(32);
        let worker_conn = connection.clone();
        tokio::spawn(async move {
            write_worker(worker_conn, rx, debounce).await;
        });

        Ok(Self {
            connection,
            tx,
            config: ArcSwap::from_pointee(AppConfig::default()),
            update_lock: Mutex::new(()),
            debounce,
        })
    }

    pub fn debounce(&self) -> Duration {
        self.debounce
    }

    pub(crate) async fn app_config_exists(&self) -> Result<bool> {
        Ok(app::Entity::find_by_id(1)
            .one(&self.connection)
            .await?
            .is_some())
    }

    pub async fn providers_get<R, F>(&self, _read: F) -> Result<R>
    where
        F: FnOnce(&ProvidersConfig) -> R + Send,
    {
        let config = self.config.load_full();
        Ok(_read(&config.providers))
    }

    pub async fn providers_update<F>(&self, _update: F) -> Result<()>
    where
        F: FnOnce(&mut ProvidersConfig, Option<&mut toml_edit::DocumentMut>) -> Result<()> + Send,
    {
        let _guard = self.update_lock.lock().await;
        let mut config = (*self.config.load_full()).clone();
        _update(&mut config.providers, None)?;
        self.save_app_config(&config).await?;
        Ok(())
    }

    pub(crate) async fn lock_update(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.update_lock.lock().await
    }

    pub(crate) fn update_cached_config<F>(&self, update: F) -> AppConfig
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut next = (*self.config.load_full()).clone();
        update(&mut next);
        self.config.store(Arc::new(next.clone()));
        next
    }
}

#[async_trait::async_trait]
impl crate::storage::ConfigStore for DatabaseStorage {
    async fn get_app_config(&self) -> Result<AppConfig> {
        Ok((*self.config.load_full()).clone())
    }

    async fn load_app_config(&self) -> Result<AppConfig> {
        let app = load_app_section(&self.connection).await?;
        let providers = load_providers(&self.connection).await?;
        let config = AppConfig {
            app,
            storage: crate::storage::StorageConfig::default(),
            providers,
        };
        self.config.store(Arc::new(config.clone()));
        Ok(config)
    }

    async fn save_app_config(&self, config: &AppConfig) -> Result<()> {
        self.config.store(Arc::new(config.clone()));
        self.tx
            .send(WriteRequest::Write(Box::new(config.clone())))
            .await
            .map_err(|_| anyhow!("storage writer task closed"))?;
        Ok(())
    }

    async fn flush_app_config(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(WriteRequest::Flush(tx))
            .await
            .map_err(|_| anyhow!("storage writer task closed"))?;
        rx.await
            .map_err(|_| anyhow!("storage writer task closed"))?
    }
}

enum WriteRequest {
    Write(Box<AppConfig>),
    Flush(oneshot::Sender<Result<()>>),
}

async fn write_worker(
    connection: DatabaseConnection,
    mut rx: mpsc::Receiver<WriteRequest>,
    debounce: Duration,
) {
    while let Some(request) = rx.recv().await {
        let mut flush_waiters: Vec<oneshot::Sender<Result<()>>> = Vec::new();
        let mut pending = match request {
            WriteRequest::Write(config) => Some(config),
            WriteRequest::Flush(tx) => {
                let _ = tx.send(Ok(()));
                continue;
            }
        };

        if debounce.is_zero() {
            loop {
                match rx.try_recv() {
                    Ok(WriteRequest::Write(config)) => {
                        pending = Some(config);
                    }
                    Ok(WriteRequest::Flush(tx)) => {
                        flush_waiters.push(tx);
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                }
            }
        } else {
            let deadline = Instant::now() + debounce;
            loop {
                let sleep = time::sleep_until(deadline);
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => break,
                    message = rx.recv() => {
                        match message {
                            Some(WriteRequest::Write(config)) => {
                                pending = Some(config);
                            }
                            Some(WriteRequest::Flush(tx)) => {
                                flush_waiters.push(tx);
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }
        }

        if let Some(config) = pending {
            let result = write_app_config(&connection, config.as_ref()).await;
            let err_msg = result.as_ref().err().map(|err| err.to_string());
            for waiter in flush_waiters {
                let send_result = match &err_msg {
                    Some(msg) => Err(anyhow!(msg.clone())),
                    None => Ok(()),
                };
                let _ = waiter.send(send_result);
            }
        }
    }
}

async fn init_schema(connection: &DatabaseConnection) -> Result<()> {
    let backend = connection.get_database_backend();
    let schema = Schema::new(backend);

    let mut app_table: sea_orm::sea_query::TableCreateStatement =
        schema.create_table_from_entity(app::Entity);
    app_table.if_not_exists();
    connection.execute(&app_table).await?;

    let mut api_table: sea_orm::sea_query::TableCreateStatement =
        schema.create_table_from_entity(app::api_key::Entity);
    api_table.if_not_exists();
    connection.execute(&api_table).await?;

    let mut openai_config = schema.create_table_from_entity(openai_db::config::Entity);
    openai_config.if_not_exists();
    connection.execute(&openai_config).await?;
    let mut openai_credentials = schema.create_table_from_entity(openai_db::credential::Entity);
    openai_credentials.if_not_exists();
    connection.execute(&openai_credentials).await?;

    let mut codex_config = schema.create_table_from_entity(codex_db::config::Entity);
    codex_config.if_not_exists();
    connection.execute(&codex_config).await?;
    let mut codex_credentials = schema.create_table_from_entity(codex_db::credential::Entity);
    codex_credentials.if_not_exists();
    connection.execute(&codex_credentials).await?;

    let mut claude_config = schema.create_table_from_entity(claude_db::config::Entity);
    claude_config.if_not_exists();
    connection.execute(&claude_config).await?;
    let mut claude_credentials = schema.create_table_from_entity(claude_db::credential::Entity);
    claude_credentials.if_not_exists();
    connection.execute(&claude_credentials).await?;

    let mut claudecode_config = schema.create_table_from_entity(claudecode_db::config::Entity);
    claudecode_config.if_not_exists();
    connection.execute(&claudecode_config).await?;
    let mut claudecode_credentials =
        schema.create_table_from_entity(claudecode_db::credential::Entity);
    claudecode_credentials.if_not_exists();
    connection.execute(&claudecode_credentials).await?;

    let mut aistudio_config = schema.create_table_from_entity(aistudio_db::config::Entity);
    aistudio_config.if_not_exists();
    connection.execute(&aistudio_config).await?;
    let mut aistudio_credentials = schema.create_table_from_entity(aistudio_db::credential::Entity);
    aistudio_credentials.if_not_exists();
    connection.execute(&aistudio_credentials).await?;

    let mut vertex_config = schema.create_table_from_entity(vertex_db::config::Entity);
    vertex_config.if_not_exists();
    connection.execute(&vertex_config).await?;
    let mut vertex_credentials = schema.create_table_from_entity(vertex_db::credential::Entity);
    vertex_credentials.if_not_exists();
    connection.execute(&vertex_credentials).await?;

    let mut vertexexpress_config =
        schema.create_table_from_entity(vertexexpress_db::config::Entity);
    vertexexpress_config.if_not_exists();
    connection.execute(&vertexexpress_config).await?;
    let mut vertexexpress_credentials =
        schema.create_table_from_entity(vertexexpress_db::credential::Entity);
    vertexexpress_credentials.if_not_exists();
    connection.execute(&vertexexpress_credentials).await?;

    let mut geminicli_config = schema.create_table_from_entity(geminicli_db::config::Entity);
    geminicli_config.if_not_exists();
    connection.execute(&geminicli_config).await?;
    let mut geminicli_credentials =
        schema.create_table_from_entity(geminicli_db::credential::Entity);
    geminicli_credentials.if_not_exists();
    connection.execute(&geminicli_credentials).await?;

    let mut antigravity_config = schema.create_table_from_entity(antigravity_db::config::Entity);
    antigravity_config.if_not_exists();
    connection.execute(&antigravity_config).await?;
    let mut antigravity_credentials =
        schema.create_table_from_entity(antigravity_db::credential::Entity);
    antigravity_credentials.if_not_exists();
    connection.execute(&antigravity_credentials).await?;

    let mut nvidia_config = schema.create_table_from_entity(nvidia_db::config::Entity);
    nvidia_config.if_not_exists();
    connection.execute(&nvidia_config).await?;
    let mut nvidia_credentials = schema.create_table_from_entity(nvidia_db::credential::Entity);
    nvidia_credentials.if_not_exists();
    connection.execute(&nvidia_credentials).await?;

    let mut deepseek_config = schema.create_table_from_entity(deepseek_db::config::Entity);
    deepseek_config.if_not_exists();
    connection.execute(&deepseek_config).await?;
    let mut deepseek_credentials = schema.create_table_from_entity(deepseek_db::credential::Entity);
    deepseek_credentials.if_not_exists();
    connection.execute(&deepseek_credentials).await?;

    let mut usage_records = schema.create_table_from_entity(usage_db::record_entity::Entity);
    usage_records.if_not_exists();
    connection.execute(&usage_records).await?;

    Ok(())
}


async fn load_app_section(connection: &DatabaseConnection) -> Result<AppSection> {
    let model = app::Entity::find_by_id(1).one(connection).await?;
    let Some(model) = model else {
        let app = AppSection::default();
        write_app_section(connection, &app).await?;
        return Ok(app);
    };

    let host: IpAddr = model.host.parse()?;
    let proxy = match model.proxy {
        Some(proxy) => Some(Url::parse(&proxy)?),
        None => None,
    };
    let api_keys = load_api_keys(connection).await?;

    Ok(AppSection {
        host,
        port: model.port as u16,
        admin_key: model.admin_key,
        api_keys,
        proxy,
    })
}

async fn load_api_keys(connection: &DatabaseConnection) -> Result<Vec<String>> {
    let keys = app::api_key::Entity::find()
        .filter(app::api_key::Column::AppId.eq(1))
        .all(connection)
        .await?;
    Ok(keys.into_iter().map(|row| row.key).collect())
}

async fn load_providers(connection: &DatabaseConnection) -> Result<ProvidersConfig> {
    let mut providers = ProvidersConfig::default();

    providers.openai.setting = openai_db::load_setting(connection).await?;
    providers.openai.credentials = openai_db::load_credentials(connection).await?;

    providers.codex.setting = codex_db::load_setting(connection).await?;
    providers.codex.credentials = codex_db::load_credentials(connection).await?;

    providers.claude.setting = claude_db::load_setting(connection).await?;
    providers.claude.credentials = claude_db::load_credentials(connection).await?;

    providers.claudecode.setting = claudecode_db::load_setting(connection).await?;
    providers.claudecode.credentials = claudecode_db::load_credentials(connection).await?;

    providers.aistudio.setting = aistudio_db::load_setting(connection).await?;
    providers.aistudio.credentials = aistudio_db::load_credentials(connection).await?;

    providers.vertex.setting = vertex_db::load_setting(connection).await?;
    providers.vertex.credentials = vertex_db::load_credentials(connection).await?;

    providers.vertexexpress.setting = vertexexpress_db::load_setting(connection).await?;
    providers.vertexexpress.credentials = vertexexpress_db::load_credentials(connection).await?;

    providers.geminicli.setting = geminicli_db::load_setting(connection).await?;
    providers.geminicli.credentials = geminicli_db::load_credentials(connection).await?;

    providers.antigravity.setting = antigravity_db::load_setting(connection).await?;
    providers.antigravity.credentials = antigravity_db::load_credentials(connection).await?;

    providers.nvidia.setting = nvidia_db::load_setting(connection).await?;
    providers.nvidia.credentials = nvidia_db::load_credentials(connection).await?;

    providers.deepseek.setting = deepseek_db::load_setting(connection).await?;
    providers.deepseek.credentials = deepseek_db::load_credentials(connection).await?;

    Ok(providers)
}

async fn write_providers<C>(connection: &C, providers: &ProvidersConfig) -> Result<()>
where
    C: ConnectionTrait,
{
    openai_db::write_setting(connection, &providers.openai.setting).await?;
    openai_db::write_credentials(connection, &providers.openai.credentials).await?;

    codex_db::write_setting(connection, &providers.codex.setting).await?;
    codex_db::write_credentials(connection, &providers.codex.credentials).await?;

    claude_db::write_setting(connection, &providers.claude.setting).await?;
    claude_db::write_credentials(connection, &providers.claude.credentials).await?;

    claudecode_db::write_setting(connection, &providers.claudecode.setting).await?;
    claudecode_db::write_credentials(connection, &providers.claudecode.credentials).await?;

    aistudio_db::write_setting(connection, &providers.aistudio.setting).await?;
    aistudio_db::write_credentials(connection, &providers.aistudio.credentials).await?;

    vertex_db::write_setting(connection, &providers.vertex.setting).await?;
    vertex_db::write_credentials(connection, &providers.vertex.credentials).await?;

    vertexexpress_db::write_setting(connection, &providers.vertexexpress.setting).await?;
    vertexexpress_db::write_credentials(connection, &providers.vertexexpress.credentials).await?;

    geminicli_db::write_setting(connection, &providers.geminicli.setting).await?;
    geminicli_db::write_credentials(connection, &providers.geminicli.credentials).await?;

    antigravity_db::write_setting(connection, &providers.antigravity.setting).await?;
    antigravity_db::write_credentials(connection, &providers.antigravity.credentials).await?;

    nvidia_db::write_setting(connection, &providers.nvidia.setting).await?;
    nvidia_db::write_credentials(connection, &providers.nvidia.credentials).await?;

    deepseek_db::write_setting(connection, &providers.deepseek.setting).await?;
    deepseek_db::write_credentials(connection, &providers.deepseek.credentials).await?;

    Ok(())
}

async fn write_app_section<C>(connection: &C, app: &AppSection) -> Result<()>
where
    C: ConnectionTrait,
{
    let model = app::Entity::find_by_id(1).one(connection).await?;
    let proxy = app.proxy.as_ref().map(Url::to_string);
    if let Some(model) = model {
        let mut active: app::ActiveModel = model.into();
        active.host = Set(app.host.to_string());
        active.port = Set(app.port as i32);
        active.admin_key = Set(app.admin_key.clone());
        active.proxy = Set(proxy);
        active.update(connection).await?;
    } else {
        let active = app::ActiveModel {
            id: Set(1),
            host: Set(app.host.to_string()),
            port: Set(app.port as i32),
            admin_key: Set(app.admin_key.clone()),
            proxy: Set(proxy),
        };
        active.insert(connection).await?;
    }

    app::api_key::Entity::delete_many()
        .filter(app::api_key::Column::AppId.eq(1))
        .exec(connection)
        .await?;

    for key in &app.api_keys {
        let active = app::api_key::ActiveModel {
            id: ActiveValue::NotSet,
            app_id: Set(1),
            key: Set(key.clone()),
        };
        active.insert(connection).await?;
    }

    Ok(())
}

async fn write_app_config(connection: &DatabaseConnection, config: &AppConfig) -> Result<()> {
    let txn = connection.begin().await?;
    write_app_section(&txn, &config.app).await?;
    write_providers(&txn, &config.providers).await?;
    txn.commit().await?;
    Ok(())
}
