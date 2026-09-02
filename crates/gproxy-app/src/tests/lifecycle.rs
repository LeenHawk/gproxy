use serde_json::json;

#[tokio::test]
async fn fresh_instance_loads_global_prices_once() {
    let directory = tempfile::tempdir().unwrap();
    let config = || super::test_config(directory.path(), crate::MasterKeyConfig::new(None));
    let app = crate::App::start(config()).await.unwrap();
    let snapshot = app.inner.host.services.control.current();
    assert_eq!(snapshot.price_rules.len(), 493);
    assert!(
        snapshot
            .price_rules
            .iter()
            .all(|rule| rule.provider_id.is_none())
    );
    drop(app);

    let app = crate::App::start(config()).await.unwrap();
    assert_eq!(
        app.inner.host.services.control.current().price_rules.len(),
        493
    );
}

#[tokio::test]
async fn shared_invalidation_refreshes_the_control_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let app = crate::App::start(super::test_config(
        directory.path(),
        crate::MasterKeyConfig::new(None),
    ))
    .await
    .unwrap();
    app.inner
        .host
        .services
        .store
        .set_setting(&gproxy_store::records::SettingInput {
            key: gproxy_store::records::INSTANCE_NAME.into(),
            value: json!("remote-instance"),
        })
        .await
        .unwrap();
    crate::invalidation::bump(&app.inner.host.services.cache)
        .await
        .unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while app.instance_name() != "remote-instance" {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("shared invalidation was not consumed");
}
