//! A small required contract plus optional, independently implemented abilities.
//! Channels own upstream knowledge; the host supplies configuration, credentials
//! and a client. Neither channel identity nor credentials are global enums.

mod binding;
mod oauth;
mod operations;
mod quota;
mod refresh;
mod service;
mod usage;

pub use binding::ChannelBinding;
pub use oauth::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, CookieLogin, DeviceAuthorization,
    DevicePoll, LoginContext, OAuthAuthorizationCode, OAuthCredential, OAuthDeviceCode,
};
pub use operations::{OperationContext, OperationFuture};
pub use quota::{
    QuotaAllowance, QuotaBalance, QuotaDimension, QuotaEntry, QuotaHeaderContext, QuotaHeaders,
    QuotaMetric, QuotaModel, QuotaQuery, QuotaReset, QuotaResetBehavior, QuotaResetCredits,
    QuotaResetOutcome, QuotaResetResult, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking,
    QuotaValue, QuotaWindow,
};
pub use service::{ChannelServices, ServiceContext, ServiceRoute, ServiceTransport};
pub use usage::{
    NormalizedUsage, ResponseView, TokenUsage, UsageAttempt, UsageCompleteness, UsageContext,
    UsageExtractor, UsageFrame, UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd,
    UsageTransport,
};

pub use refresh::{CredentialRefresh, CredentialUpdate, RefreshContext};

use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, UpstreamConnection},
};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("channel service is not supported")]
    UnsupportedService,
    #[error("invalid upstream response: {0}")]
    InvalidResponse(String),
    #[error("upstream returned {status}")]
    UpstreamResponse {
        status: http::StatusCode,
        body: gproxy_protocol::connection::Bytes,
    },
    #[error("operation is not supported: {0:?}")]
    UnsupportedOperation(OperationKey),
    #[error("wrong transport for operation: {0:?}")]
    WrongTransport(OperationKey),
    #[error("the assigned client does not support WebSocket connections")]
    WebSocketUnavailable,
    #[error("invalid channel configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid credential")]
    InvalidCredential,
    #[error(transparent)]
    Transport(#[from] CapabilityError),
}

/// Public provider configuration. Channels decode `config` into their own typed
/// settings; secrets belong exclusively to CredentialView.
#[derive(Clone, Copy)]
pub struct ProviderView<'a> {
    pub id: &'a str,
    pub channel: &'a str,
    pub base_url: Option<&'a str>,
    pub config: &'a Value,
}

/// A decrypted, host-authorized credential borrowed for this binding.
/// Intentionally not Debug or serializable. The host owns decryption and checks
/// owner permissions, enabled state and expiry before binding.
#[derive(Clone, Copy)]
pub struct CredentialView<'a> {
    pub id: &'a str,
    pub provider_id: &'a str,
    pub auth_kind: &'a str,
    pub secret: &'a Value,
    pub version: i64,
    pub expires_at_ms: Option<i64>,
}

/// The caller's selected account and client, shared by refresh, quotas and CLI
/// services. This is passed through directly, without additional binding checks.
#[derive(Clone, Copy)]
pub struct CredentialContext<'a> {
    pub provider: ProviderView<'a>,
    pub credential: CredentialView<'a>,
    pub client: &'a dyn crate::OutboundClient,
}

pub struct PrepareContext<'a, B = HttpBody> {
    pub provider: ProviderView<'a>,
    pub credential: CredentialView<'a>,
    /// The native operation after any host-owned protocol adaptation.
    pub operation: OperationKey,
    pub request: WireRequest<B>,
}

/// Channel identity plus independently overridable operation methods.
///
/// Only `id` is required. Preparation defaults to UnsupportedOperation.
/// Implement common preparation or override individual asynchronous operations;
/// each implementation determines which requests it supports.
/// Optional abilities outside protocol operations retain default-None accessors.
pub trait BaseChannel: Send + Sync {
    fn id(&self) -> &'static str;

    /// Common HTTP preparation used by the default operation implementations.
    /// Remove source authentication, inject the assigned credential, and filter
    /// forwarding headers. Channels may instead override an entire operation.
    fn prepare(
        &self,
        context: PrepareContext<'_>,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        Err(ChannelError::UnsupportedOperation(context.operation))
    }

    /// Common WebSocket handshake preparation; a socket is never an HTTP body.
    fn prepare_connect(
        &self,
        context: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        Err(ChannelError::UnsupportedOperation(context.operation))
    }

    /// List the models visible to the assigned credential.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::ListModels, context)
    }

    /// Retrieve one model.
    fn get_model<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::GetModel, context)
    }

    /// Count tokens using the channel's native operation.
    fn count_tokens<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CountTokens, context)
    }

    /// Generate a complete response over HTTP.
    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::GenerateContent, context)
    }

    /// Generate an incremental HTTP response; preserve the body stream.
    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::StreamGenerateContent, context)
    }

    /// Review content for safety.
    fn guardian_review<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::GuardianReview, context)
    }

    /// Classify content using the native guardian operation.
    fn guardian_classify<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::GuardianClassify, context)
    }

    /// Compact an existing context.
    fn compact_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CompactContent, context)
    }

    /// Summarize conversation memory.
    fn summarize_memory<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::SummarizeMemory, context)
    }

    /// Create a conversation resource.
    fn create_conversation<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateConversation, context)
    }

    /// Create embeddings.
    fn create_embedding<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateEmbedding, context)
    }

    /// Create a batch of embeddings.
    fn batch_create_embedding<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::BatchCreateEmbedding, context)
    }

    /// Rerank documents.
    fn rerank<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::Rerank, context)
    }

    /// Perform native web search.
    fn web_search<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::WebSearch, context)
    }

    /// Create images.
    fn create_image<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateImage, context)
    }

    /// Edit images; multipart may remain streamed.
    fn edit_image<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::EditImage, context)
    }

    /// Generate speech; binary responses may remain streamed.
    fn create_speech<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateSpeech, context)
    }

    /// Transcribe audio.
    fn create_transcription<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateTranscription, context)
    }

    /// Translate audio.
    fn create_translation<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateTranslation, context)
    }

    /// Upload a file without imposing buffering.
    fn create_file<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateFile, context)
    }

    /// List files visible to the assigned credential.
    fn list_files<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::ListFiles, context)
    }

    /// Retrieve file metadata.
    fn retrieve_file<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::RetrieveFile, context)
    }

    /// Download file content.
    fn retrieve_file_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::RetrieveFileContent, context)
    }

    /// Delete a file.
    fn delete_file<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::DeleteFile, context)
    }

    /// Submit video generation; returning a job is not job completion.
    fn create_video<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateVideo, context)
    }

    /// Retrieve a video job and its current status.
    fn retrieve_video<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::RetrieveVideo, context)
    }

    /// List video jobs.
    fn list_videos<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::ListVideos, context)
    }

    /// Delete a video job.
    fn delete_video<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::DeleteVideo, context)
    }

    /// Download video output through its native HTTP endpoint. Channels that
    /// obtain a download URL through a separate call may override this method.
    fn download_video_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::DownloadVideoContent, context)
    }

    /// Create a WebRTC call through its HTTP SDP handshake.
    fn create_realtime_call<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        operations::http(self, Operation::CreateRealtimeCall, context)
    }

    /// Connect a duplex Realtime session.
    fn connect_realtime<'a>(
        &'a self,
        context: OperationContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        operations::websocket(self, Operation::ConnectRealtime, context)
    }

    /// Connect the Responses WebSocket generation envelope.
    fn generate_content_websocket<'a>(
        &'a self,
        context: OperationContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        operations::websocket(self, Operation::GenerateContent, context)
    }

    /// Connect the streaming Responses WebSocket generation envelope.
    fn stream_generate_content_websocket<'a>(
        &'a self,
        context: OperationContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        operations::websocket(self, Operation::StreamGenerateContent, context)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        None
    }

    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        None
    }

    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        None
    }

    fn cookie_login(&self) -> Option<&dyn CookieLogin> {
        None
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        None
    }

    /// Declares which quota dimensions this channel's credentials have.
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        None
    }

    fn quota_reset(&self) -> Option<&dyn QuotaReset> {
        None
    }

    fn quota_headers(&self) -> Option<&dyn QuotaHeaders> {
        None
    }

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        None
    }

    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        None
    }

    fn services(&self) -> Option<&dyn ChannelServices> {
        None
    }
}
