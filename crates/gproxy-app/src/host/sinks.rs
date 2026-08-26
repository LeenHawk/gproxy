use gproxy_channel_api::BoxFuture;
use gproxy_core::{CaptureSink, Ended, UsageSink, UsageSource};

use super::AppHost;

impl UsageSink for AppHost {
    fn record<'a>(&'a self, settlement: &'a gproxy_core::Settlement) -> BoxFuture<'a, ()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let host = self.clone();
            let settlement = settlement.clone();
            Box::pin(async move {
                let task = tokio::spawn(async move {
                    record_settlement(&host, &settlement).await;
                });
                if let Err(error) = task.await {
                    tracing::error!(error = %error, "usage settlement task failed");
                }
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(record_settlement(self, settlement))
        }
    }
}

async fn record_settlement(host: &AppHost, settlement: &gproxy_core::Settlement) {
    let state = match super::admission::load(host, &settlement.request_id).await {
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
    super::admission::finish(host, &settlement.request_id, Some(settlement)).await;
    if let Err(error) = host.services.store.record_usage(&input).await {
        tracing::error!(request_id = %settlement.request_id, error = %error, "persist usage failed");
    }
}

impl CaptureSink for AppHost {
    fn record<'a>(&'a self, capture: &'a gproxy_core::host::Capture) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if !crate::cleanup::body_capture_enabled(&self.services.control.current().settings) {
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

fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_secs() as i64
}
