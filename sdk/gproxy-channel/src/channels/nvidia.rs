use serde::{Deserialize, Serialize};

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
use crate::dispatch::{DispatchTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// NVIDIA NIM API channel.
///
/// OpenAI-compatible but only exposes Chat Completions (no Response API).
/// Token counting is done locally since NVIDIA has no count_tokens endpoint.
pub struct NvidiaChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NvidiaSettings {
    #[serde(default = "default_nvidia_base_url")]
    pub base_url: String,
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_nvidia_base_url() -> String {
    "https://integrate.api.nvidia.com".to_string()
}

impl ChannelSettings for NvidiaSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NvidiaCredential {
    pub api_key: String,
}

impl ChannelCredential for NvidiaCredential {}

impl Channel for NvidiaChannel {
    const ID: &'static str = "nvidia";
    type Settings = NvidiaSettings;
    type Credential = NvidiaCredential;
    type Health = ModelCooldownHealth;

    fn dispatch_table(&self) -> DispatchTable {
        let mut t = DispatchTable::new();

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
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
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
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
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
            // === WebSocket -> stream ===
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            // === Embeddings ===
            pass(OperationFamily::Embedding, ProtocolKind::OpenAi),
            xform(
                OperationFamily::Embedding,
                ProtocolKind::Gemini,
                OperationFamily::Embedding,
                ProtocolKind::OpenAi,
            ),
            // === Compact -> generate ===
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

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let url = format!("{}{}", settings.base_url(), nvidia_request_path(request)?);
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

fn nvidia_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
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
                _ => Err(UpstreamError::Channel(format!(
                    "unsupported nvidia request route: ({}, {})",
                    request.route.operation, request.route.protocol
                ))),
            }
        }
        OperationFamily::Embedding => Ok("/v1/embeddings".to_string()),
        OperationFamily::OpenAiResponseWebSocket => Ok("/v1/responses".to_string()),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported nvidia request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn nvidia_dispatch_table() -> DispatchTable {
    NvidiaChannel.dispatch_table()
}

inventory::submit! { ChannelRegistration::new(NvidiaChannel::ID, nvidia_dispatch_table) }
