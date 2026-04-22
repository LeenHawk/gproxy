use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// OpenAI API channel.
pub struct OpenAiChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAiSettings {
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_openai_base_url() -> String {
    "https://api.openai.com".to_string()
}

fn openai_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/openai.json"))
    })
}

impl ChannelSettings for OpenAiSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAiCredential {
    pub api_key: String,
}

impl ChannelCredential for OpenAiCredential {}

impl Channel for OpenAiChannel {
    const ID: &'static str = "openai";
    type Settings = OpenAiSettings;
    type Credential = OpenAiCredential;
    type Health = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        let mut t = RoutingTable::new();

        // Helper: passthrough = src and dst are same
        let pass = |op: OperationFamily, proto: ProtocolKind| {
            (RouteKey::new(op, proto), RouteImplementation::Passthrough)
        };
        // Helper: transform = src converts to different dst
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
            // === Count tokens ===
            pass(OperationFamily::CountToken, ProtocolKind::OpenAi),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::Claude,
                OperationFamily::CountToken,
                ProtocolKind::OpenAi,
            ),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::Gemini,
                OperationFamily::CountToken,
                ProtocolKind::OpenAi,
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
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
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
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
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
            // === WebSocket ===
            pass(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
            ),
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // === Images ===
            pass(OperationFamily::CreateImage, ProtocolKind::OpenAi),
            pass(OperationFamily::StreamCreateImage, ProtocolKind::OpenAi),
            pass(OperationFamily::CreateImageEdit, ProtocolKind::OpenAi),
            pass(OperationFamily::StreamCreateImageEdit, ProtocolKind::OpenAi),
            // === Embeddings ===
            pass(OperationFamily::Embedding, ProtocolKind::OpenAi),
            xform(
                OperationFamily::Embedding,
                ProtocolKind::Gemini,
                OperationFamily::Embedding,
                ProtocolKind::OpenAi,
            ),
            // === Compact (OpenAI Responses only) ===
            pass(OperationFamily::Compact, ProtocolKind::OpenAi),
        ];

        for (key, implementation) in routes {
            t.set(key, implementation);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        openai_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let mut url = format!("{}{}", settings.base_url(), openai_request_path(request)?);
        crate::utils::url::append_query(&mut url, request.query.as_deref());
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

    fn count_strategy(&self) -> crate::count_tokens::CountStrategy {
        crate::count_tokens::CountStrategy::UpstreamApi
    }
}

fn openai_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1/models".to_string()),
        OperationFamily::ModelGet => Ok(format!(
            "/v1/models/{}",
            request.model.as_deref().unwrap_or_default()
        )),
        OperationFamily::CountToken => Ok("/v1/responses/input_tokens/count".to_string()),
        OperationFamily::Compact => Ok("/v1/responses/compact".to_string()),
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            match request.route.protocol {
                ProtocolKind::OpenAiResponse => Ok("/v1/responses".to_string()),
                ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAi => {
                    Ok("/v1/chat/completions".to_string())
                }
                _ => Err(UpstreamError::Channel(format!(
                    "unsupported openai request route: ({}, {})",
                    request.route.operation, request.route.protocol
                ))),
            }
        }
        OperationFamily::CreateImage | OperationFamily::StreamCreateImage => {
            Ok("/v1/images/generations".to_string())
        }
        OperationFamily::CreateImageEdit | OperationFamily::StreamCreateImageEdit => {
            Ok("/v1/images/edits".to_string())
        }
        OperationFamily::Embedding => Ok("/v1/embeddings".to_string()),
        OperationFamily::OpenAiResponseWebSocket => Ok("/v1/responses".to_string()),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported openai request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn openai_routing_table() -> RoutingTable {
    OpenAiChannel.routing_table()
}

inventory::submit! { ChannelRegistration::new(OpenAiChannel::ID, openai_routing_table) }
