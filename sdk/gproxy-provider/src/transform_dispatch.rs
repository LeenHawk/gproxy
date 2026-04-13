use std::marker::PhantomData;
use std::sync::Arc;

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use http::StatusCode;
use serde::{Serialize, de::DeserializeOwned};

use crate::response::UpstreamError;

/// Codec trait for response wrapper enums in `gproxy_protocol`.
///
/// Every response enum has the shape
/// `Success { stats_code, headers, body } | Error { stats_code, headers, body }`
/// where the outer envelope metadata is internal bookkeeping and the actual
/// HTTP body JSON is just the `body` field. `serde_json::from_slice::<Wrapper>`
/// cannot parse a raw upstream response because raw JSON has no `stats_code` /
/// `headers` top-level fields — it has the body fields directly.
///
/// This trait lets [`transform_json`] round-trip through a wrapper enum by
/// deserializing only the inner body JSON and wrapping it in the `Success`
/// variant (or falling back to `Error` if the body matches the error schema).
///
/// See the `impl_body_envelope!` macro below for implementations.
trait BodyEnvelope: Sized {
    /// Parse a raw HTTP response body into this wrapper's `Success` (or
    /// `Error`) variant with placeholder `stats_code` / `headers`.
    fn from_body_bytes(body: &[u8]) -> Result<Self, UpstreamError>;

    /// Serialize just the inner `body` field of this wrapper to JSON bytes
    /// for the client-facing HTTP response.
    fn into_body_bytes(self) -> Result<Vec<u8>, UpstreamError>;
}

/// Generate a [`BodyEnvelope`] impl for a protocol response wrapper enum.
///
/// The macro covers the uniform `Success { stats_code, headers, body } |
/// Error { stats_code, headers, body }` shape shared by every wrapper in
/// `gproxy_protocol::{claude, gemini, openai}::*::response`. The two `body`
/// field types differ per protocol, so the caller passes them in.
macro_rules! impl_body_envelope {
    (
        $wrapper:ty,
        success_body = $success_body:ty,
        error_body = $error_body:ty,
        headers = $headers:ty,
    ) => {
        impl BodyEnvelope for $wrapper {
            fn from_body_bytes(bytes: &[u8]) -> Result<Self, UpstreamError> {
                let success_err = match serde_json::from_slice::<$success_body>(bytes) {
                    Ok(body) => {
                        return Ok(Self::Success {
                            stats_code: StatusCode::OK,
                            headers: <$headers>::default(),
                            body,
                        });
                    }
                    Err(e) => e,
                };
                let error_err = match serde_json::from_slice::<$error_body>(bytes) {
                    Ok(body) => {
                        return Ok(Self::Error {
                            stats_code: StatusCode::BAD_REQUEST,
                            headers: <$headers>::default(),
                            body,
                        });
                    }
                    Err(e) => e,
                };
                let preview: String = String::from_utf8_lossy(
                    &bytes[..std::cmp::min(bytes.len(), 600)],
                )
                .into_owned();
                tracing::warn!(
                    wrapper = stringify!($wrapper),
                    success_error = %success_err,
                    error_variant_error = %error_err,
                    body_len = bytes.len(),
                    body_preview = %preview,
                    "response body did not match either variant of wrapper enum"
                );
                Err(UpstreamError::Channel(format!(
                    "deserialize: body does not match success or error variant of {} \
                     (success_err: {}; error_err: {})",
                    stringify!($wrapper),
                    success_err,
                    error_err
                )))
            }

            fn into_body_bytes(self) -> Result<Vec<u8>, UpstreamError> {
                match self {
                    Self::Success { body, .. } => serde_json::to_vec(&body)
                        .map_err(|e| UpstreamError::Channel(format!("serialize: {e}"))),
                    Self::Error { body, .. } => serde_json::to_vec(&body)
                        .map_err(|e| UpstreamError::Channel(format!("serialize: {e}"))),
                }
            }
        }
    };
}

impl_body_envelope!(
    gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
    success_body = gproxy_protocol::gemini::generate_content::response::ResponseBody,
    error_body = gproxy_protocol::gemini::types::GeminiApiErrorResponse,
    headers = gproxy_protocol::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
    success_body = gproxy_protocol::claude::create_message::response::ResponseBody,
    error_body = gproxy_protocol::claude::types::BetaErrorResponse,
    headers = gproxy_protocol::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
    success_body = gproxy_protocol::openai::create_chat_completions::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
    success_body = gproxy_protocol::openai::create_response::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
    success_body = gproxy_protocol::gemini::count_tokens::response::ResponseBody,
    error_body = gproxy_protocol::gemini::types::GeminiApiErrorResponse,
    headers = gproxy_protocol::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
    success_body = gproxy_protocol::openai::count_tokens::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
    success_body = gproxy_protocol::claude::count_tokens::response::ResponseBody,
    error_body = gproxy_protocol::claude::types::BetaErrorResponse,
    headers = gproxy_protocol::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
    success_body = gproxy_protocol::gemini::model_get::response::ResponseBody,
    error_body = gproxy_protocol::gemini::types::GeminiApiErrorResponse,
    headers = gproxy_protocol::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
    success_body = gproxy_protocol::openai::model_get::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
    success_body = gproxy_protocol::claude::model_get::response::ResponseBody,
    error_body = gproxy_protocol::claude::types::BetaErrorResponse,
    headers = gproxy_protocol::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
    success_body = gproxy_protocol::gemini::model_list::response::ResponseBody,
    error_body = gproxy_protocol::gemini::types::GeminiApiErrorResponse,
    headers = gproxy_protocol::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
    success_body = gproxy_protocol::openai::model_list::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
    success_body = gproxy_protocol::claude::model_list::response::ResponseBody,
    error_body = gproxy_protocol::claude::types::BetaErrorResponse,
    headers = gproxy_protocol::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::gemini::embeddings::response::GeminiEmbedContentResponse,
    success_body = gproxy_protocol::gemini::embeddings::response::ResponseBody,
    error_body = gproxy_protocol::gemini::types::GeminiApiErrorResponse,
    headers = gproxy_protocol::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::embeddings::response::OpenAiEmbeddingsResponse,
    success_body = gproxy_protocol::openai::embeddings::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::create_image::response::OpenAiCreateImageResponse,
    success_body = gproxy_protocol::openai::create_image::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
    success_body = gproxy_protocol::openai::create_image_edit::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    gproxy_protocol::openai::compact_response::response::OpenAiCompactResponse,
    success_body = gproxy_protocol::openai::compact_response::response::ResponseBody,
    error_body = gproxy_protocol::openai::types::OpenAiApiErrorResponse,
    headers = gproxy_protocol::openai::types::OpenAiResponseHeaders,
);

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

/// Generate a [`RequestDescriptor`] impl that stores `body` and uses
/// `Default::default()` for the rest of the envelope fields
/// (`method`, `path`, `query`, `headers`).
///
/// The request wrapper structs in `gproxy_protocol::*::request` all carry a
/// `{method, path, query, headers, body}` envelope — the rest of the gproxy
/// pipeline only cares about `body`, and [`transform_request_descriptor`]
/// reads the body JSON directly and reconstructs the wrapper around it.
///
/// Without this, attempting to `serde_json::from_slice::<Wrapper>` on a raw
/// HTTP request body fails with `missing field 'method'` because real HTTP
/// bodies only have the body fields at the top level, not the envelope.
macro_rules! impl_request_descriptor_default_envelope {
    ($wrapper:ty, body = $body:ty) => {
        impl RequestDescriptor for $wrapper {
            type Body = $body;

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
    };
}

impl_request_descriptor_default_envelope!(
    gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
    body = gproxy_protocol::claude::count_tokens::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
    body = gproxy_protocol::openai::count_tokens::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
    body = gproxy_protocol::gemini::count_tokens::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
    body = gproxy_protocol::claude::model_get::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
    body = gproxy_protocol::openai::model_get::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
    body = gproxy_protocol::gemini::model_get::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
    body = gproxy_protocol::claude::model_list::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
    body = gproxy_protocol::openai::model_list::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
    body = gproxy_protocol::gemini::model_list::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::openai::embeddings::request::OpenAiEmbeddingsRequest,
    body = gproxy_protocol::openai::embeddings::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::gemini::embeddings::request::GeminiEmbedContentRequest,
    body = gproxy_protocol::gemini::embeddings::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::openai::create_image::request::OpenAiCreateImageRequest,
    body = gproxy_protocol::openai::create_image::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::openai::create_image_edit::request::OpenAiCreateImageEditRequest,
    body = gproxy_protocol::openai::create_image_edit::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    gproxy_protocol::openai::compact_response::request::OpenAiCompactRequest,
    body = gproxy_protocol::openai::compact_response::request::RequestBody
);

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

        // === OpenAI Response source → OpenAI ChatCompletions ===
        //
        // Used by channels like deepseek that only expose the chat
        // completions surface but advertise the OpenAI Response protocol
        // to clients, so the dispatch table transforms on the way in.
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
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
        // Stream mirror of the non-stream arm above: deepseek and friends
        // advertise OpenAI Response streaming to clients but only speak
        // chat-completions upstream.
        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_request_descriptor_ref::<
                gproxy_protocol::openai::create_response::request::OpenAiCreateResponseRequest,
                gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
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
            transform_request_descriptor::<
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
            >(&body)
        }
        (OperationFamily::CountToken, ProtocolKind::Claude, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
            >(&body)
        }

        // --- OpenAI source ---
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
            >(&body)
        }
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
            >(&body)
        }

        // --- Gemini source ---
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
                gproxy_protocol::claude::count_tokens::request::ClaudeCountTokensRequest,
            >(&body)
        }
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::count_tokens::request::GeminiCountTokensRequest,
                gproxy_protocol::openai::count_tokens::request::OpenAiCountTokensRequest,
            >(&body)
        }

        // =====================================================================
        // model_get
        // =====================================================================

        // --- Claude source ---
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
            >(&body)
        }
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
            >(&body)
        }

        // --- OpenAI source ---
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
            >(&body)
        }
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
            >(&body)
        }

        // --- Gemini source ---
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
                gproxy_protocol::claude::model_get::request::ClaudeModelGetRequest,
            >(&body)
        }
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::model_get::request::GeminiModelGetRequest,
                gproxy_protocol::openai::model_get::request::OpenAiModelGetRequest,
            >(&body)
        }

        // =====================================================================
        // model_list
        // =====================================================================

        // --- Claude source ---
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
            >(&body)
        }
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_request_descriptor::<
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
            >(&body)
        }

        // --- OpenAI source ---
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
            >(&body)
        }
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
            >(&body)
        }

        // --- Gemini source ---
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
                gproxy_protocol::claude::model_list::request::ClaudeModelListRequest,
            >(&body)
        }
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_request_descriptor::<
                gproxy_protocol::gemini::model_list::request::GeminiModelListRequest,
                gproxy_protocol::openai::model_list::request::OpenAiModelListRequest,
            >(&body)
        }

        // =====================================================================
        // embeddings
        // =====================================================================

        (OperationFamily::Embedding, ProtocolKind::OpenAi, OperationFamily::Embedding, ProtocolKind::Gemini) => {
            transform_request_descriptor::<
                gproxy_protocol::openai::embeddings::request::OpenAiEmbeddingsRequest,
                gproxy_protocol::gemini::embeddings::request::GeminiEmbedContentRequest,
            >(&body)
        }
        (OperationFamily::Embedding, ProtocolKind::Gemini, OperationFamily::Embedding, ProtocolKind::OpenAi) => {
            transform_request_descriptor::<
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
            transform_response_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
            >(&body)
        }
        // OpenAI ChatCompletions response → Claude
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
            >(&body)
        }
        // OpenAI Response response → Claude
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
            >(&body)
        }
        // OpenAI ChatCompletions response → Gemini
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
            >(&body)
        }
        // OpenAI Response response → Gemini
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
            >(&body)
        }

        // Claude response → OpenAI ChatCompletions
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_response_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            >(&body)
        }
        // Gemini response → OpenAI ChatCompletions
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_response_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            >(&body)
        }
        // OpenAI Response response → OpenAI ChatCompletions
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion) => {
            transform_response_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            >(&body)
        }
        // OpenAI ChatCompletions response → OpenAI Response
        //
        // Mirror of the arm above, used when the client is speaking
        // OpenAI Response but the upstream only returns chat completions
        // (deepseek, groq, nvidia, etc.).
        (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_response_json::<
                gproxy_protocol::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
            >(&body)
        }

        // Claude response → OpenAI Response
        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_response_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
            >(&body)
        }
        // Gemini response → OpenAI Response
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse) => {
            transform_response_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
            >(&body)
        }

        // =====================================================================
        // count_tokens responses
        // =====================================================================

        // Gemini response → Claude
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
            >(&body)
        }
        // OpenAI response → Claude
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
            >(&body)
        }

        // Claude response → OpenAI
        (OperationFamily::CountToken, ProtocolKind::Claude, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
            >(&body)
        }
        // Gemini response → OpenAI
        (OperationFamily::CountToken, ProtocolKind::Gemini, OperationFamily::CountToken, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::CountToken, ProtocolKind::Claude, OperationFamily::CountToken, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::claude::count_tokens::response::ClaudeCountTokensResponse,
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
            >(&body)
        }
        // OpenAI response → Gemini
        (OperationFamily::CountToken, ProtocolKind::OpenAi, OperationFamily::CountToken, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::openai::count_tokens::response::OpenAiCountTokensResponse,
                gproxy_protocol::gemini::count_tokens::response::GeminiCountTokensResponse,
            >(&body)
        }

        // =====================================================================
        // model_get responses
        // =====================================================================

        // Gemini response → Claude
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
            >(&body)
        }
        // OpenAI response → Claude
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
            >(&body)
        }

        // Claude response → OpenAI
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
            >(&body)
        }
        // Gemini response → OpenAI
        (OperationFamily::ModelGet, ProtocolKind::Gemini, OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::ModelGet, ProtocolKind::Claude, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::claude::model_get::response::ClaudeModelGetResponse,
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
            >(&body)
        }
        // OpenAI response → Gemini
        (OperationFamily::ModelGet, ProtocolKind::OpenAi, OperationFamily::ModelGet, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::openai::model_get::response::OpenAiModelGetResponse,
                gproxy_protocol::gemini::model_get::response::GeminiModelGetResponse,
            >(&body)
        }

        // =====================================================================
        // model_list responses
        // =====================================================================

        // Gemini response → Claude
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
            >(&body)
        }
        // OpenAI response → Claude
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Claude) => {
            transform_response_json::<
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
            >(&body)
        }

        // Claude response → OpenAI
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
            >(&body)
        }
        // Gemini response → OpenAI
        (OperationFamily::ModelList, ProtocolKind::Gemini, OperationFamily::ModelList, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
            >(&body)
        }

        // Claude response → Gemini
        (OperationFamily::ModelList, ProtocolKind::Claude, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::claude::model_list::response::ClaudeModelListResponse,
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
            >(&body)
        }
        // OpenAI response → Gemini
        (OperationFamily::ModelList, ProtocolKind::OpenAi, OperationFamily::ModelList, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::openai::model_list::response::OpenAiModelListResponse,
                gproxy_protocol::gemini::model_list::response::GeminiModelListResponse,
            >(&body)
        }

        // =====================================================================
        // embeddings responses
        // =====================================================================

        (OperationFamily::Embedding, ProtocolKind::Gemini, OperationFamily::Embedding, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::gemini::embeddings::response::GeminiEmbedContentResponse,
                gproxy_protocol::openai::embeddings::response::OpenAiEmbeddingsResponse,
            >(&body)
        }
        (OperationFamily::Embedding, ProtocolKind::OpenAi, OperationFamily::Embedding, ProtocolKind::Gemini) => {
            transform_response_json::<
                gproxy_protocol::openai::embeddings::response::OpenAiEmbeddingsResponse,
                gproxy_protocol::gemini::embeddings::response::GeminiEmbedContentResponse,
            >(&body)
        }

        // =====================================================================
        // create_image responses
        // =====================================================================

        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImage, ProtocolKind::OpenAi)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImage, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_image::response::OpenAiCreateImageResponse,
            >(&body)
        }

        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImage, ProtocolKind::OpenAi)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImage, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::openai::create_image::response::OpenAiCreateImageResponse,
            >(&body)
        }

        // =====================================================================
        // create_image_edit responses
        // =====================================================================

        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::gemini::generate_content::response::GeminiGenerateContentResponse,
                gproxy_protocol::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
            >(&body)
        }

        (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse, OperationFamily::CreateImageEdit, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::openai::create_response::response::OpenAiCreateResponseResponse,
                gproxy_protocol::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
            >(&body)
        }

        // =====================================================================
        // compact responses
        // =====================================================================

        (OperationFamily::GenerateContent, ProtocolKind::Claude, OperationFamily::Compact, ProtocolKind::OpenAi) => {
            transform_response_json::<
                gproxy_protocol::claude::create_message::response::ClaudeCreateMessageResponse,
                gproxy_protocol::openai::compact_response::response::OpenAiCompactResponse,
            >(&body)
        }
        (OperationFamily::GenerateContent, ProtocolKind::Gemini, OperationFamily::Compact, ProtocolKind::OpenAi) => {
            transform_response_json::<
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

/// Generic JSON transform for request body structs.
///
/// Requests go through this path because their body types are plain structs
/// that deserialize directly from the raw HTTP body JSON — they don't have
/// the internal `{stats_code, headers, body}` envelope that response wrapper
/// enums use. The response-side equivalent is [`transform_response_json`].
fn transform_json<Src, Dst>(body: &[u8]) -> Result<Vec<u8>, UpstreamError>
where
    Src: serde::de::DeserializeOwned,
    Dst: TryFrom<Src> + serde::Serialize,
    Dst::Error: std::fmt::Display,
{
    let src: Src = serde_json::from_slice(body)
        .map_err(|e| UpstreamError::Channel(format!("request deserialize: {e}")))?;
    let dst = Dst::try_from(src).map_err(|e| UpstreamError::Channel(format!("transform: {e}")))?;
    serde_json::to_vec(&dst).map_err(|e| UpstreamError::Channel(format!("response serialize: {e}")))
}

/// Generic JSON transform for response wrapper enums.
///
/// Deserializes the raw upstream response body into `Src` via its
/// [`BodyEnvelope::from_body_bytes`] impl (which wraps the raw body in the
/// wrapper's `Success`/`Error` variant with placeholder metadata), converts
/// into `Dst` via `TryFrom`, then serializes just the inner body of `Dst`
/// back out for the client via [`BodyEnvelope::into_body_bytes`].
///
/// This is the central helper that makes `transform_response` route arms
/// work with the `stats_code` / `headers` / `body` wrapper enum shape —
/// without it, `from_slice::<Wrapper>` fails on real HTTP bodies because
/// they don't have top-level `stats_code` / `headers` fields.
fn transform_response_json<Src, Dst>(body: &[u8]) -> Result<Vec<u8>, UpstreamError>
where
    Src: BodyEnvelope,
    Dst: BodyEnvelope + TryFrom<Src>,
    Dst::Error: std::fmt::Display,
{
    let src = Src::from_body_bytes(body)?;
    let dst = Dst::try_from(src).map_err(|e| UpstreamError::Channel(format!("transform: {e}")))?;
    dst.into_body_bytes()
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

pub type StreamChunkNormalizer = Arc<dyn Fn(Vec<u8>) -> Vec<u8> + Send + Sync>;

/// Convert an upstream error body (non-2xx response body) from the
/// upstream protocol's error schema to the client's expected error
/// schema, falling back to the raw bytes on any failure.
///
/// Each protocol uses a different error shape:
/// - Claude: `{"type":"error","error":{"type":"...","message":"..."}}`
/// - OpenAI: `{"error":{"message":"...","type":"...","code":"..."}}`
/// - Gemini: `{"error":{"code":N,"message":"...","status":"..."}}`
///
/// An OpenAI-speaking client that receives a Claude error body cannot
/// parse it, so we route error bodies through the same `transform_response`
/// pipeline that success bodies use. The `BodyEnvelope::from_body_bytes`
/// macro tries both the success and error variants, so if the upstream
/// error conforms to the declared error schema, it's converted via the
/// existing `TryFrom<SrcResponse> for DstResponse` impls (which handle
/// the `Error { .. }` variant separately).
///
/// If the upstream error schema doesn't match the declared `error_body`
/// type (e.g. codex returning `{"detail":{"code":"..."}}` which is not
/// `OpenAiApiErrorResponse`), `from_body_bytes` fails and the whole
/// transform errors out — in which case this helper forwards the raw
/// upstream bytes so the client at least sees what the upstream sent.
///
/// For streaming operations, the operation family is substituted with
/// `GenerateContent` before dispatch. Error bodies share the same schema
/// regardless of whether the request was streaming, but `transform_response`
/// only declares arms for non-stream ops.
pub fn convert_error_body_or_raw(
    src_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_operation: OperationFamily,
    dst_protocol: ProtocolKind,
    body: Vec<u8>,
) -> Vec<u8> {
    let op_for_error = |op: OperationFamily| match op {
        OperationFamily::StreamGenerateContent => OperationFamily::GenerateContent,
        other => other,
    };
    let src_op = op_for_error(src_operation);
    let dst_op = op_for_error(dst_operation);

    // Passthrough (src == dst): no conversion needed, the error is
    // already in the client's expected format.
    if src_op == dst_op && src_protocol == dst_protocol {
        return body;
    }

    match transform_response(src_op, src_protocol, dst_op, dst_protocol, body.clone()) {
        Ok(converted) => converted,
        Err(err) => {
            tracing::debug!(
                error = %err,
                src_op = %src_operation,
                src_proto = %src_protocol,
                dst_op = %dst_operation,
                dst_proto = %dst_protocol,
                body_len = body.len(),
                "error body did not match declared schema; forwarding raw upstream bytes"
            );
            body
        }
    }
}

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

/// Stream converter for `OpenAI Responses stream` → `OpenAI Chat Completions stream`.
///
/// Used by the codex channel which forwards chat-completions traffic as
/// OpenAI Response streams upstream and must reverse the protocol on the
/// way back to the client. The wrapped stream converter lives in
/// `gproxy_protocol::transform::openai::stream_generate_content`.
#[derive(Default)]
struct OpenAiResponseToOpenAiChatCompletionsConverter(
    gproxy_protocol::transform::openai::stream_generate_content::openai_chat_completions::openai_response::response::OpenAiResponseToOpenAiChatCompletionsStream,
);

impl
    EventConverter<
        gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
    > for OpenAiResponseToOpenAiChatCompletionsConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        >,
    ) -> Result<(), UpstreamError> {
        self.0
            .on_stream_event(input, out)
            .map_err(|e| UpstreamError::Channel(format!("stream convert: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        >,
    ) -> Result<(), UpstreamError> {
        self.0
            .finish(out)
            .map_err(|e| UpstreamError::Channel(format!("stream finish: {e}")))
    }
}

/// Stream converter for `OpenAI Chat Completions stream` → `OpenAI Responses stream`.
///
/// The reverse of `OpenAiResponseToOpenAiChatCompletionsConverter`,
/// used when clients speak OpenAI Response but the upstream channel
/// only exposes chat completions (deepseek, groq, nvidia, etc.).
#[derive(Default)]
struct OpenAiChatCompletionsToOpenAiResponseConverter(
    gproxy_protocol::transform::openai::stream_generate_content::openai_response::openai_chat_completions::response::OpenAiChatCompletionsToOpenAiResponseStream,
);

impl
    EventConverter<
        gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
    > for OpenAiChatCompletionsToOpenAiResponseConverter
{
    fn on_input(
        &mut self,
        input: gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
        out: &mut Vec<gproxy_protocol::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0
            .on_stream_event(input, out)
            .map_err(|e| UpstreamError::Channel(format!("stream convert: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<gproxy_protocol::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), UpstreamError> {
        self.0
            .finish(out)
            .map_err(|e| UpstreamError::Channel(format!("stream finish: {e}")))
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

        // OpenAI Responses stream → OpenAI Chat Completions stream.
        //
        // This arm exists for providers like codex which forward chat-completions
        // traffic as OpenAI Responses upstream; the client expects chat chunks
        // back so we reverse the protocol on the response path.
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            OpenAiResponseToOpenAiChatCompletionsConverter,
        >(
            src_protocol,
            dst_protocol,
            OpenAiResponseToOpenAiChatCompletionsConverter::default(),
            normalizer,
        ),

        // OpenAI Chat Completions stream → OpenAI Responses stream.
        //
        // Mirror of the arm above, used by channels that only expose the
        // chat completions surface but advertise the Response protocol to
        // clients — deepseek, groq, nvidia, etc. Client speaks OpenAI
        // Response, upstream returns chat chunks, and this converter
        // folds them back into Response stream events.
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => build_stream_transform::<
            gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk,
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
            OpenAiChatCompletionsToOpenAiResponseConverter,
        >(
            src_protocol,
            dst_protocol,
            OpenAiChatCompletionsToOpenAiResponseConverter::default(),
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

            // Emit only the inner body — callers of `stream_to_nonstream`
            // expect raw HTTP body shape, not the internal
            // `{stats_code, headers, body}` wrapper envelope.
            response.into_body_bytes()
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

            response.into_body_bytes()
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

            response.into_body_bytes()
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

    use super::{convert_error_body_or_raw, transform_request, transform_response};

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

    /// Regression test for the request envelope bug: request wrapper structs
    /// such as `ClaudeCountTokensRequest` have shape
    /// `{method, path, query, headers, body}` for internal routing purposes.
    /// A real HTTP request body only has the body fields at the top level, so
    /// `serde_json::from_slice::<Wrapper>(body)` fails with "missing field
    /// method". The fix: route all envelope-shaped request types through
    /// `transform_request_descriptor`, which parses just the inner body JSON
    /// and reconstructs the envelope with `Default::default()`. Covers
    /// count_tokens (Claude→Gemini) as the user-visible case that failed with
    /// `POST /aistudio/v1/messages/count-tokens`.
    #[test]
    fn transform_request_count_tokens_claude_to_gemini_accepts_bare_body() {
        let body = br#"{
          "model": "gemini-3-flash-preview",
          "messages": [{"role": "user", "content": "hi"}]
        }"#
        .to_vec();

        let transformed = transform_request(
            OperationFamily::CountToken,
            ProtocolKind::Claude,
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
            body,
        )
        .expect("count_tokens Claude -> Gemini request transform should succeed");

        // Output should be the Gemini countTokens body (contents field).
        let json: Value = serde_json::from_slice(&transformed).expect("transformed json");
        assert!(
            json.get("contents").is_some()
                || json.pointer("/generateContentRequest/contents").is_some(),
            "expected gemini countTokens body shape, got: {json}"
        );
        // Must NOT leak the envelope fields back out.
        assert!(
            json.get("method").is_none(),
            "transformed body leaked the request envelope method field"
        );
    }

    /// Regression test for the non-stream response transform bug: upstream
    /// HTTP bodies are raw JSON like `{"id": "...", "output": [...]}`, not
    /// a `{stats_code, headers, body: {...}}` envelope. The wrapper enums in
    /// `gproxy_protocol::*::response` have the envelope shape for internal
    /// bookkeeping, so deserializing the raw body directly into the wrapper
    /// fails. `transform_json` must call [`BodyEnvelope::from_body_bytes`] to
    /// parse only the inner body and wrap it with placeholder metadata.
    ///
    /// Covers OpenAI Response → OpenAI ChatCompletions, the case the
    /// previous `transform_openai_response_wrapper_to_chat_completions`
    /// helper special-cased. With the `BodyEnvelope` trait the generic
    /// `transform_json` handles it without a dedicated function.
    #[test]
    fn transform_response_accepts_bare_openai_response_body_for_openai_chat() {
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

        let transformed = transform_response(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            body,
        )
        .expect("bare responses body should now be accepted");

        let json: Value = serde_json::from_slice(&transformed).expect("transformed json");
        assert_eq!(json.get("model").and_then(Value::as_str), Some("gpt-5.4"));
        assert_eq!(
            json.pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            Some("OK")
        );
        // The serialized response must be the raw chat-completions body, not
        // the internal `{stats_code, headers, body}` wrapper envelope.
        assert!(
            json.get("stats_code").is_none(),
            "serialized response leaked the internal wrapper envelope"
        );
    }

    /// Regression test for the gemini → claude non-stream response
    /// transform. The bug surfaced when posting to
    /// `/aistudio/v1/messages` (Claude format over an aistudio provider):
    /// gproxy transformed the request to Gemini, sent it upstream, got a
    /// raw `{"candidates":[...], "usageMetadata":{...}}` body back, then
    /// tried to deserialize it as `GeminiGenerateContentResponse` (the
    /// wrapper enum). That produced
    /// `data did not match any variant of untagged enum GeminiGenerateContentResponse`
    /// and the client saw a 500 "upstream provider error".
    #[test]
    fn transform_response_gemini_to_claude_accepts_bare_gemini_body() {
        let body = serde_json::to_vec(&json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello"}],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }],
            "usageMetadata": {
                "promptTokenCount": 7,
                "candidatesTokenCount": 1,
                "totalTokenCount": 8
            },
            "modelVersion": "gemini-3-flash-preview",
            "responseId": "test-response-id"
        }))
        .expect("serialize gemini body");

        let transformed = transform_response(
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            body,
        )
        .expect("gemini -> claude transform must accept raw gemini body");

        let json: Value = serde_json::from_slice(&transformed).expect("transformed json");
        // Claude response shape: top-level `id`, `content`, `role`, `type`, etc.
        assert_eq!(json.get("type").and_then(Value::as_str), Some("message"));
        assert_eq!(json.get("role").and_then(Value::as_str), Some("assistant"));
        assert!(
            json.get("stats_code").is_none(),
            "serialized response leaked the internal wrapper envelope"
        );
        assert!(json.get("content").is_some(), "missing content field");
    }

    /// When upstream Claude returns a standard Claude error body
    /// (`{"type":"error","error":{...}}`), an OpenAI chat completions
    /// client must see the error in OpenAI's `{"error":{...}}` shape —
    /// not the raw Claude JSON, which an OpenAI SDK can't parse.
    #[test]
    fn convert_error_body_claude_to_openai_chat_rewrites_schema() {
        let claude_error = br#"{
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "prompt is too long: 1057153 tokens > 1000000 maximum"
            },
            "request_id": "req_011Ca1mHSW1W47w6LbmKQNWf"
        }"#
        .to_vec();

        let converted = convert_error_body_or_raw(
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            claude_error.clone(),
        );

        let json: Value =
            serde_json::from_slice(&converted).expect("converted body should be valid JSON");
        // OpenAI error shape: top-level `error.message`, `error.type`.
        let error_obj = json
            .get("error")
            .expect("OpenAI error body must have top-level `error` field");
        assert_eq!(
            error_obj.get("message").and_then(Value::as_str),
            Some("prompt is too long: 1057153 tokens > 1000000 maximum"),
            "error message must survive the schema conversion"
        );
        assert!(
            json.get("type").and_then(Value::as_str) != Some("error"),
            "converted body must not still be in Claude's top-level `type:error` shape"
        );
    }

    /// When upstream returns an error body in a shape that doesn't match
    /// any declared `error_body` schema (e.g. codex's
    /// `{"detail":{"code":"deactivated_workspace"}}`), the helper must
    /// fall back to forwarding the raw upstream bytes so the error
    /// information isn't lost.
    #[test]
    fn convert_error_body_falls_back_to_raw_on_schema_mismatch() {
        let codex_error = br#"{"detail":{"code":"deactivated_workspace"}}"#.to_vec();

        let result = convert_error_body_or_raw(
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            codex_error.clone(),
        );

        assert_eq!(
            result, codex_error,
            "fallback must return the raw bytes verbatim"
        );
    }

    /// Passthrough routes (same src/dst protocol, same op) should leave
    /// error bodies unchanged — conversion is unnecessary and would
    /// only add latency.
    #[test]
    fn convert_error_body_passthrough_returns_unchanged() {
        let claude_error = br#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#.to_vec();

        let result = convert_error_body_or_raw(
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            claude_error.clone(),
        );

        assert_eq!(result, claude_error);
    }
}
