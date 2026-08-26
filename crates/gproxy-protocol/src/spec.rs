//! The OperationSpec registry: every fact about an operation, declared once.
//!
//! v2 scattered these facts across 10+ match sites and five parallel
//! billable lists; here classification, settlement, affinity, and console
//! metadata all read one declaration. Request-body expectations are not a
//! field — they derive from the ingress method (single truth).

use http::Method;

use crate::operation::{Operation, OperationKind};

/// One path segment of an ingress pattern. Static tables, no matcher DSL:
/// the full pattern language is exactly what the known APIs need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    /// Literal segment: `v1`, `files`.
    Lit(&'static str),
    /// Capture one segment: `{id}`.
    Param(&'static str),
    /// Gemini's `models/{model}:generateContent` shape — a capture with a
    /// literal `:action` suffix in the same segment.
    ParamAction(&'static str, &'static str),
    /// Capture the whole remaining path (service-surface prefixes). Only
    /// valid as the final segment.
    Rest(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathPattern(pub &'static [Seg]);

/// How to tell a streaming request from a buffered one at this ingress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDetect {
    /// Never streams.
    Never,
    /// A boolean body field (`"stream"`) decides; classification promotes
    /// the operation to its streaming sibling when set.
    BodyFlag(&'static str),
    /// A string body field must equal the declared value.
    BodyValue(&'static str, &'static str),
    /// A boolean field accepted from JSON or a multipart form field. Media
    /// endpoints arrive as multipart and cannot be classified through JSON.
    BodyFlagOrMultipart(&'static str),
    /// The endpoint itself is the streaming form (`:streamGenerateContent`).
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFraming {
    Sse,
    WebSocket,
    JsonArray,
}

/// One way this operation enters the proxy.
#[derive(Debug, Clone, Copy)]
pub struct Ingress {
    pub method: &'static Method,
    pub pattern: PathPattern,
    pub kind: OperationKind,
    pub stream: StreamDetect,
    pub framing: StreamFraming,
    /// This ingress is a websocket upgrade (`GET /v1/realtime`,
    /// Responses-over-WS). The engine hands matched upgrades to the WS
    /// bridge instead of the HTTP path; hosts never hardcode WS routes
    /// (v2's gateway carried a three-branch if-chain for exactly this).
    pub upgrade: bool,
}

/// When the funnel settles this operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleMode {
    /// Not billable; settles at zero for telemetry only.
    Free,
    /// Settle from the response (or stream tail) usage.
    OnResponse,
    /// Long-lived session: the setup response carries no usage; settle when
    /// the trusted server-side observer closes.
    OnSessionEnd,
    /// Async-job pattern (video): settle only when the polled body reports
    /// `status == "completed"`, deduplicated across polls.
    OnCompletedStatus,
}

/// Which credential-stickiness the operation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affinity {
    None,
    /// Conversation stickiness via session id / fingerprint.
    Session,
    /// The named resource (`"file"`, `"video"`) is bound to the credential
    /// that created it; follow-up calls must land there.
    Resource(&'static str),
}

/// Everything the engine needs to know about an operation.
#[derive(Debug, Clone, Copy)]
pub struct OperationSpec {
    pub ingress: &'static [Ingress],
    pub settle: SettleMode,
    pub affinity: Affinity,
}

/// A successful ingress match.
#[derive(Debug)]
pub struct Matched {
    pub operation: Operation,
    pub kind: OperationKind,
    pub stream: StreamDetect,
    pub framing: StreamFraming,
    pub upgrade: bool,
    /// Captured `Param`/`ParamAction` values, in pattern order.
    pub params: Vec<(&'static str, String)>,
}

pub const fn default_framing(kind: OperationKind, upgrade: bool) -> StreamFraming {
    if upgrade {
        return StreamFraming::WebSocket;
    }
    match kind {
        OperationKind::ContentGeneration(
            crate::operation::ContentGenerationKind::GeminiGenerateContent,
        ) => StreamFraming::JsonArray,
        OperationKind::ContentGeneration(
            crate::operation::ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => StreamFraming::WebSocket,
        OperationKind::ContentGeneration(
            crate::operation::ContentGenerationKind::OpenAiChat
            | crate::operation::ContentGenerationKind::OpenAiResponses
            | crate::operation::ContentGenerationKind::ClaudeMessages,
        )
        | OperationKind::Family(_) => StreamFraming::Sse,
    }
}

/// Streaming promotion: which operation a `BodyFlag` ingress becomes when
/// the flag is set. Exhaustive so a new streaming pair cannot be missed.
pub const fn streaming_sibling(operation: Operation) -> Option<Operation> {
    match operation {
        Operation::GenerateContent => Some(Operation::StreamGenerateContent),
        Operation::ListModels
        | Operation::GetModel
        | Operation::CountTokens
        | Operation::SummarizeMemory
        | Operation::StreamGenerateContent
        | Operation::CompactContent
        | Operation::CreateEmbedding
        | Operation::BatchCreateEmbedding
        | Operation::Rerank
        | Operation::WebSearch
        | Operation::CreateImage
        | Operation::EditImage
        | Operation::CreateSpeech
        | Operation::CreateTranscription
        | Operation::CreateTranslation
        | Operation::CreateFile
        | Operation::ListFiles
        | Operation::RetrieveFile
        | Operation::RetrieveFileContent
        | Operation::DeleteFile
        | Operation::CreateVideo
        | Operation::RetrieveVideo
        | Operation::ListVideos
        | Operation::DeleteVideo
        | Operation::DownloadVideoContent
        | Operation::RemixVideo
        | Operation::CreateVideoCharacter
        | Operation::GetVideoCharacter
        | Operation::EditVideo
        | Operation::ExtendVideo
        | Operation::CreateRealtimeCall => None,
    }
}

impl Operation {
    /// The one declaration everything reads. Exhaustive: a new operation
    /// does not compile until its spec exists.
    pub fn spec(self) -> &'static OperationSpec {
        crate::specs::spec(self)
    }
}
