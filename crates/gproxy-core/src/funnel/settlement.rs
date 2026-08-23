use bytes::Bytes;
use gproxy_channel_api::{Channel, NormalizedUsage, UsageCtx};
use gproxy_protocol::SettleMode;
use rust_decimal::Decimal;

use crate::control::Pricing;
use crate::host::{CacheBackend, Capture, CaptureSink, Host, UsageSink};
use crate::usage::{Ended, Settlement, UsageSource};

use super::FunnelCtx;

pub(super) struct Completion {
    pub status: Option<http::StatusCode>,
    pub response_body: Option<Bytes>,
    pub estimated_output_chars: Option<u64>,
    pub record_usage: bool,
    pub usage: Option<NormalizedUsage>,
    pub ended: Ended,
}

pub(crate) async fn complete<H: Host>(host: &H, ctx: &FunnelCtx, completion: Completion) {
    let Completion {
        status,
        response_body,
        estimated_output_chars,
        record_usage,
        usage,
        ended,
    } = completion;
    let latency_ms = ctx.started.elapsed().as_millis() as u64;
    let unique = match (record_usage, ctx.dedupe_key.as_deref()) {
        (true, Some(key)) => host.cache().incr(key, 1, None).await == 1,
        (true, None) => true,
        (false, _) => false,
    };
    if unique && ctx.pricing.is_none() {
        tracing::warn!(
            request_id = %ctx.request_id,
            provider_id = ctx.target.provider.id,
            upstream_model = %ctx.target.upstream_model,
            "pricing missing; settling at zero cost"
        );
    }
    let source = if unique && usage.is_some() {
        UsageSource::Upstream
    } else {
        UsageSource::Estimated
    };
    let usage = if unique {
        usage.unwrap_or_else(|| {
            estimate(
                &ctx.request_body,
                response_body.as_deref(),
                estimated_output_chars,
                ctx.source_key.map(|key| key.operation),
            )
        })
    } else {
        NormalizedUsage::default()
    };
    let settlement = Settlement {
        request_id: ctx.request_id.clone(),
        provider_id: ctx.target.provider.id,
        credential_id: ctx.target.credential,
        upstream_model: ctx.target.upstream_model.clone(),
        cost: if unique {
            cost(&usage, ctx.pricing.as_ref())
        } else {
            Decimal::ZERO
        },
        usage,
        source,
        ended,
        latency_ms,
    };
    if unique {
        host.usage().record(&settlement).await;
    }
    if ctx.admitted {
        host.finish_admission(&ctx.request_id, Some(&settlement))
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
        operation = ?ctx.source_key.map(|key| key.operation),
        source_framing = ?ctx.source_framing,
        target_framing = ?ctx.target_framing,
        surface = ctx.surface_label.unwrap_or(""),
        ended = ?ended,
        latency_ms,
        "request.completed"
    );
}

fn estimate(
    request_body: &[u8],
    response_body: Option<&[u8]>,
    output_chars: Option<u64>,
    operation: Option<gproxy_protocol::Operation>,
) -> NormalizedUsage {
    let input_chars = utf8_chars(request_body);
    let output_chars = output_chars
        .or_else(|| response_body.map(utf8_chars))
        .unwrap_or_default();
    let mut usage = NormalizedUsage {
        input_tokens: input_chars.div_ceil(2),
        output_tokens: output_chars.div_ceil(2),
        ..Default::default()
    };
    if operation == Some(gproxy_protocol::Operation::WebSearch) {
        usage.metrics.insert("web_searches".into(), Decimal::ONE);
    }
    usage
}

pub(super) fn utf8_chars(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .filter(|byte| **byte & 0b1100_0000 != 0b1000_0000)
        .count() as u64
}

pub(crate) fn usage(
    channel: &dyn Channel,
    ctx: &FunnelCtx,
    response_headers: &http::HeaderMap,
    body: &[u8],
) -> (bool, Option<NormalizedUsage>) {
    let extract = || {
        channel.extract_usage(UsageCtx {
            key: ctx.key.expect("billable funnel has an operation"),
            request_body: &ctx.request_body,
            response_headers,
            response_body: body,
        })
    };
    match ctx.settle {
        SettleMode::Free => (false, None),
        SettleMode::OnResponse => (true, extract()),
        SettleMode::OnCompletedStatus => {
            let completed = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|body| body.get("status")?.as_str().map(str::to_owned))
                .is_some_and(|status| status == "completed");
            (completed, completed.then(extract).flatten())
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
