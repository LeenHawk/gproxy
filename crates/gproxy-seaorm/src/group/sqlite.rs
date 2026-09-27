//! Pipeline matching writes to SQLite's worker. Combining only COMMITs still
//! leaves a scheduler round trip between every pair of SQL statements; SQLite's
//! executor can run a bounded set of prepared statements consecutively.
//!
//! Values remain parameters. Only single statements with an exact set of
//! anonymous binds can be joined: numbered binds, scripts and mixed read/write
//! batches keep the ordinary execution path, with their original semantics.

use futures_util::TryStreamExt;
use sea_orm::{
    DbErr, RuntimeErr,
    sea_query::Values,
    sqlx::{self, AssertSqlSafe, Either, Executor, SqliteConnection, sqlite::SqlitePool},
};
use sea_query_sqlx::SqlxValues;

use crate::{BatchResult, BatchStatement};

struct Prepared {
    sql: String,
    values: Values,
    lengths: Vec<usize>,
    binds: usize,
}

fn prepare<'a>(groups: impl Iterator<Item = &'a [BatchStatement]>) -> Option<Prepared> {
    let groups = groups.collect::<Vec<_>>();
    let BatchStatement::Execute(first) = groups.first()?.first()? else {
        return None;
    };
    let binds = anonymous_binds(&first.sql)?;
    // A changing mix of SQL would fill the driver's statement cache with
    // different scripts, each holding many copies of the same prepared SQL.
    // Keep that mix on the ordinary path and pipeline matching writes only.
    for group in &groups {
        for step in *group {
            let BatchStatement::Execute(statement) = step else {
                return None;
            };
            if statement.sql != first.sql
                || statement.values.as_ref().map_or(0, |v| v.0.len()) != binds
            {
                return None;
            }
        }
    }
    Some(Prepared {
        // A trailing line comment must end before the separator.
        sql: format!("{}\n;\n", first.sql),
        values: Values(
            groups
                .iter()
                .flat_map(|group| group.iter())
                .flat_map(|step| {
                    step.statement()
                        .values
                        .iter()
                        .flat_map(|values| values.0.iter().cloned())
                })
                .collect(),
        ),
        lengths: groups.iter().map(|group| group.len()).collect(),
        binds,
    })
}

/// Count anonymous binds without interpreting literals, identifiers or comments
/// as SQL. Return None for a script or non-anonymous parameters: appending such
/// a statement would change its argument indices or its result boundaries.
fn anonymous_binds(sql: &str) -> Option<usize> {
    let mut bytes = sql.bytes().peekable();
    let mut count = 0;
    let mut has_sql = false;
    while let Some(byte) = bytes.next() {
        match byte {
            b'\'' | b'"' | b'`' => {
                has_sql = true;
                loop {
                    if bytes.next()? == byte {
                        if bytes.peek() != Some(&byte) {
                            break;
                        }
                        bytes.next();
                    }
                }
            }
            b'[' => {
                has_sql = true;
                while bytes.next()? != b']' {}
            }
            b'-' if bytes.peek() == Some(&b'-') => {
                for byte in bytes.by_ref() {
                    if byte == b'\n' {
                        break;
                    }
                }
            }
            b'/' if bytes.peek() == Some(&b'*') => {
                bytes.next();
                loop {
                    if bytes.next()? == b'*' && bytes.peek() == Some(&b'/') {
                        bytes.next();
                        break;
                    }
                }
            }
            b'?' => {
                has_sql = true;
                if bytes.peek().is_some_and(u8::is_ascii_digit) {
                    return None;
                }
                count += 1;
            }
            b';' | b'$' | b':' | b'@' => return None,
            _ if !byte.is_ascii_whitespace() => has_sql = true,
            _ => {}
        }
    }
    has_sql.then_some(count)
}

fn error(error: sqlx::Error) -> DbErr {
    DbErr::Exec(RuntimeErr::SqlxError(error.into()))
}

pub(super) async fn run<'a>(
    pool: &SqlitePool,
    groups: impl Iterator<Item = &'a [BatchStatement]>,
) -> Option<Result<Vec<Vec<BatchResult>>, DbErr>> {
    let batch = prepare(groups)?;
    Some(execute(pool, batch).await)
}

async fn execute(pool: &SqlitePool, batch: Prepared) -> Result<Vec<Vec<BatchResult>>, DbErr> {
    let mut transaction = pool.begin().await.map_err(error)?;
    match statements(&mut transaction, batch).await {
        Ok(results) => {
            transaction.commit().await.map_err(error)?;
            Ok(results)
        }
        Err(cause) => {
            // OR ROLLBACK may already have ended the transaction. Preserve
            // the statement error, as the ordinary batch path does.
            let _ = transaction.rollback().await;
            Err(cause)
        }
    }
}

async fn statements(
    connection: &mut SqliteConnection,
    batch: Prepared,
) -> Result<Vec<Vec<BatchResult>>, DbErr> {
    // Use the SQLite executor's bound multi-statement support, rather than
    // interpolating values into raw SQL. Each Left marks one SQL statement's
    // completion; RETURNING rows are discarded just as execute_raw discards
    // them. The transaction is committed only after the entire stream ends.
    let count = batch.lengths.iter().sum::<usize>();
    // Power-of-two chunks keep only six reusable shapes per SQL template, and
    // at most 32 prepared statements in an entry. A full group still uses far
    // fewer worker commands than one command per statement.
    const MAX_CHUNK: usize = 32;
    let mut values = batch.values.0.into_iter();
    let mut results = Vec::with_capacity(count);
    let mut remaining = count;
    while remaining > 0 {
        let length = 1_usize << remaining.min(MAX_CHUNK).ilog2();
        let query = sqlx::query_with(
            AssertSqlSafe(batch.sql.repeat(length)),
            SqlxValues(Values(values.by_ref().take(length * batch.binds).collect())),
        );
        let mut stream = (&mut *connection).fetch_many(query);
        while let Some(result) = stream.try_next().await.map_err(error)? {
            if let Either::Left(result) = result {
                results.push(BatchResult::Executed(result.into()));
            }
        }
        remaining -= length;
    }
    if results.len() != count {
        // Empty statements and unusual SQLite extensions keep the old path.
        // The caller rolls this transaction back before replaying each job.
        return Err(DbErr::Custom(
            "SQLite write group result count changed".into(),
        ));
    }
    let mut results = results.into_iter();
    Ok(batch
        .lengths
        .into_iter()
        .map(|length| results.by_ref().take(length).collect())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BatchConnectionTrait;
    use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, Statement};

    async fn database() -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        db.execute_unprepared("CREATE TABLE t (id INTEGER PRIMARY KEY, text TEXT, bytes BLOB)")
            .await
            .unwrap();
        crate::group::install(&db);
        db
    }

    #[tokio::test]
    async fn bound_groups_preserve_values_and_each_execution_result() {
        let db = database().await;
        let text = "'；中文\0'; DROP TABLE t; -- ?1";
        let bytes = vec![0_u8, 255, 39];
        let sql = "INSERT OR IGNORE INTO t (id, text, bytes) VALUES (?, ?, ?) RETURNING ';?''$1:@' -- ? in a comment";
        let first = vec![
            BatchStatement::Execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                [1_i64.into(), text.into(), bytes.clone().into()],
            )),
            BatchStatement::Execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                [1_i64.into(), "ignored".into(), bytes.clone().into()],
            )),
        ];
        let second = vec![BatchStatement::Execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            [
                i64::MAX.into(),
                ";?'$1:@".into(),
                sea_orm::Value::Bytes(None),
            ],
        ))];
        let results = run(
            db.get_sqlite_connection_pool(),
            [first.as_slice(), second.as_slice()].into_iter(),
        )
        .await
        .expect("the statements use the bound group path")
        .unwrap();
        let affected: Vec<Vec<u64>> = results
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|result| match result {
                        BatchResult::Executed(result) => result.rows_affected(),
                        _ => panic!("execution result expected"),
                    })
                    .collect()
            })
            .collect();
        assert_eq!(affected, [vec![1, 0], vec![1]]);
        let rows = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT * FROM t ORDER BY id",
            ))
            .await
            .unwrap();
        assert_eq!(rows[0].try_get::<String>("", "text").unwrap(), text);
        assert_eq!(rows[0].try_get::<Vec<u8>>("", "bytes").unwrap(), bytes);
        assert_eq!(rows[1].try_get::<i64>("", "id").unwrap(), i64::MAX);
        assert_eq!(rows[1].try_get::<String>("", "text").unwrap(), ";?'$1:@");
    }

    #[tokio::test]
    async fn a_failed_bound_group_rolls_back_before_the_next_batch() {
        let db = database().await;
        // Fail after several worker commands, so rollback must cover earlier
        // chunks too, not just the command which saw the duplicate.
        let steps = (1_i64..=65)
            .chain(std::iter::once(1))
            .map(|id| {
                BatchStatement::Execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT OR ROLLBACK INTO t (id) VALUES (?)",
                    [id.into()],
                ))
            })
            .collect::<Vec<_>>();
        let error = run(
            db.get_sqlite_connection_pool(),
            std::iter::once(steps.as_slice()),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(
            error.to_string().contains("UNIQUE constraint failed"),
            "{error}"
        );
        // The first insert must have rolled back, and the pool must be usable.
        db.batch(&steps[..1]).await.unwrap();
    }

    #[tokio::test]
    async fn non_anonymous_binds_and_scripts_keep_their_original_scope() {
        let db = database().await;
        for (sql, values) in [
            ("INSERT INTO t (id) VALUES (?1)", vec![7_i64.into()]),
            ("INSERT INTO t (id) VALUES ($1)", vec![8_i64.into()]),
            (
                "INSERT INTO t (id) VALUES (9); INSERT INTO t (id) VALUES (10)",
                vec![],
            ),
            (
                "INSERT INTO t (id, text) VALUES (?, ?)",
                vec![11_i64.into()],
            ),
            (
                "INSERT INTO t (id) VALUES (?)",
                vec![12_i64.into(), "unused".into()],
            ),
            ("-- no statement", vec![]),
        ] {
            let steps = [BatchStatement::Execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                values,
            ))];
            assert!(prepare(std::iter::once(steps.as_slice())).is_none());
            db.batch(&steps).await.unwrap();
        }
        let rows = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT id FROM t ORDER BY id",
            ))
            .await
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.try_get::<i64>("", "id").unwrap())
                .collect::<Vec<_>>(),
            [7, 8, 9, 10, 11, 12]
        );
    }

    #[tokio::test]
    async fn dropping_a_caller_does_not_drop_its_queued_write() {
        use futures_util::FutureExt;

        let db = database().await;
        let held = db.get_sqlite_connection_pool().acquire().await.unwrap();
        let steps = [BatchStatement::Execute(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO t (id) VALUES (1)",
        ))];
        let mut pending = Box::pin(db.batch(&steps));
        assert!(pending.as_mut().now_or_never().is_none());
        drop(pending);
        drop(held);
        db.batch(&[BatchStatement::Execute(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO t (id) SELECT 2 FROM t WHERE id = 1",
        ))])
        .await
        .unwrap();
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS n FROM t",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    }
}
