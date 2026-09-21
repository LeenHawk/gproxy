//! Recognising a database this build does not own, and refusing to touch it.
//!
//! # The failure this exists to prevent
//!
//! `cargo run` with no arguments is `serve`, the default data directory is
//! `data/`, and on a developer's machine `data/gproxy.db` is quite likely to be
//! their **v3** database. What v4 did when that happened:
//!
//! ```text
//! WARN schema sync: column `users`.`id` is `integer` in the database but the
//!      entity defines `varchar`; sync is non-destructive and will not alter it
//! gproxy: Execution Error: error returned from database: (code: 1)
//!      Cannot add a NOT NULL column with default value NULL
//! ```
//!
//! It noticed, said it would not alter the column, and then went ahead and
//! tried to add columns to v3's tables anyway — dying on a raw SQLite string
//! with no mention of v3, of what it had been asked to do, or of what to do
//! instead. The file survived, but by the luck of which statement ran first.
//!
//! So: before any DDL, an instance that opens a database it does not own says
//! so and stops. This is checked in [`crate::instance::connect`], which is the
//! one place a connection is made, so `serve`, `migrate`, `bootstrap`, `export`
//! and both imports all get it — it is not a property of a command.
//!
//! # There is no in-place upgrade, and that is the design
//!
//! Nothing here rewrites a v3 file, and neither does the migration: it opens
//! one **read-only**. v3's ids are `i64` where v4's are `String`, its
//! credential secrets are a four-column envelope where v4's are one blob, and
//! its `settings` is a key/value table where v4's is one wide row. That is not
//! a sequence of `ALTER TABLE`s, and pretending it could be is what produced
//! the error above.
//!
//! # How the verdict is reached
//!
//! The decisive signal is **which migration ledger the database carries**,
//! because a ledger is a whole table rather than a column and neither version
//! can grow the other's by accident:
//!
//! | Ledger | Whose |
//! |---|---|
//! | `seaql_migrations` | SeaORM's own, which is what `gproxy-seaorm`'s `migrate_up` writes. Ours. |
//! | `schema_migrations` | v3's hand-rolled one — production's has ten rows. Theirs. |
//!
//! ## The one thing to know before changing this
//!
//! **Nothing in this workspace defines a migrator yet.** `gproxy-seaorm` has
//! the runner (`migration.rs`: `migrate_up`, `migrate_down`,
//! `migration_status`, and a D1 `MigrationProxy`), but there is no
//! `MigratorTrait` implementation anywhere and nothing calls it — only
//! `Store::sync` has ever run, and `sync` writes no ledger. Verified against a
//! freshly created v4 database: 55 tables, no `seaql_migrations`.
//!
//! So [`V4_LEDGER`] is checked first and is the *future* primary signal, while
//! [`V4_MARKER`] — `settings.config_revision`, a column only v4's wide settings
//! row has — is what actually recognises a v4 database today. When the baseline
//! migrator lands, `V4_MARKER` is the line to delete; the rest of this module
//! does not change. It is written this way round deliberately: if the two were
//! collapsed, a v4 database would start reading as foreign the moment the
//! ledger check became load-bearing, and the check would invert in silence.
//!
//! Everything else is corroboration, present in the message so an operator can
//! see *why* the conclusion was reached rather than being asked to trust it.

use sea_orm::{ConnectionTrait, Statement};

use crate::Error;

/// What a database looks like from the outside, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// v4's own, or empty. Either way this build may synchronize it.
    Ours,
    /// v3's, or v2's: it carries v3's ledger and tables whose shape no
    /// `ALTER TABLE` reaches.
    Version3 { tells: Vec<&'static str> },
    /// Tables, but neither version's ledger and none of v4's own columns. Not
    /// assumed to be v3: it is somebody else's database, and the answer is a
    /// different one.
    Foreign { tables: u64 },
}

/// SeaORM's ledger, written by `gproxy_seaorm`'s `migrate_up`. The primary
/// "this is ours" signal **once a migrator exists**; see the module note.
const V4_LEDGER: &str = "SELECT version FROM seaql_migrations LIMIT 1";

/// What recognises a v4 database *today*, because `Store::sync` writes no
/// ledger: `config_revision` is a column of v4's single wide `settings` row and
/// v3's key/value `settings` has nothing like it. Delete this when the baseline
/// migrator lands.
const V4_MARKER: &str = "SELECT config_revision FROM settings LIMIT 1";

/// v3's own hand-rolled ledger. Decisive on its own.
const V3_LEDGER: &str = "SELECT version FROM schema_migrations LIMIT 1";

/// Corroboration only. Each one is reported so the verdict is auditable, and
/// none of them decides it.
const TELLS: [(&str, &str); 2] = [
    (
        "`settings` is a key/value table (`key`, `value_json`), which is v3's shape",
        "SELECT key, value_json FROM settings LIMIT 1",
    ),
    (
        "`credentials` has v3's envelope columns (`ciphertext`, `wrapped_key`)",
        "SELECT ciphertext, wrapped_key FROM credentials LIMIT 1",
    ),
];

/// The third corroborating shape, which needs the value and not just that the
/// statement ran: every `users` table has an `id`, and what distinguishes the
/// versions is that v3's holds an integer where v4's holds a string.
const V3_USER_ID: &str = "SELECT typeof(id) AS kind FROM users LIMIT 1";

/// Look at a database without writing to it.
///
/// Every probe is a `SELECT`; a statement that fails means the table or the
/// column is not there, which is the answer rather than an error. A driver
/// failure that is *not* about a missing name therefore reads as "no signal",
/// and the ordinary schema synchronization gets its turn and its own error —
/// which is right: this function's job is to recognise a foreign database, not
/// to be a second health check.
pub async fn inspect<C: ConnectionTrait>(connection: &C) -> Verdict {
    // Ours, in the order of how reliable each signal is *right now*.
    if probe(connection, V4_LEDGER).await || probe(connection, V4_MARKER).await {
        return Verdict::Ours;
    }
    if probe(connection, V3_LEDGER).await {
        let mut tells = vec!["a `schema_migrations` ledger is present, which is v3's own"];
        for (tell, sql) in TELLS {
            if probe(connection, sql).await {
                tells.push(tell);
            }
        }
        if integer_user_ids(connection).await {
            tells.push("`users`.`id` holds an integer, where v4's holds a string");
        }
        return Verdict::Version3 { tells };
    }
    // No ledger and no v4 column. Empty is the normal first start; tables
    // without a ledger are somebody else's schema.
    match tables(connection).await {
        0 => Verdict::Ours,
        tables => Verdict::Foreign { tables },
    }
}

async fn probe<C: ConnectionTrait>(connection: &C, sql: &str) -> bool {
    let statement = Statement::from_string(connection.get_database_backend(), sql.to_owned());
    connection.query_one_raw(statement).await.is_ok()
}

/// Whether `users.id` actually holds an integer. SQLite-only, because
/// `typeof` is: on another backend this simply does not corroborate, and the
/// ledger has already decided.
async fn integer_user_ids<C: ConnectionTrait>(connection: &C) -> bool {
    if connection.get_database_backend() != sea_orm::DatabaseBackend::Sqlite {
        return false;
    }
    let statement =
        Statement::from_string(connection.get_database_backend(), V3_USER_ID.to_owned());
    connection
        .query_one_raw(statement)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get_by_index::<String>(0).ok())
        .is_some_and(|kind| kind == "integer")
}

/// How many tables the database has, across the three backends this binary
/// opens. A backend whose catalogue cannot be read counts as empty, which
/// leaves the ordinary synchronization to speak for itself.
async fn tables<C: ConnectionTrait>(connection: &C) -> u64 {
    use sea_orm::DatabaseBackend;
    let sql = match connection.get_database_backend() {
        DatabaseBackend::Sqlite => {
            "SELECT count(*) AS n FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%'"
        }
        DatabaseBackend::Postgres => {
            "SELECT count(*) AS n FROM information_schema.tables \
             WHERE table_schema = current_schema()"
        }
        DatabaseBackend::MySql => {
            "SELECT count(*) AS n FROM information_schema.tables \
             WHERE table_schema = database()"
        }
        _ => return 0,
    };
    let statement = Statement::from_string(connection.get_database_backend(), sql.to_owned());
    connection
        .query_one_raw(statement)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get_by_index::<i64>(0).ok())
        .and_then(|count| u64::try_from(count).ok())
        .unwrap_or_default()
}

/// The error an operator sees for a v3 database, naming what was found and the
/// command that actually migrates it.
///
/// Three things, in this order, because that is the order the questions arrive
/// in: what this is, what will not happen to it, and what to do.
pub fn refuse_version3(target: &str, tells: &[&'static str]) -> Error {
    let mut message = format!("{target} is a GPROXY v3 database:\n");
    for tell in tells {
        message.push_str(&format!("  - {tell}\n"));
    }
    message.push_str(
        "\nv4 will not open it for writing and will not alter it. v3's tables cannot be \
         reshaped in place — its ids are integers where v4's are strings, its credential \
         secrets are a four-column envelope where v4's are one blob — so a schema \
         synchronization would either fail halfway or quietly leave a database that is \
         neither.\n\n\
         Migrate it into a *fresh* v4 database instead, reading this file read-only and \
         leaving it exactly as it is:\n\n  \
         gproxy --data-dir ./v4-data migrate\n  \
         gproxy --data-dir ./v4-data import --from-v3 ",
    );
    message.push_str(target);
    message.push_str(
        " \\\n    --source-master-key \"$GPROXY_MASTER_KEY\" --admin-password '…'\n\n\
         Leave --source-master-key off if the v3 instance ran without one. See the v3-to-v4 \
         page in the documentation for the whole procedure, and for what does not come across.",
    );
    Error::other(message)
}

/// The error for a database that is neither version's. Deliberately not the
/// message above: telling somebody their Postgres schema "is a GPROXY v3
/// database" would send them to a migration that cannot help them.
pub fn refuse_foreign(target: &str, tables: u64) -> Error {
    Error::other(format!(
        "{target} already has {tables} tables and carries neither GPROXY's migration ledger \
         (`seaql_migrations`) nor v3's (`schema_migrations`), and none of v4's own columns. \
         This build will not synchronize a schema it does not recognise: creating its tables \
         alongside somebody else's is how two applications come to share a name and neither \
         works.\n\n\
         Point --data-dir or --dsn at a database of GPROXY's own. If this really is a GPROXY \
         database, it predates v3's ledger and has to be brought up to v3 first."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{Database, DatabaseConnection};

    async fn memory() -> DatabaseConnection {
        Database::connect("sqlite::memory:").await.unwrap()
    }

    async fn run(connection: &DatabaseConnection, sql: &str) {
        connection
            .execute_raw(Statement::from_string(
                connection.get_database_backend(),
                sql.to_owned(),
            ))
            .await
            .unwrap();
    }

    /// v3's ledger and the four tables that corroborate it.
    async fn version3() -> DatabaseConnection {
        let connection = memory().await;
        for ddl in [
            "CREATE TABLE schema_migrations (version integer NOT NULL PRIMARY KEY, \
             applied_at integer NOT NULL)",
            "CREATE TABLE settings (key text NOT NULL PRIMARY KEY, value_json text NOT NULL)",
            "CREATE TABLE users (id integer NOT NULL PRIMARY KEY AUTOINCREMENT, \
             name text NOT NULL UNIQUE, organization_id integer, team_id integer, \
             password_hash text, enabled integer NOT NULL, \
             is_admin integer NOT NULL DEFAULT (0))",
            "CREATE TABLE credentials (id integer NOT NULL PRIMARY KEY AUTOINCREMENT, \
             provider_id integer NOT NULL, label text, ciphertext blob NOT NULL, \
             wrapped_key blob NOT NULL, payload_nonce blob NOT NULL, key_nonce blob NOT NULL, \
             version integer NOT NULL, enabled integer NOT NULL, kind text NOT NULL)",
            "INSERT INTO schema_migrations VALUES (1, 1788347715)",
            "INSERT INTO users (name, enabled) VALUES ('root', 1)",
        ] {
            run(&connection, ddl).await;
        }
        connection
    }

    #[tokio::test]
    async fn an_empty_database_is_a_first_start() {
        assert_eq!(inspect(&memory().await).await, Verdict::Ours);
    }

    /// What a v4 database looks like **today**: `Store::sync` ran, so there is
    /// no ledger at all and only `settings.config_revision` says whose it is.
    #[tokio::test]
    async fn a_v4_database_without_a_ledger_is_still_ours() {
        let connection = memory().await;
        run(
            &connection,
            "CREATE TABLE settings (id integer PRIMARY KEY, config_revision bigint NOT NULL)",
        )
        .await;
        run(
            &connection,
            "CREATE TABLE providers (id varchar PRIMARY KEY)",
        )
        .await;
        assert_eq!(inspect(&connection).await, Verdict::Ours);
    }

    /// And what it will look like once the baseline migrator lands: the ledger
    /// alone is enough, with no `settings` row to read.
    #[tokio::test]
    async fn a_v4_database_with_seaorms_ledger_is_ours_on_that_alone() {
        let connection = memory().await;
        run(
            &connection,
            "CREATE TABLE seaql_migrations (version varchar PRIMARY KEY, \
             applied_at bigint NOT NULL)",
        )
        .await;
        assert_eq!(inspect(&connection).await, Verdict::Ours);
    }

    /// The ordering that matters: ours wins. A v4 database that had once been
    /// migrated from v3 in place would carry both ledgers, and refusing it
    /// would lock an operator out of their own working instance.
    #[tokio::test]
    async fn a_database_carrying_both_ledgers_is_ours() {
        let connection = version3().await;
        run(
            &connection,
            "CREATE TABLE seaql_migrations (version varchar PRIMARY KEY, \
             applied_at bigint NOT NULL)",
        )
        .await;
        assert_eq!(inspect(&connection).await, Verdict::Ours);
    }

    #[tokio::test]
    async fn v3s_ledger_is_decisive_and_the_tells_corroborate_it() {
        let Verdict::Version3 { tells } = inspect(&version3().await).await else {
            panic!("a v3 database was not recognised");
        };
        // The ledger, plus all three corroborating shapes.
        assert_eq!(tells.len(), 4, "{tells:?}");
        assert!(tells[0].contains("schema_migrations"));
        assert!(tells.iter().any(|tell| tell.contains("value_json")));
        assert!(tells.iter().any(|tell| tell.contains("wrapped_key")));
        assert!(tells.iter().any(|tell| tell.contains("`users`.`id`")));
    }

    /// The ledger alone, with nothing to corroborate it, is still v3: a v3
    /// instance that had only just created its schema is exactly this.
    #[tokio::test]
    async fn v3s_ledger_alone_is_enough() {
        let connection = memory().await;
        run(
            &connection,
            "CREATE TABLE schema_migrations (version integer PRIMARY KEY, \
             applied_at integer NOT NULL)",
        )
        .await;
        let Verdict::Version3 { tells } = inspect(&connection).await else {
            panic!("v3's ledger was not decisive on its own");
        };
        assert_eq!(tells.len(), 1);
    }

    #[tokio::test]
    async fn the_v3_refusal_says_what_it_is_what_will_not_happen_and_what_to_do() {
        let Verdict::Version3 { tells } = inspect(&version3().await).await else {
            unreachable!()
        };
        let message = refuse_version3("data/gproxy.db", &tells).to_string();
        assert!(message.contains("data/gproxy.db is a GPROXY v3 database"));
        assert!(message.contains("will not open it for writing"));
        assert!(message.contains("import --from-v3 data/gproxy.db"));
        // The file's own path, so the command can be pasted as printed.
        assert!(message.contains("--source-master-key"));
        for tell in &tells {
            assert!(message.contains(tell), "{message}");
        }
    }

    /// Tables, no ledger, none of v4's columns: somebody else's database. This
    /// must not be reported as v3.
    #[tokio::test]
    async fn a_foreign_schema_is_refused_but_not_called_v3() {
        let connection = memory().await;
        run(&connection, "CREATE TABLE ar_internal_metadata (key text)").await;
        run(&connection, "CREATE TABLE widgets (id integer PRIMARY KEY)").await;
        let Verdict::Foreign { tables } = inspect(&connection).await else {
            panic!("a foreign schema was not recognised");
        };
        assert_eq!(tables, 2);

        let message = refuse_foreign("the configured database", tables).to_string();
        assert!(message.contains("neither GPROXY's migration ledger"));
        assert!(!message.contains("is a GPROXY v3 database"));
        assert!(!message.contains("import --from-v3"));
    }

    /// `sqlite_sequence` is SQLite's own and is not somebody's table; a
    /// database holding only it is still empty.
    #[tokio::test]
    async fn sqlites_own_tables_do_not_count_as_a_foreign_schema() {
        let connection = memory().await;
        run(
            &connection,
            "CREATE TABLE t (id integer PRIMARY KEY AUTOINCREMENT)",
        )
        .await;
        run(&connection, "INSERT INTO t VALUES (1)").await;
        // `sqlite_sequence` now exists beside `t`, and only `t` is counted.
        assert_eq!(tables(&connection).await, 1);
    }
}
