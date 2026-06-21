//! Tests for [`FilePersistence`].

use super::FilePersistence;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::*;
use serde_json::json;

async fn open() -> (tempfile::TempDir, FilePersistence) {
    let dir = tempfile::tempdir().expect("tempdir");
    let fp = FilePersistence::open(dir.path().to_path_buf())
        .await
        .expect("open");
    (dir, fp)
}

#[tokio::test]
async fn open_and_health_ok() {
    let (_dir, fp) = open().await;
    fp.health().await.expect("health");
}

/// Regression: the data dir is single-instance — a second open of the same dir
/// while the first is alive must fail (flock), not silently share state.
#[tokio::test]
async fn second_open_of_same_dir_fails() {
    let (dir, fp) = open().await;
    let err = match FilePersistence::open(dir.path().to_path_buf()).await {
        Ok(_) => panic!("second open must fail"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("already in use"), "{err}");
    drop(fp);
    FilePersistence::open(dir.path().to_path_buf())
        .await
        .expect("re-open after release");
}

#[tokio::test]
async fn provider_round_trip() {
    let (_dir, fp) = open().await;
    let input = ProviderInput {
        id: None,
        name: "openai".to_owned(),
        channel: "openai".to_owned(),
        label: Some("OpenAI".to_owned()),
        settings_json: json!({"base_url": "https://api.openai.com"}),
        credential_strategy: "round_robin".to_owned(),
        proxy_url: None,
        tls_fingerprint: None,
        enabled: true,
    };
    let created = fp.upsert_provider(input).await.expect("insert");
    assert!(created.id > 0);

    // Duplicate name rejected.
    assert!(
        fp.upsert_provider(ProviderInput {
            id: None,
            name: "openai".to_owned(),
            channel: "x".to_owned(),
            label: None,
            settings_json: json!({}),
            credential_strategy: "round_robin".to_owned(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .is_err()
    );

    assert_eq!(
        fp.get_provider_by_name("openai")
            .await
            .expect("get")
            .as_ref(),
        Some(&created)
    );

    let updated = fp
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
    assert_eq!(updated.credential_strategy, "sticky");

    assert!(fp.delete_provider(created.id).await.expect("delete"));
    assert!(fp.list_providers().await.expect("list").is_empty());
}

#[tokio::test]
async fn add_quota_cost_accumulates() {
    use rust_decimal::Decimal;

    let (_dir, fp) = open().await;
    fp.upsert_quota(QuotaInput {
        id: None,
        scope: Scope::User,
        scope_id: 7,
        quota_total: Decimal::from(100),
        cost_used: Decimal::ZERO,
    })
    .await
    .expect("seed quota");

    let delta = "1.5".parse::<Decimal>().unwrap();
    fp.add_quota_cost(Scope::User, 7, delta)
        .await
        .expect("add 1");
    fp.add_quota_cost(Scope::User, 7, delta)
        .await
        .expect("add 2");

    let q = fp
        .get_quota(Scope::User, 7)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(q.cost_used, "3.0".parse::<Decimal>().unwrap());

    // Absent row → Ok, no-op (the request just isn't cost-tracked).
    fp.add_quota_cost(Scope::Org, 999, delta)
        .await
        .expect("absent row is a no-op");
    assert!(fp.get_quota(Scope::Org, 999).await.expect("get").is_none());
}

/// Regression: editing an existing quota (e.g. changing `quota_total` from the
/// admin UI) must NOT clobber the billing-accumulated `cost_used`, even when the
/// request body carries a stale `cost_used`. Seeding (insert) still honors input.
#[tokio::test]
async fn quota_upsert_preserves_accumulated_cost_used() {
    use rust_decimal::Decimal;

    let (_dir, fp) = open().await;
    let seeded = fp
        .upsert_quota(QuotaInput {
            id: None,
            scope: Scope::User,
            scope_id: 7,
            quota_total: Decimal::from(100),
            cost_used: Decimal::ZERO,
        })
        .await
        .expect("seed quota");

    // Billing accumulates cost.
    fp.add_quota_cost(Scope::User, 7, Decimal::from(42))
        .await
        .expect("charge");

    // Admin edits quota_total and sends a STALE cost_used=0 (the clobber case).
    fp.upsert_quota(QuotaInput {
        id: Some(seeded.id),
        scope: Scope::User,
        scope_id: 7,
        quota_total: Decimal::from(250),
        cost_used: Decimal::ZERO, // stale — must be ignored on update
    })
    .await
    .expect("edit quota_total");

    let q = fp
        .get_quota(Scope::User, 7)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(q.quota_total, Decimal::from(250), "quota_total updated");
    assert_eq!(
        q.cost_used,
        Decimal::from(42),
        "cost_used preserved, not clobbered by the stale request body"
    );
}
