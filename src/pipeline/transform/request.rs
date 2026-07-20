//! Per-candidate request construction and provider-native process rules.

use std::collections::HashMap;

use bytes::Bytes;
use http::HeaderMap;

use super::TransformPlan;
use crate::pipeline::classify::peek_model;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::error::PipelineError;
use crate::process;
use crate::protocol::{self, ContentGenerationKind, OperationKey, OperationKind, Provider};
use crate::transform::{self, TransformContext, TransformError, dispatch};

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

/// Build the effective request for one candidate: model rewrite (BEFORE the
/// transform, so model-conditional conversion sees the real upstream model),
/// transform (memoized per (target key, model)), endpoint synthesis, then
/// process rules on the provider-native result.
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
                if body_carries_model(op.kind) {
                    // passthrough bodies already carry the correct stream flag;
                    // never inject it here (`include_usage` is false for the
                    // non-openai-chat provider ops, so this is a pure rewrite)
                    body = patch_body(&body, Some(&cand.upstream_model_id), None, include_usage)?;
                } else {
                    let t = protocol::request_target(op, &cand.upstream_model_id, ctx.stream);
                    path = t.path;
                    if let Some(extra) = t.query {
                        query = Some(merge_query(query.as_deref(), &extra));
                    }
                }
            } else if include_usage {
                body = patch_body(&body, None, None, true)?;
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
            // endpoint synthesis plus the list-models QUERY conversion.
            if !target.operation.has_request_body() {
                let t = protocol::request_target(*target, &cand.upstream_model_id, ctx.stream);
                let fwd = TransformContext::new(*source, *target)
                    .with_request(&ctx.path, ctx.query.as_deref());
                let query = match (
                    t.query,
                    transform::models::list::query::request_query(*request_pair, &fwd),
                ) {
                    (Some(base), Some(extra)) => Some(merge_query(Some(&base), &extra)),
                    (q, converted) => converted.or(q),
                };
                (
                    RequestParts {
                        method: t.method.into(),
                        path: t.path,
                        query,
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
                        // Member model rewrite BEFORE the transform: the
                        // transform must see the real upstream model — e.g. the
                        // →claude mid-conversation system gate. Gemini WIRE
                        // bodies have no model field, but the protocol struct
                        // accepts one, so injection is safe whenever the
                        // CONVERTED body is a different wire (the transform
                        // consumes it; a gemini upstream never sees it raw).
                        let source_in_body = body_carries_model(source.kind);
                        let inbound = if (source_in_body || body_carries_model(target.kind))
                            && !cand.upstream_model_id.is_empty()
                            && memo.inbound_model(&ctx.body).as_deref()
                                != Some(cand.upstream_model_id.as_str())
                        {
                            patch_body(&ctx.body, Some(&cand.upstream_model_id), None, false)?
                        } else {
                            ctx.body.clone()
                        };
                        let fwd = TransformContext::new(*source, *target)
                            .with_request(&ctx.path, ctx.query.as_deref());
                        let converted = dispatch::request_bytes(*request_pair, &fwd, &inbound)
                            .map_err(PipelineError::TransformRequest)?;
                        let mut converted = Bytes::from(converted);
                        if body_carries_model(target.kind) {
                            let model = (!source_in_body && !cand.upstream_model_id.is_empty())
                                .then_some(cand.upstream_model_id.as_str());
                            // `stream` is a content-generation concept only
                            let stream = ctx.stream && target.operation.is_content_generation();
                            // §17: openai-chat targets need the usage chunk
                            let include_usage = ctx.stream && is_openai_chat(target.kind);
                            if model.is_some() || stream || include_usage {
                                converted = patch_body(
                                    &converted,
                                    model,
                                    stream.then_some(true),
                                    include_usage,
                                )?;
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
        TransformPlan::SynthesizeStream {
            request_pair,
            source,
            target,
            ..
        } => {
            let source_in_body = body_carries_model(source.kind);
            let inbound = if (source_in_body || body_carries_model(target.kind))
                && !cand.upstream_model_id.is_empty()
                && memo.inbound_model(&ctx.body).as_deref() != Some(cand.upstream_model_id.as_str())
            {
                patch_body(&ctx.body, Some(&cand.upstream_model_id), None, false)?
            } else {
                ctx.body.clone()
            };
            let base = match request_pair {
                Some(rp) => {
                    let normalized_source = OperationKey {
                        operation: crate::protocol::Operation::GenerateContent,
                        kind: source.kind,
                    };
                    let fwd = TransformContext::new(normalized_source, *target)
                        .with_request(&ctx.path, ctx.query.as_deref());
                    Bytes::from(
                        dispatch::request_bytes(*rp, &fwd, &inbound)
                            .map_err(PipelineError::TransformRequest)?,
                    )
                }
                None => inbound,
            };
            let body = if body_carries_model(target.kind) {
                let model = (!source_in_body && !cand.upstream_model_id.is_empty())
                    .then_some(cand.upstream_model_id.as_str());
                patch_body(&base, model, Some(false), false)?
            } else {
                base
            };
            let t = protocol::request_target(*target, &cand.upstream_model_id, false);
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
        TransformPlan::AggregateStream {
            request_pair,
            source,
            target,
            ..
        } => {
            // Force a streaming upstream regardless of `ctx.stream`; the streamed
            // response is collapsed in `materialize`. Model rewrite BEFORE the
            // conversion, as in the Transform branch (incl. the gemini-source
            // injection rationale).
            let source_in_body = body_carries_model(source.kind);
            let inbound = if (source_in_body || body_carries_model(target.kind))
                && !cand.upstream_model_id.is_empty()
                && memo.inbound_model(&ctx.body).as_deref() != Some(cand.upstream_model_id.as_str())
            {
                patch_body(&ctx.body, Some(&cand.upstream_model_id), None, false)?
            } else {
                ctx.body.clone()
            };
            // Cross-kind first converts the body to the target wire; same-kind
            // passes it through.
            let base = match request_pair {
                Some(rp) => {
                    let fwd = TransformContext::new(*source, *target)
                        .with_request(&ctx.path, ctx.query.as_deref());
                    Bytes::from(
                        dispatch::request_bytes(*rp, &fwd, &inbound)
                            .map_err(PipelineError::TransformRequest)?,
                    )
                }
                None => inbound,
            };
            // Gemini carries model (+ stream) in the URL; other families in body.
            let body = if body_carries_model(target.kind) {
                let model = (!source_in_body && !cand.upstream_model_id.is_empty())
                    .then_some(cand.upstream_model_id.as_str());
                let include_usage = is_openai_chat(target.kind);
                patch_body(&base, model, Some(true), include_usage)?
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

/// Whether this wire kind carries the model in the request BODY. Gemini keeps
/// the model (+ stream flag) in the PATH; every other family carries it in the
/// body — content AND non-content (embeddings, count_tokens) alike.
fn body_carries_model(kind: OperationKind) -> bool {
    !matches!(
        kind,
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
            | OperationKind::Provider(Provider::Gemini)
    )
}

/// Patch the provider-native body in one parse: set the member model
/// (body-model kinds), the `stream` flag when the inbound request streams but
/// the converted body would otherwise silently request a non-streaming
/// upstream response (gemini sources carry streaming in the URL), and — §17 —
/// `stream_options.include_usage` for openai-chat-bound streams (merged; other
/// `stream_options` keys are preserved).
fn patch_body(
    body: &Bytes,
    model: Option<&str>,
    stream: Option<bool>,
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
        if let Some(stream) = stream {
            obj.insert("stream".to_owned(), serde_json::Value::Bool(stream));
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
