#![cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "memory", feature = "redis")
))]

use gproxy_cache::*;
use std::{sync::Arc, time::Duration};
const TTL: Duration = Duration::from_secs(30);

async fn values(a: Arc<dyn Cache>, b: Arc<dyn Cache>) {
    assert!(a.get("bytes").await.unwrap().is_none());
    let old = a.put("bytes", vec![0, 255, 1], TTL).await.unwrap();
    assert_eq!(b.get("bytes").await.unwrap().unwrap().value, [0, 255, 1]);
    assert_eq!(
        b.compare_exchange(
            "bytes",
            None,
            Some(Replacement {
                value: vec![],
                ttl: TTL
            })
        )
        .await
        .unwrap(),
        CasOutcome::Conflict
    );
    let CasOutcome::Applied(Some(new)) = b
        .compare_exchange(
            "bytes",
            Some(old),
            Some(Replacement {
                value: vec![],
                ttl: TTL,
            }),
        )
        .await
        .unwrap()
    else {
        panic!("CAS failed")
    };
    assert_ne!(old, new);
    assert_eq!(
        a.get("bytes").await.unwrap().unwrap().value,
        Vec::<u8>::new()
    );
    assert_eq!(
        a.compare_exchange("bytes", Some(old), None).await.unwrap(),
        CasOutcome::Conflict
    );
    assert_eq!(
        a.compare_exchange("bytes", Some(new), None).await.unwrap(),
        CasOutcome::Applied(None)
    );
    assert_eq!(
        a.compare_exchange("bytes", None, None).await.unwrap(),
        CasOutcome::Applied(None)
    );
    let expired = a
        .put("bytes", vec![1], Duration::from_millis(1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(b.get("bytes").await.unwrap().is_none());
    let fresh = b.put("bytes", vec![1], TTL).await.unwrap();
    assert_ne!(expired, fresh);
    assert_eq!(
        a.compare_exchange("bytes", Some(expired), None)
            .await
            .unwrap(),
        CasOutcome::Conflict
    );
    assert!(a.delete("bytes").await.unwrap());
    assert!(!b.delete("bytes").await.unwrap());
    assert!(matches!(
        a.put("invalid", vec![], Duration::ZERO).await,
        Err(CacheError::Invalid(_))
    ));
    assert!(a.get("invalid").await.unwrap().is_none());
    assert!(matches!(a.get("").await, Err(CacheError::Invalid(_))));
    a.put("domains", vec![42], TTL).await.unwrap();
    b.increment("domains", 1, 2, TTL).await.unwrap();
    assert_eq!(a.get("domains").await.unwrap().unwrap().value, [42]);
}
async fn counters(a: Arc<dyn Cache>, b: Arc<dyn Cache>) {
    assert_eq!(
        a.increment("too-large", 2, 1, TTL).await.unwrap(),
        IncrementOutcome::Limited { current: 0 }
    );
    assert!(b.counter("too-large").await.unwrap().is_none());
    let maximum = i64::MAX as u64;
    let IncrementOutcome::Applied(first) = a
        .increment("exact", maximum - 2, maximum, TTL)
        .await
        .unwrap()
    else {
        panic!()
    };
    let IncrementOutcome::Applied(last) = b.increment("exact", 2, maximum, TTL).await.unwrap()
    else {
        panic!()
    };
    assert_eq!(last.value, maximum);
    assert_eq!(first.generation, last.generation);
    assert_eq!(
        a.increment("exact", 1, maximum, TTL).await.unwrap(),
        IncrementOutcome::Limited { current: maximum }
    );
    assert_eq!(
        a.decrement("exact", last.generation, maximum)
            .await
            .unwrap()
            .unwrap()
            .value,
        0
    );
    assert!(matches!(
        a.decrement("exact", last.generation, 1).await,
        Err(CacheError::Underflow)
    ));
    assert_eq!(b.counter("exact").await.unwrap().unwrap().value, 0);
    assert!(matches!(
        a.increment("invalid", u64::MAX, maximum, TTL).await,
        Err(CacheError::Invalid(_))
    ));
    let IncrementOutcome::Applied(old) = a
        .increment("window", 1, 10, Duration::from_millis(200))
        .await
        .unwrap()
    else {
        panic!()
    };
    let IncrementOutcome::Applied(next) = b.increment("window", 1, 10, TTL).await.unwrap() else {
        panic!()
    };
    assert_eq!(old.generation, next.generation);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        a.counter("window").await.unwrap().is_none(),
        "increments must not extend TTL"
    );
    let IncrementOutcome::Applied(new) = b.increment("window", 1, 10, TTL).await.unwrap() else {
        panic!()
    };
    assert_ne!(new.generation, old.generation);
    assert!(
        a.decrement("window", old.generation, 1)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(a.counter("window").await.unwrap().unwrap().value, 1);
}
async fn permits(a: Arc<dyn Cache>, b: Arc<dyn Cache>) {
    let long = a.acquire_permit("pool", 2, TTL).await.unwrap().unwrap();
    let short = b
        .acquire_permit("pool", 2, Duration::from_millis(1))
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!a.renew_permit("pool", short, TTL).await.unwrap());
    assert!(
        b.renew_permit("pool", long, TTL).await.unwrap(),
        "short permit must not expire the long one"
    );
    let other = b.acquire_permit("pool", 2, TTL).await.unwrap().unwrap();
    assert!(a.acquire_permit("pool", 2, TTL).await.unwrap().is_none());
    assert!(!a.release_permit("pool", short).await.unwrap());
    assert!(a.release_permit("pool", other).await.unwrap());
    assert!(!a.release_permit("pool", other).await.unwrap());
    assert!(a.release_permit("pool", long).await.unwrap());
    let old = a
        .acquire_lease("refresh", Duration::from_millis(1))
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let new = b.acquire_lease("refresh", TTL).await.unwrap().unwrap();
    assert_ne!(old, new);
    assert!(!a.release_permit("refresh", old).await.unwrap());
    assert!(!a.renew_permit("refresh", old, TTL).await.unwrap());
    assert!(a.acquire_lease("refresh", TTL).await.unwrap().is_none());
    assert!(b.release_permit("refresh", new).await.unwrap());
}
async fn contention(a: Arc<dyn Cache>, b: Arc<dyn Cache>) {
    let mut jobs = tokio::task::JoinSet::new();
    for n in 0..32 {
        let cache = if n % 2 == 0 { a.clone() } else { b.clone() };
        jobs.spawn(async move {
            let cas = cache
                .compare_exchange(
                    "race",
                    None,
                    Some(Replacement {
                        value: vec![1],
                        ttl: TTL,
                    }),
                )
                .await
                .unwrap();
            let counter = cache.increment("race", 1, 7, TTL).await.unwrap();
            let permit = cache.acquire_permit("race", 3, TTL).await.unwrap();
            (cas, counter, permit)
        });
    }
    let (mut wins, mut increments, mut holders) = (0, 0, Vec::new());
    while let Some(result) = jobs.join_next().await {
        let (cas, counter, permit) = result.unwrap();
        wins += usize::from(matches!(cas, CasOutcome::Applied(_)));
        increments += usize::from(matches!(counter, IncrementOutcome::Applied(_)));
        holders.extend(permit);
    }
    assert_eq!((wins, increments, holders.len()), (1, 7, 3));
    assert_eq!(a.counter("race").await.unwrap().unwrap().value, 7);
    for holder in holders {
        b.release_permit("race", holder).await.unwrap();
    }
}
async fn notifications(a: Arc<dyn Cache>, b: Arc<dyn Cache>) {
    a.publish("changes", b"before-subscribe".to_vec())
        .await
        .unwrap();
    let mut sub = b.subscribe("changes").await.unwrap();
    assert_eq!(sub.recv().await.unwrap(), Notification::ResyncRequired);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), sub.recv())
            .await
            .is_err()
    );
    a.publish("changes", b"revision:2".to_vec()).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap(),
        Notification::Message(b"revision:2".to_vec())
    );
    for n in 0..64 {
        a.publish("changes", vec![n]).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        sub.recv().await.unwrap(),
        Notification::ResyncRequired,
        "bounded slow consumer must detect a gap"
    );
    drop(sub);
    let mut next = b.subscribe("changes").await.unwrap();
    assert_eq!(next.recv().await.unwrap(), Notification::ResyncRequired);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), next.recv())
            .await
            .is_err()
    );
    a.publish("other", vec![9]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), next.recv())
            .await
            .is_err()
    );
}

#[cfg(feature = "memory")]
fn memory() -> (Arc<dyn Cache>, Arc<dyn Cache>) {
    let options = MemoryOptions {
        limits: Limits {
            notification_capacity: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    let cache = MemoryCache::new(options).unwrap();
    (Arc::new(cache.clone()), Arc::new(cache))
}
#[cfg(feature = "redis")]
async fn redis_pair(name: &str) -> (Arc<dyn Cache>, Arc<dyn Cache>) {
    let url = std::env::var("GPROXY_CACHE_REDIS_URL")
        .expect("set GPROXY_CACHE_REDIS_URL to a test Redis server");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut options =
        RedisOptions::new(format!("cache-test-{}-{name}-{nonce}", std::process::id()));
    options.limits.notification_capacity = 2;
    (
        Arc::new(RedisCache::connect(&url, options.clone()).await.unwrap()),
        Arc::new(RedisCache::connect(&url, options).await.unwrap()),
    )
}
macro_rules! test_contract {
    ($memory:ident, $redis:ident, $contract:ident) => {
        #[cfg(feature = "memory")]
        #[tokio::test]
        async fn $memory() {
            let (a, b) = memory();
            $contract(a, b).await;
        }
        #[cfg(feature = "redis")]
        #[tokio::test]
        #[ignore = "requires an explicitly configured test Redis server"]
        async fn $redis() {
            let (a, b) = redis_pair(stringify!($contract)).await;
            $contract(a, b).await;
        }
    };
}
test_contract!(memory_values, redis_values, values);
test_contract!(memory_counters, redis_counters, counters);
test_contract!(memory_permits, redis_permits, permits);
test_contract!(memory_contention, redis_contention, contention);
test_contract!(memory_notifications, redis_notifications, notifications);

#[cfg(feature = "memory")]
#[tokio::test]
async fn memory_capacity_never_evicts_a_live_lease_and_drop_closes_subscription() {
    let options = MemoryOptions {
        max_keys: 1,
        max_topics: 1,
        ..Default::default()
    };
    let cache = MemoryCache::new(options).unwrap();
    let owner = cache.acquire_lease("live", TTL).await.unwrap().unwrap();
    assert!(matches!(
        cache.put("other", vec![], TTL).await,
        Err(CacheError::Capacity)
    ));
    assert!(cache.acquire_lease("live", TTL).await.unwrap().is_none());
    assert!(cache.release_permit("live", owner).await.unwrap());
    cache.put("other", vec![], TTL).await.unwrap();
    let mut sub = cache.subscribe("one").await.unwrap();
    assert!(matches!(
        cache.subscribe("two").await,
        Err(CacheError::Capacity)
    ));
    assert_eq!(sub.recv().await.unwrap(), Notification::ResyncRequired);
    drop(cache);
    assert!(matches!(sub.recv().await, Err(CacheError::Closed)));
}

#[cfg(feature = "memory")]
#[tokio::test]
async fn independent_memory_instances_are_isolated_and_limits_preflight_writes() {
    let one = MemoryCache::default();
    let two = MemoryCache::default();
    one.put("same", vec![1], TTL).await.unwrap();
    assert!(two.get("same").await.unwrap().is_none());
    let bounded = MemoryCache::new(MemoryOptions {
        limits: Limits {
            max_value_bytes: 2,
            max_key_bytes: 2,
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(
        bounded.put("a", vec![0; 3], TTL).await,
        Err(CacheError::Invalid(_))
    ));
    assert!(bounded.get("a").await.unwrap().is_none());
    assert!(matches!(
        bounded.put("abc", vec![], TTL).await,
        Err(CacheError::Invalid(_))
    ));
}
