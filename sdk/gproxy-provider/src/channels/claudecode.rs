use std::collections::BTreeMap;
use std::sync::OnceLock;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
use crate::utils::claude_cache_control as cache_control;
use crate::utils::oauth2_refresh;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use tracing::Instrument;

/// Claude Code channel (Anthropic Messages API with OAuth).
pub struct ClaudeCodeChannel;

const DEFAULT_CLAUDECODE_VERSION: &str = "2.1.89";
const DEFAULT_CLAUDECODE_ENTRYPOINT: &str = "cli";
const DEFAULT_CLAUDECODE_USER_TYPE: &str = "external";
const BILLING_HASH_SALT: &str = "59cf53e54c78";
const BILLING_CCH_HEX_LEN: usize = 5;
const BILLING_VERSION_HASH_LEN: usize = 3;
const BILLING_VERSION_CHAR_OFFSETS: [usize; 3] = [4, 7, 20];
const CLAUDECODE_SESSION_NAMESPACE: uuid::Uuid =
    uuid::uuid!("f348ca5a-091f-5e75-aec7-c6d7c1b8c3d6");
const CLAUDECODE_CLAUDE_AI_BASE_URL: &str = "https://claude.ai";
const CLAUDECODE_REDIRECT_URI: &str = "https://platform.claude.com/oauth/code/callback";
const CLAUDECODE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDECODE_OAUTH_SCOPE: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
const CLAUDECODE_OAUTH_BETA: &str = "oauth-2025-04-20";
const CLAUDECODE_API_VERSION: &str = "2023-06-01";
const CLAUDECODE_OAUTH_STATE_TTL_MS: u64 = 600_000;
const CLAUDECODE_TOKEN_UA: &str = "claude-cli/2.1.89 (external, cli)";
const CLAUDECODE_PROFILE_UA: &str = "claude-code/2.1.89";

fn claudecode_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/claudecode.json"))
    })
}

#[derive(Debug, Clone)]
struct ClaudeCodeOAuthState {
    code_verifier: String,
    redirect_uri: String,
    api_base_url: String,
    claude_ai_base_url: String,
    created_at_unix_ms: u64,
}

#[derive(Debug, Deserialize)]
struct ClaudeCodeTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    #[serde(default)]
    subscription_type: Option<String>,
    #[serde(default)]
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeCodeOAuthProfileAccount {
    uuid: Option<String>,
    email: Option<String>,
    #[serde(default)]
    has_claude_max: bool,
    #[serde(default)]
    has_claude_pro: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeCodeOAuthProfileOrg {
    organization_type: Option<String>,
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeCodeOAuthProfile {
    #[serde(default)]
    account: ClaudeCodeOAuthProfileAccount,
    #[serde(default)]
    organization: ClaudeCodeOAuthProfileOrg,
}

fn claudecode_oauth_states() -> &'static DashMap<String, ClaudeCodeOAuthState> {
    static STATES: OnceLock<DashMap<String, ClaudeCodeOAuthState>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn prune_claudecode_oauth_states(now_unix_ms: u64) {
    let expired = claudecode_oauth_states()
        .iter()
        .filter_map(|entry| {
            (now_unix_ms.saturating_sub(entry.value().created_at_unix_ms)
                > CLAUDECODE_OAUTH_STATE_TTL_MS)
                .then(|| entry.key().clone())
        })
        .collect::<Vec<_>>();
    for key in expired {
        claudecode_oauth_states().remove(key.as_str());
    }
}

fn build_claudecode_authorize_url(
    claude_ai_base_url: &str,
    redirect_uri: &str,
    scope: &str,
    code_challenge: &str,
    state: &str,
) -> String {
    let query = [
        ("code", "true".to_string()),
        ("client_id", CLAUDECODE_OAUTH_CLIENT_ID.to_string()),
        ("response_type", "code".to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("scope", scope.to_string()),
        ("code_challenge", code_challenge.to_string()),
        ("code_challenge_method", "S256".to_string()),
        ("state", state.to_string()),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key}={}", crate::utils::oauth::percent_encode(&value)))
    .collect::<Vec<_>>()
    .join("&");
    format!(
        "{}/oauth/authorize?{query}",
        claude_ai_base_url.trim_end_matches('/')
    )
}

async fn exchange_claudecode_code_for_tokens(
    client: &wreq::Client,
    api_base_url: &str,
    claude_ai_base_url: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
    state: &str,
) -> Result<ClaudeCodeTokenResponse, UpstreamError> {
    let body = format!(
        "grant_type=authorization_code&client_id={}&code={}&redirect_uri={}&code_verifier={}&state={}",
        crate::utils::oauth::percent_encode(CLAUDECODE_OAUTH_CLIENT_ID),
        crate::utils::oauth::percent_encode(code),
        crate::utils::oauth::percent_encode(redirect_uri),
        crate::utils::oauth::percent_encode(code_verifier),
        crate::utils::oauth::percent_encode(state),
    );
    let response = client
        .post(format!(
            "{}/v1/oauth/token",
            api_base_url.trim_end_matches('/')
        ))
        .header("anthropic-version", CLAUDECODE_API_VERSION)
        .header("anthropic-beta", CLAUDECODE_OAUTH_BETA)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json, text/plain, */*")
        .header("origin", claude_ai_base_url)
        .header("user-agent", CLAUDECODE_TOKEN_UA)
        .body(body)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth token: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "claudecode oauth token endpoint status {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("claudecode oauth token parse: {e}")))
}

async fn fetch_claudecode_oauth_profile(
    client: &wreq::Client,
    api_base_url: &str,
    access_token: &str,
) -> Result<ClaudeCodeOAuthProfile, UpstreamError> {
    let response = client
        .get(format!(
            "{}/api/oauth/profile",
            api_base_url.trim_end_matches('/')
        ))
        .header("authorization", format!("Bearer {access_token}"))
        .header("user-agent", CLAUDECODE_PROFILE_UA)
        .header("accept", "application/json")
        .header("anthropic-beta", CLAUDECODE_OAUTH_BETA)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth profile: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth profile body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "claudecode oauth profile status {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("claudecode oauth profile parse: {e}")))
}

// ---------------------------------------------------------------------------
// Default-value helpers
// ---------------------------------------------------------------------------

fn default_claudecode_base_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn default_claudecode_platform_base_url() -> String {
    "https://platform.claude.com".to_string()
}

fn default_claudecode_claude_ai_base_url() -> String {
    "https://claude.ai".to_string()
}

fn default_claudecode_device_id() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("device_id entropy should be available");
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeSettings {
    #[serde(default = "default_claudecode_base_url")]
    pub base_url: String,
    /// Base URL for the platform API (quota / usage endpoint).
    /// Defaults to `https://platform.claude.com`.
    #[serde(default = "default_claudecode_platform_base_url")]
    pub platform_base_url: String,
    /// Base URL for claude.ai (cookie auth, organization discovery).
    /// Defaults to `https://claude.ai`.
    #[serde(default = "default_claudecode_claude_ai_base_url")]
    pub claude_ai_base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries_on_429: Option<u32>,
    #[serde(default)]
    pub enable_magic_cache: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cache_breakpoints: Vec<cache_control::CacheBreakpointRule>,
}

impl Default for ClaudeCodeSettings {
    fn default() -> Self {
        Self {
            base_url: default_claudecode_base_url(),
            platform_base_url: default_claudecode_platform_base_url(),
            claude_ai_base_url: default_claudecode_claude_ai_base_url(),
            user_agent: None,
            max_retries_on_429: None,
            enable_magic_cache: false,
            cache_breakpoints: Vec::new(),
        }
    }
}

impl ChannelSettings for ClaudeCodeSettings {
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

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeCredential {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at_ms: u64,
    #[serde(default = "default_claudecode_device_id")]
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
}

impl Default for ClaudeCodeCredential {
    fn default() -> Self {
        Self {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at_ms: 0,
            device_id: default_claudecode_device_id(),
            account_uuid: None,
            subscription_type: None,
            rate_limit_tier: None,
            cookie: None,
            user_email: None,
        }
    }
}

impl ChannelCredential for ClaudeCodeCredential {
    fn apply_update(&mut self, update: &serde_json::Value) -> bool {
        if let Some(token) = update.get("access_token").and_then(|v| v.as_str()) {
            self.access_token = token.to_string();
            if let Some(exp) = update.get("expires_at_ms").and_then(|v| v.as_u64()) {
                self.expires_at_ms = exp;
            }
            if let Some(rt) = update.get("refresh_token").and_then(|v| v.as_str()) {
                self.refresh_token = rt.to_string();
            }
            if let Some(account_uuid) = update.get("account_uuid").and_then(|v| v.as_str()) {
                self.account_uuid = Some(account_uuid.to_string());
            }
            if let Some(device_id) = update.get("device_id").and_then(|v| v.as_str()) {
                self.device_id = device_id.to_string();
            }
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Body-mutation helpers
// ---------------------------------------------------------------------------

/// Build the `metadata.user_id` JSON string that Claude Code sends.
fn build_metadata_user_id(credential: &ClaudeCodeCredential, session_id: &str) -> String {
    // The value is itself a JSON-encoded string
    serde_json::json!({
        "device_id": credential.device_id.as_str(),
        "account_uuid": credential.account_uuid.as_deref().unwrap_or(""),
        "session_id": session_id,
    })
    .to_string()
}

/// Build the billing attribution text injected as the first system element.
fn build_attribution(user_message: &str) -> String {
    let cch = truncated_sha256_hex(user_message, BILLING_CCH_HEX_LEN);
    let version_hash_input = format!(
        "{}{}{}",
        DEFAULT_CLAUDECODE_VERSION,
        BILLING_HASH_SALT,
        sampled_message_chars(user_message)
    );
    let version_hash = truncated_sha256_hex(&version_hash_input, BILLING_VERSION_HASH_LEN);

    format!(
        "x-anthropic-billing-header: cc_version={}.{}; cc_entrypoint={}; cch={};",
        DEFAULT_CLAUDECODE_VERSION, version_hash, DEFAULT_CLAUDECODE_ENTRYPOINT, cch
    )
}

fn request_session_id(request: &PreparedRequest, body: &Value) -> String {
    if let Some(session_id) = request
        .headers
        .get("x-claude-code-session-id")
        .or_else(|| request.headers.get("session_id"))
        .or_else(|| request.headers.get("x-client-request-id"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
    {
        return session_id.to_owned();
    }

    let session_seed = format!(
        "{}\n{}\n{}",
        system_fingerprint_text(body),
        first_message_fingerprint_text(body),
        request.path
    );
    Uuid::new_v5(&CLAUDECODE_SESSION_NAMESPACE, session_seed.as_bytes()).to_string()
}

fn truncated_sha256_hex(input: &str, hex_len: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();
    let mut hex = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex.chars().take(hex_len).collect()
}

fn sampled_message_chars(user_message: &str) -> String {
    let chars: Vec<char> = user_message.chars().collect();
    BILLING_VERSION_CHAR_OFFSETS
        .iter()
        .map(|index| chars.get(*index).copied().unwrap_or('0'))
        .collect()
}

fn system_fingerprint_text(body: &Value) -> String {
    match body.get("system") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(text_from_content_block)
            .collect::<Vec<_>>()
            .join(""),
        Some(value) => serde_json::to_string(value).unwrap_or_default(),
        None => String::new(),
    }
}

fn text_from_content_block(block: &Value) -> Option<String> {
    if let Some(text) = block.as_str() {
        return Some(text.to_owned());
    }

    let block_type = block.get("type").and_then(Value::as_str)?;
    if block_type != "text" {
        return None;
    }

    block
        .get("text")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn first_message_fingerprint_text(body: &Value) -> String {
    let Some(message) = body
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.first())
    else {
        return String::new();
    };

    serde_json::to_string(message).unwrap_or_default()
}

fn first_user_message_text(body: &Value) -> String {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return String::new();
    };

    let Some(message) = messages.iter().find(|message| {
        message
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role == "user")
    }) else {
        return String::new();
    };

    let Some(content) = message.get("content") else {
        return String::new();
    };

    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(text_from_content_block)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn normalize_claudecode_sampling(body: &mut Value) {
    let Some(map) = body.as_object_mut() else {
        return;
    };

    let has_temperature = map.get("temperature").and_then(Value::as_f64).is_some();
    let has_top_p = map.get("top_p").and_then(Value::as_f64).is_some();
    if has_temperature && has_top_p {
        map.remove("top_p");
    }
}

fn normalize_claudecode_unsupported_fields(body: &mut Value) {
    let Some(map) = body.as_object_mut() else {
        return;
    };

    map.remove("speed");
}

/// Inject `metadata.user_id` into the body JSON.
fn inject_metadata_user_id(body: &mut Value, user_id_value: &str) {
    let metadata = body
        .as_object_mut()
        .expect("body must be an object")
        .entry("metadata")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(m) = metadata.as_object_mut() {
        m.insert(
            "user_id".to_string(),
            Value::String(user_id_value.to_string()),
        );
    }
}

/// Inject the billing attribution as the first element of the `system` array.
///
/// - If `system` is absent, create it as an array with the attribution text block.
/// - If `system` is a string, convert to an array: [attribution, original_text].
/// - If `system` is already an array, prepend the attribution text block.
fn inject_system_attribution(body: &mut Value, attribution: &str) {
    let attribution_block = serde_json::json!({
        "type": "text",
        "text": attribution,
    });

    let obj = body.as_object_mut().expect("body must be an object");

    match obj.get("system") {
        None => {
            obj.insert("system".to_string(), Value::Array(vec![attribution_block]));
        }
        Some(val) if val.is_string() => {
            let original_text = val.as_str().unwrap().to_string();
            let original_block = serde_json::json!({
                "type": "text",
                "text": original_text,
            });
            obj.insert(
                "system".to_string(),
                Value::Array(vec![attribution_block, original_block]),
            );
        }
        Some(val) if val.is_array() => {
            let arr = obj.get_mut("system").unwrap().as_array_mut().unwrap();
            arr.insert(0, attribution_block);
        }
        _ => {
            // system is some other type – overwrite with array
            obj.insert("system".to_string(), Value::Array(vec![attribution_block]));
        }
    }
}

// ---------------------------------------------------------------------------
// Channel implementation
// ---------------------------------------------------------------------------

impl Channel for ClaudeCodeChannel {
    const ID: &'static str = "claudecode";
    type Settings = ClaudeCodeSettings;
    type Credential = ClaudeCodeCredential;
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
            pass(OperationFamily::ModelList, ProtocolKind::Claude),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::OpenAi,
                OperationFamily::ModelList,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
                OperationFamily::ModelList,
                ProtocolKind::Claude,
            ),
            pass(OperationFamily::ModelGet, ProtocolKind::Claude),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::OpenAi,
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::Gemini,
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
            ),
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
            pass(OperationFamily::GenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
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
            pass(OperationFamily::StreamGenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
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
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
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

        for (key, imp) in routes {
            t.set(key, imp);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        claudecode_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let is_file_op = crate::engine::is_file_operation_path(&request.path);

        // For file operations, pass body through as-is (may be multipart or empty).
        // For normal operations, parse JSON to inject metadata.
        let (body, session_id) = if is_file_op {
            (request.body.clone(), String::new())
        } else {
            let mut body_json: Value = serde_json::from_slice(&request.body)
                .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
            let sid = request_session_id(request, &body_json);
            let user_id_value = build_metadata_user_id(credential, &sid);
            inject_metadata_user_id(&mut body_json, &user_id_value);
            let b = serde_json::to_vec(&body_json)
                .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
            (b, sid)
        };

        // -- 2. Build the User-Agent ------------------------------------
        let user_agent = match settings.user_agent() {
            Some(ua) => ua.to_string(),
            None => format!(
                "claude-cli/{} ({}, {})",
                DEFAULT_CLAUDECODE_VERSION,
                DEFAULT_CLAUDECODE_USER_TYPE,
                DEFAULT_CLAUDECODE_ENTRYPOINT
            ),
        };

        // -- 3. Fresh client request ID per request ---------------------
        let client_request_id = Uuid::now_v7().to_string();

        // -- 4. Assemble the HTTP request -------------------------------
        let url = format!("{}{}", settings.base_url(), request.path);
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("anthropic-version", "2023-06-01")
            .header("x-app", "cli")
            .header("User-Agent", &user_agent)
            .header("x-client-request-id", &client_request_id);

        if !is_file_op {
            builder = builder
                .header("X-Claude-Code-Session-Id", &session_id)
                .header("Content-Type", "application/json");
        }

        // Forward any additional headers from the prepared request
        // (includes Content-Type for file uploads, anthropic-beta for files, etc.)
        for (key, value) in request.headers.iter() {
            builder = builder.header(key, value);
        }

        builder
            .body(body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn finalize_request(
        &self,
        settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        // File operations: inject beta header, skip JSON body normalization.
        if crate::engine::is_file_operation_path(&request.path) {
            request.headers.insert(
                "anthropic-beta",
                http::HeaderValue::from_static("files-api-2025-04-14"),
            );
            return Ok(request);
        }

        let mut body_json: Value = serde_json::from_slice(&request.body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;

        normalize_claudecode_sampling(&mut body_json);
        normalize_claudecode_unsupported_fields(&mut body_json);

        if settings.enable_magic_cache {
            cache_control::apply_magic_string_cache_control_triggers(&mut body_json);
        }
        if !settings.cache_breakpoints.is_empty() {
            cache_control::ensure_cache_breakpoint_rules(
                &mut body_json,
                &settings.cache_breakpoints,
            );
        }

        let attribution = build_attribution(&first_user_message_text(&body_json));
        inject_system_attribution(&mut body_json, &attribution);
        let session_id = request_session_id(&request, &body_json);
        let header_value = http::HeaderValue::from_str(&session_id)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        request
            .headers
            .insert("x-claude-code-session-id", header_value);
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
                let retry_after_ms = parse_claudecode_rate_limit(headers);
                ResponseClassification::RateLimited { retry_after_ms }
            }
            529 => ResponseClassification::TransientError,
            500..=599 => ResponseClassification::TransientError,
            _ => ResponseClassification::PermanentError,
        }
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::UpstreamApi
    }

    fn needs_spoof_client(&self, credential: &Self::Credential) -> bool {
        credential.cookie.as_ref().is_some_and(|c| !c.is_empty())
    }

    fn prepare_quota_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        let url = format!(
            "{}/api/oauth/usage",
            settings.platform_base_url.trim_end_matches('/')
        );
        let user_agent = match settings.user_agent() {
            Some(ua) => ua.to_string(),
            None => format!(
                "claude-cli/{} ({}, {})",
                DEFAULT_CLAUDECODE_VERSION,
                DEFAULT_CLAUDECODE_USER_TYPE,
                DEFAULT_CLAUDECODE_ENTRYPOINT
            ),
        };
        let req = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("User-Agent", &user_agent)
            .header("anthropic-beta", "oauth-2025-04-20")
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
        let span = tracing::info_span!("refresh_credential", channel = "claudecode");
        async move {
            // Path 1: Standard OAuth refresh with refresh_token
            if !credential.refresh_token.is_empty() {
                match oauth2_refresh::refresh_oauth2_token(
                    &client,
                    "https://console.anthropic.com/v1/oauth/token",
                    "",
                    "",
                    &credential.refresh_token,
                )
                .await
                {
                    Ok(result) => {
                        credential.access_token = result.access_token;
                        credential.expires_at_ms = result.expires_at_ms;
                        if let Some(rt) = result.refresh_token {
                            credential.refresh_token = rt;
                        }
                        tracing::info!("credential refreshed via token");
                        return Ok(true);
                    }
                    Err(_) if credential.cookie.as_ref().is_some_and(|c| !c.is_empty()) => {
                        tracing::info!("token refresh failed, falling back to cookie");
                        // Fall through to cookie path
                    }
                    Err(e) => return Err(e),
                }
            }

            // Path 2: Cookie-to-token exchange (fallback)
            let cookie = match &credential.cookie {
                Some(c) if !c.is_empty() => c.clone(),
                _ => return Ok(false),
            };
            let tokens = crate::utils::claudecode_cookie::exchange_tokens_with_cookie(
                &client,
                &default_claudecode_base_url(),
                &default_claudecode_claude_ai_base_url(),
                &cookie,
            )
            .await?;
            if let Some(at) = tokens.access_token {
                credential.access_token = at;
            }
            if let Some(rt) = tokens.refresh_token {
                credential.refresh_token = rt;
            }
            if let Some(exp) = tokens.expires_in {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                credential.expires_at_ms = now_ms.saturating_add(exp.saturating_mul(1000));
            }
            if let Some(st) = tokens.subscription_type {
                credential.subscription_type = Some(st);
            }
            if let Some(rlt) = tokens.rate_limit_tier {
                credential.rate_limit_tier = Some(rlt);
            }
            tracing::info!("credential refreshed via cookie exchange");
            Ok(true)
        }
        .instrument(span)
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
            prune_claudecode_oauth_states(now_unix_ms);

            let redirect_uri = crate::utils::oauth::parse_query_value(params, "redirect_uri")
                .unwrap_or_else(|| CLAUDECODE_REDIRECT_URI.to_string());
            let scope = crate::utils::oauth::parse_query_value(params, "scope")
                .unwrap_or_else(|| CLAUDECODE_OAUTH_SCOPE.to_string());
            let api_base_url = if settings.base_url().trim().is_empty() {
                "https://api.anthropic.com".to_string()
            } else {
                settings.base_url().to_string()
            };
            let claude_ai_base_url =
                crate::utils::oauth::parse_query_value(params, "claude_ai_base_url")
                    .unwrap_or_else(|| CLAUDECODE_CLAUDE_AI_BASE_URL.to_string());
            let state = crate::utils::oauth::generate_state();
            let code_verifier = crate::utils::oauth::generate_code_verifier();
            let code_challenge = crate::utils::oauth::generate_code_challenge(&code_verifier);
            let authorize_url = build_claudecode_authorize_url(
                &claude_ai_base_url,
                &redirect_uri,
                &scope,
                &code_challenge,
                &state,
            );

            claudecode_oauth_states().insert(
                state.clone(),
                ClaudeCodeOAuthState {
                    code_verifier,
                    redirect_uri: redirect_uri.clone(),
                    api_base_url,
                    claude_ai_base_url,
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

            prune_claudecode_oauth_states(crate::utils::oauth::current_unix_ms());
            let (code, state_param) = crate::utils::oauth::resolve_code_and_state(params)
                .map_err(|e| UpstreamError::Channel(format!("claudecode oauth callback: {e}")))?;
            let state_id = state_param.ok_or_else(|| {
                UpstreamError::Channel("claudecode oauth callback: missing state".to_string())
            })?;
            let (_, oauth_state) = claudecode_oauth_states()
                .remove(state_id.as_str())
                .ok_or_else(|| {
                    UpstreamError::Channel("claudecode oauth callback: missing state".to_string())
                })?;

            let token = exchange_claudecode_code_for_tokens(
                client,
                &oauth_state.api_base_url,
                &oauth_state.claude_ai_base_url,
                &oauth_state.redirect_uri,
                &oauth_state.code_verifier,
                &code,
                &state_id,
            )
            .await?;
            let access_token = token
                .access_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel(
                        "claudecode oauth callback: missing access_token".to_string(),
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
                        "claudecode oauth callback: missing refresh_token".to_string(),
                    )
                })?
                .to_string();
            let profile =
                fetch_claudecode_oauth_profile(client, &oauth_state.api_base_url, &access_token)
                    .await
                    .ok();
            let subscription_type = token.subscription_type.or_else(|| {
                profile.as_ref().and_then(|profile| {
                    profile.organization.organization_type.clone().or_else(|| {
                        if profile.account.has_claude_max {
                            Some("claude_max".to_string())
                        } else if profile.account.has_claude_pro {
                            Some("claude_pro".to_string())
                        } else {
                            None
                        }
                    })
                })
            });
            let rate_limit_tier = token.rate_limit_tier.or_else(|| {
                profile
                    .as_ref()
                    .and_then(|profile| profile.organization.rate_limit_tier.clone())
            });
            let user_email = profile
                .as_ref()
                .and_then(|profile| profile.account.email.clone());
            let account_uuid = profile
                .as_ref()
                .and_then(|profile| profile.account.uuid.clone());
            let expires_at_ms = crate::utils::oauth::current_unix_ms()
                .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000));

            Ok(Some(OAuthCredentialResult {
                credential: ClaudeCodeCredential {
                    access_token: access_token.clone(),
                    refresh_token,
                    expires_at_ms,
                    device_id: default_claudecode_device_id(),
                    account_uuid: account_uuid.clone(),
                    subscription_type,
                    rate_limit_tier,
                    cookie: None,
                    user_email: user_email.clone(),
                },
                details: json!({
                    "access_token": access_token,
                    "account_uuid": account_uuid,
                    "user_email": user_email,
                    "expires_at_ms": expires_at_ms,
                }),
            }))
        })
    }
}

/// Parse Anthropic unified rate-limit headers into a single `retry_after_ms`.
///
/// Priority:
/// 1. `anthropic-ratelimit-unified-reset` — the server-chosen reset timestamp
///    for the representative (most constrained) window.
/// 2. `retry-after` — standard HTTP header (seconds).
///
/// Falls back to `None` if neither header is present / parseable.
fn parse_claudecode_rate_limit(headers: &http::HeaderMap) -> Option<u64> {
    // Prefer the unified reset header — it reflects the actual window that
    // triggered the rejection (5h, 7d, overage, etc.).
    if let Some(reset_ms) = parse_unix_reset_header(headers, "anthropic-ratelimit-unified-reset") {
        return Some(reset_ms);
    }
    // Fallback: standard retry-after (seconds).
    parse_retry_after_secs(headers)
}

/// Convert a unix-seconds reset header to a delay in milliseconds from now.
/// Returns `None` if the header is absent, unparseable, or already in the past.
fn parse_unix_reset_header(headers: &http::HeaderMap, name: &str) -> Option<u64> {
    let reset_secs = headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let reset_ms = reset_secs.saturating_mul(1000);
    if reset_ms > now_ms {
        Some(reset_ms - now_ms)
    } else {
        None
    }
}

/// Parse the standard `retry-after` header (integer seconds) into milliseconds.
fn parse_retry_after_secs(headers: &http::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 1000)
}

fn claudecode_dispatch_table() -> DispatchTable {
    ClaudeCodeChannel.dispatch_table()
}

inventory::submit! { ChannelRegistration::new(ClaudeCodeChannel::ID, claudecode_dispatch_table) }
