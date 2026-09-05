use crate::AppHandle;
use gproxy_admin::AdminError;
use gproxy_core::{CacheBackend, Host};
use std::time::Duration;

pub(super) async fn acquire(app: &AppHandle, owner: &[u8]) -> Result<usize, AdminError> {
    for _ in 0..120 {
        for slot in 0..4 {
            if app
                .inner
                .host
                .services
                .cache
                .compare_and_swap(
                    &format!("quota:probe-slot:{slot}"),
                    None,
                    Some(owner.to_vec()),
                    Some(Duration::from_secs(120)),
                )
                .await
                .map_err(super::internal)?
            {
                return Ok(slot);
            }
        }
        app.inner.host.wait(Duration::from_millis(100)).await;
    }
    Err(AdminError::Conflict(
        "quota probe concurrency limit reached".into(),
    ))
}

pub(super) async fn release(app: &AppHandle, slot: usize, owner: &[u8]) -> Result<(), AdminError> {
    app.inner
        .host
        .services
        .cache
        .compare_and_swap(
            &format!("quota:probe-slot:{slot}"),
            Some(owner.to_vec()),
            None,
            None,
        )
        .await
        .map_err(super::internal)?;
    Ok(())
}
