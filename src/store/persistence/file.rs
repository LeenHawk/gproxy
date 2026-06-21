//! File-system persistence backend.

use std::path::PathBuf;

use tokio::sync::Mutex;

mod impl_backend;
mod table;

mod authz;
mod identity;
mod logs;
mod metrics;
mod provider;
mod routing;
mod settings;
mod tokenizers;
mod transform;
mod usage;

/// Persistence backend backed by the local file system.
///
/// Suitable for single-instance deployments. The `root` directory is
/// created on [`open`](FilePersistence::open) if it does not exist. Each table
/// is one JSON file under `root`; the `write` mutex serializes mutations
/// (control-plane data is low-write, so whole-file read/rewrite is fine).
pub struct FilePersistence {
    root: PathBuf,
    write: Mutex<()>,
    /// Exclusive cross-process lock on the data dir, held for the backend's
    /// lifetime (the OS releases it on drop or crash). The `write` mutex only
    /// serializes within this process — a second process sharing the same dir
    /// would silently lose whole-file rewrites, so refuse to start instead.
    _lock: std::fs::File,
}

impl FilePersistence {
    /// Open (and create if absent) the data directory at `data_dir`.
    ///
    /// Takes an exclusive advisory lock on `data_dir/.gproxy.lock`; fails if
    /// another process already holds it (the file backend is single-instance —
    /// use the db backend to share state across processes).
    ///
    /// Only ensures the directory exists; write-permission is verified by
    /// [`health`](FilePersistence::health), which callers should invoke at startup.
    pub async fn open(data_dir: PathBuf) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(&data_dir).await.map_err(|e| {
            anyhow::anyhow!("failed to create data dir {}: {e}", data_dir.display())
        })?;
        // The data dir holds credential ciphertext + key digests; keep it
        // owner-only on Unix so other local users can't read the store.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if let Err(e) =
                tokio::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700)).await
            {
                tracing::warn!(error = %e, dir = %data_dir.display(),
                    "could not restrict data dir to 0700");
            }
        }
        let lock = lock_data_dir(&data_dir)?;
        stamp_schema_version(&data_dir).await?;
        Ok(Self {
            root: data_dir,
            write: Mutex::new(()),
            _lock: lock,
        })
    }
}

/// Acquire the exclusive data-dir lock. `WouldBlock` (another live process owns
/// the dir) is a hard error; a filesystem that does not support advisory locks
/// degrades to a warning rather than blocking startup.
fn lock_data_dir(data_dir: &std::path::Path) -> anyhow::Result<std::fs::File> {
    let path = data_dir.join(".gproxy.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| anyhow::anyhow!("failed to open lock file {}: {e}", path.display()))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => anyhow::bail!(
            "data dir {} is already in use by another GPROXY process; \
             the file backend is single-instance — use --persistence=db \
             to share state across instances",
            data_dir.display()
        ),
        Err(std::fs::TryLockError::Error(e)) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "data-dir lock unsupported on this filesystem; concurrent \
                 GPROXY processes on this dir WILL corrupt data"
            );
        }
    }
    Ok(lock)
}

/// The file backend is schemaless JSON, so there are no table migrations — but
/// for symmetry with the SQL backends we record a version stamp. Written once;
/// left untouched if it already exists (an existing store is already at this
/// version's on-disk shape).
async fn stamp_schema_version(root: &std::path::Path) -> anyhow::Result<()> {
    use crate::store::persistence::migrations::BASELINE_VERSION;

    let path = root.join("schema_version.json");
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let body = serde_json::json!({ "version": BASELINE_VERSION, "applied_at": now });
    tokio::fs::write(&path, serde_json::to_vec_pretty(&body)?)
        .await
        .map_err(|e| anyhow::anyhow!("failed to write schema_version.json: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
