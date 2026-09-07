use serde_json::json;
use tokio_rusqlite::rusqlite::Connection;

use crate::{App, V2ImportOptions};

#[tokio::test]
async fn migration_preserves_usage_after_source_entities_were_deleted() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", super::setup::random_key());
    super::setup::v2_database(
        source.path(),
        &api_key,
        &api_key,
        &json!({"api_key":super::setup::random_key()}),
        true,
    );
    let mut connection = Connection::open(source.path().join("gproxy.db")).unwrap();
    connection
        .execute(
            "UPDATE usages SET provider_id=99,credential_id=98,org_id=97,team_id=96,user_id=95,user_key_id=94",
            [],
        )
        .unwrap();
    let transaction = connection.transaction().unwrap();
    for id in 2..=1_001 {
        transaction
            .execute(
                "INSERT INTO usages SELECT ?1,?2,at,route_name,provider_id,credential_id,org_id,team_id,user_id,user_key_id,thread_id,operation,kind,model,input_tokens,output_tokens,image_output_tokens,cache_read_tokens,cache_creation_5m_tokens,cache_creation_30m_tokens,cache_creation_1h_tokens,metrics_json,cost,latency_ms,usage_source,ended FROM usages WHERE id=1",
                (id, format!("v2-request-{id}")),
            )
            .unwrap();
    }
    transaction.commit().unwrap();
    drop(connection);

    let config = super::test_config(target.path(), crate::MasterKeyConfig::new(None));
    let report = crate::migrate_from_v2(
        &config,
        V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: None,
            apply: true,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.applied && report.issues.is_empty(), "{report}");
    assert_eq!(count(&report, "usage_provider_tombstones"), (1, 1));
    assert_eq!(count(&report, "usage_credential_tombstones"), (1, 1));

    let app = App::start(config).await.unwrap();
    let snapshot = app.inner.host.services.control.current();
    let provider = snapshot
        .providers
        .iter()
        .find(|value| value.name == "v2-deleted-provider-99")
        .unwrap();
    assert!(!provider.enabled);
    let credential = snapshot
        .credentials
        .iter()
        .find(|value| value.provider_id == provider.id)
        .unwrap();
    assert!(!credential.enabled);

    let usage = app
        .inner
        .host
        .services
        .store
        .usage_by_request("v2-request")
        .await
        .unwrap()
        .unwrap()
        .usage;
    assert_eq!(usage.provider_id, provider.id);
    assert_eq!(usage.credential_id, credential.id);
    assert_eq!(usage.organization_id, None);
    assert_eq!(usage.team_id, None);
    assert_eq!(usage.user_id, None);
    assert_eq!(usage.user_key_id, None);
    assert_eq!(usage.dimensions["v2_deleted_provider_id"], 99);
    assert_eq!(usage.dimensions["v2_deleted_credential_id"], 98);
    assert_eq!(usage.dimensions["v2_deleted_organization_id"], 97);
    assert_eq!(usage.dimensions["v2_deleted_team_id"], 96);
    assert_eq!(usage.dimensions["v2_deleted_user_id"], 95);
    assert_eq!(usage.dimensions["v2_deleted_user_key_id"], 94);
    assert_eq!(
        app.inner.host.services.store.usage_count().await.unwrap(),
        1_001
    );
}

fn count(report: &crate::V2ImportReport, entity: &str) -> (usize, usize) {
    let count = report
        .counts
        .iter()
        .find(|value| value.entity == entity)
        .unwrap();
    (count.found, count.imported)
}

#[tokio::test]
async fn migration_unwraps_v2_dimensional_metrics() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", super::setup::random_key());
    super::setup::v2_database(
        source.path(),
        &api_key,
        &api_key,
        &json!({"api_key":super::setup::random_key()}),
        true,
    );
    let connection = Connection::open(source.path().join("gproxy.db")).unwrap();
    connection.execute_batch("CREATE TABLE credential_statuses(id INTEGER PRIMARY KEY); INSERT INTO credential_statuses VALUES(1); CREATE TABLE rate_limits(id INTEGER PRIMARY KEY); INSERT INTO rate_limits VALUES(1); CREATE TABLE unknown_table(id INTEGER PRIMARY KEY); INSERT INTO unknown_table VALUES(1); INSERT INTO route_permissions VALUES(1,'user',1,'restricted/*',0,0);").unwrap();
    connection
        .execute(
            "UPDATE usages SET metrics_json=?1 WHERE id=1",
            [json!({
                "dimensions": {"operation": "stream_generate_content"},
                "quantities": {"audio_input_tokens": "0", "input_characters": "20276"},
                "cache_creation_5m_tokens": "7"
            })
            .to_string()],
        )
        .unwrap();
    drop(connection);

    let config = super::test_config(target.path(), crate::MasterKeyConfig::new(None));
    let report = crate::migrate_from_v2(
        &config,
        V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: None,
            apply: true,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.applied && report.issues.is_empty(), "{report}");
    assert!(report.to_string().contains("credential_statuses: 1 rows;"));
    for table in ["rate_limits", "unknown_table", "route_permissions"] {
        assert!(
            report.to_string().contains(&format!("{table}: 1 rows;")),
            "{report}"
        );
    }

    let app = App::start(config).await.unwrap();
    let record = app
        .usage_by_request("v2-request")
        .await
        .unwrap()
        .expect("migrated usage row");
    let metrics: std::collections::BTreeMap<String, rust_decimal::Decimal> =
        serde_json::from_value(record.usage.metrics.clone()).expect("v3 aggregates the metrics");
    assert_eq!(
        metrics["input_characters"],
        rust_decimal::Decimal::from(20_276)
    );
    assert_eq!(
        metrics["cache_creation_5m_tokens"],
        rust_decimal::Decimal::from(7)
    );
    assert_eq!(
        record.usage.dimensions["operation"],
        "stream_generate_content"
    );
}
