use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue};
use serde_json::Value;

const ZEN_BASE_URL: &str = "https://opencode.ai/zen/v1";
const GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";
pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let key = super::auth::api_key(ctx.secret)?;
    let tier = tier(ctx.provider_settings)?;
    let path = super::model::path(ctx.key);
    let uri = endpoint(&ctx, path, tier)?;
    let mut headers = crate::policy::request_headers(crate::policy::OPENCODE, &ctx)?;
    if ctx.key.operation().spec().affinity == gproxy_protocol::Affinity::Session {
        let session = headers
            .get("x-opencode-session")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| ctx.session_id.map(str::trim).filter(|value| !value.is_empty()))
            .ok_or_else(|| ChannelError::Prepare(
                "OpenCode requires x-opencode-session when no conversation identity can be inferred".into(),
            ))?;
        let value = HeaderValue::from_str(session)
            .map_err(|_| ChannelError::Prepare("invalid OpenCode session id".into()))?;
        headers.insert("x-opencode-session", value);
    }
    let body = super::model::body(&ctx)?;
    let body = super::shape::request(&ctx, &mut headers, body)?;
    if !body.is_empty() && !headers.contains_key(CONTENT_TYPE) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    if super::model::is_claude(ctx.key) {
        insert(&mut headers, HeaderName::from_static("x-api-key"), key)?;
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
    } else {
        insert(&mut headers, AUTHORIZATION, &format!("Bearer {key}"))?;
    }
    let mut request = http::Request::builder()
        .method(ctx.method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket: false,
        profile: None,
    })
}

fn endpoint(ctx: &PrepareCtx<'_>, path: &str, tier: Tier) -> Result<http::Uri, ChannelError> {
    if let Some(name) = super::model::endpoint_name(ctx.key)
        && let Some(url) = ctx
            .provider_settings
            .get("endpoints")
            .and_then(|endpoints| endpoints.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
    {
        return crate::shared::http::exact(
            &url.replace(
                "{model}",
                &crate::shared::http::encode_component(ctx.upstream_model),
            ),
            None,
        );
    }
    if let Some(base) = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
    {
        return crate::shared::http::join(base, path, None);
    }
    crate::shared::http::join(
        if tier == Tier::Go {
            GO_BASE_URL
        } else {
            ZEN_BASE_URL
        },
        path,
        None,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    Zen,
    Go,
}

fn tier(settings: &Value) -> Result<Tier, ChannelError> {
    match settings.get("tier") {
        None => Ok(Tier::Zen),
        Some(Value::String(tier)) if tier == "zen" => Ok(Tier::Zen),
        Some(Value::String(tier)) if tier == "go" => Ok(Tier::Go),
        Some(Value::String(tier)) => Err(ChannelError::Prepare(format!(
            "unknown OpenCode tier `{tier}`"
        ))),
        Some(_) => Err(ChannelError::Prepare(
            "OpenCode tier must be `zen` or `go`".into(),
        )),
    }
}

fn insert(
    headers: &mut http::HeaderMap,
    name: HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        name,
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Secret(format!("OpenCode key: {error}")))?,
    );
    Ok(())
}
