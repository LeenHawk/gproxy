use crate::AppHandle;
use futures_util::{StreamExt, stream};
use gproxy_admin::State;
use gproxy_core::{CacheBackend, Host};
use std::time::Duration;

pub(crate) fn now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_secs() as i64
}

pub(crate) fn schedule(app: &AppHandle) {
    let Some(spawner) = app.inner.host.spawner() else {
        return;
    };
    let app = app.clone();
    spawner.spawn(Box::pin(async move {
        let maintenance = async {
            loop {
                if let Err(error) = sweep(&app).await {
                    tracing::warn!(error = %error, "quota maintenance failed");
                }
                app.inner.host.wait(Duration::from_secs(30)).await;
            }
        };
        let shutdown = app.wait_shutdown();
        futures_util::pin_mut!(maintenance, shutdown);
        futures_util::future::select(maintenance, shutdown).await;
    }));
}

async fn sweep(app: &AppHandle) -> Result<(), gproxy_admin::AdminError> {
    let now = now();
    let active = app
        .inner
        .host
        .services
        .store
        .active_usage_credentials(now - 1800)
        .await?;
    let snapshot = app.inner.host.services.control.current();
    for credential in &snapshot.credentials {
        app.inner
            .host
            .services
            .store
            .repair_credential_quota(credential.id, now)
            .await?;
    }
    let credentials = snapshot
        .credentials
        .iter()
        .filter(|credential| {
            credential.enabled
                && active.contains(&credential.id)
                && snapshot
                    .providers
                    .iter()
                    .any(|provider| provider.id == credential.provider_id && provider.enabled)
        })
        .map(|credential| credential.id)
        .collect::<Vec<_>>();
    let results = stream::iter(credentials)
        .map(|id| refresh(app, id, now))
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
    for result in results {
        result?;
    }
    Ok(())
}

async fn refresh(app: &AppHandle, id: i64, now: i64) -> Result<(), gproxy_admin::AdminError> {
    if !app
        .credential_quota_capabilities(id)
        .await?
        .is_some_and(|capability| capability.probe)
    {
        return Ok(());
    }
    let store = &app.inner.host.services.store;
    store.repair_credential_quota(id, now).await?;
    let cycles = store.credential_quota_cycles(Some(id), 0, now + 1).await?;
    let fresh = cycles
        .iter()
        .any(|cycle| cycle.last_observed_at > now - 600);
    if fresh {
        return Ok(());
    }
    let cache = &app.inner.host.services.cache;
    let retry_key = format!("quota:retry:{id}");
    if cache.get(&retry_key).await.map_err(internal)?.is_some() {
        return Ok(());
    }
    if let Err(error) = crate::admin::quota_probe::run(app, id, false).await {
        tracing::warn!(credential_id = id, error = %error, "quota refresh unavailable");
    }
    Ok(())
}

pub(crate) async fn opportunistic(app: &AppHandle) {
    if app.inner.host.spawner().is_some() {
        return;
    }
    let due = app
        .inner
        .host
        .services
        .cache
        .compare_and_swap(
            "quota:maintenance:due",
            None,
            Some(vec![1]),
            Some(Duration::from_secs(30)),
        )
        .await;
    if matches!(due, Ok(true))
        && let Err(error) = sweep(app).await
    {
        tracing::warn!(error = %error, "opportunistic quota maintenance failed");
    }
}

fn internal(error: impl std::fmt::Display) -> gproxy_admin::AdminError {
    gproxy_admin::AdminError::Internal(error.to_string())
}
