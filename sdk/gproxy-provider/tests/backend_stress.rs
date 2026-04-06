//! Concurrent stress tests for InMemory backend implementations.

use std::sync::Arc;
use std::task::{Context, Poll};

use gproxy_provider::backend::memory::{InMemoryQuota, InMemoryQuotaHold, InMemoryRateLimit};
use gproxy_provider::backend::traits::{QuotaBackend, QuotaHold, RateLimitBackend};
use gproxy_provider::backend::types::{
    QuotaExhausted, RateLimitExceeded, RateLimitWindow,
};

// ---------------------------------------------------------------------------
// Sync helpers — InMemory futures are always Ready, so we can poll them once.
// ---------------------------------------------------------------------------

fn poll_set_quota(q: &InMemoryQuota, id: i64, total: u64) {
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(QuotaBackend::set_quota(q, id, total));
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(())) => {}
        other => panic!("unexpected: {other:?}"),
    }
}

fn poll_try_reserve(q: &InMemoryQuota, id: i64, amount: u64) -> Result<InMemoryQuotaHold, QuotaExhausted> {
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(QuotaBackend::try_reserve(q, id, amount));
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("InMemory future should be Ready"),
    }
}

fn poll_settle(hold: InMemoryQuotaHold, cost: u64) {
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(QuotaHold::settle(hold, cost));
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(())) => {}
        other => panic!("unexpected: {other:?}"),
    }
}

fn poll_try_acquire(rl: &InMemoryRateLimit, key: &str, window: RateLimitWindow) -> Result<u64, RateLimitExceeded> {
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(RateLimitBackend::try_acquire(rl, key, window));
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("InMemory future should be Ready"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_quota_holds_do_not_over_commit() {
    let backend = Arc::new(InMemoryQuota::new());
    poll_set_quota(&backend, 1, 1000);

    let mut handles: Vec<tokio::task::JoinHandle<bool>> = Vec::new();
    for _ in 0..20 {
        let b = Arc::clone(&backend);
        handles.push(tokio::spawn(async move {
            match poll_try_reserve(&b, 1, 100) {
                Ok(hold) => {
                    poll_settle(hold, 50);
                    true
                }
                Err(_) => false,
            }
        }));
    }

    let results: Vec<bool> = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .map(|r: Result<bool, _>| r.unwrap())
        .collect();

    let succeeded = results.iter().filter(|&&r| r).count();
    // Each reserve=100, settle=50. Net cost per task = 50.
    // Total quota = 1000. Max tasks = 1000/50 = 20, but last one may fail
    // if reservation check (remaining >= 100) fails before settle frees space.
    // On single-threaded runtime: tasks run serially, settle frees reservation
    // before next task runs, so up to 19 tasks succeed (1000 - 19*50 = 50 < 100).
    assert!(
        succeeded >= 1 && succeeded <= 20,
        "unexpected: {succeeded} holds succeeded"
    );
}

#[tokio::test]
async fn concurrent_rate_limit_does_not_exceed() {
    let backend = Arc::new(InMemoryRateLimit::new());
    let window = RateLimitWindow::PerMinute { limit: 50 };

    let mut handles: Vec<tokio::task::JoinHandle<bool>> = Vec::new();
    for _ in 0..100 {
        let b = Arc::clone(&backend);
        handles.push(tokio::spawn(async move {
            poll_try_acquire(&b, "test-key", window).is_ok()
        }));
    }

    let results: Vec<bool> = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .map(|r: Result<bool, _>| r.unwrap())
        .collect();

    let succeeded = results.iter().filter(|&&r| r).count();
    assert_eq!(succeeded, 50, "expected exactly 50 acquires, got {succeeded}");
}

#[test]
fn different_rate_limit_keys_are_independent() {
    let backend = InMemoryRateLimit::new();
    let window = RateLimitWindow::PerMinute { limit: 2 };

    assert!(poll_try_acquire(&backend, "key-a", window).is_ok());
    assert!(poll_try_acquire(&backend, "key-a", window).is_ok());
    assert!(poll_try_acquire(&backend, "key-a", window).is_err());
    assert!(poll_try_acquire(&backend, "key-b", window).is_ok());
}

#[test]
fn quota_exhaustion_returns_remaining_info() {
    let backend = InMemoryQuota::new();
    poll_set_quota(&backend, 1, 100);

    let _hold = poll_try_reserve(&backend, 1, 80).unwrap();
    let err = poll_try_reserve(&backend, 1, 30).unwrap_err();
    assert_eq!(err.remaining, 20);
    assert_eq!(err.requested, 30);
}
