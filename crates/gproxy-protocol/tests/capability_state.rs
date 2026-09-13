use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use bytes::Bytes;
use gproxy_protocol::capability::{
    CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, StateEntry, StateStore,
    StateWrite, Version,
};

fn limits() -> CapabilityLimits {
    CapabilityLimits {
        operation_total: Duration::from_secs(5),
        stream_idle: Duration::from_secs(1),
        read_bytes: 1024,
        write_bytes: 1024,
        ws_frame_bytes: 1024,
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synchronous contract fake unexpectedly pending"),
    }
}

#[derive(Clone)]
struct MemoryStore {
    inner: Arc<Mutex<StoreInner>>,
}

struct StoreInner {
    next_version: u64,
    entries: HashMap<(String, String), StateEntry>,
}

impl StateStore for MemoryStore {
    type Scope = String;

    fn get<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        let mut inner = self.inner.lock().unwrap();
        let map_key = (scope.clone(), key.to_owned());
        let expired = inner
            .entries
            .get(&map_key)
            .is_some_and(|entry| entry.expires_at.is_some_and(|at| SystemTime::now() >= at));
        if expired {
            inner.entries.remove(&map_key);
        }
        let value = inner.entries.get(&map_key).cloned();
        Box::pin(async move { Ok(value) })
    }

    fn compare_exchange<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        let mut inner = self.inner.lock().unwrap();
        let map_key = (scope.clone(), key.to_owned());
        let expired = inner
            .entries
            .get(&map_key)
            .is_some_and(|entry| entry.expires_at.is_some_and(|at| SystemTime::now() >= at));
        if expired {
            inner.entries.remove(&map_key);
        }
        let current_version = inner
            .entries
            .get(&map_key)
            .map(|entry| entry.version.clone());
        let result = if current_version != expected {
            CasResult::Conflict
        } else {
            match replacement {
                Some(write) => {
                    inner.next_version += 1;
                    let version = Version::from_bytes(inner.next_version.to_be_bytes().to_vec());
                    inner.entries.insert(
                        map_key,
                        StateEntry {
                            payload: write.payload,
                            version: version.clone(),
                            expires_at: write.expires_at,
                        },
                    );
                    CasResult::Applied(Some(version))
                }
                None => {
                    inner.entries.remove(&map_key);
                    CasResult::Applied(None)
                }
            }
        };
        Box::pin(async move { Ok(result) })
    }

    fn limits(&self) -> CapabilityLimits {
        limits()
    }
}

#[test]
fn scoped_cas_supports_resume_conflicts_ttl_and_fresh_versions() {
    let store = MemoryStore {
        inner: Arc::new(Mutex::new(StoreInner {
            next_version: 0,
            entries: HashMap::new(),
        })),
    };
    let a = "session-a".to_owned();
    let b = "session-b".to_owned();
    let applied = ready(store.compare_exchange(
        &a,
        "cursor",
        None,
        Some(StateWrite {
            payload: Bytes::from_static(b"page-2"),
            expires_at: None,
        }),
    ))
    .unwrap();
    let CasResult::Applied(Some(version_a)) = applied else {
        panic!("initial write failed")
    };
    let entry = ready(store.get(&a, "cursor")).unwrap().unwrap();
    assert_eq!(entry.payload, Bytes::from_static(b"page-2"));
    assert_eq!(
        ready(store.compare_exchange(&b, "cursor", Some(version_a.clone()), None)).unwrap(),
        CasResult::Conflict
    );
    assert_eq!(
        ready(store.compare_exchange(&a, "cursor", Some(version_a.clone()), None)).unwrap(),
        CasResult::Applied(None)
    );
    assert!(ready(store.get(&a, "cursor")).unwrap().is_none());
    let CasResult::Applied(Some(version_b)) = ready(store.compare_exchange(
        &a,
        "cursor",
        None,
        Some(StateWrite {
            payload: Bytes::from_static(b"page-3"),
            expires_at: Some(SystemTime::now() - Duration::from_secs(1)),
        }),
    ))
    .unwrap() else {
        panic!("recreate failed")
    };
    assert_ne!(version_a, version_b);
    assert!(ready(store.get(&a, "cursor")).unwrap().is_none());
}

#[test]
fn cas_treats_expired_records_as_absent_without_a_preceding_read() {
    let store = MemoryStore {
        inner: Arc::new(Mutex::new(StoreInner {
            next_version: 0,
            entries: HashMap::new(),
        })),
    };
    let scope = "session".to_owned();
    let mut versions = Vec::new();
    for key in ["replace", "stale"] {
        let CasResult::Applied(Some(version)) = ready(store.compare_exchange(
            &scope,
            key,
            None,
            Some(StateWrite {
                payload: Bytes::from_static(b"old"),
                expires_at: Some(SystemTime::now() - Duration::from_secs(1)),
            }),
        ))
        .unwrap() else {
            panic!("initial write")
        };
        versions.push(version);
    }
    let CasResult::Applied(Some(fresh)) = ready(store.compare_exchange(
        &scope,
        "replace",
        None,
        Some(StateWrite {
            payload: Bytes::from_static(b"fresh"),
            expires_at: None,
        }),
    ))
    .unwrap() else {
        panic!("expired must be absent during CAS")
    };
    assert_ne!(fresh, versions[0]);
    assert_eq!(
        ready(store.get(&scope, "replace"))
            .unwrap()
            .unwrap()
            .payload,
        "fresh"
    );
    assert_eq!(
        ready(store.compare_exchange(&scope, "stale", Some(versions[1].clone()), None)).unwrap(),
        CasResult::Conflict
    );
    assert!(ready(store.get(&scope, "stale")).unwrap().is_none());
}
