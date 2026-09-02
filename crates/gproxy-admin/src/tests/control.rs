use super::*;
use crate::dto::{ChannelSupportDto, RoutingImplementationDto};

fn channel(defaults: Vec<ChannelSupportDto>) -> ChannelDto {
    ChannelDto {
        id: "test".into(),
        display_name: "Test".into(),
        supports: Vec::new(),
        routing_defaults: defaults,
        login: None,
        provider_fields: Vec::new(),
        credential_fields: Vec::new(),
        endpoint_kinds: Vec::new(),
        traffic_policy: crate::dto::TrafficPolicyDto {
            request_headers: Vec::new(),
            response_headers: Vec::new(),
            request_query: Vec::new(),
        },
    }
}

fn route(implementation: RoutingImplementationDto) -> ChannelSupportDto {
    ChannelSupportDto {
        source: "openai".into(),
        target: "openai".into(),
        operation: "count_tokens".into(),
        target_operation: "count_tokens".into(),
        group: "count_tokens".into(),
        implementation,
    }
}

async fn provider(state: &TestState) -> i64 {
    state
        .store
        .insert_provider(&ProviderInput {
            name: "routing-provider".into(),
            label: None,
            channel: "test".into(),
            settings: serde_json::json!({}),
            credential_strategy: "round_robin".into(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .expect("insert provider")
}

#[tokio::test]
async fn delete_provider_reaches_non_rule_entity_handler() {
    let state = state().await;
    let id = state
        .store
        .insert_provider(&ProviderInput {
            name: "deletable-provider".into(),
            label: None,
            channel: "openai".into(),
            settings: serde_json::json!({}),
            credential_strategy: "round_robin".into(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .expect("insert provider");
    crate::seed_provider_rule_set(&state.store, id, "deletable-provider")
        .await
        .expect("seed provider rule set");
    seed_admin_key(&state).await;

    let response = crate::dispatch(
        &state,
        &admin_parts(Method::DELETE, &format!("/admin/api/providers/{id}")),
        Bytes::new(),
    )
    .await
    .expect("delete provider");
    assert_eq!(response.status(), StatusCode::OK);
    let snapshot = state.store.control_snapshot().await.unwrap();
    assert!(snapshot.providers.is_empty());
    assert!(snapshot.rule_sets.is_empty());
    assert!(snapshot.provider_rule_sets.is_empty());
    let audit = state.store.audit_events(1).await.unwrap();
    assert_eq!(audit[0].event.action, "providers.delete");
    assert_eq!(audit[0].event.target_id, Some(id));
    assert_eq!(audit[0].event.client_ip.as_deref(), Some("192.0.2.2"));
}

#[tokio::test]
async fn declared_local_route_is_seeded() {
    let state = state().await;
    let id = provider(&state).await;
    crate::seed_provider_defaults(
        &state.store,
        id,
        "routing-provider",
        &channel(vec![route(RoutingImplementationDto::Local)]),
    )
    .await
    .expect("seed defaults");

    let row = state
        .store
        .control_snapshot()
        .await
        .unwrap()
        .routing_rules
        .remove(0);
    assert_eq!(row.implementation, "local");
    assert_eq!(row.origin, "channel_default");
    let snapshot = state.store.control_snapshot().await.unwrap();
    assert_eq!(snapshot.rule_sets.len(), 1);
    assert_eq!(
        snapshot.rule_sets[0].description.as_deref(),
        Some(&*format!("gproxy:provider-default:{id}"))
    );
    assert!(snapshot.rules.is_empty());
    assert_eq!(snapshot.provider_rule_sets.len(), 1);
    assert_eq!(snapshot.provider_rule_sets[0].provider_id, id);
    assert_eq!(snapshot.provider_rule_sets[0].origin, "operator");
}

#[tokio::test]
async fn backfill_does_not_overwrite_operator_route() {
    let state = state().await;
    let id = provider(&state).await;
    state
        .store
        .insert_routing_rule(&gproxy_store::records::RoutingRuleInput {
            provider_id: id,
            operation: "count_tokens".into(),
            kind: "openai".into(),
            implementation: "unsupported".into(),
            dest_operation: None,
            dest_kind: None,
            sort_order: 7,
            enabled: false,
        })
        .await
        .expect("insert operator route");

    crate::backfill_provider_defaults(
        &state.store,
        &[channel(vec![route(RoutingImplementationDto::Local)])],
    )
    .await
    .expect("backfill defaults");

    let row = state
        .store
        .control_snapshot()
        .await
        .unwrap()
        .routing_rules
        .remove(0);
    assert_eq!(row.implementation, "unsupported");
    assert_eq!(row.origin, "operator");
    assert_eq!(row.sort_order, 7);
    assert!(!row.enabled);
}

#[tokio::test]
async fn backfill_refreshes_channel_owned_route_definition() {
    let state = state().await;
    let id = provider(&state).await;
    crate::seed_provider_defaults(
        &state.store,
        id,
        "routing-provider",
        &channel(vec![route(RoutingImplementationDto::Local)]),
    )
    .await
    .expect("seed defaults");

    crate::backfill_provider_defaults(
        &state.store,
        &[channel(vec![route(RoutingImplementationDto::Passthrough)])],
    )
    .await
    .expect("refresh defaults");

    let row = state
        .store
        .control_snapshot()
        .await
        .unwrap()
        .routing_rules
        .remove(0);
    assert_eq!(row.implementation, "passthrough");
    assert_eq!(row.origin, "channel_default");
    assert!(row.enabled);
}

#[tokio::test]
async fn embedded_default_prices_import_once_without_overwriting() {
    let state = state().await;
    seed_admin_key(&state).await;
    let provider_id = provider(&state).await;

    let response = crate::dispatch(
        &state,
        &admin_parts(Method::GET, "/admin/api/default-model-catalog"),
        Bytes::new(),
    )
    .await
    .expect("default model catalog");
    assert_eq!(response.status(), StatusCode::OK);
    let catalog: crate::dto::DefaultModelCatalogDto =
        serde_json::from_slice(response.body()).expect("catalog response");
    assert_eq!(catalog.models.len(), catalog.source.total_models);
    assert_eq!(
        catalog
            .models
            .iter()
            .filter(|model| model.pricing.is_some())
            .count(),
        catalog.source.priced_models
    );

    let body = Bytes::from(
        serde_json::to_vec(&crate::dto::ApplyDefaultModelPricesRequest {
            provider_id,
            model_ids: vec!["gpt-5.6-sol".into(), "claude-opus-5-20260801".into()],
        })
        .unwrap(),
    );
    let response = crate::dispatch(
        &state,
        &admin_parts(
            Method::POST,
            "/admin/api/default-model-catalog/apply-prices",
        ),
        body.clone(),
    )
    .await
    .expect("apply defaults");
    assert_eq!(response.status(), StatusCode::CREATED);
    let applied: crate::dto::ApplyDefaultModelPricesResponse =
        serde_json::from_slice(response.body()).unwrap();
    assert_eq!(applied.created, 2);
    assert_eq!(applied.skipped, 0);
    assert_eq!(applied.unmatched, 0);
    let snapshot = state.store.control_snapshot().await.unwrap();
    assert_eq!(snapshot.price_rules.len(), 2);
    assert!(snapshot.price_rates.len() >= 4);
    assert!(
        snapshot
            .price_rules
            .iter()
            .all(|rule| rule.provider_id == Some(provider_id))
    );
    assert!(
        snapshot
            .price_rules
            .iter()
            .any(|rule| rule.model_pattern == "gpt-5.6-sol")
    );

    let response = crate::dispatch(
        &state,
        &admin_parts(
            Method::POST,
            "/admin/api/default-model-catalog/apply-prices",
        ),
        body,
    )
    .await
    .expect("reapply defaults");
    assert_eq!(response.status(), StatusCode::OK);
    let applied: crate::dto::ApplyDefaultModelPricesResponse =
        serde_json::from_slice(response.body()).unwrap();
    assert_eq!(applied.created, 0);
    assert_eq!(applied.skipped, 2);
    assert_eq!(applied.unmatched, 0);
    assert_eq!(
        state
            .store
            .control_snapshot()
            .await
            .unwrap()
            .price_rules
            .len(),
        2
    );
    assert_eq!(
        state.store.audit_events(10).await.unwrap()[0].event.action,
        "default_prices.apply"
    );
}

#[tokio::test]
async fn untouched_embedded_prices_are_not_exported_as_operator_configuration() {
    let state = state().await;
    seed_admin_key(&state).await;
    let expected = crate::handlers::default_models::list()
        .and_then(|response| {
            serde_json::from_slice::<crate::dto::DefaultModelCatalogDto>(response.body())
                .map(|catalog| catalog.source.priced_models)
                .map_err(|error| crate::AdminError::Internal(error.to_string()))
        })
        .unwrap();
    assert_eq!(
        crate::seed_global_default_prices(&state.store)
            .await
            .unwrap(),
        expected
    );

    let response = crate::dispatch(
        &state,
        &admin_parts(Method::POST, "/admin/api/export"),
        Bytes::from_static(br#"{"include_secrets":false}"#),
    )
    .await
    .expect("configuration export");
    assert_eq!(response.status(), StatusCode::OK);
    let export: crate::dto::ConfigurationExportDto =
        serde_json::from_slice(response.body()).unwrap();
    assert!(export.data.price_rules.is_empty());
    assert!(export.data.price_rates.is_empty());
}
