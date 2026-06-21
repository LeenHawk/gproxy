//! §8-D usage/log retention: a native background task that purges usage and
//! request-log rows older than `instance_settings.retention_days`, at startup
//! and then on a fixed interval. `None` / `<= 0` disables purging (the historical
//! retain-forever behaviour). Edge isolates are short-lived and run no task — the
//! libSQL `purge_before` is a no-op there anyway, so server-side cleanup applies.

use std::time::Duration;

use crate::app::AppState;

/// How often the sweep runs after the startup pass.
const SWEEP_INTERVAL: Duration = Duration::from_secs(6 * 3600);
const SECS_PER_DAY: i64 = 86_400;

/// Spawn the retention sweeper (native only). It runs once immediately, then
/// every [`SWEEP_INTERVAL`]; a failed sweep is logged and retried next tick.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_retention_task(state: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = sweep_once(&state).await {
                tracing::warn!(error = %e, "retention sweep failed");
            }
            tokio::time::sleep(SWEEP_INTERVAL).await;
        }
    });
}

/// One sweep: read the effective retention window and purge rows older than it.
/// Disabled (None / non-positive) windows are a no-op.
#[cfg(not(target_arch = "wasm32"))]
async fn sweep_once(state: &AppState) -> anyhow::Result<()> {
    let Some(days) = retention_days(state).await?.filter(|d| *d > 0) else {
        return Ok(());
    };
    let cutoff = crate::util::time::unix_now() - days.saturating_mul(SECS_PER_DAY);
    let removed = state.persistence.purge_before(cutoff).await?;
    if removed > 0 {
        tracing::info!(removed, days, "retention sweep purged old usage/log rows");
    }
    Ok(())
}

/// The retention window from the (single) instance-settings row; `None` = unset.
#[cfg(not(target_arch = "wasm32"))]
async fn retention_days(state: &AppState) -> anyhow::Result<Option<i64>> {
    Ok(state
        .persistence
        .list_instance_settings()
        .await?
        .first()
        .and_then(|s| s.retention_days))
}
