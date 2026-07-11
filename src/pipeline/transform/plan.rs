//! Per-candidate transform planning: passthrough vs transform vs
//! force-stream-aggregate vs local, resolved from the provider's routing rules.

use crate::app::snapshot::ControlPlaneSnapshot;
use crate::pipeline::context::RequestCtx;
use crate::pipeline::error::PipelineError;
use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind};
use crate::transform::routing::RoutingDecision;
use crate::transform::{self, TransformError, TransformPair, dispatch, routing};

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
    /// Force a NON-STREAMING upstream for a streaming client. The buffered
    /// response is converted back to the inbound wire and synthesized into the
    /// client's streaming transport by the outer pipeline.
    SynthesizeStream {
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
        matches!(self, Self::Transform { .. } | Self::SynthesizeStream { .. })
    }

    /// `AggregateStream` forces a streaming upstream + collapse on a non-stream
    /// client.
    pub fn is_aggregate_stream(&self) -> bool {
        matches!(self, Self::AggregateStream { .. })
    }

    pub fn is_synthesize_stream(&self) -> bool {
        matches!(self, Self::SynthesizeStream { .. })
    }

    /// Whether the upstream HTTP response must be opened as a stream.
    pub fn upstream_stream(&self, ctx: &RequestCtx) -> bool {
        match self {
            Self::SynthesizeStream { .. } => false,
            _ => ctx.stream,
        }
    }

    /// Whether settlement must interpret the provider response as SSE. An
    /// aggregate plan may buffer the HTTP body, but its bytes are still an
    /// event stream.
    pub fn settle_stream(&self, ctx: &RequestCtx) -> bool {
        match self {
            Self::AggregateStream { .. } => true,
            Self::SynthesizeStream { .. } => false,
            _ => ctx.stream,
        }
    }

    /// The op to surface in [`ShapeCtx`](crate::channel::ShapeCtx): the routed
    /// target when one exists, else the inbound op.
    pub fn shape_op(&self, ctx: &RequestCtx) -> OperationKey {
        match self {
            Self::Transform { target, .. }
            | Self::AggregateStream { target, .. }
            | Self::SynthesizeStream { target, .. } => *target,
            _ => ctx.op.expect("classified before failover"),
        }
    }

    /// Target wire kind for the stream→object collapse (content-gen only).
    pub fn target_kind(&self) -> Option<ContentGenerationKind> {
        match self {
            Self::AggregateStream { target, .. }
            | Self::Transform { target, .. }
            | Self::SynthesizeStream { target, .. } => match target.kind {
                OperationKind::ContentGeneration(k) => Some(k),
                OperationKind::Provider(_) => None,
            },
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
        // Fake streaming: the client asked for a streaming operation but this
        // route deliberately targets the non-stream operation. Fetch one full
        // object, then synthesize the inbound protocol's stream events.
        RoutingDecision::TransformTo(target)
            if source.operation == Operation::StreamGenerateContent
                && target.operation == Operation::GenerateContent =>
        {
            let (request_pair, response_pair) = if source.kind == target.kind {
                (None, None)
            } else {
                let src = OperationKey {
                    operation: Operation::GenerateContent,
                    kind: source.kind,
                };
                let rp =
                    transform::resolve(src, target).map_err(PipelineError::TransformRequest)?;
                let sp =
                    transform::resolve(target, src).map_err(PipelineError::TransformRequest)?;
                if !dispatch::is_wired(rp) || !dispatch::is_wired(sp) {
                    return Err(PipelineError::TransformRequest(
                        TransformError::InvalidInput {
                            reason: "synthesize-stream pair not wired for bytes dispatch"
                                .to_owned(),
                        },
                    ));
                }
                (Some(rp), Some(sp))
            };
            Ok(TransformPlan::SynthesizeStream {
                request_pair,
                response_pair,
                source,
                target,
            })
        }
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
        // Force-stream image generation (codex): inbound `CreateImage`/`EditImage`
        // routed to a streaming Responses target. Codex generates images via the
        // Responses `image_generation` tool and only speaks event-streams, so —
        // like the content force-stream above — stream the upstream and collapse
        // the `image_generation_call` output item back into an images response for
        // the non-stream client. Unlike content, the image pair resolves directly
        // (no `GenerateContent` normalization): `resolve` accepts the streaming
        // target because `StreamGenerateContent` is a content-generation op, and
        // the same symmetric pair serves both directions.
        RoutingDecision::TransformTo(target)
            if matches!(
                source.operation,
                Operation::CreateImage | Operation::EditImage
            ) && target.operation == Operation::StreamGenerateContent =>
        {
            let request_pair =
                transform::resolve(source, target).map_err(PipelineError::TransformRequest)?;
            let response_pair =
                transform::resolve(target, source).map_err(PipelineError::TransformRequest)?;
            if !dispatch::is_wired(request_pair) || !dispatch::is_wired(response_pair) {
                return Err(PipelineError::TransformRequest(
                    TransformError::InvalidInput {
                        reason: "aggregate-stream image pair not wired for bytes dispatch"
                            .to_owned(),
                    },
                ));
            }
            Ok(TransformPlan::AggregateStream {
                request_pair: Some(request_pair),
                response_pair: Some(response_pair),
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::protocol::ContentGenerationKind as Kind;
    use crate::transform::routing::{CompiledRoutingRule, RuleImpl};

    fn plan(source_kind: Kind, target_kind: Kind) -> TransformPlan {
        let mut cp = ControlPlaneSnapshot::empty(1);
        cp.routing_rules_by_provider.insert(
            7,
            Arc::new(vec![CompiledRoutingRule {
                operation: Operation::StreamGenerateContent,
                kind: OperationKind::ContentGeneration(source_kind),
                implementation: RuleImpl::TransformTo,
                dest_operation: Some(Operation::GenerateContent),
                dest_kind: Some(OperationKind::ContentGeneration(target_kind)),
            }]),
        );
        plan_for(
            &cp,
            7,
            OperationKey::content_generation(Operation::StreamGenerateContent, source_kind),
        )
        .unwrap()
    }

    #[test]
    fn stream_to_non_stream_same_kind_needs_no_wire_pair() {
        assert!(matches!(
            plan(Kind::OpenAiChatCompletions, Kind::OpenAiChatCompletions),
            TransformPlan::SynthesizeStream {
                request_pair: None,
                response_pair: None,
                ..
            }
        ));
    }

    #[test]
    fn stream_to_non_stream_cross_kind_resolves_wire_pairs() {
        assert!(matches!(
            plan(Kind::OpenAiChatCompletions, Kind::ClaudeMessages),
            TransformPlan::SynthesizeStream {
                request_pair: Some(_),
                response_pair: Some(_),
                ..
            }
        ));
    }
}
