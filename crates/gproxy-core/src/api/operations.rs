use super::{CoreError, CoreResult, HttpExecution, WebSocketExecution};
use crate::{Core, RequestContext};
use gproxy_channel::ChannelError;
use gproxy_protocol::{HttpBody, Operation, WireRequest};
use gproxy_seaorm::BatchConnectionTrait;
use std::sync::Arc;

fn require_operation(context: &RequestContext, expected: Operation) -> CoreResult<()> {
    let actual = context.operation.operation;
    if actual != expected {
        return Err(CoreError::OperationMismatch { expected, actual });
    }
    Ok(())
}

// One list declares the named HTTP facade and the exhaustive generic dispatcher.
// A newly added protocol operation forces review of its core transport binding.
macro_rules! http_operations {
    ($($method:ident => $operation:ident),+ $(,)?) => {
        impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
            /// Generic HTTP entry point, preserving native streaming/multipart
            /// bodies. Targets and permitted credentials are supplied by the
            /// upper layer; no route or policy selection is performed here.
            pub async fn send(&self, context: Arc<RequestContext>, request: WireRequest<HttpBody>) -> CoreResult<HttpExecution> {
                // Boxed per arm: one frame holding every operation's execution
                // future overflows small stacks.
                match context.operation.operation {
                    $(Operation::$operation => Box::pin(self.$method(context, request)).await,)+
                    Operation::ConnectRealtime => Err(ChannelError::WrongTransport(context.operation).into()),
                }
            }
            $(
                #[doc = concat!("Invoke `", stringify!($operation), "` over HTTP. The context must name this operation; its dialect is preserved. Execution is pending.")]
                pub async fn $method(&self, context: Arc<RequestContext>, request: WireRequest<HttpBody>) -> CoreResult<HttpExecution> {
                    require_operation(&context, Operation::$operation)?;
                    self.execute_http(context, request).await
                }
            )+
        }
    };
}
http_operations! {
    list_models => ListModels,
    get_model => GetModel,
    count_tokens => CountTokens,
    generate_content => GenerateContent,
    stream_generate_content => StreamGenerateContent,
    guardian_review => GuardianReview,
    guardian_classify => GuardianClassify,
    compact_content => CompactContent,
    summarize_memory => SummarizeMemory,
    create_conversation => CreateConversation,
    create_embedding => CreateEmbedding,
    batch_create_embedding => BatchCreateEmbedding,
    rerank => Rerank,
    web_search => WebSearch,
    create_image => CreateImage,
    edit_image => EditImage,
    create_speech => CreateSpeech,
    create_transcription => CreateTranscription,
    create_translation => CreateTranslation,
    create_file => CreateFile,
    list_files => ListFiles,
    retrieve_file => RetrieveFile,
    retrieve_file_content => RetrieveFileContent,
    delete_file => DeleteFile,
    create_video => CreateVideo,
    retrieve_video => RetrieveVideo,
    list_videos => ListVideos,
    delete_video => DeleteVideo,
    download_video_content => DownloadVideoContent,
    create_realtime_call => CreateRealtimeCall,
}

macro_rules! websocket_operations {
    ($($method:ident => $operation:ident),+ $(,)?) => {
        impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
            /// Generic WS entry point. Rejected upgrades retain their complete
            /// HTTP response; connected sockets remain native duplex streams.
            pub async fn connect(&self, context: Arc<RequestContext>, request: WireRequest<()>) -> CoreResult<WebSocketExecution> {
                match context.operation.operation {
                    $(Operation::$operation => self.$method(context, request).await,)+
                    _ => Err(ChannelError::WrongTransport(context.operation).into()),
                }
            }
            $(
                #[doc = concat!("Invoke `", stringify!($operation), "` over WebSocket. The context must name this operation; execution is pending.")]
                pub async fn $method(&self, context: Arc<RequestContext>, request: WireRequest<()>) -> CoreResult<WebSocketExecution> {
                    require_operation(&context, Operation::$operation)?;
                    self.execute_websocket(context, request).await
                }
            )+
        }
    };
}
websocket_operations! {
    generate_content_websocket => GenerateContent,
    stream_generate_content_websocket => StreamGenerateContent,
    connect_realtime => ConnectRealtime,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
    /// Passthrough or conversion with credential selection, request/response
    /// rewriting, observation and settlement; wasm has no outbound transport yet.
    #[cfg(not(target_arch = "wasm32"))]
    async fn execute_http(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<HttpBody>,
    ) -> CoreResult<HttpExecution> {
        crate::execute::run_http(self, context, request).await
    }
    #[cfg(target_arch = "wasm32")]
    async fn execute_http(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<HttpBody>,
    ) -> CoreResult<HttpExecution> {
        let _ = (context, request);
        Err(CoreError::NotImplemented(
            "send: no outbound transport on wasm32 yet",
        ))
    }
    /// Handshake through the attempt loop; an established socket is observed
    /// in both directions and settles when it closes or is dropped.
    #[cfg(not(target_arch = "wasm32"))]
    async fn execute_websocket(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<()>,
    ) -> CoreResult<WebSocketExecution> {
        crate::execute::run_websocket(self, context, request).await
    }
    #[cfg(target_arch = "wasm32")]
    async fn execute_websocket(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<()>,
    ) -> CoreResult<WebSocketExecution> {
        let _ = (context, request);
        Err(CoreError::NotImplemented(
            "connect: no outbound transport on wasm32 yet",
        ))
    }
}
