//! The one runtime service this crate needs.
//!
//! A rate-limit charge is given back through the cache, and the cache is
//! async. When a [`RateLease`](crate::admission::RateLease) is dropped without
//! having been released — a cancelled request, a host that forgot, a panic —
//! there is nowhere in `Drop` to await that call, so it is spawned instead.
//!
//! This is deliberately the whole of it: no timers, no task handles, no
//! executor. The crate owns no server and must keep building for
//! `wasm32-unknown-unknown`, where the JS event loop drives the same future.

use std::future::Future;

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
