//! Realtime media-call setup and transparent control WebSockets.
//! Codex's backend accepts {sdp, session}; the public API accepts multipart.
use super::common::invalid_config;
use super::headers::{account, backend_headers};
use super::{CLI_HEADERS, Codex, CodexConfig};
use crate::channel::{
    BaseChannel, ChannelError, HeaderAllowlist, OperationContext, PrepareContext,
};
use futures_util::{FutureExt, StreamExt};
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    capability::{CapabilityError, CapabilityErrorKind, CapabilityErrorStage},
    codec::{CodecError, CodecErrorKind, CodecLimits, MultipartDecoder},
    connection::Bytes,
    wire::openai::realtime::RealtimeRoute,
};
use http::{HeaderMap, HeaderValue, header};
use serde_json::{Value, json};
use url::Url;

fn error(error: impl std::fmt::Display) -> ChannelError {
    invalid_config(format!("Codex realtime: {error}"))
}

fn request_error(cause: impl std::fmt::Display) -> ChannelError {
    ChannelError::Transport(CapabilityError::new(
        CapabilityErrorKind::Invalid,
        CapabilityErrorStage::BodyTransfer,
        format!("invalid realtime request: {cause}"),
    ))
}

fn boundary(headers: &HeaderMap) -> Result<Option<String>, ChannelError> {
    let Some(value) = headers.get(header::CONTENT_TYPE) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(request_error)?;
    let mut parts = value.split(';');
    if !parts
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
    {
        return Ok(None);
    }
    parts
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            key.eq_ignore_ascii_case("boundary")
                .then(|| value.trim().trim_matches('"').to_owned())
        })
        .filter(|v| !v.is_empty())
        .map(Some)
        .ok_or_else(|| request_error("multipart boundary missing"))
}

fn backend(
    provider: crate::channel::ProviderView<'_>,
    endpoint: Option<&str>,
) -> Result<bool, ChannelError> {
    let url = Url::parse(
        endpoint.unwrap_or_else(|| provider.base_url.unwrap_or(super::DEFAULT_BASE_URL)),
    )
    .map_err(error)?;
    let path = url.path();
    Ok(path == "/backend-api" || path.starts_with("/backend-api/"))
}

fn codec_error(cause: CodecError) -> ChannelError {
    let kind = match cause.kind() {
        CodecErrorKind::Transport => CapabilityErrorKind::Transport,
        CodecErrorKind::Limit => CapabilityErrorKind::Limit,
        _ => CapabilityErrorKind::Invalid,
    };
    ChannelError::Transport(CapabilityError::with_source(
        kind,
        CapabilityErrorStage::BodyTransfer,
        "realtime multipart decoding failed",
        cause,
    ))
}
fn body_error(cause: gproxy_protocol::connection::TransportError) -> ChannelError {
    match cause.downcast::<CodecError>() {
        Ok(cause) => codec_error(*cause),
        Err(cause) => ChannelError::Transport(CapabilityError::with_source(
            CapabilityErrorKind::Transport,
            CapabilityErrorStage::BodyTransfer,
            "realtime body read failed",
            cause,
        )),
    }
}

pub(super) fn prepare_buffered(ctx: &mut PrepareContext<'_>) -> Result<(), ChannelError> {
    if !backend(ctx.provider, ctx.endpoint_override)? {
        return Ok(());
    }
    if let HttpBody::Bytes(bytes) = &ctx.request.body
        && let Some(boundary) = boundary(&ctx.request.headers)?
    {
        let output = multipart(HttpBody::Bytes(bytes.clone()), boundary)
            .now_or_never()
            .ok_or_else(|| error("buffered multipart unexpectedly pending"))??;
        ctx.request.body = HttpBody::Bytes(output);
        json_headers(&mut ctx.request.headers);
    }
    Ok(())
}

fn json_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.remove(header::CONTENT_LENGTH);
}

pub(super) async fn create_call(
    channel: &Codex,
    mut ctx: OperationContext<'_>,
) -> Result<WireResponse, ChannelError> {
    if backend(ctx.provider, ctx.endpoint_override)?
        && let Some(boundary) = boundary(&ctx.request.headers)?
    {
        ctx.request.body = HttpBody::Bytes(multipart(ctx.request.body, boundary).await?);
        json_headers(&mut ctx.request.headers);
    }
    let prepared = channel.prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation: Operation::CreateRealtimeCall,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    // SDP answer and Location (call id) are owned by the caller and pass through.
    Ok(ctx.client.send(prepared).await?)
}

async fn multipart(body: HttpBody, boundary: String) -> Result<Bytes, ChannelError> {
    let mut decoder = MultipartDecoder::new(
        body,
        boundary,
        CodecLimits {
            max_buffer_bytes: 64 * 1024,
            max_line_bytes: 64 * 1024,
            max_value_bytes: 4 * 1024 * 1024,
            max_body_bytes: 4 * 1024 * 1024,
            max_part_bytes: 4 * 1024 * 1024,
            max_parts: 32,
        },
    )
    .map_err(codec_error)?;
    let mut output = serde_json::Map::new();
    while let Some(part) = decoder.next_part().await.map_err(codec_error)? {
        let name = part
            .headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.split(';').skip(1).find_map(|p| {
                    let (key, value) = p.trim().split_once('=')?;
                    key.eq_ignore_ascii_case("name")
                        .then(|| value.trim().trim_matches('"').to_owned())
                })
            })
            .ok_or_else(|| request_error("multipart field name missing"))?;
        let bytes = match part.body {
            HttpBody::Bytes(bytes) => bytes.to_vec(),
            HttpBody::Stream(mut stream) => {
                let mut bytes = Vec::new();
                while let Some(chunk) = stream.next().await {
                    bytes.extend_from_slice(&chunk.map_err(body_error)?);
                }
                bytes
            }
        };
        let value = if name == "session" {
            serde_json::from_slice::<Value>(&bytes).map_err(request_error)?
        } else {
            json!(String::from_utf8(bytes).map_err(request_error)?)
        };
        output.insert(name, value);
    }
    if !output.get("sdp").is_some_and(Value::is_string) {
        return Err(request_error("multipart sdp missing"));
    }
    if output.get("session").is_some_and(|s| !s.is_object()) {
        return Err(request_error("session must be an object"));
    }
    serde_json::to_vec(&output).map(Bytes::from).map_err(error)
}

pub(super) fn prepare_connect(
    ctx: PrepareContext<'_, ()>,
) -> Result<http::Request<()>, ChannelError> {
    let config = CodexConfig::from_view(ctx.provider)?;
    // Realtime uses the API host even with a ChatGPT credential. Existing-call
    // sidebands use that same credential; the host must bind call continuations.
    let route = RealtimeRoute::from_path(&ctx.request.path);
    let live = match route {
        Some(RealtimeRoute::Live { call_id }) => Some(call_id),
        _ => None,
    };
    let mut url = Url::parse(
        ctx.endpoint_override
            .unwrap_or("https://api.openai.com/v1/realtime"),
    )
    .map_err(error)?;
    if let Some(call_id) = live {
        if ctx.endpoint_override.is_none() {
            url.set_path("/v1/live");
        }
        if let Some(id) = call_id {
            let id_path = format!("{}/{id}", url.path().trim_end_matches('/'));
            // The host supplied an encoded path segment, not a URL.
            let prefix = format!("{}/", url.path().trim_end_matches('/'));
            url.set_path(&id_path);
            if !url.path().starts_with(&prefix) {
                return Err(error("invalid live call path"));
            }
        }
    }
    let configured_call = url
        .query_pairs()
        .find(|(key, _)| key == "call_id")
        .map(|(_, v)| v.into_owned());
    let requested_call = ctx.request.query.as_deref().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "call_id")
            .map(|(_, v)| v.into_owned())
    });
    if configured_call
        .as_ref()
        .zip(requested_call.as_ref())
        .is_some_and(|(a, b)| a != b)
    {
        return Err(error("endpoint call_id conflicts with the request"));
    }
    if let Some(query) = &ctx.request.query {
        url.query_pairs_mut()
            .extend_pairs(url::form_urlencoded::parse(query.as_bytes()));
    }
    let scheme = match url.scheme() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        _ => return Err(error("unsupported WebSocket URL scheme")),
    };
    url.set_scheme(scheme)
        .map_err(|_| error("invalid WebSocket URL"))?;
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
    let mut headers = backend_headers(
        &config,
        &account(&ctx.credential)?,
        Some((&ctx.request.headers, allowlist.as_ref())),
    )?;
    // Preserve realtime-specific beta selection, never inject Responses beta.
    if let Some(beta) = ctx.request.headers.get("openai-beta") {
        headers.insert("openai-beta", beta.clone());
    }
    let mut request = http::Request::builder()
        .method(http::Method::GET)
        .uri(url.as_str())
        .body(())
        .map_err(error)?;
    *request.headers_mut() = headers;
    Ok(request)
}
