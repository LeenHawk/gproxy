#![cfg(not(target_arch = "wasm32"))]

use gproxy_cache::{Cache, CacheError, CasOutcome, IncrementOutcome, Notification, Replacement};
use gproxy_store::{Store, StoreCache, operations::counted::CountedCharge};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend};
use std::{sync::Arc, time::Duration};

async fn database() -> Arc<Store<DatabaseConnection>> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    Arc::new(Store::new(db))
}

fn ttl(secs: u64) -> Duration {
    Duration::from_secs(secs)
}

#[tokio::test]
async fn values_are_versioned_expire_and_cas_atomically() {
    let store = database().await;
    let a = StoreCache::new(store.clone());
    let b = StoreCache::new(store.clone());
    assert!(a.get("k").await.unwrap().is_none());
    let v1 = a.put("k", b"one".to_vec(), ttl(60)).await.unwrap();
    let entry = b.get("k").await.unwrap().unwrap();
    assert_eq!(entry.value, b"one");
    assert_eq!(entry.version, v1);

    // CAS from another handle with the right version wins; the stale one loses.
    let CasOutcome::Applied(Some(v2)) = b
        .compare_exchange(
            "k",
            Some(v1),
            Some(Replacement {
                value: b"two".to_vec(),
                ttl: ttl(60),
            }),
        )
        .await
        .unwrap()
    else {
        panic!("applied");
    };
    assert_eq!(
        a.compare_exchange(
            "k",
            Some(v1),
            Some(Replacement {
                value: b"stale".to_vec(),
                ttl: ttl(60),
            }),
        )
        .await
        .unwrap(),
        CasOutcome::Conflict
    );
    assert_eq!(a.get("k").await.unwrap().unwrap().value, b"two");
    // Expecting absence fails while live; a CAS delete with the version lands.
    assert_eq!(
        a.compare_exchange("k", None, None).await.unwrap(),
        CasOutcome::Conflict
    );
    assert_eq!(
        b.compare_exchange("k", Some(v2), None).await.unwrap(),
        CasOutcome::Applied(None)
    );
    assert!(a.get("k").await.unwrap().is_none());
    // Absent expected + replacement creates.
    assert!(matches!(
        a.compare_exchange(
            "k",
            None,
            Some(Replacement {
                value: b"new".to_vec(),
                ttl: ttl(60)
            })
        )
        .await
        .unwrap(),
        CasOutcome::Applied(Some(_))
    ));
    // A one-millisecond TTL is gone after a short wait; the row is then absent
    // for reads and can be recreated with `expected = None`.
    a.put("short", b"x".to_vec(), Duration::from_millis(1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(a.get("short").await.unwrap().is_none());
    assert!(
        !a.delete("short").await.unwrap(),
        "expired rows do not count as deleted"
    );
    assert!(matches!(
        b.compare_exchange(
            "short",
            None,
            Some(Replacement {
                value: b"again".to_vec(),
                ttl: ttl(60)
            })
        )
        .await
        .unwrap(),
        CasOutcome::Applied(Some(_))
    ));
    assert!(matches!(
        a.put("", b"x".to_vec(), ttl(1)).await,
        Err(CacheError::Invalid(_))
    ));
}

#[tokio::test]
async fn many_values_are_read_in_one_call_as_each_would_be_alone() {
    let store = database().await;
    let a = StoreCache::new(store.clone());
    let b = StoreCache::new(store.clone());
    let one = a.put("one", b"1".to_vec(), ttl(60)).await.unwrap();
    a.put("gone", b"x".to_vec(), Duration::from_millis(1))
        .await
        .unwrap();
    a.put("three", b"3".to_vec(), ttl(60)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let keys = ["three", "missing", "gone", "one", "one"].map(String::from);
    let entries = b.get_many(&keys).await.unwrap();
    let values: Vec<_> = entries
        .iter()
        .map(|e| e.as_ref().map(|e| e.value.clone()))
        .collect();
    assert_eq!(
        values,
        [
            Some(b"3".to_vec()),
            None,
            None,
            Some(b"1".to_vec()),
            Some(b"1".to_vec())
        ],
        "in the order asked, duplicates included, expired rows absent"
    );
    assert_eq!(entries[3].as_ref().unwrap().version, one);
    assert!(b.get_many(&[]).await.unwrap().is_empty());
    assert!(matches!(
        b.get_many(&[String::new()]).await,
        Err(CacheError::Invalid(_))
    ));
}

#[tokio::test]
async fn counters_respect_limits_generations_and_expiry() {
    let store = database().await;
    let a = StoreCache::new(store.clone());
    let b = StoreCache::new(store.clone());
    let IncrementOutcome::Applied(first) = a.increment("c", 2, 5, ttl(60)).await.unwrap() else {
        panic!("applied");
    };
    assert_eq!(first.value, 2);
    let IncrementOutcome::Applied(second) = b.increment("c", 3, 5, ttl(60)).await.unwrap() else {
        panic!("applied");
    };
    assert_eq!(second.value, 5);
    assert_eq!(second.generation, first.generation, "same window");
    assert_eq!(
        a.increment("c", 1, 5, ttl(60)).await.unwrap(),
        IncrementOutcome::Limited { current: 5 }
    );
    let after = b
        .decrement("c", first.generation, 4)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.value, 1);
    assert!(matches!(
        a.decrement("c", first.generation, 4).await,
        Err(CacheError::Underflow)
    ));
    let other = gproxy_cache::Version::from_bytes([9; 32]);
    assert!(
        a.decrement("c", other, 1).await.unwrap().is_none(),
        "a foreign generation touches nothing"
    );
    assert_eq!(a.counter("c").await.unwrap().unwrap().value, 1);
    // A new window after expiry starts a new generation.
    a.increment("w", 1, 5, Duration::from_millis(1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(a.counter("w").await.unwrap().is_none());
    let IncrementOutcome::Applied(fresh) = b.increment("w", 1, 5, ttl(60)).await.unwrap() else {
        panic!("applied");
    };
    assert_eq!(fresh.value, 1);
}

#[tokio::test]
async fn permits_are_bounded_renewable_and_released() {
    let store = database().await;
    let a = StoreCache::new(store.clone());
    let b = StoreCache::new(store.clone());
    let lease = a.acquire_lease("lease", ttl(60)).await.unwrap().unwrap();
    assert!(b.acquire_lease("lease", ttl(60)).await.unwrap().is_none());
    assert!(a.renew_permit("lease", lease, ttl(60)).await.unwrap());
    assert!(a.release_permit("lease", lease).await.unwrap());
    assert!(!a.release_permit("lease", lease).await.unwrap());
    let second = b.acquire_lease("lease", ttl(60)).await.unwrap();
    assert!(second.is_some());
    // Two permits under a limit of two, the third refused; expiry frees them.
    let p1 = a
        .acquire_permit("pool", 2, Duration::from_millis(200))
        .await
        .unwrap();
    let p2 = a
        .acquire_permit("pool", 2, Duration::from_millis(200))
        .await
        .unwrap();
    assert!(p1.is_some() && p2.is_some());
    assert!(
        a.acquire_permit("pool", 2, ttl(60))
            .await
            .unwrap()
            .is_none()
    );
    // The two short permits expire; a generous margin keeps this deterministic
    // on a loaded machine (a 1 ms TTL raced the third acquire above).
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        b.acquire_permit("pool", 2, ttl(60))
            .await
            .unwrap()
            .is_some()
    );
    // Notifications: the initial resync marker, then nothing.
    let mut subscription = a.subscribe("topic").await.unwrap();
    assert!(matches!(
        subscription.recv().await.unwrap(),
        Notification::ResyncRequired
    ));
    a.publish("topic", b"ignored".to_vec()).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), subscription.recv())
            .await
            .is_err(),
        "no transport: nothing else arrives"
    );
}

#[tokio::test]
async fn counted_windows_charge_atomically_up_to_the_limit() {
    let store = database().await;
    let charge = |amount: i64| CountedCharge {
        credential_id: "c".into(),
        dimension: "hourly".into(),
        window_start_ms: 1_000,
        window_end_ms: 3_601_000,
        amount,
        limit: 5,
    };
    let windows = store.counted_windows();
    let first = windows.charge_many(vec![charge(2)]).await.unwrap()[0];
    assert!(first.applied);
    assert_eq!(first.used, 2);
    let second = windows
        .charge_many(vec![charge(3), charge(1)])
        .await
        .unwrap();
    assert!(second[0].applied && second[0].used == 5);
    assert!(!second[1].applied, "over the limit: refused");
    assert_eq!(second[1].used, 5);
    let refused = windows.charge_many(vec![charge(1)]).await.unwrap()[0];
    assert!(!refused.applied);
    let live = windows.live(2_000).await.unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].used, 5);
    assert!(windows.live(4_000_000).await.unwrap().is_empty());
    let oversized = windows
        .charge_many(vec![CountedCharge {
            dimension: "big".into(),
            amount: 9,
            ..charge(0)
        }])
        .await
        .unwrap()[0];
    assert!(
        !oversized.applied,
        "a first charge over the limit opens the window at zero"
    );
    assert_eq!(oversized.used, 0);
}
