//! Native process policy follows the published settings revision.
use crate::{maintenance, telemetry, update::Updater};
use gproxy_app::App;
use sea_orm::DatabaseConnection;
use std::{sync::Arc, time::Duration};

/// How often the capture payload size budget is measured and enforced. The
/// measurement scans the payload tables, so it is deliberately slower than the
/// one-minute maintenance tick that handles age-based pruning.
const CAPTURE_BUDGET_INTERVAL: Duration = Duration::from_secs(15 * 60);

pub struct RuntimeTask(tokio::task::JoinHandle<()>);
impl Drop for RuntimeTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn start(app: &Arc<App<DatabaseConnection>>, updater: Arc<Updater>) -> RuntimeTask {
    let app = Arc::downgrade(app);
    RuntimeTask(tokio::spawn(async move {
        let mut logging = None;
        let mut cleanup_policy = None;
        let mut last_cleanup = tokio::time::Instant::now() - Duration::from_secs(60);
        let mut last_budget = tokio::time::Instant::now() - CAPTURE_BUDGET_INTERVAL;
        loop {
            let Some(app) = app.upgrade() else {
                return;
            };
            let settings = app.data().settings.clone();
            if let Some(settings) = settings {
                let next_logging = (settings.log_level.clone(), settings.log_format.clone());
                if logging.as_ref() != Some(&next_logging) {
                    match telemetry::configure(&settings.log_level, &settings.log_format) {
                        Ok(()) => logging = Some(next_logging),
                        Err(error) => {
                            tracing::error!(%error, "could not apply process logging settings")
                        }
                    }
                }
                if let Err(error) = updater.configure(
                    settings.update_channel.as_deref(),
                    settings.update_source.as_deref(),
                    settings.enable_auto_update_check,
                    settings.update_verify_signature,
                ) {
                    tracing::error!(%error, "could not apply update settings");
                }
                let policy = (
                    settings.retention_days,
                    settings.quota_observation_retention_days,
                    settings.max_database_size_mb,
                    settings.capture_payload_retention_days,
                    settings.capture_payload_max_mb,
                );
                if cleanup_policy != Some(policy)
                    || last_cleanup.elapsed() >= Duration::from_secs(60)
                {
                    let db = app.gproxy().store().connection();
                    if let Err(error) = maintenance::clean(
                        db,
                        policy.0,
                        policy.1,
                        policy.2,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as i64,
                    )
                    .await
                    {
                        tracing::error!(%error, "request history cleanup failed");
                    }
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;
                    let store = app.gproxy().store();
                    // Age pruning is an indexed range scan: cheap every tick.
                    if let Some(days) = policy.3
                        && let Err(error) = store.prune_expired_capture_payloads(days, now_ms).await
                    {
                        tracing::error!(%error, "capture payload cleanup failed");
                    }
                    // The size budget has to measure the payload tables, so it
                    // runs on its own, slower cadence (and when the policy changes).
                    if let Some(mb) = policy.4
                        && (cleanup_policy != Some(policy)
                            || last_budget.elapsed() >= CAPTURE_BUDGET_INTERVAL)
                    {
                        let limit = (mb.max(0) as u64).saturating_mul(1024 * 1024);
                        if let Err(error) =
                            store.enforce_capture_payload_budget(limit, now_ms).await
                        {
                            tracing::error!(%error, "capture payload budget failed");
                        }
                        last_budget = tokio::time::Instant::now();
                    }
                    cleanup_policy = Some(policy);
                    last_cleanup = tokio::time::Instant::now();
                }
            }
            drop(app);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }))
}
