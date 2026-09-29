//! Reading a v3 SQLite file directly, read-only.
//!
//! # Why this is the primary route, and the export is the second one
//!
//! v3 has no `export` subcommand. Its binary's only subcommand is `migrate`,
//! which imports a **v2** database; `gproxy export --help` on v3.0.16 answers
//! `unrecognized subcommand`. An export document exists, but only behind
//! `POST /admin/api/export` (`v3:crates/gproxy-admin/src/route.rs`), so
//! producing one needs a **running** v3 instance and an administrator session.
//! An operator holding a `gproxy.db` and a stopped service cannot make one.
//!
//! So the file is the route, and v3 set the precedent for exactly this problem
//! one version earlier: `v3:crates/gproxy-app/src/migrate_v2/` opens a v2
//! database and translates out of it. This module is the same idea with the
//! versions moved on by one.
//!
//! # Read-only means read-only
//!
//! The connection is opened `mode=ro`, so the driver refuses a write rather
//! than relying on this module not to attempt one. Nothing here issues DDL,
//! nothing writes a marker into the source, and the file is left byte for byte
//! as it was — which is what makes the migration rollback-able: if the result
//! is wrong, the v3 service starts again on the same file.
//!
//! An operator should still **stop the v3 service first**. `mode=ro` protects
//! the file from this process, not from a concurrent writer: reading a database
//! mid-transaction can produce a torn view, and for the OAuth channels it is
//! worse than torn — `claudecode` and `codex` rotate their refresh tokens on
//! use, so a second instance that starts up and refreshes one invalidates the
//! copy the original is still holding.
//!
//! # What is read, and what is deliberately not
//!
//! This reader loads configuration and identity. Usage is streamed separately
//! by `usage` so large histories do not have to fit in memory. Captures,
//! request logs, sessions and quota observations remain in the v3 backup.
//!
//! # The shapes that are not the export's
//!
//! Three tables are stored differently from the way v3's admin API serves
//! them, so this module converts rather than deserializing:
//!
//! - `rules` keeps `kind` in a column and the rest in `config_json`, where the
//!   DTO nests them; and its `transform` locate is `{"path": …}` where the DTO
//!   writes `{"type": "path", "value": …}`
//!   (`v3:crates/gproxy-admin/src/dto/rules.rs`, `RuleConfigDto::storage`).
//! - `rules.filter_operations_json` is a JSON array in a text column.
//! - `settings` is a key/value table, which the export omits entirely.

use std::path::Path;

use sea_orm::{Database, DatabaseConnection};

use super::document::Document;
use crate::{Error, Result};

/// Open a v3 database read-only and read everything the migration needs.
pub async fn read(path: &Path) -> Result<Document> {
    if !path.is_file() {
        return Err(Error::other(format!(
            "{} is not a readable file. --from-v3 takes either a v3 SQLite database or a \
             document from v3's `POST /admin/api/export`.",
            path.display()
        )));
    }
    let connection = open(path).await?;
    // The same check every other entry point makes, for the opposite reason:
    // here a database that is *not* v3's is the error.
    match super::detect::inspect(&connection).await? {
        super::detect::Verdict::Version3 { .. } => {}
        _ => {
            return Err(Error::other(format!(
                "{} is not a GPROXY v3 database: it carries no `schema_migrations` ledger. \
                 --from-v3 reads a v3 SQLite file or a v3 export document; a v4 database is \
                 already v4.",
                path.display()
            )));
        }
    }
    let source = gproxy_app::v3::source::Source::new(&connection, "").await?;
    Ok(gproxy_app::v3::source::read(&source).await?)
}

/// Open the native SQLite source read-only.
pub(super) async fn open(path: &Path) -> Result<DatabaseConnection> {
    // `mode=ro`: the driver refuses a write, rather than this module promising
    // not to attempt one.
    let url = format!("sqlite://{}?mode=ro", path.to_string_lossy());
    Database::connect(&url).await.map_err(|error| {
        Error::other(format!(
            "could not open {} read-only: {error}",
            path.display()
        ))
    })
}

/// The settings key this migration writes into the **destination**, so that a
/// second run against the same source is a no-op rather than a second set of
/// rows.
///
/// The scheme is v3's own, moved on by one: it wrote
/// `v2_import_{sha256(canonical path)}` after importing a v2 database
/// (`v3:crates/gproxy-app/src/migrate_v2/mod.rs`, `source_marker`). Keyed by
/// path rather than by content because the content changes as the v3 instance
/// keeps running, and "have I already imported *that file*" is the question an
/// operator is asking.
pub fn source_marker(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let resolved = path
        .canonicalize()
        .map_err(|error| Error::other(format!("could not resolve {}: {error}", path.display())))?;
    let digest = Sha256::digest(resolved.as_os_str().as_encoded_bytes());
    Ok(format!("v3_import_{}", crate::v3::hex(&digest)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::ConnectionTrait;

    #[test]
    fn the_marker_is_v3s_own_scheme_with_the_version_moved_on() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let marker = source_marker(file.path()).unwrap();
        assert!(marker.starts_with("v3_import_"), "{marker}");
        // 64 hex characters of SHA-256, like v3's.
        assert_eq!(marker.len(), "v3_import_".len() + 64);
        // Deterministic for one path, and different for another.
        assert_eq!(marker, source_marker(file.path()).unwrap());
        let other = tempfile::NamedTempFile::new().unwrap();
        assert_ne!(marker, source_marker(other.path()).unwrap());
    }

    /// The capability columns and side tables become the metadata object v3's
    /// export would have carried.
    #[tokio::test]
    async fn a_models_capability_columns_and_side_tables_become_its_metadata() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let url = format!("sqlite://{}?mode=rwc", file.path().to_string_lossy());
        let connection = Database::connect(&url).await.unwrap();
        for sql in [
            "CREATE TABLE settings (key TEXT, value_json TEXT)",
            "CREATE TABLE provider_models (id INTEGER, provider_id INTEGER, model_id TEXT, \
             context_window INTEGER, max_context_window INTEGER, thinking_supported INTEGER, \
             batch_supported INTEGER, description TEXT, search_supported INTEGER, \
             input_modalities_known INTEGER, parameters_known INTEGER, enabled INTEGER)",
            "INSERT INTO provider_models VALUES (1, 7, 'm', 200000, 1000000, 1, 0, 'desc', NULL, 1, 1, 1)",
            "CREATE TABLE provider_model_modalities (provider_id INTEGER, model_id TEXT, \
             direction TEXT, modality TEXT, sort_order INTEGER)",
            "INSERT INTO provider_model_modalities VALUES (7, 'm', 'input', 'image', 1), \
             (7, 'm', 'input', 'text', 0), (7, 'other', 'input', 'audio', 0)",
            "CREATE TABLE provider_model_reasoning_levels (provider_id INTEGER, model_id TEXT, \
             effort TEXT, description TEXT, sort_order INTEGER)",
            "INSERT INTO provider_model_reasoning_levels VALUES (7, 'm', 'high', 'deep', 0)",
        ] {
            connection
                .execute_unprepared(sql)
                .await
                .unwrap_or_else(|error| panic!("{sql}: {error}"));
        }
        let source = gproxy_app::v3::source::Source::new(&connection, "")
            .await
            .unwrap();
        let models = gproxy_app::v3::source::read(&source)
            .await
            .unwrap()
            .data
            .provider_models;
        let model = &models[0];
        assert_eq!(model.context_window, Some(200000));
        assert_eq!(model.thinking_supported, Some(true));
        assert_eq!(
            model.metadata,
            serde_json::json!({
                "description": "desc",
                "max_context_window": 1000000,
                "batch_supported": false,
                "input_modalities": ["text", "image"],
                // Known and empty: "none", not "unknown".
                "supported_parameters": [],
                "reasoning_levels": [{"effort": "high", "description": "deep"}],
            })
        );
    }

    #[tokio::test]
    async fn a_path_that_is_not_a_file_says_what_from_v3_accepts() {
        let error = read(Path::new("/nonexistent/gproxy.db"))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a readable file"), "{error}");
        assert!(error.contains("export"), "{error}");
    }
}
