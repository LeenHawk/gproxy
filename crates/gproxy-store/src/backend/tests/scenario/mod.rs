mod cycle;
mod seed;

use rust_decimal::Decimal;
use serde_json::json;

use self::seed::*;
use crate::records::*;
use crate::{Store, StoreError};

#[derive(Debug, PartialEq)]
pub(super) struct Outcome {
    snapshot: ControlSnapshot,
    credential: CredentialRecord,
    usage: UsageRecord,
    window: UsageWindow,
    quota: QuotaWindowRecord,
    cycle: cycle::Outcome,
    binding: BindingPage,
    rollup_requests: i64,
    wire_logs: i64,
}

pub(super) async fn run(store: &Store) -> Outcome {
    run_inner(store)
        .await
        .expect("representative store behavior")
}

async fn run_inner(store: &Store) -> Result<Outcome, StoreError> {
    let provider = store
        .insert_provider(&ProviderInput {
            name: "provider".into(),
            channel: "channel".into(),
            settings: json!({"base_url": "https://upstream.invalid"}),
            enabled: true,
        })
        .await?;
    let credential = store
        .insert_credential(&CredentialInput {
            provider_id: provider,
            label: None,
            envelope: envelope(1),
            enabled: true,
        })
        .await?;
    let route = store
        .insert_route(&RouteInput {
            name: "route".into(),
            max_attempts: 2,
            enabled: true,
        })
        .await?;
    store
        .insert_route_member(&RouteMemberInput {
            route_id: route,
            provider_id: provider,
            credential_id: Some(credential),
            upstream_model: "upstream-model".into(),
            priority: 0,
            enabled: true,
        })
        .await?;
    store
        .insert_alias(&AliasInput {
            alias: "alias-model".into(),
            target: "public-model".into(),
            provider_id: None,
            priority: 0,
            enabled: true,
        })
        .await?;
    store
        .insert_exposed_model(&ExposedModelInput {
            name: "public-model".into(),
            route_id: route,
            enabled: true,
        })
        .await?;
    seed_identity(store).await?;
    seed_pricing(store, provider).await?;
    store
        .set_setting(&SettingInput {
            key: "capture_enabled".into(),
            value: json!(true),
        })
        .await?;
    let snapshot = store.control_snapshot().await?;

    store
        .persist_credential_rotation(credential, &envelope(2), 0)
        .await?;
    assert!(matches!(
        store
            .persist_credential_rotation(credential, &envelope(3), 0)
            .await,
        Err(StoreError::VersionConflict)
    ));
    let credential = store.credential(credential).await?.expect("credential");

    let usage_input = usage(provider, credential.id);
    assert!(store.record_usage(&usage_input).await?);
    assert!(!store.record_usage(&usage_input).await?);
    let usage = store
        .usage_by_request(&usage_input.request_id)
        .await?
        .expect("usage row");
    let window = store.usage_window(1, provider, 0).await?;
    let quota = store
        .ensure_quota_window(1, QuotaWindowKind::Daily, 3_601)
        .await?;
    let quota = store.add_quota_cost(quota.id, Decimal::new(15, 4)).await?;
    let cycle = cycle::run(store, credential.id).await?;
    let binding = seed_binding(store, provider, credential.id).await?;
    seed_capture(store, provider, credential.id).await?;
    let rollup_requests = scalar(store, "SELECT requests FROM usage_rollups").await?;
    let wire_logs = scalar(store, "SELECT COUNT(*) AS value FROM wire_logs").await?;

    assert_eq!(snapshot.providers.len(), 1);
    assert_eq!(snapshot.credentials.len(), 1);
    assert_eq!(credential.version, 1);
    assert_eq!(credential.envelope, envelope(2));
    assert_eq!(usage.usage.cost, usage_input.cost);
    assert!(usage.usage.cost > rust_decimal::Decimal::ZERO);
    assert_eq!(window.input_tokens, 10);
    assert_eq!(window.output_tokens, 5);
    assert_eq!(quota.cost_used, Decimal::new(15, 4));
    assert_eq!(quota.reset_at, Some(86_400));
    assert_eq!(binding.items.len(), 1);
    assert_eq!(binding.next_cursor, None);
    assert_eq!(rollup_requests, 1);
    assert_eq!(wire_logs, 1);

    Ok(Outcome {
        snapshot,
        credential,
        usage,
        window,
        quota,
        cycle,
        binding,
        rollup_requests,
        wire_logs,
    })
}
