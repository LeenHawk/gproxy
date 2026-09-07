use bytes::Bytes;
use gproxy_channel_api::Disposition;
use gproxy_protocol::{Operation, OperationKind};

use super::FunnelCtx;
use crate::boundary::ResponseBody;

pub(super) type Outward = (http::StatusCode, http::HeaderMap, ResponseBody, Disposition);

/// The response a detached synthesized stream opens with, before any upstream
/// attempt has answered: everything that follows, including the failure case,
/// travels as frames of the client's protocol.
pub(crate) fn opened(
    framing: gproxy_protocol::StreamFraming,
    body: crate::boundary::ByteStream,
) -> crate::boundary::ExecOutcome {
    let mut headers = http::HeaderMap::new();
    super::frame_headers(&mut headers, framing);
    crate::boundary::ExecOutcome {
        status: http::StatusCode::OK,
        headers,
        body: ResponseBody::Stream(body),
        disposition: Disposition::Success,
        _settled: super::Settled(()),
    }
}

/// A streaming client routed onto the buffered sibling operation: the upstream
/// answered with one object, already converted to the client's wire, and the
/// client's stream is synthesized from it once settlement has seen the whole body.
pub(super) fn outward_body(
    ctx: &FunnelCtx,
    status: http::StatusCode,
    mut headers: http::HeaderMap,
    body: Bytes,
    disposition: Disposition,
) -> Outward {
    let synthesized = ctx.source_key.zip(ctx.key).filter(|(source, target)| {
        status.is_success()
            && source.operation() == Operation::StreamGenerateContent
            && target.operation() == Operation::GenerateContent
    });
    let Some((OperationKind::ContentGeneration(kind), _)) =
        synthesized.map(|(source, target)| (source.kind(), target))
    else {
        return (status, headers, ResponseBody::Full(body), disposition);
    };
    match gproxy_transform::synthesize_response(kind, body, ctx.source_framing) {
        Ok(frames) => {
            super::frame_headers(&mut headers, ctx.source_framing);
            let stream: crate::boundary::ByteStream =
                Box::pin(futures_util::stream::iter(frames.into_iter().map(Ok)));
            (status, headers, ResponseBody::Stream(stream), disposition)
        }
        Err(error) => {
            let (status, headers, body, disposition) =
                super::transform_error(crate::error::CoreError::Transform(error.to_string()));
            (status, headers, ResponseBody::Full(body), disposition)
        }
    }
}
