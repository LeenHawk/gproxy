use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
use crate::count_tokens::CountStrategy;
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// OpenRouter API channel (key authentication only).
pub struct OpenRouterChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenRouterSettings {
    #[serde(default = "default_openrouter_base_url")]
    pub base_url: String,
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_openrouter_base_url() -> String {
    "https://openrouter.ai/api".to_string()
}

fn openrouter_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/openrouter.json"))
    })
}

impl ChannelSettings for OpenRouterSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenRouterCredential {
    pub api_key: String,
}

impl ChannelCredential for OpenRouterCredential {}

impl Channel for OpenRouterChannel {
    const ID: &'static str = "openrouter";
    type Settings = OpenRouterSettings;
    type Credential = OpenRouterCredential;
    type Health = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        let mut t = RoutingTable::new();

        let pass = |op: OperationFamily, proto: ProtocolKind| {
            (RouteKey::new(op, proto), RouteImplementation::Passthrough)
        };
        let xform = |op: OperationFamily,
                     proto: ProtocolKind,
                     dst_op: OperationFamily,
                     dst_proto: ProtocolKind| {
            (
                RouteKey::new(op, proto),
                RouteImplementation::TransformTo {
                    destination: RouteKey::new(dst_op, dst_proto),
                },
            )
        };

        let routes: Vec<(RouteKey, RouteImplementation)> = vec![
            // === Model list/get ===
            pass(OperationFamily::ModelList, ProtocolKind::OpenAi),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Claude,
                OperationFamily::ModelList,
                ProtocolKind::OpenAi,
            ),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
                OperationFamily::ModelList,
                ProtocolKind::OpenAi,
            ),
            pass(OperationFamily::ModelGet, ProtocolKind::OpenAi),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
                OperationFamily::ModelGet,
                ProtocolKind::OpenAi,
            ),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::Gemini,
                OperationFamily::ModelGet,
                ProtocolKind::OpenAi,
            ),
            (
                RouteKey::new(OperationFamily::CountToken, ProtocolKind::OpenAi),
                RouteImplementation::Local,
            ),
            (
                RouteKey::new(OperationFamily::CountToken, ProtocolKind::Claude),
                RouteImplementation::Local,
            ),
            (
                RouteKey::new(OperationFamily::CountToken, ProtocolKind::Gemini),
                RouteImplementation::Local,
            ),
            // === Generate content (non-stream) ===
            pass(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            pass(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(OperationFamily::GenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // === Generate content (stream) ===
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(OperationFamily::StreamGenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::GeminiNDJson,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // === Live API ===
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // === WebSocket -> stream ===
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // === Compact -> generate ===
            xform(
                OperationFamily::Compact,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // === Embeddings ===
            pass(OperationFamily::Embedding, ProtocolKind::OpenAi),
            xform(
                OperationFamily::Embedding,
                ProtocolKind::Gemini,
                OperationFamily::Embedding,
                ProtocolKind::OpenAi,
            ),
        ];

        for (key, implementation) in routes {
            t.set(key, implementation);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        openrouter_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let url = format!(
            "{}{}",
            settings.base_url(),
            openrouter_request_path(request)?
        );
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header("Authorization", format!("Bearer {}", credential.api_key))
            .header("Content-Type", "application/json");

        if let Some(ua) = settings.user_agent() {
            builder = builder.header("User-Agent", ua);
        }

        for (key, value) in request.headers.iter() {
            builder = builder.header(key, value);
        }
        crate::utils::http_headers::replace_header(
            &mut builder,
            "Authorization",
            format!("Bearer {}", credential.api_key),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "Content-Type",
            "application/json",
        )?;
        if let Some(ua) = settings.user_agent() {
            crate::utils::http_headers::replace_header(&mut builder, "User-Agent", ua)?;
        }

        builder
            .body(request.body.clone())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        let body = reshape_openrouter_error(&body);
        match request.route.operation {
            OperationFamily::ModelList => reshape_openrouter_model_list(&body),
            OperationFamily::ModelGet => reshape_openrouter_model_get(&body),
            _ => body,
        }
    }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        _body: &[u8],
    ) -> ResponseClassification {
        match status {
            200..=299 => ResponseClassification::Success,
            401 | 403 => ResponseClassification::AuthDead,
            402 => ResponseClassification::AuthDead, // insufficient credits
            429 => {
                let retry_after = headers
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|secs| secs * 1000);
                ResponseClassification::RateLimited {
                    retry_after_ms: retry_after,
                }
            }
            408 | 500..=599 => ResponseClassification::TransientError,
            _ => ResponseClassification::PermanentError,
        }
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::Local
    }

    fn handle_local(
        &self,
        operation: OperationFamily,
        protocol: ProtocolKind,
        body: &[u8],
    ) -> Option<Result<Vec<u8>, UpstreamError>> {
        (operation == OperationFamily::CountToken)
            .then(|| crate::count_tokens::local_count_response_for_protocol(protocol, body))
    }
}

fn openrouter_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1/models".to_string()),
        OperationFamily::ModelGet => Ok(format!(
            "/v1/models/{}",
            request.model.as_deref().unwrap_or_default()
        )),
        OperationFamily::CountToken => Ok("/v1/responses/input_tokens/count".to_string()),
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            match request.route.protocol {
                ProtocolKind::OpenAiResponse => Ok("/v1/responses".to_string()),
                ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAi => {
                    Ok("/v1/chat/completions".to_string())
                }
                ProtocolKind::Claude => Ok("/v1/messages".to_string()),
                _ => Err(UpstreamError::Channel(format!(
                    "unsupported openrouter request route: ({}, {})",
                    request.route.operation, request.route.protocol
                ))),
            }
        }
        OperationFamily::Embedding => Ok("/v1/embeddings".to_string()),
        OperationFamily::OpenAiResponseWebSocket => Ok("/v1/responses".to_string()),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported openrouter request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn openrouter_routing_table() -> RoutingTable {
    OpenRouterChannel.routing_table()
}

inventory::submit! { ChannelRegistration::new(OpenRouterChannel::ID, openrouter_routing_table) }

/// OpenRouter `/v1/models` returns `{"data": [...]}` without the OpenAI-required
/// `object: "list"` wrapper field, and each item omits `object: "model"` and
/// `owned_by`. Without these fields `OpenAiModelListResponse` deserialization
/// fails. Fill them in; leave OR's extra fields alone (serde ignores unknowns).
fn reshape_openrouter_model_list(body: &[u8]) -> Vec<u8> {
    let Ok(mut v): Result<Value, _> = serde_json::from_slice(body) else {
        return body.to_vec();
    };
    let Some(obj) = v.as_object_mut() else {
        return body.to_vec();
    };
    if obj.contains_key("error") {
        return body.to_vec();
    }
    obj.entry("object").or_insert_with(|| Value::from("list"));
    if let Some(arr) = obj.get_mut("data").and_then(Value::as_array_mut) {
        for item in arr {
            fill_openai_model_fields(item);
        }
    }
    serde_json::to_vec(&v).unwrap_or_else(|_| body.to_vec())
}

fn reshape_openrouter_model_get(body: &[u8]) -> Vec<u8> {
    let Ok(mut v): Result<Value, _> = serde_json::from_slice(body) else {
        return body.to_vec();
    };
    if v.as_object().is_none_or(|o| o.contains_key("error")) {
        return body.to_vec();
    }
    if let Some(data) = v.get_mut("data") {
        fill_openai_model_fields(data);
    } else {
        fill_openai_model_fields(&mut v);
    }
    serde_json::to_vec(&v).unwrap_or_else(|_| body.to_vec())
}

fn fill_openai_model_fields(item: &mut Value) {
    let Some(obj) = item.as_object_mut() else {
        return;
    };
    obj.entry("object").or_insert_with(|| Value::from("model"));
    if !obj.contains_key("owned_by") {
        let owner = obj
            .get("id")
            .and_then(Value::as_str)
            .and_then(|s| s.split_once('/'))
            .map(|(org, _)| org.to_string())
            .unwrap_or_else(|| "openrouter".to_string());
        obj.insert("owned_by".to_string(), Value::from(owner));
    }
}

/// OpenRouter error shape: `{error: {code: int, message: str, metadata?}, user_id?}`.
/// OpenAI's `OpenAiApiError` requires `message` + `type` as strings and an
/// optional string `code`. Coerce `code` to string and synthesize `type` from
/// the numeric code so downstream transforms deserialize cleanly. No-op for
/// non-error bodies.
fn reshape_openrouter_error(body: &[u8]) -> Vec<u8> {
    let Ok(mut v): Result<Value, _> = serde_json::from_slice(body) else {
        return body.to_vec();
    };
    let Some(err) = v.get_mut("error").and_then(Value::as_object_mut) else {
        return body.to_vec();
    };
    let code_num = err.get("code").and_then(Value::as_i64);
    if let Some(code) = code_num {
        err.insert("code".to_string(), Value::from(code.to_string()));
    }
    err.entry("type").or_insert_with(|| {
        Value::from(match code_num.unwrap_or(0) {
            400 => "invalid_request_error",
            401 => "authentication_error",
            402 => "insufficient_quota",
            403 => "permission_error",
            404 => "not_found_error",
            408 => "timeout_error",
            429 => "rate_limit_error",
            500..=599 => "api_error",
            _ => "api_error",
        })
    });
    serde_json::to_vec(&v).unwrap_or_else(|_| body.to_vec())
}
