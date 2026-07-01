//! Versioned schema migration framework, shared by the two SQL backends
//! (`db` SeaORM + `libsql` edge). The execution differs per backend (SeaORM
//! raw SQL vs. hand SQL over `LibsqlClient::execute`), but the *migration list*
//! and the version-ordering logic live here, once.
//!
//! ## Model
//!
//! A `schema_migrations(version BIGINT PRIMARY KEY, applied_at BIGINT)` table
//! tracks which migrations have been applied. On connect each SQL backend:
//!
//!   1. runs its existing `CREATE TABLE IF NOT EXISTS` baseline (entity-driven
//!      for `db`, the hand `TABLES` list for `libsql`) — this lands a *fresh*
//!      DB at the current schema, and is a no-op on an already-created DB;
//!   2. ensures `schema_migrations` exists;
//!   3. reads `MAX(version)` (0 when the table is empty);
//!   4. if the table is empty, **stamps [`latest_version`]** without running
//!      any SQL — step 1 just ensured the *current* schema, so every listed
//!      migration is already reflected in the tables and replaying one (e.g. an
//!      `ALTER TABLE ADD COLUMN`) would fail on the fresh DB;
//!   5. applies every [`MIGRATIONS`] entry with `version >` the stamped max, in
//!      ascending order, recording each in `schema_migrations`.
//!
//! Idempotent and safe to run on every boot.
//!
//! ## Support boundary
//!
//! A previously-unstamped DB is assumed to already match the current schema
//! (step 4). That holds for any DB first booted on or after this framework
//! landed; DBs created by *earlier* builds whose tables predate later column
//! changes are **not upgradable in place** — recreate them (or `ALTER` by
//! hand).
//!
//! ## Adding a migration
//!
//! Every schema change must land in **three places**, or it breaks on one
//! class of database:
//!
//!   - the SeaORM entity (`db/entities/`) — fresh DBs on the `db` backend;
//!   - the `libsql` `TABLES` list (`libsql/schema.rs`) — fresh edge DBs;
//!   - a [`Migration`] entry here — **existing** DBs on both backends.
//!
//! Append the [`Migration`] with the next integer version (keep the list
//! sorted ascending). Use [`MigrationSql::Shared`] for dialect-portable SQL,
//! or [`MigrationSql::ByDialect`] when a backend needs different DDL. Example:
//!
//! ```ignore
//! Migration {
//!     version: 2,
//!     description: "add providers.region",
//!     sql: MigrationSql::Shared(&["ALTER TABLE providers ADD COLUMN region TEXT"]),
//! }
//! ```

/// The version stamped for the auto-created baseline schema. Migrations in
/// [`MIGRATIONS`] must use versions strictly greater than this.
pub const BASELINE_VERSION: i64 = 1;

/// A single ordered schema migration. SQL statements execute in list order
/// (one statement per backend call).
pub struct Migration {
    /// Strictly-increasing version; must be `> BASELINE_VERSION` and unique.
    pub version: i64,
    /// Human-readable note for diagnostics.
    pub description: &'static str,
    /// Statements to run, in order.
    pub sql: MigrationSql,
}

#[derive(Clone, Copy)]
pub enum MigrationDialect {
    Sqlite,
    Postgres,
    MySql,
}

#[derive(Clone, Copy)]
pub enum MigrationSql {
    Shared(&'static [&'static str]),
    ByDialect {
        sqlite: &'static [&'static str],
        postgres: &'static [&'static str],
        mysql: &'static [&'static str],
    },
}

impl Migration {
    pub fn sql_for(&self, dialect: MigrationDialect) -> &'static [&'static str] {
        match self.sql {
            MigrationSql::Shared(sql) => sql,
            MigrationSql::ByDialect {
                sqlite,
                postgres,
                mysql,
            } => match dialect {
                MigrationDialect::Sqlite => sqlite,
                MigrationDialect::Postgres => postgres,
                MigrationDialect::MySql => mysql,
            },
        }
    }
}

/// Portable DDL for the bookkeeping table. `BIGINT` matches the Rust `i64`
/// migration metadata; no autoincrement needed (the version is supplied
/// explicitly).
pub const CREATE_MIGRATIONS_TABLE: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (\
     version BIGINT PRIMARY KEY, \
     applied_at BIGINT NOT NULL)";

/// Highest applied version, reading the bookkeeping table.
pub const SELECT_MAX_VERSION: &str = "SELECT COALESCE(MAX(version), 0) AS v FROM schema_migrations";

/// Dialect-specific version read. PostgreSQL reports legacy `INTEGER`
/// migration tables as `INT4`, and sqlx will not decode that into `i64`
/// unless the result expression is explicitly widened.
pub fn select_max_version_sql(dialect: MigrationDialect) -> &'static str {
    match dialect {
        MigrationDialect::Postgres => {
            "SELECT CAST(COALESCE(MAX(version), 0) AS BIGINT) AS v FROM schema_migrations"
        }
        MigrationDialect::Sqlite | MigrationDialect::MySql => SELECT_MAX_VERSION,
    }
}

/// Ordered list of migrations to apply *after* the baseline. Append new
/// entries here (see the module docs).
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 2,
        description: "routes.settings_json: per-route circuit-breaker override (§3.2)",
        sql: MigrationSql::Shared(&["ALTER TABLE routes ADD COLUMN settings_json TEXT"]),
    },
    Migration {
        version: 3,
        description: "instance_settings.retention_days: usage/log purge window (§8-D)",
        sql: MigrationSql::Shared(&[
            "ALTER TABLE instance_settings ADD COLUMN retention_days INTEGER",
        ]),
    },
    Migration {
        version: 4,
        description: "downstream_requests.response_body: captured downstream response body (§8-D)",
        sql: MigrationSql::Shared(&[
            "ALTER TABLE downstream_requests ADD COLUMN response_body TEXT",
        ]),
    },
    Migration {
        version: 5,
        description: "upstream_requests.response_body: captured upstream response body (§8-D)",
        sql: MigrationSql::Shared(&["ALTER TABLE upstream_requests ADD COLUMN response_body TEXT"]),
    },
    Migration {
        version: 6,
        description: "aliases: scoped regex model aliases",
        sql: MigrationSql::ByDialect {
            sqlite: &[
                "CREATE TABLE aliases_v6 (\
                    id INTEGER PRIMARY KEY, \
                    provider TEXT NOT NULL, \
                    alias TEXT NOT NULL, \
                    target TEXT NOT NULL, \
                    sort_order INTEGER NOT NULL, \
                    enabled INTEGER NOT NULL, \
                    created_at INTEGER NOT NULL, \
                    updated_at INTEGER NOT NULL, \
                    UNIQUE(provider, alias))",
                "INSERT INTO aliases_v6 \
                    (id, provider, alias, target, sort_order, enabled, created_at, updated_at) \
                    SELECT id, '*', alias, alias, 0, 1, created_at, updated_at FROM aliases",
                "DROP TABLE aliases",
                "ALTER TABLE aliases_v6 RENAME TO aliases",
            ],
            postgres: &[
                "CREATE TABLE aliases_v6 (\
                    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY, \
                    provider TEXT NOT NULL, \
                    alias TEXT NOT NULL, \
                    target TEXT NOT NULL, \
                    sort_order BIGINT NOT NULL, \
                    enabled BOOLEAN NOT NULL, \
                    created_at BIGINT NOT NULL, \
                    updated_at BIGINT NOT NULL, \
                    UNIQUE(provider, alias))",
                "INSERT INTO aliases_v6 \
                    (id, provider, alias, target, sort_order, enabled, created_at, updated_at) \
                    SELECT id, '*', alias, alias, 0, TRUE, created_at, updated_at FROM aliases",
                "DROP TABLE aliases",
                "ALTER TABLE aliases_v6 RENAME TO aliases",
                "SELECT setval(\
                    pg_get_serial_sequence('aliases', 'id'), \
                    COALESCE((SELECT MAX(id) FROM aliases), 1), \
                    (SELECT COUNT(*) > 0 FROM aliases))",
            ],
            mysql: &[
                "CREATE TABLE aliases_v6 (\
                    id BIGINT NOT NULL AUTO_INCREMENT, \
                    provider VARCHAR(255) NOT NULL, \
                    alias VARCHAR(255) NOT NULL, \
                    target TEXT NOT NULL, \
                    sort_order BIGINT NOT NULL, \
                    enabled BOOLEAN NOT NULL, \
                    created_at BIGINT NOT NULL, \
                    updated_at BIGINT NOT NULL, \
                    PRIMARY KEY (id), \
                    UNIQUE KEY uq_aliases_provider_alias (provider, alias))",
                "INSERT INTO aliases_v6 \
                    (id, provider, alias, target, sort_order, enabled, created_at, updated_at) \
                    SELECT id, '*', alias, alias, 0, TRUE, created_at, updated_at FROM aliases",
                "DROP TABLE aliases",
                "ALTER TABLE aliases_v6 RENAME TO aliases",
            ],
        },
    },
    Migration {
        version: 7,
        description: "usage_rollups: cache_write/cache_read token columns (chart metrics)",
        sql: MigrationSql::Shared(&[
            "ALTER TABLE usage_rollups ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE usage_rollups ADD COLUMN cache_read_tokens INTEGER NOT NULL DEFAULT 0",
        ]),
    },
];

/// Migrations with `version > current`, in ascending order — the work a runner
/// must apply. Pulled out as a pure function so the ordering logic is unit-
/// testable without a database.
pub fn pending(current: i64) -> Vec<&'static Migration> {
    let mut out: Vec<&Migration> = MIGRATIONS.iter().filter(|m| m.version > current).collect();
    out.sort_by_key(|m| m.version);
    out
}

/// The version to stamp on a previously-unstamped DB: the highest listed
/// migration ([`BASELINE_VERSION`] when the list is empty). The runners call
/// this right after their create routine ensured the *current* schema, so
/// every listed migration is already reflected in the tables — stamping
/// anything lower would replay DDL against a schema that already has it.
pub fn latest_version() -> i64 {
    MIGRATIONS
        .iter()
        .map(|m| m.version)
        .max()
        .unwrap_or(BASELINE_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_filters_and_orders_by_version() {
        // From a fresh baseline stamp, every listed migration is pending.
        let all = pending(BASELINE_VERSION);
        assert_eq!(
            all.iter().map(|m| m.version).collect::<Vec<_>>(),
            MIGRATIONS.iter().map(|m| m.version).collect::<Vec<_>>(),
        );
        // Versions are strictly ascending and all above the baseline.
        let mut prev = BASELINE_VERSION;
        for m in &all {
            assert!(m.version > prev, "versions must strictly ascend");
            assert!(m.version > BASELINE_VERSION, "must be above baseline");
            prev = m.version;
        }
        // Already at the top → nothing pending. This is also what a fresh DB
        // is stamped with: nothing may replay against the just-created schema.
        let top = MIGRATIONS
            .iter()
            .map(|m| m.version)
            .max()
            .unwrap_or(BASELINE_VERSION);
        assert_eq!(latest_version(), top);
        assert!(pending(latest_version()).is_empty());
    }

    #[test]
    fn postgres_max_version_query_widens_legacy_integer_tables() {
        assert_eq!(
            select_max_version_sql(MigrationDialect::Postgres),
            "SELECT CAST(COALESCE(MAX(version), 0) AS BIGINT) AS v FROM schema_migrations"
        );
        assert_eq!(
            select_max_version_sql(MigrationDialect::Sqlite),
            SELECT_MAX_VERSION
        );
        assert_eq!(
            select_max_version_sql(MigrationDialect::MySql),
            SELECT_MAX_VERSION
        );
    }
}
