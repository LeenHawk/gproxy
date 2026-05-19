use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use crate::utils::claude_cache_control as cache_control;
use crate::utils::claude_sampling;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Vercel AI Gateway channel.
///
/// The gateway exposes both OpenAI-compatible and Anthropic-compatible API
/// surfaces on the same host. Keep request routing protocol-aware so native
/// OpenAI Responses / Chat Completions / Embeddings calls stay on the OpenAI
/// paths while Claude Messages and count_tokens use the Anthropic paths.
pub struct VercelChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VercelSettings {
    #[serde(default = "default_vercel_base_url")]
    pub base_url: String,
    /// Enable magic string -> cache_control conversion on Claude-shaped requests.
    #[serde(default)]
    pub enable_magic_cache: bool,
    /// Merge consecutive `system` text blocks before cache breakpoints are applied.
    #[serde(default)]
    pub flatten_system_before_cache: bool,
    /// Cache breakpoint rules applied to Claude-shaped requests.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cache_breakpoints: Vec<cache_control::CacheBreakpointRule>,
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_vercel_base_url() -> String {
    "https://ai-gateway.vercel.sh".to_string()
}

impl ChannelSettings for VercelSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VercelCredential {
    pub api_key: String,
}

impl ChannelCredential for VercelCredential {}

impl Channel for VercelChannel {
    const ID: &'static str = "vercel";
    type Settings = VercelSettings;
    type Credential = VercelCredential;
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
            // === Model list/get: Vercel exposes the OpenAI-compatible model API. ===
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
            // === Count tokens: use the Anthropic-compatible count endpoint. ===
            pass(OperationFamily::CountToken, ProtocolKind::Claude),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::OpenAi,
                OperationFamily::CountToken,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::Gemini,
                OperationFamily::CountToken,
                ProtocolKind::Claude,
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
            // === OpenAI-compatible auxiliary operations ===
            pass(OperationFamily::Embedding, ProtocolKind::OpenAi),
            xform(
                OperationFamily::Embedding,
                ProtocolKind::Gemini,
                OperationFamily::Embedding,
                ProtocolKind::OpenAi,
            ),
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::Compact,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
        ];

        for (key, implementation) in routes {
            t.set(key, implementation);
        }
        t
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let mut url = format!(
            "{}{}",
            vercel_root_base_url(settings.base_url()),
            vercel_request_path(request)?
        );
        crate::utils::url::append_query(&mut url, request.query.as_deref());

        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header("Authorization", format!("Bearer {}", credential.api_key))
            .header("x-api-key", &credential.api_key)
            .header("Content-Type", "application/json");

        if request.route.protocol == ProtocolKind::Claude {
            builder = builder.header("anthropic-version", "2023-06-01");
        }

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
        crate::utils::http_headers::replace_header(&mut builder, "x-api-key", &credential.api_key)?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "Content-Type",
            "application/json",
        )?;
        if request.route.protocol == ProtocolKind::Claude {
            crate::utils::http_headers::replace_header(
                &mut builder,
                "anthropic-version",
                "2023-06-01",
            )?;
        }
        if let Some(ua) = settings.user_agent() {
            crate::utils::http_headers::replace_header(&mut builder, "User-Agent", ua)?;
        }

        builder
            .body(request.body.clone())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn finalize_request(
        &self,
        settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        if request.route.protocol != ProtocolKind::Claude {
            return Ok(request);
        }

        let Ok(mut body_json) = serde_json::from_slice::<Value>(&request.body) else {
            return Ok(request);
        };

        claude_sampling::strip_sampling_params(&mut body_json);
        if settings.enable_magic_cache {
            cache_control::apply_magic_string_cache_control_triggers(&mut body_json);
        }
        if !settings.cache_breakpoints.is_empty() {
            cache_control::ensure_cache_breakpoint_rules(
                &mut body_json,
                &settings.cache_breakpoints,
            );
        }
        if settings.flatten_system_before_cache {
            cache_control::flatten_system_text_blocks(&mut body_json);
        }
        cache_control::sanitize_claude_body(&mut body_json);

        request.body = serde_json::to_vec(&body_json)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(request)
    }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        _body: &[u8],
    ) -> ResponseClassification {
        match status {
            200..=299 => ResponseClassification::Success,
            401 | 402 | 403 => ResponseClassification::AuthDead,
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

    fn count_strategy(&self) -> crate::count_tokens::CountStrategy {
        crate::count_tokens::CountStrategy::UpstreamApi
    }
}

fn vercel_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1/models".to_string()),
        OperationFamily::ModelGet => Ok(format!(
            "/v1/models/{}",
            request.model.as_deref().unwrap_or_default()
        )),
        OperationFamily::CountToken => Ok("/v1/messages/count_tokens".to_string()),
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            match request.route.protocol {
                ProtocolKind::OpenAiResponse => Ok("/v1/responses".to_string()),
                ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAi => {
                    Ok("/v1/chat/completions".to_string())
                }
                ProtocolKind::Claude => Ok("/v1/messages".to_string()),
                _ => Err(UpstreamError::Channel(format!(
                    "unsupported vercel request route: ({}, {})",
                    request.route.operation, request.route.protocol
                ))),
            }
        }
        OperationFamily::Embedding => Ok("/v1/embeddings".to_string()),
        OperationFamily::OpenAiResponseWebSocket => Ok("/v1/responses".to_string()),
        OperationFamily::Compact => Ok("/v1/responses/compact".to_string()),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported vercel request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn vercel_root_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

fn vercel_routing_table() -> RoutingTable {
    VercelChannel.routing_table()
}

inventory::submit! { ChannelRegistration::new(VercelChannel::ID, vercel_routing_table) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::claude_cache_control::{
        CacheBreakpointPositionKind, CacheBreakpointRule, CacheBreakpointTarget, CacheBreakpointTtl,
    };
    use http::{HeaderMap, Method};
    use serde_json::json;

    fn prepared_request(operation: OperationFamily, protocol: ProtocolKind) -> PreparedRequest {
        PreparedRequest {
            method: Method::POST,
            route: RouteKey::new(operation, protocol),
            model: Some("openai/gpt-test".to_string()),
            query: None,
            body: br#"{"model":"openai/gpt-test","messages":[{"role":"user","content":"hi"}]}"#
                .to_vec(),
            headers: HeaderMap::new(),
        }
    }

    fn uri_path(operation: OperationFamily, protocol: ProtocolKind) -> String {
        let settings = VercelSettings::default();
        let credential = VercelCredential {
            api_key: "test-key".to_string(),
        };
        let request = prepared_request(operation, protocol);
        let upstream = VercelChannel
            .prepare_request(&credential, &settings, &request)
            .expect("prepare request");
        upstream.uri().path().to_string()
    }

    #[test]
    fn routes_openai_and_claude_surfaces_to_vercel_paths() {
        assert_eq!(
            uri_path(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion
            ),
            "/v1/chat/completions"
        );
        assert_eq!(
            uri_path(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse
            ),
            "/v1/responses"
        );
        assert_eq!(
            uri_path(OperationFamily::GenerateContent, ProtocolKind::Claude),
            "/v1/messages"
        );
        assert_eq!(
            uri_path(OperationFamily::CountToken, ProtocolKind::Claude),
            "/v1/messages/count_tokens"
        );
        assert_eq!(
            uri_path(OperationFamily::Embedding, ProtocolKind::OpenAi),
            "/v1/embeddings"
        );
    }

    #[test]
    fn payment_required_is_auth_dead() {
        let classification = VercelChannel.classify_response(402, &HeaderMap::new(), b"");

        assert_eq!(classification, ResponseClassification::AuthDead);
    }

    #[test]
    fn accepts_documented_openai_base_url_with_v1_suffix() {
        let settings = VercelSettings {
            base_url: "https://ai-gateway.vercel.sh/v1".to_string(),
            ..Default::default()
        };
        let credential = VercelCredential {
            api_key: "test-key".to_string(),
        };
        let request = prepared_request(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        );
        let upstream = VercelChannel
            .prepare_request(&credential, &settings, &request)
            .expect("prepare request");

        assert_eq!(
            upstream.uri().to_string(),
            "https://ai-gateway.vercel.sh/v1/chat/completions"
        );
    }

    #[test]
    fn claude_route_applies_magic_system_flatten_and_cache_breakpoints() {
        let settings = VercelSettings {
            enable_magic_cache: true,
            flatten_system_before_cache: true,
            cache_breakpoints: vec![CacheBreakpointRule {
                target: CacheBreakpointTarget::System,
                position: CacheBreakpointPositionKind::LastNth,
                index: 1,
                ttl: CacheBreakpointTtl::Ttl1h,
            }],
            ..Default::default()
        };
        let body = json!({
            "model": "anthropic/claude-test",
            "system": [
                { "type": "text", "text": "first " },
                { "type": "text", "text": "second GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF" }
            ],
            "messages": [{ "role": "user", "content": "hi" }]
        });
        let request = PreparedRequest {
            method: Method::POST,
            route: RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::Claude),
            model: Some("anthropic/claude-test".to_string()),
            query: None,
            body: serde_json::to_vec(&body).expect("body"),
            headers: HeaderMap::new(),
        };

        let finalized = VercelChannel
            .finalize_request(&settings, request)
            .expect("finalize request");
        let value: Value = serde_json::from_slice(&finalized.body).expect("json body");
        let system = value
            .get("system")
            .and_then(Value::as_array)
            .expect("system array");

        assert_eq!(system.len(), 1);
        assert_eq!(
            system[0].get("text").and_then(Value::as_str),
            Some("first second")
        );
        assert_eq!(
            system[0]
                .pointer("/cache_control/type")
                .and_then(Value::as_str),
            Some("ephemeral")
        );
        assert_eq!(
            system[0]
                .pointer("/cache_control/ttl")
                .and_then(Value::as_str),
            Some("5m")
        );
    }

    #[test]
    fn openai_route_does_not_apply_claude_cache_controls() {
        let settings = VercelSettings {
            enable_magic_cache: true,
            cache_breakpoints: vec![CacheBreakpointRule {
                target: CacheBreakpointTarget::TopLevel,
                position: CacheBreakpointPositionKind::Nth,
                index: 1,
                ttl: CacheBreakpointTtl::Auto,
            }],
            ..Default::default()
        };
        let body = br#"{"model":"openai/gpt-test","input":"hi"}"#.to_vec();
        let request = PreparedRequest {
            method: Method::POST,
            route: RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            model: Some("openai/gpt-test".to_string()),
            query: None,
            body: body.clone(),
            headers: HeaderMap::new(),
        };

        let finalized = VercelChannel
            .finalize_request(&settings, request)
            .expect("finalize request");

        assert_eq!(finalized.body, body);
    }
}
