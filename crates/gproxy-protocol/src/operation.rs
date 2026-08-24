//! Operation taxonomy: what a request *is*, independent of any provider.
//!
//! v2's model, kept: an [`Operation`] names the action, an [`OperationKind`]
//! names the wire shape it arrives in, and the pair ([`OperationKey`]) is
//! what routing rules and transforms key on. Content generation has several
//! kinds because OpenAI Responses and Chat Completions are genuinely
//! different wire shapes, not labels.
//!
//! Every enum here is `exhaustive`-feature gated: workspace builds match
//! exhaustively (adding a variant is a compile-error checklist), external
//! consumers see `#[non_exhaustive]`. The starter variant set covers the
//! first porting wave; it grows port by port, compiler-enforced.

/// Broad capability a client asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum OperationGroup {
    Models,
    CountTokens,
    Memories,
    GenerateContent,
    Compact,
    Embeddings,
    Images,
    Audio,
    Video,
    Files,
    Search,
    Rerank,
    Realtime,
}

/// Concrete action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Operation {
    ListModels,
    GetModel,
    CountTokens,
    SummarizeMemory,
    GenerateContent,
    StreamGenerateContent,
    CompactContent,
    CreateEmbedding,
    BatchCreateEmbedding,
    Rerank,
    WebSearch,
    CreateImage,
    EditImage,
    CreateSpeech,
    CreateTranscription,
    CreateTranslation,
    CreateFile,
    ListFiles,
    RetrieveFile,
    RetrieveFileContent,
    DeleteFile,
    CreateVideo,
    RetrieveVideo,
    ListVideos,
    DeleteVideo,
    DownloadVideoContent,
    RemixVideo,
    CreateVideoCharacter,
    GetVideoCharacter,
    EditVideo,
    ExtendVideo,
    /// SDP handshake creating a WebRTC realtime call (`/v1/realtime/calls`).
    /// The session's WS/observer operations arrive with the round-3
    /// websocket-ingress design.
    CreateRealtimeCall,
}

/// Wire family for provider-shaped (non-content-generation) operations.
/// v2 called this `Provider`; renamed — it names a wire dialect, not a
/// configured backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum WireFamily {
    OpenAi,
    Claude,
    Gemini,
}

/// The distinct content-generation wire shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ContentGenerationKind {
    OpenAiChat,
    OpenAiResponses,
    /// Envelope variant of `OpenAiResponses`: same semantics over a
    /// websocket transport. Transforms compose it onto the Responses
    /// pairs; it never gets pair families of its own.
    OpenAiResponsesWebSocket,
    ClaudeMessages,
    GeminiGenerateContent,
}

/// Wire shape of an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum OperationKind {
    ContentGeneration(ContentGenerationKind),
    Family(WireFamily),
}

/// What routing rules, transforms, and channel support tables key on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationKey {
    pub operation: Operation,
    pub kind: OperationKind,
}

impl OperationKey {
    pub const fn content(operation: Operation, kind: ContentGenerationKind) -> Self {
        Self {
            operation,
            kind: OperationKind::ContentGeneration(kind),
        }
    }

    pub const fn family(operation: Operation, family: WireFamily) -> Self {
        Self {
            operation,
            kind: OperationKind::Family(family),
        }
    }
}

impl OperationGroup {
    /// Stable permission and persistence id.
    pub const fn id(self) -> &'static str {
        use OperationGroup::*;
        match self {
            Models => "models",
            CountTokens => "count_tokens",
            Memories => "memories",
            GenerateContent => "generate_content",
            Compact => "compact",
            Embeddings => "embeddings",
            Images => "images",
            Audio => "audio",
            Video => "video",
            Files => "files",
            Search => "search",
            Rerank => "rerank",
            Realtime => "realtime",
        }
    }
}

impl Operation {
    /// Stable persistence id. Exhaustive so adding an operation cannot silently
    /// collapse into a debug-string or catch-all representation.
    pub const fn id(self) -> &'static str {
        use Operation::*;
        match self {
            ListModels => "list_models",
            GetModel => "get_model",
            CountTokens => "count_tokens",
            SummarizeMemory => "summarize_memory",
            GenerateContent => "generate_content",
            StreamGenerateContent => "stream_generate_content",
            CompactContent => "compact_content",
            CreateEmbedding => "create_embedding",
            BatchCreateEmbedding => "batch_create_embedding",
            Rerank => "rerank",
            WebSearch => "web_search",
            CreateImage => "create_image",
            EditImage => "edit_image",
            CreateSpeech => "create_speech",
            CreateTranscription => "create_transcription",
            CreateTranslation => "create_translation",
            CreateFile => "create_file",
            ListFiles => "list_files",
            RetrieveFile => "retrieve_file",
            RetrieveFileContent => "retrieve_file_content",
            DeleteFile => "delete_file",
            CreateVideo => "create_video",
            RetrieveVideo => "retrieve_video",
            ListVideos => "list_videos",
            DeleteVideo => "delete_video",
            DownloadVideoContent => "download_video_content",
            RemixVideo => "remix_video",
            CreateVideoCharacter => "create_video_character",
            GetVideoCharacter => "get_video_character",
            EditVideo => "edit_video",
            ExtendVideo => "extend_video",
            CreateRealtimeCall => "create_realtime_call",
        }
    }

    /// Exhaustive by design: a new operation fails to compile until its
    /// group — and its [`spec`](crate::spec::OperationSpec) — exist.
    pub const fn group(self) -> OperationGroup {
        use Operation::*;
        match self {
            ListModels | GetModel => OperationGroup::Models,
            CountTokens => OperationGroup::CountTokens,
            SummarizeMemory => OperationGroup::Memories,
            GenerateContent | StreamGenerateContent => OperationGroup::GenerateContent,
            CompactContent => OperationGroup::Compact,
            CreateEmbedding | BatchCreateEmbedding => OperationGroup::Embeddings,
            Rerank => OperationGroup::Rerank,
            WebSearch => OperationGroup::Search,
            CreateImage | EditImage => OperationGroup::Images,
            CreateSpeech | CreateTranscription | CreateTranslation => OperationGroup::Audio,
            CreateFile | ListFiles | RetrieveFile | RetrieveFileContent | DeleteFile => {
                OperationGroup::Files
            }
            CreateVideo | RetrieveVideo | ListVideos | DeleteVideo | DownloadVideoContent
            | RemixVideo | CreateVideoCharacter | GetVideoCharacter | EditVideo | ExtendVideo => {
                OperationGroup::Video
            }
            CreateRealtimeCall => OperationGroup::Realtime,
        }
    }
}
