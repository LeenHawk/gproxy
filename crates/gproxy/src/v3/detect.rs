//! Identify the schema before startup or source import. The store owns the
//! ledger gate; a v3 verdict selects the automatic SQLite upgrade path.

use gproxy_seaorm::SchemaSyncConnectionTrait;
use gproxy_store::SchemaState;
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

/// Classify through the store's schema gate, then explain a v3 refusal.
pub async fn inspect<C: SchemaSyncConnectionTrait>(connection: &C) -> crate::Result<Verdict> {
    let SchemaState::Foreign { tables } = SchemaState::read(connection).await? else {
        return Ok(Verdict::Ours);
    };
    if tables.iter().any(|table| table == "schema_migrations") {
        let mut tells = vec!["a `schema_migrations` ledger is present, which is v3's own"];
        for (tell, sql) in TELLS {
            if probe(connection, sql).await {
                tells.push(tell);
            }
        }
        if integer_user_ids(connection).await {
            tells.push("`users`.`id` holds an integer, where v4's holds a string");
        }
        return Ok(Verdict::Version3 { tells });
    }
    Ok(Verdict::Foreign {
        tables: tables.len() as u64,
    })
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

/// The error for a database that is neither version's. Deliberately not the
/// message above: telling somebody their Postgres schema "is a GPROXY v3
/// database" would send them to a migration that cannot help them.
pub fn refuse_foreign(target: &str, tables: u64) -> Error {
    Error::other(format!(
        "{target} already has {tables} tables and carries neither GPROXY's migration ledger \
         (`seaql_migrations`) nor v3's (`schema_migrations`), so there is no recorded schema to migrate forward from. \
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
        assert_eq!(inspect(&memory().await).await.unwrap(), Verdict::Ours);
    }

    /// A pre-migrator development database has no recorded schema to advance.
    #[tokio::test]
    async fn a_database_without_a_ledger_is_not_managed() {
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
        assert_eq!(
            inspect(&connection).await.unwrap(),
            Verdict::Foreign { tables: 2 }
        );
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
        assert_eq!(inspect(&connection).await.unwrap(), Verdict::Ours);
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
        assert_eq!(inspect(&connection).await.unwrap(), Verdict::Ours);
    }

    #[tokio::test]
    async fn v3s_ledger_is_decisive_and_the_tells_corroborate_it() {
        let Verdict::Version3 { tells } = inspect(&version3().await).await.unwrap() else {
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
        let Verdict::Version3 { tells } = inspect(&connection).await.unwrap() else {
            panic!("v3's ledger was not decisive on its own");
        };
        assert_eq!(tells.len(), 1);
    }

    /// Tables, no ledger, none of v4's columns: somebody else's database. This
    /// must not be reported as v3.
    #[tokio::test]
    async fn a_foreign_schema_is_refused_but_not_called_v3() {
        let connection = memory().await;
        run(&connection, "CREATE TABLE ar_internal_metadata (key text)").await;
        run(&connection, "CREATE TABLE widgets (id integer PRIMARY KEY)").await;
        let Verdict::Foreign { tables } = inspect(&connection).await.unwrap() else {
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
        assert_eq!(connection.table_names().await.unwrap().len(), 1);
    }
}
