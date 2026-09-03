use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::Operation;
use http::header::{HeaderName, HeaderValue};
use serde_json::Value;

const CREATE_IMAGE_VERSION: &str = "preview";
const EDIT_IMAGE_VERSION: &str = "2025-04-01";

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let target = super::model::target(&ctx)?;
    let exact = endpoint_override(&ctx, target.endpoint);
    let query = query(&ctx, exact.as_deref())?;
    let uri = endpoint(&ctx, &target.path, exact.as_deref(), query.as_deref())?;
    let mut headers = crate::policy::request_headers(crate::policy::AZURE, &ctx)?;
    apply_auth(&mut headers, target.auth, api_key)?;
    let body = super::shape::request(&ctx)?;
    let mut request = http::Request::builder()
        .method(target.method)
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

fn endpoint(
    ctx: &PrepareCtx<'_>,
    path: &str,
    exact: Option<&str>,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(exact) = exact {
        return crate::shared::http::exact(exact, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .ok_or_else(|| ChannelError::Prepare("provider setting base_url missing".into()))?;
    crate::shared::http::join(base, path, query)
}

fn endpoint_override(ctx: &PrepareCtx<'_>, name: &str) -> Option<String> {
    let url = ctx
        .provider_settings
        .get("endpoints")?
        .get(name)?
        .as_str()?
        .trim();
    (!url.is_empty()).then(|| {
        url.replace(
            "{model}",
            &crate::shared::http::encode_component(ctx.upstream_model.trim()),
        )
    })
}

fn query(ctx: &PrepareCtx<'_>, exact: Option<&str>) -> Result<Option<String>, ChannelError> {
    let exact_has_version = exact
        .and_then(|url| url.split_once('?').map(|(_, query)| query))
        .is_some_and(|query| has_query(query, "api-version"));
    let caller_query = crate::policy::request_query(crate::policy::AZURE, ctx)?;
    let mut parts = caller_query
        .as_deref()
        .unwrap_or_default()
        .split('&')
        .filter(|part| {
            let name = part.split('=').next().unwrap_or_default();
            !part.is_empty() && !(exact_has_version && name == "api-version")
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if image_version(ctx.key.operation()).is_some()
        && !exact_has_version
        && !parts.iter().any(|part| part_name(part) == "api-version")
    {
        let version = ctx
            .provider_settings
            .get("api_version")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .or_else(|| image_version(ctx.key.operation()))
            .expect("image operation has a default version");
        parts.push(format!(
            "api-version={}",
            crate::shared::http::encode_component(version)
        ));
    }
    Ok((!parts.is_empty()).then(|| parts.join("&")))
}

fn image_version(operation: Operation) -> Option<&'static str> {
    match operation {
        Operation::CreateImage => Some(CREATE_IMAGE_VERSION),
        Operation::EditImage => Some(EDIT_IMAGE_VERSION),
        _ => None,
    }
}

fn has_query(query: &str, name: &str) -> bool {
    query.split('&').any(|part| part_name(part) == name)
}

fn part_name(part: &str) -> &str {
    part.split('=').next().unwrap_or_default()
}

fn apply_auth(
    headers: &mut http::HeaderMap,
    kind: super::model::AuthKind,
    key: &str,
) -> Result<(), ChannelError> {
    let (name, version) = match kind {
        super::model::AuthKind::OpenAi => (HeaderName::from_static("api-key"), None),
        super::model::AuthKind::Claude => (
            HeaderName::from_static("x-api-key"),
            Some(HeaderValue::from_static("2023-06-01")),
        ),
    };
    headers.insert(
        name,
        HeaderValue::from_str(key)
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    if let Some(version) = version {
        headers.insert(HeaderName::from_static("anthropic-version"), version);
    }
    Ok(())
}
