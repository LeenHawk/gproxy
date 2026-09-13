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

/// What the caller wants done.
///
/// Four of these are content generation (see [`Operation::is_content_generation`])
/// and carry a [`ContentGenerationKind`]; the rest carry a [`WireFamily`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    /// Stable persistence id. Exhaustive on purpose: adding an operation must
    /// not silently collapse into a debug string or a catch-all.
    pub const fn id(self) -> &'static str {
        use Operation::*;
        match self {
            ListModels => "list_models",
            GetModel => "get_model",
            CountTokens => "count_tokens",
            GenerateContent => "generate_content",
            StreamGenerateContent => "stream_generate_content",
            GuardianReview => "guardian_review",
            GuardianClassify => "guardian_classify",
            CompactContent => "compact_content",
            SummarizeMemory => "summarize_memory",
            CreateConversation => "create_conversation",
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
            CreateRealtimeCall => "create_realtime_call",
            ConnectRealtime => "connect_realtime",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        use Operation::*;
        Some(match value {
            "list_models" => ListModels,
            "get_model" => GetModel,
            "count_tokens" => CountTokens,
            "generate_content" => GenerateContent,
            "stream_generate_content" => StreamGenerateContent,
            "guardian_review" => GuardianReview,
            "guardian_classify" => GuardianClassify,
            "compact_content" => CompactContent,
            "summarize_memory" => SummarizeMemory,
            "create_conversation" => CreateConversation,
            "create_embedding" => CreateEmbedding,
            "batch_create_embedding" => BatchCreateEmbedding,
            "rerank" => Rerank,
            "web_search" => WebSearch,
            "create_image" => CreateImage,
            "edit_image" => EditImage,
            "create_speech" => CreateSpeech,
            "create_transcription" => CreateTranscription,
            "create_translation" => CreateTranslation,
            "create_file" => CreateFile,
            "list_files" => ListFiles,
            "retrieve_file" => RetrieveFile,
            "retrieve_file_content" => RetrieveFileContent,
            "delete_file" => DeleteFile,
            "create_video" => CreateVideo,
            "retrieve_video" => RetrieveVideo,
            "list_videos" => ListVideos,
            "delete_video" => DeleteVideo,
            "download_video_content" => DownloadVideoContent,
            "create_realtime_call" => CreateRealtimeCall,
            "connect_realtime" => ConnectRealtime,
            _ => return None,
        })
    }
}

/// Wire dialect for operations that are not content generation.
///
/// Names a dialect, not a configured backend — v2 called this `Provider` and
/// the two kept getting confused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum WireFamily {
    OpenAi,
    Claude,
    Gemini,
}

impl WireFamily {
    pub const fn id(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Some(match value {
            "openai" => Self::OpenAi,
            "claude" => Self::Claude,
            "gemini" => Self::Gemini,
            _ => return None,
        })
    }
}

/// Conversation dialects. These form the dense 4x3 transform matrix.
///
/// There is no AWS variant: Bedrock Converse has no ingress path, so no client
/// speaks it to us. It is an upstream shape the `aws_bedrock` channel produces,
/// and its wire types live with that channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ContentGenerationKind {
    OpenAiChat,
    OpenAiResponses,
    /// Envelope variant of [`ContentGenerationKind::OpenAiResponses`]: same
    /// semantics carried over a websocket. It never owns transform pairs of its
    /// own — the envelope layer unwraps it onto the Responses pairs and wraps
    /// the result back.
    OpenAiResponsesWebSocket,
    ClaudeMessages,
    GeminiGenerateContent,
}

impl ContentGenerationKind {
    pub const fn id(self) -> &'static str {
        match self {
            Self::OpenAiChat => "openai_chat",
            Self::OpenAiResponses => "openai_responses",
            Self::OpenAiResponsesWebSocket => "openai_responses_websocket",
            Self::ClaudeMessages => "claude_messages",
            Self::GeminiGenerateContent => "gemini_generate_content",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Some(match value {
            "openai_chat" => Self::OpenAiChat,
            "openai_responses" => Self::OpenAiResponses,
            "openai_responses_websocket" => Self::OpenAiResponsesWebSocket,
            "claude_messages" => Self::ClaudeMessages,
            "gemini_generate_content" => Self::GeminiGenerateContent,
            _ => return None,
        })
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
    pub const fn id(self) -> &'static str {
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

    const ALL: &[Operation] = &[
        Operation::ListModels,
        Operation::GetModel,
        Operation::CountTokens,
        Operation::GenerateContent,
        Operation::StreamGenerateContent,
        Operation::GuardianReview,
        Operation::GuardianClassify,
        Operation::CompactContent,
        Operation::SummarizeMemory,
        Operation::CreateConversation,
        Operation::CreateEmbedding,
        Operation::BatchCreateEmbedding,
        Operation::Rerank,
        Operation::WebSearch,
        Operation::CreateImage,
        Operation::EditImage,
        Operation::CreateSpeech,
        Operation::CreateTranscription,
        Operation::CreateTranslation,
        Operation::CreateFile,
        Operation::ListFiles,
        Operation::RetrieveFile,
        Operation::RetrieveFileContent,
        Operation::DeleteFile,
        Operation::CreateVideo,
        Operation::RetrieveVideo,
        Operation::ListVideos,
        Operation::DeleteVideo,
        Operation::DownloadVideoContent,
        Operation::CreateRealtimeCall,
        Operation::ConnectRealtime,
    ];

    #[test]
    fn every_operation_round_trips_through_its_id() {
        assert_eq!(ALL.len(), 31, "update ALL when adding an operation");
        for operation in ALL {
            assert_eq!(Operation::from_id(operation.id()), Some(*operation));
        }
    }

    #[test]
    fn operation_ids_are_unique() {
        let mut ids: Vec<_> = ALL.iter().map(|operation| operation.id()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "two operations share a persistence id");
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
