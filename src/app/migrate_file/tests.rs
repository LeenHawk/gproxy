use serde_json::json;

use super::*;
use crate::crypto::NoopCipher;

fn table(rows: serde_json::Value) -> String {
    json!({ "next_id": 2, "rows": rows }).to_string()
}

#[tokio::test]
async fn migrates_minimal_file_backend_once() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let key = "legacy-key-that-is-long-enough-for-import";
    let fixtures = [
        (
            "orgs.json",
            json!([{"id":1,"name":"legacy","enabled":true}]),
        ),
        (
            "users.json",
            json!([{"id":1,"name":"admin","org_id":1,"enabled":true,"is_admin":true}]),
        ),
        (
            "user_keys.json",
            json!([{"id":1,"user_id":1,"api_key_ciphertext":key,"enabled":true}]),
        ),
        (
            "providers.json",
            json!([{"id":1,"name":"legacy-openai","channel":"openai","settings_json":{},"credential_strategy":"round_robin","enabled":true}]),
        ),
        (
            "credentials.json",
            json!([{"id":1,"provider_id":1,"kind":"api_key","secret_json":{"api_key":"secret"},"weight":100,"enabled":true}]),
        ),
        (
            "routes.json",
            json!([{"id":1,"name":"gpt-test","strategy":"failover","enabled":true}]),
        ),
        (
            "route_members.json",
            json!([{"id":1,"route_id":1,"provider_id":1,"upstream_model_id":"gpt-test","weight":100,"tier":0,"enabled":true}]),
        ),
    ];
    for (name, rows) in fixtures {
        std::fs::write(root.join(name), table(rows)).unwrap();
    }
    let db_path = root.join("target.db");
    let dsn = format!("sqlite://{}?mode=rwc", db_path.display());
    let channels = ChannelRegistry::with_builtin_and_linked().unwrap();

    let report = maybe_migrate_on_boot(root, &dsn, &NoopCipher, &channels)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.total, 7);
    for name in [
        "orgs.json",
        "users.json",
        "user_keys.json",
        "providers.json",
        "credentials.json",
        "routes.json",
        "route_members.json",
    ] {
        assert!(!root.join(name).exists());
        assert!(root.join(format!("{name}.filebak")).exists());
    }

    let db = DbPersistence::connect(&dsn).await.unwrap();
    assert_eq!(db.list_orgs().await.unwrap().len(), 1);
    assert_eq!(db.list_users().await.unwrap().len(), 1);
    assert_eq!(db.list_providers().await.unwrap().len(), 1);
    assert_eq!(db.list_routes().await.unwrap().len(), 1);
    assert_eq!(db.list_route_members(1).await.unwrap().len(), 1);
    let stored_key = db.list_user_keys(1).await.unwrap().remove(0);
    assert_eq!(stored_key.api_key_ciphertext, key);
    let credential = db.list_credentials(1).await.unwrap().remove(0);
    assert_eq!(
        NoopCipher.open(&credential.secret_json).unwrap(),
        json!({"api_key":"secret"})
    );
    db.close().await.unwrap();

    assert!(
        maybe_migrate_on_boot(root, &dsn, &NoopCipher, &channels)
            .await
            .unwrap()
            .is_none()
    );
}
