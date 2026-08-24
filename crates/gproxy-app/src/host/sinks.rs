use gproxy_channel_api::{BoxFuture, StateError, UsageView, UsageWindow};
use gproxy_core::{CaptureSink, Ended, UsageSink, UsageSource};

use super::AppHost;

impl UsageSink for AppHost {
    fn record<'a>(&'a self, settlement: &'a gproxy_core::Settlement) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let state = match super::admission::load(self, &settlement.request_id).await {
                Ok(state) => state,
                Err(error) => {
                    tracing::error!(request_id = %settlement.request_id, error = %error, "load usage identity failed");
                    None
                }
            };
            let identity = state.as_ref().map(|state| &state.identity);
            let input = gproxy_store::records::UsageInput {
                request_id: settlement.request_id.clone(),
                at: unix_now(),
                provider_id: settlement.provider_id,
                credential_id: settlement.credential_id.0,
                organization_id: identity.and_then(|identity| identity.org_id),
                team_id: identity.and_then(|identity| identity.team_id),
                user_id: identity.map(|identity| identity.user_id),
                user_key_id: identity.map(|identity| identity.user_key_id),
                operation: state.and_then(|state| state.operation),
                upstream_model: settlement.upstream_model.clone(),
                input_tokens: settlement.usage.input_tokens,
                output_tokens: settlement.usage.output_tokens,
                cached_input_tokens: settlement.usage.cached_input_tokens,
                metrics: serde_json::to_value(&settlement.usage.metrics)
                    .expect("decimal metrics serialize"),
                dimensions: serde_json::to_value(&settlement.usage.dimensions)
                    .expect("string dimensions serialize"),
                cost: settlement.cost,
                usage_source: match settlement.source {
                    UsageSource::Upstream => "upstream",
                    UsageSource::Estimated => "estimated",
                }
                .into(),
                ended: match settlement.ended {
                    Ended::Complete => "complete",
                    Ended::Interrupted => "interrupted",
                }
                .into(),
                latency_ms: settlement.latency_ms,
            };
            if let Err(error) = self.services.store.record_usage(&input).await {
                tracing::error!(request_id = %settlement.request_id, error = %error, "persist usage failed");
            }
        })
    }
}

impl CaptureSink for AppHost {
    fn record<'a>(&'a self, capture: &'a gproxy_core::host::Capture) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if !capture_enabled(self) {
                return;
            }
            let input = gproxy_store::records::CaptureInput {
                request_id: capture.request_id.clone(),
                at: unix_now(),
                provider_id: capture.provider_id,
                credential_id: capture.credential_id.map(|credential| credential.0),
                upstream_url: capture.upstream_url.clone(),
                response_status: capture.response_status.map(|status| status.as_u16()),
                request_body: capture.request_body.to_vec(),
                response_body: capture.response_body.as_ref().map(|body| body.to_vec()),
            };
            if let Err(error) = self.services.store.record_capture(&input).await {
                tracing::error!(request_id = %capture.request_id, error = %error, "persist capture failed");
            }
        })
    }
}

pub(super) struct AppUsageView {
    store: gproxy_store::Store,
    user_id: i64,
    provider_id: i64,
}

impl AppUsageView {
    pub(super) fn new(store: gproxy_store::Store, user_id: i64, provider_id: i64) -> Self {
        Self {
            store,
            user_id,
            provider_id,
        }
    }
}

impl UsageView for AppUsageView {
    fn window<'a>(&'a self, since_unix: i64) -> BoxFuture<'a, Result<UsageWindow, StateError>> {
        Box::pin(async move {
            let window = self
                .store
                .usage_window(self.user_id, self.provider_id, since_unix)
                .await
                .map_err(|error| StateError(error.to_string()))?;
            Ok(UsageWindow {
                cost: window.cost,
                input_tokens: window.input_tokens,
                output_tokens: window.output_tokens,
            })
        })
    }
}

fn capture_enabled(host: &AppHost) -> bool {
    host.services
        .control
        .current()
        .settings
        .iter()
        .any(|setting| {
            setting.key == "capture_enabled" && setting.value == serde_json::Value::Bool(true)
        })
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_secs() as i64
}
