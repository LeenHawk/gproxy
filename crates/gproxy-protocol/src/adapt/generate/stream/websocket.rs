//! Concrete Responses WebSocket transport using the same incremental pair states.
//! The host binds authentication to its target and state scope at connection time.

use super::super::GenerationStateAccess;
use super::{StreamChunk, StreamInvocation, bridge::StreamBridge};
use crate::{
    HttpBody, WireRequest, WireResponse,
    adapt::responses_ws as ws,
    capability::{StateStore, Upstream},
    transform::TransformError,
    wire::{
        DeclaredFields,
        openai::responses::{
            self as r,
            stream::StreamEvent,
            websocket::{ClientEvent, RequestMessage},
        },
    },
};

pub struct GenerationWsSession {
    native: ws::ResponsesWsSession,
    /// The state the connection was opened under; each turn must use it.
    binding: super::binding::StateBinding,
}

#[allow(clippy::large_enum_variant)]
pub enum GenerationWsConnect {
    Connected {
        handshake: WireResponse<()>,
        session: GenerationWsSession,
    },
    Rejected(WireResponse<HttpBody>),
}

impl GenerationWsSession {
    pub fn native(&self) -> &ws::ResponsesWsSession {
        &self.native
    }
}

pub async fn connect<U: Upstream, S: StateStore>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    limits: ws::ResponsesWsLimits,
    state: &GenerationStateAccess<'_, S>,
) -> Result<GenerationWsConnect, ws::ResponsesWsConnectError> {
    let binding = super::binding::StateBinding::new(state);
    match ws::connect(upstream, target, request, limits).await? {
        ws::ResponsesWsConnect::Connected { handshake, session } => {
            Ok(GenerationWsConnect::Connected {
                handshake,
                session: GenerationWsSession {
                    native: session,
                    binding,
                },
            })
        }
        ws::ResponsesWsConnect::Rejected(response) => Ok(GenerationWsConnect::Rejected(response)),
    }
}

/// Owns the native turn while borrowing its caller-owned conversion progress.
/// Dropping a nonterminal native turn poisons the connection; it never resends.
pub struct GenerationWsTurn<'a, B: StreamBridge<NativeEvent = StreamEvent>> {
    invocation: &'a mut StreamInvocation<B>,
    turn: Option<ws::ResponsesWsTurn<'a>>,
    failure: Option<StreamEvent>,
    websocket_failure: Option<r::websocket::ErrorMessage>,
}

impl<B: StreamBridge<NativeEvent = StreamEvent>> GenerationWsTurn<'_, B> {
    pub async fn next<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        let Some(turn) = &mut self.turn else {
            return Err(super::invalid("WebSocket generation turn failed"));
        };
        let result = self.invocation.next_inner(state, Some(turn)).await;
        if result.is_err() && self.invocation.failed {
            self.failure = turn.failure_event().cloned();
            self.websocket_failure = turn.websocket_failure().cloned();
            self.turn.take();
        }
        result
    }
    pub async fn collect<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<super::invoke::ClientFull<B>, TransformError> {
        while self.next(state).await?.is_some() {}
        let result = self
            .invocation
            .client_result()
            .ok_or_else(|| super::missing("WebSocket client result is not durably committed"))?
            .clone();
        crate::codec::encode_json(&result, self.invocation.settings.codec)
            .map_err(super::codec_error)?;
        Ok(result)
    }
    pub fn invocation(&self) -> &StreamInvocation<B> {
        self.invocation
    }
    pub fn failure_event(&self) -> Option<&StreamEvent> {
        self.failure
            .as_ref()
            .or_else(|| self.turn.as_ref().and_then(|turn| turn.failure_event()))
    }
    pub fn websocket_failure(&self) -> Option<&r::websocket::ErrorMessage> {
        self.websocket_failure
            .as_ref()
            .or_else(|| self.turn.as_ref().and_then(|turn| turn.websocket_failure()))
    }
}

impl<B: StreamBridge<NativeEvent = StreamEvent, NativeRequest = r::GenerateContentRequestBody>>
    StreamInvocation<B>
{
    /// Sends response.create at most once. The invocation and the connection
    /// must both have been prepared against this state.
    pub async fn start_websocket<'a, S: StateStore>(
        &'a mut self,
        session: &'a mut GenerationWsSession,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<GenerationWsTurn<'a, B>, TransformError> {
        self.start_websocket_on_lane(session, None, state).await
    }
    pub async fn start_websocket_on_lane<'a, S: StateStore>(
        &'a mut self,
        session: &'a mut GenerationWsSession,
        lane: Option<String>,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<GenerationWsTurn<'a, B>, TransformError> {
        self.binding.check(state)?;
        session.binding.check(state)?;
        if self.sent || self.failed {
            return Err(super::conflict("invocation already sent or failed"));
        }
        if self
            .websocket_lane
            .as_ref()
            .is_some_and(|known| known != &lane)
        {
            return Err(super::conflict("WebSocket turn lane changed"));
        }
        let mut request = self.target.clone().into_declared();
        // WS is intrinsically streaming; this is the transport control added by
        // prepare_stream, not a user-requested change of generation semantics.
        request.stream = None;
        self.websocket_lane = Some(lane.clone());
        let turn = session.native.turn_message(RequestMessage {
            stream_id: lane,
            generate: None,
            event: ClientEvent::ResponseCreate(request),
        })?;
        self.sent = true;
        Ok(GenerationWsTurn {
            invocation: self,
            turn: Some(turn),
            failure: None,
            websocket_failure: None,
        })
    }
}

impl<B: StreamBridge<ClientEvent = StreamEvent>> StreamInvocation<B> {
    /// Select bare, individually bounded JSON server messages before HTTP start.
    /// Each returned nonempty chunk is exactly one WebSocket Text payload.
    pub fn responses_websocket_output(&mut self) -> Result<(), TransformError> {
        self.responses_websocket_output_on_lane(None)
    }
    pub fn responses_websocket_output_on_lane(
        &mut self,
        lane: Option<String>,
    ) -> Result<(), TransformError> {
        if self.sent || self.failed {
            return Err(super::conflict("select WebSocket output before send"));
        }
        self.encoder = super::output::Encoder::WebSocket {
            total: 0,
            events: 0,
            limits: self.settings.codec,
            lane,
        };
        Ok(())
    }
}

/// Decode only the declared response.create shape. Unknown extensions are scrubbed.
pub fn decode_request(
    bytes: &[u8],
    limits: crate::codec::CodecLimits,
) -> Result<r::GenerateContentRequestBody, TransformError> {
    let message = decode_message(bytes, limits)?;
    if message.stream_id.is_some() {
        return Err(TransformError::shape(
            "responses.websocket.stream_id",
            "use decode_message and preserve the explicit output lane",
        ));
    }
    let ClientEvent::ResponseCreate(request) = message.event else {
        return Err(TransformError::unsupported(
            "responses.websocket.control",
            "steering and injection have no equivalent cross-protocol generation operation",
        ));
    };
    Ok(request)
}

/// Decode the WS envelope for cross-protocol generation. Warmup has no
/// equivalent operation on Chat, Claude or Gemini and is rejected explicitly.
pub fn decode_message(
    bytes: &[u8],
    limits: crate::codec::CodecLimits,
) -> Result<RequestMessage, TransformError> {
    let message = decode_session_message(bytes, limits)?;
    if message.generate == Some(false) {
        return Err(TransformError::unsupported(
            "responses.websocket.generate",
            "prefill-only warmup requires a Responses session driver",
        ));
    }
    if !matches!(message.event, ClientEvent::ResponseCreate(_)) {
        return Err(TransformError::unsupported(
            "responses.websocket.control",
            "control messages require a Responses session driver",
        ));
    }
    Ok(message)
}

/// Decode controls for an actual duplex session. Single-response adapters
/// keep using `decode_message` and cannot silently accept these controls.
pub fn decode_session_message(
    bytes: &[u8],
    limits: crate::codec::CodecLimits,
) -> Result<RequestMessage, TransformError> {
    let mut message = crate::codec::decode_json::<RequestMessage>(bytes, limits)
        .map_err(|error| {
            let kind = if error.kind() == crate::codec::CodecErrorKind::Limit {
                crate::transform::TransformErrorKind::Limit
            } else {
                crate::transform::TransformErrorKind::InvalidInput
            };
            TransformError::with_source(
                kind,
                "responses.websocket.request",
                error.to_string(),
                error,
            )
        })?
        .into_declared();

    if let Some(lane) = &message.stream_id
        && (lane.is_empty()
            || lane.len() > 256
            || !lane
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)))
    {
        return Err(TransformError::shape(
            "responses.websocket.stream_id",
            "invalid lane",
        ));
    }
    let ClientEvent::ResponseCreate(request) = &mut message.event else {
        return Ok(message);
    };
    if request.background.flatten() == Some(true) {
        return Err(TransformError::unsupported(
            "responses.websocket.background",
            "WebSocket mode has no background generation control",
        ));
    }
    request.stream = None;
    request.background = None;
    Ok(message)
}

mod image_resources;
