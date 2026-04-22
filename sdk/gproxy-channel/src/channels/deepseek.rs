use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
use crate::count_tokens::CountStrategy;
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// DeepSeek API channel.
///
/// Supports three upstream surfaces:
/// - `/v1/chat/completions` for OpenAI-compatible chat (+ transforms from
///   Gemini / OpenAiResponse).
/// - `/v1/models` and `/v1/models/{model}` for model discovery.
/// - `/anthropic/v1/messages` for native Claude protocol passthrough (the
///   endpoint DeepSeek exposes for Anthropic API compatibility).
///
/// The Claude path uses `x-api-key` auth instead of `Bearer`, and the
/// OpenAI chat path strips a set of request fields DeepSeek rejects
/// (see `DEEPSEEK_UNSUPPORTED_CHAT_FIELDS`).
pub struct DeepSeekChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekSettings {
    #[serde(default = "default_deepseek_base_url")]
    pub base_url: String,
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_deepseek_base_url() -> String {
    "https://api.deepseek.com".to_string()
}

fn deepseek_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/deepseek.json"))
    })
}

impl ChannelSettings for DeepSeekSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekCredential {
    pub api_key: String,
}

impl ChannelCredential for DeepSeekCredential {}

/// OpenAI chat fields DeepSeek rejects. Mirrors the sample gproxy list
/// so we strip them consistently across the `v1/chat/completions`
/// entry points.
const DEEPSEEK_UNSUPPORTED_CHAT_FIELDS: &[&str] = &[
    "audio",
    "function_call",
    "functions",
    "logit_bias",
    "max_completion_tokens",
    "metadata",
    "modalities",
    "n",
    "parallel_tool_calls",
    "prediction",
    "prompt_cache_key",
    "prompt_cache_retention",
    "reasoning_effort",
    "safety_identifier",
    "seed",
    "service_tier",
    "store",
    "user",
    "verbosity",
    "web_search_options",
];

impl Channel for DeepSeekChannel {
    const ID: &'static str = "deepseek";
    type Settings = DeepSeekSettings;
    type Credential = DeepSeekCredential;
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
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(OperationFamily::GenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            // === Generate content (stream) ===
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(OperationFamily::StreamGenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::GeminiNDJson,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            // === Live API ===
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            // === WebSocket → stream ===
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            // === Compact → generate ===
            xform(
                OperationFamily::Compact,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
        ];

        for (key, implementation) in routes {
            t.set(key, implementation);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        deepseek_model_pricing()
    }

    fn finalize_request(
        &self,
        _settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        // Only the `v1/chat/completions` paths need body normalization —
        // the Claude passthrough hits `/anthropic/v1/messages` which
        // accepts Claude's native schema unchanged, and model list/get
        // have empty bodies.
        if !matches!(
            request.route.operation,
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent
        ) || !matches!(
            request.route.protocol,
            ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAi
        ) {
            return Ok(request);
        }

        let Ok(mut body_json) = serde_json::from_slice::<Value>(&request.body) else {
            return Ok(request);
        };
        normalize_deepseek_chat_request_body(&mut body_json);
        request.body = serde_json::to_vec(&body_json)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(request)
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        // Only the OpenAI chat-completions responses need fixing up —
        // the Claude passthrough returns native Anthropic shape which
        // our Claude transform already understands, and model list/get
        // are fine as-is.
        if !matches!(
            request.route.operation,
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent
        ) || !matches!(
            request.route.protocol,
            ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAi
        ) {
            return body;
        }
        normalize_deepseek_chat_response_body(&body).unwrap_or(body)
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let path = deepseek_request_path(request)?;
        let mut url = format!("{}{}", settings.base_url(), path);
        crate::utils::url::append_query(&mut url, request.query.as_deref());

        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header("Content-Type", "application/json");

        // DeepSeek's Anthropic compatibility endpoint wants `x-api-key`
        // just like the real Anthropic API; everything else on this
        // host uses the standard Bearer scheme.
        if request.route.protocol == ProtocolKind::Claude {
            builder = builder.header("x-api-key", &credential.api_key);
        } else {
            builder = builder.header("Authorization", format!("Bearer {}", credential.api_key));
        }

        if let Some(ua) = settings.user_agent() {
            builder = builder.header("User-Agent", ua);
        }

        // request.headers pipes through client-supplied anthropic-version /
        // anthropic-beta for the Claude path, and any caller-provided
        // tracing headers for the OpenAI paths.
        for (key, value) in request.headers.iter() {
            builder = builder.header(key, value);
        }
        crate::utils::http_headers::replace_header(
            &mut builder,
            "Content-Type",
            "application/json",
        )?;
        if request.route.protocol == ProtocolKind::Claude {
            crate::utils::http_headers::replace_header(
                &mut builder,
                "x-api-key",
                &credential.api_key,
            )?;
        } else {
            crate::utils::http_headers::replace_header(
                &mut builder,
                "Authorization",
                format!("Bearer {}", credential.api_key),
            )?;
        }
        if let Some(ua) = settings.user_agent() {
            crate::utils::http_headers::replace_header(&mut builder, "User-Agent", ua)?;
        }

        builder
            .body(request.body.clone())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
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
            500..=599 => ResponseClassification::TransientError,
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
        _model: Option<&str>,
        _query: Option<&str>,
        body: &[u8],
    ) -> Option<Result<Vec<u8>, UpstreamError>> {
        (operation == OperationFamily::CountToken)
            .then(|| crate::count_tokens::local_count_response_for_protocol(protocol, body))
    }
}

fn deepseek_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1/models".to_string()),
        OperationFamily::ModelGet => Ok(format!(
            "/v1/models/{}",
            request.model.as_deref().unwrap_or_default()
        )),
        OperationFamily::CountToken => Ok("/v1/responses/input_tokens/count".to_string()),
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            match request.route.protocol {
                // DeepSeek exposes an Anthropic-compatible endpoint under
                // `/anthropic/v1/messages`; the Claude passthrough routes
                // reach this path directly.
                ProtocolKind::Claude => Ok("/anthropic/v1/messages".to_string()),
                ProtocolKind::OpenAiResponse => Ok("/v1/responses".to_string()),
                ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAi => {
                    Ok("/v1/chat/completions".to_string())
                }
                _ => Err(UpstreamError::Channel(format!(
                    "unsupported deepseek request route: ({}, {})",
                    request.route.operation, request.route.protocol
                ))),
            }
        }
        OperationFamily::Embedding => Ok("/v1/embeddings".to_string()),
        OperationFamily::OpenAiResponseWebSocket => Ok("/v1/responses".to_string()),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported deepseek request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

/// Normalize an outbound `/v1/chat/completions` request body so DeepSeek
/// accepts it. Ported from the sample gproxy upstream module.
///
/// Changes:
/// - Fold `extra_body.thinking` into top-level `thinking` (DeepSeek only
///   understands `enabled` / `disabled`; `adaptive` maps to `enabled`).
/// - Rename `max_completion_tokens` → `max_tokens` if `max_tokens` isn't
///   already set, and cap both at 8192 (DeepSeek's ceiling).
/// - Strip unsupported OpenAI chat fields.
/// - Rewrite `developer` role messages as `system`.
/// - Normalize tool definitions: drop non-`function` tools, re-emit each
///   kept one with just `{type, function}`.
/// - Normalize `tool_choice`: strings pass through when they're one of
///   `none` / `auto` / `required`; explicit function picks are re-emitted
///   in canonical `{type: "function", function: {name}}` shape. When no
///   tools remain after normalization, force `tool_choice` to `"none"`.
fn normalize_deepseek_chat_request_body(body_json: &mut Value) {
    let Some(map) = body_json.as_object_mut() else {
        return;
    };

    normalize_deepseek_chat_extra_body(map);

    if let Some(max_tokens) = map.get("max_tokens").and_then(Value::as_u64) {
        map.insert("max_tokens".to_string(), Value::from(max_tokens.min(8192)));
    }
    if let Some(max_completion_tokens) = map.get("max_completion_tokens").and_then(Value::as_u64) {
        let capped = max_completion_tokens.min(8192);
        map.insert("max_completion_tokens".to_string(), Value::from(capped));
    }
    if map.get("max_tokens").is_none()
        && let Some(max_completion_tokens) = map.remove("max_completion_tokens")
    {
        map.insert("max_tokens".to_string(), max_completion_tokens);
    }

    for field in DEEPSEEK_UNSUPPORTED_CHAT_FIELDS {
        map.remove(*field);
    }

    normalize_deepseek_chat_message_roles(map);
    normalize_deepseek_chat_tools(map);
}

fn normalize_deepseek_chat_extra_body(map: &mut Map<String, Value>) {
    let Some(extra_body) = map.remove("extra_body") else {
        return;
    };
    if map.contains_key("thinking") {
        return;
    }
    if let Some(thinking) = deepseek_thinking_from_extra_body(&extra_body) {
        map.insert("thinking".to_string(), thinking);
    }
}

fn deepseek_thinking_from_extra_body(extra_body: &Value) -> Option<Value> {
    let object = extra_body.as_object()?;
    if let Some(value) = object
        .get("thinking")
        .and_then(normalize_deepseek_thinking_value)
    {
        return Some(value);
    }
    object
        .get("extra_body")
        .and_then(deepseek_thinking_from_extra_body)
}

fn normalize_deepseek_thinking_value(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let mode = object.get("type")?.as_str()?;
    let normalized_type = match mode {
        "enabled" => "enabled",
        "disabled" => "disabled",
        // Claude/GProxy-compatible extension; DeepSeek only supports
        // enabled/disabled, so adaptive collapses to enabled.
        "adaptive" => "enabled",
        _ => return None,
    };
    Some(serde_json::json!({ "type": normalized_type }))
}

fn normalize_deepseek_chat_message_roles(map: &mut Map<String, Value>) {
    let Some(messages) = map.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        if let Some(object) = message.as_object_mut() {
            let is_developer = object
                .get("role")
                .and_then(Value::as_str)
                .is_some_and(|role| role.eq_ignore_ascii_case("developer"));
            if is_developer {
                object.insert("role".to_string(), Value::String("system".to_string()));
            }
        }
    }
}

fn normalize_deepseek_chat_tools(map: &mut Map<String, Value>) {
    if let Some(tools_value) = map.remove("tools") {
        let mut normalized_tools = Vec::new();
        if let Value::Array(tools) = tools_value {
            for tool in tools {
                if let Some(normalized) = normalize_deepseek_chat_tool(tool) {
                    normalized_tools.push(normalized);
                }
            }
        }
        if !normalized_tools.is_empty() {
            map.insert("tools".to_string(), Value::Array(normalized_tools));
        }
    }

    if let Some(tool_choice) = map.remove("tool_choice")
        && let Some(normalized) = normalize_deepseek_chat_tool_choice(tool_choice)
    {
        let has_tools = map
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty());
        let normalized = if has_tools || normalized == Value::String("none".to_string()) {
            normalized
        } else {
            Value::String("none".to_string())
        };
        map.insert("tool_choice".to_string(), normalized);
    }
}

fn normalize_deepseek_chat_tool(tool: Value) -> Option<Value> {
    let mut tool = tool.as_object()?.clone();
    let type_value = tool.remove("type")?.as_str()?.to_string();
    if type_value != "function" {
        return None;
    }
    let function = tool.remove("function")?.as_object()?.clone();
    Some(Value::Object(
        [
            ("type".to_string(), Value::String("function".to_string())),
            ("function".to_string(), Value::Object(function)),
        ]
        .into_iter()
        .collect(),
    ))
}

fn normalize_deepseek_chat_tool_choice(choice: Value) -> Option<Value> {
    match choice {
        Value::String(mode) => match mode.as_str() {
            "none" | "auto" | "required" => Some(Value::String(mode)),
            _ => None,
        },
        Value::Object(mut object) => {
            let type_value = object.remove("type")?.as_str()?.to_string();
            if type_value != "function" {
                return None;
            }
            let function = object.remove("function")?.as_object()?.clone();
            let name = function.get("name")?.as_str()?.to_string();
            Some(serde_json::json!({
                "type": "function",
                "function": { "name": name }
            }))
        }
        _ => None,
    }
}

/// Normalize a DeepSeek `/v1/chat/completions` response body into the
/// shape downstream OpenAI consumers expect. Ported from sample gproxy.
///
/// - Rewrite the nonstandard `finish_reason: "insufficient_system_resource"`
///   as `"length"` so clients don't need a DeepSeek-specific branch.
/// - Mirror `usage.prompt_cache_hit_tokens` into
///   `usage.prompt_tokens_details.cached_tokens` to match the canonical
///   OpenAI cache bookkeeping shape.
///
/// Returns the rewritten body only when something actually changed —
/// otherwise returns None so the caller keeps the original bytes (saves
/// a needless re-serialize).
fn normalize_deepseek_chat_response_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut value = serde_json::from_slice::<Value>(body).ok()?;
    let map = value.as_object_mut()?;
    let mut changed = false;

    if let Some(choices) = map.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices {
            if let Some(choice_map) = choice.as_object_mut()
                && let Some(reason) = choice_map.get_mut("finish_reason")
                && reason.as_str() == Some("insufficient_system_resource")
            {
                *reason = Value::String("length".to_string());
                changed = true;
            }
        }
    }

    if let Some(usage) = map.get_mut("usage").and_then(Value::as_object_mut)
        && let Some(cache_hit_tokens) = usage.get("prompt_cache_hit_tokens").and_then(Value::as_u64)
    {
        let details_value = usage
            .entry("prompt_tokens_details".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !details_value.is_object() {
            *details_value = Value::Object(Map::new());
        }
        if let Some(details) = details_value.as_object_mut() {
            details
                .entry("cached_tokens".to_string())
                .or_insert(Value::from(cache_hit_tokens));
            changed = true;
        }
    }

    changed.then(|| serde_json::to_vec(&value).ok()).flatten()
}

fn deepseek_routing_table() -> RoutingTable {
    DeepSeekChannel.routing_table()
}

inventory::submit! { ChannelRegistration::new(DeepSeekChannel::ID, deepseek_routing_table) }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_request_maps_max_completion_tokens_and_developer_role() {
        let mut body = json!({
            "model": "deepseek-chat",
            "max_completion_tokens": 1234,
            "messages": [
                { "role": "developer", "content": "rule" },
                { "role": "user", "content": "hi" }
            ],
            "parallel_tool_calls": true,
            "store": true
        });

        normalize_deepseek_chat_request_body(&mut body);

        assert_eq!(body.get("max_tokens").and_then(|v| v.as_u64()), Some(1234));
        assert!(body.get("max_completion_tokens").is_none());
        assert!(body.get("parallel_tool_calls").is_none());
        assert!(body.get("store").is_none());
        assert_eq!(
            body.get("messages")
                .and_then(|v| v.as_array())
                .and_then(|messages| messages.first())
                .and_then(|message| message.get("role"))
                .and_then(|role| role.as_str()),
            Some("system")
        );
    }

    #[test]
    fn normalize_request_caps_max_tokens_at_8192() {
        let mut body = json!({
            "model": "deepseek-chat",
            "max_tokens": 20000,
            "messages": [{ "role": "user", "content": "hi" }]
        });
        normalize_deepseek_chat_request_body(&mut body);
        assert_eq!(body.get("max_tokens").and_then(|v| v.as_u64()), Some(8192));
    }

    #[test]
    fn normalize_request_flattens_extra_body_thinking_adaptive_to_enabled() {
        let mut body = json!({
            "model": "deepseek-reasoner",
            "messages": [{ "role": "user", "content": "hi" }],
            "extra_body": { "thinking": { "type": "adaptive" } }
        });
        normalize_deepseek_chat_request_body(&mut body);
        assert!(body.get("extra_body").is_none());
        assert_eq!(
            body.get("thinking")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("enabled")
        );
    }

    #[test]
    fn normalize_response_maps_finish_reason_and_cache_tokens() {
        let body = json!({
            "choices": [{
                "index": 0,
                "finish_reason": "insufficient_system_resource",
                "message": { "role": "assistant", "content": "x" }
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15,
                "prompt_cache_hit_tokens": 3
            }
        });
        let normalized =
            normalize_deepseek_chat_response_body(&serde_json::to_vec(&body).expect("json"))
                .expect("normalized");
        let value: Value = serde_json::from_slice(&normalized).expect("valid json");
        assert_eq!(
            value
                .get("choices")
                .and_then(|v| v.get(0))
                .and_then(|v| v.get("finish_reason"))
                .and_then(|v| v.as_str()),
            Some("length")
        );
        assert_eq!(
            value
                .get("usage")
                .and_then(|v| v.get("prompt_tokens_details"))
                .and_then(|v| v.get("cached_tokens"))
                .and_then(|v| v.as_u64()),
            Some(3)
        );
    }

    #[test]
    fn normalize_tools_drops_non_function_and_forces_tool_choice_none_when_empty() {
        let mut body = json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "hi" }],
            "tools": [{ "type": "retrieval", "retrieval": {} }],
            "tool_choice": "auto"
        });
        normalize_deepseek_chat_request_body(&mut body);
        assert!(body.get("tools").is_none());
        assert_eq!(
            body.get("tool_choice").and_then(|v| v.as_str()),
            Some("none")
        );
    }
}
