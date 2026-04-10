use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::channel::{Channel, ChannelCredential, ChannelSettings};
use crate::count_tokens::CountStrategy;
use crate::dispatch::{DispatchTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::utils::claude_cache_control as cache_control;
use crate::utils::claude_sampling;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Anthropic Claude API channel.
pub struct AnthropicChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicSettings {
    #[serde(default = "default_anthropic_base_url")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries_on_429: Option<u32>,
    /// Enable magic string -> cache_control conversion (e.g. <|CACHE_5M|> in text)
    #[serde(default)]
    pub enable_magic_cache: bool,
    /// Cache breakpoint rules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cache_breakpoints: Vec<cache_control::CacheBreakpointRule>,
    /// Additional `anthropic-beta` header values merged into every
    /// request. Deduplicated case-insensitively against client-supplied
    /// values. Useful for enabling feature betas across all requests
    /// without requiring clients to set the header themselves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_beta_headers: Vec<String>,
}

fn default_anthropic_base_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn anthropic_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/anthropic.json"))
    })
}

impl ChannelSettings for AnthropicSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }
    fn max_retries_on_429(&self) -> u32 {
        self.max_retries_on_429.unwrap_or(3)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicCredential {
    pub api_key: String,
}

impl ChannelCredential for AnthropicCredential {}

impl Channel for AnthropicChannel {
    const ID: &'static str = "anthropic";
    type Settings = AnthropicSettings;
    type Credential = AnthropicCredential;
    type Health = ModelCooldownHealth;

    fn dispatch_table(&self) -> DispatchTable {
        let mut t = DispatchTable::new();
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

        let routes = vec![
            // Model list/get
            pass(OperationFamily::ModelList, ProtocolKind::Claude),
            pass(OperationFamily::ModelList, ProtocolKind::OpenAi),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
                OperationFamily::ModelList,
                ProtocolKind::Claude,
            ),
            pass(OperationFamily::ModelGet, ProtocolKind::Claude),
            pass(OperationFamily::ModelGet, ProtocolKind::OpenAi),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::Gemini,
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
            ),
            // Count tokens
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
            // Generate content (non-stream)
            pass(OperationFamily::GenerateContent, ProtocolKind::Claude),
            pass(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
            ),
            // Generate content (stream)
            pass(OperationFamily::StreamGenerateContent, ProtocolKind::Claude),
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::GeminiNDJson,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            // Live API
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            // WebSocket → stream
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            // Compact → generate
            xform(
                OperationFamily::Compact,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
            ),
            // Files API
            pass(OperationFamily::FileUpload, ProtocolKind::Claude),
            pass(OperationFamily::FileList, ProtocolKind::Claude),
            pass(OperationFamily::FileContent, ProtocolKind::Claude),
            pass(OperationFamily::FileGet, ProtocolKind::Claude),
            pass(OperationFamily::FileDelete, ProtocolKind::Claude),
        ];

        for (key, implementation) in routes {
            t.set(key, implementation);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        anthropic_model_pricing()
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
            anthropic_request_path(request)?
        );
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header("x-api-key", &credential.api_key)
            .header("anthropic-version", "2023-06-01");

        // File operations: don't force Content-Type to application/json
        // (multipart upload carries its own Content-Type via request.headers).
        if !crate::engine::is_file_operation(request.route.operation) {
            builder = builder.header("Content-Type", "application/json");
        }

        if let Some(ua) = settings.user_agent() {
            builder = builder.header("User-Agent", ua);
        }

        for (key, value) in request.headers.iter() {
            builder = builder.header(key, value);
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
        // File operations: ensure the files-api beta is present in the
        // `anthropic-beta` header without clobbering any value the
        // client or an earlier layer already set, then skip JSON body
        // normalization.
        if crate::engine::is_file_operation(request.route.operation) {
            crate::utils::anthropic_beta::ensure_anthropic_beta_tokens(
                &mut request.headers,
                &["files-api-2025-04-14"],
            )?;
            return Ok(request);
        }

        // Body may be empty or non-JSON for GET-shaped operations that still
        // reach `finalize_request` (model_list, model_get). Skip normalization
        // silently for those — there's nothing to strip or inject.
        let Ok(mut body_json) = serde_json::from_slice::<Value>(&request.body) else {
            return Ok(request);
        };

        // Strip client-supplied sampling params before anything else — some
        // newer Anthropic models reject non-default temperature / top_p /
        // top_k, and we want the default behavior for every client.
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
        // Merge any operator-configured beta values into the header.
        if !settings.extra_beta_headers.is_empty() {
            let refs: Vec<&str> = settings
                .extra_beta_headers
                .iter()
                .map(String::as_str)
                .collect();
            crate::utils::anthropic_beta::ensure_anthropic_beta_tokens(
                &mut request.headers,
                &refs,
            )?;
        }
        request.body = serde_json::to_vec(&body_json)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(request)
    }

    fn model_suffix_groups(&self) -> &'static [crate::suffix::SuffixGroup] {
        crate::suffix::CLAUDE_EXTRA_SUFFIX_GROUPS
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
            529 => ResponseClassification::TransientError,
            500..=599 => ResponseClassification::TransientError,
            _ => ResponseClassification::PermanentError,
        }
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::UpstreamApi
    }
}

fn anthropic_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::FileUpload => Ok("/v1/files".to_string()),
        OperationFamily::FileList => Ok("/v1/files".to_string()),
        OperationFamily::FileContent => Ok(format!(
            "/v1/files/{}/content",
            serde_json::from_slice::<Value>(&request.body)
                .ok()
                .and_then(|v| v
                    .pointer("/path/file_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned))
                .unwrap_or_default()
        )),
        OperationFamily::FileGet | OperationFamily::FileDelete => Ok(format!(
            "/v1/files/{}",
            serde_json::from_slice::<Value>(&request.body)
                .ok()
                .and_then(|v| v
                    .pointer("/path/file_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned))
                .unwrap_or_default()
        )),
        OperationFamily::ModelList => Ok("/v1/models".to_string()),
        OperationFamily::ModelGet => Ok(format!(
            "/v1/models/{}",
            request.model.as_deref().unwrap_or_default()
        )),
        OperationFamily::CountToken => Ok("/v1/messages/count_tokens".to_string()),
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            Ok("/v1/messages".to_string())
        }
        _ => Err(UpstreamError::Channel(format!(
            "unsupported anthropic request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn anthropic_dispatch_table() -> DispatchTable {
    AnthropicChannel.dispatch_table()
}

inventory::submit! { ChannelRegistration::new(AnthropicChannel::ID, anthropic_dispatch_table) }
