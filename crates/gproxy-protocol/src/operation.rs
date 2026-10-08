//! Operation identity: what a request is asking for, and in whose dialect.
//!
//! An operation is declared once here and nowhere else. Everything downstream
//! — transform pairs, channel support tables, routing rules — keys on
//! [`OperationKey`], so adding a variant produces a compile-error checklist
//! rather than a runtime fallthrough.
//!
//! Paths are deliberately absent. Which URL serves an operation is an HTTP
//! convention owned by the ingress layer; an SDK caller names the operation
//! directly and never sees a path.
//!
//! Persistence strings live on the variants rather than in a hand-written
//! match. `snake_case` conversion is exact for every operation; the OpenAI
//! dialects need an explicit spelling because the derive would otherwise split
//! them into `open_ai`. A pinning test guards the whole set, so renaming a
//! variant cannot silently change what is already in a database.

/// What the caller wants done.
///
/// The operation and [`Dialect`] together identify the wire shape.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    strum::IntoStaticStr,
    strum::EnumString,
    strum::EnumIter,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Operation {
    // models
    ListModels,
    GetModel,
    // counting
    CountTokens,
    // generation
    GenerateContent,
    StreamGenerateContent,
    // moderation
    /// Standard OpenAI moderation JSON, distinct from Codex Guardian's SSE.
    CreateModeration,
    /// Typed predicate, choice and score answers from the OpenAI Decisions API.
    CreateDecision,
    GuardianReview,
    GuardianClassify,
    // context management
    CompactContent,
    SummarizeMemory,
    CreateConversation,
    // embeddings
    CreateEmbedding,
    BatchCreateEmbedding,
    // retrieval
    Rerank,
    // alpha search for codex (maybe include by other providers later)
    WebSearch,
    // images
    CreateImage,
    EditImage,
    // audio
    CreateSpeech,
    CreateTranscription,
    CreateTranslation,
    // files
    CreateFile,
    ListFiles,
    RetrieveFile,
    RetrieveFileContent,
    DeleteFile,
    // video — the async-job core. Keyed with the OpenAI family dialect; the
    // request body is OpenAI's plus documented extensions, because OpenAI's own
    // five fields cap what the models can be asked for (three fixed durations,
    // four fixed sizes, one reference asset, no seed). Sora-only operations —
    // remix, edit, extend, characters — are deliberately absent: no other
    // vendor offers them. Add them back only when a second vendor does.
    CreateVideo,
    RetrieveVideo,
    ListVideos,
    DeleteVideo,
    /// Download video bytes, e.g. OpenAI's GET /videos/{video_id}/content.
    /// Some vendors instead expose a download URL in a job response; their
    /// channel implementation resolves that URL as part of this operation.
    DownloadVideoContent,
    // realtime
    /// SDP handshake creating a WebRTC realtime call.
    CreateRealtimeCall,
    /// Duplex websocket session. Never transformed — only same-protocol
    /// passthrough, because a client that can keep talking mid-response has no
    /// equivalent in a half-duplex dialect.
    ConnectRealtime,
}

impl Operation {
    /// Whether this operation performs inference or a metered tool action.
    /// Catalog, token-counting, resource management and signaling-only calls
    /// belong in request logs, not request-level usage statistics.
    pub const fn produces_usage(self) -> bool {
        match self {
            Self::GenerateContent
            | Self::StreamGenerateContent
            | Self::CreateModeration
            | Self::CreateDecision
            | Self::GuardianReview
            | Self::GuardianClassify
            | Self::CompactContent
            | Self::SummarizeMemory
            | Self::CreateEmbedding
            | Self::BatchCreateEmbedding
            | Self::Rerank
            | Self::WebSearch
            | Self::CreateImage
            | Self::EditImage
            | Self::CreateSpeech
            | Self::CreateTranscription
            | Self::CreateTranslation
            | Self::CreateVideo
            | Self::ConnectRealtime => true,
            Self::ListModels
            | Self::GetModel
            | Self::CountTokens
            | Self::CreateConversation
            | Self::CreateFile
            | Self::ListFiles
            | Self::RetrieveFile
            | Self::RetrieveFileContent
            | Self::DeleteFile
            | Self::RetrieveVideo
            | Self::ListVideos
            | Self::DeleteVideo
            | Self::DownloadVideoContent
            | Self::CreateRealtimeCall => false,
        }
    }

    /// Shared by persistence and historical usage queries.
    pub fn usage_operations() -> impl Iterator<Item = Self> {
        <Self as strum::IntoEnumIterator>::iter().filter(|op| op.produces_usage())
    }

    /// Stable persistence id.
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn from_id(value: &str) -> Option<Self> {
        value.parse().ok()
    }
}

/// Vendor grouping of dialects. Derived from a [`Dialect`], never stored
/// alongside one.
///
/// Names a dialect family, not a configured backend — v2 called this
/// `Provider` and the two kept getting confused. Which vendor actually *serves*
/// a dialect is the channel's business: Anthropic and Google both expose
/// OpenAI-compatible endpoints, and that shows up as channel support, not here.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    strum::IntoStaticStr,
    strum::EnumString,
    strum::EnumIter,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum WireFamily {
    #[serde(rename = "openai")]
    #[strum(serialize = "openai")]
    OpenAi,
    Claude,
    Gemini,
}

impl WireFamily {
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn from_id(value: &str) -> Option<Self> {
        value.parse().ok()
    }
}

/// The wire shape an operation is expressed in.
///
/// The three vendor dialects cover all operations. For content generation,
/// they mean OpenAI Responses, Claude Messages and Gemini GenerateContent.
/// OpenAI additionally has Chat Completions and a Responses websocket envelope.
/// [`Dialect::family`] derives the vendor grouping from this single axis.
///
/// There is no AWS variant: Bedrock Converse has no ingress path, so no client
/// speaks it to us. It is an upstream shape the `aws_bedrock` channel produces,
/// and its wire types live with that channel.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    strum::IntoStaticStr,
    strum::EnumString,
    strum::EnumIter,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Dialect {
    /// OpenAI's shape for the operation; Responses for content generation.
    #[serde(rename = "openai")]
    #[strum(serialize = "openai")]
    OpenAi,
    /// Claude's shape for the operation; Messages for content generation.
    Claude,
    /// Gemini's shape for the operation; GenerateContent for content generation.
    Gemini,
    /// OpenAI Chat Completions, for conversation operations only.
    #[serde(rename = "openai_chat")]
    #[strum(serialize = "openai_chat")]
    OpenAiChat,
    /// Responses over a websocket, for conversation operations only. The
    /// envelope layer unwraps onto [`Dialect::OpenAi`]'s Responses pairs and
    /// wraps the result back; this variant owns no transform pairs of its own.
    #[serde(rename = "openai_responses_websocket")]
    #[strum(serialize = "openai_responses_websocket")]
    OpenAiResponsesWebSocket,
}

impl Dialect {
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn from_id(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    /// The vendor family this shape belongs to.
    pub const fn family(self) -> WireFamily {
        match self {
            Self::OpenAi | Self::OpenAiChat | Self::OpenAiResponsesWebSocket => WireFamily::OpenAi,
            Self::Claude => WireFamily::Claude,
            Self::Gemini => WireFamily::Gemini,
        }
    }

    /// The dialect that owns the transform pairs for this one. Only the
    /// websocket envelope variant differs from itself.
    pub const fn pair_dialect(self) -> Self {
        match self {
            Self::OpenAiResponsesWebSocket => Self::OpenAi,
            other => other,
        }
    }
}

/// What transform pairs, channel support tables and routing rules key on.
///
/// Construct directly from an operation and dialect. Supported combinations
/// are defined by transform and channel support tables, not by this type.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct OperationKey {
    pub operation: Operation,
    pub dialect: Dialect,
}

impl OperationKey {
    pub const fn family(self) -> WireFamily {
        self.dialect.family()
    }

    /// The key whose transform pairs serve this one. Differs from `self` only
    /// for the websocket envelope variant.
    pub const fn pair_key(self) -> Self {
        Self {
            operation: self.operation,
            dialect: self.dialect.pair_dialect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// Pins every persistence id. The derive generates these from the variant
    /// names, so without this test renaming a variant would silently rewrite
    /// what is already stored in a database. Order and count are pinned too,
    /// which is what makes adding an operation a deliberate act.
    #[test]
    fn only_inference_and_metered_tool_operations_produce_usage() {
        for op in [
            Operation::ListModels,
            Operation::GetModel,
            Operation::CountTokens,
            Operation::CreateConversation,
            Operation::CreateFile,
            Operation::ListFiles,
            Operation::RetrieveFile,
            Operation::RetrieveFileContent,
            Operation::DeleteFile,
            Operation::RetrieveVideo,
            Operation::ListVideos,
            Operation::DeleteVideo,
            Operation::DownloadVideoContent,
            Operation::CreateRealtimeCall,
        ] {
            assert!(!op.produces_usage(), "{}", op.id());
        }
        assert!(Operation::GenerateContent.produces_usage());
        assert!(Operation::CreateEmbedding.produces_usage());
        assert!(Operation::ConnectRealtime.produces_usage());
    }

    #[test]
    fn operation_ids_are_pinned() {
        let ids: Vec<&'static str> = Operation::iter().map(Operation::id).collect();
        assert_eq!(
            ids,
            [
                "list_models",
                "get_model",
                "count_tokens",
                "generate_content",
                "stream_generate_content",
                "create_moderation",
                "create_decision",
                "guardian_review",
                "guardian_classify",
                "compact_content",
                "summarize_memory",
                "create_conversation",
                "create_embedding",
                "batch_create_embedding",
                "rerank",
                "web_search",
                "create_image",
                "edit_image",
                "create_speech",
                "create_transcription",
                "create_translation",
                "create_file",
                "list_files",
                "retrieve_file",
                "retrieve_file_content",
                "delete_file",
                "create_video",
                "retrieve_video",
                "list_videos",
                "delete_video",
                "download_video_content",
                "create_realtime_call",
                "connect_realtime",
            ]
        );
    }

    /// Pins the five canonical dialect ids used by serde and strum.
    #[test]
    fn dialect_ids_are_pinned() {
        let ids: Vec<&'static str> = Dialect::iter().map(Dialect::id).collect();
        assert_eq!(
            ids,
            [
                "openai",
                "claude",
                "gemini",
                "openai_chat",
                "openai_responses_websocket",
            ]
        );

        let families: Vec<&'static str> = WireFamily::iter().map(WireFamily::id).collect();
        assert_eq!(families, ["openai", "claude", "gemini"]);
    }

    #[test]
    fn every_id_round_trips() {
        for operation in Operation::iter() {
            assert_eq!(Operation::from_id(operation.id()), Some(operation));
        }
        for dialect in Dialect::iter() {
            assert_eq!(Dialect::from_id(dialect.id()), Some(dialect));
        }
        for family in WireFamily::iter() {
            assert_eq!(WireFamily::from_id(family.id()), Some(family));
        }
    }

    /// serde and strum derive their strings separately; nothing forces them to
    /// agree, so check that they do.
    #[test]
    fn serde_agrees_with_strum() {
        for operation in Operation::iter() {
            let json = serde_json::to_string(&operation).expect("serializes");
            assert_eq!(json, format!("\"{}\"", operation.id()));
            let back: Operation = serde_json::from_str(&json).expect("deserializes");
            assert_eq!(back, operation);
        }
        for dialect in Dialect::iter() {
            let json = serde_json::to_string(&dialect).expect("serializes");
            assert_eq!(json, format!("\"{}\"", dialect.id()));
            let back: Dialect = serde_json::from_str(&json).expect("deserializes");
            assert_eq!(back, dialect);
        }
        for family in WireFamily::iter() {
            let json = serde_json::to_string(&family).expect("serializes");
            assert_eq!(json, format!("\"{}\"", family.id()));
        }
    }

    #[test]
    fn unknown_ids_are_rejected() {
        assert_eq!(Operation::from_id("not_an_operation"), None);
        assert_eq!(Dialect::from_id("open_ai"), None);
        assert_eq!(WireFamily::from_id("open_ai"), None);
    }

    /// Every dialect belongs to exactly one family, and the family shapes are
    /// the identity of their own family.
    #[test]
    fn families_are_derivable() {
        assert_eq!(Dialect::OpenAiChat.family(), WireFamily::OpenAi);
        assert_eq!(
            Dialect::OpenAiResponsesWebSocket.family(),
            WireFamily::OpenAi
        );
        assert_eq!(Dialect::OpenAi.family(), WireFamily::OpenAi);
        assert_eq!(Dialect::Claude.family(), WireFamily::Claude);
        assert_eq!(Dialect::Gemini.family(), WireFamily::Gemini);
    }

    #[test]
    fn only_the_websocket_envelope_borrows_another_dialects_pairs() {
        let ws = OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiResponsesWebSocket,
        };
        assert_eq!(
            ws.pair_key(),
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            }
        );

        for dialect in Dialect::iter().filter(|d| *d != Dialect::OpenAiResponsesWebSocket) {
            assert_eq!(dialect.pair_dialect(), dialect);
        }
    }
}
