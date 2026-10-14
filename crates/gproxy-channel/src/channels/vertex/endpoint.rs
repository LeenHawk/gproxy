//! Where a Vertex method lives. [`method_url`] is the single place this
//! channel computes an upstream URL; [`Place`] is the project, region and
//! origin it needs, resolved once per request.

use super::config::{DEFAULT_LOCATION, VertexConfig, setting};
use crate::channel::{ChannelError, CredentialView, ProviderView};
use crate::channels::shared::google_model;
use gproxy_protocol::{Dialect, Operation, OperationKey};

/// The Google Cloud coordinates every project-scoped method is addressed by.
pub(super) struct Place {
    pub project: String,
    pub location: String,
    pub origin: String,
}

/// Resolve project, region and origin from the provider's config, the
/// service-account key and the provider's `base_url`.
///
/// `project` comes from config first and from the key's own `project_id`
/// second; `location` from credential metadata, then the key, then config, then Google's default
/// `us-central1`. Both end up in the URL path, so both are validated rather
/// than encoded: a value with a `/` in it would silently address a different
/// resource.
pub(super) fn resolve(
    config: &VertexConfig,
    provider: ProviderView<'_>,
    credential: CredentialView<'_>,
) -> Result<Place, ChannelError> {
    let secret = |name: &str| {
        credential
            .secret
            .get(name)
            .and_then(serde_json::Value::as_str)
    };
    let project = setting(config.project.as_deref())
        .or_else(|| setting(secret("project_id")))
        .ok_or_else(|| {
            ChannelError::InvalidConfig(
                "config key `project` is required when the service-account key has no project_id"
                    .into(),
            )
        })?;
    validate_segment("project", project)?;
    let location = setting(
        credential
            .metadata
            .get("location")
            .and_then(serde_json::Value::as_str),
    )
    .or_else(|| setting(secret("location")))
    .or_else(|| setting(config.location.as_deref()))
    .unwrap_or(DEFAULT_LOCATION);
    validate_segment("location", location)?;
    let origin = match setting(provider.base_url) {
        Some(base) => base.trim_end_matches('/').to_owned(),
        // `global` is not a region and has no regional host of its own.
        None if location == "global" => "https://aiplatform.googleapis.com".to_owned(),
        None => format!("https://{location}-aiplatform.googleapis.com"),
    };
    Ok(Place {
        project: project.to_owned(),
        location: location.to_owned(),
        origin,
    })
}

/// The complete upstream method URL for one request.
///
/// An `endpoint_override` is the operator's own complete method URL and wins
/// outright; only the query is still assembled around it. Otherwise the
/// native path is mapped onto the publisher hierarchy the operation belongs
/// to, with `{p}` the project and `{l}` the location:
///
/// | operation, dialect | Vertex method |
/// |---|---|
/// | ListModels, Gemini | `/v1beta1/publishers/google/models` |
/// | GetModel, Gemini | `/v1beta1/publishers/{publisher}/models/{model}` |
/// | GenerateContent, Gemini | `/v1beta1/projects/{p}/locations/{l}/publishers/google/models/{model}:generateContent` |
/// | StreamGenerateContent, Gemini | the same, `:streamGenerateContent` |
/// | CountTokens, Gemini | the same, `:countTokens` |
/// | CreateEmbedding, Gemini | the same, `:embedContent` |
/// | BatchCreateEmbedding, Gemini | the same, `:batchEmbedContents` |
/// | GenerateContent, Claude | `/v1/projects/{p}/locations/{l}/publishers/anthropic/models/{model}:rawPredict` |
/// | StreamGenerateContent, Claude | the same, `:streamRawPredict` |
/// | CountTokens, Claude | the same, with the literal model `count-tokens` |
/// | GenerateContent / StreamGenerateContent, OpenAiChat | `/v1beta1/projects/{p}/locations/{l}/endpoints/openapi/chat/completions` |
///
/// `model` is the upstream model for the operations whose URL names one; the
/// caller reads it from the native path (Gemini carries it there) or from the
/// request body (Claude does). Anything else is `UnsupportedOperation`.
pub(super) fn method_url(
    place: &Place,
    endpoint_override: Option<&str>,
    operation: OperationKey,
    query: Option<&str>,
    model: Option<&str>,
) -> Result<String, ChannelError> {
    let (url, inherited) = match setting(endpoint_override) {
        Some(url) => match url.split_once('?') {
            Some((head, tail)) => (head.to_owned(), Some(tail)),
            None => (url.to_owned(), None),
        },
        None => (
            format!("{}{}", place.origin, method_path(place, operation, model)?),
            None,
        ),
    };
    let parts: Vec<&str> = [inherited, query]
        .into_iter()
        .flatten()
        .flat_map(|source| source.split('&'))
        .filter(|part| !part.is_empty() && !is_auth_parameter(part))
        .collect();
    Ok(if parts.is_empty() {
        url
    } else {
        format!("{url}?{}", parts.join("&"))
    })
}

fn method_path(
    place: &Place,
    operation: OperationKey,
    model: Option<&str>,
) -> Result<String, ChannelError> {
    let Place {
        project, location, ..
    } = place;
    let named = |publisher: &str, version: &str, verb: &str| -> Result<String, ChannelError> {
        let model = google_model::resource_path(required_model(operation, model)?, publisher)?;
        let resource = if model.starts_with("projects/") {
            model
        } else {
            format!("projects/{project}/locations/{location}/{model}")
        };
        Ok(format!("/{version}/{resource}{verb}"))
    };
    match (operation.operation, operation.dialect) {
        (Operation::ListModels, Dialect::Gemini) => {
            Ok("/v1beta1/publishers/google/models".to_owned())
        }
        (Operation::GetModel, Dialect::Gemini) => Ok(format!(
            "/v1beta1/{}",
            google_model::resource_path(required_model(operation, model)?, "google")?
        )),
        (Operation::GenerateContent, Dialect::Gemini) => {
            named("google", "v1beta1", ":generateContent")
        }
        (Operation::StreamGenerateContent, Dialect::Gemini) => {
            named("google", "v1beta1", ":streamGenerateContent")
        }
        (Operation::CountTokens, Dialect::Gemini) => named("google", "v1beta1", ":countTokens"),
        (Operation::CreateEmbedding, Dialect::Gemini) => {
            named("google", "v1beta1", ":embedContent")
        }
        (Operation::BatchCreateEmbedding, Dialect::Gemini) => {
            named("google", "v1beta1", ":batchEmbedContents")
        }
        (Operation::GenerateContent, Dialect::Claude) => named("anthropic", "v1", ":rawPredict"),
        (Operation::StreamGenerateContent, Dialect::Claude) => {
            named("anthropic", "v1", ":streamRawPredict")
        }
        // Anthropic's Vertex token counter is one endpoint for every model,
        // addressed by the literal model name `count-tokens`; the real model
        // stays in the body.
        (Operation::CountTokens, Dialect::Claude) => Ok(format!(
            "/v1/projects/{project}/locations/{location}/publishers/anthropic/models/count-tokens:rawPredict"
        )),
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAiChat) => {
            Ok(format!(
                "/v1beta1/projects/{project}/locations/{location}/endpoints/openapi/chat/completions"
            ))
        }
        _ => Err(ChannelError::UnsupportedOperation(operation)),
    }
}

fn required_model(operation: OperationKey, model: Option<&str>) -> Result<&str, ChannelError> {
    setting(model).ok_or_else(|| {
        ChannelError::InvalidConfig(format!(
            "{:?} on Vertex needs an upstream model; none was found in the request",
            operation.operation
        ))
    })
}

fn validate_segment(name: &'static str, value: &str) -> Result<(), ChannelError> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(ChannelError::InvalidConfig(format!(
            "config key `{name}` may only contain letters, digits, `-` and `_`"
        )))
    }
}

/// Source authentication never reaches the upstream, in the query either.
fn is_auth_parameter(part: &str) -> bool {
    matches!(
        part.split('=').next().unwrap_or_default(),
        "key" | "access_token" | "api_key"
    )
}
