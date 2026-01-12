use anyhow::{Result, anyhow};
use async_trait::async_trait;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use toml_edit::DocumentMut;
use url::Url;

use crate::config::AppConfig;
use crate::providers::ProvidersConfig;

// Macros in this module:
// - storage_match!: dispatches a StorageService call to an enabled backend.

#[macro_export]
macro_rules! storage_match {
    ($self:expr, $method:ident ( $($arg:expr),* $(,)? )) => {{
        match $self {
            #[cfg(feature = "storage-memory")]
            $crate::storage::StorageService::Memory(store) => store.$method($($arg),*).await,
            #[cfg(feature = "storage-file")]
            $crate::storage::StorageService::File(store) => store.$method($($arg),*).await,
            #[cfg(feature = "storage-db")]
            $crate::storage::StorageService::Database(store) => store.$method($($arg),*).await,
            #[cfg(feature = "storage-s3")]
            $crate::storage::StorageService::S3(store) => store.$method($($arg),*).await,
            $crate::storage::StorageService::Unconfigured => {
                Err(anyhow::anyhow!("storage not configured"))
            }
        }
    }};
}

#[cfg(feature = "storage-file")]
mod file;
#[cfg(feature = "storage-file")]
pub(crate) use file::FileStorage;

#[cfg(feature = "storage-db")]
mod db;
#[cfg(feature = "storage-db")]
pub use db::DatabaseStorage;

#[cfg(feature = "storage-memory")]
mod memory;
#[cfg(feature = "storage-memory")]
pub use memory::MemoryStorage;

#[cfg(feature = "storage-s3")]
mod s3;
#[cfg(feature = "storage-s3")]
pub use s3::S3Storage;

pub(crate) mod toml_states;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageConfig {
    #[serde(default)]
    pub(crate) mode: StorageMode,
    #[serde(flatten)]
    #[serde(default)]
    pub(crate) settings: Option<StorageSettings>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let mode = StorageMode::default();
        let settings = match mode {
            #[cfg(feature = "storage-file")]
            StorageMode::File => Some(StorageSettings::File {
                path: StorageSettings::default_file_path(),
                data_dir: StorageSettings::default_usage_data_dir(),
                debounce_secs: StorageSettings::default_file_debounce_secs(),
            }),
            #[cfg(feature = "storage-memory")]
            StorageMode::Memory => None,
            #[cfg(feature = "storage-db")]
            StorageMode::Database => Some(StorageSettings::Database {
                uri: StorageSettings::default_database_uri(),
                max_connections: None,
                min_connections: None,
                connect_timeout_secs: None,
                schema: None,
                ssl_mode: None,
                debounce_secs: StorageSettings::default_db_debounce_secs(),
            }),
            #[cfg(feature = "storage-s3")]
            StorageMode::S3 => Some(StorageSettings::S3 {
                bucket: "gproxy".to_string(),
                region: "us-east-1".to_string(),
                access_key: String::new(),
                secret_key: String::new(),
                endpoint: None,
                path_style: None,
                session_token: None,
                use_tls: None,
                path: StorageSettings::default_s3_path(),
                debounce_secs: StorageSettings::default_s3_debounce_secs(),
            }),
        };
        Self { mode, settings }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    #[cfg(feature = "storage-memory")]
    Memory,
    #[cfg(feature = "storage-file")]
    File,
    #[cfg(feature = "storage-db")]
    Database,
    #[cfg(feature = "storage-s3")]
    S3,
}

#[allow(clippy::derivable_impls)]
impl Default for StorageMode {
    fn default() -> Self {
        #[cfg(feature = "storage-file")]
        {
            StorageMode::File
        }
        #[cfg(all(not(feature = "storage-file"), feature = "storage-memory"))]
        {
            StorageMode::Memory
        }
        #[cfg(all(
            not(feature = "storage-file"),
            not(feature = "storage-memory"),
            feature = "storage-db"
        ))]
        {
            StorageMode::Database
        }
        #[cfg(all(
            not(feature = "storage-file"),
            not(feature = "storage-memory"),
            not(feature = "storage-db"),
            feature = "storage-s3"
        ))]
        {
            StorageMode::S3
        }
        #[cfg(not(any(
            feature = "storage-file",
            feature = "storage-memory",
            feature = "storage-db",
            feature = "storage-s3"
        )))]
        unreachable!("storage backend default should be configured by features")
    }
}

pub enum StorageService {
    #[cfg(feature = "storage-memory")]
    Memory(MemoryStorage),
    #[cfg(feature = "storage-file")]
    File(Box<FileStorage>),
    #[cfg(feature = "storage-db")]
    Database(DatabaseStorage),
    #[cfg(feature = "storage-s3")]
    S3(Box<S3Storage>),
    Unconfigured,
}

impl Default for StorageService {
    fn default() -> Self {
        #[cfg(feature = "storage-memory")]
        {
            StorageService::Memory(MemoryStorage::default())
        }
        #[cfg(all(not(feature = "storage-memory"), feature = "storage-file"))]
        {
            let store = FileStorage::new(
                StorageSettings::default_file_path(),
                Duration::from_secs(StorageSettings::default_file_debounce_secs()),
            )
            .expect("default file storage");
            StorageService::File(Box::new(store))
        }
        #[cfg(all(not(feature = "storage-memory"), not(feature = "storage-file")))]
        {
            StorageService::Unconfigured
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum StorageSettings {
    #[cfg(feature = "storage-file")]
    File {
        path: PathBuf,
        #[serde(default)]
        data_dir: Option<PathBuf>,
        #[serde(default = "StorageSettings::default_file_debounce_secs")]
        debounce_secs: u64,
    },
    #[cfg(feature = "storage-db")]
    Database {
        uri: Url,
        max_connections: Option<u32>,
        min_connections: Option<u32>,
        connect_timeout_secs: Option<u64>,
        schema: Option<String>,
        ssl_mode: Option<String>,
        #[serde(default = "StorageSettings::default_db_debounce_secs")]
        debounce_secs: u64,
    },
    #[cfg(feature = "storage-s3")]
    S3 {
        bucket: String,
        region: String,
        access_key: String,
        secret_key: String,
        endpoint: Option<Url>,
        path_style: Option<bool>,
        session_token: Option<String>,
        use_tls: Option<bool>,
        #[serde(default = "StorageSettings::default_s3_path")]
        path: String,
        #[serde(default = "StorageSettings::default_s3_debounce_secs")]
        debounce_secs: u64,
    },
}

impl StorageConfig {
    pub fn new(mode: StorageMode, settings: StorageSettings) -> Self {
        Self {
            mode,
            settings: Some(settings),
        }
    }

    pub fn mode(&self) -> &StorageMode {
        &self.mode
    }

    pub fn settings(&self) -> Option<&StorageSettings> {
        self.settings.as_ref()
    }
}

impl StorageSettings {
    pub fn default_file_path() -> PathBuf {
        PathBuf::from("./gproxy.toml")
    }

    pub fn default_usage_data_dir() -> Option<PathBuf> {
        Some(PathBuf::from("./data"))
    }

    pub fn default_file_debounce_secs() -> u64 {
        0
    }

    pub fn default_database_uri() -> Url {
        "sqlite://gproxy.db"
            .parse()
            .expect("valid default database uri")
    }

    pub fn default_s3_path() -> String {
        "gproxy.toml".to_string()
    }

    pub fn default_s3_debounce_secs() -> u64 {
        10
    }

    pub fn default_db_debounce_secs() -> u64 {
        0
    }
}

impl StorageService {
    pub async fn connect(config: &StorageConfig) -> Result<Self> {
        match (config.mode(), config.settings()) {
            #[cfg(feature = "storage-memory")]
            (StorageMode::Memory, _) => Ok(StorageService::Memory(MemoryStorage::default())),
            #[cfg(feature = "storage-file")]
            (
                StorageMode::File,
                Some(StorageSettings::File {
                    path,
                    data_dir: _,
                    debounce_secs,
                }),
            ) => Ok(StorageService::File(Box::new(FileStorage::new(
                path.clone(),
                Duration::from_secs(*debounce_secs),
            )?))),
            #[cfg(feature = "storage-file")]
            (StorageMode::File, None) => Ok(StorageService::File(Box::new(FileStorage::new(
                StorageSettings::default_file_path(),
                Duration::from_secs(StorageSettings::default_file_debounce_secs()),
            )?))),
            #[cfg(feature = "storage-file")]
            (StorageMode::File, Some(_)) => Err(anyhow!("storage settings mismatch for file")),
            #[cfg(feature = "storage-db")]
            (StorageMode::Database, Some(settings)) => Ok(StorageService::Database(
                DatabaseStorage::connect(settings).await?,
            )),
            #[cfg(feature = "storage-db")]
            (StorageMode::Database, None) => Err(anyhow!("storage settings missing for database")),
            #[cfg(feature = "storage-s3")]
            (StorageMode::S3, Some(settings)) => Ok(StorageService::S3(Box::new(
                S3Storage::connect(settings, config.clone())?,
            ))),
            #[cfg(feature = "storage-s3")]
            (StorageMode::S3, None) => Err(anyhow!("storage settings missing for s3")),
        }
    }
}

#[async_trait]
pub trait ConfigStore: Send + Sync {
    async fn get_app_config(&self) -> Result<AppConfig>;
    async fn load_app_config(&self) -> Result<AppConfig>;
    async fn save_app_config(&self, config: &AppConfig) -> Result<()>;
    async fn flush_app_config(&self) -> Result<()>;
}

#[async_trait]
impl ConfigStore for StorageService {
    async fn get_app_config(&self) -> Result<AppConfig> {
        match self {
            #[cfg(feature = "storage-memory")]
            StorageService::Memory(store) => store.get_app_config().await,
            #[cfg(feature = "storage-file")]
            StorageService::File(store) => store.get_app_config().await,
            #[cfg(feature = "storage-db")]
            StorageService::Database(store) => store.get_app_config().await,
            #[cfg(feature = "storage-s3")]
            StorageService::S3(store) => store.get_app_config().await,
            StorageService::Unconfigured => Err(anyhow!("storage not configured")),
        }
    }

    async fn load_app_config(&self) -> Result<AppConfig> {
        match self {
            #[cfg(feature = "storage-memory")]
            StorageService::Memory(store) => store.load_app_config().await,
            #[cfg(feature = "storage-file")]
            StorageService::File(store) => store.load_app_config().await,
            #[cfg(feature = "storage-db")]
            StorageService::Database(store) => store.load_app_config().await,
            #[cfg(feature = "storage-s3")]
            StorageService::S3(store) => store.load_app_config().await,
            StorageService::Unconfigured => Err(anyhow!("storage not configured")),
        }
    }

    async fn save_app_config(&self, config: &AppConfig) -> Result<()> {
        match self {
            #[cfg(feature = "storage-memory")]
            StorageService::Memory(store) => store.save_app_config(config).await,
            #[cfg(feature = "storage-file")]
            StorageService::File(store) => store.save_app_config(config).await,
            #[cfg(feature = "storage-db")]
            StorageService::Database(store) => store.save_app_config(config).await,
            #[cfg(feature = "storage-s3")]
            StorageService::S3(store) => store.save_app_config(config).await,
            StorageService::Unconfigured => Err(anyhow!("storage not configured")),
        }
    }

    async fn flush_app_config(&self) -> Result<()> {
        match self {
            #[cfg(feature = "storage-memory")]
            StorageService::Memory(store) => store.flush_app_config().await,
            #[cfg(feature = "storage-file")]
            StorageService::File(store) => store.flush_app_config().await,
            #[cfg(feature = "storage-db")]
            StorageService::Database(store) => store.flush_app_config().await,
            #[cfg(feature = "storage-s3")]
            StorageService::S3(store) => store.flush_app_config().await,
            StorageService::Unconfigured => Err(anyhow!("storage not configured")),
        }
    }
}

impl StorageService {
    pub async fn providers_get<R, F>(&self, read: F) -> Result<R>
    where
        F: FnOnce(&ProvidersConfig) -> R + Send,
    {
        match self {
            #[cfg(feature = "storage-memory")]
            StorageService::Memory(store) => store.providers_get(read).await,
            #[cfg(feature = "storage-file")]
            StorageService::File(store) => store.providers_get(read).await,
            #[cfg(feature = "storage-db")]
            StorageService::Database(store) => store.providers_get(read).await,
            #[cfg(feature = "storage-s3")]
            StorageService::S3(store) => store.providers_get(read).await,
            StorageService::Unconfigured => Err(anyhow!("storage not configured")),
        }
    }

    pub async fn providers_update<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut ProvidersConfig, Option<&mut DocumentMut>) -> Result<()> + Send,
    {
        match self {
            #[cfg(feature = "storage-memory")]
            StorageService::Memory(store) => store.providers_update(update).await,
            #[cfg(feature = "storage-file")]
            StorageService::File(store) => store.providers_update(update).await,
            #[cfg(feature = "storage-db")]
            StorageService::Database(store) => store.providers_update(update).await,
            #[cfg(feature = "storage-s3")]
            StorageService::S3(store) => store.providers_update(update).await,
            StorageService::Unconfigured => Err(anyhow!("storage not configured")),
        }
    }
}
