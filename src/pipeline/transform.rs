//! M2 transform-dispatch step: per-candidate plan (passthrough vs transform),
//! effective upstream request parts (path/query/body/headers incl. process
//! rules + model rewrite), and response-direction conversion.

use std::collections::HashMap;

use bytes::Bytes;
use http::HeaderMap;

use crate::app::snapshot::ControlPlaneSnapshot;
use crate::pipeline::classify::peek_model;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::error::PipelineError;
use crate::process;
use crate::protocol::{
    self, ContentGenerationKind, Operation, OperationKey, OperationKind, Provider,
};
use crate::transform::routing::RoutingDecision;
use crate::transform::stream_adapter::SseTransformer;
use crate::transform::{self, TransformContext, TransformError, TransformPair, dispatch, routing};

/// Per-candidate transform plan. `Unsupported` decisions surface as errors
/// from [`plan_for`], not as variants — the loop treats them per-policy.
#[derive(Debug, Clone)]
pub enum TransformPlan {
    Passthrough,
    Transform {
        /// inbound → upstream
        request_pair: TransformPair,
        /// upstream → inbound
        response_pair: TransformPair,
        source: OperationKey,
        target: OperationKey,
    },
    /// Force a STREAMING upstream and collapse the streamed response back into a
    /// single object for a non-stream client. codex/kiro upstreams only speak
    /// event-streams, so a non-stream `GenerateContent` client must still stream
    /// the upstream and then aggregate. `*_pair` are `None` when source and
    /// target wire kinds match (only the stream-ness changes, no body convert).
    AggregateStream {
        request_pair: Option<TransformPair>,
        response_pair: Option<TransformPair>,
        source: OperationKey,
        target: OperationKey,
    },
    /// Serve locally — no upstream call (§6.3).
    Local,
}

impl TransformPlan {
    pub fn is_transform(&self) -> bool {
        matches!(self, Self::Transform { .. })
    }

    /// `AggregateStream` forces a streaming upstream + collapse on a non-stream
    /// client.
    pub fn is_aggregate_stream(&self) -> bool {
        matches!(self, Self::AggregateStream { .. })
    }

    /// The op to surface in [`ShapeCtx`](crate::channel::ShapeCtx): the routed
    /// target when one exists, else the inbound op.
    pub fn shape_op(&self, ctx: &RequestCtx) -> OperationKey {
        match self {
            Self::Transform { target, .. } | Self::AggregateStream { target, .. } => *target,
            _ => ctx.op.expect("classified before failover"),
        }
    }

    /// Target wire kind for the stream→object collapse (content-gen only).
    pub fn target_kind(&self) -> Option<ContentGenerationKind> {
        match self {
            Self::AggregateStream { target, .. } | Self::Transform { target, .. } => {
                match target.kind {
                    OperationKind::ContentGeneration(k) => Some(k),
                    OperationKind::Provider(_) => None,
                }
            }
            _ => None,
        }
    }
}

/// Resolve the plan for one candidate.
pub fn plan_for(
    cp: &ControlPlaneSnapshot,
    provider_id: i64,
    source: OperationKey,
) -> Result<TransformPlan, PipelineError> {
    let rules = cp
        .routing_rules_by_provider
        .get(&provider_id)
        .map(|r| r.as_slice())
        .unwrap_or(&[]);
    match routing::decide(rules, source) {
        RoutingDecision::Passthrough => Ok(TransformPlan::Passthrough),
        RoutingDecision::Local => Ok(TransformPlan::Local),
        RoutingDecision::Unsupported => Err(PipelineError::RuleUnsupported),
        RoutingDecision::TransformTo(target) if target == source => Ok(TransformPlan::Passthrough),
        // Force-stream routes (codex/kiro): inbound `GenerateContent` → upstream
        // `StreamGenerateContent`. The upstream only speaks event-streams; stream
        // it and collapse back to one object for non-stream clients. Any body
        // transform here is purely the wire-KIND change (operations normalized to
        // `GenerateContent` for pairing).
        RoutingDecision::TransformTo(target)
            if source.operation == Operation::GenerateContent
                && target.operation == Operation::StreamGenerateContent =>
        {
            let (request_pair, response_pair) = if source.kind == target.kind {
                (None, None)
            } else {
                let src = OperationKey {
                    operation: Operation::GenerateContent,
                    kind: source.kind,
                };
                let tgt = OperationKey {
                    operation: Operation::GenerateContent,
                    kind: target.kind,
                };
                let rp = transform::resolve(src, tgt).map_err(PipelineError::TransformRequest)?;
                let sp = transform::resolve(tgt, src).map_err(PipelineError::TransformRequest)?;
                if !dispatch::is_wired(rp) || !dispatch::is_wired(sp) {
                    return Err(PipelineError::TransformRequest(
                        TransformError::InvalidInput {
                            reason: "aggregate-stream pair not wired for bytes dispatch".to_owned(),
                        },
                    ));
                }
                (Some(rp), Some(sp))
            };
            Ok(TransformPlan::AggregateStream {
                request_pair,
                response_pair,
                source,
                target,
            })
        }
        RoutingDecision::TransformTo(target) => {
            let request_pair =
                transform::resolve(source, target).map_err(PipelineError::TransformRequest)?;
            let response_pair =
                transform::resolve(target, source).map_err(PipelineError::TransformRequest)?;
            if !dispatch::is_wired(request_pair) || !dispatch::is_wired(response_pair) {
                return Err(PipelineError::TransformRequest(
                    TransformError::InvalidInput {
                        reason: "pair not wired for bytes dispatch".to_owned(),
                    },
                ));
            }
            Ok(TransformPlan::Transform {
                request_pair,
                response_pair,
                source,
                target,
            })
        }
    }
}

/// Effective upstream request pieces for one attempt.
pub struct RequestParts {
    pub method: http::Method,
    pub path: String,
    pub query: Option<String>,
    pub body: Bytes,
    /// `Some` when process rules touched headers; otherwise use `ctx.headers`.
    pub headers: Option<HeaderMap>,
}

/// Cross-attempt memo: transformed bodies keyed by (target key, model), plus
/// the lazily-peeked inbound model. The FULL target key matters: rules with
/// `dest_operation` let two targets share a kind (e.g. both
/// `Provider(OpenAi)`) while converting to different operations.
#[derive(Default)]
pub struct AttemptMemo {
    bodies: HashMap<(OperationKey, String), Bytes>,
    inbound_model: Option<Option<String>>,
}

impl AttemptMemo {
    fn inbound_model(&mut self, body: &Bytes) -> Option<String> {
        self.inbound_model
            .get_or_insert_with(|| peek_model(body))
            .clone()
    }
}

/// Build the effective request for one candidate: transform (memoized per
/// (target key, model)), model rewrite, endpoint synthesis, then process rules
/// on the provider-native result.
pub fn request_parts(
    ctx: &RequestCtx,
    cand: &Candidate,
    plan: &TransformPlan,
    rules: Option<&[process::CompiledRule]>,
    memo: &mut AttemptMemo,
) -> Result<RequestParts, PipelineError> {
    let op = ctx.op.expect("classified before failover");
    let (mut parts, target_key) = match plan {
        // Local plans never reach request building — failover serves them.
        TransformPlan::Local => return Err(PipelineError::LocalUnimplemented),
        TransformPlan::Passthrough => {
            let mut path = ctx.path.clone();
            let mut query = ctx.query.clone();
            let mut body = ctx.body.clone();
            // §17: openai-chat-bound streams must request the final usage
            // chunk, or settlement never sees upstream usage. One body parse
            // per streaming request is the accepted cost.
            let include_usage = ctx.stream && is_openai_chat(op.kind);
            // Aggregated-mode member model rewrite. Scoped mode peeked the
            // same model into upstream_model_id, so this stays a no-op there
            // (single memoized model peek; no transform). Body-less ops
            // (models GETs) carry nothing to peek or patch.
            let model_rewrite = op.operation.has_request_body()
                && !cand.upstream_model_id.is_empty()
                && memo.inbound_model(&ctx.body).as_deref()
                    != Some(cand.upstream_model_id.as_str());
            if model_rewrite {
                // Gemini carries the model (+ stream flag) in the PATH; every
                // other family carries it in the body — content AND non-content
                // (embeddings, count_tokens) alike, mirroring the Transform
                // branch's `body_carries_model` split below.
                let model_in_path = matches!(
                    op.kind,
                    OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
                        | OperationKind::Provider(Provider::Gemini)
                );
                if model_in_path {
                    let t = protocol::request_target(op, &cand.upstream_model_id, ctx.stream);
                    path = t.path;
                    if let Some(extra) = t.query {
                        query = Some(merge_query(query.as_deref(), &extra));
                    }
                } else {
                    // passthrough bodies already carry the correct stream flag;
                    // never inject it here (`include_usage` is false for the
                    // non-openai-chat provider ops, so this is a pure rewrite)
                    body = patch_body(&body, Some(&cand.upstream_model_id), false, include_usage)?;
                }
            } else if include_usage {
                body = patch_body(&body, None, false, true)?;
            }
            (
                RequestParts {
                    method: ctx.method.clone(),
                    path,
                    query,
                    body,
                    headers: None,
                },
                op,
            )
        }
        TransformPlan::Transform {
            request_pair,
            source,
            target,
            ..
        } => {
            // Body-less ops (models GETs): nothing to transform or patch;
            // only endpoint synthesis applies.
            if !target.operation.has_request_body() {
                let t = protocol::request_target(*target, &cand.upstream_model_id, ctx.stream);
                (
                    RequestParts {
                        method: t.method.into(),
                        path: t.path,
                        query: t.query,
                        body: ctx.body.clone(),
                        headers: None,
                    },
                    *target,
                )
            } else {
                let key = (*target, cand.upstream_model_id.clone());
                let body = match memo.bodies.get(&key) {
                    Some(b) => b.clone(),
                    None => {
                        let fwd = TransformContext::new(*source, *target);
                        let converted = dispatch::request_bytes(*request_pair, &fwd, &ctx.body)
                            .map_err(PipelineError::TransformRequest)?;
                        let mut converted = Bytes::from(converted);
                        // Gemini targets keep model (+ streaming) in the URL
                        // (request_target); every other family carries the
                        // model in the body — content AND non-content
                        // (count/embeddings) alike.
                        let body_carries_model = match target.kind {
                            OperationKind::ContentGeneration(
                                ContentGenerationKind::GeminiGenerateContent,
                            ) => false,
                            OperationKind::ContentGeneration(_) => true,
                            OperationKind::Provider(Provider::Gemini) => false,
                            OperationKind::Provider(_) => true,
                        };
                        if body_carries_model {
                            let model = (!cand.upstream_model_id.is_empty())
                                .then_some(cand.upstream_model_id.as_str());
                            // `stream` is a content-generation concept only
                            let stream = ctx.stream && target.operation.is_content_generation();
                            // §17: openai-chat targets need the usage chunk
                            let include_usage = ctx.stream && is_openai_chat(target.kind);
                            if model.is_some() || stream || include_usage {
                                converted = patch_body(&converted, model, stream, include_usage)?;
                            }
                        }
                        memo.bodies.insert(key, converted.clone());
                        converted
                    }
                };
                let t = protocol::request_target(*target, &cand.upstream_model_id, ctx.stream);
                (
                    RequestParts {
                        method: t.method.into(),
                        path: t.path,
                        query: t.query,
                        body,
                        headers: None,
                    },
                    *target,
                )
            }
        }
        TransformPlan::AggregateStream {
            request_pair,
            source,
            target,
            ..
        } => {
            // Force a streaming upstream regardless of `ctx.stream`; the streamed
            // response is collapsed in `materialize`. Cross-kind first converts
            // the body to the target wire; same-kind passes it through.
            let base = match request_pair {
                Some(rp) => {
                    let fwd = TransformContext::new(*source, *target);
                    Bytes::from(
                        dispatch::request_bytes(*rp, &fwd, &ctx.body)
                            .map_err(PipelineError::TransformRequest)?,
                    )
                }
                None => ctx.body.clone(),
            };
            // Gemini carries model (+ stream) in the URL; other families in body.
            let body_carries_model = !matches!(
                target.kind,
                OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
                    | OperationKind::Provider(Provider::Gemini)
            );
            let body = if body_carries_model {
                let model =
                    (!cand.upstream_model_id.is_empty()).then_some(cand.upstream_model_id.as_str());
                let include_usage = is_openai_chat(target.kind);
                patch_body(&base, model, true, include_usage)?
            } else {
                base
            };
            let t = protocol::request_target(*target, &cand.upstream_model_id, true);
            (
                RequestParts {
                    method: t.method.into(),
                    path: t.path,
                    query: t.query,
                    body,
                    headers: None,
                },
                *target,
            )
        }
    };

    // process rules act on the provider-native request
    if let Some(rules) = rules.filter(|r| !r.is_empty()) {
        let kind = match target_key.kind {
            OperationKind::ContentGeneration(k) => Some(k),
            OperationKind::Provider(_) => None,
        };
        // §8-B: rule model filters match the PRE-variant-strip INBOUND name
        // (body model, else path-embedded model — e.g. `*-thinking` patterns
        // keyed on the requested variant), falling back to the member model.
        let filter_model = memo
            .inbound_model(&ctx.body)
            .or_else(|| crate::pipeline::classify::path_model_id(&ctx.path))
            .unwrap_or_else(|| cand.upstream_model_id.clone());
        let mut headers = ctx.headers.clone();
        parts.body = process::apply(
            rules,
            target_key,
            kind,
            &filter_model,
            &mut headers,
            parts.body,
        );
        parts.headers = Some(headers);
    }
    Ok(parts)
}

/// Convert a buffered success response back to the inbound protocol.
pub fn response_body(plan: &TransformPlan, body: Bytes) -> Result<Bytes, PipelineError> {
    match plan {
        // AggregateStream responses go through `aggregate_response_body` instead
        // (after the stream→object collapse), so identity here.
        TransformPlan::Passthrough
        | TransformPlan::Local
        | TransformPlan::AggregateStream { .. } => Ok(body),
        TransformPlan::Transform {
            response_pair,
            source,
            target,
            ..
        } => {
            let rev = TransformContext::new(*target, *source);
            dispatch::response_bytes(*response_pair, &rev, &body)
                .map(Bytes::from)
                .map_err(PipelineError::TransformResponse)
        }
    }
}

/// Convert a collapsed-stream object (already the TARGET wire kind) back to the
/// inbound wire. Identity when source and target kinds match (`response_pair`
/// is `None`). Only meaningful for [`TransformPlan::AggregateStream`].
pub fn aggregate_response_body(plan: &TransformPlan, body: Bytes) -> Result<Bytes, PipelineError> {
    match plan {
        TransformPlan::AggregateStream {
            response_pair: Some(rp),
            source,
            target,
            ..
        } => {
            let rev = TransformContext::new(*target, *source);
            dispatch::response_bytes(*rp, &rev, &body)
                .map(Bytes::from)
                .map_err(PipelineError::TransformResponse)
        }
        _ => Ok(body),
    }
}

/// Build the streaming adapter for a Transform plan (None for passthrough).
pub fn stream_transformer(plan: &TransformPlan) -> Option<SseTransformer> {
    match plan {
        TransformPlan::Passthrough | TransformPlan::Local => None,
        // Same-kind aggregate streams relay the target SSE verbatim (None);
        // cross-kind convert the target SSE to the inbound wire.
        TransformPlan::AggregateStream {
            response_pair,
            source,
            target,
            ..
        } => {
            let pair = (*response_pair)?;
            let OperationKind::ContentGeneration(inbound) = source.kind else {
                return None;
            };
            Some(SseTransformer::new(
                pair,
                TransformContext::new(*target, *source),
                inbound,
            ))
        }
        TransformPlan::Transform {
            response_pair,
            source,
            target,
            ..
        } => {
            let OperationKind::ContentGeneration(inbound) = source.kind else {
                return None;
            };
            Some(SseTransformer::new(
                *response_pair,
                TransformContext::new(*target, *source),
                inbound,
            ))
        }
    }
}

/// Patch the transformed provider-native body in one parse: set the member
/// model (body-model kinds), the `stream` flag when the inbound request
/// streams but the converted body would otherwise silently request a
/// non-streaming upstream response (gemini sources carry streaming in the
/// URL), and — §17 — `stream_options.include_usage` for openai-chat-bound
/// streams (merged; other `stream_options` keys are preserved).
fn patch_body(
    body: &Bytes,
    model: Option<&str>,
    stream: bool,
    include_usage: bool,
) -> Result<Bytes, PipelineError> {
    let mut v: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        PipelineError::TransformRequest(TransformError::InvalidInput {
            reason: format!("body patch: body is not JSON: {e}"),
        })
    })?;
    if let Some(obj) = v.as_object_mut() {
        if let Some(model) = model {
            obj.insert(
                "model".to_owned(),
                serde_json::Value::String(model.to_owned()),
            );
        }
        if stream {
            obj.insert("stream".to_owned(), serde_json::Value::Bool(true));
        }
        if include_usage {
            let opts = obj
                .entry("stream_options")
                .or_insert_with(|| serde_json::json!({}));
            match opts.as_object_mut() {
                Some(map) => {
                    map.insert("include_usage".to_owned(), serde_json::Value::Bool(true));
                }
                None => *opts = serde_json::json!({ "include_usage": true }),
            }
        }
    }
    serde_json::to_vec(&v).map(Bytes::from).map_err(|e| {
        PipelineError::TransformRequest(TransformError::Serialization {
            reason: e.to_string(),
        })
    })
}

/// §17: does this key target the openai chat-completions wire format?
fn is_openai_chat(kind: OperationKind) -> bool {
    matches!(
        kind,
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChatCompletions)
    )
}

fn merge_query(existing: Option<&str>, extra: &str) -> String {
    match existing {
        Some(q) if q.split('&').any(|p| p == extra) => q.to_owned(),
        Some(q) if !q.is_empty() => format!("{q}&{extra}"),
        _ => extra.to_owned(),
    }
}
