#![cfg(not(target_arch = "wasm32"))]
//! Keeping the identity snapshot in step: the mechanism, its guards, and the
//! question a single-node deployment turns on.
//!
//! The bug these were written against is that nothing drove `App::refresh()`
//! after startup. An identity write landed in the database, bumped
//! `settings.config_revision` and notified the peers, and was then invisible
//! to the very process that made it until it restarted — so a key created in
//! the console could not authenticate and a permission change did not take
//! effect.
//!
//! # Does a publish reach a subscriber in the same process?
//!
//! Yes, for both supplied caches, and
//! [`a_publish_reaches_a_subscriber_in_the_same_process`] is the proof for
//! `MemoryCache`: `publish` is a `tokio::sync::broadcast` send, which fans out
//! to every receiver including the ones this process made. Redis does the same
//! over its own connection.
//!
//! That is not the same as being in time. The broadcast is synchronous and the
//! Redis round trip is not, and neither the background loop nor a `tick`
//! happens between a write returning and the caller's next request arriving.
//! So the notification is convergence, never read-your-writes: only the
//! durable revision poll on the host's write path can promise a caller that
//! the thing it just created exists — which is what `App::sync_now` is and why
//! both hosts call it there.

mod support;

use std::{sync::Arc, time::Duration};

use gproxy_app::{App, AppConfig, Operations, SyncMode, dto::ApiKeyWrite};
use gproxy_cache::{Cache, MemoryCache};
use gproxy_core::{ConfigRevision, Invalidation, keys};
use gproxy_sdk::{Gproxy, GproxyBuilder};
use sea_orm::{ConnectionTrait, DatabaseConnection};

/// An instance that keeps a subscription of its own, which is what a host
/// without a background loop — an edge isolate, a test — asks for.
async fn manual(app: &Arc<App<DatabaseConnection>>) {
    app.start_sync(SyncMode::Manual, Duration::from_secs(30))
        .await;
}

/// Announce a revision on the topic, as a peer's write would.
async fn announce(cache: &Arc<dyn Cache>, revision: i64, scopes: &[&str]) {
    let payload = Invalidation::ConfigurationChanged {
        revision: ConfigRevision(u64::try_from(revision).unwrap()),
        scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    cache
        .publish(
            keys::INVALIDATION_TOPIC,
            serde_json::to_vec(&payload).unwrap(),
        )
        .await
        .unwrap();
}

/// Whether the snapshot in force knows this key. The authenticator's own
/// index, not a database read — the whole point is what the snapshot holds.
fn knows(app: &App<DatabaseConnection>, key: &str) -> bool {
    app.data().keys.lookup(&support::digest_of(key)).is_some()
}

// ------------------------------------------------------ read your writes --

#[tokio::test]
async fn a_key_written_here_authenticates_against_this_instance_at_once() {
    let (app, _client) = support::app().await;
    let app = Arc::new(app);
    support::person(app.gproxy(), "root", "admin").await;
    support::publish(&app).await;
    manual(&app).await;

    // The write goes through the real family, so it commits a revision and
    // notifies exactly as the console's would.
    let data = app.data();
    let created = Operations::new(app.gproxy(), &data, app.config())
        .api_keys()
        .create(ApiKeyWrite {
            user_id: "root".into(),
            name: "minted".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    drop(data);

    // This is the bug, stated: the row is durable, the revision moved, and the
    // snapshot this process serves has never heard of it.
    assert!(
        !knows(&app, &created.token),
        "the write does not republish on its own; that is the contract"
    );

    // …and this is the host's half of the contract.
    let published = app.sync_now().await.unwrap();
    assert!(published.is_some(), "the durable revision moved");
    assert!(knows(&app, &created.token), "the key authenticates now");
}

#[tokio::test]
async fn the_revision_poll_converges_with_no_notification_at_all() {
    // No `start_sync`: no subscription, no loop, nothing but the durable
    // number. This is what every deployment falls back to when the cache
    // refuses the topic, and it has to be enough on its own.
    let (app, _client) = support::app().await;
    support::person(app.gproxy(), "root", "admin").await;
    support::api_key(app.gproxy(), "k-root", "root", None, None).await;
    app.gproxy().store().commit_revision(vec![]).await.unwrap();

    assert!(
        app.drain_notifications().await.unwrap().is_none(),
        "there is nothing subscribed to drain"
    );
    assert!(app.sync_now().await.unwrap().is_some());
    assert!(knows(&app, "k-root"));
}

// ------------------------------------------------------------ the guards --

#[tokio::test]
async fn a_revision_that_went_backwards_is_ignored() {
    let (app, _client) = support::app().await;
    let app = Arc::new(app);
    support::publish(&app).await;
    manual(&app).await;
    let served = app.snapshot().revision();

    // A row that is durable but whose revision nobody bumped. Only a reload
    // can reveal it, so it is how a test sees whether one happened.
    support::person(app.gproxy(), "root", "admin").await;
    support::api_key(app.gproxy(), "k-root", "root", None, None).await;

    let cache = app.gproxy().cache().clone();
    for revision in [served - 1, served] {
        announce(&cache, revision, &["keys"]).await;
        assert_eq!(
            app.drain_notifications().await.unwrap(),
            None,
            "revision {revision} is not ahead of {served}"
        );
    }
    assert_eq!(app.snapshot().revision(), served);
    assert!(!knows(&app, "k-root"), "nothing reloaded");

    // The same notification, once the revision really is ahead.
    let ahead = app
        .gproxy()
        .store()
        .commit_revision(vec![])
        .await
        .unwrap()
        .revision;
    announce(&cache, ahead, &["keys"]).await;
    assert_eq!(app.drain_notifications().await.unwrap(), Some(ahead));
    assert!(knows(&app, "k-root"));
}

#[tokio::test]
async fn a_notification_about_the_engines_own_families_does_not_rebuild_identity() {
    let (app, _client) = support::app().await;
    let app = Arc::new(app);
    support::publish(&app).await;
    manual(&app).await;

    support::person(app.gproxy(), "root", "admin").await;
    support::api_key(app.gproxy(), "k-root", "root", None, None).await;
    let ahead = app
        .gproxy()
        .store()
        .commit_revision(vec![])
        .await
        .unwrap()
        .revision;

    let cache = app.gproxy().cache().clone();
    // Routing rows are the engine's view and feed no part of `AppData`.
    announce(&cache, ahead, &["routing", "pricing"]).await;
    assert_eq!(app.drain_notifications().await.unwrap(), None);
    assert!(!knows(&app, "k-root"));

    // A scope this build has never heard of is not a reason to guess.
    announce(&cache, ahead, &["something-from-next-year"]).await;
    assert_eq!(app.drain_notifications().await.unwrap(), Some(ahead));
    assert!(knows(&app, "k-root"));
}

#[tokio::test]
async fn an_unreadable_payload_refreshes_rather_than_guesses() {
    let (app, _client) = support::app().await;
    let app = Arc::new(app);
    support::publish(&app).await;
    manual(&app).await;

    support::person(app.gproxy(), "root", "admin").await;
    support::api_key(app.gproxy(), "k-root", "root", None, None).await;
    app.gproxy().store().commit_revision(vec![]).await.unwrap();

    app.gproxy()
        .cache()
        .publish(keys::INVALIDATION_TOPIC, b"{not json at all".to_vec())
        .await
        .unwrap();
    assert!(app.drain_notifications().await.unwrap().is_some());
    assert!(knows(&app, "k-root"));
}

#[tokio::test]
async fn a_failed_refresh_leaves_the_previous_snapshot_serving() {
    let (app, _client) = support::app().await;
    support::person(app.gproxy(), "root", "admin").await;
    support::api_key(app.gproxy(), "k-root", "root", None, None).await;
    support::publish(&app).await;
    let served = app.snapshot().revision();
    assert!(knows(&app, "k-root"));

    // The revision moves, and then the read that would follow it cannot
    // succeed. The settings row survives, so the poll still finds a number to
    // chase; it is the identity load that fails.
    app.gproxy().store().commit_revision(vec![]).await.unwrap();
    app.gproxy()
        .store()
        .connection()
        .execute_unprepared("DROP TABLE users")
        .await
        .unwrap();

    assert!(app.sync_now().await.is_err());
    assert_eq!(app.snapshot().revision(), served, "still the old revision");
    assert!(
        knows(&app, "k-root"),
        "a failed reload must not empty the instance"
    );
}

// ---------------------------------------------------------- self-delivery --

#[tokio::test]
async fn a_publish_reaches_a_subscriber_in_the_same_process() {
    let (app, _client) = support::app().await;
    let app = Arc::new(app);
    support::person(app.gproxy(), "root", "admin").await;
    support::publish(&app).await;
    manual(&app).await;

    // Nothing announces this but the write itself, through the same
    // `MemoryCache` this instance subscribed to.
    let data = app.data();
    let created = Operations::new(app.gproxy(), &data, app.config())
        .api_keys()
        .create(ApiKeyWrite {
            user_id: "root".into(),
            name: "minted".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    drop(data);

    assert!(
        app.drain_notifications().await.unwrap().is_some(),
        "an in-process publish is delivered to an in-process subscriber"
    );
    assert!(knows(&app, &created.token));
}

// -------------------------------------------------------- two instances --

/// One SQLite file, one shared cache, two assembled instances — a deployment
/// with two processes, minus the processes.
async fn peer(path: &str, cache: Arc<dyn Cache>, first: bool) -> Arc<App<DatabaseConnection>> {
    let gproxy: Gproxy<DatabaseConnection> = GproxyBuilder::sqlite(path)
        .await
        .unwrap()
        .plaintext_secrets()
        .without_default_channels()
        // The schema is one writer's job; the second instance opens what the
        // first created, exactly as a second process would.
        .sync_schema(first)
        .cache(cache)
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();
    let app = Arc::new(App::new(gproxy, AppConfig::default()));
    app.reload_all().await.unwrap();
    manual(&app).await;
    app
}

#[tokio::test]
async fn a_peers_write_reaches_the_other_instance_by_notification_and_by_poll() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gproxy.db");
    let path = path.to_str().unwrap();
    let cache: Arc<dyn Cache> = Arc::new(MemoryCache::default());

    let a = peer(path, cache.clone(), true).await;
    let b = peer(path, cache.clone(), false).await;

    support::person(a.gproxy(), "root", "admin").await;
    support::publish(&a).await;
    assert!(b.sync_now().await.unwrap().is_some(), "b catches up first");

    // A writes through the real family: one revision commit, one notification.
    let data = a.data();
    let created = Operations::new(a.gproxy(), &data, a.config())
        .api_keys()
        .create(ApiKeyWrite {
            user_id: "root".into(),
            name: "from-a".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    drop(data);
    a.sync_now().await.unwrap();

    // The fast path: b was told, and b had not read anything.
    assert!(b.drain_notifications().await.unwrap().is_some());
    assert!(knows(&b, &created.token), "b serves a's key");

    // The slow path: a second write, and b is only ever asked the durable
    // number — which is what happens when a notification is lost.
    let data = a.data();
    let second = Operations::new(a.gproxy(), &data, a.config())
        .api_keys()
        .create(ApiKeyWrite {
            user_id: "root".into(),
            name: "from-a-again".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    drop(data);
    assert!(b.sync_now().await.unwrap().is_some());
    assert!(knows(&b, &second.token));
}
