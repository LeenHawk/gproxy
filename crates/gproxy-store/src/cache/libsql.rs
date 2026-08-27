use std::time::Duration;

use gproxy_core::CacheBackend;
use gproxy_core::channel_api::BoxFuture;

use crate::Store;
use crate::backend::{DbValue, Statement};

use super::error;

type Error = gproxy_core::error::StoreError;

pub struct LibsqlCache {
    store: Store,
}

impl LibsqlCache {
    pub async fn connect(store: Store) -> Result<Self, Error> {
        store.backend().batch(vec![
            Statement::plain("CREATE TABLE IF NOT EXISTS gproxy_kv (k TEXT PRIMARY KEY, v BLOB NOT NULL, expires_ms INTEGER)"),
            Statement::plain("CREATE INDEX IF NOT EXISTS gproxy_kv_expires_ms_idx ON gproxy_kv(expires_ms)"),
        ]).await.map_err(|_| error("libSQL", "initialization"))?;
        Ok(Self { store })
    }

    async fn execute(
        &self,
        sql: &str,
        args: Vec<DbValue>,
        operation: &'static str,
    ) -> Result<crate::backend::QueryResult, Error> {
        self.store
            .backend()
            .execute(Statement::with_args(sql, args))
            .await
            .map_err(|_| error("libSQL", operation))
    }
}

impl CacheBackend for LibsqlCache {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, Error>> {
        Box::pin(async move {
            let result = self.execute("SELECT v FROM gproxy_kv WHERE k = ? AND (expires_ms IS NULL OR expires_ms > ?)", vec![text(key), integer(now_ms())], "get").await?;
            result
                .rows
                .into_iter()
                .next()
                .map(|row| {
                    row.blob("v")
                        .map(Vec::from)
                        .map_err(|_| error("libSQL", "get"))
                })
                .transpose()
        })
    }

    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            self.execute("INSERT INTO gproxy_kv(k,v,expires_ms) VALUES(?,?,?) ON CONFLICT(k) DO UPDATE SET v=excluded.v, expires_ms=excluded.expires_ms", vec![text(key), DbValue::Blob(value), expiry(ttl)], "set").await.map(|_| ())
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            self.execute(
                "DELETE FROM gproxy_kv WHERE k = ?",
                vec![text(key)],
                "delete",
            )
            .await
            .map(|_| ())
        })
    }

    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, Error>> {
        Box::pin(async move {
            let now = now_ms();
            let result = self.execute(
                "INSERT INTO gproxy_kv(k,v,expires_ms) VALUES(?,?,?) ON CONFLICT(k) DO UPDATE SET v=CASE WHEN gproxy_kv.expires_ms IS NOT NULL AND gproxy_kv.expires_ms<=? THEN excluded.v ELSE CAST(gproxy_kv.v AS INTEGER)+? END, expires_ms=CASE WHEN gproxy_kv.expires_ms IS NOT NULL AND gproxy_kv.expires_ms<=? THEN excluded.expires_ms ELSE gproxy_kv.expires_ms END RETURNING CAST(v AS INTEGER) AS value",
                vec![text(key), integer(by), expiry(ttl), integer(now), integer(by), integer(now)], "increment").await?;
            result
                .rows
                .first()
                .ok_or_else(|| error("libSQL", "increment"))?
                .i64("value")
                .map_err(|_| error("libSQL", "increment"))
        })
    }

    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected: Vec<u8>,
        state: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, Error>> {
        Box::pin(async move {
            let statements = vec![
                Statement::with_args(
                    "UPDATE gproxy_kv SET v=?, expires_ms=NULL WHERE k=? AND v=? AND (expires_ms IS NULL OR expires_ms>?)",
                    vec![
                        DbValue::Blob(state),
                        text(state_key),
                        DbValue::Blob(expected),
                        integer(now_ms()),
                    ],
                ),
                Statement::with_args(
                    "INSERT INTO gproxy_kv(k,v,expires_ms) SELECT ?,?,NULL WHERE changes()=1 ON CONFLICT(k) DO UPDATE SET v=CAST(gproxy_kv.v AS INTEGER)+excluded.v RETURNING CAST(v AS INTEGER) AS value",
                    vec![text(counter_key), integer(by)],
                ),
            ];
            let results = self
                .store
                .backend()
                .batch(statements)
                .await
                .map_err(|_| error("libSQL", "compare increment"))?;
            results
                .get(1)
                .and_then(|result| result.rows.first())
                .map(|row| {
                    row.i64("value")
                        .map_err(|_| error("libSQL", "compare increment"))
                })
                .transpose()
        })
    }

    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, Error>> {
        Box::pin(async move {
            let now = now_ms();
            let (sql, args) = match (expected, value) {
                (None, Some(value)) => (
                    "INSERT INTO gproxy_kv(k,v,expires_ms) VALUES(?,?,?) ON CONFLICT(k) DO UPDATE SET v=excluded.v, expires_ms=excluded.expires_ms WHERE gproxy_kv.expires_ms IS NOT NULL AND gproxy_kv.expires_ms<=? RETURNING 1 AS swapped",
                    vec![text(key), DbValue::Blob(value), expiry(ttl), integer(now)],
                ),
                (Some(expected), Some(value)) => (
                    "UPDATE gproxy_kv SET v=?,expires_ms=? WHERE k=? AND v=? AND (expires_ms IS NULL OR expires_ms>?) RETURNING 1 AS swapped",
                    vec![
                        DbValue::Blob(value),
                        expiry(ttl),
                        text(key),
                        DbValue::Blob(expected),
                        integer(now),
                    ],
                ),
                (Some(expected), None) => (
                    "DELETE FROM gproxy_kv WHERE k=? AND v=? AND (expires_ms IS NULL OR expires_ms>?) RETURNING 1 AS swapped",
                    vec![text(key), DbValue::Blob(expected), integer(now)],
                ),
                (None, None) => (
                    "DELETE FROM gproxy_kv WHERE k=? AND expires_ms IS NOT NULL AND expires_ms<=? RETURNING 1 AS swapped",
                    vec![text(key), integer(now)],
                ),
            };
            Ok(!self
                .execute(sql, args, "compare and swap")
                .await?
                .rows
                .is_empty())
        })
    }
}

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn expiry(ttl: Option<Duration>) -> DbValue {
    ttl.and_then(|ttl| now_ms().checked_add(i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX)))
        .map_or(DbValue::Null, DbValue::Integer)
}

fn text(value: &str) -> DbValue {
    DbValue::Text(value.into())
}
fn integer(value: i64) -> DbValue {
    DbValue::Integer(value)
}
