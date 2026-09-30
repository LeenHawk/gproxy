use super::*;
use crate::{
    HttpBody, WireRequest, WireResponse,
    capability::{CapabilityLimits, Upstream, UpstreamConnection},
    codec,
    wire::openai::responses::websocket::ClientEvent,
};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ResponsesWsConnect {
    Connected {
        handshake: WireResponse<()>,
        session: ResponsesWsSession,
    },
    Rejected(WireResponse<HttpBody>),
}

/// Failed preflight/transport, or contradictory connected-handshake metadata.
/// A normal HTTP rejection is the untouched `ResponsesWsConnect::Rejected`.
#[derive(Debug)]
pub struct ResponsesWsConnectError {
    pub error: TransformError,
    pub handshake: Option<Box<WireResponse<()>>>,
}

impl ResponsesWsConnectError {
    pub fn kind(&self) -> TransformErrorKind {
        self.error.kind()
    }
}

impl From<TransformError> for ResponsesWsConnectError {
    fn from(error: TransformError) -> Self {
        Self {
            error,
            handshake: None,
        }
    }
}

impl std::fmt::Display for ResponsesWsConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for ResponsesWsConnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// The host supplies the actual upgrade, including authentication and deadlines.
/// HTTP/1.1 101 and a successful HTTP/2 CONNECT status remain unmodified.
pub async fn connect<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    limits: ResponsesWsLimits,
) -> Result<ResponsesWsConnect, ResponsesWsConnectError> {
    let bounds = bound(limits, upstream.limits())?;
    // Even an empty typed response.create must fit before opening a connection.
    codec::encode_json(
        &ClientEvent::ResponseCreate(GenerateContentRequestBody::builder().build()),
        codec_limits(bounds.send_event.min(bounds.send_bytes)),
    )
    .map_err(|e| codec_error(e, true))?;
    match upstream
        .connect(target, request)
        .await
        .map_err(TransformError::from)?
    {
        UpstreamConnection::Rejected(response) => Ok(ResponsesWsConnect::Rejected(response)),
        UpstreamConnection::Connected { handshake, socket } => {
            if handshake.status != http::StatusCode::SWITCHING_PROTOCOLS
                && !handshake.status.is_success()
            {
                drop(socket);
                return Err(ResponsesWsConnectError {
                    error: invalid(
                        "host returned a connected socket with unsuccessful handshake status",
                    ),
                    handshake: Some(Box::new(handshake)),
                });
            }
            Ok(ResponsesWsConnect::Connected {
                handshake,
                session: ResponsesWsSession::connected(socket, bounds),
            })
        }
    }
}

fn host_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn bound(limits: ResponsesWsLimits, host: CapabilityLimits) -> Result<Bounds, TransformError> {
    let send_frame = limits
        .max_frame_bytes
        .min(host_usize(host.ws_frame_bytes))
        .min(host_usize(host.write_bytes));
    let receive_frame = limits
        .max_frame_bytes
        .min(host_usize(host.ws_frame_bytes))
        .min(host_usize(host.read_bytes));
    let send_event = limits.max_event_bytes.min(send_frame);
    let receive_event = limits.max_event_bytes.min(receive_frame);
    let send_bytes = limits.max_send_bytes.min(host_usize(host.write_bytes));
    let receive_bytes = limits.max_receive_bytes.min(host_usize(host.read_bytes));
    let mut collector = limits.collector;

    collector.max_bytes = collector.max_bytes.min(receive_bytes);
    collector.max_text_bytes = collector.max_text_bytes.min(receive_bytes);
    collector.max_json_bytes = collector.max_json_bytes.min(receive_bytes);
    if [
        send_frame,
        receive_frame,
        send_event,
        receive_event,
        send_bytes,
        receive_bytes,
        collector.max_bytes,
    ]
    .contains(&0)
    {
        return Err(limit(
            "responses.websocket.limits",
            "positive usable byte limits required",
        ));
    }
    Ok(Bounds {
        send_frame,
        receive_frame,
        send_event,
        receive_event,
        send_bytes,
        receive_bytes,

        allow_binary: limits.allow_binary,
        collector,
    })
}
