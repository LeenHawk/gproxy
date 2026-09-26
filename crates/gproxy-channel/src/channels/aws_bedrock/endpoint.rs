//! Which AWS host and path serves an operation.
//!
//! Bedrock has two regional planes and the channel signs both as the
//! `bedrock` service:
//!
//! - the runtime plane, `bedrock-runtime.{region}.amazonaws.com`, whose
//!   generation paths carry the model id and differ between buffered and
//!   streamed replies (`/model/{id}/invoke` against
//!   `/model/{id}/invoke-with-response-stream`);
//! - the control plane, `bedrock.{region}.amazonaws.com`, whose
//!   `/foundation-models` directory answers model discovery.
//!
//! The model id is read from the request body's `model` field: v4's
//! `PrepareContext` carries no `upstream_model`, and the host has already
//! rewritten the body to the upstream name by the time the channel sees it.

use http::Method;
use serde_json::Value;

use super::config::BedrockConfig;
use super::sigv4;
use crate::channel::{ChannelError, ProviderView};

/// The SigV4 service name for both Bedrock planes.
pub const SIGNING_SERVICE: &str = "bedrock";

/// The only `/foundation-models` filters forwarded; every other client
/// parameter is dropped rather than signed and passed on (v3
/// `policy::AWS_BEDROCK`).
const MODEL_FILTERS: &[&str] = &[
    "byCustomizationType",
    "byInferenceType",
    "byOutputModality",
    "byProvider",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Plane {
    Runtime,
    Control,
}

/// The origin for `plane`, honouring the provider's `base_url` (runtime) and
/// `config.control_base_url` (control) overrides.
pub(super) fn origin(
    provider: ProviderView<'_>,
    config: &BedrockConfig,
    plane: Plane,
) -> Result<String, ChannelError> {
    let region = config.region()?;
    let configured = match plane {
        Plane::Runtime => provider.base_url,
        Plane::Control => config.control_base_url.as_deref(),
    }
    .map(str::trim)
    .filter(|url| !url.is_empty());
    Ok(match configured {
        Some(url) => url.trim_end_matches('/').to_owned(),
        None => match plane {
            Plane::Runtime => format!("https://bedrock-runtime.{region}.amazonaws.com"),
            Plane::Control => format!("https://bedrock.{region}.amazonaws.com"),
        },
    })
}

/// The complete URL for an operation, or the host's `endpoint_override` with
/// `{model}` substituted. Overrides win over both origins and the path.
pub(super) fn url(
    provider: ProviderView<'_>,
    config: &BedrockConfig,
    plane: Plane,
    path: &str,
    query: Option<&str>,
    model: Option<&str>,
    endpoint_override: Option<&str>,
) -> Result<String, ChannelError> {
    let base = match endpoint_override
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        Some(url) => match model {
            Some(model) => url.replace("{model}", &encode_segment(model)),
            None => url.to_owned(),
        },
        None => format!("{}{path}", origin(provider, config, plane)?),
    };
    Ok(match query.filter(|query| !query.is_empty()) {
        Some(query) if base.contains('?') => format!("{base}&{query}"),
        Some(query) => format!("{base}?{query}"),
        None => base,
    })
}

/// `/model/{id}/invoke` or `/model/{id}/invoke-with-response-stream`.
pub(super) fn invoke_path(model: &str, stream: bool) -> String {
    let method = if stream {
        "invoke-with-response-stream"
    } else {
        "invoke"
    };
    format!("/model/{}/{method}", encode_segment(model))
}

/// `/foundation-models`, optionally for one model.
pub(super) fn foundation_models_path(model: Option<&str>) -> String {
    match model {
        Some(model) => format!("/foundation-models/{}", encode_segment(model)),
        None => "/foundation-models".to_owned(),
    }
}

/// The client's `/foundation-models` filters, keeping their original order
/// and encoding.
pub(super) fn model_filters(query: Option<&str>) -> Option<String> {
    let filtered = query?
        .split('&')
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or_default();
            MODEL_FILTERS.contains(&name)
        })
        .collect::<Vec<_>>()
        .join("&");
    (!filtered.is_empty()).then_some(filtered)
}

/// The Bedrock model id from a generation body. Inference profile ids
/// (`us.anthropic.…`) and ARNs both arrive here as the body's `model`.
pub(super) fn model_from_body(body: &[u8]) -> Result<String, ChannelError> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .as_ref()
        .and_then(|value| value.get("model"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ChannelError::InvalidConfig(
                "the request body carries no `model`; Bedrock addresses the model in the URL"
                    .into(),
            )
        })
}

/// The path of Bedrock's OpenAI-compatible Chat Completions on the runtime
/// plane. The model stays in the body, as OpenAI's wire has it.
pub(super) const CHAT_COMPLETIONS_PATH: &str = "/openai/v1/chat/completions";

/// Whether `model` is served on InvokeModel with Anthropic's Messages body
/// rather than on the OpenAI-compatible Chat Completions.
///
/// Anthropic models do not take Chat Completions on Bedrock, and GPT-5.x or
/// Grok take nothing else, so the wire follows the model. Anthropic ids
/// (`anthropic.…`), their cross-region profiles (`us.anthropic.…`) and ARNs
/// ending in either are Messages. An application inference profile's ARN
/// names no family; it stays on Messages, the only wire this channel had
/// before it could tell.
pub(super) fn serves_messages(model: &str) -> bool {
    let model = model.trim();
    let tail = model.rsplit('/').next().unwrap_or(model);
    tail.contains("anthropic.")
        || (model.starts_with("arn:") && model.contains(":application-inference-profile/"))
}

/// The model id from a `/v1/models/{id}` style path.
pub(super) fn model_from_path(path: &str) -> Result<String, ChannelError> {
    let segment = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if segment.is_empty() {
        return Err(ChannelError::InvalidConfig(
            "the request path names no model".into(),
        ));
    }
    percent_decode_segment(segment)
}

fn encode_segment(value: &str) -> String {
    sigv4::encode(value.as_bytes())
}

fn percent_decode_segment(value: &str) -> Result<String, ChannelError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes
                .get(index + 1..index + 3)
                .and_then(|pair| std::str::from_utf8(pair).ok())
                .and_then(|text| u8::from_str_radix(text, 16).ok())
                .ok_or_else(|| {
                    ChannelError::InvalidConfig("the model id is not percent encoded".into())
                })?;
            out.push(pair);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out)
        .map_err(|_| ChannelError::InvalidConfig("the model id is not UTF-8".into()))
}

pub(super) fn method_for(plane: Plane) -> Method {
    match plane {
        Plane::Runtime => Method::POST,
        Plane::Control => Method::GET,
    }
}
