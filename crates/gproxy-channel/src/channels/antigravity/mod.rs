//! Antigravity: a Google account used against the Code Assist host the
//! Antigravity editor's agent talks to, rather than a Gemini API key.
//!
//! The channel presents itself as the Antigravity IDE (2.6.0, the official
//! Linux x64 build), whose traffic leaves from two places:
//!
//! - The Electron main process (`resources/app/out/main.js`) logs in with
//!   google-auth-library 10.3.0: the IDE's own installed-app client id and
//!   secret, the `localhost:51121/oauth-callback` redirect, its five scopes
//!   (`cclog` and `experimentsandconfigs` beyond the Gemini CLI's),
//!   `access_type=offline`, `prompt=consent` and no PKCE. It also runs the
//!   `loadCodeAssist`/`onboardUser` project discovery with
//!   `{ideType: ANTIGRAVITY, ideVersion, ideName: "antigravity"}` and the
//!   editor user agent (`oauth`).
//! - Since 2.6.0 the IDE ships no language server: its bundled `agy`
//!   extension downloads the Antigravity CLI and runs it as
//!   `antigravity --hub` (CLI 1.4.10, hub server 2.28.1), and that Go
//!   process makes the model calls: `POST /v1internal:generateContent`,
//!   `:streamGenerateContent?alt=sse` and `:countTokens` carrying the Code
//!   Assist envelope, the catalogue from `POST /v1internal:fetchAvailableModels`
//!   with the account's project, and the quota windows from
//!   `:retrieveUserQuotaSummary` (see `quota`), all with the hub's user
//!   agent.
//!
//! Every call goes to `cloudcode-pa.googleapis.com`. The official client
//! picks the host per account (`CLIAuthProvider.UpdateEndpointURL`: the
//! production host for a GCP-terms tier or sign-in, `daily-cloudcode-pa`
//! for a consumer Google sign-in, which a 2026-10-29 capture of the hub
//! confirmed after its first `loadCodeAssist`), but `gemini-4-argon-*` was
//! reported answering 404 on `daily-` (2026-10-29) while the IDE kept
//! serving it, so the channel defaults to production; a provider `base_url`
//! can still pin `daily-`.
//!
//! The envelope shape follows the v3 channel and the reverse engineered
//! client in
//! `samples/CLIProxyAPI/internal/runtime/executor/antigravity_executor_request.go`.
//!
//! The credential secret is an `OAuthCredential`. The project and the tier
//! are facts a login discovers: they travel in `provider_fields`, the host
//! persists them on the credential row, and `prepare` reads them back from
//! the metadata. Preparation never discovers anything.
//!
//! Session identity: an Antigravity client names its session inside the
//! envelope as `request.sessionId` (camel case, unlike the Gemini CLI's
//! `request.session_id`; `antigravity_executor_request.go` reads and writes
//! exactly that path). The envelope this channel builds nests the client's
//! body under `request` unchanged, so a `sessionId` the client sent arrives
//! at `request.sessionId`, and a client that already sent a whole envelope
//! keeps its own. The field is never renamed, moved or dropped; a request
//! without one gets the id the hub sends (below).
//!
//! Envelope fields: the hub does not send the Gemini CLI's
//! `user_prompt_id`. It names itself (`userAgent: "antigravity"`) and the
//! kind of call (`requestType: "agent"`, `"image_gen"` for an image model).
//! An agent call (captured from the hub and CLI 1.4.10, 2026-10-29) carries
//! `requestId: agent/<cascade>/<ms>/<trajectory>/<step>`, where the cascade
//! and trajectory ids stay put across a conversation and the step is the
//! number of `contents`; `request.labels` repeating those ids
//! (`last_step_index` one less than the step, `request_id`
//! `<trajectory>-<model turns so far>`) and saying whether the model is
//! Claude or not Gemini; and `request.sessionId` `-3750763034362895579`,
//! the signed FNV-1a 64 of the empty session string the hub hashes
//! (`codeassistclient.BuildGenerateContentRequest`). Without the hub's
//! conversation state, the cascade and trajectory ids are derived from the
//! first user text, so every turn of a conversation shares them, and the
//! `model_enum` label (the catalogue's internal model number) comes from a
//! captured table (`MODEL_ENUMS`).
//! Any other kind keeps `<kind>/<uuid>`, and an image call
//! `image_gen/<ms>/<uuid>/12`. It sends no `safetySettings` and keeps
//! `toolConfig` inside `request`
//! (`antigravity_executor_request.go::geminiToAntigravity`).

mod claude;
mod models;
mod oauth;
mod quota;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, CredentialRefresh, CredentialView, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode,
    OAuthAuthorizationCode, OperationContext, OperationFuture, PrepareContext, ProviderView,
    QuotaModel, QuotaQuery, forwardable,
};
use crate::channels::shared::code_assist;
use gproxy_client::{Alpn, Backend, ConnectionConfig, EmulationConfig, Fingerprint, TlsVersion};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse, connection::Bytes,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, header};
use serde::Deserialize;
use serde_json::Value;

pub const ID: &str = "antigravity";
/// The production Code Assist host, the one the Gemini CLI also uses (see
/// the module docs for the `daily-` deployment the IDE picks for consumer
/// accounts).
pub const DEFAULT_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
pub const DEFAULT_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// Antigravity's own installed-app client (v3 `login.rs`, `auth.rs`).
pub const DEFAULT_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
/// Shipped inside the tool, so an identifier rather than a secret.
pub const DEFAULT_CLIENT_SECRET: &str = "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";
/// The loopback callback the editor listens on (v3 `login.rs`).
pub const DEFAULT_REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";
/// The IDE's scope list, exactly (`main.js` `Ats=[…]`): the Gemini CLI's
/// three plus `cclog` and `experimentsandconfigs`.
pub const OAUTH_SCOPE: &str = concat!(
    "https://www.googleapis.com/auth/cloud-platform ",
    "https://www.googleapis.com/auth/userinfo.email ",
    "https://www.googleapis.com/auth/userinfo.profile ",
    "https://www.googleapis.com/auth/cclog ",
    "https://www.googleapis.com/auth/experimentsandconfigs"
);

/// The Antigravity IDE release this channel impersonates: `product.json`
/// `"ideVersion": "2.6.0"` of the official Linux x64 build
/// (`antigravity/stable/2.6.0-5526304964083712`).
macro_rules! ide_version {
    () => {
        "2.6.0"
    };
}
pub const IDE_VERSION: &str = ide_version!();
/// The google-auth-library the IDE bundles
/// (`resources/app/node_modules/google-auth-library` 10.3.0), as the
/// Electron side's requests name it.
pub const AUTH_LIBRARY: &str = "google-api-nodejs-client/10.3.0";
/// The Node inside the IDE's Electron 39.2.3 (the binary carries `v22.21.1`),
/// as google-auth-library reports it in `x-goog-api-client`.
pub const NODE_CLIENT: &str = "gl-node/22.21.1";
/// The user agent of the IDE's Electron main process
/// (`cloudCodeMainService.userAgent`: `` `${"antigravity"}/${ideVersion}
/// ${platform}/${arch}` ``, `x64` written `amd64`), to which
/// google-auth-library's request interceptor appends its banner. It carries
/// the login's `loadCodeAssist`/`onboardUser` and the profile call.
pub const EDITOR_USER_AGENT: &str = concat!(
    "antigravity/",
    ide_version!(),
    " linux/amd64 google-api-nodejs-client/10.3.0"
);
/// The user agent of the hub the IDE runs (`antigravity --hub`, CLI
/// 1.4.10), which makes the catalogue, quota and generation calls
/// (`codeassistclient.UserAgent`): `%s/%s/%s (%s)` of the agent name
/// (`antigravity`), the subclient type (`hub` under `--hub`, `cli`
/// otherwise), the hub server's version (2.28.1; the CLI's own version under
/// `cli`), and `aidev_client`, `os_type=<GOOS>`, `arch=<GOARCH>` and the
/// build's `cl=<changelist>` joined by `; `. The hub's sign-in records no
/// `auth_method` (the standalone CLI's adds `auth_method=consumer`). Captured
/// from the Linux amd64 build on 2026-10-29.
pub const USER_AGENT: &str =
    "antigravity/hub/2.28.1 (aidev_client; os_type=linux; arch=amd64; cl=1009308193)";

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct AntigravityConfig {
    pub authorize_url: String,
    pub token_url: String,
    /// Static headers added to every Code Assist request.
    pub headers: std::collections::BTreeMap<String, String>,
}

impl Default for AntigravityConfig {
    fn default() -> Self {
        Self {
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            headers: std::collections::BTreeMap::new(),
        }
    }
}

impl AntigravityConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Antigravity;

/// What the Antigravity client itself sends: only its user agent, which is
/// how the Code Assist host tells it apart from the Gemini CLI.
/// The channel sets it from the credential and the provider configuration,
/// so a provider allow-list can never hide the identity being impersonated;
/// a client's own copy is dropped by `CHANNEL_HEADERS`.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["user-agent"],
    prefixes: &[],
};

/// Headers the channel owns; a client cannot supply them. Google account
/// cookies are browser identity and never belong on a bearer-token call.
const CHANNEL_HEADERS: &[&str] = &[
    "user-agent",
    "accept",
    "accept-encoding",
    "cookie",
    "x-goog-user-project",
];

/// The Go Code Assist transport of the Antigravity CLI the IDE runs as its
/// hub (captured from CLI 1.2.16, unchanged in CLI 1.4.10): no ALPN or
/// GREASE, TLS 1.2 floor, Go's cipher preferences and its extension order. BoringSSL puts TLS
/// 1.3 suites first even though Go advertises them last.
/// BoringSSL lacks the captured SecP256r1MLKEM768/SecP384r1MLKEM1024 groups
/// and signature_algorithms_cert extension; keep the
/// supported subset rather than configuring algorithms it cannot negotiate.
pub fn default_connection() -> ConnectionConfig {
    ConnectionConfig {
        backend: Backend::Wreq,
        // Go's HTTP transport negotiates and decodes gzip. The downstream
        // client's encoding preferences cannot describe this channel's parser.
        gzip: true,
        emulation: Some(EmulationConfig::Custom(Fingerprint {
            alpn: Vec::<Alpn>::new(),
            disable_alpn: Some(true),
            min_tls: Some(TlsVersion::Tls12),
            max_tls: Some(TlsVersion::Tls13),
            cipher_list: Some(
                concat!(
                    "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:",
                    "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:",
                    "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:",
                    "ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES128-SHA:",
                    "ECDHE-ECDSA-AES256-SHA:ECDHE-RSA-AES256-SHA:",
                    "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256"
                )
                .into(),
            ),
            curves_list: Some("X25519MLKEM768:X25519:P-256:P-384:P-521".into()),
            sigalgs_list: Some(
                concat!(
                    "mldsa44:mldsa65:mldsa87:",
                    "rsa_pss_rsae_sha256:ecdsa_secp256r1_sha256:ed25519:",
                    "rsa_pss_rsae_sha384:rsa_pss_rsae_sha512:rsa_pkcs1_sha256:",
                    "rsa_pkcs1_sha384:rsa_pkcs1_sha512:ecdsa_secp384r1_sha384:",
                    "ecdsa_secp521r1_sha512"
                )
                .into(),
            ),
            preserve_tls13_cipher_list: Some(true),
            grease: Some(false),
            extension_permutation: Some(vec![0, 11, 65281, 23, 18, 5, 10, 13, 43, 51]),
            ocsp_stapling: Some(true),
            signed_cert_timestamps: Some(true),
            session_ticket: Some(false),
            psk_dhe_ke: Some(false),
            http2: None,
            headers: None,
        })),
        ..ConnectionConfig::default()
    }
}

fn base_url(provider: ProviderView<'_>) -> String {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}

/// A public account fact: host metadata first, then the secret's
/// `provider_fields`, then the flat layout v3 wrote.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    fn non_empty(value: Option<&Value>) -> Option<&str> {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
    non_empty(credential.metadata.get(name))
        .or_else(|| {
            non_empty(
                credential
                    .secret
                    .pointer(&format!("/provider_fields/{name}")),
            )
        })
        .or_else(|| non_empty(credential.secret.get(name)))
}

pub(super) fn access_token<'a>(credential: &CredentialView<'a>) -> Result<&'a str, ChannelError> {
    credential
        .secret
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}

/// The Cloud project discovered for this credential at login.
pub(super) fn project(credential: &CredentialView<'_>) -> Result<String, ChannelError> {
    fact(credential, "project_id")
        .map(str::to_owned)
        .ok_or(ChannelError::InvalidCredential)
}

fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}

/// Headers every hub Code Assist call carries (catalogue, quota,
/// generation): the bearer token, the JSON content type and the hub's user
/// agent (`CodeAssistClient.setHeaders`), and no `Accept` or
/// `x-goog-api-client`. Go's transport adds `Accept-Encoding: gzip` itself,
/// which the connection's `gzip` reproduces.
pub(super) fn apply_headers(
    headers: &mut HeaderMap,
    config: &AntigravityConfig,
    access_token: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {access_token}"))?,
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                ChannelError::InvalidConfig(format!("header `{name}` is not a header name"))
            })?,
            HeaderValue::from_str(value).map_err(|_| {
                ChannelError::InvalidConfig(format!("header `{name}` has an invalid value"))
            })?,
        );
    }
    Ok(())
}

/// The Code Assist method for an operation (v3 `prepare.rs`). The client's
/// own `/v1beta/models/...` path is not trusted: Code Assist mounts every
/// operation on `/v1internal`.
fn operation_path(operation: Operation) -> Result<&'static str, ChannelError> {
    Ok(match operation {
        Operation::ListModels | Operation::GetModel => "/v1internal:fetchAvailableModels",
        Operation::CountTokens => "/v1internal:countTokens",
        Operation::GenerateContent => "/v1internal:generateContent",
        Operation::StreamGenerateContent => "/v1internal:streamGenerateContent",
        other => {
            return Err(ChannelError::UnsupportedOperation(OperationKey {
                operation: other,
                dialect: Dialect::Gemini,
            }));
        }
    })
}

/// The hub's `request.sessionId`: the FNV-1a 64 offset basis as a signed
/// integer, i.e. the hash of the empty session string it always has.
const SESSION_ID: &str = "-3750763034362895579";

fn uuid() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| ChannelError::InvalidConfig("request id randomness failed".into()))?;
    Ok(format_uuid(bytes))
}

fn format_uuid(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// The cascade and trajectory ids of a conversation: UUIDs from the SHA-256
/// of the first user text, so every turn of a conversation shares them;
/// random when the conversation opens without text.
fn conversation_ids(request: &Value) -> Result<(String, String), ChannelError> {
    use sha2::Digest as _;
    let first = request
        .get("contents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|content| content.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|content| content.pointer("/parts/0/text"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty());
    let Some(text) = first else {
        return Ok((uuid()?, uuid()?));
    };
    let seed = sha2::Sha256::digest(text.as_bytes());
    let derive = |label: &[u8]| {
        let digest = sha2::Sha256::new()
            .chain_update(label)
            .chain_update(seed)
            .finalize();
        format_uuid(digest[..16].try_into().expect("sixteen bytes"))
    };
    Ok((derive(b"cascade"), derive(b"trajectory")))
}

/// The catalogue's internal model numbers (`fetchAvailableModels`
/// `models.<id>.model`) the hub repeats as the `model_enum` label, from a
/// Google AI Pro account's catalogue on 2026-10-30 (identical on the
/// production and `daily-` hosts). A model missing here gets no label.
const MODEL_ENUMS: &[(&str, &str)] = &[
    ("claude-opus-5-5-high", "MODEL_PLACEHOLDER_M402"),
    ("claude-opus-5-5-low", "MODEL_PLACEHOLDER_M400"),
    ("claude-opus-5-5-medium", "MODEL_PLACEHOLDER_M401"),
    ("claude-sonnet-5-5-high", "MODEL_PLACEHOLDER_M405"),
    ("claude-sonnet-5-5-low", "MODEL_PLACEHOLDER_M403"),
    ("claude-sonnet-5-5-medium", "MODEL_PLACEHOLDER_M404"),
    ("gemini-2.5-flash", "MODEL_GOOGLE_GEMINI_2_5_FLASH"),
    ("gemini-2.5-flash-lite", "MODEL_GOOGLE_GEMINI_2_5_FLASH_LITE"),
    ("gemini-2.5-flash-thinking", "MODEL_GOOGLE_GEMINI_2_5_FLASH_THINKING"),
    ("gemini-2.5-pro", "MODEL_GOOGLE_GEMINI_2_5_PRO"),
    ("gemini-3-flash", "MODEL_PLACEHOLDER_M18"),
    ("gemini-3.1-flash-image", "MODEL_PLACEHOLDER_M21"),
    ("gemini-3.1-flash-lite", "MODEL_PLACEHOLDER_M50"),
    ("gemini-3.5-flash-lite", "MODEL_PLACEHOLDER_M198"),
    ("gemini-3.8-flash-high", "MODEL_PLACEHOLDER_M318"),
    ("gemini-3.8-flash-low", "MODEL_PLACEHOLDER_M320"),
    ("gemini-3.8-flash-medium", "MODEL_PLACEHOLDER_M319"),
    ("gemini-3.8-flash-tiered", "MODEL_PLACEHOLDER_M322"),
    ("gemini-4-argon-high", "MODEL_PLACEHOLDER_M417"),
    ("gemini-4-argon-low", "MODEL_PLACEHOLDER_M419"),
    ("gemini-4-argon-medium", "MODEL_PLACEHOLDER_M418"),
    ("gpt-oss-120b-medium", "MODEL_OPENAI_GPT_OSS_120B_MEDIUM"),
];

/// The `requestId` and `request.labels` of an agent call, from the
/// conversation so far.
fn agent_labels(
    request: &Value,
    model: &str,
    ms: i64,
) -> Result<(String, serde_json::Map<String, Value>), ChannelError> {
    let (cascade, trajectory) = conversation_ids(request)?;
    let contents = request
        .get("contents")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    // A model turn is a reply the model generated; a function response some
    // clients file under the model role is the tool's, not the model's.
    let model_turns = contents
        .iter()
        .filter(|content| content.get("role").and_then(Value::as_str) == Some("model"))
        .filter(|content| {
            content
                .get("parts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|part| part.get("functionResponse").is_none())
        })
        .count();
    let step = contents.len();
    // The hub keeps these for the whole conversation once a Claude or
    // non-Gemini model has answered; without its state they follow the
    // model of this call.
    let claude = claude::is_claude(model);
    let gemini = code_assist::model_id(model).starts_with("gemini");
    let mut labels = serde_json::Map::new();
    if let Some((_, number)) = MODEL_ENUMS
        .iter()
        .find(|(id, _)| *id == code_assist::model_id(model))
    {
        labels.insert("model_enum".into(), Value::from(*number));
    }
    for (name, value) in [
        ("cascade_id", cascade.clone()),
        ("last_step_index", step.saturating_sub(1).to_string()),
        ("request_id", format!("{trajectory}-{model_turns}")),
        ("root_cascade_id", cascade.clone()),
        ("trajectory_id", trajectory.clone()),
        ("used_claude", claude.to_string()),
        ("used_claude_conservative", claude.to_string()),
        ("used_non_gemini_model", (!gemini).to_string()),
    ] {
        labels.insert(name.into(), Value::from(value));
    }
    Ok((format!("agent/{cascade}/{ms}/{trajectory}/{step}"), labels))
}

/// The envelope fields the hub sends in place of the Gemini CLI's; a value
/// the client already put in the envelope is kept.
fn agent_envelope(envelope: &mut Value, model: &str) -> Result<(), ChannelError> {
    let Some(object) = envelope.as_object_mut() else {
        return Ok(());
    };
    object.remove("user_prompt_id");
    let image = code_assist::model_id(model).contains("image");
    object.insert("userAgent".into(), Value::from("antigravity"));
    let kind = object
        .get("requestType")
        .and_then(Value::as_str)
        .filter(|kind| !kind.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| if image { "image_gen" } else { "agent" }.to_owned());
    let ms = code_assist::unix_now_ms();
    let (request_id, labels) = match kind.as_str() {
        "image_gen" => (format!("image_gen/{ms}/{}/12", uuid()?), None),
        "agent" => {
            let request = object.get("request").unwrap_or(&Value::Null);
            let (id, labels) = agent_labels(request, model, ms)?;
            (id, Some(labels))
        }
        other => (format!("{other}/{}", uuid()?), None),
    };
    object.insert("requestType".into(), Value::from(kind));
    object
        .entry("requestId")
        .or_insert_with(|| Value::from(request_id));
    if let Some(tool_config) = object.remove("toolConfig")
        && let Some(request) = object.get_mut("request").and_then(Value::as_object_mut)
    {
        request.entry("toolConfig").or_insert(tool_config);
    }
    let Some(request) = object.get_mut("request").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    request.remove("safetySettings");
    if let Some(labels) = labels {
        request.entry("labels").or_insert(Value::Object(labels));
    }
    let named = request
        .get("sessionId")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty());
    if !named {
        request.insert("sessionId".into(), Value::from(SESSION_ID));
    }
    Ok(())
}

impl Antigravity {
    fn build(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = AntigravityConfig::from_view(ctx.provider)?;
        let token = access_token(&ctx.credential)?;
        let operation = ctx.operation.operation;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let WireRequest {
            path,
            headers: source,
            body,
            ..
        } = ctx.request;

        let buffered = match body {
            HttpBody::Bytes(bytes) => bytes,
            HttpBody::Stream(_) => {
                return Err(ChannelError::InvalidConfig(
                    "the Code Assist envelope needs a buffered request body".into(),
                ));
            }
        };
        let parsed = serde_json::from_slice::<Value>(&buffered).ok();
        let model = code_assist::request_model(&path, parsed.as_ref()).unwrap_or_default();

        let (body, stream) = match operation {
            Operation::ListModels | Operation::GetModel => (catalog_body(&ctx.credential), false),
            Operation::CountTokens => (code_assist::count_envelope(&buffered, &model)?, false),
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                let project = project(&ctx.credential)?;
                let bytes = code_assist::envelope(&buffered, &model, &project)?;
                let mut envelope: Value = serde_json::from_slice(&bytes)
                    .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
                if claude::is_claude(&model)
                    && let Some(request) = envelope.get_mut("request")
                {
                    claude::declare_tools(request);
                    claude::tidy_history(request);
                    claude::apply_limits(request, parsed.as_ref().and_then(claude::output_limit));
                }
                agent_envelope(&mut envelope, &model)?;
                (
                    code_assist::encode(&envelope, ChannelError::InvalidConfig)?,
                    operation == Operation::StreamGenerateContent,
                )
            }
            other => {
                return Err(ChannelError::UnsupportedOperation(OperationKey {
                    operation: other,
                    dialect: ctx.operation.dialect,
                }));
            }
        };

        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!("{}{}", base_url(ctx.provider), operation_path(operation)?),
        };
        // Code Assist takes no client query beyond the streaming marker; the
        // client's own `?key=` and paging parameters are dropped.
        let uri = match (stream, url.contains('?')) {
            (true, false) => format!("{url}?alt=sse"),
            (true, true) => format!("{url}&alt=sse"),
            (false, _) => url,
        };

        let mut headers = forwardable(&source, allowlist.as_ref(), CHANNEL_HEADERS);
        apply_headers(&mut headers, &config, token)?;
        let mut builder = http::Request::builder().method(Method::POST).uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(HttpBody::Bytes(body))
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// Buffered generation: one JSON object with the Gemini reply under
    /// `response`, which is unwrapped before the host sees it.
    async fn generate(
        &self,
        operation: Operation,
        ctx: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let OperationContext {
            provider,
            credential,
            dialect,
            request,
            client,
            endpoint_override,
            ..
        } = ctx;
        let prepared = self.build(PrepareContext {
            provider,
            credential,
            operation: OperationKey { operation, dialect },
            request,
            endpoint_override,
        })?;
        let mut response = client.send(prepared).await?;
        if !response.status.is_success() {
            return Ok(response);
        }
        if operation == Operation::StreamGenerateContent {
            response.headers.remove(header::CONTENT_LENGTH);
            response.body = code_assist::stream::unwrap_sse(response.body);
            return Ok(response);
        }
        let bytes = code_assist::read_body(response.body).await?;
        let unwrapped = code_assist::unwrap_body(&bytes)?;
        response.headers.remove(header::CONTENT_LENGTH);
        response.body = HttpBody::Bytes(unwrapped);
        Ok(response)
    }
}

impl BaseChannel for Antigravity {
    fn id(&self) -> &'static str {
        ID
    }

    /// A Google account through the Antigravity IDE: an authorization code
    /// login that also discovers the Cloud project, a refreshable token, and the
    /// 5-hour and weekly quota windows each model family shares.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Antigravity (Code Assist)",
            credential_kinds: vec!["oauth"],
            login_modes: vec![LoginMode::AuthorizationCode],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                paid_usage: true,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Code Assist origin; defaults to https://cloudcode-pa.googleapis.com. Provider column, not config JSON.",
                ).with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "authorize_url",
                    ConfigKeyKind::String,
                    "Google OAuth authorization endpoint for the browser step.",
                ).with_placeholder(DEFAULT_AUTHORIZE_URL),
                ConfigKey::optional(
                    "token_url",
                    ConfigKeyKind::String,
                    "Google OAuth token endpoint used by the code exchange and by refresh.",
                ).with_placeholder(DEFAULT_TOKEN_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every Code Assist request.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn default_connection(&self) -> Option<ConnectionConfig> {
        Some(default_connection())
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CountTokens
            | Operation::ListModels
            | Operation::GetModel => vec![Dialect::Gemini],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        self.build(ctx)
    }

    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::invoke(self, Operation::ListModels, context))
    }

    fn get_model<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::invoke(self, Operation::GetModel, context))
    }

    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.generate(Operation::GenerateContent, context))
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.generate(Operation::StreamGenerateContent, context))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
}

/// `fetchAvailableModels` names the account's project, as the IDE's
/// `getAvailableModels` (`{project: this._cloudAiCompanionProject}`) and the
/// Go client's captured request do; a credential without one sends `{}`,
/// which the endpoint also answers.
fn catalog_body(credential: &CredentialView<'_>) -> Bytes {
    match fact(credential, "project_id") {
        Some(project) => Bytes::from(
            serde_json::json!({ "project": project })
                .to_string()
                .into_bytes(),
        ),
        None => Bytes::from_static(b"{}"),
    }
}

/// The catalogue call the quota query also makes.
pub(super) fn catalog_request(
    config: &AntigravityConfig,
    provider: ProviderView<'_>,
    credential: &CredentialView<'_>,
) -> Result<(String, HeaderMap, Bytes), ChannelError> {
    let token = access_token(credential)?;
    let mut headers = HeaderMap::new();
    apply_headers(&mut headers, config, token)?;
    Ok((
        format!(
            "{}{}",
            base_url(provider),
            "/v1internal:fetchAvailableModels"
        ),
        headers,
        catalog_body(credential),
    ))
}
