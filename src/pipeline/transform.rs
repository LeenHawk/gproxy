//! M2 transform-dispatch step: per-candidate plan (passthrough vs transform),
//! effective upstream request parts (path/query/body/headers incl. process
//! rules + model rewrite), and response-direction conversion. Planning lives
//! in [`plan`]; this module builds the effective request/response bytes.

use bytes::Bytes;

use crate::pipeline::error::PipelineError;
use crate::protocol::{OperationKey, OperationKind};
use crate::transform::stream_adapter::SseTransformer;
use crate::transform::{TransformContext, dispatch};

mod plan;
mod request;

pub use plan::{TransformPlan, plan_for};
pub use request::{AttemptMemo, RequestParts, request_parts};

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
        TransformPlan::SynthesizeStream {
            response_pair: Some(response_pair),
            source,
            target,
            ..
        } => {
            let normalized_source = OperationKey {
                operation: crate::protocol::Operation::GenerateContent,
                kind: source.kind,
            };
            let rev = TransformContext::new(*target, normalized_source);
            dispatch::response_bytes(*response_pair, &rev, &body)
                .map(Bytes::from)
                .map_err(PipelineError::TransformResponse)
        }
        TransformPlan::SynthesizeStream {
            response_pair: None,
            ..
        } => Ok(body),
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
        // The upstream response is a full JSON object, not SSE. It is encoded
        // by the response-to-stream synthesizer after materialization.
        TransformPlan::SynthesizeStream { .. } => None,
    }
}
