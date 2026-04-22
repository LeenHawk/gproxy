use std::sync::OnceLock;

use dashmap::DashMap;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
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

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

fn vertex_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/vertex.json"))
    })
}

/// Vertex AI (Google Cloud) channel using OAuth2 service account authentication.
///
/// Token refresh is automatic: `refresh_credential` is called before each
/// request and only contacts the token endpoint when the cached token is
/// expired or about to expire.
pub struct VertexChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VertexSettings {
    #[serde(default = "default_vertex_base_url")]
    pub base_url: String,
    #[serde(default = "default_vertex_location")]
    pub location: String,
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_vertex_base_url() -> String {
    "https://aiplatform.googleapis.com".to_string()
}

fn default_vertex_location() -> String {
    "us-central1".to_string()
}

impl ChannelSettings for VertexSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VertexCredential {
    /// Google Cloud project ID.
    pub project_id: String,
    /// Service account email.
    pub client_email: String,
    /// PEM-encoded private key for JWT signing.
    pub private_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_uri: Option<String>,
    /// Current OAuth2 access token (populated by token refresh).
    #[serde(default)]
    pub access_token: String,
    /// Token expiry as unix timestamp in milliseconds.
    #[serde(default)]
    pub expires_at_ms: u64,
}

impl ChannelCredential for VertexCredential {
    fn apply_update(&mut self, update: &serde_json::Value) -> bool {
        if let Some(token) = update.get("access_token").and_then(|v| v.as_str()) {
            self.access_token = token.to_string();
            if let Some(exp) = update.get("expires_at_ms").and_then(|v| v.as_u64()) {
                self.expires_at_ms = exp;
            }
            true
        } else {
            false
        }
    }
}

// === Token cache ===

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    expires_at_ms: u64,
}

fn token_cache() -> &'static DashMap<String, CachedToken> {
    static CACHE: OnceLock<DashMap<String, CachedToken>> = OnceLock::new();
    CACHE.get_or_init(DashMap::new)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// === JWT + token exchange ===

#[derive(Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    expires_in: Option<u64>,
}

async fn refresh_access_token(
    client: &wreq::Client,
    credential: &VertexCredential,
) -> Result<CachedToken, UpstreamError> {
    let token_uri = credential
        .token_uri
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_TOKEN_URI);

    let now_s = now_ms() / 1000;
    let claims = JwtClaims {
        iss: &credential.client_email,
        scope: DEFAULT_SCOPE,
        aud: token_uri,
        iat: now_s,
        exp: now_s.saturating_add(3600),
    };

    let pem = credential.private_key.replace("\\n", "\n");
    let key = EncodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|e| UpstreamError::Channel(format!("invalid private key: {e}")))?;

    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("JWT".to_string());
    let assertion = encode(&header, &claims, &key)
        .map_err(|e| UpstreamError::Channel(format!("jwt sign failed: {e}")))?;

    let body = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
    );

    let resp = client
        .post(token_uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("token refresh: {e}")))?;

    let status = resp.status().as_u16();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("token refresh body: {e}")))?;

    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(&bytes);
        return Err(UpstreamError::Channel(format!(
            "token endpoint status {status}: {text}"
        )));
    }

    let parsed: TokenResponse = serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("token response parse: {e}")))?;

    let access_token = parsed
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| UpstreamError::Channel("token response missing access_token".into()))?;

    let expires_in = parsed.expires_in.unwrap_or(3600);
    let expires_at_ms = now_ms().saturating_add(expires_in.saturating_mul(1000));

    Ok(CachedToken {
        access_token,
        expires_at_ms,
    })
}

// === Channel impl ===

/// Bootstrap a Vertex credential on upsert: if `access_token` is empty but
/// `client_email` and `private_key` are present, exchange the service account
/// key for an access token so the first request doesn't fail with empty auth.
pub async fn bootstrap_vertex_token(
    client: &wreq::Client,
    credential_json: &Value,
) -> Result<
    (Option<Value>, Vec<crate::meta::UpstreamRequestMeta>),
    (UpstreamError, Vec<crate::meta::UpstreamRequestMeta>),
> {
    let cred: VertexCredential = serde_json::from_value(credential_json.clone()).map_err(|e| {
        (
            UpstreamError::Channel(format!("parse vertex credential: {e}")),
            Vec::new(),
        )
    })?;

    // Nothing to do if access_token is already populated or no SA material
    if !cred.access_token.is_empty() || cred.client_email.is_empty() || cred.private_key.is_empty()
    {
        return Ok((None, Vec::new()));
    }

    let token = refresh_access_token(client, &cred)
        .await
        .map_err(|e| (e, Vec::new()))?;

    // Populate access_token and expires_at_ms in the credential JSON
    let mut updated = credential_json.clone();
    updated["access_token"] = Value::String(token.access_token.clone());
    updated["expires_at_ms"] = Value::Number(token.expires_at_ms.into());

    // Cache the token
    token_cache().insert(cred.client_email.clone(), token);

    Ok((Some(updated), Vec::new()))
}

impl Channel for VertexChannel {
    const ID: &'static str = "vertex";
    type Settings = VertexSettings;
    type Credential = VertexCredential;
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
            // Live API (native)
            pass(OperationFamily::GeminiLive, ProtocolKind::Gemini),
            // WebSocket -> stream
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
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
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        vertex_model_pricing()
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        match request.route.operation {
            OperationFamily::ModelList => normalize_vertex_model_list_response(body),
            OperationFamily::ModelGet => normalize_vertex_model_get_response(body),
            _ => crate::utils::vertex_normalize::normalize_vertex_response(body),
        }
    }

    fn refresh_credential<'a>(
        &'a self,
        client: &'a wreq::Client,
        credential: &'a mut Self::Credential,
    ) -> impl std::future::Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        let client = client.clone();
        async move {
            // No valid refresh material → can't refresh
            if credential.client_email.is_empty() || credential.private_key.is_empty() {
                return Ok(false);
            }

            // Invalidate any cached token for this email (it just failed)
            token_cache().remove(&credential.client_email);

            // Force refresh
            let token = refresh_access_token(&client, credential).await?;
            credential.access_token = token.access_token.clone();
            credential.expires_at_ms = token.expires_at_ms;
            token_cache().insert(credential.client_email.clone(), token);
            Ok(true)
        }
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let mut url = format!(
            "{}{}",
            settings.base_url(),
            vertex_request_path(request, settings, credential)?
        );
        crate::utils::url::append_query(&mut url, request.query.as_deref());
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Content-Type", "application/json");

        if let Some(ua) = settings.user_agent() {
            builder = builder.header("User-Agent", ua);
        }

        for (key, value) in request.headers.iter() {
            // Drop Claude-specific headers that leak through cross-protocol transforms
            if key == "anthropic-version" || key == "anthropic-beta" {
                continue;
            }
            builder = builder.header(key, value);
        }
        crate::utils::http_headers::replace_header(
            &mut builder,
            "Authorization",
            format!("Bearer {}", credential.access_token),
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

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::UpstreamApi
    }
}

fn vertex_request_path(
    request: &PreparedRequest,
    settings: &VertexSettings,
    credential: &VertexCredential,
) -> Result<String, UpstreamError> {
    let model = request
        .model
        .as_deref()
        .unwrap_or_default()
        .trim_start_matches("models/")
        .to_string();
    let project_prefix = format!(
        "/v1beta1/projects/{}/locations/{}/",
        credential.project_id.trim(),
        settings.location.trim()
    );
    match request.route.operation {
        OperationFamily::ModelList => Ok("/v1beta1/publishers/google/models".to_string()),
        OperationFamily::ModelGet => Ok(format!("/v1beta1/publishers/google/models/{model}")),
        OperationFamily::CountToken => Ok(format!(
            "{project_prefix}publishers/google/models/{model}:countTokens"
        )),
        OperationFamily::GenerateContent => Ok(format!(
            "{project_prefix}publishers/google/models/{model}:generateContent"
        )),
        OperationFamily::StreamGenerateContent | OperationFamily::GeminiLive => Ok(format!(
            "{project_prefix}publishers/google/models/{model}:streamGenerateContent{}",
            if request.route.protocol == ProtocolKind::Gemini {
                "?alt=sse"
            } else {
                ""
            }
        )),
        OperationFamily::Embedding => Ok(format!(
            "{project_prefix}publishers/google/models/{model}:predict"
        )),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported vertex request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn vertex_routing_table() -> RoutingTable {
    VertexChannel.routing_table()
}

// ---------------------------------------------------------------------------
// Vertex model list/get response normalization
// ---------------------------------------------------------------------------
// Vertex AI returns `publisherModels` instead of `models`, and model names
// use the full resource path (`publishers/google/models/gemini-2.5-pro`).
// These functions convert to the standard Gemini format.

fn normalize_vertex_model_list_response(body: Vec<u8>) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Value::Object(mut map) = value else {
        return body;
    };
    // Already in standard Gemini format
    if map.contains_key("models") {
        return body;
    }
    let models = match map.remove("publisherModels") {
        Some(Value::Array(items)) => items
            .into_iter()
            .map(vertex_publisher_model_to_gemini)
            .collect::<Vec<_>>(),
        Some(item) => vec![vertex_publisher_model_to_gemini(item)],
        None => Vec::new(),
    };
    let mut out = serde_json::Map::new();
    out.insert("models".to_string(), Value::Array(models));
    if let Some(token) = map.remove("nextPageToken").filter(|v| !v.is_null()) {
        out.insert("nextPageToken".to_string(), token);
    }
    serde_json::to_vec(&Value::Object(out)).unwrap_or(body)
}

fn normalize_vertex_model_get_response(body: Vec<u8>) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Value::Object(mut map) = value else {
        return body;
    };
    // Already in standard Gemini format (name starts with "models/")
    if map
        .get("name")
        .and_then(|v| v.as_str())
        .is_some_and(|n| n.starts_with("models/"))
    {
        return body;
    }
    let converted = if let Some(inner) = map.remove("publisherModel") {
        vertex_publisher_model_to_gemini(inner)
    } else {
        vertex_publisher_model_to_gemini(Value::Object(map))
    };
    serde_json::to_vec(&converted).unwrap_or(body)
}

/// Convert a Vertex AI `publisherModel` object to standard Gemini model format.
fn vertex_publisher_model_to_gemini(value: Value) -> Value {
    let Value::Object(map) = value else {
        return value;
    };
    // Extract model ID from full resource path
    let raw_name = map
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim();
    let model_id = if let Some((_, tail)) = raw_name.rsplit_once("/models/") {
        tail
    } else {
        raw_name.strip_prefix("models/").unwrap_or(raw_name)
    };
    let model_id = if model_id.is_empty() {
        "unknown"
    } else {
        model_id
    };

    let mut out = serde_json::Map::new();
    out.insert(
        "name".to_string(),
        Value::String(format!("models/{model_id}")),
    );
    for key in [
        "baseModelId",
        "version",
        "displayName",
        "description",
        "inputTokenLimit",
        "outputTokenLimit",
        "supportedGenerationMethods",
        "thinking",
        "temperature",
        "maxTemperature",
        "topP",
        "topK",
    ] {
        if let Some(v) = map.get(key).cloned().filter(|v| !v.is_null()) {
            out.insert(key.to_string(), v);
        }
    }
    Value::Object(out)
}

inventory::submit! { ChannelRegistration::new(VertexChannel::ID, vertex_routing_table) }
