use web_time::Instant;

use bytes::Bytes;
use gproxy_channel_api::{Channel, Disposition, NormalizedUsage, ResponseShapeCtx, StreamDecoder};
use gproxy_protocol::{OperationKey, SettleMode, StreamFraming};

use crate::Shared;
use crate::boundary::{ExecOutcome, ResponseBody};
use crate::control::{Pricing, Target};
use crate::host::Host;
use crate::usage::Ended;

pub(crate) mod error;
pub(crate) mod health;
mod session;
pub(crate) use session::realtime;
mod settlement;
mod socket;
mod stream;
pub(crate) mod synth;

pub(crate) use self::socket::{bridged_websocket, websocket};
use self::stream::FunnelStream;

#[derive(Debug)]
pub(crate) struct Settled(());

pub(crate) struct FunnelCtx {
    pub upstream_started_at_ms: Option<i64>,
    pub request_id: String,
    pub target: Target,
    pub credential_version: Option<u64>,
    /// Caller-facing operation key; differs from `key` when a pair transforms.
    pub source_key: Option<OperationKey>,
    /// Channel-native upstream operation key used for usage extraction.
    pub key: Option<OperationKey>,
    pub source_framing: StreamFraming,
    pub target_framing: StreamFraming,
    pub settle: SettleMode,
    pub pricing: Option<Pricing>,
    pub started: Instant,
    pub upstream_url: Option<String>,
    pub request_method: Option<http::Method>,
    pub request_body: Bytes,
    pub request_headers: Option<http::HeaderMap>,
    pub client_headers: http::HeaderMap,
    pub requested_model: Option<String>,
    pub response_headers: Option<http::HeaderMap>,
    pub dedupe_key: Option<String>,
    pub owner_user_id: Option<i64>,
    pub resource: Option<(&'static str, String)>,
    pub admitted: bool,
    pub surface_label: Option<&'static str>,
    pub traffic_policy: Option<gproxy_channel_api::TrafficPolicyConfig>,
    pub traffic_blacklist: Option<gproxy_channel_api::TrafficBlacklistConfig>,
}

impl FunnelCtx {
    fn capture_attribution(&self) -> (Option<i64>, Option<crate::host::CredentialId>) {
        self.upstream_url.as_ref().map_or((None, None), |_| {
            (Some(self.target.provider.id), Some(self.target.credential))
        })
    }
}

pub(crate) struct BufferedRelay {
    pub response: http::Response<Bytes>,
    pub usage: Option<NormalizedUsage>,
    pub actual_service_tier: Option<String>,
    pub capture_body: Option<Bytes>,
    pub outward_ready: bool,
}

impl BufferedRelay {
    pub(crate) fn native(response: http::Response<Bytes>) -> Self {
        Self {
            response,
            usage: None,
            actual_service_tier: None,
            capture_body: None,
            outward_ready: false,
        }
    }
}

pub(crate) async fn buffered<H: Host>(
    host: Shared<H>,
    channel: &dyn Channel,
    control: Option<&dyn crate::control::ControlPlane>,
    session_channel: Option<std::sync::Arc<dyn Channel>>,
    ctx: FunnelCtx,
    relay: BufferedRelay,
    disposition: Disposition,
) -> ExecOutcome {
    if ctx.settle == SettleMode::OnSessionEnd {
        return session::buffered(
            host,
            session_channel.expect("session funnel has its channel owner"),
            control.expect("session funnel has its control plane"),
            ctx,
            relay,
            disposition,
        )
        .await;
    }
    let BufferedRelay {
        response,
        usage: usage_override,
        actual_service_tier,
        capture_body,
        outward_ready,
    } = relay;
    let (parts, body) = response.into_parts();
    let actual_service_tier = actual_service_tier
        .or_else(|| crate::control::response_service_tier(&parts.headers, &body));
    let (record_usage, extracted) = if usage_override.is_some() {
        (matches!(ctx.settle, SettleMode::OnResponse), None)
    } else {
        settlement::usage(channel, &ctx, &parts.headers, &body)
    };
    let usage = usage_override.or(extracted);
    crate::execution::resource::observe(
        host.as_ref(),
        channel,
        &ctx,
        parts.status,
        &parts.headers,
        capture_body.as_deref().unwrap_or(&body),
    )
    .await;
    let upstream_status = parts.status;
    let upstream_headers = parts.headers;
    let (status, headers, outward, disposition) = if outward_ready {
        (upstream_status, upstream_headers, body.clone(), disposition)
    } else {
        let shaped = ctx.key.map_or_else(
            || Ok(body.clone()),
            |key| {
                channel.shape_response(ResponseShapeCtx {
                    key,
                    status: upstream_status,
                    headers: &upstream_headers,
                    body: &body,
                })
            },
        );
        let shaped = if upstream_status.is_success()
            && let Some(key) = ctx.key
        {
            shaped.map(|body| {
                crate::process::apply_response(
                    &ctx.target.rules.process,
                    key,
                    process_models(&ctx),
                    &ctx.client_headers,
                    body,
                )
            })
        } else {
            shaped
        };
        transform_buffered(&ctx, upstream_status, upstream_headers, shaped, disposition)
    };
    let completion = settlement::Completion {
        status: Some(upstream_status),
        response_body: Some(capture_body.unwrap_or_else(|| body.clone())),
        estimated_output_chars: None,
        record_usage,
        usage,
        actual_service_tier,
        cost_override: None,
        capture_response: true,
        ended: Ended::Complete,
    };
    let (status, headers, body, disposition) =
        synth::outward_body(&ctx, status, headers, outward, disposition);
    let headers = outward_headers(&ctx, headers);
    settle_buffered(host, ctx, completion).await;
    ExecOutcome {
        status,
        headers,
        body,
        disposition,
        _settled: Settled(()),
    }
}

/// The spawner is the settle policy: native hosts release the response before
/// the usage row lands, exactly as streams do; edge hosts settle inline.
async fn settle_buffered<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    completion: settlement::Completion,
) {
    match host.spawner() {
        Some(spawner) => {
            let task_host = host.clone();
            let permit = spawner.reserve_settlement().await;
            spawner.spawn(Box::pin(async move {
                settlement::complete(task_host.as_ref(), &ctx, completion).await;
                drop(permit);
            }));
        }
        None => settlement::complete(host.as_ref(), &ctx, completion).await,
    }
}

fn process_models(ctx: &FunnelCtx) -> crate::process::RuleModels<'_> {
    crate::process::RuleModels::new(
        &ctx.target.upstream_model,
        ctx.requested_model
            .as_deref()
            .filter(|model| *model != ctx.target.upstream_model),
    )
}

fn transform_buffered(
    ctx: &FunnelCtx,
    status: http::StatusCode,
    mut headers: http::HeaderMap,
    shaped: Result<Bytes, gproxy_channel_api::ChannelError>,
    disposition: Disposition,
) -> (http::StatusCode, http::HeaderMap, Bytes, Disposition) {
    let body = match shaped {
        Ok(body) => body,
        Err(error) => return transform_error(crate::error::CoreError::Channel(error)),
    };
    let (Some(source), Some(target)) = (ctx.source_key, ctx.key) else {
        return (status, headers, body, disposition);
    };
    if source == target || !status.is_success() {
        return (status, headers, body, disposition);
    }
    match gproxy_transform::response(source, target, body) {
        Ok(body) => {
            headers.remove(http::header::CONTENT_LENGTH);
            (status, headers, body, disposition)
        }
        Err(error) => transform_error(crate::error::CoreError::Transform(error.to_string())),
    }
}

fn transform_error(
    error: crate::error::CoreError,
) -> (http::StatusCode, http::HeaderMap, Bytes, Disposition) {
    let headers = http::HeaderMap::from_iter([(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    )]);
    (
        error.status(),
        headers,
        Bytes::from(error.body_json().to_string()),
        Disposition::Terminal,
    )
}

/// A detached settlement's backlog slot, reserved while the response is
/// still being produced so `Drop` never has to wait for one.
async fn reserve_settlement<H: Host>(host: &Shared<H>) -> Option<crate::host::SettlementPermit> {
    match host.spawner() {
        Some(spawner) => Some(spawner.reserve_settlement().await),
        None => None,
    }
}

pub(crate) async fn streaming<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    response: http::Response<crate::boundary::ByteStream>,
    disposition: Disposition,
    decoder: Option<Box<dyn StreamDecoder>>,
) -> ExecOutcome {
    let (mut parts, body) = response.into_parts();
    if ctx.source_key != ctx.key || decoder.is_some() {
        frame_headers(&mut parts.headers, ctx.source_framing);
    }
    parts.headers = outward_headers(&ctx, parts.headers);
    let permit = reserve_settlement(&host).await;
    let body = FunnelStream::new(body, decoder, host, ctx, parts.status, permit);
    ExecOutcome {
        status: parts.status,
        headers: parts.headers,
        body: ResponseBody::Stream(Box::pin(body)),
        disposition,
        _settled: Settled(()),
    }
}

pub(crate) async fn free_buffered<H: Host>(
    host: &H,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
    disposition: Disposition,
) -> ExecOutcome {
    settlement::complete(
        host,
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: Some(body.clone()),
            estimated_output_chars: None,
            record_usage: false,
            usage: None,
            actual_service_tier: None,
            cost_override: None,
            capture_response: true,
            ended: Ended::Complete,
        },
    )
    .await;
    let headers = outward_headers(&ctx, headers);
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Full(body),
        disposition,
        _settled: Settled(()),
    }
}

pub(crate) async fn free_uncaptured_buffered<H: Host>(
    host: &H,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
    disposition: Disposition,
) -> ExecOutcome {
    settlement::complete(
        host,
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: None,
            estimated_output_chars: None,
            record_usage: false,
            usage: None,
            actual_service_tier: None,
            cost_override: None,
            capture_response: false,
            ended: Ended::Complete,
        },
    )
    .await;
    ExecOutcome {
        status,
        headers: outward_headers(&ctx, headers),
        body: ResponseBody::Full(body),
        disposition,
        _settled: Settled(()),
    }
}

pub(crate) async fn local_buffered<H: Host>(
    host: &H,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
    disposition: Disposition,
) -> ExecOutcome {
    settlement::complete(
        host,
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: Some(body.clone()),
            estimated_output_chars: None,
            record_usage: false,
            usage: Some(NormalizedUsage::default()),
            actual_service_tier: None,
            cost_override: Some(rust_decimal::Decimal::ZERO),
            capture_response: true,
            ended: Ended::Complete,
        },
    )
    .await;
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Full(body),
        disposition,
        _settled: Settled(()),
    }
}

pub(crate) async fn free_streaming<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: crate::boundary::ByteStream,
    disposition: Disposition,
) -> ExecOutcome {
    let headers = outward_headers(&ctx, headers);
    let permit = reserve_settlement(&host).await;
    let body = FunnelStream::new(body, None, host, ctx, status, permit);
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Stream(Box::pin(body)),
        disposition,
        _settled: Settled(()),
    }
}

fn frame_headers(headers: &mut http::HeaderMap, framing: StreamFraming) {
    let content_type = match framing {
        StreamFraming::Sse => Some("text/event-stream"),
        StreamFraming::JsonArray => Some("application/json"),
        StreamFraming::WebSocket => None,
    };
    if let Some(content_type) = content_type {
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static(content_type),
        );
        headers.remove(http::header::CONTENT_LENGTH);
        if framing == StreamFraming::Sse {
            headers.insert(
                http::header::CACHE_CONTROL,
                http::HeaderValue::from_static("no-cache"),
            );
        }
    }
}

pub(super) fn outward_headers(ctx: &FunnelCtx, headers: http::HeaderMap) -> http::HeaderMap {
    match (&ctx.traffic_policy, &ctx.traffic_blacklist) {
        (Some(policy), Some(blacklist)) => {
            crate::execution::forwarding::response_headers(headers, policy, blacklist)
        }
        _ => headers,
    }
}

pub(crate) async fn interrupted<H: Host>(
    host: &H,
    channel: &dyn Channel,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
) {
    let (record_usage, usage) = settlement::usage(channel, &ctx, &headers, &body);
    let actual_service_tier = crate::control::response_service_tier(&headers, &body);
    settlement::complete(
        host,
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: Some(body),
            estimated_output_chars: None,
            record_usage,
            usage,
            actual_service_tier,
            cost_override: None,
            capture_response: true,
            ended: Ended::Interrupted,
        },
    )
    .await;
}

pub(crate) async fn complete_stream<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    status: http::StatusCode,
    usage: Option<NormalizedUsage>,
    actual_service_tier: Option<String>,
    estimated_output_chars: Option<u64>,
    ended: Ended,
) {
    let record_usage = matches!(ctx.settle, SettleMode::OnResponse);
    settlement::complete(
        host.as_ref(),
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: None,
            estimated_output_chars,
            record_usage,
            usage,
            actual_service_tier,
            cost_override: None,
            capture_response: true,
            ended,
        },
    )
    .await;
}
