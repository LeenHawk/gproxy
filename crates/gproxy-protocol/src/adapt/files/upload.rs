use crate::{
    HttpBody, WireRequest, WireResponse,
    adapt::JsonInvocation,
    capability::Upstream,
    codec::{self, CodecErrorKind, CodecLimits, MultipartEncoder},
    connection::MultipartPart,
    transform::{TransformError, TransformErrorKind},
    wire::{DeclaredFields, gemini::files as g},
};
use serde::de::DeserializeOwned;

#[derive(Debug)]
pub enum UploadFailure {
    Transform(TransformError),
    Rejected(Box<WireResponse<HttpBody>>),
}

/// A request may have reached upstream even if its transport future failed.
/// `attempted_calls` is not permission to retry a side effect.
#[derive(Debug)]
pub struct UploadError {
    pub attempted_calls: usize,
    pub failure: UploadFailure,
}

impl From<TransformError> for UploadError {
    fn from(error: TransformError) -> Self {
        Self {
            attempted_calls: 0,
            failure: UploadFailure::Transform(error),
        }
    }
}

fn after(error: TransformError) -> UploadError {
    UploadError {
        attempted_calls: 1,
        failure: UploadFailure::Transform(error),
    }
}

fn codec_error(error: codec::CodecError, response: bool) -> TransformError {
    let kind = match error.kind() {
        CodecErrorKind::Limit => TransformErrorKind::Limit,
        CodecErrorKind::Transport => TransformErrorKind::Host,
        CodecErrorKind::Invalid
        | CodecErrorKind::UnexpectedEof
        | CodecErrorKind::Utf8
        | CodecErrorKind::Json
        | CodecErrorKind::Multipart => {
            if response {
                TransformErrorKind::InvalidResult
            } else {
                TransformErrorKind::InvalidInput
            }
        }
    };
    TransformError::with_source(kind, "file.upload", error.to_string(), error)
}

fn bounded(mut limits: CodecLimits, bytes: u64) -> CodecLimits {
    limits.max_body_bytes = limits.max_body_bytes.min(bytes);
    limits.max_value_bytes = limits.max_value_bytes.min(bytes);
    limits.max_buffer_bytes = limits.max_buffer_bytes.min(bytes);
    limits
}

fn clean(headers: &mut http::HeaderMap) {
    for key in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(key);
    }
}

async fn decode<O: DeserializeOwned + DeclaredFields>(
    response: WireResponse<HttpBody>,
    limits: CodecLimits,
) -> Result<JsonInvocation<O>, TransformError> {
    if !response.status.is_success() {
        return Ok(JsonInvocation::Rejected(response));
    }
    let WireResponse {
        status,
        mut headers,
        body,
    } = response;
    let bytes = codec::read_http_body(body, limits)
        .await
        .map_err(|e| codec_error(e, true))?;
    let body: O = codec::decode_json(&bytes, limits).map_err(|e| codec_error(e, true))?;
    clean(&mut headers);
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    Ok(JsonInvocation::Success(WireResponse {
        status,
        headers,
        body: body.into_declared(),
    }))
}

pub async fn upload_multipart_json<U: Upstream, O: DeserializeOwned + DeclaredFields>(
    upstream: &U,
    target: &U::Target,
    mut template: WireRequest<()>,
    boundary: String,
    parts: Vec<MultipartPart>,
    limits: CodecLimits,
) -> Result<JsonInvocation<O>, UploadError> {
    super::path(&template.path)?;
    if template.method != http::Method::POST {
        return Err(TransformError::shape("upload.method", "POST required").into());
    }
    let host = upstream.limits();
    let encoder = MultipartEncoder::new(boundary.clone(), parts, bounded(limits, host.write_bytes))
        .map_err(|e| codec_error(e, false))?;
    let body = futures_util::stream::try_unfold(encoder, |mut encoder| async move {
        encoder
            .next_chunk()
            .await
            .map(|chunk| chunk.map(|chunk| (chunk, encoder)))
            .map_err(crate::connection::TransportError::from)
    });
    clean(&mut template.headers);
    template.headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
            .map_err(|e| TransformError::shape("content_type", e.to_string()))?,
    );
    let response = upstream
        .send(
            target,
            WireRequest {
                method: template.method,
                path: template.path,
                query: template.query,
                headers: template.headers,
                body: HttpBody::Stream(Box::pin(body)),
            },
        )
        .await
        .map_err(|e| after(e.into()))?;
    decode(response, bounded(limits, host.read_bytes))
        .await
        .map_err(after)
}

#[derive(Debug, Clone)]
pub struct GeminiUploadSession {
    upload_path: String,
    query: Option<String>,
    origin: String,
    offset: u64,
    size: u64,
    closed: bool,
}

impl GeminiUploadSession {
    pub fn offset(&self) -> u64 {
        self.offset
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

fn origin(value: &str) -> Result<String, TransformError> {
    let uri: http::Uri = value.parse().map_err(|e: http::uri::InvalidUri| {
        TransformError::shape("upload.origin", e.to_string())
    })?;
    if uri.scheme_str() != Some("https")
        || uri.authority().is_none()
        || uri.authority().unwrap().as_str().contains('@')
        || !matches!(uri.path(), "" | "/")
        || uri.query().is_some()
    {
        return Err(TransformError::shape(
            "upload.origin",
            "HTTPS origin without userinfo/path/query required",
        ));
    }
    Ok(format!(
        "https://{}",
        uri.authority().unwrap().as_str().to_ascii_lowercase()
    ))
}

#[allow(clippy::too_many_arguments)] // Explicit host target, metadata and origin facts.
pub async fn gemini_start_resumable<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    mut template: WireRequest<()>,
    metadata: g::UploadFileMetadata,
    size: u64,
    mime: &str,
    expected_origin: &str,
    limits: CodecLimits,
) -> Result<GeminiUploadSession, UploadError> {
    let expected_origin = origin(expected_origin)?;
    super::path(&template.path)?;
    if template.method != http::Method::POST || mime.is_empty() {
        return Err(TransformError::shape("upload.start", "POST and MIME required").into());
    }
    let body = codec::encode_json(
        &metadata.into_declared(),
        bounded(limits, upstream.limits().write_bytes),
    )
    .map_err(|e| codec_error(e, false))?;
    clean(&mut template.headers);
    for (name, value) in [
        ("x-goog-upload-protocol", "resumable".to_owned()),
        ("x-goog-upload-command", "start".to_owned()),
        ("x-goog-upload-header-content-length", size.to_string()),
        ("x-goog-upload-header-content-type", mime.to_owned()),
        ("content-type", "application/json".to_owned()),
    ] {
        template.headers.insert(
            http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            http::HeaderValue::from_str(&value)
                .map_err(|e| TransformError::shape("upload.headers", e.to_string()))?,
        );
    }
    let response = upstream
        .send(
            target,
            WireRequest {
                method: template.method,
                path: template.path,
                query: template.query,
                headers: template.headers,
                body: HttpBody::Bytes(body),
            },
        )
        .await
        .map_err(|e| after(e.into()))?;
    if !response.status.is_success() {
        return Err(UploadError {
            attempted_calls: 1,
            failure: UploadFailure::Rejected(Box::new(response)),
        });
    }
    let url = response
        .headers
        .get("x-goog-upload-url")
        .ok_or_else(|| after(TransformError::missing_metadata("x-goog-upload-url")))?
        .to_str()
        .map_err(|e| after(TransformError::invalid_result("upload_url", e.to_string())))?;
    let parsed: http::Uri = url.parse().map_err(|e: http::uri::InvalidUri| {
        after(TransformError::invalid_result("upload_url", e.to_string()))
    })?;
    let received = format!(
        "{}://{}",
        parsed.scheme_str().unwrap_or(""),
        parsed.authority().map(|a| a.as_str()).unwrap_or("")
    );
    if origin(&received).map_err(after)? != expected_origin {
        return Err(after(TransformError::invalid_result(
            "upload_url",
            "session origin differs from selected target",
        )));
    }
    super::path(parsed.path()).map_err(after)?;
    Ok(GeminiUploadSession {
        upload_path: parsed.path().into(),
        query: parsed.query().map(str::to_owned),
        origin: expected_origin,
        offset: 0,
        size,
        closed: false,
    })
}

#[derive(Debug)]
pub enum GeminiUploadProgress {
    Accepted(http::StatusCode),
    Finalized(Box<WireResponse<g::UploadFileResponseBody>>),
}

#[allow(clippy::too_many_arguments)] // Chunk data and target-origin validation remain explicit.
pub async fn gemini_upload_chunk<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    session: &mut GeminiUploadSession,
    headers: http::HeaderMap,
    expected_origin: &str,
    chunk: bytes::Bytes,
    finalize: bool,
    limits: CodecLimits,
) -> Result<GeminiUploadProgress, UploadError> {
    if session.closed {
        return Err(TransformError::shape(
            "upload.session",
            "session is closed or outcome uncertain",
        )
        .into());
    }
    if origin(expected_origin)? != session.origin {
        return Err(TransformError::shape(
            "upload.origin",
            "selected target origin differs from session",
        )
        .into());
    }
    let next = session
        .offset
        .checked_add(chunk.len() as u64)
        .ok_or_else(|| {
            TransformError::new(TransformErrorKind::Limit, "upload.offset", "overflow")
        })?;
    if next > session.size || finalize && next != session.size {
        return Err(TransformError::shape(
            "upload.size",
            "chunk/finalize does not match declared size",
        )
        .into());
    }
    if chunk.len() as u64 > upstream.limits().write_bytes.min(limits.max_body_bytes) {
        return Err(TransformError::new(
            TransformErrorKind::Limit,
            "upload.chunk",
            "chunk exceeds write limit",
        )
        .into());
    }
    let mut headers = headers;
    clean(&mut headers);
    headers.insert(
        "x-goog-upload-command",
        http::HeaderValue::from_static(if finalize {
            "upload, finalize"
        } else {
            "upload"
        }),
    );
    headers.insert(
        "x-goog-upload-offset",
        http::HeaderValue::from_str(&session.offset.to_string()).unwrap(),
    );
    headers.insert(
        http::header::CONTENT_LENGTH,
        http::HeaderValue::from_str(&chunk.len().to_string()).unwrap(),
    );
    // Mark uncertain before awaiting: cancellation or transport failure must not
    // silently permit replay of a possibly applied chunk.
    session.closed = true;
    let response = upstream
        .send(
            target,
            WireRequest {
                method: http::Method::POST,
                path: session.upload_path.clone(),
                query: session.query.clone(),
                headers,
                body: HttpBody::Bytes(chunk),
            },
        )
        .await
        .map_err(|e| after(e.into()))?;
    if !response.status.is_success() {
        return Err(UploadError {
            attempted_calls: 1,
            failure: UploadFailure::Rejected(Box::new(response)),
        });
    }
    if finalize {
        match decode(response, bounded(limits, upstream.limits().read_bytes))
            .await
            .map_err(after)?
        {
            JsonInvocation::Success(response) => {
                session.offset = next;
                Ok(GeminiUploadProgress::Finalized(Box::new(response)))
            }
            JsonInvocation::Rejected(_) => unreachable!("checked status"),
        }
    } else {
        let status = response.status;
        codec::read_http_body(response.body, bounded(limits, upstream.limits().read_bytes))
            .await
            .map_err(|e| after(codec_error(e, true)))?;
        session.offset = next;
        session.closed = false;
        Ok(GeminiUploadProgress::Accepted(status))
    }
}
