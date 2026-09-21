//! Where an Azure method lives. [`method_url`] is the single place this
//! channel computes an upstream URL; everything else in this file exists to
//! serve it.

use super::config::{AzureConfig, setting};
use crate::channel::ChannelError;
use gproxy_protocol::{Operation, OperationKey, WireFamily};

/// Azure's image operations are not on the version-less v1 surface, so they
/// carry a version of their own when the provider configured none. These are
/// the values the v3 channel shipped and Azure's image reference documents.
fn default_api_version(operation: Operation) -> Option<&'static str> {
    match operation {
        Operation::CreateImage => Some("preview"),
        Operation::EditImage => Some("2025-04-01"),
        _ => None,
    }
}

/// The complete upstream method URL for one request.
///
/// An `endpoint_override` is the operator's own complete method URL and wins
/// outright: only the query is still assembled around it. Otherwise one of two
/// layouts is built, chosen by whether the provider configured a `deployment`:
///
/// - **v1 surface** (no `deployment`): `{origin}/openai{path}` for the OpenAI
///   family and `{origin}/anthropic{path}` for Claude, where `path` is the
///   vendor's own native path exactly as it arrived. `api-version` is sent
///   only when the provider configured one or the operation has a channel
///   default.
/// - **deployment surface** (`deployment` set): `{origin}/openai/deployments/
///   {deployment}{path}` for the OpenAI family, with the path's leading `/v1`
///   removed, and `api-version` is mandatory. The model directory is not
///   deployment-scoped and stays at `/openai/models`. Claude keeps the v1
///   surface: Foundry's Anthropic endpoint is not deployment-scoped either.
///
/// `origin` is the provider's `base_url` when it has one, otherwise
/// `https://{resource}.openai.azure.com`. The caller's own query survives,
/// minus its authentication parameters; an `api-version` already present in
/// the override or the caller's query is left alone rather than duplicated.
pub(super) fn method_url(
    config: &AzureConfig,
    base_url: Option<&str>,
    endpoint_override: Option<&str>,
    operation: OperationKey,
    path: &str,
    query: Option<&str>,
) -> Result<String, ChannelError> {
    let (url, inherited) = match endpoint_override.map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => match url.split_once('?') {
            Some((head, tail)) => (head.to_owned(), Some(tail)),
            None => (url.to_owned(), None),
        },
        None => (
            format!(
                "{}{}",
                origin(config, base_url)?,
                family_path(config, operation, path)?
            ),
            None,
        ),
    };
    Ok(
        match query_string(config, operation.operation, inherited, query)? {
            Some(query) => format!("{url}?{query}"),
            None => url,
        },
    )
}

fn origin(config: &AzureConfig, base_url: Option<&str>) -> Result<String, ChannelError> {
    if let Some(base) = base_url.map(str::trim).filter(|base| !base.is_empty()) {
        return Ok(base.trim_end_matches('/').to_owned());
    }
    let resource = setting(config.resource.as_deref()).ok_or_else(|| {
        ChannelError::InvalidConfig(
            "config key `resource` is required when the provider has no base_url".into(),
        )
    })?;
    validate_segment("resource", resource)?;
    Ok(format!("https://{resource}.openai.azure.com"))
}

fn family_path(
    config: &AzureConfig,
    operation: OperationKey,
    path: &str,
) -> Result<String, ChannelError> {
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    match operation.dialect.family() {
        WireFamily::OpenAi => match setting(config.deployment.as_deref()) {
            Some(deployment) => {
                validate_segment("deployment", deployment)?;
                let method = path.strip_prefix("/v1").unwrap_or(&path);
                // The model directory is a property of the resource, not of a
                // deployment, and stays at `/openai/models` on this surface.
                if method == "/models" || method.starts_with("/models/") {
                    Ok(format!("/openai{method}"))
                } else {
                    Ok(format!("/openai/deployments/{deployment}{method}"))
                }
            }
            None => Ok(format!("/openai{path}")),
        },
        WireFamily::Claude => Ok(format!("/anthropic{path}")),
        WireFamily::Gemini => Err(ChannelError::UnsupportedOperation(operation)),
    }
}

fn query_string(
    config: &AzureConfig,
    operation: Operation,
    inherited: Option<&str>,
    query: Option<&str>,
) -> Result<Option<String>, ChannelError> {
    let configured = setting(config.api_version.as_deref());
    if setting(config.deployment.as_deref()).is_some() && configured.is_none() {
        return Err(ChannelError::InvalidConfig(
            "config key `api_version` is required when `deployment` is set".into(),
        ));
    }
    let mut parts: Vec<String> = [inherited, query]
        .into_iter()
        .flatten()
        .flat_map(|source| source.split('&'))
        .filter(|part| !part.is_empty() && !is_auth_parameter(part))
        .map(str::to_owned)
        .collect();
    if let Some(version) = configured.or_else(|| default_api_version(operation))
        && !parts.iter().any(|part| name_of(part) == "api-version")
    {
        parts.push(format!("api-version={version}"));
    }
    Ok((!parts.is_empty()).then(|| parts.join("&")))
}

/// A configured value that becomes part of a host name or a path segment. An
/// invalid one is a configuration mistake, not something to percent-encode
/// into a URL that would then silently address the wrong resource.
fn validate_segment(name: &'static str, value: &str) -> Result<(), ChannelError> {
    let valid = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(ChannelError::InvalidConfig(format!(
            "config key `{name}` may only contain letters, digits, `-`, `_` and `.`"
        )))
    }
}

/// Source authentication never reaches the upstream, in the query either.
fn is_auth_parameter(part: &str) -> bool {
    matches!(name_of(part), "key" | "access_token" | "api_key")
}

fn name_of(part: &str) -> &str {
    part.split('=').next().unwrap_or_default()
}
