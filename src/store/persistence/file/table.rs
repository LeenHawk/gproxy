//! Generic JSON-file table helper: a `{next_id, rows}` document per entity,
//! read/rewritten whole on each operation (control-plane data is small).

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// On-disk shape for one entity table.
#[derive(Serialize, serde::Deserialize)]
pub(super) struct Table<T> {
    pub next_id: i64,
    pub rows: Vec<T>,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Self {
            next_id: 1,
            rows: Vec::new(),
        }
    }
}

/// Load a table from `path`, or an empty table if the file does not exist.
pub(super) async fn load<T: DeserializeOwned>(path: &Path) -> anyhow::Result<Table<T>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Table::default()),
        Err(e) => Err(e.into()),
    }
}

/// Write a table to `path` atomically (temp file + rename). On Unix the file is
/// created `0600` before the rename — these tables hold credential ciphertext
/// and key digests, so they must not be world-readable on a shared host even
/// when envelope encryption is off (keyless/plaintext mode).
pub(super) async fn store<T: Serialize>(path: &Path, table: &Table<T>) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(table)?;
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, &bytes).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).await?;
    }
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

/// Current unix time in seconds.
pub(super) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
