use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
use crate::count_tokens::CountStrategy;
use crate::dispatch::{DispatchTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Vertex AI Express (API-key-based Vertex AI access) channel.
pub struct VertexExpressChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VertexExpressSettings {
    #[serde(default = "default_vertexexpress_base_url")]
    pub base_url: String,
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_vertexexpress_base_url() -> String {
    "https://aiplatform.googleapis.com".to_string()
}

fn vertexexpress_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/vertexexpress.json"))
    })
}

impl ChannelSettings for VertexExpressSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VertexExpressCredential {
    pub api_key: String,
}

impl ChannelCredential for VertexExpressCredential {}

impl Channel for VertexExpressChannel {
    const ID: &'static str = "vertexexpress";
    type Settings = VertexExpressSettings;
    type Credential = VertexExpressCredential;
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
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
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
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            // WebSocket -> stream
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
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
            // Compact -> generate
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
        // Model list/get — served locally from a static model catalogue;
        // Vertex AI Express does not expose a standard model-listing endpoint.
        // All protocols are set to Local; handle_local converts the Gemini
        // catalogue to the target protocol via transform_response.
        for proto in [
            ProtocolKind::Gemini,
            ProtocolKind::Claude,
            ProtocolKind::OpenAi,
        ] {
            t.set(
                RouteKey::new(OperationFamily::ModelList, proto),
                RouteImplementation::Local,
            );
            t.set(
                RouteKey::new(OperationFamily::ModelGet, proto),
                RouteImplementation::Local,
            );
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        vertexexpress_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let path = vertexexpress_request_path(request)?;
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
            if key == "anthropic-version" || key == "anthropic-beta" {
                continue;
            }
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
        match operation {
            OperationFamily::ModelList => Some(vertexexpress_local_model_list(body).and_then(
                |gemini_body| vertexexpress_local_transform(operation, protocol, gemini_body),
            )),
            OperationFamily::ModelGet => {
                Some(vertexexpress_local_model_get(body).and_then(|gemini_body| {
                    vertexexpress_local_transform(operation, protocol, gemini_body)
                }))
            }
            _ => None,
        }
    }

    fn normalize_response(&self, _request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        crate::utils::vertex_normalize::normalize_vertex_response(body)
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::UpstreamApi
    }
}

fn vertexexpress_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    let model = request
        .model
        .as_deref()
        .unwrap_or_default()
        .trim_start_matches("models/")
        .to_string();
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1beta1/publishers/google/models".to_string()),
        OperationFamily::ModelGet => Ok(format!("/v1beta1/publishers/google/models/{model}")),
        OperationFamily::CountToken => Ok(format!(
            "/v1beta1/publishers/google/models/{model}:countTokens"
        )),
        OperationFamily::GenerateContent => Ok(format!(
            "/v1beta1/publishers/google/models/{model}:generateContent"
        )),
        OperationFamily::StreamGenerateContent | OperationFamily::GeminiLive => Ok(format!(
            "/v1beta1/publishers/google/models/{model}:streamGenerateContent{}",
            if request.route.protocol == ProtocolKind::Gemini {
                "?alt=sse"
            } else {
                ""
            }
        )),
        OperationFamily::Embedding => Ok(format!(
            "/v1beta1/publishers/google/models/{model}:embedContent"
        )),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported vertexexpress request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn vertexexpress_dispatch_table() -> DispatchTable {
    VertexExpressChannel.dispatch_table()
}

// ---------------------------------------------------------------------------
// Static model catalogue for Vertex AI Express
// ---------------------------------------------------------------------------

static VERTEXEXPRESS_MODELS_JSON: &str = include_str!("vertexexpress_models.gemini.json");

/// If the client protocol is not Gemini, transform the Gemini-format body
/// to the target protocol using the standard response transform pipeline.
fn vertexexpress_local_transform(
    operation: OperationFamily,
    protocol: ProtocolKind,
    gemini_body: Vec<u8>,
) -> Result<Vec<u8>, UpstreamError> {
    if protocol == ProtocolKind::Gemini {
        return Ok(gemini_body);
    }
    gproxy_protocol::transform::dispatch::transform_response(
        operation,
        protocol,
        operation,
        ProtocolKind::Gemini,
        gemini_body,
    )
    .map_err(Into::into)
}

fn vertexexpress_local_model_list(body: &[u8]) -> Result<Vec<u8>, UpstreamError> {
    let models_doc: serde_json::Value = serde_json::from_str(VERTEXEXPRESS_MODELS_JSON)
        .map_err(|e| UpstreamError::Channel(format!("static models parse: {e}")))?;

    // Extract pagination from the incoming request body (Gemini ModelList format).
    let req: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let page_size = req
        .get("pageSize")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let page_token = req
        .get("pageToken")
        .and_then(|v| v.as_str())
        .and_then(|t| t.parse::<usize>().ok());

    let all_models = models_doc
        .get("models")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let total = all_models.len();
    let start = page_token.unwrap_or(0).min(total);
    let size = page_size.unwrap_or(total.saturating_sub(start));
    let end = start.saturating_add(size).min(total);
    let next_page_token = if end < total {
        Some(end.to_string())
    } else {
        None
    };

    let response = serde_json::json!({
        "models": &all_models[start..end],
        "nextPageToken": next_page_token,
    });
    serde_json::to_vec(&response)
        .map_err(|e| UpstreamError::Channel(format!("model list serialize: {e}")))
}

fn vertexexpress_local_model_get(body: &[u8]) -> Result<Vec<u8>, UpstreamError> {
    let models_doc: serde_json::Value = serde_json::from_str(VERTEXEXPRESS_MODELS_JSON)
        .map_err(|e| UpstreamError::Channel(format!("static models parse: {e}")))?;

    // The Gemini ModelGet request body contains `{"name": "models/..."}`.
    let req: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let target = req.get("name").and_then(|v| v.as_str()).unwrap_or_default();
    let normalized = target.trim().trim_start_matches("models/");

    let all_models = models_doc
        .get("models")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    let found = all_models.into_iter().find(|m| {
        m.get("name")
            .and_then(|n| n.as_str())
            .map(|n| n.trim_start_matches("models/") == normalized)
            .unwrap_or(false)
    });

    match found {
        Some(model) => serde_json::to_vec(&model)
            .map_err(|e| UpstreamError::Channel(format!("model get serialize: {e}"))),
        None => serde_json::to_vec(&serde_json::json!({
            "error": {
                "code": 404,
                "message": format!("model {} not found", target),
                "status": "NOT_FOUND"
            }
        }))
        .map_err(|e| UpstreamError::Channel(format!("model get serialize: {e}"))),
    }
}

inventory::submit! { ChannelRegistration::new(VertexExpressChannel::ID, vertexexpress_dispatch_table) }
