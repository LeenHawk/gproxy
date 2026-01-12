use crate::storage::{StorageConfig, StorageMode, StorageSettings};
use anyhow::{Result, anyhow};

use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::{UsageRecord, UsageViewRecord};

mod view;
#[cfg(feature = "storage-memory")]
mod memory;
#[cfg(feature = "storage-file")]
mod file;
#[cfg(feature = "storage-s3")]
mod s3;
#[cfg(feature = "storage-db")]
pub(crate) mod database;

#[cfg(feature = "storage-memory")]
pub use memory::MemoryUsageStore;

const DEFAULT_ANCHOR_TS: i64 = 0;

pub struct UsageViewSpec {
    pub name: &'static str,
    pub slot_secs: i64,
}

pub const DEFAULT_USAGE_VIEWS: &[UsageViewSpec] = &[
    UsageViewSpec {
        name: "1min",
        slot_secs: 60,
    },
    UsageViewSpec {
        name: "1day",
        slot_secs: 24 * 60 * 60,
    },
    UsageViewSpec {
        name: "sum",
        slot_secs: 0,
    },
];

pub const CODEX_USAGE_VIEWS: &[UsageViewSpec] = &[
    UsageViewSpec {
        name: "5h",
        slot_secs: 5 * 60 * 60,
    },
    UsageViewSpec {
        name: "1w",
        slot_secs: 7 * 24 * 60 * 60,
    },
    UsageViewSpec {
        name: "sum",
        slot_secs: 0,
    },
];

pub const CLAUDE_CODE_USAGE_VIEWS: &[UsageViewSpec] = &[
    UsageViewSpec {
        name: "5h",
        slot_secs: 5 * 60 * 60,
    },
    UsageViewSpec {
        name: "1w",
        slot_secs: 7 * 24 * 60 * 60,
    },
    UsageViewSpec {
        name: "1w_sonnet",
        slot_secs: 7 * 24 * 60 * 60,
    },
    UsageViewSpec {
        name: "sum",
        slot_secs: 0,
    },
];

pub fn usage_views_for_provider(provider: ProviderKind) -> &'static [UsageViewSpec] {
    match provider {
        ProviderKind::Codex => CODEX_USAGE_VIEWS,
        ProviderKind::ClaudeCode => CLAUDE_CODE_USAGE_VIEWS,
        _ => DEFAULT_USAGE_VIEWS,
    }
}

pub fn slot_start(anchor_ts: i64, slot_secs: i64, now_ts: i64) -> i64 {
    if slot_secs <= 0 {
        return anchor_ts;
    }
    let offset = now_ts - anchor_ts;
    let slot_id = offset.div_euclid(slot_secs);
    anchor_ts + slot_id * slot_secs
}

pub fn next_anchor_ts(anchor_ts: i64, slot_secs: i64, now_ts: i64) -> i64 {
    if slot_secs <= 0 {
        return anchor_ts;
    }
    slot_start(anchor_ts, slot_secs, now_ts) + slot_secs
}

pub fn slot_secs_for_view(provider: ProviderKind, view_name: &str) -> Option<i64> {
    usage_views_for_provider(provider)
        .iter()
        .find(|spec| spec.name == view_name)
        .map(|spec| spec.slot_secs)
}

pub async fn set_default_usage_anchors(
    store: &UsageStore,
    provider: ProviderKind,
    anchor_ts: i64,
) -> Result<()> {
    for spec in DEFAULT_USAGE_VIEWS {
        if spec.slot_secs <= 0 {
            continue;
        }
        store.set_anchor(provider, spec.name, anchor_ts).await?;
    }
    Ok(())
}

#[async_trait::async_trait]
pub trait UsageBackend: Send + Sync {
    async fn record(&self, record: UsageRecord) -> Result<()>;
    async fn flush(&self) -> Result<()>;
    async fn set_anchor(
        &self,
        provider: ProviderKind,
        view_name: &str,
        anchor_ts: i64,
    ) -> Result<()>;
    async fn query_by_api_key(&self, api_key: &str) -> Result<Vec<UsageViewRecord>>;
    async fn query_by_provider_credential(
        &self,
        provider: crate::providers::credential_status::ProviderKind,
        credential_id: &str,
    ) -> Result<Vec<UsageViewRecord>>;
}

pub enum UsageStore {
    #[cfg(feature = "storage-memory")]
    Memory(memory::MemoryUsageStore),
    #[cfg(feature = "storage-file")]
    File(file::FileUsageStore),
    #[cfg(feature = "storage-s3")]
    S3(s3::S3UsageStore),
    #[cfg(feature = "storage-db")]
    Database(database::DatabaseUsageStore),
    Unconfigured,
}

impl Default for UsageStore {
    fn default() -> Self {
        #[cfg(feature = "storage-memory")]
        {
            UsageStore::memory()
        }
        #[cfg(not(feature = "storage-memory"))]
        UsageStore::Unconfigured
    }
}

impl UsageStore {
    #[cfg(feature = "storage-memory")]
    pub fn memory() -> Self {
        UsageStore::Memory(memory::MemoryUsageStore::new(DEFAULT_ANCHOR_TS))
    }

    pub async fn connect(config: &StorageConfig) -> Result<Self> {
        match (config.mode(), config.settings()) {
            #[cfg(feature = "storage-memory")]
            (StorageMode::Memory, _) => Ok(UsageStore::memory()),
            #[cfg(feature = "storage-file")]
            (
                StorageMode::File,
                Some(StorageSettings::File {
                    path,
                    data_dir,
                    debounce_secs,
                }),
            ) => Ok(UsageStore::File(
                file::FileUsageStore::new(
                    path.clone(),
                    data_dir.clone(),
                    *debounce_secs,
                    DEFAULT_ANCHOR_TS,
                )
                .await?,
            )),
            #[cfg(feature = "storage-file")]
            (StorageMode::File, None) => Ok(UsageStore::File(
                file::FileUsageStore::new(
                    StorageSettings::default_file_path(),
                    StorageSettings::default_usage_data_dir(),
                    StorageSettings::default_file_debounce_secs(),
                    DEFAULT_ANCHOR_TS,
                )
                .await?,
            )),
            #[cfg(feature = "storage-db")]
            (StorageMode::Database, Some(settings)) => Ok(UsageStore::Database(
                database::DatabaseUsageStore::connect(settings, DEFAULT_ANCHOR_TS).await?,
            )),
            #[cfg(feature = "storage-db")]
            (StorageMode::Database, None) => {
                Err(anyhow!("usage storage settings missing for database"))
            }
            #[cfg(feature = "storage-s3")]
            (StorageMode::S3, Some(settings)) => Ok(UsageStore::S3(
                s3::S3UsageStore::connect(settings, DEFAULT_ANCHOR_TS).await?,
            )),
            #[cfg(feature = "storage-s3")]
            (StorageMode::S3, None) => Err(anyhow!("usage storage settings missing for s3")),
            #[cfg(feature = "storage-file")]
            (StorageMode::File, Some(_)) => Err(anyhow!("usage storage settings mismatch for file")),
        }
    }

    pub async fn record(&self, record: UsageRecord) -> Result<()> {
        match self {
            #[cfg(feature = "storage-memory")]
            UsageStore::Memory(store) => store.record(record).await,
            #[cfg(feature = "storage-file")]
            UsageStore::File(store) => store.record(record).await,
            #[cfg(feature = "storage-s3")]
            UsageStore::S3(store) => store.record(record).await,
            #[cfg(feature = "storage-db")]
            UsageStore::Database(store) => store.record(record).await,
            UsageStore::Unconfigured => Err(anyhow!("usage store not configured")),
        }
    }

    pub async fn flush(&self) -> Result<()> {
        match self {
            #[cfg(feature = "storage-memory")]
            UsageStore::Memory(store) => store.flush().await,
            #[cfg(feature = "storage-file")]
            UsageStore::File(store) => store.flush().await,
            #[cfg(feature = "storage-s3")]
            UsageStore::S3(store) => store.flush().await,
            #[cfg(feature = "storage-db")]
            UsageStore::Database(store) => store.flush().await,
            UsageStore::Unconfigured => Err(anyhow!("usage store not configured")),
        }
    }

    pub async fn set_anchor(
        &self,
        provider: ProviderKind,
        view_name: &str,
        anchor_ts: i64,
    ) -> Result<()> {
        match self {
            #[cfg(feature = "storage-memory")]
            UsageStore::Memory(store) => store.set_anchor(provider, view_name, anchor_ts).await,
            #[cfg(feature = "storage-file")]
            UsageStore::File(store) => store.set_anchor(provider, view_name, anchor_ts).await,
            #[cfg(feature = "storage-s3")]
            UsageStore::S3(store) => store.set_anchor(provider, view_name, anchor_ts).await,
            #[cfg(feature = "storage-db")]
            UsageStore::Database(store) => store.set_anchor(provider, view_name, anchor_ts).await,
            UsageStore::Unconfigured => Err(anyhow!("usage store not configured")),
        }
    }

    pub async fn query_by_api_key(&self, api_key: &str) -> Result<Vec<UsageViewRecord>> {
        match self {
            #[cfg(feature = "storage-memory")]
            UsageStore::Memory(store) => store.query_by_api_key(api_key).await,
            #[cfg(feature = "storage-file")]
            UsageStore::File(store) => store.query_by_api_key(api_key).await,
            #[cfg(feature = "storage-s3")]
            UsageStore::S3(store) => store.query_by_api_key(api_key).await,
            #[cfg(feature = "storage-db")]
            UsageStore::Database(store) => store.query_by_api_key(api_key).await,
            UsageStore::Unconfigured => Err(anyhow!("usage store not configured")),
        }
    }

    pub async fn query_by_provider_credential(
        &self,
        provider: crate::providers::credential_status::ProviderKind,
        credential_id: &str,
    ) -> Result<Vec<UsageViewRecord>> {
        match self {
            #[cfg(feature = "storage-memory")]
            UsageStore::Memory(store) => {
                store.query_by_provider_credential(provider, credential_id).await
            }
            #[cfg(feature = "storage-file")]
            UsageStore::File(store) => {
                store.query_by_provider_credential(provider, credential_id)
                    .await
            }
            #[cfg(feature = "storage-s3")]
            UsageStore::S3(store) => {
                store.query_by_provider_credential(provider, credential_id)
                    .await
            }
            #[cfg(feature = "storage-db")]
            UsageStore::Database(store) => {
                store.query_by_provider_credential(provider, credential_id).await
            }
            UsageStore::Unconfigured => Err(anyhow!("usage store not configured")),
        }
    }
}
