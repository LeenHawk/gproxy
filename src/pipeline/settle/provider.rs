//! §17 settlement for the provider-shaped billable ops that are NOT
//! content-generation: compact content, embeddings, rerank, and image
//! generation. Buffered JSON responses settle inline; image SSE responses use
//! a bounded relay guard so they can stream without losing final usage. The
//! content-generation settle path ([`super::SettleCtx`]) is untouched.
//!
//! Pricing uses the normalized per-million-token rates from response `usage`.
//! Image operations require upstream token usage as well; a response without
//! it settles at zero rather than guessing from the number of returned images.

use bytes::Bytes;
use serde_json::Value;

use crate::app::AppState;
use crate::billing::{self, UsageRecord, price};
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::protocol::{Operation, OperationKey, Provider as Family};
use crate::usage::{Ended, UsageSource, extract};
use crate::util::time::unix_now;

mod image_sse;
mod stream;
pub(crate) use stream::StreamGuard;

/// Whether this op settles here (non-content billable operations). Lets the
/// caller skip spawning a settle task for every other buffered success.
pub(crate) fn billable(op: Option<OperationKey>) -> bool {
    matches!(
        op.map(|o| o.operation()),
        Some(
            Operation::CompactContent
                | Operation::CreateEmbedding
                | Operation::WebSearch
                | Operation::Rerank
                | Operation::CreateSpeech
                | Operation::CreateTranscription
                | Operation::CreateTranslation
                | Operation::CreateImage
                | Operation::EditImage
                | Operation::RetrieveVideo
        )
    )
}

pub(crate) fn should_settle(op: Option<OperationKey>, body: &[u8]) -> bool {
    if !billable(op) {
        return false;
    }
    if !op.is_some_and(|op| op.operation() == Operation::RetrieveVideo) {
        return true;
    }
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("status")?.as_str().map(str::to_owned))
        .is_some_and(|status| status == "completed")
}

/// Provider settlement facts fixed before a response body is detached or
/// exposed as a stream. This keeps pricing, pending refunds, and limiter targets
/// stable if the control plane changes while a long image stream is in flight.
pub(super) struct Captured {
    pub(super) state: AppState,
    pub(super) ctx: RequestCtx,
    pub(super) cand: Candidate,
    usage_family: Family,
    pricing: billing::price::Pricing,
    actual_service_tier: Option<String>,
    quota_scopes: Vec<(crate::store::persistence::records::Scope, i64)>,
    token_rlt_ids: Vec<i64>,
}

impl Captured {
    pub(super) fn new(
        state: &AppState,
        ctx: &RequestCtx,
        cand: &Candidate,
        usage_family: Family,
        actual_service_tier: Option<&str>,
    ) -> Self {
        let identity = ctx.identity.as_deref();
        let (pricing, quota_scopes, token_rlt_ids) = {
            let cp = state.cp();
            let mut pricing =
                billing::pending::resolve_pricing(&cp, cand.provider.id, &cand.upstream_model_id)
                    .pricing
                    .with_service_tier(billing::price::request_service_tier(&ctx.body).as_deref());
            billing::price::apply_actual_service_tier(&mut pricing, actual_service_tier);
            let (scopes, token_rlt_ids) = identity.map_or_else(
                || (Vec::new(), Vec::new()),
                |identity| {
                    let name = ctx.route_name.as_deref().unwrap_or(&cand.provider.name);
                    (
                        crate::pipeline::authz::quota_scopes(&cp, identity),
                        crate::pipeline::authz::token_limit_ids(&cp, identity, name),
                    )
                },
            );
            (pricing, scopes, token_rlt_ids)
        };
        Self {
            state: state.clone(),
            ctx: ctx.clone(),
            cand: cand.clone(),
            usage_family,
            pricing,
            actual_service_tier: actual_service_tier
                .and_then(billing::price::normalize_service_tier),
            quota_scopes,
            token_rlt_ids,
        }
    }
}

/// Settle a provider-shaped operation and return the same authoritative result
/// that was reconciled and persisted.
pub(crate) async fn settle(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    body: Bytes,
    usage_family: Family,
    actual_service_tier: Option<&str>,
) -> super::Settlement {
    let captured = Captured::new(state, ctx, cand, usage_family, actual_service_tier);
    settle_ended(&captured, &body, Ended::Complete).await
}

async fn settle_ended(captured: &Captured, body: &Bytes, ended: Ended) -> super::Settlement {
    let Captured {
        state,
        ctx,
        cand,
        usage_family,
        pricing,
        actual_service_tier,
        quota_scopes,
        token_rlt_ids,
    } = captured;
    let op = ctx
        .op
        .expect("provider settlement requires a classified operation");
    let is_embedding = matches!(op.operation(), Operation::CreateEmbedding);
    let is_rerank = matches!(op.operation(), Operation::Rerank);
    let is_search = matches!(op.operation(), Operation::WebSearch);
    let is_speech = matches!(op.operation(), Operation::CreateSpeech);
    let is_transcription = matches!(op.operation(), Operation::CreateTranscription);
    let is_translation = matches!(op.operation(), Operation::CreateTranslation);
    let is_compact = matches!(op.operation(), Operation::CompactContent);
    let is_image = matches!(
        op.operation(),
        Operation::CreateImage | Operation::EditImage
    );
    let is_video = matches!(op.operation(), Operation::RetrieveVideo);
    if !is_embedding
        && !is_rerank
        && !is_search
        && !is_speech
        && !is_transcription
        && !is_translation
        && !is_compact
        && !is_image
        && !is_video
    {
        unreachable!("provider settlement called for a non-billable operation");
    }

    let identity = ctx.identity.as_deref();
    let (parsed, parse_error): (Option<Value>, Option<serde_json::Error>) =
        match serde_json::from_slice(body) {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error)),
        };
    // Image responses may arrive as a buffered transcript or through the
    // streaming relay guard. Decode the completed event in either case.
    let stream_frames = ((is_image || is_transcription) && parsed.is_none())
        .then(|| super::frames::decode(body).ok())
        .flatten()
        .filter(|frames| !frames.is_empty());
    if parsed.is_none() && stream_frames.is_none() {
        tracing::warn!(
            request_id = %ctx.request_id,
            provider = %cand.provider.name,
            upstream_model = %cand.upstream_model_id,
            error = %parse_error.expect("failed JSON parse has an error"),
            "billable provider response parse failed; using zero usage"
        );
    }
    let extracted = parsed
        .as_ref()
        .and_then(|value| {
            if is_image {
                extract::from_image_response(*usage_family, value)
            } else if is_rerank {
                extract::from_rerank_response(*usage_family, value)
            } else if is_transcription {
                extract::from_transcription_response(value)
            } else if is_video {
                video_usage(value)
            } else {
                extract::from_response(*usage_family, value)
            }
        })
        .or_else(|| {
            stream_frames.as_deref().and_then(|frames| {
                if is_transcription {
                    extract::from_transcription_stream_frames(frames)
                } else {
                    extract::from_image_stream_frames(*usage_family, frames)
                }
            })
        });
    if (parsed.is_some() || stream_frames.is_some()) && extracted.is_none() {
        let operation = if is_compact {
            "compact_content"
        } else if is_embedding {
            "create_embedding"
        } else if is_search {
            "web_search"
        } else if is_rerank {
            "rerank"
        } else if is_speech {
            "create_speech"
        } else if is_transcription {
            "create_transcription"
        } else if is_translation {
            "create_translation"
        } else if matches!(op.operation(), Operation::EditImage) {
            "edit_image"
        } else {
            "create_image"
        };
        tracing::warn!(
            request_id = %ctx.request_id,
            provider = %cand.provider.name,
            upstream_model = %cand.upstream_model_id,
            operation,
            "provider usage missing; using zero usage"
        );
    }
    let source = if extracted.is_some() {
        UsageSource::Upstream
    } else {
        UsageSource::Estimated
    };
    let mut usage = extracted.unwrap_or_else(|| {
        let produced = parsed
            .as_ref()
            .map(|value| crate::tokenize::harvest(&serde_json::to_vec(value).unwrap_or_default()).0)
            .unwrap_or_default()
            .join("\n");
        super::ladder::local_estimate(
            state,
            &cand.upstream_model_id,
            cand.provider.settings_json.get("tokenizer_map"),
            &ctx.body,
            &produced,
        )
    });
    if is_speech && let Some(seconds) = speech_seconds(body, &ctx.body) {
        usage.set_metric("audio_seconds", seconds);
    }
    usage
        .dimensions
        .insert("operation".into(), super::enum_str(&op.operation()));
    let body_service_tier = parsed
        .as_ref()
        .and_then(price::response_service_tier_from_value)
        .or_else(|| {
            stream_frames.as_deref().and_then(|frames| {
                frames
                    .iter()
                    .rev()
                    .find_map(|frame| price::response_service_tier(frame.data.as_bytes()))
            })
        });
    let mut settled_pricing = pricing.clone();
    price::apply_actual_service_tier(
        &mut settled_pricing,
        body_service_tier
            .as_deref()
            .or(actual_service_tier.as_deref()),
    );
    let cost = price::cost(&usage, &settled_pricing);
    let settlement = super::Settlement {
        usage: usage.clone(),
        cost,
        source,
        ended,
    };

    if is_video {
        let dedupe_key = format!(
            "video_settle:{}:{}:{}",
            cand.provider.id,
            cand.credential.id,
            blake3::hash(ctx.path.as_bytes()).to_hex()
        );
        let seen = state
            .cache
            .incr(
                &dedupe_key,
                1,
                Some(std::time::Duration::from_secs(7 * 24 * 3600)),
            )
            .await
            .unwrap_or(1);
        if seen > 1 {
            return settlement;
        }
    }

    // Keep provider-shaped operations on the same billing and token-counter
    // path as content generation. Recording may be disabled, but quota and
    // limiter reconciliation is always required.
    super::reconcile::reconcile_target(
        super::reconcile::ReconcileTarget {
            state,
            request_id: &ctx.request_id,
            credential_id: cand.credential.id,
            pending_micros: ctx.pending_micros,
            quota_scopes,
            token_rlt_ids,
        },
        &usage,
        cost,
    )
    .await;

    let operation = super::enum_str(&op.operation());
    let kind = super::enum_str(&op.kind());
    let rec = UsageRecord {
        request_id: &ctx.request_id,
        at: unix_now(),
        route_name: ctx.route_name.as_deref(),
        provider_id: Some(cand.provider.id),
        credential_id: Some(cand.credential.id),
        org_id: identity.map(|i| i.user.org_id),
        team_id: identity.and_then(|i| i.user.team_id),
        user_id: identity.map(|i| i.user.id),
        user_key_id: identity.map(|i| i.user_key.id),
        thread_id: ctx
            .headers
            .get("thread-id")
            .and_then(|value| value.to_str().ok()),
        operation: &operation,
        kind: &kind,
        model: Some(&cand.upstream_model_id),
        usage: &usage,
        cost,
        latency_ms: 0,
        source,
        ended,
    };
    // §8-E: `enable_usage` gates the usage row only.
    if state.cp().log_settings.enable_usage
        && let Err(e) = billing::record_success(state.persistence.as_ref(), rec).await
    {
        tracing::warn!(request_id = %ctx.request_id, error = %e, "provider settle write failed");
    }
    tracing::debug!(
        request_id = %ctx.request_id,
        provider = %cand.provider.name,
        upstream_model = %cand.upstream_model_id,
        usage_source = %source,
        ended = %ended,
        image_output_tokens = usage.image_output,
        tokens = usage.total(),
        cost = %cost,
        "provider usage settled"
    );
    settlement
}

fn video_usage(value: &Value) -> Option<crate::usage::NormalizedUsage> {
    use std::str::FromStr as _;
    let usage = value.get("usage").and_then(Value::as_object);
    let mut normalized = crate::usage::NormalizedUsage::default();
    if let Some(tokens) = usage
        .and_then(|usage| usage.get("video_tokens"))
        .and_then(Value::as_u64)
    {
        normalized.set_metric("video_tokens", rust_decimal::Decimal::from(tokens));
    }
    let seconds = usage
        .and_then(|usage| usage.get("seconds"))
        .or_else(|| value.get("seconds"))
        .or_else(|| value.get("duration"))
        .and_then(|seconds| match seconds {
            Value::String(seconds) => rust_decimal::Decimal::from_str(seconds).ok(),
            Value::Number(seconds) => rust_decimal::Decimal::from_str(&seconds.to_string()).ok(),
            _ => None,
        });
    if let Some(seconds) = seconds {
        normalized.set_metric("video_seconds", seconds);
    }
    for key in ["resolution", "size", "quality"] {
        if let Some(dimension) = value.get(key).and_then(Value::as_str) {
            normalized.dimensions.insert(key.into(), dimension.into());
        }
    }
    for key in ["with_audio", "generate_audio"] {
        if let Some(dimension) = value.get(key).and_then(Value::as_bool) {
            normalized
                .dimensions
                .insert("with_audio".into(), dimension.to_string());
        }
    }
    Some(normalized)
}

fn speech_seconds(body: &[u8], request: &[u8]) -> Option<rust_decimal::Decimal> {
    use std::str::FromStr as _;
    let format = serde_json::from_slice::<Value>(request)
        .ok()
        .and_then(|value| value.get("response_format")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| "mp3".into());
    let (bytes, bytes_per_second) = match format.as_str() {
        // OpenAI-compatible PCM is 24 kHz, mono, signed 16-bit little-endian.
        "pcm" => (body.len(), 48_000usize),
        "wav" if body.len() >= 44 && &body[..4] == b"RIFF" => {
            let byte_rate = u32::from_le_bytes(body[28..32].try_into().ok()?) as usize;
            (body.len().saturating_sub(44), byte_rate)
        }
        _ => return None,
    };
    if bytes_per_second == 0 {
        return None;
    }
    rust_decimal::Decimal::from_str(&format!(
        "{}.{:06}",
        bytes / bytes_per_second,
        (bytes % bytes_per_second) * 1_000_000 / bytes_per_second
    ))
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Operation, OperationKey, Provider};

    #[test]
    fn video_settlement_waits_for_completed_poll_and_extracts_dimensions() {
        let op = Some(OperationKey::provider(
            Operation::RetrieveVideo,
            Provider::OpenAi,
        ));
        assert!(!should_settle(op, br#"{"id":"job","status":"pending"}"#));
        assert!(should_settle(
            op,
            br#"{"id":"job","status":"completed","usage":{"video_tokens":10,"seconds":5},"resolution":"1080p"}"#,
        ));
        let usage = video_usage(&serde_json::json!({
            "status": "completed",
            "usage": {"video_tokens": 10, "seconds": 5},
            "resolution": "1080p",
        }))
        .unwrap();
        assert_eq!(
            usage.metric("video_tokens"),
            rust_decimal::Decimal::from(10)
        );
        assert_eq!(
            usage.metric("video_seconds"),
            rust_decimal::Decimal::from(5)
        );
        assert_eq!(usage.dimensions["resolution"], "1080p");
    }

    #[test]
    fn speech_duration_supports_pcm_and_wav_without_touching_other_formats() {
        let pcm = vec![0u8; 96_000];
        assert_eq!(
            speech_seconds(&pcm, br#"{"response_format":"pcm"}"#),
            Some(rust_decimal::Decimal::from(2))
        );
        assert_eq!(
            speech_seconds(b"ID3", br#"{"response_format":"mp3"}"#),
            None
        );
    }
}
