//! The few runtime services synchronization needs from Tokio. This is core's
//! own shim, duplicated rather than shared so neither crate's runtime choices
//! leak into the other's public API.

use std::{future::Future, time::Duration};

/// Resolves to None when `duration` elapses first.
pub(crate) async fn timeout<F: Future>(duration: Duration, future: F) -> Option<F::Output> {
    tokio::time::timeout(duration, future).await.ok()
}

/// Run `future` to completion in the background. This needs a Tokio runtime
/// and returns false without one.
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

/// Wall-clock milliseconds since the Unix epoch: the timestamp every durable
/// row and cache payload in this workspace is written with.
pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
