use serde_json::json;
use tokio_rusqlite::rusqlite::{Connection, params};

pub(super) fn seed(connection: &Connection) {
    connection.execute_batch(r#"
        CREATE TABLE downstream_requests(id INTEGER PRIMARY KEY,request_id TEXT,at INTEGER,method TEXT,path TEXT,query TEXT,status INTEGER,headers_json TEXT,body TEXT,response_body TEXT,created_at INTEGER,updated_at INTEGER);
        CREATE TABLE upstream_requests(id INTEGER PRIMARY KEY,request_id TEXT,at INTEGER,provider_id INTEGER,credential_id INTEGER,url TEXT,method TEXT,status INTEGER,latency_ms INTEGER,headers_json TEXT,body TEXT,response_body TEXT,created_at INTEGER,updated_at INTEGER);
        INSERT INTO downstream_requests VALUES(1,'v2-request',1,'POST','/v1/responses','stream=true',200,'{"content-type":"application/json"}','请求','响应',1,2);
        INSERT INTO upstream_requests VALUES(1,'v2-request',1,1,1,'https://upstream.invalid/v1/responses','POST',503,3,'{}','请求','失败',1,2);
        INSERT INTO upstream_requests VALUES(2,'v2-request',2,1,1,'https://upstream.invalid/v1/responses','POST',200,4,'{}','请求','成功',2,3);
        BEGIN;
    "#).unwrap();
    for id in 2..=130 {
        let request_id = format!("v2-log-{id}");
        connection.execute(
            "INSERT INTO downstream_requests VALUES(?1,?2,3,'POST','/v1/messages',NULL,0,NULL,?3,NULL,3,3)",
            params![id, request_id, "binary\0text"],
        ).unwrap();
        connection.execute(
            "INSERT INTO upstream_requests VALUES(?1,?2,3,99,98,'https://deleted.invalid','POST',0,0,NULL,NULL,NULL,3,3)",
            params![id + 1, request_id],
        ).unwrap();
    }
    connection.execute_batch("COMMIT").unwrap();
}

pub(super) async fn verify(app: &crate::AppHandle) {
    let store = &app.inner.host.services.store;
    let detail = store.log_detail("v2-request").await.unwrap().unwrap();
    let request = &detail.downstream;
    assert_eq!(request.input.at, 1);
    assert_eq!(request.input.path, "/v1/responses");
    assert_eq!(request.input.method, "POST");
    assert_eq!(request.input.query.as_deref(), Some("stream=true"));
    assert_eq!(
        request.input.request_headers,
        Some(json!({"content-type":"application/json"}))
    );
    assert_eq!(
        request.input.request_body.as_deref(),
        Some("请求".as_bytes())
    );
    assert_eq!(request.response_body.as_deref(), Some("响应".as_bytes()));
    assert_eq!(request.response_status, Some(200));
    assert_eq!(detail.upstream.len(), 2);
    assert_eq!(detail.upstream[0].input.response_status, Some(503));
    assert_eq!(
        detail.upstream[0].input.response_body.as_deref(),
        Some("失败".as_bytes())
    );
    let success = &detail.upstream[1].input;
    assert_eq!(success.response_status, Some(200));
    assert_eq!(success.response_body.as_deref(), Some("成功".as_bytes()));
    assert_eq!(
        success.upstream_url.as_deref(),
        Some("https://upstream.invalid/v1/responses")
    );
    assert_eq!(success.request_method.as_deref(), Some("POST"));
    assert_eq!(success.request_body.as_deref(), Some("请求".as_bytes()));
    let last = store.log_detail("v2-log-130").await.unwrap().unwrap();
    assert_eq!(last.downstream.response_status, None);
    assert_eq!(
        last.downstream.input.request_body.as_deref(),
        Some(b"binary\0text".as_slice())
    );
    assert_eq!(last.upstream.len(), 1);
    assert_eq!(last.upstream[0].input.response_status, None);
    let snapshot = store.control_snapshot().await.unwrap();
    let provider = snapshot
        .providers
        .iter()
        .find(|provider| provider.name == "v2-deleted-provider-99")
        .unwrap();
    assert!(!provider.enabled);
    let credential = snapshot
        .credentials
        .iter()
        .find(|credential| credential.provider_id == provider.id)
        .unwrap();
    assert!(!credential.enabled);
    assert_eq!(last.upstream[0].input.provider_id, Some(provider.id));
    assert_eq!(last.upstream[0].input.credential_id, Some(credential.id));
    let counts = store.entity_counts().await.unwrap();
    assert_eq!(
        counts
            .iter()
            .find(|(name, _)| *name == "request_logs")
            .unwrap()
            .1,
        130
    );
    assert_eq!(
        counts
            .iter()
            .find(|(name, _)| *name == "wire_logs")
            .unwrap()
            .1,
        131
    );
}

#[tokio::test]
async fn migration_imports_logs_without_usage_and_rejects_duplicate_request_ids() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let key = super::setup::random_key();
    super::setup::v2_database(
        source.path(),
        &key,
        &key,
        &json!({"api_key":super::setup::random_key()}),
        false,
    );
    let path = source.path().join("gproxy.db");
    let connection = Connection::open(&path).unwrap();
    seed(&connection);
    let config = super::test_config(target.path(), crate::MasterKeyConfig::new(None));
    for apply in [false, true, true] {
        let report = crate::migrate_from_v2(
            &config,
            crate::V2ImportOptions {
                path: path.clone(),
                source_master_key: None,
                apply,
                merge: false,
            },
        )
        .await
        .unwrap();
        assert!(!report.has_blockers(), "{report}");
        for (entity, expected) in [("downstream_requests", 130), ("upstream_requests", 131)] {
            let count = report
                .counts
                .iter()
                .find(|count| count.entity == entity)
                .unwrap();
            assert_eq!(count.found, expected);
            if report.applied {
                assert_eq!(count.imported, expected);
            }
        }
    }
    let app = crate::App::start(config.clone()).await.unwrap();
    verify(&app).await;
    assert_eq!(
        app.inner.host.services.store.usage_count().await.unwrap(),
        0
    );
    app.shutdown();
    connection.execute_batch("INSERT INTO downstream_requests SELECT 131,request_id,at,method,path,query,status,headers_json,body,response_body,created_at,updated_at FROM downstream_requests WHERE id=1").unwrap();
    let report = crate::migrate_from_v2(
        &config,
        crate::V2ImportOptions {
            path,
            source_master_key: None,
            apply: false,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.has_blockers());
    assert!(report.to_string().contains("duplicate request_id"));
}
