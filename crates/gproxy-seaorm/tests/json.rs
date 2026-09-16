#![cfg(not(target_arch = "wasm32"))]

use gproxy_seaorm::json_array_contains_text;
use sea_orm::{
    ConnectionTrait, Database, DbBackend,
    sea_query::{Alias, Expr, Query},
};
use serde_json::json;

#[tokio::test]
async fn json_string_membership_is_exact_and_top_level() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let cases = [
        (Some(json!(["a", "b"])), "a", true),
        (Some(json!(["a"])), "A", false),
        (Some(json!(["a "])), "a", false),
        (Some(json!([])), "a", false),
        (None, "a", false),
        (Some(json!(null)), "a", false),
        (Some(json!("a")), "a", false),
        (Some(json!({"a": "a"})), "a", false),
        (Some(json!([["a"]])), "a", false),
        (Some(json!([1, true, null])), "1", false),
        (Some(json!(["a%_\"'\\中"])), "a%_\"'\\中", true),
        (Some(json!(["a"])), "a' OR 1=1 --", false),
    ];
    for (array, value, expected) in cases {
        let expression =
            json_array_contains_text(DbBackend::Sqlite, Expr::val(array), Expr::val(value))
                .unwrap();
        let query = Query::select()
            .expr_as(expression, Alias::new("matches"))
            .to_owned();
        let row = db
            .query_one_raw(DbBackend::Sqlite.build(&query))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            row.try_get::<bool>("", "matches").unwrap(),
            expected,
            "{value}"
        );
    }
}

#[test]
fn dialect_expressions_keep_values_bound() {
    for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
        let expression = json_array_contains_text(
            backend,
            Expr::col(Alias::new("policy")),
            Expr::val("untrusted'client"),
        )
        .unwrap();
        let statement = backend.build(&Query::select().expr(expression).to_owned());
        assert!(!statement.sql.contains("untrusted"));
        assert_eq!(statement.values.unwrap().0, vec!["untrusted'client".into()]);
        match backend {
            DbBackend::Sqlite => assert!(statement.sql.contains("json_each")),
            DbBackend::Postgres => assert!(statement.sql.contains("jsonb_array_elements")),
            DbBackend::MySql => assert!(statement.sql.contains("JSON_TABLE")),
            _ => unreachable!(),
        }
    }
}
