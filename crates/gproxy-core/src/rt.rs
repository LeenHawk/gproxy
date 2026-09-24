//! The few runtime services execution needs, with one implementation per
//! target: Tokio timers and tasks natively, browser timers and the JS event
//! loop on wasm32. Nothing else in core touches a runtime directly.

use std::{future::Future, time::Duration};

/// Resolves to None when `duration` elapses first.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn timeout<F: Future>(duration: Duration, future: F) -> Option<F::Output> {
    if duration == Duration::MAX {
        return Some(future.await);
    }
    tokio::time::timeout(duration, future).await.ok()
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn timeout<F: Future>(duration: Duration, future: F) -> Option<F::Output> {
    if duration == Duration::MAX {
        return Some(future.await);
    }
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
