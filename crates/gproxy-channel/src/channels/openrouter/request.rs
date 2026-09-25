//! URL, authentication, attribution headers and the body OpenRouter wants.

use super::config::{DEFAULT_BASE_URL, OpenRouterConfig};
use super::routing;
use crate::channel::{ChannelError, ChannelHeaders, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::cache;
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation, connection::Bytes};
use http::{HeaderMap, HeaderName, HeaderValue, header};

/// Attribution headers a client may set for itself. A provider allow-list
/// narrows what *other* headers a client may add; it must not silence the
/// caller's own app identity, which is what OpenRouter ranks apps by.
pub const ATTRIBUTION_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["http-referer", "x-title"],
    prefixes: &[],
};

/// `base_url` plus the native path. Image creation and editing share one
/// endpoint; every other operation keeps the path the caller's dialect uses.
pub(super) fn target<B>(ctx: &PrepareContext<'_, B>) -> String {
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => {
            let base = ctx
                .provider
                .base_url
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/');
            match ctx.operation.operation {
                Operation::CreateImage | Operation::EditImage => format!("{base}/v1/images"),
                _ if ctx.request.path.starts_with('/') => format!("{base}{}", ctx.request.path),
                _ => format!("{base}/{}", ctx.request.path),
            }
        }
    };
    match ctx
        .request
        .query
        .as_deref()
        .map(strip_query_auth)
        .filter(|query| !query.is_empty())
    {
        Some(query) => format!("{url}?{query}"),
        None => url,
    }
}

pub(super) fn headers<B>(
    ctx: &PrepareContext<'_, B>,
    config: &OpenRouterConfig,
) -> Result<HeaderMap, ChannelError> {
    let key = ctx
        .credential
        .secret
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, ATTRIBUTION_HEADERS)?;
    let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    for (name, value) in [
        ("http-referer", config.referer.as_deref()),
        ("x-title", config.title.as_deref()),
    ] {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        // The caller named its own app; configuration is only the fallback.
        if headers.contains_key(name) {
            continue;
        }
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(value)
                .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
        );
    }
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    Ok(headers)
}

/// Shape a buffered JSON body: magic cache breakpoints, the service tier
/// OpenRouter names differently, the routing preferences and the usage
/// accounting switch. A body that needs none of them, a streamed body and a
/// body that is not a JSON object are forwarded byte for byte.
pub(super) fn body(
    body: HttpBody,
    config: &OpenRouterConfig,
    dialect: Dialect,
    operation: Operation,
) -> Result<HttpBody, ChannelError> {
    let HttpBody::Bytes(bytes) = body else {
        return Ok(body);
    };
    let rules = cache::rules_for(
        dialect,
        config.enable_claude_magic_cache,
        config.enable_openai_magic_cache,
    );
    let tokens = cache::contains_token(&bytes);
    let Some(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .filter(serde_json::Value::is_object)
    else {
        return Ok(HttpBody::Bytes(bytes));
    };
    let mut changed = tokens;
    if tokens {
        match rules {
            Some(rules) => cache::apply(&mut value, rules),
            None => {
                cache::strip_tokens(&mut value);
            }
        }
    }
    if matches!(operation, Operation::GenerateContent | Operation::StreamGenerateContent)
        && matches!(dialect, Dialect::Claude | Dialect::OpenAiChat)
    {
        changed |= crate::channels::shared::claude_fallback::reseller(
            &mut value, &config.fallback_mode, &config.fallback_models, dialect == Dialect::Claude,
        );
    }
    changed |= routing::apply(&mut value, config)?;
    if !changed {
        return Ok(HttpBody::Bytes(bytes));
    }
    Ok(HttpBody::Bytes(Bytes::from(value.to_string())))
}
