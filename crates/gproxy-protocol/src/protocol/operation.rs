//! Shared operation taxonomy and endpoint metadata.

use serde::{Deserialize, Serialize};

/// Upstream protocol family.
///
/// Provider-specific wire modules (`openai`, `claude`, `gemini`) should reuse
/// this enum when declaring endpoint metadata or routing rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Provider {
    OpenAi,
    Claude,
    Gemini,
}

/// Coarse operation family, used to organize protocol support by capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperationGroup {
    Models,
    CountTokens,
    GenerateContent,
    Images,
    Search,
    Embeddings,
    Compact,
    Conversation,
    Realtime,
    Audio,
    Video,
    Files,
}

/// Provider-neutral operation name.
///
/// Variants are capability-oriented. A provider module should model only the
/// variants that the provider actually exposes; unsupported operations are not
/// represented by synthetic request/response types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Operation {
    ListModels,
    GetModel,
    CountTokens,
    GenerateContent,
    StreamGenerateContent,
    CreateImage,
    EditImage,
    WebSearch,
    Rerank,
    CreateEmbedding,
    CreateSpeech,
    CreateTranscription,
    CreateTranslation,
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
    CreateFile,
    ListFiles,
    RetrieveFile,
    DeleteFile,
    DownloadFileContent,
    CompactContent,
    CreateConversation,
    CreateRealtimeCall,
    ConnectRealtime,
}

impl Operation {
    /// Return the operation group for this operation.
    pub const fn group(self) -> OperationGroup {
        match self {
            Self::ListModels | Self::GetModel => OperationGroup::Models,
            Self::CountTokens => OperationGroup::CountTokens,
            Self::GenerateContent | Self::StreamGenerateContent => OperationGroup::GenerateContent,
            Self::CreateImage | Self::EditImage => OperationGroup::Images,
            Self::WebSearch | Self::Rerank => OperationGroup::Search,
            Self::CreateEmbedding => OperationGroup::Embeddings,
            Self::CreateSpeech | Self::CreateTranscription | Self::CreateTranslation => {
                OperationGroup::Audio
            }
            Self::CreateVideo
            | Self::RetrieveVideo
            | Self::ListVideos
            | Self::DeleteVideo
            | Self::DownloadVideoContent
            | Self::RemixVideo
            | Self::CreateVideoCharacter
            | Self::GetVideoCharacter
            | Self::EditVideo
            | Self::ExtendVideo => OperationGroup::Video,
            Self::CreateFile
            | Self::ListFiles
            | Self::RetrieveFile
            | Self::DeleteFile
            | Self::DownloadFileContent => OperationGroup::Files,
            Self::CompactContent => OperationGroup::Compact,
            Self::CreateConversation => OperationGroup::Conversation,
            Self::CreateRealtimeCall | Self::ConnectRealtime => OperationGroup::Realtime,
        }
    }

    /// Whether requests of this operation carry a JSON body.
    pub const fn has_request_body(self) -> bool {
        !matches!(
            self,
            Self::ListModels
                | Self::GetModel
                | Self::RetrieveVideo
                | Self::ListVideos
                | Self::DeleteVideo
                | Self::DownloadVideoContent
                | Self::GetVideoCharacter
                | Self::ConnectRealtime
                | Self::ListFiles
                | Self::RetrieveFile
                | Self::DeleteFile
                | Self::DownloadFileContent
        )
    }
}

/// Wire-format kind used together with [`Operation`].
///
/// Content generation needs a four-way kind because OpenAI has two distinct
/// native formats for the same capability. Non-content operations only need the
/// three provider families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum OperationKind {
    ContentGeneration(ContentGenerationKind),
    Provider(Provider),
}

impl OperationKind {
    pub const fn provider(self) -> Provider {
        match self {
            Self::ContentGeneration(kind) => kind.provider(),
            Self::Provider(provider) => provider,
        }
    }

    pub const fn is_content_generation(self) -> bool {
        matches!(self, Self::ContentGeneration(_))
    }
}

/// Content-generation wire formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentGenerationKind {
    OpenAiResponses,
    #[serde(rename = "open_ai_responses_websocket")]
    OpenAiResponsesWebSocket,
    OpenAiChatCompletions,
    ClaudeMessages,
    GeminiGenerateContent,
}

impl ContentGenerationKind {
    pub const fn provider(self) -> Provider {
        match self {
            Self::OpenAiResponses
            | Self::OpenAiResponsesWebSocket
            | Self::OpenAiChatCompletions => Provider::OpenAi,
            Self::ClaudeMessages => Provider::Claude,
            Self::GeminiGenerateContent => Provider::Gemini,
        }
    }
}

/// Capability plus wire-format kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub struct OperationKey {
    operation: Operation,
    kind: OperationKind,
}

impl OperationKey {
    pub fn content_generation(operation: Operation, kind: ContentGenerationKind) -> Self {
        assert!(
            operation.is_content_generation(),
            "content-generation kind used with non-content operation"
        );
        Self {
            operation,
            kind: OperationKind::ContentGeneration(kind),
        }
    }

    pub fn provider(operation: Operation, provider: Provider) -> Self {
        assert!(
            !operation.is_content_generation(),
            "provider kind used with content-generation operation"
        );
        Self {
            operation,
            kind: OperationKind::Provider(provider),
        }
    }

    pub const fn group(self) -> OperationGroup {
        self.operation.group()
    }

    /// Return the capability operation protected by this key's invariant.
    pub const fn operation(self) -> Operation {
        self.operation
    }

    /// Return the provider wire-format kind protected by this key's invariant.
    pub const fn kind(self) -> OperationKind {
        self.kind
    }

    pub const fn provider_family(self) -> Provider {
        self.kind.provider()
    }

    pub const fn is_consistent(self) -> bool {
        self.operation.is_content_generation() == self.kind.is_content_generation()
    }

    pub const fn try_new(
        operation: Operation,
        kind: OperationKind,
    ) -> Result<Self, OperationKeyError> {
        let key = Self { operation, kind };
        if key.is_consistent() {
            Ok(key)
        } else {
            Err(OperationKeyError { operation, kind })
        }
    }

    #[cfg(test)]
    pub(crate) const fn new_unchecked(operation: Operation, kind: OperationKind) -> Self {
        Self { operation, kind }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct OperationKeyError {
    pub operation: Operation,
    pub kind: OperationKind,
}

impl std::fmt::Display for OperationKeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "operation {:?} is inconsistent with kind {:?}",
            self.operation, self.kind
        )
    }
}

impl std::error::Error for OperationKeyError {}

impl<'de> Deserialize<'de> for OperationKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireOperationKey {
            operation: Operation,
            kind: OperationKind,
        }

        let wire = WireOperationKey::deserialize(deserializer)?;
        Self::try_new(wire.operation, wire.kind).map_err(serde::de::Error::custom)
    }
}

impl Operation {
    pub const fn is_content_generation(self) -> bool {
        matches!(self, Self::GenerateContent | Self::StreamGenerateContent)
    }
}

/// HTTP method for an upstream endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[non_exhaustive]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl From<HttpMethod> for http::Method {
    fn from(m: HttpMethod) -> Self {
        match m {
            HttpMethod::Get => http::Method::GET,
            HttpMethod::Post => http::Method::POST,
            HttpMethod::Put => http::Method::PUT,
            HttpMethod::Patch => http::Method::PATCH,
            HttpMethod::Delete => http::Method::DELETE,
        }
    }
}

/// Provider endpoint metadata used by routing and protocol modules.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[non_exhaustive]
pub struct Endpoint {
    pub operation_key: OperationKey,
    pub method: HttpMethod,
    /// Provider-relative path template, e.g. `/v1/chat/completions`.
    pub path: String,
}

impl Endpoint {
    pub fn new(operation_key: OperationKey, method: HttpMethod, path: impl Into<String>) -> Self {
        Self {
            operation_key,
            method,
            path: path.into(),
        }
    }

    pub fn content_generation(
        operation: Operation,
        kind: ContentGenerationKind,
        method: HttpMethod,
        path: impl Into<String>,
    ) -> Self {
        Self::new(
            OperationKey::content_generation(operation, kind),
            method,
            path,
        )
    }

    pub fn provider(
        operation: Operation,
        provider: Provider,
        method: HttpMethod,
        path: impl Into<String>,
    ) -> Self {
        Self::new(OperationKey::provider(operation, provider), method, path)
    }

    pub const fn provider_family(&self) -> Provider {
        self.operation_key.provider_family()
    }

    pub const fn group(&self) -> OperationGroup {
        self.operation_key.group()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialization_rejects_inconsistent_operation_key() {
        let value = serde_json::json!({
            "operation": "generate_content",
            "kind": "open_ai"
        });
        assert!(serde_json::from_value::<OperationKey>(value).is_err());
    }

    #[test]
    fn try_new_checks_the_invariant() {
        assert!(
            OperationKey::try_new(
                Operation::GenerateContent,
                OperationKind::Provider(Provider::OpenAi),
            )
            .is_err()
        );
    }

    #[test]
    fn rerank_is_a_provider_shaped_search_operation() {
        assert_eq!(Operation::Rerank.group(), OperationGroup::Search);
        assert!(Operation::Rerank.has_request_body());
        assert!(OperationKey::provider(Operation::Rerank, Provider::OpenAi).is_consistent());
    }

    #[test]
    fn video_operations_have_expected_group_and_body_semantics() {
        for operation in [
            Operation::CreateVideo,
            Operation::RetrieveVideo,
            Operation::ListVideos,
            Operation::DeleteVideo,
            Operation::DownloadVideoContent,
            Operation::RemixVideo,
            Operation::CreateVideoCharacter,
            Operation::GetVideoCharacter,
            Operation::EditVideo,
            Operation::ExtendVideo,
        ] {
            assert_eq!(operation.group(), OperationGroup::Video);
            assert!(OperationKey::provider(operation, Provider::OpenAi).is_consistent());
        }

        assert!(Operation::CreateVideo.has_request_body());
        assert!(Operation::RemixVideo.has_request_body());
        assert!(!Operation::RetrieveVideo.has_request_body());
        assert!(!Operation::ListVideos.has_request_body());
        assert!(!Operation::DeleteVideo.has_request_body());
        assert!(!Operation::DownloadVideoContent.has_request_body());
        assert!(!Operation::GetVideoCharacter.has_request_body());
    }
}
