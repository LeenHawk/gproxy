#![cfg(all(feature = "redis", not(target_arch = "wasm32")))]

use gproxy_cache::*;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires an explicitly configured anonymous TCP test Redis"]
async fn redis_disconnect_is_visible_and_new_subscription_requires_resync() {
    let url = std::env::var("GPROXY_CACHE_REDIS_URL").expect("set a local test Redis URL");
    let client = redis::Client::open(url.as_str()).unwrap();
    let info = client.get_connection_info();
    assert!(
        info.redis_settings().password().is_none() && info.redis_settings().username().is_none()
    );
    let redis::ConnectionAddr::Tcp(host, port) = info.addr() else {
        panic!("test requires TCP Redis")
    };
    let target = (host.clone(), *port);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!(
        "redis://127.0.0.1:{}/{}",
        listener.local_addr().unwrap().port(),
        info.redis_settings().db()
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let proxy = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                result = listener.accept() => {
                    let (mut incoming, _) = result.unwrap();
                    let target = target.clone();
                    connections.spawn(async move {
                        let mut outgoing = tokio::net::TcpStream::connect(target).await.unwrap();
                        let _ = tokio::io::copy_bidirectional(&mut incoming, &mut outgoing).await;
                    });
                }
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut options = RedisOptions::new(format!("disconnect-{nonce}"));
    options.connection_timeout = Duration::from_millis(500);
    options.response_timeout = Duration::from_millis(500);
    let first = RedisCache::connect(&proxy_url, options.clone())
        .await
        .unwrap();
    let second = RedisCache::connect(&url, options).await.unwrap();
    first
        .put("state", vec![1], Duration::from_secs(30))
        .await
        .unwrap();
    let mut old = first.subscribe("changes").await.unwrap();
    assert_eq!(old.recv().await.unwrap(), Notification::ResyncRequired);
    stop.send(()).unwrap();
    proxy.await.unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), old.recv())
            .await
            .unwrap(),
        Err(CacheError::Closed)
    ));
    assert!(
        tokio::time::timeout(Duration::from_secs(3), first.get("state"))
            .await
            .unwrap()
            .is_err()
    );
    // No memory fallback: state still lives only in the authoritative Redis namespace.
    assert_eq!(second.get("state").await.unwrap().unwrap().value, [1]);
    second
        .publish("changes", b"missed-revision".to_vec())
        .await
        .unwrap();
    let mut replacement = second.subscribe("changes").await.unwrap();
    assert_eq!(
        replacement.recv().await.unwrap(),
        Notification::ResyncRequired
    );
}

#[tokio::test]
#[ignore = "requires an anonymous TCP test Redis with at least two databases"]
async fn redis_namespace_and_database_isolate_values_and_notifications() {
    let url = std::env::var("GPROXY_CACHE_REDIS_URL").expect("set a local test Redis URL");
    let client = redis::Client::open(url.as_str()).unwrap();
    let info = client.get_connection_info();
    assert!(
        info.redis_settings().password().is_none() && info.redis_settings().username().is_none()
    );
    let redis::ConnectionAddr::Tcp(host, port) = info.addr() else {
        panic!("test requires TCP Redis")
    };
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.clone()
    };
    let other_db = if info.redis_settings().db() == 0 {
        1
    } else {
        0
    };
    let other_url = format!("redis://{host}:{port}/{other_db}");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let options = RedisOptions::new(format!("scope:{nonce}"));
    let one = RedisCache::connect(&url, options.clone()).await.unwrap();
    let database = RedisCache::connect(&other_url, options).await.unwrap();
    let namespace = RedisCache::connect(&url, RedisOptions::new(format!("scope-{nonce}")))
        .await
        .unwrap();
    one.put("same/key", vec![1], Duration::from_secs(30))
        .await
        .unwrap();
    assert!(database.get("same/key").await.unwrap().is_none());
    assert!(namespace.get("same/key").await.unwrap().is_none());
    let mut db_sub = database.subscribe("config").await.unwrap();
    let mut ns_sub = namespace.subscribe("config").await.unwrap();
    assert_eq!(db_sub.recv().await.unwrap(), Notification::ResyncRequired);
    assert_eq!(ns_sub.recv().await.unwrap(), Notification::ResyncRequired);
    one.publish("config", vec![1]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), db_sub.recv())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), ns_sub.recv())
            .await
            .is_err()
    );
}
