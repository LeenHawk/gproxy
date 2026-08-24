use gproxy_channel_api::{
    BoxFuture, CallerIdentity, CredentialId, QuotaWindow, StateError, UsageView, UsageWindow,
};
use gproxy_store::records::QuotaBoundarySource;
use rust_decimal::Decimal;

use super::AppHost;

pub(super) struct AppUsageView {
    host: AppHost,
    identity: CallerIdentity,
    provider_id: i64,
    credential: CredentialId,
}

impl AppUsageView {
    pub(super) fn new(
        host: AppHost,
        identity: CallerIdentity,
        provider_id: i64,
        credential: CredentialId,
    ) -> Self {
        Self {
            host,
            identity,
            provider_id,
            credential,
        }
    }
}

impl UsageView for AppUsageView {
    fn window<'a>(&'a self, since_unix: i64) -> BoxFuture<'a, Result<UsageWindow, StateError>> {
        Box::pin(async move {
            let window = self
                .host
                .services
                .store
                .usage_window(self.identity.user_id, self.provider_id, since_unix)
                .await
                .map_err(state_error)?;
            Ok(UsageWindow {
                cost: window.cost,
                input_tokens: window.input_tokens,
                output_tokens: window.output_tokens,
            })
        })
    }

    fn quota_windows<'a>(&'a self) -> BoxFuture<'a, Result<Vec<QuotaWindow>, StateError>> {
        Box::pin(async move {
            self.host
                .services
                .store
                .open_credential_quota_cycles(self.credential.0, unix_now())
                .await
                .map_err(state_error)
                .map(|cycles| {
                    cycles
                        .into_iter()
                        .map(|cycle| QuotaWindow {
                            key: cycle.window_key,
                            period_start: cycle.period_start,
                            reset_at: (cycle.boundary_source == QuotaBoundarySource::Upstream)
                                .then_some(cycle.period_end)
                                .flatten(),
                            used_percent: cycle.used_percent.or_else(|| {
                                let used = cycle.upstream_used?;
                                let limit = cycle.upstream_limit?;
                                (limit > Decimal::ZERO).then(|| used / limit * Decimal::ONE_HUNDRED)
                            }),
                            upstream_used: cycle.upstream_used,
                            upstream_limit: cycle.upstream_limit,
                        })
                        .collect()
                })
        })
    }
}

fn state_error(error: impl std::fmt::Display) -> StateError {
    StateError(error.to_string())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_secs() as i64
}
