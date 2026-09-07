mod sqlite;

use std::fs::{File, OpenOptions};
use std::path::Path;

use base64::Engine as _;

use crate::{AppError, Config, V2ImportOptions};

pub(crate) async fn prepare(config: &Config) -> Result<Option<File>, AppError> {
    let gproxy_store::BackendConfig::Sqlite { path } = config.backend_config() else {
        return Ok(None);
    };
    if !path.exists() || !sqlite::is_v2(&path)? {
        return Ok(None);
    }
    let path = path.canonicalize().map_err(error)?;
    let parent = path
        .parent()
        .ok_or_else(|| error("database has no parent directory"))?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(".gproxy-v2-upgrade.lock"))
        .map_err(error)?;
    lock.try_lock()
        .map_err(|_| error("another process is upgrading this database"))?;
    if !sqlite::is_v2(&path)? {
        return Ok(Some(lock));
    }
    let blocked = parent.join(".gproxy-v2-upgrade-blocked");
    if blocked.exists() {
        return Err(error(format!(
            "automatic migration previously failed; inspect {} and remove that marker only after resolving the failure",
            blocked.display()
        )));
    }
    if config.secret_keys().rotate {
        return Err(error(
            "disable master-key rotation during the v2 upgrade; rotate after the upgrade completes",
        ));
    }
    let attempt = parent.join(format!(
        "gproxy-v2-backup-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(error)?
            .as_nanos(),
        std::process::id()
    ));
    private_directory(&attempt)?;
    let result = migrate(config, &path, &attempt).await;
    match result {
        Ok(()) => {
            tracing::warn!(backup = %attempt.display(), "v2 database upgraded; retain this backup to roll back with a v2 executable");
            Ok(Some(lock))
        }
        Err(failure) => {
            if !attempt.join("report.txt").exists() {
                std::fs::write(attempt.join("report.txt"), failure.to_string()).map_err(error)?;
            }
            let message = format!("{failure}; upgrade files: {}", attempt.display());
            std::fs::write(&blocked, &message).map_err(error)?;
            Err(error(format!(
                "{message}; automatic retry blocked by {}",
                blocked.display()
            )))
        }
    }
}

async fn migrate(config: &Config, original: &Path, attempt: &Path) -> Result<(), AppError> {
    let source = sqlite::quiesce(original)?;
    sqlite::validate_source(&source)?;
    let backup = attempt.join("gproxy-v2.db");
    std::fs::copy(original, &backup).map_err(error)?;
    File::open(&backup)
        .and_then(|file| file.sync_all())
        .map_err(error)?;
    let candidate = attempt.join("candidate");
    private_directory(&candidate)?;
    let target = Config::sqlite(
        config.listen_addr(),
        candidate.clone(),
        config.secret_keys().clone(),
    );
    let report = super::migrate_from_v2(
        &target,
        V2ImportOptions {
            path: backup.clone(),
            source_master_key: config
                .secret_keys()
                .current
                .as_ref()
                .map(|key| base64::engine::general_purpose::STANDARD.encode(key)),
            apply: true,
            merge: false,
        },
    )
    .await?;
    std::fs::write(attempt.join("report.txt"), report.to_string()).map_err(error)?;
    if report.has_blockers()
        || report
            .counts
            .iter()
            .any(|count| count.found != count.imported)
    {
        return Err(error(
            "migration did not preserve every supported source row; inspect report.txt",
        ));
    }
    {
        let store = gproxy_store::Store::open(target.backend_config()).await?;
        crate::key_rotation::prepare(&store, target.secret_keys()).await?;
        crate::control::SnapshotControl::new(
            store,
            crate::control::RuntimeOverrides::from_config(config),
        )
        .await?;
    }
    let database = attempt.join("ready.db");
    sqlite::snapshot_target(&candidate.join("gproxy.db"), &database)?;
    sqlite::validate_target(&backup, &database)?;
    std::fs::set_permissions(
        &database,
        std::fs::metadata(original).map_err(error)?.permissions(),
    )
    .map_err(error)?;
    File::open(&database)
        .and_then(|file| file.sync_all())
        .map_err(error)?;
    sync_directory(attempt)?;
    source.execute_batch("ROLLBACK").map_err(error)?;
    drop(source);
    std::fs::rename(&database, original).map_err(error)?;
    if let Some(parent) = original.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

#[cfg(unix)]
fn private_directory(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(error)
}

#[cfg(not(unix))]
fn private_directory(path: &Path) -> Result<(), AppError> {
    std::fs::DirBuilder::new().create(path).map_err(error)
}

fn sync_directory(path: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(error)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn error(value: impl std::fmt::Display) -> AppError {
    AppError::Migration(value.to_string())
}
