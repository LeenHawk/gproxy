use anyhow::{Result, anyhow};
use clap::Parser;
use std::path::PathBuf;
use url::Url;

use crate::storage::{StorageConfig, StorageMode, StorageSettings};

#[derive(Debug, Parser, Clone)]
pub struct CliArgs {
    #[arg(long = "storage", env = "GPROXY_STORAGE", value_enum)]
    pub mode: Option<StorageMode>,
    #[cfg(feature = "storage-file")]
    #[arg(long = "storage-file-path", env = "GPROXY_STORAGE_FILE_PATH")]
    pub file_path: Option<PathBuf>,
    #[cfg(feature = "storage-file")]
    #[arg(long = "storage-file-data-dir", env = "GPROXY_STORAGE_FILE_DATA_DIR")]
    pub file_data_dir: Option<PathBuf>,
    #[cfg(feature = "storage-file")]
    #[arg(
        long = "storage-file-debounce-secs",
        env = "GPROXY_STORAGE_FILE_DEBOUNCE_SECS"
    )]
    pub file_debounce_secs: Option<u64>,
    #[cfg(feature = "storage-db")]
    #[arg(long = "storage-db-uri", env = "GPROXY_STORAGE_DB_URI")]
    pub db_uri: Option<Url>,
    #[cfg(feature = "storage-db")]
    #[arg(
        long = "storage-db-max-connections",
        env = "GPROXY_STORAGE_DB_MAX_CONNECTIONS"
    )]
    pub db_max_connections: Option<u32>,
    #[cfg(feature = "storage-db")]
    #[arg(
        long = "storage-db-min-connections",
        env = "GPROXY_STORAGE_DB_MIN_CONNECTIONS"
    )]
    pub db_min_connections: Option<u32>,
    #[cfg(feature = "storage-db")]
    #[arg(
        long = "storage-db-connect-timeout-secs",
        env = "GPROXY_STORAGE_DB_CONNECT_TIMEOUT_SECS"
    )]
    pub db_connect_timeout_secs: Option<u64>,
    #[cfg(feature = "storage-db")]
    #[arg(long = "storage-db-schema", env = "GPROXY_STORAGE_DB_SCHEMA")]
    pub db_schema: Option<String>,
    #[cfg(feature = "storage-db")]
    #[arg(long = "storage-db-ssl-mode", env = "GPROXY_STORAGE_DB_SSL_MODE")]
    pub db_ssl_mode: Option<String>,
    #[cfg(feature = "storage-db")]
    #[arg(
        long = "storage-db-debounce-secs",
        env = "GPROXY_STORAGE_DB_DEBOUNCE_SECS"
    )]
    pub db_debounce_secs: Option<u64>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-bucket", env = "GPROXY_STORAGE_S3_BUCKET")]
    pub s3_bucket: Option<String>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-region", env = "GPROXY_STORAGE_S3_REGION")]
    pub s3_region: Option<String>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-access-key", env = "GPROXY_STORAGE_S3_ACCESS_KEY")]
    pub s3_access_key: Option<String>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-secret-key", env = "GPROXY_STORAGE_S3_SECRET_KEY")]
    pub s3_secret_key: Option<String>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-endpoint", env = "GPROXY_STORAGE_S3_ENDPOINT")]
    pub s3_endpoint: Option<Url>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-path-style", env = "GPROXY_STORAGE_S3_PATH_STYLE")]
    pub s3_path_style: Option<bool>,
    #[cfg(feature = "storage-s3")]
    #[arg(
        long = "storage-s3-session-token",
        env = "GPROXY_STORAGE_S3_SESSION_TOKEN"
    )]
    pub s3_session_token: Option<String>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-use-tls", env = "GPROXY_STORAGE_S3_USE_TLS")]
    pub s3_use_tls: Option<bool>,
    #[cfg(feature = "storage-s3")]
    #[arg(long = "storage-s3-path", env = "GPROXY_STORAGE_S3_PATH")]
    pub s3_path: Option<String>,
    #[cfg(feature = "storage-s3")]
    #[arg(
        long = "storage-s3-debounce-secs",
        env = "GPROXY_STORAGE_S3_DEBOUNCE_SECS"
    )]
    pub s3_debounce_secs: Option<u64>,
}

impl CliArgs {
    pub fn storage_config(&self) -> Result<StorageConfig> {
        let mode = self.mode.clone().unwrap_or_default();
        let settings = match mode {
            #[cfg(feature = "storage-memory")]
            StorageMode::Memory => None,
            #[cfg(feature = "storage-file")]
            StorageMode::File => Some(StorageSettings::File {
                path: self
                    .file_path
                    .clone()
                    .unwrap_or_else(StorageSettings::default_file_path),
                data_dir: self
                    .file_data_dir
                    .clone()
                    .or_else(StorageSettings::default_usage_data_dir),
                debounce_secs: self
                    .file_debounce_secs
                    .unwrap_or_else(StorageSettings::default_file_debounce_secs),
            }),
            #[cfg(feature = "storage-db")]
            StorageMode::Database => Some(StorageSettings::Database {
                uri: self
                    .db_uri
                    .clone()
                    .ok_or_else(|| anyhow!("missing --storage-db-uri / GPROXY_STORAGE_DB_URI"))?,
                max_connections: self.db_max_connections,
                min_connections: self.db_min_connections,
                connect_timeout_secs: self.db_connect_timeout_secs,
                schema: self.db_schema.clone(),
                ssl_mode: self.db_ssl_mode.clone(),
                debounce_secs: self
                    .db_debounce_secs
                    .unwrap_or_else(StorageSettings::default_db_debounce_secs),
            }),
            #[cfg(feature = "storage-s3")]
            StorageMode::S3 => Some(StorageSettings::S3 {
                bucket: self.s3_bucket.clone().ok_or_else(|| {
                    anyhow!("missing --storage-s3-bucket / GPROXY_STORAGE_S3_BUCKET")
                })?,
                region: self.s3_region.clone().ok_or_else(|| {
                    anyhow!("missing --storage-s3-region / GPROXY_STORAGE_S3_REGION")
                })?,
                access_key: self.s3_access_key.clone().ok_or_else(|| {
                    anyhow!("missing --storage-s3-access-key / GPROXY_STORAGE_S3_ACCESS_KEY")
                })?,
                secret_key: self.s3_secret_key.clone().ok_or_else(|| {
                    anyhow!("missing --storage-s3-secret-key / GPROXY_STORAGE_S3_SECRET_KEY")
                })?,
                endpoint: self.s3_endpoint.clone(),
                path_style: self.s3_path_style,
                session_token: self.s3_session_token.clone(),
                use_tls: self.s3_use_tls,
                path: self
                    .s3_path
                    .clone()
                    .unwrap_or_else(StorageSettings::default_s3_path),
                debounce_secs: self
                    .s3_debounce_secs
                    .unwrap_or_else(StorageSettings::default_s3_debounce_secs),
            }),
        };

        Ok(StorageConfig { mode, settings })
    }
}
