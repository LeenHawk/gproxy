//! Native background maintenance for time retention and SQLite size pressure.
//! Edge isolates are short-lived and do not run background tasks.

use std::time::Duration;

use crate::app::AppState;

/// How often the sweep runs after the startup pass.
const RETENTION_SWEEP_INTERVAL: Duration = Duration::from_secs(6 * 3600);
const SIZE_SWEEP_INTERVAL: Duration = Duration::from_secs(5 * 60);
const SECS_PER_DAY: i64 = 86_400;
const MIB: u64 = 1024 * 1024;

/// Spawn native maintenance tasks. Each runs immediately and retries failures
/// on its own interval.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_retention_task(state: AppState) {
    let size_state = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = retention_sweep(&state).await {
                tracing::warn!(error = %e, "retention sweep failed");
            }
            tokio::time::sleep(RETENTION_SWEEP_INTERVAL).await;
        }
    });
    tokio::spawn(async move {
        loop {
            if let Err(e) = size_sweep(&size_state).await {
                tracing::warn!(error = %e, "database size sweep failed");
            }
            tokio::time::sleep(SIZE_SWEEP_INTERVAL).await;
        }
    });
}

/// One sweep: read the effective retention window and purge rows older than it.
/// Disabled (None / non-positive) windows are a no-op.
#[cfg(not(target_arch = "wasm32"))]
async fn retention_sweep(state: &AppState) -> anyhow::Result<()> {
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

#[cfg(not(target_arch = "wasm32"))]
async fn size_sweep(state: &AppState) -> anyhow::Result<()> {
    let Some(limit_mb) = instance_settings(state)
        .await?
        .and_then(|s| s.max_database_size_mb)
        .filter(|limit| *limit > 0)
    else {
        return Ok(());
    };
    let max_bytes = u64::try_from(limit_mb)?
        .checked_mul(MIB)
        .ok_or_else(|| anyhow::anyhow!("database size limit is too large"))?;
    let target_bytes = ((max_bytes as u128) * 90 / 100) as u64;

    let Some(result) = state
        .persistence
        .prune_observability_storage(max_bytes, target_bytes)
        .await?
    else {
        return Ok(());
    };

    if result.after_bytes > target_bytes {
        tracing::warn!(
            before_bytes = result.before_bytes,
            after_bytes = result.after_bytes,
            target_bytes,
            removed_rows = result.removed_rows,
            exhausted = result.exhausted,
            "database remains above size target; usage data was preserved"
        );
    } else {
        tracing::info!(
            before_bytes = result.before_bytes,
            after_bytes = result.after_bytes,
            target_bytes,
            removed_rows = result.removed_rows,
            "database size sweep removed old request/audit logs"
        );
    }
    Ok(())
}

/// The retention window from the (single) instance-settings row; `None` = unset.
#[cfg(not(target_arch = "wasm32"))]
async fn retention_days(state: &AppState) -> anyhow::Result<Option<i64>> {
    Ok(instance_settings(state)
        .await?
        .and_then(|s| s.retention_days))
}

#[cfg(not(target_arch = "wasm32"))]
async fn instance_settings(
    state: &AppState,
) -> anyhow::Result<Option<crate::store::persistence::records::InstanceSettings>> {
    Ok(state
        .persistence
        .list_instance_settings()
        .await?
        .into_iter()
        .next())
}
