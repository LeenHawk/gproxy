use gproxy_core::CacheBackend;

use crate::cache::AppCache;
use crate::{AppError, AppHandle};

const KEY: &str = "gproxy:invalidate";
#[cfg(not(target_arch = "wasm32"))]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub(crate) async fn current(cache: &AppCache) -> Result<i64, AppError> {
    cache
        .incr(KEY, 0, None)
        .await
        .map_err(|error| AppError::Cache(error.to_string()))
}

pub(crate) async fn bump(cache: &AppCache) -> Result<i64, AppError> {
    cache
        .incr(KEY, 1, None)
        .await
        .map_err(|error| AppError::Cache(error.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn schedule(app: &AppHandle) {
    let inner = std::sync::Arc::downgrade(&app.inner);
    let mut shutdown = app.inner.shutdown.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow_and_update() {
                        return;
                    }
                }
                _ = tokio::time::sleep(POLL_INTERVAL) => {
                    let Some(inner) = inner.upgrade() else {
                        return;
                    };
                    let app = AppHandle { inner };
                    if let Err(error) = app.sync_invalidation().await {
                        tracing::warn!(error = %error, "control-plane invalidation poll failed");
                    }
                }
            }
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn schedule(_app: &AppHandle) {}
