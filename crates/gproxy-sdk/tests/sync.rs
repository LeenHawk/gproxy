//! Staying on the same configuration as the durable state and the peers.

mod support;

use std::{sync::Arc, time::Duration};

use gproxy_cache::{
    CasOutcome, Counter, Entry, IncrementOutcome, Replacement, Subscription, Version,
};
use gproxy_sdk::{Cache, ConfigRevision, INVALIDATION_TOPIC, Invalidation, dto::ProviderWrite};

fn changed(revision: u64) -> Vec<u8> {
    serde_json::to_vec(&Invalidation::ConfigurationChanged {
        revision: ConfigRevision(revision),
        scopes: Vec::new(),
    })
    .unwrap()
}

/// A cache that accepts every publication and delivers none of them. This is
/// what a deployment whose notification transport is broken looks like from
/// the inside — the publish succeeds, the peers hear nothing — and it is the
/// case the revision poll exists for.
struct SilentCache(Arc<dyn Cache>);

#[async_trait::async_trait]
impl Cache for SilentCache {
    async fn get(&self, key: &str) -> gproxy_cache::Result<Option<Entry>> {
        self.0.get(key).await
    }
    async fn put(&self, key: &str, value: Vec<u8>, ttl: Duration) -> gproxy_cache::Result<Version> {
        self.0.put(key, value, ttl).await
    }
    async fn delete(&self, key: &str) -> gproxy_cache::Result<bool> {
        self.0.delete(key).await
    }
    async fn compare_exchange(
        &self,
        key: &str,
        expected: Option<Version>,
        replacement: Option<Replacement>,
    ) -> gproxy_cache::Result<CasOutcome> {
        self.0.compare_exchange(key, expected, replacement).await
    }
    async fn counter(&self, key: &str) -> gproxy_cache::Result<Option<Counter>> {
        self.0.counter(key).await
    }
    async fn increment(
        &self,
        key: &str,
        amount: u64,
        limit: u64,
        ttl: Duration,
    ) -> gproxy_cache::Result<IncrementOutcome> {
        self.0.increment(key, amount, limit, ttl).await
    }
    async fn decrement(
        &self,
        key: &str,
        generation: Version,
        amount: u64,
    ) -> gproxy_cache::Result<Option<Counter>> {
        self.0.decrement(key, generation, amount).await
    }
    async fn acquire_permit(
        &self,
        key: &str,
        limit: u32,
        ttl: Duration,
    ) -> gproxy_cache::Result<Option<Version>> {
        self.0.acquire_permit(key, limit, ttl).await
    }
    async fn renew_permit(
        &self,
        key: &str,
        owner: Version,
        ttl: Duration,
    ) -> gproxy_cache::Result<bool> {
        self.0.renew_permit(key, owner, ttl).await
    }
    async fn release_permit(&self, key: &str, owner: Version) -> gproxy_cache::Result<bool> {
        self.0.release_permit(key, owner).await
    }
    /// The whole point: accepted, and dropped on the floor.
    async fn publish(&self, _topic: &str, _payload: Vec<u8>) -> gproxy_cache::Result<()> {
        Ok(())
    }
    async fn subscribe(&self, topic: &str) -> gproxy_cache::Result<Box<dyn Subscription>> {
        self.0.subscribe(topic).await
    }
}

fn provider(id: &str) -> ProviderWrite {
    ProviderWrite {
        id: Some(id.to_owned()),
        name: id.to_owned(),
        channel: "test".into(),
        ..Default::default()
    }
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

#[tokio::test]
async fn a_peers_management_write_arrives_through_the_notification() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gproxy.db");
    let path = path.to_str().unwrap();
    let cache: Arc<dyn Cache> = Arc::new(gproxy_sdk::MemoryCache::default());

    let a = support::sdk_on_file(path, cache.clone()).await;
    let b = support::sdk_on_file(path, cache).await;
    let before = b.revision();

    // A real management write: one revision commit, A's own reload, then the
    // notification. Nothing about B is involved.
    a.manage()
        .providers()
        .create(provider("p-1"))
        .await
        .unwrap();
    assert!(a.core().snapshot().providers.contains_key("p-1"));
    assert!(
        !b.core().snapshot().providers.contains_key("p-1"),
        "B has not synchronized yet"
    );

    b.tick().await.unwrap();
    assert_eq!(b.revision(), a.revision());
    assert!(
        b.core().snapshot().providers.contains_key("p-1"),
        "B serves the row its peer wrote"
    );

    // A notification is enough on its own: no poll, no second query.
    a.manage()
        .providers()
        .create(provider("p-2"))
        .await
        .unwrap();
    assert!(b.drain_notifications().await.unwrap().is_some());
    assert!(b.core().snapshot().providers.contains_key("p-2"));

    // A late or duplicated delivery for a revision B already serves must not
    // cost a reload, even with a newer durable revision waiting to be found.
    let stale = b.revision();
    a.store().commit_revision(vec![]).await.unwrap();
    b.cache()
        .publish(INVALIDATION_TOPIC, changed(before.0))
        .await
        .unwrap();
    assert!(b.drain_notifications().await.unwrap().is_none());
    assert_eq!(b.revision(), stale, "an older revision is not news");

    // The poll still finds it; the ignored notification lost nothing.
    assert!(b.tick().await.unwrap().is_some());
    assert!(b.revision() > stale);
}

#[tokio::test]
async fn a_lost_notification_costs_a_poll_interval_and_nothing_else() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gproxy.db");
    let path = path.to_str().unwrap();
    // Both instances share one cache for everything except notifications,
    // which this one swallows.
    let shared: Arc<dyn Cache> = Arc::new(gproxy_sdk::MemoryCache::default());
    let silent: Arc<dyn Cache> = Arc::new(SilentCache(shared));

    let a = support::sdk_on_file(path, silent.clone()).await;
    let b = support::sdk_on_file(path, silent).await;

    a.manage()
        .providers()
        .create(provider("p-1"))
        .await
        .unwrap();
    assert!(a.core().snapshot().providers.contains_key("p-1"));

    assert!(
        b.drain_notifications().await.unwrap().is_none(),
        "nothing was delivered, because nothing was published"
    );
    assert!(b.revision() < a.revision());

    // The durable revision is the authority, and it did move.
    let outcome = b
        .sync_now()
        .await
        .unwrap()
        .expect("the poll finds what the notification lost");
    assert!(outcome.published);
    assert_eq!(b.revision(), a.revision());
    assert!(b.core().snapshot().providers.contains_key("p-1"));
    assert!(b.sync_now().await.unwrap().is_none(), "already converged");
}
