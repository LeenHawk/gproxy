use std::time::Duration;

use gproxy_core::Host;
use gproxy_store::records::SettingRecord;

use crate::host::AppHost;

const SWEEP_INTERVAL: Duration = Duration::from_secs(5 * 60);
const SECONDS_PER_DAY: u64 = 86_400;
const MIB: u64 = 1024 * 1024;

pub(crate) const RETENTION_DAYS: &str = gproxy_store::records::RETENTION_DAYS;
pub(crate) const MAX_DATABASE_SIZE_MB: &str = gproxy_store::records::MAX_DATABASE_SIZE_MB;

/// Both bounds default rather than gating capture on being configured.
/// Requiring an operator to understand retention before they may record one
/// request is a refusal, not a safeguard — and defaulting is strictly safer,
/// because every deployment gets a bound instead of only the configured ones.
/// A century of retention means the calendar never purges and the size cap is
/// the bound that actually bites.
const DEFAULT_RETENTION_DAYS: u64 = 36_500;
const DEFAULT_MAX_DATABASE_SIZE_MB: u64 = 1_024;

pub(crate) fn schedule(host: &AppHost) {
    let Some(spawner) = host.spawner() else {
        return;
    };
    let host = host.clone();
    spawner.spawn(Box::pin(async move {
        loop {
            if let Err(error) = sweep(&host).await {
                tracing::warn!(error = %error, "observability cleanup sweep failed");
            }
            host.wait(SWEEP_INTERVAL).await;
        }
    }));
}

async fn sweep(host: &AppHost) -> Result<(), gproxy_store::StoreError> {
    let snapshot = host.services.control.current();
    let retention_cutoff = positive(&snapshot.settings, RETENTION_DAYS)
        .unwrap_or(DEFAULT_RETENTION_DAYS)
        .checked_mul(SECONDS_PER_DAY)
        .and_then(|seconds| i64::try_from(seconds).ok())
        .map(|seconds| unix_now().saturating_sub(seconds));
    let max_database_bytes = Some(
        positive(&snapshot.settings, MAX_DATABASE_SIZE_MB)
            .unwrap_or(DEFAULT_MAX_DATABASE_SIZE_MB)
            .checked_mul(MIB)
            .ok_or_else(|| invalid_setting(MAX_DATABASE_SIZE_MB))?,
    );
    let result = host
        .services
        .store
        .cleanup_observability(retention_cutoff, max_database_bytes)
        .await?;
    if result.retention_rows > 0 || result.pressure_rows > 0 {
        tracing::info!(
            retention_rows = result.retention_rows,
            pressure_rows = result.pressure_rows,
            size_bytes = result.size_bytes,
            over_size_limit = result.over_size_limit,
            "observability cleanup sweep removed rows"
        );
    }
    Ok(())
}

fn positive(settings: &[SettingRecord], key: &str) -> Option<u64> {
    settings
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_i64()
        .filter(|value| *value > 0)
        .map(|value| value as u64)
}

fn invalid_setting(key: &'static str) -> gproxy_store::StoreError {
    gproxy_store::StoreError::InvalidData {
        field: "setting",
        message: format!("{key} exceeds its supported range"),
    }
}

fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_secs() as i64
}
