//! MIGRATE-V1 (remove in 2.1): one-shot legacy v1 → v2 data migration.
//!
//! Two entry points, one core:
//! - [`maybe_migrate_on_boot`] runs on startup *before* the persistence backend
//!   is built. If the configured SQLite file is a v1 database, it builds a fresh
//!   v2 database in a temp file, imports the v1 config into it, then atomically
//!   swaps it into place — backing the v1 file up as `<name>.v1.bak`. The result
//!   is a drop-in upgrade: replace the binary, start it, it just works.
//! - [`run_cli`] backs the `migrate-v1` subcommand (explicit `--from`/`--to`,
//!   `--dry-run`) for controlled/offline migrations.
//!
//! Idempotent: a v2 file is left untouched (sniffed by table presence), so the
//! boot path is a no-op after the first successful migration. The v1 file is
//! only ever read; it survives as the backup.

mod cipher;
mod map;
mod read;

use std::path::{Path, PathBuf};

use crate::app::import::{Bundle, import_bundle};
use crate::channel::registry::ChannelRegistry;
use crate::crypto::SecretCipher;
use crate::store::persistence::DbPersistence;
use cipher::V1Cipher;
use read::V1Data;

/// Per-table counts of what was migrated (for logging / dry-run output).
#[derive(Debug, Default, Clone, Copy)]
pub struct Report {
    pub providers: usize,
    pub credentials: usize,
    pub models: usize,
    pub users: usize,
    pub user_keys: usize,
    pub routes: usize,
    pub route_members: usize,
    pub quotas: usize,
    pub permissions: usize,
    pub rate_limits: usize,
    pub total: usize,
}

impl Report {
    fn of(bundle: &Bundle) -> Self {
        let total = bundle.orgs.len()
            + bundle.users.len()
            + bundle.user_keys.len()
            + bundle.providers.len()
            + bundle.credentials.len()
            + bundle.provider_models.len()
            + bundle.routes.len()
            + bundle.route_members.len()
            + bundle.quotas.len()
            + bundle.route_permissions.len()
            + bundle.rate_limits.len()
            + bundle.instance_settings.len();
        Self {
            providers: bundle.providers.len(),
            credentials: bundle.credentials.len(),
            models: bundle.provider_models.len(),
            users: bundle.users.len(),
            user_keys: bundle.user_keys.len(),
            routes: bundle.routes.len(),
            route_members: bundle.route_members.len(),
            quotas: bundle.quotas.len(),
            permissions: bundle.route_permissions.len(),
            rate_limits: bundle.rate_limits.len(),
            total,
        }
    }
}

enum Schema {
    V1,
    V2,
    Unknown,
}

/// Boot hook: migrate in place if the configured SQLite db is a v1 database.
/// Returns `None` when there is nothing to do (non-sqlite dsn, missing file, or
/// already v2). Runs BEFORE the v2 persistence backend opens the file.
pub async fn maybe_migrate_on_boot(
    db_dsn: &str,
    cipher: &dyn SecretCipher,
    channels: &ChannelRegistry,
) -> anyhow::Result<Option<Report>> {
    let Some(path) = sqlite_path_from_dsn(db_dsn) else {
        return Ok(None); // non-sqlite target → no v1 file to adopt
    };
    recover_interrupted(&path)?;
    if !path.exists() {
        return Ok(None); // fresh install
    }
    match sniff(&path).await? {
        Schema::V2 | Schema::Unknown => Ok(None),
        Schema::V1 => {
            tracing::warn!(
                path = %path.display(),
                "v1 database detected — migrating to v2 in place (original backed up)"
            );
            Ok(Some(migrate_in_place(&path, cipher, channels).await?))
        }
    }
}

/// `migrate-v1` subcommand: read `from`, import into `to_dsn` (a db backend).
/// `dry_run` reads + maps and reports counts without writing.
pub async fn run_cli(
    from: &Path,
    to_dsn: &str,
    dry_run: bool,
    cipher: &dyn SecretCipher,
    channels: &ChannelRegistry,
) -> anyhow::Result<Report> {
    anyhow::ensure!(from.exists(), "v1 database not found: {}", from.display());
    let (data, bundle) = read_and_map(from).await?;
    let report = Report::of(&bundle);
    if dry_run {
        return Ok(report);
    }
    let target = DbPersistence::connect(to_dsn).await?;
    apply(&target, cipher, channels, &data, &bundle).await?;
    target.close().await?;
    Ok(report)
}

/// Read the v1 db (read-only) and map it into a v2 import bundle.
async fn read_and_map(path: &Path) -> anyhow::Result<(V1Data, Bundle)> {
    let pool = read::open_ro(path).await?;
    let data = read::read_all(&pool).await?;
    pool.close().await;
    let v1cipher = V1Cipher::from_env();
    let bundle = map::to_bundle(&data, &v1cipher)?;
    Ok((data, bundle))
}

/// Import the bundle into `target`, then materialize each provider's channel
/// default routing rules (v1's `routing_json` vocabulary is not portable).
async fn apply(
    target: &DbPersistence,
    cipher: &dyn SecretCipher,
    channels: &ChannelRegistry,
    data: &V1Data,
    bundle: &Bundle,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(bundle)?;
    import_bundle(target, cipher, &json).await?;
    // Seed each provider's channel-default routing rules. Best-effort: a v1
    // channel with no v2 equivalent (e.g. a renamed/removed channel) is migrated
    // anyway but left without routing — warned, not fatal, so one stale provider
    // never blocks the whole migration. The operator remaps it in the console.
    let mut unknown: Vec<&str> = Vec::new();
    for p in &data.providers {
        if let Err(e) =
            crate::api::routing::seed_default_routing(target, channels, p.id, true).await
        {
            tracing::warn!(
                provider_id = p.id,
                channel = %p.channel,
                "could not seed default routing ({e:?}); fix this provider's channel in the console before it can serve"
            );
            unknown.push(p.channel.as_str());
        }
    }
    if !unknown.is_empty() {
        unknown.sort_unstable();
        unknown.dedup();
        tracing::warn!(
            channels = ?unknown,
            "migration finished, but some providers use channels unknown to v2 — remap them in the console"
        );
    }
    Ok(())
}

/// Build a v2 db in a temp file, import the v1 config, then atomically swap it
/// into `path` (backing the v1 file up first).
async fn migrate_in_place(
    path: &Path,
    cipher: &dyn SecretCipher,
    channels: &ChannelRegistry,
) -> anyhow::Result<Report> {
    let temp = temp_path(path);
    let backup = free_backup_path(path);
    remove_with_sidecars(&temp); // clear any leftover from a failed earlier run

    let (data, bundle) = read_and_map(path).await?;
    let report = Report::of(&bundle);

    // Build the fresh v2 db in the temp file, then close so its WAL is flushed.
    let temp_dsn = sqlite_dsn(&temp)?;
    let temp_db = DbPersistence::connect(&temp_dsn).await?;
    apply(&temp_db, cipher, channels, &data, &bundle).await?;
    temp_db.close().await?;

    // Atomic swap: v1 aside (with WAL sidecars), then the temp v2 into place
    // (also with sidecars, so any not-yet-checkpointed WAL travels with its db).
    move_with_sidecars(path, &backup)?;
    move_with_sidecars(&temp, path)?;

    tracing::warn!(
        backup = %backup.display(),
        "v1 database backed up; v2 database is now live at {}",
        path.display()
    );
    Ok(report)
}

/// Recover from a crash inside the two-rename swap window (best effort).
fn recover_interrupted(path: &Path) -> anyhow::Result<()> {
    let temp = temp_path(path);
    let backup = with_suffix(path, ".v1.bak");
    if !path.exists() && temp.exists() {
        tracing::warn!("resuming interrupted v1 migration: completing swap");
        move_with_sidecars(&temp, path)?;
    } else if !path.exists() && backup.exists() {
        tracing::warn!("v1 migration was interrupted; restoring backup to retry");
        move_with_sidecars(&backup, path)?;
    } else if path.exists() && temp.exists() {
        remove_with_sidecars(&temp); // stale temp next to an intact db
    }
    Ok(())
}

async fn sniff(path: &Path) -> anyhow::Result<Schema> {
    let pool = read::open_ro(path).await?;
    let is_v2 = read::table_exists(&pool, "orgs").await?
        || read::table_exists(&pool, "routes").await?
        || read::table_exists(&pool, "schema_migrations").await?;
    let schema = if is_v2 {
        Schema::V2
    } else if read::table_exists(&pool, "global_settings").await?
        && read::table_exists(&pool, "providers").await?
    {
        Schema::V1
    } else {
        Schema::Unknown
    };
    pool.close().await;
    Ok(schema)
}

/// Extract the filesystem path from a `sqlite:` DSN; `None` for `:memory:` or a
/// non-sqlite dsn (postgres/mysql have no v1 file to adopt).
fn sqlite_path_from_dsn(dsn: &str) -> Option<PathBuf> {
    let rest = dsn.strip_prefix("sqlite:")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let path = rest.split('?').next().unwrap_or(rest);
    if path.is_empty() || path.starts_with(':') || path == "memory:" {
        return None;
    }
    Some(PathBuf::from(path))
}

fn sqlite_dsn(path: &Path) -> anyhow::Result<String> {
    let abs = std::path::absolute(path).map_err(|e| anyhow::anyhow!("resolve {e}"))?;
    Ok(format!("sqlite://{}?mode=rwc", abs.display()))
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("gproxy.db");
    path.with_file_name(format!(".{name}.migrating"))
}

fn free_backup_path(path: &Path) -> PathBuf {
    let base = with_suffix(path, ".v1.bak");
    if !base.exists() {
        return base;
    }
    (2..)
        .map(|n| with_suffix(path, &format!(".v1.bak.{n}")))
        .find(|p| !p.exists())
        .expect("an unused backup index exists")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn move_with_sidecars(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::rename(from, to)
        .map_err(|e| anyhow::anyhow!("rename {} -> {}: {e}", from.display(), to.display()))?;
    for ext in ["-wal", "-shm"] {
        let f = with_suffix(from, ext);
        if f.exists() {
            let _ = std::fs::rename(&f, with_suffix(to, ext));
        }
    }
    Ok(())
}

fn remove_with_sidecars(path: &Path) {
    let _ = std::fs::remove_file(path);
    for ext in ["-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(with_suffix(path, ext));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sqlite_paths() {
        assert_eq!(
            sqlite_path_from_dsn("sqlite:///home/x/data/gproxy.db?mode=rwc"),
            Some(PathBuf::from("/home/x/data/gproxy.db"))
        );
        assert_eq!(
            sqlite_path_from_dsn("sqlite://./data/gproxy.db?mode=rwc"),
            Some(PathBuf::from("./data/gproxy.db"))
        );
        assert_eq!(
            sqlite_path_from_dsn("sqlite:data/gproxy.db"),
            Some(PathBuf::from("data/gproxy.db"))
        );
        assert_eq!(sqlite_path_from_dsn("sqlite::memory:"), None);
        assert_eq!(sqlite_path_from_dsn("postgres://localhost/db"), None);
    }

    #[test]
    fn sidecar_suffixing() {
        let p = Path::new("/d/gproxy.db");
        assert_eq!(with_suffix(p, "-wal"), PathBuf::from("/d/gproxy.db-wal"));
        assert_eq!(
            with_suffix(p, ".v1.bak"),
            PathBuf::from("/d/gproxy.db.v1.bak")
        );
    }
}
