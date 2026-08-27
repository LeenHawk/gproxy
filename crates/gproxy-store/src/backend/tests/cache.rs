use std::sync::Arc;

use gproxy_core::CacheBackend;

use super::libsql_store;

type SharedCache = Arc<dyn CacheBackend + Send + Sync>;

#[tokio::test]
async fn in_process_and_libsql_cache_operations_are_atomic() {
    let directory = tempfile::tempdir().expect("cache tempdir");
    let (store, _) = libsql_store(directory.path().join("cache.db"))
        .await
        .expect("libSQL store");
    let libsql = crate::LibsqlCache::connect(store)
        .await
        .expect("libSQL cache");
    for cache in [
        Arc::new(crate::InProcessCache::default()) as SharedCache,
        Arc::new(libsql) as SharedCache,
    ] {
        exercise_atomicity(cache).await;
    }
}

#[tokio::test]
#[ignore = "requires live Redis via GPROXY_TEST_REDIS_URL"]
async fn redis_cache_operations_are_atomic() {
    let url = std::env::var("GPROXY_TEST_REDIS_URL").expect("GPROXY_TEST_REDIS_URL");
    let cache = crate::RedisCache::connect(&url).await.expect("Redis cache");
    exercise_atomicity(Arc::new(cache)).await;
    let first = crate::RedisCache::connect(&url)
        .await
        .expect("first Redis instance");
    let second = crate::RedisCache::connect(&url)
        .await
        .expect("second Redis instance");
    let lease = format!("gproxy:test:{}:refresh", std::process::id());
    first.delete(&lease).await.expect("clear refresh lease");
    assert_eq!(first.incr(&lease, 1, None).await.expect("first lease"), 1);
    assert_eq!(second.incr(&lease, 1, None).await.expect("second lease"), 2);
    first.delete(&lease).await.expect("remove refresh lease");
}

#[tokio::test]
#[ignore = "requires live Upstash via GPROXY_TEST_UPSTASH_URL and GPROXY_TEST_UPSTASH_TOKEN"]
async fn upstash_cache_operations_are_atomic() {
    let url = std::env::var("GPROXY_TEST_UPSTASH_URL").expect("GPROXY_TEST_UPSTASH_URL");
    let token = std::env::var("GPROXY_TEST_UPSTASH_TOKEN").expect("GPROXY_TEST_UPSTASH_TOKEN");
    exercise_atomicity(Arc::new(crate::UpstashCache::new(url, token))).await;
}

async fn exercise_atomicity(cache: SharedCache) {
    let prefix = format!("gproxy:test:{}", std::process::id());
    let counter = format!("{prefix}:counter");
    cache.delete(&counter).await.expect("clear counter");
    let mut calls = Vec::new();
    for _ in 0..32 {
        let cache = cache.clone();
        let counter = counter.clone();
        calls.push(tokio::spawn(async move {
            cache.incr(&counter, 1, None).await.expect("increment")
        }));
    }
    let mut values = Vec::new();
    for call in calls {
        values.push(call.await.expect("increment task"));
    }
    values.sort_unstable();
    assert_eq!(values, (1..=32).collect::<Vec<_>>());

    let lease = format!("{prefix}:lease");
    cache.delete(&lease).await.expect("clear lease");
    let left = cache.compare_and_swap(&lease, None, Some(b"left".to_vec()), None);
    let right = cache.compare_and_swap(&lease, None, Some(b"right".to_vec()), None);
    let (left, right) = tokio::join!(left, right);
    assert_ne!(
        left.expect("left contender"),
        right.expect("right contender")
    );
    cache.delete(&counter).await.expect("remove counter");
    cache.delete(&lease).await.expect("remove lease");
}
