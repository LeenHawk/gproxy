//! `sleep` on tokio natively and on a browser timeout under wasm32.

use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis().min(u128::from(u32::MAX)) as u32)
        .await;
}
