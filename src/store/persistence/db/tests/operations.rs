//! CRUD, cascade, quota, and tokenizer persistence tests.

use super::*;
use crate::store::persistence::traits::{
    AuthzPersistence, ProviderPersistence, RoutingPersistence, SettingsPersistence,
};

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
    db.upsert_credential_model_status(CredentialModelStatusInput {
        id: None,
        credential_id: c.id,
        channel: "openai".to_owned(),
        model_id: "gpt-x-final".to_owned(),
        health_kind: "breaker".to_owned(),
        health_json: Some(json!({"state": "open"})),
        checked_at: Some(42),
        last_error: Some("failed".to_owned()),
    })
    .await
    .unwrap();
    db.upsert_provider_model(ProviderModelInput {
        id: None,
        provider_id: p.id,
        model_id: "gpt-x".to_owned(),
        display_name: None,
        variants_json: None,
        enabled: true,
    })
    .await
    .unwrap();

    let directly_deleted = db
        .upsert_credential(CredentialInput {
            id: None,
            provider_id: p.id,
            name: Some("direct-delete".to_owned()),
            kind: "api_key".to_owned(),
            secret_json: json!({"key": "y"}),
            weight: 1,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .unwrap();
    db.upsert_credential_model_status(CredentialModelStatusInput {
        id: None,
        credential_id: directly_deleted.id,
        channel: "openai".to_owned(),
        model_id: "gpt-y-final".to_owned(),
        health_kind: "breaker".to_owned(),
        health_json: None,
        checked_at: None,
        last_error: None,
    })
    .await
    .unwrap();
    assert!(db.delete_credential(directly_deleted.id).await.unwrap());
    assert!(
        db.list_credential_model_statuses(directly_deleted.id)
            .await
            .unwrap()
            .is_empty()
    );

    db.delete_provider(p.id).await.unwrap();
    assert!(db.list_credentials(p.id).await.unwrap().is_empty());
    assert!(db.list_credential_statuses(c.id).await.unwrap().is_empty());
    assert!(
        db.list_credential_model_statuses(c.id)
            .await
            .unwrap()
            .is_empty()
    );
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
        provider: "*".to_owned(),
        alias: "a".to_owned(),
        target: Some("r".to_owned()),
        sort_order: 0,
        enabled: true,
    })
    .await
    .unwrap();

    db.delete_route(r.id).await.unwrap();
    assert!(db.list_route_members(r.id).await.unwrap().is_empty());
    assert!(db.get_alias_by_name("a").await.unwrap().is_some());
}

#[tokio::test]
async fn credential_model_status_crud_uses_composite_key() {
    let db = mem().await;
    let first = db
        .upsert_credential_model_status(CredentialModelStatusInput {
            id: None,
            credential_id: 7,
            channel: "openai".to_owned(),
            model_id: "upstream/model-v1".to_owned(),
            health_kind: "breaker".to_owned(),
            health_json: Some(json!({"state": "open"})),
            checked_at: Some(10),
            last_error: Some("timeout".to_owned()),
        })
        .await
        .expect("insert model status");
    let updated = db
        .upsert_credential_model_status(CredentialModelStatusInput {
            id: None,
            credential_id: 7,
            channel: "openai".to_owned(),
            model_id: "upstream/model-v1".to_owned(),
            health_kind: "recovered".to_owned(),
            health_json: Some(json!({"state": "closed"})),
            checked_at: Some(11),
            last_error: None,
        })
        .await
        .expect("update by composite key");

    assert_eq!(updated.id, first.id);
    assert_eq!(updated.health_kind, "recovered");
    assert_eq!(
        db.list_credential_model_statuses(7).await.unwrap(),
        vec![updated.clone()]
    );
    assert_eq!(
        db.list_all_credential_model_statuses().await.unwrap(),
        vec![updated]
    );
    assert!(db.delete_credential_model_status(first.id).await.unwrap());
    assert!(
        db.list_all_credential_model_statuses()
            .await
            .unwrap()
            .is_empty()
    );
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
