//! Automatic SQLite upgrade: import a snapshot, then replace the live path.

use std::path::Path;

use gproxy_app::{
    AppConfig,
    config::{CacheBackendConfig, StoreBackendConfig},
};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

use crate::{Error, Result, Settings, config::AdminOptions, instance};

pub(crate) async fn run(config: &AppConfig, path: &Path, source: DatabaseConnection) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let backup = tempfile::Builder::new()
        .prefix(&format!(
            "{}.v3-",
            path.file_name().unwrap().to_string_lossy()
        ))
        .suffix(".bak")
        .tempfile_in(parent)
        .map_err(|error| Error::io("creating the v3 backup", error))?
        .into_temp_path();
    // SQLite copies a consistent snapshot, including committed WAL contents.
    source
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "VACUUM INTO ?",
            [backup.to_string_lossy().into_owned().into()],
        ))
        .await?;

    let staging = tempfile::Builder::new()
        .prefix(".gproxy-v4-")
        .tempdir_in(parent)
        .map_err(|error| Error::io("creating the migration directory", error))?;
    let destination = std::path::absolute(staging.path().join("gproxy.db"))
        .map_err(|error| Error::io("resolving the migration database", error))?;
    let mut staged_config = config.clone();
    staged_config.store = StoreBackendConfig::Sqlite {
        path: destination.to_string_lossy().into_owned(),
    };
    staged_config.cache = CacheBackendConfig::Memory;
    // Rotate once, after the upgraded database is in place, during normal open.
    staged_config.master_key.rotate = false;
    let settings = Settings {
        config: staged_config,
        admin: AdminOptions::default(),
        telemetry: Default::default(),
        instance_id: None,
    };
    let connection = instance::open_connection(&settings.config).await?;
    let instance = instance::assemble(
        &settings,
        instance::OpenOptions::management(),
        connection.clone(),
    )
    .await?;
    let key = config
        .master_key
        .key
        .resolve()?
        .map(|key| zeroize::Zeroizing::new(super::hex(&key)));
    tracing::info!(database = %path.display(), "migrating v3 database to v4");
    let report = super::import(
        &instance.app,
        &backup,
        key.as_deref().map(String::as_str),
        &settings.admin,
        true,
    )
    .await;
    instance.app.gproxy().shutdown();
    drop(instance);
    let report = report?;
    // A self-contained file is what rename publishes; WAL files are not moved.
    connection
        .execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)")
        .await?;
    connection
        .execute_unprepared("PRAGMA journal_mode=DELETE")
        .await?;
    connection.close().await?;
    source
        .execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)")
        .await?;
    source
        .execute_unprepared("PRAGMA journal_mode=DELETE")
        .await?;
    source.close().await?;
    let backup = backup
        .keep()
        .map_err(|error| Error::io("keeping the v3 backup", error.error))?;
    std::fs::rename(&destination, path)
        .map_err(|error| Error::io("installing the migrated database", error))?;
    report.announce();
    tracing::info!(database = %path.display(), backup = %backup.display(), "v3 migration complete");
    Ok(())
}
