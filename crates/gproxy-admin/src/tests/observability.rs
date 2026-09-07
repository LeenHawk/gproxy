use super::*;
use gproxy_store::records::{
    CredentialQuotaObservation, QuotaBoundaryConfidence, QuotaBoundarySource, SettingInput,
};

#[tokio::test]
async fn cycle_history_is_opt_in_and_usage_disabled_keeps_only_upstream_readings() {
    let state = state().await;
    seed_admin_key(&state).await;
    state
        .store
        .observe_credential_quota_cycle(&CredentialQuotaObservation {
            credential_id: 7,
            window_key: "primary".into(),
            label: None,
            period_start: Some(0),
            period_end: Some(100),
            observed_at: 10,
            boundary_source: QuotaBoundarySource::Upstream,
            boundary_confidence: QuotaBoundaryConfidence::Exact,
            sample: gproxy_core::QuotaSample {
                source: gproxy_core::QuotaSampleSource::Unknown,
                started_at_ms: 10_000,
                received_at_ms: 10_000,
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
    for (suffix, expected) in [
        ("", 0),
        ("&include_history=false", 0),
        ("&include_history=true", 1),
    ] {
        let response = crate::dispatch(
            &state,
            &admin_parts(
                Method::GET,
                &format!("/admin/api/credential-cycles?from=0&to=100{suffix}"),
            ),
            Bytes::new(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cycles: Vec<crate::dto::CredentialQuotaCycleDto> =
            serde_json::from_slice(response.body()).unwrap();
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].observations.len(), expected);
        if expected == 1 {
            assert!(cycles[0].observations[0].estimate.is_some());
        }
    }
    state
        .store
        .set_setting(&SettingInput {
            key: gproxy_store::records::ENABLE_USAGE.into(),
            value: serde_json::json!(false),
        })
        .await
        .unwrap();
    let response = crate::dispatch(
        &state,
        &admin_parts(
            Method::GET,
            "/admin/api/credential-cycles?from=0&to=100&include_history=true",
        ),
        Bytes::new(),
    )
    .await
    .unwrap();
    let cycles: Vec<crate::dto::CredentialQuotaCycleDto> =
        serde_json::from_slice(response.body()).unwrap();
    assert_eq!(
        cycles[0].observations[0].used_percent.as_deref(),
        Some("10")
    );
    assert!(cycles[0].observations[0].estimate.is_none());
    assert_eq!(
        cycles[0].estimate.as_ref().unwrap().reason.as_deref(),
        Some("usage_disabled")
    );
    let invalid = crate::dispatch(
        &state,
        &admin_parts(
            Method::GET,
            "/admin/api/credential-cycles?from=0&to=100&include_history=invalid",
        ),
        Bytes::new(),
    )
    .await;
    assert_eq!(invalid.unwrap().status(), StatusCode::BAD_REQUEST);
}
