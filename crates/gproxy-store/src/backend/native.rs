use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_rusqlite::rusqlite;
use tokio_rusqlite::rusqlite::params_from_iter;
use tokio_rusqlite::rusqlite::types::Value;

use super::{DbValue, Executor, QueryResult, Row, Statement};
use crate::StoreError;

/// Jobs the connection thread folds into one transaction when they are
/// already queued. Every settlement is its own commit otherwise, and the
/// commit, not the statements, is what bounds sqlite throughput.
const GROUP_LIMIT: usize = 256;

pub(super) struct NativeSql {
    jobs: mpsc::Sender<Job>,
}

struct Job {
    statements: Vec<Statement>,
    transactional: bool,
    reply: oneshot::Sender<rusqlite::Result<Vec<QueryResult>>>,
}

impl NativeSql {
    pub(super) async fn open(path: PathBuf) -> Result<Self, StoreError> {
        let (jobs, queue) = mpsc::channel();
        let (ready, opened) = oneshot::channel();
        std::thread::Builder::new()
            .name("gproxy-sqlite".into())
            .spawn(move || match open_connection(path) {
                Ok(connection) => {
                    let _ = ready.send(Ok(()));
                    serve(connection, queue);
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(database_error)?;
        opened
            .await
            .map_err(database_error)?
            .map_err(database_error)?;
        Ok(Self { jobs })
    }

    fn submit(
        &self,
        statements: Vec<Statement>,
        transactional: bool,
    ) -> super::DbFuture<'_, Vec<QueryResult>> {
        let (reply, response) = oneshot::channel();
        let sent = self.jobs.send(Job {
            statements,
            transactional,
            reply,
        });
        Box::pin(async move {
            sent.map_err(|_| database_error("sqlite thread is gone"))?;
            response
                .await
                .map_err(|_| database_error("sqlite thread dropped the job"))?
                .map_err(database_error)
        })
    }
}

impl Executor for NativeSql {
    fn execute<'a>(&'a self, statement: Statement) -> super::DbFuture<'a, QueryResult> {
        let job = self.submit(vec![statement], false);
        Box::pin(async move {
            job.await?
                .pop()
                .ok_or_else(|| database_error("sqlite job returned no result"))
        })
    }

    fn batch<'a>(&'a self, statements: Vec<Statement>) -> super::DbFuture<'a, Vec<QueryResult>> {
        self.submit(statements, true)
    }
}

fn open_connection(path: PathBuf) -> rusqlite::Result<rusqlite::Connection> {
    let connection = rusqlite::Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "temp_store", "MEMORY")?;
    connection.set_prepared_statement_cache_capacity(256);
    Ok(connection)
}

fn serve(mut connection: rusqlite::Connection, queue: mpsc::Receiver<Job>) {
    while let Ok(first) = queue.recv() {
        let mut group = vec![first];
        while group.len() < GROUP_LIMIT
            && let Ok(job) = queue.try_recv()
        {
            group.push(job);
        }
        if group.len() == 1 {
            let job = group.pop().expect("one job");
            let result = run_job(&mut connection, &job);
            let _ = job.reply.send(result);
            continue;
        }
        match run_group(&mut connection, &group) {
            Ok(results) => {
                for (job, result) in group.into_iter().zip(results) {
                    let _ = job.reply.send(Ok(result));
                }
            }
            // One failure must not fail its neighbours: replay each job on
            // its own so the error lands only where it belongs.
            Err(_) => {
                for job in group {
                    let result = run_job(&mut connection, &job);
                    let _ = job.reply.send(result);
                }
            }
        }
    }
}

fn run_group(
    connection: &mut rusqlite::Connection,
    group: &[Job],
) -> rusqlite::Result<Vec<Vec<QueryResult>>> {
    let transaction = connection.transaction()?;
    let results = group
        .iter()
        .map(|job| {
            job.statements
                .iter()
                .map(|statement| run(&transaction, statement))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    transaction.commit()?;
    Ok(results)
}

fn run_job(connection: &mut rusqlite::Connection, job: &Job) -> rusqlite::Result<Vec<QueryResult>> {
    if !job.transactional {
        return job
            .statements
            .iter()
            .map(|statement| run(connection, statement))
            .collect();
    }
    let transaction = connection.transaction()?;
    let results = job
        .statements
        .iter()
        .map(|statement| run(&transaction, statement))
        .collect::<rusqlite::Result<Vec<_>>>()?;
    transaction.commit()?;
    Ok(results)
}

fn run(connection: &rusqlite::Connection, statement: &Statement) -> rusqlite::Result<QueryResult> {
    let mut prepared = connection.prepare_cached(&statement.sql)?;
    let readonly = prepared.readonly();
    let column_names = (0..prepared.column_count())
        .map(|index| prepared.column_name(index).map(str::to_owned))
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let values = statement.args.iter().cloned().map(to_sql_value);

    if column_names.is_empty() {
        let affected_rows = prepared.execute(params_from_iter(values))? as u64;
        return Ok(QueryResult {
            rows: Vec::new(),
            affected_rows,
            last_insert_id: write_row_id(connection, readonly, affected_rows),
        });
    }

    let mut rows = Vec::new();
    let mut query = prepared.query(params_from_iter(values))?;
    while let Some(row) = query.next()? {
        let values = column_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let value = row.get::<_, Value>(index)?;
                Ok((name.clone(), from_sql_value(value)))
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.push(Row::new(values));
    }
    drop(query);
    let affected_rows = if readonly { 0 } else { connection.changes() };
    Ok(QueryResult {
        rows,
        affected_rows,
        last_insert_id: write_row_id(connection, readonly, affected_rows),
    })
}

fn write_row_id(
    connection: &rusqlite::Connection,
    readonly: bool,
    affected_rows: u64,
) -> Option<i64> {
    (!readonly && affected_rows > 0).then(|| connection.last_insert_rowid())
}

fn to_sql_value(value: DbValue) -> Value {
    match value {
        DbValue::Null => Value::Null,
        DbValue::Integer(value) => Value::Integer(value),
        DbValue::Real(value) => Value::Real(value),
        DbValue::Text(value) => Value::Text(value),
        DbValue::Blob(value) => Value::Blob(value),
    }
}

fn from_sql_value(value: Value) -> DbValue {
    match value {
        Value::Null => DbValue::Null,
        Value::Integer(value) => DbValue::Integer(value),
        Value::Real(value) => DbValue::Real(value),
        Value::Text(value) => DbValue::Text(value),
        Value::Blob(value) => DbValue::Blob(value),
    }
}

fn database_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::Database(error.to_string())
}
