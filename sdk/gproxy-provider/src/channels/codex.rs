use std::collections::BTreeMap;
use std::sync::OnceLock;

use base64::Engine as _;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::channel::{
    Channel, ChannelCredential, ChannelSettings, OAuthCredentialResult, OAuthFlow,
};
use crate::count_tokens::CountStrategy;
use crate::dispatch::{DispatchTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::utils::oauth2_refresh;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use tracing::Instrument;

/// Codex CLI channel (OpenAI Responses API with OAuth).
pub struct CodexChannel;

const DEFAULT_CODEX_ORIGINATOR: &str = "codex_cli_rs";
const DEFAULT_CODEX_VERSION: &str = "0.118.0";
const DEFAULT_CODEX_OS_TYPE: &str = "Linux";
const DEFAULT_CODEX_OS_VERSION: &str = "6.6";
const DEFAULT_CODEX_ARCH: &str = "x86_64";
const CODEX_SESSION_NAMESPACE: uuid::Uuid = uuid::uuid!("aef2ff08-4585-5e42-a831-1cb53cb6ea8d");
const CODEX_OAUTH_ISSUER: &str = "https://auth.openai.com";
const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_OAUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CODEX_OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const CODEX_OAUTH_ORIGINATOR: &str = "codex_vscode";
const CODEX_OAUTH_STATE_TTL_MS: u64 = 600_000;

#[derive(Debug, Clone)]
struct CodexOAuthState {
    code_verifier: String,
    redirect_uri: String,
    issuer: String,
    created_at_unix_ms: u64,
}

#[derive(Debug, Deserialize)]
struct CodexTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Default)]
struct CodexIdTokenClaims {
    email: Option<String>,
    account_id: Option<String>,
}

fn codex_oauth_states() -> &'static DashMap<String, CodexOAuthState> {
    static STATES: OnceLock<DashMap<String, CodexOAuthState>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn prune_codex_oauth_states(now_unix_ms: u64) {
    let expired = codex_oauth_states()
        .iter()
        .filter_map(|entry| {
            (now_unix_ms.saturating_sub(entry.value().created_at_unix_ms)
                > CODEX_OAUTH_STATE_TTL_MS)
                .then(|| entry.key().clone())
        })
        .collect::<Vec<_>>();
    for key in expired {
        codex_oauth_states().remove(key.as_str());
    }
}

fn codex_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING
        .get_or_init(|| crate::billing::parse_model_prices_json(include_str!("pricing/codex.json")))
}

fn build_codex_authorize_url(
    issuer: &str,
    redirect_uri: &str,
    scope: &str,
    originator: &str,
    code_challenge: &str,
    state: &str,
) -> String {
    let query = [
        ("response_type", "code".to_string()),
        ("client_id", CODEX_OAUTH_CLIENT_ID.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("scope", scope.to_string()),
        ("code_challenge", code_challenge.to_string()),
        ("code_challenge_method", "S256".to_string()),
        ("id_token_add_organizations", "true".to_string()),
        ("codex_cli_simplified_flow", "true".to_string()),
        ("state", state.to_string()),
        ("originator", originator.to_string()),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key}={}", crate::utils::oauth::percent_encode(&value)))
    .collect::<Vec<_>>()
    .join("&");
    format!("{}/oauth/authorize?{query}", issuer.trim_end_matches('/'))
}

async fn exchange_codex_code_for_tokens(
    client: &wreq::Client,
    issuer: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
) -> Result<CodexTokenResponse, UpstreamError> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        crate::utils::oauth::percent_encode(code),
        crate::utils::oauth::percent_encode(redirect_uri),
        crate::utils::oauth::percent_encode(CODEX_OAUTH_CLIENT_ID),
        crate::utils::oauth::percent_encode(code_verifier),
    );

    let response = client
        .post(format!("{}/oauth/token", issuer.trim_end_matches('/')))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("codex oauth token: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("codex oauth body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "codex oauth token endpoint status {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("codex oauth token parse: {e}")))
}

fn parse_codex_id_token_claims(id_token: &str) -> CodexIdTokenClaims {
    let mut claims = CodexIdTokenClaims::default();
    let mut parts = id_token.split('.');
    let _header = parts.next();
    let Some(payload_b64) = parts.next() else {
        return claims;
    };
    let Some(_signature) = parts.next() else {
        return claims;
    };

    let Ok(payload_bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64)
    else {
        return claims;
    };
    let Ok(payload) = serde_json::from_slice::<Value>(&payload_bytes) else {
        return claims;
    };

    claims.email = payload
        .get("email")
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("https://api.openai.com/profile")
                .and_then(|profile| profile.get("email"))
                .and_then(Value::as_str)
        })
        .map(ToOwned::to_owned);
    claims.account_id = payload
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    claims
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSettings {
    #[serde(default = "default_codex_base_url")]
    pub base_url: String,
    /// Explicit override for the entire User-Agent header.
    /// When set, this takes priority over the computed Codex UA string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries_on_429: Option<u32>,
}

impl Default for CodexSettings {
    fn default() -> Self {
        Self {
            base_url: default_codex_base_url(),
            user_agent: None,
            max_retries_on_429: None,
        }
    }
}

fn default_codex_base_url() -> String {
    "https://chatgpt.com/backend-api/codex".to_string()
}

impl CodexSettings {
    /// Build the default Codex CLI user-agent string.
    fn computed_user_agent(&self) -> String {
        let prefix = format!(
            "{}/{} ({} {}; {})",
            DEFAULT_CODEX_ORIGINATOR,
            DEFAULT_CODEX_VERSION,
            DEFAULT_CODEX_OS_TYPE,
            DEFAULT_CODEX_OS_VERSION,
            DEFAULT_CODEX_ARCH
        );
        let terminal_token = codex_terminal_user_agent();
        if terminal_token.is_empty() {
            prefix
        } else {
            format!("{prefix} {terminal_token}")
        }
    }

    /// Return the effective User-Agent: explicit override wins, otherwise computed.
    fn effective_user_agent(&self) -> String {
        match &self.user_agent {
            Some(ua) => ua.clone(),
            None => self.computed_user_agent(),
        }
    }
}

fn is_codex_managed_header(name: &http::HeaderName) -> bool {
    matches!(name.as_str(), "x-client-request-id" | "session_id")
}

fn is_codex_user_agent(request: &PreparedRequest) -> bool {
    request
        .headers
        .get(http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("codex"))
}

fn normalize_codex_model_entry(model: &Value) -> Option<Value> {
    let id = model
        .get("slug")
        .or_else(|| model.get("id"))
        .and_then(Value::as_str)?
        .to_string();

    Some(json!({
        "id": id,
        "created": 0,
        "object": "model",
        "owned_by": "openai"
    }))
}

fn normalize_codex_model_list_response(body: Vec<u8>) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };

    let Some(models) = value.get("models").and_then(Value::as_array) else {
        return body;
    };

    let normalized_models: Vec<Value> = models
        .iter()
        .filter_map(normalize_codex_model_entry)
        .collect();

    serde_json::to_vec(&json!({
        "object": "list",
        "data": normalized_models,
    }))
    .unwrap_or(body)
}

fn requested_codex_model_id(request: &PreparedRequest) -> Option<String> {
    let body = serde_json::from_slice::<Value>(&request.body).ok()?;
    body.get("path")
        .and_then(|path| path.get("model"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_codex_model_get_response(request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };

    if let Some(model) = normalize_codex_model_entry(&value) {
        return serde_json::to_vec(&model).unwrap_or(body);
    }

    let Some(models) = value.get("models").and_then(Value::as_array) else {
        return body;
    };

    let requested_id = requested_codex_model_id(request);
    let selected = requested_id.as_deref().and_then(|target| {
        models.iter().find(|model| {
            model
                .get("slug")
                .or_else(|| model.get("id"))
                .and_then(Value::as_str)
                .is_some_and(|id| id == target)
        })
    });

    let selected = selected.or_else(|| models.first());
    let Some(model) = selected.and_then(normalize_codex_model_entry) else {
        return body;
    };

    serde_json::to_vec(&model).unwrap_or(body)
}

fn codex_terminal_user_agent() -> String {
    let token = if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        let version = std::env::var("TERM_PROGRAM_VERSION").ok();
        match version.as_deref().filter(|v| !v.is_empty()) {
            Some(version) => format!("{term_program}/{version}"),
            None => term_program,
        }
    } else if let Ok(term) = std::env::var("TERM") {
        term
    } else {
        "unknown".to_string()
    };

    token
        .chars()
        .map(|ch| if matches!(ch, ' '..='~') { ch } else { '_' })
        .collect::<String>()
        .trim()
        .to_string()
}

fn request_session_id(request: &PreparedRequest) -> String {
    if let Some(session_id) = request
        .headers
        .get("session_id")
        .or_else(|| request.headers.get("x-client-request-id"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
    {
        return session_id.to_owned();
    }

    let body = serde_json::from_slice::<Value>(&request.body).unwrap_or(Value::Null);
    let session_seed = format!(
        "{}\n{}\n{}",
        codex_instructions_fingerprint(&body),
        codex_first_input_fingerprint(&body),
        format!("{}/{}", request.route.operation, request.route.protocol)
    );
    Uuid::new_v5(&CODEX_SESSION_NAMESPACE, session_seed.as_bytes()).to_string()
}

fn codex_instructions_fingerprint(body: &Value) -> String {
    body.get("instructions")
        .map(json_fingerprint_text)
        .unwrap_or_default()
}

fn codex_first_input_fingerprint(body: &Value) -> String {
    match body.get("input") {
        Some(Value::Array(items)) => items.first().map(json_fingerprint_text).unwrap_or_default(),
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

fn normalize_codex_request_body(body: &[u8], is_stream: bool) -> Vec<u8> {
    let Ok(mut body_json) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(map) = body_json.as_object_mut() else {
        return body.to_vec();
    };

    map.insert("store".to_string(), Value::Bool(false));
    map.remove("max_output_tokens");
    map.remove("metadata");
    map.remove("stream_options");
    map.remove("temperature");
    map.remove("top_p");
    map.remove("top_logprobs");
    map.remove("safety_identifier");
    map.remove("truncation");
    map.insert("stream".to_string(), Value::Bool(is_stream));

    if map
        .get("instructions")
        .is_some_and(|value| !value.is_string())
    {
        map.insert("instructions".to_string(), Value::String(String::new()));
    }

    if !map.contains_key("instructions") {
        map.insert("instructions".to_string(), Value::String(String::new()));
    }

    if let Some(input) = map.get("input")
        && let Some(text) = input.as_str()
    {
        map.insert(
            "input".to_string(),
            json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": text,
                }
            ]),
        );
    }

    serde_json::to_vec(&body_json).unwrap_or_else(|_| body.to_vec())
}

impl ChannelSettings for CodexSettings {
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
pub struct CodexCredential {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default)]
    pub expires_at_ms: u64,
}

impl ChannelCredential for CodexCredential {
    fn apply_update(&mut self, update: &serde_json::Value) -> bool {
        if let Some(token) = update.get("access_token").and_then(|v| v.as_str()) {
            self.access_token = token.to_string();
            if let Some(exp) = update.get("expires_at_ms").and_then(|v| v.as_u64()) {
                self.expires_at_ms = exp;
            }
            if let Some(rt) = update.get("refresh_token").and_then(|v| v.as_str()) {
                self.refresh_token = Some(rt.to_string());
            }
            true
        } else {
            false
        }
    }
}

impl Channel for CodexChannel {
    const ID: &'static str = "codex";
    type Settings = CodexSettings;
    type Credential = CodexCredential;
    type Health = ModelCooldownHealth;

    fn dispatch_table(&self) -> DispatchTable {
        // Native Codex traffic uses the Responses API, but the proxy can still
        // transform other request protocols into openai_response.
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

        let routes: Vec<(RouteKey, RouteImplementation)> = vec![
            // Model list/get
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

            // Generate content (internally force stream, then aggregate back)
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // Generate content (stream)
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
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
            // WebSocket
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
            // Images — route through Responses API
            xform(
                OperationFamily::CreateImage,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamCreateImage,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::CreateImageEdit,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            xform(
                OperationFamily::StreamCreateImageEdit,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            // Embeddings
            pass(OperationFamily::Embedding, ProtocolKind::OpenAi),
            xform(
                OperationFamily::Embedding,
                ProtocolKind::Gemini,
                OperationFamily::Embedding,
                ProtocolKind::OpenAi,
            ),
            // Compact
            pass(OperationFamily::Compact, ProtocolKind::OpenAi),
        ];

        for (key, imp) in routes {
            t.set(key, imp);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        codex_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let url = format!("{}{}", settings.base_url(), codex_request_path(request)?);
        let session_id = request_session_id(request);
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Content-Type", "application/json")
            .header("User-Agent", settings.effective_user_agent())
            .header("originator", DEFAULT_CODEX_ORIGINATOR)
            .header("x-client-request-id", &session_id)
            .header("session_id", &session_id);

        if let Some(account_id) = &credential.account_id
            && !account_id.is_empty()
        {
            builder = builder.header("chatgpt-account-id", account_id.as_str());
        }

        // Forward caller-provided headers (x-codex-turn-state, x-codex-turn-metadata,
        // x-codex-beta-features, OpenAI-Organization, OpenAI-Project, etc.)
        // Keep conversation identity authoritative: upstream expects both
        // x-client-request-id and session_id to equal the same conversation id.
        for (key, value) in request.headers.iter() {
            if is_codex_managed_header(key) {
                continue;
            }
            builder = builder.header(key, value);
        }

        builder
            .body(request.body.clone())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn finalize_request(
        &self,
        _settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        request.body = match request.route.operation {
            OperationFamily::GenerateContent => normalize_codex_request_body(&request.body, false),
            OperationFamily::StreamGenerateContent => {
                normalize_codex_request_body(&request.body, true)
            }
            _ => request.body,
        };
        Ok(request)
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        match request.route.operation {
            OperationFamily::ModelList if is_codex_user_agent(request) => body,
            OperationFamily::ModelList => normalize_codex_model_list_response(body),
            OperationFamily::ModelGet => normalize_codex_model_get_response(request, body),
            _ => body,
        }
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
                // Credits exhausted → credential is dead.
                if headers
                    .get("x-codex-credits-has-credits")
                    .and_then(|v| v.to_str().ok())
                    == Some("false")
                {
                    return ResponseClassification::AuthDead;
                }
                let retry_after_ms = parse_codex_rate_limit(headers);
                ResponseClassification::RateLimited { retry_after_ms }
            }
            500..=599 => ResponseClassification::TransientError,
            _ => ResponseClassification::PermanentError,
        }
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::Local
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

    fn ws_extra_headers(&self) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "OpenAI-Beta",
            http::HeaderValue::from_static("responses_websockets=2026-02-06"),
        );
        headers
    }

    fn prepare_quota_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        let base = settings.base_url().trim_end_matches('/');
        let base = base.strip_suffix("/codex").unwrap_or(base);
        let url = format!("{base}/wham/usage");
        let user_agent = settings.effective_user_agent();
        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Accept", "application/json")
            .header("originator", DEFAULT_CODEX_ORIGINATOR)
            .header("User-Agent", &user_agent);
        if let Some(account_id) = &credential.account_id {
            builder = builder.header("chatgpt-account-id", account_id.as_str());
        }
        let req = builder
            .body(Vec::new())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(Some(req))
    }

    fn refresh_credential<'a>(
        &'a self,
        client: &'a wreq::Client,
        credential: &'a mut Self::Credential,
    ) -> impl std::future::Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        let client = client.clone();
        let span = tracing::info_span!("refresh_credential", channel = "codex");
        async move {
            let refresh_token = match &credential.refresh_token {
                Some(rt) if !rt.is_empty() => rt.clone(),
                _ => return Ok(false),
            };
            let result = oauth2_refresh::refresh_oauth2_token(
                &client,
                "https://auth.openai.com/oauth/token",
                "",
                "",
                &refresh_token,
            )
            .await?;
            credential.access_token = result.access_token;
            credential.expires_at_ms = result.expires_at_ms;
            if let Some(rt) = result.refresh_token {
                credential.refresh_token = Some(rt);
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
            prune_codex_oauth_states(now_unix_ms);

            let issuer = crate::utils::oauth::parse_query_value(params, "oauth_issuer")
                .or_else(|| crate::utils::oauth::parse_query_value(params, "issuer"))
                .unwrap_or_else(|| CODEX_OAUTH_ISSUER.to_string());
            let redirect_uri = crate::utils::oauth::parse_query_value(params, "redirect_uri")
                .unwrap_or_else(|| CODEX_OAUTH_REDIRECT_URI.to_string());
            let scope = crate::utils::oauth::parse_query_value(params, "scope")
                .unwrap_or_else(|| CODEX_OAUTH_SCOPE.to_string());
            let originator = crate::utils::oauth::parse_query_value(params, "originator")
                .unwrap_or_else(|| CODEX_OAUTH_ORIGINATOR.to_string());
            let state = crate::utils::oauth::generate_state();
            let code_verifier = crate::utils::oauth::generate_code_verifier();
            let code_challenge = crate::utils::oauth::generate_code_challenge(&code_verifier);
            let authorize_url = build_codex_authorize_url(
                &issuer,
                &redirect_uri,
                &scope,
                &originator,
                &code_challenge,
                &state,
            );

            codex_oauth_states().insert(
                state.clone(),
                CodexOAuthState {
                    code_verifier,
                    redirect_uri: redirect_uri.clone(),
                    issuer,
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
                scope: Some(scope),
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
        _settings: &'a Self::Settings,
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

            prune_codex_oauth_states(crate::utils::oauth::current_unix_ms());
            let (code, state_param) = crate::utils::oauth::resolve_code_and_state(params)
                .map_err(|e| UpstreamError::Channel(format!("codex oauth callback: {e}")))?;
            let state_id = state_param.ok_or_else(|| {
                UpstreamError::Channel("codex oauth callback: missing state".to_string())
            })?;
            let (_, oauth_state) =
                codex_oauth_states()
                    .remove(state_id.as_str())
                    .ok_or_else(|| {
                        UpstreamError::Channel("codex oauth callback: missing state".to_string())
                    })?;

            let token = exchange_codex_code_for_tokens(
                client,
                &oauth_state.issuer,
                &oauth_state.redirect_uri,
                &oauth_state.code_verifier,
                &code,
            )
            .await?;

            let access_token = token
                .access_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel("codex oauth callback: missing access_token".to_string())
                })?
                .to_string();
            let refresh_token = token
                .refresh_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel(
                        "codex oauth callback: missing refresh_token".to_string(),
                    )
                })?
                .to_string();
            let id_token = token
                .id_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel("codex oauth callback: missing id_token".to_string())
                })?
                .to_string();
            let claims = parse_codex_id_token_claims(&id_token);
            let account_id = claims.account_id.ok_or_else(|| {
                UpstreamError::Channel("codex oauth callback: missing account_id".to_string())
            })?;
            let expires_at_ms = crate::utils::oauth::current_unix_ms()
                .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000));

            Ok(Some(OAuthCredentialResult {
                credential: CodexCredential {
                    access_token: access_token.clone(),
                    refresh_token: Some(refresh_token.clone()),
                    id_token: Some(id_token.clone()),
                    user_email: claims.email.clone(),
                    account_id: Some(account_id.clone()),
                    expires_at_ms,
                },
                details: json!({
                    "access_token": access_token,
                    "refresh_token": refresh_token,
                    "id_token": id_token,
                    "account_id": account_id,
                    "user_email": claims.email,
                    "expires_at_ms": expires_at_ms,
                }),
            }))
        })
    }
}
/// Parse Codex rate-limit headers into a single `retry_after_ms`.
///
/// Picks the smallest reset window first (primary before secondary) so the
/// credential is retried as early as possible.  Falls back to `retry-after`.
fn parse_codex_rate_limit(headers: &http::HeaderMap) -> Option<u64> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let delay_from_reset = |name: &str| -> Option<u64> {
        let reset_secs = headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())?;
        let reset_ms = reset_secs.saturating_mul(1000);
        (reset_ms > now_ms).then(|| reset_ms - now_ms)
    };

    // Prefer the smallest (primary) window, then secondary.
    if let Some(ms) = delay_from_reset("x-codex-primary-reset-at") {
        return Some(ms);
    }
    if let Some(ms) = delay_from_reset("x-codex-secondary-reset-at") {
        return Some(ms);
    }
    // Fallback: standard retry-after (seconds).
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 1000)
}

fn codex_dispatch_table() -> DispatchTable {
    CodexChannel.dispatch_table()
}

inventory::submit! { ChannelRegistration::new(CodexChannel::ID, codex_dispatch_table) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize_request_normalizes_v1_responses_as_streaming() {
        let channel = CodexChannel;
        let settings = CodexSettings::default();
        let request = PreparedRequest {
            method: http::Method::POST,
            route: RouteKey::new(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
            ),
            model: Some("gpt-5.4".to_string()),
            body: serde_json::to_vec(&json!({
                "model": "gpt-5.4",
                "input": "hi",
                "stream": false
            }))
            .expect("serialize request"),
            headers: http::HeaderMap::new(),
        };

        let finalized = channel
            .finalize_request(&settings, request)
            .expect("finalize request");
        let body_json: Value =
            serde_json::from_slice(&finalized.body).expect("finalized request json");

        assert_eq!(body_json.get("stream").and_then(Value::as_bool), Some(true));
        assert_eq!(body_json.get("store").and_then(Value::as_bool), Some(false));
    }
}

fn codex_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::ModelList | OperationFamily::ModelGet => {
            Ok(format!("/models?client_version={DEFAULT_CODEX_VERSION}"))
        }
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            Ok("/responses".to_string())
        }
        OperationFamily::Compact => Ok("/responses/compact".to_string()),
        OperationFamily::OpenAiResponseWebSocket => Ok("/responses".to_string()),
        _ => Err(UpstreamError::Channel(format!(
            "unsupported codex request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}
