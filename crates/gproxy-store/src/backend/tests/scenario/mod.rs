mod admin;
mod cycle;
mod oauth;
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
    statistics: Vec<UsageAggregateRecord>,
    trend: Vec<UsageTrendPoint>,
    window: UsageWindow,
    quota: QuotaWindowRecord,
    cycle: cycle::Outcome,
    binding: BindingPage,
    tokenizer_vocabs: Vec<String>,
    admin: admin::Outcome,
    rollup_requests: i64,
    wire_logs: i64,
    oauth: OAuthSessionPage,
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
            label: None,
            channel: "channel".into(),
            settings: json!({"base_url": "https://upstream.invalid"}),
            credential_strategy: "round_robin".into(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await?;
    let credential = store
        .insert_credential(&CredentialInput {
            provider_id: provider,
            label: None,
            kind: "api_key".into(),
            envelope: envelope(1),
            enabled: true,
            weight: 100,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
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
            upstream_model: "upstream-model".into(),
            tier: 0,
            weight: 100,
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
    store
        .insert_provider_model(&crate::records::ProviderModelInput {
            provider_id: provider,
            model_id: "upstream-model".into(),
            display_name: None,
            variants: None,
            context_window: Some(128_000),
            max_output_tokens: None,
            thinking_supported: None,
            thinking_adaptive_supported: None,
            thinking_enabled_supported: None,
            metadata: gproxy_core::ModelMetadata {
                description: Some("Test model".into()),
                input_modalities: Some(vec!["text".into(), "image".into()]),
                output_modalities: Some(Vec::new()),
                supported_parameters: Some(vec!["tools".into()]),
                reasoning_levels: Some(vec![gproxy_core::ModelReasoningLevel {
                    effort: "high".into(),
                    description: "Deep reasoning".into(),
                }]),
                service_tiers: Some(vec![gproxy_core::ModelServiceTier {
                    id: "priority".into(),
                    name: "Fast".into(),
                    description: "Faster responses".into(),
                }]),
                generation_methods: Some(Vec::new()),
                supported_actions: Some(Vec::new()),
                ..Default::default()
            },
            enabled: true,
        })
        .await?;
    let user_key = seed_identity(store).await?;
    seed_pricing(store, provider).await?;
    store
        .set_setting(&SettingInput {
            key: "enable_upstream_log".into(),
            value: json!(true),
        })
        .await?;
    let snapshot = store.control_snapshot().await?;
    let model = snapshot
        .provider_models
        .first()
        .expect("seeded provider model");
    assert_eq!(
        model.metadata.input_modalities.as_deref(),
        Some(&["text".to_owned(), "image".to_owned()][..])
    );
    assert_eq!(model.metadata.output_modalities, Some(Vec::new()));
    assert_eq!(model.metadata.generation_methods, Some(Vec::new()));
    assert_eq!(model.metadata.supported_actions, Some(Vec::new()));
    let admin = admin::run(store, user_key).await?;
    delete_route_takes_its_rows(store, provider).await?;

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
    let quota = store
        .add_quota_cost("quota-request", quota.id, Decimal::new(15, 4))
        .await?;
    let quota = store
        .add_quota_cost("quota-request", quota.id, Decimal::new(15, 4))
        .await?;
    let mut second_usage = usage_input.clone();
    second_usage.request_id = "request-2".into();
    second_usage.upstream_model = "alternate-model".into();
    second_usage.input_tokens = 7;
    second_usage.output_tokens = 3;
    second_usage.cached_input_tokens = 1;
    second_usage.metrics = json!({"audio_seconds": 2});
    second_usage.cost = Decimal::new(1, 5);
    assert!(store.record_usage(&second_usage).await?);
    let statistics = store
        .usage_aggregate(&UsageAggregateQuery {
            from: 0,
            to: 4_000,
            group_by: UsageGroupBy::Dimensions,
            user_key_id: None,
            user_id: None,
            provider_id: None,
            credential_id: None,
            model: None,
        })
        .await?;
    let filtered_statistics = store
        .usage_aggregate(&UsageAggregateQuery {
            from: 0,
            to: 4_000,
            group_by: UsageGroupBy::Dimensions,
            user_key_id: None,
            user_id: None,
            provider_id: None,
            credential_id: Some(credential.id),
            model: None,
        })
        .await?;
    let trend = store.usage_trend(0, 4_000).await?;
    let cycle = cycle::run(store, credential.id).await?;
    let mut binding = seed_binding(store, provider, credential.id).await?;
    binding.items[0].created_at = 0;
    seed_capture(store, provider, credential.id).await?;
    store
        .put_tokenizer_vocab("local-vocab", "owner/model", b"vocab")
        .await?;
    assert_eq!(
        store.tokenizer_vocab("local-vocab").await?,
        Some(crate::records::TokenizerVocabData {
            repository: "owner/model".into(),
            bytes: b"vocab".to_vec(),
        })
    );
    let tokenizer_vocabs = store.tokenizer_vocab_names().await?;
    store.delete_tokenizer_vocab("local-vocab").await?;
    assert_eq!(store.tokenizer_vocab("local-vocab").await?, None);
    let auth = envelope(9);
    store.put_tokenizer_auth("hugging_face", &auth).await?;
    assert_eq!(store.tokenizer_auth("hugging_face").await?, Some(auth));
    store.delete_tokenizer_auth("hugging_face").await?;
    assert_eq!(store.tokenizer_auth("hugging_face").await?, None);
    let rollup_requests = scalar(store, "SELECT requests FROM usage_rollups").await?;
    let wire_logs = scalar(store, "SELECT COUNT(*) AS value FROM wire_logs").await?;
    let log = store.log_detail("request-1").await?.expect("log detail");

    assert_eq!(snapshot.providers.len(), 1);
    assert_eq!(snapshot.credentials.len(), 1);
    assert_eq!(credential.version, 1);
    assert_eq!(credential.envelope, envelope(2));
    assert_eq!(usage.usage.cost, usage_input.cost);
    assert!(usage.usage.cost > rust_decimal::Decimal::ZERO);
    assert_eq!(statistics.len(), 2);
    assert_eq!(statistics[0].user_key_id, Some(1));
    assert_eq!(statistics[0].user_id, Some(1));
    assert_eq!(statistics[0].provider_id, provider);
    assert_eq!(statistics[0].model, "upstream-model");
    assert_eq!(statistics[0].cache_creation_5m_tokens, 3);
    assert_eq!(statistics[0].cache_creation_30m_tokens, 4);
    assert_eq!(statistics[0].cache_creation_1h_tokens, 5);
    assert_eq!(filtered_statistics, statistics);
    assert_eq!(trend.len(), 1);
    assert_eq!(trend[0].bucket_start, 3_600);
    assert_eq!(trend[0].requests, 2);
    assert_eq!(trend[0].input_tokens, 17);
    assert_eq!(trend[0].output_tokens, 8);
    assert_eq!(trend[0].cached_input_tokens, 3);
    assert_eq!(trend[0].cost, Decimal::new(3, 5));
    assert_eq!(window.input_tokens, 10);
    assert_eq!(window.output_tokens, 5);
    assert_eq!(quota.cost_used, Decimal::new(15, 4));
    assert_eq!(quota.reset_at, Some(86_400));
    assert_eq!(binding.items.len(), 1);
    assert_eq!(binding.next_cursor, None);
    assert_eq!(rollup_requests, 1);
    assert_eq!(wire_logs, 3);
    assert_eq!(
        log.downstream.input.client_ip.as_deref(),
        Some("198.51.100.7")
    );
    assert_eq!(log.downstream.duration_ms, Some(12));
    assert_eq!(log.downstream.output_tokens, Some(5));
    assert_eq!(log.upstream.len(), 2);
    assert_eq!(log.upstream[0].input.response_status, Some(503));
    assert_eq!(log.upstream[1].input.response_status, Some(200));

    let oauth = oauth::run(store).await?;
    Ok(Outcome {
        snapshot,
        credential,
        usage,
        statistics,
        trend,
        window,
        quota,
        cycle,
        binding,
        tokenizer_vocabs,
        admin,
        rollup_requests,
        wire_logs,
        oauth,
    })
}

/// Regression: a deleted route used to leave its members and public names
/// behind, and the orphaned name blocked every later mapping with it.
async fn delete_route_takes_its_rows(store: &Store, provider: i64) -> Result<(), StoreError> {
    let route = store
        .insert_route(&RouteInput {
            name: "doomed-route".into(),
            max_attempts: 1,
            enabled: true,
        })
        .await?;
    store
        .insert_route_member(&RouteMemberInput {
            route_id: route,
            provider_id: provider,
            upstream_model: "doomed-upstream".into(),
            tier: 0,
            weight: 100,
            enabled: true,
        })
        .await?;
    store
        .insert_exposed_model(&ExposedModelInput {
            name: "doomed-model".into(),
            route_id: route,
            enabled: true,
        })
        .await?;
    assert!(store.delete_route(route).await?);
    assert!(!store.delete_route(route).await?);
    let snapshot = store.control_snapshot().await?;
    assert!(!snapshot.route_members.iter().any(|m| m.route_id == route));
    assert!(!snapshot.exposed_models.iter().any(|m| m.route_id == route));
    let replacement = store
        .insert_route(&RouteInput {
            name: "replacement-route".into(),
            max_attempts: 1,
            enabled: true,
        })
        .await?;
    store
        .insert_exposed_model(&ExposedModelInput {
            name: "doomed-model".into(),
            route_id: replacement,
            enabled: true,
        })
        .await?;
    assert!(store.delete_route(replacement).await?);
    Ok(())
}
