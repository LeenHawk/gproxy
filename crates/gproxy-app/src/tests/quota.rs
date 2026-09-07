use gproxy_core::{ControlPlane, Host};
use rust_decimal::Decimal;

use super::setup;

const QUOTA_INPUT: &str = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty";

#[tokio::test]
async fn disabled_credentials_keep_quota_metadata_but_cannot_send_requests() {
    use crate::{ControlMutation, MutationResult};
    use gproxy_admin::State;
    use gproxy_core::CredentialStore;

    let fixture = setup::fixture().await;
    for (channel, subscription) in [("openai", false), ("codex", true)] {
        let MutationResult::Id(provider) = fixture
            .app
            .mutate(ControlMutation::Provider(
                gproxy_store::records::ProviderInput {
                    name: format!("disabled-{channel}"),
                    label: None,
                    channel: channel.into(),
                    settings: serde_json::json!({}),
                    credential_strategy: "round_robin".into(),
                    proxy_url: None,
                    tls_fingerprint: None,
                    enabled: true,
                },
            ))
            .await
            .unwrap()
        else {
            panic!("provider id")
        };
        let MutationResult::Id(credential) = fixture
            .app
            .mutate(ControlMutation::Credential {
                provider_id: provider,
                label: None,
                secret: serde_json::json!({"api_key": setup::random_key()}),
                enabled: false,
            })
            .await
            .unwrap()
        else {
            panic!("credential id")
        };
        let capability = fixture
            .app
            .credential_quota_capabilities(credential)
            .await
            .unwrap();
        assert_eq!(capability.is_some(), subscription);
        if subscription {
            assert_reset_credits_need_full_probe(&fixture.app, credential).await;
        }
        assert!(
            fixture
                .app
                .inner
                .host
                .load(gproxy_core::CredentialId(credential))
                .await
                .is_err()
        );
    }
    let MutationResult::Id(orphan) = fixture
        .app
        .mutate(ControlMutation::Credential {
            provider_id: i64::MAX,
            label: None,
            secret: serde_json::json!({"api_key": setup::random_key()}),
            enabled: false,
        })
        .await
        .unwrap()
    else {
        panic!("orphan credential id")
    };
    assert!(
        fixture
            .app
            .credential_quota_capabilities(orphan)
            .await
            .unwrap()
            .is_none()
    );
}

async fn assert_reset_credits_need_full_probe(app: &crate::AppHandle, credential: i64) {
    use gproxy_admin::{AdminError, State};
    use gproxy_core::CacheBackend;
    use gproxy_store::records::{
        CredentialQuotaObservation, QuotaBoundaryConfidence, QuotaBoundarySource,
    };

    let now = crate::quota_refresh::now();
    app.inner
        .host
        .services
        .store
        .observe_credential_quota_cycle(&CredentialQuotaObservation {
            credential_id: credential,
            window_key: "primary".into(),
            label: None,
            period_start: Some(now - 60),
            period_end: Some(now + 3600),
            observed_at: now,
            boundary_source: QuotaBoundarySource::Upstream,
            boundary_confidence: QuotaBoundaryConfidence::Exact,
            sample: gproxy_core::QuotaSample {
                source: gproxy_core::QuotaSampleSource::Unknown,
                started_at_ms: now * 1000,
                received_at_ms: now * 1000,
            },
            scope: gproxy_core::QuotaScope::All,
            reset_behavior: gproxy_core::QuotaResetBehavior::Periodic,
            unit: None,
            upstream_used: None,
            upstream_limit: None,
            used_percent: Some(10.into()),
        })
        .await
        .unwrap();
    let cache = &app.inner.host.services.cache;
    cache
        .set(
            &format!("quota:upstream-retry:{credential}"),
            vec![1],
            Some(std::time::Duration::from_secs(60)),
        )
        .await
        .unwrap();
    assert!(matches!(app.quota_probe(credential, false).await,
        Err(AdminError::Conflict(message)) if message == "upstream requested a longer quota retry interval"));
    let cached = gproxy_admin::dto::QuotaProbeResponse {
        windows: Vec::new(),
        cycles: Vec::new(),
        local_error: false,
        raw: String::new(),
        reset_credits: Some(gproxy_admin::dto::QuotaResetCreditsDto {
            available_count: 2,
            expires_at: None,
        }),
    };
    cache
        .set(
            &format!("quota:probe:{credential}"),
            serde_json::to_vec(&cached).unwrap(),
            Some(std::time::Duration::from_secs(60)),
        )
        .await
        .unwrap();
    assert_eq!(
        app.quota_probe(credential, false)
            .await
            .unwrap()
            .reset_credits
            .unwrap()
            .available_count,
        2
    );
}

#[tokio::test]
async fn admission_refunds_reconciles_and_leaves_no_failed_reservation() {
    let setup::Fixture {
        app,
        provider,
        credential,
        route: _,
        quota,
        client_key,
        _directory,
    } = setup::fixture().await;

    let host = &app.inner.host;
    let plan = host
        .services
        .control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
        .expect("plan");
    let first = setup::request("refund", QUOTA_INPUT, &client_key);
    let identity = host.authenticate(&first).await.expect("authenticate");
    let operation = super::generation_operation();
    host.admit(&identity, &first, Some(operation), &plan)
        .await
        .expect("first admission");
    assert!(app.admission_pending(&first.request_id).await.unwrap());
    let overlap = setup::request("overlap", QUOTA_INPUT, &client_key);
    assert!(matches!(
        host.admit(&identity, &overlap, Some(operation), &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&overlap.request_id).await.unwrap());
    host.finish_admission(&first.request_id, None).await;
    assert!(!app.admission_pending(&first.request_id).await.unwrap());

    let second = setup::request("settle", QUOTA_INPUT, &client_key);
    host.admit(&identity, &second, Some(operation), &plan)
        .await
        .expect("second admission");
    let settlement = gproxy_core::Settlement {
        attempts: Vec::new(),
        upstream_started_at_ms: None,
        request_id: second.request_id.clone(),
        provider_id: provider,
        credential_id: gproxy_core::CredentialId(credential),
        upstream_model: "upstream-model".into(),
        usage: gproxy_core::NormalizedUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        cost: Decimal::new(2, 1),
        source: gproxy_core::UsageSource::Upstream,
        ended: gproxy_core::Ended::Complete,
        latency_ms: 1,
    };
    tokio::join!(
        host.finish_admission(&second.request_id, Some(&settlement)),
        host.finish_admission(&second.request_id, Some(&settlement)),
    );
    assert!(!app.admission_pending(&second.request_id).await.unwrap());

    let windows: Vec<_> = app
        .quota_windows()
        .await
        .unwrap()
        .into_iter()
        .filter(|window| window.quota_id == quota)
        .collect();
    assert_eq!(windows.len(), 2);
    assert!(windows.iter().any(|window| window.reset_at.is_none()));
    assert!(windows.iter().any(|window| window.reset_at.is_some()));
    for window in &windows {
        assert_eq!(window.cost_used, Decimal::new(2, 1));
        assert_eq!(setup::counter(host, window.id).await, 0);
    }

    let rejected = setup::request("reject", QUOTA_INPUT, &client_key);
    assert!(matches!(
        host.admit(&identity, &rejected, Some(operation), &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&rejected.request_id).await.unwrap());
    for window in &windows {
        assert_eq!(setup::counter(host, window.id).await, 0);
    }
}
