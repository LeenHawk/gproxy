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
    assert!(
        !fixture
            .app
            .channel_catalogue()
            .iter()
            .any(|channel| channel.id == "groq")
    );
    for (channel, subscription) in [
        ("openai", false),
        ("codex", true),
        ("removed-channel", false),
    ] {
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
        assert_eq!(capability.unwrap().probe, subscription);
        let snapshot = fixture
            .app
            .credential_quota_snapshot(credential)
            .await
            .unwrap();
        if channel == "removed-channel" {
            assert_eq!(
                snapshot.sources[0].capability.support,
                gproxy_channel_api::QuotaSupport::Unsupported
            );
            assert!(snapshot.entries.is_empty());
        }
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
    use gproxy_admin::State;
    use gproxy_channel_api::{QuotaRefreshError, QuotaResetCredits, QuotaSourceState};
    use gproxy_core::CacheBackend;

    let snapshot = app.credential_quota_snapshot(credential).await.unwrap();
    let source = snapshot
        .sources
        .into_iter()
        .find(|source| source.capability.id == "subscription")
        .unwrap();
    let version = app
        .store()
        .credential(credential)
        .await
        .unwrap()
        .unwrap()
        .version;
    let now = crate::quota_refresh::now() * 1000;
    let mut state = QuotaSourceState {
        capability: source.capability,
        attempted_at_ms: Some(now - 1000),
        observed_at_ms: Some(now - 1000),
        error: None,
        reset_credits: Some(QuotaResetCredits {
            available_count: 2,
            expires_at: None,
        }),
    };
    app.store()
        .save_credential_quota_source(credential, version, &state, Some(&[]))
        .await
        .unwrap();
    state.attempted_at_ms = Some(now);
    state.error = Some(QuotaRefreshError {
        code: "rate_limited".into(),
        message: "retry later".into(),
    });
    state.reset_credits = None;
    app.store()
        .save_credential_quota_source(credential, version, &state, None)
        .await
        .unwrap();
    app.inner
        .host
        .services
        .cache
        .set(
            &format!("quota:source:{credential}:v{version}:subscription:upstream-retry"),
            vec![1],
            Some(std::time::Duration::from_secs(60)),
        )
        .await
        .unwrap();
    let result = app.quota_probe(credential, true).await.unwrap();
    assert_eq!(result.reset_credits.unwrap().available_count, 2);
    assert_eq!(
        result.snapshot.sources[0].error.as_ref().unwrap().code,
        "rate_limited"
    );
    assert_eq!(result.snapshot.sources[0].attempted_at_ms, Some(now));
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
    host.admit(&identity, &first, Some(operation), None, &plan)
        .await
        .expect("first admission");
    assert!(app.admission_pending(&first.request_id).await.unwrap());
    let overlap = setup::request("overlap", QUOTA_INPUT, &client_key);
    assert!(matches!(
        host.admit(&identity, &overlap, Some(operation), None, &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&overlap.request_id).await.unwrap());
    host.finish_admission(&first.request_id, None).await;
    assert!(!app.admission_pending(&first.request_id).await.unwrap());

    let second = setup::request("settle", QUOTA_INPUT, &client_key);
    host.admit(&identity, &second, Some(operation), None, &plan)
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
        host.admit(&identity, &rejected, Some(operation), None, &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&rejected.request_id).await.unwrap());
    for window in &windows {
        assert_eq!(setup::counter(host, window.id).await, 0);
    }
}
