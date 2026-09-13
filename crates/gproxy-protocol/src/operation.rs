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
/// Four of these are content generation and carry a conversation dialect; the
/// rest carry a plain family dialect. See [`Dialect`].
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
    /// Not an upstream call. Every video vendor hands back a time-limited
    /// signed URL rather than serving bytes, so this operation is synthesized:
    /// the engine fetches that URL and relays it.
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
    /// Stable persistence id.
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn from_id(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    /// Whether this operation takes a conversation dialect. An invariant of
    /// [`OperationKey`] construction and nothing more — deliberately private,
    /// because no caller has a reason to ask.
    ///
    /// The rule is **not** "the body is a conversation": `CountTokens` and
    /// `CompactContent` both carry one and still take a family dialect. It is
    /// "the body is a conversation *and* the dialects need converting between
    /// each other", which is why only these four carry the dense 4x3 matrix.
    /// Cross-vendor demand for the other two is sparse enough — two pairs and
    /// one pair respectively — that a family split covers it.
    const fn takes_conversation_dialect(self) -> bool {
        matches!(
            self,
            Self::GenerateContent
                | Self::StreamGenerateContent
                | Self::GuardianReview
                | Self::GuardianClassify
        )
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
/// One flat axis. The first five are conversation shapes and form the dense
/// 4x3 transform matrix; the last three are the plain family shapes every other
/// operation uses.
///
/// This was two nested enums — a `ContentGeneration(..)` variant beside a
/// `Family(..)` one — which modelled the two as alternatives when they are the
/// same axis at different zoom levels. `OpenAiChat` already says "OpenAI", so
/// the outer wrapper only restated what the inner value carried. Flat removes
/// that, and [`Dialect::family`] recovers the coarse view by derivation.
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
    // conversation shapes
    #[serde(rename = "openai_chat")]
    #[strum(serialize = "openai_chat")]
    OpenAiChat,
    #[serde(rename = "openai_responses")]
    #[strum(serialize = "openai_responses")]
    OpenAiResponses,
    /// Envelope variant of [`Dialect::OpenAiResponses`]: same semantics carried
    /// over a websocket. It never owns transform pairs of its own — the
    /// envelope layer unwraps it onto the Responses pairs and wraps the result
    /// back.
    #[serde(rename = "openai_responses_websocket")]
    #[strum(serialize = "openai_responses_websocket")]
    OpenAiResponsesWebSocket,
    ClaudeMessages,
    GeminiGenerateContent,
    // family shapes, for everything that is not a conversation
    #[serde(rename = "openai")]
    #[strum(serialize = "openai")]
    OpenAi,
    Claude,
    Gemini,
}

impl Dialect {
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn from_id(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    /// Whether this is one of the conversation shapes.
    pub const fn is_conversation(self) -> bool {
        matches!(
            self,
            Self::OpenAiChat
                | Self::OpenAiResponses
                | Self::OpenAiResponsesWebSocket
                | Self::ClaudeMessages
                | Self::GeminiGenerateContent
        )
    }

    /// The vendor family this shape belongs to.
    pub const fn family(self) -> WireFamily {
        match self {
            Self::OpenAiChat
            | Self::OpenAiResponses
            | Self::OpenAiResponsesWebSocket
            | Self::OpenAi => WireFamily::OpenAi,
            Self::ClaudeMessages | Self::Claude => WireFamily::Claude,
            Self::GeminiGenerateContent | Self::Gemini => WireFamily::Gemini,
        }
    }

    /// The dialect that owns the transform pairs for this one. Only the
    /// websocket envelope variant differs from itself.
    pub const fn pair_dialect(self) -> Self {
        match self {
            Self::OpenAiResponsesWebSocket => Self::OpenAiResponses,
            other => other,
        }
    }
}

/// What transform pairs, channel support tables and routing rules key on.
///
/// The pairing is checked: a content-generation operation must carry a
/// conversation dialect, everything else a family dialect. Constructing
/// `(ListModels, ClaudeMessages)` is a bug, not a state to handle downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationKey {
    operation: Operation,
    dialect: Dialect,
}

impl OperationKey {
    /// Panics if the operation and dialect disagree. Use in const contexts
    /// where the pairing is known statically.
    pub const fn new(operation: Operation, dialect: Dialect) -> Self {
        assert!(
            operation.takes_conversation_dialect() == dialect.is_conversation(),
            "operation and dialect disagree on whether this is a conversation"
        );
        Self { operation, dialect }
    }

    pub const fn try_new(
        operation: Operation,
        dialect: Dialect,
    ) -> Result<Self, OperationKeyError> {
        if operation.takes_conversation_dialect() == dialect.is_conversation() {
            Ok(Self { operation, dialect })
        } else {
            Err(OperationKeyError { operation, dialect })
        }
    }

    pub const fn operation(self) -> Operation {
        self.operation
    }

    pub const fn dialect(self) -> Dialect {
        self.dialect
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationKeyError {
    pub operation: Operation,
    pub dialect: Dialect,
}

impl core::fmt::Display for OperationKeyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "operation `{}` cannot be expressed as `{}`",
            self.operation.id(),
            self.dialect.id()
        )
    }
}

impl core::error::Error for OperationKeyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// Pins every persistence id. The derive generates these from the variant
    /// names, so without this test renaming a variant would silently rewrite
    /// what is already stored in a database. Order and count are pinned too,
    /// which is what makes adding an operation a deliberate act.
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

    /// Flattening must not have changed any stored string: a conversation
    /// dialect used to serialize through the inner enum and a family dialect
    /// through the outer one, and both produced exactly these.
    #[test]
    fn dialect_ids_are_pinned() {
        let ids: Vec<&'static str> = Dialect::iter().map(Dialect::id).collect();
        assert_eq!(
            ids,
            [
                "openai_chat",
                "openai_responses",
                "openai_responses_websocket",
                "claude_messages",
                "gemini_generate_content",
                "openai",
                "claude",
                "gemini",
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
        assert_eq!(Dialect::OpenAiResponses.family(), WireFamily::OpenAi);
        assert_eq!(
            Dialect::OpenAiResponsesWebSocket.family(),
            WireFamily::OpenAi
        );
        assert_eq!(Dialect::OpenAi.family(), WireFamily::OpenAi);
        assert_eq!(Dialect::ClaudeMessages.family(), WireFamily::Claude);
        assert_eq!(Dialect::Claude.family(), WireFamily::Claude);
        assert_eq!(Dialect::GeminiGenerateContent.family(), WireFamily::Gemini);
        assert_eq!(Dialect::Gemini.family(), WireFamily::Gemini);
    }

    #[test]
    fn exactly_five_dialects_are_conversations() {
        let conversations: Vec<Dialect> = Dialect::iter().filter(|d| d.is_conversation()).collect();
        assert_eq!(
            conversations,
            [
                Dialect::OpenAiChat,
                Dialect::OpenAiResponses,
                Dialect::OpenAiResponsesWebSocket,
                Dialect::ClaudeMessages,
                Dialect::GeminiGenerateContent,
            ]
        );
    }

    #[test]
    fn dialect_must_match_the_operation() {
        assert!(OperationKey::try_new(Operation::GenerateContent, Dialect::Claude).is_err());
        assert!(OperationKey::try_new(Operation::ListModels, Dialect::ClaudeMessages).is_err());
        assert!(OperationKey::try_new(Operation::ListModels, Dialect::Claude).is_ok());
        assert!(OperationKey::try_new(Operation::GenerateContent, Dialect::ClaudeMessages).is_ok());
    }

    #[test]
    fn only_the_websocket_envelope_borrows_another_dialects_pairs() {
        let ws = OperationKey::new(
            Operation::StreamGenerateContent,
            Dialect::OpenAiResponsesWebSocket,
        );
        assert_eq!(
            ws.pair_key(),
            OperationKey::new(Operation::StreamGenerateContent, Dialect::OpenAiResponses)
        );

        for dialect in Dialect::iter().filter(|d| *d != Dialect::OpenAiResponsesWebSocket) {
            assert_eq!(dialect.pair_dialect(), dialect);
        }
    }
}
