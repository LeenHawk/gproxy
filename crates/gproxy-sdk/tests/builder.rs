//! Assembly: what a built handle guarantees before anything is configured.

mod support;

use gproxy_sdk::{GproxyBuilder, LoginMode, SdkError, SyncMode};

#[tokio::test]
async fn builds_on_an_in_memory_database() {
    let gproxy = support::sdk().await;

    // The settings row is the builder's own precondition: without it no
    // management write could allocate a revision.
    let settings = gproxy
        .store()
        .settings()
        .get()
        .await
        .unwrap()
        .expect("the builder creates the global settings row");
    assert_eq!(settings.id, 1);
    assert!(settings.config_revision >= 0);

    // The snapshot the handle serves is the durable revision, not a guess.
    assert_eq!(
        gproxy.revision().0,
        u64::try_from(settings.config_revision).unwrap()
    );
    // Routing is published from the same reload, at the same revision.
    assert_eq!(gproxy.routing().revision, gproxy.revision());
    assert!(!gproxy.instance_id().is_empty());
}

#[tokio::test]
async fn lists_only_the_registered_channels() {
    let gproxy = support::sdk().await;
    let channels = gproxy.channels();
    assert_eq!(
        channels.iter().map(|c| c.id).collect::<Vec<_>>(),
        ["test"],
        "without_default_channels leaves exactly what was registered"
    );

    // The descriptor is derived from the channel's capability accessors, so a
    // management UI cannot be shown a login mode the channel does not have.
    let descriptor = &channels[0];
    assert!(
        descriptor
            .login_modes
            .contains(&LoginMode::AuthorizationCode)
    );
    assert!(descriptor.login_modes.contains(&LoginMode::DeviceCode));
    assert!(descriptor.login_modes.contains(&LoginMode::Cookie));
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.services);
}

#[tokio::test]
async fn reload_publishes_only_a_newer_revision() {
    let gproxy = support::sdk().await;
    let before = gproxy.revision();

    let outcome = gproxy.reload().await.unwrap();
    assert!(
        !outcome.published,
        "re-reading the same revision must not republish"
    );
    assert_eq!(outcome.active_revision, before);

    let commit = gproxy.store().commit_revision(vec![]).await.unwrap();
    let outcome = gproxy.reload().await.unwrap();
    assert!(outcome.published);
    assert_eq!(
        outcome.active_revision.0,
        u64::try_from(commit.revision).unwrap()
    );
    assert!(outcome.active_revision > before);
    assert_eq!(gproxy.routing().revision, outcome.active_revision);
}

#[tokio::test]
async fn a_build_without_a_secret_codec_is_refused() {
    let error = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .without_default_channels()
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .expect_err("a codec is never chosen implicitly");
    assert!(matches!(error, SdkError::Invalid(_)), "{error}");
    assert_eq!(error.status_code(), 400);
}

#[tokio::test]
async fn a_duplicate_channel_id_is_a_conflict() {
    let error = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .plaintext_secrets()
        .without_default_channels()
        .channel(std::sync::Arc::new(support::TestChannel::default()))
        .channel(std::sync::Arc::new(support::TestChannel::default()))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .expect_err("two channels cannot answer to one id");
    assert!(matches!(error, SdkError::Conflict(_)), "{error}");
    assert_eq!(error.status_code(), 409);
}
