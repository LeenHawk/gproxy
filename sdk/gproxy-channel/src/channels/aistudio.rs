use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::channel::{Channel, ChannelCredential, ChannelSettings};
use crate::count_tokens::CountStrategy;
use crate::dispatch::{DispatchTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Google AI Studio (Gemini REST API) channel.
pub struct AiStudioChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiStudioSettings {
    #[serde(default = "default_aistudio_base_url")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries_on_429: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sanitize_rules: Vec<crate::utils::sanitize::SanitizeRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewrite_rules: Vec<crate::utils::rewrite::RewriteRule>,
}

fn default_aistudio_base_url() -> String {
    "https://generativelanguage.googleapis.com".to_string()
}

fn aistudio_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/aistudio.json"))
    })
}

impl ChannelSettings for AiStudioSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }
    fn max_retries_on_429(&self) -> u32 {
        self.max_retries_on_429.unwrap_or(3)
    }
    fn sanitize_rules(&self) -> &[crate::utils::sanitize::SanitizeRule] {
        &self.sanitize_rules
    }
    fn rewrite_rules(&self) -> &[crate::utils::rewrite::RewriteRule] {
        &self.rewrite_rules
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiStudioCredential {
    pub api_key: String,
}

impl ChannelCredential for AiStudioCredential {}

impl Channel for AiStudioChannel {
    const ID: &'static str = "aistudio";
    type Settings = AiStudioSettings;
    type Credential = AiStudioCredential;
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
            pass(OperationFamily::ModelList, ProtocolKind::Gemini),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Claude,
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
            ),
            pass(OperationFamily::ModelList, ProtocolKind::OpenAi),
            pass(OperationFamily::ModelGet, ProtocolKind::Gemini),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
                OperationFamily::ModelGet,
                ProtocolKind::Gemini,
            ),
            pass(OperationFamily::ModelGet, ProtocolKind::OpenAi),
            // Count tokens
            pass(OperationFamily::CountToken, ProtocolKind::Gemini),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::Claude,
                OperationFamily::CountToken,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::OpenAi,
                OperationFamily::CountToken,
                ProtocolKind::Gemini,
            ),
            // Generate content (non-stream)
            pass(OperationFamily::GenerateContent, ProtocolKind::Gemini),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
            ),
            pass(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
            ),
            // Generate content (stream)
            pass(OperationFamily::StreamGenerateContent, ProtocolKind::Gemini),
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::GeminiNDJson,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            // Live API
            pass(OperationFamily::GeminiLive, ProtocolKind::Gemini),
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
            ),
            // Images
            xform(
                OperationFamily::CreateImage,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::StreamCreateImage,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::CreateImageEdit,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::StreamCreateImageEdit,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            // Embeddings
            pass(OperationFamily::Embedding, ProtocolKind::Gemini),
            xform(
                OperationFamily::Embedding,
                ProtocolKind::OpenAi,
                OperationFamily::Embedding,
                ProtocolKind::Gemini,
            ),
            // Compact → generate
            xform(
                OperationFamily::Compact,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
            ),
        ];

        for (key, implementation) in routes {
            t.set(key, implementation);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        aistudio_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        // Gemini API uses query parameter for auth
        let path = aistudio_request_path(request)?;
        let separator = if path.contains('?') { "&" } else { "?" };
        let url = format!(
            "{}{}{}key={}",
            settings.base_url(),
            path,
            separator,
            credential.api_key
        );

        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header("Content-Type", "application/json");

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
        CountStrategy::UpstreamApi
    }
}

fn aistudio_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    let model = request.model.as_deref().unwrap_or_default();
    let model = if model.starts_with("models/") {
        model.to_string()
    } else {
        format!("models/{model}")
    };
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1beta/models".to_string()),
        OperationFamily::ModelGet => Ok(format!("/v1beta/{model}")),
        OperationFamily::CountToken => Ok(format!("/v1beta/{model}:countTokens")),
        OperationFamily::GenerateContent => Ok(format!("/v1beta/{model}:generateContent")),
        OperationFamily::StreamGenerateContent | OperationFamily::GeminiLive => Ok(format!(
            "/v1beta/{model}:streamGenerateContent{}",
            if request.route.protocol == ProtocolKind::Gemini {
                "?alt=sse"
            } else {
                ""
            }
        )),
        OperationFamily::Embedding => Ok(format!("/v1beta/{model}:embedContent")),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported aistudio request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn aistudio_dispatch_table() -> DispatchTable {
    AiStudioChannel.dispatch_table()
}

inventory::submit! { ChannelRegistration::new(AiStudioChannel::ID, aistudio_dispatch_table) }
