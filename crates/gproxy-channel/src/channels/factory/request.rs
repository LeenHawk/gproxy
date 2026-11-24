//! The inference request the CLI makes: one origin, the wire chosen by path,
//! the upstream named by `x-api-provider`, and the CLI's identity headers.
//!
//! Every header here is from the loopback capture of droid 1.18.0
//! (`samples/droid-1.18.0/captures`). The body is the client's own; only the
//! Gemini wire needs the model added, because its path names none.

use super::config::FactoryConfig;
use super::models::{self, Wire};
use crate::channel::{ChannelHeaders, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use gproxy_upstream::{CredentialView, UpstreamError};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

/// Client headers the CLI's own requests carry and a client may set: the
/// Anthropic betas it asks for and the session it keys a conversation on.
pub const CLIENT_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["anthropic-beta", "x-session-id"],
    prefixes: &[],
};

/// The identity this channel writes; a client cannot supply its own.
const IDENTITY: &[&str] = &[
    "user-agent",
    "x-factory-client",
    "x-client-version",
    "x-factory-request-surface",
    "x-factory-exec-output-format",
    "x-factory-org-id",
    "x-api-provider",
    "x-provider-routing-source",
    "x-assistant-message-id",
    "openai-platform",
    "anthropic-version",
];

/// The OpenAI organization the CLI names on every OpenAI-provider call.
const OPENAI_PLATFORM: &str = "org-bHuLtG1fGmYk5YaOihAAXFBw";
/// The Anthropic version the CLI pins, not the newest one gproxy speaks.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// A public fact the login recorded: host metadata first, then the secret's
/// `provider_fields`, then a flat field on the secret.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    fn text(value: Option<&Value>) -> Option<&str> {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
    text(credential.metadata.get(name))
        .or_else(|| text(credential.secret.pointer(&format!("/provider_fields/{name}"))))
        .or_else(|| text(credential.secret.get(name)))
}

/// The bearer: a pasted Factory API key or the WorkOS access token.
pub(super) fn token<'a>(credential: &CredentialView<'a>) -> Result<&'a str, UpstreamError> {
    ["api_key", "access_token"]
        .into_iter()
        .find_map(|name| credential.secret.get(name).and_then(Value::as_str))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(UpstreamError::InvalidCredential)
}

/// Just `Authorization`. The CLI carries the organization in the token's own
/// `org_id` claim and sends no `X-Factory-Org-Id` on inference or billing
/// (verified live: adding it with the login's organization is answered 403
/// "Requested active organization is not accessible by this user"). The
/// organization is kept in metadata for display only.
pub(super) fn auth_headers(
    credential: &CredentialView<'_>,
    headers: &mut HeaderMap,
) -> Result<(), UpstreamError> {
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token(credential)?))
            .map_err(|_| UpstreamError::InvalidCredential)?,
    );
    Ok(())
}

/// The client identity every control-plane and inference call carries.
pub(super) fn identity_headers(
    config: &FactoryConfig,
    headers: &mut HeaderMap,
) -> Result<(), UpstreamError> {
    let version = config.client_version();
    let value = |text: String| {
        HeaderValue::from_str(&text)
            .map_err(|_| UpstreamError::InvalidConfig("client_version is not a header value".into()))
    };
    headers.insert(header::USER_AGENT, value(format!("factory-cli/{version}"))?);
    headers.insert(HeaderName::from_static("x-client-version"), value(version.to_owned())?);
    headers.insert(
        HeaderName::from_static("x-factory-client"),
        HeaderValue::from_static("cli"),
    );
    headers.insert(
        HeaderName::from_static("x-factory-request-surface"),
        HeaderValue::from_static("cli_exec"),
    );
    headers.insert(
        HeaderName::from_static("x-factory-exec-output-format"),
        HeaderValue::from_static("json"),
    );
    Ok(())
}

fn path(operation: OperationKey) -> Result<&'static str, UpstreamError> {
    match (operation.operation, operation.dialect) {
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::Claude) => {
            Ok("/api/llm/a/v1/messages")
        }
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAi) => {
            Ok("/api/llm/o/v1/responses")
        }
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAiChat) => {
            Ok("/api/llm/o/v1/chat/completions")
        }
        (Operation::StreamGenerateContent, Dialect::Gemini) => Ok("/api/llm/g/v1/generate"),
        _ => Err(UpstreamError::UnsupportedOperation(operation)),
    }
}

/// A random RFC 4122 version 4 UUID, as the CLI mints its message ids.
fn uuid() -> Result<String, UpstreamError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        UpstreamError::InvalidConfig(format!("operating-system randomness is unavailable: {error}"))
    })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

/// The Gemini wire names the model in the path; strip the resource prefix
/// and the method.
fn gemini_model(path: &str) -> Option<&str> {
    let tail = path.split_once("/models/").map(|(_, tail)| tail)?;
    let model = tail.split(':').next().unwrap_or(tail);
    (!model.is_empty()).then_some(model)
}

/// The upstream model this request names, from the path (Gemini) or the body.
fn model_of(dialect: Dialect, path: &str, body: &[u8]) -> Option<String> {
    if dialect == Dialect::Gemini {
        return gemini_model(path).map(str::to_owned);
    }
    serde_json::from_slice::<Value>(body)
        .ok()?
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The identity line Factory's usage-policy check requires at the very start
/// of the system prompt (`tools.rs` documents the check). The client's own
/// system text rides along after it.
pub(super) const DROID_IDENTITY: &str =
    "You are Droid, an AI software engineering agent built by Factory.";

const fn is_generation(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    )
}

fn starts_with_identity(text: &str) -> bool {
    text.trim_start().starts_with(DROID_IDENTITY)
}

/// Put the Droid identity line first in the system prompt of `dialect`.
fn inject_identity(body: &mut Value, dialect: Dialect) {
    let identity = || Value::String(DROID_IDENTITY.to_owned());
    let prefixed = |text: &str| Value::String(format!("{DROID_IDENTITY}\n\n{text}"));
    match dialect {
        Dialect::Claude => match body.get_mut("system") {
            None => {
                insert(body, "system", identity());
            }
            Some(Value::String(text)) if !starts_with_identity(text) => {
                *body.get_mut("system").unwrap() = prefixed(text);
            }
            Some(Value::Array(blocks)) => {
                let first_is_identity = blocks
                    .first()
                    .and_then(|block| block.get("text"))
                    .and_then(Value::as_str)
                    .is_some_and(starts_with_identity);
                if !first_is_identity {
                    blocks.insert(0, serde_json::json!({"type": "text", "text": DROID_IDENTITY}));
                }
            }
            _ => {}
        },
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => match body.get_mut("instructions") {
            None => insert(body, "instructions", identity()),
            Some(Value::String(text)) if !starts_with_identity(text) => {
                *body.get_mut("instructions").unwrap() = prefixed(text);
            }
            _ => {}
        },
        Dialect::OpenAiChat => {
            let first_is_identity = body
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|messages| messages.first())
                .filter(|m| m.get("role").and_then(Value::as_str) == Some("system"))
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .is_some_and(starts_with_identity);
            if !first_is_identity
                && let Some(messages) = body
                    .get_mut("messages")
                    .and_then(Value::as_array_mut)
            {
                messages.insert(
                    0,
                    serde_json::json!({"role": "system", "content": DROID_IDENTITY}),
                );
            }
        }
        Dialect::Gemini => {
            let Some(object) = body.as_object_mut() else {
                return;
            };
            let instruction = object
                .entry("systemInstruction")
                .or_insert_with(|| serde_json::json!({"parts": []}));
            let Some(parts) = instruction.as_object_mut().and_then(|instruction| {
                instruction
                    .entry("parts")
                    .or_insert_with(|| Value::Array(vec![]))
                    .as_array_mut()
            }) else {
                return;
            };
            let first_is_identity = parts
                .first()
                .and_then(|part| part.get("text"))
                .and_then(Value::as_str)
                .is_some_and(starts_with_identity);
            if !first_is_identity {
                parts.insert(0, serde_json::json!({"text": DROID_IDENTITY}));
            }
        }
        _ => {}
    }
}

fn insert(body: &mut Value, key: &str, value: Value) {
    if let Some(object) = body.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
}

/// Inject the Droid identity, launder every tool name, and (on the Gemini
/// path, which names the model in its URL, not its body) add `model`.
fn transform_body(bytes: Bytes, dialect: Dialect, model: Option<&str>) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) else {
        return bytes;
    };
    if !value.is_object() {
        return bytes;
    }
    inject_identity(&mut value, dialect);
    super::tools::map_request(&mut value, dialect, super::tools::encode_name);
    if dialect == Dialect::Gemini
        && let Some(model) = model
        && let Some(object) = value.as_object_mut()
    {
        object.insert("model".into(), Value::String(model.to_owned()));
    }
    serde_json::to_vec(&value).map_or(bytes, Bytes::from)
}

pub(super) fn build(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, UpstreamError> {
    let config = FactoryConfig::from_view(ctx.provider)?;
    let dialect = ctx.operation.dialect;
    let bytes = match ctx.request.body {
        HttpBody::Bytes(bytes) => bytes,
        HttpBody::Stream(_) => {
            return Err(UpstreamError::InvalidConfig(
                "factory: the request body must be buffered before preparation".into(),
            ));
        }
    };
    let model = model_of(dialect, &ctx.request.path, &bytes);
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => format!("{}{}", FactoryConfig::base_url(ctx.provider), path(ctx.operation)?),
    };
    // The Gemini path takes no query: the CLI sends none, and `alt` would be
    // meaningless on a stream that is always SSE.
    let uri = match ctx
        .request
        .query
        .as_deref()
        .filter(|_| dialect != Dialect::Gemini)
        .map(strip_query_auth)
        .filter(|query| !query.is_empty())
    {
        Some(query) => format!("{url}?{query}"),
        None => url,
    };
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLIENT_HEADERS)?;
    let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), IDENTITY);
    auth_headers(&ctx.credential, &mut headers)?;
    identity_headers(&config, &mut headers)?;
    let provider = model
        .as_deref()
        .and_then(|model| config.api_providers.get(model).map(String::as_str))
        .unwrap_or_else(|| model.as_deref().map_or("fireworks", models::api_provider));
    let header_value =
        |text: &str| HeaderValue::from_str(text).map_err(|_| UpstreamError::InvalidConfig(
            format!("factory: `{text}` is not a header value"),
        ));
    headers.insert(HeaderName::from_static("x-api-provider"), header_value(provider)?);
    headers.insert(
        HeaderName::from_static("x-provider-routing-source"),
        HeaderValue::from_static("registry_default"),
    );
    headers.insert(HeaderName::from_static("x-assistant-message-id"), header_value(&uuid()?)?);
    if !headers.contains_key("x-session-id") {
        headers.insert(HeaderName::from_static("x-session-id"), header_value(&uuid()?)?);
    }
    if dialect == Dialect::Claude {
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
    }
    if provider == "openai" {
        headers.insert(
            HeaderName::from_static("openai-platform"),
            HeaderValue::from_static(OPENAI_PLATFORM),
        );
    }
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    let body = if is_generation(ctx.operation.operation) {
        transform_body(bytes, dialect, model.as_deref())
    } else {
        bytes
    };
    if !body.is_empty() {
        headers
            .entry(header::CONTENT_TYPE)
            .or_insert_with(|| HeaderValue::from_static("application/json"));
    }
    let mut builder = http::Request::builder()
        .method(ctx.request.method.clone())
        .uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    builder
        .body(HttpBody::Bytes(body))
        .map_err(|error| UpstreamError::InvalidConfig(error.to_string()))
}

/// The wire a request for `model` takes, as a dialect.
pub(super) fn dialect(model: Option<&str>) -> Dialect {
    models::wire(model).dialect()
}

pub(super) fn is_gemini(model: Option<&str>) -> bool {
    models::wire(model) == Wire::Gemini
}
