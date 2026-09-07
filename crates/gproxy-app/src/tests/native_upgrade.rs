use std::path::{Path, PathBuf};

use gproxy_core::ControlPlane as _;
use serde_json::json;
use tokio_rusqlite::rusqlite::Connection;

use crate::{App, MasterKeyConfig};

#[tokio::test]
async fn native_upgrade_preserves_wal_secrets_usage_and_restarts_without_reimport() {
    let directory = tempfile::tempdir().unwrap();
    let mut master = [0_u8; 32];
    getrandom::fill(&mut master).unwrap();
    let key = format!("sk-{}", super::setup::random_key());
    let stored_key = super::setup::v2_seal(&json!(key), master).to_string();
    super::setup::v2_database(
        directory.path(),
        &key,
        &stored_key,
        &super::setup::v2_seal(&json!({"api_key":super::setup::random_key()}), master),
        true,
    );
    let original = directory.path().join("gproxy.db");
    let connection = Connection::open(&original).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=WAL; UPDATE usages SET cost='23.45'; UPDATE users SET is_admin=0; INSERT INTO route_permissions VALUES(1,'user',1,'*',0,0);")
        .unwrap();
    let skipped = [
        "credential_statuses",
        "credential_model_statuses",
        "credential_usage_daily",
        "credential_quota_cycles",
        "credential_quota_cycle_models",
        "tokenizer_vocabs",
        "codex_task_bindings",
        "audit_logs",
        "schema_migrations",
        "rate_limits",
        "unknown_table",
    ];
    for table in skipped {
        connection
            .execute_batch(&format!(
                "CREATE TABLE {table}(id INTEGER PRIMARY KEY); INSERT INTO {table} VALUES(1);"
            ))
            .unwrap();
    }
    connection.execute_batch("CREATE TABLE usage_rollups(id INTEGER PRIMARY KEY, requests INTEGER, cost TEXT); INSERT INTO usage_rollups VALUES(1,999,'999.999'); CREATE TABLE unknown_empty(id INTEGER PRIMARY KEY);").unwrap();
    super::migrate_v2_logs::seed(&connection);
    connection.execute_batch("ALTER TABLE usages ADD COLUMN irrelevant TEXT; INSERT INTO users VALUES(2,'limited-user',1,NULL,NULL,1,0); INSERT INTO route_permissions VALUES(2,'user',2,'restricted/*',0,0),(3,'unknown',1,'*',0,0),(4,'user',999,'*',0,0);").unwrap();
    drop(connection);
    let config = super::test_config(directory.path(), MasterKeyConfig::new(Some(master)));
    let app = App::start(config.clone()).await.unwrap();
    super::migrate_v2_logs::verify(&app).await;
    assert_eq!(
        app.inner.host.services.store.usage_count().await.unwrap(),
        1
    );
    let usage = app
        .inner
        .host
        .services
        .store
        .usage_by_request("v2-request")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(usage.usage.cost.to_string(), "23.45");
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        format!("Bearer {key}").parse().unwrap(),
    );
    let identity = crate::host::authenticate_headers(&app.inner.host, &headers).unwrap();
    let control = &app.inner.host.services.control;
    let snapshot = control.current();
    let limited = snapshot
        .users
        .iter()
        .find(|user| user.name == "limited-user")
        .unwrap();
    let stored = app
        .inner
        .host
        .services
        .store
        .control_snapshot()
        .await
        .unwrap();
    assert!(
        !stored
            .permissions
            .iter()
            .any(|permission| permission.subject_kind == "user"
                && permission.subject_id == limited.id)
    );
    let plan = control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            Some(identity.user_key_id),
        )
        .unwrap();
    assert!(
        crate::host::authorize(
            &control.current(),
            &identity,
            Some(super::generation_operation()),
            &plan
        )
        .is_ok()
    );
    let archives = backups(directory.path());
    assert_eq!(archives.len(), 1);
    let report = std::fs::read_to_string(archives[0].join("report.txt")).unwrap();
    assert!(
        report.contains("route_permissions: 1 imported (4 found)"),
        "{report}"
    );
    assert!(report.contains("route_permissions: 3 rows;"), "{report}");
    assert!(
        report.contains("downstream_requests: 130 imported (130 found)"),
        "{report}"
    );
    assert!(
        report.contains("upstream_requests: 131 imported (131 found)"),
        "{report}"
    );
    for table in skipped.into_iter().chain(["usage_rollups"]) {
        assert!(report.contains(&format!("{table}: 1 rows;")), "{report}");
    }
    let trend = app
        .inner
        .host
        .services
        .store
        .usage_trend(0, 3_600)
        .await
        .unwrap();
    assert_eq!(trend.len(), 1);
    assert_eq!(trend[0].requests, 1);
    assert_eq!(trend[0].cost.to_string(), "23.45");
    let backup = Connection::open(archives[0].join("gproxy-v2.db")).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT cost FROM usage_rollups", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "999.999"
    );
    assert_eq!(
        backup
            .query_row("SELECT cost FROM usages", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "23.45"
    );
    assert_eq!(
        backup
            .query_row("SELECT api_key_ciphertext FROM user_keys", [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
        stored_key
    );
    app.shutdown();
    drop(app);
    let restarted = App::start(config).await.unwrap();
    assert_eq!(
        restarted
            .inner
            .host
            .services
            .store
            .usage_count()
            .await
            .unwrap(),
        1
    );
    assert_eq!(backups(directory.path()).len(), 1);
    restarted.shutdown();
}

#[tokio::test]
async fn native_upgrade_keeps_source_on_unrecoverable_keys() {
    let directory = tempfile::tempdir().unwrap();
    let key = format!("sk-{}", super::setup::random_key());
    let invalid =
        json!({"kek_id":"local","wrapped_dek":"bad","nonce":"bad","ciphertext":"bad"}).to_string();
    super::setup::v2_database(
        directory.path(),
        &key,
        &invalid,
        &json!({"api_key":super::setup::random_key()}),
        true,
    );
    let path = directory.path().join("gproxy.db");
    let config = super::test_config(directory.path(), MasterKeyConfig::new(None));
    let error = match App::start(config.clone()).await {
        Ok(_) => panic!("migration with unrecoverable keys succeeded"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("report.txt"), "{error}");
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM usages", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        connection
            .prepare("SELECT api_key_ciphertext FROM user_keys")
            .is_ok()
    );
    let attempts = backups(directory.path());
    assert_eq!(attempts.len(), 1);
    assert!(attempts[0].join("report.txt").is_file());
    assert!(attempts[0].join("gproxy-v2.db").is_file());
    assert!(App::start(config).await.is_err());
    assert_eq!(backups(directory.path()).len(), 1);
}

fn backups(path: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("gproxy-v2-backup-")
        })
        .collect()
}
