//! The Code Assist envelope and the bounded HTTP the Gemini-family CLI
//! channels share.
//!
//! `geminicli` and `antigravity` both impersonate a Google tool talking to a
//! `cloudcode-pa.googleapis.com`-style internal host with a Google account's
//! OAuth access token, and both use the same envelope: the Gemini request is
//! nested under `request`, the model id, the Cloud project and a per-prompt
//! id sit beside it, and every reply (one JSON object, or one SSE `data:`
//! payload per chunk) carries the Gemini response under `response`.
//!
//! Wire facts: v3 `crates/gproxy-channels/src/shared/code_assist.rs` on `main`
//! and the Gemini CLI's own converter
//! (`samples/geminicli/packages/core/src/code_assist/converter.ts`:
//! `CAGenerateContentRequest`, `CaGenerateContentResponse`,
//! `CaCountTokenRequest`).
//!
//! Policy stays in each channel; this module only executes what one asks for.

pub(crate) mod google;
pub(crate) mod quota;
pub(crate) mod stream;

use crate::OutboundClient;
use crate::channel::ChannelError;
use futures_util::StreamExt;
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{HttpBody, WireResponse};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Map, Value, json};

/// Bodies these channels read themselves (login, refresh, quota, catalogs).
const MAX_SERVICE_BODY: usize = 8 * 1024 * 1024;

pub(crate) fn invalid_request(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
}

pub(crate) fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

/// The bare model id: `models/gemini-3-pro` and
/// `projects/p/locations/l/models/gemini-3-pro` both reduce to the last
/// segment (v3 `shared/gemini/model.rs::model_id`).
pub(crate) fn model_id(model: &str) -> &str {
    let model = model.trim();
    model
        .rsplit_once("/models/")
        .map(|(_, id)| id)
        .unwrap_or_else(|| model.strip_prefix("models/").unwrap_or(model))
}

/// The model a native Gemini request names: the `{model}:{method}` segment of
/// the path the client sent, else the body's own `model` field. v4 hands
/// `prepare` the client's request, not the resolved upstream model, so the
/// model is read back out of the request the same way the Gemini API does.
pub(crate) fn request_model(path: &str, body: Option<&Value>) -> Option<String> {
    let from_path = path
        .rsplit_once("/models/")
        .map(|(_, tail)| tail)
        .and_then(|tail| tail.split(':').next())
        .map(str::trim)
        .filter(|model| !model.is_empty() && !model.contains('/'));
    if let Some(model) = from_path {
        return Some(model.to_owned());
    }
    let body = body?;
    let named = body
        .get("model")
        .or_else(|| body.pointer("/request/model"))
        .and_then(Value::as_str)
        .map(model_id)
        .map(str::trim)
        .filter(|model| !model.is_empty())?;
    Some(named.to_owned())
}

/// A random 32-hex `user_prompt_id`, as the CLI mints one per prompt
/// (v3 `shared/code_assist.rs::prompt_id`).
pub(crate) fn prompt_id() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| invalid_request("Code Assist prompt id randomness failed"))?;
    use std::fmt::Write as _;
    let mut output = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    Ok(output)
}

/// Options Code Assist rejects once the Gemini body is nested under `request`
/// (v3 `sanitize_value`). Fields named `store` inside parts stay valid data,
/// so only the root key is dropped.
fn sanitize(request: &mut Value) -> Result<(), ChannelError> {
    let object = request
        .as_object_mut()
        .ok_or_else(|| invalid_request("Gemini request body must be an object"))?;
    object.remove("store");
    if let Some(config) = object
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
    {
        for name in [
            "maxOutputTokens",
            "max_output_tokens",
            "logprobs",
            "responseLogprobs",
            "response_logprobs",
        ] {
            config.remove(name);
        }
    }
    // A tool list that declares functions may not also carry the vendor's
    // built-in tool entries.
    if let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut)
        && tools.iter().any(|tool| {
            tool.get("functionDeclarations")
                .and_then(Value::as_array)
                .is_some_and(|declarations| !declarations.is_empty())
        })
    {
        tools.retain(|tool| tool.get("functionDeclarations").is_some());
    }
    Ok(())
}

/// Code Assist rejects a content without an explicit role (v3 `force_roles`).
fn force_roles(request: &mut Value) {
    if let Some(contents) = request.get_mut("contents").and_then(Value::as_array_mut) {
        for content in contents {
            if let Some(content) = content.as_object_mut() {
                content
                    .entry("role")
                    .or_insert_with(|| Value::String("user".into()));
            }
        }
    }
}

/// True when the client already sent a Code Assist envelope rather than a
/// plain Gemini body: a Gemini `GenerateContentRequest` never has a nested
/// `request` object, and an envelope never carries `contents` at its root.
fn is_envelope(body: &Value) -> bool {
    body.get("request").is_some_and(Value::is_object) && body.get("contents").is_none()
}

/// `{model, project, user_prompt_id, request}`. A plain Gemini body becomes
/// the `request`; an envelope the client already built keeps its own
/// `request` (and therefore its `request.session_id` / `request.sessionId`,
/// which this channel never renames or drops) and only has the routed model,
/// the discovered project and a prompt id filled in where it left them out.
pub(crate) fn envelope(body: &[u8], model: &str, project: &str) -> Result<Bytes, ChannelError> {
    let parsed: Value = serde_json::from_slice(body)
        .map_err(|error| invalid_request(format!("Gemini request body JSON: {error}")))?;
    let mut envelope = if is_envelope(&parsed) {
        parsed
            .as_object()
            .cloned()
            .ok_or_else(|| invalid_request("Code Assist envelope must be an object"))?
    } else {
        let mut object = Map::new();
        object.insert("request".into(), parsed);
        object
    };
    let mut request = envelope
        .remove("request")
        .unwrap_or_else(|| Value::Object(Map::new()));
    sanitize(&mut request)?;
    force_roles(&mut request);
    envelope.insert("request".into(), request);
    envelope.insert("model".into(), Value::String(model_id(model).to_owned()));
    if project.is_empty() {
        envelope.remove("project");
    } else {
        envelope.insert("project".into(), Value::String(project.to_owned()));
    }
    if !envelope
        .get("user_prompt_id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty())
    {
        envelope.insert("user_prompt_id".into(), Value::String(prompt_id()?));
    }
    encode(&Value::Object(envelope), invalid_request)
}

/// `{request: {...}}` for `:countTokens`. A client may send either the
/// Gemini `countTokens` shape (`contents`, optionally wrapped in
/// `generateContentRequest`) or the envelope itself (v3 `wrap_count`).
pub(crate) fn count_envelope(body: &[u8], model: &str) -> Result<Bytes, ChannelError> {
    let parsed: Value = serde_json::from_slice(body)
        .map_err(|error| invalid_request(format!("count tokens body JSON: {error}")))?;
    let mut request = if is_envelope(&parsed) {
        parsed["request"].clone()
    } else if let Some(inner) = parsed.get("generateContentRequest") {
        inner.clone()
    } else {
        let mut request = Map::new();
        if let Some(contents) = parsed.get("contents") {
            request.insert("contents".into(), contents.clone());
        }
        Value::Object(request)
    };
    sanitize(&mut request)?;
    force_roles(&mut request);
    // The CLI's `toCountTokenRequest` sends the qualified `models/<id>` name.
    if let Some(object) = request.as_object_mut()
        && !model.is_empty()
    {
        object.insert(
            "model".into(),
            Value::String(format!("models/{}", model_id(model))),
        );
    }
    encode(&json!({ "request": request }), invalid_request)
}

/// The Gemini payload inside a Code Assist reply; a reply that is already
/// unwrapped (an error object, for instance) is returned as it is.
pub(crate) fn unwrap_value(value: &Value) -> &Value {
    value
        .get("response")
        .filter(|inner| inner.is_object())
        .unwrap_or(value)
}

/// Unwrap a buffered reply and apply the Vertex-shaped normalizations the
/// Gemini API clients expect (v3 `shared/gemini/vertex.rs::normalize_content`):
/// `citationMetadata.citations` is the Vertex spelling of `citationSources`,
/// and `BLOCKED_REASON_UNSPECIFIED` is the Vertex spelling of
/// `BLOCK_REASON_UNSPECIFIED`.
pub(crate) fn unwrap_body(body: &[u8]) -> Result<Bytes, ChannelError> {
    let parsed: Value = serde_json::from_slice(body)
        .map_err(|error| invalid_response(format!("Code Assist response JSON: {error}")))?;
    let mut value = unwrap_value(&parsed).clone();
    normalize_content(&mut value);
    encode(&value, invalid_response)
}

pub(crate) fn normalize_content(value: &mut Value) {
    if let Some(candidates) = value.get_mut("candidates").and_then(Value::as_array_mut) {
        for candidate in candidates {
            let Some(metadata) = candidate
                .get_mut("citationMetadata")
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            if let Some(citations) = metadata.remove("citations") {
                metadata.entry("citationSources").or_insert(citations);
            }
        }
    }
    if let Some(reason) = value.pointer_mut("/promptFeedback/blockReason")
        && reason.as_str() == Some("BLOCKED_REASON_UNSPECIFIED")
    {
        *reason = Value::String("BLOCK_REASON_UNSPECIFIED".into());
    }
}

pub(crate) fn encode(
    value: &Value,
    error: impl FnOnce(String) -> ChannelError,
) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(|cause| error(cause.to_string()))
}

pub(crate) async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| invalid_response(error.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > MAX_SERVICE_BODY {
                    return Err(invalid_response("response exceeds the read limit"));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

/// One bounded JSON exchange through the assigned client.
pub(crate) async fn send(
    client: &dyn OutboundClient,
    method: Method,
    url: &str,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
) -> Result<(StatusCode, HeaderMap, Bytes), ChannelError> {
    let mut builder = http::Request::builder().method(method).uri(url);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    let request = builder
        .body(HttpBody::Bytes(body.map(Bytes::from).unwrap_or_default()))
        .map_err(|error| invalid_request(error.to_string()))?;
    let WireResponse {
        status,
        headers,
        body,
    } = client.send(request).await?;
    Ok((status, headers, read_body(body).await?))
}

pub(crate) fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A non-empty trimmed string field.
pub(crate) fn text<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
