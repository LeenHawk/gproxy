use super::DbPersistence;
use crate::store::persistence::traits::RoutingPersistence;
use sea_orm::{ConnectionTrait, Database};

#[tokio::test]
async fn adds_only_supported_missing_audio_cells() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pre-audio-routing.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
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
         (1, 'openai', 'openai', NULL, '{}', 'round_robin', NULL, NULL, 1, 0, 0), \
         (2, 'openrouter', 'openrouter', NULL, '{}', 'round_robin', NULL, NULL, 1, 0, 0), \
         (3, 'custom', 'custom', NULL, '{}', 'round_robin', NULL, NULL, 1, 0, 0), \
         (4, 'other', 'claudeapi', NULL, '{}', 'round_robin', NULL, NULL, 1, 0, 0)",
    )
    .await
    .expect("legacy providers");
    conn.execute_unprepared(
        "INSERT INTO routing_rules \
         (id, provider_id, operation, kind, implementation, dest_operation, dest_kind, \
          sort_order, enabled, created_at, updated_at) VALUES \
         (10, 3, 'create_speech', 'open_ai', 'unsupported', NULL, NULL, 7, 0, 8, 9)",
    )
    .await
    .expect("user-owned rule");
    conn.execute_unprepared("DELETE FROM schema_migrations")
        .await
        .expect("clear stamp");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (23, 0)")
        .await
        .expect("version 23");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let operations = |provider_id| {
        let db = &db;
        async move {
            db.list_routing_rules(provider_id)
                .await
                .expect("routing rules")
                .into_iter()
                .map(|rule| (rule.operation, rule.implementation, rule.enabled))
                .collect::<Vec<_>>()
        }
    };

    let openai = operations(1).await;
    assert_eq!(openai.len(), 3);
    assert!(
        openai
            .iter()
            .all(|(_, implementation, enabled)| implementation == "passthrough" && *enabled)
    );

    let openrouter = operations(2).await;
    assert_eq!(openrouter.len(), 2);
    assert!(
        openrouter
            .iter()
            .all(|(operation, _, _)| operation != "create_translation")
    );

    let custom = operations(3).await;
    assert_eq!(custom.len(), 3);
    assert!(custom.contains(&("create_speech".into(), "unsupported".into(), false)));
    assert!(operations(4).await.is_empty());
}
