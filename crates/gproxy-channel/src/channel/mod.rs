//! A small required contract plus optional, independently implemented abilities.
//! Channels own upstream knowledge; the host supplies configuration, credentials
//! and a client. Neither channel identity nor credentials are global enums.

mod binding;
mod descriptor;
mod fallback;
mod headers;
mod oauth;
mod operations;
mod quota;
pub(crate) mod reason;
mod refresh;
mod registry;
mod service;
mod state;
mod usage;

pub use binding::ChannelBinding;
pub use descriptor::{
    ChannelCapabilities, ChannelDescriptor, ConfigKey, ConfigKeyKind, HOST_CONFIG_KEYS, LoginMode,
};
pub use fallback::{CLAUDE_FALLBACK_KEYS, ClaudeFallback, claude_fallback_model};
pub use headers::{ChannelHeaders, HeaderAllowlist, forwardable};
pub use oauth::{
    AcquiredCredential, AuthorizationCode, AuthorizationRequest, AuthorizationStart, CookieLogin,
    DeviceAuthorization, DevicePoll, LoginContext, OAuthAuthorizationCode, OAuthCredential,
    OAuthDeviceCode,
};
pub use operations::{OperationContext, OperationFuture};
pub use quota::{
    QuotaAllowance, QuotaBalance, QuotaBreakdownRow, QuotaDimension, QuotaEntry,
    QuotaHeaderContext, QuotaHeaders, QuotaMetric, QuotaModel, QuotaQuery, QuotaReset,
    QuotaResetBehavior, QuotaResetCredits, QuotaResetOption, QuotaResetOutcome, QuotaResetRequest,
    QuotaResetResult, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue,
    QuotaWindow,
};
pub use registry::{ChannelRegistry, RegistryError};
pub use service::{
    CallerIdentity, CallerRole, CallerUsage, CallerUsageWindow, ChannelServices, ResourceAccess,
    ResourceBindingRecord, ServiceCaller, ServiceClass, ServiceContext, ServiceRoute,
    ServiceTransport, ServiceView, match_template,
};
pub use state::{ChannelState, NoState};
pub use usage::{
    NormalizedUsage, ResponseUsage, ResponseView, TokenUsage, UsageAttempt, UsageCompleteness,
    UsageContext, UsageExtractor, UsageFrame, UsageObserver, UsageStream, UsageStreamContext,
    UsageStreamEnd, UsageTransport,
};

pub use reason::{ResponseReason, ResponseReasonObserver, standard_reason_observer};

pub use refresh::{CredentialRefresh, CredentialUpdate, RefreshContext};

use gproxy_client::{ConnectionConfig, OutboundClient};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
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
    #[error("invalid channel configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid credential")]
    InvalidCredential,
    /// The upstream definitively refused to refresh this credential
    /// (invalid_grant, revoked or expired refresh token). Retrying later does
    /// not help; the host marks the credential dead until a person logs in again.
    /// Transient transport or 5xx failures must not use this variant.
    #[error("credential refresh rejected: {0}")]
    RefreshRejected(String),
    /// A continuation of an earlier request is held by another host process
    /// (a live upstream connection cannot move). The host routes the request
    /// to that instance; nothing about the credential is wrong.
    #[error("continuation is held by instance `{instance_id}`")]
    ContinuationElsewhere { instance_id: String },
    /// The host's caller facts (`ServiceCaller`) could not be read or written.
    #[error("host service failed: {0}")]
    Host(String),
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
    /// Public, non-secret facts the host recorded for this credential, such
    /// as the plan a login discovered; `QuotaModel` reads its dimensions here.
    pub metadata: &'a Value,
    pub version: i64,
    pub expires_at_ms: Option<i64>,
}

/// The caller's selected account and client, shared by refresh, quotas and CLI
/// services. This is passed through directly, without additional binding checks.
#[derive(Clone, Copy)]
pub struct CredentialContext<'a> {
    pub provider: ProviderView<'a>,
    pub credential: CredentialView<'a>,
    pub client: &'a dyn OutboundClient,
}

pub struct PrepareContext<'a, B = HttpBody> {
    pub provider: ProviderView<'a>,
    pub credential: CredentialView<'a>,
    /// The native operation after any host-owned protocol adaptation.
    pub operation: OperationKey,
    pub request: WireRequest<B>,
    /// Complete method URL configured for this provider/operation; see
    /// `OperationContext::endpoint_override`.
    pub endpoint_override: Option<&'a str>,
}

/// Transport defaults vary between API calls and browser cookie login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPurpose {
    Request,
    CookieLogin,
}

/// Channel identity plus independently overridable operation methods.
///
/// Only `id` is required. Preparation defaults to UnsupportedOperation.
/// Implement common preparation or override individual asynchronous operations;
/// each implementation determines which requests it supports.
/// Optional abilities outside protocol operations retain default-None accessors.
pub trait BaseChannel: Send + Sync {
    fn id(&self) -> &'static str;

    /// This channel as configuration data: identity, login flows, optional
    /// abilities, provider configuration keys. The default answers from the
    /// capability accessors below, names the channel after its id and declares
    /// no configuration keys, which is correct for a channel whose provider
    /// rows carry nothing but the common host keys. Concrete channels override
    /// it to add a display name and the keys they decode.
    fn descriptor(&self) -> ChannelDescriptor {
        let mut login_modes = Vec::new();
        if self.oauth_authorization_code().is_some() {
            login_modes.push(LoginMode::AuthorizationCode);
        }
        if self.oauth_device_code().is_some() {
            login_modes.push(LoginMode::DeviceCode);
        }
        if self.cookie_login().is_some() {
            login_modes.push(LoginMode::Cookie);
        }
        ChannelDescriptor {
            id: self.id(),
            display_name: self.id(),
            login_modes,
            capabilities: ChannelCapabilities {
                refresh: self.credential_refresh().is_some(),
                quota_query: self.quota_query().is_some(),
                quota_reset: self.quota_reset().is_some(),
                services: self.services().is_some(),
                // No accessor describes transport; a socket channel says so itself.
                websocket: false,
            },
            config_keys: Vec::new(),
        }
    }

    /// The outbound client this channel's upstream expects when nothing names
    /// one: the host resolves credential profile → provider profile → Setting default → this → `ConnectionConfig::default()`. Channels whose
    /// upstream fingerprints its clients return the captured client identity;
    /// any explicit host profile on the credential or provider still wins.
    fn default_connection(&self) -> Option<ConnectionConfig> {
        None
    }

    /// A login may need a browser transport while normal API traffic does not.
    fn default_connection_for(&self, purpose: ConnectionPurpose) -> Option<ConnectionConfig> {
        let _ = purpose;
        self.default_connection()
    }

    /// An explicit conversion destination for channels accepting multiple native wires.
    fn default_conversion_target(
        &self,
        provider: ProviderView<'_>,
        source: OperationKey,
    ) -> Option<OperationKey> {
        let _ = (provider, source);
        None
    }

    /// Native methods which produce their answer without an upstream request.
    fn local_operations(&self) -> &'static [Operation] {
        &[]
    }

    /// The wire dialects this provider's upstream accepts natively for an
    /// operation. The collection declares support, not preference. The host uses it to choose between
    /// default mappings. Explicit provider mappings are keyed by operation
    /// and incoming dialect, not by position in this collection.
    /// Empty means the channel declares nothing and configuration must decide.
    fn native_dialects(&self, provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        let _ = (provider, operation);
        Vec::new()
    }

    /// Gateway-executed refusal fallback, only for native Claude generation.
    /// Upstreams that execute their own model fallback leave this unset.
    fn claude_fallback(&self) -> Option<ClaudeFallback> {
        None
    }

    fn fallback_model(&self, primary: &str, fallback: &str) -> String {
        claude_fallback_model(primary, fallback)
    }

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

    /// Classifies raw upstream responses independently of usage extraction.
    /// Override for private wire formats; standard JSON/SSE protocols share a parser.
    fn response_reason_observer(
        &self,
        _status: http::StatusCode,
        headers: &http::HeaderMap,
        max_bytes: u64,
    ) -> Option<Box<dyn ResponseReasonObserver>> {
        standard_reason_observer(headers, max_bytes)
    }

    /// Classify one complete incoming WS message; control frames carry no model verdict.
    /// The transport has already reassembled WebSocket fragmentation.
    fn websocket_response_reason(
        &self,
        frame: &gproxy_protocol::connection::WsFrame,
    ) -> Option<ResponseReason> {
        reason::websocket_reason(frame)
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
