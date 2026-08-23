use bytes::Bytes;
use gproxy_channel_api::{Channel, NormalizedUsage};
use gproxy_protocol::SettleMode;
use rust_decimal::Decimal;

use crate::control::Pricing;
use crate::funnel::FunnelCtx;
use crate::host::{CacheBackend, Capture, CaptureSink, Host, UsageSink};
use crate::usage::{Ended, Settlement, UsageSource};

pub(crate) async fn complete<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    status: Option<http::StatusCode>,
    response_body: Option<Bytes>,
    record_usage: bool,
    usage: Option<NormalizedUsage>,
    ended: Ended,
) {
    let latency_ms = ctx.started.elapsed().as_millis() as u64;
    let unique = match (record_usage, ctx.dedupe_key.as_deref()) {
        (true, Some(key)) => host.cache().incr(key, 1, None).await == 1,
        (true, None) => true,
        (false, _) => false,
    };
    if unique {
        if ctx.pricing.is_none() {
            tracing::warn!(
                request_id = %ctx.request_id,
                provider_id = ctx.target.provider.id,
                upstream_model = %ctx.target.upstream_model,
                "pricing missing; settling at zero cost"
            );
        }
        let source = if usage.is_some() {
            UsageSource::Upstream
        } else {
            UsageSource::Estimated
        };
        let usage = usage.unwrap_or_default();
        host.usage()
            .record(&Settlement {
                request_id: ctx.request_id.clone(),
                provider_id: ctx.target.provider.id,
                credential_id: ctx.target.credential,
                upstream_model: ctx.target.upstream_model.clone(),
                cost: cost(&usage, ctx.pricing.as_ref()),
                usage,
                source,
                ended,
                latency_ms,
            })
            .await;
    }
    host.capture()
        .record(&Capture {
            request_id: ctx.request_id.clone(),
            upstream_url: ctx.upstream_url.clone(),
            request_body: ctx.request_body.clone(),
            response_status: status,
            response_body,
        })
        .await;
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = ctx.target.provider.id,
        credential_id = ctx.target.credential.0,
        operation = ?ctx.key.operation,
        ended = ?ended,
        latency_ms,
        "request.completed"
    );
}

pub(crate) fn usage(
    channel: &dyn Channel,
    ctx: &FunnelCtx,
    body: &[u8],
) -> (bool, Option<NormalizedUsage>) {
    match ctx.settle {
        SettleMode::Free => (false, None),
        SettleMode::OnResponse => (true, channel.extract_usage(ctx.key, body)),
        SettleMode::OnCompletedStatus => {
            let completed = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|body| body.get("status")?.as_str().map(str::to_owned))
                .is_some_and(|status| status == "completed");
            (
                completed,
                completed
                    .then(|| channel.extract_usage(ctx.key, body))
                    .flatten(),
            )
        }
    }
}

fn cost(usage: &NormalizedUsage, pricing: Option<&Pricing>) -> Decimal {
    let Some(pricing) = pricing else {
        return Decimal::ZERO;
    };
    let million = Decimal::from(1_000_000_u64);
    let cached = usage.cached_input_tokens.min(usage.input_tokens);
    let uncached = usage.input_tokens - cached;
    let mut total = Decimal::from(uncached) * pricing.input_per_million / million;
    total += Decimal::from(cached)
        * pricing
            .cached_input_per_million
            .unwrap_or(pricing.input_per_million)
        / million;
    total += Decimal::from(usage.output_tokens) * pricing.output_per_million / million;
    for (metric, amount) in &usage.metrics {
        if let Some(rate) = pricing.metric_rates.get(metric) {
            total += *amount * *rate;
        }
    }
    total
}
