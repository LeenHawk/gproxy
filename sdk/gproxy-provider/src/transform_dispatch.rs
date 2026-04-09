use std::marker::PhantomData;
use std::sync::Arc;

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use serde::{Serialize, de::DeserializeOwned};

use crate::response::UpstreamError;

trait RequestDescriptor: Sized {
    type Body: DeserializeOwned + Serialize;

    fn from_body(body: Self::Body) -> Self;
    fn into_body(self) -> Self::Body;
}

impl RequestDescriptor
    for gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest
{
    type Body = gproxy_protocol::openai::create_chat_completions::request::RequestBody;

    fn from_body(body: Self::Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

impl RequestDescriptor
    for gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest
{
    type Body = gproxy_protocol::openai::create_response::request::RequestBody;

    fn from_body(body: Self::Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

impl RequestDescriptor
    for gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest
{
    type Body = gproxy_protocol::claude::create_message::request::RequestBody;

    fn from_body(body: Self::Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

impl RequestDescriptor
    for gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest
{
    type Body = gproxy_protocol::gemini::generate_content::request::RequestBody;

    fn from_body(body: Self::Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

impl RequestDescriptor for gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest {
    type Body = gproxy_protocol::gemini::stream_generate_content::request::RequestBody;

    fn from_body(body: Self::Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

/// Transform a request body from one (operation, protocol) to another.
///
/// This dispatches to the appropriate `TryFrom` implementation in `gproxy_protocol::transform`.
pub fn transform_request(
    src_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_operation: OperationFamily,
    dst_protocol: ProtocolKind,
    body: Vec<u8>,
) -> Result<Vec<u8>, UpstreamError> {
    if src_operation == dst_operation && src_protocol == dst_protocol {
        return Ok(body);
    }

    tracing::debug!(
        src_operation = %src_operation,
        src_protocol = %src_protocol,
        dst_operation = %dst_operation,
        dst_protocol = %dst_protocol,
        "transforming request"
    );
    let key = (src_operation, src_protocol, dst_operation, dst_protocol);

    match key {
        // =====================================================================
        // generate_content
        // =====================================================================

        // === Claude source → OpenAI targets ===
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            >(&body)
        }
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // === Claude source → Gemini targets ===
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
            >(&body)
        }

        // === OpenAI ChatCompletions source → Claude ===
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }

        // === OpenAI ChatCompletions source → Gemini ===
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
            >(&body)
        }

        // === OpenAI ChatCompletions source → OpenAI Response ===
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // === OpenAI Response source → Claude ===
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }

        // === OpenAI Response source → Gemini ===
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
            >(&body)
        }

        // === Gemini source → Claude ===
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }

        // === Gemini source → OpenAI ChatCompletions ===
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            >(&body)
        }

        // === Gemini source → OpenAI Response ===
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // =====================================================================
        // stream_generate_content (request transforms only)
        // =====================================================================

        // --- Claude source ---
        (OperationFamily::StreamGenerateContent, ProtocolKind::Claude, OperationFamily::StreamGenerateContent, ProtocolKind::Gemini) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::Claude, OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::Claude, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::Claude, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // --- Gemini source ---
        (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini, OperationFamily::StreamGenerateContent, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson, OperationFamily::StreamGenerateContent, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // --- OpenAI ChatCompletions source ---
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::StreamGenerateContent, ProtocolKind::Claude) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::StreamGenerateContent, ProtocolKind::Gemini) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // --- OpenAI Response source ---
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::StreamGenerateContent, ProtocolKind::Claude) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::StreamGenerateContent, ProtocolKind::Gemini) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            >(&body)
        }
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
                gproxy_protocol::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            >(&body)
        }

        // =====================================================================
        // count_tokens
        // =====================================================================

        // --- Claude source ---
        (OperationFamily::CountToken, ProtocolKind::Claude, OperationFamily::CountToken, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
            >(&body)
        }
        (OperationFamily::CountToken, ProtocolKind::Claude, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
            >(&body)
        }

        // --- OpenAI source ---
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
            >(&body)
        }
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
            >(&body)
        }

        // --- Gemini source ---
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
            >(&body)
        }
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
            >(&body)
        }

        // =====================================================================
        // model_get
        // =====================================================================

        // --- Claude source ---
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
            >(&body)
        }
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
            >(&body)
        }

        // --- OpenAI source ---
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
            >(&body)
        }
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
            >(&body)
        }

        // --- Gemini source ---
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
            >(&body)
        }
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
            >(&body)
        }

        // =====================================================================
        // model_list
        // =====================================================================

        // --- Claude source ---
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
            >(&body)
        }
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
            >(&body)
        }

        // --- OpenAI source ---
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
            >(&body)
        }
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
            >(&body)
        }

        // --- Gemini source ---
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
            >(&body)
        }
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
            >(&body)
        }

        // =====================================================================
        // embeddings
        // =====================================================================

        (OperationFamily::Embedding, ProtocolKind::OpenAi, OperationFamily::Embedding, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::embeddings::request::OpenAiEmbeddingsRequest,
                gproxy_protocol::gemini::embeddings::request::GeminiEmbedContentRequest,
            >(&body)
        }
        (OperationFamily::Embedding, ProtocolKind::Gemini, OperationFamily::Embedding, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::embeddings::request::GeminiEmbedContentRequest,
                gproxy_protocol::openai::embeddings::request::OpenAiEmbeddingsRequest,
            >(&body)
        }

        // =====================================================================
        // create_image
        // =====================================================================

        (OperationFamily::CreateImage, ProtocolKind::OpenAi, OperationFamily::GenerateContent, ProtocolKind::Gemini)
        | (OperationFamily::CreateImage, ProtocolKind::OpenAi, OperationFamily::StreamGenerateContent, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::create_image::request::OpenAiCreateImageRequest,
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
            >(&body)
        }

        (OperationFamily::CreateImage, ProtocolKind::OpenAi, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse)
        | (OperationFamily::CreateImage, ProtocolKind::OpenAi, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_json::<
                gproxy_protocol::openai::create_image::request::OpenAiCreateImageRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // =====================================================================
        // create_image_edit
        // =====================================================================

        (OperationFamily::CreateImageEdit, ProtocolKind::OpenAi, OperationFamily::GenerateContent, ProtocolKind::Gemini)
        | (OperationFamily::CreateImageEdit, ProtocolKind::OpenAi, OperationFamily::StreamGenerateContent, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::create_image_edit::request::OpenAiCreateImageEditRequest,
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
            >(&body)
        }

        (OperationFamily::CreateImageEdit, ProtocolKind::OpenAi, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse)
        | (OperationFamily::CreateImageEdit, ProtocolKind::OpenAi, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_json::<
                gproxy_protocol::openai::create_image_edit::request::OpenAiCreateImageEditRequest,
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
            >(&body)
        }

        // =====================================================================
        // compact
        // =====================================================================

        (OperationFamily::Compact, ProtocolKind::OpenAi, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::compact_response::request::OpenAiCompactRequest,
                gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest,
            >(&body)
        }
        (OperationFamily::Compact, ProtocolKind::OpenAi, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::compact_response::request::OpenAiCompactRequest,
                gproxy_protocol::gemini::generate_content::request::GeminiGenerateContentRequest,
            >(&body)
        }

        _ => Err(UpstreamError::Channel(format!(
            "no request transform for ({}, {}) -> ({}, {})",
            src_operation, src_protocol, dst_operation, dst_protocol
        ))),
    }
}

/// Transform a response body from upstream protocol back to client protocol.
///
/// When a client sends request in (src_op, src_proto) and upstream responds in
/// (dst_op, dst_proto), we need to convert the upstream response back to the
/// client's protocol. The key is looked up as (dst_op, dst_proto, src_op, src_proto)
/// because we're converting FROM the upstream format TO the client format.
pub fn transform_response(
    src_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_operation: OperationFamily,
    dst_protocol: ProtocolKind,
    body: Vec<u8>,
) -> Result<Vec<u8>, UpstreamError> {
    tracing::debug!(
        src_operation = %src_operation,
        src_protocol = %src_protocol,
        dst_operation = %dst_operation,
        dst_protocol = %dst_protocol,
        "transforming response"
    );
    // Response direction: upstream responded in (dst_op, dst_proto),
    // client expects (src_op, src_proto).
    let key = (dst_operation, dst_protocol, src_operation, src_protocol);

    match key {
        // =====================================================================
        // generate_content responses
        // =====================================================================

        // Gemini response → Claude
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
            >(&body)
        }
        // OpenAI ChatCompletions response → Claude
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
            >(&body)
        }
        // OpenAI Response response → Claude
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
            >(&body)
        }
        // OpenAI ChatCompletions response → Gemini
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
            >(&body)
        }
        // OpenAI Response response → Gemini
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
            >(&body)
        }

        // Claude response → OpenAI ChatCompletions
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            >(&body)
        }
        // Gemini response → OpenAI ChatCompletions
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            >(&body)
        }
        // OpenAI Response response → OpenAI ChatCompletions
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_openai_response_wrapper_to_chat_completions(&body)
        }

        // Claude response → OpenAI Response
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
            >(&body)
        }
        // Gemini response → OpenAI Response
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
            >(&body)
        }

        // =====================================================================
        // count_tokens responses
        // =====================================================================

        // Gemini response → Claude
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
            >(&body)
        }
        // OpenAI response → Claude
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
            >(&body)
        }

        // Claude response → OpenAI
        (OperationFamily::CountToken, ProtocolKind::Claude, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
            >(&body)
        }
        // Gemini response → OpenAI
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::CountToken, ProtocolKind::Claude, OperationFamily::CountToken, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
            >(&body)
        }
        // OpenAI response → Gemini
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
            >(&body)
        }

        // =====================================================================
        // model_get responses
        // =====================================================================

        // Gemini response → Claude
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
            >(&body)
        }
        // OpenAI response → Claude
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
            >(&body)
        }

        // Claude response → OpenAI
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
            >(&body)
        }
        // Gemini response → OpenAI
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
            >(&body)
        }
        // OpenAI response → Gemini
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
            >(&body)
        }

        // =====================================================================
        // model_list responses
        // =====================================================================

        // Gemini response → Claude
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
            >(&body)
        }
        // OpenAI response → Claude
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_json::<
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
            >(&body)
        }

        // Claude response → OpenAI
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
            >(&body)
        }
        // Gemini response → OpenAI
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
            >(&body)
        }
        // OpenAI response → Gemini
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
            >(&body)
        }

        // =====================================================================
        // embeddings responses
        // =====================================================================

        (OperationFamily::Embedding, ProtocolKind::Gemini, OperationFamily::Embedding, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::embeddings::response::GeminiEmbedContentResponse,
                gproxy_protocol::openai::embeddings::response::OpenAiEmbeddingsResponse,
            >(&body)
        }
        (OperationFamily::Embedding, ProtocolKind::OpenAi, OperationFamily::Embedding, ProtocolKind::Gemini) => {
            transform_json::<
                gproxy_protocol::openai::embeddings::response::OpenAiEmbeddingsResponse,
                gproxy_protocol::gemini::embeddings::response::GeminiEmbedContentResponse,
            >(&body)
        }

        // =====================================================================
        // create_image responses
        // =====================================================================

        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImage, ProtocolKind::OpenAi)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImage, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_image::response::OpenAiCreateImageResponse,
            >(&body)
        }

        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImage, ProtocolKind::OpenAi)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImage, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::openai::create_image::response::OpenAiCreateImageResponse,
            >(&body)
        }

        // =====================================================================
        // create_image_edit responses
        // =====================================================================

        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
            >(&body)
        }

        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
            >(&body)
        }

        // =====================================================================
        // compact responses
        // =====================================================================

        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::Compact, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::openai::compact_response::response::OpenAiCompactResponse,
            >(&body)
        }
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::Compact, ProtocolKind::OpenAi) => {
            transform_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::compact_response::response::OpenAiCompactResponse,
            >(&body)
        }

        _ => Err(UpstreamError::Channel(format!(
            "no response transform from upstream ({}, {}) to client ({}, {})",
            dst_operation, dst_protocol, src_operation, src_protocol
        ))),
    }
}

/// Generic JSON transform: deserialize as Src, TryFrom into Dst, serialize back.
fn transform_json<Src, Dst>(body: &[u8]) -> Result<Vec<u8>, UpstreamError>
where
    Src: serde::de::DeserializeOwned,
    Dst: TryFrom<Src> + serde::Serialize,
    Dst::Error: std::fmt::Display,
{
    let src: Src = serde_json::from_slice(body)
        .map_err(|e| UpstreamError::Channel(format!("request deserialize: {}", e)))?;

    let dst =
        Dst::try_from(src).map_err(|e| UpstreamError::Channel(format!("transform: {}", e)))?;

    serde_json::to_vec(&dst)
        .map_err(|e| UpstreamError::Channel(format!("response serialize: {}", e)))
}

fn transform_request_descriptor<Src, Dst>(body: &[u8]) -> Result<Vec<u8>, UpstreamError>
where
    Src: RequestDescriptor,
    Dst: RequestDescriptor + TryFrom<Src>,
    Dst::Error: std::fmt::Display,
{
    let src_body: Src::Body = serde_json::from_slice(body)
        .map_err(|e| UpstreamError::Channel(format!("request deserialize: {}", e)))?;
    let src = Src::from_body(src_body);
    let dst =
        Dst::try_from(src).map_err(|e| UpstreamError::Channel(format!("transform: {}", e)))?;

    serde_json::to_vec(&dst.into_body())
        .map_err(|e| UpstreamError::Channel(format!("response serialize: {}", e)))
}

fn transform_request_descriptor_ref<Src, Dst>(body: &[u8]) -> Result<Vec<u8>, UpstreamError>
where
    Src: RequestDescriptor,
    for<'a> Dst: RequestDescriptor + TryFrom<&'a Src>,
    for<'a> <Dst as TryFrom<&'a Src>>::Error: std::fmt::Display,
{
    let src_body: Src::Body = serde_json::from_slice(body)
        .map_err(|e| UpstreamError::Channel(format!("request deserialize: {}", e)))?;
    let src = Src::from_body(src_body);
    let dst =
        Dst::try_from(&src).map_err(|e| UpstreamError::Channel(format!("transform: {}", e)))?;

    serde_json::to_vec(&dst.into_body())
        .map_err(|e| UpstreamError::Channel(format!("response serialize: {}", e)))
}

fn transform_openai_response_wrapper_to_chat_completions(
    body: &[u8],
) -> Result<Vec<u8>, UpstreamError> {
    use gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse;
    use gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse;

    let wrapped: OpenAiCreateResponseResponse = serde_json::from_slice(body).map_err(|e| {
        UpstreamError::Channel(format!(
            "response deserialize: expected OpenAiCreateResponseResponse: {e}"
        ))
    })?;

    let converted = OpenAiChatCompletionsResponse::try_from(wrapped)
        .map_err(|e| UpstreamError::Channel(format!("transform: {}", e)))?;

    match converted {
        OpenAiChatCompletionsResponse::Success { body, .. } => serde_json::to_vec(&body)
            .map_err(|e| UpstreamError::Channel(format!("response serialize: {}", e))),
        OpenAiChatCompletionsResponse::Error { body, .. } => serde_json::to_vec(&body)
            .map_err(|e| UpstreamError::Channel(format!("response serialize: {}", e))),
    }
}

pub type StreamChunkNormalizer = Arc<dyn Fn(Vec<u8>) -> Vec<u8> + Send + Sync>;

pub struct StreamResponseTransformer {
    decoder: StreamChunkDecoder,
    inner: Box<dyn StreamChunkTransform>,
    normalizer: Option<StreamChunkNormalizer>,
}

impl StreamResponseTransformer {
    pub fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<u8>, UpstreamError> {
        let mut json_chunks = Vec::new();
        self.decoder.push_chunk(chunk, &mut json_chunks);
        self.process_json_chunks(json_chunks)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, UpstreamError> {
        let mut json_chunks = Vec::new();
        self.decoder.finish(&mut json_chunks);
        let mut out = self.process_json_chunks(json_chunks)?;
        self.inner.finish(&mut out)?;
        Ok(out)
    }

    fn process_json_chunks(&mut self, json_chunks: Vec<Vec<u8>>) -> Result<Vec<u8>, UpstreamError> {
        let mut out = Vec::new();
        for chunk in json_chunks {
            let chunk = if let Some(normalizer) = &self.normalizer {
                normalizer(chunk)
            } else {
                chunk
            };
            if chunk.is_empty() {
                continue;
            }
            self.inner.on_json_chunk(&chunk, &mut out)?;
        }
        Ok(out)
    }
}

trait StreamChunkTransform: Send {
    fn on_json_chunk(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> Result<(), UpstreamError>;
    fn finish(&mut self, out: &mut Vec<u8>) -> Result<(), UpstreamError>;
}

trait EventConverter<Input, Output>: Send {
    fn on_input(&mut self, input: Input, out: &mut Vec<Output>) -> Result<(), UpstreamError>;
    fn finish(&mut self, out: &mut Vec<Output>) -> Result<(), UpstreamError>;
}

struct TypedStreamTransform<Input, Output, Converter> {
    converter: Converter,
    encoder: StreamChunkEncoder,
    _marker: PhantomData<(Input, Output)>,
}

impl<Input, Output, Converter> StreamChunkTransform
    for TypedStreamTransform<Input, Output, Converter>
where
    Input: DeserializeOwned + Send + 'static,
    Output: Serialize + Send + 'static,
    Converter: EventConverter<Input, Output> + Send + 'static,
{
    fn on_json_chunk(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> Result<(), UpstreamError> {
        let input: Input = serde_json::from_slice(chunk)
            .map_err(|e| UpstreamError::Channel(format!("stream chunk deserialize failed: {e}")))?;
        let mut events = Vec::new();
        self.converter.on_input(input, &mut events)?;
        self.encoder.encode_events(&events, out)
    }

    fn finish(&mut self, out: &mut Vec<u8>) -> Result<(), UpstreamError> {
        let mut events = Vec::new();
        self.converter.finish(&mut events)?;
        self.encoder.encode_events(&events, out)?;
        self.encoder.finish(out);
        Ok(())
    }
}

enum StreamChunkDecoder {
    Sse(gproxy_protocol::stream::SseToNdjsonRewriter),
    Ndjson(Vec<u8>),
}

impl StreamChunkDecoder {
    fn from_protocol(protocol: ProtocolKind) -> Result<Self, UpstreamError> {
        match protocol {
            ProtocolKind::Claude
            | ProtocolKind::OpenAiChatCompletion
            | ProtocolKind::OpenAiResponse
            | ProtocolKind::Gemini => Ok(Self::Sse(
                gproxy_protocol::stream::SseToNdjsonRewriter::default(),
            )),
            ProtocolKind::GeminiNDJson => Ok(Self::Ndjson(Vec::new())),
            _ => Err(UpstreamError::Channel(format!(
                "unsupported stream input protocol: {protocol}"
            ))),
        }
    }

    fn push_chunk(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        match self {
            Self::Sse(rewriter) => {
                let converted = rewriter.push_chunk(chunk);
                split_json_lines(&converted, out);
            }
            Self::Ndjson(pending) => {
                pending.extend_from_slice(chunk);
                drain_json_lines(pending, out);
            }
        }
    }

    fn finish(&mut self, out: &mut Vec<Vec<u8>>) {
        match self {
            Self::Sse(rewriter) => {
                let converted = rewriter.finish();
                split_json_lines(&converted, out);
            }
            Self::Ndjson(pending) => {
                if pending.is_empty() {
                    return;
                }
                let mut line = std::mem::take(pending);
                if line.last().copied() == Some(b'\r') {
                    line.pop();
                }
                if !line.is_empty() {
                    out.push(line);
                }
            }
        }
    }
}

enum StreamChunkEncoder {
    Sse { append_done_marker: bool },
    Ndjson,
}

impl StreamChunkEncoder {
    fn from_protocol(protocol: ProtocolKind) -> Result<Self, UpstreamError> {
        match protocol {
            ProtocolKind::Claude | ProtocolKind::OpenAiResponse | ProtocolKind::Gemini => {
                Ok(Self::Sse {
                    append_done_marker: false,
                })
            }
            ProtocolKind::OpenAiChatCompletion => Ok(Self::Sse {
                append_done_marker: true,
            }),
            ProtocolKind::GeminiNDJson => Ok(Self::Ndjson),
            _ => Err(UpstreamError::Channel(format!(
                "unsupported stream output protocol: {protocol}"
            ))),
        }
    }

    fn encode_events<T: Serialize>(
        &self,
        events: &[T],
        out: &mut Vec<u8>,
    ) -> Result<(), UpstreamError> {
        for event in events {
            let json = serde_json::to_vec(event).map_err(|e| {
                UpstreamError::Channel(format!("stream chunk serialize failed: {e}"))
            })?;
            match self {
                Self::Sse { .. } => {
                    out.extend_from_slice(b"data: ");
                    out.extend_from_slice(&json);
                    out.extend_from_slice(b"\n\n");
                }
                Self::Ndjson => {
                    out.extend_from_slice(&json);
                    out.push(b'\n');
                }
            }
        }
        Ok(())
    }

    fn finish(&self, out: &mut Vec<u8>) {
        if let Self::Sse {
            append_done_marker: true,
        } = self
        {
            out.extend_from_slice(b"data: [DONE]\n\n");
        }
    }
}

use gproxy_protocol::stream::{drain_lines as drain_json_lines, split_lines as split_json_lines};

struct IdentityConverter<T>(PhantomData<T>);

impl<T> Default for IdentityConverter<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send> EventConverter<T, T> for IdentityConverter<T> {
    fn on_input(&mut self, input: T, out: &mut Vec<T>) -> Result<(), UpstreamError> {
        out.push(input);
        Ok(())
    }

    fn finish(&mut self, _out: &mut Vec<T>) -> Result<(), UpstreamError> {
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiChatToClaudeConverter(
    gproxy_protocol::transform::claude::stream_generate_content::openai_chat_completions::response::OpenAiChatCompletionsToClaudeStream,
);

impl
    EventConverter<
        gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
    > for OpenAiChatToClaudeConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        out: &mut Vec<gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct GeminiToClaudeConverter(
    gproxy_protocol::transform::claude::stream_generate_content::gemini::response::GeminiToClaudeStream,
);

impl
    EventConverter<
        gproxy_protocol::gemini::generate_content::response::ResponseBody,
        gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
    > for GeminiToClaudeConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiResponseToClaudeConverter(
    gproxy_protocol::transform::claude::stream_generate_content::openai_response::response::OpenAiResponseToClaudeStream,
);

impl
    EventConverter<
        gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
    > for OpenAiResponseToClaudeConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.on_stream_event(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct ClaudeToOpenAiChatConverter(
    gproxy_protocol::transform::openai::stream_generate_content::openai_chat_completions::claude::response::ClaudeToOpenAiChatCompletionsStream,
);

impl
    EventConverter<
        gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
        gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
    > for ClaudeToOpenAiChatConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
        out: &mut Vec<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        >,
    ) -> Result<(), UpstreamError> {
        self.0
            .on_event(input, out)
            .map_err(|e| UpstreamError::Channel(format!("stream transform failed: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        >,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct GeminiToOpenAiChatConverter(
    gproxy_protocol::transform::openai::stream_generate_content::openai_chat_completions::gemini::response::GeminiToOpenAiChatCompletionsStream,
);

impl
    EventConverter<
        gproxy_protocol::gemini::generate_content::response::ResponseBody,
        gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
    > for GeminiToOpenAiChatConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        >,
    ) -> Result<(), UpstreamError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        >,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct ClaudeToOpenAiResponseConverter(
    gproxy_protocol::transform::openai::stream_generate_content::openai_response::claude::response::ClaudeToOpenAiResponseStream,
);

impl
    EventConverter<
        gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
        gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
    > for ClaudeToOpenAiResponseConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
        out: &mut Vec<gproxy_protocol::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0
            .on_event(input, out)
            .map_err(|e| UpstreamError::Channel(format!("stream transform failed: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct GeminiToOpenAiResponseConverter(
    gproxy_protocol::transform::openai::stream_generate_content::openai_response::gemini::response::GeminiToOpenAiResponseStream,
);

impl
    EventConverter<
        gproxy_protocol::gemini::generate_content::response::ResponseBody,
        gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
    > for GeminiToOpenAiResponseConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<gproxy_protocol::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct ClaudeToGeminiConverter(
    gproxy_protocol::transform::gemini::stream_generate_content::claude::response::ClaudeToGeminiStream,
);

impl
    EventConverter<
        gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
        gproxy_protocol::gemini::generate_content::response::ResponseBody,
    > for ClaudeToGeminiConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
        out: &mut Vec<gproxy_protocol::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), UpstreamError> {
        self.0
            .on_event(input, out)
            .map_err(|e| UpstreamError::Channel(format!("stream transform failed: {e}")))
    }

    fn finish(
        &mut self,
        _out: &mut Vec<gproxy_protocol::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), UpstreamError> {
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiChatToGeminiConverter(
    gproxy_protocol::transform::gemini::stream_generate_content::openai_chat_completions::response::OpenAiChatCompletionsToGeminiStream,
);

impl
    EventConverter<
        gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        gproxy_protocol::gemini::generate_content::response::ResponseBody,
    > for OpenAiChatToGeminiConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        out: &mut Vec<gproxy_protocol::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), UpstreamError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiResponseToGeminiConverter(
    gproxy_protocol::transform::gemini::stream_generate_content::openai_response::response::OpenAiResponseToGeminiStream,
);

impl
    EventConverter<
        gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        gproxy_protocol::gemini::generate_content::response::ResponseBody,
    > for OpenAiResponseToGeminiConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<gproxy_protocol::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), UpstreamError> {
        self.0.on_stream_event(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct ResponseStreamToImageStreamConverter(
    gproxy_protocol::transform::openai::create_image::openai_response::stream::ResponseStreamToImageStream,
);

impl
    EventConverter<
        gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent,
    > for ResponseStreamToImageStreamConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.on_event(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct GeminiToImageStreamConverter {
    partial_count: u32,
}

impl
    EventConverter<
        gproxy_protocol::gemini::generate_content::response::ResponseBody,
        gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent,
    > for GeminiToImageStreamConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), UpstreamError> {
        use gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent;
        use gproxy_protocol::transform::openai::create_image::gemini::utils::{
            best_effort_openai_image_usage_from_gemini, gemini_inline_image_outputs_from_response,
        };

        let is_finished = input
            .candidates
            .as_ref()
            .and_then(|cs| cs.first())
            .and_then(|c| c.finish_reason.as_ref())
            .is_some();

        let images = gemini_inline_image_outputs_from_response(&input);
        let usage_metadata = input.usage_metadata.as_ref();

        for img in &images {
            if is_finished {
                out.push(ImageGenerationStreamEvent::Completed {
                    b64_json: img.b64_json.clone(),
                    background:
                        gproxy_protocol::openai::create_image::types::OpenAiImageBackground::Auto,
                    created_at: 0,
                    output_format: img.output_format.clone(),
                    quality: gproxy_protocol::openai::create_image::types::OpenAiImageQuality::Auto,
                    size: gproxy_protocol::openai::create_image::types::OpenAiImageSize::Auto,
                    usage: best_effort_openai_image_usage_from_gemini(usage_metadata),
                });
            } else {
                let index = self.partial_count;
                self.partial_count += 1;
                out.push(ImageGenerationStreamEvent::PartialImage {
                    b64_json: img.b64_json.clone(),
                    background:
                        gproxy_protocol::openai::create_image::types::OpenAiImageBackground::Auto,
                    created_at: 0,
                    output_format: img.output_format.clone(),
                    partial_image_index: index,
                    quality: gproxy_protocol::openai::create_image::types::OpenAiImageQuality::Auto,
                    size: gproxy_protocol::openai::create_image::types::OpenAiImageSize::Auto,
                });
            }
        }
        Ok(())
    }

    fn finish(
        &mut self,
        _out: &mut Vec<gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), UpstreamError> {
        Ok(())
    }
}

fn build_stream_transform<Input, Output, Converter>(
    src_protocol: ProtocolKind,
    dst_protocol: ProtocolKind,
    converter: Converter,
    normalizer: Option<StreamChunkNormalizer>,
) -> Result<StreamResponseTransformer, UpstreamError>
where
    Input: DeserializeOwned + Send + 'static,
    Output: Serialize + Send + 'static,
    Converter: EventConverter<Input, Output> + Send + 'static,
{
    Ok(StreamResponseTransformer {
        decoder: StreamChunkDecoder::from_protocol(dst_protocol)?,
        inner: Box::new(TypedStreamTransform::<Input, Output, Converter> {
            converter,
            encoder: StreamChunkEncoder::from_protocol(src_protocol)?,
            _marker: PhantomData,
        }),
        normalizer,
    })
}

pub fn create_stream_response_transformer(
    src_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_operation: OperationFamily,
    dst_protocol: ProtocolKind,
    normalizer: Option<StreamChunkNormalizer>,
) -> Result<StreamResponseTransformer, UpstreamError> {
    let key = (src_operation, src_protocol, dst_operation, dst_protocol);

    match key {
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => build_stream_transform::<
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            IdentityConverter<gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent>,
        >(
            src_protocol,
            dst_protocol,
            IdentityConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            IdentityConverter<
                gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            >,
        >(
            src_protocol,
            dst_protocol,
            IdentityConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            IdentityConverter<
                gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            >,
        >(
            src_protocol,
            dst_protocol,
            IdentityConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => build_stream_transform::<
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            IdentityConverter<gproxy_protocol::gemini::generate_content::response::ResponseBody>,
        >(
            src_protocol,
            dst_protocol,
            IdentityConverter::default(),
            normalizer,
        ),

        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            OpenAiChatToClaudeConverter,
        >(
            src_protocol,
            dst_protocol,
            OpenAiChatToClaudeConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            OpenAiResponseToClaudeConverter,
        >(
            src_protocol,
            dst_protocol,
            OpenAiResponseToClaudeConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => build_stream_transform::<
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            GeminiToClaudeConverter,
        >(
            src_protocol,
            dst_protocol,
            GeminiToClaudeConverter::default(),
            normalizer,
        ),

        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => build_stream_transform::<
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            ClaudeToOpenAiChatConverter,
        >(
            src_protocol,
            dst_protocol,
            ClaudeToOpenAiChatConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => build_stream_transform::<
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            GeminiToOpenAiChatConverter,
        >(
            src_protocol,
            dst_protocol,
            GeminiToOpenAiChatConverter::default(),
            normalizer,
        ),

        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => build_stream_transform::<
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            ClaudeToOpenAiResponseConverter,
        >(
            src_protocol,
            dst_protocol,
            ClaudeToOpenAiResponseConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => build_stream_transform::<
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            GeminiToOpenAiResponseConverter,
        >(
            src_protocol,
            dst_protocol,
            GeminiToOpenAiResponseConverter::default(),
            normalizer,
        ),

        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => build_stream_transform::<
            gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent,
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            ClaudeToGeminiConverter,
        >(
            src_protocol,
            dst_protocol,
            ClaudeToGeminiConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            OpenAiChatToGeminiConverter,
        >(
            src_protocol,
            dst_protocol,
            OpenAiChatToGeminiConverter::default(),
            normalizer,
        ),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            OpenAiResponseToGeminiConverter,
        >(
            src_protocol,
            dst_protocol,
            OpenAiResponseToGeminiConverter::default(),
            normalizer,
        ),

        // =====================================================================
        // stream_create_image / stream_create_image_edit → openai_response
        // =====================================================================
        (
            OperationFamily::StreamCreateImage,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        )
        | (
            OperationFamily::StreamCreateImageEdit,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent,
            ResponseStreamToImageStreamConverter,
        >(
            src_protocol,
            dst_protocol,
            ResponseStreamToImageStreamConverter::default(),
            normalizer,
        ),

        (
            OperationFamily::StreamCreateImage,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::StreamCreateImage,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        )
        | (
            OperationFamily::StreamCreateImageEdit,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::StreamCreateImageEdit,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => build_stream_transform::<
            gproxy_protocol::gemini::generate_content::response::ResponseBody,
            gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent,
            GeminiToImageStreamConverter,
        >(
            src_protocol,
            dst_protocol,
            GeminiToImageStreamConverter::default(),
            normalizer,
        ),

        _ => Err(UpstreamError::Channel(format!(
            "no stream response transform from upstream ({}, {}) to client ({}, {})",
            dst_operation, dst_protocol, src_operation, src_protocol
        ))),
    }
}

// =====================================================================
// Nonstream ↔ Stream conversions (same protocol, format change)
// =====================================================================

/// Convert a non-streaming response to stream events (same protocol).
/// Output is NDJSON (one JSON line per event) written into `out`.
pub fn nonstream_to_stream(
    protocol: ProtocolKind,
    body: &[u8],
    out: &mut Vec<u8>,
) -> Result<(), UpstreamError> {
    match protocol {
        ProtocolKind::Claude => {
            use gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse;
            use gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent;
            use gproxy_protocol::transform::claude::nonstream_to_stream::nonstream_to_stream;

            let response: ClaudeCreateMessageResponse = serde_json::from_slice(body)
                .map_err(|e| UpstreamError::Channel(format!("deserialize: {e}")))?;

            let mut events: Vec<ClaudeStreamEvent> = Vec::new();
            nonstream_to_stream(response, &mut events)
                .map_err(|e| UpstreamError::Channel(format!("nonstream_to_stream: {e}")))?;

            for event in &events {
                let json = serde_json::to_vec(event)
                    .map_err(|e| UpstreamError::Channel(format!("serialize event: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        ProtocolKind::OpenAiChatCompletion => {
            use gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse;
            use gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk;

            let response: OpenAiChatCompletionsResponse = serde_json::from_slice(body)
                .map_err(|e| UpstreamError::Channel(format!("deserialize: {e}")))?;

            let chunks = Vec::<ChatCompletionChunk>::try_from(response)
                .map_err(|e| UpstreamError::Channel(format!("nonstream_to_stream: {e}")))?;

            for chunk in &chunks {
                let json = serde_json::to_vec(chunk)
                    .map_err(|e| UpstreamError::Channel(format!("serialize chunk: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        ProtocolKind::OpenAiResponse => {
            use gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse;
            use gproxy_protocol::openai::create_response::stream::ResponseStreamEvent;

            let response: OpenAiCreateResponseResponse = serde_json::from_slice(body)
                .map_err(|e| UpstreamError::Channel(format!("deserialize: {e}")))?;

            let events = Vec::<ResponseStreamEvent>::try_from(response)
                .map_err(|e| UpstreamError::Channel(format!("nonstream_to_stream: {e}")))?;

            for event in &events {
                let json = serde_json::to_vec(event)
                    .map_err(|e| UpstreamError::Channel(format!("serialize event: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        ProtocolKind::Gemini => {
            use gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse;

            let response: GeminiGenerateContentResponse = serde_json::from_slice(body)
                .map_err(|e| UpstreamError::Channel(format!("deserialize: {e}")))?;

            // Gemini non-stream and stream share the same chunk body shape
            if let GeminiGenerateContentResponse::Success { body: resp, .. } = response {
                let json = serde_json::to_vec(&resp)
                    .map_err(|e| UpstreamError::Channel(format!("serialize chunk: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        _ => Err(UpstreamError::Channel(format!(
            "no nonstream_to_stream for protocol: {protocol}"
        ))),
    }
}

/// Convert stream events (NDJSON lines) to a non-streaming response (same protocol).
pub fn stream_to_nonstream(
    protocol: ProtocolKind,
    chunks: &[&[u8]],
) -> Result<Vec<u8>, UpstreamError> {
    match protocol {
        ProtocolKind::Claude => {
            use gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse;
            use gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent;

            let events: Vec<ClaudeStreamEvent> = chunks
                .iter()
                .map(|c| serde_json::from_slice(c))
                .collect::<Result<_, _>>()
                .map_err(|e| UpstreamError::Channel(format!("deserialize events: {e}")))?;

            let response = ClaudeCreateMessageResponse::try_from(events)
                .map_err(|e| UpstreamError::Channel(format!("stream_to_nonstream: {e}")))?;

            serde_json::to_vec(&response)
                .map_err(|e| UpstreamError::Channel(format!("serialize: {e}")))
        }
        ProtocolKind::OpenAiChatCompletion => {
            use gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse;
            use gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk;

            let chunks_parsed: Vec<ChatCompletionChunk> = chunks
                .iter()
                .map(|c| serde_json::from_slice(c))
                .collect::<Result<_, _>>()
                .map_err(|e| UpstreamError::Channel(format!("deserialize chunks: {e}")))?;

            let response = OpenAiChatCompletionsResponse::try_from(chunks_parsed)
                .map_err(|e| UpstreamError::Channel(format!("stream_to_nonstream: {e}")))?;

            serde_json::to_vec(&response)
                .map_err(|e| UpstreamError::Channel(format!("serialize: {e}")))
        }
        ProtocolKind::OpenAiResponse => {
            use gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse;
            use gproxy_protocol::openai::create_response::stream::ResponseStreamEvent;

            let events: Vec<ResponseStreamEvent> = chunks
                .iter()
                .map(|c| serde_json::from_slice(c))
                .collect::<Result<_, _>>()
                .map_err(|e| UpstreamError::Channel(format!("deserialize events: {e}")))?;

            let response = OpenAiCreateResponseResponse::try_from(events)
                .map_err(|e| UpstreamError::Channel(format!("stream_to_nonstream: {e}")))?;

            serde_json::to_vec(&response)
                .map_err(|e| UpstreamError::Channel(format!("serialize: {e}")))
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            use gproxy_protocol::gemini::generate_content::response::ResponseBody;
            use gproxy_protocol::gemini::generate_content::types::GeminiCandidate;
            use std::collections::BTreeMap;

            let mut merged = ResponseBody::default();
            let mut candidate_map: BTreeMap<u32, GeminiCandidate> = BTreeMap::new();

            for chunk in chunks {
                let body: ResponseBody = serde_json::from_slice(chunk)
                    .map_err(|e| UpstreamError::Channel(format!("deserialize chunk: {e}")))?;
                gproxy_protocol::transform::gemini::stream_to_nonstream::merge_chunk(
                    &mut merged,
                    &mut candidate_map,
                    body,
                );
            }

            let body = gproxy_protocol::transform::gemini::stream_to_nonstream::finalize_body(
                merged,
                candidate_map,
            );

            serde_json::to_vec(&body).map_err(|e| UpstreamError::Channel(format!("serialize: {e}")))
        }
        _ => Err(UpstreamError::Channel(format!(
            "no stream_to_nonstream for protocol: {protocol}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
    use serde_json::{Value, json};

    use super::{transform_openai_response_wrapper_to_chat_completions, transform_request};

    #[test]
    fn transform_request_supports_openai_chat_to_openai_response() {
        let body = br#"{
          "model": "gpt-5.4",
          "messages": [
            { "role": "user", "content": "reply ok" }
          ],
          "stream": false
        }"#
        .to_vec();

        let transformed = transform_request(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            body,
        )
        .expect("chat -> response request transform should succeed");

        let json: Value = serde_json::from_slice(&transformed).expect("transformed json");
        assert_eq!(json.get("model").and_then(Value::as_str), Some("gpt-5.4"));
        assert!(json.get("input").is_some());
    }

    #[test]
    fn transform_response_rejects_bare_openai_response_body_for_openai_chat() {
        let body = serde_json::to_vec(&json!({
          "id": "resp_123",
          "created_at": 1,
          "metadata": {},
          "model": "gpt-5.4",
          "object": "response",
          "output": [
            {
              "id": "msg_0",
              "content": [
                {
                  "annotations": [],
                  "text": "OK",
                  "type": "output_text"
                }
              ],
              "role": "assistant",
              "status": "completed",
              "type": "message"
            }
          ],
          "parallel_tool_calls": false,
          "temperature": 1.0,
          "tool_choice": "auto",
          "tools": [],
          "top_p": 1.0
        }))
        .expect("serialize response body");

        let err = transform_openai_response_wrapper_to_chat_completions(&body)
            .expect_err("bare responses body should be rejected");
        assert!(err.to_string().contains("OpenAiCreateResponseResponse"));
    }

    #[test]
    fn transform_response_supports_wrapped_openai_response_to_openai_chat() {
        let body = serde_json::to_vec(&json!({
          "stats_code": 200,
          "headers": {},
          "body": {
            "id": "resp_123",
            "created_at": 1,
            "metadata": {},
            "model": "gpt-5.4",
            "object": "response",
            "output": [
              {
                "id": "msg_0",
                "content": [
                  {
                    "annotations": [],
                    "text": "OK",
                    "type": "output_text"
                  }
                ],
                "role": "assistant",
                "status": "completed",
                "type": "message"
              }
            ],
            "parallel_tool_calls": false,
            "temperature": 1.0,
            "tool_choice": "auto",
            "tools": [],
            "top_p": 1.0
          }
        }))
        .expect("serialize wrapped response body");

        let transformed = transform_openai_response_wrapper_to_chat_completions(&body)
            .expect("wrapped response -> chat transform should succeed");

        let json: Value = serde_json::from_slice(&transformed).expect("transformed json");
        assert_eq!(json.get("model").and_then(Value::as_str), Some("gpt-5.4"));
        assert_eq!(
            json.pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            Some("OK")
        );
    }
}
