//! Staying on the same configuration as the durable state and the peers.

mod support;

use gproxy_sdk::{Cache, ConfigRevision, INVALIDATION_TOPIC, Invalidation};

fn changed(revision: u64) -> Vec<u8> {
    serde_json::to_vec(&Invalidation::ConfigurationChanged {
        revision: ConfigRevision(revision),
        scopes: Vec::new(),
    })
    .unwrap()
}

#[tokio::test]
async fn the_revision_poll_sees_a_write_nobody_announced() {
    let gproxy = support::sdk().await;
    let before = gproxy.revision();
    assert!(
        gproxy.tick().await.unwrap().is_none(),
        "an instance already at the durable revision has nothing to do"
    );

    // A peer's write, or this instance's own store used directly: no
    // notification is involved, which is exactly what the poll is for.
    gproxy.store().commit_revision(vec![]).await.unwrap();

    let outcome = gproxy
        .tick()
        .await
        .unwrap()
        .expect("the poll must find the durable revision ahead");
    assert!(outcome.published);
    assert_eq!(gproxy.revision().0, before.0 + 1);
    assert!(gproxy.tick().await.unwrap().is_none(), "already converged");
}

#[tokio::test]
async fn an_older_configuration_notification_does_not_reload() {
    let gproxy = support::sdk().await;
    let before = gproxy.revision();

    // The durable state is ahead, so any reload at all would be visible.
    gproxy.store().commit_revision(vec![]).await.unwrap();

    // A notification for a revision this instance already serves: a late or
    // duplicated delivery, not news.
    gproxy
        .cache()
        .publish(INVALIDATION_TOPIC, changed(before.0))
        .await
        .unwrap();

    assert!(gproxy.drain_notifications().await.unwrap().is_none());
    assert_eq!(
        gproxy.revision(),
        before,
        "an older revision must not cost a reload"
    );

    // The poll still converges; the ignored notification lost nothing.
    assert!(gproxy.tick().await.unwrap().is_some());
    assert_eq!(gproxy.revision().0, before.0 + 1);
}

#[tokio::test]
async fn a_newer_configuration_notification_reloads_without_polling() {
    let gproxy = support::sdk().await;
    let before = gproxy.revision();
    let commit = gproxy.store().commit_revision(vec![]).await.unwrap();

    gproxy
        .cache()
        .publish(
            INVALIDATION_TOPIC,
            changed(u64::try_from(commit.revision).unwrap()),
        )
        .await
        .unwrap();

    let outcome = gproxy
        .drain_notifications()
        .await
        .unwrap()
        .expect("a newer revision is news");
    assert!(outcome.published);
    assert_eq!(gproxy.revision().0, before.0 + 1);
}

#[tokio::test]
async fn an_unreadable_notification_reloads_rather_than_guesses() {
    let gproxy = support::sdk().await;
    let before = gproxy.revision();
    gproxy.store().commit_revision(vec![]).await.unwrap();

    gproxy
        .cache()
        .publish(INVALIDATION_TOPIC, b"not json".to_vec())
        .await
        .unwrap();

    assert!(gproxy.drain_notifications().await.unwrap().is_some());
    assert_eq!(gproxy.revision().0, before.0 + 1);
}

#[tokio::test]
async fn two_instances_on_one_database_converge_through_the_shared_cache() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gproxy.db");
    let path = path.to_str().unwrap();
    // One cache object shared by both handles is one deployment's cache.
    let cache: std::sync::Arc<dyn Cache> = std::sync::Arc::new(gproxy_sdk::MemoryCache::default());

    let a = support::sdk_on_file(path, cache.clone()).await;
    let b = support::sdk_on_file(path, cache).await;
    assert_eq!(a.revision(), b.revision());

    // A writes and announces, as the management families will.
    let commit = a.store().commit_revision(vec![]).await.unwrap();
    a.reload().await.unwrap();
    a.cache()
        .publish(
            INVALIDATION_TOPIC,
            changed(u64::try_from(commit.revision).unwrap()),
        )
        .await
        .unwrap();

    assert!(b.revision() < a.revision(), "B has not looked yet");
    b.tick().await.unwrap();
    assert_eq!(b.revision(), a.revision());
}
