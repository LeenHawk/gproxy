use serde::{Deserialize, Serialize};

pub type RequestId = String;
pub type AnthropicOrganizationId = String;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicHeaders {
    #[serde(rename = "anthropic-version")]
    pub anthropic_version: AnthropicVersion,
    #[serde(rename = "anthropic-beta", skip_serializing_if = "Option::is_none")]
    pub anthropic_beta: Option<AnthropicBetaHeader>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AnthropicVersion {
    #[default]
    #[serde(rename = "2023-06-01")]
    V20230601,
    #[serde(rename = "2023-01-01")]
    V20230101,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicResponseHeaders {
    #[serde(rename = "request-id")]
    pub request_id: RequestId,
    #[serde(
        rename = "anthropic-organization-id",
        skip_serializing_if = "Option::is_none"
    )]
    pub anthropic_organization_id: Option<AnthropicOrganizationId>,
}

pub use crate::claude::count_tokens::types::*;
pub use crate::claude::count_tokens::{
    BetaCountTokensContextManagementResponse, BetaMessageTokensCount, CountTokensHeaders,
    CountTokensRequest, CountTokensRequestBody, CountTokensResponse,
};
pub use crate::claude::create_message::stream::{
    BetaStreamContentBlock, BetaStreamContentBlockDelta, BetaStreamEvent, BetaStreamEventKnown,
    BetaStreamMessage, BetaStreamMessageDelta, BetaStreamUsage,
};
pub use crate::claude::create_message::types::BetaMessage;
pub use crate::claude::create_message::{
    CreateMessageHeaders, CreateMessageRequest, CreateMessageRequestBody, CreateMessageResponse,
};
pub use crate::claude::error::{ErrorDetail, ErrorResponse, ErrorResponseType, ErrorType};
pub use crate::claude::get_model::{
    GetModelHeaders, GetModelPath, GetModelRequest, GetModelResponse, ModelInfo,
};
pub use crate::claude::list_models::{
    BetaModelInfo, ListModelsHeaders, ListModelsQuery, ListModelsRequest, ListModelsResponse,
    ModelType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnthropicBetaKnown {
    #[serde(rename = "message-batches-2024-09-24")]
    MessageBatches20240924,
    #[serde(rename = "prompt-caching-2024-07-31")]
    PromptCaching20240731,
    #[serde(rename = "computer-use-2024-10-22")]
    ComputerUse20241022,
    #[serde(rename = "computer-use-2025-01-24")]
    ComputerUse20250124,
    #[serde(rename = "pdfs-2024-09-25")]
    Pdfs20240925,
    #[serde(rename = "token-counting-2024-11-01")]
    TokenCounting20241101,
    #[serde(rename = "token-efficient-tools-2025-02-19")]
    TokenEfficientTools20250219,
    #[serde(rename = "output-128k-2025-02-19")]
    Output128k20250219,
    #[serde(rename = "files-api-2025-04-14")]
    FilesApi20250414,
    #[serde(rename = "mcp-client-2025-04-04")]
    McpClient20250404,
    #[serde(rename = "mcp-client-2025-11-20")]
    McpClient20251120,
    #[serde(rename = "dev-full-thinking-2025-05-14")]
    DevFullThinking20250514,
    #[serde(rename = "interleaved-thinking-2025-05-14")]
    InterleavedThinking20250514,
    #[serde(rename = "code-execution-2025-05-22")]
    CodeExecution20250522,
    #[serde(rename = "extended-cache-ttl-2025-04-11")]
    ExtendedCacheTtl20250411,
    #[serde(rename = "context-1m-2025-08-07")]
    Context1m20250807,
    #[serde(rename = "context-management-2025-06-27")]
    ContextManagement20250627,
    #[serde(rename = "model-context-window-exceeded-2025-08-26")]
    ModelContextWindowExceeded20250826,
    #[serde(rename = "skills-2025-10-02")]
    Skills20251002,
    #[serde(rename = "structured-outputs-2025-11-13")]
    StructuredOutputs20251113,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicBeta {
    Known(AnthropicBetaKnown),
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicBetaHeader {
    Single(AnthropicBeta),
    Multiple(Vec<AnthropicBeta>),
}
