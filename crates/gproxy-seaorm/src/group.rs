//! Group commit for a native SQLite store.
//!
//! SQLite has one writer, and a native store talks to it through a pool of
//! one connection. Every [`BatchConnectionTrait::batch`] is a transaction of
//! its own, so under load each request's handful of small writes queue one by
//! one behind the others, and the store settles at the rate it can open and
//! commit transactions rather than the rate it can write rows.
//!
//! [`install`] makes a SQLite connection's reads and write batches queue. A writer —
//! started by the first batch that finds none running, gone once the queue is
//! empty — runs as many as are waiting — up to
//! [`MAX_GROUP`] — inside one transaction. If one of them fails, the group is
//! rolled back and each batch is run again on its own, so a failing batch is
//! rolled back alone, exactly as it would have been in a transaction of its
//! own, and the rest commit. A caller hears back only once the transaction
//! holding its batch has committed, so a returned batch is as durable as it
//! ever was.
//!
//! Nothing waits to fill a group. The writer takes whatever queued while the
//! previous transaction ran, so a lone request pays one transaction and a busy
//! store pays one per group.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr,
    sqlx::sqlite::{SqliteBatchStatement, SqlitePool},
};
use tokio::sync::oneshot;

use crate::{BatchResult, BatchStatement};

mod sqlite;

/// The most batches one transaction carries.
pub const MAX_GROUP: usize = 256;

type Reply = oneshot::Sender<Result<Vec<BatchResult>, DbErr>>;

struct Job {
    /// Encoded before enqueueing; moved into the worker command without cloning.
    statements: Vec<SqliteBatchStatement>,
    /// Keep originals for the rare rollback/retry path. The caller also retains
    /// this allocation so normal completion frees payloads outside the drainer.
    replay: Arc<[SqliteBatchStatement]>,
    /// Ordinary queries need a transaction only when combined with other jobs.
    transactional: bool,
    reply: Reply,
}

impl Job {
    fn new(statements: Vec<BatchStatement>, transactional: bool, reply: Reply) -> Self {
        let replay: Arc<[SqliteBatchStatement]> = sqlite::encode(statements).into();
        Self {
            statements: replay.to_vec(),
            replay,
            transactional,
            reply,
        }
    }
}

/// One pool's queue, and whether a writer is draining it.
///
/// There is no resident writer: the batch that finds nobody draining starts
/// one, and it stops as soon as the queue is empty. A task that lived for the
/// whole process would hold a handle to the pool and keep it open after its
/// owner closed it; this one holds it only while there is work.
#[derive(Default)]
pub(crate) struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    draining: AtomicBool,
}

type Queues = Mutex<Vec<(usize, Arc<Queue>)>>;

/// The queues installed in this process, by pool. A pool is identified by its
/// connect options, which it holds behind an `Arc` for its whole life.
fn queues() -> &'static Queues {
    static QUEUES: OnceLock<Queues> = OnceLock::new();
    QUEUES.get_or_init(|| Mutex::new(Vec::new()))
}

fn identity(pool: &SqlitePool) -> usize {
    Arc::as_ptr(&pool.connect_options()) as usize
}

/// `db`'s SQLite pool. `get_sqlite_connection_pool` panics on anything else,
/// and a mock connection can report the SQLite backend without having one.
fn sqlite_pool(db: &DatabaseConnection) -> Option<&SqlitePool> {
    (db.get_database_backend() == DbBackend::Sqlite && !db.is_mock_connection())
        .then(|| db.get_sqlite_connection_pool())
}

/// Commit `db`'s write batches in groups from now on. A no-op for anything but
/// a SQLite pool, and for a pool that already groups.
pub fn install(db: &DatabaseConnection) {
    let Some(pool) = sqlite_pool(db) else {
        return;
    };
    let key = identity(pool);
    let mut queues = queues().lock().unwrap_or_else(|poison| poison.into_inner());
    if !queues.iter().any(|(existing, _)| *existing == key) {
        queues.push((key, Arc::default()));
    }
}

/// The queue for `db`, if it groups.
pub(crate) fn queue(db: &DatabaseConnection) -> Option<Arc<Queue>> {
    let key = identity(sqlite_pool(db)?);
    queues()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .find(|(existing, _)| *existing == key)
        .map(|(_, queue)| queue.clone())
}

/// Queue `statements` and wait for the transaction that ran them to commit.
pub(crate) async fn submit(
    db: &DatabaseConnection,
    queue: Arc<Queue>,
    statements: Vec<BatchStatement>,
    transactional: bool,
) -> Result<Vec<BatchResult>, DbErr> {
    let (reply, answer) = oneshot::channel();
    let job = Job::new(statements, transactional, reply);
    let retained = job.replay.clone();
    queue
        .jobs
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push_back(job);
    if !queue.draining.swap(true, Ordering::AcqRel) {
        // A task, not this future: a caller that gives up waiting must not
        // strand the batches queued behind its own.
        tokio::spawn(drain(db.clone(), queue));
    }
    let result = answer
        .await
        .map_err(|_| DbErr::Custom("the store's writer dropped a batch".into()))?;
    drop(retained);
    result
}

/// Run groups until the queue is empty, then stand down.
async fn drain(db: DatabaseConnection, queue: Arc<Queue>) {
    loop {
        let jobs: Vec<Job> = {
            let mut waiting = queue
                .jobs
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let take = waiting.len().min(MAX_GROUP);
            waiting.drain(..take).collect()
        };
        if jobs.is_empty() {
            queue.draining.store(false, Ordering::Release);
            // A batch queued between the last take and the store above found
            // `draining` still set and started nobody; take it over, unless a
            // new drain already has.
            let empty = queue
                .jobs
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .is_empty();
            if empty || queue.draining.swap(true, Ordering::AcqRel) {
                return;
            }
            continue;
        }
        run_group(&db, jobs).await;
    }
}

/// One transaction for every job in `jobs`.
///
/// Optimistic: the jobs run back to back with no savepoints between them,
/// because a savepoint and its release are two more round trips per job and a
/// failing batch is rare. When one does fail, the whole group is rolled back
/// and every job runs again in a transaction of its own, so each hears exactly
/// the outcome it would have had alone.
async fn run_group(db: &DatabaseConnection, mut jobs: Vec<Job>) {
    let results = sqlite::run_jobs(db.get_sqlite_connection_pool(), &mut jobs).await;
    for (job, result) in jobs.into_iter().zip(results) {
        let Job { replay, reply, .. } = job;
        // Release the drainer's payload reference before waking the caller,
        // so the normal final drop happens on that caller's task.
        drop(replay);
        let _ = reply.send(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BatchConnectionTrait;
    use sea_orm::{ConnectOptions, Database, Statement};

    fn insert(id: i64) -> BatchStatement {
        BatchStatement::Execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO t (id) VALUES (?)",
            [id.into()],
        ))
    }

    #[tokio::test]
    async fn a_failed_query_does_not_lose_neighbouring_writes_or_row_sets() {
        use crate::{BatchQuery, Projection};

        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        db.execute_unprepared("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        let query = |sql: &str| {
            BatchStatement::Query(BatchQuery::new(
                Statement::from_string(DbBackend::Sqlite, sql),
                Projection::new(),
            ))
        };
        let steps = [
            (vec![insert(1)], true),
            (vec![query("SELECT id FROM missing")], false),
            (vec![insert(2)], true),
            (vec![query("SELECT id FROM t ORDER BY id")], false),
        ];
        let mut replies = Vec::new();
        let jobs = steps
            .into_iter()
            .map(|(statements, transactional)| {
                let (reply, receiver) = oneshot::channel();
                replies.push(receiver);
                Job::new(statements, transactional, reply)
            })
            .collect();
        run_group(&db, jobs).await;
        let mut replies = replies.into_iter();
        assert!(replies.next().unwrap().await.unwrap().is_ok());
        assert!(matches!(
            replies.next().unwrap().await.unwrap(),
            Err(DbErr::Query(_))
        ));
        assert!(replies.next().unwrap().await.unwrap().is_ok());
        let results = replies.next().unwrap().await.unwrap().unwrap();
        let BatchResult::Rows(rows) = &results[0] else {
            panic!("query rows")
        };
        assert_eq!(
            rows.iter()
                .map(|row| row.try_get::<i64>("", "id").unwrap())
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failing_batch_in_a_group_fails_alone() {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options
            .min_connections(1)
            .max_connections(1)
            .sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        db.execute_unprepared("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        db.execute_unprepared("INSERT INTO t (id) VALUES (0)")
            .await
            .unwrap();
        install(&db);

        // Enough at once that they queue behind each other and share groups.
        let mut tasks = Vec::new();
        for id in 0..200_i64 {
            let db = db.clone();
            tasks.push(tokio::spawn(async move {
                // Id 0 exists already: that one batch fails, and only it.
                db.batch(&[insert(id)]).await.map(|_| id)
            }));
        }
        let mut failed = Vec::new();
        for task in tasks {
            if let Err(id) = task.await.unwrap().map_err(|_| ()) {
                failed.push(id);
            }
        }
        assert_eq!(failed.len(), 1, "only the duplicate fails");
        let rows = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS n FROM t",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rows.try_get::<i64>("", "n").unwrap(), 200);
    }
}
