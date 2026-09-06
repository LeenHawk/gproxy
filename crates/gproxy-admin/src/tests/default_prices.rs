use super::*;
use crate::dto::{ApplyDefaultModelPricesRequest, ApplyDefaultModelPricesResponse};

#[tokio::test]
async fn global_default_prices_select_then_import_all_preserves_scopes_and_rates() {
    let state = state().await;
    seed_admin_key(&state).await;
    let provider_id = provider(&state).await;
    let catalog = crate::handlers::default_models::list().unwrap();
    let catalog: crate::dto::DefaultModelCatalogDto =
        serde_json::from_slice(catalog.body()).unwrap();
    let models = catalog
        .models
        .iter()
        .filter(|model| model.pricing.is_some())
        .collect::<Vec<_>>();
    let source = models[0].pricing.as_ref().unwrap();
    let body = |provider_id, model_ids| {
        Bytes::from(
            serde_json::to_vec(&ApplyDefaultModelPricesRequest {
                provider_id,
                model_ids,
            })
            .unwrap(),
        )
    };
    let parts = admin_parts(
        Method::POST,
        "/admin/api/default-model-catalog/apply-prices",
    );
    for scope in [Some(provider_id), None] {
        let response = crate::dispatch(
            &state,
            &parts,
            body(scope, vec![models[0].model_id.clone()]),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let snapshot = state.store.control_snapshot().await.unwrap();
    assert_eq!(snapshot.price_rules.len(), 2);
    let global = snapshot
        .price_rules
        .iter()
        .find(|rule| rule.provider_id.is_none())
        .unwrap();
    assert_eq!(global.model_pattern, source.model_pattern);
    assert_eq!(global.priority, source.priority);
    assert_eq!(global.tiers, source.tiers);
    let rate = snapshot
        .price_rates
        .iter()
        .find(|rate| rate.rule_id == global.id)
        .unwrap();
    state
        .store
        .update_price_rate(
            rate.id,
            &gproxy_store::records::PriceRateInput {
                rule_id: global.id,
                metric: rate.metric.clone(),
                unit_size: rate.unit_size,
                price: "123.456".parse().unwrap(),
                conditions: rate.conditions.clone(),
                priority: rate.priority,
            },
        )
        .await
        .unwrap();
    let all = models
        .iter()
        .map(|model| model.model_id.clone())
        .collect::<Vec<_>>();
    let response = crate::dispatch(&state, &parts, body(None, all.clone()))
        .await
        .unwrap();
    let result: ApplyDefaultModelPricesResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(result.created, models.len() - 1);
    assert_eq!(result.skipped, 1);
    assert_eq!(result.unmatched, 0);
    let response = crate::dispatch(&state, &parts, body(None, all))
        .await
        .unwrap();
    let result: ApplyDefaultModelPricesResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(result.created, 0);
    assert_eq!(result.skipped, models.len());
    let snapshot = state.store.control_snapshot().await.unwrap();
    assert_eq!(snapshot.price_rules.len(), models.len() + 1);
    assert_eq!(
        snapshot
            .price_rates
            .iter()
            .find(|row| row.id == rate.id)
            .unwrap()
            .price
            .to_string(),
        "123.456"
    );
    let response = crate::dispatch(&state, &parts, body(None, vec!["unknown/model".into()]))
        .await
        .unwrap();
    let result: ApplyDefaultModelPricesResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(result.unmatched, 1);
    let response = crate::dispatch(&state, &parts, body(None, vec![]))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
