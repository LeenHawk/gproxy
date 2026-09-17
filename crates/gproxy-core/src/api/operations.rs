use super::{CoreError, CoreResult, HttpExecution, WebSocketExecution};
use crate::{Core, RequestContext};
use gproxy_channel::ChannelError;
use gproxy_protocol::{HttpBody, Operation, WireRequest};
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
        impl<C> Core<C> {
            /// Generic HTTP entry point, preserving native streaming/multipart
            /// bodies. Targets and permitted credentials are supplied by the
            /// upper layer; no route or policy selection is performed here.
            pub async fn send(&self, context: Arc<RequestContext>, request: WireRequest<HttpBody>) -> CoreResult<HttpExecution> {
                match context.operation.operation {
                    $(Operation::$operation => self.$method(context, request).await,)+
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
        impl<C> Core<C> {
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

impl<C> Core<C> {
    // Private shared execution boundaries only. Internal selection, refresh,
    // rewrite and outcome steps will be added when their implementations exist.
    async fn execute_http(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<HttpBody>,
    ) -> CoreResult<HttpExecution> {
        let _ = (context, request);
        Err(CoreError::NotImplemented("send"))
    }
    async fn execute_websocket(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<()>,
    ) -> CoreResult<WebSocketExecution> {
        let _ = (context, request);
        Err(CoreError::NotImplemented("connect"))
    }
}
