#![cfg(not(target_arch = "wasm32"))]

use gproxy_seaorm::{
    BatchConnectionTrait, BatchQuery, BatchResult, BatchStatement, D1Type, Projection,
};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};

fn sql(sql: &str) -> Statement {
    Statement::from_string(DbBackend::Sqlite, sql)
}

fn query(sql_text: &str, alias: &str, kind: D1Type) -> BatchQuery {
    BatchQuery::new(
        sql(sql_text),
        Projection::new().column(alias, kind, false).unwrap(),
    )
}

async fn database() -> DatabaseConnection {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE items (id INTEGER PRIMARY KEY, value INTEGER NOT NULL, receipt TEXT)",
    )
    .await
    .unwrap();
    db
}

async fn values(db: &DatabaseConnection) -> Vec<i64> {
    db.query_batch(&[query(
        "SELECT value FROM items ORDER BY id",
        "value",
        D1Type::I64,
    )])
    .await
    .unwrap()
    .remove(0)
    .into_iter()
    .map(|r| r.try_get("", "value").unwrap())
    .collect()
}

#[tokio::test]
async fn a_single_query_uses_the_native_query_path_on_each_backend() {
    use sea_orm::{MockDatabase, Transaction, Value};
    use std::collections::BTreeMap;

    for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
        let row = BTreeMap::from([("value".to_owned(), Value::Int(Some(7)))]);
        let db = MockDatabase::new(backend)
            .append_query_results([vec![row]])
            .into_connection();
        let rows = db
            .query_rows(BatchQuery::new(
                Statement::from_string(backend, "SELECT 7 AS value"),
                Projection::new()
                    .column("value", D1Type::I32, false)
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(rows[0].try_get::<i32>("", "value").unwrap(), 7);
        assert_eq!(
            db.into_transaction_log(),
            [Transaction::one(Statement::from_string(
                backend,
                "SELECT 7 AS value"
            ))]
        );
    }
}

#[tokio::test]
async fn bulk_crud_returns_results_in_input_order() {
    let db = database().await;
    let result = db
        .atomic_batch(&[
            sql("INSERT INTO items VALUES (1,10,NULL)"),
            sql("INSERT INTO items VALUES (2,20,NULL)"),
        ])
        .await
        .unwrap();
    assert_eq!(
        result.iter().map(|r| r.rows_affected()).collect::<Vec<_>>(),
        [1, 1]
    );
    let rows = db
        .query_batch(&[
            query("SELECT value FROM items WHERE id=2", "value", D1Type::I64),
            query("SELECT count(*) AS total FROM items", "total", D1Type::I64),
            query("SELECT value FROM items WHERE id=999", "value", D1Type::I64),
            query("SELECT value FROM items WHERE id=1", "value", D1Type::I64),
        ])
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0][0].try_get::<i64>("", "value").unwrap(), 20);
    assert_eq!(rows[1][0].try_get::<i64>("", "total").unwrap(), 2);
    assert!(rows[2].is_empty());
    assert_eq!(rows[3][0].try_get::<i64>("", "value").unwrap(), 10);
    db.atomic_batch(&[
        sql("UPDATE items SET value=11 WHERE id=1"),
        sql("UPDATE items SET value=22 WHERE id=2"),
    ])
    .await
    .unwrap();
    assert_eq!(values(&db).await, [11, 22]);
    let deleted = db
        .atomic_batch(&[
            sql("DELETE FROM items WHERE id=2"),
            sql("DELETE FROM items WHERE id=1"),
        ])
        .await
        .unwrap();
    assert_eq!(
        deleted
            .iter()
            .map(|r| r.rows_affected())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    assert!(values(&db).await.is_empty());
}

#[tokio::test]
async fn mixed_batch_reads_its_own_writes_and_returning_rows() {
    let db = database().await;
    let results = db
        .batch(&[
            BatchStatement::Execute(sql("INSERT INTO items VALUES (1,10,NULL)")),
            BatchStatement::Query(query(
                "UPDATE items SET value=value+1 WHERE id=1 RETURNING value",
                "value",
                D1Type::I64,
            )),
            BatchStatement::Query(query(
                "SELECT value FROM items WHERE id=1",
                "value",
                D1Type::I64,
            )),
        ])
        .await
        .unwrap();
    assert!(matches!(&results[0], BatchResult::Executed(r) if r.rows_affected()==1));
    for result in &results[1..] {
        let BatchResult::Rows(rows) = result else {
            panic!("expected query result")
        };
        assert_eq!(rows[0].try_get::<i64>("", "value").unwrap(), 11);
    }
    assert_eq!(values(&db).await, [11]);
}

#[tokio::test]
async fn later_sql_failure_rolls_back_writes_even_after_query_steps() {
    let db = database().await;
    db.execute_raw(sql("INSERT INTO items VALUES (1,10,NULL)"))
        .await
        .unwrap();
    assert!(
        db.batch(&[
            BatchStatement::Execute(sql("UPDATE items SET value=90 WHERE id=1")),
            BatchStatement::Query(query("SELECT value FROM items", "value", D1Type::I64)),
            BatchStatement::Execute(sql("INSERT INTO items VALUES (1,99,NULL)")),
        ])
        .await
        .is_err()
    );
    assert_eq!(values(&db).await, [10]);
}

#[tokio::test]
async fn zero_row_write_is_not_failure_and_dependent_writes_need_conditions() {
    let db = database().await;
    db.execute_raw(sql("INSERT INTO items VALUES (1,10,'winner')"))
        .await
        .unwrap();
    let result = db.atomic_batch(&[
        sql("UPDATE items SET receipt='loser' WHERE id=1 AND receipt IS NULL"),
        sql("INSERT INTO items SELECT 2,20,NULL WHERE EXISTS (SELECT 1 FROM items WHERE id=1 AND receipt='loser')"),
        sql("INSERT INTO items VALUES (3,30,NULL)"),
    ]).await.unwrap();
    assert_eq!(
        result.iter().map(|r| r.rows_affected()).collect::<Vec<_>>(),
        [0, 0, 1]
    );
    assert_eq!(values(&db).await, [10, 30]);
}

#[tokio::test]
async fn batch_preflight_rejects_later_invalid_steps_before_writing() {
    let db = database().await;
    assert!(
        db.atomic_batch(&[
            sql("INSERT INTO items VALUES (1,10,NULL)"),
            Statement::from_string(DbBackend::Postgres, "SELECT 1"),
        ])
        .await
        .is_err()
    );
    assert!(values(&db).await.is_empty());
    let positional = BatchQuery::new(
        sql("SELECT value FROM items"),
        Projection::new()
            .column("value", D1Type::I64, false)
            .unwrap()
            .by_index(),
    );
    assert!(
        db.batch(&[
            BatchStatement::Execute(sql("INSERT INTO items VALUES (1,10,NULL)")),
            BatchStatement::Query(positional),
        ])
        .await
        .is_err()
    );
    assert!(values(&db).await.is_empty());
}

#[tokio::test]
async fn empty_batches_do_not_require_an_open_connection() {
    let db = DatabaseConnection::default();
    assert!(db.batch(&[]).await.unwrap().is_empty());
    assert!(db.atomic_batch(&[]).await.unwrap().is_empty());
    assert!(db.query_batch(&[]).await.unwrap().is_empty());
}
