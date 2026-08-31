use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, StreamFraming};
use http::{HeaderValue, Uri};

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const API_KEY: http::header::HeaderName = http::header::HeaderName::from_static("x-goog-api-key");
pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let query = crate::policy::request_query(crate::policy::AISTUDIO, &ctx)?;
    let framing = framing(&ctx, query.as_deref());
    let uri = endpoint(&ctx, query.as_deref())?;
    let body = super::model::rewrite(ctx.key.operation, ctx.body, ctx.upstream_model)?;
    let mut request = http::Request::builder()
        .method(ctx.method)
        .uri(strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = crate::policy::request_headers(crate::policy::AISTUDIO, &ctx)?;
    request.headers_mut().insert(
        API_KEY,
        HeaderValue::from_str(api_key)
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    Ok(PreparedRequest {
        request,
        framing,
        websocket: false,
        profile: None,
    })
}

fn framing(ctx: &PrepareCtx<'_>, query: Option<&str>) -> Option<StreamFraming> {
    (ctx.key.operation == Operation::StreamGenerateContent).then(|| {
        if query.is_some_and(has_sse_alt) {
            StreamFraming::Sse
        } else {
            StreamFraming::JsonArray
        }
    })
}

fn has_sse_alt(query: &str) -> bool {
    query.split('&').any(|pair| pair == "alt=sse")
}

fn endpoint(ctx: &PrepareCtx<'_>, query: Option<&str>) -> Result<Uri, ChannelError> {
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    let uri = format!("{}{}", base.trim_end_matches('/'), upstream_path(ctx))
        .parse::<Uri>()
        .map_err(|error| ChannelError::Prepare(format!("bad upstream URL: {error}")))?;
    with_query(uri, query)
}

fn upstream_path(ctx: &PrepareCtx<'_>) -> String {
    if matches!(
        ctx.key.operation,
        Operation::CreateVideo | Operation::RetrieveVideo
    ) && let Some(suffix) = ctx.path.strip_prefix("/v1/videos")
    {
        return format!("/v1beta/openai/videos{suffix}");
    }
    if ctx.upstream_model.is_empty() || !has_model_path(ctx.key.operation) {
        return ctx.path.to_owned();
    }
    let Some(rest) = ctx.path.strip_prefix("/v1beta/models/") else {
        return ctx.path.to_owned();
    };
    let end = rest.find([':', '/']).unwrap_or(rest.len());
    format!(
        "/v1beta/models/{}{}",
        encode_component(ctx.upstream_model),
        &rest[end..]
    )
}

fn has_model_path(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GetModel
            | Operation::CountTokens
            | Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CreateEmbedding
            | Operation::BatchCreateEmbedding
            | Operation::CreateImage
            | Operation::CreateVideo
            | Operation::RetrieveVideo
    )
}

fn with_query(uri: Uri, query: Option<&str>) -> Result<Uri, ChannelError> {
    if uri.scheme().is_none() || uri.authority().is_none() {
        return Err(ChannelError::Prepare(
            "upstream URL must be absolute".into(),
        ));
    }
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Ok(uri);
    };
    let path_and_query = match uri.query() {
        Some(existing) => format!("{}?{existing}&{query}", uri.path()),
        None => format!("{}?{query}", uri.path()),
    };
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(
        path_and_query
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("bad endpoint query: {error}")))?,
    );
    Uri::from_parts(parts).map_err(|error| ChannelError::Prepare(format!("bad endpoint: {error}")))
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
