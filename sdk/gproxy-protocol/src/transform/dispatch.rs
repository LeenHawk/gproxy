//! Runtime-keyed transform dispatcher.
//!
//! The typed transforms in `crate::transform::{claude,gemini,openai}::*` are
//! statically dispatched via `TryFrom` bounds. This module exposes the same
//! conversions as a dynamic API keyed on `(OperationFamily, ProtocolKind)`
//! pairs, which is what an LLM gateway needs at request time when the source
//! and destination protocols are only known at runtime.
//!
//! Moved here from `gproxy-provider::transform_dispatch` in the SDK layer
//! refactor (spec: docs/superpowers/specs/2026-04-13-sdk-layer-refactor-design.md).

use std::marker::PhantomData;
use std::sync::Arc;

use crate::kinds::{OperationFamily, ProtocolKind};
use http::StatusCode;
use serde::{Serialize, de::DeserializeOwned};

use crate::transform::utils::TransformError;

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
    fn from_body_bytes(body: &[u8]) -> Result<Self, TransformError>;

    /// Serialize just the inner `body` field of this wrapper to JSON bytes
    /// for the client-facing HTTP response.
    fn into_body_bytes(self) -> Result<Vec<u8>, TransformError>;
}

/// Generate a [`BodyEnvelope`] impl for a protocol response wrapper enum.
///
/// The macro covers the uniform `Success { stats_code, headers, body } |
/// Error { stats_code, headers, body }` shape shared by every wrapper in
/// `crate::{claude, gemini, openai}::*::response`. The two `body`
/// field types differ per protocol, so the caller passes them in.
macro_rules! impl_body_envelope {
    (
        $wrapper:ty,
        success_body = $success_body:ty,
        error_body = $error_body:ty,
        headers = $headers:ty,
    ) => {
        impl BodyEnvelope for $wrapper {
            fn from_body_bytes(bytes: &[u8]) -> Result<Self, TransformError> {
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
                Err(TransformError::new(format!(
                    "deserialize: body does not match success or error variant of {} \
                     (success_err: {}; error_err: {})",
                    stringify!($wrapper),
                    success_err,
                    error_err
                )))
            }

            fn into_body_bytes(self) -> Result<Vec<u8>, TransformError> {
                match self {
                    Self::Success { body, .. } => serde_json::to_vec(&body)
                        .map_err(|e| TransformError::new(format!("serialize: {e}"))),
                    Self::Error { body, .. } => serde_json::to_vec(&body)
                        .map_err(|e| TransformError::new(format!("serialize: {e}"))),
                }
            }
        }
    };
}

impl_body_envelope!(
    crate::gemini::generate_content::response::GeminiGenerateContentResponse,
    success_body = crate::gemini::generate_content::response::ResponseBody,
    error_body = crate::gemini::types::GeminiApiErrorResponse,
    headers = crate::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    crate::claude::create_message::response::ClaudeCreateMessageResponse,
    success_body = crate::claude::create_message::response::ResponseBody,
    error_body = crate::claude::types::BetaErrorResponse,
    headers = crate::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
    success_body = crate::openai::create_chat_completions::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::create_response::response::OpenAiCreateResponseResponse,
    success_body = crate::openai::create_response::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::gemini::count_tokens::response::GeminiCountTokensResponse,
    success_body = crate::gemini::count_tokens::response::ResponseBody,
    error_body = crate::gemini::types::GeminiApiErrorResponse,
    headers = crate::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::count_tokens::response::OpenAiCountTokensResponse,
    success_body = crate::openai::count_tokens::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::claude::count_tokens::response::ClaudeCountTokensResponse,
    success_body = crate::claude::count_tokens::response::ResponseBody,
    error_body = crate::claude::types::BetaErrorResponse,
    headers = crate::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    crate::gemini::model_get::response::GeminiModelGetResponse,
    success_body = crate::gemini::model_get::response::ResponseBody,
    error_body = crate::gemini::types::GeminiApiErrorResponse,
    headers = crate::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::model_get::response::OpenAiModelGetResponse,
    success_body = crate::openai::model_get::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::claude::model_get::response::ClaudeModelGetResponse,
    success_body = crate::claude::model_get::response::ResponseBody,
    error_body = crate::claude::types::BetaErrorResponse,
    headers = crate::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    crate::gemini::model_list::response::GeminiModelListResponse,
    success_body = crate::gemini::model_list::response::ResponseBody,
    error_body = crate::gemini::types::GeminiApiErrorResponse,
    headers = crate::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::model_list::response::OpenAiModelListResponse,
    success_body = crate::openai::model_list::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::claude::model_list::response::ClaudeModelListResponse,
    success_body = crate::claude::model_list::response::ResponseBody,
    error_body = crate::claude::types::BetaErrorResponse,
    headers = crate::claude::types::ClaudeResponseHeaders,
);

impl_body_envelope!(
    crate::gemini::embeddings::response::GeminiEmbedContentResponse,
    success_body = crate::gemini::embeddings::response::ResponseBody,
    error_body = crate::gemini::types::GeminiApiErrorResponse,
    headers = crate::gemini::types::GeminiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::embeddings::response::OpenAiEmbeddingsResponse,
    success_body = crate::openai::embeddings::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::create_image::response::OpenAiCreateImageResponse,
    success_body = crate::openai::create_image::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
    success_body = crate::openai::create_image_edit::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

impl_body_envelope!(
    crate::openai::compact_response::response::OpenAiCompactResponse,
    success_body = crate::openai::compact_response::response::ResponseBody,
    error_body = crate::openai::types::OpenAiApiErrorResponse,
    headers = crate::openai::types::OpenAiResponseHeaders,
);

trait RequestDescriptor: Sized {
    type Body: DeserializeOwned + Serialize;

    fn from_body(body: Self::Body) -> Self;
    fn into_body(self) -> Self::Body;

    /// Inject a model identifier into the request descriptor's path parameters.
    ///
    /// The HTTP body alone does not always carry the model — Gemini requests,
    /// for example, keep it in the URL (`/v1beta/models/{model}:generateContent`).
    /// `transform_request` strips the envelope to the body for JSON parsing, so
    /// without this hook the downstream `TryFrom` impls read an empty
    /// `path.model` and the transformed upstream body is missing the model.
    ///
    /// Default: no-op for descriptors whose path carries no model.
    fn with_model(self, _model: Option<&str>) -> Self {
        self
    }
}

impl RequestDescriptor
    for crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest
{
    type Body = crate::openai::create_chat_completions::request::RequestBody;

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

impl RequestDescriptor for crate::openai::create_response::request::OpenAiCreateResponseRequest {
    type Body = crate::openai::create_response::request::RequestBody;

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

impl RequestDescriptor for crate::claude::create_message::request::ClaudeCreateMessageRequest {
    type Body = crate::claude::create_message::request::RequestBody;

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

impl RequestDescriptor for crate::gemini::generate_content::request::GeminiGenerateContentRequest {
    type Body = crate::gemini::generate_content::request::RequestBody;

    fn from_body(body: Self::Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    fn into_body(self) -> Self::Body {
        self.body
    }

    fn with_model(mut self, model: Option<&str>) -> Self {
        if let Some(m) = model {
            self.path.model = m.to_string();
        }
        self
    }
}

impl RequestDescriptor
    for crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest
{
    type Body = crate::gemini::stream_generate_content::request::RequestBody;

    fn from_body(body: Self::Body) -> Self {
        Self {
            body,
            ..Self::default()
        }
    }

    fn into_body(self) -> Self::Body {
        self.body
    }

    fn with_model(mut self, model: Option<&str>) -> Self {
        if let Some(m) = model {
            self.path.model = m.to_string();
        }
        self
    }
}

/// Generate a [`RequestDescriptor`] impl that stores `body` and uses
/// `Default::default()` for the rest of the envelope fields
/// (`method`, `path`, `query`, `headers`).
///
/// The request wrapper structs in `crate::*::request` all carry a
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
    ($wrapper:ty, body = $body:ty, path_model = $field:ident) => {
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

            fn with_model(mut self, model: Option<&str>) -> Self {
                if let Some(m) = model {
                    self.path.$field = m.to_string();
                }
                self
            }
        }
    };
}

impl_request_descriptor_default_envelope!(
    crate::claude::count_tokens::request::ClaudeCountTokensRequest,
    body = crate::claude::count_tokens::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::openai::count_tokens::request::OpenAiCountTokensRequest,
    body = crate::openai::count_tokens::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::gemini::count_tokens::request::GeminiCountTokensRequest,
    body = crate::gemini::count_tokens::request::RequestBody,
    path_model = model
);
impl_request_descriptor_default_envelope!(
    crate::claude::model_get::request::ClaudeModelGetRequest,
    body = crate::claude::model_get::request::RequestBody,
    path_model = model_id
);
impl_request_descriptor_default_envelope!(
    crate::openai::model_get::request::OpenAiModelGetRequest,
    body = crate::openai::model_get::request::RequestBody,
    path_model = model
);
impl_request_descriptor_default_envelope!(
    crate::gemini::model_get::request::GeminiModelGetRequest,
    body = crate::gemini::model_get::request::RequestBody,
    path_model = name
);
impl_request_descriptor_default_envelope!(
    crate::claude::model_list::request::ClaudeModelListRequest,
    body = crate::claude::model_list::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::openai::model_list::request::OpenAiModelListRequest,
    body = crate::openai::model_list::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::gemini::model_list::request::GeminiModelListRequest,
    body = crate::gemini::model_list::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::openai::embeddings::request::OpenAiEmbeddingsRequest,
    body = crate::openai::embeddings::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::gemini::embeddings::request::GeminiEmbedContentRequest,
    body = crate::gemini::embeddings::request::RequestBody,
    path_model = model
);
impl_request_descriptor_default_envelope!(
    crate::openai::create_image::request::OpenAiCreateImageRequest,
    body = crate::openai::create_image::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::openai::create_image_edit::request::OpenAiCreateImageEditRequest,
    body = crate::openai::create_image_edit::request::RequestBody
);
impl_request_descriptor_default_envelope!(
    crate::openai::compact_response::request::OpenAiCompactRequest,
    body = crate::openai::compact_response::request::RequestBody
);

/// Translate URL query keys across protocols for operations whose query
/// semantics differ. Currently only ModelList (`pageSize`↔`limit`,
/// `pageToken`↔`after_id`). Unknown keys are dropped since the upstream
/// protocol won't understand them.
fn translate_request_query(
    src_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_operation: OperationFamily,
    dst_protocol: ProtocolKind,
    query: Option<&str>,
) -> Option<String> {
    let Some(raw) = query else {
        return None;
    };
    if raw.is_empty() {
        return None;
    }

    // Only ModelList has cross-protocol query semantics worth translating.
    // Everything else passes through verbatim — upstream will accept or
    // ignore as per its contract.
    if !(src_operation == OperationFamily::ModelList
        && dst_operation == OperationFamily::ModelList
        && src_protocol != dst_protocol)
    {
        return Some(raw.to_owned());
    }

    let mut page_size: Option<String> = None;
    let mut page_token: Option<String> = None;
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        match (src_protocol, key.as_ref()) {
            (ProtocolKind::Gemini | ProtocolKind::GeminiNDJson, "pageSize") => {
                page_size = Some(value.into_owned())
            }
            (ProtocolKind::Gemini | ProtocolKind::GeminiNDJson, "pageToken") => {
                page_token = Some(value.into_owned())
            }
            (ProtocolKind::Claude, "limit") => page_size = Some(value.into_owned()),
            (ProtocolKind::Claude, "after_id") => page_token = Some(value.into_owned()),
            (
                ProtocolKind::OpenAi
                | ProtocolKind::OpenAiChatCompletion
                | ProtocolKind::OpenAiResponse,
                "limit",
            ) => page_size = Some(value.into_owned()),
            (
                ProtocolKind::OpenAi
                | ProtocolKind::OpenAiChatCompletion
                | ProtocolKind::OpenAiResponse,
                "after",
            ) => page_token = Some(value.into_owned()),
            _ => {}
        }
    }

    let mut out = url::form_urlencoded::Serializer::new(String::new());
    match dst_protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            if let Some(v) = page_size {
                out.append_pair("pageSize", &v);
            }
            if let Some(v) = page_token {
                out.append_pair("pageToken", &v);
            }
        }
        ProtocolKind::Claude => {
            if let Some(v) = page_size {
                out.append_pair("limit", &v);
            }
            if let Some(v) = page_token {
                out.append_pair("after_id", &v);
            }
        }
        ProtocolKind::OpenAi
        | ProtocolKind::OpenAiChatCompletion
        | ProtocolKind::OpenAiResponse => {
            if let Some(v) = page_size {
                out.append_pair("limit", &v);
            }
            if let Some(v) = page_token {
                out.append_pair("after", &v);
            }
        }
    }
    let s = out.finish();
    if s.is_empty() { None } else { Some(s) }
}

/// Transform a request body from one (operation, protocol) to another.
///
/// This dispatches to the appropriate `TryFrom` implementation in `crate::transform`.
pub fn transform_request(
    src_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_operation: OperationFamily,
    dst_protocol: ProtocolKind,
    model: Option<&str>,
    query: Option<&str>,
    body: Vec<u8>,
) -> Result<(Option<String>, Vec<u8>), TransformError> {
    if src_operation == dst_operation && src_protocol == dst_protocol {
        return Ok((query.map(str::to_owned), body));
    }

    // Translate URL query for operations whose query semantics differ
    // across protocols. Body transform continues below as normal.
    let translated_query = translate_request_query(
        src_operation,
        src_protocol,
        dst_operation,
        dst_protocol,
        query,
    );

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
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_request_descriptor::<
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
        >(&body, model),
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_request_descriptor::<
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body, model),

        // === Claude source → Gemini targets ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
        >(&body, model),

        // === OpenAI ChatCompletions source → Claude ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body, model),

        // === OpenAI ChatCompletions source → Gemini ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
        >(&body, model),

        // === OpenAI ChatCompletions source → OpenAI Response ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_request_descriptor::<
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body, model),

        // === OpenAI Response source → OpenAI ChatCompletions ===
        //
        // Used by channels like deepseek that only expose the chat
        // completions surface but advertise the OpenAI Response protocol
        // to clients, so the dispatch table transforms on the way in.
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_request_descriptor::<
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
        >(&body, model),

        // === OpenAI Response source → Claude ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body, model),

        // === OpenAI Response source → Gemini ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
        >(&body, model),

        // === Gemini source → Claude ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body, model),

        // === Gemini source → OpenAI ChatCompletions ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_request_descriptor::<
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
        >(&body, model),

        // === Gemini source → OpenAI Response ===
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_request_descriptor::<
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body, model),

        // =====================================================================
        // stream_generate_content (request transforms only)
        // =====================================================================

        // --- Claude source ---
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor_ref::<
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => transform_request_descriptor_ref::<
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_request_descriptor_ref::<
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_request_descriptor_ref::<
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body, model),

        // --- Gemini source ---
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_request_descriptor::<
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_request_descriptor::<
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_request_descriptor::<
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_request_descriptor::<
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body, model),

        // --- OpenAI ChatCompletions source ---
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body, model),
        // Stream mirror of the non-stream arm above: deepseek and friends
        // advertise OpenAI Response streaming to clients but only speak
        // chat-completions upstream.
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
            crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest,
        >(&body, model),

        // --- OpenAI Response source ---
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
        >(&body, model),
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::GeminiNDJson,
        ) => transform_request_descriptor_ref::<
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
            crate::gemini::stream_generate_content::request::GeminiStreamGenerateContentRequest,
        >(&body, model),

        // =====================================================================
        // count_tokens
        // =====================================================================

        // --- Claude source ---
        (
            OperationFamily::CountToken,
            ProtocolKind::Claude,
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::claude::count_tokens::request::ClaudeCountTokensRequest,
            crate::gemini::count_tokens::request::GeminiCountTokensRequest,
        >(&body, model),
        (
            OperationFamily::CountToken,
            ProtocolKind::Claude,
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
        ) => transform_request_descriptor::<
            crate::claude::count_tokens::request::ClaudeCountTokensRequest,
            crate::openai::count_tokens::request::OpenAiCountTokensRequest,
        >(&body, model),

        // --- OpenAI source ---
        (
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
            OperationFamily::CountToken,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::openai::count_tokens::request::OpenAiCountTokensRequest,
            crate::claude::count_tokens::request::ClaudeCountTokensRequest,
        >(&body, model),
        (
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::openai::count_tokens::request::OpenAiCountTokensRequest,
            crate::gemini::count_tokens::request::GeminiCountTokensRequest,
        >(&body, model),

        // --- Gemini source ---
        (
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
            OperationFamily::CountToken,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::gemini::count_tokens::request::GeminiCountTokensRequest,
            crate::claude::count_tokens::request::ClaudeCountTokensRequest,
        >(&body, model),
        (
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
        ) => transform_request_descriptor::<
            crate::gemini::count_tokens::request::GeminiCountTokensRequest,
            crate::openai::count_tokens::request::OpenAiCountTokensRequest,
        >(&body, model),

        // =====================================================================
        // model_get
        // =====================================================================

        // --- Claude source ---
        (
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::claude::model_get::request::ClaudeModelGetRequest,
            crate::gemini::model_get::request::GeminiModelGetRequest,
        >(&body, model),
        (
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
        ) => transform_request_descriptor::<
            crate::claude::model_get::request::ClaudeModelGetRequest,
            crate::openai::model_get::request::OpenAiModelGetRequest,
        >(&body, model),

        // --- OpenAI source ---
        (
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::openai::model_get::request::OpenAiModelGetRequest,
            crate::claude::model_get::request::ClaudeModelGetRequest,
        >(&body, model),
        (
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::openai::model_get::request::OpenAiModelGetRequest,
            crate::gemini::model_get::request::GeminiModelGetRequest,
        >(&body, model),

        // --- Gemini source ---
        (
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::gemini::model_get::request::GeminiModelGetRequest,
            crate::claude::model_get::request::ClaudeModelGetRequest,
        >(&body, model),
        (
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
        ) => transform_request_descriptor::<
            crate::gemini::model_get::request::GeminiModelGetRequest,
            crate::openai::model_get::request::OpenAiModelGetRequest,
        >(&body, model),

        // =====================================================================
        // model_list
        // =====================================================================

        // --- Claude source ---
        (
            OperationFamily::ModelList,
            ProtocolKind::Claude,
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::claude::model_list::request::ClaudeModelListRequest,
            crate::gemini::model_list::request::GeminiModelListRequest,
        >(&body, model),
        (
            OperationFamily::ModelList,
            ProtocolKind::Claude,
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
        ) => transform_request_descriptor::<
            crate::claude::model_list::request::ClaudeModelListRequest,
            crate::openai::model_list::request::OpenAiModelListRequest,
        >(&body, model),

        // --- OpenAI source ---
        (
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
            OperationFamily::ModelList,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::openai::model_list::request::OpenAiModelListRequest,
            crate::claude::model_list::request::ClaudeModelListRequest,
        >(&body, model),
        (
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::openai::model_list::request::OpenAiModelListRequest,
            crate::gemini::model_list::request::GeminiModelListRequest,
        >(&body, model),

        // --- Gemini source ---
        (
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            OperationFamily::ModelList,
            ProtocolKind::Claude,
        ) => transform_request_descriptor::<
            crate::gemini::model_list::request::GeminiModelListRequest,
            crate::claude::model_list::request::ClaudeModelListRequest,
        >(&body, model),
        (
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
        ) => transform_request_descriptor::<
            crate::gemini::model_list::request::GeminiModelListRequest,
            crate::openai::model_list::request::OpenAiModelListRequest,
        >(&body, model),

        // =====================================================================
        // embeddings
        // =====================================================================
        (
            OperationFamily::Embedding,
            ProtocolKind::OpenAi,
            OperationFamily::Embedding,
            ProtocolKind::Gemini,
        ) => transform_request_descriptor::<
            crate::openai::embeddings::request::OpenAiEmbeddingsRequest,
            crate::gemini::embeddings::request::GeminiEmbedContentRequest,
        >(&body, model),
        (
            OperationFamily::Embedding,
            ProtocolKind::Gemini,
            OperationFamily::Embedding,
            ProtocolKind::OpenAi,
        ) => transform_request_descriptor::<
            crate::gemini::embeddings::request::GeminiEmbedContentRequest,
            crate::openai::embeddings::request::OpenAiEmbeddingsRequest,
        >(&body, model),

        // =====================================================================
        // create_image
        // =====================================================================
        (
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        ) => transform_json::<
            crate::openai::create_image::request::OpenAiCreateImageRequest,
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
        >(&body),

        (
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        )
        | (
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_json::<
            crate::openai::create_image::request::OpenAiCreateImageRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body),

        // =====================================================================
        // create_image_edit
        // =====================================================================
        (
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        )
        | (
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        ) => transform_json::<
            crate::openai::create_image_edit::request::OpenAiCreateImageEditRequest,
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
        >(&body),

        (
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
        )
        | (
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_json::<
            crate::openai::create_image_edit::request::OpenAiCreateImageEditRequest,
            crate::openai::create_response::request::OpenAiCreateResponseRequest,
        >(&body),

        // =====================================================================
        // compact
        // =====================================================================
        (
            OperationFamily::Compact,
            ProtocolKind::OpenAi,
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
        ) => transform_json::<
            crate::openai::compact_response::request::OpenAiCompactRequest,
            crate::claude::create_message::request::ClaudeCreateMessageRequest,
        >(&body),
        (
            OperationFamily::Compact,
            ProtocolKind::OpenAi,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        ) => transform_json::<
            crate::openai::compact_response::request::OpenAiCompactRequest,
            crate::gemini::generate_content::request::GeminiGenerateContentRequest,
        >(&body),

        _ => Err(TransformError::new(format!(
            "no request transform for ({}, {}) -> ({}, {})",
            src_operation, src_protocol, dst_operation, dst_protocol
        ))),
    }
    .map(|body| (translated_query, body))
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
) -> Result<Vec<u8>, TransformError> {
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
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
            crate::claude::create_message::response::ClaudeCreateMessageResponse,
        >(&body),
        // OpenAI ChatCompletions response → Claude
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            crate::claude::create_message::response::ClaudeCreateMessageResponse,
        >(&body),
        // OpenAI Response response → Claude
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
            crate::claude::create_message::response::ClaudeCreateMessageResponse,
        >(&body),

        // Claude response → Gemini
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::claude::create_message::response::ClaudeCreateMessageResponse,
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
        >(&body),
        // OpenAI ChatCompletions response → Gemini
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
        >(&body),
        // OpenAI Response response → Gemini
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
        >(&body),

        // Claude response → OpenAI ChatCompletions
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_response_json::<
            crate::claude::create_message::response::ClaudeCreateMessageResponse,
            crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
        >(&body),
        // Gemini response → OpenAI ChatCompletions
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_response_json::<
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
            crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
        >(&body),
        // OpenAI Response response → OpenAI ChatCompletions
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => transform_response_json::<
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
            crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
        >(&body),
        // OpenAI ChatCompletions response → OpenAI Response
        //
        // Mirror of the arm above, used when the client is speaking
        // OpenAI Response but the upstream only returns chat completions
        // (deepseek, groq, nvidia, etc.).
        (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_response_json::<
            crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse,
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
        >(&body),

        // Claude response → OpenAI Response
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_response_json::<
            crate::claude::create_message::response::ClaudeCreateMessageResponse,
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
        >(&body),
        // Gemini response → OpenAI Response
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
        ) => transform_response_json::<
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
        >(&body),

        // =====================================================================
        // count_tokens responses
        // =====================================================================

        // Gemini response → Claude
        (
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
            OperationFamily::CountToken,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::gemini::count_tokens::response::GeminiCountTokensResponse,
            crate::claude::count_tokens::response::ClaudeCountTokensResponse,
        >(&body),
        // OpenAI response → Claude
        (
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
            OperationFamily::CountToken,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::openai::count_tokens::response::OpenAiCountTokensResponse,
            crate::claude::count_tokens::response::ClaudeCountTokensResponse,
        >(&body),

        // Claude response → OpenAI
        (
            OperationFamily::CountToken,
            ProtocolKind::Claude,
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::claude::count_tokens::response::ClaudeCountTokensResponse,
            crate::openai::count_tokens::response::OpenAiCountTokensResponse,
        >(&body),
        // Gemini response → OpenAI
        (
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::gemini::count_tokens::response::GeminiCountTokensResponse,
            crate::openai::count_tokens::response::OpenAiCountTokensResponse,
        >(&body),

        // Claude response → Gemini
        (
            OperationFamily::CountToken,
            ProtocolKind::Claude,
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::claude::count_tokens::response::ClaudeCountTokensResponse,
            crate::gemini::count_tokens::response::GeminiCountTokensResponse,
        >(&body),
        // OpenAI response → Gemini
        (
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::openai::count_tokens::response::OpenAiCountTokensResponse,
            crate::gemini::count_tokens::response::GeminiCountTokensResponse,
        >(&body),

        // =====================================================================
        // model_get responses
        // =====================================================================

        // Gemini response → Claude
        (
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::gemini::model_get::response::GeminiModelGetResponse,
            crate::claude::model_get::response::ClaudeModelGetResponse,
        >(&body),
        // OpenAI response → Claude
        (
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::openai::model_get::response::OpenAiModelGetResponse,
            crate::claude::model_get::response::ClaudeModelGetResponse,
        >(&body),

        // Claude response → OpenAI
        (
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::claude::model_get::response::ClaudeModelGetResponse,
            crate::openai::model_get::response::OpenAiModelGetResponse,
        >(&body),
        // Gemini response → OpenAI
        (
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::gemini::model_get::response::GeminiModelGetResponse,
            crate::openai::model_get::response::OpenAiModelGetResponse,
        >(&body),

        // Claude response → Gemini
        (
            OperationFamily::ModelGet,
            ProtocolKind::Claude,
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::claude::model_get::response::ClaudeModelGetResponse,
            crate::gemini::model_get::response::GeminiModelGetResponse,
        >(&body),
        // OpenAI response → Gemini
        (
            OperationFamily::ModelGet,
            ProtocolKind::OpenAi,
            OperationFamily::ModelGet,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::openai::model_get::response::OpenAiModelGetResponse,
            crate::gemini::model_get::response::GeminiModelGetResponse,
        >(&body),

        // =====================================================================
        // model_list responses
        // =====================================================================

        // Gemini response → Claude
        (
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            OperationFamily::ModelList,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::gemini::model_list::response::GeminiModelListResponse,
            crate::claude::model_list::response::ClaudeModelListResponse,
        >(&body),
        // OpenAI response → Claude
        (
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
            OperationFamily::ModelList,
            ProtocolKind::Claude,
        ) => transform_response_json::<
            crate::openai::model_list::response::OpenAiModelListResponse,
            crate::claude::model_list::response::ClaudeModelListResponse,
        >(&body),

        // Claude response → OpenAI
        (
            OperationFamily::ModelList,
            ProtocolKind::Claude,
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::claude::model_list::response::ClaudeModelListResponse,
            crate::openai::model_list::response::OpenAiModelListResponse,
        >(&body),
        // Gemini response → OpenAI
        (
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::gemini::model_list::response::GeminiModelListResponse,
            crate::openai::model_list::response::OpenAiModelListResponse,
        >(&body),

        // Claude response → Gemini
        (
            OperationFamily::ModelList,
            ProtocolKind::Claude,
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::claude::model_list::response::ClaudeModelListResponse,
            crate::gemini::model_list::response::GeminiModelListResponse,
        >(&body),
        // OpenAI response → Gemini
        (
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::openai::model_list::response::OpenAiModelListResponse,
            crate::gemini::model_list::response::GeminiModelListResponse,
        >(&body),

        // =====================================================================
        // embeddings responses
        // =====================================================================
        (
            OperationFamily::Embedding,
            ProtocolKind::Gemini,
            OperationFamily::Embedding,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::gemini::embeddings::response::GeminiEmbedContentResponse,
            crate::openai::embeddings::response::OpenAiEmbeddingsResponse,
        >(&body),
        (
            OperationFamily::Embedding,
            ProtocolKind::OpenAi,
            OperationFamily::Embedding,
            ProtocolKind::Gemini,
        ) => transform_response_json::<
            crate::openai::embeddings::response::OpenAiEmbeddingsResponse,
            crate::gemini::embeddings::response::GeminiEmbedContentResponse,
        >(&body),

        // =====================================================================
        // create_image responses
        // =====================================================================
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
            crate::openai::create_image::response::OpenAiCreateImageResponse,
        >(&body),

        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
        )
        | (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::CreateImage,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
            crate::openai::create_image::response::OpenAiCreateImageResponse,
        >(&body),

        // =====================================================================
        // create_image_edit responses
        // =====================================================================
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
        )
        | (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
            crate::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
        >(&body),

        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
        )
        | (
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            OperationFamily::CreateImageEdit,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::openai::create_response::response::OpenAiCreateResponseResponse,
            crate::openai::create_image_edit::response::OpenAiCreateImageEditResponse,
        >(&body),

        // =====================================================================
        // compact responses
        // =====================================================================
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::Compact,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::claude::create_message::response::ClaudeCreateMessageResponse,
            crate::openai::compact_response::response::OpenAiCompactResponse,
        >(&body),
        (
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            OperationFamily::Compact,
            ProtocolKind::OpenAi,
        ) => transform_response_json::<
            crate::gemini::generate_content::response::GeminiGenerateContentResponse,
            crate::openai::compact_response::response::OpenAiCompactResponse,
        >(&body),

        _ => Err(TransformError::new(format!(
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
fn transform_json<Src, Dst>(body: &[u8]) -> Result<Vec<u8>, TransformError>
where
    Src: serde::de::DeserializeOwned,
    Dst: TryFrom<Src> + serde::Serialize,
    Dst::Error: std::fmt::Display,
{
    let src: Src = serde_json::from_slice(body)
        .map_err(|e| TransformError::new(format!("request deserialize: {e}")))?;
    let dst = Dst::try_from(src).map_err(|e| TransformError::new(format!("transform: {e}")))?;
    serde_json::to_vec(&dst).map_err(|e| TransformError::new(format!("response serialize: {e}")))
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
fn transform_response_json<Src, Dst>(body: &[u8]) -> Result<Vec<u8>, TransformError>
where
    Src: BodyEnvelope,
    Dst: BodyEnvelope + TryFrom<Src>,
    Dst::Error: std::fmt::Display,
{
    let src = Src::from_body_bytes(body)?;
    let dst = Dst::try_from(src).map_err(|e| TransformError::new(format!("transform: {e}")))?;
    dst.into_body_bytes()
}

fn transform_request_descriptor<Src, Dst>(
    body: &[u8],
    model: Option<&str>,
) -> Result<Vec<u8>, TransformError>
where
    Src: RequestDescriptor,
    Dst: RequestDescriptor + TryFrom<Src>,
    Dst::Error: std::fmt::Display,
{
    let src_body: Src::Body = serde_json::from_slice(body)
        .map_err(|e| TransformError::new(format!("request deserialize: {}", e)))?;
    let src = Src::from_body(src_body).with_model(model);
    let dst = Dst::try_from(src).map_err(|e| TransformError::new(format!("transform: {}", e)))?;

    serde_json::to_vec(&dst.into_body())
        .map_err(|e| TransformError::new(format!("response serialize: {}", e)))
}

fn transform_request_descriptor_ref<Src, Dst>(
    body: &[u8],
    model: Option<&str>,
) -> Result<Vec<u8>, TransformError>
where
    Src: RequestDescriptor,
    for<'a> Dst: RequestDescriptor + TryFrom<&'a Src>,
    for<'a> <Dst as TryFrom<&'a Src>>::Error: std::fmt::Display,
{
    let src_body: Src::Body = serde_json::from_slice(body)
        .map_err(|e| TransformError::new(format!("request deserialize: {}", e)))?;
    let src = Src::from_body(src_body).with_model(model);
    let dst = Dst::try_from(&src).map_err(|e| TransformError::new(format!("transform: {}", e)))?;

    serde_json::to_vec(&dst.into_body())
        .map_err(|e| TransformError::new(format!("response serialize: {}", e)))
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
    pub fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<u8>, TransformError> {
        let mut json_chunks = Vec::new();
        self.decoder.push_chunk(chunk, &mut json_chunks);
        self.process_json_chunks(json_chunks)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, TransformError> {
        let mut json_chunks = Vec::new();
        self.decoder.finish(&mut json_chunks);
        let mut out = self.process_json_chunks(json_chunks)?;
        self.inner.finish(&mut out)?;
        Ok(out)
    }

    fn process_json_chunks(
        &mut self,
        json_chunks: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, TransformError> {
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
    fn on_json_chunk(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> Result<(), TransformError>;
    fn finish(&mut self, out: &mut Vec<u8>) -> Result<(), TransformError>;
}

trait EventConverter<Input, Output>: Send {
    fn on_input(&mut self, input: Input, out: &mut Vec<Output>) -> Result<(), TransformError>;
    fn finish(&mut self, out: &mut Vec<Output>) -> Result<(), TransformError>;
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
    fn on_json_chunk(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> Result<(), TransformError> {
        let input: Input = serde_json::from_slice(chunk)
            .map_err(|e| TransformError::new(format!("stream chunk deserialize failed: {e}")))?;
        let mut events = Vec::new();
        self.converter.on_input(input, &mut events)?;
        self.encoder.encode_events(&events, out)
    }

    fn finish(&mut self, out: &mut Vec<u8>) -> Result<(), TransformError> {
        let mut events = Vec::new();
        self.converter.finish(&mut events)?;
        self.encoder.encode_events(&events, out)?;
        self.encoder.finish(out);
        Ok(())
    }
}

enum StreamChunkDecoder {
    Sse(crate::stream::SseToNdjsonRewriter),
    Ndjson(Vec<u8>),
}

impl StreamChunkDecoder {
    fn from_protocol(protocol: ProtocolKind) -> Result<Self, TransformError> {
        match protocol {
            ProtocolKind::Claude
            | ProtocolKind::OpenAiChatCompletion
            | ProtocolKind::OpenAiResponse
            | ProtocolKind::Gemini => Ok(Self::Sse(crate::stream::SseToNdjsonRewriter::default())),
            ProtocolKind::GeminiNDJson => Ok(Self::Ndjson(Vec::new())),
            _ => Err(TransformError::new(format!(
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
    fn from_protocol(protocol: ProtocolKind) -> Result<Self, TransformError> {
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
            _ => Err(TransformError::new(format!(
                "unsupported stream output protocol: {protocol}"
            ))),
        }
    }

    fn encode_events<T: Serialize>(
        &self,
        events: &[T],
        out: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        for event in events {
            let json = serde_json::to_vec(event)
                .map_err(|e| TransformError::new(format!("stream chunk serialize failed: {e}")))?;
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

use crate::stream::{drain_lines as drain_json_lines, split_lines as split_json_lines};

struct IdentityConverter<T>(PhantomData<T>);

impl<T> Default for IdentityConverter<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send> EventConverter<T, T> for IdentityConverter<T> {
    fn on_input(&mut self, input: T, out: &mut Vec<T>) -> Result<(), TransformError> {
        out.push(input);
        Ok(())
    }

    fn finish(&mut self, _out: &mut Vec<T>) -> Result<(), TransformError> {
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiChatToClaudeConverter(
    crate::transform::claude::stream_generate_content::openai_chat_completions::response::OpenAiChatCompletionsToClaudeStream,
);

impl
    EventConverter<
        crate::openai::create_chat_completions::stream::ChatCompletionChunk,
        crate::claude::create_message::stream::ClaudeStreamEvent,
    > for OpenAiChatToClaudeConverter
{
    fn on_input(
        &mut self,
        input: crate::openai::create_chat_completions::stream::ChatCompletionChunk,
        out: &mut Vec<crate::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct GeminiToClaudeConverter(
    crate::transform::claude::stream_generate_content::gemini::response::GeminiToClaudeStream,
);

impl
    EventConverter<
        crate::gemini::generate_content::response::ResponseBody,
        crate::claude::create_message::stream::ClaudeStreamEvent,
    > for GeminiToClaudeConverter
{
    fn on_input(
        &mut self,
        input: crate::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<crate::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiResponseToClaudeConverter(
    crate::transform::claude::stream_generate_content::openai_response::response::OpenAiResponseToClaudeStream,
);

impl
    EventConverter<
        crate::openai::create_response::stream::ResponseStreamEvent,
        crate::claude::create_message::stream::ClaudeStreamEvent,
    > for OpenAiResponseToClaudeConverter
{
    fn on_input(
        &mut self,
        input: crate::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<crate::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.on_stream_event(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::claude::create_message::stream::ClaudeStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct ClaudeToOpenAiChatConverter(
    crate::transform::openai::stream_generate_content::openai_chat_completions::claude::response::ClaudeToOpenAiChatCompletionsStream,
);

impl
    EventConverter<
        crate::claude::create_message::stream::ClaudeStreamEvent,
        crate::openai::create_chat_completions::stream::ChatCompletionChunk,
    > for ClaudeToOpenAiChatConverter
{
    fn on_input(
        &mut self,
        input: crate::claude::create_message::stream::ClaudeStreamEvent,
        out: &mut Vec<crate::openai::create_chat_completions::stream::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        self.0
            .on_event(input, out)
            .map_err(|e| TransformError::new(format!("stream transform failed: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::openai::create_chat_completions::stream::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct GeminiToOpenAiChatConverter(
    crate::transform::openai::stream_generate_content::openai_chat_completions::gemini::response::GeminiToOpenAiChatCompletionsStream,
);

impl
    EventConverter<
        crate::gemini::generate_content::response::ResponseBody,
        crate::openai::create_chat_completions::stream::ChatCompletionChunk,
    > for GeminiToOpenAiChatConverter
{
    fn on_input(
        &mut self,
        input: crate::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<crate::openai::create_chat_completions::stream::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::openai::create_chat_completions::stream::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct ClaudeToOpenAiResponseConverter(
    crate::transform::openai::stream_generate_content::openai_response::claude::response::ClaudeToOpenAiResponseStream,
);

impl
    EventConverter<
        crate::claude::create_message::stream::ClaudeStreamEvent,
        crate::openai::create_response::stream::ResponseStreamEvent,
    > for ClaudeToOpenAiResponseConverter
{
    fn on_input(
        &mut self,
        input: crate::claude::create_message::stream::ClaudeStreamEvent,
        out: &mut Vec<crate::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0
            .on_event(input, out)
            .map_err(|e| TransformError::new(format!("stream transform failed: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct GeminiToOpenAiResponseConverter(
    crate::transform::openai::stream_generate_content::openai_response::gemini::response::GeminiToOpenAiResponseStream,
);

impl
    EventConverter<
        crate::gemini::generate_content::response::ResponseBody,
        crate::openai::create_response::stream::ResponseStreamEvent,
    > for GeminiToOpenAiResponseConverter
{
    fn on_input(
        &mut self,
        input: crate::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<crate::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct ClaudeToGeminiConverter(
    crate::transform::gemini::stream_generate_content::claude::response::ClaudeToGeminiStream,
);

impl
    EventConverter<
        crate::claude::create_message::stream::ClaudeStreamEvent,
        crate::gemini::generate_content::response::ResponseBody,
    > for ClaudeToGeminiConverter
{
    fn on_input(
        &mut self,
        input: crate::claude::create_message::stream::ClaudeStreamEvent,
        out: &mut Vec<crate::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), TransformError> {
        self.0
            .on_event(input, out)
            .map_err(|e| TransformError::new(format!("stream transform failed: {e}")))
    }

    fn finish(
        &mut self,
        _out: &mut Vec<crate::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), TransformError> {
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiChatToGeminiConverter(
    crate::transform::gemini::stream_generate_content::openai_chat_completions::response::OpenAiChatCompletionsToGeminiStream,
);

impl
    EventConverter<
        crate::openai::create_chat_completions::stream::ChatCompletionChunk,
        crate::gemini::generate_content::response::ResponseBody,
    > for OpenAiChatToGeminiConverter
{
    fn on_input(
        &mut self,
        input: crate::openai::create_chat_completions::stream::ChatCompletionChunk,
        out: &mut Vec<crate::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), TransformError> {
        self.0.on_chunk(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

#[derive(Default)]
struct OpenAiResponseToGeminiConverter(
    crate::transform::gemini::stream_generate_content::openai_response::response::OpenAiResponseToGeminiStream,
);

impl
    EventConverter<
        crate::openai::create_response::stream::ResponseStreamEvent,
        crate::gemini::generate_content::response::ResponseBody,
    > for OpenAiResponseToGeminiConverter
{
    fn on_input(
        &mut self,
        input: crate::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<crate::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), TransformError> {
        self.0.on_stream_event(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::gemini::generate_content::response::ResponseBody>,
    ) -> Result<(), TransformError> {
        self.0.finish(out);
        Ok(())
    }
}

/// Stream converter for `OpenAI Responses stream` → `OpenAI Chat Completions stream`.
///
/// Used by the codex channel which forwards chat-completions traffic as
/// OpenAI Response streams upstream and must reverse the protocol on the
/// way back to the client. The wrapped stream converter lives in
/// `crate::transform::openai::stream_generate_content`.
#[derive(Default)]
struct OpenAiResponseToOpenAiChatCompletionsConverter(
    crate::transform::openai::stream_generate_content::openai_chat_completions::openai_response::response::OpenAiResponseToOpenAiChatCompletionsStream,
);

impl
    EventConverter<
        crate::openai::create_response::stream::ResponseStreamEvent,
        crate::openai::create_chat_completions::stream::ChatCompletionChunk,
    > for OpenAiResponseToOpenAiChatCompletionsConverter
{
    fn on_input(
        &mut self,
        input: crate::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<crate::openai::create_chat_completions::stream::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        self.0
            .on_stream_event(input, out)
            .map_err(|e| TransformError::new(format!("stream convert: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::openai::create_chat_completions::stream::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        self.0
            .finish(out)
            .map_err(|e| TransformError::new(format!("stream finish: {e}")))
    }
}

/// Stream converter for `OpenAI Chat Completions stream` → `OpenAI Responses stream`.
///
/// The reverse of `OpenAiResponseToOpenAiChatCompletionsConverter`,
/// used when clients speak OpenAI Response but the upstream channel
/// only exposes chat completions (deepseek, groq, nvidia, etc.).
#[derive(Default)]
struct OpenAiChatCompletionsToOpenAiResponseConverter(
    crate::transform::openai::stream_generate_content::openai_response::openai_chat_completions::response::OpenAiChatCompletionsToOpenAiResponseStream,
);

impl
    EventConverter<
        crate::openai::create_chat_completions::stream::ChatCompletionChunk,
        crate::openai::create_response::stream::ResponseStreamEvent,
    > for OpenAiChatCompletionsToOpenAiResponseConverter
{
    fn on_input(
        &mut self,
        input: crate::openai::create_chat_completions::stream::ChatCompletionChunk,
        out: &mut Vec<crate::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0
            .on_stream_event(input, out)
            .map_err(|e| TransformError::new(format!("stream convert: {e}")))
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::openai::create_response::stream::ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0
            .finish(out)
            .map_err(|e| TransformError::new(format!("stream finish: {e}")))
    }
}

#[derive(Default)]
struct ResponseStreamToImageStreamConverter(
    crate::transform::openai::create_image::openai_response::stream::ResponseStreamToImageStream,
);

impl
    EventConverter<
        crate::openai::create_response::stream::ResponseStreamEvent,
        crate::openai::create_image::stream::ImageGenerationStreamEvent,
    > for ResponseStreamToImageStreamConverter
{
    fn on_input(
        &mut self,
        input: crate::openai::create_response::stream::ResponseStreamEvent,
        out: &mut Vec<crate::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), TransformError> {
        self.0.on_event(input, out);
        Ok(())
    }

    fn finish(
        &mut self,
        out: &mut Vec<crate::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), TransformError> {
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
        crate::gemini::generate_content::response::ResponseBody,
        crate::openai::create_image::stream::ImageGenerationStreamEvent,
    > for GeminiToImageStreamConverter
{
    fn on_input(
        &mut self,
        input: crate::gemini::generate_content::response::ResponseBody,
        out: &mut Vec<crate::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), TransformError> {
        use crate::openai::create_image::stream::ImageGenerationStreamEvent;
        use crate::transform::openai::create_image::gemini::utils::{
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
                    background: crate::openai::create_image::types::OpenAiImageBackground::Auto,
                    created_at: 0,
                    output_format: img.output_format.clone(),
                    quality: crate::openai::create_image::types::OpenAiImageQuality::Auto,
                    size: crate::openai::create_image::types::OpenAiImageSize::Auto,
                    usage: best_effort_openai_image_usage_from_gemini(usage_metadata),
                });
            } else {
                let index = self.partial_count;
                self.partial_count += 1;
                out.push(ImageGenerationStreamEvent::PartialImage {
                    b64_json: img.b64_json.clone(),
                    background: crate::openai::create_image::types::OpenAiImageBackground::Auto,
                    created_at: 0,
                    output_format: img.output_format.clone(),
                    partial_image_index: index,
                    quality: crate::openai::create_image::types::OpenAiImageQuality::Auto,
                    size: crate::openai::create_image::types::OpenAiImageSize::Auto,
                });
            }
        }
        Ok(())
    }

    fn finish(
        &mut self,
        _out: &mut Vec<crate::openai::create_image::stream::ImageGenerationStreamEvent>,
    ) -> Result<(), TransformError> {
        Ok(())
    }
}

fn build_stream_transform<Input, Output, Converter>(
    src_protocol: ProtocolKind,
    dst_protocol: ProtocolKind,
    converter: Converter,
    normalizer: Option<StreamChunkNormalizer>,
) -> Result<StreamResponseTransformer, TransformError>
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
) -> Result<StreamResponseTransformer, TransformError> {
    let key = (src_operation, src_protocol, dst_operation, dst_protocol);

    match key {
        (
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
        ) => build_stream_transform::<
            crate::claude::create_message::stream::ClaudeStreamEvent,
            crate::claude::create_message::stream::ClaudeStreamEvent,
            IdentityConverter<crate::claude::create_message::stream::ClaudeStreamEvent>,
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
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
            IdentityConverter<crate::openai::create_chat_completions::stream::ChatCompletionChunk>,
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
            crate::openai::create_response::stream::ResponseStreamEvent,
            crate::openai::create_response::stream::ResponseStreamEvent,
            IdentityConverter<crate::openai::create_response::stream::ResponseStreamEvent>,
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
            crate::gemini::generate_content::response::ResponseBody,
            crate::gemini::generate_content::response::ResponseBody,
            IdentityConverter<crate::gemini::generate_content::response::ResponseBody>,
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
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
            crate::claude::create_message::stream::ClaudeStreamEvent,
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
            crate::openai::create_response::stream::ResponseStreamEvent,
            crate::claude::create_message::stream::ClaudeStreamEvent,
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
            crate::gemini::generate_content::response::ResponseBody,
            crate::claude::create_message::stream::ClaudeStreamEvent,
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
            crate::claude::create_message::stream::ClaudeStreamEvent,
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
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
            crate::gemini::generate_content::response::ResponseBody,
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
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
            crate::openai::create_response::stream::ResponseStreamEvent,
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
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
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
            crate::openai::create_response::stream::ResponseStreamEvent,
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
            crate::claude::create_message::stream::ClaudeStreamEvent,
            crate::openai::create_response::stream::ResponseStreamEvent,
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
            crate::gemini::generate_content::response::ResponseBody,
            crate::openai::create_response::stream::ResponseStreamEvent,
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
            crate::claude::create_message::stream::ClaudeStreamEvent,
            crate::gemini::generate_content::response::ResponseBody,
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
            crate::openai::create_chat_completions::stream::ChatCompletionChunk,
            crate::gemini::generate_content::response::ResponseBody,
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
            crate::openai::create_response::stream::ResponseStreamEvent,
            crate::gemini::generate_content::response::ResponseBody,
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
            crate::openai::create_response::stream::ResponseStreamEvent,
            crate::openai::create_image::stream::ImageGenerationStreamEvent,
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
            crate::gemini::generate_content::response::ResponseBody,
            crate::openai::create_image::stream::ImageGenerationStreamEvent,
            GeminiToImageStreamConverter,
        >(
            src_protocol,
            dst_protocol,
            GeminiToImageStreamConverter::default(),
            normalizer,
        ),

        _ => Err(TransformError::new(format!(
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
) -> Result<(), TransformError> {
    match protocol {
        ProtocolKind::Claude => {
            use crate::claude::create_message::response::ClaudeCreateMessageResponse;
            use crate::claude::create_message::stream::ClaudeStreamEvent;
            use crate::transform::claude::nonstream_to_stream::nonstream_to_stream;

            let response: ClaudeCreateMessageResponse = serde_json::from_slice(body)
                .map_err(|e| TransformError::new(format!("deserialize: {e}")))?;

            let mut events: Vec<ClaudeStreamEvent> = Vec::new();
            nonstream_to_stream(response, &mut events)
                .map_err(|e| TransformError::new(format!("nonstream_to_stream: {e}")))?;

            for event in &events {
                let json = serde_json::to_vec(event)
                    .map_err(|e| TransformError::new(format!("serialize event: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        ProtocolKind::OpenAiChatCompletion => {
            use crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse;
            use crate::openai::create_chat_completions::stream::ChatCompletionChunk;

            let response: OpenAiChatCompletionsResponse = serde_json::from_slice(body)
                .map_err(|e| TransformError::new(format!("deserialize: {e}")))?;

            let chunks = Vec::<ChatCompletionChunk>::try_from(response)
                .map_err(|e| TransformError::new(format!("nonstream_to_stream: {e}")))?;

            for chunk in &chunks {
                let json = serde_json::to_vec(chunk)
                    .map_err(|e| TransformError::new(format!("serialize chunk: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        ProtocolKind::OpenAiResponse => {
            use crate::openai::create_response::response::OpenAiCreateResponseResponse;
            use crate::openai::create_response::stream::ResponseStreamEvent;

            let response: OpenAiCreateResponseResponse = serde_json::from_slice(body)
                .map_err(|e| TransformError::new(format!("deserialize: {e}")))?;

            let events = Vec::<ResponseStreamEvent>::try_from(response)
                .map_err(|e| TransformError::new(format!("nonstream_to_stream: {e}")))?;

            for event in &events {
                let json = serde_json::to_vec(event)
                    .map_err(|e| TransformError::new(format!("serialize event: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        ProtocolKind::Gemini => {
            use crate::gemini::generate_content::response::GeminiGenerateContentResponse;

            let response: GeminiGenerateContentResponse = serde_json::from_slice(body)
                .map_err(|e| TransformError::new(format!("deserialize: {e}")))?;

            // Gemini non-stream and stream share the same chunk body shape
            if let GeminiGenerateContentResponse::Success { body: resp, .. } = response {
                let json = serde_json::to_vec(&resp)
                    .map_err(|e| TransformError::new(format!("serialize chunk: {e}")))?;
                out.extend_from_slice(&json);
                out.push(b'\n');
            }
            Ok(())
        }
        _ => Err(TransformError::new(format!(
            "no nonstream_to_stream for protocol: {protocol}"
        ))),
    }
}

/// Convert stream events (NDJSON lines) to a non-streaming response (same protocol).
pub fn stream_to_nonstream(
    protocol: ProtocolKind,
    chunks: &[&[u8]],
) -> Result<Vec<u8>, TransformError> {
    match protocol {
        ProtocolKind::Claude => {
            use crate::claude::create_message::response::ClaudeCreateMessageResponse;
            use crate::claude::create_message::stream::ClaudeStreamEvent;

            let events: Vec<ClaudeStreamEvent> = chunks
                .iter()
                .map(|c| serde_json::from_slice(c))
                .collect::<Result<_, _>>()
                .map_err(|e| TransformError::new(format!("deserialize events: {e}")))?;

            let response = ClaudeCreateMessageResponse::try_from(events)
                .map_err(|e| TransformError::new(format!("stream_to_nonstream: {e}")))?;

            // Emit only the inner body — callers of `stream_to_nonstream`
            // expect raw HTTP body shape, not the internal
            // `{stats_code, headers, body}` wrapper envelope.
            response.into_body_bytes()
        }
        ProtocolKind::OpenAiChatCompletion => {
            use crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse;
            use crate::openai::create_chat_completions::stream::ChatCompletionChunk;

            let chunks_parsed: Vec<ChatCompletionChunk> = chunks
                .iter()
                .map(|c| serde_json::from_slice(c))
                .collect::<Result<_, _>>()
                .map_err(|e| TransformError::new(format!("deserialize chunks: {e}")))?;

            let response = OpenAiChatCompletionsResponse::try_from(chunks_parsed)
                .map_err(|e| TransformError::new(format!("stream_to_nonstream: {e}")))?;

            response.into_body_bytes()
        }
        ProtocolKind::OpenAiResponse => {
            use crate::openai::create_response::response::OpenAiCreateResponseResponse;
            use crate::openai::create_response::stream::ResponseStreamEvent;

            let events: Vec<ResponseStreamEvent> = chunks
                .iter()
                .map(|c| serde_json::from_slice(c))
                .collect::<Result<_, _>>()
                .map_err(|e| TransformError::new(format!("deserialize events: {e}")))?;

            let response = OpenAiCreateResponseResponse::try_from(events)
                .map_err(|e| TransformError::new(format!("stream_to_nonstream: {e}")))?;

            response.into_body_bytes()
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            use crate::gemini::generate_content::response::ResponseBody;
            use crate::gemini::generate_content::types::GeminiCandidate;
            use std::collections::BTreeMap;

            let mut merged = ResponseBody::default();
            let mut candidate_map: BTreeMap<u32, GeminiCandidate> = BTreeMap::new();

            for chunk in chunks {
                let body: ResponseBody = serde_json::from_slice(chunk)
                    .map_err(|e| TransformError::new(format!("deserialize chunk: {e}")))?;
                crate::transform::gemini::stream_to_nonstream::merge_chunk(
                    &mut merged,
                    &mut candidate_map,
                    body,
                );
            }

            let body =
                crate::transform::gemini::stream_to_nonstream::finalize_body(merged, candidate_map);

            serde_json::to_vec(&body).map_err(|e| TransformError::new(format!("serialize: {e}")))
        }
        _ => Err(TransformError::new(format!(
            "no stream_to_nonstream for protocol: {protocol}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use crate::kinds::{OperationFamily, ProtocolKind};
    use serde_json::{Value, json};

    use super::{
        convert_error_body_or_raw, transform_request, transform_response, translate_request_query,
    };

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

        let (_qout, transformed) = transform_request(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiResponse,
            None,
            None,
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

        let (_qout, transformed) = transform_request(
            OperationFamily::CountToken,
            ProtocolKind::Claude,
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
            None,
            None,
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
    /// `crate::*::response` have the envelope shape for internal
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
        let claude_error =
            br#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#
                .to_vec();

        let result = convert_error_body_or_raw(
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Claude,
            claude_error.clone(),
        );

        assert_eq!(result, claude_error);
    }

    #[test]
    fn translate_request_query_gemini_to_claude_model_list() {
        let translated = translate_request_query(
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            OperationFamily::ModelList,
            ProtocolKind::Claude,
            Some("pageSize=25&pageToken=abc"),
        )
        .expect("translated query should be present");
        assert!(translated.contains("limit=25"), "got: {translated}");
        assert!(translated.contains("after_id=abc"), "got: {translated}");
    }

    #[test]
    fn translate_request_query_claude_to_gemini_model_list() {
        let translated = translate_request_query(
            OperationFamily::ModelList,
            ProtocolKind::Claude,
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            Some("limit=50&after_id=cursor1"),
        )
        .expect("translated query should be present");
        assert!(translated.contains("pageSize=50"), "got: {translated}");
        assert!(
            translated.contains("pageToken=cursor1"),
            "got: {translated}"
        );
    }

    #[test]
    fn translate_request_query_passes_through_for_same_protocol() {
        let translated = translate_request_query(
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            Some("pageSize=10"),
        );
        assert_eq!(translated.as_deref(), Some("pageSize=10"));
    }

    #[test]
    fn translate_request_query_passes_through_for_non_model_list() {
        let translated = translate_request_query(
            OperationFamily::GenerateContent,
            ProtocolKind::Claude,
            OperationFamily::GenerateContent,
            ProtocolKind::Gemini,
            Some("foo=bar"),
        );
        assert_eq!(translated.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn translate_request_query_drops_unknown_keys_on_cross_protocol_model_list() {
        let translated = translate_request_query(
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
            OperationFamily::ModelList,
            ProtocolKind::Claude,
            Some("pageSize=5&unknown=x"),
        )
        .expect("translated query");
        assert!(translated.contains("limit=5"));
        assert!(!translated.contains("unknown"));
    }
}
