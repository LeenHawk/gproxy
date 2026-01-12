pub mod admin;
#[cfg(feature = "provider-antigravity")]
pub mod antigravity;
#[cfg(feature = "provider-aistudio")]
pub mod aistudio;
pub mod auth;
#[cfg(feature = "provider-claude")]
pub mod claude;
#[cfg(feature = "provider-claudecode")]
pub mod claudecode;
#[cfg(feature = "provider-codex")]
pub mod codex;
pub mod credential_index;
pub mod credential_status;
#[cfg(feature = "provider-deepseek")]
pub mod deepseek;
pub mod endpoints;
#[cfg(feature = "provider-geminicli")]
pub mod geminicli;
pub(crate) mod common;
pub(crate) mod google_oauth;
pub(crate) mod google_project;
#[cfg(feature = "provider-nvidia")]
pub mod nvidia;
#[cfg(feature = "provider-openai")]
pub mod openai;
pub mod router;
pub mod usage;
#[cfg(feature = "provider-vertex")]
pub mod vertex;
#[cfg(feature = "provider-vertexexpress")]
pub mod vertexexpress;

// Macros in this module:
// - provider_storage_modules!: declares backend modules behind storage-* features.
// - provider_storage_backend!: implements Backend for StorageService via storage_match!.
// - impl_openai_stub!: generates OpenAI trait impls that return not_implemented_response.
// - impl_claude_stub!: generates Claude trait impls that return not_implemented_response.
// - impl_gemini_stub!: generates Gemini trait impls that return not_implemented_response.
// - impl_google_access_token!: generates an access-token refresh helper for Google OAuth credentials.

#[macro_export]
macro_rules! provider_storage_modules {
    () => {
        #[cfg(feature = "storage-db")]
        pub(crate) mod database;
        #[cfg(feature = "storage-file")]
        mod file;
        #[cfg(feature = "storage-memory")]
        mod memory;
        #[cfg(feature = "storage-s3")]
        mod s3;
    };
}

#[macro_export]
macro_rules! provider_storage_backend {
    ($backend_trait:ident, $setting:ty, $credential:ty) => {
        #[async_trait::async_trait]
        impl $backend_trait for $crate::storage::StorageService {
            async fn get_config(&self) -> Result<$setting> {
                $crate::storage_match!(self, get_config())
            }

            async fn load_config(&self) -> Result<$setting> {
                $crate::storage_match!(self, load_config())
            }

            async fn update_config<F>(&self, update: F) -> Result<()>
            where
                F: FnOnce(&mut $setting) + Send,
            {
                $crate::storage_match!(self, update_config(update))
            }

            async fn get_credentials(&self) -> Result<Vec<$credential>> {
                $crate::storage_match!(self, get_credentials())
            }

            async fn load_credentials(&self) -> Result<Vec<$credential>> {
                $crate::storage_match!(self, load_credentials())
            }

            async fn add_credential(&self, credential: $credential) -> Result<()> {
                $crate::storage_match!(self, add_credential(credential))
            }

            async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
            where
                F: FnOnce(&mut $credential) + Send,
            {
                $crate::storage_match!(self, update_credential(index, update))
            }

            async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
            where
                F: FnOnce(&mut $credential) + Send,
            {
                $crate::storage_match!(self, update_credential_by_id(id, update))
            }

            async fn delete_credential(&self, key: &str) -> Result<()> {
                $crate::storage_match!(self, delete_credential(key))
            }

            async fn get_credential(&self, index: usize) -> Result<Option<$credential>> {
                $crate::storage_match!(self, get_credential(index))
            }
        }
    };
}

#[macro_export]
macro_rules! impl_openai_stub {
    ($provider:ty) => {
        // Expands into OpenAI* trait impls that always return not_implemented_response.
        #[async_trait::async_trait]
        impl $crate::providers::endpoints::OpenAIChatCompletions for $provider {
            async fn openai_chat_completions(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::openai::chat_completions::CreateChatCompletionRequest,
                >,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::OpenAIResponses for $provider {
            async fn openai_responses(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::openai::responses::CreateResponseRequest,
                >,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_responses_retrieve(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _response_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_responses_delete(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _response_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_responses_cancel(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _response_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_responses_compact(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::openai::responses::CompactResponseRequest,
                >,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_responses_input_items_list(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _response_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::OpenAIResponsesInputTokens for $provider {
            async fn openai_responses_input_tokens(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest,
                >,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::OpenAIModelsList for $provider {
            async fn openai_models_list(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::OpenAIModelGet for $provider {
            async fn openai_model_get(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _model: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::OpenAIConversations for $provider {
            async fn openai_conversations_create(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::openai::conversations::CreateConversationRequest,
                >,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_conversations_retrieve(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _conversation_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_conversations_update(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::openai::conversations::UpdateConversationRequest,
                >,
                _conversation_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_conversations_delete(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _conversation_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::OpenAIConversationItems for $provider {
            async fn openai_conversation_items_list(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _conversation_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_conversation_items_create(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::openai::conversations::CreateConversationItemsRequest,
                >,
                _conversation_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_conversation_items_retrieve(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _conversation_id: String,
                _item_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }

            async fn openai_conversation_items_delete(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _conversation_id: String,
                _item_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_claude_stub {
    ($provider:ty) => {
        // Expands into Claude* trait impls that always return not_implemented_response.
        #[async_trait::async_trait]
        impl $crate::providers::endpoints::ClaudeMessages for $provider {
            async fn claude_messages(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::claude::messages::MessageCreateRequest,
                >,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::ClaudeMessagesCountTokens for $provider {
            async fn claude_messages_count_tokens(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::claude::count_tokens::CountTokensRequest,
                >,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::ClaudeModelsList for $provider {
            async fn claude_models_list(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::ClaudeModelGet for $provider {
            async fn claude_model_get(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _model_id: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_gemini_stub {
    ($provider:ty) => {
        // Expands into Gemini* trait impls that always return not_implemented_response.
        #[async_trait::async_trait]
        impl $crate::providers::endpoints::GeminiGenerateContent for $provider {
            async fn gemini_generate_content(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::gemini::generate_content::GenerateContentRequest,
                >,
                _version: $crate::providers::endpoints::GeminiVersion,
                _model: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::GeminiStreamGenerateContent for $provider {
            async fn gemini_stream_generate_content(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::gemini::generate_content::GenerateContentRequest,
                >,
                _version: $crate::providers::endpoints::GeminiVersion,
                _model: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::GeminiCountTokens for $provider {
            async fn gemini_count_tokens(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<
                    $crate::formats::gemini::count_tokens::CountTokensRequest,
                >,
                _version: $crate::providers::endpoints::GeminiVersion,
                _model: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::GeminiModelsList for $provider {
            async fn gemini_models_list(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _version: $crate::providers::endpoints::GeminiVersion,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }

        #[async_trait::async_trait]
        impl $crate::providers::endpoints::GeminiModelGet for $provider {
            async fn gemini_model_get(
                _ctx: &$crate::context::AppContext,
                _req: $crate::providers::endpoints::DownstreamRequest<()>,
                _version: $crate::providers::endpoints::GeminiVersion,
                _name: String,
            ) -> Result<axum::response::Response, axum::http::StatusCode> {
                Ok($crate::providers::router::not_implemented_response())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_google_access_token {
    ($credential:ty, $storage_fn:ident) => {
        // Expands into ensure_access_token(...) for Google OAuth credentials.
        // Requires fields: project_id, token, refresh_token, expiry, token_uri, client_id,
        // client_secret, scope.
        pub(super) async fn ensure_access_token(
            ctx: &$crate::context::AppContext,
            credential: &$credential,
        ) -> Result<String, axum::http::StatusCode> {
            let now = $crate::providers::credential_status::now_timestamp();
            if !credential.token.is_empty()
                && !$crate::providers::google_oauth::should_refresh(&credential.expiry, now)
            {
                return Ok(credential.token.clone());
            }
            if credential.refresh_token.trim().is_empty() {
                if credential.token.is_empty() {
                    return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                }
                return Ok(credential.token.clone());
            }

            let refreshed = $crate::providers::google_oauth::refresh_access_token(
                ctx,
                &credential.token_uri,
                &credential.client_id,
                &credential.client_secret,
                &credential.refresh_token,
            )
            .await?;
            let expires_at = refreshed.expires_in.map(|value| now + value);
            let expiry = $crate::providers::google_oauth::format_expiry(expires_at);
            let refresh_token = refreshed
                .refresh_token
                .unwrap_or_else(|| credential.refresh_token.clone());
            let scope = refreshed
                .scope
                .map($crate::providers::google_oauth::parse_scope)
                .unwrap_or_else(|| credential.scope.clone());

            let project_id = credential.project_id.clone();
            let access_token = refreshed.access_token.clone();
            let expiry_clone = expiry.clone();
            let refresh_clone = refresh_token.clone();
            let scope_clone = scope.clone();
            ctx.$storage_fn()
                .update_credential_by_id(&project_id, move |stored| {
                    stored.token = access_token.clone();
                    stored.refresh_token = refresh_clone.clone();
                    if !expiry_clone.is_empty() {
                        stored.expiry = expiry_clone.clone();
                    }
                    if !scope_clone.is_empty() {
                        stored.scope = scope_clone.clone();
                    }
                })
                .await
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(refreshed.access_token)
        }
    };
}

#[cfg(feature = "provider-aistudio")]
use crate::providers::aistudio::AIStudioProvider;
#[cfg(feature = "provider-antigravity")]
use crate::providers::antigravity::AntigravityProvider;
#[cfg(feature = "provider-claude")]
use crate::providers::claude::ClaudeProvider;
#[cfg(feature = "provider-claudecode")]
use crate::providers::claudecode::ClaudeCodeProvider;
#[cfg(feature = "provider-codex")]
use crate::providers::codex::CodexProvider;
#[cfg(feature = "provider-deepseek")]
use crate::providers::deepseek::DeepSeekProvider;
#[cfg(feature = "provider-geminicli")]
use crate::providers::geminicli::GeminiCliProvider;
#[cfg(feature = "provider-nvidia")]
use crate::providers::nvidia::NvidiaProvider;
#[cfg(feature = "provider-openai")]
use crate::providers::openai::OpenAIProvider;
#[cfg(feature = "provider-vertex")]
use crate::providers::vertex::VertexProvider;
#[cfg(feature = "provider-vertexexpress")]
use crate::providers::vertexexpress::VertexExpressProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ProvidersConfig {
    #[serde(default)]
    #[cfg(feature = "provider-openai")]
    pub openai: OpenAIProvider,
    #[serde(default)]
    #[cfg(feature = "provider-codex")]
    pub codex: CodexProvider,
    #[serde(default)]
    #[cfg(feature = "provider-claude")]
    pub claude: ClaudeProvider,
    #[serde(default)]
    #[cfg(feature = "provider-claudecode")]
    pub claudecode: ClaudeCodeProvider,
    #[serde(default)]
    #[cfg(feature = "provider-aistudio")]
    pub aistudio: AIStudioProvider,
    #[serde(default)]
    #[cfg(feature = "provider-vertex")]
    pub vertex: VertexProvider,
    #[serde(default)]
    #[cfg(feature = "provider-vertexexpress")]
    pub vertexexpress: VertexExpressProvider,
    #[serde(default)]
    #[cfg(feature = "provider-geminicli")]
    pub geminicli: GeminiCliProvider,
    #[serde(default)]
    #[cfg(feature = "provider-antigravity")]
    pub antigravity: AntigravityProvider,
    #[serde(default)]
    #[cfg(feature = "provider-nvidia")]
    pub nvidia: NvidiaProvider,
    #[serde(default)]
    #[cfg(feature = "provider-deepseek")]
    pub deepseek: DeepSeekProvider,
}
