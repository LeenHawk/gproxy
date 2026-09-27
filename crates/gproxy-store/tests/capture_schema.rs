#![cfg(not(target_arch = "wasm32"))]

use gproxy_store::{
    Store,
    entity::usage::{
        capture_link, downstream_event, downstream_record, upstream_event, upstream_record,
        usage_record,
    },
};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, EntityTrait, Set};
use serde_json::json;

#[tokio::test]
async fn split_captures_support_all_cardinalities_and_independent_usage_retention() {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    let store = Store::new(db);
    store.sync().await.unwrap();
    let tables = store
        .connection()
        .query_all_raw(sea_orm::Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'table'".to_owned(),
        ))
        .await
        .unwrap();
    let names: Vec<String> = tables
        .iter()
        .map(|r| r.try_get("", "name").unwrap())
        .collect();
    for table in [
        "upstream_records",
        "downstream_records",
        "usage_records",
        "capture_links",
        "upstream_events",
        "downstream_events",
    ] {
        assert!(names.iter().any(|name| name == table));
    }
    assert!(
        !names
            .iter()
            .any(|name| name == "capture_records" || name == "capture_events")
    );
    let columns = store
        .connection()
        .query_all_raw(sea_orm::Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA table_info(usage_records)".to_owned(),
        ))
        .await
        .unwrap();
    assert!(!columns.iter().any(|r| matches!(
        r.try_get::<String>("", "name").unwrap().as_str(),
        "side" | "downstream_request_id"
    )));

    store
        .downstream_records()
        .insert_many(
            ["d1", "d2", "d-alone"]
                .into_iter()
                .map(|id| downstream_record::ActiveModel {
                    id: Set(id.into()),
                    kind: Set(downstream_record::CaptureKind::Http),
                    started_at_ms: Set(1),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
    store
        .upstream_records()
        .insert_many(
            ["u1", "u2", "u-alone"]
                .into_iter()
                .map(|id| upstream_record::ActiveModel {
                    id: Set(id.into()),
                    kind: Set(upstream_record::CaptureKind::Http),
                    started_at_ms: Set(1),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
    // d1 -> u1,u2 (fan-out), d2 -> u1 (shared upstream), independent rows on both sides.
    let links: Vec<_> = [("d1", "u1"), ("d1", "u2"), ("d2", "u1")]
        .into_iter()
        .map(|(d, u)| capture_link::ActiveModel {
            downstream_id: Set(d.into()),
            upstream_id: Set(u.into()),
        })
        .collect();
    store
        .capture_links()
        .insert_many(links.clone())
        .await
        .unwrap();
    assert!(
        store
            .capture_links()
            .insert_many(vec![links[0].clone()])
            .await
            .is_err(),
        "an edge is unique"
    );
    store
        .usage_records()
        .insert_many(
            ["u1", "u2", "u-alone", "usage-without-log"]
                .into_iter()
                .map(|id| usage_record::ActiveModel {
                    request_id: Set(id.into()),
                    model: Set("m".into()),
                    operation: Set("generate_content".into()),
                    started_at_ms: Set(1),
                    input_tokens: Set(Some(7)),
                    cost: Set(Some("0.1".parse().unwrap())),
                    metrics: Set(json!({})),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
    // Owner foreign keys still cascade; the two event tables cannot attach to the other side.
    store
        .upstream_events()
        .insert_many(vec![upstream_event::ActiveModel {
            capture_id: Set("u1".into()),
            sequence: Set(0),
            direction: Set(upstream_event::CaptureDirection::Response),
            kind: Set(upstream_event::CaptureEventKind::Bytes),
            payload: Set(vec![1]),
            observed_at_ms: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
    let event = downstream_event::ActiveModel {
        capture_id: Set("u1".into()),
        sequence: Set(0),
        direction: Set(downstream_event::CaptureDirection::Response),
        kind: Set(downstream_event::CaptureEventKind::Bytes),
        payload: Set(vec![2]),
        observed_at_ms: Set(1),
        ..Default::default()
    };
    assert!(
        store
            .downstream_events()
            .insert_many(vec![event])
            .await
            .is_err()
    );
    store
        .downstream_records()
        .delete_many(&["d1".into()])
        .await
        .unwrap();
    store
        .upstream_records()
        .delete_many(&["u1".into()])
        .await
        .unwrap();
    assert!(
        store
            .upstream_events()
            .query(upstream_event::Entity::find())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .capture_links()
            .get_many(&[("d2".into(), "u1".into())])
            .await
            .unwrap()[0]
            .is_some()
    );
    let usage = store
        .usage_records()
        .query(usage_record::Entity::find())
        .await
        .unwrap();
    assert_eq!(usage.len(), 4);
    assert_eq!(usage.iter().filter_map(|r| r.input_tokens).sum::<i64>(), 28);
}
