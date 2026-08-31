use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest, SurfaceRequest};
use gproxy_protocol::Operation;
use http::{HeaderValue, Uri};
use serde_json::Value;

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    if ctx.stream
        && matches!(
            ctx.key.operation,
            Operation::CreateImage | Operation::EditImage
        )
    {
        return Err(ChannelError::Prepare(
            "Codex image streaming is not supported by the captured backend".into(),
        ));
    }
    let path = upstream_path(ctx.key.operation, ctx.upstream_model);
    let query = query(&ctx)?;
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    let mut headers = crate::policy::request_headers(crate::policy::CODEX, &ctx)?;
    let content_type = ctx.headers.get(http::header::CONTENT_TYPE).cloned();
    let session_id = super::auth::session_id(ctx.secret, &headers);
    super::auth::apply_headers(&mut headers, ctx.secret, &session_id)?;
    if ctx.key.operation == Operation::CreateRealtimeCall
        && content_type.as_ref().is_some_and(is_sdp)
        && let Some(content_type) = content_type
    {
        headers.insert(http::header::CONTENT_TYPE, content_type);
    }
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static(match ctx.key.operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => "text/event-stream",
            Operation::CreateRealtimeCall => "application/sdp",
            _ => "application/json",
        }),
    );
    let body = openai_cache(&ctx)?;
    let body = super::shape::request(ctx.key.operation, ctx.headers, &body, ctx.upstream_model)?;
    let mut request = http::Request::builder()
        .method(ctx.method)
        .uri(strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket: false,
        profile: Some(&super::profile::CLIENT_PROFILE),
    })
}

fn openai_cache(ctx: &PrepareCtx<'_>) -> Result<bytes::Bytes, ChannelError> {
    let gproxy_protocol::OperationKind::ContentGeneration(kind) = ctx.key.kind else {
        return Ok(ctx.body.clone());
    };
    if ctx
        .provider_settings
        .get("enable_openai_magic_cache")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Ok(ctx.body.clone());
    }
    let mut value = serde_json::from_slice(ctx.body)
        .map_err(|error| ChannelError::Prepare(format!("request body JSON: {error}")))?;
    crate::shared::openai::cache::apply(&mut value, kind);
    serde_json::to_vec(&value)
        .map(bytes::Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn surface(
    source: &SurfaceRequest,
    websocket: bool,
    provider_settings: &Value,
    secret: &Value,
) -> Result<PreparedRequest, ChannelError> {
    let base = provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(super::auth::DEFAULT_BASE_URL)
        .trim_end_matches('/');
    let base = base.strip_suffix("/codex").unwrap_or(base);
    let remote_bearer = matches!(source.label, "remote_control_ws" | "remote_control_token");
    let policy = crate::policy::CODEX
        .effective_traffic_policy(provider_settings)
        .map_err(ChannelError::Prepare)?;
    let caller_query = if remote_bearer {
        source.query.clone()
    } else {
        policy.filter_request_query(source.query.as_deref())
    };
    let query = surface_query(caller_query.as_deref(), remote_bearer);
    let uri = absolute_url(&format!("{base}{}", source.upstream_path), query.as_deref())?;
    let mut headers = policy.filter_request_headers(&source.headers);
    if remote_bearer && let Some(value) = source.headers.get(http::header::AUTHORIZATION) {
        headers.insert(http::header::AUTHORIZATION, value.clone());
    }
    let content_type = headers.get(http::header::CONTENT_TYPE).cloned();
    let accept = headers.get(http::header::ACCEPT).cloned();
    if !remote_bearer {
        let session_id = super::auth::session_id(secret, &headers);
        super::auth::apply_headers(&mut headers, secret, &session_id)?;
        if let Some(content_type) = content_type {
            headers.insert(http::header::CONTENT_TYPE, content_type);
        }
        if let Some(accept) = accept {
            headers.insert(http::header::ACCEPT, accept);
        }
    }
    let uri = if websocket { websocket_uri(uri)? } else { uri };
    let mut request = http::Request::builder()
        .method(&source.method)
        .uri(strip_userinfo(uri)?)
        .body(source.body.clone())
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket,
        profile: Some(&super::profile::CLIENT_PROFILE),
    })
}

fn upstream_path(operation: Operation, model: &str) -> String {
    match operation {
        Operation::ListModels => "/models".into(),
        Operation::GetModel => format!("/models/{}", encode_component(model)),
        Operation::SummarizeMemory => "/memories/trace_summarize".into(),
        Operation::GenerateContent | Operation::StreamGenerateContent => "/responses".into(),
        Operation::CompactContent => "/responses/compact".into(),
        Operation::CreateImage => "/images/generations".into(),
        Operation::EditImage => "/images/edits".into(),
        Operation::WebSearch => "/alpha/search".into(),
        Operation::CreateRealtimeCall => "/realtime/calls".into(),
        Operation::ConnectRealtime => "/realtime".into(),
        _ => "/unsupported".into(),
    }
}

fn query(ctx: &PrepareCtx<'_>) -> Result<Option<String>, ChannelError> {
    let query = crate::policy::request_query(crate::policy::CODEX, ctx)?;
    let mut kept = query
        .as_deref()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if matches!(
        ctx.key.operation,
        Operation::ListModels | Operation::GetModel
    ) && !kept
        .iter()
        .any(|pair| pair.split('=').next() == Some("client_version"))
    {
        kept.push(format!("client_version={}", super::auth::VERSION));
    }
    Ok((!kept.is_empty()).then(|| kept.join("&")))
}

fn endpoint(ctx: &PrepareCtx<'_>, path: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    if let Some(url) = endpoint_override(ctx) {
        return exact_url(&url, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(super::auth::DEFAULT_BASE_URL);
    absolute_url(&format!("{}{}", base.trim_end_matches('/'), path), query)
}

fn endpoint_override(ctx: &PrepareCtx<'_>) -> Option<String> {
    let name = match ctx.key.operation {
        Operation::ListModels => "openai_list_models",
        Operation::GetModel => "openai_get_model",
        Operation::GenerateContent | Operation::StreamGenerateContent => "openai_responses",
        Operation::CompactContent => "openai_compact",
        Operation::CreateImage => "image_generations",
        Operation::EditImage => "image_edits",
        Operation::WebSearch => "openai_search",
        Operation::CreateRealtimeCall => "openai_realtime_call",
        Operation::ConnectRealtime => return None,
        Operation::SummarizeMemory => return None,
        _ => return None,
    };
    ctx.provider_settings
        .get("endpoints")?
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| url.replace("{model}", &encode_component(ctx.upstream_model)))
}

fn is_sdp(value: &HeaderValue) -> bool {
    value.to_str().ok().is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/sdp"))
    })
}

fn surface_query(query: Option<&str>, remote_socket: bool) -> Option<String> {
    if remote_socket {
        return query.map(str::to_owned);
    }
    let kept = query?
        .split('&')
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or_default();
            !pair.is_empty() && !matches!(name, "key" | "api_key" | "x-api-key")
        })
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

fn absolute_url(url: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    let uri = url
        .parse::<Uri>()
        .map_err(|error| ChannelError::Prepare(format!("bad upstream URL: {error}")))?;
    exact_uri(uri, query)
}

fn exact_url(url: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    let uri = url
        .parse::<Uri>()
        .map_err(|error| ChannelError::Prepare(format!("bad endpoint override: {error}")))?;
    exact_uri(uri, query)
}

fn exact_uri(uri: Uri, query: Option<&str>) -> Result<Uri, ChannelError> {
    if uri.scheme().is_none() || uri.authority().is_none() {
        return Err(ChannelError::Prepare(
            "upstream URL must be absolute".into(),
        ));
    }
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Ok(uri);
    };
    let merged = match uri.query() {
        Some(existing) => format!("{}?{existing}&{query}", uri.path()),
        None => format!("{}?{query}", uri.path()),
    };
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(
        merged
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("bad endpoint query: {error}")))?,
    );
    Uri::from_parts(parts).map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn strip_userinfo(uri: Uri) -> Result<Uri, ChannelError> {
    let Some(authority) = uri.authority() else {
        return Ok(uri);
    };
    if !authority.as_str().contains('@') {
        return Ok(uri);
    }
    let clean = authority.port_u16().map_or_else(
        || authority.host().to_owned(),
        |port| format!("{}:{port}", authority.host()),
    );
    let mut parts = uri.into_parts();
    parts.authority = Some(
        clean
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("bad authority: {error}")))?,
    );
    Uri::from_parts(parts).map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

fn websocket_uri(uri: Uri) -> Result<Uri, ChannelError> {
    let mut value = uri.to_string();
    if let Some(rest) = value.strip_prefix("https://") {
        value = format!("wss://{rest}");
    } else if let Some(rest) = value.strip_prefix("http://") {
        value = format!("ws://{rest}");
    }
    value
        .parse()
        .map_err(|error| ChannelError::Prepare(format!("bad websocket URI: {error}")))
}
