use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_valid::json::FromJsonSlice;
use tracing::warn;
use url::Url;

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
use crate::providers::auth::{ApiFormat, ensure_public_auth, format_from_request};
use crate::providers::credential_status::{
    CredentialStatus, ProviderKind, status_from_error, status_from_response,
};
#[cfg(feature = "provider-codex")]
use crate::providers::codex::{cooldown_until_from_usage, fetch_codex_usage_by_account};
use crate::providers::endpoints::{
    ClaudeFiles, ClaudeSkills, DownstreamRequest, GeminiVersion, ProviderEndpoints,
    gemini_version_path,
};

fn strip_downstream_auth(
    mut headers: HeaderMap,
    mut query: HashMap<String, String>,
) -> (HeaderMap, HashMap<String, String>) {
    headers.remove(header::AUTHORIZATION);
    headers.remove("x-api-key");
    headers.remove("x-goog-api-key");
    query.remove("key");
    (headers, query)
}

pub fn provider_router<T: ProviderEndpoints>() -> Router {
    provider_router_base::<T>()
}

pub fn provider_router_with_responses<T: ProviderEndpoints>() -> Router {
    provider_router_base::<T>()
        .route("/v1/conversations", post(openai_conversations_create::<T>))
        .route(
            "/v1/conversations/{conversation_id}",
            get(openai_conversations_retrieve::<T>)
                .post(openai_conversations_update::<T>)
                .delete(openai_conversations_delete::<T>),
        )
        .route(
            "/v1/conversations/{conversation_id}/items",
            get(openai_conversation_items_list::<T>)
                .post(openai_conversation_items_create::<T>),
        )
        .route(
            "/v1/conversations/{conversation_id}/items/{item_id}",
            get(openai_conversation_items_retrieve::<T>)
                .delete(openai_conversation_items_delete::<T>),
        )
        .route("/v1/responses/compact", post(openai_responses_compact::<T>))
        .route(
            "/v1/responses/{response_id}",
            get(openai_responses_retrieve::<T>).delete(openai_responses_delete::<T>),
        )
        .route(
            "/v1/responses/{response_id}/cancel",
            post(openai_responses_cancel::<T>),
        )
        .route(
            "/v1/responses/{response_id}/input_items",
            get(openai_responses_input_items_list::<T>),
        )
}

fn provider_router_base<T: ProviderEndpoints>() -> Router {
    Router::new()
        .route("/v1/chat/completions", post(openai_chat_completions::<T>))
        .route("/v1/responses", post(openai_responses::<T>))
        .route(
            "/v1/responses/input_tokens",
            post(openai_responses_input_tokens::<T>),
        )
        .route("/v1/messages", post(claude_messages::<T>))
        .route(
            "/v1/messages/count_tokens",
            post(claude_messages_count_tokens::<T>),
        )
        .route("/v1beta/models", get(gemini_models_list_v1beta::<T>))
        .route(
            "/v1beta/models/{name}",
            get(gemini_model_get_v1beta::<T>).post(gemini_post_v1beta::<T>),
        )
        .route("/v1/models", get(common_models_list::<T>))
        .route(
            "/v1/models/{model}",
            get(common_model_get::<T>).post(gemini_post_v1::<T>),
        )
}

pub fn claude_router<T: ProviderEndpoints + ClaudeFiles + ClaudeSkills>() -> Router {
    provider_router::<T>()
        .route(
            "/v1/files",
            post(claude_files_upload::<T>).get(claude_files_list::<T>),
        )
        .route(
            "/v1/files/{file_id}",
            get(claude_files_get::<T>).delete(claude_files_delete::<T>),
        )
        .route(
            "/v1/files/{file_id}/content",
            get(claude_files_download::<T>),
        )
        .route(
            "/v1/skills",
            post(claude_skills_create::<T>).get(claude_skills_list::<T>),
        )
        .route(
            "/v1/skills/{skill_id}",
            get(claude_skill_get::<T>).delete(claude_skill_delete::<T>),
        )
        .route(
            "/v1/skills/{skill_id}/versions",
            post(claude_skill_versions_create::<T>)
                .get(claude_skill_versions_list::<T>),
        )
        .route(
            "/v1/skills/{skill_id}/versions/{version}",
            get(claude_skill_version_get::<T>)
                .delete(claude_skill_version_delete::<T>),
        )
}

async fn openai_chat_completions<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let req = CreateChatCompletionRequest::from_json_slice(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/chat/completions".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::openai_chat_completions(ctx.as_ref(), req).await
}

async fn openai_responses<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let req = CreateResponseRequest::from_json_slice(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/responses".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::openai_responses(ctx.as_ref(), req).await
}

async fn openai_responses_input_tokens<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let req = ResponseInputTokensRequest::from_json_slice(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/responses/input_tokens".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::openai_responses_input_tokens(ctx.as_ref(), req).await
}

async fn openai_responses_retrieve<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(response_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/responses/{response_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_responses_retrieve(ctx.as_ref(), req, response_id).await
}

async fn openai_responses_delete<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(response_id): Path<String>,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::DELETE,
        path: format!("/v1/responses/{response_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_responses_delete(ctx.as_ref(), req, response_id).await
}

async fn openai_responses_cancel<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(response_id): Path<String>,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: format!("/v1/responses/{response_id}/cancel"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_responses_cancel(ctx.as_ref(), req, response_id).await
}

async fn openai_responses_compact<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let req = CompactResponseRequest::from_json_slice(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/responses/compact".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::openai_responses_compact(ctx.as_ref(), req).await
}

async fn openai_responses_input_items_list<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(response_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/responses/{response_id}/input_items"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_responses_input_items_list(ctx.as_ref(), req, response_id)
        .await
}

async fn openai_conversations_create<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let req = CreateConversationRequest::from_json_slice(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/conversations".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::openai_conversations_create(ctx.as_ref(), req).await
}

async fn openai_conversations_retrieve<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(conversation_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/conversations/{conversation_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_conversations_retrieve(ctx.as_ref(), req, conversation_id).await
}

async fn openai_conversations_update<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let req = UpdateConversationRequest::from_json_slice(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: format!("/v1/conversations/{conversation_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::openai_conversations_update(ctx.as_ref(), req, conversation_id).await
}

async fn openai_conversations_delete<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::DELETE,
        path: format!("/v1/conversations/{conversation_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_conversations_delete(ctx.as_ref(), req, conversation_id).await
}

async fn openai_conversation_items_list<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(conversation_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/conversations/{conversation_id}/items"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_conversation_items_list(ctx.as_ref(), req, conversation_id).await
}

async fn openai_conversation_items_create<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(conversation_id): Path<String>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let req = CreateConversationItemsRequest::from_json_slice(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: format!("/v1/conversations/{conversation_id}/items"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::openai_conversation_items_create(ctx.as_ref(), req, conversation_id).await
}

async fn openai_conversation_items_retrieve<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path((conversation_id, item_id)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/conversations/{conversation_id}/items/{item_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_conversation_items_retrieve(ctx.as_ref(), req, conversation_id, item_id).await
}

async fn openai_conversation_items_delete<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path((conversation_id, item_id)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::DELETE,
        path: format!("/v1/conversations/{conversation_id}/items/{item_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::openai_conversation_items_delete(ctx.as_ref(), req, conversation_id, item_id)
        .await
}

async fn claude_messages<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let req = serde_json::from_slice::<MessageCreateRequest>(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/messages".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::claude_messages(ctx.as_ref(), req).await
}

async fn claude_messages_count_tokens<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let req = serde_json::from_slice::<ClaudeCountTokensRequest>(body.as_ref())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/messages/count_tokens".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: req,
    };
    T::claude_messages_count_tokens(ctx.as_ref(), req).await
}

async fn claude_skills_create<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    _body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/skills".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skills_create(ctx.as_ref(), req).await
}

async fn claude_skills_list<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: "/v1/skills".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skills_list(ctx.as_ref(), req).await
}

async fn claude_skill_get<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(skill_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/skills/{skill_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skill_get(ctx.as_ref(), req, skill_id).await
}

async fn claude_skill_delete<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::DELETE,
        path: format!("/v1/skills/{skill_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skill_delete(ctx.as_ref(), req, skill_id).await
}

async fn claude_skill_versions_create<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
    _body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: format!("/v1/skills/{skill_id}/versions"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skill_versions_create(ctx.as_ref(), req, skill_id).await
}

async fn claude_skill_versions_list<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(skill_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/skills/{skill_id}/versions"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skill_versions_list(ctx.as_ref(), req, skill_id).await
}

async fn claude_skill_version_get<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path((skill_id, version)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/skills/{skill_id}/versions/{version}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skill_version_get(ctx.as_ref(), req, skill_id, version)
        .await
}

async fn claude_skill_version_delete<T: ClaudeSkills>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path((skill_id, version)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::DELETE,
        path: format!("/v1/skills/{skill_id}/versions/{version}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_skill_version_delete(ctx.as_ref(), req, skill_id, version)
        .await
}

async fn claude_files_upload<T: ClaudeFiles>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::POST,
        path: "/v1/files".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: body.to_vec(),
    };
    T::claude_files_upload(ctx.as_ref(), req).await
}

async fn claude_files_list<T: ClaudeFiles>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: "/v1/files".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_files_list(ctx.as_ref(), req).await
}

async fn claude_files_get<T: ClaudeFiles>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(file_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/files/{file_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_files_get(ctx.as_ref(), req, file_id).await
}

async fn claude_files_download<T: ClaudeFiles>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(file_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/files/{file_id}/content"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_files_download(ctx.as_ref(), req, file_id).await
}

async fn claude_files_delete<T: ClaudeFiles>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(file_id): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::DELETE,
        path: format!("/v1/files/{file_id}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::claude_files_delete(ctx.as_ref(), req, file_id).await
}

async fn gemini_post_v1beta<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(path): Path<String>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    gemini_post::<T>(GeminiVersion::V1Beta, ctx, headers, query, path, body).await
}

async fn gemini_post_v1<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(path): Path<String>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    gemini_post::<T>(GeminiVersion::V1, ctx, headers, query, path, body).await
}

async fn gemini_post<T: ProviderEndpoints>(
    version: GeminiVersion,
    ctx: Arc<AppContext>,
    headers: HeaderMap,
    query: HashMap<String, String>,
    path: String,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Gemini, &headers, &query, &ctx)?;
    let Some((model, action)) = path.rsplit_once(':') else {
        return Err(StatusCode::NOT_FOUND);
    };
    if model.is_empty() || action.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let (headers, query) = strip_downstream_auth(headers, query);
    let downstream_path = format!("{}/models/{}", gemini_version_path(version, ""), path);
    match action {
        "generateContent" => {
            let req = GenerateContentRequest::from_json_slice(body.as_ref())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let req = DownstreamRequest {
                method: Method::POST,
                path: downstream_path,
                query,
                headers,
                caller_api_key: Some(caller_api_key.clone()),
                body: req,
            };
            T::gemini_generate_content(ctx.as_ref(), req, version, model.to_string()).await
        }
        "streamGenerateContent" => {
            let req = GenerateContentRequest::from_json_slice(body.as_ref())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let req = DownstreamRequest {
                method: Method::POST,
                path: downstream_path,
                query,
                headers,
                caller_api_key: Some(caller_api_key.clone()),
                body: req,
            };
            T::gemini_stream_generate_content(ctx.as_ref(), req, version, model.to_string()).await
        }
        "countTokens" => {
            let req = GeminiCountTokensRequest::from_json_slice(body.as_ref())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let req = DownstreamRequest {
                method: Method::POST,
                path: downstream_path,
                query,
                headers,
                caller_api_key: Some(caller_api_key),
                body: req,
            };
            T::gemini_count_tokens(ctx.as_ref(), req, version, model.to_string()).await
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn gemini_models_list_v1beta<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Gemini, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: "/v1beta/models".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::gemini_models_list(ctx.as_ref(), req, GeminiVersion::V1Beta).await
}

async fn gemini_model_get_v1beta<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(name): Path<String>,
) -> Result<Response, StatusCode> {
    let caller_api_key = ensure_public_auth(ApiFormat::Gemini, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1beta/models/{name}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    T::gemini_model_get(ctx.as_ref(), req, GeminiVersion::V1Beta, name).await
}

async fn common_models_list<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let format = format_from_request(&headers, &query)?;
    let caller_api_key = ensure_public_auth(format, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: "/v1/models".to_string(),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    match format {
        ApiFormat::OpenAI => T::openai_models_list(ctx.as_ref(), req).await,
        ApiFormat::Claude => T::claude_models_list(ctx.as_ref(), req).await,
        ApiFormat::Gemini => T::gemini_models_list(ctx.as_ref(), req, GeminiVersion::V1).await,
    }
}

async fn common_model_get<T: ProviderEndpoints>(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Path(model): Path<String>,
) -> Result<Response, StatusCode> {
    let format = format_from_request(&headers, &query)?;
    let caller_api_key = ensure_public_auth(format, &headers, &query, &ctx)?;
    let (headers, query) = strip_downstream_auth(headers, query);
    let req = DownstreamRequest {
        method: Method::GET,
        path: format!("/v1/models/{model}"),
        query,
        headers,
        caller_api_key: Some(caller_api_key),
        body: (),
    };
    match format {
        ApiFormat::OpenAI => T::openai_model_get(ctx.as_ref(), req, model).await,
        ApiFormat::Claude => T::claude_model_get(ctx.as_ref(), req, model).await,
        ApiFormat::Gemini => T::gemini_model_get(ctx.as_ref(), req, GeminiVersion::V1, model).await,
    }
}

pub(crate) fn filter_request_headers(headers: &HeaderMap) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for (name, value) in headers.iter() {
        if is_hop_header(name) || is_internal_request_header(name) {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    filtered
}

pub(crate) fn filter_response_headers(headers: &HeaderMap) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for (name, value) in headers.iter() {
        if is_hop_header(name) || name == header::CONTENT_LENGTH {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    filtered
}

mod debug_http {
    use std::env;

    use axum::http::{HeaderMap, StatusCode};
    use tracing::info;

    pub fn enabled() -> bool {
        matches!(
            env::var("GPROXY_DEBUG_HTTP")
                .as_deref()
                .map(|value| value.to_ascii_lowercase())
                .as_deref(),
            Ok("1") | Ok("true")
        )
    }

    pub fn format_headers(headers: &HeaderMap) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (name, value) in headers.iter() {
            let key = name.as_str();
            if matches!(key, "authorization" | "x-api-key" | "x-goog-api-key") {
                continue;
            }
            if let Ok(value) = value.to_str() {
                out.push((key.to_string(), value.to_string()));
            }
        }
        out
    }

    pub fn log_request(url: &str, headers: &HeaderMap) {
        if !enabled() {
            return;
        }
        info!(target: "http", "send_json_request url={} headers={:?}", url, format_headers(headers));
    }

    pub fn log_json_response(status: StatusCode, headers: &HeaderMap) {
        if !enabled() {
            return;
        }
        info!(
            target: "http",
            "parse_json_response status={} headers={:?}",
            status.as_u16(),
            format_headers(headers)
        );
    }

    pub fn log_sse_error(status: StatusCode, headers: &HeaderMap) {
        if !enabled() {
            return;
        }
        info!(
            target: "http",
            "parse_sse_response status={} headers={:?}",
            status.as_u16(),
            format_headers(headers)
        );
    }

    pub fn log_sse_event(event: &str) {
        if !enabled() {
            return;
        }
        info!(target: "http", "parse_sse_event len={}", event.len());
    }
}

pub(crate) fn take_next_sse_event(buffer: &mut String) -> Option<String> {
    buffer.find("\n\n").map(|idx| {
        let event = buffer[..idx].to_string();
        buffer.replace_range(..idx + 2, "");
        event
    })
}

pub(crate) enum ParsedBody<T> {
    Ok(T),
    Error(serde_json::Value),
}

pub(crate) struct ParsedJsonResponse<T> {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    pub(crate) body: ParsedBody<T>,
}

pub(crate) enum SseMessage<T> {
    Data(T),
    Done,
}

pub(crate) enum ParsedSseBody<T> {
    Stream(Pin<Box<dyn Stream<Item = Result<SseMessage<T>, StatusCode>> + Send + 'static>>),
    Error(serde_json::Value),
}

pub(crate) struct ParsedSseResponse<T> {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    pub(crate) body: ParsedSseBody<T>,
}

pub(crate) fn parse_sse_event<T>(event: &str) -> Result<Option<SseMessage<T>>, StatusCode>
where
    T: DeserializeOwned,
{
    let data = extract_sse_data(event);
    if data.is_empty() {
        return Ok(None);
    }
    if data.trim() == "[DONE]" {
        return Ok(Some(SseMessage::Done));
    }

    let parsed: T = serde_json::from_str(&data).map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Some(SseMessage::Data(parsed)))
}

pub(crate) fn render_sse_message<T>(message: SseMessage<T>) -> Result<String, StatusCode>
where
    T: Serialize,
{
    match message {
        SseMessage::Done => Ok("data: [DONE]\n\n".to_string()),
        SseMessage::Data(data) => {
            let serialized = serde_json::to_string(&data).map_err(|_| StatusCode::BAD_GATEWAY)?;
            Ok(format!("data: {serialized}\n\n"))
        }
    }
}

pub(crate) fn apply_query(url: &mut Url, query: &HashMap<String, String>) {
    if query.is_empty() {
        return;
    }
    let mut pairs = url.query_pairs_mut();
    for (key, value) in query {
        pairs.append_pair(key, value);
    }
}

pub(crate) fn build_url(base_url: &Url, path: &str) -> Result<Url, StatusCode> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Url::parse(trimmed).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
    }

    let mut base = base_url.clone();
    let base_path = base.path();
    let mut next_path = String::new();
    if base_path.ends_with('/') {
        next_path.push_str(base_path);
    } else {
        next_path.push_str(base_path);
        next_path.push('/');
    }
    next_path.push_str(trimmed);
    base.set_path(&next_path);
    Ok(base)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AuthMode {
    AuthorizationBearer,
    XApiKey,
    XGoogApiKey,
}

pub(crate) fn apply_auth_header(
    headers: &mut HeaderMap,
    mode: AuthMode,
    key: &str,
) -> Result<(), StatusCode> {
    match mode {
        AuthMode::AuthorizationBearer => {
            let auth_value = HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            headers.insert(header::AUTHORIZATION, auth_value);
        }
        AuthMode::XApiKey => {
            let auth_value =
                HeaderValue::from_str(key).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            headers.insert("x-api-key", auth_value);
        }
        AuthMode::XGoogApiKey => {
            let auth_value =
                HeaderValue::from_str(key).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            headers.insert("x-goog-api-key", auth_value);
        }
    }
    Ok(())
}

pub(crate) fn not_implemented_response() -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_IMPLEMENTED;
    response
}

pub(crate) async fn send_get_request(
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
) -> Result<wreq::Response, StatusCode> {
    let mut out_headers = filter_request_headers(headers);
    apply_auth_header(&mut out_headers, auth_mode, auth_key)?;
    extra_headers(&mut out_headers)?;

    client
        .get(url)
        .headers(out_headers)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

pub(crate) async fn send_delete_request(
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
) -> Result<wreq::Response, StatusCode> {
    let mut out_headers = filter_request_headers(headers);
    apply_auth_header(&mut out_headers, auth_mode, auth_key)?;
    extra_headers(&mut out_headers)?;

    client
        .delete(url)
        .headers(out_headers)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn send_get_request_with_status(
    ctx: &AppContext,
    provider: ProviderKind,
    credential_id: &str,
    model: &str,
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
) -> Result<wreq::Response, StatusCode> {
    match send_get_request(
        client,
        url,
        headers,
        auth_mode,
        auth_key,
        extra_headers,
    )
    .await
    {
        Ok(res) => {
            let status = res.status();
            let resp_headers = res.headers().clone();
            let codex_until = if provider == ProviderKind::Codex
                && status == StatusCode::TOO_MANY_REQUESTS
            {
                codex_cooldown_until(ctx, credential_id).await
            } else {
                None
            };
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    if let Some(until) = codex_until {
                        return Some(CredentialStatus::Cooldown { until });
                    }
                    Some(status_from_response(prev, status, &resp_headers, now))
                })
                .await;
            Ok(res)
        }
        Err(status) => {
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    Some(status_from_error(prev, now))
                })
                .await;
            Err(status)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn send_delete_request_with_status(
    ctx: &AppContext,
    provider: ProviderKind,
    credential_id: &str,
    model: &str,
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
) -> Result<wreq::Response, StatusCode> {
    match send_delete_request(
        client,
        url,
        headers,
        auth_mode,
        auth_key,
        extra_headers,
    )
    .await
    {
        Ok(res) => {
            let status = res.status();
            let resp_headers = res.headers().clone();
            let codex_until = if provider == ProviderKind::Codex
                && status == StatusCode::TOO_MANY_REQUESTS
            {
                codex_cooldown_until(ctx, credential_id).await
            } else {
                None
            };
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    if let Some(until) = codex_until {
                        return Some(CredentialStatus::Cooldown { until });
                    }
                    Some(status_from_response(prev, status, &resp_headers, now))
                })
                .await;
            Ok(res)
        }
        Err(status) => {
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    Some(status_from_error(prev, now))
                })
                .await;
            Err(status)
        }
    }
}

pub(crate) async fn send_json_request<TReq>(
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
    body: &TReq,
) -> Result<wreq::Response, StatusCode>
where
    TReq: Serialize,
{
    let body = serde_json::to_vec(body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut out_headers = filter_request_headers(headers);
    apply_auth_header(&mut out_headers, auth_mode, auth_key)?;
    extra_headers(&mut out_headers)?;
    out_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    debug_http::log_request(url, &out_headers);

    client
        .post(url)
        .headers(out_headers)
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn send_json_request_with_timeout<TReq>(
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
    body: &TReq,
    timeout: Duration,
) -> Result<wreq::Response, StatusCode>
where
    TReq: Serialize,
{
    let body = serde_json::to_vec(body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut out_headers = filter_request_headers(headers);
    apply_auth_header(&mut out_headers, auth_mode, auth_key)?;
    extra_headers(&mut out_headers)?;
    out_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    debug_http::log_request(url, &out_headers);

    client
        .post(url)
        .headers(out_headers)
        .body(body)
        .timeout(timeout)
        .send()
        .await
        .map_err(|err| {
            warn!("upstream request failed url={} error={}", url, err);
            StatusCode::BAD_GATEWAY
        })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn send_json_request_with_status_timeout<TReq>(
    ctx: &AppContext,
    provider: ProviderKind,
    credential_id: &str,
    model: &str,
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
    body: &TReq,
    timeout: Duration,
) -> Result<wreq::Response, StatusCode>
where
    TReq: Serialize,
{
    match send_json_request_with_timeout(
        client,
        url,
        headers,
        auth_mode,
        auth_key,
        extra_headers,
        body,
        timeout,
    )
    .await
    {
        Ok(res) => {
            let status = res.status();
            let resp_headers = res.headers().clone();
            let codex_until = if provider == ProviderKind::Codex
                && status == StatusCode::TOO_MANY_REQUESTS
            {
                codex_cooldown_until(ctx, credential_id).await
            } else {
                None
            };
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    if let Some(until) = codex_until {
                        return Some(CredentialStatus::Cooldown { until });
                    }
                    Some(status_from_response(prev, status, &resp_headers, now))
                })
                .await;
            Ok(res)
        }
        Err(status) => {
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    Some(status_from_error(prev, now))
                })
                .await;
            Err(status)
        }
    }
}

pub(crate) async fn send_bytes_request(
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
    body: Vec<u8>,
) -> Result<wreq::Response, StatusCode> {
    let mut out_headers = filter_request_headers(headers);
    apply_auth_header(&mut out_headers, auth_mode, auth_key)?;
    extra_headers(&mut out_headers)?;
    debug_http::log_request(url, &out_headers);

    client
        .post(url)
        .headers(out_headers)
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn send_json_request_with_status<TReq>(
    ctx: &AppContext,
    provider: ProviderKind,
    credential_id: &str,
    model: &str,
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
    body: &TReq,
) -> Result<wreq::Response, StatusCode>
where
    TReq: Serialize,
{
    match send_json_request(client, url, headers, auth_mode, auth_key, extra_headers, body).await {
        Ok(res) => {
            let status = res.status();
            let resp_headers = res.headers().clone();
            let codex_until = if provider == ProviderKind::Codex
                && status == StatusCode::TOO_MANY_REQUESTS
            {
                codex_cooldown_until(ctx, credential_id).await
            } else {
                None
            };
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    if let Some(until) = codex_until {
                        return Some(CredentialStatus::Cooldown { until });
                    }
                    Some(status_from_response(prev, status, &resp_headers, now))
                })
                .await;
            Ok(res)
        }
        Err(status) => {
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    Some(status_from_error(prev, now))
                })
                .await;
            Err(status)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn send_bytes_request_with_status(
    ctx: &AppContext,
    provider: ProviderKind,
    credential_id: &str,
    model: &str,
    client: wreq::Client,
    url: &str,
    headers: &HeaderMap,
    auth_mode: AuthMode,
    auth_key: &str,
    extra_headers: impl FnOnce(&mut HeaderMap) -> Result<(), StatusCode>,
    body: Vec<u8>,
) -> Result<wreq::Response, StatusCode> {
    match send_bytes_request(
        client,
        url,
        headers,
        auth_mode,
        auth_key,
        extra_headers,
        body,
    )
    .await
    {
        Ok(res) => {
            let status = res.status();
            let resp_headers = res.headers().clone();
            let codex_until = if provider == ProviderKind::Codex
                && status == StatusCode::TOO_MANY_REQUESTS
            {
                codex_cooldown_until(ctx, credential_id).await
            } else {
                None
            };
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    if let Some(until) = codex_until {
                        return Some(CredentialStatus::Cooldown { until });
                    }
                    Some(status_from_response(prev, status, &resp_headers, now))
                })
                .await;
            Ok(res)
        }
        Err(status) => {
            let _ = ctx
                .update_credential_status_by_id(provider, credential_id, model, |prev, now| {
                    Some(status_from_error(prev, now))
                })
                .await;
            Err(status)
        }
    }
}

#[cfg(feature = "provider-codex")]
async fn codex_cooldown_until(ctx: &AppContext, account_id: &str) -> Option<i64> {
    let usage = fetch_codex_usage_by_account(ctx, account_id).await.ok()?;
    Some(cooldown_until_from_usage(&usage))
}

#[cfg(not(feature = "provider-codex"))]
async fn codex_cooldown_until(_ctx: &AppContext, _account_id: &str) -> Option<i64> {
    None
}

pub(crate) async fn parse_json_response<TResp>(
    res: wreq::Response,
) -> Result<ParsedJsonResponse<TResp>, StatusCode>
where
    TResp: DeserializeOwned,
{
    let status = res.status();
    let resp_headers = filter_response_headers(res.headers());
    let resp_body = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    debug_http::log_json_response(status, &resp_headers);

    let body = if status.is_success() {
        let parsed: TResp =
            serde_json::from_slice(&resp_body).map_err(|_| StatusCode::BAD_GATEWAY)?;
        ParsedBody::Ok(parsed)
    } else {
        let parsed: serde_json::Value =
            serde_json::from_slice(&resp_body).map_err(|_| StatusCode::BAD_GATEWAY)?;
        ParsedBody::Error(parsed)
    };

    Ok(ParsedJsonResponse {
        status,
        headers: resp_headers,
        body,
    })
}

pub(crate) fn render_json_response<TResp>(
    parsed: ParsedJsonResponse<TResp>,
) -> Result<Response, StatusCode>
where
    TResp: Serialize,
{
    let body = match parsed.body {
        ParsedBody::Ok(value) => serde_json::to_vec(&value).map_err(|_| StatusCode::BAD_GATEWAY)?,
        ParsedBody::Error(value) => {
            serde_json::to_vec(&value).map_err(|_| StatusCode::BAD_GATEWAY)?
        }
    };

    let mut response = Response::new(Body::from(body));
    *response.status_mut() = parsed.status;
    *response.headers_mut() = parsed.headers;
    Ok(response)
}

pub(crate) async fn render_bytes_response(res: wreq::Response) -> Result<Response, StatusCode> {
    let status = res.status();
    let resp_headers = filter_response_headers(res.headers());
    let body = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = resp_headers;
    Ok(response)
}

pub(crate) fn map_parsed_json<TIn, TOut, F>(
    parsed: ParsedJsonResponse<TIn>,
    map: F,
) -> Result<ParsedJsonResponse<TOut>, StatusCode>
where
    F: FnOnce(TIn) -> Result<TOut, StatusCode>,
{
    let ParsedJsonResponse {
        status,
        headers,
        body,
    } = parsed;
    let body = match body {
        ParsedBody::Ok(value) => ParsedBody::Ok(map(value)?),
        ParsedBody::Error(value) => ParsedBody::Error(value),
    };
    Ok(ParsedJsonResponse {
        status,
        headers,
        body,
    })
}

pub(crate) async fn parse_sse_response<TEvent>(
    mut res: wreq::Response,
) -> Result<ParsedSseResponse<TEvent>, StatusCode>
where
    TEvent: DeserializeOwned + Send + 'static,
{
    let status = res.status();
    let resp_headers = filter_response_headers(res.headers());

    if !status.is_success() {
        let resp_body = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
        let parsed: serde_json::Value =
            serde_json::from_slice(&resp_body).map_err(|_| StatusCode::BAD_GATEWAY)?;
        debug_http::log_sse_error(status, &resp_headers);
        return Ok(ParsedSseResponse {
            status,
            headers: resp_headers,
            body: ParsedSseBody::Error(parsed),
        });
    }

    let body_stream = stream! {
        let mut buffer = String::new();
        loop {
            let chunk = match res.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(_) => break,
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            if buffer.contains('\r') {
                buffer = buffer.replace("\r\n", "\n").replace('\r', "\n");
            }
            while let Some(event) = take_next_sse_event(&mut buffer) {
                debug_http::log_sse_event(&event);
                match parse_sse_event::<TEvent>(&event) {
                    Ok(Some(output)) => {
                        yield Ok::<SseMessage<TEvent>, StatusCode>(output);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        yield Err(StatusCode::BAD_GATEWAY);
                        return;
                    }
                }
            }
        }
    };

    Ok(ParsedSseResponse {
        status,
        headers: resp_headers,
        body: ParsedSseBody::Stream(Box::pin(body_stream)),
    })
}

pub(crate) fn render_sse_response<TEvent>(
    parsed: ParsedSseResponse<TEvent>,
) -> Result<Response, StatusCode>
where
    TEvent: Serialize + Send + 'static,
{
    match parsed.body {
        ParsedSseBody::Error(value) => {
            let body = serde_json::to_vec(&value).map_err(|_| StatusCode::BAD_GATEWAY)?;
            let mut response = Response::new(Body::from(body));
            *response.status_mut() = parsed.status;
            *response.headers_mut() = parsed.headers;
            Ok(response)
        }
        ParsedSseBody::Stream(mut stream) => {
            let body_stream = stream! {
                while let Some(item) = stream.next().await {
                    let message = match item {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    let output = match render_sse_message(message) {
                        Ok(output) => output,
                        Err(_) => return,
                    };
                    yield Ok::<Bytes, Infallible>(Bytes::from(output));
                }
            };
            let mut response = Response::new(Body::from_stream(body_stream));
            *response.status_mut() = parsed.status;
            *response.headers_mut() = parsed.headers;
            Ok(response)
        }
    }
}

pub(crate) fn map_parsed_sse<TIn, TOut, F>(
    parsed: ParsedSseResponse<TIn>,
    mut map: F,
) -> ParsedSseResponse<TOut>
where
    TIn: Send + 'static,
    TOut: Send + 'static,
    F: FnMut(TIn) -> Result<TOut, StatusCode> + Send + 'static,
{
    let ParsedSseResponse {
        status,
        headers,
        body,
    } = parsed;
    match body {
        ParsedSseBody::Error(value) => ParsedSseResponse {
            status,
            headers,
            body: ParsedSseBody::Error(value),
        },
        ParsedSseBody::Stream(stream) => {
            let mapped = stream.map(move |item| match item {
                Ok(SseMessage::Done) => Ok(SseMessage::Done),
                Ok(SseMessage::Data(data)) => map(data).map(SseMessage::Data),
                Err(status) => Err(status),
            });
            ParsedSseResponse {
                status,
                headers,
                body: ParsedSseBody::Stream(Box::pin(mapped)),
            }
        }
    }
}

fn extract_sse_data(event: &str) -> String {
    let mut data_lines = Vec::new();
    for line in event.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        }
    }
    data_lines.join("\n")
}

fn is_hop_header(name: &header::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn is_internal_request_header(name: &header::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "authorization" | "host" | "content-length" | "x-api-key" | "x-goog-api-key"
    )
}
