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
/// Four of these are content generation and carry a [`ContentGenerationKind`];
/// the rest carry a [`WireFamily`].
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
    // video — the async-job core. Keyed `Family(OpenAi)`; the request body is
    // OpenAI's plus documented extensions, because OpenAI's own five fields
    // cap what the models can be asked for (three fixed durations, four fixed
    // sizes, one reference asset, no seed). Sora-only operations — remix,
    // edit, extend, characters — are deliberately absent: no other vendor
    // offers them. Add them back only when a second vendor does.
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

    /// Which kind variant this operation pairs with. An invariant of
    /// [`OperationKey`] construction and nothing more — deliberately private,
    /// because no caller has a reason to ask.
    ///
    /// The rule is **not** "the body is a conversation": `CountTokens` and
    /// `CompactContent` both carry one and still key on [`WireFamily`]. It is
    /// "the body is a conversation *and* the dialects need converting between
    /// each other", which is why only these four carry the dense 4x3 matrix.
    /// Cross-vendor demand for the other two is sparse enough — two pairs and
    /// one pair respectively — that a family split covers it.
    const fn is_content_generation(self) -> bool {
        matches!(
            self,
            Self::GenerateContent
                | Self::StreamGenerateContent
                | Self::GuardianReview
                | Self::GuardianClassify
        )
    }
}

/// Wire dialect for operations that are not content generation.
///
/// Names a dialect, not a configured backend — v2 called this `Provider` and
/// the two kept getting confused.
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

/// Conversation dialects. These form the dense 4x3 transform matrix.
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
pub enum ContentGenerationKind {
    #[serde(rename = "openai_chat")]
    #[strum(serialize = "openai_chat")]
    OpenAiChat,
    #[serde(rename = "openai_responses")]
    #[strum(serialize = "openai_responses")]
    OpenAiResponses,
    /// Envelope variant of [`ContentGenerationKind::OpenAiResponses`]: same
    /// semantics carried over a websocket. It never owns transform pairs of its
    /// own — the envelope layer unwraps it onto the Responses pairs and wraps
    /// the result back.
    #[serde(rename = "openai_responses_websocket")]
    #[strum(serialize = "openai_responses_websocket")]
    OpenAiResponsesWebSocket,
    ClaudeMessages,
    GeminiGenerateContent,
}

impl ContentGenerationKind {
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn from_id(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    /// The kind that owns the transform pairs for this dialect. Only the
    /// websocket envelope variant differs from itself.
    pub const fn pair_kind(self) -> Self {
        match self {
            Self::OpenAiResponsesWebSocket => Self::OpenAiResponses,
            other => other,
        }
    }
}

/// Which dialect an operation is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum OperationKind {
    ContentGeneration(ContentGenerationKind),
    Family(WireFamily),
}

impl OperationKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::ContentGeneration(kind) => kind.id(),
            Self::Family(family) => family.id(),
        }
    }
}

/// What transform pairs, channel support tables and routing rules key on.
///
/// The pairing is checked: a content-generation operation must carry a
/// [`ContentGenerationKind`], everything else a [`WireFamily`]. Constructing
/// `(ListModels, ClaudeMessages)` is a bug, not a state to handle downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationKey {
    operation: Operation,
    kind: OperationKind,
}

impl OperationKey {
    /// Panics if `operation` is not content generation. Use in const contexts
    /// where the pairing is known statically.
    pub const fn content(operation: Operation, kind: ContentGenerationKind) -> Self {
        assert!(
            operation.is_content_generation(),
            "content kind used with a non-content operation"
        );
        Self {
            operation,
            kind: OperationKind::ContentGeneration(kind),
        }
    }

    /// Panics if `operation` is content generation.
    pub const fn family(operation: Operation, family: WireFamily) -> Self {
        assert!(
            !operation.is_content_generation(),
            "wire family used with a content operation"
        );
        Self {
            operation,
            kind: OperationKind::Family(family),
        }
    }

    pub const fn try_new(
        operation: Operation,
        kind: OperationKind,
    ) -> Result<Self, OperationKeyError> {
        let consistent = matches!(kind, OperationKind::ContentGeneration(_))
            == operation.is_content_generation();
        if consistent {
            Ok(Self { operation, kind })
        } else {
            Err(OperationKeyError { operation, kind })
        }
    }

    pub const fn operation(self) -> Operation {
        self.operation
    }

    pub const fn kind(self) -> OperationKind {
        self.kind
    }

    /// The key whose transform pairs serve this one. Differs from `self` only
    /// for the websocket envelope variant.
    pub const fn pair_key(self) -> Self {
        match self.kind {
            OperationKind::ContentGeneration(kind) => Self {
                operation: self.operation,
                kind: OperationKind::ContentGeneration(kind.pair_kind()),
            },
            OperationKind::Family(_) => self,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationKeyError {
    pub operation: Operation,
    pub kind: OperationKind,
}

impl core::fmt::Display for OperationKeyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "operation `{}` cannot be expressed as `{}`",
            self.operation.id(),
            self.kind.id()
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

    #[test]
    fn dialect_ids_are_pinned() {
        let families: Vec<&'static str> = WireFamily::iter().map(WireFamily::id).collect();
        assert_eq!(families, ["openai", "claude", "gemini"]);

        let kinds: Vec<&'static str> = ContentGenerationKind::iter()
            .map(ContentGenerationKind::id)
            .collect();
        assert_eq!(
            kinds,
            [
                "openai_chat",
                "openai_responses",
                "openai_responses_websocket",
                "claude_messages",
                "gemini_generate_content",
            ]
        );
    }

    #[test]
    fn every_id_round_trips() {
        for operation in Operation::iter() {
            assert_eq!(Operation::from_id(operation.id()), Some(operation));
        }
        for family in WireFamily::iter() {
            assert_eq!(WireFamily::from_id(family.id()), Some(family));
        }
        for kind in ContentGenerationKind::iter() {
            assert_eq!(ContentGenerationKind::from_id(kind.id()), Some(kind));
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
        for kind in ContentGenerationKind::iter() {
            let json = serde_json::to_string(&kind).expect("serializes");
            assert_eq!(json, format!("\"{}\"", kind.id()));
        }
        for family in WireFamily::iter() {
            let json = serde_json::to_string(&family).expect("serializes");
            assert_eq!(json, format!("\"{}\"", family.id()));
        }
    }

    #[test]
    fn unknown_ids_are_rejected() {
        assert_eq!(Operation::from_id("not_an_operation"), None);
        assert_eq!(WireFamily::from_id("open_ai"), None);
    }

    #[test]
    fn kind_must_match_the_operation() {
        assert!(
            OperationKey::try_new(
                Operation::GenerateContent,
                OperationKind::Family(WireFamily::Claude),
            )
            .is_err()
        );
        assert!(
            OperationKey::try_new(
                Operation::ListModels,
                OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages),
            )
            .is_err()
        );
        assert!(
            OperationKey::try_new(
                Operation::ListModels,
                OperationKind::Family(WireFamily::Claude),
            )
            .is_ok()
        );
    }

    #[test]
    fn only_the_websocket_envelope_borrows_another_kinds_pairs() {
        let ws = OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponsesWebSocket,
        );
        assert_eq!(
            ws.pair_key(),
            OperationKey::content(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            )
        );

        for kind in [
            ContentGenerationKind::OpenAiChat,
            ContentGenerationKind::OpenAiResponses,
            ContentGenerationKind::ClaudeMessages,
            ContentGenerationKind::GeminiGenerateContent,
        ] {
            let key = OperationKey::content(Operation::GenerateContent, kind);
            assert_eq!(key.pair_key(), key);
        }
    }
}
