use std::collections::BTreeMap;
use std::sync::OnceLock;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::channel::{
    Channel, ChannelCredential, ChannelSettings, CommonChannelSettings, OAuthCredentialResult, OAuthFlow,
};
use crate::count_tokens::CountStrategy;
use crate::dispatch::{DispatchTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::utils::{code_assist_envelope, oauth2_refresh, vertex_normalize};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

use crate::utils::google_quota::classify_google_quota_response;
use tracing::Instrument;

/// Gemini CLI (Code Assist API) channel with OAuth authentication.
pub struct GeminiCliChannel;

const DEFAULT_GEMINI_CLI_VERSION: &str = "0.35.2";
const DEFAULT_GEMINI_CLI_PLATFORM: &str = "linux";
const DEFAULT_GEMINI_CLI_ARCH: &str = "x64";
const DEFAULT_GEMINI_CLI_SURFACE: &str = "terminal";
const DEFAULT_GOOGLE_GENAI_SDK_VERSION: &str = "1.30.0";
const DEFAULT_GL_NODE_VERSION: &str = "20";
const GEMINICLI_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GEMINICLI_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GEMINICLI_REDIRECT_URI: &str = "http://127.0.0.1:1455/oauth2callback";
const GEMINICLI_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const GEMINICLI_OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";
const GEMINICLI_OAUTH_STATE_TTL_MS: u64 = 600_000;

fn geminicli_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/geminicli.json"))
    })
}

#[derive(Debug, Clone)]
struct GeminiCliOAuthState {
    code_verifier: String,
    redirect_uri: String,
    project_id: Option<String>,
    created_at_unix_ms: u64,
}

#[derive(Debug, Deserialize)]
struct GeminiCliTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

fn geminicli_oauth_states() -> &'static DashMap<String, GeminiCliOAuthState> {
    static STATES: OnceLock<DashMap<String, GeminiCliOAuthState>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn prune_geminicli_oauth_states(now_unix_ms: u64) {
    let expired = geminicli_oauth_states()
        .iter()
        .filter_map(|entry| {
            (now_unix_ms.saturating_sub(entry.value().created_at_unix_ms)
                > GEMINICLI_OAUTH_STATE_TTL_MS)
                .then(|| entry.key().clone())
        })
        .collect::<Vec<_>>();
    for key in expired {
        geminicli_oauth_states().remove(key.as_str());
    }
}

fn build_geminicli_authorize_url(redirect_uri: &str, state: &str, code_challenge: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("response_type", "code")
        .append_pair("client_id", &default_geminicli_client_id())
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", GEMINICLI_OAUTH_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", code_challenge)
        .append_pair("state", state);
    format!("{}?{}", GEMINICLI_AUTH_URL, serializer.finish())
}

async fn exchange_geminicli_code_for_tokens(
    client: &wreq::Client,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<GeminiCliTokenResponse, UpstreamError> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}&code_verifier={}",
        crate::utils::oauth::percent_encode(code),
        crate::utils::oauth::percent_encode(redirect_uri),
        crate::utils::oauth::percent_encode(&default_geminicli_client_id()),
        crate::utils::oauth::percent_encode(&default_geminicli_client_secret()),
        crate::utils::oauth::percent_encode(code_verifier),
    );
    let response = client
        .post(GEMINICLI_TOKEN_URL)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli oauth token: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli oauth body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "geminicli oauth token endpoint status {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("geminicli oauth token parse: {e}")))
}

async fn fetch_geminicli_user_email(
    client: &wreq::Client,
    access_token: &str,
) -> Result<Option<String>, UpstreamError> {
    let response = client
        .get(GEMINICLI_USERINFO_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli userinfo: {e}")))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli userinfo body: {e}")))?;
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("geminicli userinfo parse: {e}")))?;
    Ok(payload
        .get("email")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned))
}

fn geminicli_code_assist_metadata(project_id: Option<&str>) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("ideType".to_string(), json!("IDE_UNSPECIFIED"));
    metadata.insert("platform".to_string(), json!("PLATFORM_UNSPECIFIED"));
    metadata.insert("pluginType".to_string(), json!("GEMINI"));
    if let Some(project) = project_id.map(str::trim).filter(|value| !value.is_empty()) {
        metadata.insert("duetProject".to_string(), json!(project));
    }
    Value::Object(metadata)
}

fn parse_project_id_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

async fn load_geminicli_code_assist(
    client: &wreq::Client,
    access_token: &str,
    base_url: &str,
    project_id: Option<&str>,
) -> Result<Option<Value>, UpstreamError> {
    let url = format!(
        "{}/v1internal:loadCodeAssist",
        base_url.trim_end_matches('/')
    );
    let body = json!({
        "cloudaicompanionProject": project_id,
        "metadata": geminicli_code_assist_metadata(project_id),
    });
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&body).map_err(|e| {
            UpstreamError::Channel(format!("geminicli loadCodeAssist serialize: {e}"))
        })?)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli loadCodeAssist: {e}")))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli loadCodeAssist body: {e}")))?;
    let payload = serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("geminicli loadCodeAssist parse: {e}")))?;
    Ok(Some(payload))
}

async fn onboard_geminicli_project(
    client: &wreq::Client,
    access_token: &str,
    base_url: &str,
    tier_id: &str,
    project_id: Option<&str>,
) -> Result<Option<String>, UpstreamError> {
    let url = format!("{}/v1internal:onboardUser", base_url.trim_end_matches('/'));
    let body = json!({
        "tierId": tier_id,
        "cloudaicompanionProject": project_id,
        "metadata": geminicli_code_assist_metadata(project_id),
    });
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(&body)
                .map_err(|e| UpstreamError::Channel(format!("geminicli onboard serialize: {e}")))?,
        )
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli onboard: {e}")))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("geminicli onboard body: {e}")))?;
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("geminicli onboard parse: {e}")))?;
    Ok(payload
        .get("response")
        .and_then(|value| value.get("cloudaicompanionProject"))
        .and_then(parse_project_id_value))
}

async fn resolve_geminicli_project_id(
    client: &wreq::Client,
    access_token: &str,
    base_url: &str,
    project_id: Option<&str>,
) -> Result<String, UpstreamError> {
    if let Some(payload) =
        load_geminicli_code_assist(client, access_token, base_url, project_id).await?
        && let Some(project) = payload
            .get("cloudaicompanionProject")
            .and_then(parse_project_id_value)
    {
        return Ok(project);
    }

    if let Some(project) =
        onboard_geminicli_project(client, access_token, base_url, "legacy-tier", project_id).await?
    {
        return Ok(project);
    }

    project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            UpstreamError::Channel("geminicli oauth callback: missing project_id".to_string())
        })
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeminiCliSettings {
    #[serde(default = "default_geminicli_base_url")]
    pub base_url: String,

    /// Explicit user-agent override.  When set, this takes precedence over the
    /// dynamic UA template built from the component fields below.


    #[serde(default = "default_geminicli_api_version")]
    pub api_version: String,
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

fn default_geminicli_base_url() -> String {
    "https://cloudcode-pa.googleapis.com".to_string()
}

fn default_geminicli_api_version() -> String {
    "v1internal".to_string()
}

impl GeminiCliSettings {
    /// Build the dynamic User-Agent string.
    ///
    /// Template: `GeminiCLI/{version}/{model} ({platform}; {arch}; {surface})`
    fn build_user_agent(&self, model: &str) -> String {
        format!(
            "GeminiCLI/{}/{} ({}; {}; {})",
            DEFAULT_GEMINI_CLI_VERSION,
            model,
            DEFAULT_GEMINI_CLI_PLATFORM,
            DEFAULT_GEMINI_CLI_ARCH,
            DEFAULT_GEMINI_CLI_SURFACE,
        )
    }
}

fn build_x_goog_api_client() -> String {
    format!(
        "google-genai-sdk/{} gl-node/{}",
        DEFAULT_GOOGLE_GENAI_SDK_VERSION, DEFAULT_GL_NODE_VERSION
    )
}

fn strip_geminicli_unsupported_generation_config(body: &mut Value) {
    let Some(generation_config) = body
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    generation_config.remove("logprobs");
    generation_config.remove("responseLogprobs");
    generation_config.remove("maxOutputTokens");
}

impl ChannelSettings for GeminiCliSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeminiCliCredential {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at_ms: u64,
    pub project_id: String,
    #[serde(default = "default_geminicli_client_id")]
    pub client_id: String,
    #[serde(default = "default_geminicli_client_secret")]
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
}

fn default_geminicli_client_id() -> String {
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com".to_string()
}

fn default_geminicli_client_secret() -> String {
    "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".to_string()
}

impl ChannelCredential for GeminiCliCredential {
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

// ---------------------------------------------------------------------------
// Channel implementation
// ---------------------------------------------------------------------------

const DEFAULT_MODEL: &str = "gemini-2.5-pro";

impl Channel for GeminiCliChannel {
    const ID: &'static str = "geminicli";
    type Settings = GeminiCliSettings;
    type Credential = GeminiCliCredential;
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
            // Live API
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
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

        for (key, imp) in routes {
            t.set(key, imp);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        geminicli_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let is_model_op = matches!(
            request.route.operation,
            OperationFamily::ModelList | OperationFamily::ModelGet
        );

        // For ModelList/ModelGet, use a quota request body instead of envelope wrapping.
        let (method, final_body) = if is_model_op {
            let quota_body = serde_json::to_vec(&json!({
                "project": credential.project_id.trim(),
            }))
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
            (http::Method::POST, quota_body)
        } else {
            let wrapped = code_assist_envelope::wrap_request(
                &request.body,
                request.model.as_deref(),
                &credential.project_id,
            )?;
            (request.method.clone(), wrapped)
        };

        // --- User-Agent ---
        // If the operator explicitly set `user_agent` in settings, honour that.
        // Otherwise build the dynamic Gemini CLI UA from the component fields.
        let user_agent = match settings.user_agent() {
            Some(ua) => ua.to_string(),
            None => {
                let model = request.model.as_deref().unwrap_or(DEFAULT_MODEL);
                settings.build_user_agent(model)
            }
        };

        let url = format!(
            "{}{}",
            settings.base_url(),
            geminicli_request_path(request)?
        );
        let x_goog_api_client = build_x_goog_api_client();

        let mut builder = http::Request::builder()
            .method(method)
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("Accept-Encoding", "gzip")
            .header("User-Agent", &user_agent)
            .header("x-goog-api-client", x_goog_api_client);

        for (key, value) in request.headers.iter() {
            builder = builder.header(key, value);
        }

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
        strip_geminicli_unsupported_generation_config(&mut body_json);
        request.body = serde_json::to_vec(&body_json)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(request)
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        match request.route.operation {
            OperationFamily::ModelList => quota_to_model_list_response(&body),
            OperationFamily::ModelGet => {
                let target = request.model.as_deref().unwrap_or_default();
                quota_to_model_get_response(&body, target)
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
            401 | 403 => ResponseClassification::AuthDead,
            429 | 499 => classify_google_quota_response(headers, body),
            500..=599 => ResponseClassification::TransientError,
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
            "{}/v1internal:retrieveUserQuota",
            settings.base_url().trim_end_matches('/')
        );
        let user_agent = match settings.user_agent() {
            Some(ua) => ua.to_string(),
            None => settings.build_user_agent(DEFAULT_MODEL),
        };
        let body = serde_json::to_vec(&serde_json::json!({
            "project": credential.project_id.trim(),
        }))
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
        let client = client.clone();
        let span = tracing::info_span!("refresh_credential", channel = "geminicli");
        async move {
            if credential.refresh_token.is_empty() {
                return Ok(false);
            }
            let result = oauth2_refresh::refresh_oauth2_token(
                &client,
                "https://oauth2.googleapis.com/token",
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

    fn oauth_start<'a>(
        &'a self,
        _client: &'a wreq::Client,
        _settings: &'a Self::Settings,
        params: &'a BTreeMap<String, String>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<OAuthFlow>, UpstreamError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let now_unix_ms = crate::utils::oauth::current_unix_ms();
            prune_geminicli_oauth_states(now_unix_ms);

            let redirect_uri = crate::utils::oauth::parse_query_value(params, "redirect_uri")
                .unwrap_or_else(|| GEMINICLI_REDIRECT_URI.to_string());
            let project_id = crate::utils::oauth::parse_query_value(params, "project_id");
            let state = crate::utils::oauth::generate_state();
            let code_verifier = crate::utils::oauth::generate_code_verifier();
            let code_challenge = crate::utils::oauth::generate_code_challenge(&code_verifier);
            let authorize_url =
                build_geminicli_authorize_url(&redirect_uri, &state, &code_challenge);

            geminicli_oauth_states().insert(
                state.clone(),
                GeminiCliOAuthState {
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
                scope: Some(GEMINICLI_OAUTH_SCOPE.to_string()),
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

            prune_geminicli_oauth_states(crate::utils::oauth::current_unix_ms());
            let (code, state_param) = crate::utils::oauth::resolve_code_and_state(params)
                .map_err(|e| UpstreamError::Channel(format!("geminicli oauth callback: {e}")))?;
            let state_id = state_param.ok_or_else(|| {
                UpstreamError::Channel("geminicli oauth callback: missing state".to_string())
            })?;
            let (_, oauth_state) = geminicli_oauth_states()
                .remove(state_id.as_str())
                .ok_or_else(|| {
                    UpstreamError::Channel("geminicli oauth callback: missing state".to_string())
                })?;

            let token = exchange_geminicli_code_for_tokens(
                client,
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
                        "geminicli oauth callback: missing access_token".to_string(),
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
                        "geminicli oauth callback: missing refresh_token".to_string(),
                    )
                })?
                .to_string();
            let project_id = resolve_geminicli_project_id(
                client,
                &access_token,
                settings.base_url(),
                oauth_state.project_id.as_deref(),
            )
            .await?;
            let user_email = fetch_geminicli_user_email(client, &access_token).await?;
            let expires_at_ms = crate::utils::oauth::current_unix_ms()
                .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000));

            Ok(Some(OAuthCredentialResult {
                credential: GeminiCliCredential {
                    access_token: access_token.clone(),
                    refresh_token,
                    expires_at_ms,
                    project_id: project_id.clone(),
                    client_id: default_geminicli_client_id(),
                    client_secret: default_geminicli_client_secret(),
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

fn geminicli_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    let model = request.model.as_deref().unwrap_or_default();
    match request.route.operation {
        OperationFamily::ModelList | OperationFamily::ModelGet => {
            Ok("/v1internal:retrieveUserQuota".to_string())
        }
        OperationFamily::CountToken => Ok("/v1internal:countTokens".to_string()),
        OperationFamily::GenerateContent => Ok("/v1internal:generateContent".to_string()),
        OperationFamily::StreamGenerateContent | OperationFamily::GeminiLive => {
            // Code Assist streaming endpoints won't stream server-sent
            // events unless `alt=sse` is explicitly set; without the
            // query param the upstream rejects with
            // `400 INVALID_ARGUMENT: Request contains an invalid argument`.
            Ok("/v1internal:streamGenerateContent?alt=sse".to_string())
        }
        OperationFamily::Embedding => {
            let model = if model.starts_with("models/") {
                model.to_string()
            } else {
                format!("models/{model}")
            };
            Ok(format!("/v1beta/{model}:embedContent"))
        }
        _ => Err(UpstreamError::Channel(format!(
            "unsupported geminicli request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

fn geminicli_dispatch_table() -> DispatchTable {
    GeminiCliChannel.dispatch_table()
}

// ---------------------------------------------------------------------------
// Model list/get from retrieveUserQuota response
// ---------------------------------------------------------------------------

/// Extract unique models from the `buckets` array in a `retrieveUserQuota` response.
fn models_from_quota_buckets(payload: &Value) -> Vec<Value> {
    let Some(buckets) = payload.get("buckets").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();
    for bucket in buckets {
        if let Some(token_type) = bucket.get("tokenType").and_then(Value::as_str)
            && token_type != "REQUESTS"
        {
            continue;
        }
        let Some(model_id_raw) = bucket.get("modelId").and_then(Value::as_str) else {
            continue;
        };
        let model_id = model_id_raw.trim().to_string();
        if model_id.is_empty() || !seen.insert(model_id.clone()) {
            continue;
        }
        let model_name = if model_id.starts_with("models/") {
            model_id.clone()
        } else {
            format!("models/{model_id}")
        };
        models.push(json!({
            "name": model_name,
            "baseModelId": model_id,
            "displayName": model_id,
            "description": "Derived from Gemini CLI retrieveUserQuota buckets.",
            "supportedGenerationMethods": [
                "generateContent",
                "streamGenerateContent",
                "countTokens"
            ]
        }));
    }
    models
}

/// Transform a `retrieveUserQuota` response body into a standard Gemini model list response.
fn quota_to_model_list_response(body: &[u8]) -> Vec<u8> {
    let payload: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return body.to_vec(),
    };
    let models = models_from_quota_buckets(&payload);
    serde_json::to_vec(&json!({ "models": models })).unwrap_or_else(|_| body.to_vec())
}

/// Transform a `retrieveUserQuota` response body into a standard Gemini model get response.
fn quota_to_model_get_response(body: &[u8], target: &str) -> Vec<u8> {
    let payload: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return body.to_vec(),
    };
    let models = models_from_quota_buckets(&payload);
    let normalized_target = target.trim().trim_start_matches("models/");
    let found = models.into_iter().find(|m| {
        m.get("name")
            .and_then(Value::as_str)
            .map(|n| n.trim_start_matches("models/") == normalized_target)
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

inventory::submit! { ChannelRegistration::new(GeminiCliChannel::ID, geminicli_dispatch_table) }
