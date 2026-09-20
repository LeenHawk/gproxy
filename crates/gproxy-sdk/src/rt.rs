//! The few runtime services synchronization needs, with one implementation per
//! target: Tokio timers and tasks natively, browser timers and the JS event
//! loop on wasm32. This is core's own shim, duplicated rather than shared so
//! neither crate's runtime choices leak into the other's public API.
// Each target uses a different subset: wasm never spawns a background loop.
#![allow(dead_code)]

use std::{future::Future, time::Duration};

/// Resolves to None when `duration` elapses first.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn timeout<F: Future>(duration: Duration, future: F) -> Option<F::Output> {
    tokio::time::timeout(duration, future).await.ok()
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn timeout<F: Future>(duration: Duration, future: F) -> Option<F::Output> {
    use futures_util::future::{Either, select};
    let future = std::pin::pin!(future);
    let timer = std::pin::pin!(sleep(duration));
    match select(future, timer).await {
        Either::Left((output, _)) => Some(output),
        Either::Right(((), _)) => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sleep(duration: Duration) {
    let millis = u32::try_from(duration.as_millis()).unwrap_or(u32::MAX);
    gloo_timers::future::TimeoutFuture::new(millis).await;
}

/// Run `future` to completion in the background. Natively this needs a Tokio
/// runtime and returns false without one; on wasm the JS event loop always
/// drives it.
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

/// Wall-clock milliseconds since the Unix epoch: the timestamp every durable
/// row and cache payload in this workspace is written with. `web_time` is the
/// shim core uses for the same purpose, so wasm32 reads the browser's clock
/// instead of panicking on `std::time::SystemTime`.
pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
