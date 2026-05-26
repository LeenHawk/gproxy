use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::channel::{
    Channel, ChannelCredential, ChannelSettings, CommonChannelSettings, OAuthCredentialResult,
    OAuthFlow,
};
use crate::count_tokens::CountStrategy;
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use crate::utils::{code_assist_envelope, oauth2_refresh, vertex_normalize};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

use crate::utils::google_quota::classify_google_quota_response;
use tracing::Instrument;

const DEFAULT_VERSION: &str = "2.0.1";
const DEFAULT_PLATFORM: &str = "Windows";
const DEFAULT_ARCH: &str = "AMD64";
const ANTIGRAVITY_REQUEST_NAMESPACE: uuid::Uuid =
    uuid::uuid!("3649aa15-8c2d-51dc-b95c-f4b79d1db453");
const ANTIGRAVITY_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const ANTIGRAVITY_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const ANTIGRAVITY_REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";
const ANTIGRAVITY_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v1/userinfo?alt=json";
const ANTIGRAVITY_OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs https://www.googleapis.com/auth/aicode";
const ANTIGRAVITY_OAUTH_STATE_TTL_MS: u64 = 600_000;

fn antigravity_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/antigravity.json"))
    })
}

#[derive(Debug, Clone)]
struct AntigravityOAuthState {
    code_verifier: String,
    redirect_uri: String,
    project_id: Option<String>,
    created_at_unix_ms: u64,
}

#[derive(Debug, Deserialize)]
struct AntigravityTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

fn antigravity_oauth_states() -> &'static DashMap<String, AntigravityOAuthState> {
    static STATES: OnceLock<DashMap<String, AntigravityOAuthState>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn prune_antigravity_oauth_states(now_unix_ms: u64) {
    let expired = antigravity_oauth_states()
        .iter()
        .filter_map(|entry| {
            (now_unix_ms.saturating_sub(entry.value().created_at_unix_ms)
                > ANTIGRAVITY_OAUTH_STATE_TTL_MS)
                .then(|| entry.key().clone())
        })
        .collect::<Vec<_>>();
    for key in expired {
        antigravity_oauth_states().remove(key.as_str());
    }
}

fn build_antigravity_authorize_url(
    authorize_url: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("response_type", "code")
        .append_pair("client_id", &default_antigravity_client_id())
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", ANTIGRAVITY_OAUTH_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", code_challenge)
        .append_pair("state", state);
    format!("{}?{}", authorize_url, serializer.finish())
}

async fn exchange_antigravity_code_for_tokens(
    client: &wreq::Client,
    token_url: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<AntigravityTokenResponse, UpstreamError> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}&code_verifier={}",
        crate::utils::oauth::percent_encode(code),
        crate::utils::oauth::percent_encode(redirect_uri),
        crate::utils::oauth::percent_encode(&default_antigravity_client_id()),
        crate::utils::oauth::percent_encode(&default_antigravity_client_secret()),
        crate::utils::oauth::percent_encode(code_verifier),
    );
    let response = client
        .post(token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("antigravity oauth token: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("antigravity oauth body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "antigravity oauth token endpoint status {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("antigravity oauth token parse: {e}")))
}

async fn fetch_antigravity_user_email(
    client: &wreq::Client,
    userinfo_url: &str,
    access_token: &str,
) -> Result<Option<String>, UpstreamError> {
    let response = client
        .get(userinfo_url)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("antigravity userinfo: {e}")))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("antigravity userinfo body: {e}")))?;
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("antigravity userinfo parse: {e}")))?;
    Ok(payload
        .get("email")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned))
}

fn antigravity_code_assist_metadata() -> Value {
    json!({
        "ideType": "ANTIGRAVITY",
        "platform": "PLATFORM_UNSPECIFIED",
        "pluginType": "GEMINI"
    })
}

async fn call_antigravity_code_assist(
    client: &wreq::Client,
    access_token: &str,
    base_url: &str,
) -> Result<Value, UpstreamError> {
    let url = format!(
        "{}/v1internal:loadCodeAssist",
        base_url.trim_end_matches('/')
    );
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(&json!({ "metadata": antigravity_code_assist_metadata() }))
                .map_err(|e| {
                    UpstreamError::Channel(format!("antigravity loadCodeAssist serialize: {e}"))
                })?,
        )
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("antigravity loadCodeAssist: {e}")))?;
    if !response.status().is_success() {
        return Err(UpstreamError::Channel(format!(
            "antigravity loadCodeAssist failed: {}",
            response.status().as_u16()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("antigravity loadCodeAssist body: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("antigravity loadCodeAssist parse: {e}")))
}

async fn resolve_antigravity_project_id(
    client: &wreq::Client,
    access_token: &str,
    base_url: &str,
    project_hint: Option<&str>,
) -> Result<String, UpstreamError> {
    if let Ok(payload) = call_antigravity_code_assist(client, access_token, base_url).await
        && let Some(project) = payload
            .get("cloudaicompanionProject")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    {
        return Ok(project);
    }

    let url = format!("{}/v1internal:onboardUser", base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(&json!({
                "tierId": "LEGACY",
                "metadata": antigravity_code_assist_metadata()
            }))
            .map_err(|e| UpstreamError::Channel(format!("antigravity onboard serialize: {e}")))?,
        )
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("antigravity onboard: {e}")))?;
    if response.status().is_success() {
        let bytes = response
            .bytes()
            .await
            .map_err(|e| UpstreamError::Http(format!("antigravity onboard body: {e}")))?;
        let payload: Value = serde_json::from_slice(&bytes)
            .map_err(|e| UpstreamError::Channel(format!("antigravity onboard parse: {e}")))?;
        if let Some(project) = payload
            .get("response")
            .and_then(|value| value.get("cloudaicompanionProject"))
            .and_then(|value| {
                value
                    .get("id")
                    .and_then(Value::as_str)
                    .or_else(|| value.as_str())
            })
            .map(ToOwned::to_owned)
        {
            return Ok(project);
        }
    }

    project_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            UpstreamError::Channel("antigravity oauth callback: missing project_id".to_string())
        })
}

/// Antigravity channel (Google internal Code Assist API with Antigravity credentials).
pub struct AntigravityChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AntigravitySettings {
    #[serde(default = "default_antigravity_base_url")]
    pub base_url: String,
    #[serde(default = "default_antigravity_api_version")]
    pub api_version: String,
    #[serde(default = "default_antigravity_oauth_authorize_url")]
    pub oauth_authorize_url: String,
    #[serde(default = "default_antigravity_oauth_token_url")]
    pub oauth_token_url: String,
    #[serde(default = "default_antigravity_oauth_userinfo_url")]
    pub oauth_userinfo_url: String,
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_antigravity_base_url() -> String {
    "https://cloudcode-pa.googleapis.com".to_string()
}

fn default_antigravity_api_version() -> String {
    "v1internal".to_string()
}

fn default_antigravity_oauth_authorize_url() -> String {
    ANTIGRAVITY_AUTH_URL.to_string()
}

fn default_antigravity_oauth_token_url() -> String {
    ANTIGRAVITY_TOKEN_URL.to_string()
}

fn default_antigravity_oauth_userinfo_url() -> String {
    ANTIGRAVITY_USERINFO_URL.to_string()
}

impl AntigravitySettings {
    /// Build the effective User-Agent string.
    fn effective_user_agent(&self) -> String {
        if let Some(ref ua) = self.common.user_agent {
            ua.clone()
        } else {
            format!(
                "antigravity/{} ({}; {})",
                DEFAULT_VERSION, DEFAULT_PLATFORM, DEFAULT_ARCH
            )
        }
    }
}

impl ChannelSettings for AntigravitySettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AntigravityCredential {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at_ms: u64,
    pub project_id: String,
    #[serde(default = "default_antigravity_client_id")]
    pub client_id: String,
    #[serde(default = "default_antigravity_client_secret")]
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
}

fn default_antigravity_client_id() -> String {
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com".to_string()
}

fn default_antigravity_client_secret() -> String {
    "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".to_string()
}

impl ChannelCredential for AntigravityCredential {
    fn apply_update(&mut self, update: &serde_json::Value) -> bool {
        if let Some(token) = update.get("access_token").and_then(|v| v.as_str()) {
            self.access_token = token.to_string();
            if let Some(exp) = update.get("expires_at_ms").and_then(|v| v.as_u64()) {
                self.expires_at_ms = exp;
            }
            if let Some(rt) = update.get("refresh_token").and_then(|v| v.as_str()) {
                self.refresh_token = rt.to_string();
            }
            true
        } else {
            false
        }
    }
}

fn generate_request_id(
    request: &PreparedRequest,
    wrapped_body: &[u8],
    request_type: &str,
) -> String {
    if let Some(request_id) = request
        .headers
        .get("requestId")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
    {
        return request_id.to_owned();
    }

    let body = serde_json::from_slice::<Value>(wrapped_body).unwrap_or(Value::Null);
    let route_label = format!("{}/{}", request.route.operation, request.route.protocol);
    let seed = format!(
        "{}\n{}\n{}\n{}\n{}",
        antigravity_instruction_fingerprint(&body),
        antigravity_first_message_fingerprint(&body),
        request.model.as_deref().unwrap_or_default(),
        route_label,
        request_type
    );
    format!(
        "req-{}",
        uuid::Uuid::new_v5(&ANTIGRAVITY_REQUEST_NAMESPACE, seed.as_bytes())
    )
}

fn antigravity_instruction_fingerprint(body: &Value) -> String {
    body.get("system_instruction")
        .map(json_fingerprint_text)
        .unwrap_or_default()
}

fn antigravity_first_message_fingerprint(body: &Value) -> String {
    match body.get("contents") {
        Some(Value::Array(contents)) => contents
            .first()
            .map(json_fingerprint_text)
            .unwrap_or_default(),
        Some(value) => json_fingerprint_text(value),
        None => String::new(),
    }
}

fn json_fingerprint_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn strip_antigravity_unsupported_generation_config(body: &mut Value, model: Option<&str>) {
    let Some(config_obj) = body
        .as_object_mut()
        .and_then(|root| root.get_mut("generationConfig"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    config_obj.remove("maxOutputTokens");
    config_obj.remove("max_output_tokens");

    if model.is_some_and(|value| value.to_ascii_lowercase().contains("gemini")) {
        config_obj.remove("logprobs");
        config_obj.remove("responseLogprobs");
        config_obj.remove("response_logprobs");
    }
}

impl Channel for AntigravityChannel {
    const ID: &'static str = "antigravity";
    type Settings = AntigravitySettings;
    type Credential = AntigravityCredential;
    type Health = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        // Same as geminicli / vertex — native protocol is ProtocolKind::Gemini
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
            pass(OperationFamily::ModelList, ProtocolKind::Gemini),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Claude,
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::OpenAi,
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
            ),
            pass(OperationFamily::ModelGet, ProtocolKind::Gemini),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
                OperationFamily::ModelGet,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::OpenAi,
                OperationFamily::ModelGet,
                ProtocolKind::Gemini,
            ),
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
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
            ),
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
            xform(
                OperationFamily::Compact,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
            ),
        ];

        for (key, imp) in routes {
            t.set(key, imp);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        antigravity_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let (method, final_body) = match request.route.operation {
            // For ModelList/ModelGet, use a simple empty body POST instead of envelope wrapping.
            OperationFamily::ModelList | OperationFamily::ModelGet => {
                let body = serde_json::to_vec(&json!({}))
                    .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
                (http::Method::POST, body)
            }
            // Antigravity's count endpoint expects only a `request` wrapper,
            // unlike generate/stream endpoints that require model/project ids.
            OperationFamily::CountToken => {
                let wrapped = wrap_count_tokens_request(&request.body)?;
                (request.method.clone(), wrapped)
            }
            _ => {
                let wrapped = code_assist_envelope::wrap_request(
                    &request.body,
                    request.model.as_deref(),
                    &credential.project_id,
                )?;
                (request.method.clone(), wrapped)
            }
        };

        let mut url = format!(
            "{}{}",
            settings.base_url(),
            antigravity_request_path(request)?
        );
        let upstream_query = if matches!(
            request.route.operation,
            OperationFamily::ModelList | OperationFamily::ModelGet
        ) {
            None
        } else {
            request.query.as_deref()
        };
        crate::utils::url::append_query(&mut url, upstream_query);

        // Determine requestType based on model name
        let request_type = request
            .model
            .as_deref()
            .filter(|m| m.to_ascii_lowercase().contains("image"))
            .map(|_| "image_gen")
            .unwrap_or("agent");

        let ua = settings.effective_user_agent();
        let request_id = generate_request_id(request, &final_body, request_type);

        let mut builder = http::Request::builder()
            .method(method)
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Content-Type", "application/json")
            .header("User-Agent", &ua)
            .header("Accept-Encoding", "gzip")
            .header("requestId", &request_id)
            .header("requestType", request_type);

        for (key, value) in request.headers.iter() {
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
        crate::utils::http_headers::replace_header(&mut builder, "User-Agent", &ua)?;
        crate::utils::http_headers::replace_header(&mut builder, "Accept-Encoding", "gzip")?;
        crate::utils::http_headers::replace_header(&mut builder, "requestId", request_id)?;
        crate::utils::http_headers::replace_header(&mut builder, "requestType", request_type)?;

        builder
            .body(final_body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn finalize_request(
        &self,
        _settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        let mut body_json: Value = serde_json::from_slice(&request.body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        strip_antigravity_unsupported_generation_config(&mut body_json, request.model.as_deref());
        request.body = serde_json::to_vec(&body_json)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(request)
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        match request.route.operation {
            OperationFamily::ModelList => available_models_to_list_response(&body),
            OperationFamily::ModelGet => {
                let target = request.model.as_deref().unwrap_or_default();
                available_models_to_get_response(&body, target)
            }
            _ => {
                let unwrapped = code_assist_envelope::unwrap_response(&body);
                vertex_normalize::normalize_vertex_response(unwrapped)
            }
        }
    }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        body: &[u8],
    ) -> ResponseClassification {
        match status {
            200..=299 => ResponseClassification::Success,
            401..=403 => ResponseClassification::AuthDead,
            429 | 503 => classify_google_quota_response(headers, body),
            500..=502 | 504..=599 => ResponseClassification::TransientError,
            _ => ResponseClassification::PermanentError,
        }
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::UpstreamApi
    }

    fn prepare_quota_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        let url = format!(
            "{}/v1internal:fetchAvailableModels",
            settings.base_url().trim_end_matches('/')
        );
        let user_agent = settings.effective_user_agent();
        let body = serde_json::to_vec(&serde_json::json!({}))
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        let req = http::Request::builder()
            .method(http::Method::POST)
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("Accept-Encoding", "gzip")
            .header("User-Agent", &user_agent)
            .body(body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(Some(req))
    }

    fn refresh_credential<'a>(
        &'a self,
        client: &'a wreq::Client,
        credential: &'a mut Self::Credential,
    ) -> impl std::future::Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        refresh_antigravity_credential(client, ANTIGRAVITY_TOKEN_URL, credential)
    }

    fn refresh_credential_with_settings<'a>(
        &'a self,
        client: &'a wreq::Client,
        settings: &'a Self::Settings,
        credential: &'a mut Self::Credential,
    ) -> impl std::future::Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        refresh_antigravity_credential(client, &settings.oauth_token_url, credential)
    }

    fn oauth_start<'a>(
        &'a self,
        _client: &'a wreq::Client,
        settings: &'a Self::Settings,
        params: &'a BTreeMap<String, String>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<OAuthFlow>, UpstreamError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let now_unix_ms = crate::utils::oauth::current_unix_ms();
            prune_antigravity_oauth_states(now_unix_ms);

            let redirect_uri = crate::utils::oauth::parse_query_value(params, "redirect_uri")
                .unwrap_or_else(|| ANTIGRAVITY_REDIRECT_URI.to_string());
            let project_id = crate::utils::oauth::parse_query_value(params, "project_id");
            let state = crate::utils::oauth::generate_state();
            let code_verifier = crate::utils::oauth::generate_code_verifier();
            let code_challenge = crate::utils::oauth::generate_code_challenge(&code_verifier);
            let authorize_url = build_antigravity_authorize_url(
                &settings.oauth_authorize_url,
                &redirect_uri,
                &state,
                &code_challenge,
            );

            antigravity_oauth_states().insert(
                state.clone(),
                AntigravityOAuthState {
                    code_verifier,
                    redirect_uri: redirect_uri.clone(),
                    project_id,
                    created_at_unix_ms: now_unix_ms,
                },
            );

            Ok(Some(OAuthFlow {
                authorize_url,
                state,
                redirect_uri: Some(redirect_uri),
                verification_uri: None,
                user_code: None,
                mode: Some("authorization_code".to_string()),
                scope: Some(ANTIGRAVITY_OAUTH_SCOPE.to_string()),
                instructions: Some(
                    "Open authorize_url and complete authorization, then call oauth_finish with code/state or callback_url."
                        .to_string(),
                ),
            }))
        })
    }

    fn oauth_finish<'a>(
        &'a self,
        client: &'a wreq::Client,
        settings: &'a Self::Settings,
        params: &'a BTreeMap<String, String>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<OAuthCredentialResult<Self::Credential>>, UpstreamError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if let Some(error) = crate::utils::oauth::parse_query_value(params, "error") {
                let detail = crate::utils::oauth::parse_query_value(params, "error_description")
                    .unwrap_or(error);
                return Err(UpstreamError::Channel(detail));
            }

            prune_antigravity_oauth_states(crate::utils::oauth::current_unix_ms());
            let (code, state_param) = crate::utils::oauth::resolve_code_and_state(params)
                .map_err(|e| UpstreamError::Channel(format!("antigravity oauth callback: {e}")))?;
            let state_id = state_param.ok_or_else(|| {
                UpstreamError::Channel("antigravity oauth callback: missing state".to_string())
            })?;
            let (_, oauth_state) = antigravity_oauth_states()
                .remove(state_id.as_str())
                .ok_or_else(|| {
                    UpstreamError::Channel("antigravity oauth callback: missing state".to_string())
                })?;

            let token = exchange_antigravity_code_for_tokens(
                client,
                &settings.oauth_token_url,
                &code,
                &oauth_state.redirect_uri,
                &oauth_state.code_verifier,
            )
            .await?;
            let access_token = token
                .access_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel(
                        "antigravity oauth callback: missing access_token".to_string(),
                    )
                })?
                .to_string();
            let refresh_token = token
                .refresh_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel(
                        "antigravity oauth callback: missing refresh_token".to_string(),
                    )
                })?
                .to_string();
            let project_id = resolve_antigravity_project_id(
                client,
                &access_token,
                settings.base_url(),
                oauth_state.project_id.as_deref(),
            )
            .await?;
            let user_email =
                fetch_antigravity_user_email(client, &settings.oauth_userinfo_url, &access_token)
                    .await?;
            let expires_at_ms = crate::utils::oauth::current_unix_ms()
                .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000));

            Ok(Some(OAuthCredentialResult {
                credential: AntigravityCredential {
                    access_token: access_token.clone(),
                    refresh_token,
                    expires_at_ms,
                    project_id: project_id.clone(),
                    client_id: default_antigravity_client_id(),
                    client_secret: default_antigravity_client_secret(),
                    user_email: user_email.clone(),
                },
                details: json!({
                    "access_token": access_token,
                    "project_id": project_id,
                    "user_email": user_email,
                    "expires_at_ms": expires_at_ms,
                }),
            }))
        })
    }
}

fn refresh_antigravity_credential<'a>(
    client: &'a wreq::Client,
    token_url: &'a str,
    credential: &'a mut AntigravityCredential,
) -> impl std::future::Future<Output = Result<bool, UpstreamError>> + Send + 'a {
    let client = client.clone();
    let span = tracing::info_span!("refresh_credential", channel = "antigravity");
    async move {
        if credential.refresh_token.is_empty() {
            return Ok(false);
        }
        let result = oauth2_refresh::refresh_oauth2_token(
            &client,
            token_url,
            &credential.client_id,
            &credential.client_secret,
            &credential.refresh_token,
        )
        .await?;
        credential.access_token = result.access_token;
        credential.expires_at_ms = result.expires_at_ms;
        if let Some(rt) = result.refresh_token {
            credential.refresh_token = rt;
        }
        tracing::info!("credential refreshed");
        Ok(true)
    }
    .instrument(span)
}

fn wrap_count_tokens_request(body: &[u8]) -> Result<Vec<u8>, UpstreamError> {
    let mut inner: Value = serde_json::from_slice(body).map_err(|e| {
        UpstreamError::RequestBuild(format!("json parse for countTokens wrap: {e}"))
    })?;

    if let Some(obj) = inner.as_object_mut() {
        obj.remove("model");
    }
    ensure_content_roles(&mut inner);

    serde_json::to_vec(&json!({ "request": inner }))
        .map_err(|e| UpstreamError::RequestBuild(format!("countTokens wrap serialize: {e}")))
}

fn ensure_content_roles(body: &mut Value) {
    let Some(contents) = body
        .as_object_mut()
        .and_then(|obj| obj.get_mut("contents"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for content in contents {
        let Some(obj) = content.as_object_mut() else {
            continue;
        };
        if !obj.contains_key("role") {
            obj.insert("role".to_string(), Value::String("user".to_string()));
        }
    }
}

fn antigravity_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::ModelList | OperationFamily::ModelGet => {
            Ok("/v1internal:fetchAvailableModels".to_string())
        }
        OperationFamily::CountToken => Ok("/v1internal:countTokens".to_string()),
        OperationFamily::GenerateContent => Ok("/v1internal:generateContent".to_string()),
        OperationFamily::StreamGenerateContent | OperationFamily::GeminiLive => {
            // Code Assist streaming endpoints won't stream server-sent
            // events unless `alt=sse` is explicitly set; without it the
            // upstream rejects with `400 INVALID_ARGUMENT`.
            Ok("/v1internal:streamGenerateContent?alt=sse".to_string())
        }
        _ => Err(UpstreamError::Channel(format!(
            "unsupported antigravity request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn antigravity_routing_table() -> RoutingTable {
    AntigravityChannel.routing_table()
}

// ---------------------------------------------------------------------------
// Model list/get from fetchAvailableModels response
// ---------------------------------------------------------------------------

/// Extract models from an `fetchAvailableModels` response.
///
/// The response contains either `{"models": {"model-id": {...}, ...}}` (object)
/// or `{"models": [{"id": "...", ...}, ...]}` (array). Newer Antigravity
/// responses can also expose model ids through purpose-specific fields such as
/// `image_generation_model_ids` and `tiered_model_ids`; keep those models visible
/// even when they are absent from the main `models` map.
fn extract_available_models(payload: &Value) -> Vec<Value> {
    let mut model_meta = BTreeMap::<String, Value>::new();
    let mut model_methods = BTreeMap::<String, BTreeSet<&'static str>>::new();

    if let Some(models_obj) = payload.get("models").and_then(Value::as_object) {
        for (model_id, meta) in models_obj {
            let id = normalize_model_id(model_id);
            model_methods
                .entry(id.clone())
                .or_default()
                .extend(default_generation_methods());
            model_meta.insert(id, meta.clone());
        }
    } else if let Some(models_arr) = payload.get("models").and_then(Value::as_array) {
        for item in models_arr {
            if let Some(id) = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
            {
                let id = normalize_model_id(id);
                model_methods
                    .entry(id.clone())
                    .or_default()
                    .extend(default_generation_methods());
                model_meta.insert(id, item.clone());
            } else if let Some(value) = item.as_str() {
                add_available_model_id(&mut model_methods, value, default_generation_methods());
            }
        }
    }

    add_model_ids_from_fields(
        payload,
        &[
            "default_agent_model_id",
            "defaultAgentModelId",
            "agent_model_sorts",
            "agentModelSorts",
            "battle_mode_model_sorts",
            "battleModeModelSorts",
            "command_model_ids",
            "commandModelIds",
            "tab_model_ids",
            "tabModelIds",
            "mquery_model_ids",
            "mqueryModelIds",
            "web_search_model_ids",
            "webSearchModelIds",
            "commit_message_model_ids",
            "commitMessageModelIds",
            "audio_transcription_model_ids",
            "audioTranscriptionModelIds",
            "tiered_model_ids",
            "tieredModelIds",
        ],
        &mut model_methods,
        default_generation_methods(),
    );
    add_model_ids_from_fields(
        payload,
        &["image_generation_model_ids", "imageGenerationModelIds"],
        &mut model_methods,
        image_generation_methods(),
    );

    let mut models = model_methods
        .into_iter()
        .filter(|(model_id, _)| !is_embedding_model_id(model_id))
        .map(|(model_id, methods)| {
            let meta = model_meta.get(&model_id).unwrap_or(&Value::Null);
            build_model_entry(&model_id, meta, &methods)
        })
        .collect::<Vec<_>>();

    models.sort_by(|a, b| {
        let a_name = a.get("name").and_then(Value::as_str).unwrap_or_default();
        let b_name = b.get("name").and_then(Value::as_str).unwrap_or_default();
        a_name.cmp(b_name)
    });
    models.dedup_by(|a, b| {
        let a_name = a.get("name").and_then(Value::as_str).unwrap_or_default();
        let b_name = b.get("name").and_then(Value::as_str).unwrap_or_default();
        a_name == b_name
    });
    models
}

fn default_generation_methods() -> BTreeSet<&'static str> {
    BTreeSet::from(["countTokens", "generateContent", "streamGenerateContent"])
}

fn image_generation_methods() -> BTreeSet<&'static str> {
    BTreeSet::from(["countTokens", "generateContent", "streamGenerateContent"])
}

fn is_embedding_model_id(model_id: &str) -> bool {
    let lower = model_id.to_ascii_lowercase();
    lower.contains("embedding") || lower.contains("embed")
}

fn add_model_ids_from_fields(
    payload: &Value,
    field_names: &[&str],
    models: &mut BTreeMap<String, BTreeSet<&'static str>>,
    methods: BTreeSet<&'static str>,
) {
    for field in field_names {
        if let Some(value) = payload.get(*field) {
            add_model_ids_from_value(models, value, &methods);
        }
    }
}

fn add_model_ids_from_value(
    models: &mut BTreeMap<String, BTreeSet<&'static str>>,
    value: &Value,
    methods: &BTreeSet<&'static str>,
) {
    match value {
        Value::String(model_id) => add_available_model_id(models, model_id, methods.clone()),
        Value::Array(values) => {
            for value in values {
                add_model_ids_from_value(models, value, methods);
            }
        }
        Value::Object(object) => {
            let direct_model_id = object
                .get("model_id")
                .and_then(Value::as_str)
                .or_else(|| object.get("modelId").and_then(Value::as_str))
                .or_else(|| object.get("id").and_then(Value::as_str))
                .or_else(|| object.get("name").and_then(Value::as_str));
            if let Some(model_id) = direct_model_id {
                add_available_model_id(models, model_id, methods.clone());
            } else {
                for value in object.values() {
                    add_model_ids_from_value(models, value, methods);
                }
            }
        }
        _ => {}
    }
}

fn add_available_model_id(
    models: &mut BTreeMap<String, BTreeSet<&'static str>>,
    model_id: &str,
    methods: BTreeSet<&'static str>,
) {
    let model_id = normalize_model_id(model_id);
    if model_id.is_empty() {
        return;
    }
    models.entry(model_id).or_default().extend(methods);
}

fn normalize_model_id(model: &str) -> String {
    model
        .trim()
        .trim_start_matches('/')
        .trim_start_matches("models/")
        .to_string()
}

fn build_model_entry(model_id: &str, meta: &Value, methods: &BTreeSet<&'static str>) -> Value {
    let display_name = meta
        .get("displayName")
        .and_then(Value::as_str)
        .or_else(|| meta.get("display_name").and_then(Value::as_str))
        .unwrap_or(model_id);
    let methods = methods.iter().copied().collect::<Vec<_>>();

    let mut obj = json!({
        "name": format!("models/{model_id}"),
        "baseModelId": model_id,
        "version": "1",
        "displayName": display_name,
        "supportedGenerationMethods": methods
    });

    if let Some(limit) = meta.get("maxTokens").and_then(Value::as_u64) {
        obj["inputTokenLimit"] = json!(limit);
    }
    if let Some(limit) = meta
        .get("maxOutputTokens")
        .and_then(Value::as_u64)
        .or_else(|| meta.get("outputTokenLimit").and_then(Value::as_u64))
    {
        obj["outputTokenLimit"] = json!(limit);
    }
    obj
}

/// Transform a `fetchAvailableModels` response body into a standard Gemini model list response.
fn available_models_to_list_response(body: &[u8]) -> Vec<u8> {
    let payload: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return body.to_vec(),
    };
    let models = extract_available_models(&payload);
    serde_json::to_vec(&json!({ "models": models })).unwrap_or_else(|_| body.to_vec())
}

/// Transform a `fetchAvailableModels` response body into a standard Gemini model get response.
fn available_models_to_get_response(body: &[u8], target: &str) -> Vec<u8> {
    let payload: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return body.to_vec(),
    };
    let models = extract_available_models(&payload);
    let normalized_target = normalize_model_id(target);
    let found = models.into_iter().find(|m| {
        m.get("name")
            .and_then(Value::as_str)
            .map(|n| normalize_model_id(n) == normalized_target)
            .unwrap_or(false)
    });
    match found {
        Some(model) => serde_json::to_vec(&model).unwrap_or_else(|_| body.to_vec()),
        None => serde_json::to_vec(&json!({
            "error": {
                "code": 404,
                "message": format!("model {} not found", target),
                "status": "NOT_FOUND"
            }
        }))
        .unwrap_or_else(|_| body.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_methods(model: &Value) -> Vec<String> {
        model
            .get("supportedGenerationMethods")
            .and_then(Value::as_array)
            .expect("methods")
            .iter()
            .map(|value| value.as_str().expect("method").to_string())
            .collect()
    }

    #[test]
    fn fetch_available_models_includes_grouped_capability_models() {
        let payload = json!({
            "models": {
                "gemini-2.5-pro": {
                    "displayName": "Gemini 2.5 Pro",
                    "maxTokens": 1048576,
                    "maxOutputTokens": 65536
                }
            },
            "image_generation_model_ids": ["gemini-2.5-flash-image-preview"],
            "embedding_model_ids": ["gemini-embedding-001"],
            "tiered_model_ids": {
                "paid": ["gemini-3-flash-preview"]
            },
            "agent_model_sorts": [
                { "modelId": "gemini-2.5-flash", "sort": 10 }
            ]
        });

        let models = extract_available_models(&payload);
        let by_name = models
            .iter()
            .map(|model| {
                (
                    model
                        .get("name")
                        .and_then(Value::as_str)
                        .expect("name")
                        .to_string(),
                    model,
                )
            })
            .collect::<BTreeMap<_, _>>();

        assert!(by_name.contains_key("models/gemini-2.5-pro"));
        assert!(by_name.contains_key("models/gemini-2.5-flash-image-preview"));
        assert!(!by_name.contains_key("models/gemini-embedding-001"));
        assert!(by_name.contains_key("models/gemini-3-flash-preview"));
        assert!(by_name.contains_key("models/gemini-2.5-flash"));

        assert_eq!(
            model_methods(by_name["models/gemini-2.5-flash-image-preview"]),
            vec![
                "countTokens".to_string(),
                "generateContent".to_string(),
                "streamGenerateContent".to_string(),
            ]
        );
    }

    #[test]
    fn fetch_available_models_get_finds_category_only_models() {
        let body = serde_json::to_vec(&json!({
            "imageGenerationModelIds": ["gemini-2.5-flash-image-preview"]
        }))
        .expect("serialize");

        let response =
            available_models_to_get_response(&body, "models/gemini-2.5-flash-image-preview");
        let model: Value = serde_json::from_slice(&response).expect("model response");

        assert_eq!(
            model.get("name").and_then(Value::as_str),
            Some("models/gemini-2.5-flash-image-preview")
        );
        assert_eq!(
            model_methods(&model),
            vec![
                "countTokens".to_string(),
                "generateContent".to_string(),
                "streamGenerateContent".to_string(),
            ]
        );
    }

    #[test]
    fn settings_accept_oauth_endpoint_overrides() {
        let settings: AntigravitySettings = serde_json::from_value(json!({
            "oauth_authorize_url": "https://auth.example.test/authorize",
            "oauth_token_url": "https://auth.example.test/token",
            "oauth_userinfo_url": "https://auth.example.test/userinfo"
        }))
        .expect("settings");

        assert_eq!(
            settings.oauth_authorize_url,
            "https://auth.example.test/authorize"
        );
        assert_eq!(settings.oauth_token_url, "https://auth.example.test/token");
        assert_eq!(
            settings.oauth_userinfo_url,
            "https://auth.example.test/userinfo"
        );

        let authorize_url = build_antigravity_authorize_url(
            &settings.oauth_authorize_url,
            "http://localhost/callback",
            "state-value",
            "challenge-value",
        );
        assert!(authorize_url.starts_with("https://auth.example.test/authorize?"));
        assert!(authorize_url.contains("state=state-value"));
        assert!(authorize_url.contains("code_challenge=challenge-value"));
    }

    #[test]
    fn model_list_does_not_forward_gemini_pagination_to_fetch_available_models() {
        let channel = AntigravityChannel;
        let settings = AntigravitySettings::default();
        let credential = AntigravityCredential {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at_ms: 0,
            project_id: "project".to_string(),
            client_id: default_antigravity_client_id(),
            client_secret: default_antigravity_client_secret(),
            user_email: None,
        };
        let request = PreparedRequest {
            method: http::Method::GET,
            route: RouteKey::new(OperationFamily::ModelList, ProtocolKind::Gemini),
            model: None,
            query: Some("pageSize=28&pageToken=abc".to_string()),
            body: Vec::new(),
            headers: http::HeaderMap::new(),
        };

        let prepared = channel
            .prepare_request(&credential, &settings, &request)
            .expect("prepare request");

        assert_eq!(prepared.uri().path(), "/v1internal:fetchAvailableModels");
        assert_eq!(prepared.uri().query(), None);
        assert_eq!(prepared.body(), &serde_json::to_vec(&json!({})).unwrap());
    }

    #[test]
    fn count_tokens_uses_request_only_wrapper() {
        let channel = AntigravityChannel;
        let settings = AntigravitySettings::default();
        let credential = AntigravityCredential {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at_ms: 0,
            project_id: "project".to_string(),
            client_id: default_antigravity_client_id(),
            client_secret: default_antigravity_client_secret(),
            user_email: None,
        };
        let body = serde_json::to_vec(&json!({
            "model": "gemini-2.5-flash",
            "contents": [{ "parts": [{ "text": "count these words" }] }]
        }))
        .expect("serialize");
        let request = PreparedRequest {
            method: http::Method::POST,
            route: RouteKey::new(OperationFamily::CountToken, ProtocolKind::Gemini),
            model: Some("gemini-2.5-flash".to_string()),
            query: None,
            body,
            headers: http::HeaderMap::new(),
        };

        let prepared = channel
            .prepare_request(&credential, &settings, &request)
            .expect("prepare request");
        let body_json: Value = serde_json::from_slice(prepared.body()).expect("json body");

        assert_eq!(prepared.uri().path(), "/v1internal:countTokens");
        assert_eq!(body_json.get("model"), None);
        assert_eq!(body_json.get("project"), None);
        assert_eq!(body_json.get("user_prompt_id"), None);
        assert_eq!(
            body_json
                .pointer("/request/contents/0/role")
                .and_then(Value::as_str),
            Some("user")
        );
        assert_eq!(
            body_json
                .pointer("/request/contents/0/parts/0/text")
                .and_then(Value::as_str),
            Some("count these words")
        );
    }

    #[test]
    fn embedding_routes_are_not_advertised() {
        let table = AntigravityChannel.routing_table();

        assert!(
            table
                .resolve(&RouteKey::new(
                    OperationFamily::Embedding,
                    ProtocolKind::Gemini
                ))
                .is_none()
        );
        assert!(
            table
                .resolve(&RouteKey::new(
                    OperationFamily::Embedding,
                    ProtocolKind::OpenAi
                ))
                .is_none()
        );
    }
}

inventory::submit! { ChannelRegistration::new(AntigravityChannel::ID, antigravity_routing_table) }
