use std::error::Error;
use std::fs::OpenOptions;
use std::path::PathBuf;

pub(crate) fn resolve_dsn(
    input: &str,
    data_dir: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    if !input.trim().is_empty() {
        ensure_sqlite_dsn(input)?;
        return Ok(input.to_string());
    }

    let db_path = PathBuf::from(data_dir).join("db").join("gproxy.db");
    let db_path = db_path.to_string_lossy();
    let dsn = if db_path.starts_with('/') {
        let trimmed = db_path.trim_start_matches('/');
        format!("sqlite:///{}", trimmed)
    } else {
        format!("sqlite://{}", db_path)
    };
    ensure_sqlite_dsn(&dsn)?;
    Ok(dsn)
}

pub(crate) fn ensure_sqlite_dsn(dsn: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !dsn.starts_with("sqlite:") {
        return Ok(());
    }

    let mut rest = &dsn["sqlite:".len()..];
    if rest.starts_with("//") {
        rest = &rest[2..];
    }
    if rest.is_empty() {
        return Ok(());
    }
    if rest.starts_with(":memory:") || rest.starts_with("memory:") {
        return Ok(());
    }

    let path_part = rest.split('?').next().unwrap_or("");
    if path_part.is_empty() {
        return Ok(());
    }

    let path = PathBuf::from(path_part);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
    }

    Ok(())
}
