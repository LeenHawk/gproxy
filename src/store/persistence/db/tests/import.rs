//! Pinned-ID bundle import and update tests.

use super::*;
use crate::store::persistence::traits::{
    IdentityPersistence, ProviderPersistence, RoutingPersistence,
};

/// A fully-pinned config bundle (explicit ids on every record) covering all 18
/// config entities. Importing it into an EMPTY db must insert-with-id rather
/// than bail "X not found: 1" (the regression this fix targets).
const IMPORT_BUNDLE: &str = r#"{
  "schema_version": 1,
  "orgs": [{ "id": 1, "name": "acme", "enabled": true, "description": "top" }],
  "teams": [{ "id": 1, "org_id": 1, "name": "core", "enabled": true }],
  "users": [{ "id": 1, "name": "dev", "org_id": 1, "team_id": 1, "password": null, "enabled": true, "is_admin": true }],
  "user_keys": [{ "id": 1, "user_id": 1, "api_key": "sk-secret-key", "label": "primary", "enabled": true }],
  "route_permissions": [{ "id": 1, "scope": "org", "scope_id": 1, "route_pattern": "*" }],
  "rate_limits": [{ "id": 1, "scope": "user", "scope_id": 1, "route_pattern": "*", "rpm": 60, "rpd": null, "total_tokens": null }],
  "quotas": [{ "id": 1, "scope": "org", "scope_id": 1, "quota_total": "100.50", "cost_used": "1.25" }],
  "providers": [
    { "id": 1, "name": "openai-main", "channel": "openai", "label": null, "settings_json": { "base_url": "https://api.openai.com" }, "credential_strategy": "round_robin", "proxy_url": null, "tls_fingerprint": null, "enabled": true }
  ],
  "credentials": [
    { "id": 1, "provider_id": 1, "label": "k1", "secret_json": { "api_key": "sk-up-plaintext" }, "weight": 100, "enabled": true }
  ],
  "provider_models": [
    { "id": 1, "provider_id": 1, "model_id": "gpt-4.1", "display_name": null, "variants_json": null, "enabled": true }
  ],
  "routes": [{ "id": 1, "name": "main", "strategy": "failover", "enabled": true, "description": null }],
  "route_members": [
    { "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-4.1", "weight": 100, "tier": 0, "enabled": true }
  ],
  "aliases": [{ "id": 1, "provider": "*", "alias": "gpt", "target": "main", "sort_order": 0, "enabled": true }],
  "rule_sets": [{ "id": 1, "name": "rs", "enabled": true, "description": null }],
  "rules": [
    { "id": 1, "rule_set_id": 1, "kind": "system_text", "config_json": { "text": "PRELUDE" }, "filter_model_pattern": null, "filter_operation_keys": null, "sort_order": 0, "enabled": true }
  ],
  "provider_rule_sets": [{ "id": 1, "provider_id": 1, "rule_set_id": 1, "sort_order": 0, "enabled": true }],
  "instance_settings": [
    { "id": 1, "instance_name": "node-a", "proxy": null, "spoof_emulation": null, "enable_usage": true, "enable_upstream_log": false, "enable_upstream_log_body": false, "enable_downstream_log": false, "enable_downstream_log_body": false, "disable_log_redaction": false, "enable_tokenizer_download": true, "update_channel": null }
  ]
}"#;

#[tokio::test]
async fn import_seeds_empty_db() {
    use crate::app::import::import_bundle;
    use crate::crypto::NoopCipher;

    let db = mem().await;

    // First import into an empty store: each `Some(id)` row is missing, so the
    // upserts must insert-with-id (previously bailed "org not found: 1").
    import_bundle(&db, &NoopCipher, IMPORT_BUNDLE)
        .await
        .expect("seed empty db");
    assert_eq!(db.list_orgs().await.unwrap().len(), 1);
    assert_eq!(db.list_providers().await.unwrap().len(), 1);
    assert_eq!(db.list_users().await.unwrap().len(), 1);
    assert_eq!(db.list_rule_sets().await.unwrap().len(), 1);

    // Re-import the same pinned bundle: idempotent — updates in place, no dups.
    import_bundle(&db, &NoopCipher, IMPORT_BUNDLE)
        .await
        .expect("re-import idempotent");
    assert_eq!(db.list_orgs().await.unwrap().len(), 1);
    assert_eq!(db.list_providers().await.unwrap().len(), 1);
    assert_eq!(db.list_users().await.unwrap().len(), 1);
}

#[tokio::test]
async fn upsert_with_existing_id_updates_not_duplicates() {
    use crate::app::import::import_bundle;
    use crate::crypto::NoopCipher;

    let db = mem().await;
    import_bundle(&db, &NoopCipher, IMPORT_BUNDLE)
        .await
        .expect("seed");

    // Explicit-id update path: same id → mutate in place, count unchanged.
    let updated = db
        .upsert_org(OrgInput {
            id: Some(1),
            name: "acme-renamed".to_owned(),
            enabled: false,
            description: None,
        })
        .await
        .expect("update existing org");
    assert_eq!(updated.id, 1);
    assert_eq!(updated.name, "acme-renamed");
    assert!(!updated.enabled);
    assert_eq!(db.list_orgs().await.unwrap().len(), 1);
}
