use async_trait::async_trait;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::Response;
use std::collections::HashMap;

use crate::context::AppContext;
use crate::formats::claude::count_tokens::CountTokensRequest as ClaudeCountTokensRequest;
use crate::formats::claude::messages::MessageCreateRequest;
use crate::formats::gemini::count_tokens::CountTokensRequest as GeminiCountTokensRequest;
use crate::formats::gemini::generate_content::GenerateContentRequest;
use crate::formats::openai::chat_completions::CreateChatCompletionRequest;
use crate::formats::openai::conversations::{
    CreateConversationItemsRequest, CreateConversationRequest, UpdateConversationRequest,
};
use crate::formats::openai::responses::{CompactResponseRequest, CreateResponseRequest};
use crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest;

pub struct DownstreamRequest<T> {
    pub method: Method,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HeaderMap,
    pub caller_api_key: Option<String>,
    pub body: T,
}

pub struct UpstreamRequest<T> {
    pub method: Method,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HeaderMap,
    pub body: T,
}

#[derive(Clone, Copy, Debug)]
pub enum GeminiVersion {
    V1,
    V1Beta,
}

pub(crate) fn gemini_version_path(version: GeminiVersion, suffix: &str) -> String {
    let prefix = match version {
        GeminiVersion::V1 => "/v1",
        GeminiVersion::V1Beta => "/v1beta",
    };
    format!("{prefix}{suffix}")
}

#[async_trait]
pub trait OpenAIChatCompletions {
    async fn openai_chat_completions(
        ctx: &AppContext,
        req: DownstreamRequest<CreateChatCompletionRequest>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait OpenAIResponses {
    async fn openai_responses(
        ctx: &AppContext,
        req: DownstreamRequest<CreateResponseRequest>,
    ) -> Result<Response, StatusCode>;

    async fn openai_responses_retrieve(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        response_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_responses_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        response_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_responses_cancel(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        response_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_responses_compact(
        ctx: &AppContext,
        req: DownstreamRequest<CompactResponseRequest>,
    ) -> Result<Response, StatusCode>;

    async fn openai_responses_input_items_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        response_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait OpenAIResponsesInputTokens {
    async fn openai_responses_input_tokens(
        ctx: &AppContext,
        req: DownstreamRequest<ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait OpenAIConversations {
    async fn openai_conversations_create(
        ctx: &AppContext,
        req: DownstreamRequest<CreateConversationRequest>,
    ) -> Result<Response, StatusCode>;

    async fn openai_conversations_retrieve(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        conversation_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_conversations_update(
        ctx: &AppContext,
        req: DownstreamRequest<UpdateConversationRequest>,
        conversation_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_conversations_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        conversation_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait OpenAIConversationItems {
    async fn openai_conversation_items_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        conversation_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_conversation_items_create(
        ctx: &AppContext,
        req: DownstreamRequest<CreateConversationItemsRequest>,
        conversation_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_conversation_items_retrieve(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        conversation_id: String,
        item_id: String,
    ) -> Result<Response, StatusCode>;

    async fn openai_conversation_items_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        conversation_id: String,
        item_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait OpenAIModelsList {
    async fn openai_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait OpenAIModelGet {
    async fn openai_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeMessages {
    async fn claude_messages(
        ctx: &AppContext,
        req: DownstreamRequest<MessageCreateRequest>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeMessagesCountTokens {
    async fn claude_messages_count_tokens(
        ctx: &AppContext,
        req: DownstreamRequest<ClaudeCountTokensRequest>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeModelsList {
    async fn claude_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeModelGet {
    async fn claude_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillsCreate {
    async fn claude_skills_create(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillsList {
    async fn claude_skills_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillGet {
    async fn claude_skill_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillDelete {
    async fn claude_skill_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillVersionsCreate {
    async fn claude_skill_versions_create(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillVersionsList {
    async fn claude_skill_versions_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillVersionGet {
    async fn claude_skill_version_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
        version: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait ClaudeSkillVersionDelete {
    async fn claude_skill_version_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        skill_id: String,
        version: String,
    ) -> Result<Response, StatusCode>;
}

pub trait ClaudeSkills:
    ClaudeSkillsCreate
    + ClaudeSkillsList
    + ClaudeSkillGet
    + ClaudeSkillDelete
    + ClaudeSkillVersionsCreate
    + ClaudeSkillVersionsList
    + ClaudeSkillVersionGet
    + ClaudeSkillVersionDelete
{
}

impl<T> ClaudeSkills for T where
    T: ClaudeSkillsCreate
        + ClaudeSkillsList
        + ClaudeSkillGet
        + ClaudeSkillDelete
        + ClaudeSkillVersionsCreate
        + ClaudeSkillVersionsList
        + ClaudeSkillVersionGet
        + ClaudeSkillVersionDelete
{
}

#[async_trait]
pub trait ClaudeFiles {
    async fn claude_files_upload(
        ctx: &AppContext,
        req: DownstreamRequest<Vec<u8>>,
    ) -> Result<Response, StatusCode>;

    async fn claude_files_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode>;

    async fn claude_files_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        file_id: String,
    ) -> Result<Response, StatusCode>;

    async fn claude_files_download(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        file_id: String,
    ) -> Result<Response, StatusCode>;

    async fn claude_files_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        file_id: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait GeminiGenerateContent {
    async fn gemini_generate_content(
        ctx: &AppContext,
        req: DownstreamRequest<GenerateContentRequest>,
        version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait GeminiStreamGenerateContent {
    async fn gemini_stream_generate_content(
        ctx: &AppContext,
        req: DownstreamRequest<GenerateContentRequest>,
        version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait GeminiCountTokens {
    async fn gemini_count_tokens(
        ctx: &AppContext,
        req: DownstreamRequest<GeminiCountTokensRequest>,
        version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait GeminiModelsList {
    async fn gemini_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
    ) -> Result<Response, StatusCode>;
}

#[async_trait]
pub trait GeminiModelGet {
    async fn gemini_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
        name: String,
    ) -> Result<Response, StatusCode>;
}

pub trait ProviderEndpoints:
    OpenAIChatCompletions
    + OpenAIResponses
    + OpenAIResponsesInputTokens
    + OpenAIConversations
    + OpenAIConversationItems
    + OpenAIModelsList
    + OpenAIModelGet
    + ClaudeMessages
    + ClaudeMessagesCountTokens
    + ClaudeModelsList
    + ClaudeModelGet
    + GeminiGenerateContent
    + GeminiStreamGenerateContent
    + GeminiCountTokens
    + GeminiModelsList
    + GeminiModelGet
    + Send
    + Sync
    + 'static
{
}
