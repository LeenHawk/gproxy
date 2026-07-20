//! Gemini CLI Code Assist response unwrapping and normalization.

use bytes::Bytes;

use super::models;
use crate::channel::envelope::{self, CodeAssistStreamDecoder};
use crate::channel::shaping::vertex_normalize;
use crate::channel::{ChannelStreamDecoder, ShapeCtx};
use crate::protocol::Operation;

pub(super) fn shape(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    if ctx.op.operation == Operation::ListModels {
        return models::quota_to_model_list(body);
    }
    let unwrapped = envelope::unwrap_code_assist(body);
    vertex_normalize::normalize_vertex_response(unwrapped)
}

pub(super) fn stream_decoder() -> Box<dyn ChannelStreamDecoder> {
    Box::new(CodeAssistStreamDecoder::new())
}
