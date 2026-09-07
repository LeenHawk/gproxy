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
    super::admission::credential_budget::record(host, settlement).await;
    let settings = host.services.control.settings();
    let state = match super::admission::load(host, &settlement.request_id).await {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(request_id = %settlement.request_id, error = %error, "load usage identity failed");
            None
        }
    };
    let identity = state.as_ref().map(|state| &state.identity);
    let mut dimensions = settlement.usage.dimensions.clone();
    dimensions.insert("instance_name".into(), settings.instance_name.clone());
    dimensions.insert("instance_id".into(), settings.instance_id.to_string());
    let mut input = gproxy_store::records::UsageInput {
        upstream_started_at_ms: settlement.upstream_started_at_ms,
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
        dimensions: serde_json::to_value(dimensions).expect("string dimensions serialize"),
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
    if !settings.enable_usage {
        return;
    }
    if settlement.attempts.is_empty() {
        if let Err(error) = host.services.store.record_usage(&input).await {
            tracing::error!(request_id = %settlement.request_id, error = %error, "persist usage failed");
        }
    } else {
        for (index, attempt) in settlement.attempts.iter().enumerate() {
            input.request_id = if index == 0 {
                settlement.request_id.clone()
            } else {
                format!("{}:attempt:{index}", settlement.request_id)
            };
            input.upstream_model.clone_from(&attempt.upstream_model);
            input.upstream_started_at_ms = attempt.started_at_ms;
            input.input_tokens = attempt.usage.input_tokens;
            input.output_tokens = attempt.usage.output_tokens;
            input.cached_input_tokens = attempt.usage.cached_input_tokens;
            input.metrics =
                serde_json::to_value(&attempt.usage.metrics).expect("decimal metrics serialize");
            let mut dimensions = attempt.usage.dimensions.clone();
            dimensions.insert("instance_name".into(), settings.instance_name.clone());
            dimensions.insert("instance_id".into(), settings.instance_id.to_string());
            dimensions.insert("parent_request_id".into(), settlement.request_id.clone());
            dimensions.insert("billable".into(), attempt.billable.to_string());
            input.dimensions =
                serde_json::to_value(dimensions).expect("string dimensions serialize");
            input.cost = attempt.cost;
            input.usage_source = match attempt.source {
                UsageSource::Upstream => "upstream",
                UsageSource::Estimated => "estimated",
            }
            .into();
            if let Err(error) = host.services.store.record_usage(&input).await {
                tracing::error!(request_id = %input.request_id, error = %error, "persist fallback usage failed");
            }
        }
    }
}

impl CaptureSink for AppHost {
    fn record<'a>(&'a self, capture: &'a gproxy_core::host::Capture) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let policy = crate::logging::Policy::read(&self.services.control.current().settings);
            if !policy.upstream {
                return;
            }
            let input =
                gproxy_store::records::CaptureInput {
                    request_id: capture.request_id.clone(),
                    at: unix_now(),
                    provider_id: capture.provider_id,
                    credential_id: capture.credential_id.map(|credential| credential.0),
                    upstream_url: capture
                        .upstream_url
                        .as_deref()
                        .map(|url| crate::logging::redaction::url_string(url, policy.redact)),
                    request_method: capture.request_method.as_ref().map(ToString::to_string),
                    request_headers: capture.request_headers.as_ref().map(|headers| {
                        crate::logging::redaction::headers_json(headers, policy.redact)
                    }),
                    response_status: capture.response_status.map(|status| status.as_u16()),
                    response_headers: capture.response_headers.as_ref().map(|headers| {
                        crate::logging::redaction::headers_json(headers, policy.redact)
                    }),
                    request_body: policy.upstream_body.then(|| {
                        crate::logging::redaction::body_bytes(&capture.request_body, policy.redact)
                    }),
                    response_body: capture
                        .response_body
                        .as_ref()
                        .filter(|_| policy.upstream_body)
                        .map(|body| crate::logging::redaction::body_bytes(body, policy.redact)),
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
