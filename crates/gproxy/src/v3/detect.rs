//! Recognising a v3 database, and refusing to touch it.
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
//! Nothing here tries to rewrite a v3 file. v3's ids are `i64` where v4's are
//! `String`, its credential secrets are a four-column envelope where v4's are
//! one opaque blob, and its `settings` is a key/value table where v4's is one
//! wide row. That is not a sequence of `ALTER TABLE`s, and pretending it could
//! be is what produced the error above. The route is v3's own
//! `POST /admin/api/export` and [`super::import`], leaving the v3 file
//! untouched and rollback-able.
//!
//! # The tells
//!
//! Three positives, any one of which is decisive, and one escape hatch checked
//! first so a database that is already v4's can never be refused:
//!
//! | Probe | Why it is decisive |
//! |---|---|
//! | `settings.config_revision` reads | **v4's own marker.** Present ⇒ ours, stop looking. |
//! | `settings.key`, `settings.value_json` read | v3's settings was a key/value table; v4's is one wide row and has neither column. |
//! | `credentials.ciphertext`, `.wrapped_key` read | v3's envelope columns; v4 stores one `secret` blob. |
//! | `schema_migrations.version` reads | v3's and v2's migration ledger. v4 has no such table: it synchronizes an entity registry and keeps no version row. |
//!
//! Each is one statement against a table that either exists with those columns
//! or does not, so nothing here depends on what any row contains — an empty v3
//! database is recognised exactly as well as a full one. A database with none
//! of the four is either empty or foreign, and an empty one is the normal case.

use sea_orm::{ConnectionTrait, Statement};

use crate::Error;

/// What a database looks like from the outside, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// v4's own, or empty. Either way this build may synchronize it.
    Ours,
    /// v3's, or v2's: it holds tables this build does not own and whose shape
    /// no `ALTER TABLE` reaches.
    Legacy { tells: Vec<&'static str> },
}

/// v4's marker. Checked first and on its own: a database that answers this is
/// this build's, and no other probe can overrule it.
const V4_MARKER: &str = "SELECT config_revision FROM settings LIMIT 1";

/// One probe each, in the order they are reported.
const TELLS: [(&str, &str); 3] = [
    (
        "`settings` is a key/value table (`key`, `value_json`), which is v3's shape",
        "SELECT key, value_json FROM settings LIMIT 1",
    ),
    (
        "`credentials` has v3's envelope columns (`ciphertext`, `wrapped_key`)",
        "SELECT ciphertext, wrapped_key FROM credentials LIMIT 1",
    ),
    (
        "a `schema_migrations` table is present, which only v2 and v3 wrote",
        "SELECT version FROM schema_migrations LIMIT 1",
    ),
];

/// Look at a database without writing to it.
///
/// Every probe is a `SELECT`; a statement that fails means the table or the
/// column is not there, which is the answer rather than an error. A driver
/// failure that is *not* about a missing name therefore reads as "no tell",
/// and the ordinary schema synchronization gets its turn and its own error —
/// which is right: this function's job is to recognise v3, not to be a second
/// health check.
pub async fn inspect<C: ConnectionTrait>(connection: &C) -> Verdict {
    if probe(connection, V4_MARKER).await {
        return Verdict::Ours;
    }
    let mut tells = Vec::new();
    for (tell, sql) in TELLS {
        if probe(connection, sql).await {
            tells.push(tell);
        }
    }
    match tells.is_empty() {
        true => Verdict::Ours,
        false => Verdict::Legacy { tells },
    }
}

async fn probe<C: ConnectionTrait>(connection: &C, sql: &str) -> bool {
    let statement = Statement::from_string(connection.get_database_backend(), sql.to_owned());
    connection.query_one_raw(statement).await.is_ok()
}

/// The error an operator sees, naming what was found and the command that
/// actually migrates it.
///
/// Three things, in this order, because that is the order the questions arrive
/// in: what this is, what will not happen to it, and what to do.
pub fn refuse(target: &str, tells: &[&'static str]) -> Error {
    let mut message = format!("{target} looks like a GPROXY v3 database:\n");
    for tell in tells {
        message.push_str(&format!("  - {tell}\n"));
    }
    message.push_str(
        "\nv4 will not open or alter it. v3's tables cannot be reshaped in place — its ids are \
         integers where v4's are strings, its credential secrets are a four-column envelope \
         where v4's are one blob — so a schema synchronization would either fail halfway or \
         quietly leave a database that is neither.\n\n\
         To migrate it, leave this file alone and export from v3 instead:\n\n  \
         # on the v3 instance, as a signed-in administrator\n  \
         curl -X POST http://127.0.0.1:7070/admin/api/export \\\n    \
         -b cookies.txt -H 'content-type: application/json' \\\n    \
         -d '{\"include_secrets\": true}' > v3-export.json\n\n  \
         # then, against a *fresh* v4 data directory\n  \
         gproxy --data-dir ./v4-data migrate\n  \
         gproxy --data-dir ./v4-data import --from-v3 v3-export.json \\\n    \
         --source-master-key \"$GPROXY_MASTER_KEY\" --admin-password '…'\n\n\
         See the v3-to-v4 page in the documentation for the whole procedure, including what \
         does not come across.",
    );
    Error::other(message)
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

    #[tokio::test]
    async fn an_empty_database_is_ours_to_synchronize() {
        assert_eq!(inspect(&memory().await).await, Verdict::Ours);
    }

    #[tokio::test]
    async fn a_v4_database_is_never_refused() {
        let connection = memory().await;
        run(
            &connection,
            "CREATE TABLE settings (id INTEGER PRIMARY KEY, config_revision BIGINT NOT NULL)",
        )
        .await;
        // Even with a foreign `schema_migrations` beside it: v4's own marker
        // is checked first and is not overruled.
        run(
            &connection,
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER)",
        )
        .await;
        assert_eq!(inspect(&connection).await, Verdict::Ours);
    }

    /// The three tells, each on its own, because any one of them is enough.
    #[tokio::test]
    async fn each_tell_is_decisive_by_itself() {
        for (expected, ddl) in [
            (
                "`settings` is a key/value table",
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL)",
            ),
            (
                "v3's envelope columns",
                "CREATE TABLE credentials (id INTEGER PRIMARY KEY, ciphertext BLOB, \
                 wrapped_key BLOB, payload_nonce BLOB, key_nonce BLOB, kind TEXT)",
            ),
            (
                "`schema_migrations` table is present",
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER)",
            ),
        ] {
            let connection = memory().await;
            run(&connection, ddl).await;
            let Verdict::Legacy { tells } = inspect(&connection).await else {
                panic!("`{ddl}` was not recognised as v3");
            };
            assert_eq!(tells.len(), 1, "{tells:?}");
            assert!(tells[0].contains(expected), "{tells:?}");
        }
    }

    /// A hand-built database with v3's actual shape, which is the same verdict
    /// the real `data/gproxy.db` gives. Every tell fires, and the message says
    /// all three things.
    #[tokio::test]
    async fn a_v3_shaped_database_is_refused_with_the_command_that_migrates_it() {
        let connection = memory().await;
        for ddl in [
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER \
             NOT NULL)",
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL)",
            "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, \
             organization_id INTEGER, team_id INTEGER, password_hash TEXT, \
             enabled INTEGER NOT NULL, is_admin INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE credentials (id INTEGER PRIMARY KEY AUTOINCREMENT, \
             provider_id INTEGER NOT NULL, label TEXT, ciphertext BLOB NOT NULL, \
             wrapped_key BLOB NOT NULL, payload_nonce BLOB NOT NULL, key_nonce BLOB NOT NULL, \
             version INTEGER NOT NULL, enabled INTEGER NOT NULL, kind TEXT NOT NULL)",
        ] {
            run(&connection, ddl).await;
        }
        let Verdict::Legacy { tells } = inspect(&connection).await else {
            panic!("a v3-shaped database was not recognised");
        };
        assert_eq!(tells.len(), 3, "{tells:?}");

        let message = refuse("data/gproxy.db", &tells).to_string();
        // What it is, what will not happen to it, and what to do instead.
        assert!(message.contains("data/gproxy.db looks like a GPROXY v3 database"));
        assert!(message.contains("will not open or alter it"));
        assert!(message.contains("import --from-v3"));
        assert!(message.contains("/admin/api/export"));
        // And every tell it found, so the verdict is auditable.
        for tell in &tells {
            assert!(message.contains(tell), "{message}");
        }
    }

    /// v4's marker is a column and not just a table: a v3 `settings` exists
    /// too, and confusing the two is exactly how the original bug got past its
    /// own warnings.
    #[tokio::test]
    async fn a_settings_table_alone_is_not_v4s_marker() {
        let connection = memory().await;
        run(
            &connection,
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL)",
        )
        .await;
        assert!(!probe(&connection, V4_MARKER).await);
        assert!(matches!(inspect(&connection).await, Verdict::Legacy { .. }));
    }
}
