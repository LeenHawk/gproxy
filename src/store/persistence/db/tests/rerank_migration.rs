use super::DbPersistence;
use crate::store::persistence::traits::RoutingPersistence;
use sea_orm::{ConnectionTrait, Database};

#[tokio::test]
async fn adds_only_missing_custom_and_openrouter_rerank_cells() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pre-rerank-routing.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());

    // Create the current baseline, then pin it to the version immediately
    // before the rerank data migration and seed representative legacy rows.
    DbPersistence::connect(&dsn)
        .await
        .expect("create baseline")
        .close()
        .await
        .expect("close baseline");
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "INSERT INTO providers \
         (id, name, channel, label, settings_json, credential_strategy, proxy_url, \
          tls_fingerprint, enabled, created_at, updated_at) VALUES \
         (1, 'custom-missing', 'custom', NULL, '{}', 'round_robin', NULL, NULL, 1, 10, 11), \
         (2, 'openrouter-missing', 'openrouter', NULL, '{}', 'round_robin', NULL, NULL, 1, 10, 11), \
         (3, 'other-channel', 'openai', NULL, '{}', 'round_robin', NULL, NULL, 1, 10, 11), \
         (4, 'custom-edited', 'custom', NULL, '{}', 'round_robin', NULL, NULL, 1, 10, 11), \
         (5, 'openrouter-edited', 'openrouter', NULL, '{}', 'round_robin', NULL, NULL, 1, 10, 11)",
    )
    .await
    .expect("legacy providers");
    conn.execute_unprepared(
        "INSERT INTO routing_rules \
         (id, provider_id, operation, kind, implementation, dest_operation, dest_kind, \
          sort_order, enabled, created_at, updated_at) VALUES \
         (10, 1, 'list_models', 'open_ai', 'passthrough', NULL, NULL, 7, 1, 100, 101), \
         (11, 4, 'rerank', 'open_ai', 'transform_to', 'web_search', 'open_ai', 9, 0, 102, 103), \
         (12, 5, 'rerank', 'open_ai', 'unsupported', NULL, NULL, 4, 0, 104, 105)",
    )
    .await
    .expect("legacy routing rules");
    conn.execute_unprepared("DELETE FROM schema_migrations")
        .await
        .expect("clear baseline stamp");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (21, 0)")
        .await
        .expect("version 21");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");

    for (provider_id, expected_sort_order) in [(1, 8), (2, 0)] {
        let rules = db
            .list_routing_rules(provider_id)
            .await
            .expect("routing rules");
        let rerank = rules
            .iter()
            .filter(|rule| rule.operation == "rerank" && rule.kind == "open_ai")
            .collect::<Vec<_>>();
        assert_eq!(rerank.len(), 1);
        assert_eq!(rerank[0].implementation, "passthrough");
        assert_eq!(rerank[0].dest_operation, None);
        assert_eq!(rerank[0].dest_kind, None);
        assert_eq!(rerank[0].sort_order, expected_sort_order);
        assert!(rerank[0].enabled);
    }

    assert!(
        db.list_routing_rules(3)
            .await
            .expect("other channel rules")
            .is_empty()
    );

    let custom_edited = db.list_routing_rules(4).await.expect("custom edited rule");
    assert_eq!(custom_edited.len(), 1);
    assert_eq!(custom_edited[0].id, 11);
    assert_eq!(custom_edited[0].implementation, "transform_to");
    assert_eq!(
        custom_edited[0].dest_operation.as_deref(),
        Some("web_search")
    );
    assert_eq!(custom_edited[0].dest_kind.as_deref(), Some("open_ai"));
    assert_eq!(custom_edited[0].sort_order, 9);
    assert!(!custom_edited[0].enabled);
    assert_eq!(custom_edited[0].created_at, 102);
    assert_eq!(custom_edited[0].updated_at, 103);

    let openrouter_edited = db
        .list_routing_rules(5)
        .await
        .expect("openrouter edited rule");
    assert_eq!(openrouter_edited.len(), 1);
    assert_eq!(openrouter_edited[0].id, 12);
    assert_eq!(openrouter_edited[0].implementation, "unsupported");
    assert!(!openrouter_edited[0].enabled);
    assert_eq!(openrouter_edited[0].created_at, 104);
    assert_eq!(openrouter_edited[0].updated_at, 105);
}
