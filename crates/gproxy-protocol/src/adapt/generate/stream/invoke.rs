use super::super::{Endpoint, GenerationIdentity, GenerationProgress, GenerationStateAccess};
use super::{
    binding::StateBinding,
    bridge::StreamBridge,
    event::{Collected, EventLimits, NativeEvent},
    output,
    reader::{NativeReader, SourceFraming},
};
use crate::{
    HttpBody, WireRequest, WireResponse,
    capability::{StateStore, Upstream},
    codec::{self, CodecLimits},
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::DeclaredFields,
};
use bytes::Bytes;
use std::collections::VecDeque;

pub(super) type NativeFull<B> = <<B as StreamBridge>::NativeEvent as NativeEvent>::Full;

pub(super) type ClientFull<B> = <<B as StreamBridge>::ClientEvent as NativeEvent>::Full;

/// Selected operation and per-invocation identity namespaces, supplied by the host.
pub struct StreamTarget {
    pub endpoint: Endpoint,
    pub identities: GenerationIdentity,
}

#[derive(Debug, Clone, Copy)]
pub struct StreamSettings {
    pub codec: CodecLimits,
    pub events: EventLimits,
    pub source_framing: SourceFraming,
    pub client_framing: SourceFraming,
}

impl StreamSettings {}

#[derive(Debug)]
pub enum StreamStart {
    Streaming(WireResponse<()>),
    Rejected(WireResponse<Bytes>),
}

/// A client event and its already bounded native transport encoding. The final
/// framing chunk (`[DONE]` or `]`) has `event = None` and `finished = true`.
#[derive(Debug)]
pub struct StreamChunk<E> {
    pub event: Option<E>,
    pub bytes: Bytes,
    pub finished: bool,
}

pub(super) struct ReadyChunk<E> {
    pub chunk: StreamChunk<E>,
}

/// Caller-owned invocation and progress. Cancellation retains pending state
/// writes/events; a started upstream send is never implicitly repeated.
pub struct StreamInvocation<B: StreamBridge> {
    pub(super) original: B::ClientRequest,
    pub(super) history: Option<super::history::History>,
    pub(super) pending_native: Option<B::NativeEvent>,
    pub(super) pending_converted: Option<Vec<B::ClientEvent>>,
    pub(super) finishing_source: bool,
    pub(super) image_resources_required: bool,
    pub(super) resource_revision: u64,
    pub(super) target: B::NativeRequest,
    pub(super) selected: StreamTarget,
    pub(super) settings: StreamSettings,
    /// The state this invocation was prepared against; every step must use it.
    pub(super) binding: StateBinding,
    /// The output lane a WebSocket start chose, so an HTTP start is refused
    /// afterwards and a retried WebSocket start cannot switch lanes.
    pub(super) websocket_lane: Option<Option<String>>,
    pub(super) bridge: Option<B>,
    pub(super) flow: IdentityFlow,
    pub(super) source: Option<<B::NativeEvent as NativeEvent>::Collector>,
    pub(super) client: Option<<B::ClientEvent as NativeEvent>::Collector>,
    pub(super) reader: Option<NativeReader>,
    pub(super) encoder: output::Encoder,
    pub(super) queued: VecDeque<(B::ClientEvent, u64)>,
    pub(super) held: Vec<(B::ClientEvent, u64)>,
    pub(super) holding: bool,
    pub(super) queued_bytes: u64,
    pub(super) ready: Option<ReadyChunk<B::ClientEvent>>,
    pub(super) last_native_event: Option<B::NativeEvent>,
    pub(super) native_final: Option<Collected<NativeFull<B>>>,
    pub(super) client_final: Option<ClientFull<B>>,
    pub(super) final_progress: GenerationProgress<Collected<NativeFull<B>>>,
    pub(super) final_saved: bool,
    pub(super) signed: crate::transform::generate::gemini_responses::stream::SignedToolBindings,
    pub(super) report: Report,
    pub(super) emit_usage: bool,
    pub(super) metadata: Option<WireResponse<()>>,
    pub(super) rejected: Option<WireResponse<Bytes>>,
    pub(super) sent: bool,
    pub(super) eof: bool,
    pub(super) websocket_terminal: bool,
    pub(super) finished: bool,
    pub(super) failed: bool,
}

impl<B: StreamBridge> StreamInvocation<B> {
    pub(super) async fn new<S: StateStore>(
        original: B::ClientRequest,
        target: B::NativeRequest,
        selected: StreamTarget,
        settings: StreamSettings,
        bridge: B,
        report: Report,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let original = original.into_declared();
        let target = target.into_declared();
        let flow = bridge.identities().clone();
        if flow.namespace() != selected.identities.response.namespace() {
            return Err(super::invalid("bridge response namespace mismatch"));
        }
        let source = B::NativeEvent::collector(
            IdentityFlow::new(flow.namespace()),
            TargetIdPolicy::new(B::NativeEvent::DIALECT),
            settings.events,
        );
        let client = B::ClientEvent::collector(
            IdentityFlow::new(flow.namespace()),
            selected.identities.response_policy.clone(),
            settings.events,
        );
        Ok(Self {
            history: None,
            pending_native: None,
            pending_converted: None,
            finishing_source: false,
            image_resources_required: false,
            resource_revision: 0,
            original,
            target,
            selected,
            settings,
            binding: StateBinding::new(state),
            websocket_lane: None,
            bridge: Some(bridge),
            flow,
            source: Some(source),
            client: Some(client),
            reader: None,
            encoder: output::Encoder::new(settings.client_framing, settings.codec),
            queued: VecDeque::new(),
            held: Vec::new(),
            holding: false,
            queued_bytes: 0,
            ready: None,
            last_native_event: None,
            native_final: None,
            client_final: None,
            final_progress: Default::default(),
            final_saved: false,
            signed: Default::default(),
            report,
            emit_usage: true,
            metadata: None,
            rejected: None,
            sent: false,
            eof: false,
            websocket_terminal: false,
            finished: false,
            failed: false,
        })
    }
    pub fn original_request(&self) -> &B::ClientRequest {
        &self.original
    }
    pub fn target_request(&self) -> &B::NativeRequest {
        &self.target
    }
    pub fn send_started(&self) -> bool {
        self.sent
    }
    pub fn report(&self) -> &Report {
        &self.report
    }
    pub fn native_metadata(&self) -> Option<&WireResponse<()>> {
        self.metadata.as_ref()
    }
    pub fn last_native_event(&self) -> Option<&B::NativeEvent> {
        self.last_native_event.as_ref()
    }
    pub fn native_result(&self) -> Option<&NativeFull<B>> {
        self.native_final.as_ref().map(|v| &v.value)
    }
    pub fn client_result(&self) -> Option<&ClientFull<B>> {
        if self.final_saved {
            self.client_final.as_ref()
        } else {
            None
        }
    }
    pub fn rejected_response(&self) -> Option<&WireResponse<Bytes>> {
        self.rejected.as_ref()
    }
    pub async fn start<U: Upstream, S: StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamStart, TransformError> {
        self.binding.check(state)?;
        if self.sent || self.failed || self.websocket_lane.is_some() {
            return Err(super::conflict(
                "stream send already started or failed; cannot replay POST",
            ));
        }
        let mut request_limits = self.settings.codec;
        request_limits.max_body_bytes = request_limits
            .max_body_bytes
            .min(upstream.limits().write_bytes);
        let body = codec::encode_json(&self.target, request_limits).map_err(super::codec_error)?;
        let mut headers = self.selected.endpoint.headers.clone();
        for name in [
            http::header::CONTENT_LENGTH,
            http::header::CONTENT_ENCODING,
            http::header::TRANSFER_ENCODING,
        ] {
            headers.remove(name);
        }
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        self.sent = true;
        let response = upstream
            .send(
                target,
                WireRequest {
                    method: http::Method::POST,
                    path: self.selected.endpoint.path.clone(),
                    query: self.selected.endpoint.query.clone(),
                    headers,
                    body: HttpBody::Bytes(body),
                },
            )
            .await?;
        let mut read_limits = self.settings.codec;
        read_limits.max_body_bytes = read_limits.max_body_bytes.min(upstream.limits().read_bytes);
        self.metadata = Some(WireResponse {
            status: response.status,
            headers: response.headers.clone(),
            body: (),
        });
        if !response.status.is_success() {
            let raw = WireResponse {
                status: response.status,
                headers: response.headers,
                body: codec::read_http_body(response.body, read_limits)
                    .await
                    .map_err(super::codec_error)?,
            };
            self.rejected = Some(WireResponse {
                status: raw.status,
                headers: raw.headers.clone(),
                body: raw.body.clone(),
            });
            return Ok(StreamStart::Rejected(raw));
        }
        let accepted = WireResponse {
            status: response.status,
            headers: output::headers(response.headers.clone(), self.settings.client_framing),
            body: (),
        };
        self.reader = Some(NativeReader::new(
            response.body,
            self.settings.source_framing,
            read_limits,
            self.settings.events.max_events,
        ));
        if let Some(value) = response.headers.get(http::header::CONTENT_TYPE) {
            let valid = value
                .to_str()
                .ok()
                .and_then(|v| v.split(';').next())
                .is_some_and(|v| match self.settings.source_framing {
                    SourceFraming::Sse => v.trim().eq_ignore_ascii_case("text/event-stream"),
                    SourceFraming::JsonArray => v.trim().eq_ignore_ascii_case("application/json"),
                    SourceFraming::Ndjson => {
                        matches!(v.trim(), "application/x-ndjson" | "application/ndjson")
                    }
                });
            if !valid {
                self.failed = true;
                self.reader = None;
                return Err(super::invalid(
                    "native content type disagrees with selected stream framing",
                ));
            }
        }
        Ok(StreamStart::Streaming(accepted))
    }
}
