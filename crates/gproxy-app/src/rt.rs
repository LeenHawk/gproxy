//! The few runtime services this crate needs.
//!
//! A rate-limit charge is given back through the cache, and the cache is
//! async. When a [`RateLease`](crate::admission::RateLease) is dropped without
//! having been released — a cancelled request, a host that forgot, a panic —
//! there is nowhere in `Drop` to await that call, so it is spawned instead.
//! [`crate::sync`] adds a timer, for the revision poll, and a way to ask a
//! future whether it is ready without waiting for it, for draining a
//! subscription.
//!
//! That is deliberately the whole of it: no task handles and no executor. The
//! crate owns no server and must keep building for
//! `wasm32-unknown-unknown`, where the JS event loop drives the same future
//! and nothing sleeps at all.

use std::future::Future;

/// The output of `future` if it is ready **now**, and `None` if it would have
/// to wait.
///
/// One poll, then the future is dropped. That is what "take whatever is
/// already delivered, wait for nothing" means for a subscription, and it is
/// the same contract `tokio::time::timeout` with a zero duration has — which
/// is what `gproxy-sdk`'s own drain uses. Written out here instead so that
/// draining costs this crate no timer on either target: wasm has no Tokio
/// clock, and a zero-length sleep through the JS event loop would turn every
/// edge request's drain into a macrotask.
///
/// Dropping a half-polled `recv()` is safe because [`gproxy_cache::Subscription`]
/// documents it as cancel-safe for both supplied backends.
pub(crate) async fn ready_now<F: Future>(future: F) -> Option<F::Output> {
    let mut future = std::pin::pin!(future);
    std::future::poll_fn(move |cx| {
        std::task::Poll::Ready(match future.as_mut().poll(cx) {
            std::task::Poll::Ready(output) => Some(output),
            std::task::Poll::Pending => None,
        })
    })
    .await
}

/// Wait out an interval, for the revision poll's loop. There is no wasm
/// implementation on purpose: an isolate has no loop to sleep in, and
/// `App::tick` is what it calls instead.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sleep(duration: std::time::Duration) {
    tokio::time::sleep(duration).await;
}

/// Run `future` to completion in the background, and report whether anything
/// will. Natively this needs a Tokio runtime and returns false without one, in
/// which case the caller falls back to letting the cache entry expire.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn<F>(future: F) -> bool
where
    F: Future<Output = ()> + Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(future);
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn<F>(future: F) -> bool
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
    true
}
