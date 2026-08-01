//! Shared helpers for temporary startup migrations.

use std::path::PathBuf;

/// Extract a filesystem path from a file-backed SQLite DSN.
pub(crate) fn sqlite_path_from_dsn(dsn: &str) -> Option<PathBuf> {
    let rest = dsn.strip_prefix("sqlite:")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let path = rest.split('?').next().unwrap_or(rest);
    if path.is_empty() || path.starts_with(':') || path == "memory:" {
        return None;
    }
    Some(PathBuf::from(path))
}
