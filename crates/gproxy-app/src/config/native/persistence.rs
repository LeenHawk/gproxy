use std::path::{Path, PathBuf};

use crate::{Config, ConfigError};

pub(super) fn sqlite(
    config: Config,
    dsn: Option<String>,
    cwd: &Path,
) -> Result<Config, ConfigError> {
    let Some(dsn) = dsn else {
        return Ok(config);
    };
    let path = dsn.strip_prefix("sqlite://").ok_or_else(|| super::invalid(
        super::DSN, "v2 automatic upgrade requires a file-backed SQLite DSN; migrate remote databases explicitly before switching to v3",
    ))?;
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    if path.is_empty()
        || path == ":memory:"
        || path.contains(['%', '#'])
        || !matches!(query, "" | "mode=rwc" | "mode=rw")
    {
        return Err(super::invalid(
            super::DSN,
            "expected sqlite://PATH with optional mode=rw or mode=rwc",
        ));
    }
    let path = PathBuf::from(path);
    Ok(config.with_sqlite_path(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }))
}
