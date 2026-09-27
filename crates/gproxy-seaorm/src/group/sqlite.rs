//! One worker command per batch, with individual SQL templates in the driver's
//! bounded cache. Callers encode parameters before enqueueing; the drainer only
//! moves ready commands. SQL and parameter payloads remain shared for retries.

use crate::{BatchResult, BatchStatement};
use sea_orm::{
    DbErr, RuntimeErr,
    sqlx::{
        self, AssertSqlSafe, IntoArguments, SqlSafeStr, SqliteConnection,
        sqlite::{SqliteBatchStatement, SqlitePool},
    },
};
use sea_query_sqlx::SqlxValues;
use std::sync::Arc;

pub(super) fn encode(steps: Vec<BatchStatement>) -> Vec<SqliteBatchStatement> {
    steps
        .into_iter()
        .map(|step| {
            let (statement, collect_rows) = match step {
                BatchStatement::Execute(statement) => (statement, false),
                BatchStatement::Query(query) => (query.statement, true),
            };
            SqliteBatchStatement {
                sql: AssertSqlSafe(Arc::<str>::from(statement.sql)).into_sql_str(),
                arguments: SqlxValues(
                    statement
                        .values
                        .unwrap_or(sea_orm::sea_query::Values(Vec::new())),
                )
                .into_arguments(),
                collect_rows,
            }
        })
        .collect()
}

fn error(error: sqlx::Error) -> DbErr {
    DbErr::Exec(RuntimeErr::SqlxError(error.into()))
}

async fn execute(
    connection: &mut SqliteConnection,
    steps: Vec<SqliteBatchStatement>,
) -> Result<Vec<BatchResult>, DbErr> {
    let queries: Vec<_> = steps.iter().map(|step| step.collect_rows).collect();
    let results = connection.execute_batch(steps).await.map_err(|failure| {
        if queries.get(failure.index).copied().unwrap_or(false) {
            DbErr::Query(RuntimeErr::SqlxError(failure.error.into()))
        } else {
            error(failure.error)
        }
    })?;
    Ok(results
        .into_iter()
        .zip(queries)
        .map(|(result, query)| {
            if query {
                BatchResult::Rows(result.rows.into_iter().map(Into::into).collect())
            } else {
                BatchResult::Executed(result.result.into())
            }
        })
        .collect())
}

async fn transaction(
    pool: &SqlitePool,
    steps: Vec<SqliteBatchStatement>,
) -> Result<Vec<BatchResult>, DbErr> {
    let mut transaction = pool.begin().await.map_err(error)?;
    match execute(&mut transaction, steps).await {
        Ok(results) => {
            transaction.commit().await.map_err(error)?;
            Ok(results)
        }
        Err(cause) => {
            let _ = transaction.rollback().await;
            Err(cause)
        }
    }
}

/// Execute the whole group optimistically. On failure roll back and retry each
/// job in order, retaining its isolation and its own result/error classification.
pub(super) async fn run_jobs(
    pool: &SqlitePool,
    jobs: &mut [super::Job],
) -> Vec<Result<Vec<BatchResult>, DbErr>> {
    let grouped = jobs.len() > 1;
    if grouped {
        let length = jobs.iter().map(|job| job.statements.len()).sum();
        let mut steps = Vec::with_capacity(length);
        for job in &mut *jobs {
            steps.append(&mut job.statements);
        }
        if let Ok(results) = transaction(pool, steps).await {
            let mut results = results.into_iter();
            return jobs
                .iter()
                .map(|job| Ok(results.by_ref().take(job.replay.len()).collect()))
                .collect();
        }
    }
    let mut results = Vec::with_capacity(jobs.len());
    for job in jobs {
        // Only a failed group needs another argument-vector copy. Payloads and
        // SQL text remain shared; nothing is re-encoded on the retry path.
        let steps = if grouped {
            job.replay.to_vec()
        } else {
            std::mem::take(&mut job.statements)
        };
        results.push(if job.transactional {
            transaction(pool, steps).await
        } else {
            match pool.acquire().await {
                Ok(mut connection) => execute(&mut connection, steps).await,
                Err(cause) => Err(error(cause)),
            }
        });
    }
    results
}

#[cfg(test)]
async fn run<'a>(
    pool: &SqlitePool,
    groups: impl Iterator<Item = &'a [BatchStatement]>,
) -> Result<Vec<Vec<BatchResult>>, DbErr> {
    let groups: Vec<_> = groups.collect();
    let lengths: Vec<_> = groups.iter().map(|group| group.len()).collect();
    let steps = encode(
        groups
            .into_iter()
            .flat_map(|group| group.iter().cloned())
            .collect(),
    );
    transaction(pool, steps).await.map(|results| {
        let mut results = results.into_iter();
        lengths
            .into_iter()
            .map(|length| results.by_ref().take(length).collect())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BatchConnectionTrait, BatchQuery, Projection};
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

    fn read(sql: &str, values: impl IntoIterator<Item = sea_orm::Value>) -> BatchStatement {
        BatchStatement::Query(BatchQuery::new(
            Statement::from_sql_and_values(DbBackend::Sqlite, sql, values),
            Projection::new(),
        ))
    }

    #[tokio::test]
    async fn mixed_batches_reuse_only_the_individual_sql_templates() {
        use sea_orm::sqlx::Connection;
        let db = database().await;
        let pool = db.get_sqlite_connection_pool();
        {
            let mut connection = pool.acquire().await.unwrap();
            connection.clear_cached_statements().await.unwrap();
        }
        // Vary both ordering and batch length. The cache must not contain
        // concatenated combinations, nor be bypassed for heterogeneous work.
        for length in 1..=40 {
            let mut steps = Vec::new();
            for id in 0..length {
                steps.push(BatchStatement::Execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT OR IGNORE INTO t (id) VALUES (?)",
                    [i64::from(id).into()],
                )));
                if id % 2 == 0 {
                    steps.push(read(
                        "SELECT id FROM t WHERE id = ?",
                        [i64::from(id).into()],
                    ));
                } else {
                    steps.push(BatchStatement::Execute(Statement::from_sql_and_values(
                        DbBackend::Sqlite,
                        "UPDATE t SET text = ? WHERE id = ?",
                        ["kept".into(), i64::from(id).into()],
                    )));
                }
            }
            run(pool, std::iter::once(steps.as_slice())).await.unwrap();
        }
        let connection = pool.acquire().await.unwrap();
        assert_eq!(connection.cached_statements_size(), 3);
    }

    #[tokio::test]
    async fn mixed_sql_keeps_rows_empty_results_and_execution_results_in_order() {
        let db = database().await;
        let write = vec![BatchStatement::Execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO t (id, text) VALUES (?, ?) RETURNING id",
            [1_i64.into(), "before; ? 中文".into()],
        ))];
        let changes = vec![
            read("SELECT text FROM t WHERE id = ?", [1_i64.into()]),
            read(
                "UPDATE t SET text = ? WHERE id = ? RETURNING text",
                ["after\0; ?".into(), 1_i64.into()],
            ),
            read("SELECT id FROM t WHERE id = ?", [99_i64.into()]),
        ];
        let last = vec![read("SELECT id, text FROM t ORDER BY id", [])];
        let result = run(
            db.get_sqlite_connection_pool(),
            [write.as_slice(), changes.as_slice(), last.as_slice()].into_iter(),
        )
        .await
        .unwrap();
        assert!(matches!(&result[0][0], BatchResult::Executed(r) if r.rows_affected() == 1));
        let BatchResult::Rows(before) = &result[1][0] else {
            panic!("query rows")
        };
        assert_eq!(
            before[0].try_get::<String>("", "text").unwrap(),
            "before; ? 中文"
        );
        let BatchResult::Rows(changed) = &result[1][1] else {
            panic!("returning rows")
        };
        assert_eq!(
            changed[0].try_get::<String>("", "text").unwrap(),
            "after\0; ?"
        );
        assert!(matches!(&result[1][2], BatchResult::Rows(rows) if rows.is_empty()));
        let BatchResult::Rows(last) = &result[2][0] else {
            panic!("query rows")
        };
        assert_eq!(last[0].try_get::<i64>("", "id").unwrap(), 1);
        assert_eq!(last[0].try_get::<String>("", "text").unwrap(), "after\0; ?");
    }

    #[tokio::test]
    async fn a_queued_query_observes_the_write_queued_before_it() {
        use futures_util::FutureExt;

        let db = database().await;
        let held = db.get_sqlite_connection_pool().acquire().await.unwrap();
        let mut write = Box::pin(db.atomic_batch_owned(vec![Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO t (id) VALUES (1)",
        )]));
        assert!(write.as_mut().now_or_never().is_none());
        let mut query = Box::pin(db.query_rows(BatchQuery::new(
            Statement::from_string(DbBackend::Sqlite, "SELECT id FROM t"),
            Projection::new(),
        )));
        assert!(query.as_mut().now_or_never().is_none());
        drop(held);
        let (written, rows) = tokio::join!(write, query);
        assert_eq!(written.unwrap()[0].rows_affected(), 1);
        assert_eq!(rows.unwrap()[0].try_get::<i64>("", "id").unwrap(), 1);
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
        // A failure late in a large batch must roll back every earlier write.
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
    async fn scripts_and_numbered_queries_keep_one_result_per_step() {
        let db = database().await;
        let results = db
            .batch_owned(vec![
                BatchStatement::Execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT INTO t (id) VALUES (?); INSERT INTO t (id) VALUES (?)",
                    [41_i64.into(), 42_i64.into()],
                )),
                read(
                    "SELECT ? AS value; SELECT ? AS value",
                    [7_i64.into(), 8_i64.into()],
                ),
                read("SELECT ?1 AS value, ?1 AS again", [9_i64.into()]),
                read("-- no query", []),
            ])
            .await
            .unwrap();
        assert_eq!(results.len(), 4);
        assert!(matches!(&results[0], BatchResult::Executed(r) if r.rows_affected() == 2));
        let BatchResult::Rows(rows) = &results[1] else {
            panic!("script rows")
        };
        assert_eq!(
            rows.iter()
                .map(|r| r.try_get::<i64>("", "value").unwrap())
                .collect::<Vec<_>>(),
            [7, 8]
        );
        let BatchResult::Rows(rows) = &results[2] else {
            panic!("numbered rows")
        };
        assert_eq!(rows[0].try_get::<i64>("", "value").unwrap(), 9);
        assert_eq!(rows[0].try_get::<i64>("", "again").unwrap(), 9);
        assert!(matches!(&results[3], BatchResult::Rows(rows) if rows.is_empty()));
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
