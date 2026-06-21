//! Shared plumbing for the API-key bulletins: base_url / api_key resolution,
//! request assembly (auth-free), and auth-header injection primitives. Each
//! channel folder's `auth.rs` composes these into its own auth.

use bytes::Bytes;
use http::Request;
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};

use crate::channel::http_util::{
    allow_headers, allow_query, build_request as build_http, join_url,
};
use crate::channel::{ChannelError, PrepareCtx};

/// Per-channel defaults consumed by [`build_request`] / [`resolve_base_url`].
pub struct ApiKeyDefaults {
    /// Baked default base_url; `None` = `settings_json.base_url` is required.
    pub default_base_url: Option<&'static str>,
    /// Inbound headers this channel forwards upstream (channel allow-list).
    pub forward_headers: &'static [&'static str],
    /// Inbound query params this channel forwards upstream.
    pub forward_query: &'static [&'static str],
}

/// Resolve the upstream base_url: `settings_json.base_url` overrides the baked
/// default; error if neither is present.
pub fn resolve_base_url(ctx: &PrepareCtx<'_>, d: &ApiKeyDefaults) -> Result<String, ChannelError> {
    if let Some(s) = ctx
        .provider_settings
        .get("base_url")
        .and_then(|v| v.as_str())
    {
        let s = s.trim();
        if !s.is_empty() {
            return Ok(s.to_string());
        }
    }
    d.default_base_url
        .map(str::to_string)
        .ok_or(ChannelError::MissingSetting("base_url"))
}

/// Resolve the credential api_key from `secret_json.api_key`.
pub fn resolve_api_key(ctx: &PrepareCtx<'_>) -> Result<String, ChannelError> {
    ctx.secret
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ChannelError::InvalidCredential("missing api_key".into()))
}

/// Build the upstream request for a header-auth channel: resolve base_url +
/// api_key, allow-list query/headers, join to an absolute URL, assemble the
/// request **without auth** (the channel's `auth.rs` injects it). Returns the
/// request plus the resolved api_key.
pub fn build_request(
    ctx: PrepareCtx<'_>,
    d: &ApiKeyDefaults,
) -> Result<(Request<Bytes>, String), ChannelError> {
    let base_url = resolve_base_url(&ctx, d)?;
    let api_key = resolve_api_key(&ctx)?;
    let query = allow_query(ctx.query, d.forward_query);
    let uri = join_url(&base_url, ctx.path, query.as_deref())?;
    let headers = allow_headers(ctx.headers, d.forward_headers);
    let req = build_http(ctx.method, uri, headers, ctx.body)?;
    Ok((req, api_key))
}

/// Inject `Authorization: Bearer <key>`.
pub fn inject_bearer(req: &mut Request<Bytes>, key: &str) -> Result<(), ChannelError> {
    let v = HeaderValue::from_str(&format!("Bearer {key}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad api_key: {e}")))?;
    req.headers_mut().insert(AUTHORIZATION, v);
    Ok(())
}

/// Insert `name: <value>` from a runtime string (e.g. an api-key header).
pub fn inject_header(
    req: &mut Request<Bytes>,
    name: HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    let v = HeaderValue::from_str(value)
        .map_err(|e| ChannelError::InvalidCredential(format!("bad header value: {e}")))?;
    req.headers_mut().insert(name, v);
    Ok(())
}

/// Insert `name: <value>` from a static string (e.g. `anthropic-version`).
pub fn inject_static(req: &mut Request<Bytes>, name: HeaderName, value: &'static str) {
    req.headers_mut()
        .insert(name, HeaderValue::from_static(value));
}

/// Append `key=<api_key>` to an allow-listed query string (Gemini `?key=` auth).
/// API keys are URL-safe in practice, so no percent-encoding is applied.
pub fn with_key_query(query: Option<String>, api_key: &str) -> Option<String> {
    let pair = format!("key={api_key}");
    Some(match query {
        Some(q) if !q.is_empty() => format!("{q}&{pair}"),
        _ => pair,
    })
}
