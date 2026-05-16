use serde::{Deserialize, Serialize};

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Vercel AI Gateway channel.
///
/// The gateway exposes OpenAI-compatible models, chat completions, and
/// embeddings endpoints with Bearer credentials.
pub struct VercelChannel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VercelSettings {
    #[serde(default = "default_vercel_base_url")]
    pub base_url: String,
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

impl Default for VercelSettings {
    fn default() -> Self {
        Self {
            base_url: default_vercel_base_url(),
            common: CommonChannelSettings::default(),
        }
    }
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

        for key in [
            pass(OperationFamily::ModelList, ProtocolKind::OpenAi),
            pass(OperationFamily::ModelGet, ProtocolKind::OpenAi),
            pass(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(OperationFamily::Embedding, ProtocolKind::OpenAi),
        ] {
            t.set(key.0, key.1);
        }

        t
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let mut url = format!("{}{}", settings.base_url(), vercel_request_path(request)?);
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

    fn prepare_quota_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        let url = format!("{}/v1/credits", settings.base_url().trim_end_matches('/'));
        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .header("Authorization", format!("Bearer {}", credential.api_key))
            .header("Accept", "application/json");

        if let Some(ua) = settings.user_agent() {
            builder = builder.header("User-Agent", ua);
        }

        builder
            .body(Vec::new())
            .map(Some)
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
}

fn vercel_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1/models".to_string()),
        OperationFamily::ModelGet => Ok(format!(
            "/v1/models/{}",
            request.model.as_deref().unwrap_or_default()
        )),
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            match request.route.protocol {
                ProtocolKind::OpenAiChatCompletion => Ok("/v1/chat/completions".to_string()),
                _ => Err(UpstreamError::Channel(format!(
                    "unsupported vercel generate route protocol: {}",
                    request.route.protocol
                ))),
            }
        }
        OperationFamily::Embedding => Ok("/v1/embeddings".to_string()),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported vercel request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn vercel_routing_table() -> RoutingTable {
    VercelChannel.routing_table()
}

inventory::submit! { ChannelRegistration::new(VercelChannel::ID, vercel_routing_table) }
