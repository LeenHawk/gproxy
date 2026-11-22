//! The few runtime services execution needs from Tokio. Nothing else in core
//! spawns a task directly.

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
