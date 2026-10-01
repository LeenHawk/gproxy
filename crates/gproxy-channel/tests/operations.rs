use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt, sink, stream};
use gproxy_channel::channel::{
    CredentialView, OperationContext, OperationFuture, PrepareContext, ProviderView,
};
use gproxy_channel::{BaseChannel, ChannelBinding, ChannelError, OutboundClient};
use gproxy_protocol::capability::{
    CapabilityError, CapabilityErrorKind, CapabilityFuture, UpstreamConnection,
};
use gproxy_protocol::connection::{Bytes, TransportError, WsFrame};
use gproxy_protocol::spec::{OPERATION_SPECS, OperationTransport};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WebSocket, WireRequest, WireResponse,
};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

fn provider(config: &Value) -> ProviderView<'_> {
    ProviderView {
        id: "p",
        channel: "test",
        base_url: Some("https://example.com"),
        config,
    }
}
fn credential(secret: &Value) -> CredentialView<'_> {
    CredentialView {
        id: "c",
        provider_id: "p",
        auth_kind: "api_key",
        secret,
        metadata: &serde_json::Value::Null,
        version: 0,
        expires_at_ms: None,
    }
}
fn request<B>(body: B) -> WireRequest<B> {
    WireRequest {
        method: Method::GET,
        path: "/".into(),
        query: None,
        headers: HeaderMap::new(),
        body,
    }
}
fn response(body: impl Into<Bytes>) -> WireResponse {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(body.into()),
    }
}

#[derive(Default)]
struct Client {
    urls: Mutex<Vec<String>>,
}
impl OutboundClient for Client {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            self.urls.lock().unwrap().push(request.uri().to_string());
            Ok(response(request.uri().path().to_owned()))
        })
    }
}

struct Empty;
impl BaseChannel for Empty {
    fn id(&self) -> &'static str {
        "test"
    }
}
#[tokio::test]
async fn all_declared_surfaces_default_to_unsupported_without_io() {
    let config = json!({});
    let secret = json!({});
    let client = Arc::new(Client::default());
    {
        let channel: &dyn BaseChannel = &Empty;
        let binding = ChannelBinding::new(
            channel,
            provider(&config),
            credential(&secret),
            client.clone(),
        );
        for spec in OPERATION_SPECS {
            let error = match spec.transport {
                OperationTransport::Http { .. } => binding
                    .send(spec.key, request(HttpBody::Bytes(Bytes::new())))
                    .await
                    .unwrap_err(),
                OperationTransport::WebSocket => {
                    binding.connect(spec.key, request(())).await.unwrap_err()
                }
            };
            assert!(
                matches!(error, ChannelError::UnsupportedOperation(key) if key == spec.key),
                "{error:?}"
            );
        }
    }
    assert!(client.urls.lock().unwrap().is_empty());
}

// Each override returns its own method's operation, independently of dispatch.
// Running the real protocol surface table catches wrong or missing dispatch arms.
macro_rules! http_overrides {
    ($($method:ident => $operation:ident),* $(,)?) => { $(
        fn $method<'a>(&'a self, _: OperationContext<'a>) -> OperationFuture<'a, WireResponse> {
            Box::pin(async { Ok(response(Operation::$operation.id())) })
        }
    )* };
}
macro_rules! ws_overrides {
    ($($method:ident => $operation:ident),* $(,)?) => { $(
        fn $method<'a>(&'a self, _: OperationContext<'a, ()>) -> OperationFuture<'a, UpstreamConnection> {
            Box::pin(async { Ok(UpstreamConnection::Rejected(response(Operation::$operation.id()))) })
        }
    )* };
}
struct Overrides;
impl BaseChannel for Overrides {
    fn id(&self) -> &'static str {
        "test"
    }
    http_overrides! {
        list_models => ListModels, get_model => GetModel, count_tokens => CountTokens,
        generate_content => GenerateContent, stream_generate_content => StreamGenerateContent,
        guardian_review => GuardianReview, guardian_classify => GuardianClassify,
        create_moderation => CreateModeration,
        compact_content => CompactContent, summarize_memory => SummarizeMemory,
        create_conversation => CreateConversation, create_embedding => CreateEmbedding,
        batch_create_embedding => BatchCreateEmbedding, rerank => Rerank, web_search => WebSearch,
        create_image => CreateImage, edit_image => EditImage, create_speech => CreateSpeech,
        create_transcription => CreateTranscription, create_translation => CreateTranslation,
        create_file => CreateFile, list_files => ListFiles, retrieve_file => RetrieveFile,
        retrieve_file_content => RetrieveFileContent, delete_file => DeleteFile,
        create_video => CreateVideo, retrieve_video => RetrieveVideo, list_videos => ListVideos,
        delete_video => DeleteVideo, download_video_content => DownloadVideoContent,
        create_realtime_call => CreateRealtimeCall,
    }
    ws_overrides! {
        connect_realtime => ConnectRealtime, generate_content_websocket => GenerateContent,
        stream_generate_content_websocket => StreamGenerateContent,
    }
}

#[tokio::test]
async fn every_operation_dispatches_to_its_independent_override_without_prepare() {
    let config = json!({});
    let secret = json!({});
    let client = Arc::new(Client::default());
    let binding = ChannelBinding::new(
        &Overrides,
        provider(&config),
        credential(&secret),
        client.clone(),
    );
    for spec in OPERATION_SPECS {
        let response = match spec.transport {
            OperationTransport::Http { .. } => binding
                .send(spec.key, request(HttpBody::Bytes(Bytes::new())))
                .await
                .unwrap(),
            OperationTransport::WebSocket => {
                let UpstreamConnection::Rejected(response) =
                    binding.connect(spec.key, request(())).await.unwrap()
                else {
                    panic!("expected marker")
                };
                response
            }
        };
        let HttpBody::Bytes(body) = response.body else {
            panic!("expected marker")
        };
        assert_eq!(body, spec.key.operation.id());
    }
    assert!(client.urls.lock().unwrap().is_empty());
}

struct PassThrough;
impl BaseChannel for PassThrough {
    fn id(&self) -> &'static str {
        "test"
    }
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        Ok(http::Request::builder()
            .uri(format!(
                "https://example.com/{}",
                ctx.operation.operation.id()
            ))
            .body(ctx.request.body)
            .unwrap())
    }
    fn prepare_connect(
        &self,
        ctx: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        Ok(http::Request::builder()
            .uri(format!(
                "wss://example.com/{}",
                ctx.operation.operation.id()
            ))
            .header(
                "authorization",
                ctx.credential.secret["api_key"].as_str().unwrap_or("key"),
            )
            .body(())
            .unwrap())
    }
}

#[tokio::test]
async fn common_http_preparation_receives_the_correct_operation() {
    let config = json!({});
    let secret = json!({});
    let client = Arc::new(Client::default());
    let binding = ChannelBinding::new(
        &PassThrough,
        provider(&config),
        credential(&secret),
        client.clone(),
    );
    for spec in OPERATION_SPECS
        .iter()
        .filter(|spec| matches!(spec.transport, OperationTransport::Http { .. }))
    {
        let result = binding
            .send(spec.key, request(HttpBody::Bytes(Bytes::new())))
            .await;
        let HttpBody::Bytes(body) = result.unwrap().body else {
            panic!("expected marker")
        };
        assert_eq!(body, format!("/{}", spec.key.operation.id()));
    }
}

struct TwoPages;
impl BaseChannel for TwoPages {
    fn id(&self) -> &'static str {
        "test"
    }
    fn list_models<'a>(&'a self, ctx: OperationContext<'a>) -> OperationFuture<'a, WireResponse> {
        Box::pin(async move {
            assert_eq!(ctx.credential.id, "c");
            assert_eq!(ctx.provider.id, "p");
            ctx.client
                .send(
                    http::Request::builder()
                        .uri("https://example.com/page1")
                        .body(HttpBody::Bytes(Bytes::new()))
                        .unwrap(),
                )
                .await?;
            ctx.client
                .send(
                    http::Request::builder()
                        .uri("https://example.com/page2")
                        .body(HttpBody::Bytes(Bytes::new()))
                        .unwrap(),
                )
                .await
                .map_err(ChannelError::from)
        })
    }
}

#[tokio::test]
async fn operation_override_can_make_multiple_calls_with_assigned_client() {
    let config = json!({});
    let secret = json!({});
    let client = Arc::new(Client::default());
    let key = OperationKey {
        operation: Operation::ListModels,
        dialect: Dialect::OpenAi,
    };
    let channel = TwoPages;
    let binding = ChannelBinding::new(
        &channel,
        provider(&config),
        credential(&secret),
        client.clone(),
    );
    binding
        .send(key, request(HttpBody::Bytes(Bytes::new())))
        .await
        .unwrap();
    assert_eq!(
        *client.urls.lock().unwrap(),
        ["https://example.com/page1", "https://example.com/page2"]
    );
}

struct DuplexClient {
    rejected: bool,
    sent: Arc<Mutex<Vec<WsFrame>>>,
    auth: Mutex<Vec<String>>,
}
impl OutboundClient for DuplexClient {
    fn send<'a>(
        &'a self,
        _: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        panic!("WebSocket must not use HTTP send")
    }
    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            self.auth
                .lock()
                .unwrap()
                .push(request.headers()["authorization"].to_str().unwrap().into());
            if self.rejected {
                let mut rejected = response("denied");
                rejected.status = StatusCode::FORBIDDEN;
                return Ok(UpstreamConnection::Rejected(rejected));
            }
            let sent = self.sent.clone();
            Ok(UpstreamConnection::Connected {
                handshake: WireResponse {
                    status: StatusCode::SWITCHING_PROTOCOLS,
                    headers: HeaderMap::new(),
                    body: (),
                },
                socket: WebSocket {
                    incoming: Box::pin(stream::iter([Ok(WsFrame::Text("hello".into()))])),
                    outgoing: Box::pin(sink::unfold(sent, |sent, frame| async move {
                        sent.lock().unwrap().push(frame);
                        Ok::<_, TransportError>(sent)
                    })),
                },
            })
        })
    }
}

#[tokio::test]
async fn websocket_preserves_duplex_frames_rejected_bodies_and_assigned_auth() {
    let config = json!({});
    let secret = json!({"api_key": "selected"});
    for rejected in [false, true] {
        let client = Arc::new(DuplexClient {
            rejected,
            sent: Arc::new(Mutex::new(Vec::new())),
            auth: Mutex::new(Vec::new()),
        });
        let binding = ChannelBinding::new(
            &PassThrough,
            provider(&config),
            credential(&secret),
            client.clone(),
        );
        for spec in OPERATION_SPECS
            .iter()
            .filter(|spec| matches!(spec.transport, OperationTransport::WebSocket))
        {
            match binding.connect(spec.key, request(())).await.unwrap() {
                UpstreamConnection::Connected {
                    handshake,
                    mut socket,
                } => {
                    assert!(!rejected);
                    assert_eq!(handshake.status, StatusCode::SWITCHING_PROTOCOLS);
                    assert_eq!(
                        socket.incoming.next().await.unwrap().unwrap(),
                        WsFrame::Text("hello".into())
                    );
                    socket
                        .outgoing
                        .send(WsFrame::Text("reply".into()))
                        .await
                        .unwrap();
                }
                UpstreamConnection::Rejected(response) => {
                    assert!(rejected);
                    assert_eq!(response.status, StatusCode::FORBIDDEN);
                    let HttpBody::Bytes(body) = response.body else {
                        panic!("expected rejection body")
                    };
                    assert_eq!(body, "denied");
                }
            }
        }
        assert_eq!(*client.auth.lock().unwrap(), ["selected"; 4]);
        assert_eq!(
            client.sent.lock().unwrap().len(),
            if rejected { 0 } else { 4 }
        );
    }
}

#[tokio::test]
async fn dispatch_uses_method_shape_and_leaves_dialect_support_to_channel() {
    let config = json!({});
    let secret = json!({});
    let client = Arc::new(Client::default());
    let binding = ChannelBinding::new(
        &PassThrough,
        provider(&config),
        credential(&secret),
        client.clone(),
    );
    let ws = OperationKey {
        operation: Operation::ConnectRealtime,
        dialect: Dialect::OpenAi,
    };
    let http = OperationKey {
        operation: Operation::ListModels,
        dialect: Dialect::OpenAi,
    };
    assert!(matches!(
        binding
            .send(ws, request(HttpBody::Bytes(Bytes::new())))
            .await,
        Err(ChannelError::WrongTransport(_))
    ));
    assert!(matches!(
        binding.connect(http, request(())).await,
        Err(ChannelError::WrongTransport(_))
    ));
    assert!(matches!(
        binding.connect(ws, request(())).await,
        Err(ChannelError::Transport(error)) if error.kind() == CapabilityErrorKind::Unsupported
    ));
    let channel_defined_pair = OperationKey {
        operation: Operation::ListModels,
        dialect: Dialect::OpenAiChat,
    };
    let HttpBody::Bytes(body) = binding
        .send(channel_defined_pair, request(HttpBody::Bytes(Bytes::new())))
        .await
        .unwrap()
        .body
    else {
        panic!("expected response from channel")
    };
    assert_eq!(body, "/list_models");
    assert_eq!(
        *client.urls.lock().unwrap(),
        ["https://example.com/list_models"]
    );
}
