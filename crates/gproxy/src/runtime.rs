//! Native process policy follows the published settings revision.
use crate::{maintenance, telemetry, update::Updater};
use gproxy_app::App;
use sea_orm::DatabaseConnection;
use std::{sync::Arc, time::Duration};

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
                    cleanup_policy = Some(policy);
                    last_cleanup = tokio::time::Instant::now();
                }
            }
            drop(app);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }))
}
