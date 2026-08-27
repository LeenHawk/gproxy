use super::*;

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
    seed_admin_key(&state).await;

    let response = crate::dispatch(
        &state,
        &admin_parts(Method::DELETE, &format!("/admin/providers/{id}")),
        Bytes::new(),
    )
    .await
    .expect("delete provider");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        state
            .store
            .control_snapshot()
            .await
            .unwrap()
            .providers
            .is_empty()
    );
    let audit = state.store.audit_events(1).await.unwrap();
    assert_eq!(audit[0].event.action, "providers.delete");
    assert_eq!(audit[0].event.target_id, Some(id));
}
