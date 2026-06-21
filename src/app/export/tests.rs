//! Export round-trip tests (§18). Two only: a full `import → export → import`
//! equality check with a real envelope cipher, and a keyless passthrough check.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use super::export_bundle;
use crate::app::import::import_bundle;
use crate::crypto::{NoopCipher, SecretCipher, cipher_from_master_key};
use crate::store::persistence::FilePersistence;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::Scope;

/// Bundle exercising every export path: orgs→teams, users→keys, the full scope
/// universe (org/team/user permissions + a rate limit + a quota), providers→
/// credentials (with a real secret), models, rule sets→rules, routes→members,
/// aliases, and instance settings.
const BUNDLE: &str = r#"{
  "schema_version": 1,
  "orgs": [{ "id": 1, "name": "acme", "enabled": true, "description": "top" }],
  "teams": [{ "id": 1, "org_id": 1, "name": "core", "enabled": true }],
  "users": [{ "id": 1, "name": "dev", "org_id": 1, "team_id": 1, "password": null, "enabled": true, "is_admin": true }],
  "user_keys": [{ "id": 1, "user_id": 1, "api_key": "sk-secret-key", "label": "primary", "enabled": true }],
  "route_permissions": [
    { "id": 1, "scope": "org", "scope_id": 1, "route_pattern": "*" },
    { "id": 2, "scope": "team", "scope_id": 1, "route_pattern": "main" },
    { "id": 3, "scope": "user", "scope_id": 1, "route_pattern": "gpt-*" }
  ],
  "rate_limits": [{ "id": 1, "scope": "user", "scope_id": 1, "route_pattern": "*", "rpm": 60, "rpd": null, "total_tokens": null }],
  "quotas": [{ "id": 1, "scope": "org", "scope_id": 1, "quota_total": "100.50", "cost_used": "1.25" }],
  "providers": [
    { "id": 1, "name": "openai-main", "channel": "openai", "label": null, "settings_json": { "base_url": "https://api.openai.com" }, "credential_strategy": "round_robin", "proxy_url": null, "tls_fingerprint": null, "enabled": true }
  ],
  "credentials": [
    { "id": 1, "provider_id": 1, "label": "k1", "secret_json": { "api_key": "sk-up-plaintext" }, "weight": 100, "enabled": true }
  ],
  "provider_models": [
    { "id": 1, "provider_id": 1, "model_id": "gpt-4.1", "display_name": null, "pricing_json": null, "variants_json": null, "enabled": true }
  ],
  "routes": [{ "id": 1, "name": "main", "strategy": "failover", "enabled": true, "description": null }],
  "route_members": [
    { "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-4.1", "weight": 100, "tier": 0, "enabled": true }
  ],
  "aliases": [{ "id": 1, "alias": "gpt", "route_id": 1 }],
  "rule_sets": [{ "id": 1, "name": "rs", "enabled": true, "description": null }],
  "rules": [
    { "id": 1, "rule_set_id": 1, "kind": "system_text", "config_json": { "text": "PRELUDE" }, "filter_model_pattern": null, "filter_operation_keys": null, "sort_order": 0, "enabled": true }
  ],
  "provider_rule_sets": [{ "id": 1, "provider_id": 1, "rule_set_id": 1, "sort_order": 0, "enabled": true }],
  "instance_settings": [
    { "id": 1, "instance_name": "node-a", "proxy": null, "spoof_emulation": null, "enable_usage": true, "enable_upstream_log": false, "enable_upstream_log_body": false, "enable_downstream_log": false, "enable_downstream_log_body": false, "disable_log_redaction": false, "enable_tokenizer_download": true, "update_channel": null }
  ]
}"#;

async fn file_store() -> (Arc<dyn PersistenceBackend>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db: Arc<dyn PersistenceBackend> = Arc::new(
        FilePersistence::open(dir.path().to_path_buf())
            .await
            .expect("file persistence"),
    );
    (db, dir)
}

fn envelope_cipher() -> Arc<dyn SecretCipher> {
    cipher_from_master_key(Some(&B64.encode([7u8; 32]))).expect("cipher")
}

/// Count every entity in a store so two stores can be compared for equality.
async fn counts(db: &dyn PersistenceBackend) -> Vec<usize> {
    let mut v = vec![
        db.list_orgs().await.unwrap().len(),
        db.list_users().await.unwrap().len(),
        db.list_providers().await.unwrap().len(),
        db.list_routes().await.unwrap().len(),
        db.list_aliases().await.unwrap().len(),
        db.list_rule_sets().await.unwrap().len(),
        db.list_instance_settings().await.unwrap().len(),
    ];
    // scoped counts: keys/perms/limits/quotas per identity, members/creds/etc.
    for org in db.list_orgs().await.unwrap() {
        v.push(db.list_teams(org.id).await.unwrap().len());
    }
    for user in db.list_users().await.unwrap() {
        v.push(db.list_user_keys(user.id).await.unwrap().len());
    }
    for p in db.list_providers().await.unwrap() {
        v.push(db.list_credentials(p.id).await.unwrap().len());
        v.push(db.list_provider_models(p.id).await.unwrap().len());
        v.push(db.list_provider_rule_sets(p.id).await.unwrap().len());
    }
    for r in db.list_routes().await.unwrap() {
        v.push(db.list_route_members(r.id).await.unwrap().len());
    }
    for set in db.list_rule_sets().await.unwrap() {
        v.push(db.list_rules(set.id).await.unwrap().len());
    }
    for (scope, id) in [(Scope::Org, 1), (Scope::Team, 1), (Scope::User, 1)] {
        v.push(db.list_route_permissions(scope, id).await.unwrap().len());
        v.push(db.list_rate_limits(scope, id).await.unwrap().len());
        v.push(usize::from(
            db.get_quota(scope, id).await.unwrap().is_some(),
        ));
    }
    v
}

#[tokio::test]
async fn export_roundtrips_import() {
    let cipher = envelope_cipher();

    // import the bundle into store A through a real envelope cipher.
    let (a, _da) = file_store().await;
    import_bundle(a.as_ref(), cipher.as_ref(), BUNDLE)
        .await
        .unwrap();

    // export A → re-serialize (must succeed) → re-import into a fresh store B.
    let bundle = export_bundle(a.as_ref(), cipher.as_ref()).await.unwrap();
    let json = serde_json::to_string(&bundle).unwrap();
    let (b, _db) = file_store().await;
    import_bundle(b.as_ref(), cipher.as_ref(), &json)
        .await
        .unwrap();

    // the two stores hold identical entity counts.
    assert_eq!(counts(a.as_ref()).await, counts(b.as_ref()).await);

    // a credential's secret_json in the re-imported store decrypts to the
    // original plaintext — the export carried the true plaintext, not the
    // at-rest ciphertext.
    let cred = &b.list_credentials(1).await.unwrap()[0];
    let plain = cipher.open(&cred.secret_json).unwrap();
    assert_eq!(plain, serde_json::json!({ "api_key": "sk-up-plaintext" }));

    // the exported bundle itself carried plaintext (not an envelope).
    assert_eq!(
        bundle.credentials[0].secret_json,
        serde_json::json!({ "api_key": "sk-up-plaintext" })
    );
    // and the bare user-key was recovered (not the sealed ciphertext).
    assert_eq!(bundle.user_keys[0].api_key, "sk-secret-key");
}

#[tokio::test]
async fn export_keyless_plaintext() {
    // keyless: import stores plaintext at rest; export emits it unchanged.
    let (a, _da) = file_store().await;
    import_bundle(a.as_ref(), &NoopCipher, BUNDLE)
        .await
        .unwrap();
    let bundle = export_bundle(a.as_ref(), &NoopCipher).await.unwrap();
    assert_eq!(
        bundle.credentials[0].secret_json,
        serde_json::json!({ "api_key": "sk-up-plaintext" })
    );
    assert_eq!(bundle.user_keys[0].api_key, "sk-secret-key");
}
