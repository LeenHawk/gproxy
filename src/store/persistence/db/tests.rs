//! Tests for [`DbPersistence`].

use super::DbPersistence;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::*;
use serde_json::json;

async fn mem() -> DbPersistence {
    DbPersistence::connect("sqlite::memory:")
        .await
        .expect("connect")
}

#[tokio::test]
async fn sqlite_memory_connect_and_health() {
    mem().await.health().await.expect("health");
}

#[tokio::test]
async fn connect_stamps_latest_and_leaves_nothing_pending() {
    use crate::store::persistence::migrations::latest_version;
    use sea_orm::{ConnectionTrait, Statement};

    let db = mem().await;
    let backend = db.conn.get_database_backend();
    let row = db
        .conn
        .query_one_raw(Statement::from_string(
            backend,
            "SELECT COALESCE(MAX(version), 0) AS v FROM schema_migrations".to_string(),
        ))
        .await
        .expect("query")
        .expect("row");
    let max = row.try_get::<i64>("", "v").expect("v");

    // Fresh connect creates the current schema and stamps the latest listed
    // version directly — replaying a migration (e.g. ADD COLUMN) against the
    // just-created tables would fail.
    assert_eq!(max, latest_version());
}

#[tokio::test]
async fn provider_round_trip() {
    let db = mem().await;
    let created = db
        .upsert_provider(ProviderInput {
            id: None,
            name: "openai".to_owned(),
            channel: "openai".to_owned(),
            label: Some("OpenAI".to_owned()),
            settings_json: json!({"base_url": "https://api.openai.com"}),
            credential_strategy: "round_robin".to_owned(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .expect("insert");
    assert!(created.id > 0);

    let fetched = db
        .get_provider_by_name("openai")
        .await
        .expect("get")
        .expect("some");
    assert_eq!(fetched, created);

    let updated = db
        .upsert_provider(ProviderInput {
            id: Some(created.id),
            name: "openai".to_owned(),
            channel: "openai".to_owned(),
            label: None,
            settings_json: json!({"base_url": "https://x"}),
            credential_strategy: "sticky".to_owned(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: false,
        })
        .await
        .expect("update");
    assert_eq!(updated.id, created.id);
    assert_eq!(updated.credential_strategy, "sticky");
    assert!(!updated.enabled);

    assert_eq!(db.list_providers().await.expect("list").len(), 1);
    assert!(db.delete_provider(created.id).await.expect("delete"));
    assert!(db.list_providers().await.expect("list").is_empty());
}

#[tokio::test]
async fn cascade_deletes() {
    let db = mem().await;

    // provider → credential → status, and provider → model.
    let p = db
        .upsert_provider(ProviderInput {
            id: None,
            name: "p".to_owned(),
            channel: "openai".to_owned(),
            label: None,
            settings_json: json!({}),
            credential_strategy: "round_robin".to_owned(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .unwrap();
    let c = db
        .upsert_credential(CredentialInput {
            id: None,
            provider_id: p.id,
            name: None,
            kind: "api_key".to_owned(),
            secret_json: json!({"key": "x"}),
            weight: 1,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .unwrap();
    db.upsert_credential_status(CredentialStatusInput {
        id: None,
        credential_id: c.id,
        channel: "openai".to_owned(),
        health_kind: "ok".to_owned(),
        health_json: None,
        checked_at: None,
        last_error: None,
    })
    .await
    .unwrap();
    db.upsert_provider_model(ProviderModelInput {
        id: None,
        provider_id: p.id,
        model_id: "gpt-x".to_owned(),
        display_name: None,
        pricing_json: None,
        variants_json: None,
        enabled: true,
    })
    .await
    .unwrap();

    db.delete_provider(p.id).await.unwrap();
    assert!(db.list_credentials(p.id).await.unwrap().is_empty());
    assert!(db.list_credential_statuses(c.id).await.unwrap().is_empty());
    assert!(db.list_provider_models(p.id).await.unwrap().is_empty());

    // route → member + alias.
    let r = db
        .upsert_route(RouteInput {
            id: None,
            name: "r".to_owned(),
            strategy: "weighted".to_owned(),
            enabled: true,
            description: None,
            settings_json: None,
        })
        .await
        .unwrap();
    db.upsert_route_member(RouteMemberInput {
        id: None,
        route_id: r.id,
        provider_id: p.id,
        upstream_model_id: "gpt-x".to_owned(),
        weight: 1,
        tier: 0,
        enabled: true,
    })
    .await
    .unwrap();
    db.upsert_alias(AliasInput {
        id: None,
        alias: "a".to_owned(),
        route_id: r.id,
    })
    .await
    .unwrap();

    db.delete_route(r.id).await.unwrap();
    assert!(db.list_route_members(r.id).await.unwrap().is_empty());
    assert!(db.get_alias_by_name("a").await.unwrap().is_none());
}

#[tokio::test]
async fn quota_decimal_exact_round_trip() {
    use rust_decimal::Decimal;

    // Money is stored as a decimal STRING; assert it survives the sqlite
    // round-trip with no float drift (exact equality, not approximate).
    let db = mem().await;
    let quota_total = "0.000000123".parse::<Decimal>().unwrap();
    let cost_used = "0.000000001".parse::<Decimal>().unwrap();

    let created = db
        .upsert_quota(QuotaInput {
            id: None,
            scope: Scope::User,
            scope_id: 42,
            quota_total,
            cost_used,
        })
        .await
        .expect("upsert quota");

    let fetched = db
        .get_quota(Scope::User, 42)
        .await
        .expect("get quota")
        .expect("quota present");

    assert_eq!(fetched.quota_total, quota_total);
    assert_eq!(fetched.cost_used, cost_used);
    assert_eq!(fetched, created);
}

#[tokio::test]
async fn add_quota_cost_accumulates() {
    use rust_decimal::Decimal;

    let db = mem().await;
    db.upsert_quota(QuotaInput {
        id: None,
        scope: Scope::User,
        scope_id: 7,
        quota_total: Decimal::from(100),
        cost_used: Decimal::ZERO,
    })
    .await
    .expect("seed quota");

    let delta = "1.5".parse::<Decimal>().unwrap();
    db.add_quota_cost(Scope::User, 7, delta)
        .await
        .expect("add 1");
    db.add_quota_cost(Scope::User, 7, delta)
        .await
        .expect("add 2");

    let q = db
        .get_quota(Scope::User, 7)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(q.cost_used, "3.0".parse::<Decimal>().unwrap());

    // Absent row → Ok, no-op.
    db.add_quota_cost(Scope::Org, 999, delta)
        .await
        .expect("absent row is a no-op");
    assert!(db.get_quota(Scope::Org, 999).await.expect("get").is_none());
}

#[tokio::test]
async fn tokenizer_vocab_round_trip() {
    let db = mem().await;
    db.put_tokenizer_vocab("org/repo", b"{\"v\":1}")
        .await
        .expect("put");
    db.put_tokenizer_vocab("org/repo", b"{\"v\":2}")
        .await
        .expect("upsert");
    let bytes = db
        .get_tokenizer_vocab("org/repo")
        .await
        .expect("get")
        .expect("present");
    assert_eq!(bytes, b"{\"v\":2}");
    assert_eq!(
        db.list_tokenizer_vocabs().await.expect("list"),
        vec!["org/repo"]
    );
    assert!(
        db.get_tokenizer_vocab("absent")
            .await
            .expect("get")
            .is_none()
    );
}

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

#[tokio::test]
async fn metrics_aggregate_sums_rollups_and_buckets_latency() {
    let db = mem().await;
    // Two settled requests with measured latency (60ms, 600ms) + an hour rollup.
    for (rid, lat) in [("r1", 60i64), ("r2", 600)] {
        db.append_usage(UsageInput {
            request_id: rid.to_owned(),
            at: 100,
            route_name: None,
            provider_id: None,
            credential_id: None,
            org_id: None,
            team_id: None,
            user_id: None,
            user_key_id: None,
            operation: "chat".into(),
            kind: "openai".into(),
            model: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_5m_tokens: 0,
            cache_creation_1h_tokens: 0,
            cost: rust_decimal::Decimal::ZERO,
            latency_ms: lat,
            usage_source: "upstream".into(),
            ended: "complete".into(),
        })
        .await
        .expect("usage");
    }
    db.add_usage_rollup(UsageRollupInput {
        granularity: "hour".into(),
        bucket_start: 0,
        provider_id: None,
        org_id: None,
        team_id: None,
        user_id: None,
        route_name: None,
        model: None,
        requests: 5,
        input_tokens: 1000,
        output_tokens: 400,
        cost: rust_decimal::Decimal::ZERO,
    })
    .await
    .expect("rollup");

    let m = db.metrics_aggregate().await.expect("aggregate");
    assert_eq!(m.requests_total, 5);
    assert_eq!(m.input_tokens_total, 1000);
    assert_eq!(m.output_tokens_total, 400);
    assert_eq!(m.latency_count, 2);
    assert_eq!(m.latency_sum_ms, 660);
    // buckets are [50,100,250,500,1000,...]: 60ms → first ≤100 bucket; 600ms → ≤1000.
    assert_eq!(m.latency_buckets[0], 0, "≤50ms");
    assert_eq!(m.latency_buckets[1], 1, "≤100ms");
    assert_eq!(m.latency_buckets[4], 2, "≤1000ms cumulative");
}

#[tokio::test]
async fn audit_log_round_trip() {
    let db = mem().await;
    db.append_audit_log(AuditLogInput {
        actor_id: Some(7),
        actor_name: Some("admin".into()),
        action: "DELETE".into(),
        target: "/admin/credentials/5".into(),
        status: 204,
        source_ip: Some("203.0.113.9".into()),
    })
    .await
    .expect("append 1");
    db.append_audit_log(AuditLogInput {
        actor_id: None,
        actor_name: None,
        action: "login.fail".into(),
        target: "alice".into(),
        status: 401,
        source_ip: None,
    })
    .await
    .expect("append 2");

    let rows = db.list_audit_logs(10).await.expect("list");
    assert_eq!(rows.len(), 2);
    // Most-recent first (id desc): the login.fail row leads.
    assert_eq!(rows[0].action, "login.fail");
    assert_eq!(rows[0].target, "alice");
    assert_eq!(rows[0].actor_id, None);
    assert_eq!(rows[1].action, "DELETE");
    assert_eq!(rows[1].actor_name.as_deref(), Some("admin"));
    assert_eq!(rows[1].status, 204);
    assert!(rows[0].id > rows[1].id);

    // limit caps the result.
    assert_eq!(db.list_audit_logs(1).await.expect("list 1").len(), 1);
}
