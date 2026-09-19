use crate::{
    BatchConnectionTrait, BatchQuery, BatchStatement, D1Type, LibsqlConnection, LibsqlFuture,
    LibsqlRequest, LibsqlResponse, LibsqlTransport, Projection,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Script {
    replies: Mutex<VecDeque<(u16, Value)>>,
    sent: Mutex<Vec<LibsqlRequest>>,
}
impl LibsqlTransport for Script {
    fn post<'a>(
        &'a self,
        request: LibsqlRequest,
    ) -> LibsqlFuture<'a, Result<LibsqlResponse, sea_orm::DbErr>> {
        self.sent.lock().unwrap().push(request);
        let (status, body) = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted reply");
        Box::pin(async move {
            Ok(LibsqlResponse {
                status,
                body: serde_json::to_vec(&body).unwrap(),
            })
        })
    }
}

fn ok(response: Value) -> Value {
    json!({"baton": null, "base_url": null, "results": [
        {"type": "ok", "response": response},
        {"type": "ok", "response": {"type": "close"}}
    ]})
}

fn connect(script: &Arc<Script>) -> LibsqlConnection {
    let transport: Arc<dyn LibsqlTransport> = script.clone();
    LibsqlConnection::new(transport, "libsql://db.turso.io/", Some("tok")).unwrap()
}

fn ready<F: std::future::Future>(future: F) -> F::Output {
    futures_executor::block_on(future)
}

#[test]
fn execute_posts_the_pipeline_with_typed_arguments_and_decodes_the_result() {
    let script = Arc::new(Script::default());
    script.replies.lock().unwrap().push_back((
        200,
        ok(json!({"type": "execute", "result": {"cols": [], "rows": [], "affected_row_count": 2, "last_insert_rowid": "41"}})),
    ));
    let db = connect(&script);
    let result = ready(db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE t SET a=?, b=?, c=?, d=? WHERE id=?",
        vec![
            7i64.into(),
            1.5f64.into(),
            "x".into(),
            vec![1u8, 2].into(),
            sea_orm::Value::String(None),
        ],
    )))
    .unwrap();
    assert_eq!(result.rows_affected(), 2);
    assert_eq!(result.last_insert_id(), 41);
    let sent = script.sent.lock().unwrap();
    assert_eq!(sent[0].url, "https://db.turso.io/v2/pipeline");
    assert!(
        sent[0]
            .headers
            .contains(&("authorization".into(), "Bearer tok".into()))
    );
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(body["requests"][1]["type"], "close");
    let stmt = &body["requests"][0]["stmt"];
    assert_eq!(stmt["want_rows"], false);
    assert_eq!(
        stmt["args"],
        json!([
            {"type": "integer", "value": "7"},
            {"type": "float", "value": 1.5},
            {"type": "text", "value": "x"},
            {"type": "blob", "base64": "AQI"},
            {"type": "null"}
        ])
    );
}

#[test]
fn queries_decode_hrana_rows_through_the_projection() {
    let script = Arc::new(Script::default());
    script.replies.lock().unwrap().push_back((
        200,
        ok(json!({"type": "execute", "result": {
            "cols": [{"name": "id"}, {"name": "big"}, {"name": "flag"}, {"name": "blob"}, {"name": "note"}],
            "rows": [[
                {"type": "integer", "value": "5"},
                {"type": "integer", "value": "9007199254740993"},
                {"type": "integer", "value": "1"},
                {"type": "blob", "base64": "AQID"},
                {"type": "null"}
            ]],
            "affected_row_count": 0, "last_insert_rowid": null}})),
    ));
    let projection = Projection::new()
        .column("id", D1Type::I32, false)
        .unwrap()
        .column("big", D1Type::I64, false)
        .unwrap()
        .column("flag", D1Type::Bool, false)
        .unwrap()
        .column("blob", D1Type::Bytes, false)
        .unwrap()
        .column("note", D1Type::Text, true)
        .unwrap();
    let db = connect(&script).with_projection(projection);
    let rows =
        ready(db.query_all_raw(Statement::from_string(DbBackend::Sqlite, "SELECT 1"))).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].try_get::<i32>("", "id").unwrap(), 5);
    assert_eq!(
        rows[0].try_get::<i64>("", "big").unwrap(),
        9007199254740993,
        "exact beyond the JS safe range"
    );
    assert!(rows[0].try_get::<bool>("", "flag").unwrap());
    assert_eq!(
        rows[0].try_get::<Vec<u8>>("", "blob").unwrap(),
        vec![1, 2, 3]
    );
    assert_eq!(rows[0].try_get::<Option<String>>("", "note").unwrap(), None);
    let body: Value = serde_json::from_slice(&script.sent.lock().unwrap()[0].body).unwrap();
    assert_eq!(body["requests"][0]["stmt"]["want_rows"], true);
}

#[test]
fn batches_are_bracketed_by_begin_commit_and_a_conditional_rollback() {
    let script = Arc::new(Script::default());
    let exec = |n: u64| json!({"cols": [], "rows": [], "affected_row_count": n, "last_insert_rowid": null});
    script.replies.lock().unwrap().push_back((
        200,
        ok(json!({"type": "batch", "result": {
            "step_results": [exec(0), exec(1), {"cols": [{"name": "n"}], "rows": [[{"type": "integer", "value": "3"}]], "affected_row_count": 0, "last_insert_rowid": null}, exec(0), null],
            "step_errors": [null, null, null, null, null]}})),
    ));
    let db = connect(&script);
    let results = ready(db.batch(&[
        BatchStatement::Execute(Statement::from_string(DbBackend::Sqlite, "DELETE FROM t")),
        BatchStatement::Query(BatchQuery::new(
            Statement::from_string(DbBackend::Sqlite, "SELECT count(*) AS n FROM t"),
            Projection::new().column("n", D1Type::I64, false).unwrap(),
        )),
    ]))
    .unwrap();
    assert_eq!(results.len(), 2);
    let crate::BatchResult::Executed(first) = &results[0] else {
        panic!("execute")
    };
    assert_eq!(first.rows_affected(), 1);
    let crate::BatchResult::Rows(rows) = &results[1] else {
        panic!("rows")
    };
    assert_eq!(rows[0].try_get::<i64>("", "n").unwrap(), 3);
    let body: Value = serde_json::from_slice(&script.sent.lock().unwrap()[0].body).unwrap();
    let steps = body["requests"][0]["batch"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 5);
    assert_eq!(steps[0]["stmt"]["sql"], "BEGIN");
    assert_eq!(steps[1]["condition"], json!({"type": "ok", "step": 0}));
    assert_eq!(steps[2]["condition"], json!({"type": "ok", "step": 1}));
    assert_eq!(steps[3]["stmt"]["sql"], "COMMIT");
    assert_eq!(steps[3]["condition"], json!({"type": "ok", "step": 2}));
    assert_eq!(steps[4]["stmt"]["sql"], "ROLLBACK");
    assert_eq!(
        steps[4]["condition"],
        json!({"type": "not", "cond": {"type": "ok", "step": 2}})
    );

    // A failing step: the server skipped the rest and rolled back.
    script.replies.lock().unwrap().push_back((
        200,
        ok(json!({"type": "batch", "result": {
            "step_results": [exec(0), null, null, null, exec(0)],
            "step_errors": [null, {"message": "UNIQUE constraint failed"}, null, null, null]}})),
    ));
    let error = ready(db.atomic_batch(&[
        Statement::from_string(DbBackend::Sqlite, "INSERT INTO t VALUES (1)"),
        Statement::from_string(DbBackend::Sqlite, "INSERT INTO t VALUES (2)"),
    ]))
    .expect_err("batch fails");
    assert!(
        error
            .to_string()
            .contains("statement 0 failed: UNIQUE constraint failed"),
        "{error}"
    );

    // HTTP-level rejection carries the server message.
    script
        .replies
        .lock()
        .unwrap()
        .push_back((401, json!({"error": "invalid token"})));
    let error = ready(db.execute_unprepared("SELECT 1")).expect_err("rejected");
    assert!(
        error.to_string().contains("HTTP 401: invalid token"),
        "{error}"
    );
    assert!(LibsqlConnection::new(script.clone(), "ftp://x", None).is_err());
}
